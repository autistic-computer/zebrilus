use crate::ThemeMode;

pub use penna::foundation::interface_theme::{
    ACCENTS, Appearance, AppearanceChange, Controls, Glass, InterfaceTheme, MAX_CORNER_RADIUS,
    ThemeOverrides, accent_color, platform_theme,
};
pub use penna::foundation::motion::{
    DEFAULT_ANIMATION_SPEED, MAX_ANIMATION_SPEED, MIN_ANIMATION_SPEED,
};
pub use zz_client::appearance::{
    AccentSetting, ControlShapeSetting, DensitySetting, ThemeModeSetting,
};

pub fn parse_theme(source: &str) -> Result<InterfaceTheme, String> {
    penna::foundation::interface_theme::parse_theme(source)
}

pub fn builtin_themes() -> &'static [(&'static str, InterfaceTheme)] {
    penna::foundation::interface_theme::builtin_themes()
}

pub fn builtin_theme(id: &str) -> Option<&'static InterfaceTheme> {
    penna::foundation::interface_theme::builtin_theme(id)
}

pub fn default_theme() -> &'static InterfaceTheme {
    penna::foundation::interface_theme::default_theme()
}

pub const fn pinned_theme_mode(setting: ThemeModeSetting) -> Option<ThemeMode> {
    match setting.pinned() {
        Some(true) => Some(ThemeMode::Dark),
        Some(false) => Some(ThemeMode::Light),
        None => None,
    }
}

pub const fn theme_mode_setting(mode: Option<ThemeMode>) -> ThemeModeSetting {
    match mode {
        Some(ThemeMode::Dark) => ThemeModeSetting::Dark,
        Some(ThemeMode::Light) => ThemeModeSetting::Light,
        None => ThemeModeSetting::System,
    }
}

pub const fn kit_density(setting: DensitySetting) -> penna::Density {
    match setting {
        DensitySetting::Compact => penna::Density::Compact,
        DensitySetting::Regular => penna::Density::Regular,
        DensitySetting::Comfortable => penna::Density::Comfortable,
    }
}

pub const fn kit_controls(setting: ControlShapeSetting) -> Controls {
    match setting {
        ControlShapeSetting::Rounded => Controls::Rounded,
        ControlShapeSetting::Pill => Controls::Pill,
    }
}

pub const fn control_shape_setting(controls: Controls) -> ControlShapeSetting {
    match controls {
        Controls::Rounded => ControlShapeSetting::Rounded,
        Controls::Pill => ControlShapeSetting::Pill,
    }
}

pub fn parse_glass(value: &str) -> Option<Glass> {
    serde_json::from_str(value)
        .or_else(|_| serde_json::from_value(serde_json::Value::String(value.to_owned())))
        .ok()
}

pub fn spelled_glass(glass: &Glass) -> String {
    match serde_json::to_value(glass) {
        Ok(serde_json::Value::String(name)) => name,
        Ok(material) => material.to_string(),
        Err(_) => String::new(),
    }
}

pub const fn density_setting(density: penna::Density) -> DensitySetting {
    match density {
        penna::Density::Compact => DensitySetting::Compact,
        penna::Density::Regular => DensitySetting::Regular,
        penna::Density::Comfortable => DensitySetting::Comfortable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Colorize as _, Elevation};
    use ala::Hsla;
    use penna::{Theme, ThemeColor};

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

    #[allow(
        clippy::disallowed_methods,
        reason = "the test measures each built-in theme's composer buttons against the composer"
    )]
    fn rest_layer(kit: &Theme, surface: impl Into<crate::Surface>) -> crate::Layer {
        kit.layer(surface, crate::PaintState::Rest)
    }

    #[test]
    fn the_composers_buttons_are_filled_in_every_builtin() {
        use crate::Surface;
        use penna::{components::button::ButtonLook, components::input::FieldLook};
        for (id, theme) in builtin_themes() {
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                let mut kit = Theme::from(&*ThemeColor::for_mode(mode));
                kit.mode = mode;
                theme.apply(&mut kit);
                let pill = rest_layer(&kit, FieldLook::Composer).ground;
                for (button, prominent, least) in [("send and stop", true, 1.5), ("+", false, 1.05)]
                {
                    let layer = rest_layer(
                        &kit,
                        Surface::from(ButtonLook::Send)
                            .prominent(prominent)
                            .on(pill),
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

    #[test]
    fn builtin_text_and_status_colors_read_on_their_surfaces() {
        for (id, theme) in builtin_themes() {
            assert_reads(id, theme);
        }
    }

    #[test]
    fn zz_names_the_accents_and_limits_the_kit_draws() {
        use penna::{foundation::interface_theme as kit, foundation::motion};
        use zz_client::appearance as config;
        assert_eq!(kit::ACCENTS, config::ACCENTS);
        assert_eq!(kit::MAX_CORNER_RADIUS, config::MAX_CORNER_RADIUS);
        let glasses = [kit::Glass::None, kit::Glass::Liquid];
        assert_eq!(
            glasses.clone().map(|glass| spelled_glass(&glass)),
            config::GLASSES
        );
        for glass in glasses {
            assert_eq!(parse_glass(&spelled_glass(&glass)), Some(glass));
        }
        assert_eq!(
            penna::blocks::appearance::CONTROL_SHAPES
                .map(|(controls, _)| control_shape_setting(controls).as_str()),
            ControlShapeSetting::ALL.map(ControlShapeSetting::as_str)
        );
        assert_eq!(
            (
                motion::DEFAULT_ANIMATION_SPEED,
                motion::MIN_ANIMATION_SPEED,
                motion::MAX_ANIMATION_SPEED
            ),
            (
                config::DEFAULT_ANIMATION_SPEED,
                config::MIN_ANIMATION_SPEED,
                config::MAX_ANIMATION_SPEED
            )
        );
        assert_eq!(
            ThemeModeSetting::ALL.map(|setting| (pinned_theme_mode(setting), setting.title())),
            penna::blocks::appearance::THEME_MODES
        );
        for setting in ThemeModeSetting::ALL {
            assert_eq!(theme_mode_setting(pinned_theme_mode(setting)), setting);
        }
    }
}
