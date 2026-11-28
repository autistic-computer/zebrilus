use super::*;

#[test]
fn repeated_terminal_input_stops_at_the_first_rejection() {
    let mut attempts = 0;
    repeat_until_rejected(u32::MAX, || {
        attempts += 1;
        attempts < 3
    });
    assert_eq!(attempts, 3);
}

#[test]
fn repeated_browser_input_is_capped_before_it_reaches_the_client() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "browser-repeat-cap", &[client]);
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("terminal context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "browser", "about:blank"]),
        )
        .expect("create browser");
    let browser = context.pane.expect("browser pane");
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "send-keys",
                ["-t", &browser.to_string(), "-l", "-N", "4294967295", "x"],
            ),
        )
        .expect("send repeated browser input");

    let commands = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::BrowserCommand { pane, command },
                ..
            }) => Some((pane, command)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        commands,
        [(
            browser,
            BrowserCommand::SendKeysRepeated {
                keys: vec![zz_protocol::KeyToken::Literal("x".to_owned())],
                count: MAX_BROWSER_KEY_REPEAT,
            }
        )]
    );
}

#[test]
fn suspend_client_leaves_control_and_nonterminal_clients_attached() {
    for kind in [ClientKind::Control, ClientKind::Interactive] {
        let shared = Arc::new(Shared::new(1));
        let (session, _, _) = switch_test_session(&shared, "suspend");
        let mailbox = OutboundMailbox::new();
        let (client, _) =
            shared.register_subscribed(kind, Some("without-tty".to_owned()), None, mailbox);
        shared.attach(client, session).unwrap();
        let mut context = ExecutionContext::default();
        shared
            .execute(
                client,
                kind,
                &mut context,
                &CommandInvocation::new("suspend-client", std::iter::empty::<&str>()),
            )
            .unwrap();
        let inner = shared.inner.lock();
        assert_eq!(inner.client(client).map(|c| c.suspended), Some(false));
        assert_eq!(
            inner.attachment.attached[&session]
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [client]
        );
    }
}

#[test]
fn terminal_mouse_is_rejected_only_for_sized_clients_when_mouse_is_off() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, pane) = switch_test_session(&shared, "A");
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("tui".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    shared.attach(client, a).expect("attach");
    let wheel_for = |pane: PaneId| InputMessage::TerminalView {
        pane,
        action: TerminalViewAction::ScrollWheel {
            lines: 3,
            input: TerminalMouseInput::new(
                TerminalMousePhase::Press,
                Some(TerminalMouseButton::ScrollDown),
                PointerCellEvent {
                    column: 1,
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
            ),
        },
    };
    let wheel = wheel_for(pane);
    {
        let inner = shared.inner.lock();
        assert!(!terminal_mouse_rejected(&inner, client, &wheel));
    }
    shared
        .inner
        .lock()
        .client_entry(client)
        .size
        .replace((80, 24));
    {
        let inner = shared.inner.lock();
        assert!(!terminal_mouse_rejected(&inner, client, &wheel));
    }
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "mouse", "off"]),
        )
        .expect("turn mouse off");
    {
        let inner = shared.inner.lock();
        assert!(terminal_mouse_rejected(&inner, client, &wheel));
        let focus = InputMessage::TerminalView {
            pane,
            action: TerminalViewAction::Focus(true),
        };
        assert!(!terminal_mouse_rejected(&inner, client, &focus));
    }
    let rejected_sort = {
        let mut inner = shared.inner.lock();
        inner.engine.state.session_mut(a).unwrap().activity = Some(11);
        inner.engine.state.sessions[&a].sort_activity
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            wheel_for(pane),
        )
        .expect("reject mouse input after recording activity");
    {
        let inner = shared.inner.lock();
        assert!(inner.engine.state.sessions[&a].sort_activity > rejected_sort);
        assert_ne!(inner.engine.state.sessions[&a].activity, Some(11));
    }
    shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.size.take());
    {
        let inner = shared.inner.lock();
        assert!(!terminal_mouse_rejected(&inner, client, &wheel));
    }

    shared
        .inner
        .lock()
        .client_entry(client)
        .size
        .replace((80, 24));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "tracked"]),
        )
        .expect("real session");
    let tracked_pane = context.pane.expect("real pane");
    {
        let inner = shared.inner.lock();
        assert!(
            terminal_mouse_rejected(&inner, client, &wheel_for(tracked_pane)),
            "a live pane without app mouse tracking still rejects"
        );
    }
    #[cfg(unix)]
    {
        let terminal = Arc::clone(&shared.inner.lock().terminals[&tracked_pane]);
        terminal.send_text("printf '\\033[?1002hMOUSEON\\n'\n");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !terminal.latest_viewport().mouse_tracking {
            assert!(
                Instant::now() < deadline,
                "pane never reported mouse tracking"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let inner = shared.inner.lock();
        assert!(
            !terminal_mouse_rejected(&inner, client, &wheel_for(tracked_pane)),
            "an app-requested mouse pane accepts forwarded events while the option is off"
        );
    }
}

#[test]
fn pane_input_resolves_key_tables_from_every_pane_kind() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    for command in [
        CommandInvocation::new("set-option", ["-g", "prefix", "C-a"]),
        CommandInvocation::new("bind-key", ["-n", "C-h", "focus-sidebar"]),
        CommandInvocation::new("bind-key", ["-r", "s", "focus-sidebar"]),
    ] {
        shared
            .execute(client, ClientKind::Interactive, &mut context, &command)
            .expect("configure key tables");
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "routes"]),
        )
        .expect("session");
    let session = context.session.expect("session");
    let terminal = context.pane.expect("terminal");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "split-window",
                ["--kind", "browser", "-h", "https://example.com"],
            ),
        )
        .expect("browser");
    let browser = context.pane.expect("browser");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "picker", "-v"]),
        )
        .expect("picker");
    let picker = context.pane.expect("picker");
    shared.attach(client, session).expect("attach");
    take_reliable_messages(&mailbox);

    let browser_key = |message: &ProtocolMessage| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::BrowserCommand {
                    command: BrowserCommand::Key(_),
                    ..
                },
                ..
            })
        )
    };
    let prefix_armed = |message: &ProtocolMessage, wanted: bool| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::PrefixArmed { armed },
                ..
            }) if *armed == wanted
        )
    };
    let focus_sidebar = |message: &ProtocolMessage| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::FocusSidebar,
                ..
            })
        )
    };

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: browser,
                input: test_key(
                    KeyCode::Character('a'),
                    Modifiers::new(false, true, false, false),
                    None,
                ),
                text_follows: false,
            },
        )
        .expect("browser prefix");
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(|message| prefix_armed(message, true)));
    assert!(!messages.iter().any(browser_key));

    let mut unrelated_surface_release = test_key(
        KeyCode::Character('a'),
        Modifiers::new(false, true, false, false),
        None,
    );
    unrelated_surface_release.action = KeyAction::Release;
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceKey {
                pane: browser,
                input: unrelated_surface_release,
                text_follows: false,
            },
        )
        .expect("unrelated browser surface release");
    assert!(take_reliable_messages(&mailbox).iter().any(browser_key));
    assert!(
        shared.inner.lock().clients[&client]
            .swallowed_keys
            .as_ref()
            .unwrap()
            .contains("a")
    );

    let mut prefix_release = test_key(
        KeyCode::Character('a'),
        Modifiers::new(false, true, false, false),
        None,
    );
    prefix_release.action = KeyAction::Release;
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: browser,
                input: prefix_release,
                text_follows: false,
            },
        )
        .expect("browser prefix release");
    assert!(!take_reliable_messages(&mailbox).iter().any(browser_key));
    assert!(
        shared.inner.lock().clients[&client]
            .swallowed_keys
            .as_ref()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("prefix")
    );

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: browser,
                input: test_key(KeyCode::Character('s'), Modifiers::default(), Some("s")),
                text_follows: true,
            },
        )
        .expect("browser prefix-table binding");
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(focus_sidebar));
    assert!(!messages.iter().any(browser_key));
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("prefix")
    );

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: browser,
                input: test_key(KeyCode::Character('Z'), Modifiers::default(), Some("Z")),
                text_follows: false,
            },
        )
        .expect("unbound key after the repeat window retries root");
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(browser_key));
    assert!(messages.iter().any(|message| prefix_armed(message, false)));

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceKey {
                pane: browser,
                input: test_key(KeyCode::Character('y'), Modifiers::default(), Some("y")),
                text_follows: false,
            },
        )
        .expect("plain browser key passes to the page");
    assert!(take_reliable_messages(&mailbox).iter().any(browser_key));

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceKey {
                pane: browser,
                input: test_key(
                    KeyCode::Character('h'),
                    Modifiers::new(false, true, false, false),
                    None,
                ),
                text_follows: false,
            },
        )
        .expect("browser surface key");
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(browser_key));
    assert!(!messages.iter().any(focus_sidebar));

    let mut surface_release = test_key(
        KeyCode::Character('h'),
        Modifiers::new(false, true, false, false),
        None,
    );
    surface_release.action = KeyAction::Release;
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceKey {
                pane: browser,
                input: surface_release,
                text_follows: false,
            },
        )
        .expect("browser surface key release");
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(browser_key));
    assert!(!messages.iter().any(focus_sidebar));

    for (input, text_follows) in [
        (
            test_key(
                KeyCode::Character('a'),
                Modifiers::new(false, true, false, false),
                None,
            ),
            false,
        ),
        (
            test_key(KeyCode::Character('s'), Modifiers::default(), Some("s")),
            true,
        ),
        (
            test_key(KeyCode::Character('Z'), Modifiers::default(), Some("Z")),
            false,
        ),
    ] {
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::Key {
                    pane: picker,
                    input,
                    text_follows,
                },
            )
            .expect("picker-source key");
    }
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(focus_sidebar));
    assert!(!messages.iter().any(browser_key));
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
            &CommandInvocation::new("bind-key", ["h", "select-pane", "-L"]),
        )
        .expect("bind h to select-pane -L");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "split-window",
                ["--kind", "picker", "-h", "-t", &terminal.to_string()],
            ),
        )
        .expect("editor picker beside the terminal");
    let editor = context.pane.expect("editor picker");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "experimental-editor-pane", "on"]),
        )
        .expect("enable editor panes");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("select-pane-kind", ["-t", &editor.to_string(), "editor"]),
        )
        .expect("editor");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("select-pane", ["-t", &editor.to_string()]),
        )
        .expect("focus the editor");
    let window = context.window.expect("window");
    assert_eq!(
        shared.inner.lock().engine.state.windows[&window].active_pane,
        editor
    );
    take_reliable_messages(&mailbox);

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: editor,
                input: test_key(
                    KeyCode::Character('a'),
                    Modifiers::new(false, true, false, false),
                    None,
                ),
                text_follows: false,
            },
        )
        .expect("editor prefix");
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(|message| prefix_armed(message, true)));
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("prefix")
    );

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: editor,
                input: test_key(KeyCode::Character('h'), Modifiers::default(), Some("h")),
                text_follows: true,
            },
        )
        .expect("editor prefix-table select-pane -L");
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(|message| prefix_armed(message, false)));
    assert_eq!(
        shared.inner.lock().engine.state.windows[&window].active_pane,
        terminal,
        "prefix+h from the editor must focus the terminal to its left"
    );
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        None
    );

    let prefix = test_key(
        KeyCode::Character('a'),
        Modifiers::new(false, true, false, false),
        None,
    );
    let binding = test_key(KeyCode::Character('s'), Modifiers::default(), Some("s"));

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: terminal,
                input: prefix,
                text_follows: false,
            },
        )
        .expect("normal prefix");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("prefix")
    );

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: terminal,
                input: binding,
                text_follows: true,
            },
        )
        .expect("prefix-table binding");
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::FocusSidebar,
                    ..
                })
            ))
    );
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        Some("prefix")
    );

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: terminal,
                input: test_key(KeyCode::Character('Z'), Modifiers::default(), Some("Z")),
                text_follows: false,
            },
        )
        .expect("unbound prefix-table key is discarded");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        None
    );

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: terminal,
                input: test_key(
                    KeyCode::Character('h'),
                    Modifiers::new(false, true, false, false),
                    None,
                ),
                text_follows: false,
            },
        )
        .expect("root binding");
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::FocusSidebar,
                    ..
                })
            ))
    );

    let error = shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceKey {
                pane: terminal,
                input: test_key(KeyCode::Character('x'), Modifiers::default(), Some("x")),
                text_follows: true,
            },
        )
        .expect_err("terminal panes cannot claim the browser surface route");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidTarget(message))
            if message == format!("{terminal} is not a browser pane")
    ));
}

#[test]
fn synchronized_input_fans_out_to_terminal_and_browser_panes() {
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
            &CommandInvocation::new(
                "new-session",
                ["-s", "sync-input", terminal_line_echo_command()],
            ),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let first = context.pane.expect("first terminal");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-h", terminal_line_echo_command()]),
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
                ["--kind", "browser", "-h", "https://example.com"],
            ),
        )
        .expect("browser pane");
    let browser = context.pane.expect("browser pane");
    shared.attach(client, session).expect("attach session");
    let (first_terminal, second_terminal) = {
        let inner = shared.inner.lock();
        (
            Arc::clone(&inner.terminals[&first]),
            Arc::clone(&inner.terminals[&second]),
        )
    };
    for terminal in [&first_terminal, &second_terminal] {
        wait_for_viewport(
            terminal,
            TerminalViewId(client.0),
            "synchronized-input terminal never became ready",
            |viewport| viewport_text(viewport).contains("zz-terminal-ready"),
        );
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-option", ["-w", "synchronize-panes", "on"]),
        )
        .expect("enable synchronized input");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("bind-key", ["-n", "Any", "focus-sidebar"]),
        )
        .expect("install a root catch-all that browser surface input must bypass");
    take_reliable_messages(&mailbox);

    let marker = "ZZ_SYNC_SEND_KEYS";
    let send_keys = vec![
        zz_protocol::KeyToken::Literal(format!("echo {marker}")),
        zz_protocol::KeyToken::Named("Enter".to_owned()),
    ];
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "send-keys",
                ["-t", &first.to_string(), &format!("echo {marker}"), "Enter"],
            ),
        )
        .expect("synchronized send-keys");
    let browser_messages = take_reliable_messages(&mailbox);
    assert!(browser_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::BrowserCommand {
                pane,
                command: BrowserCommand::SendKeys(keys),
            },
            ..
        }) if *pane == browser && keys == &send_keys
    )));

    for pane in [first, second] {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let captured = shared
                .execute(
                    client,
                    ClientKind::Command,
                    &mut context,
                    &CommandInvocation::new("capture-pane", ["-p", "-t", &pane.to_string()]),
                )
                .expect("capture synchronized terminal")
                .output;
            if captured.contains(marker) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{pane} did not receive synchronized send-keys"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    take_reliable_messages(&mailbox);
    let key = KeyInput {
        action: KeyAction::Press,
        key: KeyCode::Enter,
        modifiers: Modifiers::default(),
        text: None,
        unshifted_codepoint: None,
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceText {
                pane: browser,
                text: "xλz".to_owned(),
            },
        )
        .expect("synchronized text");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceKey {
                pane: browser,
                input: key.clone(),
                text_follows: false,
            },
        )
        .expect("synchronized key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::TerminalView {
                pane: first,
                action: zz_terminal::TerminalViewAction::Paste("pasted".to_owned()),
            },
        )
        .expect("synchronized paste");

    let messages = take_reliable_messages(&mailbox);
    assert!(!messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::FocusSidebar,
            ..
        })
    )));
    let commands = messages
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::BrowserCommand { pane, command },
                ..
            }) if pane == browser => Some(command),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        commands,
        vec![
            BrowserCommand::SendKeys(vec![zz_protocol::KeyToken::Literal("xλz".to_owned())]),
            BrowserCommand::Key(key),
            BrowserCommand::SendKeys(vec![zz_protocol::KeyToken::Literal("pasted".to_owned())]),
        ]
    );

    take_reliable_messages(&mailbox);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-buffer", ["buffer paste"]),
        )
        .expect("set paste buffer");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("paste-buffer", ["-t", &first.to_string()]),
        )
        .expect("synchronized paste-buffer");
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::BrowserCommand {
                        pane,
                        command: BrowserCommand::SendKeys(keys),
                    },
                    ..
                }) if *pane == browser
                    && keys == &[zz_protocol::KeyToken::Literal("buffer paste".to_owned())]
            ))
    );
}

#[test]
fn pane_input_off_gates_direct_synchronized_send_and_paste_input() {
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
            &CommandInvocation::new("new-session", ["-s", "input-off"]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let terminal = context.pane.expect("terminal");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "split-window",
                ["--kind", "browser", "-h", "https://example.com"],
            ),
        )
        .expect("browser pane");
    let browser = context.pane.expect("browser");
    shared.attach(client, session).expect("attach session");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-option", ["-w", "synchronize-panes", "on"]),
        )
        .expect("enable synchronized input");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("select-pane", ["-t", &terminal.to_string()]),
        )
        .expect("select terminal");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("last-pane", ["-d"]),
        )
        .expect("disable browser input");

    {
        let inner = shared.inner.lock();
        assert!(resolve_input_sinks(&inner, browser).unwrap().is_empty());
        let sinks = resolve_input_sinks(&inner, terminal).unwrap();
        assert_eq!(sinks.len(), 1);
        assert!(matches!(sinks[0], PaneSink::Terminal(_)));
    }

    take_reliable_messages(&mailbox);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("send-keys", ["-t", &browser.to_string(), "blocked"]),
        )
        .expect("send keys to disabled browser");
    assert!(
        !take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::BrowserCommand { .. },
                    ..
                })
            ))
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-buffer", ["blocked paste"]),
        )
        .expect("set buffer");
    take_reliable_messages(&mailbox);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("paste-buffer", ["-t", &browser.to_string()]),
        )
        .expect("paste to disabled browser");
    assert!(
        !take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::BrowserCommand { .. },
                    ..
                })
            ))
    );

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceText {
                pane: browser,
                text: "blocked direct input".to_owned(),
            },
        )
        .expect("direct input to disabled browser");
    assert!(
        !take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::BrowserCommand { .. },
                    ..
                })
            ))
    );
}
