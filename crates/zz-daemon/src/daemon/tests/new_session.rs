use super::*;

#[test]
fn interactive_new_session_attaches_the_requesting_client() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, hello) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    assert!(
        hello
            .capabilities
            .iter()
            .any(|capability| capability == zz_protocol::NEW_SESSION_ATTACH_CAPABILITY)
    );
    let mut context = ExecutionContext::default();

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", [] as [&str; 0]),
        )
        .expect("create and attach session");
    let session = context.session.expect("session id");

    assert_eq!(
        shared.inner.lock().attachment.attached.get(&session),
        Some(&BTreeSet::from([client]))
    );
    let messages = take_reliable_messages(&mailbox);
    let attached_index = messages.iter().position(|message| {
        matches!(
            message,
            ProtocolMessage::Attached {
                session: attached,
                snapshot,
                ..
            } if *attached == session
                && snapshot.sessions.iter().any(|candidate| candidate.id == session)
        )
    });
    let snapshot_index = messages.iter().position(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::Snapshot(_),
                ..
            })
        )
    });
    assert!(attached_index.is_some());
    assert!(snapshot_index.is_none_or(|snapshot_index| attached_index < Some(snapshot_index)));
}

#[test]
fn new_session_name_formats_only_inherit_state_from_attached_clients() {
    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut seed_context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut seed_context,
            &CommandInvocation::new("new-session", ["-d", "-s", "origin", "-n", "origin-window"]),
        )
        .unwrap();
    let origin = seed_context.session.unwrap();
    let origin_window = seed_context.window.unwrap();
    let origin_pane = seed_context.pane.unwrap();

    let interactive_mailbox = OutboundMailbox::new();
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&interactive_mailbox),
    );
    let mut interactive_context =
        ExecutionContext::new(Some(origin), Some(origin_window), Some(origin_pane));
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut interactive_context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "#{session_name}-interactive-unattached"],
            ),
        )
        .unwrap();
    let mut command_context =
        ExecutionContext::new(Some(origin), Some(origin_window), Some(origin_pane));
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "#{session_name}-command-unattached"],
            ),
        )
        .unwrap();

    shared.attach(interactive, origin).unwrap();
    interactive_context.retarget(&ExecutionContext::new(
        Some(origin),
        Some(origin_window),
        Some(origin_pane),
    ));
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut interactive_context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d",
                    "-s",
                    "#{session_name}-#{window_name}-#{pane_id}-interactive-attached",
                ],
            ),
        )
        .unwrap();
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut interactive_context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d",
                    "-s",
                    "#{session_name}-#{window_name}-#{pane_id}-interactive-still-attached",
                ],
            ),
        )
        .unwrap();

    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    shared.attach(control, origin).unwrap();
    let mut control_context =
        ExecutionContext::new(Some(origin), Some(origin_window), Some(origin_pane));
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d",
                    "-s",
                    "#{session_name}-#{window_name}-#{pane_id}-control-attached",
                ],
            ),
        )
        .unwrap();
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d",
                    "-s",
                    "#{session_name}-#{window_name}-#{pane_id}-control-still-attached",
                ],
            ),
        )
        .unwrap();

    let inner = shared.inner.lock();
    let names = inner
        .engine
        .state
        .sessions
        .values()
        .map(|session| session.name.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        names,
        BTreeSet::from([
            "origin".to_owned(),
            "-interactive-unattached".to_owned(),
            "-command-unattached".to_owned(),
            format!("origin-origin-window-{origin_pane}-interactive-attached"),
            format!("origin-origin-window-{origin_pane}-interactive-still-attached"),
            format!("origin-origin-window-{origin_pane}-control-attached"),
            format!("origin-origin-window-{origin_pane}-control-still-attached"),
        ])
    );
}

#[test]
fn interactive_new_session_without_a_terminal_is_rejected_before_creation() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared
        .register(
            ClientKind::Interactive,
            ClientInstanceId::default(),
            None,
            None,
            false,
            false,
        )
        .expect("register headless client");
    shared.subscribe(client, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();

    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "headless"]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "open terminal failed: not a terminal"
    ));
    let inner = shared.inner.lock();
    assert!(inner.engine.state.sessions.is_empty());
    assert!(inner.attachment.attached.is_empty());
    drop(inner);
    assert!(
        !take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(message, ProtocolMessage::Attached { .. }))
    );
}

#[test]
fn control_new_session_attaches_without_terminal_frames() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, hello) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    assert!(
        hello
            .capabilities
            .iter()
            .any(|capability| capability == zz_protocol::NEW_SESSION_ATTACH_CAPABILITY)
    );
    shared.inner.lock().client_entry(client).pid.replace(4242);
    let mut context = ExecutionContext::default();

    shared
        .execute(
            client,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("new-session", [] as [&str; 0]),
        )
        .expect("create and attach control session");
    let session = context.session.expect("session id");
    let pane = context.pane.expect("pane id");

    assert_eq!(
        shared.inner.lock().attachment.attached.get(&session),
        Some(&BTreeSet::from([client]))
    );
    let messages = take_reliable_messages(&mailbox);
    let attached_snapshot = messages
        .iter()
        .find_map(|message| match message {
            ProtocolMessage::Attached {
                session: attached,
                snapshot,
                ..
            } if *attached == session => Some(snapshot),
            _ => None,
        })
        .expect("attached snapshot");
    assert!(
        attached_snapshot.sessions[0]
            .viewers
            .iter()
            .any(|viewer| { viewer.name == "client-4242" && viewer.is_self })
    );
    assert_eq!(
        messages
            .iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::HookEvent { name, .. },
                    ..
                }) if matches!(
                    name.as_str(),
                    "window-linked"
                        | "session-created"
                        | "client-session-changed"
                        | "client-attached"
                ) =>
                    Some(name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [
            "window-linked",
            "session-created",
            "client-session-changed",
            "client-attached",
        ]
    );
    let session_changed_variables = messages
        .iter()
        .find_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::HookEvent { name, variables },
                ..
            }) if name == "client-session-changed" => Some(variables),
            _ => None,
        })
        .expect("client session changed variables");
    let expected_session = session.to_string();
    assert_eq!(
        session_changed_variables
            .get("hook_session")
            .map(String::as_str),
        Some(expected_session.as_str())
    );
    assert_eq!(
        session_changed_variables
            .get("hook_session_name")
            .map(String::as_str),
        Some("0")
    );
    assert_eq!(
        session_changed_variables
            .get("hook_client")
            .map(String::as_str),
        Some("client-4242")
    );
    let hook_names = messages
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::HookEvent { name, .. },
                ..
            }) => Some(name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        hook_names.iter().all(|name| matches!(
            *name,
            "window-linked"
                | "session-created"
                | "pane-focus-in"
                | "client-session-changed"
                | "client-attached"
                | "window-renamed"
                | "pane-title-changed"
                | "pane-created"
                | "window-created"
                | "client-created"
        )),
        "unexpected control hook sequence: {hook_names:?}"
    );
    shared.send_resync(client, &mailbox);
    shared.send_full(client, pane, &mailbox);
    thread::sleep(Duration::from_millis(50));
    assert!(mailbox.state.lock().terminals.is_empty());
    assert!(output_routed(&shared.inner.lock(), pane));
    shared.detach(client);
    assert!(!output_routed(&shared.inner.lock(), pane));
}

#[test]
fn new_session_attach_or_create_cwd_survives_preflight_and_control_attach() {
    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut setup_context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut setup_context,
            &CommandInvocation::new("new-session", ["-d", "-s", "source", "-c", "/source"]),
        )
        .unwrap();
    let source = setup_context.session.unwrap();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut setup_context,
            &CommandInvocation::new("new-session", ["-d", "-s", "target", "-c", "/target"]),
        )
        .unwrap();
    let target = setup_context.session.unwrap();

    let headless_mailbox = OutboundMailbox::new();
    let (headless, _) = shared
        .register(
            ClientKind::Interactive,
            ClientInstanceId::default(),
            Some("headless-cwd".to_owned()),
            None,
            false,
            false,
        )
        .unwrap();
    shared.subscribe(headless, Arc::clone(&headless_mailbox));
    let mut headless_context = ExecutionContext::default();
    assert!(matches!(
        shared.execute(
            headless,
            ClientKind::Interactive,
            &mut headless_context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-A",
                    "-s",
                    "target",
                    "-c",
                    "/#{session_name}/headless",
                ],
            ),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "open terminal failed: not a terminal"
    ));
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.engine.state.session_working_directory(target),
            Some(Path::new("/target/headless"))
        );
        assert_eq!(
            inner.engine.state.session_working_directory(source),
            Some(Path::new("/source"))
        );
        assert!(
            inner
                .attachment
                .attached
                .values()
                .all(|clients| !clients.contains(&headless))
        );
    }

    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("cwd-control".to_owned()),
        None,
        Arc::clone(&control_mailbox),
    );
    shared.attach(control, source).unwrap();
    let mut control_context = ExecutionContext::default();
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-A",
                    "-s",
                    "target",
                    "-c",
                    "/#{session_name}/#{client_name}",
                ],
            ),
        )
        .unwrap();
    let inner = shared.inner.lock();
    assert_eq!(client_attached_session(&inner, control), Some(target));
    assert_eq!(
        inner.engine.state.session_working_directory(target),
        Some(Path::new("/target/cwd-control"))
    );
    assert_eq!(
        inner.engine.state.session_working_directory(source),
        Some(Path::new("/source"))
    );
}

#[test]
fn control_output_taps_follow_new_pty_panes_and_skip_browser_panes() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "control-output-kinds"]),
        )
        .expect("new control session");
    let terminal = context.pane.expect("terminal pane");
    shared
        .execute(
            client,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new(
                "split-window",
                [
                    "--kind",
                    "browser",
                    "-h",
                    "https://example.com/control-output",
                ],
            ),
        )
        .expect("split browser");
    let browser = context.pane.expect("browser pane");

    {
        let inner = shared.inner.lock();
        assert!(output_routed(&inner, terminal));
        assert!(!output_routed(&inner, browser));
    }
    shared.detach(client);
}
