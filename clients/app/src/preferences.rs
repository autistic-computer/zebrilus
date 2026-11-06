use penna::foundation::interface_theme::Tint;
use serde::{Deserialize, Serialize, de::value::MapDeserializer};
use zz_ui::interface_theme::{
    self, Appearance, AppearanceChange, InterfaceTheme, ThemeModeSetting, ThemeOverrides,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub sidebar: bool,
    pub flat_sidebar: bool,
    pub program_colors: bool,
    pub sidebar_width: f32,
    pub gaps: bool,
    pub dark: bool,
    pub terminal_font_family: Option<String>,
    pub terminal_font_scale: f32,
    #[serde(flatten)]
    pub appearance: Appearance,
    pub pane_inactive_opacity: f32,
    pub pane_glow_strength: f32,
    pub pane_margin: f32,
    pub pane_corner_radius: Option<f32>,
    pub palette_grouped: bool,
    pub palette_show_keys: bool,
    pub palette_vim_mode: bool,
    pub extend_bottom_safe_area: bool,
    pub keep_screen_awake: bool,
    pub system_text_size: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            sidebar: true,
            flat_sidebar: false,
            program_colors: true,
            sidebar_width: penna::blocks::sidebar::SIDEBAR_DEFAULT_WIDTH,
            gaps: false,
            dark: true,
            terminal_font_family: None,
            terminal_font_scale: 1.0,
            appearance: Appearance {
                mode: Some("system".into()),
                ..Appearance::default()
            },
            pane_inactive_opacity: 0.7,
            pane_glow_strength: 1.0,
            pane_margin: 6.0,
            pane_corner_radius: None,
            palette_grouped: true,
            palette_show_keys: true,
            palette_vim_mode: false,
            extend_bottom_safe_area: false,
            keep_screen_awake: false,
            system_text_size: true,
        }
    }
}

impl Preferences {
    #[must_use]
    pub fn margin(&self) -> ala::Pixels {
        ala::px(if self.gaps { self.pane_margin } else { 0.0 })
    }

    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.sidebar_width = bounded(
            self.sidebar_width,
            160.0,
            640.0,
            penna::blocks::sidebar::SIDEBAR_DEFAULT_WIDTH,
        );
        let overrides = &mut self.appearance.overrides;
        for (value, max) in [
            (&mut overrides.radius, interface_theme::MAX_CORNER_RADIUS),
            (&mut overrides.tint, 1.0),
            (&mut overrides.tint_hue, 360.0),
        ] {
            *value = value
                .filter(|value| value.is_finite())
                .map(|value| value.clamp(0.0, max));
        }
        overrides.accent = overrides
            .accent
            .take()
            .filter(|accent| Tint::named(accent).is_some());
        let appearance = &mut self.appearance;
        appearance.zoom = bounded(appearance.zoom, 0.5, 3.0, 1.0);
        appearance.contrast = bounded(appearance.contrast, 1.0, 2.0, 1.0);
        appearance.ui_font_family = appearance.ui_font_family.trim().into();
        self.terminal_font_scale = bounded(self.terminal_font_scale, 0.5, 3.0, 1.0);
        for (value, min, max, fallback) in [
            (&mut self.pane_inactive_opacity, 0., 1., 0.7),
            (&mut self.pane_glow_strength, 0., 2., 1.),
            (&mut self.pane_margin, 0., 32., 6.),
        ] {
            *value = bounded(*value, min, max, fallback);
        }
        self.pane_corner_radius = self
            .pane_corner_radius
            .filter(|radius| radius.is_finite())
            .map(|radius| radius.clamp(0.0, interface_theme::MAX_CORNER_RADIUS));
        self.appearance.mode = Some(self.theme_mode().as_str().into());
        self.terminal_font_family = self
            .terminal_font_family
            .map(|family| family.trim().to_owned())
            .filter(|family| !family.is_empty());
        self
    }

    #[must_use]
    pub fn palette_settings(&self) -> zz_ui::command::PaletteSettings {
        zz_ui::command::PaletteSettings {
            grouped: self.palette_grouped,
            show_keys: self.palette_show_keys,
            vim: self.palette_vim_mode,
        }
    }

    #[must_use]
    pub fn sidebar_width(&self, available_width: f32) -> f32 {
        self.sidebar_width
            .clamp(160.0, (available_width * 0.5).clamp(160.0, 640.0))
    }

    pub fn change_appearance(&mut self, change: AppearanceChange) {
        self.appearance.change(change);
    }

    #[must_use]
    pub fn shown_ui_font(&self) -> &str {
        self.appearance.ui_font()
    }

    #[must_use]
    pub fn shown_theme(&self) -> InterfaceTheme {
        self.appearance.shown_theme()
    }

    #[must_use]
    pub fn widget_corner_radius(&self) -> f32 {
        self.appearance.corner_radius()
    }

    #[must_use]
    pub fn theme_mode(&self) -> ThemeModeSetting {
        self.appearance.mode.as_deref().map_or(
            if self.dark {
                ThemeModeSetting::Dark
            } else {
                ThemeModeSetting::Light
            },
            |mode| ThemeModeSetting::parse(mode).unwrap_or(ThemeModeSetting::System),
        )
    }

    pub fn decode(value: &str) -> serde_json::Result<Self> {
        let mut source: serde_json::Value = serde_json::from_str(value)?;
        if let Some(fields) = source.as_object_mut() {
            fields.retain(|key, value| {
                let field = std::iter::once((key.as_str(), &*value));
                ThemeOverrides::deserialize(MapDeserializer::<_, serde_json::Error>::new(field))
                    .is_ok()
            });
        }
        let mut preferences: Self = serde_json::from_value(source.clone())?;
        if source.get("mode").is_none() && source.get("dark").is_none() {
            preferences.appearance.mode = Some("system".into());
        }
        Ok(preferences.sanitized())
    }

    #[cfg(any(target_os = "ios", target_os = "android", test))]
    pub fn load_path(path: &std::path::Path) -> std::io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(value) => Self::decode(&value)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    #[cfg(any(target_os = "ios", target_os = "android", test))]
    pub fn save_path(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let value = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, value)?;
        std::fs::rename(temporary, path)
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn path() -> std::path::PathBuf {
        support_directory().join("preferences.json")
    }

    #[must_use]
    pub fn load() -> Self {
        #[cfg(target_family = "wasm")]
        if let Some(value) = web_sys::window()
            .and_then(|window| window.local_storage().ok().flatten())
            .and_then(|storage| storage.get_item(Self::STORAGE_KEY).ok().flatten())
            .and_then(|value| Self::decode(&value).ok())
        {
            return value;
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            Self::load_path(&Self::path()).unwrap_or_default()
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        Self::default().sanitized()
    }

    pub fn save(&self) {
        #[cfg(target_family = "wasm")]
        if let Some(storage) =
            web_sys::window().and_then(|window| window.local_storage().ok().flatten())
            && let Ok(value) = serde_json::to_string(self)
        {
            let _ = storage.set_item(Self::STORAGE_KEY, &value);
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        if let Err(error) = self.save_path(&Self::path()) {
            log::warn!("Could not save preferences: {error}");
        }
    }

    #[cfg(target_family = "wasm")]
    pub const STORAGE_KEY: &str = if zz_protocol::app_identity::DEVELOPMENT {
        "zz-dev-web-preferences"
    } else {
        "zz-web-preferences"
    };
}

fn bounded(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub(crate) fn support_directory() -> std::path::PathBuf {
    #[cfg(target_os = "ios")]
    let base =
        std::path::PathBuf::from(std::env::var_os("HOME").expect("iOS application home directory"))
            .join("Library/Application Support");
    #[cfg(target_os = "android")]
    let base = ala_platform::android::files_dir().expect("Android files directory");
    base.join("ala")
}

#[cfg(test)]
mod tests {
    use super::Preferences;
    use penna::foundation::interface_theme::{Glass, Neutral, Tint};
    use zz_ui::interface_theme::{
        self, Appearance, AppearanceChange, Controls, InterfaceTheme, ThemeModeSetting,
        ThemeOverrides,
    };

    #[test]
    fn preferences_persist_and_missing_fields_keep_defaults() {
        let directory = std::env::temp_dir().join(format!(
            "zz-ios-preferences-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = directory.join("preferences.json");
        assert_eq!(
            Preferences::load_path(&path).unwrap(),
            Preferences::default()
        );
        let expected = Preferences {
            flat_sidebar: true,
            terminal_font_family: Some("Lilex".into()),
            terminal_font_scale: 1.25,
            pane_margin: 7.5,
            appearance: Appearance {
                mode: Some("light".into()),
                motion: penna::foundation::motion::Motion::Off,
                ui_font_family: "Inter Variable".into(),
                overrides: ThemeOverrides {
                    accent: Some("green".into()),
                    tint: Some(0.25),
                    tint_hue: Some(200.0),
                    ..ThemeOverrides::default()
                },
                ..Appearance::default()
            },
            ..Preferences::default()
        };
        expected.save_path(&path).unwrap();
        assert_eq!(Preferences::load_path(&path).unwrap(), expected);
        std::fs::write(&path, r#"{"mode":"dark"}"#).unwrap();
        let mut dark = Preferences::default();
        dark.appearance.mode = Some("dark".into());
        assert_eq!(Preferences::load_path(&path).unwrap(), dark);
        std::fs::write(&path, "broken json").unwrap();
        assert_eq!(
            Preferences::load_path(&path).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn fresh_preferences_follow_system_and_legacy_dark_choice_survives() {
        assert_eq!(
            Preferences::decode("{}").unwrap().theme_mode(),
            ThemeModeSetting::System
        );
        assert_eq!(
            Preferences::decode(r#"{"dark":true}"#)
                .unwrap()
                .theme_mode(),
            ThemeModeSetting::Dark
        );
        assert_eq!(
            Preferences::decode(r#"{"dark":false}"#)
                .unwrap()
                .theme_mode(),
            ThemeModeSetting::Light
        );
        assert_eq!(
            Preferences::decode(r#"{"mode":"system","dark":true}"#)
                .unwrap()
                .theme_mode(),
            ThemeModeSetting::System
        );
    }

    #[test]
    fn older_preferences_gain_status_and_appearance_defaults() {
        let defaults = serde_json::from_str::<Preferences>(r#"{"sidebar":false}"#).unwrap();
        assert!(!defaults.sidebar);
        assert_eq!(
            defaults.appearance.motion,
            penna::foundation::motion::Motion::Default
        );
        assert_eq!(defaults.appearance.overrides, ThemeOverrides::default());
        assert_eq!(
            defaults.sidebar_width,
            penna::blocks::sidebar::SIDEBAR_DEFAULT_WIDTH
        );
        let mut changed = Preferences {
            sidebar_width: 320.0,
            pane_corner_radius: Some(18.0),
            ..defaults
        };
        changed.appearance.motion = penna::foundation::motion::Motion::Fast;
        let saved = serde_json::to_string(&changed).unwrap();
        let restored = serde_json::from_str::<Preferences>(&saved)
            .unwrap()
            .sanitized();
        assert_eq!(restored.pane_corner_radius, Some(18.0));
        assert_eq!(
            restored.appearance.motion,
            penna::foundation::motion::Motion::Fast
        );
        let platform = interface_theme::platform_theme();
        assert_eq!(restored.shown_theme(), *platform);
        assert_eq!(restored.widget_corner_radius(), platform.radius);
        assert_eq!(restored.sidebar_width, 320.0);
        let retired = Preferences::decode(
            r##"{"sidebar":false,"status_agents":false,"palette_host_prefix":"#"}"##,
        )
        .unwrap();
        assert!(!retired.sidebar);
    }

    #[test]
    fn sidebar_and_theme_overrides_stay_within_supported_limits() {
        for (width, available, expected) in [
            (0.0, 1000.0, 160.0),
            (300.0, 1000.0, 300.0),
            (800.0, 1000.0, 500.0),
            (800.0, 2000.0, 640.0),
            (300.0, 200.0, 160.0),
        ] {
            let preferences = Preferences {
                sidebar_width: width,
                ..Preferences::default()
            }
            .sanitized();
            assert_eq!(preferences.sidebar_width(available), expected);
        }
        let mut invalid = Preferences {
            sidebar_width: f32::NAN,
            ..Preferences::default()
        };
        invalid.appearance.overrides = ThemeOverrides {
            radius: Some(f32::INFINITY),
            accent: Some("chartreuse".into()),
            tint: Some(f32::NAN),
            tint_hue: Some(f32::NAN),
            ..ThemeOverrides::default()
        };
        let invalid = invalid.sanitized();
        assert_eq!(invalid.sidebar_width, Preferences::default().sidebar_width);
        assert_eq!(invalid.appearance.overrides, ThemeOverrides::default());
        assert_eq!(
            invalid.widget_corner_radius(),
            interface_theme::platform_theme().radius
        );
        for (value, expected) in [
            ((-1.0, -0.1, -10.0), (0.0, 0.0, 0.0)),
            ((30.0, 2.0, 400.0), (25.0, 1.0, 360.0)),
            ((12.5, 0.5, 145.0), (12.5, 0.5, 145.0)),
        ] {
            let mut preferences = Preferences::default();
            preferences.appearance.overrides = ThemeOverrides {
                radius: Some(value.0),
                tint: Some(value.1),
                tint_hue: Some(value.2),
                ..ThemeOverrides::default()
            };
            let overrides = preferences.sanitized().appearance.overrides;
            assert_eq!(
                (overrides.radius, overrides.tint, overrides.tint_hue),
                (Some(expected.0), Some(expected.1), Some(expected.2))
            );
        }
        for accent in ["ink", "green", "#ff8800"] {
            let valid = ThemeOverrides {
                accent: Some(accent.into()),
                ..ThemeOverrides::default()
            };
            let mut preferences = Preferences::default();
            preferences.appearance.overrides = valid.clone();
            assert_eq!(preferences.sanitized().appearance.overrides, valid);
        }
    }

    #[test]
    fn contrast_defaults_clamps_and_round_trips_with_preferences() {
        let defaults = serde_json::from_str::<Preferences>("{}").unwrap();
        assert_eq!(defaults.appearance.contrast, 1.0);
        for (value, expected) in [(0.1, 1.0), (3.0, 2.0), (1.5, 1.5), (f32::NAN, 1.0)] {
            let mut preferences = Preferences::default();
            preferences.appearance.contrast = value;
            let preferences = preferences.sanitized();
            assert_eq!(preferences.appearance.contrast, expected);
            let saved = serde_json::to_string(&preferences).unwrap();
            assert_eq!(
                serde_json::from_str::<Preferences>(&saved)
                    .unwrap()
                    .appearance
                    .contrast,
                expected
            );
        }
    }

    #[test]
    fn full_widget_radius_survives_saved_preferences() {
        let mut preferences = Preferences::default();
        preferences.appearance.overrides.radius = Some(25.0);
        let saved = serde_json::to_string(&preferences).unwrap();
        let loaded = serde_json::from_str::<Preferences>(&saved)
            .unwrap()
            .sanitized();
        assert_eq!(loaded.widget_corner_radius(), 25.0);
    }

    #[test]
    fn a_theme_file_sets_every_knob_and_reset_returns_to_the_platform() {
        let older = Preferences::decode(r#"{"radius":12,"tint":0.3,"style":"flat"}"#).unwrap();
        assert_eq!(
            older.appearance.overrides,
            ThemeOverrides {
                radius: Some(12.0),
                tint: Some(0.3),
                ..ThemeOverrides::default()
            }
        );

        let mut tuned = older;
        let neon = interface_theme::parse_theme(
            r#"{"name":"Neon","radius":3,"tint":"pink","face":"Lilex","control-radius":"full","glass":"none","neutral":{"hue":300,"amount":0.5}}"#,
        )
        .unwrap();
        tuned.change_appearance(AppearanceChange::Import(Box::new(neon)));
        let platform = interface_theme::platform_theme();
        assert_eq!(
            tuned.shown_theme(),
            InterfaceTheme {
                tint: Tint::named("pink").unwrap(),
                neutral: Neutral {
                    hue: 300.0,
                    amount: 0.5
                },
                radius: 3.0,
                control_radius: Some(penna::foundation::interface_theme::ControlRadius::Full),
                glass: Some(Glass::None),
                ..platform.clone()
            }
        );
        assert_eq!(tuned.shown_ui_font(), "Lilex");
        assert_eq!(tuned.appearance.controls(), Controls::Pill);
        let restored = Preferences::decode(&serde_json::to_string(&tuned).unwrap()).unwrap();
        assert_eq!(restored, tuned);

        tuned.change_appearance(AppearanceChange::ResetLook);
        assert_eq!(tuned.appearance.overrides, ThemeOverrides::default());
        assert_eq!(tuned.shown_theme(), *platform);
        assert_eq!(
            tuned.shown_ui_font(),
            platform.face.as_deref().unwrap_or("")
        );
    }

    #[test]
    fn picked_themes_saved_themes_and_frosted_glass_from_older_preferences_are_ignored() {
        let older = Preferences::decode(
            r#"{"theme":"theme-1","themes":[["theme-1",{"name":"Kept","radius":3}]],"radius":12,"shadows":"none","selection":"wash","field":"outline","outline":1,"shadow-strength":0.3,"row-inset":2,"primary":"accent","glass":"frosted","pane_radius":9,"pane_border_width":2,"colors":[null,"ff0000"],"style":3}"#,
        )
        .unwrap();
        assert_eq!(
            older.appearance.overrides,
            ThemeOverrides {
                radius: Some(12.0),
                ..ThemeOverrides::default()
            }
        );
        let written = serde_json::to_value(&older).unwrap();
        assert!(written.get("theme").is_none() && written.get("themes").is_none());
    }

    #[test]
    fn older_preferences_gain_pane_controls_and_values_remain_bounded() {
        let preferences = serde_json::from_str::<Preferences>(
            r#"{"gaps":true,"radius":8,"pane_radius":20,"pane_border_width":3}"#,
        )
        .unwrap()
        .sanitized();
        assert!(preferences.gaps);
        assert_eq!(preferences.appearance.overrides.radius, Some(8.0));
        assert_eq!(preferences.pane_margin, 6.0);
        assert_eq!(preferences.pane_corner_radius, None);
        assert_eq!(preferences.pane_glow_strength, 1.0);
        assert_eq!(
            serde_json::from_str::<Preferences>(
                r#"{"gaps":true,"mode":"system","pane_glass":true}"#
            )
            .unwrap(),
            Preferences {
                gaps: true,
                ..Preferences::default()
            }
        );
        let preferences = Preferences {
            pane_margin: -1.0,
            pane_inactive_opacity: 2.0,
            pane_glow_strength: 3.0,
            ..preferences
        }
        .sanitized();
        assert_eq!(preferences.pane_margin, 0.0);
        assert_eq!(preferences.pane_inactive_opacity, 1.0);
        assert_eq!(preferences.pane_glow_strength, 2.0);
        for (value, expected) in [(Some(40.0), Some(25.0)), (Some(f32::NAN), None)] {
            assert_eq!(
                Preferences {
                    pane_corner_radius: value,
                    ..Preferences::default()
                }
                .sanitized()
                .pane_corner_radius,
                expected,
            );
        }
    }
}
