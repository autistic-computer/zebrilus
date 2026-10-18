use std::collections::VecDeque;

use web_time::{Duration, Instant};
use zz_client::floating::{FloatCells, FloatPlace};
use zz_gpui::{
    AnyElement, App, Bounds, ClickEvent, CursorStyle, Div, ElementId, FontWeight, IntoElement,
    ParentElement as _, Pixels, Point, SharedString, Size, SpringConfig, SpringState, Stateful,
    Styled as _, Window, div, point, prelude::*, px,
};
use zz_protocol::PaneBorderLines;

use crate::{
    ActiveTheme as _, Colorize as _, IconName, PaintState, StyledExt as _, Surface, button::Button,
    roles,
};

const GROUP: &str = "pip";
const BAR_HEIGHT: f32 = 30.0;
const VELOCITY_WINDOW: Duration = Duration::from_millis(100);
const FRAME: Duration = Duration::from_millis(16);
const DECELERATION: f32 = 0.998;
const GLIDE: SpringConfig = SpringConfig::new(324.0, 36.0, 1.0);
const SETTLED: f32 = 0.5;
const OUTWARD: f32 = 128.0;

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
pub struct PipGlide {
    x: SpringState,
    y: SpringState,
    at: Instant,
}

impl PipGlide {
    #[must_use]
    pub fn new(offset: Point<Pixels>, velocity: Point<f32>, now: Instant) -> Self {
        let (frequency, _) = GLIDE.canonical();
        let axis = |offset: f32, velocity: f32| {
            let limit = if offset * velocity < 0.0 {
                frequency * offset.abs()
            } else {
                frequency * OUTWARD
            };
            SpringState {
                position: offset,
                velocity: velocity.clamp(-limit, limit),
            }
        };
        Self {
            x: axis(f32::from(offset.x), velocity.x),
            y: axis(f32::from(offset.y), velocity.y),
            at: now,
        }
    }

    #[must_use]
    pub fn offset(&self) -> Point<Pixels> {
        point(px(self.x.position), px(self.y.position))
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.at).as_secs_f32();
        self.at = now;
        self.x = GLIDE.step(self.x, 0.0, elapsed);
        self.y = GLIDE.step(self.y, 0.0, elapsed);
        let moving =
            !(GLIDE.is_settled(self.x, 0.0, SETTLED) && GLIDE.is_settled(self.y, 0.0, SETTLED));
        if !moving {
            self.x = SpringState::default();
            self.y = SpringState::default();
        }
        moving
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PipDrag {
    pub start: FloatCells,
    lines: PaneBorderLines,
    window: (u16, u16),
    cell: Size<Pixels>,
    press: Point<Pixels>,
    pointer: Point<Pixels>,
    origin: Point<Pixels>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PipLanding {
    pub place: FloatPlace,
    pub target: FloatCells,
    pub glide: PipGlide,
}

impl PipDrag {
    #[must_use]
    pub fn new(
        start: FloatCells,
        lines: PaneBorderLines,
        window: (u16, u16),
        cell: Size<Pixels>,
        press: Point<Pixels>,
        offset: Point<Pixels>,
    ) -> Self {
        let mut drag = Self {
            start,
            lines,
            window,
            cell,
            press,
            pointer: press,
            origin: Point::default(),
        };
        drag.origin = drag.frame(start).origin + offset;
        drag
    }

    pub fn moved(&mut self, pointer: Point<Pixels>) {
        self.pointer = pointer;
    }

    #[must_use]
    pub fn drawn_origin(&self) -> Point<Pixels> {
        self.origin + (self.pointer - self.press)
    }

    fn pad(&self) -> i32 {
        i32::from(self.lines != PaneBorderLines::None)
    }

    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "window cell offsets are far below f32's exact integer range"
    )]
    pub fn frame(&self, cells: FloatCells) -> Bounds<Pixels> {
        let pad = self.pad();
        Bounds::new(
            point(
                self.cell.width * (cells.xoff - pad) as f32,
                self.cell.height * (cells.yoff - pad) as f32,
            ),
            zz_gpui::size(
                self.cell.width * (i32::from(cells.sx) + 2 * pad) as f32,
                self.cell.height * (i32::from(cells.sy) + 2 * pad) as f32,
            ),
        )
    }

    #[must_use]
    pub fn landing(&self, velocity: Point<f32>) -> FloatPlace {
        let frame = self.frame(self.start);
        let landing = projected(self.drawn_origin(), velocity);
        let centre = (
            (landing.x + frame.size.width / 2.0) / self.cell.width,
            (landing.y + frame.size.height / 2.0) / self.cell.height,
        );
        FloatPlace::nearest(centre, self.start, self.window, self.lines)
    }

    pub fn slots(&self, velocity: Point<f32>) -> impl Iterator<Item = (Bounds<Pixels>, bool)> {
        let lit = self.landing(velocity);
        let drag = *self;
        FloatPlace::ALL.into_iter().map(move |place| {
            let cells = place.cells(drag.start, drag.window, drag.lines);
            (drag.frame(cells), place == lit)
        })
    }

    #[must_use]
    pub fn release(&self, velocity: Point<f32>, now: Instant, reduce_motion: bool) -> PipLanding {
        let place = self.landing(velocity);
        let target = place.cells(self.start, self.window, self.lines);
        let glide = if reduce_motion {
            PipGlide::new(Point::default(), Point::default(), now)
        } else {
            PipGlide::new(
                self.drawn_origin() - self.frame(target).origin,
                velocity,
                now,
            )
        };
        PipLanding {
            place,
            target,
            glide,
        }
    }
}

type Handler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type BarHandle = Box<dyn FnOnce(Stateful<Div>) -> Stateful<Div>>;

#[derive(IntoElement)]
pub struct PipSurface {
    id: ElementId,
    title: SharedString,
    content: AnyElement,
    inset_x: Pixels,
    inset_y: Pixels,
    active: bool,
    bordered: bool,
    bar: Option<BarHandle>,
    on_tile: Option<Handler>,
    on_zoom: Option<Handler>,
    on_close: Option<Handler>,
}

impl PipSurface {
    pub fn new(id: impl Into<ElementId>, content: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            title: SharedString::default(),
            content: content.into_any_element(),
            inset_x: Pixels::ZERO,
            inset_y: Pixels::ZERO,
            active: false,
            bordered: true,
            bar: None,
            on_tile: None,
            on_zoom: None,
            on_close: None,
        }
    }

    #[must_use]
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }

    #[must_use]
    pub const fn content_inset(mut self, x: Pixels, y: Pixels) -> Self {
        self.inset_x = x;
        self.inset_y = y;
        self
    }

    #[must_use]
    pub const fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    #[must_use]
    pub const fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }

    #[must_use]
    pub fn bar_handle(
        mut self,
        handle: impl FnOnce(Stateful<Div>) -> Stateful<Div> + 'static,
    ) -> Self {
        self.bar = Some(Box::new(handle));
        self
    }

    #[must_use]
    pub fn on_tile(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_tile = Some(Box::new(handler));
        self
    }

    #[must_use]
    pub fn on_zoom(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_zoom = Some(Box::new(handler));
        self
    }

    #[must_use]
    pub fn on_close(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for PipSurface {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut layer = cx.theme().layer(roles::PIP, PaintState::Rest);
        if !self.bordered {
            layer.border_width = Pixels::ZERO;
        } else if self.active {
            layer.border_width = cx.theme().outline.max(px(1.0));
            layer.border = crate::pane::pane_border_color(true, cx);
        }
        let border = layer.border_width;
        let inset_x = (self.inset_x - border).max(Pixels::ZERO);
        let inset_y = (self.inset_y - border).max(Pixels::ZERO);
        let coarse = crate::touch::CoarsePointer::get(cx);
        let ink = cx.theme().foreground.faint();
        let button = |id: &'static str, icon: IconName, label: &'static str, handler: Handler| {
            Button::compact_icon(id, icon)
                .surface_role(roles::PIP_BUTTON)
                .size(crate::PANE_HEADER_CONTROL_HEIGHT)
                .text_color(ink)
                .tooltip(label)
                .on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    handler(event, window, cx);
                })
        };
        let buttons = [
            self.on_tile
                .map(|handler| button("pip-tile", IconName::LayoutColumns, "Tile back", handler)),
            self.on_zoom
                .map(|handler| button("pip-zoom", IconName::WindowMaximize, "Zoom", handler)),
            self.on_close
                .map(|handler| button("pip-close", IconName::Xmark, "Close", handler)),
        ];
        let bar = div()
            .id("pip-bar")
            .debug_selector(|| "pip-bar".into())
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(BAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(2.0))
            .pl(px(10.0))
            .pr(px(4.0))
            .surface(Surface::new(roles::PIP_BAR), cx)
            .cursor(CursorStyle::OpenHand)
            .when(!coarse, |bar| {
                bar.invisible().group_hover(GROUP, zz_gpui::Styled::visible)
            })
            .font_family(cx.theme().font_family.clone())
            .text_size(crate::rems_from_px(12.0))
            .line_height(px(16.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().foreground)
                    .child(self.title),
            )
            .children(buttons.into_iter().flatten());
        let bar = match self.bar {
            Some(handle) => handle(bar),
            None => bar,
        };
        div()
            .id(self.id)
            .group(GROUP)
            .relative()
            .size_full()
            .overflow_hidden()
            .occlude()
            .layer(&layer)
            .on_mouse_down(zz_gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .absolute()
                    .left(inset_x)
                    .right(inset_x)
                    .top(inset_y)
                    .bottom(inset_y)
                    .overflow_hidden()
                    .child(self.content),
            )
            .child(bar)
    }
}

pub fn pip_snap_slots(
    slots: impl IntoIterator<Item = (Bounds<Pixels>, bool)>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    div()
        .absolute()
        .inset_0()
        .children(slots.into_iter().map(|(bounds, lit)| {
            let state = if lit {
                PaintState::Selected
            } else {
                PaintState::Rest
            };
            div()
                .absolute()
                .left(bounds.origin.x)
                .top(bounds.origin.y)
                .w(bounds.size.width)
                .h(bounds.size.height)
                .layer(&theme.layer(roles::PIP_SNAP, state))
        }))
        .into_any_element()
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
    fn a_throw_coasts_half_a_second_of_its_speed() {
        let landing = projected(point(px(100.0), px(100.0)), point(1000.0, -200.0));
        assert!((f32::from(landing.x) - 599.0).abs() < 1.0, "{landing:?}");
        assert!((f32::from(landing.y) - 0.2).abs() < 1.0, "{landing:?}");
    }

    #[test]
    fn a_fast_throw_lands_where_it_was_headed_not_where_it_was_let_go() {
        let start = FloatCells {
            xoff: 30,
            yoff: 9,
            sx: 20,
            sy: 6,
        };
        let cell = zz_gpui::size(px(8.0), px(16.0));
        let mut drag = PipDrag::new(
            start,
            PaneBorderLines::Single,
            (80, 24),
            cell,
            Point::default(),
            Point::default(),
        );
        assert_eq!(drag.landing(Point::default()), FloatPlace::Centre);
        assert_eq!(drag.landing(point(1500.0, 900.0)), FloatPlace::BottomRight);
        assert_eq!(drag.landing(point(-1500.0, 0.0)), FloatPlace::CentreLeft);
        drag.moved(point(px(-200.0), px(-120.0)));
        assert_eq!(drag.landing(Point::default()), FloatPlace::TopLeft);
        let landing = drag.release(Point::default(), Instant::now(), false);
        assert_eq!(
            landing.target,
            FloatPlace::TopLeft.cells(start, (80, 24), PaneBorderLines::Single)
        );
        assert_eq!(
            drag.drawn_origin() - landing.glide.offset(),
            drag.frame(landing.target).origin
        );
    }

    #[test]
    fn a_glide_settles_on_its_place_without_overshoot() {
        let start = Instant::now();
        let mut glide = PipGlide::new(point(px(-400.0), px(120.0)), point(9000.0, -50.0), start);
        let mut now = start;
        let mut frames = 0;
        while glide.tick(now) {
            let offset = glide.offset();
            assert!(f32::from(offset.x) <= 0.5, "{offset:?}");
            assert!(f32::from(offset.y) >= -0.5, "{offset:?}");
            now += FRAME;
            frames += 1;
            assert!(frames < 120, "never settled");
        }
        assert_eq!(glide.offset(), Point::default());
    }

    #[test]
    fn a_throw_past_its_place_swings_out_a_little_and_comes_back() {
        let start = Instant::now();
        let mut glide = PipGlide::new(point(px(100.0), px(0.0)), point(48000.0, 0.0), start);
        let mut now = start;
        let mut furthest: f32 = 0.0;
        while glide.tick(now) {
            furthest = furthest.max(f32::from(glide.offset().x));
            now += FRAME;
        }
        assert!(furthest < 160.0, "swung out to {furthest}");
        assert!(now.saturating_duration_since(start) < Duration::from_millis(900));
    }
}
