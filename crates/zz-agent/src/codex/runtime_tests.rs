use super::*;

#[test]
fn only_a_codex_binary_selects_the_native_driver() {
    let command = CodexCommand::parse("RUST_LOG=warn codex -c model=o3").expect("native");
    assert_eq!(command.program, "codex");
    assert_eq!(command.args, ["-c", "model=o3"]);
    assert_eq!(command.env, [("RUST_LOG".to_owned(), "warn".to_owned())]);
    assert_eq!(
        CodexCommand::parse("codex app-server").map(|command| command.args),
        Some(Vec::new())
    );
    assert_eq!(
        CodexCommand::parse("npx -y @agentclientprotocol/codex-acp@1.11.0"),
        None
    );
}

#[test]
fn decisions_speak_each_request_kinds_dialect() {
    assert_eq!(decision(&Reply::Command, "allow")["decision"], "accept");
    assert_eq!(
        decision(&Reply::FileChange, "allow_always")["decision"],
        "acceptForSession"
    );
    assert_eq!(decision(&Reply::Command, "reject")["decision"], "decline");
    assert_eq!(decision(&Reply::Legacy, "allow")["decision"], "approved");
    assert_eq!(decision(&Reply::Legacy, "cancel")["decision"], "abort");
    let answers = question_answers(&[AgentQuestionAnswer {
        id: "q1".to_owned(),
        answers: vec!["yes".to_owned()],
    }]);
    assert_eq!(
        answers,
        json!({ "answers": { "q1": { "answers": ["yes"] } } })
    );
}

#[test]
fn settings_map_presets_onto_turn_overrides() {
    let mut settings = Settings {
        models: vec![json!({
            "model": "gpt-a", "displayName": "A", "description": "",
            "isDefault": true, "defaultReasoningEffort": "medium",
            "supportedReasoningEfforts": [{ "reasoningEffort": "low", "description": "" },
                { "reasoningEffort": "medium", "description": "" }],
        })],
        ..Settings::default()
    };
    settings.adopt(&json!({
        "model": "gpt-a", "reasoningEffort": "medium",
        "approvalPolicy": "on-request", "sandbox": { "type": "readOnly" },
    }));
    assert_eq!(settings.mode.as_deref(), Some("read-only"));
    let options = settings.config_options();
    serde_json::from_value::<Vec<agent_client_protocol::v1::schema::SessionConfigOption>>(
        options.clone(),
    )
    .expect("ACP config options");
    assert_eq!(options.as_array().map(Vec::len), Some(3));
    let mut params = json!({});
    settings.turn_overrides(&mut params);
    assert_eq!(params, json!({}), "nothing overrides until the user picks");
    settings.mode = Some("full-access".to_owned());
    settings.mode_override = true;
    settings.turn_overrides(&mut params);
    assert_eq!(params["approvalPolicy"], "never");
    assert_eq!(params["sandboxPolicy"]["type"], "dangerFullAccess");
}

#[cfg(unix)]
const FAKE_CODEX: &str = r#"#!/bin/sh
[ "$1" = app-server ] || exit 2
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$0.log"
  id=$(printf '%s' "$line" | sed -n 's/^{\("method":"[^"]*",\)\{0,1\}"id":\([0-9][0-9]*\),.*/\2/p')
  case "$line" in
    *'"method":"config/read"'*) printf '{"id":%s,"result":{"config":{"developer_instructions":"be terse"},"origins":{}}}\n' "$id" ;;
    *'"method":"thread/resume"'*'"threadId":"t2"'*) printf '{"id":%s,"result":{"thread":{"id":"t2"}}}\n' "$id" ;;
    *'"method":"thread/resume"'*'"threadId":"gone"'*) printf '{"id":%s,"error":{"code":-32600,"message":"no rollout for gone"}}\n' "$id" ;;
    *'"method":"thread/resume"'*'"threadId":"t3"'*) printf '{"id":%s,"result":{"thread":{"id":"t3"}}}\n' "$id" ;;
    *'"method":"thread/resume"'*'"threadId":"t4"'*) printf '{"id":%s,"result":{"thread":{"id":"t4"}}}\n' "$id" ;;
    *'"method":"thread/read"'*'"threadId":"t4"'*)
      printf '{"id":%s,"result":{"thread":{"id":"t4","status":{"type":"active","activeFlags":[]},"turns":[{"id":"r1","status":"inProgress","items":[{"type":"userMessage","id":"m4","content":[{"type":"text","text":"keep going"}]}]}]}}}\n' "$id" ;;
    *'"method":"turn/interrupt"'*'"turnId":"r1"'*)
      printf '{"id":%s,"result":{}}\n' "$id"
      printf '%s\n' '{"method":"turn/completed","params":{"threadId":"t4","turn":{"id":"r1","status":"interrupted"}}}' ;;
    *'"method":"thread/read"'*'"threadId":"t3"'*)
      printf '{"id":%s,"result":{"thread":{"id":"t3","turns":[]}}}\n' "$id"
      printf '%s\n' '{"method":"item/commandExecution/requestApproval","id":901,"params":{"threadId":"t1","turnId":"u9","itemId":"exec-left","command":"rm -rf build"}}'
      printf '%s\n' '{"method":"item/commandExecution/requestApproval","id":902,"params":{"threadId":"c3","turnId":"v9","itemId":"exec-subagent","command":"ls"}}' ;;
    *'"method":"thread/read"'*'"threadId":"t2"'*)
      printf '{"id":%s,"result":{"thread":{"id":"t2","turns":[{"id":"u0","items":[{"type":"userMessage","id":"m0","clientId":null,"content":[{"type":"text","text":"one"}]}]},{"id":"u1","items":[{"type":"userMessage","id":"m1","clientId":"zz-prompt-2-x","content":[{"type":"text","text":"two"}]}]}]}}}\n' "$id" ;;
    *'"method":"thread/fork"'*'"beforeTurnId":"u1"'*) printf '{"id":%s,"result":{"thread":{"id":"f1"}}}\n' "$id" ;;
    *'"method":"thread/read"'*'"threadId":"f1"'*)
      printf '{"id":%s,"result":{"thread":{"id":"f1","turns":[{"id":"u0","items":[{"type":"userMessage","id":"m0","clientId":null,"content":[{"type":"text","text":"one"}]}]}]}}}\n' "$id" ;;
    *'"method":"thread/start"'*|*'"method":"thread/resume"'*) printf '{"id":%s,"result":{"thread":{"id":"t1"}}}\n' "$id" ;;
    *'"method":"thread/read"'*'"threadId":"t1"'*)
      printf '{"id":%s,"result":{"thread":{"id":"t1","turns":[{"id":"u0","items":[{"type":"userMessage","id":"m0","content":[{"type":"text","text":"scan"}]},{"type":"subAgentActivity","id":"a1","kind":"started","agentThreadId":"c1","agentPath":"/root/scan_files"},{"type":"subAgentActivity","id":"a2","kind":"completed","agentThreadId":"c1","agentPath":"/root/scan_files"}]}]}}}\n' "$id" ;;
    *'"method":"thread/read"'*'"threadId":"c1"'*)
      printf '{"id":%s,"result":{"thread":{"id":"c1","turns":[{"id":"v0","items":[{"type":"userMessage","id":"n0","content":[{"type":"text","text":"list"}]},{"type":"commandExecution","id":"exec-c","command":"ls","commandActions":[],"status":"completed","exitCode":0,"aggregatedOutput":"x"},{"type":"agentMessage","id":"n1","text":"found x"}]}]}}}\n' "$id" ;;
    *'"method":"turn/start"'*'"text":"ask me"'*)
      printf '{"id":%s,"result":{"turn":{"id":"u2"}}}\n' "$id"
      printf '%s\n' '{"method":"turn/started","params":{"threadId":"t1","turn":{"id":"u2"}}}'
      printf '%s\n' '{"method":"item/tool/requestUserInput","id":900,"params":{"threadId":"t1","turnId":"u2","itemId":"ask-1","questions":[{"id":"fruit","header":"Fruit","question":"Which fruit?","options":[{"label":"apple"},{"label":"pear"}]}]}}' ;;
    *'"id":900,'*'"fruit":{"answers":["pear"]}'*)
      printf '%s\n' '{"method":"turn/completed","params":{"threadId":"t1","turn":{"id":"u2","status":"completed"}}}' ;;
    *'"id":900,'*) echo "bad answer: $line" >&2; exit 4 ;;
    *'"method":"turn/start"'*'"text":"answer late"'*) (sleep 0.4; printf '{"id":%s,"result":{"turn":{"id":"u8"}}}\n' "$id") & ;;
    *'"method":"turn/start"'*'"text":"hold the id"'*) held=$id ;;
    *'"method":"thread/list"'*)
      [ -n "$held" ] && printf '{"id":%s,"result":{"turn":{"id":"u7"}}}\n' "$held"
      held=
      printf '{"id":%s,"result":{"data":[],"nextCursor":null}}\n' "$id" ;;
    *'"method":"turn/steer"'*)
      printf '%s\n' '{"method":"turn/completed","params":{"threadId":"t1","turn":{"id":"u7","status":"completed"}}}'
      printf '{"id":%s,"error":{"code":-32600,"message":"no active turn to steer"}}\n' "$id" ;;
    *'"method":"turn/start"'*)
      printf '{"id":%s,"result":{"turn":{"id":"u1"}}}\n' "$id"
      printf '%s\n' '{"method":"turn/started","params":{"threadId":"t1","turn":{"id":"u1"}}}'
      printf '%s\n' '{"method":"item/started","params":{"threadId":"t1","item":{"type":"commandExecution","id":"exec-1","command":"sleep 300","commandActions":[],"processId":"p1","status":"inProgress"}}}'
      printf '%s\n' '{"method":"turn/completed","params":{"threadId":"t1","turn":{"id":"u1","status":"completed"}}}' ;;
    *'"method":"thread/backgroundTerminals/list"'*)
      printf '{"id":%s,"result":{"data":[{"itemId":"exec-1","processId":"p1","command":"sleep 300","cwd":"/"}],"nextCursor":null}}\n' "$id" ;;
    *'"method":"thread/backgroundTerminals/terminate"'*'"processId":"p1"'*)
      printf '{"id":%s,"result":{"terminated":true}}\n' "$id"
      printf '%s\n' '{"method":"item/completed","params":{"threadId":"t1","item":{"type":"commandExecution","id":"exec-1","command":"sleep 300","commandActions":[],"processId":"p1","status":"failed","exitCode":-1}}}' ;;
    *) [ -n "$id" ] && printf '{"id":%s,"result":{}}\n' "$id" ;;
  esac
done
"#;

#[cfg(unix)]
fn fake_codex(
    directory: &Path,
) -> (
    async_channel::Sender<RuntimeCommand>,
    async_channel::Receiver<AgentStreamPayload>,
    impl std::future::Future<Output = Result<(), String>>,
) {
    let (commands, _, events, runtime) =
        fake_codex_in(directory, AgentWorkspaceEnvironment::default());
    (commands, events, runtime)
}

#[cfg(unix)]
fn fake_codex_in(
    directory: &Path,
    workspace: AgentWorkspaceEnvironment,
) -> (
    async_channel::Sender<RuntimeCommand>,
    async_channel::Sender<RuntimeControl>,
    async_channel::Receiver<AgentStreamPayload>,
    impl std::future::Future<Output = Result<(), String>>,
) {
    fake_codex_ending(directory, workspace, Arc::default())
}

#[cfg(unix)]
fn fake_codex_ending(
    directory: &Path,
    workspace: AgentWorkspaceEnvironment,
    ended: Arc<AtomicBool>,
) -> (
    async_channel::Sender<RuntimeCommand>,
    async_channel::Sender<RuntimeControl>,
    async_channel::Receiver<AgentStreamPayload>,
    impl std::future::Future<Output = Result<(), String>>,
) {
    use std::os::unix::fs::PermissionsExt as _;

    let program = directory.join("codex");
    std::fs::write(&program, FAKE_CODEX).expect("fake codex");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let command = CodexCommand::parse(&program.display().to_string()).expect("native");
    let (commands, command_rx) = async_channel::unbounded();
    let (controls, control_rx) = async_channel::unbounded();
    let (events, event_rx) = async_channel::unbounded();
    let runtime = run_codex_runtime(
        command,
        workspace,
        AgentProvider::Codex,
        RuntimeChannels {
            auto_approve: Arc::new(Mutex::new(AgentAutoApprove::Off)),
            permission_ids: Arc::new(AtomicU64::new(1)),
            journal: None,
            ended,
            commands: command_rx,
            controls: control_rx,
            events,
        },
    );
    (commands, controls, event_rx, runtime)
}

#[cfg(unix)]
async fn until(
    events: &async_channel::Receiver<AgentStreamPayload>,
    accept: fn(&AgentStreamPayload) -> bool,
) -> AgentStreamPayload {
    loop {
        let payload =
            futures_lite::future::or(async { events.recv().await.expect("event") }, async {
                smol::Timer::after(Duration::from_secs(20)).await;
                panic!("timed out waiting for the runtime");
            })
            .await;
        if accept(&payload) {
            return payload;
        }
    }
}

#[cfg(unix)]
#[test]
fn a_new_thread_carries_the_workspace_context_after_the_users_instructions() {
    let directory = tempfile::tempdir().expect("tempdir");
    let workspace = AgentWorkspaceEnvironment {
        pane: Some("%3".to_owned()),
        session: Some("work".to_owned()),
        ..AgentWorkspaceEnvironment::default()
    };
    let (commands, _, events, runtime) = fake_codex_in(directory.path(), workspace);
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: None,
            })
            .await
            .expect("open");
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::SessionReady { .. })
        })
        .await;
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
    let log = std::fs::read_to_string(directory.path().join("codex.log")).expect("log");
    let requests = log
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    let position = |method: &str| {
        requests
            .iter()
            .position(|request| request["method"] == method)
            .unwrap_or_else(|| panic!("no {method} in {log}"))
    };
    assert!(position("config/read") < position("thread/start"));
    let config = &requests[position("thread/start")]["params"]["config"];
    assert_eq!(config["shell_environment_policy.set.ZZ_PANE"], "%3");
    assert_eq!(config["shell_environment_policy.set.ZZ_SESSION"], "work");
    assert_eq!(
        config[format!(
            "shell_environment_policy.set.{}",
            zz_daemon_client::agent_context::WORKSPACE_CONTEXT_SENT_ENVIRONMENT_VARIABLE
        )],
        "1"
    );
    let instructions = requests[position("thread/start")]["params"]["developerInstructions"]
        .as_str()
        .expect("developer instructions")
        .to_owned();
    assert!(instructions.starts_with("be terse\n\n"), "{instructions}");
    assert!(instructions.contains("ZZ_PANE=%3\n"), "{instructions}");
}

#[cfg(unix)]
#[test]
fn background_terminals_fill_the_tray_and_stop_from_it() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (commands, events, runtime) = fake_codex(directory.path());
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: None,
            })
            .await
            .expect("open");
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::SessionReady { .. })
        })
        .await;
        commands
            .send(RuntimeCommand::Prompt {
                turn_id: 1,
                note: None,
                prompt: AgentPrompt {
                    text: "start a server".to_owned(),
                    ..Default::default()
                },
            })
            .await
            .expect("prompt");
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::PromptFinished { .. })
        })
        .await;
        let AgentStreamPayload::TasksChanged { tasks } = until(&events, |payload| {
            matches!(payload, AgentStreamPayload::TasksChanged { .. })
        })
        .await
        else {
            unreachable!();
        };
        assert_eq!(
            tasks,
            [AgentTaskWire {
                id: "p1".to_owned(),
                kind: "shell".to_owned(),
                description: "sleep 300".to_owned(),
                tool_call_id: Some("exec-1".to_owned()),
            }]
        );
        commands
            .send(RuntimeCommand::StopTask {
                task_id: "p1".to_owned(),
            })
            .await
            .expect("stop");
        let AgentStreamPayload::TasksChanged { tasks } = until(&events, |payload| {
            matches!(payload, AgentStreamPayload::TasksChanged { .. })
        })
        .await
        else {
            unreachable!();
        };
        assert!(tasks.is_empty());
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
}

#[cfg(unix)]
#[test]
fn a_resumed_thread_replays_subagent_steps_under_their_agent() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (commands, events, runtime) = fake_codex(directory.path());
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: Some("t1".to_owned()),
            })
            .await
            .expect("open");
        let mut updates = Vec::new();
        loop {
            match until(&events, |_| true).await {
                AgentStreamPayload::Update { update } => updates.push(update),
                AgentStreamPayload::SessionReady { .. } => break,
                _ => {}
            }
        }
        let rows = updates
            .iter()
            .filter(|update| update["sessionUpdate"] != "available_commands_update")
            .map(|update| {
                (
                    update["sessionUpdate"].as_str().unwrap_or_default(),
                    update["toolCallId"].as_str().unwrap_or_default(),
                    update["_meta"]["zz"]["parent"].as_str().unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rows,
            [
                ("user_message_chunk", "", ""),
                ("tool_call", "a1", ""),
                ("tool_call", "exec-c", "a1"),
                ("tool_call_update", "exec-c", ""),
                ("tool_call_update", "a1", ""),
                ("tool_call_update", "a1", ""),
            ]
        );
        assert_eq!(updates[1]["title"], "Scan files");
        assert_eq!(updates[4]["content"][0]["content"]["text"], "found x");
        assert_eq!(updates[5]["status"], "completed");
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
}

#[cfg(unix)]
#[test]
fn a_single_question_picked_by_option_records_the_label() {
    use zz_protocol::chat::{Answer, QuestionAnswer, RequestClose};

    let directory = tempfile::tempdir().expect("tempdir");
    let (commands, controls, events, runtime) =
        fake_codex_in(directory.path(), AgentWorkspaceEnvironment::default());
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: None,
            })
            .await
            .expect("open");
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::SessionReady { .. })
        })
        .await;
        commands
            .send(RuntimeCommand::Prompt {
                turn_id: 1,
                note: None,
                prompt: AgentPrompt {
                    text: "ask me".to_owned(),
                    ..Default::default()
                },
            })
            .await
            .expect("prompt");
        let AgentStreamPayload::PermissionRequested {
            request_id,
            options,
            ..
        } = until(&events, |payload| {
            matches!(payload, AgentStreamPayload::PermissionRequested { .. })
        })
        .await
        else {
            unreachable!();
        };
        assert_eq!(options[1]["optionId"], "answer-1");
        controls
            .send(RuntimeControl::RespondPermission {
                request_id,
                option_id: Some("answer-1".to_owned()),
            })
            .await
            .expect("answer");
        let AgentStreamPayload::Chat {
            event: ChatEvent::RequestClosed { close, .. },
        } = until(&events, |payload| {
            matches!(
                payload,
                AgentStreamPayload::Chat {
                    event: ChatEvent::RequestClosed { .. }
                }
            )
        })
        .await
        else {
            unreachable!();
        };
        assert_eq!(
            close,
            RequestClose::Answered {
                answer: Answer::Questions {
                    answers: vec![QuestionAnswer {
                        question: "fruit".to_owned(),
                        answers: vec!["pear".to_owned()],
                    }],
                },
            }
        );
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::PromptFinished { .. })
        })
        .await;
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
}

#[cfg(unix)]
#[test]
fn rewinding_to_a_prompt_row_forks_before_its_turn() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (commands, events, runtime) = fake_codex(directory.path());
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: Some("t2".to_owned()),
            })
            .await
            .expect("open");
        let mut prompts = Vec::new();
        loop {
            match until(&events, |_| true).await {
                AgentStreamPayload::Update { update }
                    if update["sessionUpdate"] == "user_message_chunk" =>
                {
                    prompts.push(update["messageId"].clone());
                }
                AgentStreamPayload::SessionReady { .. } => break,
                _ => {}
            }
        }
        assert_eq!(prompts, ["m0", "zz-prompt-2-x"]);
        let verb = |text: &str| RuntimeCommand::Verb {
            prompt: AgentPrompt {
                text: text.to_owned(),
                ..Default::default()
            },
        };
        commands.send(verb("/rewind gone")).await.expect("verb");
        let AgentStreamPayload::Update { update } = until(&events, |payload| {
            matches!(payload, AgentStreamPayload::Update { update } if update["_meta"]["zz"]["notice"] == true)
        })
        .await
        else {
            unreachable!();
        };
        assert_eq!(
            update["content"]["text"],
            "That prompt is no longer in this conversation."
        );
        commands
            .send(verb("/rewind zz-prompt-2-x"))
            .await
            .expect("verb");
        let AgentStreamPayload::SessionSwitched { session_id, .. } = until(&events, |payload| {
            matches!(payload, AgentStreamPayload::SessionSwitched { .. })
        })
        .await
        else {
            unreachable!();
        };
        assert_eq!(session_id, "f1");
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
}

#[cfg(unix)]
#[test]
fn a_failed_switch_keeps_the_thread_and_takes_the_next_prompt() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (commands, events, runtime) = fake_codex(directory.path());
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: None,
            })
            .await
            .expect("open");
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::SessionReady { .. })
        })
        .await;
        commands
            .send(RuntimeCommand::SwitchSession {
                session: AgentSessionSummary {
                    session_id: "gone".to_owned(),
                    cwd: directory.path().to_path_buf(),
                    additional_directories: Vec::new(),
                    title: None,
                    updated_at: None,
                },
            })
            .await
            .expect("switch");
        let failed = until(&events, |payload| {
            matches!(
                payload,
                AgentStreamPayload::SessionSwitchFailed { .. }
                    | AgentStreamPayload::SessionSwitched { .. }
            )
        })
        .await;
        assert_eq!(
            failed,
            AgentStreamPayload::SessionSwitchFailed {
                message: "Codex could not open the thread: no rollout for gone".to_owned(),
            }
        );
        commands
            .send(RuntimeCommand::Prompt {
                turn_id: 1,
                note: None,
                prompt: AgentPrompt {
                    text: "start a server".to_owned(),
                    ..Default::default()
                },
            })
            .await
            .expect("prompt");
        let finished = until(&events, |payload| {
            matches!(payload, AgentStreamPayload::PromptFinished { .. })
        })
        .await;
        assert_eq!(
            finished,
            AgentStreamPayload::PromptFinished {
                turn_id: 1,
                outcome: AgentPromptOutcome::Finished {
                    stop_reason: Value::from("end_turn"),
                },
            }
        );
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
    let log = std::fs::read_to_string(directory.path().join("codex.log")).expect("log");
    let turn = log
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|request| request["method"] == "turn/start")
        .expect("a turn/start");
    assert_eq!(turn["params"]["threadId"], "t1");
}

#[cfg(unix)]
#[test]
fn a_switched_chat_cancels_the_left_threads_approvals_and_shows_subagents() {
    let directory = tempfile::tempdir().expect("tempdir");
    let log = directory.path().join("codex.log");
    let (commands, events, runtime) = fake_codex(directory.path());
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: None,
            })
            .await
            .expect("open");
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::SessionReady { .. })
        })
        .await;
        commands
            .send(RuntimeCommand::SwitchSession {
                session: AgentSessionSummary {
                    session_id: "t3".to_owned(),
                    cwd: directory.path().to_path_buf(),
                    additional_directories: Vec::new(),
                    title: None,
                    updated_at: None,
                },
            })
            .await
            .expect("switch");
        let asked = until(&events, |payload| {
            matches!(payload, AgentStreamPayload::PermissionRequested { .. })
        })
        .await;
        let AgentStreamPayload::PermissionRequested { tool_call, .. } = asked else {
            unreachable!("waited for a permission");
        };
        assert_eq!(tool_call["toolCallId"], "exec-subagent");
        let deadline = Instant::now() + Duration::from_secs(20);
        while !std::fs::read_to_string(&log)
            .unwrap_or_default()
            .contains(r#""id":901,"result""#)
        {
            assert!(Instant::now() < deadline, "no answer for the left thread");
            smol::Timer::after(Duration::from_millis(10)).await;
        }
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
    let sent = std::fs::read_to_string(&log)
        .expect("log")
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    assert_eq!(
        sent.iter()
            .filter(|message| message["method"] == "thread/unsubscribe")
            .map(|message| message["params"].clone())
            .collect::<Vec<_>>(),
        [json!({ "threadId": "t1" })]
    );
    assert_eq!(
        sent.iter()
            .find(|message| message["id"] == 901 && message.get("method").is_none())
            .map(|message| message["result"].clone()),
        Some(json!({ "decision": "cancel" }))
    );
}

#[cfg(unix)]
#[test]
fn a_steer_waits_for_its_turn_and_goes_out_as_the_next_prompt_when_it_misses() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (commands, events, runtime) = fake_codex(directory.path());
    let prompt = |text: &str| AgentPrompt {
        text: text.to_owned(),
        ..Default::default()
    };
    let list = || RuntimeCommand::ListSessions {
        client: zz_protocol::ClientId(1),
        cwd: None,
        cursor: None,
        replace: true,
    };
    let seen = |payload: AgentStreamPayload| match payload {
        AgentStreamPayload::Update { update }
            if update["sessionUpdate"] == "user_message_chunk" =>
        {
            Some(
                update["content"]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            )
        }
        AgentStreamPayload::Update { update } if update["_meta"]["zz"]["notice"] == true => {
            Some(format!("notice: {}", update["content"]["text"]))
        }
        AgentStreamPayload::PromptFinished { turn_id, outcome } => {
            Some(format!("turn {turn_id}: {outcome:?}"))
        }
        _ => None,
    };
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: None,
            })
            .await
            .expect("open");
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::SessionReady { .. })
        })
        .await;
        commands
            .send(RuntimeCommand::Prompt {
                turn_id: 1,
                note: None,
                prompt: prompt("hold the id"),
            })
            .await
            .expect("prompt");
        commands
            .send(RuntimeCommand::Verb {
                prompt: prompt("/steer go left"),
            })
            .await
            .expect("steer");
        commands.send(list()).await.expect("list");
        let mut shown = Vec::new();
        loop {
            let payload = until(&events, |_| true).await;
            if payload == (AgentStreamPayload::Activity { busy: false }) {
                break;
            }
            shown.extend(seen(payload));
            if shown
                .last()
                .is_some_and(|text| text.starts_with("notice: "))
            {
                break;
            }
        }
        commands
            .send(RuntimeCommand::Verb {
                prompt: prompt("/steer go right"),
            })
            .await
            .expect("steer");
        commands.send(list()).await.expect("list");
        loop {
            let payload = until(&events, |_| true).await;
            if matches!(payload, AgentStreamPayload::SessionsListed { .. }) {
                break;
            }
            shown.extend(seen(payload));
        }
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
        shown
    };
    let (result, shown) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
    assert_eq!(
        shown,
        [
            "hold the id",
            "/steer go left",
            "turn 1: Finished { stop_reason: String(\"end_turn\") }",
            "go left",
            "go right",
        ]
    );
    let log = std::fs::read_to_string(directory.path().join("codex.log")).expect("log");
    let requests = log
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|request| {
            matches!(
                request["method"].as_str(),
                Some("turn/start" | "turn/steer" | "thread/list")
            )
        })
        .map(|request| {
            (
                request["method"].as_str().unwrap_or_default().to_owned(),
                request["params"]["expectedTurnId"].clone(),
                request["params"]["input"][0]["text"].clone(),
            )
        })
        .collect::<Vec<_>>();
    let sent = |method: &str, turn: Value, text: Value| (method.to_owned(), turn, text);
    assert_eq!(
        requests,
        [
            sent("turn/start", Value::Null, json!("hold the id")),
            sent("thread/list", Value::Null, Value::Null),
            sent("turn/steer", json!("u7"), json!("go left")),
            sent("turn/start", Value::Null, json!("go left")),
            sent("turn/start", Value::Null, json!("go right")),
            sent("thread/list", Value::Null, Value::Null),
        ],
        "{log}"
    );
}

#[cfg(unix)]
#[test]
fn a_thread_opened_mid_turn_shows_the_turn_running_until_stop_ends_it() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (commands, controls, events, runtime) =
        fake_codex_in(directory.path(), AgentWorkspaceEnvironment::default());
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: Some("t4".to_owned()),
            })
            .await
            .expect("open");
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::SessionReady { .. })
        })
        .await;
        commands
            .send(RuntimeCommand::ListSessions {
                client: zz_protocol::ClientId(1),
                cwd: None,
                cursor: None,
                replace: true,
            })
            .await
            .expect("list");
        let mut busy = Vec::new();
        loop {
            match until(&events, |_| true).await {
                AgentStreamPayload::Activity { busy: now } => busy.push(now),
                AgentStreamPayload::SessionsListed { .. } => break,
                _ => {}
            }
        }
        assert_eq!(busy, [true]);
        controls
            .send(RuntimeControl::Cancel {
                turn_id: 0,
                session_id: Some("t4".to_owned()),
            })
            .await
            .expect("stop");
        let settled = until(&events, |payload| {
            matches!(payload, AgentStreamPayload::Activity { .. })
        })
        .await;
        assert_eq!(settled, AgentStreamPayload::Activity { busy: false });
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
    let log = std::fs::read_to_string(directory.path().join("codex.log")).expect("log");
    let interrupts = log
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|request| request["method"] == "turn/interrupt")
        .map(|request| request["params"].clone())
        .collect::<Vec<_>>();
    assert_eq!(interrupts, [json!({ "threadId": "t4", "turnId": "r1" })]);
}

#[cfg(unix)]
#[test]
fn a_chat_closed_for_good_stops_the_turn_it_leaves_on_the_server() {
    let directory = tempfile::tempdir().expect("tempdir");
    let ended = Arc::new(AtomicBool::new(false));
    let (commands, _controls, events, runtime) = fake_codex_ending(
        directory.path(),
        AgentWorkspaceEnvironment::default(),
        Arc::clone(&ended),
    );
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: Some("t4".to_owned()),
            })
            .await
            .expect("open");
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::Activity { busy: true })
        })
        .await;
        ended.store(true, Ordering::Release);
        Ok(())
    };
    assert_eq!(
        smol::block_on(futures_lite::future::or(driver, runtime)),
        Ok(())
    );
    let log = directory.path().join("codex.log");
    let interrupts = || {
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|request| request["method"] == "turn/interrupt")
            .map(|request| request["params"].clone())
            .collect::<Vec<_>>()
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    while interrupts().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(interrupts(), [json!({ "threadId": "t4", "turnId": "r1" })]);
}

#[cfg(unix)]
#[test]
fn a_chat_closed_before_its_turn_start_is_answered_still_stops_the_turn() {
    let directory = tempfile::tempdir().expect("tempdir");
    let ended = Arc::new(AtomicBool::new(false));
    let (commands, _controls, events, runtime) = fake_codex_ending(
        directory.path(),
        AgentWorkspaceEnvironment::default(),
        Arc::clone(&ended),
    );
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: None,
            })
            .await
            .expect("open");
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::SessionReady { .. })
        })
        .await;
        commands
            .send(RuntimeCommand::Prompt {
                turn_id: 1,
                prompt: AgentPrompt {
                    text: "answer late".to_owned(),
                    ..Default::default()
                },
                note: None,
            })
            .await
            .expect("prompt");
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::PromptAccepted { .. })
        })
        .await;
        ended.store(true, Ordering::Release);
        Ok(())
    };
    assert_eq!(
        smol::block_on(futures_lite::future::or(driver, runtime)),
        Ok(())
    );
    let log = directory.path().join("codex.log");
    let interrupts = || {
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|request| request["method"] == "turn/interrupt")
            .map(|request| request["params"].clone())
            .collect::<Vec<_>>()
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    while interrupts().is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(interrupts(), [json!({ "threadId": "t1", "turnId": "u8" })]);
}

#[cfg(unix)]
const FAKE_NAMING_CODEX: &str = r#"#!/bin/sh
[ "$1" = app-server ] || exit 2
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$0.log"
  id=$(printf '%s' "$line" | sed -n 's/^{\("method":"[^"]*",\)\{0,1\}"id":\([0-9][0-9]*\),.*/\2/p')
  case "$line" in
    *'"method":"model/list"'*)
      printf '{"id":%s,"result":{"data":[{"model":"gpt-big","isDefault":true,"supportedReasoningEfforts":[{"reasoningEffort":"high"}]},{"model":"gpt-luna","supportedReasoningEfforts":[{"reasoningEffort":"low"}]}]}}\n' "$id" ;;
    *'"method":"thread/start"'*'"ephemeral":true'*) printf '{"id":%s,"result":{"thread":{"id":"h1","ephemeral":true}}}\n' "$id" ;;
    *'"method":"thread/start"'*) printf '{"id":%s,"result":{"thread":{"id":"t1","name":null}}}\n' "$id" ;;
    *'"method":"turn/start"'*'"threadId":"h1"'*)
      printf '{"id":%s,"result":{"turn":{"id":"h-turn"}}}\n' "$id"
      printf '%s\n' '{"method":"item/agentMessage/delta","params":{"threadId":"h1","itemId":"n","delta":"\"Fix the flaky "}}'
      printf '%s\n' '{"method":"item/agentMessage/delta","params":{"threadId":"h1","itemId":"n","delta":"resize test.\""}}'
      printf '%s\n' '{"method":"turn/completed","params":{"threadId":"h1","turn":{"id":"h-turn","status":"completed"}}}' ;;
    *'"method":"turn/start"'*'"text":"rename it"'*)
      printf '{"id":%s,"result":{"turn":{"id":"u2"}}}\n' "$id"
      printf '%s\n' '{"method":"thread/name/updated","params":{"threadId":"t1","threadName":"Renamed in the TUI"}}'
      printf '%s\n' '{"method":"turn/completed","params":{"threadId":"t1","turn":{"id":"u2","status":"completed"}}}' ;;
    *'"method":"turn/start"'*)
      printf '{"id":%s,"result":{"turn":{"id":"u1"}}}\n' "$id"
      printf '%s\n' '{"method":"turn/started","params":{"threadId":"t1","turn":{"id":"u1"}}}'
      printf '%s\n' '{"method":"turn/completed","params":{"threadId":"t1","turn":{"id":"u1","status":"completed"}}}' ;;
    *'"method":"thread/name/set"'*'"threadId":"t1"'*)
      printf '{"id":%s,"result":{}}\n' "$id"
      printf '%s\n' '{"method":"thread/name/updated","params":{"threadId":"t1","threadName":"Fix the flaky resize test"}}' ;;
    *) [ -n "$id" ] && printf '{"id":%s,"result":{}}\n' "$id" ;;
  esac
done
"#;

#[cfg(unix)]
#[test]
fn a_new_thread_is_named_on_a_throwaway_thread_and_follows_renames() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().expect("tempdir");
    let program = directory.path().join("codex");
    std::fs::write(&program, FAKE_NAMING_CODEX).expect("fake codex");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let command = CodexCommand::parse(&program.display().to_string()).expect("native");
    let (commands, command_rx) = async_channel::unbounded();
    let (_controls, control_rx) = async_channel::unbounded();
    let (events, event_rx) = async_channel::unbounded();
    let runtime = run_codex_runtime(
        command,
        AgentWorkspaceEnvironment::default(),
        AgentProvider::Codex,
        RuntimeChannels {
            auto_approve: Arc::new(Mutex::new(AgentAutoApprove::Off)),
            permission_ids: Arc::new(AtomicU64::new(1)),
            journal: None,
            ended: Arc::default(),
            commands: command_rx,
            controls: control_rx,
            events,
        },
    );
    let prompt = |turn_id, text: &str| RuntimeCommand::Prompt {
        turn_id,
        note: None,
        prompt: AgentPrompt {
            text: text.to_owned(),
            ..Default::default()
        },
    };
    let title = || async {
        let AgentStreamPayload::TitleChanged { title } = until(&event_rx, |payload| {
            matches!(payload, AgentStreamPayload::TitleChanged { .. })
        })
        .await
        else {
            unreachable!();
        };
        title
    };
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: None,
            })
            .await
            .expect("open");
        until(&event_rx, |payload| {
            matches!(payload, AgentStreamPayload::SessionReady { .. })
        })
        .await;
        commands
            .send(prompt(1, "the resize test fails on CI, fix it"))
            .await
            .expect("prompt");
        assert_eq!(title().await.as_deref(), Some("Fix the flaky resize test"));
        commands.send(prompt(2, "rename it")).await.expect("prompt");
        assert_eq!(title().await.as_deref(), Some("Renamed in the TUI"));
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
    let log = std::fs::read_to_string(directory.path().join("codex.log")).expect("log");
    let requests = log
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>();
    let sent = |method: &str| {
        requests
            .iter()
            .filter(|request| request["method"] == method)
            .map(|request| request["params"].clone())
            .collect::<Vec<_>>()
    };
    let helpers = sent("thread/start")
        .into_iter()
        .filter(|params| params["ephemeral"] == true)
        .collect::<Vec<_>>();
    assert_eq!(helpers.len(), 1, "{log}");
    assert_eq!(helpers[0]["model"], "gpt-luna");
    assert_eq!(helpers[0]["approvalPolicy"], "never");
    assert_eq!(helpers[0]["sandbox"], "read-only");
    let helper_turn = sent("turn/start")
        .into_iter()
        .find(|params| params["threadId"] == "h1")
        .expect("a turn on the throwaway thread");
    assert_eq!(helper_turn["effort"], "low");
    assert!(
        helper_turn["input"][0]["text"]
            .as_str()
            .is_some_and(|text| text.ends_with("the resize test fails on CI, fix it")),
        "{helper_turn}"
    );
    assert_eq!(
        sent("thread/name/set"),
        [json!({ "threadId": "t1", "name": "Fix the flaky resize test" })]
    );
    assert_eq!(sent("thread/unsubscribe"), [json!({ "threadId": "h1" })]);
}
