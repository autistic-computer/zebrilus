use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use zz_gpui::{
    Anchor, App, Corners, Edges, GlassMaterial, Global, Hsla, Pixels, SharedString, Window,
    WindowAppearance, px, size,
};

use crate::{BASE_UI_FONT_SIZE, TITLE_BAR_HEIGHT, highlighter::HighlightTheme};

use super::{
    Colorize as _, Highlight, Kind, PrimaryStyle, ShadowMaterial, Smoothing, StylePatch,
    SurfaceRole, ThemeColor, color,
};

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScrollbarShow {
    #[default]
    Scrolling,
    Hover,
    Always,
}

#[derive(Debug, Clone)]
pub struct NotificationSettings {
    pub placement: Anchor,
    pub margins: Edges<Pixels>,
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
    pub colors: ThemeColor,
    pub highlight_theme: Arc<HighlightTheme>,

    pub mode: ThemeMode,
    pub font_family: SharedString,
    pub font_size: Pixels,
    pub mono_font_family: SharedString,
    pub mono_font_size: Pixels,
    pub radius: Pixels,
    pub control_radius: Option<Pixels>,
    pub contrast: f32,
    pub outline: Pixels,
    pub picked_outlines: bool,
    pub picked_shadows: bool,
    pub smoothing: Smoothing,
    pub picked_smoothing: bool,
    pub contact_shadow: ShadowMaterial,
    pub float_shadow: ShadowMaterial,
    pub row_inset: Pixels,
    pub primary: PrimaryStyle,
    pub flat_highlights: bool,
    pub bright_surfaces: bool,
    pub highlight_strength: f32,
    pub glass: Option<GlassMaterial>,
    pub styles: HashMap<SharedString, StylePatch>,
    pub inspect: Option<SharedString>,
    pub inspect_strength: f32,
    pub pip_opacity: f32,
    pub pane_margin: Pixels,
    pub pane_frames: bool,
    pub pane_glow_strength: f32,
    pub transparent: Hsla,
    pub scrollbar_show: ScrollbarShow,
    pub notification: NotificationSettings,
}

impl Default for Theme {
    fn default() -> Self {
        Self::from(&*ThemeColor::light())
    }
}

pub const CORNER_SMOOTHING: f32 = 4.0;
pub const ADAPTIVE_CORNER_FRACTION: f32 = 0.45;
pub const PANE_HEADER_CONTROL_HEIGHT: Pixels = px(22.0);
pub const PANE_HEADER_HEIGHT: Pixels = px(22.0 + 2.0 * CHROME_GAP);
const FLUSH_PANE_HEADER_HEIGHT: Pixels = px(40.0);
const PANE_VEIL: [f32; 2] = [0.05, 0.18];
const PANE_WASH: [f32; 2] = [0.3, 0.05];
const PANE_RIM_DIM: f32 = 0.3;

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
        let radius = self.control_radius.unwrap_or(self.radius);
        if radius > px(24.0) {
            zz_gpui::FULL_CORNER_RADIUS
        } else {
            radius
        }
    }

    pub fn swatch_radius(&self) -> Pixels {
        self.control_radius()
    }

    pub fn selection_background(&self) -> Hsla {
        self.accent.opaque()
    }

    pub fn menu_radius(&self) -> Pixels {
        (self.radius - px(4.0)).max(px(0.0))
    }

    pub fn row_radius(&self, row_height: Pixels) -> Pixels {
        if self.inset_rows() {
            self.menu_radius().min(row_height * 0.40)
        } else {
            px(0.0)
        }
    }

    pub fn inset_rows(&self) -> bool {
        self.row_inset > px(0.0)
    }

    pub fn outlined(&self) -> bool {
        self.outline > px(0.0)
    }

    pub fn outlines_edges(&self) -> bool {
        self.outlined() && !self.picked_outlines
    }

    pub fn glass_effect(&self) -> Option<GlassMaterial> {
        self.glass.filter(|material| !material.is_opaque_fill())
    }

    pub fn casts_shadow(&self, role: SurfaceRole) -> bool {
        !self.shadows_of(self.style(role).shadow).is_empty()
    }

    pub fn pane_color(&self, fill: Hsla) -> Hsla {
        self.background.opaque().blend(fill)
    }

    pub fn lights_active_pane(&self) -> bool {
        self.pane_glow_strength > 0.0 && self.style(Kind::Pane.role()).selected != Highlight::None
    }

    pub fn pane_frame(&self, plane: Hsla, active: bool) -> GlassMaterial {
        let glass = self.glass_effect();
        let tint = match glass {
            Some(material) if self.bright_surfaces && !self.mode.is_dark() => self
                .raise(plane.opaque(), 1)
                .alpha(if plane.a < 1.0 { material.tint.a } else { 1.0 }),
            _ if plane.a < 1.0 => plane.alpha(0.0),
            _ => plane,
        };
        let tint = self.pane_focus(tint, active);
        let frame = glass
            .unwrap_or_else(|| {
                let width = self.pane_outline();
                let edge = GlassMaterial::edge(tint).glint_width(width);
                if width > px(0.0) {
                    edge
                } else {
                    edge.specular(0.0)
                }
            })
            .blur(px(0.0))
            .refraction(px(0.0))
            .dispersion(0.0)
            .tint(tint)
            .fallback(tint);
        frame.specular(frame.specular * self.pane_rim(active))
    }

    fn pane_focus(&self, tint: Hsla, active: bool) -> Hsla {
        if !self.lights_active_pane() {
            return tint;
        }
        let (pole, weights) = if active {
            (zz_gpui::white(), PANE_WASH)
        } else {
            (zz_gpui::black(), PANE_VEIL)
        };
        let weight = weights[usize::from(self.mode.is_dark())] * self.pane_glow_strength;
        tint.blend(pole.alpha(weight.min(1.0)))
    }

    fn pane_rim(&self, active: bool) -> f32 {
        if active || !self.lights_active_pane() {
            1.0
        } else {
            (1.0 - PANE_RIM_DIM * self.pane_glow_strength).max(0.0)
        }
    }

    pub fn pane_edge_color(&self, active: bool) -> Hsla {
        let divider = self.divider_color();
        divider.alpha(divider.a * self.pane_rim(active))
    }

    pub fn pane_edge_width(&self) -> Pixels {
        if self.pane_frames {
            self.pane_outline()
        } else {
            px(0.0)
        }
    }

    fn pane_outline(&self) -> Pixels {
        if self.style(Kind::Pane.role()).outline {
            self.outline
        } else {
            px(0.0)
        }
    }

    pub fn pane_radius(&self) -> Pixels {
        if self.radius <= px(0.0) {
            return px(0.0);
        }
        let control = size(PANE_HEADER_CONTROL_HEIGHT, PANE_HEADER_CONTROL_HEIGHT);
        Corners::all(self.control_radius())
            .resolve_radii_for_quad_size(control, ADAPTIVE_CORNER_FRACTION)
            .top_left
            + px(CHROME_GAP)
    }

    pub fn pane_header_height(&self) -> Pixels {
        if self.pane_margin > px(0.0) {
            PANE_HEADER_HEIGHT
        } else {
            FLUSH_PANE_HEADER_HEIGHT
        }
    }

    pub fn title_bar_height(&self) -> Pixels {
        self.pane_header_height() + self.pane_margin * 2.0
    }

    pub fn divider_color(&self) -> Hsla {
        self.foreground.opacity(color::edge_weight())
    }

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
            control_radius: None,
            contrast: 1.0,
            outline: px(0.5),
            picked_outlines: false,
            picked_shadows: false,
            smoothing: Smoothing::THEME,
            picked_smoothing: false,
            contact_shadow: ShadowMaterial::CONTACT,
            float_shadow: ShadowMaterial::FLOAT,
            row_inset: px(4.0),
            primary: PrimaryStyle::Foreground,
            flat_highlights: false,
            bright_surfaces: false,
            highlight_strength: 1.0,
            glass: None,
            styles: HashMap::default(),
            inspect: None,
            inspect_strength: 1.0,
            pip_opacity: 0.8,
            pane_margin: px(0.0),
            pane_frames: false,
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
                    .resolve_radii_for_quad_size(size, ADAPTIVE_CORNER_FRACTION);
                let surface = zz_gpui::Corners::all(theme.radius)
                    .resolve_radii_for_quad_size(size, ADAPTIVE_CORNER_FRACTION);
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
    fn a_pane_frame_lights_an_opaque_edge_with_or_without_the_looks_glass() {
        let mut theme = Theme {
            pane_glow_strength: 0.0,
            ..Theme::default()
        };
        let plane = theme.background;
        let tint = plane;
        let edge = GlassMaterial::edge(tint).glint_width(theme.outline);
        let frosted = GlassMaterial::frosted();
        for (glass, rim) in [
            (None, edge),
            (
                Some(GlassMaterial::regular().vanished().tint(plane.opaque())),
                edge,
            ),
            (Some(frosted), frosted),
        ] {
            theme.glass = glass;
            let frame = theme.pane_frame(plane, false);
            assert_eq!(frame.tint, tint);
            assert_eq!(frame.fallback, Some(tint));
            assert!(!frame.reads_backdrop());
            assert!(!frame.is_opaque_fill());
            assert_eq!(
                (frame.blur, frame.refraction, frame.dispersion),
                (px(0.0), px(0.0), 0.0)
            );
            assert_eq!(
                (
                    frame.bezel,
                    frame.glint_width,
                    frame.specular,
                    frame.fresnel
                ),
                (rim.bezel, rim.glint_width, rim.specular, rim.fresnel)
            );
            assert_eq!(
                (frame.noise, frame.light_color),
                (rim.noise, rim.light_color)
            );
        }

        theme.outline = px(0.0);
        assert_eq!(theme.pane_frame(plane, false).specular, frosted.specular);
        theme.glass = None;
        assert_eq!(theme.pane_frame(plane, false).specular, 0.0);
        assert_eq!(theme.pane_edge_width(), px(0.0));
        theme.outline = px(2.0);
        assert_eq!(theme.pane_edge_width(), px(0.0));
        theme.pane_frames = true;
        assert_eq!(theme.pane_edge_width(), px(2.0));
    }

    #[test]
    fn a_pane_frame_over_a_translucent_plane_adds_nothing_to_it() {
        let theme = Theme {
            pane_glow_strength: 0.0,
            ..Theme::default()
        };
        let plane = theme.background.alpha(0.93);
        assert_eq!(theme.pane_frame(plane, false).tint.a, 0.0);
        assert_eq!(theme.pane_frame(plane.opaque(), false).tint.a, 1.0);
    }

    #[test]
    fn bright_glass_panes_rise_in_light_mode_only() {
        let mut light = (*ThemeColor::light()).clone();
        light.background = zz_gpui::hsla(0., 0., 0.97, 1.);
        for (mode, colors) in [
            (ThemeMode::Light, &light),
            (ThemeMode::Dark, &*ThemeColor::dark()),
        ] {
            let mut theme = Theme::from(colors);
            theme.mode = mode;
            let glass = GlassMaterial::regular();
            theme.glass = Some(glass);
            theme.pane_glow_strength = 0.0;
            for alpha in [0.7, 1.0] {
                let plane = theme.background.alpha(alpha);
                let plain = theme.pane_frame(plane, false);
                theme.bright_surfaces = true;
                let bright = theme.pane_frame(plane, false);
                if theme.mode.is_dark() {
                    assert_eq!(bright, plain);
                } else {
                    assert!(bright.tint.l > plane.l);
                    assert_eq!(bright.tint.a, if alpha < 1.0 { glass.tint.a } else { 1.0 });
                    assert_eq!(bright.fallback, Some(bright.tint));
                }
                theme.bright_surfaces = false;
            }
        }
    }

    #[test]
    fn a_pane_rounds_concentric_with_its_header_controls() {
        let control = size(px(96.0), PANE_HEADER_CONTROL_HEIGHT);
        for radius in [4.0, 10.0, 24.0, 25.0] {
            let theme = Theme {
                radius: px(radius),
                ..Theme::default()
            };
            let rounded = Corners::all(theme.control_radius())
                .resolve_radii_for_quad_size(control, ADAPTIVE_CORNER_FRACTION);
            let gap = theme.pane_radius() - rounded.top_left;
            assert!((gap - px(CHROME_GAP)).abs() < px(0.001));
        }
        let capsules = Theme {
            radius: px(25.0),
            ..Theme::default()
        };
        assert_eq!(capsules.pane_radius(), PANE_HEADER_HEIGHT / 2.0);
        let square = Theme {
            radius: px(0.0),
            ..Theme::default()
        };
        assert_eq!(square.pane_radius(), px(0.0));
    }

    #[test]
    fn the_title_bar_shares_the_pane_headers_center_line() {
        let mut theme = Theme::default();
        assert_eq!(theme.title_bar_height(), FLUSH_PANE_HEADER_HEIGHT);
        theme.pane_margin = px(8.0);
        assert_eq!(
            theme.title_bar_height() / 2.0,
            theme.pane_margin + PANE_HEADER_HEIGHT / 2.0
        );
    }

    #[test]
    fn the_active_pane_stands_apart_by_light_not_color() {
        let mut theme = Theme::default();
        let plane = theme.background;
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            theme.mode = mode;
            let active = theme.pane_frame(plane, true);
            let inactive = theme.pane_frame(plane, false);
            assert!(
                crate::oklab_lightness(inactive.tint) < crate::oklab_lightness(active.tint),
                "{mode:?}"
            );
            assert_eq!(active.light_color, inactive.light_color);
            assert!(inactive.specular < active.specular);
            assert!(theme.pane_edge_color(false).a < theme.pane_edge_color(true).a);
        }
        theme.pane_glow_strength = 0.0;
        assert_eq!(
            theme.pane_frame(plane, true),
            theme.pane_frame(plane, false)
        );
        assert_eq!(theme.pane_edge_color(false), theme.divider_color());
    }
}
