//! The driver chat: the conversation with the Driver Agent, read from its pane's
//! lens stream on whichever host runs it. The driver is the first terminal
//! running Claude Code or Codex in a session named `driver`; zz follows it from
//! every window, attached to that session or not. While `driver-agent` is on
//! and no host has a `driver` session, the sidebar offers to wake one, which zz
//! starts on this machine with the model and effort the Agents settings pick.
//! zz never starts one on its own, so restarting zz spends no tokens.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use zz_cli::driver::Recovery;
pub(crate) use zz_client::driver_chat::DRIVER_SESSION;
use zz_client::{
    driver_chat::{DriverMessage, DriverMessageKind, driver_messages, driver_pane, driver_steps},
    program::Program,
    window_summary::pane_program,
};
use zz_gpui::{
    AnyElement, App, AppContext as _, Context, Entity, Hsla, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use zz_protocol::{AgentProvider, ClientMessageKind, CommandInvocation, PaneId};
use zz_ui::{
    ActiveTheme as _, Ground,
    agent::{
        chat::{ChatBubble, ChatSender, ChatTyping, Receipt, TYPING_STEPS},
        presentation::{permission_card, permission_option},
    },
    button::Button,
    command::target_hosts,
    target::TargetHost,
};

use super::controller::{
    AgentController, AgentPaneState, AgentPermissionKind, AgentPermissionRequest,
};
use crate::{
    config::{DriverEffort, DriverModel},
    mux::{
        client::MuxClient,
        hosts::{HostId, HostState},
    },
};

/// Entries that arrive this soon after zz starts following a driver are its
/// history being replayed, not messages arriving, so they get no time.
const REPLAY_SETTLE: Duration = Duration::from_secs(3);
/// How long the sidebar keeps saying the driver wakes after a host started or
/// found one, while its session reaches this window.
const WAKE_GRACE: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Driver {
    pub(crate) host: HostId,
    pub(crate) pane: PaneId,
    pub(crate) provider: AgentProvider,
    pub(crate) cwd: PathBuf,
    pub(crate) host_name: SharedString,
    pub(crate) program: Option<&'static Program>,
}

/// The driver: the first terminal running Claude Code or Codex in a session
/// named [`DRIVER_SESSION`], on the first host in fleet order that has one.
pub(crate) fn find_driver(mux: &MuxClient, local_name: &SharedString) -> Option<Driver> {
    mux.fleet_hosts().find_map(|(host, name, _, snapshot)| {
        let (pane, provider) = driver_pane(snapshot?)?;
        Some(Driver {
            host,
            pane: pane.id,
            provider,
            cwd: Some(PathBuf::from(&pane.summary.cwd))
                .filter(|cwd| cwd.is_absolute())
                .unwrap_or_else(|| PathBuf::from("/")),
            host_name: if host == HostId::LOCAL {
                local_name.clone()
            } else {
                SharedString::from(name.to_owned())
            },
            program: pane_program(pane),
        })
    })
}

/// The session the app's session lists leave out: the driver's, while
/// `driver-agent` is on. The CLI, `list-sessions` and `choose-tree` still list it.
pub(crate) fn hidden_session(cx: &App) -> Option<&'static str> {
    crate::config::resolved_config(cx)
        .driver_agent
        .value
        .then_some(DRIVER_SESSION)
}

/// The command zz starts the driver with: the CLI `model` names, with the
/// model and effort as its flags. Codex has no `max` effort and gets `xhigh`.
pub(crate) fn driver_command(model: DriverModel, effort: DriverEffort) -> String {
    let effort = (effort != DriverEffort::Default).then_some(effort);
    if model == DriverModel::Codex {
        return effort.map_or_else(
            || "codex".to_owned(),
            |effort| {
                let effort = if effort == DriverEffort::Max {
                    DriverEffort::Xhigh
                } else {
                    effort
                };
                format!("codex -c model_reasoning_effort={}", effort.as_str())
            },
        );
    }
    let mut command = "claude".to_owned();
    if model != DriverModel::Claude {
        command.push_str(" --model ");
        command.push_str(model.as_str());
    }
    if let Some(effort) = effort {
        command.push_str(" --effort ");
        command.push_str(effort.as_str());
    }
    command
}

/// Whether zz may ask the fleet for a driver now: this machine's daemon is
/// connected and has sent its sessions. `zz driver recover` asks every host
/// itself, so a host zz is still reconnecting to cannot hide a live driver.
fn may_start_driver(mux: &MuxClient) -> bool {
    mux.fleet_hosts().any(|(host, _, state, snapshot)| {
        host == HostId::LOCAL
            && matches!(state, HostState::Connected)
            && snapshot.is_some_and(|snapshot| !snapshot.sessions.is_empty())
    })
}

pub(crate) struct DriverChat {
    mux: Entity<MuxClient>,
    controller: Entity<AgentController>,
    local_name: SharedString,
    driver: Option<Driver>,
    following_since: Instant,
    times: HashMap<u64, Option<SharedString>>,
    read: usize,
    error: Option<Arc<str>>,
    recovering: bool,
    /// Set while a driver another host started or runs has yet to show up.
    woke: bool,
    unreachable: Option<(usize, usize)>,
    kept: HashSet<(HostId, usize)>,
}

impl DriverChat {
    pub(crate) fn new(
        mux: Entity<MuxClient>,
        local_name: SharedString,
        cx: &mut Context<Self>,
    ) -> Self {
        let controller = cx.new(|_| AgentController::for_driver());
        controller.update(cx, |controller, _| controller.attach_mux(mux.clone()));
        cx.observe(&mux, |chat, _, cx| chat.synchronize(cx))
            .detach();
        cx.observe_global::<crate::config::AppConfig>(Self::synchronize)
            .detach();
        cx.observe(&controller, |chat, _, cx| {
            chat.stamp(cx);
            cx.notify();
        })
        .detach();
        let mut chat = Self {
            mux,
            controller,
            local_name,
            driver: None,
            following_since: Instant::now(),
            times: HashMap::new(),
            read: 0,
            error: None,
            recovering: false,
            woke: false,
            unreachable: None,
            kept: HashSet::new(),
        };
        chat.synchronize(cx);
        chat
    }

    fn synchronize(&mut self, cx: &mut Context<Self>) {
        let config = crate::config::resolved_config(cx);
        if !config.driver_agent.value {
            self.unreachable = None;
        }
        let driver = if config.driver_agent.value {
            self.keep_hosts(cx);
            find_driver(self.mux.read(cx), &self.local_name)
        } else {
            None
        };
        if driver.is_some() {
            self.woke = false;
        }
        if driver != self.driver {
            let pane = driver.as_ref().map(|driver| driver.pane);
            let same_pane = pane.is_some()
                && self
                    .driver
                    .as_ref()
                    .map(|driver| (driver.host, driver.pane))
                    == driver.as_ref().map(|driver| (driver.host, driver.pane));
            if !same_pane {
                self.times.clear();
                self.read = 0;
                self.error = None;
                self.following_since = Instant::now();
            }
            self.controller.update(cx, |controller, cx| {
                controller.retain_panes(&pane.into_iter().collect::<BTreeSet<_>>(), cx);
                if let Some(driver) = &driver {
                    controller.ensure_pane(driver.pane, driver.provider, driver.cwd.clone());
                }
            });
            self.driver = driver;
            cx.notify();
        }
        let followed = self
            .driver
            .as_ref()
            .map(|driver| (driver.host, driver.pane));
        let events = self.mux.update(cx, |mux, _| {
            mux.follow_driver(followed);
            mux.has_driver_events().then(|| mux.take_driver_events())
        });
        if let Some(events) = events {
            self.controller.update(cx, |controller, cx| {
                for (pane, items) in events.items {
                    controller.apply_stream_items(pane, items, cx);
                }
                for (pane, state) in events.states {
                    controller.apply_pane_state(pane, &state, cx);
                }
            });
        }
    }

    /// Ask the fleet for the driver with `zz driver recover`: follow a live
    /// one, or start one on this machine from the newest copy any host keeps
    /// once most hosts answered. `force` starts it with fewer answers.
    fn start(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.recovering || !may_start_driver(self.mux.read(cx)) {
            return;
        }
        self.recovering = true;
        cx.notify();
        let config = crate::config::resolved_config(cx);
        let socket = self.mux.read(cx).local_socket_path().to_path_buf();
        let command = driver_command(config.driver_model.value, config.driver_effort.value);
        let hosts = crate::config::driver_hosts(cx);
        let recovery = cx
            .background_executor()
            .spawn(async move { zz_cli::driver::recover(&socket, force, &command, &hosts) });
        cx.spawn(async move |chat, cx| {
            let recovery = recovery.await;
            let _ = chat.update(cx, |chat, cx| chat.recovered(recovery, cx));
        })
        .detach();
    }

    fn recovered(&mut self, recovery: Result<Recovery, String>, cx: &mut Context<Self>) {
        self.recovering = false;
        self.unreachable = None;
        match recovery {
            Ok(Recovery::Unreachable { answered, hosts }) => {
                self.unreachable = Some((answered, hosts));
            }
            Ok(Recovery::Alive { .. } | Recovery::Started { .. }) => {
                self.woke = self.driver.is_none();
                cx.spawn(async move |chat, cx| {
                    cx.background_executor().timer(WAKE_GRACE).await;
                    let _ = chat.update(cx, |chat, cx| {
                        chat.woke = false;
                        cx.notify();
                    });
                })
                .detach();
            }
            Err(error) => {
                self.mux.update(cx, |_, cx| {
                    MuxClient::emit_notification(
                        ClientMessageKind::Warning,
                        format!("zz could not start the Driver Agent: {error}"),
                        cx,
                    );
                });
            }
        }
        cx.notify();
    }

    /// How many hosts answered when too few did for zz to start a driver.
    pub(crate) const fn unreachable(&self) -> Option<(usize, usize)> {
        self.unreachable
    }

    /// Start a driver on this machine although too few hosts answered.
    pub(crate) fn start_here(&mut self, cx: &mut Context<Self>) {
        self.start(true, cx);
    }

    /// The sidebar's Wake button: follow the driver a host still runs, or
    /// start one.
    pub(crate) fn wake(&mut self, cx: &mut Context<Self>) {
        self.start(false, cx);
    }

    /// Whether the sidebar offers to wake a driver: `driver-agent` is on, no
    /// host runs one and the last start did not stop short of a quorum.
    pub(crate) fn asleep(&self, cx: &App) -> bool {
        self.driver.is_none()
            && self.unreachable.is_none()
            && crate::config::resolved_config(cx).driver_agent.value
    }

    /// Whether a wake is under way.
    pub(crate) const fn waking(&self) -> bool {
        self.recovering || self.woke
    }

    /// Start `zz driver keep` on every connected host once per connection, so
    /// each keeps its own copy of the driver's chat. A second keeper on a host
    /// finds the first one's lock and exits.
    fn keep_hosts(&mut self, cx: &App) {
        let mux = self.mux.read(cx);
        let connected = mux
            .fleet_hosts()
            .filter(|(_, _, state, _)| matches!(state, HostState::Connected))
            .filter_map(|(host, _, _, _)| Some((host, mux.connection_identity(host)?)))
            .collect::<Vec<_>>();
        for connection in connected {
            if self.kept.insert(connection) {
                mux.execute_on_host(
                    connection.0,
                    CommandInvocation::new("run-shell", ["-b", "zz driver keep"]),
                );
            }
        }
    }

    fn stamp(&mut self, cx: &App) {
        let Some(pane) = self.driver.as_ref().map(|driver| driver.pane) else {
            return;
        };
        let Some((entries, _, _)) = self.controller.read(cx).pane_entries(pane) else {
            return;
        };
        let replaying = self.following_since.elapsed() < REPLAY_SETTLE;
        for entry in entries {
            self.times.entry(entry.id()).or_insert_with(|| {
                (!replaying).then(|| chrono::Local::now().format("%-H:%M").to_string().into())
            });
        }
    }

    pub(crate) fn driver(&self) -> Option<&Driver> {
        self.driver.as_ref()
    }

    pub(crate) const fn mux(&self) -> &Entity<MuxClient> {
        &self.mux
    }

    /// The driver's context window: tokens used and its size.
    pub(crate) fn usage(&self, cx: &App) -> Option<(u64, u64)> {
        self.state(cx)?.usage
    }

    fn state(&self, cx: &App) -> Option<AgentPaneState> {
        self.controller
            .read(cx)
            .pane_state(self.driver.as_ref()?.pane)
    }

    /// Whether the driver is in a turn, which the composer's button queues
    /// behind.
    pub(crate) fn running(&self, cx: &App) -> bool {
        self.state(cx)
            .is_some_and(|state| state.connection.has_active_turn())
    }

    pub(crate) fn messages(&self, cx: &App) -> Vec<DriverMessage> {
        let Some(driver) = &self.driver else {
            return Vec::new();
        };
        self.controller
            .read(cx)
            .pane_entries(driver.pane)
            .map(|(entries, _, _)| driver_messages(entries, self.running(cx)))
            .unwrap_or_default()
    }

    /// The driver's last answer, which the sidebar shows.
    pub(crate) fn last_answer(&self, cx: &App) -> Option<String> {
        self.messages(cx)
            .into_iter()
            .rev()
            .find(|message| message.kind == DriverMessageKind::Driver)
            .map(|message| message.text)
    }

    /// The driver's steps in its live turn, or `None` while it is not working.
    pub(crate) fn steps(&self, cx: &App) -> Option<Vec<String>> {
        let driver = self.driver.as_ref()?;
        if !self.running(cx) {
            return None;
        }
        Some(
            self.controller
                .read(cx)
                .pane_entries(driver.pane)
                .map(|(entries, _, _)| driver_steps(entries))
                .unwrap_or_default(),
        )
    }

    /// The thread as a surface draws it: the bubbles, then the driver typing
    /// with its latest steps while it works. The count grows with every
    /// bubble and step, and when an answer takes the typing bubble's place,
    /// so a surface can follow the bottom as it grows.
    pub(crate) fn thread(
        &self,
        scope: &'static str,
        ground: Ground,
        cx: &App,
    ) -> (Vec<AnyElement>, usize) {
        let mut bubbles = self.bubbles(scope, ground, cx);
        let mut depth = bubbles.len() * (TYPING_STEPS + 2);
        if let Some(steps) = self.steps(cx) {
            depth += 1 + steps.len().min(TYPING_STEPS);
            bubbles.push(
                ChatTyping::new((scope, usize::MAX))
                    .steps(steps)
                    .on(ground)
                    .into_any_element(),
            );
        }
        (bubbles, depth)
    }

    /// The color of the dot on the chat's title-bar button: an approval waiting
    /// on you, or messages you have not seen.
    pub(crate) fn attention(&self, cx: &App) -> Option<Hsla> {
        if self
            .state(cx)
            .is_some_and(|state| !state.pending_permissions.is_empty())
        {
            return Some(cx.theme().warning);
        }
        (self.messages(cx).len() > self.read).then_some(cx.theme().accent)
    }

    /// Count everything up to now as seen. Called by the surfaces that show
    /// the chat; it does not notify, so a render can call it.
    pub(crate) fn mark_read(&mut self, cx: &App) {
        self.read = self.messages(cx).len();
    }

    pub(crate) fn error(&self) -> Option<&Arc<str>> {
        self.error.as_ref()
    }

    /// Type `text` into the driver's pane as your turn.
    pub(crate) fn send(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.driver.as_ref().map(|driver| driver.pane) else {
            return false;
        };
        let sent = self.controller.update(cx, |controller, cx| {
            controller.prompt(pane, text, Vec::new(), cx)
        });
        self.error = sent.err();
        cx.notify();
        self.error.is_none()
    }

    /// Type `text` into the driver's running turn, which it reads as
    /// steering; with no turn running it is your next turn, as [`Self::send`].
    pub(crate) fn steer(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.driver.as_ref().map(|driver| driver.pane) else {
            return false;
        };
        let sent = self
            .controller
            .update(cx, |controller, cx| controller.steer(pane, text, cx));
        self.error = sent.err();
        cx.notify();
        self.error.is_none()
    }

    /// How many of your messages wait behind the driver's turn.
    pub(crate) fn queued(&self, cx: &App) -> usize {
        self.state(cx).map_or(0, |state| state.queued_prompts)
    }

    /// Stop the driver's turn, the way Esc does in its terminal. Whether it
    /// was working.
    pub(crate) fn stop(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.driver.as_ref().map(|driver| driver.pane) else {
            return false;
        };
        if !self.running(cx) {
            return false;
        }
        self.controller
            .update(cx, |controller, cx| controller.cancel(pane, cx));
        true
    }

    /// Take your last message back to edit it. Messages still queued behind
    /// the driver's turn come back through [`Self::take_reclaimed`];
    /// otherwise this is the last one you sent, and the driver stops if it is
    /// still working on it.
    pub(crate) fn take_back(&mut self, cx: &mut Context<Self>) -> Option<String> {
        let pane = self.driver.as_ref()?.pane;
        if self.queued(cx) > 0 {
            self.controller
                .update(cx, |controller, cx| controller.unqueue_prompts(pane, cx));
            return None;
        }
        let messages = self.messages(cx);
        let last = messages
            .iter()
            .rposition(|message| matches!(message.kind, DriverMessageKind::Mine { .. }))?;
        if messages[last + 1..]
            .iter()
            .all(|message| message.kind != DriverMessageKind::Driver)
        {
            self.stop(cx);
        }
        Some(messages[last].text.clone())
    }

    /// Queued messages the daemon handed back, for the composer to refill.
    pub(crate) fn take_reclaimed(&mut self, cx: &mut Context<Self>) -> Option<String> {
        let pane = self.driver.as_ref()?.pane;
        self.controller
            .update(cx, |controller, _| controller.take_pending_composer(pane))
    }

    /// Pick option `index` of the oldest approval waiting on you, the way a
    /// digit does in the lens. Whether there was one to answer.
    pub(crate) fn answer_waiting(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let Some((request_id, option_id)) = self.state(cx).and_then(|state| {
            let request = state.pending_permissions.first()?;
            Some((request.request_id, request.options.get(index)?.id.clone()))
        }) else {
            return false;
        };
        self.respond(request_id, Some(option_id), cx);
        true
    }

    fn respond(&mut self, request_id: u64, option_id: Option<String>, cx: &mut Context<Self>) {
        let Some(pane) = self.driver.as_ref().map(|driver| driver.pane) else {
            return;
        };
        self.controller.update(cx, |controller, cx| {
            controller.respond_permission(pane, request_id, option_id, cx);
        });
    }

    /// Who an agent notice is about: its window's name on the driver's host and
    /// the program its tile shows.
    fn sender(&self, pane: PaneId, cx: &App) -> ChatSender {
        let found = self.driver.as_ref().and_then(|driver| {
            let mux = self.mux.read(cx);
            let snapshot = mux.host_snapshot(driver.host)?;
            snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .find_map(|window| {
                    window
                        .panes
                        .get(&pane)
                        .map(|snapshot| (window.name.clone(), pane_program(snapshot)))
                })
        });
        match found {
            Some((name, program)) => ChatSender {
                name: name.into(),
                program,
            },
            None => ChatSender {
                name: pane.to_string().into(),
                program: None,
            },
        }
    }

    /// The driver's tile and name for the chat header.
    pub(crate) fn driver_sender(&self) -> Option<ChatSender> {
        self.driver.as_ref().map(|driver| ChatSender {
            name: "Driver".into(),
            program: driver.program,
        })
    }

    /// The bubbles, drawn on `ground`, with ids under `scope`.
    pub(crate) fn bubbles(&self, scope: &'static str, ground: Ground, cx: &App) -> Vec<AnyElement> {
        let backend = crate::command::palette::palette_backend(self.mux.clone());
        let hosts: Arc<[TargetHost]> = target_hosts(&backend.tree(cx)).into();
        self.messages(cx)
            .into_iter()
            .map(|message| {
                let time = self
                    .times
                    .get(&message.id)
                    .cloned()
                    .flatten()
                    .unwrap_or_default();
                let id = (scope, usize::try_from(message.id).unwrap_or(usize::MAX));
                match message.kind {
                    DriverMessageKind::Mine { read } => ChatBubble::mine(id, message.text, time)
                        .receipt(if read {
                            Receipt::Read
                        } else {
                            Receipt::Delivered
                        })
                        .links(hosts.clone())
                        .on(ground)
                        .into_any_element(),
                    DriverMessageKind::Driver => ChatBubble::theirs(id, message.text, time)
                        .links(hosts.clone())
                        .on(ground)
                        .into_any_element(),
                    DriverMessageKind::Agent(pane) => ChatBubble::theirs(id, message.text, time)
                        .sender(self.sender(pane, cx))
                        .links(hosts.clone())
                        .on(ground)
                        .into_any_element(),
                }
            })
            .collect()
    }

    /// The oldest approval or question waiting on you, as the lens's
    /// permission card; picking an option answers it in the driver's pane.
    pub(crate) fn ask(chat: &Entity<Self>, scope: &'static str, cx: &App) -> Option<AnyElement> {
        let request: AgentPermissionRequest = chat
            .read(cx)
            .state(cx)?
            .pending_permissions
            .first()?
            .clone();
        let options = request
            .options
            .iter()
            .enumerate()
            .map(|(index, option)| {
                let chat = chat.clone();
                let option_id = option.id.clone();
                let request_id = request.request_id;
                permission_option(
                    (SharedString::from(format!("{scope}-option")), index),
                    index,
                    option.name.clone(),
                    matches!(
                        option.kind,
                        AgentPermissionKind::RejectOnce | AgentPermissionKind::RejectAlways
                    ),
                    index == 0,
                    true,
                    cx,
                )
                .on_click(move |_, _, cx| {
                    let option_id = option_id.clone();
                    chat.update(cx, |chat, cx| chat.respond(request_id, Some(option_id), cx));
                })
                .into_any_element()
            })
            .collect();
        let cancel = {
            let chat = chat.clone();
            let request_id = request.request_id;
            Button::new(SharedString::from(format!("{scope}-cancel")))
                .tooltip("Cancel request")
                .on_click(move |_, _, cx| {
                    chat.update(cx, |chat, cx| chat.respond(request_id, None, cx));
                })
        };
        Some(
            div()
                .pb(px(8.0))
                .child(permission_card(request.title, None, options, cancel, cx))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_driver_starts_with_its_model_and_effort_as_flags() {
        assert_eq!(
            driver_command(DriverModel::Claude, DriverEffort::Default),
            "claude"
        );
        assert_eq!(
            driver_command(DriverModel::Opus, DriverEffort::Xhigh),
            "claude --model opus --effort xhigh"
        );
        assert_eq!(
            driver_command(DriverModel::Codex, DriverEffort::Default),
            "codex"
        );
        assert_eq!(
            driver_command(DriverModel::Codex, DriverEffort::Max),
            "codex -c model_reasoning_effort=xhigh",
            "Codex has no max effort"
        );
    }
}
