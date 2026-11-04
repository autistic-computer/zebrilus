use ala::{Div, IntoElement, Length, ParentElement as _, Styled as _, div};

use super::{Space, StyledExt as _};

pub fn stack(gap: Space) -> Div {
    div().v_flex().gap(gap)
}

pub fn inline(gap: Space) -> Div {
    div().h_flex().gap(gap)
}

pub fn inset(inset: Space) -> Div {
    div().p(inset)
}

pub fn split(
    aside: impl Into<Length> + Clone,
    gap: Space,
    side: impl IntoElement,
    main: impl IntoElement,
) -> Div {
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap(gap)
        .child(div().flex_none().w(aside).child(side))
        .child(div().flex_1().min_w_0().child(main))
}

pub fn center(measure: impl Into<Length> + Clone) -> Div {
    div().w_full().max_w(measure).mx_auto()
}

pub fn tiles(columns: u16, gap: Space) -> Div {
    div().grid().grid_cols(columns).gap(gap)
}
