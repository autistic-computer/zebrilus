use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use agent_client_protocol::{AcpAgent, AcpAgentConfig};
use serde_json::Value;
use zz_protocol::{AGENT_ANTIGRAVITY_COMMAND, AgentProvider};

use crate::agent::{
    environment::agent_path,
    host::RuntimeChannels,
    runtime::{AgentSpawnConfig, RuntimeCommand, run_acp_agent},
    stream::AgentStreamPayload,
};

const ACP: &str = "acp";
const ACP_USAGE: &str = "agy acp";
const USAGE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_USAGE_BYTES: usize = 64 * 1024;
const EFFORT: &str = "effort";
const THOUGHT_LEVEL: &str = "thought_level";

pub(crate) async fn run_agy_runtime(
    config: AgentSpawnConfig,
    provider: AgentProvider,
    channels: RuntimeChannels,
) -> Result<(), String> {
    let inherited = std::env::var("PATH").ok();
    let agent = AcpAgent::new(
        AcpAgentConfig::new(program(agent_path().or(inherited.as_deref()))?).arg(ACP),
    );
    let (command_tx, command_rx) = async_channel::bounded(1);
    let (event_tx, event_rx) = async_channel::bounded(1);
    let commands = channels.commands;
    let events = channels.events;
    let outbound = async move {
        while let Ok(command) = commands.recv().await {
            if command_tx.send(to_agy(command)).await.is_err() {
                break;
            }
        }
        drop(command_tx);
        std::future::pending::<Result<(), String>>().await
    };
    let inbound = async move {
        while let Ok(payload) = event_rx.recv().await {
            if events.send(to_zz(payload)).await.is_err() {
                break;
            }
        }
    };
    let runtime = async move {
        futures_lite::future::or(
            run_acp_agent(
                agent,
                config,
                provider,
                channels.auto_approve,
                channels.permission_ids,
                channels.journal,
                command_rx,
                channels.controls,
                event_tx,
            ),
            outbound,
        )
        .await
    };
    futures_lite::future::zip(runtime, inbound).await.0
}

fn program(path: Option<&str>) -> Result<PathBuf, String> {
    let installed = path
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .map(|directory| directory.join(AGENT_ANTIGRAVITY_COMMAND))
        .filter(|candidate| candidate.is_file())
        .collect::<Vec<_>>();
    let Some(first) = installed.first() else {
        return Err(
            "Antigravity is not installed: `agy` was not found on PATH. Install it from https://antigravity.google."
                .to_owned(),
        );
    };
    installed
        .iter()
        .find(|candidate| speaks_acp(candidate))
        .cloned()
        .ok_or_else(|| {
            format!(
                "Antigravity at {} is too old for chats: it has no `agy acp`. Update it with `agy update`.",
                first.display()
            )
        })
}

fn speaks_acp(program: &Path) -> bool {
    let mut command = Command::new(program);
    command.args([ACP, "--help"]).stdin(Stdio::null());
    crate::bounded_command::run_output_until(
        command,
        MAX_USAGE_BYTES,
        Instant::now() + USAGE_TIMEOUT,
    )
    .is_ok_and(|output| {
        [output.stdout, output.stderr]
            .iter()
            .any(|text| String::from_utf8_lossy(text).contains(ACP_USAGE))
    })
}

fn to_agy(command: RuntimeCommand) -> RuntimeCommand {
    match command {
        RuntimeCommand::SetConfigOption { option_id, value } if option_id == EFFORT => {
            RuntimeCommand::SetConfigOption {
                option_id: THOUGHT_LEVEL.to_owned(),
                value,
            }
        }
        command => command,
    }
}

fn to_zz(payload: AgentStreamPayload) -> AgentStreamPayload {
    match payload {
        AgentStreamPayload::SessionReady {
            session_id,
            modes,
            mut config_options,
        } => {
            if let Some(options) = &mut config_options {
                rename_effort(options);
            }
            AgentStreamPayload::SessionReady {
                session_id,
                modes,
                config_options,
            }
        }
        AgentStreamPayload::SessionSwitched {
            session_id,
            cwd,
            modes,
            mut config_options,
            replay,
        } => {
            if let Some(options) = &mut config_options {
                rename_effort(options);
            }
            AgentStreamPayload::SessionSwitched {
                session_id,
                cwd,
                modes,
                config_options,
                replay,
            }
        }
        AgentStreamPayload::ConfigOptionsChanged {
            option_id,
            value,
            mut config_options,
        } => {
            rename_effort(&mut config_options);
            AgentStreamPayload::ConfigOptionsChanged {
                option_id: zz_option(option_id),
                value,
                config_options,
            }
        }
        AgentStreamPayload::SessionsListed {
            client,
            mut sessions,
            next_cursor,
            cwd_filter,
            replace,
        } => {
            if let Some(folder) = &cwd_filter {
                sessions.retain(|session| &session.cwd == folder);
            }
            AgentStreamPayload::SessionsListed {
                client,
                sessions,
                next_cursor,
                cwd_filter,
                replace,
            }
        }
        AgentStreamPayload::SettingFailed { option_id, message } => {
            AgentStreamPayload::SettingFailed {
                option_id: zz_option(option_id),
                message,
            }
        }
        AgentStreamPayload::Update { mut update } => match update["sessionUpdate"].as_str() {
            Some("session_info_update") if update.get("title").is_some() => {
                AgentStreamPayload::TitleChanged {
                    title: update["title"]
                        .as_str()
                        .map(str::trim)
                        .filter(|title| !title.is_empty())
                        .map(str::to_owned),
                }
            }
            Some("config_option_update") => {
                rename_effort(&mut update["configOptions"]);
                AgentStreamPayload::Update { update }
            }
            _ => AgentStreamPayload::Update { update },
        },
        payload => payload,
    }
}

fn zz_option(option_id: String) -> String {
    if option_id == THOUGHT_LEVEL {
        EFFORT.to_owned()
    } else {
        option_id
    }
}

fn rename_effort(options: &mut Value) {
    for option in options.as_array_mut().into_iter().flatten() {
        if option["id"] == THOUGHT_LEVEL {
            option["id"] = Value::from(EFFORT);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn thought_level_is_the_chat_effort_option_and_session_info_is_the_title() {
        let options = || {
            json!([
                {"id": "model", "category": "model", "currentValue": "gemini-3.8-flash"},
                {"id": "thought_level", "category": "thought_level", "currentValue": "low"},
            ])
        };
        let AgentStreamPayload::ConfigOptionsChanged {
            option_id,
            config_options,
            ..
        } = to_zz(AgentStreamPayload::ConfigOptionsChanged {
            option_id: "thought_level".to_owned(),
            value: "low".to_owned(),
            config_options: options(),
        })
        else {
            panic!("the payload kind changed");
        };
        assert_eq!(option_id, "effort");
        assert_eq!(
            config_options,
            json!([
                {"id": "model", "category": "model", "currentValue": "gemini-3.8-flash"},
                {"id": "effort", "category": "thought_level", "currentValue": "low"},
            ])
        );
        assert!(matches!(
            to_agy(RuntimeCommand::SetConfigOption {
                option_id: "effort".to_owned(),
                value: "high".to_owned(),
            }),
            RuntimeCommand::SetConfigOption { option_id, value }
                if option_id == "thought_level" && value == "high"
        ));
        assert_eq!(
            to_zz(AgentStreamPayload::Update {
                update: json!({
                    "sessionUpdate": "session_info_update",
                    "title": "Create And Read Probe File",
                }),
            }),
            AgentStreamPayload::TitleChanged {
                title: Some("Create And Read Probe File".to_owned()),
            }
        );
    }
}
