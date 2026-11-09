use crate::{
    ActiveTheme as _, Colorize as _, Edge, Icon, IconName, Layer, RuleLook, Sizable as _, Size,
    Space, StyledExt as _, Surface,
    components::input::{Input, InputState},
    foundation::roles,
    h_flex, list_row, rems_from_px, v_flex,
};
use ala::{
    AnimationElement, App, Div, ElementId, Entity, FULL_CORNER_RADIUS, MouseButton, Pixels,
    SharedString, Stateful, div, prelude::*, px, relative,
};

pub fn list_row_height(cx: &App) -> Pixels {
    row_px(Size::Large, cx)
}

fn row_px(size: Size, cx: &App) -> Pixels {
    rems_from_px(size.control_px()).to_pixels(cx.theme().font_size)
}

pub(crate) fn outer_radius(layer: &Layer, row_corner: Pixels, inset: Pixels) -> Pixels {
    if layer.radius > px(0.0) && layer.radius < FULL_CORNER_RADIUS {
        row_corner + inset
    } else {
        layer.radius
    }
}

pub fn picker_overlay(id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .p(Space::S16)
        .bg(cx.theme().scrim)
        .occlude()
}

pub fn picker_modal(id: impl Into<ElementId>, cx: &App) -> AnimationElement<Stateful<Div>> {
    picker_modal_sized(id, 660.0, cx)
}

pub fn picker_modal_sized(
    id: impl Into<ElementId>,
    width: f32,
    cx: &App,
) -> AnimationElement<Stateful<Div>> {
    let id = id.into();
    let surface = v_flex()
        .id(id.clone())
        .relative()
        .w(relative(0.92))
        .max_w(px(width))
        .h(relative(0.82))
        .min_h(px(240.0))
        .max_h(px(720.0))
        .overflow_hidden()
        .surface(Surface::new(roles::PICKER), cx)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    crate::foundation::surface_enter(surface, (id, "picker-open"), px(0.0))
}

pub fn picker_header(cx: &App) -> Div {
    v_flex()
        .flex_none()
        .gap(Space::GAP_RELATED)
        .rule(RuleLook::Bar, Edge::Bottom, cx)
        .p(Space::S8)
}

pub fn picker_search(input: &Entity<InputState>, cx: &App) -> Div {
    div().w_full().child(
        Input::new(input)
            .on(cx.theme().ground_of(Surface::new(roles::PICKER)))
            .w_full()
            .min_w_0()
            .prefix(
                Icon::new(IconName::Search)
                    .small()
                    .text_color(cx.theme().foreground.muted()),
            ),
    )
}

pub fn picker_list() -> Div {
    v_flex()
        .relative()
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .p(Space::S8)
}

pub fn picker_empty(label: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .type_style(crate::TextStyle::Caption, cx)
        .text_color(cx.theme().foreground.muted())
        .child(label.into())
}

pub fn picker_footer(cx: &App) -> Div {
    h_flex()
        .w_full()
        .min_h(px(40.0))
        .flex_none()
        .gap(Space::GAP_RELATED)
        .rule(RuleLook::Bar, Edge::Top, cx)
        .py(Space::INSET_CHROME)
        .pl(Space::S16)
        .pr(Space::INSET_CHROME)
        .type_style(crate::TextStyle::Micro, cx)
        .text_color(cx.theme().foreground.muted())
}

pub fn picker_row(id: impl Into<ElementId>, selected: bool, cx: &App) -> Stateful<Div> {
    list_row(id, row_px(Size::Medium, cx), selected, true, cx)
        .w_full()
        .gap(Space::S8)
        .px(Space::S10)
        .type_style(crate::TextStyle::Small, cx)
        .click_cursor()
}

pub fn path_row(
    id: impl Into<ElementId>,
    icon: IconName,
    label: impl Into<SharedString>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    picker_row(id, selected, cx)
        .child(
            Icon::new(icon)
                .small()
                .opacity(if selected { 1.0 } else { 0.8 }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(label.into()),
        )
}
