use ala::{App, ElementId, IntoElement, RenderOnce, Window};

use crate::{
    Ground, PaintState, Surface, SurfaceRole,
    plate::{Plate, plate_element},
    roles,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TabLook {
    Window,
    Page,
    Tray,
    Title,
}

impl TabLook {
    const fn role(self) -> SurfaceRole {
        match self {
            Self::Window => roles::TAB_WINDOW,
            Self::Page => roles::TAB_PAGE,
            Self::Tray => roles::TAB_TRAY,
            Self::Title => roles::BUTTON_TITLE,
        }
    }
}

impl From<TabLook> for Surface {
    fn from(look: TabLook) -> Self {
        Self::new(look.role())
    }
}

#[derive(IntoElement)]
pub struct Tab {
    plate: Plate,
}

impl Tab {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, look: TabLook) -> Self {
        let mut plate = Plate::new(id, look.role());
        plate.interactive = true;
        Self { plate }
    }

    #[must_use]
    pub fn state(mut self, state: PaintState) -> Self {
        self.plate.state = state;
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.plate.surface = self.plate.surface.on(ground);
        self
    }

    #[must_use]
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.plate.interactive = interactive;
        self
    }
}

plate_element!(Tab);

impl RenderOnce for Tab {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        self.plate.paint(cx)
    }
}
