use std::{
    fs::File,
    io::{Read as _, Seek as _, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

use serde_json::{Value, json};
use zz_protocol::{
    AgentProvider, AgentQuestionAnswer, MAX_AGENT_PERMISSION_BYTES, PaneId,
    agent_stream::{AgentQuestion, AgentQuestionOption},
    chat::ChatEvent,
};

use crate::agent::{
    chat_bridge::{Shadow, chat_answers},
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
    lens_commands::{self, Listed},
    runtime::{RuntimeCommand, RuntimeControl},
    stream::{
        AgentPrompt, AgentPromptOutcome, AgentSessionCapabilities, AgentStreamPayload, agent_verb,
    },
    turn,
};

const POLL: Duration = Duration::from_millis(250);
const LOCATE: Duration = Duration::from_secs(1);
const CODEX_LOCATE: Duration = Duration::from_secs(5);
const ROLLOUT_DAYS: usize = 7;
const SETTLE: Duration = Duration::from_secs(5);
const IDLE_GRACE: Duration = Duration::from_secs(2);
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
    failed: Option<String>,
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
    subagents: Option<PathBuf>,
    agents: Vec<Subagent>,
    listed: Vec<Listed>,
    listed_changed: bool,
}

struct Subagent {
    tool: String,
    tail: Tail,
    meta: PathBuf,
    seen: bool,
    last: Option<String>,
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
            subagents: None,
            agents: Vec::new(),
            listed: Vec::new(),
            listed_changed: false,
        }
    }

    fn note_listing(&mut self, entry: &Value) {
        let Some(listed) = lens_commands::skill_listing(entry) else {
            return;
        };
        if entry["attachment"]["isInitial"] == false {
            for skill in listed {
                if !self.listed.iter().any(|known| known.name == skill.name) {
                    self.listed.push(skill);
                    self.listed_changed = true;
                }
            }
        } else if self.listed != listed {
            self.listed = listed;
            self.listed_changed = true;
        }
    }

    fn locate(&mut self, pane_pid: Option<u32>) -> Option<Found> {
        let files = self.files.as_ref()?;
        let mut records = claude_peers::read_records_in(&files.registry).ok()?;
        records.retain(|record| record.zz.is_none());
        let record = claude_peers::record_in_pane(&records, pane_pid?)?.clone();
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
        let fresh = std::fs::read(files.registry.join(format!("{pid}.json")))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PeerRecord>(&bytes).ok())
            .filter(|record| record.pid == pid);
        match fresh {
            Some(record) => self.record = Some(record),
            None if crate::pid_alive(pid) => {}
            None => {
                self.record = None;
                return Some(Status::default());
            }
        }
        let record = self.record.as_ref()?;
        Some(Status {
            busy: matches!(record.status.as_str(), "busy" | "waiting"),
            waiting: (record.status == "waiting")
                .then(|| record.waiting_for.clone())
                .filter(|why| !why.is_empty()),
        })
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
        self.subagents = Some(tail.path.with_extension("").join("subagents"));
        self.agents.clear();
        self.listed.clear();
        for line in text
            .lines()
            .filter(|line| line.contains("\"skill_listing\""))
        {
            if let Ok(entry) = serde_json::from_str::<Value>(line) {
                self.note_listing(&entry);
            }
        }
        self.listed_changed = false;
        self.discover();
        let mut updates = Vec::new();
        for entry in sessions::chain_tail(&text) {
            self.track(&entry);
            updates.extend(self.translator.history_entry(&entry));
            updates.extend(self.agent_lines(false));
        }
        updates.extend(self.finished_agents());
        self.tail = Some(tail);
        updates
    }

    fn read(&mut self) -> Read {
        let mut read = Read::default();
        let Some(tail) = self.tail.as_mut() else {
            return read;
        };
        let lines = tail.lines();
        self.discover();
        for entry in lines {
            if read.reload {
                break;
            }
            read.updates.extend(self.agent_lines(true));
            self.entry(&entry, &mut read);
        }
        if !read.reload {
            read.updates.extend(self.agent_lines(true));
            read.updates.extend(self.finished_agents());
        }
        read
    }

    fn discover(&mut self) {
        let Some(dir) = &self.subagents else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            let Some(agent) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_suffix(".meta.json"))
            else {
                continue;
            };
            let transcript = dir.join(format!("{agent}.jsonl"));
            if self
                .agents
                .iter()
                .any(|known| known.tail.path == transcript)
            {
                continue;
            }
            let Some(tool) = std::fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .and_then(|meta| meta["toolUseId"].as_str().map(str::to_owned))
            else {
                continue;
            };
            self.agents.push(Subagent {
                tool,
                tail: Tail {
                    path: transcript,
                    ..Tail::default()
                },
                meta: path,
                seen: false,
                last: None,
            });
        }
    }

    fn agent_lines(&mut self, live: bool) -> Vec<Value> {
        let mut entries = Vec::new();
        for agent in &mut self.agents {
            let due = if agent.seen {
                live && self.translator.running(&agent.tool)
            } else {
                self.translator.knows(&agent.tool)
            };
            if !due {
                continue;
            }
            agent.seen = true;
            for mut entry in agent.tail.lines() {
                if entry["type"] == "assistant"
                    && let Some(text) = claude_translate::prompt_text(&entry)
                {
                    agent.last = Some(text);
                }
                entry["parent_tool_use_id"] = Value::from(agent.tool.as_str());
                entries.push(entry);
            }
        }
        let mut updates = Vec::new();
        for entry in entries {
            self.track(&entry);
            updates.extend(self.translator.sidechain_entry(&entry));
        }
        updates
    }

    fn finished_agents(&mut self) -> Vec<Value> {
        let finished = self
            .agents
            .iter()
            .filter(|agent| agent.seen && self.translator.waiting_on(&agent.tool))
            .filter(|agent| {
                std::fs::read(&agent.meta)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .is_some_and(|meta| meta["finished"] == true)
            })
            .map(|agent| (agent.tool.clone(), agent.last.clone()))
            .collect::<Vec<_>>();
        finished
            .into_iter()
            .flat_map(|(tool, last)| self.translator.agent_finished(&tool, last.as_deref()))
            .collect()
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
        self.note_listing(entry);
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
    ended: Option<Result<(), String>>,
}

impl Codex {
    fn locate(&mut self, pane_pid: Option<u32>) -> Option<Found> {
        self.pid = codex_queue::codex_pid(pane_pid?);
        let pid = self.pid?;
        let (path, meta) = open_files(pid)
            .into_iter()
            .filter(|path| is_rollout(path))
            .filter_map(|path| RolloutMeta::read(&path).map(|meta| (path, meta)))
            .find(|(_, meta)| meta.parent.is_none())
            .or_else(|| newest_rollout(&codex_sessions()?, pid))?;
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
        self.ended = (!self.busy && read.finished).then(|| read.failed.map_or(Ok(()), Err));
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
                read.failed = payload["error"]["message"].as_str().map(codex_error);
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

fn codex_error(message: &str) -> String {
    serde_json::from_str::<Value>(message)
        .ok()
        .and_then(|error| error["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| message.to_owned())
}

fn is_rollout(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "jsonl")
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("rollout-"))
}

fn codex_sessions() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
        .map(|home| home.join("sessions"))
}

fn subdirectories(path: &Path) -> Vec<PathBuf> {
    let mut directories = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    directories.sort();
    directories
}

fn newest_rollout(sessions: &Path, pid: u32) -> Option<(PathBuf, RolloutMeta)> {
    let cwd =
        std::fs::canonicalize(zz_daemon_client::process_info::working_directory(pid)?).ok()?;
    let since = SystemTime::now().checked_sub(zz_daemon_client::process_info::running_for(pid)?)?;
    let mut days = subdirectories(sessions)
        .iter()
        .flat_map(|year| subdirectories(year))
        .flat_map(|month| subdirectories(&month))
        .collect::<Vec<_>>();
    days.sort();
    days.into_iter()
        .rev()
        .take(ROLLOUT_DAYS)
        .flat_map(|day| std::fs::read_dir(day).into_iter().flatten().flatten())
        .map(|entry| entry.path())
        .filter(|path| is_rollout(path))
        .filter_map(|path| Some((std::fs::metadata(&path).ok()?.modified().ok()?, path)))
        .filter(|(modified, _)| *modified >= since)
        .filter_map(|(modified, path)| RolloutMeta::read(&path).map(|meta| (modified, path, meta)))
        .filter(|(_, _, meta)| {
            meta.parent.is_none()
                && std::fs::canonicalize(&meta.cwd).is_ok_and(|session| session == cwd)
        })
        .max_by_key(|(modified, _, _)| *modified)
        .map(|(_, path, meta)| (path, meta))
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
    Claude(Box<Claude>),
    Codex(Box<Codex>),
}

impl Source {
    fn locate(&mut self, pane_pid: Option<u32>) -> Option<Found> {
        match self {
            Self::Claude(claude) => claude.locate(pane_pid),
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

    fn take_ended(&mut self) -> Option<Result<(), String>> {
        match self {
            Self::Claude(_) => None,
            Self::Codex(codex) => codex.ended.take(),
        }
    }

    fn pending_tool(&self) -> Option<&(String, String, Value)> {
        match self {
            Self::Claude(claude) => claude.open.last(),
            Self::Codex(_) => None,
        }
    }

    fn pending_question(&self) -> Option<(String, AgentQuestion)> {
        let Self::Claude(claude) = self else {
            return None;
        };
        let (id, _, input) = claude
            .open
            .iter()
            .rev()
            .find(|(_, name, _)| name == "AskUserQuestion")?;
        Some((id.clone(), pickable(input)?))
    }

    const fn every(&self) -> Duration {
        match self {
            Self::Claude(_) => LOCATE,
            Self::Codex(_) => CODEX_LOCATE,
        }
    }

    fn listed(&self) -> &[Listed] {
        match self {
            Self::Claude(claude) => &claude.listed,
            Self::Codex(_) => &[],
        }
    }

    fn take_listing_change(&mut self) -> bool {
        match self {
            Self::Claude(claude) => std::mem::take(&mut claude.listed_changed),
            Self::Codex(_) => false,
        }
    }
}

struct Turn {
    id: u64,
    sent: Instant,
    busy: bool,
    idle_since: Option<Instant>,
    cancelled: bool,
}

struct Permission {
    request_id: u64,
    tool: Option<String>,
    answerable: bool,
    answered: bool,
    choices: Vec<String>,
}

struct Lens {
    spec: LensSpec,
    source: Source,
    permission_ids: Arc<AtomicU64>,
    session: Option<String>,
    cwd: PathBuf,
    busy: bool,
    turn: Option<Turn>,
    failed: Option<String>,
    permission: Option<Permission>,
    next_locate: Instant,
}

fn pickable(input: &Value) -> Option<AgentQuestion> {
    let [question] = input["questions"].as_array()?.as_slice() else {
        return None;
    };
    let options = question["options"].as_array()?;
    if question["multiSelect"] == true || options.is_empty() || options.len() > 9 {
        return None;
    }
    Some(AgentQuestion {
        id: "q0".to_owned(),
        header: question["header"].as_str().map(str::to_owned),
        question: question["question"].as_str()?.to_owned(),
        options: options
            .iter()
            .map(|option| {
                Some(AgentQuestionOption {
                    label: option["label"].as_str()?.to_owned(),
                    description: option["description"].as_str().map(str::to_owned),
                })
            })
            .collect::<Option<_>>()?,
        ..AgentQuestion::default()
    })
}

fn waiting_title(why: &str) -> String {
    match why {
        "input needed" | "dialog open" => "Claude Code needs your input in the terminal".to_owned(),
        why => format!("Waiting for a {why} in the terminal"),
    }
}

fn option(id: &str, name: &str, kind: &str) -> Value {
    json!({ "optionId": id, "name": name, "kind": kind })
}

impl Lens {
    fn new(spec: LensSpec, permission_ids: Arc<AtomicU64>) -> Self {
        let source = match spec.provider {
            AgentProvider::ClaudeCode => {
                Source::Claude(Box::new(Claude::new(ClaudeFiles::from_environment())))
            }
            AgentProvider::Codex => Source::Codex(Box::default()),
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
            failed: None,
            permission: None,
            next_locate: Instant::now(),
        }
    }

    fn placeholder(&self) -> String {
        format!("lens-{}", self.spec.pane)
    }

    fn commands(&self) -> AgentStreamPayload {
        AgentStreamPayload::Update {
            update: lens_commands::update(
                self.spec.provider,
                &self.cwd,
                std::env::home_dir().as_deref(),
                self.source.listed(),
            ),
        }
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
        payloads.extend(self.observe(false, Vec::new()));
        payloads
    }

    fn locate(&mut self, opening: bool) -> Vec<AgentStreamPayload> {
        self.next_locate = Instant::now() + self.source.every();
        let found = self.source.locate(self.spec.io.pid());
        let Some(found) = found else {
            return if opening {
                vec![
                    AgentStreamPayload::SessionReset { restoring: false },
                    AgentStreamPayload::SessionReady {
                        session_id: self.placeholder(),
                        modes: None,
                        config_options: None,
                    },
                    self.commands(),
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
        let mut payloads = self.load(found, opening);
        self.source.take_listing_change();
        payloads.push(self.commands());
        payloads
    }

    fn load(&mut self, found: Found, opening: bool) -> Vec<AgentStreamPayload> {
        let replay = self.source.load(found.path, &found.cwd);
        let ended = self.source.take_ended();
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
            let mut payloads = updates.collect::<Vec<_>>();
            if let Some(ended) = ended {
                self.failed = ended.err();
                payloads.extend(self.settle(false));
                self.failed = None;
            }
            return payloads;
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
        let mut payloads = if Instant::now() >= self.next_locate
            || (self.turn.is_some() && self.session.is_none())
        {
            self.locate(false)
        } else {
            Vec::new()
        };
        let read = self.source.read();
        if read.reload {
            if let Some(found) = self.source.locate(self.spec.io.pid()) {
                self.session = Some(String::new());
                payloads.extend(self.reload(found, false));
            }
            return payloads;
        }
        self.failed = read.failed;
        payloads.extend(self.observe(read.finished, read.updates));
        self.failed = None;
        if self.source.take_listing_change() {
            payloads.push(self.commands());
        }
        payloads
    }

    fn observe(&mut self, finished: bool, updates: Vec<Value>) -> Vec<AgentStreamPayload> {
        let updates = updates
            .into_iter()
            .map(|update| AgentStreamPayload::Update { update });
        let Some(status) = self.source.status() else {
            return updates.collect();
        };
        let mut payloads = Vec::new();
        if status.busy && !self.busy {
            self.busy = true;
            payloads.push(AgentStreamPayload::Activity { busy: true });
        }
        payloads.extend(updates);
        payloads.extend(self.follow_permission(status.waiting.as_deref()));
        if self.busy && !status.busy {
            self.busy = false;
            payloads.push(AgentStreamPayload::Activity { busy: false });
        } else if finished && !status.busy && self.turn.is_none() {
            payloads.push(AgentStreamPayload::Activity { busy: false });
        }
        let settled = self.turn.as_mut().is_some_and(|turn| {
            turn.busy |= status.busy;
            if status.busy {
                turn.idle_since = None;
                return finished;
            }
            let idle = *turn.idle_since.get_or_insert_with(Instant::now);
            finished || (turn.busy && idle.elapsed() >= IDLE_GRACE) || turn.sent.elapsed() >= SETTLE
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
        let answerable = why == PERMISSION_PROMPT;
        let question = (!answerable)
            .then(|| self.source.pending_question())
            .flatten();
        let tool = question.as_ref().map(|(id, _)| id.clone()).or(tool);
        if let Some(permission) = &self.permission {
            let upgrade = (answerable || question.is_some())
                && !permission.answered
                && permission.tool.is_none()
                && tool.is_some();
            let same = !permission.answered || permission.tool.is_none() || permission.tool == tool;
            if !upgrade && same {
                return payloads;
            }
            if upgrade {
                payloads.push(AgentStreamPayload::PermissionResolved {
                    request_id: permission.request_id,
                    canceled: true,
                });
            }
            self.permission = None;
        }
        let request_id = self.permission_ids.fetch_add(1, Ordering::Relaxed);
        if let Some((id, question)) = question {
            self.permission = Some(Permission {
                request_id,
                tool: Some(id.clone()),
                answerable: true,
                answered: false,
                choices: question
                    .options
                    .iter()
                    .map(|option| option.label.clone())
                    .collect(),
            });
            payloads.push(AgentStreamPayload::PermissionRequested {
                request_id,
                tool_call: json!({
                    "toolCallId": id,
                    "title": question.question,
                    "kind": "other",
                }),
                options: json!([]),
                questions: vec![question],
            });
            return payloads;
        }
        let pending = self.source.pending_tool().filter(|_| answerable);
        let answers = json!([
            option(ALLOW, "Allow", "allow_once"),
            option(DENY, "Deny", "reject_once"),
        ]);
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
            (call, answers)
        } else if answerable {
            (
                json!({
                    "toolCallId": format!("lens-permission-{request_id}"),
                    "title": "Claude Code asks for permission; the terminal shows what for",
                    "kind": "other",
                }),
                answers,
            )
        } else {
            (
                json!({
                    "toolCallId": format!("lens-wait-{request_id}"),
                    "title": waiting_title(why),
                    "kind": "other",
                }),
                json!([]),
            )
        };
        self.permission = Some(Permission {
            request_id,
            tool: pending.map(|(id, _, _)| id.clone()),
            answerable,
            answered: false,
            choices: Vec::new(),
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
        if let Some(message) = self.failed.take() {
            return Some(AgentStreamPayload::PromptFinished {
                turn_id: turn.id,
                outcome: AgentPromptOutcome::Failed { message },
            });
        }
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
                    idle_since: None,
                    cancelled: false,
                });
                Vec::new()
            }
            Err(message) => failed(&message),
        }
    }

    fn steer(&mut self, prompt: AgentPrompt) -> Vec<AgentStreamPayload> {
        let Some(("steer", text)) = agent_verb(&prompt.text) else {
            return Vec::new();
        };
        if text.is_empty() {
            return Vec::new();
        }
        match self.spec.io.submit(text) {
            Ok(()) => Vec::new(),
            Err(error) => {
                log::debug!(target: "zz::agent", "could not steer {}: {error}", self.spec.pane);
                vec![AgentStreamPayload::PromptsReclaimed {
                    prompts: vec![AgentPrompt {
                        text: text.to_owned(),
                        ..prompt
                    }],
                }]
            }
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

    fn answer(
        &mut self,
        request_id: u64,
        answers: &[AgentQuestionAnswer],
    ) -> Vec<AgentStreamPayload> {
        let Some(permission) = self
            .permission
            .as_mut()
            .filter(|permission| permission.request_id == request_id && !permission.answered)
        else {
            return Vec::new();
        };
        let Some(index) = answers
            .first()
            .and_then(|answer| answer.answers.first())
            .and_then(|label| permission.choices.iter().position(|choice| choice == label))
        else {
            return Vec::new();
        };
        let key = (index + 1).to_string();
        match self.spec.io.keys(&[key.as_str()]) {
            Ok(()) => {
                permission.answered = true;
                vec![AgentStreamPayload::PermissionResolved {
                    request_id,
                    canceled: false,
                }]
            }
            Err(error) => {
                log::debug!(target: "zz::agent", "could not answer {}: {error}", self.spec.pane);
                Vec::new()
            }
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
        if !permission.answerable {
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
            RuntimeCommand::Verb { prompt } => self.steer(prompt),
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
            RuntimeCommand::StopTask { .. }
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
            RuntimeControl::AnswerQuestion {
                request_id,
                answers,
            } => self.answer(request_id, &answers),
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
    let mut shadow = match spec.provider {
        AgentProvider::ClaudeCode => Shadow::default().attached(),
        AgentProvider::Codex => Shadow::default().codex().attached(),
    };
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
            Input::Control(Ok(control)) => {
                let answer = match &control {
                    RuntimeControl::AnswerQuestion {
                        request_id,
                        answers,
                    } => Some((*request_id, chat_answers(answers))),
                    _ => None,
                };
                let payloads = lens.control(control);
                if let Some((request, answer)) = answer.filter(|_| !payloads.is_empty()) {
                    shadow.answered(request, answer);
                }
                payloads
            }
            Input::Tick if opened => lens.tick(),
            Input::Tick => Vec::new(),
        };
        let chat = shadow_lens(&mut shadow, &payloads);
        for payload in payloads.into_iter().chain(
            chat.into_iter()
                .map(|event| AgentStreamPayload::Chat { event }),
        ) {
            events
                .send(payload)
                .await
                .map_err(|error| error.to_string())?;
        }
    }
}

fn shadow_lens(shadow: &mut Shadow, payloads: &[AgentStreamPayload]) -> Vec<ChatEvent> {
    let now = Instant::now();
    let ready = payloads.iter().find_map(|payload| match payload {
        AgentStreamPayload::SessionReady { session_id, .. } => Some(session_id.clone()),
        _ => None,
    });
    for payload in payloads {
        match payload {
            AgentStreamPayload::SessionReset { .. } => {
                shadow.turn(&turn::Fact::SessionClosed, None, now);
                shadow.payload(payload);
            }
            AgentStreamPayload::SessionReady { .. } => {}
            AgentStreamPayload::Activity { busy: true } => {
                shadow.turn(&turn::Fact::Busy, None, now);
            }
            AgentStreamPayload::Activity { busy: false }
            | AgentStreamPayload::PromptFinished { .. } => {
                shadow.turn(&turn::Fact::Idle, None, now);
            }
            _ => shadow.payload(payload),
        }
    }
    if let Some(session) = ready {
        shadow.turn(&turn::Fact::SessionReady, Some(&session), now);
    }
    shadow.drain()
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
                Source::Claude(Box::new(Claude::new(Some(ClaudeFiles {
                    registry: self.root.path().join("registry"),
                    config: self.root.path().join("config"),
                })))),
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

    fn queued(uuid: &str, parent: &str, origin: &str, text: &str) -> Value {
        json!({
            "type": "attachment", "uuid": uuid, "parentUuid": parent, "isSidechain": false,
            "attachment": {
                "type": "queued_command", "prompt": text, "commandMode": "prompt",
                "origin": { "kind": origin }, "humanTurn": origin == "human",
            },
        })
    }

    fn turn_duration(uuid: &str, parent: &str) -> Value {
        json!({
            "type": "system", "subtype": "turn_duration", "uuid": uuid, "parentUuid": parent,
            "isSidechain": false, "durationMs": 1200,
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
    fn the_lens_feeds_the_chat_machine_a_clean_chat() {
        use zz_protocol::chat::{ChatPhase, ChatView};
        let claudes = Claudes::new();
        claudes.line(&prompt("u1", None, "first question"));
        claudes.line(&answer("a1", "u1", "first answer"));
        let mut lens = claudes.lens();
        let mut shadow = Shadow::recording();
        shadow_lens(&mut shadow, &lens.open());
        claudes.record("busy", "");
        shadow_lens(&mut shadow, &lens.tick());
        claudes.line(&prompt("u2", Some("a1"), "second question"));
        claudes.line(&answer("a2", "u2", "second answer"));
        shadow_lens(&mut shadow, &lens.tick());
        let mut view = ChatView::default();
        for event in &shadow.events {
            view.apply(event)
                .unwrap_or_else(|skip| panic!("skipped {event:?}: {skip:?}"));
        }
        assert_eq!(view.phase(), ChatPhase::Running);
        assert_eq!(view.turns.len(), 1);
        let texts = view
            .items
            .values()
            .map(|item| item.body.text.clone())
            .collect::<std::collections::BTreeSet<_>>();
        for text in [
            "first question",
            "first answer",
            "second question",
            "second answer",
        ] {
            assert!(texts.contains(text), "{text} missing from {texts:?}");
        }
        claudes.record("idle", "");
        shadow_lens(&mut shadow, &lens.tick());
        let mut view = ChatView::default();
        for event in &shadow.events {
            view.apply(event).expect("valid");
        }
        assert_eq!(view.phase(), ChatPhase::Idle);
        assert!(view.open_items().next().is_none());
    }

    #[test]
    fn a_background_subagent_nests_its_own_transcript_and_ends_on_its_notification() {
        use zz_protocol::chat::{ChatView, ItemId, ItemStatus};
        let claudes = Claudes::new();
        claudes.line(&prompt("u1", None, "look around"));
        claudes.line(&json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1", "isSidechain": false,
            "message": { "id": "msg_a1", "role": "assistant", "content": [{ "type": "tool_use", "id": "g1",
                "name": "Agent", "input": { "description": "Find it", "prompt": "find it", "subagent_type": "Explore" } }] },
        }));
        claudes.line(&json!({
            "type": "user", "uuid": "u2", "parentUuid": "a1", "isSidechain": false,
            "toolUseResult": { "isAsync": true, "status": "async_launched", "agentId": "a9" },
            "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": "g1",
                "content": [{ "type": "text", "text": "Async agent launched successfully." }] }] },
        }));
        let agents = claudes.transcript.with_extension("").join("subagents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(agents.join("agent-a9.meta.json"), r#"{"toolUseId":"g1"}"#).unwrap();
        let sidechain = |entry: Value| format!("{entry}\n");
        std::fs::write(
            agents.join("agent-a9.jsonl"),
            [
                sidechain(json!({ "type": "user", "uuid": "s1", "isSidechain": true,
                    "message": { "role": "user", "content": "find it" } })),
                sidechain(tool_use("s2", "s1", "r1", "rg ChatMachine")),
            ]
            .concat(),
        )
        .unwrap();
        let mut lens = claudes.lens();
        let mut shadow = Shadow::recording().attached();
        shadow_lens(&mut shadow, &lens.open());
        claudes.record("busy", "");
        shadow_lens(&mut shadow, &lens.tick());
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(agents.join("agent-a9.jsonl"))
            .unwrap();
        writeln!(
            file,
            "{}",
            json!({ "type": "user", "uuid": "s3", "isSidechain": true, "message": { "role": "user",
                "content": [{ "type": "tool_result", "tool_use_id": "r1", "content": "src/chat.rs" }] } })
        )
        .unwrap();
        writeln!(file, "{}", answer("s4", "s3", "src/chat.rs:148")).unwrap();
        claudes.line(&json!({
            "type": "user", "uuid": "u3", "parentUuid": "u2", "isSidechain": false,
            "message": { "role": "user", "content": "<task-notification>\n<task-id>a9</task-id>\n<tool-use-id>g1</tool-use-id>\n<status>completed</status>\n<result>src/chat.rs:148</result>\n</task-notification>" },
        }));
        shadow_lens(&mut shadow, &lens.tick());
        let mut view = ChatView::default();
        for event in &shadow.events {
            view.apply(event)
                .unwrap_or_else(|skip| panic!("skipped {event:?}: {skip:?}"));
        }
        let agent = view.item(&ItemId::new("g1")).expect("the subagent row");
        assert_eq!(agent.status, ItemStatus::Completed);
        assert_eq!(agent.body.text, "src/chat.rs:148");
        let read = view
            .item(&ItemId::new("r1"))
            .expect("the subagent's own tool");
        assert_eq!(read.parent, Some(ItemId::new("g1")));
        assert_eq!(read.status, ItemStatus::Completed);
        assert!(
            agent.children.iter().any(|child| view
                .item(child)
                .is_some_and(|item| item.body.text == "src/chat.rs:148")),
            "{:?}",
            agent.children
        );
        assert!(
            !view
                .items
                .values()
                .any(|item| item.body.text.contains("find it") && item.parent.is_none()),
            "the subagent prompt is not a prompt of yours"
        );
    }

    #[test]
    fn a_background_subagent_whose_notification_never_came_ends_when_its_meta_says_finished() {
        use zz_protocol::chat::{ChatView, ItemId, ItemStatus};
        let claudes = Claudes::new();
        let mut lens = claudes.lens();
        let mut shadow = Shadow::recording().attached();
        shadow_lens(&mut shadow, &lens.open());
        claudes.record("busy", "");
        claudes.line(&prompt("u1", None, "look around"));
        claudes.line(&json!({
            "type": "assistant", "uuid": "a1", "parentUuid": "u1", "isSidechain": false,
            "message": { "id": "msg_a1", "role": "assistant", "content": [{ "type": "tool_use", "id": "g1",
                "name": "Agent", "input": { "description": "Find it", "prompt": "find it" } }] },
        }));
        claudes.line(&json!({
            "type": "user", "uuid": "u2", "parentUuid": "a1", "isSidechain": false,
            "toolUseResult": { "isAsync": true, "status": "async_launched", "agentId": "a9" },
            "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": "g1",
                "content": [{ "type": "text", "text": "Async agent launched successfully." }] }] },
        }));
        let agents = claudes.transcript.with_extension("").join("subagents");
        std::fs::create_dir_all(&agents).unwrap();
        let meta = agents.join("agent-a9.meta.json");
        std::fs::write(&meta, r#"{"toolUseId":"g1","finished":false}"#).unwrap();
        std::fs::write(
            agents.join("agent-a9.jsonl"),
            format!("{}\n", answer("s1", "s0", "src/chat.rs:148")),
        )
        .unwrap();
        shadow_lens(&mut shadow, &lens.tick());
        let open = |shadow: &Shadow| {
            let mut view = ChatView::default();
            for event in &shadow.events {
                view.apply(event)
                    .unwrap_or_else(|skip| panic!("skipped {event:?}: {skip:?}"));
            }
            view
        };
        assert!(
            open(&shadow)
                .item(&ItemId::new("g1"))
                .is_some_and(|agent| agent.status.is_open())
        );
        std::fs::write(&meta, r#"{"toolUseId":"g1","finished":true}"#).unwrap();
        shadow_lens(&mut shadow, &lens.tick());
        let view = open(&shadow);
        let agent = view.item(&ItemId::new("g1")).expect("the subagent row");
        assert_eq!(agent.status, ItemStatus::Completed);
        assert_eq!(agent.body.text, "src/chat.rs:148");
    }

    #[test]
    fn a_prompt_typed_in_the_terminal_opens_its_turn_before_claude_says_busy() {
        use zz_protocol::chat::{ChatPhase, ChatView, ItemId};
        let claudes = Claudes::new();
        let mut lens = claudes.lens();
        let mut shadow = Shadow::recording().attached();
        shadow_lens(&mut shadow, &lens.open());
        claudes.line(&prompt("u1", None, "look around"));
        claudes.line(&answer("a1", "u1", "looked"));
        claudes.line(&turn_duration("d1", "a1"));
        shadow_lens(&mut shadow, &lens.tick());
        let mut view = ChatView::default();
        for event in &shadow.events {
            view.apply(event)
                .unwrap_or_else(|skip| panic!("skipped {event:?}: {skip:?}"));
        }
        assert!(view.background.is_empty(), "{:?}", view.background);
        assert_eq!(view.turns.len(), 1);
        assert_eq!(
            view.turns[0].items,
            [ItemId::new("u:u1"), ItemId::new("a:msg_a1#1")]
        );
        assert_eq!(view.phase(), ChatPhase::Idle);
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
        assert!(
            !idle
                .iter()
                .any(|payload| matches!(payload, AgentStreamPayload::PromptFinished { .. })),
            "idle before the transcript lands keeps the turn open"
        );
        claudes.line(&answer("a1", "u1", "all green"));
        claudes.line(&turn_duration("d1", "a1"));
        let done = lens.tick();
        assert!(texts(&done, "agent_message_chunk").contains(&"all green".to_owned()));
        assert!(done.iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::PromptFinished { turn_id: 3, outcome: AgentPromptOutcome::Finished { stop_reason } }
                if stop_reason == "end_turn"
        )));
    }

    fn verb(text: &str) -> RuntimeCommand {
        RuntimeCommand::Verb {
            prompt: AgentPrompt {
                text: text.to_owned(),
                ..AgentPrompt::default()
            },
        }
    }

    #[test]
    fn a_steer_types_into_the_running_turn_and_lands_as_your_message() {
        let claudes = Claudes::new();
        claudes.line(&prompt("u1", None, "run the tests"));
        let mut lens = claudes.lens();
        lens.open();
        lens.command(RuntimeCommand::Prompt {
            turn_id: 3,
            prompt: AgentPrompt {
                text: "run the tests".to_owned(),
                ..AgentPrompt::default()
            },
        });
        claudes.record("busy", "");
        lens.tick();
        assert!(lens.command(verb("/steer only the fast ones")).is_empty());
        assert_eq!(
            *claudes.pane.sent.lock(),
            ["run the tests", "only the fast ones"]
        );
        claudes.line(&tool_use("a1", "u1", "toolu_1", "cargo test"));
        claudes.line(&queued("q1", "a1", "human", "only the fast ones"));
        claudes.line(&queued("q2", "q1", "peer", "a peer said hi"));
        let absorbed = lens.tick();
        assert_eq!(
            texts(&absorbed, "user_message_chunk"),
            ["only the fast ones"]
        );
        assert!(
            !absorbed.iter().any(|payload| matches!(
                payload,
                AgentStreamPayload::PromptFinished { .. } | AgentStreamPayload::SessionReset { .. }
            )),
            "{absorbed:?}"
        );
        claudes.record("idle", "");
        claudes.line(&answer("a2", "q2", "the fast ones pass"));
        claudes.line(&turn_duration("d1", "a2"));
        assert!(lens.tick().iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::PromptFinished { turn_id: 3, outcome: AgentPromptOutcome::Finished { stop_reason } }
                if stop_reason == "end_turn"
        )));
    }

    #[test]
    fn a_steer_the_terminal_refuses_goes_back_to_the_composer() {
        let mut claudes = Claudes::new();
        claudes.pane = Arc::new(FakePane {
            refuse: true,
            ..FakePane::default()
        });
        let mut lens = claudes.lens();
        lens.open();
        let refused = lens.command(verb("/steer stop there"));
        assert!(
            matches!(
                &refused[..],
                [AgentStreamPayload::PromptsReclaimed { prompts }] if prompts[0].text == "stop there"
            ),
            "{refused:?}"
        );
        assert!(lens.turn.is_none());
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
    fn a_record_caught_mid_write_keeps_the_turn_running() {
        let claudes = Claudes::new();
        let mut lens = claudes.lens();
        lens.open();
        lens.command(RuntimeCommand::Prompt {
            turn_id: 3,
            prompt: AgentPrompt {
                text: "hello".to_owned(),
                ..AgentPrompt::default()
            },
        });
        claudes.record("busy", "");
        assert!(
            lens.tick()
                .contains(&AgentStreamPayload::Activity { busy: true })
        );
        let pid = std::process::id();
        std::fs::write(
            claudes.root.path().join(format!("registry/{pid}.json")),
            "{\"pid\"",
        )
        .unwrap();
        let torn = lens.tick();
        assert!(
            !torn
                .iter()
                .any(|payload| matches!(payload, AgentStreamPayload::PromptFinished { .. })),
            "{torn:?}"
        );
        claudes.record("idle", "");
        claudes.line(&turn_duration("d1", "u1"));
        assert!(lens.tick().iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::PromptFinished { turn_id: 3, .. }
        )));
    }

    #[test]
    fn a_permission_prompt_answers_before_its_tool_call_and_upgrades_once_it_shows() {
        let claudes = Claudes::new();
        claudes.line(&prompt("u1", None, "touch a file"));
        let mut lens = claudes.lens();
        lens.open();
        claudes.record("waiting", PERMISSION_PROMPT);
        let early = lens.tick();
        let Some(placeholder) = early.iter().find_map(|payload| match payload {
            AgentStreamPayload::PermissionRequested {
                request_id,
                tool_call,
                options,
                ..
            } if tool_call["toolCallId"] == format!("lens-permission-{request_id}")
                && options[0]["optionId"] == ALLOW =>
            {
                Some(*request_id)
            }
            _ => None,
        }) else {
            panic!("{early:?}");
        };
        claudes.line(&tool_use("a1", "u1", "toolu_1", "touch probe.txt"));
        let upgraded = lens.tick();
        assert!(upgraded.contains(&AgentStreamPayload::PermissionResolved {
            request_id: placeholder,
            canceled: true,
        }));
        assert!(upgraded.iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::PermissionRequested { tool_call, .. }
                if tool_call["toolCallId"] == "toolu_1"
        )));
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

    fn ask(uuid: &str, parent: &str, id: &str, multi: bool) -> Value {
        json!({
            "type": "assistant", "uuid": uuid, "parentUuid": parent, "isSidechain": false,
            "message": { "id": format!("msg_{uuid}"), "role": "assistant", "content": [{
                "type": "tool_use", "id": id, "name": "AskUserQuestion", "input": { "questions": [{
                    "question": "Which file?", "header": "Open first", "multiSelect": multi,
                    "options": [{ "label": "chat.rs" }, { "label": "lib.rs" }, { "label": "mod.rs" }],
                }] },
            }] },
        })
    }

    #[test]
    fn a_single_choice_question_shows_inline_and_answers_with_its_digit() {
        let claudes = Claudes::new();
        claudes.line(&prompt("u1", None, "ask me"));
        claudes.line(&ask("a1", "u1", "q1", false));
        let mut lens = claudes.lens();
        lens.open();
        claudes.record("waiting", "input needed");
        let asked = lens.tick();
        let Some((request_id, tool_call, questions)) =
            asked.iter().find_map(|payload| match payload {
                AgentStreamPayload::PermissionRequested {
                    request_id,
                    tool_call,
                    questions,
                    ..
                } => Some((*request_id, tool_call.clone(), questions.clone())),
                _ => None,
            })
        else {
            panic!("{asked:?}");
        };
        assert_eq!(tool_call["toolCallId"], "q1");
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].options.len(), 3);
        let answered = lens.control(RuntimeControl::AnswerQuestion {
            request_id,
            answers: vec![AgentQuestionAnswer {
                id: questions[0].id.clone(),
                answers: vec!["lib.rs".to_owned()],
            }],
        });
        assert_eq!(*claudes.pane.keys.lock(), ["2"]);
        assert_eq!(
            answered,
            [AgentStreamPayload::PermissionResolved {
                request_id,
                canceled: false
            }]
        );
    }

    #[test]
    fn a_multi_select_question_stays_in_the_terminal() {
        let claudes = Claudes::new();
        claudes.line(&prompt("u1", None, "ask me"));
        claudes.line(&ask("a1", "u1", "q1", true));
        let mut lens = claudes.lens();
        lens.open();
        claudes.record("waiting", "input needed");
        let asked = lens.tick();
        let Some((tool_call, questions)) = asked.iter().find_map(|payload| match payload {
            AgentStreamPayload::PermissionRequested {
                tool_call,
                questions,
                ..
            } => Some((tool_call.clone(), questions.clone())),
            _ => None,
        }) else {
            panic!("{asked:?}");
        };
        assert!(questions.is_empty());
        assert_eq!(
            tool_call["title"],
            "Claude Code needs your input in the terminal"
        );
    }

    #[test]
    fn a_codex_whose_app_server_holds_the_rollout_finds_its_newest_top_level_one() {
        let sessions = tempfile::tempdir().unwrap();
        let cwd = std::env::current_dir().unwrap();
        let rollout = |day: &str, name: &str, cwd: &Path, parent: Option<&str>| {
            let directory = sessions.path().join(day);
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join(format!("rollout-{name}.jsonl"));
            let meta = json!({ "type": "session_meta", "payload": {
                "id": name, "cwd": cwd, "parent_thread_id": parent,
            } });
            std::fs::write(&path, format!("{meta}\n")).unwrap();
            std::thread::sleep(Duration::from_millis(20));
            path
        };
        rollout("2026/10/22", "older", &cwd, None);
        let newest = rollout("2026/10/23", "mine", &cwd, None);
        rollout("2026/10/23", "child", &cwd, Some("mine"));
        rollout("2026/10/23", "elsewhere", Path::new("/"), None);
        let (path, meta) = newest_rollout(sessions.path(), std::process::id()).unwrap();
        assert_eq!(path, newest);
        assert_eq!(meta.thread, "mine");
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
        assert_eq!(read.failed, None);
    }

    #[test]
    fn a_codex_turn_that_ends_in_an_api_error_fails() {
        let root = tempfile::tempdir().unwrap();
        let rollout = root.path().join("rollout-2026-10-24T10-09-22-test.jsonl");
        std::fs::write(&rollout, "").unwrap();
        let pane = Arc::new(FakePane::default());
        let mut codex = Codex::default();
        codex.load(rollout.clone(), root.path());
        let mut lens = Lens::with_source(
            LensSpec {
                pane: PaneId(7),
                provider: AgentProvider::Codex,
                io: pane.clone(),
            },
            Source::Codex(Box::new(codex)),
            Arc::new(AtomicU64::new(1)),
        );
        lens.next_locate = Instant::now() + Duration::from_mins(1);
        assert!(
            lens.command(RuntimeCommand::Prompt {
                turn_id: 4,
                prompt: AgentPrompt {
                    text: "Reply with pong".to_owned(),
                    ..AgentPrompt::default()
                },
            })
            .is_empty()
        );
        let error = json!({"type": "error", "status": 400, "error": {
            "type": "invalid_request_error",
            "message": "The 'gpt-zz' model is not supported when using Codex with a ChatGPT account.",
        }});
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&rollout)
            .unwrap();
        for line in [
            json!({ "type": "event_msg", "payload": { "type": "task_started", "turn_id": "t1" } }),
            json!({ "type": "event_msg", "payload": { "type": "task_complete", "turn_id": "t1",
                "last_agent_message": null, "error": { "message": error.to_string() } } }),
        ] {
            writeln!(file, "{line}").unwrap();
        }
        let done = lens.tick();
        assert!(done.iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::PromptFinished { turn_id: 4, outcome: AgentPromptOutcome::Failed { message } }
                if message == "The 'gpt-zz' model is not supported when using Codex with a ChatGPT account."
        )), "{done:?}");
    }

    #[test]
    fn a_codex_rollout_found_after_its_turn_failed_still_fails_the_turn() {
        let root = tempfile::tempdir().unwrap();
        let mut lens = Lens::with_source(
            LensSpec {
                pane: PaneId(7),
                provider: AgentProvider::Codex,
                io: Arc::new(FakePane::default()),
            },
            Source::Codex(Box::default()),
            Arc::new(AtomicU64::new(1)),
        );
        lens.command(RuntimeCommand::Prompt {
            turn_id: 5,
            prompt: AgentPrompt {
                text: "Reply with pong".to_owned(),
                ..AgentPrompt::default()
            },
        });
        let rollout = root.path().join("rollout-2026-10-24T10-09-22-late.jsonl");
        let lines = [
            json!({ "type": "event_msg", "payload": { "type": "task_started", "turn_id": "t1" } }),
            json!({ "type": "event_msg", "payload": { "type": "task_complete", "turn_id": "t1",
                "error": { "message": "stream disconnected" } } }),
        ];
        let mut file = std::fs::File::create(&rollout).unwrap();
        for line in &lines {
            writeln!(file, "{line}").unwrap();
        }
        let found = lens.reload(
            Found {
                session: "thread-1".to_owned(),
                path: rollout,
                cwd: root.path().to_path_buf(),
            },
            false,
        );
        assert!(found.iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::PromptFinished { turn_id: 5, outcome: AgentPromptOutcome::Failed { message } }
                if message == "stream disconnected"
        )), "{found:?}");
        assert!(lens.turn.is_none());
    }
}
