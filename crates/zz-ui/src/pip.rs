use std::collections::{BTreeMap, BTreeSet, VecDeque};

use web_time::{Duration, Instant};
use zz_client::floating::{FloatCells, FloatPlace};
use zz_gpui::{
    AnyElement, App, Bounds, ClickEvent, ElementId, GlassMaterial, Hsla, IntoElement,
    ParentElement as _, Pixels, Point, Size, Styled as _, Window, div, point, prelude::*, px,
};
use zz_protocol::{PaneBorderLines, PaneId, WindowId, WindowSnapshot};

use crate::{ActiveTheme as _, Colorize as _, PaintState, StyledExt as _, roles};

const GROUP: &str = "pip";
const MARGIN: f32 = 12.0;
const FROST: f32 = 24.0;
const STACK_DEPTH: usize = 3;
const PEEK: f32 = 60.0;
const VELOCITY_WINDOW: Duration = Duration::from_millis(100);
const FRAME: Duration = Duration::from_millis(16);
const DECELERATION: f32 = 0.998;

#[derive(Clone, Debug, Default)]
pub struct PipThrow {
    samples: VecDeque<(Instant, Point<f32>)>,
}

impl PipThrow {
    pub fn track(&mut self, at: Instant, pointer: Point<Pixels>) {
        self.samples
            .push_back((at, point(f32::from(pointer.x), f32::from(pointer.y))));
        while self
            .samples
            .front()
            .is_some_and(|(sample, _)| at.saturating_duration_since(*sample) > VELOCITY_WINDOW)
        {
            self.samples.pop_front();
        }
    }

    #[must_use]
    pub fn velocity(&self, now: Instant) -> Point<f32> {
        let mut recent = self
            .samples
            .iter()
            .filter(|(at, _)| now.saturating_duration_since(*at) <= VELOCITY_WINDOW);
        let Some(&(first_at, first)) = recent.next() else {
            return Point::default();
        };
        let &(last_at, last) = recent.next_back().unwrap_or(&(first_at, first));
        let span = last_at
            .saturating_duration_since(first_at)
            .max(FRAME)
            .as_secs_f32();
        point((last.x - first.x) / span, (last.y - first.y) / span)
    }
}

#[must_use]
pub fn projected(position: Point<Pixels>, velocity: Point<f32>) -> Point<Pixels> {
    let reach = DECELERATION / (1.0 - DECELERATION) / 1000.0;
    point(
        position.x + px(velocity.x * reach),
        position.y + px(velocity.y * reach),
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PipLayout {
    pub window: (u16, u16),
    pub cell: Size<Pixels>,
    pub canvas: Size<Pixels>,
    pub lines: PaneBorderLines,
    pub header: Pixels,
}

impl PipLayout {
    fn bordered(&self) -> bool {
        self.lines != PaneBorderLines::None
    }

    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "window cell offsets are far below f32's exact integer range"
    )]
    pub fn size(&self, cells: FloatCells) -> Size<Pixels> {
        let (width, height) = (
            self.cell.width * f32::from(cells.sx),
            self.cell.height * f32::from(cells.sy),
        );
        if self.bordered() {
            zz_gpui::size(
                width + self.cell.width * 2.0,
                height + self.header.max(self.cell.height) + self.cell.height,
            )
        } else {
            zz_gpui::size(width, height)
        }
    }

    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "window cell offsets are far below f32's exact integer range"
    )]
    pub fn frame(&self, cells: FloatCells) -> Bounds<Pixels> {
        let size = self.size(cells);
        let area = self.area();
        let pad = i32::from(self.bordered());
        let axis = |offset: i32, extent: u16, window: u16, frame: Pixels, canvas: Pixels, cell| {
            if window == 0 {
                return cell * (offset - pad) as f32;
            }
            let free = canvas - frame;
            if free <= Pixels::ZERO {
                return Pixels::ZERO;
            }
            let room = f32::from(window) - f32::from(extent) - 2.0 * pad as f32;
            let along = if room > 0.0 {
                ((offset - pad) as f32 / room).clamp(0.0, 1.0)
            } else {
                0.5
            };
            let margin = px(MARGIN).min(free / 2.0);
            margin + (free - margin * 2.0) * along
        };
        Bounds::new(
            point(
                axis(
                    cells.xoff,
                    cells.sx,
                    self.window.0,
                    size.width,
                    area.width,
                    self.cell.width,
                ),
                axis(
                    cells.yoff,
                    cells.sy,
                    self.window.1,
                    size.height,
                    area.height,
                    self.cell.height,
                ),
            ),
            size,
        )
    }

    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the cell count is floored and clamped to u16 before the cast"
    )]
    pub fn max_cells(&self) -> (u16, u16) {
        let area = self.area();
        let chrome = self.size(FloatCells {
            xoff: 0,
            yoff: 0,
            sx: 0,
            sy: 0,
        });
        let pad = if self.bordered() { 2 } else { 0 };
        let fit = |room: Pixels, chrome: Pixels, cell: Pixels, window: u16| {
            let fits = if cell > Pixels::ZERO {
                ((room - px(MARGIN) * 2.0 - chrome) / cell)
                    .floor()
                    .clamp(1.0, f32::from(u16::MAX)) as u16
            } else {
                u16::MAX
            };
            if window == 0 {
                fits
            } else {
                fits.min(window.saturating_sub(pad).max(1))
            }
        };
        (
            fit(area.width, chrome.width, self.cell.width, self.window.0),
            fit(area.height, chrome.height, self.cell.height, self.window.1),
        )
    }

    fn area(&self) -> Size<Pixels> {
        if self.canvas.width > Pixels::ZERO && self.canvas.height > Pixels::ZERO {
            self.canvas
        } else {
            zz_gpui::size(
                self.cell.width * f32::from(self.window.0),
                self.cell.height * f32::from(self.window.1),
            )
        }
    }

    fn place_at(&self, centre: Point<Pixels>) -> FloatPlace {
        let area = self.area();
        let fraction = |at: Pixels, extent: Pixels, cells: u16| {
            if extent > Pixels::ZERO {
                at / extent * f32::from(cells)
            } else {
                0.0
            }
        };
        FloatPlace::zone(
            (
                fraction(centre.x, area.width, self.window.0),
                fraction(centre.y, area.height, self.window.1),
            ),
            self.window,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PipDrag {
    pub start: FloatCells,
    layout: PipLayout,
    press: Point<Pixels>,
    pointer: Point<Pixels>,
    origin: Point<Pixels>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PipLanding {
    pub place: FloatPlace,
    pub target: FloatCells,
    pub stash: Option<PipSide>,
}

impl PipDrag {
    #[must_use]
    pub fn new(
        start: FloatCells,
        layout: PipLayout,
        press: Point<Pixels>,
        offset: Point<Pixels>,
    ) -> Self {
        Self {
            start,
            layout,
            press,
            pointer: press,
            origin: layout.frame(start).origin + offset,
        }
    }

    pub fn moved(&mut self, pointer: Point<Pixels>) {
        self.pointer = pointer;
    }

    #[must_use]
    pub fn drawn_origin(&self) -> Point<Pixels> {
        self.origin + (self.pointer - self.press)
    }

    #[must_use]
    pub fn frame(&self, cells: FloatCells) -> Bounds<Pixels> {
        self.layout.frame(cells)
    }

    #[must_use]
    pub fn landing(&self, velocity: Point<f32>) -> FloatPlace {
        let size = self.layout.size(self.start);
        let landing = projected(self.drawn_origin(), velocity);
        self.layout.place_at(point(
            landing.x + size.width / 2.0,
            landing.y + size.height / 2.0,
        ))
    }

    #[must_use]
    pub fn release(&self, velocity: Point<f32>) -> PipLanding {
        let place = self.landing(velocity);
        let target = place.cells(self.start, self.layout.window, self.layout.lines);
        let centre = self.drawn_origin().x + self.layout.size(self.start).width / 2.0;
        let stash = if centre < Pixels::ZERO {
            Some(PipSide::Left)
        } else if centre > self.layout.area().width {
            Some(PipSide::Right)
        } else {
            None
        };
        PipLanding {
            place,
            target,
            stash,
        }
    }
}

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PipCard {
    pub pane: PaneId,
    pub frame: Bounds<Pixels>,
    pub held: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PipPlaced {
    pub pane: PaneId,
    pub frame: Bounds<Pixels>,
    pub stashed: Option<PipSide>,
}

#[derive(Clone, Debug, Default)]
pub struct PipStage {
    window: Option<WindowId>,
    active: Option<PaneId>,
    floats: BTreeSet<PaneId>,
    stashed: BTreeSet<PaneId>,
}

impl PipStage {
    pub fn observe(&mut self, window: &WindowSnapshot) {
        let floats = window
            .floating
            .iter()
            .filter(|float| {
                float.visible && window.modal.is_none_or(|modal| modal.pane != float.pane)
            })
            .map(|float| float.pane)
            .collect::<BTreeSet<_>>();
        self.focus(
            window.id,
            window.active_pane,
            floats,
            |pane| window.floating.iter().any(|float| float.pane == pane),
            |pane| window.panes.contains_key(&pane),
        );
    }

    fn focus(
        &mut self,
        window: WindowId,
        active: PaneId,
        floats: BTreeSet<PaneId>,
        floating: impl Fn(PaneId) -> bool,
        alive: impl Fn(PaneId) -> bool,
    ) {
        if self.window == Some(window) {
            self.stashed
                .retain(|pane| floats.contains(pane) || !self.floats.contains(pane));
            if self.active != Some(active) {
                let tiled = !floating(active);
                let left = self.active.is_some_and(&alive);
                if floats.contains(&active) {
                    self.stashed.remove(&active);
                } else if tiled && left {
                    self.stashed
                        .extend(floats.intersection(&self.floats).copied());
                }
            }
        }
        self.window = Some(window);
        self.active = Some(active);
        self.floats = floats;
    }

    pub fn stash(&mut self, pane: PaneId) {
        self.stashed.insert(pane);
    }

    pub fn restore(&mut self, pane: PaneId) {
        self.stashed.remove(&pane);
    }

    #[must_use]
    pub fn arrange(&self, cards: &[PipCard], canvas: Size<Pixels>, peek: Pixels) -> Vec<PipPlaced> {
        let sides = cards
            .iter()
            .map(|card| {
                (!card.held && self.stashed.contains(&card.pane)).then(|| {
                    if card.frame.center().x < canvas.width / 2.0 {
                        PipSide::Left
                    } else {
                        PipSide::Right
                    }
                })
            })
            .collect::<Vec<_>>();
        let shifts = pile_shifts(cards, &sides, canvas, peek);
        cards
            .iter()
            .zip(sides)
            .zip(shifts)
            .map(|((card, stashed), shift)| {
                let mut frame = Bounds::new(card.frame.origin + shift, card.frame.size);
                let peek = px(PEEK).min(frame.size.width);
                match stashed {
                    Some(PipSide::Left) => frame.origin.x = peek - frame.size.width,
                    Some(PipSide::Right) => frame.origin.x = canvas.width - peek,
                    None => {}
                }
                PipPlaced {
                    pane: card.pane,
                    frame,
                    stashed,
                }
            })
            .collect()
    }
}

fn covers(front: Bounds<Pixels>, back: Bounds<Pixels>) -> bool {
    let area =
        |bounds: Bounds<Pixels>| f32::from(bounds.size.width) * f32::from(bounds.size.height);
    let shared = front.intersect(&back);
    area(shared) * 2.0 >= area(front).min(area(back)) && area(shared) > 0.0
}

fn pile_shifts(
    cards: &[PipCard],
    sides: &[Option<PipSide>],
    canvas: Size<Pixels>,
    peek: Pixels,
) -> Vec<Point<Pixels>> {
    let mut pile = (0..cards.len()).collect::<Vec<_>>();
    for back in 0..cards.len() {
        if cards[back].held {
            continue;
        }
        if let Some(front) = (0..back).find(|&front| {
            !cards[front].held
                && sides[front] == sides[back]
                && covers(cards[front].frame, cards[back].frame)
        }) {
            pile[back] = pile[front];
        }
    }
    let mut tops = cards
        .iter()
        .map(|card| card.frame.origin.y)
        .collect::<Vec<_>>();
    let mut counts = BTreeMap::new();
    let mut above = BTreeMap::new();
    for (index, root) in pile.iter().enumerate() {
        let count = counts.entry(*root).or_insert(0);
        if let Some(&front) = above.get(root) {
            let reach = if *count <= STACK_DEPTH {
                peek
            } else {
                Pixels::ZERO
            };
            tops[index] = tops[index].min(front - reach);
        }
        above.insert(*root, tops[index]);
        *count += 1;
    }
    let mut lift = BTreeMap::new();
    for (index, root) in pile.iter().enumerate() {
        let top = lift.entry(*root).or_insert(canvas.height);
        *top = (*top).min(tops[index]);
    }
    let margin = px(MARGIN);
    (0..cards.len())
        .map(|index| {
            if counts[&pile[index]] < 2 {
                return Point::default();
            }
            let drop = (margin - lift[&pile[index]]).max(Pixels::ZERO);
            point(
                Pixels::ZERO,
                tops[index] + drop - cards[index].frame.origin.y,
            )
        })
        .collect()
}

#[must_use]
pub fn pip_content_radius(cx: &App) -> Pixels {
    let border = cx.theme().layer(roles::PIP, PaintState::Rest).border_width;
    (cx.theme().pane_radius() - border).max(Pixels::ZERO)
}

#[derive(IntoElement)]
pub struct PipSurface {
    id: ElementId,
    header: Option<AnyElement>,
    content: AnyElement,
    inset_x: Pixels,
    inset_y: Pixels,
    bordered: bool,
    fill: Option<Hsla>,
    stashed: Option<Handler>,
}

impl PipSurface {
    pub fn new(id: impl Into<ElementId>, content: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            header: None,
            content: content.into_any_element(),
            inset_x: Pixels::ZERO,
            inset_y: Pixels::ZERO,
            bordered: true,
            fill: None,
            stashed: None,
        }
    }

    #[must_use]
    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    #[must_use]
    pub const fn content_inset(mut self, x: Pixels, y: Pixels) -> Self {
        self.inset_x = x;
        self.inset_y = y;
        self
    }

    #[must_use]
    pub const fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }

    #[must_use]
    pub const fn fill(mut self, fill: Hsla) -> Self {
        self.fill = Some(fill);
        self
    }

    #[must_use]
    pub fn stashed(
        mut self,
        stashed: bool,
        on_restore: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.stashed = stashed.then(|| Box::new(on_restore) as Handler);
        self
    }
}

#[must_use]
pub fn pip_material(fill: Hsla, cx: &App) -> GlassMaterial {
    let theme = cx.theme();
    let fill = fill.opaque();
    let opacity = theme.pip_opacity;
    theme
        .layer(roles::PIP, PaintState::Rest)
        .glass
        .unwrap_or_else(|| {
            GlassMaterial::edge(fill)
                .blur(px(FROST))
                .specular(0.0)
                .fresnel(0.0)
                .edge_shadow(0.0)
                .glint_width(Pixels::ZERO)
        })
        .tint(fill.alpha(opacity))
        .fallback(fill)
}

impl RenderOnce for PipSurface {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut layer = cx.theme().layer(roles::PIP, PaintState::Rest);
        if !self.bordered {
            layer.border_width = Pixels::ZERO;
        }
        let border = layer.border_width;
        let inset_x = (self.inset_x - border).max(Pixels::ZERO);
        let inset_y = (self.inset_y - border).max(Pixels::ZERO);
        let coarse = crate::touch::CoarsePointer::get(cx);
        let docked = self.header.is_some() && self.inset_y > Pixels::ZERO;
        let header_height = cx.theme().pane_header_height().max(self.inset_y);
        let inset_top = if docked {
            (header_height - border).max(Pixels::ZERO)
        } else {
            inset_y
        };
        let stashed = self.stashed.is_some();
        let fill = self
            .fill
            .unwrap_or_else(|| cx.theme().pane_color(cx.theme().background));
        let material = pip_material(fill, cx);
        let header = self.header.map(|header| {
            div()
                .debug_selector(|| "pip-header".into())
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(header_height)
                .when(!coarse && !docked && !stashed, |header| {
                    header
                        .invisible()
                        .group_hover(GROUP, zz_gpui::Styled::visible)
                })
                .child(header)
        });
        let stash = self.stashed.map(|restore| {
            div()
                .id("pip-stash")
                .debug_selector(|| "pip-stash".into())
                .absolute()
                .inset_0()
                .occlude()
                .cursor_pointer()
                .on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    restore(event, window, cx);
                })
        });
        div()
            .id(self.id)
            .group(GROUP)
            .relative()
            .size_full()
            .overflow_hidden()
            .occlude()
            .layer(&layer)
            .rounded(cx.theme().pane_radius())
            .bg(cx.theme().transparent)
            .glass(material)
            .on_mouse_down(zz_gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .absolute()
                    .left(inset_x)
                    .right(inset_x)
                    .top(inset_top)
                    .bottom(inset_y)
                    .overflow_hidden()
                    .child(self.content),
            )
            .children(header)
            .children(stash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_throw_is_measured_over_the_last_tenth_of_a_second() {
        let start = Instant::now();
        let mut throw = PipThrow::default();
        throw.track(start, point(px(0.0), px(0.0)));
        throw.track(start + Duration::from_millis(200), point(px(10.0), px(0.0)));
        throw.track(
            start + Duration::from_millis(250),
            point(px(60.0), px(20.0)),
        );
        let velocity = throw.velocity(start + Duration::from_millis(250));
        assert!((velocity.x - 1000.0).abs() < 1.0, "{velocity:?}");
        assert!((velocity.y - 400.0).abs() < 1.0, "{velocity:?}");
        assert_eq!(
            throw.velocity(start + Duration::from_millis(500)),
            Point::default()
        );
    }

    #[test]
    fn a_pip_casts_the_float_shadow() {
        let theme = crate::Theme::from(&*crate::ThemeColor::dark());
        let pip = theme.layer(roles::PIP, PaintState::Rest);
        assert_eq!(pip.shadow, theme.float_shadows());
        assert!(!pip.shadow.is_empty());
    }

    #[test]
    fn a_throw_coasts_half_a_second_of_its_speed() {
        let landing = projected(point(px(100.0), px(100.0)), point(1000.0, -200.0));
        assert!((f32::from(landing.x) - 599.0).abs() < 1.0, "{landing:?}");
        assert!((f32::from(landing.y) - 0.2).abs() < 1.0, "{landing:?}");
    }

    const LAYOUT: PipLayout = PipLayout {
        window: (80, 24),
        cell: Size {
            width: px(8.0),
            height: px(16.0),
        },
        canvas: Size {
            width: px(640.0),
            height: px(384.0),
        },
        lines: PaneBorderLines::Single,
        header: crate::PANE_HEADER_HEIGHT,
    };

    #[test]
    fn every_place_sits_the_same_margin_in_from_the_canvas_edges() {
        let float = FloatCells {
            xoff: 30,
            yoff: 9,
            sx: 20,
            sy: 6,
        };
        let frame =
            |place: FloatPlace| LAYOUT.frame(place.cells(float, LAYOUT.window, LAYOUT.lines));
        let size = LAYOUT.size(float);
        assert_eq!(
            size,
            zz_gpui::size(px(176.0), px(96.0 + 16.0) + crate::PANE_HEADER_HEIGHT)
        );
        assert_eq!(
            frame(FloatPlace::TopLeft).origin,
            point(px(MARGIN), px(MARGIN))
        );
        let bottom_right = frame(FloatPlace::BottomRight);
        assert_eq!(
            bottom_right.bottom_right(),
            point(px(640.0 - MARGIN), px(384.0 - MARGIN))
        );
        let centre = frame(FloatPlace::Centre).center();
        assert_eq!(centre.x, px(320.0));
        assert!((f32::from(centre.y) - 192.0).abs() <= 16.0, "{centre:?}");
    }

    #[test]
    fn a_pip_taller_than_the_room_left_stays_inside_the_canvas() {
        for sy in [18, 22, 24, 40] {
            let float = FloatCells {
                xoff: 30,
                yoff: 1,
                sx: 20,
                sy,
            };
            for place in FloatPlace::ALL {
                let frame = LAYOUT.frame(place.cells(float, LAYOUT.window, LAYOUT.lines));
                assert!(frame.origin.y >= Pixels::ZERO, "{sy} {place:?} {frame:?}");
                if frame.size.height <= LAYOUT.canvas.height {
                    assert!(
                        frame.bottom() <= LAYOUT.canvas.height,
                        "{sy} {place:?} {frame:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_largest_pip_a_resize_allows_fits_a_margin_inside_the_canvas() {
        let (sx, sy) = LAYOUT.max_cells();
        let size = |sx, sy| {
            LAYOUT.size(FloatCells {
                xoff: 0,
                yoff: 0,
                sx,
                sy,
            })
        };
        let largest = size(sx, sy);
        assert!(largest.width <= LAYOUT.canvas.width - px(MARGIN * 2.0));
        assert!(largest.height <= LAYOUT.canvas.height - px(MARGIN * 2.0));
        assert!(size(sx + 1, sy).width > LAYOUT.canvas.width - px(MARGIN * 2.0));
        assert!(size(sx, sy + 1).height > LAYOUT.canvas.height - px(MARGIN * 2.0));
    }

    #[test]
    fn a_pip_let_go_past_the_side_edge_is_stashed_there() {
        let start = FloatCells {
            xoff: 30,
            yoff: 9,
            sx: 20,
            sy: 6,
        };
        let mut drag = PipDrag::new(start, LAYOUT, Point::default(), Point::default());
        assert_eq!(drag.release(Point::default()).stash, None);
        drag.moved(point(px(-400.0), px(0.0)));
        let landing = drag.release(Point::default());
        assert_eq!(landing.stash, Some(PipSide::Left));
        assert_eq!(landing.place, FloatPlace::CentreLeft);
        drag.moved(point(px(450.0), px(0.0)));
        assert_eq!(drag.release(Point::default()).stash, Some(PipSide::Right));
    }

    fn card(pane: u64, x: f32, y: f32) -> PipCard {
        PipCard {
            pane: PaneId(pane),
            frame: Bounds::new(point(px(x), px(y)), zz_gpui::size(px(200.0), px(120.0))),
            held: false,
        }
    }

    const CANVAS: Size<Pixels> = Size {
        width: px(800.0),
        height: px(600.0),
    };

    #[test]
    fn pips_on_one_place_fan_out_so_each_one_behind_shows_its_header() {
        let stage = PipStage::default();
        let bottom = [
            card(1, 588.0, 468.0),
            card(2, 588.0, 468.0),
            card(3, 20.0, 20.0),
        ];
        let placed = stage.arrange(&bottom, CANVAS, px(34.0));
        assert_eq!(placed[0].frame.origin, point(px(588.0), px(468.0)));
        assert_eq!(placed[1].frame.origin, point(px(588.0), px(434.0)));
        assert_eq!(placed[2].frame.origin, point(px(20.0), px(20.0)));
        let top = [
            card(1, 588.0, 12.0),
            card(2, 588.0, 12.0),
            card(3, 588.0, 12.0),
        ];
        let placed = stage.arrange(&top, CANVAS, px(34.0));
        assert_eq!(placed[2].frame.origin.y, px(12.0));
        assert_eq!(placed[1].frame.origin.y, px(46.0));
        assert_eq!(placed[0].frame.origin.y, px(80.0));
        let mut taller = card(2, 560.0, 300.0);
        taller.frame.size = zz_gpui::size(px(228.0), px(288.0));
        let placed = stage.arrange(&[card(1, 588.0, 468.0), taller], CANVAS, px(34.0));
        assert_eq!(
            placed[1].frame.origin.y,
            px(300.0),
            "a taller PiP behind already shows its header"
        );
    }

    #[test]
    fn focusing_a_tiled_pane_stashes_the_pips_and_focusing_one_brings_it_back() {
        let mut stage = PipStage::default();
        let window = WindowId(1);
        let floats = BTreeSet::from([PaneId(5), PaneId(6)]);
        let floating = |pane: PaneId| pane.0 >= 5;
        let alive = |pane: PaneId| pane.0 != 9;
        stage.focus(window, PaneId(5), floats.clone(), floating, alive);
        stage.focus(window, PaneId(1), floats.clone(), floating, alive);
        let cards = [card(5, 588.0, 468.0), card(6, 20.0, 20.0)];
        let placed = stage.arrange(&cards, CANVAS, px(34.0));
        assert_eq!(placed[0].stashed, Some(PipSide::Right));
        assert_eq!(placed[0].frame.left(), CANVAS.width - px(PEEK));
        assert_eq!(placed[1].stashed, Some(PipSide::Left));
        assert_eq!(placed[1].frame.right(), px(PEEK));
        stage.focus(window, PaneId(6), floats.clone(), floating, alive);
        let placed = stage.arrange(&cards, CANVAS, px(34.0));
        assert_eq!(placed[0].stashed, Some(PipSide::Right));
        assert_eq!(placed[1].stashed, None);
        let more = BTreeSet::from([PaneId(5), PaneId(6), PaneId(7)]);
        stage.focus(window, PaneId(6), more.clone(), floating, alive);
        assert!(!stage.stashed.contains(&PaneId(7)));
    }

    #[test]
    fn a_stashed_pile_keeps_each_pip_in_sight() {
        let mut stage = PipStage::default();
        stage.stash(PaneId(1));
        stage.stash(PaneId(2));
        let pile = [card(1, 588.0, 468.0), card(2, 588.0, 468.0)];
        let placed = stage.arrange(&pile, CANVAS, px(34.0));
        assert!(
            placed
                .iter()
                .all(|placed| placed.stashed == Some(PipSide::Right))
        );
        assert_eq!(placed[0].frame.origin.y, px(468.0));
        assert_eq!(placed[1].frame.origin.y, px(434.0));
    }

    #[test]
    fn closing_the_focused_pane_or_switching_windows_stashes_nothing() {
        let mut stage = PipStage::default();
        let floats = BTreeSet::from([PaneId(5)]);
        let floating = |pane: PaneId| pane.0 >= 5;
        let alive = |pane: PaneId| pane.0 != 9;
        stage.focus(WindowId(1), PaneId(9), floats.clone(), floating, alive);
        stage.focus(WindowId(1), PaneId(1), floats.clone(), floating, alive);
        stage.focus(WindowId(2), PaneId(2), floats.clone(), floating, alive);
        assert!(stage.stashed.is_empty());
    }

    #[test]
    fn a_fast_throw_lands_where_it_was_headed_not_where_it_was_let_go() {
        let start = FloatCells {
            xoff: 30,
            yoff: 9,
            sx: 20,
            sy: 6,
        };
        let mut drag = PipDrag::new(start, LAYOUT, Point::default(), Point::default());
        assert_eq!(drag.landing(Point::default()), FloatPlace::Centre);
        assert_eq!(drag.landing(point(1500.0, 900.0)), FloatPlace::BottomRight);
        assert_eq!(drag.landing(point(-1500.0, 0.0)), FloatPlace::CentreLeft);
        drag.moved(point(px(-200.0), px(-120.0)));
        assert_eq!(drag.landing(Point::default()), FloatPlace::TopLeft);
        let landing = drag.release(Point::default());
        assert_eq!(
            landing.target,
            FloatPlace::TopLeft.cells(start, (80, 24), PaneBorderLines::Single)
        );
    }
}
