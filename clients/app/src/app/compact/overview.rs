use std::{cell::RefCell, ops::Range, rc::Rc};

use ala::{
    AnyElement, App, Bounds, Context, Entity, Font, Hsla, PaintScaleExt as _, ParentElement as _,
    Pixels, ScrollDelta, ScrollWheelEvent, Size, StyledText, TextRun, Window, canvas, div, font,
    linear_color_stop, linear_gradient, point, prelude::*, px, size,
};
use penna::{
    ActiveTheme as _, Colorize as _, IconName, Sizable as _, Space,
    blocks::compact::{
        BENTO_GAP, BENTO_INSET, BentoSpan, COMPACT_PILL_HEIGHT, COMPACT_TITLE_BAR_HEIGHT,
        MACHINE_TILE_HEIGHT, MachineTile, MiniPane, PhoneTile, PillPlacement, StripMachine,
        TAIL_LINES, WindowCard, WindowTail, bento_grid, compact_bar_disc, compact_floating_bar,
        folding_title_bar, machine_strip, phone_card_ground, pill_handle, pill_handle_bounds,
        scrolled, split_pill_bounds, zoom_card_ground,
    },
    blocks::settings::{large_title, large_title_lead},
    blocks::touch::StatusMark,
    blocks::touch::{
        ACTION_BAR_KEYBOARD, PagerEvent, PhoneEmpty, StackMenu, TouchHeader, action_bar,
    },
    components::button::{Button, ButtonVariants as _},
    v_flex,
};
use penna::{
    blocks::compact::ZoomCard,
    blocks::sheet::close_sheet,
    blocks::touch::{TouchGroup, TouchRow},
};
use web_time::Instant;
use zz_client::{
    AgentAttentionStatus, StatusBarModel, StatusBarWindow, agent_attention_status,
    navigation::session_label,
    window_summary::{Attention, DetailPart, PaneTile, summarize_window},
};
use zz_protocol::{PaneId, PaneKindSnapshot, SessionId, WindowId};
use zz_terminal::{
    Cursor, Glyph, LocalColors, TerminalAppearance, TerminalPalette, TerminalViewport,
};
#[cfg(any(target_os = "ios", target_os = "android"))]
use zz_ui::compact::found_machines;
use zz_ui::{
    command::PaletteHostId,
    compact::{
        HostState, MachineMenu, close_many_detail, close_pane_detail, host_mark, host_state,
        machine_face, mark_dot, session_in_view, window_faces, window_mark,
    },
    terminal::terminal_background,
};

use super::{
    AppShell, MENU_DIM, Page, pages,
    sheets::{PhoneSheet, Target, Thing},
};
use crate::{connection::Connection, terminal::phone_colors};

const PREVIEW_ROWS: u16 = 40;
const PREVIEW_COLUMNS: usize = 56;
const HOST_GAP: f32 = 12.0;
const ZOOM_FADE_START: f32 = 0.3;
const ZOOM_FADE_SPAN: f32 = 0.4;
const PANE_ZOOM_REST: f32 = 0.88;
const PANE_ZOOM_PUSH: f32 = 1.08;
const TILE_FALLBACK_TOP: f32 = 160.0;
const TILE_FALLBACK_HEIGHT: f32 = 178.0;
const CARD_EMPTY_TOP: f32 = 96.0;
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

fn tail_rows(viewport: &TerminalViewport) -> Range<u16> {
    let written = |row: &u16| {
        viewport.row(*row).is_some_and(|cells| {
            cells.iter().any(|cell| match viewport.glyph(*cell) {
                Glyph::Empty => false,
                Glyph::Scalar(character) => !character.is_whitespace(),
                Glyph::Grapheme(_) => true,
            })
        })
    };
    let text = (0..viewport.rows).rev().find(written);
    let cursor = viewport.cursor.map(Cursor::row);
    let end = text.max(cursor).map_or(0, |row| row + 1).min(viewport.rows);
    end.saturating_sub(TAIL_LINES)..end
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

pub(super) fn pane_zoom_layers(
    parent: AnyElement,
    child: AnyElement,
    stage: Size<Pixels>,
    progress: f32,
    ground: Hsla,
    corner: Pixels,
) -> ala::Div {
    let progress = progress.clamp(0.0, 1.0);
    let centered = |scale: f32| {
        point(
            stage.width * ((1.0 - scale) / 2.0),
            stage.height * ((1.0 - scale) / 2.0),
        )
    };
    let parent_scale = PANE_ZOOM_PUSH + (1.0 - PANE_ZOOM_PUSH) * progress;
    let child_scale = 1.0 + (PANE_ZOOM_REST - 1.0) * progress;
    let layer = || {
        div()
            .absolute()
            .top_0()
            .left_0()
            .w(stage.width)
            .h(stage.height)
    };
    div()
        .absolute()
        .inset_0()
        .child(
            layer()
                .opacity(zoom_fade(progress))
                .child(parent)
                .paint_scale(parent_scale, centered(parent_scale)),
        )
        .child(
            layer()
                .overflow_hidden()
                .rounded(corner)
                .bg(ground)
                .opacity(zoom_fade(1.0 - progress))
                .child(child)
                .paint_scale(child_scale, centered(child_scale)),
        )
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
            self.compact.entering = None;
            self.compact.lift.animate(true, Instant::now());
        }
        cx.notify();
    }

    pub(in super::super) fn close_overview(&mut self, cx: &mut Context<Self>) {
        self.compact.overview = false;
        self.compact.close_sheets();
        self.compact.landing = None;
        self.compact.entering = None;
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
        if let Some(endpoint) = self
            .host_pages(cx)
            .get(index)
            .filter(|host| host.connected(cx))
            .and_then(|host| host.endpoint.clone())
            && !self.is_attached_host(&endpoint, cx)
        {
            self.switch_host(&endpoint, None, None, window, cx);
        }
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
        if progress < 1.0 || dragging {
            bars = bars.child(
                div()
                    .absolute()
                    .inset_0()
                    .opacity(1.0 - progress)
                    .child(self.host_bar(&hosts, index, placement, window, cx)),
            );
        }
        if progress >= 1.0 && !moving && !dragging {
            bars = div().id("overview-bars");
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
            cx.haptic(ala::Haptic::Selection);
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
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let found = self.found_list(cx);
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let found: Option<AnyElement> = None;
        let tiles = self.machine_tiles(&hosts, window.viewport_size().width, cx);
        let menu = self.machines_menu(cx);
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
                    .pt(top_inset + px(COMPACT_TITLE_BAR_HEIGHT))
                    .pb(placement.zone() + px(16.0))
                    .child(large_title("Machines", cx))
                    .children(tiles)
                    .children(found)
                    .child(menu),
            )
            .child(folding_title_bar(
                "Machines",
                scrolled(&scroll),
                top_inset,
                cx,
            ))
            .into_any_element()
    }

    fn machine_tiles(
        &self,
        hosts: &[HostPage],
        width: Pixels,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if hosts.is_empty() {
            return None;
        }
        let mut items: Vec<(usize, HostState, Option<HostCounts>)> = hosts
            .iter()
            .enumerate()
            .map(|(index, host)| {
                let state = host.state(cx);
                let counts = match (state, &host.connection) {
                    (HostState::Connected, Some(connection)) => Some(host_counts(connection, cx)),
                    _ => None,
                };
                (index, state, counts)
            })
            .collect();
        items.sort_by_key(|(_, state, counts)| match state {
            HostState::Connected if counts.as_ref().is_some_and(|counts| counts.waiting > 0) => 0,
            HostState::Connected => 1,
            HostState::Connecting => 2,
            HostState::Offline | HostState::Failed => 3,
        });
        let grid = (width - px(BENTO_INSET * 2.0)).max(px(0.0));
        let half = ((grid - px(BENTO_GAP)) / 2.0).max(px(0.0));
        let tiles: Vec<AnyElement> = items
            .iter()
            .map(|(index, state, counts)| {
                div()
                    .w(half)
                    .h(px(MACHINE_TILE_HEIGHT))
                    .child(self.machine_tile(*index, &hosts[*index], *state, counts.as_ref(), cx))
                    .into_any_element()
            })
            .collect();
        Some(
            div()
                .flex()
                .flex_wrap()
                .gap(px(BENTO_GAP))
                .px(px(BENTO_INSET))
                .pt(Space::S16)
                .children(tiles)
                .into_any_element(),
        )
    }

    fn machine_windows(
        connection: &Entity<Connection>,
        cx: &App,
    ) -> Vec<(PaneTile, zz_client::window_summary::Attention)> {
        connection
            .read(cx)
            .core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .filter_map(|window| {
                let summary = summarize_window(window);
                let tile = summary
                    .tiles
                    .iter()
                    .find(|tile| tile.pane == summary.headline)
                    .or_else(|| summary.tiles.first())?
                    .clone();
                Some((tile, summary.attention))
            })
            .collect()
    }

    fn machine_tile(
        &self,
        index: usize,
        host: &HostPage,
        state: HostState,
        counts: Option<&HostCounts>,
        cx: &mut Context<Self>,
    ) -> MachineTile {
        let waiting = counts.map_or(0, |counts| counts.waiting);
        let (mark, word) = host_mark(state, waiting);
        let ground = phone_card_ground(waiting > 0, cx);
        let platform = host.platform(cx);
        let spinning = mark == StatusMark::Spinner;
        let face = machine_face(platform, mark_dot((!spinning).then_some(mark), cx), ground);
        let tile = MachineTile::new(("hosts-tile", index), host.title.clone()).face(face);
        let tile = if waiting > 0 {
            tile.waiting(format!("{waiting} waiting"))
        } else {
            tile
        };
        let tile = match (state, &host.connection) {
            (HostState::Connected, Some(connection)) => tile.faces(window_faces(
                &Self::machine_windows(connection, cx),
                ground,
                cx,
            )),
            (HostState::Offline, _) => tile.state(mark, word).dimmed(),
            (HostState::Connecting, _) => {
                let location = host
                    .endpoint
                    .as_deref()
                    .and_then(|key| self.hosts.location(key));
                match location {
                    Some(location) => tile.state(mark, format!("{word} \u{b7} {location}")),
                    None => tile.state(mark, word),
                }
            }
            _ => tile.state(mark, word),
        };
        let attached = host
            .connection
            .as_ref()
            .is_some_and(|connection| *connection == self.connection);
        let enterable = attached || state == HostState::Connected;
        let endpoint = host.endpoint.clone();
        let bounds = Rc::clone(&self.compact.host_tiles);
        let tile = tile
            .child(
                canvas(
                    move |area, _, _| {
                        bounds.borrow_mut().insert(index, area);
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
                        this.connect_host(endpoint.clone(), window, cx);
                    }
                    _ => {}
                }),
            );
        let Some(held) = host.endpoint.clone() else {
            return tile;
        };
        let view = cx.entity().downgrade();
        tile.on_hold(move |_, cx| {
            let _ = view.update(cx, |this, cx| {
                this.compact.held = Some(held.clone());
                cx.notify();
            });
        })
    }

    fn machines_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let add = Some(
            TouchRow::new("hosts-add", "Add machine")
                .icon(IconName::Plus)
                .more()
                .on_click(cx.listener(|this, _, window, cx| this.open_add_machine(window, cx))),
        );
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let add: Option<TouchRow> = None;
        let settings = TouchRow::new("hosts-settings", "Settings")
            .icon(IconName::Settings)
            .more()
            .on_click(cx.listener(|this, _, _, cx| this.open_settings(cx)));
        div()
            .px(px(BENTO_INSET))
            .pt(Space::S24)
            .child(
                TouchGroup::new()
                    .ruled()
                    .rows(add.into_iter().chain([settings])),
            )
            .into_any_element()
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn open_add_machine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.compact.held = None;
        self.hosts.error = None;
        self.hosts.sheet = Some(super::super::hosts::HostSheet::Add);
        if self.found_machines(cx).is_empty() {
            let input = self.hosts.destination.clone();
            cx.on_next_frame(window, move |_, window, cx| {
                input.update(cx, |input, cx| input.focus(window, cx));
                window.request_virtual_keyboard();
            });
        }
        cx.notify();
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn found_list(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let found = self.found_machines(cx);
        let machines: Vec<_> = found.iter().map(|host| host.machine.clone()).collect();
        let endpoints: Vec<String> = found.into_iter().map(|host| host.endpoint).collect();
        let view = cx.entity().downgrade();
        let list = found_machines(&machines, move |index, window, cx| {
            let Some(endpoint) = endpoints.get(index).cloned() else {
                return;
            };
            let _ = view.update(cx, |this, cx| {
                this.hosts.saved.add(&endpoint);
                this.connect_host(endpoint, window, cx);
            });
        })?;
        Some(
            div()
                .px(px(BENTO_INSET))
                .pt(Space::S24)
                .child(list)
                .into_any_element(),
        )
    }

    fn open_machine_card(
        &mut self,
        card: MachineCard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        let MachineCard::Connecting(endpoint) = card;
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

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub(in super::super) fn found_machines(&self, cx: &App) -> Vec<crate::connection::FoundHost> {
        let known: Vec<zz_daemon_client::Endpoint> = self
            .hosts
            .saved
            .endpoints()
            .iter()
            .map(String::as_str)
            .chain(
                self.host_connections()
                    .iter()
                    .filter_map(|connection| connection.read(cx).endpoint()),
            )
            .filter_map(|endpoint| zz_daemon_client::Endpoint::parse(endpoint).ok())
            .collect();
        self.hosts
            .found
            .iter()
            .filter(|host| !crate::connection::already_known(host, &known))
            .cloned()
            .collect()
    }

    fn machine_card(
        &mut self,
        card: &MachineCard,
        top_inset: Pixels,
        bottom: Pixels,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ground = zoom_card_ground(cx);
        let close = cx.listener(|this, _, window, cx| this.close_machine_card(window, cx));
        match card {
            MachineCard::Connecting(endpoint) => {
                let target = endpoint.clone();
                ZoomCard::new("connecting-card", self.hosts.title(endpoint))
                    .top_inset(top_inset)
                    .bottom_inset(bottom)
                    .on_close(close)
                    .child(
                        div().mt(px(CARD_EMPTY_TOP)).child(
                            PhoneEmpty::spinner("Connecting")
                                .line(self.hosts.waiting(endpoint))
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

    pub(super) fn machine_sheet(&self, endpoint: &str, cx: &mut Context<Self>) -> AnyElement {
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
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let remove = {
            let removed = endpoint.to_owned();
            Some(
                TouchRow::new("machine-sheet-remove", "Remove machine")
                    .icon(IconName::Xmark)
                    .danger()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.disconnect_host(&removed, window, cx);
                        this.hosts.saved.remove(&removed);
                        this.hosts.saved.save();
                        close_sheet(MACHINE_SHEET, cx);
                        cx.notify();
                    })),
            )
        };
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let remove: Option<TouchRow> = None;
        let dismiss = cx.weak_entity();
        zz_ui::compact::hold_sheet(
            MACHINE_SHEET,
            self.hosts.title(endpoint),
            toggle_row.into_iter().chain(remove),
            move |_, cx| {
                let _ = dismiss.update(cx, |this, cx| {
                    this.compact.held = None;
                    cx.notify();
                });
            },
            cx,
        )
        .into_any_element()
    }

    pub(super) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.settings = Some(zz_ui::settings::SettingsSection::Appearance);
        self.focused_pane = None;
        cx.notify();
    }

    fn host_bar(
        &self,
        hosts: &[HostPage],
        index: usize,
        placement: &PillPlacement,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shown_line = match hosts.get(index).map(|host| host.state(cx)) {
            Some(HostState::Connected) => self
                .machine_session(cx)
                .map_or_else(|| "No sessions".to_owned(), |(_, name)| name),
            Some(HostState::Connecting) => "Connecting".to_owned(),
            _ => "Not connected".to_owned(),
        };
        let machines: Vec<StripMachine> = hosts
            .iter()
            .enumerate()
            .map(|(page, host)| StripMachine {
                mark: host.mark(cx),
                name: host.title.clone().into(),
                line: if page == index {
                    shown_line.clone()
                } else {
                    host.strip_line(cx)
                }
                .into(),
            })
            .collect();
        let stack = self.compact.menu.as_ref().map(|(menu, _)| menu.clone());
        let new_stack = self
            .compact
            .new_menu
            .as_ref()
            .map(|(menu, _)| menu.clone())
            .filter(|menu| menu.read(cx).has_groups());
        let menu = stack
            .as_ref()
            .map_or(0.0, |stack| stack.read(cx).close_shown());
        let made = new_stack
            .as_ref()
            .map_or(0.0, |stack| stack.read(cx).close_shown());
        let width = split_pill_bounds(
            window.fully_visible_bounds().size.width,
            placement,
            false,
            new_stack.is_some(),
        )
        .size
        .width;
        let view = cx.entity().downgrade();
        let pill = machine_strip(
            "overview-host",
            &machines,
            self.compact.host_pager.position(),
            width,
            placement,
            move |picked, window, cx| {
                view.update(cx, |this, cx| this.pick_host(picked, window, cx))
                    .ok();
            },
            window,
            cx,
        )
        .opacity((1.0 - menu) * (1.0 - made * MENU_DIM))
        .children(stack.map(|stack| stack.read(cx).track_anchor()))
        .child({
            let handle = pill_handle_bounds(Bounds::new(
                point(px(0.0), px(0.0)),
                size(width, px(COMPACT_PILL_HEIGHT)),
            ));
            div()
                .absolute()
                .left(handle.origin.x)
                .top(handle.origin.y)
                .child(
                    pill_handle("overview-host-handle", cx)
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            cx.haptic(ala::Haptic::Light);
                            this.open_hosts(cx);
                        }))
                        .on_scroll_wheel(cx.listener(Self::host_pill_scroll)),
                )
        })
        .on_scroll_wheel(cx.listener(Self::host_pill_scroll));
        let plus = new_stack.map(|stack| {
            compact_bar_disc("overview-host-new", IconName::Plus, "New", placement, cx)
                .opacity((1.0 - made) * (1.0 - MENU_DIM * menu))
                .child(StackMenu::pill_anchor(&stack, cx))
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.haptic(ala::Haptic::Light);
                    this.open_new_menu(cx);
                }))
        });
        compact_floating_bar(pill, placement, cx)
            .children(plus)
            .into_any_element()
    }

    fn pick_host(&mut self, picked: usize, window: &mut Window, cx: &mut Context<Self>) {
        cx.haptic(ala::Haptic::Light);
        if picked == self.compact.host_pager.index() {
            self.open_pill_menu(cx);
            return;
        }
        self.compact.close_sheets();
        self.compact.host_pager.go_to(picked, Instant::now());
        self.show_host(picked, window, cx);
        cx.notify();
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
            cx.haptic(ala::Haptic::Light);
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
            cx.haptic(ala::Haptic::Selection);
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
                    .child(large_title_lead(host.mark(cx), host.title.clone(), cx))
                    .children(body),
            )
            .child(folding_title_bar(
                host.title.clone(),
                scrolled(&scroll),
                frame.top,
                cx,
            ))
            .into_any_element()
    }

    fn empty_machine(&self, page: usize, grid: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let add = PhoneTile::add(("machine-new", page), "New")
            .on_click(cx.listener(|this, _, _, cx| this.open_new_menu(cx)));
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
        reveal: &mut Option<(usize, usize)>,
        drawn_panes: &mut Vec<PaneId>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let attached_session = attached_model.and(connection.read(cx).core.attached_session());
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
                            let (mark, _) = window_mark(&summary);
                            let closing = match window.panes.values().next() {
                                Some(only) if window.panes.len() == 1 => {
                                    close_pane_detail(&only.kind, &summary.title)
                                }
                                _ => Some(close_many_detail(window.panes.len(), "pane")),
                            };
                            WindowShape {
                                id: window.id,
                                closing,
                                tiles: summary.tiles,
                                title: summary.title,
                                detail: summary.detail,
                                mark,
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
            let cards: Vec<AnyElement> = model
                .windows
                .iter()
                .map(|window| {
                    let shape = shapes.iter().find(|shape| shape.id == window.id);
                    self.window_card(
                        window,
                        shape,
                        &pages,
                        connection,
                        target.clone(),
                        drawn_panes,
                        cx,
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
                    .child(header)
                    .child(v_flex().gap(px(BENTO_GAP)).children(cards))
                    .into_any_element(),
            );
        }
        sections
    }

    #[allow(clippy::too_many_arguments)]
    fn window_card(
        &self,
        window: &StatusBarWindow,
        shape: Option<&WindowShape>,
        pages: &[Page],
        connection: &Entity<Connection>,
        target: CardTarget,
        drawn_panes: &mut Vec<PaneId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = window
            .panes
            .iter()
            .find(|pane| pane.active)
            .or_else(|| window.panes.first())
            .map(|pane| pane.id);
        let shown: Vec<&Page> = window
            .panes
            .iter()
            .filter_map(|pane| pages.iter().find(|page| page.pane == pane.id))
            .collect();
        drawn_panes.extend(shown.iter().map(|page| page.pane));
        let unseen = shown.iter().any(|page| page.attention);
        let mark = shape
            .and_then(|shape| shape.mark)
            .or_else(|| unseen.then_some(StatusMark::Ink));
        let waiting = mark == Some(StatusMark::Warning);
        let ground = phone_card_ground(waiting, cx);
        let name = shape.map_or_else(|| window.label.clone(), |shape| shape.title.clone());
        let faces: Vec<(PaneTile, Attention)> = shape
            .map(|shape| {
                shape
                    .tiles
                    .iter()
                    .map(|tile| (tile.clone(), tile.attention))
                    .collect()
            })
            .unwrap_or_default();
        let (word, meta, tone) = shape.map_or_else(
            || (String::new(), String::new(), penna::Tone::Neutral),
            |shape| super::pill_words(&shape.detail),
        );
        let terminals: Vec<&Page> = shown
            .iter()
            .copied()
            .filter(|page| matches!(page.kind, PaneKindSnapshot::Terminal))
            .take(2)
            .collect();
        let tails: Vec<WindowTail> = {
            let core = &connection.read(cx).core;
            let base = font(cx.theme().mono_font_family.clone());
            terminals
                .iter()
                .map(|page| {
                    let text = core.viewport(page.pane).map_or_else(
                        || StyledText::new(page.title.clone()),
                        |viewport| {
                            terminal_preview(
                                viewport,
                                phone_colors(viewport, &host_palette(core), cx),
                                &base,
                                tail_rows(viewport),
                                PREVIEW_COLUMNS,
                            )
                        },
                    );
                    WindowTail::new(("window-tail", page.pane.0), text)
                })
                .collect()
        };
        let card = WindowCard::new(("overview-window", window.id.0), name)
            .faces(window_faces(&faces, ground, cx))
            .waiting(waiting)
            .working(mark == Some(StatusMark::Spinner))
            .line(meta);
        let card = match tone {
            penna::Tone::Warning => card.state(StatusMark::Warning, word),
            penna::Tone::Danger => card.state(StatusMark::Danger, word),
            _ => card,
        };
        let card = tails.into_iter().fold(card, WindowCard::tail);
        let opened = open.and_then(|open| pages.iter().find(|page| page.pane == open).cloned());
        let held = shape.map(|shape| Thing {
            connection: connection.clone(),
            target: Target::Window(shape.id),
            name: shape.title.clone(),
            closing: shape.closing.clone(),
        });
        let view = cx.entity().downgrade();
        let card = match held {
            Some(held) => card.on_hold(move |window, cx| {
                let held = held.clone();
                let _ = view.update(cx, |this, cx| {
                    this.open_phone_sheet(PhoneSheet::Hold(held), window, cx);
                });
            }),
            None => card,
        };
        card.on_click(cx.listener(move |this, _, window, cx| {
            let Some(page) = &opened else {
                return;
            };
            match &target {
                CardTarget::Here => {
                    this.compact_select(page, cx);
                    this.land_overview(Some(page.pane), cx);
                    return;
                }
                CardTarget::Session(session) => {
                    this.enter_session(*session, page.pane, cx);
                    return;
                }
                CardTarget::Host(endpoint, session) => {
                    this.switch_host(endpoint, Some(*session), Some(page.pane), window, cx);
                }
            }
            this.close_overview(cx);
        }))
        .into_any_element()
    }

    pub(super) fn mini_preview(
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
                || MiniPane::terminal(id, format!("$ {}", page.title)),
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
                PhoneEmpty::spinner("Connecting").line(
                    host.endpoint
                        .as_deref()
                        .map(|key| self.hosts.waiting(key))
                        .unwrap_or_default(),
                ),
                Button::new(("machine-cancel", page))
                    .label("Cancel")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(endpoint) = &endpoint {
                            this.disconnect_host(endpoint, window, cx);
                        }
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
                        if let Some(endpoint) = &endpoint {
                            this.connect_host(endpoint.clone(), window, cx);
                        }
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
            .child(large_title_lead(host.mark(cx), host.title.clone(), cx))
            .child(div().mt(px(CARD_EMPTY_TOP)).child(empty))
            .child(div().flex_1())
            .child(
                action_bar(frame.bottom + px(COMPACT_PILL_HEIGHT + Space::GAP_RELATED.design_px()))
                    .child(action.with_size(penna::Size::TouchLarge).w_full()),
            )
            .into_any_element()
    }

    fn enter_session(&mut self, session: SessionId, pane: PaneId, cx: &mut Context<Self>) {
        self.connection
            .update(cx, |connection, cx| connection.attach(session, cx));
        self.compact.close_sheets();
        self.compact.entering = Some((pane, Instant::now()));
        cx.notify();
    }

    pub(in super::super) fn host_pages(&self, cx: &App) -> Vec<HostPage> {
        let connections = self.host_connections();
        let saved = self.hosts.saved.endpoints();
        let mut pages: Vec<HostPage> = connections
            .iter()
            .filter_map(|connection| {
                let endpoint = connection.read(cx).endpoint()?;
                (!saved.iter().any(|saved| saved == endpoint)).then(|| HostPage {
                    id: self.host_ids.id(Some(endpoint)),
                    endpoint: Some(endpoint.to_owned()),
                    title: self.hosts.title(endpoint),
                    connection: Some(connection.clone()),
                })
            })
            .collect();
        pages.extend(saved.iter().map(|endpoint| {
            HostPage {
                id: self.host_ids.id(Some(endpoint)),
                endpoint: Some(endpoint.clone()),
                title: self.hosts.title(endpoint),
                connection: connections
                    .iter()
                    .find(|connection| connection.read(cx).endpoint() == Some(endpoint.as_str()))
                    .cloned(),
            }
        }));
        pages
    }

    pub(in super::super) fn host_page(&self, id: PaletteHostId, cx: &App) -> Option<HostPage> {
        self.host_pages(cx).into_iter().find(|page| page.id == id)
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
            let end = scroll.max_offset().y;
            let at_end = end > px(0.0) && -scroll.offset().y >= end - px(1.0);
            session_in_view(&tops, f32::from(self.compact.top_row), at_end)
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
    bottom: Pixels,
}

#[derive(Clone)]
enum CardTarget {
    Here,
    Session(SessionId),
    Host(String, SessionId),
}

struct WindowShape {
    id: WindowId,
    title: String,
    closing: Option<String>,
    mark: Option<StatusMark>,
    detail: Vec<DetailPart>,
    tiles: Vec<PaneTile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum MachineCard {
    Connecting(String),
}

#[derive(Clone, Copy)]
pub(in super::super) enum NewThing {
    Chat,
    Terminal,
    Browser,
    Session,
}

#[derive(Default)]
pub(crate) struct HostIds(RefCell<Vec<Option<String>>>);

impl HostIds {
    pub(crate) fn id(&self, endpoint: Option<&str>) -> PaletteHostId {
        let mut known = self.0.borrow_mut();
        let index = known
            .iter()
            .position(|known| known.as_deref() == endpoint)
            .unwrap_or_else(|| {
                known.push(endpoint.map(ToOwned::to_owned));
                known.len() - 1
            });
        PaletteHostId(u32::try_from(index).expect("fewer than u32::MAX hosts"))
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct HostPage {
    pub id: PaletteHostId,
    pub endpoint: Option<String>,
    pub title: String,
    pub connection: Option<Entity<Connection>>,
}

impl HostPage {
    fn platform(&self, cx: &App) -> zz_protocol::HostPlatform {
        self.connection
            .as_ref()
            .map_or(zz_protocol::HostPlatform::Unknown, |connection| {
                connection.read(cx).core.host_platform()
            })
    }

    fn mark(&self, cx: &App) -> IconName {
        zz_ui::program::platform_icon(self.platform(cx)).unwrap_or(IconName::Server)
    }

    fn strip_line(&self, cx: &App) -> String {
        match self.state(cx) {
            HostState::Connected => self
                .connection
                .as_ref()
                .and_then(|connection| {
                    connection
                        .read(cx)
                        .core
                        .snapshot()
                        .sessions
                        .first()
                        .map(|session| session_label(session).text)
                })
                .unwrap_or_else(|| "No sessions".to_owned()),
            HostState::Connecting => "Connecting".to_owned(),
            _ => "Not connected".to_owned(),
        }
    }

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
}
