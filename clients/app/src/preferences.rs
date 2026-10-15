use serde::{Deserialize, Deserializer, Serialize};
use zz_ui::interface_theme::{
    self, InterfaceTheme, MaterialOverrides, Materials, ThemeModeSetting,
};

pub const MAX_SAVED_THEMES: usize = 32;
const MAX_THEME_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub sidebar: bool,
    pub flat_sidebar: bool,
    pub sidebar_width: f32,
    pub status_badges: bool,
    pub status_agents: bool,
    pub animations: bool,
    pub gaps: bool,
    pub agent_enabled: bool,
    pub dark: bool,
    #[serde(default)]
    pub mode: Option<String>,
    /// The built-in or saved theme by id; `None` is the default theme.
    pub theme: Option<String>,
    /// Themes saved on this device by Duplicate or Import, by id.
    #[serde(deserialize_with = "saved_themes")]
    pub themes: Vec<(String, InterfaceTheme)>,
    pub ui_font_family: String,
    pub terminal_font_family: Option<String>,
    pub terminal_font_scale: f32,
    pub zoom: f32,
    /// Material values set over the theme. Picking a theme clears them.
    #[serde(flatten)]
    pub materials: MaterialOverrides,
    pub contrast: f32,
    pub pane_background_opacity: f32,
    pub pane_inactive_opacity: f32,
    pub pane_glow_strength: f32,
    pub pane_margin: f32,
    pub pane_radius: f32,
    pub pane_border_width: f32,
    pub palette_grouped: bool,
    pub palette_host_prefix: String,
    pub palette_show_keys: bool,
    pub extend_bottom_safe_area: bool,
    pub keep_screen_awake: bool,
    pub system_text_size: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            sidebar: true,
            flat_sidebar: false,
            sidebar_width: zz_ui::navigation::WORKSPACE_SIDEBAR_DEFAULT_WIDTH,
            status_badges: true,
            status_agents: true,
            animations: true,
            gaps: false,
            agent_enabled: true,
            dark: true,
            mode: Some("system".into()),
            theme: None,
            themes: Vec::new(),
            ui_font_family: String::new(),
            terminal_font_family: None,
            terminal_font_scale: 1.0,
            zoom: 1.0,
            materials: MaterialOverrides::default(),
            contrast: 1.0,
            pane_background_opacity: 0.5,
            pane_inactive_opacity: 0.7,
            pane_glow_strength: 1.0,
            pane_margin: 6.0,
            pane_radius: 13.5,
            pane_border_width: 0.5,
            palette_grouped: true,
            palette_host_prefix: "~".into(),
            palette_show_keys: true,
            extend_bottom_safe_area: false,
            keep_screen_awake: false,
            system_text_size: true,
        }
    }
}

impl Preferences {
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.sidebar_width = bounded(
            self.sidebar_width,
            160.0,
            640.0,
            zz_ui::navigation::WORKSPACE_SIDEBAR_DEFAULT_WIDTH,
        );
        let mut ids = std::collections::HashSet::new();
        self.themes.retain(|(id, _)| {
            !id.is_empty() && !interface_theme::is_builtin_theme(id) && ids.insert(id.clone())
        });
        self.themes.truncate(MAX_SAVED_THEMES);
        self.theme = self.theme.take().filter(|id| self.knows_theme(id));
        let theme = self.interface_theme().materials.clone();
        self.materials.radius = self
            .materials
            .radius
            .map(|value| bounded(value, 0.0, 25.0, theme.radius));
        self.materials.outline = self
            .materials
            .outline
            .map(|value| bounded(value, 0.0, 2.0, theme.outline));
        self.materials.shadow_strength = self
            .materials
            .shadow_strength
            .map(|value| bounded(value, 0.0, 2.0, 1.0));
        self.materials.row_inset = self
            .materials
            .row_inset
            .map(|value| bounded(value, 0.0, 12.0, theme.row_inset));
        self.materials.glass = self.materials.glass.take().filter(|glass| {
            zz_ui::settings::appearance::AppearancePageItem::Glass
                .material_choices()
                .iter()
                .skip(1)
                .any(|(_, choice)| choice == glass)
        });
        self.zoom = bounded(self.zoom, 0.5, 3.0, 1.0);
        self.terminal_font_scale = bounded(self.terminal_font_scale, 0.5, 3.0, 1.0);
        self.contrast = bounded(self.contrast, 1.0, 2.0, 1.0);
        for (value, min, max, fallback) in [
            (&mut self.pane_background_opacity, 0., 1., 0.5),
            (&mut self.pane_inactive_opacity, 0., 1., 0.7),
            (&mut self.pane_glow_strength, 0., 2., 1.),
            (&mut self.pane_margin, 0., 32., 6.),
            (&mut self.pane_radius, 0., 32., 13.5),
            (&mut self.pane_border_width, 0., 8., 0.5),
        ] {
            *value = bounded(*value, min, max, fallback);
        }
        self.mode = Some(self.theme_mode().as_str().into());
        if self.palette_host_prefix != "#" {
            self.palette_host_prefix = "~".into();
        }
        self.ui_font_family = self.ui_font_family.trim().into();
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
            show_agents: self.status_agents,
            show_host: false,
            show_update: false,
        }
    }

    #[must_use]
    pub fn palette_settings(&self) -> zz_ui::command::PaletteSettings {
        zz_ui::command::PaletteSettings {
            grouped: self.palette_grouped,
            host_prefix: if self.palette_host_prefix == "#" {
                "#"
            } else {
                "~"
            },
            show_keys: self.palette_show_keys,
        }
    }

    #[must_use]
    pub fn sidebar_width(&self, available_width: f32) -> f32 {
        self.sidebar_width
            .clamp(160.0, (available_width * 0.5).clamp(160.0, 640.0))
    }

    #[must_use]
    pub fn interface_theme(&self) -> &InterfaceTheme {
        self.theme
            .as_deref()
            .and_then(|id| {
                self.saved_theme(id)
                    .or_else(|| interface_theme::builtin_theme(id))
            })
            .unwrap_or_else(|| interface_theme::default_theme())
    }

    #[must_use]
    pub fn theme_id(&self) -> &str {
        self.theme
            .as_deref()
            .filter(|id| self.knows_theme(id))
            .unwrap_or(interface_theme::DEFAULT_THEME)
    }

    #[must_use]
    pub fn saved_theme(&self, id: &str) -> Option<&InterfaceTheme> {
        self.themes
            .iter()
            .find(|(saved, _)| saved == id)
            .map(|(_, theme)| theme)
    }

    fn knows_theme(&self, id: &str) -> bool {
        interface_theme::is_builtin_theme(id) || self.saved_theme(id).is_some()
    }

    /// Selects the built-in or saved theme `id`, or the default for any other
    /// id, and drops the overrides of what the theme decides: the
    /// materials, and the UI font when it sets one, so it shows as designed.
    pub fn set_interface_theme(&mut self, id: &str) {
        self.theme = self.knows_theme(id).then(|| id.to_owned());
        self.materials = MaterialOverrides::default();
        if self.interface_theme().materials.font.is_some() {
            self.ui_font_family.clear();
        }
    }

    /// Saves the active theme as it draws now, with the material and UI
    /// font overrides folded into its materials, and selects the copy.
    pub fn duplicate_theme(&mut self) -> Result<String, String> {
        let mut theme = self.interface_theme().clone();
        theme.materials = self.materials();
        if !self.ui_font_family.is_empty() {
            theme.materials.font = Some(self.ui_font_family.clone().into());
        }
        self.save_theme(theme)
    }

    /// Saves the theme file on the clipboard and selects it.
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
        self.themes.push((id.clone(), theme));
        self.set_interface_theme(&id);
        Ok(id)
    }

    /// Deletes the saved theme `id`, and selects the default theme when it
    /// was the selected one.
    pub fn delete_theme(&mut self, id: &str) {
        self.themes.retain(|(saved, _)| saved != id);
        if self.theme.as_deref() == Some(id) {
            self.set_interface_theme(interface_theme::DEFAULT_THEME);
        }
    }

    /// The UI font: the one picked on this device, else the theme's, or `""`
    /// for the system font.
    #[must_use]
    pub fn shown_ui_font(&self) -> &str {
        if self.ui_font_family.is_empty() {
            self.interface_theme()
                .materials
                .font
                .as_deref()
                .unwrap_or("")
        } else {
            &self.ui_font_family
        }
    }

    /// The active theme's materials with the overrides set here.
    #[must_use]
    pub fn materials(&self) -> Materials {
        let mut materials = self.interface_theme().materials.clone();
        self.materials.apply(&mut materials);
        materials
    }

    #[must_use]
    pub fn widget_corner_radius(&self) -> f32 {
        self.materials().radius
    }

    #[must_use]
    pub fn theme_mode(&self) -> ThemeModeSetting {
        self.mode.as_deref().map_or(
            if self.dark {
                ThemeModeSetting::Dark
            } else {
                ThemeModeSetting::Light
            },
            |mode| ThemeModeSetting::parse(mode).unwrap_or(ThemeModeSetting::System),
        )
    }

    pub fn decode(value: &str) -> serde_json::Result<Self> {
        let source: serde_json::Value = serde_json::from_str(value)?;
        let mut preferences: Self = serde_json::from_value(source.clone())?;
        if source.get("mode").is_none() && source.get("dark").is_none() {
            preferences.mode = Some("system".into());
        }
        if source.get("theme").is_none() {
            preferences.materials = MaterialOverrides::default();
        }
        Ok(preferences.sanitized())
    }

    #[cfg(any(target_os = "ios", test))]
    pub fn load_path(path: &std::path::Path) -> std::io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(value) => Self::decode(&value)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    #[cfg(any(target_os = "ios", test))]
    pub fn save_path(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let value = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, value)?;
        std::fs::rename(temporary, path)
    }

    #[cfg(target_os = "ios")]
    fn path() -> std::path::PathBuf {
        std::path::PathBuf::from(std::env::var_os("HOME").expect("iOS application home directory"))
            .join("Library/Application Support/zz-gpui/preferences.json")
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
        #[cfg(target_os = "ios")]
        {
            Self::load_path(&Self::path()).unwrap_or_default()
        }
        #[cfg(not(target_os = "ios"))]
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
        #[cfg(target_os = "ios")]
        if let Err(error) = self.save_path(&Self::path()) {
            log::warn!("Could not save preferences: {error}");
        }
    }

    #[cfg(target_family = "wasm")]
    const STORAGE_KEY: &str = if zz_protocol::app_identity::DEVELOPMENT {
        "zz-dev-web-preferences"
    } else {
        "zz-web-preferences"
    };
}

/// Reads the saved themes, skipping any this build cannot read instead of
/// losing every preference over one theme.
fn saved_themes<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<(String, InterfaceTheme)>, D::Error> {
    Ok(
        Vec::<(String, serde_json::Value)>::deserialize(deserializer)?
            .into_iter()
            .filter_map(|(id, theme)| Some((id, serde_json::from_value(theme).ok()?)))
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

#[cfg(test)]
mod tests {
    use super::Preferences;
    use zz_ui::interface_theme::{MaterialOverrides, ThemeModeSetting};

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
            mode: Some("light".into()),
            animations: false,
            flat_sidebar: true,
            ui_font_family: "Inter Variable".into(),
            terminal_font_family: Some("Lilex".into()),
            terminal_font_scale: 1.25,
            pane_margin: 7.5,
            pane_radius: 10.5,
            pane_border_width: 1.5,
            ..Preferences::default()
        };
        expected.save_path(&path).unwrap();
        assert_eq!(Preferences::load_path(&path).unwrap(), expected);
        std::fs::write(&path, r#"{"mode":"dark"}"#).unwrap();
        assert_eq!(
            Preferences::load_path(&path).unwrap(),
            Preferences {
                mode: Some("dark".into()),
                ..Preferences::default()
            }
        );
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
        assert!(defaults.animations);
        assert_eq!(defaults.materials, MaterialOverrides::default());
        assert_eq!(
            defaults.sidebar_width,
            zz_ui::navigation::WORKSPACE_SIDEBAR_DEFAULT_WIDTH
        );
        let status = defaults.status_bar_settings();
        assert!(status.badges && status.show_agents);
        assert!(!status.show_host && !status.show_update);
        let changed = Preferences {
            status_badges: false,
            status_agents: false,
            animations: false,
            materials: MaterialOverrides {
                shadow_strength: Some(0.0),
                ..MaterialOverrides::default()
            },
            sidebar_width: 320.0,
            ..defaults
        };
        let saved = serde_json::to_string(&changed).unwrap();
        let restored = serde_json::from_str::<Preferences>(&saved)
            .unwrap()
            .sanitized();
        assert_eq!(
            restored.status_bar_settings(),
            changed.status_bar_settings()
        );
        assert!(!restored.animations);
        assert_eq!(restored.materials().float.strength, 0.0);
        assert_eq!(restored.materials().contact.strength, 0.0);
        assert_eq!(restored.sidebar_width, 320.0);
    }

    #[test]
    fn sidebar_and_material_values_stay_within_supported_limits() {
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
        let invalid = Preferences {
            sidebar_width: f32::NAN,
            materials: MaterialOverrides {
                outline: Some(f32::INFINITY),
                shadow_strength: Some(f32::NAN),
                glass: Some("stained".to_owned()),
                ..MaterialOverrides::default()
            },
            ..Preferences::default()
        }
        .sanitized();
        assert_eq!(invalid.sidebar_width, Preferences::default().sidebar_width);
        assert_eq!(invalid.materials().outline, 0.5);
        assert_eq!(invalid.materials.shadow_strength, Some(1.0));
        assert_eq!(invalid.materials.glass, None);
        for (value, expected) in [(-0.1, 0.0), (3.0, 2.0), (1.5, 1.5)] {
            let preferences = Preferences {
                materials: MaterialOverrides {
                    outline: Some(value),
                    shadow_strength: Some(value),
                    ..MaterialOverrides::default()
                },
                ..Preferences::default()
            }
            .sanitized();
            assert_eq!(preferences.materials().outline, expected);
            assert_eq!(preferences.materials.shadow_strength, Some(expected));
        }
    }

    #[test]
    fn contrast_defaults_clamps_and_round_trips_with_preferences() {
        let defaults = serde_json::from_str::<Preferences>("{}").unwrap();
        assert_eq!(defaults.contrast, 1.0);
        for (value, expected) in [(0.1, 1.0), (3.0, 2.0), (1.5, 1.5), (f32::NAN, 1.0)] {
            let preferences = Preferences {
                contrast: value,
                ..Preferences::default()
            }
            .sanitized();
            assert_eq!(preferences.contrast, expected);
            let saved = serde_json::to_string(&preferences).unwrap();
            assert_eq!(
                serde_json::from_str::<Preferences>(&saved)
                    .unwrap()
                    .contrast,
                expected
            );
        }
    }

    #[test]
    fn full_widget_radius_survives_saved_preferences() {
        let preferences = Preferences {
            materials: MaterialOverrides {
                radius: Some(25.0),
                ..MaterialOverrides::default()
            },
            ..Preferences::default()
        };
        let saved = serde_json::to_string(&preferences).unwrap();
        let loaded = serde_json::from_str::<Preferences>(&saved)
            .unwrap()
            .sanitized();
        assert_eq!(loaded.widget_corner_radius(), 25.0);
    }

    #[test]
    fn the_theme_decides_radius_and_shadow_until_they_are_set() {
        let older =
            Preferences::decode(r#"{"radius":12,"shadow_strength":0.3,"style":"flat"}"#).unwrap();
        assert_eq!(older.theme_id(), "default");
        assert_eq!(older.materials, MaterialOverrides::default());
        assert_eq!(older.widget_corner_radius(), 10.0);

        let mut tuned = Preferences::decode(r#"{"theme":null,"radius":12}"#).unwrap();
        assert_eq!(tuned.widget_corner_radius(), 12.0);
        let saved = serde_json::to_string(&tuned).unwrap();
        assert_eq!(
            Preferences::decode(&saved).unwrap().materials.radius,
            Some(12.0)
        );

        tuned.set_interface_theme("default");
        let picked = Preferences::decode(&serde_json::to_string(&tuned).unwrap()).unwrap();
        assert_eq!(picked.theme_id(), "default");
        let materials = &zz_ui::interface_theme::builtin_theme("default")
            .unwrap()
            .materials;
        assert_eq!(
            (picked.widget_corner_radius(), picked.materials().float),
            (materials.radius, materials.float)
        );
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

        let mut frosted = Preferences {
            materials: MaterialOverrides {
                glass: Some("frosted".to_owned()),
                ..MaterialOverrides::default()
            },
            ..Preferences::default()
        };
        frosted.duplicate_theme().unwrap();
        let restored = Preferences::decode(&serde_json::to_string(&frosted).unwrap()).unwrap();
        assert_eq!(restored.theme_id(), "theme-1");
        assert!(restored.interface_theme().materials.glass.is_some());

        let older = Preferences::decode(
            r#"{"theme":"default","radius":12,"shadows":"none","selection":"wash","field":"outline"}"#,
        )
        .unwrap();
        assert!(older.themes.is_empty());
        assert_eq!(older.theme_id(), "default");
        assert_eq!(
            older.materials,
            MaterialOverrides {
                radius: Some(12.0),
                ..MaterialOverrides::default()
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
        let preferences = Preferences {
            theme: Some("theme-2".into()),
            themes: vec![
                ("theme-2".into(), theme()),
                ("default".into(), theme()),
                (String::new(), theme()),
                ("theme-2".into(), theme()),
            ],
            ..Preferences::default()
        }
        .sanitized();
        assert_eq!(preferences.theme.as_deref(), Some("theme-2"));
        assert_eq!(preferences.themes.len(), 1);
        let dangling = Preferences {
            theme: Some("theme-9".into()),
            ..preferences
        }
        .sanitized();
        assert_eq!(dangling.theme, None);
        assert_eq!(dangling.theme_id(), "default");

        let crowded = Preferences {
            themes: (1..=40)
                .map(|index| (format!("theme-{index}"), theme()))
                .collect(),
            ..Preferences::default()
        }
        .sanitized();
        assert_eq!(crowded.themes.len(), super::MAX_SAVED_THEMES);
    }

    #[test]
    fn duplicate_keeps_what_is_on_screen() {
        let mut preferences = Preferences {
            theme: Some("default".into()),
            materials: MaterialOverrides {
                radius: Some(7.0),
                shadow_strength: Some(0.5),
                ..MaterialOverrides::default()
            },
            ui_font_family: "Lilex".into(),
            ..Preferences::default()
        };
        assert_eq!(preferences.duplicate_theme().unwrap(), "theme-1");
        let copy = preferences.saved_theme("theme-1").unwrap();
        assert_eq!(copy.name, "theme-1");
        let builtin = zz_ui::interface_theme::builtin_theme("default").unwrap();
        let float = builtin.materials.float.strength * 0.5;
        assert_eq!(
            (
                copy.materials.radius,
                copy.materials.float.strength,
                copy.materials.font.as_deref()
            ),
            (7.0, float, Some("Lilex"))
        );
        assert_eq!(copy.roles, builtin.roles);
        assert_eq!((&copy.light, &copy.dark), (&builtin.light, &builtin.dark));
        assert_eq!(preferences.theme_id(), "theme-1");
        assert_eq!(preferences.materials, MaterialOverrides::default());
        assert!(preferences.ui_font_family.is_empty());
        assert_eq!(
            (
                preferences.widget_corner_radius(),
                preferences.materials().float.strength,
                preferences.shown_ui_font()
            ),
            (7.0, float, "Lilex")
        );
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
        assert!(preferences.import_theme(NEON).is_err());
        assert!(preferences.duplicate_theme().is_err());
        assert_eq!(preferences.themes.len(), super::MAX_SAVED_THEMES);
    }

    #[test]
    fn deleting_the_selected_theme_falls_back_to_the_default() {
        let mut preferences = Preferences::default();
        preferences.import_theme(NEON).unwrap();
        preferences.import_theme("{}").unwrap();
        preferences.materials.radius = Some(4.0);
        preferences.delete_theme("theme-1");
        assert_eq!(preferences.theme_id(), "theme-2");
        assert_eq!(preferences.materials.radius, Some(4.0));
        preferences.delete_theme("theme-2");
        assert!(preferences.themes.is_empty());
        assert_eq!(preferences.theme_id(), "default");
        assert_eq!(preferences.materials.radius, None);
        assert_eq!(preferences.import_theme("{}").unwrap(), "theme-1");
    }

    #[test]
    fn older_preferences_gain_pane_controls_and_values_remain_bounded() {
        let preferences = serde_json::from_str::<Preferences>(r#"{"gaps":true,"radius":8}"#)
            .unwrap()
            .sanitized();
        assert!(preferences.gaps);
        assert_eq!(preferences.materials.radius, Some(8.0));
        assert_eq!(preferences.pane_margin, 6.0);
        assert_eq!(preferences.pane_radius, 13.5);
        assert_eq!(preferences.pane_background_opacity, 0.5);
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
            pane_radius: 200.0,
            pane_border_width: f32::NAN,
            pane_inactive_opacity: 2.0,
            pane_background_opacity: -1.0,
            pane_glow_strength: 3.0,
            ..preferences
        }
        .sanitized();
        assert_eq!(preferences.pane_margin, 0.0);
        assert_eq!(preferences.pane_radius, 32.0);
        assert_eq!(preferences.pane_border_width, 0.5);
        assert_eq!(preferences.pane_inactive_opacity, 1.0);
        assert_eq!(preferences.pane_background_opacity, 0.0);
        assert_eq!(preferences.pane_glow_strength, 2.0);
        for (value, expected) in [(2.0, 1.0), (f32::NAN, 0.5)] {
            assert_eq!(
                Preferences {
                    pane_background_opacity: value,
                    ..Preferences::default()
                }
                .sanitized()
                .pane_background_opacity,
                expected,
            );
        }
    }
}
