use std::{ops::Range, rc::Rc};

use ala::{
    AnyElement, App, Bounds, Context, Entity, Font, Hsla, PaintScaleExt as _, ParentElement as _,
    Pixels, ScrollDelta, ScrollWheelEvent, Size, StyledText, TextRun, Window, canvas, div, font,
    linear_color_stop, linear_gradient, point, prelude::*, px, size,
};
use penna::{
    ActiveTheme as _, Colorize as _, IconName, Label, TextStyle,
    blocks::compact::{
        BENTO_INSET, BentoSpan, MiniPane, PhoneTile, PillLabel, PillPlacement, bento_grid,
        compact_bar_button, compact_floating_bar, compact_floating_group, compact_floating_pill,
        folding_title_bar, scrolled, window_span,
    },
    blocks::settings::large_title,
    blocks::touch::StatusMark,
    blocks::touch::{PageDot, PagerEvent, StackMenu, page_dots},
    components::button::{Button, ButtonVariants as _},
};
use web_time::Instant;
use zz_client::{
    AgentAttentionStatus, StatusBarModel, StatusBarWindow, agent_attention_status,
    navigation::session_label, pane_rects, window_summary::summarize_window,
};
use zz_protocol::{PaneId, PaneKindSnapshot, SessionId, WindowId};
use zz_terminal::{Glyph, LocalColors, TerminalAppearance, TerminalPalette, TerminalViewport};
use zz_ui::{
    compact::{
        HostState, host_mark, overview_count, overview_host_empty, overview_session_heading,
        window_mark,
    },
    terminal::terminal_background,
};

use super::{AppShell, Page, pages};
use crate::{connection::Connection, terminal::phone_colors};

const PREVIEW_ROWS: u16 = 40;
const PREVIEW_COLUMNS: usize = 56;
const HOST_GAP: f32 = 12.0;
const ZOOM_FADE_START: f32 = 0.3;
const ZOOM_FADE_SPAN: f32 = 0.4;
const TILE_FALLBACK_TOP: f32 = 160.0;
const TILE_FALLBACK_HEIGHT: f32 = 178.0;

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
        if progress < 1.0 || dragging {
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
        let zone = placement.zone();
        let mut reveal = None;
        let mut panes = Vec::new();
        let mut strip = Vec::new();
        for (page, x) in layout.pages {
            let Some(host) = hosts.get(page) else {
                continue;
            };
            let body = self.host_body(
                page,
                host,
                model,
                top_inset,
                zone,
                px(width),
                &mut reveal,
                &mut panes,
                cx,
            );
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
        let hosts = self.host_pages(cx);
        let width = window.viewport_size().width;
        let grid = (width - px(BENTO_INSET * 2.0)).max(px(0.0));
        let connected = hosts.iter().filter(|host| host.connected(cx)).count();
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let add = Some((
            BentoSpan::new(2, 2),
            PhoneTile::add("hosts-add", "Add host")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.compact.add_host = true;
                    cx.notify();
                }))
                .into_any_element(),
        ));
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let add: Option<(BentoSpan, AnyElement)> = None;
        let tiles: Vec<(BentoSpan, AnyElement)> = hosts
            .iter()
            .enumerate()
            .map(|(index, host)| (BentoSpan::new(2, 2), self.host_bento_tile(index, host, cx)))
            .chain(add)
            .collect();
        let subtitle = format!(
            "{} · {} connected",
            overview_count(hosts.len(), "host"),
            connected
        );
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
                    .child(large_title("Hosts", cx))
                    .child(
                        Label::new(TextStyle::Heading, subtitle)
                            .muted()
                            .px(px(BENTO_INSET)),
                    )
                    .child(
                        div()
                            .px(px(BENTO_INSET))
                            .pt(px(16.0))
                            .child(bento_grid(grid, tiles)),
                    ),
            )
            .child(folding_title_bar("Hosts", scrolled(&scroll), top_inset, cx))
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
            (HostState::Connecting, _) => host.endpoint.clone().into_iter().collect(),
            _ => Vec::new(),
        };
        let tiles = Rc::clone(&self.compact.host_tiles);
        let attached = host
            .connection
            .as_ref()
            .is_some_and(|connection| *connection == self.connection);
        let enterable = attached || state == HostState::Connected;
        let endpoint = host.endpoint.clone();
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
                        #[cfg(any(target_os = "ios", target_os = "android"))]
                        this.disconnect_host(endpoint, window, cx);
                        let _ = endpoint;
                    }
                    _ if enterable => this.enter_host(index, window, cx),
                    (HostState::Offline, Some(endpoint)) => {
                        #[cfg(any(target_os = "ios", target_os = "android"))]
                        this.connect_host(endpoint.clone(), window, cx);
                        let _ = endpoint;
                    }
                    _ => {}
                }),
            )
            .into_any_element()
    }

    pub(super) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.settings = Some(zz_ui::settings::SettingsSection::Appearance);
        self.focused_pane = None;
        cx.notify();
    }

    pub(super) fn shown_host_connected(&self, cx: &App) -> bool {
        self.host_pages(cx)
            .get(self.compact.host_pager.index())
            .is_some_and(|host| host.connected(cx))
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
        let meta = host.map_or_else(String::new, |host| host.detail(cx));
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
                meta: meta.into(),
                warning: false,
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

    #[allow(clippy::too_many_arguments)]
    fn host_body(
        &mut self,
        page: usize,
        host: &HostPage,
        model: &StatusBarModel,
        top_inset: Pixels,
        zone: Pixels,
        width: Pixels,
        reveal: &mut Option<(usize, usize)>,
        panes: &mut Vec<PaneId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let attached = host
            .connection
            .as_ref()
            .is_some_and(|connection| *connection == self.connection);
        if !attached && !host.connected(cx) {
            return self.empty_host(host, cx);
        }
        let connection = host
            .connection
            .clone()
            .unwrap_or_else(|| self.connection.clone());
        let grid = (width - px(BENTO_INSET * 2.0)).max(px(0.0));
        let sections = self.host_sections(
            page,
            host,
            &connection,
            attached.then_some(model),
            grid,
            reveal,
            panes,
            cx,
        );
        if sections.is_empty() {
            return self.empty_host(host, cx);
        }
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
                    .pt(top_inset)
                    .pb(zone + px(16.0))
                    .children(sections),
            )
            .child(folding_title_bar(
                host.title.clone(),
                scrolled(&scroll),
                top_inset,
                cx,
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
                            let (mark, state) = window_mark(&summary);
                            WindowShape {
                                id: window.id,
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
        let mut sections = vec![large_title(host.title.clone(), cx).into_any_element()];
        for (session, name, model, shapes) in sessions {
            let live = attached_session == Some(session);
            let (pages, _) = pages(&model, &self.unseen_agents);
            let target = if live {
                CardTarget::Here
            } else if attached_model.is_some() {
                CardTarget::Session(session)
            } else {
                CardTarget::Host(host.endpoint.clone().unwrap_or_default(), session)
            };
            sections.push(overview_session_heading(&name, cx).into_any_element());
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
                *reveal = Some((page, sections.len()));
            }
            sections.push(
                div()
                    .px(px(BENTO_INSET))
                    .pb(px(8.0))
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

    fn empty_host(&self, host: &HostPage, cx: &mut Context<Self>) -> AnyElement {
        let (connected, busy, status) =
            host.connection
                .as_ref()
                .map_or((false, false, String::new()), |connection| {
                    let connection = connection.read(cx);
                    (
                        connection.connected,
                        connection.busy_or_connecting(),
                        connection.status.clone(),
                    )
                });
        let endpoint = host.endpoint.clone();
        let (detail, action): (String, Option<AnyElement>) = if connected {
            (
                "No sessions yet.".into(),
                Some(
                    Button::new("overview-empty-new-session")
                        .label("New session")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.create(NewThing::Session, window, cx);
                        }))
                        .into_any_element(),
                ),
            )
        } else if busy {
            (
                "Connecting…".into(),
                endpoint.map(|endpoint| {
                    Button::new("overview-empty-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            #[cfg(any(target_os = "ios", target_os = "android"))]
                            this.disconnect_host(&endpoint, window, cx);
                            let _ = (this, &endpoint, window, cx);
                        }))
                        .into_any_element()
                }),
            )
        } else {
            let detail = Some(status)
                .filter(|status| !status.is_empty() && status != "Not connected")
                .unwrap_or_else(|| {
                    format!(
                        "{} is not connected. Your other hosts stay connected while it signs in.",
                        host.endpoint.as_deref().unwrap_or(&host.title)
                    )
                });
            (
                detail,
                endpoint.map(|endpoint| {
                    Button::new("overview-empty-connect")
                        .label("Connect")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            #[cfg(any(target_os = "ios", target_os = "android"))]
                            this.connect_host(endpoint.clone(), window, cx);
                            let _ = (this, &endpoint, window, cx);
                        }))
                        .into_any_element()
                }),
            )
        };
        overview_host_empty(host.title.clone(), detail, action, cx).into_any_element()
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Some(endpoint) = self
            .host_pages(cx)
            .get(self.compact.host_pager.index())
            .and_then(|host| host.endpoint.clone())
            && !self.is_attached_host(&endpoint, cx)
        {
            self.switch_host(&endpoint, None, None, window, cx);
        }
        let _ = window;
        let here = || vec!["-c".to_owned(), "#{pane_current_path}".to_owned()];
        match what {
            NewThing::Terminal => self.command("new-window", here(), cx),
            NewThing::Browser => {
                self.command("new-window", vec!["--kind".into(), "browser".into()], cx);
            }
            NewThing::Session => self.command("new-session", Vec::new(), cx),
        }
        self.close_overview(cx);
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub(super) fn overview_add_host_sheet(
        &self,
        bottom_inset: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.add_host_sheet(
            bottom_inset,
            |this, cx| {
                this.compact.add_host = false;
                let count = this.host_pages(cx).len();
                let shown = this.compact.host_pager.index();
                this.compact.host_pager.sync(shown, count);
                this.compact
                    .host_pager
                    .go_to(count.saturating_sub(1), Instant::now());
                cx.notify();
            },
            |this, cx| {
                this.compact.add_host = false;
                cx.notify();
            },
            cx,
        )
    }
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
    mark: Option<StatusMark>,
    state: Option<String>,
    rects: Vec<(PaneId, [f32; 4])>,
}

#[derive(Clone, Copy)]
pub(in super::super) enum NewThing {
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
        match &self.connection {
            Some(connection) if connection.read(cx).connected => HostState::Connected,
            Some(connection) if connection.read(cx).busy_or_connecting() => HostState::Connecting,
            _ => HostState::Offline,
        }
    }

    fn detail(&self, cx: &App) -> String {
        match &self.connection {
            Some(connection) if connection.read(cx).connected => overview_count(
                connection.read(cx).core.snapshot().sessions.len(),
                "session",
            ),
            Some(connection) if connection.read(cx).busy_or_connecting() => "Connecting…".into(),
            _ => "Not connected".into(),
        }
    }
}
