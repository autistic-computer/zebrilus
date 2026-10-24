use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
};

use agent_client_protocol::schema::v1::{
    PermissionOption, SessionUpdate, StopReason, ToolCallUpdate,
};
use zz_daemon::{AgentPromptOutcome, AgentStreamItem, AgentStreamPayload};
use zz_gpui::{App, Context, Entity, Image, ImageFormat, Task};
use zz_protocol::{
    AgentConnectionPhase, AgentGitSummary, AgentPaneWire, AgentProvider, AgentQuestionAnswer,
    AgentTaskWire, MAX_AGENT_AVAILABLE_COMMANDS, MAX_AGENT_QUEUED_PROMPTS, PaneId,
    agent_stream::AgentQuestion,
};

use crate::{
    agent::attachment,
    mux::client::{AgentRequest, MuxClient},
};

/// A skipped update is described by its `sessionUpdate` tag and nothing else:
/// the payload behind it is adapter-supplied and can be megabytes of tool
/// output, so neither it nor the serde error quoting it belongs in a log line.
const MAX_SESSION_UPDATE_TAG_BYTES: usize = 64;
const MAX_DECODE_ERROR_BYTES: usize = 256;
const UNTAGGED_SESSION_UPDATE: &str = "<untagged>";
/// Tool payloads live in the thread for as long as the pane does, so the
/// reducer caps what it keeps: agents happily emit multi-megabyte outputs.
#[cfg(test)]
const MAX_TOOL_PAYLOAD_BYTES: usize = 512 * 1024;
#[cfg(test)]
const TRUNCATION_MARKER: &str = "… [truncated]";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentConnectionState {
    Starting,
    Restoring,
    Ready,
    Running,
    Cancelling,
    Failed,
    Disconnected,
}

impl AgentConnectionState {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Starting => "STARTING",
            Self::Restoring => "RESTORING",
            Self::Ready => "READY",
            Self::Running => "RUNNING",
            Self::Cancelling => "CANCELLING",
            Self::Failed => "FAILED",
            Self::Disconnected => "OFFLINE",
        }
    }

    pub(crate) const fn accepts_prompt(self) -> bool {
        matches!(self, Self::Ready)
    }

    pub(crate) const fn has_active_turn(self) -> bool {
        matches!(self, Self::Running | Self::Cancelling)
    }
}

#[cfg(test)]
pub(crate) use zz_client::agent_transcript::AgentPermissionOption;
pub(crate) use zz_client::agent_transcript::{
    AgentPermissionKind, AgentPermissionRequest, AgentToolKindModel, AgentToolStatusModel,
    ToolPayload,
};
use zz_client::agent_transcript::{AgentTranscript, capped, is_zz_command};
pub(crate) type AgentThreadEntry = zz_client::agent_transcript::AgentThreadEntry<Arc<Image>>;

pub(crate) use zz_client::agent_completion::AgentCommand;

use zz_client::agent_completion::agent_command_model;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "ACP exposes these independent capabilities"
)]
pub(crate) struct AgentSessionCapabilities {
    pub(crate) load: bool,
    pub(crate) list: bool,
    pub(crate) close: bool,
    pub(crate) delete: bool,
    pub(crate) additional_directories: bool,
    /// A prompt capability rather than a session one, but it arrives in the
    /// same handshake and gates the composer the same way.
    pub(crate) images: bool,
    pub(crate) verbs: bool,
    pub(crate) watch: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentPaneState {
    pub(crate) provider: AgentProvider,
    pub(crate) connection: AgentConnectionState,
    pub(crate) pending_permissions: Arc<[AgentPermissionRequest]>,
    pub(crate) error: Option<Arc<str>>,
    pub(crate) agent_name: Option<Arc<str>>,
    pub(crate) cwd: PathBuf,
    pub(crate) session_id: Option<Arc<str>>,
    pub(crate) session_capabilities: AgentSessionCapabilities,
    pub(crate) available_commands: Arc<[AgentCommand]>,
    pub(crate) usage: Option<(u64, u64)>,
    pub(crate) git: Option<AgentGitSummary>,
    pub(crate) tasks: Arc<[AgentTaskWire]>,
    /// Text `agent-send` routed here, waiting for the pane's view to fold it
    /// into the composer draft.
    pub(crate) pending_composer: Option<Arc<str>>,
    /// Prompts typed during the live turn, waiting for it to settle.
    pub(crate) queued_prompts: usize,
}

#[derive(Debug)]
struct AgentThread {
    transcript: AgentTranscript<Arc<Image>>,
    provider: AgentProvider,
    connection: AgentConnectionState,
    submitted: bool,
    error: Option<Arc<str>>,
    agent_name: Option<Arc<str>>,
    available_commands: Arc<[AgentCommand]>,
    usage: Option<(u64, u64)>,
    git: Option<AgentGitSummary>,
    tasks: Arc<[AgentTaskWire]>,
    cwd: PathBuf,
    session_id: Option<Arc<str>>,
    session_capabilities: AgentSessionCapabilities,
    session_reset: bool,
    /// How many `session/update`s this build could not read. ACP reserves
    /// unknown `sessionUpdate` values for future variants, so skipping one is
    /// forward compatibility rather than a fault — the running count is what
    /// keeps a silent skip diagnosable. Survives a session reset: it describes
    /// the adapter zz is talking to, not the conversation.
    unknown_updates: u64,
}

impl AgentThread {
    fn new(provider: AgentProvider, cwd: PathBuf, session_id: Option<String>) -> Self {
        Self {
            transcript: AgentTranscript::new(transcript_image),
            provider,
            connection: AgentConnectionState::Starting,
            submitted: false,
            error: None,
            agent_name: None,
            available_commands: Arc::from([]),
            usage: None,
            git: None,
            tasks: Arc::from([]),
            cwd,
            session_id: session_id.map(Arc::from),
            session_capabilities: AgentSessionCapabilities::default(),
            session_reset: false,
            unknown_updates: 0,
        }
    }

    fn snapshot(&self) -> AgentPaneState {
        AgentPaneState {
            provider: self.provider,
            connection: self.connection,
            pending_permissions: self.transcript.permissions().clone(),
            error: self.error.clone(),
            agent_name: self.agent_name.clone(),
            cwd: self.cwd.clone(),
            session_id: self.session_id.clone(),
            session_capabilities: self.session_capabilities,
            available_commands: self.available_commands.clone(),
            usage: self.usage,
            git: self.git.clone(),
            tasks: self.tasks.clone(),
            pending_composer: None,
            queued_prompts: 0,
        }
    }

    fn reset_for_open(&mut self, restoring: bool) {
        self.session_reset = true;
        self.transcript.reset();
        self.connection = if restoring {
            AgentConnectionState::Restoring
        } else {
            AgentConnectionState::Starting
        };
        self.error = None;
        self.available_commands = Arc::from([]);
        self.usage = None;
        self.git = None;
        self.tasks = Arc::from([]);
    }

    fn prompt_refusal(&self, has_images: bool) -> Option<Arc<str>> {
        if !self.connection.accepts_prompt() {
            return Some(Arc::from(format!(
                "agent is not ready ({})",
                self.connection.label().to_ascii_lowercase()
            )));
        }
        self.image_refusal(has_images)
    }

    /// Checked before a prompt is queued as well as before it is sent: an agent
    /// that takes no images will not take them a turn later either.
    fn image_refusal(&self, has_images: bool) -> Option<Arc<str>> {
        (has_images && !self.session_capabilities.images)
            .then(|| Arc::<str>::from("this agent does not accept images"))
    }

    fn begin_prompt(&mut self, prompt: String, images: Vec<Arc<Image>>) {
        self.transcript.begin_prompt(prompt, images);
        self.connection = AgentConnectionState::Running;
        self.submitted = true;
        self.error = None;
    }

    fn apply_update(&mut self, update: SessionUpdate) {
        self.apply_flat_update(update);
    }

    fn apply_runtime_update(&mut self, update: SessionUpdate) {
        self.apply_update(update);
    }

    fn apply_flat_update(&mut self, update: SessionUpdate) {
        match update {
            update @ (SessionUpdate::UserMessageChunk(_)
            | SessionUpdate::AgentMessageChunk(_)
            | SessionUpdate::AgentThoughtChunk(_)
            | SessionUpdate::ToolCall(_)
            | SessionUpdate::ToolCallUpdate(_)
            | SessionUpdate::Plan(_)) => self.transcript.apply_update(update),
            SessionUpdate::AvailableCommandsUpdate(update) => {
                self.available_commands = update
                    .available_commands
                    .into_iter()
                    .take(MAX_AGENT_AVAILABLE_COMMANDS)
                    .map(agent_command_model)
                    .collect::<Vec<_>>()
                    .into();
            }
            SessionUpdate::UsageUpdate(update) => {
                self.usage = Some((update.used, update.size));
            }
            _ => {}
        }
    }
}

#[derive(Debug)]
enum RuntimeEvent {
    Ready {
        agent_name: String,
        session_capabilities: AgentSessionCapabilities,
    },
    SessionReset {
        pane: PaneId,
        restoring: bool,
    },
    SessionReady {
        pane: PaneId,
        session_id: String,
    },
    SessionSwitched {
        pane: PaneId,
        session_id: String,
        cwd: PathBuf,
        replay: Vec<SessionUpdate>,
    },
    SessionSwitchFailed {
        pane: PaneId,
        message: String,
    },
    SessionUpdate {
        pane: PaneId,
        update: SessionUpdate,
    },
    PermissionRequested {
        pane: PaneId,
        request_id: u64,
        tool_call: ToolCallUpdate,
        options: Vec<PermissionOption>,
        questions: Vec<AgentQuestion>,
    },
    PermissionResolved {
        pane: PaneId,
        request_id: u64,
        canceled: bool,
    },
    PromptFinished {
        pane: PaneId,
        result: Result<StopReason, String>,
    },
    PaneFailed {
        pane: PaneId,
        message: String,
    },
    PromptsReclaimed {
        pane: PaneId,
        count: usize,
        text: String,
        images: Vec<Arc<Image>>,
    },
}

#[derive(Default)]
struct PaneViewport {
    last_applied: u64,
    queued_prompts: usize,
    conversation_epoch: u64,
    last_reclaim_id: u64,
}

pub struct AgentController {
    mux: Option<Entity<MuxClient>>,
    panes: BTreeMap<PaneId, AgentThread>,
    viewports: BTreeMap<PaneId, PaneViewport>,
    pending_composer: BTreeMap<PaneId, String>,
    pending_images: BTreeMap<PaneId, Vec<Arc<Image>>>,
    retained_panes: BTreeSet<PaneId>,
    shutting_down: bool,
    /// Sends go to the lead's host instead of the attached one.
    lead: bool,
}

impl Default for AgentController {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentController {
    pub(crate) fn new() -> Self {
        Self {
            mux: None,
            panes: BTreeMap::new(),
            viewports: BTreeMap::new(),
            pending_composer: BTreeMap::new(),
            pending_images: BTreeMap::new(),
            retained_panes: BTreeSet::new(),
            shutting_down: false,
            lead: false,
        }
    }

    /// The controller of the lead agent's one pane, which may live on any
    /// host: its requests ride [`MuxClient::send_lead_request`].
    pub(crate) fn for_lead() -> Self {
        Self {
            lead: true,
            ..Self::new()
        }
    }

    pub(crate) fn attach_mux(&mut self, mux: Entity<MuxClient>) {
        self.mux = Some(mux);
    }

    pub(crate) fn pane_state(&self, pane: PaneId) -> Option<AgentPaneState> {
        self.panes.get(&pane).map(|thread| {
            let mut state = thread.snapshot();
            state.pending_composer = self
                .pending_composer
                .get(&pane)
                .map(|text| Arc::from(text.as_str()));
            state.queued_prompts = self.queued_count(pane);
            state
        })
    }

    pub(crate) fn queued_count(&self, pane: PaneId) -> usize {
        self.viewports
            .get(&pane)
            .map_or(0, |viewport| viewport.queued_prompts)
    }

    pub(crate) fn conversation_epoch(&self, pane: PaneId) -> u64 {
        self.viewports
            .get(&pane)
            .map_or(0, |viewport| viewport.conversation_epoch)
    }

    pub(crate) fn pane_entries(&self, pane: PaneId) -> Option<(&[AgentThreadEntry], &[u64], u64)> {
        self.panes.get(&pane).map(|thread| {
            (
                thread.transcript.entries(),
                thread.transcript.entry_revisions(),
                thread.transcript.revision(),
            )
        })
    }

    pub(crate) fn pane_entry_changes(&self, pane: PaneId, since: u64) -> Option<Vec<usize>> {
        self.panes.get(&pane)?.transcript.changed_entries(since)
    }

    pub(crate) fn registered_panes(&self) -> BTreeSet<PaneId> {
        self.panes.keys().copied().collect()
    }

    pub(crate) fn ensure_pane(&mut self, pane: PaneId, provider: AgentProvider, cwd: PathBuf) {
        let provider_changed = self
            .panes
            .get(&pane)
            .is_some_and(|thread| thread.provider != provider);
        if provider_changed || !self.panes.contains_key(&pane) {
            self.panes
                .insert(pane, AgentThread::new(provider, cwd, None));
        }
        self.retained_panes.insert(pane);
        let viewport = self.viewports.entry(pane).or_default();
        if provider_changed {
            viewport.conversation_epoch = viewport.conversation_epoch.saturating_add(1);
            viewport.last_applied = 0;
        }
    }

    pub(crate) fn retain_panes(&mut self, retained: &BTreeSet<PaneId>, cx: &mut Context<Self>) {
        let removed = self
            .retained_panes
            .difference(retained)
            .copied()
            .collect::<Vec<_>>();
        for pane in removed {
            self.panes.remove(&pane);
        }
        self.viewports.retain(|pane, _| retained.contains(pane));
        self.pending_composer
            .retain(|pane, _| retained.contains(pane));
        self.pending_images
            .retain(|pane, _| retained.contains(pane));
        let changed = self.retained_panes != *retained;
        self.retained_panes.clone_from(retained);
        if changed {
            cx.notify();
        }
    }

    /// Send the prompt. The daemon queues it behind a turn already running and
    /// hands it back through the stream if that turn is stopped, so the
    /// at-least-once rule holds without a client-side queue.
    pub(crate) fn prompt(
        &mut self,
        pane: PaneId,
        text: &str,
        images: Vec<Arc<Image>>,
        cx: &mut Context<Self>,
    ) -> Result<(), Arc<str>> {
        let text = text.trim().to_owned();
        if text.is_empty() && images.is_empty() {
            return Ok(());
        }
        let Some(thread) = self.panes.get(&pane) else {
            return Err(Arc::from("the lens is not open"));
        };
        if let Some(refusal) = thread.image_refusal(!images.is_empty()) {
            return Err(refusal);
        }
        let command = thread.session_capabilities.verbs && is_zz_command(&text);
        let active = thread.connection.has_active_turn();
        if !active && let Some(refusal) = thread.prompt_refusal(!images.is_empty()) {
            return Err(refusal);
        }
        let queueing = active && !command;
        if queueing && self.queued_count(pane) >= MAX_AGENT_QUEUED_PROMPTS {
            return Err(Arc::from(
                "finish or unqueue one of the four queued prompts first",
            ));
        }
        let wire_images = attachment::wire_images(&images);
        if !self.send(
            pane,
            AgentRequest::Prompt {
                text: text.clone(),
                images: wire_images,
            },
            cx,
        ) {
            if let Some(thread) = self.panes.get_mut(&pane) {
                thread.connection = AgentConnectionState::Failed;
                thread.error = Some(Arc::from("agent daemon is not connected"));
            }
            cx.notify();
            return Err(Arc::from("agent daemon is not connected"));
        }
        if command {
            return Ok(());
        }
        if queueing {
            // The daemon publishes the queue depth; showing it immediately keeps
            // the composer's Queue affordance honest between publications.
            self.viewport_mut(pane).queued_prompts += 1;
            cx.notify();
            return Ok(());
        }
        if let Some(thread) = self.panes.get_mut(&pane) {
            thread.begin_prompt(text, images);
        }
        cx.notify();
        Ok(())
    }

    fn viewport_mut(&mut self, pane: PaneId) -> &mut PaneViewport {
        self.viewports.entry(pane).or_default()
    }

    /// Empty the pane's queue back into its composer draft on the user's ask.
    /// The prompts themselves come back through the stream.
    pub(crate) fn unqueue_prompts(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        if self.queued_count(pane) == 0 {
            return;
        }
        self.send(pane, AgentRequest::Unqueue, cx);
    }

    fn queue_composer_text(&mut self, pane: PaneId, text: &str) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        let pending = self.pending_composer.entry(pane).or_default();
        if !pending.is_empty() {
            pending.push('\n');
        }
        pending.push_str(text);
        true
    }

    /// Hand the queued composer text to the pane's view, which owns the draft.
    pub(crate) fn take_pending_composer(&mut self, pane: PaneId) -> Option<String> {
        self.pending_composer
            .remove(&pane)
            .filter(|text| !text.is_empty())
    }

    /// Hand back the images a reclaimed prompt carried, for the view to put in
    /// its attachment strip again.
    pub(crate) fn take_pending_images(&mut self, pane: PaneId) -> Vec<Arc<Image>> {
        self.pending_images.remove(&pane).unwrap_or_default()
    }

    pub(crate) fn cancel(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        let Some(thread) = self.panes.get(&pane) else {
            return;
        };
        if !thread.connection.has_active_turn() {
            return;
        }
        if self.send(pane, AgentRequest::Cancel, cx) {
            self.panes
                .get_mut(&pane)
                .expect("pane checked above")
                .connection = AgentConnectionState::Cancelling;
        } else if let Some(thread) = self.panes.get_mut(&pane) {
            thread.error = Some(Arc::from("agent daemon is not connected"));
        }
        cx.notify();
    }

    pub(crate) fn respond_permission(
        &mut self,
        pane: PaneId,
        request_id: u64,
        option_id: Option<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        let canceled = option_id.is_none();
        if self.send(
            pane,
            AgentRequest::RespondPermission {
                request_id,
                option_id,
            },
            cx,
        ) {
            if let Some(thread) = self.panes.get_mut(&pane) {
                thread.transcript.resolve_permission(request_id, canceled);
            }
            cx.notify();
            return true;
        }
        false
    }

    pub(crate) fn answer_question(
        &mut self,
        pane: PaneId,
        request_id: u64,
        answers: Vec<AgentQuestionAnswer>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.send(
            pane,
            AgentRequest::AnswerQuestion {
                request_id,
                answers,
            },
            cx,
        ) {
            return false;
        }
        if let Some(thread) = self.panes.get_mut(&pane) {
            thread.transcript.resolve_permission(request_id, false);
        }
        cx.notify();
        true
    }

    pub(crate) fn stop_task(
        &mut self,
        pane: PaneId,
        task_id: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let known = self
            .panes
            .get(&pane)
            .is_some_and(|thread| thread.tasks.iter().any(|task| task.id == task_id));
        known && self.send(pane, AgentRequest::StopTask { task_id }, cx)
    }

    pub(crate) fn tool_parent_entry(&self, pane: PaneId, entry_id: u64) -> Option<u64> {
        let transcript = &self.panes.get(&pane)?.transcript;
        transcript
            .tool_parent(entry_id)
            .and_then(|parent| transcript.tool_entry(parent))
    }

    pub(crate) fn tool_entry(&self, pane: PaneId, tool_call_id: &str) -> Option<u64> {
        self.panes.get(&pane)?.transcript.tool_entry(tool_call_id)
    }

    pub(crate) fn is_shutting_down(&self) -> bool {
        self.shutting_down
    }

    pub(crate) fn is_shutdown_complete(&self) -> bool {
        self.shutting_down
    }

    /// Nothing to wind down: the adapters are the daemon's children and a
    /// running turn is meant to outlive the window.
    pub(crate) fn shutdown(&mut self, _cx: &mut Context<Self>) -> Task<bool> {
        self.shutting_down = true;
        Task::ready(true)
    }

    fn send(&self, pane: PaneId, request: AgentRequest, cx: &App) -> bool {
        self.mux.as_ref().is_some_and(|mux| {
            let mux = mux.read(cx);
            if self.lead {
                mux.send_lead_request(request)
            } else {
                mux.send_agent_request(pane, request)
            }
        })
    }

    /// Apply one batch of stream items, in seq order, to the pane's reducer.
    /// A pane's viewport and its transcript are dropped together, so an item at
    /// or below the applied cursor is one this transcript already holds: an
    /// attach clears the mux client's cursor and replays from the top, and the
    /// pane it replays into may be the same one that reduced those items.
    pub(crate) fn apply_stream_items(
        &mut self,
        pane: PaneId,
        items: Vec<AgentStreamItem>,
        cx: &mut Context<Self>,
    ) {
        if !self.panes.contains_key(&pane) {
            return;
        }
        let mut changed = false;
        for item in items {
            if item.seq <= self.viewport_mut(pane).last_applied {
                continue;
            }
            self.viewport_mut(pane).last_applied = item.seq;
            if let AgentStreamPayload::StateSynced { state } = &item.payload {
                self.apply_pane_state(pane, state, cx);
                continue;
            }
            let Some(event) = self.translate_stream_payload(pane, item.payload, cx) else {
                continue;
            };
            changed |= self.handle_runtime_event(pane, event, cx);
        }
        if changed {
            cx.notify();
        }
    }

    /// Adopt the daemon's published pane state: connection phase, queue depth,
    /// and the permission request a late-attaching client has to see.
    pub(crate) fn apply_pane_state(
        &mut self,
        pane: PaneId,
        state: &AgentPaneWire,
        cx: &mut Context<Self>,
    ) {
        let Some(before) = self.pane_state(pane) else {
            return;
        };
        self.viewport_mut(pane).queued_prompts = state.queued_prompts as usize;
        let thread = self.panes.get_mut(&pane).expect("pane checked above");
        let was_failed = thread.connection == AgentConnectionState::Failed;
        match &state.phase {
            AgentConnectionPhase::Starting => {
                thread.connection = AgentConnectionState::Starting;
                thread.error = state.error.as_deref().map(Arc::from);
            }
            AgentConnectionPhase::Ready => {
                let settled = matches!(
                    thread.connection,
                    AgentConnectionState::Starting
                        | AgentConnectionState::Failed
                        | AgentConnectionState::Disconnected
                ) || (!thread.submitted && thread.connection.has_active_turn());
                if settled {
                    thread.connection = AgentConnectionState::Ready;
                }
                if was_failed || state.error.is_some() {
                    thread.error = state.error.as_deref().map(Arc::from);
                }
            }
            AgentConnectionPhase::Running | AgentConnectionPhase::AwaitingPermission => {
                thread.submitted = false;
                if !thread.connection.has_active_turn() {
                    thread.connection = AgentConnectionState::Running;
                }
                thread.error = state.error.as_deref().map(Arc::from);
            }
            AgentConnectionPhase::Failed { message } => {
                thread.connection = AgentConnectionState::Failed;
                thread.error = Some(Arc::from(message.as_str()));
                thread.transcript.fail_inflight();
            }
        }
        thread.git.clone_from(&state.git);
        if *thread.tasks != *state.tasks {
            thread.tasks = state.tasks.clone().into();
        }
        thread.transcript.clear_permissions();
        if let Some(permission) = &state.pending_permission
            && let Some((tool_call, options, questions)) =
                decode_permission_payload(&permission.payload)
        {
            thread.transcript.request_questions(
                permission.request_id,
                tool_call,
                options,
                questions,
            );
        }
        if self.pane_state(pane).as_ref() != Some(&before) {
            cx.notify();
        }
    }

    /// Turn one wire item into the reducer's input. `None` is an item the
    /// reducer has nothing to do with, or one whose payload did not survive
    /// re-typing — the stream carries the ACP SDK's JSON verbatim.
    fn translate_stream_payload(
        &mut self,
        pane: PaneId,
        payload: AgentStreamPayload,
        cx: &App,
    ) -> Option<RuntimeEvent> {
        Some(match payload {
            AgentStreamPayload::Ready {
                agent_name,
                capabilities,
                ..
            } => RuntimeEvent::Ready {
                agent_name,
                session_capabilities: AgentSessionCapabilities {
                    load: capabilities.load,
                    list: capabilities.list,
                    close: capabilities.close,
                    delete: capabilities.delete,
                    additional_directories: capabilities.additional_directories,
                    images: capabilities.images,
                    verbs: capabilities.verbs,
                    watch: capabilities.watch,
                },
            },
            AgentStreamPayload::SessionReset { restoring } => {
                RuntimeEvent::SessionReset { pane, restoring }
            }
            AgentStreamPayload::SessionReady { session_id, .. } => {
                RuntimeEvent::SessionReady { pane, session_id }
            }
            AgentStreamPayload::StateSynced { .. }
            | AgentStreamPayload::PromptAccepted { .. }
            | AgentStreamPayload::TasksChanged { .. }
            | AgentStreamPayload::Activity { .. }
            | AgentStreamPayload::SessionsListed { .. }
            | AgentStreamPayload::SessionListFailed { .. }
            | AgentStreamPayload::SessionDeleted { .. }
            | AgentStreamPayload::SessionDeleteFailed { .. }
            | AgentStreamPayload::Authenticated
            | AgentStreamPayload::ConfigOptionsChanged { .. }
            | AgentStreamPayload::ModeChanged { .. } => {
                return None;
            }
            AgentStreamPayload::SessionSwitched {
                session_id,
                cwd,
                replay,
                ..
            } => RuntimeEvent::SessionSwitched {
                pane,
                session_id,
                cwd,
                replay: replay
                    .into_iter()
                    .filter_map(|update| self.decode_session_update(pane, update))
                    .collect(),
            },
            AgentStreamPayload::SessionSwitchFailed { message }
            | AgentStreamPayload::SettingFailed { message, .. } => {
                RuntimeEvent::SessionSwitchFailed { pane, message }
            }
            AgentStreamPayload::Update { update } => RuntimeEvent::SessionUpdate {
                pane,
                update: self.decode_session_update(pane, update)?,
            },
            AgentStreamPayload::PermissionRequested {
                request_id,
                tool_call,
                options,
                questions,
            } => RuntimeEvent::PermissionRequested {
                pane,
                request_id,
                tool_call: decode_value(tool_call)?,
                options: decode_value(options)?,
                questions,
            },
            AgentStreamPayload::PermissionResolved {
                request_id,
                canceled,
            } => RuntimeEvent::PermissionResolved {
                pane,
                request_id,
                canceled,
            },
            AgentStreamPayload::PromptFinished { outcome, .. } => RuntimeEvent::PromptFinished {
                pane,
                result: match outcome {
                    AgentPromptOutcome::Finished { stop_reason } => Ok(decode_value(stop_reason)?),
                    AgentPromptOutcome::Failed { message } => Err(message),
                },
            },
            AgentStreamPayload::TurnStarted { .. } => return None,
            AgentStreamPayload::PaneFailed { message }
            | AgentStreamPayload::AuthenticationFailed { message } => {
                RuntimeEvent::PaneFailed { pane, message }
            }
            AgentStreamPayload::PromptsReclaimed { prompts } => {
                let count = prompts.len();
                let mut text = String::new();
                let mut images = Vec::new();
                for prompt in prompts {
                    if !prompt.text.trim().is_empty() {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(&prompt.text);
                    }
                    images.extend(prompt.images.iter().filter_map(attachment::inbound_image));
                }
                RuntimeEvent::PromptsReclaimed {
                    pane,
                    count,
                    text,
                    images,
                }
            }
            AgentStreamPayload::PromptsRestored {
                reclaim_id,
                prompts,
            } => {
                let client = self
                    .mux
                    .as_ref()
                    .and_then(|mux| mux.read(cx).client_instance_id());
                let prompts = prompts
                    .into_iter()
                    .filter(|prompt| prompt.owner.0 == 0 || Some(prompt.owner) == client)
                    .collect::<Vec<_>>();
                if prompts.is_empty() {
                    return None;
                }
                {
                    let viewport = self.viewport_mut(pane);
                    if reclaim_id <= viewport.last_reclaim_id {
                        return None;
                    }
                    viewport.last_reclaim_id = reclaim_id;
                }
                _ = self.send(
                    pane,
                    AgentRequest::AcknowledgePromptRestore { reclaim_id },
                    cx,
                );
                let count = prompts.len();
                let mut text = String::new();
                let mut images = Vec::new();
                for prompt in prompts {
                    if !prompt.text.trim().is_empty() {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(&prompt.text);
                    }
                    images.extend(prompt.images.iter().filter_map(attachment::inbound_image));
                }
                RuntimeEvent::PromptsReclaimed {
                    pane,
                    count,
                    text,
                    images,
                }
            }
        })
    }

    /// Re-type one `session/update`, skipping what this build cannot read.
    /// ACP reserves unknown `sessionUpdate` values for future variants and
    /// `_`-prefixed ones for adapter extensions, so a shape zz does not know is
    /// forward compatibility, not a fault: the item is counted on the pane and
    /// dropped on its own, leaving the rest of its batch — and any message it
    /// interrupts mid-coalesce — untouched. Only the discriminant and a clipped
    /// error reach the log; the payload can be megabytes of tool output.
    fn decode_session_update(
        &mut self,
        pane: PaneId,
        update: serde_json::Value,
    ) -> Option<SessionUpdate> {
        let tag = session_update_tag(&update);
        match serde_json::from_value(update) {
            Ok(update) => Some(update),
            Err(error) => {
                let skipped = self.panes.get_mut(&pane).map_or(0, |thread| {
                    thread.unknown_updates = thread.unknown_updates.saturating_add(1);
                    thread.unknown_updates
                });
                log::debug!(
                    target: "zz::agent",
                    "skipping a session update zz cannot read pane={pane} tag={tag} skipped={skipped}: {}",
                    log_excerpt(&error.to_string(), MAX_DECODE_ERROR_BYTES)
                );
                None
            }
        }
    }

    fn handle_runtime_event(
        &mut self,
        runtime_pane: PaneId,
        event: RuntimeEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut changed_pane = None;
        match event {
            RuntimeEvent::Ready {
                agent_name,
                session_capabilities,
            } => {
                if let Some(thread) = self.panes.get_mut(&runtime_pane) {
                    thread.agent_name = Some(Arc::from(agent_name));
                    thread.session_capabilities = session_capabilities;
                    changed_pane = Some(runtime_pane);
                }
            }
            RuntimeEvent::SessionReset { pane, restoring } => {
                let viewport = self.viewport_mut(pane);
                viewport.conversation_epoch = viewport.conversation_epoch.saturating_add(1);
                if let Some(thread) = self.panes.get_mut(&pane) {
                    thread.reset_for_open(restoring);
                    changed_pane = Some(pane);
                }
            }
            RuntimeEvent::SessionReady { pane, session_id } => {
                if let Some(thread) = self.panes.get_mut(&pane) {
                    thread.session_id = Some(Arc::from(session_id));
                    if thread.connection == AgentConnectionState::Restoring {
                        thread.transcript.finish_replay();
                    }
                    thread.session_reset = false;
                    thread.transcript.finish_turn();
                    thread.connection = AgentConnectionState::Ready;
                    thread.error = None;
                    changed_pane = Some(pane);
                }
            }
            RuntimeEvent::SessionSwitched {
                pane,
                session_id,
                cwd,
                replay,
            } => {
                if self
                    .panes
                    .get(&pane)
                    .is_some_and(|thread| !thread.session_reset)
                {
                    let viewport = self.viewport_mut(pane);
                    viewport.conversation_epoch = viewport.conversation_epoch.saturating_add(1);
                }
                if let Some(thread) = self.panes.get_mut(&pane) {
                    if !thread.session_reset {
                        thread.reset_for_open(true);
                    }
                    thread.cwd = cwd;
                    for update in replay {
                        thread.apply_update(update);
                    }
                    thread.transcript.finish_replay();
                    thread.session_reset = false;
                    thread.transcript.finish_turn();
                    thread.session_id = Some(Arc::from(session_id));
                    thread.connection = AgentConnectionState::Ready;
                    thread.error = None;
                    changed_pane = Some(pane);
                }
            }
            RuntimeEvent::SessionSwitchFailed { pane, message } => {
                if let Some(thread) = self.panes.get_mut(&pane) {
                    if matches!(
                        thread.connection,
                        AgentConnectionState::Starting | AgentConnectionState::Restoring
                    ) {
                        thread.connection = AgentConnectionState::Ready;
                    }
                    if thread.connection != AgentConnectionState::Failed {
                        thread.error = Some(Arc::from(message));
                    }
                    changed_pane = Some(pane);
                }
            }
            RuntimeEvent::SessionUpdate { pane, update } => {
                if let Some(thread) = self.panes.get_mut(&pane) {
                    thread.apply_runtime_update(update);
                    changed_pane = Some(pane);
                }
            }
            RuntimeEvent::PermissionRequested {
                pane,
                request_id,
                tool_call,
                options,
                questions,
            } => {
                if let Some(thread) = self.panes.get_mut(&pane) {
                    thread
                        .transcript
                        .request_questions(request_id, tool_call, options, questions);
                    changed_pane = Some(pane);
                }
            }
            RuntimeEvent::PermissionResolved {
                pane,
                request_id,
                canceled,
            } => {
                if let Some(thread) = self.panes.get_mut(&pane) {
                    thread.transcript.resolve_permission(request_id, canceled);
                    changed_pane = Some(pane);
                }
            }
            RuntimeEvent::PromptFinished { pane, result } => {
                if let Some(thread) = self.panes.get_mut(&pane) {
                    thread.submitted = false;
                    match result {
                        Ok(StopReason::Cancelled) => {
                            thread.connection = AgentConnectionState::Ready;
                            thread.error = None;
                            thread.transcript.cancel_inflight();
                        }
                        Ok(_) => {
                            thread.connection = AgentConnectionState::Ready;
                            thread.error = None;
                            thread.transcript.finish_turn();
                        }
                        Err(error) => {
                            thread.connection = AgentConnectionState::Ready;
                            thread.error = Some(Arc::from(error));
                            thread.transcript.fail_inflight();
                        }
                    }
                    changed_pane = Some(pane);
                }
            }
            RuntimeEvent::PaneFailed { pane, message } => {
                if let Some(thread) = self.panes.get_mut(&pane) {
                    thread.connection = AgentConnectionState::Failed;
                    thread.error = Some(Arc::from(message));
                    thread.transcript.fail_inflight();
                    changed_pane = Some(pane);
                }
            }
            RuntimeEvent::PromptsReclaimed {
                pane,
                count,
                text,
                images,
            } => {
                let viewport = self.viewport_mut(pane);
                viewport.queued_prompts = viewport.queued_prompts.saturating_sub(count);
                if let Some(thread) = self.panes.get_mut(&pane) {
                    thread.submitted = false;
                }
                self.queue_composer_text(pane, &text);
                if !images.is_empty() {
                    self.pending_images.entry(pane).or_default().extend(images);
                }
                changed_pane = Some(pane);
            }
        }
        let _ = cx;
        changed_pane.is_some()
    }
}

fn decode_value<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Option<T> {
    serde_json::from_value(value)
        .map_err(|error| {
            log::warn!(
                target: "zz::agent",
                "dropping an agent payload zz could not re-type: {}",
                log_excerpt(&error.to_string(), MAX_DECODE_ERROR_BYTES)
            );
        })
        .ok()
}

fn session_update_tag(update: &serde_json::Value) -> String {
    log_excerpt(
        update
            .get("sessionUpdate")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(UNTAGGED_SESSION_UPDATE),
        MAX_SESSION_UPDATE_TAG_BYTES,
    )
}

fn log_excerpt(text: &str, max_bytes: usize) -> String {
    capped(
        text.chars()
            .filter(|character| !character.is_control())
            .collect(),
        max_bytes,
    )
}

fn decode_state_blob<T: serde::de::DeserializeOwned>(blob: &str) -> Option<T> {
    if blob.is_empty() {
        return None;
    }
    serde_json::from_str(blob)
        .map_err(|error| {
            log::warn!(target: "zz::agent", "dropping an agent state blob zz could not re-type: {error}");
        })
        .ok()
}

fn decode_permission_payload(
    payload: &str,
) -> Option<(ToolCallUpdate, Vec<PermissionOption>, Vec<AgentQuestion>)> {
    let value = decode_state_blob::<serde_json::Value>(payload)?;
    let tool_call = decode_value(value.get("toolCall")?.clone())?;
    let options = decode_value(value.get("options")?.clone())?;
    let questions = match value.get("questions") {
        Some(questions) => decode_value(questions.clone())?,
        None => Vec::new(),
    };
    Some((tool_call, options, questions))
}

fn transcript_image(mime: &str, bytes: Vec<u8>) -> Option<Arc<Image>> {
    Some(Arc::new(Image::from_bytes(
        ImageFormat::from_mime_type(mime)?,
        bytes,
    )))
}

#[cfg(test)]
fn split_inline_images(text: &str) -> (String, Vec<Arc<Image>>) {
    zz_client::agent_transcript::split_inline_images(text, transcript_image)
}

#[cfg(test)]
mod tests {
    use agent_client_protocol::schema::v1::{
        AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate, ContentBlock,
        ContentChunk, Diff, ImageContent, MessageId, PermissionOptionKind, Plan, PlanEntry,
        PlanEntryPriority, PlanEntryStatus, Terminal, TextContent, ToolCall, ToolCallContent,
        ToolCallLocation, ToolCallStatus, ToolCallUpdateFields, ToolKind,
    };
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use std::{cell::RefCell, rc::Rc};
    use zz_protocol::{MAX_AGENT_PERMISSION_OPTIONS, MAX_AGENT_TOOL_CONTENT_ITEMS};

    use zz_gpui::{AppContext as _, TestAppContext};

    use super::*;

    fn thread() -> AgentThread {
        AgentThread::new(AgentProvider::Codex, PathBuf::from("/workspace"), None)
    }

    #[test]
    fn reducer_coalesces_message_chunks_by_protocol_id() {
        let mut thread = thread();
        thread.apply_update(SessionUpdate::AgentMessageChunk(
            ContentChunk::new(ContentBlock::Text(TextContent::new("hello ")))
                .message_id(MessageId::new("message-1")),
        ));
        let entry_id = thread.transcript.entries()[0].id();
        let first_revision = thread.transcript.entry_revisions()[0];
        thread.apply_update(SessionUpdate::AgentMessageChunk(
            ContentChunk::new(ContentBlock::Text(TextContent::new("world")))
                .message_id(MessageId::new("message-1")),
        ));

        assert_eq!(thread.transcript.entries().len(), 1);
        assert_eq!(
            thread.transcript.entry_revisions().len(),
            thread.transcript.entries().len()
        );
        assert_eq!(thread.transcript.entries()[0].id(), entry_id);
        assert!(thread.transcript.entry_revisions()[0] > first_revision);
        assert!(matches!(
            &thread.transcript.entries()[0],
            AgentThreadEntry::Assistant { markdown, .. } if markdown == "hello world"
        ));
    }

    #[test]
    fn reducer_tracks_available_commands() {
        let mut thread = thread();
        thread.apply_update(SessionUpdate::AvailableCommandsUpdate(
            AvailableCommandsUpdate::new(vec![
                AvailableCommand::new("review", "Review changes").input(
                    AvailableCommandInput::Unstructured(
                        agent_client_protocol::schema::v1::UnstructuredCommandInput::new(
                            "optional instructions",
                        ),
                    ),
                ),
                AvailableCommand::new("brainstorm", "Explore an idea"),
            ]),
        ));
        assert!(matches!(
            thread.available_commands.as_ref(),
            [AgentCommand {
                name,
                input_hint: Some(hint),
                ..
            }, AgentCommand {
                name: second_name,
                ..
            }] if name == "review"
                && hint == "optional instructions"
                && second_name == "brainstorm"
        ));
    }

    #[test]
    fn adapter_controlled_ui_collections_are_bounded() {
        let mut thread = thread();
        let commands = (0..MAX_AGENT_AVAILABLE_COMMANDS + 8)
            .map(|index| AvailableCommand::new(format!("command-{index}"), "run it"))
            .collect();
        thread.apply_update(SessionUpdate::AvailableCommandsUpdate(
            AvailableCommandsUpdate::new(commands),
        ));
        assert_eq!(
            thread.available_commands.len(),
            MAX_AGENT_AVAILABLE_COMMANDS
        );

        let content = (0..MAX_AGENT_TOOL_CONTENT_ITEMS + 8)
            .map(|index| ContentBlock::Text(TextContent::new(index.to_string())).into())
            .collect::<Vec<_>>();
        thread.apply_update(SessionUpdate::ToolCall(
            ToolCall::new("bounded", "Bounded").content(content),
        ));
        assert!(
            matches!(thread.transcript.entries().last(), Some(AgentThreadEntry::Tool { output, .. }) if output.len() == MAX_AGENT_TOOL_CONTENT_ITEMS)
        );

        let permission_options = (0..MAX_AGENT_PERMISSION_OPTIONS + 8)
            .map(|index| {
                PermissionOption::new(
                    format!("allow-{index}"),
                    format!("Allow {index}"),
                    PermissionOptionKind::AllowOnce,
                )
            })
            .collect();
        thread.transcript.request_permission(
            42,
            ToolCallUpdate::new(
                "tool-cap",
                ToolCallUpdateFields::new().title("Bound choices"),
            ),
            permission_options,
        );
        assert_eq!(
            thread.transcript.permissions()[0].options.len(),
            MAX_AGENT_PERMISSION_OPTIONS
        );
    }

    #[test]
    fn tool_updates_keep_a_stable_native_entry_id() {
        let mut thread = thread();
        thread.apply_update(SessionUpdate::ToolCall(
            ToolCall::new("tool-1", "Run tests")
                .kind(ToolKind::Execute)
                .status(ToolCallStatus::InProgress),
        ));
        let id = thread.transcript.entries()[0].id();
        thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "tool-1",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .raw_output(serde_json::json!({"ok": true})),
        )));

        assert_eq!(thread.transcript.entries().len(), 1);
        assert_eq!(thread.transcript.entries()[0].id(), id);
        assert!(matches!(
            &thread.transcript.entries()[0],
            AgentThreadEntry::Tool {
                status: AgentToolStatusModel::Completed,
                output,
                ..
            } if matches!(
                output.as_slice(),
                [ToolPayload::Json(output)] if output.contains("ok")
            )
        ));
    }

    #[test]
    fn vendor_metadata_is_inert_and_parented_updates_stay_flat() {
        let mut thread = thread();
        let metadata = serde_json::json!({
            "claudeCode": {
                "subagent": true,
                "parentToolUseId": "parent-tool"
            },
            "codex": {
                "subagent": {
                    "threadId": "thread-1",
                    "activity": "started"
                },
                "collaboration": {
                    "tool": "spawnAgent"
                }
            },
            "contextCompaction": true,
            "terminal_info": {
                "terminal_id": "terminal-1"
            }
        })
        .as_object()
        .unwrap()
        .clone();
        thread.apply_update(SessionUpdate::ToolCall(
            ToolCall::new("tool-1", "Task")
                .status(ToolCallStatus::InProgress)
                .raw_input(serde_json::json!({"prompt": "sleep"}))
                .meta(metadata.clone()),
        ));
        thread.apply_update(SessionUpdate::ToolCallUpdate(
            ToolCallUpdate::new("tool-1", ToolCallUpdateFields::new()).meta(metadata.clone()),
        ));
        thread.apply_update(SessionUpdate::AgentMessageChunk(
            ContentChunk::new(ContentBlock::Text(TextContent::new("child output"))).meta(metadata),
        ));

        assert!(matches!(
            thread.transcript.entries(),
            [
                AgentThreadEntry::Tool {
                    label,
                    status: AgentToolStatusModel::Running,
                    input: Some(ToolPayload::Json(input)),
                    output,
                    ..
                },
                AgentThreadEntry::Assistant { markdown, .. }
            ] if label == "Task"
                && input.contains("sleep")
                && output.is_empty()
                && markdown == "child output"
        ));
    }

    #[test]
    fn prompt_completion_ends_activity_without_fabricating_tool_completion() {
        let mut thread = thread();
        thread.connection = AgentConnectionState::Running;
        thread.apply_update(SessionUpdate::ToolCall(
            ToolCall::new("tool-running", "Run tests").status(ToolCallStatus::InProgress),
        ));
        thread.apply_update(SessionUpdate::ToolCall(ToolCall::new(
            "tool-pending",
            "Read file",
        )));

        thread.transcript.finish_turn();
        thread.connection = AgentConnectionState::Ready;

        let statuses = thread
            .transcript
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                AgentThreadEntry::Tool { status, .. } => Some(*status),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            statuses,
            [AgentToolStatusModel::Running, AgentToolStatusModel::Pending,]
        );
        assert_eq!(thread.connection, AgentConnectionState::Ready);
    }

    #[test]
    fn late_standard_updates_change_data_without_reviving_pane_activity() {
        let mut thread = thread();
        thread.connection = AgentConnectionState::Ready;

        thread.apply_runtime_update(SessionUpdate::ToolCall(
            ToolCall::new("tool-late", "Read replayed file").status(ToolCallStatus::InProgress),
        ));
        thread.apply_runtime_update(SessionUpdate::AgentMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new("late output")),
        )));

        assert_eq!(thread.connection, AgentConnectionState::Ready);
        assert!(matches!(
            thread.transcript.entries(),
            [
                AgentThreadEntry::Tool {
                    status: AgentToolStatusModel::Running,
                    ..
                },
                AgentThreadEntry::Assistant { markdown, .. }
            ] if markdown == "late output"
        ));
    }

    #[test]
    fn tool_payloads_preserve_structure_and_prefer_content_over_raw_output() {
        let mut thread = thread();
        thread.apply_update(SessionUpdate::ToolCall(ToolCall::new(
            "tool-1",
            "Edit file",
        )));
        thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "tool-1",
            ToolCallUpdateFields::new()
                .locations(vec![
                    ToolCallLocation::new("/workspace/src/lib.rs").line(12),
                ])
                .raw_input(serde_json::json!({"path": "/workspace/src/lib.rs"}))
                .content(vec![
                    Diff::new("/workspace/src/lib.rs", "new text")
                        .old_text("old text")
                        .into(),
                    ContentBlock::Text(TextContent::new("finished")).into(),
                    ToolCallContent::Terminal(Terminal::new("terminal-1")),
                ])
                .raw_output(serde_json::json!({"ignored": true})),
        )));

        assert!(matches!(
            &thread.transcript.entries()[0],
            AgentThreadEntry::Tool {
                location: Some(location),
                input: Some(ToolPayload::Json(input)),
                output,
                ..
            } if location == "/workspace/src/lib.rs:12"
                && input.contains("path")
                && matches!(
                    output.as_slice(),
                    [
                        ToolPayload::Diff { path, old: Some(old), new },
                        ToolPayload::Text(text),
                        ToolPayload::Terminal(terminal),
                    ] if path == "/workspace/src/lib.rs"
                        && old == "old text"
                        && new == "new text"
                        && text == "finished"
                        && terminal == "[terminal terminal-1]"
                )
        ));

        thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "tool-1",
            ToolCallUpdateFields::new().raw_output(serde_json::json!({"late": true})),
        )));
        assert!(matches!(
            &thread.transcript.entries()[0],
            AgentThreadEntry::Tool { output, .. }
                if matches!(output.first(), Some(ToolPayload::Diff { .. }))
        ));

        thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "tool-1",
            ToolCallUpdateFields::new()
                .content(Vec::new())
                .raw_output(serde_json::json!({"fallback": true})),
        )));
        assert!(matches!(
            &thread.transcript.entries()[0],
            AgentThreadEntry::Tool { output, .. }
                if matches!(
                    output.as_slice(),
                    [ToolPayload::Json(output)] if output.contains("fallback")
                )
        ));
    }

    #[test]
    fn plan_updates_replace_the_existing_plan_entry() {
        let mut thread = thread();
        let plan = |status| {
            Plan::new(vec![PlanEntry::new(
                "Implement ACP",
                PlanEntryPriority::High,
                status,
            )])
        };
        thread.apply_update(SessionUpdate::Plan(plan(PlanEntryStatus::InProgress)));
        let id = thread.transcript.entries()[0].id();
        thread.apply_update(SessionUpdate::Plan(plan(PlanEntryStatus::Completed)));

        assert_eq!(thread.transcript.entries().len(), 1);
        assert_eq!(thread.transcript.entries()[0].id(), id);
        assert!(matches!(
            &thread.transcript.entries()[0],
            AgentThreadEntry::Plan { markdown, .. } if markdown.starts_with("- [x]")
        ));
    }

    #[test]
    fn permission_lifecycle_marks_and_releases_the_tool() {
        let mut thread = thread();
        thread.apply_update(SessionUpdate::ToolCall(ToolCall::new(
            "tool-1",
            "Edit file",
        )));
        thread.transcript.request_permission(
            7,
            ToolCallUpdate::new(
                "tool-1",
                ToolCallUpdateFields::new().title("Edit file".to_owned()),
            ),
            vec![PermissionOption::new(
                "allow-once",
                "Allow once",
                PermissionOptionKind::AllowOnce,
            )],
        );
        assert!(matches!(
            &thread.transcript.entries()[0],
            AgentThreadEntry::Tool {
                status: AgentToolStatusModel::NeedsApproval,
                ..
            }
        ));
        thread.transcript.resolve_permission(7, false);
        assert!(thread.transcript.permissions().is_empty());
        assert!(matches!(
            &thread.transcript.entries()[0],
            AgentThreadEntry::Tool {
                status: AgentToolStatusModel::Pending,
                ..
            }
        ));
    }

    const PNG_PIXEL: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    fn attachment() -> Arc<Image> {
        Arc::new(Image::from_bytes(ImageFormat::Png, PNG_PIXEL.to_vec()))
    }

    #[test]
    fn a_replayed_user_image_returns_to_the_transcript() {
        let mut thread = thread();
        let attachment = attachment();
        let sent = ContentBlock::Image(ImageContent::new(
            BASE64.encode(&attachment.bytes),
            attachment.format.mime_type(),
        ));

        thread.apply_update(SessionUpdate::UserMessageChunk(ContentChunk::new(sent)));

        let Some(AgentThreadEntry::User {
            markdown, images, ..
        }) = thread.transcript.entries().first()
        else {
            panic!("the replayed chunk should open a user entry");
        };
        assert!(
            markdown.is_empty(),
            "an image should not leave a text placeholder behind"
        );
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].bytes, PNG_PIXEL);
    }

    #[test]
    fn a_replayed_data_uri_becomes_an_image_rather_than_a_link() {
        let mut thread = thread();
        let replayed = format!(
            "hi can you read this image properly?[@image](data:image/png;base64,{})",
            BASE64.encode(PNG_PIXEL)
        );
        thread.apply_update(SessionUpdate::UserMessageChunk(ContentChunk::new(
            ContentBlock::Text(TextContent::new(replayed)),
        )));

        let Some(AgentThreadEntry::User {
            markdown, images, ..
        }) = thread.transcript.entries().first()
        else {
            panic!("the replayed chunk should open a user entry");
        };
        assert_eq!(
            markdown, "hi can you read this image properly?",
            "the link should leave with the image, not linger as `[@image]`"
        );
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].bytes, PNG_PIXEL);
    }

    #[test]
    fn text_that_only_looks_like_an_attachment_is_left_alone() {
        let (kept, images) = split_inline_images("see data:image/png;base64,not-base64!!");
        assert!(images.is_empty());
        assert_eq!(kept, "see data:image/png;base64,not-base64!!");

        let (kept, images) = split_inline_images("just a sentence");
        assert!(images.is_empty());
        assert_eq!(kept, "just a sentence");
    }

    #[test]
    fn an_agent_that_takes_no_images_refuses_them_before_the_turn_starts() {
        let mut thread = thread();
        thread.connection = AgentConnectionState::Ready;

        assert!(
            !thread.session_capabilities.images,
            "an agent is assumed to take no images until it advertises them"
        );
        assert!(
            thread.prompt_refusal(false).is_none(),
            "text is always fine"
        );
        assert_eq!(
            thread.prompt_refusal(true).as_deref(),
            Some("this agent does not accept images")
        );

        thread.session_capabilities.images = true;
        assert!(thread.prompt_refusal(true).is_none());

        thread.connection = AgentConnectionState::Starting;
        assert!(thread.prompt_refusal(true).is_some());
    }

    #[test]
    fn transport_loss_clears_approvals_and_cancels_inflight_tools() {
        let mut thread = thread();
        thread.begin_prompt("make a change".to_owned(), Vec::new());
        thread.apply_update(SessionUpdate::ToolCall(
            ToolCall::new("tool-1", "Edit file")
                .kind(ToolKind::Edit)
                .status(ToolCallStatus::InProgress),
        ));
        thread.transcript.request_permission(
            7,
            ToolCallUpdate::new("tool-1", ToolCallUpdateFields::new()),
            vec![PermissionOption::new(
                "allow-once",
                "Allow once",
                PermissionOptionKind::AllowOnce,
            )],
        );

        thread.transcript.cancel_inflight();

        assert!(thread.transcript.permissions().is_empty());
        assert!(matches!(
            thread.transcript.entries().last(),
            Some(AgentThreadEntry::Tool {
                status: AgentToolStatusModel::Canceled,
                ..
            })
        ));
    }

    #[test]
    fn pending_composer_text_stacks_and_is_taken_once() {
        let mut controller = AgentController::new();
        let pane = PaneId(7);
        assert!(controller.take_pending_composer(pane).is_none());

        assert!(controller.queue_composer_text(pane, "first"));
        assert!(controller.queue_composer_text(pane, "second"));
        assert!(!controller.queue_composer_text(pane, "   "));
        assert!(controller.pane_state(pane).is_none());

        controller.panes.insert(pane, thread());
        assert_eq!(
            controller
                .pane_state(pane)
                .expect("state")
                .pending_composer
                .as_deref(),
            Some("first\nsecond")
        );
        assert_eq!(
            controller.take_pending_composer(pane).as_deref(),
            Some("first\nsecond")
        );
        assert!(controller.take_pending_composer(pane).is_none());
        assert!(
            controller
                .pane_state(pane)
                .expect("state")
                .pending_composer
                .is_none()
        );
    }

    #[test]
    fn a_prompt_is_refused_unless_the_pane_is_ready() {
        let mut thread = thread();
        assert!(thread.prompt_refusal(false).is_some());
        thread.connection = AgentConnectionState::Ready;
        assert!(thread.prompt_refusal(false).is_none());
        thread.connection = AgentConnectionState::Running;
        assert!(thread.prompt_refusal(false).is_some());
    }

    /// The question branch is unreachable while zz speaks ACP v1: the schema
    /// types an option kind as a closed enum, so a question-shaped kind never
    /// survives deserialization to reach [`is_user_question`].
    #[test]
    fn acp_v1_rejects_permission_options_with_an_unknown_kind() {
        assert!(
            serde_json::from_value::<PermissionOption>(serde_json::json!({
                "optionId": "pick-a",
                "name": "Option A",
                "kind": "answer"
            }))
            .is_err()
        );
    }

    #[test]
    fn status_only_updates_never_retype_a_typed_tool() {
        let mut thread = thread();
        thread.apply_update(SessionUpdate::ToolCall(
            ToolCall::new("tool-1", "cargo test")
                .kind(ToolKind::Execute)
                .status(ToolCallStatus::InProgress),
        ));
        thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "tool-1",
            ToolCallUpdateFields::new()
                .kind(ToolKind::Other)
                .status(ToolCallStatus::Completed)
                .raw_output(serde_json::json!({"ok": true})),
        )));
        assert!(matches!(
            &thread.transcript.entries()[0],
            AgentThreadEntry::Tool {
                kind: AgentToolKindModel::Execute,
                status: AgentToolStatusModel::Completed,
                label,
                ..
            } if label == "cargo test"
        ));

        thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "tool-1",
            ToolCallUpdateFields::new().title(String::new()),
        )));
        assert!(matches!(
            &thread.transcript.entries()[0],
            AgentThreadEntry::Tool { label, .. } if label == "cargo test"
        ));

        thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "tool-1",
            ToolCallUpdateFields::new()
                .kind(ToolKind::Other)
                .title("Something else".to_owned()),
        )));
        assert!(matches!(
            &thread.transcript.entries()[0],
            AgentThreadEntry::Tool {
                kind: AgentToolKindModel::Other,
                label,
                ..
            } if label == "Something else"
        ));
    }

    #[test]
    fn oversized_tool_payloads_are_capped_on_char_boundaries() {
        let text = capped("é".repeat(16), TRUNCATION_MARKER.len() + 5);
        assert_eq!(text, "éé".to_owned() + TRUNCATION_MARKER);

        let mut thread = thread();
        thread.apply_update(SessionUpdate::ToolCall(
            ToolCall::new("tool-big", "Read file").content(vec![ToolCallContent::from(
                ContentBlock::Text(TextContent::new("é".repeat(MAX_TOOL_PAYLOAD_BYTES))),
            )]),
        ));
        assert!(matches!(
            &thread.transcript.entries()[0],
            AgentThreadEntry::Tool { output, .. }
                if matches!(
                    output.as_slice(),
                    [ToolPayload::Text(text)]
                        if text.len() <= MAX_TOOL_PAYLOAD_BYTES
                            && text.ends_with(TRUNCATION_MARKER)
                )
        ));
    }

    /// back, standing in for the daemon on the other end of the wire.
    fn proxy_controller(
        cx: &mut TestAppContext,
    ) -> (
        Entity<AgentController>,
        Rc<RefCell<Vec<(PaneId, AgentRequest)>>>,
    ) {
        cx.update(|cx| {
            crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            let sink = mux.update(cx, |mux, _| {
                mux.set_agent_client_instance_id_for_test(zz_protocol::ClientInstanceId(1));
                mux.record_agent_requests_for_test()
            });
            let controller = cx.new(|_| AgentController::new());
            controller.update(cx, |controller, _| controller.attach_mux(mux));
            (controller, sink)
        })
    }

    /// A registered pane whose agent is connected and taking prompts.
    fn ready_pane(controller: &mut AgentController, pane: PaneId) {
        let mut thread = thread();
        thread.connection = AgentConnectionState::Ready;
        thread.session_capabilities.images = true;
        controller.panes.insert(pane, thread);
        controller.viewports.insert(pane, PaneViewport::default());
        controller.retained_panes.insert(pane);
    }

    fn item(seq: u64, payload: AgentStreamPayload) -> AgentStreamItem {
        AgentStreamItem { seq, payload }
    }

    fn json(value: &impl serde::Serialize) -> serde_json::Value {
        serde_json::to_value(value).expect("the ACP schema encodes")
    }

    /// One `session/update` carrying a `sessionUpdate` this build has no
    /// variant for: a future ACP addition, or an `_`-prefixed extension.
    fn unknown_update(tag: &str) -> AgentStreamPayload {
        AgentStreamPayload::Update {
            update: serde_json::json!({"sessionUpdate": tag, "payload": {"opaque": true}}),
        }
    }

    fn chunk_update(text: &str, message_id: &str) -> AgentStreamPayload {
        AgentStreamPayload::Update {
            update: json(&SessionUpdate::AgentMessageChunk(
                ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                    .message_id(MessageId::new(message_id)),
            )),
        }
    }

    fn turn_finished(reason: StopReason) -> AgentStreamPayload {
        AgentStreamPayload::PromptFinished {
            turn_id: 1,
            outcome: AgentPromptOutcome::Finished {
                stop_reason: json(&reason),
            },
        }
    }

    #[zz_gpui::test]
    fn stream_items_reduce_exactly_as_the_runtime_events_did(cx: &mut TestAppContext) {
        let (controller, _sink) = proxy_controller(cx);
        let pane = PaneId(3);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller
                    .viewports
                    .get_mut(&pane)
                    .expect("viewport")
                    .queued_prompts = 1;
                controller.apply_stream_items(
                    pane,
                    vec![
                        item(
                            1,
                            AgentStreamPayload::Ready {
                                agent_name: "Codex".to_owned(),
                                agent_key: "codex".to_owned(),
                                auth_methods: vec![zz_daemon::AgentAuthMethod {
                                    id: "api".to_owned(),
                                    name: "API key".to_owned(),
                                    description: None,
                                }],
                                capabilities: zz_daemon::AgentSessionCapabilities {
                                    list: true,
                                    load: true,
                                    ..zz_daemon::AgentSessionCapabilities::default()
                                },
                            },
                        ),
                        item(
                            2,
                            AgentStreamPayload::SessionReady {
                                session_id: "s-1".to_owned(),
                                modes: None,
                                config_options: None,
                            },
                        ),
                        item(
                            3,
                            AgentStreamPayload::Update {
                                update: json(&SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                    ContentBlock::Text(TextContent::new("hello")),
                                ))),
                            },
                        ),
                        item(
                            4,
                            AgentStreamPayload::Update {
                                update: json(&SessionUpdate::ToolCall(
                                    ToolCall::new("tool-1", "Read file")
                                        .status(ToolCallStatus::InProgress),
                                )),
                            },
                        ),
                        item(
                            5,
                            AgentStreamPayload::PermissionRequested {
                                request_id: 7,
                                tool_call: json(&ToolCallUpdate::new(
                                    "tool-1",
                                    ToolCallUpdateFields::new(),
                                )),
                                options: json(&vec![PermissionOption::new(
                                    "allow-once",
                                    "Allow once",
                                    PermissionOptionKind::AllowOnce,
                                )]),
                                questions: Vec::new(),
                            },
                        ),
                        item(
                            6,
                            AgentStreamPayload::PermissionResolved {
                                request_id: 7,
                                canceled: false,
                            },
                        ),
                        item(7, turn_finished(StopReason::EndTurn)),
                        item(
                            8,
                            AgentStreamPayload::PromptsReclaimed {
                                prompts: vec![zz_daemon::AgentPrompt {
                                    owner: zz_protocol::ClientInstanceId::default(),
                                    text: "retry that".to_owned(),
                                    images: Vec::new(),
                                }],
                            },
                        ),
                    ],
                    cx,
                );

                let state = controller.pane_state(pane).expect("the pane is registered");
                assert_eq!(state.agent_name.as_deref(), Some("Codex"));
                assert_eq!(state.session_id.as_deref(), Some("s-1"));
                assert!(state.session_capabilities.list);
                assert_eq!(state.connection, AgentConnectionState::Ready);
                assert!(state.pending_permissions.is_empty());
                assert_eq!(state.queued_prompts, 0);
                let (entries, ..) = controller.pane_entries(pane).expect("entries");
                assert!(matches!(
                    entries.first(),
                    Some(AgentThreadEntry::Assistant { markdown, .. }) if markdown == "hello"
                ));
                assert!(matches!(
                    entries.get(1),
                    Some(AgentThreadEntry::Tool {
                        status: AgentToolStatusModel::Pending,
                        ..
                    })
                ));
                assert_eq!(
                    controller.take_pending_composer(pane).as_deref(),
                    Some("retry that"),
                    "a reclaimed prompt is visible in the draft again"
                );
                assert_eq!(
                    controller.viewports[&pane].last_applied, 8,
                    "the cursor follows the highest applied seq"
                );
            });
        });
    }

    #[zz_gpui::test]
    fn restored_prompts_only_refill_their_owners_composer(cx: &mut TestAppContext) {
        let (controller, sink) = proxy_controller(cx);
        let pane = PaneId(4);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller.apply_stream_items(
                    pane,
                    vec![
                        item(
                            1,
                            AgentStreamPayload::PromptsRestored {
                                reclaim_id: 1,
                                prompts: vec![zz_daemon::AgentPrompt {
                                    owner: zz_protocol::ClientInstanceId(2),
                                    text: "someone else's draft".to_owned(),
                                    images: Vec::new(),
                                }],
                            },
                        ),
                        item(
                            2,
                            AgentStreamPayload::PromptsRestored {
                                reclaim_id: 2,
                                prompts: vec![zz_daemon::AgentPrompt {
                                    owner: zz_protocol::ClientInstanceId(1),
                                    text: "my draft".to_owned(),
                                    images: Vec::new(),
                                }],
                            },
                        ),
                    ],
                    cx,
                );

                assert_eq!(
                    controller.take_pending_composer(pane).as_deref(),
                    Some("my draft")
                );
            });
        });
        assert_eq!(
            &*sink.borrow(),
            &[(
                pane,
                AgentRequest::AcknowledgePromptRestore { reclaim_id: 2 }
            )]
        );
    }

    #[zz_gpui::test]
    fn an_item_the_client_cannot_re_type_is_dropped_without_stalling_the_cursor(
        cx: &mut TestAppContext,
    ) {
        let (controller, _sink) = proxy_controller(cx);
        let pane = PaneId(4);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller.apply_stream_items(
                    pane,
                    vec![
                        item(
                            1,
                            AgentStreamPayload::Update {
                                update: serde_json::json!({"sessionUpdate": "from the future"}),
                            },
                        ),
                        item(
                            2,
                            AgentStreamPayload::Update {
                                update: json(&SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                    ContentBlock::Text(TextContent::new("still here")),
                                ))),
                            },
                        ),
                    ],
                    cx,
                );

                let (entries, ..) = controller.pane_entries(pane).expect("entries");
                assert_eq!(entries.len(), 1);
                assert_eq!(controller.viewports[&pane].last_applied, 2);
                assert_eq!(controller.panes[&pane].unknown_updates, 1);
            });
        });
    }

    #[zz_gpui::test]
    fn an_unknown_update_between_two_known_ones_only_skips_itself(cx: &mut TestAppContext) {
        let (controller, _sink) = proxy_controller(cx);
        let pane = PaneId(40);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller.apply_stream_items(
                    pane,
                    vec![
                        item(1, chunk_update("first", "message-1")),
                        item(2, unknown_update("terminal_output_chunk")),
                        item(3, chunk_update("second", "message-2")),
                    ],
                    cx,
                );

                let (entries, ..) = controller.pane_entries(pane).expect("entries");
                assert_eq!(entries.len(), 2, "an unknown item poisons nothing after it");
                assert!(matches!(
                    (&entries[0], &entries[1]),
                    (
                        AgentThreadEntry::Assistant { markdown: first, .. },
                        AgentThreadEntry::Assistant { markdown: second, .. },
                    ) if first == "first" && second == "second"
                ));
                assert_eq!(controller.viewports[&pane].last_applied, 3);
                assert_eq!(controller.panes[&pane].unknown_updates, 1);
            });
        });
    }

    #[zz_gpui::test]
    fn a_replay_over_a_surviving_transcript_reduces_each_item_once(cx: &mut TestAppContext) {
        let (controller, _sink) = proxy_controller(cx);
        let pane = PaneId(44);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller.apply_stream_items(
                    pane,
                    vec![
                        item(1, chunk_update("first", "message-1")),
                        item(2, chunk_update("second", "message-2")),
                    ],
                    cx,
                );

                controller.apply_stream_items(
                    pane,
                    vec![
                        item(1, chunk_update("first", "message-1")),
                        item(2, chunk_update("second", "message-2")),
                        item(3, chunk_update("third", "message-3")),
                    ],
                    cx,
                );

                let (entries, ..) = controller.pane_entries(pane).expect("entries");
                assert_eq!(
                    entries.len(),
                    3,
                    "an attach replays over the transcript it already reduced"
                );
                assert_eq!(controller.viewports[&pane].last_applied, 3);
            });
        });
    }

    #[zz_gpui::test]
    fn submitted_and_queued_user_messages_survive_session_switch_replay(cx: &mut TestAppContext) {
        let (controller, _sink) = proxy_controller(cx);
        let pane = PaneId(44);
        let user = |text: &str, id: &str| {
            let mut chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text)));
            chunk.message_id = Some(MessageId::new(id));
            AgentStreamPayload::Update {
                update: json(&SessionUpdate::UserMessageChunk(chunk)),
            }
        };
        let items = vec![
            item(1, user("first", "zz-prompt-1")),
            item(2, chunk_update("answer one", "answer-1")),
            item(3, turn_finished(StopReason::EndTurn)),
            item(4, user("second", "zz-prompt-2")),
            item(5, chunk_update("answer two", "answer-2")),
            item(6, turn_finished(StopReason::EndTurn)),
        ];
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                for provider in [AgentProvider::Codex, AgentProvider::ClaudeCode] {
                    ready_pane(controller, pane);
                    controller.panes.get_mut(&pane).expect("pane").provider = provider;
                    controller.prompt(pane, "first", Vec::new(), cx).expect("first prompt");
                    controller.prompt(pane, "second", Vec::new(), cx).expect("queued prompt");
                    controller.apply_stream_items(pane, items.clone(), cx);
                    let expected = controller.pane_entries(pane).expect("entries").0.to_vec();
                    assert_eq!(expected.len(), 4);
                    assert!(matches!(&expected[0], AgentThreadEntry::User { markdown, .. } if markdown == "first"));
                    assert!(matches!(&expected[2], AgentThreadEntry::User { markdown, .. } if markdown == "second"));
                    controller.retain_panes(&BTreeSet::new(), cx);
                    ready_pane(controller, pane);
                    controller.panes.get_mut(&pane).expect("pane").provider = provider;
                    controller.apply_stream_items(pane, items.clone(), cx);
                    controller.apply_stream_items(pane, items.clone(), cx);
                    assert_eq!(controller.pane_entries(pane).expect("replayed entries").0, expected);
                    controller.retain_panes(&BTreeSet::new(), cx);
                }
            });
        });
    }

    #[zz_gpui::test]
    fn an_unknown_update_mid_message_does_not_break_its_coalescing(cx: &mut TestAppContext) {
        let (controller, _sink) = proxy_controller(cx);
        let pane = PaneId(41);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller.apply_stream_items(
                    pane,
                    vec![
                        item(1, chunk_update("hello ", "message-1")),
                        item(2, unknown_update("_zz_private_extension")),
                        item(3, chunk_update("world", "message-1")),
                    ],
                    cx,
                );

                let (entries, ..) = controller.pane_entries(pane).expect("entries");
                assert_eq!(entries.len(), 1, "the message stays one entry");
                assert!(matches!(
                    &entries[0],
                    AgentThreadEntry::Assistant { markdown, .. } if markdown == "hello world"
                ));
                assert_eq!(controller.panes[&pane].unknown_updates, 1);
            });
        });
    }

    #[zz_gpui::test]
    fn a_batch_of_only_unknown_updates_advances_the_cursor(cx: &mut TestAppContext) {
        let (controller, sink) = proxy_controller(cx);
        let pane = PaneId(42);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller.apply_stream_items(
                    pane,
                    vec![
                        item(5, unknown_update("state_update")),
                        item(6, unknown_update("_vendor_thing")),
                        item(
                            7,
                            AgentStreamPayload::Update {
                                update: serde_json::json!({
                                    "sessionUpdate": "agent_message_chunk",
                                    "content": 42,
                                }),
                            },
                        ),
                    ],
                    cx,
                );

                assert!(
                    controller
                        .pane_entries(pane)
                        .is_some_and(|(entries, ..)| entries.is_empty())
                );
                assert_eq!(
                    controller.viewports[&pane].last_applied, 7,
                    "every seq counts as consumed, so nothing asks to be replayed"
                );
                assert_eq!(controller.panes[&pane].unknown_updates, 3);
            });
        });
        assert!(
            sink.borrow().is_empty(),
            "a batch zz skipped whole is not a hole in the journal"
        );
    }

    #[zz_gpui::test]
    fn a_restored_session_replays_around_updates_this_build_cannot_read(cx: &mut TestAppContext) {
        let (controller, _sink) = proxy_controller(cx);
        let pane = PaneId(43);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller.apply_stream_items(
                    pane,
                    vec![item(
                        1,
                        AgentStreamPayload::SessionSwitched {
                            session_id: "s-2".to_owned(),
                            cwd: PathBuf::from("/workspace"),
                            modes: None,
                            config_options: None,
                            replay: vec![
                                json(&SessionUpdate::AgentMessageChunk(
                                    ContentChunk::new(ContentBlock::Text(TextContent::new(
                                        "restored",
                                    )))
                                    .message_id(MessageId::new("message-1")),
                                )),
                                serde_json::json!({"sessionUpdate": "plan_update", "planId": "p1"}),
                                json(&SessionUpdate::AgentMessageChunk(
                                    ContentChunk::new(ContentBlock::Text(TextContent::new(
                                        " and whole",
                                    )))
                                    .message_id(MessageId::new("message-1")),
                                )),
                            ],
                        },
                    )],
                    cx,
                );

                let state = controller.pane_state(pane).expect("pane state");
                assert_eq!(state.session_id.as_deref(), Some("s-2"));
                assert_eq!(state.connection, AgentConnectionState::Ready);
                let (entries, ..) = controller.pane_entries(pane).expect("entries");
                assert_eq!(entries.len(), 1);
                assert!(matches!(
                    &entries[0],
                    AgentThreadEntry::Assistant { markdown, .. } if markdown == "restored and whole"
                ));
                assert_eq!(controller.panes[&pane].unknown_updates, 1);
            });
        });
    }

    #[test]
    fn a_skipped_update_is_logged_by_tag_and_never_by_payload() {
        let payload = "x".repeat(4 * 1024);
        let update = serde_json::json!({
            "sessionUpdate": format!("_evil\n{payload}"),
            "content": payload,
        });

        let tag = session_update_tag(&update);
        assert!(tag.len() <= MAX_SESSION_UPDATE_TAG_BYTES);
        assert!(tag.starts_with("_evil"));
        assert!(!tag.contains('\n'), "a tag cannot forge a second log line");
        assert!(!tag.contains(&payload));

        let error = serde_json::from_value::<SessionUpdate>(update)
            .expect_err("an unknown discriminant does not decode");
        let excerpt = log_excerpt(&error.to_string(), MAX_DECODE_ERROR_BYTES);
        assert!(excerpt.len() <= MAX_DECODE_ERROR_BYTES);

        assert_eq!(
            session_update_tag(&serde_json::json!({"content": "no tag"})),
            UNTAGGED_SESSION_UPDATE
        );
    }

    #[zz_gpui::test]
    fn published_pane_state_drives_the_composer_and_the_wizard(cx: &mut TestAppContext) {
        let (controller, _sink) = proxy_controller(cx);
        let pane = PaneId(5);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller.panes.get_mut(&pane).expect("pane").session_id = Some(Arc::from("s-9"));
                let state = AgentPaneWire {
                    phase: AgentConnectionPhase::AwaitingPermission,
                    queued_prompts: 2,
                    session_id: Some("s-9".to_owned()),
                    title: Some("ship it".to_owned()),
                    error: None,
                    auth_methods: serde_json::to_string(&vec![zz_daemon::AgentAuthMethod {
                        id: "api".to_owned(),
                        name: "API key".to_owned(),
                        description: None,
                    }])
                    .expect("auth methods encode"),
                    config_options: String::new(),
                    modes: String::new(),
                    pending_permission: Some(zz_protocol::AgentPermissionWire {
                        request_id: 11,
                        payload: serde_json::json!({
                            "toolCall": json(&ToolCallUpdate::new(
                                "tool-9",
                                ToolCallUpdateFields::new(),
                            )),
                            "options": json(&vec![PermissionOption::new(
                                "allow-once",
                                "Allow once",
                                PermissionOptionKind::AllowOnce,
                            )]),
                        })
                        .to_string(),
                    }),
                    git: Some(AgentGitSummary {
                        branch: Some("main".to_owned()),
                        changed_files: 3,
                        additions: 21,
                        deletions: 8,
                    }),
                    tasks: Vec::new(),
                };

                controller.apply_pane_state(pane, &state, cx);

                assert_eq!(controller.queued_count(pane), 2);
                let pane_state = controller.pane_state(pane).expect("the pane is registered");
                assert_eq!(pane_state.queued_prompts, 2);
                assert_eq!(pane_state.session_id.as_deref(), Some("s-9"));
                assert_eq!(pane_state.git, state.git);
                assert_eq!(
                    pane_state
                        .pending_permissions
                        .first()
                        .map(|request| request.request_id),
                    Some(11),
                    "a client that attaches mid-question still sees the wizard"
                );

                controller.apply_pane_state(
                    pane,
                    &AgentPaneWire {
                        phase: AgentConnectionPhase::Ready,
                        queued_prompts: 0,
                        session_id: None,
                        title: None,
                        error: None,
                        auth_methods: String::new(),
                        config_options: String::new(),
                        modes: String::new(),
                        pending_permission: None,
                        git: None,
                        tasks: Vec::new(),
                    },
                    cx,
                );
                let pane_state = controller.pane_state(pane).expect("the pane is registered");
                assert_eq!(pane_state.queued_prompts, 0);
                assert_eq!(pane_state.session_id.as_deref(), Some("s-9"));
                assert!(pane_state.pending_permissions.is_empty());
                assert_eq!(pane_state.git, None);
            });
        });
    }

    #[zz_gpui::test]
    fn a_turn_the_agent_starts_on_its_own_ends_when_the_daemon_is_ready(cx: &mut TestAppContext) {
        let (controller, _sink) = proxy_controller(cx);
        let pane = PaneId(5);
        let wire = |phase| AgentPaneWire {
            phase,
            queued_prompts: 0,
            session_id: None,
            title: None,
            error: None,
            auth_methods: String::new(),
            config_options: String::new(),
            modes: String::new(),
            pending_permission: None,
            git: None,
            tasks: Vec::new(),
        };
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                let connection = |controller: &AgentController| {
                    controller.pane_state(pane).expect("pane").connection
                };

                controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Running), cx);
                assert_eq!(connection(controller), AgentConnectionState::Running);
                controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Ready), cx);
                assert_eq!(connection(controller), AgentConnectionState::Ready);

                controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Running), cx);
                controller.cancel(pane, cx);
                assert_eq!(connection(controller), AgentConnectionState::Cancelling);
                controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Ready), cx);
                assert_eq!(connection(controller), AgentConnectionState::Ready);

                controller
                    .prompt(pane, "hello", Vec::new(), cx)
                    .expect("a ready pane takes a prompt");
                controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Ready), cx);
                assert_eq!(
                    connection(controller),
                    AgentConnectionState::Running,
                    "a Ready published before the daemon saw the prompt does not end it"
                );
                controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Running), cx);
                controller.apply_pane_state(pane, &wire(AgentConnectionPhase::Ready), cx);
                assert_eq!(
                    connection(controller),
                    AgentConnectionState::Ready,
                    "once the daemon ran the prompt, its Ready ends the turn without waiting for PromptFinished"
                );
            });
        });
    }

    fn asked(id: &str, labels: &[&str], multi_select: bool) -> AgentQuestion {
        AgentQuestion {
            id: id.to_owned(),
            question: format!("{id}?"),
            options: labels
                .iter()
                .map(|label| zz_protocol::agent_stream::AgentQuestionOption {
                    label: (*label).to_owned(),
                    description: None,
                })
                .collect(),
            multi_select,
            allow_other: true,
            ..AgentQuestion::default()
        }
    }

    fn reply(id: &str, answers: &[&str]) -> AgentQuestionAnswer {
        AgentQuestionAnswer {
            id: id.to_owned(),
            answers: answers.iter().map(|answer| (*answer).to_owned()).collect(),
        }
    }

    #[zz_gpui::test]
    fn a_question_card_answers_through_its_own_request(cx: &mut TestAppContext) {
        let (controller, sink) = proxy_controller(cx);
        let pane = PaneId(8);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller.apply_stream_items(
                    pane,
                    vec![item(
                        1,
                        AgentStreamPayload::PermissionRequested {
                            request_id: 21,
                            tool_call: serde_json::json!({
                                "toolCallId": "ask-1",
                                "title": "Claude has questions",
                            }),
                            options: serde_json::json!([]),
                            questions: vec![
                                asked("fruit", &["apple", "pear"], false),
                                asked("tools", &["saw", "drill"], true),
                            ],
                        },
                    )],
                    cx,
                );
                let pending = controller
                    .pane_state(pane)
                    .expect("the pane is registered")
                    .pending_permissions;
                assert_eq!(pending[0].questions.len(), 2);
                assert!(pending[0].options.is_empty());

                let answers = vec![
                    reply("fruit", &["pear"]),
                    reply("tools", &["saw", "chisel"]),
                ];
                assert!(controller.answer_question(pane, 21, answers.clone(), cx));
                assert!(
                    controller
                        .pane_state(pane)
                        .expect("the pane is registered")
                        .pending_permissions
                        .is_empty(),
                    "an answered card leaves this client at once"
                );
                assert_eq!(
                    sink.borrow().as_slice(),
                    &[(
                        pane,
                        AgentRequest::AnswerQuestion {
                            request_id: 21,
                            answers,
                        }
                    )]
                );
            });
        });
    }

    #[zz_gpui::test]
    fn published_tasks_and_a_parked_card_reach_the_pane(cx: &mut TestAppContext) {
        let (controller, sink) = proxy_controller(cx);
        let pane = PaneId(9);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                let questions = vec![asked("fruit", &["apple"], true)];
                let state = AgentPaneWire {
                    phase: AgentConnectionPhase::AwaitingPermission,
                    pending_permission: Some(zz_protocol::AgentPermissionWire {
                        request_id: 4,
                        payload: serde_json::json!({
                            "toolCall": {"toolCallId": "ask-2", "title": "fruit?"},
                            "options": [],
                            "questions": questions,
                        })
                        .to_string(),
                    }),
                    tasks: vec![AgentTaskWire {
                        id: "b1".to_owned(),
                        kind: "shell".to_owned(),
                        description: "cargo build".to_owned(),
                        tool_call_id: Some("bash-1".to_owned()),
                    }],
                    ..AgentPaneWire::default()
                };

                controller.apply_pane_state(pane, &state, cx);

                let pane_state = controller.pane_state(pane).expect("the pane is registered");
                assert_eq!(
                    pane_state.pending_permissions[0].questions, questions,
                    "a client that attaches mid-card still sees every question"
                );
                assert_eq!(pane_state.tasks.as_ref(), state.tasks.as_slice());
                assert!(
                    !controller.stop_task(pane, "gone".to_owned(), cx),
                    "only a listed task stops"
                );
                assert!(controller.stop_task(pane, "b1".to_owned(), cx));
                assert_eq!(
                    sink.borrow().as_slice(),
                    &[(
                        pane,
                        AgentRequest::StopTask {
                            task_id: "b1".to_owned()
                        }
                    )]
                );

                controller.apply_pane_state(
                    pane,
                    &AgentPaneWire {
                        phase: AgentConnectionPhase::Ready,
                        ..AgentPaneWire::default()
                    },
                    cx,
                );
                let pane_state = controller.pane_state(pane).expect("the pane is registered");
                assert!(pane_state.tasks.is_empty());
                assert!(pane_state.pending_permissions.is_empty());
            });
        });
    }

    #[zz_gpui::test]
    fn subagent_tool_rows_resolve_to_their_agents_entry(cx: &mut TestAppContext) {
        let (controller, _sink) = proxy_controller(cx);
        let pane = PaneId(10);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller.apply_stream_items(
                    pane,
                    vec![
                        item(
                            1,
                            AgentStreamPayload::Update {
                                update: serde_json::json!({
                                    "sessionUpdate": "tool_call",
                                    "toolCallId": "agent-1",
                                    "title": "Survey",
                                    "kind": "think",
                                }),
                            },
                        ),
                        item(
                            2,
                            AgentStreamPayload::Update {
                                update: serde_json::json!({
                                    "sessionUpdate": "tool_call",
                                    "toolCallId": "read-1",
                                    "title": "Read a.rs",
                                    "kind": "read",
                                    "_meta": {"zz": {"parent": "agent-1"}},
                                }),
                            },
                        ),
                    ],
                    cx,
                );
                let agent = controller.tool_entry(pane, "agent-1").expect("agent row");
                let step = controller.tool_entry(pane, "read-1").expect("step row");
                assert_eq!(controller.tool_parent_entry(pane, step), Some(agent));
                assert_eq!(controller.tool_parent_entry(pane, agent), None);
            });
        });
    }

    #[zz_gpui::test]
    fn ordered_state_sync_wins_after_a_synthesized_replay(cx: &mut TestAppContext) {
        let (controller, _sink) = proxy_controller(cx);
        let pane = PaneId(16);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller.apply_stream_items(
                    pane,
                    vec![
                        item(1, AgentStreamPayload::SessionReset { restoring: true }),
                        item(
                            2,
                            AgentStreamPayload::SessionReady {
                                session_id: "s-1".to_owned(),
                                modes: None,
                                config_options: None,
                            },
                        ),
                        item(
                            3,
                            AgentStreamPayload::StateSynced {
                                state: AgentPaneWire {
                                    phase: AgentConnectionPhase::Failed {
                                        message: "adapter exited".to_owned(),
                                    },
                                    session_id: Some("s-1".to_owned()),
                                    ..AgentPaneWire::default()
                                },
                            },
                        ),
                    ],
                    cx,
                );

                let state = controller.pane_state(pane).expect("pane state");
                assert_eq!(state.connection, AgentConnectionState::Failed);
                assert_eq!(state.error.as_deref(), Some("adapter exited"));
                assert_eq!(controller.viewports[&pane].last_applied, 3);
            });
        });
    }

    #[zz_gpui::test]
    fn nonfatal_wire_error_survives_the_ordered_ready_state(cx: &mut TestAppContext) {
        let (controller, _sink) = proxy_controller(cx);
        let pane = PaneId(21);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller.handle_runtime_event(
                    pane,
                    RuntimeEvent::SessionSwitchFailed {
                        pane,
                        message: "could not restore".to_owned(),
                    },
                    cx,
                );
                controller.apply_pane_state(
                    pane,
                    &AgentPaneWire {
                        phase: AgentConnectionPhase::Ready,
                        error: Some("could not restore".to_owned()),
                        ..AgentPaneWire::default()
                    },
                    cx,
                );
                assert_eq!(
                    controller.pane_state(pane).and_then(|state| state.error),
                    Some(Arc::from("could not restore"))
                );
            });
        });
    }

    #[zz_gpui::test]
    fn fatal_state_fails_inflight_tools(cx: &mut TestAppContext) {
        let (controller, _sink) = proxy_controller(cx);
        let pane = PaneId(22);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                let thread = controller.panes.get_mut(&pane).expect("pane");
                thread.apply_update(SessionUpdate::ToolCall(
                    ToolCall::new("task-tool", "Research")
                        .kind(ToolKind::Think)
                        .status(ToolCallStatus::InProgress),
                ));
                controller.apply_pane_state(
                    pane,
                    &AgentPaneWire {
                        phase: AgentConnectionPhase::Failed {
                            message: "adapter exited".to_owned(),
                        },
                        ..AgentPaneWire::default()
                    },
                    cx,
                );
                let thread = controller.panes.get(&pane).expect("pane");
                assert!(matches!(
                    thread.transcript.entries().first(),
                    Some(AgentThreadEntry::Tool {
                        status: AgentToolStatusModel::Failed,
                        ..
                    })
                ));
            });
        });
    }

    #[zz_gpui::test]
    fn a_prompt_reaches_the_wire_with_normalized_attachments(cx: &mut TestAppContext) {
        let (controller, sink) = proxy_controller(cx);
        let pane = PaneId(6);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller
                    .prompt(pane, "  ship it  ", vec![attachment()], cx)
                    .expect("the prompt is accepted");
            });
        });

        assert_eq!(
            &*sink.borrow(),
            &[(
                pane,
                AgentRequest::Prompt {
                    text: "ship it".to_owned(),
                    images: vec![zz_protocol::AgentImage {
                        format: "image/png".to_owned(),
                        data: PNG_PIXEL.to_vec(),
                    }],
                }
            )],
            "the daemon receives bytes plus format, never a zz_gpui image"
        );
    }

    #[zz_gpui::test]
    fn a_prompt_typed_mid_turn_is_queued_by_the_daemon(cx: &mut TestAppContext) {
        let (controller, sink) = proxy_controller(cx);
        let pane = PaneId(7);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                controller
                    .prompt(pane, "first", Vec::new(), cx)
                    .expect("first prompt");
                controller
                    .prompt(pane, "second", Vec::new(), cx)
                    .expect("second prompt is queued rather than refused");

                assert_eq!(controller.queued_count(pane), 1);
                controller.unqueue_prompts(pane, cx);
            });
        });

        assert_eq!(
            sink.borrow()
                .iter()
                .map(|(_, request)| request.clone())
                .collect::<Vec<_>>(),
            vec![
                AgentRequest::Prompt {
                    text: "first".to_owned(),
                    images: Vec::new(),
                },
                AgentRequest::Prompt {
                    text: "second".to_owned(),
                    images: Vec::new(),
                },
                AgentRequest::Unqueue,
            ]
        );
    }

    #[zz_gpui::test]
    fn a_zz_command_opens_no_turn_and_joins_no_queue(cx: &mut TestAppContext) {
        let (controller, sink) = proxy_controller(cx);
        let pane = PaneId(7);
        cx.update(|cx| {
            controller.update(cx, |controller, cx| {
                ready_pane(controller, pane);
                let thread = controller.panes.get_mut(&pane).expect("pane");
                thread.session_capabilities.verbs = true;
                controller
                    .prompt(pane, "/rewind u-1", Vec::new(), cx)
                    .expect("the command is sent");
                let thread = &controller.panes[&pane];
                assert_eq!(thread.connection, AgentConnectionState::Ready);
                assert!(
                    thread.transcript.entries().is_empty(),
                    "the daemon echoes the command itself"
                );
                controller
                    .prompt(pane, "first", Vec::new(), cx)
                    .expect("first prompt");
                controller
                    .prompt(pane, "/steer faster", Vec::new(), cx)
                    .expect("a command goes out mid-turn");
                assert_eq!(controller.queued_count(pane), 0);
                let thread = controller.panes.get_mut(&pane).expect("pane");
                thread.session_capabilities.verbs = false;
                controller
                    .prompt(pane, "/steer later", Vec::new(), cx)
                    .expect("a plain prompt is queued");
                assert_eq!(controller.queued_count(pane), 1);
            });
        });

        assert_eq!(
            sink.borrow()
                .iter()
                .map(|(_, request)| match request {
                    AgentRequest::Prompt { text, .. } => text.as_str(),
                    _ => "",
                })
                .collect::<Vec<_>>(),
            ["/rewind u-1", "first", "/steer faster", "/steer later"]
        );
    }
}
