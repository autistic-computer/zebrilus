use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::agent_stream::AgentQuestion;

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(transparent)]
pub struct TurnId(pub u64);

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ItemId(pub String);

impl ItemId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(transparent)]
pub struct RequestId(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "by")]
pub enum TurnBy {
    User { prompt: u64 },
    Agent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "outcome")]
pub enum TurnOutcome {
    Completed { stop_reason: String },
    Cancelled,
    Failed { message: String },
    Dropped,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    User,
    Agent,
    Reasoning,
    Plan,
    Notice,
    Error,
    Subagent,
    Read,
    Edit,
    Search,
    Execute,
    Fetch,
    Web,
    Mcp,
    Tool,
}

impl ItemKind {
    pub fn is_text(self) -> bool {
        matches!(
            self,
            Self::User | Self::Agent | Self::Reasoning | Self::Plan | Self::Notice | Self::Error
        )
    }

    pub fn is_tool(self) -> bool {
        matches!(
            self,
            Self::Read
                | Self::Edit
                | Self::Search
                | Self::Execute
                | Self::Fetch
                | Self::Web
                | Self::Mcp
                | Self::Tool
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum ItemStatus {
    Running,
    Completed,
    Failed { message: Option<String> },
    Declined,
    Interrupted,
}

impl ItemStatus {
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Running)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub text: String,
    pub status: StepStatus,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemBody {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<PlanStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

impl ItemBody {
    pub fn titled(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OptionKind {
    Allow,
    AllowAlways,
    Deny,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskOption {
    pub id: String,
    pub label: String,
    pub kind: OptionKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "ask")]
pub enum Ask {
    Approval {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        options: Vec<AskOption>,
    },
    Questions {
        questions: Vec<AgentQuestion>,
    },
    Plan {
        markdown: String,
        options: Vec<AskOption>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionAnswer {
    pub question: String,
    pub answers: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "answer")]
pub enum Answer {
    Option { id: String },
    Questions { answers: Vec<QuestionAnswer> },
    Cancel,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "close")]
pub enum RequestClose {
    Answered { answer: Answer },
    Withdrawn,
    Dismissed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "end")]
pub enum SessionEnd {
    Closed,
    Exited { message: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedPrompt {
    pub id: u64,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub used: u64,
    pub size: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "event")]
pub enum ChatEvent {
    SessionReady {
        session: String,
    },
    SessionEnded {
        end: SessionEnd,
    },
    TurnStarted {
        turn: TurnId,
        by: TurnBy,
        at: u64,
    },
    TurnClaimed {
        turn: TurnId,
        prompt: u64,
    },
    TurnStopping {
        turn: TurnId,
    },
    TurnEnded {
        turn: TurnId,
        outcome: TurnOutcome,
        at: u64,
    },
    ItemStarted {
        item: ItemId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn: Option<TurnId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<ItemId>,
        kind: ItemKind,
        body: ItemBody,
        at: u64,
    },
    ItemAppended {
        item: ItemId,
        text: String,
    },
    ItemChanged {
        item: ItemId,
        body: ItemBody,
    },
    ItemBackgrounded {
        item: ItemId,
    },
    ItemEnded {
        item: ItemId,
        status: ItemStatus,
        at: u64,
    },
    RequestOpened {
        request: RequestId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn: Option<TurnId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item: Option<ItemId>,
        ask: Ask,
        blocking: bool,
        at: u64,
    },
    RequestClosed {
        request: RequestId,
        close: RequestClose,
        at: u64,
    },
    QueueChanged {
        prompts: Vec<QueuedPrompt>,
    },
    UsageChanged {
        usage: Usage,
    },
    Restart,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum SessionView {
    #[default]
    Starting,
    Ready {
        session: String,
    },
    Ended(SessionEnd),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChatPhase {
    Starting,
    Idle,
    Running,
    Waiting,
    Stopping,
    Ended,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnView {
    pub id: TurnId,
    pub by: TurnBy,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub outcome: Option<TurnOutcome>,
    pub stopping: bool,
    pub items: Vec<ItemId>,
}

impl TurnView {
    pub fn is_open(&self) -> bool {
        self.outcome.is_none()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemView {
    pub id: ItemId,
    pub kind: ItemKind,
    pub body: ItemBody,
    pub status: ItemStatus,
    pub turn: Option<TurnId>,
    pub parent: Option<ItemId>,
    pub children: Vec<ItemId>,
    pub background: bool,
    pub started_at: u64,
    pub ended_at: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestView {
    pub id: RequestId,
    pub turn: Option<TurnId>,
    pub item: Option<ItemId>,
    pub ask: Ask,
    pub blocking: bool,
    pub opened_at: u64,
    pub close: Option<RequestClose>,
    pub closed_at: Option<u64>,
}

impl RequestView {
    pub fn is_open(&self) -> bool {
        self.close.is_none()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Skip {
    TurnStillOpen,
    NotRunning,
    UnknownTurn,
    UnknownItem,
    DuplicateItem,
    ItemClosed,
    ParentClosed,
    ChildrenOpen,
    StillRunning,
    UnknownRequest,
    DuplicateRequest,
    RequestClosed,
    RequestsOpen,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChatView {
    pub session: SessionView,
    pub turns: Vec<TurnView>,
    pub items: HashMap<ItemId, ItemView>,
    pub background: Vec<ItemId>,
    pub requests: Vec<RequestView>,
    pub queue: Vec<QueuedPrompt>,
    pub usage: Option<Usage>,
    pub skipped: u32,
    pub last_session: Option<String>,
}

impl ChatView {
    pub fn apply(&mut self, event: &ChatEvent) -> Result<(), Skip> {
        let result = self.try_apply(event);
        if result.is_err() {
            self.skipped = self.skipped.saturating_add(1);
        }
        result
    }

    pub fn running(&self) -> Option<&TurnView> {
        self.turns.last().filter(|turn| turn.is_open())
    }

    pub fn open_requests(&self) -> impl Iterator<Item = &RequestView> {
        self.requests.iter().filter(|request| request.is_open())
    }

    pub fn waiting(&self) -> bool {
        self.open_requests().any(|request| request.blocking)
    }

    pub fn phase(&self) -> ChatPhase {
        match &self.session {
            SessionView::Starting => return ChatPhase::Starting,
            SessionView::Ended(_) => return ChatPhase::Ended,
            SessionView::Ready { .. } => {}
        }
        if self.waiting() {
            return ChatPhase::Waiting;
        }
        match self.running() {
            Some(turn) if turn.stopping => ChatPhase::Stopping,
            Some(_) => ChatPhase::Running,
            None => ChatPhase::Idle,
        }
    }

    pub fn item(&self, id: &ItemId) -> Option<&ItemView> {
        self.items.get(id)
    }

    pub fn request(&self, id: RequestId) -> Option<&RequestView> {
        self.requests.iter().find(|request| request.id == id)
    }

    pub fn open_items(&self) -> impl Iterator<Item = &ItemView> {
        self.items.values().filter(|item| item.status.is_open())
    }

    fn try_apply(&mut self, event: &ChatEvent) -> Result<(), Skip> {
        match event {
            ChatEvent::SessionReady { session } => {
                if self.running().is_some() {
                    return Err(Skip::TurnStillOpen);
                }
                if !session.is_empty() {
                    if self
                        .last_session
                        .as_ref()
                        .is_some_and(|known| known != session)
                    {
                        self.turns.clear();
                        self.items.clear();
                        self.background.clear();
                        self.requests.clear();
                    }
                    self.last_session = Some(session.clone());
                }
                self.session = SessionView::Ready {
                    session: session.clone(),
                };
            }
            ChatEvent::SessionEnded { end } => {
                if self.running().is_some() {
                    return Err(Skip::TurnStillOpen);
                }
                if self.open_items().next().is_some() {
                    return Err(Skip::StillRunning);
                }
                if self.open_requests().next().is_some() {
                    return Err(Skip::RequestsOpen);
                }
                self.session = SessionView::Ended(end.clone());
            }
            ChatEvent::TurnStarted { turn, by, at } => {
                if self.running().is_some() {
                    return Err(Skip::TurnStillOpen);
                }
                if self.turns.last().is_some_and(|last| last.id >= *turn) {
                    return Err(Skip::UnknownTurn);
                }
                self.turns.push(TurnView {
                    id: *turn,
                    by: *by,
                    started_at: *at,
                    ended_at: None,
                    outcome: None,
                    stopping: false,
                    items: Vec::new(),
                });
            }
            ChatEvent::TurnClaimed { turn, prompt } => {
                self.running_mut(*turn)?.by = TurnBy::User { prompt: *prompt };
            }
            ChatEvent::TurnStopping { turn } => {
                let running = self.running_mut(*turn)?;
                running.stopping = true;
            }
            ChatEvent::TurnEnded { turn, outcome, at } => {
                let id = *turn;
                if self
                    .items
                    .values()
                    .any(|item| item.turn == Some(id) && !item.background && item.status.is_open())
                {
                    return Err(Skip::StillRunning);
                }
                if self
                    .requests
                    .iter()
                    .any(|request| request.turn == Some(id) && request.is_open())
                {
                    return Err(Skip::RequestsOpen);
                }
                let running = self.running_mut(id)?;
                running.outcome = Some(outcome.clone());
                running.ended_at = Some(*at);
            }
            ChatEvent::ItemStarted {
                item,
                turn,
                parent,
                kind,
                body,
                at,
            } => {
                if self.items.contains_key(item) {
                    return Err(Skip::DuplicateItem);
                }
                if let Some(turn) = turn
                    && self.running().is_none_or(|running| running.id != *turn)
                {
                    return Err(Skip::NotRunning);
                }
                let mut background = false;
                if let Some(parent) = parent {
                    let parent = self.items.get(parent).ok_or(Skip::UnknownItem)?;
                    if !parent.status.is_open() {
                        return Err(Skip::ParentClosed);
                    }
                    background = parent.background;
                }
                match (parent, turn) {
                    (Some(parent), _) => {
                        if let Some(parent) = self.items.get_mut(parent) {
                            parent.children.push(item.clone());
                        }
                    }
                    (None, Some(turn)) => self.running_mut(*turn)?.items.push(item.clone()),
                    (None, None) => {
                        background = true;
                        self.background.push(item.clone());
                    }
                }
                self.items.insert(
                    item.clone(),
                    ItemView {
                        id: item.clone(),
                        kind: *kind,
                        body: body.clone(),
                        status: ItemStatus::Running,
                        turn: *turn,
                        parent: parent.clone(),
                        children: Vec::new(),
                        background,
                        started_at: *at,
                        ended_at: None,
                    },
                );
                for request in &mut self.requests {
                    if request.is_open() && request.item.as_ref() == Some(item) {
                        request.turn = *turn;
                    }
                }
            }
            ChatEvent::ItemAppended { item, text } => {
                self.open_item_mut(item)?.body.text.push_str(text);
            }
            ChatEvent::ItemChanged { item, body } => {
                self.open_item_mut(item)?.body = body.clone();
            }
            ChatEvent::ItemBackgrounded { item } => {
                self.open_item_mut(item)?;
                let mut stack = vec![item.clone()];
                while let Some(id) = stack.pop() {
                    if let Some(view) = self.items.get_mut(&id) {
                        view.background = true;
                        stack.extend(view.children.iter().cloned());
                    }
                }
                for request in &mut self.requests {
                    if request.is_open()
                        && request
                            .item
                            .as_ref()
                            .and_then(|id| self.items.get(id))
                            .is_some_and(|view| view.background)
                    {
                        request.turn = None;
                    }
                }
            }
            ChatEvent::ItemEnded { item, status, at } => {
                if status.is_open() {
                    return Err(Skip::StillRunning);
                }
                let view = self.items.get(item).ok_or(Skip::UnknownItem)?;
                if !view.status.is_open() {
                    return Err(Skip::ItemClosed);
                }
                let outlives = |child: &ItemView| child.background && !view.background;
                if view.children.iter().any(|child| {
                    self.items
                        .get(child)
                        .is_some_and(|child| child.status.is_open() && !outlives(child))
                }) {
                    return Err(Skip::ChildrenOpen);
                }
                let view = self.open_item_mut(item)?;
                view.status = status.clone();
                view.ended_at = Some(*at);
            }
            ChatEvent::RequestOpened {
                request,
                turn,
                item,
                ask,
                blocking,
                at,
            } => {
                if self.request(*request).is_some() {
                    return Err(Skip::DuplicateRequest);
                }
                if let Some(turn) = turn
                    && self.running().is_none_or(|running| running.id != *turn)
                {
                    return Err(Skip::NotRunning);
                }
                self.requests.push(RequestView {
                    id: *request,
                    turn: *turn,
                    item: item.clone(),
                    ask: ask.clone(),
                    blocking: *blocking,
                    opened_at: *at,
                    close: None,
                    closed_at: None,
                });
            }
            ChatEvent::RequestClosed { request, close, at } => {
                let view = self
                    .requests
                    .iter_mut()
                    .find(|view| view.id == *request)
                    .ok_or(Skip::UnknownRequest)?;
                if !view.is_open() {
                    return Err(Skip::RequestClosed);
                }
                view.close = Some(close.clone());
                view.closed_at = Some(*at);
            }
            ChatEvent::QueueChanged { prompts } => self.queue.clone_from(prompts),
            ChatEvent::UsageChanged { usage } => self.usage = Some(*usage),
            ChatEvent::Restart => *self = Self::default(),
        }
        Ok(())
    }

    fn running_mut(&mut self, id: TurnId) -> Result<&mut TurnView, Skip> {
        self.turns
            .last_mut()
            .filter(|turn| turn.is_open() && turn.id == id)
            .ok_or(Skip::NotRunning)
    }

    fn open_item_mut(&mut self, id: &ItemId) -> Result<&mut ItemView, Skip> {
        let view = self.items.get_mut(id).ok_or(Skip::UnknownItem)?;
        if view.status.is_open() {
            Ok(view)
        } else {
            Err(Skip::ItemClosed)
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ChatLog {
    events: Vec<ChatEvent>,
    started: HashMap<ItemId, usize>,
    queue: Option<ChatEvent>,
    usage: Option<ChatEvent>,
    session: Option<String>,
}

impl ChatLog {
    pub fn push(&mut self, event: &ChatEvent) {
        match event {
            ChatEvent::Restart => *self = Self::default(),
            ChatEvent::SessionReady { session } => {
                if !session.is_empty() {
                    if self.session.as_ref().is_some_and(|known| known != session) {
                        *self = Self::default();
                    }
                    self.session = Some(session.clone());
                }
                self.events.push(event.clone());
            }
            ChatEvent::ItemStarted { item, .. } => {
                if !self.started.contains_key(item) {
                    self.started.insert(item.clone(), self.events.len());
                }
                self.events.push(event.clone());
            }
            ChatEvent::ItemAppended { item, text } => {
                if let Some(ChatEvent::ItemStarted { body, .. }) = self
                    .started
                    .get(item)
                    .and_then(|index| self.events.get_mut(*index))
                {
                    body.text.push_str(text);
                }
            }
            ChatEvent::ItemChanged { item, body } => {
                if let Some(ChatEvent::ItemStarted { body: started, .. }) = self
                    .started
                    .get(item)
                    .and_then(|index| self.events.get_mut(*index))
                {
                    started.clone_from(body);
                }
            }
            ChatEvent::QueueChanged { .. } => self.queue = Some(event.clone()),
            ChatEvent::UsageChanged { .. } => self.usage = Some(event.clone()),
            _ => self.events.push(event.clone()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.queue.is_none() && self.usage.is_none()
    }

    pub fn events(&self) -> impl Iterator<Item = &ChatEvent> {
        self.events
            .iter()
            .chain(self.queue.iter())
            .chain(self.usage.iter())
    }
}
