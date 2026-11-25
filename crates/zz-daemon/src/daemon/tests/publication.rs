use super::*;

#[test]
fn mouse_publication_is_per_recipient_for_session_effective_values() {
    let shared = Arc::new(Shared::new(1));
    let ((_alpha, alpha_mailbox, _a), (_beta, beta_mailbox, _b)) = two_session_pair(&shared);

    let mouse_values = |mailbox: &OutboundMailbox| {
        take_reliable_messages(mailbox)
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::MuxOptionsChanged { options },
                    ..
                }) => options
                    .get(MuxOptionKey::Mouse)
                    .map(|value| value.value.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    shared
        .execute(
            ClientId(91),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-t", "a:", "mouse", "off"]),
        )
        .expect("session mouse write");
    assert_eq!(mouse_values(&alpha_mailbox), ["off"]);
    assert!(mouse_values(&beta_mailbox).is_empty());

    shared
        .execute(
            ClientId(91),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "mouse", "off"]),
        )
        .expect("global mouse write");
    assert_eq!(mouse_values(&alpha_mailbox), ["off"]);
    assert_eq!(mouse_values(&beta_mailbox), ["off"]);

    shared
        .execute(
            ClientId(91),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "mouse", "on"]),
        )
        .expect("global mouse restore");
    assert!(mouse_values(&alpha_mailbox).is_empty());
    assert_eq!(mouse_values(&beta_mailbox), ["on"]);
}

#[test]
fn attach_and_switch_publish_the_effective_map_only_when_it_changes() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    for name in ["a", "b"] {
        shared
            .execute(
                ClientId(90),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("new-session", ["-d", "-s", name]),
            )
            .expect("session");
    }
    shared
        .execute(
            ClientId(91),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-t", "a:", "mouse", "off"]),
        )
        .expect("session mouse write");

    let mailbox = OutboundMailbox::new();
    let (alpha, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("alpha".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    take_reliable_messages(&mailbox);

    let mouse_values = |mailbox: &OutboundMailbox| {
        take_reliable_messages(mailbox)
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::MuxOptionsChanged { options },
                    ..
                }) => options
                    .get(MuxOptionKey::Mouse)
                    .map(|value| value.value.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    let mut attach_context = ExecutionContext::default();
    shared
        .execute(
            alpha,
            ClientKind::Interactive,
            &mut attach_context,
            &CommandInvocation::new("attach-session", ["-t", "a"]),
        )
        .expect("attach to a");
    assert_eq!(mouse_values(&mailbox), ["off"]);

    shared
        .execute(
            alpha,
            ClientKind::Interactive,
            &mut attach_context,
            &CommandInvocation::new("switch-client", ["-t", "b"]),
        )
        .expect("switch to b");
    assert_eq!(mouse_values(&mailbox), ["on"]);

    shared
        .execute(
            ClientId(91),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-t", "b:", "mouse", "on"]),
        )
        .expect("session write equal to the published value");
    assert!(mouse_values(&mailbox).is_empty());

    shared
        .execute(
            alpha,
            ClientKind::Interactive,
            &mut attach_context,
            &CommandInvocation::new("switch-client", ["-t", "a"]),
        )
        .expect("switch back to a");
    assert_eq!(mouse_values(&mailbox), ["off"]);
}

#[test]
fn status_position_publishes_per_recipient_for_session_scoped_writes() {
    let shared = Arc::new(Shared::new(1));
    let ((_alpha, alpha_mailbox, _a), (_beta, beta_mailbox, _b)) = two_session_pair(&shared);
    for option in ["status-left", "status-right"] {
        shared
            .execute(
                ClientId(91),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("set-option", ["-g", option, ""]),
            )
            .expect("fixed status half");
    }
    shared.refresh_status();
    take_reliable_messages(&alpha_mailbox);
    take_reliable_messages(&beta_mailbox);

    shared
        .execute(
            ClientId(91),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-t", "a:", "status-position", "top"]),
        )
        .expect("session status position");

    let positions = |mailbox: &OutboundMailbox| {
        take_reliable_messages(mailbox)
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::StatusChanged { status },
                    ..
                }) => Some(status.position),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        positions(&alpha_mailbox),
        [zz_protocol::StatusPosition::Top]
    );
    let beta_positions = positions(&beta_mailbox);
    assert!(beta_positions.is_empty(), "{beta_positions:?}");
}

#[test]
fn terminal_preview_streams_inactive_windows_without_owning_geometry() {
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
    let (session, first, first_terminal) =
        output_view_session_fixture(&shared, "preview-first", "first");
    let (second_window, second) = shared
        .inner
        .lock()
        .engine
        .state
        .create_window(session, Some("second".to_owned()), PaneKind::Terminal)
        .expect("second window");
    let second_terminal = Arc::new(TerminalSession::spawn_output_view(
        "preview-second".to_owned(),
        "second".to_owned(),
    ));
    shared
        .inner
        .lock()
        .terminals_mut()
        .insert(second, Arc::clone(&second_terminal));
    shared
        .watch_terminal(second, &second_terminal)
        .expect("watch second terminal");
    shared
        .attach(first_client, session)
        .expect("attach first client");
    shared
        .attach(second_client, session)
        .expect("attach second client");
    let first_view = TerminalViewId(first_client.0);
    let first_window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(first)
        .expect("first window");
    wait_for_viewport(
        &first_terminal,
        first_view,
        "first preview viewport",
        |_| true,
    );
    wait_for_viewport(
        &second_terminal,
        first_view,
        "second foreground viewport",
        |_| true,
    );
    first_mailbox.suspend_terminal(first);
    first_mailbox.suspend_terminal(second);
    second_mailbox.suspend_terminal(first);
    second_mailbox.suspend_terminal(second);

    let mut context = ExecutionContext::default();
    shared
        .input(
            first_client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane: second,
                columns: 120,
                rows: 40,
                cell_width_px: 8,
                cell_height_px: 18,
            },
        )
        .expect("foreground geometry");
    let owner = terminal_geometry_owner(&shared.inner.lock(), second);

    shared.set_terminal_preview(first_client, ClientKind::Interactive, true);
    {
        let inner = shared.inner.lock();
        assert_eq!(
            (*inner.clients[&first_client]
                .visible_terminals
                .as_ref()
                .unwrap()),
            BTreeSet::from([second])
        );
        assert_eq!(
            (*inner.clients[&first_client]
                .streamed_terminals
                .as_ref()
                .unwrap()),
            BTreeMap::from([
                (first, TerminalStreamKind::Preview),
                (second, TerminalStreamKind::Foreground),
            ])
        );
        assert_eq!(
            (*inner.clients[&second_client]
                .streamed_terminals
                .as_ref()
                .unwrap()),
            BTreeMap::from([(second, TerminalStreamKind::Foreground)])
        );
        assert!(
            inner
                .client(first_client)
                .is_some_and(|c| c.terminal_preview)
        );
        assert!(
            !inner
                .client(second_client)
                .is_some_and(|c| c.terminal_preview)
        );
        assert_eq!(terminal_geometry_owner(&inner, second), owner);
    }
    {
        let state = first_mailbox.state.lock();
        assert!(
            state
                .terminals
                .get(&first)
                .is_some_and(|pending| pending.preview)
        );
        assert!(!state.closed);
    }

    shared
        .inner
        .lock()
        .client_entry(first_client)
        .focused_window
        .replace(first_window);
    shared.refresh_terminal_visibility();
    {
        let inner = shared.inner.lock();
        assert_eq!(
            (*inner.clients[&first_client]
                .visible_terminals
                .as_ref()
                .unwrap()),
            BTreeSet::from([first])
        );
        assert_eq!(
            (*inner.clients[&first_client]
                .streamed_terminals
                .as_ref()
                .unwrap()),
            BTreeMap::from([
                (first, TerminalStreamKind::Foreground),
                (second, TerminalStreamKind::Preview),
            ])
        );
    }
    {
        let state = first_mailbox.state.lock();
        let pending = state
            .terminals
            .get(&first)
            .expect("preview promoted to foreground full");
        assert!(!pending.preview);
        assert!(matches!(
            decode_protocol_frame(&pending.encoded),
            Ok(ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalViewport { pane, .. },
                ..
            })) if pane == first
        ));
    }
    shared
        .inner
        .lock()
        .client_entry(first_client)
        .focused_window
        .replace(second_window);
    shared.refresh_terminal_visibility();
    assert_eq!(
        (*shared.inner.lock().clients[&first_client]
            .visible_terminals
            .as_ref()
            .unwrap()),
        BTreeSet::from([second])
    );

    first_mailbox.suspend_terminal(first);
    shared.send_full(first_client, first, &first_mailbox);
    {
        let state = first_mailbox.state.lock();
        assert!(
            state
                .terminals
                .get(&first)
                .is_some_and(|pending| pending.preview)
        );
        assert!(!state.closed);
    }

    shared
        .input(
            first_client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane: first,
                columns: 20,
                rows: 5,
                cell_width_px: 2,
                cell_height_px: 4,
            },
        )
        .expect("preview geometry is ignored");
    {
        let inner = shared.inner.lock();
        assert!(
            !inner
                .terminal_geometries
                .get(&first)
                .is_some_and(|geometries| geometries.contains_key(&first_client))
        );
        assert_eq!(terminal_geometry_owner(&inner, second), owner);
    }

    shared.set_terminal_preview(first_client, ClientKind::Interactive, false);
    {
        let inner = shared.inner.lock();
        assert_eq!(
            (*inner.clients[&first_client]
                .visible_terminals
                .as_ref()
                .unwrap()),
            BTreeSet::from([second])
        );
        assert_eq!(
            (*inner.clients[&first_client]
                .streamed_terminals
                .as_ref()
                .unwrap()),
            BTreeMap::from([(second, TerminalStreamKind::Foreground)])
        );
        assert!(
            !inner
                .client(first_client)
                .is_some_and(|c| c.terminal_preview)
        );
        assert_eq!(terminal_geometry_owner(&inner, second), owner);
    }
    {
        let state = first_mailbox.state.lock();
        assert!(!state.terminals.contains_key(&first));
        assert!(!state.terminal_order.contains(&first));
        assert!(!state.closed);
    }

    shared.set_terminal_preview(first_client, ClientKind::Interactive, true);
    shared.detach(first_client);
    let inner = shared.inner.lock();
    assert!(
        inner
            .client(first_client)
            .is_none_or(|c| c.visible_terminals.is_none())
    );
    assert!(
        inner
            .client(first_client)
            .is_none_or(|c| c.streamed_terminals.is_none())
    );
    assert!(
        !inner
            .client(first_client)
            .is_some_and(|c| c.terminal_preview)
    );
}

#[test]
fn terminal_preview_streams_every_session_on_the_host() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (session, attached, _) =
        output_view_session_fixture(&shared, "preview-attached", "attached");
    let (_, other, other_terminal) = output_view_session_fixture(&shared, "preview-other", "other");
    shared.attach(client, session).expect("attach client");
    assert_eq!(
        (*shared.inner.lock().clients[&client]
            .streamed_terminals
            .as_ref()
            .unwrap()),
        BTreeMap::from([(attached, TerminalStreamKind::Foreground)])
    );

    shared.set_terminal_preview(client, ClientKind::Interactive, true);
    assert_eq!(
        (*shared.inner.lock().clients[&client]
            .streamed_terminals
            .as_ref()
            .unwrap()),
        BTreeMap::from([
            (attached, TerminalStreamKind::Foreground),
            (other, TerminalStreamKind::Preview),
        ])
    );
    wait_for_viewport(
        &other_terminal,
        TerminalViewId(client.0),
        "other session preview viewport",
        |_| true,
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while !mailbox
        .state
        .lock()
        .terminals
        .get(&other)
        .is_some_and(|pending| pending.preview)
    {
        assert!(
            Instant::now() < deadline,
            "other session preview reaches the client"
        );
        thread::sleep(Duration::from_millis(10));
    }

    shared.set_terminal_preview(client, ClientKind::Interactive, false);
    assert_eq!(
        (*shared.inner.lock().clients[&client]
            .streamed_terminals
            .as_ref()
            .unwrap()),
        BTreeMap::from([(attached, TerminalStreamKind::Foreground)])
    );
}

#[test]
fn move_and_swap_window_publish_topology_snapshots() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("desktop".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "work", "-n", "main"]),
        )
        .unwrap();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-window", ["-d", "-n", "other"]),
        )
        .unwrap();
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("move-window", ["-d", "-s", "work:other", "-t", "work:5"]),
        )
        .unwrap();
    let moved = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::Snapshot(snapshot),
                ..
            }) => Some(snapshot),
            _ => None,
        })
        .collect::<Vec<_>>();
    let moved = moved.last().expect("move publishes a snapshot");
    assert_eq!(
        moved.sessions[0]
            .windows
            .iter()
            .map(|window| (window.index, window.name.as_str()))
            .collect::<Vec<_>>(),
        [(0, "main"), (5, "other")]
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("swap-window", ["-s", "work:0", "-t", "work:5"]),
        )
        .unwrap();
    let swapped = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::Snapshot(snapshot),
                ..
            }) => Some(snapshot),
            _ => None,
        })
        .collect::<Vec<_>>();
    let swapped = swapped.last().expect("swap publishes a snapshot");
    assert_eq!(
        swapped.sessions[0]
            .windows
            .iter()
            .map(|window| (window.index, window.name.as_str()))
            .collect::<Vec<_>>(),
        [(0, "other"), (5, "main")]
    );
}

#[test]
fn command_error_that_mutates_mux_state_publishes_snapshot() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (window, first) = {
        let mut inner = shared.inner.lock();
        let (_, window, first) = inner
            .engine
            .state
            .create_session("work")
            .expect("create session");
        inner
            .engine
            .state
            .split_pane(first, zz_protocol::Axis::Horizontal, PaneKind::Terminal)
            .expect("split window");
        inner.engine.state.toggle_zoom(first).expect("zoom pane");
        (window, first)
    };
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, first)
        .expect("command context");
    shared.publish_snapshot();
    take_reliable_messages(&mailbox);
    let generation = shared.inner.lock().engine.state.generation();

    let error = shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("select-layout", ["bogus"]),
        )
        .expect_err("invalid layout");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "malformed layout header: bogus"
    ));
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.state.windows[&window].zoomed_pane, None);
        assert_eq!(inner.engine.state.generation(), generation + 1);
        assert_eq!(inner.last_published_mux_generation, generation + 1);
    }
    let snapshots = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::Snapshot(snapshot),
                ..
            }) => Some(snapshot),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].generation, generation + 1);
    assert_eq!(snapshots[0].sessions[0].windows[0].zoomed_pane, None);
}
