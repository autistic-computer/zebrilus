use super::{
    ActiveTheme, Edge, Emphasis, Layer, PaintState, StatefulSurfaceExt, Surface, SurfaceRole, Tone,
};
use ala::{
    AbsoluteLength, App, BoxShadow, DefiniteLength, Div, Edges, ElementId, FULL_CORNER_RADIUS,
    Hsla, InteractiveElement as _, Pixels, Refineable, Stateful, StatefulInteractiveElement,
    StyleRefinement, Styled, div, px,
};

use super::rems_from_px;

#[inline(always)]
pub fn h_flex() -> Div {
    div().h_flex()
}

#[inline(always)]
pub fn v_flex() -> Div {
    div().v_flex()
}

macro_rules! font_weight {
    ($fn:ident, $const:ident) => {
        #[inline]
        fn $fn(self) -> Self {
            self.font_weight(ala::FontWeight::$const)
        }
    };
}

pub trait StyledExt: Styled + Sized {
    fn refine_style(mut self, style: &StyleRefinement) -> Self {
        self.style().refine(style);
        self
    }

    #[inline(always)]
    fn h_flex(self) -> Self {
        self.flex().flex_row().items_center()
    }

    #[inline(always)]
    fn v_flex(self) -> Self {
        self.flex().flex_col()
    }

    fn paddings<L>(self, paddings: impl Into<Edges<L>>) -> Self
    where
        L: Into<DefiniteLength> + Clone + Default + std::fmt::Debug + PartialEq,
    {
        let paddings = paddings.into();
        self.pt(paddings.top.into())
            .pb(paddings.bottom.into())
            .pl(paddings.left.into())
            .pr(paddings.right.into())
    }

    fn margins<L>(self, margins: impl Into<Edges<L>>) -> Self
    where
        L: Into<DefiniteLength> + Clone + Default + std::fmt::Debug + PartialEq,
    {
        let margins = margins.into();
        self.mt(margins.top.into())
            .mb(margins.bottom.into())
            .ml(margins.left.into())
            .mr(margins.right.into())
    }

    fn surface_outline(self, color: Hsla, cx: &App) -> Self {
        let this = self.border_color(color);
        if cx.theme().outlines_edges() {
            this.border(cx.theme().outline)
        } else {
            this
        }
    }

    fn surface_edge(self, cx: &App) -> Self {
        self.surface_outline(cx.theme().border(), cx)
    }

    fn list_inset(self, cx: &App) -> Self {
        let this = self.px(cx.theme().row_inset);
        if cx.theme().inset_rows() {
            this.gap_y_0p5()
        } else {
            this
        }
    }

    fn focus_ring(self, paint: &crate::Paint) -> Self {
        self.border(FOCUS_RING_WIDTH).border_color(paint.focus)
    }

    fn chosen_ring(self, cx: &App) -> Self {
        self.border(CHOSEN_RING_WIDTH)
            .border_color(cx.theme().accent)
    }

    fn surface(self, surface: impl Into<Surface>, cx: &App) -> Self {
        self.surface_at(surface, PaintState::Rest, cx)
    }

    fn surface_at(self, surface: impl Into<Surface>, state: PaintState, cx: &App) -> Self {
        self.layer(&cx.theme().layer(surface, state))
    }

    fn layer(self, layer: &Layer) -> Self {
        let this = self
            .rounded(layer.radius)
            .corner_smoothing(layer.smoothing)
            .border(layer.border_width)
            .border_color(layer.border)
            .shadow(layer.shadow.clone())
            .text_color(layer.text);
        let this = match layer.mark {
            Some(mark) => this.edge_mark(mark),
            None => this,
        };
        match layer.glass {
            Some(material) => this.glass(material),
            None => this.bg(layer.fill),
        }
    }

    fn rule_on(self, role: SurfaceRole, edge: Edge, cx: &App) -> Self {
        let theme = cx.theme();
        if !theme.style(role).outline {
            return self;
        }
        let width = RULE_WIDTH;
        match edge {
            Edge::Top => self.border_t(width),
            Edge::Bottom => self.border_b(width),
            Edge::Left => self.border_l(width),
            Edge::Right => self.border_r(width),
        }
        .border_color(theme.divider_color())
    }

    font_weight!(font_thin, THIN);
    font_weight!(font_extralight, EXTRA_LIGHT);
    font_weight!(font_light, LIGHT);
    font_weight!(font_normal, NORMAL);
    font_weight!(font_medium, MEDIUM);
    font_weight!(font_semibold, SEMIBOLD);
    font_weight!(font_bold, BOLD);
    font_weight!(font_extrabold, EXTRA_BOLD);
    font_weight!(font_black, BLACK);
}

impl<E: Styled> StyledExt for E {}

pub fn row_radius(layer: &Layer, height: Pixels) -> Pixels {
    if layer.radius >= FULL_CORNER_RADIUS {
        layer.radius
    } else {
        layer.radius.min(height * ROW_RADIUS_SHARE)
    }
}

pub fn row_corners<E: Styled>(element: E, layer: &Layer, height: Pixels) -> E {
    element
        .rounded(row_radius(layer, height))
        .corner_radius_mode(ala::CornerRadiusMode::Fixed)
        .corner_smoothing(ROW_CORNER_SMOOTHING)
}

const ROW_RADIUS_SHARE: f32 = 0.40;
pub(crate) const ROW_CORNER_SMOOTHING: f32 = 2.5;

pub fn row_surface<E: StatefulInteractiveElement + Styled>(
    element: E,
    surface: impl Into<Surface>,
    state: PaintState,
    height: Pixels,
    cx: &App,
) -> E {
    let surface = surface.into();
    let layer = cx.theme().layer(surface, state);
    row_corners(
        element.surface_interactive(surface, state, cx),
        &layer,
        height,
    )
}

pub fn muted_on(layer: &Layer, cx: &App) -> Hsla {
    cx.theme()
        .paint_on(layer.ground, Tone::Neutral, Emphasis::Ghost)
        .muted_at(PaintState::Rest)
}

pub const fn row_state(selected: bool, enabled: bool) -> PaintState {
    match (enabled, selected) {
        (false, _) => PaintState::Disabled,
        (true, true) => PaintState::Selected,
        (true, false) => PaintState::Rest,
    }
}

pub fn list_row(
    id: impl Into<ElementId>,
    height: Pixels,
    selected: bool,
    enabled: bool,
    cx: &App,
) -> Stateful<Div> {
    row_surface(
        h_flex().id(id).flex_none().h(height),
        SurfaceRole::MENU_ITEM,
        row_state(selected, enabled),
        height,
        cx,
    )
}

pub const RULE_WIDTH: Pixels = px(1.0);

pub const FOCUS_RING_WIDTH: Pixels = px(1.5);

const CHOSEN_RING_WIDTH: Pixels = px(2.0);

pub fn lifted_shadow(cx: &App) -> Vec<BoxShadow> {
    cx.theme().lifted_shadows()
}

#[derive(Clone, Default, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    Size(Pixels),
    Small,
    #[default]
    Medium,
}

impl Size {
    pub fn control_px(&self) -> f32 {
        match self {
            Size::Size(value) => value.as_f32(),
            Size::Small => 24.,
            Size::Medium => 28.,
        }
    }

    pub fn control_text(&self) -> AbsoluteLength {
        match self {
            Size::Size(value) => (*value * 0.875).into(),
            Size::Small => rems_from_px(12.).into(),
            Size::Medium => rems_from_px(13.).into(),
        }
    }

    pub fn input_px(&self) -> DefiniteLength {
        match self {
            Self::Medium => rems_from_px(8.).into(),
            Self::Small => rems_from_px(6.).into(),
            Self::Size(_) => px(8.).into(),
        }
    }

    pub fn input_py(&self) -> DefiniteLength {
        match self {
            Size::Medium => rems_from_px(2.).into(),
            Size::Small => rems_from_px(0.).into(),
            Size::Size(_) => px(2.).into(),
        }
    }

    pub fn control_h(&self) -> DefiniteLength {
        match self {
            Size::Size(value) => (*value).into(),
            size => rems_from_px(size.control_px()).into(),
        }
    }
}

impl From<Pixels> for Size {
    fn from(size: Pixels) -> Self {
        Size::Size(size)
    }
}

#[allow(patterns_in_fns_without_body)]
pub trait Selectable: Sized {
    fn selected(mut self, selected: bool) -> Self;

    fn is_selected(&self) -> bool;

    fn secondary_selected(self, _: bool) -> Self {
        self
    }
}

#[allow(patterns_in_fns_without_body)]
pub trait Disableable {
    fn disabled(mut self, disabled: bool) -> Self;
}

#[allow(patterns_in_fns_without_body)]
pub trait Sizable: Sized {
    fn with_size(mut self, size: impl Into<Size>) -> Self;

    #[inline(always)]
    fn small(self) -> Self {
        self.with_size(Size::Small)
    }
}

#[cfg(test)]
mod tests {
    use crate::Theme;

    #[ala::test]
    fn a_knobs_shadow_follows_the_contact_strength(cx: &mut ala::TestAppContext) {
        cx.update(crate::init);
        cx.update(|cx| {
            assert!(!super::lifted_shadow(cx).is_empty());
            Theme::global_mut(cx).picked_shadows = true;
            assert!(super::lifted_shadow(cx).is_empty());
            Theme::global_mut(cx).picked_shadows = false;
            Theme::global_mut(cx).contact_shadow.strength = 0.0;
            assert!(super::lifted_shadow(cx).is_empty());
        });
    }
}
