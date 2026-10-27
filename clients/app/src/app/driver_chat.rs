use std::{collections::HashMap, time::Duration};

use serde_json::Value;
use zz_client::{
    CoreEvent,
    agent_transcript::AgentPermissionRequest,
    driver_chat::{DriverMessage, DriverMessageKind, driver_messages, driver_pane, driver_steps},
    driver_log::DriverLog,
    program::Program,
    window_summary::pane_program,
};
use zz_gpui::{App, Context, Entity, SharedString, Subscription, Task};
use zz_protocol::{
    AgentConnectionPhase, CommandInvocation, PaneId, ProtocolMessage,
    agent_stream::AgentStreamPayload,
};
use zz_ui::compact::Instant;

use crate::connection::Connection;

const REPLAY_SETTLE: Duration = Duration::from_secs(3);
const WAKE_GRACE: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Wake {
    Asleep,
    Waking,
    Stalled,
}

#[derive(Clone)]
pub(super) struct Driver {
    pub connection: Entity<Connection>,
    pub pane: PaneId,
    pub host: SharedString,
    pub program: Option<&'static Program>,
}

impl PartialEq for Driver {
    fn eq(&self, other: &Self) -> bool {
        self.connection == other.connection && self.pane == other.pane
    }
}

pub(super) struct DriverChat {
    hosts: Vec<(Entity<Connection>, SharedString)>,
    driver: Option<Driver>,
    log: DriverLog,
    applied: u64,
    usage: Option<(u64, u64)>,
    times: HashMap<u64, SharedString>,
    following_since: Instant,
    read: usize,
    wake: Wake,
    wake_timer: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl DriverChat {
    pub(super) fn new() -> Self {
        Self {
            hosts: Vec::new(),
            driver: None,
            log: DriverLog::default(),
            applied: 0,
            usage: None,
            times: HashMap::new(),
            following_since: Instant::now(),
            read: 0,
            wake: Wake::Asleep,
            wake_timer: None,
            _subscriptions: Vec::new(),
        }
    }

    pub(super) fn set_hosts(
        &mut self,
        hosts: Vec<(Entity<Connection>, SharedString)>,
        cx: &mut Context<Self>,
    ) {
        if hosts == self.hosts {
            return;
        }
        self._subscriptions = hosts
            .iter()
            .flat_map(|(connection, _)| {
                [
                    cx.observe(connection, |chat, _, cx| chat.synchronize(cx)),
                    cx.subscribe(connection, |chat, _, event: &CoreEvent, cx| {
                        if matches!(
                            event,
                            CoreEvent::AgentUpdates { .. }
                                | CoreEvent::AgentStateChanged { .. }
                                | CoreEvent::SnapshotChanged
                        ) {
                            chat.synchronize(cx);
                        }
                    }),
                ]
            })
            .collect();
        self.hosts = hosts;
        self.synchronize(cx);
    }

    fn find(&self, cx: &App) -> Option<Driver> {
        self.hosts.iter().find_map(|(connection, host)| {
            let state = connection.read(cx);
            if !state.connected {
                return None;
            }
            let (pane, _) = driver_pane(state.core.snapshot())?;
            Some(Driver {
                connection: connection.clone(),
                pane: pane.id,
                host: host.clone(),
                program: pane_program(pane),
            })
        })
    }

    fn synchronize(&mut self, cx: &mut Context<Self>) {
        let driver = self.find(cx);
        let mut changed = false;
        if driver != self.driver {
            if let Some(old) = self.driver.take() {
                old.connection
                    .update(cx, |connection, cx| connection.follow_agent(None, cx));
            }
            if let Some(new) = &driver {
                let pane = new.pane;
                new.connection
                    .update(cx, |connection, cx| connection.follow_agent(Some(pane), cx));
            }
            if driver.is_some() {
                self.wake = Wake::Asleep;
                self.wake_timer = None;
            }
            self.driver = driver;
            self.reset();
            changed = true;
        }
        changed |= self.apply_journal(cx);
        if changed {
            cx.notify();
        }
    }

    fn reset(&mut self) {
        self.log = DriverLog::default();
        self.applied = 0;
        self.usage = None;
        self.times.clear();
        self.read = 0;
        self.following_since = Instant::now();
    }

    fn apply_journal(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(driver) = &self.driver else {
            return false;
        };
        let connection = driver.connection.read(cx);
        let journal = connection
            .agent_events
            .get(&driver.pane)
            .map_or(&[][..], Vec::as_slice);
        let newest = journal.last().map_or(0, |(sequence, _)| *sequence);
        let skipped = journal
            .first()
            .is_some_and(|(first, _)| self.applied > 0 && *first > self.applied + 1);
        if newest == self.applied && !skipped {
            return false;
        }
        let restart = newest < self.applied || skipped;
        let start = if restart { 0 } else { self.applied };
        let items: Vec<Value> = journal
            [journal.partition_point(|(sequence, _)| *sequence <= start)..]
            .iter()
            .filter_map(|(_, bytes)| serde_json::from_slice(bytes).ok())
            .collect();
        if restart {
            let read = self.read;
            self.reset();
            self.read = read;
        }
        for item in items {
            self.track_usage(&item);
            if let Ok(payload) = serde_json::from_value::<AgentStreamPayload>(item) {
                self.log.apply(payload);
            }
        }
        self.applied = newest;
        self.stamp();
        true
    }

    fn track_usage(&mut self, item: &Value) {
        if matches!(
            item.get("item").and_then(Value::as_str),
            Some("sessionReset" | "sessionSwitched")
        ) {
            self.usage = None;
            for update in item
                .get("replay")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                self.track_usage(update);
            }
        }
        let update = item.get("update").unwrap_or(item);
        if update.get("sessionUpdate").and_then(Value::as_str) == Some("usage_update")
            && let (Some(used), Some(size)) = (
                update.get("used").and_then(Value::as_u64),
                update.get("size").and_then(Value::as_u64),
            )
        {
            self.usage = Some((used, size));
        }
    }

    fn stamp(&mut self) {
        let replaying = self.following_since.elapsed() < REPLAY_SETTLE;
        for entry in self.log.entries() {
            self.times.entry(entry.id()).or_insert_with(|| {
                if replaying {
                    SharedString::default()
                } else {
                    chrono::Local::now().format("%-H:%M").to_string().into()
                }
            });
        }
    }

    pub(super) const fn wake_state(&self) -> Wake {
        self.wake
    }

    pub(super) fn wake(
        &mut self,
        connection: &Entity<Connection>,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        if self.driver.is_some() || self.wake == Wake::Waking {
            return;
        }
        let line = if force {
            "zz driver recover --force"
        } else {
            "zz driver recover"
        };
        let sent = connection.update(cx, |connection, cx| {
            if !connection.connected || connection.core.attached_read_only() {
                return false;
            }
            connection.invoke(CommandInvocation::new("run-shell", ["-b", line]), cx);
            true
        });
        if !sent {
            return;
        }
        self.wake = Wake::Waking;
        self.wake_timer = Some(cx.spawn(async move |chat, cx| {
            cx.background_executor().timer(WAKE_GRACE).await;
            let _ = chat.update(cx, |chat, cx| {
                if chat.driver.is_none() && chat.wake == Wake::Waking {
                    chat.wake = Wake::Stalled;
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    pub(super) const fn driver(&self) -> Option<&Driver> {
        self.driver.as_ref()
    }

    pub(super) const fn usage(&self) -> Option<(u64, u64)> {
        self.usage
    }

    pub(super) fn time(&self, id: u64) -> SharedString {
        self.times.get(&id).cloned().unwrap_or_default()
    }

    fn phase(&self, cx: &App) -> Option<AgentConnectionPhase> {
        let driver = self.driver.as_ref()?;
        driver
            .connection
            .read(cx)
            .core
            .agent_state(driver.pane)
            .map(|state| state.phase.clone())
    }

    pub(super) fn running(&self, cx: &App) -> bool {
        self.driver.is_some()
            && (self.log.live()
                || matches!(
                    self.phase(cx),
                    Some(AgentConnectionPhase::Running | AgentConnectionPhase::AwaitingPermission)
                ))
    }

    pub(super) fn messages(&self, cx: &App) -> Vec<DriverMessage> {
        if self.driver.is_none() {
            return Vec::new();
        }
        driver_messages(self.log.entries(), self.running(cx))
    }

    pub(super) fn last_answer(&self, cx: &App) -> Option<DriverMessage> {
        self.messages(cx)
            .into_iter()
            .rev()
            .find(|message| message.kind == DriverMessageKind::Driver)
    }

    pub(super) fn steps(&self, cx: &App) -> Option<Vec<String>> {
        self.running(cx).then(|| driver_steps(self.log.entries()))
    }

    pub(super) fn permission(&self) -> Option<AgentPermissionRequest> {
        self.log.permissions().first().cloned()
    }

    pub(super) fn unread(&self, cx: &App) -> bool {
        self.messages(cx).len() > self.read
    }

    pub(super) fn mark_read(&mut self, cx: &App) {
        self.read = self.messages(cx).len();
    }

    fn send_message(&self, message: ProtocolMessage, cx: &mut Context<Self>) -> bool {
        let Some(driver) = &self.driver else {
            return false;
        };
        driver.connection.update(cx, |connection, cx| {
            if !connection.connected || connection.core.attached_read_only() {
                return false;
            }
            connection.send(message, cx);
            true
        })
    }

    pub(super) fn send(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.driver.as_ref().map(|driver| driver.pane) else {
            return false;
        };
        let message = if self.running(cx) {
            ProtocolMessage::AgentSteer {
                pane,
                text: text.to_owned(),
            }
        } else {
            ProtocolMessage::AgentPrompt {
                pane,
                text: text.to_owned(),
                images: Vec::new(),
            }
        };
        self.send_message(message, cx)
    }

    pub(super) fn stop(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.driver.as_ref().map(|driver| driver.pane) else {
            return false;
        };
        self.running(cx) && self.send_message(ProtocolMessage::AgentCancel { pane }, cx)
    }

    pub(super) fn respond(
        &mut self,
        request_id: u64,
        option_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.driver.as_ref().map(|driver| driver.pane) else {
            return;
        };
        self.send_message(
            ProtocolMessage::AgentRespondPermission {
                pane,
                request_id,
                option_id,
            },
            cx,
        );
    }

    pub(super) fn sender_name(&self, pane: PaneId, cx: &App) -> (String, Option<&'static Program>) {
        self.driver
            .as_ref()
            .and_then(|driver| {
                driver
                    .connection
                    .read(cx)
                    .core
                    .snapshot()
                    .sessions
                    .iter()
                    .flat_map(|session| &session.windows)
                    .find_map(|window| {
                        window
                            .panes
                            .get(&pane)
                            .map(|snapshot| (window.name.clone(), pane_program(snapshot)))
                    })
            })
            .unwrap_or_else(|| (pane.to_string(), None))
    }
}
