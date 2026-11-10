use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

use ala::{
    AnyElement, App, AvailableSpace, Bounds, Element, ElementId, Empty, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, ParentElement as _, Pixels, Point, Size,
    SpringConfig, SpringState, Style, Styled as _, WeakEntity, Window, div, point, px, relative,
    size,
};
use web_time::{Duration, Instant};
use zz_protocol::{PaneId, WindowId};

use super::launch::{self, Drawn, Flight, Launch, SourceState};

const SPRING: SpringConfig = SpringConfig::new(784.0, 56.0, 1.0);
const SETTLED: f32 = 0.5;
const OUTWARD: f32 = 128.0;
const LANDED: f32 = 0.002;
const DROP: Duration = Duration::from_millis(160);
const SHRINK: f32 = 0.04;

#[derive(Clone, Default)]
pub struct PaneMotion(Rc<RefCell<Motions>>);

#[derive(Default)]
struct Motions {
    window: Option<WindowId>,
    viewport: Size<Pixels>,
    frame: u64,
    snap: bool,
    panes: BTreeMap<PaneId, Motion>,
    flings: BTreeMap<PaneId, (Point<f32>, bool)>,
    links: BTreeMap<PaneId, Link>,
    leaving: BTreeMap<PaneId, Leaving>,
    released: Vec<WeakEntity<SourceState>>,
    frozen: Option<f32>,
}

#[derive(Clone)]
struct Leaving {
    link: Link,
    rest: Bounds<Pixels>,
    drawn: Bounds<Pixels>,
    from: f32,
    asked: u64,
    way: Way,
    at: Instant,
    seen: Option<Drawn>,
    frames: u8,
}

#[derive(Clone, Copy, Debug)]
enum Way {
    Deciding,
    Back(SpringState),
    Drop(Instant),
}

#[derive(Clone)]
struct Link {
    source: WeakEntity<SourceState>,
    drawn: Drawn,
}

#[derive(Clone, Copy, Debug)]
struct Motion {
    target: Bounds<Pixels>,
    edges: [SpringState; 4],
    at: Instant,
    placed: u64,
    last: Bounds<Pixels>,
    drawn: Bounds<Pixels>,
    rise: Option<Rise>,
}

#[derive(Clone, Copy, Debug)]
struct Rise {
    from: Bounds<Pixels>,
    remaining: SpringState,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    frame: Bounds<Pixels>,
    moving: bool,
    progress: Option<f32>,
}

impl PaneMotion {
    pub fn begin(&self, window: WindowId, viewport: Size<Pixels>, still: bool) {
        let mut motions = self.0.borrow_mut();
        if motions.window != Some(window) {
            motions.panes.clear();
            motions.flings.clear();
            let leaving = std::mem::take(&mut motions.leaving);
            motions
                .released
                .extend(leaving.into_values().map(|leaving| leaving.link.source));
        }
        let frame = motions.frame;
        motions.panes.retain(|_, motion| motion.placed == frame);
        for motion in motions.panes.values_mut() {
            motion.last = motion.drawn;
        }
        motions.snap = still || motions.viewport != viewport;
        motions.frame += 1;
        motions.window = Some(window);
        motions.viewport = viewport;
    }

    pub fn fling(&self, pane: PaneId, velocity: Point<f32>, swing: bool) {
        self.0.borrow_mut().flings.insert(pane, (velocity, swing));
    }

    #[must_use]
    pub fn drawn(&self, pane: PaneId) -> Option<Bounds<Pixels>> {
        self.0.borrow().panes.get(&pane).map(|motion| motion.drawn)
    }

    #[must_use]
    pub fn moving(&self) -> bool {
        self.0.borrow().panes.values().any(Motion::moving)
    }

    #[must_use]
    pub fn launched(&self, pane: PaneId) -> bool {
        self.0.borrow().links.contains_key(&pane)
    }

    pub fn freeze(&self, progress: Option<f32>) {
        self.0.borrow_mut().frozen = progress.map(|progress| progress.clamp(0.0, 1.0));
    }

    fn knows(&self, pane: PaneId) -> bool {
        self.0.borrow().panes.contains_key(&pane)
    }

    fn link(&self, pane: PaneId, launch: Launch) {
        let link = Link {
            source: launch.source,
            drawn: launch.drawn,
        };
        let mut motions = self.0.borrow_mut();
        if let Some(old) = motions.links.insert(pane, link) {
            motions.released.push(old.source);
        }
    }

    pub fn relink(&self, pane: PaneId, window: &Window, cx: &mut App) {
        if cx.reduce_motion() {
            return;
        }
        let now = cx.background_executor().now();
        let Some(launch) = launch::claim(window.window_handle().window_id(), now, cx) else {
            return;
        };
        let source = launch.source.clone();
        let old = self.0.borrow_mut().links.insert(
            pane,
            Link {
                source: launch.source,
                drawn: launch.drawn,
            },
        );
        if let Some(old) = old {
            show(&old.source, None, window, cx);
        }
        show(&source, Some(1.0), window, cx);
    }

    pub fn depart(&self, pane: PaneId, cx: &mut App) -> bool {
        let now = cx.background_executor().now();
        let mut motions = self.0.borrow_mut();
        let Some(link) = motions.links.remove(&pane) else {
            return false;
        };
        let frame = motions.frame;
        let drawn = motions
            .panes
            .remove(&pane)
            .filter(|motion| motion.placed + 1 >= frame && !cx.reduce_motion());
        let source = link.source.upgrade();
        let Some(motion) = drawn else {
            drop(motions);
            if let Some(source) = source {
                source.update(cx, |source, cx| {
                    source.show(None);
                    cx.notify();
                });
            }
            return false;
        };
        let asked = source
            .as_ref()
            .map_or(u64::MAX, |source| source.read(cx).seq());
        motions.leaving.insert(
            pane,
            Leaving {
                rest: motion.target,
                drawn: motion.drawn,
                from: motion.placed().progress.unwrap_or(1.0),
                link,
                asked,
                way: Way::Deciding,
                at: now,
                seen: None,
                frames: 0,
            },
        );
        drop(motions);
        if let Some(source) = source {
            source.update(cx, |_, cx| cx.notify());
        }
        true
    }

    #[must_use]
    pub fn departing(&self, pane: PaneId) -> bool {
        self.0.borrow().leaving.contains_key(&pane)
    }

    #[must_use]
    pub fn departures(&self) -> Vec<(PaneId, Bounds<Pixels>)> {
        self.0
            .borrow()
            .leaving
            .iter()
            .map(|(pane, leaving)| (*pane, leaving.rest))
            .collect()
    }

    fn leaving(&self, pane: PaneId) -> Option<(WeakEntity<SourceState>, u64)> {
        self.0
            .borrow()
            .leaving
            .get(&pane)
            .map(|leaving| (leaving.link.source.clone(), leaving.asked))
    }

    fn leave(&self, pane: PaneId, now: Instant, source: Option<Drawn>) -> Option<Flight> {
        let mut motions = self.0.borrow_mut();
        let frozen = motions.frozen;
        let flight = motions.leaving.get_mut(&pane)?.step(now, source, frozen);
        if flight.is_none() {
            motions.leaving.remove(&pane);
        }
        flight
    }

    fn flight(&self, pane: PaneId, placed: Placed, rest: Bounds<Pixels>) -> Option<Flight> {
        let progress = placed.progress?;
        let drawn = self.0.borrow().links.get(&pane)?.drawn;
        Some(Flight {
            progress,
            fade: launch::shown(progress),
            frame: placed.frame,
            rest,
            source: Some(drawn),
        })
    }

    fn show_source(&self, pane: PaneId, progress: Option<f32>, window: &Window, cx: &mut App) {
        let source = self
            .0
            .borrow()
            .links
            .get(&pane)
            .map(|link| link.source.clone());
        if let Some(source) = source {
            show(&source, progress, window, cx);
        }
    }

    fn release(&self, window: &Window, cx: &mut App) {
        let released = std::mem::take(&mut self.0.borrow_mut().released);
        for source in released {
            show(&source, None, window, cx);
        }
    }

    fn place(
        &self,
        pane: PaneId,
        target: Bounds<Pixels>,
        held: bool,
        now: Instant,
        still: bool,
        launch: Option<Bounds<Pixels>>,
    ) -> Placed {
        let mut motions = self.0.borrow_mut();
        let frame = motions.frame;
        if let Some(motion) = motions
            .panes
            .get(&pane)
            .filter(|motion| motion.placed == frame)
        {
            return motion.placed();
        }
        let mut motion = motions.panes.get(&pane).copied().unwrap_or_else(|| {
            let from = launch.unwrap_or_else(|| motions.origin(target));
            Motion {
                target,
                edges: offsets(from, target).map(|position| SpringState {
                    position,
                    velocity: 0.0,
                }),
                at: now,
                placed: frame,
                last: from,
                drawn: from,
                rise: launch.map(|from| Rise {
                    from,
                    remaining: SpringState {
                        position: 1.0,
                        velocity: 0.0,
                    },
                }),
            }
        });
        if motion.target != target {
            if !motion.moving() {
                motion.at = now;
            }
            for (edge, shift) in motion.edges.iter_mut().zip(offsets(motion.target, target)) {
                edge.position += shift;
            }
            motion.target = target;
        }
        if let Some((velocity, swing)) = motions.flings.remove(&pane) {
            let (frequency, _) = SPRING.canonical();
            for (index, edge) in motion.edges.iter_mut().enumerate() {
                let speed = if index % 2 == 0 {
                    velocity.x
                } else {
                    velocity.y
                };
                let limit = if edge.position * speed < 0.0 {
                    frequency * edge.position.abs()
                } else if swing {
                    frequency * OUTWARD
                } else {
                    0.0
                };
                edge.velocity = speed.clamp(-limit, limit);
            }
            motion.at = now;
        }
        if let (Some(progress), Some(rise)) = (motions.frozen, motion.rise) {
            motion.at = now;
            motion.placed = frame;
            motion.drawn = launch::lerp_bounds(rise.from, target, progress);
            motions.panes.insert(pane, motion);
            return Placed {
                frame: motion.drawn,
                moving: false,
                progress: Some(progress),
            };
        }
        if held || still || motions.snap {
            motion.edges = [SpringState::default(); 4];
            motion.rise = None;
        } else {
            let elapsed = now.saturating_duration_since(motion.at).as_secs_f32();
            for edge in &mut motion.edges {
                *edge = SPRING.step(*edge, 0.0, elapsed);
                if SPRING.is_settled(*edge, 0.0, SETTLED) {
                    *edge = SpringState::default();
                }
            }
            if let Some(rise) = &mut motion.rise {
                rise.remaining = SPRING.step(rise.remaining, 0.0, elapsed);
            }
            if !motion.moving() {
                motion.rise = None;
            }
        }
        motion.at = now;
        motion.placed = frame;
        motion.drawn = motion.frame();
        motions.panes.insert(pane, motion);
        motion.placed()
    }
}

impl Motions {
    fn origin(&self, target: Bounds<Pixels>) -> Bounds<Pixels> {
        let slack = px(1.0);
        self.panes
            .values()
            .map(|motion| motion.last)
            .find(|last| {
                last.left() - slack <= target.left()
                    && last.top() - slack <= target.top()
                    && target.right() <= last.right() + slack
                    && target.bottom() <= last.bottom() + slack
            })
            .map_or(target, |source| grown_from(target, source))
    }
}

impl Motion {
    fn frame(&self) -> Bounds<Pixels> {
        let [left, top, right, bottom] = self.edges.map(|edge| px(edge.position));
        Bounds::from_corners(
            point(self.target.left() + left, self.target.top() + top),
            point(self.target.right() + right, self.target.bottom() + bottom),
        )
    }

    fn moving(&self) -> bool {
        self.edges
            .iter()
            .any(|edge| *edge != SpringState::default())
    }

    fn placed(&self) -> Placed {
        Placed {
            frame: self.drawn,
            moving: self.moving(),
            progress: self
                .rise
                .map(|rise| (1.0 - rise.remaining.position).clamp(0.0, 1.0)),
        }
    }
}

impl Leaving {
    fn step(&mut self, now: Instant, source: Option<Drawn>, frozen: Option<f32>) -> Option<Flight> {
        if source.is_some() {
            self.seen = source;
        }
        if matches!(self.way, Way::Deciding) {
            if source.is_some() {
                self.way = Way::Back(SpringState {
                    position: self.from,
                    velocity: 0.0,
                });
                self.at = now;
            } else if self.frames > 0 {
                self.way = Way::Drop(now);
            } else {
                self.frames += 1;
                return Some(Flight {
                    progress: self.from,
                    fade: launch::shown(self.from),
                    frame: self.drawn,
                    rest: self.rest,
                    source: Some(self.link.drawn),
                });
            }
        }
        match self.way {
            Way::Deciding => None,
            Way::Back(state) => {
                let source = self.seen.unwrap_or(self.link.drawn);
                if let Some(progress) = frozen {
                    return Some(Flight::between(progress, source, self.rest));
                }
                let elapsed = now.saturating_duration_since(self.at).as_secs_f32();
                self.at = now;
                let state = SPRING.step(state, 0.0, elapsed);
                if SPRING.is_settled(state, 0.0, LANDED) {
                    return None;
                }
                self.way = Way::Back(state);
                Some(Flight::between(
                    state.position.clamp(0.0, 1.0),
                    source,
                    self.rest,
                ))
            }
            Way::Drop(start) => {
                let gone = frozen.map_or_else(
                    || {
                        ala::animation_time(now.saturating_duration_since(start)).as_secs_f32()
                            / DROP.as_secs_f32()
                    },
                    |progress| 1.0 - progress,
                );
                if gone >= 1.0 {
                    return None;
                }
                let gone = ala::ease_out_quint()(gone);
                let inset = Size {
                    width: self.drawn.size.width * SHRINK * gone,
                    height: self.drawn.size.height * SHRINK * gone,
                };
                Some(Flight {
                    progress: 1.0,
                    fade: launch::shown(self.from) * (1.0 - gone),
                    frame: Bounds::from_corners(
                        point(
                            self.drawn.left() + inset.width,
                            self.drawn.top() + inset.height,
                        ),
                        point(
                            self.drawn.right() - inset.width,
                            self.drawn.bottom() - inset.height,
                        ),
                    ),
                    rest: self.rest,
                    source: None,
                })
            }
        }
    }
}

fn show(source: &WeakEntity<SourceState>, progress: Option<f32>, window: &Window, cx: &mut App) {
    let Some(source) = source.upgrade() else {
        return;
    };
    let changed = source.update(cx, |source, _| source.show(progress));
    if changed {
        let id = source.entity_id();
        window.on_next_frame(move |_, cx| cx.notify(id));
    }
}

fn offsets(from: Bounds<Pixels>, to: Bounds<Pixels>) -> [f32; 4] {
    [
        f32::from(from.left() - to.left()),
        f32::from(from.top() - to.top()),
        f32::from(from.right() - to.right()),
        f32::from(from.bottom() - to.bottom()),
    ]
}

fn grown_from(target: Bounds<Pixels>, source: Bounds<Pixels>) -> Bounds<Pixels> {
    let narrower = target.size.width < source.size.width - px(1.0);
    let shorter = target.size.height < source.size.height - px(1.0);
    let edge = |near: Pixels, far: Pixels, source_near: Pixels, source_far: Pixels| {
        if (source_far - far).abs() <= (near - source_near).abs() {
            (far, far)
        } else {
            (near, near)
        }
    };
    match (narrower, shorter) {
        (true, false) => {
            let (left, right) = edge(target.left(), target.right(), source.left(), source.right());
            Bounds::from_corners(point(left, target.top()), point(right, target.bottom()))
        }
        (false, true) => {
            let (top, bottom) = edge(target.top(), target.bottom(), source.top(), source.bottom());
            Bounds::from_corners(point(target.left(), top), point(target.right(), bottom))
        }
        _ => target,
    }
}

pub fn moving_pane(pane: PaneId, motion: &PaneMotion, child: impl IntoElement) -> MovingPane {
    MovingPane {
        pane,
        motion: motion.clone(),
        child: child.into_any_element(),
        held: false,
        hold: None,
        launches: false,
    }
}

pub struct MovingPane {
    pane: PaneId,
    motion: PaneMotion,
    child: AnyElement,
    held: bool,
    hold: Option<Rc<Cell<bool>>>,
    launches: bool,
}

impl MovingPane {
    #[must_use]
    pub const fn launches(mut self) -> Self {
        self.launches = true;
        self
    }

    #[must_use]
    pub const fn held(mut self, held: bool) -> Self {
        self.held = held;
        self
    }

    #[must_use]
    pub fn hold(mut self, hold: Option<Rc<Cell<bool>>>) -> Self {
        self.hold = hold;
        self
    }
}

impl IntoElement for MovingPane {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for MovingPane {
    type RequestLayoutState = ();
    type PrepaintState = bool;

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
        let style = Style {
            size: size(relative(1.).into(), relative(1.).into()),
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let now = cx.background_executor().now();
        let still = cx.reduce_motion();
        self.motion.release(window, cx);
        let (frame, flight) = if let Some((source, asked)) = self.motion.leaving(self.pane) {
            let current = source
                .upgrade()
                .and_then(|source| source.read(cx).since(asked));
            window.request_animation_frame();
            let Some(flight) = self.motion.leave(self.pane, now, current) else {
                show(&source, None, window, cx);
                return false;
            };
            if flight.source.is_some() {
                show(&source, Some(flight.progress), window, cx);
            }
            (flight.frame, Some(flight))
        } else {
            let launch = (self.launches && !still && !self.motion.knows(self.pane))
                .then(|| launch::claim(window.window_handle().window_id(), now, cx))
                .flatten();
            let placed = self.motion.place(
                self.pane,
                bounds,
                self.held,
                now,
                still,
                launch.as_ref().map(|launch| launch.drawn.bounds),
            );
            if let Some(launch) = launch {
                self.motion.link(self.pane, launch);
            }
            self.motion
                .show_source(self.pane, Some(placed.progress.unwrap_or(1.0)), window, cx);
            if let Some(hold) = &self.hold {
                hold.set(placed.moving);
            }
            if placed.moving {
                window.request_animation_frame();
            }
            (placed.frame, self.motion.flight(self.pane, placed, bounds))
        };
        if let Some(flight) = flight {
            let child = std::mem::replace(&mut self.child, Empty.into_any_element());
            self.child = div()
                .size_full()
                .opacity(flight.fade)
                .child(child)
                .into_any_element();
        }
        let mut lay_out = |cx: &mut App| {
            self.child.layout_as_root(
                size(
                    AvailableSpace::Definite(frame.size.width.max(Pixels::ZERO)),
                    AvailableSpace::Definite(frame.size.height.max(Pixels::ZERO)),
                ),
                window,
                cx,
            );
            self.child.prepaint_at(frame.origin, window, cx);
        };
        match flight {
            Some(flight) => launch::flying(cx, flight, lay_out),
            None => lay_out(cx),
        }
        true
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut (),
        drawn: &mut bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        if *drawn {
            self.child.paint(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use web_time::Duration;

    const WINDOW: WindowId = WindowId(1);
    const CANVAS: Size<Pixels> = Size {
        width: px(1000.0),
        height: px(600.0),
    };

    fn rect(left: f32, top: f32, right: f32, bottom: f32) -> Bounds<Pixels> {
        Bounds::from_corners(point(px(left), px(top)), point(px(right), px(bottom)))
    }

    fn settle(motion: &PaneMotion, panes: &[(PaneId, Bounds<Pixels>)], start: Instant) -> Instant {
        let mut now = start;
        for _ in 0..240 {
            now += Duration::from_millis(16);
            motion.begin(WINDOW, CANVAS, false);
            for (pane, target) in panes {
                motion.place(*pane, *target, false, now, false, None);
            }
            if !motion.moving() {
                break;
            }
        }
        now
    }

    #[test]
    fn a_pane_springs_from_where_it_was_drawn_to_its_new_place() {
        let motion = PaneMotion::default();
        let pane = PaneId(1);
        let start = Instant::now();
        motion.begin(WINDOW, CANVAS, false);
        let full = rect(0.0, 0.0, 1000.0, 600.0);
        assert_eq!(
            motion.place(pane, full, false, start, false, None).frame,
            full
        );
        let half = rect(0.0, 0.0, 500.0, 600.0);
        motion.begin(WINDOW, CANVAS, false);
        let first = motion.place(pane, half, false, start, false, None);
        assert_eq!(first.frame, full);
        assert!(first.moving);
        motion.begin(WINDOW, CANVAS, false);
        let next = motion.place(
            pane,
            half,
            false,
            start + Duration::from_millis(16),
            false,
            None,
        );
        assert!(next.frame.right() < full.right() && next.frame.right() > half.right());
        settle(&motion, &[(pane, half)], start);
        assert_eq!(motion.drawn(pane), Some(half));
    }

    #[test]
    fn a_launched_pip_starts_on_its_source_and_one_progress_moves_every_edge() {
        let motion = PaneMotion::default();
        let pip = PaneId(7);
        let start = Instant::now();
        let source = rect(40.0, 300.0, 400.0, 336.0);
        let rest = rect(600.0, 40.0, 960.0, 560.0);
        motion.begin(WINDOW, CANVAS, false);
        motion.begin(WINDOW, CANVAS, false);
        let first = motion.place(pip, rest, false, start, false, Some(source));
        assert_eq!(first.frame, source);
        assert_eq!(first.progress, Some(0.0));
        motion.begin(WINDOW, CANVAS, false);
        let next = motion.place(
            pip,
            rest,
            false,
            start + Duration::from_millis(48),
            false,
            None,
        );
        let progress = next.progress.expect("still flying");
        assert!(progress > 0.0 && progress < 1.0, "{progress}");
        let expected = launch::lerp_bounds(source, rest, progress);
        for (got, want) in [
            (next.frame.left(), expected.left()),
            (next.frame.top(), expected.top()),
            (next.frame.right(), expected.right()),
            (next.frame.bottom(), expected.bottom()),
        ] {
            assert!((got - want).abs() < px(0.01), "{got:?} {want:?}");
        }
        settle(&motion, &[(pip, rest)], start);
        assert_eq!(motion.drawn(pip), Some(rest));
        motion.begin(WINDOW, CANVAS, false);
        assert_eq!(
            motion.place(pip, rest, false, start, false, None).progress,
            None
        );
    }

    #[test]
    fn a_departing_pip_flies_back_to_its_source_or_shrinks_in_place_without_one() {
        let launched = Drawn::new(rect(40.0, 300.0, 400.0, 336.0), px(8.0));
        let rest = rect(600.0, 40.0, 960.0, 560.0);
        let start = Instant::now();
        let leaving = || Leaving {
            link: Link {
                source: WeakEntity::new_invalid(),
                drawn: launched,
            },
            rest,
            drawn: rest,
            from: 1.0,
            asked: 0,
            way: Way::Deciding,
            at: start,
            seen: None,
            frames: 0,
        };
        let scrolled = Drawn::new(rect(40.0, 120.0, 400.0, 156.0), px(8.0));
        let mut back = leaving();
        let first = back.step(start, Some(scrolled), None).expect("flying");
        assert_eq!(first.frame, rest);
        let mut now = start;
        let mut last = first;
        while let Some(flight) = back.step(now, None, None) {
            last = flight;
            now += Duration::from_millis(16);
            assert!(now < start + Duration::from_secs(1), "{last:?}");
        }
        assert_eq!(last.source, Some(scrolled));
        assert!(last.progress < 0.01, "{last:?}");
        for (got, want) in [
            (last.frame.left(), scrolled.bounds.left()),
            (last.frame.top(), scrolled.bounds.top()),
            (last.frame.right(), scrolled.bounds.right()),
            (last.frame.bottom(), scrolled.bounds.bottom()),
        ] {
            assert!((got - want).abs() < px(2.0), "{got:?} {want:?}");
        }
        let mut dropped = leaving();
        assert_eq!(
            dropped.step(start, None, None).map(|flight| flight.frame),
            Some(rest)
        );
        let half = dropped
            .step(start + Duration::from_millis(16), None, None)
            .expect("shrinking");
        assert_eq!(half.source, None);
        assert_eq!(half.frame, rest);
        assert_eq!(half.fade, 1.0);
        let end = dropped
            .step(start + Duration::from_millis(175), None, None)
            .expect("still shrinking");
        assert!(
            (f32::from(end.frame.left()) - (600.0 + 360.0 * 0.04)).abs() < 0.01,
            "{end:?}"
        );
        assert!(
            (f32::from(end.frame.top()) - (40.0 + 520.0 * 0.04)).abs() < 0.01,
            "{end:?}"
        );
        assert!(end.fade < 0.001, "{end:?}");
        assert_eq!(
            dropped.step(start + Duration::from_millis(176), None, None),
            None
        );
    }

    #[test]
    fn a_split_grows_the_new_pane_out_of_the_edge_it_shares_with_its_source() {
        let motion = PaneMotion::default();
        let (old, new) = (PaneId(1), PaneId(2));
        let start = Instant::now();
        motion.begin(WINDOW, CANVAS, false);
        motion.place(
            old,
            rect(0.0, 0.0, 1000.0, 600.0),
            false,
            start,
            false,
            None,
        );
        motion.begin(WINDOW, CANVAS, false);
        let left = motion.place(old, rect(0.0, 0.0, 494.0, 600.0), false, start, false, None);
        let right = motion.place(
            new,
            rect(506.0, 0.0, 1000.0, 600.0),
            false,
            start,
            false,
            None,
        );
        assert_eq!(left.frame.right(), px(1000.0));
        assert_eq!(right.frame.left(), px(1000.0));
        assert_eq!(right.frame.size.width, Pixels::ZERO);
        let mut now = start;
        for _ in 0..10 {
            now += Duration::from_millis(16);
            motion.begin(WINDOW, CANVAS, false);
            let left = motion.place(old, rect(0.0, 0.0, 494.0, 600.0), false, now, false, None);
            let right = motion.place(
                new,
                rect(506.0, 0.0, 1000.0, 600.0),
                false,
                now,
                false,
                None,
            );
            assert!(left.frame.right() <= right.frame.left());
        }
    }

    #[test]
    fn a_resized_canvas_or_a_held_pane_moves_at_once() {
        let motion = PaneMotion::default();
        let pane = PaneId(1);
        let start = Instant::now();
        motion.begin(WINDOW, CANVAS, false);
        motion.place(
            pane,
            rect(0.0, 0.0, 400.0, 300.0),
            false,
            start,
            false,
            None,
        );
        motion.begin(WINDOW, size(px(800.0), px(600.0)), false);
        let resized = rect(0.0, 0.0, 300.0, 300.0);
        assert_eq!(
            motion.place(pane, resized, false, start, false, None).frame,
            resized
        );
        motion.begin(WINDOW, size(px(800.0), px(600.0)), false);
        let held = rect(40.0, 20.0, 340.0, 320.0);
        let placed = motion.place(pane, held, true, start, false, None);
        assert_eq!(placed.frame, held);
        assert!(!placed.moving);
    }

    #[test]
    fn a_fling_at_its_place_settles_without_passing_it() {
        let motion = PaneMotion::default();
        let pane = PaneId(1);
        let start = Instant::now();
        motion.begin(WINDOW, CANVAS, false);
        let place = rect(700.0, 400.0, 980.0, 580.0);
        motion.place(
            pane,
            rect(200.0, 100.0, 480.0, 280.0),
            true,
            start,
            false,
            None,
        );
        motion.fling(pane, point(9000.0, 6000.0), true);
        let mut now = start;
        let mut furthest = Pixels::ZERO;
        for _ in 0..120 {
            now += Duration::from_millis(16);
            motion.begin(WINDOW, CANVAS, false);
            let placed = motion.place(pane, place, false, now, false, None);
            furthest = furthest.max(placed.frame.left());
        }
        assert!(furthest <= place.left() + px(SETTLED), "{furthest:?}");
        assert_eq!(motion.drawn(pane), Some(place));
    }

    #[test]
    fn a_pip_thrown_into_its_stash_comes_to_rest_without_swinging_out() {
        let motion = PaneMotion::default();
        let pane = PaneId(1);
        let start = Instant::now();
        motion.begin(WINDOW, CANVAS, false);
        motion.place(
            pane,
            rect(820.0, 200.0, 1220.0, 500.0),
            true,
            start,
            false,
            None,
        );
        motion.fling(pane, point(4000.0, 0.0), false);
        let stashed = rect(600.0, 200.0, 1000.0, 500.0);
        let mut now = start;
        let mut last = px(f32::MAX);
        for _ in 0..120 {
            now += Duration::from_millis(16);
            motion.begin(WINDOW, CANVAS, false);
            let placed = motion.place(pane, stashed, false, now, false, None);
            assert!(placed.frame.left() <= last, "{placed:?}");
            last = placed.frame.left();
        }
        assert_eq!(motion.drawn(pane), Some(stashed));
        let swung = PaneMotion::default();
        swung.begin(WINDOW, CANVAS, false);
        swung.place(
            pane,
            rect(820.0, 200.0, 1220.0, 500.0),
            true,
            start,
            false,
            None,
        );
        swung.fling(pane, point(4000.0, 0.0), true);
        swung.begin(WINDOW, CANVAS, false);
        swung.place(pane, stashed, false, start, false, None);
        swung.begin(WINDOW, CANVAS, false);
        let out = swung.place(
            pane,
            stashed,
            false,
            start + Duration::from_millis(16),
            false,
            None,
        );
        assert!(
            out.frame.left() > px(820.0),
            "a throw elsewhere keeps its swing"
        );
    }
}
