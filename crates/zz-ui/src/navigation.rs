pub mod flat;
pub mod pane_stack;
pub mod sidebar;
pub mod status;

use crate::{
    ActiveTheme as _, Colorize as _, Disableable as _, Emphasis, Ground, IconName, PaintState,
    Sizable as _, StyledExt as _, Surface, Tone,
    button::{Button, ButtonLook, ButtonVariants as _, INSIDE_GAP},
    kbd::Kbd,
    rems_from_px,
    sidebar::{
        CHROME_BUTTON_SIZE, SIDEBAR_CONTENT_INSET, SIDEBAR_SCROLLBAR_GUTTER, chrome_button,
        sidebar_panel,
    },
    tooltip::Tooltip,
    touch::{CoarsePointer, TOUCH_TARGET},
};
use ala::{
    App, ElementId, Hsla, IntoElement, Keystroke, MouseButton, MouseDownEvent, ParentElement as _,
    Pixels, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::*, px,
};
pub use penna::blocks::sidebar::sidebar_fade as workspace_sidebar_fade;
use penna::{Panel, PanelLook, Row, RowLook, Space, Tab, TabLook, TabStrip, TextStyle};

pub const WORKSPACE_TREE_ROW_HEIGHT: f32 = 32.0;
pub const WORKSPACE_TREE_INDENT_WIDTH: f32 = 20.0;
pub const WORKSPACE_TREE_MARKER_SLOT_WIDTH: f32 = 18.0;
pub const WORKSPACE_TREE_NODE_ICON_SIZE: f32 = 14.0;
pub const WORKSPACE_TREE_MARKER_LABEL_GAP: f32 = 6.0;
const WORKSPACE_TREE_FILL_VERTICAL_INSET: f32 = 1.0;
const WORKSPACE_CHROME_CONTROL_GAP: f32 = 4.0;
const WORKSPACE_SIDEBAR_ACTIONS_GAP: f32 = 8.0;
pub const WORKSPACE_SIDEBAR_FADE_HEIGHT: Pixels = px(40.0);
pub const WORKSPACE_STATUS_PILL_HEIGHT: Pixels = px(32.0);
pub const WORKSPACE_STATUS_TAB_HEIGHT: Pixels = px(28.0);
const WORKSPACE_STATUS_LINE_HEIGHT: Pixels = px(18.0);
const WORKSPACE_STATUS_WINDOW_DEFAULT_WIDTH: Pixels = px(240.0);
const WORKSPACE_STATUS_WINDOW_MIN_WIDTH: Pixels = px(36.0);

#[must_use]
pub fn workspace_status_tab_height(alone: bool) -> Pixels {
    if alone {
        WORKSPACE_STATUS_TAB_HEIGHT + px(Space::S2.design_px() * 2.0)
    } else {
        WORKSPACE_STATUS_TAB_HEIGHT
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceStatusWindowState {
    pub connected: bool,
    pub active: bool,
    pub alone: bool,
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
pub fn workspace_layout_button(id: impl Into<ElementId>, open: bool) -> Button {
    let icon = if open {
        IconName::PanelLeftDuotone
    } else {
        IconName::PanelLeft
    };
    chrome_button(id, icon, "Toggle sidebar")
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
pub fn workspace_sidebar_view_button(id: impl Into<ElementId>, flat: bool) -> Button {
    let (icon, tooltip) = if flat {
        (IconName::ChatDuotone, "Show the session tree")
    } else {
        (IconName::Chat, "Show chats")
    };
    chrome_button(id, icon, tooltip)
}

#[must_use]
pub fn workspace_sidebar_titlebar_controls(
    view: impl IntoElement,
    layout: impl IntoElement,
) -> ala::Div {
    div()
        .flex()
        .items_center()
        .gap(rems_from_px(WORKSPACE_CHROME_CONTROL_GAP))
        .child(view)
        .child(layout)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarAction {
    NewChat,
    AddHost,
    Settings,
}

impl SidebarAction {
    #[must_use]
    pub const fn icon(self) -> IconName {
        match self {
            Self::NewChat => IconName::ChatPlus,
            Self::AddHost => IconName::ServerPlus,
            Self::Settings => IconName::Settings,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NewChat => "New chat",
            Self::AddHost => "Add host",
            Self::Settings => "Settings",
        }
    }

    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::NewChat => "sidebar-new-chat",
            Self::AddHost => "sidebar-add-host",
            Self::Settings => "sidebar-settings",
        }
    }

    #[must_use]
    pub const fn above(flat: bool) -> &'static [Self] {
        if flat {
            &[Self::NewChat, Self::AddHost]
        } else {
            &[Self::AddHost]
        }
    }
}

#[must_use]
pub fn workspace_sidebar_action(
    action: SidebarAction,
    ground: Ground,
    cx: &App,
) -> Stateful<ala::Div> {
    flat::flat_action_row(action.id(), action.icon(), action.label(), ground, cx)
}

#[must_use]
pub fn workspace_sidebar_actions(rows: impl IntoIterator<Item = Stateful<ala::Div>>) -> ala::Div {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w_full()
        .pr(SIDEBAR_SCROLLBAR_GUTTER)
        .pb(px(WORKSPACE_SIDEBAR_ACTIONS_GAP))
        .children(rows)
}

#[must_use]
pub fn workspace_sidebar_footer(
    id: impl Into<ElementId>,
    row: impl IntoElement,
    cx: &App,
) -> Stateful<ala::Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .flex_none()
        .w_full()
        .pt(Space::S4)
        .pr(SIDEBAR_SCROLLBAR_GUTTER)
        .pb(cx.theme().row_inset)
        .child(row)
}

#[must_use]
pub fn workspace_chrome_controls_width_for(count: usize, window: &Window) -> Pixels {
    let count = f32::from(u16::try_from(count).unwrap_or(u16::MAX));
    CHROME_BUTTON_SIZE * count
        + rems_from_px(WORKSPACE_CHROME_CONTROL_GAP).to_pixels(window.rem_size())
            * (count - 1.0).max(0.0)
}

#[must_use]
pub fn workspace_status_bar_ground(cx: &App) -> Ground {
    cx.theme().ground_of(penna::BarLook::Window)
}

#[must_use]
pub fn workspace_status_tab_ground(alone: bool, cx: &App) -> Ground {
    TabStrip::ground(
        workspace_status_bar_ground(cx),
        if alone { 1 } else { 2 },
        cx,
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
) -> Tab {
    let foreground = cx.theme().foreground;
    let name = div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_fade(crate::LABEL_FADE)
        .child(name);
    Tab::new(id, TabLook::Window)
        .on(workspace_status_tab_ground(state.alone, cx))
        .state(state.paint_state())
        .interactive(state.connected)
        .when(state.connected, penna::StyledExt::click_cursor)
        .relative()
        .flex()
        .flex_shrink_1()
        .w(WORKSPACE_STATUS_WINDOW_DEFAULT_WIDTH)
        .min_w(WORKSPACE_STATUS_WINDOW_MIN_WIDTH)
        .max_w(WORKSPACE_STATUS_WINDOW_DEFAULT_WIDTH)
        .map(|item| {
            if state.alone {
                item.h_full()
            } else {
                item.h(WORKSPACE_STATUS_TAB_HEIGHT)
            }
        })
        .items_center()
        .gap(px(5.0))
        .px(px(9.0))
        .overflow_hidden()
        .when(!state.active, |item| item.text_color(foreground.muted()))
        .type_style(TextStyle::Lead, cx)
        .line_height(WORKSPACE_STATUS_LINE_HEIGHT)
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .map(|item| match deck {
            Some(deck) => item.child(deck.beside(name, px(5.0))),
            None => item.child(name),
        })
}

#[must_use]
pub fn workspace_tree_guide_origin(cx: &App) -> Pixels {
    flat::action_glyph_center(cx)
}

fn workspace_tree_trailing_inset(cx: &App) -> Pixels {
    cx.theme().row_inset + INSIDE_GAP
}

fn workspace_tree_fill_height(cx: &App) -> Pixels {
    let inset = if cx.theme().inset_rows() {
        px(WORKSPACE_TREE_FILL_VERTICAL_INSET)
    } else {
        px(0.)
    };
    px(workspace_tree_row_height(cx)) - inset * 2.0
}

#[must_use]
pub fn workspace_tree_action_side(cx: &App) -> Pixels {
    workspace_tree_fill_height(cx) - INSIDE_GAP * 2.0
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
            Panel::new("workspace-slideover", PanelLook::Popover)
                .rounded(px(0.0))
                .h_full()
                .flex()
                .flex_none()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(sidebar),
        )
}

#[must_use]
pub fn workspace_sidebar_ground(window: Ground, slideover: bool, cx: &App) -> Ground {
    if slideover {
        cx.theme().ground_of(PanelLook::Popover)
    } else {
        window
    }
}

pub fn workspace_row_fill(
    id: &ElementId,
    look: RowLook,
    lit: PaintState,
    hover_group: Option<SharedString>,
    ground: Ground,
    cx: &App,
) -> Row {
    let theme = cx.theme();
    let inset = theme.row_inset;
    let vertical_inset = if theme.inset_rows() {
        px(WORKSPACE_TREE_FILL_VERTICAL_INSET)
    } else {
        px(0.)
    };
    let fill = Row::new(ElementId::from((id.clone(), "fill")), look)
        .state(lit)
        .on(ground)
        .absolute()
        .top(vertical_inset)
        .bottom(vertical_inset)
        .left(inset)
        .right(inset);
    match hover_group {
        Some(group) => fill.hover_group(group),
        None => fill.interactive(false).press_highlight(true),
    }
}

const fn tree_row_state(active: bool, selected: bool, focused: bool) -> PaintState {
    if selected || active && focused {
        PaintState::Current
    } else {
        PaintState::Rest
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkspaceTreeInk {
    pub ground: Ground,
    pub text: Hsla,
    pub muted: Hsla,
    pub faint: Hsla,
}

impl WorkspaceTreeInk {
    #[must_use]
    pub fn new(active: bool, selected: bool, focused: bool, ground: Ground, cx: &App) -> Self {
        Self::at(tree_row_state(active, selected, focused), ground, cx)
    }

    #[must_use]
    pub fn rest(cx: &App) -> Self {
        Self::at(PaintState::Rest, cx.theme().ground(), cx)
    }

    fn at(lit: PaintState, ground: Ground, cx: &App) -> Self {
        let theme = cx.theme();
        let ground = theme.ground_at(Surface::from(RowLook::Sidebar).on(ground), lit);
        if ground.solid {
            let paint = theme.paint_on(ground, Tone::Neutral, Emphasis::Ghost);
            Self {
                ground,
                text: ground.ink,
                muted: paint.muted_at(PaintState::Rest),
                faint: paint.faint_at(PaintState::Rest),
            }
        } else {
            Self {
                ground,
                text: theme.foreground,
                muted: theme.foreground.muted(),
                faint: theme.foreground.faint(),
            }
        }
    }

    #[must_use]
    pub const fn node(self, emphasized: bool) -> Hsla {
        if emphasized { self.text } else { self.muted }
    }
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
    let lit = tree_row_state(active, selected, focused);
    let ink = WorkspaceTreeInk::at(lit, ground, cx);
    let fill = workspace_row_fill(
        &id,
        RowLook::Sidebar,
        lit,
        Some(row_group.clone()),
        ground,
        cx,
    );
    workspace_tree_row_frame(id, depth, cx)
        .map(|row| {
            if active {
                row.type_style_strong(TextStyle::Lead, cx)
            } else {
                row.type_style(TextStyle::Lead, cx)
            }
        })
        .pr(workspace_tree_trailing_inset(cx))
        .group(row_group)
        .child(fill)
        .text_color(if connected { ink.ground.ink } else { ink.muted })
        .when(connected, |row| {
            row.hover(move |row| row.text_color(ink.text))
        })
        .when(clickable, penna::StyledExt::click_cursor)
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
    ground: Ground,
    cx: &App,
) -> Button {
    let slop = ((px(TOUCH_TARGET) - workspace_tree_action_side(cx)) / 2.0)
        .max(px(0.))
        .as_f32();
    workspace_row_action(
        Button::new(id)
            .icon(icon)
            .inside(RowLook::Sidebar, workspace_tree_fill_height(cx))
            .hit_slop(slop, slop)
            .tooltip(tooltip),
        ground,
        disabled,
        cx,
    )
}

#[must_use]
pub fn workspace_row_action(button: Button, ground: Ground, disabled: bool, cx: &App) -> Button {
    let ink = cx.theme().paint_on(ground, Tone::Neutral, Emphasis::Ghost);
    button
        .ghost()
        .look(ButtonLook::RowIcon)
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
        .pl(
            workspace_tree_guide_origin(cx) - px(WORKSPACE_TREE_MARKER_SLOT_WIDTH / 2.0)
                + px(f32::from(depth) * WORKSPACE_TREE_INDENT_WIDTH),
        )
        .pr(px(SIDEBAR_CONTENT_INSET))
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
    ink: WorkspaceTreeInk,
) -> Stateful<ala::Div> {
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
        .click_cursor()
        .text_color(ink.muted)
        .hover(move |this| this.text_color(ink.text))
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
