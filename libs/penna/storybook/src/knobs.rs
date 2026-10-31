use std::collections::BTreeMap;

use ala::{App, GlassMaterial, Hsla, px};
use penna::interface_theme::{
    self, Appearance, InterfaceTheme, Materials, ThemeColors, ThemeOverrides,
};
use penna::{
    Emphasis, PrimaryStyle, Shadow, Smoothing, Style, StylePatch, SurfaceRole, Theme, ThemeColor,
    ThemeMode, Tone, UiZoom,
};

use crate::catalog::{Control, catalog};

pub const UI_FONT: &str = "Inter Variable";

#[derive(Clone, Debug, PartialEq)]
pub struct Knobs {
    pub theme: Option<ThemeMode>,
    pub interface: &'static str,
    pub name: String,
    pub materials: Materials,
    pub roles: BTreeMap<String, StylePatch>,
    pub light: ThemeColors,
    pub dark: ThemeColors,
    pub contrast: f32,
    pub tint_hue: f32,
    pub tint: f32,
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

pub const MATERIAL_KNOBS: &[Knob<Materials>] = &[
    ("radius", |m| m.radius, |m, v| m.radius = v),
    ("outline", |m| m.outline, |m, v| m.outline = v),
    ("row-inset", |m| m.row_inset, |m, v| m.row_inset = v),
    (
        "highlight-strength",
        |m| m.highlight_strength,
        |m, v| m.highlight_strength = v,
    ),
    ("contact-y", |m| m.contact.y, |m, v| m.contact.y = v),
    (
        "contact-blur",
        |m| m.contact.blur,
        |m, v| m.contact.blur = v,
    ),
    (
        "contact-spread",
        |m| m.contact.spread,
        |m, v| m.contact.spread = v,
    ),
    (
        "contact-strength",
        |m| m.contact.strength,
        |m, v| m.contact.strength = v,
    ),
    ("float-y", |m| m.float.y, |m, v| m.float.y = v),
    ("float-blur", |m| m.float.blur, |m, v| m.float.blur = v),
    (
        "float-spread",
        |m| m.float.spread,
        |m, v| m.float.spread = v,
    ),
    (
        "float-strength",
        |m| m.float.strength,
        |m, v| m.float.strength = v,
    ),
];

type ColorKnob = (
    &'static str,
    fn(&ThemeColors) -> Option<Hsla>,
    fn(&mut ThemeColors, Hsla),
);

pub const COLOR_KNOBS: &[ColorKnob] = &[
    (
        "background",
        |c| c.background,
        |c, v| c.background = Some(v),
    ),
    (
        "foreground",
        |c| c.foreground,
        |c, v| c.foreground = Some(v),
    ),
    ("accent", |c| c.accent, |c, v| c.accent = Some(v)),
    ("success", |c| c.success, |c, v| c.success = Some(v)),
    ("warning", |c| c.warning, |c, v| c.warning = Some(v)),
    ("danger", |c| c.danger, |c, v| c.danger = Some(v)),
    ("on-accent", |c| c.on_accent, |c, v| c.on_accent = Some(v)),
    (
        "shadow-color",
        |c| c.shadow_color,
        |c, v| c.shadow_color = Some(v),
    ),
];

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
            name: theme.name.clone(),
            materials: theme.materials.clone(),
            roles: theme.roles.clone(),
            light: theme.light.clone(),
            dark: theme.dark.clone(),
            contrast: 1.0,
            tint_hue: interface_theme::DEFAULT_TINT_HUE,
            tint: 0.0,
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
            if !matches!(*value, "" | "theme") {
                knobs.materials.light_glass = None;
            }
            knobs.materials.glass = match *value {
                "" | "theme" => knobs.materials.glass,
                "off" => None,
                name => Some(
                    GlassMaterial::preset(name)
                        .ok_or_else(|| format!("no glass preset named {name}"))?,
                ),
            };
        }
        let mut control_radius = None;
        for (key, value) in pairs {
            match key {
                "interface-theme" | "glass" => {}
                "control-radius" => control_radius = Some(number(key, value)?),
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
                "name" => knobs.name = decode(value),
                "roles" => knobs.roles = parse_roles(&decode(value))?,
                "primary" => {
                    knobs.materials.primary = match value {
                        "foreground" => PrimaryStyle::Foreground,
                        "accent" => PrimaryStyle::Accent,
                        _ => {
                            return Err(format!(
                                "primary must be foreground or accent, not {value}"
                            ));
                        }
                    }
                }
                "outlines" => {
                    knobs.materials.picked_outlines = match value {
                        "built-in" => false,
                        "picked" => true,
                        _ => {
                            return Err(format!(
                                "outlines must be built-in or picked, not {value}"
                            ));
                        }
                    }
                }
                "smoothing" => {
                    knobs.materials.smoothing = Smoothing::try_from(number(key, value)?)?;
                }
                "smoothed" => {
                    knobs.materials.picked_smoothing = match value {
                        "global" => false,
                        "picked" => true,
                        _ => {
                            return Err(format!("smoothed must be global or picked, not {value}"));
                        }
                    }
                }
                "shadows" => {
                    knobs.materials.picked_shadows = match value {
                        "built-in" => false,
                        "picked" => true,
                        _ => {
                            return Err(format!("shadows must be built-in or picked, not {value}"));
                        }
                    }
                }
                "highlights" => {
                    knobs.materials.flat_highlights = match value {
                        "lift" => false,
                        "flat" => true,
                        _ => {
                            return Err(format!("highlights must be lift or flat, not {value}"));
                        }
                    }
                }
                "surfaces" => {
                    knobs.materials.bright_surfaces = match value {
                        "darker" => false,
                        "brighter" => true,
                        _ => {
                            return Err(format!(
                                "surfaces must be darker or brighter, not {value}"
                            ));
                        }
                    }
                }
                "font" => {
                    let family = decode(value);
                    knobs.materials.font = (!family.is_empty()).then(|| family.into());
                }
                "contrast" => knobs.contrast = number(key, value)?,
                "tint-hue" => knobs.tint_hue = number(key, value)?,
                "tint" => knobs.tint = number(key, value)?,
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
                        MATERIAL_KNOBS.iter().find(|(name, ..)| *name == key)
                    {
                        set(&mut knobs.materials, number(key, value)?);
                    } else if let Some((_, _, set)) =
                        GLASS_KNOBS.iter().find(|(name, ..)| *name == key)
                    {
                        let value = number(key, value)?;
                        if let Some(material) = knobs.materials.glass.as_mut() {
                            set(material, value);
                        }
                    } else if let Some((_, _, set)) = key
                        .strip_prefix("light-")
                        .and_then(|name| GLASS_KNOBS.iter().find(|(glass, ..)| *glass == name))
                    {
                        let value = number(key, value)?;
                        if let Some(material) = knobs.materials.light_glass.as_mut() {
                            set(material, value);
                        }
                    } else if let Some((colors, mode, name)) = key
                        .strip_prefix("light-")
                        .map(|name| (&mut knobs.light, ThemeMode::Light, name))
                        .or_else(|| {
                            key.strip_prefix("dark-")
                                .map(|name| (&mut knobs.dark, ThemeMode::Dark, name))
                        })
                        && let Some((_, get, set)) =
                            COLOR_KNOBS.iter().find(|(color, ..)| *color == name)
                    {
                        let previous = get(colors).or_else(|| {
                            (name == "shadow-color")
                                .then(|| ThemeColor::for_mode(mode).shadow_color)
                        });
                        set(colors, color(key, value, previous)?);
                    }
                }
            }
        }
        if let Some(radius) = control_radius {
            knobs.materials.control_radius = (radius != knobs.materials.radius).then_some(radius);
        }
        Ok(knobs)
    }

    pub fn interface_theme(&self) -> InterfaceTheme {
        InterfaceTheme {
            name: self.name.clone(),
            materials: self.materials.clone(),
            roles: self.roles.clone(),
            light: self.light.clone(),
            dark: self.dark.clone(),
        }
    }

    pub fn appearance(&self) -> Appearance {
        let mut appearance = Appearance {
            theme: Some(self.interface.to_owned()),
            overrides: ThemeOverrides {
                tint: Some(self.tint),
                tint_hue: Some(self.tint_hue),
                ..ThemeOverrides::default()
            },
            contrast: self.contrast,
            zoom: self.zoom,
            animations: self.motion,
            animation_speed: self.speed,
            ..Appearance::default()
        };
        appearance.set_theme_mode(self.theme);
        appearance
    }

    pub fn apply(&self, cx: &mut App) {
        self.interface_theme().activate(self.theme, None, cx);
        cx.set_reduce_motion(!self.motion);
        cx.set_animation_speed(self.speed);
        let font_loads =
            (self.materials.font.as_ref()).is_some_and(|family| cx.text_system().has_font(family));
        let theme = Theme::global_mut(cx);
        theme.colors = theme.colors.tinted(self.tint_hue, self.tint);
        if !font_loads {
            theme.font_family = UI_FONT.into();
        }
        theme.set_contrast(self.contrast);
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

pub fn material_knobs(materials: &Materials) -> serde_json::Map<String, serde_json::Value> {
    let mut knobs = MATERIAL_KNOBS
        .iter()
        .map(|(name, get, _)| ((*name).to_owned(), serde_json::json!(get(materials))))
        .collect::<serde_json::Map<_, _>>();
    knobs.insert(
        "control-radius".to_owned(),
        serde_json::json!(materials.control_radius.unwrap_or(materials.radius)),
    );
    knobs.insert("primary".to_owned(), serde_json::json!(materials.primary));
    knobs.insert(
        "smoothing".to_owned(),
        serde_json::json!(materials.smoothing.exponent()),
    );
    knobs.insert(
        "smoothed".to_owned(),
        serde_json::json!(if materials.picked_smoothing {
            "picked"
        } else {
            "global"
        }),
    );
    knobs.insert(
        "outlines".to_owned(),
        serde_json::json!(if materials.picked_outlines {
            "picked"
        } else {
            "built-in"
        }),
    );
    knobs.insert(
        "shadows".to_owned(),
        serde_json::json!(if materials.picked_shadows {
            "picked"
        } else {
            "built-in"
        }),
    );
    knobs.insert(
        "highlights".to_owned(),
        serde_json::json!(if materials.flat_highlights {
            "flat"
        } else {
            "lift"
        }),
    );
    knobs.insert(
        "surfaces".to_owned(),
        serde_json::json!(if materials.bright_surfaces {
            "brighter"
        } else {
            "darker"
        }),
    );
    knobs.insert(
        "font".to_owned(),
        serde_json::json!(materials.font.as_ref().map_or("", |font| font.as_ref())),
    );
    match materials.glass {
        None => {
            knobs.insert("glass".to_owned(), serde_json::json!("off"));
        }
        Some(material) => {
            knobs.insert("glass".to_owned(), serde_json::json!("regular"));
            for (name, get, _) in GLASS_KNOBS {
                knobs.insert((*name).to_owned(), serde_json::json!(get(&material)));
            }
        }
    }
    knobs
}

pub fn theme_knobs(theme: &InterfaceTheme) -> serde_json::Map<String, serde_json::Value> {
    let mut knobs = material_knobs(&theme.materials);
    knobs.insert("name".to_owned(), serde_json::json!(theme.name));
    knobs.insert("roles".to_owned(), serde_json::json!(theme.roles));
    for (mode, prefix) in [(ThemeMode::Light, "light"), (ThemeMode::Dark, "dark")] {
        for (name, value) in resolved_colors(theme, mode) {
            knobs.insert(format!("{prefix}-{name}"), serde_json::json!(value));
        }
    }
    knobs
}

fn resolved_colors(theme: &InterfaceTheme, mode: ThemeMode) -> Vec<(&'static str, String)> {
    let colors = theme.colors(mode);
    let ink = accent_ink(colors);
    let rgb = |color: Hsla| penna::to_hex(color.alpha(1.0));
    vec![
        ("background", rgb(colors.background)),
        ("foreground", rgb(colors.foreground)),
        ("accent", rgb(colors.accent)),
        ("success", rgb(colors.success)),
        ("warning", rgb(colors.warning)),
        ("danger", rgb(colors.danger)),
        ("on-accent", rgb(ink)),
        ("shadow-color", rgb(colors.shadow_color)),
    ]
}

fn parse_roles(json: &str) -> Result<BTreeMap<String, StylePatch>, String> {
    let roles = serde_json::from_str::<serde_json::Value>(json)
        .map_err(|error| format!("roles must be JSON: {error}"))?;
    interface_theme::parse_theme(&serde_json::json!({ "roles": roles }).to_string())
        .map(|theme| theme.roles)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Origin {
    pub role: Option<SurfaceRole>,
    pub themed: bool,
}

pub fn material_origin(prop: &str, role: SurfaceRole, materials: &Materials) -> String {
    match prop {
        "smoothing" if materials.picked_smoothing && role.picks_smoothing() => {
            "picked smoothing".to_owned()
        }
        "smoothing" => "global smoothing".to_owned(),
        prop => format!("picked {prop}s"),
    }
}

pub const PROPS: [&str; 8] = [
    "fill",
    "outline",
    "shadow",
    "shape",
    "smoothing",
    "hover",
    "selected",
    "ink",
];

pub fn inherited(
    role: SurfaceRole,
    roles: &BTreeMap<String, StylePatch>,
    materials: &Materials,
) -> (Style, [Origin; 8]) {
    let knobs = role.knobs();
    let mut style = Style {
        smoothing: materials.smoothing,
        ..role.kind.base()
    };
    let mut origins = [Origin {
        role: Some(role.kind.role()),
        themed: false,
    }; 8];
    origins[4] = Origin {
        role: None,
        themed: true,
    };
    let mut layer = |patch: StylePatch, origin: Origin| {
        patch.apply(&mut style);
        let named = [
            patch.fill.is_some(),
            patch.outline.is_some(),
            patch.shadow.is_some(),
            patch.shape.is_some(),
            patch.smoothing.is_some(),
            patch.hover.is_some(),
            patch.selected.is_some(),
            patch.ink.is_some(),
        ];
        for (from, named) in origins.iter_mut().zip(named) {
            if named {
                *from = origin;
            }
        }
    };
    let lineage = role.lineage().collect::<Vec<_>>();
    for node in lineage.iter().rev() {
        layer(
            node.patch,
            Origin {
                role: Some(*node),
                themed: false,
            },
        );
    }
    let picked = Origin {
        role: None,
        themed: true,
    };
    if materials.picked_smoothing && role.picks_smoothing() {
        layer(StylePatch::EMPTY.smoothing(Smoothing::CIRCLE), picked);
    }
    if materials.picked_outlines && role.picks_outline() {
        layer(StylePatch::EMPTY.outline(false), picked);
    }
    if materials.picked_shadows && role.picks_shadow() {
        layer(StylePatch::EMPTY.shadow(Shadow::None), picked);
    }
    for node in lineage.iter().rev().filter(|node| node.id != role.id) {
        if let Some(patch) = roles.get(node.id) {
            layer(
                knobs.admit(*patch),
                Origin {
                    role: Some(*node),
                    themed: true,
                },
            );
        }
    }
    (style, origins)
}

pub fn knobs_json(knobs: penna::Knobs) -> serde_json::Value {
    serde_json::json!({
        "fill": knobs.fill,
        "outline": knobs.outline,
        "shadow": knobs.shadow,
        "shape": knobs.shape,
        "smoothing": knobs.smoothing,
        "hover": knobs.hover,
        "selected": knobs.selected,
        "ink": knobs.ink,
    })
}

pub fn style_json(style: Style) -> serde_json::Value {
    serde_json::json!({
        "fill": style.fill,
        "outline": style.outline,
        "shadow": style.shadow,
        "shape": style.shape,
        "smoothing": style.smoothing,
        "hover": style.hover,
        "selected": style.selected,
        "ink": style.ink,
    })
}

fn accent_ink(colors: ThemeColor) -> Hsla {
    let mut theme = Theme::from(&colors);
    theme.primary = PrimaryStyle::Accent;
    theme.paint(Tone::Primary, Emphasis::Solid).text
}

fn color(key: &str, value: &str, previous: Option<Hsla>) -> Result<Hsla, String> {
    let value = decode(value);
    let parsed =
        penna::parse_hex(&value).map_err(|error| format!("{key} must be a hex color: {error}"))?;
    let digits = value.trim().trim_start_matches('#').len();
    Ok(match previous {
        Some(previous) if digits != 8 && digits != 4 => parsed.alpha(previous.a),
        _ => parsed,
    })
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

    fn themes() -> Vec<(String, InterfaceTheme)> {
        interface_theme::builtin_themes()
            .iter()
            .map(|(id, theme)| ((*id).to_owned(), theme.clone()))
            .collect()
    }

    #[test]
    fn parses_known_keys_and_ignores_the_rest() {
        let knobs = Knobs::parse(
            "?theme=dark&radius=12&zoom=1.25&story=buttons&interface-theme=default&motion=0",
        )
        .unwrap();
        assert_eq!(knobs.theme, Some(ThemeMode::Dark));
        assert_eq!(knobs.materials.radius, 12.0);
        assert_eq!(knobs.zoom, 1.25);
        assert_eq!(knobs.interface, "default");
        assert!(!knobs.motion);
        assert_eq!(Knobs::parse("app-knob=1").unwrap(), Knobs::default());
        assert!(Knobs::parse("radius=wide").is_err());
        assert!(Knobs::parse("interface-theme=sepia").is_err());
        let controls = |query| Knobs::parse(query).unwrap().materials.control_radius;
        assert_eq!(controls("control-radius=4&radius=8"), Some(4.0));
        assert_eq!(controls("radius=8&control-radius=8"), None);
        assert_eq!(controls("radius=8"), None);
    }

    #[test]
    fn the_appearance_knobs_make_an_appearance() {
        let knobs = Knobs::parse(
            "theme=light&interface-theme=macos&contrast=1.5&tint=0.3&tint-hue=145&zoom=1.25&motion=0&speed=0.5",
        )
        .unwrap();
        let appearance = knobs.appearance();
        assert_eq!(appearance.theme_mode(), Some(ThemeMode::Light));
        assert_eq!(appearance.theme.as_deref(), Some("macos"));
        assert_eq!(
            (appearance.overrides.tint, appearance.overrides.tint_hue),
            (Some(0.3), Some(145.0))
        );
        assert_eq!(appearance.contrast, 1.5);
        assert_eq!(appearance.zoom, 1.25);
        assert!(!appearance.animations);
        assert_eq!(appearance.animation_speed, 0.5);
        assert_eq!(Knobs::default().appearance().theme_mode(), None);
    }

    #[test]
    fn glass_sliders_override_the_preset_in_any_order() {
        let knobs = Knobs::parse("glass-blur=30&glass=frosted&glass-tint=0.5").unwrap();
        let glass = knobs.materials.glass.unwrap();
        assert_eq!(glass.blur, px(30.0));
        assert_eq!(glass.tint.a, 0.5);
        assert_eq!(glass.bezel, GlassMaterial::frosted().bezel);
        assert_eq!(
            Knobs::parse("glass=off&glass-blur=30")
                .unwrap()
                .materials
                .glass,
            None
        );
        assert!(Knobs::parse("glass=lava").is_err());
        let lit = Knobs::parse("glass=regular&glass-light=90&glass-glint-width=3").unwrap();
        let glass = lit.materials.glass.unwrap();
        assert!((glass.light_angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert_eq!(glass.glint_width, px(3.0));
    }

    #[test]
    fn light_glass_sliders_edit_the_themes_light_glass_until_a_preset_replaces_it() {
        let knobs = Knobs::parse("interface-theme=macos&light-glass-edge=0.3&glass-edge=0.6")
            .unwrap()
            .materials;
        assert_eq!(knobs.light_glass.unwrap().edge_shadow, 0.3);
        assert_eq!(knobs.glass.unwrap().edge_shadow, 0.6);
        let preset = Knobs::parse("interface-theme=macos&glass=frosted&light-glass-edge=0.3")
            .unwrap()
            .materials;
        assert_eq!(preset.light_glass, None);
        assert_eq!(preset.glass, GlassMaterial::preset("frosted"));
    }

    #[test]
    fn glass_lighting_and_activity_survive_import_export_and_reset() {
        let imported = crate::import_theme(
            r#"{"materials":{"glass":{"preset":"regular","light_directionality":0.37,"inactive_flattening":0.63}}}"#,
        )
        .unwrap();
        let knobs: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&imported).unwrap();
        let query = knobs
            .iter()
            .map(|(name, value)| {
                let value = value
                    .as_str()
                    .map_or_else(|| value.to_string(), |text| text.replace('#', "%23"));
                format!("{name}={value}")
            })
            .collect::<Vec<_>>()
            .join("&");
        let restored = Knobs::parse(&query).unwrap().interface_theme();
        let exported = interface_theme::parse_theme(&restored.to_json()).unwrap();
        let glass = exported.materials.glass.unwrap();
        assert_eq!(glass.light_directionality, 0.37);
        assert_eq!(glass.inactive_flattening, 0.63);

        let regular = Knobs::parse("glass=regular")
            .unwrap()
            .materials
            .glass
            .unwrap();
        assert_eq!(regular.light_directionality, 0.0);
        assert_eq!(regular.inactive_flattening, 0.0);
        for (id, theme) in interface_theme::builtin_themes() {
            let Some(expected) = theme.materials.glass else {
                continue;
            };
            let tuned = Knobs::parse(&format!(
                "interface-theme={id}&glass-light-directionality=0.37&glass-inactive-flattening=0.63"
            ))
            .unwrap()
            .materials
            .glass
            .unwrap();
            assert_eq!(tuned.light_directionality, 0.37);
            assert_eq!(tuned.inactive_flattening, 0.63);
            let reset = Knobs::parse(&format!("interface-theme={id}"))
                .unwrap()
                .materials
                .glass
                .unwrap();
            assert_eq!(reset.light_directionality, expected.light_directionality);
            assert_eq!(reset.inactive_flattening, expected.inactive_flattening);
        }
    }

    #[test]
    fn the_shadows_knob_picks_where_shadows_go() {
        assert!(
            Knobs::parse("shadows=picked")
                .unwrap()
                .materials
                .picked_shadows
        );
        assert!(Knobs::parse("shadows=some").is_err());
        let picked = Knobs::parse("shadows=picked").unwrap().materials;
        assert_eq!(material_knobs(&picked)["shadows"], "picked");
        assert_eq!(material_knobs(&Materials::default())["shadows"], "built-in");
    }

    #[test]
    fn the_smoothing_knobs_set_the_corners_and_where_they_go() {
        let materials = Knobs::parse("smoothing=2.5&smoothed=picked")
            .unwrap()
            .materials;
        assert_eq!(materials.smoothing.exponent(), 2.5);
        assert!(materials.picked_smoothing);
        assert_eq!(material_knobs(&materials)["smoothing"], 2.5);
        assert_eq!(material_knobs(&materials)["smoothed"], "picked");
        assert_eq!(material_knobs(&Materials::default())["smoothed"], "global");
        assert!(Knobs::parse("smoothing=1").is_err());
        assert!(Knobs::parse("smoothed=some").is_err());
    }

    #[test]
    fn the_outlines_knob_picks_where_outlines_go() {
        assert!(
            Knobs::parse("outlines=picked")
                .unwrap()
                .materials
                .picked_outlines
        );
        assert!(
            !Knobs::parse("outlines=built-in")
                .unwrap()
                .materials
                .picked_outlines
        );
        assert!(Knobs::parse("outlines=some").is_err());
        let picked = Knobs::parse("outlines=picked").unwrap().materials;
        assert_eq!(material_knobs(&picked)["outlines"], "picked");
    }

    #[test]
    fn the_interface_theme_sets_the_knobs_until_one_says_otherwise() {
        let default = interface_theme::default_theme();
        let knobs = Knobs::parse("interface-theme=default").unwrap();
        assert_eq!(knobs.interface_theme(), *default);
        let tuned = Knobs::parse(
            "radius=12&primary=accent&dark-on-accent=%23112233&interface-theme=default",
        )
        .unwrap();
        assert_eq!(tuned.materials.radius, 12.0);
        assert_eq!(tuned.materials.primary, PrimaryStyle::Accent);
        assert_eq!(tuned.dark.on_accent, penna::parse_hex("#112233").ok());
        assert_eq!(tuned.light, default.light);
        assert!(Knobs::parse("primary=loud").is_err());
        assert!(Knobs::parse("contact-y=low").is_err());
        assert!(Knobs::parse("light-accent=blue").is_err());
    }

    #[test]
    fn shadow_and_role_knobs_change_only_what_they_name() {
        let default = interface_theme::default_theme();
        let theme = Knobs::parse(
            "interface-theme=default&float-y=4&contact-strength=0.5&roles=%7B%22tile%22%3A%7B%22fill%22%3A%22glass%22%7D%7D",
        )
        .unwrap()
        .interface_theme();
        assert_eq!(theme.materials.float.y, 4.0);
        assert_eq!(theme.materials.float.blur, default.materials.float.blur);
        assert_eq!(theme.materials.contact.strength, 0.5);
        assert_eq!(theme.roles.len(), 1);
        assert_eq!(
            theme.roles["tile"],
            StylePatch::EMPTY.fill(penna::Fill::Glass)
        );
        let file = interface_theme::parse_theme(&theme.to_json()).unwrap();
        assert_eq!(file, theme);
        assert_eq!(
            Knobs::parse("interface-theme=default").unwrap().roles,
            default.roles
        );
        assert!(Knobs::parse("roles=%7B%22tilee%22%3A%7B%7D%7D").is_err());
        assert!(Knobs::parse("roles=chip").is_err());
        assert!(Knobs::parse("float-blur=soft").is_err());
    }

    #[test]
    fn a_role_inherits_what_the_theme_resolves_and_says_from_where() {
        let group_patches = BTreeMap::from([
            (
                "icon-button".to_owned(),
                StylePatch::EMPTY
                    .fill(penna::Fill::Tint)
                    .outline(true)
                    .hover(penna::Highlight::Ring),
            ),
            (
                "floating".to_owned(),
                StylePatch::EMPTY
                    .fill(penna::Fill::Glass)
                    .shape(penna::Shape::Pill),
            ),
        ]);
        let patched = InterfaceTheme {
            roles: group_patches.clone(),
            ..interface_theme::default_theme().clone()
        };
        let materials = Materials {
            picked_outlines: true,
            picked_shadows: true,
            smoothing: Smoothing::try_from(3.0).unwrap(),
            picked_smoothing: true,
            ..patched.materials.clone()
        };
        let picked = InterfaceTheme {
            materials: materials.clone(),
            ..patched.clone()
        };
        for (id, theme) in themes().into_iter().chain([
            ("patched groups".to_owned(), patched),
            ("picked outlines, shadows and smoothing".to_owned(), picked),
        ]) {
            let mut kit = Theme::default();
            theme.apply_roles(&mut kit);
            theme.materials.apply(&mut kit);
            for role in penna::roles::all() {
                let (mut style, _) = inherited(role, &theme.roles, &theme.materials);
                if let Some(patch) = theme.roles.get(role.id) {
                    patch.apply(&mut style);
                }
                assert_eq!(style, kit.style(role), "{id} {}", role.id);
            }
        }
        let built_in = Materials::default();
        let origin = |role, themed| Origin {
            role: Some(role),
            themed,
        };
        let (style, origins) = inherited(penna::roles::WINDOW_CONTROLS, &group_patches, &built_in);
        assert_eq!(style.fill, penna::Fill::Tint);
        assert_eq!(origins[0], origin(penna::SurfaceRole::ICON_BUTTON, true));
        assert_eq!(origins[2], origin(penna::SurfaceRole::ICON_BUTTON, false));
        assert_eq!(origins[3], origin(penna::roles::WINDOW_CONTROLS, false));
        assert_eq!(style.hover, penna::Highlight::Ring);
        assert_eq!(origins[5], origin(penna::SurfaceRole::ICON_BUTTON, true));
        let (style, origins) = inherited(penna::roles::INLINE_PANEL, &group_patches, &built_in);
        assert_eq!(style.fill, penna::Fill::Glass);
        assert_eq!(origins[0], origin(penna::Kind::Floating.role(), true));
        let (style, origins) = inherited(penna::SurfaceRole::DIALOG, &group_patches, &built_in);
        assert_eq!(style.shape, penna::Shape::Round);
        assert_eq!(origins[3], origin(penna::Kind::Floating.role(), false));
        assert_eq!(
            inherited(penna::SurfaceRole::TOAST, &group_patches, &built_in)
                .0
                .shape,
            penna::Shape::Pill
        );
        let (style, origins) = inherited(penna::SurfaceRole::DIALOG, &group_patches, &materials);
        assert!(!style.outline);
        assert_eq!(style.shadow, Shadow::None);
        let picked = Origin {
            role: None,
            themed: true,
        };
        assert_eq!((origins[1], origins[2]), (picked, picked));
        let (style, origins) = inherited(penna::roles::WINDOW_CONTROLS, &group_patches, &materials);
        assert!(style.outline);
        assert_eq!(origins[1], origin(penna::SurfaceRole::ICON_BUTTON, true));
    }

    #[test]
    fn a_shadow_color_knob_keeps_the_themes_alpha() {
        let tinted = Knobs::parse("dark-shadow-color=%2310203080&dark-shadow-color=%23405060")
            .unwrap()
            .dark
            .shadow_color
            .unwrap();
        assert!((tinted.a - 128.0 / 255.0).abs() < 1e-3);
        assert_eq!(penna::to_hex(tinted.alpha(1.0)), "#405060");
        let default = Knobs::parse("dark-shadow-color=%23102030").unwrap();
        let shadow = default.dark.shadow_color.unwrap();
        assert_eq!(shadow.a, ThemeColor::dark().shadow_color.a);
        assert_eq!(penna::to_hex(shadow.alpha(1.0)), "#102030");
    }

    #[test]
    fn a_theme_survives_the_trip_through_its_knobs() {
        for (id, theme) in themes() {
            let theme = &theme;
            let query = theme_knobs(theme)
                .iter()
                .map(|(name, value)| {
                    let value = value.as_str().map_or_else(
                        || value.to_string(),
                        |text| text.replace(' ', "+").replace('#', "%23"),
                    );
                    format!("{name}={value}")
                })
                .collect::<Vec<_>>()
                .join("&");
            let parsed = Knobs::parse(&format!("interface-theme=default&{query}")).unwrap();
            assert_eq!(parsed.name, theme.name, "{id}");
            assert_eq!(
                Materials {
                    glass: None,
                    light_glass: None,
                    ..parsed.materials.clone()
                },
                Materials {
                    glass: None,
                    light_glass: None,
                    ..theme.materials.clone()
                },
                "{id}"
            );
            assert_eq!(parsed.interface_theme().roles, theme.roles, "{id}");
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                assert_eq!(
                    resolved_colors(&parsed.interface_theme(), mode),
                    resolved_colors(theme, mode),
                    "{id} {mode:?}"
                );
            }
            if let (Some(parsed), Some(theirs)) = (parsed.materials.glass, theme.materials.glass) {
                for (name, get, _) in GLASS_KNOBS {
                    assert!((get(&parsed) - get(&theirs)).abs() < 1e-3, "{id} {name}");
                }
            }
        }
        assert_eq!(decode("Fira%20Sans+Mono%"), "Fira Sans Mono%");
    }
}
