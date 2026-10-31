use std::path::Path;

use serde_json::{Value, json};
use zz_protocol::AgentProvider;

use crate::agent::lens_commands::read_small;

pub(crate) const MODEL: &str = "model";
pub(crate) const EFFORT: &str = "effort";
pub(crate) const CWD: &str = "cwd";
pub(crate) const DIALOG: &str = "dialog";
pub(crate) const DIALOG_CLOSED: &str = "closed";

const CLAUDE_MODELS: [(&str, &str); 4] = [
    ("opus", "Opus"),
    ("sonnet", "Sonnet"),
    ("haiku", "Haiku"),
    ("fable", "Fable"),
];
const CLAUDE_EFFORTS: [(&str, &str); 5] = [
    ("low", "Low"),
    ("medium", "Medium"),
    ("high", "High"),
    ("xhigh", "Extra high"),
    ("max", "Max"),
];
const MAX_VALUE_BYTES: usize = 4096;

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
        if entry["type"] != "turn_context" {
            return;
        }
        let payload = &entry["payload"];
        if let Some(model) = payload["model"].as_str().filter(|model| !model.is_empty()) {
            self.model = Some(model.to_owned());
        }
        let effort = payload["effort"]
            .as_str()
            .or_else(|| payload["collaboration_mode"]["settings"]["reasoning_effort"].as_str());
        if let Some(effort) = effort.filter(|effort| !effort.is_empty()) {
            self.effort = Some(effort.to_owned());
        }
        if let Some(cwd) = payload["cwd"].as_str().filter(|cwd| !cwd.is_empty()) {
            self.cwd = Some(cwd.to_owned());
        }
    }

    pub(crate) fn config_options(&self, provider: AgentProvider) -> Value {
        let (models, efforts, terminal) = match provider {
            AgentProvider::ClaudeCode => (
                CLAUDE_MODELS
                    .iter()
                    .map(|(value, name)| json!({ "value": value, "name": name }))
                    .collect(),
                CLAUDE_EFFORTS
                    .iter()
                    .map(|(value, name)| json!({ "value": value, "name": name }))
                    .collect(),
                Value::Null,
            ),
            AgentProvider::Codex => (Vec::new(), Vec::new(), Value::from("/model")),
        };
        let model = self.model.as_deref().unwrap_or_default();
        json!([
            {
                "id": MODEL,
                "name": "Model",
                "category": "model",
                "type": "select",
                "currentValue": model_alias(model).unwrap_or(model),
                "options": models,
                "_meta": { "zz": { "label": model_label(model), "terminal": terminal } },
            },
            {
                "id": EFFORT,
                "name": "Effort",
                "category": "thought_level",
                "type": "select",
                "currentValue": self.effort.as_deref().unwrap_or_default(),
                "options": efforts,
                "_meta": { "zz": { "terminal": terminal } },
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
) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_VALUE_BYTES || value.chars().any(char::is_control) {
        return Err(format!("not a {option} the CLI can take"));
    }
    match (provider, option) {
        (AgentProvider::ClaudeCode, MODEL) => {
            if value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-._[]".contains(character))
            {
                Ok(format!("/model {value}"))
            } else {
                Err(format!("{value} is not a model name"))
            }
        }
        (AgentProvider::ClaudeCode, EFFORT) => CLAUDE_EFFORTS
            .iter()
            .any(|(effort, _)| *effort == value)
            .then(|| format!("/effort {value}"))
            .ok_or_else(|| format!("{value} is not an effort level")),
        (_, CWD) => Ok(format!("/cd {value}")),
        (AgentProvider::Codex, MODEL | EFFORT) => {
            Err("Codex picks its model and effort in its own /model dialog".to_owned())
        }
        _ => Err(format!("the lens cannot set {option}")),
    }
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

        let options = facts.config_options(AgentProvider::ClaudeCode);
        assert_eq!(options[0]["currentValue"], "haiku");
        assert_eq!(options[0]["_meta"]["zz"]["label"], "Haiku 5.5");
        assert_eq!(options[1]["currentValue"], "max");
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

    #[test]
    fn codex_facts_come_from_its_turn_context() {
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
        let options = facts.config_options(AgentProvider::Codex);
        assert_eq!(options[0]["_meta"]["zz"]["terminal"], "/model");
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
        assert_eq!(
            command(AgentProvider::ClaudeCode, MODEL, "sonnet").as_deref(),
            Ok("/model sonnet")
        );
        assert_eq!(
            command(AgentProvider::ClaudeCode, EFFORT, "xhigh").as_deref(),
            Ok("/effort xhigh")
        );
        assert_eq!(
            command(AgentProvider::Codex, CWD, "/tmp/x").as_deref(),
            Ok("/cd /tmp/x")
        );
        assert!(command(AgentProvider::ClaudeCode, EFFORT, "turbo").is_err());
        assert!(command(AgentProvider::ClaudeCode, MODEL, "opus; rm").is_err());
        assert!(command(AgentProvider::Codex, MODEL, "gpt").is_err());
        assert!(command(AgentProvider::ClaudeCode, CWD, "a\nb").is_err());
    }
}
