use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use ala::{
    Anchor, App, Edges, GlassMaterial, Global, Hsla, Pixels, SharedString, Window,
    WindowAppearance, px,
};

use crate::{BASE_UI_FONT_SIZE, TITLE_BAR_HEIGHT, foundation::highlighter::HighlightTheme};

use super::{Colorize as _, ShadowMaterial, Smoothing, Space, Surface, ThemeColor, color};

pub const CHROME_GAP: f32 = Space::INSET_CHROME.design_px();

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
    pub smoothing: Smoothing,
    pub contact_shadow: ShadowMaterial,
    pub float_shadow: ShadowMaterial,
    pub row_inset: Pixels,
    pub glass: Option<GlassMaterial>,
    pub inspect: Option<SharedString>,
    pub inspect_strength: f32,
    pub pane_margin: Pixels,
    pub window_opacity: f32,
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
pub const DEFAULT_RADIUS: f32 = 12.0;
pub const ADAPTIVE_CORNER_FRACTION: f32 = 0.45;
pub const PANE_HEADER_CONTROL_HEIGHT: Pixels = px(22.0);
const MAX_PANE_RADIUS: f32 = 16.0;
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
        self.control_radius.unwrap_or(self.radius)
    }

    pub fn pill_controls(&self) -> bool {
        self.control_radius() >= ala::FULL_CORNER_RADIUS
    }

    pub fn swatch_radius(&self) -> Pixels {
        self.control_radius()
    }

    pub fn accent_ink(&self) -> Hsla {
        self.on_accent
            .unwrap_or_else(|| super::color::mark_on(self.accent.opaque()))
    }

    pub fn control_roundness(&self) -> f32 {
        let radius = if self.pill_controls() {
            self.radius
        } else {
            self.control_radius()
        };
        ADAPTIVE_CORNER_FRACTION * super::interface_theme::corner_share(f32::from(radius))
    }

    pub fn control_corner(&self, height: Pixels) -> Pixels {
        if self.pill_controls() {
            height / 2.0
        } else {
            height * self.control_roundness()
        }
    }

    pub fn inset_rows(&self) -> bool {
        self.row_inset > px(0.0)
    }

    pub fn outlined(&self) -> bool {
        self.outline > px(0.0)
    }

    pub fn glass_effect(&self) -> Option<GlassMaterial> {
        self.glass.filter(|material| !material.is_opaque_fill())
    }

    pub fn casts_shadow(&self, surface: impl Into<Surface>) -> bool {
        !self.shadows_of(self.style_of(surface).shadow).is_empty()
    }

    pub fn pane_radius(&self) -> Pixels {
        let share = super::interface_theme::corner_share(f32::from(self.radius));
        let knee = MAX_PANE_RADIUS / super::interface_theme::MAX_CORNER_RADIUS;
        if share <= knee {
            return self.radius.max(px(0.0));
        }
        px(MAX_PANE_RADIUS * (1.0 - (1.0 - share).powi(2) / (1.0 - knee)))
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

    pub fn title_bar_over_panes_height(&self) -> Pixels {
        self.pane_header_height() + self.pane_margin
    }

    pub fn divider_color(&self) -> Hsla {
        self.foreground.opacity(color::divider_weight())
    }

    pub fn contrast(&self) -> f32 {
        self.contrast
    }

    pub fn set_colors(&mut self, colors: ThemeColor) {
        color::set_muted_ceiling(colors.foreground, colors.background);
        self.colors = colors;
        self.highlight_theme =
            HighlightTheme::with_diff_inks(&self.highlight_theme, colors.success, colors.danger);
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
        theme.highlight_theme = if mode.is_dark() {
            HighlightTheme::default_dark()
        } else {
            HighlightTheme::default_light()
        };
        theme.set_colors(colors);

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

pub fn platform_mono_font_family() -> SharedString {
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        "Menlo".into()
    } else if cfg!(target_os = "windows") {
        "Consolas".into()
    } else if cfg!(target_family = "wasm") {
        "Lilex".into()
    } else {
        "DejaVu Sans Mono".into()
    }
}

impl From<&ThemeColor> for Theme {
    fn from(colors: &ThemeColor) -> Self {
        Theme {
            mode: ThemeMode::default(),
            transparent: Hsla::transparent_black(),
            font_family: ".SystemUIFont".into(),
            font_size: px(BASE_UI_FONT_SIZE),
            mono_font_family: platform_mono_font_family(),
            mono_font_size: px(13.),
            radius: px(DEFAULT_RADIUS),
            control_radius: None,
            contrast: 1.0,
            outline: px(1.0),
            smoothing: Smoothing::THEME,
            contact_shadow: ShadowMaterial::CONTACT,
            float_shadow: ShadowMaterial::FLOAT,
            row_inset: px(6.0),
            glass: None,
            inspect: None,
            inspect_strength: 1.0,
            pane_margin: px(0.0),
            window_opacity: 1.0,
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
    fn only_a_full_control_radius_draws_pills() {
        for (radius, control_radius) in [
            (25.0, None),
            (24.5, Some(px(25.0))),
            (6.0, Some(ala::FULL_CORNER_RADIUS)),
        ] {
            let theme = Theme {
                radius: px(radius),
                control_radius,
                ..Theme::default()
            };
            let control = theme.control_corner(px(30.0));
            let pill = control_radius == Some(ala::FULL_CORNER_RADIUS);
            assert_eq!(theme.pill_controls(), pill, "{radius} {control_radius:?}");
            assert_eq!(control == px(15.0), pill, "{radius} {control_radius:?}");
        }
    }

    #[test]
    fn every_control_rounds_to_one_share_of_its_height_and_a_pane_eases_from_the_radius_to_sixteen()
    {
        let theme = Theme::default();
        assert!((theme.control_roundness() - 0.31177).abs() < 1e-4);
        assert!((f32::from(theme.control_corner(px(28.0))) - 8.7295).abs() < 1e-3);
        assert!((f32::from(theme.control_corner(px(46.0))) - 14.3414).abs() < 1e-3);
        let widest = Theme {
            radius: px(25.0),
            ..Theme::default()
        };
        assert!((f32::from(widest.control_corner(px(28.0))) - 12.6).abs() < 1e-4);
        for (radius, pane) in [
            (4.0, 4.0),
            (10.24, 10.24),
            (12.0, 11.8063),
            (16.0, 14.2222),
            (20.25, 15.5556),
            (25.0, 16.0),
        ] {
            for control_radius in [None, Some(ala::FULL_CORNER_RADIUS)] {
                let theme = Theme {
                    radius: px(radius),
                    control_radius,
                    ..Theme::default()
                };
                assert!(
                    (f32::from(theme.pane_radius()) - pane).abs() < 1e-3,
                    "{radius} {control_radius:?}"
                );
            }
        }
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
    fn a_title_bar_over_the_panes_leaves_one_margin_and_matches_the_flush_bar_at_six() {
        let mut theme = Theme::default();
        assert_eq!(
            theme.title_bar_over_panes_height(),
            FLUSH_PANE_HEADER_HEIGHT
        );
        theme.pane_margin = px(6.0);
        assert_eq!(
            theme.title_bar_over_panes_height(),
            FLUSH_PANE_HEADER_HEIGHT
        );
        theme.pane_margin = px(10.0);
        assert_eq!(theme.title_bar_over_panes_height(), px(44.0));
    }
}
