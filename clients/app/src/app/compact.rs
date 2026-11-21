use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use ala::{
    AnyElement, App, Bounds, Context, Corners, Div, Entity, Focusable as _, Hsla, IntoElement,
    Keystroke, ParentElement as _, PinchEvent, Pixels, ScrollDelta, ScrollHandle, ScrollWheelEvent,
    SharedString, Stateful, Styled as _, Subscription, TouchPhase, Window, div, point, prelude::*,
    px,
};
use penna::{
    ActiveTheme as _, IconName,
    blocks::compact::{
        PillPlacement, PillSpring, StripItem, StripMark, bar_disc_bounds, bar_ends,
        compact_bar_disc, compact_floating_bar, compact_hud, compact_pill_morph_between, edge_blur,
        morph_bar_ink, over_pill, pill_corner, pill_glass_over, pill_handle, pill_handle_bounds,
        pill_morph_frame, pill_strip, split_pill_bounds,
    },
    blocks::sheet::{bottom_sheet, close_sheet},
    blocks::touch::{
        ArrowPadEvent, KEY_ROW_HEIGHT, KEY_ROW_MARGIN_X, KEY_ROW_MARGIN_Y, KeyRow, KeyRowKey, Lift,
        LiftEvent, Pager, PagerEvent, PhoneFind, StackClose, StackMenu, StackMenuEvent,
        WhichKeyList,
    },
    blocks::which_key::WhichKeyRow,
    components::kbd::Kbd,
};
use web_time::Instant;
use zz_client::{
    StatusBarModel, open_uri::OpenUriRoute, program::Program, window_summary::pane_program,
};
use zz_protocol::{InputMessage, PaneId, PaneKindSnapshot, WindowId};
use zz_terminal::KeyAction;
use zz_ui::{
    command::tmux_keystroke,
    compact::{
        ALL_BINDINGS, ALL_MACHINES, CLOSE_PANE, FIND, MachineMenu, NEW_BROWSER, NEW_CHAT,
        NEW_SESSION, NEW_TERMINAL, RENAME_PANE, SETTINGS, ToolKeys, close_pane_detail,
        machine_menu, new_menu, pane_menu, strip_face,
    },
};

use super::AppShell;
use crate::terminal::{TerminalPane, TerminalText};

mod browser;
mod overview;
pub(crate) use overview::{HostIds, HostPage};
mod sheets;

const PAGE_GAP: f32 = 12.0;
const EDGE_FADE: f32 = 40.0;
const PINCH_STEPS: f32 = 20.0;
const SCALE_HUD_LINGER: Duration = Duration::from_secs(1);
#[cfg(any(target_os = "ios", target_os = "android"))]
const NOTICE_LINGER: Duration = Duration::from_secs(2);
const LANDING_WAIT: Duration = Duration::from_secs(1);
const MAKING_WAIT: Duration = Duration::from_secs(10);
const TOUCH_TARGET: f32 = penna::blocks::touch::TOUCH_TARGET;
const BAR_SWAP: f32 = 8.0;
const MENU_DIM: f32 = 0.45;

const BINDINGS_SHEET: &str = "compact-bindings-sheet";

#[derive(Default)]
pub(super) struct CompactState {
    pager: Pager,
    width: f32,
    tool_keys: Option<(ToolKeys, Vec<Subscription>)>,
    bindings: bool,
    preview: Option<bool>,
    zoom_intent: Option<PaneId>,
    compact_keyboard: Option<bool>,
    last_current: Option<PaneId>,
    last_zoomed: Option<WindowId>,
    pinch: Option<(f32, f32)>,
    scale_hud: Option<Instant>,
    notice: Option<(PaneId, &'static str, PillNotice, Instant)>,
    safe_bottom: Pixels,
    keyboard_overlap: Pixels,
    keyboard_moving: bool,
    extra_height: Pixels,
    pub(super) overview: bool,
    card: Option<overview::MachineCard>,
    card_zoom: Lift,
    held: Option<String>,
    host_pager: Pager,
    shown_host: Option<(usize, zz_ui::command::PaletteHostId)>,
    making: Option<Instant>,
    host_scrolls: BTreeMap<usize, ScrollHandle>,
    hosts_scroll: ScrollHandle,
    host_tiles: Rc<RefCell<BTreeMap<usize, Bounds<Pixels>>>>,
    top_row: Pixels,
    overview_reveal: bool,
    lift: Lift,
    hosts: Lift,
    morph: PillSpring,
    landing: Option<(PaneId, Instant)>,
    entering: Option<(PaneId, Instant)>,
    text: Option<TerminalText>,
    page_background: Option<ala::Hsla>,
    menu: Option<(Entity<StackMenu>, Subscription)>,
    new_menu: Option<(Entity<StackMenu>, Subscription)>,
    sheet: Option<sheets::PhoneSheet>,
    next_sheet: Option<sheets::PhoneSheet>,
    rename: Option<(Entity<penna::components::input::InputState>, Subscription)>,
    chat_dock: Option<super::agent_pane::PhoneDock>,
}

impl CompactState {
    #[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
    pub(super) fn forget_panes(&mut self) {
        self.notice = None;
        self.landing = None;
        self.entering = None;
        self.host_scrolls.clear();
    }

    pub(super) fn reset_attachment(&mut self) {
        self.preview = None;
        self.zoom_intent = None;
        self.last_current = None;
        self.last_zoomed = None;
        self.bindings = std::env::var("ALA_BENCH").is_ok_and(|bench| bench.starts_with("sheet"));
        self.menu = None;
        self.new_menu = None;
        self.tool_keys = None;
        self.close_sheets();
    }

    pub(super) fn close_sheets(&mut self) {
        self.held = None;
        self.sheet = None;
        self.next_sheet = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
pub(super) enum PillNotice {
    Done,
    Hint,
    Failed,
}

fn bar_wash(dim: f32, corner: Pixels, ground: Hsla) -> Option<Div> {
    (dim > 0.0).then(|| {
        div()
            .absolute()
            .inset_0()
            .rounded(corner)
            .bg(ground.alpha(dim))
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
pub(super) struct BarHoles {
    pub pill: (Bounds<Pixels>, Pixels),
    pub disc: Option<(Bounds<Pixels>, Pixels)>,
    pub handle: Bounds<Pixels>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Page {
    pub pane: PaneId,
    pub window: WindowId,
    pub kind: PaneKindSnapshot,
    pub title: SharedString,
    pub attention: bool,
}

pub(super) fn pages(model: &StatusBarModel, unseen: &BTreeSet<PaneId>) -> (Vec<Page>, usize) {
    let mut pages = Vec::new();
    let mut current = 0;
    for window in &model.windows {
        for pane in &window.panes {
            if window.active && pane.active {
                current = pages.len();
            }
            pages.push(Page {
                pane: pane.id,
                window: window.id,
                kind: pane.kind.clone(),
                title: pane.label.clone().into(),
                attention: unseen.contains(&pane.id)
                    || (!window.active && (window.bell || window.activity)),
            });
        }
    }
    (pages, current)
}

pub(super) fn pill_words(
    parts: &[zz_client::window_summary::DetailPart],
) -> (String, String, penna::Tone) {
    use zz_client::window_summary::Tone;
    let rest = |from: usize| {
        parts[from..]
            .iter()
            .map(|part| part.text.as_str())
            .collect::<String>()
    };
    match parts.first() {
        Some(first) if first.tone == Tone::Warning => {
            let word = if first.text == "Question" {
                "Has a question".to_owned()
            } else {
                first.text.clone()
            };
            (word, String::new(), penna::Tone::Warning)
        }
        Some(first) if first.tone == Tone::Danger && first.text == "Failed" => {
            (first.text.clone(), String::new(), penna::Tone::Danger)
        }
        Some(first) if first.tone == Tone::Danger => {
            (first.text.clone(), rest(1), penna::Tone::Danger)
        }
        _ => (String::new(), rest(0), penna::Tone::Neutral),
    }
}

pub(super) fn pinch_font_scale(start: f32, pinch: f32) -> f32 {
    let scale = (start * pinch * PINCH_STEPS).round() / PINCH_STEPS;
    if scale.is_finite() {
        scale.clamp(0.5, 3.0)
    } else {
        start
    }
}

pub(super) const fn kind_icon(kind: &PaneKindSnapshot) -> IconName {
    match kind {
        PaneKindSnapshot::Terminal => IconName::SquareTerminal,
        PaneKindSnapshot::Browser(_) => IconName::Globe,
        PaneKindSnapshot::Editor(_) => IconName::File,
        PaneKindSnapshot::Picker => IconName::Plus,
        PaneKindSnapshot::Agent(_) => IconName::Chat,
        PaneKindSnapshot::View(_) => IconName::Eye,
        PaneKindSnapshot::Other { .. } => IconName::Info,
    }
}

fn dispatch_key(keystroke: Keystroke, window: &mut Window, cx: &mut App) {
    window.defer(cx, move |window, cx| {
        window.dispatch_keystroke(keystroke, cx);
    });
}

fn send_chord(
    _: &mut AppShell,
    _: &Entity<StackMenu>,
    event: &StackMenuEvent,
    window: &mut Window,
    cx: &mut Context<AppShell>,
) {
    let StackMenuEvent::Pick(id) = event;
    if let Some(keystroke) = zz_ui::compact::chip_keystroke(id) {
        let keystroke = penna::blocks::touch::StickyModifiers::apply(&keystroke, cx);
        dispatch_key(keystroke, window, cx);
    }
}

fn key_row(keys: &ToolKeys, lead: Option<KeyRowKey>) -> KeyRow {
    KeyRow::new(keys.row(lead))
        .discs(keys.discs())
        .on_key(|keystroke, window, cx| dispatch_key(keystroke.clone(), window, cx))
        .on_done(|window, _| window.dismiss_virtual_keyboard())
}

fn key_row_lift(zone: Pixels, overlap: Pixels) -> Pixels {
    (zone - px(KEY_ROW_HEIGHT) - overlap).max(px(0.0))
}

fn key_row_top(zone: Pixels, overlap: Pixels) -> Pixels {
    px(KEY_ROW_HEIGHT) + key_row_lift(zone, overlap) + overlap
}

fn screen_radius(window: &Window) -> Option<Pixels> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if ala_platform::mobile::phone() {
        return ala_platform::mobile::display_corner_radius()
            .map(|radius| px(radius / window.zoom()));
    }
    let _ = window;
    None
}

pub(super) fn keyboard_visible(window: &Window) -> bool {
    window.visual_viewport_bounds().size.height + px(1.0) < window.viewport_size().height
}

fn keyboard_heading_up(window: &Window) -> bool {
    window.viewport_size().height - window.target_visual_viewport_bounds().bottom() > px(1.0)
}

fn prefix_spelling(core: &zz_client::ClientCore) -> String {
    core.mux_options()
        .get(zz_protocol::MuxOptionKey::Prefix)
        .filter(|option| !option.value.eq_ignore_ascii_case("none"))
        .map_or_else(
            || "C-b".to_owned(),
            |option| zz_protocol::canonical_key(&option.value),
        )
}

fn live_prefix(options: &zz_protocol::MuxOptions) -> Option<Keystroke> {
    [
        zz_protocol::MuxOptionKey::Prefix,
        zz_protocol::MuxOptionKey::Prefix2,
    ]
    .into_iter()
    .filter_map(|key| options.get(key))
    .find(|option| !option.value.eq_ignore_ascii_case("none"))
    .and_then(|option| tmux_keystroke(&zz_protocol::canonical_key(&option.value)))
}

fn binding_rows(core: &zz_client::ClientCore, prefix: &str) -> Arc<[WhichKeyRow]> {
    zz_client::which_key::action_rows(core.key_tables(), "prefix", prefix)
        .into_iter()
        .filter(|row| tmux_keystroke(row.first_key()).is_some())
        .map(|row| zz_ui::which_key::row(row, tmux_keystroke))
        .collect()
}

impl AppShell {
    pub(super) fn open_machines_when_unattached(&mut self, window: &Window, cx: &App) {
        let connection = self.connection.read(cx);
        if Self::narrow(window)
            && self.settings.is_none()
            && !self.compact.overview
            && !connection.busy_or_connecting()
            && !connection.attaching()
            && self.active_window(cx).is_none()
        {
            self.show_overview(cx);
        }
    }

    pub(super) fn show_overview(&mut self, cx: &App) {
        self.place_overview(cx);
        self.compact.lift.snap(true);
    }

    pub(super) fn compact_active(&self, window: &Window, cx: &App) -> bool {
        Self::narrow(window)
            && self.settings.is_none()
            && (self.compact.overview || self.active_window(cx).is_some())
    }

    pub(super) fn sync_compact_mode(&mut self, compact: bool, cx: &mut Context<Self>) {
        if self.compact.preview != Some(compact) && self.connection.read(cx).connected {
            self.compact.preview = Some(compact);
            self.connection.update(cx, |connection, cx| {
                connection.send(
                    zz_protocol::ProtocolMessage::SetTerminalPreview { enabled: compact },
                    cx,
                );
            });
            if compact {
                self.compact.zoom_intent = self.active_window(cx).map(|window| window.active_pane);
            }
        }
        if self.compact.compact_keyboard != Some(compact) {
            self.compact.compact_keyboard = Some(compact);
            #[cfg(any(target_os = "ios", target_os = "android"))]
            ala_platform::mobile::set_compact_keyboard(compact);
            #[cfg(target_os = "ios")]
            ala_platform::mobile::set_app_key_row(!compact);
        }
        if !compact {
            self.compact.chat_dock = None;
            for agent in self.agents.values() {
                agent.update(cx, |agent, cx| agent.set_phone(None, cx));
            }
            #[cfg(any(target_os = "ios", target_os = "android"))]
            ala_platform::mobile::set_status_bar_on_dark(None);
            if self.compact.pinch.take().is_some() {
                self.terminal_resize_suppressed.set(false);
                self.preferences.save();
            }
        }
    }

    pub(super) fn sync_keyboard_motion(&mut self, window: &mut Window) {
        let height = window.viewport_size().height;
        let overlap = height - window.visual_viewport_bounds().bottom();
        let closing = height - window.target_visual_viewport_bounds().bottom() < overlap;
        let moved = self.compact.keyboard_overlap != overlap;
        if moved {
            self.compact.keyboard_overlap = overlap;
            window.request_animation_frame();
        }
        let holding = moved && !closing;
        if holding {
            self.terminal_resize_suppressed.set(true);
        } else if self.compact.keyboard_moving
            && self.split_drag.is_none()
            && self.compact.pinch.is_none()
            && !self.compact.pager.is_moving()
        {
            self.terminal_resize_suppressed.set(false);
        }
        self.compact.keyboard_moving = holding;
    }

    fn compact_select(&mut self, page: &Page, cx: &mut Context<Self>) {
        let target = page.pane.to_string();
        if self
            .active_window(cx)
            .is_none_or(|window| window.id != page.window)
        {
            self.command("select-window", vec!["-t".into(), target.clone()], cx);
        }
        self.command("select-pane", vec!["-Z".into(), "-t".into(), target], cx);
        self.compact.zoom_intent = Some(page.pane);
        self.focused_pane = None;
    }

    fn commit_page(&mut self, event: Option<PagerEvent>, cx: &mut Context<Self>) {
        let Some(PagerEvent::Commit { index }) = event else {
            return;
        };
        cx.haptic(ala::Haptic::Selection);
        let model = self.status_model(cx);
        let (pages, _) = pages(&model, &self.unseen_agents);
        if let Some(page) = pages.get(index) {
            self.compact_select(page, cx);
        }
    }

    fn follow_compact_page(&mut self, current: Option<PaneId>, cx: &mut Context<Self>) {
        let shown = {
            let core = &self.connection.read(cx).core;
            let snapshot = core.snapshot();
            snapshot
                .sessions
                .iter()
                .find(|session| Some(session.id) == core.attached_session())
                .map(|session| {
                    let id = snapshot.focused_window_for(session);
                    let zoomed = session
                        .windows
                        .iter()
                        .any(|window| window.id == id && window.zoomed_pane.is_some());
                    let watched = session
                        .viewers
                        .iter()
                        .any(|viewer| !viewer.is_self && viewer.window == id);
                    (id, zoomed, watched)
                })
        };
        let left_unzoomed =
            shown.is_some_and(|(id, _, watched)| watched && self.compact.last_zoomed != Some(id));
        if self.compact.last_current.is_some()
            && current != self.compact.last_current
            && !left_unzoomed
        {
            self.compact.zoom_intent = current;
        }
        self.compact.last_current = current;
        self.compact.last_zoomed = shown.and_then(|(id, zoomed, _)| zoomed.then_some(id));
        self.reconcile_compact_zoom(cx);
    }

    fn reconcile_compact_zoom(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.compact.zoom_intent else {
            return;
        };
        let Some(active) = self.active_window(cx) else {
            return;
        };
        if active.active_pane != target {
            return;
        }
        self.compact.zoom_intent = None;
        let connection = self.connection.read(cx);
        if active.panes.len() > 1
            && active.zoomed_pane.is_none()
            && connection.connected
            && !connection.core.attached_read_only()
        {
            self.command(
                "resize-pane",
                vec!["-Z".into(), "-t".into(), target.to_string()],
                cx,
            );
        }
    }

    fn tool_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) -> ToolKeys {
        if let Some((keys, _)) = &self.compact.tool_keys {
            return keys.clone();
        }
        let keys = ToolKeys::new(cx);
        let prefix = keys.prefix.read(cx).menu().clone();
        let chords = keys.chord_menus(cx);
        let mut subscriptions = vec![
            cx.subscribe_in(
                &keys.arrows,
                window,
                |_, _, event: &ArrowPadEvent, window, cx| {
                    let ArrowPadEvent::Arrow(direction) = event;
                    if let Ok(keystroke) = Keystroke::parse(direction.key()) {
                        let keystroke =
                            penna::blocks::touch::StickyModifiers::apply(&keystroke, cx);
                        dispatch_key(keystroke, window, cx);
                    }
                },
            ),
            cx.subscribe_in(&prefix, window, Self::stack_pick),
        ];
        subscriptions.extend(
            chords
                .iter()
                .map(|menu| cx.subscribe_in(menu, window, send_chord)),
        );
        self.compact.tool_keys = Some((keys.clone(), subscriptions));
        keys
    }

    pub(super) fn pill_stack(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<StackMenu> {
        if let Some((menu, _)) = &self.compact.menu {
            return menu.clone();
        }
        let menu = cx.new(|cx| StackMenu::new(StackClose::Disc, cx).scrim_apart());
        let subscription = cx.subscribe_in(&menu, window, Self::stack_pick);
        self.compact.menu = Some((menu.clone(), subscription));
        menu
    }

    pub(super) fn new_stack(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<StackMenu> {
        if let Some((menu, _)) = &self.compact.new_menu {
            return menu.clone();
        }
        let menu = cx.new(|cx| StackMenu::new(StackClose::Disc, cx).scrim_apart());
        let subscription = cx.subscribe_in(&menu, window, Self::stack_pick);
        self.compact.new_menu = Some((menu.clone(), subscription));
        menu
    }

    fn stacks(&self) -> impl Iterator<Item = &Entity<StackMenu>> {
        [&self.compact.menu, &self.compact.new_menu]
            .into_iter()
            .filter_map(|menu| menu.as_ref().map(|(menu, _)| menu))
    }

    #[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
    pub(super) fn shown_stack(&self, cx: &App) -> Option<Entity<StackMenu>> {
        self.stacks().find(|menu| menu.read(cx).is_shown()).cloned()
    }

    pub(super) fn open_pill_menu(&mut self, cx: &mut Context<Self>) {
        if let Some((menu, _)) = &self.compact.menu {
            menu.update(cx, StackMenu::open);
        }
    }

    pub(super) fn open_new_menu(&mut self, cx: &mut Context<Self>) {
        if let Some((menu, _)) = &self.compact.new_menu {
            menu.update(cx, StackMenu::open);
        }
    }

    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    fn close_pill_menu(&mut self, cx: &mut Context<Self>) {
        for menu in self.stacks().cloned().collect::<Vec<_>>() {
            menu.update(cx, StackMenu::close);
        }
        if let Some((keys, _)) = &self.compact.tool_keys {
            keys.clone().close(cx);
        }
    }

    pub(super) fn close_key_stacks(&mut self, cx: &mut Context<Self>) {
        if let Some((keys, _)) = &self.compact.tool_keys {
            keys.clone().close(cx);
        }
    }

    fn shown_terminal(&self, cx: &App) -> Option<Entity<TerminalPane>> {
        let model = self.status_model(cx);
        let (pages, _) = pages(&model, &self.unseen_agents);
        pages
            .get(self.compact.pager.index())
            .filter(|page| page.kind == PaneKindSnapshot::Terminal)
            .and_then(|page| self.terminals.get(&page.pane).cloned())
    }

    fn open_phone_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(terminal) = self.shown_terminal(cx) {
            terminal.update(cx, |terminal, cx| terminal.open_phone_find(window, cx));
            self.focused_pane = None;
            cx.notify();
        }
    }

    fn phone_find_field(
        terminal: &Entity<TerminalPane>,
        state: crate::terminal::PhoneFindState,
        cx: &App,
    ) -> PhoneFind {
        let close = terminal.downgrade();
        let previous = terminal.downgrade();
        let next = terminal.downgrade();
        PhoneFind::new("compact-find", &state.input, cx.theme().ground())
            .count(state.count)
            .found(state.found)
            .on_close(move |window, cx| {
                let _ = close.update(cx, |terminal, cx| terminal.close_find(window, cx));
            })
            .on_previous(move |_, cx| {
                let _ = previous.update(cx, |terminal, cx| terminal.find_step(true, cx));
            })
            .on_next(move |_, cx| {
                let _ = next.update(cx, |terminal, cx| terminal.find_step(false, cx));
            })
    }

    pub(super) fn open_link(
        &mut self,
        link: &crate::connection::OpenLink,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let compact = self.compact_active(window, cx);
        match self.connection.read(cx).uri_route(link.pane, &link.uri) {
            OpenUriRoute::PastedImage { pane, number } => {
                self.connection.update(cx, |connection, cx| {
                    connection.preview_pasted_image(pane, number, cx);
                });
            }
            _ if compact && link.tapped => {
                window.dismiss_virtual_keyboard();
                self.open_phone_sheet(sheets::PhoneSheet::Link(link.uri.clone()), window, cx);
            }
            #[cfg(any(target_os = "ios", target_os = "android"))]
            OpenUriRoute::Embedded { pane, url }
                if !compact && !self.connection.read(cx).core.attached_read_only() =>
            {
                window.dismiss_virtual_keyboard();
                if let Some(browser) = self.browsers.get(&pane).cloned() {
                    browser.update(cx, |browser, cx| browser.load_url(url, window, cx));
                } else {
                    self.connection.update(cx, |connection, cx| {
                        connection.navigate_browser(pane, url, cx);
                    });
                }
            }
            _ if zz_client::terminal_view::openable_uri(&link.uri) => cx.open_url(&link.uri),
            _ => self.connection.update(cx, |connection, cx| {
                connection.notify_error(format!("Cannot open this URI: {}", link.uri), cx);
            }),
        }
    }

    pub(super) fn open_link_in_browser(&mut self, uri: &str, cx: &mut Context<Self>) {
        let mut args = self
            .connection
            .read(cx)
            .core
            .attached_session()
            .map(|session| vec!["-t".to_owned(), format!("{session}:")])
            .unwrap_or_default();
        args.extend(["--kind".to_owned(), "browser".to_owned(), uri.to_owned()]);
        self.command("new-window", args, cx);
    }

    fn stack_pick(
        &mut self,
        _: &Entity<StackMenu>,
        event: &StackMenuEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let StackMenuEvent::Pick(id) = event;
        self.pill_menu_pick(id, window, cx);
    }

    fn pill_menu_pick(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.browser_menu_pick(id, window, cx) {
            return;
        }
        let new = match id {
            NEW_CHAT => Some(overview::NewThing::Chat),
            NEW_TERMINAL => Some(overview::NewThing::Terminal),
            NEW_BROWSER => Some(overview::NewThing::Browser),
            NEW_SESSION => Some(overview::NewThing::Session),
            _ => None,
        };
        if let Some(new) = new {
            let session = if self.compact.overview {
                self.machine_session(cx).map(|(session, _)| session)
            } else {
                self.connection.read(cx).core.attached_session()
            };
            self.create(new, session, window, cx);
            return;
        }
        match id {
            ALL_MACHINES => self.open_hosts(cx),
            SETTINGS => self.open_settings(cx),
            ALL_BINDINGS => {
                self.compact.bindings = true;
                cx.notify();
            }
            RENAME_PANE | CLOSE_PANE => self.pane_sheet(id == RENAME_PANE, window, cx),
            FIND => self.open_phone_find(window, cx),
            _ => {}
        }
    }

    fn pane_sheet(&mut self, rename: bool, window: &mut Window, cx: &mut Context<Self>) {
        let connection = self.connection.read(cx);
        if !connection.connected || connection.core.attached_read_only() {
            return;
        }
        let model = self.status_model(cx);
        let (pages, _) = pages(&model, &self.unseen_agents);
        let Some(page) = pages.get(self.compact.pager.index()).cloned() else {
            return;
        };
        let name = self
            .browser_page(&page, cx)
            .map_or_else(|| page.title.to_string(), |(title, _)| title.to_string());
        let thing = sheets::Thing {
            connection: self.connection.clone(),
            target: sheets::Target::Pane(page.pane),
            closing: close_pane_detail(&page.kind, &name),
            name,
        };
        window.dismiss_virtual_keyboard();
        let sheet = if rename {
            sheets::PhoneSheet::Rename(thing)
        } else {
            sheets::PhoneSheet::Close(thing)
        };
        self.open_phone_sheet(sheet, window, cx);
    }

    fn send_binding(&self, key: &str, cx: &mut Context<Self>) {
        let Some(pane) = self.active_window(cx).map(|window| window.active_pane) else {
            return;
        };
        let core = &self.connection.read(cx).core;
        let mut spellings = Vec::new();
        if !core.prefix_armed() {
            spellings.push(prefix_spelling(core));
        }
        spellings.push(key.to_owned());
        let Some(keystrokes) = spellings
            .iter()
            .map(|spelling| tmux_keystroke(spelling))
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        for keystroke in keystrokes {
            for action in [KeyAction::Press, KeyAction::Release] {
                self.send_input(
                    InputMessage::Key {
                        pane,
                        input: crate::terminal::keystroke_input(&keystroke, action),
                        text_follows: false,
                    },
                    cx,
                );
            }
        }
    }

    fn compact_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.compact.lift.is_dragging() {
            return;
        }
        let delta_x = match event.delta {
            ScrollDelta::Pixels(delta) => f32::from(delta.x),
            ScrollDelta::Lines(delta) => delta.x * 20.0,
        };
        let response = self.compact.pager.scroll(
            delta_x,
            event.touch_phase,
            self.compact.width,
            Instant::now(),
        );
        if !response.consumed {
            return;
        }
        cx.stop_propagation();
        if self.compact.pager.swallows_momentum() {
            window.end_touch_momentum();
        }
        self.terminal_resize_suppressed
            .set(self.compact.pager.is_moving());
        let changed = response.changed || response.event.is_some();
        self.commit_page(response.event, cx);
        if changed {
            cx.notify();
        }
    }

    fn compact_pinch(&mut self, event: &PinchEvent, cx: &mut Context<Self>) {
        match event.phase {
            TouchPhase::Started => {
                let model = self.status_model(cx);
                let (pages, _) = pages(&model, &self.unseen_agents);
                if !pages
                    .get(self.compact.pager.index())
                    .is_some_and(|page| page.kind == PaneKindSnapshot::Terminal)
                {
                    return;
                }
                self.compact.pinch = Some((self.preferences.terminal_font_scale, 1.0));
                self.terminal_resize_suppressed.set(true);
            }
            TouchPhase::Moved => {
                let Some((start, pinch)) = self.compact.pinch.as_mut() else {
                    return;
                };
                *pinch += event.delta;
                let scale = pinch_font_scale(*start, *pinch);
                if scale != self.preferences.terminal_font_scale {
                    self.preferences.terminal_font_scale = scale;
                    cx.set_global(self.preferences.terminal_display());
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if self.compact.pinch.take().is_none() {
                    return;
                }
                self.preferences.save();
                self.terminal_resize_suppressed.set(false);
                self.compact.scale_hud = Some(Instant::now() + SCALE_HUD_LINGER);
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(SCALE_HUD_LINGER).await;
                    this.update(cx, |_, cx| cx.notify()).ok();
                })
                .detach();
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn status_model(&self, cx: &App) -> StatusBarModel {
        let core = &self.connection.read(cx).core;
        StatusBarModel::from_snapshot(core.snapshot(), core.attached_session())
    }

    #[allow(clippy::too_many_arguments)]
    fn compact_page(
        &mut self,
        slot: &Page,
        current: bool,
        motion: f32,
        top_inset: Pixels,
        reserve: Pixels,
        hole: Option<BarHoles>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let _ = hole;
        let radius = cx.theme().radius_of(penna::ContentLook::Pane) * motion;
        let pane_id = slot.pane;
        let snapshot = self
            .connection
            .read(cx)
            .core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&pane_id).cloned());
        let Some(pane) = snapshot else {
            return div().size_full().into_any_element();
        };
        let can_focus = current && self.compact_can_focus(window, cx);
        let dead = zz_client::terminal_view::dead_label(pane.dead, pane.dead_status);
        let mut background = cx.theme().background;
        let mut rows_above = false;
        let mut rows_below = false;
        let content = match &pane.kind {
            PaneKindSnapshot::Agent(_) => {
                let agent = self.agent_entity(pane_id, window, cx);
                let dock = self.compact.chat_dock;
                let ground = cx.theme().ground();
                agent.update(cx, |agent, cx| {
                    agent.set_phone(dock, cx);
                    agent.set_ground(ground, cx);
                    agent.set_header_band(None, cx);
                    agent.set_corner_radii(
                        Corners {
                            top_left: px(0.0),
                            top_right: px(0.0),
                            bottom_left: radius,
                            bottom_right: radius,
                        },
                        cx,
                    );
                });
                if can_focus && self.focused_pane != Some(pane_id) {
                    agent.read(cx).focus_handle(cx).focus(window, cx);
                    self.focused_pane = Some(pane_id);
                }
                agent.into_any_element()
            }
            PaneKindSnapshot::Terminal => {
                let terminal = self.pane_terminal(pane_id, window, cx);
                if can_focus && self.focused_pane != Some(pane_id) {
                    terminal.read(cx).focus_handle(cx).focus(window, cx);
                    self.focused_pane = Some(pane_id);
                }
                terminal.update(cx, |terminal, cx| {
                    terminal.set_compact(true, cx);
                    terminal.set_extra_height(self.compact.extra_height, cx);
                    terminal.set_text_dimmed(false, 1.0, cx);
                    terminal.set_pane_status(dead.clone(), pane.synchronized_input, cx);
                    terminal.set_corner_radii(
                        Corners {
                            top_left: px(0.0),
                            top_right: px(0.0),
                            bottom_left: radius,
                            bottom_right: radius,
                        },
                        cx,
                    );
                    terminal.set_header_band(None, cx);
                    terminal.set_rows_above(true, cx);
                    terminal.set_rows_below(true, cx);
                });
                background = terminal.read(cx).fill(cx);
                rows_above = terminal.read(cx).rows_above_shown(cx);
                rows_below = terminal.read(cx).rows_below_shown(cx);
                if current
                    && !self.compact.pager.is_moving()
                    && let Some(text) = terminal.read(cx).text()
                {
                    self.compact.text = Some(text);
                }
                terminal.into_any_element()
            }
            #[cfg(any(target_os = "ios", target_os = "android"))]
            PaneKindSnapshot::Browser(descriptor) => {
                let browser = self.browser_entity(pane_id, descriptor, window, cx);
                browser.update(cx, |browser, cx| {
                    browser.synchronize(
                        descriptor,
                        current,
                        Corners::all(radius),
                        true,
                        window,
                        cx,
                    );
                    browser.set_pill(reserve, hole, cx);
                });
                background = zz_ui::pane_look::pane_color(cx.theme(), cx.theme().background);
                if can_focus && self.focused_pane != Some(pane_id) {
                    browser.read(cx).focus_handle(cx).focus(window, cx);
                    self.focused_pane = Some(pane_id);
                }
                browser.into_any_element()
            }
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            PaneKindSnapshot::Browser(descriptor) => super::unsupported_pane(
                "Browser",
                IconName::Globe,
                "Embedded Chromium needs the desktop app. Open this page in a browser tab.",
                descriptor.tabs.get(descriptor.active_tab).cloned(),
                Corners::all(radius),
                cx,
            ),
            PaneKindSnapshot::Editor(_) => super::unsupported_pane(
                "Editor",
                IconName::File,
                "The daemon does not share editor file contents with browser clients yet.",
                None,
                Corners::all(radius),
                cx,
            ),
            PaneKindSnapshot::Other { kind } => super::unsupported_pane(
                kind,
                IconName::Info,
                &zz_client::navigation::newer_pane_text(kind),
                None,
                Corners::all(radius),
                cx,
            ),
            PaneKindSnapshot::View(descriptor) => {
                self.view_element(pane_id, descriptor, can_focus, window, cx)
            }
            PaneKindSnapshot::Picker => {
                let picker = self.picker_entity(pane_id, true, cx);
                picker.update(cx, |picker, cx| {
                    picker.set_corner_radii(Corners::all(radius), cx);
                });
                if can_focus && self.focused_pane != Some(pane_id) {
                    picker.read(cx).focus_handle(cx).focus(window, cx);
                    self.focused_pane = Some(pane_id);
                }
                picker.into_any_element()
            }
        };
        if current {
            self.compact.page_background = Some(background);
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if current {
            ala_platform::mobile::set_status_bar_on_dark(Some(background.l < 0.5));
        }
        let browser = matches!(pane.kind, PaneKindSnapshot::Browser(_));
        let bleed = matches!(pane.kind, PaneKindSnapshot::Agent(_));
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .when(!bleed, |page| page.pt(top_inset))
            .when(!browser && !bleed, |page| page.pb(reserve))
            .bg(background)
            .rounded(radius)
            .child(div().flex_1().min_h_0().min_w_0().child(content))
            .when(bleed && top_inset > px(0.0), |page| {
                page.child(edge_blur(
                    penna::Edge::Top,
                    top_inset + px(EDGE_FADE),
                    background,
                ))
            })
            .when(rows_above && top_inset > px(0.0), |page| {
                page.child(edge_blur(penna::Edge::Top, top_inset, background))
            })
            .when(rows_below && reserve > px(0.0), |page| {
                page.child(edge_blur(penna::Edge::Bottom, reserve, background))
            })
            .into_any_element()
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub(super) fn show_pill_notice(
        &mut self,
        pane: PaneId,
        text: &'static str,
        kind: PillNotice,
        cx: &mut Context<Self>,
    ) {
        self.compact.notice = Some((pane, text, kind, Instant::now() + NOTICE_LINGER));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTICE_LINGER).await;
            this.update(cx, |_, cx| cx.notify()).ok();
        })
        .detach();
        cx.notify();
    }

    fn phone_auth_open(&self) -> bool {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        return self.phone_auth.is_some();
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        false
    }

    fn compact_can_focus(&self, window: &Window, cx: &App) -> bool {
        let core = &self.connection.read(cx).core;
        if self.phone_auth_open() {
            return false;
        }
        self.settings.is_none()
            && self.compact.sheet.is_none()
            && self.prompt.is_none()
            && !self.slideover
            && !self.compact_lifted()
            && !self.sidebar_focus.is_focused(window)
            && core.choose_tree().is_none()
            && core.choose_buffer().is_none()
            && core.command_prompt().is_none()
            && core.menu().is_none()
            && core.confirm().is_none()
            && core.command_output().is_none()
    }

    fn compact_lifted(&self) -> bool {
        self.compact.overview
            || self.compact.lift.progress() > 0.0
            || self.compact.lift.is_moving()
            || self.compact.landing.is_some()
    }

    fn compact_lift_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(window.line_height());
        let response =
            self.compact
                .lift
                .scroll(f32::from(delta.y), event.touch_phase, Instant::now());
        if !response.consumed {
            return;
        }
        cx.stop_propagation();
        if response.started {
            if !self.compact.overview {
                self.aim_overview(cx);
            }
            self.compact.landing = None;
            self.compact.overview_reveal = true;
        }
        if let Some(event) = response.event {
            window.end_touch_momentum();
            cx.haptic(ala::Haptic::Light);
            self.compact.overview = event == LiftEvent::Open;
        }
        cx.notify();
    }

    #[cfg(any(target_os = "ios", target_os = "android", target_family = "wasm"))]
    pub(super) fn compact_visible_panes(&self, cx: &App) -> BTreeSet<PaneId> {
        if self.compact_lifted() || self.compact.bindings {
            return BTreeSet::new();
        }
        if self.shown_stack(cx).is_some() || self.compact.sheet.is_some() {
            #[cfg(target_family = "wasm")]
            return BTreeSet::new();
            #[cfg(not(target_family = "wasm"))]
            return self
                .active_window(cx)
                .map(|window| window.active_pane)
                .filter(|pane| {
                    self.browsers
                        .get(pane)
                        .is_some_and(|browser| !browser.read(cx).still_ready())
                })
                .into_iter()
                .collect();
        }
        if self.compact.pager.is_moving() {
            let (pages, _) = pages(&self.status_model(cx), &self.unseen_agents);
            return self
                .compact
                .pager
                .layout(self.compact.width, PAGE_GAP)
                .pages
                .into_iter()
                .filter_map(|(index, _)| pages.get(index).map(|page| page.pane))
                .collect();
        }
        self.active_window(cx)
            .map(|window| BTreeSet::from([window.active_pane]))
            .unwrap_or_default()
    }

    pub(super) fn compact_shell(
        &mut self,
        terminal_overlays: Vec<AnyElement>,
        overlays: Vec<AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<ala::Div> {
        self.prune_pane_entities(cx);
        let model = self.status_model(cx);
        let (pages, current) = pages(&model, &self.unseen_agents);
        let current_pane = pages.get(current).map(|page| page.pane);
        self.follow_compact_page(current_pane, cx);
        let width = f32::from(window.fully_visible_bounds().size.width);
        self.compact.width = width;
        self.compact.pager.sync(current, pages.len());
        if cx.reduce_motion() {
            self.compact.pager.finish();
            self.compact.lift.finish();
        }
        let animating = self.compact.pager.tick(Instant::now());
        let event = self.compact.pager.take_event();
        self.commit_page(event, cx);
        if animating {
            window.request_animation_frame();
        } else if self.terminal_resize_suppressed.get()
            && self.split_drag.is_none()
            && self.compact.pinch.is_none()
            && !self.compact.keyboard_moving
        {
            self.terminal_resize_suppressed.set(false);
        }
        let motion = self.compact.pager.motion(width);
        let layout = self.compact.pager.layout(width, PAGE_GAP);
        let visible = window.fully_visible_bounds();
        let top_inset = visible.top();
        let keyboard = keyboard_visible(window);
        let safe_bottom = (window.viewport_size().height - visible.bottom()).max(px(0.0));
        let overlap = window.viewport_size().height - window.visual_viewport_bounds().bottom();
        if safe_bottom > overlap || overlap <= px(0.0) {
            self.compact.safe_bottom = safe_bottom;
        }
        #[cfg_attr(not(target_os = "android"), allow(unused_mut))]
        let mut placement = PillPlacement::new(screen_radius(window), self.compact.safe_bottom);
        #[cfg(target_os = "android")]
        {
            placement.bottom = placement.bottom.max(self.compact.safe_bottom);
        }
        let zone = placement.zone();
        self.compact.extra_height = if keyboard {
            let rest =
                window.viewport_size().height - window.target_visual_viewport_bounds().bottom();
            (key_row_top(zone, overlap) - key_row_top(zone, rest)).max(px(0.0))
        } else {
            px(0.0)
        };
        let now = Instant::now();
        if self.compact.lift.tick(now) {
            window.request_animation_frame();
        }
        let keyless = pages.get(self.compact.pager.index()).is_some_and(|page| {
            matches!(page.kind, PaneKindSnapshot::Browser(_))
                || matches!(page.kind, PaneKindSnapshot::Agent(_))
        });
        let shown_terminal = pages
            .get(self.compact.pager.index())
            .filter(|page| page.kind == PaneKindSnapshot::Terminal)
            .and_then(|page| self.terminals.get(&page.pane).cloned());
        let finding = shown_terminal
            .as_ref()
            .is_some_and(|terminal| terminal.read(cx).phone_find(cx).is_some());
        self.compact
            .morph
            .aim(!keyboard_heading_up(window) || keyless || finding, now);
        if cx.reduce_motion() {
            self.compact.morph.finish();
        }
        if self.compact.morph.tick(now) {
            window.request_animation_frame();
        }
        let morph = self.compact.morph.progress();
        if let Some((target, since)) = self.compact.entering {
            if let Some(page) = pages.iter().find(|page| page.pane == target).cloned() {
                self.compact.entering = None;
                self.compact_select(&page, cx);
                self.land_overview(Some(target), cx);
            } else if now.saturating_duration_since(since) >= LANDING_WAIT {
                self.compact.entering = None;
                self.close_overview(cx);
            } else {
                window.request_animation_frame();
            }
        }
        if let Some(since) = self.compact.making {
            if current_pane.is_some() {
                self.compact.making = None;
                self.land_overview(current_pane, cx);
            } else if now.saturating_duration_since(since) >= MAKING_WAIT {
                self.compact.making = None;
            } else {
                window.request_animation_frame();
            }
        }
        if let Some((target, since)) = self.compact.landing
            && !self.compact.lift.is_moving()
        {
            if current_pane == Some(target) || now.saturating_duration_since(since) >= LANDING_WAIT
            {
                self.compact.landing = None;
            } else {
                window.request_animation_frame();
            }
        }
        let progress = self.compact.lift.progress();
        let open = self.compact.overview && !self.compact.lift.is_moving();
        let lifting = !open
            && (progress > 0.0 || self.compact.lift.is_moving() || self.compact.landing.is_some());
        let shown = self.compact.pager.index();
        let shown_page = pages.get(shown);
        let floating = !keyboard;
        let can_make = {
            let connection = self.connection.read(cx);
            connection.connected && !connection.core.attached_read_only()
        };
        let pill_shown = split_pill_bounds(px(width), &placement, false, can_make);
        let ends = bar_ends(false, can_make);
        let zone_top = window.viewport_size().height - zone;
        let in_window = |bounds: Bounds<Pixels>| {
            Bounds::new(
                point(bounds.origin.x, bounds.origin.y + zone_top),
                bounds.size,
            )
        };
        let hole = (floating && morph >= 1.0).then(|| BarHoles {
            pill: (
                in_window(pill_shown),
                pill_corner(pill_shown, &placement, cx),
            ),
            disc: can_make.then(|| {
                let disc = bar_disc_bounds(px(width), &placement, true);
                let disc = Bounds::new(
                    point(
                        disc.origin.x,
                        pill_shown.center().y - disc.size.height / 2.0,
                    ),
                    disc.size,
                );
                (in_window(disc), pill_corner(disc, &placement, cx))
            }),
            handle: in_window(pill_handle_bounds(pill_shown)),
        });
        let reserve = if floating { zone } else { px(0.0) };
        let veil = self
            .stacks()
            .map(|menu| menu.read(cx).close_shown())
            .fold(0.0, f32::max);
        self.compact.chat_dock = Some(super::agent_pane::PhoneDock {
            bottom: placement.bottom,
            top: top_inset,
            reserve,
            keyboard,
            veil: if open { 0.0 } else { veil },
            covered: !self.compact_can_focus(window, cx)
                || self.compact.bindings
                || self.stacks().any(|menu| menu.read(cx).is_open()),
        });
        let mut strip = Vec::new();
        if !lifting {
            for (index, x) in layout.pages {
                let Some(page) = pages.get(index).cloned() else {
                    continue;
                };
                let element = self.compact_page(
                    &page,
                    index == shown,
                    motion,
                    top_inset,
                    reserve,
                    hole,
                    window,
                    cx,
                );
                strip.push(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(x))
                        .w(px(width))
                        .child(element)
                        .into_any_element(),
                );
            }
        }
        let pager_area = div()
            .id("compact-pager")
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .when(!lifting, |area| {
                area.on_pinch(cx.listener(|this, event: &PinchEvent, _, cx| {
                    if event.phase == TouchPhase::Started {
                        this.compact_pinch(event, cx);
                    }
                }))
                .capture_pinch(cx.listener(|this, event: &PinchEvent, _, cx| {
                    if event.phase != TouchPhase::Started {
                        this.compact_pinch(event, cx);
                    }
                }))
                .children(strip)
                .children(terminal_overlays)
            })
            .when(
                self.compact.pinch.is_some()
                    || self
                        .compact
                        .scale_hud
                        .is_some_and(|until| Instant::now() < until),
                |area| {
                    area.child(compact_hud(
                        format!("{:.0}%", self.preferences.terminal_font_scale * 100.0),
                        cx,
                    ))
                },
            );
        if !keyboard && let Some((keys, _)) = &self.compact.tool_keys {
            keys.clone().close(cx);
        }
        let stack = self.pill_stack(window, cx);
        let new_stack = self.new_stack(window, cx);
        let session = self.attached_session_name(cx);
        let made = if open {
            new_menu(self.machine_menu_text(cx).as_menu())
        } else if can_make {
            new_menu(MachineMenu::Session(&session))
        } else {
            Vec::new()
        };
        let (groups, fold) = if open {
            (machine_menu(), None)
        } else {
            match self.browser_groups(shown_page, cx) {
                Some((groups, fold)) => (groups, Some(fold)),
                None => (
                    pane_menu(
                        shown_page.is_some_and(|page| page.kind == PaneKindSnapshot::Terminal),
                    ),
                    None,
                ),
            }
        };
        if let Some((keys, _)) = &self.compact.tool_keys {
            let prefix = keys.prefix.read(cx).menu().clone();
            let groups: Vec<_> = made.iter().chain(&groups).cloned().collect();
            prefix.update(cx, |menu, cx| menu.set_groups(groups, cx));
        }
        stack.update(cx, |menu, cx| {
            menu.set_groups(groups, cx);
            menu.set_fold(fold, cx);
        });
        new_stack.update(cx, |menu, cx| menu.set_groups(made, cx));
        let chat = shown_page.is_some_and(|page| matches!(page.kind, PaneKindSnapshot::Agent(_)));
        if keyboard && !chat {
            for menu in [&stack, &new_stack] {
                if menu.read(cx).is_shown() {
                    menu.update(cx, StackMenu::close);
                }
            }
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            let prefix_open = self
                .compact
                .tool_keys
                .as_ref()
                .is_some_and(|(keys, _)| keys.prefix.read(cx).menu().read(cx).is_shown());
            if [&stack, &new_stack].into_iter().any(|menu| {
                let menu = menu.read(cx);
                menu.is_shown() && !menu.is_open()
            }) {
                window.request_animation_frame();
            }
            let frozen = (!open
                && (stack.read(cx).is_shown()
                    || new_stack.read(cx).is_shown()
                    || prefix_open
                    || self.compact.sheet.is_some()))
            .then_some(current_pane)
            .flatten();
            for (pane, browser) in &self.browsers {
                let still = frozen == Some(*pane);
                browser.update(cx, |browser, cx| browser.set_frozen(still, window, cx));
            }
        }
        let (menu_progress, new_progress) = if open {
            (0.0, 0.0)
        } else {
            (
                stack.read(cx).close_shown(),
                new_stack.read(cx).close_shown(),
            )
        };
        let page_background = self
            .compact
            .page_background
            .unwrap_or(cx.theme().background);
        let picking = self.browser_picking(cx);
        let floating = !keyboard && morph >= 1.0;
        let backdrop = shown_page
            .filter(|_| floating)
            .and_then(|page| self.browser_backdrop(page, cx))
            .or(self.compact.page_background.filter(|_| floating));
        let pill_placement = PillPlacement {
            glass: backdrop.and_then(|backdrop| pill_glass_over(backdrop, cx)),
            ..placement
        };
        let find = shown_terminal
            .as_ref()
            .and_then(|terminal| {
                let state = terminal.read(cx).phone_find(cx)?;
                Some(Self::phone_find_field(terminal, state, cx))
            })
            .or_else(|| self.browser_address_field(cx));
        let suggestions = shown_terminal
            .is_none()
            .then(|| self.browser_suggestions(cx))
            .flatten();
        let bottom = if let Some(find) = find {
            if keyboard {
                let lift = key_row_lift(zone, overlap);
                div()
                    .relative()
                    .flex_none()
                    .w_full()
                    .h(px(KEY_ROW_HEIGHT) + lift)
                    .px(px(KEY_ROW_MARGIN_X))
                    .pt(px(KEY_ROW_MARGIN_Y))
                    .pb(px(KEY_ROW_MARGIN_Y) + lift)
                    .bg(page_background)
                    .child(find)
                    .children(suggestions.map(|suggestions| {
                        div()
                            .absolute()
                            .left(px(KEY_ROW_MARGIN_X))
                            .right(px(KEY_ROW_MARGIN_X))
                            .bottom(
                                px(KEY_ROW_HEIGHT) + lift - px(KEY_ROW_MARGIN_Y)
                                    + px(penna::Space::GAP_RELATED.design_px()),
                            )
                            .child(suggestions)
                    }))
                    .into_any_element()
            } else {
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(zone)
                    .child(compact_floating_bar(find, &placement, cx))
                    .into_any_element()
            }
        } else if keyboard && chat {
            div()
                .flex_none()
                .w_full()
                .h((zone - placement.pill_top() - overlap).max(px(0.0)))
                .bg(page_background)
                .into_any_element()
        } else if keyboard && keyless {
            div()
                .flex_none()
                .w_full()
                .h((zone - overlap).max(px(0.0)))
                .bg(page_background)
                .into_any_element()
        } else if keyboard {
            let lift = key_row_lift(zone, overlap);
            let height = px(KEY_ROW_HEIGHT) + lift;
            let row = self.compact_key_row(window, cx);
            let area = div()
                .relative()
                .flex_none()
                .w_full()
                .h(height)
                .bg(page_background);
            if morph > 0.0 {
                let pill_top = height + overlap - zone + placement.pill_top();
                let frame =
                    pill_morph_frame(row.pill_end(), morph, pill_top, height, ends, &placement);
                let label = self.pane_strip(
                    &model,
                    &pages,
                    picking,
                    pill_shown.size.width,
                    &placement,
                    now,
                    window,
                    cx,
                );
                area.child(compact_pill_morph_between(
                    row, label, morph, pill_top, height, ends, &placement, cx,
                ))
                .children(self.bar_discs(
                    px(width),
                    frame.bounds(px(width)),
                    &placement,
                    can_make,
                    &new_stack,
                    [morph_bar_ink(morph); 2],
                    (0.0, page_background),
                    cx,
                ))
            } else {
                area.pb(lift).child(row)
            }
            .into_any_element()
        } else if morph < 1.0 {
            let row = self.compact_key_row(window, cx);
            let frame = pill_morph_frame(
                row.pill_end(),
                morph,
                placement.pill_top(),
                zone,
                ends,
                &placement,
            );
            let label = self.pane_strip(
                &model,
                &pages,
                picking,
                pill_shown.size.width,
                &placement,
                now,
                window,
                cx,
            );
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(zone)
                .child(compact_pill_morph_between(
                    row,
                    label,
                    morph,
                    placement.pill_top(),
                    zone,
                    ends,
                    &placement,
                    cx,
                ))
                .children(self.bar_discs(
                    px(width),
                    frame.bounds(px(width)),
                    &placement,
                    can_make,
                    &new_stack,
                    [morph_bar_ink(morph); 2],
                    (0.0, page_background),
                    cx,
                ))
                .into_any_element()
        } else {
            let fade = if lifting {
                (1.0 - progress * BAR_SWAP).max(0.0)
            } else {
                1.0
            };
            let strip = self
                .pane_strip(
                    &model,
                    &pages,
                    picking,
                    pill_shown.size.width,
                    &pill_placement,
                    now,
                    window,
                    cx,
                )
                .when(!picking, |pill| pill.child(stack.read(cx).track_anchor()))
                .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                    this.compact_scroll(event, window, cx);
                }));
            let pill = div()
                .absolute()
                .left(pill_shown.origin.x)
                .top(pill_shown.origin.y)
                .w(pill_shown.size.width)
                .h(pill_shown.size.height)
                .flex()
                .opacity(fade * (1.0 - menu_progress))
                .child(strip)
                .children(bar_wash(
                    new_progress * MENU_DIM,
                    pill_corner(pill_shown, &pill_placement, cx),
                    page_background,
                ));
            let jump = (!lifting
                && shown_terminal
                    .as_ref()
                    .is_some_and(|terminal| terminal.read(cx).scrolled_back(cx)))
            .then(|| {
                let at = over_pill(pill_shown, px(TOUCH_TARGET));
                let terminal = shown_terminal.clone();
                div()
                    .absolute()
                    .left(at.x)
                    .top(at.y)
                    .occlude()
                    .opacity(fade * (1.0 - menu_progress))
                    .child(
                        penna::blocks::chat::phone_jump_button(
                            "compact-jump",
                            "Jump to bottom",
                            cx.theme().ground(),
                        )
                        .on_click(move |_, _, cx| {
                            if let Some(terminal) = &terminal {
                                terminal.update(cx, TerminalPane::jump_to_bottom);
                            }
                        }),
                    )
            });
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(zone)
                .children(jump)
                .child(
                    div()
                        .id("compact-bar-zone")
                        .absolute()
                        .left(pill_shown.origin.x)
                        .top(pill_shown.origin.y)
                        .w(pill_shown.size.width)
                        .h(pill_shown.size.height)
                        .on_scroll_wheel(cx.listener(Self::compact_lift_scroll)),
                )
                .child(pill)
                .children(self.bar_discs(
                    px(width),
                    pill_shown,
                    &pill_placement,
                    can_make,
                    &new_stack,
                    [
                        fade * (1.0 - MENU_DIM * menu_progress.max(new_progress)),
                        fade * (1.0 - new_progress),
                    ],
                    (MENU_DIM * menu_progress, page_background),
                    cx,
                ))
                .into_any_element()
        };
        let mut sheets = Vec::new();
        let over_pill = if keyboard { px(0.0) } else { placement.bottom };
        if self.compact.bindings {
            sheets.push(self.compact_bindings_sheet(over_pill, cx));
        }
        if !self.compact.pager.is_moving()
            && self.compact_can_focus(window, cx)
            && let Some(page) = pages
                .get(self.compact.pager.index())
                .filter(|page| matches!(page.kind, PaneKindSnapshot::Picker))
        {
            let picker = self.picker_entity(page.pane, true, cx);
            sheets.extend(picker.update(cx, |picker, cx| picker.sheet(over_pill, cx)));
        }
        sheets.extend(self.phone_sheet(window, cx));
        let scrim = (!open)
            .then(|| {
                [
                    StackMenu::scrim(&stack, window, cx).into_any_element(),
                    StackMenu::scrim(&new_stack, window, cx).into_any_element(),
                ]
            })
            .into_iter()
            .flatten();
        let screen = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(pager_area)
            .children(scrim)
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .when(
                        self.compact.sheet.is_some()
                            || self.browser_tabs_open(cx)
                            || self.phone_auth_open(),
                        |bottom| bottom.opacity(0.0),
                    )
                    .child(bottom),
            );
        let content = if open {
            if let Some(endpoint) = self.compact.held.clone() {
                sheets.push(self.machine_sheet(&endpoint, cx));
            }
            #[cfg(any(target_os = "ios", target_os = "android"))]
            if self.settings.is_none() && self.hosts.sheet == Some(super::hosts::HostSheet::Add) {
                sheets.push(self.add_host_sheet(
                    over_pill,
                    |_, cx| close_sheet(super::hosts::ADD_HOST_SHEET, cx),
                    |this, cx| {
                        this.hosts.sheet = None;
                        cx.notify();
                    },
                    cx,
                ));
            }
            let (pages, bars) = self.overview_layers(&model, top_inset, &placement, window, cx);
            div()
                .relative()
                .size_full()
                .child(pages)
                .child(StackMenu::scrim(&stack, window, cx))
                .child(StackMenu::scrim(&new_stack, window, cx))
                .child(bars)
                .into_any_element()
        } else if lifting {
            let target = self
                .compact
                .landing
                .map(|(pane, _)| pane)
                .or(current_pane)
                .and_then(|pane| pages.iter().find(|page| page.pane == pane).cloned());
            let stage = window.viewport_size();
            let (overview, bars) = self.overview_layers(&model, top_inset, &placement, window, cx);
            let swap = (progress * BAR_SWAP).min(1.0);
            let child = target.map_or_else(
                || div().into_any_element(),
                |page| self.compact_page(&page, false, 0.0, top_inset, zone, None, window, cx),
            );
            div()
                .relative()
                .size_full()
                .child(overview::pane_zoom_layers(
                    overview,
                    child,
                    stage,
                    progress,
                    page_background,
                    screen_radius(window).unwrap_or_else(|| cx.theme().phone_radius()),
                ))
                .child(div().absolute().inset_0().opacity(swap).child(bars))
                .child(
                    div()
                        .id("overview-lift-block")
                        .absolute()
                        .inset_0()
                        .occlude(),
                )
                .child(div().absolute().inset_0().child(screen))
                .into_any_element()
        } else {
            screen.into_any_element()
        };
        let menus = [&stack, &new_stack]
            .into_iter()
            .filter(|menu| menu.read(cx).is_shown())
            .cloned()
            .collect::<Vec<_>>();
        div()
            .id("compact-shell")
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .child(content)
            .children(menus)
            .children(sheets)
            .child(penna::blocks::touch::touch_scale(
                div()
                    .absolute()
                    .top(top_inset)
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .children(overlays),
                cx,
            ))
    }

    #[allow(clippy::too_many_arguments)]
    fn bar_discs(
        &self,
        width: Pixels,
        pill: Bounds<Pixels>,
        placement: &PillPlacement,
        can_make: bool,
        new_stack: &Entity<StackMenu>,
        ink: [f32; 2],
        wash: (f32, Hsla),
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if ink.iter().all(|ink| *ink <= 0.0) {
            return Vec::new();
        }
        let handle_at = pill_handle_bounds(pill);
        let up = div()
            .absolute()
            .left(handle_at.origin.x)
            .top(handle_at.origin.y)
            .opacity(ink[0])
            .child(
                pill_handle("compact-bar-handle", cx)
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        cx.haptic(ala::Haptic::Light);
                        window.dismiss_virtual_keyboard();
                        this.open_overview(cx);
                    }))
                    .on_scroll_wheel(cx.listener(Self::compact_lift_scroll)),
            )
            .into_any_element();
        let plus = can_make.then(|| {
            let plus_at = bar_disc_bounds(width, placement, true);
            div()
                .absolute()
                .left(plus_at.origin.x)
                .top(pill.center().y - plus_at.size.height / 2.0)
                .opacity(ink[1])
                .child(
                    compact_bar_disc("compact-bar-new", IconName::Plus, "New", placement, cx)
                        .child(StackMenu::pill_anchor(new_stack, cx))
                        .children(bar_wash(
                            wash.0,
                            pill_corner(plus_at, placement, cx),
                            wash.1,
                        ))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.haptic(ala::Haptic::Light);
                            this.open_new_menu(cx);
                        })),
                )
                .into_any_element()
        });
        std::iter::once(up).chain(plus).collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn pane_strip(
        &self,
        model: &StatusBarModel,
        pages: &[Page],
        picking: bool,
        width: Pixels,
        placement: &PillPlacement,
        now: Instant,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let shown = self.compact.pager.index();
        let ground = penna::blocks::touch::key_pill_layer(cx.theme()).ground;
        let items = pages
            .iter()
            .enumerate()
            .map(|(index, page)| {
                let (mut title, word, mut meta, mut tone) = self.pill_text(model, Some(page), cx);
                if index == shown {
                    if picking {
                        meta = "Picking an element".into();
                    }
                    if let Some((pane, text, kind, until)) = self.compact.notice
                        && until > now
                        && page.pane == pane
                    {
                        title = text.into();
                        tone = if kind == PillNotice::Failed {
                            penna::Tone::Warning
                        } else {
                            penna::Tone::Neutral
                        };
                    }
                }
                let dot = (index != shown && (tone != penna::Tone::Neutral || page.attention))
                    .then(|| {
                        cx.theme().tone(if tone == penna::Tone::Neutral {
                            penna::Tone::Warning
                        } else {
                            tone
                        })
                    });
                let face = strip_face(
                    self.page_program(page, cx),
                    kind_icon(&page.kind),
                    dot,
                    ground,
                    cx,
                );
                StripItem::new(StripMark::Face(face.into_any_element()), title, meta)
                    .word(word, tone)
            })
            .collect();
        let this = cx.entity().downgrade();
        pill_strip(
            "compact-pill",
            items,
            self.compact.pager.position(),
            width,
            placement,
            move |index, window, cx| {
                _ = this.update(cx, |this, cx| {
                    if index != this.compact.pager.index() {
                        cx.haptic(ala::Haptic::Selection);
                        this.compact.pager.go_to(index, Instant::now());
                        this.commit_page(Some(PagerEvent::Commit { index }), cx);
                        cx.notify();
                    } else if this.browser_picking(cx) {
                        this.end_browser_pick(window, cx);
                    } else {
                        cx.haptic(ala::Haptic::Light);
                        this.open_pill_menu(cx);
                    }
                });
            },
            window,
            cx,
        )
    }

    fn page_program(&self, page: &Page, cx: &App) -> Option<&'static Program> {
        self.connection
            .read(cx)
            .core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&page.pane))
            .and_then(pane_program)
    }

    fn pill_text(
        &self,
        model: &StatusBarModel,
        page: Option<&Page>,
        cx: &App,
    ) -> (SharedString, SharedString, SharedString, penna::Tone) {
        let Some(page) = page else {
            return (
                SharedString::default(),
                SharedString::default(),
                SharedString::default(),
                penna::Tone::Neutral,
            );
        };
        let core = &self.connection.read(cx).core;
        let waiting = core.agent_state(page.pane).filter(|state| {
            zz_client::agent_attention_status(state) == zz_client::AgentAttentionStatus::NeedsInput
        });
        if let Some(state) = waiting {
            let asks =
                zz_client::agent_lens::pending_permissions(&[], state.pending_permission.as_ref())
                    .first()
                    .is_some_and(|request| !request.questions.is_empty());
            let word = if asks {
                "Has a question"
            } else {
                "Needs approval"
            };
            return (
                page.title.clone(),
                word.into(),
                SharedString::default(),
                penna::Tone::Warning,
            );
        }
        let browser = self.browser_page(page, cx);
        if let Some((title, host)) = &browser
            && !host.is_empty()
        {
            return (
                title.clone(),
                SharedString::default(),
                host.clone().into(),
                penna::Tone::Neutral,
            );
        }
        let title = browser.map_or_else(|| page.title.clone(), |(title, _)| title);
        let parts = core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find(|window| window.id == page.window)
            .and_then(|window| {
                window
                    .panes
                    .get(&page.pane)
                    .map(|pane| zz_client::window_summary::pane_detail(window, pane, &title))
            });
        let Some(parts) = parts else {
            let meta = model
                .windows
                .iter()
                .find(|window| window.id == page.window)
                .map(|window| window.label.clone())
                .unwrap_or_default();
            return (
                title,
                SharedString::default(),
                meta.into(),
                penna::Tone::Neutral,
            );
        };
        let (word, meta, tone) = pill_words(&parts);
        (title, word.into(), meta.into(), tone)
    }

    fn browser_backdrop(&self, page: &Page, cx: &App) -> Option<ala::Hsla> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(browser) = self.browsers.get(&page.pane) {
            return browser.read(cx).backdrop_color();
        }
        let _ = (page, cx);
        None
    }

    pub(super) fn browser_page(&self, page: &Page, cx: &App) -> Option<(SharedString, String)> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(browser) = self.browsers.get(&page.pane) {
            let (title, url) = browser.read(cx).shown_page();
            let title = if title.is_empty() {
                page.title.clone()
            } else {
                title.to_owned().into()
            };
            return Some((title, super::browser_pane::host(url).to_owned()));
        }
        let _ = (page, cx);
        None
    }

    fn compact_key_row(&mut self, window: &mut Window, cx: &mut Context<Self>) -> KeyRow {
        let keys = self.tool_keys(window, cx);
        let core = &self.connection.read(cx).core;
        let armed = core.prefix_armed();
        let prefix = tmux_keystroke(&prefix_spelling(core));
        keys.prefix.update(cx, |key, cx| {
            key.set_armed(armed, cx);
            key.set_keystroke(prefix, cx);
        });
        key_row(&keys, Some(keys.lead()))
    }

    pub(super) fn wide_key_row(
        &mut self,
        compact: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<KeyRow> {
        if compact {
            return None;
        }
        if !self.wide_keys_wanted(window, cx) {
            penna::blocks::touch::StickyModifiers::take(cx);
            self.close_key_stacks(cx);
            return None;
        }
        let keys = self.tool_keys(window, cx);
        let lead = live_prefix(self.connection.read(cx).core.mux_options()).map(|keystroke| {
            KeyRowKey::Key {
                label: Kbd::format(&keystroke).into(),
                icon: None,
                keystroke,
            }
        });
        Some(key_row(&keys, lead))
    }

    fn wide_keys_wanted(&self, window: &Window, cx: &App) -> bool {
        #[cfg(target_os = "ios")]
        if (ala_platform::mobile::soft_keyboard() || keyboard_visible(window))
            && !ala_platform::mobile::hardware_keyboard()
            && self
                .terminals
                .values()
                .chain(self.output_terminal.iter().map(|(_, terminal)| terminal))
                .any(|terminal| terminal.read(cx).typing(window))
        {
            return true;
        }
        let _ = (window, cx);
        false
    }

    fn compact_bindings_sheet(
        &mut self,
        bottom_inset: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let core = &self.connection.read(cx).core;
        let prefix = prefix_spelling(core);
        let rows = binding_rows(core, &prefix);
        let cap = tmux_keystroke(&prefix).map(|key| Kbd::new(key).lowercase().into_any_element());
        let pick = cx.weak_entity();
        let dismiss = cx.weak_entity();
        bottom_sheet(
            BINDINGS_SHEET,
            "All bindings",
            cap,
            WhichKeyList::new(rows).on_pick(move |id, _, cx| {
                let _ = pick.update(cx, |this, cx| this.send_binding(id, cx));
                close_sheet(BINDINGS_SHEET, cx);
            }),
            bottom_inset,
            move |_, cx| {
                let _ = dismiss.update(cx, |this, cx| {
                    this.compact.bindings = false;
                    cx.notify();
                });
            },
        )
        .into_any_element()
    }
}

#[cfg(target_os = "android")]
enum BackTarget {
    Sheet(&'static str),
    Card,
    Menu,
    Overlay,
    Settings,
    Overview,
}

#[cfg(target_os = "android")]
impl AppShell {
    fn back_target(&self, dialog: bool, window: &Window, cx: &App) -> Option<BackTarget> {
        if self.compact.bindings {
            return Some(BackTarget::Sheet(BINDINGS_SHEET));
        }
        if self.compact.held.is_some() {
            return Some(BackTarget::Sheet(overview::MACHINE_SHEET));
        }
        if let Some(sheet) = &self.compact.sheet {
            return Some(BackTarget::Sheet(sheet.id()));
        }
        if self.compact.card.is_some() {
            return Some(BackTarget::Card);
        }
        match self.hosts.sheet {
            Some(super::hosts::HostSheet::Add) => {
                return Some(BackTarget::Sheet(super::hosts::ADD_HOST_SHEET));
            }
            Some(super::hosts::HostSheet::Host(_)) => {
                return Some(BackTarget::Sheet(super::hosts::HOST_SHEET));
            }
            None => {}
        }
        if self.shown_stack(cx).is_some()
            || self
                .compact
                .tool_keys
                .as_ref()
                .is_some_and(|(keys, _)| keys.is_open(cx))
        {
            return Some(BackTarget::Menu);
        }
        let core = &self.connection.read(cx).core;
        let menu = window.focused(cx).is_some_and(|focus| {
            cx.build_action("penna_menu::Cancel", None)
                .is_ok_and(|action| {
                    window
                        .highest_precedence_binding_for_action_in(action.as_ref(), &focus)
                        .is_some()
                })
        });
        if dialog
            || menu
            || self.prompt.is_some()
            || self.slideover
            || core.choose_tree().is_some()
            || core.choose_buffer().is_some()
            || core.command_prompt().is_some()
            || core.menu().is_some()
            || core.confirm().is_some()
            || core.command_output().is_some()
            || core.display_panes().is_some()
        {
            return Some(BackTarget::Overlay);
        }
        if self.settings.is_some() {
            return Some(BackTarget::Settings);
        }
        self.compact.overview.then_some(BackTarget::Overview)
    }

    pub(super) fn sync_system_back(
        &mut self,
        dialog: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let enabled = self.back_target(dialog, window, cx).is_some();
        if self.system_back != Some(enabled) {
            self.system_back = Some(enabled);
            window.set_system_back_enabled(enabled);
        }
    }

    pub(super) fn system_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = window
            .root::<penna::Root>()
            .flatten()
            .is_some_and(|root| root.read(cx).has_active_dialog());
        match self.back_target(dialog, window, cx) {
            Some(BackTarget::Sheet(id)) => close_sheet(id, cx),
            Some(BackTarget::Card) => self.close_machine_card(window, cx),
            Some(BackTarget::Menu) => self.close_pill_menu(cx),
            Some(BackTarget::Overlay) => {
                if let Ok(escape) = ala::Keystroke::parse("escape") {
                    window.dispatch_keystroke(escape, cx);
                }
            }
            Some(BackTarget::Settings) => self.settings_back(window, cx),
            Some(BackTarget::Overview) => self.close_overview(cx),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinch_scales_terminal_text_in_five_percent_steps_within_the_setting_range() {
        assert_eq!(pinch_font_scale(1.0, 1.0), 1.0);
        assert_eq!(pinch_font_scale(1.0, 1.02), 1.0);
        assert_eq!(pinch_font_scale(1.0, 1.5), 1.5);
        assert_eq!(pinch_font_scale(1.2, 1.12), 1.35);
        assert_eq!(pinch_font_scale(1.0, 0.8), 0.8);
        assert_eq!(pinch_font_scale(2.5, 2.0), 3.0);
        assert_eq!(pinch_font_scale(0.6, 0.1), 0.5);
        assert_eq!(pinch_font_scale(1.1, f32::NAN), 1.1);
    }

    #[test]
    fn the_key_row_rests_on_the_bar_line_until_the_keyboard_passes_it() {
        let zone = px(92.0);
        let top = |overlap: f32| key_row_top(zone, px(overlap));
        assert_eq!(top(0.0), zone);
        assert_eq!(top(20.0), zone);
        assert_eq!(top(300.0), px(KEY_ROW_HEIGHT + 300.0));
    }

    #[test]
    fn the_wide_prefix_key_sends_a_live_prefix_or_is_left_out() {
        use zz_protocol::{MuxOptionKey, MuxOptionSource, MuxOptions};
        let mut options = MuxOptions::default();
        assert_eq!(live_prefix(&options), tmux_keystroke("C-b"));
        options.set(
            MuxOptionKey::Prefix,
            "None",
            MuxOptionSource::RuntimeCommand,
        );
        assert_eq!(live_prefix(&options), None);
        options.set(
            MuxOptionKey::Prefix2,
            "C-a",
            MuxOptionSource::RuntimeCommand,
        );
        assert_eq!(live_prefix(&options), tmux_keystroke("C-a"));
    }

    #[cfg(not(target_family = "wasm"))]
    fn watched_two_panes(active: u64, zoomed: Option<u64>) -> zz_protocol::MuxSnapshot {
        use zz_protocol::{
            Axis, LayoutNode, MuxSnapshot, PaneSnapshot, PaneSummary, SessionId, SessionSnapshot,
            SessionViewer, SplitId, WindowSnapshot,
        };
        let pane = |id: u64| {
            (
                PaneId(id),
                PaneSnapshot {
                    id: PaneId(id),
                    title: format!("pane {id}"),
                    kind: PaneKindSnapshot::Terminal,
                    synchronized_input: false,
                    bell: false,
                    dead: false,
                    dead_status: None,
                    border_colour: None,
                    active_border_colour: None,
                    border_status_text: String::new(),
                    mode: None,
                    status: None,
                    summary: PaneSummary::default(),
                },
            )
        };
        let viewer = |name: &str, is_self: bool| SessionViewer {
            name: name.to_owned(),
            window: WindowId(1),
            is_self,
        };
        MuxSnapshot {
            generation: 1,
            sessions: vec![SessionSnapshot {
                id: SessionId(1),
                name: "work".into(),
                active_window: WindowId(1),
                viewers: vec![viewer("phone", true), viewer("desktop", false)],
                windows: vec![WindowSnapshot {
                    id: WindowId(1),
                    index: 0,
                    name: "work".into(),
                    automatic_rename: true,
                    active_pane: PaneId(active),
                    zoomed_pane: zoomed.map(PaneId),
                    layout: LayoutNode::Split {
                        id: SplitId(1),
                        axis: Axis::Horizontal,
                        ratio: 0.5,
                        first: Box::new(LayoutNode::Pane(PaneId(1))),
                        second: Box::new(LayoutNode::Pane(PaneId(3))),
                    },
                    panes: [pane(1), pane(3)].into(),
                    layout_dump: String::new(),
                    visible_layout_dump: String::new(),
                    status_label: String::new(),
                    activity: false,
                    silence: false,
                    pane_border_status: zz_protocol::PaneBorderStatus::default(),
                    pane_border_lines: zz_protocol::PaneBorderLines::default(),
                    pane_border_indicators: zz_protocol::PaneBorderIndicators::default(),
                    pane_order: vec![PaneId(1), PaneId(3)],
                    pane_z_order: vec![PaneId(1), PaneId(3)],
                    floating: Vec::new(),
                    modal: None,
                    sx: 160,
                    sy: 48,
                    pane_base_index: 0,
                }],
            }],
            focused_window: Some(WindowId(1)),
        }
    }

    #[cfg(not(target_family = "wasm"))]
    #[ala::test]
    fn a_page_change_from_another_device_zooms_only_a_window_that_was_zoomed(
        cx: &mut ala::TestAppContext,
    ) {
        use zz_protocol::{CommandInvocation, Event, EventPayload, ProtocolMessage, SessionId};
        cx.update(|cx| {
            zz_ui::init(cx);
            cx.set_global(penna::UiZoom(1.0));
        });
        let (shell, cx) = cx.add_window_view(AppShell::new);
        shell.update_in(cx, |shell, _, cx| {
            let sent = shell
                .connection
                .update(cx, |connection, _| connection.record_input_for_test());
            shell.connection.update(cx, |connection, cx| {
                connection.connected = true;
                connection.handle_message_for_test(
                    ProtocolMessage::Attached {
                        session: SessionId(1),
                        snapshot: watched_two_panes(1, None),
                        read_only: false,
                        client_flags: String::new(),
                    },
                    cx,
                );
            });
            let render = |shell: &mut AppShell,
                          snapshot: Option<zz_protocol::MuxSnapshot>,
                          cx: &mut Context<AppShell>| {
                if let Some(snapshot) = snapshot {
                    shell.connection.update(cx, |connection, cx| {
                        connection.handle_message_for_test(
                            ProtocolMessage::Event(Event {
                                sequence: 1,
                                payload: EventPayload::Snapshot(snapshot),
                            }),
                            cx,
                        );
                    });
                }
                let (pages, current) = pages(&shell.status_model(cx), &shell.unseen_agents);
                shell.follow_compact_page(pages.get(current).map(|page| page.pane), cx);
            };
            let zooms = || {
                sent.borrow()
                    .iter()
                    .filter_map(|message| match message {
                        ProtocolMessage::CommandRequest(request)
                            if request.command.name == "resize-pane" =>
                        {
                            Some(request.command.clone())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };

            render(shell, None, cx);
            render(shell, Some(watched_two_panes(3, None)), cx);
            assert_eq!(zooms(), Vec::<CommandInvocation>::new());

            render(shell, Some(watched_two_panes(1, Some(1))), cx);
            render(shell, Some(watched_two_panes(3, None)), cx);
            assert_eq!(
                zooms(),
                [CommandInvocation::new("resize-pane", ["-Z", "-t", "%3"])]
            );
        });
    }
}
