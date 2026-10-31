use std::{io::Read as _, path::Path};

use serde::Deserialize;
use serde_json::{Value, json};
use zz_protocol::AgentProvider;

use crate::agent::lens_commands::read_small;

pub(crate) const MODEL: &str = "model";
pub(crate) const EFFORT: &str = "effort";
pub(crate) const CWD: &str = "cwd";
pub(crate) const DIALOG: &str = "dialog";
pub(crate) const DIALOG_CLOSED: &str = "closed";
pub(crate) const CODEX_MODELS: &str = "models_cache.json";

const CLAUDE_MODELS: [(&str, &str); 4] = [
    ("opus", "Opus"),
    ("sonnet", "Sonnet"),
    ("haiku", "Haiku"),
    ("fable", "Fable"),
];
const CLAUDE_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
const CODEX_EFFORTS: [&str; 4] = ["low", "medium", "high", "xhigh"];
const EFFORT_NAMES: [(&str, &str); 8] = [
    ("none", "None"),
    ("minimal", "Minimal"),
    ("low", "Low"),
    ("medium", "Medium"),
    ("high", "High"),
    ("xhigh", "Extra high"),
    ("max", "Max"),
    ("ultra", "Ultra"),
];
const MAX_VALUE_BYTES: usize = 4096;
const MAX_CATALOG_BYTES: u64 = 8 * 1024 * 1024;
const MAX_MODELS: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ModelChoice {
    value: String,
    name: String,
    efforts: Vec<String>,
    default_effort: Option<String>,
    listed: bool,
}

impl ModelChoice {
    fn unknown(provider: AgentProvider, model: &str) -> Self {
        let (name, efforts) = match provider {
            AgentProvider::ClaudeCode => (model_label(model), &CLAUDE_EFFORTS[..]),
            AgentProvider::Codex => (model.to_owned(), &CODEX_EFFORTS[..]),
        };
        Self {
            value: model.to_owned(),
            name,
            efforts: efforts.iter().map(|effort| (*effort).to_owned()).collect(),
            default_effort: None,
            listed: true,
        }
    }

    fn option(&self) -> Value {
        json!({
            "value": self.value,
            "name": self.name,
            "_meta": { "zz": { "efforts": self.efforts, "defaultEffort": self.default_effort } },
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Catalog(Vec<ModelChoice>);

#[derive(Deserialize)]
struct CodexCache {
    #[serde(default)]
    models: Vec<CodexModel>,
}

#[derive(Deserialize)]
struct CodexModel {
    slug: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    default_reasoning_level: Option<String>,
    #[serde(default)]
    supported_reasoning_levels: Vec<CodexLevel>,
}

#[derive(Deserialize)]
struct CodexLevel {
    effort: String,
}

impl Catalog {
    pub(crate) fn claude() -> Self {
        Self(
            CLAUDE_MODELS
                .iter()
                .map(|(value, name)| ModelChoice {
                    name: (*name).to_owned(),
                    ..ModelChoice::unknown(AgentProvider::ClaudeCode, value)
                })
                .collect(),
        )
    }

    pub(crate) fn codex(path: &Path) -> Self {
        let mut text = String::new();
        std::fs::File::open(path)
            .and_then(|file| file.take(MAX_CATALOG_BYTES + 1).read_to_string(&mut text))
            .ok()
            .filter(|read| u64::try_from(*read).is_ok_and(|read| read <= MAX_CATALOG_BYTES))
            .map_or_else(Self::default, |_| Self::codex_json(&text))
    }

    pub(crate) fn codex_json(text: &str) -> Self {
        let Ok(cache) = serde_json::from_str::<CodexCache>(text) else {
            return Self::default();
        };
        let mut models = cache
            .models
            .into_iter()
            .filter(|model| model_name(&model.slug))
            .collect::<Vec<_>>();
        models.sort_by_key(|model| model.priority.unwrap_or(i64::MAX));
        Self(
            models
                .into_iter()
                .take(MAX_MODELS)
                .map(|model| ModelChoice {
                    name: model
                        .display_name
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or_else(|| model.slug.clone()),
                    efforts: model
                        .supported_reasoning_levels
                        .into_iter()
                        .map(|level| level.effort)
                        .filter(|effort| effort_word(effort))
                        .collect(),
                    default_effort: model
                        .default_reasoning_level
                        .filter(|effort| effort_word(effort)),
                    listed: model.visibility.as_deref() == Some("list"),
                    value: model.slug,
                })
                .collect(),
        )
    }

    fn entry(&self, provider: AgentProvider, model: &str) -> ModelChoice {
        self.0
            .iter()
            .find(|choice| choice.value == model)
            .cloned()
            .unwrap_or_else(|| ModelChoice::unknown(provider, model))
    }

    fn choices(&self, provider: AgentProvider, current: &str) -> Vec<ModelChoice> {
        let mut choices = self
            .0
            .iter()
            .filter(|choice| choice.listed)
            .cloned()
            .collect::<Vec<_>>();
        if !current.is_empty() && !choices.iter().any(|choice| choice.value == current) {
            choices.push(self.entry(provider, current));
        }
        choices
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Setter {
    pub(crate) slot: &'static str,
    pub(crate) text: String,
    pub(crate) applied: Vec<(&'static str, String)>,
}

impl Setter {
    fn one(option: &'static str, text: String, value: &str) -> Self {
        Self {
            slot: option,
            text,
            applied: vec![(option, value.to_owned())],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Facts {
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) cwd: Option<String>,
}

impl Facts {
    pub(crate) fn claude_entry(&mut self, entry: &Value) -> bool {
        if entry["isSidechain"] == true {
            return false;
        }
        if let Some(cwd) = entry["cwd"].as_str().filter(|cwd| !cwd.is_empty()) {
            self.cwd = Some(cwd.to_owned());
        }
        if entry["type"] == "assistant" {
            if let Some(model) = entry["message"]["model"]
                .as_str()
                .filter(|model| model.starts_with("claude-"))
            {
                self.model = Some(model.to_owned());
            }
            if let Some(effort) = entry["effort"].as_str().filter(|effort| !effort.is_empty()) {
                self.effort = Some(effort.to_owned());
            }
            return false;
        }
        if entry["type"] != "user" {
            return false;
        }
        let Some(output) = local_output(entry) else {
            return false;
        };
        if let Some(rest) = output.strip_prefix("Set model to ") {
            if let Some(model) = rest.split('`').nth(1).filter(|model| !model.is_empty()) {
                self.model = Some(model.to_owned());
            }
        } else if let Some(rest) = output.strip_prefix("Set effort level to ")
            && let Some(effort) = rest
                .split(|character: char| !character.is_ascii_alphanumeric())
                .next()
                .filter(|effort| !effort.is_empty())
        {
            self.effort = Some(effort.to_owned());
        }
        true
    }

    pub(crate) fn codex_entry(&mut self, entry: &Value) {
        let payload = &entry["payload"];
        let settings = match (entry["type"].as_str(), payload["type"].as_str()) {
            (Some("turn_context"), _) => payload,
            (Some("event_msg"), Some("thread_settings_applied")) => &payload["thread_settings"],
            _ => return,
        };
        if let Some(model) = settings["model"].as_str().filter(|model| !model.is_empty()) {
            self.model = Some(model.to_owned());
        }
        let effort = settings["effort"]
            .as_str()
            .or_else(|| settings["reasoning_effort"].as_str())
            .or_else(|| settings["collaboration_mode"]["settings"]["reasoning_effort"].as_str());
        if let Some(effort) = effort.filter(|effort| !effort.is_empty()) {
            self.effort = Some(effort.to_owned());
        }
        if let Some(cwd) = settings["cwd"].as_str().filter(|cwd| !cwd.is_empty()) {
            self.cwd = Some(cwd.to_owned());
        }
    }

    pub(crate) fn config_options(&self, provider: AgentProvider, catalog: &Catalog) -> Value {
        let model = self.model.as_deref().unwrap_or_default();
        let (current, label) = match provider {
            AgentProvider::ClaudeCode => (model_alias(model).unwrap_or(model), model_label(model)),
            AgentProvider::Codex => (model, catalog.entry(provider, model).name),
        };
        let choices = catalog.choices(provider, current);
        let efforts = match choices.iter().find(|choice| choice.value == current) {
            Some(choice) => choice.efforts.clone(),
            None if provider == AgentProvider::ClaudeCode => {
                ModelChoice::unknown(provider, current).efforts
            }
            None => Vec::new(),
        };
        json!([
            {
                "id": MODEL,
                "name": "Model",
                "category": "model",
                "type": "select",
                "currentValue": current,
                "options": choices.iter().map(ModelChoice::option).collect::<Vec<_>>(),
                "_meta": { "zz": { "label": label } },
            },
            {
                "id": EFFORT,
                "name": "Effort",
                "category": "thought_level",
                "type": "select",
                "currentValue": self.effort.as_deref().unwrap_or_default(),
                "options": efforts
                    .iter()
                    .map(|effort| json!({ "value": effort, "name": effort_name(effort).unwrap_or(effort) }))
                    .collect::<Vec<_>>(),
            },
            {
                "id": CWD,
                "name": "Directory",
                "category": "_zz_cwd",
                "type": "select",
                "currentValue": self.cwd.as_deref().unwrap_or_default(),
                "options": [],
            },
        ])
    }

    pub(crate) fn apply(&mut self, option: &str, value: &str) {
        let value = Some(value.to_owned());
        match option {
            MODEL => self.model = value,
            EFFORT => self.effort = value,
            CWD => self.cwd = value,
            _ => {}
        }
    }

    pub(crate) fn or(&self, launch: &Self) -> Self {
        Self {
            model: self.model.clone().or_else(|| launch.model.clone()),
            effort: self.effort.clone().or_else(|| launch.effort.clone()),
            cwd: self.cwd.clone().or_else(|| launch.cwd.clone()),
        }
    }

    pub(crate) fn claude_launch(arguments: &[String], config: Option<&Path>, cwd: &str) -> Self {
        let mut facts = Self {
            cwd: Some(cwd.to_owned()).filter(|cwd| !cwd.is_empty()),
            ..Self::default()
        };
        let project = Path::new(cwd).join(".claude");
        let files = config
            .map(|config| config.join("settings.json"))
            .into_iter()
            .chain(["settings.json", "settings.local.json"].map(|name| project.join(name)));
        for settings in files
            .filter_map(|path| read_small(&path))
            .filter_map(|text| serde_json::from_str::<Value>(&text).ok())
        {
            if let Some(model) = launch_text(&settings["model"]) {
                facts.model = Some(model);
            }
            if let Some(effort) = launch_text(&settings["effortLevel"]) {
                facts.effort = Some(effort);
            }
        }
        if let Some(model) = option_values(arguments, &["--model"])
            .pop()
            .and_then(setting_text)
        {
            facts.model = Some(model);
        }
        if let Some(effort) = option_values(arguments, &["--effort"])
            .pop()
            .and_then(setting_text)
        {
            facts.effort = Some(effort);
        }
        facts
    }

    pub(crate) fn codex_launch(
        arguments: &[String],
        home: Option<&Path>,
        cwd: Option<String>,
    ) -> Self {
        let mut facts = Self {
            cwd,
            ..Self::default()
        };
        if let Some(text) = home.and_then(|home| read_small(&home.join("config.toml"))) {
            facts.model = toml_top_level(&text, "model");
            facts.effort = toml_top_level(&text, "model_reasoning_effort");
        }
        for (key, value) in option_values(arguments, &["-c", "--config"])
            .into_iter()
            .filter_map(|value| value.split_once('='))
        {
            match (key.trim(), setting_text(value)) {
                ("model", Some(model)) => facts.model = Some(model),
                ("model_reasoning_effort", Some(effort)) => facts.effort = Some(effort),
                _ => {}
            }
        }
        if let Some(model) = option_values(arguments, &["-m", "--model"])
            .pop()
            .and_then(setting_text)
        {
            facts.model = Some(model);
        }
        facts
    }
}

fn launch_text(value: &Value) -> Option<String> {
    value.as_str().and_then(setting_text)
}

fn option_values<'a>(arguments: &'a [String], names: &[&str]) -> Vec<&'a str> {
    let mut values = Vec::new();
    let mut words = arguments.iter();
    while let Some(word) = words.next() {
        if names.contains(&word.as_str()) {
            values.extend(words.next().map(String::as_str));
        } else if let Some(value) = names
            .iter()
            .filter(|name| name.starts_with("--"))
            .find_map(|name| word.strip_prefix(name)?.strip_prefix('='))
        {
            values.push(value);
        }
    }
    values
}

fn setting_text(value: &str) -> Option<String> {
    let value = value.trim();
    let text = match value.chars().next() {
        Some(quote @ ('"' | '\'')) => value[1..].split(quote).next().unwrap_or_default(),
        _ => value.split('#').next().unwrap_or_default().trim(),
    };
    (!text.is_empty() && text.len() <= MAX_VALUE_BYTES && !text.chars().any(char::is_control))
        .then(|| text.to_owned())
}

fn toml_top_level(text: &str, key: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .take_while(|line| !line.starts_with('['))
        .filter_map(|line| line.split_once('='))
        .filter(|(name, _)| name.trim().trim_matches('"') == key)
        .find_map(|(_, value)| setting_text(value))
}

pub(crate) fn command(
    provider: AgentProvider,
    option: &str,
    value: &str,
    facts: &Facts,
    catalog: &Catalog,
) -> Result<Setter, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_VALUE_BYTES || value.chars().any(char::is_control) {
        return Err(format!("not a {option} the CLI can take"));
    }
    match (provider, option) {
        (_, CWD) => Ok(Setter::one(CWD, format!("/cd {value}"), value)),
        (_, MODEL) if !model_name(value) => Err(format!("{value} is not a model name")),
        (AgentProvider::ClaudeCode, MODEL) => {
            Ok(Setter::one(MODEL, format!("/model {value}"), value))
        }
        (AgentProvider::ClaudeCode, EFFORT) => CLAUDE_EFFORTS
            .contains(&value)
            .then(|| Setter::one(EFFORT, format!("/effort {value}"), value))
            .ok_or_else(|| format!("{value} is not an effort level")),
        (AgentProvider::Codex, MODEL) => {
            let current = facts.model.as_deref().unwrap_or_default();
            let choice = catalog
                .choices(provider, current)
                .into_iter()
                .find(|choice| choice.value == value)
                .ok_or_else(|| format!("Codex does not offer {value}"))?;
            let effort = facts
                .effort
                .clone()
                .filter(|effort| choice.efforts.contains(effort))
                .or(choice.default_effort);
            let mut applied = vec![(MODEL, value.to_owned())];
            let text = match effort {
                Some(effort) => {
                    let text = format!("/model {value} --reasoning={effort}");
                    applied.push((EFFORT, effort));
                    text
                }
                None => format!("/model {value}"),
            };
            Ok(Setter {
                slot: MODEL,
                text,
                applied,
            })
        }
        (AgentProvider::Codex, EFFORT) => {
            let model = facts
                .model
                .as_deref()
                .filter(|model| model_name(model))
                .ok_or("Codex has not said which model it runs yet")?;
            if !catalog
                .entry(provider, model)
                .efforts
                .iter()
                .any(|effort| effort == value)
            {
                return Err(format!("{model} does not take {value} effort"));
            }
            Ok(Setter {
                slot: MODEL,
                text: format!("/model {model} --reasoning={value}"),
                applied: vec![(EFFORT, value.to_owned())],
            })
        }
        _ => Err(format!("the lens cannot set {option}")),
    }
}

fn model_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_VALUE_BYTES
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-._[]".contains(character))
}

fn effort_word(effort: &str) -> bool {
    !effort.is_empty()
        && effort.len() <= 32
        && effort
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn effort_name(effort: &str) -> Option<&'static str> {
    EFFORT_NAMES
        .iter()
        .find(|(value, _)| *value == effort)
        .map(|(_, name)| *name)
}

pub(crate) fn claude_composer_shown(screen: &str) -> bool {
    let rule = |line: &str| {
        let line = line.trim();
        !line.is_empty() && line.chars().all(|character| character == '─')
    };
    let lines = screen
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    lines
        .windows(3)
        .rev()
        .take(12)
        .any(|window| rule(window[0]) && window[1].trim_start().starts_with('❯') && rule(window[2]))
}

fn local_output(entry: &Value) -> Option<&str> {
    let content = &entry["message"]["content"];
    let text = content.as_str().or_else(|| {
        content
            .as_array()?
            .iter()
            .find_map(|block| block["text"].as_str())
    })?;
    text.trim()
        .strip_prefix("<local-command-stdout>")?
        .strip_suffix("</local-command-stdout>")
        .map(str::trim)
}

fn model_alias(model: &str) -> Option<&'static str> {
    let lower = model.to_ascii_lowercase();
    CLAUDE_MODELS
        .iter()
        .map(|(alias, _)| *alias)
        .find(|alias| lower.contains(alias))
}

fn model_label(model: &str) -> String {
    if let Some((rest, name)) = CLAUDE_MODELS.iter().find_map(|(alias, name)| {
        model
            .strip_prefix(alias)
            .filter(|rest| rest.is_empty() || rest.starts_with('['))
            .map(|rest| (rest, name))
    }) {
        return format!("{name}{rest}");
    }
    let id = model
        .split_once("(claude-")
        .map_or(model, |(_, inner)| inner.trim_end_matches(')'));
    let id = id.strip_prefix("claude-").unwrap_or(id);
    if model.starts_with("claude-") || model.contains("(claude-") {
        let mut parts = id.split('-').filter(|part| !part.is_empty());
        let family = parts.next().unwrap_or_default();
        let version = parts
            .take_while(|part| part.chars().all(|character| character.is_ascii_digit()))
            .collect::<Vec<_>>()
            .join(".");
        let mut label = family
            .chars()
            .next()
            .map(|first| first.to_ascii_uppercase().to_string() + &family[first.len_utf8()..])
            .unwrap_or_default();
        if !version.is_empty() {
            label.push(' ');
            label.push_str(&version);
        }
        return label;
    }
    model.split(" (").next().unwrap_or(model).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(text: &str) -> Value {
        json!({
            "type": "user",
            "uuid": "u",
            "cwd": "/work",
            "message": { "role": "user", "content": format!("<local-command-stdout>{text}</local-command-stdout>") },
        })
    }

    #[test]
    fn claude_facts_follow_the_transcript_and_its_set_commands() {
        let mut facts = Facts::default();
        let assistant = json!({
            "type": "assistant",
            "cwd": "/work",
            "effort": "xhigh",
            "message": { "model": "claude-opus-5-5", "content": [] },
        });
        assert!(!facts.claude_entry(&assistant));
        assert_eq!(facts.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(facts.effort.as_deref(), Some("xhigh"));
        assert_eq!(facts.cwd.as_deref(), Some("/work"));

        assert!(facts.claude_entry(&local(
            "Set model to `Haiku 5.5` and saved as your default for new sessions"
        )));
        assert!(facts.claude_entry(&local(
            "Set effort level to max (this session only): Maximum capability"
        )));
        assert!(facts.claude_entry(&local("Settings dialog dismissed")));
        assert_eq!(facts.model.as_deref(), Some("Haiku 5.5"));
        assert_eq!(facts.effort.as_deref(), Some("max"));

        let options = facts.config_options(AgentProvider::ClaudeCode, &Catalog::claude());
        assert_eq!(options[0]["currentValue"], "haiku");
        assert_eq!(options[0]["_meta"]["zz"]["label"], "Haiku 5.5");
        assert_eq!(options[0]["options"].as_array().map(Vec::len), Some(4));
        assert_eq!(options[1]["currentValue"], "max");
        assert_eq!(options[1]["options"][3]["name"], "Extra high");
        assert_eq!(options[2]["currentValue"], "/work");
    }

    #[test]
    fn claude_shows_its_input_box_only_when_no_dialog_covers_it() {
        let rule = "─".repeat(40);
        let idle = format!("  ● high · /effort\n{rule}\n❯ \n{rule}\n  ⏵⏵ auto mode on\n");
        let dialog = "  Status   Config   Usage\n  Version: 2.1.316\n   Esc to cancel\n";
        assert!(claude_composer_shown(&idle));
        assert!(!claude_composer_shown(dialog));
    }

    #[test]
    fn model_labels_read_ids_and_the_names_claude_prints() {
        assert_eq!(model_label("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(model_label("haiku (claude-haiku-5-5)"), "Haiku 5.5");
        assert_eq!(model_label("Opus 5.5 (default)"), "Opus 5.5");
        assert_eq!(model_label("sonnet"), "Sonnet");
        assert_eq!(model_label("opus[1m]"), "Opus[1m]");
        assert_eq!(model_label(""), "");
    }

    const CODEX_CACHE: &str = r#"{"client_version":"0.166.0","models":[
        {"slug":"gpt-6-luna","display_name":"GPT-6-Luna","visibility":"list","priority":5,"default_reasoning_level":"medium","supported_reasoning_levels":[{"effort":"low"},{"effort":"medium"},{"effort":"high"},{"effort":"xhigh"},{"effort":"max"}],"context_window":400000},
        {"slug":"gpt-reserve","display_name":"GPT-Reserve","visibility":"hide","priority":4,"default_reasoning_level":"medium","supported_reasoning_levels":[{"effort":"low"},{"effort":"medium"}]},
        {"slug":"gpt-6.1-sol","display_name":"GPT-6.1-Sol","visibility":"list","priority":1,"default_reasoning_level":"low","supported_reasoning_levels":[{"effort":"low"},{"effort":"medium"},{"effort":"high"},{"effort":"xhigh"},{"effort":"max"},{"effort":"ultra"}]},
        {"slug":"bad slug; rm","visibility":"list","priority":0}
    ]}"#;

    #[test]
    fn codex_facts_come_from_its_turn_context_and_thread_settings() {
        let mut facts = Facts::default();
        facts.codex_entry(&json!({
            "type": "turn_context",
            "payload": {
                "model": "gpt-5.5-codex",
                "cwd": "/repo",
                "collaboration_mode": { "settings": { "reasoning_effort": "high" } },
            },
        }));
        assert_eq!(facts.model.as_deref(), Some("gpt-5.5-codex"));
        assert_eq!(facts.effort.as_deref(), Some("high"));
        assert_eq!(facts.cwd.as_deref(), Some("/repo"));
        facts.codex_entry(&json!({
            "type": "event_msg",
            "payload": {
                "type": "thread_settings_applied",
                "thread_settings": { "model": "gpt-6-luna", "reasoning_effort": "low", "cwd": "/repo/x" },
            },
        }));
        assert_eq!(facts.model.as_deref(), Some("gpt-6-luna"));
        assert_eq!(facts.effort.as_deref(), Some("low"));
        assert_eq!(facts.cwd.as_deref(), Some("/repo/x"));
    }

    #[test]
    fn the_codex_catalog_lists_its_models_by_priority_with_their_efforts() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(CODEX_MODELS);
        std::fs::write(&path, CODEX_CACHE).unwrap();
        let catalog = Catalog::codex(&path);
        assert_eq!(catalog, Catalog::codex_json(CODEX_CACHE));
        assert_eq!(
            Catalog::codex(&root.path().join("missing.json")),
            Catalog::default()
        );
        assert_eq!(Catalog::codex_json("{"), Catalog::default());

        let facts = Facts {
            model: Some("gpt-6-luna".to_owned()),
            effort: Some("max".to_owned()),
            cwd: None,
        };
        let options = facts.config_options(AgentProvider::Codex, &catalog);
        let values = |option: &Value| {
            option["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|choice| choice["value"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(values(&options[0]), ["gpt-6.1-sol", "gpt-6-luna"]);
        assert_eq!(options[0]["currentValue"], "gpt-6-luna");
        assert_eq!(options[0]["_meta"]["zz"]["label"], "GPT-6-Luna");
        assert!(options[0]["_meta"]["zz"]["terminal"].is_null());
        assert_eq!(
            options[0]["options"][0]["_meta"]["zz"]["efforts"],
            json!(["low", "medium", "high", "xhigh", "max", "ultra"])
        );
        assert_eq!(
            options[0]["options"][0]["_meta"]["zz"]["defaultEffort"],
            "low"
        );
        assert_eq!(
            values(&options[1]),
            ["low", "medium", "high", "xhigh", "max"]
        );
        assert_eq!(options[1]["currentValue"], "max");

        let hidden = Facts {
            model: Some("gpt-reserve".to_owned()),
            ..facts.clone()
        };
        let options = hidden.config_options(AgentProvider::Codex, &catalog);
        assert_eq!(
            values(&options[0]),
            ["gpt-6.1-sol", "gpt-6-luna", "gpt-reserve"]
        );
        assert_eq!(values(&options[1]), ["low", "medium"]);

        let options = facts.config_options(AgentProvider::Codex, &Catalog::default());
        assert_eq!(values(&options[0]), ["gpt-6-luna"]);
        assert_eq!(options[0]["_meta"]["zz"]["label"], "gpt-6-luna");
        assert_eq!(values(&options[1]), ["low", "medium", "high", "xhigh"]);
        let options = Facts::default().config_options(AgentProvider::Codex, &Catalog::default());
        assert!(values(&options[0]).is_empty() && values(&options[1]).is_empty());
    }

    #[test]
    fn launch_facts_read_the_command_line_over_the_clis_config() {
        let root = tempfile::tempdir().unwrap();
        let words = |words: &[&str]| {
            words
                .iter()
                .map(|word| (*word).to_owned())
                .collect::<Vec<_>>()
        };
        let config = root.path().join("claude");
        let project = root.path().join("project");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(project.join(".claude")).unwrap();
        std::fs::write(
            config.join("settings.json"),
            r#"{"model":"sonnet","effortLevel":"xhigh"}"#,
        )
        .unwrap();
        std::fs::write(
            project.join(".claude/settings.local.json"),
            r#"{"model":"opus[1m]"}"#,
        )
        .unwrap();
        let cwd = project.to_str().unwrap();
        let claude = Facts::claude_launch(
            &words(&["claude", "--dangerously-skip-permissions"]),
            Some(&config),
            cwd,
        );
        assert_eq!(claude.model.as_deref(), Some("opus[1m]"));
        assert_eq!(claude.effort.as_deref(), Some("xhigh"));
        assert_eq!(claude.cwd.as_deref(), Some(cwd));
        let claude = Facts::claude_launch(
            &words(&[
                "node",
                "/x/cli.js",
                "--effort",
                "xhigh",
                "--model=haiku",
                "--effort",
                "high",
            ]),
            None,
            cwd,
        );
        assert_eq!(claude.model.as_deref(), Some("haiku"));
        assert_eq!(claude.effort.as_deref(), Some("high"));
        let transcript = Facts {
            model: Some("claude-opus-5-5".to_owned()),
            ..Facts::default()
        };
        assert_eq!(
            transcript.or(&claude),
            Facts {
                model: Some("claude-opus-5-5".to_owned()),
                effort: Some("high".to_owned()),
                cwd: Some(cwd.to_owned()),
            }
        );

        let home = root.path().join("codex");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join("config.toml"),
            "model = \"gpt-5.5\" # default\nmodel_reasoning_effort = 'medium'\n\n[profiles.fast]\nmodel = \"gpt-5.5-mini\"\n",
        )
        .unwrap();
        let codex = Facts::codex_launch(&words(&["codex"]), Some(&home), None);
        assert_eq!(codex.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(codex.effort.as_deref(), Some("medium"));
        let codex = Facts::codex_launch(
            &words(&[
                "codex",
                "-c",
                "model_reasoning_effort=\"high\"",
                "--config=model=o3",
            ]),
            Some(&home),
            None,
        );
        assert_eq!(codex.model.as_deref(), Some("o3"));
        assert_eq!(codex.effort.as_deref(), Some("high"));
        let codex = Facts::codex_launch(
            &words(&[
                "codex",
                "-c",
                "model=o3",
                "-m",
                "gpt-5.5-codex",
                "resume",
                "t",
            ]),
            None,
            Some("/repo".to_owned()),
        );
        assert_eq!(codex.model.as_deref(), Some("gpt-5.5-codex"));
        assert_eq!(codex.effort, None);
        assert_eq!(codex.cwd.as_deref(), Some("/repo"));
    }

    #[test]
    fn setters_become_the_clis_own_slash_commands() {
        let none = Facts::default();
        let claude = Catalog::claude();
        let text = |provider, option, value, facts: &Facts, catalog: &Catalog| {
            command(provider, option, value, facts, catalog).map(|setter| setter.text)
        };
        assert_eq!(
            text(AgentProvider::ClaudeCode, MODEL, "sonnet", &none, &claude).as_deref(),
            Ok("/model sonnet")
        );
        assert_eq!(
            text(AgentProvider::ClaudeCode, EFFORT, "xhigh", &none, &claude).as_deref(),
            Ok("/effort xhigh")
        );
        assert_eq!(
            text(
                AgentProvider::Codex,
                CWD,
                "/tmp/x",
                &none,
                &Catalog::default()
            )
            .as_deref(),
            Ok("/cd /tmp/x")
        );
        assert!(text(AgentProvider::ClaudeCode, EFFORT, "turbo", &none, &claude).is_err());
        assert!(text(AgentProvider::ClaudeCode, MODEL, "opus; rm", &none, &claude).is_err());
        assert!(text(AgentProvider::ClaudeCode, CWD, "a\nb", &none, &claude).is_err());
    }

    #[test]
    fn codex_picks_type_its_model_command_with_the_reasoning_level() {
        let catalog = Catalog::codex_json(CODEX_CACHE);
        let facts = Facts {
            model: Some("gpt-6.1-sol".to_owned()),
            effort: Some("ultra".to_owned()),
            cwd: None,
        };
        let set = |option, value| command(AgentProvider::Codex, option, value, &facts, &catalog);
        assert_eq!(
            set(MODEL, "gpt-6-luna"),
            Ok(Setter {
                slot: MODEL,
                text: "/model gpt-6-luna --reasoning=medium".to_owned(),
                applied: vec![
                    (MODEL, "gpt-6-luna".to_owned()),
                    (EFFORT, "medium".to_owned())
                ],
            })
        );
        assert_eq!(
            set(EFFORT, "high"),
            Ok(Setter {
                slot: MODEL,
                text: "/model gpt-6.1-sol --reasoning=high".to_owned(),
                applied: vec![(EFFORT, "high".to_owned())],
            })
        );
        assert!(set(MODEL, "gpt-nope").is_err());
        assert!(set(MODEL, "gpt-reserve").is_err());
        assert!(set(EFFORT, "turbo").is_err());
        let kept = Facts {
            effort: Some("high".to_owned()),
            ..facts.clone()
        };
        assert_eq!(
            command(AgentProvider::Codex, MODEL, "gpt-6-luna", &kept, &catalog)
                .map(|setter| setter.text)
                .as_deref(),
            Ok("/model gpt-6-luna --reasoning=high")
        );
        assert!(
            command(
                AgentProvider::Codex,
                EFFORT,
                "low",
                &Facts::default(),
                &catalog
            )
            .is_err()
        );
        assert_eq!(
            command(
                AgentProvider::Codex,
                EFFORT,
                "xhigh",
                &facts,
                &Catalog::default()
            )
            .map(|setter| setter.text)
            .as_deref(),
            Ok("/model gpt-6.1-sol --reasoning=xhigh")
        );
    }
}
