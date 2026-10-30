use ala::{
    AnyElement, App, Div, ElementId, Hsla, ParentElement as _, SharedString, Stateful, Styled as _,
    div, prelude::*, px, relative,
};
use penna::compact::{BentoSpan, bento_tile};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Layer, PaintState, StyledExt as _, roles,
};

use super::overview::{HostState, host_state_dot};

const TILE_PADDING: f32 = 14.0;
const WINDOW_PADDING: f32 = 10.0;
const WINDOW_HEADER: f32 = 18.0;
const MINI_GAP: f32 = 4.0;
const MINI_RADIUS: f32 = 10.0;

pub fn window_span(panes: usize) -> BentoSpan {
    match panes {
        0..=2 => BentoSpan::new(2, 2),
        3 => BentoSpan::new(4, 2),
        _ => BentoSpan::new(4, 3),
    }
}

pub fn host_tile(
    id: impl Into<ElementId>,
    press: impl Into<ElementId>,
    state: HostState,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    footer: Vec<AnyElement>,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    bento_tile(id, press, cx).child(
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .p(px(TILE_PADDING))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(host_state_dot(state, cx))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(17.0))
                            .font_semibold()
                            .child(title.into()),
                    ),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(theme.foreground.muted())
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(detail.into()),
            )
            .child(div().flex_1())
            .children(footer),
    )
}

pub fn host_tile_line(color: Hsla, text: impl Into<SharedString>, warning: bool, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(13.0))
        .text_color(if warning {
            color
        } else {
            cx.theme().foreground.muted()
        })
        .child(div().flex_none().size(px(6.0)).rounded_full().bg(color))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(text.into()),
        )
}

pub fn window_tile(
    id: impl Into<ElementId>,
    press: impl Into<ElementId>,
    index: u32,
    title: impl Into<SharedString>,
    folder: Option<SharedString>,
    attention: bool,
    body: impl IntoElement,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let muted = theme.foreground.muted();
    bento_tile(id, press, cx).child(
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(WINDOW_PADDING))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.0))
                    .h(px(WINDOW_HEADER))
                    .px(px(4.0))
                    .text_size(px(15.0))
                    .child(div().flex_none().text_color(muted).child(index.to_string()))
                    .child(
                        div()
                            .flex_none()
                            .max_w(relative(0.6))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_semibold()
                            .child(title.into()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(13.0))
                            .text_color(muted)
                            .children(folder),
                    )
                    .when(attention, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .size(px(8.0))
                                .rounded_full()
                                .bg(theme.warning),
                        )
                    }),
            )
            .child(div().relative().flex_1().min_h_0().child(body)),
    )
}

pub fn window_layout(panes: Vec<([f32; 4], AnyElement)>) -> Div {
    let half = px(MINI_GAP / 2.0);
    div()
        .absolute()
        .inset(-half)
        .children(panes.into_iter().map(|([x, y, width, height], pane)| {
            div()
                .absolute()
                .left(relative(x))
                .top(relative(y))
                .w(relative(width))
                .h(relative(height))
                .p(half)
                .child(pane)
        }))
}

pub fn mini_pane(
    icon: IconName,
    title: Option<SharedString>,
    preview: Option<AnyElement>,
    background: Hsla,
    current: bool,
    attention: bool,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let frame = theme.layer(
        roles::PHONE_MINI,
        if current {
            PaintState::Selected
        } else {
            PaintState::Rest
        },
    );
    div()
        .relative()
        .size_full()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .p(px(6.0))
        .overflow_hidden()
        .layer(&Layer {
            fill: background,
            glass: None,
            radius: px(MINI_RADIUS),
            ..frame
        })
        .when_some(title, |pane, title| {
            pane.child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(px(11.0))
                    .text_color(theme.foreground.muted())
                    .child(Icon::new(icon).size(px(12.0)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(title),
                    )
                    .when(attention, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .size(px(6.0))
                                .rounded_full()
                                .bg(theme.warning),
                        )
                    }),
            )
        })
        .child(div().flex_1().min_h_0().overflow_hidden().children(preview))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_grows_with_its_panes() {
        assert_eq!(window_span(1), BentoSpan::new(2, 2));
        assert_eq!(window_span(2), BentoSpan::new(2, 2));
        assert_eq!(window_span(3), BentoSpan::new(4, 2));
        assert_eq!(window_span(6), BentoSpan::new(4, 3));
    }
}
