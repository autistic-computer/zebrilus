#[cfg(unix)]
pub mod control;
pub mod rollout;
pub mod translate;

mod approvals;

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use agent_client_protocol::v1::schema::{MessageId, ToolKind};
use async_channel::Sender;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use parking_lot::Mutex;

use crate::chat_bridge::{
    Shadow, chat_answer, chat_answers, journal_marks, turn_mark, without_images,
};
use serde_json::{Map, Value, json};
use zz_protocol::agent_stream::agent_verb;
use zz_protocol::chat::ChatEvent;
use zz_protocol::{
    AgentAutoApprove, AgentProvider, AgentQuestionAnswer, AgentTaskWire, MAX_AGENT_TASKS,
};

use crate::{
    accounts,
    child::{
        ChildEvent, Input, Process, Rewind, Title, cancelled, clean_title, fit_update, next_input,
        option, prompt_outcome, random_u64, spawn_thread, verb_echo,
    },
    environment::{AgentWorkspaceEnvironment, agent_path, find_executable},
    host::RuntimeChannels,
    journal::{AgentJournal, JournalEntry},
    runtime::{
        RuntimeCommand, RuntimeControl, StderrTail, prompt_blocks, prompt_echo,
        report_journal_error, tier_approves, validate_payload,
    },
    stream::{
        AgentPrompt, AgentPromptOutcome, AgentQuestion, AgentQuestionOption,
        AgentSessionCapabilities, AgentSessionSummary, AgentStreamPayload,
    },
    turn::{self, Effect, End, Fact, Reject, Turn, TurnMachine, TurnState},
    visualize::Visuals,
    zz_command,
};
use zz_machine::Machine;

use translate::{Asked, Translator, question_reply};

pub use approvals::{
    approval, approval_options, chosen, decision, question_answers, question_options, user_input,
};

const REQUEST_TIMEOUT: Duration = Duration::from_mins(1);
#[cfg(unix)]
const MANAGED_START: Duration = Duration::from_secs(20);
const SESSION_PAGE: u64 = 50;
const MAX_CHILD_THREADS: usize = 16;
const AGENT_NAME: &str = "Codex";
const AGENT_KEY: &str = "codex";
const VERB_HELP: &str = "zz commands: `/btw <question>` (or `/side`) asks Codex in a throwaway copy of the thread, `/steer <text>` redirects the running turn, `/fork` continues in a copy of this conversation, and `/rewind [n]` continues from before your last n prompts (1 by default) without changing files. `/compact` and `/review` run Codex's own compaction and review.";
const SIDE_PREFIX: &str = "Answer this side question briefly from what you already know. Do not run commands, edit files, or call tools.\n\n";
const TITLE_PREFIX: &str = "Name the conversation that opens with the message below. Reply with the name alone on one line: a task title of at most six words that starts with a verb, in the message's language, with no quotes, markdown or closing period. Do not run commands or call tools.\n\nMessage:\n";
const TITLE_MODELS: [&str; 4] = ["luna", "nano", "mini", "small"];
const STOP_GRACE: Duration = Duration::from_secs(5);
const STOP_POLL: Duration = Duration::from_millis(20);
const MODES: [(&str, &str, &str); 3] = [
    (
        "read-only",
        "Read only",
        "Ask before changing files or running commands",
    ),
    (
        "auto",
        "Auto",
        "Edit and run inside the workspace, ask beyond it",
    ),
    ("full-access", "Full access", "Run anything without asking"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl CodexCommand {
    pub fn parse(command: &str) -> Option<Self> {
        let mut env = Vec::new();
        let mut tokens = command.split_whitespace();
        let program = loop {
            let token = tokens.next()?;
            match token.split_once('=') {
                Some((name, value))
                    if !name.is_empty()
                        && name.chars().all(|character| {
                            character.is_ascii_alphanumeric() || character == '_'
                        }) =>
                {
                    env.push((name.to_owned(), value.to_owned()));
                }
                _ => break token,
            }
        };
        let name = Path::new(program).file_name()?.to_str()?;
        let lower = name.to_ascii_lowercase();
        matches!(lower.as_str(), "codex" | "codex.exe" | "codex.cmd").then(|| Self {
            program: program.to_owned(),
            args: tokens
                .map(str::to_owned)
                .filter(|arg| arg != "app-server")
                .collect(),
            env,
        })
    }
}

pub async fn run_codex_runtime(
    command: CodexCommand,
    workspace: AgentWorkspaceEnvironment,
    provider: AgentProvider,
    channels: RuntimeChannels,
) -> Result<(), String> {
    let (child_tx, child_rx) = async_channel::unbounded();
    let mut runtime = Runtime {
        provider,
        command,
        workspace,
        auto_approve: channels.auto_approve,
        permission_ids: channels.permission_ids,
        journal: channels.journal,
        ended: channels.ended,
        events: channels.events,
        child_tx,
        child_rx: child_rx.clone(),
        process: None,
        generation: 0,
        stderr: StderrTail::default(),
        initialized: false,
        ready_sent: false,
        session: None,
        starting: None,
        translator: Translator::default(),
        requests: HashMap::new(),
        next_request: 0,
        permissions: HashMap::new(),
        turns: TurnState::default(),
        chat: Mutex::new(Shadow::default().codex().marking_turns()),
        visuals: Mutex::new(Visuals::default()),
        last_turn: 0,
        deferred_cancels: HashSet::new(),
        steers: Vec::new(),
        settings: Settings::default(),
        notices: 0,
        reply: None,
        sides: HashMap::new(),
        left: HashMap::new(),
        tasks: Vec::new(),
        loading: None,
        instructions: None,
        title: Title::default(),
        titler: None,
    };
    let commands = channels.commands;
    let controls = channels.controls;
    loop {
        match next_input(&commands, &controls, &child_rx, runtime.next_deadline()).await {
            Input::Command(Ok(RuntimeCommand::Shutdown) | Err(_)) => {
                runtime.shutdown().await;
                return Ok(());
            }
            Input::Command(Ok(command)) => runtime.command(command).await?,
            Input::Control(Ok(control)) => runtime.control(control).await?,
            Input::Child(Ok(event)) => runtime.child(event).await?,
            Input::Control(Err(_)) | Input::Child(Err(_)) => {}
            Input::Deadline => runtime.expire().await?,
        }
    }
}

struct Session {
    id: String,
    cwd: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Start {
    Open,
    New,
    Switch,
    Fork,
}

struct Starting {
    start: Start,
    cwd: PathBuf,
    resume: Option<String>,
    before: Option<String>,
    live: bool,
}

enum Outgoing {
    Initialize,
    Instructions,
    Models,
    Thread {
        start: Start,
        cwd: PathBuf,
    },
    History {
        start: Start,
        thread: Value,
        cwd: PathBuf,
    },
    ChildHistory {
        thread: String,
    },
    Turn {
        turn_id: u64,
    },
    Command {
        turn_id: u64,
    },
    Quiet,
    Steer {
        thread: String,
        steer: AgentPrompt,
    },
    SteerTurn,
    Answer {
        text: String,
        steered: bool,
    },
    Terminals,
    Terminate {
        task_id: String,
    },
    Rewind {
        target: Rewind,
    },
    SideFork {
        question: String,
        message_id: String,
        reply: String,
    },
    SideTurn {
        thread: String,
    },
    TitleThread {
        prompt: String,
        effort: Option<String>,
    },
    TitleTurn,
    TitleName {
        thread: String,
        title: String,
    },
    Sessions {
        client: zz_protocol::ClientId,
        cwd: Option<PathBuf>,
        replace: bool,
    },
}

pub enum Reply {
    Command,
    FileChange,
    Legacy,
    Permissions(Value),
    Questions(Vec<AgentQuestion>),
    Async {
        call: String,
        questions: Vec<AgentQuestion>,
    },
}

struct Pending {
    rpc_id: Value,
    reply: Reply,
}

#[derive(Default)]
struct Settings {
    models: Vec<Value>,
    model: Option<String>,
    effort: Option<String>,
    mode: Option<String>,
    model_override: bool,
    effort_override: bool,
    mode_override: bool,
}

impl Settings {
    fn current_model(&self) -> Option<&Value> {
        self.models
            .iter()
            .find(|model| model["model"].as_str() == self.model.as_deref())
            .or_else(|| self.models.iter().find(|model| model["isDefault"] == true))
    }

    fn mode_state(&self) -> Value {
        json!({
            "currentModeId": self.mode.as_deref().unwrap_or("auto"),
            "availableModes": MODES.iter().map(|(id, name, description)| json!({
                "id": id, "name": name, "description": description,
            })).collect::<Vec<_>>(),
        })
    }

    fn config_options(&self) -> Value {
        let mut options = Vec::new();
        if !self.models.is_empty() {
            options.push(json!({
                "id": "model",
                "name": "Model",
                "category": "model",
                "type": "select",
                "currentValue": self.model.as_deref().unwrap_or_default(),
                "options": self.models.iter().filter_map(|model| {
                    let value = model["model"].as_str()?;
                    Some(json!({
                        "value": value,
                        "name": model["displayName"].as_str().unwrap_or(value),
                        "description": model["description"].as_str(),
                    }))
                }).collect::<Vec<_>>(),
            }));
        }
        options.push(json!({
            "id": "mode",
            "name": "Mode",
            "category": "mode",
            "type": "select",
            "currentValue": self.mode.as_deref().unwrap_or("auto"),
            "options": MODES.iter().map(|(id, name, description)| json!({
                "value": id, "name": name, "description": description,
            })).collect::<Vec<_>>(),
        }));
        let efforts = self
            .current_model()
            .and_then(|model| model["supportedReasoningEfforts"].as_array())
            .filter(|efforts| !efforts.is_empty());
        if let Some(efforts) = efforts {
            let current = self
                .effort
                .clone()
                .or_else(|| {
                    self.current_model()
                        .and_then(|model| model["defaultReasoningEffort"].as_str())
                        .map(str::to_owned)
                })
                .unwrap_or_default();
            options.push(json!({
                "id": "effort",
                "name": "Effort",
                "category": "thought_level",
                "type": "select",
                "currentValue": current,
                "options": efforts.iter().filter_map(|effort| {
                    let value = effort["reasoningEffort"].as_str()?;
                    let mut name = value.to_owned();
                    if let Some(first) = name.get_mut(0..1) {
                        first.make_ascii_uppercase();
                    }
                    Some(json!({ "value": value, "name": name, "description": effort["description"].as_str() }))
                }).collect::<Vec<_>>(),
            }));
        }
        Value::Array(options)
    }

    fn adopt(&mut self, response: &Value) {
        if self.model.is_none() || !self.model_override {
            self.model = response["model"].as_str().map(str::to_owned);
        }
        if !self.effort_override {
            self.effort = response["reasoningEffort"].as_str().map(str::to_owned);
        }
        if !self.mode_override {
            self.mode = Some(mode_of(&response["approvalPolicy"], &response["sandbox"]).to_owned());
        }
    }

    fn turn_overrides(&self, params: &mut Value) {
        if self.model_override
            && let Some(model) = &self.model
        {
            params["model"] = Value::from(model.as_str());
        }
        if self.effort_override
            && let Some(effort) = &self.effort
        {
            params["effort"] = Value::from(effort.as_str());
        }
        if self.mode_override {
            let (approval, sandbox) = match self.mode.as_deref() {
                Some("read-only") => (
                    json!("on-request"),
                    json!({ "type": "readOnly", "networkAccess": false }),
                ),
                Some("full-access") => (json!("never"), json!({ "type": "dangerFullAccess" })),
                _ => (
                    json!("on-request"),
                    json!({
                        "type": "workspaceWrite",
                        "writableRoots": [],
                        "networkAccess": false,
                        "excludeTmpdirEnvVar": false,
                        "excludeSlashTmp": false,
                    }),
                ),
            };
            params["approvalPolicy"] = approval;
            params["sandboxPolicy"] = sandbox;
        }
    }
}

fn mode_of(approval: &Value, sandbox: &Value) -> &'static str {
    match (approval.as_str(), sandbox["type"].as_str()) {
        (Some("never"), Some("dangerFullAccess")) => "full-access",
        (_, Some("readOnly")) => "read-only",
        _ => "auto",
    }
}

struct Runtime {
    provider: AgentProvider,
    command: CodexCommand,
    workspace: AgentWorkspaceEnvironment,
    auto_approve: Arc<Mutex<AgentAutoApprove>>,
    permission_ids: Arc<AtomicU64>,
    journal: Option<Arc<AgentJournal>>,
    ended: Arc<AtomicBool>,
    events: Sender<AgentStreamPayload>,
    child_tx: Sender<ChildEvent>,
    child_rx: async_channel::Receiver<ChildEvent>,
    process: Option<Process>,
    generation: u64,
    stderr: StderrTail,
    initialized: bool,
    ready_sent: bool,
    session: Option<Session>,
    starting: Option<Starting>,
    translator: Translator,
    requests: HashMap<u64, (Outgoing, Instant)>,
    next_request: u64,
    permissions: HashMap<u64, Pending>,
    turns: TurnState,
    chat: Mutex<Shadow>,
    visuals: Mutex<Visuals>,
    last_turn: u64,
    deferred_cancels: HashSet<u64>,
    steers: Vec<AgentPrompt>,
    settings: Settings,
    notices: u64,
    reply: Option<String>,
    sides: HashMap<String, Side>,
    left: HashMap<String, String>,
    tasks: Vec<AgentTaskWire>,
    loading: Option<Loading>,
    instructions: Option<String>,
    title: Title,
    titler: Option<Titler>,
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.stop_turn_left_behind();
    }
}

struct Titler {
    target: String,
    helper: Option<String>,
    text: String,
    streamed: bool,
}

struct Loading {
    start: Start,
    thread: Value,
    cwd: PathBuf,
    children: HashMap<String, Option<Vec<Value>>>,
}

struct Side {
    message_id: String,
    reply: String,
    text: String,
    streamed: bool,
}

impl Runtime {
    async fn emit(&self, payload: AgentStreamPayload) -> Result<(), String> {
        let payloads = self.visuals.lock().pass(payload);
        for payload in payloads {
            self.emit_one(payload).await?;
        }
        Ok(())
    }

    async fn emit_one(&self, payload: AgentStreamPayload) -> Result<(), String> {
        let chat = {
            let mut shadow = self.chat.lock();
            shadow.payload(&payload);
            shadow.drain()
        };
        let payload = without_images(payload);
        if let Err(error) = validate_payload(&payload) {
            log::warn!(target: "zz::agent", "dropping an oversized Codex item: {error}");
            return self.send_chat(chat).await;
        }
        self.events
            .send(payload)
            .await
            .map_err(|error| format!("agent stream is unavailable: {error}"))?;
        self.send_chat(chat).await
    }

    async fn send_chat(&self, events: Vec<ChatEvent>) -> Result<(), String> {
        for event in events {
            let payload = AgentStreamPayload::Chat { event };
            if validate_payload(&payload).is_err() {
                continue;
            }
            self.events
                .send(payload)
                .await
                .map_err(|error| format!("agent stream is unavailable: {error}"))?;
        }
        Ok(())
    }

    async fn flush_chat(&self) -> Result<(), String> {
        let (marks, events) = {
            let mut shadow = self.chat.lock();
            (shadow.take_marks(), shadow.drain())
        };
        journal_marks(
            self.journal.as_deref(),
            self.provider,
            self.session.as_ref().map(|session| session.id.as_str()),
            &marks,
        );
        self.send_chat(events).await
    }

    async fn update(&self, update: Value, record: bool) -> Result<(), String> {
        let update = fit_update(update);
        if record
            && let (Some(journal), Some(session)) = (self.journal.as_deref(), &self.session)
            && let Err(error) = journal.append_for(self.provider, &session.id, &update)
        {
            report_journal_error(&session.id, &error.to_string());
        }
        self.emit(AgentStreamPayload::Update { update }).await
    }

    async fn notice(&mut self, text: &str) -> Result<(), String> {
        let reply = self.reply.clone();
        self.notice_to(text, reply.as_deref()).await
    }

    async fn notice_to(&mut self, text: &str, reply: Option<&str>) -> Result<(), String> {
        let mut zz = json!({ "notice": true });
        if let Some(reply) = reply {
            zz["reply"] = json!(reply);
        }
        self.notices += 1;
        let id = format!(
            "zz-notice-{}-{:08x}",
            self.notices,
            random_u64() & 0xffff_ffff
        );
        self.update(
            json!({
                "sessionUpdate": "agent_message_chunk",
                "messageId": id,
                "content": { "type": "text", "text": text },
                "_meta": { "zz": zz },
            }),
            true,
        )
        .await
    }

    fn send(&self, message: &Value) {
        if let Some(process) = &self.process {
            process.send(message);
        }
    }

    fn request(&mut self, method: &str, params: &Value, outgoing: Outgoing) {
        self.next_request += 1;
        let id = self.next_request;
        self.send(&json!({ "method": method, "id": id, "params": params }));
        self.requests
            .insert(id, (outgoing, Instant::now() + REQUEST_TIMEOUT));
    }

    fn reply(&self, rpc_id: &Value, result: &Value) {
        self.send(&json!({ "id": rpc_id, "result": result }));
    }

    fn reply_error(&self, rpc_id: &Value, message: &str) {
        self.send(&json!({ "id": rpc_id, "error": { "code": -32601, "message": message } }));
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.requests
            .values()
            .map(|(_, deadline)| *deadline)
            .chain(TurnMachine::wake_at(&self.turns))
            .min()
    }

    async fn apply(&mut self, fact: &Fact) -> Result<Option<Reject>, String> {
        let idle = self.turns.turn().is_none();
        let now = Instant::now();
        let step = TurnMachine::step(&mut self.turns, fact, &turn::Ctx { now });
        self.chat.lock().turn(
            fact,
            self.session.as_ref().map(|session| session.id.as_str()),
            now,
        );
        self.flush_chat().await?;
        if let Some(reject) = &step.rejected {
            log::debug!(target: "zz::agent", "Codex turn kept {fact:?} out: {reject:?}");
        }
        if idle && self.turns.turn().is_some_and(|turn| turn.owner.is_none()) {
            self.emit(AgentStreamPayload::Activity { busy: true })
                .await?;
        }
        let gone = matches!(fact, Fact::Exited { .. } | Fact::SessionClosed);
        let mut drained = false;
        for effect in step.effects {
            match effect {
                Effect::Interrupt => {
                    let codex = self
                        .turns
                        .turn()
                        .and_then(Turn::active_command)
                        .map(str::to_owned);
                    if let (Some(thread), Some(codex)) = (self.thread_id(), codex) {
                        self.request(
                            "turn/interrupt",
                            &json!({ "threadId": thread, "turnId": codex }),
                            Outgoing::Quiet,
                        );
                    }
                }
                Effect::Settle {
                    turn: Some(turn_id),
                    outcome,
                } => self.finish(turn_id, prompt_outcome(outcome)).await?,
                Effect::Settle { turn: None, .. } if !gone => {
                    self.emit(AgentStreamPayload::Activity { busy: false })
                        .await?;
                }
                Effect::Drain => drained = true,
                Effect::Settle { turn: None, .. } | Effect::Dismiss { .. } | Effect::Reclaim => {}
            }
        }
        self.resolve_steers(drained).await?;
        Ok(step.rejected)
    }

    async fn resolve_steers(&mut self, drained: bool) -> Result<(), String> {
        if self.steers.is_empty() {
            return Ok(());
        }
        match self.turns.phase {
            turn::Phase::Running(_) => {
                self.send_steers();
                Ok(())
            }
            turn::Phase::Cancelling { .. } => Ok(()),
            turn::Phase::Idle if drained => {
                for steer in std::mem::take(&mut self.steers) {
                    self.steer_as_prompt(steer).await?;
                }
                Ok(())
            }
            turn::Phase::Idle | turn::Phase::Offline | turn::Phase::Exited => {
                let steers = std::mem::take(&mut self.steers);
                self.give_back(steers).await
            }
        }
    }

    fn send_steers(&mut self) {
        let turn::Phase::Running(turn) = &self.turns.phase else {
            return;
        };
        let (Some(command), Some(thread)) =
            (turn.active_command().map(str::to_owned), self.thread_id())
        else {
            return;
        };
        for steer in std::mem::take(&mut self.steers) {
            self.request(
                "turn/steer",
                &json!({
                    "threadId": thread,
                    "expectedTurnId": command,
                    "input": [{ "type": "text", "text": steer.text, "text_elements": [] }],
                }),
                Outgoing::Steer {
                    thread: thread.clone(),
                    steer,
                },
            );
        }
    }

    async fn steer_as_prompt(&mut self, steer: AgentPrompt) -> Result<(), String> {
        let live = !matches!(self.turns.phase, turn::Phase::Offline | turn::Phase::Exited);
        let Some(thread) = self.thread_id().filter(|_| live) else {
            return self.give_back(vec![steer]).await;
        };
        let echo = format!("zz-steer-{:016x}", random_u64());
        for update in prompt_echo(&prompt_blocks(steer.clone()), &MessageId::new(echo.clone()))? {
            self.update(update, true).await?;
        }
        self.translator.expect_prompt(&steer.text);
        let mut params = json!({
            "threadId": thread,
            "input": turn_input(&steer, None),
            "clientUserMessageId": echo,
        });
        self.settings.turn_overrides(&mut params);
        self.request("turn/start", &params, Outgoing::SteerTurn);
        Ok(())
    }

    async fn missed_steer(
        &mut self,
        thread: String,
        steer: AgentPrompt,
        error: &str,
    ) -> Result<(), String> {
        log::debug!(target: "zz::agent", "Codex did not take a steer: {error}");
        if self.thread_id().as_deref() != Some(thread.as_str()) {
            return self.give_back(vec![steer]).await;
        }
        if matches!(self.turns.phase, turn::Phase::Cancelling { .. }) {
            self.steers.push(steer);
            return Ok(());
        }
        self.steer_as_prompt(steer).await
    }

    async fn give_back(&self, prompts: Vec<AgentPrompt>) -> Result<(), String> {
        if prompts.is_empty() {
            return Ok(());
        }
        self.emit(AgentStreamPayload::PromptsReclaimed { prompts })
            .await
    }

    fn owns(&self, turn_id: u64) -> bool {
        self.turns
            .turn()
            .is_some_and(|turn| turn.owner == Some(turn_id))
    }

    fn thread_id(&self) -> Option<String> {
        self.session.as_ref().map(|session| session.id.clone())
    }

    fn cwd(&self) -> PathBuf {
        self.session
            .as_ref()
            .map(|session| session.cwd.clone())
            .unwrap_or_default()
    }

    async fn command(&mut self, command: RuntimeCommand) -> Result<(), String> {
        match command {
            RuntimeCommand::Open {
                cwd,
                resume_session,
            } => self.begin(cwd, Start::Open, resume_session).await,
            RuntimeCommand::NewSession { cwd } => self.begin(cwd, Start::New, None).await,
            RuntimeCommand::SwitchSession { session } => {
                self.begin(session.cwd, Start::Switch, Some(session.session_id))
                    .await
            }
            RuntimeCommand::ListSessions {
                client,
                cwd,
                cursor,
                replace,
            } => {
                let mut params = json!({
                    "limit": SESSION_PAGE,
                    "archived": false,
                    "useStateDbOnly": true,
                });
                if let Some(cwd) = &cwd {
                    params["cwd"] = Value::from(cwd.display().to_string());
                }
                if let Some(cursor) = cursor {
                    params["cursor"] = Value::from(cursor);
                }
                self.request(
                    "thread/list",
                    &params,
                    Outgoing::Sessions {
                        client,
                        cwd,
                        replace,
                    },
                );
                Ok(())
            }
            RuntimeCommand::DeleteSession { client, .. } => {
                self.emit(AgentStreamPayload::SessionDeleteFailed {
                    client,
                    message: "Codex threads are not deleted from zz".to_owned(),
                })
                .await
            }
            RuntimeCommand::Prompt {
                turn_id,
                prompt,
                note,
            } => self.prompt(turn_id, prompt, note.as_deref()).await,
            RuntimeCommand::Verb { prompt } => self.verb(prompt).await,
            RuntimeCommand::Authenticate { .. } => {
                self.emit(AgentStreamPayload::AuthenticationFailed {
                    message: "Codex is signed out.".to_owned(),
                })
                .await
            }
            RuntimeCommand::SetConfigOption { option_id, value } => {
                match option_id.as_str() {
                    "model" => {
                        self.settings.model = Some(value.clone());
                        self.settings.model_override = true;
                        let supported = self
                            .settings
                            .current_model()
                            .and_then(|model| model["supportedReasoningEfforts"].as_array())
                            .is_some_and(|efforts| {
                                efforts.iter().any(|effort| {
                                    effort["reasoningEffort"].as_str()
                                        == self.settings.effort.as_deref()
                                })
                            });
                        if !supported {
                            self.settings.effort = None;
                            self.settings.effort_override = false;
                        }
                    }
                    "effort" => {
                        self.settings.effort = Some(value.clone());
                        self.settings.effort_override = true;
                    }
                    "mode" => {
                        self.settings.mode = Some(value.clone());
                        self.settings.mode_override = true;
                    }
                    _ => {
                        return self
                            .emit(AgentStreamPayload::SettingFailed {
                                option_id,
                                message: "Codex has no such setting".to_owned(),
                            })
                            .await;
                    }
                }
                self.emit(AgentStreamPayload::ConfigOptionsChanged {
                    option_id,
                    value,
                    config_options: self.settings.config_options(),
                })
                .await?;
                self.publish_settings().await
            }
            RuntimeCommand::SetMode { mode_id } => {
                self.settings.mode = Some(mode_id.clone());
                self.settings.mode_override = true;
                self.emit(AgentStreamPayload::ModeChanged { mode_id })
                    .await?;
                self.publish_settings().await
            }
            RuntimeCommand::StopTask { task_id } => {
                if let Some(thread) = self.thread_id() {
                    self.request(
                        "thread/backgroundTerminals/terminate",
                        &json!({ "threadId": thread, "processId": task_id }),
                        Outgoing::Terminate { task_id },
                    );
                }
                Ok(())
            }
            RuntimeCommand::Shutdown | RuntimeCommand::Ask { .. } => Ok(()),
        }
    }

    async fn control(&mut self, control: RuntimeControl) -> Result<(), String> {
        match control {
            RuntimeControl::Cancel { turn_id, .. } => {
                let owner = self.turns.turn().map(|turn| turn.owner.unwrap_or(0));
                if owner == Some(turn_id) {
                    self.apply(&Fact::Cancel).await?;
                    self.cancel_blocking().await
                } else if turn_id == 0 && owner.is_none() {
                    self.cancel_blocking().await
                } else {
                    if turn_id > self.last_turn {
                        self.deferred_cancels.insert(turn_id);
                    }
                    Ok(())
                }
            }
            RuntimeControl::RespondPermission {
                request_id,
                option_id,
            } => self.answer(request_id, option_id.as_deref()).await,
            RuntimeControl::AnswerQuestion {
                request_id,
                answers,
            } => self.answer_questions(request_id, &answers).await,
        }
    }

    async fn begin(
        &mut self,
        cwd: PathBuf,
        start: Start,
        resume: Option<String>,
    ) -> Result<(), String> {
        self.begin_with(start, cwd, resume, None).await
    }

    async fn begin_with(
        &mut self,
        start: Start,
        cwd: PathBuf,
        resume: Option<String>,
        before: Option<String>,
    ) -> Result<(), String> {
        if start == Start::Fork && resume.is_none() {
            return self
                .notice("Nothing to fork yet: send a prompt first.")
                .await;
        }
        let live = !matches!(self.turns.phase, turn::Phase::Offline | turn::Phase::Exited);
        self.cancel_permissions().await?;
        self.apply(&Fact::SessionClosed).await?;
        if self.process.is_none()
            && let Err(message) = self.spawn()
        {
            return self.fail_start(start, message).await;
        }
        self.starting = Some(Starting {
            start,
            cwd,
            resume,
            before,
            live,
        });
        if self.initialized {
            self.open_thread();
        }
        Ok(())
    }

    fn spawn(&mut self) -> Result<(), String> {
        let program = find_executable(&self.command.program).ok_or_else(|| {
            format!(
                "Codex is not installed: `{}` was not found on PATH. Install it from https://developers.openai.com/codex, or point agent-command at it.",
                self.command.program
            )
        })?;
        if !crate::sandboxed()
            && let Some(home) = codex_home()
            && let Err(error) = ensure_codex_rule(&home)
        {
            log::warn!(target: "zz::agent", "could not write the zz rule under {}: {error}", home.display());
        }
        self.generation += 1;
        #[cfg(unix)]
        let managed = managed_daemon(&program, &self.command);
        #[cfg(not(unix))]
        let managed: Option<PathBuf> = None;
        if let Some(socket) = managed {
            self.process = Some(Process::connect(
                &socket,
                self.provider,
                AGENT_NAME,
                self.generation,
                &self.child_tx,
            )?);
        } else {
            let mut command = Command::new(program);
            command.arg("app-server").args(&self.command.args);
            if let Some(path) = self.workspace.path(agent_path().as_deref()) {
                command.env("PATH", path);
            }
            for (name, value) in self.workspace.entries() {
                command.env(name, value);
            }
            if self.workspace.managed_login {
                accounts::strip_logins(&mut command, AgentProvider::Codex);
            }
            if self.workspace.pane.is_some() {
                command.env(
                    zz_daemon_client::agent_context::WORKSPACE_CONTEXT_SENT_ENVIRONMENT_VARIABLE,
                    "1",
                );
            }
            for (name, value) in &self.command.env {
                command.env(name, value);
            }
            self.process = Some(Process::launch(
                &mut command,
                self.provider,
                AGENT_NAME,
                self.generation,
                &self.child_tx,
                &self.stderr,
            )?);
        }
        self.initialized = false;
        self.requests.clear();
        self.request(
            "initialize",
            &json!({
                "clientInfo": { "name": "zz", "title": "zz", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": { "experimentalApi": true, "requestAttestation": false },
            }),
            Outgoing::Initialize,
        );
        Ok(())
    }

    fn open_thread(&mut self) {
        let Some(starting) = &self.starting else {
            return;
        };
        let cwd = starting.cwd.clone();
        let start = starting.start;
        let cwd_text = cwd.display().to_string();
        let before = starting.before.clone();
        match (start, starting.resume.clone()) {
            (Start::Fork, Some(thread)) => {
                let mut params = json!({
                    "threadId": thread,
                    "cwd": cwd_text,
                    "config": self.thread_config(),
                });
                if let Some(before) = before {
                    params["beforeTurnId"] = Value::from(before);
                }
                self.request("thread/fork", &params, Outgoing::Thread { start, cwd });
            }
            (_, Some(thread)) => self.request(
                "thread/resume",
                &json!({ "threadId": thread, "cwd": cwd_text, "config": self.thread_config() }),
                Outgoing::Thread { start, cwd },
            ),
            (_, None) => {
                let params = self.start_params(&cwd_text);
                self.request("thread/start", &params, Outgoing::Thread { start, cwd });
            }
        }
    }

    fn thread_config(&self) -> Value {
        let mut config = Map::new();
        let mut set = |name: &str, value: &str| {
            config.insert(
                format!("shell_environment_policy.set.{name}"),
                Value::from(value),
            );
        };
        for (name, value) in self.workspace.entries() {
            set(name, value);
        }
        if self.workspace.pane.is_some() {
            set(
                zz_daemon_client::agent_context::WORKSPACE_CONTEXT_SENT_ENVIRONMENT_VARIABLE,
                "1",
            );
        }
        if let Some(path) = self.workspace.path(agent_path().as_deref()) {
            set("PATH", &path);
        }
        Value::Object(config)
    }

    fn start_params(&self, cwd: &str) -> Value {
        let mut params = json!({ "cwd": cwd, "config": self.thread_config() });
        if let Some(instructions) = &self.instructions {
            params["developerInstructions"] = Value::from(instructions.as_str());
        }
        params
    }

    fn read_instructions(&mut self) {
        let Some(pane) = self.workspace.pane.as_deref() else {
            self.initialized = true;
            self.open_thread();
            return;
        };
        let session = self.workspace.session.as_deref().unwrap_or_default();
        self.instructions = Some(zz_daemon_client::agent_context::workspace_context(
            pane, session, true,
        ));
        let params = self.starting.as_ref().map_or_else(
            || json!({}),
            |starting| json!({ "cwd": starting.cwd.display().to_string() }),
        );
        self.request("config/read", &params, Outgoing::Instructions);
    }

    async fn fail_start(&mut self, start: Start, message: String) -> Result<(), String> {
        let live = self.starting.take().is_some_and(|starting| starting.live);
        match start {
            Start::Open => {
                self.process = None;
                self.emit(AgentStreamPayload::PaneFailed { message }).await
            }
            Start::New | Start::Switch | Start::Fork => {
                if live {
                    self.apply(&Fact::SessionReady).await?;
                }
                self.emit(AgentStreamPayload::SessionSwitchFailed { message })
                    .await
            }
        }
    }

    async fn ready(&mut self) -> Result<(), String> {
        if self.ready_sent {
            return Ok(());
        }
        self.ready_sent = true;
        self.emit(AgentStreamPayload::Ready {
            agent_name: AGENT_NAME.to_owned(),
            agent_key: AGENT_KEY.to_owned(),
            auth_methods: Vec::new(),
            capabilities: AgentSessionCapabilities {
                load: true,
                list: true,
                close: false,
                delete: false,
                additional_directories: false,
                images: true,
                verbs: true,
                watch: false,
            },
        })
        .await
    }

    async fn threaded(
        &mut self,
        start: Start,
        cwd: PathBuf,
        response: Value,
    ) -> Result<(), String> {
        let Some(thread) = response["thread"]["id"].as_str().map(str::to_owned) else {
            return self
                .fail_start(start, "Codex returned no thread".to_owned())
                .await;
        };
        self.settings.adopt(&response);
        if start == Start::Open
            && self
                .starting
                .as_ref()
                .and_then(|starting| starting.resume.as_deref())
                == Some(thread.as_str())
            || matches!(start, Start::Switch | Start::Fork)
        {
            self.request(
                "thread/read",
                &json!({ "threadId": thread, "includeTurns": true }),
                Outgoing::History {
                    start,
                    thread: response["thread"].clone(),
                    cwd,
                },
            );
            return Ok(());
        }
        self.settle(
            start,
            thread,
            cwd,
            Vec::new(),
            Vec::new(),
            &response["thread"],
        )
        .await
    }

    async fn settle(
        &mut self,
        start: Start,
        thread: String,
        cwd: PathBuf,
        history: Vec<(Value, bool)>,
        questions: Vec<Asked>,
        opened: &Value,
    ) -> Result<(), String> {
        let (source, before) = self
            .starting
            .take()
            .map_or((None, None), |starting| (starting.resume, starting.before));
        let resumed = start != Start::Fork && source.is_some();
        self.ready().await?;
        if let Some(previous) = self.thread_id().filter(|previous| *previous != thread) {
            let children = self.translator.agent_threads();
            for left in std::iter::once(previous.clone()).chain(children) {
                self.request(
                    "thread/unsubscribe",
                    &json!({ "threadId": left }),
                    Outgoing::Quiet,
                );
                self.left.insert(left, previous.clone());
            }
        }
        self.left
            .retain(|left, chat| *left != thread && *chat != thread);
        self.translator = Translator::new(cwd.clone());
        self.translator.reopen(questions);
        self.tasks.clear();
        self.session = Some(Session {
            id: thread.clone(),
            cwd: cwd.clone(),
        });
        let restoring = match start {
            Start::Open => !history.is_empty(),
            Start::New | Start::Switch | Start::Fork => true,
        };
        self.emit(AgentStreamPayload::SessionReset { restoring })
            .await?;
        for (update, record) in history {
            self.update(update, record).await?;
        }
        self.open_questions().await?;
        let modes = Some(self.settings.mode_state());
        let config_options = Some(self.settings.config_options());
        self.apply(&Fact::SessionReady).await?;
        match start {
            Start::Open => {
                self.emit(AgentStreamPayload::SessionReady {
                    session_id: thread,
                    modes,
                    config_options,
                })
                .await?;
            }
            Start::New | Start::Switch | Start::Fork => {
                self.emit(AgentStreamPayload::SessionSwitched {
                    session_id: thread,
                    cwd,
                    modes,
                    config_options,
                    replay: Vec::new(),
                })
                .await?;
            }
        }
        if let Some(command) = running_turn(opened) {
            self.apply(&Fact::Started {
                command: command.to_owned(),
            })
            .await?;
        }
        self.update(
            json!({
                "sessionUpdate": "available_commands_update",
                "availableCommands": [
                    { "name": "compact", "description": "Summarize the conversation to free context" },
                    { "name": "review", "description": "Review the uncommitted changes" },
                ],
            }),
            true,
        )
        .await?;
        let title = opened["name"].as_str().and_then(clean_title).or_else(|| {
            (start == Start::Fork)
                .then(|| self.title.current())
                .flatten()
        });
        if self.title.start(title.clone()) {
            self.emit(AgentStreamPayload::TitleChanged { title })
                .await?;
        }
        if resumed && let Some(thread) = self.thread_id() {
            self.request(
                "thread/backgroundTerminals/list",
                &json!({ "threadId": thread }),
                Outgoing::Terminals,
            );
        }
        if let (Start::Fork, Some(source)) = (start, source) {
            let text = if before.is_some() {
                format!(
                    "Went back to an earlier prompt. The full conversation stays in thread `{source}`, and files on disk are unchanged."
                )
            } else {
                format!("Forked from thread `{source}`. The original is unchanged.")
            };
            self.notice(&text).await?;
        }
        Ok(())
    }

    async fn rewind(&mut self, target: &Rewind, thread: &Value) -> Result<(), String> {
        if self.turns.turn().is_some() {
            return self.notice("Stop the turn before rewinding.").await;
        }
        let Some(source) = thread["id"].as_str().map(str::to_owned) else {
            return Ok(());
        };
        let mut prompts = thread["turns"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(index, turn)| {
                let ids = turn["items"]
                    .as_array()?
                    .iter()
                    .filter(|item| item["type"] == "userMessage")
                    .flat_map(|item| [item["id"].as_str(), item["clientId"].as_str()])
                    .flatten()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if ids.is_empty() {
                    return None;
                }
                Some((index, turn["id"].as_str()?.to_owned(), ids))
            })
            .collect::<Vec<_>>();
        let index = match target.pick(prompts.len(), |id| {
            prompts
                .iter()
                .position(|(_, _, ids)| ids.iter().any(|known| known == id))
        }) {
            Ok(index) => index,
            Err(text) => return self.notice(&text).await,
        };
        let cwd = self.cwd();
        match prompts.swap_remove(index) {
            (0, _, _) => self.begin(cwd, Start::New, None).await,
            (_, before, _) => {
                self.begin_with(Start::Fork, cwd, Some(source), Some(before))
                    .await
            }
        }
    }

    async fn set_tasks(&mut self, tasks: Vec<AgentTaskWire>) -> Result<(), String> {
        if tasks == self.tasks {
            return Ok(());
        }
        self.tasks.clone_from(&tasks);
        self.emit(AgentStreamPayload::TasksChanged { tasks }).await
    }

    async fn load(&mut self) -> Result<(), String> {
        if self
            .loading
            .as_ref()
            .is_none_or(|loading| loading.children.values().any(Option::is_none))
        {
            return Ok(());
        }
        let Some(loading) = self.loading.take() else {
            return Ok(());
        };
        let children = loading
            .children
            .into_iter()
            .map(|(thread, items)| (thread, items.unwrap_or_default()))
            .collect();
        let id = loading.thread["id"].as_str().unwrap_or_default().to_owned();
        let previous = self.session.take();
        self.session = Some(Session {
            id: id.clone(),
            cwd: loading.cwd.clone(),
        });
        let (history, questions) = self.history(&loading.thread, &children);
        self.session = previous;
        self.settle(
            loading.start,
            id,
            loading.cwd,
            history,
            questions,
            &loading.thread,
        )
        .await
    }

    fn history(
        &mut self,
        thread: &Value,
        children: &HashMap<String, Vec<Value>>,
    ) -> (Vec<(Value, bool)>, Vec<Asked>) {
        let id = thread["id"].as_str().unwrap_or_default().to_owned();
        let mut translator = Translator::new(self.cwd());
        let updates = thread["turns"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|turn| turn_history(&mut translator, turn, children))
            .collect::<Vec<_>>();
        if updates.is_empty()
            && let Some(journal) = self.journal.as_deref()
        {
            let replayed = match journal.replay_for(self.provider, &id) {
                Ok(records) => records
                    .into_iter()
                    .map(|(_, JournalEntry::Update(update))| (update, false))
                    .collect(),
                Err(error) => {
                    report_journal_error(&id, &error.to_string());
                    Vec::new()
                }
            };
            return (replayed, Vec::new());
        }
        if let Some(journal) = self.journal.as_deref()
            && let Err(error) = journal.remove_for(self.provider, &id)
        {
            report_journal_error(&id, &error.to_string());
        }
        (
            updates.into_iter().map(|update| (update, true)).collect(),
            translator.open_questions(),
        )
    }

    async fn prompt(
        &mut self,
        turn_id: u64,
        prompt: AgentPrompt,
        note: Option<&str>,
    ) -> Result<(), String> {
        self.last_turn = self.last_turn.max(turn_id);
        if self.deferred_cancels.remove(&turn_id) {
            self.emit(AgentStreamPayload::PromptAccepted { turn_id })
                .await?;
            return self.finish(turn_id, cancelled()).await;
        }
        let Some(thread) = self.thread_id() else {
            return self
                .emit(AgentStreamPayload::PaneFailed {
                    message: "agent session is not ready".to_owned(),
                })
                .await;
        };
        let echo = format!("zz-prompt-{turn_id}-{:016x}", random_u64());
        let text = prompt.text.clone();
        let input = turn_input(&prompt, note);
        let fact = Fact::Prompt {
            turn: turn_id,
            command: (text.trim() == "/compact").then(|| compact_command(turn_id)),
        };
        if let Some(reject) = self.apply(&fact).await? {
            self.emit(AgentStreamPayload::PromptAccepted { turn_id })
                .await?;
            let message = if reject == Reject::Offline {
                "agent session is not ready"
            } else {
                "a turn is already running"
            };
            return self
                .finish(
                    turn_id,
                    AgentPromptOutcome::Failed {
                        message: message.to_owned(),
                    },
                )
                .await;
        }
        for update in prompt_echo(&prompt_blocks(prompt), &MessageId::new(echo.clone()))? {
            self.update(update, true).await?;
        }
        self.emit(AgentStreamPayload::PromptAccepted { turn_id })
            .await?;
        match text.trim() {
            "/compact" => {
                self.request(
                    "thread/compact/start",
                    &json!({ "threadId": thread }),
                    Outgoing::Command { turn_id },
                );
            }
            "/review" => {
                self.request(
                    "review/start",
                    &json!({
                        "threadId": thread,
                        "target": { "type": "uncommittedChanges" },
                        "delivery": "inline",
                    }),
                    Outgoing::Turn { turn_id },
                );
            }
            _ => {
                self.translator.expect_prompt(&text);
                let mut params = json!({
                    "threadId": thread,
                    "input": input,
                    "clientUserMessageId": echo,
                });
                self.settings.turn_overrides(&mut params);
                self.request("turn/start", &params, Outgoing::Turn { turn_id });
                self.ask_title(thread, &text);
            }
        }
        Ok(())
    }

    fn ask_title(&mut self, thread: String, prompt: &str) {
        if self.titler.is_some() {
            return;
        }
        let Some(prompt) = self.title.ask(prompt) else {
            return;
        };
        let model = self
            .settings
            .models
            .iter()
            .find(|model| {
                model["model"]
                    .as_str()
                    .is_some_and(|id| TITLE_MODELS.iter().any(|mark| id.contains(mark)))
            })
            .or_else(|| self.settings.current_model());
        let low = model.is_some_and(|model| {
            model["supportedReasoningEfforts"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|effort| effort["reasoningEffort"] == "low")
        });
        let mut params = json!({
            "cwd": self.cwd().display().to_string(),
            "ephemeral": true,
            "approvalPolicy": "never",
            "sandbox": "read-only",
        });
        if let Some(id) = model.and_then(|model| model["model"].as_str()) {
            params["model"] = Value::from(id);
        }
        self.titler = Some(Titler {
            target: thread,
            helper: None,
            text: String::new(),
            streamed: false,
        });
        self.request(
            "thread/start",
            &params,
            Outgoing::TitleThread {
                prompt,
                effort: low.then(|| "low".to_owned()),
            },
        );
    }

    fn is_title_helper(&self, thread: &str) -> bool {
        self.titler
            .as_ref()
            .and_then(|titler| titler.helper.as_deref())
            == Some(thread)
    }

    fn title_notification(&mut self, method: &str, params: &Value) {
        let Some(titler) = self.titler.as_mut() else {
            return;
        };
        match method {
            "item/agentMessage/delta" => {
                if let Some(delta) = params["delta"].as_str() {
                    titler.text.push_str(delta);
                    titler.streamed = true;
                }
            }
            "item/completed" if params["item"]["type"] == "agentMessage" && !titler.streamed => {
                if let Some(text) = params["item"]["text"].as_str() {
                    titler.text.push_str(text);
                }
            }
            "turn/completed" => {
                let text = std::mem::take(&mut titler.text);
                self.finish_title(&text);
            }
            _ => {}
        }
    }

    fn finish_title(&mut self, text: &str) {
        let Some(titler) = self.titler.take() else {
            return;
        };
        if let Some(helper) = titler.helper {
            self.request(
                "thread/unsubscribe",
                &json!({ "threadId": helper }),
                Outgoing::Quiet,
            );
        }
        if let Some(title) = generated_title(text)
            && self.thread_id().as_deref() == Some(titler.target.as_str())
        {
            self.request(
                "thread/name/set",
                &json!({ "threadId": titler.target, "name": title }),
                Outgoing::TitleName {
                    thread: titler.target,
                    title,
                },
            );
        }
    }

    async fn verb(&mut self, prompt: AgentPrompt) -> Result<(), String> {
        let text = prompt.text.as_str();
        let (name, rest) = agent_verb(text).unwrap_or(("", ""));
        let mut steer =
            (name == "steer" && !rest.is_empty() && self.session.is_some()).then(|| AgentPrompt {
                text: rest.to_owned(),
                ..prompt.clone()
            });
        if let Some(steer) = steer.take_if(|_| self.turns.turn().is_none()) {
            return self.steer_as_prompt(steer).await;
        }
        self.notices += 1;
        let echo = format!(
            "zz-command-{}-{:08x}",
            self.notices,
            random_u64() & 0xffff_ffff
        );
        let shown = verb_echo(name, rest, text);
        self.update(
            json!({
                "sessionUpdate": "user_message_chunk",
                "messageId": echo,
                "content": { "type": "text", "text": shown },
            }),
            true,
        )
        .await?;
        if steer.is_some() && shown == rest {
            self.translator.expect_prompt(rest);
        }
        self.reply = Some(echo);
        let answered = match steer {
            Some(steer) => {
                self.steers.push(steer);
                self.send_steers();
                Ok(())
            }
            None => self.answer_verb(name, rest).await,
        };
        self.reply = None;
        answered
    }

    async fn answer_verb(&mut self, name: &str, rest: &str) -> Result<(), String> {
        let Some(thread) = self.thread_id() else {
            return self.notice("Codex is not running.").await;
        };
        match name {
            "fork" => {
                if self.turns.turn().is_some() {
                    return self.notice("Stop the turn before forking.").await;
                }
                let cwd = self.cwd();
                self.begin(cwd, Start::Fork, Some(thread)).await
            }
            "rewind" => {
                let Some(target) = Rewind::parse(rest) else {
                    return self.notice(VERB_HELP).await;
                };
                if self.turns.turn().is_some() {
                    return self.notice("Stop the turn before rewinding.").await;
                }
                self.request(
                    "thread/read",
                    &json!({ "threadId": thread, "includeTurns": true }),
                    Outgoing::Rewind { target },
                );
                Ok(())
            }
            "btw" | "side" if !rest.is_empty() => {
                self.notices += 1;
                let message_id = format!(
                    "zz-side-{}-{:08x}",
                    self.notices,
                    random_u64() & 0xffff_ffff
                );
                self.request(
                    "thread/fork",
                    &json!({ "threadId": thread, "ephemeral": true, "excludeTurns": true }),
                    Outgoing::SideFork {
                        question: rest.to_owned(),
                        message_id,
                        reply: self.reply.clone().unwrap_or_default(),
                    },
                );
                Ok(())
            }
            _ => self.notice(VERB_HELP).await,
        }
    }

    async fn publish_settings(&self) -> Result<(), String> {
        self.update(
            json!({
                "sessionUpdate": "current_mode_update",
                "currentModeId": self.settings.mode.as_deref().unwrap_or("auto"),
            }),
            true,
        )
        .await?;
        self.update(
            json!({ "sessionUpdate": "config_option_update", "configOptions": self.settings.config_options() }),
            true,
        )
        .await
    }

    async fn finish(&self, turn_id: u64, outcome: AgentPromptOutcome) -> Result<(), String> {
        self.emit(AgentStreamPayload::PromptFinished { turn_id, outcome })
            .await
    }

    async fn shutdown(&mut self) {
        let _ = self.cancel_permissions().await;
        self.stop_turn_left_behind();
        self.process = None;
    }

    fn stop_turn_left_behind(&mut self) {
        if !self.ended.load(Ordering::Acquire) {
            return;
        }
        let Some(thread) = self.thread_id() else {
            return;
        };
        let running = self
            .turns
            .turn()
            .and_then(Turn::active_command)
            .map(str::to_owned);
        let starting = self.requests.iter().find_map(|(id, (outgoing, _))| {
            matches!(outgoing, Outgoing::Turn { .. } | Outgoing::SteerTurn).then_some(*id)
        });
        if running.is_none() && starting.is_none() {
            return;
        }
        let Some(process) = self.process.take() else {
            return;
        };
        self.next_request += 1;
        let id = self.next_request;
        let lines = self.child_rx.clone();
        let stopper = spawn_thread("zz-agent-stop", move || {
            let interrupt = |turn: &str| {
                log::info!(target: "zz::agent", "interrupting turn {turn} of closed thread {thread}");
                process.send(&json!({
                    "method": "turn/interrupt",
                    "id": id,
                    "params": { "threadId": thread, "turnId": turn },
                }));
            };
            let mut asked = running.is_some();
            if let Some(turn) = &running {
                interrupt(turn);
            }
            let deadline = Instant::now() + STOP_GRACE;
            while Instant::now() < deadline {
                let line = match lines.try_recv() {
                    Ok(ChildEvent::Line(_, line)) => line,
                    Ok(ChildEvent::Closed(_)) | Err(async_channel::TryRecvError::Closed) => return,
                    Err(async_channel::TryRecvError::Empty) => {
                        std::thread::sleep(STOP_POLL);
                        continue;
                    }
                };
                let Ok(frame) = serde_json::from_slice::<Value>(&line) else {
                    continue;
                };
                if asked && frame["id"] == id {
                    return;
                }
                if !asked
                    && Some(&frame["id"]) == starting.map(Value::from).as_ref()
                    && let Some(turn) = frame["result"]["turn"]["id"].as_str()
                {
                    interrupt(turn);
                    asked = true;
                }
            }
            log::warn!(target: "zz::agent", "closed thread {thread} did not answer its interrupt");
        });
        if let Err(error) = stopper {
            log::warn!(target: "zz::agent", "could not stop a closed chat's turn: {error}");
        }
    }

    async fn expire(&mut self) -> Result<(), String> {
        let now = Instant::now();
        if TurnMachine::wake_at(&self.turns).is_some_and(|wake| wake <= now) {
            self.apply(&Fact::Tick).await?;
        }
        let expired = self
            .requests
            .iter()
            .filter(|(_, (_, deadline))| *deadline <= now)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in expired {
            if let Some((outgoing, _)) = self.requests.remove(&id) {
                self.settled(outgoing, Err("Codex did not answer in time".to_owned()))
                    .await?;
            }
        }
        Ok(())
    }

    async fn settled(
        &mut self,
        outgoing: Outgoing,
        result: Result<Value, String>,
    ) -> Result<(), String> {
        match (outgoing, result) {
            (Outgoing::Initialize, Ok(_)) => {
                self.send(&json!({ "method": "initialized" }));
                self.request("model/list", &json!({}), Outgoing::Models);
                self.read_instructions();
                Ok(())
            }
            (Outgoing::Instructions, result) => {
                match result {
                    Ok(response) => {
                        if let (Some(own), Some(context)) = (
                            response["config"]["developer_instructions"]
                                .as_str()
                                .filter(|own| !own.trim().is_empty()),
                            self.instructions.as_mut(),
                        ) {
                            *context = format!("{own}\n\n{context}");
                        }
                    }
                    Err(error) => {
                        log::warn!(target: "zz::agent", "could not read the Codex config, so zz sends no instructions: {error}");
                        self.instructions = None;
                    }
                }
                self.initialized = true;
                self.open_thread();
                Ok(())
            }
            (Outgoing::Initialize, Err(error)) => {
                let start = self
                    .starting
                    .as_ref()
                    .map_or(Start::Open, |starting| starting.start);
                let tail = self.stderr.snapshot();
                let message = tail.map_or_else(
                    || format!("Codex did not start: {error}"),
                    |tail| format!("Codex did not start: {error}\n{tail}"),
                );
                self.process = None;
                self.fail_start(start, message).await
            }
            (Outgoing::Models, Ok(response)) => {
                self.settings.models = response["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|model| model["hidden"] != true)
                    .cloned()
                    .collect();
                if self.session.is_some() {
                    self.publish_settings().await?;
                }
                Ok(())
            }
            (Outgoing::Models | Outgoing::Terminals, Err(_))
            | (Outgoing::Quiet, _)
            | (
                Outgoing::Steer { .. }
                | Outgoing::SteerTurn
                | Outgoing::SideTurn { .. }
                | Outgoing::Answer { .. }
                | Outgoing::TitleTurn,
                Ok(_),
            ) => Ok(()),
            (Outgoing::Steer { thread, steer }, Err(error)) => {
                self.missed_steer(thread, steer, &error).await
            }
            (
                Outgoing::Answer {
                    text,
                    steered: true,
                },
                Err(_),
            ) => {
                self.send_answer(text, None);
                Ok(())
            }
            (Outgoing::Answer { .. }, Err(error)) => {
                self.notice(&format!("Codex did not take the answer: {error}"))
                    .await
            }
            (Outgoing::Thread { start, cwd }, Ok(response)) => {
                self.threaded(start, cwd, response).await
            }
            (Outgoing::Thread { start, cwd }, Err(error)) => {
                if start == Start::Open
                    && let Some(starting) = &mut self.starting
                    && starting.resume.take().is_some()
                {
                    log::warn!(target: "zz::agent", "could not resume the Codex thread: {error}");
                    let params = self.start_params(&cwd.display().to_string());
                    self.request("thread/start", &params, Outgoing::Thread { start, cwd });
                    return Ok(());
                }
                self.fail_start(start, format!("Codex could not open the thread: {error}"))
                    .await
            }
            (Outgoing::History { start, thread, cwd }, result) => {
                let full = result.map_or(thread, |response| response["thread"].clone());
                let mut children = HashMap::new();
                for item in thread_items(&full) {
                    for child in spawned(&item) {
                        if children.len() < MAX_CHILD_THREADS {
                            children.insert(child, None);
                        }
                    }
                }
                for child in children.keys() {
                    self.request(
                        "thread/read",
                        &json!({ "threadId": child, "includeTurns": true }),
                        Outgoing::ChildHistory {
                            thread: child.clone(),
                        },
                    );
                }
                self.loading = Some(Loading {
                    start,
                    thread: full,
                    cwd,
                    children,
                });
                self.load().await
            }
            (Outgoing::ChildHistory { thread }, result) => {
                if let Some(slot) = self
                    .loading
                    .as_mut()
                    .and_then(|loading| loading.children.get_mut(&thread))
                {
                    *slot = Some(
                        result
                            .map(|response| thread_items(&response["thread"]))
                            .unwrap_or_default(),
                    );
                }
                self.load().await
            }
            (Outgoing::Turn { turn_id }, Ok(response)) => {
                if let Some(command) = response["turn"]["id"].as_str() {
                    self.apply(&Fact::Accepted {
                        turn: turn_id,
                        command: command.to_owned(),
                    })
                    .await?;
                }
                Ok(())
            }
            (Outgoing::Turn { turn_id }, Err(error)) => {
                if !self.owns(turn_id) {
                    return Ok(());
                }
                self.apply(&Fact::Ended {
                    command: format!("zz-turn-{turn_id}"),
                    end: End::Failed(error),
                })
                .await
                .map(drop)
            }
            (Outgoing::Command { turn_id }, result) => {
                if !self.owns(turn_id) {
                    return Ok(());
                }
                let end = result.map_or_else(End::Failed, |_| End::Completed);
                self.apply(&Fact::Ended {
                    command: compact_command(turn_id),
                    end,
                })
                .await
                .map(drop)
            }
            (
                Outgoing::SideFork {
                    question,
                    message_id,
                    reply,
                },
                Ok(response),
            ) => {
                let Some(side) = response["thread"]["id"].as_str().map(str::to_owned) else {
                    return self
                        .notice_to("Codex did not open the side thread.", Some(&reply))
                        .await;
                };
                self.sides.insert(
                    side.clone(),
                    Side {
                        message_id,
                        reply,
                        text: String::new(),
                        streamed: false,
                    },
                );
                self.request(
                    "turn/start",
                    &json!({
                        "threadId": side,
                        "input": [{ "type": "text", "text": format!("{SIDE_PREFIX}{question}"), "text_elements": [] }],
                        "approvalPolicy": "never",
                        "sandboxPolicy": { "type": "readOnly", "networkAccess": false },
                    }),
                    Outgoing::SideTurn { thread: side },
                );
                Ok(())
            }
            (Outgoing::TitleThread { prompt, effort }, Ok(response)) => {
                let current = self.thread_id();
                let helper = response["thread"]["id"]
                    .as_str()
                    .filter(|helper| Some(*helper) != current.as_deref())
                    .map(str::to_owned);
                let Some((titler, helper)) = self.titler.as_mut().zip(helper) else {
                    self.finish_title("");
                    return Ok(());
                };
                titler.helper = Some(helper.clone());
                let mut params = json!({
                    "threadId": helper,
                    "input": [{ "type": "text", "text": format!("{TITLE_PREFIX}{prompt}"), "text_elements": [] }],
                    "approvalPolicy": "never",
                    "sandboxPolicy": { "type": "readOnly", "networkAccess": false },
                });
                if let Some(effort) = effort {
                    params["effort"] = Value::from(effort);
                }
                self.request("turn/start", &params, Outgoing::TitleTurn);
                Ok(())
            }
            (Outgoing::TitleThread { .. } | Outgoing::TitleTurn, Err(error)) => {
                log::debug!(target: "zz::agent", "Codex did not name the thread: {error}");
                self.finish_title("");
                Ok(())
            }
            (Outgoing::TitleName { thread, title }, result) => {
                if result.is_err() || self.thread_id().as_deref() != Some(thread.as_str()) {
                    return Ok(());
                }
                if self.title.generated(Some(title.clone())) {
                    self.emit(AgentStreamPayload::TitleChanged { title: Some(title) })
                        .await?;
                }
                Ok(())
            }
            (Outgoing::SideFork { reply, .. }, Err(error)) => {
                self.notice_to(&format!("The side question failed: {error}"), Some(&reply))
                    .await
            }
            (Outgoing::SideTurn { thread }, Err(error)) => {
                let reply = self.sides.remove(&thread).map(|side| side.reply);
                self.notice_to(
                    &format!("The side question failed: {error}"),
                    reply.as_deref(),
                )
                .await
            }
            (Outgoing::Terminals, Ok(response)) => {
                let tasks = response["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|terminal| {
                        Some(AgentTaskWire {
                            id: terminal["processId"].as_str()?.to_owned(),
                            kind: "shell".to_owned(),
                            description: terminal["command"]
                                .as_str()
                                .unwrap_or_default()
                                .chars()
                                .take(1000)
                                .collect(),
                            tool_call_id: terminal["itemId"].as_str().map(str::to_owned),
                        })
                    })
                    .take(MAX_AGENT_TASKS)
                    .collect();
                self.set_tasks(tasks).await
            }
            (Outgoing::Terminate { task_id }, Ok(response)) if response["terminated"] == true => {
                let tasks = self
                    .tasks
                    .iter()
                    .filter(|task| task.id != task_id)
                    .cloned()
                    .collect();
                self.set_tasks(tasks).await
            }
            (Outgoing::Terminate { .. }, result) => {
                let reason = result
                    .err()
                    .map_or_else(String::new, |error| format!(": {error}"));
                self.notice(&format!("Codex could not stop that process{reason}"))
                    .await
            }
            (Outgoing::Rewind { target }, Ok(response)) => {
                self.rewind(&target, &response["thread"]).await
            }
            (Outgoing::Rewind { .. }, Err(error)) => {
                self.notice(&format!("Codex could not read the thread: {error}"))
                    .await
            }
            (Outgoing::SteerTurn, Err(error)) => {
                self.notice(&format!("Codex did not take the steer: {error}"))
                    .await
            }
            (
                Outgoing::Sessions {
                    client,
                    cwd,
                    replace,
                },
                Ok(response),
            ) => {
                let sessions = response["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|thread| {
                        Some(AgentSessionSummary {
                            session_id: thread["id"].as_str()?.to_owned(),
                            cwd: PathBuf::from(thread["cwd"].as_str()?),
                            additional_directories: Vec::new(),
                            title: thread["name"]
                                .as_str()
                                .or_else(|| thread["preview"].as_str())
                                .filter(|title| !title.is_empty())
                                .map(|title| title.chars().take(200).collect()),
                            updated_at: thread["updatedAt"].as_u64().and_then(|seconds| {
                                crate::claude::sessions::rfc3339(u128::from(seconds) * 1000)
                            }),
                        })
                    })
                    .filter(|session| session.cwd.is_dir())
                    .collect();
                self.emit(AgentStreamPayload::SessionsListed {
                    client,
                    sessions,
                    next_cursor: response["nextCursor"].as_str().map(str::to_owned),
                    cwd_filter: cwd,
                    replace,
                })
                .await
            }
            (Outgoing::Sessions { client, .. }, Err(error)) => {
                self.emit(AgentStreamPayload::SessionListFailed {
                    client,
                    message: format!("could not list Codex threads: {error}"),
                })
                .await
            }
        }
    }

    async fn child(&mut self, event: ChildEvent) -> Result<(), String> {
        match event {
            ChildEvent::Line(generation, _) | ChildEvent::Closed(generation)
                if generation != self.generation =>
            {
                Ok(())
            }
            ChildEvent::Line(_, line) => {
                let Ok(message) = serde_json::from_slice::<Value>(&line) else {
                    return Ok(());
                };
                self.message(message).await
            }
            ChildEvent::Closed(_) => self.closed().await,
        }
    }

    async fn closed(&mut self) -> Result<(), String> {
        let detail = self.process.as_mut().and_then(Process::exit_detail);
        self.process = None;
        self.initialized = false;
        let mut message = detail.map_or_else(
            || "Codex exited".to_owned(),
            |status| format!("Codex exited ({status})"),
        );
        if let Some(tail) = self.stderr.snapshot() {
            message.push('\n');
            message.push_str(&tail);
        }
        self.cancel_permissions().await?;
        let mut flying = self
            .requests
            .drain()
            .filter_map(|(id, (outgoing, _))| match outgoing {
                Outgoing::Steer { steer, .. } => Some((id, steer)),
                _ => None,
            })
            .collect::<Vec<_>>();
        flying.sort_unstable_by_key(|(id, _)| *id);
        let steers = flying
            .into_iter()
            .map(|(_, steer)| steer)
            .chain(std::mem::take(&mut self.steers))
            .collect();
        self.give_back(steers).await?;
        if let Some(starting) = self.starting.take() {
            return self.fail_start(starting.start, message).await;
        }
        self.apply(&Fact::Exited {
            message: message.clone(),
        })
        .await?;
        self.emit(AgentStreamPayload::PaneFailed { message }).await
    }

    async fn message(&mut self, message: Value) -> Result<(), String> {
        let method = message["method"].as_str().map(str::to_owned);
        match (method, message.get("id")) {
            (Some(method), Some(rpc_id)) => {
                let rpc_id = rpc_id.clone();
                self.server_request(&method, rpc_id, &message["params"])
                    .await
            }
            (Some(method), None) => self.notification(&method, &message["params"]).await,
            (None, Some(id)) => {
                let Some((outgoing, _)) = id.as_u64().and_then(|id| self.requests.remove(&id))
                else {
                    return Ok(());
                };
                let result = match message.get("error") {
                    Some(error) => Err(error["message"]
                        .as_str()
                        .unwrap_or("request failed")
                        .to_owned()),
                    None => Ok(message["result"].clone()),
                };
                self.settled(outgoing, result).await
            }
            (None, None) => Ok(()),
        }
    }

    async fn resolved(&mut self, params: &Value) -> Result<(), String> {
        let request = params["requestId"].clone();
        let resolved = self
            .permissions
            .iter()
            .filter(|(_, pending)| pending.rpc_id == request)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for request_id in resolved {
            self.permissions.remove(&request_id);
            self.emit(AgentStreamPayload::PermissionResolved {
                request_id,
                canceled: false,
            })
            .await?;
        }
        Ok(())
    }

    async fn notification(&mut self, method: &str, params: &Value) -> Result<(), String> {
        if method == "serverRequest/resolved" {
            return self.resolved(params).await;
        }
        if let Some(thread) = params["threadId"].as_str()
            && self.sides.contains_key(thread)
        {
            return self
                .side_notification(thread.to_owned(), method, params)
                .await;
        }
        if params["threadId"]
            .as_str()
            .is_some_and(|thread| self.is_title_helper(thread))
        {
            self.title_notification(method, params);
            return Ok(());
        }
        if params["threadId"]
            .as_str()
            .is_some_and(|thread| self.left.contains_key(thread))
        {
            return Ok(());
        }
        let current = self.thread_id();
        if let Some(thread) = params["threadId"].as_str()
            && Some(thread) != current.as_deref()
        {
            for update in self
                .translator
                .child(thread, method, params)
                .unwrap_or_default()
            {
                self.update(update, true).await?;
            }
            return Ok(());
        }
        match method {
            "turn/started" => {
                if let Some(command) = params["turn"]["id"].as_str() {
                    self.apply(&Fact::Started {
                        command: command.to_owned(),
                    })
                    .await?;
                }
            }
            "thread/status/changed" => {
                let fact = if params["status"]["type"] == "active" {
                    Fact::Busy
                } else {
                    Fact::Idle
                };
                self.apply(&fact).await?;
            }
            "turn/completed" => {
                if let Some(command) = params["turn"]["id"].as_str() {
                    let end = match params["turn"]["status"].as_str() {
                        Some("interrupted") => End::Cancelled,
                        Some("failed") => End::Failed(
                            params["turn"]["error"]["message"]
                                .as_str()
                                .unwrap_or("Codex turn failed")
                                .to_owned(),
                        ),
                        _ => End::Completed,
                    };
                    self.apply(&Fact::Ended {
                        command: command.to_owned(),
                        end,
                    })
                    .await?;
                }
                if let Some(thread) = current {
                    self.request(
                        "thread/backgroundTerminals/list",
                        &json!({ "threadId": thread }),
                        Outgoing::Terminals,
                    );
                }
            }
            "item/completed" if params["item"]["type"] == "commandExecution" => {
                let item = params["item"]["id"].as_str();
                if self
                    .tasks
                    .iter()
                    .any(|task| task.tool_call_id.as_deref() == item)
                {
                    let tasks = self
                        .tasks
                        .iter()
                        .filter(|task| task.tool_call_id.as_deref() != item)
                        .cloned()
                        .collect();
                    self.set_tasks(tasks).await?;
                }
            }
            "error" => {
                if let Some(text) = params["error"]["message"].as_str() {
                    self.notice(text).await?;
                }
            }
            "account/rateLimits/updated" => {
                if let Some(quota) = translate::rate_limits(&params["rateLimits"]) {
                    self.emit(AgentStreamPayload::QuotaChanged { quota })
                        .await?;
                }
                return Ok(());
            }
            "thread/name/updated" => {
                let title = params["threadName"].as_str().and_then(clean_title);
                if self.title.rename(title.clone()) {
                    self.emit(AgentStreamPayload::TitleChanged { title })
                        .await?;
                }
            }
            _ => {}
        }
        if self.session.is_none() {
            return Ok(());
        }
        let updates = self.translator.notification(method, params);
        self.answered_elsewhere(&updates).await?;
        for update in updates {
            self.update(update, true).await?;
        }
        self.open_questions().await
    }

    async fn side_notification(
        &mut self,
        thread: String,
        method: &str,
        params: &Value,
    ) -> Result<(), String> {
        let Some(side) = self.sides.get_mut(&thread) else {
            return Ok(());
        };
        match method {
            "item/agentMessage/delta" => {
                if let Some(delta) = params["delta"].as_str() {
                    side.text.push_str(delta);
                    side.streamed = true;
                }
            }
            "item/completed" if params["item"]["type"] == "agentMessage" && !side.streamed => {
                if let Some(text) = params["item"]["text"].as_str() {
                    if !side.text.is_empty() {
                        side.text.push_str("\n\n");
                    }
                    side.text.push_str(text);
                }
            }
            "turn/completed" => {
                if let Some(side) = self.sides.remove(&thread) {
                    let answer = if side.text.trim().is_empty() {
                        "Codex had no answer.".to_owned()
                    } else {
                        side.text
                    };
                    self.update(
                        json!({
                            "sessionUpdate": "agent_message_chunk",
                            "messageId": side.message_id,
                            "content": { "type": "text", "text": answer },
                            "_meta": { "zz": { "side": true, "reply": side.reply } },
                        }),
                        true,
                    )
                    .await?;
                    self.request(
                        "thread/unsubscribe",
                        &json!({ "threadId": thread }),
                        Outgoing::Quiet,
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn server_request(
        &mut self,
        method: &str,
        rpc_id: Value,
        params: &Value,
    ) -> Result<(), String> {
        if params["threadId"]
            .as_str()
            .is_some_and(|thread| self.sides.contains_key(thread))
        {
            self.reply_error(&rpc_id, "a side question does not run tools");
            return Ok(());
        }
        if params["threadId"]
            .as_str()
            .is_some_and(|thread| self.is_title_helper(thread))
        {
            self.reply_error(&rpc_id, "naming a thread does not run tools");
            return Ok(());
        }
        if params["threadId"]
            .as_str()
            .is_some_and(|thread| self.left.contains_key(thread))
        {
            match approval(method, params) {
                Some((reply, _)) => self.reply(&rpc_id, &decision(&reply, "cancel")),
                None => self.reply_error(&rpc_id, "the chat moved to another thread"),
            }
            return Ok(());
        }
        if let Some((reply, tool_call)) = approval(method, params) {
            if matches!(
                method,
                "item/commandExecution/requestApproval" | "execCommandApproval"
            ) && *self.auto_approve.lock() != AgentAutoApprove::Off
                && zz_command::runs_only_zz(&params["command"])
            {
                self.reply(&rpc_id, &decision(&reply, "allow"));
                return Ok(());
            }
            return self.approve(rpc_id, reply, tool_call).await;
        }
        match method {
            "item/tool/requestUserInput" => {
                let item = params["itemId"].as_str().unwrap_or_default();
                let (questions, options, title) = user_input(params);
                self.ask(
                    Pending {
                        rpc_id,
                        reply: Reply::Questions(questions.clone()),
                    },
                    json!({ "toolCallId": item, "title": title }),
                    options,
                    questions,
                )
                .await
            }
            "mcpServer/elicitation/request" => {
                self.reply(
                    &rpc_id,
                    &json!({ "action": "decline", "content": null, "_meta": null }),
                );
                Ok(())
            }
            _ => {
                self.reply_error(&rpc_id, &format!("zz does not handle {method}"));
                Ok(())
            }
        }
    }
}

fn generated_title(text: &str) -> Option<String> {
    let title = clean_title(text)?;
    let title = title
        .trim_matches(|character| matches!(character, '"' | '`' | '*' | '“' | '”'))
        .trim_end_matches('.')
        .trim();
    (!title.is_empty()).then(|| title.to_owned())
}

pub fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| zz_daemon_client::home_directory().map(|home| home.join(".codex")))
}

pub fn ensure_codex_rule(codex_home: &Path) -> std::io::Result<bool> {
    let rules = codex_home.join("rules");
    let path = rules.join("zz.rules");
    if path.exists() {
        return Ok(false);
    }
    std::fs::create_dir_all(&rules)?;
    std::fs::write(&path, zz_daemon_client::agent_context::CODEX_RULES)?;
    Ok(true)
}

#[cfg(unix)]
pub fn uses_managed_daemon(command: &CodexCommand) -> bool {
    command.args.is_empty() && command.env.is_empty()
}

#[cfg(unix)]
pub fn managed_socket() -> Option<PathBuf> {
    control::socket(&codex_home()?)
}

#[cfg(unix)]
pub fn restart_managed_daemon(program: &Path) {
    use zz_daemon_client::unmasked::SpawnUnmasked as _;

    if crate::sandboxed() || managed_socket().is_none() {
        return;
    }
    let mut restart = Command::new(program);
    restart
        .args(["app-server", "daemon", "restart"])
        .env_remove("ZZ_PANE")
        .env_remove("ZZ_SESSION")
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    accounts::strip_logins(&mut restart, AgentProvider::Codex);
    match restart.status_unmasked() {
        Ok(status) if status.success() => {}
        Ok(status) => {
            log::warn!(target: "zz::agent", "codex app-server daemon restart exited with {status}");
        }
        Err(error) => {
            log::warn!(target: "zz::agent", "could not restart the Codex app server daemon: {error}");
        }
    }
}

#[cfg(unix)]
fn managed_daemon(program: &Path, command: &CodexCommand) -> Option<PathBuf> {
    use zz_daemon_client::unmasked::SpawnUnmasked as _;

    if crate::sandboxed() || !uses_managed_daemon(command) {
        return None;
    }
    if let Some(socket) = managed_socket() {
        return Some(socket);
    }
    let mut start = Command::new(program);
    start
        .args(["app-server", "daemon", "start"])
        .env_remove("ZZ_PANE")
        .env_remove("ZZ_SESSION")
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    accounts::strip_logins(&mut start, AgentProvider::Codex);
    if let Err(error) = start.status_unmasked() {
        log::warn!(target: "zz::agent", "could not start the Codex app server daemon: {error}");
        return None;
    }
    let deadline = Instant::now() + MANAGED_START;
    loop {
        if let Some(socket) = managed_socket() {
            return Some(socket);
        }
        if Instant::now() >= deadline {
            log::warn!(target: "zz::agent", "the Codex app server daemon did not open its socket; running Codex's app server for this pane");
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn thread_items(thread: &Value) -> Vec<Value> {
    thread["turns"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|turn| turn["items"].as_array().cloned().unwrap_or_default())
        .collect()
}

fn running_turn(thread: &Value) -> Option<&str> {
    let turn = thread["turns"].as_array()?.last()?;
    if thread["status"]["type"] != "active" || turn["status"] != "inProgress" {
        return None;
    }
    turn["id"].as_str()
}

fn turn_history(
    translator: &mut Translator,
    turn: &Value,
    children: &HashMap<String, Vec<Value>>,
) -> Vec<Value> {
    let mut updates = Vec::new();
    for item in turn["items"].as_array().into_iter().flatten() {
        updates.extend(stamped(translator.history_start(item), item));
        for child in spawned(item) {
            for step in children.get(&child).into_iter().flatten() {
                updates.extend(stamped(translator.child_history(&child, step), step));
            }
        }
        updates.extend(stamped(translator.history_end(item), item));
    }
    let seconds = |at: &Value| at.as_u64().map(|at| at.saturating_mul(1000));
    let end = match turn["status"].as_str() {
        Some("completed") => "completed",
        Some("interrupted") => "cancelled",
        Some("failed") => "failed",
        _ => return updates,
    };
    if updates.is_empty() {
        return updates;
    }
    let mut ended = turn_mark(end, seconds(&turn["completedAt"]));
    if end == "failed" {
        let error = turn["error"]["message"].as_str();
        ended["_meta"]["zz"]["error"] = Value::from(error.unwrap_or("Codex turn failed"));
    }
    updates.insert(0, turn_mark("started", seconds(&turn["startedAt"])));
    updates.push(ended);
    updates
}

fn stamped(updates: Vec<Value>, item: &Value) -> impl Iterator<Item = Value> {
    let started = item["startedAtMs"].as_u64();
    let ended = item["completedAtMs"].as_u64();
    updates.into_iter().map(move |mut update| {
        let at = if update["sessionUpdate"] == "tool_call_update" {
            ended.or(started)
        } else {
            started.or(ended)
        };
        if let Some(at) = at {
            update["_meta"]["zz"]["at"] = Value::from(at);
        }
        update
    })
}

fn spawned(item: &Value) -> Vec<String> {
    let threads = match item["type"].as_str() {
        Some("subAgentActivity") if item["kind"] == "started" => vec![&item["agentThreadId"]],
        Some("collabAgentToolCall") if item["tool"] == "spawnAgent" => item["receiverThreadIds"]
            .as_array()
            .map(|threads| threads.iter().collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    threads
        .into_iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn compact_command(turn_id: u64) -> String {
    format!("zz-compact-{turn_id}")
}

pub fn turn_input(prompt: &AgentPrompt, note: Option<&str>) -> Value {
    let mut input = Vec::new();
    if !prompt.text.is_empty() {
        input.push(json!({ "type": "text", "text": prompt.text, "text_elements": [] }));
    }
    if let Some(note) = note {
        input.push(json!({ "type": "text", "text": note, "text_elements": [] }));
    }
    for image in &prompt.images {
        input.push(json!({
            "type": "image",
            "url": format!("data:{};base64,{}", image.format, BASE64.encode(&image.data)),
        }));
    }
    Value::Array(input)
}

#[cfg(test)]
mod replay_tests;
#[cfg(test)]
pub use replay_tests::tapes;

#[cfg(test)]
mod runtime_tests;
