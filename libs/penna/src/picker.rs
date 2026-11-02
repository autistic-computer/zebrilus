use crate::{
    ActiveTheme as _, CHROME_GAP, Colorize as _, Edge, Icon, IconName, Layer, RuleLook,
    Sizable as _, StyledExt as _, Surface, h_flex,
    input::{Input, InputState},
    list_row, roles, v_flex,
};
use ala::{
    AnimationElement, App, Div, ElementId, Entity, FULL_CORNER_RADIUS, MouseButton, Pixels,
    SharedString, Stateful, div, prelude::*, px, relative,
};

const PICKER_ROW_HEIGHT: Pixels = px(26.0);
pub const LIST_ROW_HEIGHT: Pixels = px(32.0);

pub(crate) fn outer_radius(layer: &Layer, inset: Pixels) -> Pixels {
    if layer.radius > px(0.0) && layer.radius < FULL_CORNER_RADIUS {
        layer.radius + inset
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
        .p_4()
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
        .gap(px(CHROME_GAP))
        .rule(RuleLook::Bar, Edge::Bottom, cx)
        .p(px(8.0))
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
        .p(px(8.0))
}

pub fn picker_empty(label: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .text_size(crate::rems_from_px(11.0))
        .text_color(cx.theme().foreground.muted())
        .child(label.into())
}

pub fn picker_footer(cx: &App) -> Div {
    h_flex()
        .w_full()
        .min_h(px(40.0))
        .flex_none()
        .gap(px(CHROME_GAP))
        .rule(RuleLook::Bar, Edge::Top, cx)
        .py(px(CHROME_GAP))
        .pl_4()
        .pr(px(CHROME_GAP))
        .text_size(crate::rems_from_px(10.0))
        .text_color(cx.theme().foreground.muted())
}

pub fn picker_row(id: impl Into<ElementId>, selected: bool, cx: &App) -> Stateful<Div> {
    list_row(id, PICKER_ROW_HEIGHT, selected, true, cx)
        .w_full()
        .gap_2()
        .px_2p5()
        .cursor_pointer()
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
                .text_size(crate::rems_from_px(12.0))
                .child(label.into()),
        )
}
