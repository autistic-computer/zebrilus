use super::*;

#[test]
fn resize_window_from_attached_clients_filters_scope_and_freezes_manual_extent() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "attached-sizes", "target");
    let (other_session, _, _) = output_view_session_fixture(&shared, "other-sizes", "other");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .unwrap();
    let other_window = shared
        .inner
        .lock()
        .engine
        .state
        .create_window_at(
            session,
            None,
            Some("other-window".to_owned()),
            PaneKind::Terminal,
            false,
        )
        .unwrap()
        .0;
    let (first, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (second, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (unrelated, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (detached, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (command_client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    let (unrelated_control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, OutboundMailbox::new());
    shared.attach(first, session).unwrap();
    shared.attach(second, session).unwrap();
    shared.attach(unrelated, other_session).unwrap();
    shared.attach(unrelated_control, other_session).unwrap();
    {
        let mut inner = shared.inner.lock();
        inner
            .attachment
            .attached
            .entry(session)
            .or_default()
            .insert(command_client);
        inner.client_entry(first).size.replace((101, 31));
        inner.client_entry(second).size.replace((141, 21));
        inner.client_entry(unrelated).size.replace((181, 51));
        inner.client_entry(detached).size.replace((201, 61));
        inner.client_entry(command_client).size.replace((251, 71));
        inner
            .client_entry(second)
            .focused_window
            .replace(other_window);
    }
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-t", "attached-sizes", "status", "off"]),
        )
        .unwrap();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                ["-t", &window.to_string(), "aggressive-resize", "on"],
            ),
        )
        .unwrap();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-t", "attached-sizes", "default-size", "73x19"],
            ),
        )
        .unwrap();

    let resize = |flag: &str, context: &mut ExecutionContext| {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                context,
                &CommandInvocation::new("resize-window", ["-t", &window.to_string(), flag]),
            )
            .unwrap();
        shared.inner.lock().engine.pane_geometry(pane).unwrap()
    };
    assert_eq!(resize("-A", &mut context), (141, 31));
    assert_eq!(
        shared.inner.lock().engine.window_size(window),
        WindowSize::Manual
    );
    assert_eq!(resize("-a", &mut context), (101, 21));

    shared
        .input(
            first,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientTerminalSize {
                columns: 151,
                rows: 41,
            },
        )
        .unwrap();
    assert_eq!(resize("-A", &mut context), (151, 41));
    shared
        .input(
            second,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 300,
                rows: 100,
                cell_width_px: 8,
                cell_height_px: 16,
            },
        )
        .unwrap();
    assert_eq!(
        shared.inner.lock().engine.pane_geometry(pane),
        Some((151, 41))
    );

    {
        let mut inner = shared.inner.lock();
        inner.client_entry(first).size.replace((101, 31));
        inner.client_entry(second).size.replace((141, 21));
        inner
            .attachment
            .client_flags
            .apply(unrelated, "ignore-size");
    }
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "refresh-client",
                ["-t", &second.0.to_string(), "-f", "ignore-size"],
            ),
        )
        .unwrap();
    assert!(client_ignores_size(&shared.inner.lock(), second));
    assert_eq!(resize("-A", &mut context), (101, 31));
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "refresh-client",
                ["-t", &second.0.to_string(), "-f", "!ignore-size"],
            ),
        )
        .unwrap();
    shared
        .inner
        .lock()
        .attachment
        .client_flags
        .apply(first, "ignore-size");
    assert_eq!(resize("-A", &mut context), (141, 21));
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "refresh-client",
                ["-t", &second.0.to_string(), "-f", "ignore-size"],
            ),
        )
        .unwrap();
    assert_eq!(resize("-A", &mut context), (73, 19));
    shared
        .inner
        .lock()
        .attachment
        .client_flags
        .apply(unrelated_control, "ignore-size");
    assert_eq!(resize("-A", &mut context), (141, 31));
    assert_eq!(resize("-a", &mut context), (101, 21));
}

#[test]
fn attached_client_extents_clamp_retained_and_default_dimensions() {
    let shared = Arc::new(Shared::new(1));
    let (session, _, pane) = switch_test_session(&shared, "extent-clamp");
    let (other_session, _, _) = switch_test_session(&shared, "extent-clamp-other");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .unwrap();
    let (first, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (second, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (projected, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (suppressor, _) =
        shared.register_subscribed(ClientKind::Control, None, None, OutboundMailbox::new());
    for client in [first, second, projected] {
        shared.attach(client, session).unwrap();
    }
    shared.attach(suppressor, other_session).unwrap();
    {
        let mut inner = shared.inner.lock();
        inner.client_entry(first).size.replace((u16::MAX, 12_001));
        inner.client_entry(second).size.replace((12_002, u16::MAX));
        inner
            .panes
            .terminal_geometries
            .entry(pane)
            .or_default()
            .insert(
                projected,
                TerminalGeometry {
                    columns: u16::MAX,
                    rows: u16::MAX,
                    cell_width_px: 8,
                    cell_height_px: 16,
                },
            );
    }
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "remain-on-exit", "on"]),
        )
        .unwrap();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-t", "extent-clamp", "status", "off"]),
        )
        .unwrap();
    let resize = |flag: &str, context: &mut ExecutionContext| {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                context,
                &CommandInvocation::new("resize-window", ["-t", &window.to_string(), flag]),
            )
            .unwrap();
        shared.inner.lock().engine.pane_geometry(pane).unwrap()
    };
    assert_eq!(resize("-A", &mut context), (10_000, 10_000));
    assert_eq!(resize("-a", &mut context), (10_000, 10_000));
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-t", "extent-clamp", "default-size", "65000x64000"],
            ),
        )
        .unwrap();
    {
        let mut inner = shared.inner.lock();
        for client in [first, second, projected] {
            inner.attachment.client_flags.apply(client, "ignore-size");
        }
    }
    assert_eq!(resize("-A", &mut context), (10_000, 10_000));
    assert_eq!(resize("-a", &mut context), (10_000, 10_000));
}

#[test]
fn attached_client_extents_subtract_status_project_gui_and_clamp_control() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "extent-sources", "target");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .unwrap();
    let (outer, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (projected, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, OutboundMailbox::new());
    shared.attach(outer, session).unwrap();
    shared.attach(projected, session).unwrap();
    shared.attach(control, session).unwrap();
    {
        let mut inner = shared.inner.lock();
        inner.client_entry(outer).size.replace((200, 50));
        inner
            .panes
            .terminal_geometries
            .entry(pane)
            .or_default()
            .insert(
                projected,
                TerminalGeometry {
                    columns: 180,
                    rows: 60,
                    cell_width_px: 8,
                    cell_height_px: 16,
                },
            );
    }

    {
        let inner = shared.inner.lock();
        assert_eq!(
            interactive_client_window_extent(&inner, outer, session, window),
            Some((200, 49))
        );
        assert_eq!(
            interactive_client_window_extent(&inner, projected, session, window),
            Some((180, 60))
        );
        assert_eq!(
            attached_client_window_extent(&inner, window, WindowSize::Largest),
            Some((200, 60))
        );
    }

    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-t", "extent-sources", "status", "3"]),
        )
        .unwrap();
    assert_eq!(
        interactive_client_window_extent(&shared.inner.lock(), outer, session, window),
        Some((200, 47))
    );
    shared.set_control_client_size(control, "210x40").unwrap();
    assert_eq!(
        attached_client_window_extent(&shared.inner.lock(), window, WindowSize::Largest),
        Some((210, 60))
    );
    assert_eq!(
        attached_client_window_extent(&shared.inner.lock(), window, WindowSize::Smallest),
        Some((180, 40))
    );

    shared
        .set_control_client_size(control, &format!("{window}:190x55"))
        .unwrap();
    assert_eq!(
        attached_client_window_extent(&shared.inner.lock(), window, WindowSize::Largest),
        Some((190, 55))
    );
    assert_eq!(
        terminal_geometry_for_mode(&shared.inner.lock(), pane, false, WindowSize::Largest)
            .map(|geometry| (geometry.columns, geometry.rows)),
        Some((190, 55))
    );
    shared
        .set_control_client_size(control, &format!("{window}:"))
        .unwrap();
    assert_eq!(
        attached_client_window_extent(&shared.inner.lock(), window, WindowSize::Largest),
        Some((210, 60))
    );

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-t", "extent-sources", "status", "off"]),
        )
        .unwrap();
    assert_eq!(
        interactive_client_window_extent(&shared.inner.lock(), outer, session, window),
        Some((200, 50))
    );
}

#[test]
fn a_client_no_taller_than_its_status_block_keeps_every_row() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "status-floor", "target");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .expect("pane window");
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    shared.attach(client, session).expect("attach");
    shared
        .inner
        .lock()
        .client_entry(client)
        .size
        .replace((80, 3));

    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let extent = || interactive_client_window_extent(&shared.inner.lock(), client, session, window);
    let set = |value: &str, context: &mut ExecutionContext| {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                context,
                &CommandInvocation::new("set-option", ["-t", "status-floor", "status", value]),
            )
            .expect("set status");
    };

    assert_eq!(extent(), Some((80, 2)));
    set("2", &mut context);
    assert_eq!(extent(), Some((80, 1)));
    set("3", &mut context);
    assert_eq!(extent(), Some((80, 3)));
    set("4", &mut context);
    assert_eq!(extent(), Some((80, 3)));
    set("off", &mut context);
    assert_eq!(extent(), Some((80, 3)));
}

#[test]
fn status_rows_resize_every_window_of_an_interactive_clients_session() {
    let shared = Arc::new(Shared::new(1));
    let (session, window, first) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("status-rows")
        .expect("create session");
    let second = shared
        .inner
        .lock()
        .engine
        .state
        .split_pane(first, zz_protocol::Axis::Vertical, PaneKind::Terminal)
        .expect("split the window");
    let client = ClientId(1);
    {
        let mut inner = shared.inner.lock();
        inner
            .client_entry(client)
            .kind
            .replace(ClientKind::Interactive);
        inner
            .attachment
            .attached
            .insert(session, BTreeSet::from([client]));
        inner.client_entry(client).size.replace((80, 10));
        assert!(inner.engine.resize_window_to_extent(window, 80, 9));
        assert_eq!(inner.engine.state.windows[&window].active_pane, second);
        assert_eq!(inner.engine.pane_geometry(first), Some((80, 4)));
        assert_eq!(inner.engine.pane_geometry(second), Some((80, 4)));
    }

    let mut context = ExecutionContext::new(Some(session), Some(window), Some(second));
    let set = |option: &str, value: &str, context: &mut ExecutionContext| {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                context,
                &CommandInvocation::new("set-option", ["-t", "status-rows", option, value]),
            )
            .expect("set the status option");
    };

    set("status", "2", &mut context);
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner
                .engine
                .window_extent(window, zz_protocol::Axis::Vertical),
            Some(8)
        );
        assert_eq!(inner.engine.pane_geometry(first), Some((80, 3)));
        assert_eq!(inner.engine.pane_geometry(second), Some((80, 4)));
    }

    set("status-position", "top", &mut context);
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.pane_geometry(first), Some((80, 3)));
        assert_eq!(inner.engine.pane_geometry(second), Some((80, 4)));
    }

    set("status-position", "bottom", &mut context);
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.pane_geometry(first), Some((80, 3)));
        assert_eq!(inner.engine.pane_geometry(second), Some((80, 4)));
    }

    set("status", "on", &mut context);
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner
                .engine
                .window_extent(window, zz_protocol::Axis::Vertical),
            Some(9)
        );
        assert_eq!(inner.engine.pane_geometry(first), Some((80, 4)));
        assert_eq!(inner.engine.pane_geometry(second), Some((80, 4)));
    }
}

#[test]
fn a_floored_client_measurement_sizes_the_pty_from_the_pane_it_reports() {
    let shared = Arc::new(Shared::new(1));
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "residue", QUIET_PANE_COMMAND]),
        )
        .expect("create session");
    let session = context.session.expect("session id");
    let pane = context.pane.expect("pane id");
    shared.attach(client, session).expect("attach");
    let terminal = shared.inner.lock().terminals[&pane].clone();
    let view = TerminalViewId(client.0);

    let report = |pane: PaneId, columns: u16, rows: u16, context: &mut ExecutionContext| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::ResizeTerminal {
                    pane,
                    columns,
                    rows,
                    cell_width_px: 8,
                    cell_height_px: 16,
                },
            )
            .expect("client measurement");
    };
    let family = |context: &mut ExecutionContext| {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                context,
                &CommandInvocation::new(
                    "display-message",
                    [
                        "-p",
                        "#{pane_width}x#{pane_height} #{pane_left},#{pane_top} \
                             #{pane_right},#{pane_bottom} \
                             #{pane_at_left}#{pane_at_top}#{pane_at_right}#{pane_at_bottom} \
                             #{window_width}x#{window_height}",
                    ],
                ),
            )
            .expect("display-message")
            .output
    };

    report(pane, 120, 24, &mut context);
    wait_for_terminal_dimensions(&terminal, view, 120, 24);
    report(pane, 119, 23, &mut context);
    wait_for_terminal_dimensions(&terminal, view, 119, 23);
    assert_eq!(
        shared.inner.lock().engine.pane_geometry(pane),
        Some((119, 23))
    );
    assert_eq!(family(&mut context), "119x23 0,0 118,22 1111 119x23");

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-h", "-l", "10", QUIET_PANE_COMMAND]),
        )
        .expect("split the window");
    let narrow = context.pane.expect("narrow pane id");
    let narrow_terminal = shared.inner.lock().terminals[&narrow].clone();
    assert_eq!(
        terminal_resize_for_pane(&shared.inner.lock(), pane)
            .map(|(_, geometry)| (geometry.columns, geometry.rows)),
        Some((108, 23)),
        "the split moved the left cell, so its stale 119-column report is not the pty size"
    );

    report(narrow, 9, 23, &mut context);
    assert_eq!(
        shared.inner.lock().engine.pane_geometry(narrow),
        Some((10, 23))
    );
    assert_eq!(
        terminal_resize_for_pane(&shared.inner.lock(), narrow)
            .map(|(_, geometry)| (geometry.columns, geometry.rows)),
        Some((10, 23))
    );
    wait_for_terminal_dimensions(&narrow_terminal, view, 10, 23);
    assert_eq!(family(&mut context), "10x23 109,0 118,22 0111 119x23");
    let layout = shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-p", "#{window_layout}"]),
        )
        .expect("display-message")
        .output;
    assert!(
        layout.contains(
            r#"{"t":"h","w":119,"h":23,"x":0,"y":0,"c":[{"t":"p","w":108,"h":23,"x":0,"y":0,"#
        ) && layout.contains(r#"{"t":"p","w":10,"h":23,"x":109,"y":0,"#),
        "{layout}"
    );
}

#[test]
fn a_detached_split_keeps_the_window_its_client_reported() {
    for (axis, first, second) in [("-h", "40x23", "39x23"), ("-v", "80x11", "80x11")] {
        let shared = Arc::new(Shared::new(1));
        let (client, _) =
            shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
        let mut context = ExecutionContext::default();
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new(
                    "new-session",
                    [
                        "-d",
                        "-s",
                        "detached-split",
                        "-x",
                        "80",
                        "-y",
                        "23",
                        QUIET_PANE_COMMAND,
                    ],
                ),
            )
            .expect("create session");
        let session = context.session.expect("session id");
        let pane = context.pane.expect("pane id");
        shared.attach(client, session).expect("attach");
        assert_eq!(
            shared.inner.lock().engine.pane_geometry(pane),
            Some((80, 23))
        );
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::ResizeTerminal {
                    pane,
                    columns: 80,
                    rows: 23,
                    cell_width_px: 8,
                    cell_height_px: 16,
                },
            )
            .expect("client measurement");
        assert_eq!(
            shared.inner.lock().engine.pane_geometry(pane),
            Some((80, 23))
        );
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("split-window", ["-d", axis, QUIET_PANE_COMMAND]),
            )
            .expect("split the window");
        let panes = shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new(
                    "list-panes",
                    [
                        "-F",
                        "#{pane_width}x#{pane_height} #{window_width}x#{window_height}",
                    ],
                ),
            )
            .expect("list-panes")
            .output;
        assert_eq!(
            panes,
            format!("{first} 80x23\n{second} 80x23"),
            "split-window -d {axis} keeps the window at the reported 80x23"
        );

        let run = |context: &mut ExecutionContext, args: &[&str]| {
            shared
                .execute(
                    client,
                    ClientKind::Interactive,
                    context,
                    &CommandInvocation::new(args[0], args[1..].iter().copied()),
                )
                .expect("command")
                .output
        };
        run(&mut context, &["new-window", QUIET_PANE_COMMAND]);
        let hidden = context.pane.expect("new window pane");
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::ResizeTerminal {
                    pane: hidden,
                    columns: 80,
                    rows: 23,
                    cell_width_px: 8,
                    cell_height_px: 16,
                },
            )
            .expect("client measurement");
        run(&mut context, &["select-window", "-t", ":0"]);
        run(
            &mut context,
            &["split-window", "-d", axis, "-t", ":1", QUIET_PANE_COMMAND],
        );
        run(&mut context, &["select-window", "-t", ":1"]);
        assert_eq!(
            run(
                &mut context,
                &[
                    "list-panes",
                    "-t",
                    ":1",
                    "-F",
                    "#{pane_width}x#{pane_height} #{window_width}x#{window_height}",
                ],
            ),
            format!("{first} 80x23\n{second} 80x23"),
            "split-window -d {axis} in an unseen window keeps it at 80x23 once selected"
        );
    }
}

#[test]
fn a_stale_report_leaves_the_latest_control_size_to_write_back() {
    let shared = Arc::new(Shared::new(1));
    let run =
        |client: ClientId, kind: ClientKind, context: &mut ExecutionContext, args: &[&str]| {
            shared
                .execute(
                    client,
                    kind,
                    context,
                    &CommandInvocation::new(args[0], args[1..].iter().copied()),
                )
                .expect("command")
                .output
        };
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let mut context = ExecutionContext::default();
    run(
        client,
        ClientKind::Interactive,
        &mut context,
        &[
            "new-session",
            "-d",
            "-s",
            "stale-control",
            "-x",
            "80",
            "-y",
            "23",
            QUIET_PANE_COMMAND,
        ],
    );
    let session = context.session.expect("session id");
    shared.attach(client, session).expect("attach");
    run(
        client,
        ClientKind::Interactive,
        &mut context,
        &["new-window", QUIET_PANE_COMMAND],
    );
    let hidden = context.pane.expect("new window pane");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane: hidden,
                columns: 80,
                rows: 23,
                cell_width_px: 8,
                cell_height_px: 16,
            },
        )
        .expect("client measurement");
    run(
        client,
        ClientKind::Interactive,
        &mut context,
        &["select-window", "-t", ":0"],
    );
    run(
        client,
        ClientKind::Interactive,
        &mut context,
        &["split-window", "-d", "-h", "-t", ":1", QUIET_PANE_COMMAND],
    );
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, OutboundMailbox::new());
    shared.attach(control, session).expect("attach control");
    let mut control_context = ExecutionContext::default();
    run(
        control,
        ClientKind::Control,
        &mut control_context,
        &["refresh-client", "-C", "100x24"],
    );
    assert_eq!(
        run(
            client,
            ClientKind::Interactive,
            &mut context,
            &[
                "list-panes",
                "-t",
                ":1",
                "-F",
                "#{pane_width}x#{pane_height} #{window_width}x#{window_height}",
            ],
        ),
        "50x24 100x24\n49x24 100x24",
        "the latest control client sizes the window past a stale interactive report"
    );
}

#[test]
fn one_client_fresh_report_leaves_another_client_report_stale() {
    let shared = Arc::new(Shared::new(1));
    let run = |client: ClientId, context: &mut ExecutionContext, args: &[&str]| {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                context,
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .expect("command")
            .output
    };
    let report = |client: ClientId, context: &mut ExecutionContext, pane, columns| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::ResizeTerminal {
                    pane,
                    columns,
                    rows: 23,
                    cell_width_px: 8,
                    cell_height_px: 16,
                },
            )
            .expect("client measurement");
    };
    let register = || {
        shared
            .register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new())
            .0
    };
    let (first, second) = (register(), register());
    let mut context = ExecutionContext::default();
    run(
        first,
        &mut context,
        &[
            "new-session",
            "-d",
            "-s",
            "two-reports",
            "-x",
            "80",
            "-y",
            "23",
            QUIET_PANE_COMMAND,
        ],
    );
    let session = context.session.expect("session id");
    let pane = context.pane.expect("pane id");
    shared.attach(second, session).expect("attach second");
    shared.attach(first, session).expect("attach first");
    report(first, &mut context, pane, 80);
    report(second, &mut context, pane, 80);
    run(
        first,
        &mut context,
        &["split-window", "-d", "-h", QUIET_PANE_COMMAND],
    );
    report(second, &mut context, pane, 40);
    assert_eq!(
        run(
            first,
            &mut context,
            &[
                "list-panes",
                "-F",
                "#{pane_width}x#{pane_height} #{window_width}x#{window_height}",
            ],
        ),
        "40x23 80x23\n39x23 80x23",
        "the second client's fresh report does not revive the latest client's stale one"
    );
}

#[test]
fn a_stale_report_still_bounds_a_smallest_window_by_its_client_size() {
    let shared = Arc::new(Shared::new(1));
    let run =
        |client: ClientId, kind: ClientKind, context: &mut ExecutionContext, args: &[&str]| {
            shared
                .execute(
                    client,
                    kind,
                    context,
                    &CommandInvocation::new(args[0], args[1..].iter().copied()),
                )
                .expect("command")
                .output
        };
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let mut context = ExecutionContext::default();
    run(
        client,
        ClientKind::Interactive,
        &mut context,
        &[
            "new-session",
            "-d",
            "-s",
            "stale-smallest",
            "-x",
            "80",
            "-y",
            "23",
            QUIET_PANE_COMMAND,
        ],
    );
    let session = context.session.expect("session id");
    let pane = context.pane.expect("pane id");
    shared.attach(client, session).expect("attach");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 80,
                rows: 23,
                cell_width_px: 8,
                cell_height_px: 16,
            },
        )
        .expect("client measurement");
    run(
        client,
        ClientKind::Interactive,
        &mut context,
        &["set-window-option", "-g", "window-size", "smallest"],
    );
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, OutboundMailbox::new());
    shared.attach(control, session).expect("attach control");
    run(
        client,
        ClientKind::Interactive,
        &mut context,
        &["split-window", "-d", "-h", QUIET_PANE_COMMAND],
    );
    let mut control_context = ExecutionContext::default();
    let mut window_size = |size: &str| {
        run(
            control,
            ClientKind::Control,
            &mut control_context,
            &["refresh-client", "-C", size],
        );
        run(
            client,
            ClientKind::Interactive,
            &mut context,
            &["display-message", "-p", "#{window_width}x#{window_height}"],
        )
    };
    assert_eq!(window_size("60x24"), "60x23");
    assert_eq!(
        window_size("100x24"),
        "80x23",
        "the stale 80-column client bounds a smallest window again once control grows"
    );
}

#[test]
fn automatic_window_modes_share_the_global_ignore_size_fallback() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) = output_view_session_fixture(&shared, "dynamic-ignore", "target");
    let (other_session, _, _) =
        output_view_session_fixture(&shared, "dynamic-ignore-other", "other");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .unwrap();
    let (first, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (second, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, OutboundMailbox::new());
    shared.attach(first, session).unwrap();
    shared.attach(second, session).unwrap();
    shared.attach(control, other_session).unwrap();
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    for (client, columns, rows) in [(first, 200, 30), (second, 100, 60)] {
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
                    cell_height_px: 16,
                },
            )
            .unwrap();
    }
    shared
        .inner
        .lock()
        .attachment
        .client_flags
        .apply(first, "ignore-size");
    {
        let inner = shared.inner.lock();
        for mode in [
            WindowSize::Latest,
            WindowSize::Largest,
            WindowSize::Smallest,
        ] {
            assert_eq!(
                terminal_geometry_for_mode(&inner, pane, false, mode)
                    .map(|geometry| (geometry.columns, geometry.rows)),
                Some((100, 60))
            );
        }
    }
    shared
        .inner
        .lock()
        .attachment
        .client_flags
        .apply(second, "ignore-size");
    assert_eq!(
        terminal_geometry_for_mode(&shared.inner.lock(), pane, false, WindowSize::Largest),
        None
    );
    shared
        .inner
        .lock()
        .attachment
        .client_flags
        .apply(control, "ignore-size");
    let inner = shared.inner.lock();
    assert_eq!(
        terminal_geometry_for_mode(&inner, pane, false, WindowSize::Largest)
            .map(|geometry| (geometry.columns, geometry.rows)),
        Some((200, 60))
    );
    assert_eq!(
        terminal_geometry_for_mode(&inner, pane, false, WindowSize::Smallest)
            .map(|geometry| (geometry.columns, geometry.rows)),
        Some((100, 30))
    );
    assert_eq!(
        terminal_geometry_for_mode(&inner, pane, false, WindowSize::Latest)
            .map(|geometry| (geometry.columns, geometry.rows)),
        Some((100, 60))
    );
}

#[test]
fn window_size_and_aggressive_resize_compose_for_each_viewed_window() {
    let shared = Arc::new(Shared::new(1));
    let (first_client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (second_client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (session, first_pane, _) =
        output_view_session_fixture(&shared, "aggressive", "first window");
    let (first_window, second_window, second_pane) = {
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
        let terminal = Arc::new(TerminalSession::spawn_output_view(
            "second window".to_owned(),
            String::new(),
        ));
        inner.terminals_mut().insert(second_pane, terminal);
        (first_window, second_window, second_pane)
    };
    shared
        .attach(first_client, session)
        .expect("attach first client");
    shared
        .attach(second_client, session)
        .expect("attach second client");

    let mut first_context =
        ExecutionContext::new(Some(session), Some(first_window), Some(first_pane));
    let mut second_context = first_context.clone();
    let resize = |client, context: &mut ExecutionContext, pane, columns, rows| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::ResizeTerminal {
                    pane,
                    columns,
                    rows,
                    cell_width_px: 8,
                    cell_height_px: 16,
                },
            )
            .expect("record terminal geometry");
    };
    resize(first_client, &mut first_context, first_pane, 200, 30);
    resize(second_client, &mut second_context, first_pane, 100, 60);
    assert_eq!(
        shared.inner.lock().engine.pane_geometry(first_pane),
        Some((100, 60))
    );
    shared
        .attach(first_client, session)
        .expect("reattach latest client");
    assert_eq!(
        shared.inner.lock().engine.pane_geometry(first_pane),
        Some((200, 30))
    );
    {
        let inner = shared.inner.lock();
        let geometry = |mode| {
            let geometry = terminal_geometry_for_mode(&inner, first_pane, false, mode)
                .expect("window-size geometry");
            (geometry.columns, geometry.rows)
        };
        assert_eq!(geometry(WindowSize::Latest), (200, 30));
        assert_eq!(geometry(WindowSize::Manual), (200, 30));
        assert_eq!(geometry(WindowSize::Largest), (200, 60));
        assert_eq!(geometry(WindowSize::Smallest), (100, 30));
        assert_eq!(
            terminal_geometry_for_mode(&inner, first_pane, true, WindowSize::Largest)
                .map(|geometry| (geometry.columns, geometry.rows)),
            Some((200, 60))
        );
    }

    let manual_effect = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut first_context,
            &CommandInvocation::new(
                "resize-window",
                ["-t", &first_window.to_string(), "-x", "140", "-y", "42"],
            ),
        )
        .expect("select manual window size");
    assert_eq!(
        manual_effect.effects,
        vec![
            MuxEffect::WindowSizeChanged {
                window: Some(first_window)
            },
            MuxEffect::SnapshotChanged,
        ]
    );
    assert_eq!(
        shared.inner.lock().engine.pane_geometry(first_pane),
        Some((140, 42))
    );
    resize(first_client, &mut first_context, first_pane, 200, 30);
    assert_eq!(
        shared.inner.lock().engine.pane_geometry(first_pane),
        Some((140, 42))
    );
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut first_context,
            &CommandInvocation::new(
                "set-window-option",
                ["-u", "-t", &first_window.to_string(), "window-size"],
            ),
        )
        .expect("restore inherited window size");
    assert_eq!(
        shared.inner.lock().engine.pane_geometry(first_pane),
        Some((200, 30))
    );

    let window_size_effect = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut first_context,
            &CommandInvocation::new("set-window-option", ["-g", "window-size", "smallest"]),
        )
        .expect("select smallest window size");
    assert_eq!(
        window_size_effect.effects,
        vec![MuxEffect::WindowSizeChanged { window: None }]
    );
    assert_eq!(
        shared.inner.lock().engine.pane_geometry(first_pane),
        Some((100, 30))
    );

    let effect = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut first_context,
            &CommandInvocation::new("set-window-option", ["-g", "aggressive-resize", "on"]),
        )
        .expect("enable aggressive resize");
    assert_eq!(
        effect.effects,
        vec![
            MuxEffect::AggressiveResizeChanged { window: None },
            MuxEffect::SnapshotChanged,
        ]
    );
    assert_eq!(
        shared.inner.lock().engine.pane_geometry(first_pane),
        Some((100, 30))
    );
    assert_eq!(
        terminal_resize_for_pane(&shared.inner.lock(), first_pane)
            .expect("aggressive geometry")
            .1
            .columns,
        100
    );
    assert_eq!(
        terminal_resize_for_pane(&shared.inner.lock(), first_pane)
            .expect("aggressive geometry")
            .1
            .rows,
        30
    );

    shared
        .execute(
            first_client,
            ClientKind::Interactive,
            &mut first_context,
            &CommandInvocation::new("select-window", ["-t", &second_window.to_string()]),
        )
        .expect("first client views the second window");
    resize(first_client, &mut first_context, second_pane, 160, 50);
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.pane_geometry(first_pane), Some((100, 60)));
        assert_eq!(inner.engine.pane_geometry(second_pane), Some((160, 50)));
    }

    let mut command_context = first_context.clone();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("select-window", ["-t", &first_window.to_string()]),
        )
        .expect("move every viewer to the first window");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("select-window", ["-t", &second_window.to_string()]),
        )
        .expect("move every viewer to the second window");
    resize(second_client, &mut second_context, second_pane, 90, 20);
    assert_eq!(
        shared.inner.lock().engine.pane_geometry(second_pane),
        Some((90, 20))
    );

    shared
        .execute(
            first_client,
            ClientKind::Interactive,
            &mut first_context,
            &CommandInvocation::new("select-window", ["-t", &first_window.to_string()]),
        )
        .expect("park the larger client on the first window");
    let inner = shared.inner.lock();
    assert_eq!(
        client_focused_window_for_attachment(&inner, first_client),
        Some(first_window)
    );
    assert_eq!(
        client_focused_window_for_attachment(&inner, second_client),
        Some(second_window)
    );
    assert_eq!(inner.engine.pane_geometry(first_pane), Some((200, 30)));
    assert_eq!(inner.engine.pane_geometry(second_pane), Some((90, 20)));
}

#[test]
fn attach_session_target_publishes_requested_focus_and_preserves_other_viewers() {
    for (target_pane_included, same_session) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let shared = Arc::new(Shared::new(1));
        let (a, _, _) = switch_test_session(&shared, "A");
        let (b, target_window, first_pane) = switch_test_session(&shared, "B");
        let (last_window, target_pane) = {
            let mut inner = shared.inner.lock();
            let pane = inner
                .engine
                .state
                .split_pane(
                    first_pane,
                    zz_protocol::Axis::Horizontal,
                    PaneKind::Terminal,
                )
                .expect("split target window");
            inner
                .engine
                .state
                .select_pane(first_pane)
                .expect("restore first pane");
            let (window, _) = inner
                .engine
                .state
                .create_window_at(b, Some(1), None, PaneKind::Terminal, true)
                .expect("create last focused window");
            (window, pane)
        };
        let mailbox = OutboundMailbox::new();
        let (client, _) =
            shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
        let (other, _) =
            shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
        shared
            .attach(client, if same_session { b } else { a })
            .expect("attach to original session");
        shared
            .attach(other, b)
            .expect("attach other viewer to destination");
        if same_session {
            shared
                .execute(
                    other,
                    ClientKind::Interactive,
                    &mut ExecutionContext::default(),
                    &CommandInvocation::new("select-window", ["-t", &target_window.to_string()]),
                )
                .expect("other viewer selects target while client keeps last window");
        }
        take_reliable_messages(&mailbox);
        let target = if target_pane_included {
            format!("{b}:{target_window}.{target_pane}")
        } else {
            format!("{b}:{target_window}")
        };
        let expected_pane = if target_pane_included {
            target_pane
        } else {
            first_pane
        };
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("attach-session", ["-t", &target]),
            )
            .expect("attach directly to clicked row");

        let messages = take_reliable_messages(&mailbox);
        let attached = messages.iter().position(|message| {
            matches!(message, ProtocolMessage::Attached { session, .. } if *session == b)
        }).expect("targeted attachment is published");
        let snapshots = messages[attached..]
            .iter()
            .filter_map(|message| match message {
                ProtocolMessage::Attached { snapshot, .. }
                | ProtocolMessage::Event(Event {
                    payload: EventPayload::Snapshot(snapshot),
                    ..
                }) => Some(snapshot),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!snapshots.is_empty());
        for snapshot in snapshots {
            assert_eq!(snapshot.focused_window, Some(target_window));
            let window = snapshot
                .sessions
                .iter()
                .find(|session| session.id == b)
                .expect("destination session is present")
                .windows
                .iter()
                .find(|window| window.id == target_window)
                .expect("target window is present");
            assert_eq!(window.active_pane, expected_pane);
        }
        assert_eq!(
            client_focused_window_for_attachment(&shared.inner.lock(), other),
            Some(if same_session {
                target_window
            } else {
                last_window
            })
        );
    }
}

#[test]
fn select_window_updates_client_focus_when_session_already_selected_it() {
    let shared = Arc::new(Shared::new(1));
    let (session, first_window, _) = switch_test_session(&shared, "focus");
    let second_window = shared
        .inner
        .lock()
        .engine
        .state
        .create_window_at(session, Some(1), None, PaneKind::Terminal, false)
        .expect("create second window")
        .0;
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (other, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    shared.attach(client, session).expect("attach client");
    shared.attach(other, session).expect("attach other viewer");
    let (mover, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    shared.attach(mover, session).expect("attach moving viewer");
    shared
        .execute(
            mover,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("select-window", ["-t", &second_window.to_string()]),
        )
        .expect("another viewer selects the second window");
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("select-window", ["-t", &second_window.to_string()]),
        )
        .expect("select the window already selected by the session");

    let inner = shared.inner.lock();
    assert_eq!(
        client_focused_window_for_attachment(&inner, client),
        Some(second_window)
    );
    assert_eq!(
        client_focused_window_for_attachment(&inner, other),
        Some(first_window)
    );
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Snapshot(snapshot),
                    ..
                }) if snapshot.focused_window == Some(second_window)
            ))
    );
}

#[test]
fn multi_client_window_focus_isolated_stamped_and_falls_back_after_kill() {
    let shared = Arc::new(Shared::new(1));
    let first_mailbox = OutboundMailbox::new();
    let second_mailbox = OutboundMailbox::new();
    let observer_mailbox = OutboundMailbox::new();
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
    let (observer, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&observer_mailbox),
    );

    let mut create_context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut create_context,
            &CommandInvocation::new("new-session", ["-d", "-s", "shared-focus", "-n", "agent"]),
        )
        .expect("create shared session");
    let session = create_context.session.expect("shared session");
    let first_window = create_context.window.expect("agent window");
    let first_pane = create_context.pane.expect("agent terminal");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut create_context,
            &CommandInvocation::new("new-window", ["-n", "logs"]),
        )
        .expect("create logs window");
    let second_window = create_context.window.expect("logs window");
    let second_pane = create_context.pane.expect("logs terminal");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut create_context,
            &CommandInvocation::new("select-window", ["-t", &first_window.to_string()]),
        )
        .expect("restore agent as the session default");

    let first_attached = shared
        .attach(first_client, session)
        .expect("attach first client");
    let second_attached = shared
        .attach(second_client, session)
        .expect("attach second client");
    assert_eq!(first_attached.focused_window, Some(first_window));
    assert_eq!(second_attached.focused_window, Some(first_window));
    assert_eq!(
        first_attached.focused_window_for(&first_attached.sessions[0]),
        first_window
    );

    let mut first_context = create_context.clone();
    let mut second_context = create_context.clone();
    let resize = |client, context: &mut ExecutionContext, pane, columns, rows| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::ResizeTerminal {
                    pane,
                    columns,
                    rows,
                    cell_width_px: 8,
                    cell_height_px: 16,
                },
            )
            .expect("record client geometry");
    };
    resize(first_client, &mut first_context, first_pane, 80, 24);
    resize(second_client, &mut second_context, first_pane, 120, 40);
    resize(first_client, &mut first_context, second_pane, 160, 50);
    resize(second_client, &mut second_context, second_pane, 90, 30);
    take_reliable_messages(&first_mailbox);
    take_reliable_messages(&second_mailbox);
    take_reliable_messages(&observer_mailbox);

    shared
        .execute(
            first_client,
            ClientKind::Interactive,
            &mut first_context,
            &CommandInvocation::new("select-window", ["-t", &second_window.to_string()]),
        )
        .expect("first client focuses logs");

    {
        let mut inner = shared.inner.lock();
        let state = &inner.engine.state.sessions[&session];
        assert_eq!(state.active_window, second_window);
        assert_eq!(
            client_focused_window(&inner, first_client, state),
            second_window
        );
        assert_eq!(
            client_focused_window(&inner, second_client, state),
            first_window
        );
        assert_eq!(
            (*inner.clients[&first_client]
                .visible_terminals
                .as_ref()
                .unwrap()),
            BTreeSet::from([second_pane])
        );
        assert_eq!(
            (*inner.clients[&second_client]
                .visible_terminals
                .as_ref()
                .unwrap()),
            BTreeSet::from([first_pane])
        );

        let first_resize = terminal_resize_for_pane(&inner, first_pane)
            .expect("second client still views the agent terminal")
            .1;
        assert_eq!((first_resize.columns, first_resize.rows), (120, 40));
        let second_resize = terminal_resize_for_pane(&inner, second_pane)
            .expect("first client now views the logs terminal")
            .1;
        assert_eq!((second_resize.columns, second_resize.rows), (160, 50));
        inner.engine.set_pane_geometry(first_pane, 120, 40);
        inner.engine.set_pane_geometry(second_pane, 160, 50);

        let snapshot = inner.engine.state.snapshot();
        let first_status = status_context(
            &snapshot,
            &inner.engine,
            Some(session),
            client_focused_window_for_attachment(&inner, first_client),
        );
        let second_status = status_context(
            &snapshot,
            &inner.engine,
            Some(session),
            client_focused_window_for_attachment(&inner, second_client),
        );
        assert_eq!(first_status.window_name, "logs");
        assert_eq!(second_status.window_name, "agent");
        assert_eq!(first_status.pane_width, Some(160));
        assert_eq!(first_status.pane_height, Some(50));
        assert_eq!(first_status.window_width, Some(160));
        assert_eq!(first_status.window_height, Some(50));
        assert_eq!(first_status.pane_active, Some(true));
        assert_eq!(first_status.window_active, Some(true));
        assert_eq!(second_status.pane_width, Some(120));
        assert_eq!(second_status.pane_height, Some(40));
        assert_eq!(second_status.window_width, Some(120));
        assert_eq!(second_status.window_height, Some(40));
        assert_eq!(second_status.pane_active, Some(true));
        assert_eq!(second_status.window_active, Some(true));
    }

    let first_messages = take_reliable_messages(&first_mailbox);
    let second_messages = take_reliable_messages(&second_mailbox);
    let observer_messages = take_reliable_messages(&observer_mailbox);
    assert!(first_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::Snapshot(snapshot),
            ..
        }) if snapshot.focused_window == Some(second_window)
    )));
    assert!(second_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::Snapshot(snapshot),
            ..
        }) if snapshot.focused_window == Some(first_window)
    )));
    assert!(observer_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::Snapshot(snapshot),
            ..
        }) if snapshot.focused_window.is_none()
    )));

    let mut command_context =
        ExecutionContext::new(Some(session), Some(second_window), Some(second_pane));
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("select-window", ["-t", &first_window.to_string()]),
        )
        .expect("unattached command client moves every viewer");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner
                .client(first_client)
                .and_then(|c| c.focused_window.as_ref()),
            Some(&first_window)
        );
        assert_eq!(
            inner
                .client(second_client)
                .and_then(|c| c.focused_window.as_ref()),
            Some(&first_window)
        );
        assert_eq!(
            (*inner.clients[&first_client]
                .visible_terminals
                .as_ref()
                .unwrap()),
            BTreeSet::from([first_pane])
        );
        assert_eq!(
            (*inner.clients[&second_client]
                .visible_terminals
                .as_ref()
                .unwrap()),
            BTreeSet::from([first_pane])
        );
    }

    shared
        .execute(
            second_client,
            ClientKind::Interactive,
            &mut second_context,
            &CommandInvocation::new("select-window", ["-t", &second_window.to_string()]),
        )
        .expect("second client focuses logs");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner
                .client(first_client)
                .and_then(|c| c.focused_window.as_ref()),
            Some(&first_window)
        );
        assert_eq!(
            inner
                .client(second_client)
                .and_then(|c| c.focused_window.as_ref()),
            Some(&second_window)
        );
    }
    take_reliable_messages(&second_mailbox);

    shared
        .execute(
            first_client,
            ClientKind::Interactive,
            &mut first_context,
            &CommandInvocation::new("kill-window", ["-t", &second_window.to_string()]),
        )
        .expect("kill the second client's focused window");
    {
        let inner = shared.inner.lock();
        let state = &inner.engine.state.sessions[&session];
        assert_eq!(state.active_window, first_window);
        assert_eq!(
            inner
                .client(second_client)
                .and_then(|c| c.focused_window.as_ref()),
            Some(&second_window),
            "stale entries are retained and healed on read"
        );
        assert_eq!(
            client_focused_window(&inner, second_client, state),
            first_window
        );
        assert_eq!(
            (*inner.clients[&second_client]
                .visible_terminals
                .as_ref()
                .unwrap()),
            BTreeSet::from([first_pane])
        );
    }
    assert!(
        take_reliable_messages(&second_mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Snapshot(snapshot),
                    ..
                }) if snapshot.focused_window == Some(first_window)
            ))
    );

    shared.detach(first_client);
    assert!(shared.read_client(first_client, |c| {
        c.is_none_or(|c| c.focused_window.is_none())
    }));
    shared.unregister(second_client);
    assert!(shared.read_client(second_client, |c| {
        c.is_none_or(|c| c.focused_window.is_none())
    }));
    shared.unregister(observer);
}

#[test]
fn multi_client_session_switch_and_unregister_clean_views_and_geometry() {
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
    let (first_session, first_pane, first_terminal) =
        output_view_session_fixture(&shared, "one", "slice1-peer-still-live");
    let (second_session, second_pane, second_terminal) =
        output_view_session_fixture(&shared, "two", "second session");

    shared
        .attach(first_client, first_session)
        .expect("attach first client");
    shared
        .attach(second_client, first_session)
        .expect("join second client");
    let mut first_context = ExecutionContext::default();
    let mut second_context = ExecutionContext::default();
    shared
        .input(
            first_client,
            ClientKind::Interactive,
            &mut first_context,
            InputMessage::ResizeTerminal {
                pane: first_pane,
                columns: 200,
                rows: 60,
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
                pane: first_pane,
                columns: 100,
                rows: 30,
                cell_width_px: 7,
                cell_height_px: 16,
            },
        )
        .expect("second geometry");
    wait_for_terminal_dimensions(&first_terminal, TerminalViewId(first_client.0), 100, 30);

    {
        let mut inner = shared.inner.lock();
        let first_window = inner.engine.state.sessions[&first_session].active_window;
        inner
            .client_entry(second_client)
            .focused_window
            .replace(first_window);
    }
    shared
        .attach(second_client, second_session)
        .expect("switch second client to session two");
    wait_for_terminal_dimensions(&first_terminal, TerminalViewId(first_client.0), 200, 60);
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.attachment.attached.get(&first_session),
            Some(&BTreeSet::from([first_client]))
        );
        assert_eq!(
            inner.attachment.attached.get(&second_session),
            Some(&BTreeSet::from([second_client]))
        );
        assert_eq!(
            inner
                .client(second_client)
                .and_then(|c| c.visible_terminals.as_ref()),
            Some(&BTreeSet::from([second_pane]))
        );
        assert!(
            inner
                .client(second_client)
                .is_none_or(|c| c.focused_window.is_none()),
            "switching sessions must evict the previous per-client focus"
        );
    }
    shared
        .input(
            second_client,
            ClientKind::Interactive,
            &mut second_context,
            InputMessage::ResizeTerminal {
                pane: second_pane,
                columns: 90,
                rows: 20,
                cell_width_px: 7,
                cell_height_px: 16,
            },
        )
        .expect("second-session geometry");
    wait_for_terminal_dimensions(&second_terminal, TerminalViewId(second_client.0), 90, 20);

    shared.unregister(second_client);
    let deadline = Instant::now() + Duration::from_secs(30);
    while (first_terminal
        .latest_viewport_for(TerminalViewId(second_client.0))
        .is_some()
        || second_terminal
            .latest_viewport_for(TerminalViewId(second_client.0))
            .is_some())
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(10));
    }
    shared.send_resync(first_client, &first_mailbox);
    let mut first_state = TerminalTestState::default();
    wait_for_mailbox_terminal(&first_mailbox, &mut first_state, |message, state| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalViewport { pane, .. },
                ..
            }) if *pane == first_pane
        ) && state
            .viewports
            .get(&first_pane)
            .is_some_and(|viewport| viewport_text(viewport).contains("slice1-peer-still-live"))
    });

    let inner = shared.inner.lock();
    assert_eq!(
        inner.attachment.attached.get(&first_session),
        Some(&BTreeSet::from([first_client]))
    );
    assert!(!inner.attachment.attached.contains_key(&second_session));
    assert!(
        inner
            .client(first_client)
            .is_some_and(|c| c.subscriber.is_some())
    );
    assert!(
        inner
            .panes
            .terminal_geometries
            .values()
            .all(|geometries| !geometries.contains_key(&second_client))
    );
    assert!(
        first_terminal
            .latest_viewport_for(TerminalViewId(first_client.0))
            .is_some(),
        "unregistering one client must leave the peer view active"
    );
    assert!(
        first_terminal
            .latest_viewport_for(TerminalViewId(second_client.0))
            .is_none()
            && second_terminal
                .latest_viewport_for(TerminalViewId(second_client.0))
                .is_none(),
        "unregister must release every view owned by the client"
    );
}

#[test]
fn daemon_cross_session_swap_transfers_visibility_without_restarting_surfaces() {
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
    let mut first_context = ExecutionContext::default();
    let mut second_context = ExecutionContext::default();

    shared
        .execute(
            first_client,
            ClientKind::Interactive,
            &mut first_context,
            &CommandInvocation::new("new-session", ["-s", "first"]),
        )
        .expect("first session");
    let first_session = first_context.session.expect("first session id");
    let first_pane = first_context.pane.expect("first pane");
    shared
        .attach(first_client, first_session)
        .expect("attach first session");

    shared
        .execute(
            second_client,
            ClientKind::Interactive,
            &mut second_context,
            &CommandInvocation::new("new-session", ["-s", "second"]),
        )
        .expect("second session");
    let second_session = second_context.session.expect("second session id");
    let second_pane = second_context.pane.expect("second pane");
    shared
        .attach(second_client, second_session)
        .expect("attach second session");
    let (first_terminal, second_terminal) = {
        let inner = shared.inner.lock();
        (
            Arc::clone(&inner.terminals[&first_pane]),
            Arc::clone(&inner.terminals[&second_pane]),
        )
    };
    take_reliable_messages(&first_mailbox);
    take_reliable_messages(&second_mailbox);
    shared
        .execute(
            first_client,
            ClientKind::Interactive,
            &mut first_context,
            &CommandInvocation::new("choose-tree", [] as [&str; 0]),
        )
        .expect("open chooser on first pane");
    assert!(shared.read_client(first_client, |c| c.is_some_and(|c| c.choose_tree.is_some())));

    shared
        .execute(
            second_client,
            ClientKind::Interactive,
            &mut second_context,
            &CommandInvocation::new(
                "swap-pane",
                [
                    "-s",
                    &first_pane.to_string(),
                    "-t",
                    &second_pane.to_string(),
                ],
            ),
        )
        .expect("cross-session swap");

    let inner = shared.inner.lock();
    assert_eq!(
        (*inner.clients[&first_client]
            .visible_terminals
            .as_ref()
            .unwrap()),
        BTreeSet::from([second_pane])
    );
    assert_eq!(
        (*inner.clients[&second_client]
            .visible_terminals
            .as_ref()
            .unwrap()),
        BTreeSet::from([first_pane])
    );
    assert!(
        inner
            .client(first_client)
            .is_none_or(|c| c.choose_tree.is_none())
    );
    assert!(Arc::ptr_eq(&first_terminal, &inner.terminals[&first_pane]));
    assert!(Arc::ptr_eq(
        &second_terminal,
        &inner.terminals[&second_pane]
    ));
    assert_eq!(
        inner
            .engine
            .state
            .window_for_pane(first_pane)
            .map(|window| inner.engine.state.windows[&window].session),
        Some(second_session)
    );
    assert_eq!(
        inner
            .engine
            .state
            .window_for_pane(second_pane)
            .map(|window| inner.engine.state.windows[&window].session),
        Some(first_session)
    );
}

#[test]
fn daemon_break_and_join_keep_the_terminal_actor_and_refresh_visibility() {
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
    let first = context.pane.expect("first pane");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-h"]),
        )
        .expect("split terminal");
    let moving = context.pane.expect("moving pane");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&moving]);
    shared.attach(client, session).expect("attach session");
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("break-pane", ["-s", &moving.to_string()]),
        )
        .expect("break pane");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.state.sessions[&session].windows.len(), 2);
        assert_eq!(
            (*inner.clients[&client].visible_terminals.as_ref().unwrap()),
            BTreeSet::from([moving])
        );
        assert!(Arc::ptr_eq(&terminal, &inner.terminals[&moving]));
    }

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "join-pane",
                ["-h", "-s", &moving.to_string(), "-t", &first.to_string()],
            ),
        )
        .expect("join pane");
    let inner = shared.inner.lock();
    assert_eq!(inner.engine.state.sessions[&session].windows.len(), 1);
    assert_eq!(
        (*inner.clients[&client].visible_terminals.as_ref().unwrap()),
        BTreeSet::from([first, moving])
    );
    assert!(Arc::ptr_eq(&terminal, &inner.terminals[&moving]));
    assert!(inner.engine.state.validate().is_ok());
}

#[test]
fn daemon_rotation_preserves_terminal_actors_and_moves_zoom_visibility() {
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
            &CommandInvocation::new(
                "split-window",
                ["--kind", "browser", "-h", "https://rotate.example"],
            ),
        )
        .expect("browser pane");
    let browser = context.pane.expect("browser pane");
    let first_actor = Arc::clone(&shared.inner.lock().terminals[&first]);
    let second_actor = Arc::clone(&shared.inner.lock().terminals[&second]);
    shared.attach(client, session).expect("attach session");
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("resize-pane", ["-Z"]),
        )
        .expect("zoom browser");
    assert!(
        shared.inner.lock().clients[&client]
            .visible_terminals
            .as_ref()
            .unwrap()
            .is_empty()
    );
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("rotate-window", ["-Z"]),
        )
        .expect("rotate zoomed browser forward");
    assert_eq!(context.pane, Some(first));
    {
        let inner = shared.inner.lock();
        assert_eq!(
            (*inner.clients[&client].visible_terminals.as_ref().unwrap()),
            BTreeSet::from([first])
        );
        assert!(Arc::ptr_eq(&first_actor, &inner.terminals[&first]));
        assert!(Arc::ptr_eq(&second_actor, &inner.terminals[&second]));
        assert_eq!(
            inner.engine.state.windows[&context.window.unwrap()].zoomed_pane,
            Some(first)
        );
    }
    assert!(take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::Snapshot(snapshot),
                ..
            }) if snapshot.sessions[0].windows[0].active_pane == first
                && snapshot.sessions[0].windows[0].zoomed_pane == Some(first)
        )
    }));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("rotate-window", ["-D", "-Z"]),
        )
        .expect("rotate back to browser");
    assert_eq!(context.pane, Some(browser));
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
            &CommandInvocation::new("rotate-window", [] as [&str; 0]),
        )
        .expect("rotate and leave zoom");
    let inner = shared.inner.lock();
    assert_eq!(context.pane, Some(first));
    assert_eq!(
        (*inner.clients[&client].visible_terminals.as_ref().unwrap()),
        BTreeSet::from([first, second])
    );
    assert_eq!(
        inner.engine.state.windows[&context.window.unwrap()].zoomed_pane,
        None
    );
    assert!(Arc::ptr_eq(&first_actor, &inner.terminals[&first]));
    assert!(Arc::ptr_eq(&second_actor, &inner.terminals[&second]));
    assert!(inner.engine.state.validate().is_ok());
}
