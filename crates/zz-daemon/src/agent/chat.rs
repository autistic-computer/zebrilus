use std::collections::{HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;

use zz_machine::{Decision, Machine};
use zz_protocol::chat::{
    Answer, Ask, ChatEvent, ItemBody, ItemId, ItemKind, ItemStatus, QueuedPrompt, RequestClose,
    RequestId, SessionEnd, TurnBy, TurnId, TurnOutcome, Usage,
};

use super::turn::{self, Outcome, Phase, TurnMachine, TurnState};

const REMEMBERED_ITEMS: usize = 8192;
const REMEMBERED_REQUESTS: usize = 1024;
const ORPHAN_LIMIT: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Fact {
    SessionReady {
        session: String,
    },
    SessionClosed,
    Exited {
        message: String,
    },
    Turn(turn::Fact),
    ItemStarted {
        item: ItemId,
        parent: Option<ItemId>,
        kind: ItemKind,
        body: ItemBody,
        background: bool,
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
    },
    RequestOpened {
        request: RequestId,
        item: Option<ItemId>,
        ask: Ask,
        blocking: bool,
    },
    RequestAnswered {
        request: RequestId,
        answer: Answer,
    },
    RequestWithdrawn {
        request: RequestId,
    },
    Submit {
        prompt: QueuedPrompt,
    },
    SendFailed {
        prompt: u64,
    },
    Unqueue {
        prompt: u64,
    },
    Usage {
        usage: Usage,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    Emit(ChatEvent),
    Interrupt,
    Settled {
        owner: Option<u64>,
        outcome: Outcome,
    },
    Send {
        prompt: QueuedPrompt,
    },
    Reclaim {
        prompts: Vec<QueuedPrompt>,
    },
    Respond {
        request: RequestId,
        answer: Answer,
    },
    Withdraw {
        requests: Vec<RequestId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reject {
    Turn(turn::Reject),
    Misrouted,
    Offline,
    InvalidStatus,
    StaleItem(ItemId),
    UnknownItem(ItemId),
    StaleRequest(RequestId),
    UnknownRequest(RequestId),
    UnknownPrompt(u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Open {
    id: ItemId,
    turn: Option<TurnId>,
    parent: Option<ItemId>,
    kind: ItemKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OpenRequest {
    id: RequestId,
    turn: Option<TurnId>,
    item: Option<ItemId>,
    blocking: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChatState {
    pub(crate) turn: TurnState,
    current: Option<TurnId>,
    last_turn: u64,
    items: Vec<Open>,
    closed_items: VecDeque<u64>,
    orphans: VecDeque<Fact>,
    requests: Vec<OpenRequest>,
    closed_requests: VecDeque<RequestId>,
    queue: Vec<QueuedPrompt>,
    sending: Option<QueuedPrompt>,
    session: Option<String>,
}

pub(crate) struct Ctx {
    pub(crate) now: Instant,
    pub(crate) at: u64,
}

pub(crate) struct ChatMachine;

type ChatDecision = Decision<ChatState, Effect, Reject>;

impl Machine for ChatMachine {
    type State = ChatState;
    type Signal = Fact;
    type Ctx = Ctx;
    type Effect = Effect;
    type Time = Instant;
    type Reject = Reject;

    fn transition(state: &ChatState, fact: &Fact, ctx: &Ctx) -> ChatDecision {
        if let Some(decision) = quiet(state, fact) {
            return decision;
        }
        let mut draft = Draft {
            state: state.clone(),
            effects: Vec::new(),
            at: ctx.at,
            now: ctx.now,
        };
        match draft.apply(fact) {
            Ok(()) => Decision::to(draft.state).with_effects(draft.effects),
            Err(reject) => Decision::reject(reject),
        }
    }

    fn wake_at(state: &ChatState) -> Option<Instant> {
        TurnMachine::wake_at(&state.turn)
    }
}

fn quiet(state: &ChatState, fact: &Fact) -> Option<ChatDecision> {
    let (item, emit) = match fact {
        Fact::ItemAppended { item, text } => (
            item,
            ChatEvent::ItemAppended {
                item: item.clone(),
                text: text.clone(),
            },
        ),
        Fact::ItemChanged { item, body } => (
            item,
            ChatEvent::ItemChanged {
                item: item.clone(),
                body: body.clone(),
            },
        ),
        Fact::Usage { usage } => {
            return Some(
                Decision::stay()
                    .with_effect(Effect::Emit(ChatEvent::UsageChanged { usage: *usage })),
            );
        }
        _ => return None,
    };
    if state.open(item).is_some() {
        return Some(Decision::stay().with_effect(Effect::Emit(emit)));
    }
    if state.pending(item) {
        return None;
    }
    if state.remembers(item) {
        return Some(Decision::reject(Reject::StaleItem(item.clone())));
    }
    Some(Decision::reject(Reject::UnknownItem(item.clone())))
}

struct Draft {
    state: ChatState,
    effects: Vec<Effect>,
    at: u64,
    now: Instant,
}

impl Draft {
    fn apply(&mut self, fact: &Fact) -> Result<(), Reject> {
        match fact {
            Fact::SessionReady { session } => {
                if matches!(self.state.turn.phase, Phase::Idle) || self.state.turn.turn().is_some()
                {
                    return Ok(());
                }
                if !session.is_empty() {
                    if self
                        .state
                        .session
                        .as_ref()
                        .is_some_and(|known| known != session)
                    {
                        self.state.closed_items.clear();
                        self.state.closed_requests.clear();
                    }
                    self.state.session = Some(session.clone());
                }
                self.emit(ChatEvent::SessionReady {
                    session: session.clone(),
                });
                self.turn(&turn::Fact::SessionReady)
            }
            Fact::SessionClosed => self.end_session(&turn::Fact::SessionClosed, SessionEnd::Closed),
            Fact::Exited { message } => self.end_session(
                &turn::Fact::Exited {
                    message: message.clone(),
                },
                SessionEnd::Exited {
                    message: message.clone(),
                },
            ),
            Fact::Turn(fact) => match fact {
                turn::Fact::SessionReady
                | turn::Fact::SessionClosed
                | turn::Fact::Exited { .. }
                | turn::Fact::PermissionOpened { .. }
                | turn::Fact::PermissionClosed { .. } => Err(Reject::Misrouted),
                turn::Fact::Prompt { turn, .. } => {
                    self.turn(fact)?;
                    if self
                        .state
                        .sending
                        .as_ref()
                        .is_some_and(|prompt| prompt.id == *turn)
                    {
                        self.state.sending = None;
                    }
                    Ok(())
                }
                _ => self.turn(fact),
            },
            Fact::ItemStarted {
                item,
                parent,
                kind,
                body,
                background,
            } => self.start_item(fact, item, parent.as_ref(), *kind, body, *background),
            Fact::ItemAppended { item, .. } | Fact::ItemChanged { item, .. } => {
                self.orphan(item, fact)
            }
            Fact::ItemBackgrounded { item } => self.background_item(fact, item),
            Fact::ItemEnded { item, status } => self.end_item(fact, item, status),
            Fact::RequestOpened {
                request,
                item,
                ask,
                blocking,
            } => self.open_request(*request, item.as_ref(), ask, *blocking),
            Fact::RequestAnswered { request, answer } => {
                self.close_request(
                    *request,
                    RequestClose::Answered {
                        answer: answer.clone(),
                    },
                )?;
                self.effects.push(Effect::Respond {
                    request: *request,
                    answer: answer.clone(),
                });
                Ok(())
            }
            Fact::RequestWithdrawn { request } => {
                if self.state.closed_requests.contains(request) {
                    return Ok(());
                }
                self.close_request(*request, RequestClose::Withdrawn)
            }
            Fact::Submit { prompt } => {
                if self.state.idle() && self.state.sending.is_none() && self.state.queue.is_empty()
                {
                    self.state.sending = Some(prompt.clone());
                    self.effects.push(Effect::Send {
                        prompt: prompt.clone(),
                    });
                } else {
                    self.state.queue.push(prompt.clone());
                    self.queue_changed();
                }
                Ok(())
            }
            Fact::SendFailed { prompt } => {
                let Some(sending) = self.state.sending.take_if(|sending| sending.id == *prompt)
                else {
                    return Err(Reject::UnknownPrompt(*prompt));
                };
                self.effects.push(Effect::Reclaim {
                    prompts: vec![sending],
                });
                if self.state.idle() {
                    self.drain();
                }
                Ok(())
            }
            Fact::Unqueue { prompt } => {
                let before = self.state.queue.len();
                self.state.queue.retain(|queued| queued.id != *prompt);
                if self.state.queue.len() == before {
                    return Err(Reject::UnknownPrompt(*prompt));
                }
                self.queue_changed();
                Ok(())
            }
            Fact::Usage { .. } => Ok(()),
        }
    }

    fn turn(&mut self, fact: &turn::Fact) -> Result<(), Reject> {
        let decision =
            TurnMachine::transition(&self.state.turn, fact, &turn::Ctx { now: self.now });
        if let Some(reject) = decision.rejected {
            return Err(Reject::Turn(reject));
        }
        let was_running = matches!(self.state.turn.phase, Phase::Running(_));
        let unowned = self
            .state
            .turn
            .turn()
            .is_some_and(|turn| turn.owner.is_none());
        if let Some(next) = decision.next {
            self.state.turn = next;
        }
        for effect in decision.effects {
            match effect {
                turn::Effect::Settle { turn, outcome } => self.end_turn(turn, outcome),
                turn::Effect::Interrupt => self.effects.push(Effect::Interrupt),
                turn::Effect::Dismiss { .. } => {}
                turn::Effect::Drain => self.drain(),
                turn::Effect::Reclaim => self.reclaim(),
            }
        }
        if let Some(owner) = self.state.turn.turn().map(|turn| turn.owner)
            && self.state.current.is_none()
        {
            self.state.last_turn += 1;
            let id = TurnId(self.state.last_turn);
            self.state.current = Some(id);
            self.emit(ChatEvent::TurnStarted {
                turn: id,
                by: owner.map_or(TurnBy::Agent, |prompt| TurnBy::User { prompt }),
                at: self.at,
            });
        }
        if unowned
            && let Some(current) = self.state.current
            && let Some(prompt) = self.state.turn.turn().and_then(|turn| turn.owner)
        {
            self.emit(ChatEvent::TurnClaimed {
                turn: current,
                prompt,
            });
        }
        if was_running
            && matches!(self.state.turn.phase, Phase::Cancelling { .. })
            && let Some(current) = self.state.current
        {
            self.emit(ChatEvent::TurnStopping { turn: current });
            self.dismiss_requests(|request| request.turn == Some(current));
        }
        Ok(())
    }

    fn end_turn(&mut self, owner: Option<u64>, outcome: Outcome) {
        if let Some(current) = self.state.current.take() {
            self.dismiss_requests(|request| request.turn == Some(current));
            self.close_items(|item| item.turn == Some(current));
            self.emit(ChatEvent::TurnEnded {
                turn: current,
                outcome: turn_outcome(&outcome),
                at: self.at,
            });
        }
        self.effects.push(Effect::Settled { owner, outcome });
    }

    fn end_session(&mut self, fact: &turn::Fact, end: SessionEnd) -> Result<(), Reject> {
        let exited = matches!(end, SessionEnd::Exited { .. });
        match self.state.turn.phase {
            Phase::Exited => return Ok(()),
            Phase::Offline if !exited => return Ok(()),
            _ => {}
        }
        let sending = self.state.sending.take();
        if let Some(prompt) = sending.clone() {
            self.state.queue.insert(0, prompt);
        }
        self.turn(fact)?;
        if sending.is_some() && !exited {
            self.queue_changed();
        }
        self.dismiss_requests(|_| true);
        self.close_items(|_| true);
        self.state.orphans.clear();
        self.emit(ChatEvent::SessionEnded { end });
        Ok(())
    }

    fn start_item(
        &mut self,
        fact: &Fact,
        item: &ItemId,
        parent: Option<&ItemId>,
        kind: ItemKind,
        body: &ItemBody,
        background: bool,
    ) -> Result<(), Reject> {
        if !self.state.live() {
            return Err(Reject::Offline);
        }
        if self.state.open(item).is_some() {
            self.emit(ChatEvent::ItemChanged {
                item: item.clone(),
                body: body.clone(),
            });
            return Ok(());
        }
        if self.state.remembers(item) {
            return Err(Reject::StaleItem(item.clone()));
        }
        if self.state.pending(item) {
            self.buffer(fact.clone());
            return Ok(());
        }
        let turn = match parent {
            Some(parent) => match self.state.open(parent) {
                Some(open) => open.turn,
                None if self.state.remembers(parent) => {
                    return Err(Reject::StaleItem(parent.clone()));
                }
                None => {
                    self.buffer(fact.clone());
                    return Ok(());
                }
            },
            None if background => None,
            None => self.state.current,
        };
        self.state.items.push(Open {
            id: item.clone(),
            turn,
            parent: parent.cloned(),
            kind,
        });
        self.emit(ChatEvent::ItemStarted {
            item: item.clone(),
            turn,
            parent: parent.cloned(),
            kind,
            body: body.clone(),
            at: self.at,
        });
        for request in &mut self.state.requests {
            if request.item.as_ref() == Some(item) {
                request.turn = turn;
            }
        }
        self.adopt(item)
    }

    fn adopt(&mut self, parent: &ItemId) -> Result<(), Reject> {
        let mut waiting = Vec::new();
        let mut kept = VecDeque::new();
        for fact in self.state.orphans.drain(..) {
            if fact_waits_on(&fact, parent) {
                waiting.push(fact);
            } else {
                kept.push_back(fact);
            }
        }
        self.state.orphans = kept;
        for fact in waiting {
            match self.apply(&fact) {
                Ok(()) | Err(Reject::StaleItem(_) | Reject::UnknownItem(_)) => {}
                Err(reject) => return Err(reject),
            }
        }
        Ok(())
    }

    fn orphan(&mut self, item: &ItemId, fact: &Fact) -> Result<(), Reject> {
        if self.state.pending(item) {
            self.buffer(fact.clone());
            return Ok(());
        }
        match fact {
            Fact::ItemAppended { text, .. } if self.state.open(item).is_some() => {
                self.emit(ChatEvent::ItemAppended {
                    item: item.clone(),
                    text: text.clone(),
                });
                Ok(())
            }
            Fact::ItemChanged { body, .. } if self.state.open(item).is_some() => {
                self.emit(ChatEvent::ItemChanged {
                    item: item.clone(),
                    body: body.clone(),
                });
                Ok(())
            }
            _ => Err(self.missing(item)),
        }
    }

    fn background_item(&mut self, fact: &Fact, item: &ItemId) -> Result<(), Reject> {
        if self.state.pending(item) {
            self.buffer(fact.clone());
            return Ok(());
        }
        if self.state.open(item).is_none() {
            return Err(self.missing(item));
        }
        let moved = self.state.subtree(item);
        if moved
            .iter()
            .all(|id| self.state.open(id).is_some_and(|open| open.turn.is_none()))
        {
            return Ok(());
        }
        for open in &mut self.state.items {
            if moved.contains(&open.id) {
                open.turn = None;
            }
        }
        for request in &mut self.state.requests {
            if request.item.as_ref().is_some_and(|id| moved.contains(id)) {
                request.turn = None;
            }
        }
        self.emit(ChatEvent::ItemBackgrounded { item: item.clone() });
        Ok(())
    }

    fn end_item(&mut self, fact: &Fact, item: &ItemId, status: &ItemStatus) -> Result<(), Reject> {
        if status.is_open() {
            return Err(Reject::InvalidStatus);
        }
        if self.state.pending(item) {
            self.buffer(fact.clone());
            return Ok(());
        }
        if self.state.open(item).is_none() {
            return Err(self.missing(item));
        }
        self.close_subtree(item, Some(status.clone()));
        Ok(())
    }

    fn open_request(
        &mut self,
        request: RequestId,
        item: Option<&ItemId>,
        ask: &Ask,
        blocking: bool,
    ) -> Result<(), Reject> {
        if !self.state.live() {
            return Err(Reject::Offline);
        }
        if self.state.requests.iter().any(|open| open.id == request) {
            return Ok(());
        }
        if self.state.closed_requests.contains(&request) {
            return Err(Reject::StaleRequest(request));
        }
        let turn = match item {
            Some(item) if self.state.open(item).is_some() => {
                self.state.open(item).and_then(|open| open.turn)
            }
            Some(item) if !self.state.remembers(item) => None,
            _ => self.state.current,
        };
        let item = item.cloned();
        self.state.requests.push(OpenRequest {
            id: request,
            turn,
            item: item.clone(),
            blocking,
        });
        self.emit(ChatEvent::RequestOpened {
            request,
            turn,
            item,
            ask: ask.clone(),
            blocking,
            at: self.at,
        });
        Ok(())
    }

    fn close_request(&mut self, request: RequestId, close: RequestClose) -> Result<(), Reject> {
        if !self.state.requests.iter().any(|open| open.id == request) {
            if self.state.closed_requests.contains(&request) {
                return Err(Reject::StaleRequest(request));
            }
            return Err(Reject::UnknownRequest(request));
        }
        self.finish_request(request, close);
        Ok(())
    }

    fn finish_request(&mut self, request: RequestId, close: RequestClose) {
        self.state.requests.retain(|open| open.id != request);
        remember(
            &mut self.state.closed_requests,
            request,
            REMEMBERED_REQUESTS,
        );
        self.emit(ChatEvent::RequestClosed {
            request,
            close,
            at: self.at,
        });
    }

    fn dismiss_requests(&mut self, which: impl Fn(&OpenRequest) -> bool) {
        let dismissed = self
            .state
            .requests
            .iter()
            .filter(|request| which(request))
            .map(|request| request.id)
            .collect::<Vec<_>>();
        for request in &dismissed {
            self.finish_request(*request, RequestClose::Dismissed);
        }
        if !dismissed.is_empty() {
            self.effects.push(Effect::Withdraw {
                requests: dismissed,
            });
        }
    }

    fn close_items(&mut self, which: impl Fn(&Open) -> bool) {
        let roots = self
            .state
            .items
            .iter()
            .rev()
            .filter(|open| which(open))
            .map(|open| open.id.clone())
            .collect::<Vec<_>>();
        for root in roots {
            if self.state.open(&root).is_some() {
                self.close_subtree(&root, None);
            }
        }
    }

    fn close_subtree(&mut self, root: &ItemId, status: Option<ItemStatus>) {
        let subtree = self.state.scope_subtree(root);
        let withdrawn = self
            .state
            .requests
            .iter()
            .filter(|request| request.item.as_ref().is_some_and(|id| subtree.contains(id)))
            .map(|request| request.id)
            .collect::<Vec<_>>();
        for request in &withdrawn {
            self.finish_request(*request, RequestClose::Withdrawn);
        }
        if !withdrawn.is_empty() {
            self.effects.push(Effect::Withdraw {
                requests: withdrawn,
            });
        }
        let closing = self
            .state
            .items
            .iter()
            .rev()
            .filter(|open| open.id != *root && subtree.contains(&open.id))
            .map(|open| (open.id.clone(), text_or_interrupted(open.kind)))
            .collect::<Vec<_>>();
        for (id, status) in closing {
            self.close_item(&id, status);
        }
        let status = status.unwrap_or_else(|| {
            self.state
                .open(root)
                .map_or(ItemStatus::Interrupted, |open| {
                    text_or_interrupted(open.kind)
                })
        });
        self.close_item(root, status);
    }

    fn close_item(&mut self, item: &ItemId, status: ItemStatus) {
        let Some(index) = self.state.items.iter().position(|open| open.id == *item) else {
            return;
        };
        self.state.items.remove(index);
        remember(&mut self.state.closed_items, key(item), REMEMBERED_ITEMS);
        self.emit(ChatEvent::ItemEnded {
            item: item.clone(),
            status,
            at: self.at,
        });
    }

    fn drain(&mut self) {
        if self.state.sending.is_some() || self.state.queue.is_empty() {
            return;
        }
        let prompt = self.state.queue.remove(0);
        self.state.sending = Some(prompt.clone());
        self.queue_changed();
        self.effects.push(Effect::Send { prompt });
    }

    fn reclaim(&mut self) {
        if self.state.queue.is_empty() {
            return;
        }
        let prompts = std::mem::take(&mut self.state.queue);
        self.queue_changed();
        self.effects.push(Effect::Reclaim { prompts });
    }

    fn queue_changed(&mut self) {
        self.emit(ChatEvent::QueueChanged {
            prompts: self.state.queue.clone(),
        });
    }

    fn buffer(&mut self, fact: Fact) {
        if self.state.orphans.len() >= ORPHAN_LIMIT {
            self.state.orphans.pop_front();
        }
        self.state.orphans.push_back(fact);
    }

    fn missing(&self, item: &ItemId) -> Reject {
        if self.state.remembers(item) {
            Reject::StaleItem(item.clone())
        } else {
            Reject::UnknownItem(item.clone())
        }
    }

    fn emit(&mut self, event: ChatEvent) {
        self.effects.push(Effect::Emit(event));
    }
}

impl ChatState {
    pub(crate) fn current(&self) -> Option<TurnId> {
        self.current
    }

    pub(crate) fn open_items(&self) -> impl Iterator<Item = &ItemId> {
        self.items.iter().map(|open| &open.id)
    }

    pub(crate) fn open_requests(&self) -> impl Iterator<Item = RequestId> + '_ {
        self.requests.iter().map(|request| request.id)
    }

    pub(crate) fn waiting(&self) -> bool {
        self.requests.iter().any(|request| request.blocking)
    }

    pub(crate) fn asked_about(&self, item: &ItemId) -> bool {
        let subtree = self.subtree(item);
        self.requests.iter().any(|request| {
            request
                .item
                .as_ref()
                .is_some_and(|asked| subtree.contains(asked))
        })
    }

    pub(crate) fn waits_for_parent(&self, item: &ItemId) -> bool {
        self.pending(item)
    }

    pub(crate) fn remembers(&self, item: &ItemId) -> bool {
        self.closed_items.contains(&key(item))
    }

    pub(crate) fn queue(&self) -> &[QueuedPrompt] {
        &self.queue
    }

    pub(crate) fn sending(&self) -> Option<&QueuedPrompt> {
        self.sending.as_ref()
    }

    fn idle(&self) -> bool {
        matches!(self.turn.phase, Phase::Idle)
    }

    fn open(&self, item: &ItemId) -> Option<&Open> {
        self.items.iter().find(|open| open.id == *item)
    }

    pub(crate) fn within(&self, item: &ItemId, roots: &HashSet<ItemId>) -> bool {
        let mut at = self.open(item);
        while let Some(open) = at {
            if roots.contains(&open.id) {
                return true;
            }
            at = open.parent.as_ref().and_then(|parent| self.open(parent));
        }
        false
    }

    fn pending(&self, item: &ItemId) -> bool {
        self.orphans.iter().any(|fact| match fact {
            Fact::ItemStarted { item: started, .. } => started == item,
            _ => false,
        })
    }

    fn scope_subtree(&self, root: &ItemId) -> Vec<ItemId> {
        let scope = self.open(root).map(|open| open.turn);
        let mut found = vec![root.clone()];
        let mut index = 0;
        while index < found.len() {
            let parent = found[index].clone();
            found.extend(
                self.items
                    .iter()
                    .filter(|open| open.parent.as_ref() == Some(&parent))
                    .filter(|open| scope.is_none_or(|scope| scope.is_none() || open.turn == scope))
                    .map(|open| open.id.clone()),
            );
            index += 1;
        }
        found
    }

    pub(crate) fn live(&self) -> bool {
        !matches!(self.turn.phase, Phase::Offline | Phase::Exited)
    }

    pub(crate) fn open_kinds(&self) -> impl Iterator<Item = (&ItemId, ItemKind, Option<TurnId>)> {
        self.items
            .iter()
            .map(|open| (&open.id, open.kind, open.turn))
    }

    fn subtree(&self, root: &ItemId) -> Vec<ItemId> {
        let mut found = vec![root.clone()];
        let mut index = 0;
        while index < found.len() {
            let parent = found[index].clone();
            found.extend(
                self.items
                    .iter()
                    .filter(|open| open.parent.as_ref() == Some(&parent))
                    .map(|open| open.id.clone()),
            );
            index += 1;
        }
        found
    }
}

fn fact_waits_on(fact: &Fact, parent: &ItemId) -> bool {
    match fact {
        Fact::ItemStarted {
            item,
            parent: Some(waiting),
            ..
        } => waiting == parent || item == parent,
        Fact::ItemAppended { item, .. }
        | Fact::ItemChanged { item, .. }
        | Fact::ItemBackgrounded { item }
        | Fact::ItemEnded { item, .. } => item == parent,
        _ => false,
    }
}

fn key(item: &ItemId) -> u64 {
    let mut hasher = DefaultHasher::new();
    item.as_str().hash(&mut hasher);
    hasher.finish()
}

fn text_or_interrupted(kind: ItemKind) -> ItemStatus {
    if kind.is_text() {
        ItemStatus::Completed
    } else {
        ItemStatus::Interrupted
    }
}

fn remember<T: PartialEq>(ring: &mut VecDeque<T>, value: T, limit: usize) {
    if ring.contains(&value) {
        return;
    }
    if ring.len() >= limit {
        ring.pop_front();
    }
    ring.push_back(value);
}

fn turn_outcome(outcome: &Outcome) -> TurnOutcome {
    match outcome {
        Outcome::Completed { stop_reason } => TurnOutcome::Completed {
            stop_reason: stop_reason.clone(),
        },
        Outcome::Cancelled => TurnOutcome::Cancelled,
        Outcome::Failed { message } => TurnOutcome::Failed {
            message: message.clone(),
        },
        Outcome::Dropped => TurnOutcome::Dropped,
    }
}
