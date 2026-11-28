use super::*;

#[test]
fn user_event_hook_client_is_the_invoking_client() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (attached, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "event-client", &[attached]);
    let (command, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    for args in [
        vec!["-g", "@ping", "set -gF @pong '#{hook_client}'"],
        vec!["-E", "@ping"],
    ] {
        shared
            .execute(
                command,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-hook", args),
            )
            .expect("set-hook");
    }
    let expected = client_format_name(&shared.inner.lock(), command);
    assert_ne!(expected, client_format_name(&shared.inner.lock(), attached));
    let shown = shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-options", ["-gv", "@pong"]),
        )
        .expect("show the hook's write");
    assert_eq!(shown.output.to_string().trim_end(), expected);
}

#[test]
fn hook_events_reach_only_control_clients_and_buffer_mutations_name_the_buffer() {
    let shared = Arc::new(Shared::new(1));
    let control_mailbox = OutboundMailbox::new();
    let interactive_mailbox = OutboundMailbox::new();
    shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&interactive_mailbox),
    );
    let context = ExecutionContext::default();
    shared
        .buffer_command(
            &context,
            "set-buffer",
            &["-b", "named", "payload"].map(RawText::from),
        )
        .expect("set buffer");
    shared
        .buffer_command(
            &context,
            "set-buffer",
            &["-a", "-b", "named", "more"].map(RawText::from),
        )
        .expect("append buffer");
    shared
        .buffer_command(
            &context,
            "delete-buffer",
            &["-b", "named"].map(RawText::from),
        )
        .expect("delete buffer");

    let events = take_reliable_messages(&control_mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::HookEvent { name, variables },
                ..
            }) => Some((name, variables)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        events
            .iter()
            .map(|(name, variables)| {
                (
                    name.as_str(),
                    variables.get("hook_paste_buffer").map(String::as_str),
                )
            })
            .collect::<Vec<_>>(),
        [
            ("paste-buffer-changed", Some("named")),
            ("paste-buffer-changed", Some("named")),
            ("paste-buffer-deleted", Some("named")),
        ]
    );
    assert!(
        take_reliable_messages(&interactive_mailbox)
            .iter()
            .all(|message| !matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::PaneOutput { .. },
                    ..
                })
            ))
    );
}

#[test]
fn an_editor_write_back_names_its_buffer_and_skips_a_replaced_one() {
    let shared = Arc::new(Shared::new(1));
    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    let context = ExecutionContext::default();
    let set = |data: &str| {
        shared
            .buffer_command(
                &context,
                "set-buffer",
                &["-b", "edit1", data].map(RawText::from),
            )
            .expect("set buffer");
    };
    let edit = |pane: u64, text: &str| {
        let file = tempfile::NamedTempFile::new().expect("editor file");
        std::fs::write(file.path(), text).expect("write editor file");
        let path = file.into_temp_path().keep().expect("keep editor file");
        let data = {
            let inner = shared.inner.lock();
            Arc::clone(
                &inner
                    .buffers
                    .paste_buffers
                    .iter()
                    .find(|buffer| buffer.name == "edit1")
                    .expect("edit1")
                    .data,
            )
        };
        shared.inner.lock().modes.editors.insert(
            PaneId(pane),
            EditorSession {
                client: control,
                path,
                target: EditorTarget::Buffer {
                    name: "edit1".to_owned(),
                    data,
                },
            },
        );
    };
    let buffer = || {
        let inner = shared.inner.lock();
        inner
            .buffers
            .paste_buffers
            .iter()
            .find(|buffer| buffer.name == "edit1")
            .map(|buffer| buffer.data.to_vec())
            .expect("edit1")
    };
    let changes = || {
        take_reliable_messages(&control_mailbox)
            .into_iter()
            .filter(|message| {
                matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::HookEvent { name, .. },
                        ..
                    }) if name == "paste-buffer-changed"
                )
            })
            .count()
    };

    set("first line");
    changes();
    edit(900, "first lineappended\n");
    shared.finish_editor(PaneId(900), 0);
    assert_eq!(buffer(), b"first lineappended");
    assert_eq!(changes(), 1);

    edit(901, "stale\n");
    set("replaced");
    changes();
    shared.finish_editor(PaneId(901), 0);
    assert_eq!(buffer(), b"replaced");
    assert_eq!(changes(), 0);

    edit(902, "failed\n");
    shared.finish_editor(PaneId(902), 1);
    assert_eq!(buffer(), b"replaced");
    assert_eq!(changes(), 0);
}

#[test]
fn paste_buffer_hooks_respect_the_existing_no_hooks_boundary() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    context.no_hooks = true;
    shared
        .buffer_command(
            &context,
            "set-buffer",
            &["-b", "outer", "outer"].map(RawText::from),
        )
        .expect("outer paste buffer");

    let inner = shared.inner.lock();
    assert!(
        inner
            .buffers
            .paste_buffers
            .iter()
            .any(|buffer| buffer.name == "outer")
    );
    drop(inner);
    assert!(take_reliable_messages(&mailbox).is_empty());
}

#[test]
fn control_hook_inventory_carries_every_rendering_variable() {
    let mut engine = MuxEngine::default();
    let (session, window, pane) = engine
        .state
        .create_session("inventory")
        .expect("hook inventory session");
    let snapshot = MuxHookSnapshot::capture(&engine);
    let session_state = &snapshot.sessions[&session];
    let window_state = &snapshot.windows[&window];
    let pane_state = &snapshot.panes[&pane];
    let require = |event: PendingHookEvent, keys: &[&str]| {
        let mut variables = event.variables.clone();
        hook_events::fit_payload(event.name, &mut variables);
        let mut payload = variables
            .keys()
            .filter_map(|key| key.strip_prefix("hook_"))
            .collect::<Vec<_>>();
        payload.sort_unstable();
        let mut expected = keys.to_vec();
        expected.sort_unstable();
        assert_eq!(payload, expected, "{}", event.name);
    };

    for name in ["session-created", "session-closed", "session-renamed"] {
        require(
            PendingHookEvent::session(name, session, session_state, &snapshot),
            &["session", "session_name"],
        );
    }
    for name in ["window-linked", "window-unlinked", "session-window-changed"] {
        require(
            PendingHookEvent::winlink(
                name,
                session,
                session_state,
                window,
                window_state,
                &snapshot,
            ),
            &[
                "session",
                "session_name",
                "window",
                "window_name",
                "window_index",
            ],
        );
    }
    for name in ["window-renamed", "window-layout-changed", "window-resized"] {
        require(
            PendingHookEvent::window(name, window, &snapshot),
            &["window", "window_name"],
        );
    }
    for name in [
        "pane-title-changed",
        "pane-mode-changed",
        "pane-died",
        "pane-exited",
    ] {
        require(
            PendingHookEvent::pane(name, pane, pane_state, &snapshot),
            &["pane", "window", "window_name"],
        );
    }
    for name in [
        "client-attached",
        "client-session-changed",
        "client-detached",
    ] {
        require(
            PendingHookEvent::client(name, snapshot.session_context(session), ClientId(7), None),
            &["client"],
        );
    }
    for name in ["paste-buffer-changed", "paste-buffer-deleted"] {
        require(
            PendingHookEvent::paste_buffer(name, "named".to_owned()),
            &["paste_buffer"],
        );
    }
}

#[test]
fn hook_payloads_drop_the_keys_3_8_does_not_fire() {
    let full = BTreeMap::from(
        [
            "hook",
            "hook_event",
            "hook_client",
            "hook_session",
            "hook_session_name",
            "hook_window",
            "hook_window_name",
            "hook_window_index",
            "hook_pane",
            "hook_old_name",
            "hook_new_name",
            "hook_exit_status",
            "unzoomed_layout",
        ]
        .map(|key| (key.to_owned(), "x".to_owned())),
    );
    let fit = |name: &str| {
        let mut variables = full.clone();
        hook_events::fit_payload(name, &mut variables);
        variables.into_keys().collect::<Vec<_>>().join(" ")
    };
    assert_eq!(
        fit("window-renamed"),
        "hook hook_event hook_new_name hook_old_name hook_window hook_window_name unzoomed_layout"
    );
    assert_eq!(
        fit("pane-exited"),
        "hook hook_event hook_exit_status hook_pane hook_window hook_window_name unzoomed_layout"
    );
    assert_eq!(
        fit("alert-bell"),
        "hook hook_event hook_session hook_session_name hook_window hook_window_index hook_window_name unzoomed_layout"
    );
    assert_eq!(fit("agent-state-changed"), fit("@user"));
    assert_eq!(fit("@user").split(' ').count(), full.len());
}

#[test]
fn client_hook_name_matches_client_format_name_fallbacks() {
    let shared = Arc::new(Shared::new(1));
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("macbook".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let mut inner = shared.inner.lock();

    assert_eq!(client_format_name(&inner, client), "macbook");
    inner.client_entry(client).tty.replace(String::new());
    assert_eq!(client_format_name(&inner, client), "macbook");
    inner
        .client_entry(client)
        .tty
        .replace("/dev/pts/40".to_owned());
    assert_eq!(client_format_name(&inner, client), "/dev/pts/40");
    if let Some(registered) = inner.client_mut(client) {
        registered.tty = None;
    }
    inner.client_entry(client).name.replace(String::new());
    inner.client_entry(client).pid.replace(4242);
    assert_eq!(client_format_name(&inner, client), "client-4242");
    inner.client_entry(client).pid.replace(0);
    assert_eq!(
        client_format_name(&inner, client),
        format!("device-{}", client.0)
    );
}

#[test]
fn displayed_pid_fallback_is_an_attached_client_target() {
    let shared = Arc::new(Shared::new(1));
    let (client, _) =
        shared.register_subscribed(ClientKind::Control, None, None, OutboundMailbox::new());
    let mut inner = shared.inner.lock();
    let (session, _, _) = inner
        .engine
        .state
        .create_session("pid-target")
        .expect("pid target session");
    inner
        .attachment
        .attached
        .entry(session)
        .or_default()
        .insert(client);
    inner.client_entry(client).pid.replace(4242);

    assert_eq!(client_format_name(&inner, client), "client-4242");
    assert_eq!(find_attached_client(&inner, "client-4242"), Some(client));
}

#[test]
fn pane_focus_follows_the_pinned_predicate_and_queues_ahead_of_the_client_hooks() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "pane-focus", "focus");
    let mut hook_context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("pane focus hook context");
    install_client_event_log_hooks(
        &shared,
        &mut hook_context,
        &[
            "pane-focus-in",
            "pane-focus-out",
            "client-session-changed",
            "client-attached",
            "client-detached",
        ],
    );
    let (first, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("first-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (second, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("second-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.inner.lock().history.message_log.clear();

    shared.attach(first, session).expect("attach first client");
    assert_eq!(
        client_event_log(&shared)
            .iter()
            .map(|entry| entry.split('|').next().unwrap_or_default().to_owned())
            .collect::<Vec<_>>(),
        ["pane-focus-in", "client-session-changed", "client-attached"]
    );
    assert_eq!(
        shared.inner.lock().attachment.pane_focus,
        BTreeSet::from([pane])
    );

    shared
        .attach(second, session)
        .expect("attach second client");
    shared.inner.lock().history.message_log.clear();

    let report = |client, focused| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                InputMessage::ClientFocus { focused },
            )
            .expect("report client focus");
    };

    report(first, false);
    assert!(
        !client_event_log(&shared)
            .iter()
            .any(|entry| entry.starts_with("pane-focus")),
        "the pane stays focused while the second client still is"
    );
    report(second, false);
    report(first, true);
    report(second, true);
    report(first, false);
    report(second, false);
    assert_eq!(
        client_event_log(&shared)
            .iter()
            .filter(|entry| entry.starts_with("pane-focus"))
            .map(|entry| entry.split('|').next().unwrap_or_default().to_owned())
            .collect::<Vec<_>>(),
        ["pane-focus-out", "pane-focus-in", "pane-focus-out"]
    );
    assert!(shared.inner.lock().attachment.pane_focus.is_empty());

    shared.attach(first, session).expect("re-attach");
    shared.inner.lock().history.message_log.clear();
    shared.detach(first);
    shared.detach(second);
    assert_eq!(
        client_event_log(&shared)
            .iter()
            .map(|entry| entry.split('|').next().unwrap_or_default().to_owned())
            .collect::<Vec<_>>(),
        ["pane-focus-out", "client-detached", "client-detached"]
    );
}

#[test]
fn client_focus_and_activity_hooks_preserve_reports_order_and_latest_changes() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "client-hooks", "focus");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .expect("client hook window");
    let (first, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("first-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (second, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("second-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    {
        let mut inner = shared.inner.lock();
        inner
            .client_entry(first)
            .tty
            .replace("/dev/pts/41".to_owned());
        inner
            .client_entry(second)
            .tty
            .replace("/dev/pts/42".to_owned());
        inner
            .client_entry(control)
            .tty
            .replace("/dev/pts/43".to_owned());
    }
    shared.attach(first, session).expect("attach first client");
    shared
        .attach(second, session)
        .expect("attach second client");
    shared
        .attach(control, session)
        .expect("attach control client");
    let mut hook_context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("client hook context");
    install_client_event_log_hooks(
        &shared,
        &mut hook_context,
        &["client-focus-in", "client-focus-out", "client-active"],
    );
    let context_ids = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut hook_context,
            &CommandInvocation::new(
                "display-message",
                ["-p", "#{session_id}|#{window_id}|#{pane_id}"],
            ),
        )
        .expect("expand client hook context")
        .output;
    shared.inner.lock().history.message_log.clear();

    for focused in [true, true, false] {
        shared
            .input(
                first,
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                InputMessage::ClientFocus { focused },
            )
            .expect("report first client focus");
    }
    shared
        .input(
            control,
            ClientKind::Control,
            &mut ExecutionContext::default(),
            InputMessage::ClientFocus { focused: true },
        )
        .expect("report control focus");
    shared.note_terminal_input(first, pane);
    shared.note_terminal_input(second, pane);
    shared.note_terminal_input(second, pane);
    shared.note_terminal_input(control, pane);

    assert_eq!(
        client_event_log(&shared),
        [
            format!("client-focus-in|/dev/pts/41|{context_ids}"),
            format!("client-active|/dev/pts/41|{context_ids}"),
            format!("client-focus-in|/dev/pts/41|{context_ids}"),
            format!("client-focus-out|/dev/pts/41|{context_ids}"),
            format!("client-active|/dev/pts/42|{context_ids}"),
        ]
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .attachment
            .window_latest_clients
            .get(&window),
        Some(&second)
    );

    let mut no_hooks = ExecutionContext::default();
    no_hooks.no_hooks = true;
    shared
        .input(
            first,
            ClientKind::Interactive,
            &mut no_hooks,
            InputMessage::ClientFocus { focused: true },
        )
        .expect("suppress client focus hooks");
    assert_eq!(client_event_log(&shared).len(), 5);
    assert_eq!(
        shared
            .inner
            .lock()
            .attachment
            .window_latest_clients
            .get(&window),
        Some(&first)
    );

    let mut message_context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("message context");
    shared
        .execute(
            first,
            ClientKind::Interactive,
            &mut message_context,
            &CommandInvocation::new("display-message", ["-N", "-d", "5000", "ignore focus"]),
        )
        .expect("arm ignored focus input");
    shared.inner.lock().history.message_log.clear();
    shared
        .input(
            first,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ClientFocus { focused: false },
        )
        .expect("report ignored client focus");
    assert_eq!(
        client_event_log(&shared),
        [format!("client-focus-out|/dev/pts/41|{context_ids}")]
    );
    let inner = shared.inner.lock();
    assert!(inner.client(first).is_some_and(|c| c.message.is_some()));
    assert!(inner.client(first).is_some_and(|c| c.message_ignore_keys));
    assert_eq!(
        inner.attachment.window_latest_clients.get(&window),
        Some(&first)
    );
}

#[test]
fn client_resize_and_theme_hooks_fire_for_every_interactive_report() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "client-reports", "reports");
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("reporter".to_owned()),
        Some(TerminalColorScheme::Dark),
        OutboundMailbox::new(),
    );
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control-reporter".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    {
        let mut inner = shared.inner.lock();
        inner
            .client_entry(interactive)
            .tty
            .replace("/dev/pts/50".to_owned());
        inner
            .client_entry(control)
            .tty
            .replace("/dev/pts/51".to_owned());
    }
    shared
        .attach(interactive, session)
        .expect("attach interactive reporter");
    shared
        .attach(control, session)
        .expect("attach control reporter");
    let mut hook_context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("client report context");
    install_client_event_log_hooks(
        &shared,
        &mut hook_context,
        &[
            "client-active",
            "client-resized",
            "client-dark-theme",
            "client-light-theme",
        ],
    );
    let context_ids = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut hook_context,
            &CommandInvocation::new(
                "display-message",
                ["-p", "#{session_id}|#{window_id}|#{pane_id}"],
            ),
        )
        .expect("expand client report context")
        .output;
    shared.inner.lock().history.message_log.clear();

    for (kind, client, columns, rows) in [
        (ClientKind::Interactive, interactive, 120, 40),
        (ClientKind::Interactive, interactive, 120, 40),
        (ClientKind::Interactive, interactive, 0, 40),
        (ClientKind::Control, control, 90, 30),
    ] {
        shared
            .input(
                client,
                kind,
                &mut ExecutionContext::default(),
                InputMessage::ClientTerminalSize { columns, rows },
            )
            .expect("report client terminal size");
    }
    for color_scheme in [
        TerminalColorScheme::Dark,
        TerminalColorScheme::Dark,
        TerminalColorScheme::Light,
        TerminalColorScheme::Light,
    ] {
        shared.set_client_color_scheme(interactive, color_scheme);
    }
    shared.set_client_color_scheme(control, TerminalColorScheme::Dark);

    assert_eq!(
        client_event_log(&shared),
        [
            format!("client-active|/dev/pts/50|{context_ids}"),
            format!("client-resized|/dev/pts/50|{context_ids}"),
            format!("client-resized|/dev/pts/50|{context_ids}"),
            format!("client-dark-theme|/dev/pts/50|{context_ids}"),
            format!("client-dark-theme|/dev/pts/50|{context_ids}"),
            format!("client-light-theme|/dev/pts/50|{context_ids}"),
            format!("client-light-theme|/dev/pts/50|{context_ids}"),
        ]
    );
    let inner = shared.inner.lock();
    assert_eq!(
        inner.client(interactive).and_then(|c| c.size.as_ref()),
        Some(&(120, 40))
    );
    assert_eq!(
        inner
            .client(interactive)
            .and_then(|c| c.color_scheme.as_ref()),
        Some(&TerminalColorScheme::Light)
    );
    assert!(
        inner
            .client(control)
            .is_none_or(|c| c.color_scheme.is_none())
    );
}

#[test]
fn detaching_latest_client_promotes_a_surviving_control_client() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "client-promotion", "promote");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .expect("promotion window");
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("surviving-control".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (departing, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("departing-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    {
        let mut inner = shared.inner.lock();
        inner
            .client_entry(control)
            .tty
            .replace("/dev/pts/60".to_owned());
        inner
            .client_entry(departing)
            .tty
            .replace("/dev/pts/61".to_owned());
    }
    shared
        .attach(control, session)
        .expect("attach control survivor");
    shared
        .attach(departing, session)
        .expect("attach departing client");
    let mut hook_context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("promotion hook context");
    install_client_event_log_hooks(
        &shared,
        &mut hook_context,
        &["client-active", "client-detached"],
    );
    let context_ids = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut hook_context,
            &CommandInvocation::new(
                "display-message",
                ["-p", "#{session_id}|#{window_id}|#{pane_id}"],
            ),
        )
        .expect("expand promotion context")
        .output;
    shared.inner.lock().history.message_log.clear();

    shared.detach(departing);

    assert_eq!(
        client_event_log(&shared),
        [
            format!("client-active|/dev/pts/60|{context_ids}"),
            format!("client-detached|/dev/pts/61|{context_ids}"),
        ]
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .attachment
            .window_latest_clients
            .get(&window),
        Some(&control)
    );
}

#[test]
fn snapshot_publication_prunes_destroyed_latest_windows() {
    let shared = Arc::new(Shared::new(1));
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("latest-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let destroyed = WindowId(u64::MAX);
    shared
        .inner
        .lock()
        .attachment
        .window_latest_clients
        .insert(destroyed, client);

    shared.publish_snapshot_state();

    assert_eq!(
        shared
            .inner
            .lock()
            .attachment
            .window_latest_clients
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        []
    );
}

#[test]
fn window_link_events_carry_the_session_that_owned_the_link_edge() {
    let mut engine = MuxEngine::default();
    let (_, window, _) = engine
        .state
        .create_session("source")
        .expect("source session");
    let (linked_session, _, _) = engine
        .state
        .create_session("linked")
        .expect("linked session");
    let before = MuxHookSnapshot::capture(&engine);
    engine
        .state
        .session_mut(linked_session)
        .expect("linked session state")
        .windows
        .push(window);
    let linked = MuxHookSnapshot::capture(&engine);
    let added = mux_hook_events(&before, &linked, "link-window")
        .into_iter()
        .find(|event| event.name == "window-linked")
        .expect("window linked event");
    assert_eq!(
        added.variables.get("hook_session"),
        Some(&linked_session.to_string())
    );

    engine
        .state
        .session_mut(linked_session)
        .expect("linked session state")
        .windows
        .retain(|candidate| *candidate != window);
    let unlinked = MuxHookSnapshot::capture(&engine);
    let removed = mux_hook_events(&linked, &unlinked, "unlink-window")
        .into_iter()
        .find(|event| event.name == "window-unlinked")
        .expect("window unlinked event");
    assert_eq!(
        removed.variables.get("hook_session"),
        Some(&linked_session.to_string())
    );
}

#[test]
fn show_messages_logs_commands_newest_first_and_bounds_the_ring() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "work"]),
        )
        .unwrap();

    let displayed = shared.execute_command_request(
        client,
        ClientKind::Command,
        &mut context,
        1,
        &CommandInvocation::new("display-message", ["hello"]),
    );
    assert!(matches!(
        displayed,
        CommandResponse::Success { output, .. } if output.is_empty()
    ));
    let shown = shared.execute_command_request(
        client,
        ClientKind::Command,
        &mut context,
        2,
        &CommandInvocation::new("showmsgs", [] as [&str; 0]),
    );
    let CommandResponse::Success { output, .. } = shown else {
        panic!("show-messages should succeed");
    };
    let lines = output.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].ends_with(": device-7 command: show-messages"));
    assert!(lines[1].ends_with(": hello"));
    assert!(lines[2].ends_with(": device-7 command: display-message hello"));
    assert!(lines.iter().all(|line| {
        let bytes = line.as_bytes();
        bytes.get(2) == Some(&b':') && bytes.get(5) == Some(&b':')
    }));

    let mut inner = shared.inner.lock();
    inner.history.message_log.clear();
    inner.history.next_message_number = 0;
    drop(inner);
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-s", "message-limit", "3"]),
        )
        .unwrap();
    let mut inner = shared.inner.lock();
    for number in 0..5 {
        push_server_message(&mut inner, format!("message-{number}"));
    }
    assert_eq!(
        inner
            .history
            .message_log
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        ["message-2", "message-3", "message-4"]
    );
    assert_eq!(
        inner
            .history
            .message_log
            .iter()
            .rev()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        ["message-4", "message-3", "message-2"]
    );
    inner.history.message_log.clear();
    inner.history.next_message_number = 0;
    drop(inner);

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-su", "message-limit"]),
        )
        .unwrap();
    let mut inner = shared.inner.lock();
    for number in 0..5 {
        push_server_message(&mut inner, format!("message-{number}"));
    }
    drop(inner);
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-s", "message-limit", "3"]),
        )
        .unwrap();
    let mut inner = shared.inner.lock();
    assert_eq!(inner.history.message_log.len(), 5);
    push_server_message(&mut inner, "message-5".to_owned());
    assert_eq!(
        inner
            .history
            .message_log
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        ["message-3", "message-4", "message-5"]
    );
}

#[test]
fn command_hooks_fire_at_the_daemon_boundary_with_formats_and_nohooks() {
    let shared = Arc::new(Shared::new(1));
    let (session, first_pane, _) = output_view_session_fixture(&shared, "hooks", "first");
    let (first_window, second_window) = {
        let mut inner = shared.inner.lock();
        let first_window = inner
            .engine
            .state
            .window_for_pane(first_pane)
            .expect("first window");
        let (second_window, second_pane) = inner
            .engine
            .state
            .create_window_at(
                session,
                None,
                Some("second".to_owned()),
                PaneKind::Terminal,
                false,
            )
            .expect("second window");
        inner.terminals_mut().insert(
            second_pane,
            Arc::new(TerminalSession::spawn_output_view(
                "second fixture".to_owned(),
                "second".to_owned(),
            )),
        );
        (first_window, second_window)
    };
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, first_pane)
        .expect("hook context");
    let client = ClientId(7);

    for (name, value) in [
        (
            "after-select-window",
            "display-message '#{hook}|#{hook_arguments}|#{hook_flag_t}|#{window_name}'",
        ),
        (
            "command-error",
            "display-message '#{hook}|#{hook_arguments}'",
        ),
        ("after-list-buffers", "display-message 'daemon=#{hook}'"),
        ("after-set-buffer", "list-buffers -F '#{hook}|#{command}'"),
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-hook", [name, value]),
            )
            .unwrap();
    }
    shared.inner.lock().history.message_log.clear();

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("select-window", ["-t", second_window.to_string().as_str()]),
        )
        .unwrap();
    assert_eq!(
        shared
            .inner
            .lock()
            .history
            .message_log
            .iter()
            .map(|message| message.text.clone())
            .collect::<Vec<_>>(),
        [format!(
            "after-select-window|-t {second_window}|{second_window}|second"
        )]
    );

    shared.inner.lock().history.message_log.clear();
    assert!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("select-window", ["-t", "missing"]),
            )
            .is_err()
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .history
            .message_log
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        ["command-error|-t missing"]
    );

    shared.inner.lock().history.message_log.clear();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("list-buffers", [] as [&str; 0]),
        )
        .unwrap();
    assert_eq!(
        shared
            .inner
            .lock()
            .history
            .message_log
            .back()
            .map(|message| message.text.as_str()),
        Some("daemon=after-list-buffers")
    );

    let hook_output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-buffer", ["-b", "hook-row", "value"]),
        )
        .expect("daemon-preempted hook body");
    assert_eq!(hook_output.output, "after-set-buffer|set-buffer");

    for (args, value) in [
        (
            vec!["alert-bell"],
            "display-message 'immediate=#{hook}:#{hook_arguments}'",
        ),
        (
            vec!["after-select-window"],
            "set-hook -R alert-bell ignored",
        ),
    ] {
        let mut command_args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
        command_args.push(value.to_owned());
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-hook", command_args),
            )
            .unwrap();
    }
    shared.inner.lock().history.message_log.clear();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("select-window", ["-t", first_window.to_string().as_str()]),
        )
        .unwrap();
    assert_eq!(
        shared
            .inner
            .lock()
            .history
            .message_log
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        ["immediate=alert-bell:"]
    );

    for (args, value) in [
        (
            vec!["after-set-option"],
            "display-message nested-after-set-option",
        ),
        (
            vec!["after-select-window"],
            "set-option @inside yes ; select-window -t missing ; display-message unreachable",
        ),
        (
            vec!["-a", "after-select-window"],
            "display-message next-index",
        ),
    ] {
        let mut command_args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
        command_args.push(value.to_owned());
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-hook", command_args),
            )
            .unwrap();
    }
    shared.inner.lock().history.message_log.clear();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("select-window", ["-t", second_window.to_string().as_str()]),
        )
        .unwrap();
    assert_eq!(
        shared
            .inner
            .lock()
            .history
            .message_log
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        ["can't find window: missing", "next-index"]
    );
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-options", ["-v", "@inside"]),
            )
            .unwrap()
            .output,
        "yes"
    );
}

#[test]
fn user_hook_run_now_uses_shared_option_storage_and_only_hook_format() {
    let shared = Arc::new(Shared::new(1));
    let (_, pane, _) = output_view_session_fixture(&shared, "user-hook", "first");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("user hook context");
    let client = ClientId(7);

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "@myhook", "v"]),
        )
        .unwrap();
    let hook_command = "display-message -p 'AT-HOOK-RAN|#{hook}|#{hook_arguments}'";
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-hook", ["-g", "@myhook", hook_command]),
        )
        .unwrap();
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-options", ["-g", "-v", "@myhook"]),
            )
            .unwrap()
            .output,
        hook_command
    );
    assert!(
        !shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-hooks", ["-g"]),
            )
            .unwrap()
            .output
            .lines()
            .any(|line| line.starts_with('@'))
    );

    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-hook", ["-R", "@myhook"]),
            )
            .unwrap()
            .output,
        "AT-HOOK-RAN|@myhook|"
    );

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "@myhook", "v"]),
        )
        .unwrap();
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-hook", ["-R", "@myhook"]),
            )
            .unwrap()
            .output,
        ""
    );
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-hook", ["-g", "-u", "@myhook"]),
        )
        .unwrap();
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-options", ["-g", "-q", "-v", "@myhook"]),
            )
            .unwrap()
            .output,
        ""
    );
}

#[test]
fn command_hook_output_joins_the_triggering_client_response_in_pin_order() {
    let shared = Arc::new(Shared::new(1));
    let (session, first_pane, _) = output_view_session_fixture(&shared, "hook-output", "first");
    let second_window = {
        let mut inner = shared.inner.lock();
        let (window, pane) = inner
            .engine
            .state
            .create_window_at(
                session,
                None,
                Some("second".to_owned()),
                PaneKind::Terminal,
                false,
            )
            .expect("second window");
        inner.terminals_mut().insert(
            pane,
            Arc::new(TerminalSession::spawn_output_view(
                "second fixture".to_owned(),
                "second".to_owned(),
            )),
        );
        window
    };
    let client = ClientId(7);
    let mut setup_context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, first_pane)
            .expect("hook setup context");
    for (name, value) in [
        ("after-select-window", "display-message -p FIRED-SW"),
        ("command-error", "display-message -p CMD-ERR"),
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut setup_context,
                &CommandInvocation::new("set-hook", ["-g", name, value]),
            )
            .expect("global hook");
    }

    let success = shared.execute_command_request(
        client,
        ClientKind::Command,
        &mut ExecutionContext::default(),
        1,
        &CommandInvocation::new("select-window", ["-t", second_window.to_string().as_str()]),
    );
    assert_eq!(
        success,
        CommandResponse::Success {
            request_id: 1,
            output: "FIRED-SW".into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );

    let failure = shared.execute_command_request(
        client,
        ClientKind::Command,
        &mut ExecutionContext::default(),
        2,
        &CommandInvocation::new("select-window", ["-t", "missing"]),
    );
    assert_eq!(
        failure,
        CommandResponse::Error {
            request_id: 2,
            error: ServerError::WindowNotFound("missing".to_owned()),
            output: "CMD-ERR".into(),
        }
    );

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut setup_context,
            &CommandInvocation::new("set-buffer", ["-b", "buffer0", "payload"]),
        )
        .expect("buffer fixture");
    for (name, value) in [
        ("after-list-buffers[1]", "display-message -p IDX-B"),
        ("after-list-buffers[0]", "display-message -p IDX-A"),
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut setup_context,
                &CommandInvocation::new("set-hook", ["-g", name, value]),
            )
            .expect("indexed hook");
    }
    let ordered = shared.execute_command_request(
        client,
        ClientKind::Command,
        &mut ExecutionContext::default(),
        3,
        &CommandInvocation::new("list-buffers", ["-F", "OWN-#{buffer_name}"]),
    );
    assert_eq!(
        ordered,
        CommandResponse::Success {
            request_id: 3,
            output: "OWN-buffer0\nIDX-A\nIDX-B".into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );

    for (name, value) in [
        ("after-set-option", "display-message -p SETOPT-FIRED"),
        ("after-display-message", "display-message -p DM-NESTED"),
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut setup_context,
                &CommandInvocation::new("set-hook", ["-g", name, value]),
            )
            .expect("no-hooks fixture");
    }
    let no_hooks = shared.execute_command_request(
        client,
        ClientKind::Command,
        &mut ExecutionContext::default(),
        4,
        &CommandInvocation::new("set-option", ["-g", "@hook-output", "1"]),
    );
    assert_eq!(
        no_hooks,
        CommandResponse::Success {
            request_id: 4,
            output: "SETOPT-FIRED".into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut setup_context,
            &CommandInvocation::new(
                "set-hook",
                ["-g", "alert-bell", "display-message -p R-FIRED"],
            ),
        )
        .expect("run-now fixture");
    let run_now = shared.execute_command_request(
        client,
        ClientKind::Command,
        &mut ExecutionContext::default(),
        5,
        &CommandInvocation::new("set-hook", ["-gR", "alert-bell"]),
    );
    assert_eq!(
        run_now,
        CommandResponse::Success {
            request_id: 5,
            output: "R-FIRED".into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
}

#[test]
fn event_hooks_fire_after_mutation_with_captured_formats() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "automatic-rename", "off"]),
        )
        .expect("hold window names");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "session-created",
                    "set-environment -gF ZZ_TEST_SESSION_CREATED '#{hook}|#{hook_session}|#{hook_session_name}|#{hook_arguments}'",
                ],
            ),
        )
        .expect("session-created hook");
    let created = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "events"]),
        )
        .expect("new session");
    let session = context.session.expect("created session");
    let window = context.window.expect("created window");
    assert!(created.output.is_empty());
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-environment", ["-g", "ZZ_TEST_SESSION_CREATED"],),
            )
            .unwrap()
            .output,
        format!("ZZ_TEST_SESSION_CREATED=session-created|{session}|events|")
    );

    for (name, value) in [
        (
            "window-renamed",
            "set-environment -gF ZZ_TEST_WINDOW_RENAMED '#{hook}|#{hook_window}|#{hook_window_name}'",
        ),
        (
            "window-layout-changed",
            "set-environment -gF ZZ_TEST_WINDOW_LAYOUT_CHANGED '#{hook}|#{hook_window}'",
        ),
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-hook", ["-g", name, value]),
            )
            .expect("window event hook");
    }
    let renamed = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("rename-window", ["-t", &window.to_string(), "named"]),
        )
        .expect("rename window");
    assert!(renamed.output.is_empty());
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-environment", ["-g", "ZZ_TEST_WINDOW_RENAMED"],),
            )
            .unwrap()
            .output,
        format!("ZZ_TEST_WINDOW_RENAMED=window-renamed|{window}|named")
    );

    let split = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("split-window", ["-d"]),
        )
        .expect("split window");
    assert!(split.output.is_empty());
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "show-environment",
                    ["-g", "ZZ_TEST_WINDOW_LAYOUT_CHANGED"],
                ),
            )
            .unwrap()
            .output,
        format!("ZZ_TEST_WINDOW_LAYOUT_CHANGED=window-layout-changed|{window}")
    );
}

#[test]
fn window_event_hooks_fire_only_for_their_window() {
    let shared = Arc::new(Shared::new(1));
    let (session, first_pane, _) = output_view_session_fixture(&shared, "window-hooks", "first");
    let (first_window, second_window) = {
        let mut inner = shared.inner.lock();
        let first_window = inner
            .engine
            .state
            .window_for_pane(first_pane)
            .expect("first window");
        let (second_window, second_pane) = inner
            .engine
            .state
            .create_window_at(
                session,
                None,
                Some("second".to_owned()),
                PaneKind::Terminal,
                false,
            )
            .expect("second window");
        inner.terminals_mut().insert(
            second_pane,
            Arc::new(TerminalSession::spawn_output_view(
                "second fixture".to_owned(),
                "second".to_owned(),
            )),
        );
        (first_window, second_window)
    };
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, first_pane)
        .expect("window hook context");
    let client = ClientId(7);
    let first_target = first_window.to_string();

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "window-renamed",
                    "set-environment -gF ZZ_TEST_WINDOW_RENAMED 'global=#{hook_window_name}'",
                ],
            ),
        )
        .unwrap();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-w",
                    "-t",
                    first_target.as_str(),
                    "window-renamed",
                    "set-environment -gF ZZ_TEST_WINDOW_RENAMED 'local=#{hook_window_name}'",
                ],
            ),
        )
        .unwrap();

    let first_rename = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "rename-window",
                ["-t", first_target.as_str(), "first-renamed"],
            ),
        )
        .unwrap();
    assert!(first_rename.output.is_empty());
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-environment", ["-g", "ZZ_TEST_WINDOW_RENAMED"],),
            )
            .unwrap()
            .output,
        "ZZ_TEST_WINDOW_RENAMED=local=first-renamed"
    );

    let second_rename = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "rename-window",
                ["-t", second_window.to_string().as_str(), "second-renamed"],
            ),
        )
        .unwrap();
    assert!(second_rename.output.is_empty());
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-environment", ["-g", "ZZ_TEST_WINDOW_RENAMED"],),
            )
            .unwrap()
            .output,
        "ZZ_TEST_WINDOW_RENAMED=global=second-renamed"
    );
}

#[test]
fn event_hooks_are_fully_dropped_inside_hooks() {
    let shared = Arc::new(Shared::new(1));
    let (session, first_pane, _) = output_view_session_fixture(&shared, "events", "first");
    let second_window = {
        let mut inner = shared.inner.lock();
        inner
            .engine
            .state
            .create_window_at(
                session,
                None,
                Some("second".to_owned()),
                PaneKind::Terminal,
                false,
            )
            .expect("second window")
            .0
    };
    let client = ClientId(7);
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, first_pane)
        .expect("hook context");
    for (name, value) in [
        (
            "window-layout-changed",
            "display-message event-layout-fired",
        ),
        ("after-select-window", "split-window -d"),
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-hook", ["-g", name, value]),
            )
            .expect("hook setup");
    }
    shared.inner.lock().history.message_log.clear();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("select-window", ["-t", &second_window.to_string()]),
        )
        .expect("select window");
    assert_eq!(
        shared
            .inner
            .lock()
            .history
            .message_log
            .iter()
            .map(|message| message.text.clone())
            .collect::<Vec<_>>(),
        Vec::<String>::new()
    );
}

#[test]
fn terminal_events_fire_rename_title_and_bell_hooks() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, terminal) =
        output_view_session_fixture(&shared, "terminal-events", "first");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .expect("window");
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("event context");
    for (name, value) in [
        (
            "window-renamed",
            "display-message 'renamed=#{hook_window_name}'",
        ),
        (
            "pane-title-changed",
            "display-message 'title=#{hook_pane}:#{hook_window}:#{hook_new_title}'",
        ),
        (
            "alert-bell",
            "display-message 'bell=#{hook_window}/#{hook_pane}'",
        ),
    ] {
        shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-hook", ["-g", name, value]),
            )
            .expect("terminal event hook");
    }
    shared.inner.lock().history.message_log.clear();

    let (window_activity, window_activity_time, session_activity) = {
        let mut inner = shared.inner.lock();
        inner.engine.set_format_now(1);
        inner.engine.state.touch_window_activity_for_pane(pane);
        (
            inner.engine.state.windows[&window].activity,
            inner.engine.state.windows[&window].activity_time,
            inner.engine.state.sessions[&session].sort_activity,
        )
    };
    shared.synchronize_pane_runtime(pane, &terminal, None, "fish", None, false, Instant::now());
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.engine.state.windows[&window].activity,
            window_activity
        );
        assert_eq!(
            inner.engine.state.windows[&window].activity_time,
            window_activity_time
        );
        assert_eq!(
            inner.engine.state.sessions[&session].sort_activity,
            session_activity
        );
    }
    shared.note_pane_output(pane, &terminal, Instant::now());
    {
        let inner = shared.inner.lock();
        assert!(inner.engine.state.windows[&window].activity > window_activity);
        assert!(inner.engine.state.windows[&window].activity_time > window_activity_time);
        assert_eq!(
            inner.engine.state.sessions[&session].sort_activity,
            session_activity
        );
    }
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("rename-window", ["-t", &window.to_string(), "manual"]),
        )
        .expect("explicit rename");
    shared.synchronize_pane_title(pane, &terminal, "osc-title", true);
    shared.raise_pane_bell(pane);
    shared.raise_pane_bell(pane);

    assert_eq!(
        shared
            .inner
            .lock()
            .history
            .message_log
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        [
            "renamed=fish",
            "renamed=manual",
            &format!("title={pane}:{window}:osc-title"),
            &format!("bell={window}/"),
            &format!("bell={window}/"),
        ]
    );
}

#[test]
fn session_closed_hook_keeps_payload_after_the_session_is_gone() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "session-closed",
                    "set-environment -gF ZZ_TEST_SESSION_CLOSED '#{hook_session}|#{hook_session_name}|#{session_name}' ; split-window",
                ],
            ),
        )
        .expect("session-closed hook");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "closing"]),
        )
        .expect("session");
    let session = context.session.expect("session id");
    let closed = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", &session.to_string()]),
        )
        .expect("kill session");
    assert!(closed.output.is_empty());
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-environment", ["-g", "ZZ_TEST_SESSION_CLOSED"],),
            )
            .unwrap()
            .output,
        format!("ZZ_TEST_SESSION_CLOSED={session}|closing|")
    );
    assert!(shared.inner.lock().engine.state.sessions.is_empty());
}

#[test]
fn terminal_exit_fires_pane_exited_or_pane_died_by_retention() {
    let shared = Arc::new(Shared::new(1));
    let (_, anchor, _) = output_view_session_fixture(&shared, "exit-anchor", "anchor");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, anchor)
        .expect("anchor context");
    for (name, value) in [
        ("pane-exited", "display-message 'exited=#{hook_pane}'"),
        ("pane-died", "display-message 'died=#{hook_pane}'"),
        (
            "window-unlinked",
            "display-message 'unlinked=#{hook_session_name}'",
        ),
        (
            "session-closed",
            "display-message 'closed=#{hook_session_name}'",
        ),
    ] {
        shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-hook", ["-g", name, value]),
            )
            .expect("pane exit hook");
    }
    shared.inner.lock().history.message_log.clear();

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "exit-off", "exit"]),
        )
        .expect("non-retained terminal");
    let exited = context.pane.expect("exited pane");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let inner = shared.inner.lock();
        if inner.engine.state.pane(exited).is_none() && inner.history.message_log.len() == 3 {
            break;
        }
        drop(inner);
        assert!(Instant::now() < deadline, "pane-exited hook timed out");
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        shared
            .inner
            .lock()
            .history
            .message_log
            .iter()
            .map(|message| message.text.clone())
            .collect::<Vec<_>>(),
        [
            format!("exited={exited}"),
            "unlinked=exit-off".to_owned(),
            "closed=exit-off".to_owned(),
        ]
    );
    shared.inner.lock().history.message_log.clear();

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "remain-on-exit", "on"]),
        )
        .expect("enable retained exits");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "exit-on", "exit 7"]),
        )
        .expect("retained terminal");
    let died = context.pane.expect("dead pane");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let inner = shared.inner.lock();
        if inner.engine.state.pane(died).is_some_and(|pane| pane.dead)
            && inner
                .history
                .message_log
                .iter()
                .any(|message| message.text == format!("died={died}"))
        {
            break;
        }
        drop(inner);
        assert!(Instant::now() < deadline, "pane-died hook timed out");
        thread::sleep(Duration::from_millis(10));
    }
    let messages = shared
        .inner
        .lock()
        .history
        .message_log
        .iter()
        .map(|message| message.text.clone())
        .collect::<Vec<_>>();
    assert_eq!(messages, [format!("died={died}")]);
}

#[test]
fn copy_mode_entry_and_exit_fire_pane_mode_changed() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "mode-hook", "mode");
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("mode-client".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(client, session).expect("attach");
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("mode context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "pane-mode-changed",
                    "display-message 'mode=#{hook_pane}'",
                ],
            ),
        )
        .expect("mode hook");
    shared.inner.lock().history.message_log.clear();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("copy-mode", ["-t", &pane.to_string()]),
        )
        .expect("enter copy mode");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("send-keys", ["-t", &pane.to_string(), "-X", "cancel"]),
        )
        .expect("exit copy mode");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::TerminalView {
                pane,
                action: zz_terminal::TerminalViewAction::EnterCopyMode,
            },
        )
        .expect("enter copy mode from the terminal view");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::TerminalView {
                pane,
                action: zz_terminal::TerminalViewAction::CopyMode(
                    zz_terminal::CopyModeAction::Cancel,
                ),
            },
        )
        .expect("exit copy mode from the terminal view");
    assert_eq!(
        shared
            .inner
            .lock()
            .history
            .message_log
            .iter()
            .map(|message| message.text.clone())
            .collect::<Vec<_>>(),
        [
            format!("mode={pane}"),
            format!("mode={pane}"),
            format!("mode={pane}"),
            format!("mode={pane}"),
        ]
    );
}

#[test]
fn terminal_resize_fires_layout_then_window_resized() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "resize-hooks", "resize");
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("resize-client".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(client, session).expect("attach");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("resize context");
    for (name, value) in [
        (
            "window-layout-changed",
            "display-message window-layout-changed",
        ),
        ("window-resized", "display-message window-resized"),
    ] {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("set-hook", ["-g", name, value]),
            )
            .expect("resize hook");
    }
    shared.inner.lock().history.message_log.clear();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 120,
                rows: 40,
                cell_width_px: 8,
                cell_height_px: 16,
            },
        )
        .expect("resize terminal");
    assert_eq!(
        shared
            .inner
            .lock()
            .history
            .message_log
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        ["window-layout-changed", "window-resized"]
    );
}

#[test]
fn command_error_hook_output_opens_only_the_triggering_interactive_client_view() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let observer_mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (observer, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&observer_mailbox),
    );
    let (session, pane, _) = output_view_session_fixture(&shared, "hook-interactive", "first");
    shared
        .attach(client, session)
        .expect("attach triggering client");
    shared.attach(observer, session).expect("attach observer");
    take_reliable_messages(&mailbox);
    take_reliable_messages(&observer_mailbox);

    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("interactive hook context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                ["-g", "command-error", "display-message -p CMD-ERR"],
            ),
        )
        .expect("command-error hook");
    let response = shared.execute_command_request(
        client,
        ClientKind::Interactive,
        &mut context,
        6,
        &CommandInvocation::new("select-window", ["-t", "missing"]),
    );
    assert_eq!(
        response,
        CommandResponse::Error {
            request_id: 6,
            error: ServerError::WindowNotFound("missing".to_owned()),
            output: "CMD-ERR".into(),
        }
    );

    let message = take_command_output_message(&mailbox);
    let ProtocolMessage::Event(Event {
        payload:
            EventPayload::CommandOutput {
                pane: output_pane,
                viewport: Some(viewport),
                ..
            },
        ..
    }) = message
    else {
        panic!("expected hook command-output viewport");
    };
    assert_eq!(output_pane, pane);
    assert!(viewport_text(&viewport).contains("CMD-ERR"));
    assert!(observer_mailbox.state.lock().command_output.is_none());
    assert!(shared.read_client(client, |c| c.is_some_and(|c| c.command_output.is_some())));
    assert!(shared.read_client(observer, |c| c.is_none_or(|c| c.command_output.is_none())));
}

#[test]
fn configured_new_window_hooks_run_in_order_in_the_created_context() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mux_config = directory.path().join("mux.conf");
    fs::write(
        &mux_config,
        "set-hook -g after-new-window[2] \"display-message -p 'NW-SECOND=#{window_name}'\"\n\
             set-hook -g after-new-window[0] \"display-message -p 'NW-FIRST=#{window_name}'\"\n",
    )
    .expect("hook config fixture");

    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .load_config_file(&mux_config, &mut context, 0)
        .expect("load hook config");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "hook-config", "--", "sleep", "30"],
            ),
        )
        .expect("hook session");
    let session = context.session.expect("hook session id");
    shared.inner.lock().history.message_log.clear();

    let response = shared.execute_command_request(
        ClientId(7),
        ClientKind::Command,
        &mut context,
        1,
        &CommandInvocation::new("new-window", ["-d", "-n", "hooked", "--", "sleep", "30"]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 1,
            output: "NW-FIRST=hooked\nNW-SECOND=hooked".into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );

    let reused = shared.execute_command_request(
        ClientId(7),
        ClientKind::Command,
        &mut context,
        2,
        &CommandInvocation::new(
            "new-window",
            ["-S", "-n", "hooked", "-P", "-F", "SHOULD-NOT-PRINT"],
        ),
    );
    assert_eq!(
        reused,
        CommandResponse::Success {
            request_id: 2,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", session.to_string().as_str()]),
        )
        .expect("clean up hook session");
}

#[test]
fn pane_creation_output_waits_for_spawn_facts_and_preserves_empty_lines() {
    let sleep = if cfg!(windows) { "timeout" } else { "sleep" };
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "format-output", "--", sleep, "30"],
            ),
        )
        .expect("format output session");
    let session = context.session.expect("format output session id");
    let source_pane = context.pane.expect("format output source pane");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-window",
                [
                    "-d",
                    "-P",
                    "-F",
                    "#{pane_start_path}|#{pane_pid}|#{pane_tty}|#{pane_start_command}",
                    "--",
                    sleep,
                    "30",
                ],
            ),
        )
        .expect("formatted new window")
        .output;
    let fields = output.split('|').collect::<Vec<_>>();
    assert_eq!(fields.len(), 4);
    assert!(!fields[0].is_empty());
    assert!(fields[1].parse::<u32>().is_ok_and(|pid| pid != 0));
    if cfg!(unix) {
        assert!(!fields[2].is_empty());
    }
    assert!(fields[3].contains(sleep));
    assert!(fields[3].contains("30"));

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-window", ["-d", "-P", "-F", "#{command}"]),
        )
        .expect("new-window command item")
        .output;
    assert_eq!(output, "new-window\n");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("split-window", ["-d", "-P", "-F", "#{command}"]),
        )
        .expect("split-window command item")
        .output;
    assert_eq!(output, "split-window\n");

    context
        .format_variables
        .insert("command".to_owned(), "explicit".to_owned());
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("split-window", ["-d", "-P", "-F", "#{command}"]),
        )
        .expect("split-window explicit command state")
        .output;
    assert_eq!(output, "explicit\n");
    context.format_variables.remove("command");

    let empty = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("split-window", ["-d", "-P", "-F", ""]),
        )
        .expect("empty formatted split")
        .output;
    assert_eq!(empty.as_bytes(), b"\n");

    let trailing_newline = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("split-window", ["-d", "-P", "-F", "X\n"]),
        )
        .expect("formatted split ending in a newline")
        .output;
    assert_eq!(trailing_newline.as_bytes(), b"X\n\n");

    let (attached_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("format-output-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .attach(attached_client, session)
        .expect("attach format output client");
    let mut attached_context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, source_pane)
            .expect("attached format output context");
    let output = shared
        .execute(
            attached_client,
            ClientKind::Interactive,
            &mut attached_context,
            &CommandInvocation::new("new-window", ["-d", "-P", "-F", "#{session_active}"]),
        )
        .expect("attached new-window format")
        .output;
    assert_eq!(output, "1\n");
}

#[test]
fn failing_commands_log_pin_shaped_message_and_command_pairs() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "work"]),
        )
        .unwrap();

    let response = shared.execute_command_request(
        client,
        ClientKind::Command,
        &mut context,
        1,
        &CommandInvocation::new("select-window", ["-t", "99"]),
    );
    assert!(matches!(
        response,
        CommandResponse::Error {
            request_id: 1,
            error: ServerError::WindowNotFound(window),
            output,
        } if window == "99" && output.is_empty()
    ));

    let response = shared.execute_command_request(
        client,
        ClientKind::Command,
        &mut context,
        2,
        &CommandInvocation::new("wibble", [] as [&str; 0]),
    );
    assert!(matches!(
        response,
        CommandResponse::Error {
            request_id: 2,
            error: ServerError::CommandParse(message),
            output,
        } if message == "unknown command: wibble" && output.is_empty()
    ));

    let response = shared.execute_command_request(
        client,
        ClientKind::Command,
        &mut context,
        3,
        &CommandInvocation::new("link-window", [] as [&str; 0]),
    );
    assert!(matches!(
        response,
        CommandResponse::Error {
            request_id: 3,
            error: ServerError::UnsupportedCommand(message),
            output,
        } if message == "link-window" && output.is_empty()
    ));

    let inner = shared.inner.lock();
    assert_eq!(inner.history.message_log.len(), 6);
    assert_eq!(inner.history.next_message_number, 6);
    assert_eq!(
        inner
            .history
            .message_log
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        [
            "device-7 command: select-window -t 99",
            "device-7 message: can't find window: 99",
            "device-7 command: wibble",
            "device-7 message: unknown command: wibble",
            "device-7 command: link-window",
            "device-7 message: unsupported command: link-window",
        ]
    );
    drop(inner);

    let output = shared.show_messages("show-messages", &[]).unwrap().output;
    let lines = output.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 6);
    assert!(lines[0].ends_with(": device-7 message: unsupported command: link-window"));
    assert!(lines[1].ends_with(": device-7 command: link-window"));
    assert!(lines[2].ends_with(": device-7 message: unknown command: wibble"));
    assert!(lines[3].ends_with(": device-7 command: wibble"));
    assert!(lines[4].ends_with(": device-7 message: can't find window: 99"));
    assert!(lines[5].ends_with(": device-7 command: select-window -t 99"));
    assert!(lines.iter().all(|line| {
        let bytes = line.as_bytes();
        bytes.get(2) == Some(&b':') && bytes.get(5) == Some(&b':')
    }));
}

#[cfg(unix)]
#[test]
fn command_response_carries_shell_output_and_nonzero_exit_status_together() {
    let shared = Arc::new(Shared::new(1));
    let response = shared.execute_command_request(
        ClientId(7),
        ClientKind::Command,
        &mut ExecutionContext::default(),
        44,
        &CommandInvocation::new("run-shell", ["printf before; exit 3"]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 44,
            output: "before\n'printf before; exit 3' returned 3".into(),
            exit_code: 3,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
}
