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
        assert!(v2_item(&json!({ "type": "SubAgentActivity", "kind": "interacted" })).is_none());
        let change =
            v2_item(&json!({ "type": "WebSearch", "id": "w", "search_query": "zz" })).unwrap();
        assert_eq!(
            change,
            json!({ "type": "webSearch", "id": "w", "searchQuery": "zz" })
        );
    }
}
