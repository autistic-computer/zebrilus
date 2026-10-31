use serde_json::Value;
use zz_protocol::AgentProvider;

pub const MODEL: &str = "model";
pub const EFFORT: &str = "effort";
pub const CWD: &str = "cwd";
pub const DIALOG: &str = "dialog";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Choice {
    pub value: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LensSettings {
    pub model: String,
    pub model_label: String,
    pub models: Vec<Choice>,
    pub effort: String,
    pub efforts: Vec<Choice>,
    pub cwd: String,
    pub terminal: Option<String>,
}

impl LensSettings {
    #[must_use]
    pub fn parse(options: &Value) -> Option<Self> {
        let options = options.as_array()?;
        let find = |id: &str| options.iter().find(|option| option["id"] == id);
        let model = find(MODEL)?;
        let effort = find(EFFORT);
        let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
        let choices = |option: Option<&Value>| {
            option
                .and_then(|option| option["options"].as_array())
                .into_iter()
                .flatten()
                .filter_map(|choice| {
                    Some(Choice {
                        value: choice["value"].as_str()?.to_owned(),
                        name: choice["name"].as_str().unwrap_or_default().to_owned(),
                    })
                })
                .collect::<Vec<_>>()
        };
        let current = text(&model["currentValue"]);
        let label = model["_meta"]["zz"]["label"]
            .as_str()
            .filter(|label| !label.is_empty())
            .map_or_else(|| current.clone(), str::to_owned);
        Some(Self {
            model: current,
            model_label: label,
            models: choices(Some(model)),
            effort: effort
                .map(|effort| text(&effort["currentValue"]))
                .unwrap_or_default(),
            efforts: choices(effort),
            cwd: find(CWD)
                .map(|cwd| text(&cwd["currentValue"]))
                .unwrap_or_default(),
            terminal: model["_meta"]["zz"]["terminal"].as_str().map(str::to_owned),
        })
    }

    #[must_use]
    pub fn from_json(text: &str) -> Option<Self> {
        serde_json::from_str::<Value>(text)
            .ok()
            .as_ref()
            .and_then(Self::parse)
    }

    #[must_use]
    pub fn effort_name(&self) -> &str {
        self.efforts
            .iter()
            .find(|choice| choice.value == self.effort)
            .map_or(self.effort.as_str(), |choice| choice.name.as_str())
    }

    #[must_use]
    pub fn summary(&self) -> String {
        match (self.model_label.is_empty(), self.effort.is_empty()) {
            (false, false) => format!("{} · {}", self.model_label, self.effort_name()),
            (false, true) => self.model_label.clone(),
            (true, false) => self.effort_name().to_owned(),
            (true, true) => String::new(),
        }
    }

    #[must_use]
    pub fn short_cwd(&self, home: Option<&str>) -> String {
        let cwd = self.cwd.trim_end_matches('/');
        if let Some(home) = home.map(|home| home.trim_end_matches('/'))
            && !home.is_empty()
            && let Some(rest) = cwd.strip_prefix(home)
            && (rest.is_empty() || rest.starts_with('/'))
        {
            return format!("~{rest}");
        }
        if cwd.is_empty() && !self.cwd.is_empty() {
            return "/".to_owned();
        }
        cwd.to_owned()
    }
}

#[must_use]
pub fn source_message(item: &str) -> Option<&str> {
    let id = item.strip_prefix("u:")?;
    let id = id.split_once('~').map_or(id, |(id, _)| id);
    (!id.is_empty() && id != "message" && id != "user").then_some(id)
}

#[must_use]
pub fn side_chat_command(provider: AgentProvider, session: &str) -> Vec<String> {
    match provider {
        AgentProvider::ClaudeCode => ["claude", "--resume", session, "--fork-session"]
            .map(str::to_owned)
            .to_vec(),
        AgentProvider::Codex => ["codex", "fork", session].map(str::to_owned).to_vec(),
    }
}

#[must_use]
pub fn fork_command(provider: AgentProvider, session: &str, message: &str) -> Vec<String> {
    let provider = match provider {
        AgentProvider::ClaudeCode => "claude",
        AgentProvider::Codex => "codex",
    };
    ["zz", "tools", "--fork-chat", provider, session, message]
        .map(str::to_owned)
        .to_vec()
}

#[must_use]
pub fn real_session(session: &str) -> bool {
    !session.is_empty() && !session.starts_with("lens-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lens_settings_read_the_daemons_config_options() {
        let options = json!([
            {"id":"model","currentValue":"opus","options":[{"value":"opus","name":"Opus"},{"value":"haiku","name":"Haiku"}],"_meta":{"zz":{"label":"Opus 5.5","terminal":null}}},
            {"id":"effort","currentValue":"xhigh","options":[{"value":"high","name":"High"},{"value":"xhigh","name":"Extra high"}]},
            {"id":"cwd","currentValue":"/Users/me/src/zz","options":[]},
        ]);
        let settings = LensSettings::parse(&options).expect("settings");
        assert_eq!(settings.summary(), "Opus 5.5 · Extra high");
        assert_eq!(settings.models.len(), 2);
        assert_eq!(settings.terminal, None);
        assert_eq!(settings.short_cwd(Some("/Users/me")), "~/src/zz");
        assert_eq!(settings.short_cwd(Some("/Users/m")), "/Users/me/src/zz");
        let codex = LensSettings::parse(&json!([
            {"id":"model","currentValue":"gpt-5.5-codex","options":[],"_meta":{"zz":{"label":"gpt-5.5-codex","terminal":"/model"}}},
        ]))
        .expect("codex");
        assert_eq!(codex.terminal.as_deref(), Some("/model"));
        assert_eq!(codex.summary(), "gpt-5.5-codex");
        assert_eq!(LensSettings::from_json(""), None);
    }

    #[test]
    fn chat_items_name_the_message_a_fork_goes_through() {
        assert_eq!(source_message("u:3f2a"), Some("3f2a"));
        assert_eq!(source_message("u:3f2a~2"), Some("3f2a"));
        assert_eq!(source_message("a:3f2a"), None);
        assert_eq!(source_message("u:message"), None);
        assert_eq!(
            fork_command(AgentProvider::Codex, "t", "i"),
            ["zz", "tools", "--fork-chat", "codex", "t", "i"]
        );
        assert_eq!(
            side_chat_command(AgentProvider::ClaudeCode, "s"),
            ["claude", "--resume", "s", "--fork-session"]
        );
        assert!(!real_session("lens-%3"));
    }
}
