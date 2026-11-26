use super::*;

#[test]
fn customize_quit_fires_the_mode_exit_hooks_without_further_input() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "customize-quit", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("customize-mode", ["-t", &pane.to_string()]),
        )
        .expect("enter customize mode");
    let Some(PaneModeRequest::Customize(mode)) = shared
        .inner
        .lock()
        .modes
        .pane_modes
        .get(&pane)
        .and_then(|modes| modes.last())
        .cloned()
    else {
        panic!("customize mode is on top");
    };
    shared.apply_customize_result(
        client,
        &mut context,
        pane,
        mode,
        &CustomizeResult {
            close: true,
            commands: Vec::new(),
            menu: None,
            edit: None,
            stop_on_error: false,
            message: None,
            remembered: None,
        },
    );
    let inner = shared.inner.lock();
    assert!(!inner.modes.pane_modes.contains_key(&pane));
    assert!(inner.modes.pane_mode_transitions.is_empty());
}

#[test]
fn binding_errors_leave_a_native_clients_terminals_painting() {
    for native in [false, true] {
        let shared = Arc::new(Shared::new(1));
        let mailbox = OutboundMailbox::new();
        let (client, _) =
            shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
        if native {
            shared.inner.lock().client_entry(client).native_chooser = true;
        }
        let (_, pane, _) = attached_message_fixture(&shared, "binding-error", &[client]);
        let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
            .expect("pane context");
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("set-option", ["-g", "display-time", "0"]),
            )
            .expect("display-time");
        let error = shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("kill-pane", ["-t", "missing"]),
            )
            .expect_err("a missing pane");
        take_reliable_messages(&mailbox);
        shared.publish_background_command_error(client, &context, &error, true);
        let shown = take_reliable_messages(&mailbox)
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload:
                        EventPayload::TimedClientMessage {
                            text, duration_ms, ..
                        },
                    ..
                }) => Some((text, duration_ms)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(shown, [("Can't find pane: missing".to_owned(), 0)]);
        let inner = shared.inner.lock();
        assert!(inner.client(client).is_some_and(|c| c.message.is_some()));
        assert_eq!(client_terminal_publication_frozen(&inner, client), !native);
    }
}

#[test]
fn customize_errors_are_status_messages_that_fire_no_display_hooks_and_answers_are_history() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "customize-error", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    for command in [
        CommandInvocation::new(
            "set-hook",
            ["-g", "after-display-message", "set -g @shown y"],
        ),
        CommandInvocation::new("customize-mode", ["-t", &pane.to_string()]),
    ] {
        shared
            .execute(client, ClientKind::Interactive, &mut context, &command)
            .expect("setup");
    }
    let Some(PaneModeRequest::Customize(mode)) = shared
        .inner
        .lock()
        .modes
        .pane_modes
        .get(&pane)
        .and_then(|modes| modes.last())
        .cloned()
    else {
        panic!("customize mode is on top");
    };
    take_reliable_messages(&mailbox);
    shared.apply_customize_result(
        client,
        &mut context,
        pane,
        mode,
        &CustomizeResult {
            close: false,
            commands: vec![CommandInvocation::new(
                "bind-key",
                ["-T", "root", "C-a", "nosuchcommand"],
            )],
            menu: None,
            edit: None,
            stop_on_error: false,
            message: Some("Unknown key: Nope".to_owned()),
            remembered: Some((CommandPromptType::Command, "C-a nosuchcommand".to_owned())),
        },
    );
    assert_eq!(
        shared.inner.lock().history.command_history,
        ["C-a nosuchcommand".to_owned()]
    );
    let shown = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::TimedClientMessage {
                        kind: ClientMessageKind::Error,
                        text,
                        duration_ms,
                        ..
                    },
                ..
            }) => Some((text, duration_ms)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        shown,
        [
            ("Unknown key: Nope".to_owned(), 750),
            ("Unknown command: nosuchcommand".to_owned(), 750)
        ]
    );
    let hook = shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("show-options", ["-gqv", "@shown"]),
        )
        .expect("show-options");
    assert_eq!(hook.output, "");
}

#[test]
fn customize_stop_on_error_results_skip_the_commands_after_a_failure() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "customize-chain", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    for command in [
        CommandInvocation::new(
            "set-hook",
            ["-g", "after-new-window", "display-message kept"],
        ),
        CommandInvocation::new("customize-mode", ["-t", &pane.to_string()]),
    ] {
        shared
            .execute(client, ClientKind::Interactive, &mut context, &command)
            .expect("setup");
    }
    let Some(PaneModeRequest::Customize(mode)) = shared
        .inner
        .lock()
        .modes
        .pane_modes
        .get(&pane)
        .and_then(|modes| modes.last())
        .cloned()
    else {
        panic!("customize mode is on top");
    };
    let commands = vec![
        CommandInvocation::new("set-option", ["-g", "after-new-window[4294967296]", "x"]),
        CommandInvocation::new("set-option", ["-u", "-g", "after-new-window[0]"]),
    ];
    for (stop_on_error, kept) in [(true, true), (false, false)] {
        shared.apply_customize_result(
            client,
            &mut context,
            pane,
            mode.clone(),
            &CustomizeResult {
                close: false,
                commands: commands.clone(),
                menu: None,
                edit: None,
                stop_on_error,
                message: None,
                remembered: None,
            },
        );
        let output = shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("show-hooks", ["-g", "after-new-window"]),
            )
            .expect("show-hooks");
        assert_eq!(
            output.output == "after-new-window[0] display-message kept",
            kept,
            "{}",
            output.output
        );
    }
}

#[test]
fn choose_tree_model_flattens_mixed_panes_and_supports_collapse_and_search() {
    let mut engine = MuxEngine::default();
    let (session, window, terminal) = engine.state.create_session("work").expect("session");
    let browser = engine
        .state
        .split_pane(
            terminal,
            zz_protocol::Axis::Horizontal,
            PaneKind::Browser(zz_protocol::BrowserDescriptor::single(
                "https://example.com/docs".to_owned(),
                "default".to_owned(),
            )),
        )
        .expect("browser pane");
    let facts = FormatHookFacts::default();
    let mut chooser = ChooseTreeSession::new(
        ChooseTreeKind::Panes,
        false,
        Vec::new(),
        false,
        terminal,
        &engine,
        Some(session),
        &facts,
        None,
        None,
        false,
        false,
        false,
        TmuxSort::parse(None, false, Some(TmuxSortOrder::Index)).unwrap(),
        None,
    )
    .expect("chooser");

    assert_eq!(chooser.rendered.items.len(), 4);
    assert!(chooser.rendered.items.iter().any(|item| {
        item.target == ChooseTreeTarget::Pane(browser)
            && item.pane_kind == Some(ChooseTreePaneKind::Browser)
    }));
    assert_eq!(
        chooser.rendered.items[usize::try_from(chooser.rendered.selected).unwrap()].target,
        ChooseTreeTarget::Pane(terminal)
    );

    chooser
        .apply(ChooseTreeAction::Select(0), &engine, Some(session), &facts)
        .expect("select session");
    chooser
        .apply(ChooseTreeAction::Collapse, &engine, Some(session), &facts)
        .expect("collapse session");
    assert_eq!(chooser.rendered.items.len(), 1);
    assert_eq!(
        chooser.rendered.items[0].target,
        ChooseTreeTarget::Session(session)
    );

    chooser
        .apply(ChooseTreeAction::Expand, &engine, Some(session), &facts)
        .expect("expand session");
    chooser
        .apply(ChooseTreeAction::Select(1), &engine, Some(session), &facts)
        .expect("select window");
    chooser
        .apply(ChooseTreeAction::Expand, &engine, Some(session), &facts)
        .expect("expand window");
    chooser
        .apply(
            ChooseTreeAction::SearchStart { reverse: false },
            &engine,
            Some(session),
            &facts,
        )
        .expect("start search");
    chooser
        .apply(
            ChooseTreeAction::SearchAppend("example.com".to_owned()),
            &engine,
            Some(session),
            &facts,
        )
        .expect("search browser");
    assert_eq!(chooser.selected, Some(ChooseTreeTarget::Pane(browser)));
    assert_eq!(engine.state.windows[&window].panes.len(), 2);
}

#[test]
fn choose_buffer_model_bounds_metadata_and_searches_server_side_contents() {
    let mut engine = MuxEngine::default();
    let (session, _, pane) = engine.state.create_session("work").expect("session");
    let buffers = vec![
        PasteBuffer {
            name: "newest".to_owned(),
            data: Arc::from(b"first line\nsecond line".as_slice()),
            created: UNIX_EPOCH + Duration::from_secs(20),
            automatic: false,
        },
        PasteBuffer {
            name: "older".to_owned(),
            data: Arc::from(
                format!(
                    "{}hidden needle",
                    "x".repeat(MAX_CHOOSE_BUFFER_PREVIEW_BYTES)
                )
                .into_bytes(),
            ),
            created: UNIX_EPOCH + Duration::from_secs(10),
            automatic: false,
        },
    ];
    let mut chooser = ChooseBufferSession::new(
        pane,
        &engine,
        &buffers,
        Some(session),
        &FormatHookFacts::default(),
        None,
        None,
        false,
        TmuxSort::parse(None, false, Some(TmuxSortOrder::Creation)).unwrap(),
        None,
    )
    .expect("valid pane")
    .expect("nonempty chooser");

    assert_eq!(chooser.rendered.items.len(), 2);
    assert_eq!(chooser.rendered.items[0].name, "newest");
    assert!(chooser.rendered.items[0].preview.contains('⏎'));
    assert!(
        chooser.rendered.items[1].preview.len() <= MAX_CHOOSE_BUFFER_PREVIEW_BYTES,
        "wire previews must remain bounded"
    );
    chooser
        .apply(ChooseBufferAction::SearchStart { reverse: false }, &buffers)
        .expect("start search");
    chooser
        .apply(
            ChooseBufferAction::SearchAppend("NEEDLE".to_owned()),
            &buffers,
        )
        .expect("search full buffer");
    assert_eq!(chooser.selected.as_deref(), Some("older"));
    assert!(matches!(
        chooser
            .apply(ChooseBufferAction::Paste, &buffers)
            .expect("paste selected"),
        ChooseBufferResult::Paste(names) if names == ["older"]
    ));
}

#[test]
fn chooser_templates_substitute_targets_and_resolve_aliases_at_selection() {
    let (shared, client, mailbox, mut context) = popup_test_workspace("chooser-templates");
    let source_session = context.session.expect("source session");
    let source_window = context.window.expect("source window");
    let source_pane = context.pane.expect("source pane");
    let (target_session, _, target_pane) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("target-session")
        .expect("target session");
    let run = |context: &mut ExecutionContext, command: CommandInvocation| {
        shared
            .execute(client, ClientKind::Interactive, context, &command)
            .expect("chooser command")
    };
    let select_buffer = |context: &mut ExecutionContext| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::ChooseBuffer {
                    action: ChooseBufferAction::Paste,
                },
            )
            .expect("select chooser buffer");
    };

    {
        let inner = shared.inner.lock();
        assert_eq!(
            choose_tree_command_target(
                &inner.engine,
                &[],
                ChooseTreeTarget::Session(source_session)
            )
            .unwrap(),
            "=popup-test:"
        );
        assert_eq!(
            choose_tree_command_target(
                &inner.engine,
                &[],
                ChooseTreeTarget::Window(source_window),
            )
            .unwrap(),
            "=popup-test:0."
        );
        assert_eq!(
            choose_tree_command_target(&inner.engine, &[], ChooseTreeTarget::Pane(source_pane))
                .unwrap(),
            format!("=popup-test:0.{source_pane}")
        );
    }

    let set_alias = |value: &str| {
        shared
            .inner
            .lock()
            .engine
            .execute(
                &mut ExecutionContext::default(),
                &CommandInvocation::new("set-option", ["-s", "command-alias[98]", value]),
            )
            .expect("set chooser alias");
    };
    run(
        &mut context,
        CommandInvocation::new("set-buffer", ["-b", "picked", "payload"]),
    );

    set_alias("pick10j=set-environment -g CHOOSER_TYPED");
    run(
        &mut context,
        CommandInvocation::new("choose-buffer", ["{ pick10j %% }"]).with_command_blocks([0]),
    );
    set_alias("pick10j=set-environment -g CHOOSER_REPLACED");
    select_buffer(&mut context);
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.engine.global_environment_variable("CHOOSER_TYPED"),
            Some("picked".to_owned())
        );
        assert_eq!(
            inner.engine.global_environment_variable("CHOOSER_REPLACED"),
            None
        );
    }

    set_alias("pick10j=set-environment -g CHOOSER_OLD");
    run(
        &mut context,
        CommandInvocation::new("choose-buffer", ["pick10j %%"]),
    );
    set_alias("pick10j=set-environment -g CHOOSER_FRESH");
    select_buffer(&mut context);
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.engine.global_environment_variable("CHOOSER_FRESH"),
            Some("picked".to_owned())
        );
        assert_eq!(
            inner.engine.global_environment_variable("CHOOSER_OLD"),
            None
        );
    }

    run(
        &mut context,
        CommandInvocation::new(
            "choose-tree",
            ["-s", "-O", "index", "set-option -p @TREE %1"],
        ),
    );
    let target_row = shared.inner.lock().clients[&client]
        .choose_tree
        .as_ref()
        .unwrap()
        .rendered
        .items
        .iter()
        .position(|item| item.target == ChooseTreeTarget::Session(target_session))
        .and_then(|row| u32::try_from(row).ok())
        .expect("target row");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ChooseTree {
                action: ChooseTreeAction::ActivateIndex(target_row),
            },
        )
        .expect("run tree template");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), client),
        Some(source_session)
    );
    let source_pane = source_pane.to_string();
    let target_pane = target_pane.to_string();
    assert_eq!(
        run(
            &mut context,
            CommandInvocation::new("display-message", ["-p", "-t", &source_pane, "#{@TREE}"],),
        )
        .output,
        "=target-session:"
    );
    assert_eq!(
        run(
            &mut context,
            CommandInvocation::new("display-message", ["-p", "-t", &target_pane, "#{@TREE}"],),
        )
        .output,
        "\n"
    );

    take_reliable_messages(&mailbox);
    run(
        &mut context,
        CommandInvocation::new("choose-buffer", ["unknownchooser %%"]),
    );
    take_reliable_messages(&mailbox);
    select_buffer(&mut context);
    assert!(take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::TimedClientMessage {
                    kind: ClientMessageKind::Error,
                    text,
                    ..
                },
                ..
            }) if text == "Unknown command: unknownchooser"
        )
    }));

    run(
        &mut context,
        CommandInvocation::new("choose-buffer", ["set-environment -g CHOOSER_STALE %%"]),
    );
    shared.inner.lock().buffers.paste_buffers.clear();
    select_buffer(&mut context);
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("CHOOSER_STALE"),
        None
    );
}

#[test]
fn chooser_rows_wear_the_pin_key_ladder_and_run_out_at_thirty_six() {
    let mut engine = MuxEngine::default();
    let (first, _, first_pane) = engine.state.create_session("s00").expect("first session");
    for index in 1..40 {
        engine
            .state
            .create_session(format!("s{index:02}"))
            .expect("session");
    }
    let facts = FormatHookFacts::default();
    let chooser = ChooseTreeSession::new(
        ChooseTreeKind::Windows,
        false,
        Vec::new(),
        true,
        first_pane,
        &engine,
        Some(first),
        &facts,
        None,
        None,
        false,
        false,
        false,
        TmuxSort::parse(None, false, Some(TmuxSortOrder::Name)).unwrap(),
        None,
    )
    .expect("session chooser");
    let keys = chooser
        .rendered
        .items
        .iter()
        .map(|item| item.key.as_str())
        .collect::<Vec<_>>();

    assert_eq!(keys.len(), 40);
    assert_eq!(
        &keys[..10],
        ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]
    );
    assert_eq!(keys[10], "M-a");
    assert_eq!(keys[35], "M-z");
    assert!(
        keys[36..].iter().all(|key| key.is_empty()),
        "the pin stops assigning keys past row 36"
    );

    for (row, key) in keys.iter().enumerate() {
        if key.is_empty() {
            continue;
        }
        assert_eq!(
            chooser_row_for_key(
                &chooser.rendered.items,
                &chooser_key_press(key),
                false,
                |item| { &item.key }
            ),
            u32::try_from(row).ok(),
            "row {row} must answer to {key}"
        );
    }
    assert_eq!(
        chooser_row_for_key(
            &chooser.rendered.items,
            &chooser_key_press("0"),
            true,
            |item| { &item.key }
        ),
        None,
        "a chooser mid-search keeps typing into its query"
    );
}

#[test]
fn chooser_key_formats_expand_per_row_and_drop_unpressable_keys() {
    let mut engine = MuxEngine::default();
    let (first, _, first_pane) = engine.state.create_session("alpha").expect("first session");
    for name in ["beta", "gamma"] {
        engine.state.create_session(name).expect("session");
    }
    let facts = FormatHookFacts::default();
    let keys = |format: &str| {
        ChooseTreeSession::new(
            ChooseTreeKind::Windows,
            false,
            Vec::new(),
            true,
            first_pane,
            &engine,
            Some(first),
            &facts,
            None,
            None,
            false,
            false,
            false,
            TmuxSort::parse(None, false, Some(TmuxSortOrder::Name)).unwrap(),
            Some(format.to_owned()),
        )
        .expect("session chooser")
        .rendered
        .items
        .into_iter()
        .map(|item| item.key)
        .collect::<Vec<_>>()
    };

    assert_eq!(keys("#{line}"), ["0", "1", "2"]);
    assert_eq!(keys("x"), ["x", "x", "x"]);
    assert_eq!(keys("notakey"), ["", "", ""]);
    assert_eq!(keys(""), ["", "", ""]);
    assert_eq!(keys("C-a"), ["C-a", "C-a", "C-a"]);
    assert_eq!(keys("Alt-b"), ["M-b", "M-b", "M-b"]);
    assert_eq!(
        keys("#{?#{==:#{session_name},beta},z,}"),
        ["", "z", ""],
        "a key format expands in each row's own context"
    );
}

#[test]
fn a_duplicate_row_key_activates_the_first_row_that_holds_it() {
    let mut engine = MuxEngine::default();
    let (first, _, first_pane) = engine.state.create_session("alpha").expect("first session");
    for name in ["beta", "gamma"] {
        engine.state.create_session(name).expect("session");
    }
    let facts = FormatHookFacts::default();
    let mut chooser = ChooseTreeSession::new(
        ChooseTreeKind::Windows,
        false,
        Vec::new(),
        true,
        first_pane,
        &engine,
        Some(first),
        &facts,
        None,
        None,
        false,
        false,
        false,
        TmuxSort::parse(None, false, Some(TmuxSortOrder::Name)).unwrap(),
        Some("x".to_owned()),
    )
    .expect("session chooser");
    chooser
        .apply(ChooseTreeAction::Next, &engine, Some(first), &facts)
        .expect("move off the first row");
    assert_eq!(chooser.rendered.selected, 1);

    let row = chooser_row_for_key(
        &chooser.rendered.items,
        &chooser_key_press("x"),
        false,
        |item| &item.key,
    )
    .expect("a duplicate key still claims a row");
    assert_eq!(row, 0, "mode_tree_key breaks on the first match");
    assert!(matches!(
        chooser
            .apply(
                ChooseTreeAction::ActivateIndex(row),
                &engine,
                Some(first),
                &facts
            )
            .expect("activate the keyed row"),
        ChooseTreeResult::Activate(target) if target == ChooseTreeTarget::Session(first)
    ));
}

#[test]
fn buffer_chooser_rows_carry_the_same_key_ladder() {
    let mut engine = MuxEngine::default();
    let (session, _, pane) = engine.state.create_session("work").expect("session");
    let buffers = (0..12)
        .map(|index| PasteBuffer {
            name: format!("buffer{index}"),
            data: Arc::from(format!("payload {index}").into_bytes()),
            created: UNIX_EPOCH + Duration::from_secs(100 + index),
            automatic: false,
        })
        .collect::<Vec<_>>();
    let facts = FormatHookFacts::default();
    let keys = |key_format: Option<&str>| {
        ChooseBufferSession::new(
            pane,
            &engine,
            &buffers,
            Some(session),
            &facts,
            None,
            None,
            false,
            TmuxSort::parse(None, false, Some(TmuxSortOrder::Name)).unwrap(),
            key_format.map(str::to_owned),
        )
        .expect("valid pane")
        .expect("nonempty chooser")
        .rendered
        .items
        .into_iter()
        .map(|item| item.key)
        .collect::<Vec<_>>()
    };

    let stock = keys(None);
    assert_eq!(
        &stock[..10],
        ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]
    );
    assert_eq!(&stock[10..], ["M-a", "M-b"]);
    assert_eq!(
        keys(Some("#{?#{==:#{buffer_name},buffer3},q,}"))
            .iter()
            .filter(|key| !key.is_empty())
            .count(),
        1,
        "a buffer key format sees each row's own buffer"
    );
    assert!(keys(Some("nope")).iter().all(String::is_empty));
}

#[test]
fn chooser_specs_sort_filter_and_fall_back_when_filters_match_nothing() {
    let mut engine = MuxEngine::default();
    let (z, z_window, z_pane) = engine.state.create_session("z").expect("z session");
    let (a, _, _) = engine.state.create_session("a").expect("a session");
    let facts = FormatHookFacts::default();
    let session_state = |filter: Option<&str>, reversed: bool| {
        ChooseTreeSession::new(
            ChooseTreeKind::Windows,
            false,
            Vec::new(),
            true,
            z_pane,
            &engine,
            Some(z),
            &facts,
            filter.map(str::to_owned),
            None,
            false,
            false,
            false,
            TmuxSort::parse(None, reversed, Some(TmuxSortOrder::Index)).unwrap(),
            None,
        )
        .expect("session chooser")
        .rendered
    };
    let unfiltered = session_state(None, false);
    assert!(!unfiltered.filter_no_matches);
    assert_eq!(
        unfiltered
            .items
            .into_iter()
            .map(|item| item.target)
            .collect::<Vec<_>>(),
        [ChooseTreeTarget::Session(z), ChooseTreeTarget::Session(a)]
    );
    let reversed = session_state(None, true);
    assert!(!reversed.filter_no_matches);
    assert_eq!(
        reversed
            .items
            .into_iter()
            .map(|item| item.target)
            .collect::<Vec<_>>(),
        [ChooseTreeTarget::Session(a), ChooseTreeTarget::Session(z)]
    );
    let matched = session_state(Some("#{==:#{session_name},a}"), false);
    assert!(!matched.filter_no_matches);
    assert_eq!(
        matched
            .items
            .into_iter()
            .map(|item| item.target)
            .collect::<Vec<_>>(),
        [ChooseTreeTarget::Session(a)]
    );
    let fallback = session_state(Some("#{==:#{session_name},missing}"), false);
    assert!(fallback.filter_no_matches);
    assert_eq!(
        fallback
            .items
            .into_iter()
            .map(|item| item.target)
            .collect::<Vec<_>>(),
        [ChooseTreeTarget::Session(z), ChooseTreeTarget::Session(a)]
    );
    let mut collapsed_sessions = ChooseTreeSession::new(
        ChooseTreeKind::Windows,
        false,
        Vec::new(),
        true,
        z_pane,
        &engine,
        Some(z),
        &facts,
        None,
        None,
        false,
        false,
        false,
        TmuxSort::parse(None, false, Some(TmuxSortOrder::Index)).unwrap(),
        None,
    )
    .expect("collapsed session chooser");
    collapsed_sessions
        .apply(ChooseTreeAction::Expand, &engine, Some(z), &facts)
        .expect("expand selected session");
    assert!(
        collapsed_sessions
            .rendered
            .items
            .iter()
            .any(|item| item.target == ChooseTreeTarget::Window(z_window) && item.depth == 1)
    );

    let mut collapsed_windows = ChooseTreeSession::new(
        ChooseTreeKind::Windows,
        false,
        Vec::new(),
        false,
        z_pane,
        &engine,
        Some(z),
        &facts,
        None,
        None,
        false,
        false,
        false,
        TmuxSort::parse(None, false, Some(TmuxSortOrder::Index)).unwrap(),
        None,
    )
    .expect("collapsed window chooser");
    assert_eq!(
        collapsed_windows.selected,
        Some(ChooseTreeTarget::Window(z_window))
    );
    collapsed_windows
        .apply(ChooseTreeAction::Expand, &engine, Some(z), &facts)
        .expect("expand selected window");
    assert!(
        collapsed_windows
            .rendered
            .items
            .iter()
            .any(|item| item.target == ChooseTreeTarget::Pane(z_pane) && item.depth == 2)
    );

    let default_tree = ChooseTreeSession::new(
        ChooseTreeKind::Panes,
        false,
        Vec::new(),
        false,
        z_pane,
        &engine,
        Some(z),
        &facts,
        None,
        None,
        false,
        false,
        false,
        TmuxSort::parse(None, false, Some(TmuxSortOrder::Index)).unwrap(),
        None,
    )
    .expect("default chooser");
    assert_eq!(
        default_tree.selected,
        Some(ChooseTreeTarget::Window(z_window))
    );
    assert!(!default_tree.rendered.filter_no_matches);

    let mut rebuilt_tree = ChooseTreeSession::new(
        ChooseTreeKind::Windows,
        false,
        Vec::new(),
        true,
        z_pane,
        &engine,
        Some(z),
        &facts,
        Some("#{==:#{session_name},missing}".to_owned()),
        None,
        false,
        false,
        false,
        TmuxSort::parse(None, false, Some(TmuxSortOrder::Index)).unwrap(),
        None,
    )
    .expect("fallback tree chooser");
    assert!(rebuilt_tree.rendered.filter_no_matches);
    let (missing, _, _) = engine
        .state
        .create_session("missing")
        .expect("matching session");
    rebuilt_tree.rebuild(&engine, Some(z), &facts);
    assert!(!rebuilt_tree.rendered.filter_no_matches);
    let missing_window = engine.state.sessions[&missing].active_window;
    let missing_pane = engine.state.windows[&missing_window].active_pane;
    assert_eq!(
        rebuilt_tree
            .rendered
            .items
            .iter()
            .map(|item| item.target)
            .collect::<Vec<_>>(),
        [
            ChooseTreeTarget::Session(missing),
            ChooseTreeTarget::Window(missing_window),
            ChooseTreeTarget::Pane(missing_pane),
        ]
    );
    engine
        .state
        .kill_session(missing)
        .expect("remove matching session");
    rebuilt_tree.rebuild(&engine, Some(z), &facts);
    assert!(rebuilt_tree.rendered.filter_no_matches);

    let mut buffers = vec![
        PasteBuffer {
            name: "newest".to_owned(),
            data: Arc::from(b"new".as_slice()),
            created: UNIX_EPOCH + Duration::from_secs(20),
            automatic: false,
        },
        PasteBuffer {
            name: "older".to_owned(),
            data: Arc::from(b"old".as_slice()),
            created: UNIX_EPOCH + Duration::from_secs(10),
            automatic: false,
        },
    ];
    let filtered = ChooseBufferSession::new(
        z_pane,
        &engine,
        &buffers,
        Some(z),
        &facts,
        Some("#{==:#{buffer_name},older}".to_owned()),
        None,
        false,
        TmuxSort::parse(None, true, Some(TmuxSortOrder::Creation)).unwrap(),
        None,
    )
    .expect("valid source")
    .expect("filtered chooser");
    assert_eq!(filtered.names, ["older"]);
    assert!(!filtered.rendered.filter_no_matches);

    let mut fallback = ChooseBufferSession::new(
        z_pane,
        &engine,
        &buffers,
        Some(z),
        &facts,
        Some("#{==:#{buffer_name},missing}".to_owned()),
        None,
        false,
        TmuxSort::parse(None, true, Some(TmuxSortOrder::Creation)).unwrap(),
        None,
    )
    .expect("valid source")
    .expect("fallback chooser");
    assert_eq!(fallback.names, ["older", "newest"]);
    assert_eq!(fallback.rendered.selected, 0);
    assert_eq!(fallback.selected.as_deref(), Some("older"));
    assert!(fallback.rendered.filter_no_matches);
    buffers.push(PasteBuffer {
        name: "missing".to_owned(),
        data: Arc::from(b"match".as_slice()),
        created: UNIX_EPOCH + Duration::from_secs(30),
        automatic: false,
    });
    fallback.rebuild(&engine, &buffers, Some(z), &facts);
    assert_eq!(fallback.names, ["missing"]);
    assert!(!fallback.rendered.filter_no_matches);
    buffers.pop();
    fallback.rebuild(&engine, &buffers, Some(z), &facts);
    assert!(fallback.rendered.filter_no_matches);
}

#[test]
fn daemon_choose_buffer_searches_pastes_and_deletes_across_native_surfaces() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "work"]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "browser", "https://example.com"]),
        )
        .expect("browser pane");
    let browser = context.pane.expect("browser pane");
    shared.attach(client, session).expect("attach session");
    {
        let mut inner = shared.inner.lock();
        insert_paste_buffer(
            &mut inner,
            Some("binary"),
            "buffer",
            b"a\nb\0\xff".to_vec(),
            true,
        )
        .expect("binary buffer");
    }
    take_reliable_messages(&mailbox);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("paste-buffer", ["-b", "binary", "-t", &browser.to_string()]),
        )
        .expect("safe binary browser paste");
    assert_eq!(take_browser_literals(&mailbox, browser), ["a\rb^@M^?"]);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "paste-buffer",
                ["-r", "-b", "binary", "-t", &browser.to_string()],
            ),
        )
        .expect("raw-newline browser paste");
    assert_eq!(take_browser_literals(&mailbox, browser), ["a\nb^@M^?"]);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "paste-buffer",
                ["-s", "::", "-b", "binary", "-t", &browser.to_string()],
            ),
        )
        .expect("custom-separator browser paste");
    assert_eq!(take_browser_literals(&mailbox, browser), ["a::b^@M^?"]);
    let error = shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "paste-buffer",
                ["-d", "-S", "-b", "binary", "-t", &browser.to_string()],
            ),
        )
        .expect_err("binary browser paste");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message.contains("not valid UTF-8 for a browser pane")
    ));
    assert!(
        shared
            .inner
            .lock()
            .buffers
            .paste_buffers
            .iter()
            .any(|buffer| buffer.name == "binary"),
        "a rejected delete-and-paste must leave the buffer intact"
    );
    assert!(
        !take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::BrowserCommand { .. },
                    ..
                })
            )),
        "a rejected binary paste must not partially reach Chromium"
    );
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("delete-buffer", ["-b", "binary"]),
        )
        .expect("delete binary fixture");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-buffer", ["-b", "older", "alpha payload"]),
        )
        .expect("older buffer");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-buffer", ["-b", "newest", "beta payload"]),
        )
        .expect("newer buffer");
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("choose-buffer", ["-Z"]),
        )
        .expect("open buffer chooser");
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ChooseBuffer { state: Some(state) },
                    ..
                }) if state.items.len() == 2 && state.items[0].name == "newest"
            ))
    );
    shared.send_resync(client, &mailbox);
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ChooseBuffer { state: Some(state) },
                    ..
                }) if state.items.len() == 2
            ))
    );
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane: browser,
                text: "must not leak".to_owned(),
            },
        )
        .expect("block covered browser input");
    assert!(
        !take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::BrowserCommand { .. },
                    ..
                })
            ))
    );
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ChooseBuffer {
                action: ChooseBufferAction::SearchStart { reverse: false },
            },
        )
        .expect("start search");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ChooseBuffer {
                action: ChooseBufferAction::SearchAppend("alpha".to_owned()),
            },
        )
        .expect("search older buffer");
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ChooseBufferUpdate { selected: 1, .. },
                    ..
                })
            ))
    );
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ChooseBuffer {
                action: ChooseBufferAction::Paste,
            },
        )
        .expect("paste into browser");
    let pasted = take_reliable_messages(&mailbox);
    assert!(pasted.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::ChooseBuffer { state: None },
            ..
        })
    )));
    assert!(pasted.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::BrowserCommand {
                pane,
                command: BrowserCommand::SendKeys(keys),
            },
            ..
        }) if *pane == browser
            && keys == &[zz_protocol::KeyToken::Literal("alpha payload".to_owned())]
    )));

    for remaining in [1, 0] {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("choose-buffer", ["-Z"]),
            )
            .expect("reopen buffer chooser");
        take_reliable_messages(&mailbox);
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::ChooseBuffer {
                    action: ChooseBufferAction::Delete,
                },
            )
            .expect("delete selected buffer");
        let messages = take_reliable_messages(&mailbox);
        assert!(messages.iter().any(|message| matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ChooseBuffer { state },
                ..
            }) if state.as_ref().map_or(0, |state| state.items.len()) == remaining
        )));
    }
    assert!(shared.inner.lock().buffers.paste_buffers.is_empty());
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.choose_buffer.is_none())));
}

#[test]
fn display_panes_model_uses_pane_order_and_tmux_selection_keys() {
    let mut engine = MuxEngine::default();
    let (session, window, source) = engine.state.create_session("work").expect("session");
    for _ in 1..=36 {
        engine
            .state
            .split_pane_with(
                source,
                zz_protocol::Axis::Horizontal,
                PaneKind::Terminal,
                zz_mux::SplitPlacement {
                    size: zz_mux::SplitSize::Cells(1),
                    ..zz_mux::SplitPlacement::default()
                },
            )
            .expect("split pane");
    }
    let active = engine.state.windows[&window].pane_order()[36];
    engine.state.select_pane(active).expect("select last pane");

    let (_, actual_window, overlay) = build_display_panes_state(
        &engine,
        "",
        &FormatHookFacts::default(),
        active,
        1_000,
        session,
    )
    .expect("pane indicators");
    assert_eq!(actual_window, window);
    assert_eq!(overlay.indicators.len(), 37);
    assert_eq!(overlay.indicators[0].select_key, b'0');
    assert_eq!(overlay.indicators[9].select_key, b'9');
    assert_eq!(overlay.indicators[10].select_key, b'a');
    assert_eq!(overlay.indicators[35].select_key, b'z');
    assert_eq!(overlay.indicators[36].select_key, 0);
    assert!(overlay.indicators[36].active());

    engine
        .execute(
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-window-option", ["-g", "pane-base-index", "1"]),
        )
        .expect("one-based pane indicators");
    let (_, _, overlay) = build_display_panes_state(
        &engine,
        "",
        &FormatHookFacts::default(),
        active,
        1_000,
        session,
    )
    .expect("one-based indicators");
    assert_eq!(overlay.indicators[0].index, 1);
    assert_eq!(overlay.indicators[0].select_key, b'1');
    assert_eq!(overlay.indicators[8].index, 9);
    assert_eq!(overlay.indicators[8].select_key, b'9');
    assert_eq!(overlay.indicators[9].index, 10);
    assert_eq!(overlay.indicators[9].select_key, b'a');
    assert_eq!(overlay.indicators[35].index, 36);
    assert_eq!(overlay.indicators[35].select_key, 0);
}

#[test]
fn daemon_native_split_resize_commits_exactly_and_rejects_stale_contexts() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "work", QUIET_PANE_COMMAND]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let window = context.window.expect("window");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-h", QUIET_PANE_COMMAND]),
        )
        .expect("split pane");
    let panes = shared.inner.lock().engine.state.windows[&window]
        .pane_order()
        .to_vec();
    wait_for_pane_runtime_facts(&shared, &panes);
    let split = {
        let inner = shared.inner.lock();
        let mut splits = Vec::new();
        inner.engine.state.windows[&window]
            .layout
            .project()
            .splits(&mut splits);
        splits[0]
    };
    shared.attach(client, session).expect("attach session");
    take_reliable_messages(&mailbox);

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeSplit {
                window,
                split,
                ratio_basis_points: 6_750,
            },
        )
        .expect("resize split");
    {
        let inner = shared.inner.lock();
        let LayoutNode::Split { ratio, .. } = inner.engine.state.windows[&window].layout.project()
        else {
            panic!("window should remain split");
        };
        assert!((ratio - 53.0 / 79.0).abs() < f32::EPSILON);
    }
    assert!(take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::Snapshot(snapshot),
                ..
            }) if matches!(
                snapshot.sessions[0].windows[0].layout,
                LayoutNode::Split { ratio, .. }
                    if (ratio - 53.0 / 79.0).abs() < f32::EPSILON
            )
        )
    }));

    let generation = shared.inner.lock().engine.state.generation();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeSplit {
                window,
                split,
                ratio_basis_points: 6_751,
            },
        )
        .expect("cell-snapped resize");
    assert_eq!(shared.inner.lock().engine.state.generation(), generation);
    assert!(take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::Snapshot(snapshot),
                ..
            }) if snapshot.generation == generation
        )
    }));

    let command_error = shared
        .input(
            client,
            ClientKind::Command,
            &mut context,
            InputMessage::ResizeSplit {
                window,
                split,
                ratio_basis_points: 5_000,
            },
        )
        .expect_err("command clients cannot drag");
    assert!(matches!(
        command_error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message.contains("interactive client")
    ));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("resize-pane", ["-Z"]),
        )
        .expect("zoom pane");
    let zoom_error = shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeSplit {
                window,
                split,
                ratio_basis_points: 5_000,
            },
        )
        .expect_err("zoomed windows cannot be dragged");
    assert!(matches!(
        zoom_error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message.contains("zoomed")
    ));
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("resize-pane", ["-Z"]),
        )
        .expect("unzoom pane");

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-window", [] as [&str; 0]),
        )
        .expect("new active window");
    let stale_error = shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeSplit {
                window,
                split,
                ratio_basis_points: 5_000,
            },
        )
        .expect_err("inactive windows cannot be dragged");
    assert!(matches!(
        stale_error,
        DaemonError::Server(ServerError::InvalidTarget(message))
            if message.contains("not active")
    ));
}

#[test]
fn daemon_zoom_subscribes_only_the_visible_terminal_surface() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "work"]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let first = context.pane.expect("first terminal");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-h"]),
        )
        .expect("second terminal");
    let second = context.pane.expect("second terminal");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "browser", "https://example.com"]),
        )
        .expect("browser pane");
    let browser = context.pane.expect("browser pane");
    shared.attach(client, session).expect("attach session");
    take_reliable_messages(&mailbox);
    assert_eq!(
        (*shared.inner.lock().clients[&client]
            .visible_terminals
            .as_ref()
            .unwrap()),
        BTreeSet::from([first, second])
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("resize-pane", ["-Z", "-t", &first.to_string()]),
        )
        .expect("zoom terminal");
    assert_eq!(
        (*shared.inner.lock().clients[&client]
            .visible_terminals
            .as_ref()
            .unwrap()),
        BTreeSet::from([first])
    );
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Snapshot(snapshot),
                    ..
                }) if snapshot.sessions[0].windows[0].zoomed_pane == Some(first)
            ))
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("resize-pane", ["-Z", "-t", &browser.to_string()]),
        )
        .expect("unzoom terminal");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("resize-pane", ["-Z", "-t", &browser.to_string()]),
        )
        .expect("zoom browser");
    assert!(
        shared.inner.lock().clients[&client]
            .visible_terminals
            .as_ref()
            .unwrap()
            .is_empty()
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("next-layout", [] as [&str; 0]),
        )
        .expect("apply a preset layout");
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.unwrap()].zoomed_pane,
        None
    );
    assert_eq!(
        (*shared.inner.lock().clients[&client]
            .visible_terminals
            .as_ref()
            .unwrap()),
        BTreeSet::from([first, second])
    );
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Snapshot(snapshot),
                    ..
                }) if matches!(
                    snapshot.sessions[0].windows[0].layout,
                    LayoutNode::Split {
                        axis: zz_protocol::Axis::Horizontal,
                        ..
                    }
                ) && snapshot.sessions[0].windows[0].zoomed_pane.is_none()
            ))
    );
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("resize-pane", ["-Z", "-t", &browser.to_string()]),
        )
        .expect("rezoom browser after layout selection");
    assert!(
        shared.inner.lock().clients[&client]
            .visible_terminals
            .as_ref()
            .unwrap()
            .is_empty()
    );

    let (_, _, display) = build_display_panes_state(
        &shared.inner.lock().engine,
        "",
        &FormatHookFacts::default(),
        browser,
        1_000,
        session,
    )
    .expect("zoomed display panes");
    assert_eq!(display.indicators.len(), 1);
    assert_eq!(display.indicators[0].pane, browser);
    assert_eq!(display.indicators[0].select_key, b'2');

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-panes", ["-d", "0"]),
        )
        .expect("display zoomed browser pane");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::DisplayPanes {
                action: DisplayPanesAction::Key(test_key(
                    KeyCode::Character('2'),
                    Modifiers::default(),
                    Some("2"),
                )),
            },
        )
        .expect("select displayed zoomed pane");
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.unwrap()].zoomed_pane,
        None
    );
    assert_eq!(
        (*shared.inner.lock().clients[&client]
            .visible_terminals
            .as_ref()
            .unwrap()),
        BTreeSet::from([first, second])
    );
}

#[test]
fn choose_tree_closes_when_its_source_pane_is_removed() {
    let shared = Shared::new(1);
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let session = {
        let mut inner = shared.inner.lock();
        let (session, _, source) = inner
            .engine
            .state
            .create_session("ephemeral")
            .expect("session");
        inner
            .attachment
            .attached
            .entry(session)
            .or_default()
            .insert(client);
        let facts = format_hook_facts(&inner);
        let chooser = ChooseTreeSession::new(
            ChooseTreeKind::Panes,
            false,
            Vec::new(),
            false,
            source,
            &inner.engine,
            Some(session),
            &facts,
            None,
            None,
            false,
            false,
            false,
            TmuxSort::parse(None, false, Some(TmuxSortOrder::Index)).unwrap(),
            None,
        )
        .expect("chooser");
        inner.client_entry(client).choose_tree.replace(chooser);
        session
    };
    shared
        .inner
        .lock()
        .engine
        .state
        .kill_session(session)
        .expect("remove source session");

    shared.refresh_choose_trees();

    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.choose_tree.is_none())));
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ChooseTree { state: None },
                    ..
                })
            ))
    );
}

#[test]
fn choose_tree_z_order_lists_panes_by_pane_z() {
    let shared = Shared::new(1);
    let mut inner = shared.inner.lock();
    let (session, _, first) = inner.engine.state.create_session("z").expect("session");
    let mut context =
        ExecutionContext::for_pane(&inner.engine.state, first).expect("first pane context");
    let mut floats = Vec::new();
    for _ in 0..2 {
        inner
            .engine
            .execute(
                &mut context,
                &CommandInvocation::new("new-pane", Vec::<String>::new()),
            )
            .expect("open a float");
        floats.push(context.pane.expect("float pane"));
    }
    let facts = format_hook_facts(&inner);
    let chooser = ChooseTreeSession::new(
        ChooseTreeKind::Panes,
        false,
        Vec::new(),
        false,
        first,
        &inner.engine,
        Some(session),
        &facts,
        None,
        None,
        false,
        false,
        false,
        TmuxSort::parse(Some("z"), false, None).unwrap(),
        None,
    )
    .expect("chooser");
    let panes = chooser
        .rendered
        .items
        .iter()
        .filter_map(|item| match item.target {
            ChooseTreeTarget::Pane(pane) => Some(pane),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(panes, [floats[1], floats[0], first]);
}

#[test]
fn daemon_focus_sidebar_stays_separate_from_session_and_window_choosers() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "work"]),
        )
        .expect("session");
    let session = context.session.expect("session id");
    shared.attach(client, session).expect("attach session");
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("focus-sidebar", [] as [&str; 0]),
        )
        .expect("focus sidebar");
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::FocusSidebar,
            ..
        })
    )));
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.choose_tree.is_none())));

    for (flag, sessions_only) in [("-Zs", true), ("-Zw", false)] {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("choose-tree", [flag]),
            )
            .expect("open chooser");
        let messages = take_reliable_messages(&mailbox);
        assert!(messages.iter().any(|message| matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ChooseTree { state: Some(_) },
                ..
            })
        )));
        assert!(!messages.iter().any(|message| matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::FocusSidebar,
                ..
            })
        )));
        let inner = shared.inner.lock();
        let chooser = inner
            .client(client)
            .and_then(|c| c.choose_tree.as_ref())
            .expect("stored chooser");
        assert_eq!(chooser.kind, ChooseTreeKind::Windows);
        assert_eq!(chooser.sessions_only, sessions_only);
    }

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("focus-sidebar", [] as [&str; 0]),
        )
        .expect_err("command clients cannot focus native UI");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message.contains("interactive client")
    ));
}

#[test]
fn choose_path_opens_only_for_desktop_clients_in_a_usable_terminal_pane() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "pick"]),
        )
        .expect("session");
    let session = context.session.expect("session id");
    let pane = context.pane.expect("pane id");
    shared.attach(client, session).expect("attach session");
    take_reliable_messages(&mailbox);
    let choose_path = |kind: ClientKind, context: &mut ExecutionContext, args: &[&str]| {
        shared
            .execute(
                client,
                kind,
                context,
                &CommandInvocation::new("choose-path", args.iter().copied()),
            )
            .map(|_| ())
    };

    assert!(
        choose_path_error(choose_path(ClientKind::Command, &mut context, &[]))
            .contains("interactive client")
    );
    assert_eq!(
        choose_path_error(choose_path(ClientKind::Interactive, &mut context, &[])),
        "choose-path needs the zz desktop app"
    );

    shared.inner.lock().client_entry(client).path_picker = true;
    shared
        .inner
        .lock()
        .client_entry(client)
        .path_list_root
        .replace((1, Some(PathBuf::from("/stale"))));
    choose_path(
        ClientKind::Interactive,
        &mut context,
        &["-c", "/tmp/#{session_name}"],
    )
    .expect("open path picker");
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::OpenPathPicker { pane: opened, start_dir },
            ..
        }) if *opened == pane && start_dir.as_deref() == Some("/tmp/pick")
    )));
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.path_list_root.is_none())));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-window", [] as [&str; 0]),
        )
        .expect("second window");
    let result = choose_path(
        ClientKind::Interactive,
        &mut context,
        &["-t", &pane.to_string()],
    );
    assert!(matches!(
        result,
        Err(DaemonError::Server(ServerError::PaneNotAttached(refused))) if refused == pane
    ));
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("select-window", ["-t", &pane.to_string()]),
        )
        .expect("back to the first window");
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");

    shared.inner.lock().attachment.client_flags.insert(client);
    assert_eq!(
        choose_path_error(choose_path(ClientKind::Interactive, &mut context, &[])),
        "client is read-only"
    );
    shared.inner.lock().attachment.client_flags.remove(client);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("copy-mode", [] as [&str; 0]),
        )
        .expect("enter copy mode");
    assert!(shared.read_client(client, |c| c.is_some_and(|c| c.copy_session.is_some())));
    assert!(
        choose_path_error(choose_path(ClientKind::Interactive, &mut context, &[]))
            .contains("copy mode")
    );
    shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.copy_session.take());

    let long_start = format!("/{}", "a".repeat(zz_protocol::MAX_PATH_LIST_TEXT_BYTES));
    for start in [long_start.as_str(), "/tmp/a\u{1b}b"] {
        assert_eq!(
            choose_path_error(choose_path(
                ClientKind::Interactive,
                &mut context,
                &["-c", start]
            )),
            "choose-path start directory cannot be listed"
        );
    }

    let terminal = shared.inner.lock().terminals_mut().remove(&pane);
    assert_eq!(
        choose_path_error(choose_path(ClientKind::Interactive, &mut context, &[])),
        "choose-path needs a terminal pane"
    );
    drop(terminal);

    shared.unregister(client);
    assert!(!shared.read_client(client, |c| c.is_some_and(|c| c.path_picker)));
}

#[test]
fn chooser_zoom_applies_only_to_raw_terminal_clients_and_preserves_existing_zoom() {
    for native_chooser in [false, true] {
        for initially_zoomed in [false, true] {
            let shared = Arc::new(Shared::new(1));
            let mailbox = OutboundMailbox::new();
            let (client, _) = shared.register_subscribed(
                ClientKind::Interactive,
                None,
                None,
                Arc::clone(&mailbox),
            );
            let (session, window, pane) = {
                let mut inner = shared.inner.lock();
                if native_chooser {
                    inner.client_entry(client).native_chooser = true;
                }
                let (session, window, pane) =
                    inner.engine.state.create_session("work").expect("session");
                inner
                    .engine
                    .state
                    .split_pane(pane, zz_protocol::Axis::Horizontal, PaneKind::Terminal)
                    .expect("split pane");
                inner.engine.state.select_pane(pane).expect("select pane");
                if initially_zoomed {
                    inner.engine.state.toggle_zoom(pane).expect("zoom pane");
                }
                (session, window, pane)
            };
            let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
                .expect("command context");
            shared.attach(client, session).expect("attach session");
            shared
                .execute(
                    client,
                    ClientKind::Interactive,
                    &mut context,
                    &CommandInvocation::new("set-buffer", ["chooser payload"]),
                )
                .expect("set buffer");

            for (command, flag, close) in [
                (
                    "choose-tree",
                    "-Zs",
                    InputMessage::ChooseTree {
                        action: ChooseTreeAction::Close,
                    },
                ),
                (
                    "choose-tree",
                    "-Zw",
                    InputMessage::ChooseTree {
                        action: ChooseTreeAction::Close,
                    },
                ),
                (
                    "choose-buffer",
                    "-Z",
                    InputMessage::ChooseBuffer {
                        action: ChooseBufferAction::Close,
                    },
                ),
            ] {
                let generation = shared.inner.lock().engine.state.generation();
                let temporary_zoom = !native_chooser && !initially_zoomed;
                shared
                    .execute(
                        client,
                        ClientKind::Interactive,
                        &mut context,
                        &CommandInvocation::new(command, [flag]),
                    )
                    .expect("open chooser");
                {
                    let inner = shared.inner.lock();
                    assert_eq!(
                        inner.engine.state.windows[&window].zoomed_pane,
                        (initially_zoomed || temporary_zoom).then_some(pane),
                    );
                    assert_eq!(
                        inner
                            .client(client)
                            .is_some_and(|c| c.chooser_zoom.is_some()),
                        temporary_zoom
                    );
                    assert_eq!(
                        inner.engine.state.generation(),
                        generation + u64::from(temporary_zoom),
                    );
                    assert!(
                        inner
                            .client(client)
                            .is_some_and(|c| c.choose_tree.is_some())
                            || inner
                                .client(client)
                                .is_some_and(|c| c.choose_buffer.is_some())
                    );
                }
                shared
                    .input(client, ClientKind::Interactive, &mut context, close)
                    .expect("close chooser");
                let inner = shared.inner.lock();
                assert_eq!(
                    inner.engine.state.windows[&window].zoomed_pane,
                    initially_zoomed.then_some(pane),
                );
                assert_eq!(
                    inner.engine.state.generation(),
                    generation + 2 * u64::from(temporary_zoom),
                );
                assert!(
                    inner
                        .client(client)
                        .is_none_or(|c| c.chooser_zoom.is_none())
                );
                assert!(inner.client(client).is_none_or(|c| c.choose_tree.is_none()));
                assert!(
                    inner
                        .client(client)
                        .is_none_or(|c| c.choose_buffer.is_none())
                );
            }
            shared.unregister(client);
            assert!(!shared.read_client(client, |c| c.is_some_and(|c| c.native_chooser)));
        }
    }
}
