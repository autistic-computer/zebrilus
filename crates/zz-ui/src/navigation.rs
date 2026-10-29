pub mod flat;
pub mod pane_stack;
pub mod sidebar;
pub mod status;
pub mod tree;

use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, Ground, Icon, IconName,
    InteractiveSurfaceExt as _, Layer, PaintState, Shape, Sizable as _, StyledExt as _, Surface,
    SurfaceRole, Tone,
    button::{Button, ButtonVariants as _, COMPACT_ICON_BUTTON_SIZE},
    kbd::Kbd,
    rems_from_px, roles,
    sidebar::{
        CHROME_BUTTON_SIZE, SIDEBAR_CONTENT_INSET, SIDEBAR_ROW_ACTION_CENTER, chrome_button,
        sidebar_panel, status_dot,
    },
    tooltip::Tooltip,
    touch::{CoarsePointer, TOUCH_TARGET},
};
use ala::{
    AnyElement, App, ElementId, Hsla, IntoElement, Keystroke, MouseButton, MouseDownEvent,
    ParentElement as _, Pixels, SharedString, Stateful, StatefulInteractiveElement as _,
    Styled as _, Window, div, prelude::*, px,
};

pub const WORKSPACE_TREE_ROW_HEIGHT: f32 = 32.0;
pub const WORKSPACE_TREE_INDENT_WIDTH: f32 = 20.0;
pub const WORKSPACE_TREE_MARKER_SLOT_WIDTH: f32 = 18.0;
pub const WORKSPACE_TREE_NODE_ICON_SIZE: f32 = 14.0;
pub const WORKSPACE_TREE_MARKER_LABEL_GAP: f32 = 6.0;
const TREE_TOUCH_SLOP: f32 = (TOUCH_TARGET - COMPACT_ICON_BUTTON_SIZE) / 2.0;
const WORKSPACE_TREE_FILL_VERTICAL_INSET: f32 = 1.0;
const WORKSPACE_CHROME_CONTROL_GAP: f32 = 4.0;
const WORKSPACE_ROW_ACTION_GAP: f32 = 10.0;
pub const WORKSPACE_SIDEBAR_FADE_HEIGHT: Pixels = px(40.0);
pub const WORKSPACE_STATUS_PILL_HEIGHT: Pixels = px(32.0);
const WORKSPACE_STATUS_WINDOW_TEXT_SIZE: f32 = 14.0;
const WORKSPACE_STATUS_LINE_HEIGHT: Pixels = px(18.0);
const WORKSPACE_STATUS_ITEM_MAX_WIDTH: Pixels = px(180.0);
const WORKSPACE_STATUS_WINDOW_DEFAULT_WIDTH: Pixels = px(240.0);
const WORKSPACE_STATUS_WINDOW_MIN_WIDTH: Pixels = px(36.0);
const WORKSPACE_STATUS_ICON_SIZE: Pixels = px(14.0);
const WORKSPACE_STATUS_TEXT_SIZE: f32 = 13.0;
const WORKSPACE_STATUS_CHIP_PADDING: Pixels = px(6.0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceStatusWindowState {
    pub connected: bool,
    pub active: bool,
    pub bell: bool,
    pub activity: bool,
}

impl WorkspaceStatusWindowState {
    #[must_use]
    pub const fn paint_state(self) -> PaintState {
        if self.active {
            PaintState::Current
        } else {
            PaintState::Rest
        }
    }
}

#[must_use]
pub fn workspace_layout_button(id: impl Into<ElementId>) -> Button {
    chrome_button(id, IconName::PanelLeft, "Toggle sidebar")
}

#[must_use]
pub fn workspace_palette_button(id: impl Into<ElementId>) -> Button {
    chrome_button(id, IconName::Search, "Command palette")
}

#[must_use]
pub fn workspace_settings_button(id: impl Into<ElementId>) -> Button {
    chrome_button(id, IconName::Settings, "Settings")
}

#[must_use]
pub fn workspace_add_host_button(id: impl Into<ElementId>) -> Button {
    chrome_button(id, IconName::ServerPlus, "Add host")
}

#[must_use]
pub fn workspace_sidebar_view_button(id: impl Into<ElementId>, flat: bool) -> Button {
    if flat {
        chrome_button(id, IconName::List, "Show the session tree")
    } else {
        chrome_button(id, IconName::ListTree, "Show chats")
    }
}

#[must_use]
pub fn workspace_sidebar_footer_controls(
    add_host: impl IntoElement,
    view: impl IntoElement,
) -> ala::Div {
    div()
        .flex()
        .items_center()
        .gap(rems_from_px(WORKSPACE_CHROME_CONTROL_GAP))
        .child(add_host)
        .child(view)
}

#[must_use]
pub fn workspace_chrome_controls_width_for(count: usize, window: &Window) -> Pixels {
    let count = f32::from(u16::try_from(count).unwrap_or(u16::MAX));
    CHROME_BUTTON_SIZE * count
        + rems_from_px(WORKSPACE_CHROME_CONTROL_GAP).to_pixels(window.rem_size())
            * (count - 1.0).max(0.0)
}

#[must_use]
pub fn workspace_status_item(
    id: impl Into<ElementId>,
    icon: Option<IconName>,
    text: SharedString,
    cx: &App,
) -> Stateful<ala::Div> {
    div()
        .id(id)
        .flex()
        .flex_shrink_1()
        .min_w_0()
        .max_w(WORKSPACE_STATUS_ITEM_MAX_WIDTH)
        .h(WORKSPACE_STATUS_PILL_HEIGHT)
        .items_center()
        .gap(px(5.0))
        .px(WORKSPACE_STATUS_CHIP_PADDING)
        .overflow_hidden()
        .surface(Surface::new(roles::STATUS_CHIP), cx)
        .text_color(cx.theme().foreground.muted())
        .text_size(rems_from_px(WORKSPACE_STATUS_TEXT_SIZE))
        .line_height(WORKSPACE_STATUS_LINE_HEIGHT)
        .children(icon.map(|icon| {
            div()
                .flex_none()
                .relative()
                .top(crate::icon::cap_center_drop(
                    px(WORKSPACE_STATUS_TEXT_SIZE),
                    WORKSPACE_STATUS_ICON_SIZE,
                    cx,
                ))
                .child(Icon::new(icon).size(WORKSPACE_STATUS_ICON_SIZE))
        }))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(text),
        )
}

#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn workspace_status_window(
    id: impl Into<ElementId>,
    name: SharedString,
    tooltip: SharedString,
    state: WorkspaceStatusWindowState,
    deck: Option<pane_stack::PaneStack>,
    cx: &App,
) -> Stateful<ala::Div> {
    let foreground = cx.theme().foreground;
    let tab = Surface::new(roles::WINDOW_TAB);
    let paint_state = state.paint_state();
    let name = div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_fade(crate::LABEL_FADE)
        .child(name);
    div()
        .id(id)
        .relative()
        .flex()
        .flex_shrink_1()
        .w(WORKSPACE_STATUS_WINDOW_DEFAULT_WIDTH)
        .min_w(WORKSPACE_STATUS_WINDOW_MIN_WIDTH)
        .max_w(WORKSPACE_STATUS_WINDOW_DEFAULT_WIDTH)
        .h(WORKSPACE_STATUS_PILL_HEIGHT)
        .items_center()
        .gap(px(5.0))
        .px(px(9.0))
        .overflow_hidden()
        .map(|item| {
            if state.connected {
                item.cursor_pointer().surface_hover(tab, paint_state, cx)
            } else {
                item.surface_at(tab, paint_state, cx)
            }
        })
        .when(!state.active, |item| item.text_color(foreground.muted()))
        .text_size(rems_from_px(WORKSPACE_STATUS_WINDOW_TEXT_SIZE))
        .line_height(WORKSPACE_STATUS_LINE_HEIGHT)
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .map(|item| match deck {
            Some(deck) => item.child(deck.beside(name, px(5.0))),
            None => item.child(name),
        })
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(3.0))
                .child(status_dot(cx.theme().warning).opacity(if state.bell { 1.0 } else { 0.0 }))
                .child(status_dot(cx.theme().success).opacity(if state.activity {
                    1.0
                } else {
                    0.0
                })),
        )
}

fn workspace_tree_trailing_inset(cx: &App) -> Pixels {
    cx.theme().row_inset + SIDEBAR_ROW_ACTION_CENTER - px(COMPACT_ICON_BUTTON_SIZE / 2.0)
}

fn workspace_row_action_gap() -> Pixels {
    px(WORKSPACE_ROW_ACTION_GAP)
}

#[must_use]
pub fn workspace_sidebar_surface(
    id: impl Into<ElementId>,
    width: f32,
    titlebar: impl IntoElement,
    navigation: impl IntoElement,
    cx: &App,
) -> Stateful<ala::Div> {
    sidebar_panel(
        id,
        width,
        titlebar,
        navigation,
        cx.theme().ground(),
        true,
        cx,
    )
}

#[must_use]
pub fn workspace_slideover_layer(cx: &App) -> Layer {
    Layer {
        radius: px(0.0),
        ..cx.theme().layer(SurfaceRole::POPOVER, PaintState::Rest)
    }
}

#[must_use]
pub fn workspace_sidebar_slideover(
    id: impl Into<ElementId>,
    sidebar: impl IntoElement,
    on_dismiss: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<ala::Div> {
    div()
        .id(id)
        .absolute()
        .inset_0()
        .flex()
        .items_start()
        .bg(cx.theme().scrim)
        .occlude()
        .on_mouse_down(MouseButton::Left, on_dismiss)
        .child(
            div()
                .h_full()
                .flex()
                .flex_none()
                .layer(&workspace_slideover_layer(cx))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(sidebar),
        )
}

#[must_use]
pub fn workspace_sidebar_ground(window: Ground, slideover: bool, cx: &App) -> Ground {
    if slideover {
        workspace_slideover_layer(cx).ground
    } else {
        window
    }
}

#[must_use]
pub fn workspace_sidebar_fade(ground: Ground, cx: &App) -> Hsla {
    let sidebar = cx
        .theme()
        .layer(Surface::new(roles::SIDEBAR).on(ground), PaintState::Rest);
    ground.fill.blend(sidebar.fill)
}

pub fn workspace_row_fill(
    id: &ElementId,
    role: SurfaceRole,
    lit: PaintState,
    hover_group: Option<SharedString>,
    ground: Ground,
    radius: Option<Pixels>,
    cx: &App,
) -> impl Iterator<Item = AnyElement> + use<> {
    let theme = cx.theme();
    let inset = theme.row_inset;
    let vertical_inset = if theme.inset_rows() {
        px(WORKSPACE_TREE_FILL_VERTICAL_INSET)
    } else {
        px(0.)
    };
    let surface = Surface::new(role).on(ground);
    let radius = radius.filter(|_| theme.style(role).shape == Shape::Round);
    let layer = |state| {
        let mut layer = theme.layer(surface, state);
        if let Some(radius) = radius {
            layer.radius = radius;
        }
        layer
    };
    let hover = layer(PaintState::Hover);
    let fill = div()
        .absolute()
        .top(vertical_inset)
        .bottom(vertical_inset)
        .left(inset)
        .right(inset)
        .layer(&layer(lit))
        .when_some(
            hover_group.filter(|_| lit == PaintState::Rest),
            |this, group| this.group_hover(group, move |style| style.layer(&hover)),
        );
    let press = CoarsePointer::get(cx).then(|| {
        let pressed = layer(PaintState::Active);
        crate::touch::press_highlight(id.clone(), pressed.fill, pressed.radius)
            .top(vertical_inset)
            .bottom(vertical_inset)
            .left(inset)
            .right(inset)
    });
    std::iter::once(fill.into_any_element()).chain(press.map(IntoElement::into_any_element))
}

#[must_use]
#[allow(clippy::fn_params_excessive_bools)]
pub fn workspace_tree_row(
    id: impl Into<ElementId>,
    depth: u8,
    active: bool,
    selected: bool,
    focused: bool,
    connected: bool,
    clickable: bool,
    hover_actions: bool,
    row_group: SharedString,
    marker: impl IntoElement,
    label: impl IntoElement,
    actions: impl IntoElement,
    ground: Ground,
    cx: &App,
) -> Stateful<ala::Div> {
    let trailing = div()
        .h_full()
        .flex()
        .flex_none()
        .items_center()
        .pl(px(6.0))
        .when(hover_actions, |this| {
            this.invisible()
                .group_hover(row_group.clone(), ala::Styled::visible)
        })
        .child(actions);
    let id = id.into();
    let lit = if selected || active && focused {
        PaintState::Current
    } else {
        PaintState::Rest
    };
    let foreground = cx.theme().foreground;
    let ink = cx
        .theme()
        .layer(Surface::new(roles::SIDEBAR_ROW).on(ground), lit)
        .text;
    let fill = workspace_row_fill(
        &id,
        roles::SIDEBAR_ROW,
        lit,
        Some(row_group.clone()),
        ground,
        None,
        cx,
    );
    workspace_tree_row_frame(id, depth, cx)
        .pr(workspace_tree_trailing_inset(cx))
        .group(row_group)
        .children(fill)
        .text_color(if connected { ink } else { foreground.muted() })
        .when(connected, |row| {
            row.hover(move |row| row.text_color(foreground))
        })
        .when(active, crate::StyledExt::font_medium)
        .when(clickable, ala::Styled::cursor_pointer)
        .child(marker)
        .child(workspace_tree_row_label(label))
        .child(trailing)
}

#[must_use]
pub fn window_shortcut_cap(stroke: Keystroke, ground: Ground) -> Kbd {
    Kbd::new(stroke).small().on(ground)
}

#[must_use]
pub fn workspace_tree_shortcut(
    row: Stateful<ala::Div>,
    stroke: Option<Keystroke>,
    ground: Ground,
) -> Stateful<ala::Div> {
    row.children(stroke.map(|stroke| {
        div()
            .flex_none()
            .pl(px(6.0))
            .child(window_shortcut_cap(stroke, ground))
    }))
}

#[must_use]
pub fn workspace_tree_action_button(
    id: impl Into<ElementId>,
    icon: IconName,
    tooltip: impl Into<SharedString>,
    disabled: bool,
    cx: &App,
) -> Button {
    workspace_row_action(
        Button::compact_icon(id, icon)
            .hit_slop(TREE_TOUCH_SLOP, TREE_TOUCH_SLOP)
            .tooltip(tooltip),
        cx.theme().ground(),
        disabled,
        cx,
    )
}

#[must_use]
pub fn workspace_row_action(button: Button, ground: Ground, disabled: bool, cx: &App) -> Button {
    let ink = cx.theme().paint_on(ground, Tone::Neutral, Emphasis::Ghost);
    button
        .ghost()
        .surface_role(roles::ROW_ACTION)
        .on(ground)
        .disabled(disabled)
        .text_color(if disabled {
            ink.faint_at(PaintState::Rest)
        } else {
            ink.muted_at(PaintState::Rest)
        })
}

fn workspace_tree_row_height(cx: &App) -> f32 {
    if CoarsePointer::get(cx) {
        TOUCH_TARGET
    } else {
        px(WORKSPACE_TREE_ROW_HEIGHT).as_f32()
    }
}

fn workspace_tree_row_frame(id: impl Into<ElementId>, depth: u8, cx: &App) -> Stateful<ala::Div> {
    div()
        .id(id)
        .w_full()
        .min_w_0()
        .h(px(workspace_tree_row_height(cx)))
        .flex_none()
        .relative()
        .flex()
        .items_center()
        .pl(px(
            SIDEBAR_CONTENT_INSET + f32::from(depth) * WORKSPACE_TREE_INDENT_WIDTH
        ))
        .pr(px(SIDEBAR_CONTENT_INSET))
        .text_sm()
}

fn workspace_tree_row_label(label: impl IntoElement) -> ala::Div {
    div()
        .min_w_0()
        .flex_1()
        .ml(px(WORKSPACE_TREE_MARKER_LABEL_GAP))
        .flex()
        .items_center()
        .gap_2()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_fade(crate::LABEL_FADE)
        .child(label)
}

#[must_use]
pub fn workspace_tree_marker(marker: impl IntoElement) -> ala::Div {
    div()
        .h_full()
        .w(px(WORKSPACE_TREE_MARKER_SLOT_WIDTH))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .child(marker)
}

fn workspace_tree_turn(disclosure: ElementId) -> ElementId {
    ElementId::from((disclosure, "turn"))
}

pub fn workspace_tree_opened(
    id: impl Into<ElementId>,
    cx: &mut App,
) -> Option<crate::motion::Instant> {
    crate::motion::turned(workspace_tree_turn(id.into()), true, cx)
}

#[must_use]
pub fn workspace_tree_disclosure(
    id: impl Into<ElementId>,
    marker: impl IntoElement,
    expanded: bool,
    row_group: SharedString,
    cx: &App,
) -> Stateful<ala::Div> {
    let foreground = cx.theme().foreground;
    let id = id.into();
    let chevron = crate::motion::Disclosure::new(
        workspace_tree_turn(id.clone()),
        expanded,
        rems_from_px(WORKSPACE_TREE_NODE_ICON_SIZE),
    );
    div()
        .id(id)
        .relative()
        .h_full()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(foreground.muted())
        .hover(move |this| this.text_color(foreground))
        .child(
            div()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .group_hover(row_group.clone(), ala::Styled::invisible)
                .child(marker),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .invisible()
                .group_hover(row_group, ala::Styled::visible)
                .child(chevron),
        )
}
