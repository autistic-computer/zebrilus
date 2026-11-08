use std::sync::OnceLock;

use ala::{App, GlassMaterial, Hsla, SharedString, Window, px};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    Colorize as _, Elevation, ShadowMaterial, Smoothing, Theme, ThemeColor, ThemeMode,
    foundation::{color::contrast_ratio, theme::DEFAULT_RADIUS, theme_color},
};

pub const DEFAULT_THEME: &str = "default";

pub const BUILTIN_THEMES: [&str; 3] = [DEFAULT_THEME, "macos", "yaru"];

pub const MAX_CORNER_RADIUS: f32 = 25.0;

pub const ACCENTS: [&str; 8] = [
    "blue", "purple", "pink", "red", "orange", "yellow", "green", "graphite",
];

const ACCENT_COLORS: [(&str, &str, &str); ACCENTS.len()] = [
    ("blue", "#0a64dd", "#3183ff"),
    ("purple", "#8549d1", "#a366f7"),
    ("pink", "#c02a82", "#e648a0"),
    ("red", "#cc252d", "#f44647"),
    ("orange", "#ad4f00", "#de6700"),
    ("yellow", "#876400", "#af8200"),
    ("green", "#007b2c", "#009f3b"),
    ("graphite", "#6a6a6a", "#868686"),
];

const TEXT_ON_GLASS: f32 = 4.5;

pub fn accent_color(value: &str, mode: ThemeMode) -> Option<Hsla> {
    let value = value.trim();
    match ACCENT_COLORS.iter().find(|(name, ..)| *name == value) {
        Some((_, light, dark)) => crate::parse_hex(if mode.is_dark() { dark } else { light }).ok(),
        None => crate::parse_hex(value).ok(),
    }
}

pub const DEFAULT_ACCENT: &str = "blue";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tint {
    pub light: Hsla,
    pub dark: Hsla,
}

impl Default for Tint {
    fn default() -> Self {
        Self {
            light: accent_color(DEFAULT_ACCENT, ThemeMode::Light).unwrap_or_default(),
            dark: accent_color(DEFAULT_ACCENT, ThemeMode::Dark).unwrap_or_default(),
        }
    }
}

impl Tint {
    pub fn named(value: &str) -> Option<Self> {
        Some(Self {
            light: accent_color(value, ThemeMode::Light)?,
            dark: accent_color(value, ThemeMode::Dark)?,
        })
    }

    pub fn color(self, mode: ThemeMode) -> Hsla {
        if mode.is_dark() {
            self.dark
        } else {
            self.light
        }
    }
}

impl Serialize for Tint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let pair = [
            ("light", crate::to_hex(self.light)),
            ("dark", crate::to_hex(self.dark)),
        ];
        serializer.collect_map(pair)
    }
}

impl<'de> Deserialize<'de> for Tint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Spelled {
            One(String),
            Pair { light: String, dark: String },
        }
        let unknown = |value: &str| D::Error::custom(format!("`{value}` is no tint"));
        match Spelled::deserialize(deserializer)? {
            Spelled::One(value) => Self::named(&value).ok_or_else(|| unknown(&value)),
            Spelled::Pair { light, dark } => Ok(Self {
                light: accent_color(&light, ThemeMode::Light).ok_or_else(|| unknown(&light))?,
                dark: accent_color(&dark, ThemeMode::Dark).ok_or_else(|| unknown(&dark))?,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BaseColors {
    #[serde(skip_serializing_if = "Option::is_none", with = "hex_color")]
    pub bg: Option<Hsla>,
    #[serde(skip_serializing_if = "Option::is_none", with = "hex_color")]
    pub fg: Option<Hsla>,
    #[serde(skip_serializing_if = "Option::is_none", with = "hex_color")]
    pub success: Option<Hsla>,
    #[serde(skip_serializing_if = "Option::is_none", with = "hex_color")]
    pub warning: Option<Hsla>,
    #[serde(skip_serializing_if = "Option::is_none", with = "hex_color")]
    pub danger: Option<Hsla>,
}

impl BaseColors {
    fn lay_on(&self, colors: &mut ThemeColor) {
        for (slot, color) in [
            (&mut colors.background, self.bg),
            (&mut colors.foreground, self.fg),
            (&mut colors.success, self.success),
            (&mut colors.warning, self.warning),
            (&mut colors.danger, self.danger),
        ] {
            if let Some(color) = color {
                *slot = color;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModeColors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<BaseColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dark: Option<BaseColors>,
}

impl ModeColors {
    pub fn get(&self, mode: ThemeMode) -> Option<&BaseColors> {
        if mode.is_dark() {
            self.dark.as_ref()
        } else {
            self.light.as_ref()
        }
    }

    pub fn set(&mut self, mode: ThemeMode, colors: BaseColors) {
        if mode.is_dark() {
            self.dark = Some(colors);
        } else {
            self.light = Some(colors);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.light.is_none() && self.dark.is_none()
    }
}

mod hex_color {
    use ala::Hsla;
    use serde::{Deserialize as _, Deserializer, Serializer, de::Error as _};

    #[allow(
        clippy::ref_option,
        reason = "serde's `with` hands the field over by reference"
    )]
    pub fn serialize<S: Serializer>(
        color: &Option<Hsla>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match color {
            Some(color) => serializer.serialize_str(&crate::to_hex(*color)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Hsla>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|value| crate::parse_hex(&value).map_err(D::Error::custom))
            .transpose()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Neutral {
    pub hue: f32,
    pub amount: f32,
}

impl Default for Neutral {
    fn default() -> Self {
        Self {
            hue: theme_color::NEUTRAL_HUE,
            amount: theme_color::NEUTRAL_AMOUNT,
        }
    }
}

impl Neutral {
    #[must_use]
    pub fn clamped(self) -> Self {
        let finite = |value: f32, fallback: f32| if value.is_finite() { value } else { fallback };
        let default = Self::default();
        Self {
            hue: finite(self.hue, default.hue).rem_euclid(360.0),
            amount: finite(self.amount, default.amount).clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControlRadius {
    Radius(f32),
    Full,
}

impl ControlRadius {
    pub fn pixels(self) -> ala::Pixels {
        match self {
            Self::Radius(radius) => px(radius),
            Self::Full => ala::FULL_CORNER_RADIUS,
        }
    }
}

impl Serialize for ControlRadius {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Radius(radius) => serializer.serialize_f32(*radius),
            Self::Full => serializer.serialize_str("full"),
        }
    }
}

impl<'de> Deserialize<'de> for ControlRadius {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::String(full) if full == "full" => Ok(Self::Full),
            serde_json::Value::Number(radius) => Ok(Self::Radius(clamp_radius(
                radius.as_f64().unwrap_or_default() as f32,
            ))),
            other => Err(D::Error::custom(format!(
                "control-radius is a number from 0 to 25 or \"full\", not {other}"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Glass {
    None,
    Liquid,
    Material(Box<GlassPair>),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct GlassPair {
    pub light: GlassMaterial,
    pub dark: GlassMaterial,
}

impl Glass {
    pub fn material(&self, mode: ThemeMode) -> Option<GlassMaterial> {
        match self {
            Self::None => None,
            Self::Liquid => Some(liquid(mode)),
            Self::Material(pair) => Some(if mode.is_dark() {
                pair.dark
            } else {
                pair.light
            }),
        }
    }
}

impl Serialize for Glass {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::None => serializer.serialize_str("none"),
            Self::Liquid => serializer.serialize_str("liquid"),
            Self::Material(pair) => pair.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Glass {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        use serde_json::Value;
        let over = |fields: Value, mode: ThemeMode| -> Result<GlassMaterial, D::Error> {
            let mut merged = serde_json::to_value(liquid(mode)).map_err(D::Error::custom)?;
            if let (Value::Object(base), Value::Object(fields)) = (&mut merged, fields) {
                base.extend(fields);
            }
            serde_json::from_value(merged).map_err(D::Error::custom)
        };
        match Value::deserialize(deserializer)? {
            Value::String(name) => match name.as_str() {
                "none" => Ok(Self::None),
                "liquid" => Ok(Self::Liquid),
                _ => Err(D::Error::custom(format!(
                    "glass is none, liquid or a material, not {name}"
                ))),
            },
            Value::Object(mut fields) => {
                let light = fields.remove("light");
                let dark = fields.remove("dark");
                let both = Value::Object(fields);
                let light = over(light.unwrap_or_else(|| both.clone()), ThemeMode::Light)?;
                let dark = over(dark.unwrap_or(both), ThemeMode::Dark)?;
                Ok(Self::Material(Box::new(GlassPair { light, dark })))
            }
            _ => Err(D::Error::custom("glass is none, liquid or a material")),
        }
    }
}

pub fn liquid(mode: ThemeMode) -> GlassMaterial {
    let dark = GlassMaterial {
        blur: px(20.0),
        bezel: px(3.0),
        refraction: px(4.0),
        dispersion: 0.015,
        tint: crate::parse_hex("#ffffff8c").unwrap_or_default(),
        saturation: 1.2,
        brightness: 0.07,
        contrast: 1.12,
        specular: 0.26,
        glint_width: px(0.85),
        light_angle: -std::f32::consts::FRAC_PI_2,
        light_directionality: 0.85,
        inactive_flattening: 0.85,
        fresnel: 0.045,
        edge_shadow: 0.4,
        edge_width: px(0.65),
        edge_spread: 0.0,
        ..GlassMaterial::regular()
    };
    if mode.is_dark() {
        return dark;
    }
    GlassMaterial {
        tint: crate::parse_hex("#ffffff99").unwrap_or_default(),
        saturation: 1.0,
        brightness: 0.0,
        contrast: 1.0,
        specular: 0.32,
        glint_width: px(0.65),
        fresnel: 0.025,
        edge_shadow: 0.14,
        edge_width: px(0.5),
        edge_spread: 0.3,
        ..dark
    }
}

pub fn tint_floor(fill: Hsla, text: Hsla) -> f32 {
    let fill = ala::Rgba::from(fill.opaque());
    let worst = if contrast_ratio(text, crate::parse_hex("#ffffff").unwrap_or_default())
        < contrast_ratio(text, crate::parse_hex("#000000").unwrap_or_default())
    {
        1.0
    } else {
        0.0
    };
    let over = |alpha: f32| {
        let mix = |channel: f32| channel * alpha + worst * (1.0 - alpha);
        Hsla::from(ala::Rgba {
            r: mix(fill.r),
            g: mix(fill.g),
            b: mix(fill.b),
            a: 1.0,
        })
    };
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    if contrast_ratio(text, over(high)) < TEXT_ON_GLASS {
        return high;
    }
    for _ in 0..24 {
        let middle = (low + high) / 2.0;
        if contrast_ratio(text, over(middle)) >= TEXT_ON_GLASS {
            high = middle;
        } else {
            low = middle;
        }
    }
    (high * 1000.0).ceil() / 1000.0
}

fn holding_text(material: GlassMaterial, fill: Hsla, text: Hsla) -> GlassMaterial {
    let floor = tint_floor(fill, text);
    GlassMaterial {
        tint: material.tint.alpha(material.tint.a.max(floor)),
        ..material
    }
}

pub fn corner_share(radius: f32) -> f32 {
    (clamp_radius(radius) / MAX_CORNER_RADIUS).sqrt()
}

pub fn corner_radius_at(share: f32) -> f32 {
    let share = if share.is_finite() {
        share.clamp(0.0, 1.0)
    } else {
        0.0
    };
    MAX_CORNER_RADIUS * share * share
}

fn clamp_radius(radius: f32) -> f32 {
    if radius.is_finite() {
        radius.clamp(0.0, MAX_CORNER_RADIUS)
    } else {
        DEFAULT_RADIUS
    }
}

fn deserialize_radius<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    f32::deserialize(deserializer).map(clamp_radius)
}

fn deserialize_smoothing<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    f32::deserialize(deserializer).map(|smoothing| Smoothing::clamped(smoothing).exponent())
}

fn deserialize_neutral<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Neutral, D::Error> {
    Neutral::deserialize(deserializer).map(Neutral::clamped)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeLock {
    #[default]
    Follow,
    Light,
    Dark,
}

impl ModeLock {
    pub fn locked(self) -> Option<ThemeMode> {
        match self {
            Self::Follow => None,
            Self::Light => Some(ThemeMode::Light),
            Self::Dark => Some(ThemeMode::Dark),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct InterfaceTheme {
    pub name: String,
    pub tint: Tint,
    #[serde(deserialize_with = "deserialize_neutral")]
    pub neutral: Neutral,
    #[serde(deserialize_with = "deserialize_radius")]
    pub radius: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_radius: Option<ControlRadius>,
    #[serde(deserialize_with = "deserialize_smoothing")]
    pub smoothing: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glass: Option<Glass>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face: Option<SharedString>,
    pub mode: ModeLock,
    #[serde(skip_serializing_if = "ModeColors::is_empty")]
    pub colors: ModeColors,
}

impl Default for InterfaceTheme {
    fn default() -> Self {
        Self {
            name: String::new(),
            tint: Tint::default(),
            neutral: Neutral::default(),
            radius: DEFAULT_RADIUS,
            control_radius: None,
            smoothing: Smoothing::THEME.exponent(),
            glass: None,
            face: None,
            mode: ModeLock::Follow,
            colors: ModeColors::default(),
        }
    }
}

impl InterfaceTheme {
    pub fn colors(&self, mode: ThemeMode) -> ThemeColor {
        let neutral = self.neutral.clamped();
        let mut base = *ThemeColor::for_mode(mode);
        if let Some(brought) = self.colors.get(mode) {
            brought.lay_on(&mut base);
        }
        let mut colors = base.neutral(neutral.hue, neutral.amount);
        colors.accent = self.tint.color(mode);
        colors.fitted()
    }

    pub fn glass(&self, mode: ThemeMode) -> Option<GlassMaterial> {
        self.glass.as_ref().map_or_else(
            || Glass::Liquid.material(mode),
            |glass| glass.material(mode),
        )
    }

    pub fn apply(&self, theme: &mut Theme) {
        let mode = theme.mode;
        theme.set_colors(self.colors(mode));
        theme.radius = px(clamp_radius(self.radius));
        theme.control_radius = self.control_radius.map(ControlRadius::pixels);
        theme.smoothing = Smoothing::clamped(self.smoothing);
        theme.outline = px(1.0);
        theme.row_inset = px(6.0);
        theme.contact_shadow = ShadowMaterial::CONTACT;
        theme.float_shadow = ShadowMaterial::FLOAT;
        theme.font_family = self
            .face
            .clone()
            .unwrap_or_else(|| ala::Font::default().family);
        let popover = theme.surface(Elevation::Popover).fill;
        let text = theme.foreground;
        theme.glass = self
            .glass(mode)
            .map(|material| holding_text(material, popover, text));
    }

    pub fn activate(&self, mode: Option<ThemeMode>, window: Option<&mut Window>, cx: &mut App) {
        let mode = self.mode.locked().or(mode).unwrap_or_else(|| {
            window
                .as_ref()
                .map_or_else(|| cx.window_appearance(), |window| window.appearance())
                .into()
        });
        Theme::change(mode, window, cx);
        self.apply(Theme::global_mut(cx));
        cx.refresh_windows();
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
    serde_json::from_value(value).map_err(|error| error.to_string())
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

const BUILTIN_SOURCES: [&str; BUILTIN_THEMES.len()] = [
    include_str!("../../themes/default.json"),
    include_str!("../../themes/macos.json"),
    include_str!("../../themes/yaru.json"),
];

static BUILTINS: OnceLock<Vec<(&'static str, InterfaceTheme)>> = OnceLock::new();

pub fn builtin_themes() -> &'static [(&'static str, InterfaceTheme)] {
    BUILTINS.get_or_init(|| {
        BUILTIN_THEMES
            .into_iter()
            .zip(BUILTIN_SOURCES)
            .map(|(id, source)| {
                let theme = parse_theme(source)
                    .unwrap_or_else(|error| panic!("built-in theme {id} does not parse: {error}"));
                (id, theme)
            })
            .collect()
    })
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

pub fn platform_theme() -> &'static InterfaceTheme {
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        builtin_theme("macos").expect("the macOS theme is built in")
    } else {
        default_theme()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Controls {
    Rounded,
    Pill,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controls: Option<Controls>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glass: Option<Glass>,
}

impl ThemeOverrides {
    pub fn apply_to(&self, theme: &mut InterfaceTheme) {
        if let Some(radius) = self.radius {
            theme.radius = clamp_radius(radius);
        }
        if let Some(tint) = self.accent.as_deref().and_then(Tint::named) {
            theme.tint = tint;
        }
        if let Some(amount) = self.tint {
            theme.neutral.amount = amount;
        }
        if let Some(hue) = self.tint_hue {
            theme.neutral.hue = hue;
        }
        theme.neutral = theme.neutral.clamped();
        match self.controls {
            Some(Controls::Rounded) => theme.control_radius = None,
            Some(Controls::Pill) => theme.control_radius = Some(ControlRadius::Full),
            None => {}
        }
        if let Some(glass) = &self.glass {
            theme.glass = Some(glass.clone());
        }
    }

    pub fn from_theme(theme: &InterfaceTheme) -> Self {
        let accent = ACCENTS
            .iter()
            .find(|name| Tint::named(name) == Some(theme.tint))
            .map_or_else(
                || crate::to_hex(theme.tint.light),
                |name| (*name).to_owned(),
            );
        let neutral = theme.neutral.clamped();
        Self {
            radius: Some(clamp_radius(theme.radius)),
            accent: Some(accent),
            tint: Some(neutral.amount),
            tint_hue: Some(neutral.hue),
            controls: Some(match theme.control_radius {
                Some(ControlRadius::Full) => Controls::Pill,
                _ => Controls::Rounded,
            }),
            glass: Some(theme.glass.clone().unwrap_or(Glass::Liquid)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub mode: Option<String>,
    #[serde(flatten)]
    pub overrides: ThemeOverrides,
    pub contrast: f32,
    pub reduce_transparency: bool,
    pub ui_font_family: String,
    pub mono_font_family: String,
    pub zoom: f32,
    pub motion: crate::foundation::motion::Motion,
    pub density: crate::Density,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            mode: None,
            overrides: ThemeOverrides::default(),
            contrast: 1.0,
            reduce_transparency: false,
            ui_font_family: String::new(),
            mono_font_family: String::new(),
            zoom: 1.0,
            motion: crate::foundation::motion::Motion::Default,
            density: crate::Density::Regular,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppearanceChange {
    Mode(Option<ThemeMode>),
    Accent(Option<String>),
    TintHue(f32),
    Tint(f32),
    Radius(f32),
    Controls(Controls),
    Glass(Glass),
    Import(Box<InterfaceTheme>),
    ResetLook,
    Contrast(f32),
    ReduceTransparency(bool),
    UiFont(String),
    MonoFont(String),
    Zoom(f32),
    Motion(crate::foundation::motion::Motion),
    Density(crate::Density),
}

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

    pub fn shown_theme(&self) -> InterfaceTheme {
        let mut theme = platform_theme().clone();
        self.overrides.apply_to(&mut theme);
        theme
    }

    pub fn corner_radius(&self) -> f32 {
        self.shown_theme().radius
    }

    pub fn neutral(&self) -> Neutral {
        self.shown_theme().neutral
    }

    pub fn controls(&self) -> Controls {
        match self.shown_theme().control_radius {
            Some(ControlRadius::Full) => Controls::Pill,
            _ => Controls::Rounded,
        }
    }

    pub fn glass(&self) -> Glass {
        self.shown_theme().glass.unwrap_or(Glass::Liquid)
    }

    pub fn accent(&self) -> String {
        self.overrides.accent.clone().unwrap_or_else(|| {
            ThemeOverrides::from_theme(platform_theme())
                .accent
                .unwrap_or_default()
        })
    }

    pub fn ui_font(&self) -> &str {
        if self.ui_font_family.is_empty() {
            platform_theme().face.as_deref().unwrap_or("")
        } else {
            &self.ui_font_family
        }
    }

    pub fn mono_font(&self) -> &str {
        &self.mono_font_family
    }

    pub fn import(&mut self, theme: &InterfaceTheme) {
        self.overrides = ThemeOverrides::from_theme(theme);
        self.ui_font_family = theme.face.as_deref().unwrap_or("").to_owned();
    }

    pub fn reset_look(&mut self) {
        self.overrides = ThemeOverrides::default();
        self.ui_font_family.clear();
        self.mono_font_family.clear();
    }

    pub fn change(&mut self, change: AppearanceChange) {
        match change {
            AppearanceChange::Mode(mode) => self.set_theme_mode(mode),
            AppearanceChange::Accent(accent) => self.overrides.accent = accent,
            AppearanceChange::TintHue(hue) => self.overrides.tint_hue = Some(hue),
            AppearanceChange::Tint(tint) => self.overrides.tint = Some(tint),
            AppearanceChange::Radius(radius) => {
                self.overrides.radius = Some(clamp_radius(radius));
            }
            AppearanceChange::Controls(controls) => self.overrides.controls = Some(controls),
            AppearanceChange::Glass(glass) => self.overrides.glass = Some(glass),
            AppearanceChange::Import(theme) => self.import(&theme),
            AppearanceChange::ResetLook => self.reset_look(),
            AppearanceChange::Contrast(contrast) => self.contrast = contrast,
            AppearanceChange::ReduceTransparency(reduce) => self.reduce_transparency = reduce,
            AppearanceChange::UiFont(family) => self.ui_font_family = family,
            AppearanceChange::MonoFont(family) => self.mono_font_family = family,
            AppearanceChange::Zoom(zoom) => self.zoom = zoom,
            AppearanceChange::Motion(motion) => self.motion = motion,
            AppearanceChange::Density(density) => self.density = density,
        }
    }

    pub fn apply(&self, mut window: Option<&mut Window>, cx: &mut App) {
        cx.set_reduce_motion(self.motion.reduced());
        cx.set_animation_speed(self.motion.speed());
        self.shown_theme()
            .activate(self.theme_mode(), window.as_deref_mut(), cx);
        let font = self.ui_font();
        let family = if !font.is_empty() && cx.text_system().has_font(font) {
            SharedString::from(font.to_owned())
        } else {
            ala::Font::default().family
        };
        let mono = self.mono_font();
        let mono = if !mono.is_empty() && cx.text_system().has_font(mono) {
            SharedString::from(mono.to_owned())
        } else {
            crate::foundation::platform_mono_font_family()
        };
        let theme = Theme::global_mut(cx);
        theme.font_family = family;
        theme.mono_font_family = mono;
        theme.set_contrast(self.contrast);
        if self.reduce_transparency {
            theme.glass = None;
        }
        self.density.set();
        cx.set_global(crate::UiZoom(self.zoom));
        if let Some(window) = window {
            window.set_zoom(self.zoom);
        } else {
            let zoom = self.zoom;
            cx.defer(move |cx| {
                for window in cx.windows() {
                    window.update(cx, |_, window, _| window.set_zoom(zoom)).ok();
                }
            });
        }
        cx.refresh_windows();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Emphasis, PaintState, Tone};

    fn kit(theme: &InterfaceTheme, mode: ThemeMode) -> Theme {
        let mut kit = Theme::from(&*ThemeColor::for_mode(mode));
        kit.mode = mode;
        theme.apply(&mut kit);
        kit
    }

    #[test]
    fn the_default_knobs_draw_the_color_table() {
        for (mode, table) in [
            (
                ThemeMode::Light,
                [
                    "#f6f6f3", "#e9e9e6", "#dcdcda", "#1c1c1a", "#575756", "#555553", "#90908e",
                    "#0a64dd", "#ebebeb", "#1c1c1a", "#007b3e", "#a75400", "#cc2737",
                ],
            ),
            (
                ThemeMode::Dark,
                [
                    "#1a1a19", "#212120", "#292928", "#e9e7e3", "#908f8c", "#a4a3a0", "#4d4c4b",
                    "#3183ff", "#010101", "#e9e7e3", "#3fbf74", "#f0a14a", "#f2555f",
                ],
            ),
        ] {
            let kit = kit(default_theme(), mode);
            let primary = kit.paint(Tone::Primary, Emphasis::Solid);
            let drawn = [
                kit.background,
                kit.surface(Elevation::Panel).fill,
                kit.surface(Elevation::Popover).fill,
                kit.foreground,
                kit.foreground.muted(),
                kit.paint(Tone::Neutral, Emphasis::Ghost)
                    .muted_at(PaintState::Rest),
                kit.foreground.faint(),
                kit.accent,
                primary.text,
                primary.fill,
                kit.success,
                kit.warning,
                kit.danger,
            ]
            .map(crate::to_hex);
            assert_eq!(drawn, table, "{mode:?}");
        }
    }

    fn assert_reads(id: &str, theme: &InterfaceTheme) {
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let kit = kit(theme, mode);
            for surface in [
                kit.background,
                kit.surface(Elevation::Popover).fill.opaque(),
            ] {
                let ratio = contrast_ratio(kit.foreground, surface);
                assert!(ratio >= 7.0, "{id} {mode:?} foreground is {ratio:.2}:1");
                for (name, color) in [
                    ("accent", kit.accent),
                    ("success", kit.success),
                    ("warning", kit.warning),
                    ("danger", kit.danger),
                ] {
                    let ratio = contrast_ratio(color, surface);
                    assert!(ratio >= 3.0, "{id} {mode:?} {name} is {ratio:.2}:1");
                }
            }
            for (name, color) in [
                ("muted text", kit.foreground.muted()),
                ("accent", kit.accent),
                ("success", kit.success),
                ("warning", kit.warning),
                ("danger", kit.danger),
            ] {
                let ratio = contrast_ratio(color, kit.background);
                assert!(ratio >= 4.5, "{id} {mode:?} {name} as text is {ratio:.2}:1");
            }
            for tone in [
                Tone::Primary,
                Tone::Accent,
                Tone::Success,
                Tone::Warning,
                Tone::Danger,
            ] {
                let solid = kit.paint(tone, Emphasis::Solid);
                let ratio = contrast_ratio(solid.text, solid.fill);
                let floor = if tone == Tone::Accent { 3.0 } else { 4.5 };
                assert!(
                    ratio >= floor,
                    "{id} {mode:?} ink on a solid {tone:?} fill is {ratio:.2}:1"
                );
            }
        }
    }

    #[test]
    fn every_accent_reads_in_every_theme_at_any_neutral() {
        assert_eq!(ACCENT_COLORS.map(|(name, ..)| name), ACCENTS);
        for (id, builtin) in builtin_themes() {
            assert!(!builtin.name.is_empty(), "{id} has no name");
            assert_reads(id, builtin);
            for amount in [0.0, 0.25, 0.5, 1.0] {
                for hue in (0..360).step_by(30) {
                    for accent in ACCENTS {
                        let mut theme = builtin.clone();
                        ThemeOverrides {
                            accent: Some(accent.to_owned()),
                            tint: Some(amount),
                            tint_hue: Some(hue as f32),
                            ..ThemeOverrides::default()
                        }
                        .apply_to(&mut theme);
                        assert_reads(&format!("{id} {accent} at {amount} of hue {hue}"), &theme);
                    }
                }
            }
        }
    }

    #[test]
    fn glass_holds_text_at_four_and_a_half_over_any_backdrop() {
        for (glass, mode, floor, alpha) in [
            (Glass::Liquid, ThemeMode::Light, 0.596, 0.6),
            (Glass::Liquid, ThemeMode::Dark, 0.705, 0.705),
        ] {
            let theme = InterfaceTheme {
                glass: Some(glass.clone()),
                ..InterfaceTheme::default()
            };
            let kit = kit(&theme, mode);
            let popover = kit.surface(Elevation::Popover).fill;
            assert_eq!(
                tint_floor(popover, kit.foreground),
                floor,
                "{glass:?} {mode:?}"
            );
            let drawn = kit.glass.expect("glass draws").tint.a;
            assert_eq!(
                (drawn * 1000.0).round() / 1000.0,
                alpha,
                "{glass:?} {mode:?}"
            );
        }
        let none = InterfaceTheme {
            glass: Some(Glass::None),
            ..InterfaceTheme::default()
        };
        assert_eq!(kit(&none, ThemeMode::Dark).glass, None);
    }

    #[test]
    fn knobs_clamp_where_a_theme_is_parsed_and_where_overrides_apply() {
        let theme = parse_theme(
            r#"{"radius":40,"control-radius":-3,"smoothing":9,"neutral":{"hue":-30,"amount":2}}"#,
        )
        .unwrap();
        assert_eq!(theme.radius, 25.0);
        assert_eq!(theme.control_radius, Some(ControlRadius::Radius(0.0)));
        assert_eq!(theme.smoothing, 5.0);
        assert_eq!(
            theme.neutral,
            Neutral {
                hue: 330.0,
                amount: 1.0
            }
        );
        let pills = parse_theme(r#"{"control-radius":"full","smoothing":1}"#).unwrap();
        assert_eq!(pills.control_radius, Some(ControlRadius::Full));
        assert_eq!(pills.smoothing, 2.0);
        assert!(pills.to_json().contains("\"control-radius\": \"full\""));
        assert!(parse_theme(r#"{"control-radius":"round"}"#).is_err());

        let mut theme = InterfaceTheme::default();
        ThemeOverrides {
            radius: Some(99.0),
            tint: Some(-1.0),
            tint_hue: Some(400.0),
            ..ThemeOverrides::default()
        }
        .apply_to(&mut theme);
        assert_eq!(theme.radius, 25.0);
        assert_eq!(
            theme.neutral,
            Neutral {
                hue: 40.0,
                amount: 0.0
            }
        );
        let mut appearance = Appearance::default();
        appearance.change(AppearanceChange::Radius(-4.0));
        assert_eq!(appearance.corner_radius(), 0.0);
    }

    #[test]
    fn corner_radius_runs_a_square_curve_from_square_to_25() {
        assert_eq!(
            [0.0, 0.25, 0.5, 1.0, 1.5].map(corner_radius_at),
            [0.0, 1.5625, 6.25, 25.0, 25.0]
        );
        assert_eq!((corner_share(12.0) * 100.0).round(), 69.0);
        assert_eq!(corner_share(40.0), 1.0);
    }

    #[test]
    fn presets_are_knobs_with_names() {
        let macos = builtin_theme("macos").unwrap();
        assert_eq!(macos.tint, Tint::named("blue").unwrap());
        assert_eq!(
            (macos.control_radius, macos.smoothing, macos.glass.clone()),
            (Some(ControlRadius::Full), 4.0, None)
        );
        let yaru = builtin_theme("yaru").unwrap();
        assert_eq!(yaru.tint, Tint::named("orange").unwrap());
        assert_eq!(
            (yaru.radius, yaru.smoothing, yaru.glass.clone()),
            (8.0, 2.0, Some(Glass::None))
        );
        for (id, theme) in builtin_themes() {
            assert_eq!(&parse_theme(&theme.to_json()).unwrap(), theme, "{id}");
        }
    }

    #[test]
    fn appearance_reads_and_writes_the_same_keys() {
        assert_eq!(
            serde_json::from_str::<Appearance>("{}").unwrap(),
            Appearance::default()
        );
        let source = r#"{"mode":"dark","theme":"macos","radius":12,"accent":"green","tint":0.25,"tint-hue":145,"controls":"pill","glass":"none","contrast":1.5,"reduce_transparency":true,"ui_font_family":"Inter","mono_font_family":"Lilex","zoom":1.25,"motion":"fast","density":"comfortable","sidebar":true}"#;
        let appearance = serde_json::from_str::<Appearance>(source).unwrap();
        assert_eq!(
            appearance,
            Appearance {
                mode: Some("dark".into()),
                overrides: ThemeOverrides {
                    radius: Some(12.0),
                    accent: Some("green".into()),
                    tint: Some(0.25),
                    tint_hue: Some(145.0),
                    controls: Some(Controls::Pill),
                    glass: Some(Glass::None),
                },
                contrast: 1.5,
                reduce_transparency: true,
                ui_font_family: "Inter".into(),
                mono_font_family: "Lilex".into(),
                zoom: 1.25,
                motion: crate::foundation::motion::Motion::Fast,
                density: crate::Density::Comfortable,
            }
        );
        let written = serde_json::to_value(&appearance).unwrap();
        assert_eq!(
            serde_json::from_value::<Appearance>(written).unwrap(),
            appearance
        );
    }

    #[test]
    fn a_persons_knobs_sit_over_the_platform_look_and_a_theme_file_sets_them_all() {
        let mut appearance = Appearance {
            ui_font_family: "Inter".into(),
            mono_font_family: "Berkeley Mono".into(),
            ..Appearance::default()
        };
        assert_eq!(appearance.shown_theme(), *platform_theme());
        appearance.change(AppearanceChange::Radius(7.0));
        appearance.change(AppearanceChange::Accent(Some("pink".into())));
        appearance.change(AppearanceChange::Controls(Controls::Pill));
        appearance.change(AppearanceChange::Glass(Glass::None));
        let shown = appearance.shown_theme();
        assert_eq!(
            (shown.radius, shown.tint, shown.control_radius, shown.glass),
            (
                7.0,
                Tint::named("pink").unwrap(),
                Some(ControlRadius::Full),
                Some(Glass::None)
            )
        );
        assert_eq!(appearance.accent(), "pink");
        assert_eq!(appearance.ui_font(), "Inter");

        let file = parse_theme(
            r#"{"name":"Mono","tint":"orange","face":"Lilex","radius":2,"glass":"liquid"}"#,
        )
        .unwrap();
        appearance.change(AppearanceChange::Import(Box::new(file)));
        let shown = appearance.shown_theme();
        assert_eq!(
            (shown.radius, shown.tint, shown.control_radius, shown.glass),
            (
                2.0,
                Tint::named("orange").unwrap(),
                None,
                Some(Glass::Liquid)
            )
        );
        assert_eq!(appearance.accent(), "orange");
        assert_eq!(appearance.controls(), Controls::Rounded);
        assert_eq!(appearance.ui_font(), "Lilex");
        assert_eq!(appearance.mono_font(), "Berkeley Mono");

        appearance.change(AppearanceChange::ResetLook);
        assert_eq!(appearance.overrides, ThemeOverrides::default());
        assert_eq!(appearance.mono_font(), "");
        assert_eq!(appearance.shown_theme(), *platform_theme());
    }

    #[ala::test]
    fn applying_an_appearance_shows_its_theme_contrast_transparency_zoom_and_density(
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
            reduce_transparency: true,
            zoom: 1.25,
            motion: crate::foundation::motion::Motion::Off,
            density: crate::Density::Compact,
            ..Appearance::default()
        };
        cx.update(|cx| {
            crate::init(cx);
            assert!(Theme::global(cx).glass.is_some());
            appearance.apply(None, cx);
            let theme = Theme::global(cx);
            assert_eq!(theme.mode, ThemeMode::Dark);
            assert_eq!(theme.radius, px(5.0));
            assert_eq!(Some(theme.accent), accent_color("green", ThemeMode::Dark));
            assert_eq!(theme.contrast(), 1.5);
            assert_eq!(theme.glass, None);
            assert_eq!(crate::UiZoom::get(cx), 1.25);
            assert!(cx.reduce_motion());
            assert_eq!(
                crate::Size::LADDER.map(|size| size.control_px()),
                [20.0, 24.0, 24.0, 32.0, 40.0, 44.0]
            );
            crate::Density::Regular.set();
            Theme::global_mut(cx).set_contrast(1.0);
        });
    }

    #[ala::test]
    fn applying_without_a_window_from_a_window_callback_zooms_that_window(
        cx: &mut ala::TestAppContext,
    ) {
        cx.update(crate::init);
        let window = cx.add_empty_window();
        let appearance = Appearance {
            zoom: 1.5,
            ..Appearance::default()
        };
        window.update(|window, cx| {
            appearance.apply(None, cx);
            assert_eq!(window.zoom(), 1.0);
        });
        window.run_until_parked();
        window.update(|window, _| assert_eq!(window.zoom(), 1.5));
    }

    #[test]
    fn layers_merge_objects_and_replace_everything_else() {
        let theme = parse_layers(&[
            r#"{"name":"Base","neutral":{"hue":200,"amount":0.5},"radius":4}"#,
            r#"{"neutral":{"amount":0.25},"face":"Inter"}"#,
        ])
        .unwrap();
        assert_eq!(theme.name, "Base");
        assert_eq!(
            theme.neutral,
            Neutral {
                hue: 200.0,
                amount: 0.25
            }
        );
        assert_eq!(theme.radius, 4.0);
        assert_eq!(theme.face.as_deref(), Some("Inter"));
        assert!(parse_layers(&[r#"{"name":"Base"}"#, "[]"]).is_err());
        assert_eq!(parse_layers(&[]).unwrap(), InterfaceTheme::default());
    }

    #[test]
    fn brought_colors_draw_exactly_and_the_rest_moves_until_it_reads() {
        let source = r##"{"tint":"#458588","colors":{"dark":{"bg":"#282828","fg":"#a89984","danger":"#cc241d"}}}"##;
        let theme = parse_theme(source).unwrap();
        assert_eq!(parse_theme(&theme.to_json()).unwrap(), theme);
        let dark = kit(&theme, ThemeMode::Dark);
        let drawn = [
            dark.background,
            dark.foreground,
            dark.foreground.muted(),
            dark.accent,
            dark.success,
            dark.danger,
        ]
        .map(crate::to_hex);
        assert_eq!(
            drawn,
            [
                "#282828", "#a89984", "#9a8c79", "#58989b", "#3fbf74", "#f95546"
            ]
        );
        for (color, from) in [(dark.accent, "#458588"), (dark.danger, "#cc241d")] {
            let ratio = contrast_ratio(color, dark.background);
            assert!(
                (4.5..4.52).contains(&ratio),
                "{} is {ratio:.3}:1",
                crate::to_hex(color)
            );
            let turn =
                crate::to_oklch(color).2 - crate::to_oklch(crate::parse_hex(from).unwrap()).2;
            assert!(turn.abs() < 1.0, "{from} turned {turn:.2} degrees");
        }
        assert!(contrast_ratio(dark.foreground.muted(), dark.background) >= 4.5);
        let tinted = parse_theme(r##"{"tint":"#458588"}"##).unwrap();
        assert_eq!(
            kit(&theme, ThemeMode::Light).colors,
            kit(&tinted, ThemeMode::Light).colors
        );
    }
}
