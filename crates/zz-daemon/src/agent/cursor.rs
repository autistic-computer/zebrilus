use std::path::PathBuf;

use agent_client_protocol::{AcpAgent, AcpAgentConfig};
use serde_json::Value;
use zz_protocol::{AGENT_CURSOR_COMMAND, AgentProvider};

use crate::agent::{
    dialect::{self, Dialect, Unguarded},
    environment::agent_path,
    host::RuntimeChannels,
    runtime::AgentSpawnConfig,
    session_files,
};

const SHORT_COMMAND: &str = "agent";
const NO_BROWSER: &str = "NO_OPEN_BROWSER";
pub(crate) const DIALECT: Dialect = Dialect {
    effort_option: "effort",
    signed_out: "Cursor is signed out.",
    stored_title,
    sign_in: Some("cursor_login"),
    model_name: Some(model_name),
    unguarded: Some(Unguarded {
        over: "agent",
        always: never_asks,
    }),
    quota: None,
    rules: None,
};

pub(crate) async fn run_cursor_runtime(
    config: AgentSpawnConfig,
    provider: AgentProvider,
    channels: RuntimeChannels,
) -> Result<(), String> {
    let inherited = std::env::var("PATH").ok();
    let agent = AcpAgent::new(
        AcpAgentConfig::new(program(agent_path().or(inherited.as_deref()))?)
            .arg("acp")
            .env(NO_BROWSER, "1"),
    );
    dialect::run(agent, DIALECT, config, provider, channels).await
}

fn home() -> Option<PathBuf> {
    std::env::var_os("CURSOR_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cursor")))
}

fn never_asks() -> bool {
    home()
        .and_then(|home| session_files::read_small(&home.join("cli-config.json")))
        .and_then(|config| serde_json::from_str::<Value>(&config).ok())
        .is_some_and(|config| config["approvalMode"] == "unrestricted")
}

fn model_name(value: &str) -> Option<String> {
    let (slug, parameters) = value.split_once('[')?;
    if slug == "default" {
        return None;
    }
    let parameter = |names: &[&str]| {
        parameters
            .trim_end_matches(']')
            .split(',')
            .filter_map(|parameter| parameter.split_once('='))
            .find(|(name, _)| names.contains(name))
            .map(|(_, value)| value)
    };
    let mut words = Vec::<String>::new();
    for part in slug.split('-') {
        let number =
            |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
        if number(part)
            && part.len() <= 2
            && let Some(last) = words.last_mut().filter(|last| number(last))
        {
            last.push('.');
            last.push_str(part);
            continue;
        }
        words.push(match part {
            "gpt" => "GPT".to_owned(),
            "glm" => "GLM".to_owned(),
            "deepseek" => "DeepSeek".to_owned(),
            _ => capitalized(part),
        });
    }
    match parameter(&["effort", "reasoning_effort", "reasoning"]) {
        Some("xhigh") => words.push("Extra High".to_owned()),
        Some(effort) => words.push(capitalized(effort)),
        None => {}
    }
    if parameter(&["fast"]) == Some("true") {
        words.push("Fast".to_owned());
    }
    let name = words.join(" ");
    Some(match name.strip_prefix("GPT ") {
        Some(rest) => format!("GPT-{rest}"),
        None => name,
    })
}

fn capitalized(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

fn stored_title(session: &str) -> Option<String> {
    if session.starts_with('.') || session.contains(['/', '\\']) {
        return None;
    }
    let meta = home()?.join("acp-sessions").join(session).join("meta.json");
    let meta = serde_json::from_str::<Value>(&session_files::read_small(&meta)?).ok()?;
    meta["title"]
        .as_str()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
}

fn program(path: Option<&str>) -> Result<PathBuf, String> {
    dialect::installed(path, AGENT_CURSOR_COMMAND)
        .into_iter()
        .next()
        .or_else(|| {
            dialect::installed(path, SHORT_COMMAND)
                .into_iter()
                .find(|candidate| {
                    std::fs::canonicalize(candidate).is_ok_and(|real| {
                        real.components()
                            .any(|part| part.as_os_str() == AGENT_CURSOR_COMMAND)
                    })
                })
        })
        .ok_or_else(|| {
            "Cursor's CLI is not installed: `cursor-agent` was not found on PATH. Install it with the script at https://cursor.com/install."
                .to_owned()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_value_reads_as_its_name_effort_and_speed() {
        for (value, name) in [
            ("grok-4.6[effort=high,fast=true]", "Grok 4.6 High Fast"),
            ("composer-2.5[fast=true]", "Composer 2.5 Fast"),
            (
                "claude-opus-5-5[context=300k,effort=medium,fast=false]",
                "Claude Opus 5.5 Medium",
            ),
            (
                "gpt-5.6-sol[context=272k,reasoning=xhigh,fast=false]",
                "GPT-5.6 Sol Extra High",
            ),
            ("gemini-3.1-pro[]", "Gemini 3.1 Pro"),
            (
                "deepseek-v41-flash-1017[reasoning_effort=high]",
                "DeepSeek V41 Flash 1017 High",
            ),
            ("kimi-k3[reasoning=max]", "Kimi K3 Max"),
        ] {
            assert_eq!(model_name(value).as_deref(), Some(name));
        }
        assert_eq!(model_name("default[]"), None);
    }
}
