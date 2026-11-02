use crate::{Kind, Surface, SurfaceRole, roles};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContentLook {
    Pane,
    FloatingPane,
    Pip,
    TileStack,
    Window,
}

impl ContentLook {
    const fn role(self) -> SurfaceRole {
        match self {
            Self::Pane => Kind::Pane.role(),
            Self::FloatingPane => roles::FLOATING_PANE,
            Self::Pip => roles::FLOATING_PIP,
            Self::TileStack => roles::TILE_STACK,
            Self::Window => roles::WINDOW,
        }
    }
}

impl From<ContentLook> for Surface {
    fn from(look: ContentLook) -> Self {
        Self::new(look.role())
    }
}
