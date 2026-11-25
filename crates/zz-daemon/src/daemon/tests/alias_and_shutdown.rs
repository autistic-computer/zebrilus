use super::*;

#[test]
fn control_alias_groups_emit_only_child_guards_and_stop_after_failure() {
    let shared = Arc::new(Shared::new(1));
    switch_test_session(&shared, "work");
    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    take_reliable_messages(&mailbox);
    let mut context = ExecutionContext::default();
    for (index, value) in [
        ("42", "multi=display-message -p first ; display-message -p"),
        ("43", "empty="),
        (
            "44",
            "failing=display-message -p before\nhas-session -t missing\ndisplay-message -p after",
        ),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    ["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .expect("alias write");
    }

    let prepare = |name: &str, args: Vec<&str>| {
        let command = shared
            .prepare_command_list(vec![CommandInvocation::new(name, args).with_source(
                SourceSpan {
                    source: "<control>".to_owned(),
                    line: 1,
                    column: 1,
                },
            )])
            .remove(0);
        assert_eq!(command.result, PreparedCommandResult::Ready);
        assert!(MuxEngine::is_command_alias_group(&command.invocation));
        command.invocation
    };
    let multi = prepare("multi", vec!["last"]);
    assert!(matches!(
        shared.execute_command_request_with_prepared(
            control,
            ClientKind::Control,
            &mut context,
            42,
            &multi,
            true,
        ),
        CommandResponse::Success { exit_code: 0, output, .. } if output.is_empty()
    ));
    let guards = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ControlCommandGuard {
                        output,
                        error,
                        flags,
                        ..
                    },
                ..
            }) => Some((String::from(output), error, flags)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        guards,
        [
            (
                "first".to_owned(),
                false,
                CONTROL_COMMAND_FRAME_FLAGS_CONTROL
            ),
            (
                "last".to_owned(),
                false,
                CONTROL_COMMAND_FRAME_FLAGS_CONTROL
            ),
        ]
    );

    let empty = prepare("empty", vec!["discarded"]);
    assert!(matches!(
        shared.execute_command_request_with_prepared(
            control,
            ClientKind::Control,
            &mut context,
            43,
            &empty,
            true,
        ),
        CommandResponse::Success { exit_code: 0, output, .. } if output.is_empty()
    ));
    assert!(take_reliable_messages(&mailbox).is_empty());

    let failing = prepare("failing", Vec::new());
    assert!(matches!(
        shared.execute_command_request_with_prepared(
            control,
            ClientKind::Control,
            &mut context,
            44,
            &failing,
            true,
        ),
        CommandResponse::Error { output, .. } if output.is_empty()
    ));
    let guards = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlCommandGuard { output, error, .. },
                ..
            }) => Some((String::from(output), error)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(guards.len(), 2);
    assert_eq!(guards[0], ("before".to_owned(), false));
    assert!(guards[1].1);
    assert!(!guards.iter().any(|(output, _)| output == "after"));
}

#[test]
fn control_alias_source_failure_stops_the_remaining_children() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let missing = directory.path().join("missing.conf");
    let shared = Arc::new(Shared::new(1));
    switch_test_session(&shared, "work");
    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    take_reliable_messages(&mailbox);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[53]".to_owned(),
                    format!(
                        "missing-source=display-message -p before ; source-file '{}' ; set-option -g @after-missing-source yes",
                        missing.display(),
                    ),
                ],
            ),
        )
        .expect("source alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new(
            "missing-source",
            [] as [&str; 0],
        )])
        .remove(0)
        .invocation;

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            control,
            ClientKind::Control,
            &mut context,
            53,
            &prepared,
            true,
        ),
        CommandResponse::Error {
            error: ServerError::InvalidCommand(message),
            ..
        } if message == missing_source_error(&missing)
    ));
    assert_eq!(
        control_command_guards(take_reliable_messages(&mailbox)),
        [
            ("before".to_owned(), false, false),
            (missing_source_error(&missing), true, true),
        ]
    );
    let command = ClientId(u64::from(u16::MAX));
    assert!(matches!(
        shared.execute_command_request_with_prepared(
            command,
            ClientKind::Command,
            &mut context,
            54,
            &prepared,
            true,
        ),
        CommandResponse::Success {
            exit_code: 1,
            stderr,
            ..
        } if stderr.contains(&missing_source_error(&missing))
    ));
    assert!(read_global_option(&shared, "@after-missing-source").is_empty());
}

#[cfg(unix)]
#[test]
fn control_alias_command_exits_end_and_continue() {
    let shared = Arc::new(Shared::new(1));
    switch_test_session(&shared, "work");
    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    take_reliable_messages(&mailbox);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[45]",
                    "shell=display-message -p before ; run-shell 'exit 3' ; display-message -p after",
                ],
            ),
        )
        .expect("shell alias write");
    let shell = shared
        .prepare_command_list(vec![
            CommandInvocation::new("shell", [] as [&str; 0]).with_source(SourceSpan {
                source: "<control>".to_owned(),
                line: 1,
                column: 1,
            }),
        ])
        .remove(0)
        .invocation;

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            control,
            ClientKind::Control,
            &mut context,
            45,
            &shell,
            true,
        ),
        CommandResponse::Success { exit_code: 3, output, .. } if output.is_empty()
    ));
    let timeline = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ControlCommandGuard {
                        output,
                        error,
                        flags,
                        ..
                    },
                ..
            }) => Some(format!("guard:{flags}:{error}:{output}")),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlCommandOutput { output },
                ..
            }) => Some(format!("raw:{output}")),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        timeline,
        [
            "guard:1:false:before".to_owned(),
            "guard:1:false:".to_owned(),
            "raw:'exit 3' returned 3".to_owned(),
            "guard:1:false:after".to_owned(),
        ]
    );
}

#[test]
fn control_alias_hooks_emit_only_child_guards() {
    let shared = Arc::new(Shared::new(1));
    let (session, window, pane) = switch_test_session(&shared, "work");
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    for (index, value) in [
        (
            "46",
            "hookmulti=display-message -p alias-one ; display-message -p alias-two",
        ),
        ("47", "hookempty="),
    ] {
        shared
            .execute(
                command,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    ["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .expect("alias write");
    }
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-hook", ["-g", "after-new-window", "hookmulti"]),
        )
        .expect("multi alias hook");

    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(control, session).expect("control attach");
    take_reliable_messages(&mailbox);

    assert!(matches!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            46,
            &CommandInvocation::new("new-window", ["-d"]),
        ),
        CommandResponse::Success { exit_code: 0, .. }
    ));
    assert_eq!(
        control_command_frames(take_reliable_messages(&mailbox)),
        [
            (
                "alias-one".to_owned(),
                false,
                false,
                CONTROL_COMMAND_FRAME_FLAGS_NONE,
            ),
            (
                "alias-two".to_owned(),
                false,
                false,
                CONTROL_COMMAND_FRAME_FLAGS_NONE,
            ),
        ]
    );

    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-hook", ["-g", "after-new-window", "hookempty"]),
        )
        .expect("empty alias hook");
    assert!(matches!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            47,
            &CommandInvocation::new("new-window", ["-d"]),
        ),
        CommandResponse::Success { exit_code: 0, .. }
    ));
    assert_eq!(control_command_frames(take_reliable_messages(&mailbox)), []);
}

#[test]
fn control_source_replay_accepts_alias_groups() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("alias-group.conf");
    fs::write(&source, "control-source-group\n").expect("alias source");
    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "alias-source"]),
        )
        .expect("source session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[48]",
                    "control-source-group=display-message -p FIRST ; display-message -p SECOND",
                ],
            ),
        )
        .expect("source alias");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(control, context.session.expect("source session id"))
        .expect("source control attach");
    take_reliable_messages(&mailbox);

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            48,
            &CommandInvocation::new("source-file", [source.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 48,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_frames(take_reliable_messages(&mailbox)),
        [
            (
                "FIRST".to_owned(),
                false,
                false,
                CONTROL_COMMAND_FRAME_FLAGS_NONE,
            ),
            (
                "SECOND".to_owned(),
                false,
                false,
                CONTROL_COMMAND_FRAME_FLAGS_NONE,
            ),
        ]
    );
}

#[test]
fn control_alias_self_shutdown_marks_the_kill_guard_before_clean_exit() {
    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[61]",
                    "self-stop=kill-server ; display-message -p AFTER",
                ],
            ),
        )
        .expect("self-stop alias");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new("self-stop", [] as [&str; 0])])
        .remove(0)
        .invocation;

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            control,
            ClientKind::Control,
            &mut context,
            61,
            &prepared,
            true,
        ),
        CommandResponse::Success { exit_code: 0, .. }
    ));
    shared.announce_shutdown();
    let events = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlCommandGuard { output, .. },
                ..
            }) => Some(format!("guard:{output}")),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlExit { reason },
                ..
            }) if reason.is_empty() => Some("clean-exit".to_owned()),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ServerStopping,
                ..
            }) => Some("server-stopping".to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        events,
        ["guard:", "clean-exit", "guard:AFTER", "server-stopping"]
    );
}

#[test]
fn control_foreground_callback_aggregates_only_after_forced_shutdown() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    take_reliable_messages(&mailbox);

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            62,
            &CommandInvocation::new(
                "run-shell",
                ["-C", "display-message -F MATCH one ; kill-server"],
            ),
        ),
        CommandResponse::Success {
            request_id: 62,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_frames(take_reliable_messages(&mailbox)),
        [(
            "only one of -F or argument must be given".to_owned(),
            true,
            false,
            CONTROL_COMMAND_FRAME_FLAGS_NONE,
        )]
    );
    assert!(!shared.stopping.load(Ordering::Acquire));
}

#[cfg(unix)]
#[test]
fn command_alias_drains_children_before_server_shutdown() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[49]".to_owned(),
                    format!(
                        "shutdown-alias=kill-server ; wait-for never ; run-shell 'printf first > {}' ; run-shell 'printf second > {}'",
                        first.display(),
                        second.display(),
                    ),
                ],
            ),
        )
        .expect("shutdown alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new(
            "shutdown-alias",
            [] as [&str; 0],
        )])
        .remove(0)
        .invocation;

    assert_eq!(
        shared.execute_command_request_with_prepared(
            command,
            ClientKind::Command,
            &mut context,
            49,
            &prepared,
            true,
        ),
        CommandResponse::Success {
            request_id: 49,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    assert_eq!(wait_for_file_text(&first, deadline), "first");
    assert!(!second.exists());
    while !shared.stopping.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[test]
fn command_alias_rechecks_transient_empty_state_at_queue_completion() {
    let shared = Arc::new(Shared::new(1));
    switch_test_session(&shared, "old");
    shared.initialize(false).expect("initialize daemon state");
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-s", "exit-empty", "on"]),
        )
        .expect("exit-empty option");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[50]",
                    "replace=kill-session -t old ; new-session -d -s replacement ; set-option -g @after-replace yes",
                ],
            ),
        )
        .expect("replacement alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new("replace", [] as [&str; 0])])
        .remove(0)
        .invocation;

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            command,
            ClientKind::Command,
            &mut context,
            50,
            &prepared,
            true,
        ),
        CommandResponse::Success { exit_code: 0, .. }
    ));
    assert!(
        shared
            .inner
            .lock()
            .engine
            .state
            .sessions
            .values()
            .any(|session| session.name == "replacement")
    );
    assert_eq!(read_global_option(&shared, "@after-replace"), "yes");
    assert!(!shared.stopping.load(Ordering::Acquire));
}

#[test]
fn command_alias_defers_structural_hooks_until_its_children_finish() {
    let shared = Arc::new(Shared::new(1));
    switch_test_session(&shared, "anchor");
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "session-created",
                    "set-option -g @alias-event visible",
                ],
            ),
        )
        .expect("session-created hook");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[59]",
                    "event-order=new-session -d -s created ; display-message -p '#{@alias-event}'",
                ],
            ),
        )
        .expect("event-order alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new("event-order", [] as [&str; 0])])
        .remove(0)
        .invocation;

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            command,
            ClientKind::Command,
            &mut context,
            59,
            &prepared,
            true,
        ),
        CommandResponse::Success {
            output,
            exit_code: 0,
            ..
        } if output == "\n"
    ));
    assert_eq!(read_global_option(&shared, "@alias-event"), "visible");
}

#[cfg(unix)]
#[test]
fn forced_alias_shutdown_discards_structural_hooks_queued_before_kill_server() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let marker = directory.path().join("closed");
    let shared = Arc::new(Shared::new(1));
    switch_test_session(&shared, "old");
    switch_test_session(&shared, "survivor");
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g".to_owned(),
                    "session-closed".to_owned(),
                    format!("run-shell 'printf x >> {}'", marker.display()),
                ],
            ),
        )
        .expect("session-closed hook");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[60]",
                    "force-order=kill-session -t old ; kill-server",
                ],
            ),
        )
        .expect("force-order alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new("force-order", [] as [&str; 0])])
        .remove(0)
        .invocation;

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            command,
            ClientKind::Command,
            &mut context,
            60,
            &prepared,
            true,
        ),
        CommandResponse::Success { exit_code: 0, .. }
    ));
    assert_eq!(fs::read_to_string(marker).expect("close hook marker"), "x");
}

#[cfg(unix)]
#[test]
fn nested_alias_queue_bubbles_shutdown_and_yield_to_its_parent() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let nested = directory.path().join("nested");
    let outer = directory.path().join("outer");
    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[51]".to_owned(),
                    format!(
                        "nested-shutdown=run-shell -C {{ kill-server ; wait-for never ; run-shell 'printf nested > {}' }} ; run-shell 'printf outer > {}'",
                        nested.display(),
                        outer.display(),
                    ),
                ],
            ),
        )
        .expect("nested shutdown alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new(
            "nested-shutdown",
            [] as [&str; 0],
        )])
        .remove(0)
        .invocation;

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            command,
            ClientKind::Command,
            &mut context,
            51,
            &prepared,
            true,
        ),
        CommandResponse::Success { exit_code: 0, .. }
    ));
    let deadline = Instant::now() + Duration::from_secs(2);
    assert_eq!(wait_for_file_text(&nested, deadline), "nested");
    assert!(!outer.exists());
    while !shared.stopping.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[cfg(unix)]
#[test]
fn sourced_wait_yields_an_inherited_shutdown_queue() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("child.conf");
    let nested = directory.path().join("nested");
    let outer = directory.path().join("outer");
    fs::write(
        &source,
        format!(
            "wait-for never ; run-shell 'printf nested > {}'\n",
            nested.display()
        ),
    )
    .expect("source body");
    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[52]".to_owned(),
                    format!(
                        "source-shutdown=kill-server ; source-file '{}' ; run-shell 'printf outer > {}'",
                        source.display(),
                        outer.display(),
                    ),
                ],
            ),
        )
        .expect("source shutdown alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new(
            "source-shutdown",
            [] as [&str; 0],
        )])
        .remove(0)
        .invocation;

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            command,
            ClientKind::Command,
            &mut context,
            52,
            &prepared,
            true,
        ),
        CommandResponse::Success { exit_code: 0, .. }
    ));
    thread::sleep(Duration::from_millis(150));
    assert!(!nested.exists());
    assert!(!outer.exists());
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[cfg(unix)]
#[test]
fn nested_source_yields_an_inherited_shutdown_queue_at_its_root() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let child = directory.path().join("child.conf");
    let grandchild = directory.path().join("grandchild.conf");
    fs::write(&grandchild, "set-option -g @grandchild yes\n").expect("grandchild source");
    fs::write(
        &child,
        format!(
            "set-option -g @before-nested-source yes\nsource-file '{}'\nset-option -g @after-nested-source yes\n",
            grandchild.display()
        ),
    )
    .expect("child source");
    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[59]".to_owned(),
                    format!(
                        "nested-source-shutdown=kill-server ; source-file '{}' ; set-option -g @after-outer-source yes",
                        child.display()
                    ),
                ],
            ),
        )
        .expect("nested source shutdown alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new(
            "nested-source-shutdown",
            [] as [&str; 0],
        )])
        .remove(0)
        .invocation;

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            command,
            ClientKind::Command,
            &mut context,
            59,
            &prepared,
            true,
        ),
        CommandResponse::Success { exit_code: 0, .. }
    ));
    assert_eq!(read_global_option(&shared, "@before-nested-source"), "yes");
    assert!(read_global_option(&shared, "@grandchild").is_empty());
    assert!(read_global_option(&shared, "@after-nested-source").is_empty());
    assert!(read_global_option(&shared, "@after-outer-source").is_empty());
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[cfg(unix)]
#[test]
fn inherited_shutdown_suppresses_sourced_output_without_skipping_effects() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("child.conf");
    let outer = directory.path().join("outer");
    fs::write(
        &source,
        "set-option -g @sourced-effect yes\ndisplay-message -p CHILD\nhas-session -t missing\n",
    )
    .expect("source body");
    let shared = Arc::new(Shared::new(1));
    let (command, _) = shared
        .register(
            ClientKind::Command,
            ClientInstanceId::default(),
            None,
            None,
            false,
            false,
        )
        .expect("command client");
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[56]".to_owned(),
                    format!(
                        "source-output-shutdown=kill-server ; source-file '{}' ; display-message -p OUTER ; run-shell 'printf outer > {}'",
                        source.display(),
                        outer.display(),
                    ),
                ],
            ),
        )
        .expect("source output alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new(
            "source-output-shutdown",
            [] as [&str; 0],
        )])
        .remove(0)
        .invocation;

    assert_eq!(
        shared.execute_command_request_with_prepared(
            command,
            ClientKind::Command,
            &mut context,
            56,
            &prepared,
            true,
        ),
        CommandResponse::Success {
            request_id: 56,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(read_global_option(&shared, "@sourced-effect"), "yes");
    let deadline = Instant::now() + Duration::from_secs(2);
    assert_eq!(wait_for_file_text(&outer, deadline), "outer");
    while !shared.stopping.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[cfg(unix)]
#[test]
fn unattached_sourced_show_buffer_yields_an_inherited_shutdown_queue() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("child.conf");
    let outer = directory.path().join("outer");
    fs::write(&source, "show-buffer\n").expect("source body");
    let shared = Arc::new(Shared::new(1));
    let (command, _) = shared
        .register(
            ClientKind::Command,
            ClientInstanceId::default(),
            None,
            None,
            false,
            false,
        )
        .expect("command client");
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-buffer", ["payload"]),
        )
        .expect("paste buffer");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[57]".to_owned(),
                    format!(
                        "source-show-buffer-shutdown=kill-server ; source-file '{}' ; run-shell 'printf outer > {}'",
                        source.display(),
                        outer.display(),
                    ),
                ],
            ),
        )
        .expect("show-buffer alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new(
            "source-show-buffer-shutdown",
            [] as [&str; 0],
        )])
        .remove(0)
        .invocation;

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            command,
            ClientKind::Command,
            &mut context,
            57,
            &prepared,
            true,
        ),
        CommandResponse::Success {
            output,
            exit_code: 0,
            stderr,
            ..
        } if output.is_empty() && stderr.is_empty()
    ));
    assert!(!outer.exists());
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[cfg(unix)]
#[test]
fn shutdown_waits_for_a_draining_foreground_shell_job() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let marker = directory.path().join("foreground");
    let outer = directory.path().join("outer");
    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[58]".to_owned(),
                    format!(
                        "long-shell-shutdown=kill-server ; run-shell 'sleep 0.35; printf foreground > {}' ; run-shell 'printf outer > {}'",
                        marker.display(),
                        outer.display(),
                    ),
                ],
            ),
        )
        .expect("long shell alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new(
            "long-shell-shutdown",
            [] as [&str; 0],
        )])
        .remove(0)
        .invocation;

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            command,
            ClientKind::Command,
            &mut context,
            58,
            &prepared,
            true,
        ),
        CommandResponse::Success { exit_code: 0, .. }
    ));
    assert!(!marker.exists());
    assert!(!outer.exists());
    assert!(!shared.stopping.load(Ordering::Acquire));
    let deadline = Instant::now() + Duration::from_secs(2);
    assert_eq!(wait_for_file_text(&marker, deadline), "foreground");
    while !shared.stopping.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(shared.stopping.load(Ordering::Acquire));
    assert!(!outer.exists());
}

#[cfg(unix)]
#[test]
fn direct_shutdown_waits_for_preexisting_foreground_jobs() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let shell_marker = directory.path().join("shell");
    let shell_release = directory.path().join("release-1");

    let shared = Arc::new(Shared::new(1));
    let worker_shared = Arc::clone(&shared);
    let shell_command = format!(
        "while [ ! -e {} ]; do sleep 0.01; done; printf shell > {}",
        shell_quote(&shell_release),
        shell_quote(&shell_marker)
    );
    let shell = thread::spawn(move || {
        worker_shared.execute(
            ClientId(201),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("run-shell", [shell_command]),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.active_shutdown_blockers() != 1 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(shared.active_shutdown_blockers(), 1);
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("kill-server", [] as [&str; 0]),
        )
        .expect("direct shutdown");
    assert!(!shared.stopping.load(Ordering::Acquire));
    fs::write(&shell_release, b"").expect("release the shell job");
    shell
        .join()
        .expect("foreground shell worker")
        .expect("foreground shell result");
    assert_eq!(
        fs::read_to_string(&shell_marker).expect("shell marker"),
        "shell"
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while !shared.stopping.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(shared.stopping.load(Ordering::Acquire));

    for (server_id, client) in [(2, ClientId(202)), (3, ClientId(u64::MAX))] {
        let release = directory.path().join(format!("release-{server_id}"));
        let condition_marker = directory.path().join(format!("condition-{server_id}"));
        let branch_marker = directory.path().join(format!("branch-{server_id}"));
        let shared = Arc::new(Shared::new(server_id));
        let (reached_sender, stopping_reached) = crossbeam_channel::bounded(1);
        let (release_stopping, release_receiver) = crossbeam_channel::bounded(1);
        *shared.begin_stopping_hook.lock() = Some(ResponseAdmissionHook {
            reached: reached_sender,
            release: release_receiver,
        });
        let worker_shared = Arc::clone(&shared);
        let condition = format!(
            "while [ ! -e {} ]; do sleep 0.01; done; printf condition > {}; true",
            shell_quote(&release),
            shell_quote(&condition_marker)
        );
        let branch = format!("run-shell 'printf branch > {}'", branch_marker.display());
        let (result_sender, if_shell) = mpsc::channel();
        thread::spawn(move || {
            let _ = result_sender.send(worker_shared.execute(
                client,
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("if-shell", [condition, branch]),
            ));
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while shared.active_shutdown_blockers() != 1 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(shared.active_shutdown_blockers(), 1);
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("kill-server", [] as [&str; 0]),
            )
            .expect("shutdown during if-shell");
        while !shared.shutdown_blockers.lock().closed && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(shared.shutdown_blockers.lock().closed);
        fs::write(&release, b"").expect("release the condition");
        stopping_reached
            .recv_timeout(Duration::from_secs(5))
            .expect("shutdown reaches stopping");
        let result = if_shell
            .recv_timeout(Duration::from_secs(5))
            .expect("if-shell finishes before stopping");
        assert!(!shared.stopping.load(Ordering::Acquire));
        release_stopping.send(()).expect("release stopping");
        result.expect("foreground if-shell result");
        assert_eq!(
            fs::read_to_string(condition_marker).expect("condition marker"),
            "condition"
        );
        assert!(!branch_marker.exists());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !shared.stopping.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(shared.stopping.load(Ordering::Acquire));
    }
}

#[cfg(unix)]
#[test]
fn shutdown_skips_the_rest_of_a_queue_parked_on_a_preexisting_job() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let release = directory.path().join("release");
    let first = directory.path().join("first");
    let after = directory.path().join("after");
    let shared = Arc::new(Shared::new(1));
    let (reached_sender, stopping_reached) = crossbeam_channel::bounded(1);
    let (release_stopping, release_receiver) = crossbeam_channel::bounded(1);
    *shared.begin_stopping_hook.lock() = Some(ResponseAdmissionHook {
        reached: reached_sender,
        release: release_receiver,
    });
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[59]".to_owned(),
                    format!(
                        "parked-queue=run-shell 'while [ ! -e {} ]; do sleep 0.01; done; printf first > {}' ; run-shell 'printf after > {}'",
                        release.display(),
                        first.display(),
                        after.display(),
                    ),
                ],
            ),
        )
        .expect("parked queue alias");
    let worker_shared = Arc::clone(&shared);
    let (result_sender, queue) = mpsc::channel();
    thread::spawn(move || {
        let _ = result_sender.send(worker_shared.execute(
            ClientId(203),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("parked-queue", [] as [&str; 0]),
        ));
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.active_shutdown_blockers() != 1 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(shared.active_shutdown_blockers(), 1);
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("kill-server", [] as [&str; 0]),
        )
        .expect("shutdown during the parked queue");
    while !shared.shutdown_blockers.lock().closed && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(shared.shutdown_blockers.lock().closed);
    fs::write(&release, b"").expect("release the first job");
    stopping_reached
        .recv_timeout(Duration::from_secs(5))
        .expect("shutdown reaches stopping");
    let result = queue
        .recv_timeout(Duration::from_secs(5))
        .expect("queue finishes before stopping");
    assert!(!shared.stopping.load(Ordering::Acquire));
    release_stopping.send(()).expect("release stopping");
    result.expect("parked queue result");
    assert_eq!(fs::read_to_string(first).expect("first marker"), "first");
    assert!(!after.exists());
    let deadline = Instant::now() + Duration::from_secs(2);
    while !shared.stopping.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[cfg(unix)]
#[test]
fn command_prompt_submitted_after_shutdown_closes_runs_nothing() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let marker = directory.path().join("prompt");
    let (shared, client, mailbox, mut context) = popup_test_workspace("prompt-shutdown");
    let blocker = ShutdownBlocker::acquire(&shared, false).expect("pre-existing job");
    shared.request_shutdown();
    assert!(shared.shutdown_blockers.lock().closed);
    assert!(!shared.stopping.load(Ordering::Acquire));
    take_reliable_messages(&mailbox);

    shared.submit_command_prompt(
        client,
        ClientKind::Interactive,
        &mut context,
        &CommandPromptSubmission {
            inputs: Vec::new(),
            template: Some(CommandPromptTemplate::String(format!(
                "run-shell 'printf prompt > {}'",
                marker.display()
            ))),
            source: None,
        },
    );

    assert!(!take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientMessage {
                    kind: ClientMessageKind::Error,
                    ..
                },
                ..
            })
        )
    }));
    assert!(!marker.exists());
    drop(blocker);
    lifecycle::wait_for_cleanup(&shared);
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[cfg(unix)]
#[test]
fn direct_shutdown_answers_command_clients_connected_before_it() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let release = directory.path().join("release");
    let shared = Arc::new(Shared::new(1));
    let (mut client, server) = std::os::unix::net::UnixStream::pair().expect("pair");
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout");
    let server_shared = Arc::clone(&shared);
    let connection = thread::spawn(move || handle_connection(server, &server_shared));
    zz_protocol::write_protocol_message(
        &mut client,
        &ProtocolMessage::ClientHello(ClientHello {
            protocol_version: PROTOCOL_VERSION,
            client_instance_id: ClientInstanceId(1),
            kind: ClientKind::Command,
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
    .expect("hello");
    assert!(matches!(
        zz_protocol::read_protocol_message(&mut client).expect("server hello"),
        ProtocolMessage::ServerHello(_)
    ));
    let mut load = CommandInvocation::new("load-buffer", ["-b", "late", "-"]);
    load.set_stdin_available(true);
    zz_protocol::write_protocol_message(
        &mut client,
        &ProtocolMessage::CommandRequest(CommandRequest {
            request_id: 1,
            command: load,
            prepared: false,
        }),
    )
    .expect("load-buffer request");
    let ProtocolMessage::ClientFileRequest(file) =
        zz_protocol::read_protocol_message(&mut client).expect("stdin request")
    else {
        panic!("load-buffer did not ask for stdin");
    };

    let worker_shared = Arc::clone(&shared);
    let job = format!(
        "while [ ! -e {} ]; do sleep 0.01; done",
        shell_quote(&release)
    );
    let shell = thread::spawn(move || {
        worker_shared.execute(
            ClientId(201),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("run-shell", [job]),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.active_shutdown_blockers() != 1 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(shared.active_shutdown_blockers(), 1);
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("kill-server", [] as [&str; 0]),
        )
        .expect("direct shutdown");
    assert!(shared.shutdown_pending.load(Ordering::Acquire));
    assert!(!shared.stopping.load(Ordering::Acquire));

    zz_protocol::write_protocol_message(
        &mut client,
        &ProtocolMessage::ClientFileResponse(ClientFileResponse {
            request_id: file.request_id,
            data: b"payload".to_vec(),
            error: None,
        }),
    )
    .expect("stdin reply");
    assert!(matches!(
        zz_protocol::read_protocol_message(&mut client).expect("admitted response"),
        ProtocolMessage::CommandResponse(CommandResponse::Success {
            request_id: 1,
            exit_code: 0,
            ..
        })
    ));
    zz_protocol::write_protocol_message(
        &mut client,
        &ProtocolMessage::PrepareCommandList {
            request_id: 2,
            commands: vec![CommandInvocation::new("list-sessions", [] as [&str; 0])],
        },
    )
    .expect("late prepare");
    assert!(matches!(
        zz_protocol::read_protocol_message(&mut client).expect("late prepare response"),
        ProtocolMessage::PreparedCommandList { request_id: 2, .. }
    ));
    zz_protocol::write_protocol_message(
        &mut client,
        &ProtocolMessage::CommandRequest(CommandRequest {
            request_id: 3,
            command: CommandInvocation::new("list-sessions", [] as [&str; 0]),
            prepared: true,
        }),
    )
    .expect("late request");
    assert_eq!(
        zz_protocol::read_protocol_message(&mut client).expect("late request response"),
        server_stopping_response(3)
    );
    assert!(!shared.stopping.load(Ordering::Acquire));

    fs::write(&release, b"").expect("release foreground job");
    shell
        .join()
        .expect("foreground shell worker")
        .expect("foreground shell result");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !shared.stopping.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert!(shared.stopping.load(Ordering::Acquire));
    drop(client);
    connection
        .join()
        .expect("connection thread")
        .expect("connection result");
}

#[cfg(unix)]
#[test]
fn direct_shutdown_runs_session_close_hooks() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let marker = directory.path().join("session-close-hook");
    let tail = directory.path().join("session-close-tail");
    let unlinked = directory.path().join("window-unlinked-hook");
    let shared = Arc::new(Shared::new(1));
    switch_test_session(&shared, "hooked");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g".to_owned(),
                    "session-closed".to_owned(),
                    format!(
                        "run-shell 'printf hook > {}' ; run-shell 'printf tail > {}'",
                        marker.display(),
                        tail.display()
                    ),
                ],
            ),
        )
        .expect("session close hook");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g".to_owned(),
                    "window-unlinked".to_owned(),
                    format!("run-shell 'printf unlinked > {}'", unlinked.display()),
                ],
            ),
        )
        .expect("window unlink hook");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("kill-server", [] as [&str; 0]),
        )
        .expect("direct shutdown");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !shared.stopping.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(shared.stopping.load(Ordering::Acquire));
    assert_eq!(
        fs::read_to_string(marker).expect("session close hook marker"),
        "hook"
    );
    assert!(!tail.exists());
    assert!(!unlinked.exists());
}

#[test]
fn forced_shutdown_groups_close_events_in_session_name_order() {
    let shared = Arc::new(Shared::new(1));
    let (z, z_window, _) = switch_test_session(&shared, "z");
    let (a, a_window, _) = switch_test_session(&shared, "a");

    let events = shared.stop_shutdown_resources(false, true);
    let observed = events
        .iter()
        .map(|event| {
            let session = &event.variables[HOOK_SESSION_NAME_CONTEXT_FORMAT];
            event.variables.get(HOOK_WINDOW_CONTEXT_FORMAT).map_or_else(
                || format!("{}:{session}", event.name),
                |window| format!("{}:{session}:{window}", event.name),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        observed,
        [
            "session-closed:a".to_owned(),
            format!("window-unlinked:a:{a_window}"),
            "session-closed:z".to_owned(),
            format!("window-unlinked:z:{z_window}"),
        ]
    );
    assert!(!shared.inner.lock().engine.state.sessions.contains_key(&a));
    assert!(!shared.inner.lock().engine.state.sessions.contains_key(&z));
}

#[cfg(unix)]
#[test]
fn existing_blocker_keeps_all_forced_shutdown_hook_items() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let marker = directory.path().join("hook-order");
    let shared = Arc::new(Shared::new(1));
    switch_test_session(&shared, "hooked");
    for (hook, value) in [
        (
            "session-closed",
            format!(
                "run-shell 'printf S >> {}' ; run-shell 'printf s >> {}'",
                marker.display(),
                marker.display()
            ),
        ),
        (
            "window-unlinked",
            format!(
                "run-shell 'printf U >> {}' ; run-shell 'printf u >> {}'",
                marker.display(),
                marker.display()
            ),
        ),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("set-hook", ["-g", hook, value.as_str()]),
            )
            .expect("shutdown hook");
    }
    let blocker = ShutdownBlocker::acquire(&shared, false).expect("blocker admitted");

    shared.request_shutdown();

    assert_eq!(fs::read_to_string(marker).expect("hook order"), "SsUu");
    assert!(!shared.stopping.load(Ordering::Acquire));
    drop(blocker);
    #[cfg(unix)]
    lifecycle::wait_for_cleanup(&shared);
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[cfg(unix)]
#[test]
fn forced_shutdown_drops_delayed_event_hooks() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let delayed = directory.path().join("delayed");
    let unlinked = directory.path().join("unlinked");
    let shared = Arc::new(Shared::new(1));
    switch_test_session(&shared, "hooked");
    for (hook, value) in [
        (
            "session-closed",
            format!("run-shell -d 0.2 'printf delayed > {}'", delayed.display()),
        ),
        (
            "window-unlinked",
            format!("run-shell 'printf unlinked > {}'", unlinked.display()),
        ),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("set-hook", ["-g", hook, value.as_str()]),
            )
            .expect("shutdown hook");
    }

    shared.request_shutdown();

    assert!(shared.stopping.load(Ordering::Acquire));
    thread::sleep(Duration::from_millis(300));
    assert!(!delayed.exists());
    assert!(!unlinked.exists());
}

#[test]
fn quiet_shutdown_cause_survives_announcement_and_blocker() {
    let shared = Arc::new(Shared::new(1));
    let (session, _, _) = switch_test_session(&shared, "keeper");
    let blocker = ShutdownBlocker::acquire(&shared, false).expect("blocker admitted");

    shared.request_shutdown_without_hooks();
    shared.announce_shutdown();

    assert!(!shared.shutdown_forced.load(Ordering::Acquire));
    assert!(!shared.stopping.load(Ordering::Acquire));
    assert!(ShutdownBlocker::acquire(&shared, false).is_none());
    drop(blocker);
    assert!(shared.stopping.load(Ordering::Acquire));
    assert!(
        shared
            .inner
            .lock()
            .engine
            .state
            .sessions
            .contains_key(&session)
    );
}

#[test]
fn closed_shutdown_gate_rejects_late_job_admission() {
    let shared = Arc::new(Shared::new(1));

    assert!(shared.close_shutdown_blockers());
    assert!(!shared.stopping.load(Ordering::Acquire));
    assert!(ShutdownBlocker::acquire(&shared, true).is_none());
    assert!(ShellJobPermit::acquire(&shared, false, true, None).is_none());

    let shared = Arc::new(Shared::new(2));
    let blocker = ShutdownBlocker::acquire(&shared, false).expect("blocker admitted");
    shared.request_shutdown_after_blockers();

    assert!(!shared.stopping.load(Ordering::Acquire));
    assert!(ShutdownBlocker::acquire(&shared, false).is_none());
    let inherited = blocker.fork(true);
    drop(blocker);
    assert!(!shared.stopping.load(Ordering::Acquire));
    drop(inherited);
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[test]
fn foreground_job_admission_blocks_shutdown_before_state_lock() {
    let shared = Arc::new(Shared::new(1));
    let inner = shared.inner.lock();
    let permit_shared = Arc::clone(&shared);
    let (permit_sender, permit_receiver) = mpsc::sync_channel(1);
    let permit_worker = thread::spawn(move || {
        permit_sender
            .send(ShellJobPermit::acquire(&permit_shared, false, true, None))
            .expect("send foreground permit");
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.active_shutdown_blockers() != 1 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(shared.active_shutdown_blockers(), 1);

    let shutdown_shared = Arc::clone(&shared);
    let shutdown = thread::spawn(move || shutdown_shared.request_shutdown());
    let deadline = Instant::now() + Duration::from_secs(2);
    while !shared.shutdown_pending.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(shared.shutdown_pending.load(Ordering::Acquire));
    assert!(!shared.stopping.load(Ordering::Acquire));
    drop(inner);

    let permit = permit_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("receive foreground permit")
        .expect("foreground permit admitted");
    permit_worker.join().expect("foreground permit worker");
    shutdown.join().expect("deferred shutdown worker");
    assert!(!shared.stopping.load(Ordering::Acquire));
    drop(permit);
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[cfg(unix)]
#[test]
fn detached_command_mode_job_survives_alias_shutdown() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let marker = directory.path().join("detached");
    let hook_marker = directory.path().join("hook");
    let shared = Arc::new(Shared::new(1));
    switch_test_session(&shared, "hooked");
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g".to_owned(),
                    "session-closed".to_owned(),
                    format!("run-shell 'printf hook > {}'", hook_marker.display()),
                ],
            ),
        )
        .expect("session-closed hook");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[55]".to_owned(),
                    format!(
                        "detached-shutdown=kill-server ; run-shell -bC {{ run-shell 'sleep 0.2; printf detached > {}' }}",
                        marker.display(),
                    ),
                ],
            ),
        )
        .expect("detached shutdown alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new(
            "detached-shutdown",
            [] as [&str; 0],
        )])
        .remove(0)
        .invocation;

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            command,
            ClientKind::Command,
            &mut context,
            55,
            &prepared,
            true,
        ),
        CommandResponse::Success { exit_code: 0, .. }
    ));
    assert!(!shared.stopping.load(Ordering::Acquire));
    let deadline = Instant::now() + Duration::from_secs(2);
    assert_eq!(wait_for_file_text(&marker, deadline), "detached");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !shared.stopping.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(shared.stopping.load(Ordering::Acquire));
    assert!(!hook_marker.exists());
}

#[test]
fn shutdown_wins_over_a_later_alias_child_error() {
    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[54]",
                    "shutdown-error=kill-server ; has-session -t missing ; set-option -g @after-shutdown-error yes",
                ],
            ),
        )
        .expect("shutdown error alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new(
            "shutdown-error",
            [] as [&str; 0],
        )])
        .remove(0)
        .invocation;

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            command,
            ClientKind::Command,
            &mut context,
            54,
            &prepared,
            true,
        ),
        CommandResponse::Success {
            exit_code: 0,
            stderr,
            ..
        } if stderr.contains("can't find session: missing")
    ));
    assert!(read_global_option(&shared, "@after-shutdown-error").is_empty());
    assert!(shared.stopping.load(Ordering::Acquire));
}
