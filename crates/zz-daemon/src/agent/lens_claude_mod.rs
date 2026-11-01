use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use zz_protocol::{AgentQuestionAnswer, agent_stream::AgentQuestion};

use crate::agent::{
    claude::{
        allow, allow_with_mode, answered, approval_request, chosen, deny, plan_request,
        question_list, question_request, translate::chunk,
    },
    lens::LensPane,
    lens_channel::Channel,
    runtime::validate_payload,
    stream::AgentStreamPayload,
};

const UNRELAYED: Duration = Duration::from_secs(2);
const SETTLED: Duration = Duration::from_secs(1);
const MAX_BLOCKS: usize = 64;
const MAX_STRANGERS: usize = 8;
const EXITED: &str = "Claude Code exited before it took the prompt";

pub(crate) struct Link {
    channel: Channel,
    io: Arc<dyn LensPane>,
    permission_ids: Arc<AtomicU64>,
    instance: Option<Instance>,
    held: Vec<Value>,
    strangers: Vec<(String, String)>,
    blocks: Vec<Block>,
    asks: Vec<Ask>,
    payloads: Vec<AgentStreamPayload>,
    commands: u64,
    prompts: Vec<Prompt>,
    unrelayed: Option<Instant>,
    done: Option<Instant>,
    started: bool,
}

struct Prompt {
    id: u64,
    instance: String,
    text: String,
}

struct Instance {
    id: String,
    turn: Option<String>,
    model: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Text,
    Thinking,
}

impl Kind {
    fn named(name: &str) -> Option<Self> {
        match name {
            "text" => Some(Self::Text),
            "thinking" => Some(Self::Thinking),
            _ => None,
        }
    }

    const fn update(self) -> &'static str {
        match self {
            Self::Text => "agent_message_chunk",
            Self::Thinking => "agent_thought_chunk",
        }
    }

    fn of(block: &Value) -> Option<(Self, &str)> {
        match block["type"].as_str()? {
            "text" => Some((Self::Text, block["text"].as_str()?)),
            "thinking" => Some((
                Self::Thinking,
                block["thinking"].as_str().unwrap_or_default(),
            )),
            _ => None,
        }
    }
}

struct Block {
    turn: String,
    step: u64,
    index: u64,
    kind: Kind,
    shown: String,
    units: usize,
    frozen: bool,
}

impl Block {
    fn key(&self) -> String {
        format!("live:{}:{}:{}", self.turn, self.step, self.index)
    }
}

struct Ask {
    key: String,
    request_id: u64,
    tool: Option<String>,
    agent: Option<String>,
    input: Value,
    suggestions: Value,
    kind: AskKind,
    shown: bool,
}

enum AskKind {
    Approval,
    Plan,
    Question(Vec<AgentQuestion>),
}

#[derive(Default)]
pub(crate) struct Drained {
    pub(crate) live: Vec<(Option<u64>, Value)>,
    pub(crate) finished: bool,
    pub(crate) failed: Option<String>,
}

impl Link {
    pub(crate) fn open(
        mut channel: Channel,
        permission_ids: Arc<AtomicU64>,
        io: Arc<dyn LensPane>,
    ) -> Self {
        let held = channel.greeting();
        Self {
            channel,
            io,
            permission_ids,
            instance: None,
            held,
            strangers: Vec::new(),
            blocks: Vec::new(),
            asks: Vec::new(),
            payloads: Vec::new(),
            commands: 0,
            prompts: Vec::new(),
            unrelayed: None,
            done: None,
            started: false,
        }
    }

    pub(crate) fn greet(&mut self, session: Option<&str>) {
        for hello in std::mem::take(&mut self.held) {
            if let Some(from) = hello["i"].as_str() {
                self.hello(from, &hello, session);
            }
        }
    }

    pub(crate) fn live(&self) -> bool {
        self.instance
            .as_ref()
            .is_some_and(|instance| self.channel.polling(&instance.id))
    }

    pub(crate) fn busy(&self) -> bool {
        self.instance
            .as_ref()
            .is_some_and(|instance| instance.turn.is_some())
    }

    #[cfg(test)]
    pub(crate) fn model(&self) -> Option<&str> {
        self.instance.as_ref()?.model.as_deref()
    }

    pub(crate) fn status(
        &mut self,
        busy: bool,
        asking: bool,
        waiting: Option<String>,
    ) -> (bool, Option<String>) {
        if !self.live() {
            self.unrelayed = None;
            self.resolve(|_| true);
            return (busy, waiting);
        }
        if asking {
            for ask in &mut self.asks {
                ask.shown = true;
            }
        } else {
            self.resolve(|ask| ask.shown);
        }
        let settled = self.done.is_some_and(|done| done.elapsed() < SETTLED);
        let busy = self.busy() || self.started || (busy && !settled);
        if !self.asks.is_empty() || waiting.is_none() {
            self.unrelayed = None;
            return (busy, None);
        }
        let since = *self.unrelayed.get_or_insert_with(Instant::now);
        (busy, waiting.filter(|_| since.elapsed() >= UNRELAYED))
    }

    pub(crate) fn drain(
        &mut self,
        session: Option<&str>,
        cwd: &Path,
        alive: impl FnOnce() -> bool,
    ) -> Drained {
        let mut drained = Drained::default();
        self.started = false;
        self.greet(session);
        if !self.live()
            && let Some(position) = self.strangers.iter().position(|(id, from)| {
                session.is_none_or(|session| session == from) && self.channel.polling(id)
            })
        {
            let (id, _) = self.strangers.remove(position);
            self.adopt(&id, None, None);
        }
        for event in self.channel.drain() {
            self.event(&event, session, cwd, &mut drained);
        }
        self.recover(alive, &mut drained);
        drained
    }

    fn recover(&mut self, alive: impl FnOnce() -> bool, drained: &mut Drained) {
        let lost = self
            .channel
            .lost()
            .iter()
            .filter_map(|command| command["id"].as_u64())
            .collect::<Vec<_>>();
        let current = self
            .instance
            .as_ref()
            .map(|instance| instance.id.clone())
            .filter(|id| self.channel.polling(id));
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.prompts)
            .into_iter()
            .partition(|prompt| {
                lost.contains(&prompt.id) || current.as_ref() != Some(&prompt.instance)
            });
        self.prompts = kept;
        if gone.is_empty() {
            return;
        }
        for prompt in &gone {
            self.channel.abandon(&prompt.instance);
        }
        if !alive() {
            log::debug!(target: "zz::agent", "Claude Code left with {} lens prompt(s) unread", gone.len());
            drained.failed = Some(EXITED.to_owned());
            drained.finished = true;
            return;
        }
        for prompt in gone {
            log::debug!(target: "zz::agent", "the Claude Code plugin left before it took a lens prompt; typing it");
            if let Err(error) = self.io.submit(&prompt.text) {
                drained.failed = Some(error);
                drained.finished = true;
            }
        }
    }

    fn event(&mut self, event: &Value, session: Option<&str>, cwd: &Path, drained: &mut Drained) {
        let Some(from) = event["i"].as_str() else {
            return;
        };
        if event["t"] == "hello" {
            self.hello(from, event, session);
        }
        if self.instance.as_ref().map(|instance| instance.id.as_str()) != Some(from) {
            if let Some(key) = event["key"].as_str() {
                self.channel.answer(key, json!({ "pass": true }));
            }
            return;
        }
        let at = event["at"].as_u64();
        let agent = event["agent"].as_str();
        match event["t"].as_str().unwrap_or_default() {
            "turn" => {
                let turn = event["turn"].as_str().map(str::to_owned);
                self.blocks
                    .retain(|block| !block.frozen || !block.shown.is_empty());
                self.done = None;
                self.started = true;
                if let Some(instance) = self.instance.as_mut() {
                    instance.turn = turn;
                }
            }
            "step" => {
                self.resolve(|ask| ask.agent.as_deref() == agent);
                let turn = event["turn"].as_str();
                if let Some(instance) = self.instance.as_mut().filter(|_| agent.is_none())
                    && turn.is_some()
                    && instance.turn.as_deref() != turn
                {
                    instance.turn = turn.map(str::to_owned);
                    self.done = None;
                    self.started = true;
                }
            }
            "done" if agent.is_none() => {
                if let Some(instance) = self.instance.as_mut() {
                    instance.turn = None;
                }
                self.done = Some(Instant::now());
                drained.finished = true;
                self.resolve(|ask| ask.agent.is_none());
            }
            "done" => self.resolve(|ask| ask.agent.as_deref() == agent),
            "end" => {
                if let Some(instance) = self.instance.as_mut() {
                    instance.turn = None;
                }
                self.resolve(|_| true);
            }
            "used" => {
                let tool = event["id"].as_str();
                self.resolve(|ask| tool.is_some() && ask.tool.as_deref() == tool);
            }
            "ask" => self.ask(event, cwd),
            "model" => {
                if let Some(instance) = self.instance.as_mut() {
                    instance.model = event["model"].as_str().map(str::to_owned);
                }
            }
            "ack" => {
                let prompt = event["id"]
                    .as_u64()
                    .and_then(|id| self.prompts.iter().position(|prompt| prompt.id == id));
                if let Some(position) = prompt {
                    let Prompt { text, .. } = self.prompts.remove(position);
                    if event["ok"] == false {
                        log::debug!(
                            target: "zz::agent",
                            "Claude Code refused the lens prompt ({}); typing it",
                            event["error"].as_str().unwrap_or_default()
                        );
                        if let Err(error) = self.io.submit(&text) {
                            drained.failed = Some(error);
                            drained.finished = true;
                        }
                    }
                }
            }
            kind => {
                if let Some(update) = Kind::named(kind).and_then(|kind| self.chunk(event, kind)) {
                    drained.live.push((at, update));
                }
            }
        }
    }

    fn hello(&mut self, from: &str, event: &Value, session: Option<&str>) {
        let turn = event["turn"].as_str().map(str::to_owned);
        let model = event["model"].as_str().map(str::to_owned);
        if let Some(instance) = self
            .instance
            .as_mut()
            .filter(|instance| instance.id == from)
        {
            instance.turn = turn;
            instance.model = model;
            return;
        }
        if let Some(session) = session
            && event["session"].as_str() != Some(session)
        {
            self.strangers.retain(|(id, _)| id != from);
            if self.strangers.len() >= MAX_STRANGERS {
                self.strangers.remove(0);
            }
            self.strangers.push((
                from.to_owned(),
                event["session"].as_str().unwrap_or_default().to_owned(),
            ));
            return;
        }
        self.adopt(from, turn, model);
    }

    fn adopt(&mut self, from: &str, turn: Option<String>, model: Option<String>) {
        self.strangers.retain(|(id, _)| id != from);
        self.resolve(|_| true);
        self.blocks.clear();
        self.instance = Some(Instance {
            id: from.to_owned(),
            turn,
            model,
        });
    }

    fn chunk(&mut self, event: &Value, kind: Kind) -> Option<Value> {
        let turn = event["turn"].as_str()?;
        let step = event["step"].as_u64()?;
        let index = event["index"].as_u64()?;
        let off = usize::try_from(event["off"].as_u64()?).ok()?;
        let text = event["text"].as_str()?;
        let found = self.blocks.iter().position(|block| {
            block.turn == turn && block.step == step && block.index == index && block.kind == kind
        });
        let position = found.unwrap_or_else(|| {
            if self.blocks.len() >= MAX_BLOCKS {
                self.blocks.remove(0);
            }
            self.blocks.push(Block {
                turn: turn.to_owned(),
                step,
                index,
                kind,
                shown: String::new(),
                units: 0,
                frozen: off != 0,
            });
            self.blocks.len() - 1
        });
        let block = &mut self.blocks[position];
        if block.frozen || text.is_empty() {
            return None;
        }
        if off != block.units {
            block.frozen = true;
            return None;
        }
        block.units += text.encode_utf16().count();
        block.shown.push_str(text);
        let mut update = chunk(kind.update(), &block.key(), text);
        if let Some(at) = event["at"].as_u64() {
            update["_meta"] = json!({ "zz": { "at": at } });
        }
        Some(update)
    }

    pub(crate) fn reconcile(&mut self, entry: &Value, mut updates: Vec<Value>) -> Vec<Value> {
        if entry["type"] != "assistant" || entry["isSidechain"] == true || self.blocks.is_empty() {
            return updates;
        }
        let content = entry["message"]["content"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let index = entry["apiBlockIndex"]
            .as_u64()
            .filter(|_| content.len() == 1);
        for (kind, text) in content.iter().filter_map(Kind::of) {
            let Some(position) = self.blocks.iter().position(|live| {
                live.kind == kind
                    && !live.shown.is_empty()
                    && index.is_none_or(|index| live.index == index)
                    && (text.starts_with(&live.shown) || live.shown.starts_with(text))
            }) else {
                continue;
            };
            let live = self.blocks.remove(position);
            let recorded = updates.iter().position(|update| {
                update["sessionUpdate"] == kind.update()
                    && update["content"]["text"] == text
                    && update["_meta"]["zz"]["parent"].is_null()
            });
            let rest = text
                .strip_prefix(live.shown.as_str())
                .filter(|rest| !rest.is_empty())
                .map(|rest| chunk(kind.update(), &live.key(), rest));
            match (recorded, rest) {
                (Some(at), Some(mut rest)) => {
                    rest["_meta"] = updates[at]["_meta"].clone();
                    updates[at] = rest;
                }
                (Some(at), None) => {
                    updates.remove(at);
                }
                (None, Some(rest)) => updates.push(rest),
                (None, None) => {}
            }
        }
        updates
    }

    pub(crate) fn forget(&mut self) {
        for block in &mut self.blocks {
            block.frozen = true;
            block.shown.clear();
        }
    }

    fn ask(&mut self, event: &Value, cwd: &Path) {
        let Some(key) = event["key"].as_str() else {
            return;
        };
        if self.asks.iter().any(|ask| ask.key == key) {
            return;
        }
        let tool = event["tool"].as_str().unwrap_or_default();
        let input = event["input"].clone();
        let suggestions = event["suggestions"].clone();
        let tool_use = event["id"].as_str().map(str::to_owned);
        let request_id = self.permission_ids.fetch_add(1, Ordering::Relaxed);
        let call = tool_use
            .clone()
            .unwrap_or_else(|| format!("lens-permission-{request_id}"));
        let (kind, (tool_call, options), questions) = match tool {
            "AskUserQuestion" => {
                let questions = question_list(&input);
                if questions.is_empty() {
                    self.channel.answer(key, json!({ "pass": true }));
                    return;
                }
                let card = question_request(&call, &questions);
                (AskKind::Question(questions.clone()), card, questions)
            }
            "ExitPlanMode" => (AskKind::Plan, plan_request(&call, &input), Vec::new()),
            _ => (
                AskKind::Approval,
                approval_request(tool, &call, &input, &suggestions, true, cwd),
                Vec::new(),
            ),
        };
        let mut payload = AgentStreamPayload::PermissionRequested {
            request_id,
            tool_call,
            options,
            questions,
        };
        if validate_payload(&payload).is_err()
            && let AgentStreamPayload::PermissionRequested { tool_call, .. } = &mut payload
            && let Some(fields) = tool_call.as_object_mut()
        {
            fields.remove("rawInput");
            fields.remove("content");
        }
        if validate_payload(&payload).is_err() {
            self.channel.answer(key, json!({ "pass": true }));
            return;
        }
        self.asks.push(Ask {
            key: key.to_owned(),
            request_id,
            tool: tool_use,
            agent: event["agent"].as_str().map(str::to_owned),
            input,
            suggestions,
            kind,
            shown: false,
        });
        self.payloads.push(payload);
    }

    fn resolve(&mut self, gone: impl Fn(&Ask) -> bool) {
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.asks)
            .into_iter()
            .partition(|ask| gone(ask));
        self.asks = kept;
        for ask in gone {
            self.channel.answer(&ask.key, json!({ "pass": true }));
            self.payloads.push(AgentStreamPayload::PermissionResolved {
                request_id: ask.request_id,
                canceled: false,
            });
        }
    }

    pub(crate) fn take_payloads(&mut self) -> Vec<AgentStreamPayload> {
        std::mem::take(&mut self.payloads)
    }

    pub(crate) fn abandon(&mut self) -> Option<String> {
        if self.prompts.is_empty() {
            return None;
        }
        for prompt in std::mem::take(&mut self.prompts) {
            self.channel.abandon(&prompt.instance);
        }
        Some(EXITED.to_owned())
    }

    pub(crate) fn respond(
        &mut self,
        request_id: u64,
        option_id: Option<&str>,
    ) -> Option<Vec<AgentStreamPayload>> {
        let position = self
            .asks
            .iter()
            .position(|ask| ask.request_id == request_id)?;
        let ask = self.asks.remove(position);
        let decision = match &ask.kind {
            AskKind::Approval => match option_id {
                Some("allow") => allow(&ask.input, None),
                Some("allow_always") => allow(&ask.input, Some(ask.suggestions.clone())),
                Some(_) => deny("The user rejected this tool use.", false),
                None => deny("The user cancelled the request.", true),
            },
            AskKind::Plan => match option_id {
                Some("accept_edits") => allow_with_mode(&ask.input, "acceptEdits"),
                Some("approve") => allow_with_mode(&ask.input, "default"),
                Some(_) => deny("The user wants to keep planning.", false),
                None => deny("The user cancelled the request.", true),
            },
            AskKind::Question(questions) => match chosen(questions, option_id) {
                Some(answer) => answered(&ask.input, questions, &[answer]),
                None => deny("The user dismissed the question.", false),
            },
        };
        self.channel
            .answer(&ask.key, json!({ "decision": decision }));
        Some(vec![AgentStreamPayload::PermissionResolved {
            request_id,
            canceled: option_id.is_none(),
        }])
    }

    pub(crate) fn answer(
        &mut self,
        request_id: u64,
        answers: &[AgentQuestionAnswer],
    ) -> Option<Vec<AgentStreamPayload>> {
        let position = self.asks.iter().position(|ask| {
            ask.request_id == request_id && matches!(ask.kind, AskKind::Question(_))
        })?;
        let ask = self.asks.remove(position);
        let AskKind::Question(questions) = &ask.kind else {
            return None;
        };
        self.channel.answer(
            &ask.key,
            json!({ "decision": answered(&ask.input, questions, answers) }),
        );
        Some(vec![AgentStreamPayload::PermissionResolved {
            request_id,
            canceled: false,
        }])
    }

    fn send(&mut self, op: &str, mut command: Value) -> Result<(u64, String), String> {
        let unlinked = "Claude Code's zz plugin is not connected";
        let instance = self.instance.as_ref().ok_or(unlinked)?.id.clone();
        command["id"] = Value::from(self.commands + 1);
        command["op"] = Value::from(op);
        if !self.channel.send(&instance, command) {
            return Err(unlinked.to_owned());
        }
        self.commands += 1;
        Ok((self.commands, instance))
    }

    pub(crate) fn prompt(&mut self, text: &str) -> Result<(), String> {
        let (id, instance) = self.send("prompt", json!({ "text": text }))?;
        self.prompts.push(Prompt {
            id,
            instance,
            text: text.to_owned(),
        });
        Ok(())
    }

    pub(crate) fn set_model(&mut self, value: &str) -> Result<(), String> {
        self.send("model", json!({ "value": value })).map(drop)
    }

    pub(crate) fn stop(&mut self) -> Vec<AgentStreamPayload> {
        let mut payloads = Vec::new();
        for ask in std::mem::take(&mut self.asks) {
            self.channel.answer(
                &ask.key,
                json!({ "decision": deny("The user cancelled the request.", true) }),
            );
            payloads.push(AgentStreamPayload::PermissionResolved {
                request_id: ask.request_id,
                canceled: true,
            });
        }
        if let Err(error) = self.send("abort", json!({})) {
            log::debug!(target: "zz::agent", "could not stop Claude Code: {error}");
        }
        payloads
    }
}

pub(crate) fn before(live: &mut Vec<(Option<u64>, Value)>, at: Option<u64>) -> Vec<Value> {
    let Some(at) = at else {
        return Vec::new();
    };
    let due = live
        .iter()
        .take_while(|(stamp, _)| stamp.is_none_or(|stamp| stamp <= at))
        .count();
    live.drain(..due).map(|(_, update)| update).collect()
}
