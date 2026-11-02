use ala::{
    AnyElement, App, ElementId, IntoElement, ParentElement, Pixels, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _,
};

use crate::{
    ActiveTheme as _, Edge, Ground, Label, PaintState, RuleLook, Space, StyledExt as _, Surface,
    SurfaceRole, TextStyle, Tone, h_flex,
    plate::{Plate, plate_element},
    roles, v_flex,
};

#[derive(IntoElement, Default)]
pub struct Well {
    refinement: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Well {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn ground(cx: &App) -> Ground {
        cx.theme().ground_of(Surface::new(SurfaceRole::WELL))
    }
}

impl ParentElement for Well {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Well {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.refinement
    }
}

impl RenderOnce for Well {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .surface(Surface::new(SurfaceRole::WELL), cx)
            .refine_style(&self.refinement)
            .children(self.children)
    }
}

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
        self.plate.state = state;
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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut plate = self.plate;
        if self.variant != TileVariant::Veil {
            let edge = self.edge;
            return plate.paint_with(cx, move |layer| {
                if let Some(width) = edge.filter(|_| layer.border_width > Pixels::ZERO) {
                    layer.border_width = width;
                }
            });
        }
        let ground = plate.surface.ground.unwrap_or_else(|| cx.theme().ground());
        plate.surface = plate.surface.on(Ground {
            glass: true,
            ..ground
        });
        plate.state = PaintState::Hover;
        plate.paint_with(cx, |layer| layer.border_width = Pixels::ZERO)
    }
}

#[derive(IntoElement)]
pub struct Dock {
    title: SharedString,
    subtitle: Option<SharedString>,
    action: Option<AnyElement>,
    header_height: Option<Pixels>,
    refinement: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Dock {
    #[must_use]
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            action: None,
            header_height: None,
            refinement: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    #[must_use]
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    #[must_use]
    pub fn header_height(mut self, height: Pixels) -> Self {
        self.header_height = Some(height);
        self
    }
}

impl ParentElement for Dock {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Dock {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.refinement
    }
}

impl RenderOnce for Dock {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .rule(RuleLook::Bar, Edge::Top, cx)
            .refine_style(&self.refinement)
            .child(
                h_flex()
                    .when_some(self.header_height, Styled::h)
                    .px(Space::S8)
                    .gap(Space::S6)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(Label::new(TextStyle::Body, self.title).strong())
                            .children(
                                self.subtitle
                                    .map(|subtitle| Label::new(TextStyle::Small, subtitle).muted()),
                            ),
                    )
                    .children(self.action),
            )
            .children(self.children)
    }
}
