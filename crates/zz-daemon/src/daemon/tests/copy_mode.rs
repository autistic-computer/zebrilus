use super::*;

#[test]
fn word_separator_changes_reconfigure_existing_and_future_terminal_actors() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "words"]),
        )
        .expect("session");
    let session = context.session.expect("session id");
    let first = context.pane.expect("first pane");
    assert!(
        shared.inner.lock().terminals[&first]
            .word_separators()
            .contains_separator('.')
    );

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-t", &session.to_string(), "word-separators", ""],
            ),
        )
        .expect("empty session separators");
    assert!(
        !shared.inner.lock().terminals[&first]
            .word_separators()
            .contains_separator('.')
    );

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("split-window", ["-h"]),
        )
        .expect("second terminal");
    let second = context.pane.expect("second pane");
    assert!(
        !shared.inner.lock().terminals[&second]
            .word_separators()
            .contains_separator('.')
    );

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "word-separators", "|"]),
        )
        .expect("global separators");
    assert!(
        !shared.inner.lock().terminals[&first]
            .word_separators()
            .contains_separator('|')
    );
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-u", "-t", &session.to_string(), "word-separators"],
            ),
        )
        .expect("restore inheritance");
    let inner = shared.inner.lock();
    for pane in [first, second] {
        assert!(
            inner.terminals[&pane]
                .word_separators()
                .contains_separator('|')
        );
    }
}

#[test]
fn mode_keys_retarget_active_and_future_native_copy_modes() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(ClientKind::Interactive, None, None, mailbox);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "mode-keys"]),
        )
        .expect("session");
    let session = context.session.expect("session id");
    let window = context.window.expect("window id");
    let pane = context.pane.expect("pane id");
    shared.attach(client, session).expect("attach client");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    let view = TerminalViewId(client.0);

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("copy-mode", ["-t", &pane.to_string()]),
        )
        .expect("enter default copy mode");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("copy-mode")
    );
    wait_for_view_mode(
        &terminal,
        view,
        "default copy mode never reached the viewport",
        |mode| matches!(mode, TerminalMode::Copy { .. }),
    );
    wait_for_observed_copy_session(&shared, client);

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                ["-t", &window.to_string(), "mode-keys", "vi"],
            ),
        )
        .expect("switch active mode table");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("copy-mode-vi")
    );

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-gw", "mode-keys", "emacs"]),
        )
        .expect("change global default under override");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("copy-mode-vi")
    );

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-u", "-t", &window.to_string(), "mode-keys"]),
        )
        .expect("restore live inheritance");
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
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("send-keys", ["-t", &pane.to_string(), "-X", "cancel"]),
        )
        .expect("leave copy mode");
    wait_for_view_mode(&terminal, view, "cancel never thawed the pane", |mode| {
        mode == TerminalMode::Live
    });
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                ["-t", &window.to_string(), "mode-keys", "vi"],
            ),
        )
        .expect("configure future copy mode");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("copy-mode", ["-t", &pane.to_string()]),
        )
        .expect("enter configured copy mode");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("copy-mode-vi")
    );
}

#[test]
fn command_output_hands_the_key_table_back_while_keys_target_another_pane() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (session, active, _) = output_view_session_fixture(&shared, "output-focus", "active");
    let (_, hidden, _) = output_view_session_fixture(&shared, "output-hidden", "hidden");
    shared
        .attach(client, session)
        .expect("attach output client");
    take_reliable_messages(&mailbox);
    shared
        .open_command_output(client, Some(hidden), "fixture".to_owned(), "one\ntwo")
        .expect("open command output");
    take_command_output_message(&mailbox);

    let mut inner = shared.inner.lock();
    let table = inner.clients[&client]
        .key_engine
        .as_ref()
        .unwrap()
        .active_table()
        .map(str::to_owned);
    assert!(matches!(
        table.as_deref(),
        Some("copy-mode" | "copy-mode-vi")
    ));
    assert!(command_output_owns_pane(&inner, client, hidden));
    assert!(!command_output_owns_pane(&inner, client, active));

    follow_command_output_focus(&mut inner, client, active);
    assert_eq!(
        inner.clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        None
    );
    assert!(
        inner.clients[&client]
            .command_output
            .as_ref()
            .unwrap()
            .parked
    );
    follow_command_output_focus(&mut inner, client, active);
    assert_eq!(
        inner.clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        None
    );

    follow_command_output_focus(&mut inner, client, hidden);
    assert_eq!(
        inner.clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table()
            .map(str::to_owned),
        table
    );
    assert!(
        !inner.clients[&client]
            .command_output
            .as_ref()
            .unwrap()
            .parked
    );
}

#[test]
fn mode_keys_retarget_active_command_output_and_restore_the_previous_table() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let observer_mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (observer, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, observer_mailbox);
    let (session, pane, _) = output_view_session_fixture(&shared, "output-mode-keys", "pane");
    let window = shared
        .inner
        .lock()
        .engine
        .state
        .window_for_pane(pane)
        .expect("output window");
    shared
        .attach(client, session)
        .expect("attach output client");
    shared.attach(observer, session).expect("attach observer");
    take_reliable_messages(&mailbox);
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("output context");

    set_test_window_mode_keys(&shared, 71, &mut context, window, "emacs");
    for table in ["custom-client-table", "observer-table"] {
        shared
            .execute(
                ClientId(71),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "bind-key",
                    ["-T", table, "x", "display-message", "-p", table],
                ),
            )
            .expect("configure client table");
    }
    {
        let mut inner = shared.inner.lock();
        inner
            .client_entry(client)
            .key_engine
            .get_or_insert_default()
            .switch_table(Some("custom-client-table".to_owned()));
        inner
            .client_entry(observer)
            .key_engine
            .get_or_insert_default()
            .switch_table(Some("observer-table".to_owned()));
    }
    shared
        .open_command_output(client, Some(pane), "fixture".to_owned(), "one\ntwo")
        .expect("open command output");
    take_command_output_message(&mailbox);
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            Some("copy-mode")
        );
        assert_eq!(
            inner.clients[&observer]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            Some("observer-table")
        );
    }

    set_test_window_mode_keys(&shared, 72, &mut context, window, "vi");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            Some("copy-mode-vi")
        );
        assert_eq!(
            inner.clients[&observer]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            Some("observer-table")
        );
    }
    shared
        .execute(
            ClientId(74),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                [
                    "-T",
                    "copy-mode-vi",
                    "x",
                    "display-message",
                    "-p",
                    "EDITED-BINDING",
                ],
            ),
        )
        .expect("edit the active command-output table");
    input_test_key(
        &shared,
        client,
        &mut context,
        pane,
        test_key(KeyCode::Character('x'), Modifiers::default(), Some("x")),
    );
    let edited = take_command_output_message(&mailbox);
    let ProtocolMessage::Event(Event {
        payload:
            EventPayload::CommandOutput {
                viewport: Some(viewport),
                ..
            },
        ..
    }) = edited
    else {
        panic!("expected edited binding output");
    };
    assert!(viewport_text(&viewport).contains("EDITED-BINDING"));
    assert!(shared.read_client(client, |c| c.is_some_and(|c| c.command_output.is_some())));
    input_test_key(
        &shared,
        client,
        &mut context,
        pane,
        test_key(KeyCode::Escape, Modifiers::default(), None),
    );
    thread::sleep(Duration::from_millis(50));
    assert!(shared.read_client(client, |c| c.is_some_and(|c| c.command_output.is_some())));

    set_test_window_mode_keys(&shared, 73, &mut context, window, "emacs");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            Some("copy-mode")
        );
        assert_eq!(
            inner.clients[&observer]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            Some("observer-table")
        );
    }
    let output_id = current_command_output_id(&shared, client);
    input_test_key(
        &shared,
        client,
        &mut context,
        pane,
        test_key(KeyCode::Escape, Modifiers::default(), None),
    );
    wait_for_command_output_close(&mailbox, output_id);
    let inner = shared.inner.lock();
    assert!(
        inner
            .client(client)
            .is_none_or(|c| c.command_output.is_none())
    );
    assert_eq!(
        inner.clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("custom-client-table")
    );
    assert_eq!(
        inner.clients[&observer]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("observer-table")
    );
}

#[cfg(unix)]
#[test]
fn mode_keys_update_the_copy_table_beneath_command_output() {
    let (shared, client, mut context, pane, terminal, mailbox) =
        copy_mode_fixture("output-copy-restore", "printf 'one\\ntwo\\n'");
    let window = context.window.expect("copy window");
    let mut command_context = context.clone();
    set_test_window_mode_keys(&shared, 81, &mut command_context, window, "emacs");
    enter_observed_copy_mode(&shared, client, &mut context, pane, &terminal);
    shared
        .open_command_output(client, Some(pane), "fixture".to_owned(), "one\ntwo")
        .expect("open command output over copy mode");
    take_command_output_message(&mailbox);

    set_test_window_mode_keys(&shared, 82, &mut command_context, window, "vi");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            Some("copy-mode-vi")
        );
        assert_eq!(
            inner.clients[&client]
                .command_output
                .as_ref()
                .unwrap()
                .previous_key_table
                .as_deref(),
            Some("copy-mode-vi")
        );
    }
    let output_id = current_command_output_id(&shared, client);
    input_test_key(
        &shared,
        client,
        &mut context,
        pane,
        test_key(KeyCode::Character('q'), Modifiers::default(), Some("q")),
    );
    wait_for_command_output_close(&mailbox, output_id);
    let inner = shared.inner.lock();
    assert!(
        inner
            .client(client)
            .is_some_and(|c| c.copy_session.is_some())
    );
    assert_eq!(
        inner.clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("copy-mode-vi")
    );
}

#[cfg(unix)]
#[test]
fn mode_keys_scope_visible_command_output_separately_from_underlying_copy_mode() {
    let (shared, client, mut context, pane, terminal, mailbox) =
        copy_mode_fixture("output-copy-scope", "printf 'one\\ntwo\\n'");
    let session = context.session.expect("copy session");
    let copy_window = context.window.expect("copy window");
    let mut command_context = context.clone();
    shared
        .execute(
            ClientId(83),
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new(
                "new-window",
                ["-d", "-n", "output-scope", "--", "sleep", "300"],
            ),
        )
        .expect("create output window");
    let (output_window, output_pane) = {
        let inner = shared.inner.lock();
        inner
            .engine
            .state
            .windows
            .iter()
            .find(|(window, state)| **window != copy_window && state.session == session)
            .map(|(window, state)| (*window, state.active_pane))
            .expect("separate output window")
    };
    for window in [copy_window, output_window] {
        set_test_window_mode_keys(&shared, 84, &mut command_context, window, "emacs");
    }
    enter_observed_copy_mode(&shared, client, &mut context, pane, &terminal);
    shared
        .open_command_output(client, Some(output_pane), "fixture".to_owned(), "one\ntwo")
        .expect("open differently scoped command output");
    take_command_output_message(&mailbox);

    set_test_window_mode_keys(&shared, 85, &mut command_context, copy_window, "vi");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .key_engine
                .as_ref()
                .unwrap()
                .active_table(),
            Some("copy-mode")
        );
        assert_eq!(
            inner.clients[&client]
                .command_output
                .as_ref()
                .unwrap()
                .previous_key_table
                .as_deref(),
            Some("copy-mode-vi")
        );
    }
    let output_id = current_command_output_id(&shared, client);
    input_test_key(
        &shared,
        client,
        &mut context,
        output_pane,
        test_key(KeyCode::Escape, Modifiers::default(), None),
    );
    wait_for_command_output_close(&mailbox, output_id);
    let inner = shared.inner.lock();
    assert!(
        inner
            .client(client)
            .is_some_and(|c| c.copy_session.is_some())
    );
    assert_eq!(
        inner.clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("copy-mode-vi")
    );
}

#[cfg(not(windows))]
#[test]
fn copy_pipe_receives_the_selection_on_standard_input() {
    let output = tempfile::NamedTempFile::new().expect("output file");
    let command = format!("cat > {}", shell_quote(output.path()));

    run_copy_pipe(
        &command,
        "native selection\nwith two lines",
        &test_copy_pipe_environment(),
    )
    .expect("copy pipe");

    assert_eq!(
        fs::read_to_string(output.path()).expect("piped output"),
        "native selection\nwith two lines"
    );
}

#[cfg(unix)]
#[test]
fn copy_pipe_timeout_kills_descendants() {
    let scratch = tempfile::tempdir().expect("temporary directory");
    let group = scratch.path().join("group");
    let command = format!(
        "printf '%s' \"$$\" > {}; sleep 30 & wait",
        shell_quote(&group)
    );

    let error = run_copy_pipe_with_timeout(
        &command,
        "",
        &test_copy_pipe_environment(),
        Duration::from_secs(2),
    )
    .expect_err("copy pipe should time out");
    assert!(error.contains("timed out"), "{error}");
    let group = fs::read_to_string(group)
        .expect("copy-pipe process group never started")
        .parse()
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .expect("copy-pipe process group pid");
    wait_for_process_group_exit(group, Duration::from_secs(8), "copy-pipe");
}

#[test]
fn copy_pipe_failure_is_a_reliable_client_message_and_releases_its_permit() {
    let shared = Arc::new(Shared::new(1));
    #[cfg(unix)]
    let _loop = pipe_jobs::Driver::new(&shared);
    let subscriber = OutboundMailbox::new();
    let control_subscriber = OutboundMailbox::new();
    let (pane, client, control) = {
        let mut inner = shared.inner.lock();
        let mut context = ExecutionContext::default();
        inner
            .engine
            .execute(
                &mut context,
                &CommandInvocation::new("new-session", [] as [&str; 0]),
            )
            .expect("session");
        let session = context.session.expect("session id");
        let pane = context.pane.expect("pane id");
        let client = ClientId(41);
        let control = ClientId(42);
        inner.attached.entry(session).or_default().insert(client);
        inner.attached.entry(session).or_default().insert(control);
        inner
            .client_entry(client)
            .subscriber
            .replace(Arc::clone(&subscriber));
        inner
            .client_entry(control)
            .subscriber
            .replace(Arc::clone(&control_subscriber));
        inner
            .client_entry(client)
            .kind
            .replace(ClientKind::Interactive);
        inner
            .client_entry(control)
            .kind
            .replace(ClientKind::Control);
        (pane, client, control)
    };
    let command = if cfg!(windows) { "exit /B 7" } else { "exit 7" };

    let wait_for_error = |subscriber: &OutboundMailbox| {
        let deadline = Instant::now() + Duration::from_secs(30);
        while subscriber.state.lock().reliable.is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !subscriber.state.lock().reliable.is_empty(),
            "copy-pipe diagnostic was not delivered"
        );
        let frame = subscriber.recv().expect("copy-pipe diagnostic");
        decode_protocol_frame(&frame).expect("diagnostic frame")
    };

    shared.spawn_copy_pipe(pane, client, command.to_owned(), "selection".to_owned());

    let message = wait_for_error(&subscriber);
    assert!(
        matches!(
            &message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientMessage {
                    pane: Some(actual),
                    kind: ClientMessageKind::Error,
                    text,
                },
                ..
            }) if *actual == pane && text.contains("exited unsuccessfully")
        ),
        "{message:?}"
    );
    assert!(control_subscriber.state.lock().reliable.is_empty());
    assert_eq!(shared.inner.lock().active_copy_pipes, 0);

    shared.spawn_copy_pipe(pane, control, command.to_owned(), "selection".to_owned());

    let message = wait_for_error(&subscriber);
    assert!(
        matches!(
            &message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientMessage {
                    pane: Some(actual),
                    kind: ClientMessageKind::Error,
                    text,
                },
                ..
            }) if *actual == pane && text.contains("exited unsuccessfully")
        ),
        "{message:?}"
    );
    assert!(control_subscriber.state.lock().reliable.is_empty());
    assert_eq!(shared.inner.lock().active_copy_pipes, 0);
}

#[test]
fn copy_pipe_rejects_work_past_the_process_limit() {
    let shared = Arc::new(Shared::new(1));
    shared.inner.lock().active_copy_pipes = MAX_COPY_PIPE_PROCESSES;

    shared.spawn_copy_pipe(
        PaneId(9),
        ClientId(9),
        "ignored".to_owned(),
        "selection".to_owned(),
    );

    assert_eq!(
        shared.inner.lock().active_copy_pipes,
        MAX_COPY_PIPE_PROCESSES
    );
}

#[cfg(unix)]
#[test]
fn upward_wheel_scrolls_a_live_viewport_and_typing_snaps_it_back_to_the_bottom() {
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
            &CommandInvocation::new("new-session", ["-s", "wheel-copy-mode"]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let pane = context.pane.expect("pane");
    shared.attach(client, session).expect("attach session");

    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    let view = TerminalViewId(client.0);
    terminal.resize(16, 4, 8, 18);
    wait_for_terminal_dimensions(&terminal, view, 16, 4);
    terminal.send_text(
        "i=0; while [ $i -lt 40 ]; do printf 'ZZW%02d\\r\\n' \"$i\"; i=$((i+1)); done; printf 'ZZ_WHEEL_DONE\\r\\n'\n",
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ready = terminal.latest_viewport_for(view).is_some_and(|viewport| {
            viewport
                .scrollbar
                .total
                .saturating_sub(viewport.scrollbar.len)
                >= 10
        });
        if ready {
            break;
        }
        assert!(Instant::now() < deadline, "terminal history did not fill");
        thread::sleep(Duration::from_millis(10));
    }

    terminal.view_action(view, TerminalViewAction::ScrollToOffset(5));
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let pinned = terminal.latest_viewport_for(view).is_some_and(|viewport| {
            matches!(viewport.mode, TerminalMode::Live) && viewport.scrollbar.offset == 5
        });
        if pinned {
            break;
        }
        assert!(Instant::now() < deadline, "live viewport did not pin");
        thread::sleep(Duration::from_millis(10));
    }

    let wheel = TerminalMouseInput::new(
        TerminalMousePhase::Press,
        Some(TerminalMouseButton::ScrollUp),
        PointerCellEvent {
            column: 3,
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
    );
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::TerminalView {
                pane,
                action: TerminalViewAction::ScrollWheel {
                    lines: -1,
                    input: wheel,
                },
            },
        )
        .expect("wheel scrolls the live viewport");

    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner
                .client(client)
                .and_then(|c| c.key_engine.as_ref())
                .and_then(KeyEngine::active_table),
            None,
            "a wheel gesture must not move the client onto a copy-mode table"
        );
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.copy_session.is_none()),
            "a wheel gesture must not open a copy session"
        );
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let scrolled = terminal.latest_viewport_for(view).is_some_and(|viewport| {
            viewport.mode == TerminalMode::Live && viewport.scrollbar.offset == 4
        });
        if scrolled {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "wheel did not scroll the live viewport"
        );
        thread::sleep(Duration::from_millis(10));
    }

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(KeyCode::Character('z'), Modifiers::default(), Some("z")),
                text_follows: false,
            },
        )
        .expect("type while scrolled back");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let at_bottom = terminal.latest_viewport_for(view).is_some_and(|viewport| {
            viewport.mode == TerminalMode::Live
                && viewport
                    .scrollbar
                    .offset
                    .saturating_add(viewport.scrollbar.len)
                    >= viewport.scrollbar.total
        });
        if at_bottom {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "typing did not snap the live viewport back to the bottom"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
#[test]
fn a_pending_repeat_prefix_is_spent_by_one_mode_command() {
    let (shared, client, _context, pane, _terminal, _mailbox) =
        copy_mode_fixture("copy-repeat-prefix", ":");
    let mut inner = shared.inner.lock();
    enter_copy_session(&mut inner, client, pane).expect("claim copy mode");
    inner
        .client_entry(client)
        .key_engine
        .get_or_insert_default()
        .set_repeat_count(5);

    let up = zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Up);
    assert_eq!(
        counted_copy_mode_action(&mut inner, client, &up),
        zz_terminal::TerminalViewAction::CopyModeCounted {
            action: zz_terminal::CopyModeAction::Up,
            count: 5,
        }
    );
    assert_eq!(counted_copy_mode_action(&mut inner, client, &up), up);

    let stop =
        zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::StopSelection);
    inner
        .client_entry(client)
        .key_engine
        .get_or_insert_default()
        .set_repeat_count(4);
    assert_eq!(counted_copy_mode_action(&mut inner, client, &stop), stop);
    assert_eq!(counted_copy_mode_action(&mut inner, client, &up), up);
}

#[cfg(unix)]
#[test]
fn pane_search_string_outlives_the_copy_session_and_seeds_the_next_entry() {
    let (shared, client, mut context, pane, terminal, _mailbox) =
        copy_mode_fixture("copy-pane-search", "printf 'needle\\r\\nfill\\r\\n'");
    let target = pane.to_string();
    let answer = |format: &str| {
        shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("display-message", ["-p", "-t", &target, format]),
            )
            .expect("display-message")
            .output
            .trim_end()
            .to_owned()
    };
    let wait_for_answer = |format: &str, expected: &str| {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let got = answer(format);
            if got == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{format} stayed {got:?}, expected {expected:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    };
    let copy_command = |context: &mut ExecutionContext, arguments: &[&str]| {
        let mut full = vec!["-X", "-t", target.as_str()];
        full.extend_from_slice(arguments);
        shared
            .execute(
                client,
                ClientKind::Interactive,
                context,
                &CommandInvocation::new("send-keys", full),
            )
            .expect("send-keys -X");
    };
    assert_eq!(answer("[#{pane_search_string}]"), "[]");

    enter_observed_copy_mode(&shared, client, &mut context, pane, &terminal);
    copy_command(&mut context, &["search-backward", "needle"]);
    wait_for_answer("#{copy_cursor_line}", "needle");
    assert_eq!(answer("[#{pane_search_string}]"), "[needle]");

    copy_command(&mut context, &["cancel"]);
    wait_for_answer("#{pane_in_mode}", "0");
    wait_for_root_key_table(&shared, client, "cancel left the copy session open");
    assert_eq!(answer("[#{pane_search_string}]"), "[needle]");

    enter_observed_copy_mode(&shared, client, &mut context, pane, &terminal);
    assert_eq!(answer("[#{pane_search_string}]"), "[needle]");
    assert_eq!(answer("#{copy_cursor_line}"), "");
    copy_command(&mut context, &["search-again"]);
    wait_for_answer("#{copy_cursor_line}", "needle");
}

#[cfg(unix)]
#[test]
fn send_keys_and_send_prefix_into_copy_mode_run_the_copy_table() {
    let (shared, client, mut context, pane, terminal, _mailbox) =
        copy_mode_fixture("copy-send-keys", "printf 'one\\r\\ntwo\\r\\nthree\\r\\n'");
    let target = pane.to_string();
    let run = |context: &mut ExecutionContext, name: &str, arguments: &[&str]| {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                context,
                &CommandInvocation::new(name, arguments.to_vec()),
            )
            .expect(name)
            .output
    };
    let wait_for_answer = |context: &mut ExecutionContext, format: &str, expected: &str| {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let got = run(context, "display-message", &["-p", "-t", &target, format]);
            if got.trim_end() == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{format} stayed {got:?}, expected {expected:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    };
    run(&mut context, "set-option", &["-g", "mode-keys", "vi"]);
    enter_observed_copy_mode(&shared, client, &mut context, pane, &terminal);
    let start = run(
        &mut context,
        "display-message",
        &["-p", "-t", &target, "#{copy_cursor_y}"],
    )
    .trim_end()
    .parse::<u32>()
    .expect("copy cursor row");
    assert!(start >= 2, "copy cursor row {start}");

    run(&mut context, "send-keys", &["-t", &target, "Z", "k"]);
    wait_for_answer(&mut context, "#{copy_cursor_y}", &(start - 1).to_string());

    run(&mut context, "set-option", &["-g", "prefix", "k"]);
    run(&mut context, "send-prefix", &["-t", &target]);
    wait_for_answer(&mut context, "#{copy_cursor_y}", &(start - 2).to_string());

    run(&mut context, "send-keys", &["-t", &target, "-X", "cancel"]);
    wait_for_answer(&mut context, "#{pane_in_mode}", "0");
    run(&mut context, "send-keys", &["-t", &target, "LIVE", "Enter"]);
    wait_for_viewport(
        &terminal,
        TerminalViewId(client.0),
        "keys sent after the cancel never reached the pane",
        |viewport| viewport_text(viewport).contains("LIVE"),
    );
    let text = viewport_text(
        &terminal
            .latest_viewport_for(TerminalViewId(client.0))
            .expect("viewport"),
    );
    assert!(
        !text.contains('Z'),
        "a key sent into copy mode reached the pane: {text:?}"
    );
}

#[cfg(unix)]
#[test]
fn exiting_copy_session_stays_claimed_until_live_mode_is_published() {
    let (shared, client, _context, pane, _terminal, _mailbox) =
        copy_mode_fixture("copy-exit-order", ":");
    let mut inner = shared.inner.lock();
    enter_copy_session(&mut inner, client, pane).expect("claim copy mode");
    begin_exit_copy_session(&mut inner, client);

    assert!(
        inner.clients[&client]
            .copy_session
            .as_ref()
            .unwrap()
            .exiting
    );
    assert!(active_copy_mode_panes(&inner).is_empty());
    assert_eq!(
        inner
            .client(client)
            .and_then(|c| c.key_engine.as_ref())
            .and_then(KeyEngine::active_table),
        None
    );

    let (unclaimed, changed) = reconcile_copy_session(
        &mut inner,
        pane,
        client,
        TerminalMode::Copy {
            position: 0,
            total: 0,
            hide_position: false,
        },
    );
    assert!(unclaimed.is_none());
    assert!(!changed);
    assert!(
        inner.clients[&client]
            .copy_session
            .as_ref()
            .unwrap()
            .exiting
    );
    assert!(
        !inner.clients[&client]
            .copy_session
            .as_ref()
            .unwrap()
            .observed
    );

    let (unclaimed, changed) = reconcile_copy_session(&mut inner, pane, client, TerminalMode::Live);
    assert!(unclaimed.is_none());
    assert!(!changed);
    assert!(
        inner
            .client(client)
            .is_none_or(|c| c.copy_session.is_none())
    );
}

#[cfg(unix)]
#[test]
fn unclaimed_copy_mode_self_heal_does_not_displace_an_existing_claim() {
    let (shared, client, mut context, claimed_pane, _terminal, _mailbox) =
        copy_mode_fixture("copy-self-heal", ":");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-h"]),
        )
        .expect("split pane");
    let unclaimed_pane = context.pane.expect("unclaimed pane");

    let mut inner = shared.inner.lock();
    enter_copy_session(&mut inner, client, claimed_pane).expect("claim first pane");
    let expected = Arc::clone(&inner.terminals[&unclaimed_pane]);
    let (unclaimed, changed) = reconcile_copy_session(
        &mut inner,
        unclaimed_pane,
        client,
        TerminalMode::Copy {
            position: 0,
            total: 0,
            hide_position: false,
        },
    );

    assert!(
        unclaimed
            .as_ref()
            .is_some_and(|terminal| Arc::ptr_eq(terminal, &expected))
    );
    assert!(!changed);
    assert_eq!(
        inner.clients[&client].copy_session.as_ref().unwrap().pane,
        claimed_pane
    );
    assert!(
        !inner.clients[&client]
            .copy_session
            .as_ref()
            .unwrap()
            .exiting
    );
}

#[cfg(unix)]
#[test]
fn copy_mode_ed_instant_exit_frees_the_key_table_without_observation() {
    let (shared, client, mut context, pane, terminal, _mailbox) =
        copy_mode_fixture("copy-instant-exit", "printf 'one\\ntwo\\n'");
    let view = TerminalViewId(client.0);
    let target = pane.to_string();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("copy-mode", ["-e", "-d", "-t", target.as_str()]),
        )
        .expect("instant scroll-exit entry");
    wait_for_view_mode(
        &terminal,
        view,
        "copy mode did not exit instantly",
        |mode| mode == TerminalMode::Live,
    );
    wait_for_root_key_table(
        &shared,
        client,
        "instant scroll-exit stranded the key table",
    );
}

#[cfg(unix)]
#[test]
fn copy_mode_dash_h_publishes_a_hidden_position_and_nothing_else() {
    for (name, copy_args, hidden) in [
        ("copy-hide-plain", &[][..], false),
        ("copy-hide-scroll", &["-e"][..], false),
        ("copy-hide-only", &["-H"][..], true),
        ("copy-hide-both", &["-eH"][..], true),
    ] {
        let (shared, client, mut context, pane, terminal, _mailbox) = copy_mode_fixture(
            name,
            "i=0; while [ $i -lt 12 ]; do printf 'line-%02d\\n' \"$i\"; i=$((i+1)); done",
        );
        let view = TerminalViewId(client.0);
        let target = pane.to_string();
        let args = copy_args
            .iter()
            .copied()
            .chain(["-t", target.as_str()])
            .collect::<Vec<_>>();
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("copy-mode", args),
            )
            .expect("enter copy mode");
        wait_for_view_mode(&terminal, view, "copy mode did not freeze", |mode| {
            matches!(mode, TerminalMode::Copy { .. })
        });
        let mode = terminal
            .latest_viewport_for(view)
            .expect("frozen viewport")
            .mode;
        let TerminalMode::Copy {
            position,
            total,
            hide_position,
        } = mode
        else {
            panic!("{name} did not publish copy mode");
        };
        assert_eq!(hide_position, hidden, "{name} published the wrong -H state");
        assert!(total > 0, "{name} lost its position denominator");
        assert!(
            position <= total,
            "{name} published a position past its total"
        );
    }
}

#[cfg(unix)]
#[test]
fn copy_mode_scroll_exit_reconciles_the_client_copy_session() {
    for (name, copy_args, exits) in [
        ("copy-scroll-exit", &["-e"][..], true),
        ("copy-scroll-stays", &[][..], false),
    ] {
        let (shared, client, mut context, pane, terminal, _mailbox) = copy_mode_fixture(
            name,
            "i=0; while [ $i -lt 12 ]; do printf 'line-%02d\\n' \"$i\"; i=$((i+1)); done",
        );
        let view = TerminalViewId(client.0);
        let target = pane.to_string();
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
                        "display-message pane-mode-changed",
                    ],
                ),
            )
            .expect("pane mode hook");
        let args = copy_args
            .iter()
            .copied()
            .chain(["-t", target.as_str()])
            .collect::<Vec<_>>();
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("copy-mode", args),
            )
            .expect("enter copy mode");
        wait_for_view_mode(&terminal, view, "copy mode did not freeze", |mode| {
            matches!(mode, TerminalMode::Copy { .. })
        });
        wait_for_observed_copy_session(&shared, client);
        shared.inner.lock().history.message_log.clear();
        let generation = terminal
            .latest_viewport_for(view)
            .expect("copy viewport")
            .view_generation;
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("send-keys", ["-t", &target, "-X", "page-down"]),
            )
            .expect("page down");
        if exits {
            wait_for_view_mode(
                &terminal,
                view,
                "scroll-exit did not leave copy mode",
                |mode| mode == TerminalMode::Live,
            );
            wait_for_root_key_table(
                &shared,
                client,
                "scroll-exit left the client on a copy table",
            );
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if shared
                    .inner
                    .lock()
                    .history
                    .message_log
                    .iter()
                    .any(|message| message.text == "pane-mode-changed")
                {
                    break;
                }
                assert!(Instant::now() < deadline, "scroll-exit hook did not fire");
                thread::sleep(Duration::from_millis(5));
            }
        } else {
            wait_for_viewport(
                &terminal,
                view,
                "plain copy mode did not publish page-down",
                |viewport| {
                    viewport.view_generation > generation
                        && matches!(viewport.mode, TerminalMode::Copy { .. })
                },
            );
            let inner = shared.inner.lock();
            assert!(
                inner
                    .client(client)
                    .is_some_and(|c| c.copy_session.is_some())
            );
            assert_eq!(
                inner
                    .client(client)
                    .and_then(|c| c.key_engine.as_ref())
                    .and_then(KeyEngine::active_table),
                Some("copy-mode")
            );
            assert!(inner.history.message_log.is_empty());
        }
    }
}

#[cfg(unix)]
#[test]
fn copy_mode_kill_does_not_survive_an_errored_entry() {
    let (shared, client, mut context, pane, terminal, _mailbox) = copy_mode_fixture(
        "kill-error",
        "i=0; while [ $i -lt 12 ]; do printf 'line-%02d\\n' \"$i\"; i=$((i+1)); done",
    );
    let target = pane.to_string();
    let session = context.session.expect("session");

    shared.detach(client);
    let mut errored = ExecutionContext::default();
    let denied = shared.execute(
        ClientId(u64::MAX),
        ClientKind::Command,
        &mut errored,
        &CommandInvocation::new("copy-mode", ["-k", "-t", &target]),
    );
    assert!(
        denied.is_err(),
        "copy-mode -k with no attached client should error"
    );
    assert!(
        shared.inner.lock().pending_copy_kill.is_none(),
        "an errored copy-mode -k left the kill armed"
    );

    shared
        .attach(client, session)
        .expect("attach after the error");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("copy-mode", ["-t", &target]),
        )
        .expect("plain copy-mode after the errored kill");
    wait_for_view_mode(
        &terminal,
        TerminalViewId(client.0),
        "copy mode did not freeze",
        |mode| matches!(mode, TerminalMode::Copy { .. }),
    );
    wait_for_observed_copy_session(&shared, client);
    assert!(
        !shared.inner.lock().clients[&client]
            .copy_session
            .as_ref()
            .unwrap()
            .kill,
        "a plain copy-mode inherited a kill from an earlier errored entry"
    );
}

#[cfg(unix)]
#[test]
fn copy_mode_source_does_not_survive_an_errored_entry() {
    let (shared, client, mut context, pane, _terminal, _mailbox) = copy_mode_fixture(
        "source-error",
        "i=0; while [ $i -lt 12 ]; do printf 'line-%02d\\n' \"$i\"; i=$((i+1)); done",
    );
    let target = pane.to_string();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-d", "-t", &target]),
        )
        .expect("split a source pane");
    let source = shared
        .inner
        .lock()
        .terminals
        .keys()
        .copied()
        .find(|candidate| *candidate != pane)
        .expect("source pane");
    let source_target = source.to_string();

    shared.detach(client);
    let mut errored = ExecutionContext::default();
    let denied = shared.execute(
        ClientId(u64::MAX),
        ClientKind::Command,
        &mut errored,
        &CommandInvocation::new("copy-mode", ["-s", &source_target, "-t", &target]),
    );
    assert!(
        denied.is_err(),
        "copy-mode -s with no attached client should error"
    );
    assert!(
        shared.inner.lock().pending_copy_source.is_none(),
        "an errored copy-mode -s left the source armed"
    );
}

#[cfg(unix)]
#[test]
fn send_keys_count_prefix_moves_once_with_the_armed_count() {
    let (shared, client, mut context, pane, terminal, _mailbox) = copy_mode_fixture(
        "copy-repeat-count",
        "i=0; while [ $i -lt 12 ]; do printf 'line-%02d\\n' \"$i\"; i=$((i+1)); done",
    );
    let view = TerminalViewId(client.0);
    let target = pane.to_string();
    for command in [
        CommandInvocation::new("set-window-option", ["mode-keys", "vi"]),
        CommandInvocation::new("copy-mode", ["-t", &target]),
    ] {
        shared
            .execute(client, ClientKind::Interactive, &mut context, &command)
            .expect("configure copy mode");
    }
    wait_for_view_mode(&terminal, view, "copy mode did not freeze", |mode| {
        matches!(mode, TerminalMode::Copy { .. })
    });
    wait_for_observed_copy_session(&shared, client);
    let start = match terminal
        .latest_viewport_for(view)
        .expect("copy viewport")
        .mode
    {
        TerminalMode::Copy { position, .. } => position,
        other => panic!("expected copy mode, got {other:?}"),
    };
    assert!(start > 3);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("send-keys", ["-t", &target, "-N", "3"]),
        )
        .expect("arm copy-mode count");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(KeyCode::Character('k'), Modifiers::default(), Some("k")),
                text_follows: false,
            },
        )
        .expect("counted movement key");
    wait_for_view_mode(&terminal, view, "counted movement did not land", |mode| {
        matches!(
            mode,
            TerminalMode::Copy { position, .. } if position == start - 3
        )
    });
}

#[cfg(unix)]
#[test]
fn a_left_drag_selects_live_and_never_enters_copy_mode() {
    let (shared, client, mut context, pane, terminal, _mailbox) = copy_mode_fixture(
        "drag-select",
        "printf '\\033[2J\\033[HZZDRAG one two\\r\\n'",
    );
    let view = TerminalViewId(client.0);
    wait_for_viewport(
        &terminal,
        view,
        "drag fixture never reached the viewport",
        |viewport| {
            viewport.mode == TerminalMode::Live
                && viewport_text(viewport).contains("ZZDRAG one two")
        },
    );

    for (column, phase) in [
        (1_u16, TerminalMousePhase::Press),
        (5, TerminalMousePhase::Motion),
        (9, TerminalMousePhase::Motion),
        (9, TerminalMousePhase::Release),
    ] {
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::TerminalView {
                    pane,
                    action: TerminalViewAction::Mouse(test_drag(column, phase)),
                },
            )
            .expect("drag gesture");
    }

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let selected = terminal
            .latest_viewport_for(view)
            .is_some_and(|viewport| !viewport.overlays.is_empty());
        if selected {
            break;
        }
        assert!(Instant::now() < deadline, "drag painted no selection");
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        terminal
            .latest_viewport_for(view)
            .is_some_and(|viewport| viewport.mode == TerminalMode::Live),
        "a drag must leave the pane live"
    );
    let inner = shared.inner.lock();
    assert_eq!(
        inner
            .client(client)
            .and_then(|c| c.key_engine.as_ref())
            .and_then(KeyEngine::active_table),
        None,
        "a drag must not move the client onto a copy-mode table"
    );
    assert!(
        inner
            .client(client)
            .is_none_or(|c| c.copy_session.is_none())
    );
}

#[cfg(unix)]
#[test]
fn clear_selection_or_cancel_clears_before_leaving_copy_mode() {
    let (shared, client, mut context, pane, terminal, _mailbox) =
        copy_mode_fixture("vi-escape", ":");
    let view = TerminalViewId(client.0);
    let target = pane.to_string();
    let run = |context: &mut ExecutionContext, command: CommandInvocation| {
        shared
            .execute(client, ClientKind::Interactive, context, &command)
            .expect("copy-mode command");
    };

    run(
        &mut context,
        CommandInvocation::new("set-window-option", ["mode-keys", "vi"]),
    );
    run(
        &mut context,
        CommandInvocation::new("copy-mode", ["-t", &target]),
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .client(client)
            .and_then(|c| c.key_engine.as_ref())
            .and_then(KeyEngine::active_table),
        Some("copy-mode-vi")
    );
    wait_for_view_mode(
        &terminal,
        view,
        "copy-mode command did not freeze",
        |mode| matches!(mode, TerminalMode::Copy { .. }),
    );
    wait_for_observed_copy_session(&shared, client);

    run(
        &mut context,
        CommandInvocation::new("send-keys", ["-t", &target, "-X", "begin-selection"]),
    );
    run(
        &mut context,
        CommandInvocation::new(
            "send-keys",
            ["-t", &target, "-X", "clear-selection-or-cancel"],
        ),
    );
    thread::sleep(Duration::from_millis(50));
    assert!(
        terminal
            .latest_viewport_for(view)
            .is_some_and(|viewport| matches!(viewport.mode, TerminalMode::Copy { .. })),
        "the first clear-or-cancel action must not leave copy mode"
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .client(client)
            .and_then(|c| c.key_engine.as_ref())
            .and_then(KeyEngine::active_table),
        Some("copy-mode-vi")
    );

    run(
        &mut context,
        CommandInvocation::new(
            "send-keys",
            ["-t", &target, "-X", "clear-selection-or-cancel"],
        ),
    );
    wait_for_view_mode(
        &terminal,
        view,
        "the second clear-or-cancel action did not exit",
        |mode| mode == TerminalMode::Live,
    );
    wait_for_root_key_table(
        &shared,
        client,
        "clear-or-cancel left the client on a copy table",
    );
}

#[cfg(unix)]
#[test]
fn configured_v_e_y_preserves_indentation_in_the_clipboard() {
    let (shared, client, mut context, pane, terminal, mailbox) =
        copy_mode_fixture("vi-v-e-y", "printf '\\033[2J\\033[H    alpha beta\\n'");
    let view = TerminalViewId(client.0);
    let target = pane.to_string();
    for command in [
        CommandInvocation::new("set-window-option", ["mode-keys", "vi"]),
        CommandInvocation::new("set-option", ["-g", "set-clipboard", "on"]),
        CommandInvocation::new(
            "bind-key",
            [
                "-T",
                "copy-mode-vi",
                "v",
                "send-keys",
                "-X",
                "begin-selection",
            ],
        ),
        CommandInvocation::new(
            "bind-key",
            [
                "-T",
                "copy-mode-vi",
                "y",
                "send-keys",
                "-X",
                "copy-selection-and-cancel",
            ],
        ),
    ] {
        shared
            .execute(client, ClientKind::Interactive, &mut context, &command)
            .expect("configure vi copy mode");
    }

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if terminal
            .capture(CaptureOptions::default())
            .is_ok_and(|text| text.lines().any(|line| line == "    alpha beta"))
        {
            break;
        }
        assert!(Instant::now() < deadline, "terminal never printed fixture");
        thread::sleep(Duration::from_millis(10));
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("copy-mode", ["-t", &target]),
        )
        .expect("enter copy mode");
    wait_for_view_mode(&terminal, view, "copy mode did not freeze", |mode| {
        matches!(mode, TerminalMode::Copy { .. })
    });
    wait_for_observed_copy_session(&shared, client);
    take_reliable_messages(&mailbox);

    for input in [
        test_key(KeyCode::Character('k'), Modifiers::default(), Some("k")),
        test_key(KeyCode::Character('0'), Modifiers::default(), Some("0")),
        test_key(KeyCode::Character('v'), Modifiers::default(), Some("v")),
        test_key(
            KeyCode::Character('e'),
            Modifiers::new(true, false, false, false),
            Some("E"),
        ),
    ] {
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::Key {
                    pane,
                    input,
                    text_follows: false,
                },
            )
            .expect("vi copy key");
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if terminal.latest_viewport_for(view).is_some_and(|viewport| {
            viewport.overlays.iter().any(|overlay| {
                overlay.kind() == zz_terminal::OverlayKind::Selection
                    && overlay.row == 0
                    && overlay.start == 0
                    && overlay.end == 9
            })
        }) {
            break;
        }
        assert!(Instant::now() < deadline, "v E painted no selection");
        thread::sleep(Duration::from_millis(10));
    }
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(KeyCode::Character('y'), Modifiers::default(), Some("y")),
                text_follows: false,
            },
        )
        .expect("vi copy yank key");

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut observed = Vec::new();
    loop {
        let writes = take_clipboard_writes(&mailbox, pane);
        observed.extend(writes.iter().cloned());
        if writes
            .iter()
            .any(|(target, text)| *target == ClipboardTarget::Clipboard && text == "    alpha")
        {
            break;
        }
        if Instant::now() >= deadline {
            let mode = terminal
                .latest_viewport_for(view)
                .map(|viewport| viewport.mode);
            let (table, buffers) = {
                let inner = shared.inner.lock();
                (
                    inner
                        .client(client)
                        .and_then(|c| c.key_engine.as_ref())
                        .and_then(KeyEngine::active_table)
                        .map(str::to_owned),
                    inner
                        .paste_buffers
                        .iter()
                        .map(|buffer| String::from_utf8_lossy(&buffer.data).into_owned())
                        .collect::<Vec<_>>(),
                )
            };
            panic!(
                "v E y never copied the indented word: {observed:?}; mode={mode:?}; table={table:?}; buffers={buffers:?}"
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
#[test]
fn focusing_another_pane_ends_the_copy_session() {
    let (shared, client, mut context, first, terminal, _mailbox) =
        copy_mode_fixture("copy-focus", ":");
    let view = TerminalViewId(client.0);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-h"]),
        )
        .expect("split pane");
    let second = context.pane.expect("second pane");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("select-pane", ["-t", &first.to_string()]),
        )
        .expect("focus the first pane");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("copy-mode", ["-t", &first.to_string()]),
        )
        .expect("enter copy mode");
    wait_for_view_mode(&terminal, view, "copy-mode did not freeze", |mode| {
        matches!(mode, TerminalMode::Copy { .. })
    });

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("select-pane", ["-t", &second.to_string()]),
        )
        .expect("focus the second pane");

    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.copy_session.is_none()),
            "focusing away must end the copy session"
        );
        assert_eq!(
            inner
                .client(client)
                .and_then(|c| c.key_engine.as_ref())
                .and_then(KeyEngine::active_table),
            None
        );
    }
    wait_for_view_mode(
        &terminal,
        view,
        "the unfocused pane stayed frozen",
        |mode| mode == TerminalMode::Live,
    );
}

#[cfg(unix)]
#[test]
fn a_copy_session_that_diverges_from_its_pane_heals_on_the_next_publish() {
    let (shared, client, mut context, pane, terminal, _mailbox) =
        copy_mode_fixture("copy-reconcile", ":");
    let view = TerminalViewId(client.0);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("copy-mode", ["-t", &pane.to_string()]),
        )
        .expect("enter copy mode");
    wait_for_view_mode(&terminal, view, "copy-mode did not freeze", |mode| {
        matches!(mode, TerminalMode::Copy { .. })
    });
    wait_for_observed_copy_session(&shared, client);
    terminal.view_action(view, TerminalViewAction::ClearHistory);
    wait_for_root_key_table(
        &shared,
        client,
        "a live pane left the client stranded on a copy table",
    );

    terminal.view_action(view, TerminalViewAction::EnterCopyMode);
    thread::sleep(Duration::from_millis(200));
    wait_for_view_mode(
        &terminal,
        view,
        "an unclaimed frozen pane was left frozen",
        |mode| mode == TerminalMode::Live,
    );
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.copy_session.is_none())));
}

#[cfg(unix)]
#[test]
fn history_request_is_guarded_clamped_and_returns_self_contained_rows() {
    let shared = Arc::new(Shared::new(1));
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (unattached, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "history-request"]),
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
        .expect("split pane");
    let second = context.pane.expect("second pane");
    shared.attach(client, session).expect("attach session");

    let terminal = Arc::clone(&shared.inner.lock().terminals[&first]);
    terminal.resize(16, 4, 8, 18);
    wait_for_terminal_dimensions(&terminal, TerminalViewId(client.0), 16, 4);
    terminal.send_text(
        "i=0; while [ $i -lt 530 ]; do printf 'ZZH%03d\\r\\n' \"$i\"; i=$((i+1)); done; printf 'ZZ_HISTORY_DONE\\r\\n'\n",
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ready = terminal
            .latest_viewport_for(TerminalViewId(client.0))
            .is_some_and(|viewport| {
                viewport
                    .scrollbar
                    .total
                    .saturating_sub(viewport.scrollbar.len)
                    >= 512
            });
        if ready {
            break;
        }
        assert!(Instant::now() < deadline, "terminal history did not fill");
        thread::sleep(Duration::from_millis(10));
    }

    let replies = OutboundMailbox::new();
    shared.send_history(unattached, first, 0, 10, &replies);
    assert!(take_reliable_messages(&replies).is_empty());

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("resize-pane", ["-Z", "-t", &first.to_string()]),
        )
        .expect("zoom first pane");
    shared.send_history(client, second, 0, 10, &replies);
    assert!(take_reliable_messages(&replies).is_empty());

    shared.send_history(client, first, 0, u32::MAX, &replies);
    let messages = take_reliable_messages(&replies);
    let chunk = messages.into_iter().find_map(|message| match message {
        ProtocolMessage::Event(Event {
            payload:
                EventPayload::HistoryChunk {
                    pane,
                    start,
                    total,
                    offset,
                    columns,
                    rows,
                    dictionary,
                },
            ..
        }) => Some((pane, start, total, offset, columns, rows, dictionary)),
        _ => None,
    });
    let (pane, start, total, offset, columns, rows, dictionary) = chunk.expect("history chunk");
    assert_eq!(pane, first);
    assert_eq!(start, 0);
    assert_eq!(columns, 16);
    assert_eq!(rows.len(), usize::try_from(MAX_HISTORY_CHUNK_ROWS).unwrap());
    assert!(offset <= total);
    assert!(rows.iter().all(|row| row.len() == usize::from(columns)));
    assert!(history_chunk_text(&rows, &dictionary).contains("ZZH"));
}
