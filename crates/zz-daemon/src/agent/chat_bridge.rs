use std::collections::{HashMap, HashSet};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize as _;
use serde_json::{Value, json};
use zz_machine::Machine;
use zz_protocol::agent_stream::{AgentImage, AgentQuestion, AgentStreamPayload};
use zz_protocol::chat::{
    Answer, Ask, AskOption, ChatEvent, ItemBody, ItemId, ItemKind, ItemStatus, OptionKind,
    PlanStep, QuestionAnswer, RequestId, StepStatus, Usage,
};
use zz_protocol::{
    AgentQuestionAnswer, MAX_AGENT_IMAGE_FORMAT_BYTES, MAX_AGENT_PROMPT_BYTES,
    MAX_AGENT_PROMPT_IMAGES,
};

use super::chat::{ChatMachine, ChatState, Ctx, Effect, Fact, Reject};
use super::turn::{self, End, Outcome, TurnMachine};

const HISTORY_LIMIT: usize = 20_000;
const TERMINAL_ANSWERS: u64 = 1 << 48;
const REPLAYED_TURNS: u64 = 1 << 48;
const MAX_FAILURE_CHARS: usize = 1000;
pub(crate) const MAX_PROMPT_IMAGES_BASE64: usize = MAX_AGENT_PROMPT_BYTES;

#[derive(Default)]
pub(crate) struct Shadow {
    pub(crate) state: ChatState,
    bodies: HashMap<ItemId, ItemBody>,
    plan: Option<ItemId>,
    plans: u64,
    splits: HashMap<ItemId, u32>,
    loose: Option<ItemId>,
    asked: HashMap<u64, ItemId>,
    denials: HashMap<u64, (ItemId, Vec<String>)>,
    declined: HashSet<ItemId>,
    answered_items: HashSet<ItemId>,
    answers: u64,
    history: Vec<AgentStreamPayload>,
    pub(crate) events: Vec<ChatEvent>,
    outbox: Vec<ChatEvent>,
    pub(crate) rejects: Vec<(Fact, Reject)>,
    keep: bool,
    codex: bool,
    now: Option<Instant>,
    stamp: Option<u64>,
    replaying: bool,
    replayed: u64,
    replayed_command: Option<String>,
    session: Option<String>,
}

impl Shadow {
    #[cfg(test)]
    pub(crate) fn recording() -> Self {
        Self {
            keep: true,
            ..Self::default()
        }
    }

    pub(crate) fn drain(&mut self) -> Vec<ChatEvent> {
        std::mem::take(&mut self.outbox)
    }

    #[must_use]
    pub(crate) fn codex(mut self) -> Self {
        self.codex = true;
        self
    }

    pub(crate) fn answered(&mut self, request: u64, answer: Answer) {
        if let Some(item) = self.asked.get(&request) {
            self.answered_items.insert(item.clone());
        }
        if let Some((item, denials)) = self.denials.remove(&request) {
            let declined = match &answer {
                Answer::Cancel => true,
                Answer::Option { id } => denials.contains(id),
                Answer::Questions { .. } => false,
            };
            if declined {
                self.declined.insert(item);
            }
        }
        self.step(Fact::RequestAnswered {
            request: RequestId(request),
            answer,
        });
    }

    pub(crate) fn turn(&mut self, fact: &turn::Fact, session: Option<&str>, now: Instant) {
        self.now = Some(now);
        if !matches!(fact, turn::Fact::Tick) {
            self.settle_loose();
        }
        if self.codex
            && matches!(
                fact,
                turn::Fact::Ended {
                    end: End::Completed,
                    ..
                }
            )
            && completes(&self.state, fact, now)
        {
            self.background_running();
        }
        let ready = matches!(fact, turn::Fact::SessionReady);
        if ready
            && let Some(session) = session.filter(|session| !session.is_empty())
            && self.session.as_deref() != Some(session)
        {
            if self.session.is_some() {
                self.splits.clear();
                self.bodies.clear();
                self.denials.clear();
                self.declined.clear();
                self.plan = None;
            }
            self.session = Some(session.to_owned());
        }
        let fact = match fact {
            turn::Fact::SessionReady => Fact::SessionReady {
                session: session.unwrap_or_default().to_owned(),
            },
            turn::Fact::SessionClosed => {
                self.history.clear();
                Fact::SessionClosed
            }
            turn::Fact::Exited { message } => Fact::Exited {
                message: message.clone(),
            },
            turn::Fact::PermissionOpened { .. } | turn::Fact::PermissionClosed { .. } => return,
            other => Fact::Turn(other.clone()),
        };
        self.step(fact);
        self.now = None;
        if ready && self.state.live() {
            self.replay_history();
        }
    }

    pub(crate) fn payload(&mut self, payload: &AgentStreamPayload) {
        if matches!(payload, AgentStreamPayload::SessionReset { .. }) {
            self.history.retain(|payload| {
                matches!(
                    payload,
                    AgentStreamPayload::PermissionRequested { .. }
                        | AgentStreamPayload::PermissionResolved { .. }
                )
            });
            return;
        }
        if !self.state.live() {
            if matches!(
                payload,
                AgentStreamPayload::Update { .. }
                    | AgentStreamPayload::TasksChanged { .. }
                    | AgentStreamPayload::PermissionRequested { .. }
                    | AgentStreamPayload::PermissionResolved { .. }
            ) && self.history.len() < HISTORY_LIMIT
            {
                self.history.push(payload.clone());
            }
            return;
        }
        match payload {
            AgentStreamPayload::Update { update } => self.update(update),
            AgentStreamPayload::PermissionRequested {
                request_id,
                tool_call,
                options,
                questions,
            } => {
                let item = tool_call["toolCallId"].as_str().map(ItemId::new);
                if let Some(item) = item.clone().filter(|_| !questions.is_empty()) {
                    self.asked.insert(*request_id, item);
                }
                let ask = ask(tool_call, options, questions);
                if let Some(item) = item.clone() {
                    self.denials.insert(*request_id, (item, denials(&ask)));
                }
                self.step(Fact::RequestOpened {
                    request: RequestId(*request_id),
                    item,
                    ask,
                    blocking: tool_call["_meta"]["zz"]["background"] != true,
                });
            }
            AgentStreamPayload::PermissionResolved { request_id, .. } => {
                self.denials.remove(request_id);
                self.step(Fact::RequestWithdrawn {
                    request: RequestId(*request_id),
                });
            }
            AgentStreamPayload::TasksChanged { tasks } => {
                for task in tasks {
                    if let Some(tool) = &task.tool_call_id {
                        let item = ItemId::new(tool.clone());
                        if self.open(&item) || self.state.waits_for_parent(&item) {
                            self.step(Fact::ItemBackgrounded { item });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn update(&mut self, update: &Value) {
        self.stamp = update["_meta"]["zz"]["at"].as_u64();
        self.dispatch(update);
        self.stamp = None;
    }

    fn dispatch(&mut self, update: &Value) {
        match update["sessionUpdate"].as_str() {
            Some("user_message_chunk") => self.text(update, "u", ItemKind::User, true),
            Some("agent_message_chunk") => {
                let notice = !update["_meta"]["zz"]["notice"].is_null();
                let kind = if notice {
                    ItemKind::Notice
                } else {
                    ItemKind::Agent
                };
                self.text(update, "a", kind, notice);
            }
            Some("agent_thought_chunk") => self.text(update, "r", ItemKind::Reasoning, false),
            Some("tool_call") => self.tool_call(update),
            Some("tool_call_update") => self.tool_update(update),
            Some("plan") => self.plan(update),
            Some("session_info_update") => self.session_info(update),
            Some("usage_update") => {
                if let (Some(used), Some(size)) = (update["used"].as_u64(), update["size"].as_u64())
                {
                    self.step(Fact::Usage {
                        usage: Usage { used, size },
                    });
                }
            }
            _ => {}
        }
    }

    fn text(&mut self, update: &Value, prefix: &str, kind: ItemKind, whole: bool) {
        let Some(text) = update["content"]["text"].as_str() else {
            return;
        };
        let parent = update["_meta"]["zz"]["parent"].as_str().map(ItemId::new);
        let whole = whole || parent.is_some();
        let key = update["messageId"].as_str().unwrap_or("message");
        let base = ItemId::new(format!("{prefix}:{key}"));
        let split = self.splits.get(&base).copied().unwrap_or(0);
        let item = if split == 0 {
            base.clone()
        } else {
            ItemId::new(format!("{prefix}:{key}~{split}"))
        };
        if self.loose.as_ref().is_some_and(|loose| *loose != item) {
            self.settle_loose();
        }
        if self.open(&item) {
            self.step(Fact::ItemAppended {
                item,
                text: text.to_owned(),
            });
            return;
        }
        if self.replaying && self.started(&item) {
            return;
        }
        let item = if self.started(&item) {
            let split = split + 1;
            self.splits.insert(base, split);
            ItemId::new(format!("{prefix}:{key}~{split}"))
        } else {
            item
        };
        let alone = self.state.current().is_none() && !self.replaying;
        self.step(Fact::ItemStarted {
            item: item.clone(),
            parent,
            kind,
            body: ItemBody {
                text: text.to_owned(),
                images: images(update),
                ..ItemBody::default()
            },
            background: false,
        });
        if whole {
            self.step(Fact::ItemEnded {
                item,
                status: ItemStatus::Completed,
            });
        } else if alone {
            self.loose = Some(item);
        }
    }

    fn settle_loose(&mut self) {
        if let Some(item) = self.loose.take()
            && self.open(&item)
        {
            self.step(Fact::ItemEnded {
                item,
                status: ItemStatus::Completed,
            });
        }
    }

    fn tool_call(&mut self, update: &Value) {
        let Some(id) = update["toolCallId"].as_str() else {
            return;
        };
        self.settle_loose();
        let item = ItemId::new(id);
        let body = ItemBody {
            title: update["title"].as_str().unwrap_or_default().to_owned(),
            target: target(update),
            text: content_text(&update["content"]).unwrap_or_default(),
            activity: update["_meta"]["zz"]["activity"]
                .as_str()
                .map(str::trim)
                .filter(|activity| !activity.is_empty())
                .map(str::to_owned),
            ..ItemBody::default()
        };
        let parent = update["_meta"]["zz"]["parent"].as_str().map(ItemId::new);
        let ended = self.stamp;
        if let Some(ms) = update["_meta"]["zz"]["ms"].as_u64() {
            self.stamp = ended.map(|at| at.saturating_sub(ms));
        }
        self.step(Fact::ItemStarted {
            item: item.clone(),
            parent,
            kind: tool_kind(update),
            body: body.clone(),
            background: false,
        });
        self.stamp = ended;
        if self.open(&item) {
            self.bodies.insert(item.clone(), body.clone());
        }
        self.background_tool(&item, update);
        self.settle_tool(&item, update, &body);
    }

    fn background_tool(&mut self, item: &ItemId, update: &Value) {
        if update["_meta"]["zz"]["background"] == true
            && (self.open(item) || self.state.waits_for_parent(item))
        {
            self.step(Fact::ItemBackgrounded { item: item.clone() });
        }
    }

    fn tool_update(&mut self, update: &Value) {
        let Some(id) = update["toolCallId"].as_str() else {
            return;
        };
        let item = ItemId::new(id);
        let mut body = self.bodies.get(&item).cloned().unwrap_or_default();
        let mut changed = false;
        if let Some(title) = update["title"]
            .as_str()
            .filter(|title| *title != body.title)
        {
            title.clone_into(&mut body.title);
            changed = true;
        }
        if update["_meta"]["zz"].get("target").is_some() {
            let target = target(update);
            if target != body.target {
                body.target = target;
                changed = true;
            }
        }
        if let Some(text) = content_text(&update["content"]) {
            body.text = text;
            changed = true;
        }
        if let Some(code) = update["_meta"]["zz"]["exitCode"]
            .as_i64()
            .and_then(|code| i32::try_from(code).ok())
        {
            body.exit_code = Some(code);
            changed = true;
        }
        if changed {
            if self.open(&item) {
                self.bodies.insert(item.clone(), body.clone());
            }
            self.step(Fact::ItemChanged {
                item: item.clone(),
                body: body.clone(),
            });
        }
        self.background_tool(&item, update);
        self.answered_in_terminal(&item, update);
        self.settle_tool(&item, update, &body);
    }

    fn answered_in_terminal(&mut self, item: &ItemId, update: &Value) {
        if self.answered_items.contains(item) || !self.open(item) {
            return;
        }
        let zz = &update["_meta"]["zz"];
        let declined = Vec::<AgentQuestion>::deserialize(&zz["questions"])
            .ok()
            .filter(|questions| zz["declined"] == true && !questions.is_empty());
        if let Some(questions) = declined {
            self.asked_in_terminal(item, questions, Answer::Cancel);
            return;
        }
        let Some(answered) = zz["answered"].as_array() else {
            return;
        };
        let (questions, answers) = answered
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let id = format!("q{index}");
                let question = AgentQuestion {
                    id: id.clone(),
                    header: entry["header"].as_str().map(str::to_owned),
                    question: entry["question"].as_str().unwrap_or_default().to_owned(),
                    ..AgentQuestion::default()
                };
                let answer = QuestionAnswer {
                    question: id,
                    answers: vec![entry["answer"].as_str().unwrap_or_default().to_owned()],
                };
                (question, answer)
            })
            .unzip();
        self.asked_in_terminal(item, questions, Answer::Questions { answers });
    }

    fn asked_in_terminal(&mut self, item: &ItemId, questions: Vec<AgentQuestion>, answer: Answer) {
        self.answers += 1;
        let request = RequestId(TERMINAL_ANSWERS + self.answers);
        self.step(Fact::RequestOpened {
            request,
            item: Some(item.clone()),
            ask: Ask::Questions { questions },
            blocking: false,
        });
        self.answered(request.0, answer);
    }

    fn settle_tool(&mut self, item: &ItemId, update: &Value, body: &ItemBody) {
        let status = match update["status"].as_str() {
            Some("completed") => ItemStatus::Completed,
            Some("failed")
                if self.declined.contains(item) || update["_meta"]["zz"]["declined"] == true =>
            {
                ItemStatus::Declined
            }
            Some("failed") => {
                let command = body.exit_code.is_some()
                    || self
                        .state
                        .open_kinds()
                        .any(|(id, kind, _)| id == item && kind == ItemKind::Execute);
                ItemStatus::Failed {
                    message: failure(&body.text, command),
                }
            }
            _ => return,
        };
        self.declined.remove(item);
        self.bodies.remove(item);
        self.step(Fact::ItemEnded {
            item: item.clone(),
            status,
        });
    }

    fn replayed_turn(&mut self, mark: &str) {
        if mark != "started" {
            let end = if mark == "cancelled" {
                End::Cancelled
            } else {
                End::Completed
            };
            self.end_replayed_turn(end);
            return;
        }
        self.end_replayed_turn(End::Completed);
        self.replayed += 1;
        let turn = REPLAYED_TURNS + self.replayed;
        let command = format!("replay-{}", self.replayed);
        self.step(Fact::Turn(turn::Fact::Prompt {
            turn,
            command: Some(command.clone()),
        }));
        let owned = self
            .state
            .turn
            .turn()
            .is_some_and(|running| running.owner == Some(turn));
        if owned {
            self.replayed_command = Some(command);
        }
    }

    fn end_replayed_turn(&mut self, end: End) {
        if let Some(command) = self.replayed_command.take() {
            self.step(Fact::Turn(turn::Fact::Ended { command, end }));
        }
    }

    fn session_info(&mut self, update: &Value) {
        let zz = &update["_meta"]["zz"];
        if self.replaying
            && let Some(mark) = zz["turn"].as_str()
        {
            self.replayed_turn(mark);
        }
        if let (Some(summary), Some(ids)) = (
            zz["summary"].as_str().map(str::trim),
            zz["toolCallIds"].as_array(),
        ) && !summary.is_empty()
        {
            let items = ids
                .iter()
                .filter_map(Value::as_str)
                .map(ItemId::new)
                .collect::<Vec<_>>();
            if !items.is_empty() {
                self.step(Fact::Summarized {
                    items,
                    summary: summary.to_owned(),
                });
            }
        }
        if let Some(activity) = zz.get("activity") {
            self.step(Fact::Activity {
                text: activity
                    .as_str()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(str::to_owned),
            });
        }
    }

    fn plan(&mut self, update: &Value) {
        self.settle_loose();
        let steps = update["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|entry| PlanStep {
                text: entry["content"].as_str().unwrap_or_default().to_owned(),
                status: match entry["status"].as_str() {
                    Some("in_progress") => StepStatus::Running,
                    Some("completed") => StepStatus::Done,
                    Some("cancelled") => StepStatus::Cancelled,
                    _ => StepStatus::Pending,
                },
            })
            .collect::<Vec<_>>();
        let body = ItemBody {
            steps,
            ..ItemBody::default()
        };
        if let Some(item) = self.plan.clone().filter(|item| self.open(item)) {
            self.step(Fact::ItemChanged { item, body });
            return;
        }
        self.plans += 1;
        let item = ItemId::new(format!("plan:{}", self.plans));
        self.plan = Some(item.clone());
        let alone = self.state.current().is_none() && !self.replaying;
        self.step(Fact::ItemStarted {
            item: item.clone(),
            parent: None,
            kind: ItemKind::Plan,
            body,
            background: false,
        });
        if alone {
            self.step(Fact::ItemEnded {
                item,
                status: ItemStatus::Completed,
            });
        }
    }

    fn background_running(&mut self) {
        let current = self.state.current();
        let running = self
            .state
            .open_kinds()
            .filter(|(id, kind, turn)| {
                !kind.is_text()
                    && turn.is_some()
                    && *turn == current
                    && !self.asked.values().any(|asked| asked == *id)
            })
            .map(|(id, _, _)| id.clone())
            .collect::<Vec<_>>();
        for item in running {
            self.step(Fact::ItemBackgrounded { item });
        }
    }

    fn replay_history(&mut self) {
        let before = self.state.open_items().cloned().collect::<Vec<_>>();
        self.replaying = true;
        for payload in std::mem::take(&mut self.history) {
            self.payload(&payload);
        }
        self.end_replayed_turn(End::Completed);
        self.replaying = false;
        let leftover = self
            .state
            .open_kinds()
            .filter(|(id, _, _)| !before.contains(id) && !self.state.asked_about(id))
            .map(|(id, kind, _)| (id.clone(), kind))
            .collect::<Vec<_>>();
        for (item, kind) in leftover.into_iter().rev() {
            let status = if kind.is_text() {
                ItemStatus::Completed
            } else {
                ItemStatus::Interrupted
            };
            self.step(Fact::ItemEnded { item, status });
        }
    }

    fn open(&self, item: &ItemId) -> bool {
        self.state.open_items().any(|open| open == item)
    }

    fn started(&self, item: &ItemId) -> bool {
        self.state.remembers(item)
    }

    fn step(&mut self, fact: Fact) -> Vec<Effect> {
        let ctx = Ctx {
            now: self.now.unwrap_or_else(Instant::now),
            at: self.stamp.unwrap_or_else(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |since| {
                        u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
                    })
            }),
        };
        let step = ChatMachine::step(&mut self.state, &fact, &ctx);
        if let Some(reject) = step.rejected {
            log::debug!(target: "zz::agent", "chat machine kept {fact:?} out: {reject:?}");
            if self.keep {
                self.rejects.push((fact, reject));
            }
            return Vec::new();
        }
        let mut effects = Vec::new();
        for effect in step.effects {
            match effect {
                Effect::Emit(event) => {
                    match &event {
                        ChatEvent::ItemEnded { item, .. } => {
                            self.bodies.remove(item);
                        }
                        ChatEvent::SessionEnded { .. } => self.bodies.clear(),
                        _ => {}
                    }
                    if self.keep {
                        self.events.push(event.clone());
                    }
                    self.outbox.push(event);
                }
                other => effects.push(other),
            }
        }
        effects
    }
}

pub(crate) fn attach_images(
    update: &mut Value,
    images: impl IntoIterator<Item = (String, String)>,
) {
    let text = update["content"]["text"].as_str().map_or(0, str::len);
    let mut left = MAX_PROMPT_IMAGES_BASE64.saturating_sub(text);
    let images = images
        .into_iter()
        .filter(|(format, data)| {
            let fits = !format.is_empty()
                && format.len() <= MAX_AGENT_IMAGE_FORMAT_BYTES
                && !data.is_empty()
                && data.len() <= left;
            if fits {
                left -= data.len();
            }
            fits
        })
        .take(MAX_AGENT_PROMPT_IMAGES)
        .map(|(format, data)| json!({ "format": format, "data": data }))
        .collect::<Vec<_>>();
    if !images.is_empty() {
        update["_meta"]["zz"]["images"] = Value::Array(images);
    }
}

pub(crate) fn without_images(mut payload: AgentStreamPayload) -> AgentStreamPayload {
    if let AgentStreamPayload::Update { update } = &mut payload
        && let Some(zz) = update
            .get_mut("_meta")
            .and_then(|meta| meta.get_mut("zz"))
            .and_then(Value::as_object_mut)
    {
        zz.remove("images");
    }
    payload
}

fn images(update: &Value) -> Vec<AgentImage> {
    Vec::<AgentImage>::deserialize(&update["_meta"]["zz"]["images"]).unwrap_or_default()
}

pub(crate) fn chat_answer(option: Option<&str>) -> Answer {
    option.map_or(Answer::Cancel, |id| Answer::Option { id: id.to_owned() })
}

pub(crate) fn chat_answers(answers: &[AgentQuestionAnswer]) -> Answer {
    Answer::Questions {
        answers: answers
            .iter()
            .map(|answer| QuestionAnswer {
                question: answer.id.clone(),
                answers: answer.answers.clone(),
            })
            .collect(),
    }
}

fn completes(state: &ChatState, fact: &turn::Fact, now: Instant) -> bool {
    let decision = TurnMachine::transition(&state.turn, fact, &turn::Ctx { now });
    decision.rejected.is_none()
        && decision.effects.iter().any(|effect| {
            matches!(
                effect,
                turn::Effect::Settle {
                    outcome: Outcome::Completed { .. },
                    ..
                }
            )
        })
}

fn ask(tool_call: &Value, options: &Value, questions: &[AgentQuestion]) -> Ask {
    if !questions.is_empty() {
        return Ask::Questions {
            questions: questions.to_vec(),
        };
    }
    let options = options
        .as_array()
        .into_iter()
        .flatten()
        .map(|option| AskOption {
            id: option["optionId"].as_str().unwrap_or_default().to_owned(),
            label: option["name"].as_str().unwrap_or_default().to_owned(),
            kind: match option["kind"].as_str() {
                Some("allow_once") => OptionKind::Allow,
                Some("allow_always") => OptionKind::AllowAlways,
                Some("reject_once" | "reject_always") => OptionKind::Deny,
                _ => OptionKind::Other,
            },
        })
        .collect();
    if tool_call["kind"] == "switch_mode" {
        let markdown = tool_call["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|block| block["content"]["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        if !markdown.trim().is_empty() {
            return Ask::Plan { markdown, options };
        }
    }
    Ask::Approval {
        title: tool_call["title"].as_str().unwrap_or_default().to_owned(),
        detail: None,
        options,
    }
}

fn denials(ask: &Ask) -> Vec<String> {
    match ask {
        Ask::Approval { options, .. } | Ask::Plan { options, .. } => options
            .iter()
            .filter(|option| option.kind == OptionKind::Deny)
            .map(|option| option.id.clone())
            .collect(),
        Ask::Questions { .. } => Vec::new(),
    }
}

fn failure(text: &str, command: bool) -> Option<String> {
    let mut lines = text
        .lines()
        .skip_while(|line| line.trim().is_empty())
        .peekable();
    if command
        && lines.peek().is_some_and(|line| {
            line.trim()
                .strip_prefix("Exit code ")
                .is_some_and(|code| code.parse::<i64>().is_ok())
        })
    {
        lines.next();
    }
    let message = if command {
        lines
            .filter(|line| !line.trim().is_empty())
            .last()?
            .trim()
            .to_owned()
    } else {
        lines.collect::<Vec<_>>().join("\n").trim().to_owned()
    };
    if message.is_empty() {
        return None;
    }
    if message.chars().count() <= MAX_FAILURE_CHARS {
        return Some(message);
    }
    let mut clipped = message.chars().take(MAX_FAILURE_CHARS).collect::<String>();
    clipped.push('…');
    Some(clipped)
}

fn tool_kind(update: &Value) -> ItemKind {
    let name = update["_meta"]["claudeCode"]["toolName"].as_str();
    let codex = update["_meta"]["codex"]["itemType"].as_str();
    let spawned = codex == Some("collabAgentToolCall")
        && matches!(
            update["rawInput"]["tool"].as_str(),
            None | Some("spawnAgent")
        );
    if matches!(name, Some("Agent" | "Task")) || codex == Some("subAgentActivity") || spawned {
        return ItemKind::Subagent;
    }
    if name.is_some_and(|name| name.starts_with("mcp__")) || codex == Some("mcpToolCall") {
        return ItemKind::Mcp;
    }
    if name == Some("WebSearch") || codex == Some("webSearch") {
        return ItemKind::Web;
    }
    match update["kind"].as_str() {
        Some("read") => ItemKind::Read,
        Some("edit" | "delete" | "move") => ItemKind::Edit,
        Some("search") => ItemKind::Search,
        Some("execute") => ItemKind::Execute,
        Some("fetch") => ItemKind::Fetch,
        _ => ItemKind::Tool,
    }
}

fn target(update: &Value) -> Option<String> {
    if let Some(target) = update["_meta"]["zz"].get("target") {
        return target
            .as_str()
            .filter(|target| !target.trim().is_empty())
            .map(str::to_owned);
    }
    update["locations"][0]["path"]
        .as_str()
        .or_else(|| update["rawInput"]["command"].as_str())
        .or_else(|| update["rawInput"]["pattern"].as_str())
        .or_else(|| update["rawInput"]["url"].as_str())
        .or_else(|| update["rawInput"]["query"].as_str())
        .or_else(|| update["rawInput"]["prompt"].as_str())
        .map(str::to_owned)
}

fn content_text(content: &Value) -> Option<String> {
    let parts = content
        .as_array()?
        .iter()
        .filter_map(|part| {
            part["content"]["text"]
                .as_str()
                .or_else(|| part["text"].as_str())
        })
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join("\n"))
}
