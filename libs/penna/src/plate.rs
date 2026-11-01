use ala::{
    App, Div, ElementId, InteractiveElement as _, ParentElement as _, Pixels, SharedString,
    Stateful, StatefulInteractiveElement as _, StyleRefinement, div,
};

use crate::{
    ActiveTheme as _, Layer, PaintState, StyledExt as _, Surface, SurfaceRole,
    foundation::{restyle, row_corners, row_radius},
    touch::{CoarsePointer, PressCorners, press_highlight},
};

pub(crate) struct Plate {
    pub(crate) base: Stateful<Div>,
    pub(crate) style: StyleRefinement,
    pub(crate) surface: Surface,
    pub(crate) state: PaintState,
    pub(crate) interactive: bool,
    pub(crate) row_height: Option<Pixels>,
    pub(crate) hover_group: Option<SharedString>,
    id: ElementId,
}

impl Plate {
    pub(crate) fn new(id: impl Into<ElementId>, role: SurfaceRole) -> Self {
        let id = id.into();
        Self {
            base: div().id(id.clone()),
            style: StyleRefinement::default(),
            surface: Surface::new(role),
            state: PaintState::Rest,
            interactive: false,
            row_height: None,
            hover_group: None,
            id,
        }
    }

    pub(crate) fn paint(self, cx: &App) -> Stateful<Div> {
        self.paint_with(cx, |_| {})
    }

    pub(crate) fn paint_with(self, cx: &App, adjust: impl Fn(&mut Layer)) -> Stateful<Div> {
        let Self {
            base,
            style,
            surface,
            state,
            interactive,
            row_height,
            hover_group,
            id,
        } = self;
        let theme = cx.theme();
        let layer_at = |state| {
            let mut layer = theme.layer(surface, state);
            adjust(&mut layer);
            layer
        };
        let layer = layer_at(state);
        let plate = base.layer(&layer);
        let plate = match row_height {
            Some(height) => row_corners(plate, &layer, height),
            None => plate,
        };
        let lively = interactive && matches!(state, PaintState::Rest | PaintState::Focused);
        let press = (interactive && hover_group.is_some() && CoarsePointer::get(cx)).then(|| {
            let pressed = layer_at(PaintState::Active);
            let corners = match row_height {
                Some(height) => PressCorners::Row(row_radius(&pressed, height)),
                None => PressCorners::Adaptive(pressed.radius),
            };
            press_highlight(id, pressed.fill, corners)
        });
        let plate = match (lively, hover_group) {
            (false, _) => plate,
            (true, Some(group)) => {
                let hover = layer_at(PaintState::Hover);
                plate.group_hover(group, move |style| restyle(style, &hover))
            }
            (true, None) => {
                let hover = layer_at(PaintState::Hover);
                let active = layer_at(PaintState::Active);
                plate
                    .hover(move |style| restyle(style, &hover))
                    .active(move |style| restyle(style, &active))
            }
        };
        plate.refine_style(&style).children(press)
    }
}

macro_rules! plate_element {
    ($name:ident) => {
        impl ala::ParentElement for $name {
            fn extend(&mut self, elements: impl IntoIterator<Item = ala::AnyElement>) {
                self.plate.base.extend(elements);
            }
        }

        $crate::plate::plate_element!($name, children);
    };
    ($name:ident, children) => {
        impl ala::Styled for $name {
            fn style(&mut self) -> &mut ala::StyleRefinement {
                &mut self.plate.style
            }
        }

        impl ala::InteractiveElement for $name {
            fn interactivity(&mut self) -> &mut ala::Interactivity {
                self.plate.base.interactivity()
            }
        }

        impl ala::StatefulInteractiveElement for $name {}
    };
}

pub(crate) use plate_element;

#[cfg(test)]
mod tests {
    use ala::{
        Context, IntoElement, ParentElement as _, Render, Styled as _, TestAppContext, Window, div,
        px, solid_background,
    };

    use crate::{ActiveTheme as _, PaintState, Tile, TileVariant};

    struct Mini;

    impl Render for Mini {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                Tile::new("mini")
                    .variant(TileVariant::Mini)
                    .state(PaintState::Selected)
                    .size(px(40.0))
                    .bg(cx.theme().danger),
            )
        }
    }

    #[ala::test]
    fn the_callers_fill_lands_over_the_look(cx: &mut TestAppContext) {
        cx.update(crate::init_without_theme);
        let (_, cx) = cx.add_window_view(|_, _| Mini);
        cx.update(|window, cx| {
            _ = window.draw(cx);
            let fill = solid_background(cx.theme().danger);
            assert!(
                window
                    .painted_quads()
                    .iter()
                    .any(|quad| quad.background == fill),
                "the mini tile's look painted over the caller's fill"
            );
        });
    }
}
