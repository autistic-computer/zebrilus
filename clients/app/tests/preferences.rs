use zz_app::preferences::Preferences;
use zz_ui::interface_theme::{ThemeModeSetting, ThemeOverrides};

#[test]
fn preferences_reject_invalid_themes_modes_and_numeric_values() {
    let preferences = Preferences {
        mode: Some("unknown".into()),
        theme: Some("unknown".into()),
        zoom: f32::NAN,
        contrast: f32::INFINITY,
        overrides: ThemeOverrides {
            style: Some("unknown".into()),
            radius: Some(-20.0),
            accent: Some("unknown".into()),
            tint: Some(4.0),
            tint_hue: Some(f32::NAN),
        },
        terminal_font_family: Some(" ".into()),
        terminal_font_scale: f32::NAN,
        ..Preferences::default()
    }
    .sanitized();
    assert_eq!(preferences.theme_mode(), ThemeModeSetting::System);
    assert_eq!(preferences.theme, None);
    assert_eq!(
        (
            preferences.zoom,
            preferences.contrast,
            preferences.overrides
        ),
        (
            1.0,
            1.0,
            ThemeOverrides {
                radius: Some(0.0),
                tint: Some(1.0),
                ..ThemeOverrides::default()
            }
        )
    );
    assert_eq!(preferences.terminal_font_family, None);
    assert_eq!(preferences.terminal_font_scale, 1.0);
}

#[test]
fn pane_preferences_bound_values_and_preserve_fractional_frames() {
    let preferences = Preferences {
        gaps: true,
        pane_background_opacity: -1.0,
        pane_inactive_opacity: 3.0,
        pane_glow_strength: f32::NAN,
        pane_margin: 33.0,
        ..Preferences::default()
    }
    .sanitized();
    assert!(preferences.gaps);
    assert_eq!(preferences.pane_background_opacity, 0.0);
    assert_eq!(preferences.pane_inactive_opacity, 1.0);
    assert_eq!(preferences.pane_glow_strength, 1.0);
    assert_eq!(preferences.pane_margin, 32.0);
    let fractional = Preferences {
        pane_margin: 7.5,
        ..preferences
    }
    .sanitized();
    assert_eq!(fractional.pane_margin, 7.5);
    let restored = Preferences::decode(&serde_json::to_string(&fractional).unwrap()).unwrap();
    assert_eq!(restored, fractional);
}
