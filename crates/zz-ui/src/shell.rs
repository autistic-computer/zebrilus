use crate::ActiveTheme as _;
use crate::Colorize as _;
use crate::StyledExt as _;
use crate::kbd::Kbd;
use crate::{Edge, Ground};
pub use penna::blocks::shell::*;
use penna::{Bar, BarLook, Label, RowLook, RuleLook, TextStyle};

use ala::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, Keystroke,
    ParentElement as _, Pixels, SharedString, Stateful, Styled as _, WindowControlArea, div,
    prelude::FluentBuilder as _, px,
};

#[must_use]
pub fn shell_ground(window: ala::Hsla, gaps: bool, cx: &App) -> ala::Hsla {
    if gaps && window.a >= 1.0 {
        crate::navigation::workspace_sidebar_fade(
            Ground {
                fill: window,
                ..cx.theme().ground()
            },
            cx,
        )
    } else {
        window
    }
}

pub fn titlebar_over_panes(titlebar: impl IntoElement, gap: Pixels) -> ala::Div {
    div().flex_none().w_full().mb(-gap).child(titlebar)
}

pub fn app_workspace_surface(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    overlays: impl IntoIterator<Item = AnyElement>,
    cx: &App,
) -> Stateful<ala::Div> {
    div()
        .id(id)
        .relative()
        .flex()
        .size_full()
        .text_color(cx.theme().foreground)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(div().flex().flex_1().child(content)),
        )
        .children(overlays)
}

pub fn app_connection_state(message: impl IntoElement, cx: &App) -> ala::Div {
    crate::feedback::empty_state(None, message, cx)
}

#[derive(Default)]
pub struct WorkspaceStatusSlots {
    pub windows: Vec<AnyElement>,
    pub right: Vec<AnyElement>,
    pub titlebar_controls: Option<(AnyElement, Pixels)>,
    pub window_controls: Option<AnyElement>,
}

pub fn workspace_status_bar(
    gaps: bool,
    leading_inset: Pixels,
    slots: WorkspaceStatusSlots,
    cx: &App,
) -> Stateful<ala::Div> {
    let height = cx.theme().title_bar_height();
    let window_strip = div()
        .flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .items_center()
        .gap(px(2.0))
        .px(px(2.0))
        .overflow_hidden()
        .children(slots.windows);
    let content = div()
        .flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .items_center()
        .gap(px(6.0))
        .px(px(6.0))
        .window_control_area(WindowControlArea::Drag)
        .child(window_strip)
        .children(slots.right);
    let leading = div()
        .flex_none()
        .w(leading_inset)
        .h(height)
        .window_control_area(WindowControlArea::Drag);
    let titlebar_controls = slots.titlebar_controls.map(|(controls, width)| {
        div()
            .flex()
            .flex_none()
            .items_center()
            .w(width)
            .h(height)
            .child(controls)
            .into_any_element()
    });
    let window_controls = slots.window_controls.map(|controls| {
        div()
            .flex_none()
            .h(height)
            .child(controls)
            .into_any_element()
    });

    let bar = Bar::new("gui-status-titlebar-bar", BarLook::Window)
        .when(!gaps, |bar| bar.rule(RuleLook::Bar, Edge::Bottom, cx))
        .flex()
        .size_full()
        .px_0()
        .items_center()
        .overflow_hidden()
        .child(leading)
        .children(titlebar_controls)
        .child(content)
        .children(window_controls);
    div()
        .id("gui-status-titlebar")
        .relative()
        .flex()
        .flex_none()
        .w_full()
        .h(height)
        .child(bar)
}

const EMPTY_WORKSPACE_WIDTH: f32 = 340.0;
const EMPTY_KEY_COLUMN: f32 = 92.0;
const EMPTY_KEY_GAP: f32 = 10.0;
const EMPTY_HINT_HEIGHT: f32 = 24.0;

pub fn empty_workspace(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    cx: &App,
) -> Stateful<ala::Div> {
    div()
        .id(id)
        .flex()
        .size_full()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .text_color(cx.theme().foreground)
        .px(px(12.0))
        .child(content)
}

pub fn empty_workspace_keys(rows: impl IntoIterator<Item = AnyElement>) -> ala::Div {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w_full()
        .max_w(px(EMPTY_WORKSPACE_WIDTH))
        .gap(px(2.0))
        .children(rows)
}

pub fn empty_workspace_action(
    id: impl Into<ElementId>,
    keys: impl IntoIterator<Item = Keystroke>,
    label: impl Into<SharedString>,
    strong: bool,
    selected: bool,
    cx: &App,
) -> Stateful<ala::Div> {
    let state = crate::row_state(selected, true);
    let ground = cx.theme().ground_at(RowLook::Menu, state);
    crate::list_row(id, crate::picker::list_row_height(cx), selected, true, cx)
        .px(px(12.0))
        .cursor_pointer()
        .child(key_hint(keys, label, strong, ground).flex_1().min_w_0())
}

pub fn empty_workspace_caption(label: impl Into<SharedString>, cx: &App) -> ala::Div {
    div()
        .mt(px(12.0))
        .mb(px(4.0))
        .pl(px(12.0 + EMPTY_KEY_COLUMN + EMPTY_KEY_GAP))
        .type_style(TextStyle::Caption, cx)
        .text_color(cx.theme().foreground.muted())
        .child(label.into())
}

pub fn empty_workspace_hint(
    keys: impl IntoIterator<Item = Keystroke>,
    label: impl Into<SharedString>,
    cx: &App,
) -> ala::Div {
    key_hint(keys, label, false, cx.theme().ground())
        .h(px(EMPTY_HINT_HEIGHT))
        .px(px(12.0))
}

pub fn empty_workspace_connect(action: impl IntoElement, cx: &App) -> ala::Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .max_w(px(EMPTY_WORKSPACE_WIDTH))
        .items_start()
        .gap_3()
        .type_style(TextStyle::Small, cx)
        .child("Connect to a computer")
        .child(
            div()
                .text_color(cx.theme().foreground.muted())
                .child(
                    "zz attaches to a daemon over SSH. On your Mac, enable Remote Login in System Settings, then add it here.",
                ),
        )
        .child(action)
}

fn key_hint(
    keys: impl IntoIterator<Item = Keystroke>,
    label: impl Into<SharedString>,
    strong: bool,
    ground: Ground,
) -> ala::Div {
    let label = Label::new(TextStyle::Small, label)
        .on(ground)
        .flex_1()
        .min_w_0()
        .truncate();
    let label = if strong {
        label.text_color(ground.ink)
    } else {
        label.muted()
    };
    div()
        .flex()
        .items_center()
        .gap(px(EMPTY_KEY_GAP))
        .child(
            div()
                .flex()
                .flex_none()
                .w(px(EMPTY_KEY_COLUMN))
                .items_center()
                .justify_end()
                .gap(px(4.0))
                .children(
                    keys.into_iter()
                        .map(|key| Kbd::new(key).lowercase().on(ground)),
                ),
        )
        .child(label)
}
