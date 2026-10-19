use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::{BufRead as _, BufReader, Read as _, Seek as _, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde_json::{Map, Value, json};

use super::translate::Translator;
use crate::agent::feed::{ChildPoll, FeedSource, Phase, StopTarget};

const READ_LIMIT: u64 = 4 * 1024 * 1024;
const META_LIMIT: u64 = 4 * 1024 * 1024;
const TAIL_BYTES: u64 = 64 * 1024;
const RECENT: Duration = Duration::from_mins(10);
const GRACE: Duration = Duration::from_secs(10);
const DAYS: usize = 2;

pub(crate) fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
}

pub(crate) fn sessions_dir() -> Option<PathBuf> {
    codex_home().map(|home| home.join("sessions"))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RolloutMeta {
    pub(crate) thread: String,
    pub(crate) root: String,
    pub(crate) parent: Option<String>,
    pub(crate) cwd: PathBuf,
    start: u64,
    heading: String,
}

fn humanize(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .replace(['_', '-'], " ")
}

impl RolloutMeta {
    fn read(path: &Path) -> Option<Self> {
        let mut line = Vec::new();
        BufReader::new(File::open(path).ok()?)
            .take(META_LIMIT)
            .read_until(b'\n', &mut line)
            .ok()?;
        let entry: Value = serde_json::from_slice(&line).ok()?;
        if entry["type"] != "session_meta" {
            return None;
        }
        let meta = &entry["payload"];
        let thread = meta["id"].as_str()?.to_owned();
        let spawn = &meta["source"]["subagent"]["thread_spawn"];
        let parent = meta["parent_thread_id"]
            .as_str()
            .or_else(|| spawn["parent_thread_id"].as_str())
            .map(str::to_owned);
        let role = meta["agent_role"].as_str().filter(|role| !role.is_empty());
        let task = meta["agent_path"]
            .as_str()
            .or_else(|| spawn["agent_path"].as_str())
            .map(humanize)
            .filter(|task| !task.trim().is_empty());
        let nickname = meta["agent_nickname"].as_str();
        let heading = match (role, task, nickname) {
            (Some(role), Some(task), _) => format!("{role} · {task}"),
            (None, Some(task), Some(nickname)) => format!("{nickname} · {task}"),
            (None, Some(task), None) => task,
            (Some(role), None, _) => role.to_owned(),
            (None, None, Some(nickname)) => nickname.to_owned(),
            (None, None, None) => "Subagent".to_owned(),
        };
        Some(Self {
            root: meta["session_id"].as_str().unwrap_or(&thread).to_owned(),
            thread,
            parent,
            cwd: PathBuf::from(meta["cwd"].as_str().unwrap_or("/")),
            start: meta["subagent_history_start_ordinal"].as_u64().unwrap_or(0),
            heading,
        })
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn sorted_dirs(path: &Path) -> Vec<PathBuf> {
    let mut dirs = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    dirs.sort();
    dirs.reverse();
    dirs
}

fn recent_rollouts(sessions: &Path) -> Vec<PathBuf> {
    let days = sorted_dirs(sessions)
        .into_iter()
        .flat_map(|year| sorted_dirs(&year))
        .flat_map(|month| sorted_dirs(&month))
        .take(DAYS);
    days.flat_map(|day| {
        std::fs::read_dir(day)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "jsonl")
            })
            .collect::<Vec<_>>()
    })
    .collect()
}

fn line_phase(entry: &Value, start: u64) -> Option<(Phase, Option<String>)> {
    if entry["type"] != "event_msg" || entry["ordinal"].as_u64().unwrap_or(0) < start {
        return None;
    }
    let payload = &entry["payload"];
    let turn = payload["turn_id"].as_str().map(str::to_owned);
    match payload["type"].as_str()? {
        "task_started" => Some((Phase::Running, turn)),
        "task_complete" => Some((Phase::Done, turn)),
        "turn_aborted" => Some((Phase::Stopped, turn)),
        _ => None,
    }
}

pub(crate) fn rollout_phase(path: &Path, start: u64) -> Phase {
    let Ok(mut file) = File::open(path) else {
        return Phase::Running;
    };
    let length = file.metadata().map_or(0, |metadata| metadata.len());
    let mut bytes = Vec::new();
    if file
        .seek(SeekFrom::Start(length.saturating_sub(TAIL_BYTES)))
        .is_err()
        || file.read_to_end(&mut bytes).is_err()
    {
        return Phase::Running;
    }
    bytes
        .split(|byte| *byte == b'\n')
        .rev()
        .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
        .find_map(|entry| line_phase(&entry, start))
        .map_or(Phase::Running, |(phase, _)| phase)
}

#[derive(Debug, Default)]
pub(crate) struct Rollouts {
    metas: HashMap<PathBuf, (Option<SystemTime>, Option<RolloutMeta>)>,
}

impl Rollouts {
    fn scan(&mut self, sessions: &Path, now: SystemTime) -> Vec<(PathBuf, RolloutMeta)> {
        let mut found = Vec::new();
        for path in recent_rollouts(sessions) {
            let stamp = modified(&path);
            let recent = stamp
                .and_then(|at| now.duration_since(at).ok())
                .is_some_and(|age| age <= RECENT);
            if !recent {
                continue;
            }
            let meta = match self.metas.get(&path) {
                Some((read, meta)) if meta.is_some() || *read == stamp => meta.clone(),
                _ => {
                    let meta = RolloutMeta::read(&path);
                    self.metas.insert(path.clone(), (stamp, meta.clone()));
                    meta
                }
            };
            if let Some(meta) = meta {
                found.push((path, meta));
            }
        }
        found
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RunningTree {
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) children: Vec<String>,
}

pub(crate) fn running_children(
    sessions: &Path,
    now: SystemTime,
    rollouts: &mut Rollouts,
) -> BTreeMap<String, RunningTree> {
    let scanned = rollouts.scan(sessions, now);
    let mut trees = BTreeMap::<String, RunningTree>::new();
    for (path, meta) in &scanned {
        if meta.parent.is_some() && rollout_phase(path, meta.start) == Phase::Running {
            trees
                .entry(meta.root.clone())
                .or_default()
                .children
                .push(meta.thread.clone());
        }
    }
    for (_, meta) in scanned {
        if meta.parent.is_none()
            && let Some(tree) = trees.get_mut(&meta.thread)
        {
            tree.cwd = Some(meta.cwd);
        }
    }
    for tree in trees.values_mut() {
        tree.children.sort();
    }
    trees
}

fn camel(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut upper = false;
    for character in key.chars() {
        if character == '_' {
            upper = true;
        } else if upper {
            out.extend(character.to_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

fn camelize(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (camel(key), camelize(value)))
                .collect::<Map<_, _>>(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(camelize).collect()),
        other => other.clone(),
    }
}

fn texts(parts: &Value) -> Vec<String> {
    parts
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part.as_str().or_else(|| part["text"].as_str()))
        .map(str::to_owned)
        .collect()
}

pub(crate) fn v2_item(item: &Value) -> Option<Value> {
    let kind = item["type"].as_str()?;
    let mut characters = kind.chars();
    let kind = characters
        .next()
        .map(|first| first.to_lowercase().chain(characters).collect::<String>())?;
    let id = item["id"].clone();
    match kind.as_str() {
        "agentMessage" => Some(json!({
            "type": "agentMessage",
            "id": id,
            "text": texts(&item["content"]).join("\n\n"),
        })),
        "reasoning" => Some(json!({
            "type": "reasoning",
            "id": id,
            "summary": texts(&item["summary_text"]),
            "content": texts(&item["raw_content"]),
        })),
        "subAgentActivity" if item["kind"] == "interacted" => None,
        "userMessage" => None,
        "commandExecution" => {
            let command = match &item["command"] {
                Value::Array(parts) => parts
                    .last()
                    .and_then(Value::as_str)
                    .filter(|_| parts.len() == 3 && parts[1] == "-lc")
                    .map_or_else(
                        || {
                            parts
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(" ")
                        },
                        str::to_owned,
                    ),
                other => other.as_str().unwrap_or_default().to_owned(),
            };
            let actions = item["parsed_cmd"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|action| {
                    json!({
                        "type": match action["type"].as_str() {
                            Some("read") => "read",
                            Some("list_files") => "listFiles",
                            Some("search") => "search",
                            _ => "unknown",
                        },
                        "command": action["cmd"],
                        "path": action["path"].as_str().or_else(|| action["name"].as_str()),
                    })
                })
                .collect::<Vec<_>>();
            Some(json!({
                "type": "commandExecution",
                "id": id,
                "command": command,
                "cwd": item["cwd"].as_str().map(|cwd| cwd.trim_start_matches("file://")),
                "status": item["status"],
                "aggregatedOutput": item["aggregated_output"],
                "exitCode": item["exit_code"],
                "commandActions": actions,
            }))
        }
        _ => {
            let mut item = camelize(item);
            item["type"] = Value::from(kind);
            Some(item)
        }
    }
}

struct Child {
    meta: RolloutMeta,
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
    translator: Translator,
    phase: Phase,
    turn: Option<String>,
}

impl Child {
    fn read(&mut self) -> Vec<Value> {
        let mut fresh = Vec::new();
        if let Ok(mut file) = File::open(&self.path) {
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
            let Ok(entry) = serde_json::from_slice::<Value>(&line) else {
                continue;
            };
            if let Some((phase, turn)) = line_phase(&entry, self.meta.start) {
                self.phase = phase;
                if turn.is_some() {
                    self.turn = turn;
                }
                continue;
            }
            if entry["type"] == "event_msg"
                && entry["payload"]["type"] == "item_completed"
                && entry["ordinal"].as_u64().unwrap_or(0) >= self.meta.start
                && let Some(item) = v2_item(&entry["payload"]["item"])
            {
                fresh.extend(self.translator.history_item(&item));
            }
        }
        fresh
    }
}

pub(crate) struct CodexSource {
    sessions: PathBuf,
    root: String,
    cwd: PathBuf,
    since: SystemTime,
    rollouts: Rollouts,
    children: Vec<Child>,
    ignored: HashMap<PathBuf, Option<SystemTime>>,
}

impl CodexSource {
    pub(crate) fn new(sessions: PathBuf, root: String, cwd: PathBuf, since: SystemTime) -> Self {
        Self {
            sessions,
            root,
            cwd,
            since,
            rollouts: Rollouts::default(),
            children: Vec::new(),
            ignored: HashMap::new(),
        }
    }

    fn discover(&mut self, now: SystemTime) {
        let mut found = self
            .rollouts
            .scan(&self.sessions, now)
            .into_iter()
            .filter(|(path, meta)| {
                meta.parent.is_some()
                    && meta.root == self.root
                    && !self.children.iter().any(|child| child.path == *path)
            })
            .collect::<Vec<_>>();
        found.sort_by_key(|(path, _)| path.file_name().map(std::ffi::OsStr::to_owned));
        for (path, meta) in found {
            let stamp = modified(&path);
            if self
                .ignored
                .get(&path)
                .is_some_and(|ignored| *ignored == stamp)
            {
                continue;
            }
            let fresh = stamp.is_some_and(|at| at + GRACE >= self.since);
            if !fresh && rollout_phase(&path, meta.start) != Phase::Running {
                self.ignored.insert(path, stamp);
                continue;
            }
            self.children.push(Child {
                translator: Translator::new(self.cwd.clone()),
                meta,
                path,
                offset: 0,
                partial: Vec::new(),
                phase: Phase::Running,
                turn: None,
            });
        }
    }
}

impl FeedSource for CodexSource {
    fn poll(&mut self, now: SystemTime) -> Vec<ChildPoll> {
        self.discover(now);
        self.children
            .iter_mut()
            .map(|child| {
                let fresh = child.read();
                ChildPoll {
                    id: child.meta.thread.clone(),
                    heading: child.meta.heading.clone(),
                    phase: child.phase,
                    fresh,
                }
            })
            .collect()
    }

    fn stop_target(&self, id: &str) -> Option<StopTarget> {
        let child = self.children.iter().find(|child| child.meta.thread == id)?;
        Some(StopTarget {
            task: id.to_owned(),
            turn: Some(child.turn.clone()?),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;
    use crate::agent::feed::{Feed, FeedEvent};

    struct Sessions {
        _root: tempfile::TempDir,
        day: PathBuf,
        sessions: PathBuf,
        ordinal: u64,
    }

    fn sessions() -> Sessions {
        let root = tempfile::tempdir().unwrap();
        let sessions = root.path().join("sessions");
        let day = sessions.join("2026").join("10").join("18");
        std::fs::create_dir_all(&day).unwrap();
        Sessions {
            _root: root,
            day,
            sessions,
            ordinal: 0,
        }
    }

    impl Sessions {
        fn path(&self, thread: &str) -> PathBuf {
            self.day
                .join(format!("rollout-2026-10-18T10-00-00-{thread}.jsonl"))
        }

        fn write(&mut self, thread: &str, kind: &str, payload: &Value) {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.path(thread))
                .unwrap();
            self.ordinal += 1;
            writeln!(
                file,
                "{}",
                json!({ "ordinal": self.ordinal, "type": kind, "payload": payload })
            )
            .unwrap();
        }

        fn root(&mut self, thread: &str) {
            self.write(
                thread,
                "session_meta",
                &json!({ "id": thread, "session_id": thread, "cwd": "/w" }),
            );
        }

        fn child(&mut self, root: &str, thread: &str, path: &str) {
            self.write(
                thread,
                "session_meta",
                &json!({
                    "id": thread, "session_id": root, "parent_thread_id": root, "cwd": "/w",
                    "agent_role": "codebase-locator", "agent_path": path,
                    "subagent_history_start_ordinal": 0,
                }),
            );
            self.write(
                thread,
                "event_msg",
                &json!({ "type": "task_started", "turn_id": format!("turn-{thread}") }),
            );
        }

        fn command(&mut self, thread: &str, id: &str, command: &str) {
            self.write(
                thread,
                "event_msg",
                &json!({ "type": "item_completed", "thread_id": thread, "item": {
                    "type": "CommandExecution", "id": id,
                    "command": ["/bin/bash", "-lc", command], "cwd": "file:///w",
                    "parsed_cmd": [{ "type": "unknown", "cmd": command }],
                    "status": "completed", "aggregated_output": "ok\n", "exit_code": 0,
                } }),
            );
        }

        fn answer(&mut self, thread: &str, text: &str) {
            self.write(
                thread,
                "event_msg",
                &json!({ "type": "item_completed", "thread_id": thread, "item": {
                    "type": "AgentMessage", "id": format!("msg-{thread}"),
                    "content": [{ "type": "Text", "text": text }],
                } }),
            );
            self.write(
                thread,
                "event_msg",
                &json!({ "type": "task_complete", "turn_id": format!("turn-{thread}") }),
            );
        }
    }

    #[test]
    fn running_children_group_under_their_root_with_its_cwd() {
        let mut sessions = sessions();
        sessions.root("root-1");
        sessions.child("root-1", "kid-a", "/root/map_status");
        sessions.child("root-1", "kid-b", "/root/count_files");
        sessions.answer("kid-b", "done");
        let mut rollouts = Rollouts::default();
        let trees = running_children(&sessions.sessions, SystemTime::now(), &mut rollouts);
        assert_eq!(
            trees.get("root-1"),
            Some(&RunningTree {
                cwd: Some(PathBuf::from("/w")),
                children: vec!["kid-a".to_owned()],
            })
        );
    }

    #[test]
    fn a_child_rollout_becomes_a_turn_and_stops_by_its_turn() {
        let mut sessions = sessions();
        sessions.root("root-1");
        sessions.child("root-1", "kid-a", "/root/map_status");
        sessions.command("kid-a", "exec-1", "rg --files");
        let mut feed = Feed::new(Box::new(CodexSource::new(
            sessions.sessions.clone(),
            "root-1".to_owned(),
            "/w".into(),
            SystemTime::now(),
        )));
        let events = feed.poll(SystemTime::now());
        let FeedEvent::Append(updates) = &events[0] else {
            panic!("{events:?}");
        };
        assert_eq!(
            updates[0]["content"]["text"],
            "codebase-locator · map status"
        );
        assert_eq!(updates[1]["sessionUpdate"], "tool_call");
        assert_eq!(updates[1]["title"], "rg --files");
        assert_eq!(updates[2]["status"], "completed");
        assert!(events.contains(&FeedEvent::Busy(true)));
        assert_eq!(
            feed.stop_target("kid-a"),
            Some(StopTarget {
                task: "kid-a".to_owned(),
                turn: Some("turn-kid-a".to_owned())
            })
        );

        sessions.answer("kid-a", "Mapped it.");
        let events = feed.poll(SystemTime::now());
        let FeedEvent::Append(updates) = &events[0] else {
            panic!("{events:?}");
        };
        assert_eq!(updates[0]["sessionUpdate"], "agent_message_chunk");
        assert_eq!(updates[0]["content"]["text"], "Mapped it.");
        assert!(events.contains(&FeedEvent::Busy(false)));
    }

    #[test]
    fn a_rollout_caught_before_its_first_line_is_read_again() {
        let mut sessions = sessions();
        std::fs::write(
            sessions.path("kid-a"),
            "{\"ordinal\":0,\"type\":\"session_m",
        )
        .unwrap();
        let mut rollouts = Rollouts::default();
        assert!(running_children(&sessions.sessions, SystemTime::now(), &mut rollouts).is_empty());
        std::fs::remove_file(sessions.path("kid-a")).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        sessions.child("root-1", "kid-a", "/root/map_status");
        let trees = running_children(&sessions.sessions, SystemTime::now(), &mut rollouts);
        assert_eq!(trees["root-1"].children, ["kid-a"]);
    }

    #[test]
    fn rollout_items_take_the_app_server_shape() {
        let item = v2_item(&json!({
            "type": "Reasoning", "id": "rs", "summary_text": ["plan"], "raw_content": [],
        }))
        .unwrap();
        assert_eq!(
            item,
            json!({ "type": "reasoning", "id": "rs", "summary": ["plan"], "content": [] })
        );
        assert!(v2_item(&json!({ "type": "SubAgentActivity", "kind": "interacted" })).is_none());
        let change =
            v2_item(&json!({ "type": "WebSearch", "id": "w", "search_query": "zz" })).unwrap();
        assert_eq!(
            change,
            json!({ "type": "webSearch", "id": "w", "searchQuery": "zz" })
        );
    }
}
