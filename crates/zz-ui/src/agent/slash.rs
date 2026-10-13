use zz_gpui::{
    App, Div, ElementId, IntoElement, Pixels, SharedString, Stateful, div, prelude::*, px,
};

use crate::{
    ActiveTheme as _, Emphasis, InteractivePaintExt as _, PaintState, StyledExt as _, Tone, h_flex,
    v_flex,
};

const ROW_HEIGHT: f32 = 52.0;
const MAX_VISIBLE_ROWS: u8 = 6;
const LIST_PADDING: f32 = 4.0;

pub fn suggestion_list_height(count: usize) -> Pixels {
    px(ROW_HEIGHT
        * f32::from(
            u8::try_from(count)
                .unwrap_or(MAX_VISIBLE_ROWS)
                .min(MAX_VISIBLE_ROWS),
        ))
}

pub fn suggestion_row(
    id: impl Into<ElementId>,
    name: &str,
    description: Option<SharedString>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    let paint = cx.theme().paint(Tone::Neutral, Emphasis::Ghost);
    let state = if selected {
        PaintState::Selected
    } else {
        PaintState::Rest
    };
    h_flex()
        .id(id)
        .w_full()
        .h(px(ROW_HEIGHT))
        .items_center()
        .gap_3()
        .menu_item_corners(px(ROW_HEIGHT), cx)
        .px_3()
        .cursor_pointer()
        .painted_interactive(paint, selected, true)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_size(crate::rems_from_px(13.0))
                        .font_weight(zz_gpui::FontWeight::MEDIUM)
                        .child(format!("/{name}")),
                )
                .when_some(description, |column, description| {
                    column.child(
                        div()
                            .w_full()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(crate::rems_from_px(12.0))
                            .text_color(paint.muted_at(state))
                            .child(description),
                    )
                }),
        )
}

pub fn suggestion_list(rows: impl IntoElement, cx: &App) -> Div {
    v_flex()
        .w_full()
        .popover_style(cx)
        .list_inset(cx)
        .py(px(LIST_PADDING))
        .overflow_hidden()
        .child(rows)
}
