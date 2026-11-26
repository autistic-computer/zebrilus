use super::*;

#[test]
fn display_popup_parser_keeps_repeated_values_and_command_shape() {
    assert_eq!(
        parse_display_popup_args(&[
            "-BEEkN".into(),
            "-w20".into(),
            "-w".into(),
            "75%".into(),
            "-e".into(),
            "A=1".into(),
            "-eB=2".into(),
            "--".into(),
            "printf".into(),
            "%s".into(),
            "-x".into(),
        ])
        .unwrap(),
        ParsedDisplayPopup {
            borderless: true,
            clear: false,
            close_on_exit_count: 2,
            close_on_any_key: true,
            reset_close_flags: true,
            width: Some("75%".to_owned()),
            environment: vec!["A=1".to_owned(), "B=2".to_owned()],
            command: vec!["printf".to_owned(), "%s".to_owned(), "-x".to_owned()],
            ..ParsedDisplayPopup::default()
        }
    );
    assert!(matches!(
        parse_display_popup_args(&["-T".into()]),
        Err(ServerError::CommandParse(message))
            if message == "display-popup -T requires a value"
    ));
    assert!(matches!(
        parse_display_popup_args(&["-Z".into()]),
        Err(ServerError::CommandParse(message))
            if message == "unsupported command: display-popup -Z"
    ));
}

#[test]
fn popup_geometry_uses_the_clients_terminal_metrics_when_a_browser_is_active() {
    let shared = Shared::new(1);
    let mut inner = shared.inner.lock();
    let (session, window, terminal) = inner.engine.state.create_session("popup-metrics").unwrap();
    let agent = inner
        .engine
        .state
        .split_pane(
            terminal,
            zz_protocol::Axis::Horizontal,
            PaneKind::Browser(zz_protocol::BrowserDescriptor::single(
                "about:blank".to_owned(),
                "default".to_owned(),
            )),
        )
        .unwrap();
    inner
        .engine
        .set_manual_window_extent(window, 120, 40)
        .unwrap();
    let retina = ClientId(1);
    let other = ClientId(2);
    inner
        .attachment
        .attached
        .insert(session, BTreeSet::from([retina, other]));
    inner
        .client_entry(retina)
        .kind
        .replace(ClientKind::Interactive);
    inner
        .client_entry(other)
        .kind
        .replace(ClientKind::Interactive);
    let measured = TerminalGeometry {
        columns: 59,
        rows: 40,
        cell_width_px: 16,
        cell_height_px: 36,
    };
    let other_measured = TerminalGeometry {
        cell_width_px: 9,
        cell_height_px: 20,
        ..measured
    };
    inner.panes.terminal_geometries.insert(
        terminal,
        BTreeMap::from([(retina, measured), (other, other_measured)]),
    );
    assert_eq!(inner.engine.state.windows[&window].active_pane, agent);
    assert_eq!(
        popup_client_geometry(&inner, retina).unwrap(),
        Some(TerminalGeometry {
            columns: 120,
            rows: 40,
            ..measured
        }),
    );
    assert_eq!(
        popup_client_geometry(&inner, other).unwrap(),
        Some(TerminalGeometry {
            columns: 120,
            rows: 40,
            ..other_measured
        }),
    );
    inner
        .panes
        .terminal_geometries
        .get_mut(&terminal)
        .unwrap()
        .remove(&retina);
    assert_eq!(
        popup_client_geometry(&inner, retina).unwrap(),
        Some(TerminalGeometry {
            columns: 120,
            rows: 40,
            cell_width_px: CONTROL_CELL_WIDTH_PX,
            cell_height_px: CONTROL_CELL_HEIGHT_PX,
        }),
    );
}

#[test]
fn popup_geometry_takes_the_clients_full_terminal_height_like_tty_sy() {
    let shared = Shared::new(1);
    let mut inner = shared.inner.lock();
    let (session, window, _terminal) = inner
        .engine
        .state
        .create_session("popup-full-height")
        .unwrap();
    inner
        .engine
        .set_manual_window_extent(window, 79, 22)
        .unwrap();
    let client = ClientId(1);
    inner
        .attachment
        .attached
        .insert(session, BTreeSet::from([client]));
    inner
        .client_entry(client)
        .kind
        .replace(ClientKind::Interactive);
    inner.client_entry(client).size.replace((79, 23));
    let geometry = popup_client_geometry(&inner, client).unwrap().unwrap();
    assert_eq!((geometry.columns, geometry.rows), (79, 23));
}

#[test]
fn popup_dimensions_percentages_and_errors_match_the_pin() {
    assert_eq!(parse_popup_dimension("width", None, 80).unwrap(), 40);
    assert_eq!(parse_popup_dimension("height", None, 25).unwrap(), 12);
    assert_eq!(
        parse_popup_dimension("width", Some("100%"), 80).unwrap(),
        80
    );
    assert_eq!(parse_popup_dimension("width", Some("25%"), 80).unwrap(), 20);
    for (value, reason) in [
        ("", "empty"),
        ("0", "too small"),
        ("81", "too large"),
        ("-1", "too small"),
        ("wat", "invalid"),
        ("1%", "too small"),
        ("200%", "too large"),
    ] {
        assert!(matches!(
            parse_popup_dimension("width", Some(value), 80),
            Err(ServerError::InvalidCommand(message))
                if message == format!("width {reason}")
        ));
    }
}

#[test]
fn popup_positions_use_left_and_bottom_coordinates_then_clamp() {
    let expand = |value: &str| match value {
        "#{popup_centre_x}" => "29".to_owned(),
        "#{popup_centre_y}" => "16".to_owned(),
        "#{popup_pane_right}" => "55".to_owned(),
        "#{popup_pane_bottom}" => "23".to_owned(),
        "#{popup_last_x}" => "0".to_owned(),
        "#{popup_last_y}" => "10".to_owned(),
        _ => value.to_owned(),
    };
    assert_eq!(popup_position(None, None, 80, 24, 20, 10, expand), (29, 6));
    assert_eq!(
        popup_position(Some("R"), Some("P"), 80, 24, 20, 10, expand),
        (55, 13)
    );
    assert_eq!(
        popup_position(Some("L"), Some("L"), 80, 24, 20, 10, expand),
        (0, 0)
    );
    assert_eq!(
        popup_position(Some("50%"), Some("50%"), 80, 24, 20, 10, expand),
        (40, 2)
    );
    assert_eq!(
        popup_position(Some("999"), Some("999"), 80, 24, 20, 10, expand),
        (60, 14)
    );
}

#[cfg(unix)]
#[test]
fn display_popup_is_one_modal_pane_every_attached_client_sees() {
    let (shared, first, first_mailbox, mut context) = popup_test_workspace("modal-first");
    let second_mailbox = OutboundMailbox::new();
    let (second, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("modal-second".to_owned()),
        None,
        Arc::clone(&second_mailbox),
    );
    shared
        .attach(second, context.session.expect("popup test session"))
        .expect("attach second client");
    take_reliable_messages(&first_mailbox);
    take_reliable_messages(&second_mailbox);

    shared
        .execute(
            first,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-popup", ["-E", "sleep 1"]),
        )
        .expect("open popup");
    let modal = modal_pane(&shared, &context).expect("display-popup made a modal pane");
    for mailbox in [&first_mailbox, &second_mailbox] {
        let messages = take_reliable_messages(mailbox);
        assert!(!messages.iter().any(|message| matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::RetiredPopup { .. },
                ..
            })
        )));
        let window = &latest_reliable_snapshot(&messages).sessions[0].windows[0];
        assert_eq!(window.modal.map(|modal| modal.pane), Some(modal));
        assert!(window.modal.is_some_and(|modal| modal.capture_keys));
        assert!(window.floating.iter().any(|float| float.pane == modal));
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while modal_pane(&shared, &context).is_some() {
        assert!(
            Instant::now() < deadline,
            "the -E popup did not close on exit"
        );
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(window_pane_count(&shared, &context), 1);
    assert!(!shared.inner.lock().terminals.contains_key(&modal));
}

#[cfg(unix)]
#[test]
fn nested_attach_passes_from_a_popup_and_is_refused_from_a_modal_pane() {
    let (shared, first, _mailbox, mut context) = popup_test_workspace("popup-nested");
    let session = context.session.expect("popup test session");
    let attach_from = |pane: PaneId, name: &str| {
        let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
        wait_for_terminal_identity(&terminal);
        let tty = terminal
            .tty()
            .expect("pane tty")
            .to_string_lossy()
            .into_owned();
        let (nested, _) = shared.register_subscribed(
            ClientKind::Control,
            Some(name.to_owned()),
            None,
            OutboundMailbox::new(),
        );
        {
            let mut inner = shared.inner.lock();
            inner.client_entry(nested).tty.replace(tty);
            inner.client_entry(nested).nested = true;
        }
        shared.execute(
            nested,
            ClientKind::Control,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("attach-session", ["-t", &session.to_string()]),
        )
    };

    shared
        .execute(
            first,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-popup", ["-E", "sleep 30"]),
        )
        .expect("open popup");
    let popup = modal_pane(&shared, &context).expect("display-popup made a modal pane");
    attach_from(popup, "from-popup").expect("attach from a popup's tty");

    shared
        .execute(
            first,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-popup", ["-C"]),
        )
        .expect("close popup");
    shared
        .execute(
            first,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-pane", ["-O", "sleep 30"]),
        )
        .expect("open a modal pane");
    let modal = modal_pane(&shared, &context).expect("new-pane -O made a modal pane");
    assert!(!shared.inner.lock().panes.popup_panes.contains(&modal));
    match attach_from(modal, "from-modal") {
        Err(DaemonError::Server(ServerError::InvalidCommand(message))) => assert_eq!(
            message,
            "sessions should be nested with care, unset $TMUX to force"
        ),
        other => panic!("attach from a modal pane's tty was not refused: {other:?}"),
    }
    shared.request_shutdown();
}

#[cfg(unix)]
#[test]
fn display_popup_from_a_command_client_exits_with_the_pane_status() {
    let (shared, _, _, context) = popup_test_workspace("modal-exit");
    let error = popup_command_with_timeout(
        &shared,
        ClientId(7_000),
        &context,
        vec!["-E".to_owned(), "exit 3".to_owned()],
    )
    .expect_err("a failing popup command fails the command client");
    assert!(
        matches!(error, DaemonError::CommandExit { exit_code: 3, .. }),
        "{error:?}"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while modal_pane(&shared, &context).is_some() {
        assert!(Instant::now() < deadline, "the -E popup did not close");
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
#[test]
fn display_popup_clear_second_popup_and_empty_command_follow_master() {
    let (shared, client, _, mut context) = popup_test_workspace("modal-clear");
    let tile = context.pane.expect("popup test pane");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-pane", ["-O", "-t", &tile.to_string()]),
        )
        .expect("open a modal pane");
    assert!(modal_pane(&shared, &context).is_some());
    assert_eq!(window_pane_count(&shared, &context), 2);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-popup", ["-t", &tile.to_string()]),
        )
        .expect("a second popup is ignored");
    assert_eq!(window_pane_count(&shared, &context), 2);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-popup", ["-C", "-t", &tile.to_string()]),
        )
        .expect("display-popup -C kills the modal");
    assert_eq!(modal_pane(&shared, &context), None);
    assert_eq!(window_pane_count(&shared, &context), 1);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-popup", ["-t", &tile.to_string(), ""]),
        )
        .expect("display-popup with an empty command");
    let modal = modal_pane(&shared, &context).expect("an empty command still opens a modal");
    thread::sleep(Duration::from_millis(300));
    let inner = shared.inner.lock();
    let pane = inner.engine.state.pane(modal).expect("modal pane");
    assert!(!pane.dead);
    assert!(
        inner
            .terminals
            .get(&modal)
            .is_some_and(|terminal| terminal.completion().is_none())
    );
}

#[cfg(unix)]
#[test]
fn display_popup_keeps_a_failed_command_that_exits_at_once() {
    let (shared, client, _, mut context) = popup_test_workspace("modal-failed");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-popup", ["-EE", "exit 3"]),
        )
        .expect("open a popup whose command fails at once");
    let modal = modal_pane(&shared, &context).expect("display-popup made a modal pane");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let dead = shared
            .inner
            .lock()
            .engine
            .state
            .pane(modal)
            .is_some_and(|pane| pane.dead);
        if dead {
            break;
        }
        assert!(
            modal_pane(&shared, &context) == Some(modal),
            "the -EE popup closed on a failed exit"
        );
        assert!(Instant::now() < deadline, "the failed popup never died");
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(200));
    assert_eq!(modal_pane(&shared, &context), Some(modal));
    let shown = shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "show-options",
                ["-pv", "-t", &modal.to_string(), "remain-on-exit"],
            ),
        )
        .expect("show the popup's remain-on-exit");
    assert_eq!(
        String::from_utf8_lossy(shown.output.as_bytes()).trim(),
        "failed"
    );
}

#[cfg(unix)]
#[test]
fn a_waited_popup_killed_before_its_command_exits_answers_129() {
    let (shared, client, _, mut context) = popup_test_workspace("modal-killed");
    let waiter = {
        let shared = Arc::clone(&shared);
        let mut context = context.clone();
        thread::spawn(move || {
            shared.execute(
                ClientId(7_001),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("display-popup", ["-E", "sleep 30"]),
            )
        })
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while modal_pane(&shared, &context).is_none() {
        assert!(Instant::now() < deadline, "the waited popup never opened");
        thread::sleep(Duration::from_millis(20));
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-popup", ["-C"]),
        )
        .expect("display-popup -C kills the waited popup");
    let error = waiter
        .join()
        .expect("popup waiter")
        .expect_err("a killed popup fails its command client");
    assert!(
        matches!(error, DaemonError::CommandExit { exit_code: 129, .. }),
        "{error:?}"
    );
}

#[cfg(unix)]
#[test]
fn escape_kills_a_close_on_cancel_modal_and_capture_keys_skips_the_prefix() {
    let (shared, client, _, mut context) = popup_test_workspace("modal-keys");
    let tile = context.pane.expect("popup test pane");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-pane", ["-O", "-K", "-t", &tile.to_string()]),
        )
        .expect("open a capturing modal");
    let modal = modal_pane(&shared, &context).expect("capturing modal");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: modal,
                input: test_key(
                    KeyCode::Character('b'),
                    Modifiers::new(false, true, false, false),
                    None,
                ),
                text_follows: false,
            },
        )
        .expect("send the prefix to the modal");
    assert_ne!(
        shared.read_client(client, |c| c
            .and_then(|c| c.key_engine.as_ref())
            .and_then(|engine| engine.active_table().map(str::to_owned))),
        Some("prefix".to_owned())
    );
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: modal,
                input: test_key(KeyCode::Escape, Modifiers::default(), None),
                text_follows: false,
            },
        )
        .expect("escape without -D");
    assert_eq!(modal_pane(&shared, &context), Some(modal));
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("kill-pane", ["-t", &modal.to_string()]),
        )
        .expect("close the capturing modal");

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-popup", ["-t", &tile.to_string()]),
        )
        .expect("open a plain popup");
    let modal = modal_pane(&shared, &context).expect("popup modal");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: modal,
                input: test_key(KeyCode::Escape, Modifiers::default(), None),
                text_follows: false,
            },
        )
        .expect("escape the popup");
    assert_eq!(modal_pane(&shared, &context), None);
    assert_eq!(window_pane_count(&shared, &context), 1);
}

#[cfg(unix)]
#[test]
fn a_click_outside_a_modal_does_nothing_or_closes_a_close_on_click_modal() {
    let (shared, client, _, mut context) = popup_test_workspace("modal-click");
    let tile = context.pane.expect("popup test pane");
    let window = context.window.expect("popup test window");
    let click = |shared: &Arc<Shared>, context: &mut ExecutionContext| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::MouseKey {
                    key: "MouseDown1Pane".to_owned(),
                    pane: Some(tile),
                    window: Some(window),
                    column: 1,
                    row: 20,
                    border: None,
                    view_action: None,
                    press_action: None,
                    status_range_start: None,
                    press: None,
                },
            )
            .expect("click outside the modal");
    };
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-pane", ["-O", "-t", &tile.to_string()]),
        )
        .expect("open a modal");
    let modal = modal_pane(&shared, &context).expect("modal");
    click(&shared, &mut context);
    assert_eq!(modal_pane(&shared, &context), Some(modal));
    assert_eq!(
        shared.inner.lock().engine.state.windows[&window].active_pane,
        modal
    );
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("kill-pane", ["-t", &modal.to_string()]),
        )
        .expect("close the modal");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-pane", ["-O", "-C", "-t", &tile.to_string()]),
        )
        .expect("open a close-on-click modal");
    assert!(modal_pane(&shared, &context).is_some());
    click(&shared, &mut context);
    assert_eq!(modal_pane(&shared, &context), None);
    assert_eq!(window_pane_count(&shared, &context), 1);
}

#[cfg(unix)]
#[test]
fn floating_pane_ptys_follow_their_allocation() {
    let (shared, client, _, mut context) = popup_test_workspace("float-sizes");
    let tile = context.pane.expect("popup test pane");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-pane", ["-x", "22", "-y", "8"]),
        )
        .expect("open a float");
    let float = context.pane.expect("float pane");
    wait_for_terminal_size(&shared, float, (20, 6));
    let float_cell = |shared: &Shared| {
        terminal_resize_for_pane(&shared.inner.lock(), float)
            .map(|(_, geometry)| (geometry.cell_width_px, geometry.cell_height_px))
    };
    assert_eq!(float_cell(&shared), Some((8, 18)));
    for focus in [tile, float] {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("select-pane", ["-t", &focus.to_string()]),
            )
            .expect("focus a pane");
        assert_eq!(float_cell(&shared), Some((8, 18)), "focus on {focus}");
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "resize-pane",
                ["-t", &float.to_string(), "-x", "30", "-y", "10"],
            ),
        )
        .expect("resize the float");
    wait_for_terminal_size(&shared, float, (28, 8));
    let window = context.window.expect("float window");
    let window_extent = |shared: &Shared| {
        let inner = shared.inner.lock();
        (
            inner
                .engine
                .window_extent(window, zz_protocol::Axis::Horizontal),
            inner
                .engine
                .window_extent(window, zz_protocol::Axis::Vertical),
        )
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane: float,
                columns: 50,
                rows: 20,
                cell_width_px: 8,
                cell_height_px: 18,
            },
        )
        .expect("a client size report for the float");
    assert_eq!(window_extent(&shared), (Some(80), Some(24)));
    wait_for_terminal_size(&shared, float, (28, 8));

    let toggle_zoom = |context: &mut ExecutionContext| {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                context,
                &CommandInvocation::new("resize-pane", ["-Z", "-t", &float.to_string()]),
            )
            .expect("toggle the float's zoom");
    };
    toggle_zoom(&mut context);
    wait_for_terminal_size(&shared, float, (80, 24));
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane: float,
                columns: 60,
                rows: 20,
                cell_width_px: 8,
                cell_height_px: 18,
            },
        )
        .expect("a client size report for the zoomed float");
    assert_eq!(window_extent(&shared), (Some(60), Some(20)));
    wait_for_terminal_size(&shared, float, (60, 20));
    toggle_zoom(&mut context);
    wait_for_terminal_size(&shared, float, (28, 8));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-t", &tile.to_string()]),
        )
        .expect("split the tile");
    let split = context.pane.expect("split pane");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("resize-pane", ["-Z", "-t", &split.to_string()]),
        )
        .expect("zoom a tile under the float");
    thread::sleep(Duration::from_millis(200));
    wait_for_terminal_size(&shared, float, (28, 8));
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("resize-pane", ["-Z", "-t", &split.to_string()]),
        )
        .expect("unzoom");

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("resize-window", ["-x", "15", "-y", "5"]),
        )
        .expect("shrink the window");
    wait_for_terminal_size(&shared, float, (13, 3));
}

#[cfg(unix)]
#[test]
fn a_float_tiled_back_takes_its_unchanged_tile_report() {
    let (shared, client, _, mut context) = popup_test_workspace("float-tile-back");
    let pane = context.pane.expect("popup test pane");
    shared.inner.lock().client_entry(client).ctrl_subscriptions =
        Some(zz_protocol::Subscriptions::terminal());
    let mut run = |name: &str, args: &[&str]| {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new(name, args.iter().copied()),
            )
            .expect(name);
    };
    let target = pane.to_string();
    run("break-pane", &["-W", "-x", "22", "-y", "8", "-s", &target]);
    wait_for_terminal_size(&shared, pane, (20, 6));
    run("join-pane", &["-s", &target, "-t", &target]);
    shared.compact_tree_messages(client, true);
    let layout_generation = shared.inner.lock().clients[&client]
        .ctrl_layout
        .expect("a layout generation")
        .1;
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminalV2 {
                pane,
                columns: 80,
                rows: 24,
                cell_width_px: 8,
                cell_height_px: 18,
                layout_generation,
            },
        )
        .expect("the tile's report");
    wait_for_terminal_size(&shared, pane, (80, 24));
}

#[cfg(unix)]
#[test]
fn a_popup_from_a_floated_pane_goes_to_the_client_that_typed_last() {
    let (shared, first, _, mut context) = popup_test_workspace("float-popup-first");
    let floated = context.pane.expect("popup test pane");
    let (second, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("float-popup-second".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .attach(second, context.session.expect("popup test session"))
        .expect("attach the second client");
    shared
        .execute(
            first,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-h", "-t", &floated.to_string()]),
        )
        .expect("split the pane");
    let tile = context.pane.expect("split pane");
    let report = InputMessage::ResizeTerminal {
        pane: tile,
        columns: 40,
        rows: 24,
        cell_width_px: 8,
        cell_height_px: 18,
    };
    for (client, message) in [
        (first, report.clone()),
        (second, report),
        (second, InputMessage::ClientFocus { focused: true }),
    ] {
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                message,
            )
            .expect("a client input");
    }
    shared
        .execute(
            first,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("break-pane", ["-W", "-s", &floated.to_string()]),
        )
        .expect("float the pane");
    let inner = shared.inner.lock();
    let invoked = ExecutionContext::for_pane(&inner.engine.state, floated)
        .expect("the floated pane's context");
    assert_eq!(
        resolve_popup_client(&inner, ClientId(u64::MAX), &invoked, None).ok(),
        Some(second)
    );
}
