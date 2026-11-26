use super::*;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use zz_protocol::{AgentPaneWire, ClientId, PaneId, QuotaWindows};

use super::AcceptWake;
use crate::agent::fanout::{AgentPublisher, AgentRequestReply, AgentToolCall};

#[derive(Clone)]
pub(super) struct Sender {
    sender: crossbeam_channel::Sender<Message>,
    pub(super) wake: Arc<AcceptWake>,
    pub(super) pending: Arc<AtomicBool>,
    pub(super) incarnation: Arc<AtomicU64>,
}

#[derive(Clone)]
pub(super) struct Publisher {
    sender: Sender,
    incarnation: u64,
}

pub(super) struct Message {
    pub(super) incarnation: u64,
    pub(super) pane: PaneId,
    pub(super) generation: u64,
    pub(super) payload: Payload,
}

pub(super) enum Payload {
    Updates {
        first_seq: u64,
        items: Vec<Vec<u8>>,
        also: Option<ClientId>,
    },
    Replay {
        client: ClientId,
        frames: Vec<(u64, Vec<Vec<u8>>)>,
    },
    State(AgentPaneWire),
    Folder(PathBuf),
    Quota(QuotaWindows),
    ToolCall(AgentToolCall),
    Reply(AgentRequestReply),
}

impl Sender {
    pub(super) fn new() -> (Self, crossbeam_channel::Receiver<Message>) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        (
            Self {
                sender,
                wake: Arc::new(AcceptWake::new()),
                pending: Arc::new(AtomicBool::new(false)),
                incarnation: Arc::new(AtomicU64::new(0)),
            },
            receiver,
        )
    }

    pub(super) fn publisher(&self) -> Publisher {
        Publisher {
            sender: self.clone(),
            incarnation: self.incarnation.fetch_add(1, Ordering::AcqRel) + 1,
        }
    }

    pub(super) fn notify_loop(&self) {
        if !self.pending.swap(true, Ordering::AcqRel) {
            self.wake.wake();
        }
    }
}

impl Publisher {
    fn post(&self, generation: u64, pane: PaneId, payload: Payload) {
        if self
            .sender
            .sender
            .send(Message {
                incarnation: self.incarnation,
                pane,
                generation,
                payload,
            })
            .is_ok()
        {
            self.sender.notify_loop();
        }
    }
}

impl AgentPublisher for Publisher {
    fn publish_agent_updates(
        &self,
        generation: u64,
        pane: PaneId,
        first_seq: u64,
        items: Vec<Vec<u8>>,
        also: Option<ClientId>,
    ) {
        self.post(
            generation,
            pane,
            Payload::Updates {
                first_seq,
                items,
                also,
            },
        );
    }

    fn send_agent_replay(
        &self,
        generation: u64,
        client: ClientId,
        pane: PaneId,
        frames: Vec<(u64, Vec<Vec<u8>>)>,
    ) {
        self.post(generation, pane, Payload::Replay { client, frames });
    }

    fn publish_agent_state(&self, generation: u64, pane: PaneId, state: AgentPaneWire) {
        self.post(generation, pane, Payload::State(state));
    }

    fn publish_agent_folder(&self, generation: u64, pane: PaneId, cwd: PathBuf) {
        self.post(generation, pane, Payload::Folder(cwd));
    }

    fn publish_agent_quota(&self, generation: u64, pane: PaneId, quota: QuotaWindows) {
        self.post(generation, pane, Payload::Quota(quota));
    }

    fn publish_agent_tool_call(&self, generation: u64, pane: PaneId, call: AgentToolCall) {
        self.post(generation, pane, Payload::ToolCall(call));
    }

    fn send_agent_reply(&self, generation: u64, pane: PaneId, reply: AgentRequestReply) {
        self.post(generation, pane, Payload::Reply(reply));
    }
}

#[cfg(feature = "agent")]
impl Shared {
    pub(super) fn publish_agent_updates(
        &self,
        pane: PaneId,
        first_seq: u64,
        items: Vec<Vec<u8>>,
        also: Option<ClientId>,
    ) {
        let message = Self::event(EventPayload::AgentUpdates {
            pane,
            first_seq,
            items,
        });
        let subscribers = agent_stream_subscribers(&self.inner.lock(), pane, also);
        let Ok(encoded) = zz_protocol::encode_protocol_message(&message).map(Arc::<[u8]>::from)
        else {
            return;
        };
        for subscriber in subscribers {
            subscriber.enqueue_agent(pane, first_seq, Arc::clone(&encoded));
        }
    }

    pub(super) fn send_agent_replay(
        &self,
        client: ClientId,
        pane: PaneId,
        frames: Vec<(u64, Vec<Vec<u8>>)>,
    ) {
        let Some(subscriber) =
            self.read_client(client, |c| c.and_then(|c| c.subscriber.as_ref()).cloned())
        else {
            return;
        };
        let through_seq = frames.last().map_or(0, |(first_seq, items)| {
            first_seq.saturating_add(items.len().saturating_sub(1) as u64)
        });
        let mut encoded = Vec::with_capacity(frames.len());
        for (first_seq, items) in frames {
            let message = Self::event(EventPayload::AgentUpdates {
                pane,
                first_seq,
                items,
            });
            let Ok(frame) = subscriber.encode_message(&message) else {
                log::error!("failed to encode an agent replay batch for {pane}");
                return;
            };
            encoded.push((first_seq, frame));
        }
        if !encoded.is_empty() && !subscriber.enqueue_agent_replay(pane, encoded) {
            return;
        }
        let _ = subscriber.enqueue_reliable(&Self::event(EventPayload::AgentReplayed {
            pane,
            through_seq,
        }));
    }

    pub(super) fn apply_agent_state(
        self: &Arc<Self>,
        pane: PaneId,
        state: AgentPaneWire,
    ) -> Option<PendingHookEvent> {
        let facts = agent_chat_facts(&state);
        let (event, status_changed) = {
            let mut inner = self.inner.lock();
            if inner.engine.set_agent_chat(pane, Some(facts.clone())) {
                let status_changed = pane_summary::apply_pane_status(
                    &mut inner,
                    pane,
                    pane_summary::agent_state_status(Some(facts.state)),
                    unix_timestamp(),
                );
                let event = PendingHookEvent::live_pane("agent-state-changed", pane, &inner.engine)
                    .map(|mut event| {
                        event.variables.extend([
                            ("agent_state".to_owned(), facts.state.to_owned()),
                            (
                                "agent_pending_permission".to_owned(),
                                facts
                                    .pending_permission
                                    .map(|id| id.to_string())
                                    .unwrap_or_default(),
                            ),
                        ]);
                        event
                    });
                (event, status_changed)
            } else {
                (None, false)
            }
        };
        if status_changed {
            self.request_publish(timers::PublishReason::Tree);
        }
        let options =
            serde_json::from_str::<serde_json::Value>(&state.config_options).unwrap_or_default();
        let options = options.as_array().map(Vec::as_slice).unwrap_or_default();
        let adopted = self
            .inner
            .lock()
            .engine
            .state
            .update_agent(pane, |agent| {
                if let Some(session_id) = &state.session_id {
                    agent.session_id = Some(session_id.clone());
                }
                for (id, shown) in [("model", &mut agent.model), ("effort", &mut agent.effort)] {
                    if let Some(option) = options.iter().find(|option| option["id"] == id) {
                        *shown = option["currentValue"]
                            .as_str()
                            .filter(|value| !value.is_empty() && *value != "default")
                            .map(str::to_owned);
                    }
                }
            })
            .unwrap_or(false);
        if adopted {
            self.publish_snapshot_state();
            self.note_child_descriptor(pane);
        }
        self.apply_agent_title(pane, state.title.as_deref());
        let subscribers = {
            let inner = self.inner.lock();
            attached_clients_for_pane(&inner, pane)
                .into_iter()
                .flatten()
                .copied()
                .chain(agent_watchers(&inner, pane))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter_map(|client| {
                    inner
                        .client(client)
                        .and_then(|c| c.subscriber.as_ref())
                        .cloned()
                })
                .collect::<Vec<_>>()
        };
        let payload = EventPayload::AgentState { pane, state };
        for subscriber in subscribers {
            Self::send_event(&subscriber, payload.clone());
        }
        event
    }

    pub(super) fn apply_agent_folder(self: &Arc<Self>, pane: PaneId, cwd: PathBuf) {
        let moved = {
            let mut inner = self.inner.lock();
            let folder = cwd.to_string_lossy().into_owned();
            let moved = inner
                .engine
                .state
                .update_agent(pane, |agent| agent.cwd = Some(cwd))
                .unwrap_or(false);
            if moved {
                let mut facts = inner
                    .engine
                    .pane_runtime_facts(pane)
                    .cloned()
                    .unwrap_or_default();
                facts.current_path = folder;
                inner.engine.set_pane_runtime_facts(pane, facts);
            }
            moved
        };
        if moved {
            self.publish_snapshot_state();
            self.request_pane_location(pane);
        }
    }

    fn apply_agent_title(self: &Arc<Self>, pane: PaneId, title: Option<&str>) {
        let events = {
            let mut inner = self.inner.lock();
            if !inner.engine.allow_set_title(pane) {
                return;
            }
            let scope = hook_events::HookScope::open(&mut inner.engine);
            if !inner.engine.update_agent_title(pane, title) {
                return;
            }
            scope.finish(&inner.engine, "").events
        };
        self.request_publish(timers::PublishReason::Tree);
        self.run_event_hooks(events);
    }

    pub(super) fn agent_tool_call_event(
        &self,
        pane: PaneId,
        call: AgentToolCall,
    ) -> Option<PendingHookEvent> {
        {
            let inner = self.inner.lock();
            PendingHookEvent::live_pane("agent-tool-call", pane, &inner.engine).map(|mut event| {
                event.variables.extend([
                    ("tool_call_id".to_owned(), call.id),
                    ("tool_title".to_owned(), call.title),
                    ("tool_kind".to_owned(), call.kind),
                    ("tool_status".to_owned(), call.status),
                ]);
                event
            })
        }
    }

    pub(super) fn send_agent_reply(&self, pane: PaneId, reply: AgentRequestReply) {
        let AgentRequestReply::Sessions { client, result } = reply;
        let payload = EventPayload::AgentSessions {
            pane,
            request_id: 0,
            result,
        };
        let subscriber =
            self.read_client(client, |c| c.and_then(|c| c.subscriber.as_ref()).cloned());
        if let Some(subscriber) = subscriber {
            Self::send_event(&subscriber, payload);
        }
    }
}

#[cfg(feature = "agent")]
fn agent_chat_facts(state: &AgentPaneWire) -> zz_mux::AgentChatFacts {
    let pending = state.pending_permission.as_ref();
    let asks = pending.is_some_and(|permission| {
        serde_json::from_str::<serde_json::Value>(&permission.payload)
            .is_ok_and(|payload| payload.get("questions").is_some())
    });
    let word = match state.phase {
        zz_protocol::AgentConnectionPhase::Failed { .. } => "failed",
        zz_protocol::AgentConnectionPhase::Starting => "starting",
        _ if asks => "needs-input",
        zz_protocol::AgentConnectionPhase::AwaitingPermission => "needs-approval",
        _ if pending.is_some() => "needs-approval",
        zz_protocol::AgentConnectionPhase::Running => "working",
        zz_protocol::AgentConnectionPhase::Ready => "idle",
    };
    zz_mux::AgentChatFacts {
        state: word,
        pending_permission: pending.map(|permission| permission.request_id),
    }
}

#[cfg(feature = "agent")]
fn agent_stream_subscribers(
    inner: &ServerState,
    pane: PaneId,
    also: Option<ClientId>,
) -> Vec<Arc<OutboundMailbox>> {
    let mut clients = attached_clients_for_pane(inner, pane)
        .into_iter()
        .flatten()
        .filter(|client| {
            inner
                .client(**client)
                .and_then(|c| c.visible_agents.as_ref())
                .is_some_and(|visible| visible.contains(&pane))
        })
        .copied()
        .collect::<BTreeSet<_>>();
    clients.extend(also.filter(|client| client_is_attached_to_pane(inner, *client, pane)));
    clients.extend(agent_watchers(inner, pane));
    clients
        .into_iter()
        .filter_map(|client| {
            inner
                .client(client)
                .filter(|c| !c.muted_agents.contains(&pane))
                .and_then(|c| c.subscriber.as_ref())
                .cloned()
        })
        .collect()
}

#[cfg(feature = "agent")]
fn agent_watchers(inner: &ServerState, pane: PaneId) -> impl Iterator<Item = ClientId> + '_ {
    inner
        .clients
        .iter()
        .filter(move |(_, client)| client.watched_agents.contains(&pane))
        .map(|(client, _)| *client)
}
