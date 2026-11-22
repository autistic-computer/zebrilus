use std::fmt::Write as _;

use ala::{GlassMaterial, Hsla};
use penna::{
    Colorize as _, Elevation, Theme, ThemeColor, ThemeMode, foundation::theme_css, oklch, to_hex,
};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use zz_app::preferences::Preferences;
use zz_protocol::{ProviderStatus, ProviderTask, ProvidersReport};
use zz_ui::{
    interface_theme::{self, ACCENTS, Glass},
    pane_look::{PaneLook, rim_only},
    settings::{SettingsSection, about, providers::PROVIDERS},
};

const HUE_STOPS: u16 = 12;
const WEB_UI_FONT: &str = "Inter Variable";

fn decode(json: &str) -> Result<Preferences, JsError> {
    Preferences::decode(json).map_err(|error| JsError::new(&error.to_string()))
}

fn encode(preferences: &Preferences) -> Value {
    serde_json::to_value(preferences).unwrap_or(Value::Null)
}

fn mode(dark: bool) -> ThemeMode {
    if dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    }
}

fn color(color: Hsla) -> String {
    to_hex(color)
}

fn number(value: f32) -> String {
    if value.abs() < 0.0005 {
        return "0".to_owned();
    }
    let formatted = format!("{value:.3}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn length(value: f32) -> String {
    format!("{}px", number(value))
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn shadow(inset: bool, x: f32, y: f32, blur: f32, spread: f32, ink: Hsla) -> String {
    format!(
        "{}{} {} {} {} {}",
        if inset { "inset " } else { "" },
        length(x),
        length(y),
        length(blur),
        length(spread),
        color(ink)
    )
}

#[expect(
    clippy::disallowed_types,
    reason = "converts penna's own theme shadows into CSS tokens"
)]
fn shadows(list: &[ala::BoxShadow]) -> String {
    if list.is_empty() {
        return "0 0 #0000".to_owned();
    }
    list.iter()
        .map(|entry| {
            shadow(
                entry.inset,
                f32::from(entry.offset.x),
                f32::from(entry.offset.y),
                f32::from(entry.blur_radius) * 2.0,
                f32::from(entry.spread_radius),
                entry.color,
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn push_rim_layer(
    layers: &mut Vec<String>,
    inset: bool,
    dx: f32,
    dy: f32,
    blur: f32,
    spread: f32,
    ink: Hsla,
) {
    if ink.a > 0.001
        && (dx.abs() > 0.001 || dy.abs() > 0.001 || blur > 0.001 || spread.abs() > 0.001)
    {
        layers.push(shadow(inset, dx, dy, blur, spread, ink));
    }
}

fn push_glint(layers: &mut Vec<String>, material: &GlassMaterial, dir: (f32, f32)) {
    let bezel = f32::from(material.bezel);
    let glint = f32::from(material.glint_width).min(bezel);
    let light = material.light_color;
    let lit = if bezel > 0.0 { light.a } else { 0.0 };
    if glint <= 0.0 || lit * material.specular <= 0.0 {
        return;
    }
    let directionality = material.light_directionality.clamp(0.0, 1.0);
    let glint_spread = material.glint_spread.clamp(0.0, 1.0);
    let base_lit = mix(mix(0.15, 0.06, directionality), 1.0, glint_spread);
    let facing_extra = (1.0 - base_lit).max(0.0);
    let far_lit = mix(mix(1.0, 0.18, directionality), 1.0, glint_spread);
    let far_extra = (far_lit - base_lit).max(0.0);
    let spec = lit * material.specular;
    push_rim_layer(
        layers,
        true,
        0.0,
        0.0,
        0.0,
        glint,
        light.alpha((spec * base_lit).min(1.0)),
    );
    if bezel > glint {
        let tail = (glint * 4.0).min(bezel) * 2.0;
        push_rim_layer(
            layers,
            true,
            0.0,
            0.0,
            tail,
            0.0,
            light.alpha((spec * 0.2 * base_lit).min(1.0)),
        );
    }
    push_rim_layer(
        layers,
        true,
        -dir.0 * glint,
        -dir.1 * glint,
        0.0,
        0.0,
        light.alpha((spec * facing_extra).min(1.0)),
    );
    push_rim_layer(
        layers,
        true,
        dir.0 * glint,
        dir.1 * glint,
        0.0,
        0.0,
        light.alpha((spec * far_extra).min(1.0)),
    );
}

fn push_contour(layers: &mut Vec<String>, material: &GlassMaterial, dir: (f32, f32)) {
    let edge = f32::from(material.edge_width);
    let dark = material.edge_color;
    if edge <= 0.0 || dark.a * material.edge_shadow <= 0.0 {
        return;
    }
    let directionality = material.light_directionality.clamp(0.0, 1.0);
    let edge_spread = material.edge_spread.clamp(0.0, 1.0);
    let base_edge = mix(mix(0.35, 0.2, directionality), 1.0, edge_spread);
    let away_extra = (1.0 - base_edge).max(0.0);
    let toward_lit = mix(mix(1.0, 0.2, directionality), 1.0, edge_spread);
    let toward_extra = (toward_lit - base_edge).max(0.0);
    let contour = dark.a * material.edge_shadow;
    push_rim_layer(
        layers,
        false,
        0.0,
        0.0,
        0.0,
        edge,
        dark.alpha((contour * base_edge).min(1.0)),
    );
    push_rim_layer(
        layers,
        false,
        -dir.0 * edge,
        -dir.1 * edge,
        0.0,
        0.0,
        dark.alpha((contour * away_extra).min(1.0)),
    );
    push_rim_layer(
        layers,
        false,
        dir.0 * edge,
        dir.1 * edge,
        0.0,
        0.0,
        dark.alpha((contour * toward_extra).min(1.0)),
    );
}

fn rim(material: &GlassMaterial) -> String {
    let dir = (material.light_angle.cos(), material.light_angle.sin());
    let bezel = f32::from(material.bezel);
    let light = material.light_color;
    let lit = if bezel > 0.0 { light.a } else { 0.0 };
    let mut layers = Vec::new();
    push_glint(&mut layers, material, dir);
    if bezel > 0.0 && lit * material.fresnel > 0.0 {
        push_rim_layer(
            &mut layers,
            true,
            0.0,
            0.0,
            bezel * 2.0,
            0.0,
            light.alpha((lit * material.fresnel).min(1.0)),
        );
    }
    push_contour(&mut layers, material, dir);
    if layers.is_empty() {
        "0 0 #0000".to_owned()
    } else {
        layers.join(", ")
    }
}

fn render_stylesheet(preferences: &Preferences, dark: bool) -> String {
    let kit = kit(preferences, mode(dark));
    let mut css = theme_css::stylesheet(&kit, &penna::foundation::roles::TONED);
    let glass = kit.glass_effect();
    let has_glass = glass.is_some();
    let glass_rim = glass.as_ref().map_or_else(|| "0 0 #0000".to_owned(), rim);
    let pane_look = PaneLook {
        frames: preferences.gaps,
        glow_strength: preferences.pane_glow_strength,
    };
    let active_frame = pane_look.frame(&kit, kit.background, true);
    let inactive_frame = pane_look.frame(&kit, kit.background, false);
    let active_rim = if has_glass {
        rim(&rim_only(active_frame, &kit))
    } else {
        "0 0 #0000".to_owned()
    };
    let inactive_rim = if has_glass {
        rim(&rim_only(inactive_frame, &kit))
    } else {
        "0 0 #0000".to_owned()
    };
    let pane_shadow = if has_glass {
        shadows(&kit.pane_shadows())
    } else {
        "0 0 #0000".to_owned()
    };
    let ground = kit.background.opaque();
    let pip = glass
        .as_ref()
        .map_or(ground, |material| ground.alpha(material.tint.a));
    let pip_shadow = if has_glass {
        shadows(&kit.glass_float_shadows())
    } else {
        shadows(&kit.float_shadows())
    };
    let shell_frame = kit.surface(Elevation::Panel).fill;

    let _ = writeln!(css, ":root {{");
    let _ = writeln!(css, "  --penna-glass-rim: {glass_rim};");
    if let Some(material) = glass {
        let _ = writeln!(
            css,
            "  --penna-glass-glint-spread: {};",
            number(material.glint_spread)
        );
        let _ = writeln!(css, "  --penna-glass-noise: {};", number(material.noise));
        let _ = writeln!(css, "  --penna-glass-glow: {};", number(material.glow));
        let _ = writeln!(
            css,
            "  --penna-glass-glow-radius: {};",
            length(f32::from(material.glow_radius))
        );
    }
    let _ = writeln!(css, "  --zz-shell-frame: {};", color(shell_frame));
    let _ = writeln!(css, "  --zz-pane-fill: {};", color(active_frame.tint));
    let _ = writeln!(
        css,
        "  --zz-pane-fill-inactive: {};",
        color(inactive_frame.tint)
    );
    let _ = writeln!(css, "  --zz-pane-rim: {active_rim};");
    let _ = writeln!(css, "  --zz-pane-rim-inactive: {inactive_rim};");
    let _ = writeln!(css, "  --zz-pane-shadow: {pane_shadow};");
    let _ = writeln!(
        css,
        "  --zz-pane-edge: {};",
        color(pane_look.edge_color(&kit, true))
    );
    let _ = writeln!(
        css,
        "  --zz-pane-edge-inactive: {};",
        color(pane_look.edge_color(&kit, false))
    );
    let _ = writeln!(
        css,
        "  --zz-pane-edge-width: {};",
        length(f32::from(pane_look.edge_width(&kit)))
    );
    let _ = writeln!(css, "  --zz-pip-fill: {};", color(pip));
    let _ = writeln!(css, "  --zz-pip-rim: {glass_rim};");
    let _ = writeln!(css, "  --zz-pip-shadow: {pip_shadow};");
    let _ = writeln!(css, "  --zz-has-glass: {};", u8::from(has_glass));
    let _ = writeln!(css, "}}");
    let _ = writeln!(
        css,
        ".penna-gaps [data-penna=\"pane\"] {{ --penna-rim: {active_rim}; --penna-shadow: {pane_shadow}; }}"
    );
    css
}

fn kit(preferences: &Preferences, mode: ThemeMode) -> Theme {
    let mut kit = theme_css::kit_theme(&preferences.shown_theme(), mode);
    kit.set_contrast(preferences.appearance.contrast);
    kit.pane_margin = preferences.margin();
    let font = preferences.shown_ui_font();
    kit.font_family = if font.is_empty() { WEB_UI_FONT } else { font }
        .to_owned()
        .into();
    let mono = preferences.appearance.mono_font();
    if !mono.is_empty() {
        kit.mono_font_family = mono.to_owned().into();
    } else if let Some(family) = &preferences.terminal_font_family {
        kit.mono_font_family = family.clone().into();
    }
    kit
}

#[wasm_bindgen]
pub fn load() -> String {
    encode(&Preferences::load()).to_string()
}

#[wasm_bindgen]
pub fn save(json: &str) -> Result<String, JsError> {
    let preferences = decode(json)?;
    preferences.save();
    Ok(encode(&preferences).to_string())
}

#[wasm_bindgen]
pub fn platform_theme() -> String {
    interface_theme::builtin_themes()
        .iter()
        .find(|(_, theme)| std::ptr::eq(theme, interface_theme::platform_theme()))
        .map_or("default", |(id, _)| id)
        .to_owned()
}

#[wasm_bindgen]
pub fn stylesheet(json: &str, dark: bool) -> Result<String, JsError> {
    let preferences = decode(json)?;
    Ok(render_stylesheet(&preferences, dark))
}

#[wasm_bindgen]
pub fn icons(json: &str) -> Result<String, JsError> {
    let preferences = decode(json)?;
    Ok(theme_css::icons(&kit(&preferences, ThemeMode::Dark)))
}

#[allow(
    clippy::disallowed_methods,
    reason = "previews draw another theme's colors, as zz-ui's settings previews do"
)]
fn palette(colors: &ThemeColor) -> Value {
    json!({
        "sidebar": color(colors.background.raised(2)),
        "background": color(colors.background),
        "border": color(colors.border()),
        "lights": [color(colors.danger), color(colors.warning), color(colors.success)],
        "text": color(colors.foreground.muted()),
        "accent": color(colors.accent),
    })
}

#[wasm_bindgen]
pub fn picks(json: &str, dark: bool) -> Result<String, JsError> {
    let preferences = decode(json)?;
    let mode = mode(dark);
    let kit = kit(&preferences, mode);
    let shown = preferences.shown_theme();
    let appearance = &preferences.appearance;
    let accent = appearance.accent();
    let custom = Some(accent.as_str())
        .filter(|accent| !ACCENTS.contains(accent))
        .and_then(|accent| interface_theme::accent_color(accent, ThemeMode::Dark));
    let neutral = appearance.neutral();
    let hue = neutral.hue;
    let glass = match appearance.glass() {
        Glass::Material(_) => "custom".to_owned(),
        glass => interface_theme::spelled_glass(&glass),
    };
    Ok(json!({
        "modes": {
            "light": palette(&shown.colors(ThemeMode::Light)),
            "dark": palette(&shown.colors(ThemeMode::Dark)),
            "radius": f32::from(kit.radius),
        },
        "accent": {
            "selected": accent,
            "named": ACCENTS.iter().filter_map(|name| {
                interface_theme::accent_color(name, mode).map(|value| json!({ "id": name, "color": color(value) }))
            }).collect::<Vec<_>>(),
            "custom": custom.map(color),
            "ring": color(kit.foreground),
            "radius": f32::from(kit.swatch_radius()),
        },
        "tint": {
            "hue": hue,
            "dot": color(oklch(0.72, 0.13, hue)),
            "stops": (0..=HUE_STOPS).map(|stop| color(oklch(0.72, 0.13, f32::from(stop) * 360.0 / f32::from(HUE_STOPS)))).collect::<Vec<_>>(),
            "strength": neutral.amount,
        },
        "radius": interface_theme::corner_share(preferences.widget_corner_radius()),
        "controls": interface_theme::control_shape_setting(appearance.controls()).as_str(),
        "glass": glass,
        "font": preferences.shown_ui_font(),
        "mono": appearance.mono_font(),
        "limits": {
            "radius": interface_theme::MAX_CORNER_RADIUS,
        },
    })
    .to_string())
}

#[wasm_bindgen]
pub fn accent_hex(value: &str) -> Option<String> {
    interface_theme::accent_color(value, ThemeMode::Dark).map(|color| to_hex(color.opaque()))
}

#[wasm_bindgen]
pub fn about(platform: &str) -> String {
    json!({
        "version": about::VERSION,
        "repository": about::REPOSITORY_URL,
        "releases": about::RELEASES_URL,
        "issues": about::ISSUES_URL,
        "buildInfo": about::build_info(platform),
        "development": zz_protocol::app_identity::DEVELOPMENT,
    })
    .to_string()
}

const SECTIONS: [SettingsSection; 8] = [
    SettingsSection::Appearance,
    SettingsSection::Interface,
    SettingsSection::Panes,
    SettingsSection::Terminal,
    SettingsSection::Hosts,
    SettingsSection::Agents,
    SettingsSection::Advanced,
    SettingsSection::About,
];

#[wasm_bindgen]
pub fn sections() -> String {
    Value::Array(
        SECTIONS
            .iter()
            .map(|section| {
                let icon = section.icon().path();
                json!({
                    "id": format!("{section:?}").to_lowercase(),
                    "title": section.title(),
                    "icon": icon.trim_start_matches("icons/").trim_end_matches(".svg"),
                    "group": section.navigation_group() as u8,
                })
            })
            .collect(),
    )
    .to_string()
}

fn home_relative(path: &str) -> String {
    ["/Users/", "/home/"]
        .iter()
        .find_map(|root| {
            let rest = path.strip_prefix(root)?;
            let tail = rest.find('/').map_or("", |slash| &rest[slash..]);
            Some(format!("~{tail}"))
        })
        .unwrap_or_else(|| path.to_owned())
}

fn provider_row(status: &ProviderStatus, checking: bool) -> Value {
    let installed = status.installed();
    let version = match (&status.version, &status.latest) {
        (Some(version), Some(latest)) if status.outdated() => format!("{version} → {latest}"),
        (Some(version), _) => version.clone(),
        (None, Some(latest)) if !installed => format!("Latest {latest}"),
        (None, _) if installed => "Unknown version".to_owned(),
        (None, _) => String::new(),
    };
    let action = if matches!(status.task, ProviderTask::Running { .. }) {
        Some(
            json!({ "label": if installed { "Updating" } else { "Installing" }, "disabled": true }),
        )
    } else if installed && !status.outdated() {
        None
    } else if let Some(manual) = &status.manual {
        manual
            .command
            .as_ref()
            .map(|command| json!({ "label": "Copy command", "icon": "copy", "copy": command }))
    } else {
        Some(json!({
            "label": if installed { "Update" } else { "Install" },
            "disabled": checking && status.latest.is_none() && installed,
            "primary": !installed || status.outdated(),
        }))
    };
    let mut lines = Vec::new();
    if let Some(manual) = status.manual.as_ref().filter(|_| status.outdated()) {
        match &manual.command {
            Some(command) => {
                lines.push(json!({ "text": format!("Installed by {}. Update it with:", manual.owner), "tone": "muted" }));
                lines.push(json!({ "text": command, "tone": "plain" }));
            }
            None => lines.push(json!({ "text": format!("Installed by {}. Update it there.", manual.owner), "tone": "muted" })),
        }
    }
    match &status.task {
        ProviderTask::Running { output } if !output.is_empty() => {
            lines.push(json!({ "text": output, "tone": "muted" }));
        }
        ProviderTask::Failed { message } => {
            lines.push(json!({ "text": message, "tone": "warning" }));
        }
        ProviderTask::Running { .. } | ProviderTask::Idle => {}
    }
    let icon = zz_ui::pane::agent_provider_icon(status.provider).path();
    let icon = icon.trim_start_matches("icons/").trim_end_matches(".svg");
    let icon = if penna::components::icon::glyphs::names().any(|name| name == icon) {
        icon
    } else {
        "bot"
    };
    json!({
        "provider": status.provider.as_str(),
        "label": status.provider.label(),
        "icon": icon,
        "description": status.path.as_deref().map_or_else(|| "Not installed".to_owned(), home_relative),
        "version": version,
        "check": status.version.is_some() && status.latest.is_some() && !status.outdated(),
        "action": action,
        "lines": lines,
    })
}

#[wasm_bindgen]
pub fn providers(report: &str) -> String {
    let Ok(Some(report)) = serde_json::from_str::<Option<ProvidersReport>>(report) else {
        return json!({ "checking": true, "rows": [] }).to_string();
    };
    if report.checking && report.providers.is_empty() {
        return json!({ "checking": true, "rows": [] }).to_string();
    }
    let rows = PROVIDERS
        .iter()
        .map(|provider| {
            let status = report
                .provider(*provider)
                .cloned()
                .unwrap_or_else(|| ProviderStatus::new(*provider));
            provider_row(&status, report.checking)
        })
        .collect::<Vec<_>>();
    json!({ "checking": false, "rows": rows }).to_string()
}

#[wasm_bindgen]
pub fn storage_key() -> String {
    #[cfg(target_family = "wasm")]
    {
        Preferences::STORAGE_KEY.to_owned()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        if zz_protocol::app_identity::DEVELOPMENT {
            "zz-dev-web-preferences"
        } else {
            "zz-web-preferences"
        }
        .to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stylesheet_matches_desktop_glass_and_pane_look() {
        let mut prefs = Preferences {
            gaps: true,
            ..Preferences::default()
        };
        let dark = render_stylesheet(&prefs, true);
        assert!(dark.contains("--penna-glass-rim: inset 0px 0px 0px 0.5px #ffffff2b;"));
        assert!(dark.contains("--zz-shell-frame: #212120;"));
        assert!(dark.contains("--zz-pane-fill: #1a1a19;"));
        assert!(dark.contains("--zz-pane-fill-inactive: #151515;"));
        assert!(dark.contains(
            "--zz-pane-rim: inset 0px 0px 0px 0.5px #ffffff2b, inset 0px 0px 4px 0px #ffffff09;"
        ));
        assert!(dark.contains("--zz-has-glass: 1;"));

        prefs.appearance.overrides.glass = Some(Glass::None);
        let flat = render_stylesheet(&prefs, true);
        assert!(flat.contains("--penna-glass-rim: 0 0 #0000;"));
        assert!(flat.contains("--zz-pip-fill: #1a1a19;"));
        assert!(flat.contains("--zz-has-glass: 0;"));
    }
}
