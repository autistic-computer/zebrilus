use std::{cell::RefCell, rc::Rc};

use zz_gpui::{
    AnimationElement, App, Bounds, Div, ElementId, Hsla, Keystroke, Pixels, Point, Role,
    SharedString, Size, Stateful, div, point, prelude::*, px, rems, size,
};
use zz_protocol::{MenuItem, MenuState, PopupBorderLines, parse_styled_segments};

use crate::{
    ActiveTheme as _, Colorize as _, Edge, ElementExt as _, PaintState, StyledExt as _,
    SurfaceRole, h_flex, rems_from_px, roles, row_corners,
    tmux_style::{split_tmux_alignment, tmux_literal_colour, tmux_styled_segments_text},
    touch::{CoarsePointer, TOUCH_TARGET},
    v_flex,
};

const LIST_PADDING: Pixels = px(zz_client::MENU_LIST_PADDING);
const ROW_PADDING: Pixels = px(8.0);
const LINE_HEIGHT: Pixels = px(zz_client::MENU_LINE_HEIGHT);
const TITLE_HEIGHT: Pixels = px(zz_client::MENU_TITLE_HEIGHT);
const WINDOW_MARGIN: Pixels = px(zz_client::MENU_WINDOW_MARGIN);

#[derive(Debug, Default)]
pub struct MenuHits {
    pub surface: Bounds<Pixels>,
    pub rows: Bounds<Pixels>,
    pub tops: Vec<Pixels>,
}

struct MenuInks {
    rest: Hsla,
    selected: Option<(Hsla, Hsla)>,
    selected_text: Option<Hsla>,
    ground: Hsla,
    rule: Hsla,
}

pub fn display_menu(
    state: &MenuState,
    selected: Option<usize>,
    room: Size<Pixels>,
    hits: &Rc<RefCell<MenuHits>>,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let coarse = CoarsePointer::get(cx);
    let titled = state.border_lines != PopupBorderLines::None && !state.title.is_empty();
    let outlined = !matches!(
        state.border_lines,
        PopupBorderLines::None | PopupBorderLines::Padded
    );
    let fill = tmux_literal_colour(&state.style, "bg", cx);
    let text = tmux_literal_colour(&state.style, "fg", cx);
    let outline = tmux_literal_colour(&state.border_style, "fg", cx);
    let selected_text = tmux_literal_colour(&state.selected_style, "fg", cx);
    let rest = text.unwrap_or(theme.foreground);
    let mut surface = theme.layer(SurfaceRole::POPOVER, PaintState::Rest);
    let inks = MenuInks {
        rest,
        selected: tmux_literal_colour(&state.selected_style, "bg", cx)
            .map(|fill| (fill, selected_text.unwrap_or(rest))),
        selected_text,
        ground: fill.unwrap_or(surface.ground.fill),
        rule: outline.unwrap_or(theme.divider_color()),
    };
    if let Some(fill) = fill {
        match surface.glass {
            Some(material) => {
                surface.glass = Some(material.tint(fill.alpha(material.tint.a)).fallback(fill));
            }
            None => surface.fill = fill,
        }
    }
    let row_height = menu_row_height(state, titled, room, cx);
    hits.borrow_mut()
        .tops
        .resize(state.items.len(), Pixels::ZERO);
    let surface_hits = hits.clone();
    let rows_hits = hits.clone();
    let rows = state.items.iter().enumerate().map(|(index, item)| {
        let tops = hits.clone();
        let record = move |bounds: Bounds<Pixels>, _: &mut zz_gpui::Window, _: &mut App| {
            if let Some(top) = tops.borrow_mut().tops.get_mut(index) {
                *top = bounds.top();
            }
        };
        match item {
            Some(item) => menu_row(index, item, selected == Some(index), row_height, &inks, cx)
                .on_prepaint(record)
                .into_any_element(),
            None => div()
                .flex_none()
                .my(px(2.0))
                .mx(theme.row_inset)
                .rule_on(roles::LIST_RULE, Edge::Bottom, cx)
                .border_color(inks.rule)
                .on_prepaint(record)
                .into_any_element(),
        }
    });
    let list = v_flex()
        .pt(if !titled {
            LIST_PADDING
        } else if theme.inset_rows() {
            px(2.0)
        } else {
            Pixels::ZERO
        })
        .pb(LIST_PADDING)
        .list_inset(cx)
        .on_prepaint(move |bounds, _, _| rows_hits.borrow_mut().rows = bounds)
        .children(rows);
    v_flex()
        .id("display-menu")
        .debug_selector(|| "display-menu".to_owned())
        .role(Role::Menu)
        .relative()
        .occlude()
        .layer(&surface)
        .when_some(text, Styled::text_color)
        .when_some(outline.filter(|_| outlined), |surface, outline| {
            literal_edge(surface, outline, cx)
        })
        .when(!outlined, Styled::border_0)
        .min_w(rems(8.0))
        .max_w(
            px(500.0)
                .min(room.width - WINDOW_MARGIN * 2.0)
                .max(px(128.0)),
        )
        .font_family(theme.font_family.clone())
        .text_size(rems_from_px(if coarse { 13.0 } else { 12.0 }))
        .line_height(LINE_HEIGHT)
        .on_prepaint(move |bounds, _, _| surface_hits.borrow_mut().surface = bounds)
        .when(titled, |surface| {
            surface.child(menu_title(&state.title, &inks, cx))
        })
        .child(list)
}

#[allow(
    clippy::disallowed_methods,
    reason = "a literal popup-border-style colour the user configured wins over the look's edge"
)]
fn literal_edge(surface: Stateful<Div>, color: Hsla, cx: &App) -> Stateful<Div> {
    surface
        .border(cx.theme().outline.max(px(1.0)))
        .border_color(color)
}

pub fn menu_enter(surface: Stateful<Div>) -> AnimationElement<Stateful<Div>> {
    zz_gpui_kit::foundation::surface_enter(surface, "display-menu-open", Pixels::ZERO)
}

fn menu_row(
    index: usize,
    item: &MenuItem,
    selected: bool,
    row_height: Pixels,
    inks: &MenuInks,
    cx: &App,
) -> Stateful<Div> {
    let highlighted = selected && item.enabled;
    let layer = cx.theme().layer(
        SurfaceRole::MENU_ITEM,
        if highlighted {
            PaintState::Selected
        } else {
            PaintState::Rest
        },
    );
    let (ink, ground) = match (highlighted, inks.selected) {
        (true, Some((fill, text))) => (text, fill),
        (true, None) => (inks.selected_text.unwrap_or(layer.text), layer.ground.fill),
        (false, _) if item.enabled => (inks.rest, inks.ground),
        (false, _) => (inks.rest.muted(), inks.ground),
    };
    let name = tmux_styled_segments_text(&parse_styled_segments(&item.name), ink, ground, cx);
    h_flex()
        .id(("display-menu-row", index))
        .debug_selector(move || format!("display-menu-row-{index}"))
        .role(Role::MenuItem)
        .aria_selected(highlighted)
        .flex_none()
        .h(row_height)
        .px(ROW_PADDING)
        .gap_3()
        .justify_between()
        .layer(&layer)
        .map(|row| row_corners(row, &layer, row_height))
        .when_some(inks.selected.filter(|_| highlighted), |row, (fill, _)| {
            row.bg(fill)
        })
        .text_color(ink)
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(name.into_styled_text()),
        )
        .when_some(item.annotation.clone(), |row, key| {
            row.child(
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_xs()
                    .text_color(ink)
                    .opacity(if highlighted { 1.0 } else { 0.8 })
                    .child(key),
            )
        })
}

fn menu_title(title: &str, inks: &MenuInks, cx: &App) -> Div {
    let ink = inks.rest.muted();
    let [left, centre, right] = split_tmux_alignment(title).map(|segments| {
        (!segments.is_empty())
            .then(|| tmux_styled_segments_text(&segments, ink, inks.ground, cx).into_styled_text())
    });
    h_flex()
        .flex_none()
        .h(TITLE_HEIGHT)
        .pt(LIST_PADDING)
        .px(cx.theme().row_inset + ROW_PADDING)
        .text_size(rems_from_px(11.0))
        .text_color(ink)
        .whitespace_nowrap()
        .overflow_hidden()
        .child(h_flex().flex_grow_1().children(left))
        .children(centre)
        .child(h_flex().flex_grow_1().justify_end().children(right))
}

fn menu_row_height(state: &MenuState, titled: bool, room: Size<Pixels>, cx: &App) -> Pixels {
    let natural = if CoarsePointer::get(cx) {
        TOUCH_TARGET
    } else {
        26.0
    };
    px(zz_client::menu_row_height(
        state,
        titled,
        f32::from(room.height),
        natural,
        cx.theme().inset_rows(),
    ))
}

#[must_use]
pub fn menu_hit_cell(state: &MenuState, hits: &MenuHits, at: Point<Pixels>) -> (u16, u16) {
    let rect = |bounds: Bounds<Pixels>| zz_client::floating::PixelRect {
        x: f32::from(bounds.origin.x),
        y: f32::from(bounds.origin.y),
        width: f32::from(bounds.size.width),
        height: f32::from(bounds.size.height),
    };
    let tops: Vec<f32> = hits.tops.iter().map(|top| f32::from(*top)).collect();
    zz_client::menu_hit_cell(
        state,
        rect(hits.surface),
        rect(hits.rows),
        &tops,
        (f32::from(at.x), f32::from(at.y)),
    )
}

pub fn confirm_prompt(
    id: impl Into<ElementId>,
    prompt: impl Into<SharedString>,
    font_family: impl Into<SharedString>,
) -> Stateful<Div> {
    div()
        .id(id)
        .size_full()
        .flex()
        .items_center()
        .px(px(12.0))
        .font_family(font_family.into())
        .text_size(px(13.0))
        .line_height(px(16.0))
        .child(prompt.into())
}

pub fn confirm_accepts(confirm_key: u8, default_yes: bool, keystroke: &Keystroke) -> bool {
    zz_client::confirm_accepts(
        confirm_key,
        default_yes,
        &keystroke.key,
        keystroke.key_char.as_deref(),
        (
            keystroke.modifiers.control,
            keystroke.modifiers.platform,
            keystroke.modifiers.function,
        ),
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatingFrame {
    pub bounds: Bounds<Pixels>,
    pub inset_x: Pixels,
    pub inset_y: Pixels,
}

pub fn floating_frame(
    left: u16,
    top: u16,
    width: u16,
    height: u16,
    client_columns: u16,
    client_rows: u16,
    cell_width_px: u32,
    cell_height_px: u32,
    bordered: bool,
    origin: Point<Pixels>,
    canvas: Size<Pixels>,
    scale: f32,
) -> FloatingFrame {
    let frame = zz_client::floating::overlay_frame(
        (left, top, width, height),
        (client_columns, client_rows),
        (cell_width_px, cell_height_px),
        bordered,
        (f32::from(canvas.width), f32::from(canvas.height)),
        scale,
    );
    FloatingFrame {
        bounds: Bounds::new(
            point(origin.x + px(frame.bounds.x), origin.y + px(frame.bounds.y)),
            size(px(frame.bounds.width), px(frame.bounds.height)),
        ),
        inset_x: px(frame.inset_x),
        inset_y: px(frame.inset_y),
    }
}

pub const RELEASE_BUTTONS: u8 = 3;
pub const WHEEL_BUTTONS: u8 = 64;

pub const fn menu_press_buttons(button: zz_gpui::MouseButton) -> u8 {
    match button {
        zz_gpui::MouseButton::Left => 0,
        zz_gpui::MouseButton::Middle => 1,
        zz_gpui::MouseButton::Right => 2,
        zz_gpui::MouseButton::Navigate(_) => 128,
    }
}

#[cfg(test)]
mod tests {
    use zz_client::MenuBox;
    use zz_gpui::{Bounds, point, px, size};
    use zz_protocol::{MenuItem, MenuState, PopupBorderLines};

    use super::{MenuHits, menu_hit_cell};

    #[test]
    fn a_pointer_lands_on_the_cell_of_the_row_above_it_and_the_title_is_the_border() {
        let item = |name: &str| {
            Some(MenuItem {
                name: name.to_owned(),
                key: None,
                annotation: None,
                enabled: true,
            })
        };
        let state = MenuState {
            left: 10,
            top: 4,
            width: 12,
            height: 5,
            client_columns: 80,
            client_rows: 24,
            cell_width_px: 8,
            cell_height_px: 18,
            title: "Pane".to_owned(),
            style: String::new(),
            selected_style: String::new(),
            border_style: String::new(),
            border_lines: PopupBorderLines::Single,
            items: vec![item("Split"), None, item("Kill")],
            selected: None,
            stay_open: false,
            mouse_keys: true,
        };
        let hits = MenuHits {
            surface: Bounds::new(point(px(100.0), px(50.0)), size(px(160.0), px(90.0))),
            rows: Bounds::new(point(px(100.0), px(71.0)), size(px(160.0), px(69.0))),
            tops: vec![px(75.0), px(103.0), px(108.0)],
        };
        let frame = MenuBox {
            left: state.left,
            top: state.top,
            width: state.width,
            items: state.items.len(),
        };
        let at = |x: f32, y: f32| menu_hit_cell(&state, &hits, point(px(x), px(y)));

        assert_eq!(at(120.0, 80.0), (11, 5));
        assert_eq!(at(120.0, 72.0), (11, 5));
        assert_eq!(at(120.0, 102.0), (11, 5));
        assert_eq!(at(120.0, 104.0), (11, 6));
        assert_eq!(at(120.0, 120.0), (11, 7));
        assert_eq!(at(120.0, 60.0), (11, 4));
        let (column, row) = at(120.0, 60.0);
        assert!(!frame.contains(column, row));
        let (column, row) = at(120.0, 139.0);
        assert!(frame.contains(column, row));
        assert_eq!(at(90.0, 80.0), (u16::MAX, 0));
        let (column, row) = at(90.0, 80.0);
        assert!(!frame.contains(column, row));
    }
}
