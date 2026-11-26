use super::*;

#[test]
fn read_only_terminal_view_actions_use_the_typed_navigation_allowlist() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) =
        output_view_session_fixture(&shared, "read-only-view", "one\ntwo\nthree");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("read-only-viewer".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(client, session).expect("attach viewer");
    shared.inner.lock().attachment.client_flags.insert(client);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("view context");

    let safe = [
        TerminalViewAction::ScrollLines(-1),
        TerminalViewAction::ScrollPages(-1),
        TerminalViewAction::ScrollTop,
        TerminalViewAction::ScrollBottom,
        TerminalViewAction::ScrollToFraction(0),
        TerminalViewAction::ScrollToOffset(0),
        TerminalViewAction::EnterCopyMode,
        TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::SetMark),
        TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Left),
    ];
    for action in safe {
        assert!(terminal_view_action_is_read_only_safe(&action));
        let before = {
            let mut inner = shared.inner.lock();
            inner.engine.state.set_pane_bell(pane, true);
            assert!(inner.engine.state.pane(pane).expect("view pane").bell);
            (
                inner.attachment.activity_sequence,
                inner.panes.terminal_input_sequence,
            )
        };
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::TerminalView { pane, action },
            )
            .expect("apply read-only local view action");
        let inner = shared.inner.lock();
        assert_eq!(inner.attachment.activity_sequence, before.0 + 1);
        assert_eq!(inner.panes.terminal_input_sequence, before.1 + 1);
        assert!(inner.engine.state.pane(pane).expect("view pane").bell);
    }
    assert_eq!(
        shared.inner.lock().clients[&client]
            .copy_session
            .as_ref()
            .unwrap()
            .pane,
        pane
    );

    let mouse = || {
        TerminalMouseInput::new(
            TerminalMousePhase::Press,
            Some(TerminalMouseButton::ScrollUp),
            PointerCellEvent {
                column: 0,
                row: 0,
                click_count: 1,
                rectangle: false,
            },
            0,
            0,
            80,
            24,
            8,
            18,
            Modifiers::default(),
            false,
        )
    };
    let blocked = [
        TerminalViewAction::Paste("blocked".to_owned()),
        TerminalViewAction::ClearHistory,
        TerminalViewAction::SelectionPress(PointerCellEvent {
            column: 0,
            row: 0,
            click_count: 1,
            rectangle: false,
        }),
        TerminalViewAction::CopySelection {
            request_id: 1,
            target: ClipboardTarget::Clipboard,
        },
        TerminalViewAction::SearchBegin(zz_terminal::SearchQuery::literal("blocked")),
        TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::StartSelection),
        TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::SearchAgain { reverse: false }),
        TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::ToggleRectangle),
        TerminalViewAction::Mouse(mouse()),
        TerminalViewAction::ScrollWheel {
            lines: -1,
            input: mouse(),
        },
    ];
    for action in blocked {
        assert!(!terminal_view_action_is_read_only_safe(&action));
        let before = {
            let inner = shared.inner.lock();
            (
                inner.attachment.activity_sequence,
                inner.panes.terminal_input_sequence,
            )
        };
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::TerminalView { pane, action },
            )
            .expect("reject read-only shared view action");
        let inner = shared.inner.lock();
        assert_eq!(inner.attachment.activity_sequence, before.0 + 1);
        assert_eq!(inner.panes.terminal_input_sequence, before.1 + 1);
        assert_eq!(
            inner.clients[&client].copy_session.as_ref().unwrap().pane,
            pane
        );
        assert!(inner.engine.state.pane(pane).expect("view pane").bell);
    }

    let before_focus = {
        let inner = shared.inner.lock();
        (
            inner.attachment.activity_sequence,
            inner.panes.terminal_input_sequence,
        )
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::TerminalView {
                pane,
                action: TerminalViewAction::Focus(true),
            },
        )
        .expect("reject read-only pane focus");
    let inner = shared.inner.lock();
    assert_eq!(inner.attachment.activity_sequence, before_focus.0);
    assert_eq!(inner.panes.terminal_input_sequence, before_focus.1);
    assert!(inner.engine.state.pane(pane).expect("view pane").bell);
}

#[cfg(unix)]
#[test]
fn read_only_pane_focus_never_reaches_application_focus_reporting() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("read-only-focus".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-s",
                    "read-only-focus",
                    "stty raw -echo; printf '\\033[?1004h\\033]0;focus-ready\\007'; bytes=$(dd bs=3 count=1 2>/dev/null | od -An -tx1 | tr -d ' \\n'); printf '\\r\\nFOCUS_BYTES:%s\\r\\n' \"$bytes\"; exec /bin/cat",
                ],
            ),
        )
        .expect("spawn focus fixture");
    let session = context.session.expect("focus session");
    let pane = context.pane.expect("focus pane");
    shared
        .attach(client, session)
        .expect("attach focus fixture");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "focus-events", "on"]),
        )
        .expect("enable application focus events");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    let view = TerminalViewId(client.0);
    wait_for_viewport(
        &terminal,
        view,
        "focus fixture never became ready",
        |viewport| viewport.title() == "focus-ready",
    );

    shared.inner.lock().attachment.client_flags.insert(client);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::TerminalView {
                pane,
                action: TerminalViewAction::Focus(true),
            },
        )
        .expect("reject read-only pane focus");
    thread::sleep(Duration::from_millis(100));
    assert!(
        terminal
            .latest_viewport_for(view)
            .is_some_and(|viewport| !viewport_text(&viewport).contains("FOCUS_BYTES:"))
    );

    shared.inner.lock().attachment.client_flags.remove(client);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::TerminalView {
                pane,
                action: TerminalViewAction::Focus(true),
            },
        )
        .expect("forward writable pane focus");
    wait_for_viewport(
        &terminal,
        view,
        "writable focus did not reach the application",
        |viewport| viewport_text(viewport).contains("FOCUS_BYTES:1b5b49"),
    );
}

#[test]
fn read_only_send_keys_and_binding_preflight_are_all_or_nothing() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) =
        output_view_session_fixture(&shared, "read-only-send", "one\ntwo\nthree");
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("read-only-sender".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(client, session).expect("attach sender");
    let (peer_session, peer_pane, _) =
        output_view_session_fixture(&shared, "read-only-send-peer", "peer");
    let (peer, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("read-only-peer".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(peer, peer_session).expect("attach peer");
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("send context");
    let target = pane.to_string();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "split-window",
                ["--kind", "browser", "-h", "https://example.com"],
            ),
        )
        .expect("create browser input sink");
    let browser = context.pane.expect("browser pane");
    let browser_target = browser.to_string();
    context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("restore send context");
    shared.inner.lock().attachment.client_flags.insert(client);
    take_reliable_messages(&mailbox);

    for (request_id, command) in [
        (
            1,
            CommandInvocation::new("send-keys", ["-t", &browser_target, "blocked", "Enter"]),
        ),
        (
            2,
            CommandInvocation::new("send-keys", ["-t", &target, "-X", "begin-selection"]),
        ),
    ] {
        assert!(matches!(
            shared.execute_command_request(
                client,
                ClientKind::Interactive,
                &mut context,
                request_id,
                &command,
            ),
            CommandResponse::Error {
                error: ServerError::InvalidCommand(message),
                ..
            } if message == "client is read-only"
        ));
        assert!(take_browser_literals(&mailbox, browser).is_empty());
    }
    let peer_target = peer_pane.to_string();
    assert!(matches!(
        shared.execute_command_request(
            client,
            ClientKind::Interactive,
            &mut context,
            5,
            &CommandInvocation::new(
                "send-keys",
                ["-t", &peer_target, "-X", "cursor-left"],
            ),
        ),
        CommandResponse::Error {
            error: ServerError::InvalidCommand(message),
            ..
        } if message == "not in a mode"
    ));

    assert!(matches!(
        shared.execute_command_request(
            client,
            ClientKind::Interactive,
            &mut context,
            3,
            &CommandInvocation::new("copy-mode", ["-t", &target]),
        ),
        CommandResponse::Success { .. }
    ));
    assert_eq!(
        shared.inner.lock().clients[&client]
            .copy_session
            .as_ref()
            .unwrap()
            .pane,
        pane
    );
    assert!(matches!(
        shared.execute_command_request(
            client,
            ClientKind::Interactive,
            &mut context,
            4,
            &CommandInvocation::new("send-keys", ["-t", &target, "-X", "cursor-left"],),
        ),
        CommandResponse::Success { .. }
    ));

    shared
        .execute_key_commands(
            client,
            ClientKind::Interactive,
            &mut context,
            pane,
            &[
                CommandInvocation::new("detach-client", [] as [&str; 0]),
                CommandInvocation::new("send-keys", ["-t", &browser_target, "blocked"]),
            ],
            false,
        )
        .expect("preflight mixed binding");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), client),
        Some(session)
    );
    let blocked_messages = take_reliable_messages(&mailbox);
    assert!(!blocked_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::BrowserCommand { pane, .. },
            ..
        }) if *pane == browser
    )));
    assert!(blocked_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::TimedClientMessage { text, .. },
            ..
        }) if text == "client is read-only"
    )));

    shared
        .execute_key_commands(
            client,
            ClientKind::Interactive,
            &mut context,
            pane,
            &[CommandInvocation::new(
                "send-keys",
                ["-t", &target, "-X", "cancel"],
            )],
            false,
        )
        .expect("run safe copy binding");
    assert!(
        shared.inner.lock().clients[&client]
            .copy_session
            .as_ref()
            .unwrap()
            .exiting
    );
    assert!(
        !take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::TimedClientMessage { text, .. },
                    ..
                }) if text == "client is read-only"
            ))
    );

    for (target_client, literal) in [
        ("read-only-peer", "K"),
        ("/dev/pts/nonexistent-client", "N"),
    ] {
        shared
            .execute_key_commands(
                client,
                ClientKind::Interactive,
                &mut context,
                pane,
                &[CommandInvocation::new(
                    "send-keys",
                    ["-c", target_client, "-t", &browser_target, "-l", literal],
                )],
                false,
            )
            .expect("run client-selected binding");
        assert_eq!(
            take_browser_literals(&mailbox, browser),
            vec![literal.to_owned()],
            "binding selecting {target_client} must follow that client's flag"
        );
    }
    shared
        .execute_key_commands(
            client,
            ClientKind::Interactive,
            &mut context,
            pane,
            &[CommandInvocation::new(
                "send-keys",
                ["-c", "read-only-sender", "-t", &browser_target, "-l", "U"],
            )],
            false,
        )
        .expect("preflight self-selected binding");
    let self_messages = take_reliable_messages(&mailbox);
    assert!(!self_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::BrowserCommand { pane, .. },
            ..
        }) if *pane == browser
    )));
    assert!(self_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::TimedClientMessage { text, .. },
            ..
        }) if text == "client is read-only"
    )));
}

#[test]
fn read_only_command_requests_authorize_the_expanded_alias_once() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, _) = switch_test_session(&shared, "A");
    let (b, _, _) = switch_test_session(&shared, "B");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("viewer".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("attach-session", ["-r", "-t", "A"]),
        )
        .expect("read-only attach");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                ["-s", "command-alias[40]", "a=kill-session -t B"],
            ),
        )
        .expect("unsafe alias");

    let blocked = shared.execute_command_request(
        client,
        ClientKind::Interactive,
        &mut context,
        9,
        &CommandInvocation::new("a", [] as [&str; 0]),
    );
    assert!(matches!(
        blocked,
        CommandResponse::Error {
            error: ServerError::InvalidCommand(message),
            ..
        } if message == "client is read-only"
    ));
    assert!(shared.inner.lock().engine.state.sessions.contains_key(&b));
    assert_eq!(
        client_attached_session(&shared.inner.lock(), client),
        Some(a)
    );

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[40]",
                    "a=list-clients -F '#{client_name}'",
                ],
            ),
        )
        .expect("safe alias");
    let allowed = shared.execute_command_request(
        client,
        ClientKind::Interactive,
        &mut context,
        10,
        &CommandInvocation::new("a", [] as [&str; 0]),
    );
    assert!(matches!(
        allowed,
        CommandResponse::Success { output, .. } if output == "viewer"
    ));
}

#[test]
fn prepared_command_lists_freeze_one_alias_layer_without_side_effects() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(90);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "work"]),
        )
        .unwrap();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-s", "command-alias[40]", "a=display-message -p old"],
            ),
        )
        .unwrap();
    for (index, value) in [
        ("41", "multi=display-message -p first ; display-message -p"),
        ("42", "empty="),
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    ["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .unwrap();
    }
    for (index, value) in [
        ("43", "child10e=display-message -p nested"),
        ("44", "outer10e=confirm-before { child10e }"),
        ("45", "loop10e=confirm-before { loop10e }"),
        ("46", "outerbad10e=wibble { childbad10e }"),
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    ["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .unwrap();
    }
    let source = zz_protocol::SourceSpan {
        source: "control".to_owned(),
        line: 7,
        column: 3,
    };
    let classified = shared.prepare_command_list(vec![
        CommandInvocation::new("a", ["tail"]).with_source(source.clone()),
        CommandInvocation::new("list-sess", [] as [&str; 0]),
        CommandInvocation::new("new-pane", [] as [&str; 0]),
        CommandInvocation::new("bogus", [] as [&str; 0]),
        CommandInvocation::new("kill-s", [] as [&str; 0]),
        CommandInvocation::new("multi", ["last"]),
        CommandInvocation::new("empty", ["discarded"]),
    ]);
    assert_eq!(classified[0].invocation.name, "display-message");
    assert_eq!(classified[0].invocation.args, ["-p", "old", "tail"]);
    assert_eq!(classified[0].invocation.source.as_ref(), Some(&source));
    assert!(classified[0].alias_matched);
    assert_eq!(
        classified[1].canonical_name.as_deref(),
        Some("list-sessions")
    );
    assert_eq!(classified[1].result, PreparedCommandResult::Ready);
    assert_eq!(classified[2].canonical_name.as_deref(), Some("new-pane"));
    assert_eq!(classified[2].result, PreparedCommandResult::Ready);
    for command in &classified[3..5] {
        assert!(!command.alias_matched);
        assert!(matches!(
            command.result,
            PreparedCommandResult::Error(ref error) if error.is_command_parse()
        ));
    }
    for command in &classified[5..] {
        assert!(command.alias_matched);
        assert_eq!(command.canonical_name, None);
        assert!(MuxEngine::is_command_alias_group(&command.invocation));
        assert_eq!(command.result, PreparedCommandResult::Ready);
    }
    assert!(matches!(
        shared.execute_command_request_with_prepared(
            client,
            ClientKind::Command,
            &mut context,
            40,
            &classified[5].invocation,
            true,
        ),
        CommandResponse::Success { output, exit_code: 0, .. } if output == "first\nlast"
    ));
    assert!(matches!(
        shared.execute_command_request_with_prepared(
            client,
            ClientKind::Command,
            &mut context,
            41,
            &classified[6].invocation,
            true,
        ),
        CommandResponse::Success { output, exit_code: 0, .. } if output.is_empty()
    ));
    let typed_option = shared.prepare_command_list(vec![
        CommandInvocation::new(
            "bind",
            [
                "-T",
                "{ display-message typed-table }",
                "F1",
                "display-message",
            ],
        )
        .with_command_blocks([1]),
    ]);
    assert_eq!(
        typed_option[0].result,
        PreparedCommandResult::Error(ServerError::CommandParse(
            "command bind-key: -T argument must be a string".to_owned()
        ))
    );
    let typed_confirm = shared.prepare_command_list(vec![
        CommandInvocation::new("confirm-before", ["{ display -p ready }"]).with_command_blocks([0]),
        CommandInvocation::new("confirm-before", ["{ wibble }"]).with_command_blocks([0]),
    ]);
    assert_eq!(
        typed_confirm[0].invocation.args,
        ["{ display-message -p ready }"]
    );
    assert_eq!(typed_confirm[0].result, PreparedCommandResult::Ready);
    assert_eq!(
        typed_confirm[1].result,
        PreparedCommandResult::Error(ServerError::CommandParse(
            "unknown command: wibble".to_owned()
        ))
    );
    let nested = {
        let inner = shared.inner.lock();
        inner.engine.parse_config(
            "control",
            "confirm-before { if-shell -F 1 { child10e } }\n\
                 bind-key -T review F1 confirm-before { wibble }",
        )
    };
    assert!(nested.diagnostics.is_empty());
    let nested = shared.prepare_command_list(nested.commands);
    assert_eq!(
        nested[0].invocation.args,
        ["{ if-shell -F 1 { display-message -p nested } }"]
    );
    assert_eq!(nested[0].result, PreparedCommandResult::Ready);
    assert_eq!(
        nested[1].result,
        PreparedCommandResult::Error(ServerError::CommandParse(
            "unknown command: wibble".to_owned()
        ))
    );
    let lexical_errors = {
        let inner = shared.inner.lock();
        inner
            .engine
            .parse_config("control", "wibble { childbad10e }\nouterbad10e")
    };
    assert!(lexical_errors.diagnostics.is_empty());
    for command in shared.prepare_command_list(lexical_errors.commands) {
        assert_eq!(
            command.result,
            PreparedCommandResult::Error(ServerError::CommandParse(
                "unknown command: childbad10e".to_owned()
            ))
        );
    }
    let alias_budget = shared.prepare_command_list(vec![
        CommandInvocation::new("outer10e", [] as [&str; 0]),
        CommandInvocation::new("loop10e", [] as [&str; 0]),
    ]);
    for (command, expected) in alias_budget
        .iter()
        .zip(["unknown command: child10e", "unknown command: loop10e"])
    {
        assert!(command.alias_matched);
        assert_eq!(
            command.result,
            PreparedCommandResult::Error(ServerError::CommandParse(expected.to_owned()))
        );
    }
    let ordinary_invalid =
        shared.prepare_command_list(vec![CommandInvocation::new("list-sessions", ["-Z"])]);
    assert_eq!(
        ordinary_invalid[0].result,
        PreparedCommandResult::Error(ServerError::CommandParse(
            "command list-sessions: unknown flag -Z".to_owned()
        ))
    );
    let mutation = CommandInvocation::new(
        "set-option",
        ["-s", "command-alias[40]", "a=display-message -p new"],
    );
    let prepared = shared.prepare_command_list(vec![
        CommandInvocation::new("a", [] as [&str; 0]),
        mutation,
        CommandInvocation::new("a", [] as [&str; 0]),
    ]);
    assert_eq!(prepared.len(), 3);
    for command in [&prepared[0], &prepared[2]] {
        assert!(command.alias_matched);
        assert_eq!(command.canonical_name.as_deref(), Some("display-message"));
        assert_eq!(command.invocation.name, "display-message");
        assert_eq!(command.invocation.args, ["-p", "old"]);
        assert_eq!(command.result, PreparedCommandResult::Ready);
    }
    assert!(!prepared[1].alias_matched);
    assert_eq!(prepared[1].canonical_name.as_deref(), Some("set-option"));

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            client,
            ClientKind::Command,
            &mut context,
            1,
            &prepared[1].invocation,
            true,
        ),
        CommandResponse::Success { .. }
    ));
    assert!(matches!(
        shared.execute_command_request_with_prepared(
            client,
            ClientKind::Command,
            &mut context,
            2,
            &prepared[2].invocation,
            true,
        ),
        CommandResponse::Success { output, .. } if output == "old"
    ));
    let refreshed = shared.prepare_command_list(vec![CommandInvocation::new("a", [] as [&str; 0])]);
    assert_eq!(refreshed[0].invocation.args, ["-p", "new"]);

    assert!(matches!(
        shared.execute_command_request_with_prepared(
            client,
            ClientKind::Command,
            &mut context,
            3,
            &CommandInvocation::new("set-option", ["-Z", "@bad", "value"]),
            true,
        ),
        CommandResponse::Error {
            error: ServerError::CommandParse(message),
            ..
        } if message == "command set-option: unknown flag -Z"
    ));
}

#[test]
fn prepared_commands_reject_static_syntax_before_ready_except_exact_native_attach() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(91);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "work"]),
        )
        .expect("session");

    let exact_native_attach = [
        shared
            .prepare_command_list(vec![CommandInvocation::new(
                "attach",
                ["--restart-daemon", "work"],
            )])
            .remove(0),
        shared
            .prepare_command_list(vec![CommandInvocation::new("attach-session", ["work"])])
            .remove(0),
    ];
    for command in exact_native_attach {
        assert!(!command.alias_matched);
        assert_eq!(command.canonical_name.as_deref(), Some("attach-session"));
        assert_eq!(command.result, PreparedCommandResult::Ready);
    }

    let later_exact_attach = shared.prepare_command_list(vec![
        CommandInvocation::new("list-sessions", [] as [&str; 0]),
        CommandInvocation::new("attach", ["work"]),
        CommandInvocation::new("attach-session", ["work"]),
    ]);
    assert_eq!(later_exact_attach[0].result, PreparedCommandResult::Ready);
    for command in &later_exact_attach[1..] {
        assert!(!command.alias_matched);
        assert_eq!(command.canonical_name.as_deref(), Some("attach-session"));
        assert_eq!(
            command.result,
            PreparedCommandResult::Error(ServerError::CommandParse(
                "command attach-session: too many arguments (need at most 0)".to_owned()
            ))
        );
    }

    for (index, value) in [
        (60, "badflag10r=list-sessions -Z"),
        (61, "badarity10r=list-sessions extra"),
        (62, "list-sessions=display-message -p shadow"),
        (
            63,
            "nested10r=confirm-before { if-shell -F 1 { bind-key -T { display-message bad-table } F2 display-message } }",
        ),
        (64, "parked10r=capture-pane -C"),
        (65, "go10u=attach-session work"),
        (66, "attach=attach-session work"),
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    ["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .expect("alias");
    }

    let prepared = shared.prepare_command_list(vec![
        CommandInvocation::new("list-windows", ["-Z"]),
        CommandInvocation::new("ls", ["extra"]),
        CommandInvocation::new("list-sess", ["-F"]),
        CommandInvocation::new("clock-mode", ["-Z"]),
        CommandInvocation::new("agent-send", ["--submit", "hello"]),
        CommandInvocation::new("badflag10r", [] as [&str; 0]),
        CommandInvocation::new("badarity10r", [] as [&str; 0]),
        CommandInvocation::new("list-sessions", [] as [&str; 0]),
        CommandInvocation::new("nested10r", [] as [&str; 0]),
        CommandInvocation::new("parked10r", [] as [&str; 0]),
        CommandInvocation::new("go10u", [] as [&str; 0]),
        CommandInvocation::new("attach", [] as [&str; 0]),
    ]);

    for (command, expected) in prepared[..4].iter().zip([
        "command list-windows: unknown flag -Z",
        "command list-sessions: too many arguments (need at most 0)",
        "command list-sessions: -F expects an argument",
        "command clock-mode: unknown flag -Z",
    ]) {
        assert!(!command.alias_matched);
        assert_eq!(
            command.result,
            PreparedCommandResult::Error(ServerError::CommandParse(expected.to_owned()))
        );
    }
    assert_eq!(prepared[4].canonical_name.as_deref(), Some("agent-send"));
    assert_eq!(prepared[4].result, PreparedCommandResult::Ready);
    assert_eq!(
        prepared[5].result,
        PreparedCommandResult::Error(ServerError::CommandParse(
            "command list-sessions: unknown flag -Z".to_owned()
        ))
    );
    assert_eq!(
        prepared[6].result,
        PreparedCommandResult::Error(ServerError::CommandParse(
            "command list-sessions: too many arguments (need at most 0)".to_owned()
        ))
    );
    assert!(prepared[7].alias_matched);
    assert_eq!(
        prepared[7].canonical_name.as_deref(),
        Some("display-message")
    );
    assert_eq!(prepared[7].invocation.args, ["-p", "shadow"]);
    assert_eq!(prepared[7].result, PreparedCommandResult::Ready);
    assert_eq!(
        prepared[8].result,
        PreparedCommandResult::Error(ServerError::CommandParse(
            "command bind-key: -T argument must be a string".to_owned()
        ))
    );
    assert!(prepared[9].alias_matched);
    assert_eq!(prepared[9].canonical_name.as_deref(), Some("capture-pane"));
    assert_eq!(prepared[9].result, PreparedCommandResult::Ready);
    for command in &prepared[10..] {
        assert!(command.alias_matched);
        assert_eq!(command.canonical_name.as_deref(), Some("attach-session"));
        assert_eq!(
            command.result,
            PreparedCommandResult::Error(ServerError::CommandParse(
                "command attach-session: too many arguments (need at most 0)".to_owned()
            ))
        );
    }
}

#[test]
fn cold_prepare_abort_requires_an_exclusive_owner_and_disconnect() {
    let request = |server_id: u64| {
        vec![
            CommandInvocation::new("frobnicate", [] as [&str; 0]),
            CommandInvocation::new(
                zz_daemon_client::COLD_START_PREPARE_ABORT_COMMAND,
                [server_id.to_string()],
            ),
        ]
    };
    let register = |shared: &Shared, startup_reentry| {
        shared
            .register(
                ClientKind::Command,
                ClientInstanceId::default(),
                None,
                None,
                false,
                startup_reentry,
            )
            .expect("register bootstrap client")
            .0
    };

    let exclusive = Arc::new(Shared::new(71));
    exclusive.inner.lock().attachment.cold_bootstrap = ColdBootstrapLease::AwaitingFirstExternal;
    let owner = register(&exclusive, false);
    let prepared = exclusive.prepare_command_list_for_request(owner, request(71));
    assert_eq!(prepared.len(), 1);
    assert!(matches!(
        &prepared[0].result,
        PreparedCommandResult::Error(ServerError::CommandParse(message))
            if message == "unknown command: frobnicate"
    ));
    assert_eq!(
        exclusive.inner.lock().attachment.cold_bootstrap,
        ColdBootstrapLease::PendingAbort(owner)
    );
    assert!(!exclusive.stopping.load(Ordering::Acquire));
    exclusive.unregister(owner);
    assert!(exclusive.stopping.load(Ordering::Acquire));

    let wrong_generation = Arc::new(Shared::new(72));
    wrong_generation.inner.lock().attachment.cold_bootstrap =
        ColdBootstrapLease::AwaitingFirstExternal;
    let owner = register(&wrong_generation, false);
    let _ = wrong_generation.prepare_command_list_for_request(owner, request(71));
    wrong_generation.unregister(owner);
    assert!(!wrong_generation.stopping.load(Ordering::Acquire));

    let contested = Arc::new(Shared::new(73));
    contested.inner.lock().attachment.cold_bootstrap = ColdBootstrapLease::AwaitingFirstExternal;
    let owner = register(&contested, false);
    let startup_reentry = register(&contested, true);
    assert_eq!(
        contested.inner.lock().attachment.cold_bootstrap,
        ColdBootstrapLease::Owned(owner)
    );
    contested.unregister(startup_reentry);
    let competitor = register(&contested, false);
    contested.unregister(competitor);
    assert_eq!(
        contested.inner.lock().attachment.cold_bootstrap,
        ColdBootstrapLease::Contested
    );
    let _ = contested.prepare_command_list_for_request(owner, request(73));
    contested.unregister(owner);
    assert!(!contested.stopping.load(Ordering::Acquire));

    let pipelined = Arc::new(Shared::new(75));
    pipelined.inner.lock().attachment.cold_bootstrap = ColdBootstrapLease::AwaitingFirstExternal;
    let owner = register(&pipelined, false);
    let _ = pipelined.prepare_command_list_for_request(owner, request(75));
    pipelined
        .inner
        .lock()
        .attachment
        .cold_bootstrap
        .command(owner);
    pipelined.unregister(owner);
    assert!(!pipelined.stopping.load(Ordering::Acquire));

    let valid = Arc::new(Shared::new(74));
    valid.inner.lock().attachment.cold_bootstrap = ColdBootstrapLease::AwaitingFirstExternal;
    let owner = register(&valid, false);
    let commands = vec![
        CommandInvocation::new("list-sessions", [] as [&str; 0]),
        CommandInvocation::new(zz_daemon_client::COLD_START_PREPARE_ABORT_COMMAND, ["74"]),
    ];
    let _ = valid.prepare_command_list_for_request(owner, commands);
    assert_eq!(
        valid.inner.lock().attachment.cold_bootstrap,
        ColdBootstrapLease::Committed
    );
    valid.unregister(owner);
    assert!(!valid.stopping.load(Ordering::Acquire));

    let stopping = Shared::new(76);
    stopping.stopping.store(true, Ordering::Release);
    assert!(
        stopping
            .register(
                ClientKind::Command,
                ClientInstanceId::default(),
                None,
                None,
                false,
                false,
            )
            .is_none()
    );
}

#[test]
fn stored_callbacks_do_not_reexpand_constructed_commands() {
    let (shared, client, _, mut context) = popup_test_workspace("callback-freeze");
    let set_alias = |index: u8, value: &str| {
        let mut alias_context = ExecutionContext::default();
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut alias_context,
                &CommandInvocation::new(
                    "set-option",
                    ["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .expect("set callback alias");
    };

    set_alias(90, "a10e=display-message");
    set_alias(91, "display-message=set-environment -g RUN_DOUBLE yes");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("run-shell", ["-C", "{ a10e }"]).with_command_blocks([1]),
        )
        .expect("run prepared block");
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("RUN_DOUBLE"),
        None
    );

    set_alias(91, "display-message=list-sessions");
    set_alias(92, "list-sessions=set-environment -g HOOK_THIRD yes");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-hook", ["-g", "after-new-window", "{ a10e }"])
                .with_command_blocks([2]),
        )
        .expect("store constructed hook");
    let stored_hook = shared
        .inner
        .lock()
        .engine
        .event_hook_commands(&context, "after-new-window")
        .expect("stored hook");
    assert_eq!(stored_hook[0][0].name, "list-sessions");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-hook", ["-R", "after-new-window"]),
        )
        .expect("run stored hook");
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("HOOK_THIRD"),
        None
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("bind-key", ["-T", "root", "F1", "{ a10e }"])
                .with_command_blocks([3]),
        )
        .expect("store constructed binding");
    let commands = shared
        .inner
        .lock()
        .engine
        .keys
        .get("root", "F1")
        .expect("stored binding")
        .commands
        .clone();
    assert_eq!(commands[0].name, "display-message");
    set_alias(91, "display-message=set-environment -g BIND_DOUBLE yes");
    let pane = context.pane.expect("callback pane");
    shared
        .execute_key_commands(
            client,
            ClientKind::Interactive,
            &mut context,
            pane,
            &commands,
            false,
        )
        .expect("run stored binding");
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("BIND_DOUBLE"),
        None
    );
}

#[test]
fn forged_prepared_requests_are_still_read_only_authorized() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, _) = switch_test_session(&shared, "A");
    let (b, _, _) = switch_test_session(&shared, "B");
    let (client, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("viewer".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("attach-session", ["-r", "-t", "A"]),
        )
        .unwrap();
    let response = shared.execute_command_request_with_prepared(
        client,
        ClientKind::Control,
        &mut context,
        7,
        &CommandInvocation::new("kill-session", ["-t", "B"]),
        true,
    );
    assert!(matches!(
        response,
        CommandResponse::Error {
            error: ServerError::InvalidCommand(message),
            ..
        } if message == "client is read-only"
    ));
    assert!(shared.inner.lock().engine.state.sessions.contains_key(&b));
    assert_eq!(
        client_attached_session(&shared.inner.lock(), client),
        Some(a)
    );

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[41]",
                    "mixed=list-clients ; kill-session -t B",
                ],
            ),
        )
        .expect("mixed alias");
    let prepared = shared
        .prepare_command_list(vec![CommandInvocation::new("mixed", [] as [&str; 0])])
        .remove(0);
    assert!(MuxEngine::is_command_alias_group(&prepared.invocation));
    let response = shared.execute_command_request_with_prepared(
        client,
        ClientKind::Control,
        &mut context,
        8,
        &prepared.invocation,
        true,
    );
    assert!(matches!(
        response,
        CommandResponse::Error {
            error: ServerError::InvalidCommand(message),
            ..
        } if message == "client is read-only"
    ));
    assert!(shared.inner.lock().engine.state.sessions.contains_key(&b));
}
