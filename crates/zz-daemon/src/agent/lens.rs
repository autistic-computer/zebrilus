use std::{
    fs::File,
    io::{Read as _, Seek as _, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use zz_protocol::{AgentProvider, MAX_AGENT_PERMISSION_BYTES, PaneId};

use crate::agent::{
    claude::{
        sessions,
        translate::{self as claude_translate, Translator as ClaudeTranslator},
    },
    claude_peers::{self, PeerRecord},
    codex::{
        rollout::{self, RolloutMeta},
        translate::Translator as CodexTranslator,
    },
    codex_queue,
    host::RuntimeChannels,
    runtime::{RuntimeCommand, RuntimeControl},
    stream::{AgentPrompt, AgentPromptOutcome, AgentSessionCapabilities, AgentStreamPayload},
};

const POLL: Duration = Duration::from_millis(250);
const LOCATE: Duration = Duration::from_secs(1);
const CODEX_LOCATE: Duration = Duration::from_secs(5);
const SETTLE: Duration = Duration::from_secs(5);
const TAIL_BYTES: u64 = 8 * 1024 * 1024;
const READ_LIMIT: u64 = 4 * 1024 * 1024;
const PERMISSION_PROMPT: &str = "permission prompt";
pub(crate) const ALLOW: &str = "allow";
pub(crate) const DENY: &str = "deny";

pub(crate) trait LensPane: Send + Sync {
    fn pid(&self) -> Option<u32>;
    fn submit(&self, text: &str) -> Result<(), String>;
    fn keys(&self, keys: &[&str]) -> Result<(), String>;
}

#[derive(Clone)]
pub(crate) struct LensSpec {
    pub(crate) pane: PaneId,
    pub(crate) provider: AgentProvider,
    pub(crate) io: Arc<dyn LensPane>,
}

impl std::fmt::Debug for LensSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LensSpec")
            .field("pane", &self.pane)
            .field("provider", &self.provider)
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
struct Tail {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
}

impl Tail {
    fn open(path: PathBuf) -> (Self, String) {
        let mut tail = Self {
            path,
            ..Self::default()
        };
        let text = tail.start();
        (tail, text)
    }

    fn start(&mut self) -> String {
        let Ok(mut file) = File::open(&self.path) else {
            return String::new();
        };
        let length = file.metadata().map_or(0, |metadata| metadata.len());
        let start = length.saturating_sub(TAIL_BYTES);
        let mut bytes = Vec::new();
        if file.seek(SeekFrom::Start(start)).is_err()
            || file.take(length - start).read_to_end(&mut bytes).is_err()
        {
            return String::new();
        }
        let begin = if start > 0 {
            bytes
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |at| at + 1)
        } else {
            0
        };
        let end = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(begin, |at| at + 1)
            .max(begin);
        self.offset = start + end as u64;
        String::from_utf8_lossy(&bytes[begin..end]).into_owned()
    }

    fn lines(&mut self) -> Vec<Value> {
        let Ok(mut file) = File::open(&self.path) else {
            return Vec::new();
        };
        let length = file.metadata().map_or(0, |metadata| metadata.len());
        if length < self.offset {
            self.offset = 0;
            self.partial.clear();
        }
        if length > self.offset && file.seek(SeekFrom::Start(self.offset)).is_ok() {
            let mut bytes = Vec::new();
            if file
                .take(READ_LIMIT.min(length - self.offset))
                .read_to_end(&mut bytes)
                .is_ok()
            {
                self.offset += bytes.len() as u64;
                self.partial.extend_from_slice(&bytes);
            }
        }
        let mut lines = Vec::new();
        while let Some(end) = self.partial.iter().position(|byte| *byte == b'\n') {
            let line = self.partial.drain(..=end).collect::<Vec<_>>();
            if let Ok(entry) = serde_json::from_slice::<Value>(&line) {
                lines.push(entry);
            }
        }
        lines
    }
}

#[derive(Default)]
struct Read {
    updates: Vec<Value>,
    finished: bool,
    reload: bool,
}

struct Found {
    session: String,
    path: PathBuf,
    cwd: PathBuf,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Status {
    busy: bool,
    waiting: Option<String>,
}

struct ClaudeFiles {
    registry: PathBuf,
    config: PathBuf,
}

impl ClaudeFiles {
    fn from_environment() -> Option<Self> {
        Some(Self {
            registry: claude_peers::registry_dir().ok()?,
            config: sessions::config_home()?,
        })
    }
}

struct Claude {
    files: Option<ClaudeFiles>,
    record: Option<PeerRecord>,
    tail: Option<Tail>,
    translator: ClaudeTranslator,
    last: Option<String>,
    open: Vec<(String, String, Value)>,
}

impl Claude {
    fn new(files: Option<ClaudeFiles>) -> Self {
        Self {
            files,
            record: None,
            tail: None,
            translator: ClaudeTranslator::default(),
            last: None,
            open: Vec::new(),
        }
    }

    fn locate(&mut self, pane: PaneId, pane_pid: Option<u32>) -> Option<Found> {
        let files = self.files.as_ref()?;
        let mut records = claude_peers::read_records_in(&files.registry).ok()?;
        records.retain(|record| record.zz.is_none());
        let record = claude_peers::record_for_pane(&records, &pane.to_string(), pane_pid)?.clone();
        let cwd = PathBuf::from(&record.cwd);
        let path = sessions::session_file(&files.config, &cwd, &record.session_id);
        let session = record.session_id.clone();
        self.record = Some(record);
        Some(Found {
            session,
            path: path?,
            cwd,
        })
    }

    fn status(&mut self) -> Option<Status> {
        let files = self.files.as_ref()?;
        let pid = self.record.as_ref()?.pid;
        let record = std::fs::read(files.registry.join(format!("{pid}.json")))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PeerRecord>(&bytes).ok())
            .filter(|record| record.pid == pid);
        let Some(record) = record else {
            self.record = None;
            return Some(Status::default());
        };
        let status = Status {
            busy: matches!(record.status.as_str(), "busy" | "waiting"),
            waiting: (record.status == "waiting")
                .then(|| record.waiting_for.clone())
                .filter(|why| !why.is_empty()),
        };
        self.record = Some(record);
        Some(status)
    }

    fn load(&mut self, path: PathBuf, cwd: &Path) -> Vec<Value> {
        let (tail, text) = Tail::open(path);
        self.translator = ClaudeTranslator::new(cwd.to_path_buf());
        self.open.clear();
        self.last = text
            .lines()
            .rev()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find_map(|entry| entry["uuid"].as_str().map(str::to_owned));
        let mut updates = Vec::new();
        for entry in sessions::chain_tail(&text) {
            self.track(&entry);
            updates.extend(self.translator.history_entry(&entry));
        }
        self.tail = Some(tail);
        updates
    }

    fn read(&mut self) -> Read {
        let mut read = Read::default();
        let Some(tail) = self.tail.as_mut() else {
            return read;
        };
        for entry in tail.lines() {
            if read.reload {
                break;
            }
            self.entry(&entry, &mut read);
        }
        read
    }

    fn entry(&mut self, entry: &Value, read: &mut Read) {
        if entry["type"] == "system" && entry["subtype"] == "turn_duration" {
            read.finished = true;
        }
        let Some(uuid) = entry["uuid"].as_str() else {
            return;
        };
        if entry["isSidechain"] == true {
            return;
        }
        let branched = entry["type"] == "user"
            && entry["isCompactSummary"] != true
            && claude_translate::prompt_text(entry).is_some()
            && self.last.is_some()
            && entry["parentUuid"].as_str() != self.last.as_deref();
        self.last = Some(uuid.to_owned());
        if branched {
            read.reload = true;
            return;
        }
        self.track(entry);
        read.updates.extend(self.translator.history_entry(entry));
    }

    fn track(&mut self, entry: &Value) {
        let blocks = entry["message"]["content"].as_array().into_iter().flatten();
        match entry["type"].as_str() {
            Some("assistant") => {
                for block in blocks.filter(|block| block["type"] == "tool_use") {
                    if let (Some(id), Some(name)) = (block["id"].as_str(), block["name"].as_str()) {
                        self.open
                            .push((id.to_owned(), name.to_owned(), block["input"].clone()));
                    }
                }
            }
            Some("user") => {
                for block in blocks.filter(|block| block["type"] == "tool_result") {
                    if let Some(id) = block["tool_use_id"].as_str() {
                        self.open.retain(|(open, _, _)| open != id);
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct Codex {
    pid: Option<u32>,
    tail: Option<Tail>,
    translator: CodexTranslator,
    busy: bool,
}

impl Codex {
    fn locate(&mut self, pane_pid: Option<u32>) -> Option<Found> {
        self.pid = codex_queue::codex_pid(pane_pid?);
        let (path, meta) = open_files(self.pid?)
            .into_iter()
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "jsonl")
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("rollout-"))
            })
            .filter_map(|path| RolloutMeta::read(&path).map(|meta| (path, meta)))
            .find(|(_, meta)| meta.parent.is_none())?;
        Some(Found {
            session: meta.thread,
            path,
            cwd: meta.cwd,
        })
    }

    fn load(&mut self, path: PathBuf, cwd: &Path) -> Vec<Value> {
        let (tail, text) = Tail::open(path);
        self.translator = CodexTranslator::new(cwd.to_path_buf());
        self.busy = false;
        let mut read = Read::default();
        for entry in text
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        {
            self.entry(&entry, &mut read);
        }
        self.tail = Some(tail);
        read.updates
    }

    fn read(&mut self) -> Read {
        let mut read = Read::default();
        let lines = self.tail.as_mut().map(Tail::lines).unwrap_or_default();
        for entry in lines {
            self.entry(&entry, &mut read);
        }
        read
    }

    fn entry(&mut self, entry: &Value, read: &mut Read) {
        if entry["type"] != "event_msg" {
            return;
        }
        let payload = &entry["payload"];
        match payload["type"].as_str() {
            Some("task_started") => self.busy = true,
            Some("task_complete" | "turn_aborted") => {
                self.busy = false;
                read.finished = true;
            }
            Some("item_completed") => {
                let item = &payload["item"];
                let item = if item["type"] == "UserMessage" {
                    Some(
                        json!({ "type": "userMessage", "id": item["id"], "content": item["content"] }),
                    )
                } else {
                    rollout::v2_item(item)
                };
                if let Some(item) = item {
                    read.updates.extend(self.translator.history_item(&item));
                }
            }
            _ => {}
        }
    }
}

#[cfg(target_os = "linux")]
fn open_files(pid: u32) -> Vec<PathBuf> {
    std::fs::read_dir(format!("/proc/{pid}/fd"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| std::fs::read_link(entry.path()).ok())
        .collect()
}

#[cfg(not(target_os = "linux"))]
fn open_files(pid: u32) -> Vec<PathBuf> {
    use zz_daemon_client::unmasked::SpawnUnmasked as _;

    std::process::Command::new("lsof")
        .args(["-n", "-P", "-Fn", "-p", &pid.to_string()])
        .output_unmasked()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| line.strip_prefix('n'))
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

enum Source {
    Claude(Claude),
    Codex(Codex),
}

impl Source {
    fn locate(&mut self, pane: PaneId, pane_pid: Option<u32>) -> Option<Found> {
        match self {
            Self::Claude(claude) => claude.locate(pane, pane_pid),
            Self::Codex(codex) => codex.locate(pane_pid),
        }
    }

    fn load(&mut self, path: PathBuf, cwd: &Path) -> Vec<Value> {
        match self {
            Self::Claude(claude) => claude.load(path, cwd),
            Self::Codex(codex) => codex.load(path, cwd),
        }
    }

    fn read(&mut self) -> Read {
        match self {
            Self::Claude(claude) => claude.read(),
            Self::Codex(codex) => codex.read(),
        }
    }

    fn status(&mut self) -> Option<Status> {
        match self {
            Self::Claude(claude) => claude.status(),
            Self::Codex(codex) => Some(Status {
                busy: codex.busy,
                waiting: None,
            }),
        }
    }

    fn pending_tool(&self) -> Option<&(String, String, Value)> {
        match self {
            Self::Claude(claude) => claude.open.last(),
            Self::Codex(_) => None,
        }
    }

    const fn every(&self) -> Duration {
        match self {
            Self::Claude(_) => LOCATE,
            Self::Codex(_) => CODEX_LOCATE,
        }
    }
}

struct Turn {
    id: u64,
    sent: Instant,
    busy: bool,
    cancelled: bool,
}

struct Permission {
    request_id: u64,
    tool: Option<String>,
    answered: bool,
}

struct Lens {
    spec: LensSpec,
    source: Source,
    permission_ids: Arc<AtomicU64>,
    session: Option<String>,
    cwd: PathBuf,
    busy: bool,
    turn: Option<Turn>,
    permission: Option<Permission>,
    next_locate: Instant,
}

fn option(id: &str, name: &str, kind: &str) -> Value {
    json!({ "optionId": id, "name": name, "kind": kind })
}

impl Lens {
    fn new(spec: LensSpec, permission_ids: Arc<AtomicU64>) -> Self {
        let source = match spec.provider {
            AgentProvider::ClaudeCode => {
                Source::Claude(Claude::new(ClaudeFiles::from_environment()))
            }
            AgentProvider::Codex => Source::Codex(Codex::default()),
        };
        Self::with_source(spec, source, permission_ids)
    }

    fn with_source(spec: LensSpec, source: Source, permission_ids: Arc<AtomicU64>) -> Self {
        Self {
            spec,
            source,
            permission_ids,
            session: None,
            cwd: PathBuf::from("/"),
            busy: false,
            turn: None,
            permission: None,
            next_locate: Instant::now(),
        }
    }

    fn placeholder(&self) -> String {
        format!("lens-{}", self.spec.pane)
    }

    fn open(&mut self) -> Vec<AgentStreamPayload> {
        let (agent_name, agent_key) = match self.spec.provider {
            AgentProvider::ClaudeCode => ("Claude Code", "claude-code"),
            AgentProvider::Codex => ("Codex", "codex"),
        };
        self.session = None;
        let mut payloads = vec![AgentStreamPayload::Ready {
            agent_name: agent_name.to_owned(),
            agent_key: agent_key.to_owned(),
            auth_methods: Vec::new(),
            capabilities: AgentSessionCapabilities::default(),
        }];
        payloads.extend(self.locate(true));
        payloads.extend(self.observe(false));
        payloads
    }

    fn locate(&mut self, opening: bool) -> Vec<AgentStreamPayload> {
        self.next_locate = Instant::now() + self.source.every();
        let found = self.source.locate(self.spec.pane, self.spec.io.pid());
        let Some(found) = found else {
            return if opening {
                vec![
                    AgentStreamPayload::SessionReset { restoring: false },
                    AgentStreamPayload::SessionReady {
                        session_id: self.placeholder(),
                        modes: None,
                        config_options: None,
                    },
                ]
            } else {
                Vec::new()
            };
        };
        if !opening && self.session.as_deref() == Some(found.session.as_str()) {
            return Vec::new();
        }
        self.reload(found, opening)
    }

    fn reload(&mut self, found: Found, opening: bool) -> Vec<AgentStreamPayload> {
        let replay = self.source.load(found.path, &found.cwd);
        let first = self.session.is_none();
        self.session = Some(found.session.clone());
        self.cwd.clone_from(&found.cwd);
        self.permission = None;
        let updates = replay
            .into_iter()
            .map(|update| AgentStreamPayload::Update { update });
        if opening {
            let mut payloads = vec![AgentStreamPayload::SessionReset {
                restoring: updates.len() > 0,
            }];
            payloads.extend(updates);
            payloads.push(AgentStreamPayload::SessionReady {
                session_id: found.session,
                modes: None,
                config_options: None,
            });
            return payloads;
        }
        if first {
            return updates.collect();
        }
        let mut payloads = self.settle(false).into_iter().collect::<Vec<_>>();
        payloads.push(AgentStreamPayload::SessionReset { restoring: true });
        payloads.extend(updates);
        payloads.push(AgentStreamPayload::SessionSwitched {
            session_id: found.session,
            cwd: found.cwd,
            modes: None,
            config_options: None,
            replay: Vec::new(),
        });
        payloads
    }

    fn tick(&mut self) -> Vec<AgentStreamPayload> {
        let mut payloads = if Instant::now() >= self.next_locate {
            self.locate(false)
        } else {
            Vec::new()
        };
        let read = self.source.read();
        if read.reload {
            if let Some(found) = self.source.locate(self.spec.pane, self.spec.io.pid()) {
                self.session = Some(String::new());
                payloads.extend(self.reload(found, false));
            }
            return payloads;
        }
        payloads.extend(
            read.updates
                .into_iter()
                .map(|update| AgentStreamPayload::Update { update }),
        );
        payloads.extend(self.observe(read.finished));
        payloads
    }

    fn observe(&mut self, finished: bool) -> Vec<AgentStreamPayload> {
        let mut payloads = Vec::new();
        let Some(status) = self.source.status() else {
            return payloads;
        };
        payloads.extend(self.follow_permission(status.waiting.as_deref()));
        if status.busy != self.busy {
            self.busy = status.busy;
            payloads.push(AgentStreamPayload::Activity { busy: status.busy });
        }
        let settled = self.turn.as_mut().is_some_and(|turn| {
            turn.busy |= status.busy;
            finished || (!status.busy && (turn.busy || turn.sent.elapsed() >= SETTLE))
        });
        if settled {
            payloads.extend(self.settle(false));
        }
        payloads
    }

    fn follow_permission(&mut self, waiting: Option<&str>) -> Vec<AgentStreamPayload> {
        let tool = self.source.pending_tool().map(|(id, _, _)| id.clone());
        let Some(why) = waiting else {
            return match self.permission.take() {
                Some(permission) if !permission.answered => {
                    vec![AgentStreamPayload::PermissionResolved {
                        request_id: permission.request_id,
                        canceled: false,
                    }]
                }
                _ => Vec::new(),
            };
        };
        let mut payloads = Vec::new();
        if let Some(permission) = &self.permission {
            if !permission.answered || permission.tool == tool {
                return payloads;
            }
            self.permission = None;
        }
        let request_id = self.permission_ids.fetch_add(1, Ordering::Relaxed);
        let answerable = why == PERMISSION_PROMPT;
        let pending = self.source.pending_tool().filter(|_| answerable);
        let (tool_call, options) = if let Some((id, name, input)) = pending {
            let info = claude_translate::tool_info(name, input, &self.cwd);
            let mut call = json!({
                "toolCallId": id,
                "title": info.title,
                "kind": info.kind,
                "rawInput": input,
            });
            if !info.content.is_empty() {
                call["content"] = Value::Array(info.content);
            }
            if serde_json::to_vec(&call).map_or(0, |bytes| bytes.len())
                > MAX_AGENT_PERMISSION_BYTES / 2
                && let Some(fields) = call.as_object_mut()
            {
                fields.remove("rawInput");
                fields.remove("content");
            }
            (
                call,
                json!([
                    option(ALLOW, "Allow", "allow_once"),
                    option(DENY, "Deny", "reject_once"),
                ]),
            )
        } else {
            (
                json!({
                    "toolCallId": format!("lens-wait-{request_id}"),
                    "title": format!("Waiting for a {why} in the terminal"),
                    "kind": "other",
                }),
                json!([]),
            )
        };
        self.permission = Some(Permission {
            request_id,
            tool: pending.map(|(id, _, _)| id.clone()),
            answered: false,
        });
        payloads.push(AgentStreamPayload::PermissionRequested {
            request_id,
            tool_call,
            options,
            questions: Vec::new(),
        });
        payloads
    }

    fn settle(&mut self, failed: bool) -> Option<AgentStreamPayload> {
        let turn = self.turn.take()?;
        let stop_reason = if failed || turn.cancelled {
            "cancelled"
        } else {
            "end_turn"
        };
        Some(AgentStreamPayload::PromptFinished {
            turn_id: turn.id,
            outcome: AgentPromptOutcome::Finished {
                stop_reason: Value::from(stop_reason),
            },
        })
    }

    fn prompt(&mut self, turn_id: u64, prompt: &AgentPrompt) -> Vec<AgentStreamPayload> {
        let failed = |message: &str| {
            vec![AgentStreamPayload::PromptFinished {
                turn_id,
                outcome: AgentPromptOutcome::Failed {
                    message: message.to_owned(),
                },
            }]
        };
        if !prompt.images.is_empty() {
            return failed("The lens sends text only; paste images in the terminal.");
        }
        if prompt.text.trim().is_empty() {
            return failed("Nothing to send.");
        }
        match self.spec.io.submit(&prompt.text) {
            Ok(()) => {
                self.turn = Some(Turn {
                    id: turn_id,
                    sent: Instant::now(),
                    busy: false,
                    cancelled: false,
                });
                Vec::new()
            }
            Err(message) => failed(&message),
        }
    }

    fn cancel(&mut self) {
        if let Some(turn) = self.turn.as_mut() {
            turn.cancelled = true;
        }
        if let Err(error) = self.spec.io.keys(&["Escape"]) {
            log::debug!(target: "zz::agent", "could not stop {}: {error}", self.spec.pane);
        }
    }

    fn respond(&mut self, request_id: u64, option_id: Option<&str>) -> Vec<AgentStreamPayload> {
        let Some(permission) = self
            .permission
            .as_mut()
            .filter(|permission| permission.request_id == request_id && !permission.answered)
        else {
            return Vec::new();
        };
        if permission.tool.is_none() {
            return Vec::new();
        }
        let keys: &[&str] = if option_id == Some(ALLOW) {
            &["1"]
        } else {
            &["Escape"]
        };
        match self.spec.io.keys(keys) {
            Ok(()) => {
                permission.answered = true;
                vec![AgentStreamPayload::PermissionResolved {
                    request_id,
                    canceled: option_id.is_none(),
                }]
            }
            Err(error) => {
                log::debug!(target: "zz::agent", "could not answer {}: {error}", self.spec.pane);
                Vec::new()
            }
        }
    }

    fn command(&mut self, command: RuntimeCommand) -> Vec<AgentStreamPayload> {
        let unsupported = "The lens follows the terminal; use the CLI's own commands there.";
        match command {
            RuntimeCommand::Open { .. } => self.open(),
            RuntimeCommand::Prompt { turn_id, prompt } => self.prompt(turn_id, &prompt),
            RuntimeCommand::ListSessions { client, .. } => {
                vec![AgentStreamPayload::SessionListFailed {
                    client,
                    message: unsupported.to_owned(),
                }]
            }
            RuntimeCommand::SwitchSession { .. } | RuntimeCommand::NewSession { .. } => {
                vec![AgentStreamPayload::SessionSwitchFailed {
                    message: unsupported.to_owned(),
                }]
            }
            RuntimeCommand::DeleteSession { client, .. } => {
                vec![AgentStreamPayload::SessionDeleteFailed {
                    client,
                    message: unsupported.to_owned(),
                }]
            }
            RuntimeCommand::SetConfigOption { option_id, .. } => {
                vec![AgentStreamPayload::SettingFailed {
                    option_id,
                    message: unsupported.to_owned(),
                }]
            }
            RuntimeCommand::SetMode { .. } => vec![AgentStreamPayload::SettingFailed {
                option_id: "legacy-session-mode".to_owned(),
                message: unsupported.to_owned(),
            }],
            RuntimeCommand::Verb { .. }
            | RuntimeCommand::StopTask { .. }
            | RuntimeCommand::Authenticate { .. }
            | RuntimeCommand::Shutdown => Vec::new(),
        }
    }

    fn control(&mut self, control: RuntimeControl) -> Vec<AgentStreamPayload> {
        match control {
            RuntimeControl::Cancel { .. } => {
                self.cancel();
                Vec::new()
            }
            RuntimeControl::RespondPermission {
                request_id,
                option_id,
            } => self.respond(request_id, option_id.as_deref()),
            RuntimeControl::AnswerQuestion { .. } => Vec::new(),
        }
    }
}

enum Input {
    Command(Result<RuntimeCommand, async_channel::RecvError>),
    Control(Result<RuntimeControl, async_channel::RecvError>),
    Tick,
}

pub(crate) async fn run_lens(spec: LensSpec, channels: RuntimeChannels) -> Result<(), String> {
    let RuntimeChannels {
        permission_ids,
        commands,
        controls,
        events,
        ..
    } = channels;
    let mut lens = Lens::new(spec, permission_ids);
    let mut opened = false;
    loop {
        let input = futures_lite::future::or(
            async { Input::Command(commands.recv().await) },
            futures_lite::future::or(async { Input::Control(controls.recv().await) }, async {
                smol::Timer::after(POLL).await;
                Input::Tick
            }),
        )
        .await;
        let payloads = match input {
            Input::Command(Ok(RuntimeCommand::Shutdown) | Err(_)) | Input::Control(Err(_)) => {
                return Ok(());
            }
            Input::Command(Ok(command)) => {
                opened |= matches!(command, RuntimeCommand::Open { .. });
                lens.command(command)
            }
            Input::Control(Ok(control)) => lens.control(control),
            Input::Tick if opened => lens.tick(),
            Input::Tick => Vec::new(),
        };
        for payload in payloads {
            events
                .send(payload)
                .await
                .map_err(|error| error.to_string())?;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use parking_lot::Mutex;

    use super::*;

    #[derive(Default)]
    struct FakePane {
        sent: Mutex<Vec<String>>,
        keys: Mutex<Vec<String>>,
        refuse: bool,
    }

    impl LensPane for FakePane {
        fn pid(&self) -> Option<u32> {
            Some(std::process::id())
        }

        fn submit(&self, text: &str) -> Result<(), String> {
            if self.refuse {
                return Err("text not echoed".to_owned());
            }
            self.sent.lock().push(text.to_owned());
            Ok(())
        }

        fn keys(&self, keys: &[&str]) -> Result<(), String> {
            self.keys
                .lock()
                .extend(keys.iter().map(|key| (*key).to_owned()));
            Ok(())
        }
    }

    struct Claudes {
        root: tempfile::TempDir,
        transcript: PathBuf,
        pane: Arc<FakePane>,
    }

    const SESSION: &str = "9c792c32-4a8f-43cd-8003-e470a8775bc3";

    impl Claudes {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let cwd = root.path().join("work");
            std::fs::create_dir_all(&cwd).unwrap();
            let cwd = std::fs::canonicalize(cwd).unwrap();
            let project = root
                .path()
                .join("config/projects")
                .join(cwd.to_string_lossy().replace(['/', '.'], "-"));
            std::fs::create_dir_all(&project).unwrap();
            std::fs::create_dir_all(root.path().join("registry")).unwrap();
            let transcript = project.join(format!("{SESSION}.jsonl"));
            std::fs::write(&transcript, "").unwrap();
            let claudes = Self {
                root,
                transcript,
                pane: Arc::new(FakePane::default()),
            };
            claudes.record("idle", "");
            claudes
        }

        fn cwd(&self) -> PathBuf {
            std::fs::canonicalize(self.root.path().join("work")).unwrap()
        }

        fn record(&self, status: &str, waiting_for: &str) {
            let pid = std::process::id();
            std::fs::write(
                self.root.path().join(format!("registry/{pid}.json")),
                json!({
                    "pid": pid,
                    "sessionId": SESSION,
                    "cwd": self.cwd(),
                    "tmux": "zz:@1.%7",
                    "messagingSocketPath": "/tmp/cc-socks/none.sock",
                    "updatedAt": SystemTimeMs::now(),
                    "status": status,
                    "waitingFor": waiting_for,
                })
                .to_string(),
            )
            .unwrap();
        }

        fn line(&self, entry: &Value) {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&self.transcript)
                .unwrap();
            writeln!(file, "{entry}").unwrap();
        }

        fn lens(&self) -> Lens {
            let spec = LensSpec {
                pane: PaneId(7),
                provider: AgentProvider::ClaudeCode,
                io: self.pane.clone(),
            };
            Lens::with_source(
                spec,
                Source::Claude(Claude::new(Some(ClaudeFiles {
                    registry: self.root.path().join("registry"),
                    config: self.root.path().join("config"),
                }))),
                Arc::new(AtomicU64::new(1)),
            )
        }
    }

    struct SystemTimeMs;

    impl SystemTimeMs {
        fn now() -> u64 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64
        }
    }

    fn prompt(uuid: &str, parent: Option<&str>, text: &str) -> Value {
        json!({
            "type": "user", "uuid": uuid, "parentUuid": parent, "isSidechain": false,
            "message": { "role": "user", "content": text },
        })
    }

    fn tool_use(uuid: &str, parent: &str, id: &str, command: &str) -> Value {
        json!({
            "type": "assistant", "uuid": uuid, "parentUuid": parent, "isSidechain": false,
            "message": {
                "id": format!("msg_{uuid}"), "role": "assistant",
                "content": [{ "type": "tool_use", "id": id, "name": "Bash", "input": { "command": command } }],
            },
        })
    }

    fn answer(uuid: &str, parent: &str, text: &str) -> Value {
        json!({
            "type": "assistant", "uuid": uuid, "parentUuid": parent, "isSidechain": false,
            "message": {
                "id": format!("msg_{uuid}"), "role": "assistant", "stop_reason": "end_turn",
                "content": [{ "type": "text", "text": text }],
            },
        })
    }

    fn texts(payloads: &[AgentStreamPayload], kind: &str) -> Vec<String> {
        payloads
            .iter()
            .filter_map(|payload| match payload {
                AgentStreamPayload::Update { update } if update["sessionUpdate"] == kind => {
                    update["content"]["text"].as_str().map(str::to_owned)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_lens_replays_a_claude_transcript_then_tails_it() {
        let claudes = Claudes::new();
        claudes.line(&prompt("u1", None, "first question"));
        claudes.line(&answer("a1", "u1", "first answer"));
        let mut lens = claudes.lens();
        let opened = lens.open();
        assert!(matches!(opened[0], AgentStreamPayload::Ready { .. }));
        assert!(matches!(
            opened[1],
            AgentStreamPayload::SessionReset { restoring: true }
        ));
        assert_eq!(texts(&opened, "user_message_chunk"), ["first question"]);
        assert_eq!(texts(&opened, "agent_message_chunk"), ["first answer"]);
        assert!(opened.iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::SessionReady { session_id, .. } if session_id == SESSION
        )));

        claudes.line(&prompt("u2", Some("a1"), "second question"));
        claudes.line(&answer("a2", "u2", "second answer"));
        let tailed = lens.tick();
        assert_eq!(texts(&tailed, "user_message_chunk"), ["second question"]);
        assert_eq!(texts(&tailed, "agent_message_chunk"), ["second answer"]);
        assert!(
            !tailed
                .iter()
                .any(|payload| matches!(payload, AgentStreamPayload::SessionReset { .. }))
        );
    }

    #[test]
    fn a_rewound_prompt_reloads_the_chain_from_its_new_branch() {
        let claudes = Claudes::new();
        claudes.line(&prompt("u1", None, "keep"));
        claudes.line(&answer("a1", "u1", "kept"));
        claudes.line(&prompt("u2", Some("a1"), "dropped"));
        claudes.line(&answer("a2", "u2", "gone"));
        let mut lens = claudes.lens();
        lens.open();
        claudes.line(&prompt("u3", Some("a1"), "instead"));
        let reloaded = lens.tick();
        assert!(reloaded.iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::SessionReset { restoring: true }
        )));
        assert_eq!(texts(&reloaded, "user_message_chunk"), ["keep", "instead"]);
    }

    #[test]
    fn a_lens_prompt_is_pasted_and_settles_when_claude_goes_idle() {
        let claudes = Claudes::new();
        claudes.line(&prompt("u1", None, "hello"));
        let mut lens = claudes.lens();
        lens.open();
        assert!(
            lens.command(RuntimeCommand::Prompt {
                turn_id: 3,
                prompt: AgentPrompt {
                    text: "run the tests".to_owned(),
                    ..AgentPrompt::default()
                },
            })
            .is_empty()
        );
        assert_eq!(*claudes.pane.sent.lock(), ["run the tests"]);
        claudes.record("busy", "");
        let busy = lens.tick();
        assert!(busy.contains(&AgentStreamPayload::Activity { busy: true }));
        claudes.record("idle", "");
        let idle = lens.tick();
        assert!(idle.contains(&AgentStreamPayload::Activity { busy: false }));
        assert!(idle.iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::PromptFinished { turn_id: 3, outcome: AgentPromptOutcome::Finished { stop_reason } }
                if stop_reason == "end_turn"
        )));
    }

    #[test]
    fn a_prompt_the_terminal_never_echoes_fails_its_turn() {
        let mut claudes = Claudes::new();
        claudes.pane = Arc::new(FakePane {
            refuse: true,
            ..FakePane::default()
        });
        let mut lens = claudes.lens();
        lens.open();
        let failed = lens.command(RuntimeCommand::Prompt {
            turn_id: 9,
            prompt: AgentPrompt {
                text: "hi".to_owned(),
                ..AgentPrompt::default()
            },
        });
        assert!(matches!(
            &failed[..],
            [AgentStreamPayload::PromptFinished {
                turn_id: 9,
                outcome: AgentPromptOutcome::Failed { .. }
            }]
        ));
    }

    #[test]
    fn a_claude_permission_prompt_becomes_a_card_answered_with_the_tuis_keys() {
        let claudes = Claudes::new();
        claudes.line(&prompt("u1", None, "touch a file"));
        claudes.line(&tool_use("a1", "u1", "toolu_1", "touch probe.txt"));
        let mut lens = claudes.lens();
        lens.open();
        claudes.record("waiting", PERMISSION_PROMPT);
        let asked = lens.tick();
        let Some((request_id, tool_call, options)) =
            asked.iter().find_map(|payload| match payload {
                AgentStreamPayload::PermissionRequested {
                    request_id,
                    tool_call,
                    options,
                    ..
                } => Some((*request_id, tool_call.clone(), options.clone())),
                _ => None,
            })
        else {
            panic!("{asked:?}");
        };
        assert_eq!(tool_call["toolCallId"], "toolu_1");
        assert_eq!(options[0]["optionId"], ALLOW);
        assert_eq!(options[1]["optionId"], DENY);
        assert!(lens.tick().is_empty(), "one card per prompt");
        let answered = lens.control(RuntimeControl::RespondPermission {
            request_id,
            option_id: Some(ALLOW.to_owned()),
        });
        assert_eq!(
            answered,
            [AgentStreamPayload::PermissionResolved {
                request_id,
                canceled: false
            }]
        );
        assert_eq!(*claudes.pane.keys.lock(), ["1"]);
        claudes.record("busy", "");
        assert!(
            !lens
                .tick()
                .iter()
                .any(|payload| matches!(payload, AgentStreamPayload::PermissionResolved { .. }))
        );
    }

    #[test]
    fn any_other_wait_shows_a_card_only_the_terminal_can_answer() {
        let claudes = Claudes::new();
        claudes.line(&prompt("u1", None, "plan it"));
        let mut lens = claudes.lens();
        lens.open();
        claudes.record("waiting", "plan approval");
        let asked = lens.tick();
        let Some((request_id, options)) = asked.iter().find_map(|payload| match payload {
            AgentStreamPayload::PermissionRequested {
                request_id,
                options,
                ..
            } => Some((*request_id, options.clone())),
            _ => None,
        }) else {
            panic!("{asked:?}");
        };
        assert_eq!(options, json!([]));
        assert!(
            lens.control(RuntimeControl::RespondPermission {
                request_id,
                option_id: Some(ALLOW.to_owned()),
            })
            .is_empty()
        );
        assert!(claudes.pane.keys.lock().is_empty());
        claudes.record("idle", "");
        assert!(
            lens.tick()
                .contains(&AgentStreamPayload::PermissionResolved {
                    request_id,
                    canceled: false
                })
        );
    }

    #[test]
    fn codex_rollout_lines_become_the_timeline_and_mark_the_turn() {
        let mut codex = Codex::default();
        let mut read = Read::default();
        let lines = [
            json!({ "type": "event_msg", "payload": { "type": "task_started", "turn_id": "t1" } }),
            json!({ "type": "event_msg", "payload": { "type": "item_completed", "item": {
                "type": "UserMessage", "id": "m1", "content": [{ "type": "text", "text": "make a file" }],
            } } }),
            json!({ "type": "event_msg", "payload": { "type": "item_completed", "item": {
                "type": "AgentMessage", "id": "m2", "content": [{ "type": "Text", "text": "Done." }],
            } } }),
        ];
        for line in &lines {
            codex.entry(line, &mut read);
        }
        assert!(codex.busy);
        assert!(!read.finished);
        let kinds = read
            .updates
            .iter()
            .map(|update| update["sessionUpdate"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(kinds, ["user_message_chunk", "agent_message_chunk"]);
        codex.entry(
            &json!({ "type": "event_msg", "payload": { "type": "task_complete", "turn_id": "t1" } }),
            &mut read,
        );
        assert!(!codex.busy);
        assert!(read.finished);
    }
}
