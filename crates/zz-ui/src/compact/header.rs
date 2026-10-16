use zz_gpui::{
    AnyElement, App, Div, ElementId, ParentElement as _, SharedString, Styled as _, div, prelude::*,
};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, PANE_HEADER_CONTROL_HEIGHT, StyledExt as _,
    button::{Button, ButtonVariants as _},
    rems_from_px, roles,
};

pub const COMPACT_PANE_HEADER_HEIGHT: f32 = 36.0;

const PADDING_X: f32 = 12.0;
const ACTIONS_RIGHT: f32 = 6.0;
const ACTION_SLOT: f32 = 28.0;

pub fn compact_pane_header(
    icon: IconName,
    title: impl Into<SharedString>,
    actions: impl IntoIterator<Item = AnyElement>,
    cx: &App,
) -> Div {
    let actions: Vec<AnyElement> = actions.into_iter().collect();
    let reserve = if actions.is_empty() {
        PADDING_X
    } else {
        ACTIONS_RIGHT + ACTION_SLOT * actions.len() as f32 + 4.0
    };
    div()
        .debug_selector(|| "compact-pane-header".to_owned())
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .w_full()
        .h(rems_from_px(COMPACT_PANE_HEADER_HEIGHT))
        .pl(rems_from_px(PADDING_X))
        .pr(rems_from_px(reserve))
        .gap(rems_from_px(8.0))
        .font_family(cx.theme().font_family.clone())
        .text_size(rems_from_px(15.0))
        .line_height(rems_from_px(20.0))
        .surface(roles::PANE_HEADER, cx)
        .text_color(cx.theme().foreground.muted())
        .child(
            div()
                .flex_none()
                .relative()
                .top(rems_from_px(0.5))
                .opacity(0.8)
                .child(Icon::new(icon).size(rems_from_px(16.0))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(title.into()),
        )
        .when(!actions.is_empty(), |header| {
            header.child(
                div()
                    .debug_selector(|| "compact-pane-header-actions".to_owned())
                    .absolute()
                    .top_0()
                    .right(rems_from_px(ACTIONS_RIGHT))
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(rems_from_px(4.0))
                    .children(actions),
            )
        })
}

/// A pane header button showing how many tabs a page holds, the count in a
/// rounded square; it opens the page's tab drawer.
#[allow(
    clippy::disallowed_methods,
    reason = "the box is a glyph in the button's text color, not a surface edge"
)]
pub fn compact_tabs_button(id: impl Into<ElementId>, count: usize, cx: &App) -> Button {
    let color = cx.theme().foreground.faint();
    Button::new(id)
        .ghost()
        .compact()
        .surface_role(roles::PANE_HEADER_BUTTON)
        .size(PANE_HEADER_CONTROL_HEIGHT)
        .text_color(color)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(rems_from_px(15.0))
                .rounded(rems_from_px(4.0))
                .border(rems_from_px(1.5))
                .border_color(color)
                .text_size(rems_from_px(9.0))
                .line_height(rems_from_px(10.0))
                .font_semibold()
                .child(SharedString::from(count.min(99).to_string())),
        )
}
