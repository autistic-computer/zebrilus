use ala::{
    App, ElementId, IntoElement, Refineable as _, RenderOnce, Role, StyleRefinement, Styled as _,
    Window,
};

use crate::{
    ActiveTheme as _, Ground, Kind, Space, Surface, SurfaceRole,
    components::plate::{Plate, plate_element},
    foundation::roles,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BarLook {
    #[default]
    Plain,
    Pane,
    Window,
}

impl BarLook {
    const fn announced(self) -> Role {
        match self {
            Self::Plain | Self::Pane => Role::Toolbar,
            Self::Window => Role::TitleBar,
        }
    }

    const fn role(self) -> SurfaceRole {
        match self {
            Self::Plain => Kind::Bar.role(),
            Self::Pane => roles::BAR_PANE,
            Self::Window => roles::TITLEBAR,
        }
    }
}

impl From<BarLook> for Surface {
    fn from(look: BarLook) -> Self {
        Self::new(look.role())
    }
}

#[derive(IntoElement)]
pub struct Bar {
    plate: Plate,
    look: BarLook,
}

impl Bar {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, look: BarLook) -> Self {
        Self {
            plate: Plate::new(id, look.role()).announce(look.announced()),
            look,
        }
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.plate.surface = self.plate.surface.on(ground);
        self
    }
}

plate_element!(Bar);

impl RenderOnce for Bar {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let height = match self.look {
            BarLook::Plain | BarLook::Pane => theme.pane_header_height(),
            BarLook::Window => theme.title_bar_height(),
        };
        let mut style = StyleRefinement::default().h(height).px(Space::INSET_CHROME);
        style.refine(&self.plate.style);
        self.plate.style = style;
        self.plate.paint(window, cx)
    }
}

#[cfg(test)]
mod tests {
    use ala::{
        Context, IntoElement, ParentElement as _, Render, ScaledPixels, Styled as _,
        TestAppContext, Window, blue, div, green, px, red, solid_background,
    };

    use super::{Bar, BarLook};

    struct Bars;

    impl Render for Bars {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    Bar::new("own", BarLook::Pane)
                        .w(px(200.0))
                        .bg(red())
                        .child(div().size(px(10.0)).bg(blue())),
                )
                .child(Bar::new("callers", BarLook::Plain).h(px(30.0)).bg(green()))
        }
    }

    #[ala::test]
    fn a_bar_sets_its_height_and_inset_until_the_caller_does(cx: &mut TestAppContext) {
        cx.update(crate::init_without_theme);
        let (_, cx) = cx.add_window_view(|_, _| Bars);
        cx.update(|window, cx| {
            _ = window.draw(cx);
            let quads = window.painted_quads();
            let bounds = |color| {
                quads
                    .iter()
                    .find(|quad| quad.background == solid_background(color))
                    .map(|quad| quad.bounds)
                    .expect("painted")
            };
            assert_eq!(bounds(red()).size.height, ScaledPixels(80.0));
            assert_eq!(bounds(blue()).origin.x, ScaledPixels(12.0));
            assert_eq!(bounds(green()).size.height, ScaledPixels(60.0));
        });
    }
}
