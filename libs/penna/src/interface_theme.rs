use std::{collections::BTreeMap, sync::OnceLock};

use ala::{App, GlassMaterial, Hsla, SharedString, Window, px};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{PrimaryStyle, ShadowMaterial, Smoothing, StylePatch, Theme, ThemeColor, ThemeMode};

pub const DEFAULT_THEME: &str = "default";

pub const BUILTIN_THEMES: [&str; 3] = [DEFAULT_THEME, "macos", "yaru"];

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
        theme.font_family = self
            .font
            .clone()
            .unwrap_or_else(|| ala::Font::default().family);
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

    pub fn activate(&self, mode: Option<ThemeMode>, window: Option<&mut Window>, cx: &mut App) {
        let mode = mode.unwrap_or_else(|| {
            window
                .as_ref()
                .map_or_else(|| cx.window_appearance(), |window| window.appearance())
                .into()
        });
        Theme::change(mode, window, cx);
        self.apply(Theme::global_mut(cx));
        cx.refresh_windows();
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
    parse_layers(&[source])
}

pub fn parse_layers(sources: &[&str]) -> Result<InterfaceTheme, String> {
    let mut value = serde_json::Value::Object(serde_json::Map::new());
    for source in sources {
        let layer =
            serde_json::from_str::<serde_json::Value>(source).map_err(|error| error.to_string())?;
        if !layer.is_object() {
            return Err("a theme is a JSON object".to_owned());
        }
        lay(&mut value, layer);
    }
    light_glass_over_glass(&mut value);
    serde_json::from_value(value).map_err(|error| with_spelling_hint(error.to_string()))
}

fn lay(base: &mut serde_json::Value, layer: serde_json::Value) {
    use serde_json::Value;
    match (base, layer) {
        (Value::Object(base), Value::Object(layer)) => {
            for (key, value) in layer {
                match base.get_mut(&key) {
                    Some(slot) => lay(slot, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, layer) => *base = layer,
    }
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

const BUILTIN_SOURCES: [&str; BUILTIN_THEMES.len()] = [
    include_str!("../themes/default.json"),
    include_str!("../themes/macos.json"),
    include_str!("../themes/yaru.json"),
];

static LAYERS: OnceLock<&'static [(&'static str, &'static str)]> = OnceLock::new();
static BUILTINS: OnceLock<Vec<(&'static str, InterfaceTheme)>> = OnceLock::new();

pub fn register(themes: &'static [(&'static str, &'static str)]) {
    assert!(
        LAYERS.get().is_some() || BUILTINS.get().is_none(),
        "the built-in themes were read before the app registered its theme layers; call \
         penna::register before penna::init and before reading any theme"
    );
    crate::roles::set_once(&LAYERS, themes, "penna::interface_theme::register");
}

fn builtins_from(themes: &[(&'static str, &'static str)]) -> Vec<(&'static str, InterfaceTheme)> {
    let mut stacks = BUILTIN_THEMES
        .into_iter()
        .zip(BUILTIN_SOURCES)
        .map(|(id, source)| (id, vec![source]))
        .collect::<Vec<_>>();
    for &(id, source) in themes {
        match stacks.iter_mut().find(|(known, _)| *known == id) {
            Some((_, stack)) => stack.push(source),
            None => stacks.push((id, vec![source])),
        }
    }
    stacks
        .into_iter()
        .map(|(id, stack)| {
            let theme = parse_layers(&stack).unwrap_or_else(|error| {
                let hint = if error.starts_with("unknown role") && !crate::roles::registered() {
                    "; an app's roles must be registered before its theme layers are read, so \
                     call penna::register before penna::init and before reading any theme"
                } else {
                    ""
                };
                let theme = if stack.len() == 1 && BUILTIN_THEMES.contains(&id) {
                    format!("built-in theme {id}")
                } else {
                    format!("theme {id} with the app's theme layer")
                };
                panic!("{theme} does not parse: {error}{hint}")
            });
            (id, theme)
        })
        .collect()
}

pub fn builtin_themes() -> &'static [(&'static str, InterfaceTheme)] {
    BUILTINS.get_or_init(|| builtins_from(LAYERS.get().copied().unwrap_or_default()))
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub mode: Option<String>,
    pub theme: Option<String>,
    #[serde(flatten)]
    pub overrides: ThemeOverrides,
    pub contrast: f32,
    pub ui_font_family: String,
    pub zoom: f32,
    pub animations: bool,
    pub animation_speed: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            mode: None,
            theme: None,
            overrides: ThemeOverrides::default(),
            contrast: 1.0,
            ui_font_family: String::new(),
            zoom: 1.0,
            animations: true,
            animation_speed: crate::motion::DEFAULT_ANIMATION_SPEED,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppearanceChange {
    Mode(Option<ThemeMode>),
    Theme(SharedString),
    Accent(Option<String>),
    TintHue(f32),
    Tint(f32),
    Radius(f32),
    Contrast(f32),
    UiFont(String),
    Zoom(f32),
    Animations(bool),
    AnimationSpeed(f32),
}

pub type SavedThemes = [(SharedString, InterfaceTheme)];

impl Appearance {
    pub fn theme_mode(&self) -> Option<ThemeMode> {
        match self.mode.as_deref().map(str::trim) {
            Some("light") => Some(ThemeMode::Light),
            Some("dark") => Some(ThemeMode::Dark),
            _ => None,
        }
    }

    pub fn set_theme_mode(&mut self, mode: Option<ThemeMode>) {
        let mode = match mode {
            None => "system",
            Some(ThemeMode::Light) => "light",
            Some(ThemeMode::Dark) => "dark",
        };
        self.mode = Some(mode.to_owned());
    }

    pub fn theme_id(&self, saved: &SavedThemes) -> &str {
        self.theme
            .as_deref()
            .filter(|id| knows_theme(id, saved))
            .unwrap_or(DEFAULT_THEME)
    }

    pub fn interface_theme<'a>(&self, saved: &'a SavedThemes) -> &'a InterfaceTheme {
        self.theme
            .as_deref()
            .and_then(|id| saved_theme(id, saved).or_else(|| builtin_theme(id)))
            .unwrap_or_else(|| default_theme())
    }

    pub fn shown_theme(&self, saved: &SavedThemes) -> InterfaceTheme {
        let mut theme = self.interface_theme(saved).clone();
        self.overrides.apply_to(&mut theme);
        theme
    }

    pub fn corner_radius(&self, saved: &SavedThemes) -> f32 {
        self.overrides
            .radius
            .unwrap_or_else(|| self.interface_theme(saved).materials.radius)
    }

    pub fn ui_font<'a>(&'a self, saved: &'a SavedThemes) -> &'a str {
        if self.ui_font_family.is_empty() {
            self.interface_theme(saved)
                .materials
                .font
                .as_deref()
                .unwrap_or("")
        } else {
            &self.ui_font_family
        }
    }

    pub fn pick_theme(&mut self, id: &str, saved: &SavedThemes) {
        self.theme = knows_theme(id, saved).then(|| id.to_owned());
        self.overrides.pick_theme();
        if self.interface_theme(saved).materials.font.is_some() {
            self.ui_font_family.clear();
        }
    }

    pub fn change(&mut self, change: AppearanceChange, saved: &SavedThemes) {
        match change {
            AppearanceChange::Mode(mode) => self.set_theme_mode(mode),
            AppearanceChange::Theme(id) => self.pick_theme(&id, saved),
            AppearanceChange::Accent(accent) => self.overrides.accent = accent,
            AppearanceChange::TintHue(hue) => self.overrides.tint_hue = Some(hue),
            AppearanceChange::Tint(tint) => self.overrides.tint = Some(tint),
            AppearanceChange::Radius(radius) => self.overrides.radius = Some(radius),
            AppearanceChange::Contrast(contrast) => self.contrast = contrast,
            AppearanceChange::UiFont(family) => self.ui_font_family = family,
            AppearanceChange::Zoom(zoom) => self.zoom = zoom,
            AppearanceChange::Animations(animations) => self.animations = animations,
            AppearanceChange::AnimationSpeed(speed) => self.animation_speed = speed,
        }
    }

    pub fn apply(&self, saved: &SavedThemes, mut window: Option<&mut Window>, cx: &mut App) {
        cx.set_reduce_motion(!self.animations);
        cx.set_animation_speed(self.animation_speed);
        self.shown_theme(saved)
            .activate(self.theme_mode(), window.as_deref_mut(), cx);
        let font = self.ui_font(saved);
        let family = if !font.is_empty() && cx.text_system().has_font(font) {
            SharedString::from(font.to_owned())
        } else {
            ala::Font::default().family
        };
        let theme = Theme::global_mut(cx);
        theme.font_family = family;
        theme.set_contrast(self.contrast);
        cx.set_global(crate::UiZoom(self.zoom));
        match window {
            Some(window) => window.set_zoom(self.zoom),
            None => {
                for window in cx.windows() {
                    window
                        .update(cx, |_, window, _| window.set_zoom(self.zoom))
                        .ok();
                }
            }
        }
        cx.refresh_windows();
    }
}

fn saved_theme<'a>(id: &str, saved: &'a SavedThemes) -> Option<&'a InterfaceTheme> {
    saved
        .iter()
        .find(|(saved, _)| saved.as_str() == id)
        .map(|(_, theme)| theme)
}

fn knows_theme(id: &str, saved: &SavedThemes) -> bool {
    is_builtin_theme(id) || saved_theme(id, saved).is_some()
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
        let mut kit = Theme::default();
        kit.mode = ThemeMode::Light;
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
    fn app_layers_lay_over_the_builtins_and_new_ids_add_themes() {
        use crate::{Fill, Shape};
        let layer = r#"{"materials":{"radius":3,"float":{"y":2}},"roles":{"button":{"fill":"tint"},"chrome.button":{"shape":"pill"}}}"#;
        let again = r#"{"roles":{"button":{"shape":"square"}}}"#;
        let plain = r#"{"name":"Plain","materials":{"radius":3}}"#;
        let ids = |themes: &[(&'static str, InterfaceTheme)]| {
            themes.iter().map(|(id, _)| *id).collect::<Vec<_>>()
        };
        let alone = builtins_from(&[]);
        let layered = builtins_from(&[("macos", layer), ("plain", plain), ("macos", again)]);
        assert_eq!(ids(&layered), ["default", "macos", "yaru", "plain"]);
        assert_eq!(layered[0], alone[0]);
        assert_eq!(layered[2], alone[2]);

        let (macos, before) = (&layered[1].1, &alone[1].1);
        assert_eq!(macos.name, before.name);
        assert_eq!(
            macos.materials,
            Materials {
                radius: 3.0,
                float: ShadowMaterial {
                    y: 2.0,
                    ..before.materials.float
                },
                ..before.materials.clone()
            }
        );
        assert_eq!((&macos.light, &macos.dark), (&before.light, &before.dark));
        assert_eq!(
            macos.roles["button"],
            before.roles["button"].fill(Fill::Tint).shape(Shape::Square)
        );
        assert_eq!(
            macos.roles["chrome.button"],
            before.roles["chrome.button"].shape(Shape::Pill)
        );
        let untouched = |roles: &BTreeMap<String, StylePatch>| {
            roles
                .iter()
                .filter(|(id, _)| !matches!(id.as_str(), "button" | "chrome.button"))
                .map(|(id, patch)| (id.clone(), *patch))
                .collect::<Vec<_>>()
        };
        assert_eq!(untouched(&macos.roles), untouched(&before.roles));

        let plain = &layered[3].1;
        assert_eq!(plain.name, "Plain");
        assert_eq!(
            plain.materials,
            Materials {
                radius: 3.0,
                ..Materials::default()
            }
        );
        assert!(plain.roles.is_empty());
        assert_eq!(plain.light, ThemeColors::default());
    }

    #[test]
    fn a_layer_naming_unregistered_roles_says_to_register_first() {
        let read = std::panic::catch_unwind(|| {
            builtins_from(&[("default", r#"{"roles":{"app.card":{}}}"#)])
        });
        let message = *read.unwrap_err().downcast::<String>().unwrap();
        assert!(message.contains("unknown role `app.card`"), "{message}");
        assert!(
            message.contains("call penna::register before penna::init"),
            "{message}"
        );
    }

    fn assert_shows(shown: &InterfaceTheme, mode: ThemeMode, cx: &ala::App) {
        let mut expected = Theme::from(&*ThemeColor::for_mode(mode));
        expected.mode = mode;
        shown.apply(&mut expected);
        let theme = Theme::global(cx);
        assert_eq!(theme.mode, mode);
        assert_eq!(theme.colors, expected.colors, "{mode:?}");
        assert_eq!(
            (
                theme.radius,
                theme.control_radius,
                theme.outline,
                theme.row_inset
            ),
            (
                expected.radius,
                expected.control_radius,
                expected.outline,
                expected.row_inset
            ),
            "{mode:?}"
        );
        assert_eq!(
            (
                theme.smoothing,
                theme.picked_smoothing,
                theme.picked_outlines
            ),
            (
                expected.smoothing,
                expected.picked_smoothing,
                expected.picked_outlines
            ),
            "{mode:?}"
        );
        assert_eq!(
            (theme.primary, theme.flat_highlights, theme.bright_surfaces),
            (
                expected.primary,
                expected.flat_highlights,
                expected.bright_surfaces
            ),
            "{mode:?}"
        );
        assert_eq!(
            (theme.highlight_strength, theme.glass, theme.picked_shadows),
            (
                expected.highlight_strength,
                expected.glass,
                expected.picked_shadows
            ),
            "{mode:?}"
        );
        assert_eq!(
            (theme.contact_shadow, theme.float_shadow),
            (expected.contact_shadow, expected.float_shadow),
            "{mode:?}"
        );
        assert_eq!(theme.styles, expected.styles, "{mode:?}");
    }

    #[ala::test]
    fn init_applies_the_default_theme(cx: &mut ala::TestAppContext) {
        cx.update(|cx| {
            crate::init(cx);
            let theme = Theme::global(cx);
            assert_eq!(theme.radius, px(default_theme().materials.radius));
            assert!(!theme.styles.is_empty());
            assert_shows(default_theme(), theme.mode, cx);
        });
    }

    #[ala::test]
    fn activating_in_either_mode_shows_only_that_theme(cx: &mut ala::TestAppContext) {
        let layered = parse_layers(&[
            default_theme().to_json().as_str(),
            r##"{"materials":{"radius":3,"control-radius":2,"outline":1,"picked-outlines":true,"glass":"clear","light-glass":"frosted","float":{"y":2}},"roles":{"button":{"fill":"tint"}},"light":{"accent":"#ff0000"},"dark":{"accent":"#00ff00"}}"##,
        ])
        .unwrap();
        let plain = parse_theme(r#"{"materials":{"radius":12}}"#).unwrap();
        cx.update(|cx| {
            crate::init(cx);
            for mode in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::Light] {
                layered.activate(Some(mode), None, cx);
                assert_shows(&layered, mode, cx);
            }
            plain.activate(Some(ThemeMode::Dark), None, cx);
            assert_shows(&plain, ThemeMode::Dark, cx);
            assert!(Theme::global(cx).styles.is_empty());
            layered.activate(Some(ThemeMode::Dark), None, cx);
            plain.activate(Some(ThemeMode::Light), None, cx);
            assert_shows(&plain, ThemeMode::Light, cx);
        });
    }

    #[ala::test]
    fn a_theme_without_a_font_shows_the_system_font(cx: &mut ala::TestAppContext) {
        let lilex = parse_theme(r#"{"materials":{"font":"Lilex"}}"#).unwrap();
        let plain = parse_theme(r#"{"materials":{"radius":12}}"#).unwrap();
        cx.update(|cx| {
            crate::init(cx);
            lilex.activate(Some(ThemeMode::Dark), None, cx);
            assert_eq!(Theme::global(cx).font_family, "Lilex");
            plain.activate(Some(ThemeMode::Dark), None, cx);
            assert_eq!(Theme::global(cx).font_family, ala::Font::default().family);
        });
        let mut kit = Theme::from(&*ThemeColor::for_mode(ThemeMode::Light));
        lilex.apply(&mut kit);
        assert_eq!(kit.font_family, "Lilex");
        plain.apply(&mut kit);
        assert_eq!(kit.font_family, ala::Font::default().family);
    }

    #[ala::test]
    fn activating_without_a_mode_follows_the_system(cx: &mut ala::TestAppContext) {
        cx.update(|cx| {
            crate::init(cx);
            let system = ThemeMode::from(cx.window_appearance());
            default_theme().activate(
                Some(match system {
                    ThemeMode::Light => ThemeMode::Dark,
                    ThemeMode::Dark => ThemeMode::Light,
                }),
                None,
                cx,
            );
            default_theme().activate(None, None, cx);
            assert_shows(default_theme(), system, cx);
        });
    }

    #[test]
    fn appearance_reads_and_writes_the_same_keys() {
        assert_eq!(
            serde_json::from_str::<Appearance>("{}").unwrap(),
            Appearance::default()
        );
        let source = r#"{"mode":"dark","theme":"macos","radius":12,"accent":"green","tint":0.25,"tint-hue":145,"contrast":1.5,"ui_font_family":"Inter","zoom":1.25,"animations":false,"animation_speed":0.5,"sidebar":true}"#;
        let appearance = serde_json::from_str::<Appearance>(source).unwrap();
        assert_eq!(
            appearance,
            Appearance {
                mode: Some("dark".into()),
                theme: Some("macos".into()),
                overrides: ThemeOverrides {
                    radius: Some(12.0),
                    accent: Some("green".into()),
                    tint: Some(0.25),
                    tint_hue: Some(145.0),
                },
                contrast: 1.5,
                ui_font_family: "Inter".into(),
                zoom: 1.25,
                animations: false,
                animation_speed: 0.5,
            }
        );
        let written = serde_json::to_value(&appearance).unwrap();
        let mut keys = written
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "accent",
                "animation_speed",
                "animations",
                "contrast",
                "mode",
                "radius",
                "theme",
                "tint",
                "tint-hue",
                "ui_font_family",
                "zoom"
            ]
        );
        assert_eq!(
            serde_json::from_value::<Appearance>(written).unwrap(),
            appearance
        );
    }

    #[test]
    fn appearance_changes_follow_the_theme_rules() {
        let font = parse_theme(r#"{"name":"Mono","materials":{"font":"Lilex","radius":2}}"#);
        let saved = [(SharedString::from("theme-1"), font.unwrap())];
        let mut appearance = Appearance {
            ui_font_family: "Inter".into(),
            ..Appearance::default()
        };
        assert_eq!(appearance.theme_mode(), None);
        assert_eq!(appearance.theme_id(&saved), DEFAULT_THEME);
        for (mode, spelled) in [
            (Some(ThemeMode::Dark), "dark"),
            (Some(ThemeMode::Light), "light"),
            (None, "system"),
        ] {
            appearance.change(AppearanceChange::Mode(mode), &saved);
            assert_eq!(appearance.mode.as_deref(), Some(spelled));
            assert_eq!(appearance.theme_mode(), mode);
        }
        appearance.change(AppearanceChange::Radius(7.0), &saved);
        appearance.change(AppearanceChange::Accent(Some("pink".into())), &saved);
        assert_eq!(appearance.corner_radius(&saved), 7.0);
        assert_eq!(appearance.ui_font(&saved), "Inter");

        appearance.change(AppearanceChange::Theme("macos".into()), &saved);
        assert_eq!(appearance.theme_id(&saved), "macos");
        assert_eq!(appearance.overrides.radius, None);
        assert_eq!(appearance.overrides.accent.as_deref(), Some("pink"));
        assert_eq!(appearance.ui_font(&saved), "Inter");

        appearance.change(AppearanceChange::Theme("theme-1".into()), &saved);
        assert_eq!(appearance.interface_theme(&saved).name, "Mono");
        assert_eq!(appearance.ui_font(&saved), "Lilex");
        assert_eq!(appearance.corner_radius(&saved), 2.0);
        assert_eq!(appearance.shown_theme(&saved).materials.radius, 2.0);

        appearance.change(AppearanceChange::Theme("vaporwave".into()), &saved);
        assert_eq!(appearance.theme, None);
        assert_eq!(appearance.theme_id(&saved), DEFAULT_THEME);
    }

    #[ala::test]
    fn applying_an_appearance_shows_its_theme_contrast_zoom_and_reduced_motion(
        cx: &mut ala::TestAppContext,
    ) {
        let appearance = Appearance {
            mode: Some("dark".into()),
            overrides: ThemeOverrides {
                radius: Some(5.0),
                accent: Some("green".into()),
                ..ThemeOverrides::default()
            },
            contrast: 1.5,
            zoom: 1.25,
            animations: false,
            ..Appearance::default()
        };
        cx.update(|cx| {
            crate::init(cx);
            appearance.apply(&[], None, cx);
            let theme = Theme::global(cx);
            assert_eq!(theme.mode, ThemeMode::Dark);
            assert_eq!(theme.radius, px(5.0));
            assert_eq!(Some(theme.accent), accent_color("green", ThemeMode::Dark));
            assert_eq!(theme.contrast(), 1.5);
            assert_eq!(theme.font_family, ala::Font::default().family);
            assert_eq!(crate::UiZoom::get(cx), 1.25);
            assert!(cx.reduce_motion());
        });
    }

    #[test]
    fn layers_merge_objects_and_replace_everything_else() {
        let theme = parse_layers(&[
            r#"{"name":"Base","materials":{"glass":{"preset":"clear","blur":3},"outline":1}}"#,
            r#"{"materials":{"glass":{"edge_shadow":0.7},"font":"Inter"}}"#,
        ])
        .unwrap();
        assert_eq!(theme.name, "Base");
        assert_eq!(theme.materials.outline, 1.0);
        assert_eq!(theme.materials.font.as_deref(), Some("Inter"));
        assert_eq!(
            theme.materials.glass,
            Some(GlassMaterial {
                blur: px(3.0),
                edge_shadow: 0.7,
                ..GlassMaterial::preset("clear").unwrap()
            })
        );
        let named = parse_layers(&[
            r#"{"materials":{"glass":{"preset":"clear","blur":3}}}"#,
            r#"{"materials":{"glass":"frosted"}}"#,
        ])
        .unwrap();
        assert_eq!(named.materials.glass, GlassMaterial::preset("frosted"));
        assert!(parse_layers(&[r#"{"name":"Base"}"#, "[]"]).is_err());
        assert!(parse_layers(&[r#"{"name":"Base"}"#, r#"{"roles":{"sparkle":{}}}"#]).is_err());
        assert_eq!(parse_layers(&[]).unwrap(), parse_theme("{}").unwrap());
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
