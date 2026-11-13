use ala::{Bounds, Pixels, Point, px};

pub const STACK_HOLD_MS: u64 = 280;
pub const STACK_SLOP: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackSlide {
    Pass,
    UpOpens,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackTouch {
    Idle,
    Opened,
    Entered(usize),
    Left,
    Tap,
    Passed,
    Picked(usize),
    Kept,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackTarget {
    pub bounds: Bounds<Pixels>,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Phase {
    #[default]
    Idle,
    Pressed {
        origin: Point<Pixels>,
    },
    Holding,
    Passed,
    Open,
    Reopened,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StackGesture {
    phase: Phase,
    over: Option<usize>,
    anchor: Bounds<Pixels>,
    targets: Vec<StackTarget>,
    blocks: Vec<Bounds<Pixels>>,
}

impl StackGesture {
    pub fn is_open(&self) -> bool {
        matches!(self.phase, Phase::Holding | Phase::Open | Phase::Reopened)
    }

    pub fn is_held(&self) -> bool {
        matches!(self.phase, Phase::Holding | Phase::Reopened)
    }

    pub fn is_pressed(&self) -> bool {
        matches!(self.phase, Phase::Pressed { .. } | Phase::Holding)
    }

    pub fn over(&self) -> Option<usize> {
        self.over
    }

    pub fn set_layout(
        &mut self,
        anchor: Bounds<Pixels>,
        targets: Vec<StackTarget>,
        blocks: Vec<Bounds<Pixels>>,
    ) {
        self.anchor = anchor;
        self.targets = targets;
        self.blocks = blocks;
    }

    pub fn open(&mut self) -> StackTouch {
        if self.is_open() {
            return StackTouch::Idle;
        }
        self.phase = Phase::Open;
        self.over = None;
        StackTouch::Opened
    }

    pub fn close(&mut self) {
        self.phase = Phase::Idle;
        self.over = None;
    }

    pub fn press(&mut self, at: Point<Pixels>) -> StackTouch {
        self.over = None;
        self.phase = match self.phase {
            Phase::Open | Phase::Reopened => Phase::Reopened,
            _ => Phase::Pressed { origin: at },
        };
        StackTouch::Idle
    }

    pub fn hold(&mut self) -> StackTouch {
        if matches!(self.phase, Phase::Pressed { .. }) {
            self.phase = Phase::Holding;
            self.over = None;
            return StackTouch::Opened;
        }
        StackTouch::Idle
    }

    pub fn pass(&mut self) -> StackTouch {
        if matches!(self.phase, Phase::Pressed { .. }) {
            self.phase = Phase::Passed;
            return StackTouch::Passed;
        }
        StackTouch::Idle
    }

    pub fn track(&mut self, at: Point<Pixels>, slide: StackSlide) -> StackTouch {
        match self.phase {
            Phase::Pressed { origin } => {
                let moved = at - origin;
                if moved.magnitude() < f64::from(STACK_SLOP) {
                    return StackTouch::Idle;
                }
                if slide == StackSlide::UpOpens && -moved.y > moved.x.abs() {
                    self.phase = Phase::Holding;
                    self.over = None;
                    let entered = self.follow(at);
                    return match entered {
                        StackTouch::Entered(_) => entered,
                        _ => StackTouch::Opened,
                    };
                }
                self.phase = Phase::Passed;
                StackTouch::Passed
            }
            Phase::Holding | Phase::Reopened => self.follow(at),
            Phase::Idle | Phase::Passed | Phase::Open => StackTouch::Idle,
        }
    }

    pub fn release(&mut self, at: Point<Pixels>) -> StackTouch {
        let phase = std::mem::take(&mut self.phase);
        let over = self.over.take().or_else(|| self.target_at(at));
        match phase {
            Phase::Pressed { .. } => StackTouch::Tap,
            Phase::Holding => match over {
                Some(index) => StackTouch::Picked(index),
                None if self.anchor.contains(&at) => {
                    self.phase = Phase::Open;
                    StackTouch::Kept
                }
                None => StackTouch::Closed,
            },
            Phase::Reopened => over.map_or(StackTouch::Closed, StackTouch::Picked),
            Phase::Open => {
                self.phase = Phase::Open;
                StackTouch::Idle
            }
            Phase::Idle | Phase::Passed => StackTouch::Idle,
        }
    }

    pub fn cancel(&mut self) -> StackTouch {
        let was_open = self.is_open();
        self.close();
        if was_open {
            StackTouch::Closed
        } else {
            StackTouch::Idle
        }
    }

    fn follow(&mut self, at: Point<Pixels>) -> StackTouch {
        let over = self.target_at(at);
        if over == self.over {
            return StackTouch::Idle;
        }
        self.over = over;
        over.map_or(StackTouch::Left, StackTouch::Entered)
    }

    fn target_at(&self, at: Point<Pixels>) -> Option<usize> {
        zones(&self.targets, &self.blocks)
            .into_iter()
            .position(|zone| zone.contains(&at))
            .filter(|index| self.targets[*index].enabled)
    }
}

fn zones(targets: &[StackTarget], blocks: &[Bounds<Pixels>]) -> Vec<Bounds<Pixels>> {
    let Some(left) = targets
        .iter()
        .map(|target| target.bounds.left())
        .reduce(Ord::min)
    else {
        return Vec::new();
    };
    let right = targets
        .iter()
        .map(|target| target.bounds.right())
        .fold(left, Ord::max);
    let mut rows: Vec<(Pixels, Pixels)> = targets
        .iter()
        .map(|target| (target.bounds.top(), target.bounds.bottom()))
        .chain(blocks.iter().map(|block| (block.top(), block.bottom())))
        .collect();
    rows.sort_by_key(|row| row.0);
    rows.dedup_by(|a, b| (a.0 - b.0).abs() < px(1.0));
    let band = |top: Pixels| {
        let index = rows
            .iter()
            .position(|row| (row.0 - top).abs() < px(1.0))
            .unwrap_or_default();
        let (row_top, row_bottom) = rows[index];
        let above = index
            .checked_sub(1)
            .map_or(row_top, |above| (rows[above].1 + row_top) / 2.0);
        let below = rows
            .get(index + 1)
            .map_or(row_bottom, |below| (row_bottom + below.0) / 2.0);
        (above, below)
    };
    targets
        .iter()
        .map(|target| {
            let bounds = target.bounds;
            let (top, bottom) = band(bounds.top());
            let mut beside: Vec<Bounds<Pixels>> = targets
                .iter()
                .map(|other| other.bounds)
                .filter(|other| (other.top() - bounds.top()).abs() < px(1.0))
                .collect();
            beside.sort_by_key(Bounds::left);
            let index = beside
                .iter()
                .position(|other| *other == bounds)
                .unwrap_or_default();
            let start = index.checked_sub(1).map_or(left, |before| {
                (beside[before].right() + bounds.left()) / 2.0
            });
            let end = beside
                .get(index + 1)
                .map_or(right, |after| (bounds.right() + after.left()) / 2.0);
            Bounds::from_corners(ala::point(start, top), ala::point(end, bottom))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ala::{point, size};

    use super::*;

    fn chip(top: f32) -> StackTarget {
        StackTarget {
            bounds: Bounds::new(point(px(100.0), px(top)), size(px(160.0), px(52.0))),
            enabled: true,
        }
    }

    fn gesture() -> StackGesture {
        let mut gesture = StackGesture::default();
        gesture.set_layout(
            Bounds::new(point(px(16.0), px(700.0)), size(px(358.0), px(56.0))),
            vec![chip(632.0), chip(572.0), chip(452.0)],
            vec![Bounds::new(
                point(px(120.0), px(516.0)),
                size(px(120.0), px(28.0)),
            )],
        );
        gesture
    }

    #[test]
    fn a_hold_then_a_release_on_a_chip_picks_it() {
        let mut gesture = gesture();
        let anchor = point(px(195.0), px(728.0));
        assert_eq!(gesture.press(anchor), StackTouch::Idle);
        assert_eq!(gesture.hold(), StackTouch::Opened);
        assert_eq!(
            gesture.track(point(px(180.0), px(628.0)), StackSlide::Pass),
            StackTouch::Entered(0)
        );
        assert_eq!(
            gesture.track(point(px(180.0), px(627.0)), StackSlide::Pass),
            StackTouch::Entered(1)
        );
        assert_eq!(
            gesture.track(point(px(180.0), px(530.0)), StackSlide::Pass),
            StackTouch::Left
        );
        assert_eq!(
            gesture.track(point(px(180.0), px(480.0)), StackSlide::Pass),
            StackTouch::Entered(2)
        );
        assert_eq!(
            gesture.release(point(px(180.0), px(480.0))),
            StackTouch::Picked(2)
        );
        assert!(!gesture.is_open());

        gesture.press(anchor);
        gesture.hold();
        assert_eq!(gesture.release(anchor), StackTouch::Kept);
        assert!(gesture.is_open());
        gesture.press(point(px(180.0), px(600.0)));
        assert_eq!(
            gesture.release(point(px(180.0), px(600.0))),
            StackTouch::Picked(1)
        );
    }

    #[test]
    fn a_ten_point_move_before_the_hold_never_opens_it() {
        let mut gesture = gesture();
        let anchor = point(px(195.0), px(728.0));
        gesture.press(anchor);
        assert_eq!(
            gesture.track(point(px(201.0), px(720.0)), StackSlide::Pass),
            StackTouch::Passed
        );
        assert_eq!(gesture.hold(), StackTouch::Idle);
        assert_eq!(gesture.release(anchor), StackTouch::Idle);
        assert!(!gesture.is_open());

        gesture.press(anchor);
        assert_eq!(
            gesture.track(point(px(195.0), px(719.0)), StackSlide::Pass),
            StackTouch::Idle
        );
        assert_eq!(gesture.release(anchor), StackTouch::Tap);
        assert!(!gesture.is_open());
    }
}
