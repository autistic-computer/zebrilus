use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash as _, Hasher as _},
    rc::{Rc, Weak},
    sync::Arc,
    time::Duration,
};

use ala::{
    AnyElement, App, ClipboardItem, Context, Div, FollowMode, Global, Image, ImageFormat,
    IntoElement, ListAlignment, ListSizingBehavior, ListState, ScrollHandle, SharedString,
    Stateful, Subscription, Window, div, list, prelude::*, px, relative,
};
use penna::{
    Card, CardLook, Label, Sizable as _, TextStyle,
    components::{
        button::{ButtonLook, ButtonVariants as _},
        scroll::{ScrollableElement as _, Scrollbar},
    },
};
use zz_client::{
    chat_rows::{self, AgentState, Fold, RunLabel, Unit},
    program::Program,
    window_summary::Attention,
};
use zz_protocol::{
    ViewContent,
    agent_stream::AgentImage,
    chat::{
        Ask, ChatView, ItemId, ItemKind, ItemStatus, RequestId, RequestView, SessionView,
        StepStatus, TurnId, TurnView,
    },
};

use super::preview::{ViewOpener, ViewRequest};
use super::refs::{ChatRefs, RefLinks, ref_links, ref_text};
use super::{
    agent_jump_to_bottom_button, agent_text_view, assistant_markdown_extensions, attachment_tile,
};
use crate::navigation::pane_stack::status_dot;
use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Icon, IconName, StyledExt as _, Tone,
    WindowExt as _,
    button::Button,
    h_flex,
    notification::Notification,
    pane::{PipPart, PipSource, pip_part},
    pip::PipSurface,
    text::TextView,
    v_flex,
};
use penna::blocks::chat::{
    ACTIVITY_DISCLOSURE_SIZE, ActivityEnd, ActivityMark, CHAT_CONTENT_MAX_WIDTH,
    COMPOSER_OUTER_PADDING, TRANSCRIPT_ATTACHMENT, activity_glyph, activity_row, spinner_phase,
    touch_transcript_text, transcript_text,
};
use penna::blocks::chat::{card_chip, card_glyph, card_header, card_title};

#[derive(Clone, Debug, Default)]
pub struct ChatOpen {
    pub turns: Vec<TurnId>,
    pub runs: Vec<ItemId>,
    pub agent: Option<ItemId>,
    pub arrivals: HashMap<ItemId, u64>,
    pub images: HashMap<ItemId, Arc<[Arc<Image>]>>,
    pub texts: HashMap<ItemId, SharedString>,
    pub previews: HashMap<ItemId, SharedString>,
    pub expanded: HashSet<ItemId>,
    pub tall: HashSet<ItemId>,
    pub unfolded: HashSet<ItemId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChatClick {
    Turn(TurnId),
    Run(ItemId),
    Agent(ItemId),
    CloseAgent,
    Fork(ItemId),
    Hold(HeldMessage),
    Unfold(ItemId),
    Tall(ItemId, bool),
    Expand(ItemId),
    Collapse(ItemId, ala::Pixels),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeldMessage {
    pub title: SharedString,
    pub text: SharedString,
    pub at: Option<u64>,
    pub fork: Option<ItemId>,
}

impl HeldMessage {
    fn new(text: &SharedString, at: Option<u64>, fork: Option<ItemId>) -> Self {
        let title = text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default()
            .to_owned();
        Self {
            title: title.into(),
            text: text.clone(),
            at,
            fork,
        }
    }
}

pub type ChatClicks = Rc<dyn Fn(&ChatClick, &mut Window, &mut App)>;

#[derive(Default)]
pub struct ChatHooks<'a> {
    pub click: Option<ChatClicks>,
    pub fork: bool,
    pub markdown: Option<&'a ChatMarkdown>,
    pub vendor: Option<&'static Program>,
    pub owner: Option<u64>,
    pub ground: Option<penna::Ground>,
    pub touch: bool,
}

pub struct ChatMarkdown {
    refs: Arc<ChatRefs>,
    extensions: crate::text::MarkdownExtensions,
}

impl ChatMarkdown {
    #[must_use]
    pub fn new(refs: ChatRefs) -> Self {
        let refs = Arc::new(refs);
        let links = Arc::clone(&refs);
        let extensions = match refs.view.clone() {
            Some(open) => crate::agent::previewing_markdown_extensions(open),
            None => assistant_markdown_extensions(),
        };
        Self {
            extensions: extensions
                .inline_links(move |text, source| ref_links(&links, text, source)),
            refs,
        }
    }

    fn reads(&self, refs: &ChatRefs) -> bool {
        self.refs.hosts == refs.hosts && self.refs.cwd == refs.cwd
    }
}

impl ChatHooks<'_> {
    fn refs(&self) -> Option<&ChatRefs> {
        self.markdown.map(|markdown| &*markdown.refs)
    }

    fn clickable<E: StatefulInteractiveElement + Styled>(
        &self,
        element: E,
        target: ChatClick,
    ) -> E {
        match self.click.clone() {
            Some(click) => element
                .click_cursor()
                .on_click(move |_, window, cx| click(&target, window, cx)),
            None => element,
        }
    }

    fn hold(&self, held: HeldMessage) -> Option<AnyElement> {
        let click = self.click.clone().filter(|_| self.touch)?;
        Some(
            penna::blocks::compact::hold_listener(Rc::new(move |window, cx| {
                click(&ChatClick::Hold(held.clone()), window, cx);
            }))
            .into_any_element(),
        )
    }

    fn fork_from(&self, id: &ItemId) -> Option<(ChatClicks, ItemId)> {
        self.click
            .clone()
            .filter(|_| self.fork)
            .filter(|_| zz_client::agent_settings::source_message(id.as_str()).is_some())
            .map(|click| (click, id.clone()))
    }
}

const REVEAL_MAX_MS: u64 = 900;
const REVEAL_MIN_MS: u64 = 220;
const REVEAL_MS_PER_CHAR: u64 = 2;
const USER_FOLD_HEIGHT: f32 = 176.0;
const USER_FOLD_SLACK: f32 = 48.0;
const USER_FADE_HEIGHT: f32 = 64.0;
const THOUGHT_MAX_HEIGHT: f32 = 260.0;
const PIP_GAP: f32 = 10.0;
const PREVIEW_MAX_BYTES: usize = 32 * 1024;
const PREVIEW_MAX_LINES: usize = 512;
const PREVIEW_CUT_BYTES: usize = 8 * 1024;
const PREVIEW_CUT_LINES: usize = 64;
const PREVIEW_HEIGHT: f32 = 420.0;
const PREVIEW_FADE_HEIGHT: f32 = 96.0;
const PIP_MIN_WIDTH: f32 = 280.0;
const PIP_MAX_WIDTH: f32 = 520.0;
const TOUCH_GUTTER: f32 = 16.0;
const JUMP_GAP: f32 = 8.0;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Row {
    Background(Unit),
    Turn(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RowKey {
    Turn(TurnId),
    Item(ItemId),
    Request(RequestId),
    Empty,
}

impl Row {
    fn key(&self, view: &ChatView) -> RowKey {
        match self {
            Self::Turn(index) => view
                .turns
                .get(*index)
                .map_or(RowKey::Empty, |turn| RowKey::Turn(turn.id)),
            Self::Background(unit) => match unit {
                Unit::User(id)
                | Unit::Text(id)
                | Unit::Subagent(id)
                | Unit::Plan(id)
                | Unit::Notice(id) => RowKey::Item(id.clone()),
                Unit::Run(run) => run
                    .items
                    .first()
                    .map_or(RowKey::Empty, |id| RowKey::Item(id.clone())),
                Unit::Agents(ids) => ids
                    .first()
                    .map_or(RowKey::Empty, |id| RowKey::Item(id.clone())),
                Unit::Ask(request) | Unit::Record(request) => RowKey::Request(*request),
            },
        }
    }
}

fn holds(unit: &Unit, id: &ItemId) -> bool {
    match unit {
        Unit::User(held)
        | Unit::Text(held)
        | Unit::Subagent(held)
        | Unit::Plan(held)
        | Unit::Notice(held) => held == id,
        Unit::Run(run) => run.items.contains(id),
        Unit::Agents(ids) => ids.contains(id),
        Unit::Ask(_) | Unit::Record(_) => false,
    }
}

fn thread_rows(view: &ChatView) -> Vec<Row> {
    let mut pending = chat_rows::background_rows(view).into_iter().peekable();
    let mut rows = Vec::new();
    for (index, turn) in view.turns.iter().enumerate() {
        while let Some(unit) = pending
            .next_if(|unit| chat_rows::unit_at(view, unit).is_some_and(|at| at < turn.started_at))
        {
            rows.push(Row::Background(unit));
        }
        rows.push(Row::Turn(index));
    }
    rows.extend(pending.map(Row::Background));
    rows
}

#[derive(Clone, Default)]
pub struct ChatFrame {
    pub now: u64,
    pub phase: f32,
    pub click: Option<ChatClicks>,
    pub fork: bool,
    pub markdown: Option<Rc<ChatMarkdown>>,
    pub vendor: Option<&'static Program>,
    pub owner: Option<u64>,
    pub ground: Option<penna::Ground>,
    pub touch: bool,
}

impl ChatFrame {
    fn hooks(&self) -> ChatHooks<'_> {
        ChatHooks {
            click: self.click.clone(),
            fork: self.fork,
            markdown: self.markdown.as_deref(),
            vendor: self.vendor,
            owner: self.owner,
            ground: self.ground,
            touch: self.touch,
        }
    }
}

pub struct ChatPane {
    pub open: ChatOpen,
    list: ListState,
    rows: Vec<Row>,
    keys: Vec<RowKey>,
    background_live: bool,
    frame: ChatFrame,
    jump: bool,
    session: Option<String>,
    known: HashSet<ItemId>,
    primed: bool,
    retired: Vec<Arc<Image>>,
    shown: Rc<RefCell<HashSet<u64>>>,
    markdown: Option<Rc<ChatMarkdown>>,
    listed: bool,
}

impl Default for ChatPane {
    fn default() -> Self {
        let list = ListState::new(0, ListAlignment::Top, px(1_200.0));
        list.set_follow_mode(FollowMode::Tail);
        Self {
            open: ChatOpen::default(),
            list,
            rows: Vec::new(),
            keys: Vec::new(),
            background_live: false,
            frame: ChatFrame::default(),
            jump: false,
            session: None,
            known: HashSet::new(),
            primed: false,
            retired: Vec::new(),
            shown: Rc::default(),
            markdown: None,
            listed: false,
        }
    }
}

#[derive(Default)]
struct ShownImages(Vec<Weak<RefCell<HashSet<u64>>>>);

impl Global for ShownImages {}

impl ChatPane {
    pub fn markdown(&mut self, refs: ChatRefs) -> Rc<ChatMarkdown> {
        match &self.markdown {
            Some(markdown) if markdown.reads(&refs) => Rc::clone(markdown),
            _ => {
                let markdown = Rc::new(ChatMarkdown::new(refs));
                self.markdown = Some(Rc::clone(&markdown));
                markdown
            }
        }
    }

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
            ChatClick::Fork(_) | ChatClick::Hold(_) => return,
            ChatClick::Unfold(item) => {
                if !self.open.unfolded.remove(item) {
                    self.open.unfolded.insert(item.clone());
                }
            }
            ChatClick::Tall(item, tall) => {
                if *tall {
                    self.open.tall.insert(item.clone());
                } else {
                    self.open.tall.remove(item);
                }
                return;
            }
            ChatClick::Expand(item) => {
                self.open.expanded.insert(item.clone());
            }
            ChatClick::Collapse(item, shrink) => {
                self.open.expanded.remove(item);
                self.list.pause_following_tail();
                self.list.scroll_by(-*shrink);
                return;
            }
        }
        self.list.pause_following_tail();
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
            self.rows.clear();
            self.keys.clear();
            self.list.reset(0);
            self.follow();
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
        self.sync_texts(view);
        self.sync_rows(view);
    }

    fn sync_texts(&mut self, view: &ChatView) {
        let texts = &mut self.open.texts;
        let previews = &mut self.open.previews;
        let expanded = &mut self.open.expanded;
        texts.retain(|id, _| view.item(id).is_some());
        previews.retain(|id, _| texts.contains_key(id));
        for item in view.items.values() {
            if !matches!(item.kind, ItemKind::User | ItemKind::Agent) {
                continue;
            }
            let fresh = texts.get(&item.id).is_some_and(|held| {
                held.len() == item.body.text.len()
                    && (!item.status.is_open() || held.as_ref() == item.body.text)
            });
            if fresh {
                continue;
            }
            texts.insert(item.id.clone(), item.body.text.clone().into());
            match preview(&item.body.text).filter(|_| item.kind == ItemKind::Agent) {
                Some(cut) => {
                    previews.insert(item.id.clone(), cut.into());
                }
                None => {
                    previews.remove(&item.id);
                }
            }
        }
        expanded.retain(|id| previews.contains_key(id));
    }

    fn sync_rows(&mut self, view: &ChatView) {
        let rows = thread_rows(view);
        let keys = rows.iter().map(|row| row.key(view)).collect::<Vec<_>>();
        let kept = self
            .keys
            .iter()
            .zip(&keys)
            .take_while(|(held, key)| held == key)
            .count();
        if kept < self.keys.len() || kept < keys.len() {
            self.list.splice(kept..self.keys.len(), keys.len() - kept);
        }
        self.rows = rows;
        self.keys = keys;
        self.background_live = chat_rows::background_live(view);
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

    pub fn follow(&mut self) {
        self.list.set_follow_mode(FollowMode::Tail);
    }

    pub fn scroll_to_top(&mut self) {
        self.list.scroll_to(ala::ListOffset::default());
    }

    pub fn reveal(&mut self, view: &ChatView, item: &ItemId) {
        let mut root = item.clone();
        while let Some(parent) = view.item(&root).and_then(|found| found.parent.clone()) {
            root = parent;
        }
        let Some(found) = view.item(&root) else {
            return;
        };
        let Some(index) = self.rows.iter().position(|row| match row {
            Row::Turn(index) => view
                .turns
                .get(*index)
                .is_some_and(|turn| Some(turn.id) == found.turn),
            Row::Background(unit) => holds(unit, &root),
        }) else {
            return;
        };
        if let Some(Row::Turn(index)) = self.rows.get(index)
            && let Some(turn) = view.turns.get(*index)
        {
            let rows = chat_rows::turn_rows(view, turn, chat_rows::now_ms());
            if rows.folded.iter().any(|unit| holds(unit, &root))
                && !self.open.turns.contains(&turn.id)
            {
                self.open.turns.push(turn.id);
            }
            let run = rows
                .shown
                .iter()
                .chain(&rows.folded)
                .find_map(|unit| match unit {
                    Unit::Run(run) if run.items.contains(&root) => run.items.first().cloned(),
                    _ => None,
                });
            if let Some(run) = run
                && !self.open.runs.contains(&run)
            {
                self.open.runs.push(run);
            }
        }
        if root != *item && found.kind == ItemKind::Subagent {
            self.open.agent = Some(root);
        }
        self.list.pause_following_tail();
        self.list.scroll_to_reveal_item(index);
    }

    #[must_use]
    pub fn away(&self) -> bool {
        !self.list.is_following_tail()
    }

    #[must_use]
    pub fn shows_jump(&self) -> bool {
        self.away() && self.list.is_scrolled_to_end() == Some(false)
    }

    fn draw<'a, 'h>(
        &'a self,
        view: &'a ChatView,
        hooks: &'a ChatHooks<'h>,
        cx: &'a App,
    ) -> Draw<'a, 'h> {
        Draw {
            view,
            now: self.frame.now,
            open: &self.open,
            phase: self.frame.phase,
            hooks,
            cx,
        }
    }

    fn row(&self, index: usize, view: &ChatView, hooks: &ChatHooks<'_>, cx: &App) -> AnyElement {
        let draw = self.draw(view, hooks, cx);
        let body = match self.rows.get(index) {
            Some(Row::Turn(turn)) => view.turns.get(*turn).map(|turn| draw.turn(turn)),
            Some(Row::Background(unit)) => Some(draw.unit(unit, self.background_live)),
            None => None,
        };
        let gutter = if hooks.touch {
            px(TOUCH_GUTTER)
        } else {
            px(12.0)
        };
        div()
            .w_full()
            .px(gutter)
            .pb(gutter)
            .child(
                div()
                    .w_full()
                    .max_w(px(CHAT_CONTENT_MAX_WIDTH))
                    .mx_auto()
                    .children(body),
            )
            .into_any_element()
    }
}

pub trait ChatHost: Sized + 'static {
    fn chat_pane(&mut self) -> &mut ChatPane;
    fn chat_parts<'a>(&'a mut self, cx: &'a App) -> (&'a mut ChatPane, Option<&'a ChatView>);
    fn chat_read<'a>(&'a self, cx: &'a App) -> (&'a ChatPane, Option<&'a ChatView>);
    fn chat_fork(&mut self, item: &ItemId, cx: &mut Context<Self>);
    fn chat_hold(&mut self, _: &HeldMessage, _: &mut Context<Self>) {}
}

pub struct ChatLook {
    pub pane: u64,
    pub fork: bool,
    pub refs: Option<ChatRefs>,
    pub vendor: Option<&'static Program>,
    pub ground: Option<penna::Ground>,
    pub touch: bool,
}

pub fn chat_surface<T: ChatHost>(
    host: &mut T,
    look: ChatLook,
    window: &mut Window,
    cx: &mut Context<T>,
) -> AnyElement {
    let entity = cx.entity();
    let now = chat_rows::now_ms();
    let live = {
        let (pane, view) = host.chat_parts(cx);
        let Some(view) = view else {
            return div().into_any_element();
        };
        pane.sync(view);
        chat_live(view) || pane.revealing(now)
    };
    let phase = if live {
        spinner_phase(cx.entity_id(), cx)
    } else {
        0.0
    };
    let shows_agents = look.refs.as_ref().and_then(|refs| refs.view.clone());
    let click: ChatClicks = {
        let entity = entity.clone();
        Rc::new(move |click, window, cx| {
            if let (ChatClick::Agent(item), Some(open)) = (click, shows_agents.as_ref()) {
                let title = entity
                    .read(cx)
                    .chat_read(cx)
                    .1
                    .and_then(|view| view.item(item))
                    .map_or_else(|| "Subagent".to_owned(), agent_title);
                open(
                    ViewRequest {
                        title,
                        content: ViewContent::Subagent {
                            item: item.as_str().to_owned(),
                        },
                    },
                    window,
                    cx,
                );
                return;
            }
            entity.update(cx, |host, cx| {
                match click {
                    ChatClick::Fork(item) => host.chat_fork(item, cx),
                    ChatClick::Hold(held) => host.chat_hold(held, cx),
                    _ => {}
                }
                host.chat_pane().click(click);
                cx.notify();
            });
        })
    };
    let pane = host.chat_pane();
    let markdown = look.refs.map(|refs| pane.markdown(refs));
    pane.frame = ChatFrame {
        now,
        phase,
        click: Some(click),
        fork: look.fork,
        markdown,
        vendor: look.vendor,
        owner: Some(look.pane),
        ground: look.ground,
        touch: look.touch,
    };
    pane.jump = pane.shows_jump();
    pane.release(window, cx);
    let list_state = pane.list.clone();
    let show_jump = pane.jump;
    let watched = entity.downgrade();
    list_state.set_scroll_handler(move |_, _, cx| {
        let watched = watched.clone();
        cx.defer(move |cx| {
            watched
                .update(cx, |host, cx| {
                    let pane = host.chat_pane();
                    if pane.jump != pane.shows_jump() {
                        cx.notify();
                    }
                })
                .ok();
        });
    });
    let docked = !crate::touch::CoarsePointer::get(cx);
    let touch = look.touch;
    let pip = {
        let (pane, view) = host.chat_read(cx);
        view.and_then(|view| {
            let hooks = pane.frame.hooks();
            chat_agent_pip(view, now, &pane.open, phase, &hooks, cx)
        })
    };
    let rows = entity.clone();
    let thread = list(list_state.clone(), move |index, _, cx| {
        let host = rows.read(cx);
        let (pane, view) = host.chat_read(cx);
        let Some(view) = view else {
            return div().into_any_element();
        };
        let hooks = pane.frame.hooks();
        pane.row(index, view, &hooks, cx)
    })
    .with_sizing_behavior(ListSizingBehavior::Auto)
    .size_full()
    .pt_3()
    .pb(px(if touch { 0.0 } else { COMPOSER_OUTER_PADDING }));
    let jump = {
        let entity = entity.clone();
        move |_: &ala::ClickEvent, _: &mut Window, cx: &mut App| {
            entity.update(cx, |host, cx| {
                host.chat_pane().follow();
                cx.notify();
            });
            cx.stop_propagation();
        }
    };
    let column = div()
        .relative()
        .flex_1()
        .min_w_0()
        .h_full()
        .text_color(cx.theme().foreground)
        .child(thread)
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom(px(COMPOSER_OUTER_PADDING))
                .child(Scrollbar::vertical(&list_state)),
        )
        .when(show_jump, |column| {
            let button = if touch {
                penna::blocks::chat::phone_jump_button(
                    ("agent-chat-jump", look.pane),
                    "Jump to latest",
                    look.ground.unwrap_or_else(|| cx.theme().ground()),
                )
            } else {
                agent_jump_to_bottom_button(("agent-chat-jump", look.pane), cx)
            };
            column.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(if touch {
                        JUMP_GAP
                    } else {
                        2.0 * COMPOSER_OUTER_PADDING
                    }))
                    .flex()
                    .justify_center()
                    .child(button.on_click(jump)),
            )
        });
    let pip = pip.map(|pip| chat_pip_frame(pip, docked));
    h_flex()
        .id(("agent-chat", look.pane))
        .relative()
        .size_full()
        .items_start()
        .child(column)
        .children(pip)
        .into_any_element()
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

pub fn chat_pip_frame(pip: AnyElement, docked: bool) -> AnyElement {
    if docked {
        div()
            .flex_none()
            .h_full()
            .w(relative(0.45))
            .min_w(px(PIP_MIN_WIDTH))
            .max_w(px(PIP_MAX_WIDTH))
            .pt(px(PIP_GAP))
            .pr(px(PIP_GAP))
            .pb(px(COMPOSER_OUTER_PADDING))
            .child(pip)
            .into_any_element()
    } else {
        div()
            .absolute()
            .inset_0()
            .p(px(PIP_GAP))
            .child(pip)
            .into_any_element()
    }
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
    v_flex()
        .w_full()
        .gap_3()
        .children(thread_rows(view).iter().filter_map(|row| match row {
            Row::Turn(index) => view.turns.get(*index).map(|turn| draw.turn(turn)),
            Row::Background(unit) => Some(draw.unit(unit, live)),
        }))
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
                    fold_row(turn.id, fold, expanded, self.hooks.touch, self.cx),
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
                        Unit::Text(id) => Some(self.text(id)),
                        _ => None,
                    })
                    .filter(|text| !text.trim().is_empty())
                    .collect::<Vec<_>>();
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
                column.when(
                    !self.hooks.touch && (!reply.is_empty() || fork.is_some()),
                    |column| {
                        column.child(message_actions(
                            &format!("turn-{}", turn.id.0),
                            stamp,
                            reply,
                            fork,
                            REPLY_GROUP,
                            self.cx,
                        ))
                    },
                )
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
            Unit::User(id) => {
                let text = self.text(id);
                let at = self.view.item(id).map(|item| item.started_at);
                let fork = self.hooks.fork_from(id);
                let hold = self.hooks.hold(HeldMessage::new(
                    &text,
                    at,
                    fork.as_ref().map(|(_, id)| id.clone()),
                ));
                user_bubble(
                    id,
                    &text,
                    self.images(id).as_deref().unwrap_or_default(),
                    at,
                    fork,
                    self.fold(id),
                    self.hooks.refs(),
                    Bubble {
                        touch: self.hooks.touch,
                        hold,
                    },
                    cx,
                )
            }
            Unit::Text(id) => self.held_reply(id),
            Unit::Run(run) => self.run(run, live),
            Unit::Subagent(id) => self.agents(std::slice::from_ref(id)),
            Unit::Agents(ids) => self.agents(ids),
            Unit::Plan(id) => plan_card(self.view, id, cx),
            Unit::Notice(id) => notice_row(self.view, id, self.hooks.touch, cx),
            Unit::Ask(request) => self.view.request(*request).map_or_else(
                || div().into_any_element(),
                |request| ask_row(self.view, request, self.hooks.touch, cx),
            ),
            Unit::Record(request) => self.view.request(*request).map_or_else(
                || div().into_any_element(),
                |request| record_rows(request, self.hooks.touch, cx),
            ),
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
        mark.touch = self.hooks.touch;
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

    fn text(&self, id: &ItemId) -> SharedString {
        self.open
            .texts
            .get(id)
            .cloned()
            .unwrap_or_else(|| text_of(self.view, id))
    }

    fn held_reply(&self, id: &ItemId) -> AnyElement {
        let reply = self.reply(id);
        if !self.hooks.touch {
            return reply;
        }
        let item = self.view.item(id);
        let fork = item
            .and_then(|item| item.turn)
            .and_then(|turn| {
                self.view
                    .items
                    .values()
                    .find(|other| other.turn == Some(turn) && other.kind == ItemKind::User)
            })
            .and_then(|user| self.hooks.fork_from(&user.id))
            .map(|(_, id)| id);
        let held = HeldMessage::new(&self.text(id), item.map(|item| item.started_at), fork);
        div()
            .relative()
            .w_full()
            .child(reply)
            .children(self.hooks.hold(held))
            .into_any_element()
    }

    fn reply(&self, id: &ItemId) -> AnyElement {
        let Some(cut) = self.open.previews.get(id) else {
            return self.full_reply(id);
        };
        let expanded = self.open.expanded.contains(id);
        let body = if expanded {
            self.full_reply(id)
        } else {
            markdown(id, cut.clone(), false, self.hooks, self.cx)
        };
        let anchored = self.view.item(id).is_some_and(|item| item.parent.is_none());
        long_reply(
            id,
            body,
            expanded,
            anchored,
            self.text(id),
            self.hooks,
            self.cx,
        )
    }

    fn full_reply(&self, id: &ItemId) -> AnyElement {
        let streams = self.streams(id);
        let text = self.shown_text(id);
        let text = if streams && text.len() <= super::MAX_STREAMING_MEND_BYTES {
            crate::mend::mend(&text).map_or(text, SharedString::from)
        } else {
            text
        };
        markdown(id, text, streams, self.hooks, self.cx)
    }

    fn streams(&self, id: &ItemId) -> bool {
        self.open.arrivals.contains_key(id)
            || self.view.item(id).is_some_and(|item| item.status.is_open())
    }

    fn shown_text(&self, id: &ItemId) -> SharedString {
        let text = self.text(id);
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
        mark.touch = self.hooks.touch;
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
                        Some(step_row(
                            self.view,
                            item,
                            self.phase,
                            self.hooks.touch,
                            self.cx,
                        ))
                    } else {
                        thought_row(item, self.hooks.owner, self.hooks.touch, self.cx)
                    }
                }),
        )
    }

    fn fold(&self, id: &ItemId) -> Option<UserFold> {
        self.hooks.click.clone().map(|click| UserFold {
            click,
            tall: self.open.tall.contains(id),
            unfolded: self.open.unfolded.contains(id),
        })
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
        let pane = self.hooks.ground.unwrap_or_else(|| cx.theme().ground());
        let ground = cx
            .theme()
            .ground_of(penna::Surface::from(CardLook::Activity).on(pane));
        let extras = h_flex()
            .flex_none()
            .gap_2()
            .items_center()
            .when(!crate::touch::CoarsePointer::get(cx), |extras| {
                extras
                    .invisible()
                    .group_hover(AGENT_ROW_GROUP, ala::Styled::visible)
            })
            .when_some(ms, |extras, ms| {
                extras.child(
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
        let row = Card::new(
            SharedString::from(format!("chat-agent-{}", id.as_str())),
            CardLook::Activity,
        )
        .group(AGENT_ROW_GROUP)
        .role(ala::Role::Button)
        .aria_label(agent_title(item))
        .when_some(agent_status(state.as_ref()), |row, status| {
            row.aria_description(status)
        })
        .h_flex()
        .w_full()
        .h(px(if self.hooks.touch {
            penna::blocks::touch::TOUCH_TARGET
        } else {
            36.0
        }))
        .p_0()
        .px(px(8.0))
        .gap_2()
        .type_style(
            if self.hooks.touch {
                TextStyle::TouchSmall
            } else {
                TextStyle::Body
            },
            cx,
        )
        .text_color(if open { foreground } else { foreground.muted() })
        .hover(move |row| row.text_color(foreground))
        .child(pip_part(
            PipPart::Icon,
            vendor_tile(
                SharedString::from(format!("chat-agent-tile-{}", id.as_str())),
                self.hooks.vendor,
                state.as_ref(),
                ground,
                cx,
            ),
        ))
        .child(pip_part(
            PipPart::Title,
            div().min_w_0().flex_1().truncate().child(agent_title(item)),
        ))
        .child(extras);
        let asks = if open {
            Vec::new()
        } else {
            asks_under(self.view, id)
        };
        let source = PipSource::new(
            SharedString::from(format!("chat-agent-source-{}", id.as_str())),
            cx.theme().radius_of(CardLook::Activity),
        );
        let opens_view = self.hooks.refs().is_some_and(|refs| refs.view.is_some());
        let launch = source.clone();
        let row = row.when(opens_view, |row| {
            row.on_click(move |_, window, cx| launch.launch(window, cx))
        });
        v_flex()
            .w_full()
            .child(source.draw(self.hooks.clickable(row, ChatClick::Agent(id.clone()))))
            .children(
                asks.into_iter()
                    .map(|request| ask_row(self.view, request, self.hooks.touch, cx)),
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
                let prompt_id = ItemId::new(format!("prompt-{}", id.as_str()));
                body.child(user_bubble(
                    &prompt_id,
                    &prompt.into(),
                    self.images(id).as_deref().unwrap_or_default(),
                    None,
                    None,
                    self.fold(&prompt_id),
                    self.hooks.refs(),
                    Bubble {
                        touch: self.hooks.touch,
                        hold: None,
                    },
                    cx,
                ))
            })
            .children(rows.iter().map(|unit| self.unit(unit, live)))
            .when_some(result.filter(|_| !says), |body, result| {
                body.child(markdown(
                    &ItemId::new(format!("result-{}", id.as_str())),
                    result.into(),
                    false,
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

const AGENT_ROW_GROUP: &str = "chat-agent-row";
const VENDOR_TILE: f32 = 22.0;

fn agent_attention(state: Option<&AgentState>) -> Attention {
    match state {
        Some(AgentState::Working { .. }) => Attention::Working,
        Some(AgentState::Waiting) => Attention::NeedsYou,
        Some(AgentState::Done { .. }) => Attention::Done,
        Some(AgentState::Failed { .. }) => Attention::Failed,
        Some(AgentState::Stopped) | None => Attention::None,
    }
}

fn agent_status(state: Option<&AgentState>) -> Option<&'static str> {
    Some(match state? {
        AgentState::Working { .. } => "Working",
        AgentState::Waiting => "Waiting for you",
        AgentState::Done { .. } => "Done",
        AgentState::Failed { .. } => "Failed",
        AgentState::Stopped => "Stopped",
    })
}

fn vendor_tile(
    id: impl Into<ala::ElementId>,
    vendor: Option<&'static Program>,
    state: Option<&AgentState>,
    ground: penna::Ground,
    cx: &App,
) -> Stateful<Div> {
    let size = px(VENDOR_TILE);
    let tile = match vendor.filter(|_| crate::program::program_colors(cx)) {
        Some(program) => crate::program::program_tile(program, size, 0.0, cx),
        None => div()
            .flex_none()
            .size(size)
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .bg(cx.theme().paint(Tone::Neutral, Emphasis::Soft).fill)
            .child(
                vendor
                    .and_then(|program| {
                        crate::program::program_mark(program, px(13.0), cx.theme().foreground, None)
                    })
                    .unwrap_or_else(|| Icon::new(IconName::Bot).size(px(13.0)).into_any_element()),
            ),
    };
    tile.relative()
        .children(status_dot(agent_attention(state), size, ground.fill, cx))
        .id(id)
        .when_some(agent_status(state), |tile, status| {
            tile.role(ala::Role::Image)
                .aria_label(status)
                .tooltip(move |window, cx| crate::tooltip::Tooltip::new(status).build(window, cx))
        })
}

#[must_use]
pub fn chat_agent_body(
    view: &ChatView,
    id: &ItemId,
    now: u64,
    open: &ChatOpen,
    phase: f32,
    hooks: &ChatHooks<'_>,
    cx: &App,
) -> Option<(SharedString, Div)> {
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
    Some((
        agent_title(item).into(),
        draw.agent_body(id, state.as_ref(), ms),
    ))
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
        .child(vendor_tile(
            "chat-agent-pip-tile",
            hooks.vendor,
            state.as_ref(),
            cx.theme().ground(),
            cx,
        ))
        .child(
            Label::new(TextStyle::Small, agent_title(item))
                .strong()
                .truncate()
                .min_w_0()
                .flex_1(),
        )
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
            .size_full()
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

fn fold_row(turn: TurnId, fold: Fold, expanded: bool, touch: bool, cx: &App) -> Stateful<Div> {
    let mut mark = ActivityMark::bare();
    mark.touch = touch;
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

pub(crate) fn stamp_label(at: u64) -> Option<String> {
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
    copy: Vec<SharedString>,
    fork: Option<(ChatClicks, ItemId)>,
    group: &'static str,
    cx: &App,
) -> AnyElement {
    let stamp = stamp.filter(|at| *at > 0).and_then(stamp_label);
    let copy = (!copy.is_empty()).then_some(copy);
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
                    .text_color(penna::blocks::chat::activity_color(cx)),
            )
        })
        .when_some(copy, |row, parts| {
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
                    let text = parts
                        .iter()
                        .map(SharedString::as_ref)
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
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

struct UserFold {
    click: ChatClicks,
    tall: bool,
    unfolded: bool,
}

struct Bubble {
    touch: bool,
    hold: Option<AnyElement>,
}

#[allow(clippy::too_many_arguments)]
fn user_bubble(
    id: &ItemId,
    text: &SharedString,
    images: &[Arc<Image>],
    stamp: Option<u64>,
    fork: Option<(ChatClicks, ItemId)>,
    fold: Option<UserFold>,
    refs: Option<&ChatRefs>,
    bubble: Bubble,
    cx: &App,
) -> AnyElement {
    let key = id.as_str().to_owned();
    let ground = cx.theme().ground_of(CardLook::Bubble);
    let body = refs
        .and_then(|refs| {
            ref_text(
                SharedString::from(format!("chat-user-text-{key}")),
                text,
                refs,
                cx,
            )
        })
        .unwrap_or_else(|| text.clone().into_any_element());
    let actions = (!bubble.touch).then(|| {
        message_actions(
            &format!("user-{key}"),
            stamp,
            if text.is_empty() {
                Vec::new()
            } else {
                vec![text.clone()]
            },
            fork,
            USER_GROUP,
            cx,
        )
    });
    v_flex()
        .id(SharedString::from(format!("chat-user-{key}")))
        .group(USER_GROUP)
        .w_full()
        .items_end()
        .child(
            div()
                .relative()
                .max_w(relative(if bubble.touch { 0.85 } else { 0.8 }))
                .children(actions.map(|actions| {
                    div()
                        .absolute()
                        .right(relative(1.0))
                        .bottom_0()
                        .pr_1()
                        .child(actions)
                }))
                .child(user_card(
                    id,
                    text,
                    body,
                    images,
                    fold,
                    refs.and_then(|refs| refs.view.as_ref()),
                    ground,
                    bubble.touch,
                    cx,
                ))
                .children(bubble.hold),
        )
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn user_card(
    id: &ItemId,
    text: &SharedString,
    body: AnyElement,
    images: &[Arc<Image>],
    fold: Option<UserFold>,
    view: Option<&ViewOpener>,
    ground: penna::Ground,
    touch: bool,
    cx: &App,
) -> AnyElement {
    let key = id.as_str();
    let card = Card::new("chat-user-bubble", CardLook::Bubble)
        .v_flex()
        .gap_2();
    let card = if touch {
        touch_transcript_text(card.px(px(14.0)).py(px(10.0)), cx)
    } else {
        transcript_text(card.px_3().py_2(), cx)
    };
    card.when(!images.is_empty(), |bubble| {
        bubble.child(
            h_flex()
                .flex_wrap()
                .gap_1()
                .children(images.iter().enumerate().map(|(index, image)| {
                    let tile = (
                        SharedString::from(format!("chat-user-attachment-{key}")),
                        index,
                    );
                    match view {
                        Some(open) => {
                            let open = Arc::clone(open);
                            let request = ViewRequest {
                                title: image_title(index, images.len()),
                                content: ViewContent::Image {
                                    item: key.to_owned(),
                                    index: u32::try_from(index).unwrap_or(u32::MAX),
                                },
                            };
                            let source = PipSource::new(
                                (
                                    SharedString::from(format!(
                                        "chat-user-attachment-source-{key}"
                                    )),
                                    index,
                                ),
                                cx.theme().radius_of(CardLook::Image),
                            )
                            .picture();
                            let launch = source.clone();
                            source
                                .draw(
                                    penna::blocks::chat::attachment_tile(
                                        tile,
                                        Arc::clone(image),
                                        TRANSCRIPT_ATTACHMENT,
                                        ground,
                                        move |_, window, cx| {
                                            launch.launch(window, cx);
                                            open(request.clone(), window, cx);
                                        },
                                    )
                                    .debug_selector(|| "chat-user-attachment".to_owned()),
                                )
                                .into_any_element()
                        }
                        None => {
                            attachment_tile(tile, Arc::clone(image), TRANSCRIPT_ATTACHMENT, ground)
                                .debug_selector(|| "chat-user-attachment".to_owned())
                                .into_any_element()
                        }
                    }
                })),
        )
    })
    .when(!text.is_empty(), |bubble| {
        bubble.child(user_text(id, body, fold, ground, cx))
    })
    .into_any_element()
}

fn image_title(index: usize, count: usize) -> String {
    if count > 1 {
        format!("Image {} of {count}", index + 1)
    } else {
        "Image".to_owned()
    }
}

struct FoldFade {
    child: AnyElement,
    fade: Option<ala::Pixels>,
}

impl FoldFade {
    fn range(&self, bounds: ala::Bounds<ala::Pixels>) -> Option<(ala::Pixels, ala::Pixels)> {
        self.fade
            .map(|fade| (bounds.bottom() - fade, bounds.bottom()))
    }
}

impl IntoElement for FoldFade {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl ala::Element for FoldFade {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ala::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&ala::GlobalElementId>,
        _: Option<&ala::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (ala::LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&ala::GlobalElementId>,
        _: Option<&ala::InspectorElementId>,
        bounds: ala::Bounds<ala::Pixels>,
        (): &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let fade = self.range(bounds);
        window.with_text_fade_along(ala::Axis::Vertical, fade, |window| {
            self.child.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _: Option<&ala::GlobalElementId>,
        _: Option<&ala::InspectorElementId>,
        bounds: ala::Bounds<ala::Pixels>,
        (): &mut Self::RequestLayoutState,
        (): &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let fade = self.range(bounds);
        window.with_text_fade_along(ala::Axis::Vertical, fade, |window| {
            self.child.paint(window, cx);
        });
    }
}

fn user_text(
    id: &ItemId,
    body: AnyElement,
    fold: Option<UserFold>,
    ground: penna::Ground,
    cx: &App,
) -> AnyElement {
    let Some(UserFold {
        click,
        tall,
        unfolded,
    }) = fold
    else {
        return body;
    };
    let folded = tall && !unfolded;
    let measure = {
        let click = Rc::clone(&click);
        let id = id.clone();
        move |bounds: Vec<ala::Bounds<ala::Pixels>>, window: &mut Window, cx: &mut App| {
            let Some(height) = bounds.first().map(|bounds| bounds.size.height) else {
                return;
            };
            let measured = height > px(USER_FOLD_HEIGHT + USER_FOLD_SLACK);
            if measured != tall {
                let click = Rc::clone(&click);
                let id = id.clone();
                window.defer(cx, move |window, cx| {
                    click(&ChatClick::Tall(id, measured), window, cx);
                });
            }
        }
    };
    let clip = div()
        .relative()
        .w_full()
        .flex()
        .flex_col()
        .when(folded, |clip| {
            clip.max_h(px(USER_FOLD_HEIGHT)).overflow_hidden()
        })
        .child(div().flex_none().w_full().child(body))
        .on_children_prepainted(measure);
    let clip = FoldFade {
        child: clip.into_any_element(),
        fade: folded.then_some(px(USER_FADE_HEIGHT)),
    };
    let target = ChatClick::Unfold(id.clone());
    v_flex()
        .w_full()
        .child(clip)
        .when(tall, |text| {
            text.child(
                h_flex().w_full().justify_end().child(
                    Button::new(SharedString::from(format!(
                        "chat-user-unfold-{}",
                        id.as_str()
                    )))
                    .ghost()
                    .small()
                    .on(ground)
                    .label(if unfolded {
                        "Show less"
                    } else {
                        "Show full message"
                    })
                    .text_color(cx.theme().foreground.muted())
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        click(&target, window, cx);
                    }),
                ),
            )
        })
        .into_any_element()
}

fn block_scope(owner: Option<u64>, id: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    owner.hash(&mut hasher);
    id.hash(&mut hasher);
    hasher.finish()
}

fn cut_end(text: &str, bytes: usize, lines: usize) -> usize {
    let mut end = text.len().min(bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if let Some((newline, _)) = text[..end].match_indices('\n').nth(lines - 1) {
        end = newline + 1;
    }
    end
}

fn preview(text: &str) -> Option<String> {
    if cut_end(text, PREVIEW_MAX_BYTES, PREVIEW_MAX_LINES) >= text.len() {
        return None;
    }
    let prefix = &text[..cut_end(text, PREVIEW_CUT_BYTES, PREVIEW_CUT_LINES)];
    Some(crate::mend::mend(prefix).unwrap_or_else(|| prefix.to_owned()))
}

fn long_reply(
    id: &ItemId,
    body: AnyElement,
    expanded: bool,
    anchored: bool,
    full: SharedString,
    hooks: &ChatHooks<'_>,
    cx: &App,
) -> AnyElement {
    let key = id.as_str();
    let height = Rc::new(Cell::new(ala::Pixels::ZERO));
    let body = if expanded {
        let height = Rc::clone(&height);
        div()
            .w_full()
            .child(body)
            .on_children_prepainted(move |bounds, _, _| {
                if let Some(bounds) = bounds.first() {
                    height.set(bounds.size.height);
                }
            })
            .into_any_element()
    } else {
        let clip = ScrollHandle::new();
        let ground = hooks.ground.unwrap_or_else(|| cx.theme().ground());
        div()
            .id(SharedString::from(format!("chat-long-{key}")))
            .relative()
            .w_full()
            .child(
                div()
                    .id(SharedString::from(format!("chat-long-clip-{key}")))
                    .w_full()
                    .flex()
                    .flex_col()
                    .max_h(px(PREVIEW_HEIGHT))
                    .overflow_hidden()
                    .track_scroll(&clip)
                    .child(div().flex_none().w_full().child(body)),
            )
            .bottom_fade(&clip, ground.fill, px(PREVIEW_FADE_HEIGHT))
            .into_any_element()
    };
    let toggle = reply_action(
        SharedString::from(format!("chat-long-toggle-{key}")),
        None,
        if expanded {
            "Show less"
        } else {
            "Show full message"
        },
        Some(if expanded {
            IconName::ChevronUp
        } else {
            IconName::ChevronDown
        }),
        hooks.touch,
        cx,
    )
    .when_some(hooks.click.clone(), |row, click| {
        let id = id.clone();
        row.on_click(move |_, window, cx| {
            cx.stop_propagation();
            let target = if expanded {
                let shrink = if anchored {
                    (height.get() - px(PREVIEW_HEIGHT)).max(ala::Pixels::ZERO)
                } else {
                    ala::Pixels::ZERO
                };
                ChatClick::Collapse(id.clone(), shrink)
            } else {
                ChatClick::Expand(id.clone())
            };
            click(&target, window, cx);
        })
    });
    let copy = reply_action(
        SharedString::from(format!("chat-long-copy-{key}")),
        Some(IconName::Copy),
        "Copy",
        None,
        hooks.touch,
        cx,
    )
    .on_click(move |_, window, cx| {
        cx.stop_propagation();
        cx.write_to_clipboard(ClipboardItem::new_string(full.to_string()));
        window.push_notification(
            Notification::new()
                .message("Copied")
                .autohide_after(Duration::from_millis(1600)),
            cx,
        );
    });
    v_flex()
        .w_full()
        .gap_1()
        .child(body)
        .child(h_flex().gap_4().child(toggle).child(copy))
        .into_any_element()
}

fn reply_action(
    id: SharedString,
    icon: Option<IconName>,
    label: &'static str,
    chevron: Option<IconName>,
    touch: bool,
    cx: &App,
) -> Stateful<Div> {
    let foreground = cx.theme().foreground;
    let mut mark = ActivityMark::bare();
    mark.icon = icon;
    mark.touch = touch;
    activity_row(id, mark, label.into(), ActivityEnd::None, cx)
        .click_cursor()
        .hover(move |row| row.text_color(foreground))
        .when_some(chevron, |row, chevron| {
            row.child(activity_glyph(chevron, ACTIVITY_DISCLOSURE_SIZE, cx))
        })
}

fn markdown(
    id: &ItemId,
    text: SharedString,
    streaming: bool,
    hooks: &ChatHooks<'_>,
    cx: &App,
) -> AnyElement {
    let id = SharedString::from(format!("chat-text-{}", id.as_str()));
    let scope = block_scope(hooks.owner, &id);
    let view = TextView::markdown(id, text);
    let (view, extensions) = match hooks.markdown {
        Some(markdown) => (
            view.links(RefLinks::new(Arc::clone(&markdown.refs))),
            markdown.extensions.clone(),
        ),
        None => (view, assistant_markdown_extensions()),
    };
    let column = if hooks.touch {
        touch_transcript_text(div().w_full(), cx)
    } else {
        transcript_text(div().w_full(), cx)
    };
    column
        .child(
            agent_text_view(view, scope, hooks.owner, cx)
                .selectable(true)
                .streaming(streaming)
                .markdown_extensions(extensions),
        )
        .into_any_element()
}

pub(super) fn kind_icon(kind: ItemKind) -> IconName {
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

fn step_row(
    view: &ChatView,
    item: &zz_protocol::chat::ItemView,
    phase: f32,
    touch: bool,
    cx: &App,
) -> AnyElement {
    let mut mark = ActivityMark::plain(kind_icon(item.kind));
    mark.touch = touch;
    match &item.status {
        ItemStatus::Running => mark.spin = Some(phase),
        ItemStatus::Failed { .. } => mark.tint = Some(cx.theme().danger),
        ItemStatus::Declined => mark.tint = Some(cx.theme().warning),
        _ => {}
    }
    if matches!(item.status, ItemStatus::Failed { .. }) {
        mark.label_color = Some(cx.theme().danger);
    }
    let step = chat_rows::step_label(view, item);
    mark.detail = step.detail.map(SharedString::from);
    activity_row(
        SharedString::from(format!("chat-step-{}", item.id.as_str())),
        mark,
        step.label.into(),
        ActivityEnd::None,
        cx,
    )
    .into_any_element()
}

fn thought_row(
    item: &zz_protocol::chat::ItemView,
    owner: Option<u64>,
    touch: bool,
    cx: &App,
) -> Option<AnyElement> {
    let body = chat_rows::reasoning_body(&item.body.text)?;
    let id = SharedString::from(format!("chat-thought-{}", item.id.as_str()));
    let scope = block_scope(owner, &id);
    let text = if touch {
        touch_transcript_text
    } else {
        transcript_text
    };
    Some(
        text(
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
    let ground = cx.theme().ground_of(CardLook::Activity);
    Card::new(
        SharedString::from(format!("chat-plan-{}", id.as_str())),
        CardLook::Activity,
    )
    .p_0()
    .v_flex()
    .w_full()
    .overflow_hidden()
    .child(
        card_header()
            .child(card_glyph(Icon::new(IconName::List), cx))
            .child(card_title("Plan"))
            .child(card_chip(format!("{done} of {}", item.body.steps.len())).on(ground)),
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

fn notice_row(view: &ChatView, id: &ItemId, touch: bool, cx: &App) -> AnyElement {
    let Some(item) = view.item(id) else {
        return div().into_any_element();
    };
    let mut mark = ActivityMark::plain(if item.kind == ItemKind::Error {
        IconName::TriangleAlert
    } else {
        IconName::Info
    });
    mark.touch = touch;
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

fn ask_row(view: &ChatView, request: &RequestView, touch: bool, cx: &App) -> AnyElement {
    let (icon, label) = match &request.ask {
        Ask::Questions { questions } => (
            IconName::Chat,
            questions
                .first()
                .map(|question| {
                    question
                        .header
                        .clone()
                        .filter(|header| !header.is_empty())
                        .unwrap_or_else(|| question.question.clone())
                })
                .unwrap_or_default(),
        ),
        Ask::Plan { .. } => (IconName::List, "Plan".to_owned()),
        Ask::Approval { title, .. } => (
            request
                .item
                .as_ref()
                .and_then(|id| view.item(id))
                .map_or(IconName::Info, |item| kind_icon(item.kind)),
            title.clone(),
        ),
    };
    let mut mark = ActivityMark::plain(icon);
    mark.touch = touch;
    mark.spread = true;
    mark.detail = Some("waiting for you".into());
    activity_row(
        SharedString::from(format!("chat-ask-{}", request.id.0)),
        mark,
        label.into(),
        ActivityEnd::None,
        cx,
    )
    .into_any_element()
}

fn record_rows(request: &RequestView, touch: bool, cx: &App) -> AnyElement {
    let headers = match &request.ask {
        Ask::Questions { questions } => questions
            .iter()
            .map(|question| question.header.clone().filter(|header| !header.is_empty()))
            .collect(),
        _ => Vec::new(),
    };
    let pairs = chat_rows::answered(request);
    if pairs.is_empty() {
        let mut mark = ActivityMark::plain(IconName::Chat);
        mark.touch = touch;
        mark.detail = Some("skipped".into());
        return activity_row(
            SharedString::from(format!("chat-record-{}", request.id.0)),
            mark,
            "Question".into(),
            ActivityEnd::None,
            cx,
        )
        .into_any_element();
    }
    v_flex()
        .w_full()
        .children(
            pairs
                .into_iter()
                .enumerate()
                .map(|(index, (question, answer))| {
                    let label = headers.get(index).cloned().flatten().unwrap_or(question);
                    let mut mark = ActivityMark::plain(IconName::Chat);
                    mark.touch = touch;
                    mark.spread = true;
                    mark.detail = Some(answer.into());
                    mark.detail_color = Some(cx.theme().foreground);
                    activity_row(
                        (
                            SharedString::from(format!("chat-record-{}", request.id.0)),
                            index,
                        ),
                        mark,
                        label.into(),
                        ActivityEnd::None,
                        cx,
                    )
                }),
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
    use zz_protocol::agent_stream::{AgentImage, AgentQuestion};
    use zz_protocol::chat::{
        Answer, Ask, ChatEvent, ItemBody, ItemId, ItemKind, ItemStatus, QuestionAnswer,
        RequestClose, RequestId, TurnBy, TurnId,
    };

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

    #[ala::test]
    fn the_answer_to_a_long_question_stays_on_its_row(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let mut view = ChatView::default();
        let events = [
            ChatEvent::SessionReady {
                session: "s1".to_owned(),
            },
            ChatEvent::TurnStarted {
                turn: TurnId(1),
                by: TurnBy::Agent,
                at: 1,
            },
            ChatEvent::RequestOpened {
                request: RequestId(4),
                turn: Some(TurnId(1)),
                item: None,
                ask: Ask::Questions {
                    questions: vec![AgentQuestion {
                        id: "q0".to_owned(),
                        question: "Can you have Feb submit a bug report from the affected tab and share the ticket ID? There's no report from that session yet.".to_owned(),
                        ..AgentQuestion::default()
                    }],
                },
                blocking: false,
                at: 2,
            },
            ChatEvent::RequestClosed {
                request: RequestId(4),
                close: RequestClose::Answered {
                    answer: Answer::Questions {
                        answers: vec![QuestionAnswer {
                            question: "q0".to_owned(),
                            answers: vec!["Ticket 4821".to_owned()],
                        }],
                    },
                },
                at: 3,
            },
        ];
        for event in &events {
            view.apply(event).expect("event");
        }
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
        let answer = cx
            .debug_bounds("chat-activity-detail")
            .expect("the answer should be painted");
        assert!(answer.size.width > px(40.0), "{answer:?}");
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
        assert!(pane.away());
        pane.sync(&ready("s2"));
        assert_eq!(pane.open.agent, None);
        assert!(pane.open.turns.is_empty());
        assert!(!pane.away(), "a new session lands on its latest message");
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
        assert!(!pane.away());
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

    fn finished(view: &mut ChatView, item: &str, parent: Option<&str>, kind: ItemKind, at: u64) {
        let body = if kind == ItemKind::Agent {
            ItemBody::text("done")
        } else {
            ItemBody::titled(item)
        };
        view.apply(&ChatEvent::ItemStarted {
            item: ItemId::new(item),
            turn: Some(TurnId(1)),
            parent: parent.map(ItemId::new),
            kind,
            body,
            at,
        })
        .expect("start");
        view.apply(&ChatEvent::ItemEnded {
            item: ItemId::new(item),
            status: ItemStatus::Completed,
            at,
        })
        .expect("end");
    }

    #[test]
    fn a_long_reply_previews_its_first_64_lines() {
        let mut view = ready("s1");
        let long = format!("```\n{}```\n", "line\n".repeat(600));
        view.apply(&ChatEvent::ItemStarted {
            item: ItemId::new("long"),
            turn: Some(TurnId(1)),
            parent: None,
            kind: ItemKind::Agent,
            body: ItemBody::text(long.clone()),
            at: 3,
        })
        .expect("reply");
        finished(&mut view, "short", None, ItemKind::Agent, 4);
        let mut pane = ChatPane::default();
        pane.sync(&view);
        assert_eq!(
            pane.open.previews[&ItemId::new("long")].as_ref(),
            format!("```\n{}", "line\n".repeat(63))
        );
        assert_eq!(pane.open.texts[&ItemId::new("long")].as_ref(), long);
        assert!(!pane.open.previews.contains_key(&ItemId::new("short")));
    }

    #[test]
    fn revealing_a_folded_tool_opens_its_turn_and_run_or_its_agent() {
        let mut view = ready("s1");
        finished(&mut view, "step", Some("a"), ItemKind::Read, 3);
        view.apply(&ChatEvent::ItemEnded {
            item: ItemId::new("a"),
            status: ItemStatus::Completed,
            at: 3,
        })
        .expect("agent ends");
        finished(&mut view, "read", None, ItemKind::Read, 4);
        finished(&mut view, "reply", None, ItemKind::Agent, 5);
        view.apply(&ChatEvent::TurnEnded {
            turn: TurnId(1),
            outcome: zz_protocol::chat::TurnOutcome::Completed {
                stop_reason: "end_turn".to_owned(),
            },
            at: 6,
        })
        .expect("turn ends");
        let mut pane = ChatPane::default();
        pane.sync(&view);
        assert!(!pane.away());

        pane.reveal(&view, &ItemId::new("read"));
        assert_eq!(pane.open.turns, [TurnId(1)]);
        assert_eq!(pane.open.runs, [ItemId::new("read")]);
        assert_eq!(pane.open.agent, None);
        assert!(pane.away(), "a revealed row stays put");

        pane.reveal(&view, &ItemId::new("step"));
        assert_eq!(pane.open.turns, [TurnId(1)]);
        assert_eq!(pane.open.agent, Some(ItemId::new("a")));
    }
}
