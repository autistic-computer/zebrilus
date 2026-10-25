use crate::ActiveTheme as _;
use crate::Colorize as _;
use crate::StyledExt as _;
use crate::kbd::Kbd;
use crate::{Edge, Emphasis, Ground, PaintState, Surface, SurfaceRole, Tone, roles};
use zz_gpui::{
    AnyElement, App, ElementId, Hsla, InteractiveElement as _, IntoElement, Keystroke,
    ParentElement as _, Pixels, SharedString, Stateful, Styled as _, WindowControlArea, div,
    prelude::FluentBuilder as _, px,
};

const BLURRED_CHROME_OPACITY: f32 = 0.93;

#[allow(
    clippy::disallowed_methods,
    reason = "blurred chrome lets the window material show through"
)]
pub fn chrome_background(background: Hsla, blur_opacity: Option<f32>) -> Hsla {
    match blur_opacity {
        Some(opacity) => background.opacity(
            BLURRED_CHROME_OPACITY + (1.0 - BLURRED_CHROME_OPACITY) * opacity.clamp(0.0, 1.0),
        ),
        None => background,
    }
}

pub fn app_shell_surface(
    id: impl Into<ElementId>,
    background: Hsla,
    sidebar: impl IntoElement,
    titlebar: Option<AnyElement>,
    workspace: impl IntoElement,
    overlays: impl IntoIterator<Item = AnyElement>,
) -> Stateful<zz_gpui::Div> {
    div()
        .id(id)
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .overflow_hidden()
        .bg(background)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(sidebar)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .children(titlebar)
                        .child(div().flex().flex_1().min_w_0().min_h_0().child(workspace)),
                ),
        )
        .children(overlays)
}

/// `titlebar` laid `gap` down over the workspace under it. The workspace
/// keeps that gap above its panes as on every other side, so the panes still
/// start where the titlebar ends but their rim and shadow are not cut off at
/// the workspace's top edge.
pub fn titlebar_over_panes(titlebar: impl IntoElement, gap: Pixels) -> zz_gpui::Div {
    div().flex_none().w_full().mb(-gap).child(titlebar)
}

/// Titlebar-height strip above the content column, carrying the client-side
/// window controls at its trailing end and a drag region left of them. Mount
/// only when [`crate::draws_window_controls`] says the buttons are ours.
pub fn app_titlebar_strip(
    id: impl Into<ElementId>,
    controls: impl IntoElement,
    cx: &App,
) -> Stateful<zz_gpui::Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .w_full()
        .h(cx.theme().title_bar_height())
        .items_center()
        .child(
            div()
                .flex_1()
                .h_full()
                .min_w_0()
                .window_control_area(WindowControlArea::Drag),
        )
        .child(controls)
}

pub fn app_workspace_surface(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    overlays: impl IntoIterator<Item = AnyElement>,
    cx: &App,
) -> Stateful<zz_gpui::Div> {
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

pub fn app_connection_state(message: impl IntoElement, cx: &App) -> zz_gpui::Div {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .text_size(crate::rems_from_px(12.0))
        .text_color(cx.theme().foreground.muted())
        .child(message)
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
) -> Stateful<zz_gpui::Div> {
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

    div()
        .id("gui-status-titlebar")
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .w_full()
        .h(height)
        .overflow_hidden()
        .surface(Surface::new(roles::TITLEBAR), cx)
        .when(!gaps, |bar| bar.rule_on(roles::BAR_RULE, Edge::Bottom, cx))
        .child(leading)
        .children(titlebar_controls)
        .child(content)
        .children(window_controls)
}

const EMPTY_WORKSPACE_WIDTH: f32 = 340.0;
const EMPTY_KEY_COLUMN: f32 = 92.0;
const EMPTY_KEY_GAP: f32 = 10.0;
const EMPTY_HINT_HEIGHT: f32 = 24.0;

pub fn empty_workspace(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    cx: &App,
) -> Stateful<zz_gpui::Div> {
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

pub fn empty_workspace_keys(rows: impl IntoIterator<Item = AnyElement>) -> zz_gpui::Div {
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
) -> Stateful<zz_gpui::Div> {
    let state = crate::picker::row_state(selected, true);
    let ground = cx.theme().layer(SurfaceRole::MENU_ITEM, state).ground;
    crate::picker::list_row(id, crate::picker::LIST_ROW_HEIGHT, selected, true, cx)
        .px(px(12.0))
        .cursor_pointer()
        .child(key_hint(keys, label, strong, ground, cx).flex_1().min_w_0())
}

pub fn empty_workspace_caption(label: impl Into<SharedString>, cx: &App) -> zz_gpui::Div {
    div()
        .mt(px(12.0))
        .mb(px(4.0))
        .pl(px(12.0 + EMPTY_KEY_COLUMN + EMPTY_KEY_GAP))
        .text_size(crate::rems_from_px(11.0))
        .text_color(cx.theme().foreground.muted())
        .child(label.into())
}

pub fn empty_workspace_hint(
    keys: impl IntoIterator<Item = Keystroke>,
    label: impl Into<SharedString>,
    cx: &App,
) -> zz_gpui::Div {
    key_hint(keys, label, false, cx.theme().ground(), cx)
        .h(px(EMPTY_HINT_HEIGHT))
        .px(px(12.0))
}

pub fn empty_workspace_connect(action: impl IntoElement, cx: &App) -> zz_gpui::Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .max_w(px(EMPTY_WORKSPACE_WIDTH))
        .items_start()
        .gap_3()
        .text_size(crate::rems_from_px(12.0))
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
    cx: &App,
) -> zz_gpui::Div {
    let ink = if strong {
        ground.ink
    } else {
        cx.theme()
            .paint_on(ground, Tone::Neutral, Emphasis::Ghost)
            .muted_at(PaintState::Rest)
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
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(crate::rems_from_px(12.0))
                .text_color(ink)
                .child(label.into()),
        )
}
