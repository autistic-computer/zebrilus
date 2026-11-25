use super::*;

#[test]
fn read_only_display_panes_bypasses_overlay_consumption_and_preserves_bell() {
    let shared = Arc::new(Shared::new(1));
    let (session, pane, _) =
        output_view_session_fixture(&shared, "read-only-display-panes", "display");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("read-only-display-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .attach(client, session)
        .expect("attach display client");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("display panes context");
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
            &CommandInvocation::new("display-panes", ["-d", "0"]),
        )
        .expect("open display overlay");
    let initial = {
        let mut inner = shared.inner.lock();
        inner.client_flags.insert(client);
        assert!(inner.engine.state.set_pane_bell(pane, true));
        (
            inner.activity_sequence,
            inner.terminal_input_sequence,
            inner.clients[&client].display_panes.as_ref().unwrap().token,
        )
    };

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::DisplayPanes {
                action: DisplayPanesAction::Key(test_key(
                    KeyCode::Character('x'),
                    Modifiers::default(),
                    Some("x"),
                )),
            },
        )
        .expect("bypass display overlay with read-only key");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.activity_sequence, initial.0 + 1);
        assert_eq!(inner.terminal_input_sequence, initial.1 + 1);
        assert!(inner.clients[&client].display_panes.is_none());
        assert!(!inner.pane_modes.contains_key(&pane));
        assert_eq!(
            inner.clients[&client].copy_session.as_ref().unwrap().pane,
            pane
        );
        assert!(inner.engine.state.pane(pane).expect("display pane").bell);
    }
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("display-panes", ["-d", "0", "-t", &pane.to_string()]),
        )
        .expect("reopen display overlay over copy mode");
    let reopened = shared.inner.lock().clients[&client]
        .display_panes
        .as_ref()
        .unwrap()
        .token;
    assert_ne!(reopened, initial.2);

    let safe_view_before = {
        let inner = shared.inner.lock();
        (inner.activity_sequence, inner.terminal_input_sequence)
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
        .expect("bypass display overlay with read-only-safe copy-mode action");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.activity_sequence, safe_view_before.0 + 1);
        assert_eq!(inner.terminal_input_sequence, safe_view_before.1 + 1);
        assert_eq!(
            inner.clients[&client].display_panes.as_ref().unwrap().token,
            reopened
        );
        assert!(
            inner.clients[&client]
                .copy_session
                .as_ref()
                .unwrap()
                .exiting
        );
        assert!(inner.engine.state.pane(pane).expect("display pane").bell);
    }

    let select_before = {
        let inner = shared.inner.lock();
        (inner.activity_sequence, inner.terminal_input_sequence)
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::DisplayPanes {
                action: DisplayPanesAction::Select(pane),
            },
        )
        .expect("reject read-only native pane selection");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.activity_sequence, select_before.0 + 1);
        assert_eq!(inner.terminal_input_sequence, select_before.1 + 1);
        assert_eq!(
            inner.clients[&client].display_panes.as_ref().unwrap().token,
            reopened
        );
        assert!(inner.engine.state.pane(pane).expect("display pane").bell);
    }

    let close_before = {
        let inner = shared.inner.lock();
        (inner.activity_sequence, inner.terminal_input_sequence)
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::DisplayPanes {
                action: DisplayPanesAction::Close,
            },
        )
        .expect("bypass read-only display Escape");
    let inner = shared.inner.lock();
    assert_eq!(inner.activity_sequence, close_before.0 + 1);
    assert_eq!(inner.terminal_input_sequence, close_before.1 + 1);
    assert_eq!(
        inner.clients[&client].display_panes.as_ref().unwrap().token,
        reopened
    );
    assert!(inner.engine.state.pane(pane).expect("display pane").bell);
}

#[test]
fn client_target_collisions_follow_global_creation_order_after_session_switches() {
    let shared = Arc::new(Shared::new(1));
    let (older, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("name-collision".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (newer, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("newer".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    assert!(older < newer);
    let (early, late) = {
        let mut inner = shared.inner.lock();
        let (early, _, _) = inner
            .engine
            .state
            .create_session("early")
            .expect("early session");
        let (late, _, _) = inner
            .engine
            .state
            .create_session("late")
            .expect("late session");
        (early, late)
    };
    shared.attach(older, early).expect("attach older client");
    shared.attach(newer, late).expect("attach newer client");
    shared.attach(older, late).expect("switch older client");
    shared.attach(newer, early).expect("switch newer client");
    {
        let mut inner = shared.inner.lock();
        inner
            .client_entry(newer)
            .tty
            .replace("/dev/name-collision".to_owned());
        assert_eq!(client_attached_session(&inner, newer), Some(early));
        assert_eq!(client_attached_session(&inner, older), Some(late));
        assert_eq!(find_attached_client(&inner, "name-collision"), Some(older));
        assert_eq!(find_attached_client(&inner, "name-collision:"), Some(older));

        inner
            .client_entry(older)
            .tty
            .replace("/dev/tty-collision".to_owned());
        inner
            .client_entry(newer)
            .name
            .replace("tty-collision".to_owned());
        assert_eq!(find_attached_client(&inner, "tty-collision"), Some(older));
        assert_eq!(find_attached_client(&inner, "tty-collision:"), Some(older));
        assert_eq!(find_attached_client(&inner, "tty-collision::"), None);
        for alias in [
            older.0.to_string(),
            format!("client-{}", older.0),
            format!("device-{}", older.0),
        ] {
            assert_eq!(
                find_attached_client_with_aliases(&inner, &alias, true),
                Some(older)
            );
        }
    }
}

#[test]
fn overlay_refresh_and_lock_callers_share_client_tty_targets() {
    let (shared, target, _, mut context) = popup_test_workspace("target-client");
    let session = context.session.expect("target session");
    let (invoking, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("invoking-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control-target".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .attach(invoking, session)
        .expect("attach invoking client");
    shared
        .attach(control, session)
        .expect("attach control client");
    {
        let mut inner = shared.inner.lock();
        inner
            .client_entry(target)
            .tty
            .replace("/dev/pts/9".to_owned());
        inner
            .client_entry(control)
            .tty
            .replace("/dev/pts/10".to_owned());
    }
    let target_selectors = vec![
        "/dev/pts/9".to_owned(),
        "/dev/pts/9:".to_owned(),
        "pts/9".to_owned(),
        "pts/9:".to_owned(),
        "target-client:".to_owned(),
        target.0.to_string(),
        format!("client-{}", target.0),
        format!("device-{}", target.0),
    ];
    for selector in &target_selectors {
        shared
            .execute(
                invoking,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new(
                    "display-menu",
                    ["-c", selector, "Item", "i", "display-message selected"],
                ),
            )
            .expect("target display-menu client");
        assert!(shared.read_client(target, |c| c.is_some_and(|c| c.menu.is_some())));
        assert!(shared.read_client(invoking, |c| c.is_none_or(|c| c.menu.is_none())));
        shared
            .execute(
                invoking,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("display-popup", ["-c", selector, "-C"]),
            )
            .expect("target display-popup client");
        assert!(shared.read_client(target, |c| c.is_none_or(|c| c.menu.is_none())));

        shared
            .execute(
                invoking,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new(
                    "confirm-before",
                    ["-t", selector, "display-message confirmed"],
                ),
            )
            .expect("target confirm-before client");
        assert!(shared.read_client(target, |c| c.is_some_and(|c| c.confirm.is_some())));
        assert!(shared.read_client(invoking, |c| c.is_none_or(|c| c.confirm.is_none())));
        shared.input_confirm(target, &context, ConfirmAction::Reply(false));

        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("lock-client", ["-t", selector]),
            )
            .expect("target lock-client");
    }

    let control_selectors = vec![
        "/dev/pts/10".to_owned(),
        "/dev/pts/10:".to_owned(),
        "pts/10".to_owned(),
        "pts/10:".to_owned(),
        "control-target:".to_owned(),
        control.0.to_string(),
        format!("client-{}", control.0),
        format!("device-{}", control.0),
    ];
    for selector in &control_selectors {
        shared
            .execute(
                invoking,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("refresh-client", ["-t", selector, "-f", "no-output"]),
            )
            .expect("target refresh-client");
        assert!(
            shared.inner.lock().clients[&control]
                .control_output
                .as_ref()
                .unwrap()
                .no_output
        );
    }
}

#[test]
fn display_panes_validates_its_delay_after_its_target_and_needs_no_client() {
    let (shared, target, _, mut context) = popup_test_workspace("target");
    let error = |args: &[&str]| match shared.execute(
        ClientId(u64::MAX),
        ClientKind::Command,
        &mut context.clone(),
        &CommandInvocation::new("display-panes", args.iter().copied()),
    ) {
        Err(DaemonError::Server(error)) => error.tmux_message(),
        other => panic!("display-panes {args:?} did not fail: {other:?}"),
    };
    assert_eq!(error(&["-d", "not-a-delay"]), "delay invalid");
    assert_eq!(
        error(&["-t", "missing", "-d", "x"]),
        "can't find pane: missing"
    );
    assert_eq!(error(&["-b"]), "command display-panes: unknown flag -b");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("display-panes", ["-d", "0"]),
        )
        .expect("display-panes from a command client");
    let pane = context.pane.expect("pane");
    assert!(matches!(
        shared
            .inner
            .lock()
            .pane_modes
            .get(&pane)
            .and_then(|modes| modes.last()),
        Some(PaneModeRequest::Panes(_))
    ));
    assert!(shared.read_client(target, |c| {
        c.and_then(|c| c.display_panes.as_ref())
            .is_some_and(|overlay| overlay.mode == Some(pane))
    }));
}

#[test]
fn display_panes_selection_follows_the_pin_order_of_template_kill_and_unzoom() {
    let (shared, client, _, mut context) = popup_test_workspace("panes-order");
    let run = |context: &mut ExecutionContext, name: &str, args: &[&str]| {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                context,
                &CommandInvocation::new(name, args.iter().copied()),
            )
            .unwrap_or_else(|error| panic!("{name} {args:?}: {error:?}"))
    };
    run(&mut context.clone(), "split-window", &["-d"]);
    let window = context.window.expect("window");
    let host = context.pane.expect("host");
    run(
        &mut context.clone(),
        "display-panes",
        &["-k", "-d", "0", "set-environment -g PICKED %%%"],
    );
    assert!(shared.pane_mode_input(client, &mut context, host, PaneModeInput::Key("0")));
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("PICKED"),
        Some(host.to_string())
    );
    assert!(
        !shared.inner.lock().engine.state.windows[&window]
            .pane_order()
            .contains(&host)
    );

    let host = shared.inner.lock().engine.state.windows[&window].pane_order()[0];
    run(
        &mut context.clone(),
        "split-window",
        &["-d", "-t", &host.to_string()],
    );
    run(&mut context.clone(), "new-window", &["-d", "-n", "other"]);
    run(
        &mut context.clone(),
        "split-window",
        &["-d", "-t", ":other"],
    );
    run(
        &mut context.clone(),
        "resize-pane",
        &["-Z", "-t", &host.to_string()],
    );
    assert_eq!(
        shared.inner.lock().engine.state.windows[&window].zoomed_pane,
        Some(host)
    );
    run(
        &mut context.clone(),
        "display-panes",
        &["-d", "0", "-t", &host.to_string(), "-s", ":other"],
    );
    run(
        &mut context.clone(),
        "display-panes",
        &["-d", "nonsense", "-t", &host.to_string()],
    );
    assert!(shared.pane_mode_input(client, &mut context, host, PaneModeInput::Key("1")));
    assert_eq!(
        shared.inner.lock().engine.state.windows[&window].zoomed_pane,
        None
    );
}

#[test]
fn display_panes_hides_the_zoomed_pane_status_row_and_expands_its_border_style() {
    let (shared, client, _, mut context) = popup_test_workspace("panes-status");
    shared.inner.lock().client_entry(client).exits_on_detach = true;
    let run = |context: &mut ExecutionContext, name: &str, args: &[&str]| {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                context,
                &CommandInvocation::new(name, args.iter().copied()),
            )
            .unwrap_or_else(|error| panic!("{name} {args:?}: {error:?}"))
    };
    run(&mut context.clone(), "split-window", &["-d"]);
    run(
        &mut context.clone(),
        "set-option",
        &["-w", "pane-border-status", "top"],
    );
    run(
        &mut context.clone(),
        "set-option",
        &[
            "-w",
            "display-panes-border-style",
            "fg=#{?pane_active,red,blue}",
        ],
    );
    let window = context.window.expect("window");
    let host = context.pane.expect("host");
    run(&mut context.clone(), "display-panes", &["-d", "0"]);
    let (extent, geometry, mode) = {
        let inner = shared.inner.lock();
        let facts = format_hook_facts(&inner);
        let Some(PaneModeRequest::Panes(mode)) =
            inner.pane_modes.get(&host).and_then(|modes| modes.last())
        else {
            panic!("panes mode");
        };
        (
            (
                inner
                    .engine
                    .window_extent(window, zz_protocol::Axis::Horizontal)
                    .unwrap_or_default(),
                inner
                    .engine
                    .window_extent(window, zz_protocol::Axis::Vertical)
                    .unwrap_or_default(),
            ),
            inner.engine.pane_geometry(host),
            panes_mode_snapshot(&inner, &facts, host, mode),
        )
    };
    assert_eq!(geometry, Some(extent));
    let PaneMode::Panes {
        border_style, copy, ..
    } = mode
    else {
        panic!("panes snapshot");
    };
    assert_eq!(border_style, "fg=red");
    assert!(copy);
    assert!(shared.pane_mode_input(client, &mut context, host, PaneModeInput::Key("q")));
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .displayed_pane_border_status(window),
        zz_protocol::PaneBorderStatus::Top
    );
}

#[test]
fn display_panes_resized_back_keeps_the_live_frames() {
    let (shared, _, _, context) = popup_test_workspace("panes-frames");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("display-panes", ["-d", "0"]),
        )
        .expect("display-panes");
    let host = context.pane.expect("host");
    let mut inner = shared.inner.lock();
    let size = inner.engine.pane_geometry(host);
    let frames = inner.panes_mode_frames.get_mut(&host).expect("frames");
    assert_eq!(frames.size, size);
    frames.size = Some((1, 1));
    let facts = format_hook_facts(&inner);
    let Some(PaneModeRequest::Panes(mode)) = inner
        .pane_modes
        .get(&host)
        .and_then(|modes| modes.last())
        .cloned()
    else {
        panic!("panes mode");
    };
    let _ = panes_mode_snapshot(&inner, &facts, host, &mode);
    let frames = inner.panes_mode_frames.get_mut(&host).expect("frames");
    assert!(frames.resized.get());
    frames.size = size;
    drop(facts);
    let facts = format_hook_facts(&inner);
    let _ = panes_mode_snapshot(&inner, &facts, host, &mode);
    assert!(inner.panes_mode_frames[&host].resized.get());
}

#[test]
fn display_panes_routes_by_the_clients_viewing_the_pane() {
    let (shared, desktop, desktop_mailbox, mut context) = popup_test_workspace("panes-desk");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("split-window", ["-d"]),
        )
        .expect("split");
    let window = context.window.expect("window");
    let host = context.pane.expect("host");
    let second = shared.inner.lock().engine.state.windows[&window].pane_order()[1];
    take_reliable_messages(&desktop_mailbox);
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new(
                "display-panes",
                ["-k", "-d", "0", "-t", &second.to_string()],
            ),
        )
        .expect("display-panes from a command client");
    {
        let inner = shared.inner.lock();
        assert!(matches!(
            inner.pane_modes.get(&second).and_then(|modes| modes.last()),
            Some(PaneModeRequest::Panes(_))
        ));
        assert_eq!(inner.engine.state.windows[&window].zoomed_pane, None);
        let overlay = inner.clients[&desktop]
            .display_panes
            .as_ref()
            .expect("overlay");
        assert_eq!(overlay.mode, Some(second));
    }
    assert!(
        take_reliable_messages(&desktop_mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::DisplayPanes { state: Some(_) },
                    ..
                })
            ))
    );
    shared
        .input(
            desktop,
            ClientKind::Interactive,
            &mut context,
            InputMessage::DisplayPanes {
                action: DisplayPanesAction::Close,
            },
        )
        .expect("close");
    let inner = shared.inner.lock();
    assert!(inner.clients[&desktop].display_panes.is_none());
    assert!(!inner.pane_modes.contains_key(&second));
    assert!(
        !inner.engine.state.windows[&window]
            .pane_order()
            .contains(&second)
    );
    assert!(
        inner.engine.state.windows[&window]
            .pane_order()
            .contains(&host)
    );
}

#[test]
fn display_panes_is_a_mode_of_its_target_pane() {
    let (shared, client, _, mut context) = popup_test_workspace("panes");
    shared.inner.lock().client_entry(client).exits_on_detach = true;
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-d"]),
        )
        .expect("split");
    let window = context.window.expect("window");
    let (first, second) = {
        let inner = shared.inner.lock();
        let order = inner.engine.state.windows[&window].pane_order().to_vec();
        (order[0], order[1])
    };
    let base = context.clone();
    let open = |args: &[&str], pane: PaneId| {
        let target = pane.to_string();
        let mut args = args.to_vec();
        args.extend(["-t", target.as_str()]);
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut base.clone(),
                &CommandInvocation::new("display-panes", args.iter().copied()),
            )
            .expect("display-panes");
    };
    let mode = |pane: PaneId| {
        shared
            .inner
            .lock()
            .pane_modes
            .get(&pane)
            .and_then(|modes| modes.last())
            .cloned()
    };
    let zoomed = || shared.inner.lock().engine.state.windows[&window].zoomed_pane;
    let active = || shared.inner.lock().engine.state.windows[&window].active_pane;

    open(&["-d", "0"], first);
    assert!(matches!(mode(first), Some(PaneModeRequest::Panes(_))));
    assert_eq!(zoomed(), Some(first));
    open(&["-d", "0", "-N"], first);
    let Some(PaneModeRequest::Panes(opened)) = mode(first) else {
        panic!("panes mode");
    };
    assert!(!opened.ignore_keys);
    assert!(shared.pane_mode_input(client, &mut context, first, PaneModeInput::Key("1")));
    assert!(mode(first).is_none());
    assert_eq!(zoomed(), None);
    assert_eq!(active(), second);

    open(&["-d", "0", "-N", "-Z"], first);
    assert_eq!(zoomed(), None);
    assert!(shared.pane_mode_input(client, &mut context, first, PaneModeInput::Key("1")));
    assert!(matches!(mode(first), Some(PaneModeRequest::Panes(_))));
    assert!(shared.pane_mode_input(client, &mut context, first, PaneModeInput::Key("q")));
    assert!(mode(first).is_none());

    open(&["-d", "0"], first);
    let Some(PaneModeRequest::Panes(opened)) = mode(first) else {
        panic!("panes mode");
    };
    shared.expire_panes_mode(first, opened.token.wrapping_add(1));
    assert!(mode(first).is_some());
    shared.expire_panes_mode(first, opened.token);
    assert!(mode(first).is_none());
    assert_eq!(zoomed(), None);

    open(&["-d", "0"], first);
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut base.clone(),
            &CommandInvocation::new("clock-mode", ["-t", &first.to_string()]),
        )
        .expect("clock-mode");
    assert!(matches!(mode(first), Some(PaneModeRequest::Clock)));
    assert_eq!(shared.inner.lock().pane_modes[&first].len(), 1);
    assert_eq!(zoomed(), None);
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut base.clone(),
            &CommandInvocation::new("copy-mode", ["-q", "-t", &first.to_string()]),
        )
        .expect("copy-mode -q");

    open(&["-d", "0", "-k"], second);
    assert!(shared.pane_mode_input(client, &mut context, second, PaneModeInput::Key("Z")));
    assert!(
        !shared.inner.lock().engine.state.windows[&window]
            .pane_order()
            .contains(&second)
    );
}

#[test]
fn display_panes_fires_the_pane_mode_hooks_on_every_exit() {
    let (shared, client, _, mut context) = popup_test_workspace("panes-hooks");
    let base = context.clone();
    let run = |name: &str, args: &[&str]| {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut base.clone(),
                &CommandInvocation::new(name, args.iter().copied()),
            )
            .unwrap_or_else(|error| panic!("{name} {args:?}: {error:?}"))
    };
    run("split-window", &["-d"]);
    let pane = context.pane.expect("pane");
    let target = pane.to_string();
    run("set-option", &["-g", "@dpc", ""]);
    for hook in ["pane-mode-entered", "pane-mode-exited", "pane-mode-changed"] {
        run(
            "set-hook",
            &[
                "-g",
                hook,
                "set -gaF @dpc '#{hook}:#{hook_pane}:#{hook_current_mode}<#{hook_previous_mode}:#{hook_mode_entered} '",
            ],
        );
    }
    let token = || match shared
        .inner
        .lock()
        .pane_modes
        .get(&pane)
        .and_then(|modes| modes.last())
    {
        Some(PaneModeRequest::Panes(mode)) => mode.token,
        _ => panic!("panes mode"),
    };

    let show = |name: &str| {
        run("show-options", &["-gv", name])
            .output
            .to_string()
            .trim_end()
            .to_owned()
    };
    let entered = |current: &str, previous: &str| {
        ["pane-mode-entered", "pane-mode-changed"]
            .map(|hook| format!("{hook}:{pane}:{current}<{previous}:1"))
            .join(" ")
    };
    let exited = |current: &str, previous: &str| {
        ["pane-mode-exited", "pane-mode-changed"]
            .map(|hook| format!("{hook}:{pane}:{current}<{previous}:0"))
            .join(" ")
    };

    run("display-panes", &["-d", "0", "-t", &target]);
    assert!(shared.pane_mode_input(client, &mut context, pane, PaneModeInput::Key("q")));
    assert!(show("@dpc").ends_with(&exited("", "panes-mode")));
    run(
        "display-panes",
        &["-d", "0", "-t", &target, "set -g @picked %%%"],
    );
    assert!(shared.pane_mode_input(client, &mut context, pane, PaneModeInput::Key("1")));
    assert!(show("@picked").starts_with('%'));
    assert!(show("@dpc").ends_with(&exited("", "panes-mode")));
    run("display-panes", &["-d", "0", "-t", &target]);
    shared.expire_panes_mode(pane, token());
    assert!(show("@dpc").ends_with(&exited("", "panes-mode")));
    run("clock-mode", &["-t", &target]);
    run("display-panes", &["-d", "0", "-t", &target]);
    run("clock-mode", &["-t", &target]);

    let expected = [
        entered("panes-mode", ""),
        exited("", "panes-mode"),
        entered("panes-mode", ""),
        exited("", "panes-mode"),
        entered("panes-mode", ""),
        exited("", "panes-mode"),
        entered("clock-mode", ""),
        entered("panes-mode", "clock-mode"),
        exited("clock-mode", "panes-mode"),
        entered("clock-mode", "clock-mode"),
    ]
    .join(" ");
    assert_eq!(show("@dpc"), expected);
    assert!(shared.inner.lock().pane_mode_transitions.is_empty());
}

#[test]
fn display_panes_focus_dismissal_ends_the_mode_it_hides() {
    let (shared, client, mut context, run) = panes_overlay_fixture("panes-focus");
    let host = context.pane.expect("host");
    run("set-option", &["-g", "focus-events", "on"]);
    run("display-panes", &["-d", "0", "-t", &host.to_string()]);
    assert_eq!(panes_overlay_host(&shared, client), Some(host));
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: false },
        )
        .expect("focus out");
    assert_eq!(panes_overlay_host(&shared, client), None);
    assert!(!top_is_panes_mode(&shared, host));
}

#[test]
fn copy_mode_replaces_display_panes_and_fires_its_exit() {
    let (shared, client, mut context, run) = panes_overlay_fixture("panes-copy");
    let host = context.pane.expect("host");
    let target = host.to_string();
    run(
        "set-hook",
        &[
            "-g",
            "pane-mode-exited",
            "set -gF @px '#{hook_previous_mode}'",
        ],
    );
    run("display-panes", &["-d", "0", "-t", &target]);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("copy-mode", ["-t", &target]),
        )
        .expect("copy-mode");
    assert!(!top_is_panes_mode(&shared, host));
    assert_eq!(
        run("show-options", &["-gv", "@px"])
            .output
            .to_string()
            .trim_end(),
        "panes-mode"
    );
    shared.publish_snapshot();
    assert_eq!(panes_overlay_host(&shared, client), None);
}

#[test]
fn display_panes_overlay_follows_the_host_window_not_the_source() {
    let (shared, client, context, run) = panes_overlay_fixture("panes-source");
    let host = context.pane.expect("host");
    run("new-window", &["-d", "-n", "other"]);
    run(
        "display-panes",
        &["-d", "0", "-t", &host.to_string(), "-s", "popup-test:1"],
    );
    shared.publish_snapshot();
    assert!(top_is_panes_mode(&shared, host));
    assert_eq!(panes_overlay_host(&shared, client), Some(host));
}

#[test]
fn display_panes_overlay_reaches_a_desktop_client_that_attaches_later() {
    let (shared, _, context, run) = panes_overlay_fixture("panes-late");
    let host = context.pane.expect("host");
    run("display-panes", &["-d", "0", "-t", &host.to_string()]);
    let (late, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("panes-late-desktop".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .attach(late, context.session.expect("session"))
        .expect("attach the late client");
    shared.publish_snapshot();
    assert_eq!(panes_overlay_host(&shared, late), Some(host));
}

#[test]
fn display_panes_over_a_zoomed_customize_mode_keeps_its_zoom() {
    let (shared, client, mut context, run) = panes_overlay_fixture("panes-zoom");
    let host = context.pane.expect("host");
    let window = context.window.expect("window");
    let target = host.to_string();
    run("split-window", &["-d", "-t", &target]);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context.clone(),
            &CommandInvocation::new("customize-mode", ["-Z", "-t", &target]),
        )
        .expect("customize-mode -Z");
    let zoomed = || shared.inner.lock().engine.state.windows[&window].zoomed_pane;
    assert_eq!(zoomed(), Some(host));
    run("display-panes", &["-d", "0", "-t", &target]);
    assert!(shared.pane_mode_input(client, &mut context, host, PaneModeInput::Key("q")));
    assert!(matches!(
        shared
            .inner
            .lock()
            .pane_modes
            .get(&host)
            .and_then(|modes| modes.last()),
        Some(PaneModeRequest::Customize(_))
    ));
    assert_eq!(zoomed(), Some(host));
}
