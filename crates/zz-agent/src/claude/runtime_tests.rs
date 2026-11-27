use super::*;

#[test]
fn a_mention_sends_the_note_as_a_reminder_the_transcript_leaves_out() {
    let prompt = |text: &str| crate::stream::AgentPrompt {
        text: text.to_owned(),
        ..Default::default()
    };
    let note = zz_protocol::agent_mention::mention_note("@+codex fix it").expect("note");
    let content = prompt_content(&prompt("@+codex fix it"), Some(&note));
    assert_eq!(
        content,
        json!([
            { "type": "text", "text": "@+codex fix it" },
            { "type": "text", "text": format!("<system-reminder>\n{note}\n</system-reminder>") },
        ])
    );
    let frame = json!({ "type": "user", "message": { "role": "user", "content": content } });
    assert_eq!(
        translate::user_prompt(&frame),
        Some(("@+codex fix it".to_owned(), Vec::new()))
    );
    assert_eq!(
        prompt_content(&prompt("fix it"), None),
        json!([{ "type": "text", "text": "fix it" }])
    );
}

#[test]
fn only_a_claude_binary_selects_the_native_driver() {
    assert_eq!(
        ClaudeCommand::parse("claude"),
        Some(ClaudeCommand {
            program: "claude".to_owned(),
            args: Vec::new(),
            env: Vec::new(),
        })
    );
    let command = ClaudeCommand::parse("FOO=1 /opt/bin/claude --model opus").expect("native");
    assert_eq!(command.program, "/opt/bin/claude");
    assert_eq!(command.env, [("FOO".to_owned(), "1".to_owned())]);
    assert_eq!(command.model_flag().as_deref(), Some("opus"));
    assert_eq!(
        ClaudeCommand::parse("npx -y @agentclientprotocol/claude-agent-acp@0.76.0"),
        None
    );
    assert_eq!(ClaudeCommand::parse("claude-agent-acp --stdio"), None);
}

#[test]
fn a_user_system_prompt_keeps_the_workspace_context_out() {
    let sets = |command: &str| {
        ClaudeCommand::parse(command)
            .expect("native")
            .sets_system_prompt()
    };
    assert!(!sets("claude --model opus"));
    assert!(!sets("claude --system-prompts-are-not-a-flag"));
    assert!(sets("claude --append-system-prompt be-brief"));
    assert!(sets("claude --system-prompt=terse"));
    assert!(sets("claude --append-system-prompt-file /tmp/p.md"));
}

#[test]
fn a_user_allowed_tools_flag_keeps_the_zz_rule_out() {
    let sets = |command: &str| {
        ClaudeCommand::parse(command)
            .expect("native")
            .sets_allowed_tools()
    };
    assert!(!sets("claude --model opus"));
    assert!(sets("claude --allowedTools Bash(git:*)"));
    assert!(sets("claude --allowed-tools=Read"));
}

#[test]
fn session_ids_are_v4_uuids() {
    let id = new_uuid();
    assert!(sessions::valid_uuid(&id), "{id}");
    assert_eq!(&id[14..15], "4");
    assert_ne!(id, new_uuid());
}

#[test]
fn settings_offer_models_modes_and_the_current_models_effort() {
    let mut settings = Settings::new(&ClaudeCommand::parse("claude").expect("native"));
    let levels = json!(["low", "medium", "high", "xhigh", "max"]);
    let model = |value: &str, resolved: &str, display: &str, description: &str| json!({"value":value,"resolvedModel":resolved,"displayName":display,"description":description,"supportsEffort":true,"supportedEffortLevels":levels,"supportsAutoMode":true});
    settings.models = vec![
        model(
            "default",
            "claude-opus-5-5",
            "Default (recommended)",
            "Opus 5.5 · Best for everyday, complex tasks",
        ),
        model(
            "opus",
            "claude-opus-5-5",
            "Opus",
            "Opus 5.5 · Best for everyday, complex tasks",
        ),
        model(
            "claude-fable-5-2[1m]",
            "claude-fable-5-2",
            "Fable",
            "Fable 5.2 · Most capable for your hardest and longest-running tasks",
        ),
        model(
            "sonnet",
            "claude-sonnet-5-5",
            "Sonnet",
            "Sonnet 5.5 · Efficient for routine tasks",
        ),
        json!({"value":"haiku","resolvedModel":"claude-haiku-5-5","displayName":"Haiku","description":"Haiku 5.5 · Fastest for quick answers"}),
    ];
    let names = |options: &Value, index: usize, key: &str| {
        options[index]["options"]
            .as_array()
            .expect("choices")
            .iter()
            .map(|choice| choice[key].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
    };
    let unknown = settings.config_options();
    assert_eq!(unknown[0]["currentValue"], "");
    assert!(unknown[0].get("_meta").is_none());
    assert_eq!(unknown[2]["currentValue"], "");
    assert_eq!(
        settings.note_applied(&json!({"applied":{"model":"claude-sonnet-5-5","effort":"low"}})),
        Some(true)
    );
    assert_eq!(settings.note_applied(&json!({"error":"unknown"})), None);
    let options = settings.config_options();
    let ids = options
        .as_array()
        .expect("options")
        .iter()
        .map(|option| option["id"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["model", "mode", "effort"]);
    assert_eq!(
        names(&options, 0, "name"),
        ["Opus 5.5", "Fable 5.2", "Sonnet 5.5", "Haiku 5.5"]
    );
    assert_eq!(options[0]["currentValue"], "");
    assert_eq!(options[0]["_meta"]["zz"]["applied"], "sonnet");
    assert_eq!(
        names(&options, 2, "name"),
        ["Low", "Medium", "High", "Xhigh", "Max"]
    );
    assert_eq!(options[2]["currentValue"], "");
    assert_eq!(options[2]["_meta"]["zz"]["applied"], "low");
    assert!(!options.to_string().contains("Default"));
    let parsed = serde_json::from_value::<
        Vec<agent_client_protocol::v1::schema::SessionConfigOption>,
    >(options.clone())
    .expect("ACP config options");
    assert_eq!(parsed.len(), 3);
    assert_eq!(
        names(&options, 1, "value"),
        [
            "default",
            "acceptEdits",
            "plan",
            "auto",
            "bypassPermissions"
        ]
    );
    for alias in ["fable", "claude-fable-5-2", "claude-fable-5-2[1m]"] {
        settings.model = alias.to_owned();
        let options = settings.config_options();
        assert_eq!(
            options[0]["currentValue"], "claude-fable-5-2[1m]",
            "{alias}"
        );
    }
    settings.model = "claude-opus-4-1".to_owned();
    assert_eq!(
        settings.config_options()[0]["currentValue"],
        "claude-opus-4-1"
    );
    settings.model = "haiku".to_owned();
    let options = settings.config_options();
    assert_eq!(options.as_array().map(Vec::len), Some(2));
    assert_eq!(
        names(&options, 1, "value"),
        ["default", "acceptEdits", "plan", "bypassPermissions"]
    );
    serde_json::from_value::<agent_client_protocol::v1::schema::SessionModeState>(
        settings.mode_state(),
    )
    .expect("ACP mode state");
}

#[test]
fn oversized_updates_shed_raw_input_before_content() {
    let big = "x".repeat(zz_protocol::MAX_AGENT_RESULT_BYTES);
    let update = fit_update(json!({
        "sessionUpdate": "tool_call",
        "toolCallId": "t",
        "title": "Write a",
        "rawInput": { "content": big },
        "content": [{ "type": "content", "content": { "type": "text", "text": "small" } }],
    }));
    assert!(update.get("rawInput").is_none());
    assert_eq!(update["content"][0]["content"]["text"], "small");
    let update = fit_update(json!({
        "sessionUpdate": "agent_message_chunk",
        "messageId": "m",
        "content": { "type": "text", "text": big },
    }));
    assert!(validate_payload(&AgentStreamPayload::Update { update }).is_ok());
}

#[cfg(unix)]
const FAKE_CLAUDE: &str = r#"#!/bin/sh
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"zz-1","response":{"commands":[{"name":"compact","description":"Compact","argumentHint":""}],"models":[{"value":"default","displayName":"Default","description":"Opus"}],"current_permission_mode":"default"}}}'
read -r probe
printf '%s\n' '{"type":"control_response","response":{"subtype":"error","request_id":"zz-2","error":"Unsupported control request subtype: get_settings"}}'
read -r usage
case "$usage" in *'"request_id":"zz-3"'*'"subtype":"get_usage"'*'"skip_behaviors":true'*) ;; *) echo "bad usage: $usage" >&2; exit 6 ;; esac
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"zz-3","response":{"session":{"total_cost_usd":0,"model_usage":{}},"subscription_type":"max","rate_limits_available":true,"rate_limits":{"five_hour":null,"seven_day":{"utilization":2,"resets_at":"2026-11-20T22:00:00.220882+00:00"},"seven_day_opus":null},"behaviors":null}}}'
read -r prompt
case "$prompt" in *'"text":"hello"'*) ;; *) echo "bad prompt: $prompt" >&2; exit 3 ;; esac
read -r title
case "$title" in *'"request_id":"zz-4"'*'"generate_session_title"'*'"description":"hello"'*'"persist":true'*) ;; *) echo "bad title: $title" >&2; exit 5 ;; esac
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"zz-4","response":{"title":"Say hello"}}}'
printf '%s\n' '{"type":"system","subtype":"session_state_changed","state":"running"}'
printf '%s\n' '{"type":"assistant","parent_tool_use_id":null,"message":{"id":"m1","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"rm -rf build"}}]}}'
printf '%s\n' '{"type":"control_request","request_id":"perm-1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"rm -rf build"},"tool_use_id":"t1","permission_suggestions":[]}}'
read -r answer
case "$answer" in *'"request_id":"perm-1"'*'"behavior":"allow"'*) ;; *) echo "bad answer: $answer" >&2; exit 4 ;; esac
printf '%s\n' '{"type":"user","parent_tool_use_id":null,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"removed"}]},"tool_use_result":{"stdout":"removed","stderr":""}}'
printf '%s\n' '{"type":"assistant","parent_tool_use_id":null,"message":{"id":"m2","content":[{"type":"text","text":"done"}]}}'
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"usage":{"input_tokens":5,"output_tokens":2},"modelUsage":{"m":{"contextWindow":1000}},"total_cost_usd":0.01}'
printf '%s\n' '{"type":"system","subtype":"session_state_changed","state":"idle"}'
printf '%s\n' '{"type":"system","subtype":"session_title_changed","title":"Greet the user"}'
while read -r line; do :; done
"#;

#[cfg(unix)]
#[test]
fn a_turn_runs_end_to_end_against_a_claude_process() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().expect("tempdir");
    let program = directory.path().join("claude");
    std::fs::write(&program, FAKE_CLAUDE).expect("fake claude");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let command = ClaudeCommand::parse(&program.display().to_string()).expect("native");
    let (commands, command_rx) = async_channel::unbounded();
    let (controls, control_rx) = async_channel::unbounded();
    let (events, event_rx) = async_channel::unbounded();
    let runtime = run_claude_runtime(
        command,
        AgentWorkspaceEnvironment::default(),
        AgentProvider::ClaudeCode,
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
    let driver = async {
        let next = || async {
            futures_lite::future::or(
                async {
                    loop {
                        let event = event_rx.recv().await.expect("event");
                        if !matches!(event, AgentStreamPayload::Chat { .. }) {
                            break event;
                        }
                    }
                },
                async {
                    smol::Timer::after(Duration::from_secs(20)).await;
                    panic!("timed out waiting for the runtime");
                },
            )
            .await
        };
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: None,
            })
            .await
            .expect("open");
        assert!(
            matches!(next().await, AgentStreamPayload::Ready { agent_key, .. } if agent_key == AGENT_KEY)
        );
        assert!(matches!(
            next().await,
            AgentStreamPayload::SessionReset { restoring: false }
        ));
        let AgentStreamPayload::SessionReady {
            session_id,
            config_options,
            ..
        } = next().await
        else {
            panic!("expected the session to be ready");
        };
        assert!(sessions::valid_uuid(&session_id));
        assert_eq!(config_options.expect("options")[0]["id"], "model");
        assert!(
            matches!(next().await, AgentStreamPayload::Update { update } if update["sessionUpdate"] == "available_commands_update")
        );
        assert_eq!(
            next().await,
            AgentStreamPayload::QuotaChanged {
                quota: zz_protocol::QuotaWindows {
                    five_hour: None,
                    week: Some(zz_protocol::QuotaWindow {
                        used: 0.02,
                        resets_at: Some(1_795_212_000),
                    }),
                },
            }
        );
        commands
            .send(RuntimeCommand::Prompt {
                turn_id: 1,
                note: None,
                prompt: crate::stream::AgentPrompt {
                    text: "hello".to_owned(),
                    ..Default::default()
                },
            })
            .await
            .expect("prompt");
        assert!(
            matches!(next().await, AgentStreamPayload::Update { update } if update["sessionUpdate"] == "user_message_chunk")
        );
        assert!(matches!(
            next().await,
            AgentStreamPayload::PromptAccepted { turn_id: 1 }
        ));
        assert!(matches!(
            next().await,
            AgentStreamPayload::TitleChanged { title } if title.as_deref() == Some("Say hello")
        ));
        assert!(
            matches!(next().await, AgentStreamPayload::Update { update } if update["title"] == "rm -rf build")
        );
        let AgentStreamPayload::PermissionRequested {
            request_id,
            tool_call,
            options,
            ..
        } = next().await
        else {
            panic!("expected a permission request");
        };
        let tool_call =
            serde_json::from_value::<agent_client_protocol::v1::schema::ToolCallUpdate>(tool_call)
                .expect("clients decode the tool call");
        assert_eq!(tool_call.tool_call_id.0.as_ref(), "t1");
        let options = serde_json::from_value::<
            Vec<agent_client_protocol::v1::schema::PermissionOption>,
        >(options)
        .expect("clients decode the options");
        assert_eq!(options[0].option_id.0.as_ref(), "allow");
        controls
            .send(RuntimeControl::RespondPermission {
                request_id,
                option_id: Some("allow".to_owned()),
            })
            .await
            .expect("answer");
        assert!(matches!(
            next().await,
            AgentStreamPayload::PermissionResolved {
                canceled: false,
                ..
            }
        ));
        assert!(
            matches!(next().await, AgentStreamPayload::Update { update } if update["status"] == "completed" && update["content"][0]["content"]["text"] == "removed")
        );
        assert!(
            matches!(next().await, AgentStreamPayload::Update { update } if update["content"]["text"] == "done")
        );
        assert!(
            matches!(next().await, AgentStreamPayload::Update { update } if update["sessionUpdate"] == "usage_update" && update["size"] == 1000)
        );
        assert!(matches!(
            next().await,
            AgentStreamPayload::PromptFinished { turn_id: 1, outcome: AgentPromptOutcome::Finished { stop_reason } } if stop_reason == "end_turn"
        ));
        let renamed = loop {
            if let AgentStreamPayload::TitleChanged { title } = next().await {
                break title;
            }
        };
        assert_eq!(renamed.as_deref(), Some("Greet the user"));
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
}

#[cfg(unix)]
const FAKE_ACCOUNT_CLAUDE: &str = r#"#!/bin/sh
printf '%s\n%s' "${CLAUDE_CONFIG_DIR-unset}" "${ANTHROPIC_API_KEY-unset}" > seen
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"zz-1","response":{"commands":[],"models":[],"current_permission_mode":"default"}}}'
while read -r line; do :; done
"#;

#[cfg(unix)]
fn opened_with(
    program: &Path,
    workspace: AgentWorkspaceEnvironment,
) -> (AgentStreamPayload, Option<String>) {
    let directory = tempfile::tempdir().expect("tempdir");
    let command = ClaudeCommand::parse(&program.display().to_string()).expect("native");
    let (commands, command_rx) = async_channel::unbounded();
    let (_controls, control_rx) = async_channel::unbounded();
    let (events, event_rx) = async_channel::unbounded();
    let runtime = run_claude_runtime(
        command,
        workspace,
        AgentProvider::ClaudeCode,
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
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: None,
            })
            .await
            .expect("open");
        let first = futures_lite::future::or(
            async {
                loop {
                    let event = event_rx.recv().await.expect("event");
                    if matches!(
                        event,
                        AgentStreamPayload::Ready { .. } | AgentStreamPayload::PaneFailed { .. }
                    ) {
                        break event;
                    }
                }
            },
            async {
                smol::Timer::after(Duration::from_secs(20)).await;
                panic!("timed out waiting for the runtime");
            },
        )
        .await;
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
        first
    };
    let (_, first) = smol::block_on(futures_lite::future::zip(runtime, driver));
    let seen = std::fs::read_to_string(directory.path().join("seen")).ok();
    (first, seen)
}

#[cfg(unix)]
#[test]
fn a_managed_login_runs_claude_without_inherited_login_variables() {
    use std::os::unix::fs::PermissionsExt as _;

    const CHILD: &str = "ZZ_TEST_ACCOUNT_LOGIN_OVERRIDES";
    if std::env::var_os(CHILD).is_none() {
        let status = Command::new(std::env::current_exe().expect("test binary"))
            .args([
                "--exact",
                concat!(
                    module_path!(),
                    "::a_managed_login_runs_claude_without_inherited_login_variables"
                )
                .split_once("::")
                .expect("crate-qualified test path")
                .1,
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .env("ANTHROPIC_API_KEY", "zz-test-key")
            .status()
            .expect("isolated login environment");
        assert!(status.success());
        return;
    }
    let directory = tempfile::tempdir().expect("tempdir");
    let program = directory.path().join("claude");
    std::fs::write(&program, FAKE_ACCOUNT_CLAUDE).expect("fake claude");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let config = std::env::var("CLAUDE_CONFIG_DIR");
    let config = config.as_deref().unwrap_or("unset");

    let managed = AgentWorkspaceEnvironment {
        managed_login: true,
        ..AgentWorkspaceEnvironment::default()
    };
    let (ready, seen) = opened_with(&program, managed);
    assert!(
        matches!(ready, AgentStreamPayload::Ready { .. }),
        "{ready:?}"
    );
    assert_eq!(seen, Some(format!("{config}\nunset")));

    let (ready, seen) = opened_with(&program, AgentWorkspaceEnvironment::default());
    assert!(
        matches!(ready, AgentStreamPayload::Ready { .. }),
        "{ready:?}"
    );
    assert_eq!(seen, Some(format!("{config}\nzz-test-key")));
}

#[cfg(unix)]
const FAKE_RESPAWN_CLAUDE: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> spawns
[ $(wc -l < spawns) -eq "${FAIL_SPAWN:-0}" ] && exit 3
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"request_id":"\(zz-[0-9]*\)".*/\1/p')
  case "$line" in
    *'"subtype":"initialize"'*) printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":{"commands":[],"models":[{"value":"opus","supportsEffort":true,"supportedEffortLevels":["low","high"]}],"current_permission_mode":"default"}}}\n' "$id" ;;
    *'"type":"control_request"'*) printf '{"type":"control_response","response":{"subtype":"success","request_id":"%s","response":{}}}\n' "$id" ;;
    *'"type":"user"'*)
      printf '%s\n' '{"type":"system","subtype":"session_state_changed","state":"running"}'
      printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1}'
      printf '%s\n' '{"type":"system","subtype":"session_state_changed","state":"idle"}' ;;
  esac
done
"#;

#[cfg(unix)]
fn respawning(
    directory: &Path,
    fail_spawn: Option<usize>,
) -> (
    async_channel::Sender<RuntimeCommand>,
    async_channel::Receiver<AgentStreamPayload>,
    impl std::future::Future<Output = Result<(), String>>,
) {
    use std::os::unix::fs::PermissionsExt as _;

    let program = directory.join("claude");
    std::fs::write(&program, FAKE_RESPAWN_CLAUDE).expect("fake claude");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let line = match fail_spawn {
        Some(spawn) => format!("FAIL_SPAWN={spawn} {}", program.display()),
        None => program.display().to_string(),
    };
    let command = ClaudeCommand::parse(&line).expect("native");
    let (commands, command_rx) = async_channel::unbounded();
    let (_controls, control_rx) = async_channel::unbounded();
    let (events, event_rx) = async_channel::unbounded();
    let runtime = run_claude_runtime(
        command,
        AgentWorkspaceEnvironment::default(),
        AgentProvider::ClaudeCode,
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
    (commands, event_rx, runtime)
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
fn spawned_flags(directory: &Path, flag: &str) -> Vec<Option<String>> {
    std::fs::read_to_string(directory.join("spawns"))
        .expect("spawns")
        .lines()
        .map(|line| {
            let mut words = line.split_whitespace();
            words.find(|word| *word == flag)?;
            words.next().map(str::to_owned)
        })
        .collect()
}

#[cfg(unix)]
#[test]
fn a_failed_new_chat_resumes_the_session_it_left() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (commands, events, runtime) = respawning(directory.path(), Some(2));
    let cwd = directory.path().to_path_buf();
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: cwd.clone(),
                resume_session: None,
            })
            .await
            .expect("open");
        let AgentStreamPayload::SessionReady { session_id, .. } = until(&events, |payload| {
            matches!(payload, AgentStreamPayload::SessionReady { .. })
        })
        .await
        else {
            unreachable!();
        };
        commands
            .send(RuntimeCommand::NewSession { cwd: cwd.clone() })
            .await
            .expect("new chat");
        let failed = until(&events, |payload| {
            matches!(
                payload,
                AgentStreamPayload::SessionSwitchFailed { .. }
                    | AgentStreamPayload::SessionSwitched { .. }
                    | AgentStreamPayload::SessionReset { .. }
                    | AgentStreamPayload::PaneFailed { .. }
            )
        })
        .await;
        assert_eq!(
            failed,
            AgentStreamPayload::SessionSwitchFailed {
                message: "Claude Code exited (exit status: 3)".to_owned(),
            }
        );
        commands
            .send(RuntimeCommand::Prompt {
                turn_id: 1,
                note: None,
                prompt: crate::stream::AgentPrompt {
                    text: "hello".to_owned(),
                    ..Default::default()
                },
            })
            .await
            .expect("prompt");
        let finished = until(&events, |payload| {
            matches!(
                payload,
                AgentStreamPayload::PromptFinished { .. } | AgentStreamPayload::PaneFailed { .. }
            )
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
        session_id
    };
    let (result, session_id) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
    let sessions = spawned_flags(directory.path(), "--session-id");
    assert_eq!(sessions.len(), 3, "{sessions:?}");
    assert_eq!(sessions[0].as_deref(), Some(session_id.as_str()));
    assert_eq!(sessions[2].as_deref(), Some(session_id.as_str()));
}

#[cfg(unix)]
#[test]
fn a_new_chat_starts_on_the_picked_model_effort_and_mode() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (commands, events, runtime) = respawning(directory.path(), None);
    let cwd = directory.path().to_path_buf();
    let driver = async {
        commands
            .send(RuntimeCommand::Open {
                cwd: cwd.clone(),
                resume_session: None,
            })
            .await
            .expect("open");
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::SessionReady { .. })
        })
        .await;
        for (option_id, value) in [
            ("model", "opus"),
            ("effort", "high"),
            ("mode", "acceptEdits"),
        ] {
            commands
                .send(RuntimeCommand::SetConfigOption {
                    option_id: option_id.to_owned(),
                    value: value.to_owned(),
                })
                .await
                .expect("pick");
            until(&events, |payload| {
                matches!(payload, AgentStreamPayload::ConfigOptionsChanged { .. })
            })
            .await;
        }
        commands
            .send(RuntimeCommand::NewSession { cwd: cwd.clone() })
            .await
            .expect("new chat");
        until(&events, |payload| {
            matches!(payload, AgentStreamPayload::SessionSwitched { .. })
        })
        .await;
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
    assert_eq!(
        spawned_flags(directory.path(), "--model"),
        [None, Some("opus".to_owned())]
    );
    assert_eq!(
        spawned_flags(directory.path(), "--effort"),
        [None, Some("high".to_owned())]
    );
    assert_eq!(
        spawned_flags(directory.path(), "--permission-mode"),
        [None, Some("acceptEdits".to_owned())]
    );
}

#[cfg(unix)]
const FAKE_SIDE_CLAUDE: &str = r#"#!/bin/sh
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"zz-1","response":{"commands":[],"models":[],"current_permission_mode":"default"}}}'
read -r probe
printf '%s\n' '{"type":"control_response","response":{"subtype":"error","request_id":"zz-2","error":"Unsupported control request subtype: get_settings"}}'
read -r usage
printf '%s\n' '{"type":"control_response","response":{"subtype":"error","request_id":"zz-3","error":"Unsupported control request subtype: get_usage"}}'
read -r side
case "$side" in *'"request_id":"zz-4"'*'"subtype":"side_question"'*'"question":"what is 2+2"'*) ;; *) echo "bad side: $side" >&2; exit 3 ;; esac
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"zz-4","response":{"response":"4","synthetic":false}}}'
read -r steer
case "$steer" in *'"type":"user"'*) ;; *) echo "bad steer: $steer" >&2; exit 5 ;; esac
case "$steer" in *'"text":"left"'*) ;; *) echo "bad steer: $steer" >&2; exit 5 ;; esac
case "$steer" in *priority*) echo "steered a turn: $steer" >&2; exit 5 ;; esac
printf '%s\n' '{"type":"system","subtype":"session_state_changed","state":"running"}'
while read -r line; do :; done
"#;

#[cfg(unix)]
#[test]
fn side_questions_answer_outside_the_conversation() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().expect("tempdir");
    let program = directory.path().join("claude");
    std::fs::write(&program, FAKE_SIDE_CLAUDE).expect("fake claude");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let command = ClaudeCommand::parse(&program.display().to_string()).expect("native");
    let (commands, command_rx) = async_channel::unbounded();
    let (_controls, control_rx) = async_channel::unbounded();
    let (events, event_rx) = async_channel::unbounded();
    let runtime = run_claude_runtime(
        command,
        AgentWorkspaceEnvironment::default(),
        AgentProvider::ClaudeCode,
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
    let driver = async {
        let text_of = |payload: &AgentStreamPayload| match payload {
            AgentStreamPayload::Update { update } => {
                update["content"]["text"].as_str().map(str::to_owned)
            }
            _ => None,
        };
        let next_text = || async {
            loop {
                let payload = futures_lite::future::or(
                    async { event_rx.recv().await.expect("event") },
                    async {
                        smol::Timer::after(Duration::from_secs(20)).await;
                        panic!("timed out waiting for the runtime");
                    },
                )
                .await;
                if let AgentStreamPayload::Update { update } = &payload
                    && update["sessionUpdate"] == "available_commands_update"
                {
                    continue;
                }
                if let Some(text) = text_of(&payload) {
                    return (text, payload);
                }
            }
        };
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: None,
            })
            .await
            .expect("open");
        let verb = |text: &str| RuntimeCommand::Verb {
            prompt: crate::stream::AgentPrompt {
                text: text.to_owned(),
                ..Default::default()
            },
        };
        while !matches!(
            event_rx.recv().await.expect("event"),
            AgentStreamPayload::SessionReady { .. }
        ) {}
        commands.send(verb("/btw what is 2+2")).await.expect("btw");
        assert_eq!(next_text().await.0, "/btw what is 2+2");
        let (answer, payload) = next_text().await;
        assert_eq!(answer, "4");
        assert!(
            matches!(payload, AgentStreamPayload::Update { update } if update["_meta"]["zz"]["side"] == true)
        );
        commands.send(verb("/steer left")).await.expect("steer");
        let (steered, payload) = next_text().await;
        assert_eq!(steered, "left");
        assert!(
            matches!(payload, AgentStreamPayload::Update { update } if update["sessionUpdate"] == "user_message_chunk")
        );
        assert_eq!(
            until(&event_rx, |payload| matches!(
                payload,
                AgentStreamPayload::Activity { .. } | AgentStreamPayload::PaneFailed { .. }
            ))
            .await,
            AgentStreamPayload::Activity { busy: true }
        );
        commands.send(verb("/what")).await.expect("unknown");
        assert_eq!(next_text().await.0, "/what");
        assert_eq!(next_text().await.0, VERB_HELP);
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
}

#[cfg(unix)]
const FAKE_QUESTION_CLAUDE: &str = r#"#!/bin/sh
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"zz-1","response":{"commands":[],"models":[],"current_permission_mode":"default"}}}'
read -r probe
printf '%s\n' '{"type":"control_response","response":{"subtype":"error","request_id":"zz-2","error":"Unsupported control request subtype: get_settings"}}'
read -r usage
printf '%s\n' '{"type":"control_response","response":{"subtype":"error","request_id":"zz-3","error":"Unsupported control request subtype: get_usage"}}'
read -r prompt
read -r title
printf '%s\n' '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"b1","task_type":"local_bash","description":"sleep 30"},{"task_id":"w1","task_type":"monitor","description":"watcher","ambient":true}]}'
printf '%s\n' '{"type":"control_request","request_id":"q-1","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","tool_use_id":"t9","input":{"questions":[{"question":"Which fruit?","header":"Fruit","multiSelect":false,"options":[{"label":"apple"},{"label":"pear"}]},{"question":"Which colors?","header":"Colors","multiSelect":true,"options":[{"label":"red"},{"label":"blue"},{"label":"green"}]}]}}}'
read -r answer
case "$answer" in *'"request_id":"q-1"'*'"answers":{"Which colors?":"red, green","Which fruit?":"a fig"}'*) ;; *) echo "bad answer: $answer" >&2; exit 4 ;; esac
printf '%s\n' '{"type":"system","subtype":"session_state_changed","state":"running"}'
printf '%s\n' '{"type":"system","subtype":"session_state_changed","state":"idle"}'
while read -r line; do :; done
"#;

#[cfg(unix)]
#[test]
fn a_question_card_carries_every_question_and_answers_in_one_reply() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().expect("tempdir");
    let program = directory.path().join("claude");
    std::fs::write(&program, FAKE_QUESTION_CLAUDE).expect("fake claude");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let command = ClaudeCommand::parse(&program.display().to_string()).expect("native");
    let (commands, command_rx) = async_channel::unbounded();
    let (controls, control_rx) = async_channel::unbounded();
    let (events, event_rx) = async_channel::unbounded();
    let runtime = run_claude_runtime(
        command,
        AgentWorkspaceEnvironment::default(),
        AgentProvider::ClaudeCode,
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
    let driver = async {
        let until = |accept: fn(&AgentStreamPayload) -> bool| {
            let event_rx = event_rx.clone();
            async move {
                loop {
                    let payload = futures_lite::future::or(
                        async { event_rx.recv().await.expect("event") },
                        async {
                            smol::Timer::after(Duration::from_secs(20)).await;
                            panic!("timed out waiting for the runtime");
                        },
                    )
                    .await;
                    if accept(&payload) {
                        return payload;
                    }
                }
            }
        };
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: None,
            })
            .await
            .expect("open");
        until(|payload| matches!(payload, AgentStreamPayload::SessionReady { .. })).await;
        commands
            .send(RuntimeCommand::Prompt {
                turn_id: 1,
                note: None,
                prompt: crate::stream::AgentPrompt {
                    text: "ask me".to_owned(),
                    ..Default::default()
                },
            })
            .await
            .expect("prompt");
        let AgentStreamPayload::TasksChanged { tasks } = until(|payload| {
            matches!(payload, AgentStreamPayload::TasksChanged { tasks } if !tasks.is_empty())
        })
        .await
        else {
            unreachable!();
        };
        assert_eq!(tasks.len(), 1, "ambient watchers stay out of the tray");
        assert_eq!(tasks[0].kind, "shell");
        let AgentStreamPayload::PermissionRequested {
            request_id,
            options,
            questions,
            ..
        } = until(|payload| matches!(payload, AgentStreamPayload::PermissionRequested { .. }))
            .await
        else {
            unreachable!();
        };
        assert_eq!(options, json!([]));
        assert_eq!(questions.len(), 2);
        assert!(questions[1].multi_select);
        assert_eq!(questions[0].header.as_deref(), Some("Fruit"));
        controls
            .send(RuntimeControl::AnswerQuestion {
                request_id,
                answers: vec![
                    AgentQuestionAnswer {
                        id: "q0".to_owned(),
                        answers: vec!["a fig".to_owned()],
                    },
                    AgentQuestionAnswer {
                        id: "q1".to_owned(),
                        answers: vec!["red".to_owned(), "green".to_owned()],
                    },
                ],
            })
            .await
            .expect("answer");
        until(|payload| {
            matches!(
                payload,
                AgentStreamPayload::PermissionResolved {
                    canceled: false,
                    ..
                }
            )
        })
        .await;
        until(|payload| matches!(payload, AgentStreamPayload::PromptFinished { .. })).await;
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
}

#[cfg(unix)]
const FAKE_PICK_CLAUDE: &str = r#"#!/bin/sh
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"zz-1","response":{"commands":[],"models":[],"current_permission_mode":"default"}}}'
read -r probe
printf '%s\n' '{"type":"control_response","response":{"subtype":"error","request_id":"zz-2","error":"Unsupported control request subtype: get_settings"}}'
read -r usage
printf '%s\n' '{"type":"control_response","response":{"subtype":"error","request_id":"zz-3","error":"Unsupported control request subtype: get_usage"}}'
read -r prompt
read -r title
printf '%s\n' '{"type":"system","subtype":"session_state_changed","state":"running"}'
printf '%s\n' '{"type":"control_request","request_id":"q-1","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","tool_use_id":"t9","input":{"questions":[{"question":"Which fruit?","header":"Fruit","multiSelect":false,"options":[{"label":"apple"},{"label":"pear"}]}]}}}'
read -r answer
case "$answer" in *'"request_id":"q-1"'*'"answers":{"Which fruit?":"pear"}'*) ;; *) echo "bad answer: $answer" >&2; exit 4 ;; esac
printf '%s\n' '{"type":"system","subtype":"session_state_changed","state":"idle"}'
while read -r line; do :; done
"#;

#[cfg(unix)]
#[test]
fn a_single_question_picked_by_option_records_the_label() {
    use std::os::unix::fs::PermissionsExt as _;
    use zz_protocol::chat::{Answer, QuestionAnswer, RequestClose};

    let directory = tempfile::tempdir().expect("tempdir");
    let program = directory.path().join("claude");
    std::fs::write(&program, FAKE_PICK_CLAUDE).expect("fake claude");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let command = ClaudeCommand::parse(&program.display().to_string()).expect("native");
    let (commands, command_rx) = async_channel::unbounded();
    let (controls, control_rx) = async_channel::unbounded();
    let (events, event_rx) = async_channel::unbounded();
    let runtime = run_claude_runtime(
        command,
        AgentWorkspaceEnvironment::default(),
        AgentProvider::ClaudeCode,
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
    let driver = async {
        let until = |accept: fn(&AgentStreamPayload) -> bool| {
            let event_rx = event_rx.clone();
            async move {
                loop {
                    let payload = futures_lite::future::or(
                        async { event_rx.recv().await.expect("event") },
                        async {
                            smol::Timer::after(Duration::from_secs(20)).await;
                            panic!("timed out waiting for the runtime");
                        },
                    )
                    .await;
                    if accept(&payload) {
                        return payload;
                    }
                }
            }
        };
        commands
            .send(RuntimeCommand::Open {
                cwd: directory.path().to_path_buf(),
                resume_session: None,
            })
            .await
            .expect("open");
        until(|payload| matches!(payload, AgentStreamPayload::SessionReady { .. })).await;
        commands
            .send(RuntimeCommand::Prompt {
                turn_id: 1,
                note: None,
                prompt: crate::stream::AgentPrompt {
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
        } = until(|payload| matches!(payload, AgentStreamPayload::PermissionRequested { .. }))
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
        } = until(|payload| {
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
                        question: "q0".to_owned(),
                        answers: vec!["pear".to_owned()],
                    }],
                },
            }
        );
        until(|payload| matches!(payload, AgentStreamPayload::PromptFinished { .. })).await;
        commands
            .send(RuntimeCommand::Shutdown)
            .await
            .expect("shutdown");
    };
    let (result, ()) = smol::block_on(futures_lite::future::zip(runtime, driver));
    assert_eq!(result, Ok(()));
}

#[test]
fn a_question_answer_and_a_plan_choice_shape_the_reply() {
    let reply = allow_with_mode(&json!({"plan":"p"}), "acceptEdits");
    assert_eq!(reply["updatedPermissions"][0]["mode"], "acceptEdits");
    assert_eq!(deny("no", true)["interrupt"], true);
    assert!(deny("no", false).get("interrupt").is_none());
}
