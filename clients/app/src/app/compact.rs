use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use ala::{
    AnyElement, App, Bounds, Context, Corners, DispatchPhase, Entity, Focusable as _,
    HitboxBehavior, IntoElement, Keystroke, ParentElement as _, PinchEvent, Pixels, ScrollDelta,
    ScrollHandle, ScrollWheelEvent, SharedString, Stateful, Styled as _, Subscription, TouchPhase,
    Window, canvas, div, linear_color_stop, linear_gradient, point, prelude::*, px,
};
use zz_client::StatusBarModel;
use zz_protocol::{InputMessage, PaneId, PaneKindSnapshot, WindowId};
use zz_terminal::KeyAction;
use zz_ui::{
    ActiveTheme as _, Colorize as _, IconName, PaintState,
    command::tmux_keystroke,
    compact::{
        ArrowPadEvent, BENTO_INSET, Instant, KEY_ROW_HEIGHT, KeyRow, Lift, LiftEvent, PageDot,
        Pager, PagerEvent, PillLabel, PillMenuAlign, PillMenuItem, PillMenuLook, PillPlacement,
        PillSpring, PopoverKey, PopoverKeyEvent, ToolKeys, WhichKeyList, bottom_sheet, close_sheet,
        compact_floating_bar, compact_floating_pill, compact_hud, compact_pill_collapse,
        compact_pill_label, compact_pill_menu, compact_pill_morph, mini_pill_width, page_dots_in,
        pill_bounds, pill_corner, pill_glass_over,
    },
    kbd::Kbd,
    roles,
    which_key::{WhichKeyCap, WhichKeyRow},
};

use super::AppShell;
use crate::terminal::{TerminalDisplayPreferences, TerminalText};

mod overview;

const PAGE_GAP: f32 = 12.0;
const TOP_FADE: f32 = 12.0;
const SCRIM_ALPHA: f32 = 0.8;
const PINCH_STEPS: f32 = 20.0;
const SCALE_HUD_LINGER: Duration = Duration::from_secs(1);
#[cfg(any(target_os = "ios", target_os = "android"))]
const NOTICE_LINGER: Duration = Duration::from_secs(2);
const LANDING_WAIT: Duration = Duration::from_secs(1);
const BAR_DROP: f32 = 15.0;
const BAR_FADE: f32 = 2.2;
const COLLAPSE_TRAVEL: f32 = 24.0;
const LIFT_FALLBACK_TOP: f32 = 140.0;
const LIFT_FALLBACK_HEIGHT: f32 = 272.0;

const NEW_PANE: &str = "new-pane";
const NEW_WINDOW: &str = "new-window";
const RENAME_PANE: &str = "rename-pane";
const LAST_PANE: &str = "last-pane";
const KILL_PANE: &str = "kill-pane";
const ALL_BINDINGS: &str = "all-bindings";
const OVERVIEW: &str = "overview";
const LENS: &str = "lens";
const NEW_TERMINAL: &str = "new-terminal";
const NEW_BROWSER: &str = "new-browser";
const NEW_SESSION: &str = "new-session";
const ALL_HOSTS: &str = "all-hosts";
const SETTINGS: &str = "settings";
const BINDINGS_SHEET: &str = "compact-bindings-sheet";

#[derive(Default)]
pub(super) struct CompactState {
    pager: Pager,
    width: f32,
    tool_keys: Option<(ToolKeys, [Subscription; 4])>,
    bindings: bool,
    preview: Option<bool>,
    zoom_intent: Option<PaneId>,
    compact_keyboard: Option<bool>,
    last_current: Option<PaneId>,
    previous: Option<PaneId>,
    pinch: Option<(f32, f32)>,
    scale_hud: Option<Instant>,
    notice: Option<(PaneId, &'static str, PillNotice, Instant)>,
    safe_bottom: Pixels,
    keyboard_overlap: Pixels,
    keyboard_moving: bool,
    extra_height: Pixels,
    pub(super) overview: bool,
    add_host: bool,
    host_pager: Pager,
    host_scrolls: BTreeMap<usize, ScrollHandle>,
    hosts_scroll: ScrollHandle,
    host_tiles: Rc<RefCell<BTreeMap<usize, Bounds<Pixels>>>>,
    overview_reveal: bool,
    lift: Lift,
    hosts: Lift,
    morph: PillSpring,
    collapse: PillSpring,
    collapse_travel: f32,
    scroll_ignored: Rc<Cell<bool>>,
    landing: Option<(PaneId, Instant)>,
    cards: Rc<RefCell<BTreeMap<PaneId, Bounds<Pixels>>>>,
    text: Option<TerminalText>,
    page_background: Option<ala::Hsla>,
    lens_dismissed: BTreeSet<PaneId>,
    menu: Option<(MenuFrom, PillSpring)>,
    pill_at: Rc<Cell<Bounds<Pixels>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuFrom {
    Pill,
    Prefix,
    Host,
}

impl CompactState {
    pub(super) fn remember_lens(&mut self, pane: PaneId, open: bool) {
        if open {
            self.lens_dismissed.remove(&pane);
        } else {
            self.lens_dismissed.insert(pane);
        }
    }

    #[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
    pub(super) fn forget_panes(&mut self) {
        self.notice = None;
        self.landing = None;
        self.cards.borrow_mut().clear();
        self.host_scrolls.clear();
    }

    pub(super) fn reset_attachment(&mut self) {
        self.preview = None;
        self.zoom_intent = None;
        self.last_current = None;
        self.previous = None;
        self.bindings = std::env::var("ZZ_ALA_BENCH").is_ok_and(|bench| bench.starts_with("sheet"));
        self.menu = None;
        self.close_sheets();
    }

    pub(super) fn close_sheets(&mut self) {
        self.add_host = false;
    }
}

fn pill_menu(lens: Option<zz_ui::pane::PaneLens>) -> Vec<Vec<PillMenuItem>> {
    let mut panes = vec![
        PillMenuItem::new(RENAME_PANE, "Rename pane", IconName::Pencil),
        PillMenuItem::new(NEW_WINDOW, "New window", IconName::AppWindow),
        PillMenuItem::new(NEW_PANE, "New pane", IconName::Plus),
        PillMenuItem::new(LAST_PANE, "Last pane", IconName::History),
        PillMenuItem::new(OVERVIEW, "Overview", IconName::LayoutDashboard),
    ];
    if let Some(lens) = lens {
        let (label, icon) = if lens.open {
            ("Terminal view", IconName::SquareTerminal)
        } else {
            ("Lens view", IconName::Eye)
        };
        panes.push(PillMenuItem::new(LENS, label, icon).look(PillMenuLook::Primary));
    }
    vec![
        vec![
            PillMenuItem::new(ALL_BINDINGS, "All bindings", IconName::Keyboard)
                .look(PillMenuLook::Quiet),
            PillMenuItem::new(KILL_PANE, "Kill pane", IconName::Xmark).look(PillMenuLook::Danger),
        ],
        panes,
    ]
}

fn host_menu(connected: bool) -> Vec<Vec<PillMenuItem>> {
    let new = [
        PillMenuItem::new(NEW_SESSION, "New session", IconName::Layers),
        PillMenuItem::new(NEW_BROWSER, "New browser", IconName::Globe),
        PillMenuItem::new(NEW_TERMINAL, "New terminal", IconName::SquareTerminal),
    ];
    let host = connected
        .then_some(new)
        .into_iter()
        .flatten()
        .chain([PillMenuItem::new(ALL_HOSTS, "All hosts", IconName::Server)])
        .collect();
    vec![
        vec![PillMenuItem::new(SETTINGS, "Settings", IconName::Settings).look(PillMenuLook::Quiet)],
        host,
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
pub(super) enum PillNotice {
    Done,
    Hint,
    Failed,
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

pub(super) fn pinch_font_scale(start: f32, pinch: f32) -> f32 {
    let scale = (start * pinch * PINCH_STEPS).round() / PINCH_STEPS;
    if scale.is_finite() {
        scale.clamp(0.5, 3.0)
    } else {
        start
    }
}

pub(super) fn dot_groups(pages: &[Page], current: usize) -> Vec<Vec<PageDot>> {
    let mut groups: Vec<Vec<PageDot>> = Vec::new();
    let mut previous = None;
    for (index, page) in pages.iter().enumerate() {
        if previous != Some(page.window) {
            groups.push(Vec::new());
            previous = Some(page.window);
        }
        if let Some(group) = groups.last_mut() {
            group.push(PageDot {
                active: index == current,
                attention: page.attention && index != current,
            });
        }
    }
    groups
}

pub(super) const fn kind_icon(kind: &PaneKindSnapshot) -> IconName {
    match kind {
        PaneKindSnapshot::Terminal => IconName::SquareTerminal,
        PaneKindSnapshot::Browser(_) => IconName::Globe,
        PaneKindSnapshot::Editor(_) => IconName::File,
        PaneKindSnapshot::Picker => IconName::Plus,
    }
}

fn dispatch_key(keystroke: Keystroke, window: &mut Window, cx: &mut App) {
    window.defer(cx, move |window, cx| {
        window.dispatch_keystroke(keystroke, cx);
    });
}

fn send_chord(
    _: &mut AppShell,
    _: &Entity<PopoverKey>,
    event: &PopoverKeyEvent,
    window: &mut Window,
    cx: &mut Context<AppShell>,
) {
    if let PopoverKeyEvent::Pick(id) = event
        && let Ok(keystroke) = Keystroke::parse(id)
    {
        let keystroke = zz_ui::compact::StickyModifiers::apply(&keystroke, cx);
        dispatch_key(keystroke, window, cx);
    }
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

fn binding_rows(core: &zz_client::ClientCore, prefix: &str) -> Arc<[WhichKeyRow]> {
    let cap = |keys: &[String], yours: bool| WhichKeyCap {
        keys: keys
            .iter()
            .map(|key| tmux_keystroke(key))
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default(),
        raw: keys.join(" ").into(),
        yours,
    };
    zz_client::which_key::action_rows(core.key_tables(), "prefix", prefix)
        .into_iter()
        .filter(|row| tmux_keystroke(row.first_key()).is_some())
        .map(|row| WhichKeyRow {
            id: row.first_key().to_owned().into(),
            caps: row
                .keys
                .iter()
                .map(|set| cap(&set.keys, set.yours))
                .collect(),
            label: row.label.into(),
            group: row
                .group
                .map(|group| SharedString::new_static(group.title())),
            repeat: row.repeat,
        })
        .collect()
}

impl AppShell {
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
        }
        if !compact {
            #[cfg(any(target_os = "ios", target_os = "android"))]
            ala_platform::mobile::set_status_bar_on_dark(None);
            zz_ui::compact::StickyModifiers::take(cx);
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
        let model = self.status_model(cx);
        let (pages, _) = pages(&model, &self.unseen_agents);
        if let Some(page) = pages.get(index) {
            self.compact_select(page, cx);
        }
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
        let subscriptions = [
            cx.subscribe_in(
                &keys.arrows,
                window,
                |_, _, event: &ArrowPadEvent, window, cx| {
                    let ArrowPadEvent::Arrow(direction) = event;
                    if let Ok(keystroke) = Keystroke::parse(direction.key()) {
                        let keystroke = zz_ui::compact::StickyModifiers::apply(&keystroke, cx);
                        dispatch_key(keystroke, window, cx);
                    }
                },
            ),
            cx.subscribe_in(&keys.control, window, send_chord),
            cx.subscribe_in(&keys.alt, window, send_chord),
            cx.subscribe_in(
                &keys.prefix,
                window,
                |this, _, event: &PopoverKeyEvent, _, cx| {
                    if *event == PopoverKeyEvent::Press {
                        this.open_pill_menu(MenuFrom::Prefix, cx);
                    }
                },
            ),
        ];
        self.compact.tool_keys = Some((keys.clone(), subscriptions));
        keys
    }

    fn open_pill_menu(&mut self, from: MenuFrom, cx: &mut Context<Self>) {
        let mut spring = PillSpring::shut();
        spring.aim(true, Instant::now());
        self.compact.menu = Some((from, spring));
        cx.notify();
    }

    fn close_pill_menu(&mut self, cx: &mut Context<Self>) {
        if let Some((_, spring)) = self.compact.menu.as_mut() {
            spring.aim(false, Instant::now());
            cx.notify();
        }
    }

    fn pill_menu_pick(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.close_pill_menu(cx);
        let new = match id {
            NEW_TERMINAL => Some(overview::NewThing::Terminal),
            NEW_BROWSER => Some(overview::NewThing::Browser),
            NEW_SESSION => Some(overview::NewThing::Session),
            _ => None,
        };
        if let Some(new) = new {
            self.compact.menu = None;
            self.create(new, window, cx);
            return;
        }
        match id {
            ALL_HOSTS => {
                self.compact.menu = None;
                self.open_hosts(cx);
            }
            SETTINGS => {
                self.compact.menu = None;
                self.open_settings(cx);
            }
            OVERVIEW => {
                self.compact.menu = None;
                window.dismiss_virtual_keyboard();
                self.open_overview(cx);
            }
            LENS => {
                if let Some(pane) = self.active_window(cx).map(|window| window.active_pane) {
                    self.toggle_lens(pane, window, cx);
                }
            }
            _ => self.prefix_action(id, cx),
        }
    }

    fn prefix_action(&mut self, id: &str, cx: &mut Context<Self>) {
        let connection = self.connection.read(cx);
        if !connection.connected || connection.core.attached_read_only() {
            return;
        }
        let Some(pane) = self.active_window(cx).map(|window| window.active_pane) else {
            return;
        };
        let target = pane.to_string();
        let here = || vec!["-c".to_owned(), "#{pane_current_path}".to_owned()];
        match id {
            NEW_PANE => {
                let mut args = vec!["-h".to_owned(), "-t".to_owned(), target];
                args.extend(here());
                self.command("split-window", args, cx);
            }
            NEW_WINDOW => self.command("new-window", here(), cx),
            RENAME_PANE => self.command(
                "command-prompt",
                vec![
                    "-I".into(),
                    "#{pane_title}".into(),
                    "-p".into(),
                    "rename pane:".into(),
                    format!("select-pane -t {target} -T '%%'"),
                ],
                cx,
            ),
            LAST_PANE => {
                let model = self.status_model(cx);
                let (pages, _) = pages(&model, &self.unseen_agents);
                if let Some(page) = self
                    .compact
                    .previous
                    .and_then(|previous| pages.iter().find(|page| page.pane == previous))
                {
                    self.compact_select(page, cx);
                }
            }
            KILL_PANE => self.command("kill-pane", vec!["-t".into(), target], cx),
            ALL_BINDINGS => self.compact.bindings = true,
            _ => {}
        }
        cx.notify();
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
                    cx.set_global(TerminalDisplayPreferences {
                        font_family: self.preferences.terminal_font_family.clone(),
                        font_scale: scale,
                    });
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
        StatusBarModel::from_snapshot(
            core.snapshot(),
            core.attached_session(),
            None,
            self.preferences.status_bar_settings(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn compact_page(
        &mut self,
        slot: &Page,
        current: bool,
        motion: f32,
        top_inset: Pixels,
        reserve: Pixels,
        hole: Option<(Bounds<Pixels>, Pixels)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let _ = hole;
        let radius = cx.theme().layer(roles::PANE, PaintState::Rest).radius * motion;
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
        let lens = matches!(pane.kind, PaneKindSnapshot::Terminal)
            .then(|| self.shown_lens(pane_id, window, cx))
            .flatten();
        let content = match &pane.kind {
            PaneKindSnapshot::Terminal if let Some(lens) = lens => {
                lens.update(cx, |lens, cx| {
                    lens.set_corner_radii(
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
                    lens.read(cx).focus_handle(cx).focus(window, cx);
                    self.focused_pane = Some(pane_id);
                }
                lens.into_any_element()
            }
            PaneKindSnapshot::Terminal => {
                let terminal = self.pane_terminal(pane_id, window, cx);
                if can_focus && self.focused_pane != Some(pane_id) {
                    terminal.read(cx).focus_handle(cx).focus(window, cx);
                    self.focused_pane = Some(pane_id);
                }
                terminal.update(cx, |terminal, cx| {
                    terminal.set_extra_height(self.compact.extra_height, cx);
                    terminal.set_text_dimmed(false, 1.0, cx);
                    terminal.set_pane_status(dead.clone(), pane.synchronized_input, false, cx);
                    terminal.set_corner_radii(
                        Corners {
                            top_left: px(0.0),
                            top_right: px(0.0),
                            bottom_left: radius,
                            bottom_right: radius,
                        },
                        cx,
                    );
                    terminal.set_rows_above(true, cx);
                    terminal.set_rows_below(true, cx);
                });
                background = terminal.read(cx).fill(cx);
                rows_above = terminal.read(cx).rows_above_shown(cx);
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
                background = cx.theme().pane_color(cx.theme().background);
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
            PaneKindSnapshot::Picker => {
                let picker = self.picker_entity(pane_id, cx);
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
        let fade = top_inset + px(TOP_FADE);
        let scrim = cx.theme().background.opaque().blend(background);
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .pt(top_inset)
            .when(!browser, |page| page.pb(reserve))
            .bg(background)
            .rounded(radius)
            .child(div().flex_1().min_h_0().min_w_0().child(content))
            .when(rows_above && top_inset > px(0.0), |page| {
                page.child(
                    div()
                        .debug_selector(|| "compact-top-fade".to_owned())
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(fade)
                        .bg(linear_gradient(
                            180.0,
                            linear_color_stop(
                                scrim.alpha(SCRIM_ALPHA),
                                1.0 - TOP_FADE / f32::from(fade),
                            ),
                            linear_color_stop(scrim.alpha(0.0), 1.0),
                        )),
                )
            })
            .into_any_element()
    }

    /// Counts how far the current page's content moved toward its end
    /// (negative toward its start), shrinking the pill once it travels far
    /// enough one way and bringing it back once it travels the other way.
    pub(super) fn compact_content_scrolled(&mut self, travel: f32, cx: &mut Context<Self>) {
        if travel == 0.0 || !travel.is_finite() {
            return;
        }
        if self.compact.collapse_travel.signum() != travel.signum() {
            self.compact.collapse_travel = 0.0;
        }
        self.compact.collapse_travel += travel;
        let full = self.compact.collapse.full();
        if (full && self.compact.collapse_travel > COLLAPSE_TRAVEL)
            || (!full && self.compact.collapse_travel < -COLLAPSE_TRAVEL)
        {
            self.compact.collapse.aim(!full, Instant::now());
            self.compact.collapse_travel = 0.0;
            cx.notify();
        }
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
        self.expand_pill(cx);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTICE_LINGER).await;
            this.update(cx, |_, cx| cx.notify()).ok();
        })
        .detach();
        cx.notify();
    }

    pub(super) fn expand_pill(&mut self, cx: &mut Context<Self>) {
        self.compact.collapse_travel = 0.0;
        if !self.compact.collapse.full() {
            self.compact.collapse.aim(true, Instant::now());
            cx.notify();
        }
    }

    fn compact_scroll_watch(&self, pill: Bounds<Pixels>, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.weak_entity();
        let ignored = self.compact.scroll_ignored.clone();
        canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |_, hitbox, window, _| {
                window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    if event.touch_phase == TouchPhase::Started {
                        ignored.set(pill.contains(&event.position));
                    }
                    if ignored.get() || !hitbox.should_handle_scroll(window) {
                        return;
                    }
                    let delta = event.delta.pixel_delta(window.line_height());
                    if delta.y.abs() <= delta.x.abs() {
                        return;
                    }
                    let _ = view.update(cx, |this, cx| {
                        this.compact_content_scrolled(-f32::from(delta.y), cx);
                    });
                });
            },
        )
        .absolute()
        .inset_0()
        .into_any_element()
    }

    fn compact_can_focus(&self, window: &Window, cx: &App) -> bool {
        let core = &self.connection.read(cx).core;
        self.settings.is_none()
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
            self.compact.landing = None;
            self.compact.overview_reveal = true;
        }
        if let Some(event) = response.event {
            window.end_touch_momentum();
            self.compact.overview = event == LiftEvent::Open;
        }
        cx.notify();
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub(super) fn compact_visible_panes(&self, cx: &App) -> BTreeSet<PaneId> {
        if self.compact_lifted() || self.compact.bindings {
            return BTreeSet::new();
        }
        if self.compact.menu.is_some() {
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

    fn open_default_lenses(&mut self, pages: &[Page], cx: &mut Context<Self>) {
        if !super::settings::phone() {
            return;
        }
        let connection = self.connection.read(cx);
        self.compact
            .lens_dismissed
            .retain(|pane| connection.terminal_agent(*pane).is_some());
        let closed = pages
            .iter()
            .map(|page| page.pane)
            .filter(|pane| {
                connection.terminal_agent(*pane).is_some()
                    && !connection.agent_lens_open(*pane)
                    && !self.compact.lens_dismissed.contains(pane)
            })
            .collect::<Vec<_>>();
        for pane in closed {
            self.connection
                .update(cx, |connection, cx| connection.open_agent_lens(pane, cx));
        }
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
        self.open_default_lenses(&pages, cx);
        let current_pane = pages.get(current).map(|page| page.pane);
        if self.compact.last_current.is_some() && current_pane != self.compact.last_current {
            self.compact.zoom_intent = current_pane;
            self.compact.previous = self.compact.last_current;
            self.compact.collapse.aim(true, Instant::now());
        }
        self.compact.last_current = current_pane;
        self.reconcile_compact_zoom(cx);
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
        let placement = PillPlacement::new(screen_radius(window), self.compact.safe_bottom);
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
                || self.connection.read(cx).agent_lens_open(page.pane)
        });
        self.compact
            .morph
            .aim(!keyboard_heading_up(window) || keyless, now);
        if cx.reduce_motion() {
            self.compact.morph.finish();
        }
        if self.compact.morph.tick(now) {
            window.request_animation_frame();
        }
        let morph = self.compact.morph.progress();
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
        if keyboard || open || self.compact.pager.is_moving() {
            self.compact.collapse.aim(true, now);
        }
        if cx.reduce_motion() {
            self.compact.collapse.finish();
        }
        if self.compact.collapse.tick(now) {
            window.request_animation_frame();
        }
        let collapse = self.compact.collapse.progress();
        let shown = self.compact.pager.index();
        let shown_page = pages.get(shown);
        let short: SharedString = shown_page
            .map(|page| match self.browser_page(page, cx) {
                Some((_, host)) if !host.is_empty() => host.into(),
                Some((title, _)) => title,
                None => page.title.clone(),
            })
            .unwrap_or_default();
        let floating = !keyboard;
        let bar_width = px(width);
        let mini = mini_pill_width(&short, window, cx);
        let pill_shown = pill_bounds(collapse, bar_width, mini, &placement);
        let pill_rest = pill_bounds(
            if self.compact.collapse.full() {
                1.0
            } else {
                0.0
            },
            bar_width,
            mini,
            &placement,
        );
        let zone_top = window.viewport_size().height - zone;
        let in_window = |bounds: Bounds<Pixels>| {
            Bounds::new(
                point(bounds.origin.x, bounds.origin.y + zone_top),
                bounds.size,
            )
        };
        let hole = (floating && morph >= 1.0).then(|| {
            (
                in_window(pill_shown),
                pill_corner(collapse, pill_shown, &placement, cx),
            )
        });
        let reserve = if floating { zone } else { px(0.0) };
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
                .when(floating, |area| {
                    area.child(self.compact_scroll_watch(in_window(pill_rest), cx))
                })
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
        if !keyboard && matches!(self.compact.menu, Some((MenuFrom::Prefix, _))) {
            self.compact.menu = None;
        }
        if let Some((_, spring)) = self.compact.menu.as_mut() {
            if cx.reduce_motion() {
                spring.finish();
            }
            if spring.tick(now) {
                window.request_animation_frame();
            }
            if !spring.full() && spring.progress() <= 0.0 {
                self.compact.menu = None;
            }
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            let frozen = matches!(
                self.compact.menu,
                Some((MenuFrom::Pill | MenuFrom::Prefix, _))
            )
            .then_some(current_pane)
            .flatten();
            for (pane, browser) in &self.browsers {
                let still = frozen == Some(*pane);
                browser.update(cx, |browser, cx| browser.set_frozen(still, window, cx));
            }
        }
        let menu_progress = match self.compact.menu {
            Some((MenuFrom::Pill, spring)) => spring.progress().clamp(0.0, 1.0),
            _ => 0.0,
        };
        let pill_at = self.compact.pill_at.clone();
        let track_pill = move || {
            let at = pill_at.clone();
            canvas(move |bounds, _, _| at.set(bounds), |_, (), _, _| {})
                .absolute()
                .inset_0()
        };
        let page_background = self
            .compact
            .page_background
            .unwrap_or(cx.theme().background);
        let (mut title, meta, mut warning) = self.pill_text(&model, shown_page, cx);
        let mut icon = shown_page.map_or(IconName::SquareTerminal, |page| kind_icon(&page.kind));
        if let Some((pane, text, kind, until)) = self.compact.notice
            && until > now
            && shown_page.is_some_and(|page| page.pane == pane)
        {
            title = text.into();
            warning = kind == PillNotice::Failed;
            if kind == PillNotice::Done {
                icon = IconName::Check;
            }
        }
        let label = PillLabel {
            icon,
            title,
            meta,
            warning,
        };
        let floating = !keyboard && morph >= 1.0;
        let backdrop = shown_page
            .filter(|_| floating)
            .and_then(|page| self.browser_backdrop(page, cx))
            .or(self.compact.page_background.filter(|_| floating));
        let pill_placement = PillPlacement {
            glass: backdrop.and_then(|backdrop| pill_glass_over(backdrop, cx)),
            ..placement
        };
        let dots = page_dots_in(&dot_groups(&pages, shown), cx.theme().foreground, cx);
        let bottom = if keyboard && keyless {
            let pill = compact_floating_pill("compact-pill", label, dots, &placement, cx)
                .opacity(1.0 - menu_progress)
                .child(track_pill())
                .on_click(cx.listener(|this, _, _, cx| this.open_pill_menu(MenuFrom::Pill, cx)));
            div()
                .relative()
                .flex_none()
                .w_full()
                .h((zone - overlap).max(px(0.0)))
                .overflow_hidden()
                .bg(page_background)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .child(compact_floating_bar(pill, &placement, cx)),
                )
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
                let label = compact_pill_label(label, dots, &placement, cx);
                area.child(compact_pill_morph(
                    row, label, morph, pill_top, height, &placement, cx,
                ))
            } else {
                area.pb(lift).child(row)
            }
            .into_any_element()
        } else if morph < 1.0 {
            let row = self.compact_key_row(window, cx);
            let label = compact_pill_label(label, dots, &placement, cx);
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(zone)
                .child(compact_pill_morph(
                    row,
                    label,
                    morph,
                    placement.pill_top(),
                    zone,
                    &placement,
                    cx,
                ))
                .into_any_element()
        } else {
            let label = compact_pill_label(label, dots, &pill_placement, cx);
            let drop = if lifting { progress } else { 0.0 };
            let full = self.compact.collapse.full();
            let lowered = Bounds::new(
                point(
                    pill_shown.origin.x,
                    pill_shown.origin.y + (zone + px(BAR_DROP)) * drop,
                ),
                pill_shown.size,
            );
            let fade = (1.0 - drop.min(1.0) * BAR_FADE).max(0.0);
            let pill = compact_pill_collapse(
                "compact-pill",
                label,
                short,
                collapse,
                lowered,
                bar_width,
                &pill_placement,
                cx,
            )
            .opacity(fade * (1.0 - menu_progress))
            .child(track_pill())
            .on_click(cx.listener(move |this, _, _, cx| {
                if full {
                    this.open_pill_menu(MenuFrom::Pill, cx);
                } else {
                    this.expand_pill(cx);
                }
            }))
            .on_scroll_wheel(cx.listener(
                |this, event: &ScrollWheelEvent, window, cx| {
                    this.compact_scroll(event, window, cx);
                },
            ));
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(zone)
                .child(
                    div()
                        .id("compact-bar-zone")
                        .absolute()
                        .left(pill_rest.origin.x)
                        .top(pill_rest.origin.y)
                        .w(pill_rest.size.width)
                        .h(pill_rest.size.height)
                        .on_scroll_wheel(cx.listener(Self::compact_lift_scroll)),
                )
                .child(pill)
                .into_any_element()
        };
        let mut sheets = Vec::new();
        let sheet_inset = if keyboard { px(0.0) } else { safe_bottom };
        if self.compact.bindings {
            sheets.push(self.compact_bindings_sheet(sheet_inset, cx));
        }
        if !self.compact.pager.is_moving()
            && self.compact_can_focus(window, cx)
            && let Some(page) = pages
                .get(self.compact.pager.index())
                .filter(|page| matches!(page.kind, PaneKindSnapshot::Picker))
        {
            let picker = self.picker_entity(page.pane, cx);
            sheets.extend(picker.update(cx, |picker, cx| picker.sheet(sheet_inset, cx)));
        }
        let screen = div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(pager_area)
            .child(bottom);
        let content = if open {
            #[cfg(any(target_os = "ios", target_os = "android"))]
            if self.compact.add_host {
                sheets.push(self.overview_add_host_sheet(safe_bottom, cx));
            }
            let (pages, bars) = self.overview_layers(&model, top_inset, &placement, window, cx);
            div()
                .relative()
                .size_full()
                .child(pages)
                .child(bars)
                .into_any_element()
        } else if lifting {
            let (overview, bars) = self.overview_layers(&model, top_inset, &placement, window, cx);
            let target = self
                .compact
                .landing
                .map(|(pane, _)| pane)
                .or(current_pane)
                .and_then(|pane| pages.iter().find(|page| page.pane == pane).cloned());
            let stage = window.viewport_size();
            let card = target
                .as_ref()
                .and_then(|page| self.compact.cards.borrow().get(&page.pane).copied())
                .unwrap_or_else(|| {
                    Bounds::new(
                        point(px(BENTO_INSET), top_inset + px(LIFT_FALLBACK_TOP)),
                        ala::size(
                            stage.width - px(BENTO_INSET * 2.0),
                            px(LIFT_FALLBACK_HEIGHT),
                        ),
                    )
                });
            let child = target.map_or_else(
                || div().into_any_element(),
                |page| self.compact_page(&page, false, 0.0, top_inset, zone, None, window, cx),
            );
            div()
                .relative()
                .size_full()
                .child(overview::zoom_layers(
                    overview,
                    child,
                    &overview::Zoom {
                        tile: card,
                        stage,
                        progress,
                        ground: page_background,
                    },
                ))
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .opacity(progress.clamp(0.0, 1.0))
                        .child(bars),
                )
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
        let menu = self.compact.menu.map(|(from, spring)| {
            let (close, align) = match from {
                MenuFrom::Pill | MenuFrom::Host => {
                    (self.compact.pill_at.get(), PillMenuAlign::Center)
                }
                MenuFrom::Prefix => (
                    self.compact
                        .tool_keys
                        .as_ref()
                        .map(|(keys, _)| keys.prefix.read(cx).bounds())
                        .unwrap_or_default(),
                    PillMenuAlign::Start,
                ),
            };
            let items = if from == MenuFrom::Host {
                host_menu(self.shown_host_connected(cx))
            } else {
                pill_menu(shown_page.and_then(|page| self.pane_lens(page.pane, cx)))
            };
            let pick = cx.weak_entity();
            let shut = cx.weak_entity();
            compact_pill_menu(
                items,
                close,
                top_inset,
                align,
                spring.progress(),
                move |id, window, cx| {
                    let _ = pick.update(cx, |this, cx| this.pill_menu_pick(id, window, cx));
                },
                move |_, cx| {
                    let _ = shut.update(cx, Self::close_pill_menu);
                },
                cx,
            )
        });
        div()
            .id("compact-shell")
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .child(content)
            .children(menu)
            .children(sheets)
            .child(zz_ui::touch::touch_scale(
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

    fn pill_text(
        &self,
        model: &StatusBarModel,
        page: Option<&Page>,
        cx: &App,
    ) -> (SharedString, SharedString, bool) {
        let Some(page) = page else {
            return (SharedString::default(), SharedString::default(), false);
        };
        let waiting = self
            .connection
            .read(cx)
            .core
            .agent_state(page.pane)
            .is_some_and(|state| {
                zz_client::agent_attention_status(state)
                    == zz_client::AgentAttentionStatus::NeedsInput
            });
        if waiting {
            return (page.title.clone(), "Needs your permission".into(), true);
        }
        let browser = self.browser_page(page, cx);
        if let Some((title, host)) = &browser
            && !host.is_empty()
        {
            return (title.clone(), host.clone().into(), false);
        }
        let title = browser.map_or_else(|| page.title.clone(), |(title, _)| title);
        let core = &self.connection.read(cx).core;
        let meta = core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find(|window| window.id == page.window)
            .and_then(|window| {
                window.panes.get(&page.pane).map(|pane| {
                    zz_client::window_summary::pane_detail(window, pane, &title)
                        .into_iter()
                        .map(|part| part.text)
                        .collect::<String>()
                })
            })
            .or_else(|| {
                model
                    .windows
                    .iter()
                    .find(|window| window.id == page.window)
                    .map(|window| window.label.clone())
            })
            .unwrap_or_default();
        (title, meta.into(), false)
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
        KeyRow::new(keys.row())
            .lead(keys.lead())
            .track_scroll(&keys.scroll)
            .on_key(|keystroke, window, cx| dispatch_key(keystroke.clone(), window, cx))
            .on_hide(|window, _| window.dismiss_virtual_keyboard())
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
        .surface_role(roles::BINDINGS_DRAWER)
        .header_role(roles::PHONE_BAR)
        .into_any_element()
    }
}

#[cfg(target_os = "android")]
enum BackTarget {
    Sheet(&'static str),
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
        if self.compact.add_host {
            return Some(BackTarget::Sheet(super::hosts::ADD_HOST_SHEET));
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
        if self.compact.menu.is_some() {
            return Some(BackTarget::Menu);
        }
        let core = &self.connection.read(cx).core;
        let menu = window.focused(cx).is_some_and(|focus| {
            cx.build_action("zz_menu::Cancel", None)
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
            .root::<zz_ui::Root>()
            .flatten()
            .is_some_and(|root| root.read(cx).has_active_dialog());
        match self.back_target(dialog, window, cx) {
            Some(BackTarget::Sheet(id)) => close_sheet(id, cx),
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
        assert_eq!(top(30.0), zone);
        assert_eq!(top(300.0), px(KEY_ROW_HEIGHT + 300.0));
    }

    #[test]
    fn dots_group_by_window_and_flag_attention_off_page() {
        let page = |pane: u32, window: u32, attention: bool| Page {
            pane: PaneId(pane.into()),
            window: WindowId(window.into()),
            kind: PaneKindSnapshot::Terminal,
            title: SharedString::default(),
            attention,
        };
        let pages = [
            page(0, 1, false),
            page(1, 1, true),
            page(2, 2, true),
            page(3, 3, false),
        ];
        let groups = dot_groups(&pages, 2);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].len(), 2);
        assert!(groups[0][1].attention);
        assert!(groups[1][0].active);
        assert!(!groups[1][0].attention);
    }
}
