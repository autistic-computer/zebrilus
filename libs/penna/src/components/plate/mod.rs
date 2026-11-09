use ala::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, MouseButton, ParentElement as _,
    Pixels, Refineable as _, Role, SharedString, Stateful, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, px,
};

use crate::{
    ActiveTheme as _, Emphasis, Layer, PaintState, StyledExt as _, Surface, SurfaceRole,
    blocks::touch::{CoarsePointer, PressCorners, over_hover, press_highlight},
    components::button::rings_focus,
    foundation::{author_name, focus::with_focus_ring, restyle, row_corners, row_radius},
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
    pub(crate) focused: bool,
    pub(crate) clickable: bool,
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
            focused: false,
            clickable: false,
            id,
        }
    }

    pub(crate) fn take_focus(mut self, window: &mut Window, cx: &mut App) -> Self {
        if !self.clickable || self.state == PaintState::Disabled {
            return self;
        }
        let handle = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone()
            .tab_stop(true);
        self.focused |= handle.is_focused(window);
        self.base = self
            .base
            .track_focus(&handle)
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default());
        self
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

    pub(crate) fn paint(self, window: &Window, cx: &App) -> Stateful<Div> {
        self.paint_with(window, cx, |_| {})
    }

    pub(crate) fn paint_with(
        self,
        window: &Window,
        cx: &App,
        adjust: impl Fn(&mut Layer),
    ) -> Stateful<Div> {
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
            focused,
            clickable,
            id,
        } = self;
        let theme = cx.theme();
        let layer_at = |state| {
            let mut layer = theme.layer(surface, state);
            adjust(&mut layer);
            layer
        };
        let ringed = focused || (state == PaintState::Focused && rings_focus(surface.role));
        let layer = layer_at(state);
        let ground = surface.ground.unwrap_or_else(|| theme.ground());
        let focus = theme.paint_on(ground, surface.tone, Emphasis::Ghost);
        let smoothing = layer.smoothing;
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
                None => match pressed.share {
                    Some(share) => PressCorners::Share(share, pressed.smoothing),
                    None => PressCorners::Adaptive(pressed.radius),
                },
            };
            let shown = if lively { PaintState::Hover } else { state };
            press_highlight(id, over_hover(layer_at(shown).fill, pressed.fill), corners)
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
                press
                    .is_none()
                    .then(|| restyle(StyleRefinement::default(), &layer_at(PaintState::Active))),
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
        let plate = plate
            .when(ringed, Styled::relative)
            .when(clickable, |plate| {
                if state == PaintState::Disabled {
                    plate.cursor_default()
                } else {
                    plate.click_cursor()
                }
            })
            .refine_style(&style)
            .children(press)
            .children(children);
        with_focus_ring(plate, ringed, px(0.), &focus, smoothing, window)
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

        $crate::components::plate::plate_element!($name, children);
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

            fn on_click(
                mut self,
                listener: impl Fn(&ala::ClickEvent, &mut ala::Window, &mut ala::App) + 'static,
            ) -> Self
            where
                Self: Sized,
            {
                self.plate.clickable = true;
                ala::InteractiveElement::interactivity(&mut self.plate.base).on_click(listener);
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
    use std::{cell::RefCell, rc::Rc};

    use crate::Space;
    use ala::{
        Context, InteractiveElement as _, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke,
        Modifiers, ParentElement as _, Render, Role, ScaledPixels, StatefulInteractiveElement as _,
        StyleRefinement, Styled as _, TestAppContext, VisualTestContext, Window, blue, div, point,
        px, red, solid_background,
    };

    use crate::{
        ActiveTheme as _, Chip, Emphasis, PaintState, Row, RowLook, Surface, Tab, TabLook, Tile,
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
            let row = |id: &'static str| {
                Row::new(id, RowLook::Menu)
                    .row_height(px(28.0))
                    .w(px(120.0))
                    .h(px(28.0))
            };
            div()
                .size_full()
                .p(Space::S10)
                .flex()
                .flex_col()
                .gap(Space::S10)
                .child(row("focused").state(PaintState::Focused))
                .child(row("picked").state(PaintState::Selected).focused(true))
        }
    }

    #[ala::test]
    fn a_focused_row_draws_the_ring_and_keeps_its_own_fill(cx: &mut TestAppContext) {
        cx.update(crate::init_without_theme);
        let (_, cx) = cx.add_window_view(|_, _| Focused);
        cx.update(|window, cx| {
            _ = window.draw(cx);
            let theme = cx.theme();
            let row = Surface::new(RowLook::Menu.role());
            let selected = solid_background(theme.layer(row, PaintState::Selected).fill);
            let focus = theme
                .paint_on(theme.ground(), Tone::Neutral, Emphasis::Ghost)
                .focus;
            let quads = window.painted_quads();
            assert_eq!(
                quads
                    .iter()
                    .filter(|quad| quad.background == selected)
                    .count(),
                1,
                "only the selected row paints the selected fill"
            );
            let mut rings = quads
                .iter()
                .filter(|quad| quad.border_color == focus)
                .map(|quad| {
                    (
                        quad.bounds.origin.y,
                        quad.bounds.size.height,
                        quad.border_widths.top,
                    )
                })
                .collect::<Vec<_>>();
            rings.dedup();
            assert_eq!(
                rings,
                [
                    (ScaledPixels(17.0), ScaledPixels(62.0), ScaledPixels(3.0)),
                    (ScaledPixels(93.0), ScaledPixels(62.0), ScaledPixels(3.0)),
                ]
            );
        });
    }

    struct Keyed {
        clicks: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Render for Keyed {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let (chip, tile) = (Rc::clone(&self.clicks), Rc::clone(&self.clicks));
            div()
                .size_full()
                .p(Space::S10)
                .flex()
                .gap(Space::S10)
                .child(
                    Chip::new("chip", "Ready")
                        .interactive(true)
                        .on_click(move |_, _, _| chip.borrow_mut().push("chip")),
                )
                .child(
                    Tile::new("tile")
                        .size(px(40.0))
                        .on_click(move |_, _, _| tile.borrow_mut().push("tile")),
                )
        }
    }

    fn press(cx: &mut VisualTestContext, key: &str) {
        let keystroke = Keystroke::parse(key).expect("a key name");
        cx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(KeyUpEvent { keystroke });
    }

    fn rings(cx: &mut VisualTestContext) -> usize {
        cx.update(|window, cx| {
            _ = window.draw(cx);
            let theme = cx.theme();
            let focus = theme
                .paint_on(theme.ground(), Tone::Neutral, Emphasis::Ghost)
                .focus;
            let mut rings = window
                .painted_quads()
                .into_iter()
                .filter(|quad| quad.border_color == focus)
                .map(|quad| quad.bounds)
                .collect::<Vec<_>>();
            rings.dedup();
            rings.len()
        })
    }

    #[ala::test]
    fn clickable_chips_and_tiles_take_focus_from_the_keyboard_only(cx: &mut TestAppContext) {
        cx.update(crate::init_without_theme);
        let clicks = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let clicks = Rc::clone(&clicks);
            move |_, _| Keyed { clicks }
        });
        assert_eq!(rings(cx), 0);
        cx.simulate_click(point(px(20.0), px(20.0)), Modifiers::default());
        assert_eq!(rings(cx), 0, "a click focused the chip");
        cx.update(Window::focus_next);
        assert_eq!(rings(cx), 1);
        press(cx, "enter");
        cx.update(Window::focus_next);
        assert_eq!(rings(cx), 1);
        press(cx, "space");
        assert_eq!(*clicks.borrow(), ["chip", "chip", "tile"]);
    }
}
