use std::{
    rc::Rc,
    sync::{Arc, LazyLock},
};

use zz_client::window_summary::{
    Attention, AttentionRollup, PaneTag, PaneTile, SessionSummary, Tone as DetailTone,
    WindowSummary,
};
use zz_gpui::{
    Anchor, AnyElement, App, ClickEvent, Context, Div, ElementId, FontFeatures, FontWeight,
    HighlightStyle, Hsla, IntoElement, ParentElement as _, Pixels, Role, SharedString, Stateful,
    StyledText, Window, div, font, prelude::*, px,
};
use zz_protocol::{AgentProvider, PaneId, SessionId, WindowId};

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Ground, Icon, IconName, PaintState, Sizable as _,
    StyledExt as _, Tone,
    button::{Button, ButtonVariants as _},
    command::PaletteHostId,
    h_flex,
    icon::cap_center_drop,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu},
    navigation::{
        pane_stack::{PaneStack, StackAction, StackSize, StackTile, stack_radius},
        workspace_row_fill,
    },
    spinner::Spinner,
    tooltip::Tooltip,
    touch::{CoarsePointer, TOUCH_TARGET},
    v_flex,
};

const ROW_HEIGHT: f32 = 48.0;
const ROW_PADDING: f32 = 10.0;
const TILE_GAP: f32 = 10.0;
const TITLE_SIZE: f32 = 13.0;
const TITLE_LINE: f32 = 17.0;
const DETAIL_SIZE: f32 = 12.0;
const DETAIL_LINE: f32 = 15.0;
const AGE_SIZE: f32 = 11.0;
const LINE_GAP: f32 = 1.0;
const TITLE_TRAILING_GAP: f32 = 6.0;
const DETAIL_AGE_GAP: f32 = 8.0;
const SPINNER_SIZE: f32 = 12.0;
const ACTIONS_WIDTH: f32 = 18.0;
const ACTIONS_HEIGHT: f32 = 16.0;
const ACTIONS_ICON: f32 = 14.0;
const DIMMED: f32 = 0.45;
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
const NEW_WINDOW_ICON: f32 = 12.0;
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
    Disconnected,
    Unreachable(SharedString),
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
    pub index: Option<SharedString>,
    pub rollup: AttentionRollup,
    pub folded: bool,
    pub windows: Vec<FlatWindow>,
}

impl FlatSession {
    pub fn new(summary: SessionSummary, age: impl Fn(u64) -> Option<String>) -> Self {
        Self {
            id: summary.session,
            label: summary.label.text.into(),
            index: summary.label.index.map(Into::into),
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
    pub id: PaletteHostId,
    pub name: SharedString,
    pub remote: bool,
    pub connection: HostConnection,
    pub folded: bool,
    pub rollup: AttentionRollup,
    pub sessions: Vec<FlatSession>,
}

impl FlatHost {
    pub fn new(
        id: PaletteHostId,
        name: impl Into<SharedString>,
        remote: bool,
        connection: HostConnection,
        (sessions, rollup): (Vec<SessionSummary>, AttentionRollup),
        age: impl Fn(u64) -> Option<String>,
    ) -> Self {
        Self {
            id,
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

    fn live(&self) -> bool {
        self.connection == HostConnection::Connected
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatTarget {
    Host(PaletteHostId),
    Session(PaletteHostId, SessionId),
    Window(PaletteHostId, WindowId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatEvent {
    Open(PaletteHostId, WindowId),
    OpenPane(PaletteHostId, WindowId, PaneId),
    FoldHost(PaletteHostId),
    FoldSession(PaletteHostId, SessionId),
    NewWindow(PaletteHostId, SessionId),
    Connect(PaletteHostId),
    Upgrade(PaletteHostId),
    Hover(FlatTarget, bool),
}

pub type FlatHandler = Rc<dyn Fn(FlatEvent, &mut Window, &mut App)>;

/// Fills a window row's menu, opened from its actions button or a right
/// click.
pub type FlatMenu = Rc<
    dyn Fn(PaletteHostId, WindowId, PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu,
>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlatState {
    pub current: Option<(PaletteHostId, WindowId)>,
    pub selected: Option<(PaletteHostId, WindowId)>,
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
    pub selected: bool,
    pub hovered: bool,
    pub dimmed: bool,
}

impl FlatRowState {
    fn paint_state(self) -> PaintState {
        if self.current {
            PaintState::Current
        } else if self.selected {
            PaintState::Selected
        } else if self.hovered {
            PaintState::Hover
        } else {
            PaintState::Rest
        }
    }

    fn for_pointer(self, cx: &App) -> Self {
        Self {
            hovered: self.hovered && !CoarsePointer::get(cx),
            ..self
        }
    }
}

#[derive(Clone, Copy)]
struct RowInk {
    ground: Ground,
    text: Hsla,
    muted: Hsla,
    plain: bool,
    dimmed: bool,
}

impl RowInk {
    fn new(state: FlatRowState, cx: &App) -> Self {
        let paint = cx.theme().paint(Tone::Neutral, Emphasis::Ghost);
        let paint_state = state.paint_state();
        let text = paint.text_at(paint_state);
        Self {
            ground: paint.ground_at(paint_state),
            text,
            muted: paint.muted_at(paint_state),
            plain: text != paint.text,
            dimmed: state.dimmed,
        }
    }

    fn dim(self, color: Hsla) -> Hsla {
        if self.dimmed {
            color.mix_oklab(self.ground.fill, DIMMED)
        } else {
            color
        }
    }

    fn tone(self, tone: DetailTone, cx: &App) -> Hsla {
        self.dim(match tone {
            DetailTone::Normal => self.text,
            DetailTone::Muted => self.muted,
            DetailTone::Warning | DetailTone::Danger if self.plain => self.text,
            DetailTone::Warning => cx.theme().warning,
            DetailTone::Danger => cx.theme().danger,
        })
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

fn attention_word(attention: Attention) -> Option<&'static str> {
    match attention {
        Attention::NeedsYou => Some("needs you"),
        Attention::Failed => Some("failed"),
        Attention::Done => Some("unseen"),
        Attention::Bell => Some("bell"),
        Attention::Working => Some("working"),
        Attention::None => None,
    }
}

pub fn flat_row_ground(state: FlatRowState, cx: &App) -> Ground {
    RowInk::new(state, cx).ground
}

fn header_inset(cx: &App) -> Pixels {
    cx.theme().row_inset + px(ROW_PADDING)
}

fn row_height(cx: &App) -> Pixels {
    cx.theme().dense(px(ROW_HEIGHT))
}

fn tile_stack(
    host: PaletteHostId,
    window: WindowId,
    tiles: &[PaneTile],
    ink: RowInk,
    handler: Option<&FlatHandler>,
    cx: &App,
) -> Div {
    let size = StackSize::Row;
    let tiles = tiles
        .iter()
        .map(|tile| StackTile {
            icon: pane_glyph(tile.tag),
            favicon: None,
            label: tile.label.clone().into(),
            tooltip: tile.label.clone().into(),
            select: handler.map(|handler| {
                let handler = handler.clone();
                let pane = tile.pane;
                Rc::new(move |window_handle: &mut Window, cx: &mut App| {
                    handler(FlatEvent::OpenPane(host, window, pane), window_handle, cx);
                }) as StackAction
            }),
        })
        .collect();
    div().flex_none().w(px(size.widest())).child(
        PaneStack::new(
            ElementId::NamedInteger(format!("flat-stack-{}", host.0).into(), window.0),
            tiles,
            size,
            ink.ground,
            stack_radius(px(size.tile()), row_height(cx), cx),
        )
        .dimmed(if ink.dimmed { DIMMED } else { 0.0 }),
    )
}

pub fn flat_tile_stack(
    host: PaletteHostId,
    window: WindowId,
    tiles: &[PaneTile],
    state: FlatRowState,
    cx: &App,
) -> Div {
    tile_stack(host, window, tiles, RowInk::new(state, cx), None, cx)
}

fn spinner(size: f32, line_font: f32, color: Hsla, cx: &App) -> Div {
    div()
        .flex_none()
        .relative()
        .top(cap_center_drop(px(line_font), px(size), cx))
        .child(Spinner::new().with_size(px(size)).color(color))
}

fn glyph(icon: IconName, size: f32, line_font: f32, color: Hsla, cx: &App) -> Div {
    div()
        .flex_none()
        .relative()
        .top(cap_center_drop(px(line_font), px(size), cx))
        .child(Icon::new(icon).size(px(size)).text_color(color))
}

fn baseline_drop(line_font: f32, own_font: f32, cx: &App) -> Pixels {
    let text = cx.text_system();
    let font = text.resolve_font(&font(cx.theme().font_family.clone()));
    let middle = |size: f32| (text.ascent(font, px(size)) + text.descent(font, px(size))) / 2.0;
    middle(line_font) - middle(own_font)
}

fn detail_text(window: &WindowSummary, ink: RowInk, cx: &App) -> StyledText {
    let mut text = String::new();
    let mut highlights = Vec::with_capacity(window.detail.len());
    for part in &window.detail {
        let start = text.len();
        text.push_str(&part.text);
        highlights.push((
            start..text.len(),
            HighlightStyle {
                color: Some(ink.tone(part.tone, cx)),
                ..HighlightStyle::default()
            },
        ));
    }
    StyledText::new(text).with_highlights(highlights)
}

fn row_description(window: &FlatWindow) -> String {
    let detail = window
        .summary
        .detail
        .iter()
        .map(|part| part.text.as_str())
        .collect::<String>();
    [
        Some(detail.as_str()),
        attention_word(window.summary.attention),
        window.age.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(", ")
}

fn icon_button(id: impl Into<ElementId>, icon: IconName, icon_size: f32) -> Button {
    Button::new(id)
        .child(Icon::new(icon).size(px(icon_size)))
        .px_0()
}

fn actions_menu(
    host: PaletteHostId,
    window: WindowId,
    title: &SharedString,
    ground: Ground,
    visible: bool,
    menu: &FlatMenu,
) -> impl IntoElement {
    let menu = menu.clone();
    icon_button(
        ("flat-window-actions", window.0),
        IconName::Ellipsis,
        ACTIONS_ICON,
    )
    .w(px(ACTIONS_WIDTH))
    .h(px(ACTIONS_HEIGHT))
    .text()
    .flat()
    .text_color(ground.ink)
    .hit_slop(
        (TOUCH_TARGET - ACTIONS_WIDTH) / 2.0,
        (TOUCH_TARGET - ACTIONS_HEIGHT) / 2.0,
    )
    .tooltip(format!("Actions for {title}"))
    .when(!visible, Styled::invisible)
    .dropdown_menu_with_anchor(Anchor::TopRight, move |popup, window_handle, cx| {
        menu(host, window, popup, window_handle, cx)
    })
}

pub fn flat_window_row(
    host: PaletteHostId,
    window: &FlatWindow,
    state: FlatRowState,
    handler: &FlatHandler,
    menu: &FlatMenu,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let coarse = CoarsePointer::get(cx);
    let state = state.for_pointer(cx);
    let ink = RowInk::new(state, cx);
    let summary = &window.summary;
    let id = summary.window;
    let title = SharedString::from(summary.title.clone());
    let target = FlatTarget::Window(host, id);
    let working = summary.attention == Attention::Working && !state.dimmed;
    let busy =
        (working && !state.hovered).then(|| spinner(SPINNER_SIZE, TITLE_SIZE, ink.muted, cx));
    let actions = div()
        .relative()
        .flex_none()
        .w(px(ACTIONS_WIDTH))
        .h(px(TITLE_LINE))
        .flex()
        .items_center()
        .justify_center()
        .children(busy)
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(actions_menu(
                    host,
                    id,
                    &title,
                    ink.ground,
                    state.hovered || coarse,
                    menu,
                )),
        );
    let context = menu.clone();
    let row_id = ElementId::NamedInteger(format!("flat-window-{}", host.0).into(), id.0);
    let fill = workspace_row_fill(&row_id, state.paint_state(), None, cx);
    let hover = handler.clone();
    let click = handler.clone();
    div()
        .id(row_id)
        .role(Role::Button)
        .aria_label(title.clone())
        .aria_description(row_description(window))
        .aria_selected(state.current)
        .when(
            state.selected,
            StatefulInteractiveElement::aria_active_descendant,
        )
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(TILE_GAP))
        .h(row_height(cx))
        .px(theme.row_inset + px(ROW_PADDING))
        .text_color(ink.dim(ink.text))
        .cursor_pointer()
        .on_hover(move |hovered, window, cx| hover(FlatEvent::Hover(target, *hovered), window, cx))
        .on_click(move |_, window, cx| click(FlatEvent::Open(host, id), window, cx))
        .children(fill)
        .child(tile_stack(
            host,
            id,
            &summary.tiles,
            ink,
            (!state.dimmed).then_some(handler),
            cx,
        ))
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
                                .child(title),
                        )
                        .child(actions),
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
                                .text_color(ink.dim(ink.muted))
                                .child(detail_text(summary, ink, cx)),
                        )
                        .children(window.age.clone().map(|age| {
                            div()
                                .flex_none()
                                .relative()
                                .top(baseline_drop(DETAIL_SIZE, AGE_SIZE, cx))
                                .text_size(px(AGE_SIZE))
                                .line_height(px(DETAIL_LINE))
                                .font_features(TABULAR.clone())
                                .text_color(ink.dim(ink.muted))
                                .child(age)
                        })),
                ),
        )
        .context_menu(move |popup, window_handle, cx| context(host, id, popup, window_handle, cx))
        .into_any_element()
}

pub fn flat_rollup(rollup: &AttentionRollup, live: bool, cx: &App) -> Div {
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
        .when(live && rollup.working > 0, |this| {
            this.child(
                h_flex()
                    .gap(px(ROLLUP_ITEM_GAP))
                    .child(spinner(ROLLUP_SPINNER, NOTE_SIZE, muted, cx))
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

struct HostNote {
    text: SharedString,
    color: Hsla,
    reason: Option<SharedString>,
    action: Option<(&'static str, FlatEvent)>,
}

fn host_note(host: &FlatHost, cx: &App) -> Option<HostNote> {
    let theme = cx.theme();
    let note =
        |text: SharedString, color: Hsla, action: Option<(&'static str, FlatEvent)>| HostNote {
            text,
            color,
            reason: None,
            action,
        };
    let muted = theme.foreground.muted();
    let connect = Some(("Connect", FlatEvent::Connect(host.id)));
    Some(match &host.connection {
        HostConnection::Connected => return None,
        HostConnection::Connecting => note("Connecting…".into(), muted, None),
        HostConnection::Reconnecting => note("Reconnecting…".into(), muted, None),
        HostConnection::Disconnected => note("Disconnected".into(), muted, connect),
        HostConnection::Unreachable(reason) => HostNote {
            reason: Some(reason.clone()).filter(|reason| !reason.is_empty()),
            ..note(
                "Unreachable".into(),
                theme.danger,
                Some(("Retry", FlatEvent::Connect(host.id))),
            )
        },
        HostConnection::Incompatible(version) => note(
            version.clone(),
            theme.warning,
            Some(("Upgrade", FlatEvent::Upgrade(host.id))),
        ),
    })
}

pub fn flat_host_header(
    host: &FlatHost,
    hovered: bool,
    handler: &FlatHandler,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let hovered = hovered && !CoarsePointer::get(cx);
    let muted = theme.foreground.muted();
    let id = host.id;
    let target = FlatTarget::Host(id);
    let note = host_note(host, cx);
    let action = note.as_ref().and_then(|note| note.action);
    let rollup =
        (host.folded && action.is_none()).then(|| flat_rollup(&host.rollup, host.live(), cx));
    let ssh = (host.remote && host.live() && !host.folded).then(|| header_note("ssh", muted));
    let description = note.as_ref().map(|note| note.text.clone());
    let note = note.map(|note| {
        let text = header_note(note.text, note.color);
        match note.reason {
            Some(reason) => div()
                .id(("flat-host-reason", id.0))
                .flex_none()
                .tooltip(move |window, cx| Tooltip::new(reason.clone()).build(window, cx))
                .child(text)
                .into_any_element(),
            None => text.into_any_element(),
        }
    });
    let hover = handler.clone();
    let click = handler.clone();
    h_flex()
        .id(("flat-host", id.0))
        .role(Role::Button)
        .aria_label(host.name.clone())
        .aria_expanded(!host.folded)
        .when_some(description, |this, description| {
            this.aria_description(description)
        })
        .flex_none()
        .h(theme.dense(px(HOST_HEIGHT)))
        .px(header_inset(cx))
        .gap(px(HOST_SPACING))
        .text_color(theme.foreground)
        .text_size(px(HOST_NAME_SIZE))
        .line_height(px(HOST_LINE))
        .font_semibold()
        .cursor_pointer()
        .on_hover(move |hovered, window, cx| hover(FlatEvent::Hover(target, *hovered), window, cx))
        .on_click(move |_, window, cx| click(FlatEvent::FoldHost(id), window, cx))
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
        .children(note)
        .children(
            action.map(|(label, event)| {
                header_button(("flat-host-action", id.0), label, event, handler)
            }),
        )
}

pub fn flat_session_header(
    host: &FlatHost,
    session: &FlatSession,
    holds_current: bool,
    hovered: bool,
    handler: &FlatHandler,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let coarse = CoarsePointer::get(cx);
    let hovered = hovered && !coarse;
    let host_id = host.id;
    let session_id = session.id;
    let target = FlatTarget::Session(host_id, session_id);
    let rollup = session
        .folded
        .then(|| flat_rollup(&session.rollup, host.live(), cx));
    let new_window = (hovered || coarse).then(|| {
        let handler = handler.clone();
        icon_button(
            ("flat-new-window", session_id.0),
            IconName::Plus,
            NEW_WINDOW_ICON,
        )
        .text()
        .flat()
        .size(px(NEW_WINDOW_SIZE))
        .hit_slop(
            (TOUCH_TARGET - NEW_WINDOW_SIZE) / 2.0,
            (TOUCH_TARGET - NEW_WINDOW_SIZE) / 2.0,
        )
        .tooltip("New window")
        .on_click(move |_: &ClickEvent, window, cx| {
            cx.stop_propagation();
            handler(FlatEvent::NewWindow(host_id, session_id), window, cx);
        })
    });
    let hover = handler.clone();
    let click = handler.clone();
    div()
        .id(ElementId::NamedInteger(
            format!("flat-session-{}", host_id.0).into(),
            session_id.0,
        ))
        .role(Role::Button)
        .aria_label(session.label.clone())
        .aria_expanded(!session.folded)
        .flex_none()
        .pt(px(SESSION_TOP))
        .pb(px(SESSION_BOTTOM))
        .px(header_inset(cx))
        .cursor_pointer()
        .on_hover(move |hovered, window, cx| hover(FlatEvent::Hover(target, *hovered), window, cx))
        .on_click(move |_, window, cx| {
            click(FlatEvent::FoldSession(host_id, session_id), window, cx);
        })
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
                .children(session.index.clone().map(|index| {
                    div()
                        .flex_none()
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme.foreground.faint())
                        .child(index)
                }))
                .children(chevron(session.folded, hovered, SESSION_SIZE, cx))
                .child(div().flex_1())
                .children(rollup)
                .children(new_window),
        )
}

pub fn flat_sidebar(
    hosts: &[FlatHost],
    state: FlatState,
    handler: &FlatHandler,
    menu: &FlatMenu,
    cx: &App,
) -> Div {
    let mut list = v_flex().w_full().flex_none();
    for (index, host) in hosts.iter().enumerate() {
        list = list.child(
            flat_host_header(
                host,
                state.hovered == Some(FlatTarget::Host(host.id)),
                handler,
                cx,
            )
            .when(index > 0, |header| header.mt(px(HOST_GAP))),
        );
        if host.folded {
            continue;
        }
        let dimmed = !host.live();
        for session in &host.sessions {
            let holds_current = state.current.is_some_and(|(current_host, window)| {
                current_host == host.id
                    && session
                        .windows
                        .iter()
                        .any(|candidate| candidate.summary.window == window)
            });
            list = list.child(flat_session_header(
                host,
                session,
                holds_current,
                state.hovered == Some(FlatTarget::Session(host.id, session.id)),
                handler,
                cx,
            ));
            if session.folded {
                continue;
            }
            list = list.children(session.windows.iter().map(|window| {
                let key = Some((host.id, window.summary.window));
                flat_window_row(
                    host.id,
                    window,
                    FlatRowState {
                        current: state.current == key,
                        selected: state.selected == key,
                        hovered: state.hovered
                            == Some(FlatTarget::Window(host.id, window.summary.window)),
                        dimmed,
                    },
                    handler,
                    menu,
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
        let host = PaletteHostId(0);
        let first = FlatTarget::Window(host, WindowId(1));
        let second = FlatTarget::Window(host, WindowId(2));
        let mut state = FlatState::default();
        assert!(state.hover(first, true));
        assert!(state.hover(second, true));
        assert!(!state.hover(first, false));
        assert_eq!(state.hovered, Some(second));
        assert!(state.hover(second, false));
        assert_eq!(state.hovered, None);
    }
}
