use ala::{AbsoluteLength, DefiniteLength, Length, Rems};

use super::rems_from_px;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Space {
    S2,
    S4,
    S6,
    S8,
    S10,
    S12,
    S16,
    S24,
    S32,
    S40,
    S64,
}

impl Space {
    pub const ALL: [Self; 11] = [
        Self::S2,
        Self::S4,
        Self::S6,
        Self::S8,
        Self::S10,
        Self::S12,
        Self::S16,
        Self::S24,
        Self::S32,
        Self::S40,
        Self::S64,
    ];

    pub const GAP_TIGHT: Self = Self::S4;
    pub const GAP_RELATED: Self = Self::S8;
    pub const GAP_GROUP: Self = Self::S16;
    pub const GAP_SECTION: Self = Self::S32;

    pub const INSET_CHROME: Self = Self::S6;
    pub const INSET_CARD: Self = Self::S12;
    pub const INSET_PANE: Self = Self::S16;
    pub const INSET_PAGE: Self = Self::S32;

    pub const GAPS: [(&'static str, Self); 4] = [
        ("tight", Self::GAP_TIGHT),
        ("related", Self::GAP_RELATED),
        ("group", Self::GAP_GROUP),
        ("section", Self::GAP_SECTION),
    ];

    pub const INSETS: [(&'static str, Self); 4] = [
        ("chrome", Self::INSET_CHROME),
        ("card", Self::INSET_CARD),
        ("pane", Self::INSET_PANE),
        ("page", Self::INSET_PAGE),
    ];

    pub const fn design_px(self) -> f32 {
        match self {
            Self::S2 => 2.0,
            Self::S4 => 4.0,
            Self::S6 => 6.0,
            Self::S8 => 8.0,
            Self::S10 => 10.0,
            Self::S12 => 12.0,
            Self::S16 => 16.0,
            Self::S24 => 24.0,
            Self::S32 => 32.0,
            Self::S40 => 40.0,
            Self::S64 => 64.0,
        }
    }

    pub const fn rems(self) -> Rems {
        rems_from_px(self.design_px())
    }
}

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
    fn lengths_scale_with_the_root_rem() {
        let at = |space: Space, rem: f32| space.rems().to_pixels(px(rem));
        assert_eq!(at(Space::S2, 16.0), px(2.0));
        assert_eq!(at(Space::S10, 16.0), px(10.0));
        assert_eq!(at(Space::S64, 16.0), px(64.0));
        assert_eq!(at(Space::S8, 20.0), px(10.0));
    }
}
