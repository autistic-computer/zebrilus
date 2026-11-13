use serde_json::Value;
use zz_protocol::{
    AgentAccount, AgentProvider, AgentQuota, CommandInvocation, MuxSnapshot, PaneId,
    PaneKindSnapshot, QuotaWindow, QuotaWindows,
};

pub const MODEL: &str = "model";
pub const EFFORT: &str = "effort";
const UNSET_EFFORT: &str = "default";
pub const MODE: &str = "mode";

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
    pub mode: String,
    pub modes: Vec<Choice>,
}

impl LensSettings {
    #[must_use]
    pub fn parse(options: &Value) -> Option<Self> {
        let options = options.as_array()?;
        let find = |id: &str| options.iter().find(|option| option["id"] == id);
        let model = find(MODEL)?;
        let effort = find(EFFORT);
        let mode = find(MODE);
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
        let models = choices(Some(model));
        let label = models
            .iter()
            .find(|choice| choice.value == current && !choice.name.is_empty())
            .map_or_else(|| current.clone(), |choice| choice.name.clone());
        Some(Self {
            model: current,
            model_label: label,
            models,
            effort: effort
                .map(|effort| text(&effort["currentValue"]))
                .unwrap_or_default(),
            efforts: choices(effort),
            mode: mode
                .map(|mode| text(&mode["currentValue"]))
                .unwrap_or_default(),
            modes: choices(mode),
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
    pub fn effort_starts_unset(&self) -> bool {
        self.efforts
            .first()
            .is_some_and(|choice| choice.value == UNSET_EFFORT)
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
            MODE => Some((&self.modes, &self.mode)),
            _ => None,
        }
    }

    #[must_use]
    pub fn mode_name(&self) -> &str {
        self.modes
            .iter()
            .find(|choice| choice.value == self.mode)
            .map_or(self.mode.as_str(), |choice| choice.name.as_str())
    }

    #[must_use]
    pub fn next_mode(&self) -> Option<&str> {
        let at = self
            .modes
            .iter()
            .position(|choice| choice.value == self.mode)?;
        let next = &self.modes[(at + 1) % self.modes.len()];
        (next.value != self.mode).then_some(next.value.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeRisk {
    Asks,
    ReadOnly,
    Unasked,
    Unguarded,
}

#[must_use]
pub fn mode_risk(provider: AgentProvider, mode: &str) -> ModeRisk {
    match (provider, mode) {
        (_, "plan" | "read-only") => ModeRisk::ReadOnly,
        (_, "bypassPermissions" | "full-access") => ModeRisk::Unguarded,
        (AgentProvider::ClaudeCode, "acceptEdits" | "auto") => ModeRisk::Unasked,
        _ => ModeRisk::Asks,
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

#[must_use]
pub fn pane_account(snapshot: &MuxSnapshot, pane: PaneId) -> Option<&str> {
    snapshot
        .sessions
        .iter()
        .flat_map(|session| &session.windows)
        .find_map(|window| window.panes.get(&pane))
        .and_then(|pane| match &pane.kind {
            PaneKindSnapshot::Agent(agent) => agent.account.as_deref(),
            _ => None,
        })
}

pub const DEFAULT_ACCOUNT_NAME: &str = "Default";
pub const SIGNED_OUT: &str = "Signed out";

#[must_use]
pub fn account_name(account: &AgentAccount) -> String {
    account
        .email
        .clone()
        .or_else(|| account.id.clone())
        .unwrap_or_else(|| DEFAULT_ACCOUNT_NAME.to_owned())
}

#[must_use]
pub fn account_quota(
    quotas: &[AgentQuota],
    provider: AgentProvider,
    current: Option<&str>,
    now: u64,
) -> Option<QuotaWindows> {
    let live = |window: Option<QuotaWindow>| {
        window.filter(|window| window.resets_at.is_none_or(|at| at > now))
    };
    quotas
        .iter()
        .find(|quota| quota.provider == provider && quota.account.as_deref() == current)
        .map(|quota| QuotaWindows {
            five_hour: live(quota.windows.five_hour),
            week: live(quota.windows.week),
        })
        .filter(|windows| !windows.is_empty())
}

#[must_use]
pub fn account_detail(account: &AgentAccount) -> Option<String> {
    if !account.signed_in {
        return Some(SIGNED_OUT.to_owned());
    }
    let parts = account
        .plan
        .as_deref()
        .map(plan_name)
        .into_iter()
        .chain(account.organization.clone())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn plan_name(plan: &str) -> String {
    let words = plan
        .split('_')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut characters = words.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

const FORK_WIDTH: &str = "46%";
const FORK_HEIGHT: &str = "72%";

fn command_line(words: &[&str]) -> String {
    words
        .iter()
        .map(|word| crate::verbs::quote(word))
        .collect::<Vec<_>>()
        .join(" ")
}

#[must_use]
pub fn fork_chat_command(
    provider: AgentProvider,
    session: &str,
    cwd: &str,
    from: PaneId,
    message: Option<&str>,
    settings: Option<&LensSettings>,
    account: Option<&str>,
) -> CommandInvocation {
    let from = from.to_string();
    let opener = [
        "new-pane",
        "-t",
        &from,
        "-c",
        cwd,
        "-x",
        FORK_WIDTH,
        "-y",
        FORK_HEIGHT,
    ];
    let mut select = vec!["select-pane-kind", "--resume", session, "--fork"];
    if let Some(message) = message {
        select.extend(["--at", message]);
    }
    if let Some(settings) = settings {
        if !settings.model.is_empty() {
            select.extend(["--model", &settings.model]);
        }
        if !settings.effort.is_empty() {
            select.extend(["--effort", &settings.effort]);
        }
    }
    if let Some(account) = account {
        select.extend(["--account", account]);
    }
    select.extend(["agent", agent_program(provider)]);
    let line = format!("{} ; {}", command_line(&opener), command_line(&select));
    CommandInvocation::new("if-shell", ["-F", "1", &line])
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
    fn an_account_reads_as_its_email_else_its_id_else_default() {
        let account = |id: Option<&str>, email: Option<&str>, plan: Option<&str>| AgentAccount {
            provider: AgentProvider::ClaudeCode,
            id: id.map(str::to_owned),
            signed_in: email.is_some(),
            email: email.map(str::to_owned),
            organization: None,
            plan: plan.map(str::to_owned),
        };
        let accounts = [
            account(None, Some("me@home.dev"), Some("max_20x")),
            AgentAccount {
                organization: Some("Acme".to_owned()),
                ..account(Some("a1"), Some("me@acme.dev"), Some("team"))
            },
        ];
        assert_eq!(account_detail(&accounts[0]).as_deref(), Some("Max 20x"));
        assert_eq!(account_detail(&accounts[1]).as_deref(), Some("Team · Acme"));
        let signed_out = account(None, None, None);
        assert_eq!(account_name(&signed_out), "Default");
        assert_eq!(account_detail(&signed_out).as_deref(), Some("Signed out"));
    }

    #[test]
    fn a_window_past_its_reset_is_unknown_and_stops_warning() {
        let reset = 1_791_583_200;
        let five_hour = QuotaWindow {
            used: 0.92,
            resets_at: Some(reset),
        };
        let week = QuotaWindow {
            used: 0.61,
            resets_at: Some(reset + 86_400),
        };
        let quotas = [AgentQuota {
            provider: AgentProvider::ClaudeCode,
            account: None,
            windows: QuotaWindows {
                five_hour: Some(five_hour),
                week: Some(week),
            },
        }];
        let at = |now| account_quota(&quotas, AgentProvider::ClaudeCode, None, now);
        let week_only = Some(QuotaWindows {
            five_hour: None,
            week: Some(week),
        });
        assert_eq!(at(reset + 1), week_only);
        assert_eq!(at(reset), week_only);
        assert!(!crate::agent_timeline::quota_warns(at(reset + 1).as_ref()));
        assert_eq!(
            at(reset - 1),
            Some(QuotaWindows {
                five_hour: Some(five_hour),
                week: Some(week),
            })
        );
        assert!(crate::agent_timeline::quota_warns(at(reset - 1).as_ref()));
    }

    #[test]
    fn lens_settings_read_the_daemons_config_options() {
        let options = json!([
            {"id":"model","currentValue":"haiku","options":[{"value":"opus","name":"Opus 5.5"},{"value":"haiku","name":"Haiku 5.5"}]},
            {"id":"effort","currentValue":"xhigh","options":[{"value":"high","name":"High"},{"value":"xhigh","name":"Extra high"}]},
        ]);
        let settings = LensSettings::parse(&options).expect("settings");
        assert_eq!(settings.summary(), "Haiku 5.5 · Extra high");
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
            {"id":"model","currentValue":"gpt-6-luna","options":[{"value":"gpt-6-luna","name":"GPT-6-Luna","_meta":{"zz":{"efforts":["low","high"],"defaultEffort":"low"}}}]},
            {"id":"effort","currentValue":"high","options":[{"value":"low","name":"Low"},{"value":"high","name":"High"}]},
        ]))
        .expect("codex");
        assert_eq!(codex.summary(), "GPT-6-Luna · High");
        assert_eq!(codex.models.len(), 1);
        assert_eq!(LensSettings::from_json(""), None);
    }

    #[test]
    fn the_mode_steps_through_the_drivers_list_and_carries_a_risk() {
        let settings = LensSettings::parse(&json!([
            {"id":"model","currentValue":"opus","options":[]},
            {"id":"mode","currentValue":"plan","options":[{"value":"default","name":"Ask"},{"value":"acceptEdits","name":"Accept edits"},{"value":"plan","name":"Plan"}]},
        ]))
        .expect("settings");
        assert_eq!(settings.mode_name(), "Plan");
        assert_eq!(settings.next_mode(), Some("default"));
        assert_eq!(
            [
                mode_risk(AgentProvider::ClaudeCode, "default"),
                mode_risk(AgentProvider::ClaudeCode, "plan"),
                mode_risk(AgentProvider::ClaudeCode, "auto"),
                mode_risk(AgentProvider::Codex, "auto"),
                mode_risk(AgentProvider::Codex, "full-access"),
            ],
            [
                ModeRisk::Asks,
                ModeRisk::ReadOnly,
                ModeRisk::Unasked,
                ModeRisk::Asks,
                ModeRisk::Unguarded,
            ]
        );
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
                PaneId(2),
                Some("i"),
                None,
                None
            )),
            "new-pane -t %2 -c '/src/my app' -x 46% -y 72% ; select-pane-kind --resume t --fork --at i agent codex"
        );
        assert_eq!(
            line(fork_chat_command(
                AgentProvider::ClaudeCode,
                "s",
                "/a/it's $HOME",
                PaneId(4),
                None,
                None,
                Some("a2")
            )),
            "new-pane -t %4 -c \"/a/it's \\$HOME\" -x 46% -y 72% ; select-pane-kind --resume s --fork --account a2 agent claude"
        );
        let settings = LensSettings {
            model: "haiku".to_owned(),
            effort: "low".to_owned(),
            ..LensSettings::default()
        };
        assert_eq!(
            line(fork_chat_command(
                AgentProvider::ClaudeCode,
                "s",
                "/a",
                PaneId(7),
                Some("m"),
                Some(&settings),
                Some("a1")
            )),
            "new-pane -t %7 -c /a -x 46% -y 72% ; select-pane-kind --resume s --fork --at m --model haiku --effort low --account a1 agent claude"
        );
        assert!(!real_session("lens-%3"));
    }
}
