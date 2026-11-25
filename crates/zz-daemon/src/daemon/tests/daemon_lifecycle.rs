use super::*;

#[test]
fn command_alias_reaches_daemon_owned_commands() {
    let shared = Arc::new(Shared::new(1));
    let (session, _, _) = switch_test_session(&shared, "w");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("gui".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(client, session).expect("attach");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                ["-s", "command-alias[20]", "lsc=list-clients"],
            ),
        )
        .expect("alias write");
    let listed = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("lsc", ["-F", "#{client_name}"]),
        )
        .expect("alias routed to the daemon-owned command");
    assert!(listed.output.contains("gui"), "{:?}", listed.output);
    for (index, value) in [("21", "first=second"), ("22", "second=list-clients")] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(
                    "set-option",
                    ["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .expect("alias write");
    }
    let error = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("first", [] as [&str; 0]),
        )
        .expect_err("daemon expands one alias layer");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::CommandParse(message))
            if message == "unknown command: second"
    ));
}

#[test]
fn explicit_exit_empty_off_keeps_an_armed_empty_daemon_alive_until_unset() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    switch_test_session(&shared, "doomed");
    for (name, args) in [
        ("set-option", vec!["-s", "exit-empty", "off"]),
        ("kill-session", vec!["-t", "doomed"]),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(name, args),
            )
            .expect("command");
    }
    assert!(shared.exit_empty_armed.load(Ordering::Acquire));
    assert!(!shared.stopping.load(Ordering::Acquire));
    let (client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    shared.unregister(client);
    assert!(!shared.stopping.load(Ordering::Acquire));
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-s", "-u", "exit-empty"]),
        )
        .expect("unset restores the latch rule");
    #[cfg(unix)]
    lifecycle::wait_for_cleanup(&shared);
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[test]
fn explicit_exit_empty_on_shuts_down_without_the_session_latch() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-s", "exit-empty", "on"]),
        )
        .expect("explicit default write");
    assert!(!shared.exit_empty_armed.load(Ordering::Acquire));
    #[cfg(unix)]
    lifecycle::wait_for_cleanup(&shared);
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[test]
fn explicit_exit_unattached_exits_with_live_sessions_but_never_under_a_subscriber() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    switch_test_session(&shared, "keeper");
    let (subscriber, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("gui".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-s", "exit-unattached", "on"]),
        )
        .expect("explicit write");
    assert!(!shared.stopping.load(Ordering::Acquire));
    shared.unregister(subscriber);
    #[cfg(unix)]
    lifecycle::wait_for_cleanup(&shared);
    assert!(shared.stopping.load(Ordering::Acquire));
    assert!(!shared.inner.lock().engine.state.sessions.is_empty());
}

#[test]
fn lifecycle_policies_stay_dormant_during_the_startup_bracket() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    shared.begin_startup();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-s", "exit-unattached", "on"]),
        )
        .expect("boot config write");
    assert!(!shared.stopping.load(Ordering::Acquire));
    shared.finish_startup();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("list-sessions", [] as [&str; 0]),
        )
        .expect("first post-startup command");
    #[cfg(unix)]
    lifecycle::wait_for_cleanup(&shared);
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[test]
fn unset_lifecycle_trio_keeps_the_armed_latch_rules_intact() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    switch_test_session(&shared, "doomed");
    let (subscriber, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("gui".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    for (name, args) in [
        ("list-sessions", vec![]),
        ("kill-session", vec!["-t", "doomed"]),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(name, args),
            )
            .expect("command");
    }
    assert!(shared.exit_empty_armed.load(Ordering::Acquire));
    assert!(!shared.stopping.load(Ordering::Acquire));
    shared.unregister(subscriber);
    #[cfg(unix)]
    lifecycle::wait_for_cleanup(&shared);
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[test]
fn initialize_leaves_a_fresh_daemon_empty_alive_and_unarmed() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    let mut context = ExecutionContext::default();

    let listing = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("list-sessions", [] as [&str; 0]),
        )
        .expect("list an empty daemon");
    assert_eq!(listing.output, "");
    assert_eq!(context, ExecutionContext::default());
    assert!(shared.inner.lock().engine.state.sessions.is_empty());
    assert!(!shared.exit_empty_armed.load(Ordering::Acquire));
    assert!(!shared.stopping.load(Ordering::Acquire));

    let (client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    shared.unregister(client);
    assert!(!shared.stopping.load(Ordering::Acquire));
}

#[test]
fn first_command_session_uses_zero_ids_and_arms_last_session_shutdown() {
    let shared = Arc::new(Shared::new(1));
    #[cfg(unix)]
    let mut event_loop = event_loop::EventLoop::empty(&shared).unwrap();
    shared.initialize(false).expect("initialize daemon state");
    let mut context = ExecutionContext::default();

    let created = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d"]),
        )
        .expect("create first command session");
    assert_eq!(created.output, "");
    assert_eq!(context.session, Some(SessionId(0)));
    assert_eq!(context.window, Some(WindowId(0)));
    assert_eq!(context.pane, Some(PaneId(0)));
    assert!(shared.exit_empty_armed.load(Ordering::Acquire));
    for (command, args, expected) in [
        (
            "list-sessions",
            vec!["-F", "#{session_id}:#{session_name}"],
            "$0:0",
        ),
        (
            "list-windows",
            vec!["-t", "0", "-F", "#{window_id}:#{window_index}"],
            "@0:0",
        ),
        (
            "list-panes",
            vec!["-t", "0:0", "-F", "#{pane_id}:#{pane_index}"],
            "%0:0",
        ),
    ] {
        assert_eq!(
            shared
                .execute(
                    ClientId(u64::MAX),
                    ClientKind::Command,
                    &mut context,
                    &CommandInvocation::new(command, args),
                )
                .expect("read first allocation")
                .output,
            expected
        );
    }

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "0"]),
        )
        .expect("kill the last session");

    assert!(shared.inner.lock().engine.state.sessions.is_empty());
    #[cfg(unix)]
    event_loop_tests::drive_loop_until(&shared, &mut event_loop, || {
        shared.stopping.load(Ordering::Acquire)
    });
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[test]
fn an_attached_client_keeps_the_daemon_alive_with_zero_sessions() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let mut context = ExecutionContext::default();
    let (session, snapshot) = shared
        .attach_target(client, ClientKind::Interactive, &mut context, "")
        .expect("attach to the lazily created session");
    assert_eq!(session, SessionId(0));
    assert_eq!(context.session, Some(SessionId(0)));
    assert_eq!(context.window, Some(WindowId(0)));
    assert_eq!(context.pane, Some(PaneId(0)));
    assert_eq!(snapshot.sessions[0].name, "0");
    assert_eq!(snapshot.sessions[0].windows[0].id, WindowId(0));
    assert_eq!(snapshot.sessions[0].windows[0].active_pane, PaneId(0));

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "0"]),
        )
        .expect("kill the last session");

    assert!(shared.inner.lock().engine.state.sessions.is_empty());
    assert!(
        !shared.stopping.load(Ordering::Acquire),
        "daemon must outlive its last session while a client is attached"
    );

    shared.unregister(client);
    #[cfg(unix)]
    lifecycle::wait_for_cleanup(&shared);
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[test]
fn disconnecting_a_client_that_still_has_sessions_keeps_the_daemon_alive() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    shared
        .attach_target(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            "",
        )
        .expect("attach to the lazily created session");

    shared.unregister(client);

    assert!(!shared.inner.lock().engine.state.sessions.is_empty());
    assert!(!shared.stopping.load(Ordering::Acquire));
}

#[test]
fn only_default_interactive_attach_materializes_an_empty_daemon() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    let (command, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    let mut command_context = ExecutionContext::default();
    let command_error = shared
        .attach_target(command, ClientKind::Command, &mut command_context, "")
        .expect_err("a command attach must not create a session");
    assert!(matches!(
        command_error,
        ServerError::SessionNotFound(target) if target == "current session"
    ));
    assert!(shared.inner.lock().engine.state.sessions.is_empty());

    let (interactive, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let mut interactive_context = ExecutionContext::default();
    let explicit_error = shared
        .attach_target(
            interactive,
            ClientKind::Interactive,
            &mut interactive_context,
            "missing",
        )
        .expect_err("an explicit missing target must not create a session");
    assert!(matches!(
        explicit_error,
        ServerError::SessionNotFound(target) if target == "missing"
    ));
    assert!(shared.inner.lock().engine.state.sessions.is_empty());
}

#[test]
fn concurrent_default_interactive_attaches_atomically_share_session_zero() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    let (first, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (second, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (first_materializing_tx, first_materializing_rx) = crossbeam_channel::bounded(1);
    let (resume_first_tx, resume_first_rx) = crossbeam_channel::bounded(1);
    let first_shared = Arc::clone(&shared);
    let first_handle = thread::spawn(move || {
        let mut context = ExecutionContext::default();
        let attached = first_shared
            .attach_target_with_materialization_observer(
                first,
                ClientKind::Interactive,
                &mut context,
                "",
                || {},
                || {
                    first_materializing_tx
                        .send(())
                        .expect("report first materialization");
                    resume_first_rx
                        .recv()
                        .expect("resume first materialization");
                },
            )
            .expect("first attach to the shared lazy session");
        (attached.0, context)
    });
    first_materializing_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first attach holds materialization lock");

    let (second_waiting_tx, second_waiting_rx) = crossbeam_channel::bounded(1);
    let second_shared = Arc::clone(&shared);
    let second_handle = thread::spawn(move || {
        let mut context = ExecutionContext::default();
        let attached = second_shared
            .attach_target_with_materialization_observer(
                second,
                ClientKind::Interactive,
                &mut context,
                "",
                || second_waiting_tx.send(()).expect("report second attach"),
                || panic!("second attach attempted a duplicate materialization"),
            )
            .expect("second attach to the shared lazy session");
        (attached.0, context)
    });
    second_waiting_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("second attach reached materialization lock");
    resume_first_tx
        .send(())
        .expect("resume first materialization");

    for handle in [first_handle, second_handle] {
        let (session, context) = handle.join().expect("attach thread");
        assert_eq!(session, SessionId(0));
        assert_eq!(context.session, Some(SessionId(0)));
        assert_eq!(context.window, Some(WindowId(0)));
        assert_eq!(context.pane, Some(PaneId(0)));
    }
    let inner = shared.inner.lock();
    assert_eq!(inner.engine.state.sessions.len(), 1);
    assert_eq!(
        inner.attached.get(&SessionId(0)),
        Some(&BTreeSet::from([first, second]))
    );
}

#[test]
fn default_attach_survives_command_materializing_after_the_empty_check() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    let (interactive, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (command, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    let racing_shared = Arc::clone(&shared);
    let mut context = ExecutionContext::default();
    let (session, snapshot) = shared
        .attach_target_with_materialization_observer(
            interactive,
            ClientKind::Interactive,
            &mut context,
            "",
            || {},
            move || {
                racing_shared
                    .execute(
                        command,
                        ClientKind::Command,
                        &mut ExecutionContext::default(),
                        &CommandInvocation::new("new-session", ["-d", "-s", "raced"]),
                    )
                    .expect("command client wins the materialization race");
            },
        )
        .expect("attach to the command-created session");

    assert_eq!(session, SessionId(0));
    assert_eq!(context.session, Some(SessionId(0)));
    assert_eq!(snapshot.sessions[0].name, "raced");
    let direct_error = shared
        .execute(
            command,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("new-session", ["-A", "-d", "-s", "raced"]),
        )
        .expect_err("ordinary command clients still require a terminal to attach");
    assert!(matches!(
        direct_error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "open terminal failed: not a terminal"
    ));
}

#[test]
fn scoped_status_format_writes_refresh_only_that_sessions_clients() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    let alpha_mailbox = OutboundMailbox::new();
    let (alpha_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&alpha_mailbox),
    );
    let beta_mailbox = OutboundMailbox::new();
    let (beta_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&beta_mailbox),
    );
    let mut context = ExecutionContext::default();
    let mut panes = Vec::new();
    for name in ["alpha", "beta"] {
        shared
            .execute(
                alpha_client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("new-session", ["-d", "-s", name, QUIET_PANE_COMMAND]),
            )
            .expect("seed session");
        panes.push(context.pane.expect("seed pane"));
        for (option, value) in [
            ("status-interval", "0"),
            ("status-left", ""),
            ("status-right", ""),
        ] {
            shared
                .execute(
                    alpha_client,
                    ClientKind::Command,
                    &mut context,
                    &CommandInvocation::new("set-option", ["-t", name, option, value]),
                )
                .expect("pin the status halves");
        }
        shared
            .execute(
                alpha_client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "set-window-option",
                    ["-t", &format!("{name}:0"), "automatic-rename", "off"],
                ),
            )
            .expect("disable asynchronous window renames");
    }
    wait_for_pane_runtime_facts(&shared, &panes);
    let mut alpha_context = ExecutionContext::default();
    shared
        .attach_target(
            alpha_client,
            ClientKind::Interactive,
            &mut alpha_context,
            "alpha",
        )
        .expect("attach alpha");
    let mut beta_context = ExecutionContext::default();
    shared
        .attach_target(
            beta_client,
            ClientKind::Interactive,
            &mut beta_context,
            "beta",
        )
        .expect("attach beta");
    shared.refresh_status();
    drain_reliable_messages_until_quiet(&[&alpha_mailbox, &beta_mailbox]);
    let sequence_boundary = Shared::next_sequence();

    shared
        .execute(
            alpha_client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-t", "alpha", "status-format[0]", "ALPHA-ROW"],
            ),
        )
        .expect("scoped status-format write");

    let alpha_status = take_reliable_messages(&alpha_mailbox)
        .into_iter()
        .find_map(|message| match message {
            ProtocolMessage::Event(Event {
                sequence,
                payload: EventPayload::StatusChanged { status },
            }) if sequence > sequence_boundary && status.rows == ["ALPHA-ROW"] => Some(status),
            _ => None,
        })
        .expect("alpha client receives the scoped refresh");
    assert_eq!(alpha_status.rows, ["ALPHA-ROW"]);
    assert!(alpha_status.customized);
    let beta_statuses = take_reliable_messages(&beta_mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                sequence,
                payload: EventPayload::StatusChanged { status },
            }) if sequence > sequence_boundary => Some((sequence, status)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        beta_statuses.is_empty(),
        "the scoped write emitted beta status events: {beta_statuses:?}"
    );
}

#[test]
fn invalid_status_lines_are_rejected_and_surfaced_at_the_outbound_seam() {
    let mailbox = OutboundMailbox::new();
    let status = StatusLine {
        rows: vec![String::new(); zz_protocol::MAX_STATUS_ROWS + 1],
        ..StatusLine::default()
    };
    assert!(status.validate().is_err());
    assert!(!mailbox.enqueue_reliable(&Shared::event(EventPayload::StatusChanged { status })));

    let unclamped = StatusLine {
        rows: vec![String::new()],
        message_line: 1,
        ..StatusLine::default()
    };
    assert!(unclamped.validate().is_err());
    assert!(
        !mailbox.enqueue_reliable(&Shared::event(EventPayload::StatusChanged {
            status: unclamped,
        }))
    );

    assert!(
        mailbox.enqueue_reliable(&Shared::event(EventPayload::StatusChanged {
            status: StatusLine::default(),
        }))
    );
    assert_eq!(take_reliable_messages(&mailbox).len(), 1);
}

#[cfg(unix)]
#[test]
fn status_options_cross_the_socket_with_markers_and_pin_readback_shapes() {
    let socket = daemon_test_endpoint("status-options");
    let daemon = Daemon::new(&socket).without_user_config();
    let daemon_thread = thread::spawn(move || daemon.run_foreground());
    let mut commands = connect_command_retry(&socket);
    commands
        .execute(CommandInvocation::new(
            "new-session",
            ["-d", "-s", "status-wire"],
        ))
        .unwrap();
    let interactive = Arc::new(connect_interactive_retry(&socket));
    let (messages, reader) = spawn_reader(Arc::clone(&interactive));
    let mut terminal_state = TerminalTestState::default();
    interactive.attach("status-wire").unwrap();
    wait_for(&messages, &mut terminal_state, |_, terminal_state| {
        terminal_state.attached
    });

    for args in [
        &["-g", "status-style", "bg=blue,fg=white"] as &[&str],
        &["-g", "status-bg", "red"],
        &["-g", "status-left-style", "bold"],
        &["-g", "status-left-length", "4"],
        &["-g", "status-position", "top"],
        &["-g", "status-left", "#[italics]abcdef"],
    ] {
        commands
            .execute(CommandInvocation::new("set-option", args.iter().copied()))
            .unwrap();
    }
    for (option, expected) in [
        ("status-style", "status-style bg=blue,fg=white"),
        ("status-bg", "status-bg red"),
        ("status-left-style", "status-left-style bold"),
        ("status-left-length", "status-left-length 4"),
        ("status-position", "status-position top"),
        ("status-left", "status-left \"#[italics]abcdef\""),
    ] {
        assert_eq!(
            commands
                .execute(CommandInvocation::new("show-options", ["-g", option]))
                .unwrap(),
            expected
        );
    }
    wait_for(&messages, &mut terminal_state, |message, _| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::StatusChanged { status },
                ..
            }) if status.left
                == "#[bg=blue,fg=white,bg=red]#[bold]#[push-default]#[italics]abcd"
        )
    });

    commands
        .execute(CommandInvocation::new("kill-server", [] as [&str; 0]))
        .unwrap();
    drop(messages);
    drop(interactive);
    reader.join().unwrap();
    daemon_thread.join().unwrap().unwrap();
}

#[test]
fn ready_guard_ends_with_the_daemon_despite_a_passive_command_client() {
    struct ReadyGuard(std::sync::mpsc::Sender<()>);

    impl Drop for ReadyGuard {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }

    let socket = daemon_test_endpoint("tray-lifetime");
    let daemon = Daemon::new(&socket).without_user_config();
    let (ready, started) = std::sync::mpsc::channel();
    let (finished, dropped) = std::sync::mpsc::channel();
    let daemon_thread = thread::spawn(move || {
        daemon.run_foreground_with_ready(|server_id| {
            ready.send(server_id).expect("daemon ready");
            ReadyGuard(finished)
        })
    });
    let server_id = started
        .recv_timeout(Duration::from_secs(10))
        .expect("ready callback");
    let mut passive = connect_command_retry(&socket);
    assert_eq!(passive.server_id().expect("probe"), server_id);
    let mut commands = connect_command_retry(&socket);
    commands
        .execute(CommandInvocation::new("new-session", ["-d"]))
        .expect("session");
    assert!(dropped.try_recv().is_err());
    commands
        .execute(CommandInvocation::new("kill-session", ["-t", "0"]))
        .expect("last session");
    dropped
        .recv_timeout(Duration::from_secs(10))
        .expect("passive client must not keep daemon alive");
    daemon_thread
        .join()
        .expect("daemon thread")
        .expect("daemon shutdown");
    drop(passive);
}

#[test]
fn daemon_keeps_pty_and_mux_state_across_interactive_detach() {
    let socket = daemon_test_endpoint("daemon-test");
    let daemon = Daemon::new(&socket).without_user_config();
    let daemon_thread = thread::spawn(move || daemon.run_foreground());

    let mut commands = connect_command_retry(&socket);
    let session_name = "detach-fixture";
    commands
        .execute(CommandInvocation::new(
            "new-session",
            ["-d", "-s", session_name, terminal_line_echo_command()],
        ))
        .unwrap();
    let pane = commands
        .execute(CommandInvocation::new(
            "list-panes",
            ["-t", "detach-fixture:0", "-F", "#{pane_id}"],
        ))
        .unwrap()
        .trim()
        .parse::<PaneId>()
        .expect("fixture pane id");
    let pane_target = pane.to_string();
    let interactive = Arc::new(connect_interactive_retry(&socket));
    let (messages, reader) = spawn_reader(Arc::clone(&interactive));
    let mut terminal_state = TerminalTestState::default();
    interactive.attach(session_name).unwrap();
    wait_for(&messages, &mut terminal_state, |_, terminal_state| {
        terminal_state.attached
    });
    wait_for(&messages, &mut terminal_state, |_, terminal_state| {
        terminal_state
            .viewports
            .get(&pane)
            .is_some_and(|viewport| viewport_text(viewport).contains("zz-terminal-ready"))
    });

    interactive
        .execute(CommandInvocation::new(
            "copy-mode-search-prompt",
            ["-b", "-t", &pane_target],
        ))
        .unwrap();
    wait_for(&messages, &mut terminal_state, |message, _| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalUiCommand {
                    pane: target,
                    command: TerminalUiCommand::BeginSearch {
                        direction: SearchDirection::Backward,
                    },
                },
                ..
            })
            if *target == pane
        )
    });

    commands
        .execute(CommandInvocation::new("copy-mode", ["-t", &pane_target]))
        .unwrap();
    wait_for(&messages, &mut terminal_state, |_, terminal_state| {
        terminal_state
            .viewports
            .get(&pane)
            .is_some_and(|viewport| matches!(viewport.mode, TerminalMode::Copy { .. }))
    });
    let frozen_mode = commands
        .execute(CommandInvocation::new(
            "capture-pane",
            ["-M", "-p", "-t", &pane_target],
        ))
        .unwrap();
    commands
        .execute(CommandInvocation::new("set-buffer", ["MODE_LIVE_ONLY\n"]))
        .unwrap();
    commands
        .execute(CommandInvocation::new(
            "paste-buffer",
            ["-d", "-t", &pane_target],
        ))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let live = commands
            .execute(CommandInvocation::new(
                "capture-pane",
                ["-p", "-t", &pane_target],
            ))
            .unwrap();
        if live.contains("MODE_LIVE_ONLY") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "live PTY did not advance under copy mode"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let still_frozen = commands
        .execute(CommandInvocation::new(
            "capture-pane",
            ["-M", "-p", "-t", &pane_target],
        ))
        .unwrap();
    assert_eq!(still_frozen, frozen_mode);
    assert!(!still_frozen.contains("MODE_LIVE_ONLY"));
    wait_for(&messages, &mut terminal_state, |_, terminal_state| {
        terminal_state.viewports.get(&pane).is_some_and(|viewport| {
            matches!(viewport.mode, TerminalMode::Copy { .. })
                && viewport.unseen_output > 0
                && !viewport_text(viewport).contains("MODE_LIVE_ONLY")
        })
    });
    commands
        .execute(CommandInvocation::new(
            "send-keys",
            ["-t", &pane_target, "-X", "cancel"],
        ))
        .unwrap();
    wait_for(&messages, &mut terminal_state, |_, terminal_state| {
        terminal_state.viewports.get(&pane).is_some_and(|viewport| {
            viewport.mode == TerminalMode::Live
                && viewport_text(viewport).contains("MODE_LIVE_ONLY")
        })
    });

    commands
        .execute(CommandInvocation::new("new-window", ["-t", session_name]))
        .unwrap();
    wait_for(&messages, &mut terminal_state, |_, terminal_state| {
        terminal_state
            .snapshot
            .sessions
            .iter()
            .find(|session| session.name == session_name)
            .is_some_and(|session| session.windows.len() == 2)
    });

    commands
        .execute(CommandInvocation::new(
            "send-keys",
            ["-t", &pane_target, "E2E_DAEMON_OK", "Enter"],
        ))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let captured = commands
            .execute(CommandInvocation::new(
                "capture-pane",
                ["-p", "-t", &pane_target],
            ))
            .unwrap();
        if captured.contains("E2E_DAEMON_OK") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "hidden terminal did not produce output"
        );
        thread::sleep(Duration::from_millis(10));
    }
    while let Ok(message) = messages.try_recv() {
        terminal_state.observe(&message);
    }
    assert!(
        terminal_state
            .viewports
            .get(&pane)
            .is_none_or(|viewport| !viewport_text(viewport).contains("E2E_DAEMON_OK")),
        "hidden windows must not receive terminal frames"
    );

    commands
        .execute(CommandInvocation::new(
            "select-window",
            ["-t", "detach-fixture:0"],
        ))
        .unwrap();
    wait_for(&messages, &mut terminal_state, |_, terminal_state| {
        terminal_state
            .viewports
            .get(&pane)
            .is_some_and(|viewport| viewport_text(viewport).contains("E2E_DAEMON_OK"))
    });
    commands
        .execute(CommandInvocation::new(
            "send-keys",
            ["-t", &pane_target, "PATCH_OK", "Enter"],
        ))
        .unwrap();
    wait_for(&messages, &mut terminal_state, |_, terminal_state| {
        terminal_state
            .viewports
            .get(&pane)
            .is_some_and(|viewport| viewport_text(viewport).contains("PATCH_OK"))
    });

    interactive.detach().unwrap();
    interactive.attach(session_name).unwrap();
    wait_for(&messages, &mut terminal_state, |_, terminal_state| {
        terminal_state.attached
    });
    let reattached = commands
        .execute(CommandInvocation::new(
            "capture-pane",
            ["-p", "-t", &pane_target],
        ))
        .unwrap();
    assert!(reattached.contains("E2E_DAEMON_OK"));
    assert!(reattached.contains("PATCH_OK"));

    terminal_state = TerminalTestState::default();
    interactive.request_resync().unwrap();
    wait_for(&messages, &mut terminal_state, |_, terminal_state| {
        terminal_state.viewports.get(&pane).is_some_and(|viewport| {
            let text = viewport_text(viewport);
            text.contains("E2E_DAEMON_OK") && text.contains("PATCH_OK")
        })
    });
    drop(messages);
    interactive.detach().unwrap();
    drop(interactive);
    reader.join().unwrap();

    commands
        .execute(CommandInvocation::new("kill-server", [] as [&str; 0]))
        .unwrap();
    daemon_thread.join().unwrap().unwrap();
}

#[cfg(unix)]
#[test]
fn kitty_images_and_placements_reach_an_interactive_client() {
    let socket = std::env::temp_dir().join(format!(
        "zz-kitty-e2e-{}-{}.sock",
        std::process::id(),
        server_id()
    ));
    let daemon = Daemon::new(&socket).without_user_config();
    let daemon_thread = thread::spawn(move || daemon.run_foreground());

    let mut commands = connect_command_retry(&socket);
    let interactive = Arc::new(connect_interactive_retry(&socket));
    let (messages, reader) = spawn_reader(Arc::clone(&interactive));
    let mut terminal_state = TerminalTestState::default();
    interactive.attach("").unwrap();
    wait_for(&messages, &mut terminal_state, |_, terminal_state| {
        terminal_state.attached
    });
    interactive
        .send_input(InputMessage::ResizeTerminal {
            pane: PaneId(0),
            columns: 100,
            rows: 30,
            cell_width_px: 8,
            cell_height_px: 18,
        })
        .unwrap();

    commands
        .execute(CommandInvocation::new(
            "send-keys",
            [
                "-t",
                "%0",
                r"printf '\033_Ga=T,f=24,s=1,v=1,i=77;/wAA\033\\'",
                "Enter",
            ],
        ))
        .unwrap();

    let mut saw_begin = false;
    let mut pixel_bytes = 0_usize;
    wait_for(&messages, &mut terminal_state, |message, terminal_state| {
        if let ProtocolMessage::Event(Event { payload, .. }) = message {
            match payload {
                EventPayload::KittyImageBegin {
                    pane,
                    image_id,
                    width,
                    height,
                    total_bytes,
                    ..
                } => {
                    assert_eq!((*pane, *image_id), (PaneId(0), 77));
                    assert_eq!((*width, *height, *total_bytes), (1, 1, 4));
                    saw_begin = true;
                }
                EventPayload::KittyImageChunk {
                    image_id, bytes, ..
                } => {
                    assert!(saw_begin, "pixel chunk arrived before its header");
                    assert_eq!(*image_id, 77);
                    pixel_bytes += bytes.len();
                }
                _ => {}
            }
        }
        saw_begin
            && pixel_bytes == 4
            && terminal_state
                .viewports
                .get(&PaneId(0))
                .is_some_and(|viewport| {
                    viewport
                        .kitty_placements
                        .iter()
                        .any(|placement| placement.image_id == 77)
                })
    });

    drop(messages);
    interactive.detach().unwrap();
    drop(interactive);
    reader.join().unwrap();
    commands
        .execute(CommandInvocation::new("kill-server", [] as [&str; 0]))
        .unwrap();
    daemon_thread.join().unwrap().unwrap();
}

#[cfg(unix)]
#[test]
fn startup_owner_control_receives_causes_immediately_after_server_hello() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    *shared.startup_config_causes.lock() = Some(vec!["owner cause".to_owned()]);
    let (mut client_stream, server_stream) =
        std::os::unix::net::UnixStream::pair().expect("create paired stream");
    client_stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    let connection_shared = Arc::clone(&shared);
    let connection = thread::spawn(move || handle_connection(server_stream, &connection_shared));

    zz_protocol::write_protocol_message(
        &mut client_stream,
        &ProtocolMessage::ClientHello(ClientHello {
            protocol_version: PROTOCOL_VERSION,
            client_instance_id: ClientInstanceId(1),
            kind: ClientKind::Control,
            device_name: None,
            capabilities: vec![ClientHello::STARTUP_CONFIG_OWNER_CAPABILITY.to_owned()],
            color_scheme: None,
            origin: None,
            environment: Vec::new(),
            working_directory: None,
            process_id: 0,
            revision: zz_protocol::REVISION,
        }),
    )
    .expect("send owner hello");
    assert!(matches!(
        zz_protocol::read_protocol_message(&mut client_stream).expect("server hello"),
        ProtocolMessage::ServerHello(_)
    ));
    assert!(matches!(
        zz_protocol::read_protocol_message(&mut client_stream).expect("startup causes"),
        ProtocolMessage::Event(Event {
            payload: EventPayload::StartupConfigCauses { causes },
            ..
        }) if causes == ["owner cause"]
    ));
    assert!(shared.startup_config_causes.lock().is_none());

    drop(client_stream);
    connection
        .join()
        .expect("connection thread")
        .expect("connection");
}

#[cfg(unix)]
#[test]
fn interactive_connection_fires_attach_and_detach_event_hooks() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    let mut setup_context = ExecutionContext::default();
    for (name, value) in [
        (
            "client-session-changed",
            "display-message 'session=#{hook_client}'",
        ),
        (
            "client-attached",
            "display-message 'attached=#{hook_client}'",
        ),
        (
            "client-detached",
            "display-message 'detached=#{hook_client}'",
        ),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut setup_context,
                &CommandInvocation::new("set-hook", ["-g", name, value]),
            )
            .expect("client event hook");
    }
    shared.inner.lock().message_log.clear();

    let (mut client_stream, server_stream) =
        std::os::unix::net::UnixStream::pair().expect("create paired stream");
    client_stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    let connection_shared = Arc::clone(&shared);
    let connection = thread::spawn(move || handle_connection(server_stream, &connection_shared));
    zz_protocol::write_protocol_message(
        &mut client_stream,
        &ProtocolMessage::ClientHello(ClientHello {
            protocol_version: PROTOCOL_VERSION,
            client_instance_id: ClientInstanceId(1),
            kind: ClientKind::Interactive,
            device_name: Some("hook-client".to_owned()),
            capabilities: Vec::new(),
            color_scheme: None,
            origin: None,
            environment: Vec::new(),
            working_directory: None,
            process_id: 0,
            revision: zz_protocol::REVISION,
        }),
    )
    .expect("send hello");
    assert!(matches!(
        zz_protocol::read_protocol_message(&mut client_stream).expect("server hello"),
        ProtocolMessage::ServerHello(_)
    ));
    zz_protocol::write_protocol_message(
        &mut client_stream,
        &ProtocolMessage::Attach {
            session: String::new(),
        },
    )
    .expect("attach");
    loop {
        if matches!(
            zz_protocol::read_protocol_message(&mut client_stream).expect("attach response"),
            ProtocolMessage::Attached { .. }
        ) {
            break;
        }
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while shared.inner.lock().message_log.len() < 2 {
        assert!(Instant::now() < deadline, "attach hooks did not fire");
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        shared
            .inner
            .lock()
            .message_log
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        ["session=hook-client", "attached=hook-client"]
    );

    zz_protocol::write_protocol_message(&mut client_stream, &ProtocolMessage::Detach)
        .expect("detach");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if shared
            .inner
            .lock()
            .message_log
            .back()
            .is_some_and(|message| message.text == "detached=hook-client")
        {
            break;
        }
        assert!(Instant::now() < deadline, "detach hook did not fire");
        thread::sleep(Duration::from_millis(5));
    }
    drop(client_stream);
    connection
        .join()
        .expect("connection thread")
        .expect("connection");
}

#[cfg(unix)]
#[test]
fn control_connection_attaches_rejects_input_and_sends_no_terminal_frames() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");
    let (mut client_stream, server_stream) =
        std::os::unix::net::UnixStream::pair().expect("create paired stream");
    client_stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    let connection_shared = Arc::clone(&shared);
    let connection = thread::spawn(move || handle_connection(server_stream, &connection_shared));

    zz_protocol::write_protocol_message(
        &mut client_stream,
        &ProtocolMessage::ClientHello(ClientHello {
            protocol_version: PROTOCOL_VERSION,
            client_instance_id: ClientInstanceId(1),
            kind: ClientKind::Control,
            device_name: None,
            capabilities: Vec::new(),
            color_scheme: None,
            origin: None,
            environment: Vec::new(),
            working_directory: None,
            process_id: 0,
            revision: zz_protocol::REVISION,
        }),
    )
    .expect("send control hello");
    assert!(matches!(
        zz_protocol::read_protocol_message(&mut client_stream).expect("server hello"),
        ProtocolMessage::ServerHello(_)
    ));

    zz_protocol::write_protocol_message(
        &mut client_stream,
        &ProtocolMessage::Attach {
            session: String::new(),
        },
    )
    .expect("attach control client");
    let pane = loop {
        let message = zz_protocol::read_protocol_message(&mut client_stream)
            .expect("control attach response");
        assert!(!matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalViewport { .. } | EventPayload::TerminalPatch { .. },
                ..
            })
        ));
        if let ProtocolMessage::Attached { snapshot, .. } = message {
            break snapshot.sessions[0].windows[0].active_pane;
        }
    };

    zz_protocol::write_protocol_message(
        &mut client_stream,
        &ProtocolMessage::Input(InputMessage::ResizeTerminal {
            pane,
            columns: 90,
            rows: 30,
            cell_width_px: 8,
            cell_height_px: 18,
        }),
    )
    .expect("send forbidden input");
    loop {
        let message =
            zz_protocol::read_protocol_message(&mut client_stream).expect("input rejection");
        assert!(!matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalViewport { .. } | EventPayload::TerminalPatch { .. },
                ..
            })
        ));
        if matches!(
            message,
            ProtocolMessage::CommandResponse(CommandResponse::Error {
                request_id: 0,
                error: ServerError::InvalidCommand(message),
                ..
            }) if message == "command client cannot send input"
        ) {
            break;
        }
    }

    drop(client_stream);
    connection
        .join()
        .expect("connection thread")
        .expect("connection");
}

#[cfg(unix)]
#[test]
fn detach_keeps_one_connection_restores_views_and_clears_client_state() {
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize daemon state");

    let (mut client_stream, server_stream) =
        std::os::unix::net::UnixStream::pair().expect("create in-memory client connection");
    client_stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("set client read timeout");
    let connection_shared = Arc::clone(&shared);
    let connection = thread::spawn(move || handle_connection(server_stream, &connection_shared));

    zz_protocol::write_protocol_message(
        &mut client_stream,
        &ProtocolMessage::ClientHello(ClientHello {
            protocol_version: PROTOCOL_VERSION,
            client_instance_id: ClientInstanceId(1),
            kind: ClientKind::Interactive,
            device_name: Some("same-connection".to_owned()),
            capabilities: Vec::new(),
            color_scheme: None,
            origin: None,
            environment: Vec::new(),
            working_directory: None,
            process_id: 0,
            revision: zz_protocol::REVISION,
        }),
    )
    .expect("send client hello");
    let client = match zz_protocol::read_protocol_message(&mut client_stream)
        .expect("receive server hello")
    {
        ProtocolMessage::ServerHello(hello) => hello.client_id,
        other => panic!("expected ServerHello, got {other:?}"),
    };

    zz_protocol::write_protocol_message(
        &mut client_stream,
        &ProtocolMessage::Attach {
            session: String::new(),
        },
    )
    .expect("send first attach");
    let (session, pane) = loop {
        if let ProtocolMessage::Attached {
            session, snapshot, ..
        } =
            zz_protocol::read_protocol_message(&mut client_stream).expect("receive first Attached")
        {
            let pane = snapshot.sessions[0].windows[0].active_pane;
            break (session, pane);
        }
    };
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    let view = TerminalViewId(client.0);

    zz_protocol::write_protocol_message(
        &mut client_stream,
        &ProtocolMessage::Input(InputMessage::TerminalView {
            pane,
            action: TerminalViewAction::EnterCopyMode,
        }),
    )
    .expect("enter copy mode");
    let mut terminal_state = TerminalTestState::default();
    while !terminal_state
        .viewports
        .get(&pane)
        .is_some_and(|viewport| matches!(viewport.mode, TerminalMode::Copy { .. }))
    {
        let message = zz_protocol::read_protocol_message(&mut client_stream)
            .expect("receive copy-mode viewport");
        terminal_state.observe(&message);
    }
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("copy-mode"),
        "entering copy mode owns the client's key table"
    );

    zz_protocol::write_protocol_message(
        &mut client_stream,
        &ProtocolMessage::Input(InputMessage::TerminalView {
            pane,
            action: TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Cancel),
        }),
    )
    .expect("leave copy mode");
    while !terminal_state
        .viewports
        .get(&pane)
        .is_some_and(|viewport| viewport.mode == TerminalMode::Live)
    {
        let message =
            zz_protocol::read_protocol_message(&mut client_stream).expect("receive live viewport");
        terminal_state.observe(&message);
    }

    zz_protocol::write_protocol_message(
        &mut client_stream,
        &ProtocolMessage::Input(InputMessage::Key {
            pane,
            input: KeyInput {
                action: KeyAction::Press,
                key: KeyCode::Character('b'),
                modifiers: Modifiers::new(false, true, false, false),
                text: None,
                unshifted_codepoint: Some('b'),
            },
            text_follows: false,
        }),
    )
    .expect("arm prefix");
    loop {
        let message = zz_protocol::read_protocol_message(&mut client_stream)
            .expect("receive armed-prefix event");
        terminal_state.observe(&message);
        if matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::PrefixArmed { armed: true },
                ..
            })
        ) {
            break;
        }
    }

    let (reply, response) = crossbeam_channel::bounded(1);
    {
        let mut inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            Some("prefix")
        );
        assert!(prefix_published(&inner, client));
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.swallowed_keys.is_some())
        );
        inner
            .client_entry(client)
            .suppressed_text
            .replace(BTreeMap::from([('b', 1)]));
        inner
            .client_entry(client)
            .pending_committed_text
            .replace(VecDeque::from([PendingCommittedText {
                pane,
                lane: CommittedTextLane::Terminal,
                suppressed_character: Some('b'),
            }]));
        inner.pending_gui_requests.insert(
            99,
            PendingGuiRequest {
                client,
                reply: reply.into(),
                screenshot: None,
            },
        );
    }

    zz_protocol::write_protocol_message(&mut client_stream, &ProtocolMessage::Detach)
        .expect("detach without closing the connection");
    loop {
        let message = zz_protocol::read_protocol_message(&mut client_stream)
            .expect("receive disarmed-prefix event");
        terminal_state.observe(&message);
        if matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::PrefixArmed { armed: false },
                ..
            })
        ) {
            break;
        }
    }
    assert!(
        response
            .recv_timeout(Duration::from_secs(1))
            .expect("detaching must fail the in-flight GUI request")
            .is_err()
    );
    {
        let inner = shared.inner.lock();
        assert!(inner.client(client).is_some_and(|c| c.subscriber.is_some()));
        assert!(
            inner
                .attached
                .values()
                .all(|clients| !clients.contains(&client))
        );
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.visible_terminals.is_none())
        );
        assert!(inner.client(client).is_none_or(|c| c.key_engine.is_none()));
        assert!(!prefix_published(&inner, client));
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.swallowed_keys.is_none())
        );
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.suppressed_text.is_none())
        );
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.pending_committed_text.is_none())
        );
        assert!(!inner.pending_gui_requests.contains_key(&99));
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while terminal.latest_viewport_for(view).is_some() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        terminal.latest_viewport_for(view).is_none(),
        "detach must park the terminal view"
    );

    let pending_attach_input = chooser_key_press("a");
    shared.enqueue_pending_committed_text(
        client,
        pane,
        CommittedTextLane::Terminal,
        &pending_attach_input,
        true,
    );
    shared.suppress_committed_character(
        client,
        pane,
        CommittedTextLane::Terminal,
        &pending_attach_input,
        true,
    );
    zz_protocol::write_protocol_message(
        &mut client_stream,
        &ProtocolMessage::Attach {
            session: session.to_string(),
        },
    )
    .expect("re-attach on the same connection");
    loop {
        if let ProtocolMessage::Attached {
            session: attached, ..
        } =
            zz_protocol::read_protocol_message(&mut client_stream).expect("receive second Attached")
        {
            assert_eq!(attached, session);
            break;
        }
    }
    wait_for_viewport(
        &terminal,
        view,
        "reattached terminal view never became available",
        |_| true,
    );
    terminal_state = TerminalTestState::default();
    zz_protocol::write_protocol_message(&mut client_stream, &ProtocolMessage::Resync)
        .expect("request a full restored terminal view");
    while !terminal_state
        .viewports
        .get(&pane)
        .is_some_and(|viewport| viewport.mode == TerminalMode::Live)
    {
        let message = zz_protocol::read_protocol_message(&mut client_stream)
            .expect("receive restored terminal view");
        terminal_state.observe(&message);
    }
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.attached.get(&session),
            Some(&BTreeSet::from([client]))
        );
        assert!(inner.client(client).is_none_or(|c| c.key_engine.is_none()));
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.copy_session.is_none())
        );
        assert!(!prefix_published(&inner, client));
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.pending_committed_text.is_none())
        );
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.suppressed_text.is_none())
        );
    }

    drop(client_stream);
    connection
        .join()
        .expect("connection handler panicked")
        .expect("connection handler failed");
}
