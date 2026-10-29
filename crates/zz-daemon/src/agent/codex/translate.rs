use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

const MAX_TITLE_CHARS: usize = 500;

#[derive(Default)]
pub(crate) struct Translator {
    cwd: PathBuf,
    streamed: HashSet<String>,
    commands: HashMap<String, String>,
    prompts: HashSet<String>,
    agents: HashMap<String, String>,
    spawned: HashMap<String, String>,
    finished: HashSet<String>,
    parents: HashMap<String, String>,
    pending: HashMap<String, Vec<(String, Value)>>,
    restoring: bool,
}

const PENDING_THREADS: usize = 32;
const PENDING_FRAMES: usize = 512;

impl Translator {
    pub(crate) fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            ..Self::default()
        }
    }

    pub(crate) fn expect_prompt(&mut self, text: &str) {
        self.prompts.insert(text.trim().to_owned());
    }

    pub(crate) fn notification(&mut self, method: &str, params: &Value) -> Vec<Value> {
        match method {
            "item/started" => self.started(&params["item"]),
            "item/completed" => self.completed(&params["item"]),
            "item/agentMessage/delta" => self.delta("agent_message_chunk", params),
            "item/reasoning/summaryTextDelta" | "item/reasoning/textDelta" => {
                self.delta("agent_thought_chunk", params)
            }
            "item/commandExecution/outputDelta" => {
                let (Some(id), Some(delta)) = (params["itemId"].as_str(), params["delta"].as_str())
                else {
                    return Vec::new();
                };
                let output = self.commands.entry(id.to_owned()).or_default();
                output.push_str(delta);
                vec![json!({
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": id,
                    "content": [text_content(&tail(output))],
                })]
            }
            "turn/plan/updated" => vec![json!({
                "sessionUpdate": "plan",
                "entries": params["plan"].as_array().into_iter().flatten().map(|step| json!({
                    "content": step["step"].as_str().unwrap_or_default(),
                    "priority": "medium",
                    "status": match step["status"].as_str() {
                        Some("inProgress" | "in_progress") => "in_progress",
                        Some("completed") => "completed",
                        _ => "pending",
                    },
                })).collect::<Vec<_>>(),
            })],
            "thread/tokenUsage/updated" => {
                let usage = &params["tokenUsage"];
                let Some(size) = usage["modelContextWindow"].as_u64() else {
                    return Vec::new();
                };
                let used = usage["last"]["totalTokens"].as_u64().unwrap_or_default();
                vec![json!({ "sessionUpdate": "usage_update", "used": used, "size": size })]
            }
            _ => Vec::new(),
        }
    }

    pub(crate) fn child(
        &mut self,
        thread: &str,
        method: &str,
        params: &Value,
    ) -> Option<Vec<Value>> {
        let Some(parent) = self.agents.get(thread).cloned() else {
            let lifecycle = method.starts_with("turn/") || method == "item/completed";
            let delta = method.ends_with("/delta") || method.ends_with("Delta");
            if !delta && (self.pending.len() < PENDING_THREADS || self.pending.contains_key(thread))
            {
                let frames = self.pending.entry(thread.to_owned()).or_default();
                if frames.len() < PENDING_FRAMES
                    || method.starts_with("turn/")
                    || (lifecycle && frames.len() < PENDING_FRAMES * 2)
                {
                    frames.push((method.to_owned(), params.clone()));
                }
            }
            return None;
        };
        match method {
            "turn/started" => return Some(self.agent_turn(thread, &parent)),
            "turn/completed" => return Some(self.agent_done(&parent, params)),
            _ => {}
        }
        let item = &params["item"];
        let mut updates = match (method, item["type"].as_str()) {
            ("item/completed", Some("agentMessage")) => item["text"]
                .as_str()
                .filter(|text| !text.trim().is_empty())
                .map(|text| {
                    vec![json!({
                        "sessionUpdate": "tool_call_update",
                        "toolCallId": parent,
                        "content": [text_content(&tail(text))],
                    })]
                })
                .unwrap_or_default(),
            (
                "item/started" | "item/completed",
                Some("userMessage" | "agentMessage" | "reasoning" | "plan"),
            ) => Vec::new(),
            ("item/started" | "item/completed" | "item/commandExecution/outputDelta", _) => {
                self.notification(method, params)
            }
            _ => Vec::new(),
        };
        for update in &mut updates {
            if update["sessionUpdate"] == "tool_call" && update["_meta"]["zz"]["parent"].is_null() {
                update["_meta"]["zz"]["parent"] = Value::from(parent.clone());
                if let Some(id) = update["toolCallId"].as_str()
                    && self.spawned.contains_key(id)
                {
                    self.parents.insert(id.to_owned(), parent.clone());
                }
            }
        }
        Some(updates)
    }

    pub(crate) fn child_history(&mut self, thread: &str, item: &Value) -> Vec<Value> {
        let restoring = std::mem::replace(&mut self.restoring, true);
        let updates = self.child_history_item(thread, item);
        self.restoring = restoring;
        updates
    }

    fn child_history_item(&mut self, thread: &str, item: &Value) -> Vec<Value> {
        let params = json!({ "item": item });
        let mut updates = self
            .child(thread, "item/started", &params)
            .unwrap_or_default();
        updates.extend(
            self.child(thread, "item/completed", &params)
                .unwrap_or_default(),
        );
        updates
    }

    pub(crate) fn history_item(&mut self, item: &Value) -> Vec<Value> {
        if item["type"] == "userMessage" {
            return user_text(item)
                .map(|text| vec![chunk("user_message_chunk", &prompt_id(item), &text)])
                .unwrap_or_default();
        }
        let mut updates = self.history_start(item);
        updates.extend(self.history_end(item));
        updates
    }

    pub(crate) fn history_start(&mut self, item: &Value) -> Vec<Value> {
        if item["type"] == "userMessage" {
            return Vec::new();
        }
        self.restoring = true;
        let updates = self.started(item);
        self.restoring = false;
        updates
    }

    pub(crate) fn history_end(&mut self, item: &Value) -> Vec<Value> {
        if item["type"] == "userMessage" {
            return self.history_item(item);
        }
        self.restoring = true;
        let updates = self.completed(item);
        self.restoring = false;
        updates
    }

    fn delta(&mut self, kind: &str, params: &Value) -> Vec<Value> {
        let (Some(id), Some(delta)) = (params["itemId"].as_str(), params["delta"].as_str()) else {
            return Vec::new();
        };
        if delta.is_empty() {
            return Vec::new();
        }
        self.streamed.insert(id.to_owned());
        vec![chunk(kind, id, delta)]
    }

    fn started(&mut self, item: &Value) -> Vec<Value> {
        let id = id_of(item);
        if item["type"] == "subAgentActivity" {
            return self.activity(&id, item);
        }
        let adopted = if item["tool"] == "spawnAgent" {
            self.adopt(&id, item)
        } else {
            Vec::new()
        };
        let Some(info) = tool_info(item, &self.cwd) else {
            return Vec::new();
        };
        if item["tool"] == "spawnAgent" {
            self.spawned.insert(id.clone(), info.title.clone());
        }
        let mut call = json!({
            "sessionUpdate": "tool_call",
            "toolCallId": id,
            "title": info.title,
            "kind": info.kind,
            "status": "in_progress",
            "rawInput": info.input,
            "_meta": { "codex": { "itemType": item["type"] } },
        });
        if !info.content.is_empty() {
            call["content"] = Value::Array(info.content);
        }
        if !info.locations.is_empty() {
            call["locations"] = Value::Array(info.locations);
        }
        let mut updates = vec![call];
        for thread in adopted {
            updates.extend(self.release(&thread));
        }
        updates
    }

    fn completed(&mut self, item: &Value) -> Vec<Value> {
        let id = id_of(item);
        let mut updates = Vec::new();
        if item["tool"] == "spawnAgent" {
            for thread in self.adopt(&id, item) {
                updates.extend(self.release(&thread));
            }
        }
        updates.extend(self.completed_item(&id, item));
        updates
    }

    fn release(&mut self, thread: &str) -> Vec<Value> {
        let Some(frames) = self.pending.remove(thread) else {
            return Vec::new();
        };
        frames
            .into_iter()
            .flat_map(|(method, params)| self.child(thread, &method, &params).unwrap_or_default())
            .collect()
    }

    fn completed_item(&mut self, id: &str, item: &Value) -> Vec<Value> {
        let id = id.to_owned();
        match item["type"].as_str() {
            Some("userMessage") => {
                let Some(text) = user_text(item) else {
                    return Vec::new();
                };
                if self.prompts.remove(text.trim()) {
                    return Vec::new();
                }
                vec![chunk("user_message_chunk", &prompt_id(item), &text)]
            }
            Some("agentMessage" | "plan") => {
                self.whole(&id, "agent_message_chunk", item["text"].as_str())
            }
            Some("reasoning") => {
                let text = item["summary"]
                    .as_array()
                    .into_iter()
                    .chain(item["content"].as_array())
                    .flatten()
                    .filter_map(|part| part.as_str().or_else(|| part["text"].as_str()))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                self.whole(&id, "agent_thought_chunk", Some(&text))
            }
            Some(_) => {
                let Some(mut status) = tool_status(item) else {
                    return Vec::new();
                };
                if status == "completed"
                    && !self.restoring
                    && self.spawned.contains_key(&id)
                    && !self.finished.contains(&id)
                    && self.agents.values().any(|row| *row == id)
                {
                    status = "in_progress";
                }
                let mut update = json!({
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": id,
                    "status": status,
                });
                if let Some(code) = item["exitCode"].as_i64() {
                    update["_meta"] = json!({ "zz": { "exitCode": code } });
                }
                let streamed = self.commands.remove(&id);
                if let Some(output) = tool_output(item)
                    .or(streamed)
                    .filter(|output| !output.trim().is_empty())
                {
                    update["content"] = json!([text_content(&tail(&output))]);
                }
                vec![update]
            }
            None => Vec::new(),
        }
    }

    fn agent_turn(&mut self, thread: &str, row: &str) -> Vec<Value> {
        if !self.finished.contains(row) {
            return Vec::new();
        }
        let title = self.spawned.get(row).cloned().unwrap_or_default();
        let base = row.split('~').next().unwrap_or(row);
        let mut next = 2;
        while self.spawned.contains_key(&format!("{base}~{next}")) {
            next += 1;
        }
        let id = format!("{base}~{next}");
        self.spawned.insert(id.clone(), title.clone());
        self.agents.insert(thread.to_owned(), id.clone());
        let mut call = json!({
            "sessionUpdate": "tool_call",
            "toolCallId": id,
            "title": title,
            "kind": "think",
            "status": "in_progress",
            "rawInput": { "agentThreadId": thread },
            "_meta": { "codex": { "itemType": "collabAgentToolCall" } },
        });
        if let Some(parent) = self.parents.get(row).cloned() {
            call["_meta"]["zz"] = json!({ "parent": parent });
            self.parents.insert(id, parent);
        }
        vec![call]
    }

    fn agent_done(&mut self, row: &str, params: &Value) -> Vec<Value> {
        if !self.spawned.contains_key(row) || !self.finished.insert(row.to_owned()) {
            return Vec::new();
        }
        let status = match params["turn"]["status"].as_str() {
            Some("failed" | "interrupted") => "failed",
            _ => "completed",
        };
        vec![json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": row,
            "status": status,
        })]
    }

    fn adopt(&mut self, id: &str, item: &Value) -> Vec<String> {
        let mut adopted = Vec::new();
        for thread in item["receiverThreadIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if self
                .agents
                .insert(thread.to_owned(), id.to_owned())
                .is_none()
            {
                adopted.push(thread.to_owned());
            }
        }
        adopted
    }

    fn activity(&mut self, id: &str, item: &Value) -> Vec<Value> {
        let Some(thread) = item["agentThreadId"].as_str() else {
            return Vec::new();
        };
        let status = match item["kind"].as_str() {
            Some("completed") => "completed",
            Some("interrupted") => "failed",
            _ => "in_progress",
        };
        if let Some(row) = self.agents.get(thread) {
            return vec![json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": row,
                "status": status,
            })];
        }
        self.agents.insert(thread.to_owned(), id.to_owned());
        let thread = thread.to_owned();
        let name = item["agentPath"]
            .as_str()
            .and_then(|path| path.rsplit('/').find(|part| !part.is_empty()))
            .unwrap_or("agent")
            .replace(['_', '-'], " ");
        let mut title = name.chars();
        let title = title.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(title).collect()
        });
        let mut updates = vec![json!({
            "sessionUpdate": "tool_call",
            "toolCallId": id,
            "title": clip_title(&title),
            "kind": "think",
            "status": status,
            "rawInput": { "agentPath": item["agentPath"], "agentThreadId": thread },
            "_meta": { "codex": { "itemType": "subAgentActivity" } },
        })];
        updates.extend(self.release(&thread));
        updates
    }

    fn whole(&mut self, id: &str, kind: &str, text: Option<&str>) -> Vec<Value> {
        if self.streamed.remove(id) {
            return Vec::new();
        }
        text.filter(|text| !text.trim().is_empty())
            .map(|text| vec![chunk(kind, id, text)])
            .unwrap_or_default()
    }
}

fn id_of(item: &Value) -> String {
    item["id"].as_str().unwrap_or("item").to_owned()
}

fn prompt_id(item: &Value) -> String {
    item["clientId"]
        .as_str()
        .map_or_else(|| id_of(item), str::to_owned)
}

fn user_text(item: &Value) -> Option<String> {
    let text = item["content"]
        .as_array()?
        .iter()
        .filter(|part| part["type"] == "text")
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    (!text.trim().is_empty()).then_some(text)
}

fn chunk(kind: &str, message_id: &str, text: &str) -> Value {
    json!({
        "sessionUpdate": kind,
        "messageId": message_id,
        "content": { "type": "text", "text": text },
    })
}

fn text_content(text: &str) -> Value {
    json!({ "type": "content", "content": { "type": "text", "text": text } })
}

const MAX_OUTPUT_CHARS: usize = 256 * 1024;

fn tail(text: &str) -> String {
    let count = text.chars().count();
    if count <= MAX_OUTPUT_CHARS {
        return text.trim_end().to_owned();
    }
    text.chars()
        .skip(count - MAX_OUTPUT_CHARS)
        .collect::<String>()
}

fn tool_status(item: &Value) -> Option<&'static str> {
    let status = item["status"].as_str()?;
    Some(match status {
        "completed" if item["exitCode"].as_i64().is_some_and(|code| code != 0) => "failed",
        "failed" | "declined" | "error" => "failed",
        "inProgress" | "in_progress" => "in_progress",
        _ => "completed",
    })
}

fn tool_output(item: &Value) -> Option<String> {
    match item["type"].as_str()? {
        "commandExecution" => item["aggregatedOutput"].as_str().map(str::to_owned),
        "mcpToolCall" => {
            if let Some(error) = item["error"]["message"].as_str() {
                return Some(error.to_owned());
            }
            let parts = item["result"]["content"]
                .as_array()?
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>();
            (!parts.is_empty()).then(|| parts.join("\n"))
        }
        "dynamicToolCall" => {
            let parts = item["contentItems"]
                .as_array()?
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>();
            (!parts.is_empty()).then(|| parts.join("\n"))
        }
        "collabAgentToolCall" => item["agentsStates"]
            .as_object()
            .map(|states| {
                states
                    .values()
                    .filter_map(|state| state["message"].as_str().or(state["status"].as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|text| !text.is_empty()),
        _ => None,
    }
}

pub(crate) struct ToolInfo {
    pub(crate) title: String,
    pub(crate) kind: &'static str,
    pub(crate) input: Value,
    pub(crate) content: Vec<Value>,
    pub(crate) locations: Vec<Value>,
}

pub(crate) fn tool_info(item: &Value, cwd: &Path) -> Option<ToolInfo> {
    let (title, kind, content, locations) = match item["type"].as_str()? {
        "commandExecution" => {
            let action = item["commandActions"]
                .as_array()
                .and_then(|actions| actions.first());
            let command = action
                .and_then(|action| action["command"].as_str())
                .or_else(|| item["command"].as_str())
                .unwrap_or("command");
            match action.and_then(|action| action["type"].as_str()) {
                Some("read") => {
                    let path = action.and_then(|action| action["path"].as_str());
                    let shown =
                        path.map_or_else(|| command.to_owned(), |path| display_path(path, cwd));
                    (
                        format!("Read {shown}"),
                        "read",
                        Vec::new(),
                        path.map(|path| vec![json!({ "path": path })])
                            .unwrap_or_default(),
                    )
                }
                Some("listFiles" | "search") => {
                    (command.to_owned(), "search", Vec::new(), Vec::new())
                }
                _ => (command.to_owned(), "execute", Vec::new(), Vec::new()),
            }
        }
        "fileChange" => {
            let changes = item["changes"].as_array().cloned().unwrap_or_default();
            let paths = changes
                .iter()
                .filter_map(|change| change["path"].as_str())
                .collect::<Vec<_>>();
            let title = match paths.as_slice() {
                [only] => format!("Edit {}", display_path(only, cwd)),
                [] => "Edit".to_owned(),
                many => format!("Edit {} files", many.len()),
            };
            let content = changes
                .iter()
                .filter_map(|change| {
                    let path = change["path"].as_str()?;
                    let (old, new) = split_diff(change["diff"].as_str().unwrap_or_default());
                    let created = change["kind"]["type"] == "add";
                    Some(json!({
                        "type": "diff",
                        "path": path,
                        "oldText": if created { Value::Null } else { Value::from(old) },
                        "newText": new,
                    }))
                })
                .collect();
            let locations = paths.iter().map(|path| json!({ "path": path })).collect();
            (title, "edit", content, locations)
        }
        "mcpToolCall" => (
            format!(
                "{}: {}",
                item["server"].as_str().unwrap_or("mcp"),
                item["tool"].as_str().unwrap_or("tool")
            ),
            "other",
            Vec::new(),
            Vec::new(),
        ),
        "dynamicToolCall" => (
            item["tool"].as_str().unwrap_or("tool").to_owned(),
            "other",
            Vec::new(),
            Vec::new(),
        ),
        "webSearch" => (
            item["query"].as_str().map_or_else(
                || "Web search".to_owned(),
                |query| format!("Search \"{query}\""),
            ),
            "fetch",
            Vec::new(),
            Vec::new(),
        ),
        "collabAgentToolCall" => (
            match item["tool"].as_str() {
                Some("wait") => "Wait for agents".to_owned(),
                Some("closeAgent") => "Close agent".to_owned(),
                Some("interruptAgent") => "Stop agent".to_owned(),
                Some("listAgents") => "List agents".to_owned(),
                Some("resumeAgent") => "Resume agent".to_owned(),
                tool => item["prompt"].as_str().map_or_else(
                    || {
                        if tool == Some("spawnAgent") {
                            "Start agent"
                        } else {
                            "Message agent"
                        }
                        .to_owned()
                    },
                    str::to_owned,
                ),
            },
            "think",
            Vec::new(),
            Vec::new(),
        ),
        "imageView" => (
            format!(
                "View {}",
                display_path(item["path"].as_str().unwrap_or("image"), cwd)
            ),
            "read",
            Vec::new(),
            Vec::new(),
        ),
        "imageGeneration" => ("Generate image".to_owned(), "other", Vec::new(), Vec::new()),
        "contextCompaction" => (
            "Compact context".to_owned(),
            "think",
            Vec::new(),
            Vec::new(),
        ),
        _ => return None,
    };
    let mut input = item.clone();
    if let Some(fields) = input.as_object_mut() {
        for key in [
            "aggregatedOutput",
            "result",
            "contentItems",
            "changes",
            "status",
        ] {
            fields.remove(key);
        }
    }
    Some(ToolInfo {
        title: clip_title(&title),
        kind,
        input,
        content,
        locations,
    })
}

fn split_diff(diff: &str) -> (String, String) {
    let mut old = String::new();
    let mut new = String::new();
    for line in diff.lines() {
        if line.starts_with("@@") || line.starts_with("---") || line.starts_with("+++") {
            continue;
        }
        if let Some(rest) = line.strip_prefix('-') {
            old.push_str(rest);
            old.push('\n');
        } else if let Some(rest) = line.strip_prefix('+') {
            new.push_str(rest);
            new.push('\n');
        } else {
            let rest = line.strip_prefix(' ').unwrap_or(line);
            old.push_str(rest);
            old.push('\n');
            new.push_str(rest);
            new.push('\n');
        }
    }
    (old, new)
}

fn clip_title(title: &str) -> String {
    let line = title.lines().next().unwrap_or_default();
    let multiline = title.lines().nth(1).is_some();
    let mut clipped = line.chars().take(MAX_TITLE_CHARS).collect::<String>();
    if multiline || line.chars().count() > MAX_TITLE_CHARS {
        clipped.push('…');
    }
    clipped
}

fn display_path(path: &str, cwd: &Path) -> String {
    Path::new(path)
        .strip_prefix(cwd)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map_or_else(
            || path.to_owned(),
            |relative| relative.display().to_string(),
        )
}

#[cfg(test)]
mod tests {
    use agent_client_protocol::schema::v1::SessionUpdate;

    use super::*;

    fn replay() -> Vec<Value> {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/agent/codex/fixtures/tools.ndjson");
        let mut translator = Translator::new(PathBuf::from("/work"));
        translator.expect_prompt("Reply with exactly: hi there");
        let updates = std::fs::read_to_string(&path)
            .expect("fixture")
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|record| record["dir"] == "out")
            .map(|record| record["msg"].clone())
            .filter(|message| message.get("id").is_none())
            .flat_map(|message| {
                translator.notification(
                    message["method"].as_str().unwrap_or_default(),
                    &message["params"],
                )
            })
            .collect::<Vec<_>>();
        for update in &updates {
            serde_json::from_value::<SessionUpdate>(update.clone())
                .unwrap_or_else(|error| panic!("{update} is not an ACP update: {error}"));
        }
        updates
    }

    #[test]
    fn a_recorded_codex_turn_becomes_rows_and_streamed_text() {
        let updates = replay();
        let text = updates
            .iter()
            .filter(|update| update["sessionUpdate"] == "agent_message_chunk")
            .filter_map(|update| update["content"]["text"].as_str())
            .collect::<String>();
        assert!(text.starts_with("hi there"), "{text}");
        assert!(text.ends_with("done"), "{text}");
        let calls = updates
            .iter()
            .filter(|update| update["sessionUpdate"] == "tool_call")
            .collect::<Vec<_>>();
        assert_eq!(calls[0]["title"], "echo cx > cx.txt && cat notes.txt");
        assert_eq!(calls[0]["kind"], "execute");
        assert_eq!(calls[1]["title"], "Edit notes.txt");
        assert_eq!(calls[1]["content"][0]["oldText"], "hullo\n");
        assert_eq!(calls[1]["content"][0]["newText"], "hallo\n");
        assert_eq!(calls[2]["title"], "Read notes.txt");
        assert_eq!(calls[2]["kind"], "read");
        let done = updates
            .iter()
            .find(|update| {
                update["sessionUpdate"] == "tool_call_update"
                    && update["toolCallId"] == calls[0]["toolCallId"]
                    && update.get("status").is_some()
            })
            .expect("command completion");
        assert_eq!(done["status"], "completed");
        assert_eq!(done["content"][0]["content"]["text"], "hullo");
        assert!(
            updates
                .iter()
                .any(|update| update["sessionUpdate"] == "usage_update")
        );
        assert!(
            !updates
                .iter()
                .any(|update| update["sessionUpdate"] == "user_message_chunk"
                    && update["content"]["text"] == "Reply with exactly: hi there"),
            "the prompt echo stays with the host"
        );
    }

    fn spawn(id: &str, thread: &str, status: &str, receivers: &[&str]) -> Value {
        json!({"threadId": thread, "item": {"type": "collabAgentToolCall", "id": id,
            "tool": "spawnAgent", "status": status, "senderThreadId": thread,
            "receiverThreadIds": receivers, "prompt": "look around", "agentsStates": {}}})
    }

    #[test]
    fn a_follow_up_turn_of_a_nested_agent_keeps_its_parent() {
        let mut translator = Translator::new(PathBuf::from("/work"));
        translator.notification("item/started", &spawn("spawn-a", "main", "inProgress", &[]));
        translator.notification(
            "item/completed",
            &spawn("spawn-a", "main", "completed", &["a"]),
        );
        let nested = translator
            .child(
                "a",
                "item/started",
                &spawn("spawn-b", "a", "inProgress", &[]),
            )
            .expect("adopted");
        assert_eq!(nested[0]["_meta"]["zz"]["parent"], "spawn-a");
        translator.child(
            "a",
            "item/completed",
            &spawn("spawn-b", "a", "completed", &["b"]),
        );
        let turn = |status: &str| json!({"threadId": "b", "turn": {"id": "t", "status": status}});
        translator.child("b", "turn/completed", &turn("completed"));
        let again = translator
            .child("b", "turn/started", &turn("inProgress"))
            .expect("adopted");
        assert_eq!(again[0]["toolCallId"], "spawn-b~2");
        assert_eq!(again[0]["_meta"]["zz"]["parent"], "spawn-a");
    }

    #[test]
    fn child_output_before_the_spawn_names_its_thread_is_replayed() {
        let mut translator = Translator::new(PathBuf::from("/work"));
        translator.notification("item/started", &spawn("spawn-s", "main", "inProgress", &[]));
        let command = json!({"threadId": "c", "item": {"type": "commandExecution", "id": "x1",
            "command": "ls", "cwd": "/work", "status": "inProgress", "commandActions": []}});
        assert_eq!(translator.child("c", "item/started", &command), None);
        let done = json!({"threadId": "c", "turn": {"id": "t", "status": "completed"}});
        assert_eq!(translator.child("c", "turn/completed", &done), None);
        let updates = translator.notification(
            "item/completed",
            &spawn("spawn-s", "main", "completed", &["c"]),
        );
        assert!(
            updates
                .iter()
                .any(|update| update["sessionUpdate"] == "tool_call"
                    && update["toolCallId"] == "x1"
                    && update["_meta"]["zz"]["parent"] == "spawn-s")
        );
        assert!(
            updates
                .iter()
                .any(|update| update["toolCallId"] == "spawn-s" && update["status"] == "completed")
        );
    }

    #[test]
    fn a_grandchild_replayed_on_adoption_keeps_its_own_parent() {
        let mut translator = Translator::new(PathBuf::from("/work"));
        translator.notification("item/started", &spawn("spawn-a", "main", "inProgress", &[]));
        translator.notification(
            "item/completed",
            &spawn("spawn-a", "main", "completed", &["a"]),
        );
        translator.child(
            "a",
            "item/started",
            &spawn("spawn-b", "a", "inProgress", &[]),
        );
        let tool = json!({"threadId": "b", "item": {"type": "commandExecution", "id": "x",
            "command": "ls", "cwd": "/work", "status": "inProgress", "commandActions": []}});
        assert_eq!(translator.child("b", "item/started", &tool), None);
        let updates = translator
            .child(
                "a",
                "item/completed",
                &spawn("spawn-b", "a", "completed", &["b"]),
            )
            .expect("adopted");
        let call = updates
            .iter()
            .find(|update| update["toolCallId"] == "x")
            .expect("replayed");
        assert_eq!(call["_meta"]["zz"]["parent"], "spawn-b");
    }

    #[test]
    fn a_full_child_buffer_still_keeps_the_child_turn_end() {
        let mut translator = Translator::new(PathBuf::from("/work"));
        translator.notification("item/started", &spawn("spawn-s", "main", "inProgress", &[]));
        let item = |index: usize| {
            json!({"threadId": "c", "item": {"type": "reasoning",
            "id": format!("r{index}"), "summary": [], "content": []}})
        };
        for index in 0..PENDING_FRAMES + 10 {
            translator.child("c", "item/started", &item(index));
        }
        let done = json!({"threadId": "c", "turn": {"id": "t", "status": "completed"}});
        translator.child("c", "turn/completed", &done);
        let updates = translator.notification(
            "item/completed",
            &spawn("spawn-s", "main", "completed", &["c"]),
        );
        assert!(
            updates
                .iter()
                .any(|update| update["toolCallId"] == "spawn-s" && update["status"] == "completed")
        );
    }

    #[test]
    fn a_restored_spawn_that_finished_stays_finished() {
        let mut translator = Translator::new(PathBuf::from("/work"));
        let item = spawn("spawn-r", "main", "completed", &["r"]);
        let updates = translator.history_item(&item["item"]);
        assert!(
            updates
                .iter()
                .any(|update| update["toolCallId"] == "spawn-r" && update["status"] == "completed")
        );
    }

    #[test]
    fn a_spawned_agent_row_runs_until_the_child_turn_ends() {
        let mut translator = Translator::new(PathBuf::from("/work"));
        let spawn = |status: &str, receivers: &[&str]| {
            json!({"threadId": "main", "item": {"type": "collabAgentToolCall", "id": "spawn1",
                "tool": "spawnAgent", "status": status, "senderThreadId": "main",
                "receiverThreadIds": receivers, "prompt": "look around", "agentsStates": {}}})
        };
        let started = translator.notification("item/started", &spawn("inProgress", &[]));
        assert_eq!(started[0]["sessionUpdate"], "tool_call");
        let completed = translator.notification("item/completed", &spawn("completed", &["child"]));
        assert_eq!(completed[0]["status"], "in_progress");
        let turn =
            |status: &str| json!({"threadId": "child", "turn": {"id": "t", "status": status}});
        assert_eq!(
            translator.child("child", "turn/started", &turn("inProgress")),
            Some(Vec::new())
        );
        let done = translator
            .child("child", "turn/completed", &turn("completed"))
            .expect("adopted");
        assert_eq!(done[0]["toolCallId"], "spawn1");
        assert_eq!(done[0]["status"], "completed");
        let again = translator
            .child("child", "turn/started", &turn("inProgress"))
            .expect("adopted");
        assert_eq!(again[0]["sessionUpdate"], "tool_call");
        assert_eq!(again[0]["toolCallId"], "spawn1~2");
        let failed = translator
            .child("child", "turn/completed", &turn("interrupted"))
            .expect("adopted");
        assert_eq!(failed[0]["toolCallId"], "spawn1~2");
        assert_eq!(failed[0]["status"], "failed");
    }

    #[test]
    fn a_recorded_subagent_nests_its_steps_under_its_row() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/agent/codex/fixtures/subagent.ndjson");
        let messages = std::fs::read_to_string(&path)
            .expect("fixture")
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .map(|record| record["msg"].clone())
            .collect::<Vec<_>>();
        let main = messages
            .iter()
            .find(|message| message["method"] == "turn/started")
            .and_then(|message| message["params"]["threadId"].as_str())
            .expect("main thread")
            .to_owned();
        let mut translator = Translator::new(PathBuf::from("/work"));
        let updates = messages
            .iter()
            .flat_map(|message| {
                let method = message["method"].as_str().unwrap_or_default();
                let params = &message["params"];
                match params["threadId"].as_str() {
                    Some(thread) if thread != main => {
                        translator.child(thread, method, params).unwrap_or_default()
                    }
                    _ => translator.notification(method, params),
                }
            })
            .collect::<Vec<_>>();
        for update in &updates {
            serde_json::from_value::<SessionUpdate>(update.clone())
                .unwrap_or_else(|error| panic!("{update} is not an ACP update: {error}"));
        }
        let calls = updates
            .iter()
            .filter(|update| update["sessionUpdate"] == "tool_call")
            .collect::<Vec<_>>();
        let agent = calls
            .iter()
            .find(|call| call["title"] == "List directory")
            .expect("agent row");
        assert_eq!(agent["kind"], "think");
        let steps = calls
            .iter()
            .filter(|call| call["_meta"]["zz"]["parent"] == agent["toolCallId"])
            .collect::<Vec<_>>();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0]["title"], "ls");
        assert!(calls.iter().any(|call| call["title"] == "Wait for agents"));
        let row = updates
            .iter()
            .filter(|update| {
                update["sessionUpdate"] == "tool_call_update"
                    && update["toolCallId"] == agent["toolCallId"]
            })
            .collect::<Vec<_>>();
        assert!(
            row.iter()
                .any(|update| update["content"][0]["content"]["text"] == "one.txt, two.txt")
        );
        assert_eq!(row.last().expect("completion")["status"], "completed");
        assert!(!updates.iter().any(|update| {
            update["sessionUpdate"] == "agent_message_chunk"
                && update["content"]["text"]
                    .as_str()
                    .is_some_and(|text| text.contains("one.txt"))
        }));
        assert!(
            translator
                .child("unknown", "item/started", &json!({}))
                .is_none()
        );
    }

    #[test]
    fn a_failed_command_and_an_added_file_render_honestly() {
        let mut translator = Translator::new(PathBuf::from("/work"));
        let failed = json!({"type":"commandExecution","id":"c1","command":"false","commandActions":[],
            "status":"completed","exitCode":1,"aggregatedOutput":"boom"});
        translator.notification("item/started", &json!({ "item": failed }));
        let updates = translator.notification("item/completed", &json!({ "item": failed }));
        assert_eq!(updates[0]["status"], "failed");
        let added = json!({"type":"fileChange","id":"f1","status":"completed","changes":[
            {"path":"/work/new.rs","kind":{"type":"add"},"diff":"+fn main() {}\n"}]});
        let updates = translator.notification("item/started", &json!({ "item": added }));
        assert_eq!(updates[0]["content"][0]["oldText"], Value::Null);
        assert_eq!(updates[0]["content"][0]["newText"], "fn main() {}\n");
    }
}
