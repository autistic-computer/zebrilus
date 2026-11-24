use std::io;

use semver::Version;
use zz_protocol::{PROTOCOL_VERSION, ProtocolError, ServerError};

use crate::{DaemonError, EndpointError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostFailure {
    Unreachable { reason: String },
    Incompatible { local: u16, remote: Option<u16> },
    DaemonTooOld { daemon: u32, minimum: u32 },
    ClientTooOld { client: u32, minimum: u32 },
    Missing,
}

impl HostFailure {
    #[must_use]
    pub fn classify(error: &DaemonError) -> Self {
        if let DaemonError::DaemonTooOld { daemon, minimum } = *error {
            return Self::DaemonTooOld { daemon, minimum };
        }
        if let DaemonError::IncompatibleDaemon { daemon, client } = error {
            return daemon.map_or_else(
                || Self::Unreachable {
                    reason: error.to_string(),
                },
                |daemon| Self::Incompatible {
                    local: *client,
                    remote: Some(daemon),
                },
            );
        }
        let mut ssh_reason = None;
        let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
        while let Some(current) = source {
            if ssh_reason.is_none()
                && let Some(endpoint_error) = current.downcast_ref::<EndpointError>()
            {
                if let EndpointError::RemoteProtocolMismatch { daemon, client, .. } = endpoint_error
                {
                    return Self::Incompatible {
                        local: *client,
                        remote: Some(*daemon),
                    };
                }
                if matches!(endpoint_error, EndpointError::RemoteProtocolUnknown { .. }) {
                    return Self::Incompatible {
                        local: PROTOCOL_VERSION,
                        remote: None,
                    };
                }
                if matches!(endpoint_error, EndpointError::RemoteBinaryMissing { .. }) {
                    return Self::Missing;
                }
                ssh_reason = endpoint_error.ssh_reason();
            }
            if let Some(ProtocolError::VersionMismatch { received, .. }) =
                current.downcast_ref::<ProtocolError>()
            {
                return Self::Incompatible {
                    local: PROTOCOL_VERSION,
                    remote: Some(*received),
                };
            }
            match current.downcast_ref::<ServerError>() {
                Some(ServerError::ProtocolMismatch { server, .. }) => {
                    return Self::Incompatible {
                        local: PROTOCOL_VERSION,
                        remote: Some(*server),
                    };
                }
                Some(ServerError::ClientTooOld { client, minimum }) => {
                    return Self::ClientTooOld {
                        client: *client,
                        minimum: *minimum,
                    };
                }
                _ => {}
            }
            if let Some(DaemonError::DaemonTooOld { daemon, minimum }) =
                current.downcast_ref::<DaemonError>()
            {
                return Self::DaemonTooOld {
                    daemon: *daemon,
                    minimum: *minimum,
                };
            }
            if let Some(error) = current.downcast_ref::<io::Error>()
                && let Some(inner) = error.get_ref()
            {
                source = Some(inner);
                continue;
            }
            source = current.source();
        }
        Self::Unreachable {
            reason: ssh_reason.unwrap_or_else(|| error.to_string()),
        }
    }

    #[must_use]
    pub const fn client_too_old(&self) -> bool {
        match self {
            Self::ClientTooOld { .. } => true,
            Self::Incompatible {
                local,
                remote: Some(remote),
            } => *remote > *local,
            _ => false,
        }
    }

    #[must_use]
    pub const fn install_offer(&self) -> Option<InstallOffer> {
        match self {
            Self::Missing => Some(InstallOffer::Install),
            Self::Incompatible { remote: None, .. } | Self::DaemonTooOld { .. } => {
                Some(InstallOffer::Update)
            }
            Self::Incompatible {
                local,
                remote: Some(remote),
            } if *local >= *remote => Some(InstallOffer::Update),
            _ => None,
        }
    }

    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::Unreachable { reason } => reason.clone(),
            Self::Incompatible { remote: None, .. } => {
                "That machine runs a zz too old to report its protocol.".to_owned()
            }
            Self::Incompatible {
                local,
                remote: Some(remote),
            } if local == remote => {
                format!("That machine runs an older build of protocol v{remote}.")
            }
            Self::Incompatible {
                local,
                remote: Some(remote),
            } => format!("This zz speaks protocol v{local}; that machine speaks v{remote}."),
            Self::DaemonTooOld { daemon, minimum } => format!(
                "That machine's zz daemon is revision {daemon}; this zz needs revision {minimum} \
                 or newer."
            ),
            Self::ClientTooOld { .. } => "That machine runs a newer zz.".to_owned(),
            Self::Missing => "zz is not installed on that machine.".to_owned(),
        }
    }

    #[must_use]
    pub fn step(&self) -> Option<&'static str> {
        match self {
            Self::Incompatible { remote: None, .. } => Some("Update zz there, then reconnect."),
            Self::Incompatible {
                local,
                remote: Some(remote),
            } if local == remote => {
                Some("Upgrade zz there, or restart its daemon, then reconnect.")
            }
            Self::Incompatible { .. } => Some("Upgrade whichever side is older, then reconnect."),
            Self::Unreachable { .. }
            | Self::DaemonTooOld { .. }
            | Self::ClientTooOld { .. }
            | Self::Missing => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallOffer {
    Install,
    Update,
}

impl InstallOffer {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Install => "Install zz",
            Self::Update => "Update zz",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonBehind {
    pub daemon: u32,
    pub client: u32,
}

impl DaemonBehind {
    #[must_use]
    pub const fn compare(daemon: u32, client: u32) -> Option<Self> {
        if daemon < client {
            Some(Self { daemon, client })
        } else {
            None
        }
    }

    #[must_use]
    pub fn restart_text(self, sessions: Option<usize>) -> String {
        format!(
            "The zz daemon runs an older build, revision {}; this zz is revision {}. Restarting it \
             to update ends {}.",
            self.daemon,
            self.client,
            session_count(sessions)
        )
    }
}

#[must_use]
pub fn session_count(sessions: Option<usize>) -> String {
    match sessions {
        None => "every running session".to_owned(),
        Some(0) => "no sessions".to_owned(),
        Some(1) => "1 session".to_owned(),
        Some(sessions) => format!("{sessions} sessions"),
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DaemonFacts {
    pub version: Option<Version>,
    pub revision: Option<u32>,
    pub owned: Option<Version>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateRoute {
    Cli,
    Daemon,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostSeen<'a> {
    Nothing,
    Facts(&'a DaemonFacts),
    Failure(&'a HostFailure),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateState {
    Unknown,
    Current,
    Outdated,
    RestartPending,
    CannotUpdate,
}

impl UpdateState {
    #[must_use]
    pub fn of(route: UpdateRoute, seen: HostSeen<'_>, version: &Version, revision: u32) -> Self {
        match seen {
            HostSeen::Nothing => Self::Unknown,
            HostSeen::Failure(HostFailure::DaemonTooOld { .. }) => Self::Outdated,
            HostSeen::Failure(HostFailure::Incompatible { local, remote }) => match remote {
                Some(remote) if remote > local => Self::Unknown,
                Some(remote) if remote == local => Self::Outdated,
                _ if route == UpdateRoute::Daemon => Self::CannotUpdate,
                _ => Self::Outdated,
            },
            HostSeen::Failure(_) => Self::Unknown,
            HostSeen::Facts(facts) => {
                let older = facts.version.as_ref().is_none_or(|daemon| daemon < version);
                let staged = facts.owned.as_ref().is_some_and(|owned| owned >= version);
                if older && staged {
                    Self::RestartPending
                } else if older || facts.revision.is_some_and(|daemon| daemon < revision) {
                    Self::Outdated
                } else {
                    Self::Current
                }
            }
        }
    }
}

#[must_use]
pub fn outdated_tooltip(daemon: Option<&Version>, version: &Version) -> String {
    daemon.map_or_else(
        || format!("an older zz, this zz is {version}"),
        |daemon| format!("zz {daemon}, this zz is {version}"),
    )
}

#[must_use]
pub fn restart_pending_tooltip(installed: &Version) -> String {
    format!("zz {installed} installed, restart to finish")
}

#[must_use]
pub fn cannot_update_text(host: &str) -> String {
    format!("Update zz on {host} itself")
}

fn host_list(names: &[&str]) -> String {
    match names {
        [] => "these hosts".to_owned(),
        [name] => (*name).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn sessions_ended(hosts: &[(String, usize)]) -> String {
    let total = session_count(Some(hosts.iter().map(|(_, sessions)| sessions).sum()));
    match hosts {
        [_] | [] => format!("{total} there"),
        hosts => format!(
            "{total}: {}",
            hosts
                .iter()
                .map(|(name, sessions)| format!("{sessions} on {name}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[must_use]
pub fn host_update_question(hosts: &[(String, usize)]) -> (String, String) {
    let names = hosts
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    (
        format!("Update zz on {}?", host_list(&names)),
        format!("This ends {}.", sessions_ended(hosts)),
    )
}

#[must_use]
pub fn host_restart_question(hosts: &[(String, usize)], installed: &Version) -> (String, String) {
    let names = hosts
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    (
        format!("Restart zz on {}?", host_list(&names)),
        format!(
            "zz {installed} is installed. Restarting ends {}.",
            sessions_ended(hosts)
        ),
    )
}

#[must_use]
pub fn update_prompt(offers: &[(String, Option<usize>)]) -> (String, String) {
    let version = env!("CARGO_PKG_VERSION");
    let names = offers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    let hosts = host_list(&names);
    let (runs, daemons, them) = if names.len() == 1 {
        ("runs", "its daemon", "it")
    } else {
        ("run", "their daemons", "them")
    };
    let ended = offers
        .iter()
        .map(|(_, sessions)| *sessions)
        .sum::<Option<usize>>()
        .filter(|_| !offers.is_empty())
        .map_or_else(
            || "the sessions".to_owned(),
            |sessions| session_count(Some(sessions)),
        );
    (
        format!("Update zz on {hosts}?"),
        format!(
            "{hosts} {runs} an older zz. Updating installs zz {version} in ~/.local there and \
             restarts {daemons}, which ends {ended} running on {them}."
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_missing_or_older_remote_offers_an_install() {
        assert_eq!(
            HostFailure::Missing.install_offer(),
            Some(InstallOffer::Install)
        );
        for remote in [Some(107), Some(108), None] {
            assert_eq!(
                HostFailure::Incompatible { local: 108, remote }.install_offer(),
                Some(InstallOffer::Update),
                "{remote:?}"
            );
        }
        assert_eq!(
            HostFailure::DaemonTooOld {
                daemon: 1,
                minimum: 2
            }
            .install_offer(),
            Some(InstallOffer::Update)
        );
        for failure in [
            HostFailure::Incompatible {
                local: 107,
                remote: Some(108),
            },
            HostFailure::ClientTooOld {
                client: 1,
                minimum: 2,
            },
            HostFailure::Unreachable {
                reason: "no route".to_owned(),
            },
        ] {
            assert_eq!(failure.install_offer(), None, "{failure:?}");
        }
    }

    #[test]
    fn a_daemon_behind_names_both_revisions_and_how_many_sessions_end() {
        assert_eq!(DaemonBehind::compare(2, 2), None);
        assert_eq!(DaemonBehind::compare(3, 2), None);
        let behind = DaemonBehind::compare(1, 2).expect("revision 1 is behind 2");
        assert_eq!(
            behind,
            DaemonBehind {
                daemon: 1,
                client: 2
            }
        );
        for (sessions, ends) in [
            (Some(0), "no sessions"),
            (Some(1), "1 session"),
            (Some(3), "3 sessions"),
            (None, "every running session"),
        ] {
            assert_eq!(
                behind.restart_text(sessions),
                format!(
                    "The zz daemon runs an older build, revision 1; this zz is revision 2. \
                     Restarting it to update ends {ends}."
                )
            );
        }
    }

    #[test]
    fn one_prompt_names_every_host_it_updates() {
        let version = env!("CARGO_PKG_VERSION");
        assert_eq!(
            update_prompt(&[("xps".to_owned(), None)]),
            (
                "Update zz on xps?".to_owned(),
                format!(
                    "xps runs an older zz. Updating installs zz {version} in ~/.local there and \
                     restarts its daemon, which ends the sessions running on it."
                )
            )
        );
        assert_eq!(
            update_prompt(&[("xps".to_owned(), Some(1)), ("ubuntu".to_owned(), Some(2))]).1,
            format!(
                "xps and ubuntu run an older zz. Updating installs zz {version} in ~/.local there \
                 and restarts their daemons, which ends 3 sessions running on them."
            )
        );
        let (title, description) = update_prompt(&[
            ("xps".to_owned(), None),
            ("ubuntu".to_owned(), Some(2)),
            ("alienware".to_owned(), None),
        ]);
        assert_eq!(title, "Update zz on xps, ubuntu and alienware?");
        assert!(
            description.starts_with("xps, ubuntu and alienware run an older zz.")
                && description
                    .ends_with("restarts their daemons, which ends the sessions running on them."),
            "{description}"
        );
    }
    #[test]
    fn a_host_is_outdated_by_its_daemons_version_against_this_zz() {
        let version = |text: &str| Version::parse(text).expect("a version");
        let facts = |daemon: Option<&str>, revision, owned: Option<&str>| DaemonFacts {
            version: daemon.map(version),
            revision: Some(revision),
            owned: owned.map(version),
        };
        let epoch = |remote| HostFailure::Incompatible { local: 126, remote };
        let this = version("0.31.0");
        let cases = [
            (
                UpdateRoute::Daemon,
                Ok(facts(Some("0.31.0"), 9, None)),
                UpdateState::Current,
            ),
            (
                UpdateRoute::Daemon,
                Ok(facts(Some("0.30.0"), 9, None)),
                UpdateState::Outdated,
            ),
            (
                UpdateRoute::Cli,
                Ok(facts(Some("0.30.0"), 9, Some("0.30.1"))),
                UpdateState::Outdated,
            ),
            (
                UpdateRoute::Daemon,
                Ok(facts(Some("0.30.0"), 9, Some("0.31.0"))),
                UpdateState::RestartPending,
            ),
            (
                UpdateRoute::Cli,
                Ok(facts(None, 9, None)),
                UpdateState::Outdated,
            ),
            (
                UpdateRoute::Cli,
                Ok(facts(Some("0.31.0"), 8, Some("0.31.0"))),
                UpdateState::Outdated,
            ),
            (
                UpdateRoute::Daemon,
                Ok(facts(Some("0.32.0"), 10, None)),
                UpdateState::Current,
            ),
            (
                UpdateRoute::Daemon,
                Err(epoch(Some(125))),
                UpdateState::CannotUpdate,
            ),
            (
                UpdateRoute::Cli,
                Err(epoch(Some(125))),
                UpdateState::Outdated,
            ),
            (
                UpdateRoute::Cli,
                Err(epoch(Some(127))),
                UpdateState::Unknown,
            ),
        ];
        for (route, seen, expected) in cases {
            let state = match &seen {
                Ok(facts) => UpdateState::of(route, HostSeen::Facts(facts), &this, 9),
                Err(failure) => UpdateState::of(route, HostSeen::Failure(failure), &this, 9),
            };
            assert_eq!(state, expected, "{route:?} {seen:?}");
        }
        assert_eq!(
            UpdateState::of(UpdateRoute::Cli, HostSeen::Nothing, &this, 9),
            UpdateState::Unknown
        );
    }

    #[test]
    fn the_update_question_counts_the_sessions_it_ends() {
        assert_eq!(
            host_update_question(&[("xps".to_owned(), 3)]),
            (
                "Update zz on xps?".to_owned(),
                "This ends 3 sessions there.".to_owned()
            )
        );
        assert_eq!(
            host_update_question(&[("xps".to_owned(), 0), ("alienware".to_owned(), 2)]),
            (
                "Update zz on xps and alienware?".to_owned(),
                "This ends 2 sessions: 0 on xps, 2 on alienware.".to_owned()
            )
        );
    }
}
