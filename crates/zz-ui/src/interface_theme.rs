//! Interface themes: a [`Look`] (shape, outlines, shadows, glass) and the
//! colors it is drawn in for each mode, bundled in one file so a theme can
//! imitate a whole app instead of pairing a style with an unrelated palette.
//! The light, dark or system mode only picks which color set applies.

use std::sync::LazyLock;

use serde::{Deserialize, Deserializer, Serialize};
use zz_gpui::{GlassMaterial, Hsla, SharedString, px};

pub use zz_client::appearance::{
    BUILTIN_THEMES, DEFAULT_THEME, ThemeModeSetting, is_builtin_theme,
};

use crate::{FieldStyle, PrimaryStyle, SelectionStyle, Theme, ThemeColor, ThemeMode};

/// Every theme value that is not a color, in logical pixels where it is a
/// length. Missing keys take [`Look::default`]: the default theme's shape
/// with plain, glassless surfaces.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Look {
    pub radius: f32,
    pub corner_smoothing: f32,
    /// How strong every shadow is; 0 turns them off.
    pub shadow: f32,
    /// How much of that strength controls cast; 0 keeps buttons, fields and
    /// rows flat while popovers and cards keep theirs.
    pub control_shadow: f32,
    pub elevation: f32,
    pub outline: f32,
    pub outline_width: f32,
    pub row_inset: f32,
    pub selection: SelectionStyle,
    pub primary: PrimaryStyle,
    pub field: FieldStyle,
    pub density: f32,
    pub control_fill: f32,
    pub divider: f32,
    pub animation_speed: f32,
    /// The interface font family; `None` keeps the platform's.
    pub font: Option<SharedString>,
    /// What floating surfaces become; `None` fills them plainly. A theme
    /// file names a preset, or a preset with the fields it changes.
    #[serde(deserialize_with = "deserialize_glass")]
    pub glass: Option<GlassMaterial>,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            radius: 24.0,
            corner_smoothing: 4.0,
            shadow: 1.0,
            control_shadow: 1.0,
            elevation: 1.0,
            outline: 1.0,
            outline_width: 0.5,
            row_inset: 4.0,
            selection: SelectionStyle::Accent,
            primary: PrimaryStyle::Foreground,
            field: FieldStyle::Soft,
            density: 1.0,
            control_fill: 1.0,
            divider: 1.0,
            animation_speed: 1.0,
            font: None,
            glass: None,
        }
    }
}

impl Look {
    pub fn apply(&self, theme: &mut Theme) {
        theme.radius = px(self.radius);
        theme.corner_smoothing = self.corner_smoothing;
        theme.shadow = self.shadow > 0.0;
        theme.shadow_strength = self.shadow;
        theme.control_shadow = self.control_shadow;
        theme.elevation = self.elevation;
        theme.outline = self.outline;
        theme.outline_width = px(self.outline_width);
        theme.row_inset = px(self.row_inset);
        theme.selection = self.selection;
        theme.primary = self.primary;
        theme.field = self.field;
        theme.density = self.density;
        theme.control_fill = self.control_fill;
        theme.divider = self.divider;
        theme.animation_speed = self.animation_speed;
        if let Some(font) = &self.font {
            theme.font_family = font.clone();
        }
        theme.glass = self.glass;
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
#[serde(default, rename_all = "kebab-case")]
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

/// A look and its colors in both modes: one `themes/<name>.json` file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct InterfaceTheme {
    /// What Settings calls it.
    pub name: String,
    pub look: Look,
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

    /// Draws `theme` in this look and in this theme's colors for the mode
    /// `theme` is already in.
    pub fn apply(&self, theme: &mut Theme) {
        theme.colors = self.colors(theme.mode);
        self.look.apply(theme);
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Reads a theme file. Missing keys keep [`Look::default`] and the built-in
/// palette.
pub fn parse_theme(source: &str) -> Result<InterfaceTheme, String> {
    let value =
        serde_json::from_str::<serde_json::Value>(source).map_err(|error| error.to_string())?;
    if !value.is_object() {
        return Err("a theme is a JSON object".to_owned());
    }
    serde_json::from_value(value).map_err(|error| error.to_string())
}

const BUILTIN_SOURCES: [(&str, &str); BUILTIN_THEMES.len()] = [
    ("default", include_str!("../themes/default.json")),
    ("macos", include_str!("../themes/macos.json")),
    ("linear", include_str!("../themes/linear.json")),
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
            assert_eq!(
                Look {
                    glass: None,
                    ..once.look.clone()
                },
                Look {
                    glass: None,
                    ..theme.look.clone()
                },
            );
        }
    }

    #[test]
    fn partial_themes_keep_the_defaults_they_leave_out() {
        let theme =
            parse_theme(r##"{"look":{"outline":0.4},"dark":{"accent":"#ff0000"}}"##).unwrap();
        assert_eq!(
            theme.look,
            Look {
                outline: 0.4,
                ..Look::default()
            }
        );
        let dark = theme.colors(ThemeMode::Dark);
        assert_eq!(dark.accent, crate::parse_hex("#ff0000").unwrap());
        assert_eq!(dark.background, ThemeColor::dark().background);
        assert_eq!(theme.colors(ThemeMode::Light), *ThemeColor::light());
    }

    #[test]
    fn glass_takes_a_preset_or_a_preset_with_changes() {
        let named = parse_theme(r#"{"look":{"glass":"frosted"}}"#).unwrap();
        assert_eq!(named.look.glass, GlassMaterial::preset("frosted"));
        let changed = parse_theme(r#"{"look":{"glass":{"preset":"clear","blur":3}}}"#).unwrap();
        let clear = GlassMaterial::preset("clear").unwrap();
        assert_eq!(
            changed.look.glass,
            Some(GlassMaterial {
                blur: px(3.0),
                ..clear
            })
        );
        assert!(parse_theme(r#"{"look":{"glass":"stained"}}"#).is_err());
        assert!(parse_theme("[]").is_err());
    }
}
