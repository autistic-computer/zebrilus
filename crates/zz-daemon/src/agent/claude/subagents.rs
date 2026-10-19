use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{Read as _, Seek as _, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde_json::Value;

use super::translate::Translator;
use crate::agent::feed::{ChildPoll, FeedSource, Phase};

const READ_LIMIT: u64 = 4 * 1024 * 1024;
const PARENT_TAIL: u64 = 8 * 1024 * 1024;
const TAIL_BYTES: u64 = 64 * 1024;
const RECENT: Duration = Duration::from_mins(10);
const GRACE: Duration = Duration::from_secs(10);
const INTERRUPTED: &str = "[Request interrupted by user";
const LAUNCHED: &str = "Async agent launched";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SubagentMeta {
    pub(crate) kind: String,
    pub(crate) description: String,
    pub(crate) stopped: bool,
    pub(crate) finished: bool,
    pub(crate) background: bool,
    pub(crate) tool_use_id: Option<String>,
}

impl SubagentMeta {
    fn read(path: &Path) -> Option<Self> {
        let meta: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        Some(Self {
            kind: meta["agentType"].as_str().unwrap_or_default().to_owned(),
            description: meta["description"].as_str().unwrap_or_default().to_owned(),
            stopped: meta["stoppedByUser"] == true,
            finished: meta["finished"] == true,
            background: meta["requestShape"] == "background",
            tool_use_id: meta["toolUseId"].as_str().map(str::to_owned),
        })
    }

    fn heading(&self) -> String {
        match (self.kind.is_empty(), self.description.is_empty()) {
            (false, false) => format!("{} · {}", self.kind, self.description),
            (true, false) => self.description.clone(),
            (false, true) => self.kind.clone(),
            (true, true) => "Subagent".to_owned(),
        }
    }
}

pub(crate) fn subagents_dir(session_file: &Path) -> PathBuf {
    session_file.with_extension("").join("subagents")
}

fn agent_id(name: &str) -> Option<&str> {
    name.strip_prefix("agent-")?.strip_suffix(".meta.json")
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn tagged<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = start + text[start..].find(&close)?;
    Some(&text[start..end])
}

#[derive(Debug, Default)]
pub(crate) struct Completions {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
    notified: HashMap<String, Phase>,
    results: HashSet<String>,
    readable: bool,
}

impl Completions {
    pub(crate) fn new(dir: &Path) -> Self {
        Self {
            path: dir
                .parent()
                .map(|session| session.with_extension("jsonl"))
                .unwrap_or_default(),
            ..Self::default()
        }
    }

    pub(crate) fn refresh(&mut self) {
        let Ok(mut file) = File::open(&self.path) else {
            self.readable = false;
            return;
        };
        self.readable = true;
        let length = file.metadata().map_or(0, |metadata| metadata.len());
        if length < self.offset {
            *self = Self {
                path: std::mem::take(&mut self.path),
                ..Self::default()
            };
            self.readable = true;
        }
        let skip_first = self.offset == 0 && length > PARENT_TAIL;
        if skip_first {
            self.offset = length - PARENT_TAIL;
        }
        if length <= self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let mut bytes = Vec::new();
        if file
            .take(READ_LIMIT.min(length - self.offset))
            .read_to_end(&mut bytes)
            .is_err()
        {
            return;
        }
        self.offset += bytes.len() as u64;
        self.partial.extend_from_slice(&bytes);
        if skip_first && let Some(end) = self.partial.iter().position(|byte| *byte == b'\n') {
            self.partial.drain(..=end);
        }
        while let Some(end) = self.partial.iter().position(|byte| *byte == b'\n') {
            let line = self.partial.drain(..=end).collect::<Vec<_>>();
            self.line(&line);
        }
    }

    fn line(&mut self, line: &[u8]) {
        let notification = line
            .windows(17)
            .any(|window| window == b"task-notification");
        let result = line.windows(11).any(|window| window == b"tool_result");
        if !notification && !result {
            return;
        }
        let Ok(entry) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        let texts = [&entry["content"], &entry["message"]["content"]]
            .into_iter()
            .filter_map(Value::as_str);
        for text in texts {
            for block in text.split("<task-notification>").skip(1) {
                if let (Some(id), Some(status)) =
                    (tagged(block, "task-id"), tagged(block, "status"))
                {
                    let phase = if status == "completed" {
                        Phase::Done
                    } else {
                        Phase::Stopped
                    };
                    self.notified.insert(id.to_owned(), phase);
                }
            }
        }
        if entry["type"] != "user" {
            return;
        }
        for block in entry["message"]["content"].as_array().into_iter().flatten() {
            let launched = super::translate::result_text(&block["content"]).starts_with(LAUNCHED);
            if block["type"] == "tool_result"
                && !launched
                && let Some(id) = block["tool_use_id"].as_str()
            {
                self.results.insert(id.to_owned());
            }
        }
    }

    fn settle(&self, id: &str, meta: &SubagentMeta, lines: Phase) -> Phase {
        if meta.stopped || lines == Phase::Stopped {
            return Phase::Stopped;
        }
        if meta.finished {
            return Phase::Done;
        }
        if !self.readable {
            return lines;
        }
        if meta.background {
            return self.notified.get(id).copied().unwrap_or(Phase::Running);
        }
        match &meta.tool_use_id {
            Some(tool) if self.results.contains(tool) => Phase::Done,
            Some(_) => Phase::Running,
            None => lines,
        }
    }
}

fn line_phase(entry: &Value) -> Option<Phase> {
    match entry["type"].as_str()? {
        "assistant" => Some(
            if entry["message"]["stop_reason"].as_str() == Some("end_turn") {
                Phase::Done
            } else {
                Phase::Running
            },
        ),
        "user" => {
            let interrupted = match &entry["message"]["content"] {
                Value::String(text) => text.starts_with(INTERRUPTED),
                Value::Array(blocks) => blocks.iter().any(|block| {
                    block["type"] == "text"
                        && block["text"]
                            .as_str()
                            .is_some_and(|text| text.starts_with(INTERRUPTED))
                }),
                _ => false,
            };
            Some(if interrupted {
                Phase::Stopped
            } else {
                Phase::Running
            })
        }
        _ => None,
    }
}

pub(crate) fn tail_phase(log: &Path) -> Phase {
    let Ok(mut file) = File::open(log) else {
        return Phase::Running;
    };
    let length = file.metadata().map_or(0, |metadata| metadata.len());
    let start = length.saturating_sub(TAIL_BYTES);
    let mut bytes = Vec::new();
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_to_end(&mut bytes).is_err() {
        return Phase::Running;
    }
    bytes
        .split(|byte| *byte == b'\n')
        .rev()
        .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
        .find_map(|entry| line_phase(&entry))
        .unwrap_or(Phase::Running)
}

pub(crate) fn running_subagents(
    dir: &Path,
    now: SystemTime,
    completions: &Completions,
) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut running = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let id = agent_id(&name)?.to_owned();
            let log = dir.join(format!("agent-{id}.jsonl"));
            let recent = modified(&log)
                .and_then(|at| now.duration_since(at).ok())
                .is_some_and(|age| age <= RECENT);
            let meta = SubagentMeta::read(&entry.path())?;
            (recent && completions.settle(&id, &meta, tail_phase(&log)) == Phase::Running)
                .then_some(id)
        })
        .collect::<Vec<_>>();
    running.sort();
    running
}

struct Subagent {
    id: String,
    meta: SubagentMeta,
    meta_path: PathBuf,
    meta_stamp: Option<SystemTime>,
    log: PathBuf,
    offset: u64,
    partial: Vec<u8>,
    prompted: bool,
    translator: Translator,
    lines: Phase,
}

impl Subagent {
    fn new(id: String, meta: SubagentMeta, meta_path: PathBuf, log: PathBuf, cwd: &Path) -> Self {
        Self {
            meta_stamp: modified(&meta_path),
            id,
            meta,
            meta_path,
            log,
            offset: 0,
            partial: Vec::new(),
            prompted: false,
            translator: Translator::new(cwd.to_path_buf()),
            lines: Phase::Running,
        }
    }

    fn read(&mut self) -> Vec<Value> {
        let stamp = modified(&self.meta_path);
        if stamp != self.meta_stamp {
            self.meta_stamp = stamp;
            if let Some(meta) = SubagentMeta::read(&self.meta_path) {
                self.meta = meta;
            }
        }
        let mut fresh = Vec::new();
        if let Ok(mut file) = File::open(&self.log) {
            let length = file.metadata().map_or(0, |metadata| metadata.len());
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
        }
        while let Some(end) = self.partial.iter().position(|byte| *byte == b'\n') {
            let line = self.partial.drain(..=end).collect::<Vec<_>>();
            if let Ok(entry) = serde_json::from_slice::<Value>(&line) {
                fresh.extend(self.entry(&entry));
            }
        }
        fresh
    }

    fn entry(&mut self, entry: &Value) -> Vec<Value> {
        let Some(phase) = line_phase(entry) else {
            return Vec::new();
        };
        self.lines = phase;
        if phase == Phase::Stopped {
            return Vec::new();
        }
        if entry["type"] == "user" && !self.prompted && entry["parentUuid"].is_null() {
            self.prompted = true;
            return Vec::new();
        }
        self.translator.sidechain_entry(entry)
    }
}

pub(crate) struct ClaudeSource {
    dir: PathBuf,
    cwd: PathBuf,
    since: SystemTime,
    completions: Completions,
    agents: Vec<Subagent>,
    ignored: HashMap<String, Option<SystemTime>>,
}

impl ClaudeSource {
    pub(crate) fn new(dir: PathBuf, cwd: PathBuf, since: SystemTime) -> Self {
        Self {
            completions: Completions::new(&dir),
            dir,
            cwd,
            since,
            agents: Vec::new(),
            ignored: HashMap::new(),
        }
    }

    fn discover(&mut self, now: SystemTime) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let mut found = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                Some((agent_id(&name)?.to_owned(), entry.path()))
            })
            .filter(|(id, _)| !self.agents.iter().any(|agent| agent.id == *id))
            .collect::<Vec<_>>();
        found.sort_by_key(|(_, path)| modified(path));
        for (id, meta_path) in found {
            let log = self.dir.join(format!("agent-{id}.jsonl"));
            let stamp = modified(&log);
            if self
                .ignored
                .get(&id)
                .is_some_and(|ignored| *ignored == stamp)
            {
                continue;
            }
            let Some(meta) = SubagentMeta::read(&meta_path) else {
                self.ignored.insert(id, stamp);
                continue;
            };
            let fresh = modified(&meta_path).is_some_and(|at| at + GRACE >= self.since);
            let live = stamp
                .and_then(|at| now.duration_since(at).ok())
                .is_some_and(|age| age <= RECENT)
                && self.completions.settle(&id, &meta, tail_phase(&log)) == Phase::Running;
            if !fresh && !live {
                self.ignored.insert(id, stamp);
                continue;
            }
            self.agents
                .push(Subagent::new(id, meta, meta_path, log, &self.cwd));
        }
    }
}

impl FeedSource for ClaudeSource {
    fn poll(&mut self, now: SystemTime) -> Vec<ChildPoll> {
        self.completions.refresh();
        self.discover(now);
        let completions = &self.completions;
        self.agents
            .iter_mut()
            .map(|agent| {
                let fresh = agent.read();
                ChildPoll {
                    id: agent.id.clone(),
                    heading: agent.meta.heading(),
                    phase: completions.settle(&agent.id, &agent.meta, agent.lines),
                    fresh,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use serde_json::json;

    use super::*;
    use crate::agent::{
        claude::translate::chunk,
        feed::{Feed, FeedEvent},
    };

    fn feed(dir: &Path, since: SystemTime) -> Feed {
        Feed::new(Box::new(ClaudeSource::new(
            dir.to_path_buf(),
            "/w".into(),
            since,
        )))
    }

    struct Dir {
        _root: tempfile::TempDir,
        path: PathBuf,
    }

    fn dir() -> Dir {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("session").join("subagents");
        std::fs::create_dir_all(&path).unwrap();
        Dir { _root: root, path }
    }

    fn spawn(dir: &Path, id: &str, description: &str) {
        std::fs::write(
            dir.join(format!("agent-{id}.meta.json")),
            json!({ "agentType": "Explore", "description": description, "toolUseId": format!("toolu_{id}") })
                .to_string(),
        )
        .unwrap();
        line(
            dir,
            id,
            &json!({
                "type": "user", "parentUuid": null, "isSidechain": true, "agentId": id,
                "message": { "role": "user", "content": format!("Long prompt for {description}") },
            }),
        );
    }

    fn line(dir: &Path, id: &str, entry: &Value) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(format!("agent-{id}.jsonl")))
            .unwrap();
        writeln!(file, "{entry}").unwrap();
    }

    fn tool(dir: &Path, id: &str, call: &str) {
        line(
            dir,
            id,
            &json!({
                "type": "assistant", "isSidechain": true, "parentUuid": "p",
                "message": { "id": format!("msg_{call}"), "role": "assistant", "stop_reason": "tool_use",
                    "content": [{ "type": "tool_use", "id": call, "name": "Read", "input": { "file_path": "/w/src/lib.rs" } }] },
            }),
        );
        line(
            dir,
            id,
            &json!({
                "type": "user", "isSidechain": true, "parentUuid": "q",
                "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": call, "content": "fn main() {}" }] },
            }),
        );
    }

    fn answer(dir: &Path, id: &str, text: &str) {
        line(
            dir,
            id,
            &json!({
                "type": "assistant", "isSidechain": true, "parentUuid": "r",
                "message": { "id": format!("msg_end_{id}"), "role": "assistant", "stop_reason": "end_turn",
                    "content": [{ "type": "text", "text": text }] },
            }),
        );
    }

    fn kinds(updates: &[Value]) -> Vec<String> {
        updates
            .iter()
            .map(|update| {
                let kind = update["sessionUpdate"].as_str().unwrap_or_default();
                let text = update["content"]["text"]
                    .as_str()
                    .or_else(|| update["toolCallId"].as_str())
                    .unwrap_or_default();
                format!("{kind}:{text}")
            })
            .collect()
    }

    #[test]
    fn a_subagent_becomes_a_turn_headed_by_its_description() {
        let dir = dir();
        let since = SystemTime::now();
        spawn(&dir.path, "a1", "Find the float code");
        tool(&dir.path, "a1", "t1");
        let mut feed = feed(&dir.path, since);
        let events = feed.poll(SystemTime::now());
        let FeedEvent::Append(updates) = &events[0] else {
            panic!("{events:?}");
        };
        assert_eq!(
            kinds(updates),
            [
                "user_message_chunk:Explore · Find the float code",
                "tool_call:t1",
                "tool_call_update:t1",
            ]
        );
        assert!(events.contains(&FeedEvent::Busy(true)));
        assert!(matches!(&events[1], FeedEvent::Tasks(tasks) if tasks[0].id == "a1"));

        answer(&dir.path, "a1", "Found it in pane.rs");
        let events = feed.poll(SystemTime::now());
        assert_eq!(
            events,
            [
                FeedEvent::Append(vec![chunk(
                    "agent_message_chunk",
                    "msg_end_a1#1",
                    "Found it in pane.rs"
                )]),
                FeedEvent::Tasks(Vec::new()),
                FeedEvent::Busy(false),
            ]
        );
        assert!(feed.poll(SystemTime::now()).is_empty());
    }

    #[test]
    fn a_turn_that_finishes_behind_the_followed_one_is_placed_before_it() {
        let dir = dir();
        let since = SystemTime::now();
        spawn(&dir.path, "a1", "first");
        let mut feed = feed(&dir.path, since);
        feed.poll(SystemTime::now());
        spawn(&dir.path, "a2", "second");
        tool(&dir.path, "a2", "t2");
        let events = feed.poll(SystemTime::now());
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, FeedEvent::Append(_) | FeedEvent::Replace(_))),
            "an unfollowed subagent waits: {events:?}"
        );
        answer(&dir.path, "a2", "second done");
        let events = feed.poll(SystemTime::now());
        let FeedEvent::Replace(timeline) = &events[0] else {
            panic!("{events:?}");
        };
        assert!(
            events.contains(&FeedEvent::Busy(true)),
            "a rebuilt timeline reopens the live turn: {events:?}"
        );
        assert_eq!(
            kinds(timeline),
            [
                "user_message_chunk:Explore · second",
                "tool_call:t2",
                "tool_call_update:t2",
                "agent_message_chunk:second done",
                "user_message_chunk:Explore · first",
            ]
        );
        tool(&dir.path, "a1", "t1");
        assert_eq!(
            kinds(match &feed.poll(SystemTime::now())[0] {
                FeedEvent::Append(updates) => updates,
                other => panic!("{other:?}"),
            }),
            ["tool_call:t1", "tool_call_update:t1"]
        );
    }

    #[test]
    fn the_next_running_subagent_is_followed_once_the_first_ends() {
        let dir = dir();
        let since = SystemTime::now();
        spawn(&dir.path, "a1", "first");
        spawn(&dir.path, "a2", "second");
        let mut feed = feed(&dir.path, since);
        feed.poll(SystemTime::now());
        tool(&dir.path, "a2", "t2");
        answer(&dir.path, "a1", "first done");
        let events = feed.poll(SystemTime::now());
        let FeedEvent::Append(updates) = &events[0] else {
            panic!("{events:?}");
        };
        let timeline = kinds(&feed.timeline());
        assert_eq!(
            kinds(updates),
            [
                "agent_message_chunk:first done",
                "user_message_chunk:Explore · second",
                "tool_call:t2",
                "tool_call_update:t2",
            ]
        );
        assert_eq!(timeline.len(), 5);
    }

    #[test]
    fn a_background_subagent_runs_until_its_parent_hears_it_finished() {
        let dir = dir();
        let parent = dir.path.parent().unwrap().with_extension("jsonl");
        std::fs::write(&parent, "").unwrap();
        std::fs::write(
            dir.path.join("agent-bg.meta.json"),
            json!({ "agentType": "Explore", "description": "slow", "toolUseId": "toolu_bg", "requestShape": "background" })
                .to_string(),
        )
        .unwrap();
        answer(&dir.path, "bg", "waiting on my own shell");
        let mut completions = Completions::new(&dir.path);
        completions.refresh();
        assert_eq!(
            running_subagents(&dir.path, SystemTime::now(), &completions),
            ["bg"],
            "an ended turn is not the end of a background subagent"
        );
        let mut feed = feed(&dir.path, SystemTime::now());
        assert!(
            feed.poll(SystemTime::now())
                .contains(&FeedEvent::Busy(true))
        );

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&parent)
            .unwrap();
        writeln!(
            file,
            "{}",
            json!({ "type": "queue-operation", "operation": "enqueue",
                "content": "<task-notification><task-id>bg</task-id><tool-use-id>toolu_bg</tool-use-id><status>completed</status></task-notification>" })
        )
        .unwrap();
        completions.refresh();
        assert!(running_subagents(&dir.path, SystemTime::now(), &completions).is_empty());
        assert!(
            feed.poll(SystemTime::now())
                .contains(&FeedEvent::Busy(false))
        );
    }

    #[test]
    fn a_stop_or_an_old_finished_subagent_never_runs() {
        let dir = dir();
        spawn(&dir.path, "old", "earlier");
        answer(&dir.path, "old", "long ago");
        let since = SystemTime::now() + Duration::from_mins(1);
        let mut feed = feed(&dir.path, since);
        assert!(
            feed.poll(SystemTime::now()).is_empty(),
            "a finished one from before stays out"
        );
        assert!(
            running_subagents(&dir.path, SystemTime::now(), &Completions::new(&dir.path))
                .is_empty()
        );

        spawn(&dir.path, "live", "now");
        assert_eq!(
            running_subagents(&dir.path, SystemTime::now(), &Completions::new(&dir.path)),
            ["live"]
        );
        line(
            &dir.path,
            "live",
            &json!({
                "type": "user", "isSidechain": true, "parentUuid": "s",
                "message": { "role": "user", "content": [{ "type": "text", "text": "[Request interrupted by user]" }] },
            }),
        );
        assert!(
            running_subagents(&dir.path, SystemTime::now(), &Completions::new(&dir.path))
                .is_empty()
        );
        assert_eq!(
            tail_phase(&dir.path.join("agent-live.jsonl")),
            Phase::Stopped
        );
    }
}
