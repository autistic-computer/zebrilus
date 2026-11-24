use std::{sync::Arc, thread};

use ala::Context;
use semver::Version;
use zz_cli::update::{
    Daemon, HostProgress, HostUpdateError, host_daemon, host_install, host_restart,
};
use zz_config::owned::BuildInfo;
use zz_daemon_client::{DaemonFacts, Endpoint, HostSeen, SshPrompts, UpdateRoute, UpdateState};
use zz_protocol::{ClientMessageKind, app_identity::DEVELOPMENT};

use super::MuxClient;
use crate::mux::hosts::HostId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HostUpdate {
    Outdated {
        daemon: Option<Version>,
        failed: Option<String>,
    },
    RestartPending {
        installed: Version,
        failed: Option<String>,
    },
    Updating {
        progress: Option<String>,
    },
    CannotUpdate,
}

impl HostUpdate {
    pub(crate) const fn offered(&self) -> bool {
        matches!(self, Self::Outdated { .. } | Self::RestartPending { .. })
    }
}

#[derive(Default)]
pub(super) struct HostUpdateSlot {
    facts: Option<DaemonFacts>,
    asking: bool,
    running: bool,
    progress: Option<String>,
    failed: Option<String>,
    blocked: bool,
}

pub(crate) struct HostUpdateQuestion {
    pub(crate) hosts: Vec<(String, usize)>,
    pub(crate) installed: Option<Version>,
}

enum Signal {
    Progress(String),
    Done(Result<Daemon, HostUpdateError>),
}

impl MuxClient {
    #[must_use]
    pub(crate) fn host_update(&self, host: HostId) -> Option<HostUpdate> {
        if DEVELOPMENT {
            return None;
        }
        let entry = self.registry.get(host)?;
        let connection = self.connections.get(&host)?;
        let slot = self.host_updates.get(&entry.name);
        if let Some(slot) = slot
            && (slot.asking || slot.running)
        {
            return Some(HostUpdate::Updating {
                progress: slot.progress.clone(),
            });
        }
        let facts = slot.and_then(|slot| slot.facts.as_ref());
        let failure = connection.state.failure().filter(|_| host != HostId::LOCAL);
        let seen = match (&failure, facts) {
            (Some(failure), _) => HostSeen::Failure(failure),
            (None, Some(facts)) if connection.is_connected() => HostSeen::Facts(facts),
            _ => HostSeen::Nothing,
        };
        let route = if matches!(entry.endpoint, Endpoint::Tailnet(_)) {
            UpdateRoute::Daemon
        } else {
            UpdateRoute::Cli
        };
        let state = UpdateState::of(
            route,
            seen,
            &BuildInfo::current().version,
            zz_protocol::REVISION,
        );
        let failed = slot.and_then(|slot| slot.failed.clone());
        let blocked = slot.is_some_and(|slot| slot.blocked);
        match state {
            UpdateState::Unknown | UpdateState::Current => None,
            UpdateState::CannotUpdate => Some(HostUpdate::CannotUpdate),
            UpdateState::Outdated | UpdateState::RestartPending if blocked => {
                Some(HostUpdate::CannotUpdate)
            }
            UpdateState::Outdated => Some(HostUpdate::Outdated {
                daemon: facts.and_then(|facts| facts.version.clone()),
                failed,
            }),
            UpdateState::RestartPending => Some(HostUpdate::RestartPending {
                installed: facts.and_then(|facts| facts.owned.clone())?,
                failed,
            }),
        }
    }

    #[must_use]
    pub(crate) fn host_daemon_version(&self, host: HostId) -> Option<Version> {
        let entry = self.registry.get(host)?;
        if !self.connections.get(&host)?.is_connected() {
            return None;
        }
        self.host_updates
            .get(&entry.name)?
            .facts
            .as_ref()?
            .version
            .clone()
    }

    #[must_use]
    pub(crate) fn hosts_to_update(&self) -> Vec<HostId> {
        self.registry
            .iter()
            .map(|(host, _)| host)
            .filter(|host| *host != HostId::LOCAL)
            .filter(|host| {
                self.host_update(*host)
                    .is_some_and(|update| update.offered())
            })
            .collect()
    }

    pub(super) fn fetch_host_facts(&mut self, host: HostId, cx: &mut Context<Self>) {
        if DEVELOPMENT || cfg!(test) {
            return;
        }
        let Some(entry) = self.registry.get(host) else {
            return;
        };
        let name = entry.name.clone();
        let endpoint = entry.endpoint.clone();
        self.host_updates.entry(name.clone()).or_default().facts = None;
        let (answers, incoming) = async_channel::bounded(1);
        let asked = thread::Builder::new()
            .name("zz-host-facts".to_owned())
            .spawn(move || {
                let _ = answers.send_blocking(host_daemon(&endpoint));
            });
        if let Err(error) = asked {
            log::warn!("could not ask {name} for its zz version: {error}");
            return;
        }
        cx.spawn(async move |this, cx| {
            let Ok(daemon) = incoming.recv().await else {
                return;
            };
            let _ = this.update(cx, |client, cx| {
                client.note_host_facts(&name, &daemon);
                cx.notify();
            });
        })
        .detach();
    }

    fn note_host_facts(&mut self, name: &str, daemon: &Daemon) {
        self.host_updates.entry(name.to_owned()).or_default().facts = match daemon {
            Daemon::Running {
                version,
                revision,
                owned,
                ..
            } => Some(DaemonFacts {
                version: version.clone(),
                revision: *revision,
                owned: owned.clone(),
            }),
            Daemon::Missing | Daemon::OtherEpoch(_) | Daemon::Unreachable(_) => None,
        };
    }

    pub(crate) fn request_host_updates(&mut self, hosts: &[HostId], cx: &mut Context<Self>) {
        let targets = hosts
            .iter()
            .filter(|host| **host != HostId::LOCAL)
            .filter(|host| {
                self.host_update(**host)
                    .is_some_and(|update| update.offered())
            })
            .filter_map(|host| self.registry.get(*host))
            .map(|entry| (entry.name.clone(), entry.endpoint.clone()))
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return;
        }
        for (name, _) in &targets {
            let slot = self.host_updates.entry(name.clone()).or_default();
            slot.asking = true;
            slot.progress = None;
            slot.failed = None;
        }
        cx.notify();
        let (answers, incoming) = async_channel::bounded(1);
        let asked = thread::Builder::new()
            .name("zz-host-facts".to_owned())
            .spawn(move || {
                let daemons = thread::scope(|scope| {
                    targets
                        .iter()
                        .map(|(_, endpoint)| scope.spawn(|| host_daemon(endpoint)))
                        .collect::<Vec<_>>()
                        .into_iter()
                        .map(|job| {
                            job.join().unwrap_or_else(|_| {
                                Daemon::Unreachable("the question did not finish".to_owned())
                            })
                        })
                        .collect::<Vec<_>>()
                });
                let names = targets.into_iter().map(|(name, _)| name);
                let _ = answers.send_blocking(names.zip(daemons).collect::<Vec<_>>());
            });
        if let Err(error) = asked {
            log::warn!("could not start the update: {error}");
            for slot in self.host_updates.values_mut() {
                slot.asking = false;
            }
            return;
        }
        cx.spawn(async move |this, cx| {
            let Ok(answers) = incoming.recv().await else {
                return;
            };
            let _ = this.update(cx, |client, cx| client.ask_or_run_host_updates(answers, cx));
        })
        .detach();
    }

    fn ask_or_run_host_updates(&mut self, answers: Vec<(String, Daemon)>, cx: &mut Context<Self>) {
        let mut ready = Vec::new();
        for (name, daemon) in answers {
            self.host_updates.entry(name.clone()).or_default().asking = false;
            if let Daemon::Unreachable(reason) = &daemon {
                self.fail_host_update(&name, reason.clone(), cx);
                continue;
            }
            self.note_host_facts(&name, &daemon);
            let sessions = match daemon {
                Daemon::Running { sessions, .. } => sessions,
                _ => 0,
            };
            let update = self
                .registry
                .get_by_name(&name)
                .and_then(|(host, _)| self.host_update(host));
            let installed = match update {
                Some(HostUpdate::Outdated { .. }) => None,
                Some(HostUpdate::RestartPending { installed, .. }) => Some(installed),
                _ => continue,
            };
            ready.push((name, sessions, installed));
        }
        cx.notify();
        let names = ready
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect::<Vec<_>>();
        if ready.iter().all(|(_, sessions, _)| *sessions == 0) {
            self.run_host_updates(&names, true, cx);
            return;
        }
        for name in &names {
            self.host_updates.entry(name.clone()).or_default().asking = true;
        }
        let installed = ready
            .iter()
            .map(|(_, _, installed)| installed.clone())
            .collect::<Option<Vec<_>>>()
            .and_then(|versions| versions.into_iter().max());
        cx.emit(HostUpdateQuestion {
            hosts: ready
                .into_iter()
                .map(|(name, sessions, _)| (name, sessions))
                .collect(),
            installed,
        });
    }

    pub(crate) fn dismiss_host_updates(&mut self, names: &[String], cx: &mut Context<Self>) {
        for name in names {
            if let Some(slot) = self.host_updates.get_mut(name) {
                slot.asking = false;
            }
        }
        cx.notify();
    }

    pub(crate) fn run_host_updates(
        &mut self,
        names: &[String],
        restart: bool,
        cx: &mut Context<Self>,
    ) {
        let version = BuildInfo::current().version;
        for name in names {
            self.host_updates.entry(name.clone()).or_default().asking = false;
            let Some((host, entry)) = self.registry.get_by_name(name) else {
                continue;
            };
            let install = match self.host_update(host) {
                Some(HostUpdate::Outdated { .. }) => true,
                Some(HostUpdate::RestartPending { .. }) if restart => false,
                _ => continue,
            };
            let endpoint = entry.endpoint.clone();
            let prompts = self.ssh_prompts(host, &endpoint, cx);
            let slot = self.host_updates.entry(name.clone()).or_default();
            slot.running = true;
            slot.progress = None;
            slot.failed = None;
            let (signals, incoming) = async_channel::unbounded();
            let lines = signals.clone();
            let progress: HostProgress = Arc::new(move |text| {
                if let Some(line) = text
                    .as_str()
                    .lines()
                    .map(str::trim)
                    .rfind(|line| !line.is_empty())
                {
                    let _ = lines.send_blocking(Signal::Progress(line.to_owned()));
                }
            });
            let job_name = name.clone();
            let job_version = version.clone();
            let job = thread::Builder::new()
                .name("zz-host-update".to_owned())
                .spawn(move || {
                    let result = update_host(
                        &endpoint,
                        &job_name,
                        &job_version,
                        install,
                        restart,
                        progress,
                        prompts,
                    )
                    .map(|()| host_daemon(&endpoint));
                    let _ = signals.send_blocking(Signal::Done(result));
                });
            if let Err(error) = job {
                self.fail_host_update(name, format!("could not start the update: {error}"), cx);
                continue;
            }
            let name = name.clone();
            cx.spawn(async move |this, cx| {
                while let Ok(signal) = incoming.recv().await {
                    let done = matches!(signal, Signal::Done(_));
                    let _ = this.update(cx, |client, cx| match signal {
                        Signal::Progress(line) => {
                            client
                                .host_updates
                                .entry(name.clone())
                                .or_default()
                                .progress = Some(line);
                            cx.notify();
                        }
                        Signal::Done(result) => client.finish_host_update(&name, result, cx),
                    });
                    if done {
                        break;
                    }
                }
            })
            .detach();
        }
        cx.notify();
    }

    fn finish_host_update(
        &mut self,
        name: &str,
        result: Result<Daemon, HostUpdateError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(daemon) => {
                self.note_host_facts(name, &daemon);
                let slot = self.host_updates.entry(name.to_owned()).or_default();
                slot.running = false;
                slot.progress = None;
                let attached = self.registry.get_by_name(name).map(|(host, _)| {
                    (
                        host,
                        self.connections
                            .get(&host)
                            .is_some_and(super::HostConnection::is_connected),
                    )
                });
                if let Some((host, false)) = attached {
                    self.retry_host_now(host, cx);
                }
                cx.notify();
            }
            Err(HostUpdateError::NoVerb(reason) | HostUpdateError::NoRestart(reason)) => {
                self.host_updates
                    .entry(name.to_owned())
                    .or_default()
                    .blocked = true;
                self.fail_host_update(name, reason, cx);
            }
            Err(HostUpdateError::Failed(reason)) => self.fail_host_update(name, reason, cx),
        }
    }

    fn fail_host_update(&mut self, name: &str, reason: String, cx: &mut Context<Self>) {
        let slot = self.host_updates.entry(name.to_owned()).or_default();
        slot.asking = false;
        slot.running = false;
        slot.progress = None;
        Self::emit_notification(ClientMessageKind::Error, format!("{name}: {reason}"), cx);
        slot.failed = Some(reason);
        cx.notify();
    }
}

fn update_host(
    endpoint: &Endpoint,
    name: &str,
    version: &Version,
    install: bool,
    restart: bool,
    progress: HostProgress,
    prompts: Option<SshPrompts>,
) -> Result<(), HostUpdateError> {
    let Endpoint::Ssh(ssh) = endpoint else {
        if install {
            host_install(endpoint, name, version, Some(progress))?;
        }
        if restart {
            host_restart(endpoint, name, version)?;
        }
        return Ok(());
    };
    let answer = if restart {
        host_restart(endpoint, name, version)
    } else {
        host_install(endpoint, name, version, None)
    };
    match answer {
        Ok(_) => Ok(()),
        Err(HostUpdateError::NoVerb(_)) => {
            let outcome =
                zz_daemon_client::install_remote(ssh, prompts, restart).map_err(|error| {
                    HostUpdateError::Failed(error.ssh_reason().unwrap_or_else(|| error.to_string()))
                })?;
            match outcome.notice(name) {
                (ClientMessageKind::Error, text) => Err(HostUpdateError::Failed(text)),
                _ => Ok(()),
            }
        }
        Err(error) => Err(error),
    }
}
