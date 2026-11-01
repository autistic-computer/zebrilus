use ala::{App, ElementId, IntoElement, RenderOnce, Window};

use crate::{
    Ground, Kind, Surface, SurfaceRole,
    plate::{Plate, plate_element},
    roles,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BarLook {
    #[default]
    Plain,
    Pane,
    Window,
}

impl BarLook {
    const fn role(self) -> SurfaceRole {
        match self {
            Self::Plain => Kind::Bar.role(),
            Self::Pane => roles::BAR_PANE,
            Self::Window => roles::TITLEBAR,
        }
    }
}

impl From<BarLook> for Surface {
    fn from(look: BarLook) -> Self {
        Self::new(look.role())
    }
}

#[derive(IntoElement)]
pub struct Bar {
    plate: Plate,
}

impl Bar {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, look: BarLook) -> Self {
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

plate_element!(Bar);

impl RenderOnce for Bar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        self.plate.paint(cx)
    }
}
