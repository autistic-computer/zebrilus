//! The shared styling vocabulary: one [`Size`] scale, one set of state traits,
//! one set of [`zz_gpui::Styled`] extensions.

use super::Colorize as _;
use super::{ActiveTheme, Elevation, Ground};
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

    /// A 1px border in the theme's ring color.
    #[inline]
    fn focused_border(self, cx: &App) -> Self {
        self.border_1().border_color(cx.theme().accent)
    }

    fn control_highlight(self, cx: &App) -> Self {
        self.control_highlight_on(cx.theme().ground(), cx)
    }

    /// The control's hairline, and its shadow when `ground` is the window: a
    /// control on a panel or a popover stays flat, and so does every control
    /// when the theme turns control shadows off.
    fn control_highlight_on(self, ground: Ground, cx: &App) -> Self {
        let this = self.border_color(cx.theme().foreground.opacity(0.1 * cx.theme().outline));
        if cx.theme().control_shadows() && ground.elevation == Elevation::Window {
            this.shadow(control_shadow(cx))
        } else {
            this
        }
    }

    fn control_surface(self, cx: &App) -> Self {
        self.control_surface_on(cx.theme().ground(), cx)
    }

    fn control_surface_on(self, ground: Ground, cx: &App) -> Self {
        if cx.theme().outlined() {
            self.border(cx.theme().outline_width)
                .control_highlight_on(ground, cx)
        } else {
            self.control_highlight_on(ground, cx)
        }
    }

    /// The outline of a floating surface drawn in `color`, at the theme's
    /// outline width and strength.
    fn surface_outline(self, color: Hsla, cx: &App) -> Self {
        let this = self.border_color(color.opacity(cx.theme().outline));
        if cx.theme().outlined() {
            this.border(cx.theme().outline_width)
        } else {
            this
        }
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

    fn menu_item_corners(self, row_height: Pixels, cx: &App) -> Self {
        self.rounded(cx.theme().menu_radius().min(row_height * 0.40))
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

    font_weight!(font_thin, THIN);
    font_weight!(font_extralight, EXTRA_LIGHT);
    font_weight!(font_light, LIGHT);
    font_weight!(font_normal, NORMAL);
    font_weight!(font_medium, MEDIUM);
    font_weight!(font_semibold, SEMIBOLD);
    font_weight!(font_bold, BOLD);
    font_weight!(font_extrabold, EXTRA_BOLD);
    font_weight!(font_black, BLACK);

    /// The floating-panel look: popover background, border, shadow and radius.
    #[inline]
    fn popover_style(self, cx: &App) -> Self {
        let surface = self
            .text_color(cx.theme().foreground)
            .control_surface(cx)
            .surface_fill(
                cx.theme().surface(crate::Elevation::Popover).fill.opaque(),
                cx,
            )
            .rounded(cx.theme().radius);
        if cx.theme().shadow && cx.theme().shadow_strength > 0.0 {
            let elevation = cx.theme().elevation;
            let mut shadows = contact_shadow(cx);
            shadows.push(BoxShadow {
                color: cx.theme().shadow_color.opacity(cx.theme().shadow_strength),
                offset: point(px(0.0), px(12.0 * elevation)),
                blur_radius: px(32.0 * elevation),
                spread_radius: px(-4.0 * elevation),
                inset: false,
            });
            surface.shadow(shadows)
        } else {
            surface
        }
    }

    fn corner_radii(self, radius: Corners<Pixels>) -> Self {
        self.rounded_tl(radius.top_left)
            .rounded_tr(radius.top_right)
            .rounded_bl(radius.bottom_left)
            .rounded_br(radius.bottom_right)
    }
}

impl<E: Styled> StyledExt for E {}

/// How far the ring reaches past the element's own edge.
pub const SURFACE_RING_OUTSET: Pixels = px(0.5);

const RING_SPREAD: Pixels = SURFACE_RING_OUTSET;

const RING_INK_FLOOR: f32 = 0.12;

fn ring(sink: Pixels, cx: &App) -> BoxShadow {
    let ink = cx.theme().shadow_color;
    BoxShadow {
        color: ink.divide(ink.a.max(RING_INK_FLOOR)),
        offset: point(px(0.), sink),
        blur_radius: px(0.),
        spread_radius: RING_SPREAD,
        inset: false,
    }
}

/// The tight shadow a surface sits on: a pane, a card, the edge of a popover.
pub fn contact_shadow(cx: &App) -> Vec<BoxShadow> {
    tight_shadow(cx.theme().shadow_strength, cx)
}

/// The tight shadow a control casts, at the theme's share for controls.
pub fn control_shadow(cx: &App) -> Vec<BoxShadow> {
    tight_shadow(cx.theme().shadow_strength * cx.theme().control_shadow, cx)
}

fn tight_shadow(strength: f32, cx: &App) -> Vec<BoxShadow> {
    if strength <= 0.0 {
        return Vec::new();
    }
    vec![BoxShadow {
        color: cx.theme().shadow_color.alpha(0.2 * strength),
        offset: point(px(0.), px(1.)),
        blur_radius: px(2.),
        spread_radius: px(0.),
        inset: false,
    }]
}

/// The small shadow of a chip that floats over content: a tooltip, a toast,
/// a switch thumb.
pub fn lifted_shadow(cx: &App) -> Vec<BoxShadow> {
    let strength = cx.theme().shadow_strength;
    if strength <= 0.0 {
        return Vec::new();
    }
    let ink = cx.theme().shadow_color.alpha(0.1 * strength);
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

/// The control hairline without the soft falloff. Apply it to a box expanded by
/// [`SURFACE_RING_OUTSET`], so the inset shadow occupies only that band and lays
/// no ink beneath translucent content.
pub fn surface_ring(cx: &App) -> Vec<BoxShadow> {
    let mut ring = ring(px(0.), cx);
    ring.inset = true;
    vec![ring]
}

/// The ring alone, for a surface assembled from stacked pieces that each paint
/// their own share of it. Pass `caps_top: false` on a piece that does not own the
/// top edge, so its stroke sinks under its own background instead of the piece
/// above.
pub fn stacked_ring(caps_top: bool, cx: &App) -> Vec<BoxShadow> {
    vec![ring(if caps_top { px(0.) } else { RING_SPREAD }, cx)]
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
    /// Control height in pixels before density.
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

    /// Vertical padding for input-shaped controls at `density`.
    pub fn input_py(&self, density: f32) -> DefiniteLength {
        match self {
            Size::Medium => rems_from_px(2. * density).into(),
            Size::Small => rems_from_px(0.).into(),
            Size::Size(_) => px(2.).into(),
        }
    }

    /// Height of every control that sits on a form row at `density`: text
    /// fields, selects, and the square of an icon-only button. `Size::Size`
    /// stays the caller's literal pixels.
    pub fn control_h(&self, density: f32) -> DefiniteLength {
        match self {
            Size::Size(value) => (*value).into(),
            size => rems_from_px((size.control_px() * density).round()).into(),
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
    #[zz_gpui::test]
    fn control_shadow_strength_scales_ink_without_changing_shape(cx: &mut zz_gpui::TestAppContext) {
        cx.update(crate::init);
        cx.update(|cx| {
            let original = super::control_shadow(cx);
            assert_eq!(original[0].color.a, 0.2);

            crate::Theme::global_mut(cx).shadow_strength = 0.5;
            let mut expected = original;
            expected[0].color.a *= 0.5;
            assert_eq!(super::control_shadow(cx), expected);

            crate::Theme::change(crate::ThemeMode::Dark, None, cx);
            assert_eq!(super::control_shadow(cx)[0].color.a, 0.1);

            crate::Theme::global_mut(cx).shadow_strength = 0.0;
            assert!(super::control_shadow(cx).is_empty());
        });
    }

    #[zz_gpui::test]
    fn flat_controls_keep_surface_shadows(cx: &mut zz_gpui::TestAppContext) {
        cx.update(crate::init);
        cx.update(|cx| {
            crate::Theme::global_mut(cx).control_shadow = 0.0;
            assert!(super::control_shadow(cx).is_empty());
            assert!(!crate::Theme::global(cx).control_shadows());
            assert_eq!(super::contact_shadow(cx).len(), 1);
        });
    }
}
