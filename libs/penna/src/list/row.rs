use ala::{App, ElementId, IntoElement, Pixels, RenderOnce, SharedString, Window};

use crate::{
    ActiveTheme as _, Ground, PaintState, Shape, Surface, SurfaceRole, Tone,
    plate::{Plate, plate_element},
    roles,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RowLook {
    #[default]
    Menu,
    MenuCard,
    MenuTint,
    Sidebar,
    Nav,
    Cursor,
}

impl RowLook {
    pub(crate) const fn role(self) -> SurfaceRole {
        match self {
            Self::Menu => SurfaceRole::MENU_ITEM,
            Self::MenuCard => roles::MENU_ITEM_CARD,
            Self::MenuTint => roles::MENU_ITEM_TINT,
            Self::Sidebar => roles::SIDEBAR_ROW,
            Self::Nav => roles::ROW_NAV,
            Self::Cursor => roles::ROW_CURSOR,
        }
    }
}

impl From<RowLook> for Surface {
    fn from(look: RowLook) -> Self {
        Self::new(look.role())
    }
}

#[derive(IntoElement)]
pub struct Row {
    plate: Plate,
    radius: Option<Pixels>,
}

impl Row {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, look: RowLook) -> Self {
        let mut plate = Plate::new(id, look.role());
        plate.interactive = true;
        Self {
            plate,
            radius: None,
        }
    }

    #[must_use]
    pub fn state(mut self, state: PaintState) -> Self {
        self.plate.state = state;
        self
    }

    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.plate.surface = self.plate.surface.tone(tone);
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.plate.surface = self.plate.surface.on(ground);
        self
    }

    #[must_use]
    pub fn row_height(mut self, height: Pixels) -> Self {
        self.plate.row_height = Some(height);
        self
    }

    #[must_use]
    pub fn radius(mut self, radius: Pixels) -> Self {
        self.radius = Some(radius);
        self
    }

    #[must_use]
    pub fn hover_group(mut self, group: impl Into<SharedString>) -> Self {
        self.plate.hover_group = Some(group.into());
        self
    }

    #[must_use]
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.plate.interactive = interactive;
        self
    }

    #[must_use]
    pub fn press_highlight(mut self, press: bool) -> Self {
        self.plate.press = press;
        self
    }
}

plate_element!(Row);

impl RenderOnce for Row {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let role = self.plate.surface.role;
        let round = theme.style(role).shape == Shape::Round
            && theme
                .styles
                .get(role.id)
                .and_then(|patch| patch.shape)
                .is_none();
        let radius = self.radius.filter(|_| round);
        self.plate.paint_with(cx, move |layer| {
            if let Some(radius) = radius {
                layer.radius = radius;
            }
        })
    }
}
