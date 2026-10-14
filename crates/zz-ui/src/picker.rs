use crate::{
    ActiveTheme as _, CHROME_GAP, Colorize as _, Edge, Emphasis, Icon, IconName, Kind, Layer,
    PaintState, Sizable as _, StatefulSurfaceExt as _, StyledExt as _, Surface, SurfaceRole, Tone,
    badge::Badge,
    h_flex,
    input::{Input, InputState},
    roles, v_flex,
};
use zz_gpui::{
    AnimationElement, App, Div, ElementId, Entity, FULL_CORNER_RADIUS, FontWeight, Hsla,
    MouseButton, Pixels, SharedString, Stateful, div, prelude::*, px, relative,
};

const PICKER_ROW_HEIGHT: Pixels = px(26.0);
const HISTORY_ROW_HEIGHT: Pixels = px(52.0);
/// A row `height` tall drawn as `surface` in `state`, with its hover and
/// press while it rests.
pub fn row_surface<E: StatefulInteractiveElement + Styled>(
    element: E,
    surface: impl Into<Surface>,
    state: PaintState,
    height: Pixels,
    cx: &App,
) -> E {
    let surface = surface.into();
    let layer = cx.theme().layer(surface, state);
    crate::row_corners(
        element.surface_interactive(surface, state, cx),
        &layer,
        height,
    )
}

/// Secondary text on what `layer` paints: its ink pulled toward its fill.
pub fn muted_on(layer: &Layer, cx: &App) -> Hsla {
    cx.theme()
        .paint_on(layer.ground, Tone::Neutral, Emphasis::Ghost)
        .muted_at(PaintState::Rest)
}

/// The state of a row the cursor may be on.
pub const fn row_state(selected: bool, enabled: bool) -> PaintState {
    match (enabled, selected) {
        (false, _) => PaintState::Disabled,
        (true, true) => PaintState::Selected,
        (true, false) => PaintState::Rest,
    }
}

/// The radius of a surface drawn as `layer` whose rows sit `inset` inside
/// it, so the two stay concentric. Square and pill shapes keep theirs.
pub fn outer_radius(layer: &Layer, inset: Pixels) -> Pixels {
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
        .surface(roles::PICKER, cx)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    zz_gpui_kit::foundation::surface_enter(surface, (id, "picker-open"), px(0.0))
}

pub fn picker_header(cx: &App) -> Div {
    v_flex()
        .flex_none()
        .gap(px(CHROME_GAP))
        .rule_on(roles::BAR_RULE, Edge::Bottom, cx)
        .p(px(8.0))
}

pub fn picker_search(input: &Entity<InputState>, cx: &App) -> Div {
    picker_search_on(input, roles::PICKER, cx)
}

/// The search field of a picker drawn as `role`, on its ground.
pub fn picker_search_on(input: &Entity<InputState>, role: SurfaceRole, cx: &App) -> Div {
    div().w_full().child(
        Input::new(input)
            .on(cx.theme().ground_of(role))
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
        .rule_on(roles::BAR_RULE, Edge::Top, cx)
        .py(px(CHROME_GAP))
        .pl_4()
        .pr(px(CHROME_GAP))
        .text_size(crate::rems_from_px(10.0))
        .text_color(cx.theme().foreground.muted())
}

pub fn list_row(
    id: impl Into<ElementId>,
    height: Pixels,
    selected: bool,
    enabled: bool,
    cx: &App,
) -> Stateful<Div> {
    row_surface(
        h_flex().id(id).flex_none().h(height),
        Kind::Row.role(),
        row_state(selected, enabled),
        height,
        cx,
    )
}

pub fn picker_row(id: impl Into<ElementId>, selected: bool, cx: &App) -> Stateful<Div> {
    sized_picker_row(id, PICKER_ROW_HEIGHT, selected, cx)
}

fn sized_picker_row(
    id: impl Into<ElementId>,
    height: Pixels,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    list_row(id, height, selected, true, cx)
        .w_full()
        .gap_2()
        .px_2p5()
        .cursor_pointer()
}

pub fn directory_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    cx: &App,
) -> Stateful<Div> {
    path_row(id, IconName::Folder, label, selected, cx)
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

pub fn history_row(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    directory: impl Into<SharedString>,
    updated_at: Option<SharedString>,
    selected: bool,
    current: bool,
    cx: &App,
) -> Stateful<Div> {
    sized_picker_row(id, HISTORY_ROW_HEIGHT, selected, cx).child(
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
                    .text_size(crate::rems_from_px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child(title.into()),
            )
            .child(
                h_flex()
                    .min_w_0()
                    .gap_1()
                    .text_size(crate::rems_from_px(9.0))
                    .opacity(if selected { 1.0 } else { 0.8 })
                    .child(
                        Badge::new(directory)
                            .small()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_size(crate::rems_from_px(9.0)),
                    )
                    .when(current, |row| {
                        row.child(
                            Badge::new("CURRENT")
                                .small()
                                .tone(Tone::Success)
                                .emphasis(Emphasis::Solid),
                        )
                    })
                    .child(div().flex_1())
                    .when_some(updated_at, |row, timestamp| {
                        row.child(
                            div()
                                .flex_none()
                                .max_w(px(112.0))
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(timestamp),
                        )
                    }),
            ),
    )
}
