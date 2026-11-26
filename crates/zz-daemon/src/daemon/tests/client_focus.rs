use super::*;

#[test]
fn attach_and_terminal_input_update_only_their_activity_boundaries() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(ClientKind::Interactive, None, None, mailbox);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "A"]),
        )
        .unwrap();
    let first = context.session.expect("first session");
    let first_pane = context.pane.expect("first pane");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "B"]),
        )
        .unwrap();
    let second = context.session.expect("second session");
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .state
            .most_recent_context()
            .map(|context| context.0),
        Some(second)
    );

    let first_sort = {
        let mut inner = shared.inner.lock();
        inner.engine.state.session_mut(first).unwrap().activity = Some(1);
        inner.engine.state.sessions[&first].sort_activity
    };
    shared.attach(client, first).unwrap();
    {
        let inner = shared.inner.lock();
        assert!(inner.engine.state.sessions[&first].sort_activity > first_sort);
        assert_ne!(inner.engine.state.sessions[&first].activity, Some(1));
        assert_eq!(
            inner
                .engine
                .state
                .most_recent_context()
                .map(|context| context.0),
            Some(first)
        );
    }
    let same_sort = {
        let mut inner = shared.inner.lock();
        inner.engine.state.session_mut(first).unwrap().activity = Some(2);
        inner.engine.state.sessions[&first].sort_activity
    };
    shared.attach(client, first).unwrap();
    {
        let inner = shared.inner.lock();
        assert!(inner.engine.state.sessions[&first].sort_activity > same_sort);
        assert_ne!(inner.engine.state.sessions[&first].activity, Some(2));
    }
    let second_sort = {
        let mut inner = shared.inner.lock();
        inner.engine.state.session_mut(second).unwrap().activity = Some(3);
        inner.engine.state.sessions[&second].sort_activity
    };
    shared.attach(client, second).unwrap();
    {
        let inner = shared.inner.lock();
        assert!(inner.engine.state.sessions[&second].sort_activity > second_sort);
        assert_ne!(inner.engine.state.sessions[&second].activity, Some(3));
        assert_eq!(
            inner
                .engine
                .state
                .most_recent_context()
                .map(|context| context.0),
            Some(second)
        );
    }
    shared.attach(client, first).unwrap();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "C"]),
        )
        .unwrap();
    let third = context.session.expect("third session");
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .state
            .most_recent_context()
            .map(|context| context.0),
        Some(third)
    );

    let ordinary_sort = {
        let mut inner = shared.inner.lock();
        inner.engine.state.session_mut(first).unwrap().activity = Some(4);
        inner.engine.state.sessions[&first].sort_activity
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::Key {
                pane: first_pane,
                input: test_key(KeyCode::Character('x'), Modifiers::default(), Some("x")),
                text_follows: false,
            },
        )
        .unwrap();
    {
        let inner = shared.inner.lock();
        assert!(inner.engine.state.sessions[&first].sort_activity > ordinary_sort);
        assert_ne!(inner.engine.state.sessions[&first].activity, Some(4));
        assert_eq!(
            inner
                .engine
                .state
                .most_recent_context()
                .map(|context| context.0),
            Some(first)
        );
    }

    let read_only_sort = {
        let mut inner = shared.inner.lock();
        inner.attachment.client_flags.insert(client);
        inner.engine.state.session_mut(first).unwrap().activity = Some(5);
        inner.engine.state.sessions[&first].sort_activity
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::Key {
                pane: first_pane,
                input: test_key(KeyCode::Character('y'), Modifiers::default(), Some("y")),
                text_follows: false,
            },
        )
        .unwrap();
    {
        let mut inner = shared.inner.lock();
        assert!(inner.engine.state.sessions[&first].sort_activity > read_only_sort);
        assert_ne!(inner.engine.state.sessions[&first].activity, Some(5));
        inner.attachment.client_flags.remove(client);
    }

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("choose-tree", [] as [&str; 0]),
        )
        .unwrap();
    let consumed_sort = {
        let mut inner = shared.inner.lock();
        inner.engine.state.session_mut(first).unwrap().activity = Some(6);
        inner.engine.state.sessions[&first].sort_activity
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::Key {
                pane: first_pane,
                input: test_key(KeyCode::Character('z'), Modifiers::default(), Some("z")),
                text_follows: false,
            },
        )
        .unwrap();
    let chooser_activity = {
        let inner = shared.inner.lock();
        assert!(inner.engine.state.sessions[&first].sort_activity > consumed_sort);
        assert_ne!(inner.engine.state.sessions[&first].activity, Some(6));
        (
            inner.engine.state.sessions[&first].sort_activity,
            inner.engine.state.sessions[&first].activity,
        )
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::Text {
                pane: first_pane,
                text: "overlay".to_owned(),
            },
        )
        .unwrap();
    {
        let inner = shared.inner.lock();
        assert_eq!(
            (
                inner.engine.state.sessions[&first].sort_activity,
                inner.engine.state.sessions[&first].activity,
            ),
            chooser_activity
        );
    }
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ChooseTree {
                action: ChooseTreeAction::Close,
            },
        )
        .unwrap();

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("command-prompt", ["-p", "activity"]),
        )
        .unwrap();
    let prompt_sort = {
        let mut inner = shared.inner.lock();
        inner.engine.state.session_mut(first).unwrap().activity = Some(7);
        inner.engine.state.sessions[&first].sort_activity
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::Key {
                pane: first_pane,
                input: test_key(KeyCode::Character('p'), Modifiers::default(), Some("p")),
                text_follows: false,
            },
        )
        .unwrap();
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.engine.state.sessions[&first].sort_activity,
            prompt_sort
        );
        assert_eq!(inner.engine.state.sessions[&first].activity, Some(7));
    }
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::Text {
                pane: first_pane,
                text: "typed".to_owned(),
            },
        )
        .unwrap();
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.engine.state.sessions[&first].sort_activity,
            prompt_sort
        );
        assert_eq!(inner.engine.state.sessions[&first].activity, Some(7));
    }
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::CommandPrompt {
                action: CommandPromptAction::Close,
            },
        )
        .unwrap();

    let theme_sort = {
        let mut inner = shared.inner.lock();
        inner.engine.state.session_mut(first).unwrap().activity = Some(8);
        inner.engine.state.sessions[&first].sort_activity
    };
    shared.set_client_color_scheme(client, TerminalColorScheme::Light);
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.engine.state.sessions[&first].sort_activity,
            theme_sort
        );
        assert_eq!(inner.engine.state.sessions[&first].activity, Some(8));
    }

    let untouched_sort = {
        let mut inner = shared.inner.lock();
        inner.engine.state.session_mut(first).unwrap().activity = Some(9);
        inner.engine.state.sessions[&first].sort_activity
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ResizeTerminal {
                pane: first_pane,
                columns: 81,
                rows: 25,
                cell_width_px: 8,
                cell_height_px: 18,
            },
        )
        .unwrap();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("switch-client", ["-T", "root"]),
        )
        .unwrap();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("list-sessions", [] as [&str; 0]),
        )
        .unwrap();
    let inner = shared.inner.lock();
    assert_eq!(
        inner.engine.state.sessions[&first].sort_activity,
        untouched_sort
    );
    assert_eq!(inner.engine.state.sessions[&first].activity, Some(9));
}

#[test]
fn client_focus_updates_activity_and_focus_in_owns_latest_geometry() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, terminal) = output_view_session_fixture(&shared, "client-focus", "focus");
    let first_mailbox = OutboundMailbox::new();
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .expect("focus window");
    let window_target = window.to_string();
    let (first, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("first-focus-client".to_owned()),
        None,
        Arc::clone(&first_mailbox),
    );
    let (second, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("second-focus-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(first, session).expect("attach first client");
    shared
        .attach(second, session)
        .expect("attach second client");
    shared
        .input(
            first,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ResizeTerminal {
                pane,
                columns: 100,
                rows: 30,
                cell_width_px: 10,
                cell_height_px: 20,
            },
        )
        .expect("store first geometry");
    shared
        .input(
            second,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ResizeTerminal {
                pane,
                columns: 80,
                rows: 20,
                cell_width_px: 8,
                cell_height_px: 18,
            },
        )
        .expect("store second geometry");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-window-option",
                ["-t", window_target.as_str(), "window-size", "latest"],
            ),
        )
        .expect("select latest geometry");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "focus-events", "on"]),
        )
        .expect("enable focus events");
    shared
        .attach(first, session)
        .expect("make first attached client latest");
    wait_for_pending_terminal(&first_mailbox, pane);
    let deadline = Instant::now() + Duration::from_secs(30);
    while shared
        .inner
        .lock()
        .engine
        .state
        .pane(pane)
        .expect("focus pane")
        .title
        != "client-focus fixture"
    {
        assert!(
            Instant::now() < deadline,
            "the fixture title never reached the pane"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let mut chooser_context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("activity chooser context");
    shared
        .execute(
            first,
            ClientKind::Interactive,
            &mut chooser_context,
            &CommandInvocation::new("choose-tree", ["-O", "activity"]),
        )
        .expect("open activity chooser");
    {
        let mut inner = shared.inner.lock();
        assert!(inner.engine.state.set_pane_bell(pane, true));
        inner.engine.state.session_mut(session).unwrap().activity = Some(-10);
    }
    wait_for_viewport(
        &terminal,
        TerminalViewId(first.0),
        "first client geometry was not selected",
        |viewport| viewport.columns == 100 && viewport.rows == 30,
    );
    take_reliable_messages(&first_mailbox);
    let (activity_sequence, terminal_input_sequence, sort_activity, mux_generation) = {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.pane_geometry(pane), Some((100, 30)));
        (
            inner.attachment.activity_sequence,
            inner.panes.terminal_input_sequence,
            inner.engine.state.sessions[&session].sort_activity,
            inner.engine.state.generation(),
        )
    };
    shared
        .input(
            second,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ClientFocus { focused: true },
        )
        .expect("focus second client");
    wait_for_viewport(
        &terminal,
        TerminalViewId(second.0),
        "focused client did not become latest geometry",
        |viewport| viewport.columns == 80 && viewport.rows == 20,
    );
    {
        let inner = shared.inner.lock();
        let retained = inner.engine.state.sessions[&session]
            .activity
            .expect("retained activity");
        assert_ne!(retained, -10);
        assert_eq!(
            inner.engine.state.sessions[&session].sort_activity,
            sort_activity + 1
        );
        assert_eq!(inner.attachment.activity_sequence, activity_sequence + 1);
        assert_eq!(
            inner.clients[&second].activity.unwrap(),
            activity_sequence + 1
        );
        assert_eq!(
            i64::try_from(inner.clients[&second].activity_time.unwrap()).ok(),
            Some(retained)
        );
        assert_eq!(
            inner.panes.terminal_input_sequence,
            terminal_input_sequence + 1
        );
        assert_eq!(terminal_geometry_owner(&inner, pane), Some(second));
        assert_eq!(inner.engine.pane_geometry(pane), Some((80, 20)));
        assert!(inner.engine.state.generation() > mux_generation);
        assert!(inner.publication.last_published_mux_generation > mux_generation);
        assert!(inner.engine.state.pane(pane).expect("focus pane").bell);
    }
    let peer_messages = take_reliable_messages(&first_mailbox);
    assert_eq!(
        peer_messages
            .iter()
            .filter(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ChooseTree { state: Some(_) },
                    ..
                })
            ))
            .count(),
        0
    );
    let peer_snapshot = peer_messages
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::Snapshot(snapshot),
                ..
            }) => Some(snapshot),
            _ => None,
        })
        .find(|snapshot| {
            snapshot.generation > mux_generation
                && snapshot
                    .sessions
                    .iter()
                    .flat_map(|session| &session.windows)
                    .find(|candidate| candidate.id == window)
                    .is_some_and(|window| {
                        zz_mux::legacy_layout(&window.layout_dump)
                            .ends_with(&format!("80x20,0,0,{}", pane.0))
                            && window.panes.get(&pane).is_some_and(|pane| pane.bell)
                    })
        })
        .expect("peer focus geometry snapshot");
    let peer_window = peer_snapshot.sessions[0]
        .windows
        .iter()
        .find(|candidate| candidate.id == window)
        .expect("peer focus window");
    assert!(
        zz_mux::legacy_layout(&peer_window.layout_dump).ends_with(&format!("80x20,0,0,{}", pane.0)),
        "{}",
        peer_window.layout_dump
    );
    assert!(peer_window.panes[&pane].bell);

    let paired_state = {
        let inner = shared.inner.lock();
        (
            inner.engine.state.sessions[&session].activity,
            inner.engine.state.sessions[&session].sort_activity,
            inner.attachment.activity_sequence,
            inner.panes.terminal_input_sequence,
            inner.clients[&second].activity.unwrap(),
            terminal_geometry_owner(&inner, pane),
        )
    };
    shared
        .input(
            second,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::TerminalView {
                pane,
                action: TerminalViewAction::Focus(true),
            },
        )
        .expect("forward paired pane focus");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            (
                inner.engine.state.sessions[&session].activity,
                inner.engine.state.sessions[&session].sort_activity,
                inner.attachment.activity_sequence,
                inner.panes.terminal_input_sequence,
                inner.clients[&second].activity.unwrap(),
                terminal_geometry_owner(&inner, pane),
            ),
            paired_state
        );
        assert!(inner.engine.state.pane(pane).expect("focus pane").bell);
    }

    let focus_out_state = {
        let mut inner = shared.inner.lock();
        inner.engine.state.session_mut(session).unwrap().activity = Some(-11);
        (
            inner.engine.state.sessions[&session].sort_activity,
            inner.attachment.activity_sequence,
            inner.panes.terminal_input_sequence,
            terminal_geometry_owner(&inner, pane),
        )
    };
    shared
        .input(
            first,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ClientFocus { focused: false },
        )
        .expect("blur first client");
    {
        let inner = shared.inner.lock();
        assert_ne!(inner.engine.state.sessions[&session].activity, Some(-11));
        assert_eq!(
            inner.engine.state.sessions[&session].sort_activity,
            focus_out_state.0 + 1
        );
        assert_eq!(inner.attachment.activity_sequence, focus_out_state.1 + 1);
        assert_eq!(
            inner.clients[&first].activity.unwrap(),
            focus_out_state.1 + 1
        );
        assert_eq!(inner.panes.terminal_input_sequence, focus_out_state.2);
        assert_eq!(terminal_geometry_owner(&inner, pane), focus_out_state.3);
        assert!(inner.engine.state.pane(pane).expect("focus pane").bell);
    }

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "focus-events", "off"]),
        )
        .expect("disable focus events");
    let inactive = {
        let mut inner = shared.inner.lock();
        inner.engine.state.session_mut(session).unwrap().activity = Some(-12);
        (
            inner.engine.state.sessions[&session].activity,
            inner.engine.state.sessions[&session].sort_activity,
            inner.attachment.activity_sequence,
            inner.panes.terminal_input_sequence,
            inner.clients[&second].activity.unwrap(),
            terminal_geometry_owner(&inner, pane),
        )
    };
    for focused in [true, false] {
        shared
            .input(
                second,
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                InputMessage::ClientFocus { focused },
            )
            .expect("ignore disabled client focus");
    }
    let inner = shared.inner.lock();
    assert_eq!(
        (
            inner.engine.state.sessions[&session].activity,
            inner.engine.state.sessions[&session].sort_activity,
            inner.attachment.activity_sequence,
            inner.panes.terminal_input_sequence,
            inner.clients[&second].activity.unwrap(),
            terminal_geometry_owner(&inner, pane),
        ),
        inactive
    );
    assert!(inner.engine.state.pane(pane).expect("focus pane").bell);
}

#[test]
fn client_focus_dispatches_any_after_writable_prequeue_and_latest_accounting() {
    let shared = Arc::new(Shared::new(1));
    let first_mailbox = OutboundMailbox::new();
    let (session, pane, _) = output_view_session_fixture(&shared, "focus-any-order", "focus");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .expect("focus window");
    let (first, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("focus-any-first".to_owned()),
        None,
        Arc::clone(&first_mailbox),
    );
    let (second, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("focus-any-second".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(first, session).expect("attach first client");
    shared
        .attach(second, session)
        .expect("attach second client");
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("focus context");
    for (client, columns, rows) in [(first, 100, 30), (second, 80, 20)] {
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::ResizeTerminal {
                    pane,
                    columns,
                    rows,
                    cell_width_px: 8,
                    cell_height_px: 18,
                },
            )
            .expect("store client geometry");
    }
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                ["-t", &window.to_string(), "window-size", "latest"],
            ),
        )
        .expect("select latest geometry");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "focus-events", "on"]),
        )
        .expect("enable focus events");
    shared
        .attach(second, session)
        .expect("make second client latest");
    shared
        .execute(
            first,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-panes", ["-d", "60000"]),
        )
        .expect("open display overlay");
    shared
        .execute(
            first,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "5000", "focus message"]),
        )
        .expect("open focus message");
    shared
        .inner
        .lock()
        .client_entry(first)
        .command_prompt
        .replace(CommandPrompt::new(
            vec![CommandPromptStep {
                label: "prompt".to_owned(),
                input: "7".to_owned(),
            }],
            Some(CommandPromptTemplate::String(
                "rename-session -- 'prompt-before-any'".to_owned(),
            )),
            None,
            CommandPromptType::Command,
            CommandPromptMode::Numeric,
            false,
            None,
        ));
    bind_focus_any(
        &shared,
        "root",
        CommandInvocation::new("rename-session", ["--", "any-after-accounting"]),
    );
    take_reliable_messages(&first_mailbox);
    let activity = shared.inner.lock().attachment.activity_sequence;

    shared
        .input(
            first,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: true },
        )
        .expect("dispatch focused Any binding");

    let inner = shared.inner.lock();
    assert_eq!(
        inner.engine.state.sessions[&session].name,
        "any-after-accounting"
    );
    assert_eq!(inner.attachment.activity_sequence, activity + 1);
    assert_eq!(terminal_geometry_owner(&inner, pane), Some(first));
    assert_eq!(inner.engine.pane_geometry(pane), Some((100, 30)));
    assert!(inner.client(first).is_none_or(|c| c.message.is_none()));
    assert!(
        inner
            .client(first)
            .is_none_or(|c| c.display_panes.is_none())
    );
    assert!(
        inner
            .client(first)
            .is_none_or(|c| c.command_prompt.is_none())
    );
}

#[test]
fn client_focus_any_prefers_chooser_and_copy_modes_then_falls_back_to_root() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "focus-any-modes", "focus");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("focus-any-modes-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(client, session).expect("attach focus client");
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("focus context");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "focus-events", "on"]),
        )
        .expect("enable focus events");
    bind_focus_any(
        &shared,
        "root",
        CommandInvocation::new("rename-session", ["--", "root-focus-any"]),
    );
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("choose-tree", [] as [&str; 0]),
        )
        .expect("open tree chooser");

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: false },
        )
        .expect("fall back from chooser to root Any");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.state.sessions[&session].name, "root-focus-any");
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.choose_tree.is_some())
        );
    }

    bind_focus_any(
        &shared,
        "choose-tree",
        CommandInvocation::new("rename-session", ["--", "chooser-focus-any"]),
    );
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: false },
        )
        .expect("dispatch chooser Any");
    {
        let mut inner = shared.inner.lock();
        assert_eq!(
            inner.engine.state.sessions[&session].name,
            "chooser-focus-any"
        );
        inner.client_mut(client).and_then(|c| c.choose_tree.take());
    }

    let copy_window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .expect("copy window");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                ["-t", &copy_window.to_string(), "mode-keys", "vi"],
            ),
        )
        .expect("select vi copy keys");
    let copy_table = {
        let mut inner = shared.inner.lock();
        enter_copy_session(&mut inner, client, pane).expect("enter copy session");
        inner
            .engine
            .copy_mode_table_for_pane(pane)
            .expect("copy table")
            .to_owned()
    };
    assert_eq!(copy_table, "copy-mode-vi");
    bind_focus_any(
        &shared,
        &copy_table,
        CommandInvocation::new("rename-session", ["--", "copy-focus-any"]),
    );
    shared.inner.lock().engine.keys.bind(
        &copy_table,
        "C-M-f",
        zz_mux::Binding {
            commands: vec![CommandInvocation::new("send-keys", ["-X", "jump-forward"])],
            repeat: false,
            note: None,
        },
    );
    assert_eq!(
        shared.key_decision(client, "C-M-f", false),
        KeyDecision::Ignore
    );

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: true },
        )
        .expect("dispatch copy-mode Any");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.state.sessions[&session].name, "copy-focus-any");
        assert_eq!(
            inner
                .client(client)
                .and_then(|c| c.key_engine.as_ref())
                .and_then(KeyEngine::active_table),
            Some("copy-mode-vi")
        );
    }
    assert_eq!(
        shared.key_decision(client, "x", false),
        KeyDecision::Commands(vec![CommandInvocation::new(
            "send-keys",
            ["-X", "jump-forward", "x"],
        )])
    );

    shared.inner.lock().engine.keys.unbind(&copy_table, "Any");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: false },
        )
        .expect("fall back from copy mode to root Any");
    let inner = shared.inner.lock();
    assert_eq!(inner.engine.state.sessions[&session].name, "root-focus-any");
    assert_eq!(
        inner
            .client(client)
            .and_then(|c| c.key_engine.as_ref())
            .and_then(KeyEngine::active_table),
        Some("copy-mode-vi")
    );
}

#[test]
fn read_only_client_focus_any_uses_authorization_and_isolates_key_state() {
    let shared = Arc::new(Shared::new(1));
    let first_mailbox = OutboundMailbox::new();
    let (session, _, _) = output_view_session_fixture(&shared, "read-only-focus-any", "focus");
    let (first, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("read-only-focus-any-first".to_owned()),
        None,
        Arc::clone(&first_mailbox),
    );
    let (second, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("read-only-focus-any-second".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(first, session).expect("attach first client");
    shared
        .attach(second, session)
        .expect("attach second client");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "focus-events", "on"]),
        )
        .expect("enable focus events");
    bind_focus_any(
        &shared,
        "root",
        CommandInvocation::new("switch-client", ["-T", "prefix"]),
    );
    let (original_name, activity) = {
        let mut inner = shared.inner.lock();
        inner.attachment.client_flags.insert(first);
        (
            inner.engine.state.sessions[&session].name.clone(),
            inner.attachment.activity_sequence,
        )
    };

    shared
        .input(
            first,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ClientFocus { focused: false },
        )
        .expect("dispatch safe read-only Any");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.attachment.activity_sequence, activity + 1);
        assert_eq!(
            inner
                .client(first)
                .and_then(|c| c.key_engine.as_ref())
                .and_then(KeyEngine::active_table),
            Some("prefix")
        );
        assert_eq!(
            inner
                .client(second)
                .and_then(|c| c.key_engine.as_ref())
                .and_then(KeyEngine::active_table),
            None
        );
        assert!(prefix_published(&inner, first));
        assert!(!prefix_published(&inner, second));
    }

    bind_focus_any(
        &shared,
        "root",
        CommandInvocation::new("rename-session", ["--", "blocked-focus-any"]),
    );
    shared
        .input(
            first,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ClientFocus { focused: false },
        )
        .expect("authorize unsafe read-only Any");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.attachment.activity_sequence, activity + 2);
        assert_eq!(inner.engine.state.sessions[&session].name, original_name);
        assert_eq!(
            inner
                .client(first)
                .and_then(|c| c.key_engine.as_ref())
                .and_then(KeyEngine::active_table),
            None
        );
        assert_eq!(
            inner
                .client(second)
                .and_then(|c| c.key_engine.as_ref())
                .and_then(KeyEngine::active_table),
            None
        );
        assert!(!prefix_published(&inner, first));
        assert!(!prefix_published(&inner, second));
    }
    assert!(
        take_reliable_messages(&first_mailbox)
            .iter()
            .any(|message| {
                matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::TimedClientMessage { text, .. },
                        ..
                    }) if text == "client is read-only"
                )
            })
    );
}

#[test]
fn client_focus_obeys_writable_command_prompt_prequeue() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (session, pane, _) = output_view_session_fixture(&shared, "focus-prompt", "prompt");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("focus-prompt-client".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared
        .attach(client, session)
        .expect("attach prompt client");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("prompt context");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 80,
                rows: 24,
                cell_width_px: 8,
                cell_height_px: 16,
            },
        )
        .expect("store prompt client geometry");
    let open_prompt = |mode, input: &str, template: Option<&str>| {
        shared
            .inner
            .lock()
            .client_entry(client)
            .command_prompt
            .replace(CommandPrompt::new(
                vec![CommandPromptStep {
                    label: "prompt".to_owned(),
                    input: input.to_owned(),
                }],
                template.map(|template| CommandPromptTemplate::String(template.to_owned())),
                None,
                CommandPromptType::Command,
                mode,
                false,
                None,
            ));
    };
    let focus_state = || {
        let inner = shared.inner.lock();
        (
            inner.attachment.activity_sequence,
            inner.panes.terminal_input_sequence,
            inner.engine.state.sessions[&session].sort_activity,
            terminal_geometry_owner(&inner, pane),
        )
    };

    open_prompt(CommandPromptMode::Text, "", None);
    let disabled = focus_state();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: true },
        )
        .expect("ignore disabled prompt focus");
    assert_eq!(focus_state(), disabled);
    assert!(shared.read_client(client, |c| c.is_some_and(|c| c.command_prompt.is_some())));
    shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.command_prompt.take());

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "focus-events", "on"]),
        )
        .expect("enable focus events");
    shared.inner.lock().engine.state.set_pane_bell(pane, true);

    open_prompt(CommandPromptMode::Text, "kept", None);
    let consumed = focus_state();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: true },
        )
        .expect("consume text prompt focus");
    assert_eq!(focus_state(), consumed);
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .command_prompt
                .as_ref()
                .unwrap()
                .input,
            "kept"
        );
        assert!(inner.engine.state.pane(pane).expect("prompt pane").bell);
    }
    shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.command_prompt.take());

    for (focused, answer) in [(true, "FocusIn"), (false, "FocusOut")] {
        open_prompt(CommandPromptMode::Key, "", Some("rename-session -- '%%'"));
        let before = focus_state();
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::ClientFocus { focused },
            )
            .expect("submit key prompt focus");
        assert_eq!(focus_state(), before);
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.command_prompt.is_none())
        );
        assert_eq!(inner.engine.state.sessions[&session].name, answer);
        assert!(inner.engine.state.pane(pane).expect("key prompt pane").bell);
    }

    for (focused, input) in [(false, "7"), (true, "8")] {
        open_prompt(
            CommandPromptMode::Numeric,
            input,
            Some("rename-session -- 'numeric-%%'"),
        );
        let before = focus_state();
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::ClientFocus { focused },
            )
            .expect("pass numeric prompt focus");
        let after = focus_state();
        assert_eq!(after.0, before.0 + 1);
        assert_eq!(after.1, before.1 + u64::from(focused));
        assert_eq!(after.2, before.2 + 1);
        assert_eq!(after.3, if focused { Some(client) } else { before.3 });
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.command_prompt.is_none())
        );
        assert_eq!(
            inner.engine.state.sessions[&session].name,
            format!("numeric-{input}")
        );
        assert!(
            inner
                .engine
                .state
                .pane(pane)
                .expect("numeric prompt pane")
                .bell
        );
        assert!(inner.history.command_history.is_empty());
    }
    open_prompt(
        CommandPromptMode::Numeric,
        "9",
        Some("set-option -g focus-events off"),
    );
    let before = focus_state();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: false },
        )
        .expect("account accepted focus after numeric submission");
    let after = focus_state();
    assert_eq!(after.0, before.0 + 1);
    assert_eq!(after.1, before.1);
    assert_eq!(after.2, before.2 + 1);
    {
        let inner = shared.inner.lock();
        assert!(!inner.engine.focus_events());
        assert!(inner.history.command_history.is_empty());
    }
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::CommandPrompt { state: None },
            ..
        })
    )));
}

#[test]
fn client_focus_closes_display_panes_and_preserves_chooser_modes() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (session, pane, _) = output_view_session_fixture(&shared, "focus-modal", "modal");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("focus-modal-client".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(client, session).expect("attach modal client");
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("modal context");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 80,
                rows: 24,
                cell_width_px: 8,
                cell_height_px: 16,
            },
        )
        .expect("store modal client geometry");
    let deadline = Instant::now() + Duration::from_secs(10);
    while shared
        .inner
        .lock()
        .engine
        .state
        .pane(pane)
        .is_none_or(|pane| pane.title != "focus-modal fixture")
    {
        assert!(Instant::now() < deadline, "the fixture title did not sync");
        thread::sleep(Duration::from_millis(5));
    }
    wait_for_viewport(
        &Arc::clone(&shared.inner.lock().terminals[&pane]),
        TerminalViewId(client.0),
        "the modal client's view never rendered",
        |_| true,
    );
    shared.refresh_status();
    let focus_state = || {
        let inner = shared.inner.lock();
        (
            inner.attachment.activity_sequence,
            inner.panes.terminal_input_sequence,
            inner.engine.state.sessions[&session].sort_activity,
            terminal_geometry_owner(&inner, pane),
        )
    };
    let open_display = |context: &mut ExecutionContext, args: &[&str]| {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                context,
                &CommandInvocation::new("display-panes", args.iter().copied()),
            )
            .expect("open display overlay");
        take_reliable_messages(&mailbox);
    };

    open_display(&mut context, &["-d", "0"]);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "5000", "disabled focus"]),
        )
        .expect("arm disabled-focus message");
    let disabled_message = armed_client_message(&shared, client);
    assert!(disabled_message.freeze);
    take_reliable_messages(&mailbox);
    drain_terminal_lane(&mailbox);
    let disabled = focus_state();
    let disabled_token = shared.inner.lock().clients[&client]
        .display_panes
        .as_ref()
        .unwrap()
        .token;
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: false },
        )
        .expect("ignore disabled display focus");
    assert_eq!(focus_state(), disabled);
    assert_eq!(
        shared.inner.lock().clients[&client]
            .display_panes
            .as_ref()
            .unwrap()
            .token,
        disabled_token
    );
    assert_eq!(armed_client_message(&shared, client), disabled_message);
    assert!(client_terminal_publication_frozen(
        &shared.inner.lock(),
        client
    ));
    let messages = take_reliable_messages(&mailbox);
    assert!(
        messages.iter().all(|message| matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::Snapshot(_),
                ..
            })
        )),
        "{messages:#?}"
    );
    assert!(mailbox.state.lock().terminals.is_empty());

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "focus-events", "on"]),
        )
        .expect("enable focus events");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::DisplayPanes {
                action: DisplayPanesAction::Key(display_panes_escape_key()),
            },
        )
        .expect("close the untimed display overlay");
    take_reliable_messages(&mailbox);
    open_display(&mut context, &["-d", "60000"]);
    {
        let mut inner = shared.inner.lock();
        assert!(inner.engine.state.set_pane_bell(pane, true));
        assert!(inner.clients[&client].display_panes.is_some());
    }
    drain_terminal_lane(&mailbox);
    let before = focus_state();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: false },
        )
        .expect("close selectable display overlay");
    let after = focus_state();
    assert_eq!(after.0, before.0 + 1);
    assert_eq!(after.1, before.1);
    assert_eq!(after.2, before.2 + 1);
    assert_eq!(after.3, before.3);
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.display_panes.is_none())
        );
        assert!(inner.client(client).is_none_or(|c| c.message.is_none()));
        assert!(!client_terminal_publication_frozen(&inner, client));
    }
    let messages = take_reliable_messages(&mailbox);
    assert_eq!(cleared_message_ids(&messages), [disabled_message.token]);
    assert!(messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::DisplayPanes { state: None },
            ..
        })
    )));
    assert!(mailbox.state.lock().terminals.contains_key(&pane));

    open_display(&mut context, &["-N", "-d", "0"]);
    let before = focus_state();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: true },
        )
        .expect("close no-select display overlay");
    let after = focus_state();
    assert_eq!(after.0, before.0 + 1);
    assert_eq!(after.1, before.1 + 1);
    assert_eq!(after.2, before.2 + 1);
    assert_eq!(after.3, Some(client));
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.display_panes.is_none())
        );
        assert!(inner.engine.state.pane(pane).expect("display pane").bell);
    }

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("choose-tree", [] as [&str; 0]),
        )
        .expect("open tree chooser");
    let before = focus_state();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: false },
        )
        .expect("pass tree chooser focus");
    let after = focus_state();
    assert_eq!(after.0, before.0 + 1);
    assert_eq!(after.1, before.1);
    assert_eq!(after.2, before.2 + 1);
    assert!(shared.read_client(client, |c| c.is_some_and(|c| c.choose_tree.is_some())));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-buffer", ["-b", "focus", "value"]),
        )
        .expect("create chooser buffer");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("choose-buffer", [] as [&str; 0]),
        )
        .expect("open buffer chooser");
    let before = focus_state();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: true },
        )
        .expect("pass buffer chooser focus");
    let after = focus_state();
    assert_eq!(after.0, before.0 + 1);
    assert_eq!(after.1, before.1 + 1);
    assert_eq!(after.2, before.2 + 1);
    assert_eq!(after.3, Some(client));
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.choose_buffer.is_some())
        );
        assert!(inner.engine.state.pane(pane).expect("chooser pane").bell);
    }
}

#[test]
fn read_only_client_focus_updates_activity_and_focus_in_owns_stored_geometry() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) =
        output_view_session_fixture(&shared, "read-only-client-focus", "focus");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .expect("focus window");
    let writable_mailbox = OutboundMailbox::new();
    let read_only_mailbox = OutboundMailbox::new();
    let (writable, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("writable-focus-client".to_owned()),
        None,
        Arc::clone(&writable_mailbox),
    );
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("read-only-focus-client".to_owned()),
        None,
        Arc::clone(&read_only_mailbox),
    );
    shared
        .attach(writable, session)
        .expect("attach writable client");
    shared
        .attach(client, session)
        .expect("attach read-only client");
    shared
        .input(
            writable,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ResizeTerminal {
                pane,
                columns: 100,
                rows: 30,
                cell_width_px: 10,
                cell_height_px: 20,
            },
        )
        .expect("store writable geometry");
    shared.inner.lock().attachment.client_flags.insert(client);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ResizeTerminal {
                pane,
                columns: 80,
                rows: 20,
                cell_width_px: 8,
                cell_height_px: 18,
            },
        )
        .expect("store read-only geometry");
    shared
        .attach(writable, session)
        .expect("make writable client latest");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "focus-events", "on"]),
        )
        .expect("enable focus events");
    let mut modal_context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("read-only modal context");
    shared.inner.lock().attachment.client_flags.remove(client);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut modal_context,
            &CommandInvocation::new("display-panes", ["-d", "0"]),
        )
        .expect("open read-only display overlay");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut modal_context,
            &CommandInvocation::new("display-message", ["-d", "5000", "read-only focus"]),
        )
        .expect("arm read-only focus message");
    let retained_message = armed_client_message(&shared, client);
    take_reliable_messages(&read_only_mailbox);
    drain_terminal_lane(&read_only_mailbox);
    let focus_out_before = {
        let mut inner = shared.inner.lock();
        inner.attachment.client_flags.insert(client);
        assert!(inner.engine.state.set_pane_bell(pane, true));
        inner
            .engine
            .state
            .session_mut(session)
            .expect("focus session")
            .activity = Some(-10);
        assert!(inner.attachment.client_flags.contains(client));
        assert_eq!(
            inner.panes.terminal_geometries[&pane][&client],
            TerminalGeometry {
                columns: 80,
                rows: 20,
                cell_width_px: 8,
                cell_height_px: 18,
            }
        );
        assert_eq!(terminal_geometry_owner(&inner, pane), Some(writable));
        assert_eq!(inner.engine.pane_geometry(pane), Some((100, 30)));
        (
            inner.engine.state.sessions[&session].sort_activity,
            inner.attachment.activity_sequence,
            inner.panes.terminal_input_sequence,
            inner.clients[&client].display_panes.as_ref().unwrap().token,
        )
    };
    take_reliable_messages(&writable_mailbox);

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ClientFocus { focused: false },
        )
        .expect("accept read-only client focus out");
    {
        let inner = shared.inner.lock();
        let retained = inner.engine.state.sessions[&session]
            .activity
            .expect("retained focus-out activity");
        assert_ne!(retained, -10);
        assert_eq!(
            inner.engine.state.sessions[&session].sort_activity,
            focus_out_before.0 + 1
        );
        assert_eq!(inner.attachment.activity_sequence, focus_out_before.1 + 1);
        assert_eq!(
            inner.clients[&client].activity.unwrap(),
            focus_out_before.1 + 1
        );
        assert_eq!(
            i64::try_from(inner.clients[&client].activity_time.unwrap()).ok(),
            Some(retained)
        );
        assert_eq!(inner.panes.terminal_input_sequence, focus_out_before.2);
        assert_eq!(
            inner.clients[&client].display_panes.as_ref().unwrap().token,
            focus_out_before.3
        );
        assert_eq!(
            (*inner.clients[&client].message.as_ref().unwrap()),
            retained_message
        );
        assert!(client_terminal_publication_frozen(&inner, client));
        assert_eq!(terminal_geometry_owner(&inner, pane), Some(writable));
        assert_eq!(inner.engine.pane_geometry(pane), Some((100, 30)));
        assert!(inner.engine.state.pane(pane).expect("focus pane").bell);
    }
    assert!(cleared_message_ids(&take_reliable_messages(&read_only_mailbox)).is_empty());
    assert!(read_only_mailbox.state.lock().terminals.is_empty());
    take_reliable_messages(&writable_mailbox);

    let focus_in_before = {
        let mut inner = shared.inner.lock();
        take_display_panes(&mut inner, client).expect("retire display overlay");
        inner
            .client_entry(client)
            .command_prompt
            .replace(CommandPrompt::new(
                vec![CommandPromptStep {
                    label: "prompt".to_owned(),
                    input: "kept".to_owned(),
                }],
                None,
                None,
                CommandPromptType::Command,
                CommandPromptMode::Text,
                false,
                None,
            ));
        inner
            .engine
            .state
            .session_mut(session)
            .expect("focus session")
            .activity = Some(-11);
        (
            inner.engine.state.sessions[&session].sort_activity,
            inner.attachment.activity_sequence,
            inner.panes.terminal_input_sequence,
            inner.engine.state.generation(),
        )
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ClientFocus { focused: true },
        )
        .expect("accept read-only client focus in");
    {
        let inner = shared.inner.lock();
        let retained = inner.engine.state.sessions[&session]
            .activity
            .expect("retained focus-in activity");
        assert_ne!(retained, -11);
        assert_eq!(
            inner.engine.state.sessions[&session].sort_activity,
            focus_in_before.0 + 1
        );
        assert_eq!(inner.attachment.activity_sequence, focus_in_before.1 + 1);
        assert_eq!(
            inner.clients[&client].activity.unwrap(),
            focus_in_before.1 + 1
        );
        assert_eq!(
            i64::try_from(inner.clients[&client].activity_time.unwrap()).ok(),
            Some(retained)
        );
        assert_eq!(inner.panes.terminal_input_sequence, focus_in_before.2 + 1);
        assert_eq!(
            inner.clients[&client]
                .command_prompt
                .as_ref()
                .unwrap()
                .input,
            "kept"
        );
        assert_eq!(
            (*inner.clients[&client].message.as_ref().unwrap()),
            retained_message
        );
        assert!(client_terminal_publication_frozen(&inner, client));
        assert_eq!(terminal_geometry_owner(&inner, pane), Some(client));
        assert_eq!(inner.engine.pane_geometry(pane), Some((80, 20)));
        assert!(inner.engine.state.generation() > focus_in_before.3);
        assert!(inner.publication.last_published_mux_generation > focus_in_before.3);
        assert!(inner.engine.state.pane(pane).expect("focus pane").bell);
    }
    let peer_messages = take_reliable_messages(&writable_mailbox);
    let peer_snapshot = peer_messages
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::Snapshot(snapshot),
                ..
            }) => Some(snapshot),
            _ => None,
        })
        .find(|snapshot| {
            snapshot.generation > focus_in_before.3
                && snapshot
                    .sessions
                    .iter()
                    .flat_map(|session| &session.windows)
                    .find(|candidate| candidate.id == window)
                    .is_some_and(|window| {
                        zz_mux::legacy_layout(&window.layout_dump)
                            .ends_with(&format!("80x20,0,0,{}", pane.0))
                            && window.panes.get(&pane).is_some_and(|pane| pane.bell)
                    })
        })
        .expect("peer read-only focus geometry snapshot");
    let peer_window = peer_snapshot.sessions[0]
        .windows
        .iter()
        .find(|candidate| candidate.id == window)
        .expect("peer focus window");
    assert!(
        zz_mux::legacy_layout(&peer_window.layout_dump).ends_with(&format!("80x20,0,0,{}", pane.0)),
        "{}",
        peer_window.layout_dump
    );
    assert!(peer_window.panes[&pane].bell);
}

#[test]
fn client_focus_resizes_unchanged_owner_siblings_after_layout_write_back() {
    let shared = Arc::new(Shared::new(1));
    let (session, active, active_terminal) =
        output_view_session_fixture(&shared, "client-focus-sibling", "active");
    let (window, sibling, sibling_terminal) = {
        let mut inner = shared.inner.lock();
        let window = inner
            .engine
            .state
            .window_for_pane(active)
            .expect("focus window");
        let sibling = inner
            .engine
            .state
            .split_pane(active, zz_protocol::Axis::Horizontal, PaneKind::Terminal)
            .expect("split focus window");
        inner
            .engine
            .state
            .select_pane(active)
            .expect("select active pane");
        inner
            .engine
            .state
            .resize_pane_to(active, zz_protocol::Axis::Horizontal, 78)
            .expect("skew focus split");
        let terminal = Arc::new(TerminalSession::spawn_output_view(
            "client-focus sibling fixture".to_owned(),
            "sibling".to_owned(),
        ));
        inner.terminals_mut().insert(sibling, Arc::clone(&terminal));
        (window, sibling, terminal)
    };
    shared
        .watch_terminal(sibling, &sibling_terminal)
        .expect("watch sibling terminal");
    let control_mailbox = OutboundMailbox::new();
    let interactive_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("focus-control".to_owned()),
        None,
        Arc::clone(&control_mailbox),
    );
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("focus-interactive".to_owned()),
        None,
        Arc::clone(&interactive_mailbox),
    );
    shared
        .attach(control, session)
        .expect("attach control client");
    shared
        .attach(interactive, session)
        .expect("attach interactive client");
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(active));
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-C", "80x24"]),
        )
        .expect("store control geometry");
    shared
        .input(
            interactive,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane: active,
                columns: 38,
                rows: 24,
                cell_width_px: 7,
                cell_height_px: 16,
            },
        )
        .expect("store active interactive geometry");
    shared
        .attach(control, session)
        .expect("make control client latest");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "focus-events", "on"]),
        )
        .expect("enable focus events");
    let (initial_active_geometry, initial_sibling_geometry, initial_sibling_resize) = {
        let inner = shared.inner.lock();
        assert_eq!(
            (*inner.clients[&interactive]
                .visible_terminals
                .as_ref()
                .unwrap()),
            BTreeSet::from([active, sibling])
        );
        assert!(
            !inner
                .panes
                .terminal_geometries
                .get(&sibling)
                .is_some_and(|geometries| geometries.contains_key(&interactive))
        );
        assert_eq!(terminal_geometry_owner(&inner, active), Some(control));
        assert_eq!(terminal_geometry_owner(&inner, sibling), Some(control));
        let active_geometry = inner.engine.pane_geometry(active).expect("active geometry");
        let sibling_geometry = inner
            .engine
            .pane_geometry(sibling)
            .expect("sibling geometry");
        let sibling_resize = terminal_resize_for_pane(&inner, sibling)
            .expect("initial sibling geometry")
            .1;
        assert_eq!(sibling_resize.cell_width_px, CONTROL_CELL_WIDTH_PX);
        assert_eq!(sibling_resize.cell_height_px, CONTROL_CELL_HEIGHT_PX);
        (active_geometry, sibling_geometry, sibling_resize)
    };
    wait_for_terminal_dimensions(
        &sibling_terminal,
        TerminalViewId(interactive.0),
        initial_sibling_resize.columns,
        initial_sibling_resize.rows,
    );
    take_reliable_messages(&control_mailbox);
    take_reliable_messages(&interactive_mailbox);
    let generation = shared.inner.lock().engine.state.generation();

    shared
        .input(
            interactive,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: true },
        )
        .expect("focus interactive client");

    let (active_resize, sibling_resize) = {
        let inner = shared.inner.lock();
        assert_eq!(terminal_geometry_owner(&inner, active), Some(interactive));
        assert_eq!(terminal_geometry_owner(&inner, sibling), Some(control));
        assert_ne!(
            inner.engine.pane_geometry(active),
            Some(initial_active_geometry)
        );
        assert_ne!(
            inner.engine.pane_geometry(sibling),
            Some(initial_sibling_geometry)
        );
        assert!(inner.engine.state.generation() > generation);
        let active_resize = terminal_resize_for_pane(&inner, active)
            .expect("focused active geometry")
            .1;
        let sibling_resize = terminal_resize_for_pane(&inner, sibling)
            .expect("recomputed sibling geometry")
            .1;
        assert_ne!(sibling_resize, initial_sibling_resize);
        assert_eq!(sibling_resize.cell_width_px, CONTROL_CELL_WIDTH_PX);
        assert_eq!(sibling_resize.cell_height_px, CONTROL_CELL_HEIGHT_PX);
        (active_resize, sibling_resize)
    };
    wait_for_terminal_dimensions(
        &active_terminal,
        TerminalViewId(interactive.0),
        active_resize.columns,
        active_resize.rows,
    );
    wait_for_terminal_dimensions(
        &sibling_terminal,
        TerminalViewId(interactive.0),
        sibling_resize.columns,
        sibling_resize.rows,
    );
}

#[test]
fn client_focus_refreshes_non_latest_owner_cell_metrics() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, terminal) =
        output_view_session_fixture(&shared, "client-focus-cell-metrics", "focus");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .expect("focus window");
    let window_target = window.to_string();
    let (first, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("first-cell-metrics-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (second, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("second-cell-metrics-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(first, session).expect("attach first client");
    shared
        .attach(second, session)
        .expect("attach second client");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-window-option",
                ["-t", window_target.as_str(), "window-size", "largest"],
            ),
        )
        .expect("select largest geometry");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "focus-events", "on"]),
        )
        .expect("enable focus events");
    for (client, cell_width_px, cell_height_px) in [(first, 10, 20), (second, 7, 16)] {
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                InputMessage::ResizeTerminal {
                    pane,
                    columns: 90,
                    rows: 25,
                    cell_width_px,
                    cell_height_px,
                },
            )
            .expect("store client geometry");
    }
    shared
        .attach(first, session)
        .expect("make first cell-metrics client latest");
    wait_for_viewport(
        &terminal,
        TerminalViewId(first.0),
        "first cell metrics geometry was not applied",
        |viewport| viewport.columns == 90 && viewport.rows == 25,
    );
    let generation = terminal
        .latest_viewport_for(TerminalViewId(first.0))
        .expect("first client viewport")
        .generation;
    assert_eq!(
        terminal_resize_for_pane(&shared.inner.lock(), pane)
            .expect("first owner geometry")
            .1,
        TerminalGeometry {
            columns: 90,
            rows: 25,
            cell_width_px: 10,
            cell_height_px: 20,
        }
    );

    shared
        .input(
            second,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ClientFocus { focused: true },
        )
        .expect("focus second client");

    assert_eq!(
        terminal_geometry_owner(&shared.inner.lock(), pane),
        Some(second)
    );
    assert_eq!(
        terminal_resize_for_pane(&shared.inner.lock(), pane)
            .expect("second owner geometry")
            .1,
        TerminalGeometry {
            columns: 90,
            rows: 25,
            cell_width_px: 7,
            cell_height_px: 16,
        }
    );
    wait_for_viewport(
        &terminal,
        TerminalViewId(first.0),
        "non-latest owner cell metrics did not reach the terminal",
        |viewport| {
            viewport.columns == 90 && viewport.rows == 25 && viewport.generation > generation
        },
    );
}
