use std::{
    rc::Rc,
    sync::{Arc, LazyLock},
};

use zz_client::window_summary::{
    Attention, AttentionRollup, PaneTag, SessionSummary, Tone as DetailTone, WindowSummary,
};
use zz_gpui::{
    App, ClickEvent, CornerRadiusMode, Div, ElementId, FontFeatures, FontWeight, HighlightStyle,
    Hsla, IntoElement, ParentElement as _, Pixels, SharedString, Stateful, StyledText, Window, div,
    prelude::*, px,
};
use zz_protocol::{AgentProvider, SessionId, WindowId};

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Ground, Icon, IconName, Paint, PaintState,
    Sizable as _, StyledExt as _, Tone,
    button::{Button, ButtonVariants as _},
    h_flex,
    icon::cap_center_drop,
    spinner::Spinner,
    v_flex,
};

pub const FLAT_ROW_HEIGHT: f32 = 48.0;
const ROW_PADDING: f32 = 10.0;
const TILE_SLOT: f32 = 28.0;
const TILE_GAP: f32 = 10.0;
const TILE_GLYPH: f32 = 16.0;
const STACK_TILE: f32 = 0.58;
const STACK_OFFSET: f32 = 0.42;
const STACK_GLYPH: f32 = 0.6;
const STACK_RING: f32 = 1.5;
const STACK_LIMIT: usize = 3;
const DOT_SIZE: f32 = 8.0;
const DOT_RING: f32 = 2.0;
const DOT_OFFSET: f32 = 2.0;
const TITLE_SIZE: f32 = 13.0;
const TITLE_LINE: f32 = 17.0;
const DETAIL_SIZE: f32 = 12.0;
const DETAIL_LINE: f32 = 15.0;
const AGE_SIZE: f32 = 11.0;
const LINE_GAP: f32 = 1.0;
const TITLE_TRAILING_GAP: f32 = 6.0;
const DETAIL_AGE_GAP: f32 = 8.0;
const SPINNER_SIZE: f32 = 12.0;
const ACTIONS_WIDTH: f32 = 20.0;
const ACTIONS_HEIGHT: f32 = 16.0;
const ACTIONS_RADIUS_MAX: f32 = 8.0;
const DIMMED: f32 = 0.45;
const HEADER_INSET: f32 = 16.0;
const HEADER_TRAILING_INSET: f32 = 12.0;
const HOST_HEIGHT: f32 = 30.0;
const HOST_GAP: f32 = 18.0;
const HOST_GLYPH: f32 = 16.0;
const HOST_NAME_SIZE: f32 = 13.0;
const HOST_LINE: f32 = 16.0;
const HOST_SPACING: f32 = 8.0;
const SESSION_TOP: f32 = 6.0;
const SESSION_BOTTOM: f32 = 2.0;
const SESSION_LINE: f32 = 18.0;
const SESSION_SIZE: f32 = 12.0;
const SESSION_SPACING: f32 = 6.0;
const NEW_WINDOW_SIZE: f32 = 18.0;
const CHEVRON_SIZE: f32 = 10.0;
const NOTE_SIZE: f32 = 11.0;
const NOTE_LINE: f32 = 15.0;
const HEADER_BUTTON_HEIGHT: f32 = 20.0;
const ROLLUP_DOT: f32 = 7.0;
const ROLLUP_SPINNER: f32 = 10.0;
const ROLLUP_GAP: f32 = 8.0;
const ROLLUP_ITEM_GAP: f32 = 4.0;

static TABULAR: LazyLock<FontFeatures> =
    LazyLock::new(|| FontFeatures(Arc::new(vec![("tnum".into(), 1)])));

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostConnection {
    Connected,
    Connecting,
    Reconnecting,
    Unreachable,
    Incompatible(SharedString),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatWindow {
    pub summary: WindowSummary,
    pub age: Option<SharedString>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatSession {
    pub id: SessionId,
    pub label: SharedString,
    pub rollup: AttentionRollup,
    pub folded: bool,
    pub windows: Vec<FlatWindow>,
}

impl FlatSession {
    pub fn new(summary: SessionSummary, age: impl Fn(u64) -> Option<String>) -> Self {
        Self {
            id: summary.session,
            label: summary.label.into(),
            rollup: summary.rollup,
            folded: false,
            windows: summary
                .windows
                .into_iter()
                .map(|window| FlatWindow {
                    age: age(window.changed_at).map(Into::into),
                    summary: window,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatHost {
    pub name: SharedString,
    pub remote: bool,
    pub connection: HostConnection,
    pub folded: bool,
    pub rollup: AttentionRollup,
    pub sessions: Vec<FlatSession>,
}

impl FlatHost {
    pub fn new(
        name: impl Into<SharedString>,
        remote: bool,
        connection: HostConnection,
        (sessions, rollup): (Vec<SessionSummary>, AttentionRollup),
        age: impl Fn(u64) -> Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            remote,
            connection,
            folded: false,
            rollup,
            sessions: sessions
                .into_iter()
                .map(|session| FlatSession::new(session, &age))
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatTarget {
    Host(usize),
    Session(usize, SessionId),
    Window(usize, WindowId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatEvent {
    Open(usize, WindowId),
    Actions(usize, WindowId),
    Fold(FlatTarget),
    NewWindow(usize, SessionId),
    Retry(usize),
    Upgrade(usize),
    Hover(FlatTarget, bool),
}

pub type FlatHandler = Rc<dyn Fn(FlatEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlatState {
    pub current: Option<(usize, WindowId)>,
    pub hovered: Option<FlatTarget>,
}

impl FlatState {
    pub fn hover(&mut self, target: FlatTarget, hovered: bool) -> bool {
        let next = if hovered {
            Some(target)
        } else if self.hovered == Some(target) {
            None
        } else {
            self.hovered
        };
        std::mem::replace(&mut self.hovered, next) != next
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlatRowState {
    pub current: bool,
    pub hovered: bool,
    pub dimmed: bool,
}

impl FlatRowState {
    fn paint_state(self) -> PaintState {
        if self.current {
            PaintState::Current
        } else if self.hovered {
            PaintState::Hover
        } else {
            PaintState::Rest
        }
    }
}

pub fn pane_glyph(tag: PaneTag) -> IconName {
    match tag {
        PaneTag::Agent(AgentProvider::ClaudeCode) => IconName::Asterisk,
        PaneTag::Agent(AgentProvider::Codex) => IconName::Braces,
        PaneTag::Terminal => IconName::Terminal,
        PaneTag::Browser => IconName::Globe,
        PaneTag::Editor => IconName::File,
        PaneTag::Picker => IconName::Plus,
    }
}

pub fn attention_color(attention: Attention, cx: &App) -> Option<Hsla> {
    let theme = cx.theme();
    match attention {
        Attention::Done => Some(theme.accent),
        Attention::NeedsYou | Attention::Bell => Some(theme.warning),
        Attention::Failed => Some(theme.danger),
        Attention::Working | Attention::None => None,
    }
}

pub fn flat_row_ground(state: FlatRowState, cx: &App) -> Ground {
    cx.theme()
        .paint(Tone::Neutral, Emphasis::Ghost)
        .ground_at(state.paint_state())
}

fn row_height(cx: &App) -> Pixels {
    cx.theme().dense(px(FLAT_ROW_HEIGHT))
}

fn row_radius(cx: &App) -> Pixels {
    cx.theme().control_radius().min(row_height(cx) / 2.0)
}

fn tile_radius(cx: &App) -> Pixels {
    let inset = (row_height(cx) - px(TILE_SLOT)) / 2.0;
    (row_radius(cx) - inset)
        .max(cx.theme().radius * 0.25)
        .min(px(TILE_SLOT / 2.0))
}

fn tile(tag: Option<PaneTag>, size: f32, radius: Pixels, glyph: f32, paint: &Paint) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size))
        .rounded(radius)
        .corner_radius_mode(CornerRadiusMode::Fixed)
        .bg(paint.fill)
        .border(paint.border_width)
        .border_color(paint.border)
        .text_color(paint.text)
        .children(tag.map(|tag| Icon::new(pane_glyph(tag)).size(px(glyph))))
}

pub fn flat_tile_stack(tiles: &[PaneTag], ground: Ground, cx: &App) -> Div {
    let paint = cx
        .theme()
        .paint_on(ground, Tone::Neutral, Emphasis::Soft)
        .opaque();
    let radius = tile_radius(cx);
    let slot = div().relative().flex_none().size(px(TILE_SLOT));
    let shown = &tiles[..tiles.len().min(STACK_LIMIT)];
    if shown.len() < 2 {
        return slot.child(tile(
            shown.first().copied(),
            TILE_SLOT,
            radius,
            TILE_GLYPH,
            &paint,
        ));
    }
    let offsets: &[(f32, f32)] = if shown.len() == 2 {
        &[(0.0, 0.0), (STACK_OFFSET, STACK_OFFSET)]
    } else {
        &[
            (0.0, 0.0),
            (STACK_OFFSET, 0.0),
            (STACK_OFFSET / 2.0, STACK_OFFSET),
        ]
    };
    let size = TILE_SLOT * STACK_TILE;
    let radius = radius * STACK_TILE;
    let glyph = (size * STACK_GLYPH).round();
    let back_to_front = shown[1..].iter().rev().chain(&shown[..1]);
    slot.children(back_to_front.zip(offsets).map(|(tag, (x, y))| {
        div()
            .absolute()
            .left(px(x * TILE_SLOT - STACK_RING))
            .top(px(y * TILE_SLOT - STACK_RING))
            .p(px(STACK_RING))
            .rounded(radius + px(STACK_RING))
            .corner_radius_mode(CornerRadiusMode::Fixed)
            .bg(ground.fill)
            .child(tile(Some(*tag), size, radius, glyph, &paint))
    }))
}

pub fn flat_attention_dot(attention: Attention, ground: Ground, cx: &App) -> Option<Div> {
    let color = attention_color(attention, cx)?;
    let ring = DOT_SIZE + DOT_RING * 2.0;
    Some(
        div()
            .absolute()
            .top(px(-DOT_OFFSET - DOT_RING))
            .right(px(-DOT_OFFSET - DOT_RING))
            .size(px(ring))
            .p(px(DOT_RING))
            .rounded_full()
            .bg(ground.fill)
            .child(div().size(px(DOT_SIZE)).rounded_full().bg(color)),
    )
}

fn spinner(size: f32, line_font: f32, cx: &App) -> Div {
    div()
        .flex_none()
        .relative()
        .top(cap_center_drop(px(line_font), px(size), cx))
        .child(
            Spinner::new()
                .with_size(px(size))
                .color(cx.theme().foreground.muted()),
        )
}

fn glyph(icon: IconName, size: f32, line_font: f32, color: Hsla, cx: &App) -> Div {
    div()
        .flex_none()
        .relative()
        .top(cap_center_drop(px(line_font), px(size), cx))
        .child(Icon::new(icon).size(px(size)).text_color(color))
}

fn detail_text(window: &WindowSummary, paint: &Paint, state: PaintState, cx: &App) -> StyledText {
    let theme = cx.theme();
    let mut text = String::new();
    let mut highlights = Vec::with_capacity(window.detail.len());
    for part in &window.detail {
        let start = text.len();
        text.push_str(&part.text);
        let color = match part.tone {
            DetailTone::Normal => paint.text_at(state),
            DetailTone::Muted => paint.muted_at(state),
            DetailTone::Warning => theme.warning,
            DetailTone::Danger => theme.danger,
        };
        highlights.push((
            start..text.len(),
            HighlightStyle {
                color: Some(color),
                ..HighlightStyle::default()
            },
        ));
    }
    StyledText::new(text).with_highlights(highlights)
}

fn actions_button(host: usize, window: WindowId, ground: Ground, handler: &FlatHandler) -> Button {
    let handler = handler.clone();
    Button::new(("flat-window-actions", window.0))
        .icon(IconName::Ellipsis)
        .with_size(px(ACTIONS_HEIGHT))
        .w(px(ACTIONS_WIDTH))
        .flat()
        .on(ground)
        .tooltip("Window actions")
        .on_click(move |_: &ClickEvent, window_handle, cx| {
            cx.stop_propagation();
            handler(FlatEvent::Actions(host, window), window_handle, cx);
        })
}

pub fn flat_window_row(
    host: usize,
    window: &FlatWindow,
    state: FlatRowState,
    handler: &FlatHandler,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let paint = theme.paint(Tone::Neutral, Emphasis::Ghost);
    let paint_state = state.paint_state();
    let ground = paint.ground_at(paint_state);
    let summary = &window.summary;
    let id = summary.window;
    let working = summary.attention == Attention::Working;
    let muted = paint.muted_at(paint_state);
    let target = FlatTarget::Window(host, id);
    let trailing = if state.hovered {
        Some(
            actions_button(host, id, ground, handler)
                .rounded((theme.radius / 2.0).min(px(ACTIONS_RADIUS_MAX)))
                .into_any_element(),
        )
    } else if working {
        Some(spinner(SPINNER_SIZE, TITLE_SIZE, cx).into_any_element())
    } else {
        None
    };
    let age = window.age.clone().filter(|_| !working);
    let hover = handler.clone();
    let click = handler.clone();
    div()
        .id(ElementId::NamedInteger(
            format!("flat-window-{host}").into(),
            id.0,
        ))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(TILE_GAP))
        .h(row_height(cx))
        .mx(theme.row_inset)
        .px(px(ROW_PADDING))
        .rounded(row_radius(cx))
        .corner_radius_mode(CornerRadiusMode::Fixed)
        .bg(paint.fill_at(paint_state))
        .text_color(paint.text_at(paint_state))
        .when(state.dimmed, |row| row.opacity(DIMMED))
        .cursor_pointer()
        .on_hover(move |hovered, window, cx| hover(FlatEvent::Hover(target, *hovered), window, cx))
        .on_click(move |_, window, cx| click(FlatEvent::Open(host, id), window, cx))
        .child(
            flat_tile_stack(&summary.tiles, ground, cx).children(flat_attention_dot(
                summary.attention,
                ground,
                cx,
            )),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(LINE_GAP))
                .child(
                    h_flex()
                        .min_w_0()
                        .h(px(TITLE_LINE))
                        .gap(px(TITLE_TRAILING_GAP))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(TITLE_SIZE))
                                .line_height(px(TITLE_LINE))
                                .font_medium()
                                .child(SharedString::from(summary.title.clone())),
                        )
                        .children(trailing),
                )
                .child(
                    h_flex()
                        .min_w_0()
                        .h(px(DETAIL_LINE))
                        .gap(px(DETAIL_AGE_GAP))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(DETAIL_SIZE))
                                .line_height(px(DETAIL_LINE))
                                .text_color(muted)
                                .child(detail_text(summary, &paint, paint_state, cx)),
                        )
                        .children(age.map(|age| {
                            div()
                                .flex_none()
                                .text_size(px(AGE_SIZE))
                                .line_height(px(DETAIL_LINE))
                                .font_features(TABULAR.clone())
                                .text_color(muted)
                                .child(age)
                        })),
                ),
        )
}

pub fn flat_rollup(rollup: &AttentionRollup, cx: &App) -> Div {
    let theme = cx.theme();
    let muted = theme.foreground.muted();
    let count = |value: u32| div().child(SharedString::from(value.to_string()));
    let dots = [
        (rollup.needs_you.saturating_add(rollup.bell), theme.warning),
        (rollup.failed, theme.danger),
        (rollup.done, theme.accent),
    ];
    h_flex()
        .flex_none()
        .gap(px(ROLLUP_GAP))
        .text_size(px(NOTE_SIZE))
        .line_height(px(NOTE_LINE))
        .font_features(TABULAR.clone())
        .font_weight(FontWeight::NORMAL)
        .text_color(muted)
        .when(rollup.working > 0, |this| {
            this.child(
                h_flex()
                    .gap(px(ROLLUP_ITEM_GAP))
                    .child(spinner(ROLLUP_SPINNER, NOTE_SIZE, cx))
                    .child(count(rollup.working)),
            )
        })
        .children(
            dots.into_iter()
                .filter(|(value, _)| *value > 0)
                .map(|(value, color)| {
                    h_flex()
                        .gap(px(ROLLUP_ITEM_GAP))
                        .child(
                            div()
                                .flex_none()
                                .relative()
                                .top(cap_center_drop(px(NOTE_SIZE), px(ROLLUP_DOT), cx))
                                .size(px(ROLLUP_DOT))
                                .rounded_full()
                                .bg(color),
                        )
                        .child(count(value))
                }),
        )
}

fn header_note(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .flex_none()
        .whitespace_nowrap()
        .text_size(px(NOTE_SIZE))
        .line_height(px(NOTE_LINE))
        .font_weight(FontWeight::NORMAL)
        .text_color(color)
        .child(text.into())
}

fn header_button(
    id: impl Into<ElementId>,
    label: &'static str,
    event: FlatEvent,
    handler: &FlatHandler,
) -> Button {
    let handler = handler.clone();
    Button::new(id)
        .label(label)
        .small()
        .h(px(HEADER_BUTTON_HEIGHT))
        .font_weight(FontWeight::NORMAL)
        .flat()
        .on_click(move |_: &ClickEvent, window, cx| {
            cx.stop_propagation();
            handler(event, window, cx);
        })
}

fn chevron(folded: bool, hovered: bool, line_font: f32, cx: &App) -> Option<Div> {
    let icon = if folded {
        IconName::ChevronRight
    } else if hovered {
        IconName::ChevronDown
    } else {
        return None;
    };
    Some(glyph(
        icon,
        CHEVRON_SIZE,
        line_font,
        cx.theme().foreground.muted(),
        cx,
    ))
}

pub fn flat_host_header(
    index: usize,
    host: &FlatHost,
    hovered: bool,
    handler: &FlatHandler,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let muted = theme.foreground.muted();
    let target = FlatTarget::Host(index);
    let (state, action) = match &host.connection {
        HostConnection::Connected => (None, None),
        HostConnection::Connecting => (Some(("Connecting…".into(), muted)), None),
        HostConnection::Reconnecting => (Some(("Reconnecting…".into(), muted)), None),
        HostConnection::Unreachable => (
            Some(("Unreachable".into(), theme.danger)),
            Some(("Retry", FlatEvent::Retry(index))),
        ),
        HostConnection::Incompatible(version) => (
            Some((version.clone(), theme.warning)),
            Some(("Upgrade", FlatEvent::Upgrade(index))),
        ),
    };
    let rollup = host.folded.then(|| flat_rollup(&host.rollup, cx));
    let ssh = (host.remote && host.connection == HostConnection::Connected && !host.folded)
        .then(|| header_note("ssh", muted));
    let hover = handler.clone();
    let click = handler.clone();
    h_flex()
        .id(("flat-host", index))
        .flex_none()
        .h(theme.dense(px(HOST_HEIGHT)))
        .pl(px(HEADER_INSET))
        .pr(px(HEADER_TRAILING_INSET))
        .gap(px(HOST_SPACING))
        .text_color(theme.foreground)
        .text_size(px(HOST_NAME_SIZE))
        .line_height(px(HOST_LINE))
        .font_semibold()
        .cursor_pointer()
        .on_hover(move |hovered, window, cx| hover(FlatEvent::Hover(target, *hovered), window, cx))
        .on_click(move |_, window, cx| click(FlatEvent::Fold(target), window, cx))
        .child(glyph(
            if host.remote {
                IconName::Server
            } else {
                IconName::Laptop
            },
            HOST_GLYPH,
            HOST_NAME_SIZE,
            muted,
            cx,
        ))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(host.name.clone()),
        )
        .children(chevron(host.folded, hovered, HOST_NAME_SIZE, cx))
        .child(div().flex_1())
        .children(rollup)
        .children(ssh)
        .children(state.map(|(text, color)| header_note(text, color)))
        .children(action.map(|(label, event)| {
            header_button(("flat-host-action", index), label, event, handler)
        }))
}

pub fn flat_session_header(
    host: usize,
    session: &FlatSession,
    holds_current: bool,
    hovered: bool,
    handler: &FlatHandler,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let target = FlatTarget::Session(host, session.id);
    let rollup = session.folded.then(|| flat_rollup(&session.rollup, cx));
    let new_window = hovered.then(|| {
        let handler = handler.clone();
        let session = session.id;
        Button::new(("flat-new-window", session.0))
            .icon(IconName::Plus)
            .ghost()
            .with_size(px(NEW_WINDOW_SIZE))
            .tooltip("New window")
            .on_click(move |_: &ClickEvent, window, cx| {
                cx.stop_propagation();
                handler(FlatEvent::NewWindow(host, session), window, cx);
            })
    });
    let hover = handler.clone();
    let click = handler.clone();
    div()
        .id(ElementId::NamedInteger(
            format!("flat-session-{host}").into(),
            session.id.0,
        ))
        .flex_none()
        .pt(px(SESSION_TOP))
        .pb(px(SESSION_BOTTOM))
        .pl(px(HEADER_INSET))
        .pr(px(HEADER_TRAILING_INSET))
        .cursor_pointer()
        .on_hover(move |hovered, window, cx| hover(FlatEvent::Hover(target, *hovered), window, cx))
        .on_click(move |_, window, cx| click(FlatEvent::Fold(target), window, cx))
        .child(
            h_flex()
                .h(px(SESSION_LINE))
                .gap(px(SESSION_SPACING))
                .text_size(px(SESSION_SIZE))
                .line_height(px(SESSION_LINE))
                .font_semibold()
                .text_color(if holds_current || hovered {
                    theme.foreground
                } else {
                    theme.foreground.muted()
                })
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(session.label.clone()),
                )
                .children(chevron(session.folded, hovered, SESSION_SIZE, cx))
                .child(div().flex_1())
                .children(rollup)
                .children(new_window),
        )
}

pub fn flat_sidebar(hosts: &[FlatHost], state: FlatState, handler: &FlatHandler, cx: &App) -> Div {
    let mut list = v_flex().w_full().flex_none();
    for (index, host) in hosts.iter().enumerate() {
        list = list.child(
            flat_host_header(
                index,
                host,
                state.hovered == Some(FlatTarget::Host(index)),
                handler,
                cx,
            )
            .when(index > 0, |header| header.mt(px(HOST_GAP))),
        );
        if host.folded {
            continue;
        }
        let dimmed = host.connection != HostConnection::Connected;
        for session in &host.sessions {
            let holds_current = state.current.is_some_and(|(current_host, window)| {
                current_host == index
                    && session
                        .windows
                        .iter()
                        .any(|candidate| candidate.summary.window == window)
            });
            list = list.child(flat_session_header(
                index,
                session,
                holds_current,
                state.hovered == Some(FlatTarget::Session(index, session.id)),
                handler,
                cx,
            ));
            if session.folded {
                continue;
            }
            list = list.children(session.windows.iter().map(|window| {
                let id = window.summary.window;
                flat_window_row(
                    index,
                    window,
                    FlatRowState {
                        current: state.current == Some((index, id)),
                        hovered: state.hovered == Some(FlatTarget::Window(index, id)),
                        dimmed,
                    },
                    handler,
                    cx,
                )
            }));
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_survives_entering_the_next_row_before_leaving_the_last() {
        let first = FlatTarget::Window(0, WindowId(1));
        let second = FlatTarget::Window(0, WindowId(2));
        let mut state = FlatState::default();
        assert!(state.hover(first, true));
        assert!(state.hover(second, true));
        assert!(!state.hover(first, false));
        assert_eq!(state.hovered, Some(second));
        assert!(state.hover(second, false));
        assert_eq!(state.hovered, None);
    }
}
