use ala::{
    AnyElement, App, Div, ElementId, FULL_CORNER_RADIUS, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, RenderOnce, Role, Stateful, StatefulInteractiveElement as _,
    StyleRefinement, Styled as _, Window, div, px,
};

use crate::{
    ActiveTheme as _, Ground, Layer, PaintState, Sizable, Size, Space, StyledExt as _, Surface,
    SurfaceRole,
    components::plate::{Plate, plate_element},
    foundation::{author_name, roles, row_radius},
    rems_from_px,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TabLook {
    Window,
    Page,
    Tray,
    Title,
}

impl TabLook {
    const fn role(self) -> SurfaceRole {
        match self {
            Self::Window => roles::TAB_WINDOW,
            Self::Page => roles::TAB_PAGE,
            Self::Tray => roles::TAB_TRAY,
            Self::Title => roles::BUTTON_TITLE,
        }
    }
}

impl From<TabLook> for Surface {
    fn from(look: TabLook) -> Self {
        Self::new(look.role())
    }
}

#[derive(IntoElement)]
pub struct Tab {
    plate: Plate,
    size: Size,
}

impl Tab {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, look: TabLook) -> Self {
        let mut plate = Plate::new(id, look.role()).announce(Role::Tab);
        plate.interactive = true;
        Self {
            plate,
            size: Size::Large,
        }
    }

    #[must_use]
    pub fn state(mut self, state: PaintState) -> Self {
        self.plate = self.plate.with_choice(state);
        self
    }

    #[must_use]
    pub fn focused(mut self, focused: bool) -> Self {
        self.plate.focused = focused;
        self
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.plate.surface = self.plate.surface.on(ground);
        self
    }

    #[must_use]
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.plate.interactive = interactive;
        self
    }
}

impl Sizable for Tab {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

plate_element!(Tab);

impl RenderOnce for Tab {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { mut plate, size } = self;
        plate.base = plate.base.h(size.control_h());
        plate.paint(window, cx)
    }
}

#[derive(IntoElement)]
pub struct TabStrip {
    id: ElementId,
    base: Stateful<Div>,
    look: TabLook,
    on: Option<Ground>,
    size: Size,
    children: Vec<AnyElement>,
}

impl TabStrip {
    #[must_use]
    pub fn new(id: impl Into<ElementId>, look: TabLook) -> Self {
        let id = id.into();
        Self {
            base: div().id(id.clone()),
            id,
            look,
            on: None,
            size: Size::Large,
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn on(mut self, ground: Ground) -> Self {
        self.on = Some(ground);
        self
    }

    #[must_use]
    pub fn ground(on: Ground, cx: &App) -> Ground {
        cx.theme().ground_of(Surface::new(roles::TAB_STRIP).on(on))
    }
}

impl Sizable for TabStrip {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl ala::ParentElement for TabStrip {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl ala::Styled for TabStrip {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

fn strip_radius(tab: Pixels, tab_height: Pixels, inset: Pixels) -> Pixels {
    if tab >= FULL_CORNER_RADIUS || tab >= tab_height / 2.0 {
        FULL_CORNER_RADIUS
    } else if tab > px(0.0) {
        tab + inset
    } else {
        px(0.0)
    }
}

impl RenderOnce for TabStrip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let layer = cx.theme().layer(
            Surface::new(roles::TAB_STRIP).on_maybe(self.on),
            PaintState::Rest,
        );
        let height = match self.size {
            Size::Size(value) => value,
            size => rems_from_px(size.control_px()).to_pixels(window.rem_size()),
        };
        let tab = cx
            .theme()
            .layer(Surface::from(self.look).on(layer.ground), PaintState::Rest);
        let inset = Space::S2.rems().to_pixels(window.rem_size());
        let radius = strip_radius(row_radius(&tab, height), height, inset);
        self.base
            .role(Role::TabList)
            .accessibility_id(author_name(&self.id))
            .flex()
            .items_center()
            .gap(inset)
            .p(inset)
            .layer(&Layer {
                radius,
                share: None,
                ..layer
            })
            .children(self.children)
    }
}
