use std::{
    rc::Rc,
    sync::{Arc, LazyLock},
};

use ala::{
    AnyElement, App, ClickEvent, Context, Div, ElementId, FULL_CORNER_RADIUS, FontFeatures,
    FontWeight, HighlightStyle, Hsla, IntoElement, Keystroke, ParentElement as _, Pixels, Role,
    SharedString, Stateful, StyledText, Window, div, font, prelude::*, px,
};
use zz_client::window_summary::{
    Attention, AttentionRollup, PaneTag, PaneTile, Tone as DetailTone, WindowSummary,
};
use zz_protocol::{PaneId, WindowId};

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Ground, Highlight, Icon, IconName, PaintState,
    Shape, Sizable as _, StyledExt as _, Surface, Tone,
    button::Button,
    command::PaletteHostId,
    h_flex,
    icon::cap_center_drop,
    menu::{ContextMenuExt as _, PopupMenu},
    motion::{self, Disclosure},
    navigation::{
        pane_stack::{PaneStack, StackAction, StackSize, StackTile},
        workspace_row_action, workspace_row_fill,
    },
    roles,
    spinner::Spinner,
    tooltip::Tooltip,
    touch::{CoarsePointer, TOUCH_TARGET},
    v_flex,
};

const ROW_HEIGHT: f32 = 48.0;
const ROW_RADIUS: f32 = 24.0;
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
const DIMMED: f32 = 0.45;
const HOST_HEIGHT: f32 = 30.0;
const HOST_GAP: f32 = 18.0;
const HOST_GLYPH: f32 = 16.0;
const HOST_NAME_SIZE: f32 = 13.0;
const HOST_LINE: f32 = 16.0;
const HOST_SPACING: f32 = 8.0;
const ADD_BUTTON_SIZE: f32 = 18.0;
const ADD_BUTTON_ICON: f32 = 14.0;
const CHEVRON_SIZE: f32 = 10.0;
const NOTE_SIZE: f32 = 11.0;
const NOTE_LINE: f32 = 15.0;
const HEADER_BUTTON_HEIGHT: f32 = 20.0;
const STATUS_ICON: f32 = 14.0;
const ROLLUP_DOT: f32 = super::STATUS_DOT_SIZE;
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
    pub shortcut: Option<Keystroke>,
}

impl FlatWindow {
    pub fn new(summary: WindowSummary, age: impl Fn(u64) -> Option<String>) -> Self {
        Self {
            age: age(summary.changed_at).map(Into::into),
            summary,
            shortcut: None,
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
    pub chats: Vec<FlatWindow>,
}

impl FlatHost {
    pub fn new(
        id: PaletteHostId,
        name: impl Into<SharedString>,
        remote: bool,
        connection: HostConnection,
        (chats, rollup): (Vec<WindowSummary>, AttentionRollup),
        age: impl Fn(u64) -> Option<String>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            remote,
            connection,
            folded: false,
            rollup,
            chats: chats
                .into_iter()
                .map(|chat| FlatWindow::new(chat, &age))
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
    Window(PaletteHostId, WindowId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatEvent {
    Open(PaletteHostId, WindowId),
    OpenPane(PaletteHostId, WindowId, PaneId),
    FoldHost(PaletteHostId),
    NewChat(PaletteHostId),
    Connect(PaletteHostId),
    Upgrade(PaletteHostId),
    Hover(FlatTarget, bool),
}

pub type FlatHandler = Rc<dyn Fn(FlatEvent, &mut Window, &mut App)>;

/// Fills a window row's menu, opened by a right click or a long press.
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlatRowState {
    pub current: bool,
    pub selected: bool,
    pub hovered: bool,
    pub dimmed: bool,
    /// What the row sits on; the window when unset.
    pub ground: Option<Ground>,
    /// The most tiles any row in its list shows, so their labels line up;
    /// 0 holds room for the row's own.
    pub stack_tiles: usize,
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

    fn surface(self) -> (Surface, PaintState) {
        let (role, state) = match self.paint_state() {
            PaintState::Selected => (roles::SIDEBAR_CURSOR, PaintState::Selected),
            state => (roles::SIDEBAR_ENTRY, state),
        };
        (Surface::new(role).on_maybe(self.ground), state)
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
        let theme = cx.theme();
        let (surface, paint_state) = state.surface();
        let layer = theme.layer(surface, paint_state);
        let lit = matches!(
            paint_state,
            PaintState::Selected | PaintState::Current | PaintState::Focused
        );
        Self {
            ground: layer.ground,
            text: layer.text,
            muted: theme
                .paint_on(layer.ground, Tone::Neutral, Emphasis::Ghost)
                .muted_at(PaintState::Rest),
            plain: lit && theme.style(surface.role).selected == Highlight::Accent,
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

fn row_height() -> Pixels {
    px(ROW_HEIGHT)
}

/// Where a row's trailing text ends: the spinner centers on the column the
/// headers' + button and the sidebar's chrome buttons do.
fn row_trailing_inset(cx: &App) -> Pixels {
    cx.theme().row_inset + super::WORKSPACE_ROW_ACTION_CENTER - px(SPINNER_SIZE / 2.0)
}

/// A tile's corners: concentric with the row's when its role is round, a
/// circle when it is a pill.
fn tile_radius(size: StackSize, cx: &App) -> Pixels {
    match cx.theme().style(roles::PANE_STACK).shape {
        Shape::Square => Pixels::ZERO,
        Shape::Pill => FULL_CORNER_RADIUS,
        Shape::Round => px(ROW_RADIUS) - (row_height() - px(size.tile())) / 2.0,
    }
}

fn tile_stack(
    host: PaletteHostId,
    window: WindowId,
    tiles: &[PaneTile],
    ink: RowInk,
    handler: Option<&FlatHandler>,
    cx: &App,
) -> PaneStack {
    let size = StackSize::Row;
    let tiles = tiles
        .iter()
        .map(|tile| StackTile {
            icon: pane_glyph(tile.tag),
            favicon: None,
            program: tile.program,
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
    PaneStack::new(
        ElementId::NamedInteger(format!("flat-stack-{}", host.0).into(), window.0),
        tiles,
        size,
        ink.ground,
        tile_radius(size, cx),
    )
    .dimmed(if ink.dimmed { DIMMED } else { 0.0 })
}

pub fn flat_tile_stack(
    host: PaletteHostId,
    window: WindowId,
    tiles: &[PaneTile],
    state: FlatRowState,
    cx: &App,
) -> Div {
    div()
        .flex_none()
        .w(px(StackSize::Row.widest()))
        .child(tile_stack(
            host,
            window,
            tiles,
            RowInk::new(state, cx),
            None,
            cx,
        ))
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

pub fn flat_window_row(
    host: PaletteHostId,
    window: &FlatWindow,
    state: FlatRowState,
    handler: &FlatHandler,
    menu: &FlatMenu,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let state = state.for_pointer(cx);
    let ink = RowInk::new(state, cx);
    let summary = &window.summary;
    let id = summary.window;
    let title = SharedString::from(summary.title.clone());
    let target = FlatTarget::Window(host, id);
    let working = summary.attention == Attention::Working && !state.dimmed;
    let busy = working.then(|| spinner(SPINNER_SIZE, TITLE_SIZE, ink.muted, cx));
    let context = menu.clone();
    let row_id = ElementId::NamedInteger(format!("flat-window-{}", host.0).into(), id.0);
    let (surface, paint_state) = state.surface();
    let fill = workspace_row_fill(
        &row_id,
        surface.role,
        paint_state,
        None,
        surface.ground.unwrap_or_else(|| theme.ground()),
        theme
            .styles
            .get(surface.role.id)
            .and_then(|patch| patch.shape)
            .is_none()
            .then_some(px(ROW_RADIUS)),
        cx,
    );
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
        .h(row_height())
        .pl(theme.row_inset + px(ROW_PADDING))
        .pr(row_trailing_inset(cx))
        .text_color(ink.dim(ink.text))
        .cursor_pointer()
        .on_hover(move |hovered, window, cx| hover(FlatEvent::Hover(target, *hovered), window, cx))
        .on_click(move |_, window, cx| click(FlatEvent::Open(host, id), window, cx))
        .children(fill)
        .child(
            tile_stack(
                host,
                id,
                &summary.tiles,
                ink,
                (!state.dimmed).then_some(handler),
                cx,
            )
            .room(state.stack_tiles)
            .beside(
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
                                    .text_fade(crate::LABEL_FADE)
                                    .text_size(px(TITLE_SIZE))
                                    .line_height(px(TITLE_LINE))
                                    .font_medium()
                                    .child(title),
                            )
                            .children(busy),
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
                                    .text_fade(crate::LABEL_FADE)
                                    .text_size(px(DETAIL_SIZE))
                                    .line_height(px(DETAIL_LINE))
                                    .text_color(ink.dim(ink.muted))
                                    .child(detail_text(summary, ink, cx)),
                            )
                            .children(
                                window
                                    .age
                                    .clone()
                                    .filter(|_| window.shortcut.is_none())
                                    .map(|age| {
                                        div()
                                            .flex_none()
                                            .relative()
                                            .top(baseline_drop(DETAIL_SIZE, AGE_SIZE, cx))
                                            .text_size(px(AGE_SIZE))
                                            .line_height(px(DETAIL_LINE))
                                            .font_features(TABULAR.clone())
                                            .text_color(ink.dim(ink.muted))
                                            .child(age)
                                    }),
                            ),
                    ),
                px(TILE_GAP),
            ),
        )
        .children(window.shortcut.clone().map(|stroke| {
            super::window_shortcut_cap(stroke, surface.ground.unwrap_or_else(|| theme.ground()))
                .flex_none()
        }))
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
        (rollup.done, theme.success),
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
                        .child(super::status_dot(color).relative().top(cap_center_drop(
                            px(NOTE_SIZE),
                            px(ROLLUP_DOT),
                            cx,
                        )))
                        .child(count(value))
                }),
        )
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
        .surface_role(roles::HOST_ACTION)
        .on_click(move |_: &ClickEvent, window, cx| {
            cx.stop_propagation();
            handler(event, window, cx);
        })
}

fn chevron(
    id: ElementId,
    folded: bool,
    hovered: bool,
    line_font: f32,
    cx: &App,
) -> Option<impl IntoElement> {
    (folded || hovered).then(|| {
        motion::appear(
            ElementId::from((id.clone(), "appear")),
            div()
                .flex_none()
                .relative()
                .top(cap_center_drop(px(line_font), px(CHEVRON_SIZE), cx))
                .child(
                    Disclosure::new(id, !folded, px(CHEVRON_SIZE))
                        .color(cx.theme().foreground.muted()),
                ),
        )
    })
}

struct HostNote {
    icon: Option<IconName>,
    color: Hsla,
    text: SharedString,
    action: Option<(&'static str, FlatEvent)>,
}

fn host_note(host: &FlatHost, cx: &App) -> Option<HostNote> {
    let theme = cx.theme();
    let note = |icon: Option<IconName>,
                color: Hsla,
                text: SharedString,
                action: Option<(&'static str, FlatEvent)>| HostNote {
        icon,
        color,
        text,
        action,
    };
    let muted = theme.foreground.muted();
    let connect = Some(("Connect", FlatEvent::Connect(host.id)));
    Some(match &host.connection {
        HostConnection::Connected => return None,
        HostConnection::Connecting => note(None, muted, "Connecting…".into(), None),
        HostConnection::Reconnecting => note(None, muted, "Reconnecting…".into(), None),
        HostConnection::Disconnected => note(
            Some(IconName::CircleX),
            muted,
            "Disconnected".into(),
            connect,
        ),
        HostConnection::Unreachable(reason) => note(
            Some(IconName::CircleX),
            theme.danger,
            if reason.is_empty() {
                "Unreachable".into()
            } else {
                format!("Unreachable: {reason}").into()
            },
            Some(("Retry", FlatEvent::Connect(host.id))),
        ),
        HostConnection::Incompatible(version) => note(
            Some(IconName::TriangleAlert),
            theme.warning,
            format!("Speaks {version}").into(),
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
    let coarse = CoarsePointer::get(cx);
    let hovered = hovered && !coarse;
    let muted = theme.foreground.muted();
    let id = host.id;
    let target = FlatTarget::Host(id);
    let note = host_note(host, cx);
    let action = note.as_ref().and_then(|note| note.action);
    let rollup =
        (host.folded && action.is_none()).then(|| flat_rollup(&host.rollup, host.live(), cx));
    let description = note.as_ref().map(|note| note.text.clone());
    let note = note.map(|note| {
        let text = note.text;
        div()
            .id(("flat-host-status", id.0))
            .flex_none()
            .tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
            .child(match note.icon {
                Some(icon) => glyph(icon, STATUS_ICON, HOST_NAME_SIZE, note.color, cx),
                None => spinner(STATUS_ICON, HOST_NAME_SIZE, note.color, cx),
            })
    });
    let new_chat = (host.live() && (hovered || coarse)).then(|| {
        add_button(
            ElementId::from(("flat-new-chat", id.0)),
            IconName::ChatPlus,
            "New agent",
            FlatEvent::NewChat(id),
            handler,
            cx,
        )
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
        .h(px(HOST_HEIGHT))
        .px(header_inset(cx))
        .gap(px(HOST_SPACING))
        .text_color(if hovered {
            theme.foreground
        } else {
            theme.foreground.rest()
        })
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
                .text_fade(crate::LABEL_FADE)
                .child(host.name.clone()),
        )
        .children(chevron(
            ElementId::from(("flat-host-fold", id.0)),
            host.folded,
            hovered,
            HOST_NAME_SIZE,
            cx,
        ))
        .child(div().flex_1())
        .children(rollup)
        .children(note)
        .children(
            action.map(|(label, event)| {
                header_button(("flat-host-action", id.0), label, event, handler)
            }),
        )
        .children(new_chat)
}

fn add_button(
    id: ElementId,
    icon: IconName,
    tooltip: &'static str,
    event: FlatEvent,
    handler: &FlatHandler,
    cx: &App,
) -> impl IntoElement + use<> {
    let handler = handler.clone();
    let button = workspace_row_action(
        icon_button(id.clone(), icon, ADD_BUTTON_ICON),
        cx.theme().ground(),
        false,
        cx,
    )
    .size(px(ADD_BUTTON_SIZE))
    .debug_selector(|| "flat-new-chat".into())
    .hit_slop(
        (TOUCH_TARGET - ADD_BUTTON_SIZE) / 2.0,
        (TOUCH_TARGET - ADD_BUTTON_SIZE) / 2.0,
    )
    .tooltip(tooltip)
    .on_click(move |_: &ClickEvent, window, cx| {
        cx.stop_propagation();
        handler(event, window, cx);
    });
    motion::appear(ElementId::from((id, "appear")), button)
}

pub fn flat_sidebar(
    hosts: &[FlatHost],
    state: FlatState,
    handler: &FlatHandler,
    menu: &FlatMenu,
    ground: Ground,
    cx: &App,
) -> Div {
    let stack_tiles = hosts
        .iter()
        .filter(|host| !host.folded)
        .flat_map(|host| &host.chats)
        .map(|chat| chat.summary.tiles.len())
        .max()
        .unwrap_or_default();
    let mut list = v_flex()
        .w_full()
        .flex_none()
        .pr(super::WORKSPACE_SIDEBAR_SCROLLBAR_GUTTER);
    for (index, host) in hosts.iter().enumerate() {
        list = list.child(motion::enter(
            ("flat-host-enter", host.id.0),
            flat_host_header(
                host,
                state.hovered == Some(FlatTarget::Host(host.id)),
                handler,
                cx,
            )
            .when(index > 0, |header| header.mt(px(HOST_GAP))),
        ));
        if host.folded {
            continue;
        }
        let dimmed = !host.live();
        list = list.children(host.chats.iter().map(|chat| {
            let key = Some((host.id, chat.summary.window));
            let row = flat_window_row(
                host.id,
                chat,
                FlatRowState {
                    current: state.current == key,
                    selected: state.selected == key,
                    hovered: state.hovered
                        == Some(FlatTarget::Window(host.id, chat.summary.window)),
                    dimmed,
                    ground: Some(ground),
                    stack_tiles,
                },
                handler,
                menu,
                cx,
            );
            motion::enter(
                ElementId::NamedInteger(
                    format!("flat-window-enter-{}", host.id.0).into(),
                    chat.summary.window.0,
                ),
                row,
            )
        }));
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation::{
        WORKSPACE_CHROME_BUTTON_SIZE, WORKSPACE_SIDEBAR_DEFAULT_WIDTH,
        WORKSPACE_SIDEBAR_SCROLLBAR_GUTTER, workspace_sidebar_footer, workspace_sidebar_titlebar,
    };

    struct Sidebar;

    impl ala::Render for Sidebar {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let chrome = |id: &'static str| {
                div()
                    .debug_selector(move || id.to_owned())
                    .size(WORKSPACE_CHROME_BUTTON_SIZE)
            };
            let host = PaletteHostId(0);
            let hosts = [FlatHost {
                id: host,
                name: "macbook".into(),
                remote: false,
                connection: HostConnection::Connected,
                folded: false,
                rollup: AttentionRollup::default(),
                chats: Vec::new(),
            }];
            let state = FlatState {
                hovered: Some(FlatTarget::Host(host)),
                ..FlatState::default()
            };
            let handler: FlatHandler = Rc::new(|_, _, _| {});
            let menu: FlatMenu = Rc::new(|_, _, menu, _, _| menu);
            v_flex()
                .w(px(WORKSPACE_SIDEBAR_DEFAULT_WIDTH))
                .child(workspace_sidebar_titlebar("titlebar", chrome("toggle"), cx))
                .child(
                    div()
                        .debug_selector(|| "rows".to_owned())
                        .child(flat_sidebar(
                            &hosts,
                            state,
                            &handler,
                            &menu,
                            cx.theme().ground(),
                            cx,
                        )),
                )
                .child(workspace_sidebar_footer(
                    "footer",
                    div(),
                    Some(chrome("eye").into_any_element()),
                    cx,
                ))
        }
    }

    #[ala::test]
    fn trailing_buttons_share_the_row_actions_column_beside_the_scrollbar(
        cx: &mut ala::TestAppContext,
    ) {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|_, _| Sidebar);
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let new_chat = cx.debug_bounds("flat-new-chat").unwrap();
        let toggle = cx.debug_bounds("toggle").unwrap();
        let eye = cx.debug_bounds("eye").unwrap();
        assert_eq!(toggle.center().x, new_chat.center().x);
        assert_eq!(eye.center().x, new_chat.center().x);
        let rows = cx.debug_bounds("rows").unwrap();
        let inset = cx.update(|_, cx| header_inset(cx));
        assert_eq!(
            new_chat.right(),
            rows.right() - WORKSPACE_SIDEBAR_SCROLLBAR_GUTTER - inset
        );
    }

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
