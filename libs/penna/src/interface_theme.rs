use std::{collections::BTreeMap, sync::OnceLock};

use ala::{GlassMaterial, Hsla, SharedString, px};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{PrimaryStyle, ShadowMaterial, Smoothing, StylePatch, Theme, ThemeColor, ThemeMode};

pub const DEFAULT_THEME: &str = "default";

pub const MAX_CORNER_RADIUS: f32 = 25.0;

pub const ACCENTS: [&str; 8] = [
    "blue", "purple", "pink", "red", "orange", "yellow", "green", "graphite",
];

pub const DEFAULT_TINT_HUE: f32 = 260.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Materials {
    pub radius: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_radius: Option<f32>,
    pub smoothing: Smoothing,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub picked_smoothing: bool,
    pub outline: f32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub picked_outlines: bool,
    pub row_inset: f32,
    pub primary: PrimaryStyle,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub flat_highlights: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bright_surfaces: bool,
    pub highlight_strength: f32,
    pub font: Option<SharedString>,
    #[serde(deserialize_with = "deserialize_glass")]
    pub glass: Option<GlassMaterial>,
    #[serde(
        deserialize_with = "deserialize_glass",
        skip_serializing_if = "Option::is_none"
    )]
    pub light_glass: Option<GlassMaterial>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub picked_shadows: bool,
    #[serde(deserialize_with = "deserialize_contact")]
    pub contact: ShadowMaterial,
    #[serde(deserialize_with = "deserialize_float")]
    pub float: ShadowMaterial,
}

impl Default for Materials {
    fn default() -> Self {
        Self {
            radius: 10.0,
            control_radius: None,
            smoothing: Smoothing::THEME,
            picked_smoothing: false,
            outline: 0.5,
            picked_outlines: false,
            row_inset: 4.0,
            primary: PrimaryStyle::Foreground,
            flat_highlights: false,
            bright_surfaces: false,
            highlight_strength: 1.0,
            font: None,
            glass: None,
            light_glass: None,
            picked_shadows: false,
            contact: ShadowMaterial::CONTACT,
            float: ShadowMaterial::FLOAT,
        }
    }
}

impl Materials {
    pub fn apply(&self, theme: &mut Theme) {
        theme.radius = px(self.radius);
        theme.control_radius = self.control_radius.map(px);
        theme.smoothing = self.smoothing;
        theme.picked_smoothing = self.picked_smoothing;
        theme.outline = px(self.outline);
        theme.picked_outlines = self.picked_outlines;
        theme.row_inset = px(self.row_inset);
        theme.primary = self.primary;
        theme.flat_highlights = self.flat_highlights;
        theme.bright_surfaces = self.bright_surfaces;
        theme.highlight_strength = self.highlight_strength.max(0.0);
        if let Some(font) = &self.font {
            theme.font_family = font.clone();
        }
        theme.glass = match theme.mode {
            ThemeMode::Light => self.light_glass.or(self.glass),
            ThemeMode::Dark => self.glass,
        };
        theme.picked_shadows = self.picked_shadows;
        theme.contact_shadow = self.contact;
        theme.float_shadow = self.float;
    }
}

const ACCENT_COLORS: [(&str, &str, &str); ACCENTS.len()] = [
    ("blue", "#146ae3", "#76abfd"),
    ("purple", "#8a4fd7", "#bd97fe"),
    ("pink", "#c63187", "#f585bd"),
    ("red", "#d32e33", "#ff877f"),
    ("orange", "#b35407", "#f88e4f"),
    ("yellow", "#8d6a07", "#d3a229"),
    ("green", "#028132", "#59be6d"),
    ("graphite", "#6f6f6f", "#aaaaaa"),
];

pub fn accent_color(value: &str, mode: ThemeMode) -> Option<Hsla> {
    let value = value.trim();
    match ACCENT_COLORS.iter().find(|(name, ..)| *name == value) {
        Some((_, light, dark)) => crate::parse_hex(if mode.is_dark() { dark } else { light }).ok(),
        None => crate::parse_hex(value).ok(),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ThemeOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tint: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tint_hue: Option<f32>,
}

impl ThemeOverrides {
    pub fn apply_to(&self, theme: &mut InterfaceTheme) {
        if let Some(radius) = self.radius {
            theme.materials.radius = radius;
        }
        let tint = self.tint.unwrap_or(0.0).clamp(0.0, 1.0);
        let hue = self.tint_hue.unwrap_or(DEFAULT_TINT_HUE);
        for (colors, mode) in [
            (&mut theme.light, ThemeMode::Light),
            (&mut theme.dark, ThemeMode::Dark),
        ] {
            if let Some(accent) = self
                .accent
                .as_deref()
                .and_then(|accent| accent_color(accent, mode))
            {
                colors.accent = Some(accent);
                colors.on_accent = None;
            }
            if tint > 0.0 {
                let tinted = colors.resolve(mode).tinted(hue, tint);
                colors.background = Some(tinted.background);
                colors.foreground = Some(tinted.foreground);
            }
        }
    }

    pub fn pick_theme(&mut self) {
        self.radius = None;
    }
}

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

fn deserialize_roles<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, StylePatch>, D::Error> {
    use serde::de::Error as _;
    let mut roles = BTreeMap::<String, StylePatch>::deserialize(deserializer)?;
    roles.retain(|id, _| !crate::roles::is_retired(id));
    for (id, patch) in &roles {
        let Some(role) = crate::roles::find(id) else {
            return Err(D::Error::custom(unknown_role(id)));
        };
        if let Some(refusal) = role.knobs().refusal(patch) {
            return Err(D::Error::custom(format!(
                "`{id}` is a {}: {refusal}",
                role.kind.name()
            )));
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_accent: Option<Hsla>,
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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct InterfaceTheme {
    pub name: String,
    pub materials: Materials,
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

    pub fn apply(&self, theme: &mut Theme) {
        theme.colors = self.colors(theme.mode);
        self.materials.apply(theme);
        self.apply_roles(theme);
    }

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

pub fn parse_theme(source: &str) -> Result<InterfaceTheme, String> {
    let mut value =
        serde_json::from_str::<serde_json::Value>(source).map_err(|error| error.to_string())?;
    if !value.is_object() {
        return Err("a theme is a JSON object".to_owned());
    }
    light_glass_over_glass(&mut value);
    serde_json::from_value(value).map_err(|error| with_spelling_hint(error.to_string()))
}

fn light_glass_over_glass(theme: &mut serde_json::Value) {
    use serde_json::Value;
    let Some(materials) = theme.get_mut("materials").and_then(Value::as_object_mut) else {
        return;
    };
    let mut glass = match materials.get("glass") {
        Some(Value::String(name)) => {
            serde_json::Map::from_iter([("preset".to_owned(), Value::String(name.clone()))])
        }
        Some(Value::Object(glass)) => glass.clone(),
        _ => return,
    };
    if let Some(Value::Object(light)) = materials.get_mut("light-glass")
        && !light.contains_key("preset")
    {
        glass.extend(std::mem::take(light));
        *light = glass;
    }
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

const DEFAULT_SOURCE: (&str, &str) = ("default", include_str!("../themes/default.json"));

static SOURCES: OnceLock<&'static [(&'static str, &'static str)]> = OnceLock::new();
static BUILTINS: OnceLock<Vec<(&'static str, InterfaceTheme)>> = OnceLock::new();

pub fn register(themes: &'static [(&'static str, &'static str)]) {
    let fresh = SOURCES.set(themes).is_ok();
    assert!(
        !fresh || BUILTINS.get().is_none(),
        "built-in themes were read before the app registered its own"
    );
}

fn builtins_from(themes: &[(&'static str, &'static str)]) -> Vec<(&'static str, InterfaceTheme)> {
    let mut sources = vec![DEFAULT_SOURCE];
    for &(id, source) in themes {
        match sources.iter_mut().find(|(known, _)| *known == id) {
            Some(slot) => *slot = (id, source),
            None => sources.push((id, source)),
        }
    }
    sources
        .into_iter()
        .map(|(id, source)| {
            let theme = parse_theme(source)
                .unwrap_or_else(|error| panic!("built-in theme {id} does not parse: {error}"));
            (id, theme)
        })
        .collect()
}

pub fn builtin_themes() -> &'static [(&'static str, InterfaceTheme)] {
    BUILTINS.get_or_init(|| builtins_from(SOURCES.get().copied().unwrap_or_default()))
}

pub fn builtin_theme(id: &str) -> Option<&'static InterfaceTheme> {
    builtin_themes()
        .iter()
        .find(|(builtin, _)| *builtin == id.trim())
        .map(|(_, theme)| theme)
}

pub fn is_builtin_theme(id: &str) -> bool {
    builtin_theme(id).is_some()
}

pub fn default_theme() -> &'static InterfaceTheme {
    builtin_theme(DEFAULT_THEME).expect("the default theme is built in")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Colorize as _, Elevation};

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
    fn every_builtin_parses_with_its_id_and_a_name() {
        let ids = builtin_themes()
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        assert_eq!(ids, [DEFAULT_THEME]);
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
    fn a_control_radius_reaches_the_kit_and_built_ins_follow_the_radius() {
        let theme = parse_theme(r#"{"materials":{"radius":8,"control-radius":4}}"#).unwrap();
        assert_eq!(theme.materials.control_radius, Some(4.0));
        let mut kit = Theme::from(&*ThemeColor::for_mode(ThemeMode::Dark));
        theme.apply(&mut kit);
        assert_eq!(kit.control_radius, Some(px(4.0)));
        assert!(theme.to_json().contains("\"control-radius\": 4.0"));

        for (id, builtin) in builtin_themes() {
            assert_eq!(builtin.materials.control_radius, None, "{id}");
            assert!(!builtin.to_json().contains("control-radius"), "{id}");
        }
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
        assert_eq!(
            chrome.hover,
            kit.style(crate::SurfaceRole::ICON_BUTTON).hover
        );
        assert!(!kit.style(Kind::Button.role()).outline);
        assert!(kit.style(crate::SurfaceRole::BADGE).outline);
    }

    #[test]
    fn picked_outlines_go_only_where_the_theme_turns_them_on() {
        use crate::Kind;
        let theme = parse_theme(
            r#"{"materials":{"outline":1,"picked-outlines":true},"roles":{"popover":{"outline":true}}}"#,
        )
        .unwrap();
        let mut kit = Theme::default();
        theme.apply(&mut kit);
        assert!(kit.style(crate::SurfaceRole::POPOVER).outline);
        assert!(!kit.style(Kind::Button.role()).outline);
        assert!(!kit.style(crate::SurfaceRole::BADGE).outline);
        assert!(kit.style(crate::roles::BAR_RULE).outline);
        assert!(theme.to_json().contains("\"picked-outlines\": true"));
        assert!(!default_theme().to_json().contains("picked-outlines"));
    }

    #[test]
    fn picked_shadows_go_only_where_the_theme_casts_them() {
        use crate::{Kind, Shadow};
        let theme = parse_theme(
            r#"{"materials":{"picked-shadows":true},"roles":{"popover":{"shadow":"float"}}}"#,
        )
        .unwrap();
        let mut kit = Theme::default();
        theme.apply(&mut kit);
        assert_eq!(kit.style(crate::SurfaceRole::POPOVER).shadow, Shadow::Float);
        assert_eq!(kit.style(Kind::Button.role()).shadow, Shadow::None);
        assert_eq!(kit.style(crate::SurfaceRole::DIALOG).shadow, Shadow::None);
        assert_eq!(kit.style(crate::roles::WINDOW).shadow, Shadow::Float);
        assert!(theme.to_json().contains("\"picked-shadows\": true"));
        assert!(!default_theme().to_json().contains("picked-shadows"));
    }

    #[test]
    fn unknown_roles_are_errors_that_suggest_the_role() {
        let error = parse_theme(r#"{"roles":{"chromebutton":{}}}"#).unwrap_err();
        assert!(error.contains("did you mean `chrome.button`"), "{error}");
        let error = parse_theme(r#"{"roles":{"preview":{}}}"#).unwrap_err();
        assert!(error.contains("did you mean `settings.preview`"), "{error}");
        assert!(parse_theme(r#"{"roles":{"sparkle":{}}}"#).is_err());
        assert!(parse_theme(r#"{"roles":{"row":{"fill":"neon"}}}"#).is_err());
    }

    #[test]
    fn roles_take_only_what_their_kind_offers() {
        let error = parse_theme(r#"{"roles":{"settings.card":{"shape":"pill"}}}"#).unwrap_err();
        assert!(error.contains("`settings.card` is a card"), "{error}");
        assert!(error.contains("`shape` can't be \"pill\""), "{error}");
        let error = parse_theme(r#"{"roles":{"titlebar":{"hover":"tint"}}}"#).unwrap_err();
        assert!(error.contains("`hover` is fixed"), "{error}");
        assert!(parse_theme(r#"{"roles":{"settings.card":{"shape":"square"}}}"#).is_ok());
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
        let lit =
            parse_theme(r##"{"materials":{"glass":{"preset":"clear","light_color":"#ff0000"}}}"##)
                .unwrap();
        assert_eq!(
            lit.materials.glass.map(|glass| glass.light_color),
            Some(crate::parse_hex("#ff0000").unwrap())
        );
        assert!(parse_theme(r#"{"materials":{"glass":"stained"}}"#).is_err());
        assert!(parse_theme("[]").is_err());
    }

    #[test]
    fn light_mode_draws_the_light_glass_laid_over_the_glass() {
        let theme = parse_theme(
            r#"{"materials":{"glass":{"preset":"clear","blur":3},"light-glass":{"edge_shadow":0.7}}}"#,
        )
        .unwrap();
        let glass = theme.materials.glass.unwrap();
        let mut kit = Theme {
            mode: ThemeMode::Light,
            ..Theme::default()
        };
        theme.apply(&mut kit);
        assert_eq!(
            kit.glass,
            Some(GlassMaterial {
                edge_shadow: 0.7,
                ..glass
            })
        );
        kit.mode = ThemeMode::Dark;
        theme.apply(&mut kit);
        assert_eq!(kit.glass, Some(glass));
        assert_eq!(parse_theme(&theme.to_json()).unwrap(), theme);

        let preset =
            parse_theme(r#"{"materials":{"glass":"clear","light-glass":"frosted"}}"#).unwrap();
        assert_eq!(
            preset.materials.light_glass,
            GlassMaterial::preset("frosted")
        );
        assert!(!default_theme().to_json().contains("light-glass"));
    }

    #[test]
    fn overrides_compose_a_radius_accent_and_tint_over_the_theme() {
        let mut theme = default_theme().clone();
        theme.materials.font = Some("Inter".into());
        theme.roles.insert(
            "badge".to_owned(),
            StylePatch::EMPTY.shape(crate::Shape::Pill),
        );
        theme.dark.on_accent = Some(crate::parse_hex("#000000").unwrap());
        let roles = theme.roles.clone();
        let colors = theme.dark.clone();
        ThemeOverrides {
            radius: Some(14.0),
            accent: Some("green".to_owned()),
            tint: Some(1.0),
            tint_hue: Some(145.0),
        }
        .apply_to(&mut theme);
        assert_eq!(theme.roles, roles);
        assert_eq!(theme.materials.font.as_deref(), Some("Inter"));
        assert_eq!(theme.materials.outline, default_theme().materials.outline);
        assert_eq!(theme.materials.radius, 14.0);
        assert_eq!(theme.light.accent, accent_color("green", ThemeMode::Light));
        assert_eq!(theme.dark.accent, accent_color("green", ThemeMode::Dark));
        assert_eq!(theme.dark.on_accent, None);
        let tinted = colors.resolve(ThemeMode::Dark).tinted(145.0, 1.0);
        assert_eq!(theme.dark.background, Some(tinted.background));
        assert_eq!(theme.dark.foreground, Some(tinted.foreground));

        let mut plain = default_theme().clone();
        ThemeOverrides::default().apply_to(&mut plain);
        assert_eq!(&plain, default_theme());
        assert_eq!(
            accent_color("#ff8800", ThemeMode::Dark),
            crate::parse_hex("#ff8800").ok()
        );
        assert_eq!(accent_color("chartreuse", ThemeMode::Dark), None);
    }

    #[test]
    fn app_themes_join_the_builtins_and_can_replace_the_default() {
        let plain = r#"{"name":"Plain","materials":{"radius":3}}"#;
        let ids = |themes: &[(&'static str, InterfaceTheme)]| {
            themes.iter().map(|(id, _)| *id).collect::<Vec<_>>()
        };
        let added = builtins_from(&[("plain", plain)]);
        assert_eq!(ids(&added), ["default", "plain"]);
        assert_eq!(added[0].1, builtins_from(&[])[0].1);
        let replaced = builtins_from(&[("plain", plain), ("default", plain)]);
        assert_eq!(ids(&replaced), ["default", "plain"]);
        assert_eq!(replaced[0].1.materials.radius, 3.0);
    }

    #[test]
    fn picking_a_theme_keeps_the_accent_and_tint() {
        let mut overrides = ThemeOverrides {
            radius: Some(14.0),
            accent: Some("green".to_owned()),
            tint: Some(0.5),
            tint_hue: Some(145.0),
        };
        overrides.pick_theme();
        assert_eq!(
            overrides,
            ThemeOverrides {
                accent: Some("green".to_owned()),
                tint: Some(0.5),
                tint_hue: Some(145.0),
                ..ThemeOverrides::default()
            }
        );
    }

    #[test]
    fn every_named_accent_reads_at_any_tint() {
        let names = ACCENT_COLORS.map(|(name, ..)| name);
        assert_eq!(names, ACCENTS);
        for hue in (0..360).step_by(30) {
            for accent in ACCENTS {
                let mut theme = default_theme().clone();
                ThemeOverrides {
                    accent: Some(accent.to_owned()),
                    tint: Some(1.0),
                    tint_hue: Some(hue as f32),
                    ..ThemeOverrides::default()
                }
                .apply_to(&mut theme);
                assert_reads(&format!("{accent} at hue {hue}"), &theme);
            }
        }
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
