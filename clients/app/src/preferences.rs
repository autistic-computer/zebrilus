use ala::SharedString;
use penna::ThemeMode;
use serde::{Deserialize, Deserializer, Serialize, de::value::MapDeserializer};
use zz_ui::interface_theme::{
    self, Appearance, AppearanceChange, InterfaceTheme, SavedThemes, ThemeModeSetting,
    ThemeOverrides,
};

pub const MAX_SAVED_THEMES: usize = 32;
const MAX_THEME_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub sidebar: bool,
    pub flat_sidebar: bool,
    pub program_colors: bool,
    pub sidebar_width: f32,
    pub status_badges: bool,
    pub gaps: bool,
    pub dark: bool,
    #[serde(deserialize_with = "saved_themes")]
    pub themes: Vec<(SharedString, InterfaceTheme)>,
    pub terminal_font_family: Option<String>,
    pub terminal_font_scale: f32,
    #[serde(flatten)]
    pub appearance: Appearance,
    pub pip_background_opacity: f32,
    pub pane_inactive_opacity: f32,
    pub pane_glow_strength: f32,
    pub pane_margin: f32,
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
            sidebar_width: penna::sidebar::SIDEBAR_DEFAULT_WIDTH,
            status_badges: true,
            gaps: false,
            dark: true,
            themes: Vec::new(),
            terminal_font_family: None,
            terminal_font_scale: 1.0,
            appearance: Appearance {
                mode: Some("system".into()),
                ..Appearance::default()
            },
            pip_background_opacity: 0.8,
            pane_inactive_opacity: 0.7,
            pane_glow_strength: 1.0,
            pane_margin: 6.0,
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
            penna::sidebar::SIDEBAR_DEFAULT_WIDTH,
        );
        let mut ids = std::collections::HashSet::new();
        self.themes.retain(|(id, _)| {
            !id.is_empty() && !interface_theme::is_builtin_theme(id) && ids.insert(id.clone())
        });
        self.themes.truncate(MAX_SAVED_THEMES);
        let theme = self.appearance.theme.take();
        self.appearance.theme = theme.filter(|id| self.knows_theme(id));
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
            .filter(|accent| interface_theme::accent_color(accent, ThemeMode::Dark).is_some());
        let appearance = &mut self.appearance;
        appearance.zoom = bounded(appearance.zoom, 0.5, 3.0, 1.0);
        appearance.contrast = bounded(appearance.contrast, 1.0, 2.0, 1.0);
        appearance.animation_speed = bounded(
            appearance.animation_speed,
            interface_theme::MIN_ANIMATION_SPEED,
            interface_theme::MAX_ANIMATION_SPEED,
            interface_theme::DEFAULT_ANIMATION_SPEED,
        );
        appearance.ui_font_family = appearance.ui_font_family.trim().into();
        self.terminal_font_scale = bounded(self.terminal_font_scale, 0.5, 3.0, 1.0);
        for (value, min, max, fallback) in [
            (&mut self.pip_background_opacity, 0., 1., 0.8),
            (&mut self.pane_inactive_opacity, 0., 1., 0.7),
            (&mut self.pane_glow_strength, 0., 2., 1.),
            (&mut self.pane_margin, 0., 32., 6.),
        ] {
            *value = bounded(*value, min, max, fallback);
        }
        self.appearance.mode = Some(self.theme_mode().as_str().into());
        self.terminal_font_family = self
            .terminal_font_family
            .map(|family| family.trim().to_owned())
            .filter(|family| !family.is_empty());
        self
    }

    #[must_use]
    pub fn status_bar_settings(&self) -> zz_client::StatusBarSettings {
        zz_client::StatusBarSettings {
            badges: self.status_badges,
            show_host: false,
            show_update: false,
        }
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

    #[must_use]
    pub fn saved(&self) -> &SavedThemes {
        zz_ui::register();
        &self.themes
    }

    #[must_use]
    pub fn interface_theme(&self) -> &InterfaceTheme {
        self.appearance.interface_theme(self.saved())
    }

    #[must_use]
    pub fn theme_id(&self) -> &str {
        self.appearance.theme_id(self.saved())
    }

    #[must_use]
    pub fn saved_theme(&self, id: &str) -> Option<&InterfaceTheme> {
        self.themes
            .iter()
            .find(|(saved, _)| saved.as_str() == id)
            .map(|(_, theme)| theme)
    }

    fn knows_theme(&self, id: &str) -> bool {
        interface_theme::is_builtin_theme(id) || self.saved_theme(id).is_some()
    }

    pub fn set_interface_theme(&mut self, id: &str) {
        self.change_appearance(AppearanceChange::Theme(id.to_owned().into()));
    }

    pub fn change_appearance(&mut self, change: AppearanceChange) {
        zz_ui::register();
        self.appearance.change(change, &self.themes);
    }

    pub fn import_theme(&mut self, clipboard: &str) -> Result<String, String> {
        let theme = if clipboard.len() > MAX_THEME_BYTES {
            Err(format!("it is over {} KB", MAX_THEME_BYTES / 1024))
        } else {
            interface_theme::parse_theme(clipboard)
        }
        .map_err(|error| {
            format!(
                "The clipboard holds no theme: {error}. Copy one with Copy JSON in the storybook."
            )
        })?;
        self.save_theme(theme)
    }

    fn save_theme(&mut self, mut theme: InterfaceTheme) -> Result<String, String> {
        if self.themes.len() >= MAX_SAVED_THEMES {
            return Err(format!(
                "This device keeps {MAX_SAVED_THEMES} themes. Delete one to save another."
            ));
        }
        let id = (1..=self.themes.len() + 1)
            .map(|index| format!("theme-{index}"))
            .find(|id| self.saved_theme(id).is_none())
            .expect("one more id than themes holds a free one");
        theme.name_after(&id);
        self.themes.push((id.clone().into(), theme));
        self.set_interface_theme(&id);
        Ok(id)
    }

    pub fn delete_theme(&mut self, id: &str) {
        self.themes.retain(|(saved, _)| saved.as_str() != id);
        if self.appearance.theme.as_deref() == Some(id) {
            self.set_interface_theme(interface_theme::DEFAULT_THEME);
        }
    }

    #[must_use]
    pub fn shown_ui_font(&self) -> &str {
        self.appearance.ui_font(self.saved())
    }

    #[must_use]
    pub fn shown_theme(&self) -> InterfaceTheme {
        self.appearance.shown_theme(self.saved())
    }

    #[must_use]
    pub fn widget_corner_radius(&self) -> f32 {
        self.appearance.corner_radius(self.saved())
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
        if source.get("theme").is_none() {
            preferences.appearance.overrides = ThemeOverrides::default();
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

fn saved_themes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<(SharedString, InterfaceTheme)>, D::Error> {
    Ok(
        Vec::<(String, serde_json::Value)>::deserialize(deserializer)?
            .into_iter()
            .filter_map(|(id, theme)| Some((id.into(), serde_json::from_value(theme).ok()?)))
            .collect(),
    )
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
    use zz_ui::interface_theme::{self, Appearance, ThemeModeSetting, ThemeOverrides};

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
                animations: false,
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
        assert!(defaults.appearance.animations);
        assert_eq!(defaults.appearance.overrides, ThemeOverrides::default());
        assert_eq!(
            defaults.sidebar_width,
            penna::sidebar::SIDEBAR_DEFAULT_WIDTH
        );
        let status = defaults.status_bar_settings();
        assert!(status.badges);
        assert!(!status.show_host && !status.show_update);
        let mut changed = Preferences {
            status_badges: false,
            sidebar_width: 320.0,
            ..defaults
        };
        changed.appearance.animations = false;
        changed.appearance.theme = Some("macos".into());
        let saved = serde_json::to_string(&changed).unwrap();
        let restored = serde_json::from_str::<Preferences>(&saved)
            .unwrap()
            .sanitized();
        assert_eq!(
            restored.status_bar_settings(),
            changed.status_bar_settings()
        );
        assert!(!restored.appearance.animations);
        let macos = &interface_theme::builtin_theme("macos").unwrap().materials;
        assert_eq!(restored.shown_theme().materials.outline, macos.outline);
        assert_eq!(restored.widget_corner_radius(), macos.radius);
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
        };
        let invalid = invalid.sanitized();
        assert_eq!(invalid.sidebar_width, Preferences::default().sidebar_width);
        assert_eq!(invalid.appearance.overrides, ThemeOverrides::default());
        assert_eq!(
            invalid.widget_corner_radius(),
            interface_theme::default_theme().materials.radius
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
        for accent in ["green", "#ff8800"] {
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
    fn the_theme_decides_the_look_until_overrides_are_set() {
        let builtin = interface_theme::default_theme();
        let older = Preferences::decode(r#"{"radius":12,"tint":0.3,"style":"flat"}"#).unwrap();
        assert_eq!(older.theme_id(), "default");
        assert_eq!(older.appearance.overrides, ThemeOverrides::default());
        assert_eq!(older.shown_theme(), *builtin);

        let mut tuned =
            Preferences::decode(r#"{"theme":null,"radius":12,"accent":"green","tint":0.4}"#)
                .unwrap();
        assert_eq!(tuned.widget_corner_radius(), 12.0);
        let saved = serde_json::to_string(&tuned).unwrap();
        assert_eq!(
            Preferences::decode(&saved).unwrap().appearance.overrides,
            tuned.appearance.overrides
        );

        tuned.set_interface_theme("default");
        let picked = Preferences::decode(&serde_json::to_string(&tuned).unwrap()).unwrap();
        assert_eq!(picked.theme_id(), "default");
        assert_eq!(
            picked.appearance.overrides,
            ThemeOverrides {
                accent: Some("green".into()),
                tint: Some(0.4),
                ..ThemeOverrides::default()
            }
        );
        assert_eq!(picked.widget_corner_radius(), builtin.materials.radius);
        assert_eq!(picked.shown_theme().materials, builtin.materials);
        tuned.set_interface_theme("vaporwave");
        assert_eq!(tuned.theme_id(), "default");
    }

    const NEON: &str = r##"{"name":"Neon","materials":{"radius":3,"float":{"strength":0.5},"font":"Lilex"},"dark":{"accent":"#ff00ff"}}"##;

    #[test]
    fn saved_themes_round_trip_and_older_preferences_have_none() {
        let mut preferences = Preferences::default();
        assert_eq!(preferences.import_theme(NEON).unwrap(), "theme-1");
        let restored = Preferences::decode(&serde_json::to_string(&preferences).unwrap()).unwrap();
        assert_eq!(restored, preferences);
        assert_eq!(restored.theme_id(), "theme-1");
        assert_eq!(restored.interface_theme().name, "Neon");

        let mut glass = Preferences::default();
        glass.appearance.theme = Some("macos".into());
        glass.save_theme(glass.shown_theme()).unwrap();
        let restored = Preferences::decode(&serde_json::to_string(&glass).unwrap()).unwrap();
        assert_eq!(restored.theme_id(), "theme-1");
        assert!(restored.interface_theme().materials.glass.is_some());

        let older = Preferences::decode(
            r#"{"theme":"default","radius":12,"shadows":"none","selection":"wash","field":"outline","outline":1,"shadow-strength":0.3,"row-inset":2,"primary":"accent","glass":"frosted","pane_radius":9,"pane_border_width":2,"colors":[null,"ff0000"],"style":3}"#,
        )
        .unwrap();
        assert!(older.themes.is_empty());
        assert_eq!(older.theme_id(), "default");
        assert_eq!(
            older.appearance.overrides,
            ThemeOverrides {
                radius: Some(12.0),
                ..ThemeOverrides::default()
            }
        );

        let unreadable = Preferences::decode(
            r#"{"theme":"theme-2","themes":[["theme-1",{"name":"Kept"}],["theme-2",{"materials":{"glass":"stained"}}],["theme-3",{"materials":{"corner_smoothing":2}}]]}"#,
        )
        .unwrap();
        assert_eq!(unreadable.themes.len(), 1);
        assert_eq!(unreadable.theme_id(), "default");
    }

    #[test]
    fn sanitizing_keeps_a_selection_only_while_it_names_a_theme() {
        let theme = zz_ui::interface_theme::InterfaceTheme::default;
        let mut preferences = Preferences {
            themes: vec![
                ("theme-2".into(), theme()),
                ("default".into(), theme()),
                (String::new().into(), theme()),
                ("theme-2".into(), theme()),
            ],
            ..Preferences::default()
        };
        preferences.appearance.theme = Some("theme-2".into());
        let mut preferences = preferences.sanitized();
        assert_eq!(preferences.appearance.theme.as_deref(), Some("theme-2"));
        assert_eq!(preferences.themes.len(), 1);
        preferences.appearance.theme = Some("theme-9".into());
        let dangling = preferences.sanitized();
        assert_eq!(dangling.appearance.theme, None);
        assert_eq!(dangling.theme_id(), "default");

        let crowded = Preferences {
            themes: (1..=40)
                .map(|index| (format!("theme-{index}").into(), theme()))
                .collect(),
            ..Preferences::default()
        }
        .sanitized();
        assert_eq!(crowded.themes.len(), super::MAX_SAVED_THEMES);
    }

    #[test]
    fn import_names_built_in_and_nameless_themes_after_their_id() {
        let mut preferences = Preferences::default();
        preferences.import_theme(r#"{"name":"Default"}"#).unwrap();
        preferences.import_theme("{}").unwrap();
        preferences.import_theme(NEON).unwrap();
        let names = preferences
            .themes
            .iter()
            .map(|(id, theme)| (id.as_str(), theme.name.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                ("theme-1", "theme-1"),
                ("theme-2", "theme-2"),
                ("theme-3", "Neon")
            ]
        );
        for source in ["[]", "not json", &" ".repeat(super::MAX_THEME_BYTES + 1)] {
            let error = preferences.import_theme(source).unwrap_err();
            assert!(error.contains("Copy JSON"), "{error}");
        }
        while preferences.themes.len() < super::MAX_SAVED_THEMES {
            preferences.import_theme("{}").unwrap();
        }
        preferences.appearance.overrides.accent = Some("green".into());
        assert!(preferences.import_theme(NEON).is_err());
        assert_eq!(preferences.themes.len(), super::MAX_SAVED_THEMES);
        assert_eq!(
            preferences.appearance.overrides.accent.as_deref(),
            Some("green")
        );
    }

    #[test]
    fn deleting_the_selected_theme_falls_back_to_the_default() {
        let mut preferences = Preferences::default();
        preferences.import_theme(NEON).unwrap();
        preferences.import_theme("{}").unwrap();
        preferences.appearance.overrides.radius = Some(4.0);
        preferences.appearance.overrides.accent = Some("pink".into());
        preferences.delete_theme("theme-1");
        assert_eq!(preferences.theme_id(), "theme-2");
        assert_eq!(preferences.appearance.overrides.radius, Some(4.0));
        preferences.delete_theme("theme-2");
        assert!(preferences.themes.is_empty());
        assert_eq!(preferences.theme_id(), "default");
        assert_eq!(preferences.appearance.overrides.radius, None);
        assert_eq!(
            preferences.appearance.overrides.accent.as_deref(),
            Some("pink")
        );
        assert_eq!(preferences.import_theme("{}").unwrap(), "theme-1");
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
        assert_eq!(preferences.pip_background_opacity, 0.8);
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
            pip_background_opacity: -1.0,
            pane_glow_strength: 3.0,
            ..preferences
        }
        .sanitized();
        assert_eq!(preferences.pane_margin, 0.0);
        assert_eq!(preferences.pane_inactive_opacity, 1.0);
        assert_eq!(preferences.pip_background_opacity, 0.0);
        assert_eq!(preferences.pane_glow_strength, 2.0);
        for (value, expected) in [(2.0, 1.0), (f32::NAN, 0.8)] {
            assert_eq!(
                Preferences {
                    pip_background_opacity: value,
                    ..Preferences::default()
                }
                .sanitized()
                .pip_background_opacity,
                expected,
            );
        }
    }
}
