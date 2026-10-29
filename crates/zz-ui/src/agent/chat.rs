use std::rc::Rc;

use zz_client::chat_rows::{self, AgentState, Fold, RunLabel, Unit};
use zz_gpui::{
    AnyElement, App, Div, IntoElement, ScrollHandle, ScrollWheelEvent, SharedString, Stateful,
    Window, div, prelude::*, px, relative,
};
use zz_protocol::chat::{
    Answer, Ask, ChatView, ItemId, ItemKind, ItemStatus, OptionKind, RequestClose, RequestView,
    SessionView, StepStatus, TurnId, TurnView,
};

use super::presentation::{
    card_chip, card_glyph, card_header, card_surface, card_tiles, permission_card,
    permission_option,
};
use super::{ActivityMark, RowEnd, activity_row_marked};
use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Edge, Icon, IconName, StyledExt as _, Tone,
    button::Button, h_flex, text::TextView, v_flex,
};

#[derive(Clone, Debug, Default)]
pub struct ChatOpen {
    pub turns: Vec<TurnId>,
    pub runs: Vec<ItemId>,
    pub agents: Vec<ItemId>,
    pub highlighted: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChatClick {
    Turn(TurnId),
    Run(ItemId),
    Agent(ItemId),
}

pub type ChatClicks = Rc<dyn Fn(&ChatClick, &mut Window, &mut App)>;

#[derive(Default)]
pub struct ChatHooks<'a> {
    pub click: Option<ChatClicks>,
    pub ask: Option<&'a dyn Fn(&RequestView, &App) -> Option<AnyElement>>,
}

impl ChatHooks<'_> {
    fn clickable<E: StatefulInteractiveElement + Styled>(
        &self,
        element: E,
        target: ChatClick,
    ) -> E {
        match self.click.clone() {
            Some(click) => element
                .cursor_pointer()
                .on_click(move |_, window, cx| click(&target, window, cx)),
            None => element,
        }
    }
}

const FOLLOW_SLACK: f32 = 24.0;

#[derive(Default)]
pub struct ChatPane {
    pub open: ChatOpen,
    pub scroll: ScrollHandle,
    pub away: bool,
    session: Option<String>,
}

impl ChatPane {
    pub fn click(&mut self, click: &ChatClick) {
        match click {
            ChatClick::Turn(turn) => toggle(&mut self.open.turns, *turn),
            ChatClick::Run(item) => toggle(&mut self.open.runs, item.clone()),
            ChatClick::Agent(item) => toggle(&mut self.open.agents, item.clone()),
        }
        self.away = true;
    }

    pub fn sync(&mut self, view: &ChatView) {
        let session = match &view.session {
            SessionView::Ready { session } => Some(session.clone()),
            _ => view.last_session.clone(),
        };
        if session != self.session {
            self.open = ChatOpen::default();
            self.session = session;
        }
        self.open
            .turns
            .retain(|turn| view.turns.iter().any(|held| held.id == *turn));
        self.open.runs.retain(|id| view.item(id).is_some());
        self.open.agents.retain(|id| view.item(id).is_some());
    }

    fn distance(&self) -> f32 {
        let offset = self.scroll.offset().y;
        let max = self.scroll.max_offset().y;
        f32::from(max + offset).max(0.0)
    }

    pub fn wheel(&mut self, event: &ScrollWheelEvent) -> bool {
        if event.delta.pixel_delta(px(16.0)).y > px(0.0) {
            self.away = true;
            return false;
        }
        true
    }

    pub fn settle(&mut self) -> bool {
        let away = self.distance() > FOLLOW_SLACK;
        let changed = self.away != away;
        self.away = away;
        changed
    }

    pub fn follow(&mut self) {
        self.away = false;
        self.scroll.scroll_to_bottom();
    }

    pub fn pin(&self) {
        if !self.away {
            self.scroll.scroll_to_bottom();
        }
    }

    pub fn shows_jump(&self) -> bool {
        self.away && self.distance() > FOLLOW_SLACK
    }
}

fn toggle<T: PartialEq>(open: &mut Vec<T>, value: T) {
    if let Some(index) = open.iter().position(|held| *held == value) {
        open.remove(index);
    } else {
        open.push(value);
    }
}

pub fn shows_chat(view: &ChatView) -> bool {
    !view.turns.is_empty() || !view.background.is_empty()
}

pub fn chat_live(view: &ChatView) -> bool {
    view.running().is_some() || view.open_items().next().is_some()
}

pub fn chat_thread(
    view: &ChatView,
    now: u64,
    open: &ChatOpen,
    phase: f32,
    hooks: &ChatHooks<'_>,
    cx: &App,
) -> Div {
    let draw = Draw {
        view,
        now,
        open,
        phase,
        hooks,
        cx,
    };
    let live = chat_rows::background_live(view);
    let background = chat_rows::background_rows(view);
    let mut pending = background.iter().peekable();
    let mut children = Vec::new();
    for turn in &view.turns {
        while let Some(unit) = pending
            .next_if(|unit| chat_rows::unit_at(view, unit).is_some_and(|at| at < turn.started_at))
        {
            children.push(draw.unit(unit, live));
        }
        children.push(draw.turn(turn));
    }
    children.extend(pending.map(|unit| draw.unit(unit, live)));
    v_flex().w_full().gap_3().children(children)
}

struct Draw<'a, 'h> {
    view: &'a ChatView,
    now: u64,
    open: &'a ChatOpen,
    phase: f32,
    hooks: &'a ChatHooks<'h>,
    cx: &'a App,
}

impl Draw<'_, '_> {
    fn turn(&self, turn: &TurnView) -> AnyElement {
        let rows = chat_rows::turn_rows(self.view, turn, self.now);
        let live = turn.is_open();
        let lead = rows
            .shown
            .iter()
            .take_while(|unit| matches!(unit, Unit::User(_)))
            .count();
        let expanded = self.open.turns.contains(&turn.id);
        let fold = rows.fold.filter(|_| !rows.folded.is_empty());
        v_flex()
            .id(("chat-turn", turn.id.0))
            .w_full()
            .gap_3()
            .children(rows.shown[..lead].iter().map(|unit| self.unit(unit, live)))
            .when_some(fold, |column, fold| {
                column.child(self.hooks.clickable(
                    fold_row(turn.id, fold, expanded, self.cx),
                    ChatClick::Turn(turn.id),
                ))
            })
            .when(expanded, |column| {
                column.children(rows.folded.iter().map(|unit| self.unit(unit, live)))
            })
            .children(rows.shown[lead..].iter().map(|unit| self.unit(unit, live)))
            .into_any_element()
    }

    fn unit(&self, unit: &Unit, live: bool) -> AnyElement {
        let cx = self.cx;
        match unit {
            Unit::User(id) => user_bubble(id, text_of(self.view, id), cx),
            Unit::Text(id) => markdown(id, text_of(self.view, id)),
            Unit::Run(run) => self.run(run, live),
            Unit::Subagent(id) => self.agents(std::slice::from_ref(id)),
            Unit::Agents(ids) => self.agents(ids),
            Unit::Plan(id) => plan_card(self.view, id, cx),
            Unit::Notice(id) => notice_row(self.view, id, cx),
            Unit::Ask(request) => self
                .view
                .request(*request)
                .map_or_else(|| div().into_any_element(), |request| self.ask(request)),
            Unit::Record(request) => self.view.request(*request).map_or_else(
                || div().into_any_element(),
                |request| record_card(request, cx),
            ),
        }
    }

    fn ask(&self, request: &RequestView) -> AnyElement {
        match self.hooks.ask {
            Some(ask) => ask(request, self.cx).unwrap_or_else(|| ask_card(request, None, self.cx)),
            None => ask_card(request, Some(self.open.highlighted), self.cx),
        }
    }

    fn run(&self, run: &chat_rows::Run, live: bool) -> AnyElement {
        let view = self.view;
        let first = run.items.first().cloned().unwrap_or_default();
        let icon = run
            .items
            .iter()
            .filter_map(|id| view.item(id))
            .find(|item| item.kind.is_tool())
            .map_or(IconName::Sparkle, |item| kind_icon(item.kind));
        let mut mark = ActivityMark::plain(icon);
        let label: SharedString = match &run.label {
            RunLabel::Live { .. } | RunLabel::Thinking { .. } => {
                mark.spin = live.then_some(self.phase);
                run_label(&run.label).into()
            }
            RunLabel::Done { .. } => run_label(&run.label).into(),
        };
        let expanded = self.open.runs.contains(&first);
        v_flex()
            .w_full()
            .child(self.hooks.clickable(
                activity_row_marked(
                    SharedString::from(format!("chat-run-{}", first.as_str())),
                    mark,
                    label,
                    RowEnd::Disclosure(expanded),
                    self.cx,
                ),
                ChatClick::Run(first.clone()),
            ))
            .when(expanded, |column| {
                column.child(
                    v_flex().w_full().pl(px(18.0)).children(
                        run.items
                            .iter()
                            .filter_map(|id| view.item(id))
                            .filter(|item| item.kind.is_tool())
                            .map(|item| step_row(item, self.phase, self.cx)),
                    ),
                )
            })
            .into_any_element()
    }

    fn agents(&self, ids: &[ItemId]) -> AnyElement {
        let cx = self.cx;
        let Some(first) = ids.first() else {
            return div().into_any_element();
        };
        let states = ids
            .iter()
            .map(|id| chat_rows::agent_state(self.view, id))
            .collect::<Vec<_>>();
        card_surface(cx)
            .id(SharedString::from(format!(
                "chat-agents-{}",
                first.as_str()
            )))
            .when(ids.len() > 1, |card| {
                card.child(
                    card_header()
                        .child(card_glyph(Icon::new(IconName::Bot), cx))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .child(format!("{} subagents", ids.len())),
                        )
                        .child(card_chip(group_summary(&states))),
                )
            })
            .child(
                v_flex().w_full().px(px(12.0)).py(px(6.0)).gap_1().children(
                    ids.iter()
                        .zip(&states)
                        .map(|(id, state)| self.agent(id, state.as_ref())),
                ),
            )
            .into_any_element()
    }

    fn agent(&self, id: &ItemId, state: Option<&AgentState>) -> AnyElement {
        let cx = self.cx;
        let Some(item) = self.view.item(id) else {
            return div().into_any_element();
        };
        let expanded = self.open.agents.contains(id);
        let ms = chat_rows::agent_ms(self.view, id, self.now)
            .filter(|ms| item.status.is_open() || *ms >= 1000);
        let mut mark = ActivityMark::plain(IconName::Bot);
        mark.spread = true;
        let preview = match state {
            Some(AgentState::Working { label }) => {
                mark.spin = Some(self.phase);
                run_label(label)
            }
            Some(AgentState::Waiting) => {
                mark.tint = Some(cx.theme().warning);
                "Needs you".to_owned()
            }
            Some(AgentState::Done { result }) => {
                mark.tint = Some(cx.theme().success);
                result
                    .as_deref()
                    .and_then(|result| result.lines().find(|line| !line.trim().is_empty()))
                    .unwrap_or_default()
                    .trim()
                    .to_owned()
            }
            Some(AgentState::Failed { message }) => {
                mark.tint = Some(cx.theme().danger);
                message.clone().unwrap_or_else(|| "Failed".to_owned())
            }
            Some(AgentState::Stopped) | None => "Stopped".to_owned(),
        };
        if !preview.is_empty() && !expanded {
            mark.detail = Some(preview.into());
        }
        mark.tag = ms.map(|ms| {
            (
                chat_rows::duration(ms).into(),
                if matches!(state, Some(AgentState::Failed { .. })) {
                    Tone::Danger
                } else {
                    Tone::Neutral
                },
            )
        });
        let title = if item.body.title.is_empty() {
            "Subagent".to_owned()
        } else {
            item.body.title.clone()
        };
        let row = self.hooks.clickable(
            activity_row_marked(
                SharedString::from(format!("chat-agent-{}", id.as_str())),
                mark,
                title.into(),
                RowEnd::Disclosure(expanded),
                cx,
            ),
            ChatClick::Agent(id.clone()),
        );
        v_flex()
            .w_full()
            .child(row)
            .when(expanded, |column| {
                column.child(self.agent_body(id, state, ms))
            })
            .when(!expanded, |column| {
                column.children(
                    asks_under(self.view, id)
                        .into_iter()
                        .map(|request| div().w_full().pt_1().pb_2().child(self.ask(request))),
                )
            })
            .into_any_element()
    }

    fn agent_body(&self, id: &ItemId, state: Option<&AgentState>, ms: Option<u64>) -> Div {
        let cx = self.cx;
        let live = self.view.item(id).is_some_and(|item| item.status.is_open());
        let prompt = chat_rows::agent_prompt(self.view, id).map(str::to_owned);
        let footer = match (state, ms.map(chat_rows::duration)) {
            (Some(AgentState::Working { .. }), Some(took)) => Some(format!("Working for {took}")),
            (Some(AgentState::Working { .. } | AgentState::Waiting), None) => None,
            (Some(AgentState::Waiting), Some(took)) => {
                Some(format!("Waiting for you after {took}"))
            }
            (Some(AgentState::Done { .. }), Some(took)) => Some(format!("Done in {took}")),
            (Some(AgentState::Done { .. }), None) => Some("Done".to_owned()),
            (Some(AgentState::Failed { .. }), Some(took)) => Some(format!("Failed after {took}")),
            (Some(AgentState::Failed { .. }), None) => Some("Failed".to_owned()),
            (Some(AgentState::Stopped) | None, Some(took)) => Some(format!("Stopped after {took}")),
            (Some(AgentState::Stopped) | None, None) => Some("Stopped".to_owned()),
        };
        v_flex()
            .w_full()
            .ml(px(6.0))
            .pl(px(12.0))
            .pt_1()
            .pb_2()
            .gap_2()
            .rule_on(crate::roles::TREE_RULE, Edge::Left, cx)
            .when_some(prompt, |body, prompt| {
                body.child(
                    v_flex()
                        .w_full()
                        .gap_1()
                        .text_size(crate::rems_from_px(12.0))
                        .text_color(cx.theme().foreground.muted())
                        .child(
                            div()
                                .text_color(cx.theme().foreground.faint())
                                .child("Prompt"),
                        )
                        .child(div().line_clamp(6).child(prompt)),
                )
            })
            .children(
                chat_rows::agent_rows(self.view, id)
                    .iter()
                    .map(|unit| self.unit(unit, live)),
            )
            .when_some(footer, |body, footer| {
                body.child(
                    div()
                        .text_size(crate::rems_from_px(11.5))
                        .text_color(cx.theme().foreground.faint())
                        .child(footer),
                )
            })
    }
}

fn run_label(label: &RunLabel) -> String {
    match label {
        RunLabel::Live { action, detail } => format!("{action} {detail}"),
        RunLabel::Thinking { heading } => heading.clone().unwrap_or_else(|| "Thinking".to_owned()),
        RunLabel::Done { summary } => summary.clone(),
    }
}

fn group_summary(states: &[Option<AgentState>]) -> String {
    let count = |matches: fn(&AgentState) -> bool| {
        states
            .iter()
            .filter(|state| state.as_ref().is_some_and(matches))
            .count()
    };
    [
        (
            count(|state| matches!(state, AgentState::Working { .. })),
            "working",
        ),
        (
            count(|state| matches!(state, AgentState::Waiting)),
            "need you",
        ),
        (
            count(|state| matches!(state, AgentState::Done { .. })),
            "done",
        ),
        (
            count(|state| matches!(state, AgentState::Failed { .. })),
            "failed",
        ),
        (
            states
                .iter()
                .filter(|state| matches!(state, Some(AgentState::Stopped) | None))
                .count(),
            "stopped",
        ),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, word)| {
        if count == 1 && word == "need you" {
            "1 needs you".to_owned()
        } else {
            format!("{count} {word}")
        }
    })
    .collect::<Vec<_>>()
    .join(" · ")
}

fn fold_row(turn: TurnId, fold: Fold, expanded: bool, cx: &App) -> Stateful<Div> {
    let mut mark = ActivityMark::bare();
    if matches!(fold, Fold::Failed { .. }) {
        mark.icon = Some(IconName::TriangleAlert);
        mark.tint = Some(cx.theme().danger);
    }
    activity_row_marked(
        ("chat-fold", turn.0),
        mark,
        fold.label().into(),
        RowEnd::Disclosure(expanded),
        cx,
    )
}

fn text_of(view: &ChatView, id: &ItemId) -> SharedString {
    view.item(id)
        .map(|item| item.body.text.clone())
        .unwrap_or_default()
        .into()
}

fn user_bubble(id: &ItemId, text: SharedString, cx: &App) -> AnyElement {
    h_flex()
        .id(SharedString::from(format!("chat-user-{}", id.as_str())))
        .w_full()
        .justify_end()
        .child(
            div()
                .max_w(relative(0.8))
                .px_3()
                .py_2()
                .surface(crate::roles::AGENT_BUBBLE, cx)
                .text_size(crate::rems_from_px(13.0))
                .child(text),
        )
        .into_any_element()
}

fn markdown(id: &ItemId, text: SharedString) -> AnyElement {
    div()
        .w_full()
        .text_size(crate::rems_from_px(13.0))
        .child(TextView::markdown(
            SharedString::from(format!("chat-text-{}", id.as_str())),
            text,
        ))
        .into_any_element()
}

fn kind_icon(kind: ItemKind) -> IconName {
    match kind {
        ItemKind::Read => IconName::File,
        ItemKind::Edit => IconName::Pencil,
        ItemKind::Search => IconName::Search,
        ItemKind::Execute => IconName::SquareTerminal,
        ItemKind::Fetch | ItemKind::Web => IconName::Globe,
        ItemKind::Subagent => IconName::Bot,
        ItemKind::Plan => IconName::List,
        ItemKind::Reasoning => IconName::Sparkle,
        ItemKind::Error => IconName::TriangleAlert,
        _ => IconName::Layers,
    }
}

fn step_row(item: &zz_protocol::chat::ItemView, phase: f32, cx: &App) -> AnyElement {
    let mut mark = ActivityMark::plain(kind_icon(item.kind));
    match &item.status {
        ItemStatus::Running => mark.spin = Some(phase),
        ItemStatus::Failed { .. } => mark.tint = Some(cx.theme().danger),
        ItemStatus::Declined => mark.tint = Some(cx.theme().warning),
        _ => {}
    }
    if matches!(item.status, ItemStatus::Failed { .. }) {
        mark.label_color = Some(cx.theme().danger);
    }
    let label = item
        .body
        .target
        .clone()
        .filter(|target| !target.is_empty())
        .unwrap_or_else(|| item.body.title.clone());
    activity_row_marked(
        SharedString::from(format!("chat-step-{}", item.id.as_str())),
        mark,
        label.into(),
        RowEnd::None,
        cx,
    )
    .into_any_element()
}

fn asks_under<'a>(view: &'a ChatView, root: &ItemId) -> Vec<&'a RequestView> {
    let mut found = vec![root.clone()];
    let mut index = 0;
    while index < found.len() {
        if let Some(item) = view.item(&found[index]) {
            found.extend(item.children.iter().cloned());
        }
        index += 1;
    }
    view.open_requests()
        .filter(|request| {
            request
                .item
                .as_ref()
                .is_some_and(|item| item != root && found.contains(item))
        })
        .collect()
}

fn plan_card(view: &ChatView, id: &ItemId, cx: &App) -> AnyElement {
    let Some(item) = view.item(id) else {
        return div().into_any_element();
    };
    let done = item
        .body
        .steps
        .iter()
        .filter(|step| step.status == StepStatus::Done)
        .count();
    card_surface(cx)
        .child(
            card_header()
                .child(card_glyph(Icon::new(IconName::List), cx))
                .child(div().min_w_0().flex_1().child("Plan"))
                .child(card_chip(format!("{done} of {}", item.body.steps.len()))),
        )
        .child(
            v_flex()
                .w_full()
                .px(px(12.0))
                .pb(px(10.0))
                .gap_1()
                .text_size(crate::rems_from_px(12.5))
                .children(item.body.steps.iter().map(|step| {
                    let (icon, color) = match step.status {
                        StepStatus::Done => (IconName::CircleCheck, cx.theme().success),
                        StepStatus::Running => (IconName::Loader, cx.theme().accent),
                        StepStatus::Cancelled => (IconName::CircleX, cx.theme().foreground.muted()),
                        StepStatus::Pending => (IconName::Clock, cx.theme().foreground.muted()),
                    };
                    h_flex()
                        .gap_2()
                        .child(Icon::new(icon).size(px(12.0)).text_color(color))
                        .child(step.text.clone())
                })),
        )
        .into_any_element()
}

fn notice_row(view: &ChatView, id: &ItemId, cx: &App) -> AnyElement {
    let Some(item) = view.item(id) else {
        return div().into_any_element();
    };
    let mut mark = ActivityMark::plain(if item.kind == ItemKind::Error {
        IconName::TriangleAlert
    } else {
        IconName::Info
    });
    if item.kind == ItemKind::Error {
        mark.tint = Some(cx.theme().danger);
    }
    activity_row_marked(
        SharedString::from(format!("chat-notice-{}", id.as_str())),
        mark,
        item.body.text.clone().into(),
        RowEnd::None,
        cx,
    )
    .into_any_element()
}

fn ask_card(request: &RequestView, highlighted: Option<usize>, cx: &App) -> AnyElement {
    let id = request.id.0;
    let enabled = highlighted.is_some();
    match &request.ask {
        Ask::Approval { title, options, .. }
        | Ask::Plan {
            markdown: title,
            options,
        } => {
            let options = options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    permission_option(
                        (SharedString::from(format!("chat-ask-{id}")), index),
                        index,
                        option.label.clone(),
                        option.kind == OptionKind::Deny,
                        highlighted == Some(index),
                        enabled,
                        cx,
                    )
                    .into_any_element()
                })
                .collect();
            permission_card(
                title.clone(),
                None,
                options,
                Button::new(SharedString::from(format!("chat-ask-close-{id}")))
                    .tooltip("Dismiss")
                    .disabled(!enabled),
                cx,
            )
            .into_any_element()
        }
        Ask::Questions { questions } => {
            let Some(question) = questions.first() else {
                return div().into_any_element();
            };
            let counter = (questions.len() > 1).then(|| format!("1 of {}", questions.len()));
            card_surface(cx)
                .child(
                    card_header()
                        .child(card_glyph(Icon::new(IconName::Info), cx))
                        .child(div().min_w_0().flex_1().child(question.question.clone()))
                        .when_some(counter, |header, counter| header.child(card_chip(counter))),
                )
                .child(
                    card_tiles()
                        .pb(px(10.0))
                        .children(question.options.iter().enumerate().map(|(index, option)| {
                            permission_option(
                                (SharedString::from(format!("chat-question-{id}")), index),
                                index,
                                option.label.clone(),
                                false,
                                highlighted == Some(index),
                                enabled,
                                cx,
                            )
                            .into_any_element()
                        })),
                )
                .into_any_element()
        }
    }
}

fn record_card(request: &RequestView, cx: &App) -> AnyElement {
    let pairs = chat_rows::answered(request);
    if pairs.is_empty() {
        return record_row(request, cx);
    }
    let noun = if pairs.len() == 1 {
        "question"
    } else {
        "questions"
    };
    v_flex()
        .id(SharedString::from(format!("chat-record-{}", request.id.0)))
        .w_full()
        .gap_1()
        .px(px(12.0))
        .py(px(8.0))
        .surface(crate::roles::INLINE_PANEL, cx)
        .text_size(crate::rems_from_px(12.5))
        .child(
            h_flex()
                .gap_2()
                .text_color(cx.theme().foreground.muted())
                .child(Icon::new(IconName::CheckRead).size(px(12.0)))
                .child(format!("{} {noun}", pairs.len())),
        )
        .children(pairs.into_iter().map(|(label, value)| {
            h_flex()
                .w_full()
                .gap_2()
                .pl(px(20.0))
                .child(
                    div()
                        .flex_none()
                        .max_w(relative(0.4))
                        .truncate()
                        .text_color(cx.theme().foreground.muted())
                        .child(label),
                )
                .child(
                    Icon::new(IconName::ArrowRight)
                        .size(px(11.0))
                        .text_color(cx.theme().foreground.faint()),
                )
                .child(div().min_w_0().flex_1().truncate().child(value))
        }))
        .into_any_element()
}

fn record_row(request: &RequestView, cx: &App) -> AnyElement {
    let count = match &request.ask {
        Ask::Questions { questions } => questions.len().max(1),
        _ => 1,
    };
    let answers = match &request.close {
        Some(RequestClose::Answered {
            answer: Answer::Questions { answers },
        }) => answers
            .iter()
            .flat_map(|answer| answer.answers.iter().cloned())
            .collect::<Vec<_>>()
            .join(", "),
        Some(RequestClose::Answered { .. }) => "answered".to_owned(),
        Some(RequestClose::Withdrawn | RequestClose::Dismissed) => "no answer".to_owned(),
        None => String::new(),
    };
    let noun = if count == 1 { "question" } else { "questions" };
    activity_row_marked(
        SharedString::from(format!("chat-record-{}", request.id.0)),
        ActivityMark::plain(IconName::CheckRead),
        format!("Asked {count} {noun} · {answers}").into(),
        RowEnd::None,
        cx,
    )
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use zz_protocol::chat::{ChatEvent, ItemBody, ItemId, ItemKind, TurnBy, TurnId};

    use super::{ChatClick, ChatPane, ChatView};

    fn ready(session: &str) -> ChatView {
        let mut view = ChatView::default();
        view.apply(&ChatEvent::SessionReady {
            session: session.to_owned(),
        })
        .expect("ready");
        view.apply(&ChatEvent::TurnStarted {
            turn: TurnId(1),
            by: TurnBy::Agent,
            at: 1,
        })
        .expect("turn");
        view.apply(&ChatEvent::ItemStarted {
            item: ItemId::new("a"),
            turn: Some(TurnId(1)),
            parent: None,
            kind: ItemKind::Subagent,
            body: ItemBody::titled("Explore"),
            at: 2,
        })
        .expect("item");
        view
    }

    #[test]
    fn opened_rows_close_when_the_session_changes() {
        let mut pane = ChatPane::default();
        let first = ready("s1");
        pane.sync(&first);
        pane.click(&ChatClick::Agent(ItemId::new("a")));
        pane.click(&ChatClick::Turn(TurnId(1)));
        pane.sync(&first);
        assert_eq!(pane.open.agents, [ItemId::new("a")]);
        assert!(pane.away);
        pane.sync(&ready("s2"));
        assert!(pane.open.agents.is_empty());
        assert!(pane.open.turns.is_empty());
    }

    #[test]
    fn opened_rows_drop_items_the_view_no_longer_has() {
        let mut pane = ChatPane::default();
        let view = ready("s1");
        pane.sync(&view);
        pane.click(&ChatClick::Agent(ItemId::new("gone")));
        pane.sync(&view);
        assert!(pane.open.agents.is_empty());
    }
}
