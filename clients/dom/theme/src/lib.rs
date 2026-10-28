use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use zz_app::preferences::Preferences;
use zz_gpui::Hsla;
use zz_protocol::{ProviderStatus, ProviderTask, ProvidersReport};
use zz_ui::{
    Colorize as _, Emphasis, Fill, Highlight, PrimaryStyle, SurfaceRole, Theme, ThemeColor,
    ThemeMode, Tone,
    interface_theme::{self, ACCENTS, DEFAULT_TINT_HUE, InterfaceTheme},
    oklch,
    settings::{SettingsSection, about, providers::PROVIDERS},
    theme_css, to_hex,
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
    kit.set_contrast(preferences.contrast);
    kit.pip_opacity = preferences.pip_background_opacity;
    kit.pane_glow_strength = preferences.pane_glow_strength;
    kit.pane_frames = preferences.gaps;
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
pub fn theme_action(json: &str, action: &str, argument: &str) -> Result<String, JsError> {
    let mut preferences = decode(json)?;
    let outcome = match action {
        "pick" => {
            preferences.set_interface_theme(argument);
            Ok(None)
        }
        "save" => preferences.duplicate_theme().map(Some),
        "import" => preferences.import_theme(argument).map(Some),
        "delete" => {
            preferences.delete_theme(argument);
            Ok(None)
        }
        _ => return Err(JsError::new(&format!("no theme action {action}"))),
    };
    if outcome.is_ok() {
        preferences.save();
    }
    Ok(match outcome {
        Ok(saved) => json!({ "preferences": encode(&preferences), "saved": saved }),
        Err(error) => json!({ "preferences": encode(&preferences), "error": error }),
    }
    .to_string())
}

#[wasm_bindgen]
pub fn stylesheet(json: &str, dark: bool) -> Result<String, JsError> {
    let preferences = decode(json)?;
    let kit = kit(&preferences, mode(dark));
    Ok(theme_css::stylesheet(&preferences.shown_theme().name, &kit))
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

fn accent_ink(colors: &ThemeColor) -> Hsla {
    let mut theme = Theme::from(colors);
    theme.primary = PrimaryStyle::Accent;
    theme.paint(Tone::Primary, Emphasis::Solid).text
}

#[allow(
    clippy::disallowed_methods,
    reason = "previews draw another theme's colors, as zz-ui's settings previews do"
)]
fn theme_preview(theme: &InterfaceTheme, mode: ThemeMode) -> Value {
    let colors = theme.colors(mode);
    let materials = &theme.materials;
    let mut kit = Theme::from(&colors);
    theme.apply(&mut kit);
    let popover = kit.style(SurfaceRole::POPOVER);
    let surface_radius = (materials.radius / 3.0).min(10.0);
    let frosted = popover.fill == Fill::Glass
        && materials
            .glass
            .is_some_and(|glass| f32::from(glass.blur) > 0.0);
    let (selection, selected_text) = match kit.style(SurfaceRole::MENU_ITEM).selected {
        Highlight::Accent => (colors.accent.opaque(), accent_ink(&colors)),
        _ => (colors.background.washed(3), colors.foreground),
    };
    let surface = colors.background.raised(2).opaque();
    let (primary, primary_text) = match materials.primary {
        PrimaryStyle::Accent => (colors.accent, accent_ink(&colors)),
        PrimaryStyle::Foreground => (colors.foreground, colors.background),
    };
    let outline = materials.outline > 0.0 && popover.outline;
    let shadow = kit.casts_shadow(SurfaceRole::POPOVER) && materials.float.strength > 0.0;
    json!({
        "sidebar": color(colors.background.raised(2)),
        "background": color(colors.background),
        "border": color(colors.border()),
        "lights": [color(colors.danger), color(colors.warning), color(colors.success)],
        "text": color(colors.foreground.muted()),
        "radius": surface_radius.max(4.0),
        "menu": {
            "fill": color(if frosted { surface.opacity(0.6) } else { surface }),
            "radius": surface_radius,
            "rowRadius": surface_radius / 2.0,
            "inset": materials.row_inset.min(3.0),
            "outline": outline.then(|| color(colors.foreground.opacity(0.15))),
            "shadow": shadow.then(|| color(colors.shadow_color)),
            "selection": color(selection),
            "selectedText": color(selected_text),
        },
        "button": color(primary),
        "buttonText": color(primary_text),
    })
}

#[wasm_bindgen]
pub fn picks(json: &str, dark: bool) -> Result<String, JsError> {
    let preferences = decode(json)?;
    let mode = mode(dark);
    let kit = kit(&preferences, mode);
    let shown = preferences.shown_theme();
    let themes = interface_theme::builtin_themes()
        .iter()
        .map(|(id, theme)| ((*id).to_owned(), theme, false))
        .chain(
            preferences
                .themes
                .iter()
                .map(|(id, theme)| (id.clone(), theme, true)),
        )
        .map(|(id, theme, saved)| {
            let name = if theme.name.is_empty() {
                id.clone()
            } else {
                theme.name.clone()
            };
            json!({ "id": id, "name": name, "saved": saved, "preview": theme_preview(theme, mode) })
        })
        .collect::<Vec<_>>();
    let selected = preferences.theme_id();
    let mut own = preferences.overrides.clone();
    own.accent = None;
    let mut base = preferences.interface_theme().clone();
    own.apply_to(&mut base);
    let accent = preferences.overrides.accent.clone();
    let custom = accent
        .as_deref()
        .filter(|accent| !ACCENTS.contains(accent))
        .and_then(|accent| interface_theme::accent_color(accent, ThemeMode::Dark));
    let hue = preferences.overrides.tint_hue.unwrap_or(DEFAULT_TINT_HUE);
    Ok(json!({
        "themes": themes,
        "selected": selected,
        "saved": preferences.saved_theme(selected).is_some(),
        "modes": {
            "light": palette(&shown.colors(ThemeMode::Light)),
            "dark": palette(&shown.colors(ThemeMode::Dark)),
            "radius": f32::from(kit.radius),
        },
        "accent": {
            "selected": accent.unwrap_or_else(|| "theme".to_owned()),
            "own": color(base.colors(mode).accent),
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
            "strength": preferences.overrides.tint.unwrap_or(0.0),
        },
        "radius": preferences.widget_corner_radius(),
        "font": preferences.shown_ui_font(),
        "limits": {
            "radius": interface_theme::MAX_CORNER_RADIUS,
            "animation": [interface_theme::MIN_ANIMATION_SPEED, interface_theme::MAX_ANIMATION_SPEED],
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
    SettingsSection::StatusBar,
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
    let icon = if zz_ui::icon::glyphs::names().any(|name| name == icon) {
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
