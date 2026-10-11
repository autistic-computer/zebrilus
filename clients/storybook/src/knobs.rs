use zz_gpui::{App, GlassMaterial, Hsla, WindowAppearance, px};
use zz_ui::{
    Emphasis, FieldStyle, PrimaryStyle, SelectionStyle, Theme, ThemeColor, ThemeMode, Tone, UiZoom,
    interface_theme::{self, InterfaceTheme, Look, ThemeColors},
};

const ADAPTIVE_CORNER_FRACTION: f32 = 0.45;

/// The interface font the storybook loads and falls back to.
pub const UI_FONT: &str = "Inter Variable";

#[derive(Clone, Debug, PartialEq)]
pub struct Knobs {
    /// Light, dark, or `None` to follow the browser.
    pub theme: Option<ThemeMode>,
    /// The built-in theme the knobs start from.
    pub interface: &'static str,
    pub look: Look,
    pub light: ThemeColors,
    pub dark: ThemeColors,
    pub contrast: f32,
    pub zoom: f32,
    pub pane_opacity: f32,
    pub pane_glow: f32,
    pub pane_glass: bool,
    pub motion: bool,
    pub backdrop: Backdrop,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backdrop {
    #[default]
    Plain,
    Code,
    Color,
}

type Knob<T> = (&'static str, fn(&T) -> f32, fn(&mut T, f32));

/// The numeric parts of a [`Look`] other than its glass, by knob name.
pub const LOOK_KNOBS: &[Knob<Look>] = &[
    ("radius", |l| l.radius, |l, v| l.radius = v),
    (
        "smoothing",
        |l| l.corner_smoothing,
        |l, v| l.corner_smoothing = v,
    ),
    ("shadow", |l| l.shadow, |l, v| l.shadow = v),
    (
        "control-shadow",
        |l| l.control_shadow,
        |l, v| l.control_shadow = v,
    ),
    ("elevation", |l| l.elevation, |l, v| l.elevation = v),
    ("outline", |l| l.outline, |l, v| l.outline = v),
    (
        "outline-width",
        |l| l.outline_width,
        |l, v| l.outline_width = v,
    ),
    ("row-inset", |l| l.row_inset, |l, v| l.row_inset = v),
    ("density", |l| l.density, |l, v| l.density = v),
    (
        "control-fill",
        |l| l.control_fill,
        |l, v| l.control_fill = v,
    ),
    ("divider", |l| l.divider, |l, v| l.divider = v),
    (
        "animation-speed",
        |l| l.animation_speed,
        |l, v| l.animation_speed = v,
    ),
];

type ColorKnob = (
    &'static str,
    fn(&ThemeColors) -> Option<Hsla>,
    fn(&mut ThemeColors, Hsla),
);

/// One mode's colors by knob name. The panel prefixes each with `light-` or
/// `dark-`.
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
    ("glass-fresnel", |m| m.fresnel, |m, v| m.fresnel = v),
    ("glass-edge", |m| m.edge_shadow, |m, v| m.edge_shadow = v),
    (
        "glass-edge-width",
        |m| m.edge_width.as_f32(),
        |m, v| m.edge_width = px(v),
    ),
    ("glass-noise", |m| m.noise, |m, v| m.noise = v),
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
            look: theme.look.clone(),
            light: theme.light.clone(),
            dark: theme.dark.clone(),
            contrast: 1.0,
            zoom: 1.0,
            pane_opacity: 0.5,
            pane_glow: 1.0,
            pane_glass: false,
            motion: true,
            backdrop: Backdrop::Plain,
        }
    }

    /// Reads a knob query. The interface theme comes first, wherever it sits
    /// in the query, and every look and color knob then overrides it.
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
                            interface_theme::BUILTIN_THEMES.join(", ")
                        )
                    })?;
                Self::from_theme(id, theme)
            }
            None => Self::default(),
        };
        if let Some((_, value)) = pairs.iter().find(|(key, _)| *key == "glass") {
            knobs.look.glass = match *value {
                "" | "theme" => knobs.look.glass,
                "off" => None,
                name => Some(
                    GlassMaterial::preset(name)
                        .ok_or_else(|| format!("no glass preset named {name}"))?,
                ),
            };
        }
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
                "selection" => {
                    knobs.look.selection = match value {
                        "accent" => SelectionStyle::Accent,
                        "wash" => SelectionStyle::Wash,
                        _ => return Err(format!("selection must be accent or wash, not {value}")),
                    }
                }
                "primary" => {
                    knobs.look.primary = match value {
                        "foreground" => PrimaryStyle::Foreground,
                        "accent" => PrimaryStyle::Accent,
                        _ => {
                            return Err(format!(
                                "primary must be foreground or accent, not {value}"
                            ));
                        }
                    }
                }
                "field" => {
                    knobs.look.field = match value {
                        "soft" => FieldStyle::Soft,
                        "outline" => FieldStyle::Outline,
                        _ => return Err(format!("field must be soft or outline, not {value}")),
                    }
                }
                "font" => {
                    let family = decode(value);
                    knobs.look.font = (!family.is_empty()).then(|| family.into());
                }
                "contrast" => knobs.contrast = number(key, value)?,
                "zoom" => knobs.zoom = number(key, value)?,
                "pane-opacity" => knobs.pane_opacity = number(key, value)?,
                "pane-glow" => knobs.pane_glow = number(key, value)?,
                "pane-glass" => {
                    knobs.pane_glass = value != "0" && value != "off" && value != "false";
                }
                "motion" => knobs.motion = value != "0" && value != "off" && value != "false",
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
                    if let Some((_, _, set)) = LOOK_KNOBS.iter().find(|(name, ..)| *name == key) {
                        set(&mut knobs.look, number(key, value)?);
                    } else if let Some((_, _, set)) =
                        GLASS_KNOBS.iter().find(|(name, ..)| *name == key)
                    {
                        let value = number(key, value)?;
                        if let Some(material) = knobs.look.glass.as_mut() {
                            set(material, value);
                        }
                    } else if let Some((colors, name)) = key
                        .strip_prefix("light-")
                        .map(|name| (&mut knobs.light, name))
                        .or_else(|| {
                            key.strip_prefix("dark-")
                                .map(|name| (&mut knobs.dark, name))
                        })
                        && let Some((_, get, set)) =
                            COLOR_KNOBS.iter().find(|(color, ..)| *color == name)
                    {
                        set(colors, color(key, value, get(colors))?);
                    }
                }
            }
        }
        Ok(knobs)
    }

    /// The theme the knobs describe, as a theme file would hold it.
    pub fn interface_theme(&self) -> InterfaceTheme {
        InterfaceTheme {
            name: interface_theme::builtin_theme(self.interface)
                .map(|theme| theme.name.clone())
                .unwrap_or_default(),
            look: self.look.clone(),
            light: self.light.clone(),
            dark: self.dark.clone(),
        }
    }

    pub fn apply(&self, cx: &mut App) {
        let mode = self.theme.unwrap_or_else(|| match cx.window_appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => ThemeMode::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => ThemeMode::Light,
        });
        Theme::change(mode, None, cx);
        cx.set_reduce_motion(!self.motion);
        let theme = Theme::global_mut(cx);
        self.interface_theme().apply(theme);
        if self.look.font.is_none() {
            theme.font_family = UI_FONT.into();
        }
        theme.pane_background_opacity = self.pane_opacity;
        theme.pane_glow_strength = self.pane_glow;
        theme.pane_glass = self.pane_glass;
        theme.set_contrast(self.contrast);
        cx.set_global(UiZoom(self.zoom));
        let knobs = self.clone();
        for window in cx.windows() {
            window
                .update(cx, |_, window, _| {
                    window.set_zoom(knobs.zoom);
                    window.set_default_corner_smoothing(knobs.look.corner_smoothing);
                    window.set_adaptive_corner_fraction(Some(ADAPTIVE_CORNER_FRACTION));
                    window.refresh();
                })
                .ok();
        }
    }
}

/// The knob values that reproduce `look`, by knob name, for the page to
/// load: every look knob, the selection, the font and the glass.
pub fn look_knobs(look: &Look) -> serde_json::Map<String, serde_json::Value> {
    let mut knobs = LOOK_KNOBS
        .iter()
        .map(|(name, get, _)| ((*name).to_owned(), serde_json::json!(get(look))))
        .collect::<serde_json::Map<_, _>>();
    knobs.insert("selection".to_owned(), serde_json::json!(look.selection));
    knobs.insert("primary".to_owned(), serde_json::json!(look.primary));
    knobs.insert("field".to_owned(), serde_json::json!(look.field));
    knobs.insert(
        "font".to_owned(),
        serde_json::json!(look.font.as_ref().map_or("", |font| font.as_ref())),
    );
    match look.glass {
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

/// The knob values that reproduce `theme`: its look and both modes' colors,
/// each color as the `#rrggbb` a color input takes. A color the theme leaves
/// to the palette comes out as what the palette resolves it to.
pub fn theme_knobs(theme: &InterfaceTheme) -> serde_json::Map<String, serde_json::Value> {
    let mut knobs = look_knobs(&theme.look);
    for (mode, prefix) in [(ThemeMode::Light, "light"), (ThemeMode::Dark, "dark")] {
        for (name, value) in resolved_colors(theme, mode) {
            knobs.insert(format!("{prefix}-{name}"), serde_json::json!(value));
        }
    }
    knobs
}

/// Each color knob's value in `mode`, as `#rrggbb`.
pub fn resolved_colors(theme: &InterfaceTheme, mode: ThemeMode) -> Vec<(&'static str, String)> {
    let colors = theme.colors(mode);
    let ink = accent_ink(colors);
    let rgb = |color: Hsla| zz_ui::to_hex(color.alpha(1.0));
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

fn accent_ink(colors: ThemeColor) -> Hsla {
    let mut theme = Theme::from(&colors);
    theme.primary = PrimaryStyle::Accent;
    theme.paint(Tone::Primary, Emphasis::Solid).text
}

/// A color knob's value: `#rrggbb` keeps `previous`'s alpha, so a color
/// input can set a translucent shadow's hue.
fn color(key: &str, value: &str, previous: Option<Hsla>) -> Result<Hsla, String> {
    let value = decode(value);
    let parsed =
        zz_ui::parse_hex(&value).map_err(|error| format!("{key} must be a hex color: {error}"))?;
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

    #[test]
    fn parses_known_keys_and_ignores_the_rest() {
        let knobs = Knobs::parse(
            "?theme=dark&radius=12&zoom=1.25&story=agent&interface-theme=linear&motion=0",
        )
        .unwrap();
        assert_eq!(knobs.theme, Some(ThemeMode::Dark));
        assert_eq!(knobs.look.radius, 12.0);
        assert_eq!(knobs.zoom, 1.25);
        assert_eq!(knobs.interface, "linear");
        assert!(!knobs.motion);
        assert!(!knobs.pane_glass);
        assert!(Knobs::parse("pane-glass=on").unwrap().pane_glass);
        assert!(Knobs::parse("radius=wide").is_err());
        assert!(Knobs::parse("interface-theme=sepia").is_err());
    }

    #[test]
    fn glass_sliders_override_the_preset_in_any_order() {
        let knobs = Knobs::parse("glass-blur=30&glass=frosted&glass-tint=0.5").unwrap();
        let glass = knobs.look.glass.unwrap();
        assert_eq!(glass.blur, px(30.0));
        assert_eq!(glass.tint.a, 0.5);
        assert_eq!(glass.bezel, GlassMaterial::frosted().bezel);
        assert_eq!(
            Knobs::parse("glass=off&glass-blur=30").unwrap().look.glass,
            None
        );
        assert!(Knobs::parse("glass=lava").is_err());
        let lit = Knobs::parse("glass-light=90&glass-glint-width=3").unwrap();
        let glass = lit.look.glass.unwrap();
        assert!((glass.light_angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert_eq!(glass.glint_width, px(3.0));
    }

    #[test]
    fn the_interface_theme_sets_the_knobs_until_one_says_otherwise() {
        let linear = interface_theme::builtin_theme("linear").unwrap();
        let knobs = Knobs::parse("interface-theme=linear").unwrap();
        assert_eq!(knobs.interface_theme(), *linear);
        let tuned = Knobs::parse(
            "radius=12&selection=accent&dark-on-accent=%23112233&interface-theme=linear",
        )
        .unwrap();
        assert_eq!(tuned.look.radius, 12.0);
        assert_eq!(tuned.look.selection, SelectionStyle::Accent);
        assert_eq!(tuned.dark.on_accent, zz_ui::parse_hex("#112233").ok());
        assert_eq!(tuned.light, linear.light);
        assert!(Knobs::parse("selection=loud").is_err());
        assert!(Knobs::parse("light-accent=blue").is_err());
    }

    #[test]
    fn a_shadow_color_knob_keeps_the_themes_alpha() {
        let macos = interface_theme::builtin_theme("macos").unwrap();
        let knobs = Knobs::parse("interface-theme=macos&dark-shadow-color=%23102030").unwrap();
        let shadow = knobs.dark.shadow_color.unwrap();
        assert_eq!(shadow.a, macos.dark.shadow_color.unwrap().a);
        assert_eq!(zz_ui::to_hex(shadow.alpha(1.0)), "#102030");
    }

    #[test]
    fn a_theme_survives_the_trip_through_its_knobs() {
        for (id, theme) in interface_theme::builtin_themes() {
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
            let parsed = Knobs::parse(&format!("interface-theme={id}&{query}")).unwrap();
            assert_eq!(
                Look {
                    glass: None,
                    ..parsed.look.clone()
                },
                Look {
                    glass: None,
                    ..theme.look.clone()
                },
                "{id}"
            );
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                assert_eq!(
                    resolved_colors(&parsed.interface_theme(), mode),
                    resolved_colors(theme, mode),
                    "{id} {mode:?}"
                );
            }
            if let (Some(parsed), Some(look)) = (parsed.look.glass, theme.look.glass) {
                for (name, get, _) in GLASS_KNOBS {
                    assert!((get(&parsed) - get(&look)).abs() < 1e-3, "{id} {name}");
                }
            }
        }
        assert_eq!(decode("Fira%20Sans+Mono%"), "Fira Sans Mono%");
    }
}
