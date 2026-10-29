use crate::ThemeMode;

pub use penna::interface_theme::{
    ACCENTS, DEFAULT_THEME, DEFAULT_TINT_HUE, InterfaceTheme, MAX_CORNER_RADIUS, Materials,
    ThemeColors, ThemeOverrides, accent_color,
};
pub use zz_client::appearance::{
    AccentSetting, BUILTIN_THEMES, DEFAULT_ANIMATION_SPEED, MAX_ANIMATION_SPEED,
    MIN_ANIMATION_SPEED, ThemeModeSetting, is_builtin_theme,
};

pub(crate) const BUILTIN_SOURCES: [(&str, &str); BUILTIN_THEMES.len()] = [
    ("default", include_str!("../themes/default.json")),
    ("macos", include_str!("../themes/macos.json")),
    ("yaru", include_str!("../themes/yaru.json")),
];

pub fn parse_theme(source: &str) -> Result<InterfaceTheme, String> {
    crate::register();
    penna::interface_theme::parse_theme(source)
}

pub fn builtin_themes() -> &'static [(&'static str, InterfaceTheme)] {
    crate::register();
    penna::interface_theme::builtin_themes()
}

pub fn builtin_theme(id: &str) -> Option<&'static InterfaceTheme> {
    crate::register();
    penna::interface_theme::builtin_theme(id)
}

pub fn default_theme() -> &'static InterfaceTheme {
    crate::register();
    penna::interface_theme::default_theme()
}

pub const fn pinned_theme_mode(setting: ThemeModeSetting) -> Option<ThemeMode> {
    match setting.pinned() {
        Some(true) => Some(ThemeMode::Dark),
        Some(false) => Some(ThemeMode::Light),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Colorize as _, Elevation, Theme, ThemeColor};
    use ala::Hsla;

    fn luminance(color: Hsla) -> f32 {
        let rgba: ala::Rgba = color.into();
        let linear = |channel: f32| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(rgba.r) + 0.7152 * linear(rgba.g) + 0.0722 * linear(rgba.b)
    }

    fn contrast(left: Hsla, right: Hsla) -> f32 {
        let left = luminance(left);
        let right = luminance(right);
        (left.max(right) + 0.05) / (left.min(right) + 0.05)
    }

    #[test]
    fn the_composers_buttons_are_filled_in_every_builtin() {
        use crate::{PaintState, Surface, roles};
        for (id, theme) in builtin_themes() {
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                let mut kit = Theme::from(&*ThemeColor::for_mode(mode));
                kit.mode = mode;
                theme.apply(&mut kit);
                let pill = kit.layer(roles::AGENT_COMPOSER, PaintState::Rest).ground;
                for (button, prominent, least) in [("send and stop", true, 1.5), ("+", false, 1.05)]
                {
                    let layer = kit.layer(
                        Surface::new(roles::AGENT_SEND)
                            .prominent(prominent)
                            .on(pill),
                        PaintState::Rest,
                    );
                    let ratio = contrast(pill.fill.blend(layer.fill), pill.fill);
                    assert!(
                        layer.glass.is_some() || ratio >= least,
                        "{id} {mode:?} {button} is {ratio:.2}:1 on the composer"
                    );
                }
            }
        }
    }

    #[test]
    fn every_builtin_parses_with_its_id_and_a_name() {
        let ids = builtin_themes()
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        assert_eq!(ids, BUILTIN_THEMES);
        for (id, theme) in builtin_themes() {
            assert!(!theme.name.is_empty(), "{id} has no name");
            sets_every_root(id, &theme.light, &theme.dark);
        }
    }

    fn assert_reads(id: &str, theme: &InterfaceTheme) {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let mut kit = Theme::from(&*ThemeColor::for_mode(mode));
            kit.mode = mode;
            theme.apply(&mut kit);
            for surface in [
                kit.background,
                kit.surface(Elevation::Popover).fill.opaque(),
            ] {
                let ratio = contrast(kit.foreground, surface);
                assert!(ratio >= 7.0, "{id} {mode:?} foreground is {ratio:.2}:1");
                for (name, color) in [
                    ("accent", kit.accent),
                    ("success", kit.success),
                    ("warning", kit.warning),
                    ("danger", kit.danger),
                ] {
                    let ratio = contrast(color, surface);
                    assert!(ratio >= 3.0, "{id} {mode:?} {name} is {ratio:.2}:1");
                }
            }
            let accent = kit.paint(crate::Tone::Primary, crate::Emphasis::Solid);
            let ink = kit.on_accent.unwrap_or(accent.text);
            let ratio = contrast(ink, kit.accent);
            assert!(
                ratio >= 3.0,
                "{id} {mode:?} text on the accent is {ratio:.2}:1"
            );
        }
    }

    fn sets_every_root(id: &str, light: &ThemeColors, dark: &ThemeColors) {
        for colors in [light, dark] {
            for root in [
                colors.background,
                colors.foreground,
                colors.accent,
                colors.success,
                colors.warning,
                colors.danger,
            ] {
                assert!(root.is_some(), "{id} leaves a root to the fallback palette");
            }
        }
    }

    #[test]
    fn builtin_text_and_status_colors_read_on_their_surfaces() {
        for (id, theme) in builtin_themes() {
            assert_reads(id, theme);
        }
    }

    #[test]
    fn themes_round_trip_through_json() {
        for (id, theme) in builtin_themes() {
            let once = parse_theme(&theme.to_json()).unwrap();
            let twice = parse_theme(&once.to_json()).unwrap();
            assert_eq!(once, twice, "{id} drifts through JSON");
            assert_eq!(once.roles, theme.roles, "{id} loses its roles");
            assert_eq!(
                Materials {
                    glass: None,
                    ..once.materials.clone()
                },
                Materials {
                    glass: None,
                    ..theme.materials.clone()
                },
            );
        }
    }

    #[test]
    fn retired_roles_load_and_drop() {
        let theme = parse_theme(r#"{"roles":{"agent.status":{"fill":"tint"},"code":{}}}"#).unwrap();
        assert_eq!(theme.roles.keys().collect::<Vec<_>>(), ["code"]);
        for id in crate::roles::RETIRED {
            assert_eq!(crate::roles::find(id), None, "{id}");
        }
    }

    #[test]
    fn zz_names_the_accents_and_limits_the_kit_draws() {
        use zz_client::appearance as config;
        assert_eq!(ACCENTS, config::ACCENTS);
        assert_eq!(DEFAULT_THEME, config::DEFAULT_THEME);
        assert_eq!(DEFAULT_TINT_HUE, config::DEFAULT_TINT_HUE);
        assert_eq!(MAX_CORNER_RADIUS, config::MAX_CORNER_RADIUS);
        for id in BUILTIN_THEMES {
            assert!(penna::interface_theme::is_builtin_theme(id), "{id}");
        }
    }

    #[test]
    fn a_theme_naming_zz_roles_hints_at_their_full_ids() {
        let error = parse_theme(r#"{"roles":{"composer":{}}}"#).unwrap_err();
        assert!(error.contains("did you mean `agent.composer`"), "{error}");
        let theme = parse_theme(r#"{"roles":{"pane.chip":{"shape":"pill"}}}"#).unwrap();
        assert_eq!(theme.roles.keys().collect::<Vec<_>>(), ["pane.chip"]);
    }
}
