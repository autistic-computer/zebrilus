//! Interface themes: the materials a theme draws with (radius, outlines,
//! shadows, glass), how each piece of UI uses them (its roles), and the
//! colors for each mode, bundled in one file so a theme can imitate a whole
//! app. The light, dark or system mode only picks which color set applies.

use std::{collections::BTreeMap, sync::LazyLock};

use serde::{Deserialize, Deserializer, Serialize};
use zz_gpui::{GlassMaterial, Hsla, SharedString, px};

pub use zz_client::appearance::{
    BUILTIN_THEMES, DEFAULT_THEME, ThemeModeSetting, is_builtin_theme,
};

use crate::{PrimaryStyle, ShadowMaterial, StylePatch, Theme, ThemeColor, ThemeMode};

/// What each treatment looks like, in logical pixels where it is a length.
/// Missing keys take [`Materials::default`]: the Default theme's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Materials {
    pub radius: f32,
    /// The width of every outline; 0 draws none.
    pub outline: f32,
    /// How far list rows sit in from their list's edges; 0 makes them
    /// square bands that touch.
    pub row_inset: f32,
    pub primary: PrimaryStyle,
    /// The interface font family; `None` keeps the platform's.
    pub font: Option<SharedString>,
    /// What surfaces whose role takes glass become; `None` fills them
    /// plainly. A theme file names a preset, or a preset with the fields it
    /// changes.
    #[serde(deserialize_with = "deserialize_glass")]
    pub glass: Option<GlassMaterial>,
    /// The tight shadow of things resting on the window.
    #[serde(deserialize_with = "deserialize_contact")]
    pub contact: ShadowMaterial,
    /// The deep shadow of things floating over it.
    #[serde(deserialize_with = "deserialize_float")]
    pub float: ShadowMaterial,
}

impl Default for Materials {
    fn default() -> Self {
        Self {
            radius: 24.0,
            outline: 0.5,
            row_inset: 4.0,
            primary: PrimaryStyle::Foreground,
            font: None,
            glass: None,
            contact: ShadowMaterial::CONTACT,
            float: ShadowMaterial::FLOAT,
        }
    }
}

impl Materials {
    pub fn apply(&self, theme: &mut Theme) {
        theme.radius = px(self.radius);
        theme.outline = px(self.outline);
        theme.row_inset = px(self.row_inset);
        theme.primary = self.primary;
        if let Some(font) = &self.font {
            theme.font_family = font.clone();
        }
        theme.glass = self.glass;
        theme.contact_shadow = self.contact;
        theme.float_shadow = self.float;
    }
}

/// Material values Settings sets over the active theme, each `None` while
/// it follows the theme. Picking a theme clears them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct MaterialOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline: Option<f32>,
    /// Scales both of the theme's shadows: 1 keeps them, 0 turns them off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_strength: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_inset: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<PrimaryStyle>,
    /// `off`, or the name of a glass preset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glass: Option<String>,
}

impl MaterialOverrides {
    /// `materials` with every override set here.
    pub fn apply(&self, materials: &mut Materials) {
        if let Some(radius) = self.radius {
            materials.radius = radius;
        }
        if let Some(outline) = self.outline {
            materials.outline = outline;
        }
        if let Some(strength) = self.shadow_strength {
            materials.contact.strength *= strength;
            materials.float.strength *= strength;
        }
        if let Some(row_inset) = self.row_inset {
            materials.row_inset = row_inset;
        }
        if let Some(primary) = self.primary {
            materials.primary = primary;
        }
        match self.glass.as_deref() {
            Some("off") => materials.glass = None,
            Some(name) => {
                if let Some(material) = GlassMaterial::preset(name) {
                    materials.glass = Some(material);
                }
            }
            None => {}
        }
    }
}

/// A look setting spelled as a theme file and `zz/config` spell it, such as
/// `surfaces` for [`Shadows::Surfaces`].
pub fn choice_name<T: Serialize>(value: T) -> Option<String> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(name)) => Some(name),
        _ => None,
    }
}

/// The look setting `name` spells, the inverse of [`choice_name`].
pub fn parse_choice<T: serde::de::DeserializeOwned>(name: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(name.trim().to_owned())).ok()
}

/// A JSON object over `base`: the fields it names, `base` for the rest.
fn over<'de, T, D>(base: T, deserializer: D) -> Result<T, D::Error>
where
    T: Serialize + serde::de::DeserializeOwned,
    D: Deserializer<'de>,
{
    use serde::de::Error as _;
    let fields = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
    let mut merged = serde_json::to_value(base).map_err(D::Error::custom)?;
    if let serde_json::Value::Object(target) = &mut merged {
        target.extend(fields);
    }
    serde_json::from_value(merged).map_err(D::Error::custom)
}

fn deserialize_contact<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<ShadowMaterial, D::Error> {
    over(ShadowMaterial::CONTACT, deserializer)
}

fn deserialize_float<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<ShadowMaterial, D::Error> {
    over(ShadowMaterial::FLOAT, deserializer)
}

/// A theme's role styles, keyed by role ids the UI knows.
fn deserialize_roles<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, StylePatch>, D::Error> {
    use serde::de::Error as _;
    let roles = BTreeMap::<String, StylePatch>::deserialize(deserializer)?;
    for id in roles.keys() {
        if crate::roles::find(id).is_none() {
            return Err(D::Error::custom(unknown_role(id)));
        }
    }
    Ok(roles)
}

fn unknown_role(id: &str) -> String {
    let spelled = |name: &str| name.replace(['-', '_', '.'], "").to_ascii_lowercase();
    let near = crate::roles::all()
        .map(|role| role.id)
        .find(|known| spelled(known) == spelled(id))
        .or_else(|| {
            crate::roles::all()
                .map(|role| role.id)
                .find(|known| known.ends_with(&format!(".{id}")))
        });
    match near {
        Some(known) => format!("unknown role `{id}`, did you mean `{known}`?"),
        None => format!("unknown role `{id}`"),
    }
}

fn deserialize_glass<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<GlassMaterial>, D::Error> {
    use serde::de::Error as _;
    let preset = |name: &str| {
        GlassMaterial::preset(name)
            .ok_or_else(|| D::Error::custom(format!("no glass preset named {name}")))
    };
    match Option::<serde_json::Value>::deserialize(deserializer)? {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(name)) => preset(&name).map(Some),
        Some(serde_json::Value::Object(mut fields)) => {
            let base = match fields.remove("preset") {
                Some(serde_json::Value::String(name)) => preset(&name)?,
                Some(_) => return Err(D::Error::custom("glass preset must be a name")),
                None => GlassMaterial::regular(),
            };
            let mut merged = serde_json::to_value(base).map_err(D::Error::custom)?;
            if let serde_json::Value::Object(target) = &mut merged {
                target.extend(fields);
            }
            serde_json::from_value(merged)
                .map(Some)
                .map_err(D::Error::custom)
        }
        Some(_) => Err(D::Error::custom("glass must be a preset name or an object")),
    }
}

/// One mode's colors. The six roots fall back to the built-in palette for
/// the mode; `on-accent` and `shadow-color` fall back to what the palette
/// derives.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct ThemeColors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<Hsla>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreground: Option<Hsla>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<Hsla>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<Hsla>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<Hsla>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub danger: Option<Hsla>,
    /// Text on an accent fill.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_accent: Option<Hsla>,
    /// The ink of drop shadows; its alpha is a floating surface's shadow at
    /// full strength.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_color: Option<Hsla>,
}

impl ThemeColors {
    pub fn resolve(&self, mode: ThemeMode) -> ThemeColor {
        let mut colors = *ThemeColor::for_mode(mode);
        let roots = [
            (&mut colors.background, self.background),
            (&mut colors.foreground, self.foreground),
            (&mut colors.accent, self.accent),
            (&mut colors.success, self.success),
            (&mut colors.warning, self.warning),
            (&mut colors.danger, self.danger),
            (&mut colors.shadow_color, self.shadow_color),
        ];
        for (root, value) in roots {
            if let Some(value) = value {
                *root = value;
            }
        }
        colors.on_accent = self.on_accent.or(colors.on_accent);
        colors
    }
}

/// Materials, roles and colors in both modes: one `themes/<name>.json` file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct InterfaceTheme {
    /// What Settings calls it.
    pub name: String,
    pub materials: Materials,
    /// How the theme changes each kind and role, by id; what it leaves out
    /// draws as built in.
    #[serde(
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "deserialize_roles"
    )]
    pub roles: BTreeMap<String, StylePatch>,
    pub light: ThemeColors,
    pub dark: ThemeColors,
}

impl InterfaceTheme {
    pub fn colors(&self, mode: ThemeMode) -> ThemeColor {
        match mode {
            ThemeMode::Light => self.light.resolve(mode),
            ThemeMode::Dark => self.dark.resolve(mode),
        }
    }

    /// Draws `theme` in this theme's materials and roles, and in its colors
    /// for the mode `theme` is already in.
    pub fn apply(&self, theme: &mut Theme) {
        theme.colors = self.colors(theme.mode);
        self.materials.apply(theme);
        self.apply_roles(theme);
    }

    /// Hands the theme's role styles to `theme`.
    pub fn apply_roles(&self, theme: &mut Theme) {
        theme.styles = self
            .roles
            .iter()
            .map(|(id, patch)| (SharedString::from(id.clone()), *patch))
            .collect();
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Names a theme saved as `id` after it when it has no name of its own
    /// or a built-in's, so Settings never shows two tiles called Default.
    pub fn name_after(&mut self, id: &str) {
        if self.name.trim().is_empty()
            || builtin_themes()
                .iter()
                .any(|(_, builtin)| builtin.name == self.name)
        {
            id.clone_into(&mut self.name);
        }
    }
}

/// Reads a theme file. Missing keys keep [`Materials::default`], the
/// built-in role styles and the built-in palette.
pub fn parse_theme(source: &str) -> Result<InterfaceTheme, String> {
    let value =
        serde_json::from_str::<serde_json::Value>(source).map_err(|error| error.to_string())?;
    if !value.is_object() {
        return Err("a theme is a JSON object".to_owned());
    }
    serde_json::from_value(value).map_err(|error| with_spelling_hint(error.to_string()))
}

fn with_spelling_hint(message: String) -> String {
    let Some((key, expected)) = message
        .strip_prefix("unknown field `")
        .and_then(|rest| rest.split_once("`, expected "))
    else {
        return message;
    };
    let spelled = |name: &str| name.replace(['-', '_'], "").to_ascii_lowercase();
    expected
        .split('`')
        .skip(1)
        .step_by(2)
        .find(|name| spelled(name) == spelled(key))
        .map_or_else(
            || message.clone(),
            |name| format!("unknown field `{key}`, did you mean `{name}`?"),
        )
}

const BUILTIN_SOURCES: [(&str, &str); BUILTIN_THEMES.len()] = [
    ("default", include_str!("../themes/default.json")),
    ("macos", include_str!("../themes/macos.json")),
    ("tahoe", include_str!("../themes/tahoe.json")),
    ("linear", include_str!("../themes/linear.json")),
    ("glass", include_str!("../themes/glass.json")),
];

static BUILTINS: LazyLock<Vec<(&'static str, InterfaceTheme)>> = LazyLock::new(|| {
    BUILTIN_SOURCES
        .iter()
        .map(|(id, source)| {
            let theme = parse_theme(source)
                .unwrap_or_else(|error| panic!("built-in theme {id} does not parse: {error}"));
            (*id, theme)
        })
        .collect()
});

/// The themes every client ships, by id, in the order Settings lists them.
pub fn builtin_themes() -> &'static [(&'static str, InterfaceTheme)] {
    &BUILTINS
}

pub fn builtin_theme(id: &str) -> Option<&'static InterfaceTheme> {
    builtin_themes()
        .iter()
        .find(|(builtin, _)| *builtin == id.trim())
        .map(|(_, theme)| theme)
}

pub fn default_theme() -> &'static InterfaceTheme {
    builtin_theme(DEFAULT_THEME).expect("the default theme is built in")
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
    use crate::{Colorize as _, Elevation};

    fn luminance(color: Hsla) -> f32 {
        let rgba: zz_gpui::Rgba = color.into();
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
    fn every_builtin_parses_with_its_id_and_a_name() {
        let ids = builtin_themes()
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        assert_eq!(ids, BUILTIN_THEMES);
        for (id, theme) in builtin_themes() {
            assert!(!theme.name.is_empty(), "{id} has no name");
            for colors in [&theme.light, &theme.dark] {
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
    }

    #[test]
    fn builtin_text_and_status_colors_read_on_their_surfaces() {
        for (id, theme) in builtin_themes() {
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
    fn partial_themes_keep_the_defaults_they_leave_out() {
        let theme = parse_theme(
            r##"{"materials":{"outline":0.4,"float":{"y":4}},"dark":{"accent":"#ff0000"}}"##,
        )
        .unwrap();
        assert_eq!(
            theme.materials,
            Materials {
                outline: 0.4,
                float: ShadowMaterial {
                    y: 4.0,
                    ..ShadowMaterial::FLOAT
                },
                ..Materials::default()
            }
        );
        assert!(theme.roles.is_empty());
        let dark = theme.colors(ThemeMode::Dark);
        assert_eq!(dark.accent, crate::parse_hex("#ff0000").unwrap());
        assert_eq!(dark.background, ThemeColor::dark().background);
        assert_eq!(theme.colors(ThemeMode::Light), *ThemeColor::light());
    }

    #[test]
    fn roles_change_only_what_they_name() {
        use crate::{Fill, Kind, Shape};
        let theme = parse_theme(
            r#"{"roles":{"chrome.button":{"fill":"glass","shape":"pill"},"button":{"outline":false}}}"#,
        )
        .unwrap();
        let mut kit = Theme::default();
        theme.apply(&mut kit);
        let chrome = kit.style(crate::roles::CHROME_BUTTON);
        assert_eq!((chrome.fill, chrome.shape), (Fill::Glass, Shape::Pill));
        assert_eq!(chrome.hover, Kind::IconButton.base().hover);
        assert!(!kit.style(Kind::Button.role()).outline);
        assert!(kit.style(crate::roles::PANE_CHIP).outline);
    }

    #[test]
    fn unknown_roles_are_errors_that_suggest_the_role() {
        let error = parse_theme(r#"{"roles":{"chromebutton":{}}}"#).unwrap_err();
        assert!(
            error.contains("did you mean `chrome.button`"),
            "{error}"
        );
        let error = parse_theme(r#"{"roles":{"composer":{}}}"#).unwrap_err();
        assert!(error.contains("did you mean `agent.composer`"), "{error}");
        assert!(parse_theme(r#"{"roles":{"sparkle":{}}}"#).is_err());
        assert!(parse_theme(r#"{"roles":{"row":{"fill":"neon"}}}"#).is_err());
    }

    #[test]
    fn glass_takes_a_preset_or_a_preset_with_changes() {
        let named = parse_theme(r#"{"materials":{"glass":"frosted"}}"#).unwrap();
        assert_eq!(named.materials.glass, GlassMaterial::preset("frosted"));
        let changed =
            parse_theme(r#"{"materials":{"glass":{"preset":"clear","blur":3}}}"#).unwrap();
        let clear = GlassMaterial::preset("clear").unwrap();
        assert_eq!(
            changed.materials.glass,
            Some(GlassMaterial {
                blur: px(3.0),
                ..clear
            })
        );
        let lit = parse_theme(
            r##"{"materials":{"glass":{"preset":"clear","light_color":"#ff0000"}}}"##,
        )
        .unwrap();
        assert_eq!(
            lit.materials.glass.map(|glass| glass.light_color),
            Some(crate::parse_hex("#ff0000").unwrap())
        );
        assert!(parse_theme(r#"{"materials":{"glass":"stained"}}"#).is_err());
        assert!(parse_theme("[]").is_err());
    }

    #[test]
    fn overrides_replace_only_what_they_set() {
        let theme = builtin_theme("macos").unwrap();
        let mut materials = theme.materials.clone();
        MaterialOverrides {
            outline: Some(1.0),
            shadow_strength: Some(0.5),
            glass: Some("frosted".to_owned()),
            ..MaterialOverrides::default()
        }
        .apply(&mut materials);
        assert_eq!(materials.outline, 1.0);
        assert_eq!(
            materials.float.strength,
            theme.materials.float.strength * 0.5
        );
        assert_eq!(materials.glass, GlassMaterial::preset("frosted"));
        assert_eq!(materials.radius, theme.materials.radius);
        assert_eq!(materials.primary, theme.materials.primary);

        MaterialOverrides {
            glass: Some("off".to_owned()),
            ..MaterialOverrides::default()
        }
        .apply(&mut materials);
        assert_eq!(materials.glass, None);
    }

    #[test]
    fn choices_spell_the_way_settings_and_config_do() {
        use zz_client::appearance::{GLASS_CHOICES, PRIMARY_CHOICES};
        for name in PRIMARY_CHOICES {
            let primary = parse_choice::<PrimaryStyle>(name).unwrap();
            assert_eq!(choice_name(primary).as_deref(), Some(name));
        }
        let presets = GlassMaterial::PRESETS
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>();
        assert_eq!(GLASS_CHOICES[2..], presets[..]);
    }

    #[test]
    fn unknown_keys_are_errors_that_name_the_key() {
        for (source, key, spelling) in [
            (
                r#"{"materials":{"row_inset":2}}"#,
                "row_inset",
                Some("row-inset"),
            ),
            (
                r##"{"materials":{"glass":{"preset":"clear","Light-Color":"#ff0000"}}}"##,
                "Light-Color",
                Some("light_color"),
            ),
            (
                r##"{"dark":{"onAccent":"#ffffff"}}"##,
                "onAccent",
                Some("on-accent"),
            ),
            (r#"{"Materials":{}}"#, "Materials", Some("materials")),
            (r#"{"materials":{"sparkle":1}}"#, "sparkle", None),
            (r#"{"materials":{"glass":{"sparkle":1}}}"#, "sparkle", None),
            (r#"{"look":{}}"#, "look", None),
        ] {
            let error = parse_theme(source).unwrap_err();
            match spelling {
                Some(spelling) => assert_eq!(
                    error,
                    format!("unknown field `{key}`, did you mean `{spelling}`?")
                ),
                None => assert!(
                    error.starts_with(&format!("unknown field `{key}`, expected ")),
                    "{error}"
                ),
            }
        }
    }
}
