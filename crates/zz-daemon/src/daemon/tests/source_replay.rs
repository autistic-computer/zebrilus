use super::*;

#[test]
fn replayed_command_stdout_is_ordered_and_routed_once_per_invoking_client() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let child = directory.path().join("child.conf");
    fs::write(
        &child,
        "display-message -p CHILD_ONE\nlist-sessions -F CHILD_TWO\n",
    )
    .expect("child source");
    let root = directory.path().join("root.conf");
    fs::write(
        &root,
        format!(
            "display-message -p ROOT_ONE\nsource-file -v {}\ndisplay-message -p ROOT_TWO\n",
            tmux_path(&child)
        ),
    )
    .expect("root source");

    let shared = Arc::new(Shared::new(63));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "replay-output"]),
        )
        .expect("source output session");
    let session = context.session.expect("source output session id");
    let pane = context.pane.expect("source output pane");
    let child_verbose = format!(
        "{}:1: display-message -p CHILD_ONE\n\
             {}:2: list-sessions -F CHILD_TWO",
        child.display(),
        child.display(),
    );
    let root_verbose = format!(
        "{}:1: display-message -p ROOT_ONE\n\
             {}:2: source-file -v {}\n\
             {}:3: display-message -p ROOT_TWO",
        root.display(),
        root.display(),
        tmux_path(&child),
        root.display(),
    );
    let child_replay = "CHILD_ONE\nCHILD_TWO";
    let root_replay = format!("ROOT_ONE\n{child_verbose}\n{child_replay}\nROOT_TWO");
    let expected = format!("{root_verbose}\n{root_replay}\n");
    let aggregate_expected =
        format!("{child_verbose}\n{root_verbose}\n{child_replay}\n{root_replay}\n");
    let invocation =
        CommandInvocation::new("source-file", ["-v".to_owned(), root.display().to_string()]);

    assert_eq!(
        shared.execute_command_request(command, ClientKind::Command, &mut context, 1, &invocation,),
        CommandResponse::Success {
            request_id: 1,
            output: expected.clone().into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            2,
            &CommandInvocation::new(
                "source-file",
                [
                    "-v".to_owned(),
                    child.display().to_string(),
                    root.display().to_string(),
                ],
            ),
        ),
        CommandResponse::Success {
            request_id: 2,
            output: aggregate_expected.clone().into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            3,
            &CommandInvocation::new(
                "source-file",
                [
                    "-v".to_owned(),
                    directory.path().join("*.conf").display().to_string()
                ],
            ),
        ),
        CommandResponse::Success {
            request_id: 3,
            output: aggregate_expected.into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );

    let mut startup_report = ConfigLoadReport::default();
    shared
        .load_config_file_with_report(&root, &mut context, 0, &mut startup_report)
        .expect("clientless source replay");
    assert_eq!(startup_report.stdout_transcript(), None);

    let interactive_mailbox = OutboundMailbox::new();
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&interactive_mailbox),
    );
    shared
        .attach(interactive, session)
        .expect("attach interactive source client");
    take_reliable_messages(&interactive_mailbox);
    let mut interactive_context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
            .expect("interactive source context");
    assert_eq!(
        shared.execute_command_request(
            interactive,
            ClientKind::Interactive,
            &mut interactive_context,
            4,
            &invocation,
        ),
        CommandResponse::Success {
            request_id: 4,
            output: expected.clone().into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    let ProtocolMessage::Event(Event {
        payload:
            EventPayload::CommandOutput {
                viewport: Some(viewport),
                ..
            },
        ..
    }) = take_command_output_message(&interactive_mailbox)
    else {
        panic!("expected replay command-output viewport");
    };
    let rendered = viewport_text(&viewport);
    for marker in ["ROOT_ONE", "CHILD_ONE", "CHILD_TWO", "ROOT_TWO"] {
        assert_eq!(rendered.matches(marker).count(), 2, "{marker}");
    }
    let mut offset = 0;
    for marker in ["ROOT_ONE", "CHILD_ONE", "CHILD_TWO", "ROOT_TWO"] {
        let index = rendered[offset..]
            .find(marker)
            .unwrap_or_else(|| panic!("missing replay output marker {marker}"));
        offset += index + marker.len();
    }
    assert_eq!(
        shared
            .inner
            .lock()
            .clients
            .values()
            .filter(|c| c.command_output.is_some())
            .count(),
        1
    );
    assert!(
        take_reliable_messages(&interactive_mailbox)
            .iter()
            .all(|message| !matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ClientMessage {
                        kind: ClientMessageKind::Info | ClientMessageKind::Warning,
                        ..
                    },
                    ..
                })
            ))
    );
    let replay_id = current_command_output_id(&shared, interactive);
    shared
        .input(
            interactive,
            ClientKind::Interactive,
            &mut interactive_context,
            InputMessage::Text {
                pane,
                text: "q".to_owned(),
            },
        )
        .expect("dismiss replay command output");
    wait_for_command_output_close(&interactive_mailbox, replay_id);
    assert!(
        shared
            .inner
            .lock()
            .clients
            .values()
            .all(|c| c.command_output.is_none())
    );

    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    shared
        .attach(control, session)
        .expect("attach control source client");
    take_reliable_messages(&control_mailbox);
    let mut control_context = interactive_context;
    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut control_context,
            5,
            &invocation,
        ),
        CommandResponse::Success {
            request_id: 5,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_guards(take_reliable_messages(&control_mailbox)),
        [
            ("ROOT_ONE".to_owned(), false, false),
            (String::new(), false, false),
            ("CHILD_ONE".to_owned(), false, false),
            ("CHILD_TWO".to_owned(), false, false),
            ("ROOT_TWO".to_owned(), false, false),
        ]
    );
}

#[test]
fn replayed_indirect_source_stdout_is_routed_once() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let child = directory.path().join("child.conf");
    fs::write(
        &child,
        "display-message -p CHILD_ONE\nlist-sessions -F CHILD_TWO\n",
    )
    .expect("child source");
    let alias_root = directory.path().join("alias-root.conf");
    fs::write(
        &alias_root,
        "display-message -p ALIAS_BEFORE\nindirect\ndisplay-message -p ALIAS_AFTER\n",
    )
    .expect("alias source");
    let conditional_root = directory.path().join("conditional-root.conf");
    fs::write(
        &conditional_root,
        format!(
            "display-message -p CONDITIONAL_BEFORE\n\
                 if-shell -F 1 'source-file -v {}'\n\
                 display-message -p CONDITIONAL_AFTER\n",
            tmux_path(&child)
        ),
    )
    .expect("conditional source");
    let hook_child = directory.path().join("hook-child.conf");
    fs::write(&hook_child, "list-sessions -F HOOK_CHILD\n").expect("hook child source");
    let hook_root = directory.path().join("hook-root.conf");
    fs::write(
        &hook_root,
        "display-message -p HOOK_TRIGGER\nlist-sessions -F HOOK_LATER\n",
    )
    .expect("hook source");
    let error_child = directory.path().join("error-child.conf");
    fs::write(
        &error_child,
        "display-message -p BEFORE_CHILD\n\
             kill-session -t missing-indirect\n\
             display-message -p AFTER_CHILD\n",
    )
    .expect("indirect error child source");
    let error_root = directory.path().join("error-root.conf");
    fs::write(
        &error_root,
        format!(
            "display-message -p ROOT_BEFORE\n\
                 if-shell -F 1 'source-file {}'\n\
                 display-message -p ROOT_AFTER\n",
            tmux_path(&error_child)
        ),
    )
    .expect("indirect error source");
    let ordered_error_child = directory.path().join("ordered-error-child.conf");
    fs::write(&ordered_error_child, "kill-session -t missing-B\n")
        .expect("ordered error child source");
    let ordered_error_root = directory.path().join("ordered-error-root.conf");
    fs::write(
        &ordered_error_root,
        format!(
            "kill-session -t missing-A\n\
                 if-shell -F 1 'source-file {}'\n\
                 kill-session -t missing-C\n",
            tmux_path(&ordered_error_child)
        ),
    )
    .expect("ordered error source");
    let terminal_hook_child = directory.path().join("terminal-hook-child.conf");
    fs::write(&terminal_hook_child, "new-session -d -s hook-command\n")
        .expect("terminal hook child source");
    let terminal_hook_root = directory.path().join("terminal-hook-root.conf");
    fs::write(
        &terminal_hook_root,
        "display-message -p TERMINAL_HOOK_TRIGGER\nset-option -g @terminal-hook-after yes\n",
    )
    .expect("terminal hook source");

    let shared = Arc::new(Shared::new(63));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "indirect-output"]),
        )
        .expect("indirect output session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[90]".to_owned(),
                    format!("indirect=source-file -v {}", tmux_path(&child)),
                ],
            ),
        )
        .expect("indirect source alias");
    let child_output = format!(
        "{}:1: display-message -p CHILD_ONE\n\
             {}:2: list-sessions -F CHILD_TWO\n\
             CHILD_ONE\nCHILD_TWO",
        child.display(),
        child.display(),
    );

    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            1,
            &CommandInvocation::new("source-file", [alias_root.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: format!("ALIAS_BEFORE\n{child_output}\nALIAS_AFTER\n").into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            2,
            &CommandInvocation::new("source-file", [conditional_root.display().to_string()],),
        ),
        CommandResponse::Success {
            request_id: 2,
            output: format!("CONDITIONAL_BEFORE\n{child_output}\nCONDITIONAL_AFTER\n").into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g".to_owned(),
                    "after-display-message".to_owned(),
                    format!("source-file {}", tmux_path(&hook_child)),
                ],
            ),
        )
        .expect("indirect source hook");
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            3,
            &CommandInvocation::new("source-file", [hook_root.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 3,
            output: "HOOK_TRIGGER\nHOOK_CHILD\nHOOK_LATER\n".into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-hook", ["-gu", "after-display-message"]),
        )
        .expect("clear indirect source hook");
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            4,
            &CommandInvocation::new("source-file", [error_root.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 4,
            output: "ROOT_BEFORE\nBEFORE_CHILD\nAFTER_CHILD\nROOT_AFTER\n".into(),
            exit_code: 1,
            stderr: "can't find session: missing-indirect\n".to_owned(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            5,
            &CommandInvocation::new("source-file", [ordered_error_root.display().to_string()],),
        ),
        CommandResponse::Success {
            request_id: 5,
            output: RawText::default(),
            exit_code: 1,
            stderr: "can't find session: missing-A\n\
                         can't find session: missing-B\n\
                         can't find session: missing-C\n"
                .to_owned(),
            stdout_claim: StdoutClaim::None,
        }
    );
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g".to_owned(),
                    "after-display-message".to_owned(),
                    format!("source-file {}", tmux_path(&terminal_hook_child)),
                ],
            ),
        )
        .expect("command source hook");
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            6,
            &CommandInvocation::new("source-file", [terminal_hook_root.display().to_string()],),
        ),
        CommandResponse::Success {
            request_id: 6,
            output: "TERMINAL_HOOK_TRIGGER\n".into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("has-session", ["-t", "hook-command"]),
        )
        .expect("command hook-created session");
}

#[test]
fn callback_group_action_uses_direct_source_depth_and_fail_priority() {
    let failure = |group_action, owner, depth| CallbackParseFailure {
        message: "syntax error".to_owned(),
        group_action,
        owner,
        control_event: None,
        depth,
    };

    for failures in [
        vec![
            failure(
                CallbackGroupAction::Continue,
                CallbackFailureOwner::Source,
                1,
            ),
            failure(CallbackGroupAction::Fail, CallbackFailureOwner::Source, 1),
        ],
        vec![
            failure(CallbackGroupAction::Fail, CallbackFailureOwner::Source, 1),
            failure(
                CallbackGroupAction::Continue,
                CallbackFailureOwner::Source,
                1,
            ),
        ],
    ] {
        assert!(
            select_callback_group_action(&failures, "run-shell", 2)
                == Some(CallbackGroupAction::Fail)
        );
    }

    assert!(
        select_callback_group_action(
            &[
                failure(CallbackGroupAction::Fail, CallbackFailureOwner::Source, 2,),
                failure(CallbackGroupAction::Fail, CallbackFailureOwner::Hook, 1,),
                failure(
                    CallbackGroupAction::Handled,
                    CallbackFailureOwner::Source,
                    1,
                ),
                failure(
                    CallbackGroupAction::Continue,
                    CallbackFailureOwner::Source,
                    1,
                ),
            ],
            "if-shell",
            2,
        ) == Some(CallbackGroupAction::Continue)
    );
    assert!(select_callback_group_action(&[], "if-shell", 2).is_none());
    assert!(select_callback_group_action(&[], "if-shell", 1) == Some(CallbackGroupAction::Fail));
    assert!(
        select_callback_group_action(&[], "run-shell", 1) == Some(CallbackGroupAction::Continue)
    );
}

#[test]
fn callback_parse_capture_qualifies_only_marked_events() {
    let hook_event = Arc::new(());
    let first_event = Arc::new(());
    let second_event = Arc::new(());
    let mut captured = CapturedControlCommandEvents {
        events: vec![
            EventPayload::ControlCommandOutput {
                output: "syntax error".to_owned(),
            },
            EventPayload::ControlCommandOutput {
                output: "syntax error".to_owned(),
            },
            EventPayload::ControlCommandOutput {
                output: "syntax error".to_owned(),
            },
            EventPayload::ControlCommandOutput {
                output: "syntax error".to_owned(),
            },
        ],
        callback_parse_events: vec![
            (0, hook_event),
            (2, Arc::clone(&first_event)),
            (3, Arc::clone(&second_event)),
        ],
    };

    assert_eq!(
        captured.qualify_callback_parse_events(&[
            CallbackParseFailure {
                message: "source.conf:1: syntax error".to_owned(),
                group_action: CallbackGroupAction::Handled,
                owner: CallbackFailureOwner::Source,
                control_event: Some(first_event),
                depth: 1,
            },
            CallbackParseFailure {
                message: "source.conf:2: syntax error".to_owned(),
                group_action: CallbackGroupAction::Handled,
                owner: CallbackFailureOwner::Source,
                control_event: Some(second_event),
                depth: 1,
            },
        ]),
        2
    );
    assert!(captured.callback_parse_events.is_empty());
    assert!(matches!(
        captured.events.as_slice(),
        [
            EventPayload::ControlCommandOutput { output: first },
            EventPayload::ControlCommandOutput { output: second },
            EventPayload::ControlCommandOutput { output: third },
            EventPayload::ControlCommandOutput { output: fourth },
        ] if first == "syntax error"
            && second == "syntax error"
            && third == "source.conf:1: syntax error"
            && fourth == "source.conf:2: syntax error"
    ));
}

#[test]
fn control_command_event_capture_does_not_intercept_other_items() {
    let shared = Arc::new(Shared::new(79));
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);

    let shared = shared.command_item(None);
    let capture = shared.begin_control_command_event_capture(control);
    shared.publish_control_command_guard(
        Some((control, CONTROL_COMMAND_FRAME_FLAGS_CONTROL)),
        "LOCAL_BEFORE".into(),
        false,
        false,
    );
    let worker = {
        let shared = shared.server_owner();
        thread::spawn(move || {
            shared.publish_to_client(
                control,
                EventPayload::ClientMessage {
                    pane: None,
                    kind: ClientMessageKind::Error,
                    text: "BACKGROUND_ERROR".to_owned(),
                },
            );
        })
    };
    worker.join().expect("background publisher");
    assert!(matches!(
        take_reliable_messages(&mailbox).as_slice(),
        [ProtocolMessage::Event(Event {
            payload: EventPayload::ClientMessage {
                kind: ClientMessageKind::Error,
                text,
                ..
            },
            ..
        })] if text == "BACKGROUND_ERROR"
    ));

    shared.publish_control_command_guard(
        Some((control, CONTROL_COMMAND_FRAME_FLAGS_CONTROL)),
        "LOCAL_AFTER".into(),
        false,
        false,
    );
    shared.publish_captured_control_command_events(control, capture.finish());
    assert_eq!(
        control_command_guards(take_reliable_messages(&mailbox)),
        [
            ("LOCAL_BEFORE".to_owned(), false, false),
            ("LOCAL_AFTER".to_owned(), false, false),
        ]
    );
    assert!(
        shared
            .inner
            .lock()
            .waits
            .control_command_event_captures
            .is_empty()
    );
}

#[test]
fn control_command_hook_mixed_source_retains_status_without_error_guard() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let missing = directory.path().join("missing.conf");
    let hit = directory.path().join("hit.conf");
    fs::write(&hit, "display-message -p MIXED_HIT\n").expect("mixed hit source");

    let shared = Arc::new(Shared::new(80));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "control-hook-mixed"]),
        )
        .expect("mixed hook session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g".to_owned(),
                    "after-display-message".to_owned(),
                    format!("source-file '{}' '{}'", missing.display(), hit.display()),
                ],
            ),
        )
        .expect("mixed source hook");

    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(control, context.session.expect("mixed hook session id"))
        .expect("attach mixed hook control client");
    take_reliable_messages(&mailbox);

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            1,
            &CommandInvocation::new("display-message", ["-p", "MIXED_TRIGGER"]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: "MIXED_TRIGGER".into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        control_command_frames(take_reliable_messages(&mailbox)),
        [
            (missing_source_error(&missing), false, true, 0),
            ("MIXED_HIT".to_owned(), false, false, 0),
        ]
    );
}

#[cfg(unix)]
#[test]
fn control_if_shell_replay_preserves_indirect_source_guard_order() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let success_child = directory.path().join("success-child.conf");
    fs::write(&success_child, "display-message -p CHILD\n").expect("success child source");
    let leaf = directory.path().join("leaf.conf");
    fs::write(&leaf, "display-message -p LEAF\n").expect("leaf source");
    let middle = directory.path().join("middle.conf");
    fs::write(
        &middle,
        format!("if-shell -F 1 'source-file {}'\n", tmux_path(&leaf)),
    )
    .expect("middle source");
    let success_root = directory.path().join("success-root.conf");
    fs::write(
        &success_root,
        format!(
            "if-shell -F 1 'display-message -p DIRECT_INSERT'\n\
                 if-shell -F 1 'new-pane ; display-message -p AFTER_UNSUPPORTED'\n\
                 run-shell -C 'display-message -p RUN_DIRECT'\n\
                 run-shell -C 'source-file {}'\n\
                 run-sh -C 'source-f {}'\n\
                 if-sh -F 1 'source-f {}'\n\
                 if-shell -bF 1 'source-file {}'\n\
                 if-shell -F 1 'source-file {}'\n\
                 if-shell '{succeed}' 'source-file {}'\n\
                 if-shell -F 1 'source-file {}'\n\
                 display-message -p ROOT_LATER\n",
            tmux_path(&success_child),
            tmux_path(&success_child),
            tmux_path(&success_child),
            tmux_path(&success_child),
            tmux_path(&success_child),
            tmux_path(&success_child),
            tmux_path(&middle),
            succeed = test_shell::SUCCEED,
        ),
    )
    .expect("success root source");
    let direct_root = directory.path().join("direct-root.conf");
    fs::write(
        &direct_root,
        format!("source-file {}\n", tmux_path(&success_child)),
    )
    .expect("direct root source");
    let alias_root = directory.path().join("alias-root.conf");
    fs::write(&alias_root, "indirect-control-source\n").expect("alias root source");

    let missing = directory.path().join("missing.conf");
    let missing_root = directory.path().join("missing-root.conf");
    fs::write(
        &missing_root,
        format!(
            "if-shell -F 1 'source-file {}'\n\
                 display-message -p AFTER_MISSING\n",
            tmux_path(&missing),
        ),
    )
    .expect("missing root source");

    let error_child = directory.path().join("error-child.conf");
    fs::write(
        &error_child,
        "kill-session -t missing-runtime-child\n\
             display-message -p CHILD_AFTER_ERROR\n",
    )
    .expect("error child source");
    let error_root = directory.path().join("error-root.conf");
    fs::write(
        &error_root,
        format!(
            "run-shell -C 'kill-session -t missing-run-shell-direct'\n\
                 if-shell -F 1 'kill-session -t direct-insert-missing'\n\
                 if-shell -F 1 'source-file {}'\n\
                 display-message -p ROOT_AFTER_ERROR\n",
            tmux_path(&error_child),
        ),
    )
    .expect("error root source");

    let hook_child = directory.path().join("hook-child.conf");
    fs::write(&hook_child, "display-message -p HOOK_CHILD\n").expect("hook child source");
    let hook_root = directory.path().join("hook-root.conf");
    fs::write(&hook_root, "display-message -p HOOK_TRIGGER\n").expect("hook root source");
    let unknown = directory.path().join("unknown.conf");
    fs::write(&unknown, "wibble\n").expect("unknown source");
    let unknown_root = directory.path().join("unknown-root.conf");
    fs::write(
        &unknown_root,
        format!(
            "if-shell -F 1 'source-file {}'\n\
                 display-message -p AFTER_UNKNOWN\n",
            tmux_path(&unknown),
        ),
    )
    .expect("unknown root source");
    let unreadable = directory.path().join("unreadable.conf");
    fs::create_dir(&unreadable).expect("unreadable source");
    let unreadable_root = directory.path().join("unreadable-root.conf");
    fs::write(
        &unreadable_root,
        format!(
            "if-shell -F 1 'source-file {}'\n\
                 display-message -p AFTER_UNREADABLE\n",
            tmux_path(&unreadable),
        ),
    )
    .expect("unreadable root source");

    let shared = Arc::new(Shared::new(78));
    let command = ClientId(u64::from(u16::MAX));
    let mut command_context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("new-session", ["-d", "-s", "control-if-source"]),
        )
        .expect("control if-shell source session");
    let session = command_context.session.expect("source session");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(control, session)
        .expect("attach control if-shell source client");
    take_reliable_messages(&mailbox);
    let mut context = command_context;

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            1,
            &control_stdin_command("source-file", [success_root.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_guards(take_reliable_messages(&mailbox)),
        [
            (String::new(), false, false),
            ("DIRECT_INSERT".to_owned(), false, false),
            (String::new(), false, false),
            (String::new(), false, false),
            ("AFTER_UNSUPPORTED".to_owned(), false, false),
            (String::new(), false, false),
            ("RUN_DIRECT".to_owned(), false, false),
            (String::new(), false, false),
            (String::new(), false, false),
            ("CHILD".to_owned(), false, false),
            (String::new(), false, false),
            (String::new(), false, false),
            ("CHILD".to_owned(), false, false),
            (String::new(), false, false),
            (String::new(), false, false),
            ("CHILD".to_owned(), false, false),
            (String::new(), false, false),
            (String::new(), false, false),
            ("CHILD".to_owned(), false, false),
            (String::new(), false, false),
            (String::new(), false, false),
            ("CHILD".to_owned(), false, false),
            (String::new(), false, false),
            (String::new(), false, false),
            ("CHILD".to_owned(), false, false),
            (String::new(), false, false),
            (String::new(), false, false),
            (String::new(), false, false),
            (String::new(), false, false),
            ("LEAF".to_owned(), false, false),
            ("ROOT_LATER".to_owned(), false, false),
        ]
    );

    for (request_id, source, exit_code, expected) in [
        (
            30,
            &unknown_root,
            0,
            vec![
                "guard:false:false:".to_owned(),
                "guard:false:false:".to_owned(),
                format!("warning:{}:1: unknown command: wibble", unknown.display()),
                "guard:false:false:AFTER_UNKNOWN".to_owned(),
            ],
        ),
        (
            31,
            &unreadable_root,
            1,
            vec![
                "guard:false:false:".to_owned(),
                "guard:false:false:".to_owned(),
                format!("read-error:Is a directory: {}", unreadable.display()),
                "guard:false:false:AFTER_UNREADABLE".to_owned(),
            ],
        ),
    ] {
        assert_eq!(
            shared.execute_command_request(
                control,
                ClientKind::Control,
                &mut context,
                request_id,
                &control_stdin_command("source-file", [source.display().to_string()]),
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }
        );
        let events = take_reliable_messages(&mailbox)
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload:
                        EventPayload::ControlCommandGuard {
                            output,
                            error,
                            sticky_failure,
                            ..
                        },
                    ..
                }) => Some(format!("guard:{error}:{sticky_failure}:{output}")),
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlConfigError { text },
                    ..
                }) => Some(format!("warning:{text}")),
                ProtocolMessage::Event(Event {
                    payload:
                        EventPayload::ClientMessage {
                            kind: ClientMessageKind::Error,
                            text,
                            ..
                        },
                    ..
                }) => Some(format!("error:{text}")),
                ProtocolMessage::Event(Event {
                    payload:
                        EventPayload::ControlSourceFile {
                            event: ControlSourceFileEvent::ReadError(text),
                        },
                    ..
                }) => Some(format!("read-error:{text}")),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(events, expected);
    }

    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[91]".to_owned(),
                    format!(
                        "indirect-control-source=source-file {}",
                        tmux_path(&success_child)
                    ),
                ],
            ),
        )
        .expect("control source alias");
    for (request_id, source) in [(40, &direct_root), (41, &alias_root)] {
        assert_eq!(
            shared.execute_command_request(
                control,
                ClientKind::Control,
                &mut context,
                request_id,
                &control_stdin_command("source-file", [source.display().to_string()]),
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }
        );
        assert_eq!(
            control_command_guards(take_reliable_messages(&mailbox)),
            [
                (String::new(), false, false),
                ("CHILD".to_owned(), false, false),
            ]
        );
    }

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            2,
            &control_stdin_command("source-file", [missing_root.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 2,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_guards(take_reliable_messages(&mailbox)),
        [
            (String::new(), false, false),
            (
                format!("No such file or directory: {}", missing.display()),
                true,
                true,
            ),
            ("AFTER_MISSING".to_owned(), false, false),
        ]
    );

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            2,
            &control_stdin_command("source-file", [error_root.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 2,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_guards(take_reliable_messages(&mailbox)),
        [
            (String::new(), false, false),
            (
                "can't find session: missing-run-shell-direct".to_owned(),
                true,
                true,
            ),
            (String::new(), false, false),
            (
                "can't find session: direct-insert-missing".to_owned(),
                true,
                true,
            ),
            (String::new(), false, false),
            (String::new(), false, false),
            (
                "can't find session: missing-runtime-child".to_owned(),
                true,
                true,
            ),
            ("CHILD_AFTER_ERROR".to_owned(), false, false),
            ("ROOT_AFTER_ERROR".to_owned(), false, false),
        ]
    );

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            4,
            &control_stdin_command("display-message", ["-p", "OUTSIDE_REPLAY"]),
        ),
        CommandResponse::Success {
            request_id: 4,
            output: "OUTSIDE_REPLAY".into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert!(control_command_guards(take_reliable_messages(&mailbox)).is_empty());
    assert!(
        shared
            .inner
            .lock()
            .waits
            .control_command_event_captures
            .is_empty()
    );

    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g".to_owned(),
                    "after-display-message".to_owned(),
                    format!("source-file {}", tmux_path(&hook_child)),
                ],
            ),
        )
        .expect("control if-shell hook");
    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            5,
            &control_stdin_command("source-file", [hook_root.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 5,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_frames(take_reliable_messages(&mailbox)),
        [
            ("HOOK_TRIGGER".to_owned(), false, false, 1),
            (String::new(), false, false, 0),
            ("HOOK_CHILD".to_owned(), false, false, 0),
        ]
    );
}

#[test]
fn control_command_hooks_use_flags_zero_without_folding_or_leaking() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let leaf = directory.path().join("hook-leaf.conf");
    fs::write(&leaf, "display-message -p HOOK_LEAF\n").expect("hook leaf source");
    let unknown = directory.path().join("hook-unknown.conf");
    fs::write(&unknown, "wibble\n").expect("unknown hook source");
    let unreadable = directory.path().join("hook-read-error");
    fs::create_dir(&unreadable).expect("hook read error directory");
    let unreadable_error = fs::read_to_string(&unreadable).expect_err("directory read error");
    let unreadable_error = source_read_error_warning(&unreadable, &unreadable_error);
    let root = directory.path().join("hook-root.conf");
    fs::write(
        &root,
        format!(
            "display-message -p HOOK_SOURCE_BEFORE\n\
                 source-file {}\n\
                 source-file {}\n\
                 source-file {}\n\
                 kill-session -t hook-source-runtime\n\
                 display-message -p HOOK_SOURCE_AFTER\n",
            tmux_path(&leaf),
            tmux_path(&unknown),
            tmux_path(&unreadable),
        ),
    )
    .expect("hook root source");
    let missing = directory.path().join("hook-missing.conf");
    let trigger = directory.path().join("hook-trigger.conf");
    fs::write(&trigger, "display-message -p HOOK_TRIGGER\n").expect("hook trigger source");
    let command_error = directory.path().join("command-error.conf");
    fs::write(
        &command_error,
        "kill-session -t outer-command-error\ndisplay-message -p OUTER_AFTER_ERROR\n",
    )
    .expect("command error source");
    let final_source = directory.path().join("final.conf");
    fs::write(&final_source, "display-message -p FINAL_FLAGS_ONE\n").expect("final source");

    let shared = Arc::new(Shared::new(80));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "control-hook-frames", QUIET_PANE_COMMAND],
            ),
        )
        .expect("hook frame session");
    wait_for_quiet_panes(&shared, &[context.pane.expect("hook frame pane")]);
    let session = context.session.expect("hook frame session id");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(control, session)
        .expect("attach hook frame control client");
    take_reliable_messages(&mailbox);

    for (index, value) in [
        (
            0,
            "display-message -p HOOK_ARRAY_ZERO ; kill-session -t hook-direct-runtime ; display-message -p HOOK_ARRAY_ZERO_SKIPPED".to_owned(),
        ),
        (1, format!("source-file {}", tmux_path(&root))),
        (2, format!("source-file {}", tmux_path(&missing))),
        (3, "display-message -p HOOK_ARRAY_THREE".to_owned()),
    ] {
        shared
            .execute(
                command,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "set-hook",
                    [
                        "-g".to_owned(),
                        format!("after-display-message[{index}]"),
                        value,
                    ],
                ),
            )
            .expect("install command hook array");
    }
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "command-error",
                    "display-message -p COMMAND_ERROR_HOOK",
                ],
            ),
        )
        .expect("install command-error hook");

    let mut control_context = context.clone();
    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut control_context,
            1,
            &control_stdin_command("source-file", [trigger.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    let first_events = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ControlCommandGuard {
                        output,
                        error,
                        sticky_failure,
                        flags,
                    },
                ..
            }) => Some(format!("guard:{flags}:{error}:{sticky_failure}:{output}")),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlConfigError { text },
                ..
            }) => Some(format!("warning:{text}")),
            ProtocolMessage::Event(
                Event {
                    payload:
                        EventPayload::ClientMessage {
                            kind: ClientMessageKind::Error,
                            text,
                            ..
                        },
                    ..
                }
                | Event {
                    payload:
                        EventPayload::ControlSourceFile {
                            event: ControlSourceFileEvent::ReadError(text),
                        },
                    ..
                },
            ) => Some(format!("error:{text}")),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        first_events,
        [
            "guard:1:false:false:HOOK_TRIGGER".to_owned(),
            "guard:0:false:false:HOOK_ARRAY_ZERO".to_owned(),
            "guard:0:true:true:can't find session: hook-direct-runtime".to_owned(),
            "guard:0:false:false:".to_owned(),
            "guard:0:false:false:HOOK_SOURCE_BEFORE".to_owned(),
            "guard:0:false:false:".to_owned(),
            "guard:0:false:false:HOOK_LEAF".to_owned(),
            "guard:0:false:false:".to_owned(),
            format!("warning:{}:1: unknown command: wibble", unknown.display()),
            "guard:0:false:false:".to_owned(),
            format!("error:{unreadable_error}"),
            "guard:0:true:true:can't find session: hook-source-runtime".to_owned(),
            "guard:0:false:false:HOOK_SOURCE_AFTER".to_owned(),
            format!(
                "guard:0:true:true:No such file or directory: {}",
                missing.display()
            ),
            "guard:0:false:false:HOOK_ARRAY_THREE".to_owned(),
        ]
    );
    assert!(
        first_events
            .iter()
            .all(|event| !event.contains("HOOK_ARRAY_ZERO_SKIPPED"))
    );

    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-hook", ["-gu", "after-display-message"]),
        )
        .expect("clear display command hook");
    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut control_context,
            2,
            &control_stdin_command("source-file", [command_error.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 2,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_frames(take_reliable_messages(&mailbox)),
        [
            (
                "can't find session: outer-command-error".to_owned(),
                true,
                true,
                1,
            ),
            ("COMMAND_ERROR_HOOK".to_owned(), false, false, 0),
            ("OUTER_AFTER_ERROR".to_owned(), false, false, 1),
        ]
    );

    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-hook", ["-gu", "command-error"]),
        )
        .expect("clear command-error hook");
    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut control_context,
            3,
            &control_stdin_command("source-file", [final_source.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 3,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_frames(take_reliable_messages(&mailbox)),
        [("FINAL_FLAGS_ONE".to_owned(), false, false, 1)]
    );
    assert!(
        shared
            .inner
            .lock()
            .waits
            .control_command_event_captures
            .is_empty()
    );
}

#[test]
fn nested_parse_diagnostics_stay_at_the_source_position() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let invalid = directory.path().join("invalid.conf");
    fs::write(
        &invalid,
        "display-message -p CHILD_SHOULD_NOT_RUN\nset @bad \\400\n",
    )
    .expect("invalid child source");
    let direct = directory.path().join("direct.conf");
    fs::write(
        &direct,
        format!(
            "display-message -p ROOT_BEFORE\n\
                 source-file {}\n\
                 display-message -p ROOT_AFTER\n",
            tmux_path(&invalid)
        ),
    )
    .expect("direct diagnostic source");
    let conditional = directory.path().join("conditional.conf");
    fs::write(
        &conditional,
        format!(
            "display-message -p ROOT_BEFORE\n\
                 if-shell -F 1 'source-file {}'\n\
                 display-message -p ROOT_AFTER\n",
            tmux_path(&invalid)
        ),
    )
    .expect("conditional diagnostic source");
    let good = directory.path().join("good.conf");
    fs::write(&good, "display-message -p GOOD_OUTPUT\n").expect("good source");
    let later = directory.path().join("later.conf");
    fs::write(&later, "display-message -p LATER_OUTPUT\n").expect("later source");
    let nested_multi = directory.path().join("nested-multi.conf");
    fs::write(
        &nested_multi,
        format!(
            "display-message -p ROOT_BEFORE\n\
                 source-file -v {} {} {}\n\
                 display-message -p ROOT_AFTER\n",
            tmux_path(&good),
            tmux_path(&invalid),
            tmux_path(&later),
        ),
    )
    .expect("nested multi diagnostic source");
    let unknown = directory.path().join("unknown.conf");
    fs::write(&unknown, "wibble\n").expect("unknown command source");
    let nested_unknown = directory.path().join("nested-unknown.conf");
    fs::write(
        &nested_unknown,
        format!(
            "display-message -p ROOT_BEFORE\n\
                 source-file {}\n\
                 display-message -p ROOT_AFTER\n",
            tmux_path(&unknown)
        ),
    )
    .expect("nested unknown source");

    let shared = Arc::new(Shared::new(63));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "parse-output"]),
        )
        .expect("parse output session");
    let diagnostic = format!("{}:2: invalid octal escape", invalid.display());
    let direct_verbose = format!(
        "{}:1: display-message -p ROOT_BEFORE\n\
             {}:2: source-file {}\n\
             {}:3: display-message -p ROOT_AFTER",
        direct.display(),
        direct.display(),
        tmux_path(&invalid),
        direct.display(),
    );

    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            1,
            &CommandInvocation::new(
                "source-file",
                ["-v".to_owned(), direct.display().to_string()],
            ),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: format!("{direct_verbose}\nROOT_BEFORE\n{diagnostic}\nROOT_AFTER\n").into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            2,
            &CommandInvocation::new("source-file", [conditional.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 2,
            output: format!("ROOT_BEFORE\n{diagnostic}\nROOT_AFTER\n").into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            3,
            &CommandInvocation::new(
                "source-file",
                [
                    "-v".to_owned(),
                    good.display().to_string(),
                    invalid.display().to_string(),
                    later.display().to_string(),
                ],
            ),
        ),
        CommandResponse::Success {
            request_id: 3,
            output: format!(
                "{}:1: display-message -p GOOD_OUTPUT\n\
                     {}:1: display-message -p LATER_OUTPUT\n\
                     GOOD_OUTPUT\nLATER_OUTPUT\n{diagnostic}\n",
                good.display(),
                later.display(),
            )
            .into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            4,
            &CommandInvocation::new(
                "source-file",
                ["-v".to_owned(), nested_multi.display().to_string()],
            ),
        ),
        CommandResponse::Success {
            request_id: 4,
            output: format!(
                "{}:1: display-message -p ROOT_BEFORE\n\
                     {}:2: source-file -v {} {} {}\n\
                     {}:3: display-message -p ROOT_AFTER\n\
                     ROOT_BEFORE\n\
                     {}:1: display-message -p GOOD_OUTPUT\n\
                     {}:1: display-message -p LATER_OUTPUT\n\
                     GOOD_OUTPUT\nLATER_OUTPUT\n{diagnostic}\nROOT_AFTER\n",
                nested_multi.display(),
                nested_multi.display(),
                tmux_path(&good),
                tmux_path(&invalid),
                tmux_path(&later),
                nested_multi.display(),
                good.display(),
                later.display(),
            )
            .into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            5,
            &CommandInvocation::new(
                "source-file",
                ["-v".to_owned(), nested_unknown.display().to_string()],
            ),
        ),
        CommandResponse::Success {
            request_id: 5,
            output: format!(
                "{}:1: display-message -p ROOT_BEFORE\n\
                     {}:2: source-file {}\n\
                     {}:3: display-message -p ROOT_AFTER\n\
                     ROOT_BEFORE\n{}:1: unknown command: wibble\nROOT_AFTER\n",
                nested_unknown.display(),
                nested_unknown.display(),
                tmux_path(&unknown),
                nested_unknown.display(),
                unknown.display(),
            )
            .into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
}

#[test]
fn source_file_batches_parsing_before_replay() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let assignment = directory.path().join("assignment.conf");
    fs::write(&assignment, "SOURCE_BATCH_ASSIGN=1\n").expect("assignment source");
    let assignment_branch = directory.path().join("assignment-branch.conf");
    fs::write(
        &assignment_branch,
        "%if \"$SOURCE_BATCH_ASSIGN\"\n\
             display-message -p ASSIGN_VISIBLE\n\
             %else\n\
             display-message -p ASSIGN_MISSING\n\
             %endif\n",
    )
    .expect("assignment branch source");
    let replay = directory.path().join("replay.conf");
    fs::write(&replay, "set-environment -g SOURCE_REPLAY_FLAG 1\n").expect("replay source");
    let replay_branch = directory.path().join("replay-branch.conf");
    fs::write(
        &replay_branch,
        "%if \"$SOURCE_REPLAY_FLAG\"\n\
             display-message -p REPLAY_VISIBLE\n\
             %else\n\
             display-message -p REPLAY_NOT_VISIBLE\n\
             %endif\n",
    )
    .expect("replay branch source");
    let parse_only_assignment = directory.path().join("parse-only-assignment.conf");
    fs::write(&parse_only_assignment, "SOURCE_PARSE_ONLY_ASSIGN=1\n")
        .expect("parse-only assignment source");
    let parse_only_branch = directory.path().join("parse-only-branch.conf");
    fs::write(
        &parse_only_branch,
        "%if \"$SOURCE_PARSE_ONLY_ASSIGN\"\n\
             display-message -p PARSE_ONLY_VISIBLE\n\
             %else\n\
             display-message -p PARSE_ONLY_NOT_VISIBLE\n\
             %endif\n",
    )
    .expect("parse-only branch source");

    let shared = Arc::new(Shared::new(63));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "source-batch"]),
        )
        .expect("source batch session");

    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            1,
            &CommandInvocation::new(
                "source-file",
                [
                    "-v".to_owned(),
                    assignment.display().to_string(),
                    assignment_branch.display().to_string(),
                    replay.display().to_string(),
                    replay_branch.display().to_string(),
                ],
            ),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: format!(
                "{}:2: display-message -p ASSIGN_VISIBLE\n\
                     {}:1: set-environment -g SOURCE_REPLAY_FLAG 1\n\
                     {}:4: display-message -p REPLAY_NOT_VISIBLE\n\
                     ASSIGN_VISIBLE\n\
                     REPLAY_NOT_VISIBLE\n",
                assignment_branch.display(),
                replay.display(),
                replay_branch.display(),
            )
            .into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            2,
            &CommandInvocation::new(
                "source-file",
                [
                    "-nv".to_owned(),
                    parse_only_assignment.display().to_string(),
                    parse_only_branch.display().to_string(),
                ],
            ),
        ),
        CommandResponse::Success {
            request_id: 2,
            output: format!(
                "{}:4: display-message -p PARSE_ONLY_NOT_VISIBLE\n",
                parse_only_branch.display()
            )
            .into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    let inner = shared.inner.lock();
    assert_eq!(
        inner
            .engine
            .global_environment_variable("SOURCE_BATCH_ASSIGN"),
        Some("1".to_owned())
    );
    assert_eq!(
        inner
            .engine
            .global_environment_variable("SOURCE_REPLAY_FLAG"),
        Some("1".to_owned())
    );
    assert_eq!(
        inner
            .engine
            .global_environment_variable("SOURCE_PARSE_ONLY_ASSIGN"),
        None
    );
}

#[test]
fn config_alias_snapshot_spans_source_batches_and_refreshes_nested_sources() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let first = directory.path().join("first.conf");
    let child = directory.path().join("child.conf");
    fs::write(
        &first,
        format!(
            "ALIAS_PARSE_ENV=one\n\
                 snap\n\
                 set-option -s command-alias[90] 'snap=display-message -p nested-new'\n\
                 source-file '{}'\n",
            child.display()
        ),
    )
    .expect("first alias source");
    fs::write(&child, "ALIAS_PARSE_ENV=two\nsnap\n").expect("child alias source");

    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "source-alias"]),
        )
        .expect("source alias session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[90]",
                    "snap=display-message -p $ALIAS_PARSE_ENV",
                ],
            ),
        )
        .expect("seed source alias");

    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            1,
            &CommandInvocation::new(
                "source-file",
                [first.display().to_string(), child.display().to_string(),],
            ),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: "one\nnested-new\ntwo\n".into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    let inner = shared.inner.lock();
    assert_eq!(
        inner.engine.global_environment_variable("ALIAS_PARSE_ENV"),
        Some("two".to_owned())
    );
}

#[test]
fn replayed_error_hook_output_stays_in_stdout_order() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let config = directory.path().join("runtime.conf");
    fs::write(
        &config,
        "display-message -p BEFORE\n\
             kill-session -t missing-runtime\n\
             display-message -p AFTER\n\
             list-sessions -F 'LIST_#{session_name}'\n",
    )
    .expect("runtime source");

    let shared = Arc::new(Shared::new(63));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "s"]),
        )
        .expect("runtime source session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                ["-g", "command-error", "display-message -p HOOK"],
            ),
        )
        .expect("command-error hook");

    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            1,
            &CommandInvocation::new("source-file", [config.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: "BEFORE\nHOOK\nAFTER\nLIST_s\n".into(),
            exit_code: 1,
            stderr: "can't find session: missing-runtime\n".to_owned(),
            stdout_claim: StdoutClaim::Print,
        }
    );
}
