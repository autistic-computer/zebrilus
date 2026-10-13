//! The shared styling vocabulary: one [`Size`] scale, one set of state traits,
//! one set of [`zz_gpui::Styled`] extensions.

use super::{ActiveTheme, Edge, Elevation, Ground, Kind, Layer, PaintState, Surface, SurfaceRole};
use zz_gpui::{
    AbsoluteLength, App, BoxShadow, Corners, DefiniteLength, Div, Edges, Hsla, Pixels, Refineable,
    StyleRefinement, Styled, div, point, px,
};

use super::rems_from_px;

/// A [`Div`] laid out as a horizontal flex row, vertically centered.
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
        /// [docs](https://tailwindcss.com/docs/font-weight)
        #[inline]
        fn $fn(self) -> Self {
            self.font_weight(zz_gpui::FontWeight::$const)
        }
    };
}

/// Extends [`zz_gpui::Styled`] with the shorthands this crate leans on.
pub trait StyledExt: Styled + Sized {
    fn refine_style(mut self, style: &StyleRefinement) -> Self {
        self.style().refine(style);
        self
    }

    /// Lay out as a horizontal flex row, vertically centered.
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

    fn control_highlight(self, cx: &App) -> Self {
        self.control_highlight_on(cx.theme().ground(), cx)
    }

    /// The control's hairline color, and its shadow when `ground` is the
    /// window: a control on a panel or a popover stays flat, and so does every
    /// control while the theme's buttons cast none.
    fn control_highlight_on(self, ground: Ground, cx: &App) -> Self {
        let this = self.border_color(cx.theme().foreground.opacity(CONTROL_EDGE_ALPHA));
        if ground.elevation == Elevation::Window {
            this.shadow(control_shadow(cx))
        } else {
            this
        }
    }

    fn control_surface(self, cx: &App) -> Self {
        self.control_surface_on(cx.theme().ground(), cx)
    }

    /// A control's hairline at the look's outline width, and its shadow.
    fn control_surface_on(self, ground: Ground, cx: &App) -> Self {
        if cx.theme().outlined() {
            self.border(cx.theme().outline)
                .control_highlight_on(ground, cx)
        } else {
            self.control_highlight_on(ground, cx)
        }
    }

    /// An edge in `color` at the look's outline width, or none when the look
    /// has no outline.
    fn surface_outline(self, color: Hsla, cx: &App) -> Self {
        let this = self.border_color(color);
        if cx.theme().outlined() {
            this.border(cx.theme().outline)
        } else {
            this
        }
    }

    /// The edge of a card or a floating surface: the border color at the
    /// look's outline width. Glass hides it behind its own rim.
    fn surface_edge(self, cx: &App) -> Self {
        self.surface_outline(cx.theme().border(), cx)
    }

    /// A card: a raised panel that sits in the window, drawn as the theme
    /// draws cards.
    fn card_style(self, cx: &App) -> Self {
        self.surface(Kind::Card, cx)
    }

    /// A rule along the top edge.
    fn rule_top(self, cx: &App) -> Self {
        self.border_t(RULE_WIDTH)
            .border_color(cx.theme().divider_color())
    }

    /// A rule along the bottom edge.
    fn rule_bottom(self, cx: &App) -> Self {
        self.border_b(RULE_WIDTH)
            .border_color(cx.theme().divider_color())
    }

    /// A rule along the left edge.
    fn rule_left(self, cx: &App) -> Self {
        self.border_l(RULE_WIDTH)
            .border_color(cx.theme().divider_color())
    }

    /// A rule along the right edge.
    fn rule_right(self, cx: &App) -> Self {
        self.border_r(RULE_WIDTH)
            .border_color(cx.theme().divider_color())
    }

    /// A list of rows: padded in from its edges by the look's row inset, with
    /// a hair of space between rows while they float.
    fn list_inset(self, cx: &App) -> Self {
        let this = self.px(cx.theme().row_inset);
        if cx.theme().inset_rows() {
            this.gap_y_0p5()
        } else {
            this
        }
    }

    /// A focus ring in `paint`'s focus color.
    fn focus_ring(self, paint: &crate::Paint) -> Self {
        self.border(FOCUS_RING_WIDTH).border_color(paint.focus)
    }

    /// A ring marking the chosen tile in a picker, in the accent.
    fn chosen_ring(self, cx: &App) -> Self {
        self.border(CHOSEN_RING_WIDTH)
            .border_color(cx.theme().accent)
    }

    /// Fills a floating surface with `color`, or with the theme's glass
    /// tinted by it. Glass draws its own edge, so it hides the hairline while
    /// keeping its width; call this after setting the border.
    fn surface_fill(self, color: Hsla, cx: &App) -> Self {
        match cx.theme().glass {
            Some(material) if !material.is_opaque_fill() => self
                .glass(material.tint(color.alpha(material.tint.a)).fallback(color))
                .border_color(cx.theme().transparent),
            _ => self.bg(color),
        }
    }

    /// A list row's corners: rounded while rows float, square bands when the
    /// look's row inset is zero.
    fn menu_item_corners(self, row_height: Pixels, cx: &App) -> Self {
        self.rounded(cx.theme().row_radius(row_height))
            .corner_radius_mode(zz_gpui::CornerRadiusMode::Fixed)
            .corner_smoothing(2.5)
    }

    /// A selected row: the neutral ghost paint's selected fill, with the
    /// text that reads on it.
    fn selection_highlight(self, cx: &App) -> Self {
        let paint = cx
            .theme()
            .paint(crate::Tone::Neutral, crate::Emphasis::Ghost);
        self.bg(paint.selected)
            .border_color(paint.selected)
            .text_color(paint.selected_text)
            .shadow_none()
    }

    /// `surface` at rest.
    fn surface(self, surface: impl Into<Surface>, cx: &App) -> Self {
        self.surface_at(surface, PaintState::Rest, cx)
    }

    /// `surface` in `state`. Pointing and pressing are not tracked; see
    /// [`InteractiveSurfaceExt::surface_interactive`].
    fn surface_at(self, surface: impl Into<Surface>, state: PaintState, cx: &App) -> Self {
        self.layer(&cx.theme().layer(surface, state))
    }

    /// Paints `layer`: fill or glass, edge, shadows, corners and ink.
    fn layer(self, layer: &Layer) -> Self {
        let this = self
            .rounded(layer.radius)
            .border(layer.border_width)
            .border_color(layer.border)
            .shadow(layer.shadow.clone())
            .text_color(layer.text);
        match layer.glass {
            Some(material) => this.glass(material),
            None => this.bg(layer.fill),
        }
    }

    /// A divider along `edge`, when `role` draws its outline.
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

    /// A surface floating over the window, drawn as the theme draws
    /// popovers.
    #[inline]
    fn popover_style(self, cx: &App) -> Self {
        self.surface(Kind::Popover, cx)
    }

    fn corner_radii(self, radius: Corners<Pixels>) -> Self {
        self.rounded_tl(radius.top_left)
            .rounded_tr(radius.top_right)
            .rounded_bl(radius.bottom_left)
            .rounded_br(radius.bottom_right)
    }
}

impl<E: Styled> StyledExt for E {}

/// The width of every divider rule.
pub const RULE_WIDTH: Pixels = px(1.0);

/// The width of a focus ring.
pub const FOCUS_RING_WIDTH: Pixels = px(1.5);

const CHOSEN_RING_WIDTH: Pixels = px(2.0);

/// How strongly a control's hairline shows the foreground.
const CONTROL_EDGE_ALPHA: f32 = 0.1;

/// The theme's tight shadow: what a pane, a card or a resting control casts.
pub fn contact_shadow(cx: &App) -> Vec<BoxShadow> {
    cx.theme().contact_shadows()
}

/// The shadow a control casts on the window: the contact shadow while the
/// theme's buttons cast one.
pub fn control_shadow(cx: &App) -> Vec<BoxShadow> {
    let theme = cx.theme();
    if theme.casts_shadow(Kind::Button.role()) {
        theme.contact_shadows()
    } else {
        Vec::new()
    }
}

/// The shadow of anything that floats over the window: a popover, a dialog,
/// a floating pane, a chip being dragged. The contact shadow with a deep,
/// soft one under it, drawn in the theme's shadow color.
pub fn floating_shadow(cx: &App) -> Vec<BoxShadow> {
    cx.theme().float_shadows()
}

/// The small shadow of a knob riding on a control, like a switch thumb.
pub fn lifted_shadow(cx: &App) -> Vec<BoxShadow> {
    let strength = cx.theme().contact_shadow.strength;
    if strength <= 0.0 {
        return Vec::new();
    }
    let ink = cx.theme().shadow_color.alpha((0.1 * strength).min(1.0));
    vec![
        BoxShadow {
            color: ink,
            offset: point(px(0.), px(4.)),
            blur_radius: px(6.),
            spread_radius: px(-1.),
            inset: false,
        },
        BoxShadow {
            color: ink,
            offset: point(px(0.), px(2.)),
            blur_radius: px(4.),
            spread_radius: px(-2.),
            inset: false,
        },
    ]
}

/// The size scale shared by every widget in the crate: [`Size::Medium`] for
/// form controls, [`Size::Small`] for toolbars and dense rows.
#[derive(Clone, Default, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    /// An explicit pixel size, for the few places the scale does not fit.
    Size(Pixels),
    Small,
    #[default]
    Medium,
}

impl Size {
    /// Control height in pixels.
    pub fn control_px(&self) -> f32 {
        match self {
            Size::Size(value) => value.as_f32(),
            Size::Small => 24.,
            Size::Medium => 28.,
        }
    }

    /// Label text of every control at this size.
    pub fn control_text(&self) -> AbsoluteLength {
        match self {
            Size::Size(value) => (*value * 0.875).into(),
            Size::Small => rems_from_px(12.).into(),
            Size::Medium => rems_from_px(13.).into(),
        }
    }

    /// Horizontal padding for input-shaped controls.
    pub fn input_px(&self) -> DefiniteLength {
        match self {
            Self::Medium => rems_from_px(8.).into(),
            Self::Small => rems_from_px(6.).into(),
            Self::Size(_) => px(8.).into(),
        }
    }

    /// Vertical padding for input-shaped controls.
    pub fn input_py(&self) -> DefiniteLength {
        match self {
            Size::Medium => rems_from_px(2.).into(),
            Size::Small => rems_from_px(0.).into(),
            Size::Size(_) => px(2.).into(),
        }
    }

    /// Height of every control that sits on a form row: text fields,
    /// selects, and the square of an icon-only button. `Size::Size` stays the
    /// caller's literal pixels.
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

    /// Mark the element as right-click-selected. Does nothing by default.
    fn secondary_selected(self, _: bool) -> Self {
        self
    }
}

#[allow(patterns_in_fns_without_body)]
pub trait Disableable {
    fn disabled(mut self, disabled: bool) -> Self;
}

/// An element that takes a [`Size`]. [`Size::Medium`] is the default.
#[allow(patterns_in_fns_without_body)]
pub trait Sizable: Sized {
    /// Accepts a [`Pixels`] for a custom size: `.with_size(px(30.))`.
    fn with_size(mut self, size: impl Into<Size>) -> Self;

    #[inline(always)]
    fn small(self) -> Self {
        self.with_size(Size::Small)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Kind, Shadow, StylePatch, Theme};

    #[zz_gpui::test]
    fn controls_cast_only_while_the_theme_lets_buttons_cast(cx: &mut zz_gpui::TestAppContext) {
        cx.update(crate::init);
        cx.update(|cx| {
            assert!(!super::control_shadow(cx).is_empty());
            Theme::global_mut(cx).styles.insert(
                Kind::Button.name().into(),
                StylePatch::EMPTY.shadow(Shadow::None),
            );
            assert!(super::control_shadow(cx).is_empty());
            assert!(!super::contact_shadow(cx).is_empty());
            Theme::global_mut(cx).contact_shadow.strength = 0.0;
            assert!(super::contact_shadow(cx).is_empty());
            assert!(super::lifted_shadow(cx).is_empty());
            assert_eq!(super::floating_shadow(cx).len(), 1);
        });
    }
}
