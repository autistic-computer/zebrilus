use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use zz_protocol::{AgentPaneWire, ClientId, PaneId};

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
    BroadcastReplay {
        frames: Vec<(u64, Vec<Vec<u8>>)>,
        also: Option<ClientId>,
    },
    State(AgentPaneWire),
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

    fn publish_agent_replay(
        &self,
        generation: u64,
        pane: PaneId,
        frames: Vec<(u64, Vec<Vec<u8>>)>,
        also: Option<ClientId>,
    ) {
        self.post(generation, pane, Payload::BroadcastReplay { frames, also });
    }

    fn publish_agent_state(&self, generation: u64, pane: PaneId, state: AgentPaneWire) {
        self.post(generation, pane, Payload::State(state));
    }

    fn publish_agent_tool_call(&self, generation: u64, pane: PaneId, call: AgentToolCall) {
        self.post(generation, pane, Payload::ToolCall(call));
    }

    fn send_agent_reply(&self, generation: u64, pane: PaneId, reply: AgentRequestReply) {
        self.post(generation, pane, Payload::Reply(reply));
    }
}
