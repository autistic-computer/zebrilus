use super::*;

#[test]
fn switch_client_moves_one_client_tracks_last_and_routes_one_attached() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, a_pane) = switch_test_session(&shared, "A");
    let (b, _, _) = switch_test_session(&shared, "B");
    let target_mailbox = OutboundMailbox::new();
    let peer_mailbox = OutboundMailbox::new();
    let hook_mailbox = OutboundMailbox::new();
    let (target, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("target".to_owned()),
        None,
        Arc::clone(&target_mailbox),
    );
    let (peer, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("peer".to_owned()),
        None,
        Arc::clone(&peer_mailbox),
    );
    let (hooks, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("hooks".to_owned()),
        None,
        Arc::clone(&hook_mailbox),
    );
    shared.attach(target, a).expect("attach target to A");
    shared.attach(peer, a).expect("attach peer to A");
    shared.attach(hooks, b).expect("attach hook observer to B");
    take_reliable_messages(&target_mailbox);
    take_reliable_messages(&peer_mailbox);
    take_reliable_messages(&hook_mailbox);

    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, a_pane)
        .expect("target context");
    shared
        .execute(
            target,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-t", "B"]),
        )
        .expect("switch target to B");

    {
        let inner = shared.inner.lock();
        assert_eq!(client_attached_session(&inner, target), Some(b));
        assert_eq!(client_attached_session(&inner, peer), Some(a));
        assert_eq!(
            inner.client(target).and_then(|c| c.last_session.as_ref()),
            Some(&a)
        );
    }
    let target_messages = take_reliable_messages(&target_mailbox);
    assert_eq!(
        target_messages
            .iter()
            .filter(|message| matches!(message, ProtocolMessage::Attached { session, .. } if *session == b))
            .count(),
        1
    );
    assert!(
        take_reliable_messages(&peer_mailbox)
            .iter()
            .all(|message| !matches!(message, ProtocolMessage::Attached { .. }))
    );
    let hook_messages = take_reliable_messages(&hook_mailbox);
    assert_eq!(
        hook_messages
            .iter()
            .filter(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::HookEvent { name, .. },
                    ..
                }) if name == "client-session-changed"
            ))
            .count(),
        1
    );
    assert!(hook_messages.iter().all(|message| !matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::HookEvent { name, .. },
            ..
        }) if name == "client-attached"
    )));

    shared
        .execute(
            target,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-l"]),
        )
        .expect("switch back to A");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), target),
        Some(a)
    );
    shared
        .execute(
            target,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-t", "B"]),
        )
        .expect("return to B");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "A"]),
        )
        .expect("kill A");
    assert!(matches!(
        shared.execute(
            target,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-l"]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "can't find last session"
    ));
}

#[test]
fn switch_client_notifies_when_the_session_does_not_change() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, a_pane) = switch_test_session(&shared, "A");
    let (b, _, _) = switch_test_session(&shared, "B");
    let target_mailbox = OutboundMailbox::new();
    let hook_mailbox = OutboundMailbox::new();
    let (target, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("target".to_owned()),
        None,
        Arc::clone(&target_mailbox),
    );
    let (hooks, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("hooks".to_owned()),
        None,
        Arc::clone(&hook_mailbox),
    );
    shared.attach(target, a).expect("attach target");
    shared.attach(hooks, b).expect("attach hook observer");
    take_reliable_messages(&target_mailbox);
    take_reliable_messages(&hook_mailbox);

    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, a_pane)
        .expect("target context");
    shared
        .execute(
            target,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-t", "A"]),
        )
        .expect("same-session switch");

    assert_eq!(
        take_reliable_messages(&hook_mailbox)
            .iter()
            .filter(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::HookEvent { name, .. },
                    ..
                }) if name == "client-session-changed"
            ))
            .count(),
        1
    );
}

#[test]
fn switch_client_resolves_origin_errors_names_and_walks_name_order() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, a_pane) = switch_test_session(&shared, "a");
    let (m, _, _) = switch_test_session(&shared, "m");
    let (z, _, _) = switch_test_session(&shared, "z");
    let (a_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("alpha".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (z_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("zeta".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (command, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    shared.attach(a_client, a).expect("attach alpha");
    shared.attach(z_client, z).expect("attach zeta");
    shared
        .inner
        .lock()
        .client_entry(command)
        .origin
        .replace(a_pane);

    shared
        .execute(
            command,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("switch-client", ["-t", "m"]),
        )
        .expect("origin selects alpha");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), a_client),
        Some(m)
    );
    assert_eq!(
        client_attached_session(&shared.inner.lock(), z_client),
        Some(z)
    );

    shared
        .execute(
            a_client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("switch-client", ["-t", "a"]),
        )
        .expect("reset alpha to a");
    shared
        .execute(
            a_client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("switch-client", ["-t", "m", "-n"]),
        )
        .expect("walk from the current session instead of the parsed target");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), a_client),
        Some(m)
    );

    let mut context = ExecutionContext::default();
    shared
        .execute(
            a_client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-n"]),
        )
        .expect("m next is z");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), a_client),
        Some(z)
    );
    shared
        .execute(
            a_client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-r", "-n"]),
        )
        .expect("z next wraps to a");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), a_client),
        Some(a)
    );
    assert!(shared.inner.lock().client_flags.contains(a_client));
    shared
        .execute(
            a_client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-p"]),
        )
        .expect("a previous wraps to z");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), a_client),
        Some(z)
    );
    shared
        .execute(
            a_client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-O", "name", "-r", "-n"]),
        )
        .expect("explicit reverse order walks z to m");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), a_client),
        Some(m)
    );
    assert!(!shared.inner.lock().client_flags.contains(a_client));
    shared
        .execute(
            a_client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-t", "a"]),
        )
        .expect("reset before combined walk flags");
    shared
        .execute(
            a_client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-np"]),
        )
        .expect("next wins when next and previous are both set");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), a_client),
        Some(m)
    );
    assert!(matches!(
        shared.execute(
            a_client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "switch-client",
                ["-t", "missing", "-O", "bogus"],
            ),
        ),
        Err(DaemonError::Server(ServerError::SessionNotFound(target)))
            if target == "missing"
    ));
    assert!(matches!(
        shared.execute(
            a_client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-r", "-O", "bogus"]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "invalid sort order"
    ));
    assert!(shared.inner.lock().client_flags.contains(a_client));

    for target in ["bogus", "bogus:"] {
        assert!(matches!(
            shared.execute(
                command,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("switch-client", ["-c", target]),
            ),
            Err(DaemonError::Server(ServerError::InvalidCommand(message)))
                if message == "can't find client: bogus"
        ));
    }
    let solo = Arc::new(Shared::new(3));
    let (solo_client, _) =
        solo.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let mut solo_context = ExecutionContext::default();
    solo.execute(
        solo_client,
        ClientKind::Command,
        &mut solo_context,
        &CommandInvocation::new("new-session", ["-d", "-s", "only"]),
    )
    .expect("solo session");
    let only = solo_context.session.expect("solo session id");
    solo.attach(solo_client, only)
        .expect("attach the solo client");
    for flag in ["-n", "-p"] {
        solo.execute(
            solo_client,
            ClientKind::Interactive,
            &mut solo_context,
            &CommandInvocation::new("switch-client", [flag]),
        )
        .expect("a single session wraps to itself like the pin");
        assert_eq!(
            client_attached_session(&solo.inner.lock(), solo_client),
            Some(only)
        );
    }
    let empty = Arc::new(Shared::new(2));
    let (command, _) =
        empty.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    assert!(matches!(
        empty.execute(
            command,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("switch-client", [] as [&str; 0]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "no current client"
    ));
}

#[test]
fn switch_client_key_table_and_formats_are_client_local() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, _) = switch_test_session(&shared, "A");
    let (b, _, b_pane) = switch_test_session(&shared, "B");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("desktop".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(client, a).expect("attach desktop");
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "repeat-time", "60000"]),
        )
        .expect("keep the repeat table active while checking client scope");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-t", "B"]),
        )
        .expect("switch to B");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-r", "-T", "prefix", "-O", "bogus"]),
        )
        .expect("set prefix table before sort validation");
    {
        let inner = shared.inner.lock();
        assert_eq!(client_attached_session(&inner, client), Some(b));
        assert_eq!(
            inner.clients[&client]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            Some("prefix")
        );
    }
    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-T", "nope"]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "table nope doesn't exist"
    ));
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-t", "B"]),
        )
        .expect("typed same-session switch resets the key table");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        None
    );
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("bind-key", ["-r", "R", "switch-client", "-t", "A"]),
        )
        .expect("bind repeat switch");
    for input in [
        test_key(
            KeyCode::Character('b'),
            Modifiers::new(false, true, false, false),
            None,
        ),
        test_key(KeyCode::Character('R'), Modifiers::default(), Some("R")),
    ] {
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::Key {
                    pane: b_pane,
                    input,
                    text_follows: false,
                },
            )
            .expect("execute repeat switch binding");
    }
    {
        let inner = shared.inner.lock();
        assert_eq!(client_attached_session(&inner, client), Some(a));
        assert_eq!(
            inner.clients[&client]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            Some("prefix")
        );
    }
    let listed = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "list-clients",
                [
                    "-F",
                    "#{client_last_session}|#{client_session}|#{client_readonly}|#{client_flags}|#{client_key_table}|#{session_last_attached}",
                ],
            ),
        )
        .expect("list switch formats")
        .output;
    let fields = listed.split('|').collect::<Vec<_>>();
    assert_eq!(
        &fields[..5],
        ["B", "A", "1", "attached,focused,read-only", "prefix",]
    );
    assert!(fields[5].parse::<u64>().is_ok_and(|time| time > 0));
}

#[test]
fn removing_an_active_key_table_resets_clients_to_their_session_default() {
    let shared = Arc::new(Shared::new(1));
    let (session, _, _) = switch_test_session(&shared, "work");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("desktop".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(client, session).expect("attach desktop");
    let mut context = ExecutionContext::default();

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-T", "copy-mode"]),
        )
        .expect("select copy-mode table");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("copy-mode")
    );
    shared
        .execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("unbind-key", ["-a", "-T", "copy-mode"]),
        )
        .expect("remove copy-mode table");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            None
        );
        assert!(
            inner
                .engine
                .keys
                .table_names()
                .all(|table| table != "copy-mode")
        );
    }
    assert_eq!(
        shared
            .execute(
                ClientId(99),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("list-clients", ["-F", "#{client_key_table}"],),
            )
            .expect("list reset client table")
            .output,
        "root"
    );

    shared
        .execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                ["-T", "custom", "F1", "display-message", "custom"],
            ),
        )
        .expect("create custom table");
    shared
        .execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-t", "work", "key-table", "custom"]),
        )
        .expect("set session default table");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-T", "custom"]),
        )
        .expect("select custom table");
    shared
        .execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("unbind-key", ["-a", "-T", "custom"]),
        )
        .expect("remove custom default table");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            None
        );
        assert!(
            inner
                .engine
                .keys
                .table_names()
                .any(|table| table == "custom")
        );
        assert!(inner.engine.keys.list(Some("custom")).next().is_none());
    }
    assert_eq!(
        shared
            .execute(
                ClientId(99),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("list-clients", ["-F", "#{client_key_table}"],),
            )
            .expect("list custom reset client table")
            .output,
        "custom"
    );

    shared
        .execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-t", "work", "key-table", "prefix"]),
        )
        .expect("set prefix as the session default");
    shared
        .execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("unbind-key", ["-a", "-T", "prefix"]),
        )
        .expect("replace the default prefix table with an empty table");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-T", "prefix"]),
        )
        .expect("select the empty prefix table");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("prefix")
    );
    assert!(!prefix_published(&shared.inner.lock(), client));
    shared
        .execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("unbind-key", ["-a", "-T", "prefix"]),
        )
        .expect("remove and recreate the empty default table");
    {
        let inner = shared.inner.lock();
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
                .engine
                .keys
                .table_names()
                .any(|table| table == "prefix")
        );
        assert!(inner.engine.keys.list(Some("prefix")).next().is_none());
    }
}

#[test]
fn switch_client_repeat_binding_preserves_target_client_table() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, a_pane) = switch_test_session(&shared, "A");
    let (b, _, _) = switch_test_session(&shared, "B");
    let (invoker, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("invoker".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (target, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("target".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(invoker, a).expect("attach invoker");
    shared.attach(target, a).expect("attach target");
    let mut invoker_context = ExecutionContext::default();
    let mut target_context = ExecutionContext::default();
    shared
        .execute(
            target,
            ClientKind::Interactive,
            &mut target_context,
            &CommandInvocation::new("switch-client", ["-T", "prefix"]),
        )
        .expect("set target prefix table");
    shared
        .execute(
            invoker,
            ClientKind::Interactive,
            &mut invoker_context,
            &CommandInvocation::new(
                "bind-key",
                ["-r", "R", "switch-client", "-c", "target", "-t", "B"],
            ),
        )
        .expect("bind targeted repeat switch");
    for input in [
        test_key(
            KeyCode::Character('b'),
            Modifiers::new(false, true, false, false),
            None,
        ),
        test_key(KeyCode::Character('R'), Modifiers::default(), Some("R")),
    ] {
        shared
            .input(
                invoker,
                ClientKind::Interactive,
                &mut invoker_context,
                InputMessage::Key {
                    pane: a_pane,
                    input,
                    text_follows: false,
                },
            )
            .expect("execute targeted repeat switch binding");
    }
    let inner = shared.inner.lock();
    assert_eq!(client_attached_session(&inner, target), Some(b));
    assert_eq!(
        inner.clients[&target]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("prefix")
    );
}

#[test]
fn switch_client_targets_panes_preserves_zoom_and_orders_hooks() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, _) = switch_test_session(&shared, "A");
    let (b, _, _) = switch_test_session(&shared, "B");
    let (target_window, first_pane, target_pane) = {
        let mut inner = shared.inner.lock();
        let (window, first) = inner
            .engine
            .state
            .create_window_at(
                b,
                Some(1),
                Some("target".to_owned()),
                PaneKind::Terminal,
                false,
            )
            .expect("create target window");
        let target = inner
            .engine
            .state
            .split_pane(first, zz_protocol::Axis::Horizontal, PaneKind::Terminal)
            .expect("split target window once");
        inner
            .engine
            .state
            .split_pane(first, zz_protocol::Axis::Vertical, PaneKind::Terminal)
            .expect("split target window twice");
        inner
            .engine
            .state
            .toggle_zoom(first)
            .expect("zoom target window");
        (window, first, target)
    };
    let target_mailbox = OutboundMailbox::new();
    let hook_mailbox = OutboundMailbox::new();
    let (target, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("target".to_owned()),
        None,
        Arc::clone(&target_mailbox),
    );
    let (hooks, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("hooks".to_owned()),
        None,
        Arc::clone(&hook_mailbox),
    );
    shared.attach(target, a).expect("attach target to A");
    shared.attach(hooks, b).expect("attach hook observer to B");
    take_reliable_messages(&target_mailbox);
    take_reliable_messages(&hook_mailbox);
    shared.inner.lock().engine.set_format_now(1);

    shared
        .execute(
            target,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("switch-client", ["-Z", "-t", "B:1.2"]),
        )
        .expect("switch to indexed pane and preserve zoom");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.state.sessions[&b].active_window, target_window);
        assert_eq!(
            inner.engine.state.windows[&target_window].active_pane,
            target_pane
        );
        assert_eq!(
            inner.engine.state.windows[&target_window].zoomed_pane,
            Some(target_pane)
        );
        assert!(
            inner.engine.state.windows[&target_window]
                .activity_time
                .is_some_and(|activity| activity > 1)
        );
    }
    let ordered_hooks = take_reliable_messages(&hook_mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::HookEvent { name, .. },
                ..
            }) if matches!(
                name.as_str(),
                "window-pane-changed" | "session-window-changed" | "client-session-changed"
            ) =>
            {
                Some(name)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ordered_hooks,
        [
            "window-pane-changed",
            "session-window-changed",
            "client-session-changed"
        ]
    );

    shared
        .execute(
            target,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("switch-client", ["-t", "A"]),
        )
        .expect("switch away from target pane");
    shared
        .execute(
            target,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("switch-client", ["-t", &format!("%{}", target_pane.0)]),
        )
        .expect("switch back to the already-active pane");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.engine.state.windows[&target_window].active_pane,
            target_pane
        );
        assert_eq!(
            inner.engine.state.windows[&target_window].zoomed_pane,
            Some(target_pane)
        );
    }
    assert_ne!(first_pane, target_pane);
    shared
        .execute(
            target,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("switch-client", ["-t", &format!("%{}", first_pane.0)]),
        )
        .expect("switch to an inactive pane and clear zoom");
    let inner = shared.inner.lock();
    assert_eq!(
        inner.engine.state.windows[&target_window].active_pane,
        first_pane
    );
    assert_eq!(inner.engine.state.windows[&target_window].zoomed_pane, None);
}

#[test]
fn attached_context_sync_follows_a_same_session_window_switch() {
    let shared = Arc::new(Shared::new(1));
    let (session, first_window, first_pane) = switch_test_session(&shared, "same");
    let (second_window, second_pane) = {
        let mut inner = shared.inner.lock();
        let (window, pane) = inner
            .engine
            .state
            .create_window_at(
                session,
                Some(1),
                Some("second".to_owned()),
                PaneKind::Terminal,
                false,
            )
            .expect("create second window");
        inner
            .engine
            .state
            .select_window(session, first_window)
            .expect("restore first window");
        (window, pane)
    };
    let (victim, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("victim".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (command, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    shared.attach(victim, session).expect("attach victim");
    let mut victim_context =
        ExecutionContext::new(Some(session), Some(first_window), Some(first_pane));

    shared
        .execute(
            command,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("switch-client", ["-c", "victim", "-t", "same:1"]),
        )
        .expect("move victim within its session");
    assert_eq!(victim_context.window, Some(first_window));
    sync_context_with_attachment(&shared.inner.lock(), victim, &mut victim_context);
    assert_eq!(victim_context.session, Some(session));
    assert_eq!(victim_context.window, Some(second_window));
    assert_eq!(victim_context.pane, Some(second_pane));
}

#[test]
fn detach_on_destroy_retargets_control_clients_without_a_detach() {
    let shared = Arc::new(Shared::new(1));
    let (_, _, _) = switch_test_session(&shared, "a");
    let (b, _, b_pane) = switch_test_session(&shared, "b");
    let (c, _, _) = switch_test_session(&shared, "c");
    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared
        .attach(control, b)
        .expect("attach the control client to b");
    take_reliable_messages(&mailbox);
    let survivor_sort = {
        let mut inner = shared.inner.lock();
        inner.engine.state.session_mut(c).unwrap().activity = Some(9);
        inner.engine.state.sessions[&c].sort_activity
    };
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, b_pane).expect("b context");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-t", "b", "detach-on-destroy", "off"]),
        )
        .expect("set detach-on-destroy");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "b"]),
        )
        .expect("destroy b");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), control),
        Some(c)
    );
    {
        let inner = shared.inner.lock();
        assert!(inner.engine.state.sessions[&c].sort_activity > survivor_sort);
        assert_ne!(inner.engine.state.sessions[&c].activity, Some(9));
    }
    let messages = take_reliable_messages(&mailbox);
    let kinds = messages
        .iter()
        .map(|message| match message {
            ProtocolMessage::Attached { session, .. } => format!("attached:{session}"),
            ProtocolMessage::Event(Event { payload, .. }) => {
                format!("event:{}", std::any::type_name_of_val(payload))
                    .replace("zz_protocol::message::", "")
                    + &match payload {
                        EventPayload::Detached {
                            session, reason, ..
                        } => {
                            format!(" detached:{session} {reason:?}")
                        }
                        EventPayload::HookEvent { name, .. } => format!(" hook:{name}"),
                        _ => String::new(),
                    }
            }
            other => format!("other:{}", std::any::type_name_of_val(other)),
        })
        .collect::<Vec<_>>();
    assert!(
        !messages.iter().any(|message| matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::Detached { .. },
                ..
            })
        )),
        "control client must not be detached when a survivor exists: {kinds:?}"
    );
    assert!(
        messages.iter().any(|message| matches!(
            message,
            ProtocolMessage::Attached { session, .. } if *session == c
        )),
        "control client must be attached to the survivor: {kinds:?}"
    );
}

#[test]
fn detach_on_destroy_applies_no_detach_fallback_per_client() {
    fn run_case(
        policy: &str,
        a_attached: Option<bool>,
        c_attached: Option<bool>,
        flagged_expected: Option<&str>,
        plain_expected: Option<&str>,
    ) {
        let shared = Arc::new(Shared::new(1));
        let a = a_attached.map(|_| switch_test_session(&shared, "a").0);
        let (b, _, b_pane) = switch_test_session(&shared, "b");
        let c = c_attached.map(|_| switch_test_session(&shared, "c").0);
        for (name, session, attached) in [("a-client", a, a_attached), ("c-client", c, c_attached)]
        {
            if !attached.unwrap_or_default() {
                continue;
            }
            let (client, _) = shared.register_subscribed(
                ClientKind::Interactive,
                Some(name.to_owned()),
                None,
                OutboundMailbox::new(),
            );
            shared
                .attach(client, session.unwrap())
                .expect("attach survivor client");
        }
        let flagged_mailbox = OutboundMailbox::new();
        let (flagged, _) = shared.register_subscribed(
            ClientKind::Interactive,
            Some("flagged".to_owned()),
            None,
            Arc::clone(&flagged_mailbox),
        );
        let plain_mailbox = OutboundMailbox::new();
        let (plain, _) = shared.register_subscribed(
            ClientKind::Interactive,
            Some("plain".to_owned()),
            None,
            Arc::clone(&plain_mailbox),
        );
        shared.attach(flagged, b).expect("attach flagged client");
        shared.attach(plain, b).expect("attach plain client");
        shared
            .inner
            .lock()
            .client_flags
            .apply(flagged, "no-detach-on-destroy");
        take_reliable_messages(&flagged_mailbox);
        take_reliable_messages(&plain_mailbox);
        let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, b_pane)
            .expect("b context");
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-option", ["-t", "b", "detach-on-destroy", policy]),
            )
            .expect("set detach-on-destroy");
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("kill-session", ["-t", "b"]),
            )
            .expect("destroy b");

        let resolve = |expected| match expected {
            Some("a") => a,
            Some("c") => c,
            _ => None,
        };
        for (label, client, expected, mailbox) in [
            (
                "flagged",
                flagged,
                resolve(flagged_expected),
                flagged_mailbox,
            ),
            ("plain", plain, resolve(plain_expected), plain_mailbox),
        ] {
            assert_eq!(
                client_attached_session(&shared.inner.lock(), client),
                expected,
                "{label} client under {policy}"
            );
            let messages = take_reliable_messages(&mailbox);
            if let Some(expected) = expected {
                assert!(messages.iter().any(|message| matches!(
                    message,
                    ProtocolMessage::Attached { session, .. } if *session == expected
                )));
                assert!(!messages.iter().any(|message| matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::Detached { .. },
                        ..
                    })
                )));
            } else {
                assert!(messages.iter().any(|message| matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::Detached { session, reason, .. },
                        ..
                    }) if *session == b && reason.is_session_destroyed()
                )));
                assert!(
                    !messages
                        .iter()
                        .any(|message| matches!(message, ProtocolMessage::Attached { .. }))
                );
            }
        }
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(flagged)
                .is_none_or(|c| c.last_session.is_none())
        );
        assert!(inner.client(plain).is_none_or(|c| c.last_session.is_none()));
        assert!(inner.client_flags.get(flagged).no_detach_on_destroy);
        assert!(!inner.client_flags.get(plain).no_detach_on_destroy);
    }

    run_case("on", Some(false), Some(true), Some("c"), None);
    run_case("no-detached", Some(true), Some(true), Some("c"), None);
    run_case("no-detached", Some(false), Some(true), Some("a"), Some("a"));
    run_case("on", None, None, None, None);
}

#[test]
fn detach_on_destroy_selects_each_policy_survivor() {
    for (policy, flagged, expected) in [
        ("off", true, Some("c")),
        ("on", false, None),
        ("no-detached", false, Some("c")),
        ("previous", true, Some("a")),
        ("next", true, Some("c")),
    ] {
        let shared = Arc::new(Shared::new(1));
        let (a, _, _) = switch_test_session(&shared, "a");
        let (b, _, b_pane) = switch_test_session(&shared, "b");
        let (c, _, _) = switch_test_session(&shared, "c");
        let target_mailbox = OutboundMailbox::new();
        let (target, _) = shared.register_subscribed(
            ClientKind::Interactive,
            Some("target".to_owned()),
            None,
            Arc::clone(&target_mailbox),
        );
        let (activity, _) = shared.register_subscribed(
            ClientKind::Interactive,
            Some("activity".to_owned()),
            None,
            OutboundMailbox::new(),
        );
        shared.attach(activity, a).expect("activate a");
        if policy == "off" {
            shared.attach(activity, c).expect("make c newest");
        }
        shared.attach(target, b).expect("attach target to b");
        if flagged {
            shared
                .inner
                .lock()
                .client_flags
                .apply(target, "no-detach-on-destroy");
        }
        take_reliable_messages(&target_mailbox);
        let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, b_pane)
            .expect("b context");
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-option", ["-t", "b", "detach-on-destroy", policy]),
            )
            .expect("set detach-on-destroy");
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("kill-session", ["-t", "b"]),
            )
            .expect("destroy b");

        let attached = client_attached_session(&shared.inner.lock(), target);
        let expected_id = match expected {
            Some("a") => Some(a),
            Some("c") => Some(c),
            _ => None,
        };
        assert_eq!(attached, expected_id, "policy {policy}");
        assert!(shared.read_client(target, |c| c.is_none_or(|c| c.last_session.is_none())));
        let messages = take_reliable_messages(&target_mailbox);
        if let Some(expected_id) = expected_id {
            assert!(messages.iter().any(|message| matches!(
                message,
                ProtocolMessage::Attached { session, .. } if *session == expected_id
            )));
        } else {
            assert!(messages.iter().any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Detached { session, reason, .. },
                    ..
                }) if *session == b && reason.is_session_destroyed()
            )));
        }
    }
}

#[test]
fn detach_on_destroy_runs_for_last_window_and_last_pane() {
    for (command, target) in [("kill-window", "b:0"), ("kill-pane", "b:0.0")] {
        let shared = Arc::new(Shared::new(1));
        switch_test_session(&shared, "a");
        let (b, _, b_pane) = switch_test_session(&shared, "b");
        let mailbox = OutboundMailbox::new();
        let (client, _) = shared.register_subscribed(
            ClientKind::Interactive,
            Some("target".to_owned()),
            None,
            Arc::clone(&mailbox),
        );
        shared.attach(client, b).expect("attach target to b");
        take_reliable_messages(&mailbox);
        let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, b_pane)
            .expect("b context");
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(command, ["-t", target]),
            )
            .expect("destroy final resource");
        assert_eq!(client_attached_session(&shared.inner.lock(), client), None);
        assert!(
            take_reliable_messages(&mailbox)
                .iter()
                .any(|message| matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::Detached { session, reason, .. },
                        ..
                    }) if *session == b && reason.is_session_destroyed()
                ))
        );
    }
}

#[test]
fn destroy_unattached_policies_fire_only_when_the_last_client_detaches() {
    for (policy, destroyed) in [
        (None, false),
        (Some("off"), false),
        (Some("keep-last"), false),
        (Some("on"), true),
        (Some("keep-group"), true),
    ] {
        let shared = Arc::new(Shared::new(1));
        let (a, _, _) = switch_test_session(&shared, "a");
        let (b, _, _) = switch_test_session(&shared, "b");
        let (first, _) = shared.register_subscribed(
            ClientKind::Interactive,
            Some("first".to_owned()),
            None,
            OutboundMailbox::new(),
        );
        let (second, _) = shared.register_subscribed(
            ClientKind::Interactive,
            Some("second".to_owned()),
            None,
            OutboundMailbox::new(),
        );
        shared.attach(first, b).expect("attach first");
        shared.attach(second, b).expect("attach second");
        if let Some(policy) = policy {
            shared
                .execute(
                    ClientId(u64::MAX),
                    ClientKind::Command,
                    &mut ExecutionContext::default(),
                    &CommandInvocation::new(
                        "set-option",
                        ["-t", "b", "destroy-unattached", policy],
                    ),
                )
                .expect("set destroy-unattached");
        }
        shared.detach(first);
        assert!(
            shared.inner.lock().engine.state.sessions.contains_key(&b),
            "policy {policy:?} destroyed b while a client was still attached"
        );
        shared.detach(second);
        assert_eq!(
            shared.inner.lock().engine.state.sessions.contains_key(&b),
            !destroyed,
            "policy {policy:?}"
        );
        assert!(
            shared.inner.lock().engine.state.sessions.contains_key(&a),
            "policy {policy:?} leaked onto the session that never set it"
        );
        assert!(!shared.stopping.load(Ordering::Acquire));
    }
}

#[test]
fn destroy_unattached_runs_before_the_shutdown_check_in_unregister() {
    let shared = Arc::new(Shared::new(1));
    let (b, _, _) = switch_test_session(&shared, "b");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("only".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(client, b).expect("attach");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "destroy-unattached", "on"]),
        )
        .expect("set policy");
    shared.unregister(client);
    assert!(shared.inner.lock().engine.state.sessions.is_empty());
    #[cfg(unix)]
    lifecycle::wait_for_cleanup(&shared);
    assert!(shared.stopping.load(Ordering::Acquire));
}

#[test]
fn destroy_unattached_composes_with_detach_on_destroy_after_switching() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, _) = switch_test_session(&shared, "a");
    let (b, _, _) = switch_test_session(&shared, "b");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("mover".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(client, b).expect("attach to b");
    for (name, value) in [
        ("destroy-unattached", "on"),
        ("detach-on-destroy", "previous"),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("set-option", ["-g", name, value]),
            )
            .expect("set option");
    }
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-t", "a"]),
        )
        .expect("switch to a");
    {
        let inner = shared.inner.lock();
        assert!(!inner.engine.state.sessions.contains_key(&b));
        assert!(inner.engine.state.sessions.contains_key(&a));
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.last_session.is_none())
        );
    }
    assert_eq!(
        client_attached_session(&shared.inner.lock(), client),
        Some(a)
    );
    assert!(!shared.stopping.load(Ordering::Acquire));
}

#[test]
fn destroy_unattached_refuses_an_attach_that_lands_before_the_kill() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, _) = switch_test_session(&shared, "a");
    let (b, _, _) = switch_test_session(&shared, "b");
    let (leaver, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("leaver".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (joiner, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("joiner".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(leaver, b).expect("attach leaver");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-t", "b", "destroy-unattached", "on"]),
        )
        .expect("set destroy-unattached");
    let (reached_tx, reached_rx) = crossbeam_channel::bounded(1);
    let (release_tx, release_rx) = crossbeam_channel::bounded(1);
    *shared.destroy_unattached_hook.lock() = Some(ResponseAdmissionHook {
        reached: reached_tx,
        release: release_rx,
    });
    let detacher = {
        let shared = Arc::clone(&shared);
        thread::spawn(move || shared.detach(leaver))
    };
    reached_rx
        .recv()
        .expect("destroy-unattached reached the window before kill-session");
    let attach = shared.attach(joiner, b);
    release_tx.send(()).expect("release kill-session");
    detacher.join().expect("detach thread");
    assert!(matches!(attach, Err(ServerError::MissingTarget(_))));
    let inner = shared.inner.lock();
    assert!(!inner.engine.state.sessions.contains_key(&b));
    assert!(inner.engine.state.sessions.contains_key(&a));
    assert_eq!(client_attached_session(&inner, joiner), None);
    assert!(inner.destroying_unattached.is_empty());
}
