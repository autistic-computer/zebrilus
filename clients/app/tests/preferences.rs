use zz_app::preferences::Preferences;
use zz_ui::interface_theme::{Appearance, ThemeModeSetting, ThemeOverrides};

#[test]
fn preferences_reject_invalid_themes_modes_and_numeric_values() {
    let preferences = Preferences {
        appearance: Appearance {
            mode: Some("unknown".into()),
            theme: Some("unknown".into()),
            zoom: f32::NAN,
            contrast: f32::INFINITY,
            overrides: ThemeOverrides {
                radius: Some(-20.0),
                accent: Some("unknown".into()),
                tint: Some(4.0),
                tint_hue: Some(f32::NAN),
            },
            ..Appearance::default()
        },
        terminal_font_family: Some(" ".into()),
        terminal_font_scale: f32::NAN,
        ..Preferences::default()
    }
    .sanitized();
    assert_eq!(preferences.theme_mode(), ThemeModeSetting::System);
    assert_eq!(preferences.appearance.theme, None);
    assert_eq!(
        (
            preferences.appearance.zoom,
            preferences.appearance.contrast,
            preferences.appearance.overrides
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
        pip_background_opacity: -1.0,
        pane_inactive_opacity: 3.0,
        pane_glow_strength: f32::NAN,
        pane_margin: 33.0,
        ..Preferences::default()
    }
    .sanitized();
    assert!(preferences.gaps);
    assert_eq!(preferences.pip_background_opacity, 0.0);
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

#[test]
fn a_preferences_file_written_before_appearance_moved_into_the_kit_loads_the_same() {
    let source = include_str!("fixtures/preferences-0.22.json");
    let preferences = Preferences::decode(source).unwrap();
    assert!(!preferences.sidebar);
    assert!(preferences.flat_sidebar);
    assert!(!preferences.program_colors);
    assert_eq!(preferences.sidebar_width, 312.0);
    assert!(!preferences.status_badges);
    assert!(preferences.gaps);
    assert_eq!(preferences.terminal_font_family.as_deref(), Some("Lilex"));
    assert_eq!(preferences.terminal_font_scale, 1.25);
    assert_eq!(
        (
            preferences.pip_background_opacity,
            preferences.pane_inactive_opacity,
            preferences.pane_glow_strength,
            preferences.pane_margin,
        ),
        (0.6, 0.5, 1.5, 7.5)
    );
    assert!(!preferences.palette_grouped);
    assert!(!preferences.palette_show_keys);
    assert!(preferences.palette_vim_mode);
    assert!(preferences.extend_bottom_safe_area);
    assert!(preferences.keep_screen_awake);
    assert!(!preferences.system_text_size);
    assert_eq!(
        preferences.appearance,
        Appearance {
            mode: Some("light".into()),
            theme: Some("theme-1".into()),
            overrides: ThemeOverrides {
                radius: Some(7.0),
                accent: Some("#ff8800".into()),
                tint: Some(0.25),
                tint_hue: Some(145.0),
            },
            contrast: 1.4,
            ui_font_family: "Inter Variable".into(),
            zoom: 1.35,
            animations: false,
            animation_speed: 1.5,
        }
    );
    assert_eq!(preferences.theme_mode(), ThemeModeSetting::Light);
    assert_eq!(preferences.themes.len(), 1);
    assert_eq!(preferences.theme_id(), "theme-1");
    assert_eq!(preferences.interface_theme().name, "Neon");
    assert_eq!(preferences.shown_ui_font(), "Inter Variable");
    assert_eq!(preferences.widget_corner_radius(), 7.0);

    let written = serde_json::to_string(&preferences).unwrap();
    let parsed = |json: &str| serde_json::from_str::<serde_json::Value>(json).unwrap();
    assert_eq!(parsed(&written), parsed(source));
}
