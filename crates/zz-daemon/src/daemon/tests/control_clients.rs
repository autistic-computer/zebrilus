use super::*;

#[test]
fn control_output_fans_out_only_to_attached_control_clients() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "output", "pane");
    let first_mailbox = OutboundMailbox::new();
    let second_mailbox = OutboundMailbox::new();
    let interactive_mailbox = OutboundMailbox::new();
    let (first, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&first_mailbox));
    let (second, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&second_mailbox));
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&interactive_mailbox),
    );
    shared.attach(first, session).expect("attach first control");
    shared
        .attach(second, session)
        .expect("attach second control");
    shared
        .attach(interactive, session)
        .expect("attach interactive");
    take_reliable_messages(&first_mailbox);
    take_reliable_messages(&second_mailbox);
    take_reliable_messages(&interactive_mailbox);

    shared.publish_control_output_for_pane(pane, &Arc::from(b"bytes".as_slice()));
    for mailbox in [&first_mailbox, &second_mailbox] {
        assert!(take_reliable_messages(mailbox).iter().any(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::PaneOutput { pane: target, bytes },
                    ..
                }) if *target == pane && bytes == b"bytes"
            )
        }));
    }
    let interactive_messages = take_reliable_messages(&interactive_mailbox);
    assert!(
        interactive_messages.iter().all(|message| !matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::PaneOutput { .. },
                ..
            })
        )),
        "interactive client received control output: {interactive_messages:#?}"
    );
}

#[test]
fn refresh_client_controls_pane_entry_gates_and_state_notifications() {
    let shared = Arc::new(Shared::new(1));
    let (session, window, pane) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("control-state")
        .expect("control state session");
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(client, session).expect("attach control");
    take_reliable_messages(&mailbox);
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let state =
        |state: &str| CommandInvocation::new("refresh-client", ["-A", &format!("{pane}:{state}")]);

    shared
        .execute(client, ClientKind::Control, &mut context, &state("off"))
        .expect("disable pane output");
    shared.publish_control_output_for_pane(pane, &Arc::from(b"hidden".as_slice()));
    shared
        .execute(client, ClientKind::Control, &mut context, &state("on"))
        .expect("enable pane output");
    shared.publish_control_output_for_pane(pane, &Arc::from(b"visible".as_slice()));
    shared
        .execute(client, ClientKind::Control, &mut context, &state("pause"))
        .expect("pause pane output");
    shared
        .execute(client, ClientKind::Control, &mut context, &state("pause"))
        .expect("pause pane output twice");
    shared.publish_control_output_for_pane(pane, &Arc::from(b"paused".as_slice()));
    shared
        .execute(
            client,
            ClientKind::Control,
            &mut context,
            &state("continue"),
        )
        .expect("continue pane output");
    shared
        .execute(
            client,
            ClientKind::Control,
            &mut context,
            &state("continue"),
        )
        .expect("continue pane output twice");
    shared.publish_control_output_for_pane(pane, &Arc::from(b"resumed".as_slice()));
    shared
        .execute(
            client,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new(
                "refresh-client",
                [
                    "-A",
                    "bad",
                    "-A",
                    "%999999:on",
                    "-A",
                    &format!("{pane}:bogus"),
                ],
            ),
        )
        .expect("ignore malformed pane states");

    let payloads = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event { payload, .. }) => Some(payload),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(payloads.iter().all(|payload| !matches!(
        payload,
        EventPayload::PaneOutput { bytes, .. } if bytes == b"hidden" || bytes == b"paused"
    )));
    assert_eq!(
        payloads
            .iter()
            .filter_map(|payload| match payload {
                EventPayload::PaneOutput { bytes, .. } => Some(bytes.as_slice()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [b"visible".as_slice(), b"resumed".as_slice()]
    );
    assert_eq!(
        payloads
            .iter()
            .filter_map(|payload| match payload {
                EventPayload::PaneOutputState { paused, .. } => Some(*paused),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [true, false]
    );
}

#[test]
fn refresh_client_a_requires_control_and_flags_round_trip() {
    let mut parsed_flags = ControlClientOutput::default();
    apply_control_client_flags(&mut parsed_flags, "no-output,wait-exit,pause-after,unknown");
    assert!(parsed_flags.no_output);
    assert!(parsed_flags.wait_exit);
    assert_eq!(parsed_flags.pause_after_ms, Some(0));
    apply_control_client_flags(
        &mut parsed_flags,
        "!no-output,!wait-exit,!pause-after,pause-after=3",
    );
    assert!(!parsed_flags.no_output);
    assert!(!parsed_flags.wait_exit);
    assert_eq!(parsed_flags.pause_after_ms, Some(3000));
    apply_control_client_flags(&mut parsed_flags, "!pause-after=3,pause-after=overflow");
    assert_eq!(parsed_flags.pause_after_ms, None);

    let shared = Arc::new(Shared::new(1));
    let (session, window, pane) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("control-flags")
        .expect("control flags session");
    let control_mailbox = OutboundMailbox::new();
    let interactive_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        Arc::clone(&control_mailbox),
    );
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("interactive".to_owned()),
        None,
        Arc::clone(&interactive_mailbox),
    );
    shared.attach(control, session).expect("attach control");
    shared
        .attach(interactive, session)
        .expect("attach interactive");
    take_reliable_messages(&control_mailbox);
    take_reliable_messages(&interactive_mailbox);
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));

    let error = shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-A", &format!("{pane}:off")]),
        )
        .expect_err("non-control pane state");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "not a control client"
    ));
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-f", "no-output,pause-after=2"]),
        )
        .expect("ignore control flags on an interactive client");
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "refresh-client",
                ["-t", "control", "-F", "no-output,wait-exit,pause-after=2"],
            ),
        )
        .expect("set control flags from another client");
    shared.publish_control_output_for_pane(pane, &Arc::from(b"suppressed".as_slice()));

    let listed = shared
        .list_clients(
            &context,
            "list-clients",
            &["-F".into(), "#{client_name}:#{client_flags}".into()],
        )
        .expect("list client flags")
        .output;
    assert!(
        listed.contains("control:attached,focused,control-mode,no-output,wait-exit,pause-after=2")
    );
    assert!(listed.contains("interactive:attached"));
    let messages = take_reliable_messages(&control_mailbox);
    assert!(messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::ControlFlags {
                wait_exit: true,
                pause_after_ms: Some(2000),
                no_output: true,
                new_layouts: false,
            },
            ..
        })
    )));
    assert!(messages.iter().all(|message| !matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::PaneOutput { bytes, .. },
            ..
        }) if bytes == b"suppressed"
    )));

    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new(
                "refresh-client",
                ["-f", "!no-output,!wait-exit,!pause-after"],
            ),
        )
        .expect("clear control flags");
    assert_eq!(
        format_client_flags(&shared.inner.lock(), control),
        "attached,focused,control-mode"
    );
}

#[test]
fn control_clients_read_v1_layouts_until_they_set_new_layouts() {
    let shared = Arc::new(Shared::new(1));
    let (session, window, pane) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("layouts")
        .expect("layouts session");
    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        Arc::clone(&control_mailbox),
    );
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("interactive".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(control, session).expect("attach control");
    shared
        .attach(interactive, session)
        .expect("attach interactive");
    take_reliable_messages(&control_mailbox);
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let layouts = |client: ClientId, kind: ClientKind, context: &mut ExecutionContext| {
        let shown = shared
            .execute(
                client,
                kind,
                context,
                &CommandInvocation::new(
                    "display-message",
                    ["-p", "#{window_layout} #{window_visible_layout}"],
                ),
            )
            .expect("display layouts")
            .output
            .to_string();
        let listed = shared
            .execute(
                client,
                kind,
                context,
                &CommandInvocation::new("list-windows", ["-t", "layouts"]),
            )
            .expect("list windows")
            .output
            .to_string();
        (shown, listed)
    };
    let v1 = "b25d,80x24,0,0,0";
    let v2 = r#"{"V":2,"L":{"t":"p","w":80,"h":24,"x":0,"y":0,"a":true,"i":0,"I":"%0"}}"#;
    let (shown, listed) = layouts(control, ClientKind::Control, &mut context);
    assert_eq!(shown.trim_end(), format!("{v1} {v1}"));
    assert!(listed.contains(&format!("[layout {v1}]")), "{listed}");
    let (shown, listed) = layouts(interactive, ClientKind::Interactive, &mut context);
    assert_eq!(shown.trim_end(), format!("{v2} {v2}"));
    assert!(listed.contains(&format!("[layout {v2}]")), "{listed}");

    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-f", "new-layouts"]),
        )
        .expect("set new-layouts");
    assert_eq!(
        format_client_flags(&shared.inner.lock(), control),
        "attached,focused,control-mode,new-layouts"
    );
    assert!(
        take_reliable_messages(&control_mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlFlags {
                        new_layouts: true,
                        ..
                    },
                    ..
                })
            ))
    );
    let (shown, listed) = layouts(control, ClientKind::Control, &mut context);
    assert_eq!(shown.trim_end(), format!("{v2} {v2}"));
    assert!(listed.contains(&format!("[layout {v2}]")), "{listed}");

    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-f", "!new-layouts"]),
        )
        .expect("clear new-layouts");
    let (shown, _) = layouts(control, ClientKind::Control, &mut context);
    assert_eq!(shown.trim_end(), format!("{v1} {v1}"));
    let snapshot = shared.inner.lock().engine.state.snapshot();
    assert_eq!(snapshot.sessions[0].windows[0].layout_dump, v2);
}

#[test]
fn a_control_command_leaves_other_clients_status_layouts_in_v2() {
    let shared = Arc::new(Shared::new(1));
    let ((_alpha, alpha_mailbox, a), _) = two_session_pair(&shared);
    shared
        .execute(
            ClientId(91),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "status-right", "#{window_layout}"]),
        )
        .expect("layout status");
    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        Arc::clone(&control_mailbox),
    );
    shared.attach(control, a).expect("attach control");
    shared.refresh_status();
    take_reliable_messages(&alpha_mailbox);
    let mut context = ExecutionContext::new(Some(a), None, None);
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("split-window", ["-d", "-t", "a", QUIET_PANE_COMMAND]),
        )
        .expect("control split");
    let rights = take_reliable_messages(&alpha_mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::StatusChanged { status },
                ..
            }) => Some(status.right),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!rights.is_empty());
    assert!(
        rights
            .iter()
            .all(|right| right.contains(r#"{"V":2,"L":{"t":"v""#)),
        "{rights:?}"
    );
}

#[test]
fn a_control_clients_show_buffer_keeps_stored_layout_bytes() {
    let shared = Arc::new(Shared::new(1));
    let (session, window, pane) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("stored")
        .expect("stored session");
    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(control, session).expect("attach control");
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let stored = r#"{"V":2,"L":{"t":"p","w":80,"h":24,"x":0,"y":0,"a":true,"i":0,"I":"%0"}}"#;
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("set-buffer", ["-b", "kept", stored]),
        )
        .expect("store layout bytes");
    let shown = shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("show-buffer", ["-b", "kept"]),
        )
        .expect("show buffer")
        .output
        .to_string();
    assert_eq!(shown, stored);
    take_reliable_messages(&mailbox);
    shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut context,
        2,
        &CommandInvocation::new("if-shell", ["-F", "1", "show-buffer -b kept"]),
    );
    let guards = control_command_guards(take_reliable_messages(&mailbox));
    assert!(
        guards
            .iter()
            .any(|(output, error, _)| output == stored && !error),
        "{guards:?}"
    );
    let displayed = shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("display-message", ["-p", "#{window_layout}"]),
        )
        .expect("display layout")
        .output
        .to_string();
    assert_eq!(displayed.trim_end(), "b25d,80x24,0,0,0");
}

#[test]
fn a_control_command_leaves_another_clients_chooser_layouts_in_v2() {
    let shared = Arc::new(Shared::new(1));
    let ((alpha, alpha_mailbox, a), _) = two_session_pair(&shared);
    let (window, pane) = {
        let inner = shared.inner.lock();
        let window = inner.engine.state.sessions[&a].active_window;
        (window, inner.engine.state.windows[&window].active_pane)
    };
    let mut context = ExecutionContext::new(Some(a), Some(window), Some(pane));
    for args in [
        &["set-buffer", "-b", "one", "value"][..],
        &["choose-buffer", "-F", "#{window_layout}"][..],
    ] {
        shared
            .execute(
                alpha,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .expect("open layout chooser");
    }
    take_reliable_messages(&alpha_mailbox);
    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        Arc::clone(&control_mailbox),
    );
    shared.attach(control, a).expect("attach control");
    let mut control_context = ExecutionContext::new(Some(a), Some(window), Some(pane));
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new("set-buffer", ["-b", "two", "value"]),
        )
        .expect("control set-buffer");
    let rows = take_reliable_messages(&alpha_mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ChooseBuffer {
                        state: Some(state), ..
                    },
                ..
            }) => Some(state.items),
            _ => None,
        })
        .flatten()
        .map(|item| item.text)
        .collect::<Vec<_>>();
    assert!(rows.len() >= 2, "{rows:?}");
    assert!(
        rows.iter().all(|row| row.starts_with(r#"{"V":2,"L":"#)),
        "{rows:?}"
    );
}

#[test]
fn subscriptions_print_v2_layouts_whatever_the_new_layouts_flag_says() {
    let shared = Arc::new(Shared::new(1));
    let (session, window, pane) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("subscribed")
        .expect("subscribed session");
    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("subscriber".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(control, session).expect("attach control");
    take_reliable_messages(&mailbox);
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let run = |context: &mut ExecutionContext, args: &[&str]| {
        shared
            .execute(
                control,
                ClientKind::Control,
                context,
                &CommandInvocation::new("refresh-client", args.iter().copied()),
            )
            .expect("refresh-client");
    };
    let values = |mailbox: &OutboundMailbox| {
        take_reliable_messages(mailbox)
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::SubscriptionChanged { name, value, .. },
                    ..
                }) if name == "lay" => Some(value),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let v2 = r#"{"V":2,"L":{"t":"p","w":80,"h":24,"x":0,"y":0,"a":true,"i":0,"I":"%0"}}"#;
    run(&mut context, &["-B", "lay::#{window_layout}"]);
    shared.refresh_control_subscriptions();
    assert_eq!(values(&mailbox), [v2]);
    run(&mut context, &["-f", "new-layouts"]);
    shared.refresh_control_subscriptions();
    run(&mut context, &["-f", "!new-layouts"]);
    shared.refresh_control_subscriptions();
    assert!(values(&mailbox).is_empty());
}

#[test]
fn pause_after_uses_unsigned_scan_prefix_and_u32_wrap() {
    let mut output = ControlClientOutput::default();

    apply_control_client_flags(&mut output, "pause-after=+3suffix");
    assert_eq!(output.pause_after_ms, Some(3000));

    apply_control_client_flags(&mut output, "pause-after= \t4suffix");
    assert_eq!(output.pause_after_ms, Some(4000));

    apply_control_client_flags(&mut output, "pause-after=-1suffix");
    assert_eq!(output.pause_after_ms, Some(4_294_966_296));

    apply_control_client_flags(&mut output, "pause-after=4294967296suffix");
    assert_eq!(output.pause_after_ms, Some(0));

    apply_control_client_flags(&mut output, "pause-after=4294967295suffix");
    assert_eq!(output.pause_after_ms, Some(4_294_966_296));

    apply_control_client_flags(&mut output, "pause-after=18446744073709551616suffix");
    assert_eq!(output.pause_after_ms, Some(4_294_966_296));

    apply_control_client_flags(&mut output, "pause-after=-18446744073709551616suffix");
    assert_eq!(output.pause_after_ms, Some(4_294_966_296));
}

#[test]
fn refresh_client_c_parses_sizes_bounds_overrides_and_control_only() {
    assert!(matches!(
        parse_control_client_size("100,50"),
        Ok(ControlClientSizeUpdate::Client(TerminalGeometry {
            columns: 100,
            rows: 50,
            cell_width_px: CONTROL_CELL_WIDTH_PX,
            cell_height_px: CONTROL_CELL_HEIGHT_PX,
        }))
    ));
    assert!(matches!(
        parse_control_client_size("100x50"),
        Ok(ControlClientSizeUpdate::Client(TerminalGeometry {
            columns: 100,
            rows: 50,
            ..
        }))
    ));
    for value in ["bad", "100:50", "@1:100,50", "@bad:100x50"] {
        assert!(matches!(
            parse_control_client_size(value),
            Err(ServerError::InvalidCommand(message)) if message == "bad size argument"
        ));
    }
    for value in ["0x50", "50x0", "10001x50", "50x10001"] {
        assert!(matches!(
            parse_control_client_size(value),
            Err(ServerError::InvalidCommand(message))
                if message == "size too small or too big"
        ));
    }

    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "control-size", "size");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .unwrap();
    let control_mailbox = OutboundMailbox::new();
    let interactive_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("sized-control".to_owned()),
        None,
        control_mailbox,
    );
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("interactive".to_owned()),
        None,
        interactive_mailbox,
    );
    shared.attach(control, session).unwrap();
    shared.attach(interactive, session).unwrap();
    assert_eq!(terminal_geometry_owner(&shared.inner.lock(), pane), None);
    assert_eq!(
        popup_client_geometry(&shared.inner.lock(), control).unwrap(),
        None
    );
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));

    let error = shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("refresh-client", ["one", "two"]),
        )
        .expect_err("too many adjustments");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::CommandParse(message))
            if message == "command refresh-client: too many arguments (need at most 1)"
    ));

    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-C", "120x60"]),
        )
        .unwrap();
    assert_eq!(
        control_client_geometry(&shared.inner.lock(), control, window),
        Some(TerminalGeometry {
            columns: 120,
            rows: 60,
            cell_width_px: 8,
            cell_height_px: 18,
        })
    );
    assert_eq!(
        terminal_geometry_owner(&shared.inner.lock(), pane),
        Some(control)
    );

    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-C", "90x40", "-C", "120,60"]),
        )
        .unwrap();
    assert_eq!(
        control_client_geometry(&shared.inner.lock(), control, window)
            .map(|geometry| (geometry.columns, geometry.rows)),
        Some((120, 60))
    );
    let listed = shared
        .list_clients(
            &context,
            "list-clients",
            &[
                "-F".into(),
                "#{client_name}:#{client_width}x#{client_height}".into(),
            ],
        )
        .unwrap();
    assert!(listed.output.contains("sized-control:120x"));

    let override_value = format!("{window}:80x30");
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-C", &override_value]),
        )
        .unwrap();
    assert_eq!(
        control_client_geometry(&shared.inner.lock(), control, window)
            .map(|geometry| (geometry.columns, geometry.rows)),
        Some((80, 30))
    );
    let clear_value = format!("{window}:");
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-C", &clear_value]),
        )
        .unwrap();
    assert_eq!(
        control_client_geometry(&shared.inner.lock(), control, window)
            .map(|geometry| (geometry.columns, geometry.rows)),
        Some((120, 60))
    );
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-C", &clear_value]),
        )
        .unwrap();

    let error = shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-C", "100x50"]),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "not a control client"
    ));
}

#[test]
fn control_resize_notifies_after_the_live_layout_snapshot() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "control-layout", "layout");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .unwrap();
    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("layout-control".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(control, session).unwrap();
    take_reliable_messages(&mailbox);
    shared.set_control_client_size(control, "100,30").unwrap();
    let messages = take_reliable_messages(&mailbox);
    let snapshot_position = messages
        .iter()
        .position(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event { payload: EventPayload::Snapshot(snapshot), .. })
                    if snapshot.sessions.iter().flat_map(|session| &session.windows)
                        .any(|state| state.id == window && state.layout_dump.contains(r#""w":100,"h":30"#))
            )
        })
        .expect("live 100x30 snapshot");
    let notifications = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::HookEvent { name, variables },
                ..
            }) if name == "window-layout-changed"
                && variables.get("hook_window") == Some(&window.to_string()) =>
            {
                Some(index)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(notifications.len(), 1);
    assert!(snapshot_position < notifications[0]);
    shared.set_control_client_size(control, "100,30").unwrap();
    let repeated = take_reliable_messages(&mailbox)
        .into_iter()
        .filter(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::HookEvent { name, variables },
                    ..
                }) if name == "window-layout-changed"
                    && variables.get("hook_window") == Some(&window.to_string())
            )
        })
        .count();
    assert_eq!(repeated, 1);
}

#[test]
fn control_resize_fires_every_sized_window_even_when_no_size_changes() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "resize-all", "all");
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    for command in [
        CommandInvocation::new("new-window", ["-d", "-t", "resize-all"]),
        CommandInvocation::new(
            "set-hook",
            [
                "-g",
                "window-resized",
                "set-option -gF @resized '#{@resized}r'",
            ],
        ),
        CommandInvocation::new(
            "set-hook",
            [
                "-g",
                "window-layout-changed",
                "set-option -gF @layout '#{@layout}l'",
            ],
        ),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &command,
            )
            .unwrap();
    }
    let windows = shared.inner.lock().engine.state.sessions[&session]
        .windows
        .clone();
    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("resize-all".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(control, session).unwrap();
    let layout_notices = |mailbox: &OutboundMailbox| {
        take_reliable_messages(mailbox)
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::HookEvent { name, variables },
                    ..
                }) if name == "window-layout-changed" => variables.get("hook_window").cloned(),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let every_window = windows.iter().map(ToString::to_string).collect::<Vec<_>>();
    layout_notices(&mailbox);
    shared.set_control_client_size(control, "100,30").unwrap();
    assert_eq!(layout_notices(&mailbox), every_window);
    shared.set_control_client_size(control, "100,30").unwrap();
    assert_eq!(layout_notices(&mailbox), every_window);
    shared
        .set_control_client_size(control, &format!("{}:", windows[1]))
        .unwrap();
    assert_eq!(layout_notices(&mailbox), every_window);
    let option = |name: &str| {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("show-options", ["-gv", name]),
            )
            .expect("option readback")
            .output
            .to_string()
            .trim_end()
            .to_owned()
    };
    assert_eq!(option("@resized"), "rrrrrr");
    assert_eq!(option("@layout"), "llllll");
}

#[test]
fn control_resize_of_a_zoomed_window_reports_the_unzoom_step_to_control_only() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "resize-zoom", "zoom");
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    for command in [
        CommandInvocation::new("split-window", ["-d", "-t", &pane.to_string()]),
        CommandInvocation::new("resize-pane", ["-Z", "-t", &pane.to_string()]),
        CommandInvocation::new(
            "set-hook",
            [
                "-g",
                "window-layout-changed",
                "set-option -gF @seen '#{@seen}[#{unzoomed_layout}]'",
            ],
        ),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &command,
            )
            .unwrap();
    }
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .unwrap();
    let tiled = {
        let inner = shared.inner.lock();
        inner.engine.state.windows[&window].layout_string(
            zz_mux::LayoutFormat::V2,
            inner.engine.state.pane_base_index(window),
        )
    };
    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("resize-zoom".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(control, session).unwrap();
    take_reliable_messages(&mailbox);
    shared.set_control_client_size(control, "100,30").unwrap();
    let unzoomed = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::HookEvent { name, variables },
                ..
            }) if name == "window-layout-changed"
                && variables.get("hook_window") == Some(&window.to_string()) =>
            {
                Some(
                    variables
                        .get(zz_protocol::UNZOOMED_LAYOUT_VARIABLE)
                        .cloned(),
                )
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(unzoomed, vec![Some(tiled), None, None]);
    let seen = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("show-options", ["-gv", "@seen"]),
        )
        .expect("hook readback")
        .output
        .to_string();
    assert_eq!(seen.trim_end(), "[][][]");
}

#[test]
fn read_only_control_clients_run_queries_and_refresh_themselves_only() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "ro-control", "ro");
    let control = |name: &str| {
        let (client, _) = shared.register_subscribed(
            ClientKind::Control,
            Some(name.to_owned()),
            None,
            OutboundMailbox::new(),
        );
        shared.attach(client, session).unwrap();
        client
    };
    let viewer = control("viewer");
    let peer = control("peer");
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("watcher".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(interactive, session).unwrap();
    {
        let mut inner = shared.inner.lock();
        inner.client_flags.insert(viewer);
        inner.client_flags.insert(interactive);
    }
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    let pane_target = pane.to_string();
    let mut request_id = 0;
    let mut run = |client: ClientId, kind: ClientKind, name: &str, args: &[&str]| {
        request_id += 1;
        shared.execute_command_request(
            client,
            kind,
            &mut context,
            request_id,
            &CommandInvocation::new(name, args.iter().copied()),
        )
    };
    let read_only = |response: &CommandResponse| {
        matches!(
            response,
            CommandResponse::Error {
                error: ServerError::InvalidCommand(message),
                ..
            } if message == "client is read-only"
        )
    };
    for (name, args) in [
        ("list-windows", &["-F", "#{window_index}"][..]),
        ("display-message", &["-p", "#{client_readonly}"][..]),
        ("show-options", &["-g", "base-index"][..]),
        ("capture-pane", &["-p", "-t", pane_target.as_str()][..]),
        ("has-session", &["-t", "ro-control"][..]),
        ("list-clients", &[][..]),
        ("refresh-client", &["-C", "100,30"][..]),
        (
            "refresh-client",
            &["-t", "viewer", "-B", "sub::#{window_id}"][..],
        ),
        ("refresh-client", &["-f", "!read-only"][..]),
    ] {
        let response = run(viewer, ClientKind::Control, name, args);
        assert!(
            matches!(response, CommandResponse::Success { .. }),
            "{name} {args:?} answered {response:?}"
        );
    }
    for (name, args) in [
        ("new-window", &["-d"][..]),
        ("set-option", &["-g", "@ro", "1"][..]),
        ("send-keys", &["-t", pane_target.as_str(), "x"][..]),
        ("set-buffer", &["-b", "ro", "x"][..]),
        ("capture-pane", &["-b", "ro"][..]),
        ("display-message", &["-I", "-t", pane_target.as_str()][..]),
        ("refresh-client", &["-t", "peer", "-C", "90,30"][..]),
        ("kill-session", &["-t", "ro-control"][..]),
    ] {
        let response = run(viewer, ClientKind::Control, name, args);
        assert!(
            read_only(&response),
            "{name} {args:?} answered {response:?}"
        );
    }
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                ["-s", "command-alias[41]", "romix=copy-mode ; list-windows"],
            ),
        )
        .expect("mixed alias");
    let response = run(viewer, ClientKind::Control, "romix", &[]);
    assert!(!read_only(&response), "mixed alias answered {response:?}");
    let response = run(interactive, ClientKind::Interactive, "list-windows", &[]);
    assert!(
        read_only(&response),
        "interactive list-windows answered {response:?}"
    );
    let inner = shared.inner.lock();
    assert!(inner.client_flags.contains(viewer));
    assert_eq!(inner.engine.state.sessions[&session].windows.len(), 1);
    assert!(inner.paste_buffers.iter().all(|buffer| buffer.name != "ro"));
    assert!(
        inner
            .client(peer)
            .and_then(|c| c.control_output.as_ref())
            .is_none_or(|output| output.geometry.is_none())
    );
}

#[test]
fn respawn_drops_the_output_control_clients_still_had_queued() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "respawn-feed", "feed");
    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("respawn-feed".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(control, session).unwrap();
    let feed = mailbox.ensure_control_feed(&shared.control_wake);
    feed.queue_at(pane, b"OLD-PROCESS-OUTPUT", Instant::now());
    assert!(feed.order(&mailbox, shard_sink::ControlOrder::AfterOutput, |_| true));
    assert_eq!(feed.queued_lines(), 1);
    assert!(
        feed.queued_bytes(pane)
            .windows(18)
            .any(|bytes| bytes == b"OLD-PROCESS-OUTPUT")
    );
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("respawn-pane", ["-k", "-t", &pane.to_string()]),
        )
        .expect("respawn");
    assert!(
        !feed
            .queued_bytes(pane)
            .windows(18)
            .any(|bytes| bytes == b"OLD-PROCESS-OUTPUT")
    );
}

#[test]
fn control_resize_fires_manual_windows_no_client_sizes() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "resize-manual", "manual");
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    for command in [
        CommandInvocation::new("set-option", ["-w", "window-size", "manual"]),
        CommandInvocation::new(
            "set-hook",
            [
                "-g",
                "window-resized",
                "set-option -gF @resized '#{@resized}r'",
            ],
        ),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &command,
            )
            .unwrap();
    }
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .unwrap();
    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("resize-manual".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(control, session).unwrap();
    take_reliable_messages(&mailbox);
    shared
        .set_control_client_size(control, &format!("{window}:"))
        .unwrap();
    let notices = take_reliable_messages(&mailbox)
        .into_iter()
        .filter(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::HookEvent { name, variables },
                    ..
                }) if name == "window-layout-changed"
                    && variables.get("hook_window") == Some(&window.to_string())
            )
        })
        .count();
    assert_eq!(notices, 1);
    let resized = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("show-options", ["-gv", "@resized"]),
        )
        .expect("hook readback")
        .output
        .to_string();
    assert_eq!(resized.trim_end(), "r");
}

#[test]
fn control_switch_publishes_the_new_attachment_before_its_live_layout_notice() {
    let shared = Arc::new(Shared::new(1));
    let (first, first_pane, _) = output_view_session_fixture(&shared, "layout-first", "first");
    let (second, second_pane, _) = output_view_session_fixture(&shared, "layout-second", "second");
    let (first_window, second_window) = {
        let inner = shared.inner.lock();
        (
            inner.engine.state.window_for_pane(first_pane).unwrap(),
            inner.engine.state.window_for_pane(second_pane).unwrap(),
        )
    };
    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("switch-layout".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(control, first).unwrap();
    shared.set_control_client_size(control, "100,30").unwrap();
    take_reliable_messages(&mailbox);
    let mut context = ExecutionContext::new(Some(first), Some(first_window), Some(first_pane));
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("switch-client", ["-t", "layout-second"]),
        )
        .unwrap();
    let messages = take_reliable_messages(&mailbox);
    let attachment = messages.iter().position(|message| matches!(
        message,
        ProtocolMessage::Attached { session, snapshot, .. }
            if *session == second && snapshot.sessions.iter().flat_map(|session| &session.windows)
                .any(|window| window.id == second_window && window.layout_dump.contains(r#""w":100,"h":30"#))
    )).expect("new attachment carries resized layout");
    let session_changed = messages
        .iter()
        .position(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event { payload: EventPayload::HookEvent { name, .. }, .. })
                    if name == "client-session-changed"
            )
        })
        .expect("session change notification");
    let layout_changed = messages.iter().position(|message| matches!(
        message,
        ProtocolMessage::Event(Event { payload: EventPayload::HookEvent { name, variables }, .. })
            if name == "window-layout-changed" && variables.get("hook_window") == Some(&second_window.to_string())
    )).expect("layout change notification");
    assert!(attachment < session_changed && session_changed < layout_changed);
}

#[test]
fn control_size_feeds_aggressive_resize_and_menu_gating() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "control-gate", "size");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .unwrap();
    let control_mailbox = OutboundMailbox::new();
    let interactive_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control-target".to_owned()),
        None,
        Arc::clone(&control_mailbox),
    );
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("menu-source".to_owned()),
        None,
        interactive_mailbox,
    );
    shared.attach(control, session).unwrap();
    shared.attach(interactive, session).unwrap();
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let menu = CommandInvocation::new(
        "display-menu",
        [
            "-c",
            "control-target",
            "0123456789",
            "x",
            "display-message chosen",
        ],
    );
    shared
        .execute(interactive, ClientKind::Interactive, &mut context, &menu)
        .unwrap();
    assert!(shared.read_client(control, |c| c.is_none_or(|c| c.menu.is_none())));

    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-C", "5x2"]),
        )
        .unwrap();
    shared
        .execute(interactive, ClientKind::Interactive, &mut context, &menu)
        .unwrap();
    assert!(shared.read_client(control, |c| c.is_none_or(|c| c.menu.is_none())));

    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-C", "100x50"]),
        )
        .unwrap();
    shared
        .execute(interactive, ClientKind::Interactive, &mut context, &menu)
        .unwrap();
    assert_eq!(
        shared.inner.lock().clients[&control]
            .menu
            .as_ref()
            .unwrap()
            .state
            .client_columns,
        100
    );
    shared
        .inner
        .lock()
        .client_mut(control)
        .and_then(|c| c.menu.take());

    shared
        .input(
            interactive,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 120,
                rows: 40,
                cell_width_px: 7,
                cell_height_px: 16,
            },
        )
        .unwrap();
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-window-option", ["aggressive-resize", "on"]),
        )
        .unwrap();
    let geometry = aggressive_terminal_geometry(&shared.inner.lock(), pane).unwrap();
    assert_eq!((geometry.columns, geometry.rows), (100, 40));
}

#[test]
fn empty_control_subscriptions_still_refresh_the_format_clock() {
    for attached_control in [false, true] {
        let shared = Arc::new(Shared::new(1));
        if attached_control {
            let (session, _, _) = shared
                .inner
                .lock()
                .engine
                .state
                .create_session("clock")
                .unwrap();
            let (control, _) =
                shared.register_subscribed(ClientKind::Control, None, None, OutboundMailbox::new());
            shared.attach(control, session).unwrap();
        }
        {
            let mut inner = shared.inner.lock();
            assert!(
                inner
                    .clients
                    .values()
                    .filter_map(|c| c.control_output.as_ref())
                    .all(|output| output.subscriptions.is_empty())
            );
            inner.engine.set_format_now(1);
        }
        let before = unix_timestamp();
        shared.refresh_control_subscriptions();
        let after = unix_timestamp();
        let context = shared
            .inner
            .lock()
            .engine
            .format_status_context(None, None, None)
            .detach(FormatNeeds::NONE);
        let now = u64::try_from(context.format_now.unwrap()).unwrap();
        assert!((before..=after).contains(&now));
    }
}

#[test]
fn refresh_client_b_parses_replaces_removes_and_emits_changed_scopes() {
    assert!(matches!(
        parse_control_subscription("name:%*:#{pane_title}:tail"),
        ControlSubscriptionUpdate::Set {
            name: "name",
            scope: ControlSubscriptionScope::AllPanes,
            format: "#{pane_title}:tail",
        }
    ));
    assert!(matches!(
        parse_control_subscription("name"),
        ControlSubscriptionUpdate::Remove("name")
    ));
    assert!(matches!(
        parse_control_subscription("name:"),
        ControlSubscriptionUpdate::Remove("name:")
    ));

    let shared = Arc::new(Shared::new(1));
    let (session, first_window, first_pane, second_window, second_pane) = {
        let mut inner = shared.inner.lock();
        let (session, first_window, first_pane) =
            inner.engine.state.create_session("watched").unwrap();
        let (second_window, second_pane) = inner
            .engine
            .state
            .create_window(
                session,
                Some("second".to_owned()),
                zz_mux::PaneKind::Terminal,
            )
            .unwrap();
        (
            session,
            first_window,
            first_pane,
            second_window,
            second_pane,
        )
    };
    let mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("subscriber".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(control, session).unwrap();
    take_reliable_messages(&mailbox);
    let mut context = ExecutionContext::new(Some(session), Some(first_window), Some(first_pane));
    for value in [
        "session::#{session_name}".to_owned(),
        format!("pane:{first_pane}:#{{pane_id}}"),
        "panes:%*:#{pane_id}".to_owned(),
        format!("window:{first_window}:#{{window_name}}"),
        "windows:@*:#{window_name}".to_owned(),
        "fallback:bogus:#{session_name}".to_owned(),
        "flags::#{client_flags}".to_owned(),
        "command::#{command}".to_owned(),
        "active::#{session_active}".to_owned(),
    ] {
        shared
            .execute(
                control,
                ClientKind::Control,
                &mut context,
                &CommandInvocation::new("refresh-client", ["-B", &value]),
            )
            .unwrap();
    }
    shared.refresh_control_subscriptions();
    let initial = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::SubscriptionChanged {
                        name,
                        session,
                        window,
                        window_index,
                        pane,
                        value,
                    },
                ..
            }) => Some((name, session, window, window_index, pane, value)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let first_pane_name = first_pane.to_string();
    assert!(initial.iter().any(|event| {
        event.0 == "session"
            && event.1 == session
            && event.2.is_none()
            && event.3.is_none()
            && event.4.is_none()
            && event.5 == "watched"
    }));
    assert!(initial.iter().any(|event| {
        event.0 == "pane"
            && event.2 == Some(first_window)
            && event.4 == Some(first_pane)
            && event.5 == first_pane_name
    }));
    assert_eq!(initial.iter().filter(|event| event.0 == "panes").count(), 2);
    assert_eq!(
        initial.iter().filter(|event| event.0 == "window").count(),
        1
    );
    assert_eq!(
        initial.iter().filter(|event| event.0 == "windows").count(),
        2
    );
    assert_eq!(
        initial.iter().filter(|event| event.0 == "fallback").count(),
        1
    );
    assert!(
        initial
            .iter()
            .any(|event| event.0 == "flags" && event.5 == "attached,focused,control-mode")
    );
    assert!(
        initial
            .iter()
            .any(|event| event.0 == "command" && event.5.is_empty())
    );
    assert!(
        initial
            .iter()
            .any(|event| event.0 == "active" && event.5 == "1")
    );

    shared.refresh_control_subscriptions();
    assert!(take_reliable_messages(&mailbox).is_empty());
    shared
        .inner
        .lock()
        .engine
        .state
        .rename_session(session, "renamed".to_owned())
        .unwrap();
    shared.refresh_control_subscriptions();
    let changed = take_reliable_messages(&mailbox);
    assert_eq!(changed.len(), 2);
    assert_eq!(
        changed
            .iter()
            .filter(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::SubscriptionChanged { name, value, .. },
                    ..
                }) if (name == "session" || name == "fallback") && value == "renamed"
            ))
            .count(),
        2
    );

    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-B", "session::replacement"]),
        )
        .unwrap();
    shared.refresh_control_subscriptions();
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::SubscriptionChanged { name, value, .. },
                    ..
                }) if name == "session" && value == "replacement"
            ))
    );
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-B", "session"]),
        )
        .unwrap();
    assert!(
        !shared.inner.lock().clients[&control]
            .control_output
            .as_ref()
            .unwrap()
            .subscriptions
            .contains_key("session")
    );

    shared
        .inner
        .lock()
        .engine
        .state
        .kill_window(second_window)
        .unwrap();
    shared.refresh_control_subscriptions();
    let inner = shared.inner.lock();
    for name in ["panes", "windows"] {
        let previous = &inner.clients[&control]
            .control_output
            .as_ref()
            .unwrap()
            .subscriptions[name]
            .previous;
        assert!(previous.keys().all(|target| {
            target.window != Some(second_window) && target.pane != Some(second_pane)
        }));
    }
    assert_eq!(CONTROL_SUBSCRIPTION_INTERVAL, Duration::from_secs(1));
}

#[test]
fn refresh_client_l_stores_the_client_clipboard_reply_while_the_query_is_pending() {
    let shared = Arc::new(Shared::new(1));
    let (session, window, pane) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("clipboard")
        .unwrap();
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("tty".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(client, session).unwrap();
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let queries = || {
        take_reliable_messages(&mailbox)
            .iter()
            .filter(|message| {
                matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::ClipboardQuery,
                        ..
                    })
                )
            })
            .count()
    };
    let buffers = || {
        shared
            .inner
            .lock()
            .paste_buffers
            .iter()
            .map(|buffer| (buffer.name.clone(), buffer.data.clone(), buffer.automatic))
            .collect::<Vec<_>>()
    };
    let reply = |context: &mut ExecutionContext, data: &[u8]| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::ClipboardReply {
                    data: data.to_vec(),
                },
            )
            .unwrap();
    };
    let refresh = |context: &mut ExecutionContext| {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                context,
                &CommandInvocation::new("refresh-client", ["-l"]),
            )
            .unwrap();
    };

    reply(&mut context, b"unasked");
    assert!(buffers().is_empty());
    queries();
    refresh(&mut context);
    refresh(&mut context);
    assert_eq!(queries(), 1);
    reply(&mut context, &[0, 0xff, b'x']);
    reply(&mut context, b"late");
    let stored = buffers();
    assert_eq!(stored.len(), 1, "{stored:?}");
    assert!(stored[0].0.starts_with("buffer"), "{stored:?}");
    assert_eq!(
        (&stored[0].1[..], stored[0].2),
        (&[0, 0xff, b'x'][..], true)
    );
    refresh(&mut context);
    assert_eq!(queries(), 1);
    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("refresh-client", ["-U"]),
        ),
        Err(DaemonError::Server(ServerError::UnsupportedCommand(_)))
    ));
}

#[test]
fn refresh_client_abc_uses_pin_precedence_and_b_c_reject_non_control_targets() {
    let shared = Arc::new(Shared::new(1));
    let (session, window, pane) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("precedence")
        .unwrap();
    let control_mailbox = OutboundMailbox::new();
    let interactive_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        control_mailbox,
    );
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("interactive".to_owned()),
        None,
        interactive_mailbox,
    );
    shared.attach(control, session).unwrap();
    shared.attach(interactive, session).unwrap();
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new(
                "refresh-client",
                [
                    "-C",
                    "100x50",
                    "-B",
                    "ignored::value",
                    "-A",
                    &format!("{pane}:off"),
                ],
            ),
        )
        .unwrap();
    let inner = shared.inner.lock();
    assert_eq!(
        inner.clients[&control]
            .control_output
            .as_ref()
            .unwrap()
            .panes[&pane]
            .mode,
        ControlPaneOutputMode::Off
    );
    assert!(
        inner.clients[&control]
            .control_output
            .as_ref()
            .unwrap()
            .subscriptions
            .is_empty()
    );
    assert!(
        inner.clients[&control]
            .control_output
            .as_ref()
            .unwrap()
            .geometry
            .is_none()
    );
    drop(inner);

    for flag in ['B', 'C'] {
        let value = if flag == 'B' { "name::value" } else { "100x50" };
        let error = shared
            .execute(
                interactive,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("refresh-client", [format!("-{flag}"), value.to_owned()]),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            DaemonError::Server(ServerError::InvalidCommand(message))
                if message == "not a control client"
        ));
    }
}

#[test]
fn control_output_age_pause_kill_and_fair_pacing_are_deterministic() {
    let now = Instant::now();
    assert_eq!(
        control_output_age_action(
            Some(1000),
            now.checked_sub(Duration::from_millis(999)).unwrap(),
            now,
        ),
        ControlOutputAgeAction::Output(999)
    );
    assert_eq!(
        control_output_age_action(
            Some(1000),
            now.checked_sub(Duration::from_secs(1)).unwrap(),
            now,
        ),
        ControlOutputAgeAction::Pause
    );
    assert_eq!(
        control_output_age_action(None, now.checked_sub(CONTROL_MAXIMUM_AGE).unwrap(), now,),
        ControlOutputAgeAction::Kill
    );

    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);
    let flood = PaneId(1);
    let quiet = PaneId(2);
    {
        let mut inner = shared.inner.lock();
        let output = inner
            .client_mut(client)
            .and_then(|c| c.control_output.as_mut())
            .expect("control output");
        output.pause_after_ms = Some(10_000);
    }
    for (pane, bytes) in [(flood, vec![b'x'; 20_000]), (quiet, b"quiet".to_vec())] {
        mailbox.ensure_control_feed(&shared.control_wake).queue_at(
            pane,
            &bytes,
            now.checked_sub(Duration::from_millis(5)).unwrap(),
        );
    }
    shared.pump_control_output_at(now);
    let output = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::PaneOutputAged {
                        pane,
                        age_ms,
                        bytes,
                    },
                ..
            }) => Some((pane, age_ms, bytes)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(output.len() >= 2);
    assert!(output.iter().all(|(_, _, bytes)| bytes.len() <= 2730));
    assert!(output.iter().take(2).any(|(pane, _, _)| *pane == quiet));
    let flood_ages = output
        .iter()
        .filter_map(|(pane, age, _)| (*pane == flood).then_some(*age))
        .collect::<Vec<_>>();
    assert!(flood_ages.windows(2).all(|ages| ages[0] <= ages[1]));

    let paused_mailbox = OutboundMailbox::new();
    let (paused, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&paused_mailbox));
    take_reliable_messages(&paused_mailbox);
    {
        let mut inner = shared.inner.lock();
        let output = inner
            .client_mut(paused)
            .and_then(|c| c.control_output.as_mut())
            .expect("paused output");
        output.pause_after_ms = Some(1000);
    }
    paused_mailbox
        .ensure_control_feed(&shared.control_wake)
        .queue_at(
            flood,
            b"old",
            now.checked_sub(Duration::from_secs(1)).unwrap(),
        );
    shared.pump_control_output_at(now);
    assert!(
        take_reliable_messages(&paused_mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::PaneOutputState { pane, paused: true },
                    ..
                }) if *pane == flood
            ))
    );

    let killed_mailbox = OutboundMailbox::new();
    let _ =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&killed_mailbox));
    killed_mailbox
        .ensure_control_feed(&shared.control_wake)
        .queue_at(
            flood,
            b"ancient",
            now.checked_sub(CONTROL_MAXIMUM_AGE).unwrap(),
        );
    shared.pump_control_output_at(now);
    assert!(
        take_reliable_messages(&killed_mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlExit { reason },
                    ..
                }) if reason == "too far behind"
            ))
    );
}

#[test]
fn detached_refresh_client_accepts_the_pin_grammar_then_rejects_the_unknown_target() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    let error = shared
        .execute(
            ClientId(5),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "refresh",
                [
                    "-cDlLRSU",
                    "-A",
                    "%0:on",
                    "-B",
                    "name:what:format",
                    "-C",
                    "80x24",
                    "-F",
                    "focused",
                    "-f",
                    "focused",
                    "-r",
                    "%0:report",
                    "-t",
                    "client",
                    "+1",
                ],
            ),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "can't find client: client"
    ));
}
