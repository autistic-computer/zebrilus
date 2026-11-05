use crate::{
    ActiveTheme as _, Colorize as _, Ground, PaintState, Surface, SurfaceRole, Tone,
    components::plate::{Plate, plate_element},
    foundation::roles,
};
use ala::{App, ElementId, IntoElement, Pixels, RenderOnce, Role, Window};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TileVariant {
    #[default]
    Card,
    Screen,
    Drop,
    Veil,
    Mini,
}

impl TileVariant {
    const fn role(self) -> SurfaceRole {
        match self {
            Self::Card => SurfaceRole::CARD_TILE,
            Self::Screen => SurfaceRole::SCREEN_TILE,
            Self::Drop | Self::Veil => roles::TILE_DROP,
            Self::Mini => roles::TILE_MINI,
        }
    }
}

impl From<TileVariant> for Surface {
    fn from(variant: TileVariant) -> Self {
        Self::new(variant.role())
    }
}

#[derive(IntoElement)]
pub struct Tile {
    plate: Plate,
    variant: TileVariant,
    edge: Option<Pixels>,
}

impl Tile {
    #[must_use]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            plate: Plate::new(id, TileVariant::Card.role()),
            variant: TileVariant::Card,
            edge: None,
        }
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.plate.surface = self.plate.surface.on(ground);
        self
    }

    #[must_use]
    pub fn variant(mut self, variant: TileVariant) -> Self {
        self.variant = variant;
        self.plate.surface.role = variant.role();
        self
    }

    #[must_use]
    pub fn tone(mut self, tone: Tone) -> Self {
        self.plate.surface = self.plate.surface.tone(tone);
        self
    }

    #[must_use]
    pub fn state(mut self, state: PaintState) -> Self {
        self.plate = self.plate.with_state(state);
        self
    }

    #[must_use]
    pub fn edge(mut self, width: Pixels) -> Self {
        self.edge = Some(width);
        self
    }
}

plate_element!(Tile);

impl RenderOnce for Tile {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut plate = self.plate;
        if self.variant != TileVariant::Veil {
            plate = plate.take_focus(window, cx);
            let role = if plate.clickable {
                Role::Button
            } else {
                Role::Group
            };
            plate.role = plate.role.or(Some(role));
            let edge = self.edge;
            let mini = self.variant == TileVariant::Mini;
            let lifted = (plate.state == PaintState::Hover && self.variant != TileVariant::Drop)
                .then(|| {
                    let theme = cx.theme();
                    let ground = plate.surface.ground.unwrap_or_else(|| theme.ground());
                    (theme.lifted_shadows(), ground.fill.washed(1))
                });
            if let Some((_, wash)) = lifted
                && let Some(fill) = plate
                    .style
                    .background
                    .as_ref()
                    .and_then(ala::Fill::color)
                    .and_then(|fill| fill.as_solid())
            {
                plate.style.background = Some(fill.blend(wash).into());
            }
            return plate.paint_with(window, cx, move |layer| {
                if let Some(width) = edge.filter(|_| layer.border_width > Pixels::ZERO) {
                    layer.border_width = width;
                }
                if mini {
                    layer.glass = None;
                }
                if let Some((float, wash)) = &lifted {
                    layer.fill = layer.fill.blend(*wash);
                    layer.shadow.retain(|shadow| shadow.inset);
                    layer.shadow.extend(float.iter().cloned());
                }
            });
        }
        let ground = plate.surface.ground.unwrap_or_else(|| cx.theme().ground());
        plate.surface = plate.surface.on(Ground {
            glass: true,
            ..ground
        });
        plate.state = PaintState::Hover;
        plate.paint_with(window, cx, |layer| layer.border_width = Pixels::ZERO)
    }
}
