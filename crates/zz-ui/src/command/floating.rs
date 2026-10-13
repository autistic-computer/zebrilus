use std::{cell::RefCell, rc::Rc};

use zz_gpui::{
    AnimationElement, App, Bounds, Div, ElementId, Hsla, Keystroke, Pixels, Point, Role,
    SharedString, Size, Stateful, div, point, prelude::*, px, rems, size,
};
use zz_protocol::{MenuItem, MenuState, PopupBorderLines, parse_styled_segments};

use crate::{
    ActiveTheme as _, Colorize as _, ElementExt as _, Elevation, Emphasis, StyledExt as _, Tone,
    h_flex, rems_from_px,
    tmux_style::{split_tmux_alignment, tmux_literal_colour, tmux_styled_segments_text},
    touch::{CoarsePointer, TOUCH_TARGET},
    v_flex,
};

const LIST_PADDING: Pixels = px(4.0);
const ROW_PADDING: Pixels = px(8.0);
const LINE_HEIGHT: Pixels = px(16.0);
const TITLE_HEIGHT: Pixels = px(21.0);
const SEPARATOR_HEIGHT: Pixels = px(5.0);
const WINDOW_MARGIN: Pixels = px(8.0);

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
    let inks = MenuInks {
        rest,
        selected: tmux_literal_colour(&state.selected_style, "bg", cx)
            .map(|fill| (fill, selected_text.unwrap_or(rest))),
        selected_text,
        ground: fill.unwrap_or(theme.surface(Elevation::Popover).fill.opaque()),
        rule: outline.unwrap_or(theme.divider_color()),
    };
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
                .rule_bottom(cx)
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
        .popover_style(cx)
        .when_some(fill, |surface, fill| surface.surface_fill(fill, cx))
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
    let paint = cx.theme().paint(Tone::Neutral, Emphasis::Ghost);
    let (ink, ground) = match (highlighted, inks.selected) {
        (true, Some((fill, text))) => (text, fill),
        (true, None) => (
            inks.selected_text.unwrap_or(paint.selected_text),
            paint.selected,
        ),
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
        .menu_item_corners(row_height, cx)
        .when(highlighted, |row| match inks.selected {
            Some((fill, _)) => row.bg(fill),
            None => row.selection_highlight(cx),
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
    let theme = cx.theme();
    let natural = if CoarsePointer::get(cx) {
        px(TOUCH_TARGET)
    } else {
        px(26.0)
    };
    let rows = state.items.iter().flatten().count();
    if rows == 0 {
        return natural;
    }
    let gaps = if theme.inset_rows() {
        px(2.0) * state.items.len().saturating_sub(1)
    } else {
        Pixels::ZERO
    };
    let fixed = WINDOW_MARGIN * 2.0
        + LIST_PADDING * 2.0
        + if titled { TITLE_HEIGHT } else { Pixels::ZERO }
        + SEPARATOR_HEIGHT * (state.items.len() - rows)
        + gaps;
    let fit = (room.height - fixed) / f32::from(u16::try_from(rows).unwrap_or(u16::MAX));
    natural.min(fit.floor()).max(LINE_HEIGHT)
}

#[must_use]
pub fn menu_hit_cell(state: &MenuState, hits: &MenuHits, at: Point<Pixels>) -> (u16, u16) {
    let column = state.left.saturating_add(1);
    if hits.rows.contains(&at) {
        let index = hits
            .tops
            .iter()
            .rposition(|top| *top <= at.y)
            .unwrap_or_default();
        let index = u16::try_from(index).unwrap_or(u16::MAX);
        return (column, state.top.saturating_add(1).saturating_add(index));
    }
    if hits.surface.contains(&at) {
        return (column, state.top);
    }
    (u16::MAX, 0)
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
    if keystroke.key == "enter" {
        return default_yes && !keystroke.modifiers.platform && !keystroke.modifiers.function;
    }
    if keystroke.modifiers.control || keystroke.modifiers.platform || keystroke.modifiers.function {
        return false;
    }
    keystroke.key_char.as_deref().is_some_and(|value| {
        value.len() == 1 && value.as_bytes().first().copied() == Some(confirm_key)
    })
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
    let cell_width = px(f32::from(u16::try_from(cell_width_px).unwrap_or(u16::MAX)) / scale);
    let cell_height = px(f32::from(u16::try_from(cell_height_px).unwrap_or(u16::MAX)) / scale);
    let grid_width = cell_width * usize::from(client_columns);
    let grid_height = cell_height * usize::from(client_rows);
    let grid_left = ((canvas.width - grid_width) / 2.0).max(Pixels::ZERO);
    let grid_top = ((canvas.height - grid_height) / 2.0).max(Pixels::ZERO);
    FloatingFrame {
        bounds: Bounds::new(
            point(
                origin.x + grid_left + cell_width * usize::from(left),
                origin.y + grid_top + cell_height * usize::from(top),
            ),
            size(
                cell_width * usize::from(width),
                cell_height * usize::from(height),
            ),
        ),
        inset_x: if bordered { cell_width } else { Pixels::ZERO },
        inset_y: if bordered { cell_height } else { Pixels::ZERO },
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
