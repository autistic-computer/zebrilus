use std::{cell::RefCell, rc::Rc};

use ala::{
    AnyElement, App, AvailableSpace, Bounds, Div, Element, ElementId, Entity, Global,
    GlobalElementId, Img, InspectorElementId, IntoElement, LayoutId, Pixels, Point, RenderOnce,
    Size, Styled, ViewElement, WeakEntity, Window, WindowId, point, size,
};
use web_time::{Duration, Instant};

const UNCLAIMED: Duration = Duration::from_secs(1);
const SHOWN_BY: f32 = 0.1;
const LIT_FROM: f32 = 0.35;
const LIT_OVER: f32 = 0.5;
const DIMMED: f32 = 0.4;
const DIMMED_BY: f32 = 0.35;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipPart {
    Icon,
    Title,
    Picture,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SourceParts {
    icon: Option<Bounds<Pixels>>,
    title: Option<Bounds<Pixels>>,
    picture: Option<Bounds<Pixels>>,
}

impl SourceParts {
    const fn get(&self, part: PipPart) -> Option<Bounds<Pixels>> {
        match part {
            PipPart::Icon => self.icon,
            PipPart::Title => self.title,
            PipPart::Picture => self.picture,
        }
    }

    const fn set(&mut self, part: PipPart, bounds: Bounds<Pixels>) {
        match part {
            PipPart::Icon => self.icon = Some(bounds),
            PipPart::Title => self.title = Some(bounds),
            PipPart::Picture => self.picture = Some(bounds),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    pub bounds: Bounds<Pixels>,
    pub radius: Pixels,
    pub parts: SourceParts,
    pub carries: bool,
}

impl Drawn {
    #[must_use]
    pub fn new(bounds: Bounds<Pixels>, radius: Pixels) -> Self {
        Self {
            bounds,
            radius,
            parts: SourceParts::default(),
            carries: false,
        }
    }
}

#[derive(Default)]
pub struct SourceState {
    drawn: Option<Drawn>,
    seq: u64,
    progress: Option<f32>,
}

impl SourceState {
    fn record(&mut self, drawn: Drawn) {
        self.drawn = Some(drawn);
        self.seq += 1;
    }

    pub(super) const fn seq(&self) -> u64 {
        self.seq
    }

    pub(super) fn since(&self, seq: u64) -> Option<Drawn> {
        self.drawn.filter(|_| self.seq > seq)
    }

    pub(super) fn show(&mut self, progress: Option<f32>) -> bool {
        let changed = self.progress != progress;
        self.progress = progress;
        changed
    }

    fn shown(&self) -> f32 {
        self.progress.map_or(1.0, dimmed)
    }
}

#[must_use]
pub fn dimmed(progress: f32) -> f32 {
    1.0 - (1.0 - DIMMED) * (progress / DIMMED_BY).clamp(0.0, 1.0)
}

#[derive(Clone)]
pub(super) struct Launch {
    pub(super) source: WeakEntity<SourceState>,
    pub(super) drawn: Drawn,
    at: Instant,
    window: WindowId,
}

#[derive(Default)]
struct Pending(Option<Launch>);

impl Global for Pending {}

impl Pending {
    fn take(&mut self, window: WindowId, now: Instant) -> Option<Launch> {
        let launch = self.0.take()?;
        if now.saturating_duration_since(launch.at) >= UNCLAIMED {
            return None;
        }
        if launch.window != window {
            self.0 = Some(launch);
            return None;
        }
        Some(launch)
    }
}

pub(super) fn claim(window: WindowId, now: Instant, cx: &mut App) -> Option<Launch> {
    cx.default_global::<Pending>().take(window, now)
}

#[derive(Clone)]
pub struct PipSource {
    id: ElementId,
    radius: Pixels,
    carries: bool,
    picture: bool,
    state: Rc<RefCell<Option<WeakEntity<SourceState>>>>,
}

impl PipSource {
    pub fn new(id: impl Into<ElementId>, radius: Pixels) -> Self {
        Self {
            id: id.into(),
            radius,
            carries: false,
            picture: false,
            state: Rc::default(),
        }
    }

    #[must_use]
    pub const fn carries_content(mut self) -> Self {
        self.carries = true;
        self
    }

    #[must_use]
    pub const fn picture(mut self) -> Self {
        self.picture = true;
        self
    }

    pub fn launch(&self, window: &Window, cx: &mut App) {
        let Some(source) = self.state.borrow().clone() else {
            return;
        };
        let Some(drawn) = source.upgrade().and_then(|state| state.read(cx).drawn) else {
            return;
        };
        let at = cx.background_executor().now();
        cx.set_global(Pending(Some(Launch {
            source,
            drawn,
            at,
            window: window.window_handle().window_id(),
        })));
    }

    pub fn draw<E: Styled + IntoElement + 'static>(&self, child: E) -> Sourced<E> {
        Sourced {
            source: self.clone(),
            child,
        }
    }
}

pub struct Sourced<E> {
    source: PipSource,
    child: E,
}

impl<E: Styled + IntoElement + 'static> IntoElement for Sourced<E> {
    type Element = ViewElement<Self>;

    fn into_element(self) -> Self::Element {
        ViewElement::new(self)
    }
}

impl<E: Styled + IntoElement + 'static> RenderOnce for Sourced<E> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let source = self.source;
        let state = window.use_keyed_state(source.id.clone(), cx, |_, _| SourceState::default());
        *source.state.borrow_mut() = Some(state.downgrade());
        let shown = state.read(cx).shown();
        let child = if shown < 1.0 {
            self.child.opacity(shown)
        } else {
            self.child
        };
        Recorder {
            state,
            radius: source.radius,
            carries: source.carries,
            picture: source.picture,
            child: child.into_any_element(),
        }
    }
}

struct Recorder {
    state: Entity<SourceState>,
    radius: Pixels,
    carries: bool,
    picture: bool,
    child: AnyElement,
}

impl IntoElement for Recorder {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Recorder {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut parts = within(cx, Scope::Source(SourceParts::default()), |cx| {
            self.child.prepaint(window, cx);
        });
        if self.picture {
            parts.picture = Some(bounds);
        }
        let drawn = Drawn {
            bounds,
            radius: self.radius,
            parts,
            carries: self.carries,
        };
        self.state.update(cx, |state, _| state.record(drawn));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut (),
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flight {
    pub progress: f32,
    pub fade: f32,
    pub frame: Bounds<Pixels>,
    pub rest: Bounds<Pixels>,
    pub source: Option<Drawn>,
}

impl Flight {
    #[must_use]
    pub fn between(progress: f32, from: Drawn, rest: Bounds<Pixels>) -> Self {
        Self {
            progress,
            fade: shown(progress),
            frame: lerp_bounds(from.bounds, rest, progress),
            rest,
            source: Some(from),
        }
    }

    #[must_use]
    pub fn lit(&self) -> f32 {
        ((self.progress - LIT_FROM) / LIT_OVER).clamp(0.0, 1.0)
    }

    #[must_use]
    pub fn radius(&self, rest: Pixels) -> Pixels {
        self.source.map_or(rest, |source| {
            source.radius + (rest - source.radius) * self.progress.clamp(0.0, 1.0)
        })
    }

    #[must_use]
    pub fn rides(&self, part: PipPart) -> bool {
        self.source
            .is_some_and(|source| source.parts.get(part).is_some())
    }

    #[must_use]
    pub fn carries_content(&self) -> bool {
        self.source
            .is_some_and(|source| source.carries || source.parts.picture.is_some())
    }

    fn ride(&self, part: PipPart, natural: Bounds<Pixels>) -> Option<Ride> {
        let start = self.source?.parts.get(part)?;
        let p = self.progress.clamp(0.0, 1.0);
        let rest = natural.origin - self.frame.origin + self.rest.origin;
        let half = |size: Size<Pixels>| point(size.width / 2.0, size.height / 2.0);
        let from = match part {
            PipPart::Icon => start.center() - half(natural.size),
            PipPart::Title => point(start.left(), start.center().y - natural.size.height / 2.0),
            PipPart::Picture => {
                if natural.size.width <= Pixels::ZERO || natural.size.height <= Pixels::ZERO {
                    return None;
                }
                let rest = Bounds::new(rest, natural.size);
                return Some(Ride::Picture(lerp_bounds(
                    contain(natural.size, start),
                    rest,
                    p,
                )));
            }
        };
        Some(Ride::Move(lerp_point(from, rest, p) - natural.origin))
    }
}

#[must_use]
pub fn shown(progress: f32) -> f32 {
    (progress / SHOWN_BY).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Ride {
    Move(Point<Pixels>),
    Picture(Bounds<Pixels>),
}

fn contain(size: Size<Pixels>, within: Bounds<Pixels>) -> Bounds<Pixels> {
    let scale = (within.size.width / size.width).min(within.size.height / size.height);
    let fitted = Size {
        width: size.width * scale,
        height: size.height * scale,
    };
    Bounds::new(
        point(
            within.left() + (within.size.width - fitted.width) / 2.0,
            within.top() + (within.size.height - fitted.height) / 2.0,
        ),
        fitted,
    )
}

fn lerp_point(from: Point<Pixels>, to: Point<Pixels>, p: f32) -> Point<Pixels> {
    point(from.x + (to.x - from.x) * p, from.y + (to.y - from.y) * p)
}

pub(super) fn lerp_bounds(from: Bounds<Pixels>, to: Bounds<Pixels>, p: f32) -> Bounds<Pixels> {
    Bounds::from_corners(
        lerp_point(from.origin, to.origin, p),
        lerp_point(from.bottom_right(), to.bottom_right(), p),
    )
}

#[derive(Clone, Copy)]
enum Scope {
    Source(SourceParts),
    Flight(Flight),
}

#[derive(Default)]
struct Scopes(Vec<Scope>);

impl Global for Scopes {}

fn within(cx: &mut App, scope: Scope, f: impl FnOnce(&mut App)) -> SourceParts {
    cx.default_global::<Scopes>().0.push(scope);
    f(cx);
    match cx.default_global::<Scopes>().0.pop() {
        Some(Scope::Source(parts)) => parts,
        _ => SourceParts::default(),
    }
}

pub(super) fn flying(cx: &mut App, flight: Flight, f: impl FnOnce(&mut App)) {
    within(cx, Scope::Flight(flight), f);
}

#[must_use]
pub fn flight(cx: &App) -> Option<Flight> {
    match cx.try_global::<Scopes>()?.0.last()? {
        Scope::Flight(flight) => Some(*flight),
        Scope::Source(_) => None,
    }
}

pub fn lit(div: Div) -> Lit {
    Lit(div)
}

#[derive(IntoElement)]
pub struct Lit(Div);

impl RenderOnce for Lit {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        match flight(cx).map(|flight| flight.lit()) {
            Some(lit) if lit < 1.0 => self.0.opacity(lit),
            _ => self.0,
        }
    }
}

pub fn pip_part(part: PipPart, child: impl IntoElement) -> Riding {
    Riding {
        part,
        child: child.into_any_element(),
        picture: None,
    }
}

pub fn pip_picture(picture: impl Fn() -> Img + 'static) -> Riding {
    Riding {
        part: PipPart::Picture,
        child: picture().into_any_element(),
        picture: Some(Box::new(picture)),
    }
}

pub struct Riding {
    part: PipPart,
    child: AnyElement,
    picture: Option<Box<dyn Fn() -> Img>>,
}

impl IntoElement for Riding {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Riding {
    type RequestLayoutState = ();
    type PrepaintState = Option<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let top = cx
            .try_global::<Scopes>()
            .and_then(|scopes| scopes.0.last().copied());
        let ride = match top {
            Some(Scope::Source(_)) => {
                if let Some(Scope::Source(parts)) = cx.default_global::<Scopes>().0.last_mut() {
                    parts.set(self.part, bounds);
                }
                None
            }
            Some(Scope::Flight(flight)) => flight.ride(self.part, bounds),
            None => None,
        };
        match (ride, &self.picture) {
            (Some(Ride::Move(offset)), _) => {
                window.with_element_offset(offset, |window| self.child.prepaint(window, cx));
                None
            }
            (Some(Ride::Picture(at)), Some(picture)) => {
                let mut picture = picture().size_full().into_any_element();
                picture.layout_as_root(
                    size(
                        AvailableSpace::Definite(at.size.width),
                        AvailableSpace::Definite(at.size.height),
                    ),
                    window,
                    cx,
                );
                picture.prepaint_at(at.origin, window, cx);
                Some(picture)
            }
            _ => {
                self.child.prepaint(window, cx);
                None
            }
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut (),
        picture: &mut Option<AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) {
        match picture {
            Some(picture) => picture.paint(window, cx),
            None => self.child.paint(window, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ala::{AppContext as _, px};

    #[ala::test]
    fn an_unclaimed_launch_is_dropped_after_a_second(cx: &mut ala::TestAppContext) {
        let source = cx.new(|_| SourceState::default());
        let at = Instant::now();
        let here = WindowId::from(1);
        let launch = |at| Launch {
            source: source.downgrade(),
            drawn: Drawn::new(
                Bounds::new(point(px(10.0), px(20.0)), size(px(200.0), px(36.0))),
                px(8.0),
            ),
            at,
            window: here,
        };
        let mut pending = Pending(Some(launch(at)));
        assert!(pending.take(WindowId::from(2), at).is_none());
        let claimed = pending.take(here, at + Duration::from_millis(999));
        assert_eq!(
            claimed.map(|launch| launch.drawn.bounds.origin),
            Some(point(px(10.0), px(20.0)))
        );
        assert!(pending.0.is_none());
        let mut pending = Pending(Some(launch(at)));
        assert!(pending.take(here, at + Duration::from_secs(1)).is_none());
        assert!(pending.0.is_none());
    }
}
