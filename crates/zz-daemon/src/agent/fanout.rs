use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use agent_client_protocol::v1::schema::SessionUpdate;
use parking_lot::{Condvar, Mutex};
use serde_json::Value;
use zz_protocol::chat::{ChatEvent, ChatLog};
use zz_protocol::{
    AgentPaneWire, AgentPermissionWire, AgentProvider, ClientId, ClientInstanceId,
    MAX_AGENT_PROMPT_BYTES, MAX_AGENT_QUEUED_PROMPTS, MAX_AGENT_RESULT_BYTES,
    MAX_AGENT_UPDATES_BYTES, PaneId, QuotaWindows,
};

use crate::agent::{
    chat_bridge::is_turn_mark,
    host::{
        AgentConnectionPhase, AgentHost, AgentPaneSpec, AgentPaneState, AgentTurnFailure,
        AgentTurnWaiter, HostCommand, PromptSource, QueuedPrompt,
    },
    journal::{AgentJournal, ChildRecord, JournalEntry},
    runtime::AgentSpawnConfig,
    stream::{AgentPrompt, AgentStreamItem, AgentStreamPayload},
};

const BATCH_WINDOW: Duration = Duration::from_millis(25);
const MAX_REPLAY_RING_BYTES: usize = 2 * MAX_AGENT_UPDATES_BYTES;
const MAX_REBUILD_BYTES: usize = MAX_AGENT_UPDATES_BYTES / 2;
const MAX_AGENT_FRAME_BYTES: usize = MAX_AGENT_RESULT_BYTES;
const MAX_RECLAIMED_PROMPTS: usize = MAX_AGENT_QUEUED_PROMPTS + 1;

#[derive(Clone, Default)]
pub(crate) struct AgentToolCall {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) kind: String,
    pub(crate) status: String,
    pub(crate) page: Option<String>,
}

pub(crate) enum AgentRequestReply {
    Sessions { client: ClientId, result: String },
}

pub(crate) trait AgentPublisher: Send + Sync + 'static {
    fn publish_agent_updates(
        &self,
        generation: u64,
        pane: PaneId,
        first_seq: u64,
        items: Vec<Vec<u8>>,
        also: Option<ClientId>,
    );
    fn send_agent_replay(
        &self,
        generation: u64,
        client: ClientId,
        pane: PaneId,
        frames: Vec<(u64, Vec<Vec<u8>>)>,
    );
    fn publish_agent_state(&self, generation: u64, pane: PaneId, state: AgentPaneWire);
    fn publish_agent_folder(&self, generation: u64, pane: PaneId, cwd: PathBuf);
    fn publish_agent_quota(&self, generation: u64, pane: PaneId, quota: QuotaWindows);
    fn publish_agent_tool_call(&self, generation: u64, pane: PaneId, call: AgentToolCall);
    fn send_agent_reply(&self, generation: u64, pane: PaneId, reply: AgentRequestReply);
}

pub(crate) const MAX_SENDS_SINCE_USER: u32 = 8;

pub(crate) struct AgentRuntime {
    host: AgentHost,
    fanout: Arc<AgentFanout>,
    generation: AtomicU64,
    _publisher: Arc<dyn AgentPublisher>,
    lifecycle: Arc<Mutex<()>>,
    sends: Mutex<BTreeMap<(PaneId, PaneId), u32>>,
    journal: Option<Arc<AgentJournal>>,
    children: Mutex<BTreeMap<(AgentProvider, String), Vec<ChildRecord>>>,
}

impl AgentRuntime {
    pub(crate) fn new(
        publisher: &Arc<dyn AgentPublisher>,
        config: AgentSpawnConfig,
        journal: Option<Arc<AgentJournal>>,
    ) -> Self {
        let lifecycle = Arc::new(Mutex::new(()));
        let journal_handle = journal.clone();
        let fanout = Arc::new(AgentFanout::new(
            publisher,
            journal.clone(),
            Arc::clone(&lifecycle),
        ));
        let sink_fanout = Arc::downgrade(&fanout);
        let host = AgentHost::with_journal(
            config,
            Box::new(move |pane, generation, state, item| {
                if let Some(fanout) = sink_fanout.upgrade() {
                    fanout.accept(pane, generation, &state, item);
                }
            }),
            journal,
        );
        Self {
            host,
            fanout,
            generation: AtomicU64::new(1),
            _publisher: Arc::clone(publisher),
            lifecycle,
            sends: Mutex::new(BTreeMap::new()),
            journal: journal_handle,
            children: Mutex::new(BTreeMap::new()),
        }
    }

    pub(crate) fn child_records(&self, provider: AgentProvider, session: &str) -> Vec<ChildRecord> {
        let children = self.children.lock();
        self.read_children(&children, provider, session)
    }

    pub(crate) fn update_child(
        &self,
        provider: AgentProvider,
        session: &str,
        pane: PaneId,
        update: impl FnOnce(&mut ChildRecord),
    ) {
        let mut children = self.children.lock();
        let mut records = self.read_children(&children, provider, session);
        let index = records
            .iter()
            .position(|record| record.pane == pane.0)
            .unwrap_or_else(|| {
                records.push(ChildRecord {
                    pane: pane.0,
                    ..ChildRecord::default()
                });
                records.len() - 1
            });
        update(&mut records[index]);
        match &self.journal {
            Some(journal) => {
                if let Err(error) = journal.save_children_for(provider, session, &records) {
                    log::warn!(target: "zz::agent", "could not save the chats {session} opened: {error}");
                }
            }
            None => {
                children.insert((provider, session.to_owned()), records);
            }
        }
    }

    fn read_children(
        &self,
        children: &BTreeMap<(AgentProvider, String), Vec<ChildRecord>>,
        provider: AgentProvider,
        session: &str,
    ) -> Vec<ChildRecord> {
        match &self.journal {
            Some(journal) => journal.children_for(provider, session).unwrap_or_else(|error| {
                log::warn!(target: "zz::agent", "could not read the chats {session} opened: {error}");
                Vec::new()
            }),
            None => children
                .get(&(provider, session.to_owned()))
                .cloned()
                .unwrap_or_default(),
        }
    }

    pub(crate) fn sends_since_user(&self, caller: PaneId, target: PaneId) -> u32 {
        self.sends
            .lock()
            .get(&(caller, target))
            .copied()
            .unwrap_or(0)
    }

    pub(crate) fn count_send(&self, caller: PaneId, target: PaneId) {
        *self.sends.lock().entry((caller, target)).or_insert(0) += 1;
    }

    pub(crate) fn open(&self, pane: PaneId, spec: AgentPaneSpec) -> bool {
        let _lifecycle = self.lifecycle.lock();
        if self.host.contains(pane) {
            return true;
        }
        let generation = self.generation.fetch_add(1, Ordering::Relaxed);
        self.fanout
            .open_lane(pane, generation, spec.provider, spec.resume_session.clone());
        if self.host.open(pane, generation, spec) {
            return true;
        }
        self.fanout.close_lane(pane);
        false
    }

    pub(crate) fn close(&self, pane: PaneId) {
        let _lifecycle = self.lifecycle.lock();
        let _ = self.host.close(pane);
        self.fanout.close_lane(pane);
    }

    pub(crate) fn close_and_wait(&self, pane: PaneId) {
        let (thread, generation) = {
            let _lifecycle = self.lifecycle.lock();
            (self.host.close(pane), self.pane_generation(pane))
        };
        if let Some(thread) = thread {
            let _ = thread.join();
            crate::agent::child::join_reapers();
        }
        let _lifecycle = self.lifecycle.lock();
        if self.pane_generation(pane) == generation {
            self.fanout.park_lane(pane);
        }
    }

    pub(crate) fn command(&self, pane: PaneId, command: HostCommand) -> bool {
        let _lifecycle = self.lifecycle.lock();
        match self.host.command(pane, command) {
            Ok(()) => true,
            Err(command) => self.fanout.reject_command(pane, command),
        }
    }

    pub(crate) fn prompt(&self, pane: PaneId, prompt: AgentPrompt) -> bool {
        self.prompt_with_waiter(pane, prompt, None)
    }

    pub(crate) fn prompt_with_waiter(
        &self,
        pane: PaneId,
        prompt: AgentPrompt,
        waiter: Option<AgentTurnWaiter>,
    ) -> bool {
        self.prompt_from(pane, prompt, waiter, PromptSource::Composer, None)
    }

    pub(crate) fn command_prompt(
        &self,
        pane: PaneId,
        prompt: AgentPrompt,
        waiter: Option<AgentTurnWaiter>,
        note: Option<String>,
    ) -> bool {
        self.prompt_from(pane, prompt, waiter, PromptSource::Command, note)
    }

    pub(crate) fn answer_question(
        &self,
        pane: PaneId,
        request: Option<u64>,
        text: &str,
    ) -> Result<u64, String> {
        let pending = self
            .host
            .snapshot_state(pane)
            .map(|state| state.pending_permissions)
            .unwrap_or_default();
        let (request_id, answers) = crate::agent::host::typed_answer(&pending, request, text)?;
        if self.command(
            pane,
            HostCommand::AnswerQuestion {
                request_id,
                answers,
            },
        ) {
            Ok(request_id)
        } else {
            Err("the chat is closed".to_owned())
        }
    }

    pub(crate) fn waiting_prompts(&self, pane: PaneId) -> usize {
        self.host
            .snapshot_state(pane)
            .map_or(0, |state| state.waiting_prompts)
    }

    fn prompt_from(
        &self,
        pane: PaneId,
        prompt: AgentPrompt,
        waiter: Option<AgentTurnWaiter>,
        source: PromptSource,
        note: Option<String>,
    ) -> bool {
        let _lifecycle = self.lifecycle.lock();
        self.sends.lock().retain(|(caller, _), _| *caller != pane);
        let queued = QueuedPrompt {
            prompt,
            waiter,
            source,
            note,
        };
        match self.host.command(pane, HostCommand::Prompt(queued)) {
            Ok(()) => true,
            Err(HostCommand::Prompt(mut queued)) => {
                queued.settle(Err(AgentTurnFailure::Reclaimed));
                source == PromptSource::Composer && self.fanout.reclaim_prompt(pane, queued.prompt)
            }
            Err(_) => unreachable!(),
        }
    }

    pub(crate) fn inject(&self, pane: PaneId, prompt: AgentPrompt) -> bool {
        let _lifecycle = self.lifecycle.lock();
        let queued = QueuedPrompt {
            prompt,
            waiter: None,
            source: PromptSource::Injected,
            note: None,
        };
        self.host.command(pane, HostCommand::Prompt(queued)).is_ok()
    }

    pub(crate) fn chat_text(&self, pane: PaneId, whole: bool) -> String {
        let lanes = self.fanout.lanes.lock();
        let Some(lane) = lanes.get(&pane) else {
            return String::new();
        };
        if !lane.chat.is_empty() {
            return crate::agent::capture::chat_text(lane.chat.events(), whole);
        }
        let updates = lane
            .ring
            .iter()
            .filter_map(|(_, encoded)| serde_json::from_slice::<AgentStreamItem>(encoded).ok())
            .filter_map(|item| match item.payload {
                AgentStreamPayload::Update { update } => Some(update),
                _ => None,
            })
            .collect::<Vec<_>>();
        let events = crate::agent::capture::stream_events(updates.iter());
        crate::agent::capture::chat_text(events.iter(), whole)
    }

    pub(crate) fn has_pending_permission(&self, pane: PaneId, request: u64) -> bool {
        self.host.snapshot_state(pane).is_some_and(|state| {
            state
                .pending_permissions
                .iter()
                .any(|pending| pending.request_id == request)
        })
    }

    pub(crate) fn join(&self, pane: PaneId, waiter: AgentTurnWaiter) -> bool {
        let _lifecycle = self.lifecycle.lock();
        self.host.command(pane, HostCommand::Join(waiter)).is_ok()
    }

    pub(crate) fn prewarm(&self) {
        if cfg!(test) {
            return;
        }
        self.host.prewarm();
    }

    pub(crate) fn reconfigure(&self, config: AgentSpawnConfig) {
        let _lifecycle = self.lifecycle.lock();
        self.host.reconfigure(config);
    }

    #[cfg(test)]
    pub(crate) fn spawn_config(&self) -> AgentSpawnConfig {
        self.host.config()
    }

    pub(crate) fn wire_state(&self, pane: PaneId) -> Option<AgentPaneWire> {
        self.fanout
            .published_state(pane)
            .or_else(|| self.host.snapshot_state(pane).map(|state| wire(&state)))
    }

    pub(crate) fn pane_generation(&self, pane: PaneId) -> Option<u64> {
        self.fanout
            .lanes
            .lock()
            .get(&pane)
            .map(|lane| lane.generation)
    }

    pub(crate) fn replay(&self, client: ClientId, pane: PaneId, from_seq: u64) {
        self.fanout.replay(client, pane, from_seq);
    }

    pub(crate) fn acknowledge_prompt_restore(
        &self,
        owner: ClientInstanceId,
        pane: PaneId,
        reclaim_id: u64,
    ) {
        let _lifecycle = self.lifecycle.lock();
        self.fanout
            .acknowledge_prompt_restore(owner, pane, reclaim_id);
    }

    pub(crate) fn shutdown(&self) {
        let lifecycle = self.lifecycle.lock();
        self.fanout.shutdown();
        drop(lifecycle);
        self.host.shutdown();
    }

    #[cfg(test)]
    pub(crate) fn set_runner_factory(&self, factory: crate::agent::host::PaneRunnerFactory) {
        self.host.set_runner_factory(factory);
    }

    #[cfg(test)]
    pub(crate) fn retained_items(&self, pane: PaneId) -> Vec<AgentStreamItem> {
        self.fanout
            .lanes
            .lock()
            .get(&pane)
            .map(|lane| {
                lane.ring
                    .iter()
                    .filter_map(|(_, encoded)| serde_json::from_slice(encoded).ok())
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Drop for AgentRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct PaneLane {
    generation: u64,
    provider: AgentProvider,
    next_seq: u64,
    opened_at: u64,
    evicted_seq: u64,
    batch: Vec<Vec<u8>>,
    batch_bytes: usize,
    batch_first_seq: u64,
    deadline: Option<Instant>,
    ring: VecDeque<(u64, Vec<u8>)>,
    chat: ChatLog,
    ring_bytes: usize,
    session_id: Option<String>,
    modes: String,
    config_options: String,
    ready: Option<AgentStreamPayload>,
    modes_value: Option<Value>,
    config_options_value: Option<Value>,
    turn_id: Option<u64>,
    reclaimed: VecDeque<ReclaimedPrompt>,
    reclaimed_bytes: usize,
    next_reclaim_id: u64,
    tool_calls: BTreeMap<String, AgentToolCall>,
    blobs: u64,
    fingerprint: Option<StateFingerprint>,
    state: Option<AgentPaneWire>,
    sync_next_state: bool,
    quota: QuotaWindows,
}

#[derive(Clone)]
struct ReclaimedPrompt {
    reclaim_id: u64,
    last_seq: u64,
    prompt: AgentPrompt,
}

fn user_facing_page(update: &Value) -> Option<String> {
    let input = update.get("rawInput")?;
    if input.pointer("/ArtifactMetadata/UserFacing")?.as_bool()? {
        let path = input
            .get("Path")
            .or_else(|| update.pointer("/locations/0/path"))?
            .as_str()?;
        zz_protocol::chat::is_page_file(path).then(|| path.to_owned())
    } else {
        None
    }
}

impl PaneLane {
    fn tool_call(&mut self, payload: &AgentStreamPayload) -> Option<AgentToolCall> {
        let AgentStreamPayload::Update { update } = payload else {
            if matches!(
                payload,
                AgentStreamPayload::TurnStarted { .. }
                    | AgentStreamPayload::SessionReset { .. }
                    | AgentStreamPayload::SessionSwitched { .. }
            ) {
                self.tool_calls.clear();
            }
            return None;
        };
        let update_kind = update.get("sessionUpdate")?.as_str()?;
        if !matches!(update_kind, "tool_call" | "tool_call_update") {
            return None;
        }
        let id = update.get("toolCallId")?.as_str()?;
        if update_kind == "tool_call" {
            self.tool_calls.remove(id);
        }
        let call = self
            .tool_calls
            .entry(id.to_owned())
            .or_insert_with(|| AgentToolCall {
                id: id.to_owned(),
                ..AgentToolCall::default()
            });
        let previous_status = call.status.clone();
        if let Some(title) = update.get("title").and_then(Value::as_str) {
            call.title = title
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(200)
                .collect();
        }
        if let Some(kind) = update.get("kind").and_then(Value::as_str) {
            kind.clone_into(&mut call.kind);
        }
        if let Some(status) = update.get("status").and_then(Value::as_str) {
            status.clone_into(&mut call.status);
        }
        if self.provider == AgentProvider::Antigravity
            && self.turn_id.is_some()
            && let Some(page) = user_facing_page(update)
        {
            call.page = Some(page);
        }
        (update_kind == "tool_call"
            || (call.status != previous_status
                && matches!(call.status.as_str(), "completed" | "failed")))
        .then(|| call.clone())
    }

    fn new(generation: u64, provider: AgentProvider, session_id: Option<String>) -> Self {
        Self {
            generation,
            provider,
            next_seq: 1,
            opened_at: 1,
            evicted_seq: 0,
            batch: Vec::new(),
            batch_bytes: 0,
            batch_first_seq: 0,
            deadline: None,
            ring: VecDeque::new(),
            chat: ChatLog::default(),
            ring_bytes: 0,
            session_id,
            modes: String::new(),
            config_options: String::new(),
            ready: None,
            modes_value: None,
            config_options_value: None,
            turn_id: None,
            reclaimed: VecDeque::new(),
            reclaimed_bytes: 0,
            next_reclaim_id: 1,
            tool_calls: BTreeMap::new(),
            blobs: 0,
            fingerprint: None,
            state: None,
            sync_next_state: false,
            quota: QuotaWindows::default(),
        }
    }

    fn push(&mut self, seq: u64, encoded: Vec<u8>) {
        if self.batch_bytes.saturating_add(encoded.len()) > MAX_AGENT_UPDATES_BYTES {
            self.batch.clear();
            self.batch_bytes = 0;
        }
        if self.batch.is_empty() {
            self.batch_first_seq = seq;
            self.deadline = Some(Instant::now() + BATCH_WINDOW);
        }
        self.batch.push(encoded.clone());
        self.batch_bytes = self.batch_bytes.saturating_add(encoded.len());
        self.push_ring(seq, encoded);
    }

    fn push_ring(&mut self, seq: u64, encoded: Vec<u8>) {
        self.ring_bytes = self.ring_bytes.saturating_add(encoded.len());
        self.ring.push_back((seq, encoded));
        while self.ring_bytes > MAX_REPLAY_RING_BYTES {
            let Some((seq, dropped)) = self.ring.pop_front() else {
                break;
            };
            self.ring_bytes = self.ring_bytes.saturating_sub(dropped.len());
            self.evicted_seq = seq;
        }
    }

    fn take_batch(&mut self) -> Vec<(u64, Vec<Vec<u8>>)> {
        self.deadline = None;
        self.batch_bytes = 0;
        let first_seq = self.batch_first_seq;
        split_frames(first_seq, std::mem::take(&mut self.batch))
    }
}

#[derive(PartialEq, Eq)]
struct StateFingerprint {
    phase: AgentConnectionPhase,
    queued: usize,
    permission: Option<u64>,
    session_id: Option<String>,
    error: Option<String>,
    auth_methods: usize,
    git: Option<zz_protocol::AgentGitSummary>,
    tasks: Vec<zz_protocol::AgentTaskWire>,
    title: Option<String>,
    blobs: u64,
}

struct AgentFanout {
    publisher: Weak<dyn AgentPublisher>,
    journal: Option<Arc<AgentJournal>>,
    lifecycle: Arc<Mutex<()>>,
    lanes: Mutex<BTreeMap<PaneId, PaneLane>>,
    parked: Mutex<BTreeMap<PaneId, (u64, u64)>>,
    wake: Condvar,
    stopped: AtomicBool,
    flusher: Mutex<Option<JoinHandle<()>>>,
}

impl AgentFanout {
    fn new(
        publisher: &Arc<dyn AgentPublisher>,
        journal: Option<Arc<AgentJournal>>,
        lifecycle: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            publisher: Arc::downgrade(publisher),
            journal,
            lifecycle,
            lanes: Mutex::new(BTreeMap::new()),
            parked: Mutex::new(BTreeMap::new()),
            wake: Condvar::new(),
            stopped: AtomicBool::new(false),
            flusher: Mutex::new(None),
        }
    }

    fn open_lane(
        self: &Arc<Self>,
        pane: PaneId,
        generation: u64,
        provider: AgentProvider,
        session_id: Option<String>,
    ) {
        let mut lane = PaneLane::new(generation, provider, session_id);
        let parked = self.parked.lock().remove(&pane);
        if let Some((next_seq, next_reclaim_id)) = parked {
            lane.next_seq = next_seq;
            lane.next_reclaim_id = next_reclaim_id;
            lane.opened_at = next_seq;
            lane.evicted_seq = next_seq.saturating_sub(1);
            lane.enqueue(pane, AgentStreamPayload::SessionReset { restoring: true });
            lane.enqueue(
                pane,
                AgentStreamPayload::Chat {
                    event: ChatEvent::Restart,
                },
            );
        }
        self.lanes.lock().insert(pane, lane);
        self.ensure_flusher();
        self.wake.notify_all();
    }

    fn close_lane(&self, pane: PaneId) {
        let lane = self.lanes.lock().remove(&pane);
        if let Some(lane) = lane {
            self.parked
                .lock()
                .insert(pane, (lane.next_seq, lane.next_reclaim_id));
        }
    }

    fn park_lane(&self, pane: PaneId) {
        let Some(mut lane) = self.lanes.lock().remove(&pane) else {
            return;
        };
        if let Some(publisher) = self.publisher.upgrade() {
            for (first_seq, items) in lane.take_batch() {
                publisher.publish_agent_updates(lane.generation, pane, first_seq, items, None);
            }
        }
        self.parked
            .lock()
            .insert(pane, (lane.next_seq, lane.next_reclaim_id));
    }

    fn published_state(&self, pane: PaneId) -> Option<AgentPaneWire> {
        self.lanes.lock().get(&pane)?.state.clone()
    }

    fn reclaim_prompt(&self, pane: PaneId, prompt: AgentPrompt) -> bool {
        let mut lanes = self.lanes.lock();
        let Some(lane) = lanes.get_mut(&pane) else {
            return false;
        };
        lane.enqueue_reclaimed(pane, vec![prompt]);
        drop(lanes);
        self.wake.notify_all();
        true
    }

    fn reject_command(&self, pane: PaneId, command: HostCommand) -> bool {
        let Some(publisher) = self.publisher.upgrade() else {
            return false;
        };
        let Some(generation) = self.lanes.lock().get(&pane).map(|lane| lane.generation) else {
            return false;
        };
        let payload = match command {
            HostCommand::Authenticate { .. } => AgentStreamPayload::AuthenticationFailed {
                message: "agent command queue is busy".to_owned(),
            },
            HostCommand::SetConfigOption { option_id, .. } => AgentStreamPayload::SettingFailed {
                option_id,
                message: "agent command queue is busy".to_owned(),
            },
            HostCommand::SetMode { .. } => AgentStreamPayload::SettingFailed {
                option_id: "legacy-session-mode".to_owned(),
                message: "agent command queue is busy".to_owned(),
            },
            HostCommand::ListSessions { client, .. } => {
                let payload = AgentStreamPayload::SessionListFailed {
                    client,
                    message: "agent command queue is busy".to_owned(),
                };
                let Some(result) = encode_reply(&payload) else {
                    return false;
                };
                publisher.send_agent_reply(
                    generation,
                    pane,
                    AgentRequestReply::Sessions { client, result },
                );
                return true;
            }
            HostCommand::DeleteSession { client, .. } => {
                let payload = AgentStreamPayload::SessionDeleteFailed {
                    client,
                    message: "agent command queue is busy".to_owned(),
                };
                let Some(result) = encode_reply(&payload) else {
                    return false;
                };
                publisher.send_agent_reply(
                    generation,
                    pane,
                    AgentRequestReply::Sessions { client, result },
                );
                return true;
            }
            HostCommand::NewSession { reply, .. } => {
                let message = "agent command queue is busy".to_owned();
                if let Some(reply) = reply {
                    reply.try_send(Err(message.clone()));
                }
                AgentStreamPayload::SessionSwitchFailed { message }
            }
            HostCommand::SwitchSession { .. } => AgentStreamPayload::SessionSwitchFailed {
                message: "agent command queue is busy".to_owned(),
            },
            HostCommand::AnswerQuestion { .. }
            | HostCommand::StopTask { .. }
            | HostCommand::Join(_)
            | HostCommand::RefreshGit => return false,
            HostCommand::Prompt(mut queued) if queued.source != PromptSource::Composer => {
                queued.settle(Err(AgentTurnFailure::Reclaimed));
                return false;
            }
            HostCommand::Prompt(mut queued) => {
                queued.settle(Err(AgentTurnFailure::Reclaimed));
                return self.reclaim_prompt(pane, queued.prompt);
            }
            HostCommand::Steer(prompt) => return self.reclaim_prompt(pane, prompt),
            HostCommand::Cancel | HostCommand::Unqueue | HostCommand::RespondPermission { .. } => {
                AgentStreamPayload::PaneFailed {
                    message: "agent control queue is busy".to_owned(),
                }
            }
        };
        let mut lanes = self.lanes.lock();
        let Some(lane) = lanes.get_mut(&pane) else {
            return false;
        };
        lane.enqueue(pane, payload);
        drop(lanes);
        self.wake.notify_all();
        true
    }

    fn accept(
        &self,
        pane: PaneId,
        generation: u64,
        state: &AgentPaneState,
        item: Option<AgentStreamItem>,
    ) {
        let _lifecycle = self.lifecycle.lock();
        if self.stopped.load(Ordering::Acquire) {
            return;
        }
        let Some(publisher) = self.publisher.upgrade() else {
            return;
        };
        let mut reply = None;
        let mut tool_call = None;
        let mut folder = None;
        let mut lanes = self.lanes.lock();
        let Some(lane) = lanes.get_mut(&pane) else {
            return;
        };
        if lane.generation != generation {
            if let Some(AgentStreamItem {
                payload: AgentStreamPayload::PromptsReclaimed { prompts },
                ..
            }) = item
            {
                lane.enqueue_reclaimed(pane, prompts);
                drop(lanes);
                self.wake.notify_all();
            }
            return;
        }
        if let Some(item) = item {
            tool_call = lane.tool_call(&item.payload);
            match &item.payload {
                AgentStreamPayload::Ready { .. } => {
                    lane.ready = Some(item.payload.clone());
                }
                AgentStreamPayload::SessionReady {
                    session_id,
                    modes,
                    config_options,
                } => {
                    if lane.session_id.as_deref() != Some(session_id) {
                        lane.turn_id = None;
                    }
                    lane.session_id = Some(session_id.clone());
                    lane.modes = blob(modes.as_ref());
                    lane.config_options = blob(config_options.as_ref());
                    lane.modes_value.clone_from(modes);
                    lane.config_options_value.clone_from(config_options);
                    lane.blobs = lane.blobs.saturating_add(1);
                }
                AgentStreamPayload::SessionSwitched {
                    session_id,
                    cwd,
                    modes,
                    config_options,
                    ..
                } => {
                    folder = Some(cwd.clone());
                    lane.turn_id = None;
                    lane.session_id = Some(session_id.clone());
                    lane.modes = blob(modes.as_ref());
                    lane.config_options = blob(config_options.as_ref());
                    lane.modes_value.clone_from(modes);
                    lane.config_options_value.clone_from(config_options);
                    lane.blobs = lane.blobs.saturating_add(1);
                }
                AgentStreamPayload::ConfigOptionsChanged { config_options, .. } => {
                    lane.config_options = blob(Some(config_options));
                    lane.config_options_value = Some(config_options.clone());
                    lane.blobs = lane.blobs.saturating_add(1);
                }
                AgentStreamPayload::ModeChanged { mode_id } => {
                    let changed = lane
                        .modes_value
                        .as_mut()
                        .and_then(Value::as_object_mut)
                        .map(|modes| {
                            modes
                                .insert("currentModeId".to_owned(), Value::String(mode_id.clone()));
                        })
                        .is_some();
                    if changed {
                        lane.modes = blob(lane.modes_value.as_ref());
                        lane.blobs = lane.blobs.saturating_add(1);
                    }
                }
                AgentStreamPayload::Update { update } => {
                    match serde_json::from_value::<SessionUpdate>(update.clone()) {
                        Ok(SessionUpdate::CurrentModeUpdate(update)) => {
                            let changed = lane
                                .modes_value
                                .as_mut()
                                .and_then(Value::as_object_mut)
                                .map(|modes| {
                                    modes.insert(
                                        "currentModeId".to_owned(),
                                        Value::String(update.current_mode_id.0.to_string()),
                                    );
                                })
                                .is_some();
                            if changed {
                                lane.modes = blob(lane.modes_value.as_ref());
                                lane.blobs = lane.blobs.saturating_add(1);
                            }
                        }
                        Ok(SessionUpdate::ConfigOptionUpdate(update)) => {
                            if let Ok(value) = serde_json::to_value(update.config_options) {
                                lane.config_options = blob(Some(&value));
                                lane.config_options_value = Some(value);
                                lane.blobs = lane.blobs.saturating_add(1);
                            }
                        }
                        _ => {}
                    }
                }
                AgentStreamPayload::SessionReset { .. } => lane.turn_id = None,
                AgentStreamPayload::Chat { event } => lane.chat.push(event),
                AgentStreamPayload::TurnStarted { turn_id } => lane.turn_id = Some(*turn_id),
                AgentStreamPayload::PromptFinished { turn_id, .. }
                    if lane.turn_id == Some(*turn_id) =>
                {
                    lane.turn_id = None;
                }
                AgentStreamPayload::SessionsListed { client, .. }
                | AgentStreamPayload::SessionListFailed { client, .. }
                | AgentStreamPayload::SessionDeleted { client, .. }
                | AgentStreamPayload::SessionDeleteFailed { client, .. } => {
                    let result = encode_reply(&item.payload).or_else(|| {
                        encode_reply(&AgentStreamPayload::SessionListFailed {
                            client: *client,
                            message: "agent returned too much session history".to_owned(),
                        })
                    });
                    reply = result.map(|result| AgentRequestReply::Sessions {
                        client: *client,
                        result,
                    });
                }
                _ => {}
            }
            if reply.is_none() {
                match item.payload {
                    AgentStreamPayload::PromptsReclaimed { prompts } => {
                        lane.enqueue_reclaimed(pane, prompts);
                    }
                    payload => lane.enqueue(pane, payload),
                }
            }
        }
        let quota = (lane.quota != state.quota).then(|| {
            lane.quota = state.quota;
            state.quota
        });
        let next_state = lane.refresh_state(state);
        if lane.sync_next_state
            && let Some(state) = next_state.as_ref()
        {
            lane.enqueue(
                pane,
                AgentStreamPayload::StateSynced {
                    state: state.clone(),
                },
            );
            lane.sync_next_state = false;
        }
        drop(lanes);
        self.wake.notify_all();
        if let Some(call) = tool_call {
            publisher.publish_agent_tool_call(generation, pane, call);
        }
        if let Some(reply) = reply {
            publisher.send_agent_reply(generation, pane, reply);
        }
        if let Some(cwd) = folder {
            publisher.publish_agent_folder(generation, pane, cwd);
        }
        if let Some(quota) = quota {
            publisher.publish_agent_quota(generation, pane, quota);
        }
        if let Some(next_state) = next_state {
            publisher.publish_agent_state(generation, pane, next_state);
        }
    }

    fn replay(&self, client: ClientId, pane: PaneId, from_seq: u64) {
        let Some(publisher) = self.publisher.upgrade() else {
            return;
        };
        let mut lanes = self.lanes.lock();
        let Some(lane) = lanes.get_mut(&pane) else {
            return;
        };
        let generation = lane.generation;
        let from = from_seq.max(1);
        if from_seq >= lane.evicted_seq {
            let items = lane
                .ring
                .iter()
                .filter(|(seq, _)| *seq >= from)
                .map(|(seq, encoded)| (*seq, encoded.clone()))
                .collect::<Vec<_>>();
            let Some(first_seq) = items.first().map(|(seq, _)| *seq) else {
                publisher.send_agent_replay(generation, client, pane, Vec::new());
                return;
            };
            let frames = split_frames(
                first_seq,
                items.into_iter().map(|(_, encoded)| encoded).collect(),
            );
            for (first_seq, items) in lane.take_batch() {
                publisher.publish_agent_updates(generation, pane, first_seq, items, None);
            }
            publisher.send_agent_replay(generation, client, pane, frames);
            return;
        }
        let intact = lane.evicted_seq.saturating_add(1) == lane.opened_at;
        let replay = lane
            .session_id
            .as_deref()
            .zip(self.journal.as_ref())
            .filter(|_| !intact)
            .filter(|(session_id, journal)| journal.is_complete_for(lane.provider, session_id))
            .and_then(|(session_id, journal)| journal.replay_for(lane.provider, session_id).ok());
        let Some(replay) = replay else {
            for (first_seq, items) in lane.take_batch() {
                publisher.publish_agent_updates(generation, pane, first_seq, items, None);
            }
            if !intact {
                log::warn!(
                    target: "zz::agent",
                    "rebuilding pane {pane} for client {client} from memory alone: \
                     asked for {from_seq}, memory starts at {}, and the journal cannot vouch for the rest",
                    lane.evicted_seq.saturating_add(1),
                );
                let frames = lane.rebuild(pane, from_seq);
                publisher.send_agent_replay(generation, client, pane, frames);
                drop(lanes);
                self.wake.notify_all();
                return;
            }
            let first_seq = lane.evicted_seq;
            let reset = AgentStreamItem {
                seq: first_seq,
                payload: AgentStreamPayload::SessionReset { restoring: true },
            };
            let Ok(reset) = serde_json::to_vec(&reset) else {
                return;
            };
            let mut items = vec![reset];
            items.extend(
                lane.ring
                    .iter()
                    .filter(|(seq, _)| *seq > first_seq)
                    .map(|(_, encoded)| encoded.clone()),
            );
            publisher.send_agent_replay(generation, client, pane, split_frames(first_seq, items));
            return;
        };
        for (first_seq, items) in lane.take_batch() {
            publisher.publish_agent_updates(generation, pane, first_seq, items, None);
        }
        log::info!(
            target: "zz::agent",
            "replaying pane {pane} for client {client} out of the journal: \
             asked for {from_seq}, memory starts at {}, {} journalled updates",
            lane.evicted_seq.saturating_add(1),
            replay.len(),
        );
        let mut payloads = vec![AgentStreamPayload::SessionReset { restoring: true }];
        payloads.extend(lane.ready.clone());
        payloads.extend(replay.into_iter().filter_map(|(_, entry)| match entry {
            JournalEntry::Update(update)
                if update["sessionUpdate"] == "config_option_update" || is_turn_mark(&update) =>
            {
                None
            }
            JournalEntry::Update(update) => Some(AgentStreamPayload::Update { update }),
        }));
        payloads.extend(lane.recovery_metadata());
        payloads.extend(lane.chat_recovery());
        let frames = lane
            .skipped(from_seq, payloads)
            .unwrap_or_else(|| lane.rebuild(pane, from_seq));
        publisher.send_agent_replay(generation, client, pane, frames);
        drop(lanes);
        self.wake.notify_all();
    }

    fn acknowledge_prompt_restore(&self, owner: ClientInstanceId, pane: PaneId, reclaim_id: u64) {
        let mut lanes = self.lanes.lock();
        if let Some(lane) = lanes.get_mut(&pane) {
            lane.acknowledge_reclaimed(owner, reclaim_id);
        }
    }

    fn ensure_flusher(self: &Arc<Self>) {
        let mut flusher = self.flusher.lock();
        if flusher.is_some() {
            return;
        }
        let fanout = Arc::downgrade(self);
        *flusher = std::thread::Builder::new()
            .name("zz-agent-flush".to_owned())
            .spawn(move || run_flusher(&fanout))
            .map_err(|error| {
                log::error!(target: "zz::agent", "could not start the agent flush thread: {error}");
            })
            .ok();
    }

    fn shutdown(&self) {
        let lanes = self.lanes.lock();
        self.stopped.store(true, Ordering::Release);
        self.wake.notify_all();
        drop(lanes);
        if let Some(flusher) = self.flusher.lock().take() {
            let _ = flusher.join();
        }
        self.lanes.lock().clear();
    }
}

impl PaneLane {
    fn enqueue_reclaimed(&mut self, pane: PaneId, prompts: Vec<AgentPrompt>) {
        for prompt in prompts {
            let reclaim_id = self.next_reclaim_id;
            self.next_reclaim_id = self.next_reclaim_id.saturating_add(1);
            let Some(last_seq) = self.enqueue_with_seq(
                pane,
                AgentStreamPayload::PromptsRestored {
                    reclaim_id,
                    prompts: vec![prompt.clone()],
                },
            ) else {
                continue;
            };
            self.reclaimed_bytes = self.reclaimed_bytes.saturating_add(prompt.byte_len());
            self.reclaimed.push_back(ReclaimedPrompt {
                reclaim_id,
                last_seq,
                prompt,
            });
            while self.reclaimed_bytes > MAX_AGENT_PROMPT_BYTES
                || self.reclaimed.len() > MAX_RECLAIMED_PROMPTS
            {
                let Some(evicted) = self.reclaimed.pop_front() else {
                    break;
                };
                self.reclaimed_bytes = self
                    .reclaimed_bytes
                    .saturating_sub(evicted.prompt.byte_len());
            }
        }
    }

    fn acknowledge_reclaimed(&mut self, owner: ClientInstanceId, reclaim_id: u64) {
        let Some(index) = self.reclaimed.iter().position(|reclaimed| {
            reclaimed.reclaim_id == reclaim_id && reclaimed.prompt.owner == owner
        }) else {
            return;
        };
        let Some(reclaimed) = self.reclaimed.remove(index) else {
            return;
        };
        self.reclaimed_bytes = self
            .reclaimed_bytes
            .saturating_sub(reclaimed.prompt.byte_len());
        while self
            .ring
            .front()
            .is_some_and(|(seq, _)| *seq <= reclaimed.last_seq)
        {
            let Some((seq, encoded)) = self.ring.pop_front() else {
                break;
            };
            self.ring_bytes = self.ring_bytes.saturating_sub(encoded.len());
            self.evicted_seq = self.evicted_seq.max(seq);
        }
        self.evicted_seq = self.evicted_seq.max(reclaimed.last_seq);
    }

    fn enqueue(&mut self, pane: PaneId, payload: AgentStreamPayload) {
        _ = self.enqueue_with_seq(pane, payload);
    }

    fn enqueue_with_seq(&mut self, pane: PaneId, payload: AgentStreamPayload) -> Option<u64> {
        let (seq, encoded) = self.stamp(pane, payload)?;
        self.push(seq, encoded);
        Some(seq)
    }

    fn recovery_metadata(&self) -> Vec<AgentStreamPayload> {
        let mut payloads = Vec::new();
        if let Some(session_id) = self.session_id.clone() {
            payloads.push(AgentStreamPayload::SessionReady {
                session_id,
                modes: self.modes_value.clone(),
                config_options: self.config_options_value.clone(),
            });
        }
        if let Some(turn_id) = self.turn_id {
            payloads.push(AgentStreamPayload::TurnStarted { turn_id });
        }
        payloads.extend(self.reclaimed.iter().map(|reclaimed| {
            AgentStreamPayload::PromptsRestored {
                reclaim_id: reclaimed.reclaim_id,
                prompts: vec![reclaimed.prompt.clone()],
            }
        }));
        payloads.extend(
            self.state
                .clone()
                .map(|state| AgentStreamPayload::StateSynced { state }),
        );
        payloads
    }

    fn chat_recovery(&self) -> Vec<AgentStreamPayload> {
        if self.chat.is_empty() {
            return Vec::new();
        }
        std::iter::once(ChatEvent::Restart)
            .chain(self.chat.events().cloned())
            .map(|event| AgentStreamPayload::Chat { event })
            .collect()
    }

    fn rebuild(&mut self, pane: PaneId, from_seq: u64) -> Vec<(u64, Vec<Vec<u8>>)> {
        let events = self.chat.events().cloned().collect::<Vec<_>>();
        let spine = self.chat.in_effect();
        let chat = |at: usize| AgentStreamPayload::Chat {
            event: events[at].clone(),
        };
        let next_seq = self.next_seq;
        let size = |payload: AgentStreamPayload| {
            serde_json::to_vec(&AgentStreamItem {
                seq: next_seq,
                payload,
            })
            .map_or(usize::MAX, |encoded| encoded.len())
        };
        let mut payloads = vec![AgentStreamPayload::SessionReset { restoring: true }];
        payloads.extend(self.ready.clone());
        payloads.extend(self.recovery_metadata());
        let restart = AgentStreamPayload::Chat {
            event: ChatEvent::Restart,
        };
        let room = usize::try_from(next_seq.saturating_sub(from_seq).saturating_sub(1))
            .unwrap_or(usize::MAX);
        let mut count = payloads.len() + 1 + spine.len();
        let mut bytes = payloads
            .iter()
            .cloned()
            .chain([restart.clone()])
            .chain(spine.iter().map(|at| chat(*at)))
            .map(size)
            .fold(0, usize::saturating_add);
        let fits = bytes <= MAX_REBUILD_BYTES;
        let mut cut = events.len();
        while fits && cut > 0 && count < room {
            if spine.binary_search(&(cut - 1)).is_ok() {
                cut -= 1;
                continue;
            }
            let next = size(chat(cut - 1));
            if bytes.saturating_add(next) > MAX_REBUILD_BYTES {
                break;
            }
            count += 1;
            bytes = bytes.saturating_add(next);
            cut -= 1;
        }
        if fits {
            let kept = spine.iter().filter(|at| **at < cut).count();
            if cut > kept {
                log::info!(
                    target: "zz::agent",
                    "rebuilt pane {pane} on its newest {} chat events and the {kept} older ones still in effect, leaving out {}",
                    events.len() - cut,
                    cut - kept,
                );
            }
            payloads.push(restart);
            payloads.extend(spine.into_iter().filter(|at| *at < cut).map(chat));
            payloads.extend((cut..events.len()).map(chat));
        } else {
            log::warn!(
                target: "zz::agent",
                "rebuilt pane {pane} without its chat: the events still in effect take {bytes} bytes, over {MAX_REBUILD_BYTES}",
            );
        }
        self.widen(pane, from_seq, payloads.len(), &events);
        let first_seq = self
            .next_seq
            .saturating_sub(u64::try_from(payloads.len()).unwrap_or(u64::MAX));
        let items = payloads
            .into_iter()
            .zip(first_seq..)
            .map(|(payload, seq)| serde_json::to_vec(&AgentStreamItem { seq, payload }))
            .collect::<Result<Vec<_>, _>>();
        match items {
            Ok(items) => split_frames(first_seq, items),
            Err(error) => {
                log::error!(target: "zz::agent", "could not encode the rebuild of pane {pane}: {error}");
                Vec::new()
            }
        }
    }

    fn widen(&mut self, pane: PaneId, from_seq: u64, needed: usize, events: &[ChatEvent]) {
        let needed = u64::try_from(needed).unwrap_or(u64::MAX);
        let room = |lane: &Self| lane.next_seq.saturating_sub(from_seq).saturating_sub(1);
        if room(self) >= needed {
            return;
        }
        let last = |kind: fn(&ChatEvent) -> bool| events.iter().rev().find(|event| kind(event));
        let event = last(|event| matches!(event, ChatEvent::QueueChanged { .. }))
            .or_else(|| last(|event| matches!(event, ChatEvent::UsageChanged { .. })))
            .cloned()
            .unwrap_or(ChatEvent::QueueChanged {
                prompts: Vec::new(),
            });
        let deficit = needed - room(self);
        log::info!(
            target: "zz::agent",
            "widening the rebuild of pane {pane} by {deficit} numbers with a repeat of {}",
            if matches!(event, ChatEvent::UsageChanged { .. }) { "the usage" } else { "the queue" },
        );
        while room(self) < needed {
            let payload = AgentStreamPayload::Chat {
                event: event.clone(),
            };
            if self.enqueue_with_seq(pane, payload).is_none() {
                break;
            }
        }
    }

    fn skipped(
        &self,
        from_seq: u64,
        payloads: Vec<AgentStreamPayload>,
    ) -> Option<Vec<(u64, Vec<Vec<u8>>)>> {
        let count = u64::try_from(payloads.len()).ok()?;
        if count > self.next_seq.saturating_sub(from_seq).saturating_sub(1) {
            return None;
        }
        let first_seq = self.next_seq - count;
        let items = payloads
            .into_iter()
            .zip(first_seq..)
            .map(|(payload, seq)| serde_json::to_vec(&AgentStreamItem { seq, payload }))
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        (items.iter().map(Vec::len).sum::<usize>() <= MAX_REBUILD_BYTES)
            .then(|| split_frames(first_seq, items))
    }

    fn stamp(&mut self, pane: PaneId, payload: AgentStreamPayload) -> Option<(u64, Vec<u8>)> {
        let item = AgentStreamItem {
            seq: self.next_seq,
            payload,
        };
        let encoded = match serde_json::to_vec(&item) {
            Ok(encoded) => encoded,
            Err(error) => {
                log::error!(target: "zz::agent", "could not encode a stream item for pane {pane}: {error}");
                return None;
            }
        };
        if encoded.len() > MAX_AGENT_UPDATES_BYTES {
            log::warn!(
                target: "zz::agent",
                "dropping a {} byte agent update for pane {pane}: one item may not exceed {MAX_AGENT_UPDATES_BYTES} bytes",
                encoded.len(),
            );
            return None;
        }
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        Some((seq, encoded))
    }

    fn refresh_state(&mut self, state: &AgentPaneState) -> Option<AgentPaneWire> {
        let fingerprint = StateFingerprint {
            phase: state.phase,
            queued: state.queued_prompts,
            permission: state
                .pending_permissions
                .first()
                .map(|permission| permission.request_id),
            session_id: state.session_id.clone(),
            error: state.error.clone(),
            auth_methods: state.auth_methods.len(),
            git: state.git.clone(),
            tasks: state.tasks.clone(),
            title: state.title.clone(),
            blobs: self.blobs,
        };
        if self.fingerprint.as_ref() == Some(&fingerprint) {
            return None;
        }
        self.fingerprint = Some(fingerprint);
        let mut next = wire(state);
        next.modes.clone_from(&self.modes);
        next.config_options.clone_from(&self.config_options);
        if let Err(error) = next.validate() {
            log::warn!(target: "zz::agent", "trimming the pane state for {:?}: {error}", state.session_id);
            next.modes.clear();
            next.config_options.clear();
            next.auth_methods.clear();
            next.pending_permission = None;
            next.git = None;
            next.validate().ok()?;
        }
        self.state = Some(next.clone());
        Some(next)
    }
}

fn run_flusher(fanout: &Weak<AgentFanout>) {
    loop {
        let Some(fanout) = fanout.upgrade() else {
            return;
        };
        let mut lanes = fanout.lanes.lock();
        if fanout.stopped.load(Ordering::Acquire) {
            return;
        }
        let now = Instant::now();
        let mut due = Vec::new();
        let mut next: Option<Instant> = None;
        for (pane, lane) in lanes.iter_mut() {
            let Some(deadline) = lane.deadline else {
                continue;
            };
            if deadline > now {
                next = Some(next.map_or(deadline, |current| current.min(deadline)));
                continue;
            }
            for (first_seq, items) in lane.take_batch() {
                due.push((*pane, lane.generation, first_seq, items));
            }
        }
        if due.is_empty() {
            match next {
                Some(deadline) => {
                    fanout
                        .wake
                        .wait_for(&mut lanes, deadline.saturating_duration_since(now));
                }
                None => fanout.wake.wait(&mut lanes),
            }
            continue;
        }
        let Some(publisher) = fanout.publisher.upgrade() else {
            return;
        };
        for (pane, generation, first_seq, items) in due {
            publisher.publish_agent_updates(generation, pane, first_seq, items, None);
        }
    }
}

fn split_frames(first_seq: u64, items: Vec<Vec<u8>>) -> Vec<(u64, Vec<Vec<u8>>)> {
    let mut frames = Vec::new();
    let mut current: Vec<Vec<u8>> = Vec::new();
    let mut current_bytes = 0usize;
    let mut seq = first_seq;
    let mut start = first_seq;
    for item in items {
        if !current.is_empty() && current_bytes.saturating_add(item.len()) > MAX_AGENT_FRAME_BYTES {
            frames.push((start, std::mem::take(&mut current)));
            start = seq;
            current_bytes = 0;
        }
        current_bytes = current_bytes.saturating_add(item.len());
        current.push(item);
        seq = seq.saturating_add(1);
    }
    if !current.is_empty() {
        frames.push((start, current));
    }
    frames
}

fn blob(value: Option<&Value>) -> String {
    value.map(ToString::to_string).unwrap_or_default()
}

fn encode_reply(payload: &AgentStreamPayload) -> Option<String> {
    let encoded = serde_json::to_string(payload)
        .map_err(
            |error| log::error!(target: "zz::agent", "could not encode an agent reply: {error}"),
        )
        .ok()?;
    if encoded.len() > MAX_AGENT_RESULT_BYTES {
        log::warn!(target: "zz::agent", "dropping an oversized {} byte agent reply", encoded.len());
        return None;
    }
    Some(encoded)
}

fn wire(state: &AgentPaneState) -> AgentPaneWire {
    let phase = match state.phase {
        AgentConnectionPhase::Starting | AgentConnectionPhase::Restoring => {
            zz_protocol::AgentConnectionPhase::Starting
        }
        AgentConnectionPhase::Ready => zz_protocol::AgentConnectionPhase::Ready,
        AgentConnectionPhase::Running | AgentConnectionPhase::Cancelling => {
            if state.pending_permissions.is_empty() {
                zz_protocol::AgentConnectionPhase::Running
            } else {
                zz_protocol::AgentConnectionPhase::AwaitingPermission
            }
        }
        AgentConnectionPhase::Failed | AgentConnectionPhase::Disconnected => {
            zz_protocol::AgentConnectionPhase::Failed {
                message: state
                    .error
                    .clone()
                    .unwrap_or_else(|| "the agent disconnected".to_owned()),
            }
        }
    };
    AgentPaneWire {
        phase,
        queued_prompts: u32::try_from(state.queued_prompts).unwrap_or(u32::MAX),
        session_id: state.session_id.clone(),
        title: state.title.clone(),
        error: state.error.clone(),
        auth_methods: serde_json::to_string(&state.auth_methods).unwrap_or_default(),
        config_options: String::new(),
        modes: String::new(),
        pending_permission: state.pending_permissions.first().map(|permission| {
            let mut payload = serde_json::json!({
                "toolCall": permission.tool_call,
                "options": permission.options,
            });
            if !permission.questions.is_empty() {
                payload["questions"] =
                    serde_json::to_value(&permission.questions).unwrap_or_default();
            }
            AgentPermissionWire {
                request_id: permission.request_id,
                payload: payload.to_string(),
            }
        }),
        git: state.git.clone(),
        tasks: state.tasks.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::mpsc::{self, Receiver, Sender},
    };

    use serde_json::json;

    use super::*;
    use crate::agent::{
        fixture::{Behavior, fixture_runner},
        stream::{AgentImage, AgentPromptOutcome, AgentSessionCapabilities, AgentSessionSummary},
    };

    #[test]
    fn only_a_page_marked_user_facing_counts_as_an_agents_page() {
        let write = |user_facing: bool, path: &str| {
            json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": "call_1",
                "status": "completed",
                "kind": "edit",
                "locations": [{ "path": path }],
                "rawInput": {
                    "ArtifactMetadata": { "RequestFeedback": false, "UserFacing": user_facing },
                    "Overwrite": true,
                    "Path": path,
                },
            })
        };
        let page = "/home/me/.gemini/antigravity-cli/brain/s1/button-comparison.html";
        assert_eq!(user_facing_page(&write(true, page)).as_deref(), Some(page));
        assert_eq!(user_facing_page(&write(false, page)), None);
        assert_eq!(
            user_facing_page(&write(true, "/home/me/brain/s1/plan.md")),
            None
        );
        assert_eq!(
            user_facing_page(&json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": "call_2",
                "kind": "edit",
                "locations": [{ "path": "/work/site/index.html" }],
                "rawInput": { "Path": "/work/site/index.html" },
            })),
            None
        );
    }

    const DEADLINE: Duration = Duration::from_secs(5);

    #[derive(Default)]
    struct Recorder {
        broadcast: Mutex<Vec<(PaneId, u64, Vec<Vec<u8>>, Option<ClientId>)>>,
        direct: Mutex<Vec<(ClientId, PaneId, u64, Vec<Vec<u8>>)>>,
        answered: Mutex<Vec<(ClientId, PaneId, usize)>>,
        states: Mutex<Vec<(PaneId, AgentPaneWire)>>,
        replies: Mutex<Vec<(ClientId, PaneId, u64, String)>>,
        update_gate: Mutex<Option<(Sender<()>, Receiver<()>)>>,
    }

    impl Recorder {
        fn delivered(&self) -> usize {
            let broadcast = self
                .broadcast
                .lock()
                .iter()
                .map(|(_, _, items, _)| items.len())
                .sum::<usize>();
            let direct = self
                .direct
                .lock()
                .iter()
                .map(|(_, _, _, items)| items.len())
                .sum::<usize>();
            broadcast + direct
        }

        fn payloads(&self) -> Vec<AgentStreamItem> {
            let mut items = self
                .broadcast
                .lock()
                .iter()
                .flat_map(|(_, _, items, _)| items.clone())
                .collect::<Vec<_>>();
            items.extend(
                self.direct
                    .lock()
                    .iter()
                    .flat_map(|(_, _, _, items)| items.clone()),
            );
            items
                .iter()
                .map(|item| serde_json::from_slice(item).expect("decode stream item"))
                .collect()
        }

        fn wait_for_items(&self, count: usize) -> Vec<AgentStreamItem> {
            let deadline = Instant::now() + DEADLINE;
            while Instant::now() < deadline {
                if self.delivered() >= count {
                    return self.payloads();
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            panic!(
                "timed out waiting for {count} items, saw {}",
                self.delivered()
            );
        }
    }

    impl AgentPublisher for Recorder {
        fn publish_agent_updates(
            &self,
            _generation: u64,
            pane: PaneId,
            first_seq: u64,
            items: Vec<Vec<u8>>,
            also: Option<ClientId>,
        ) {
            if let Some((entered, release)) = self.update_gate.lock().take() {
                let _ = entered.send(());
                let _ = release.recv();
            }
            self.broadcast.lock().push((pane, first_seq, items, also));
        }

        fn send_agent_replay(
            &self,
            _generation: u64,
            client: ClientId,
            pane: PaneId,
            frames: Vec<(u64, Vec<Vec<u8>>)>,
        ) {
            self.answered.lock().push((client, pane, frames.len()));
            self.direct.lock().extend(
                frames
                    .into_iter()
                    .map(|(first_seq, items)| (client, pane, first_seq, items)),
            );
        }

        fn publish_agent_state(&self, _generation: u64, pane: PaneId, state: AgentPaneWire) {
            self.states.lock().push((pane, state));
        }

        fn publish_agent_folder(&self, _generation: u64, _pane: PaneId, _cwd: PathBuf) {}

        fn publish_agent_quota(&self, _generation: u64, _pane: PaneId, _quota: QuotaWindows) {}

        fn publish_agent_tool_call(&self, _generation: u64, _pane: PaneId, _call: AgentToolCall) {}

        fn send_agent_reply(&self, _generation: u64, pane: PaneId, reply: AgentRequestReply) {
            let AgentRequestReply::Sessions { client, result } = reply;
            let request_id = 0;
            self.replies.lock().push((client, pane, request_id, result));
        }
    }

    struct Fixture {
        fanout: Arc<AgentFanout>,
        recorder: Arc<Recorder>,
        pane: PaneId,
    }

    impl Fixture {
        fn open(journal: Option<Arc<AgentJournal>>, session_id: Option<&str>) -> Self {
            let recorder = Arc::new(Recorder::default());
            let publisher: Arc<dyn AgentPublisher> = Arc::<Recorder>::clone(&recorder);
            let fanout = Arc::new(AgentFanout::new(
                &publisher,
                journal,
                Arc::new(Mutex::new(())),
            ));
            let pane = PaneId(4);
            fanout.open_lane(
                pane,
                1,
                AgentProvider::Codex,
                session_id.map(ToOwned::to_owned),
            );
            Self {
                fanout,
                recorder,
                pane,
            }
        }

        fn accept(&self, payload: AgentStreamPayload) {
            self.accept_generation(1, payload);
        }

        fn accept_generation(&self, generation: u64, payload: AgentStreamPayload) {
            self.fanout.accept(
                self.pane,
                generation,
                &state(AgentConnectionPhase::Running),
                Some(AgentStreamItem { seq: 0, payload }),
            );
        }

        fn chunk(&self, text: &str) {
            self.accept(AgentStreamPayload::Update {
                update: json!({ "sessionUpdate": "agent_message_chunk", "content": { "text": text } }),
            });
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.fanout.shutdown();
        }
    }

    fn state(phase: AgentConnectionPhase) -> AgentPaneState {
        let mut state = AgentPaneState::for_test();
        state.phase = phase;
        state
    }

    fn text_of(item: &AgentStreamItem) -> Option<String> {
        let AgentStreamPayload::Update { update } = &item.payload else {
            return None;
        };
        update
            .get("content")?
            .get("text")?
            .as_str()
            .map(ToOwned::to_owned)
    }

    #[test]
    fn a_window_of_items_leaves_as_one_frame_numbered_from_the_first() {
        let fixture = Fixture::open(None, Some("s-1"));
        for text in ["a", "b", "c"] {
            fixture.chunk(text);
        }
        let items = fixture.recorder.wait_for_items(3);

        assert_eq!(
            items.iter().map(|item| item.seq).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            items.iter().filter_map(text_of).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        let broadcast = fixture.recorder.broadcast.lock();
        assert_eq!(broadcast.len(), 1, "one window, one frame: {broadcast:?}");
        assert_eq!(broadcast[0].1, 1);
    }

    #[test]
    fn a_flush_settles_and_drains_before_replay_admission() {
        let fixture = Fixture::open(None, Some("s-1"));
        let (entered_send, entered_recv) = mpsc::channel();
        let (release_send, release_recv) = mpsc::channel();
        *fixture.recorder.update_gate.lock() = Some((entered_send, release_recv));

        fixture.chunk("a");
        entered_recv
            .recv_timeout(DEADLINE)
            .expect("flusher should enter publication");
        assert!(fixture.fanout.lanes.try_lock().is_none());

        release_send.send(()).expect("release flusher");
        let deadline = Instant::now() + DEADLINE;
        let settled = loop {
            if fixture.fanout.lanes.try_lock().is_some() {
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            std::thread::yield_now();
        };
        assert!(settled, "flusher should release the lane");
        fixture.chunk("b");
        assert!(
            fixture
                .fanout
                .lanes
                .lock()
                .get(&fixture.pane)
                .is_some_and(|lane| !lane.batch.is_empty())
        );
        fixture.fanout.replay(ClientId(8), fixture.pane, 0);
        assert!(
            fixture
                .fanout
                .lanes
                .lock()
                .get(&fixture.pane)
                .is_some_and(|lane| lane.batch.is_empty())
        );
        assert_eq!(fixture.recorder.direct.lock().len(), 1);
    }

    #[test]
    fn an_unflushed_batch_stays_inside_its_byte_bound() {
        let mut lane = PaneLane::new(1, AgentProvider::Codex, None);
        let item = vec![b'x'; MAX_AGENT_RESULT_BYTES];
        for seq in 1..=32 {
            lane.push(seq, item.clone());
        }
        assert!(lane.batch_bytes <= MAX_AGENT_UPDATES_BYTES);
        assert!(lane.ring_bytes <= MAX_REPLAY_RING_BYTES);
        assert!(lane.batch.len() <= MAX_AGENT_UPDATES_BYTES / MAX_AGENT_RESULT_BYTES);
    }

    #[test]
    fn zero_byte_reclaimed_prompts_stay_inside_the_count_bound() {
        let mut lane = PaneLane::new(1, AgentProvider::Codex, None);
        for _ in 0..MAX_RECLAIMED_PROMPTS + 8 {
            lane.enqueue_reclaimed(
                PaneId(1),
                vec![AgentPrompt {
                    owner: ClientInstanceId::default(),
                    text: String::new(),
                    images: vec![AgentImage {
                        format: "image/png".to_owned(),
                        data: Vec::new(),
                    }],
                }],
            );
        }

        assert_eq!(lane.reclaimed.len(), MAX_RECLAIMED_PROMPTS);
        assert_eq!(lane.reclaimed_bytes, 0);
    }

    #[test]
    fn only_the_prompt_owner_can_retire_a_restored_prompt() {
        let mut lane = PaneLane::new(1, AgentProvider::Codex, None);
        lane.enqueue_reclaimed(
            PaneId(1),
            vec![AgentPrompt {
                owner: ClientInstanceId(7),
                text: "keep me".to_owned(),
                images: Vec::new(),
            }],
        );
        let reclaim_id = lane.reclaimed[0].reclaim_id;
        let last_seq = lane.reclaimed[0].last_seq;

        lane.acknowledge_reclaimed(ClientInstanceId(8), reclaim_id);
        assert_eq!(lane.reclaimed.len(), 1);

        lane.acknowledge_reclaimed(ClientInstanceId(7), reclaim_id);
        assert!(lane.reclaimed.is_empty());
        assert_eq!(lane.reclaimed_bytes, 0);
        assert!(lane.evicted_seq >= last_seq);
        assert!(lane.ring.iter().all(|(seq, _)| *seq > last_seq));
    }

    #[test]
    fn a_replay_inside_the_ring_serves_only_what_the_client_missed() {
        let fixture = Fixture::open(None, Some("s-1"));
        for text in ["a", "b", "c"] {
            fixture.chunk(text);
        }
        fixture.recorder.wait_for_items(3);

        fixture.fanout.replay(ClientId(9), fixture.pane, 2);
        let direct = fixture.recorder.direct.lock().clone();
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].0, ClientId(9));
        assert_eq!(direct[0].2, 2, "the replay starts where the client stopped");
        let items = direct[0]
            .3
            .iter()
            .map(|item| serde_json::from_slice::<AgentStreamItem>(item).expect("decode"))
            .collect::<Vec<_>>();
        assert_eq!(
            items.iter().filter_map(text_of).collect::<Vec<_>>(),
            ["b", "c"]
        );

        fixture.fanout.replay(ClientId(9), fixture.pane, 9);
        assert_eq!(
            fixture.recorder.direct.lock().len(),
            1,
            "a client already ahead of the ring is sent no updates"
        );
        assert_eq!(
            fixture.recorder.answered.lock().last(),
            Some(&(ClientId(9), fixture.pane, 0)),
            "but its replay is still answered, so it knows it has caught up"
        );
    }

    #[test]
    fn a_replay_older_than_the_ring_resets_and_replays_the_journal() {
        let directory = tempfile::tempdir().expect("journal directory");
        let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
        for text in ["one", "two"] {
            journal
                .append(
                    "s-1",
                    &json!({
                        "sessionUpdate": "agent_message_chunk",
                        "messageId": text,
                        "content": { "text": text }
                    }),
                )
                .expect("append");
        }
        let fixture = Fixture::open(Some(journal), Some("s-1"));
        fixture.accept(AgentStreamPayload::Ready {
            agent_name: "Codex".to_owned(),
            agent_key: "codex".to_owned(),
            auth_methods: Vec::new(),
            capabilities: AgentSessionCapabilities::default(),
        });
        fixture.accept(AgentStreamPayload::SessionReady {
            session_id: "s-1".to_owned(),
            modes: Some(json!({ "currentModeId": "plan", "availableModes": [] })),
            config_options: None,
        });
        fixture.recorder.wait_for_items(2);

        let filler = "x".repeat(MAX_AGENT_UPDATES_BYTES - 4096);
        let filler_count = MAX_REPLAY_RING_BYTES / filler.len() + 2;
        for _ in 0..filler_count {
            fixture.chunk(&filler);
        }
        let last_live = fixture.fanout.lanes.lock()[&fixture.pane]
            .next_seq
            .saturating_sub(1);

        fixture.fanout.replay(ClientId(3), fixture.pane, 0);
        let replayed = direct_items(&fixture.recorder)
            .into_iter()
            .map(|(client, _, item)| {
                assert_eq!(client, ClientId(3));
                item
            })
            .collect::<Vec<_>>();

        assert!(
            matches!(
                replayed.first().map(|item| &item.payload),
                Some(AgentStreamPayload::SessionReset { restoring: true })
            ),
            "the fallback starts with an explicit stream reset: {replayed:?}"
        );
        assert!(matches!(
            replayed.get(1).map(|item| &item.payload),
            Some(AgentStreamPayload::Ready { agent_name, .. }) if agent_name == "Codex"
        ));
        assert!(matches!(
            replayed.get(replayed.len().saturating_sub(2)).map(|item| &item.payload),
            Some(AgentStreamPayload::SessionReady {
                session_id,
                modes: Some(_),
                ..
            }) if session_id == "s-1"
        ));
        assert!(matches!(
            replayed.last().map(|item| &item.payload),
            Some(AgentStreamPayload::StateSynced { .. })
        ));
        assert_eq!(
            replayed.iter().filter_map(text_of).collect::<Vec<_>>(),
            ["one", "two"]
        );
        assert_eq!(
            replayed.iter().map(|item| item.seq).collect::<Vec<_>>(),
            (last_live - 5..=last_live).collect::<Vec<_>>(),
            "the replay takes the numbers the asker skips and mints none"
        );
    }

    #[test]
    fn a_journal_too_large_to_send_rebuilds_the_asker_from_memory() {
        let directory = tempfile::tempdir().expect("journal directory");
        let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
        let body = "x".repeat(850 * 1024);
        for index in 0..12 {
            journal
                .append(
                    "s-large",
                    &json!({
                        "sessionUpdate": "agent_message_chunk",
                        "content": { "text": format!("{index}:{body}") }
                    }),
                )
                .expect("append");
        }
        let fixture = Fixture::open(Some(journal), Some("s-large"));
        fixture.accept(AgentStreamPayload::Ready {
            agent_name: "Codex".to_owned(),
            agent_key: "codex".to_owned(),
            auth_methods: Vec::new(),
            capabilities: AgentSessionCapabilities::default(),
        });
        fixture.accept(AgentStreamPayload::SessionReady {
            session_id: "s-large".to_owned(),
            modes: None,
            config_options: None,
        });
        for text in ["a", "b", "c"] {
            fixture.chunk(text);
        }
        fixture.recorder.wait_for_items(5);
        fixture
            .fanout
            .lanes
            .lock()
            .get_mut(&fixture.pane)
            .expect("lane")
            .evicted_seq = 2;
        fixture.recorder.broadcast.lock().clear();

        fixture.fanout.replay(ClientId(3), fixture.pane, 1);
        let kind = |payload: AgentStreamPayload| match payload {
            AgentStreamPayload::SessionReset { restoring: true } => "reset",
            AgentStreamPayload::Ready { .. } => "ready",
            AgentStreamPayload::SessionReady { .. } => "session",
            AgentStreamPayload::StateSynced { .. } => "state",
            AgentStreamPayload::Chat {
                event: ChatEvent::Restart,
            } => "restart",
            AgentStreamPayload::Chat {
                event: ChatEvent::QueueChanged { prompts },
            } if prompts.is_empty() => "empty queue",
            _ => "other",
        };
        assert_eq!(
            direct_items(&fixture.recorder)
                .into_iter()
                .map(|(client, seq, item)| (client, seq, kind(item.payload)))
                .collect::<Vec<_>>(),
            [
                (ClientId(3), 2, "reset"),
                (ClientId(3), 3, "ready"),
                (ClientId(3), 4, "session"),
                (ClientId(3), 5, "state"),
                (ClientId(3), 6, "restart"),
            ],
            "past the rebuild cap the journal stays out and the asker is rebuilt from memory"
        );
        fixture.recorder.wait_for_items(6);
        assert_eq!(
            fixture
                .recorder
                .broadcast
                .lock()
                .iter()
                .flat_map(|(_, _, items, _)| items)
                .map(|item| {
                    let item: AgentStreamItem = serde_json::from_slice(item).expect("decode");
                    (item.seq, kind(item.payload))
                })
                .collect::<Vec<_>>(),
            [(6, "empty queue")],
            "the other viewers get only the number the rebuild needed, as a no-op"
        );
    }

    #[test]
    fn a_replay_older_than_the_ring_still_rebuilds_the_whole_chat() {
        use zz_protocol::chat::{ChatView, ItemBody, ItemId, ItemKind, ItemStatus, TurnBy, TurnId};
        let fixture = Fixture::open(None, Some("s-1"));
        let events = vec![
            ChatEvent::SessionReady {
                session: "s-1".to_owned(),
            },
            ChatEvent::TurnStarted {
                turn: TurnId(1),
                by: TurnBy::User { prompt: 1 },
                at: 1,
            },
            ChatEvent::ItemStarted {
                item: ItemId::new("a"),
                turn: Some(TurnId(1)),
                parent: None,
                kind: ItemKind::Agent,
                body: ItemBody::text("hel"),
                at: 2,
            },
            ChatEvent::ItemAppended {
                item: ItemId::new("a"),
                text: "lo".to_owned(),
            },
            ChatEvent::ItemEnded {
                item: ItemId::new("a"),
                status: ItemStatus::Completed,
                at: 3,
            },
        ];
        let mut expected = ChatView::default();
        for event in &events {
            expected.apply(event).expect("valid");
            fixture.chunk("update");
            fixture.accept(AgentStreamPayload::Chat {
                event: event.clone(),
            });
        }
        fixture.recorder.wait_for_items(2 * events.len());
        {
            let mut lanes = fixture.fanout.lanes.lock();
            let lane = lanes.get_mut(&fixture.pane).expect("pane lane");
            while lane.ring.front().is_some_and(|(seq, _)| *seq <= 6) {
                let (seq, dropped) = lane.ring.pop_front().expect("ring item");
                lane.ring_bytes -= dropped.len();
                lane.evicted_seq = seq;
            }
        }

        fixture.fanout.replay(ClientId(3), fixture.pane, 1);

        let mut rebuilt = ChatView::default();
        for item in fixture
            .recorder
            .direct
            .lock()
            .iter()
            .filter(|(client, ..)| *client == ClientId(3))
            .flat_map(|(_, _, _, items)| items.clone())
        {
            let item = serde_json::from_slice::<AgentStreamItem>(&item).expect("decode");
            if let AgentStreamPayload::Chat { event } = item.payload {
                let _ = rebuilt.apply(&event);
            }
        }
        rebuilt.skipped = 0;
        assert_eq!(rebuilt, expected);
    }

    #[test]
    fn a_long_chat_the_journal_cannot_vouch_for_is_rebuilt_for_the_asker_alone() {
        use zz_protocol::chat::{
            ChatPhase, ChatView, ItemBody, ItemId, ItemKind, ItemStatus, SessionView, TurnBy,
            TurnId, TurnOutcome,
        };

        let directory = tempfile::tempdir().expect("journal directory");
        let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
        let refused = (0..10_000).any(|index| {
            journal
                .append(
                    "s-1",
                    &json!({
                        "sessionUpdate": "agent_message_chunk",
                        "messageId": index.to_string(),
                        "content": { "text": "x" }
                    }),
                )
                .is_err()
        });
        assert!(refused, "the journal should hit its record cap");
        let fixture = Fixture::open(Some(journal), Some("s-1"));
        fixture.accept(AgentStreamPayload::Ready {
            agent_name: "Codex".to_owned(),
            agent_key: "codex".to_owned(),
            auth_methods: Vec::new(),
            capabilities: AgentSessionCapabilities::default(),
        });
        fixture.accept(AgentStreamPayload::SessionReady {
            session_id: "s-1".to_owned(),
            modes: None,
            config_options: None,
        });
        let item = |id: &str, turn: u64, text: String| ChatEvent::ItemStarted {
            item: ItemId::new(id),
            turn: Some(TurnId(turn)),
            parent: None,
            kind: ItemKind::Agent,
            body: ItemBody::text(text),
            at: 0,
        };
        let ended = |id: &str| ChatEvent::ItemEnded {
            item: ItemId::new(id),
            status: ItemStatus::Completed,
            at: 0,
        };
        let started = |turn: u64| ChatEvent::TurnStarted {
            turn: TurnId(turn),
            by: TurnBy::Agent,
            at: 0,
        };
        let mut events = vec![ChatEvent::SessionReady {
            session: "s-1".to_owned(),
        }];
        for turn in 1..=24 {
            let id = format!("a{turn}");
            events.extend([
                started(turn),
                item(&id, turn, "x".repeat(1024 * 1024)),
                ended(&id),
                ChatEvent::TurnEnded {
                    turn: TurnId(turn),
                    outcome: TurnOutcome::Completed {
                        stop_reason: "end_turn".to_owned(),
                    },
                    at: 0,
                },
            ]);
        }
        events.extend([
            started(25),
            item("b1", 25, "y".repeat(3 * 1024 * 1024)),
            ended("b1"),
            item("b2", 25, "z".repeat(3 * 1024 * 1024)),
            ended("b2"),
            item("b3", 25, "now".to_owned()),
        ]);
        for event in events {
            fixture.accept(AgentStreamPayload::Chat { event });
        }
        let deadline = Instant::now() + DEADLINE;
        while fixture.fanout.lanes.lock()[&fixture.pane]
            .deadline
            .is_some()
        {
            assert!(Instant::now() < deadline, "the live batch flushes");
            std::thread::sleep(Duration::from_millis(2));
        }
        let (next_seq, ring) = {
            let lanes = fixture.fanout.lanes.lock();
            let lane = &lanes[&fixture.pane];
            assert!(lane.evicted_seq > 1, "the ring no longer holds the start");
            (lane.next_seq, lane.ring.len())
        };
        let broadcast_before = fixture.recorder.broadcast.lock().len();

        fixture.fanout.replay(ClientId(3), fixture.pane, 1);

        {
            let lanes = fixture.fanout.lanes.lock();
            let lane = &lanes[&fixture.pane];
            assert!(lane.batch.is_empty() && lane.deadline.is_none());
            assert_eq!((lane.next_seq, lane.ring.len()), (next_seq, ring));
        }
        assert_eq!(
            fixture.recorder.broadcast.lock().len(),
            broadcast_before,
            "no viewer that did not ask gets anything"
        );
        let direct = fixture.recorder.direct.lock().clone();
        let bytes = direct
            .iter()
            .flat_map(|(_, _, _, items)| items.iter().map(Vec::len))
            .sum::<usize>();
        assert!(bytes <= MAX_REBUILD_BYTES, "{bytes} bytes");
        let replayed = direct_items(&fixture.recorder);
        assert!(replayed.iter().all(|(client, ..)| *client == ClientId(3)));
        assert_eq!(
            replayed.iter().map(|(_, seq, _)| *seq).collect::<Vec<_>>(),
            (next_seq - 11..next_seq).collect::<Vec<_>>(),
            "the rebuild ends on the lane's last number and mints none"
        );
        assert!(matches!(
            replayed[0].2.payload,
            AgentStreamPayload::SessionReset { restoring: true }
        ));
        let mut rebuilt = ChatView::default();
        for (_, _, item) in replayed {
            if let AgentStreamPayload::Chat { event } = item.payload {
                let _ = rebuilt.apply(&event);
            }
        }
        assert_eq!(
            rebuilt.session,
            SessionView::Ready {
                session: "s-1".to_owned()
            }
        );
        assert_eq!(
            rebuilt.turns.iter().map(|turn| turn.id).collect::<Vec<_>>(),
            [TurnId(25)]
        );
        assert_eq!(
            rebuilt.turns[0].items,
            [ItemId::new("b2"), ItemId::new("b3")]
        );
        assert_eq!(rebuilt.items[&ItemId::new("b3")].body.text, "now");
        assert_eq!(rebuilt.phase(), ChatPhase::Running);
        assert_eq!(
            rebuilt.skipped, 1,
            "the end of b1, whose start was left out"
        );

        let asked = next_seq - 9;
        {
            let mut lanes = fixture.fanout.lanes.lock();
            let lane = lanes.get_mut(&fixture.pane).expect("pane lane");
            while lane.ring.front().is_some_and(|(seq, _)| *seq <= asked + 1) {
                let (seq, dropped) = lane.ring.pop_front().expect("ring item");
                lane.ring_bytes -= dropped.len();
                lane.evicted_seq = seq;
            }
        }
        fixture.fanout.replay(ClientId(4), fixture.pane, asked);
        let tight = direct_items(&fixture.recorder)
            .into_iter()
            .filter(|(client, ..)| *client == ClientId(4))
            .collect::<Vec<_>>();
        assert!(matches!(
            tight[0].2.payload,
            AgentStreamPayload::SessionReset { restoring: true }
        ));
        assert!(
            tight[0].1 > asked,
            "the reset at {} lands past {asked}, the last number the client holds",
            tight[0].1
        );
        assert_eq!(tight.last().map(|(_, seq, _)| *seq), Some(next_seq - 1));
    }

    #[test]
    fn a_capped_rebuild_keeps_an_old_approval_and_a_running_background_item() {
        use zz_protocol::chat::{
            Ask, ChatPhase, ChatView, ItemBody, ItemId, ItemKind, ItemStatus, RequestId, TurnBy,
            TurnId, TurnOutcome,
        };

        let fixture = Fixture::open(None, Some("s-1"));
        let watcher = ItemId::new("watcher");
        let turn = |id: u64, item: &str, text: String| {
            let item = ItemId::new(item);
            [
                ChatEvent::TurnStarted {
                    turn: TurnId(id),
                    by: TurnBy::Agent,
                    at: 0,
                },
                ChatEvent::ItemStarted {
                    item: item.clone(),
                    turn: Some(TurnId(id)),
                    parent: None,
                    kind: ItemKind::Agent,
                    body: ItemBody::text(text),
                    at: 0,
                },
                ChatEvent::ItemEnded {
                    item,
                    status: ItemStatus::Completed,
                    at: 0,
                },
                ChatEvent::TurnEnded {
                    turn: TurnId(id),
                    outcome: TurnOutcome::Completed {
                        stop_reason: "end_turn".to_owned(),
                    },
                    at: 0,
                },
            ]
        };
        let mut events = vec![
            ChatEvent::SessionReady {
                session: "s-1".to_owned(),
            },
            ChatEvent::TurnStarted {
                turn: TurnId(1),
                by: TurnBy::Agent,
                at: 0,
            },
            ChatEvent::ItemStarted {
                item: watcher.clone(),
                turn: Some(TurnId(1)),
                parent: None,
                kind: ItemKind::Execute,
                body: ItemBody::titled("tail -f build.log"),
                at: 0,
            },
            ChatEvent::ItemBackgrounded {
                item: watcher.clone(),
            },
            ChatEvent::RequestOpened {
                request: RequestId(7),
                turn: None,
                item: Some(watcher.clone()),
                ask: Ask::Approval {
                    title: "rm -rf build".to_owned(),
                    detail: None,
                    options: Vec::new(),
                },
                blocking: true,
                at: 0,
            },
            ChatEvent::TurnEnded {
                turn: TurnId(1),
                outcome: TurnOutcome::Completed {
                    stop_reason: "end_turn".to_owned(),
                },
                at: 0,
            },
        ];
        for id in 2..=9 {
            events.extend(turn(id, &format!("a{id}"), "x".repeat(1024 * 1024)));
        }
        let mut live = ChatView::default();
        for event in events {
            live.apply(&event).expect("live event");
            fixture.accept(AgentStreamPayload::Chat { event });
        }
        assert_eq!(live.phase(), ChatPhase::Waiting);
        fixture
            .fanout
            .lanes
            .lock()
            .get_mut(&fixture.pane)
            .expect("lane")
            .evicted_seq = 2;

        fixture.fanout.replay(ClientId(3), fixture.pane, 1);

        let replayed = direct_items(&fixture.recorder);
        let bytes = fixture
            .recorder
            .direct
            .lock()
            .iter()
            .flat_map(|(_, _, _, items)| items.iter().map(Vec::len))
            .sum::<usize>();
        assert!(bytes <= MAX_REBUILD_BYTES, "{bytes} bytes");
        let mut rebuilt = ChatView::default();
        for (_, _, item) in replayed {
            if let AgentStreamPayload::Chat { event } = item.payload {
                let _ = rebuilt.apply(&event);
            }
        }
        assert_eq!(rebuilt.phase(), ChatPhase::Waiting);
        assert_eq!(rebuilt.item(&watcher), live.item(&watcher));
        assert_eq!(rebuilt.request(RequestId(7)), live.request(RequestId(7)));
        assert_eq!(
            rebuilt
                .turns
                .iter()
                .map(|turn| turn.id.0)
                .collect::<Vec<_>>(),
            [1, 6, 7, 8, 9]
        );
        assert_eq!(
            rebuilt.skipped, 2,
            "the end of a5 and of its turn, whose starts were left out"
        );
    }

    fn label(item: &AgentStreamItem) -> String {
        match &item.payload {
            AgentStreamPayload::SessionReset { restoring: true } => "reset".to_owned(),
            AgentStreamPayload::Chat {
                event: ChatEvent::Restart,
            } => "restart".to_owned(),
            _ => text_of(item).unwrap_or_default(),
        }
    }

    fn direct_items(recorder: &Recorder) -> Vec<(ClientId, u64, AgentStreamItem)> {
        recorder
            .direct
            .lock()
            .iter()
            .flat_map(|(client, _, _, items)| {
                items.iter().map(move |item| {
                    let item: AgentStreamItem = serde_json::from_slice(item).expect("decode item");
                    (*client, item.seq, item)
                })
            })
            .collect()
    }

    #[test]
    fn a_reopened_chat_continues_its_numbers_and_replays_only_to_the_asker() {
        let fixture = Fixture::open(None, Some("s-1"));
        for text in ["a", "b", "c", "d", "e"] {
            fixture.chunk(text);
        }
        fixture.recorder.wait_for_items(5);
        fixture.fanout.close_lane(fixture.pane);
        fixture.fanout.open_lane(
            fixture.pane,
            2,
            AgentProvider::Codex,
            Some("s-1".to_owned()),
        );
        for text in ["f", "g", "h"] {
            fixture.accept_generation(
                2,
                AgentStreamPayload::Update {
                    update: json!({ "sessionUpdate": "agent_message_chunk", "content": { "text": text } }),
                },
            );
        }
        let items = fixture.recorder.wait_for_items(10);
        assert_eq!(
            items.iter().map(|item| item.seq).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
        );
        let broadcast = fixture.recorder.broadcast.lock().len();

        fixture.fanout.replay(ClientId(9), fixture.pane, 0);
        let direct = direct_items(&fixture.recorder)
            .into_iter()
            .map(|(client, seq, item)| (client, seq, label(&item)))
            .collect::<Vec<_>>();
        assert_eq!(
            direct,
            [
                (ClientId(9), 5, "reset".to_owned()),
                (ClientId(9), 6, "reset".to_owned()),
                (ClientId(9), 7, "restart".to_owned()),
                (ClientId(9), 8, "f".to_owned()),
                (ClientId(9), 9, "g".to_owned()),
                (ClientId(9), 10, "h".to_owned()),
            ]
        );
        assert_eq!(fixture.recorder.broadcast.lock().len(), broadcast);
    }

    #[test]
    fn a_client_kept_across_a_reopen_shows_the_resumed_chat_once() {
        use zz_protocol::chat::{
            ChatView, ItemBody, ItemId, ItemKind, ItemStatus, TurnBy, TurnId, TurnOutcome,
        };

        let turn = |id: u64, item: &str, text: &str| {
            let item = ItemId::new(item);
            [
                ChatEvent::TurnStarted {
                    turn: TurnId(id),
                    by: TurnBy::Agent,
                    at: 0,
                },
                ChatEvent::ItemStarted {
                    item: item.clone(),
                    turn: Some(TurnId(id)),
                    parent: None,
                    kind: ItemKind::Agent,
                    body: ItemBody::default(),
                    at: 0,
                },
                ChatEvent::ItemAppended {
                    item: item.clone(),
                    text: text.to_owned(),
                },
                ChatEvent::ItemEnded {
                    item,
                    status: ItemStatus::Completed,
                    at: 0,
                },
                ChatEvent::TurnEnded {
                    turn: TurnId(id),
                    outcome: TurnOutcome::Completed {
                        stop_reason: "end_turn".to_owned(),
                    },
                    at: 0,
                },
            ]
        };
        let fixture = Fixture::open(None, Some("s-1"));
        for event in turn(7, "agent:live", "before the flip") {
            fixture.accept(AgentStreamPayload::Chat { event });
        }
        let mut view = ChatView::default();
        for item in fixture.recorder.wait_for_items(5) {
            if let AgentStreamPayload::Chat { event } = item.payload {
                let _ = view.apply(&event);
            }
        }
        assert_eq!(view.turns.len(), 1);

        fixture.fanout.close_lane(fixture.pane);
        fixture.fanout.open_lane(
            fixture.pane,
            2,
            AgentProvider::Codex,
            Some("s-1".to_owned()),
        );
        for event in turn(1, "agent:resumed", "the resumed history") {
            fixture.accept_generation(2, AgentStreamPayload::Chat { event });
        }
        fixture.recorder.wait_for_items(12);
        fixture.fanout.replay(ClientId(9), fixture.pane, 6);
        let direct = direct_items(&fixture.recorder);
        assert_eq!(
            direct.iter().map(|(_, seq, _)| *seq).collect::<Vec<_>>(),
            [6, 7, 8, 9, 10, 11, 12]
        );
        for (_, _, item) in direct {
            if let AgentStreamPayload::Chat { event } = item.payload {
                let _ = view.apply(&event);
            }
        }
        assert_eq!(
            view.turns.iter().map(|turn| turn.id).collect::<Vec<_>>(),
            [TurnId(1)]
        );
        assert_eq!(view.items.len(), 1);
        assert_eq!(
            view.items[&ItemId::new("agent:resumed")].body.text,
            "the resumed history"
        );
        assert_eq!(view.skipped, 0);
    }

    #[test]
    fn opening_an_existing_pane_leaves_its_live_lane_intact() {
        let recorder = Arc::new(Recorder::default());
        let publisher: Arc<dyn AgentPublisher> = Arc::<Recorder>::clone(&recorder);
        let runtime = AgentRuntime::new(&publisher, AgentSpawnConfig::default(), None);
        runtime.set_runner_factory(Box::new(|spec| {
            fixture_runner(
                spec.provider,
                Behavior::Chunk,
                zz_protocol::AgentAutoApprove::Off,
                true,
            )
        }));
        let pane = PaneId(45);
        let spec = AgentPaneSpec {
            model: None,
            effort: None,
            provider: AgentProvider::Codex,
            cwd: PathBuf::from("/"),
            resume_session: None,
            auto_approve: None,
            workspace: crate::agent::environment::AgentWorkspaceEnvironment::default(),
        };

        assert!(runtime.open(pane, spec.clone()));
        let generation = runtime.fanout.lanes.lock()[&pane].generation;
        assert!(runtime.open(pane, spec));
        assert_eq!(runtime.host.pane_count(), 1);
        assert_eq!(runtime.fanout.lanes.lock()[&pane].generation, generation);
        assert!(runtime.prompt(
            pane,
            AgentPrompt {
                owner: ClientInstanceId::default(),
                text: "probe".to_owned(),
                images: Vec::new(),
            },
        ));
        let deadline = Instant::now() + DEADLINE;
        while Instant::now() < deadline {
            if recorder
                .payloads()
                .iter()
                .filter_map(text_of)
                .any(|text| text == "turn 0")
            {
                runtime.shutdown();
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("the original lane stopped delivering after a duplicate open");
    }

    #[test]
    fn closing_a_busy_chat_hands_its_queue_back_to_the_composer() {
        let recorder = Arc::new(Recorder::default());
        let publisher: Arc<dyn AgentPublisher> = Arc::<Recorder>::clone(&recorder);
        let runtime = AgentRuntime::new(&publisher, AgentSpawnConfig::default(), None);
        runtime.set_runner_factory(Box::new(|spec| {
            fixture_runner(
                spec.provider,
                Behavior::Hang,
                zz_protocol::AgentAutoApprove::Off,
                true,
            )
        }));
        let pane = PaneId(46);
        let spec = AgentPaneSpec {
            model: None,
            effort: None,
            provider: AgentProvider::Codex,
            cwd: PathBuf::from("/"),
            resume_session: None,
            auto_approve: None,
            workspace: crate::agent::environment::AgentWorkspaceEnvironment::default(),
        };
        let prompt = |text: &str| AgentPrompt {
            owner: ClientInstanceId(7),
            text: text.to_owned(),
            images: Vec::new(),
        };
        let deadline = Instant::now() + DEADLINE;
        let wait = |done: &dyn Fn(&AgentPaneState) -> bool, what: &str| {
            while !runtime
                .host
                .snapshot_state(pane)
                .is_some_and(|state| done(&state))
            {
                assert!(Instant::now() < deadline, "{what}");
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        for (running, waiting) in [("running", "waiting"), ("running again", "waiting again")] {
            assert!(runtime.open(pane, spec.clone()));
            wait(&|state| state.session_id.is_some(), "the chat never opened");
            assert!(runtime.prompt(pane, prompt(running)));
            wait(
                &|state| state.phase == AgentConnectionPhase::Running,
                "the turn never ran",
            );
            assert!(runtime.prompt(pane, prompt(waiting)));
            wait(
                &|state| state.queued_prompts == 1,
                "the prompt never queued",
            );
            runtime.close_and_wait(pane);
        }

        let restored = recorder
            .payloads()
            .into_iter()
            .filter_map(|item| match item.payload {
                AgentStreamPayload::PromptsRestored {
                    reclaim_id,
                    prompts,
                } => Some((reclaim_id, prompts[0].owner, prompts[0].text.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            restored,
            [
                (1, ClientInstanceId(7), "running".to_owned()),
                (2, ClientInstanceId(7), "waiting".to_owned()),
                (3, ClientInstanceId(7), "running again".to_owned()),
                (4, ClientInstanceId(7), "waiting again".to_owned()),
            ]
        );
        runtime.shutdown();
    }

    #[test]
    fn journal_replay_restores_the_latest_mode_and_live_state() {
        let directory = tempfile::tempdir().expect("journal directory");
        let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
        let fixture = Fixture::open(Some(journal), Some("s-1"));
        fixture.accept(AgentStreamPayload::SessionReady {
            session_id: "s-1".to_owned(),
            modes: Some(json!({
                "currentModeId": "plan",
                "availableModes": [
                    { "id": "plan", "name": "Plan" },
                    { "id": "ask", "name": "Ask" }
                ]
            })),
            config_options: None,
        });
        fixture.accept(AgentStreamPayload::Update {
            update: serde_json::to_value(SessionUpdate::CurrentModeUpdate(
                agent_client_protocol::v1::schema::CurrentModeUpdate::new("ask"),
            ))
            .expect("encode mode update"),
        });
        fixture.chunk("a");
        fixture.chunk("b");
        fixture.recorder.wait_for_items(4);
        {
            let mut lanes = fixture.fanout.lanes.lock();
            let lane = lanes.get_mut(&fixture.pane).expect("pane lane");
            lane.evicted_seq = lane.next_seq.saturating_sub(1);
        }

        fixture.fanout.replay(ClientId(3), fixture.pane, 1);
        let replayed = direct_items(&fixture.recorder)
            .into_iter()
            .map(|(_, _, item)| item)
            .collect::<Vec<_>>();
        let modes = replayed.iter().find_map(|item| match &item.payload {
            AgentStreamPayload::SessionReady {
                modes: Some(modes), ..
            } => Some(modes),
            _ => None,
        });
        assert_eq!(
            modes
                .and_then(|modes| modes.get("currentModeId"))
                .and_then(Value::as_str),
            Some("ask")
        );
        let state = replayed.last().and_then(|item| match &item.payload {
            AgentStreamPayload::StateSynced { state } => Some(state),
            _ => None,
        });
        assert!(matches!(
            state.map(|state| &state.phase),
            Some(zz_protocol::AgentConnectionPhase::Running)
        ));
        assert_eq!(
            state
                .and_then(|state| serde_json::from_str::<Value>(&state.modes).ok())
                .and_then(|modes| modes.get("currentModeId").cloned())
                .and_then(|mode| mode.as_str().map(ToOwned::to_owned))
                .as_deref(),
            Some("ask")
        );
    }

    #[test]
    fn journal_replay_restores_raw_config_option_updates() {
        use agent_client_protocol::v1::schema::{ConfigOptionUpdate, SessionConfigOption};

        let directory = tempfile::tempdir().expect("journal directory");
        let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
        let fixture = Fixture::open(Some(journal), Some("s-1"));
        fixture.accept(AgentStreamPayload::SessionReady {
            session_id: "s-1".to_owned(),
            modes: None,
            config_options: Some(
                serde_json::to_value(vec![SessionConfigOption::boolean(
                    "sandbox", "Sandbox", false,
                )])
                .expect("encode initial config"),
            ),
        });
        fixture.accept(AgentStreamPayload::Update {
            update: serde_json::to_value(SessionUpdate::ConfigOptionUpdate(
                ConfigOptionUpdate::new(vec![SessionConfigOption::boolean(
                    "sandbox", "Sandbox", true,
                )]),
            ))
            .expect("encode config update"),
        });
        fixture.chunk("a");
        fixture.chunk("b");
        fixture.recorder.wait_for_items(4);
        {
            let mut lanes = fixture.fanout.lanes.lock();
            let lane = lanes.get_mut(&fixture.pane).expect("pane lane");
            lane.evicted_seq = lane.next_seq.saturating_sub(1);
        }

        fixture.fanout.replay(ClientId(3), fixture.pane, 1);
        let replayed = direct_items(&fixture.recorder)
            .into_iter()
            .map(|(_, _, item)| item)
            .collect::<Vec<_>>();
        let config = replayed.iter().find_map(|item| match &item.payload {
            AgentStreamPayload::SessionReady {
                config_options: Some(config),
                ..
            } => Some(config),
            _ => None,
        });
        assert_eq!(
            config
                .and_then(Value::as_array)
                .and_then(|options| options.first())
                .and_then(|option| option.get("currentValue"))
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn a_journal_replay_shows_the_effort_option_as_the_live_chat_does() {
        let directory = tempfile::tempdir().expect("journal directory");
        let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
        let options = |effort: &str, level: &str| {
            json!([
                {"id": "model", "name": "Model", "type": "select", "currentValue": "gemini-3.8-flash", "options": [{"value": "gemini-3.8-flash", "name": "Flash"}]},
                {"id": effort, "name": "Thinking", "type": "select", "currentValue": level, "options": [{"value": "low", "name": "Low"}, {"value": "high", "name": "High"}]},
            ])
        };
        let fixture = Fixture::open(Some(Arc::clone(&journal)), None);
        let pane = PaneId(5);
        fixture
            .fanout
            .open_lane(pane, 1, AgentProvider::Antigravity, Some("s-1".to_owned()));
        let accept = |payload| {
            fixture.fanout.accept(
                pane,
                1,
                &state(AgentConnectionPhase::Ready),
                Some(AgentStreamItem { seq: 0, payload }),
            );
        };
        accept(AgentStreamPayload::SessionReady {
            session_id: "s-1".to_owned(),
            modes: None,
            config_options: Some(options("effort", "low")),
        });
        journal
            .append_for(
                AgentProvider::Antigravity,
                "s-1",
                &json!({"sessionUpdate": "config_option_update", "configOptions": options("thought_level", "high")}),
            )
            .expect("journal the agent's update");
        accept(AgentStreamPayload::Update {
            update: json!({"sessionUpdate": "config_option_update", "configOptions": options("effort", "high")}),
        });
        for text in ["a", "b"] {
            accept(AgentStreamPayload::Update {
                update: json!({"sessionUpdate": "agent_message_chunk", "content": {"text": text}}),
            });
        }
        let live = fixture.recorder.wait_for_items(4);
        {
            let mut lanes = fixture.fanout.lanes.lock();
            let lane = lanes.get_mut(&pane).expect("pane lane");
            lane.evicted_seq = lane.next_seq.saturating_sub(1);
        }
        fixture.recorder.broadcast.lock().clear();

        fixture.fanout.replay(ClientId(3), pane, 1);
        let lists = |items: &[AgentStreamItem]| {
            items
                .iter()
                .filter_map(|item| match &item.payload {
                    AgentStreamPayload::SessionReady {
                        config_options: Some(options),
                        ..
                    } => Some(options.clone()),
                    AgentStreamPayload::Update { update }
                        if update["sessionUpdate"] == "config_option_update" =>
                    {
                        Some(update["configOptions"].clone())
                    }
                    _ => None,
                })
                .map(|options| {
                    options
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|option| format!("{}={}", option["id"], option["currentValue"]))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            lists(&live),
            [
                ["\"model\"=\"gemini-3.8-flash\"", "\"effort\"=\"low\""],
                ["\"model\"=\"gemini-3.8-flash\"", "\"effort\"=\"high\""],
            ]
        );
        assert_eq!(
            lists(&fixture.recorder.payloads()),
            [["\"model\"=\"gemini-3.8-flash\"", "\"effort\"=\"high\""]]
        );
    }

    #[test]
    fn a_journal_replay_goes_to_the_asker_alone_on_the_numbers_it_skips() {
        let directory = tempfile::tempdir().expect("journal directory");
        let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
        journal
            .append_for(
                AgentProvider::Codex,
                "s-1",
                &json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "journalled"}}),
            )
            .expect("journal");
        let fixture = Fixture::open(Some(journal), Some("s-1"));
        fixture.accept(AgentStreamPayload::SessionReady {
            session_id: "s-1".to_owned(),
            modes: None,
            config_options: None,
        });
        for text in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"] {
            fixture.chunk(text);
        }
        fixture.recorder.wait_for_items(11);
        let evict = |upto: u64| {
            let mut lanes = fixture.fanout.lanes.lock();
            let lane = lanes.get_mut(&fixture.pane).expect("pane lane");
            while lane.ring.front().is_some_and(|(seq, _)| *seq <= upto) {
                let (seq, dropped) = lane.ring.pop_front().expect("ring item");
                lane.ring_bytes -= dropped.len();
                lane.evicted_seq = seq;
            }
            (lane.evicted_seq, lane.next_seq)
        };
        fixture.recorder.broadcast.lock().clear();
        let sent = |from_seq| {
            fixture.recorder.direct.lock().clear();
            fixture.fanout.replay(ClientId(3), fixture.pane, from_seq);
            fixture
                .recorder
                .direct
                .lock()
                .iter()
                .flat_map(|(client, _, _, items)| {
                    assert_eq!(*client, ClientId(3));
                    items.clone()
                })
                .map(|item| {
                    let item = serde_json::from_slice::<AgentStreamItem>(&item).expect("decode");
                    let kind = match &item.payload {
                        AgentStreamPayload::Update { update } => update["content"]["text"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                        AgentStreamPayload::SessionReset { .. } => "reset".to_owned(),
                        AgentStreamPayload::SessionReady { .. } => "ready".to_owned(),
                        AgentStreamPayload::StateSynced { .. } => "state".to_owned(),
                        other => format!("{other:?}"),
                    };
                    (item.seq, kind)
                })
                .collect::<Vec<_>>()
        };
        let journal_replay = [
            (8, "reset".to_owned()),
            (9, "journalled".to_owned()),
            (10, "ready".to_owned()),
            (11, "state".to_owned()),
        ];

        assert_eq!(evict(1), (1, 12));
        assert_eq!(
            sent(0),
            journal_replay,
            "a client that holds nothing is reset when the ring lacks its first item"
        );
        assert_eq!(evict(9), (9, 12));
        assert_eq!(
            sent(9),
            [(10, "i".to_owned()), (11, "j".to_owned())],
            "a client that has the last evicted number takes the ring"
        );
        assert_eq!(sent(1), journal_replay);
        assert!(fixture.recorder.broadcast.lock().is_empty());
        let lanes = fixture.fanout.lanes.lock();
        let lane = &lanes[&fixture.pane];
        assert_eq!((lane.evicted_seq, lane.next_seq), (9, 12));
    }

    #[test]
    fn session_replies_leave_the_stream_without_spending_a_sequence() {
        let fixture = Fixture::open(None, Some("s-1"));
        fixture.accept(AgentStreamPayload::SessionsListed {
            client: ClientId(8),
            sessions: vec![AgentSessionSummary {
                session_id: "s-2".to_owned(),
                cwd: PathBuf::from("/work"),
                additional_directories: Vec::new(),
                title: None,
                updated_at: None,
            }],
            next_cursor: None,
            cwd_filter: None,
            replace: true,
        });
        fixture.accept(AgentStreamPayload::SessionDeleted {
            client: ClientId(10),
            session_id: "s-3".to_owned(),
        });
        fixture.chunk("after");
        let items = fixture.recorder.wait_for_items(1);

        assert_eq!(
            items.iter().map(|item| item.seq).collect::<Vec<_>>(),
            [1],
            "a reply is not part of the transcript the client replays"
        );
        let replies = fixture.recorder.replies.lock();
        assert_eq!(replies.len(), 2);
        assert_eq!(replies[0].0, ClientId(8));
        assert_eq!(replies[0].2, 0);
        assert_eq!(replies[1].0, ClientId(10));
        assert!(replies[1].3.contains("s-3"));
    }

    #[test]
    fn an_oversized_session_page_returns_a_targeted_failure() {
        let fixture = Fixture::open(None, Some("s-1"));
        fixture.accept(AgentStreamPayload::SessionsListed {
            client: ClientId(8),
            sessions: (0..300)
                .map(|index| AgentSessionSummary {
                    session_id: format!("s-{index}"),
                    cwd: PathBuf::from("/work"),
                    additional_directories: Vec::new(),
                    title: Some("x".repeat(4 * 1024)),
                    updated_at: None,
                })
                .collect(),
            next_cursor: None,
            cwd_filter: None,
            replace: true,
        });

        let replies = fixture.recorder.replies.lock();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].0, ClientId(8));
        assert!(matches!(
            serde_json::from_str::<AgentStreamPayload>(&replies[0].3),
            Ok(AgentStreamPayload::SessionListFailed { client, message })
                if client == ClientId(8) && message.contains("too much session history")
        ));
    }

    #[test]
    fn the_pane_state_is_published_only_when_something_a_client_renders_moves() {
        let fixture = Fixture::open(None, Some("s-1"));
        fixture.chunk("a");
        fixture.chunk("b");
        assert_eq!(
            fixture.recorder.states.lock().len(),
            1,
            "a steady turn republishes nothing"
        );

        let mut running = state(AgentConnectionPhase::Running);
        running.queued_prompts = 1;
        fixture.fanout.accept(fixture.pane, 1, &running, None);
        running.git = Some(zz_protocol::AgentGitSummary {
            branch: Some("main".to_owned()),
            changed_files: 2,
            additions: 8,
            deletions: 3,
        });
        fixture.fanout.accept(fixture.pane, 1, &running, None);
        let states = fixture.recorder.states.lock();
        assert_eq!(states.len(), 3);
        assert_eq!(states[1].1.queued_prompts, 1);
        assert_eq!(states[2].1.git, running.git);
    }

    fn accept_in(fixture: &Fixture, phase: AgentConnectionPhase, payload: AgentStreamPayload) {
        fixture.fanout.accept(
            fixture.pane,
            1,
            &state(phase),
            Some(AgentStreamItem { seq: 0, payload }),
        );
    }

    fn finished(reason: &str) -> AgentStreamPayload {
        AgentStreamPayload::PromptFinished {
            turn_id: 1,
            outcome: AgentPromptOutcome::Finished {
                stop_reason: json!(reason),
            },
        }
    }

    fn replays_a_running_turn(fixture: &Fixture) -> bool {
        fixture
            .fanout
            .lanes
            .lock()
            .get(&fixture.pane)
            .expect("lane")
            .recovery_metadata()
            .iter()
            .any(|payload| matches!(payload, AgentStreamPayload::TurnStarted { .. }))
    }

    #[test]
    fn a_finished_turn_is_not_replayed_as_running() {
        let fixture = Fixture::open(None, Some("s-1"));
        accept_in(
            &fixture,
            AgentConnectionPhase::Running,
            AgentStreamPayload::TurnStarted { turn_id: 1 },
        );
        assert!(replays_a_running_turn(&fixture));
        accept_in(&fixture, AgentConnectionPhase::Ready, finished("end_turn"));
        assert!(!replays_a_running_turn(&fixture));
    }

    struct Watched {
        fixture: Fixture,
        seen: u64,
        chat: Vec<(u64, ChatEvent)>,
        straight: zz_protocol::chat::ChatView,
    }

    impl Watched {
        fn open(
            tape: &crate::agent::cut_points::Tape,
            journal: Option<Arc<AgentJournal>>,
            parked: Option<u64>,
        ) -> Self {
            let recorder = Arc::new(Recorder::default());
            let publisher: Arc<dyn AgentPublisher> = Arc::<Recorder>::clone(&recorder);
            let fanout = Arc::new(AgentFanout::new(
                &publisher,
                journal,
                Arc::new(Mutex::new(())),
            ));
            let pane = PaneId(4);
            if let Some(next_seq) = parked {
                fanout.parked.lock().insert(pane, (next_seq, 1));
            }
            fanout.open_lane(pane, 1, tape.provider, Some(tape.session.clone()));
            let mut watched = Self {
                fixture: Fixture {
                    fanout,
                    recorder,
                    pane,
                },
                seen: 0,
                chat: Vec::new(),
                straight: zz_protocol::chat::ChatView::default(),
            };
            watched.catch_up();
            watched
        }

        fn accept(&mut self, payload: &AgentStreamPayload) {
            self.fixture.accept(payload.clone());
            self.fixture.recorder.broadcast.lock().clear();
            self.catch_up();
        }

        fn catch_up(&mut self) {
            let lanes = self.fixture.fanout.lanes.lock();
            let seen = self.seen;
            for (seq, encoded) in lanes[&self.fixture.pane]
                .ring
                .iter()
                .filter(|(seq, _)| *seq > seen)
            {
                let item: AgentStreamItem = serde_json::from_slice(encoded).expect("decode");
                if let AgentStreamPayload::Chat { event } = item.payload {
                    let _ = self.straight.apply(&event);
                    self.chat.push((*seq, event));
                }
                self.seen = *seq;
            }
        }

        fn next_seq(&self) -> u64 {
            self.fixture.fanout.lanes.lock()[&self.fixture.pane].next_seq
        }

        fn log(&self) -> Vec<ChatEvent> {
            std::iter::once(ChatEvent::Restart)
                .chain(
                    self.fixture.fanout.lanes.lock()[&self.fixture.pane]
                        .chat
                        .events()
                        .cloned(),
                )
                .collect()
        }

        fn known(&self, from_seq: u64) -> zz_protocol::chat::ChatView {
            crate::agent::cut_points::fold(
                self.chat
                    .iter()
                    .take_while(|(seq, _)| *seq <= from_seq)
                    .map(|(_, event)| event),
            )
        }

        fn replay(&mut self, from_seq: u64, evicted: bool) -> Replayed {
            let pane = self.fixture.pane;
            let decode = |item: &Vec<u8>| -> AgentStreamItem {
                serde_json::from_slice(item).expect("decode")
            };
            let (before, next_seq) = {
                let mut lanes = self.fixture.fanout.lanes.lock();
                let lane = lanes.get_mut(&pane).expect("lane");
                let before = lane.evicted_seq;
                if evicted {
                    lane.evicted_seq = lane.next_seq;
                }
                (before, lane.next_seq)
            };
            self.fixture.recorder.broadcast.lock().clear();
            let straight = self.straight.clone();
            self.fixture.fanout.replay(ClientId(7), pane, from_seq);
            let mut others = {
                let mut lanes = self.fixture.fanout.lanes.lock();
                let lane = lanes.get_mut(&pane).expect("lane");
                lane.evicted_seq = before;
                lane.take_batch()
                    .into_iter()
                    .flat_map(|(_, items)| items)
                    .collect::<Vec<_>>()
            };
            others.extend(
                std::mem::take(&mut *self.fixture.recorder.broadcast.lock())
                    .into_iter()
                    .flat_map(|(_, _, items, _)| items),
            );
            let mut others = others
                .iter()
                .map(decode)
                .filter(|item| item.seq >= next_seq)
                .collect::<Vec<_>>();
            others.sort_by_key(|item| item.seq);
            let asker = std::mem::take(&mut *self.fixture.recorder.direct.lock())
                .into_iter()
                .flat_map(|(_, _, _, items)| items)
                .map(|item| decode(&item))
                .collect();
            self.catch_up();
            Replayed {
                asker,
                others,
                before: next_seq,
                after: self.next_seq(),
                straight,
            }
        }
    }

    struct Replayed {
        asker: Vec<AgentStreamItem>,
        others: Vec<AgentStreamItem>,
        before: u64,
        after: u64,
        straight: zz_protocol::chat::ChatView,
    }

    fn client_view(
        mut cursor: u64,
        mut view: zz_protocol::chat::ChatView,
        items: &[AgentStreamItem],
    ) -> Result<(u64, zz_protocol::chat::ChatView), u64> {
        for item in items {
            if item.seq > cursor.saturating_add(1)
                && matches!(
                    item.payload,
                    AgentStreamPayload::SessionReset { restoring: true }
                )
            {
                cursor = item.seq.saturating_sub(1);
            }
            if item.seq <= cursor {
                continue;
            }
            if item.seq > cursor.saturating_add(1) {
                return Err(item.seq);
            }
            cursor = item.seq;
            if let AgentStreamPayload::Chat { event } = &item.payload {
                let _ = view.apply(event);
            }
        }
        Ok((cursor, view))
    }

    #[test]
    fn every_replay_path_at_every_cut_of_a_recording_shows_the_live_chat() {
        use crate::agent::cut_points::{
            Findings, difference, fold, payload_kind, short, sub_view_difference, tapes,
        };
        use zz_protocol::chat::{ChatView, TurnBy, TurnId};

        const REOPENED_AT: u64 = 1000;
        let stale = fold(&[
            ChatEvent::SessionReady {
                session: "stale".to_owned(),
            },
            ChatEvent::TurnStarted {
                turn: TurnId(99),
                by: TurnBy::Agent,
                at: 0,
            },
        ]);
        let mut findings = Findings::new("B fanout replay");
        for tape in tapes() {
            let directory = tempfile::tempdir().expect("journal directory");
            let journal = Arc::new(AgentJournal::open(directory.path()).expect("open journal"));
            let mut reopened = Watched::open(tape, None, Some(REOPENED_AT));
            let mut journalled = Watched::open(tape, Some(Arc::clone(&journal)), None);
            let mut memory = Watched::open(tape, None, None);
            let total = tape
                .lines
                .iter()
                .map(|line| line.payloads.len())
                .sum::<usize>();
            let mut position = 0;
            for (index, line) in tape.lines.iter().enumerate() {
                journal
                    .remove_for(tape.provider, &tape.session)
                    .expect("clear the journal");
                for update in tape.journal_at(index) {
                    journal
                        .append_for(tape.provider, &tape.session, &update)
                        .expect("journal");
                }
                for payload in &line.payloads {
                    position += 1;
                    for watched in [&mut reopened, &mut journalled, &mut memory] {
                        watched.accept(payload);
                    }
                    let at = payload_kind(payload);
                    let mut check = |path: &str,
                                     watched: &mut Watched,
                                     from_seq: u64,
                                     evicted: bool| {
                        let replayed = watched.replay(from_seq, evicted);
                        let (next_seq, items) = (replayed.after, &replayed.asker);
                        let room = replayed.before - from_seq - 1;
                        let rank = (
                            total,
                            position * 10_000 + usize::try_from(room).unwrap_or(0),
                        );
                        let example = |detail: &str| {
                            format!(
                                "{} pos {position}/{total} after {} from_seq {from_seq} next_seq {} room {room}: {detail}",
                                tape.name,
                                short(payload),
                                replayed.before,
                            )
                        };
                        findings.checked();
                        if let Some(diff) = difference(&replayed.straight, &watched.straight) {
                            findings.add(
                                format!("B | {path} | a widening no-op changed the chat"),
                                at.clone(),
                                rank,
                                || example(&diff.describe()),
                            );
                        }
                        match client_view(
                            replayed.before - 1,
                            replayed.straight.clone(),
                            &replayed.others,
                        ) {
                            Ok((cursor, view)) if cursor == next_seq - 1 => {
                                if let Some(diff) = difference(&watched.straight, &view) {
                                    findings.add(
                                        format!("B | {path} | a current viewer's chat changed"),
                                        at.clone(),
                                        rank,
                                        || example(&diff.describe()),
                                    );
                                }
                            }
                            Ok((cursor, _)) => findings.add(
                                format!("B | {path} | a current viewer ends short of the lane"),
                                at.clone(),
                                rank,
                                || example(&format!("cursor {cursor} of {next_seq}")),
                            ),
                            Err(seq) => findings.add(
                                format!("B | {path} | a current viewer sees a gap"),
                                at.clone(),
                                rank,
                                || example(&format!("item {seq} after {}", replayed.before - 1)),
                            ),
                        }
                        let chat = items
                            .iter()
                            .filter_map(|item| match &item.payload {
                                AgentStreamPayload::Chat { event } => Some(event.clone()),
                                _ => None,
                            })
                            .collect::<Vec<_>>();
                        let restart = chat.contains(&ChatEvent::Restart);
                        let prior = match (restart, path) {
                            (true, _) => ChatView::default(),
                            (false, "b") => stale.clone(),
                            (false, _) => watched.known(from_seq),
                        };
                        let (cursor, view) = match client_view(from_seq, prior, items) {
                            Ok(caught) => caught,
                            Err(seq) => {
                                findings.add(
                                    format!("B | {path} | gap in the replay"),
                                    at.clone(),
                                    rank,
                                    || example(&format!("item {seq} after {from_seq}")),
                                );
                                return items.len();
                            }
                        };
                        if items.iter().any(|item| {
                            matches!(&item.payload, AgentStreamPayload::Update { update } if is_turn_mark(update))
                        }) {
                            findings.add(
                                format!("B | {path} | sends a turn mark"),
                                at.clone(),
                                rank,
                                || example("a journalled turn mark reached the asker"),
                            );
                        }
                        if cursor != next_seq - 1 {
                            findings.add(
                                format!("B | {path} | replay ends short of the lane"),
                                at.clone(),
                                rank,
                                || example(&format!("cursor {cursor}")),
                            );
                        }
                        let journal_path = items
                            .iter()
                            .any(|item| matches!(item.payload, AgentStreamPayload::Update { .. }));
                        let rebuilt = !matches!(path, "a" | "b") && !journal_path;
                        let numbers = items.iter().map(|item| item.seq).collect::<Vec<_>>();
                        let first = numbers.first().copied().unwrap_or(next_seq);
                        let widened = next_seq > replayed.before;
                        if rebuilt
                            && (numbers != (first..next_seq).collect::<Vec<_>>()
                                || first <= from_seq
                                || (widened && first != from_seq + 1)
                                || !matches!(
                                    items.first().map(|item| &item.payload),
                                    Some(AgentStreamPayload::SessionReset { restoring: true })
                                ))
                        {
                            findings.add(
                                format!("B | {path} | the rebuild is not numbered from_seq + 1 ..= next_seq - 1"),
                                at.clone(),
                                rank,
                                || example(&format!("numbered {numbers:?}")),
                            );
                        }
                        let (path, diff) = match path {
                            "a" | "b" => (path, difference(&watched.straight, &view)),
                            _ if journal_path => ("c", difference(&watched.straight, &view)),
                            _ if chat == watched.log() => {
                                ("d whole", difference(&watched.straight, &view))
                            }
                            _ if restart => ("d", sub_view_difference(&watched.straight, &view)),
                            _ => (
                                "d without Restart",
                                sub_view_difference(&watched.straight, &view).map(|mut diff| {
                                    diff.at = format!("{}, {}", diff.field, diff.at);
                                    "the asker keeps its old chat".clone_into(&mut diff.field);
                                    diff
                                }),
                            ),
                        };
                        if let Some(diff) = diff {
                            findings.add(
                                format!("B | {path} | {}", diff.field),
                                at.clone(),
                                rank,
                                || example(&diff.describe()),
                            );
                        }
                        items.len()
                    };
                    let next_seq = reopened.next_seq();
                    for from_seq in [0, REOPENED_AT - 2] {
                        check("b", &mut reopened, from_seq, false);
                    }
                    let span = next_seq - (REOPENED_AT - 1);
                    for step in 0..=8 {
                        let from_seq = REOPENED_AT - 1 + span * step / 8;
                        if from_seq < next_seq {
                            check("a", &mut reopened, from_seq, false);
                        }
                    }
                    let count = u64::try_from(check("c", &mut journalled, 0, true)).unwrap_or(0);
                    for short_by in [0, 1] {
                        if let Some(from_seq) = journalled
                            .next_seq()
                            .checked_sub(1 + count)
                            .map(|fits| fits + short_by)
                            .filter(|from_seq| (1..journalled.next_seq()).contains(from_seq))
                        {
                            check("c", &mut journalled, from_seq, true);
                        }
                    }
                    check("d", &mut memory, 0, true);
                    let log = u64::try_from(memory.log().len()).unwrap_or(0);
                    for room in 0..=log + 6 {
                        if let Some(from_seq) = memory.next_seq().checked_sub(room + 1) {
                            check("d", &mut memory, from_seq, true);
                        }
                    }
                }
            }
        }
        findings.finish(&[]);
    }
}
