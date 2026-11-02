use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use ala::{
    Anchor, App, Corners, Edges, GlassMaterial, Global, Hsla, Pixels, SharedString, Window,
    WindowAppearance, px, size,
};

use crate::{BASE_UI_FONT_SIZE, TITLE_BAR_HEIGHT, highlighter::HighlightTheme};

use super::{
    Colorize as _, PrimaryStyle, ShadowMaterial, Smoothing, StylePatch, SurfaceRole, ThemeColor,
    color,
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
    contrast: f32,
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
    pub(crate) styles: HashMap<SharedString, StylePatch>,
    pub inspect: Option<SharedString>,
    pub inspect_strength: f32,
    pub pane_margin: Pixels,
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
            ala::FULL_CORNER_RADIUS
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

    pub fn styles(&self) -> &HashMap<SharedString, StylePatch> {
        &self.styles
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

    pub fn contrast(&self) -> f32 {
        self.contrast
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
            pane_margin: px(0.0),
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
                let size = ala::size(px(width), px(height));
                let control = ala::Corners::all(theme.control_radius())
                    .resolve_radii_for_quad_size(size, ADAPTIVE_CORNER_FRACTION);
                let surface = ala::Corners::all(theme.radius)
                    .resolve_radii_for_quad_size(size, ADAPTIVE_CORNER_FRACTION);
                if radius > 24.0 {
                    assert_eq!(control, ala::Corners::all(px(height / 2.0)));
                    assert!(surface.top_left < control.top_left);
                } else {
                    assert_eq!(control, surface);
                }
                assert_eq!(theme.radius, px(radius));
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
}
