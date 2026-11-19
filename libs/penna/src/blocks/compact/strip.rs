use std::rc::Rc;

use ala::{
    AnyElement, App, Div, ElementId, Font, HighlightStyle, InteractiveElement as _, MouseButton,
    ParentElement as _, Pixels, Role, SharedString, Stateful, StatefulInteractiveElement as _,
    Styled as _, StyledText, TextRun, Window, div, font, linear_color_stop, linear_gradient,
    prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Emphasis, Icon, IconName, Layer, PaintState, Shape, StyledExt as _, Surface,
    TextStyle, Tone,
    blocks::touch::{concentric, press_highlight},
    foundation::roles,
    rems_from_px,
};

use super::bar::{COMPACT_PILL_HEIGHT, PillPlacement, floating_layer};

pub const STRIP_INSET: f32 = 4.0;
pub const STRIP_TILE: f32 = 48.0;
const STRIP_GAP: f32 = 4.0;
const MARK: f32 = 20.0;
const MARK_IN: f32 = 14.0;
const TEXT_IN: f32 = 44.0;
pub const STRIP_FACE: f32 = 32.0;
const FACE_IN: f32 = 8.0;
const FACE_TEXT_IN: f32 = 50.0;
const TEXT_END: f32 = 16.0;
const NAME_IN: f32 = 56.0;
const NAME_END: f32 = 12.0;
const SINGLE_IN: f32 = 58.0;
const CAPSULE_MIN: f32 = 88.0;
const CAPSULE_MAX: f32 = 180.0;
const TEXT_SLACK: f32 = 2.0;
const TEXT_FADE: f32 = 0.45;
const STRIP_FADE: f32 = 28.0;
const FADE_BLEED: f32 = 1.0;

pub enum StripMark {
    Glyph(IconName),
    Face(AnyElement),
}

impl StripMark {
    const fn text_in(&self) -> f32 {
        match self {
            Self::Glyph(_) => TEXT_IN,
            Self::Face(_) => FACE_TEXT_IN,
        }
    }
}

pub struct StripItem {
    pub mark: StripMark,
    pub name: SharedString,
    pub word: SharedString,
    pub line: SharedString,
    pub tone: Tone,
}

impl StripItem {
    pub fn new(
        mark: StripMark,
        name: impl Into<SharedString>,
        line: impl Into<SharedString>,
    ) -> Self {
        Self {
            mark,
            name: name.into(),
            word: SharedString::default(),
            line: line.into(),
            tone: Tone::Neutral,
        }
    }

    #[must_use]
    pub fn word(mut self, word: impl Into<SharedString>, tone: Tone) -> Self {
        self.word = word.into();
        self.tone = tone;
        self
    }

    fn second_line(&self) -> SharedString {
        if self.word.is_empty() {
            self.line.clone()
        } else {
            format!("{}{}", self.word, self.line).into()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Fit {
    capsule: f32,
    named: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripSlot {
    pub x: f32,
    pub width: f32,
    pub on: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StripLayout {
    pub slots: Vec<StripSlot>,
    pub fade_start: f32,
    pub fade_end: f32,
}

struct Placed {
    x: Vec<f32>,
    width: Vec<f32>,
    fade_start: bool,
    fade_end: bool,
}

fn near(index: usize, position: f32) -> f32 {
    (1.0 - (index as f32 - position).abs()).max(0.0)
}

fn place(inner: f32, hug: &[f32], small: &[f32], active: usize) -> Placed {
    let count = hug.len();
    let unit = STRIP_TILE + STRIP_GAP;
    let half = STRIP_TILE / 2.0 + STRIP_GAP;
    let others: f32 = small
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != active)
        .map(|(_, width)| width + STRIP_GAP)
        .sum();
    if hug[active] + others <= inner {
        let mut width = small.to_vec();
        width[active] = inner - others;
        let mut at = 0.0;
        let x = width
            .iter()
            .map(|width| {
                let x = at;
                at += width + STRIP_GAP;
                x
            })
            .collect();
        return Placed {
            x,
            width,
            fade_start: false,
            fade_end: false,
        };
    }
    let before = active;
    let after = count - 1 - active;
    let need = |left: usize, right: usize| {
        hug[active]
            + unit * (left + right) as f32
            + if before > left { half } else { 0.0 }
            + if after > right { half } else { 0.0 }
    };
    let (mut left, mut right) = (0, 0);
    loop {
        let grew_right = after > right && need(left, right + 1) <= inner;
        if grew_right {
            right += 1;
        }
        let grew_left = before > left && need(left + 1, right) <= inner;
        if grew_left {
            left += 1;
        }
        if !grew_right && !grew_left {
            break;
        }
    }
    let lead = if before > left { half } else { 0.0 };
    let trail = if after > right { half } else { 0.0 };
    let capsule = inner - lead - trail - unit * (left + right) as f32;
    let start = lead + unit * left as f32;
    let end = start + capsule;
    let x = (0..count)
        .map(|index| match index.cmp(&active) {
            std::cmp::Ordering::Less => start - unit * (active - index) as f32,
            std::cmp::Ordering::Equal => start,
            std::cmp::Ordering::Greater => end + STRIP_GAP + unit * (index - active - 1) as f32,
        })
        .collect();
    let mut width = vec![STRIP_TILE; count];
    width[active] = capsule;
    Placed {
        x,
        width,
        fade_start: before > left,
        fade_end: after > right,
    }
}

#[must_use]
pub fn strip_layout(inner: f32, hug: &[f32], small: &[f32], position: f32) -> StripLayout {
    let count = hug.len();
    if count == 0 {
        return StripLayout {
            slots: Vec::new(),
            fade_start: 0.0,
            fade_end: 0.0,
        };
    }
    let position = position.clamp(0.0, (count - 1) as f32);
    let from = position.floor() as usize;
    let to = position.ceil() as usize;
    let at = position - from as f32;
    let a = place(inner, hug, small, from);
    let b = place(inner, hug, small, to);
    let mix = |from: f32, to: f32| from + (to - from) * at;
    let flag = |from: bool, to: bool| mix(f32::from(u8::from(from)), f32::from(u8::from(to)));
    StripLayout {
        slots: (0..count)
            .map(|index| StripSlot {
                x: mix(a.x[index], b.x[index]),
                width: mix(a.width[index], b.width[index]),
                on: near(index, position),
            })
            .collect(),
        fade_start: flag(a.fade_start, b.fade_start),
        fade_end: flag(a.fade_end, b.fade_end),
    }
}

fn text_font(style: TextStyle, strong: bool, cx: &App) -> Font {
    Font {
        weight: if strong {
            style.strong_weight()
        } else {
            style.weight()
        },
        ..font(cx.theme().font_family.clone())
    }
}

fn text_width(
    text: &SharedString,
    style: TextStyle,
    strong: bool,
    window: &Window,
    cx: &App,
) -> f32 {
    let run = TextRun {
        len: text.len(),
        font: text_font(style, strong, cx),
        color: cx.theme().foreground,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let size = style.size().to_pixels(window.rem_size());
    f32::from(
        window
            .text_system()
            .shape_line(text.clone(), size, &[run], None)
            .width,
    )
}

fn fit(item: &StripItem, window: &Window, cx: &App) -> Fit {
    let name = text_width(&item.name, TextStyle::TouchSmall, true, window, cx) + TEXT_SLACK;
    let line = text_width(&item.second_line(), TextStyle::Small, false, window, cx) + TEXT_SLACK;
    Fit {
        capsule: (item.mark.text_in() + name.max(line) + TEXT_END).clamp(CAPSULE_MIN, CAPSULE_MAX),
        named: NAME_IN + name + NAME_END,
    }
}

fn second_line(item: &StripItem, ground: crate::Ground, cx: &App) -> StyledText {
    let theme = cx.theme();
    let toned = item.tone != Tone::Neutral && !item.word.is_empty();
    let word = theme.paint_on(ground, item.tone, Emphasis::Soft).text;
    StyledText::new(item.second_line()).with_highlights(toned.then(|| {
        (
            0..item.word.len(),
            HighlightStyle {
                color: Some(word),
                ..HighlightStyle::default()
            },
        )
    }))
}

fn lines(item: &StripItem, ground: crate::Ground, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .justify_center()
        .gap(crate::Space::S2)
        .min_w_0()
        .child(
            div()
                .overflow_hidden()
                .whitespace_nowrap()
                .type_style_strong(TextStyle::TouchSmall, cx)
                .child(item.name.clone()),
        )
        .child(
            div()
                .overflow_hidden()
                .whitespace_nowrap()
                .type_style(TextStyle::Small, cx)
                .text_color(
                    cx.theme()
                        .paint(Tone::Neutral, Emphasis::Ghost)
                        .muted_at(PaintState::Rest),
                )
                .child(second_line(item, ground, cx)),
        )
}

#[allow(clippy::too_many_lines)]
pub fn pill_strip(
    id: impl Into<ElementId>,
    items: Vec<StripItem>,
    position: f32,
    width: Pixels,
    placement: &PillPlacement,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
    window: &Window,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let pill = floating_layer(placement, cx);
    let backing = theme.layer(
        Surface::new(roles::PHONE_PILL_ICON).on(pill.ground),
        PaintState::Rest,
    );
    let radius = if theme.style(roles::PHONE_PILL_ICON).shape == Shape::Round {
        concentric(pill.radius, px(STRIP_INSET))
    } else {
        backing.radius
    };
    let pressed = theme
        .layer(Surface::new(roles::PHONE_PILL), PaintState::Active)
        .fill;
    let muted = theme
        .paint_on(pill.ground, Tone::Neutral, Emphasis::Ghost)
        .muted_at(PaintState::Rest);
    let ink = pill.text;
    let inner = f32::from(width) - 2.0 * STRIP_INSET;
    let id = id.into();
    let on_pick = Rc::new(on_pick);
    let circle = |left: f32, fill: f32| {
        div()
            .absolute()
            .top_0()
            .left(px(left))
            .w(px(fill))
            .h(px(STRIP_TILE))
            .layer(&Layer {
                radius,
                ..backing.clone()
            })
    };
    let mark = |mark: StripMark, color| match mark {
        StripMark::Glyph(icon) => div()
            .absolute()
            .top(px(MARK_IN))
            .left(px(MARK_IN))
            .size(px(MARK))
            .flex()
            .items_center()
            .justify_center()
            .text_color(color)
            .child(Icon::new(icon).size(rems_from_px(MARK))),
        StripMark::Face(face) => div()
            .absolute()
            .top(px(FACE_IN))
            .left(px(FACE_IN))
            .size(px(STRIP_FACE))
            .child(face),
    };
    let fits: Vec<Fit> = items.iter().map(|item| fit(item, window, cx)).collect();
    let capsules: Vec<f32> = fits.iter().map(|fit| fit.capsule).collect();
    let named: Vec<f32> = fits.iter().map(|fit| fit.named).collect();
    let roomy = items.len() == 2
        && capsules[0] + STRIP_GAP + named[1] <= inner
        && capsules[1] + STRIP_GAP + named[0] <= inner;
    let small = if roomy {
        named.clone()
    } else {
        vec![STRIP_TILE; items.len()]
    };
    let layout = strip_layout(inner, &capsules, &small, position);
    let end = if pill.radius * 2.0 >= px(COMPACT_PILL_HEIGHT) {
        px(STRIP_TILE / 2.0)
    } else {
        concentric(pill.radius, px(STRIP_INSET))
    };
    let edge = |start: bool, shown: f32| {
        div()
            .absolute()
            .top_0()
            .w(px(STRIP_FADE + FADE_BLEED))
            .h_full()
            .when(start, ala::Styled::left_0)
            .when(!start, ala::Styled::right_0)
            .opacity(shown)
            .bg(linear_gradient(
                if start { 90.0 } else { 270.0 },
                linear_color_stop(pill.fill, 0.0),
                linear_color_stop(pill.fill.alpha(0.0), 1.0),
            ))
    };
    let track = div()
        .absolute()
        .top(px(STRIP_INSET))
        .left(px(STRIP_INSET))
        .w(px(inner))
        .h(px(STRIP_TILE))
        .rounded(end)
        .overflow_hidden();
    let ground = pill.ground;
    let mut items = items;
    let track = if items.len() == 1 {
        let item = items.remove(0);
        let text_in = match item.mark {
            StripMark::Glyph(_) => SINGLE_IN,
            StripMark::Face(_) => FACE_TEXT_IN,
        };
        let text = lines(&item, ground, cx)
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(text_in))
            .w(px(inner - text_in - NAME_END));
        track.child(mark(item.mark, ink)).child(text)
    } else {
        track.children(
            items
                .into_iter()
                .zip(&layout.slots)
                .zip(&capsules)
                .enumerate()
                .map(|(index, ((item, slot), capsule))| {
                    let text = ((slot.on - (1.0 - TEXT_FADE)) / TEXT_FADE).clamp(0.0, 1.0);
                    let name = ((TEXT_FADE - slot.on) / TEXT_FADE).clamp(0.0, 1.0);
                    let fill = STRIP_TILE + (slot.width - STRIP_TILE) * slot.on;
                    let label = if slot.on > 0.5 {
                        SharedString::from(format!("{}, {}", item.name, item.second_line()))
                    } else {
                        item.name.clone()
                    };
                    let text_in = item.mark.text_in();
                    let copy = (text > 0.0).then(|| {
                        lines(&item, ground, cx)
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px(text_in))
                            .w(px(capsule.max(slot.width) - text_in - TEXT_END))
                            .text_color(ink)
                            .opacity(text)
                    });
                    let other = (roomy && name > 0.0).then(|| {
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px(NAME_IN))
                            .flex()
                            .items_center()
                            .whitespace_nowrap()
                            .type_style_strong(TextStyle::TouchSmall, cx)
                            .text_color(muted)
                            .opacity(name)
                            .child(item.name.clone())
                    });
                    let on_pick = Rc::clone(&on_pick);
                    div()
                        .id((id.clone(), format!("page-{index}")))
                        .absolute()
                        .top_0()
                        .left(px(slot.x))
                        .w(px(slot.width))
                        .h(px(STRIP_TILE))
                        .overflow_hidden()
                        .click_cursor()
                        .role(Role::Button)
                        .aria_label(label)
                        .child(circle(0.0, fill).opacity(slot.on))
                        .child(
                            press_highlight(
                                (id.clone(), format!("press-{index}")),
                                pressed,
                                radius,
                            )
                            .w(px(fill)),
                        )
                        .child(mark(item.mark, if slot.on > 0.5 { ink } else { muted }))
                        .children(copy)
                        .children(other)
                        .on_click(move |_, window, cx| on_pick(index, window, cx))
                }),
        )
    };
    let fades = (layout.fade_start > 0.0 || layout.fade_end > 0.0).then(|| {
        div()
            .absolute()
            .top(px(STRIP_INSET - FADE_BLEED))
            .left(px(STRIP_INSET - FADE_BLEED))
            .w(px(inner + 2.0 * FADE_BLEED))
            .h(px(STRIP_TILE + 2.0 * FADE_BLEED))
            .rounded(end + px(FADE_BLEED))
            .overflow_hidden()
            .when(layout.fade_start > 0.0, |fades| {
                fades.child(edge(true, layout.fade_start))
            })
            .when(layout.fade_end > 0.0, |fades| {
                fades.child(edge(false, layout.fade_end))
            })
    });
    div()
        .id(id.clone())
        .debug_selector(|| "compact-bar-pill".to_owned())
        .relative()
        .flex_1()
        .min_w_0()
        .h(rems_from_px(COMPACT_PILL_HEIGHT))
        .font_family(theme.font_family.clone())
        .text_color(ink)
        .layer(&pill)
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
        .child(track)
        .children(fades)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capsule_fills_the_row_and_an_overflowing_edge_cuts_an_icon_in_half() {
        let row = |hug: &[f32], position: f32| {
            let layout = strip_layout(234.0, hug, &vec![48.0; hug.len()], position);
            (
                layout
                    .slots
                    .iter()
                    .map(|slot| (slot.x, slot.width))
                    .collect::<Vec<_>>(),
                layout.fade_start,
                layout.fade_end,
            )
        };
        assert_eq!(
            row(&[100.0, 88.0, 96.0], 0.0),
            (vec![(0.0, 130.0), (134.0, 48.0), (186.0, 48.0)], 0.0, 0.0)
        );
        assert_eq!(
            row(&[100.0, 88.0, 96.0], 0.5),
            (vec![(0.0, 89.0), (93.0, 89.0), (186.0, 48.0)], 0.0, 0.0)
        );
        let six = [160.0, 88.0, 120.0, 100.0, 96.0, 110.0];
        assert_eq!(
            row(&six, 0.0),
            (
                vec![
                    (0.0, 206.0),
                    (210.0, 48.0),
                    (262.0, 48.0),
                    (314.0, 48.0),
                    (366.0, 48.0),
                    (418.0, 48.0)
                ],
                0.0,
                1.0
            )
        );
        assert_eq!(
            row(&six, 2.0),
            (
                vec![
                    (-76.0, 48.0),
                    (-24.0, 48.0),
                    (28.0, 126.0),
                    (158.0, 48.0),
                    (210.0, 48.0),
                    (262.0, 48.0)
                ],
                1.0,
                1.0
            )
        );
    }
}
