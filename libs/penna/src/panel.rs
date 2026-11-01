use ala::{App, ElementId, IntoElement, Pixels, RenderOnce, Window};

use crate::{
    Kind, Surface, SurfaceRole,
    picker::outer_radius,
    plate::{Plate, plate_element},
    roles,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelLook {
    Popover,
    Inline,
    Picker,
    Toast,
    Plain,
}

impl PanelLook {
    const fn role(self) -> SurfaceRole {
        match self {
            Self::Popover => SurfaceRole::POPOVER,
            Self::Inline => roles::INLINE_PANEL,
            Self::Picker => roles::PICKER,
            Self::Toast => SurfaceRole::TOAST,
            Self::Plain => Kind::Panel.role(),
        }
    }
}

impl From<PanelLook> for Surface {
    fn from(look: PanelLook) -> Self {
        Self::new(look.role())
    }
}

#[derive(IntoElement)]
pub struct Panel {
    plate: Plate,
    look: PanelLook,
    inset: Option<Pixels>,
}

impl Panel {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, look: PanelLook) -> Self {
        Self {
            plate: Plate::new(id, look.role()),
            look,
            inset: None,
        }
    }

    #[must_use]
    pub fn inset(mut self, inset: Pixels) -> Self {
        self.inset = Some(inset);
        self
    }
}

plate_element!(Panel);

impl RenderOnce for Panel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let opaque = self.look == PanelLook::Plain;
        let inset = self.inset;
        self.plate.paint_with(cx, move |layer| {
            if opaque {
                layer.fill = layer.ground.fill;
            }
            if let Some(inset) = inset {
                layer.radius = outer_radius(layer, inset);
            }
        })
    }
}
