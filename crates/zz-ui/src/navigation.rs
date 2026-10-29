pub mod flat;
pub mod pane_stack;
pub mod sidebar;
pub mod status;
pub mod tree;

use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Edge, Emphasis, Ground, Icon, IconName,
    InteractiveSurfaceExt as _, Layer, PaintState, Shape, Sizable as _, StyledExt as _, Surface,
    SurfaceRole, Tone,
    button::{Button, ButtonVariants as _, COMPACT_ICON_BUTTON_SIZE},
    kbd::Kbd,
    rems_from_px, roles,
    tooltip::Tooltip,
    touch::{CoarsePointer, TOUCH_TARGET},
};
use ala::{
    AnyElement, App, CursorStyle, ElementId, Hsla, IntoElement, Keystroke, MouseButton,
    MouseDownEvent, ParentElement as _, Pixels, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::*, px,
};

pub const WORKSPACE_TREE_ROW_HEIGHT: f32 = 32.0;
pub const WORKSPACE_TREE_INDENT_WIDTH: f32 = 20.0;
pub const WORKSPACE_TREE_CONTENT_INSET: f32 = 8.0;
pub const WORKSPACE_TREE_MARKER_SLOT_WIDTH: f32 = 18.0;
/// Icon size for a tree row's node marker. Matches `Size::Medium` in `icon/mod.rs`.
pub const WORKSPACE_TREE_NODE_ICON_SIZE: f32 = 14.0;
pub const WORKSPACE_TREE_MARKER_LABEL_GAP: f32 = 6.0;
const TREE_TOUCH_SLOP: f32 = (TOUCH_TARGET - COMPACT_ICON_BUTTON_SIZE) / 2.0;
const WORKSPACE_TREE_FILL_VERTICAL_INSET: f32 = 1.0;
pub const WORKSPACE_SIDEBAR_DEFAULT_WIDTH: f32 = 256.0;
const WORKSPACE_CHROME_CONTROL_GAP: f32 = 4.0;
/// How far a window tab's close button sits in from the tab's trailing edge.
const WORKSPACE_ROW_ACTION_GAP: f32 = 10.0;
/// Where a sidebar row's trailing actions (+, …, ×) center, in from the
/// trailing edge of the row's fill. The sidebar toggle and the view switch
/// center on the same line.
pub const WORKSPACE_ROW_ACTION_CENTER: Pixels = px(19.0);
/// How far the sidebar's rows stop short of its trailing edge past the row
/// inset, so the scrollbar's thumb (8px at its widest, 2px in) runs beside
/// them instead of over them.
pub const WORKSPACE_SIDEBAR_SCROLLBAR_GUTTER: Pixels = px(8.0);
const WORKSPACE_SIDEBAR_FOOTER_TOP: f32 = 4.0;
const WORKSPACE_SIDEBAR_RESIZE_HANDLE_WIDTH: f32 = 8.0;
pub const WORKSPACE_SIDEBAR_FADE_HEIGHT: Pixels = px(40.0);
/// The titlebar's window tabs and status chips: the largest controls in the
/// chrome, a step up from the sidebar's buttons.
pub const WORKSPACE_STATUS_PILL_HEIGHT: Pixels = px(32.0);
/// The sidebar toggle, settings and view switch: icon buttons as tall as the
/// window tabs beside them, with 17px glyphs.
pub const WORKSPACE_CHROME_BUTTON_SIZE: Pixels = WORKSPACE_STATUS_PILL_HEIGHT;
const WORKSPACE_STATUS_WINDOW_TEXT_SIZE: f32 = 14.0;
const WORKSPACE_CHROME_ICON_SIZE: Pixels = px(17.0);
const WORKSPACE_STATUS_LINE_HEIGHT: Pixels = px(18.0);
const WORKSPACE_STATUS_ITEM_MAX_WIDTH: Pixels = px(180.0);
const WORKSPACE_STATUS_WINDOW_DEFAULT_WIDTH: Pixels = px(240.0);
const WORKSPACE_STATUS_WINDOW_MIN_WIDTH: Pixels = px(36.0);
const WORKSPACE_STATUS_ICON_SIZE: Pixels = px(14.0);
const WORKSPACE_STATUS_TEXT_SIZE: f32 = 13.0;
const WORKSPACE_STATUS_CHIP_PADDING: Pixels = px(6.0);
pub(crate) const STATUS_DOT_SIZE: f32 = 6.0;

#[must_use]
pub fn status_dot(color: Hsla) -> ala::Div {
    div()
        .flex_none()
        .size(rems_from_px(STATUS_DOT_SIZE))
        .rounded_full()
        .bg(color)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceStatusWindowState {
    pub connected: bool,
    pub active: bool,
    pub bell: bool,
    pub activity: bool,
}

impl WorkspaceStatusWindowState {
    /// How the window's pill paints: current while it is the active window.
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
pub fn workspace_controls_leading_inset(cx: &App) -> Pixels {
    if cfg!(target_os = "macos") {
        crate::macos_traffic_light_clearance(cx)
    } else {
        px(WORKSPACE_TREE_CONTENT_INSET)
    }
}

#[must_use]
pub fn workspace_layout_button(id: impl Into<ElementId>) -> Button {
    workspace_chrome_button(id, IconName::PanelLeft, "Toggle sidebar")
}

#[must_use]
pub fn workspace_palette_button(id: impl Into<ElementId>) -> Button {
    workspace_chrome_button(id, IconName::Search, "Command palette")
}

#[must_use]
pub fn workspace_settings_button(id: impl Into<ElementId>) -> Button {
    workspace_chrome_button(id, IconName::Settings, "Settings")
}

#[must_use]
pub fn workspace_add_host_button(id: impl Into<ElementId>) -> Button {
    workspace_chrome_button(id, IconName::ServerPlus, "Add host")
}

/// Switches the sidebar between the session tree and the chats;
/// its icon shows the one on screen.
#[must_use]
pub fn workspace_sidebar_view_button(id: impl Into<ElementId>, flat: bool) -> Button {
    if flat {
        workspace_chrome_button(id, IconName::List, "Show the session tree")
    } else {
        workspace_chrome_button(id, IconName::ListTree, "Show chats")
    }
}

/// The footer's trailing controls: Add host, then the view toggle.
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

fn workspace_chrome_button(
    id: impl Into<ElementId>,
    icon: IconName,
    tooltip: &'static str,
) -> Button {
    Button::compact_icon(id, icon)
        .surface_role(roles::CHROME_BUTTON)
        .with_size(WORKSPACE_CHROME_ICON_SIZE / 0.75)
        .size(WORKSPACE_CHROME_BUTTON_SIZE)
        .tooltip(tooltip)
}

#[must_use]
pub fn workspace_chrome_controls_width_for(count: usize, window: &Window) -> Pixels {
    let count = f32::from(u16::try_from(count).unwrap_or(u16::MAX));
    WORKSPACE_CHROME_BUTTON_SIZE * count
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

#[must_use]
/// The sidebar's top strip: its controls sit at the trailing edge, clear of
/// the traffic lights.
pub fn workspace_sidebar_titlebar(
    id: impl Into<ElementId>,
    controls: impl IntoElement,
    cx: &App,
) -> Stateful<ala::Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .w_full()
        .h(cx.theme().title_bar_height())
        .items_center()
        .justify_end()
        .pl(workspace_controls_leading_inset(cx))
        .pr(workspace_sidebar_trailing_inset(cx))
        .child(controls)
}

/// How far the sidebar's leading buttons sit in from its edges, so their
/// centers fall on a pane header's center line: half a header in from the
/// window's edges past the pane margin.
fn workspace_chrome_inset(cx: &App) -> Pixels {
    (cx.theme().pane_header_height() - WORKSPACE_CHROME_BUTTON_SIZE) / 2.0
}

/// How far the sidebar's trailing buttons sit in from its trailing edge, so
/// they center on the column its rows' actions do.
fn workspace_sidebar_trailing_inset(cx: &App) -> Pixels {
    WORKSPACE_SIDEBAR_SCROLLBAR_GUTTER + cx.theme().row_inset + WORKSPACE_ROW_ACTION_CENTER
        - WORKSPACE_CHROME_BUTTON_SIZE / 2.0
}

/// The trailing inset of a tree row's action buttons.
fn workspace_tree_trailing_inset(cx: &App) -> Pixels {
    cx.theme().row_inset + WORKSPACE_ROW_ACTION_CENTER - px(COMPACT_ICON_BUTTON_SIZE / 2.0)
}

fn workspace_row_action_gap() -> Pixels {
    px(WORKSPACE_ROW_ACTION_GAP)
}

/// The strip under the sidebar's navigation: settings on the leading edge,
/// the view switch on the trailing edge under the sidebar toggle, both
/// centered as far from the window's bottom as the traffic lights are from
/// its top.
pub fn workspace_sidebar_footer(
    id: impl Into<ElementId>,
    leading: impl IntoElement,
    trailing: Option<ala::AnyElement>,
    cx: &App,
) -> Stateful<ala::Div> {
    let inset = workspace_chrome_inset(cx);
    let window_inset = cx.theme().pane_margin + inset;
    div()
        .id(id)
        .flex()
        .flex_none()
        .w_full()
        .items_center()
        .justify_between()
        .pt(px(WORKSPACE_SIDEBAR_FOOTER_TOP))
        .pl(window_inset)
        .pr(workspace_sidebar_trailing_inset(cx))
        .pb(window_inset)
        .child(leading)
        .children(trailing)
}

/// Full-height sidebar interior on the window, with its edge rule. Native
/// callers attach window dragging to the titlebar slot and wrap the surface
/// with resize handling.
#[must_use]
pub fn workspace_sidebar_surface(
    id: impl Into<ElementId>,
    width: f32,
    titlebar: impl IntoElement,
    navigation: impl IntoElement,
    cx: &App,
) -> Stateful<ala::Div> {
    workspace_sidebar_panel(
        id,
        width,
        titlebar,
        navigation,
        cx.theme().ground(),
        true,
        cx,
    )
}

/// [`workspace_sidebar_surface`] on `ground`, what the sidebar sits on, with
/// its edge rule only while `divider` is set: panes with gaps stand apart
/// without one.
#[must_use]
pub fn workspace_sidebar_panel(
    id: impl Into<ElementId>,
    width: f32,
    titlebar: impl IntoElement,
    navigation: impl IntoElement,
    ground: Ground,
    divider: bool,
    cx: &App,
) -> Stateful<ala::Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .w(px(width))
        .h_full()
        .min_h_0()
        .flex_none()
        .overflow_hidden()
        .relative()
        .surface(Surface::new(roles::SIDEBAR).on(ground), cx)
        .when(divider, |this| {
            this.rule_on(roles::SIDEBAR_RULE, Edge::Right, cx)
        })
        .child(titlebar)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .pb(px(8.0))
                .child(navigation),
        )
}

/// The strip along the sidebar's trailing edge that resizes it. Without a
/// divider, pointing at it shows the edge it drags. The caller attaches the
/// drag.
#[must_use]
pub fn workspace_sidebar_resize_handle(
    id: impl Into<ElementId>,
    divider: bool,
    cx: &App,
) -> Stateful<ala::Div> {
    div()
        .id(id)
        .absolute()
        .top(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .w(px(WORKSPACE_SIDEBAR_RESIZE_HANDLE_WIDTH))
        .cursor(CursorStyle::ResizeLeftRight)
        .occlude()
        .when(!divider, |this| {
            this.hover(|this| this.rule_on(roles::SIDEBAR_RULE, Edge::Right, cx))
        })
}

/// What a sidebar sliding over the panes floats on: the popover surface,
/// square against the window's edge.
#[must_use]
pub fn workspace_slideover_layer(cx: &App) -> Layer {
    Layer {
        radius: px(0.0),
        ..cx.theme().layer(SurfaceRole::POPOVER, PaintState::Rest)
    }
}

/// The sidebar sliding over the panes: `sidebar` on its own surface, over a
/// scrim that calls `on_dismiss` when pressed.
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

/// What the sidebar sits on: `window`, the plane of its column, or the
/// slideover's surface while it slides over the panes.
#[must_use]
pub fn workspace_sidebar_ground(window: Ground, slideover: bool, cx: &App) -> Ground {
    if slideover {
        workspace_slideover_layer(cx).ground
    } else {
        window
    }
}

/// The color the sidebar's list fades into at its bottom edge: the sidebar's
/// own fill over `ground`, what it sits on.
#[must_use]
pub fn workspace_sidebar_fade(ground: Ground, cx: &App) -> Hsla {
    let sidebar = cx
        .theme()
        .layer(Surface::new(roles::SIDEBAR).on(ground), PaintState::Rest);
    ground.fill.blend(sidebar.fill)
}

/// The fill behind a sidebar row drawn as `role` on `ground`, in `lit`. A
/// resting row takes its hover look while the pointer is anywhere in
/// `hover_group`. `radius`, when set, replaces the radius a round shape
/// resolves to; square and pill corners stay.
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

// Six independent row states, not a config object waiting to be extracted.
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

/// Draws `button` as an action that shows on a row under the pointer (+, ×,
/// …, the layout menu), sitting on `ground`, the row's fill: muted ink until
/// pointed at, faint while `disabled`.
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
            WORKSPACE_TREE_CONTENT_INSET + f32::from(depth) * WORKSPACE_TREE_INDENT_WIDTH
        ))
        .pr(px(WORKSPACE_TREE_CONTENT_INSET))
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

/// When the open row whose [`workspace_tree_disclosure`] is `id` last opened,
/// or `None` if it was open when first drawn.
pub fn workspace_tree_opened(
    id: impl Into<ElementId>,
    cx: &mut App,
) -> Option<crate::motion::Instant> {
    crate::motion::turned(workspace_tree_turn(id.into()), true, cx)
}

/// The expand/collapse affordance an expandable row hangs on its marker slot:
/// the node's own icon, swapped for a chevron while the row is hovered. The
/// caller wires the click and passes the [`workspace_tree_marker`] it built.
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
