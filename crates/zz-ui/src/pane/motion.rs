use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

use ala::{
    AnyElement, App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, Size, SpringConfig, SpringState,
    Style, Window, point, px, relative, size,
};
use web_time::Instant;
use zz_protocol::{PaneId, WindowId};

const SPRING: SpringConfig = SpringConfig::new(784.0, 56.0, 1.0);
const SETTLED: f32 = 0.5;
const OUTWARD: f32 = 128.0;

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
}

#[derive(Clone, Copy, Debug)]
struct Motion {
    target: Bounds<Pixels>,
    edges: [SpringState; 4],
    at: Instant,
    placed: u64,
    last: Bounds<Pixels>,
    drawn: Bounds<Pixels>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    frame: Bounds<Pixels>,
    moving: bool,
}

impl PaneMotion {
    pub fn begin(&self, window: WindowId, viewport: Size<Pixels>, still: bool) {
        let mut motions = self.0.borrow_mut();
        if motions.window != Some(window) {
            motions.panes.clear();
            motions.flings.clear();
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

    fn place(
        &self,
        pane: PaneId,
        target: Bounds<Pixels>,
        held: bool,
        now: Instant,
        still: bool,
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
            let from = motions.origin(target);
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
        if held || still || motions.snap {
            motion.edges = [SpringState::default(); 4];
        } else {
            let elapsed = now.saturating_duration_since(motion.at).as_secs_f32();
            for edge in &mut motion.edges {
                *edge = SPRING.step(*edge, 0.0, elapsed);
                if SPRING.is_settled(*edge, 0.0, SETTLED) {
                    *edge = SpringState::default();
                }
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
        }
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
    }
}

pub struct MovingPane {
    pane: PaneId,
    motion: PaneMotion,
    child: AnyElement,
    held: bool,
    hold: Option<Rc<Cell<bool>>>,
}

impl MovingPane {
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
    type PrepaintState = Placed;

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
    ) -> Placed {
        let placed = self.motion.place(
            self.pane,
            bounds,
            self.held,
            cx.background_executor().now(),
            cx.reduce_motion(),
        );
        if let Some(hold) = &self.hold {
            hold.set(placed.moving);
        }
        if placed.moving {
            window.request_animation_frame();
        }
        let frame = placed.frame;
        self.child.layout_as_root(
            size(
                AvailableSpace::Definite(frame.size.width.max(Pixels::ZERO)),
                AvailableSpace::Definite(frame.size.height.max(Pixels::ZERO)),
            ),
            window,
            cx,
        );
        self.child.prepaint_at(frame.origin, window, cx);
        placed
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut (),
        _: &mut Placed,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
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
                motion.place(*pane, *target, false, now, false);
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
        assert_eq!(motion.place(pane, full, false, start, false).frame, full);
        let half = rect(0.0, 0.0, 500.0, 600.0);
        motion.begin(WINDOW, CANVAS, false);
        let first = motion.place(pane, half, false, start, false);
        assert_eq!(first.frame, full);
        assert!(first.moving);
        motion.begin(WINDOW, CANVAS, false);
        let next = motion.place(pane, half, false, start + Duration::from_millis(16), false);
        assert!(next.frame.right() < full.right() && next.frame.right() > half.right());
        settle(&motion, &[(pane, half)], start);
        assert_eq!(motion.drawn(pane), Some(half));
    }

    #[test]
    fn a_split_grows_the_new_pane_out_of_the_edge_it_shares_with_its_source() {
        let motion = PaneMotion::default();
        let (old, new) = (PaneId(1), PaneId(2));
        let start = Instant::now();
        motion.begin(WINDOW, CANVAS, false);
        motion.place(old, rect(0.0, 0.0, 1000.0, 600.0), false, start, false);
        motion.begin(WINDOW, CANVAS, false);
        let left = motion.place(old, rect(0.0, 0.0, 494.0, 600.0), false, start, false);
        let right = motion.place(new, rect(506.0, 0.0, 1000.0, 600.0), false, start, false);
        assert_eq!(left.frame.right(), px(1000.0));
        assert_eq!(right.frame.left(), px(1000.0));
        assert_eq!(right.frame.size.width, Pixels::ZERO);
        let mut now = start;
        for _ in 0..10 {
            now += Duration::from_millis(16);
            motion.begin(WINDOW, CANVAS, false);
            let left = motion.place(old, rect(0.0, 0.0, 494.0, 600.0), false, now, false);
            let right = motion.place(new, rect(506.0, 0.0, 1000.0, 600.0), false, now, false);
            assert!(left.frame.right() <= right.frame.left());
        }
    }

    #[test]
    fn a_resized_canvas_or_a_held_pane_moves_at_once() {
        let motion = PaneMotion::default();
        let pane = PaneId(1);
        let start = Instant::now();
        motion.begin(WINDOW, CANVAS, false);
        motion.place(pane, rect(0.0, 0.0, 400.0, 300.0), false, start, false);
        motion.begin(WINDOW, size(px(800.0), px(600.0)), false);
        let resized = rect(0.0, 0.0, 300.0, 300.0);
        assert_eq!(
            motion.place(pane, resized, false, start, false).frame,
            resized
        );
        motion.begin(WINDOW, size(px(800.0), px(600.0)), false);
        let held = rect(40.0, 20.0, 340.0, 320.0);
        let placed = motion.place(pane, held, true, start, false);
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
        motion.place(pane, rect(200.0, 100.0, 480.0, 280.0), true, start, false);
        motion.fling(pane, point(9000.0, 6000.0), true);
        let mut now = start;
        let mut furthest = Pixels::ZERO;
        for _ in 0..120 {
            now += Duration::from_millis(16);
            motion.begin(WINDOW, CANVAS, false);
            let placed = motion.place(pane, place, false, now, false);
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
        motion.place(pane, rect(820.0, 200.0, 1220.0, 500.0), true, start, false);
        motion.fling(pane, point(4000.0, 0.0), false);
        let stashed = rect(600.0, 200.0, 1000.0, 500.0);
        let mut now = start;
        let mut last = px(f32::MAX);
        for _ in 0..120 {
            now += Duration::from_millis(16);
            motion.begin(WINDOW, CANVAS, false);
            let placed = motion.place(pane, stashed, false, now, false);
            assert!(placed.frame.left() <= last, "{placed:?}");
            last = placed.frame.left();
        }
        assert_eq!(motion.drawn(pane), Some(stashed));
        let swung = PaneMotion::default();
        swung.begin(WINDOW, CANVAS, false);
        swung.place(pane, rect(820.0, 200.0, 1220.0, 500.0), true, start, false);
        swung.fling(pane, point(4000.0, 0.0), true);
        swung.begin(WINDOW, CANVAS, false);
        swung.place(pane, stashed, false, start, false);
        swung.begin(WINDOW, CANVAS, false);
        let out = swung.place(
            pane,
            stashed,
            false,
            start + Duration::from_millis(16),
            false,
        );
        assert!(
            out.frame.left() > px(820.0),
            "a throw elsewhere keeps its swing"
        );
    }
}
