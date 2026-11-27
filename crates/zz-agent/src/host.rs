use std::time::{Duration, Instant};
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
};

use async_channel::{Receiver, Sender};
use parking_lot::Mutex;
use serde_json::Value;
#[cfg(test)]
use zz_protocol::ClientInstanceId;
use zz_protocol::agent_stream::agent_verb;
use zz_protocol::{
    AgentAutoApprove, AgentGitSummary, AgentProvider, AgentQuestionAnswer, AgentTaskWire, ClientId,
    MAX_AGENT_PROMPT_BYTES, MAX_AGENT_QUEUED_PROMPTS, MAX_AGENT_TASKS, PaneId, QuotaWindows,
    agent_mention::{ReportTiming, is_relayed, timed_report},
};

use crate::{
    claude::{ClaudeCommand, run_claude_runtime},
    codex::{CodexCommand, run_codex_runtime},
    environment::{AgentWorkspaceEnvironment, warm_adapter_cache},
    git_summary,
    journal::AgentJournal,
    runtime::{
        AgentSpawnConfig, RESTORE_NOTICE, RuntimeCommand, RuntimeControl, run_agent_runtime,
    },
    stream::{
        AgentAuthMethod, AgentPrompt, AgentPromptOutcome, AgentQuestion, AgentSessionCapabilities,
        AgentSessionSummary, AgentStreamItem, AgentStreamPayload,
    },
};
use zz_daemon_client::agent_context::AgentBlockPolicy;

const PANE_INBOX_CAPACITY: usize = 64;
const RUNTIME_COMMAND_CAPACITY: usize = 32;
const RUNTIME_CONTROL_CAPACITY: usize = 32;
const RUNTIME_EVENT_CAPACITY: usize = 64;

pub type AgentStreamSink =
    Box<dyn Fn(PaneId, u64, AgentPaneState, Option<AgentStreamItem>) + Send + Sync>;

#[derive(Debug)]
pub enum HostCommand {
    Prompt(QueuedPrompt),
    Join(AgentTurnWaiter),
    Steer(AgentPrompt),
    Cancel,
    Unqueue,
    RespondPermission {
        response: PermissionResponse,
    },
    Authenticate {
        method_id: String,
    },
    SetConfigOption {
        option_id: String,
        value: String,
    },
    SetMode {
        mode_id: String,
    },
    ListSessions {
        client: ClientId,
        cwd: Option<PathBuf>,
        cursor: Option<String>,
        replace: bool,
    },
    NewSession {
        cwd: PathBuf,
        reply: Option<zz_daemon_client::reply::Reply<Result<(), String>>>,
    },
    SwitchSession {
        session: AgentSessionSummary,
    },
    DeleteSession {
        client: ClientId,
        session_id: String,
    },
    AnswerQuestion {
        request_id: u64,
        answers: Vec<AgentQuestionAnswer>,
    },
    StopTask {
        task_id: String,
    },
    RefreshGit,
}

pub const MAX_AGENT_TURN_REPLY_BYTES: usize = 1024 * 1024;
const MAX_AGENT_FINAL_REPLY_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentTurnReply {
    pub text: String,
    pub truncated: bool,
    pub duration: Duration,
    pub tool_calls: u32,
    pub permissions_requested: u32,
    pub permissions_allowed: u32,
    pub permissions_denied: u32,
    pub final_text: String,
    pub stop_reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentTurnFailure {
    Failed(String),
    Blocked {
        permission: zz_protocol::AgentPermissionWire,
        reply: Box<AgentTurnReply>,
    },
    Stopped {
        reason: String,
        reply: Box<AgentTurnReply>,
    },
    Cancelled,
    Reclaimed,
    Closed,
    NoTurn,
}

#[derive(Debug)]
pub enum PermissionResponse {
    Exact {
        request_id: u64,
        option_id: Option<String>,
    },
    Select {
        request_id: Option<u64>,
        choice: PermissionChoice,
        reply: zz_daemon_client::reply::Reply<Result<String, String>>,
    },
}

#[derive(Debug)]
pub enum PermissionChoice {
    Allow,
    Deny,
    Option(String),
}

fn select_permission(
    pending: &[AgentPendingPermission],
    request_id: Option<u64>,
    choice: &PermissionChoice,
) -> Result<(u64, String), String> {
    let permission = match request_id {
        Some(id) => pending
            .iter()
            .find(|pending| pending.request_id == id)
            .ok_or_else(|| "no pending permission".to_owned())?,
        None => pending
            .first()
            .ok_or_else(|| "no pending permission".to_owned())?,
    };
    let options = permission
        .options
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    fn kind(option: &Value) -> Option<&str> {
        option.get("kind").and_then(Value::as_str)
    }
    let selected = match choice {
        PermissionChoice::Option(id) => options
            .iter()
            .find(|option| option.get("optionId").and_then(Value::as_str) == Some(id.as_str())),
        PermissionChoice::Allow => options
            .iter()
            .find(|option| kind(option) == Some("allow_once"))
            .or_else(|| {
                options
                    .iter()
                    .find(|option| kind(option) == Some("allow_always"))
            }),
        PermissionChoice::Deny => options
            .iter()
            .find(|option| matches!(kind(option), Some("reject_once" | "reject_always"))),
    };
    let id = selected
        .and_then(|option| option.get("optionId"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            let valid = options
                .iter()
                .filter_map(|option| option.get("optionId").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(", ");
            format!("unknown or unavailable permission option; valid ids: {valid}")
        })?;
    Ok((permission.request_id, id.to_owned()))
}

pub fn typed_answer(
    pending: &[AgentPendingPermission],
    request: Option<u64>,
    text: &str,
) -> Result<(u64, Vec<AgentQuestionAnswer>), String> {
    let permission = match request {
        Some(id) => pending.iter().find(|pending| pending.request_id == id),
        None => pending.first(),
    }
    .ok_or_else(|| "no pending permission".to_owned())?;
    let [question] = permission.questions.as_slice() else {
        return Err(format!(
            "request {} asks {} questions; --answer answers one",
            permission.request_id,
            permission.questions.len()
        ));
    };
    Ok((
        permission.request_id,
        vec![AgentQuestionAnswer {
            id: question.id.clone(),
            answers: vec![text.to_owned()],
        }],
    ))
}

pub type AgentTurnResult = Result<AgentTurnReply, AgentTurnFailure>;
#[derive(Debug)]
pub struct AgentTurnWaiter {
    pub reply: zz_daemon_client::reply::Reply<AgentTurnResult>,
    pub on_block: AgentBlockPolicy,
    pub audit: Arc<Mutex<Vec<String>>>,
    pub asks: Option<AgentAsks>,
}

#[derive(Clone)]
pub struct AgentAsks(pub Arc<dyn Fn(&AgentPendingPermission) + Send + Sync>);

impl std::fmt::Debug for AgentAsks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AgentAsks")
    }
}

#[derive(Debug)]
pub struct QueuedPrompt {
    pub prompt: AgentPrompt,
    pub waiter: Option<AgentTurnWaiter>,
    pub source: PromptSource,
    pub note: Option<String>,
    pub ask: Option<RelayedAsk>,
}

#[derive(Debug)]
pub struct RelayedAsk {
    pub request: u64,
    pub asker: Weak<Mutex<AgentPaneState>>,
}

impl RelayedAsk {
    fn answered(&self) -> bool {
        self.asker.upgrade().is_none_or(|state| {
            !state
                .lock()
                .pending_permissions
                .iter()
                .any(|pending| pending.request_id == self.request)
        })
    }
}

fn waiting_prompts(queue: &VecDeque<QueuedPrompt>) -> usize {
    queue
        .iter()
        .filter(|queued| queued.source != PromptSource::Injected)
        .count()
}

fn own_prompts(queue: &VecDeque<QueuedPrompt>) -> usize {
    queue
        .iter()
        .filter(|queued| queued.source == PromptSource::Composer)
        .count()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptSource {
    Composer,
    Command,
    Chat,
    Injected,
}

impl PromptSource {
    fn note(self, text: &str, extra: Option<&str>) -> Option<String> {
        match self {
            Self::Composer | Self::Command => zz_protocol::agent_mention::hidden_note(text, extra),
            Self::Chat | Self::Injected => extra.map(str::to_owned),
        }
    }
}

impl QueuedPrompt {
    pub fn settle(&mut self, result: AgentTurnResult) {
        if let Some(waiter) = self.waiter.take() {
            waiter.reply.try_send(result);
        }
    }

    fn into_reclaimed(mut self, failure: AgentTurnFailure) -> AgentPrompt {
        self.settle(Err(failure));
        self.prompt
    }
}

impl From<AgentPrompt> for QueuedPrompt {
    fn from(prompt: AgentPrompt) -> Self {
        Self {
            prompt,
            waiter: None,
            source: PromptSource::Composer,
            note: None,
            ask: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AgentPaneSpec {
    pub provider: AgentProvider,
    pub cwd: PathBuf,
    pub resume_session: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub auto_approve: Option<AgentAutoApprove>,
    pub workspace: AgentWorkspaceEnvironment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentConnectionPhase {
    Starting,
    Restoring,
    Ready,
    Running,
    Cancelling,
    Failed,
    Disconnected,
}

impl AgentConnectionPhase {
    const fn accepts_prompt(self) -> bool {
        matches!(self, Self::Ready)
    }

    const fn has_active_turn(self) -> bool {
        matches!(self, Self::Running | Self::Cancelling)
    }
}

#[derive(Clone, Debug)]
pub struct AgentPendingPermission {
    pub request_id: u64,
    pub tool_call: Value,
    pub options: Value,
    pub questions: Vec<AgentQuestion>,
}

impl AgentPendingPermission {
    pub fn outlives_turn(&self) -> bool {
        let zz = &self.tool_call["_meta"]["zz"];
        zz["background"] == true || zz["backgroundAgent"].is_string()
    }
}

#[derive(Clone, Debug)]
pub struct AgentPaneState {
    pub phase: AgentConnectionPhase,
    pub agent_name: Option<String>,
    pub agent_key: Option<String>,
    pub capabilities: AgentSessionCapabilities,
    pub auth_methods: Vec<AgentAuthMethod>,
    pub session_id: Option<String>,
    pub cwd: PathBuf,
    pub queued_prompts: usize,
    pub own_prompts: usize,
    pub waiting_prompts: usize,
    pub pending_permissions: Vec<AgentPendingPermission>,
    pub error: Option<String>,
    pub last_seq: u64,
    pub git: Option<AgentGitSummary>,
    pub tasks: Vec<AgentTaskWire>,
    pub title: Option<String>,
    pub quota: QuotaWindows,
}

impl AgentPaneState {
    fn new(spec: &AgentPaneSpec) -> Self {
        Self {
            phase: AgentConnectionPhase::Starting,
            agent_name: None,
            agent_key: None,
            capabilities: AgentSessionCapabilities::default(),
            auth_methods: Vec::new(),
            session_id: spec.resume_session.clone(),
            cwd: spec.cwd.clone(),
            queued_prompts: 0,
            own_prompts: 0,
            waiting_prompts: 0,
            pending_permissions: Vec::new(),
            error: None,
            last_seq: 0,
            git: None,
            tasks: Vec::new(),
            title: None,
            quota: QuotaWindows::default(),
        }
    }

    #[cfg(test)]
    pub fn for_test() -> Self {
        Self::new(&AgentPaneSpec {
            model: None,
            effort: None,
            provider: AgentProvider::Codex,
            cwd: PathBuf::from("/"),
            resume_session: None,
            auto_approve: None,
            workspace: AgentWorkspaceEnvironment::default(),
        })
    }
}

enum PaneInput {
    Command(HostCommand),
    Event(AgentStreamPayload),
    GitSummary {
        generation: u64,
        refresh: u64,
        cwd: PathBuf,
        summary: Option<AgentGitSummary>,
    },
    Finished(Result<(), String>),
}

struct PaneHandle {
    inbox: Sender<PaneInput>,
    control: Sender<HostCommand>,
    prompts: Sender<QueuedPrompt>,
    close: Sender<()>,
    state: Arc<Mutex<AgentPaneState>>,
    auto_approve: Arc<Mutex<AgentAutoApprove>>,
    ended: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

#[derive(Default)]
struct PaneRegistry {
    panes: Mutex<BTreeMap<PaneId, PaneHandle>>,
}

pub struct AgentHost {
    config: Mutex<AgentSpawnConfig>,
    sink: Arc<AgentStreamSink>,
    journal: Option<Arc<AgentJournal>>,
    registry: Arc<PaneRegistry>,
    permission_ids: Arc<AtomicU64>,
    #[cfg(any(test, feature = "test-support"))]
    runner_factory: Mutex<Option<PaneRunnerFactory>>,
}

pub struct RuntimeChannels {
    pub auto_approve: Arc<Mutex<AgentAutoApprove>>,
    pub permission_ids: Arc<AtomicU64>,
    pub journal: Option<Arc<AgentJournal>>,
    pub ended: Arc<AtomicBool>,
    pub commands: Receiver<RuntimeCommand>,
    pub controls: Receiver<RuntimeControl>,
    pub events: Sender<AgentStreamPayload>,
}

pub fn native_claude(config: &AgentSpawnConfig, provider: AgentProvider) -> Option<ClaudeCommand> {
    (provider == AgentProvider::ClaudeCode)
        .then(|| ClaudeCommand::parse(config.command_for(provider)))
        .flatten()
}

pub fn native_codex(config: &AgentSpawnConfig, provider: AgentProvider) -> Option<CodexCommand> {
    (provider == AgentProvider::Codex)
        .then(|| CodexCommand::parse(config.command_for(provider)))
        .flatten()
}

pub type PaneRunner = Box<
    dyn FnOnce(RuntimeChannels) -> Pin<Box<dyn Future<Output = Result<(), String>>>>
        + Send
        + 'static,
>;

pub fn pane_runner(config: AgentSpawnConfig, provider: AgentProvider) -> PaneRunner {
    if let Some(command) = native_claude(&config, provider) {
        let workspace = config.workspace;
        return Box::new(move |channels: RuntimeChannels| {
            Box::pin(run_claude_runtime(command, workspace, provider, channels))
        });
    }
    if let Some(command) = native_codex(&config, provider) {
        let workspace = config.workspace;
        return Box::new(move |channels: RuntimeChannels| {
            Box::pin(run_codex_runtime(command, workspace, provider, channels))
        });
    }
    match provider {
        AgentProvider::Antigravity => Box::new(move |channels: RuntimeChannels| {
            Box::pin(crate::agy::run_agy_runtime(config, provider, channels))
        }),
        AgentProvider::Cursor => Box::new(move |channels: RuntimeChannels| {
            Box::pin(crate::cursor::run_cursor_runtime(
                config, provider, channels,
            ))
        }),
        AgentProvider::Grok => Box::new(move |channels: RuntimeChannels| {
            Box::pin(crate::grok::run_grok_runtime(config, provider, channels))
        }),
        AgentProvider::ClaudeCode | AgentProvider::Codex => {
            Box::new(move |channels: RuntimeChannels| {
                Box::pin(run_agent_runtime(config, provider, channels))
            })
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
pub type PaneRunnerFactory = Box<dyn Fn(&AgentPaneSpec) -> PaneRunner + Send + Sync>;

impl AgentHost {
    pub fn with_journal(
        config: AgentSpawnConfig,
        sink: AgentStreamSink,
        journal: Option<Arc<AgentJournal>>,
    ) -> Self {
        Self {
            config: Mutex::new(config),
            sink: Arc::new(sink),
            journal,
            registry: Arc::new(PaneRegistry::default()),
            permission_ids: Arc::new(AtomicU64::new(1)),
            #[cfg(any(test, feature = "test-support"))]
            runner_factory: Mutex::new(None),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_runner_factory(&self, factory: PaneRunnerFactory) {
        *self.runner_factory.lock() = Some(factory);
    }

    pub fn prewarm(&self) {
        warm_adapter_cache(&self.config.lock().commands());
    }

    pub fn reconfigure(&self, config: AgentSpawnConfig) {
        *self.config.lock() = config;
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn config(&self) -> AgentSpawnConfig {
        self.config.lock().clone()
    }

    #[cfg(test)]
    pub fn pane_count(&self) -> usize {
        self.registry.panes.lock().len()
    }

    pub fn contains(&self, pane: PaneId) -> bool {
        self.registry.panes.lock().contains_key(&pane)
    }

    pub fn open(&self, pane: PaneId, generation: u64, spec: AgentPaneSpec) -> bool {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(runner) = self
            .runner_factory
            .lock()
            .as_ref()
            .map(|factory| factory(&spec))
        {
            return self.open_with(pane, generation, spec, runner);
        }
        let mut config = self.config.lock().clone();
        config.workspace.adopt_pane_identity(&spec.workspace);
        let runner = pane_runner(config, spec.provider);
        self.open_with(pane, generation, spec, runner)
    }

    pub fn open_with(
        &self,
        pane: PaneId,
        generation: u64,
        spec: AgentPaneSpec,
        runner: PaneRunner,
    ) -> bool {
        let mut panes = self.registry.panes.lock();
        if panes.contains_key(&pane) {
            return false;
        }
        let runtime_auto_approve = Arc::new(Mutex::new(
            spec.auto_approve.unwrap_or(self.config.lock().auto_approve),
        ));
        let auto_approve = Arc::clone(&runtime_auto_approve);
        let state = Arc::new(Mutex::new(AgentPaneState::new(&spec)));
        let pane_state = Arc::clone(&state);
        let sink = Arc::clone(&self.sink);
        let permission_ids = Arc::clone(&self.permission_ids);
        let journal = self.journal.clone();
        let (inbox_tx, inbox_rx) = async_channel::bounded(PANE_INBOX_CAPACITY);
        let (control_tx, control_rx) = async_channel::bounded(RUNTIME_CONTROL_CAPACITY);
        let (prompt_tx, prompt_rx) = async_channel::bounded(MAX_AGENT_QUEUED_PROMPTS);
        let (close_tx, close_rx) = async_channel::bounded::<()>(1);
        let inbox = inbox_tx.clone();
        let pane_close = close_tx.clone();
        let ended = Arc::new(AtomicBool::new(false));
        let pane_ended = Arc::clone(&ended);
        let thread = std::thread::Builder::new()
            .name(format!("zz-agent-{}", pane.0))
            .spawn(move || {
                futures_lite::future::block_on(run_pane(
                    pane,
                    generation,
                    spec,
                    runtime_auto_approve,
                    pane_state,
                    sink,
                    runner,
                    permission_ids,
                    journal,
                    pane_ended,
                    inbox,
                    inbox_rx,
                    control_rx,
                    prompt_rx,
                    pane_close,
                    close_rx,
                ));
            });
        let thread = match thread {
            Ok(thread) => thread,
            Err(error) => {
                log::error!(target: "zz::agent", "could not start the agent thread for pane {pane}: {error}");
                return false;
            }
        };
        panes.insert(
            pane,
            PaneHandle {
                inbox: inbox_tx,
                control: control_tx,
                prompts: prompt_tx,
                close: close_tx,
                state,
                auto_approve,
                ended,
                thread,
            },
        );
        true
    }

    pub fn command(&self, pane: PaneId, command: HostCommand) -> Result<(), HostCommand> {
        let panes = self.registry.panes.lock();
        let Some(handle) = panes.get(&pane) else {
            return Err(command);
        };
        let command = match command {
            HostCommand::Prompt(prompt) if prompt.source != PromptSource::Injected => {
                return handle
                    .prompts
                    .try_send(prompt)
                    .map_err(|error| HostCommand::Prompt(error.into_inner()));
            }
            command => command,
        };
        if matches!(
            &command,
            HostCommand::Cancel
                | HostCommand::RespondPermission { .. }
                | HostCommand::AnswerQuestion { .. }
        ) {
            return handle
                .control
                .try_send(command)
                .map_err(async_channel::TrySendError::into_inner);
        }
        handle
            .inbox
            .try_send(PaneInput::Command(command))
            .map_err(|error| match error.into_inner() {
                PaneInput::Command(command) => command,
                PaneInput::Event(_) | PaneInput::GitSummary { .. } | PaneInput::Finished(_) => {
                    unreachable!()
                }
            })
    }

    pub fn pane_state(&self, pane: PaneId) -> Weak<Mutex<AgentPaneState>> {
        self.registry
            .panes
            .lock()
            .get(&pane)
            .map_or_else(Weak::new, |handle| Arc::downgrade(&handle.state))
    }

    pub fn set_auto_approve(&self, pane: PaneId, tier: AgentAutoApprove) {
        if let Some(handle) = self.registry.panes.lock().get(&pane) {
            *handle.auto_approve.lock() = tier;
        }
    }

    pub fn close(&self, pane: PaneId) -> Option<JoinHandle<()>> {
        let handle = self.registry.panes.lock().remove(&pane)?;
        handle.close.close();
        Some(handle.thread)
    }

    pub fn end(&self, pane: PaneId) -> Option<JoinHandle<()>> {
        if let Some(handle) = self.registry.panes.lock().get(&pane) {
            handle.ended.store(true, Ordering::Release);
        }
        self.close(pane)
    }

    pub fn snapshot_state(&self, pane: PaneId) -> Option<AgentPaneState> {
        let panes = self.registry.panes.lock();
        panes.get(&pane).map(|handle| handle.state.lock().clone())
    }

    pub fn shutdown(&self) {
        let handles = std::mem::take(&mut *self.registry.panes.lock());
        for handle in handles.values() {
            handle.close.close();
        }
        for handle in handles.into_values() {
            let _ = handle.thread.join();
        }
        crate::child::join_reapers();
    }
}

impl Drop for AgentHost {
    fn drop(&mut self) {
        self.shutdown();
    }
}

async fn run_pane(
    pane: PaneId,
    generation: u64,
    spec: AgentPaneSpec,
    auto_approve: Arc<Mutex<AgentAutoApprove>>,
    state: Arc<Mutex<AgentPaneState>>,
    sink: Arc<AgentStreamSink>,
    runner: PaneRunner,
    permission_ids: Arc<AtomicU64>,
    journal: Option<Arc<AgentJournal>>,
    ended: Arc<AtomicBool>,
    inbox_tx: Sender<PaneInput>,
    inbox_rx: Receiver<PaneInput>,
    host_control_rx: Receiver<HostCommand>,
    prompt_rx: Receiver<QueuedPrompt>,
    close_tx: Sender<()>,
    close_rx: Receiver<()>,
) {
    let (command_tx, command_rx) = async_channel::bounded(RUNTIME_COMMAND_CAPACITY);
    let (runtime_control_tx, runtime_control_rx) = async_channel::bounded(RUNTIME_CONTROL_CAPACITY);
    let (event_tx, event_rx) = async_channel::bounded(RUNTIME_EVENT_CAPACITY);
    let runtime_close = close_rx.clone();
    let closer = event_tx.clone();
    let outcome = Arc::new(Mutex::new(None));

    let runtime_outcome = Arc::clone(&outcome);
    let runtime = async move {
        let result = futures_lite::future::race(
            runner(RuntimeChannels {
                auto_approve,
                permission_ids,
                journal,
                ended,
                commands: command_rx,
                controls: runtime_control_rx,
                events: event_tx,
            }),
            async move {
                let _ = runtime_close.recv().await;
                Ok(())
            },
        )
        .await;
        *runtime_outcome.lock() = Some(result);
        closer.close();
    };

    let pump_inbox = inbox_tx.clone();
    let forward = async move {
        while let Ok(payload) = event_rx.recv().await {
            if inbox_tx.send(PaneInput::Event(payload)).await.is_err() {
                break;
            }
        }
        let result = outcome.lock().take().unwrap_or(Ok(()));
        let _ = inbox_tx.send(PaneInput::Finished(result)).await;
        inbox_tx.close();
    };

    let pump = PanePump {
        pane,
        generation,
        spec,
        state,
        sink,
        inbox: pump_inbox,
        commands: command_tx,
        prompts: prompt_rx.clone(),
        controls: runtime_control_tx,
        close: close_tx,
        seq: 0,
        queue: VecDeque::new(),
        git_refresh: GitRefreshGate::default(),
        next_turn_id: 0,
        active_turn: None,
        dispatched_prompt: None,
        active_waiter: None,
        joined_waiters: Vec::new(),
        session_waiter: None,
        new_session_pending: false,
        turn_reply: AgentTurnReply::default(),
        turn_started: Instant::now(),
        closing: false,
    };

    futures_lite::future::zip(
        futures_lite::future::zip(runtime, forward),
        pump.run(inbox_rx, host_control_rx, prompt_rx, close_rx),
    )
    .await;
}

struct PanePump {
    pane: PaneId,
    generation: u64,
    spec: AgentPaneSpec,
    state: Arc<Mutex<AgentPaneState>>,
    sink: Arc<AgentStreamSink>,
    inbox: Sender<PaneInput>,
    commands: Sender<RuntimeCommand>,
    prompts: Receiver<QueuedPrompt>,
    controls: Sender<RuntimeControl>,
    close: Sender<()>,
    seq: u64,
    queue: VecDeque<QueuedPrompt>,
    git_refresh: GitRefreshGate,
    next_turn_id: u64,
    active_turn: Option<u64>,
    dispatched_prompt: Option<(u64, AgentPrompt)>,
    active_waiter: Option<AgentTurnWaiter>,
    joined_waiters: Vec<AgentTurnWaiter>,
    session_waiter: Option<zz_daemon_client::reply::Reply<Result<(), String>>>,
    new_session_pending: bool,
    turn_reply: AgentTurnReply,
    turn_started: Instant,
    closing: bool,
}

#[derive(Default)]
struct GitRefreshGate {
    latest: u64,
    running: Option<u64>,
    pending: Option<u64>,
}

impl GitRefreshGate {
    fn request(&mut self) -> Option<u64> {
        self.latest = self.latest.saturating_add(1);
        if self.running.is_some() {
            self.pending = Some(self.latest);
            None
        } else {
            self.running = Some(self.latest);
            Some(self.latest)
        }
    }

    fn invalidate(&mut self) {
        self.latest = self.latest.saturating_add(1);
        self.pending = None;
    }

    fn complete(&mut self, refresh: u64) -> Option<u64> {
        if self.running != Some(refresh) {
            return None;
        }
        self.running = self.pending.take();
        self.running
    }

    fn spawn_failed(&mut self, refresh: u64) {
        if self.running == Some(refresh) {
            self.running = None;
            self.pending = None;
        }
    }
}

enum FollowUp {
    None,
    DrainQueue,
    ReclaimQueue,
    Reopen,
}

impl PanePump {
    async fn run(
        mut self,
        inbox: Receiver<PaneInput>,
        control: Receiver<HostCommand>,
        prompts: Receiver<QueuedPrompt>,
        close: Receiver<()>,
    ) {
        let state = self.state.lock().clone();
        (self.sink)(self.pane, self.generation, state, None);
        self.send(RuntimeCommand::Open {
            cwd: self.spec.cwd.clone(),
            resume_session: self.spec.resume_session.clone(),
        });
        loop {
            let input = futures_lite::future::race(
                async {
                    let _ = close.recv().await;
                    None
                },
                futures_lite::future::race(
                    async { control.recv().await.ok().map(PaneInput::Command) },
                    futures_lite::future::race(
                        async {
                            prompts
                                .recv()
                                .await
                                .ok()
                                .map(HostCommand::Prompt)
                                .map(PaneInput::Command)
                        },
                        async { inbox.recv().await.ok() },
                    ),
                ),
            )
            .await;
            let Some(input) = input else {
                self.close();
                self.reclaim_accepted_inputs(&prompts, &inbox);
                break;
            };
            match input {
                PaneInput::Event(payload) => self.observe(payload),
                PaneInput::Command(command) => self.command(command),
                PaneInput::GitSummary {
                    generation,
                    refresh,
                    cwd,
                    summary,
                } => self.finish_git_refresh(generation, refresh, &cwd, summary),
                PaneInput::Finished(result) => {
                    self.finish(&result);
                    self.reclaim_accepted_inputs(&prompts, &inbox);
                    break;
                }
            }
        }
    }

    fn reclaim_accepted_inputs(
        &mut self,
        prompts: &Receiver<QueuedPrompt>,
        inbox: &Receiver<PaneInput>,
    ) {
        while let Ok(queued) = prompts.try_recv() {
            self.reclaim_prompts(vec![queued], &AgentTurnFailure::Closed);
        }
        while let Ok(input) = inbox.try_recv() {
            match input {
                PaneInput::Command(HostCommand::Prompt(queued)) => {
                    self.reclaim_prompts(vec![queued], &AgentTurnFailure::Closed);
                }
                PaneInput::Command(HostCommand::Steer(prompt)) => {
                    self.emit(AgentStreamPayload::PromptsReclaimed {
                        prompts: vec![prompt],
                    });
                }
                PaneInput::Command(HostCommand::NewSession {
                    reply: Some(reply), ..
                }) => {
                    reply.try_send(Err("agent closed".to_owned()));
                }
                _ => {}
            }
        }
    }

    fn respond_permission(&mut self, request_id: u64, option_id: Option<String>) -> bool {
        let kind = self
            .state
            .lock()
            .pending_permissions
            .iter()
            .find(|pending| pending.request_id == request_id)
            .and_then(|pending| pending.options.as_array())
            .and_then(|options| {
                options.iter().find(|option| {
                    option.get("optionId").and_then(Value::as_str) == option_id.as_deref()
                })
            })
            .and_then(|option| option.get("kind").and_then(Value::as_str))
            .map(str::to_owned);
        let denied =
            option_id.is_none() || matches!(kind.as_deref(), Some("reject_once" | "reject_always"));
        if self.send_control(RuntimeControl::RespondPermission {
            request_id,
            option_id,
        }) {
            if self.active_waiter.is_some() {
                if denied {
                    self.turn_reply.permissions_denied += 1;
                } else if matches!(kind.as_deref(), Some("allow_once" | "allow_always")) {
                    self.turn_reply.permissions_allowed += 1;
                }
            }
            true
        } else {
            self.fail_control();
            false
        }
    }

    fn command(&mut self, command: HostCommand) {
        match command {
            HostCommand::Prompt(prompt) => self.prompt(prompt),
            HostCommand::Join(waiter) => {
                if self.active_turn.is_some() {
                    self.joined_waiters.push(waiter);
                } else {
                    waiter.reply.try_send(Err(AgentTurnFailure::NoTurn));
                }
            }
            HostCommand::Steer(prompt) => self.steer(prompt),
            HostCommand::Cancel => self.cancel(),
            HostCommand::Unqueue => self.unqueue(),
            HostCommand::RespondPermission { response } => match response {
                PermissionResponse::Exact {
                    request_id,
                    option_id,
                } => {
                    self.respond_permission(request_id, option_id);
                }
                PermissionResponse::Select {
                    request_id,
                    choice,
                    reply,
                } => {
                    let selected = {
                        let state = self.state.lock();
                        let pending = &state.pending_permissions;
                        select_permission(pending, request_id, &choice)
                            .map(|(request_id, option_id)| (request_id, Some(option_id)))
                            .or_else(|error| match choice {
                                PermissionChoice::Deny => pending
                                    .iter()
                                    .find(|permission| {
                                        request_id.is_none_or(|id| permission.request_id == id)
                                    })
                                    .filter(|permission| !permission.questions.is_empty())
                                    .map(|permission| (permission.request_id, None))
                                    .ok_or(error),
                                _ => Err(error),
                            })
                    };
                    let result = selected.and_then(|(request_id, option_id)| {
                        self.respond_permission(request_id, option_id.clone())
                            .then(|| option_id.unwrap_or_else(|| "dismissed".to_owned()))
                            .ok_or_else(|| "agent runtime is busy".to_owned())
                    });
                    reply.try_send(result);
                }
            },
            HostCommand::AnswerQuestion {
                request_id,
                answers,
            } => {
                if self.send_control(RuntimeControl::AnswerQuestion {
                    request_id,
                    answers,
                }) && self.active_waiter.is_some()
                {
                    self.turn_reply.permissions_allowed += 1;
                }
            }
            HostCommand::StopTask { task_id } => {
                self.send(RuntimeCommand::StopTask { task_id });
            }
            HostCommand::RefreshGit => self.start_git_refresh(),
            HostCommand::Authenticate { method_id } => {
                if !self.send(RuntimeCommand::Authenticate { method_id }) {
                    self.observe(AgentStreamPayload::AuthenticationFailed {
                        message: "agent runtime is busy".to_owned(),
                    });
                }
            }
            HostCommand::SetConfigOption { option_id, value } => {
                if !self.send(RuntimeCommand::SetConfigOption {
                    option_id: option_id.clone(),
                    value,
                }) {
                    self.observe(AgentStreamPayload::SettingFailed {
                        option_id,
                        message: "agent runtime is busy".to_owned(),
                    });
                }
            }
            HostCommand::SetMode { mode_id } => {
                if !self.send(RuntimeCommand::SetMode { mode_id }) {
                    self.observe(AgentStreamPayload::SettingFailed {
                        option_id: "legacy-session-mode".to_owned(),
                        message: "agent runtime is busy".to_owned(),
                    });
                }
            }
            HostCommand::ListSessions {
                client,
                cwd,
                cursor,
                replace,
            } => {
                if !self.send(RuntimeCommand::ListSessions {
                    client,
                    cwd,
                    cursor,
                    replace,
                }) {
                    self.observe(AgentStreamPayload::SessionListFailed {
                        client,
                        message: "agent runtime is busy".to_owned(),
                    });
                }
            }
            HostCommand::NewSession { cwd, reply } => self.begin_session_change(
                RuntimeCommand::NewSession { cwd },
                AgentConnectionPhase::Starting,
                reply,
            ),
            HostCommand::SwitchSession { session } => {
                self.begin_session_change(
                    RuntimeCommand::SwitchSession { session },
                    AgentConnectionPhase::Restoring,
                    None,
                );
            }
            HostCommand::DeleteSession { client, session_id } => {
                if !self.send(RuntimeCommand::DeleteSession { client, session_id }) {
                    self.observe(AgentStreamPayload::SessionDeleteFailed {
                        client,
                        message: "agent runtime is busy".to_owned(),
                    });
                }
            }
        }
    }

    fn begin_session_change(
        &mut self,
        command: RuntimeCommand,
        phase: AgentConnectionPhase,
        reply: Option<zz_daemon_client::reply::Reply<Result<(), String>>>,
    ) {
        let accepted = {
            let mut state = self.state.lock();
            if state.phase != AgentConnectionPhase::Ready
                || !state
                    .pending_permissions
                    .iter()
                    .all(AgentPendingPermission::outlives_turn)
                || self.active_turn.is_some()
            {
                false
            } else {
                state.phase = phase;
                state.error = None;
                true
            }
        };
        if !accepted {
            let message = "finish or cancel the current turn before changing sessions".to_owned();
            self.emit(AgentStreamPayload::SessionSwitchFailed {
                message: message.clone(),
            });
            if let Some(reply) = reply {
                reply.try_send(Err(message));
            }
            return;
        }
        self.session_waiter = reply;
        self.new_session_pending = matches!(&command, RuntimeCommand::NewSession { .. });
        let state = self.state.lock().clone();
        (self.sink)(self.pane, self.generation, state, None);
        if !self.send(command) {
            self.observe(AgentStreamPayload::SessionSwitchFailed {
                message: "agent runtime is unavailable".to_owned(),
            });
        }
    }

    fn observe(&mut self, payload: AgentStreamPayload) {
        if matches!(&payload, AgentStreamPayload::PromptAccepted { .. }) {
            return;
        }
        if let AgentStreamPayload::TitleChanged { title } = payload {
            let state = {
                let mut state = self.state.lock();
                if state.title == title {
                    return;
                }
                state.title = title;
                state.clone()
            };
            (self.sink)(self.pane, self.generation, state, None);
            return;
        }
        let payload = match payload {
            AgentStreamPayload::PromptsReclaimed { prompts } => {
                let prompts = self.requeue_relayed(prompts);
                if prompts.is_empty() {
                    return;
                }
                AgentStreamPayload::PromptsReclaimed { prompts }
            }
            payload => payload,
        };
        if let AgentStreamPayload::QuotaChanged { quota } = payload {
            let state = {
                let mut state = self.state.lock();
                if !state.quota.merge(quota) {
                    return;
                }
                state.clone()
            };
            (self.sink)(self.pane, self.generation, state, None);
            return;
        }
        if let AgentStreamPayload::PromptFinished { turn_id, .. } = &payload
            && self.active_turn != Some(*turn_id)
        {
            return;
        }
        if let AgentStreamPayload::AuthenticationFailed { message }
        | AgentStreamPayload::PaneFailed { message } = &payload
        {
            self.reclaim_dispatched_prompt(AgentTurnFailure::Failed(message.clone()));
        }
        if let AgentStreamPayload::Update { update } = &payload {
            self.record_reply_chunk(update);
        }
        if let AgentStreamPayload::TurnStarted { turn_id } = &payload
            && self.active_turn == Some(*turn_id)
        {
            self.turn_started = Instant::now();
        }
        if self.active_waiter.is_some()
            && matches!(&payload, AgentStreamPayload::PermissionRequested { .. })
        {
            self.turn_reply.permissions_requested += 1;
        }
        self.turn_reply.duration = self.turn_started.elapsed();
        let mut follow = FollowUp::None;
        let mut turn_result = None;
        if matches!(
            &payload,
            AgentStreamPayload::SessionReset { .. } | AgentStreamPayload::SessionSwitched { .. }
        ) {
            self.active_turn = None;
            self.reclaim_dispatched_prompt(AgentTurnFailure::Reclaimed);
        }
        if matches!(&payload, AgentStreamPayload::SessionReset { .. }) {
            self.git_refresh.invalidate();
            if self.new_session_pending {
                for _ in 0..self.prompts.len() {
                    let Ok(queued) = self.prompts.try_recv() else {
                        break;
                    };
                    self.reclaim_prompts(vec![queued], &AgentTurnFailure::Reclaimed);
                }
                self.reclaim_queue();
            }
        }
        if matches!(&payload, AgentStreamPayload::SessionReady { .. }) {
            for (option_id, value) in [
                ("model", self.spec.model.take()),
                ("effort", self.spec.effort.take()),
            ] {
                if let Some(value) = value {
                    self.send(RuntimeCommand::SetConfigOption {
                        option_id: option_id.to_owned(),
                        value,
                    });
                }
            }
        }
        let refresh_git = matches!(
            &payload,
            AgentStreamPayload::SessionReady { .. }
                | AgentStreamPayload::SessionSwitched { .. }
                | AgentStreamPayload::PromptFinished { .. }
        );
        {
            let mut state = self.state.lock();
            match &payload {
                AgentStreamPayload::Ready {
                    agent_name,
                    agent_key,
                    auth_methods,
                    capabilities,
                } => {
                    state.agent_name = Some(agent_name.clone());
                    state.agent_key = Some(agent_key.clone());
                    state.auth_methods.clone_from(auth_methods);
                    state.capabilities = *capabilities;
                }
                AgentStreamPayload::SessionReset { restoring } => {
                    state.phase = if *restoring {
                        AgentConnectionPhase::Restoring
                    } else {
                        AgentConnectionPhase::Starting
                    };
                    state.error = None;
                    state.git = None;
                    state.tasks.clear();
                    state.pending_permissions.clear();
                }
                AgentStreamPayload::SessionReady { session_id, .. } => {
                    state.session_id = Some(session_id.clone());
                    state.phase = AgentConnectionPhase::Ready;
                    state.error = None;
                    follow = FollowUp::DrainQueue;
                }
                AgentStreamPayload::SessionSwitched {
                    session_id, cwd, ..
                } => {
                    state.session_id = Some(session_id.clone());
                    state.cwd.clone_from(cwd);
                    state.phase = AgentConnectionPhase::Ready;
                    state.error = None;
                    settle_turn(&mut state);
                    follow = FollowUp::DrainQueue;
                }
                AgentStreamPayload::PromptAccepted { .. } => unreachable!(),
                AgentStreamPayload::SessionSwitchFailed { message } => {
                    state.phase = AgentConnectionPhase::Ready;
                    state.error = Some(message.clone());
                    follow = FollowUp::DrainQueue;
                }
                AgentStreamPayload::SettingFailed { message, .. } => {
                    state.error = Some(message.clone());
                }
                AgentStreamPayload::PermissionRequested {
                    request_id,
                    tool_call,
                    options,
                    questions,
                } => state.pending_permissions.push(AgentPendingPermission {
                    request_id: *request_id,
                    tool_call: tool_call.clone(),
                    options: options.clone(),
                    questions: questions.clone(),
                }),
                AgentStreamPayload::TasksChanged { tasks } => {
                    state.tasks = tasks.iter().take(MAX_AGENT_TASKS).cloned().collect();
                }
                AgentStreamPayload::Activity { busy } => {
                    if *busy {
                        if self.active_turn.is_none() && state.phase == AgentConnectionPhase::Ready
                        {
                            state.phase = AgentConnectionPhase::Running;
                        }
                    } else if self.active_turn.is_none() && state.phase.has_active_turn() {
                        state.phase = AgentConnectionPhase::Ready;
                        follow = FollowUp::DrainQueue;
                    }
                }
                AgentStreamPayload::PermissionResolved { request_id, .. } => state
                    .pending_permissions
                    .retain(|pending| pending.request_id != *request_id),
                AgentStreamPayload::PromptFinished { outcome, .. } => {
                    self.active_turn = None;
                    self.dispatched_prompt = None;
                    match outcome {
                        AgentPromptOutcome::Finished { stop_reason } => {
                            state.phase = AgentConnectionPhase::Ready;
                            settle_turn(&mut state);
                            let reason = stop_reason
                                .as_str()
                                .map_or_else(|| stop_reason.to_string(), str::to_owned);
                            let cancelled = reason == "cancelled";
                            self.turn_reply.stop_reason.clone_from(&reason);
                            let reply = std::mem::take(&mut self.turn_reply);
                            turn_result = Some(match reason.as_str() {
                                "cancelled" => Err(AgentTurnFailure::Cancelled),
                                "end_turn" => Ok(reply),
                                _ => Err(AgentTurnFailure::Stopped {
                                    reason,
                                    reply: Box::new(reply),
                                }),
                            });
                            follow = if cancelled {
                                FollowUp::ReclaimQueue
                            } else {
                                FollowUp::DrainQueue
                            };
                        }
                        AgentPromptOutcome::Failed { message } => {
                            state.phase = AgentConnectionPhase::Ready;
                            state.error = Some(message.clone());
                            settle_turn(&mut state);
                            turn_result = Some(Err(AgentTurnFailure::Failed(message.clone())));
                            follow = FollowUp::ReclaimQueue;
                        }
                    }
                }
                AgentStreamPayload::Authenticated => {
                    state.error = None;
                    follow = FollowUp::Reopen;
                }
                AgentStreamPayload::AuthenticationFailed { message }
                | AgentStreamPayload::PaneFailed { message } => {
                    self.active_turn = None;
                    state.phase = AgentConnectionPhase::Failed;
                    state.error = Some(message.clone());
                    settle_turn(&mut state);
                    follow = FollowUp::ReclaimQueue;
                }
                AgentStreamPayload::ConfigOptionsChanged { .. }
                | AgentStreamPayload::ModeChanged { .. } => state.error = None,
                AgentStreamPayload::Update { .. }
                | AgentStreamPayload::StateSynced { .. }
                | AgentStreamPayload::TurnStarted { .. }
                | AgentStreamPayload::SessionsListed { .. }
                | AgentStreamPayload::SessionListFailed { .. }
                | AgentStreamPayload::SessionDeleted { .. }
                | AgentStreamPayload::SessionDeleteFailed { .. }
                | AgentStreamPayload::PromptsReclaimed { .. }
                | AgentStreamPayload::PromptsRestored { .. }
                | AgentStreamPayload::Chat { .. }
                | AgentStreamPayload::TitleChanged { .. }
                | AgentStreamPayload::QuotaChanged { .. } => {}
            }
        }
        let permission = if turn_result.is_none()
            && let AgentStreamPayload::PermissionRequested {
                request_id,
                tool_call,
                options,
                questions,
            } = &payload
        {
            Some(AgentPendingPermission {
                request_id: *request_id,
                tool_call: tool_call.clone(),
                options: options.clone(),
                questions: questions.clone(),
            })
        } else {
            None
        };
        if let Some(result) = turn_result {
            self.settle_active_turn(result);
        }
        let session_result = match &payload {
            AgentStreamPayload::SessionSwitched { .. } => Some(Ok(())),
            AgentStreamPayload::SessionSwitchFailed { message }
            | AgentStreamPayload::PaneFailed { message }
            | AgentStreamPayload::AuthenticationFailed { message } => Some(Err(message.clone())),
            _ => None,
        };
        self.emit(payload);
        if let Some(result) = session_result {
            self.settle_session_change(result);
        }
        if let Some(permission) = permission {
            self.handle_waited_permission(&permission);
        }
        if refresh_git {
            self.start_git_refresh();
        }
        match follow {
            FollowUp::None => {}
            FollowUp::DrainQueue => self.dispatch_next(),
            FollowUp::ReclaimQueue => self.reclaim_queue(),
            FollowUp::Reopen => {
                let (cwd, session_id) = {
                    let state = self.state.lock();
                    (state.cwd.clone(), state.session_id.clone())
                };
                self.send(RuntimeCommand::Open {
                    cwd,
                    resume_session: session_id,
                });
            }
        }
    }

    fn prompt(&mut self, mut queued: QueuedPrompt) {
        if queued.prompt.is_empty() {
            queued.settle(Err(AgentTurnFailure::Reclaimed));
            return;
        }
        let (verbs, running) = {
            let state = self.state.lock();
            (
                state.capabilities.verbs,
                state.phase == AgentConnectionPhase::Running,
            )
        };
        if verbs && let Some((verb, rest)) = agent_verb(&queued.prompt.text) {
            let idle_steer =
                (verb == "steer" && !running && !rest.is_empty()).then(|| rest.to_owned());
            if let Some(text) = idle_steer {
                queued.prompt.text = text;
            } else {
                let prompt = std::mem::take(&mut queued.prompt);
                if self
                    .commands
                    .try_send(RuntimeCommand::Verb { prompt })
                    .is_err()
                {
                    queued.settle(Err(AgentTurnFailure::Failed(
                        "agent is not accepting commands".to_owned(),
                    )));
                    return;
                }
                queued.settle(Ok(AgentTurnReply {
                    stop_reason: "zz_command".to_owned(),
                    ..AgentTurnReply::default()
                }));
                return;
            }
        }
        if self.state.lock().phase.accepts_prompt() {
            if !self.answered_ask(&queued) {
                self.dispatch(queued);
            }
            return;
        }
        let injected = queued.source == PromptSource::Injected;
        if injected && (self.answered_ask(&queued) || self.deliver_into_turn(&queued.prompt)) {
            return;
        }
        let queued_bytes = self.queue.iter().fold(0usize, |total, queued| {
            total.saturating_add(queued.prompt.byte_len())
        });
        let over_bytes =
            queued_bytes.saturating_add(queued.prompt.byte_len()) > MAX_AGENT_PROMPT_BYTES;
        if injected && over_bytes {
            log::warn!(
                target: "zz::agent",
                "{} dropped a {} byte message because its queue is full",
                self.pane,
                queued.prompt.byte_len()
            );
            return;
        }
        let waiting = waiting_prompts(&self.queue);
        if !injected && (waiting >= MAX_AGENT_QUEUED_PROMPTS || over_bytes) {
            self.reclaim_prompts(vec![queued], &AgentTurnFailure::Reclaimed);
            return;
        }
        self.queue.push_back(queued);
        self.publish_queue_depth();
    }

    fn steerable(&self) -> bool {
        let state = self.state.lock();
        state.phase == AgentConnectionPhase::Running
            && state
                .pending_permissions
                .iter()
                .all(AgentPendingPermission::outlives_turn)
            && state.capabilities.verbs
    }

    fn deliver_into_turn(&self, prompt: &AgentPrompt) -> bool {
        let text = prompt.text.trim();
        if text.is_empty() || !prompt.images.is_empty() || !self.steerable() {
            return false;
        }
        let in_turn = timed_report(text, ReportTiming::InTurn);
        let steer = AgentPrompt {
            text: format!("/steer {}", in_turn.as_deref().unwrap_or(text)),
            ..prompt.clone()
        };
        self.commands
            .try_send(RuntimeCommand::Verb { prompt: steer })
            .is_ok()
    }

    fn steer(&mut self, prompt: AgentPrompt) {
        let steerable = self.steerable();
        let text = prompt.text.trim();
        if !steerable || text.is_empty() || !prompt.images.is_empty() {
            self.prompt(prompt.into());
            return;
        }
        let steer = AgentPrompt {
            text: format!("/steer {text}"),
            ..prompt.clone()
        };
        if self
            .commands
            .try_send(RuntimeCommand::Verb { prompt: steer })
            .is_err()
        {
            self.emit(AgentStreamPayload::PromptsReclaimed {
                prompts: vec![prompt],
            });
        }
    }

    fn dispatch(&mut self, queued: QueuedPrompt) {
        let source = queued.source;
        let extra = queued.note;
        let note = source.note(&queued.prompt.text, extra.as_deref());
        self.next_turn_id = self.next_turn_id.saturating_add(1).max(1);
        let turn_id = self.next_turn_id;
        self.active_turn = Some(turn_id);
        self.active_waiter = queued.waiter;
        self.turn_reply = AgentTurnReply::default();
        self.turn_started = Instant::now();
        {
            let mut state = self.state.lock();
            state.phase = AgentConnectionPhase::Running;
            state.error = None;
        }
        let prompt = queued.prompt;
        let retained = prompt.clone();
        match self.commands.try_send(RuntimeCommand::Prompt {
            turn_id,
            prompt,
            note,
        }) {
            Ok(()) => {
                self.dispatched_prompt =
                    (source == PromptSource::Composer).then_some((turn_id, retained));
                self.emit(AgentStreamPayload::TurnStarted { turn_id });
            }
            Err(error) => {
                self.active_turn = None;
                {
                    let mut state = self.state.lock();
                    state.phase = AgentConnectionPhase::Ready;
                    settle_turn(&mut state);
                }
                let RuntimeCommand::Prompt { prompt, .. } = error.into_inner() else {
                    return;
                };
                self.queue.push_front(QueuedPrompt {
                    prompt,
                    waiter: self.active_waiter.take(),
                    source,
                    note: extra,
                    ask: None,
                });
                self.reclaim_queue();
            }
        }
    }

    fn dispatch_next(&mut self) {
        while let Some(queued) = self.queue.pop_front() {
            if self.answered_ask(&queued) {
                continue;
            }
            self.publish_queue_depth();
            self.dispatch(queued);
            return;
        }
        self.publish_queue_depth();
    }

    fn answered_ask(&self, queued: &QueuedPrompt) -> bool {
        let Some(ask) = queued.ask.as_ref().filter(|ask| ask.answered()) else {
            return false;
        };
        log::info!(
            target: "zz::agent",
            "request {} was answered before {} took its ask; dropped it",
            ask.request,
            self.pane
        );
        true
    }

    fn reclaim_queue(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        let queued = self.queue.drain(..).collect::<Vec<_>>();
        self.publish_queue_depth();
        self.reclaim_prompts(queued, &AgentTurnFailure::Reclaimed);
    }

    fn unqueue(&mut self) {
        let (own, kept): (Vec<_>, Vec<_>) = self
            .queue
            .drain(..)
            .partition(|queued| queued.source == PromptSource::Composer);
        self.queue = kept.into();
        if own.is_empty() {
            return;
        }
        self.publish_queue_depth();
        self.reclaim_prompts(own, &AgentTurnFailure::Reclaimed);
    }

    fn requeue_relayed(&mut self, prompts: Vec<AgentPrompt>) -> Vec<AgentPrompt> {
        let (relayed, rest): (Vec<_>, Vec<_>) = prompts
            .into_iter()
            .partition(|prompt| is_relayed(&prompt.text));
        if relayed.is_empty() {
            return rest;
        }
        for mut prompt in relayed.into_iter().rev() {
            if let Some(text) = timed_report(&prompt.text, ReportTiming::AfterTurn) {
                prompt.text = text;
            }
            self.queue.push_front(QueuedPrompt {
                prompt,
                waiter: None,
                source: PromptSource::Injected,
                note: None,
                ask: None,
            });
        }
        if self.state.lock().phase.accepts_prompt() {
            self.dispatch_next();
        } else {
            self.publish_queue_depth();
        }
        rest
    }

    fn reclaim_prompts(&mut self, queued: Vec<QueuedPrompt>, failure: &AgentTurnFailure) {
        let prompts = queued
            .into_iter()
            .filter_map(|queued| {
                let returned = queued.source == PromptSource::Composer;
                let prompt = queued.into_reclaimed(failure.clone());
                returned.then_some(prompt)
            })
            .collect::<Vec<_>>();
        if !prompts.is_empty() {
            self.emit(AgentStreamPayload::PromptsReclaimed { prompts });
        }
    }

    fn settle_active_turn(&mut self, result: AgentTurnResult) {
        for waiter in self.joined_waiters.drain(..) {
            waiter.reply.try_send(result.clone());
        }
        if let Some(waiter) = self.active_waiter.take() {
            waiter.reply.try_send(result);
        }
    }

    fn handle_waited_permission(&mut self, permission: &AgentPendingPermission) {
        let blocked = || {
            zz_protocol::AgentPermissionWire {
            request_id: permission.request_id,
            payload: serde_json::json!({ "toolCall": permission.tool_call, "options": permission.options }).to_string(),
        }
        };
        let (failing, waiting): (Vec<_>, Vec<_>) = self
            .joined_waiters
            .drain(..)
            .partition(|waiter| waiter.on_block == AgentBlockPolicy::Fail);
        self.joined_waiters = waiting;
        for waiter in failing {
            let mut reply = self.turn_reply.clone();
            "blocked".clone_into(&mut reply.stop_reason);
            waiter.reply.try_send(Err(AgentTurnFailure::Blocked {
                permission: blocked(),
                reply: Box::new(reply),
            }));
        }
        let Some(waiter) = &self.active_waiter else {
            return;
        };
        if let Some(asks) = &waiter.asks {
            (asks.0)(permission);
        }
        let policy = waiter.on_block;
        if policy == AgentBlockPolicy::Fail {
            "blocked".clone_into(&mut self.turn_reply.stop_reason);
            let reply = Box::new(std::mem::take(&mut self.turn_reply));
            if let Some(waiter) = self.active_waiter.take() {
                waiter.reply.try_send(Err(AgentTurnFailure::Blocked {
                    permission: blocked(),
                    reply,
                }));
            }
            return;
        }
        let question = serde_json::from_value(permission.options.clone()).map_or(
            true,
            |options: Vec<agent_client_protocol::v1::schema::PermissionOption>| {
                crate::runtime::is_user_question(&options)
            },
        );
        let (choice, action) = match policy {
            AgentBlockPolicy::Allow if !question => (PermissionChoice::Allow, "allowed"),
            AgentBlockPolicy::Deny => (PermissionChoice::Deny, "denied"),
            _ => return,
        };
        let selected = select_permission(
            std::slice::from_ref(permission),
            Some(permission.request_id),
            &choice,
        );
        let option_id = match selected {
            Ok((_, option_id)) => Some(option_id),
            Err(_) if question && policy == AgentBlockPolicy::Deny => None,
            Err(message) => {
                self.settle_active_turn(Err(AgentTurnFailure::Failed(message)));
                return;
            }
        };
        let audit = Arc::clone(&waiter.audit);
        if self.respond_permission(permission.request_id, option_id) {
            let kind = permission
                .tool_call
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let title = permission
                .tool_call
                .get("title")
                .or_else(|| permission.tool_call.get("toolCallId"))
                .and_then(Value::as_str)
                .unwrap_or("?");
            audit
                .lock()
                .push(format!("agent-send: {action} {kind} {title}"));
        }
    }

    fn record_reply_chunk(&mut self, update: &Value) {
        match update.get("sessionUpdate").and_then(Value::as_str) {
            Some("tool_call" | "tool_call_update") => {
                self.turn_reply.final_text.clear();
                if update.get("sessionUpdate").and_then(Value::as_str) == Some("tool_call") {
                    self.turn_reply.tool_calls += 1;
                }
                return;
            }
            Some("agent_message_chunk") => {}
            _ => return,
        }
        let Some(text) = update
            .get("content")
            .filter(|content| content.get("type").and_then(Value::as_str) == Some("text"))
            .and_then(|content| content.get("text"))
            .and_then(Value::as_str)
        else {
            return;
        };
        if update["_meta"]["zz"]["notice"] == true && text.ends_with(RESTORE_NOTICE) {
            return;
        }
        self.turn_reply.truncated |=
            append_reply_tail(&mut self.turn_reply.text, text, MAX_AGENT_TURN_REPLY_BYTES);
        self.turn_reply.truncated |= append_reply_tail(
            &mut self.turn_reply.final_text,
            text,
            MAX_AGENT_FINAL_REPLY_BYTES,
        );
    }

    fn cancel(&mut self) {
        if !self.state.lock().phase.has_active_turn() {
            return;
        }
        if self.active_turn.is_none() {
            let session_id = self.state.lock().session_id.clone();
            if self.send_control(RuntimeControl::Cancel {
                turn_id: 0,
                session_id,
            }) {
                self.state.lock().phase = AgentConnectionPhase::Cancelling;
            }
            return;
        }
        let session_id = self.state.lock().session_id.clone();
        let Some(turn_id) = self.active_turn else {
            return;
        };
        if !self.send_control(RuntimeControl::Cancel {
            turn_id,
            session_id,
        }) {
            self.fail_control();
            return;
        }
        self.state.lock().phase = AgentConnectionPhase::Cancelling;
        self.reclaim_queue();
    }

    fn start_git_refresh(&mut self) {
        let Some(refresh) = self.git_refresh.request() else {
            return;
        };
        self.spawn_git_refresh(refresh);
    }

    fn spawn_git_refresh(&mut self, refresh: u64) {
        let generation = self.generation;
        let cwd = self.state.lock().cwd.clone();
        let capture_cwd = cwd.clone();
        let inbox = self.inbox.clone();
        let pane = self.pane;
        if let Err(error) = thread::Builder::new()
            .name(format!("zz-agent-git-{}-{refresh}", pane.0))
            .spawn(move || {
                let summary = match git_summary::capture_git_summary(&capture_cwd) {
                    Ok(summary) => Some(summary),
                    Err(error) => {
                        log::debug!(target: "zz::agent", "no Git summary for pane {pane}: {error}");
                        None
                    }
                };
                let _ = inbox.send_blocking(PaneInput::GitSummary {
                    generation,
                    refresh,
                    cwd,
                    summary,
                });
            })
        {
            self.git_refresh.spawn_failed(refresh);
            log::warn!(target: "zz::agent", "could not start Git summary capture for pane {pane}: {error}");
        }
    }

    fn finish_git_refresh(
        &mut self,
        generation: u64,
        refresh: u64,
        cwd: &Path,
        summary: Option<AgentGitSummary>,
    ) {
        let next = self.git_refresh.complete(refresh);
        self.apply_git_summary(generation, refresh, cwd, summary);
        if let Some(refresh) = next {
            self.spawn_git_refresh(refresh);
        }
    }

    fn apply_git_summary(
        &mut self,
        generation: u64,
        refresh: u64,
        cwd: &Path,
        summary: Option<AgentGitSummary>,
    ) {
        if generation != self.generation || refresh != self.git_refresh.latest {
            return;
        }
        let state = {
            let mut state = self.state.lock();
            if state.cwd != cwd || state.git == summary {
                return;
            }
            state.git = summary;
            state.clone()
        };
        (self.sink)(self.pane, self.generation, state, None);
    }

    fn settle_session_change(&mut self, result: Result<(), String>) {
        self.new_session_pending = false;
        if let Some(reply) = self.session_waiter.take() {
            let result = if result.is_ok() && self.close.is_closed() {
                Err("agent closed".to_owned())
            } else {
                result
            };
            reply.try_send(result);
        }
    }

    fn close(&mut self) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.settle_session_change(Err("agent closed".to_owned()));
        self.resolve_pending_permissions();
        self.reclaim_dispatched_prompt(AgentTurnFailure::Closed);
        self.reclaim_queue();
        self.send(RuntimeCommand::Shutdown);
        self.close.close();
    }

    fn resolve_pending_permissions(&mut self) {
        let pending = std::mem::take(&mut self.state.lock().pending_permissions);
        for permission in pending {
            self.emit(AgentStreamPayload::PermissionResolved {
                request_id: permission.request_id,
                canceled: true,
            });
        }
    }

    fn reclaim_dispatched_prompt(&mut self, failure: AgentTurnFailure) {
        self.settle_active_turn(Err(failure));
        let Some((_, prompt)) = self.dispatched_prompt.take() else {
            return;
        };
        self.emit(AgentStreamPayload::PromptsReclaimed {
            prompts: vec![prompt],
        });
    }

    fn finish(&mut self, result: &Result<(), String>) {
        let closing = self.closing || self.close.is_closed();
        let message = result.as_ref().err().map_or_else(
            || "agent process disconnected unexpectedly".to_owned(),
            String::clone,
        );
        self.settle_session_change(Err(message.clone()));
        self.resolve_pending_permissions();
        self.reclaim_dispatched_prompt(if closing {
            AgentTurnFailure::Closed
        } else {
            AgentTurnFailure::Failed(message.clone())
        });
        self.reclaim_queue();
        if closing {
            self.state.lock().phase = AgentConnectionPhase::Disconnected;
            return;
        }
        {
            let mut state = self.state.lock();
            state.phase = AgentConnectionPhase::Failed;
            state.error = Some(message.clone());
            settle_turn(&mut state);
        }
        self.emit(AgentStreamPayload::PaneFailed { message });
    }

    fn send(&self, command: RuntimeCommand) -> bool {
        if self.commands.try_send(command).is_err() {
            log::debug!(
                target: "zz::agent",
                "dropping a command for pane {}: its runtime is gone",
                self.pane
            );
            return false;
        }
        true
    }

    fn send_control(&self, control: RuntimeControl) -> bool {
        self.controls.try_send(control).is_ok()
    }

    fn fail_control(&mut self) {
        let message = "agent runtime control queue is unavailable".to_owned();
        {
            let mut state = self.state.lock();
            state.phase = AgentConnectionPhase::Failed;
            state.error = Some(message.clone());
            settle_turn(&mut state);
        }
        self.active_turn = None;
        self.settle_active_turn(Err(AgentTurnFailure::Failed(message.clone())));
        self.settle_session_change(Err(message.clone()));
        self.emit(AgentStreamPayload::PaneFailed { message });
        self.close.close();
    }

    fn publish_queue_depth(&self) {
        let state = {
            let mut state = self.state.lock();
            state.queued_prompts = self.queue.len();
            state.own_prompts = own_prompts(&self.queue);
            state.waiting_prompts = waiting_prompts(&self.queue);
            state.clone()
        };
        (self.sink)(self.pane, self.generation, state, None);
    }

    fn emit(&mut self, payload: AgentStreamPayload) {
        let state = {
            let mut state = self.state.lock();
            self.seq = self.seq.saturating_add(1);
            state.last_seq = self.seq;
            state.clone()
        };
        (self.sink)(
            self.pane,
            self.generation,
            state,
            Some(AgentStreamItem {
                seq: self.seq,
                payload,
            }),
        );
    }
}

fn append_reply_tail(buffer: &mut String, text: &str, cap: usize) -> bool {
    buffer.push_str(text);
    if buffer.len() <= cap {
        return false;
    }
    let mut start = buffer.len() - cap;
    while !buffer.is_char_boundary(start) {
        start += 1;
    }
    buffer.drain(..start);
    true
}

fn settle_turn(state: &mut AgentPaneState) {
    state
        .pending_permissions
        .retain(AgentPendingPermission::outlives_turn);
}

#[cfg(test)]
mod pane_tests;
