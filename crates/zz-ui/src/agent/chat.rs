use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    rc::Rc,
    time::Duration,
};

use ala::{
    AnyElement, App, ClipboardItem, Div, FontWeight, HighlightStyle, IntoElement, ScrollHandle,
    ScrollWheelEvent, SharedString, Stateful, StyledText, Transformation, Window, div, percentage,
    prelude::*, px, relative,
};
use zz_client::chat_rows::{self, AgentState, Fold, RunLabel, Unit};
use zz_protocol::chat::{
    Answer, Ask, ChatView, ItemId, ItemKind, ItemStatus, OptionKind, RequestClose, RequestView,
    SessionView, StepStatus, TurnId, TurnView,
};

use super::presentation::{
    card_chip, card_glyph, card_header, card_surface, card_tiles, permission_card,
    permission_option,
};
use super::refs::{ChatRefs, RefLinks, linkify_markdown, ref_text};
use super::{ActivityMark, RowEnd, activity_row_marked, assistant_markdown_extensions};
use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, Icon, IconName, StyledExt as _,
    Tone, WindowExt as _, button::Button, h_flex, notification::Notification, pip::PipSurface,
    text::TextView, v_flex,
};

#[derive(Clone, Debug, Default)]
pub struct ChatOpen {
    pub turns: Vec<TurnId>,
    pub runs: Vec<ItemId>,
    pub agent: Option<ItemId>,
    pub highlighted: usize,
    pub arrivals: HashMap<ItemId, u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChatClick {
    Turn(TurnId),
    Run(ItemId),
    Agent(ItemId),
    CloseAgent,
    Fork(ItemId),
}

pub type ChatClicks = Rc<dyn Fn(&ChatClick, &mut Window, &mut App)>;

#[derive(Default)]
pub struct ChatHooks<'a> {
    pub click: Option<ChatClicks>,
    pub ask: Option<&'a dyn Fn(&RequestView, &App) -> Option<AnyElement>>,
    pub fork: bool,
    pub refs: Option<&'a ChatRefs>,
    pub vendor: Option<IconName>,
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

    fn fork_from(&self, id: &ItemId) -> Option<(ChatClicks, ItemId)> {
        self.click
            .clone()
            .filter(|_| self.fork)
            .filter(|_| zz_client::agent_settings::source_message(id.as_str()).is_some())
            .map(|click| (click, id.clone()))
    }
}

const FOLLOW_SLACK: f32 = 24.0;
const REVEAL_MAX_MS: u64 = 900;
const REVEAL_MIN_MS: u64 = 220;
const REVEAL_MS_PER_CHAR: u64 = 2;
const USER_MAX_HEIGHT: f32 = 260.0;
const SHIMMER_MS: u64 = 1600;

#[derive(Default)]
pub struct ChatPane {
    pub open: ChatOpen,
    pub scroll: ScrollHandle,
    pub away: bool,
    session: Option<String>,
    known: HashSet<ItemId>,
    primed: bool,
}

impl ChatPane {
    pub fn click(&mut self, click: &ChatClick) {
        match click {
            ChatClick::Turn(turn) => toggle(&mut self.open.turns, *turn),
            ChatClick::Run(item) => toggle(&mut self.open.runs, item.clone()),
            ChatClick::Agent(item) => {
                self.open.agent = (self.open.agent.as_ref() != Some(item)).then(|| item.clone());
                return;
            }
            ChatClick::CloseAgent => {
                self.open.agent = None;
                return;
            }
            ChatClick::Fork(_) => return,
        }
        self.away = true;
    }

    #[must_use]
    pub fn revealing(&self, now: u64) -> bool {
        self.open
            .arrivals
            .values()
            .any(|at| now.saturating_sub(*at) < REVEAL_MAX_MS)
    }

    pub fn sync(&mut self, view: &ChatView) {
        let session = match &view.session {
            SessionView::Ready { session } => Some(session.clone()),
            _ => view.last_session.clone(),
        };
        if session != self.session {
            self.open = ChatOpen::default();
            self.session = session;
            self.known.clear();
            self.primed = false;
        }
        self.open
            .turns
            .retain(|turn| view.turns.iter().any(|held| held.id == *turn));
        self.open.runs.retain(|id| view.item(id).is_some());
        if self
            .open
            .agent
            .as_ref()
            .is_some_and(|id| view.item(id).is_none())
        {
            self.open.agent = None;
        }
        let now = chat_rows::now_ms();
        for item in view.items.values() {
            if item.kind == ItemKind::Agent
                && item.parent.is_none()
                && self.known.insert(item.id.clone())
                && self.primed
            {
                self.open.arrivals.insert(item.id.clone(), now);
            }
        }
        self.primed = true;
        self.open
            .arrivals
            .retain(|_, at| now.saturating_sub(*at) < REVEAL_MAX_MS);
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
        let reply = v_flex()
            .id(("chat-reply", turn.id.0))
            .group(REPLY_GROUP)
            .w_full()
            .gap_3()
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
            .when(live && !self.view.waiting(), |column| {
                column.child(self.thinking(turn))
            })
            .when(!live, |column| {
                let ordered = rows.shown[..lead]
                    .iter()
                    .chain(&rows.folded)
                    .chain(&rows.shown[lead..]);
                let reply = ordered
                    .clone()
                    .filter_map(|unit| match unit {
                        Unit::Text(id) => Some(text_of(self.view, id)),
                        _ => None,
                    })
                    .filter(|text| !text.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let fork = ordered
                    .clone()
                    .find_map(|unit| match unit {
                        Unit::User(id) => Some(id),
                        _ => None,
                    })
                    .and_then(|id| self.hooks.fork_from(id));
                let stamp = ordered
                    .clone()
                    .rev()
                    .find_map(|unit| match unit {
                        Unit::Text(id) => self.view.item(id),
                        _ => None,
                    })
                    .map(|item| item.started_at)
                    .or(turn.ended_at);
                column.when(!reply.is_empty() || fork.is_some(), |column| {
                    column.child(message_actions(
                        &format!("turn-{}", turn.id.0),
                        stamp,
                        (!reply.is_empty()).then(|| reply.into()),
                        fork,
                        REPLY_GROUP,
                        self.cx,
                    ))
                })
            });
        v_flex()
            .id(("chat-turn", turn.id.0))
            .w_full()
            .gap_3()
            .children(rows.shown[..lead].iter().map(|unit| self.unit(unit, live)))
            .child(reply)
            .into_any_element()
    }

    fn unit(&self, unit: &Unit, live: bool) -> AnyElement {
        let cx = self.cx;
        match unit {
            Unit::User(id) => user_bubble(
                id,
                text_of(self.view, id),
                self.view.item(id).map(|item| item.started_at),
                self.hooks.fork_from(id),
                self.hooks.refs,
                cx,
            ),
            Unit::Text(id) => markdown(id, self.shown_text(id), self.hooks.refs),
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

    fn shown_text(&self, id: &ItemId) -> SharedString {
        let text = text_of(self.view, id);
        let Some(at) = self.open.arrivals.get(id) else {
            return text;
        };
        let length = u64::try_from(text.len()).unwrap_or(u64::MAX);
        let duration = length
            .saturating_mul(REVEAL_MS_PER_CHAR)
            .clamp(REVEAL_MIN_MS, REVEAL_MAX_MS);
        let elapsed = self.now.saturating_sub(*at);
        if elapsed >= duration {
            return text;
        }
        let mut cut = usize::try_from(length * elapsed / duration).unwrap_or(text.len());
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        let cut = text[..cut].rfind(char::is_whitespace).unwrap_or_default();
        text[..cut].to_owned().into()
    }

    fn thinking(&self, turn: &TurnView) -> AnyElement {
        let cx = self.cx;
        let working = turn.items.iter().any(|id| {
            self.view.item(id).is_some_and(|item| {
                item.status.is_open()
                    && !matches!(
                        item.kind,
                        ItemKind::Agent | ItemKind::Reasoning | ItemKind::User
                    )
            })
        });
        let label = if working { "Working" } else { "Thinking" };
        let seconds = self.now.saturating_sub(turn.started_at) / 1000;
        let elapsed =
            (seconds > 0 && seconds < 86_400).then(|| chat_rows::duration(seconds * 1000));
        h_flex()
            .id(("chat-thinking", turn.id.0))
            .gap_2()
            .items_center()
            .text_size(crate::rems_from_px(12.5))
            .child(
                Icon::new(IconName::Sparkle)
                    .size(px(13.0))
                    .text_color(cx.theme().foreground.muted())
                    .transform(Transformation::rotate(percentage(self.phase))),
            )
            .child(shimmer(label, self.now, cx))
            .when_some(elapsed, |row, elapsed| {
                row.child(
                    div()
                        .text_color(cx.theme().foreground.faint())
                        .child(elapsed),
                )
            })
            .into_any_element()
    }

    fn agents(&self, ids: &[ItemId]) -> AnyElement {
        v_flex()
            .w_full()
            .gap_1p5()
            .children(ids.iter().map(|id| self.agent(id)))
            .into_any_element()
    }

    fn agent(&self, id: &ItemId) -> AnyElement {
        let cx = self.cx;
        let Some(item) = self.view.item(id) else {
            return div().into_any_element();
        };
        let state = chat_rows::agent_state(self.view, id);
        let ms = chat_rows::agent_ms(self.view, id, self.now)
            .filter(|ms| item.status.is_open() || *ms >= 1000);
        let open = self.open.agent.as_ref() == Some(id);
        let foreground = cx.theme().foreground;
        let row = h_flex()
            .id(SharedString::from(format!("chat-agent-{}", id.as_str())))
            .w_full()
            .h(px(36.0))
            .px(px(8.0))
            .gap_2()
            .items_center()
            .surface(crate::roles::INLINE_PANEL, cx)
            .text_size(crate::rems_from_px(13.0))
            .text_color(if open { foreground } else { foreground.muted() })
            .hover(move |row| row.text_color(foreground))
            .child(vendor_tile(self.hooks.vendor.clone(), cx))
            .child(div().min_w_0().flex_1().truncate().child(agent_title(item)))
            .children(agent_status(state.as_ref(), self.phase, cx))
            .when_some(ms, |row, ms| {
                row.child(
                    div()
                        .flex_none()
                        .text_size(crate::rems_from_px(11.5))
                        .text_color(cx.theme().foreground.faint())
                        .child(chat_rows::duration(ms)),
                )
            })
            .child(
                Icon::new(IconName::PictureInPicture)
                    .size(px(13.0))
                    .text_color(cx.theme().foreground.faint()),
            );
        v_flex()
            .w_full()
            .child(self.hooks.clickable(row, ChatClick::Agent(id.clone())))
            .children(
                asks_under(self.view, id)
                    .into_iter()
                    .map(|request| div().w_full().pt_1().pb_2().child(self.ask(request))),
            )
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
        let result = match state {
            Some(AgentState::Done {
                result: Some(result),
            }) => Some(result.clone()),
            _ => None,
        };
        let rows = chat_rows::agent_rows(self.view, id);
        let says = rows.iter().any(|unit| matches!(unit, Unit::Text(_)));
        v_flex()
            .w_full()
            .gap_3()
            .when_some(prompt, |body, prompt| {
                body.child(user_bubble(
                    &ItemId::new(format!("prompt-{}", id.as_str())),
                    prompt.into(),
                    None,
                    None,
                    self.hooks.refs,
                    cx,
                ))
            })
            .children(rows.iter().map(|unit| self.unit(unit, live)))
            .when_some(result.filter(|_| !says), |body, result| {
                body.child(markdown(
                    &ItemId::new(format!("result-{}", id.as_str())),
                    result.into(),
                    self.hooks.refs,
                ))
            })
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

fn agent_title(item: &zz_protocol::chat::ItemView) -> String {
    if item.body.title.is_empty() {
        "Subagent".to_owned()
    } else {
        item.body.title.clone()
    }
}

fn vendor_tile(vendor: Option<IconName>, cx: &App) -> Div {
    div()
        .flex_none()
        .size(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .bg(cx.theme().paint(Tone::Neutral, Emphasis::Soft).fill)
        .child(Icon::new(vendor.unwrap_or(IconName::Bot)).size(px(13.0)))
}

fn agent_status(state: Option<&AgentState>, phase: f32, cx: &App) -> Option<AnyElement> {
    let glyph = |icon: IconName, color| Icon::new(icon).size(px(13.0)).text_color(color);
    Some(match state? {
        AgentState::Working { .. } => super::spinner(phase)
            .size(px(13.0))
            .text_color(cx.theme().foreground.muted())
            .into_any_element(),
        AgentState::Waiting => h_flex()
            .flex_none()
            .gap_1()
            .text_size(crate::rems_from_px(11.5))
            .text_color(cx.theme().warning)
            .child(glyph(IconName::TriangleAlert, cx.theme().warning))
            .child("Needs you")
            .into_any_element(),
        AgentState::Done { .. } => glyph(IconName::Check, cx.theme().success).into_any_element(),
        AgentState::Failed { .. } => glyph(IconName::CircleX, cx.theme().danger).into_any_element(),
        AgentState::Stopped => {
            glyph(IconName::Stop, cx.theme().foreground.faint()).into_any_element()
        }
    })
}

fn shimmer(label: &str, now: u64, cx: &App) -> StyledText {
    let text = format!("{label}…");
    let count = text.chars().count();
    let span = u64::try_from(count + 8).unwrap_or(u64::MAX);
    let head = usize::try_from((now % SHIMMER_MS) * span / SHIMMER_MS).unwrap_or_default();
    let theme = cx.theme();
    let mut highlights: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    for (index, (offset, character)) in text.char_indices().enumerate() {
        let distance = (index + 4).abs_diff(head);
        let color = match distance {
            0 | 1 => theme.foreground,
            2 | 3 => theme.foreground.muted(),
            _ => theme.foreground.faint(),
        };
        let range = offset..offset + character.len_utf8();
        match highlights.last_mut() {
            Some((last, style)) if style.color == Some(color) && last.end == range.start => {
                last.end = range.end;
            }
            _ => highlights.push((
                range,
                HighlightStyle {
                    color: Some(color),
                    ..HighlightStyle::default()
                },
            )),
        }
    }
    StyledText::new(text).with_highlights(highlights)
}

#[must_use]
pub fn chat_agent_pip(
    view: &ChatView,
    now: u64,
    open: &ChatOpen,
    phase: f32,
    hooks: &ChatHooks<'_>,
    cx: &App,
) -> Option<AnyElement> {
    let id = open.agent.as_ref()?;
    let item = view.item(id)?;
    let draw = Draw {
        view,
        now,
        open,
        phase,
        hooks,
        cx,
    };
    let state = chat_rows::agent_state(view, id);
    let ms = chat_rows::agent_ms(view, id, now).filter(|ms| item.status.is_open() || *ms >= 1000);
    let close = hooks.click.clone();
    let header = h_flex()
        .flex_none()
        .h(cx.theme().pane_header_height())
        .w_full()
        .pl_2()
        .pr_1()
        .gap_2()
        .items_center()
        .text_size(crate::rems_from_px(12.5))
        .child(vendor_tile(hooks.vendor.clone(), cx))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .child(agent_title(item)),
        )
        .children(agent_status(state.as_ref(), phase, cx))
        .child(
            Button::compact_icon("chat-agent-pip-close", IconName::Xmark)
                .surface_role(crate::roles::AGENT_ACTION)
                .tooltip("Close")
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    if let Some(close) = &close {
                        close(&ChatClick::CloseAgent, window, cx);
                    }
                }),
        );
    let body = div()
        .id(SharedString::from(format!(
            "chat-agent-pip-{}",
            id.as_str()
        )))
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px_3()
        .pt_2()
        .pb_3()
        .text_color(cx.theme().foreground)
        .child(draw.agent_body(id, state.as_ref(), ms));
    Some(
        div()
            .absolute()
            .top(px(10.0))
            .right(px(10.0))
            .w(relative(0.62))
            .min_w(px(300.0))
            .max_w(px(560.0))
            .h(relative(0.72))
            .child(PipSurface::new(
                SharedString::from(format!("chat-agent-pip-surface-{}", id.as_str())),
                v_flex().size_full().child(header).child(body),
            ))
            .into_any_element(),
    )
}

fn run_label(label: &RunLabel) -> String {
    match label {
        RunLabel::Live { action, detail } => format!("{action} {detail}"),
        RunLabel::Thinking { heading } => heading.clone().unwrap_or_else(|| "Thinking".to_owned()),
        RunLabel::Done { summary } => summary.clone(),
    }
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

const USER_GROUP: &str = "chat-user";
const REPLY_GROUP: &str = "chat-reply";

fn stamp_label(at: u64) -> Option<String> {
    let at = chrono::DateTime::from_timestamp_millis(i64::try_from(at).ok()?)?
        .with_timezone(&chrono::Local);
    let today = chrono::Local::now();
    let format = if at.date_naive() == today.date_naive() {
        "%-I:%M %p"
    } else if chrono::Datelike::year(&at) == chrono::Datelike::year(&today) {
        "%b %-d, %-I:%M %p"
    } else {
        "%b %-d %Y, %-I:%M %p"
    };
    Some(at.format(format).to_string())
}

fn message_actions(
    key: &str,
    stamp: Option<u64>,
    copy: Option<SharedString>,
    fork: Option<(ChatClicks, ItemId)>,
    group: &'static str,
    cx: &App,
) -> AnyElement {
    let stamp = stamp.filter(|at| *at > 0).and_then(stamp_label);
    h_flex()
        .flex_none()
        .gap_0p5()
        .items_center()
        .when(!crate::touch::CoarsePointer::get(cx), |row| {
            row.invisible().group_hover(group, ala::Styled::visible)
        })
        .when_some(stamp, |row, stamp| {
            row.child(
                div()
                    .px_1()
                    .text_size(crate::rems_from_px(11.5))
                    .text_color(cx.theme().foreground.faint())
                    .child(stamp),
            )
        })
        .when_some(copy, |row, text| {
            row.child(
                Button::compact_icon(
                    SharedString::from(format!("chat-copy-{key}")),
                    IconName::Copy,
                )
                .surface_role(crate::roles::AGENT_ACTION)
                .tooltip("Copy")
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                    window.push_notification(
                        Notification::new()
                            .message("Copied")
                            .autohide_after(Duration::from_millis(1600)),
                        cx,
                    );
                }),
            )
        })
        .when_some(fork, |row, (fork, id)| {
            let target = ChatClick::Fork(id);
            row.child(
                Button::compact_icon(
                    SharedString::from(format!("chat-fork-{key}")),
                    IconName::GitBranch,
                )
                .surface_role(crate::roles::AGENT_ACTION)
                .tooltip("Fork from here: a new chat that goes on from this message")
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    fork(&target, window, cx);
                }),
            )
        })
        .into_any_element()
}

fn user_bubble(
    id: &ItemId,
    text: SharedString,
    stamp: Option<u64>,
    fork: Option<(ChatClicks, ItemId)>,
    refs: Option<&ChatRefs>,
    cx: &App,
) -> AnyElement {
    let key = id.as_str().to_owned();
    let body = refs
        .and_then(|refs| {
            ref_text(
                SharedString::from(format!("chat-user-text-{key}")),
                &text,
                refs,
                cx,
            )
        })
        .unwrap_or_else(|| text.clone().into_any_element());
    v_flex()
        .id(SharedString::from(format!("chat-user-{key}")))
        .group(USER_GROUP)
        .w_full()
        .items_end()
        .gap_1()
        .child(
            div()
                .max_w(relative(0.8))
                .px_3()
                .py_2()
                .surface(crate::roles::AGENT_BUBBLE, cx)
                .text_size(crate::rems_from_px(13.0))
                .child(
                    div()
                        .id(SharedString::from(format!("chat-user-scroll-{key}")))
                        .max_h(px(USER_MAX_HEIGHT))
                        .overflow_y_scroll()
                        .child(body),
                ),
        )
        .child(message_actions(
            &format!("user-{key}"),
            stamp,
            Some(text),
            fork,
            USER_GROUP,
            cx,
        ))
        .into_any_element()
}

fn markdown(id: &ItemId, text: SharedString, refs: Option<&ChatRefs>) -> AnyElement {
    let id = SharedString::from(format!("chat-text-{}", id.as_str()));
    let view = match refs {
        Some(refs) => {
            let (linked, targets) = linkify_markdown(&text, refs);
            let view = TextView::markdown(id, linked);
            if targets.is_empty() {
                view
            } else {
                view.links(RefLinks::new(targets, refs.open.clone()))
            }
        }
        None => TextView::markdown(id, text),
    };
    div()
        .w_full()
        .text_size(crate::rems_from_px(13.0))
        .child(view.markdown_extensions(assistant_markdown_extensions()))
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
    v_flex()
        .id(SharedString::from(format!("chat-record-{}", request.id.0)))
        .w_full()
        .gap_3()
        .px(px(14.0))
        .py(px(10.0))
        .surface(crate::roles::INLINE_PANEL, cx)
        .text_size(crate::rems_from_px(13.0))
        .children(pairs.into_iter().map(|(question, answer)| {
            v_flex()
                .w_full()
                .gap_1()
                .child(div().w_full().child(question))
                .child(
                    div()
                        .w_full()
                        .text_color(cx.theme().foreground.muted())
                        .child(answer),
                )
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
        assert_eq!(pane.open.agent, Some(ItemId::new("a")));
        assert!(pane.away);
        pane.sync(&ready("s2"));
        assert_eq!(pane.open.agent, None);
        assert!(pane.open.turns.is_empty());
    }

    #[test]
    fn an_agent_pip_closes_on_a_second_click_or_close() {
        let mut pane = ChatPane::default();
        let view = ready("s1");
        pane.sync(&view);
        pane.click(&ChatClick::Agent(ItemId::new("a")));
        pane.click(&ChatClick::Agent(ItemId::new("a")));
        assert_eq!(pane.open.agent, None);
        pane.click(&ChatClick::Agent(ItemId::new("a")));
        pane.click(&ChatClick::CloseAgent);
        assert_eq!(pane.open.agent, None);
        assert!(!pane.away);
    }

    #[test]
    fn opened_rows_drop_items_the_view_no_longer_has() {
        let mut pane = ChatPane::default();
        let view = ready("s1");
        pane.sync(&view);
        pane.click(&ChatClick::Agent(ItemId::new("gone")));
        pane.sync(&view);
        assert_eq!(pane.open.agent, None);
    }

    #[test]
    fn only_replies_that_arrive_after_the_first_look_type_in() {
        let mut pane = ChatPane::default();
        let mut view = ready("s1");
        let reply = |view: &mut ChatView, id: &str| {
            view.apply(&ChatEvent::ItemStarted {
                item: ItemId::new(id),
                turn: Some(TurnId(1)),
                parent: None,
                kind: ItemKind::Agent,
                body: ItemBody::text("hello there"),
                at: 3,
            })
            .expect("reply");
        };
        reply(&mut view, "old");
        pane.sync(&view);
        assert!(pane.open.arrivals.is_empty());
        reply(&mut view, "new");
        pane.sync(&view);
        assert!(pane.open.arrivals.contains_key(&ItemId::new("new")));
        assert!(!pane.open.arrivals.contains_key(&ItemId::new("old")));
        assert!(pane.revealing(zz_client::chat_rows::now_ms()));
    }
}
