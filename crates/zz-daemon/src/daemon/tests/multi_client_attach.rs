use super::*;

#[test]
fn multi_client_attach_uses_latest_active_geometry_and_keeps_independent_views() {
    let shared = Arc::new(Shared::new(1));
    let first_mailbox = OutboundMailbox::new();
    let second_mailbox = OutboundMailbox::new();
    let third_mailbox = OutboundMailbox::new();
    let (first_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&first_mailbox),
    );
    let (second_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&second_mailbox),
    );
    let (third_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&third_mailbox),
    );
    let fixture = std::iter::once("zz-terminal-ready".to_owned())
        .chain((0..100).map(|line| format!("slice1-line-{line}")))
        .collect::<Vec<_>>()
        .join("\n");
    let (session, pane, terminal) = output_view_session_fixture(&shared, "shared", fixture);

    shared
        .attach(first_client, session)
        .expect("attach first client");
    shared
        .attach(second_client, session)
        .expect("attach second client");
    assert_eq!(
        shared.inner.lock().attached.get(&session),
        Some(&BTreeSet::from([first_client, second_client]))
    );
    wait_for_viewport(
        &terminal,
        TerminalViewId(first_client.0),
        "first attached terminal view never became available",
        |viewport| viewport_text(viewport).contains("zz-terminal-ready"),
    );
    wait_for_viewport(
        &terminal,
        TerminalViewId(second_client.0),
        "second attached terminal view never became available",
        |viewport| viewport_text(viewport).contains("zz-terminal-ready"),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while shared
        .inner
        .lock()
        .engine
        .state
        .pane(pane)
        .is_none_or(|pane| pane.title != "shared fixture")
    {
        assert!(Instant::now() < deadline, "the fixture title did not sync");
        thread::sleep(Duration::from_millis(5));
    }
    shared.send_resync(first_client, &first_mailbox);
    shared.send_resync(second_client, &second_mailbox);

    let mut first_state = TerminalTestState::default();
    let mut second_state = TerminalTestState::default();
    wait_for_mailbox_terminal(&first_mailbox, &mut first_state, |message, _| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalViewport { pane: target, .. },
                ..
            }) if *target == pane
        )
    });
    wait_for_mailbox_terminal(&second_mailbox, &mut second_state, |message, _| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalViewport { pane: target, .. },
                ..
            }) if *target == pane
        )
    });

    let mut first_context = ExecutionContext::default();
    let mut second_context = ExecutionContext::default();
    shared
        .input(
            first_client,
            ClientKind::Interactive,
            &mut first_context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 200,
                rows: 30,
                cell_width_px: 8,
                cell_height_px: 18,
            },
        )
        .expect("first geometry");
    shared
        .input(
            second_client,
            ClientKind::Interactive,
            &mut second_context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 100,
                rows: 60,
                cell_width_px: 7,
                cell_height_px: 16,
            },
        )
        .expect("second geometry");
    let effective = terminal_resize_for_pane(&shared.inner.lock(), pane)
        .expect("effective shared geometry")
        .1;
    assert_eq!(
        effective,
        TerminalGeometry {
            columns: 100,
            rows: 60,
            cell_width_px: 7,
            cell_height_px: 16,
        },
        "the latest attached client owns one whole geometry before later input"
    );
    assert_eq!(
        terminal_geometry_owner(&shared.inner.lock(), pane),
        Some(second_client)
    );
    wait_for_terminal_dimensions(&terminal, TerminalViewId(first_client.0), 100, 60);
    let attach_generation = shared.inner.lock().engine.state.generation();
    shared
        .attach(first_client, session)
        .expect("reattach first client without focus events");
    {
        let inner = shared.inner.lock();
        assert!(!inner.engine.focus_events());
        assert_eq!(terminal_geometry_owner(&inner, pane), Some(first_client));
        assert_eq!(inner.engine.pane_geometry(pane), Some((200, 30)));
        assert!(inner.engine.state.generation() > attach_generation);
        assert!(
            (*inner.clients[&first_client]
                .terminal_input_sequence
                .as_ref()
                .unwrap())
                > (*inner.clients[&second_client]
                    .terminal_input_sequence
                    .as_ref()
                    .unwrap())
        );
    }
    wait_for_terminal_dimensions(&terminal, TerminalViewId(first_client.0), 200, 30);
    take_reliable_messages(&first_mailbox);
    let input_generation = shared.inner.lock().engine.state.generation();

    shared
        .input(
            second_client,
            ClientKind::Interactive,
            &mut second_context,
            InputMessage::Text {
                pane,
                text: "zz-input\r".to_owned(),
            },
        )
        .expect("second client types");
    assert_eq!(
        terminal_geometry_owner(&shared.inner.lock(), pane),
        Some(second_client)
    );
    let published_generation = {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.pane_geometry(pane), Some((100, 60)));
        assert_eq!(inner.engine.state.generation(), input_generation + 1);
        assert_eq!(
            inner.publication.last_published_mux_generation,
            input_generation + 1
        );
        assert_eq!(
            terminal_resize_for_pane(&inner, pane)
                .expect("second client geometry")
                .1,
            TerminalGeometry {
                columns: 100,
                rows: 60,
                cell_width_px: 7,
                cell_height_px: 16,
            }
        );
        inner.engine.state.generation()
    };
    let peer_messages = take_reliable_messages(&first_mailbox);
    assert_eq!(
        peer_messages
            .iter()
            .filter(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Snapshot(_),
                    ..
                })
            ))
            .count(),
        1
    );
    let peer_snapshot = latest_reliable_snapshot(&peer_messages);
    assert_eq!(peer_snapshot.generation, published_generation);
    assert!(
        zz_mux::legacy_layout(&peer_snapshot.sessions[0].windows[0].layout_dump)
            .ends_with(&format!("100x60,0,0,{}", pane.0)),
        "{}",
        peer_snapshot.sessions[0].windows[0].layout_dump
    );
    wait_for_terminal_dimensions(&terminal, TerminalViewId(first_client.0), 100, 60);

    shared
        .input(
            first_client,
            ClientKind::Interactive,
            &mut first_context,
            InputMessage::Text {
                pane,
                text: "zz-input\r".to_owned(),
            },
        )
        .expect("first client types again");
    assert_eq!(
        terminal_geometry_owner(&shared.inner.lock(), pane),
        Some(first_client)
    );
    wait_for_terminal_dimensions(&terminal, TerminalViewId(first_client.0), 200, 30);
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "mouse", "off"]),
        )
        .expect("disable mouse forwarding");
    shared
        .input(
            second_client,
            ClientKind::Interactive,
            &mut second_context,
            InputMessage::ClientTerminalSize {
                columns: 100,
                rows: 60,
            },
        )
        .expect("store second client size");

    let mouse_input = |phase, button| {
        TerminalMouseInput::new(
            phase,
            button,
            PointerCellEvent {
                column: 0,
                row: 0,
                click_count: 1,
                rectangle: false,
            },
            4,
            8,
            800,
            600,
            8,
            16,
            Modifiers::default(),
            false,
        )
    };
    shared
        .input(
            second_client,
            ClientKind::Interactive,
            &mut second_context,
            InputMessage::TerminalView {
                pane,
                action: TerminalViewAction::Mouse(mouse_input(TerminalMousePhase::Motion, None)),
            },
        )
        .expect("second client moves the mouse");
    assert_eq!(
        terminal_geometry_owner(&shared.inner.lock(), pane),
        Some(first_client),
        "mouse motion must not transfer geometry ownership"
    );

    for focused in [true, false] {
        shared
            .input(
                second_client,
                ClientKind::Interactive,
                &mut second_context,
                InputMessage::TerminalView {
                    pane,
                    action: TerminalViewAction::Focus(focused),
                },
            )
            .expect("second client changes focus");
        assert_eq!(
            terminal_geometry_owner(&shared.inner.lock(), pane),
            Some(first_client),
            "focus({focused}) must not transfer geometry ownership: a window-manager \
                 event is not someone typing, and letting it claim the pty makes every \
                 focus change resize the terminal out from under the other client"
        );
    }

    take_reliable_messages(&first_mailbox);
    let mouse_press = InputMessage::TerminalView {
        pane,
        action: TerminalViewAction::Mouse(mouse_input(
            TerminalMousePhase::Press,
            Some(TerminalMouseButton::Left),
        )),
    };
    assert!(terminal_mouse_rejected(
        &shared.inner.lock(),
        second_client,
        &mouse_press
    ));
    let mouse_generation = shared.inner.lock().engine.state.generation();
    shared
        .input(
            second_client,
            ClientKind::Interactive,
            &mut second_context,
            mouse_press,
        )
        .expect("reject second client mouse press after recording activity");
    assert_eq!(
        terminal_geometry_owner(&shared.inner.lock(), pane),
        Some(second_client),
        "mouse press must transfer geometry ownership"
    );
    let mouse_published_generation = {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.pane_geometry(pane), Some((100, 60)));
        assert_eq!(inner.engine.state.generation(), mouse_generation + 1);
        assert_eq!(
            inner.publication.last_published_mux_generation,
            mouse_generation + 1
        );
        inner.engine.state.generation()
    };
    let peer_messages = take_reliable_messages(&first_mailbox);
    assert_eq!(
        peer_messages
            .iter()
            .filter(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Snapshot(_),
                    ..
                })
            ))
            .count(),
        1
    );
    let peer_snapshot = latest_reliable_snapshot(&peer_messages);
    assert_eq!(peer_snapshot.generation, mouse_published_generation);
    assert!(
        zz_mux::legacy_layout(&peer_snapshot.sessions[0].windows[0].layout_dump)
            .ends_with(&format!("100x60,0,0,{}", pane.0)),
        "{}",
        peer_snapshot.sessions[0].windows[0].layout_dump
    );
    wait_for_terminal_dimensions(&terminal, TerminalViewId(first_client.0), 100, 60);

    wait_for_mailbox_terminal(&first_mailbox, &mut first_state, |_, state| {
        state
            .viewports
            .get(&pane)
            .is_some_and(|viewport| viewport.columns == 100 && viewport.rows == 60)
    });
    wait_for_mailbox_terminal(&second_mailbox, &mut second_state, |_, state| {
        state
            .viewports
            .get(&pane)
            .is_some_and(|viewport| viewport.columns == 100 && viewport.rows == 60)
    });

    wait_for_viewport(
        &terminal,
        TerminalViewId(first_client.0),
        "output fixture did not retain enough rows to test independent views",
        |viewport| {
            viewport
                .scrollbar
                .total
                .saturating_sub(viewport.scrollbar.len)
                >= 10
        },
    );
    for view in [
        TerminalViewId(first_client.0),
        TerminalViewId(second_client.0),
    ] {
        terminal.view_action(view, zz_terminal::TerminalViewAction::ScrollBottom);
        wait_for_viewport(
            &terminal,
            view,
            "terminal view did not reach the bottom before the isolation check",
            |viewport| {
                viewport
                    .scrollbar
                    .offset
                    .saturating_add(viewport.scrollbar.len)
                    == viewport.scrollbar.total
            },
        );
    }
    wait_for_mailbox_terminal(&first_mailbox, &mut first_state, |_, state| {
        state.viewports.get(&pane).is_some_and(|viewport| {
            viewport
                .scrollbar
                .offset
                .saturating_add(viewport.scrollbar.len)
                == viewport.scrollbar.total
        })
    });
    wait_for_mailbox_terminal(&second_mailbox, &mut second_state, |_, state| {
        state.viewports.get(&pane).is_some_and(|viewport| {
            viewport
                .scrollbar
                .offset
                .saturating_add(viewport.scrollbar.len)
                == viewport.scrollbar.total
        })
    });
    let second_before = second_state.viewports[&pane].view_generation;
    terminal.view_action(
        TerminalViewId(first_client.0),
        zz_terminal::TerminalViewAction::ScrollLines(-10),
    );
    wait_for_mailbox_terminal(&first_mailbox, &mut first_state, |_, state| {
        state.viewports.get(&pane).is_some_and(|viewport| {
            viewport
                .scrollbar
                .offset
                .saturating_add(viewport.scrollbar.len)
                < viewport.scrollbar.total
        })
    });
    wait_for_mailbox_terminal(&second_mailbox, &mut second_state, |_, state| {
        state.viewports.get(&pane).is_some_and(|viewport| {
            viewport.view_generation > second_before
                && viewport
                    .scrollbar
                    .offset
                    .saturating_add(viewport.scrollbar.len)
                    == viewport.scrollbar.total
        })
    });
    assert!(
        first_state.viewports[&pane]
            .scrollbar
            .offset
            .saturating_add(first_state.viewports[&pane].scrollbar.len)
            < first_state.viewports[&pane].scrollbar.total,
        "the scrolled client must have an independent viewport"
    );
    assert_eq!(
        second_state.viewports[&pane]
            .scrollbar
            .offset
            .saturating_add(second_state.viewports[&pane].scrollbar.len),
        second_state.viewports[&pane].scrollbar.total,
    );

    shared.detach(second_client);
    assert_eq!(
        terminal_geometry_owner(&shared.inner.lock(), pane),
        Some(first_client),
        "the surviving viewer owns the geometry after the active owner detaches"
    );
    wait_for_mailbox_terminal(&first_mailbox, &mut first_state, |_, state| {
        state
            .viewports
            .get(&pane)
            .is_some_and(|viewport| viewport.columns == 200 && viewport.rows == 30)
    });

    shared
        .attach(third_client, session)
        .expect("attach inactive third client");
    let mut third_context = ExecutionContext::default();
    shared
        .input(
            third_client,
            ClientKind::Interactive,
            &mut third_context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 60,
                rows: 20,
                cell_width_px: 6,
                cell_height_px: 14,
            },
        )
        .expect("third client geometry");
    assert_eq!(
        terminal_geometry_owner(&shared.inner.lock(), pane),
        Some(third_client),
        "the latest attach owns geometry as soon as that client reports it"
    );
    assert_eq!(
        terminal_resize_for_pane(&shared.inner.lock(), pane)
            .expect("latest third client geometry")
            .1,
        TerminalGeometry {
            columns: 60,
            rows: 20,
            cell_width_px: 6,
            cell_height_px: 14,
        }
    );
    wait_for_terminal_dimensions(&terminal, TerminalViewId(first_client.0), 60, 20);

    let inner = shared.inner.lock();
    assert_eq!(
        inner.attached.get(&session),
        Some(&BTreeSet::from([first_client, third_client]))
    );
    assert!(
        inner
            .panes
            .terminal_geometries
            .values()
            .all(|geometries| !geometries.contains_key(&second_client))
    );
}

#[test]
fn detach_releases_and_reattach_reclaims_geometry_ownership() {
    let shared = Arc::new(Shared::new(1));
    let first_mailbox = OutboundMailbox::new();
    let second_mailbox = OutboundMailbox::new();
    let (first_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&first_mailbox),
    );
    let (second_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&second_mailbox),
    );
    let mut setup = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut setup,
            &CommandInvocation::new("new-session", ["-d", "-s", "yield"]),
        )
        .expect("create shared session");
    let session = setup.session.expect("shared session");
    let pane = setup.pane.expect("shared pane");

    let geometry = |columns, rows| InputMessage::ResizeTerminal {
        pane,
        columns,
        rows,
        cell_width_px: 8,
        cell_height_px: 18,
    };
    let typing = || InputMessage::Text {
        pane,
        text: "true\n".to_owned(),
    };

    shared
        .attach(first_client, session)
        .expect("attach first client");
    shared
        .attach(second_client, session)
        .expect("attach second client");
    let mut first_context = ExecutionContext::default();
    let mut second_context = ExecutionContext::default();
    shared
        .input(
            first_client,
            ClientKind::Interactive,
            &mut first_context,
            geometry(200, 30),
        )
        .expect("first geometry");
    shared
        .input(
            second_client,
            ClientKind::Interactive,
            &mut second_context,
            geometry(100, 60),
        )
        .expect("second geometry");
    shared
        .input(
            second_client,
            ClientKind::Interactive,
            &mut second_context,
            typing(),
        )
        .expect("second client types");
    shared
        .input(
            first_client,
            ClientKind::Interactive,
            &mut first_context,
            typing(),
        )
        .expect("first client types");
    assert_eq!(
        terminal_geometry_owner(&shared.inner.lock(), pane),
        Some(first_client)
    );

    shared.detach(first_client);
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(first_client)
                .is_none_or(|c| c.terminal_input_sequence.is_none())
        );
        assert_eq!(terminal_geometry_owner(&inner, pane), Some(second_client));
    }

    for stray in [geometry(300, 40), typing()] {
        let error = shared
            .input(
                first_client,
                ClientKind::Interactive,
                &mut first_context,
                stray,
            )
            .expect_err("detached clients cannot report input");
        assert!(matches!(
            error,
            DaemonError::Server(ServerError::PaneNotAttached(target)) if target == pane
        ));
    }
    {
        let inner = shared.inner.lock();
        assert!(!inner.panes.terminal_geometries[&pane].contains_key(&first_client));
        assert!(
            inner
                .client(first_client)
                .is_none_or(|c| c.terminal_input_sequence.is_none())
        );
        assert_eq!(terminal_geometry_owner(&inner, pane), Some(second_client));
    }

    shared
        .attach(first_client, session)
        .expect("re-attach first client");
    shared
        .input(
            first_client,
            ClientKind::Interactive,
            &mut first_context,
            geometry(200, 30),
        )
        .expect("first geometry after re-attach");
    assert_eq!(
        terminal_geometry_owner(&shared.inner.lock(), pane),
        Some(first_client),
        "re-attaching claims latest ownership once the client reports geometry"
    );

    shared
        .input(
            first_client,
            ClientKind::Interactive,
            &mut first_context,
            typing(),
        )
        .expect("first client types after re-attach");
    assert_eq!(
        terminal_geometry_owner(&shared.inner.lock(), pane),
        Some(first_client),
        "later typing retains ownership"
    );
}

#[test]
fn unattached_client_key_input_builds_no_key_state_and_runs_no_binding() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut setup = ExecutionContext::default();
    for command in [
        CommandInvocation::new("set-option", ["-g", "prefix", "C-a"]),
        CommandInvocation::new("bind-key", ["c", "new-window"]),
        CommandInvocation::new("new-session", ["-d", "-s", "stray"]),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut setup,
                &command,
            )
            .expect("prepare stray session");
    }
    let pane = setup.pane.expect("stray pane");
    let windows = shared.inner.lock().engine.state.windows.len();
    take_reliable_messages(&mailbox);

    let mut context = ExecutionContext::default();
    let inputs = [
        InputMessage::Key {
            pane,
            input: test_key(
                KeyCode::Character('a'),
                Modifiers::new(false, true, false, false),
                None,
            ),
            text_follows: false,
        },
        InputMessage::Key {
            pane,
            input: test_key(KeyCode::Character('c'), Modifiers::default(), Some("c")),
            text_follows: true,
        },
        InputMessage::Text {
            pane,
            text: "c".to_owned(),
        },
    ];
    for input in inputs {
        let error = shared
            .input(client, ClientKind::Interactive, &mut context, input)
            .expect_err("clients without an attached session cannot send input");
        assert!(matches!(
            error,
            DaemonError::Server(ServerError::PaneNotAttached(target)) if target == pane
        ));
    }

    {
        let inner = shared.inner.lock();
        assert!(inner.client(client).is_none_or(|c| c.key_engine.is_none()));
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.swallowed_keys.is_none())
        );
        assert!(!prefix_published(&inner, client));
        assert_eq!(
            inner.engine.state.windows.len(),
            windows,
            "a prefix binding must not run for an unattached client"
        );
    }
    assert!(!take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::PrefixArmed { .. },
                ..
            })
        )
    }));
}

#[test]
fn multi_client_presence_stamps_names_focus_self_and_nameless_fallback() {
    let shared = Arc::new(Shared::new(1));
    let desktop_mailbox = OutboundMailbox::new();
    let laptop_mailbox = OutboundMailbox::new();
    let nameless_mailbox = OutboundMailbox::new();
    let (desktop, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("desktop".to_owned()),
        None,
        Arc::clone(&desktop_mailbox),
    );
    let (laptop, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("laptop".to_owned()),
        None,
        Arc::clone(&laptop_mailbox),
    );
    let (nameless, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&nameless_mailbox),
    );
    shared
        .inner
        .lock()
        .client_entry(desktop)
        .tty
        .replace("/dev/pts/40".to_owned());

    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "presence", "-n", "agent"]),
        )
        .expect("create presence session");
    let session = context.session.expect("presence session");
    let agent_window = context.window.expect("agent window");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-window", ["-n", "logs"]),
        )
        .expect("create logs window");
    let logs_window = context.window.expect("logs window");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("select-window", ["-t", &agent_window.to_string()]),
        )
        .expect("restore agent default");

    shared.attach(desktop, session).expect("attach desktop");
    shared.attach(laptop, session).expect("attach laptop");
    take_reliable_messages(&desktop_mailbox);
    take_reliable_messages(&laptop_mailbox);

    let mut laptop_context = context.clone();
    shared
        .execute(
            laptop,
            ClientKind::Interactive,
            &mut laptop_context,
            &CommandInvocation::new("select-window", ["-t", &logs_window.to_string()]),
        )
        .expect("laptop focuses logs");

    let desktop_messages = take_reliable_messages(&desktop_mailbox);
    let laptop_messages = take_reliable_messages(&laptop_mailbox);
    let desktop_snapshot = latest_reliable_snapshot(&desktop_messages);
    let laptop_snapshot = latest_reliable_snapshot(&laptop_messages);
    let desktop_session = desktop_snapshot
        .sessions
        .iter()
        .find(|candidate| candidate.id == session)
        .expect("desktop session snapshot");
    let laptop_session = laptop_snapshot
        .sessions
        .iter()
        .find(|candidate| candidate.id == session)
        .expect("laptop session snapshot");

    assert_eq!(desktop_snapshot.focused_window, Some(agent_window));
    assert_eq!(laptop_snapshot.focused_window, Some(logs_window));
    assert!(desktop_session.viewers.iter().any(|viewer| {
        viewer.name == "desktop" && viewer.window == agent_window && viewer.is_self
    }));
    assert!(desktop_session.viewers.iter().any(|viewer| {
        viewer.name == "laptop" && viewer.window == logs_window && !viewer.is_self
    }));
    assert!(laptop_session.viewers.iter().any(|viewer| {
        viewer.name == "desktop" && viewer.window == agent_window && !viewer.is_self
    }));
    assert!(laptop_session.viewers.iter().any(|viewer| {
        viewer.name == "laptop" && viewer.window == logs_window && viewer.is_self
    }));

    let nameless_snapshot = shared
        .attach(nameless, session)
        .expect("attach nameless client");
    shared.publish_snapshot();
    let fallback = format!("device-{}", nameless.0);
    assert!(nameless_snapshot.sessions[0].viewers.iter().any(|viewer| {
        viewer.name == fallback && viewer.window == logs_window && viewer.is_self
    }));
    let desktop_messages = take_reliable_messages(&desktop_mailbox);
    assert!(
        latest_reliable_snapshot(&desktop_messages).sessions[0]
            .viewers
            .iter()
            .any(|viewer| viewer.name == fallback && !viewer.is_self)
    );
}

#[test]
fn detach_client_command_notifies_its_issuer() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("terminal".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "work"]),
        )
        .expect("create session");
    let session = context.session.expect("created session");
    shared
        .attach(client, session)
        .expect("attach terminal client");
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("detach-client", [] as [&str; 0]),
        )
        .expect("detach through mux command");

    assert!(take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::Detached {
                        session: detached,
                        by: None,
                        reason,
                        ..
                    },
                ..
            }) if *detached == session && reason.is_requested()
        )
    }));
    assert!(
        shared
            .inner
            .lock()
            .attached
            .values()
            .all(|clients| !clients.contains(&client))
    );
}

#[test]
fn detach_client_dash_a_kicks_every_peer_and_keeps_the_caller() {
    let shared = Arc::new(Shared::new(1));
    let mine = OutboundMailbox::new();
    let theirs = OutboundMailbox::new();
    let (caller, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("desktop".to_owned()),
        None,
        Arc::clone(&mine),
    );
    let (peer, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("laptop".to_owned()),
        None,
        Arc::clone(&theirs),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "work"]),
        )
        .expect("create session");
    let session = context.session.expect("created session");
    shared.attach(caller, session).expect("attach caller");
    shared.attach(peer, session).expect("attach peer");
    take_reliable_messages(&mine);
    take_reliable_messages(&theirs);

    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("detach-client", ["-a"]),
        )
        .expect("detach every other client");

    assert!(take_reliable_messages(&theirs).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::Detached {
                        session: detached,
                        by: None,
                        reason,
                        ..
                    },
                ..
            }) if *detached == session && reason.is_requested()
        )
    }));
    let inner = shared.inner.lock();
    assert!(
        inner.attached[&session].contains(&caller),
        "-a must never detach its own client"
    );
    assert!(!inner.attached[&session].contains(&peer));
}

#[test]
fn detach_client_exec_leaves_control_victims_attached() {
    let shared = Arc::new(Shared::new(1));
    let mine = OutboundMailbox::new();
    let theirs = OutboundMailbox::new();
    let (caller, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("desktop".to_owned()),
        None,
        Arc::clone(&mine),
    );
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("observer".to_owned()),
        None,
        Arc::clone(&theirs),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "work"]),
        )
        .expect("create session");
    let session = context.session.expect("created session");
    shared.attach(caller, session).expect("attach caller");
    shared
        .attach(control, session)
        .expect("attach control client");
    take_reliable_messages(&mine);
    take_reliable_messages(&theirs);

    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("detach-client", ["-a", "-E", "exit 5"]),
        )
        .expect("exec every other client");

    assert!(
        !take_reliable_messages(&theirs).iter().any(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Detached { .. },
                    ..
                })
            )
        }),
        "MSG_EXEC only acts in the pty client's attached dispatch"
    );
    assert!(shared.inner.lock().attached[&session].contains(&control));

    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("detach-client", ["-a"]),
        )
        .expect("plain detach still evicts the control victim");
    assert!(!shared.inner.lock().attached[&session].contains(&control));
}

#[test]
fn detach_client_dash_s_clears_the_session_including_the_caller() {
    let shared = Arc::new(Shared::new(1));
    let mine = OutboundMailbox::new();
    let theirs = OutboundMailbox::new();
    let (caller, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("desktop".to_owned()),
        None,
        Arc::clone(&mine),
    );
    let (peer, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("laptop".to_owned()),
        None,
        Arc::clone(&theirs),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "work"]),
        )
        .expect("create session");
    let session = context.session.expect("created session");
    shared.attach(caller, session).expect("attach caller");
    shared.attach(peer, session).expect("attach peer");
    take_reliable_messages(&mine);
    take_reliable_messages(&theirs);

    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("detach-client", ["-a", "-s", "work"]),
        )
        .expect("detach the session");

    assert!(take_reliable_messages(&mine).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::Detached {
                        session: detached,
                        by: None,
                        reason,
                        ..
                    },
                ..
            }) if *detached == session && reason.is_requested()
        )
    }));
    let inner = shared.inner.lock();
    assert!(
        inner.attached.get(&session).is_none_or(BTreeSet::is_empty),
        "-s empties the session"
    );
}

#[test]
fn detach_client_resolves_origin_and_exact_client_aliases() {
    let shared = Arc::new(Shared::new(1));
    let alpha_mailbox = OutboundMailbox::new();
    let beta_mailbox = OutboundMailbox::new();
    let gamma_mailbox = OutboundMailbox::new();
    let (alpha, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("alpha".to_owned()),
        None,
        Arc::clone(&alpha_mailbox),
    );
    let (beta, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("beta".to_owned()),
        None,
        Arc::clone(&beta_mailbox),
    );
    let (gamma, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("gamma".to_owned()),
        None,
        Arc::clone(&gamma_mailbox),
    );
    let (origin_session, origin_pane, _) =
        attached_message_fixture(&shared, "origin", &[alpha, beta]);
    let (other_session, _, _) = attached_message_fixture(&shared, "other", &[gamma]);
    let (command, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    {
        let mut inner = shared.inner.lock();
        inner.client_entry(command).origin.replace(origin_pane);
        inner.client_entry(alpha).activity.replace(10);
        inner.client_entry(beta).activity.replace(20);
        inner.client_entry(gamma).activity.replace(100);
        inner
            .client_entry(beta)
            .tty
            .replace("/dev/pts/77".to_owned());
    }
    for mailbox in [&alpha_mailbox, &beta_mailbox, &gamma_mailbox] {
        take_reliable_messages(mailbox);
    }
    let mut context = ExecutionContext::default();

    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("detach-client", [] as [&str; 0]),
        )
        .expect("detach best client on origin session");
    {
        let inner = shared.inner.lock();
        assert_eq!(client_attached_session(&inner, alpha), Some(origin_session));
        assert_eq!(client_attached_session(&inner, beta), None);
        assert_eq!(client_attached_session(&inner, gamma), Some(other_session));
    }
    assert!(take_reliable_messages(&beta_mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::Detached { session, by: None, reason, .. },
                ..
            }) if *session == origin_session && reason.is_requested()
        )
    }));

    for target in ["beta:", "/dev/pts/77", "pts/77"] {
        shared.attach(beta, origin_session).expect("reattach beta");
        take_reliable_messages(&beta_mailbox);
        shared
            .execute(
                command,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("detach-client", ["-t", target]),
            )
            .expect("detach exact alias");
        assert_eq!(client_attached_session(&shared.inner.lock(), beta), None);
        assert!(take_reliable_messages(&beta_mailbox).iter().any(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Detached { session, by: None, reason, .. },
                    ..
                }) if *session == origin_session && reason.is_requested()
            )
        }));
    }

    shared.attach(beta, origin_session).expect("reattach beta");
    let error = shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("detach-client", ["-t", "missing", "-s", "missing"]),
        )
        .expect_err("target client resolves before source session");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "can't find client: missing"
    ));
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("detach-client", ["-t", "alpha", "-s", "missing"]),
        )
        .expect("missing source session is quiet");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), alpha),
        Some(origin_session)
    );

    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("detach-client", ["-a", "-t", "alpha"]),
        )
        .expect("detach every client except explicit target");
    {
        let inner = shared.inner.lock();
        assert_eq!(client_attached_session(&inner, alpha), Some(origin_session));
        assert_eq!(client_attached_session(&inner, beta), None);
        assert_eq!(client_attached_session(&inner, gamma), None);
    }
    for mailbox in [&beta_mailbox, &gamma_mailbox] {
        assert!(take_reliable_messages(mailbox).iter().any(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Detached { by: None, reason, .. },
                    ..
                }) if reason.is_requested()
            )
        }));
    }
}

#[test]
fn read_only_detach_client_is_limited_to_itself() {
    let shared = Arc::new(Shared::new(1));
    let mine = OutboundMailbox::new();
    let theirs = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("readonly".to_owned()),
        None,
        Arc::clone(&mine),
    );
    let (peer, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("peer".to_owned()),
        None,
        Arc::clone(&theirs),
    );
    let (session, pane, _) = attached_message_fixture(&shared, "readonly", &[client, peer]);
    shared.inner.lock().client_flags.insert(client);
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("read-only context");

    for arguments in [vec!["-a"], vec!["-s", "readonly"], vec!["-t", "peer"]] {
        let error = shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("detach-client", arguments),
            )
            .expect_err("read-only detach restriction");
        assert!(matches!(
            error,
            DaemonError::Server(ServerError::InvalidCommand(message))
                if message == "client is read-only"
        ));
    }
    {
        let inner = shared.inner.lock();
        assert_eq!(client_attached_session(&inner, client), Some(session));
        assert_eq!(client_attached_session(&inner, peer), Some(session));
    }

    take_reliable_messages(&mine);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("detach-client", ["-t", "readonly:"]),
        )
        .expect("read-only client detaches itself");
    assert_eq!(client_attached_session(&shared.inner.lock(), client), None);
    assert!(take_reliable_messages(&mine).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::Detached { session: detached, by: None, reason, .. },
                ..
            }) if *detached == session && reason.is_requested()
        )
    }));
}

#[test]
fn steal_attach_notifies_peer_and_releases_focus_views_and_geometry() {
    let shared = Arc::new(Shared::new(1));
    let desktop_mailbox = OutboundMailbox::new();
    let laptop_mailbox = OutboundMailbox::new();
    let (desktop, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("desktop".to_owned()),
        None,
        Arc::clone(&desktop_mailbox),
    );
    let (laptop, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("laptop".to_owned()),
        None,
        Arc::clone(&laptop_mailbox),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "shared"]),
        )
        .expect("create shared session");
    let session = context.session.expect("shared session");
    let window = context.window.expect("shared window");
    let pane = context.pane.expect("shared pane");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);

    shared.attach(desktop, session).expect("attach desktop");
    shared.attach(laptop, session).expect("plain attach laptop");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.attached.get(&session),
            Some(&BTreeSet::from([desktop, laptop])),
            "plain attach must keep both clients attached"
        );
    }
    assert!(
        take_reliable_messages(&desktop_mailbox)
            .iter()
            .all(|message| !matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Detached { .. },
                    ..
                })
            )),
        "plain attach must not detach its peer"
    );

    let mut desktop_context = context.clone();
    let mut laptop_context = context.clone();
    shared
        .input(
            desktop,
            ClientKind::Interactive,
            &mut desktop_context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 80,
                rows: 24,
                cell_width_px: 8,
                cell_height_px: 16,
            },
        )
        .expect("desktop geometry");
    shared
        .input(
            laptop,
            ClientKind::Interactive,
            &mut laptop_context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 140,
                rows: 40,
                cell_width_px: 9,
                cell_height_px: 18,
            },
        )
        .expect("laptop geometry");
    shared
        .attach(desktop, session)
        .expect("make desktop the latest attached geometry client");
    {
        let mut inner = shared.inner.lock();
        inner.client_entry(desktop).focused_window.replace(window);
        let effective = terminal_resize_for_pane(&inner, pane)
            .expect("latest attached desktop geometry")
            .1;
        assert_eq!((effective.columns, effective.rows), (80, 24));
    }
    take_reliable_messages(&desktop_mailbox);
    take_reliable_messages(&laptop_mailbox);

    shared
        .execute(
            laptop,
            ClientKind::Interactive,
            &mut laptop_context,
            &CommandInvocation::new("attach-session", ["-d", "-t", "shared"]),
        )
        .expect("laptop steal-attaches");

    assert!(
        take_reliable_messages(&desktop_mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload:
                        EventPayload::Detached {
                            session: detached,
                            by: Some(device),
                            reason,
                            ..
                        },
                    ..
                }) if *detached == session && device == "laptop" && reason.is_evicted()
            )),
        "victim must receive the named detach event"
    );
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.attached.get(&session),
            Some(&BTreeSet::from([laptop]))
        );
        assert!(
            inner
                .client(desktop)
                .is_none_or(|c| c.focused_window.is_none())
        );
        assert!(
            inner
                .client(desktop)
                .is_none_or(|c| c.visible_terminals.is_none())
        );
        assert!(
            inner
                .client(desktop)
                .is_some_and(|c| c.subscriber.is_some())
        );
        assert!(
            inner
                .panes
                .terminal_geometries
                .values()
                .all(|geometries| !geometries.contains_key(&desktop))
        );
        let effective = terminal_resize_for_pane(&inner, pane)
            .expect("laptop-only geometry")
            .1;
        assert_eq!(
            effective,
            TerminalGeometry {
                columns: 140,
                rows: 40,
                cell_width_px: 9,
                cell_height_px: 18,
            }
        );
    }
    wait_for_terminal_dimensions(&terminal, TerminalViewId(laptop.0), 140, 40);
    let deadline = Instant::now() + Duration::from_secs(30);
    while terminal
        .latest_viewport_for(TerminalViewId(desktop.0))
        .is_some()
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        terminal
            .latest_viewport_for(TerminalViewId(desktop.0))
            .is_none(),
        "steal-attach must release the victim's terminal view"
    );
}

#[test]
fn killing_attached_session_notifies_and_detaches_every_client() {
    let shared = Arc::new(Shared::new(1));
    let first_mailbox = OutboundMailbox::new();
    let second_mailbox = OutboundMailbox::new();
    let (first, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("desktop".to_owned()),
        None,
        Arc::clone(&first_mailbox),
    );
    let (second, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("laptop".to_owned()),
        None,
        Arc::clone(&second_mailbox),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "doomed"]),
        )
        .expect("create doomed session");
    let session = context.session.expect("doomed session");
    let window = context.window.expect("doomed window");
    shared.attach(first, session).expect("attach first");
    shared.attach(second, session).expect("attach second");
    {
        let mut inner = shared.inner.lock();
        inner.client_entry(first).focused_window.replace(window);
        inner.client_entry(second).focused_window.replace(window);
    }
    take_reliable_messages(&first_mailbox);
    take_reliable_messages(&second_mailbox);

    shared
        .execute(
            first,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "doomed"]),
        )
        .expect("kill attached session");

    for mailbox in [&first_mailbox, &second_mailbox] {
        assert!(
            take_reliable_messages(mailbox)
                .iter()
                .any(|message| matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload:
                            EventPayload::Detached {
                                session: detached,
                                by: None,
                                ..
                            },
                        ..
                    }) if *detached == session
                )),
            "every attached client must learn that the session ended"
        );
    }
    let inner = shared.inner.lock();
    assert!(!inner.attached.contains_key(&session));
    assert!(
        inner
            .client(first)
            .is_none_or(|c| c.visible_terminals.is_none())
    );
    assert!(
        inner
            .client(second)
            .is_none_or(|c| c.visible_terminals.is_none())
    );
    assert!(
        inner
            .client(first)
            .is_none_or(|c| c.focused_window.is_none())
    );
    assert!(
        inner
            .client(second)
            .is_none_or(|c| c.focused_window.is_none())
    );
    assert!(inner.client(first).is_some_and(|c| c.subscriber.is_some()));
    assert!(inner.client(second).is_some_and(|c| c.subscriber.is_some()));
}
