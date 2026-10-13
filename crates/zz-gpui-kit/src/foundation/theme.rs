//! The theme global: one palette, one set of metrics, for the whole process.

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use zz_gpui::{
    Anchor, App, Edges, GlassMaterial, Global, Hsla, Pixels, SharedString, Window,
    WindowAppearance, px,
};

use crate::{BASE_UI_FONT_SIZE, TITLE_BAR_HEIGHT, highlighter::HighlightTheme};

use super::{
    Colorize as _, PrimaryStyle, SurfaceRole, ShadowMaterial, StylePatch, ThemeColor, color,
};

/// The one gap in chrome: how far a control sits from the edge of the bar it
/// lives in, and from its neighbours. A bar pads by this and spaces its
/// children by it.
pub const CHROME_GAP: f32 = 6.0;

pub(super) fn init(cx: &mut App) {
    Theme::change(ThemeMode::Light, None, cx);
    Theme::sync_scrollbar_appearance(cx);
}

pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    #[inline(always)]
    fn theme(&self) -> &Theme {
        Theme::global(self)
    }
}

/// When a scrollbar is visible.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScrollbarShow {
    /// Show while scrolling, then fade out after idle.
    #[default]
    Scrolling,
    Hover,
    Always,
}

/// Where notification toasts stack, and how many.
#[derive(Debug, Clone)]
pub struct NotificationSettings {
    pub placement: Anchor,
    /// Insets from the window edges.
    pub margins: Edges<Pixels>,
    /// How many toasts are shown at once before the oldest is dropped.
    pub max_items: usize,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        let offset = px(16.);
        Self {
            placement: Anchor::TopCenter,
            margins: Edges {
                top: TITLE_BAR_HEIGHT + offset,
                right: offset,
                bottom: offset,
                left: offset,
            },
            max_items: 10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    /// The flat palette. Reachable directly through [`Deref`].
    pub colors: ThemeColor,
    /// Syntax colors for code blocks and the editor gutter.
    pub highlight_theme: Arc<HighlightTheme>,

    pub mode: ThemeMode,
    /// The font family for the application, default is `.SystemUIFont`.
    pub font_family: SharedString,
    /// The base font size for the application, default is 16px.
    pub font_size: Pixels,
    /// The monospace font family. Defaults to `Menlo` on Apple platforms,
    /// `Consolas` on Windows, `DejaVu Sans Mono` elsewhere.
    pub mono_font_family: SharedString,
    /// The monospace font size for the application, default is 13px.
    pub mono_font_size: Pixels,
    pub radius: Pixels,
    pub contrast: f32,
    /// The width of the hairline every edged thing draws: controls, cards
    /// and floating surfaces. Zero draws none.
    pub outline: Pixels,
    /// The tight shadow of things resting on the window.
    pub contact_shadow: ShadowMaterial,
    /// The deep shadow of things floating over it.
    pub float_shadow: ShadowMaterial,
    /// How far selected and hovered list rows float in from the list's
    /// edges, and whether they stand apart. Zero makes them full-width bands
    /// that touch.
    pub row_inset: Pixels,
    /// What the primary tone is drawn in.
    pub primary: PrimaryStyle,
    /// What a surface whose role takes glass becomes, and the edge pane
    /// frames take. Each surface tints it with its own color; the material's
    /// tint alpha says how much of that color covers the glass.
    pub glass: Option<GlassMaterial>,
    /// The theme's changes to each kind and role, by the name a theme file
    /// gives it.
    pub styles: HashMap<SharedString, StylePatch>,
    /// A role whose every surface draws a marker ring, so a theme editor can
    /// show where it appears.
    pub inspect: Option<SharedString>,
    pub pane_background_opacity: f32,
    /// Whether panes draw a [`Self::pane_frame`], which then owns their
    /// fill: set while panes have gaps.
    pub pane_frames: bool,
    /// The width of the glint along a pane frame's edge; 0 turns it off.
    pub pane_edge_width: Pixels,
    /// How strongly the active pane's frame lights its edge in the accent:
    /// 1 is the look's own glint, 0 lights it like any other pane.
    pub pane_glow_strength: f32,
    pub transparent: Hsla,
    /// When scrollbars are visible, default: [`ScrollbarShow::Scrolling`].
    pub scrollbar_show: ScrollbarShow,
    pub notification: NotificationSettings,
}

impl Default for Theme {
    fn default() -> Self {
        Self::from(&*ThemeColor::light())
    }
}

/// The corner smoothing every window draws with.
pub const CORNER_SMOOTHING: f32 = 4.0;

impl Deref for Theme {
    type Target = ThemeColor;

    fn deref(&self) -> &Self::Target {
        &self.colors
    }
}

impl DerefMut for Theme {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.colors
    }
}

impl Global for Theme {}

impl Theme {
    pub fn control_radius(&self) -> Pixels {
        if self.radius > px(24.0) {
            zz_gpui::FULL_CORNER_RADIUS
        } else {
            self.radius
        }
    }

    /// The fill of the row a list's cursor is on.
    pub fn selection_background(&self) -> Hsla {
        self.accent.opaque()
    }

    pub fn menu_radius(&self) -> Pixels {
        (self.radius - px(4.0)).max(px(0.0))
    }

    /// The corner radius of a list row `row_height` tall: rounded while rows
    /// float, square when the look's row inset is zero.
    pub fn row_radius(&self, row_height: Pixels) -> Pixels {
        if self.inset_rows() {
            self.menu_radius().min(row_height * 0.40)
        } else {
            px(0.0)
        }
    }

    /// Whether list rows float as rounded highlights, inset from the list's
    /// edges and apart from each other. Square corners make them full-width
    /// bands that touch.
    pub fn inset_rows(&self) -> bool {
        self.row_inset > px(0.0)
    }

    /// Whether edged things draw their hairline at all.
    pub fn outlined(&self) -> bool {
        self.outline > px(0.0)
    }

    /// The theme's glass, when it draws anything a plain fill would not.
    pub fn glass_effect(&self) -> Option<GlassMaterial> {
        self.glass.filter(|material| !material.is_opaque_fill())
    }

    /// Whether `role` casts a shadow in this theme.
    pub fn casts_shadow(&self, role: SurfaceRole) -> bool {
        !self.shadows_of(self.style(role).shadow).is_empty()
    }

    /// The color a pane shows: `fill` over the window color, at the look's
    /// pane opacity. Pass `background` for a pane with no fill of its own.
    pub fn pane_color(&self, fill: Hsla) -> Hsla {
        self.background
            .opaque()
            .blend(fill)
            .opacity(self.pane_background_opacity)
    }

    /// How much of its fill a pane's content paints. None under
    /// [`Self::pane_frames`], where the pane frame owns the fill.
    pub fn pane_fill_opacity(&self) -> f32 {
        if self.pane_frames {
            0.0
        } else {
            self.pane_background_opacity
        }
    }

    /// What a pane's content paints as its fill: [`Self::pane_color`], or
    /// nothing under [`Self::pane_frames`], where the pane frame owns the
    /// fill. It keeps the color's channels, so `.opaque()` still gives the
    /// solid color.
    pub fn pane_background(&self, fill: Hsla) -> Hsla {
        self.background
            .opaque()
            .blend(fill)
            .opacity(self.pane_fill_opacity())
    }

    /// The glass a gapped pane draws as its frame and fill: the look's glass
    /// edge, or a plain lit edge when the look has no glass, with no frost or
    /// lensing, tinted with the pane's color over `plane`, the window color
    /// behind the pane. On an opaque window the tint is solid, so the glass
    /// paints in place without reading the backdrop. The active pane lights
    /// its edge in the accent: the glint and fresnel scaled by
    /// [`Self::pane_glow_strength`], and the contour, which also shows on a
    /// light pane where added light cannot.
    pub fn pane_frame(&self, fill: Hsla, plane: Hsla, active: bool) -> GlassMaterial {
        let tint = plane.blend(self.pane_color(fill));
        let frame = self
            .glass_effect()
            .unwrap_or_else(|| GlassMaterial::edge(tint))
            .blur(px(0.0))
            .refraction(px(0.0))
            .dispersion(0.0)
            .noise(0.0)
            .tint(tint)
            .fallback(tint)
            .glint_width(self.pane_edge_width);
        let frame = if self.pane_edge_width > px(0.0) {
            frame
        } else {
            frame.specular(0.0)
        };
        if active && self.pane_glow_strength > 0.0 {
            let strength = self.pane_glow_strength;
            frame
                .light_color(self.accent.opaque())
                .specular(frame.specular * strength)
                .fresnel(frame.fresnel * strength)
                .edge_color(self.accent.opaque())
                .edge_shadow(strength.min(1.0))
                .edge_width(frame.edge_width.max(px(1.0)))
        } else {
            frame
        }
    }

    /// The color of a divider rule.
    pub fn divider_color(&self) -> Hsla {
        self.border()
    }

    /// The radius of a surface whose content sits `inset` inside it, so the
    /// two stay concentric. Square corners stay square.
    pub fn outer_radius(&self, inset: Pixels) -> Pixels {
        if self.radius > px(0.0) {
            self.radius + inset
        } else {
            px(0.0)
        }
    }

    /// The radius of content that sits `inset` inside a surface, so the two
    /// stay concentric. Square corners stay square.
    pub fn inner_radius(&self, inset: Pixels) -> Pixels {
        (self.radius - inset).max(px(0.0))
    }

    pub fn set_contrast(&mut self, value: f32) {
        color::set_contrast(value);
        self.contrast = color::contrast();
    }

    #[inline(always)]
    pub fn global(cx: &App) -> &Theme {
        cx.global::<Theme>()
    }

    #[inline(always)]
    pub fn global_mut(cx: &mut App) -> &mut Theme {
        cx.global_mut::<Theme>()
    }

    #[inline(always)]
    pub fn is_dark(&self) -> bool {
        self.mode.is_dark()
    }

    pub fn sync_system_appearance(window: Option<&mut Window>, cx: &mut App) {
        // Prefer the window's own appearance: the app-level query errors on
        // Linux (gpui-component#104).
        let appearance = window
            .as_ref()
            .map(|window| window.appearance())
            .unwrap_or_else(|| cx.window_appearance());

        Self::change(appearance, window, cx);
    }

    pub fn sync_scrollbar_appearance(cx: &mut App) {
        Theme::global_mut(cx).scrollbar_show = if cx.should_auto_hide_scrollbars() {
            ScrollbarShow::Scrolling
        } else {
            ScrollbarShow::Hover
        };
    }

    /// Switch to `mode`, resetting the palette to that mode's built-in colors.
    /// Installs the global on first call. Fonts, radii and the notification
    /// placement are left alone.
    pub fn change(mode: impl Into<ThemeMode>, window: Option<&mut Window>, cx: &mut App) {
        let mode = mode.into();
        if !cx.has_global::<Theme>() {
            cx.set_global(Theme::default());
        }

        let colors = *ThemeColor::for_mode(mode);
        let theme = cx.global_mut::<Theme>();
        theme.mode = mode;
        theme.colors = colors;
        theme.highlight_theme = if mode.is_dark() {
            HighlightTheme::default_dark()
        } else {
            HighlightTheme::default_light()
        };

        if let Some(window) = window {
            window.refresh();
        }
    }

    /// The code editor background, falling back to one elevation step when the
    /// highlight theme does not set one.
    #[inline]
    pub fn editor_background(&self) -> Hsla {
        self.highlight_theme
            .style
            .editor_background
            .unwrap_or_else(|| self.background.raised(1))
    }
}

impl From<&ThemeColor> for Theme {
    fn from(colors: &ThemeColor) -> Self {
        Theme {
            mode: ThemeMode::default(),
            transparent: Hsla::transparent_black(),
            font_family: ".SystemUIFont".into(),
            font_size: px(BASE_UI_FONT_SIZE),
            mono_font_family: if cfg!(any(target_os = "macos", target_os = "ios")) {
                "Menlo".into()
            } else if cfg!(target_os = "windows") {
                "Consolas".into()
            } else {
                "DejaVu Sans Mono".into()
            },
            mono_font_size: px(13.),
            radius: px(6.),
            contrast: 1.0,
            outline: px(0.5),
            contact_shadow: ShadowMaterial::CONTACT,
            float_shadow: ShadowMaterial::FLOAT,
            row_inset: px(4.0),
            primary: PrimaryStyle::Foreground,
            glass: None,
            styles: HashMap::default(),
            inspect: None,
            pane_background_opacity: 0.5,
            pane_frames: false,
            pane_edge_width: px(0.5),
            pane_glow_strength: 1.0,
            scrollbar_show: ScrollbarShow::default(),
            notification: NotificationSettings::default(),
            colors: *colors,
            highlight_theme: HighlightTheme::default_light(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
}

impl ThemeMode {
    #[inline(always)]
    pub fn is_dark(&self) -> bool {
        matches!(self, Self::Dark)
    }

    /// The lowercase name: `light`, `dark`.
    pub fn name(&self) -> &'static str {
        match self {
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }
}

impl From<WindowAppearance> for ThemeMode {
    fn from(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_radius_rounds_controls_after_24_without_changing_surfaces() {
        for radius in [0.0, 6.0, 24.0, 24.5, 25.0] {
            let theme = Theme {
                radius: px(radius),
                ..Theme::default()
            };
            for (width, height) in [(24.0, 24.0), (120.0, 30.0), (360.0, 40.0)] {
                let size = zz_gpui::size(px(width), px(height));
                let control = zz_gpui::Corners::all(theme.control_radius())
                    .resolve_radii_for_quad_size(size, 0.45);
                let surface =
                    zz_gpui::Corners::all(theme.radius).resolve_radii_for_quad_size(size, 0.45);
                if radius > 24.0 {
                    assert_eq!(control, zz_gpui::Corners::all(px(height / 2.0)));
                    assert!(surface.top_left < control.top_left);
                } else {
                    assert_eq!(control, surface);
                }
                assert_eq!(theme.radius, px(radius));
            }
        }
    }

    #[test]
    fn pane_frames_own_the_pane_fill() {
        let mut theme = Theme::default();
        let fill = theme.accent;
        let solid = theme.background.opaque().blend(fill);
        assert_eq!(theme.pane_background(fill), theme.pane_color(fill));
        assert_eq!(theme.pane_fill_opacity(), theme.pane_background_opacity);

        theme.pane_frames = true;
        assert_eq!(theme.pane_background(fill).a, 0.0);
        assert_eq!(theme.pane_background(fill).opaque(), solid);
        assert_eq!(theme.pane_fill_opacity(), 0.0);
    }

    #[test]
    fn a_pane_frame_lights_an_opaque_edge_with_or_without_the_looks_glass() {
        let mut theme = Theme::default();
        let fill = theme.accent;
        let plane = theme.background;
        let tint = plane.blend(theme.pane_color(fill));
        for (glass, bezel) in [
            (None, GlassMaterial::edge(tint).bezel),
            (
                Some(GlassMaterial::regular().vanished().tint(plane.opaque())),
                GlassMaterial::edge(tint).bezel,
            ),
            (
                Some(GlassMaterial::frosted()),
                GlassMaterial::frosted().bezel,
            ),
        ] {
            theme.glass = glass;
            let frame = theme.pane_frame(fill, plane, false);
            assert_eq!(frame.tint, tint);
            assert_eq!(frame.fallback, Some(tint));
            assert_eq!(frame.bezel, bezel);
            assert!(!frame.reads_backdrop());
            assert!(!frame.is_opaque_fill());
            assert_eq!(
                (frame.blur, frame.refraction, frame.dispersion, frame.noise),
                (px(0.0), px(0.0), 0.0, 0.0)
            );
            assert_eq!(frame.glint_width, theme.pane_edge_width);
            assert_eq!(frame.light_color, GlassMaterial::regular().light_color);
        }

        theme.pane_edge_width = px(0.0);
        assert_eq!(theme.pane_frame(fill, plane, false).specular, 0.0);
    }

    #[test]
    fn the_active_pane_frame_lights_its_edge_in_the_accent() {
        let mut theme = Theme::default();
        let (fill, plane) = (theme.background, theme.background);
        let inactive = theme.pane_frame(fill, plane, false);
        let active = theme.pane_frame(fill, plane, true);
        assert_eq!(active.light_color, theme.accent.opaque());
        assert_eq!(active.edge_color, theme.accent.opaque());
        assert_eq!(inactive.edge_color, GlassMaterial::regular().edge_color);
        assert_eq!(active.edge_shadow, 1.0);
        assert!(active.edge_width >= px(1.0));
        assert_eq!(
            (active.specular, active.fresnel),
            (inactive.specular, inactive.fresnel)
        );

        theme.pane_glow_strength = 2.0;
        let bright = theme.pane_frame(fill, plane, true);
        assert_eq!(
            (bright.specular, bright.fresnel),
            (inactive.specular * 2.0, inactive.fresnel * 2.0)
        );

        theme.pane_glow_strength = 0.0;
        assert_eq!(theme.pane_frame(fill, plane, true), inactive);
    }
}
