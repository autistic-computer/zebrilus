use std::collections::BTreeMap;

use ala::{App, GlassMaterial, px};
use penna::foundation::interface_theme::{
    self, Appearance, ControlRadius, Glass, GlassPair, InterfaceTheme, ModeLock, Neutral,
    ThemeOverrides, Tint,
};
use penna::{Smoothing, Theme, ThemeMode, UiZoom};

use crate::catalog::{Control, catalog};

pub const UI_FONT: &str = "Inter Variable";

#[derive(Clone, Debug, PartialEq)]
pub struct Knobs {
    pub theme: Option<ThemeMode>,
    pub interface: &'static str,
    pub look: InterfaceTheme,
    pub accent: Option<String>,
    pub contrast: f32,
    pub tint_hue: Option<f32>,
    pub tint: Option<f32>,
    pub reduce_transparency: bool,
    pub zoom: f32,
    pub motion: bool,
    pub speed: f32,
    pub backdrop: Backdrop,
    pub app: BTreeMap<&'static str, f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backdrop {
    #[default]
    Plain,
    Code,
    Color,
}

type Knob<T> = (&'static str, fn(&T) -> f32, fn(&mut T, f32));

pub const GLASS_KNOBS: &[Knob<GlassMaterial>] = &[
    ("glass-blur", |m| m.blur.as_f32(), |m, v| m.blur = px(v)),
    ("glass-tint", |m| m.tint.a, |m, v| m.tint.a = v),
    (
        "glass-refraction",
        |m| m.refraction.as_f32(),
        |m, v| m.refraction = px(v),
    ),
    ("glass-bezel", |m| m.bezel.as_f32(), |m, v| m.bezel = px(v)),
    (
        "glass-dispersion",
        |m| m.dispersion,
        |m, v| m.dispersion = v,
    ),
    (
        "glass-saturation",
        |m| m.saturation,
        |m, v| m.saturation = v,
    ),
    (
        "glass-brightness",
        |m| m.brightness,
        |m, v| m.brightness = v,
    ),
    ("glass-contrast", |m| m.contrast, |m, v| m.contrast = v),
    ("glass-specular", |m| m.specular, |m, v| m.specular = v),
    (
        "glass-glint-width",
        |m| m.glint_width.as_f32(),
        |m, v| m.glint_width = px(v),
    ),
    (
        "glass-light",
        |m| m.light_angle.to_degrees(),
        |m, v| m.light_angle = v.to_radians(),
    ),
    (
        "glass-light-directionality",
        |m| m.light_directionality,
        |m, v| m.light_directionality = v,
    ),
    ("glass-fresnel", |m| m.fresnel, |m, v| m.fresnel = v),
    ("glass-edge", |m| m.edge_shadow, |m, v| m.edge_shadow = v),
    (
        "glass-edge-width",
        |m| m.edge_width.as_f32(),
        |m, v| m.edge_width = px(v),
    ),
    (
        "glass-edge-spread",
        |m| m.edge_spread,
        |m, v| m.edge_spread = v,
    ),
    ("glass-noise", |m| m.noise, |m, v| m.noise = v),
    (
        "glass-inactive-flattening",
        |m| m.inactive_flattening,
        |m, v| m.inactive_flattening = v,
    ),
];

impl Default for Knobs {
    fn default() -> Self {
        Self::from_theme(
            interface_theme::DEFAULT_THEME,
            interface_theme::default_theme(),
        )
    }
}

impl Knobs {
    fn from_theme(id: &'static str, theme: &InterfaceTheme) -> Self {
        Self {
            theme: None,
            interface: id,
            look: theme.clone(),
            accent: None,
            contrast: 1.0,
            tint_hue: None,
            tint: None,
            reduce_transparency: false,
            zoom: 1.0,
            motion: true,
            speed: 1.0,
            backdrop: Backdrop::Plain,
            app: catalog()
                .knobs
                .iter()
                .map(|knob| (knob.name, knob.control.value()))
                .collect(),
        }
    }

    pub fn value(&self, name: &str) -> f32 {
        self.app
            .get(name)
            .copied()
            .unwrap_or_else(|| panic!("the catalog has no knob named {name}"))
    }

    pub fn flag(&self, name: &str) -> bool {
        self.value(name) != 0.0
    }

    pub fn parse(query: &str) -> Result<Self, String> {
        let pairs = query
            .trim_start_matches('?')
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| {
                pair.split_once('=')
                    .ok_or_else(|| format!("expected key=value, not {pair}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut knobs = match pairs.iter().find(|(key, _)| *key == "interface-theme") {
            Some((_, value)) => {
                let (id, theme) = interface_theme::builtin_themes()
                    .iter()
                    .find(|(id, _)| id == value)
                    .ok_or_else(|| {
                        format!(
                            "interface-theme must be one of {}, not {value}",
                            interface_theme::builtin_themes()
                                .iter()
                                .map(|(id, _)| *id)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })?;
                Self::from_theme(id, theme)
            }
            None => Self::default(),
        };
        if let Some((_, value)) = pairs.iter().find(|(key, _)| *key == "glass") {
            knobs.look.glass = match *value {
                "" | "theme" => knobs.look.glass,
                "none" | "off" => Some(Glass::None),
                "liquid" => None,
                name => {
                    return Err(format!("glass is theme, none or liquid, not {name}"));
                }
            };
        }
        let mut glass = Vec::new();
        for (key, value) in pairs {
            match key {
                "interface-theme" | "glass" => {}
                "theme" => {
                    knobs.theme = match value {
                        "light" => Some(ThemeMode::Light),
                        "dark" => Some(ThemeMode::Dark),
                        "system" => None,
                        _ => {
                            return Err(format!(
                                "theme must be light, dark or system, not {value}"
                            ));
                        }
                    }
                }
                "name" => knobs.look.name = decode(value),
                "tint-color" => {
                    let value = decode(value);
                    knobs.look.tint = Tint::named(&value).ok_or_else(|| {
                        format!("tint-color is ink, an accent or a hex color, not {value}")
                    })?;
                }
                "light-tint-color" | "dark-tint-color" => {
                    let color = penna::parse_hex(&decode(value))
                        .map_err(|error| format!("{key} must be a hex color: {error}"))?;
                    let (mut light, mut dark) = match knobs.look.tint {
                        Tint::Color { light, dark } => (light, dark),
                        Tint::Ink => (color, color),
                    };
                    if key == "light-tint-color" {
                        light = color;
                    } else {
                        dark = color;
                    }
                    knobs.look.tint = Tint::Color { light, dark };
                }
                "neutral-hue" => knobs.look.neutral.hue = number(key, value)?,
                "neutral-amount" => knobs.look.neutral.amount = number(key, value)?,
                "radius" => knobs.look.radius = number(key, value)?.clamp(0.0, 25.0),
                "control-radius" => {
                    knobs.look.control_radius = Some(match value {
                        "full" => ControlRadius::Full,
                        value => ControlRadius::Radius(number(key, value)?.clamp(0.0, 25.0)),
                    });
                }
                "smoothing" => {
                    knobs.look.smoothing = Smoothing::clamped(number(key, value)?).exponent();
                }
                "face" => {
                    let family = decode(value);
                    knobs.look.face = (!family.is_empty()).then(|| family.into());
                }
                "lock" => {
                    knobs.look.mode = match value {
                        "follow" => ModeLock::Follow,
                        "light" => ModeLock::Light,
                        "dark" => ModeLock::Dark,
                        _ => {
                            return Err(format!("lock must be follow, light or dark, not {value}"));
                        }
                    }
                }
                "accent" => {
                    let accent = decode(value);
                    knobs.accent = (!accent.is_empty()).then_some(accent);
                }
                "contrast" => knobs.contrast = number(key, value)?,
                "tint-hue" => knobs.tint_hue = Some(number(key, value)?),
                "tint" => knobs.tint = Some(number(key, value)?),
                "reduce-transparency" => knobs.reduce_transparency = flag(value),
                "zoom" => knobs.zoom = number(key, value)?,
                "motion" => knobs.motion = flag(value),
                "speed" => knobs.speed = number(key, value)?,
                "backdrop" => {
                    knobs.backdrop = match value {
                        "plain" => Backdrop::Plain,
                        "code" => Backdrop::Code,
                        "color" => Backdrop::Color,
                        _ => {
                            return Err(format!(
                                "backdrop must be plain, code or color, not {value}"
                            ));
                        }
                    }
                }
                _ => {
                    if let Some(knob) = catalog().knobs.iter().find(|knob| knob.name == key) {
                        let value = match knob.control {
                            Control::Check(_) => f32::from(u8::from(flag(value))),
                            Control::Range { .. } => number(key, value)?,
                        };
                        knobs.app.insert(knob.name, value);
                    } else if let Some((_, _, set)) =
                        GLASS_KNOBS.iter().find(|(name, ..)| *name == key)
                    {
                        glass.push((*set, number(key, value)?));
                    }
                }
            }
        }
        knobs.look.neutral = knobs.look.neutral.clamped();
        if knobs.look.control_radius == Some(ControlRadius::Radius(knobs.look.radius)) {
            knobs.look.control_radius = None;
        }
        if !glass.is_empty() {
            let pair = [ThemeMode::Light, ThemeMode::Dark].map(|mode| {
                let mut material = knobs
                    .look
                    .glass(mode)
                    .unwrap_or_else(|| interface_theme::liquid(mode));
                for (set, value) in &glass {
                    set(&mut material, *value);
                }
                material
            });
            knobs.look.glass = Some(Glass::Material(Box::new(GlassPair {
                light: pair[0],
                dark: pair[1],
            })));
        }
        Ok(knobs)
    }

    pub fn interface_theme(&self) -> InterfaceTheme {
        self.look.clone()
    }

    pub fn overrides(&self) -> ThemeOverrides {
        ThemeOverrides {
            accent: self.accent.clone(),
            tint: self.tint,
            tint_hue: self.tint_hue,
            ..ThemeOverrides::default()
        }
    }

    pub fn shown_theme(&self) -> InterfaceTheme {
        let mut theme = self.interface_theme();
        self.overrides().apply_to(&mut theme);
        theme
    }

    pub fn neutral(&self) -> Neutral {
        self.shown_theme().neutral
    }

    pub fn appearance(&self) -> Appearance {
        let mut appearance = Appearance {
            contrast: self.contrast,
            reduce_transparency: self.reduce_transparency,
            zoom: self.zoom,
            animations: self.motion,
            animation_speed: self.speed,
            ..Appearance::default()
        };
        appearance.import(&self.shown_theme());
        appearance.set_theme_mode(self.theme);
        appearance
    }

    pub fn apply(&self, cx: &mut App) {
        self.shown_theme().activate(self.theme, None, cx);
        cx.set_reduce_motion(!self.motion);
        cx.set_animation_speed(self.speed);
        let face_loads =
            (self.look.face.as_ref()).is_some_and(|family| cx.text_system().has_font(family));
        let theme = Theme::global_mut(cx);
        if !face_loads {
            theme.font_family = UI_FONT.into();
        }
        theme.set_contrast(self.contrast);
        if self.reduce_transparency {
            theme.glass = None;
        }
        cx.set_global(UiZoom(self.zoom));
        (catalog().apply)(self, cx);
        let zoom = self.zoom;
        for window in cx.windows() {
            window
                .update(cx, |_, window, _| {
                    window.set_zoom(zoom);
                    window.refresh();
                })
                .ok();
        }
    }
}

pub fn glass_knobs(material: &GlassMaterial) -> serde_json::Map<String, serde_json::Value> {
    GLASS_KNOBS
        .iter()
        .map(|(name, get, _)| ((*name).to_owned(), serde_json::json!(get(material))))
        .collect()
}

pub fn theme_knobs(theme: &InterfaceTheme) -> serde_json::Map<String, serde_json::Value> {
    let mut knobs = serde_json::Map::new();
    let mut set = |name: &str, value: serde_json::Value| {
        knobs.insert(name.to_owned(), value);
    };
    set("name", serde_json::json!(theme.name));
    match theme.tint {
        Tint::Ink => set("tint-color", serde_json::json!("ink")),
        Tint::Color { light, dark } => {
            set("light-tint-color", serde_json::json!(penna::to_hex(light)));
            set("dark-tint-color", serde_json::json!(penna::to_hex(dark)));
        }
    }
    set("neutral-hue", serde_json::json!(theme.neutral.hue));
    set("neutral-amount", serde_json::json!(theme.neutral.amount));
    set("radius", serde_json::json!(theme.radius));
    set(
        "control-radius",
        match theme.control_radius {
            None => serde_json::json!(theme.radius),
            Some(ControlRadius::Radius(radius)) => serde_json::json!(radius),
            Some(ControlRadius::Full) => serde_json::json!("full"),
        },
    );
    set("smoothing", serde_json::json!(theme.smoothing));
    set(
        "glass",
        serde_json::json!(match &theme.glass {
            Some(Glass::None) => "none",
            None | Some(Glass::Liquid | Glass::Material(_)) => "liquid",
        }),
    );
    if let Some(Glass::Material(pair)) = &theme.glass {
        for (name, value) in glass_knobs(&pair.dark) {
            set(&name, value);
        }
    }
    set(
        "face",
        serde_json::json!(theme.face.as_ref().map_or("", |face| face.as_ref())),
    );
    set(
        "lock",
        serde_json::json!(match theme.mode {
            ModeLock::Follow => "follow",
            ModeLock::Light => "light",
            ModeLock::Dark => "dark",
        }),
    );
    knobs
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => decoded.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        decoded.push(byte);
                        index += 2;
                    }
                    Err(_) => decoded.push(b'%'),
                }
            }
            byte => decoded.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn flag(value: &str) -> bool {
    !matches!(value, "0" | "off" | "false")
}

fn number(key: &str, value: &str) -> Result<f32, String> {
    value
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("{key} must be a number, not {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(knobs: &serde_json::Map<String, serde_json::Value>) -> String {
        knobs
            .iter()
            .map(|(name, value)| {
                let value = value.as_str().map_or_else(
                    || value.to_string(),
                    |text| text.replace(' ', "+").replace('#', "%23"),
                );
                format!("{name}={value}")
            })
            .collect::<Vec<_>>()
            .join("&")
    }

    #[test]
    fn parses_known_keys_and_ignores_the_rest() {
        let knobs = Knobs::parse(
            "?theme=dark&radius=12&zoom=1.25&story=buttons&interface-theme=default&motion=0&roles=%7B%7D&outline=0.5",
        )
        .unwrap();
        assert_eq!(knobs.theme, Some(ThemeMode::Dark));
        assert_eq!(knobs.look.radius, 12.0);
        assert_eq!(knobs.zoom, 1.25);
        assert_eq!(knobs.interface, "default");
        assert!(!knobs.motion);
        assert_eq!(Knobs::parse("app-knob=1").unwrap(), Knobs::default());
        assert!(Knobs::parse("radius=wide").is_err());
        assert!(Knobs::parse("interface-theme=sepia").is_err());
        assert!(Knobs::parse("glass=lava").is_err());
        let controls = |query| Knobs::parse(query).unwrap().look.control_radius;
        assert_eq!(
            controls("control-radius=4"),
            Some(ControlRadius::Radius(4.0))
        );
        assert_eq!(controls("control-radius=full"), Some(ControlRadius::Full));
        assert_eq!(controls("radius=8"), None);
        assert_eq!(controls("radius=8&control-radius=8"), None);
    }

    #[test]
    fn the_person_knobs_make_an_appearance() {
        let knobs = Knobs::parse(
            "theme=light&interface-theme=macos&contrast=1.5&tint=0.3&tint-hue=145&accent=pink&reduce-transparency=1&zoom=1.25&motion=0&speed=0.5",
        )
        .unwrap();
        let appearance = knobs.appearance();
        assert_eq!(appearance.theme_mode(), Some(ThemeMode::Light));
        assert_eq!(appearance.accent(), "pink");
        assert_eq!(
            appearance.neutral(),
            Neutral {
                hue: 145.0,
                amount: 0.3
            }
        );
        assert_eq!(
            appearance.controls(),
            penna::foundation::interface_theme::Controls::Pill
        );
        assert_eq!(appearance.glass(), Glass::Liquid);
        assert!(appearance.reduce_transparency);
        assert_eq!(
            knobs.neutral(),
            Neutral {
                hue: 145.0,
                amount: 0.3
            }
        );
        assert_eq!(Knobs::default().appearance().theme_mode(), None);
    }

    #[test]
    fn glass_sliders_turn_the_glass_into_a_material_of_its_own() {
        let knobs = Knobs::parse("glass=liquid&glass-blur=30&glass-tint=0.8").unwrap();
        let Some(Glass::Material(pair)) = knobs.look.glass else {
            panic!("a slider makes a material");
        };
        assert_eq!(pair.dark.blur, px(30.0));
        assert_eq!(pair.light.tint.a, 0.8);
        assert_eq!(
            pair.dark.bezel,
            interface_theme::liquid(ThemeMode::Dark).bezel
        );
        assert_eq!(
            Knobs::parse("glass=none").unwrap().look.glass,
            Some(Glass::None)
        );
        assert_eq!(
            Knobs::parse("interface-theme=yaru&glass=liquid")
                .unwrap()
                .look
                .glass,
            None
        );
    }

    #[test]
    fn a_theme_survives_the_trip_through_its_knobs() {
        for (id, theme) in interface_theme::builtin_themes() {
            let parsed = Knobs::parse(&format!(
                "interface-theme=default&{}",
                query(&theme_knobs(theme))
            ))
            .unwrap();
            assert_eq!(parsed.interface_theme(), *theme, "{id}");
        }
        let tinted = Knobs::parse("tint-color=orange&dark-tint-color=%23112233").unwrap();
        assert_eq!(
            tinted.look.tint,
            Tint::Color {
                light: penna::parse_hex("#ad4f00").unwrap(),
                dark: penna::parse_hex("#112233").unwrap(),
            }
        );
        assert_eq!(decode("Fira%20Sans+Mono%"), "Fira Sans Mono%");
    }
}
