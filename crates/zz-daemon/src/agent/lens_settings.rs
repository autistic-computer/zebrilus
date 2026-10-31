use serde_json::{Value, json};
use zz_protocol::AgentProvider;

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
