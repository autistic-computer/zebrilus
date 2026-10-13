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

    fn control_highlight(self, cx: &App) -> Self {
        self.control_highlight_on(cx.theme().ground(), cx)
    }

    /// The control's hairline color, and its shadow when `ground` is the
    /// window: a control on a panel or a popover stays flat, and so does every
    /// control unless the look's shadows include controls.
    fn control_highlight_on(self, ground: Ground, cx: &App) -> Self {
        let this = self.border_color(cx.theme().foreground.opacity(CONTROL_EDGE_ALPHA));
        if cx.theme().control_shadows() && ground.elevation == Elevation::Window {
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

    /// A card: a raised panel that sits in the window with the look's edge,
    /// corners and contact shadow. A message bubble, an attachment tile, a
    /// diagram frame.
    fn card_style(self, cx: &App) -> Self {
        let theme = cx.theme();
        self.bg(theme.surface(Elevation::Panel).fill.opaque())
            .surface_edge(cx)
            .rounded(theme.radius)
            .shadow(contact_shadow(cx))
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

    font_weight!(font_thin, THIN);
    font_weight!(font_extralight, EXTRA_LIGHT);
    font_weight!(font_light, LIGHT);
    font_weight!(font_normal, NORMAL);
    font_weight!(font_medium, MEDIUM);
    font_weight!(font_semibold, SEMIBOLD);
    font_weight!(font_bold, BOLD);
    font_weight!(font_extrabold, EXTRA_BOLD);
    font_weight!(font_black, BLACK);

    /// The floating-panel look: popover background, edge, shadow and radius.
    #[inline]
    fn popover_style(self, cx: &App) -> Self {
        self.floating_surface(cx).shadow(floating_shadow(cx))
    }

    /// What every surface floating over the window shares, without its
    /// shadow: the popover fill (or the look's glass), the edge, the corners
    /// and the ink.
    fn floating_surface(self, cx: &App) -> Self {
        self.text_color(cx.theme().foreground)
            .surface_edge(cx)
            .surface_fill(
                cx.theme().surface(crate::Elevation::Popover).fill.opaque(),
                cx,
            )
            .rounded(cx.theme().radius)
    }

    /// A small chip floating over content, like a tooltip or a toast: the
    /// floating surface with the lifted shadow.
    fn lifted_style(self, cx: &App) -> Self {
        self.floating_surface(cx).shadow(lifted_shadow(cx))
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

/// The tight shadow a surface sits on: a pane, a card, the edge of a popover.
/// None when the look's surfaces cast no shadows.
pub fn contact_shadow(cx: &App) -> Vec<BoxShadow> {
    if cx.theme().surface_shadows() {
        tight_shadow(cx)
    } else {
        Vec::new()
    }
}

/// The tight shadow a control casts. None unless the look's shadows include
/// controls.
pub fn control_shadow(cx: &App) -> Vec<BoxShadow> {
    if cx.theme().control_shadows() {
        tight_shadow(cx)
    } else {
        Vec::new()
    }
}

fn tight_shadow(cx: &App) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: cx.theme().shadow_color.alpha(0.2),
        offset: point(px(0.), px(1.)),
        blur_radius: px(2.),
        spread_radius: px(0.),
        inset: false,
    }]
}

/// The shadow of anything that floats over the window: a popover, a dialog,
/// a floating pane, a chip being dragged. The contact shadow with a deep,
/// soft one under it, drawn in the theme's shadow color.
pub fn floating_shadow(cx: &App) -> Vec<BoxShadow> {
    if !cx.theme().surface_shadows() {
        return Vec::new();
    }
    let mut shadows = tight_shadow(cx);
    shadows.push(BoxShadow {
        color: cx.theme().shadow_color,
        offset: point(px(0.0), px(12.0)),
        blur_radius: px(32.0),
        spread_radius: px(-4.0),
        inset: false,
    });
    shadows
}

/// The small shadow of a chip that floats over content: a tooltip, a toast,
/// a switch thumb.
pub fn lifted_shadow(cx: &App) -> Vec<BoxShadow> {
    if !cx.theme().surface_shadows() {
        return Vec::new();
    }
    let ink = cx.theme().shadow_color.alpha(0.1);
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
    use crate::{Shadows, Theme};

    #[zz_gpui::test]
    fn shadows_pick_which_surfaces_cast(cx: &mut zz_gpui::TestAppContext) {
        cx.update(crate::init);
        cx.update(|cx| {
            for (shadows, controls, surfaces) in [
                (Shadows::All, true, true),
                (Shadows::Surfaces, false, true),
                (Shadows::None, false, false),
            ] {
                Theme::global_mut(cx).shadows = shadows;
                assert_eq!(!super::control_shadow(cx).is_empty(), controls);
                assert_eq!(!super::contact_shadow(cx).is_empty(), surfaces);
                assert_eq!(!super::floating_shadow(cx).is_empty(), surfaces);
                assert_eq!(!super::lifted_shadow(cx).is_empty(), surfaces);
            }
        });
    }
}
