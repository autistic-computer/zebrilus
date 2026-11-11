use std::{
    collections::{HashMap, HashSet},
    fs::{File, Metadata},
    io::{Read as _, Seek as _, SeekFrom},
    path::{Path, PathBuf},
    sync::LazyLock,
    time::{SystemTime, UNIX_EPOCH},
};

use parking_lot::Mutex;
use serde_json::Value;

use super::translate::{INTERRUPTED, Translator, interrupted, user_prompt};
use crate::agent::chat_bridge::turn_mark;
use crate::agent::stream::AgentSessionSummary;

const MAX_SANITIZED_LENGTH: usize = 200;
const LITE_READ_BYTES: u64 = 64 * 1024;
const MAX_TRANSCRIPT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TITLE_CHARS: usize = 200;

pub(crate) fn config_home() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claude")))
}

fn sanitize(path: &str) -> String {
    let sanitized = path
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.len() <= MAX_SANITIZED_LENGTH {
        return sanitized;
    }
    format!(
        "{}-{}",
        &sanitized[..MAX_SANITIZED_LENGTH],
        simple_hash(path)
    )
}

fn simple_hash(text: &str) -> String {
    let mut hash: i32 = 0;
    for unit in text.encode_utf16() {
        hash = hash
            .wrapping_shl(5)
            .wrapping_sub(hash)
            .wrapping_add(i32::from(unit));
    }
    let mut value = hash.unsigned_abs();
    if value == 0 {
        return "0".to_owned();
    }
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    while value > 0 {
        out.push(digits[(value % 36) as usize]);
        value /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn project_dir(projects: &Path, cwd: &Path) -> Option<PathBuf> {
    let canonical = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let sanitized = sanitize(&canonical.to_string_lossy());
    let exact = projects.join(&sanitized);
    if exact.is_dir() {
        return Some(exact);
    }
    if sanitized.len() <= MAX_SANITIZED_LENGTH {
        return None;
    }
    let prefix = format!("{}-", &sanitized[..MAX_SANITIZED_LENGTH]);
    std::fs::read_dir(projects)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(&prefix))
        })
}

pub(crate) fn session_file(config: &Path, cwd: &Path, session_id: &str) -> Option<PathBuf> {
    if !valid_uuid(session_id) {
        return None;
    }
    let projects = config.join("projects");
    let name = format!("{session_id}.jsonl");
    if let Some(path) = project_dir(&projects, cwd)
        .map(|dir| dir.join(&name))
        .filter(|path| path.is_file())
    {
        return Some(path);
    }
    std::fs::read_dir(&projects)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(&name))
        .find(|path| path.is_file())
}

pub(crate) fn valid_uuid(value: &str) -> bool {
    let parts = value.split('-').collect::<Vec<_>>();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(part, len)| part.len() == len && part.chars().all(|c| c.is_ascii_hexdigit()))
}

struct Lite {
    head: String,
    tail: String,
    modified_ms: u128,
}

fn read_lite(path: &Path) -> Option<Lite> {
    let mut file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    let size = metadata.len();
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_millis());
    let mut head = Vec::new();
    (&mut file)
        .take(LITE_READ_BYTES)
        .read_to_end(&mut head)
        .ok()?;
    if head.is_empty() {
        return None;
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let tail = if size > LITE_READ_BYTES {
        file.seek(SeekFrom::Start(size - LITE_READ_BYTES)).ok()?;
        let mut tail = Vec::new();
        file.take(LITE_READ_BYTES).read_to_end(&mut tail).ok()?;
        String::from_utf8_lossy(&tail).into_owned()
    } else {
        head.clone()
    };
    Some(Lite {
        head,
        tail,
        modified_ms,
    })
}

fn lines(text: &str) -> impl DoubleEndedIterator<Item = Value> + '_ {
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
}

fn lines_naming<'a>(text: &'a str, key: &str) -> impl DoubleEndedIterator<Item = Value> + use<'a> {
    let quoted = format!("\"{key}\"");
    text.lines()
        .filter(move |line| line.contains(&quoted))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
}

fn last_field(text: &str, key: &str) -> Option<String> {
    lines_naming(text, key).rev().find_map(|entry| {
        entry[key]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn first_field(text: &str, key: &str) -> Option<String> {
    lines_naming(text, key).find_map(|entry| {
        entry[key]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn first_prompt(head: &str) -> Option<String> {
    let mut command = None;
    for entry in lines(head) {
        if entry["type"] != "user" || entry["isMeta"] == true || entry["isCompactSummary"] == true {
            continue;
        }
        let texts = match &entry["message"]["content"] {
            Value::String(text) => vec![text.clone()],
            Value::Array(blocks) => {
                if blocks.iter().any(|block| block["type"] == "tool_result") {
                    continue;
                }
                blocks
                    .iter()
                    .filter_map(|block| block["text"].as_str().map(str::to_owned))
                    .collect()
            }
            _ => continue,
        };
        for text in texts {
            let text = text.replace('\n', " ");
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            if let Some(name) = text
                .split_once("<command-name>")
                .and_then(|(_, rest)| rest.split_once("</command-name>"))
                .map(|(name, _)| name.to_owned())
            {
                command.get_or_insert(name);
                continue;
            }
            if text.starts_with('<') || text.starts_with(INTERRUPTED) {
                continue;
            }
            return Some(clip(text));
        }
    }
    command
}

fn clip(text: &str) -> String {
    let mut clipped = text.chars().take(MAX_TITLE_CHARS).collect::<String>();
    if text.chars().count() > MAX_TITLE_CHARS {
        clipped = clipped.trim_end().to_owned();
        clipped.push('…');
    }
    clipped
}

#[derive(Clone)]
struct Digest {
    modified_ms: u128,
    cwd: Option<PathBuf>,
    title: String,
}

struct Cached {
    modified: Option<SystemTime>,
    size: u64,
    digest: Option<Digest>,
}

static DIGESTS: LazyLock<Mutex<HashMap<PathBuf, Cached>>> = LazyLock::new(Mutex::default);

fn digest(path: &Path) -> Option<Digest> {
    let lite = read_lite(path)?;
    let first_line = lite.head.lines().next().unwrap_or_default();
    if first_line.contains("\"isSidechain\":true") {
        return None;
    }
    let title = saved_title(&lite)
        .or_else(|| last_field(&lite.tail, "lastPrompt"))
        .or_else(|| last_field(&lite.tail, "summary"))
        .or_else(|| first_prompt(&lite.head))?;
    Some(Digest {
        modified_ms: lite.modified_ms,
        cwd: first_field(&lite.head, "cwd").map(PathBuf::from),
        title: clip(&title),
    })
}

fn cached_digest(path: &Path, metadata: &Metadata) -> Option<Digest> {
    let modified = metadata.modified().ok();
    let size = metadata.len();
    if let Some(cached) = DIGESTS.lock().get(path)
        && cached.modified == modified
        && cached.size == size
    {
        return cached.digest.clone();
    }
    let digest = digest(path);
    DIGESTS.lock().insert(
        path.to_path_buf(),
        Cached {
            modified,
            size,
            digest: digest.clone(),
        },
    );
    digest
}

fn summary(
    session_id: &str,
    path: &Path,
    metadata: &Metadata,
    project: Option<&Path>,
) -> Option<(u128, AgentSessionSummary)> {
    let digest = cached_digest(path, metadata)?;
    let cwd = digest.cwd.or_else(|| project.map(Path::to_path_buf))?;
    Some((
        digest.modified_ms,
        AgentSessionSummary {
            session_id: session_id.to_owned(),
            cwd,
            additional_directories: Vec::new(),
            title: Some(digest.title),
            updated_at: rfc3339(digest.modified_ms),
        },
    ))
}

fn saved_title(lite: &Lite) -> Option<String> {
    ["customTitle", "aiTitle"]
        .into_iter()
        .find_map(|key| last_field(&lite.tail, key).or_else(|| last_field(&lite.head, key)))
}

pub(crate) fn title(path: &Path) -> Option<String> {
    saved_title(&read_lite(path)?)
}

pub(crate) fn rfc3339(ms: u128) -> Option<String> {
    let secs = i64::try_from(ms / 1000).ok()?;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hour, minute, second) = (rem / 3600, rem % 3600 / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    ))
}

pub(crate) fn list(config: &Path, cwd: Option<&Path>) -> Vec<AgentSessionSummary> {
    let projects = config.join("projects");
    let dirs = match cwd {
        Some(cwd) => project_dir(&projects, cwd).into_iter().collect::<Vec<_>>(),
        None => std::fs::read_dir(&projects)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| path.is_dir())
                    .collect()
            })
            .unwrap_or_default(),
    };
    let mut seen = HashSet::new();
    let transcripts = dirs
        .iter()
        .flat_map(|dir| {
            std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .collect::<Vec<_>>()
        })
        .filter_map(|entry| {
            let path = entry.path();
            let stem = path.file_stem()?.to_str()?.to_owned();
            (path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") && valid_uuid(&stem))
                .then_some((stem, path, entry))
        })
        .filter_map(|(session_id, path, entry)| Some((session_id, path, entry.metadata().ok()?)))
        .collect::<Vec<_>>();
    if cwd.is_none() {
        let listed = transcripts
            .iter()
            .map(|(_, path, _)| path.as_path())
            .collect::<HashSet<_>>();
        DIGESTS
            .lock()
            .retain(|path, _| listed.contains(path.as_path()));
    }
    let mut sessions = transcripts
        .iter()
        .filter_map(|(session_id, path, metadata)| summary(session_id, path, metadata, cwd))
        .collect::<Vec<_>>();
    sessions.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    sessions
        .into_iter()
        .map(|(_, summary)| summary)
        .filter(|summary| seen.insert(summary.session_id.clone()))
        .collect()
}

pub(crate) fn transcript(path: &Path, cwd: PathBuf, until: Option<&str>) -> Vec<Value> {
    let entries = chain_entries(path);
    let mut replay = Replay {
        translator: Translator::new(cwd),
        subagents: subagents(path),
        updates: Vec::new(),
        open: false,
        last: None,
    };
    for entry in chain(&entries, until) {
        replay.entry(entry);
    }
    replay.end_turn("completed", None);
    replay.updates
}

pub(crate) fn trim(updates: &mut Vec<Value>, limit: usize) {
    if updates.len() <= limit {
        return;
    }
    updates.drain(..updates.len() - limit);
    let mark = updates
        .iter()
        .find_map(|update| update["_meta"]["zz"]["turn"].as_str());
    if mark.is_some_and(|mark| mark != "started") {
        let at = updates
            .iter()
            .find_map(|update| update["_meta"]["zz"]["at"].as_u64());
        updates.insert(0, turn_mark("started", at));
    }
}

pub(crate) fn prompts(path: &Path) -> Vec<(String, Option<String>)> {
    let entries = chain_entries(path);
    chain(&entries, None)
        .into_iter()
        .filter(|entry| is_prompt(entry))
        .filter_map(|entry| {
            Some((
                entry["uuid"].as_str()?.to_owned(),
                entry["parentUuid"].as_str().map(str::to_owned),
            ))
        })
        .collect()
}

fn is_prompt(entry: &Value) -> bool {
    entry["type"] == "user" && entry["isCompactSummary"] != true && user_prompt(entry).is_some()
}

struct Replay {
    translator: Translator,
    subagents: HashMap<String, PathBuf>,
    updates: Vec<Value>,
    open: bool,
    last: Option<u64>,
}

impl Replay {
    fn entry(&mut self, entry: &Value) {
        let at = stamp(entry);
        if entry["type"] == "user" && interrupted(entry) {
            self.end_turn("cancelled", at);
            return;
        }
        if is_prompt(entry) {
            self.end_turn("completed", None);
            self.updates.push(turn_mark("started", at));
            self.open = true;
            self.last = at;
        }
        let updates = self.translator.history_entry(entry);
        self.push(updates, at);
        self.sidechains(entry);
    }

    fn sidechains(&mut self, entry: &Value) {
        if entry["type"] != "assistant" {
            return;
        }
        let calls = entry["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|block| block["type"] == "tool_use")
            .filter_map(|block| block["id"].as_str());
        for call in calls {
            let Some(file) = self.subagents.remove(call) else {
                continue;
            };
            for inner in chain_entries(&file) {
                let updates = self.translator.sidechain_entry(&inner, call);
                self.push(updates, stamp(&inner));
                self.sidechains(&inner);
            }
        }
    }

    fn push(&mut self, updates: Vec<Value>, at: Option<u64>) {
        if updates.is_empty() {
            return;
        }
        self.last = self.last.max(at);
        self.updates.extend(updates.into_iter().map(|mut update| {
            if let Some(at) = at {
                update["_meta"]["zz"]["at"] = Value::from(at);
            }
            update
        }));
    }

    fn end_turn(&mut self, outcome: &str, at: Option<u64>) {
        if std::mem::take(&mut self.open) {
            self.updates.push(turn_mark(outcome, at.or(self.last)));
        }
    }
}

fn stamp(entry: &Value) -> Option<u64> {
    let at = chrono::DateTime::parse_from_rfc3339(entry["timestamp"].as_str()?).ok()?;
    u64::try_from(at.timestamp_millis()).ok()
}

fn subagents(path: &Path) -> HashMap<String, PathBuf> {
    std::fs::read_dir(path.with_extension("").join("subagents"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter_map(|meta| {
            let agent = meta.file_name()?.to_str()?.strip_suffix(".meta.json")?;
            let transcript = meta.with_file_name(format!("{agent}.jsonl"));
            let meta = serde_json::from_str::<Value>(&std::fs::read_to_string(&meta).ok()?).ok()?;
            Some((meta["toolUseId"].as_str()?.to_owned(), transcript))
        })
        .collect()
}

fn chain_entries(path: &Path) -> Vec<Value> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut text = String::new();
    if file
        .take(MAX_TRANSCRIPT_BYTES)
        .read_to_string(&mut text)
        .is_err()
    {
        return Vec::new();
    }
    lines(&text)
        .filter(|entry| {
            matches!(
                entry["type"].as_str(),
                Some("user" | "assistant" | "progress" | "system" | "attachment")
            ) && entry["uuid"].is_string()
        })
        .collect()
}

fn chain<'a>(entries: &'a [Value], until: Option<&str>) -> Vec<&'a Value> {
    let by_uuid = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| Some((entry["uuid"].as_str()?, index)))
        .collect::<HashMap<_, _>>();
    let parent_of = |index: usize| {
        entries[index]["parentUuid"]
            .as_str()
            .and_then(|parent| by_uuid.get(parent))
            .copied()
    };
    let leaf = match until {
        Some(until) => by_uuid.get(until).copied(),
        None => leaf(entries, &by_uuid, parent_of),
    };
    let Some(leaf) = leaf else {
        return Vec::new();
    };
    let mut kept = HashSet::new();
    let mut current = Some(leaf);
    while let Some(index) = current {
        if !kept.insert(index) {
            break;
        }
        current = parent_of(index);
    }
    let messages = kept
        .iter()
        .filter_map(|index| message_id(&entries[*index]))
        .collect::<HashSet<_>>();
    let mut answered = kept
        .iter()
        .flat_map(|index| answers(&entries[*index]))
        .collect::<HashSet<_>>();
    for (index, entry) in entries.iter().enumerate().take(leaf) {
        if kept.contains(&index) {
            continue;
        }
        let sibling = message_id(entry).is_some_and(|message| messages.contains(message));
        let result = parent_of(index)
            .is_some_and(|parent| kept.contains(&parent) && message_id(&entries[parent]).is_some())
            && answers(entry).next().is_some()
            && answers(entry).all(|call| !answered.contains(call));
        if sibling || result {
            kept.insert(index);
            answered.extend(answers(entry));
        }
    }
    let mut kept = kept.into_iter().collect::<Vec<_>>();
    kept.sort_unstable();
    kept.into_iter()
        .map(|index| &entries[index])
        .filter(|entry| {
            matches!(
                entry["type"].as_str(),
                Some("user" | "assistant" | "attachment")
            ) && entry["isMeta"] != true
                && entry["isSidechain"] != true
                && entry["teamName"].is_null()
        })
        .collect()
}

fn leaf(
    entries: &[Value],
    by_uuid: &HashMap<&str, usize>,
    parent_of: impl Fn(usize) -> Option<usize>,
) -> Option<usize> {
    let parents = entries
        .iter()
        .filter_map(|entry| entry["parentUuid"].as_str())
        .collect::<HashSet<_>>();
    let mut leaves = Vec::new();
    for terminal in entries.iter().filter_map(|entry| {
        let uuid = entry["uuid"].as_str()?;
        (!parents.contains(uuid)).then(|| by_uuid.get(uuid).copied())?
    }) {
        let mut seen = HashSet::new();
        let mut current = Some(terminal);
        while let Some(index) = current {
            if !seen.insert(index) {
                break;
            }
            if matches!(entries[index]["type"].as_str(), Some("user" | "assistant")) {
                leaves.push(index);
                break;
            }
            current = parent_of(index);
        }
    }
    let main = leaves
        .iter()
        .copied()
        .filter(|leaf| {
            let leaf = &entries[*leaf];
            leaf["isSidechain"] != true && leaf["isMeta"] != true && leaf["teamName"].is_null()
        })
        .max();
    main.or_else(|| leaves.into_iter().max())
}

fn message_id(entry: &Value) -> Option<&str> {
    (entry["type"] == "assistant")
        .then(|| entry["message"]["id"].as_str())
        .flatten()
}

fn answers(entry: &Value) -> impl Iterator<Item = &str> {
    entry["message"]["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|block| entry["type"] == "user" && block["type"] == "tool_result")
        .filter_map(|block| block["tool_use_id"].as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_directories_match_claude_codes_naming() {
        assert_eq!(sanitize("/home/user/.cache/ws"), "-home-user--cache-ws");
        let long = format!("/{}", "a".repeat(250));
        let sanitized = sanitize(&long);
        assert!(sanitized.starts_with(&format!("-{}", "a".repeat(199))));
        assert_eq!(simple_hash("hello"), "1n1e4y");
    }

    #[test]
    fn listing_reads_titles_and_skips_sidechains() {
        let config = tempfile::tempdir().expect("tempdir");
        let cwd = config.path().join("work");
        std::fs::create_dir_all(&cwd).expect("cwd");
        let canonical = std::fs::canonicalize(&cwd).expect("canonical");
        let dir = config
            .path()
            .join("projects")
            .join(sanitize(&canonical.to_string_lossy()));
        std::fs::create_dir_all(&dir).expect("project dir");
        let first = "11111111-1111-1111-1111-111111111111";
        std::fs::write(
            dir.join(format!("{first}.jsonl")),
            format!(
                "{{\"type\":\"user\",\"uuid\":\"a\",\"parentUuid\":null,\"cwd\":\"{}\",\"message\":{{\"role\":\"user\",\"content\":\"fix the build\"}}}}\n",
                canonical.display()
            ),
        )
        .expect("session");
        let side = "22222222-2222-2222-2222-222222222222";
        std::fs::write(
            dir.join(format!("{side}.jsonl")),
            "{\"type\":\"user\",\"isSidechain\":true,\"uuid\":\"b\",\"message\":{\"role\":\"user\",\"content\":\"x\"}}\n",
        )
        .expect("sidechain");
        std::fs::write(dir.join("notes.jsonl"), "{}\n").expect("stray");
        let sessions = list(config.path(), Some(&cwd));
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, first);
        assert_eq!(sessions[0].title.as_deref(), Some("fix the build"));
        assert_eq!(sessions[0].cwd, canonical);
        assert!(
            sessions[0]
                .updated_at
                .as_deref()
                .is_some_and(|at| at.ends_with('Z'))
        );
        assert_eq!(
            session_file(config.path(), &cwd, first),
            Some(dir.join(format!("{first}.jsonl")))
        );
        let mut transcript = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.join(format!("{first}.jsonl")))
            .expect("transcript");
        std::io::Write::write_all(
            &mut transcript,
            b"{\"type\":\"custom-title\",\"customTitle\":\"Release notes\"}\n",
        )
        .expect("rename");
        let renamed = list(config.path(), Some(&cwd));
        assert_eq!(renamed[0].title.as_deref(), Some("Release notes"));
    }

    #[test]
    fn a_saved_title_takes_a_rename_over_the_generated_one() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("s.jsonl");
        let prompt = "{\"type\":\"user\",\"uuid\":\"a\",\"message\":{\"role\":\"user\",\"content\":\"fix the build\"}}";
        let custom = "{\"type\":\"custom-title\",\"customTitle\":\"Release notes\"}";
        let ai = "{\"type\":\"ai-title\",\"aiTitle\":\"Fix the build\"}";
        let saved = |lines: &[&str]| {
            std::fs::write(&path, lines.join("\n")).expect("transcript");
            title(&path)
        };
        assert_eq!(
            saved(&[prompt, custom, ai]).as_deref(),
            Some("Release notes")
        );
        assert_eq!(saved(&[prompt, ai]).as_deref(), Some("Fix the build"));
        assert_eq!(saved(&[prompt]), None);
    }

    #[test]
    fn a_transcript_follows_the_live_branch_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("s.jsonl");
        let lines = [
            r#"{"type":"user","uuid":"u1","parentUuid":null,"message":{"role":"user","content":"first"}}"#,
            r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","message":{"id":"m1","content":[{"type":"text","text":"abandoned"}]}}"#,
            r#"{"type":"assistant","uuid":"a2","parentUuid":"u1","message":{"id":"m2","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#,
            r#"{"type":"user","uuid":"u2","parentUuid":"a2","toolUseResult":{"stdout":"a.rs","stderr":""},"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"a.rs"}]}}"#,
            r#"{"type":"assistant","uuid":"a3","parentUuid":"u2","isSidechain":true,"message":{"id":"m3","content":[{"type":"text","text":"side"}]}}"#,
            r#"{"type":"assistant","uuid":"a4","parentUuid":"u2","message":{"id":"m4","content":[{"type":"text","text":"kept"}]}}"#,
        ];
        std::fs::write(&path, lines.join("\n")).expect("write");
        let updates = transcript(&path, PathBuf::from("/work"), None);
        let kinds = updates
            .iter()
            .map(|update| update["sessionUpdate"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                "session_info_update",
                "user_message_chunk",
                "tool_call",
                "tool_call_update",
                "agent_message_chunk",
                "session_info_update"
            ]
        );
        assert_eq!(updates[3]["content"][0]["content"]["text"], "a.rs");
        assert_eq!(updates[4]["content"]["text"], "kept");
    }

    #[test]
    fn prompts_mark_where_each_turn_can_be_cut_and_a_cut_drops_what_follows() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("s.jsonl");
        let lines = [
            r#"{"type":"user","uuid":"u1","parentUuid":null,"message":{"role":"user","content":"first"}}"#,
            r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","message":{"id":"m1","content":[{"type":"text","text":"one"}]}}"#,
            r#"{"type":"attachment","uuid":"x1","parentUuid":"a1"}"#,
            r#"{"type":"user","uuid":"u2","parentUuid":"x1","message":{"role":"user","content":[{"type":"text","text":"second"}]}}"#,
            r#"{"type":"assistant","uuid":"a2","parentUuid":"u2","message":{"id":"m2","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#,
            r#"{"type":"user","uuid":"u3","parentUuid":"a2","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"a.rs"}]}}"#,
            r#"{"type":"user","uuid":"u4","parentUuid":"u3","message":{"role":"user","content":"<task-notification>done</task-notification>"}}"#,
            r#"{"type":"assistant","uuid":"a3","parentUuid":"u4","message":{"id":"m3","content":[{"type":"text","text":"two"}]}}"#,
            r#"{"type":"user","uuid":"u5","parentUuid":"a3","message":{"role":"user","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAAA"}}]}}"#,
        ];
        std::fs::write(&path, lines.join("\n")).expect("write");
        assert_eq!(
            prompts(&path),
            [
                ("u1".to_owned(), None),
                ("u2".to_owned(), Some("x1".to_owned())),
                ("u5".to_owned(), Some("a3".to_owned()))
            ]
        );
        let kept = transcript(&path, PathBuf::from("/work"), Some("x1"));
        let texts = kept
            .iter()
            .filter_map(|update| update["content"]["text"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(texts, ["first", "one"]);
        assert!(transcript(&path, PathBuf::from("/work"), Some("missing")).is_empty());
    }

    #[test]
    fn a_resumed_chat_keeps_parallel_tool_results_and_hides_the_interrupt_marker() {
        use std::time::Instant;

        use zz_protocol::agent_stream::AgentStreamPayload;
        use zz_protocol::chat::{ChatView, ItemKind, ItemStatus, TurnOutcome};

        use crate::agent::{chat_bridge::Shadow, turn};

        const START: u64 = 1_794_240_000_000;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("s.jsonl");
        let lines = [
            r#"{"type":"user","uuid":"u1","parentUuid":null,"timestamp":"2026-11-09T16:00:00.000Z","message":{"role":"user","content":"run both"}}"#,
            r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2026-11-09T16:00:01.000Z","message":{"id":"m1","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#,
            r#"{"type":"assistant","uuid":"a2","parentUuid":"a1","timestamp":"2026-11-09T16:00:01.000Z","message":{"id":"m1","content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"false"}}]}}"#,
            r#"{"type":"user","uuid":"r1","parentUuid":"a1","timestamp":"2026-11-09T16:00:02.000Z","toolUseResult":{"stdout":"a.rs","stderr":""},"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"a.rs"}]}}"#,
            r#"{"type":"user","uuid":"r2","parentUuid":"a2","timestamp":"2026-11-09T16:00:03.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t2","is_error":true,"content":"Exit code 1\nboom"}]}}"#,
            r#"{"type":"assistant","uuid":"a3","parentUuid":"r2","timestamp":"2026-11-09T16:00:04.000Z","message":{"id":"m2","content":[{"type":"text","text":"one failed"}]}}"#,
            r#"{"type":"user","uuid":"u2","parentUuid":"a3","timestamp":"2026-11-09T16:00:05.000Z","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#,
        ];
        std::fs::write(&path, lines.join("\n")).expect("write");
        assert_eq!(prompts(&path), [("u1".to_owned(), None)]);
        let mut shadow = Shadow::recording();
        for update in transcript(&path, PathBuf::from("/work"), None) {
            shadow.payload(&AgentStreamPayload::Update { update });
        }
        shadow.turn(&turn::Fact::SessionReady, Some("s1"), Instant::now());
        assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
        let mut view = ChatView::default();
        for event in &shadow.events {
            view.apply(event)
                .unwrap_or_else(|skip| panic!("skipped {event:?}: {skip:?}"));
        }
        let [turn] = view.turns.as_slice() else {
            panic!("expected one turn, got {:?}", view.turns);
        };
        let rows = turn
            .items
            .iter()
            .map(|id| {
                let item = view.item(id).expect("item");
                (
                    id.as_str(),
                    item.kind,
                    item.status.clone(),
                    item.body.text.as_str(),
                    item.body.exit_code,
                    item.started_at - START,
                    item.ended_at.map(|at| at - START),
                )
            })
            .collect::<Vec<_>>();
        let failed = ItemStatus::Failed {
            message: Some("boom".to_owned()),
        };
        assert_eq!(
            rows,
            [
                (
                    "u:u1",
                    ItemKind::User,
                    ItemStatus::Completed,
                    "run both",
                    None,
                    0,
                    Some(0)
                ),
                (
                    "t1",
                    ItemKind::Execute,
                    ItemStatus::Completed,
                    "a.rs",
                    None,
                    1000,
                    Some(2000)
                ),
                (
                    "t2",
                    ItemKind::Execute,
                    failed,
                    "Exit code 1\nboom",
                    Some(1),
                    1000,
                    Some(3000)
                ),
                (
                    "a:m2#1",
                    ItemKind::Agent,
                    ItemStatus::Completed,
                    "one failed",
                    None,
                    4000,
                    Some(5000)
                ),
            ]
        );
        assert_eq!(
            (turn.outcome.clone(), turn.started_at, turn.ended_at),
            (Some(TurnOutcome::Cancelled), START, Some(START + 5000))
        );
    }

    #[test]
    fn timestamps_render_as_utc() {
        assert_eq!(
            rfc3339(1_791_399_341_959).as_deref(),
            Some("2026-10-07T18:55:41Z")
        );
    }
}
