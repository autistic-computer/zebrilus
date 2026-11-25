use super::*;

#[test]
fn wait_for_matches_sticky_signal_gc_and_dispatch_precedence() {
    let shared = Arc::new(Shared::new(1));
    register_wait_clients(&shared, [1]);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("wait-for", ["-SL", "sticky"]),
        )
        .expect("signal wins over lock");
    assert!(shared.inner.lock().waits.wait_channels["sticky"].woken);
    assert!(!shared.inner.lock().waits.wait_channels["sticky"].locked);
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("wait", ["sticky"]),
        )
        .expect("consume sticky signal");
    assert!(
        !shared
            .inner
            .lock()
            .waits
            .wait_channels
            .contains_key("sticky")
    );
    let error = shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("wait-for", ["-U", "sticky"]),
        )
        .expect_err("unlocked channel");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "channel sticky not locked"
    ));
}

#[test]
fn agent_state_changes_publish_the_hook_event_for_terminal_panes() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "terminal-events"]),
        )
        .expect("session");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(control, context.session.expect("session"))
        .expect("attach");
    take_reliable_messages(&mailbox);
    for value in ["working", "working", "idle"] {
        shared
            .execute(
                control,
                ClientKind::Control,
                &mut context,
                &CommandInvocation::new("set-option", ["-p", "-t", "%0", "@agent_state", value]),
            )
            .expect("set state");
        let events = take_reliable_messages(&mailbox)
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::HookEvent { name, variables },
                    ..
                }) if name == "agent-state-changed" => Some(variables),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["hook_pane"], "%0");
        assert_eq!(events[0]["agent_state"], value);
        assert_eq!(events[0]["agent_pending_permission"], "");
    }
}

#[cfg(feature = "agent")]
#[test]
fn agent_tool_call_turn_publishes_start_and_terminal_status_once() {
    use crate::agent::{
        host::AgentPaneSpec,
        runtime::RuntimeCommand,
        stream::{AgentPrompt, AgentPromptOutcome, AgentStreamPayload},
    };
    let shared = Arc::new(Shared::new(1));
    let runtime = shared.build_agent_runtime(None).unwrap();
    runtime.set_runner_factory(Box::new(|_| Box::new(|channels| Box::pin(async move {
        while let Ok(command) = channels.commands.recv().await {
            match command {
                RuntimeCommand::Open { .. } => {
                    channels.events.send(AgentStreamPayload::SessionReady {
                        session_id: "progress-fixture".to_owned(), modes: None, config_options: None,
                    }).await.map_err(|error| error.to_string())?;
                }
                RuntimeCommand::Prompt { turn_id, .. } => {
                    for update in [
                        serde_json::json!({"sessionUpdate":"tool_call", "toolCallId":"one", "title":" cargo\n test\t-p zz-daemon ", "kind":"execute", "status":"pending"}),
                        serde_json::json!({"sessionUpdate":"tool_call_update", "toolCallId":"one", "status":"in_progress"}),
                        serde_json::json!({"sessionUpdate":"tool_call_update", "toolCallId":"one", "status":"completed"}),
                        serde_json::json!({"sessionUpdate":"tool_call_update", "toolCallId":"one", "status":"completed"}),
                        serde_json::json!({"sessionUpdate":"tool_call", "toolCallId":"two", "title":"界".repeat(220), "status":"pending"}),
                        serde_json::json!({"sessionUpdate":"tool_call_update", "toolCallId":"two", "status":"failed"}),
                    ] {
                        channels.events.send(AgentStreamPayload::Update { update }).await.map_err(|error| error.to_string())?;
                    }
                    channels.events.send(AgentStreamPayload::PromptFinished {
                        turn_id, outcome: AgentPromptOutcome::Finished { stop_reason: serde_json::json!("end_turn") },
                    }).await.map_err(|error| error.to_string())?;
                }
                RuntimeCommand::Shutdown => break,
                _ => {}
            }
        }
        Ok(())
    }))));
    let mut context = ExecutionContext::default();
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "tool-events"]),
        )
        .unwrap();
    let pane = context.pane.unwrap();
    shared
        .inner
        .lock()
        .engine
        .state
        .convert_pane(
            pane,
            PaneKind::Agent(zz_protocol::AgentDescriptor::default()),
        )
        .unwrap();
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared.attach(control, context.session.unwrap()).unwrap();
    take_reliable_messages(&mailbox);
    assert!(runtime.open(
        pane,
        AgentPaneSpec {
            model: None,
            effort: None,
            provider: AgentProvider::Codex,
            cwd: PathBuf::from("/"),
            resume_session: None,
            auto_approve: None,
            workspace: AgentWorkspaceEnvironment::default(),
        }
    ));
    assert!(runtime.prompt(
        pane,
        AgentPrompt {
            text: "test tool calls".to_owned(),
            ..AgentPrompt::default()
        }
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut events = Vec::new();
    while Instant::now() < deadline && events.len() < 4 {
        for message in take_reliable_messages(&mailbox) {
            if let ProtocolMessage::Event(Event {
                payload: EventPayload::HookEvent { name, variables },
                ..
            }) = message
                && name == "agent-tool-call"
            {
                events.push(variables);
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(events.len(), 4);
    for (event, id, status) in [
        (&events[0], "one", "pending"),
        (&events[1], "one", "completed"),
        (&events[2], "two", "pending"),
        (&events[3], "two", "failed"),
    ] {
        assert_eq!(event["hook_pane"], pane.to_string());
        assert_eq!(event["tool_call_id"], id);
        assert_eq!(event["tool_status"], status);
        assert_eq!(
            event["tool_title"],
            if id == "one" {
                "cargo test -p zz-daemon".to_owned()
            } else {
                "界".repeat(200)
            }
        );
        assert_eq!(event["tool_kind"], if id == "one" { "execute" } else { "" });
    }
    let row: serde_json::Value = serde_json::from_str(
        &shared
            .inspect(control, &context, &["--json".into()])
            .unwrap()
            .output,
    )
    .unwrap();
    assert!(
        row["events"]
            .as_array()
            .unwrap()
            .contains(&"agent-tool-call".into())
    );
    shared.shutdown_agents();
}

#[test]
fn user_option_writes_signal_their_wait_channel() {
    let shared = Arc::new(Shared::new(1));
    register_wait_clients(&shared, [1]);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "signals"]),
        )
        .expect("session");
    let pane = context.pane.expect("pane").to_string();
    let channel = format!("@agent_state@{pane}");
    let run = |args: &[&str]| {
        shared.execute(
            ClientId(1),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(args[0], args[1..].iter().copied()),
        )
    };

    run(&["set-option", "-p", "-t", &pane, "@agent_state", "working"]).expect("set");
    assert!(shared.inner.lock().waits.wait_channels[&channel].woken);
    run(&["wait-for", &channel]).expect("the sticky signal is consumed at once");
    assert!(
        !shared
            .inner
            .lock()
            .waits
            .wait_channels
            .contains_key(&channel)
    );

    run(&["set-option", "-p", "-t", &pane, "-u", "@agent_state"]).expect("unset");
    assert!(shared.inner.lock().waits.wait_channels[&channel].woken);
    run(&["wait-for", &channel]).expect("unset signals too");

    run(&["set-option", "-p", "-t", &pane, "-u", "@agent_state"]).expect("unset again");
    assert!(
        !shared
            .inner
            .lock()
            .waits
            .wait_channels
            .contains_key(&channel),
        "unsetting an absent option is not a change"
    );

    run(&["set-option", "-g", "@fleet", "north"]).expect("global");
    assert!(shared.inner.lock().waits.wait_channels["@fleet@global-session"].woken);
    run(&["set-option", "-s", "@daemon", "up"]).expect("server");
    assert!(shared.inner.lock().waits.wait_channels["@daemon@server"].woken);
}

#[cfg(unix)]
#[test]
fn progress_bar_transitions_write_agent_state_for_listed_commands() {
    let shared = Arc::new(Shared::new(1));
    register_wait_clients(&shared, [1, 2]);
    let (pane, command) = progress_bar_pane(&shared);
    let target = pane.to_string();
    let channel = format!("@agent_state@{pane}");
    let run = |args: &[&str]| {
        shared
            .execute(
                ClientId(1),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .expect("command")
    };
    run(&[
        "set-option",
        "-p",
        "-t",
        &target,
        "@agent-progress-commands",
        &command,
    ]);
    run(&[
        "set-hook",
        "-g",
        "@option-changed",
        "set-option -gF @progress-observed '#{hook_option}:#{hook_target}'",
    ]);
    for (osc, expected) in [("3", "working"), ("0", "idle")] {
        let parked = Arc::clone(&shared);
        let wait_channel = channel.clone();
        let (completed, completion) = mpsc::channel();
        let waiter = thread::spawn(move || {
            completed
                .send(parked.wait_for(ClientId(2), ClientKind::Command, &[wait_channel.into()]))
                .expect("wait completion");
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while shared
            .inner
            .lock()
            .waits
            .wait_channels
            .get(&channel)
            .is_none_or(|channel| channel.waiters.is_empty())
        {
            assert!(Instant::now() < deadline, "waiter did not park");
            thread::sleep(Duration::from_millis(10));
        }
        run(&[
            "send-keys",
            "-t",
            &target,
            &format!("printf '\\033]9;4;{osc}\\007'"),
            "Enter",
        ]);
        let deadline = Instant::now() + Duration::from_secs(10);
        while run(&["show-options", "-p", "-qv", "-t", &target, "@agent_state"])
            .output
            .trim()
            != expected
        {
            assert!(
                Instant::now() < deadline,
                "agent state did not become {expected}"
            );
            thread::sleep(Duration::from_millis(10));
        }
        completion
            .recv_timeout(Duration::from_secs(10))
            .expect("progress transition did not wake waiter")
            .expect("wait-for");
        waiter.join().expect("waiter");
        assert_eq!(
            read_global_option(&shared, "@progress-observed").trim(),
            format!("@agent_state:{pane}")
        );
        let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
        for state in [
            ProgressBarState::Normal,
            ProgressBarState::Error,
            ProgressBarState::Paused,
        ] {
            shared.synchronize_pane_progress(pane, &terminal, &command, state);
        }
        shared.synchronize_pane_progress(pane, &terminal, &command, terminal.progress_bar().state);
        assert!(
            !shared
                .inner
                .lock()
                .waits
                .wait_channels
                .get(&channel)
                .is_some_and(|channel| channel.woken),
            "writing the same state must not signal again"
        );
    }
    shared.request_shutdown();
}

#[cfg(unix)]
#[test]
fn progress_bar_transitions_are_ignored_for_unlisted_commands() {
    let shared = Arc::new(Shared::new(1));
    let (pane, command) = progress_bar_pane(&shared);
    let target = pane.to_string();
    let run = |args: &[&str]| {
        shared
            .execute(
                ClientId(1),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .expect("command")
    };
    let unlisted = format!("{command}-unlisted");
    run(&[
        "set-option",
        "-p",
        "-t",
        &target,
        "@agent-progress-commands",
        &unlisted,
    ]);
    for (osc, state) in [
        ("3", ProgressBarState::Indeterminate),
        ("0", ProgressBarState::Hidden),
    ] {
        let title = format!("progress-{osc}");
        run(&[
            "send-keys",
            "-t",
            &target,
            &format!("printf '\\033]9;4;{osc}\\007\\033]0;{title}\\007'"),
            "Enter",
        ]);
        let deadline = Instant::now() + Duration::from_secs(10);
        while run(&["display-message", "-p", "-t", &target, "#{pane_title}"])
            .output
            .trim()
            != title
        {
            assert!(
                Instant::now() < deadline,
                "watcher did not observe progress output"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            shared.inner.lock().terminals[&pane].progress_bar().state,
            state
        );
        assert!(
            run(&["show-options", "-p", "-qv", "-t", &target, "@agent_state"])
                .output
                .is_empty()
        );
        assert!(
            !shared
                .inner
                .lock()
                .waits
                .wait_channels
                .contains_key(&format!("@agent_state@{pane}"))
        );
    }
    shared.request_shutdown();
}

#[cfg(unix)]
#[test]
fn program_status_reports_reach_formats_the_tree_and_agent_state() {
    let shared = Arc::new(Shared::new(1));
    let (pane, command) = progress_bar_pane(&shared);
    let target = pane.to_string();
    let run = |args: &[&str]| {
        shared
            .execute(
                ClientId(1),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .expect("command")
    };
    let settle = |format: &str, expected: &str| {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let output = run(&["display-message", "-p", "-t", &target, format]).output;
            if output.trim() == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{format} stayed {output:?}, wanted {expected:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    };
    let tree_status = || {
        shared.inner.lock().engine.state.snapshot().sessions[0].windows[0].panes[&pane]
            .status
            .clone()
    };
    let peer_sample = |pid: &str| {
        run(&[
            "if-shell",
            "-F",
            "-t",
            &target,
            &format!("#{{?pane_status_reported,,#{{==:#{{pane_pid}},{pid}}}}}"),
            &format!("set-option -p -t {target} @agent_state working"),
        ]);
    };
    let pane_pid = || {
        run(&["display-message", "-p", "-t", &target, "#{pane_pid}"])
            .output
            .trim()
            .to_owned()
    };
    run(&[
        "set-option",
        "-p",
        "-t",
        &target,
        "@agent-progress-commands",
        &command,
    ]);
    run(&[
        "send-keys",
        "-t",
        &target,
        "printf '\\033]7501;state=working:app=tf\\007\\033]7501;state=blocked:id=plan:kind=permission:progress=40:title=UGxhbg:msg=QXBwbHkgIzE/\\007'",
        "Enter",
    ]);
    settle(
        "#{pane_status}|#{pane_status_kind}|#{pane_status_progress}|#{pane_status_app}|#{pane_status_title}|#{pane_status_message}|#{pane_status_raw_message}|#{pane_status_reported}|#{agent_state}",
        "blocked|permission|40|tf|Plan|Apply ##1?|Apply #1?|1|blocked",
    );
    assert_eq!(
        tree_status(),
        Some(PaneStatus {
            state: PaneStatusState::Blocked,
            kind: Some(PaneStatusKind::Permission),
            progress: Some(40),
            app: "tf".to_owned(),
            title: "Plan".to_owned(),
            message: "Apply #1?".to_owned(),
        })
    );

    let old_pid = pane_pid();
    peer_sample(&old_pid);
    settle("#{agent_state}", "blocked");

    run(&[
        "send-keys",
        "-t",
        &target,
        "printf '\\033]9;4;0\\007\\033]7501;state=done:id=plan\\007\\033]0;yielded\\007'",
        "Enter",
    ]);
    settle(
        "#{pane_title}|#{pane_status}|#{agent_state}",
        "yielded|working|working",
    );

    run(&[
        "send-keys",
        "-t",
        &target,
        "printf '\\033]133;A\\007'",
        "Enter",
    ]);
    settle(
        "#{pane_status}|#{pane_status_app}|#{agent_state}",
        "done||idle",
    );
    assert_eq!(
        tree_status().map(|status| status.state),
        Some(PaneStatusState::Done)
    );

    run(&[
        "send-keys",
        "-t",
        &target,
        "printf '\\033]7501;state=working\\007'",
        "Enter",
    ]);
    settle("#{pane_status}|#{agent_state}", "working|working");
    run(&["send-keys", "-t", &target, "printf '\\033c'", "Enter"]);
    settle(
        "#{pane_status}|#{pane_status_reported}|#{agent_state}",
        "|0|idle",
    );
    assert_eq!(tree_status(), None);

    run(&[
        "send-keys",
        "-t",
        &target,
        "printf '\\033]7501;state=blocked\\007'",
        "Enter",
    ]);
    settle("#{pane_status}|#{agent_state}", "blocked|blocked");
    run(&["respawn-pane", "-k", "-t", &target]);
    settle("#{pane_status}|#{agent_state}", "|idle");
    assert_eq!(tree_status(), None);
    peer_sample(&old_pid);
    settle("#{agent_state}", "idle");

    run(&[
        "agent-send",
        "-t",
        &target,
        "--wait",
        "--timeout",
        "10",
        "printf '\\033]7501;state=working\\007\\033]7501;state=done\\007'",
    ]);
    settle("#{pane_status}|#{agent_state}", "done|idle");
    shared.request_shutdown();
}

#[test]
fn agent_state_format_falls_back_to_the_user_option_for_terminal_panes() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "state-format"]),
        )
        .expect("mux session");
    let terminal = context.pane.expect("terminal").to_string();
    let run = |args: &[&str]| {
        shared
            .execute(
                ClientId(1),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .expect("command")
    };
    run(&["set-option", "-g", "@agent_state", "idle"]);
    assert_eq!(
        run(&["display-message", "-p", "-t", &terminal, "#{agent_state}"]).output,
        "idle"
    );
    run(&[
        "set-option",
        "-p",
        "-t",
        &terminal,
        "@agent_state",
        "working",
    ]);
    assert_eq!(
        run(&["display-message", "-p", "-t", &terminal, "#{agent_state}"]).output,
        "working"
    );
    assert_eq!(
        run(&[
            "display-message",
            "-p",
            "-t",
            &terminal,
            "#{agent_pending_permission}"
        ])
        .output
        .trim(),
        ""
    );
}

#[test]
fn wait_for_sticky_signal_survives_signaling_client_disconnect() {
    let shared = Arc::new(Shared::new(1));
    register_wait_clients(&shared, [1, 2]);
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("wait-for", ["-S", "ch1"]),
        )
        .expect("sticky signal");
    shared.unregister(ClientId(1));
    assert!(shared.inner.lock().waits.wait_channels["ch1"].woken);

    shared
        .execute(
            ClientId(2),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("wait-for", ["ch1"]),
        )
        .expect("consume sticky signal after signaler disconnects");
    assert!(!shared.inner.lock().waits.wait_channels.contains_key("ch1"));

    let error = shared
        .execute(
            ClientId(2),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("wait-for", ["-U", "ch1"]),
        )
        .expect_err("consumed channel is gone");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "channel ch1 not locked"
    ));
}

#[test]
fn user_option_writes_run_the_option_changed_hook_once() {
    let shared = Arc::new(Shared::new(1));
    register_wait_clients(&shared, [1]);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "hooks"]),
        )
        .expect("session");
    let pane = context.pane.expect("pane").to_string();
    let run = |args: &[&str]| {
        shared.execute(
            ClientId(1),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(args[0], args[1..].iter().copied()),
        )
    };

    run(&[
        "set-hook",
        "-g",
        "@option-changed",
        "display-message -p \"#{hook}:#{hook_option}=#{hook_target}\" ; set-option -g @seen yes",
    ])
    .expect("install the user hook");
    let write =
        run(&["set-option", "-p", "-t", &pane, "@agent_state", "idle"]).expect("pane write");
    assert_eq!(
        write.output.trim_end(),
        format!("@option-changed:@agent_state={pane}"),
        "the hook ran with the option and target in scope"
    );
    assert_eq!(
        run(&["show-options", "-gv", "@seen"])
            .expect("seen")
            .output
            .trim_end(),
        "yes"
    );
    assert!(
        shared.inner.lock().waits.wait_channels["@seen@global-session"].woken,
        "the hook's own write still signals its channel without re-running the hook"
    );
}

#[test]
fn wait_for_wakes_all_waiters_and_hands_locks_off_fifo() {
    let shared = Arc::new(Shared::new(1));
    register_wait_clients(&shared, [1, 2, 3, 4]);
    let wait = |client, flag: Option<&'static str>, name: &'static str| {
        let shared = Arc::clone(&shared);
        thread::spawn(move || {
            let args = flag.map_or_else(|| vec![name], |flag| vec![flag, name]);
            shared.execute(
                ClientId(client),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("wait-for", args),
            )
        })
    };
    let first = wait(1, None, "wake-all");
    let second = wait(2, None, "wake-all");
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared
        .inner
        .lock()
        .waits
        .wait_channels
        .get("wake-all")
        .map_or(0, |channel| channel.waiters.len())
        != 2
    {
        assert!(Instant::now() < deadline, "waiters did not queue");
        thread::yield_now();
    }
    shared
        .wait_for(
            ClientId(u64::MAX),
            ClientKind::Command,
            &["-S".into(), "wake-all".into()],
        )
        .expect("signal waiters");
    first.join().expect("first waiter").expect("first wait");
    second.join().expect("second waiter").expect("second wait");

    shared
        .wait_for(
            ClientId(3),
            ClientKind::Command,
            &["-L".into(), "lock".into()],
        )
        .expect("first lock");
    let locker = wait(4, Some("-L"), "lock");
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.inner.lock().waits.wait_channels["lock"]
        .lockers
        .len()
        != 1
    {
        assert!(Instant::now() < deadline, "locker did not queue");
        thread::yield_now();
    }
    shared
        .wait_for(
            ClientId(u64::MAX),
            ClientKind::Command,
            &["-U".into(), "lock".into()],
        )
        .expect("handoff lock");
    locker.join().expect("locker").expect("lock handoff");
    assert!(shared.inner.lock().waits.wait_channels["lock"].locked);
    shared
        .wait_for(
            ClientId(u64::MAX),
            ClientKind::Command,
            &["-U".into(), "lock".into()],
        )
        .expect("free lock");
    assert!(!shared.inner.lock().waits.wait_channels["lock"].locked);
}

#[test]
fn wait_for_rejects_interactive_blocking_and_disconnect_cleanup_unblocks() {
    let shared = Arc::new(Shared::new(1));
    for (client, kind) in [
        (ClientId(1), ClientKind::Interactive),
        (ClientId(u64::MAX), ClientKind::Command),
    ] {
        for (args, expected) in [
            (vec![RawText::from("channel")], "not able to wait"),
            (
                vec![RawText::from("-L"), "channel".into()],
                "not able to lock",
            ),
        ] {
            let error = shared
                .wait_for(client, kind, &args)
                .expect_err("clientless block");
            assert!(matches!(
                error,
                DaemonError::Server(ServerError::InvalidCommand(message)) if message == expected
            ));
        }
    }
    register_wait_clients(&shared, [9]);
    let waiter_shared = Arc::clone(&shared);
    let waiter = thread::spawn(move || {
        waiter_shared.wait_for(ClientId(9), ClientKind::Command, &["drop".into()])
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared
        .inner
        .lock()
        .waits
        .wait_channels
        .get("drop")
        .map_or(0, |channel| channel.waiters.len())
        != 1
    {
        assert!(Instant::now() < deadline, "waiter did not queue");
        thread::yield_now();
    }
    shared.unregister(ClientId(9));
    waiter.join().expect("waiter").expect("disconnect wake");
    assert!(
        shared
            .inner
            .lock()
            .waits
            .wait_channels
            .get("drop")
            .is_none_or(|channel| channel.waiters.is_empty())
    );
    shared
        .wait_for(ClientId(9), ClientKind::Command, &["late".into()])
        .expect("disconnected wait does not park");
    assert!(!shared.inner.lock().waits.wait_channels.contains_key("late"));
}

#[test]
fn kill_server_flushes_waiters() {
    let shared = Arc::new(Shared::new(1));
    register_wait_clients(&shared, [5]);
    let waiter_shared = Arc::clone(&shared);
    let waiter = thread::spawn(move || {
        waiter_shared.wait_for(ClientId(5), ClientKind::Command, &["shutdown".into()])
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while !shared
        .inner
        .lock()
        .waits
        .wait_channels
        .contains_key("shutdown")
    {
        assert!(Instant::now() < deadline, "shutdown waiter did not queue");
        thread::yield_now();
    }
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("kill-server", [] as [&str; 0]),
        )
        .expect("kill server");
    waiter.join().expect("waiter").expect("shutdown flush");
    assert!(shared.inner.lock().waits.wait_channels.is_empty());
}
