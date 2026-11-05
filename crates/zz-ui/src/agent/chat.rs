use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash as _, Hasher as _},
    rc::{Rc, Weak},
    sync::Arc,
    time::Duration,
};

use ala::{
    AnyElement, App, ClipboardItem, Context, Div, Global, Image, ImageFormat, IntoElement,
    ScrollHandle, ScrollWheelEvent, SharedString, Stateful, Subscription, Window, div, prelude::*,
    px, relative,
};
use penna::{Card, CardLook, Label, Panel, PanelLook, TextStyle, components::button::ButtonLook};
use zz_client::chat_rows::{self, AgentState, Fold, RunLabel, Unit};
use zz_protocol::{
    agent_stream::AgentImage,
    chat::{
        Answer, Ask, ChatView, ItemId, ItemKind, ItemStatus, OptionKind, RequestClose, RequestView,
        SessionView, StepStatus, TurnId, TurnView,
    },
};

use super::refs::{ChatRefs, RefLinks, linkify_markdown, ref_text};
use super::{agent_text_view, assistant_markdown_extensions, attachment_tile};
use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, Icon, IconName, StyledExt as _,
    Tone, WindowExt as _, button::Button, h_flex, notification::Notification, pip::PipSurface,
    text::TextView, v_flex,
};
use penna::blocks::chat::{
    ActivityEnd, ActivityMark, CHAT_CONTENT_MAX_WIDTH, TRANSCRIPT_ATTACHMENT, activity_row,
    transcript_text,
};
use penna::blocks::chat::{
    card_chip, card_glyph, card_header, card_surface, card_tiles, card_title, permission_card,
    permission_option,
};

#[derive(Clone, Debug, Default)]
pub struct ChatOpen {
    pub turns: Vec<TurnId>,
    pub runs: Vec<ItemId>,
    pub agent: Option<ItemId>,
    pub highlighted: usize,
    pub arrivals: HashMap<ItemId, u64>,
    pub images: HashMap<ItemId, Arc<[Arc<Image>]>>,
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
    pub owner: Option<u64>,
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
const THOUGHT_MAX_HEIGHT: f32 = 260.0;

#[derive(Default)]
pub struct ChatPane {
    pub open: ChatOpen,
    pub scroll: ScrollHandle,
    pub away: bool,
    session: Option<String>,
    known: HashSet<ItemId>,
    primed: bool,
    retired: Vec<Arc<Image>>,
    shown: Rc<RefCell<HashSet<u64>>>,
    listed: bool,
}

#[derive(Default)]
struct ShownImages(Vec<Weak<RefCell<HashSet<u64>>>>);

impl Global for ShownImages {}

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
            let open = std::mem::take(&mut self.open);
            self.retired
                .extend(open.images.into_values().flat_map(|images| images.to_vec()));
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
        self.retired.extend(
            self.open
                .images
                .extract_if(|id, _| view.item(id).is_none_or(|item| item.body.images.is_empty()))
                .flat_map(|(_, images)| images.to_vec()),
        );
        let now = chat_rows::now_ms();
        for item in view.items.values() {
            if !item.body.images.is_empty() && !self.open.images.contains_key(&item.id) {
                self.open
                    .images
                    .insert(item.id.clone(), decode_images(&item.body.images));
            }
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

    pub fn release(&mut self, window: &mut Window, cx: &mut App) {
        *self.shown.borrow_mut() = self
            .open
            .images
            .values()
            .flat_map(|images| images.iter().map(|image| image.id))
            .collect();
        if !self.listed {
            self.listed = true;
            cx.default_global::<ShownImages>()
                .0
                .push(Rc::downgrade(&self.shown));
        }
        self.free(Some(window), cx);
    }

    pub fn close(&mut self, window: Option<&mut Window>, cx: &mut App) {
        let open = std::mem::take(&mut self.open.images);
        self.retired
            .extend(open.into_values().flat_map(|images| images.to_vec()));
        self.shown.borrow_mut().clear();
        self.free(window, cx);
    }

    pub fn close_on_release<T: 'static>(
        window: &Window,
        cx: &mut Context<T>,
        chat: fn(&mut T) -> &mut ChatPane,
    ) -> Subscription {
        let handle = window.window_handle();
        cx.on_release(move |owner, cx| {
            let chat = chat(owner);
            if handle
                .update(cx, |_, window, cx| chat.close(Some(window), cx))
                .is_err()
            {
                chat.close(None, cx);
            }
        })
    }

    fn free(&mut self, mut window: Option<&mut Window>, cx: &mut App) {
        if self.retired.is_empty() {
            return;
        }
        let panes = &mut cx.default_global::<ShownImages>().0;
        panes.retain(|pane| pane.strong_count() > 0);
        let mut shown = HashSet::new();
        for pane in panes.iter().filter_map(Weak::upgrade) {
            shown.extend(pane.borrow().iter().copied());
        }
        for image in self.retired.drain(..) {
            if shown.contains(&image.id) {
                continue;
            }
            if let Some(window) = window.as_deref_mut()
                && let Some(render) = Arc::clone(&image).get_render_image(window, cx)
            {
                cx.drop_image(render, Some(window));
            }
            image.remove_asset(cx);
        }
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

fn decode_images(images: &[AgentImage]) -> Arc<[Arc<Image>]> {
    images
        .iter()
        .filter_map(|image| {
            let format = ImageFormat::from_mime_type(&image.format)?;
            Some(Arc::new(Image::from_bytes(format, image.data.clone())))
        })
        .collect()
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

pub fn chat_column(body: impl IntoElement) -> Div {
    div().w_full().px_3().child(
        div()
            .w_full()
            .max_w(px(CHAT_CONTENT_MAX_WIDTH))
            .mx_auto()
            .pt_3()
            .pb(px(penna::blocks::chat::COMPOSER_OUTER_PADDING + 12.0))
            .child(body),
    )
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
        let moving = live && !self.view.waiting();
        let tail = match rows.shown[lead..].last() {
            Some(Unit::Run(run)) if moving && !matches!(run.label, RunLabel::Done { .. }) => {
                Some(run)
            }
            _ => None,
        };
        let settled = rows.shown.len() - usize::from(tail.is_some());
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
            .children(
                rows.shown[lead..settled]
                    .iter()
                    .map(|unit| self.unit(unit, live)),
            )
            .when(moving, |column| {
                column.child(self.live_line(turn, rows.activity.as_deref(), tail))
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
                self.images(id).as_deref().unwrap_or_default(),
                self.view.item(id).map(|item| item.started_at),
                self.hooks.fork_from(id),
                self.hooks.refs,
                cx,
            ),
            Unit::Text(id) => markdown(id, self.shown_text(id), self.hooks, cx),
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
        let opens = chat_rows::run_shows(view, &run.items);
        let expanded = opens && self.open.runs.contains(&first);
        let row = activity_row(
            SharedString::from(format!("chat-run-{}", first.as_str())),
            mark,
            label,
            if opens {
                ActivityEnd::Disclosure(expanded)
            } else {
                ActivityEnd::None
            },
            self.cx,
        );
        let row = if opens {
            self.hooks.clickable(row, ChatClick::Run(first.clone()))
        } else {
            row
        };
        v_flex()
            .w_full()
            .child(row)
            .when(expanded, |column| column.child(self.steps(&run.items)))
            .into_any_element()
    }

    fn images(&self, id: &ItemId) -> Option<Arc<[Arc<Image>]>> {
        if let Some(images) = self.open.images.get(id) {
            return Some(Arc::clone(images));
        }
        let item = self.view.item(id)?;
        (!item.body.images.is_empty()).then(|| decode_images(&item.body.images))
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

    fn live_line(
        &self,
        turn: &TurnView,
        activity: Option<&str>,
        tail: Option<&chat_rows::Run>,
    ) -> AnyElement {
        let seconds = self.now.saturating_sub(turn.started_at) / 1000;
        let mut mark = ActivityMark::plain(IconName::Sparkle);
        mark.turn = Some(self.phase);
        mark.shimmer = Some(self.now);
        mark.detail =
            (seconds > 0 && seconds < 86_400).then(|| chat_rows::duration(seconds * 1000).into());
        let run = tail.filter(|run| chat_rows::run_shows(self.view, &run.items));
        let first = run.and_then(|run| run.items.first()).cloned();
        let expanded = first
            .as_ref()
            .is_some_and(|first| self.open.runs.contains(first));
        let row = activity_row(
            ("chat-live", turn.id.0),
            mark,
            SharedString::from(activity.unwrap_or("Thinking").to_owned()),
            if first.is_some() {
                ActivityEnd::Disclosure(expanded)
            } else {
                ActivityEnd::None
            },
            self.cx,
        );
        let row = match first {
            Some(first) => self.hooks.clickable(row, ChatClick::Run(first)),
            None => row,
        };
        v_flex()
            .w_full()
            .child(row)
            .when_some(run.filter(|_| expanded), |column, run| {
                column.child(self.steps(&run.items))
            })
            .into_any_element()
    }

    fn steps(&self, items: &[ItemId]) -> Div {
        v_flex().w_full().pl(px(18.0)).children(
            items
                .iter()
                .filter_map(|id| self.view.item(id))
                .filter_map(|item| {
                    if item.kind.is_tool() {
                        Some(step_row(item, self.phase, self.cx))
                    } else {
                        thought_row(item, self.hooks.owner, self.cx)
                    }
                }),
        )
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
        let row = Panel::new(
            SharedString::from(format!("chat-agent-{}", id.as_str())),
            PanelLook::Inline,
        )
        .h_flex()
        .w_full()
        .h(px(36.0))
        .px(px(8.0))
        .gap_2()
        .type_style(TextStyle::Body, cx)
        .text_color(if open { foreground } else { foreground.muted() })
        .hover(move |row| row.text_color(foreground))
        .child(vendor_tile(self.hooks.vendor.clone(), cx))
        .child(div().min_w_0().flex_1().truncate().child(agent_title(item)))
        .children(agent_status(state.as_ref(), self.phase, cx))
        .when_some(ms, |row, ms| {
            row.child(
                Label::new(TextStyle::Caption, chat_rows::duration(ms))
                    .flex_none()
                    .text_color(cx.theme().foreground.faint()),
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
                    self.images(id).as_deref().unwrap_or_default(),
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
                    self.hooks,
                    cx,
                ))
            })
            .when_some(footer, |body, footer| {
                body.child(
                    Label::new(TextStyle::Caption, footer)
                        .text_color(cx.theme().foreground.faint()),
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
            .type_style(TextStyle::Caption, cx)
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
        .type_style(TextStyle::Small, cx)
        .child(vendor_tile(hooks.vendor.clone(), cx))
        .child(
            Label::new(TextStyle::Small, agent_title(item))
                .strong()
                .truncate()
                .min_w_0()
                .flex_1(),
        )
        .children(agent_status(state.as_ref(), phase, cx))
        .child(
            Button::compact_icon("chat-agent-pip-close", IconName::Xmark)
                .look(ButtonLook::RowIcon)
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
    activity_row(
        ("chat-fold", turn.0),
        mark,
        fold.label().into(),
        ActivityEnd::Disclosure(expanded),
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
                Label::new(TextStyle::Caption, stamp)
                    .px_1()
                    .text_color(cx.theme().foreground.faint()),
            )
        })
        .when_some(copy, |row, text| {
            row.child(
                penna::blocks::chat::message_action(
                    Button::compact_icon(
                        SharedString::from(format!("chat-copy-{key}")),
                        IconName::Copy,
                    ),
                    cx,
                )
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
                penna::blocks::chat::message_action(
                    Button::compact_icon(
                        SharedString::from(format!("chat-fork-{key}")),
                        IconName::GitBranch,
                    ),
                    cx,
                )
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
    images: &[Arc<Image>],
    stamp: Option<u64>,
    fork: Option<(ChatClicks, ItemId)>,
    refs: Option<&ChatRefs>,
    cx: &App,
) -> AnyElement {
    let key = id.as_str().to_owned();
    let ground = cx.theme().ground_of(CardLook::Bubble);
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
            transcript_text(
                Card::new("chat-user-bubble", CardLook::Bubble)
                    .v_flex()
                    .max_w(relative(0.8))
                    .gap_2()
                    .px_3()
                    .py_2(),
                cx,
            )
            .when(!images.is_empty(), |bubble| {
                bubble.child(
                    h_flex()
                        .flex_wrap()
                        .gap_1()
                        .children(images.iter().enumerate().map(|(index, image)| {
                            attachment_tile(
                                (
                                    SharedString::from(format!("chat-user-attachment-{key}")),
                                    index,
                                ),
                                Arc::clone(image),
                                TRANSCRIPT_ATTACHMENT,
                                ground,
                            )
                            .debug_selector(|| "chat-user-attachment".to_owned())
                        })),
                )
            })
            .when(!text.is_empty(), |bubble| {
                bubble.child(
                    div()
                        .id(SharedString::from(format!("chat-user-scroll-{key}")))
                        .max_h(px(USER_MAX_HEIGHT))
                        .overflow_y_scroll()
                        .child(body),
                )
            }),
        )
        .child(message_actions(
            &format!("user-{key}"),
            stamp,
            (!text.is_empty()).then_some(text),
            fork,
            USER_GROUP,
            cx,
        ))
        .into_any_element()
}

fn block_scope(owner: Option<u64>, id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    owner.hash(&mut hasher);
    id.hash(&mut hasher);
    hasher.finish()
}

fn markdown(id: &ItemId, text: SharedString, hooks: &ChatHooks<'_>, cx: &App) -> AnyElement {
    let id = SharedString::from(format!("chat-text-{}", id.as_str()));
    let scope = block_scope(hooks.owner, &id);
    let view = match hooks.refs {
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
    transcript_text(div().w_full(), cx)
        .child(
            agent_text_view(view, scope, hooks.owner, cx)
                .markdown_extensions(assistant_markdown_extensions()),
        )
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
    activity_row(
        SharedString::from(format!("chat-step-{}", item.id.as_str())),
        mark,
        label.into(),
        ActivityEnd::None,
        cx,
    )
    .into_any_element()
}

fn thought_row(
    item: &zz_protocol::chat::ItemView,
    owner: Option<u64>,
    cx: &App,
) -> Option<AnyElement> {
    let body = chat_rows::reasoning_body(&item.body.text)?;
    let id = SharedString::from(format!("chat-thought-{}", item.id.as_str()));
    let scope = block_scope(owner, &id);
    Some(
        transcript_text(
            div()
                .id(SharedString::from(format!(
                    "chat-thought-scroll-{}",
                    item.id.as_str()
                )))
                .w_full()
                .max_h(px(THOUGHT_MAX_HEIGHT))
                .overflow_y_scroll()
                .py_1(),
            cx,
        )
        .text_color(cx.theme().foreground.muted())
        .debug_selector(|| "chat-thought".to_owned())
        .child(agent_text_view(TextView::markdown(id, body), scope, owner, cx).selectable(true))
        .into_any_element(),
    )
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
    card_surface(SharedString::from(format!("chat-plan-{}", id.as_str())))
        .child(
            card_header()
                .child(card_glyph(Icon::new(IconName::List), cx))
                .child(card_title("Plan"))
                .child(card_chip(format!("{done} of {}", item.body.steps.len()))),
        )
        .child(
            v_flex()
                .w_full()
                .px(px(12.0))
                .pb(px(10.0))
                .gap_1()
                .type_style(TextStyle::Small, cx)
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
    activity_row(
        SharedString::from(format!("chat-notice-{}", id.as_str())),
        mark,
        item.body.text.clone().into(),
        ActivityEnd::None,
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
            card_surface(SharedString::from(format!("chat-ask-card-{id}")))
                .child(
                    card_header()
                        .child(card_glyph(Icon::new(IconName::Info), cx))
                        .child(card_title(question.question.clone()))
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
    Panel::new(
        SharedString::from(format!("chat-record-{}", request.id.0)),
        PanelLook::Inline,
    )
    .v_flex()
    .w_full()
    .gap_3()
    .px(px(14.0))
    .py(px(10.0))
    .children(pairs.into_iter().map(|(question, answer)| {
        v_flex()
            .w_full()
            .gap_1()
            .child(Label::new(TextStyle::Body, question).w_full())
            .child(
                Label::new(TextStyle::Body, answer)
                    .w_full()
                    .text_color(cx.theme().foreground.muted()),
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
    activity_row(
        SharedString::from(format!("chat-record-{}", request.id.0)),
        ActivityMark::plain(IconName::CheckRead),
        format!("Asked {count} {noun} · {answers}").into(),
        ActivityEnd::None,
        cx,
    )
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ala::{
        Context, Entity, Image, IntoElement, Render, RenderImage, Subscription, TestAppContext,
        VisualTestContext, Window, div, prelude::*, px,
    };
    use zz_protocol::agent_stream::AgentImage;
    use zz_protocol::chat::{ChatEvent, ItemBody, ItemId, ItemKind, ItemStatus, TurnBy, TurnId};

    use super::{ChatClick, ChatHooks, ChatOpen, ChatPane, ChatView, chat_thread};

    const SHOT: &[u8] = include_bytes!("../fixtures/wide-screenshot.png");

    fn shot(tag: u8) -> Vec<u8> {
        let mut data = SHOT.to_vec();
        data.push(tag);
        data
    }

    fn pasted(view: &mut ChatView, id: &str, text: &str) {
        pasted_shot(view, id, text, SHOT.to_vec());
    }

    fn pasted_shot(view: &mut ChatView, id: &str, text: &str, data: Vec<u8>) {
        view.apply(&ChatEvent::ItemStarted {
            item: ItemId::new(id),
            turn: Some(TurnId(1)),
            parent: None,
            kind: ItemKind::User,
            body: ItemBody {
                text: text.to_owned(),
                images: vec![AgentImage {
                    format: "image/png".to_owned(),
                    data,
                }],
                ..ItemBody::default()
            },
            at: 2,
        })
        .expect("prompt");
        view.apply(&ChatEvent::ItemEnded {
            item: ItemId::new(id),
            status: ItemStatus::Completed,
            at: 2,
        })
        .expect("prompt ends");
    }

    struct ThreadTest {
        view: ChatView,
        open: ChatOpen,
    }

    impl Render for ThreadTest {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div().w(px(420.0)).child(chat_thread(
                &self.view,
                10,
                &self.open,
                0.0,
                &ChatHooks::default(),
                cx,
            ))
        }
    }

    #[test]
    fn pasted_images_are_decoded_once_per_item() {
        let mut pane = ChatPane::default();
        let mut view = ready("s1");
        pasted(&mut view, "u1", "what is this?");
        pane.sync(&view);
        let first = pane.open.images.get(&ItemId::new("u1")).cloned();
        assert_eq!(first.as_ref().map(|images| images.len()), Some(1));
        pane.sync(&view);
        assert!(std::sync::Arc::ptr_eq(
            first.as_ref().expect("cached"),
            pane.open
                .images
                .get(&ItemId::new("u1"))
                .expect("still cached")
        ));
        pane.sync(&ready("s2"));
        assert!(pane.open.images.is_empty());
    }

    #[ala::test]
    fn a_pasted_image_shows_as_a_tile_in_the_bubble(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let mut view = ready("s1");
        pasted(&mut view, "u1", "");
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| ThreadTest {
                view,
                open: ChatOpen::default(),
            });
            penna::Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let tile = cx
            .debug_bounds("chat-user-attachment")
            .expect("the pasted image should be painted");
        assert_eq!(tile.size.width, super::TRANSCRIPT_ATTACHMENT);
        assert_eq!(tile.size.width, tile.size.height);
    }

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

    #[ala::test]
    fn an_evicted_image_drops_its_decoded_pixels(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let mut view = ready("s1");
        pasted(&mut view, "u1", "");
        let mut pane = ChatPane::default();
        pane.sync(&view);
        let image = pane.open.images[&ItemId::new("u1")][0].clone();
        let open = pane.open.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| ThreadTest { view, open });
            penna::Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let render = cx
            .update(|window, cx| image.clone().get_render_image(window, cx))
            .expect("the pasted image should be decoded");
        cx.update(|window, cx| {
            assert!(image.is_asset_cached(cx));
            assert!(window.has_image_atlas_entry(&render));
            let mut again = ready("s2");
            pasted(&mut again, "u1", "");
            pane.sync(&again);
            pane.release(window, cx);
            assert!(
                image.is_asset_cached(cx),
                "the same image shown again stays"
            );
            assert!(window.has_image_atlas_entry(&render));
            pane.sync(&ready("s3"));
            pane.release(window, cx);
            assert!(!image.is_asset_cached(cx));
            assert!(!window.has_image_atlas_entry(&render));
        });
    }

    struct Owner {
        view: ChatView,
        chat: ChatPane,
        _release: Subscription,
    }

    impl Owner {
        fn new(view: ChatView, window: &Window, cx: &mut Context<Self>) -> Self {
            Self {
                view,
                chat: ChatPane::default(),
                _release: ChatPane::close_on_release(window, cx, |owner: &mut Self| {
                    &mut owner.chat
                }),
            }
        }

        fn image(&self, id: &str) -> Arc<Image> {
            self.chat.open.images[&ItemId::new(id)][0].clone()
        }
    }

    impl Render for Owner {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.chat.sync(&self.view);
            self.chat.release(window, cx);
            div().w(px(420.0)).child(chat_thread(
                &self.view,
                10,
                &self.chat.open,
                0.0,
                &ChatHooks::default(),
                cx,
            ))
        }
    }

    struct Owners {
        first: Option<Entity<Owner>>,
        second: Entity<Owner>,
    }

    impl Render for Owners {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .children(self.first.clone())
                .child(self.second.clone())
        }
    }

    fn painted(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.run_until_parked();
    }

    fn decoded(cx: &mut VisualTestContext, image: &Arc<Image>) -> Arc<RenderImage> {
        cx.update(|window, cx| image.clone().get_render_image(window, cx))
            .expect("the pasted image should be decoded")
    }

    fn held(cx: &mut VisualTestContext, image: &Arc<Image>, render: &RenderImage) -> (bool, bool) {
        cx.update(|window, cx| {
            (
                image.is_asset_cached(cx),
                window.has_image_atlas_entry(render),
            )
        })
    }

    #[ala::test]
    fn a_ninth_pasted_image_frees_the_oldest_ones_pixels(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let mut view = ready("s1");
        pasted_shot(&mut view, "u0", "", shot(0));
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let captured = slot.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let owner = cx.new(|cx| Owner::new(view, window, cx));
            captured.replace(Some(owner.clone()));
            penna::Root::new(owner, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        let owner = slot.borrow_mut().take().expect("owner");
        painted(cx);
        let oldest = cx.update(|_, cx| owner.read(cx).image("u0"));
        let oldest_pixels = decoded(cx, &oldest);
        assert_eq!(held(cx, &oldest, &oldest_pixels), (true, true));
        for index in 1..9 {
            cx.update(|_, cx| {
                owner.update(cx, |owner, cx| {
                    pasted_shot(&mut owner.view, &format!("u{index}"), "", shot(index));
                    cx.notify();
                });
            });
            painted(cx);
        }
        let newest = cx.update(|_, cx| owner.read(cx).image("u8"));
        let newest_pixels = decoded(cx, &newest);
        let items = cx.update(|_, cx| {
            let mut items = owner
                .read(cx)
                .chat
                .open
                .images
                .keys()
                .map(|id| id.as_str().to_owned())
                .collect::<Vec<_>>();
            items.sort();
            items
        });
        assert_eq!(
            items,
            (1..9).map(|index| format!("u{index}")).collect::<Vec<_>>()
        );
        assert_eq!(
            held(cx, &oldest, &oldest_pixels),
            (false, false),
            "the ninth image frees the first one's pixels"
        );
        assert_eq!(held(cx, &newest, &newest_pixels), (true, true));
    }

    #[ala::test]
    fn closing_a_chat_frees_its_images_but_not_one_another_chat_shows(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let mut first = ready("s1");
        pasted_shot(&mut first, "both", "", shot(1));
        pasted_shot(&mut first, "own", "", shot(2));
        let mut second = ready("s2");
        pasted_shot(&mut second, "both", "", shot(1));
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let captured = slot.clone();
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let owners = cx.new(|cx| Owners {
                first: Some(cx.new(|cx| Owner::new(first, window, cx))),
                second: cx.new(|cx| Owner::new(second, window, cx)),
            });
            captured.replace(Some(owners.clone()));
            penna::Root::new(owners, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        let owners = slot.borrow_mut().take().expect("owners");
        painted(cx);
        let (shared, own) = cx.update(|_, cx| {
            let first = owners.read(cx).first.clone().expect("first chat");
            let first = first.read(cx);
            (first.image("both"), first.image("own"))
        });
        let shared_pixels = decoded(cx, &shared);
        let own_pixels = decoded(cx, &own);
        assert_eq!(held(cx, &shared, &shared_pixels), (true, true));
        assert_eq!(held(cx, &own, &own_pixels), (true, true));
        cx.update(|_, cx| {
            owners.update(cx, |owners, cx| {
                owners.first = None;
                cx.notify();
            });
        });
        painted(cx);
        assert_eq!(
            held(cx, &own, &own_pixels),
            (false, false),
            "the closed chat's own image is freed"
        );
        assert_eq!(
            held(cx, &shared, &shared_pixels),
            (true, true),
            "the other chat still shows the shared image"
        );
    }

    #[ala::test]
    fn closing_the_window_frees_its_chats_images(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let mut view = ready("s1");
        pasted_shot(&mut view, "own", "", shot(3));
        let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let captured = slot.clone();
        let (_, window_cx) = cx.add_window_view(move |window, cx| {
            let owner = cx.new(|cx| Owner::new(view, window, cx));
            captured.replace(Some(owner.downgrade()));
            penna::Root::new(owner, window, cx)
        });
        let window_cx: &mut VisualTestContext = window_cx;
        let owner = slot.borrow_mut().take().expect("owner");
        painted(window_cx);
        let image = window_cx.update(|_, cx| owner.upgrade().expect("owner").read(cx).image("own"));
        window_cx.update(|window, cx| {
            assert!(image.is_asset_cached(cx));
            window.remove_window();
        });
        cx.run_until_parked();
        assert!(owner.upgrade().is_none(), "the window took its chat along");
        cx.update(|cx| assert!(!image.is_asset_cached(cx)));
    }

    #[ala::test]
    fn an_opened_thought_scrolls_inside_a_capped_height(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let mut view = ready("s1");
        let long = (0..200)
            .map(|line| format!("Line {line} of a long thought."))
            .collect::<Vec<_>>()
            .join("\n\n");
        view.apply(&ChatEvent::ItemStarted {
            item: ItemId::new("t1"),
            turn: Some(TurnId(1)),
            parent: None,
            kind: ItemKind::Reasoning,
            body: ItemBody::text(long),
            at: 3,
        })
        .expect("thought");
        let open = ChatOpen {
            runs: vec![ItemId::new("t1")],
            ..ChatOpen::default()
        };
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|_| ThreadTest { view, open });
            penna::Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let thought = cx
            .debug_bounds("chat-thought")
            .expect("the opened thought should be painted");
        assert!(thought.size.height <= px(super::THOUGHT_MAX_HEIGHT));
        assert!(thought.size.height > px(40.0));
    }

    #[test]
    fn the_same_item_in_two_panes_keeps_separate_block_scopes() {
        assert_ne!(
            super::block_scope(Some(7), "chat-text-a1"),
            super::block_scope(Some(8), "chat-text-a1")
        );
        assert_eq!(
            super::block_scope(Some(7), "chat-text-a1"),
            super::block_scope(Some(7), "chat-text-a1")
        );
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
