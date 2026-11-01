use serde_json::Value;
use zz_protocol::{AgentProvider, CommandInvocation, PaneId};

pub const MODEL: &str = "model";
pub const EFFORT: &str = "effort";

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
    pub fn choices(&self, option: &str) -> Option<(&[Choice], &str)> {
        match option {
            MODEL => Some((&self.models, &self.model)),
            EFFORT => Some((&self.efforts, &self.effort)),
            _ => None,
        }
    }
}

#[must_use]
pub fn short_folder(folder: &str, home: Option<&str>) -> String {
    let trimmed = folder.trim_end_matches('/');
    if let Some(home) = home.map(|home| home.trim_end_matches('/'))
        && !home.is_empty()
        && let Some(rest) = trimmed.strip_prefix(home)
        && (rest.is_empty() || rest.starts_with('/'))
    {
        return format!("~{rest}");
    }
    if trimmed.is_empty() && !folder.is_empty() {
        return "/".to_owned();
    }
    trimmed.to_owned()
}

#[must_use]
pub fn setter(text: &str) -> Option<(&'static str, Option<&str>)> {
    let command = text.trim().strip_prefix('/')?;
    let (name, value) = command
        .split_once(char::is_whitespace)
        .map_or((command, ""), |(name, value)| (name, value.trim()));
    let option = [MODEL, EFFORT].into_iter().find(|option| *option == name)?;
    if value.contains(char::is_whitespace) {
        return None;
    }
    Some((option, (!value.is_empty()).then_some(value)))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerSetter<'a> {
    List(&'static str),
    Keep,
    Set(&'static str, &'a str),
}

#[must_use]
pub fn composer_setter<'a>(
    text: &'a str,
    settings: Option<&LensSettings>,
) -> Option<ComposerSetter<'a>> {
    let (option, value) = setter(text)?;
    let choices = settings.and_then(|settings| settings.choices(option));
    match value {
        None => choices
            .filter(|(choices, _)| !choices.is_empty())
            .map(|_| ComposerSetter::List(option)),
        Some(value) if choices.is_some_and(|(_, current)| current == value) => {
            Some(ComposerSetter::Keep)
        }
        Some(value) => Some(ComposerSetter::Set(option, value)),
    }
}

#[must_use]
pub fn source_message(item: &str) -> Option<&str> {
    let id = item.strip_prefix("u:")?;
    let id = id.split_once('~').map_or(id, |(id, _)| id);
    (!id.is_empty() && id != "message" && id != "user").then_some(id)
}

#[must_use]
pub const fn agent_program(provider: AgentProvider) -> &'static str {
    match provider {
        AgentProvider::ClaudeCode => "claude",
        AgentProvider::Codex => "codex",
    }
}

pub const SIDE_CHAT_WIDTH: &str = "46%";
pub const SIDE_CHAT_HEIGHT: &str = "72%";

fn command_line(words: &[&str]) -> String {
    words
        .iter()
        .map(|word| crate::verbs::quote(word))
        .collect::<Vec<_>>()
        .join(" ")
}

fn forked_chat(
    opener: &[&str],
    provider: AgentProvider,
    session: &str,
    message: Option<&str>,
) -> CommandInvocation {
    let mut select = vec!["select-pane-kind", "--resume", session, "--fork"];
    if let Some(message) = message {
        select.extend(["--at", message]);
    }
    select.extend(["agent", agent_program(provider)]);
    let line = format!("{} ; {}", command_line(opener), command_line(&select));
    CommandInvocation::new("if-shell", ["-F", "1", &line])
}

#[must_use]
pub fn fork_chat_command(
    provider: AgentProvider,
    session: &str,
    cwd: &str,
    message: Option<&str>,
) -> CommandInvocation {
    forked_chat(&["new-window", "-c", cwd], provider, session, message)
}

#[must_use]
pub fn side_chat_command(
    provider: AgentProvider,
    session: &str,
    cwd: &str,
    from: PaneId,
) -> CommandInvocation {
    let from = from.to_string();
    forked_chat(
        &[
            "new-pane",
            "-t",
            &from,
            "-c",
            cwd,
            "-x",
            SIDE_CHAT_WIDTH,
            "-y",
            SIDE_CHAT_HEIGHT,
            "-T",
            "Side chat",
        ],
        provider,
        session,
        None,
    )
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
            {"id":"model","currentValue":"opus","options":[{"value":"opus","name":"Opus"},{"value":"haiku","name":"Haiku"}],"_meta":{"zz":{"label":"Opus 5.5"}}},
            {"id":"effort","currentValue":"xhigh","options":[{"value":"high","name":"High"},{"value":"xhigh","name":"Extra high"}]},
        ]);
        let settings = LensSettings::parse(&options).expect("settings");
        assert_eq!(settings.summary(), "Opus 5.5 · Extra high");
        assert_eq!(settings.models.len(), 2);
        assert_eq!(
            settings.choices(EFFORT).map(|(_, current)| current),
            Some("xhigh")
        );
        assert_eq!(
            short_folder("/Users/me/src/zz", Some("/Users/me")),
            "~/src/zz"
        );
        assert_eq!(
            short_folder("/Users/me/src/zz", Some("/Users/m")),
            "/Users/me/src/zz"
        );
        assert_eq!(short_folder("/", None), "/");
        let codex = LensSettings::parse(&json!([
            {"id":"model","currentValue":"gpt-6-luna","options":[{"value":"gpt-6-luna","name":"GPT-6-Luna","_meta":{"zz":{"efforts":["low","high"],"defaultEffort":"low"}}}],"_meta":{"zz":{"label":"GPT-6-Luna"}}},
            {"id":"effort","currentValue":"high","options":[{"value":"low","name":"Low"},{"value":"high","name":"High"}]},
        ]))
        .expect("codex");
        assert_eq!(codex.summary(), "GPT-6-Luna · High");
        assert_eq!(codex.models.len(), 1);
        assert_eq!(LensSettings::from_json(""), None);
    }

    #[test]
    fn typed_model_and_effort_commands_become_setters() {
        assert_eq!(
            setter(" /model gpt-6-luna "),
            Some((MODEL, Some("gpt-6-luna")))
        );
        assert_eq!(setter("/effort high"), Some((EFFORT, Some("high"))));
        assert_eq!(setter("/model"), Some((MODEL, None)));
        assert_eq!(setter("/effort "), Some((EFFORT, None)));
        assert_eq!(setter("/model gpt-6-luna --reasoning=low"), None);
        assert_eq!(setter("/models"), None);
        assert_eq!(setter("switch /model opus"), None);
        assert_eq!(setter("/cd /tmp"), None);
    }

    #[test]
    fn a_composer_setter_lists_only_known_choices_and_skips_the_current_value() {
        let settings = LensSettings {
            model: "sonnet".to_owned(),
            models: vec![
                Choice {
                    value: "opus".to_owned(),
                    name: "Opus".to_owned(),
                },
                Choice {
                    value: "sonnet".to_owned(),
                    name: "Sonnet".to_owned(),
                },
            ],
            effort: "high".to_owned(),
            ..LensSettings::default()
        };
        assert_eq!(
            composer_setter("/model", Some(&settings)),
            Some(ComposerSetter::List(MODEL))
        );
        assert_eq!(composer_setter("/effort ", Some(&settings)), None);
        assert_eq!(composer_setter("/model", None), None);
        assert_eq!(
            composer_setter("/model sonnet", Some(&settings)),
            Some(ComposerSetter::Keep)
        );
        assert_eq!(
            composer_setter("/effort high", Some(&settings)),
            Some(ComposerSetter::Keep)
        );
        assert_eq!(
            composer_setter("/model opus", Some(&settings)),
            Some(ComposerSetter::Set(MODEL, "opus"))
        );
        assert_eq!(
            composer_setter("/effort low", None),
            Some(ComposerSetter::Set(EFFORT, "low"))
        );
        assert_eq!(composer_setter("/cd /tmp", Some(&settings)), None);
    }

    #[test]
    fn chat_items_name_the_message_a_fork_goes_through() {
        assert_eq!(source_message("u:3f2a"), Some("3f2a"));
        assert_eq!(source_message("u:3f2a~2"), Some("3f2a"));
        assert_eq!(source_message("a:3f2a"), None);
        assert_eq!(source_message("u:message"), None);
        let line = |command: CommandInvocation| {
            assert_eq!(command.name, "if-shell");
            assert_eq!(command.args[..2], ["-F", "1"]);
            command.args[2].as_str().to_owned()
        };
        assert_eq!(
            line(fork_chat_command(
                AgentProvider::Codex,
                "t",
                "/src/my app",
                Some("i")
            )),
            "new-window -c '/src/my app' ; select-pane-kind --resume t --fork --at i agent codex"
        );
        assert_eq!(
            line(fork_chat_command(
                AgentProvider::ClaudeCode,
                "s",
                "/a",
                None
            )),
            "new-window -c /a ; select-pane-kind --resume s --fork agent claude"
        );
        assert_eq!(
            line(side_chat_command(
                AgentProvider::ClaudeCode,
                "s",
                "/a/it's $HOME",
                PaneId(4)
            )),
            "new-pane -t %4 -c \"/a/it's \\$HOME\" -x 46% -y 72% -T 'Side chat' ; select-pane-kind --resume s --fork agent claude"
        );
        assert!(!real_session("lens-%3"));
    }
}
