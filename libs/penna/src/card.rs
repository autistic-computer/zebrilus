use ala::{App, ElementId, IntoElement, RenderOnce, Window};

use crate::{
    Ground, Surface, SurfaceRole,
    plate::{Plate, plate_element},
    roles,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CardLook {
    Bubble,
    Code,
    Figure,
    Image,
}

impl CardLook {
    const fn role(self) -> SurfaceRole {
        match self {
            Self::Bubble => roles::CARD_BUBBLE,
            Self::Code => roles::CARD_CODE,
            Self::Figure => roles::CARD_FIGURE,
            Self::Image => roles::CARD_IMAGE,
        }
    }
}

impl From<CardLook> for Surface {
    fn from(look: CardLook) -> Self {
        Self::new(look.role())
    }
}

#[derive(IntoElement)]
pub struct Card {
    plate: Plate,
}

impl Card {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, look: CardLook) -> Self {
        Self {
            plate: Plate::new(id, look.role()),
        }
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.plate.surface = self.plate.surface.on(ground);
        self
    }
}

plate_element!(Card);

impl RenderOnce for Card {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        self.plate.paint(cx)
    }
}
