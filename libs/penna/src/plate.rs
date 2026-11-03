use ala::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, ParentElement as _, Pixels,
    Refineable as _, Role, SharedString, Stateful, StatefulInteractiveElement as _,
    StyleRefinement, Styled, div, prelude::FluentBuilder as _,
};

use crate::{
    ActiveTheme as _, Emphasis, FOCUS_RING_WIDTH, Layer, PaintState, StyledExt as _, Surface,
    SurfaceRole,
    button::rings_focus,
    foundation::{author_name, restyle, row_corners, row_radius},
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
    pub(crate) press: bool,
    pub(crate) children: Vec<AnyElement>,
    pub(crate) hover: Option<StyleRefinement>,
    pub(crate) active: Option<StyleRefinement>,
    pub(crate) role: Option<Role>,
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
            press: false,
            children: Vec::new(),
            hover: None,
            active: None,
            role: None,
            id,
        }
    }

    pub(crate) fn announce(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    pub(crate) fn with_state(mut self, state: PaintState) -> Self {
        self.state = state;
        self.base = self.base.aria_disabled(state == PaintState::Disabled);
        self
    }

    pub(crate) fn with_choice(mut self, state: PaintState) -> Self {
        self.base = self
            .base
            .aria_selected(matches!(state, PaintState::Selected | PaintState::Current));
        self.with_state(state)
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
            press,
            children,
            hover: caller_hover,
            active: caller_active,
            role,
            id,
        } = self;
        let theme = cx.theme();
        let layer_at = |state| {
            let mut layer = theme.layer(surface, state);
            adjust(&mut layer);
            layer
        };
        let ringed = state == PaintState::Focused && rings_focus(surface.role);
        let layer = layer_at(if ringed { PaintState::Rest } else { state });
        let ring = ringed.then(|| {
            let ground = surface.ground.unwrap_or_else(|| theme.ground());
            let paint = theme.paint_on(ground, surface.tone, Emphasis::Ghost);
            let radius = row_height.map_or(layer.radius, |height| row_radius(&layer, height));
            let inset = -(FOCUS_RING_WIDTH + layer.border_width);
            div()
                .absolute()
                .top(inset)
                .left(inset)
                .right(inset)
                .bottom(inset)
                .rounded(radius + FOCUS_RING_WIDTH)
                .when(row_height.is_some(), |ring| {
                    ring.corner_radius_mode(ala::CornerRadiusMode::Fixed)
                })
                .corner_smoothing(layer.smoothing)
                .focus_ring(&paint)
        });
        let base = match role {
            Some(role) => base.role(role).accessibility_id(author_name(&id)),
            None => base,
        };
        let plate = base.layer(&layer);
        let mut plate = match row_height {
            Some(height) => row_corners(plate, &layer, height),
            None => plate,
        };
        let lively = interactive && matches!(state, PaintState::Rest | PaintState::Focused);
        let pressable = press || (interactive && hover_group.is_some());
        let press = (pressable && CoarsePointer::get(cx)).then(|| {
            let pressed = layer_at(PaintState::Active);
            let corners = match row_height {
                Some(height) => PressCorners::Row(row_radius(&pressed, height), pressed.smoothing),
                None => PressCorners::Adaptive(pressed.radius),
            };
            press_highlight(id, pressed.fill, corners)
        });
        let (hover, active) = match (lively, hover_group) {
            (false, _) => (None, None),
            (true, Some(group)) => {
                let hover = layer_at(PaintState::Hover);
                plate = plate.group_hover(group, move |style| restyle(style, &hover));
                (None, None)
            }
            (true, None) => (
                Some(restyle(
                    StyleRefinement::default(),
                    &layer_at(PaintState::Hover),
                )),
                Some(restyle(
                    StyleRefinement::default(),
                    &layer_at(PaintState::Active),
                )),
            ),
        };
        let plate = match on_top(hover, caller_hover) {
            Some(hover) => plate.hover(move |_| hover),
            None => plate,
        };
        let plate = match on_top(active, caller_active) {
            Some(active) => plate.active(move |_| active),
            None => plate,
        };
        plate
            .when(ring.is_some(), Styled::relative)
            .refine_style(&style)
            .children(press)
            .children(ring)
            .children(children)
    }
}

fn on_top(
    look: Option<StyleRefinement>,
    caller: Option<StyleRefinement>,
) -> Option<StyleRefinement> {
    match (look, caller) {
        (Some(mut look), Some(caller)) => {
            look.refine(&caller);
            Some(look)
        }
        (look, caller) => look.or(caller),
    }
}

macro_rules! plate_element {
    ($name:ident) => {
        impl ala::ParentElement for $name {
            fn extend(&mut self, elements: impl IntoIterator<Item = ala::AnyElement>) {
                self.plate.children.extend(elements);
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

            fn hover(
                mut self,
                f: impl FnOnce(ala::StyleRefinement) -> ala::StyleRefinement,
            ) -> Self {
                self.plate.hover = Some(f(ala::StyleRefinement::default()));
                self
            }
        }

        impl ala::StatefulInteractiveElement for $name {
            fn role(mut self, role: ala::Role) -> Self {
                self.plate.role = Some(role);
                self
            }

            fn active(
                mut self,
                f: impl FnOnce(ala::StyleRefinement) -> ala::StyleRefinement,
            ) -> Self
            where
                Self: Sized,
            {
                self.plate.active = Some(f(ala::StyleRefinement::default()));
                self
            }
        }
    };
}

pub(crate) use plate_element;

#[cfg(test)]
mod tests {
    use ala::{
        Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
        ScaledPixels, StatefulInteractiveElement as _, StyleRefinement, Styled as _,
        TestAppContext, Window, blue, div, px, red, solid_background,
    };

    use crate::{
        ActiveTheme as _, Emphasis, PaintState, Row, RowLook, Surface, Tab, TabLook, Tile,
        TileVariant, Tone,
    };

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

    struct Hovered;

    impl Render for Hovered {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let danger = cx.theme().danger;
            div().size_full().child(
                Row::new("row", RowLook::Menu)
                    .size(px(40.0))
                    .hover(move |style| style.bg(danger)),
            )
        }
    }

    #[ala::test]
    fn the_callers_hover_lands_over_the_looks(cx: &mut TestAppContext) {
        cx.update(crate::init_without_theme);
        let (_, cx) = cx.add_window_view(|_, _| Hovered);
        cx.update(|window, cx| _ = window.draw(cx));
        let look = StyleRefinement::default().bg(red()).border_color(red());
        let caller = StyleRefinement::default().bg(blue());
        let hover = super::on_top(Some(look), Some(caller)).expect("both set a hover");
        assert_eq!(hover.background, Some(blue().into()));
        assert_eq!(hover.border_color, Some(red()));
    }

    struct Announced;

    impl Render for Announced {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(Tab::new(("tab", 2usize), TabLook::Page).state(PaintState::Selected))
                .child(Row::new("row", RowLook::Menu).state(PaintState::Disabled))
                .child(
                    div()
                        .id("outer")
                        .group("outer")
                        .child(Row::new("fill", RowLook::Sidebar).hover_group("outer")),
                )
                .child(
                    div().id("button").role(Role::Button).child(
                        Row::new("pressed", RowLook::Sidebar)
                            .interactive(false)
                            .press_highlight(true),
                    ),
                )
                .child(
                    Row::new("item", RowLook::Menu)
                        .hover_group("outer")
                        .role(Role::MenuItem),
                )
        }
    }

    #[ala::test]
    fn plates_announce_their_role_and_id_and_fills_stay_silent(cx: &mut TestAppContext) {
        cx.update(crate::init_without_theme);
        let (_, cx) = cx.add_window_view(|_, _| Announced);
        let nodes = cx.update(|window, cx| {
            window.set_a11y_forced(true);
            window.draw(cx).clear(cx);
            let tree = window.a11y_tree().expect("forced a11y builds a tree");
            let mut nodes = tree
                .nodes
                .iter()
                .filter_map(|(_, node)| {
                    node.author_id().map(|id| {
                        (
                            id.to_owned(),
                            node.role(),
                            node.is_selected(),
                            node.is_disabled(),
                        )
                    })
                })
                .collect::<Vec<_>>();
            nodes.sort_by(|a, b| a.0.cmp(&b.0));
            nodes
        });
        assert_eq!(
            nodes,
            [
                ("item".to_owned(), Role::MenuItem, None, false),
                ("row".to_owned(), Role::ListItem, Some(false), true),
                ("tab-2".to_owned(), Role::Tab, Some(true), false),
            ]
        );
    }

    struct Focused;

    impl Render for Focused {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().p(px(10.0)).child(
                Row::new("row", RowLook::Sidebar)
                    .state(PaintState::Focused)
                    .row_height(px(28.0))
                    .w(px(120.0))
                    .h(px(28.0)),
            )
        }
    }

    #[ala::test]
    fn a_focused_ringing_row_keeps_its_rest_fill_and_draws_the_focus_ring(cx: &mut TestAppContext) {
        cx.update(crate::init_without_theme);
        let (_, cx) = cx.add_window_view(|_, _| Focused);
        cx.update(|window, cx| {
            _ = window.draw(cx);
            let theme = cx.theme();
            let row = Surface::new(RowLook::Sidebar.role());
            let selected = theme.layer(row, PaintState::Selected).fill;
            let focus = theme
                .paint_on(theme.ground(), Tone::Neutral, Emphasis::Ghost)
                .focus;
            let quads = window.painted_quads();
            assert!(
                quads
                    .iter()
                    .all(|quad| quad.background != solid_background(selected)),
                "a focused sidebar row painted the selected fill"
            );
            let ring = quads
                .iter()
                .find(|quad| quad.border_color == focus)
                .expect("a focused sidebar row draws a focus ring");
            assert_eq!(ring.border_widths.top, ScaledPixels(3.0));
            assert_eq!(ring.bounds.size.height, ScaledPixels(62.0));
        });
    }
}
