use ala::{AbsoluteLength, DefiniteLength, Length, Rems};

use super::rems_from_px;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Space {
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
    S8,
    S12,
    S16,
}

impl Space {
    pub const ALL: [Self; 9] = [
        Self::S1,
        Self::S2,
        Self::S3,
        Self::S4,
        Self::S5,
        Self::S6,
        Self::S8,
        Self::S12,
        Self::S16,
    ];

    pub const fn steps(self) -> u8 {
        match self {
            Self::S1 => 1,
            Self::S2 => 2,
            Self::S3 => 3,
            Self::S4 => 4,
            Self::S5 => 5,
            Self::S6 => 6,
            Self::S8 => 8,
            Self::S12 => 12,
            Self::S16 => 16,
        }
    }

    pub const fn design_px(self) -> f32 {
        self.steps() as f32 * STEP_PX
    }

    pub const fn rems(self) -> Rems {
        rems_from_px(self.design_px())
    }
}

const STEP_PX: f32 = 2.0;

impl From<Space> for Rems {
    fn from(space: Space) -> Self {
        space.rems()
    }
}

impl From<Space> for AbsoluteLength {
    fn from(space: Space) -> Self {
        space.rems().into()
    }
}

impl From<Space> for DefiniteLength {
    fn from(space: Space) -> Self {
        space.rems().into()
    }
}

impl From<Space> for Length {
    fn from(space: Space) -> Self {
        space.rems().into()
    }
}

#[cfg(test)]
mod tests {
    use ala::px;

    use super::*;

    #[test]
    fn steps_scale_with_the_root_rem() {
        let at = |space: Space, rem: f32| space.rems().to_pixels(px(rem));
        assert_eq!(at(Space::S1, 16.0), px(2.0));
        assert_eq!(at(Space::S5, 16.0), px(10.0));
        assert_eq!(at(Space::S16, 16.0), px(32.0));
        assert_eq!(at(Space::S4, 20.0), px(10.0));
    }
}
