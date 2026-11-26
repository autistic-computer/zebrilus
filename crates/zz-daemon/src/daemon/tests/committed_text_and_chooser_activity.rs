use super::*;

#[test]
fn chooser_mouse_activity_preserves_bell_and_read_only_updates_latest_geometry() {
    let shared = Arc::new(Shared::new(1));
    let fixture = (0..80)
        .map(|line| format!("modal mouse line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let (session, pane, terminal) =
        output_view_session_fixture(&shared, "modal-mouse-activity", fixture);
    let (other, _, _) = switch_test_session(&shared, "modal-mouse-other");
    let (geometry_owner_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("modal-mouse-owner".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("modal-mouse-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .attach(geometry_owner_client, session)
        .expect("attach geometry owner");
    shared.attach(client, session).expect("attach modal client");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("modal mouse context");
    shared
        .input(
            geometry_owner_client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ResizeTerminal {
                pane,
                columns: 100,
                rows: 10,
                cell_width_px: 9,
                cell_height_px: 19,
            },
        )
        .expect("store owning geometry");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 80,
                rows: 8,
                cell_width_px: 8,
                cell_height_px: 18,
            },
        )
        .expect("store modal geometry");
    shared
        .attach(geometry_owner_client, session)
        .expect("make geometry owner latest");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientTerminalSize {
                columns: 80,
                rows: 8,
            },
        )
        .expect("store modal client size");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "mouse", "off"]),
        )
        .expect("disable mouse forwarding");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-buffer", ["-b", "modal", "value"]),
        )
        .expect("create chooser buffer");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("choose-buffer", [] as [&str; 0]),
        )
        .expect("open chooser");
    let view = TerminalViewId(client.0);
    terminal.view_action(view, TerminalViewAction::ScrollToOffset(5));
    wait_for_viewport(
        &terminal,
        view,
        "modal mouse fixture did not reach its pinned offset",
        |viewport| viewport.scrollbar.offset == 5,
    );
    let wheel = || InputMessage::TerminalView {
        pane,
        action: TerminalViewAction::ScrollWheel {
            lines: -1,
            input: TerminalMouseInput::new(
                TerminalMousePhase::Press,
                Some(TerminalMouseButton::ScrollUp),
                PointerCellEvent {
                    column: 1,
                    row: 1,
                    click_count: 1,
                    rectangle: false,
                },
                24,
                18,
                128,
                72,
                8,
                18,
                Modifiers::default(),
                false,
            ),
        },
    };
    let motion = || InputMessage::TerminalView {
        pane,
        action: TerminalViewAction::Mouse(TerminalMouseInput::new(
            TerminalMousePhase::Motion,
            None,
            PointerCellEvent {
                column: 2,
                row: 2,
                click_count: 0,
                rectangle: false,
            },
            32,
            36,
            128,
            72,
            8,
            18,
            Modifiers::default(),
            false,
        )),
    };
    {
        let mut inner = shared.inner.lock();
        inner.client_flags.insert(client);
        inner.engine.mark_session_active_at(other, 1_700_000_020);
        inner.engine.state.session_mut(session).unwrap().activity = Some(-30);
        assert!(inner.engine.state.set_pane_bell(pane, true));
    }
    let (other_sort, activity_sequence, geometry, terminal_input_sequence) = {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.choose_buffer.is_some())
        );
        assert!(terminal_mouse_rejected(&inner, client, &wheel()));
        assert_eq!(
            terminal_geometry_owner(&inner, pane),
            Some(geometry_owner_client)
        );
        (
            inner.engine.state.sessions[&other].sort_activity,
            inner.activity_sequence,
            inner.panes.terminal_geometries[&pane].clone(),
            inner.panes.terminal_input_sequence,
        )
    };
    shared
        .input(client, ClientKind::Interactive, &mut context, wheel())
        .expect("reject read-only modal mouse");
    {
        let inner = shared.inner.lock();
        let retained = inner.engine.state.sessions[&session]
            .activity
            .expect("modal mouse activity");
        assert_ne!(retained, -30);
        assert_eq!(
            inner.engine.state.sessions[&session].sort_activity,
            other_sort + 1
        );
        assert_eq!(inner.activity_sequence, activity_sequence + 1);
        assert_eq!(
            inner.clients[&client].activity.unwrap(),
            activity_sequence + 1
        );
        assert_eq!(
            i64::try_from(inner.clients[&client].activity_time.unwrap()).ok(),
            Some(retained)
        );
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.choose_buffer.is_some())
        );
        assert_eq!(inner.panes.terminal_geometries[&pane], geometry);
        assert_eq!(terminal_geometry_owner(&inner, pane), Some(client));
        assert_eq!(
            inner.panes.terminal_input_sequence,
            terminal_input_sequence + 1
        );
        assert_eq!(
            inner
                .client(client)
                .and_then(|c| c.terminal_input_sequence.as_ref())
                .copied(),
            Some(terminal_input_sequence + 1)
        );
        assert_eq!(inner.engine.pane_geometry(pane), Some((80, 8)));
        assert!(inner.engine.state.pane(pane).expect("modal pane").bell);
    }
    assert_eq!(
        terminal
            .latest_viewport_for(view)
            .expect("modal mouse viewport")
            .scrollbar
            .offset,
        5
    );

    let read_only_motion_before = {
        let inner = shared.inner.lock();
        (inner.activity_sequence, inner.panes.terminal_input_sequence)
    };
    shared
        .input(client, ClientKind::Interactive, &mut context, motion())
        .expect("account read-only chooser mouse motion");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.activity_sequence, read_only_motion_before.0 + 1);
        assert_eq!(
            inner.panes.terminal_input_sequence,
            read_only_motion_before.1 + 1
        );
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.choose_buffer.is_some())
        );
        assert!(inner.engine.state.pane(pane).expect("modal pane").bell);
    }

    shared
        .attach(geometry_owner_client, session)
        .expect("restore prior geometry owner");
    wait_for_viewport(
        &terminal,
        view,
        "restored geometry owner did not resize the terminal",
        |viewport| viewport.columns == 100 && viewport.rows == 10,
    );
    let writable_offset = terminal
        .latest_viewport_for(view)
        .expect("resized writable modal viewport")
        .scrollbar
        .offset;
    let writable = {
        let mut inner = shared.inner.lock();
        inner.client_flags.remove(client);
        assert!(inner.engine.state.set_pane_bell(pane, true));
        inner.engine.state.session_mut(session).unwrap().activity = Some(-31);
        (
            inner.engine.state.sessions[&session].activity,
            inner.engine.state.sessions[&session].sort_activity,
            inner.activity_sequence,
            inner.clients[&client].activity.unwrap(),
            inner.clients[&client].activity_time.unwrap(),
            inner.panes.terminal_input_sequence,
            inner
                .client(client)
                .and_then(|c| c.terminal_input_sequence.as_ref())
                .copied(),
            terminal_geometry_owner(&inner, pane),
        )
    };
    assert_eq!(writable.7, Some(geometry_owner_client));
    shared
        .input(client, ClientKind::Interactive, &mut context, wheel())
        .expect("reject writable modal mouse");
    {
        let inner = shared.inner.lock();
        assert_ne!(inner.engine.state.sessions[&session].activity, writable.0);
        assert_eq!(
            inner.engine.state.sessions[&session].sort_activity,
            writable.1 + 1
        );
        assert_eq!(inner.activity_sequence, writable.2 + 1);
        assert_eq!(inner.clients[&client].activity.unwrap(), writable.2 + 1);
        assert_eq!(
            i64::try_from(inner.clients[&client].activity_time.unwrap()).ok(),
            inner.engine.state.sessions[&session].activity
        );
        assert_eq!(inner.panes.terminal_input_sequence, writable.5 + 1);
        assert_eq!(
            inner
                .client(client)
                .and_then(|c| c.terminal_input_sequence.as_ref())
                .copied(),
            Some(writable.5 + 1)
        );
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.choose_buffer.is_some())
        );
        assert_eq!(inner.panes.terminal_geometries[&pane], geometry);
        assert_eq!(terminal_geometry_owner(&inner, pane), Some(client));
        assert!(inner.engine.state.pane(pane).expect("modal pane").bell);
    }
    assert_eq!(
        terminal
            .latest_viewport_for(view)
            .expect("writable modal viewport")
            .scrollbar
            .offset,
        writable_offset
    );

    let before_motion = {
        let inner = shared.inner.lock();
        (inner.activity_sequence, inner.panes.terminal_input_sequence)
    };
    shared
        .input(client, ClientKind::Interactive, &mut context, motion())
        .expect("account chooser mouse motion");
    let inner = shared.inner.lock();
    assert_eq!(inner.activity_sequence, before_motion.0 + 1);
    assert_eq!(inner.panes.terminal_input_sequence, before_motion.1 + 1);
    assert!(
        inner
            .client(client)
            .is_some_and(|c| c.choose_buffer.is_some())
    );
    assert!(inner.engine.state.pane(pane).expect("modal pane").bell);
}

#[test]
fn chooser_actions_count_once_and_raw_keys_stay_client_scoped() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) =
        output_view_session_fixture(&shared, "chooser-input-activity", "chooser");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("chooser-owner".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (peer, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("chooser-peer".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .attach(client, session)
        .expect("attach chooser owner");
    shared.attach(peer, session).expect("attach chooser peer");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("chooser input context");
    for (name, value) in [("first", "one"), ("second", "two")] {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("set-buffer", ["-b", name, value]),
            )
            .expect("create chooser buffer");
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("bind-key", ["-n", "x", "copy-mode"]),
        )
        .expect("bind read-only-safe root command");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("choose-buffer", [] as [&str; 0]),
        )
        .expect("open chooser");

    let initial = {
        let mut inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .choose_buffer
                .as_ref()
                .unwrap()
                .rendered
                .selected,
            0
        );
        assert!(inner.engine.state.set_pane_bell(pane, true));
        (inner.activity_sequence, inner.panes.terminal_input_sequence)
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
        .expect("ignore pane focus while chooser is active");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.activity_sequence, initial.0);
        assert_eq!(inner.panes.terminal_input_sequence, initial.1);
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.choose_buffer.is_some())
        );
        assert!(inner.engine.state.pane(pane).expect("chooser pane").bell);
    }
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ChooseBuffer {
                action: ChooseBufferAction::Next,
            },
        )
        .expect("move chooser selection");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.activity_sequence, initial.0 + 1);
        assert_eq!(inner.panes.terminal_input_sequence, initial.1 + 1);
        assert_eq!(
            inner.clients[&client]
                .choose_buffer
                .as_ref()
                .unwrap()
                .rendered
                .selected,
            1
        );
        assert!(inner.engine.state.pane(pane).expect("chooser pane").bell);
    }

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(KeyCode::ArrowUp, Modifiers::default(), None),
                text_follows: false,
            },
        )
        .expect("route raw owner key into chooser");
    let after_owner_key = {
        let inner = shared.inner.lock();
        assert_eq!(inner.activity_sequence, initial.0 + 2);
        assert_eq!(inner.panes.terminal_input_sequence, initial.1 + 2);
        assert_eq!(
            inner.clients[&client]
                .choose_buffer
                .as_ref()
                .unwrap()
                .rendered
                .selected,
            0
        );
        inner.clients[&client]
            .choose_buffer
            .as_ref()
            .unwrap()
            .rendered
            .clone()
    };

    let peer_before = {
        let inner = shared.inner.lock();
        (inner.activity_sequence, inner.panes.terminal_input_sequence)
    };
    shared
        .input(
            peer,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(KeyCode::ArrowDown, Modifiers::default(), None),
                text_follows: false,
            },
        )
        .expect("route peer key outside chooser");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.activity_sequence, peer_before.0 + 1);
        assert_eq!(inner.panes.terminal_input_sequence, peer_before.1 + 1);
        assert_eq!(
            inner.clients[&client]
                .choose_buffer
                .as_ref()
                .unwrap()
                .rendered,
            after_owner_key
        );
    }

    let read_only_before = {
        let mut inner = shared.inner.lock();
        inner.client_flags.insert(client);
        assert!(inner.engine.state.set_pane_bell(pane, true));
        (
            inner.activity_sequence,
            inner.panes.terminal_input_sequence,
            inner.clients[&client]
                .choose_buffer
                .as_ref()
                .unwrap()
                .rendered
                .clone(),
        )
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(KeyCode::Character('x'), Modifiers::default(), Some("x")),
                text_follows: false,
            },
        )
        .expect("bypass chooser with read-only root key");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.activity_sequence, read_only_before.0 + 1);
        assert_eq!(inner.panes.terminal_input_sequence, read_only_before.1 + 1);
        assert_eq!(
            inner.clients[&client]
                .choose_buffer
                .as_ref()
                .unwrap()
                .rendered,
            read_only_before.2
        );
        assert_eq!(
            inner.clients[&client].copy_session.as_ref().unwrap().pane,
            pane
        );
        assert!(inner.engine.state.pane(pane).expect("chooser pane").bell);
    }

    let safe_view_before = {
        let inner = shared.inner.lock();
        (inner.activity_sequence, inner.panes.terminal_input_sequence)
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::TerminalView {
                pane,
                action: TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Cancel),
            },
        )
        .expect("bypass chooser with read-only-safe copy-mode action");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.activity_sequence, safe_view_before.0 + 1);
        assert_eq!(inner.panes.terminal_input_sequence, safe_view_before.1 + 1);
        assert_eq!(
            inner.clients[&client]
                .choose_buffer
                .as_ref()
                .unwrap()
                .rendered,
            read_only_before.2
        );
        assert!(
            inner.clients[&client]
                .copy_session
                .as_ref()
                .unwrap()
                .exiting
        );
        assert!(inner.engine.state.pane(pane).expect("chooser pane").bell);
    }

    let dedicated_before = {
        let inner = shared.inner.lock();
        (inner.activity_sequence, inner.panes.terminal_input_sequence)
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ChooseBuffer {
                action: ChooseBufferAction::Next,
            },
        )
        .expect("reject read-only dedicated chooser action");
    let inner = shared.inner.lock();
    assert_eq!(inner.activity_sequence, dedicated_before.0 + 1);
    assert_eq!(inner.panes.terminal_input_sequence, dedicated_before.1 + 1);
    assert_eq!(
        inner.clients[&client]
            .choose_buffer
            .as_ref()
            .unwrap()
            .rendered,
        read_only_before.2
    );
    assert!(inner.engine.state.pane(pane).expect("chooser pane").bell);
}

#[test]
fn cross_session_chooser_activation_touches_source_input_then_target_attach() {
    let shared = Arc::new(Shared::new(1));
    let (source, _, source_pane) = switch_test_session(&shared, "chooser-activity-source");
    let (target, _, _) = switch_test_session(&shared, "chooser-activity-target");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("chooser-activity-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .attach(client, source)
        .expect("attach source session");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, source_pane)
        .expect("source chooser context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("choose-tree", ["-s"]),
        )
        .expect("open sessions chooser");
    let (target_row, activity_sequence, source_sort, target_sort) = {
        let inner = shared.inner.lock();
        let chooser = inner.clients[&client].choose_tree.as_ref().unwrap();
        let target_row = chooser
            .rendered
            .items
            .iter()
            .position(|item| item.target == ChooseTreeTarget::Session(target))
            .and_then(|row| u32::try_from(row).ok())
            .expect("target session row");
        (
            target_row,
            inner.activity_sequence,
            inner.engine.state.sessions[&source].sort_activity,
            inner.engine.state.sessions[&target].sort_activity,
        )
    };

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ChooseTree {
                action: ChooseTreeAction::ActivateIndex(target_row),
            },
        )
        .expect("activate target session");

    let inner = shared.inner.lock();
    assert_eq!(client_attached_session(&inner, client), Some(target));
    assert!(inner.client(client).is_none_or(|c| c.choose_tree.is_none()));
    assert_eq!(inner.activity_sequence, activity_sequence + 2);
    assert_eq!(
        inner.clients[&client].activity.unwrap(),
        activity_sequence + 2
    );
    assert!(inner.engine.state.sessions[&source].sort_activity > source_sort);
    assert!(inner.engine.state.sessions[&target].sort_activity > target_sort);
    assert!(
        inner.engine.state.sessions[&target].sort_activity
            > inner.engine.state.sessions[&source].sort_activity
    );
}

#[test]
fn read_only_prompt_keys_bypass_consumption_and_preserve_prompt_and_bell() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "read-only-prompt", "prompt");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("read-only-prompt-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .attach(client, session)
        .expect("attach prompt client");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("prompt context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("bind-key", ["-n", "x", "copy-mode"]),
        )
        .expect("bind read-only-safe root command");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-p", "readonly"]),
        )
        .expect("open prompt");
    let initial = {
        let mut inner = shared.inner.lock();
        inner.client_flags.insert(client);
        assert!(inner.engine.state.set_pane_bell(pane, true));
        (
            inner.activity_sequence,
            inner.panes.terminal_input_sequence,
            inner.clients[&client]
                .command_prompt
                .as_ref()
                .unwrap()
                .input
                .clone(),
        )
    };

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(KeyCode::Character('x'), Modifiers::default(), Some("x")),
                text_follows: false,
            },
        )
        .expect("bypass read-only prompt with root key");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.activity_sequence, initial.0 + 1);
        assert_eq!(inner.panes.terminal_input_sequence, initial.1 + 1);
        assert_eq!(
            inner.clients[&client]
                .command_prompt
                .as_ref()
                .unwrap()
                .input,
            initial.2
        );
        assert_eq!(
            inner.clients[&client].copy_session.as_ref().unwrap().pane,
            pane
        );
        assert!(inner.engine.state.pane(pane).expect("prompt pane").bell);
    }

    let update_before = {
        let inner = shared.inner.lock();
        (inner.activity_sequence, inner.panes.terminal_input_sequence)
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::CommandPrompt {
                action: CommandPromptAction::Update {
                    input: "blocked".to_owned(),
                    cursor: 7,
                },
            },
        )
        .expect("reject read-only native prompt update");
    let inner = shared.inner.lock();
    assert_eq!(inner.activity_sequence, update_before.0 + 1);
    assert_eq!(inner.panes.terminal_input_sequence, update_before.1 + 1);
    assert_eq!(
        inner.clients[&client]
            .command_prompt
            .as_ref()
            .unwrap()
            .input,
        initial.2
    );
    assert!(inner.engine.state.pane(pane).expect("prompt pane").bell);
}

#[test]
fn terminal_committed_text_accounts_standalone_and_paired_input_at_most_once() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "committed-terminal", "terminal");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("committed-terminal-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .attach(client, session)
        .expect("attach terminal client");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("terminal context");
    let activity = || {
        let inner = shared.inner.lock();
        (
            inner.activity_sequence,
            inner.panes.terminal_input_sequence,
            inner.engine.state.sessions[&session].sort_activity,
        )
    };

    let initial = activity();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane,
                text: "a".to_owned(),
            },
        )
        .expect("standalone writable text");
    assert_eq!(activity(), (initial.0 + 1, initial.1 + 1, initial.2 + 1));

    let empty_before = activity();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane,
                text: String::new(),
            },
        )
        .expect("empty writable text");
    assert_eq!(activity(), empty_before);

    let paired_before = activity();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: chooser_key_press("j"),
                text_follows: true,
            },
        )
        .expect("paired writable key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane,
                text: "j".to_owned(),
            },
        )
        .expect("paired writable text");
    assert_eq!(
        activity(),
        (
            paired_before.0 + 1,
            paired_before.1 + 1,
            paired_before.2 + 1
        )
    );

    {
        let mut inner = shared.inner.lock();
        inner.client_flags.insert(client);
        assert!(inner.engine.state.set_pane_bell(pane, true));
    }
    let read_only_before = activity();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane,
                text: "r".to_owned(),
            },
        )
        .expect("standalone read-only text");
    assert_eq!(
        activity(),
        (
            read_only_before.0 + 1,
            read_only_before.1 + 1,
            read_only_before.2 + 1
        )
    );
    assert!(shared.inner.lock().engine.state.pane(pane).unwrap().bell);

    let read_only_paired_before = activity();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: chooser_key_press("k"),
                text_follows: true,
            },
        )
        .expect("paired read-only key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane,
                text: "k".to_owned(),
            },
        )
        .expect("paired read-only text");
    assert_eq!(
        activity(),
        (
            read_only_paired_before.0 + 1,
            read_only_paired_before.1 + 1,
            read_only_paired_before.2 + 1
        )
    );
    let inner = shared.inner.lock();
    assert!(inner.engine.state.pane(pane).unwrap().bell);
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

#[test]
fn browser_committed_text_respects_pairing_and_modal_consumption() {
    let shared = Arc::new(Shared::new(1));
    let (session, terminal, _) =
        output_view_session_fixture(&shared, "committed-browser", "browser");
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("committed-browser-client".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared
        .attach(client, session)
        .expect("attach browser client");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, terminal)
        .expect("browser context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "browser", "about:blank"]),
        )
        .expect("create browser pane");
    let browser = context.pane.expect("browser pane");
    take_reliable_messages(&mailbox);
    let activity = || {
        let inner = shared.inner.lock();
        (inner.activity_sequence, inner.panes.terminal_input_sequence)
    };

    let standalone_before = activity();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceText {
                pane: browser,
                text: "a".to_owned(),
            },
        )
        .expect("standalone browser text");
    assert_eq!(activity(), (standalone_before.0 + 1, standalone_before.1));

    let empty_before = activity();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceText {
                pane: browser,
                text: String::new(),
            },
        )
        .expect("empty browser text");
    assert_eq!(activity(), empty_before);

    let paired_before = activity();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceKey {
                pane: browser,
                input: chooser_key_press("b"),
                text_follows: true,
            },
        )
        .expect("paired browser key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceText {
                pane: browser,
                text: "b".to_owned(),
            },
        )
        .expect("paired browser text");
    assert_eq!(activity(), (paired_before.0 + 1, paired_before.1));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-p", "browser"]),
        )
        .expect("open browser prompt");
    let prompt_before = activity();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceKey {
                pane: browser,
                input: chooser_key_press("p"),
                text_follows: true,
            },
        )
        .expect("browser prompt key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceText {
                pane: browser,
                text: "p".to_owned(),
            },
        )
        .expect("browser prompt paired text");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceText {
                pane: browser,
                text: "s".to_owned(),
            },
        )
        .expect("browser prompt standalone text");
    assert_eq!(activity(), prompt_before);
    assert_eq!(
        shared.inner.lock().clients[&client]
            .command_prompt
            .as_ref()
            .unwrap()
            .input,
        "ps"
    );
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::CommandPrompt {
                action: CommandPromptAction::Close,
            },
        )
        .expect("close browser prompt");

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-buffer", ["-b", "browser", "value"]),
        )
        .expect("create browser chooser buffer");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("choose-buffer", [] as [&str; 0]),
        )
        .expect("open browser chooser");
    let chooser_before = activity();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceKey {
                pane: browser,
                input: chooser_key_press("q"),
                text_follows: true,
            },
        )
        .expect("browser chooser key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceText {
                pane: browser,
                text: "q".to_owned(),
            },
        )
        .expect("browser chooser text");
    assert_eq!(activity(), chooser_before);
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.choose_buffer.is_some())
        );
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

    shared.inner.lock().client_flags.insert(client);
    let read_only_before = activity();
    for input in [
        InputMessage::BrowserSurfaceText {
            pane: browser,
            text: "r".to_owned(),
        },
        InputMessage::BrowserSurfaceKey {
            pane: browser,
            input: chooser_key_press("x"),
            text_follows: true,
        },
        InputMessage::BrowserSurfaceText {
            pane: browser,
            text: "x".to_owned(),
        },
        InputMessage::BrowserSurfaceText {
            pane: browser,
            text: String::new(),
        },
    ] {
        shared
            .input(client, ClientKind::Interactive, &mut context, input)
            .expect("read-only browser input");
    }
    assert_eq!(activity(), read_only_before);
    let inner = shared.inner.lock();
    assert!(
        inner
            .client(client)
            .is_some_and(|c| c.choose_buffer.is_some())
    );
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

#[test]
fn terminal_committed_text_inherits_writable_and_read_only_modal_results() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "committed-modals", "modals");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("committed-modal-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(client, session).expect("attach modal client");
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("modal context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-buffer", ["-b", "modal", "value"]),
        )
        .expect("create modal buffer");
    let activity = || {
        let inner = shared.inner.lock();
        (inner.activity_sequence, inner.panes.terminal_input_sequence)
    };
    let paired = |context: &mut ExecutionContext, character: &str| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::Key {
                    pane,
                    input: chooser_key_press(character),
                    text_follows: true,
                },
            )
            .expect("modal key");
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::Text {
                    pane,
                    text: character.to_owned(),
                },
            )
            .expect("modal text");
    };

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("choose-buffer", [] as [&str; 0]),
        )
        .expect("open writable chooser");
    let chooser_before = activity();
    paired(&mut context, "q");
    assert_eq!(activity(), (chooser_before.0 + 1, chooser_before.1 + 1));
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.choose_buffer.is_none())
        );
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

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-p", "writable"]),
        )
        .expect("open writable prompt");
    let prompt_before = activity();
    paired(&mut context, "p");
    assert_eq!(activity(), prompt_before);
    assert_eq!(
        shared.inner.lock().clients[&client]
            .command_prompt
            .as_ref()
            .unwrap()
            .input,
        "p"
    );
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::CommandPrompt {
                action: CommandPromptAction::Close,
            },
        )
        .expect("close writable prompt");

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-panes", ["-d", "0"]),
        )
        .expect("open writable display-panes");
    let select_key = {
        let inner = shared.inner.lock();
        char::from(
            inner.clients[&client]
                .display_panes
                .as_ref()
                .unwrap()
                .state
                .indicators
                .iter()
                .find(|indicator| indicator.pane == pane)
                .expect("source indicator")
                .select_key,
        )
        .to_string()
    };
    let display_before = activity();
    paired(&mut context, &select_key);
    assert_eq!(activity(), display_before);
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.display_panes.is_none())
        );
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

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("choose-buffer", [] as [&str; 0]),
        )
        .expect("open read-only chooser");
    {
        let mut inner = shared.inner.lock();
        inner.client_flags.insert(client);
        inner.engine.state.set_pane_bell(pane, true);
    }
    let read_only_chooser_before = activity();
    paired(&mut context, "z");
    assert_eq!(
        activity(),
        (
            read_only_chooser_before.0 + 1,
            read_only_chooser_before.1 + 1
        )
    );
    {
        let mut inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.choose_buffer.is_some())
        );
        assert!(inner.engine.state.pane(pane).unwrap().bell);
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.pending_committed_text.is_none())
        );
        inner.client_flags.remove(client);
        inner
            .client_mut(client)
            .and_then(|c| c.choose_buffer.take());
    }

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-p", "read-only"]),
        )
        .expect("open read-only prompt");
    shared.inner.lock().client_flags.insert(client);
    let read_only_prompt_before = activity();
    paired(&mut context, "z");
    assert_eq!(
        activity(),
        (read_only_prompt_before.0 + 1, read_only_prompt_before.1 + 1)
    );
    {
        let mut inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .command_prompt
                .as_ref()
                .unwrap()
                .input,
            ""
        );
        assert!(inner.engine.state.pane(pane).unwrap().bell);
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.pending_committed_text.is_none())
        );
        inner.client_flags.remove(client);
        inner
            .client_mut(client)
            .and_then(|c| c.command_prompt.take());
    }

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-panes", ["-d", "0"]),
        )
        .expect("open read-only display-panes");
    shared.inner.lock().client_flags.insert(client);
    let read_only_display_before = activity();
    paired(&mut context, "z");
    assert_eq!(
        activity(),
        (
            read_only_display_before.0 + 1,
            read_only_display_before.1 + 1
        )
    );
    let inner = shared.inner.lock();
    assert!(
        inner
            .client(client)
            .is_some_and(|c| c.display_panes.is_some())
    );
    assert!(inner.engine.state.pane(pane).unwrap().bell);
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

#[test]
fn committed_text_fifo_handles_overlap_mismatch_switch_and_cleanup() {
    let shared = Arc::new(Shared::new(1));
    let (session, terminal, _) = output_view_session_fixture(&shared, "committed-fifo", "fifo");
    let (target, _, target_pane) = switch_test_session(&shared, "committed-switch-target");
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("committed-fifo-client".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(client, session).expect("attach fifo client");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, terminal)
        .expect("fifo context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "browser", "about:blank"]),
        )
        .expect("create fifo browser");
    let browser = context.pane.expect("fifo browser");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "browser", "about:blank"]),
        )
        .expect("create second fifo browser");
    let second_browser = context.pane.expect("second fifo browser");
    take_reliable_messages(&mailbox);
    let activity = || {
        let inner = shared.inner.lock();
        (inner.activity_sequence, inner.panes.terminal_input_sequence)
    };

    let overlap_before = activity();
    for character in ["a", "b"] {
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::Key {
                    pane: terminal,
                    input: chooser_key_press(character),
                    text_follows: true,
                },
            )
            .expect("overlapping key");
    }
    assert_eq!(
        shared.inner.lock().clients[&client]
            .pending_committed_text
            .as_ref()
            .unwrap()
            .len(),
        2
    );
    for character in ["a", "b"] {
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::Text {
                    pane: terminal,
                    text: character.to_owned(),
                },
            )
            .expect("overlapping text");
    }
    assert_eq!(activity(), (overlap_before.0 + 2, overlap_before.1 + 2));
    assert!(shared.read_client(client, |c| {
        c.is_none_or(|c| c.pending_committed_text.is_none())
    }));

    let mismatch_before = activity();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: terminal,
                input: chooser_key_press("c"),
                text_follows: true,
            },
        )
        .expect("terminal mismatch key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceKey {
                pane: browser,
                input: chooser_key_press("d"),
                text_follows: true,
            },
        )
        .expect("browser mismatch key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceText {
                pane: browser,
                text: "d".to_owned(),
            },
        )
        .expect("forward-matched browser text");
    assert!(shared.read_client(client, |c| {
        c.is_none_or(|c| c.pending_committed_text.is_none())
    }));
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane: terminal,
                text: "c".to_owned(),
            },
        )
        .expect("stale terminal text becomes standalone");
    assert_eq!(activity(), (mismatch_before.0 + 3, mismatch_before.1 + 2));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("bind-key", ["-n", "x", "focus-sidebar"]),
        )
        .expect("bind second browser key");
    take_reliable_messages(&mailbox);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: browser,
                input: chooser_key_press("w"),
                text_follows: true,
            },
        )
        .expect("strand first browser key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: second_browser,
                input: chooser_key_press("x"),
                text_follows: true,
            },
        )
        .expect("run second browser binding");
    let binding_messages = take_reliable_messages(&mailbox);
    assert_eq!(
        binding_messages
            .iter()
            .filter(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::FocusSidebar,
                    ..
                })
            ))
            .count(),
        1
    );
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .pending_committed_text
                .as_ref()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            (*inner.clients[&client].suppressed_text.as_ref().unwrap()),
            BTreeMap::from([('x', 1)])
        );
    }
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane: second_browser,
                text: "x".to_owned(),
            },
        )
        .expect("match second browser text");
    let text_messages = take_reliable_messages(&mailbox);
    assert!(!text_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::FocusSidebar | EventPayload::BrowserCommand { .. },
            ..
        })
    )));
    {
        let inner = shared.inner.lock();
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
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                ["-n", "x", "switch-client", "-t", "committed-switch-target"],
            ),
        )
        .expect("bind internal switch");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: terminal,
                input: chooser_key_press("x"),
                text_follows: true,
            },
        )
        .expect("run internal switch binding");
    {
        let inner = shared.inner.lock();
        assert_eq!(client_attached_session(&inner, client), Some(target));
        assert_eq!(
            inner.clients[&client]
                .pending_committed_text
                .as_ref()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            (*inner.clients[&client].suppressed_text.as_ref().unwrap()),
            BTreeMap::from([('x', 1)])
        );
    }
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane: terminal,
                text: "x".to_owned(),
            },
        )
        .expect("consume trailing text after internal switch");
    {
        let inner = shared.inner.lock();
        assert_eq!(client_attached_session(&inner, client), Some(target));
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

    shared.enqueue_pending_committed_text(
        client,
        target_pane,
        CommittedTextLane::Terminal,
        &chooser_key_press("y"),
        true,
    );
    shared.detach(client);
    assert!(shared.read_client(client, |c| {
        c.is_none_or(|c| c.pending_committed_text.is_none())
    }));
    shared.attach(client, target).expect("reattach fifo client");
    shared.enqueue_pending_committed_text(
        client,
        target_pane,
        CommittedTextLane::Terminal,
        &chooser_key_press("y"),
        true,
    );
    shared.unregister(client);
    assert!(shared.read_client(client, |c| {
        c.is_none_or(|c| c.pending_committed_text.is_none())
    }));
    let (reconnected, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("committed-fifo-reconnect".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    assert_ne!(reconnected, client);
    assert!(shared.read_client(reconnected, |c| {
        c.is_none_or(|c| c.pending_committed_text.is_none())
    }));
}

#[test]
fn committed_text_fifo_is_bounded_and_empty_text_retires_one_suppression() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(77);
    let pane = PaneId(88);
    let orphan = chooser_key_press("o");
    shared.suppress_committed_character(client, pane, CommittedTextLane::Terminal, &orphan, true);
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.suppressed_text.is_none())));
    let evicted = chooser_key_press("a");
    shared.enqueue_pending_committed_text(
        client,
        pane,
        CommittedTextLane::Terminal,
        &evicted,
        true,
    );
    shared.suppress_committed_character(client, pane, CommittedTextLane::Terminal, &evicted, true);
    for index in 1..=MAX_PENDING_COMMITTED_TEXT {
        let character = char::from(b'a' + u8::try_from(index % 26).unwrap());
        let input = chooser_key_press(&character.to_string());
        shared.enqueue_pending_committed_text(
            client,
            pane,
            CommittedTextLane::Terminal,
            &input,
            true,
        );
    }
    assert_eq!(
        shared.inner.lock().clients[&client]
            .pending_committed_text
            .as_ref()
            .unwrap()
            .len(),
        MAX_PENDING_COMMITTED_TEXT
    );
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.suppressed_text.is_none())));

    shared.clear_pending_committed_text(client);
    let input = chooser_key_press("z");
    shared.enqueue_pending_committed_text(client, pane, CommittedTextLane::Terminal, &input, true);
    shared.suppress_committed_character(client, pane, CommittedTextLane::Terminal, &input, true);
    assert!(
        shared
            .match_pending_committed_text(client, pane, CommittedTextLane::Terminal, true,)
            .is_some()
    );
    let inner = shared.inner.lock();
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
