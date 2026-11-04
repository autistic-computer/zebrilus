use crate::Space;
use ala::{
    AnyElement, App, ClickEvent, Div, ElementId, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, RenderOnce, Role, Stateful,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _,
};

use crate::{
    ActiveTheme as _, AuthoredElement as _, Disableable, PaintState, RowLook, Selectable, Size,
    StyledExt as _, Surface, SurfaceRole, TextStyle,
    foundation::{restyle, row_corners, row_state},
    h_flex, rems_from_px,
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type MouseDownHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;
type MouseMoveHandler = Box<dyn Fn(&MouseMoveEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct ListItem {
    base: Stateful<Div>,
    style: StyleRefinement,
    disabled: bool,
    selected: bool,
    surface_role: Option<SurfaceRole>,
    row_height: Option<Pixels>,
    on_click: Option<ClickHandler>,
    on_mouse_down: Vec<(MouseButton, MouseDownHandler)>,
    on_mouse_enter: Option<MouseMoveHandler>,
    children: Vec<AnyElement>,
}

impl ListItem {
    #[must_use]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: h_flex().author_id(id).role(Role::ListItem),
            style: StyleRefinement::default(),
            disabled: false,
            selected: false,
            surface_role: None,
            row_height: None,
            on_click: None,
            on_mouse_down: Vec::new(),
            on_mouse_enter: None,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    #[must_use]
    pub fn role(mut self, role: Role) -> Self {
        self.base = self.base.role(role);
        self
    }

    #[must_use]
    pub(crate) fn surface_role(mut self, role: SurfaceRole) -> Self {
        self.surface_role = Some(role);
        self
    }

    #[must_use]
    pub fn look(self, look: RowLook) -> Self {
        self.surface_role(look.role())
    }

    #[must_use]
    pub fn row_height(mut self, height: Pixels) -> Self {
        self.row_height = Some(height);
        self
    }

    #[must_use]
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    #[must_use]
    pub fn on_mouse_down(
        mut self,
        button: MouseButton,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_mouse_down.push((button, Box::new(handler)));
        self
    }

    #[must_use]
    pub fn on_mouse_enter(
        mut self,
        handler: impl Fn(&MouseMoveEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_mouse_enter = Some(Box::new(handler));
        self
    }
}

impl Disableable for ListItem {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for ListItem {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl Styled for ListItem {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for ListItem {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ListItem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            base,
            style,
            disabled,
            selected,
            surface_role,
            row_height,
            on_click,
            on_mouse_down,
            on_mouse_enter,
            children,
        } = self;

        let row_height = row_height.unwrap_or_else(|| {
            rems_from_px(Size::Medium.control_px()).to_pixels(window.rem_size())
        });
        let theme = cx.theme();
        let surface = Surface::new(surface_role.unwrap_or(SurfaceRole::MENU_ITEM));
        let state = row_state(selected, !disabled);
        let layer = theme.layer(surface, state);
        let lively = state == PaintState::Rest;

        base.aria_selected(selected)
            .aria_disabled(disabled)
            .relative()
            .justify_between()
            .gap_x(Space::S4)
            .min_h(row_height)
            .px(Space::S12)
            .type_style(TextStyle::Lead, cx)
            .layer(&layer)
            .map(|this| row_corners(this, &layer, row_height))
            .refine_style(&style)
            .when(!disabled, |this| {
                this.when_some(on_click, |this, on_click| this.on_click(on_click))
                    .when_some(on_mouse_enter, |this, on_mouse_enter| {
                        this.on_mouse_move(on_mouse_enter)
                    })
                    .map(|this| {
                        on_mouse_down
                            .into_iter()
                            .fold(this, |this, (button, handler)| {
                                this.on_mouse_down(button, handler)
                            })
                    })
            })
            .when(lively, |this| {
                let hover = theme.layer(surface, PaintState::Hover);
                let active = theme.layer(surface, PaintState::Active);
                this.hover(move |style| restyle(style, &hover))
                    .active(move |style| restyle(style, &active))
            })
            .child(div().w_full().children(children))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Highlight, StylePatch};
    use ala::{Context, IntoElement, Render, TestAppContext, solid_background};

    const TINTED: SurfaceRole =
        SurfaceRole::under(&SurfaceRole::MENU_ITEM, "test.row", "Tinted rows", "")
            .with(StylePatch::EMPTY.selected(Highlight::Tint));

    struct Rows;

    impl Render for Rows {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(ListItem::new("plain").selected(true).child("Plain"))
                .child(
                    ListItem::new("tinted")
                        .surface_role(TINTED)
                        .selected(true)
                        .child("Tinted"),
                )
        }
    }

    #[ala::test]
    fn a_row_roles_selected_highlight_reaches_the_selected_row(cx: &mut TestAppContext) {
        cx.update(crate::init_without_theme);
        let (_, cx) = cx.add_window_view(|_, _| Rows);
        cx.update(|window, cx| {
            _ = window.draw(cx);
            let theme = cx.theme();
            let tinted = theme.layer(Surface::new(TINTED), PaintState::Selected).fill;
            let accent = theme
                .layer(Surface::new(SurfaceRole::MENU_ITEM), PaintState::Selected)
                .fill;
            assert_eq!(accent, theme.selection_background());
            assert_ne!(tinted, accent);
            let quads = window.painted_quads();
            for fill in [accent, tinted] {
                assert!(
                    quads
                        .iter()
                        .any(|quad| quad.background == solid_background(fill)),
                    "no row painted {fill:?}"
                );
            }
        });
    }
}
