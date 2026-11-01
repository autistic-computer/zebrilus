use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use zz_protocol::{
    AgentQuestionAnswer,
    agent_stream::{AgentQuestion, AgentQuestionOption},
};

use super::{
    AgentPrompt, AgentPromptOutcome, AgentStreamPayload, Codex, Found, Lens, Read, Source, Tail,
    Turn, arguments, codex_home,
};
use crate::agent::{
    child::option,
    codex::{
        self as native, Reply,
        control::{self, Connection, Next},
        rollout::{self, RolloutMeta},
    },
    lens_settings,
    runtime::validate_payload,
};

const JOIN: Duration = Duration::from_secs(3);
const EMBEDDED: [&str; 11] = [
    "-c",
    "--config",
    "--enable",
    "--disable",
    "--search",
    "--profile",
    "-p",
    "--oss",
    "--no-daemon",
    "--strict-config",
    "--remote",
];
const QUIET: [&str; 5] = [
    "turn/moderationMetadata",
    "account/rateLimits/updated",
    "account/updated",
    "thread/readState/changed",
    "remoteControl/status/changed",
];
const ASYNC_QUESTION: &str = "request_user_input_async";
const REPLY_OPEN: &str = "<send_user_message_question_reply>";
const REPLY_CLOSE: &str = "</send_user_message_question_reply>";

#[derive(Default)]
pub(super) struct Link {
    live: Option<Box<Live>>,
    relive: bool,
    cwd: PathBuf,
    pub(super) ids: Arc<AtomicU64>,
    #[cfg(test)]
    pub(super) wire: Option<Connection>,
}

pub(super) struct Live {
    connection: Connection,
    thread: String,
    path: PathBuf,
    cwd: PathBuf,
    joined: Value,
    turn: Option<String>,
    history: HashSet<String>,
    seen: HashSet<String>,
    calls: HashMap<u64, Call>,
    requests: HashMap<u64, Pending>,
    asks: HashMap<String, Vec<AgentQuestion>>,
    card: Option<(u64, String)>,
    next: Option<(String, Option<PathBuf>)>,
}

enum Call {
    Start,
    Steer(AgentPrompt),
    Settings(String),
    Quiet,
}

enum Pending {
    Server {
        rpc_id: Value,
        reply: Reply,
        decisions: Vec<Value>,
    },
    Question {
        item: String,
        questions: Vec<AgentQuestion>,
    },
}

pub(super) fn embedded(arguments: &[String]) -> bool {
    arguments.iter().skip(1).any(|argument| {
        EMBEDDED.iter().any(|flag| {
            argument == flag
                || argument
                    .strip_prefix(flag)
                    .is_some_and(|rest| rest.starts_with('=') || !flag.starts_with("--"))
        })
    })
}

impl Live {
    fn join(
        mut connection: Connection,
        thread: String,
        path: &Path,
        cwd: &Path,
    ) -> Result<Self, String> {
        connection.call(
            "initialize",
            &json!({
                "clientInfo": { "name": "zz", "title": "zz", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": {
                    "experimentalApi": true,
                    "requestAttestation": false,
                    "optOutNotificationMethods": QUIET,
                },
            }),
            JOIN,
        )?;
        connection.notify("initialized");
        let mut cursor = Value::Null;
        loop {
            let page = connection.call("thread/loaded/list", &json!({ "cursor": cursor }), JOIN)?;
            if page["data"]
                .as_array()
                .is_some_and(|threads| threads.iter().any(|loaded| *loaded == thread.as_str()))
            {
                break;
            }
            cursor = page["nextCursor"].clone();
            if cursor.is_null() {
                return Err(format!("thread {thread} is not loaded in the app server"));
            }
        }
        let joined = connection.call(
            "thread/resume",
            &json!({ "threadId": thread, "excludeTurns": true }),
            JOIN,
        )?;
        Ok(Self {
            connection,
            thread,
            path: path.to_path_buf(),
            cwd: cwd.to_path_buf(),
            joined,
            turn: None,
            history: HashSet::new(),
            seen: HashSet::new(),
            calls: HashMap::new(),
            requests: HashMap::new(),
            asks: HashMap::new(),
            card: None,
            next: None,
        })
    }

    fn call(&mut self, call: Call, method: &str, params: &Value) {
        let id = self.connection.request(method, params);
        self.calls.insert(id, call);
    }

    fn started_thread(&mut self, thread: &Value) {
        let Some(id) = thread["id"].as_str().filter(|id| *id != self.thread) else {
            return;
        };
        if thread["ephemeral"] == true
            || !thread["parentThreadId"].is_null()
            || thread["threadSource"]
                .as_str()
                .is_some_and(|source| source != "user")
            || !thread["cwd"]
                .as_str()
                .is_some_and(|cwd| same_dir(Path::new(cwd), &self.cwd))
        {
            return;
        }
        self.next = Some((id.to_owned(), thread["path"].as_str().map(PathBuf::from)));
    }

    fn resolved(&mut self, request: &Value, read: &mut Read) {
        let resolved = self
            .requests
            .iter()
            .filter(|(_, pending)| {
                matches!(pending, Pending::Server { rpc_id, .. } if rpc_id == request)
            })
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for request_id in resolved {
            self.requests.remove(&request_id);
            read.payloads.push(AgentStreamPayload::PermissionResolved {
                request_id,
                canceled: false,
            });
        }
    }

    fn close_requests(&mut self, read: &mut Read) {
        let closed = self
            .requests
            .iter()
            .filter(|(_, pending)| matches!(pending, Pending::Server { .. }))
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for request_id in closed {
            self.requests.remove(&request_id);
            read.payloads.push(AgentStreamPayload::PermissionResolved {
                request_id,
                canceled: true,
            });
        }
    }

    fn switching(&self) -> bool {
        self.next
            .as_ref()
            .is_some_and(|(_, path)| path.as_deref().is_some_and(Path::exists))
    }

    fn reply_async(
        &mut self,
        item: &str,
        questions: &[AgentQuestion],
        answers: &[AgentQuestionAnswer],
    ) {
        let entries = questions
            .iter()
            .enumerate()
            .filter_map(|(index, question)| {
                let answer = answers.iter().find(|answer| answer.id == question.id)?;
                Some(json!({
                    "questionItemId": json!([ASYNC_QUESTION, item, index]).to_string(),
                    "question": question.question,
                    "answer": answer.answers.join(", "),
                }))
            })
            .collect::<Vec<_>>();
        if entries.is_empty() {
            return;
        }
        let text = format!("{REPLY_OPEN}\n{}\n{REPLY_CLOSE}\n", Value::Array(entries));
        let input = json!([{ "type": "text", "text": text, "text_elements": [] }]);
        self.send_input(Call::Quiet, &input);
    }

    fn send_input(&mut self, call: Call, input: &Value) {
        let thread = self.thread.clone();
        match self.turn.clone() {
            Some(turn) => self.call(
                call,
                "turn/steer",
                &json!({ "threadId": thread, "input": input, "expectedTurnId": turn }),
            ),
            None => self.call(
                call,
                "turn/start",
                &json!({ "threadId": thread, "input": input }),
            ),
        }
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.connection
            .request("thread/unsubscribe", &json!({ "threadId": self.thread }));
    }
}

fn same_dir(left: &Path, right: &Path) -> bool {
    left == right
        || std::fs::canonicalize(left)
            .ok()
            .zip(std::fs::canonicalize(right).ok())
            .is_some_and(|(left, right)| left == right)
}

fn questions_of(raw: &Value) -> Vec<AgentQuestion> {
    raw.as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, question)| AgentQuestion {
            id: question["id"]
                .as_str()
                .map_or_else(|| format!("q{index}"), str::to_owned),
            question: question["question"].as_str().unwrap_or_default().to_owned(),
            options: question["options"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|choice| {
                    Some(AgentQuestionOption {
                        label: choice["label"].as_str()?.to_owned(),
                        description: None,
                    })
                })
                .collect(),
            ..AgentQuestion::default()
        })
        .collect()
}

fn note_asks(live: &mut Live, updates: &[Value]) {
    for update in updates {
        if update["sessionUpdate"] == "tool_call"
            && update["_meta"]["codex"]["itemType"] == "userInputRequest"
            && let Some(id) = update["toolCallId"].as_str()
        {
            live.asks.insert(
                id.to_owned(),
                questions_of(&update["rawInput"]["questions"]),
            );
        }
    }
}

fn decision_options(decisions: &[Value]) -> Value {
    let declines = decisions.iter().any(|decision| decision == "decline");
    Value::Array(
        decisions
            .iter()
            .enumerate()
            .filter_map(|(index, decision)| {
                let (name, kind) = decision_name(decision, declines)?;
                Some(option(&format!("decision-{index}"), &name, kind))
            })
            .collect(),
    )
}

fn decision_name(decision: &Value, declines: bool) -> Option<(String, &'static str)> {
    let named = |name: &str, kind| Some((name.to_owned(), kind));
    match decision.as_str() {
        Some("accept") => named("Allow", "allow_once"),
        Some("acceptForSession") => named("Always allow this session", "allow_always"),
        Some("decline") => named("Reject", "reject_once"),
        Some("cancel") => named(
            if declines {
                "Reject and stop"
            } else {
                "Reject"
            },
            "reject_once",
        ),
        Some(_) => None,
        None => {
            if let Some(parts) =
                decision["acceptWithExecpolicyAmendment"]["execpolicy_amendment"].as_array()
            {
                let command = parts
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ");
                return Some((format!("Always allow `{command}`"), "allow_always"));
            }
            let rule = &decision["applyNetworkPolicyAmendment"]["network_policy_amendment"];
            let host = rule["host"].as_str()?;
            let verb = if rule["action"] == "deny" {
                "Always block"
            } else {
                "Always allow"
            };
            Some((format!("{verb} {host}"), "allow_always"))
        }
    }
}

fn pick(decisions: &[Value], option: Option<&str>) -> Value {
    let named = |name: &str| decisions.iter().find(|decision| *decision == name).cloned();
    if let Some(decision) = option
        .and_then(|option| option.strip_prefix("decision-"))
        .and_then(|index| index.parse::<usize>().ok())
        .and_then(|index| decisions.get(index))
    {
        return decision.clone();
    }
    let chosen = match option {
        Some("allow") => named("accept"),
        Some("allow_always") => named("acceptForSession").or_else(|| {
            decisions
                .iter()
                .find(|decision| decision.is_object())
                .cloned()
        }),
        _ => None,
    };
    chosen
        .or_else(|| named("decline"))
        .or_else(|| named("cancel"))
        .unwrap_or_else(|| Value::from("decline"))
}

impl Codex {
    pub(super) fn leave(&mut self) {
        if self.link.live.take().is_none() {
            return;
        }
        self.busy = false;
        if let Some(tail) = self.tail.as_mut() {
            tail.offset = std::fs::metadata(&tail.path).map_or(tail.offset, |meta| meta.len());
            tail.partial.clear();
        }
    }

    pub(super) fn relaunched(&mut self, pid: u32) {
        self.leave();
        self.link.relive = self.tail.is_some() && !embedded(&arguments(pid));
    }

    pub(super) fn vanished(&mut self) {
        if self.link.live.is_some() {
            self.leave();
            self.link.relive = self.tail.is_some();
        }
    }

    pub(super) fn is_live(&self) -> bool {
        self.link.live.is_some()
    }

    pub(super) fn located(&self) -> Option<Found> {
        let live = self.link.live.as_ref()?;
        let (session, path) = match &live.next {
            Some((thread, Some(path))) if path.exists() => (thread, path),
            Some(_) => return None,
            None => (&live.thread, &live.path),
        };
        Some(Found {
            session: session.clone(),
            path: path.clone(),
            cwd: live.cwd.clone(),
        })
    }

    pub(super) fn join(&mut self, path: &Path, cwd: &Path) {
        self.link.live = None;
        self.link.relive = false;
        self.link.cwd = cwd.to_path_buf();
        let Some(connection) = self.dial() else {
            return;
        };
        let Some(thread) = RolloutMeta::read(path).map(|meta| meta.thread) else {
            return;
        };
        match Live::join(connection, thread, path, cwd) {
            Ok(live) => self.link.live = Some(Box::new(live)),
            Err(error) => {
                log::info!(target: "zz::agent", "the Codex lens reads the rollout instead: {error}");
            }
        }
    }

    fn dial(&mut self) -> Option<Connection> {
        #[cfg(test)]
        if let Some(wire) = self.link.wire.take() {
            return Some(wire);
        }
        let pid = self.pid?;
        if embedded(&arguments(pid)) {
            return None;
        }
        let socket = control::socket(&codex_home()?)?;
        Connection::open(&socket, JOIN)
            .map_err(|error| {
                log::info!(target: "zz::agent", "no Codex app server to join: {error}");
            })
            .ok()
    }

    pub(super) fn heard(&mut self, id: &str) {
        if let Some(live) = self.link.live.as_mut() {
            live.history.insert(id.to_owned());
        }
    }

    pub(super) fn joined(&mut self, updates: &[Value]) {
        let Some(mut live) = self.link.live.take() else {
            return;
        };
        note_asks(&mut live, updates);
        let joined = std::mem::take(&mut live.joined);
        self.busy = joined["thread"]["status"]["type"] == "active";
        self.settings(&json!({
            "model": joined["model"],
            "effort": joined["reasoningEffort"],
            "cwd": joined["cwd"],
        }));
        self.link.live = Some(live);
    }

    fn settings(&mut self, settings: &Value) {
        let text = |key: &str| {
            settings[key]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        if let Some(model) = text("model") {
            self.facts.model = Some(model);
        }
        if let Some(effort) = text("effort") {
            self.facts.effort = Some(effort);
        }
        if let Some(cwd) = text("cwd") {
            self.facts.cwd = Some(cwd);
        }
    }

    fn rejoin(&mut self) -> Read {
        let mut caught = Read::default();
        let Some(path) = self.tail.as_ref().map(|tail| tail.path.clone()) else {
            return caught;
        };
        let cwd = self.link.cwd.clone();
        self.join(&path, &cwd);
        if self.link.live.is_some() {
            let lines = self.tail.as_mut().map(Tail::backlog).unwrap_or_default();
            for entry in lines {
                self.entry(&entry, &mut caught);
            }
            self.joined(&caught.updates);
        }
        caught
    }

    pub(super) fn read_live(&mut self) -> Option<Read> {
        let caught = if self.link.relive && self.pid.is_some() {
            self.link.relive = false;
            self.rejoin()
        } else {
            Read::default()
        };
        let mut live = self.link.live.take()?;
        let mut read = Read {
            updates: std::mem::take(&mut self.dropped),
            ..Read::default()
        };
        read.updates.extend(caught.updates);
        read.finished = caught.finished;
        read.failed = caught.failed;
        self.look_for_children();
        self.follow_question(&mut live, &mut read);
        let busy = self.busy;
        let mut gone = false;
        loop {
            if !read.payloads.is_empty() || read.finished || self.busy != busy {
                live.connection.poke();
                break;
            }
            match live.connection.next() {
                Next::Message(message) => {
                    self.message(&mut live, &message, &mut read);
                    self.follow_question(&mut live, &mut read);
                }
                Next::Empty => break,
                Next::Gone => {
                    gone = true;
                    break;
                }
            }
        }
        read.reload |= live.switching();
        self.link.live = Some(live);
        if gone {
            self.leave();
            read.reload = true;
        }
        read.updates.extend(self.child_lines(None, false));
        Some(read)
    }

    fn message(&mut self, live: &mut Live, message: &Value, read: &mut Read) {
        let params = &message["params"];
        match (message["method"].as_str(), message.get("id")) {
            (Some(method), Some(id)) => self.server_request(live, method, id, params, read),
            (Some(method), None) => self.notification(live, method, params, read),
            (None, Some(id)) => self.response(live, id, message, read),
            (None, None) => {}
        }
    }

    fn notification(&mut self, live: &mut Live, method: &str, params: &Value, read: &mut Read) {
        match method {
            "thread/started" => return live.started_thread(&params["thread"]),
            "serverRequest/resolved" => return live.resolved(&params["requestId"], read),
            _ => {}
        }
        if params["threadId"].as_str() != Some(live.thread.as_str()) {
            return;
        }
        if self.busy
            && let Some(turn) = params["turnId"].as_str()
        {
            live.turn = Some(turn.to_owned());
        }
        match method {
            "turn/started" => {
                self.busy = true;
                live.turn = params["turn"]["id"].as_str().map(str::to_owned);
            }
            "turn/completed" => {
                read.updates.extend(self.child_lines(None, true));
                live.close_requests(read);
                self.busy = false;
                live.turn = None;
                read.finished = true;
                read.failed = (params["turn"]["status"] == "failed").then(|| {
                    params["turn"]["error"]["message"]
                        .as_str()
                        .unwrap_or("Codex turn failed")
                        .to_owned()
                });
            }
            "thread/status/changed" => {
                if let Some(status) = params["status"]["type"].as_str() {
                    self.busy = status == "active";
                }
            }
            "thread/settings/updated" => self.settings(&params["threadSettings"]),
            "item/started" | "item/completed" => self.item(live, method, &params["item"], read),
            _ => {
                if params["itemId"]
                    .as_str()
                    .is_some_and(|id| live.history.contains(id))
                {
                    return;
                }
                read.updates
                    .extend(self.translator.notification(method, params));
            }
        }
    }

    fn item(&mut self, live: &mut Live, method: &str, item: &Value, read: &mut Read) {
        let Some(item) = rollout::live_item(item) else {
            return;
        };
        let Some(id) = item["id"].as_str().map(str::to_owned) else {
            return;
        };
        if live.history.contains(&id) {
            return;
        }
        if item["type"] == "subAgentActivity" {
            let thread = item["agentThreadId"].as_str().unwrap_or_default();
            if item["kind"] == "started" {
                self.adopt(thread, None, item["agentPath"].as_str());
            } else if item["kind"] == "completed" && method == "item/completed" {
                read.updates.extend(self.child_lines(None, true));
            }
        }
        let wrapped = json!({ "item": item });
        let mut updates = Vec::new();
        if method == "item/started" {
            live.seen.insert(id);
        } else if !live.seen.remove(&id) {
            updates.extend(self.translator.notification("item/started", &wrapped));
        }
        updates.extend(self.translator.notification(method, &wrapped));
        note_asks(live, &updates);
        read.updates.extend(updates);
    }

    fn response(&mut self, live: &mut Live, id: &Value, message: &Value, read: &mut Read) {
        let Some(call) = id.as_u64().and_then(|id| live.calls.remove(&id)) else {
            return;
        };
        let Some(error) = message.get("error").map(control::error_text) else {
            return;
        };
        match call {
            Call::Start => {
                read.finished = true;
                read.failed = Some(error);
            }
            Call::Steer(prompt) => read.payloads.push(AgentStreamPayload::PromptsReclaimed {
                prompts: vec![prompt],
            }),
            Call::Settings(option_id) => {
                self.picks.forget();
                read.payloads.push(AgentStreamPayload::SettingFailed {
                    option_id,
                    message: error,
                });
            }
            Call::Quiet => {
                log::debug!(target: "zz::agent", "the Codex app server refused a lens request: {error}");
            }
        }
    }

    fn server_request(
        &mut self,
        live: &mut Live,
        method: &str,
        rpc_id: &Value,
        params: &Value,
        read: &mut Read,
    ) {
        if params["threadId"].as_str() != Some(live.thread.as_str()) {
            return;
        }
        if let Some(turn) = params["turnId"].as_str() {
            live.turn = Some(turn.to_owned());
        }
        let (pending, tool_call, options, questions) =
            if let Some((reply, tool_call)) = native::approval(method, params) {
                let decisions = params["availableDecisions"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let options = if decisions.is_empty() {
                    native::approval_options(&reply)
                } else {
                    decision_options(&decisions)
                };
                let pending = Pending::Server {
                    rpc_id: rpc_id.clone(),
                    reply,
                    decisions,
                };
                (pending, tool_call, options, Vec::new())
            } else if method == "item/tool/requestUserInput" {
                let (questions, options, title) = native::user_input(params);
                let tool_call =
                    json!({ "toolCallId": params["itemId"], "title": title, "kind": "other" });
                let pending = Pending::Server {
                    rpc_id: rpc_id.clone(),
                    reply: Reply::Questions(questions.clone()),
                    decisions: Vec::new(),
                };
                (pending, tool_call, options, questions)
            } else {
                return;
            };
        let request_id = self.link.ids.fetch_add(1, Ordering::Relaxed);
        let payload = AgentStreamPayload::PermissionRequested {
            request_id,
            tool_call,
            options,
            questions,
        };
        if validate_payload(&payload).is_err() {
            return;
        }
        live.requests.insert(request_id, pending);
        read.payloads.push(payload);
    }

    fn follow_question(&mut self, live: &mut Live, read: &mut Read) {
        let asking = self.translator.asking().map(|(id, _)| id.to_owned());
        if let Some((request_id, item)) = live.card.clone()
            && asking.as_deref() != Some(item.as_str())
        {
            live.card = None;
            live.asks.remove(&item);
            if live.requests.remove(&request_id).is_some() {
                read.payloads.push(AgentStreamPayload::PermissionResolved {
                    request_id,
                    canceled: false,
                });
            }
        }
        if live.card.is_some() {
            return;
        }
        let Some(item) = asking else {
            return;
        };
        let Some(questions) = live.asks.get(&item).cloned() else {
            return;
        };
        let (options, title) = native::question_options(&questions);
        let request_id = self.link.ids.fetch_add(1, Ordering::Relaxed);
        read.payloads.push(AgentStreamPayload::PermissionRequested {
            request_id,
            tool_call: json!({ "toolCallId": item, "title": title, "kind": "other" }),
            options,
            questions: questions.clone(),
        });
        live.requests.insert(
            request_id,
            Pending::Question {
                item: item.clone(),
                questions,
            },
        );
        live.card = Some((request_id, item));
    }
}

impl Lens {
    pub(super) fn wake(&self) -> Option<async_channel::Receiver<()>> {
        match &self.source {
            Source::Codex(codex) => codex.link.live.as_ref().map(|live| live.connection.wake()),
            Source::Claude(_) => None,
        }
    }

    fn live(&mut self) -> Option<&mut Live> {
        match &mut self.source {
            Source::Codex(codex) => codex.link.live.as_deref_mut(),
            Source::Claude(_) => None,
        }
    }

    pub(super) fn live_prompt(
        &mut self,
        turn_id: u64,
        prompt: &AgentPrompt,
    ) -> Option<Vec<AgentStreamPayload>> {
        let text = prompt.text.trim();
        if text == "/plan" {
            return self.live_plan(turn_id);
        }
        if text.starts_with('/')
            || text.starts_with('!')
            || (text.is_empty() && prompt.images.is_empty())
        {
            return None;
        }
        let live = self.live()?;
        let thread = live.thread.clone();
        live.call(
            Call::Start,
            "turn/start",
            &json!({ "threadId": thread, "input": native::turn_input(prompt) }),
        );
        self.turn = Some(Turn {
            id: turn_id,
            sent: Instant::now(),
            busy: false,
            idle_since: None,
            cancelled: false,
        });
        Some(Vec::new())
    }

    fn live_plan(&mut self, turn_id: u64) -> Option<Vec<AgentStreamPayload>> {
        let facts = self.source.facts();
        let model = facts.model?;
        let live = self.live()?;
        let thread = live.thread.clone();
        live.call(
            Call::Settings("mode".to_owned()),
            "thread/settings/update",
            &json!({
                "threadId": thread,
                "collaborationMode": {
                    "mode": "plan",
                    "settings": {
                        "model": model,
                        "reasoning_effort": facts.effort,
                        "developer_instructions": null,
                    },
                },
            }),
        );
        Some(vec![AgentStreamPayload::PromptFinished {
            turn_id,
            outcome: AgentPromptOutcome::Finished {
                stop_reason: Value::from("end_turn"),
            },
        }])
    }

    pub(super) fn live_steer(
        &mut self,
        text: &str,
        prompt: &AgentPrompt,
    ) -> Option<Vec<AgentStreamPayload>> {
        let live = self.live()?;
        let steered = AgentPrompt {
            text: text.to_owned(),
            ..prompt.clone()
        };
        let input = native::turn_input(&steered);
        live.send_input(Call::Steer(steered), &input);
        Some(Vec::new())
    }

    pub(super) fn live_cancel(&mut self) -> Option<Vec<AgentStreamPayload>> {
        let live = self.live()?;
        let turn = live.turn.clone()?;
        let thread = live.thread.clone();
        live.call(
            Call::Quiet,
            "turn/interrupt",
            &json!({ "threadId": thread, "turnId": turn }),
        );
        Some(Vec::new())
    }

    pub(super) fn live_respond(
        &mut self,
        request_id: u64,
        option_id: Option<&str>,
    ) -> Option<Vec<AgentStreamPayload>> {
        let live = self.live()?;
        match live.requests.remove(&request_id)? {
            Pending::Server {
                rpc_id,
                reply: Reply::Questions(questions),
                ..
            } => {
                let chosen = native::chosen(&questions, option_id);
                live.connection
                    .reply(&rpc_id, &native::question_answers(chosen.as_slice()));
            }
            Pending::Server {
                rpc_id,
                reply,
                decisions,
            } => {
                let result = if decisions.is_empty() {
                    native::decision(&reply, option_id.unwrap_or("cancel"))
                } else {
                    json!({ "decision": pick(&decisions, option_id) })
                };
                live.connection.reply(&rpc_id, &result);
            }
            Pending::Question { item, questions } => {
                if let Some(answer) = native::chosen(&questions, option_id) {
                    live.reply_async(&item, &questions, &[answer]);
                }
            }
        }
        Some(vec![AgentStreamPayload::PermissionResolved {
            request_id,
            canceled: option_id.is_none(),
        }])
    }

    pub(super) fn live_answer(
        &mut self,
        request_id: u64,
        answers: &[AgentQuestionAnswer],
    ) -> Option<Vec<AgentStreamPayload>> {
        let live = self.live()?;
        match live.requests.remove(&request_id)? {
            Pending::Server {
                rpc_id,
                reply: Reply::Questions(_),
                ..
            } => live
                .connection
                .reply(&rpc_id, &native::question_answers(answers)),
            pending @ Pending::Server { .. } => {
                live.requests.insert(request_id, pending);
                return Some(Vec::new());
            }
            Pending::Question { item, questions } => live.reply_async(&item, &questions, answers),
        }
        Some(vec![AgentStreamPayload::PermissionResolved {
            request_id,
            canceled: false,
        }])
    }

    pub(super) fn live_set(
        &mut self,
        option_id: &str,
        value: &str,
    ) -> Option<Vec<AgentStreamPayload>> {
        if option_id != lens_settings::MODEL && option_id != lens_settings::EFFORT {
            return None;
        }
        self.live()?;
        let facts = self.source.facts();
        let setter = match lens_settings::command(
            self.spec.provider,
            option_id,
            value,
            &facts,
            self.source.catalog(),
        ) {
            Ok(setter) => setter,
            Err(message) => {
                return Some(vec![AgentStreamPayload::SettingFailed {
                    option_id: option_id.to_owned(),
                    message,
                }]);
            }
        };
        let mut chosen = facts;
        for (option, value) in &setter.applied {
            chosen.apply(option, value);
        }
        let live = self.live()?;
        let mut params = json!({ "threadId": live.thread });
        if let Some(model) = chosen.model {
            params["model"] = Value::from(model);
        }
        if let Some(effort) = chosen.effort {
            params["effort"] = Value::from(effort);
        }
        live.call(
            Call::Settings(option_id.to_owned()),
            "thread/settings/update",
            &params,
        );
        for (option, value) in &setter.applied {
            self.source.facts_mut().apply(option, value);
        }
        Some(vec![self.settings_changed(option_id, value.trim())])
    }
}

#[cfg(test)]
#[path = "lens_codex_live_tests.rs"]
mod tests;
