use std::collections::{HashMap, HashSet};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use zz_machine::Machine;
use zz_protocol::AgentQuestionAnswer;
use zz_protocol::agent_stream::{AgentQuestion, AgentStreamPayload};
use zz_protocol::chat::{
    Answer, Ask, AskOption, ChatEvent, ItemBody, ItemId, ItemKind, ItemStatus, OptionKind,
    PlanStep, QuestionAnswer, RequestId, StepStatus, Usage,
};

use super::chat::{ChatMachine, ChatState, Ctx, Effect, Fact, Reject};
use super::turn::{self, End, Outcome, TurnMachine};

const HISTORY_LIMIT: usize = 20_000;
const TERMINAL_ANSWERS: u64 = 1 << 48;

#[derive(Default)]
pub(crate) struct Shadow {
    pub(crate) state: ChatState,
    bodies: HashMap<ItemId, ItemBody>,
    plan: Option<ItemId>,
    plans: u64,
    splits: HashMap<ItemId, u32>,
    asked: HashMap<u64, ItemId>,
    answered_items: HashSet<ItemId>,
    answers: u64,
    backgrounded: HashSet<ItemId>,
    attached: bool,
    history: Vec<AgentStreamPayload>,
    pub(crate) events: Vec<ChatEvent>,
    outbox: Vec<ChatEvent>,
    pub(crate) rejects: Vec<(Fact, Reject)>,
    keep: bool,
    codex: bool,
    now: Option<Instant>,
    replaying: bool,
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

    #[must_use]
    pub(crate) fn attached(mut self) -> Self {
        self.attached = true;
        self
    }

    pub(crate) fn answered(&mut self, request: u64, answer: Answer) {
        if let Some(item) = self.asked.get(&request) {
            self.answered_items.insert(item.clone());
        }
        self.step(Fact::RequestAnswered {
            request: RequestId(request),
            answer,
        });
    }

    pub(crate) fn turn(&mut self, fact: &turn::Fact, session: Option<&str>, now: Instant) {
        self.now = Some(now);
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
                self.backgrounded.clear();
                self.bodies.clear();
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
                self.step(Fact::RequestOpened {
                    request: RequestId(*request_id),
                    item,
                    ask: ask(tool_call, options, questions),
                    blocking: true,
                });
            }
            AgentStreamPayload::PermissionResolved { request_id, .. } => {
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
        if self.attached
            && kind == ItemKind::User
            && !self.replaying
            && self.state.current().is_none()
        {
            self.step(Fact::Turn(turn::Fact::Busy));
        }
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
            body: ItemBody::text(text),
            background: false,
        });
        if whole || alone {
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
        let item = ItemId::new(id);
        let body = ItemBody {
            title: update["title"].as_str().unwrap_or_default().to_owned(),
            target: target(update),
            text: content_text(&update["content"]).unwrap_or_default(),
            ..ItemBody::default()
        };
        let parent = update["_meta"]["zz"]["parent"].as_str().map(ItemId::new);
        self.step(Fact::ItemStarted {
            item: item.clone(),
            parent,
            kind: tool_kind(update),
            body: body.clone(),
            background: false,
        });
        if self.open(&item) {
            self.bodies.insert(item.clone(), body);
        }
        self.background_tool(&item, update);
        self.settle_tool(&item, update);
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
                body,
            });
        }
        self.background_tool(&item, update);
        self.answered_in_terminal(&item, update);
        self.settle_tool(&item, update);
    }

    fn answered_in_terminal(&mut self, item: &ItemId, update: &Value) {
        let Some(answered) = update["_meta"]["zz"]["answered"].as_array() else {
            return;
        };
        if self.answered_items.contains(item) || !self.open(item) {
            return;
        }
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
        self.answers += 1;
        let request = RequestId(TERMINAL_ANSWERS + self.answers);
        self.step(Fact::RequestOpened {
            request,
            item: Some(item.clone()),
            ask: Ask::Questions { questions },
            blocking: false,
        });
        self.answered(request.0, Answer::Questions { answers });
    }

    fn settle_tool(&mut self, item: &ItemId, update: &Value) {
        let status = match update["status"].as_str() {
            Some("completed") => ItemStatus::Completed,
            Some("failed") => ItemStatus::Failed { message: None },
            _ => return,
        };
        self.bodies.remove(item);
        self.step(Fact::ItemEnded {
            item: item.clone(),
            status,
        });
    }

    fn plan(&mut self, update: &Value) {
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
            .filter(|(_, kind, turn)| !kind.is_text() && turn.is_some() && *turn == current)
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
        self.replaying = false;
        let leftover = self
            .state
            .open_kinds()
            .filter(|(id, _, _)| {
                !before.contains(id)
                    && !self.state.asked_about(id)
                    && (!self.attached || !self.state.within(id, &self.backgrounded))
            })
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
            at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |since| {
                    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
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
        if let Fact::ItemBackgrounded { item } = &fact {
            self.backgrounded.insert(item.clone());
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
    Ask::Approval {
        title: tool_call["title"].as_str().unwrap_or_default().to_owned(),
        detail: None,
        options,
    }
}

fn tool_kind(update: &Value) -> ItemKind {
    let name = update["_meta"]["claudeCode"]["toolName"].as_str();
    let codex = update["_meta"]["codex"]["itemType"].as_str();
    if matches!(name, Some("Agent" | "Task"))
        || matches!(codex, Some("collabAgentToolCall" | "subAgentActivity"))
    {
        return ItemKind::Subagent;
    }
    if name.is_some_and(|name| name.starts_with("mcp__")) || codex == Some("mcpToolCall") {
        return ItemKind::Mcp;
    }
    match update["kind"].as_str() {
        Some("read") => ItemKind::Read,
        Some("edit" | "delete" | "move") => ItemKind::Edit,
        Some("search") => ItemKind::Search,
        Some("execute") => ItemKind::Execute,
        Some("fetch") => ItemKind::Fetch,
        _ if matches!(name, Some("WebSearch")) || codex == Some("webSearch") => ItemKind::Web,
        _ => ItemKind::Tool,
    }
}

fn target(update: &Value) -> Option<String> {
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
