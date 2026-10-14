use std::collections::BTreeMap;

use zz_gpui::{App, GlassMaterial, Hsla, SharedString, WindowAppearance, px};
use zz_ui::{
    CORNER_SMOOTHING, Emphasis, PrimaryStyle, Style, StylePatch, SurfaceRole, Theme, ThemeColor,
    ThemeMode, Tone, UiZoom,
    interface_theme::{self, InterfaceTheme, Materials, ThemeColors},
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
    /// What the theme file calls the theme.
    pub name: String,
    pub materials: Materials,
    /// How the theme changes each kind and role: the start theme's until
    /// the `roles` knob names them.
    pub roles: BTreeMap<String, StylePatch>,
    /// The role whose surfaces draw the inspect ring.
    pub inspect: Option<SharedString>,
    pub light: ThemeColors,
    pub dark: ThemeColors,
    pub contrast: f32,
    pub zoom: f32,
    pub pane: PaneKnobs,
    pub motion: bool,
    pub backdrop: Backdrop,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneKnobs {
    pub gaps: bool,
    pub margin: f32,
    pub radius: f32,
    pub border: f32,
    pub background: f32,
    pub inactive: f32,
    pub glow: f32,
}

impl Default for PaneKnobs {
    fn default() -> Self {
        Self {
            gaps: false,
            margin: 6.0,
            radius: 13.5,
            border: 0.5,
            background: 0.5,
            inactive: 0.7,
            glow: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Backdrop {
    #[default]
    Plain,
    Code,
    Color,
}

type Knob<T> = (&'static str, fn(&T) -> f32, fn(&mut T, f32));

/// The lengths of [`Materials`] and its two shadows, by knob name.
pub const MATERIAL_KNOBS: &[Knob<Materials>] = &[
    ("radius", |m| m.radius, |m, v| m.radius = v),
    ("outline", |m| m.outline, |m, v| m.outline = v),
    ("row-inset", |m| m.row_inset, |m, v| m.row_inset = v),
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
            name: theme.name.clone(),
            materials: theme.materials.clone(),
            roles: theme.roles.clone(),
            inspect: None,
            light: theme.light.clone(),
            dark: theme.dark.clone(),
            contrast: 1.0,
            zoom: 1.0,
            pane: PaneKnobs::default(),
            motion: true,
            backdrop: Backdrop::Plain,
        }
    }

    /// Reads a knob query. The interface theme comes first, wherever it sits
    /// in the query, and every material and color knob then overrides it.
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
            knobs.materials.glass = match *value {
                "" | "theme" => knobs.materials.glass,
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
                "name" => knobs.name = decode(value),
                "roles" => knobs.roles = parse_roles(&decode(value))?,
                "inspect" => {
                    let id = decode(value);
                    knobs.inspect = match id.as_str() {
                        "" => None,
                        id if zz_ui::roles::find(id).is_some() => Some(id.to_owned().into()),
                        id => return Err(format!("inspect names no role: {id}")),
                    };
                }
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
                "font" => {
                    let family = decode(value);
                    knobs.materials.font = (!family.is_empty()).then(|| family.into());
                }
                "contrast" => knobs.contrast = number(key, value)?,
                "zoom" => knobs.zoom = number(key, value)?,
                "pane-gaps" => knobs.pane.gaps = flag(value),
                "pane-margin" => knobs.pane.margin = number(key, value)?,
                "pane-corner-radius" => knobs.pane.radius = number(key, value)?,
                "pane-border-width" => knobs.pane.border = number(key, value)?,
                "pane-opacity" => knobs.pane.background = number(key, value)?,
                "pane-inactive-opacity" => knobs.pane.inactive = number(key, value)?,
                "pane-glow" => knobs.pane.glow = number(key, value)?,
                "motion" => knobs.motion = flag(value),
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
                    if let Some((_, _, set)) = MATERIAL_KNOBS.iter().find(|(name, ..)| *name == key)
                    {
                        set(&mut knobs.materials, number(key, value)?);
                    } else if let Some((_, _, set)) =
                        GLASS_KNOBS.iter().find(|(name, ..)| *name == key)
                    {
                        let value = number(key, value)?;
                        if let Some(material) = knobs.materials.glass.as_mut() {
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
        Ok(knobs)
    }

    /// The theme the knobs describe, as a theme file would hold it.
    pub fn interface_theme(&self) -> InterfaceTheme {
        InterfaceTheme {
            name: self.name.clone(),
            materials: self.materials.clone(),
            roles: self.roles.clone(),
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
        if self.materials.font.is_none() {
            theme.font_family = UI_FONT.into();
        }
        theme.pane_background_opacity = self.pane.background;
        theme.pane_glow_strength = self.pane.glow;
        theme.pane_frames = self.pane.gaps;
        theme.pane_edge_width = px(self.pane.border);
        theme.inspect.clone_from(&self.inspect);
        theme.set_contrast(self.contrast);
        cx.set_global(UiZoom(self.zoom));
        let knobs = self.clone();
        for window in cx.windows() {
            window
                .update(cx, |_, window, _| {
                    window.set_zoom(knobs.zoom);
                    window.set_default_corner_smoothing(CORNER_SMOOTHING);
                    window.set_adaptive_corner_fraction(Some(ADAPTIVE_CORNER_FRACTION));
                    window.refresh();
                })
                .ok();
        }
    }
}

/// The knob values that reproduce `materials`, by knob name, for the page to
/// load: every length, both shadows, the primary style, the font and the
/// glass.
pub fn material_knobs(materials: &Materials) -> serde_json::Map<String, serde_json::Value> {
    let mut knobs = MATERIAL_KNOBS
        .iter()
        .map(|(name, get, _)| ((*name).to_owned(), serde_json::json!(get(materials))))
        .collect::<serde_json::Map<_, _>>();
    knobs.insert("primary".to_owned(), serde_json::json!(materials.primary));
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

/// The knob values that reproduce `theme`: its name, materials, roles and
/// both modes' colors, each color as the `#rrggbb` a color input takes. A
/// color the theme leaves to the palette comes out as what the palette
/// resolves it to.
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

/// A `roles` knob: the theme file's `roles` object, checked the way a theme
/// file is.
fn parse_roles(json: &str) -> Result<BTreeMap<String, StylePatch>, String> {
    let roles = serde_json::from_str::<serde_json::Value>(json)
        .map_err(|error| format!("roles must be JSON: {error}"))?;
    interface_theme::parse_theme(&serde_json::json!({ "roles": roles }).to_string())
        .map(|theme| theme.roles)
}

/// Where a role's prop comes from before the theme names the role itself:
/// its kind's built-in style, the theme's patch for its kind, or the role's
/// own built-in patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Kind,
    ThemeKind,
    Role,
}

impl Origin {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Kind => "kind",
            Self::ThemeKind => "theme-kind",
            Self::Role => "role",
        }
    }
}

/// The props of a [`Style`] in theme-file order.
pub const PROPS: [&str; 6] = ["fill", "outline", "shadow", "shape", "hover", "selected"];

/// The style `role` takes from everything but the theme's patch for it, and
/// where each of its [`PROPS`] comes from. For a kind, the theme's patch
/// for the kind is its own patch, so it inherits only the built-in.
pub fn inherited(role: SurfaceRole, roles: &BTreeMap<String, StylePatch>) -> (Style, [Origin; 6]) {
    let mut style = role.kind.base();
    let mut origins = [Origin::Kind; 6];
    let mut layer = |patch: &StylePatch, origin: Origin| {
        patch.apply(&mut style);
        let named = [
            patch.fill.is_some(),
            patch.outline.is_some(),
            patch.shadow.is_some(),
            patch.shape.is_some(),
            patch.hover.is_some(),
            patch.selected.is_some(),
        ];
        for (from, named) in origins.iter_mut().zip(named) {
            if named {
                *from = origin;
            }
        }
    };
    if !role.is_kind()
        && let Some(patch) = roles.get(role.kind.name())
    {
        layer(patch, Origin::ThemeKind);
    }
    layer(&role.patch, Origin::Role);
    (style, origins)
}

/// `style`'s props by name, spelled as a theme file spells them.
pub fn style_json(style: Style) -> serde_json::Value {
    serde_json::json!({
        "fill": style.fill,
        "outline": style.outline,
        "shadow": style.shadow,
        "shape": style.shape,
        "hover": style.hover,
        "selected": style.selected,
    })
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

    #[test]
    fn parses_known_keys_and_ignores_the_rest() {
        let knobs = Knobs::parse(
            "?theme=dark&radius=12&zoom=1.25&story=agent&interface-theme=linear&motion=0",
        )
        .unwrap();
        assert_eq!(knobs.theme, Some(ThemeMode::Dark));
        assert_eq!(knobs.materials.radius, 12.0);
        assert_eq!(knobs.zoom, 1.25);
        assert_eq!(knobs.interface, "linear");
        assert!(!knobs.motion);
        assert_eq!(
            Knobs::parse("pane-glass=on").unwrap().pane,
            PaneKnobs::default()
        );
        let pane = Knobs::parse("pane-gaps=1&pane-corner-radius=20&pane-inactive-opacity=0.4")
            .unwrap()
            .pane;
        assert!(pane.gaps);
        assert_eq!(pane.radius, 20.0);
        assert_eq!(pane.inactive, 0.4);
        assert!(Knobs::parse("pane-margin=wide").is_err());
        assert!(Knobs::parse("radius=wide").is_err());
        assert!(Knobs::parse("interface-theme=sepia").is_err());
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
        let lit = Knobs::parse("glass-light=90&glass-glint-width=3").unwrap();
        let glass = lit.materials.glass.unwrap();
        assert!((glass.light_angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert_eq!(glass.glint_width, px(3.0));
    }

    #[test]
    fn the_interface_theme_sets_the_knobs_until_one_says_otherwise() {
        let linear = interface_theme::builtin_theme("linear").unwrap();
        let knobs = Knobs::parse("interface-theme=linear").unwrap();
        assert_eq!(knobs.interface_theme(), *linear);
        let tuned = Knobs::parse(
            "radius=12&primary=foreground&dark-on-accent=%23112233&interface-theme=linear",
        )
        .unwrap();
        assert_eq!(tuned.materials.radius, 12.0);
        assert_eq!(tuned.materials.primary, PrimaryStyle::Foreground);
        assert_eq!(tuned.dark.on_accent, zz_ui::parse_hex("#112233").ok());
        assert_eq!(tuned.light, linear.light);
        assert!(Knobs::parse("primary=loud").is_err());
        assert!(Knobs::parse("contact-y=low").is_err());
        assert!(Knobs::parse("light-accent=blue").is_err());
    }

    #[test]
    fn shadow_and_role_knobs_change_only_what_they_name() {
        let tahoe = interface_theme::builtin_theme("tahoe").unwrap();
        let theme = Knobs::parse(
            "interface-theme=tahoe&float-y=4&contact-strength=0.5&roles=%7B%22chip%22%3A%7B%22fill%22%3A%22glass%22%7D%7D",
        )
        .unwrap()
        .interface_theme();
        assert_eq!(theme.materials.float.y, 4.0);
        assert_eq!(theme.materials.float.blur, tahoe.materials.float.blur);
        assert_eq!(theme.materials.contact.strength, 0.5);
        assert_eq!(theme.roles.len(), 1);
        assert_eq!(
            theme.roles["chip"],
            StylePatch::EMPTY.fill(zz_ui::Fill::Glass)
        );
        let file = interface_theme::parse_theme(&theme.to_json()).unwrap();
        assert_eq!(file, theme);
        assert_eq!(
            Knobs::parse("interface-theme=tahoe").unwrap().roles,
            tahoe.roles
        );
        assert!(Knobs::parse("roles=%7B%22chipp%22%3A%7B%7D%7D").is_err());
        assert!(Knobs::parse("roles=chip").is_err());
        assert!(Knobs::parse("float-blur=soft").is_err());
    }

    #[test]
    fn inspect_names_a_role_or_nothing() {
        assert_eq!(
            Knobs::parse("inspect=chrome.button")
                .unwrap()
                .inspect
                .as_deref(),
            Some("chrome.button")
        );
        assert_eq!(Knobs::parse("inspect=").unwrap().inspect, None);
        assert!(Knobs::parse("inspect=sparkle").is_err());
    }

    #[test]
    fn a_role_inherits_what_the_theme_resolves_and_says_from_where() {
        for (id, theme) in interface_theme::builtin_themes() {
            let mut kit = Theme::default();
            theme.apply_roles(&mut kit);
            for role in zz_ui::roles::all() {
                let (mut style, _) = inherited(role, &theme.roles);
                if let Some(patch) = theme.roles.get(role.id) {
                    patch.apply(&mut style);
                }
                assert_eq!(style, kit.style(role), "{id} {}", role.id);
            }
        }
        let tahoe = &interface_theme::builtin_theme("tahoe").unwrap().roles;
        let (style, origins) = inherited(zz_ui::roles::STATUS_CHIP, tahoe);
        assert_eq!(style.fill, zz_ui::Fill::Tint);
        assert_eq!(origins[0], Origin::Role);
        assert_eq!(origins[3], Origin::ThemeKind);
        assert_eq!(origins[1], Origin::ThemeKind);
        assert_eq!(origins[4], Origin::Kind);
        let (_, origins) = inherited(zz_ui::Kind::Chip.role(), tahoe);
        assert_eq!(origins, [Origin::Kind; 6]);
    }

    #[test]
    fn a_shadow_color_knob_keeps_the_themes_alpha() {
        let macos = interface_theme::builtin_theme("macos").unwrap();
        let knobs = Knobs::parse("interface-theme=macos&dark-shadow-color=%23102030").unwrap();
        let shadow = knobs.dark.shadow_color.unwrap();
        assert_eq!(shadow.a, macos.dark.shadow_color.unwrap().a);
        assert_eq!(zz_ui::to_hex(shadow.alpha(1.0)), "#102030");
        let default = Knobs::parse("dark-shadow-color=%23102030").unwrap();
        let shadow = default.dark.shadow_color.unwrap();
        assert_eq!(shadow.a, ThemeColor::dark().shadow_color.a);
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
            let parsed = Knobs::parse(&format!("interface-theme=default&{query}")).unwrap();
            assert_eq!(parsed.name, theme.name, "{id}");
            assert_eq!(
                Materials {
                    glass: None,
                    ..parsed.materials.clone()
                },
                Materials {
                    glass: None,
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
