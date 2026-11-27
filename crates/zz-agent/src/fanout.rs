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
    AgentAutoApprove, AgentPaneWire, AgentPermissionWire, AgentProvider, ClientId,
    ClientInstanceId, MAX_AGENT_PROMPT_BYTES, MAX_AGENT_QUEUED_PROMPTS, MAX_AGENT_RESULT_BYTES,
    MAX_AGENT_UPDATES_BYTES, PaneId, QuotaWindows,
};

use crate::{
    chat_bridge::is_zz_note,
    host::{
        AgentConnectionPhase, AgentHost, AgentPaneSpec, AgentPaneState, AgentTurnFailure,
        AgentTurnWaiter, HostCommand, PromptSource, QueuedPrompt, RelayedAsk,
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
pub struct AgentToolCall {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub status: String,
    pub page: Option<String>,
}

pub enum AgentRequestReply {
    Sessions { client: ClientId, result: String },
}

pub trait AgentPublisher: Send + Sync + 'static {
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

pub const MAX_SENDS_SINCE_USER: u32 = 8;

pub struct AgentRuntime {
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
    pub fn new(
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

    pub fn child_records(&self, provider: AgentProvider, session: &str) -> Vec<ChildRecord> {
        let children = self.children.lock();
        self.read_children(&children, provider, session)
    }

    pub fn update_child(
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

    pub fn sends_since_user(&self, caller: PaneId, target: PaneId) -> u32 {
        self.sends
            .lock()
            .get(&(caller, target))
            .copied()
            .unwrap_or(0)
    }

    pub fn count_send(&self, caller: PaneId, target: PaneId) {
        *self.sends.lock().entry((caller, target)).or_insert(0) += 1;
    }

    pub fn open(&self, pane: PaneId, spec: AgentPaneSpec) -> bool {
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

    pub fn close(&self, pane: PaneId) {
        let _lifecycle = self.lifecycle.lock();
        let _ = self.host.end(pane);
        self.fanout.close_lane(pane);
    }

    pub fn close_and_wait(&self, pane: PaneId) {
        let (thread, generation) = {
            let _lifecycle = self.lifecycle.lock();
            (self.host.close(pane), self.pane_generation(pane))
        };
        if let Some(thread) = thread {
            let _ = thread.join();
            crate::child::join_reapers();
        }
        let _lifecycle = self.lifecycle.lock();
        if self.pane_generation(pane) == generation {
            self.fanout.park_lane(pane);
        }
    }

    pub fn command(&self, pane: PaneId, command: HostCommand) -> bool {
        let _lifecycle = self.lifecycle.lock();
        match self.host.command(pane, command) {
            Ok(()) => true,
            Err(command) => self.fanout.reject_command(pane, command),
        }
    }

    pub fn prompt(&self, pane: PaneId, prompt: AgentPrompt) -> bool {
        self.prompt_with_waiter(pane, prompt, None)
    }

    pub fn prompt_with_waiter(
        &self,
        pane: PaneId,
        prompt: AgentPrompt,
        waiter: Option<AgentTurnWaiter>,
    ) -> bool {
        self.prompt_from(pane, prompt, waiter, PromptSource::Composer, None)
    }

    pub fn command_prompt(
        &self,
        pane: PaneId,
        prompt: AgentPrompt,
        waiter: Option<AgentTurnWaiter>,
        source: PromptSource,
        note: Option<String>,
    ) -> bool {
        self.prompt_from(pane, prompt, waiter, source, note)
    }

    pub fn answer_question(
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
        let (request_id, answers) = crate::host::typed_answer(&pending, request, text)?;
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

    #[cfg(any(test, feature = "test-support"))]
    pub fn queued_prompts(&self, pane: PaneId) -> usize {
        self.host
            .snapshot_state(pane)
            .map_or(0, |state| state.queued_prompts)
    }

    pub fn waiting_prompts(&self, pane: PaneId) -> usize {
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
            ask: None,
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

    pub fn inject(&self, pane: PaneId, prompt: AgentPrompt, ask: Option<(PaneId, u64)>) -> bool {
        let _lifecycle = self.lifecycle.lock();
        let queued = QueuedPrompt {
            prompt,
            waiter: None,
            source: PromptSource::Injected,
            note: None,
            ask: ask.map(|(asker, request)| RelayedAsk {
                request,
                asker: self.host.pane_state(asker),
            }),
        };
        self.host.command(pane, HostCommand::Prompt(queued)).is_ok()
    }

    pub fn chat_text(&self, pane: PaneId, whole: bool) -> String {
        let lanes = self.fanout.lanes.lock();
        let Some(lane) = lanes.get(&pane) else {
            return String::new();
        };
        if !lane.chat.is_empty() {
            return crate::capture::chat_text(lane.chat.events(), whole);
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
        let events = crate::capture::stream_events(updates.iter());
        crate::capture::chat_text(events.iter(), whole)
    }

    pub fn has_pending_permission(&self, pane: PaneId, request: u64) -> bool {
        self.host.snapshot_state(pane).is_some_and(|state| {
            state
                .pending_permissions
                .iter()
                .any(|pending| pending.request_id == request)
        })
    }

    pub fn join(&self, pane: PaneId, waiter: AgentTurnWaiter) -> bool {
        let _lifecycle = self.lifecycle.lock();
        self.host.command(pane, HostCommand::Join(waiter)).is_ok()
    }

    pub fn prewarm(&self) {
        if crate::sandboxed() {
            return;
        }
        self.host.prewarm();
    }

    pub fn reconfigure(&self, config: AgentSpawnConfig) {
        let _lifecycle = self.lifecycle.lock();
        self.host.reconfigure(config);
    }

    pub fn set_auto_approve(&self, pane: PaneId, tier: AgentAutoApprove) {
        self.host.set_auto_approve(pane, tier);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn spawn_config(&self) -> AgentSpawnConfig {
        self.host.config()
    }

    pub fn wire_state(&self, pane: PaneId) -> Option<AgentPaneWire> {
        self.fanout
            .published_state(pane)
            .or_else(|| self.host.snapshot_state(pane).map(|state| wire(&state)))
    }

    pub fn pane_generation(&self, pane: PaneId) -> Option<u64> {
        self.fanout
            .lanes
            .lock()
            .get(&pane)
            .map(|lane| lane.generation)
    }

    pub fn replay(&self, client: ClientId, pane: PaneId, from_seq: u64) {
        self.fanout.replay(client, pane, from_seq);
    }

    pub fn acknowledge_prompt_restore(
        &self,
        owner: ClientInstanceId,
        pane: PaneId,
        reclaim_id: u64,
    ) {
        let _lifecycle = self.lifecycle.lock();
        self.fanout
            .acknowledge_prompt_restore(owner, pane, reclaim_id);
    }

    pub fn shutdown(&self) {
        let lifecycle = self.lifecycle.lock();
        self.fanout.shutdown();
        drop(lifecycle);
        self.host.shutdown();
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_runner_factory(&self, factory: crate::host::PaneRunnerFactory) {
        self.host.set_runner_factory(factory);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn retained_items(&self, pane: PaneId) -> Vec<AgentStreamItem> {
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
                if update["sessionUpdate"] == "config_option_update" || is_zz_note(&update) =>
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
        let event = last(|event| matches!(event, ChatEvent::UsageChanged { .. }))
            .or_else(|| last(|event| matches!(event, ChatEvent::QueueChanged { .. })))
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
            queued: state.own_prompts,
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
        queued_prompts: u32::try_from(state.own_prompts).unwrap_or(u32::MAX),
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
mod stream_tests;
