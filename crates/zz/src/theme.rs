//! zz's theme overrides, layered over zz-ui's base palette.

use std::sync::Arc;

use zz_gpui::{App, Entity, Global, Hsla, SharedString, Window};
use zz_terminal::TerminalAppearance;
use zz_ui::{Theme, ThemeMode};

use crate::{config, mux::client::MuxClient};

pub use zz_ui::interface_theme::ThemeModeSetting;

#[derive(Clone, Copy)]
struct SystemThemeMode(ThemeMode);

impl Global for SystemThemeMode {}

#[derive(Clone)]
struct LatestTerminalAppearance(Arc<TerminalAppearance>);

impl Global for LatestTerminalAppearance {}

pub(crate) fn set_terminal_appearance(appearance: Arc<TerminalAppearance>, cx: &mut App) {
    cx.set_global(LatestTerminalAppearance(appearance));
    config::apply_window_background_appearance(cx);
    refresh_current_theme(cx);
}

pub(crate) fn terminal_appearance(cx: &App) -> Option<Arc<TerminalAppearance>> {
    cx.try_global::<LatestTerminalAppearance>()
        .map(|appearance| Arc::clone(&appearance.0))
}

pub(crate) fn chrome_blur(cx: &App) -> bool {
    config::resolved_config(cx).window_background_blur.value
        && crate::window::background::compositor_supports_blur(cx)
}

/// The chrome planes' fill: translucent while the blur is on, the base plane otherwise.
pub fn chrome_background(cx: &App) -> Hsla {
    zz_ui::shell::chrome_background(Theme::global(cx).background, chrome_blur(cx))
}

pub fn app_pane_background(cx: &App) -> Hsla {
    let theme = Theme::global(cx);
    theme.pane_background(theme.background)
}

pub(crate) fn report_terminal_color_scheme(mux: &Entity<MuxClient>, cx: &mut App) {
    let mux = mux.downgrade();
    cx.observe_global::<Theme>(move |cx| {
        let color_scheme = crate::terminal_color_scheme(Theme::global(cx).mode);
        mux.update(cx, |mux, _| mux.set_color_scheme(color_scheme))
            .ok();
    })
    .detach();
}

pub(crate) fn refresh_current_theme(cx: &mut App) {
    if !cx.has_global::<Theme>() {
        return;
    }
    let mode =
        zz_ui::interface_theme::pinned_theme_mode(config::theme_mode(cx)).unwrap_or_else(|| {
            cx.try_global::<SystemThemeMode>()
                .map_or_else(|| Theme::global(cx).mode, |mode| mode.0)
        });
    Theme::change(mode, None, cx);
    apply_zz_overrides(cx);
    let corner_smoothing = zz_ui::CORNER_SMOOTHING;
    for window in cx.windows() {
        window
            .update(cx, |_, window, _| {
                window.set_default_corner_smoothing(corner_smoothing);
                window.set_adaptive_corner_fraction(Some(ADAPTIVE_CORNER_FRACTION));
            })
            .ok();
    }
    cx.refresh_windows();
}

const ADAPTIVE_CORNER_FRACTION: f32 = 0.45;

/// Sync the zz-ui theme with the OS appearance, then reapply zz's overrides.
pub fn sync_system_appearance(mut window: Option<&mut Window>, cx: &mut App) {
    let system_mode = window
        .as_ref()
        .map_or_else(|| cx.window_appearance(), |window| window.appearance())
        .into();
    cx.set_global(SystemThemeMode(system_mode));
    if let Some(window) = window.as_deref_mut() {
        window.set_default_corner_smoothing(zz_ui::CORNER_SMOOTHING);
        window.set_adaptive_corner_fraction(Some(ADAPTIVE_CORNER_FRACTION));
    }
    Theme::sync_system_appearance(window, cx);
    crate::app_icon::apply(cx);
    if let Some(mode) = zz_ui::interface_theme::pinned_theme_mode(config::theme_mode(cx)) {
        Theme::change(mode, None, cx);
    }
    apply_zz_overrides(cx);
}

fn apply_zz_overrides(cx: &mut App) {
    let appearance = cx
        .try_global::<LatestTerminalAppearance>()
        .map(|appearance| Arc::clone(&appearance.0));
    let terminal_mono_font_family = appearance
        .as_deref()
        .map(crate::terminal::view::terminal_font)
        .map(|font| font.family);
    let active = config::shown_theme(cx);
    let ui_font = config::ui_font_family(cx);
    let ui_font_family = ui_font
        .value
        .map(SharedString::from)
        .or_else(|| {
            (ui_font.provenance == config::ConfigProvenance::Default)
                .then(|| active.materials.font.clone())
                .flatten()
        })
        .filter(|family| cx.text_system().has_font(family))
        .unwrap_or_else(|| zz_gpui::Font::default().family);
    let chrome_contrast = config::chrome_contrast(cx);
    let pane_background_opacity = config::pane_background_opacity(cx);
    let pane_glow_strength = config::resolved_config(cx).pane_glow_strength.value;
    let pane_frames = config::pane_gaps(cx);
    let theme = Theme::global_mut(cx);

    theme.colors = active.colors(theme.mode);
    active.materials.apply(theme);
    active.apply_roles(theme);
    theme.font_family = ui_font_family;
    if let Some(font_family) = terminal_mono_font_family {
        theme.mono_font_family = font_family;
    }

    theme.set_contrast(chrome_contrast);
    theme.pane_background_opacity = pane_background_opacity;
    theme.pane_glow_strength = pane_glow_strength;
    theme.pane_frames = pane_frames;
}

#[cfg(test)]
mod tests {
    use super::*;
    use zz_ui::interface_theme::builtin_themes;

    const SEPARATOR_DELTA_FLOOR_LIGHT: f32 = 0.045;
    const SEPARATOR_DELTA_FLOOR_DARK: f32 = 0.060;
    const SEPARATOR_DELTA_CEILING: f32 = 0.160;

    #[test]
    fn separators_stay_legible_without_reading_as_rules() {
        for (id, theme) in builtin_themes() {
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                let floor = if mode.is_dark() {
                    SEPARATOR_DELTA_FLOOR_DARK
                } else {
                    SEPARATOR_DELTA_FLOOR_LIGHT
                };
                let colors = theme.colors(mode);
                let hairline = colors.border();
                let delta = (zz_ui::oklab_lightness(hairline)
                    - zz_ui::oklab_lightness(colors.background))
                .abs();

                assert!(
                    (floor..=SEPARATOR_DELTA_CEILING).contains(&delta),
                    "{id} {mode:?}: border {} is {:.1}% from background {}, outside {:.1}%..={:.1}%",
                    zz_ui::to_hex(hairline),
                    delta * 100.0,
                    zz_ui::to_hex(colors.background),
                    floor * 100.0,
                    SEPARATOR_DELTA_CEILING * 100.0,
                );
            }
        }
    }

    #[zz_gpui::test]
    fn the_selected_theme_paints_the_mode_it_is_in(cx: &mut zz_gpui::TestAppContext) {
        cx.update(zz_ui::init);
        cx.update(|cx| {
            let default = zz_ui::interface_theme::default_theme();
            cx.set_global(config::Themes {
                active: default.clone(),
                selected: "default".into(),
                saved: Vec::new(),
                error: None,
            });
            for (setting, mode) in [
                (ThemeModeSetting::Light, ThemeMode::Light),
                (ThemeModeSetting::Dark, ThemeMode::Dark),
            ] {
                let mut config = config::AppConfig::default();
                config.theme_mode.value = setting;
                cx.set_global(config);
                refresh_current_theme(cx);
                let theme = Theme::global(cx);
                assert_eq!(theme.colors, default.colors(mode));
                assert!(theme.glass.is_none());
            }
        });
    }

    #[test]
    fn only_an_explicit_mode_pins_the_colors() {
        assert_eq!(
            zz_ui::interface_theme::pinned_theme_mode(ThemeModeSetting::System),
            None
        );
        assert_eq!(
            zz_ui::interface_theme::pinned_theme_mode(ThemeModeSetting::Light),
            Some(ThemeMode::Light)
        );
        assert_eq!(
            zz_ui::interface_theme::pinned_theme_mode(ThemeModeSetting::Dark),
            Some(ThemeMode::Dark)
        );
    }

    #[zz_gpui::test]
    fn returning_to_system_restores_the_last_os_mode(cx: &mut zz_gpui::TestAppContext) {
        cx.update(zz_ui::init);
        cx.update(|cx| {
            cx.set_global(SystemThemeMode(ThemeMode::Dark));
            let mut config = config::AppConfig::default();
            config.theme_mode.value = ThemeModeSetting::Light;
            cx.set_global(config);
            refresh_current_theme(cx);
            assert_eq!(Theme::global(cx).mode, ThemeMode::Light);

            config.theme_mode.value = ThemeModeSetting::System;
            cx.set_global(config);
            refresh_current_theme(cx);
            assert_eq!(Theme::global(cx).mode, ThemeMode::Dark);
        });
    }

    #[zz_gpui::test]
    fn terminal_opacity_never_reaches_chrome_or_app_panes(cx: &mut zz_gpui::TestAppContext) {
        cx.update(zz_ui::init);
        cx.update(|cx| {
            set_terminal_appearance(
                Arc::new(TerminalAppearance {
                    background_opacity: 0.25,
                    ..TerminalAppearance::default()
                }),
                cx,
            );
        });

        cx.update(|cx| {
            assert!(!chrome_blur(cx));
            assert_alpha(Theme::global(cx).background, 1.0);
            assert_alpha(chrome_background(cx), 1.0);
            assert_alpha(app_pane_background(cx), 0.5);
        });

        cx.update(|cx| {
            let mut config = config::AppConfig::default();
            config.window_background_blur.value = true;
            cx.set_global(config);
            refresh_current_theme(cx);
        });

        cx.update(|cx| {
            assert!(chrome_blur(cx));
            assert_alpha(Theme::global(cx).background, 1.0);
            assert_alpha(chrome_background(cx), 0.93);
            assert_alpha(app_pane_background(cx), 0.5);
        });
    }

    #[zz_gpui::test]
    fn gapped_panes_draw_frames_in_every_theme(cx: &mut zz_gpui::TestAppContext) {
        cx.update(zz_ui::init);
        cx.update(|cx| {
            let default = zz_ui::interface_theme::default_theme().clone();
            let glass = zz_ui::interface_theme::builtin_theme("liquid-glass")
                .expect("the Liquid Glass theme")
                .clone();
            for (id, theme, gaps) in [
                ("default", &default, true),
                ("default", &default, false),
                ("liquid-glass", &glass, true),
                ("liquid-glass", &glass, false),
            ] {
                cx.set_global(config::Themes {
                    active: theme.clone(),
                    selected: "default".into(),
                    saved: Vec::new(),
                    error: None,
                });
                let mut config = config::AppConfig::default();
                config.pane_gaps.value = gaps;
                cx.set_global(config);
                refresh_current_theme(cx);
                let theme = Theme::global(cx);
                assert_eq!(theme.pane_frames, gaps, "{id} with gaps {gaps}");
                assert_eq!(
                    theme.pane_edge_width(),
                    if gaps {
                        theme.outline
                    } else {
                        zz_gpui::px(0.0)
                    },
                    "{id} with gaps {gaps}"
                );
                assert_alpha(app_pane_background(cx), if gaps { 0.0 } else { 0.5 });
            }
        });
    }

    #[zz_gpui::test]
    fn app_panes_ignore_translucent_chrome_overrides(cx: &mut zz_gpui::TestAppContext) {
        cx.update(zz_ui::init);
        cx.update(|cx| {
            Theme::global_mut(cx).colors.background =
                zz_ui::parse_hex("#10203066").expect("test background parses");

            assert_alpha(Theme::global(cx).background, 0.4);
            assert_alpha(app_pane_background(cx), 0.5);
        });
    }

    #[zz_gpui::test]
    fn unsupported_compositor_keeps_the_chrome_opaque(cx: &mut zz_gpui::TestAppContext) {
        cx.update(zz_ui::init);
        cx.update(|cx| {
            let mut config = config::AppConfig::default();
            config.window_background_blur.value = true;
            cx.set_global(config);
            crate::window::background::set_compositor_support_for_tests(false, cx);
        });

        cx.update(|cx| {
            assert!(!chrome_blur(cx));
            assert_alpha(chrome_background(cx), 1.0);
        });
    }

    #[track_caller]
    fn assert_alpha(color: Hsla, expected: f32) {
        assert!(
            (color.a - expected).abs() < f32::EPSILON,
            "alpha {} is not {expected}",
            color.a
        );
    }
}
