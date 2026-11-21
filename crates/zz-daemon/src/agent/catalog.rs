use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicU64},
    time::Duration,
};

use parking_lot::Mutex;
use serde_json::Value;
use zz_protocol::{AgentAutoApprove, AgentProvider};

use super::{
    host::{PaneRunner, RuntimeChannels, pane_runner},
    runtime::{AgentSpawnConfig, RuntimeCommand, RuntimeControl},
    stream::AgentStreamPayload,
};

pub(crate) async fn load(
    mut config: AgentSpawnConfig,
    provider: AgentProvider,
    cwd: PathBuf,
) -> Result<Value, String> {
    config.auto_approve = AgentAutoApprove::Off;
    load_with_runner(cwd, pane_runner(config, provider)).await
}

const APPLIED_WAIT: Duration = Duration::from_secs(3);

fn names_model(options: &Value) -> bool {
    options
        .as_array()
        .and_then(|options| options.iter().find(|option| option["id"] == "model"))
        .is_none_or(|model| {
            [&model["currentValue"], &model["_meta"]["zz"]["applied"]]
                .into_iter()
                .any(|value| value.as_str().is_some_and(|value| !value.is_empty()))
        })
}

async fn load_with_runner(cwd: PathBuf, runner: PaneRunner) -> Result<Value, String> {
    let (commands, command_rx) = async_channel::bounded(2);
    let (controls, control_rx) = async_channel::bounded(4);
    let (events, event_rx) = async_channel::bounded(32);
    commands
        .try_send(RuntimeCommand::Open {
            cwd,
            resume_session: None,
        })
        .map_err(|error| error.to_string())?;
    let closer = events.clone();
    let runtime = async move {
        let result = runner(RuntimeChannels {
            auto_approve: Arc::new(Mutex::new(AgentAutoApprove::Off)),
            permission_ids: Arc::new(AtomicU64::new(1)),
            journal: None,
            commands: command_rx,
            controls: control_rx,
            events,
        })
        .await;
        closer.close();
        result
    };
    let collect = async move {
        let mut result = None;
        let mut wait = None;
        loop {
            let event = match wait.as_mut() {
                Some(timer) => {
                    futures_lite::future::or(async { Some(event_rx.recv().await) }, async {
                        timer.await;
                        None
                    })
                    .await
                }
                None => Some(event_rx.recv().await),
            };
            let Some(event) = event else {
                wait = None;
                let _ = commands.send(RuntimeCommand::Shutdown).await;
                continue;
            };
            let Ok(event) = event else {
                break;
            };
            match event {
                AgentStreamPayload::SessionReady { config_options, .. } => {
                    let options = config_options.unwrap_or_else(|| serde_json::json!([]));
                    if names_model(&options) {
                        let _ = commands.send(RuntimeCommand::Shutdown).await;
                    } else {
                        wait = Some(smol::Timer::after(APPLIED_WAIT));
                    }
                    result = Some(Ok(options));
                }
                AgentStreamPayload::Update { update }
                    if wait.is_some() && update["sessionUpdate"] == "config_option_update" =>
                {
                    wait = None;
                    result = Some(Ok(update["configOptions"].clone()));
                    let _ = commands.send(RuntimeCommand::Shutdown).await;
                }
                AgentStreamPayload::PaneFailed { message }
                | AgentStreamPayload::AuthenticationFailed { message } => {
                    result = Some(Err(message));
                    let _ = commands.send(RuntimeCommand::Shutdown).await;
                }
                AgentStreamPayload::PermissionRequested { request_id, .. } => {
                    let _ = controls
                        .send(RuntimeControl::RespondPermission {
                            request_id,
                            option_id: None,
                        })
                        .await;
                }
                _ => {}
            }
        }
        result
    };
    futures_lite::future::race(
        async {
            let (runtime, result) = futures_lite::future::zip(runtime, collect).await;
            match result {
                Some(result) => result,
                None => Err(runtime.err().unwrap_or_else(|| {
                    "The agent closed before returning its model catalog.".into()
                })),
            }
        },
        async {
            smol::Timer::after(Duration::from_secs(35)).await;
            Err("Timed out loading the agent model catalog.".into())
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_opens_an_isolated_session_and_shuts_it_down_without_prompting() {
        let runner: PaneRunner = Box::new(|channels| {
            Box::pin(async move {
                assert!(channels.journal.is_none());
                assert!(
                    matches!(channels.commands.recv().await.unwrap(), RuntimeCommand::Open { cwd, resume_session: None } if cwd == std::path::Path::new("/workspace"))
                );
                channels
                    .events
                    .send(AgentStreamPayload::SessionReady {
                        session_id: "temporary".into(),
                        modes: None,
                        config_options: Some(
                            serde_json::json!([{ "id": "model", "currentValue": "fast" }]),
                        ),
                    })
                    .await
                    .unwrap();
                assert!(matches!(
                    channels.commands.recv().await.unwrap(),
                    RuntimeCommand::Shutdown
                ));
                Ok(())
            })
        });
        let catalog =
            futures_lite::future::block_on(load_with_runner("/workspace".into(), runner)).unwrap();
        assert_eq!(catalog[0]["currentValue"], "fast");
    }

    #[test]
    fn catalog_waits_for_the_model_in_use() {
        let runner: PaneRunner = Box::new(|channels| {
            Box::pin(async move {
                channels.commands.recv().await.unwrap();
                let options = |meta: Value| serde_json::json!([{ "id": "model", "currentValue": "", "_meta": meta }]);
                channels
                    .events
                    .send(AgentStreamPayload::SessionReady {
                        session_id: "temporary".into(),
                        modes: None,
                        config_options: Some(options(Value::Null)),
                    })
                    .await
                    .unwrap();
                futures_lite::future::yield_now().await;
                assert!(channels.commands.try_recv().is_err());
                channels
                    .events
                    .send(AgentStreamPayload::Update {
                        update: serde_json::json!({
                            "sessionUpdate": "config_option_update",
                            "configOptions": options(serde_json::json!({ "zz": { "applied": "sonnet" } })),
                        }),
                    })
                    .await
                    .unwrap();
                assert!(matches!(
                    channels.commands.recv().await.unwrap(),
                    RuntimeCommand::Shutdown
                ));
                Ok(())
            })
        });
        let catalog =
            futures_lite::future::block_on(load_with_runner("/workspace".into(), runner)).unwrap();
        assert_eq!(catalog[0]["_meta"]["zz"]["applied"], "sonnet");
    }

    #[test]
    fn catalog_reads_the_advertised_options_over_acp() {
        use agent_client_protocol::{
            Agent,
            v1::schema::{
                InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse,
            },
        };
        let agent = Agent.v1()
            .on_receive_request(async |initialize: InitializeRequest, responder, _| {
                responder.respond(InitializeResponse::new(initialize.protocol_version))
            }, agent_client_protocol::on_receive_request!())
            .on_receive_request(async |request: NewSessionRequest, responder, _| {
                assert_eq!(request.cwd, PathBuf::from("/workspace"));
                let response: NewSessionResponse = serde_json::from_value(serde_json::json!({
                    "sessionId": "catalog-only",
                    "configOptions": [{"id": "model", "name": "Model", "category": "model", "type": "select", "currentValue": "fast", "options": [{"value": "fast", "name": "Fast"}]}]
                })).unwrap();
                responder.respond(response)
            }, agent_client_protocol::on_receive_request!());
        let runner: PaneRunner = Box::new(move |channels| {
            Box::pin(super::super::runtime::run_agent_connection(
                AgentProvider::ClaudeCode,
                channels.auto_approve,
                agent,
                channels.permission_ids,
                None,
                channels.commands,
                channels.controls,
                channels.events,
                None,
            ))
        });
        let result =
            futures_lite::future::block_on(load_with_runner("/workspace".into(), runner)).unwrap();
        assert_eq!(result[0]["currentValue"], "fast");
        assert_eq!(result[0]["options"][0]["name"], "Fast");
    }

    #[test]
    fn catalog_reports_startup_failure() {
        let runner: PaneRunner = Box::new(|_| Box::pin(async { Err("not authenticated".into()) }));
        assert_eq!(
            futures_lite::future::block_on(load_with_runner("/workspace".into(), runner)),
            Err("not authenticated".into())
        );
    }
}
