use super::*;

#[test]
fn text_suppression_filter_borrows_the_normal_path_and_owns_changes() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(99);
    let text = String::from("xλz");
    assert!(matches!(
        shared.filter_suppressed_text(client, &text, None),
        Cow::Borrowed("xλz")
    ));

    shared
        .inner
        .lock()
        .client_entry(client)
        .suppressed_text
        .get_or_insert_default()
        .insert('x', 1);
    assert!(matches!(
        shared.filter_suppressed_text(
            client,
            &text,
            Some(PendingCommittedText {
                pane: PaneId(1),
                lane: CommittedTextLane::Terminal,
                suppressed_character: Some('x'),
            }),
        ),
        Cow::Owned(filtered) if filtered == "λz"
    ));
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.suppressed_text.is_none())));
}

#[test]
fn text_that_fails_to_repay_a_binding_disarms_the_suppression() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(98);
    shared
        .inner
        .lock()
        .client_entry(client)
        .suppressed_text
        .replace(BTreeMap::from([('c', 1)]));
    assert_eq!(
        shared
            .filter_suppressed_text(
                client,
                "ç",
                Some(PendingCommittedText {
                    pane: PaneId(1),
                    lane: CommittedTextLane::Terminal,
                    suppressed_character: Some('c'),
                }),
            )
            .as_ref(),
        "ç"
    );
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.suppressed_text.is_none())));
}

#[test]
fn a_swallowed_chord_press_pairs_with_its_bare_release() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(97);
    assert_eq!(
        shared.key_decision(client, "C-b", false),
        KeyDecision::Prefix
    );
    assert_eq!(shared.key_decision(client, "b", true), KeyDecision::Prefix);
    assert!(
        shared.inner.lock().clients[&client]
            .swallowed_keys
            .as_ref()
            .unwrap()
            .is_empty()
    );
    assert_eq!(shared.key_decision(client, "b", true), KeyDecision::Pass);
}

#[test]
fn cancel_prefix_disarms_only_the_prefix_table() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let prefix_events = |mailbox: &OutboundMailbox| {
        let mut states = Vec::new();
        let mut cancelled = Vec::new();
        for message in take_reliable_messages(mailbox) {
            match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::PrefixArmed { armed },
                    ..
                }) => states.push(armed),
                ProtocolMessage::Event(Event {
                    payload: EventPayload::PrefixCancelled { request_id },
                    ..
                }) => cancelled.push(request_id),
                _ => {}
            }
        }
        (states, cancelled)
    };
    assert_eq!(
        shared.key_decision(client, "C-b", false),
        KeyDecision::Prefix
    );
    shared.sync_key_table(client, false);
    assert_eq!(prefix_events(&mailbox), (vec![true], Vec::new()));

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::CancelPrefix { request_id: 11 },
        )
        .expect("cancel prefix");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        None
    );
    assert!(!prefix_published(&shared.inner.lock(), client));
    assert_eq!(prefix_events(&mailbox), (vec![false], vec![11]));

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::CancelPrefix { request_id: 12 },
        )
        .expect("cancel unarmed prefix");
    assert_eq!(prefix_events(&mailbox), (Vec::new(), vec![12]));

    for table in ["copy-mode-vi", "custom-client-table"] {
        shared
            .inner
            .lock()
            .client_entry(client)
            .key_engine
            .get_or_insert_default()
            .switch_table(None);
        assert_eq!(
            shared.key_decision(client, "C-b", false),
            KeyDecision::Prefix
        );
        shared.sync_key_table(client, false);
        assert_eq!(prefix_events(&mailbox), (vec![true], Vec::new()));
        shared
            .inner
            .lock()
            .client_entry(client)
            .key_engine
            .get_or_insert_default()
            .switch_table(Some(table.to_owned()));

        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                InputMessage::CancelPrefix { request_id: 13 },
            )
            .expect("cancel outside prefix table");

        assert_eq!(
            shared.inner.lock().clients[&client]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            Some(table)
        );
        assert!(!prefix_published(&shared.inner.lock(), client));
        assert_eq!(prefix_events(&mailbox), (vec![false], vec![13]));
    }
}

#[cfg(unix)]
#[test]
fn key_table_sync_shows_user_tables_and_stays_silent_in_copy_mode() {
    let (shared, client, mut context, pane, mailbox) = key_table_fixture("key-table-sync");
    let prefix = test_key(
        KeyCode::Character('b'),
        Modifiers::new(false, true, false, false),
        None,
    );
    let key = |character: char| {
        test_key(
            KeyCode::Character(character),
            Modifiers::default(),
            Some(&character.to_string()),
        )
    };

    for table in [Some("copy-mode"), Some("copy-mode-vi"), None] {
        shared
            .inner
            .lock()
            .client_entry(client)
            .key_engine
            .get_or_insert_default()
            .switch_table(table.map(str::to_owned));
        shared.sync_key_table(client, true);
        assert_eq!(key_table_events(&mailbox), Vec::new(), "{table:?}");
    }

    run_test_command(
        &shared,
        client,
        &mut context,
        &[
            "bind-key",
            "-T",
            "sticky",
            "y",
            "switch-client",
            "-T",
            "sticky",
        ],
    );
    run_test_command(
        &shared,
        client,
        &mut context,
        &["switch-client", "-T", "sticky"],
    );
    assert_eq!(
        key_table_events(&mailbox),
        vec![table_active(Some("sticky"), false)]
    );

    input_test_key(&shared, client, &mut context, pane, key('y'));
    assert_eq!(
        key_table_events(&mailbox),
        vec![table_active(Some("sticky"), false)]
    );

    input_test_key(&shared, client, &mut context, pane, key('z'));
    assert_eq!(key_table_events(&mailbox), vec![table_active(None, false)]);

    input_test_key(&shared, client, &mut context, pane, prefix.clone());
    assert_eq!(
        key_table_events(&mailbox),
        vec![
            EventPayload::PrefixArmed { armed: true },
            table_active(Some("prefix"), false),
        ]
    );
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::CancelPrefix { request_id: 3 },
        )
        .expect("cancel prefix");
    assert_eq!(
        key_table_events(&mailbox),
        vec![
            EventPayload::PrefixArmed { armed: false },
            table_active(None, false),
        ]
    );

    run_test_command(
        &shared,
        client,
        &mut context,
        &["set-option", "-t", "key-table-sync", "key-table", "sticky"],
    );
    run_test_command(
        &shared,
        client,
        &mut context,
        &["switch-client", "-T", "sticky"],
    );
    assert_eq!(key_table_events(&mailbox), Vec::new());
    assert!(shared.read_client(client, |c| {
        c.is_none_or(|c| c.published_key_table.is_none())
    }));
}

#[cfg(unix)]
#[test]
fn a_reattach_republishes_the_live_key_table() {
    let (shared, client, mut context, _, mailbox) = key_table_fixture("key-table-attach");
    run_test_command(
        &shared,
        client,
        &mut context,
        &["switch-client", "-T", "prefix"],
    );
    assert_eq!(
        key_table_events(&mailbox),
        vec![
            EventPayload::PrefixArmed { armed: true },
            table_active(Some("prefix"), false),
        ]
    );
    let session = context.session.expect("session");
    let snapshot = shared.attach(client, session).expect("attach session");
    assert!(shared.send_attached(client, &mailbox, session, snapshot));
    assert_eq!(
        key_table_events(&mailbox),
        vec![
            EventPayload::PrefixArmed { armed: true },
            table_active(Some("prefix"), false),
        ]
    );
}

#[cfg(unix)]
#[test]
fn closing_output_from_a_repeat_prefix_binding_leaves_the_prefix_disarmed() {
    let (shared, client, mut context, pane, mailbox) = key_table_fixture("stuck-prefix");
    run_test_command(
        &shared,
        client,
        &mut context,
        &["bind-key", "-r", "x", "list-keys"],
    );
    for input in [
        test_key(
            KeyCode::Character('b'),
            Modifiers::new(false, true, false, false),
            None,
        ),
        test_key(KeyCode::Character('x'), Modifiers::default(), Some("x")),
    ] {
        input_test_key(&shared, client, &mut context, pane, input);
    }
    take_command_output_message(&mailbox);
    assert_eq!(
        shared.inner.lock().clients[&client]
            .command_output
            .as_ref()
            .unwrap()
            .previous_key_table,
        None
    );
    let output_id = current_command_output_id(&shared, client);
    input_test_key(
        &shared,
        client,
        &mut context,
        pane,
        test_key(KeyCode::Character('q'), Modifiers::default(), Some("q")),
    );
    wait_for_command_output_close(&mailbox, output_id);
    let inner = shared.inner.lock();
    assert!(
        inner
            .client(client)
            .is_none_or(|c| c.command_output.is_none())
    );
    assert_eq!(
        inner.clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        None
    );
    assert!(!prefix_published(&inner, client));
    assert!(
        inner
            .client(client)
            .is_none_or(|c| c.published_key_table.is_none())
    );
}

#[cfg(unix)]
#[test]
fn key_table_deadlines_clear_the_published_state_without_another_key() {
    let (shared, client, mut context, pane, mailbox) = key_table_fixture("key-table-timer");
    shared.start_timers().expect("start key table deadlines");
    run_test_command(
        &shared,
        client,
        &mut context,
        &["set-option", "-g", "prefix-timeout", "1000"],
    );
    run_test_command(
        &shared,
        client,
        &mut context,
        &["set-option", "-g", "repeat-time", "2000"],
    );
    take_reliable_messages(&mailbox);
    let prefix = test_key(
        KeyCode::Character('b'),
        Modifiers::new(false, true, false, false),
        None,
    );
    let up = test_key(KeyCode::ArrowUp, Modifiers::default(), None);
    let wait_until = |at: Instant| {
        if let Some(remaining) = at.checked_duration_since(Instant::now()) {
            thread::sleep(remaining);
        }
    };
    let wait_for_clear = |mailbox: &OutboundMailbox| {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut events = Vec::new();
        while events
            != vec![
                EventPayload::PrefixArmed { armed: false },
                table_active(None, false),
            ]
        {
            assert!(
                Instant::now() < deadline,
                "published key table never cleared: {events:?}"
            );
            thread::sleep(Duration::from_millis(10));
            events.extend(key_table_events(mailbox));
        }
    };

    let start = Instant::now();
    input_test_key(&shared, client, &mut context, pane, prefix.clone());
    assert_eq!(
        key_table_events(&mailbox),
        vec![
            EventPayload::PrefixArmed { armed: true },
            table_active(Some("prefix"), false),
        ]
    );
    input_test_key(&shared, client, &mut context, pane, up.clone());
    assert_eq!(
        key_table_events(&mailbox),
        vec![table_active(Some("prefix"), true)]
    );
    wait_until(start + Duration::from_millis(1300));
    input_test_key(&shared, client, &mut context, pane, up);
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("prefix")
    );
    assert_eq!(
        key_table_events(&mailbox),
        vec![table_active(Some("prefix"), true)]
    );
    wait_for_clear(&mailbox);
    assert!(start.elapsed() >= Duration::from_millis(3300));
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        None
    );

    let start = Instant::now();
    input_test_key(&shared, client, &mut context, pane, prefix);
    assert_eq!(
        key_table_events(&mailbox),
        vec![
            EventPayload::PrefixArmed { armed: true },
            table_active(Some("prefix"), false),
        ]
    );
    wait_for_clear(&mailbox);
    assert!(start.elapsed() >= Duration::from_secs(1));
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("prefix")
    );
}

#[test]
fn unparsable_alias_shadows_fail_direct_commands_while_bindings_stay_frozen() {
    let shared = Arc::new(Shared::new(1));
    let (session, _, pane) = switch_test_session(&shared, "work");
    let (victim, _, _) = switch_test_session(&shared, "victim");
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("viewer".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("attach-session", ["-r", "-t", "work"]),
        )
        .expect("read-only attach");
    take_reliable_messages(&mailbox);
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                ["-s", "command-alias[40]", "kill-session=display-message \\"],
            ),
        )
        .expect("unparsable destructive shadow");

    assert!(matches!(
        shared.execute_command_request(
            client,
            ClientKind::Interactive,
            &mut context,
            41,
            &CommandInvocation::new("kill-session", ["-t", "victim"]),
        ),
        CommandResponse::Error {
            error: ServerError::CommandParse(message),
            ..
        } if message == "unknown command: kill-session"
    ));
    assert!(
        shared
            .inner
            .lock()
            .engine
            .state
            .sessions
            .contains_key(&victim)
    );
    assert_eq!(
        client_attached_session(&shared.inner.lock(), client),
        Some(session)
    );

    let error = shared
        .inner
        .lock()
        .engine
        .prepare_callback_commands("kill-session -t victim", false, "bind-key")
        .expect_err("unsupported alias blocks binding construction");
    assert_eq!(
        error,
        ServerError::CommandParse("unknown command: kill-session".to_owned())
    );
    assert!(
        shared
            .inner
            .lock()
            .engine
            .state
            .sessions
            .contains_key(&victim)
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

    shared.inner.lock().client_flags.remove(client);
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                ["-s", "command-alias[41]", "kill-server=display-message \\"],
            ),
        )
        .expect("unparsable server shadow");
    assert!(matches!(
        shared.execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("kill-server", [] as [&str; 0]),
        ),
        Err(DaemonError::Server(ServerError::CommandParse(message)))
            if message == "unknown command: kill-server"
    ));
    assert!(!shared.stopping.load(Ordering::Acquire));

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-s", "-u", "command-alias[40]"]),
        )
        .expect("clear first shadow");
    let (dynamic_victim, _, _) = switch_test_session(&shared, "dynamic-victim");
    let commands = shared
        .inner
        .lock()
        .engine
        .prepare_callback_commands(
            "set-option -s command-alias[40] kill-session= ; kill-session -t dynamic-victim",
            false,
            "bind-key",
        )
        .expect("construct binding before alias mutation");
    shared
        .execute_key_commands(
            client,
            ClientKind::Interactive,
            &mut context,
            pane,
            &commands,
            false,
        )
        .expect("constructed binding ignores later shadow");
    assert!(
        !shared
            .inner
            .lock()
            .engine
            .state
            .sessions
            .contains_key(&dynamic_victim)
    );
}

#[test]
fn read_only_key_bindings_authorize_the_expanded_alias_once() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, pane) = switch_test_session(&shared, "A");
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
    let unsafe_commands = shared
        .inner
        .lock()
        .engine
        .prepare_callback_commands("a", false, "bind-key")
        .expect("construct unsafe binding");
    shared
        .execute_key_commands(
            client,
            ClientKind::Interactive,
            &mut context,
            pane,
            &unsafe_commands,
            false,
        )
        .expect("blocked binding");
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
            &CommandInvocation::new("set-option", ["-s", "command-alias[40]", "a=detach-client"]),
        )
        .expect("safe alias");
    let safe_commands = shared
        .inner
        .lock()
        .engine
        .prepare_callback_commands("a", false, "bind-key")
        .expect("construct safe binding");
    shared
        .execute_key_commands(
            client,
            ClientKind::Interactive,
            &mut context,
            pane,
            &safe_commands,
            false,
        )
        .expect("allowed binding");
    assert_eq!(client_attached_session(&shared.inner.lock(), client), None);
}

#[test]
fn key_binding_commands_keep_construction_time_aliases() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, pane) = switch_test_session(&shared, "A");
    let (b, _, _) = switch_test_session(&shared, "B");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("writer".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(client, a).expect("attach");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                ["-s", "command-alias[40]", "dynamic=kill-session -t B"],
            ),
        )
        .expect("initial alias");
    let commands = shared
        .inner
        .lock()
        .engine
        .prepare_callback_commands("dynamic", false, "bind-key")
        .expect("construct binding");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                ["-s", "command-alias[40]", "dynamic=display-message changed"],
            ),
        )
        .expect("change alias");
    shared
        .execute_key_commands(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            pane,
            &commands,
            false,
        )
        .expect("frozen binding");
    assert!(!shared.inner.lock().engine.state.sessions.contains_key(&b));
}

#[test]
fn key_binding_command_groups_continue_after_a_failed_physical_line() {
    let shared = Arc::new(Shared::new(1));
    let (session, _, pane) = switch_test_session(&shared, "grouped");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("writer".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(client, session).expect("attach");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("binding context");

    let first_group = SourceSpan {
        source: "<bind-key>".to_owned(),
        line: 1,
        column: 1,
    };
    let second_group = SourceSpan {
        source: "<bind-key>".to_owned(),
        line: 2,
        column: 1,
    };
    shared
        .execute_key_commands(
            client,
            ClientKind::Interactive,
            &mut context,
            pane,
            &[
                CommandInvocation::new("kill-pane", ["-t", "missing"])
                    .with_source(first_group.clone()),
                CommandInvocation::new("set-environment", ["-g", "BIND_GROUP_CONTINUED", "yes"])
                    .with_source(second_group),
            ],
            false,
        )
        .expect_err("first physical group fails");
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("BIND_GROUP_CONTINUED")
            .as_deref(),
        Some("yes")
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-environment", ["-g", "-u", "BIND_GROUP_CONTINUED"]),
        )
        .expect("clear group marker");
    shared
        .execute_key_commands(
            client,
            ClientKind::Interactive,
            &mut context,
            pane,
            &[
                CommandInvocation::new("kill-pane", ["-t", "missing"])
                    .with_source(first_group.clone()),
                CommandInvocation::new("set-environment", ["-g", "BIND_GROUP_CONTINUED", "no"])
                    .with_source(first_group),
            ],
            false,
        )
        .expect_err("one command group fails");
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("BIND_GROUP_CONTINUED"),
        None
    );
}

#[test]
fn key_binding_command_exits_are_queue_status_not_input_errors() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let missing = directory.path().join("missing.conf");
    let shared = Arc::new(Shared::new(1));
    let (session, _, pane) = switch_test_session(&shared, "binding-command-exit");
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("writer".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(client, session).expect("attach");
    take_reliable_messages(&mailbox);
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("binding context");
    let first_group = SourceSpan {
        source: "<bind-key>".to_owned(),
        line: 1,
        column: 1,
    };
    let second_group = SourceSpan {
        source: "<bind-key>".to_owned(),
        line: 2,
        column: 1,
    };
    let third_group = SourceSpan {
        source: "<bind-key>".to_owned(),
        line: 3,
        column: 1,
    };

    shared
        .execute_key_commands(
            client,
            ClientKind::Interactive,
            &mut context,
            pane,
            &[
                CommandInvocation::new("source-file", [missing.display().to_string()])
                    .with_source(first_group.clone()),
                CommandInvocation::new(
                    "set-environment",
                    ["-g", "BIND_COMMAND_EXIT_SAME_GROUP", "no"],
                )
                .with_source(first_group.clone()),
                CommandInvocation::new(
                    "set-environment",
                    ["-g", "BIND_COMMAND_EXIT_NEXT_GROUP", "yes"],
                )
                .with_source(second_group.clone()),
            ],
            false,
        )
        .expect("command exit stays inside the key queue");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner
                .engine
                .global_environment_variable("BIND_COMMAND_EXIT_SAME_GROUP"),
            None
        );
        assert_eq!(
            inner
                .engine
                .global_environment_variable("BIND_COMMAND_EXIT_NEXT_GROUP"),
            Some("yes".to_owned())
        );
    }
    assert!(take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientMessage {
                    kind: ClientMessageKind::Warning,
                    text,
                    ..
                },
                ..
            }) if text == &missing_source_error(&missing)
        )
    }));

    let error = shared
        .execute_key_commands(
            client,
            ClientKind::Interactive,
            &mut context,
            pane,
            &[
                CommandInvocation::new("source-file", [missing.display().to_string()])
                    .with_source(first_group),
                CommandInvocation::new("kill-pane", ["-t", "missing"]).with_source(second_group),
                CommandInvocation::new(
                    "set-environment",
                    ["-g", "BIND_COMMAND_EXIT_AFTER_ERROR", "yes"],
                )
                .with_source(third_group),
            ],
            false,
        )
        .expect_err("later server error escapes the key queue");
    assert_eq!(daemon_error_text(&error), "can't find pane: missing");
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("BIND_COMMAND_EXIT_AFTER_ERROR"),
        Some("yes".to_owned())
    );
}
