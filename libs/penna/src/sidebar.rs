use crate::{
    ActiveTheme as _, Edge, Ground, IconName, Sizable as _, StyledExt as _, Surface,
    button::Button, roles,
};
use ala::{
    App, CursorStyle, ElementId, Hsla, IntoElement, ParentElement as _, Pixels, Stateful,
    Styled as _, div, prelude::*, px,
};

pub const SIDEBAR_DEFAULT_WIDTH: f32 = 256.0;
pub const SIDEBAR_CONTENT_INSET: f32 = 8.0;
pub const SIDEBAR_SCROLLBAR_GUTTER: Pixels = px(8.0);
pub const SIDEBAR_ROW_ACTION_CENTER: Pixels = px(19.0);
pub const CHROME_BUTTON_SIZE: Pixels = px(32.0);
const CHROME_ICON_SIZE: Pixels = px(17.0);
const SIDEBAR_FOOTER_TOP: f32 = 4.0;
const SIDEBAR_RESIZE_HANDLE_WIDTH: f32 = 8.0;
pub const STATUS_DOT_SIZE: f32 = 6.0;

#[must_use]
pub fn status_dot(color: Hsla) -> ala::Div {
    div()
        .flex_none()
        .size(crate::rems_from_px(STATUS_DOT_SIZE))
        .rounded_full()
        .bg(color)
}

#[must_use]
pub fn controls_leading_inset(cx: &App) -> Pixels {
    if cfg!(target_os = "macos") {
        crate::macos_traffic_light_clearance(cx)
    } else {
        px(SIDEBAR_CONTENT_INSET)
    }
}

#[must_use]
pub fn chrome_button(id: impl Into<ElementId>, icon: IconName, tooltip: &'static str) -> Button {
    Button::compact_icon(id, icon)
        .surface_role(roles::CHROME_BUTTON)
        .with_size(CHROME_ICON_SIZE / 0.75)
        .size(CHROME_BUTTON_SIZE)
        .tooltip(tooltip)
}

fn chrome_inset(cx: &App) -> Pixels {
    (cx.theme().pane_header_height() - CHROME_BUTTON_SIZE) / 2.0
}

fn trailing_inset(cx: &App) -> Pixels {
    SIDEBAR_SCROLLBAR_GUTTER + cx.theme().row_inset + SIDEBAR_ROW_ACTION_CENTER
        - CHROME_BUTTON_SIZE / 2.0
}

#[must_use]
pub fn sidebar_titlebar(
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
        .pl(controls_leading_inset(cx))
        .pr(trailing_inset(cx))
        .child(controls)
}

pub fn sidebar_footer(
    id: impl Into<ElementId>,
    leading: impl IntoElement,
    trailing: Option<ala::AnyElement>,
    cx: &App,
) -> Stateful<ala::Div> {
    let window_inset = cx.theme().pane_margin + chrome_inset(cx);
    div()
        .id(id)
        .flex()
        .flex_none()
        .w_full()
        .items_center()
        .justify_between()
        .pt(px(SIDEBAR_FOOTER_TOP))
        .pl(window_inset)
        .pr(trailing_inset(cx))
        .pb(window_inset)
        .child(leading)
        .children(trailing)
}

#[must_use]
pub fn sidebar_panel(
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

#[must_use]
pub fn sidebar_resize_handle(
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
        .w(px(SIDEBAR_RESIZE_HANDLE_WIDTH))
        .cursor(CursorStyle::ResizeLeftRight)
        .occlude()
        .when(!divider, |this| {
            this.hover(|this| this.rule_on(roles::SIDEBAR_RULE, Edge::Right, cx))
        })
}
