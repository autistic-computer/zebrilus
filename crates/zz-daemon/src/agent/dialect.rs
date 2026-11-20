use std::path::{Path, PathBuf};

use agent_client_protocol::AcpAgent;
use async_channel::Sender;
use parking_lot::Mutex;
use serde_json::Value;
use zz_protocol::{AgentProvider, chat::ChatEvent};

use crate::agent::{
    acp_chat::Transcript,
    host::RuntimeChannels,
    runtime::{AgentSpawnConfig, RuntimeCommand, run_acp_agent, validate_payload},
    stream::AgentStreamPayload,
};

const EFFORT: &str = "effort";
const SIGNED_OUT: &str = "Authentication required";

#[derive(Clone, Copy)]
pub(crate) struct Dialect {
    pub(crate) effort_option: &'static str,
    pub(crate) signed_out: &'static str,
    pub(crate) stored_title: fn(&str) -> Option<String>,
}

pub(crate) async fn run(
    agent: AcpAgent,
    dialect: Dialect,
    config: AgentSpawnConfig,
    provider: AgentProvider,
    channels: RuntimeChannels,
) -> Result<(), String> {
    let (command_tx, command_rx) = async_channel::bounded(1);
    let (control_tx, control_rx) = async_channel::bounded(1);
    let (event_tx, event_rx) = async_channel::bounded(1);
    let transcript = Mutex::new(Transcript::default());
    let commands = channels.commands;
    let controls = channels.controls;
    let events = channels.events;
    let outbound = async {
        while let Ok(command) = commands.recv().await {
            if command_tx
                .send(to_agent(command, dialect.effort_option))
                .await
                .is_err()
            {
                break;
            }
        }
        drop(command_tx);
        std::future::pending::<Result<(), String>>().await
    };
    let answers = async {
        while let Ok(control) = controls.recv().await {
            let chat = transcript.lock().control(&control);
            if send_chat(&events, chat).await.is_err() || control_tx.send(control).await.is_err() {
                break;
            }
        }
        drop(control_tx);
        std::future::pending::<Result<(), String>>().await
    };
    let inbound = async {
        let mut titled = false;
        while let Ok(payload) = event_rx.recv().await {
            let chat = transcript.lock().payload(&payload);
            let title = match &payload {
                AgentStreamPayload::SessionReset { restoring } => {
                    titled = false;
                    (!restoring).then_some(None)
                }
                AgentStreamPayload::SessionReady { session_id, .. }
                | AgentStreamPayload::SessionSwitched { session_id, .. }
                    if !titled =>
                {
                    (dialect.stored_title)(session_id).map(Some)
                }
                _ => None,
            };
            let payload = to_zz(payload, dialect);
            titled |= matches!(payload, AgentStreamPayload::TitleChanged { title: Some(_) });
            if events.send(payload).await.is_err() {
                break;
            }
            if let Some(title) = title
                && events
                    .send(AgentStreamPayload::TitleChanged { title })
                    .await
                    .is_err()
            {
                break;
            }
            if send_chat(&events, chat).await.is_err() {
                break;
            }
        }
    };
    let runtime = futures_lite::future::or(
        futures_lite::future::or(
            run_acp_agent(
                agent,
                config,
                provider,
                channels.auto_approve,
                channels.permission_ids,
                channels.journal,
                command_rx,
                control_rx,
                event_tx,
            ),
            outbound,
        ),
        answers,
    );
    let result = futures_lite::future::zip(runtime, inbound).await.0;
    if let Err(message) = &result {
        let chat = transcript.lock().exited(message);
        let _ = send_chat(&events, chat).await;
    }
    result
}

async fn send_chat(
    events: &Sender<AgentStreamPayload>,
    chat: Vec<ChatEvent>,
) -> Result<(), async_channel::SendError<AgentStreamPayload>> {
    for event in chat {
        let payload = AgentStreamPayload::Chat { event };
        if validate_payload(&payload).is_ok() {
            events.send(payload).await?;
        }
    }
    Ok(())
}

pub(crate) fn installed(path: Option<&str>, program: &str) -> Vec<PathBuf> {
    path.map(std::env::split_paths)
        .into_iter()
        .flatten()
        .map(|directory| directory.join(program))
        .filter(|candidate| candidate.is_file())
        .collect()
}

fn to_agent(command: RuntimeCommand, effort_option: &str) -> RuntimeCommand {
    match command {
        RuntimeCommand::SetConfigOption { option_id, value } if option_id == EFFORT => {
            RuntimeCommand::SetConfigOption {
                option_id: effort_option.to_owned(),
                value,
            }
        }
        command => command,
    }
}

fn to_zz(payload: AgentStreamPayload, dialect: Dialect) -> AgentStreamPayload {
    let effort_option = dialect.effort_option;
    match payload {
        AgentStreamPayload::PaneFailed { message } if message.starts_with(SIGNED_OUT) => {
            AgentStreamPayload::PaneFailed {
                message: dialect.signed_out.to_owned(),
            }
        }
        AgentStreamPayload::SessionReady {
            session_id,
            modes,
            mut config_options,
        } => {
            if let Some(options) = &mut config_options {
                rename_effort(options, effort_option);
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
                rename_effort(options, effort_option);
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
            rename_effort(&mut config_options, effort_option);
            AgentStreamPayload::ConfigOptionsChanged {
                option_id: zz_option(option_id, effort_option),
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
                let wanted = resolved(folder);
                sessions.retain_mut(|session| {
                    let here = session.cwd == *folder || resolved(&session.cwd) == wanted;
                    if here {
                        session.cwd.clone_from(folder);
                    }
                    here
                });
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
                option_id: zz_option(option_id, effort_option),
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
                rename_effort(&mut update["configOptions"], effort_option);
                AgentStreamPayload::Update { update }
            }
            _ => AgentStreamPayload::Update { update },
        },
        payload => payload,
    }
}

fn resolved(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

fn zz_option(option_id: String, effort_option: &str) -> String {
    if option_id == effort_option {
        EFFORT.to_owned()
    } else {
        option_id
    }
}

fn rename_effort(options: &mut Value, effort_option: &str) {
    for option in options.as_array_mut().into_iter().flatten() {
        if option["id"] == effort_option {
            option["id"] = Value::from(EFFORT);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const AGY: Dialect = Dialect {
        effort_option: "thought_level",
        signed_out: "signed out of agy",
        stored_title: |_| None,
    };
    const GROK: Dialect = Dialect {
        effort_option: "reasoning_effort",
        signed_out: "signed out of grok",
        stored_title: |_| None,
    };

    #[test]
    fn the_agents_effort_option_is_the_chat_effort_option_and_session_info_is_the_title() {
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
        } = to_zz(
            AgentStreamPayload::ConfigOptionsChanged {
                option_id: "thought_level".to_owned(),
                value: "low".to_owned(),
                config_options: options(),
            },
            AGY,
        )
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
            to_agent(
                RuntimeCommand::SetConfigOption {
                    option_id: "effort".to_owned(),
                    value: "high".to_owned(),
                },
                "reasoning_effort",
            ),
            RuntimeCommand::SetConfigOption { option_id, value }
                if option_id == "reasoning_effort" && value == "high"
        ));
        assert_eq!(
            to_zz(
                AgentStreamPayload::Update {
                    update: json!({
                        "sessionUpdate": "session_info_update",
                        "title": "Create And Read Probe File",
                    }),
                },
                GROK,
            ),
            AgentStreamPayload::TitleChanged {
                title: Some("Create And Read Probe File".to_owned()),
            }
        );
        assert_eq!(
            to_zz(
                AgentStreamPayload::PaneFailed {
                    message: "Authentication required: \"no auth method id provided\"".to_owned(),
                },
                GROK,
            ),
            AgentStreamPayload::PaneFailed {
                message: "signed out of grok".to_owned(),
            }
        );
    }
}
