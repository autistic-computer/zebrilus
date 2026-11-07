use ala::Hsla;
use penna::{Colorize as _, Theme, ThemeColor, ThemeMode, foundation::theme_css, oklch, to_hex};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use zz_app::preferences::Preferences;
use zz_protocol::{ProviderStatus, ProviderTask, ProvidersReport};
use zz_ui::{
    interface_theme::{self, ACCENTS, Glass},
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

fn kit(preferences: &Preferences, mode: ThemeMode) -> Theme {
    let mut kit = theme_css::kit_theme(&preferences.shown_theme(), mode);
    kit.set_contrast(preferences.appearance.contrast);
    if preferences.appearance.reduce_transparency {
        kit.glass = None;
    }
    kit.pane_margin = preferences.margin();
    let font = preferences.shown_ui_font();
    kit.font_family = if font.is_empty() { WEB_UI_FONT } else { font }
        .to_owned()
        .into();
    if let Some(family) = &preferences.terminal_font_family {
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
    let kit = kit(&preferences, mode(dark));
    let ground = kit.background.opaque();
    let pip = kit
        .glass_effect()
        .map_or(ground, |glass| ground.alpha(glass.tint.a));
    Ok(format!(
        "{}:root {{ --zz-pip-fill: {}; }}\n",
        theme_css::stylesheet(&kit, &penna::foundation::roles::TONED),
        color(pip)
    ))
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

const SECTIONS: [SettingsSection; 7] = [
    SettingsSection::Appearance,
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
    let stale = status.outdated() || status.latest.is_none();
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
    } else if installed && !stale {
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
    if let Some(manual) = status.manual.as_ref().filter(|_| stale) {
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
        "check": installed && !status.outdated() && status.latest.is_some(),
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
    Preferences::STORAGE_KEY.to_owned()
}
