use std::{
    fs::File,
    io::{BufRead as _, BufReader, Read as _},
    path::{Path, PathBuf},
};

use serde_json::{Map, Value, json};

const META_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RolloutMeta {
    pub(crate) thread: String,
    pub(crate) parent: Option<String>,
    pub(crate) cwd: PathBuf,
}

impl RolloutMeta {
    pub(crate) fn read(path: &Path) -> Option<Self> {
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
        let parent = meta["parent_thread_id"]
            .as_str()
            .or_else(|| meta["source"]["subagent"]["thread_spawn"]["parent_thread_id"].as_str())
            .map(str::to_owned);
        Some(Self {
            thread: meta["id"].as_str()?.to_owned(),
            parent,
            cwd: PathBuf::from(meta["cwd"].as_str().unwrap_or("/")),
        })
    }
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

const QUESTION_REPLY: &str = "<send_user_message_question_reply>";
const QUESTION_REPLY_END: &str = "</send_user_message_question_reply>";
const SHELLS: [&str; 4] = ["bash", "zsh", "sh", "dash"];

fn script(parts: &[Value]) -> Option<&str> {
    let [shell, flag, script] = parts else {
        return None;
    };
    let shell = Path::new(shell.as_str()?).file_name()?.to_str()?;
    if SHELLS.contains(&shell) && matches!(flag.as_str(), Some("-c" | "-lc")) {
        script.as_str()
    } else {
        None
    }
}

fn command_text(command: &Value) -> String {
    match command {
        Value::Array(parts) => script(parts).map_or_else(
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
    }
}

fn strip_file_url(path: &Value) -> Value {
    path.as_str().map_or_else(
        || path.clone(),
        |path| Value::from(path.trim_start_matches("file://")),
    )
}

fn file_changes(changes: &Value) -> Value {
    let Some(changes) = changes.as_object() else {
        return camelize(changes);
    };
    let lines = |text: &str, sign: char| {
        let count = text.lines().count();
        let header = if sign == '+' {
            format!("@@ -0,0 +1,{count} @@\n")
        } else {
            format!("@@ -1,{count} +0,0 @@\n")
        };
        text.lines().fold(header, |mut all, line| {
            all.push(sign);
            all.push_str(line);
            all.push('\n');
            all
        })
    };
    Value::Array(
        changes
            .iter()
            .map(|(path, change)| {
                let kind = change["type"].as_str().unwrap_or("update");
                let content = change["content"].as_str().unwrap_or_default();
                let diff = match kind {
                    "add" => lines(content, '+'),
                    "delete" => lines(content, '-'),
                    _ => change["unified_diff"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                };
                let mut entry = json!({ "path": path, "kind": { "type": kind }, "diff": diff });
                if let Some(target) = change["move_path"].as_str() {
                    entry["kind"]["movePath"] = Value::from(target);
                }
                entry
            })
            .collect(),
    )
}

fn questions(item: &Value) -> Option<Value> {
    let asked = item["questions"]
        .as_array()
        .filter(|asked| !asked.is_empty())?;
    Some(Value::Array(
        asked
            .iter()
            .enumerate()
            .map(|(index, question)| {
                let title = question["title"]
                    .as_str()
                    .or_else(|| question["question"].as_str())
                    .unwrap_or_default();
                let options = question["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|label| json!({ "label": label }))
                    .collect::<Vec<_>>();
                json!({ "id": format!("q{index}"), "question": title, "options": options })
            })
            .collect(),
    ))
}

fn replies(text: &str) -> Option<Value> {
    let body = text.trim().strip_prefix(QUESTION_REPLY)?.trim_end();
    let body = body.strip_suffix(QUESTION_REPLY_END).unwrap_or(body);
    let entries = serde_json::from_str::<Value>(body.trim()).unwrap_or_default();
    Some(Value::Array(
        entries
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let item = serde_json::from_str::<Value>(entry["questionItemId"].as_str()?).ok()?;
                Some(json!({
                    "callId": item[1].as_str()?,
                    "index": item[2].as_u64().unwrap_or_default(),
                    "question": entry["question"],
                    "answer": entry["answer"].as_str().unwrap_or_default(),
                }))
            })
            .collect(),
    ))
}

fn user_message(item: &Value) -> Value {
    let reply = texts(&item["content"])
        .first()
        .and_then(|text| replies(text));
    match reply {
        Some(replies) => json!({ "type": "userInputReply", "id": item["id"], "replies": replies }),
        None => json!({ "type": "userMessage", "id": item["id"], "content": item["content"] }),
    }
}

fn extension(item: &Value) -> Option<Value> {
    match item["kind"].as_str()? {
        "web.search" => Some(json!({
            "type": "webSearch",
            "id": item["id"],
            "query": item["query"],
            "action": camelize(&item["action"]),
            "results": item["results"],
            "status": "completed",
        })),
        "clock.sleep" => Some(json!({
            "type": "extension",
            "id": item["id"],
            "kind": "clock.sleep",
            "durationMs": item["durationMs"],
            "status": "completed",
        })),
        _ => None,
    }
}

pub(crate) fn v2_item(item: &Value) -> Option<Value> {
    let kind = item["type"].as_str()?;
    let mut characters = kind.chars();
    let kind = characters
        .next()
        .map(|first| first.to_lowercase().chain(characters).collect::<String>())?;
    let id = item["id"].clone();
    match kind.as_str() {
        "agentMessage" => Some(questions(item).map_or_else(
            || {
                json!({
                    "type": "agentMessage",
                    "id": id,
                    "text": texts(&item["content"]).join("\n\n"),
                })
            },
            |questions| json!({ "type": "userInputRequest", "id": id, "questions": questions }),
        )),
        "reasoning" => Some(json!({
            "type": "reasoning",
            "id": id,
            "summary": texts(&item["summary_text"]),
            "content": texts(&item["raw_content"]),
        })),
        "subAgentActivity" if item["kind"] == "interacted" => None,
        "userMessage" => Some(user_message(item)),
        "extension" => extension(item),
        "commandExecution" => {
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
                "command": command_text(&item["command"]),
                "cwd": item["cwd"].as_str().map(|cwd| cwd.trim_start_matches("file://")),
                "status": item["status"],
                "aggregatedOutput": item["aggregated_output"],
                "exitCode": item["exit_code"],
                "commandActions": actions,
            }))
        }
        "fileChange" => Some(json!({
            "type": "fileChange",
            "id": id,
            "status": item["status"],
            "changes": file_changes(&item["changes"]),
        })),
        _ => {
            let mut converted = camelize(item);
            converted["type"] = Value::from(kind.as_str());
            if kind == "imageView" {
                converted["path"] = strip_file_url(&item["path"]);
            }
            if kind != "subAgentActivity" && converted["status"].is_null() {
                converted["status"] = Value::from("completed");
            }
            Some(converted)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let empty = v2_item(&json!({
            "type": "Reasoning", "id": "rs2", "summary_text": [], "raw_content": [],
        }))
        .unwrap();
        assert_eq!(empty["summary"], json!([]));
        assert!(v2_item(&json!({ "type": "SubAgentActivity", "kind": "interacted" })).is_none());
        let started = v2_item(&json!({
            "type": "SubAgentActivity", "id": "call_1", "kind": "started",
            "agent_thread_id": "t1", "agent_path": "/root/lens_counter",
        }))
        .unwrap();
        assert_eq!(started["agentThreadId"], "t1");
        assert!(
            started["status"].is_null(),
            "an agent row closes on its own record"
        );
    }

    fn command(parts: &Value) -> Value {
        v2_item(&json!({
            "type": "CommandExecution", "id": "c", "command": parts,
            "cwd": "file:///work", "status": "completed", "parsed_cmd": [],
        }))
        .unwrap()
    }

    #[test]
    fn a_shell_wrapper_leaves_only_its_script() {
        let script = "for i in $(seq 1 20); do echo tick $i; sleep 1; done";
        for shell in ["/opt/homebrew/bin/bash", "/bin/zsh", "sh", "/usr/bin/dash"] {
            for flag in ["-c", "-lc"] {
                assert_eq!(command(&json!([shell, flag, script]))["command"], script);
            }
        }
        assert_eq!(
            command(&json!(["python3", "-c", "print(1)"]))["command"],
            "python3 -c print(1)"
        );
        assert_eq!(command(&json!("ls -la"))["command"], "ls -la");
        assert_eq!(command(&json!([]))["cwd"], "/work");
    }

    #[test]
    fn a_file_change_map_becomes_a_list_with_its_paths_intact() {
        let item = v2_item(&json!({
            "type": "FileChange", "id": "f", "status": "completed",
            "changes": {
                "/work/my_file.rs": { "type": "add", "content": "fn main() {}\nfn other() {}\n" },
                "/work/old_name.txt": {
                    "type": "update", "unified_diff": "@@ -1 +1 @@\n-a\n+b\n",
                    "move_path": "/work/new_name.txt",
                },
                "/work/gone_file.txt": { "type": "delete", "content": "---\ntitle: bye\n---\n" },
            },
        }))
        .unwrap();
        let changes = item["changes"].as_array().expect("a list of changes");
        let change = |path: &str| {
            changes
                .iter()
                .find(|change| change["path"] == path)
                .unwrap_or_else(|| panic!("{path} missing from {changes:?}"))
        };
        assert_eq!(change("/work/my_file.rs")["kind"]["type"], "add");
        assert_eq!(
            change("/work/my_file.rs")["diff"],
            "@@ -0,0 +1,2 @@\n+fn main() {}\n+fn other() {}\n"
        );
        assert_eq!(
            change("/work/old_name.txt")["diff"],
            "@@ -1 +1 @@\n-a\n+b\n"
        );
        assert_eq!(
            change("/work/old_name.txt")["kind"]["movePath"],
            "/work/new_name.txt"
        );
        assert_eq!(
            change("/work/gone_file.txt")["diff"],
            "@@ -1,3 +0,0 @@\n----\n-title: bye\n----\n"
        );
        assert_eq!(change("/work/gone_file.txt")["kind"]["type"], "delete");
    }

    #[test]
    fn an_async_question_becomes_a_request_and_its_reply_an_answer() {
        let asked = v2_item(&json!({
            "type": "AgentMessage", "id": "call_q", "phase": "final_answer", "delivery": "async",
            "content": [{ "type": "Text", "text": "Which one?\n- A\n- B" }],
            "questions": [{ "title": "Which one?", "options": ["A", "B"] }],
        }))
        .unwrap();
        assert_eq!(
            asked,
            json!({ "type": "userInputRequest", "id": "call_q", "questions": [{
                "id": "q0", "question": "Which one?", "options": [{ "label": "A" }, { "label": "B" }],
            }] })
        );
        let plain = v2_item(&json!({
            "type": "AgentMessage", "id": "m", "phase": "commentary", "delivery": "async",
            "content": [{ "type": "Text", "text": "Still working." }],
        }))
        .unwrap();
        assert_eq!(
            plain,
            json!({ "type": "agentMessage", "id": "m", "text": "Still working." })
        );
        let text = format!(
            "<send_user_message_question_reply>\n{}\n</send_user_message_question_reply>\n",
            json!([{
                "questionItemId": json!(["request_user_input_async", "call_q", 0]).to_string(),
                "question": "Which one?", "answer": "B",
            }])
        );
        let reply = v2_item(&json!({
            "type": "UserMessage", "id": "u", "content": [{ "type": "text", "text": text }],
        }))
        .unwrap();
        assert_eq!(
            reply,
            json!({ "type": "userInputReply", "id": "u", "replies": [{
                "callId": "call_q", "index": 0, "question": "Which one?", "answer": "B",
            }] })
        );
        let garbled = v2_item(&json!({
            "type": "UserMessage", "id": "u3",
            "content": [{ "type": "text", "text": "<send_user_message_question_reply>\n[{\"quest" }],
        }))
        .unwrap();
        assert_eq!(
            garbled,
            json!({ "type": "userInputReply", "id": "u3", "replies": [] })
        );
        let prompt = v2_item(&json!({
            "type": "UserMessage", "id": "u2", "content": [{ "type": "text", "text": "hello" }],
        }))
        .unwrap();
        assert_eq!(prompt["type"], "userMessage");
        assert_eq!(prompt["content"][0]["text"], "hello");
    }

    #[test]
    fn items_without_a_status_are_complete_when_recorded() {
        let compaction = v2_item(&json!({ "type": "ContextCompaction", "id": "cc" })).unwrap();
        assert_eq!(
            compaction,
            json!({ "type": "contextCompaction", "id": "cc", "status": "completed" })
        );
        let image = v2_item(&json!({
            "type": "ImageView", "id": "iv", "path": "file:///tmp/shot_one.png",
        }))
        .unwrap();
        assert_eq!(image["path"], "/tmp/shot_one.png");
        assert_eq!(image["status"], "completed");
        let change =
            v2_item(&json!({ "type": "WebSearch", "id": "w", "search_query": "zz" })).unwrap();
        assert_eq!(
            change,
            json!({ "type": "webSearch", "id": "w", "searchQuery": "zz", "status": "completed" })
        );
    }

    #[test]
    fn extensions_become_web_rows_and_waits() {
        let web = v2_item(&json!({
            "type": "Extension", "kind": "web.search", "id": "x", "query": "https://example.com",
            "action": { "type": "openPage", "url": "https://example.com" },
            "results": [{ "title": "Example Domain", "url": "https://example.com/" }],
        }))
        .unwrap();
        assert_eq!(web["type"], "webSearch");
        assert_eq!(web["action"]["url"], "https://example.com");
        assert_eq!(web["status"], "completed");
        let sleep = v2_item(&json!({
            "type": "Extension", "kind": "clock.sleep", "id": "s", "durationMs": 60000,
        }))
        .unwrap();
        assert_eq!(
            sleep,
            json!({ "type": "extension", "id": "s", "kind": "clock.sleep",
                "durationMs": 60000, "status": "completed" })
        );
        assert!(v2_item(&json!({ "type": "Extension", "kind": "mystery", "id": "m" })).is_none());
    }
}
