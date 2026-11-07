use std::sync::Arc;

use ala::{App, Entity, Global, Hsla, SharedString, Window};
use penna::{Theme, ThemeMode};
use zz_terminal::TerminalAppearance;
use zz_ui::pane_look::PaneLook;

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

pub fn shell_frame(cx: &App) -> Option<Hsla> {
    zz_ui::shell::shell_frame(
        chrome_background(cx),
        config::pane_margin(cx) > ala::px(0.0),
        cx,
    )
}

pub fn chrome_background(cx: &App) -> Hsla {
    zz_ui::shell::chrome_background(Theme::global(cx).background, chrome_blur(cx))
}

pub(crate) fn report_terminal_color_scheme(mux: &Entity<MuxClient>, cx: &mut App) {
    let mux = mux.downgrade();
    cx.observe_global::<Theme>(move |cx| {
        let color_scheme = crate::terminal_color_scheme(asked_mode(cx));
        mux.update(cx, |mux, _| mux.set_color_scheme(color_scheme))
            .ok();
    })
    .detach();
}

pub(crate) fn asked_mode(cx: &App) -> ThemeMode {
    zz_ui::interface_theme::pinned_theme_mode(config::theme_mode(cx)).unwrap_or_else(|| {
        cx.try_global::<SystemThemeMode>()
            .map_or_else(|| Theme::global(cx).mode, |mode| mode.0)
    })
}

pub(crate) fn refresh_current_theme(cx: &mut App) {
    if !cx.has_global::<Theme>() {
        return;
    }
    let mode = asked_mode(cx);
    config::shown_theme(cx).activate(Some(mode), None, cx);
    apply_zz_overrides(cx);
    for window in cx.windows() {
        window
            .update(cx, |_, window, cx| {
                window.set_adaptive_corner_fraction(Some(penna::ADAPTIVE_CORNER_FRACTION));
                window.set_apparent_corners(true);
                crate::ui_scale::place_traffic_lights(window, cx);
            })
            .ok();
    }
    cx.refresh_windows();
}

pub fn sync_system_appearance(mut window: Option<&mut Window>, cx: &mut App) {
    let system_mode = window
        .as_ref()
        .map_or_else(|| cx.window_appearance(), |window| window.appearance())
        .into();
    cx.set_global(SystemThemeMode(system_mode));
    if let Some(window) = window.as_deref_mut() {
        window.set_adaptive_corner_fraction(Some(penna::ADAPTIVE_CORNER_FRACTION));
        window.set_apparent_corners(true);
    }
    config::shown_theme(cx).activate(
        zz_ui::interface_theme::pinned_theme_mode(config::theme_mode(cx)),
        window,
        cx,
    );
    crate::app_icon::apply(cx);
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
    let ui_font = config::appearance(cx).ui_font().to_owned();
    let ui_font_family = Some(ui_font)
        .filter(|family| !family.is_empty() && cx.text_system().has_font(family))
        .map_or_else(|| ala::Font::default().family, SharedString::from);
    let chrome_contrast = config::chrome_contrast(cx);
    let reduce_transparency = config::resolved_config(cx).reduce_transparency.value;
    let pane_look = PaneLook {
        frames: config::pane_gaps(cx),
        glow_strength: config::resolved_config(cx).pane_glow_strength.value,
    };
    let pane_margin = config::pane_margin(cx);
    let window_opacity = chrome_background(cx).a;
    cx.set_global(pane_look);
    let theme = Theme::global_mut(cx);

    theme.font_family = ui_font_family;
    if let Some(font_family) = terminal_mono_font_family {
        theme.mono_font_family = font_family;
    }

    theme.set_contrast(chrome_contrast);
    if reduce_transparency {
        theme.glass = None;
    }
    theme.pane_margin = pane_margin;
    theme.window_opacity = window_opacity;
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
                let delta = (penna::oklab_lightness(hairline)
                    - penna::oklab_lightness(colors.background))
                .abs();

                assert!(
                    (floor..=SEPARATOR_DELTA_CEILING).contains(&delta),
                    "{id} {mode:?}: border {} is {:.1}% from background {}, outside {:.1}%..={:.1}%",
                    penna::to_hex(hairline),
                    delta * 100.0,
                    penna::to_hex(colors.background),
                    floor * 100.0,
                    SEPARATOR_DELTA_CEILING * 100.0,
                );
            }
        }
    }

    #[ala::test]
    fn the_platform_look_paints_the_mode_it_is_in(cx: &mut ala::TestAppContext) {
        cx.update(zz_ui::init);
        cx.update(|cx| {
            let platform = zz_ui::interface_theme::platform_theme();
            for (setting, mode, reduce) in [
                (ThemeModeSetting::Light, ThemeMode::Light, false),
                (ThemeModeSetting::Dark, ThemeMode::Dark, true),
            ] {
                let mut config = config::AppConfig::default();
                config.theme_mode.value = setting;
                config.reduce_transparency.value = reduce;
                cx.set_global(config);
                refresh_current_theme(cx);
                let theme = Theme::global(cx);
                assert_eq!(theme.colors, platform.colors(mode));
                assert_eq!(theme.glass.is_none(), reduce, "{mode:?}");
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

    #[ala::test]
    fn returning_to_system_restores_the_last_os_mode(cx: &mut ala::TestAppContext) {
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

    #[ala::test]
    fn terminal_opacity_never_reaches_the_chrome(cx: &mut ala::TestAppContext) {
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
            assert_alpha(Theme::global(cx).ground().fill, 1.0);
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
            assert_alpha(Theme::global(cx).ground().fill, 0.93);
        });

        cx.update(|cx| {
            let mut config = config::AppConfig::default();
            config.window_background_blur.value = true;
            cx.set_global(config);
            assert_eq!(shell_frame(cx), None);

            config.pane_gaps.value = true;
            config.pane_margin.value = 8.0;
            for blur in [true, false] {
                config.window_background_blur.value = blur;
                cx.set_global(config);
                let theme = Theme::global(cx);
                let sidebar = theme.layer(
                    penna::Surface::new(penna::foundation::roles::SIDEBAR).on(penna::Ground {
                        fill: chrome_background(cx),
                        ..theme.ground()
                    }),
                    penna::PaintState::Rest,
                );
                assert_eq!(shell_frame(cx), Some(sidebar.fill), "blur {blur}");
                assert_eq!(sidebar.fill.a < 1.0, blur, "blur {blur}");
            }
        });
    }

    #[ala::test]
    fn gapped_panes_draw_frames_with_and_without_glass(cx: &mut ala::TestAppContext) {
        cx.update(zz_ui::init);
        cx.update(|cx| {
            for (id, glass, gaps) in [
                ("liquid", "liquid", true),
                ("liquid", "liquid", false),
                ("none", "none", true),
                ("none", "none", false),
            ] {
                cx.set_global(config::GlassConfig(config::ConfigValue {
                    value: Some(glass.to_owned()),
                    provenance: config::ConfigProvenance::Override,
                }));
                let mut config = config::AppConfig::default();
                config.pane_gaps.value = gaps;
                cx.set_global(config);
                refresh_current_theme(cx);
                let theme = Theme::global(cx);
                let look = PaneLook::global(cx);
                assert_eq!(look.frames, gaps, "{id} with gaps {gaps}");
                let outlined = theme.style_of(penna::ContentLook::Pane).outline;
                assert_eq!(
                    look.edge_width(theme),
                    if gaps && outlined {
                        theme.outline
                    } else {
                        ala::px(0.0)
                    },
                    "{id} with gaps {gaps}"
                );
            }
        });
    }

    #[ala::test]
    fn pip_fills_ignore_translucent_chrome_overrides(cx: &mut ala::TestAppContext) {
        cx.update(zz_ui::init);
        cx.update(|cx| {
            Theme::global_mut(cx).colors.background =
                penna::parse_hex("#10203066").expect("test background parses");

            let theme = Theme::global(cx);
            assert_alpha(theme.background, 0.4);
            assert_alpha(zz_ui::pane_look::pane_color(theme, theme.background), 1.0);
        });
    }

    #[ala::test]
    fn unsupported_compositor_keeps_the_chrome_opaque(cx: &mut ala::TestAppContext) {
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
