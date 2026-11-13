use std::{ops::Range, rc::Rc};

use ala::{
    AnyElement, App, Bounds, Context, Entity, Font, Hsla, PaintScaleExt as _, ParentElement as _,
    Pixels, ScrollDelta, ScrollWheelEvent, Size, StyledText, TextRun, Window, canvas, div, font,
    linear_color_stop, linear_gradient, point, prelude::*, px, size,
};
use penna::{
    ActiveTheme as _, Colorize as _, IconName, Sizable as _, Space,
    blocks::compact::{
        BENTO_INSET, BentoSpan, COMPACT_TITLE_BAR_HEIGHT, MiniPane, PhoneTile, PillLabel,
        PillPlacement, TopRowEnd, bento_grid, compact_bar_button, compact_floating_bar,
        compact_floating_group, compact_floating_pill, folding_title_bar, scrolled, top_row_button,
        window_span, zoom_card_ground,
    },
    blocks::settings::large_title,
    blocks::touch::StatusMark,
    blocks::touch::{
        ACTION_BAR_KEYBOARD, PageDot, PagerEvent, PhoneEmpty, StackMenu, TouchHeader, action_bar,
        page_dots,
    },
    components::button::{Button, ButtonVariants as _},
};
#[cfg(any(target_os = "ios", target_os = "android"))]
use penna::{
    Disableable as _,
    blocks::compact::ZoomCard,
    blocks::sheet::{bottom_sheet, close_sheet, sheet_close},
    blocks::touch::TouchRow,
    components::field::Field,
    components::input::Input,
};
use web_time::Instant;
use zz_client::{
    AgentAttentionStatus, StatusBarModel, StatusBarWindow, agent_attention_status,
    navigation::session_label,
    pane_rects,
    window_summary::{DaemonClock, hold_detail, summarize_window},
};
use zz_protocol::{PaneId, PaneKindSnapshot, SessionId, WindowId};
use zz_terminal::{Glyph, LocalColors, TerminalAppearance, TerminalPalette, TerminalViewport};
#[cfg(any(target_os = "ios", target_os = "android"))]
use zz_ui::compact::found_machines;
use zz_ui::{
    compact::{
        HostState, MachineMenu, close_many_detail, close_pane_detail, host_mark, host_state,
        overview_count, session_in_view, session_tail, window_mark,
    },
    terminal::terminal_background,
};

use super::{
    AppShell, Page, pages,
    sheets::{PhoneSheet, Target, Thing},
};
use crate::{connection::Connection, terminal::phone_colors};

const PREVIEW_ROWS: u16 = 40;
const PREVIEW_COLUMNS: usize = 56;
const HOST_GAP: f32 = 12.0;
const ZOOM_FADE_START: f32 = 0.3;
const ZOOM_FADE_SPAN: f32 = 0.4;
const TILE_FALLBACK_TOP: f32 = 160.0;
const TILE_FALLBACK_HEIGHT: f32 = 178.0;
const CARD_EMPTY_TOP: f32 = 96.0;
#[cfg(any(target_os = "ios", target_os = "android"))]
pub(super) const MACHINE_SHEET: &str = "machine-sheet";

fn page_host(url: &str) -> &str {
    if url == "about:blank" {
        return "";
    }
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

fn host_palette(core: &zz_client::ClientCore) -> TerminalPalette {
    core.appearance().map_or_else(
        || TerminalAppearance::default().palette,
        |appearance| appearance.palette,
    )
}

fn terminal_preview(
    viewport: &TerminalViewport,
    local: Option<LocalColors>,
    base: &Font,
    rows: Range<u16>,
    columns: usize,
) -> StyledText {
    let fallback = terminal_background(
        local.map_or(viewport.foreground, |local| local.foreground()),
        1.0,
    );
    let mut text = String::new();
    let mut runs: Vec<TextRun> = Vec::new();
    let mut push = |text: &mut String, start: usize, color: Hsla| {
        let len = text.len() - start;
        match runs.last_mut() {
            Some(run) if run.color == color => run.len += len,
            _ => runs.push(TextRun {
                len,
                font: base.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }),
        }
    };
    let first = rows.start;
    for row in rows {
        if row > first {
            let start = text.len();
            text.push('\n');
            push(&mut text, start, fallback);
        }
        let Some(cells) = viewport.row(row) else {
            continue;
        };
        let cells = &cells[..cells.len().min(columns)];
        let end = cells
            .iter()
            .rposition(|cell| match viewport.glyph(*cell) {
                Glyph::Empty => false,
                Glyph::Scalar(character) => !character.is_whitespace(),
                Glyph::Grapheme(_) => true,
            })
            .map_or(0, |last| last + 1);
        for cell in &cells[..end] {
            let start = text.len();
            viewport.push_glyph(*cell, &mut text);
            if text.len() == start {
                text.push(' ');
            }
            let color = viewport.style(*cell).map_or(fallback, |style| {
                let style = local.map_or(style, |local| local.recolor(style));
                terminal_background(style.foreground(), 1.0)
            });
            push(&mut text, start, color);
        }
    }
    StyledText::new(text).with_runs(runs)
}

pub(super) struct Zoom {
    pub tile: Bounds<Pixels>,
    pub stage: Size<Pixels>,
    pub progress: f32,
    pub ground: Hsla,
    pub corner: Pixels,
}

fn zoom_fade(at: f32) -> f32 {
    ((at - ZOOM_FADE_START) / ZOOM_FADE_SPAN).clamp(0.0, 1.0)
}

pub(super) fn zoom_layers(parent: AnyElement, child: AnyElement, zoom: &Zoom) -> ala::Div {
    let progress = zoom.progress.clamp(0.0, 1.0);
    let width = zoom.stage.width;
    let height = zoom.stage.height;
    let tile = zoom.tile;
    let rest = (f32::from(tile.size.width) / f32::from(width).max(1.0)).clamp(0.05, 1.0);
    let child_scale = 1.0 + (rest - 1.0) * progress;
    let band = height + (tile.size.height / rest - height) * progress;
    let parent_scale = 1.0 / rest + (1.0 - 1.0 / rest) * progress;
    let parent_layer = div()
        .absolute()
        .top_0()
        .left_0()
        .w(width)
        .h(height)
        .opacity(zoom_fade(progress))
        .child(parent)
        .paint_scale(
            parent_scale,
            point(
                -tile.origin.x / rest * (1.0 - progress),
                -tile.origin.y / rest * (1.0 - progress),
            ),
        );
    let child_layer = div()
        .absolute()
        .top_0()
        .left_0()
        .w(width)
        .h(band.max(px(0.0)))
        .overflow_hidden()
        .rounded(zoom.corner / child_scale * progress)
        .bg(zoom.ground)
        .opacity(zoom_fade(1.0 - progress))
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .w(width)
                .h(height)
                .child(child),
        )
        .paint_scale(
            child_scale,
            point(tile.origin.x * progress, tile.origin.y * progress),
        );
    div()
        .absolute()
        .inset_0()
        .child(parent_layer)
        .child(child_layer)
}

#[derive(Default)]
struct HostCounts {
    sessions: usize,
    waiting: usize,
    working: usize,
}

fn host_counts(connection: &Entity<Connection>, cx: &App) -> HostCounts {
    let core = &connection.read(cx).core;
    let mut counts = HostCounts::default();
    for session in &core.snapshot().sessions {
        counts.sessions += 1;
        for window in &session.windows {
            for pane in window.panes.keys() {
                match core.agent_state(*pane).map(agent_attention_status) {
                    Some(AgentAttentionStatus::NeedsInput) => counts.waiting += 1,
                    Some(AgentAttentionStatus::Working) => counts.working += 1,
                    _ => {}
                }
            }
        }
    }
    counts
}

impl AppShell {
    pub(in super::super) fn open_overview(&mut self, cx: &mut Context<Self>) {
        if !self.compact.overview {
            let hosts = self.host_pages(cx);
            let attached = hosts
                .iter()
                .position(|host| host.connection.as_ref() == Some(&self.connection))
                .unwrap_or(0);
            self.compact.host_pager.sync(attached, hosts.len());
            self.compact.host_pager.go_to(attached, Instant::now());
            self.compact.host_pager.finish();
            self.compact.hosts.snap(false);
            self.compact.overview = true;
            self.compact.overview_reveal = true;
            self.compact.landing = None;
            self.compact.lift.animate(true, Instant::now());
        }
        cx.notify();
    }

    pub(in super::super) fn close_overview(&mut self, cx: &mut Context<Self>) {
        self.compact.overview = false;
        self.compact.close_sheets();
        self.compact.landing = None;
        self.compact.lift.snap(false);
        self.compact.hosts.snap(false);
        self.focused_pane = None;
        cx.notify();
    }

    pub(in super::super) fn land_overview(&mut self, pane: Option<PaneId>, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.compact.overview = false;
        self.compact.close_sheets();
        self.compact.landing = pane.map(|pane| (pane, now));
        self.compact.lift.animate(false, now);
        self.compact.hosts.snap(false);
        self.focused_pane = None;
        cx.notify();
    }

    pub(super) fn open_hosts(&mut self, cx: &mut Context<Self>) {
        self.compact.close_sheets();
        self.compact.hosts.animate(true, Instant::now());
        cx.notify();
    }

    fn enter_host(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.show_host(index, window, cx);
        let count = self.host_pages(cx).len();
        self.compact.host_pager.sync(index, count);
        self.compact.host_pager.go_to(index, Instant::now());
        self.compact.host_pager.finish();
        self.compact.overview_reveal = true;
        self.compact.hosts.animate(false, Instant::now());
        cx.notify();
    }

    fn show_host(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(endpoint) = self
            .host_pages(cx)
            .get(index)
            .filter(|host| host.connected(cx))
            .and_then(|host| host.endpoint.clone())
            && !self.is_attached_host(&endpoint, cx)
        {
            self.switch_host(&endpoint, None, None, window, cx);
        }
        let _ = (index, window, cx);
    }

    pub(super) fn overview_layers(
        &mut self,
        model: &StatusBarModel,
        top_inset: Pixels,
        placement: &PillPlacement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (AnyElement, AnyElement) {
        if cx.reduce_motion() {
            self.compact.hosts.finish();
        }
        if self.compact.hosts.tick(Instant::now()) {
            window.request_animation_frame();
        }
        let progress = self.compact.hosts.progress().clamp(0.0, 1.0);
        let moving = self.compact.hosts.is_moving();
        let dragging = self.compact.hosts.is_dragging();
        let host_view =
            (progress < 1.0).then(|| self.host_level(model, top_inset, placement, window, cx));
        let fleet_view =
            (progress > 0.0 || moving).then(|| self.hosts_level(top_inset, placement, window, cx));
        let index = self.compact.host_pager.index();
        let stage = window.viewport_size();
        let pages = match (fleet_view, host_view) {
            (Some(hosts), Some(host)) => {
                let tile = self
                    .compact
                    .host_tiles
                    .borrow()
                    .get(&index)
                    .copied()
                    .unwrap_or_else(|| {
                        Bounds::new(
                            point(px(BENTO_INSET), top_inset + px(TILE_FALLBACK_TOP)),
                            size(
                                stage.width / 2.0 - px(BENTO_INSET),
                                px(TILE_FALLBACK_HEIGHT),
                            ),
                        )
                    });
                zoom_layers(
                    hosts,
                    host,
                    &Zoom {
                        tile,
                        stage,
                        progress,
                        ground: cx.theme().background,
                        corner: cx.theme().phone_radius(),
                    },
                )
                .into_any_element()
            }
            (Some(hosts), None) => hosts,
            (None, Some(host)) => host,
            (None, None) => div().into_any_element(),
        };
        let hosts = self.host_pages(cx);
        let scrim = cx.theme().background.opaque();
        let mut bars = div()
            .id("overview-bars")
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .h(placement.zone())
            .occlude()
            .child(div().absolute().inset_0().bg(linear_gradient(
                180.0,
                linear_color_stop(scrim.alpha(0.0), 0.0),
                linear_color_stop(scrim, 0.55),
            )));
        let unreached = !hosts.get(index).is_some_and(|host| host.connected(cx));
        if unreached && progress <= 0.0 && !moving {
            bars = div().id("overview-bars");
        } else if progress < 1.0 || dragging {
            bars = bars.child(
                div()
                    .absolute()
                    .inset_0()
                    .opacity(1.0 - progress)
                    .child(self.host_bar(&hosts, index, placement, cx)),
            );
        }
        if progress > 0.0 {
            bars = bars.child(
                div()
                    .absolute()
                    .inset_0()
                    .opacity(progress)
                    .child(self.hosts_bar(placement, cx)),
            );
        }
        if moving && !dragging {
            bars = bars.child(
                div()
                    .id("overview-zoom-block")
                    .absolute()
                    .inset_0()
                    .occlude(),
            );
        }
        let pages = div()
            .relative()
            .size_full()
            .child(pages)
            .when(moving, |pages| {
                pages.child(div().id("hosts-zoom-block").absolute().inset_0().occlude())
            })
            .into_any_element();
        if self.compact.card.is_some() {
            let card = self.compact.card_zoom.progress().clamp(0.0, 1.0);
            if card >= 1.0 && !self.compact.card_zoom.is_moving() {
                return (pages, div().into_any_element());
            }
            return (pages, bars.opacity(1.0 - card).into_any_element());
        }
        (pages, bars.into_any_element())
    }

    fn host_level(
        &mut self,
        model: &StatusBarModel,
        top_inset: Pixels,
        placement: &PillPlacement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hosts = self.host_pages(cx);
        let width = f32::from(window.viewport_size().width);
        let shown = self.compact.host_pager.index();
        self.compact.host_pager.sync(shown, hosts.len());
        if cx.reduce_motion() {
            self.compact.host_pager.finish();
        }
        if self.compact.host_pager.tick(Instant::now()) {
            window.request_animation_frame();
        }
        if let Some(PagerEvent::Commit { index }) = self.compact.host_pager.take_event() {
            cx.defer_in(window, move |this, window, cx| {
                this.show_host(index, window, cx);
            });
        }
        let layout = self.compact.host_pager.layout(width, HOST_GAP);
        let frame = PageFrame {
            top: top_inset,
            line: top_inset + px(COMPACT_TITLE_BAR_HEIGHT),
            zone: placement.zone(),
            width: px(width),
            height: window.viewport_size().height,
            bottom: self.card_bottom(placement),
        };
        self.compact.top_row = frame.line;
        let mut reveal = None;
        let mut panes = Vec::new();
        let mut strip = Vec::new();
        for (page, x) in layout.pages {
            let Some(host) = hosts.get(page) else {
                continue;
            };
            let body = self.host_body(page, host, model, &frame, &mut reveal, &mut panes, cx);
            strip.push(
                div()
                    .id(("overview-host", page))
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(x))
                    .w(px(width))
                    .child(body)
                    .into_any_element(),
            );
        }
        self.compact
            .cards
            .borrow_mut()
            .retain(|pane, _| panes.contains(pane));
        if self.compact.overview_reveal {
            self.compact.overview_reveal = false;
            if let Some((page, section)) = reveal
                && let Some(scroll) = self.compact.host_scrolls.get(&page)
            {
                scroll.scroll_to_item(section);
            }
        }
        div()
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .child(div().absolute().inset_0().overflow_hidden().children(strip))
            .into_any_element()
    }

    fn hosts_level(
        &mut self,
        top_inset: Pixels,
        placement: &PillPlacement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let grid = self.machines_grid(top_inset, placement, window, cx);
        let Some(card) = self.compact.card.clone() else {
            return grid;
        };
        if cx.reduce_motion() {
            self.compact.card_zoom.finish();
        }
        if self.compact.card_zoom.tick(Instant::now()) {
            window.request_animation_frame();
        }
        let open = self.compact.card_zoom.progress().clamp(0.0, 1.0);
        let moving = self.compact.card_zoom.is_moving();
        if !moving && open <= 0.0 {
            self.compact.card = None;
            return grid;
        }
        if !moving && self.follow_card(&card, window, cx) {
            return grid;
        }
        let bottom = self.card_bottom(placement);
        let view = self.machine_card(&card, top_inset, bottom, window, cx);
        if !moving && open >= 1.0 {
            return view;
        }
        let stage = window.viewport_size();
        let tile = self.card_tile(&card, cx).unwrap_or_else(|| {
            Bounds::new(
                point(px(BENTO_INSET), top_inset + px(TILE_FALLBACK_TOP)),
                size(
                    stage.width / 2.0 - px(BENTO_INSET),
                    px(TILE_FALLBACK_HEIGHT),
                ),
            )
        });
        div()
            .relative()
            .size_full()
            .child(zoom_layers(
                grid,
                view,
                &Zoom {
                    tile,
                    stage,
                    progress: 1.0 - open,
                    ground: zoom_card_ground(cx).fill,
                    corner: cx.theme().phone_radius(),
                },
            ))
            .child(div().id("card-zoom-block").absolute().inset_0().occlude())
            .into_any_element()
    }

    fn machines_grid(
        &mut self,
        top_inset: Pixels,
        placement: &PillPlacement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hosts = self.host_pages(cx);
        let width = window.viewport_size().width;
        let grid = (width - px(BENTO_INSET * 2.0)).max(px(0.0));
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let add = Some((BentoSpan::new(2, 2), self.add_tile(cx)));
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let add: Option<(BentoSpan, AnyElement)> = None;
        let tiles: Vec<(BentoSpan, AnyElement)> = hosts
            .iter()
            .enumerate()
            .map(|(index, host)| (BentoSpan::new(2, 2), self.host_bento_tile(index, host, cx)))
            .chain(add)
            .collect();
        let theme = cx.theme();
        let scroll = self.compact.hosts_scroll.clone();
        div()
            .relative()
            .size_full()
            .bg(theme.background)
            .child(
                div()
                    .id("hosts-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&scroll)
                    .pt(top_inset)
                    .pb(placement.zone() + px(16.0))
                    .child(large_title("Machines", cx))
                    .child(
                        div()
                            .px(px(BENTO_INSET))
                            .pt(px(16.0))
                            .child(bento_grid(grid, tiles)),
                    ),
            )
            .child(folding_title_bar(
                "Machines",
                scrolled(&scroll),
                top_inset,
                cx,
            ))
            .into_any_element()
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn add_tile(&self, cx: &mut Context<Self>) -> AnyElement {
        let bounds = Rc::clone(&self.compact.add_tile);
        PhoneTile::add("hosts-add", "Add machine")
            .child(
                canvas(
                    move |tile, _, _| {
                        bounds.borrow_mut().replace(tile);
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_machine_card(MachineCard::Add, window, cx);
            }))
            .into_any_element()
    }

    fn host_bento_tile(&self, index: usize, host: &HostPage, cx: &mut Context<Self>) -> AnyElement {
        let state = host.state(cx);
        let counts = match (state, &host.connection) {
            (HostState::Connected, Some(connection)) => Some(host_counts(connection, cx)),
            _ => None,
        };
        let (mark, word) = host_mark(state, counts.as_ref().map_or(0, |counts| counts.waiting));
        let foot: Vec<String> = match (state, counts) {
            (HostState::Connected, Some(counts)) => {
                std::iter::once(overview_count(counts.sessions, "session"))
                    .chain((counts.working > 0).then(|| format!("{} working", counts.working)))
                    .collect()
            }
            (HostState::Connecting, _) => host.location().into_iter().collect(),
            (HostState::Failed, _) => vec![host.status(cx)],
            _ => Vec::new(),
        };
        let tiles = Rc::clone(&self.compact.host_tiles);
        let attached = host
            .connection
            .as_ref()
            .is_some_and(|connection| *connection == self.connection);
        let enterable = attached || state == HostState::Connected;
        let endpoint = host.endpoint.clone();
        let held = host.endpoint.clone();
        foot.into_iter()
            .fold(
                PhoneTile::new(("hosts-tile", index), host.title.clone())
                    .mark(mark)
                    .state(word),
                PhoneTile::foot,
            )
            .child(
                canvas(
                    move |bounds, _, _| {
                        tiles.borrow_mut().insert(index, bounds);
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .on_click(
                cx.listener(move |this, _, window, cx| match (state, &endpoint) {
                    (HostState::Connecting, Some(endpoint)) => {
                        this.open_machine_card(
                            MachineCard::Connecting(endpoint.clone()),
                            window,
                            cx,
                        );
                    }
                    _ if enterable => this.enter_host(index, window, cx),
                    (HostState::Offline | HostState::Failed, Some(endpoint)) => {
                        #[cfg(any(target_os = "ios", target_os = "android"))]
                        this.connect_host(endpoint.clone(), window, cx);
                        let _ = endpoint;
                    }
                    _ => {}
                }),
            )
            .when_some(held, |tile, endpoint| {
                let view = cx.entity().downgrade();
                tile.on_hold(move |_, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.compact.held = Some(endpoint.clone());
                        cx.notify();
                    });
                })
            })
            .into_any_element()
    }

    fn open_machine_card(
        &mut self,
        card: MachineCard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if card == MachineCard::Add {
            self.hosts.error = None;
            if self.found_machines(cx).is_empty() {
                cx.on_next_frame(window, |this, window, cx| {
                    this.hosts
                        .destination
                        .update(cx, |input, cx| input.focus(window, cx));
                });
            }
        }
        let _ = window;
        self.compact.held = None;
        self.compact.card = Some(card);
        self.compact.card_zoom.snap(false);
        self.compact.card_zoom.animate(true, Instant::now());
        cx.notify();
    }

    pub(in super::super) fn close_machine_card(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        super::super::settings::finish_field(window, cx);
        if self.compact.card.is_some() {
            self.compact.card_zoom.animate(false, Instant::now());
            cx.notify();
        }
    }

    fn follow_card(
        &mut self,
        card: &MachineCard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let MachineCard::Connecting(endpoint) = card else {
            return false;
        };
        let hosts = self.host_pages(cx);
        let Some((index, host)) = hosts
            .iter()
            .enumerate()
            .find(|(_, host)| host.endpoint.as_ref() == Some(endpoint))
        else {
            self.close_machine_card(window, cx);
            return false;
        };
        match host.state(cx) {
            HostState::Connected => {
                self.compact.card = None;
                self.compact.card_zoom.snap(false);
                cx.defer_in(window, move |this, window, cx| {
                    this.show_host(index, window, cx);
                    let count = this.host_pages(cx).len();
                    this.compact.host_pager.sync(index, count);
                    this.compact.host_pager.go_to(index, Instant::now());
                    this.compact.host_pager.finish();
                    this.compact.overview_reveal = true;
                    this.compact.hosts.snap(false);
                    cx.notify();
                });
                true
            }
            HostState::Connecting => false,
            HostState::Offline | HostState::Failed => {
                self.close_machine_card(window, cx);
                false
            }
        }
    }

    fn card_tile(&self, card: &MachineCard, cx: &App) -> Option<Bounds<Pixels>> {
        match card {
            MachineCard::Add => *self.compact.add_tile.borrow(),
            MachineCard::Connecting(endpoint) => {
                let index = self
                    .host_pages(cx)
                    .iter()
                    .position(|host| host.endpoint.as_ref() == Some(endpoint))?;
                self.compact.host_tiles.borrow().get(&index).copied()
            }
        }
    }

    fn card_bottom(&self, placement: &PillPlacement) -> Pixels {
        let keyboard = self.compact.keyboard_overlap;
        if keyboard > px(0.0) {
            keyboard + px(ACTION_BAR_KEYBOARD)
        } else {
            placement.bottom
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn machine_card(
        &mut self,
        _: &MachineCard,
        _: Pixels,
        _: Pixels,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> AnyElement {
        div().into_any_element()
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn found_machines(&self, cx: &App) -> Vec<crate::connection::FoundHost> {
        let saved = self.hosts.saved.endpoints();
        let open: Vec<String> = self
            .host_connections()
            .iter()
            .filter_map(|connection| connection.read(cx).endpoint().map(ToOwned::to_owned))
            .collect();
        self.hosts
            .found
            .iter()
            .filter(|host| !saved.contains(&host.endpoint) && !open.contains(&host.endpoint))
            .cloned()
            .collect()
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn machine_card(
        &mut self,
        card: &MachineCard,
        top_inset: Pixels,
        bottom: Pixels,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use super::super::hosts::{host_title, waiting_for};
        let ground = zoom_card_ground(cx);
        let close = cx.listener(|this, _, window, cx| this.close_machine_card(window, cx));
        match card {
            MachineCard::Add => {
                let found = self.found_machines(cx);
                let rows: Vec<_> = found.iter().map(|host| host.machine.clone()).collect();
                let endpoints: Vec<String> = found.into_iter().map(|host| host.endpoint).collect();
                let view = cx.entity().downgrade();
                let rows = found_machines(&rows, move |index, window, cx| {
                    let Some(endpoint) = endpoints.get(index).cloned() else {
                        return;
                    };
                    let _ = view.update(cx, |this, cx| {
                        this.hosts.saved.add(&endpoint);
                        this.connect_host(endpoint, window, cx);
                        this.close_machine_card(window, cx);
                    });
                });
                let label = if rows.is_some() {
                    "Or an address"
                } else {
                    "Address"
                };
                let empty = self.hosts.destination.read(cx).value().trim().is_empty();
                ZoomCard::new("add-machine-card", "Add machine")
                    .top_inset(top_inset)
                    .bottom_inset(bottom)
                    .on_close(close)
                    .children(rows)
                    .child(
                        Field::new()
                            .with_size(penna::Size::TouchLarge)
                            .label(label)
                            .when_some(self.hosts.error.clone(), Field::error)
                            .child(
                                Input::new(&self.hosts.destination)
                                    .with_size(penna::Size::TouchLarge)
                                    .on(ground),
                            ),
                    )
                    .action(
                        Button::new("add-machine-connect")
                            .primary()
                            .label("Connect")
                            .with_size(penna::Size::TouchLarge)
                            .w_full()
                            .on(ground)
                            .disabled(empty)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.add_host(window, cx) {
                                    this.close_machine_card(window, cx);
                                }
                            })),
                    )
                    .into_any_element()
            }
            MachineCard::Connecting(endpoint) => {
                let target = endpoint.clone();
                ZoomCard::new("connecting-card", host_title(endpoint))
                    .top_inset(top_inset)
                    .bottom_inset(bottom)
                    .on_close(close)
                    .child(
                        div().mt(px(CARD_EMPTY_TOP)).child(
                            PhoneEmpty::spinner("Connecting")
                                .line(waiting_for(endpoint))
                                .on(ground),
                        ),
                    )
                    .action(
                        Button::new("connecting-cancel")
                            .label("Cancel")
                            .with_size(penna::Size::TouchLarge)
                            .w_full()
                            .on(ground)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.disconnect_host(&target, window, cx);
                                this.close_machine_card(window, cx);
                            })),
                    )
                    .into_any_element()
            }
        }
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub(super) fn machine_sheet(
        &self,
        endpoint: &str,
        bottom_inset: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use super::super::hosts::{host_location, host_title};
        let state = self
            .host_connection(endpoint, cx)
            .map_or(HostState::Offline, |connection| {
                let connection = connection.read(cx);
                host_state(connection.connected, connection.busy(), &connection.status)
            });
        let toggle = endpoint.to_owned();
        let toggle_row = match state {
            HostState::Connected => Some(
                TouchRow::new("machine-sheet-disconnect", "Disconnect")
                    .icon(IconName::Stop)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.disconnect_host(&toggle, window, cx);
                        close_sheet(MACHINE_SHEET, cx);
                    })),
            ),
            HostState::Offline | HostState::Failed => Some(
                TouchRow::new("machine-sheet-connect", "Connect")
                    .icon(IconName::Server)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.connect_host(toggle.clone(), window, cx);
                        close_sheet(MACHINE_SHEET, cx);
                    })),
            ),
            HostState::Connecting => None,
        };
        let removed = endpoint.to_owned();
        let remove = TouchRow::new("machine-sheet-remove", "Remove machine")
            .icon(IconName::Xmark)
            .danger()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.disconnect_host(&removed, window, cx);
                this.hosts.saved.remove(&removed);
                this.hosts.saved.save();
                close_sheet(MACHINE_SHEET, cx);
                cx.notify();
            }));
        let dismiss = cx.weak_entity();
        bottom_sheet(
            MACHINE_SHEET,
            host_title(endpoint),
            [sheet_close("machine-sheet-close")
                .on_click(|_, _, cx| close_sheet(MACHINE_SHEET, cx))
                .into_any_element()],
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .px(cx.theme().row_inset)
                .pb(px(8.0))
                .children(toggle_row.map(|row| row.in_sheet(cx)))
                .child(remove.in_sheet(cx)),
            bottom_inset,
            move |_, cx| {
                let _ = dismiss.update(cx, |this, cx| {
                    this.compact.held = None;
                    cx.notify();
                });
            },
        )
        .detail(host_location(endpoint))
        .into_any_element()
    }

    pub(super) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.settings = Some(zz_ui::settings::SettingsSection::Appearance);
        self.focused_pane = None;
        cx.notify();
    }

    fn hosts_bar(&self, placement: &PillPlacement, cx: &mut Context<Self>) -> AnyElement {
        let settings = compact_bar_button("hosts-settings", IconName::Settings)
            .on_click(cx.listener(|this, _, _, cx| this.open_settings(cx)));
        compact_floating_bar(div().flex_1(), placement, cx)
            .child(compact_floating_group([settings], placement, cx))
            .into_any_element()
    }

    fn host_bar(
        &self,
        hosts: &[HostPage],
        index: usize,
        placement: &PillPlacement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let host = hosts.get(index);
        let meta = match host.map(|host| host.state(cx)) {
            Some(HostState::Connected) => self
                .machine_session(cx)
                .map_or_else(|| "No sessions".to_owned(), |(_, name)| name),
            Some(HostState::Connecting) => "Connecting".to_owned(),
            _ => "Not connected".to_owned(),
        };
        let dots = (hosts.len() > 1).then(|| {
            page_dots(
                &[(0..hosts.len())
                    .map(|page| PageDot {
                        active: page == index,
                        attention: false,
                    })
                    .collect()],
                cx,
            )
        });
        let stack = self.compact.menu.as_ref().map(|(menu, _)| menu.clone());
        let menu = stack
            .as_ref()
            .map_or(0.0, |stack| stack.read(cx).close_shown());
        let pill = compact_floating_pill(
            "overview-host",
            PillLabel {
                icon: IconName::Server,
                title: host
                    .map_or_else(String::new, |host| host.title.clone())
                    .into(),
                word: ala::SharedString::default(),
                meta: meta.into(),
                tone: penna::Tone::Neutral,
            },
            div().children(dots),
            placement,
            cx,
        )
        .opacity(1.0 - menu)
        .children(stack.map(|stack| StackMenu::pill_anchor(&stack, cx)))
        .on_click(cx.listener(|this, _, _, cx| this.open_pill_menu(cx)))
        .on_scroll_wheel(cx.listener(Self::host_pill_scroll));
        compact_floating_bar(pill, placement, cx).into_any_element()
    }

    fn host_pill_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.swipe_host(event, window, cx) {
            return;
        }
        let delta = event.delta.pixel_delta(window.line_height());
        let response =
            self.compact
                .hosts
                .scroll(f32::from(delta.y), event.touch_phase, Instant::now());
        if !response.consumed {
            return;
        }
        cx.stop_propagation();
        if response.started {
            self.compact.close_sheets();
        }
        if response.event.is_some() {
            window.end_touch_momentum();
        }
        cx.notify();
    }

    fn swipe_host(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let delta_x = match event.delta {
            ScrollDelta::Pixels(delta) => f32::from(delta.x),
            ScrollDelta::Lines(delta) => delta.x * 20.0,
        };
        let response = self.compact.host_pager.scroll(
            delta_x,
            event.touch_phase,
            self.compact.width,
            Instant::now(),
        );
        if !response.consumed {
            return false;
        }
        cx.stop_propagation();
        if self.compact.host_pager.swallows_momentum() {
            window.end_touch_momentum();
        }
        if let Some(PagerEvent::Commit { index }) = response.event {
            self.show_host(index, window, cx);
        }
        cx.notify();
        true
    }

    fn host_body(
        &mut self,
        page: usize,
        host: &HostPage,
        model: &StatusBarModel,
        frame: &PageFrame,
        reveal: &mut Option<(usize, usize)>,
        panes: &mut Vec<PaneId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let attached = host
            .connection
            .as_ref()
            .is_some_and(|connection| *connection == self.connection);
        if !host.connected(cx) {
            return self.unreached_host(page, host, frame, cx);
        }
        let connection = host
            .connection
            .clone()
            .unwrap_or_else(|| self.connection.clone());
        let grid = (frame.width - px(BENTO_INSET * 2.0)).max(px(0.0));
        let sections = self.host_sections(
            page,
            host,
            &connection,
            attached.then_some(model),
            grid,
            frame,
            reveal,
            panes,
            cx,
        );
        let body = if sections.is_empty() {
            vec![self.empty_machine(page, grid, cx)]
        } else {
            sections
        };
        let scroll = self.compact.host_scrolls.entry(page).or_default().clone();
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .id(("overview-scroll", page))
                    .size_full()
                    .overflow_y_scroll()
                    .restrict_scroll_to_axis()
                    .track_scroll(&scroll)
                    .pt(frame.line)
                    .pb(frame.zone + px(16.0))
                    .child(large_title(host.title.clone(), cx))
                    .children(body),
            )
            .child(folding_title_bar(
                host.title.clone(),
                scrolled(&scroll),
                frame.top,
                cx,
            ))
            .child(self.machine_close(page, frame, cx))
            .into_any_element()
    }

    fn machine_close(&self, page: usize, frame: &PageFrame, cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(frame.line)
            .child(top_row_button(
                TopRowEnd::End,
                px(BENTO_INSET),
                compact_bar_button(("machine-close", page), IconName::Xmark)
                    .tooltip("Close")
                    .on_click(cx.listener(|this, _, _, cx| this.open_hosts(cx))),
            ))
            .into_any_element()
    }

    fn empty_machine(&self, page: usize, grid: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let add = PhoneTile::add(("machine-new", page), "New")
            .on_click(cx.listener(|this, _, _, cx| this.open_pill_menu(cx)));
        div()
            .px(px(BENTO_INSET))
            .pt(Space::GAP_GROUP)
            .child(bento_grid(
                grid,
                vec![(BentoSpan::new(2, 2), add.into_any_element())],
            ))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn host_sections(
        &mut self,
        page: usize,
        host: &HostPage,
        connection: &Entity<Connection>,
        attached_model: Option<&StatusBarModel>,
        grid: Pixels,
        frame: &PageFrame,
        reveal: &mut Option<(usize, usize)>,
        drawn_panes: &mut Vec<PaneId>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let attached_session = attached_model.and(connection.read(cx).core.attached_session());
        let now = if attached_model.is_some() {
            self.flat.clock
        } else {
            DaemonClock::default()
        }
        .now(super::super::unix_seconds());
        let sessions: Vec<(SessionId, String, StatusBarModel, Vec<WindowShape>)> = {
            let core = &connection.read(cx).core;
            core.snapshot()
                .sessions
                .iter()
                .map(|session| {
                    let model = match attached_model {
                        Some(model) if attached_session == Some(session.id) => model.clone(),
                        _ => StatusBarModel::from_snapshot(core.snapshot(), Some(session.id)),
                    };
                    let shapes = session
                        .windows
                        .iter()
                        .map(|window| {
                            let summary = summarize_window(window);
                            let (mark, state) = window_mark(&summary);
                            let closing = match window.panes.values().next() {
                                Some(only) if window.panes.len() == 1 => {
                                    close_pane_detail(&only.kind, &summary.title)
                                }
                                _ => Some(close_many_detail(window.panes.len(), "pane")),
                            };
                            WindowShape {
                                id: window.id,
                                detail: hold_detail(window, now),
                                closing,
                                title: summary.title,
                                mark,
                                state,
                                rects: pane_rects(&window.layout)
                                    .into_iter()
                                    .map(|(pane, rect)| {
                                        (pane, [rect.x, rect.y, rect.width, rect.height])
                                    })
                                    .collect(),
                            }
                        })
                        .collect();
                    (session.id, session_label(session).text, model, shapes)
                })
                .collect()
        };
        if sessions.is_empty() {
            return Vec::new();
        }
        let count = sessions.len();
        let tail = px(session_tail(
            f32::from(frame.height),
            f32::from(frame.line),
            f32::from(frame.zone) + 16.0,
            count,
        ));
        let mut sections = Vec::new();
        for (index, (session, name, model, shapes)) in sessions.into_iter().enumerate() {
            let live = attached_session == Some(session);
            let (pages, _) = pages(&model, &self.unseen_agents);
            let target = if live {
                CardTarget::Here
            } else if attached_model.is_some() {
                CardTarget::Session(session)
            } else {
                CardTarget::Host(host.endpoint.clone().unwrap_or_default(), session)
            };
            let held = Thing {
                connection: connection.clone(),
                target: Target::Session(session),
                name: name.clone(),
                detail: None,
                closing: Some(close_many_detail(model.windows.len(), "window")),
            };
            let view = cx.entity().downgrade();
            let header = TouchHeader::new(name.clone())
                .over_tiles()
                .on_hold(move |window, cx| {
                    let held = held.clone();
                    let _ = view.update(cx, |this, cx| {
                        this.open_phone_sheet(PhoneSheet::Hold(held), window, cx);
                    });
                });
            let tiles = model
                .windows
                .iter()
                .map(|window| {
                    let shape = shapes.iter().find(|shape| shape.id == window.id);
                    (
                        window_span(window.panes.len()),
                        self.window_bento_tile(
                            window,
                            shape,
                            &pages,
                            connection,
                            target.clone(),
                            drawn_panes,
                            cx,
                        ),
                    )
                })
                .collect();
            if live {
                *reveal = Some((page, sections.len() + 1));
            }
            sections.push(
                div()
                    .px(px(BENTO_INSET))
                    .when(index > 0, |section| section.mt(Space::GAP_GROUP))
                    .when(index + 1 == count, |section| section.min_h(tail))
                    .child(header)
                    .child(bento_grid(grid, tiles))
                    .into_any_element(),
            );
        }
        sections
    }

    #[allow(clippy::too_many_arguments)]
    fn window_bento_tile(
        &self,
        window: &StatusBarWindow,
        shape: Option<&WindowShape>,
        pages: &[Page],
        connection: &Entity<Connection>,
        target: CardTarget,
        drawn_panes: &mut Vec<PaneId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let here = matches!(target, CardTarget::Here);
        let open = window
            .panes
            .iter()
            .find(|pane| pane.active)
            .or_else(|| window.panes.first())
            .map(|pane| pane.id);
        let count = window.panes.len();
        let rects: Vec<(PaneId, [f32; 4])> = shape
            .map(|shape| shape.rects.clone())
            .filter(|rects| !rects.is_empty())
            .unwrap_or_else(|| {
                let fraction = 1.0 / count.max(1) as f32;
                window
                    .panes
                    .iter()
                    .enumerate()
                    .map(|(index, pane)| (pane.id, [0.0, fraction * index as f32, 1.0, fraction]))
                    .collect()
            });
        let minis: Vec<([f32; 4], MiniPane)> = rects
            .iter()
            .filter_map(|(pane, rect)| {
                let page = pages.iter().find(|page| page.pane == *pane)?;
                drawn_panes.push(page.pane);
                let mini = self.mini_preview(page, connection, here, cx);
                let pane_id = page.pane;
                let cards = Rc::clone(&self.compact.cards);
                let mini = if here {
                    mini.child(
                        canvas(
                            move |bounds, _, _| {
                                cards.borrow_mut().insert(pane_id, bounds);
                            },
                            |_, (), _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                } else {
                    mini
                };
                Some((*rect, mini))
            })
            .collect();
        let unseen = pages
            .iter()
            .any(|page| page.window == window.id && page.attention);
        let (mark, state) = shape.map_or((None, None), |shape| (shape.mark, shape.state.clone()));
        let mark = mark.or_else(|| unseen.then_some(StatusMark::Ink));
        let name = shape.map_or_else(|| window.label.clone(), |shape| shape.title.clone());
        let opened = open.and_then(|open| pages.iter().find(|page| page.pane == open).cloned());
        let cell = PhoneTile::new(("overview-window", window.id.0), name)
            .mark(mark)
            .panes(minis);
        let cell = match state {
            Some(state) => cell.state(state),
            None => cell,
        };
        let held = shape.map(|shape| Thing {
            connection: connection.clone(),
            target: Target::Window(shape.id),
            name: shape.title.clone(),
            detail: Some(shape.detail.clone()).filter(|detail| !detail.is_empty()),
            closing: shape.closing.clone(),
        });
        let view = cx.entity().downgrade();
        let cell = match held {
            Some(held) => cell.on_hold(move |window, cx| {
                let held = held.clone();
                let _ = view.update(cx, |this, cx| {
                    this.open_phone_sheet(PhoneSheet::Hold(held), window, cx);
                });
            }),
            None => cell,
        };
        cell.on_click(cx.listener(move |this, _, window, cx| {
            let Some(page) = &opened else {
                return;
            };
            match &target {
                CardTarget::Here => {
                    this.compact_select(page, cx);
                    this.land_overview(Some(page.pane), cx);
                    return;
                }
                CardTarget::Session(session) => this.open_pane(*session, page.pane, cx),
                #[cfg(any(target_os = "ios", target_os = "android"))]
                CardTarget::Host(endpoint, session) => {
                    this.switch_host(endpoint, Some(*session), Some(page.pane), window, cx);
                }
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                CardTarget::Host(..) => {}
            }
            let _ = window;
            this.close_overview(cx);
        }))
        .into_any_element()
    }

    fn mini_preview(
        &self,
        page: &Page,
        connection: &Entity<Connection>,
        here: bool,
        cx: &App,
    ) -> MiniPane {
        let id = ("mini-pane", page.pane.0);
        let core = &connection.read(cx).core;
        match &page.kind {
            PaneKindSnapshot::Terminal => core.viewport(page.pane).map_or_else(
                || MiniPane::blank(id),
                |viewport| {
                    MiniPane::terminal(
                        id,
                        terminal_preview(
                            viewport,
                            phone_colors(viewport, &host_palette(core), cx),
                            &font(cx.theme().mono_font_family.clone()),
                            0..viewport.rows.min(PREVIEW_ROWS),
                            PREVIEW_COLUMNS,
                        ),
                    )
                },
            ),
            PaneKindSnapshot::Browser(descriptor) => {
                let host = here
                    .then(|| self.browser_page(page, cx))
                    .flatten()
                    .map(|(_, host)| host)
                    .or_else(|| {
                        descriptor
                            .tabs
                            .get(descriptor.active_tab)
                            .map(|url| page_host(url).to_owned())
                    })
                    .unwrap_or_default();
                MiniPane::browser(id, host)
            }
            PaneKindSnapshot::Agent(_) => MiniPane::chat(id),
            _ => MiniPane::blank(id),
        }
    }

    fn unreached_host(
        &self,
        page: usize,
        host: &HostPage,
        frame: &PageFrame,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = host.state(cx);
        let endpoint = host.endpoint.clone();
        let (empty, action) = if state == HostState::Connecting {
            (
                PhoneEmpty::spinner("Connecting").line(host.waiting()),
                Button::new(("machine-cancel", page))
                    .label("Cancel")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        #[cfg(any(target_os = "ios", target_os = "android"))]
                        if let Some(endpoint) = &endpoint {
                            this.disconnect_host(endpoint, window, cx);
                        }
                        let _ = (this, &endpoint, window, cx);
                    })),
            )
        } else {
            let status = host.status(cx);
            let empty = PhoneEmpty::new(IconName::Server, "Not connected");
            let empty = if state == HostState::Failed && !status.is_empty() {
                empty.line(status)
            } else {
                empty
            };
            (
                empty,
                Button::new(("machine-connect", page))
                    .primary()
                    .label("Connect")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        #[cfg(any(target_os = "ios", target_os = "android"))]
                        if let Some(endpoint) = &endpoint {
                            this.connect_host(endpoint.clone(), window, cx);
                        }
                        let _ = (this, &endpoint, window, cx);
                    })),
            )
        };
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .pt(frame.line)
            .child(large_title(host.title.clone(), cx))
            .child(div().mt(px(CARD_EMPTY_TOP)).child(empty))
            .child(div().flex_1())
            .child(
                action_bar(frame.bottom).child(action.with_size(penna::Size::TouchLarge).w_full()),
            )
            .child(self.machine_close(page, frame, cx))
            .into_any_element()
    }

    pub(in super::super) fn open_pane(
        &mut self,
        session: SessionId,
        pane: PaneId,
        cx: &mut Context<Self>,
    ) {
        self.connection
            .update(cx, |connection, cx| connection.attach(session, cx));
        let target = pane.to_string();
        self.command("select-window", vec!["-t".into(), target.clone()], cx);
        self.command("select-pane", vec!["-Z".into(), "-t".into(), target], cx);
    }

    pub(in super::super) fn host_pages(&self, cx: &App) -> Vec<HostPage> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            use super::super::hosts::host_title;
            let connections = self.host_connections();
            let saved = self.hosts.saved.endpoints();
            let mut pages: Vec<HostPage> = connections
                .iter()
                .filter_map(|connection| {
                    let endpoint = connection.read(cx).endpoint()?;
                    (!saved.iter().any(|saved| saved == endpoint)).then(|| HostPage {
                        endpoint: Some(endpoint.to_owned()),
                        title: host_title(endpoint),
                        connection: Some(connection.clone()),
                    })
                })
                .collect();
            pages.extend(saved.iter().map(|endpoint| {
                HostPage {
                    endpoint: Some(endpoint.clone()),
                    title: host_title(endpoint),
                    connection: connections
                        .iter()
                        .find(|connection| {
                            connection.read(cx).endpoint() == Some(endpoint.as_str())
                        })
                        .cloned(),
                }
            }));
            pages
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let _ = cx;
            vec![HostPage {
                endpoint: None,
                title: "This computer".into(),
                connection: Some(self.connection.clone()),
            }]
        }
    }

    pub(in super::super) fn create(
        &mut self,
        what: NewThing,
        session: Option<SessionId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let switched = self.create_on_shown_host(session, window, cx);
        let kind = |what: NewThing| -> Vec<String> {
            match what {
                NewThing::Browser => vec!["--kind".into(), "browser".into()],
                NewThing::Chat => vec!["--kind".into(), "picker".into()],
                NewThing::Terminal | NewThing::Session => {
                    vec!["-c".into(), "#{pane_current_path}".into()]
                }
            }
        };
        match (what, session) {
            (NewThing::Session | NewThing::Terminal, None) if switched => {}
            (NewThing::Session | NewThing::Terminal, None) | (NewThing::Session, Some(_)) => {
                self.command("new-session", Vec::new(), cx);
            }
            (thing, None) => {
                if !switched {
                    self.command("new-session", Vec::new(), cx);
                }
                let mut args = vec!["-k".into(), "-t".into(), ":^".into()];
                args.extend(kind(thing));
                self.command("new-window", args, cx);
            }
            (thing, Some(session)) => {
                if !switched && self.connection.read(cx).core.attached_session() != Some(session) {
                    self.connection
                        .update(cx, |connection, cx| connection.attach(session, cx));
                }
                let mut args = vec!["-t".into(), format!("{session}:")];
                args.extend(kind(thing));
                self.command("new-window", args, cx);
            }
        }
        self.close_overview(cx);
    }

    fn create_on_shown_host(
        &mut self,
        session: Option<SessionId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if self.compact.overview
            && let Some(endpoint) = self
                .host_pages(cx)
                .get(self.compact.host_pager.index())
                .and_then(|host| host.endpoint.clone())
            && !self.is_attached_host(&endpoint, cx)
        {
            self.switch_host(&endpoint, session, None, window, cx);
            return true;
        }
        let _ = (session, window, cx);
        false
    }

    pub(in super::super) fn machine_session(&self, cx: &App) -> Option<(SessionId, String)> {
        let index = self.compact.host_pager.index();
        let host = self.host_pages(cx).into_iter().nth(index)?;
        if !host.connected(cx) {
            return None;
        }
        let connection = host.connection.unwrap_or_else(|| self.connection.clone());
        let sessions: Vec<(SessionId, String)> = connection
            .read(cx)
            .core
            .snapshot()
            .sessions
            .iter()
            .map(|session| (session.id, session_label(session).text))
            .collect();
        let at = self.compact.host_scrolls.get(&index).map_or(0, |scroll| {
            let top = scroll.bounds().top() - scroll.offset().y;
            let tops: Vec<f32> = (1..=sessions.len())
                .map_while(|item| scroll.bounds_for_item(item))
                .map(|bounds| f32::from(bounds.top() - top))
                .collect();
            session_in_view(&tops, f32::from(self.compact.top_row))
        });
        sessions.get(at).or(sessions.first()).cloned()
    }

    pub(in super::super) fn machine_menu_text(&self, cx: &App) -> MachineMenuText {
        let connected = self
            .host_pages(cx)
            .get(self.compact.host_pager.index())
            .is_some_and(|host| host.connected(cx));
        match self.machine_session(cx) {
            _ if !connected => MachineMenuText::Unreached,
            Some((_, name)) => MachineMenuText::Session(name),
            None => MachineMenuText::Empty,
        }
    }

    pub(in super::super) fn attached_session_name(&self, cx: &App) -> String {
        let core = &self.connection.read(cx).core;
        core.attached_session()
            .and_then(|id| {
                core.snapshot()
                    .sessions
                    .iter()
                    .find(|session| session.id == id)
            })
            .map(|session| session_label(session).text)
            .unwrap_or_default()
    }
}

pub(in super::super) enum MachineMenuText {
    Unreached,
    Empty,
    Session(String),
}

impl MachineMenuText {
    pub(in super::super) fn as_menu(&self) -> MachineMenu<'_> {
        match self {
            Self::Unreached => MachineMenu::Unreached,
            Self::Empty => MachineMenu::Empty,
            Self::Session(name) => MachineMenu::Session(name),
        }
    }
}

struct PageFrame {
    top: Pixels,
    line: Pixels,
    zone: Pixels,
    width: Pixels,
    height: Pixels,
    bottom: Pixels,
}

#[derive(Clone)]
enum CardTarget {
    Here,
    Session(SessionId),
    #[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
    Host(String, SessionId),
}

struct WindowShape {
    id: WindowId,
    title: String,
    detail: String,
    closing: Option<String>,
    mark: Option<StatusMark>,
    state: Option<String>,
    rects: Vec<(PaneId, [f32; 4])>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
pub(super) enum MachineCard {
    Add,
    Connecting(String),
}

#[derive(Clone, Copy)]
pub(in super::super) enum NewThing {
    Chat,
    Terminal,
    Browser,
    Session,
}

#[derive(Clone)]
pub(in super::super) struct HostPage {
    pub endpoint: Option<String>,
    pub title: String,
    pub connection: Option<Entity<Connection>>,
}

impl HostPage {
    fn connected(&self, cx: &App) -> bool {
        self.connection
            .as_ref()
            .is_some_and(|connection| connection.read(cx).connected)
    }

    fn state(&self, cx: &App) -> HostState {
        self.connection
            .as_ref()
            .map_or(HostState::Offline, |connection| {
                let connection = connection.read(cx);
                host_state(
                    connection.connected,
                    connection.busy_or_connecting(),
                    &connection.status,
                )
            })
    }

    fn status(&self, cx: &App) -> String {
        self.connection
            .as_ref()
            .map(|connection| connection.read(cx).status.clone())
            .unwrap_or_default()
    }

    fn location(&self) -> Option<String> {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            self.endpoint
                .as_deref()
                .map(super::super::hosts::host_location)
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            None
        }
    }

    fn waiting(&self) -> String {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            self.endpoint
                .as_deref()
                .map(super::super::hosts::waiting_for)
                .unwrap_or_default()
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            String::new()
        }
    }
}
