use ala::{App, ElementId, IntoElement, RenderOnce, Role, Styled as _, Window};

use crate::{
    Ground, PaintState, Sizable, Size, Surface, SurfaceRole,
    components::plate::{Plate, plate_element},
    foundation::roles,
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
    size: Size,
}

impl Tab {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, look: TabLook) -> Self {
        let mut plate = Plate::new(id, look.role()).announce(Role::Tab);
        plate.interactive = true;
        Self {
            plate,
            size: Size::Large,
        }
    }

    #[must_use]
    pub fn state(mut self, state: PaintState) -> Self {
        self.plate = self.plate.with_choice(state);
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

impl Sizable for Tab {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

plate_element!(Tab);

impl RenderOnce for Tab {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { mut plate, size } = self;
        plate.base = plate.base.h(size.control_h());
        plate.paint(cx)
    }
}
