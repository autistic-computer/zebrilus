use super::*;

#[test]
fn an_attached_waiter_sees_each_payload_while_it_waits() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (session, pane, _) = attached_message_fixture(&shared, "event-view", &[client]);
    shared.inner.lock().client_entry(client).instance_id = Some(ClientInstanceId(client.0));
    let context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    let waiter = {
        let shared = Arc::clone(&shared);
        let mut context = context.clone();
        std::thread::spawn(move || {
            shared.execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new(
                    "wait-for",
                    [
                        "-E",
                        "-v",
                        "-F",
                        "#{||:#{pane_id},#{==:#{new_name},stop}}",
                        "session-renamed",
                    ],
                ),
            )
        })
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while shared.inner.lock().event_waiters.is_empty() {
        assert!(!waiter.is_finished(), "the wait returned without parking");
        assert!(Instant::now() < deadline, "the wait never parked");
        std::thread::sleep(Duration::from_millis(5));
    }
    let rename = |name: &str| {
        shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("rename-session", ["-t", &session.to_string(), name]),
            )
            .expect("rename-session");
    };
    rename("first");
    assert!(
        !waiter.is_finished(),
        "#{{pane_id}} has no target in the filter"
    );
    assert_eq!(shared.inner.lock().event_waiters.len(), 1);
    assert!(shared.read_client(client, |c| c.is_some_and(|c| c.command_output.is_some())));
    rename("stop");
    waiter.join().expect("waiter thread").expect("wait-for -E");
    assert!(shared.inner.lock().event_waiters.is_empty());
}

#[test]
fn wait_for_wakes_on_an_agent_state_event() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "agent-event", &[client]);
    shared.inner.lock().client_entry(client).instance_id = Some(ClientInstanceId(client.0));
    let wait = |target: String| {
        let shared = Arc::clone(&shared);
        std::thread::spawn(move || {
            shared.execute(
                client,
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(
                    "wait-for",
                    [
                        "-E",
                        "-F",
                        &format!("#{{==:#{{pane}},{target}}}"),
                        "agent-state-changed",
                    ],
                ),
            )
        })
    };
    let other = wait("%999".to_owned());
    let waiter = wait(pane.to_string());
    let deadline = Instant::now() + Duration::from_secs(10);
    while shared.inner.lock().event_waiters.len() < 2 {
        assert!(!waiter.is_finished(), "the wait returned without parking");
        assert!(Instant::now() < deadline, "the wait never parked");
        std::thread::sleep(Duration::from_millis(5));
    }
    shared.write_pane_agent_state(pane, "working");
    while !waiter.is_finished() {
        assert!(
            Instant::now() < deadline,
            "the state change never woke the wait"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    waiter
        .join()
        .expect("waiter thread")
        .expect("wait-for -E agent-state-changed");
    assert!(!other.is_finished(), "another pane's wait stays parked");
    assert_eq!(shared.inner.lock().event_waiters.len(), 1);
    let payload = hook_events::event_payload(
        "agent-state-changed",
        &BTreeMap::from([
            ("hook_pane".to_owned(), pane.to_string()),
            ("agent_state".to_owned(), "working".to_owned()),
        ]),
    );
    assert_eq!(
        payload.get("agent_state").map(String::as_str),
        Some("working")
    );
    assert_eq!(payload.get("pane"), Some(&pane.to_string()));
}

#[test]
fn pane_prompts_fire_the_prompt_events_with_their_type() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "pane-prompt-events", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    let run = |context: &mut ExecutionContext, args: &[&str]| {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                context,
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .expect(args[0]);
    };
    let log = |shared: &Arc<Shared>| {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("show-options", ["-gqv", "@prompts"]),
            )
            .expect("show-options")
            .output
            .to_string()
            .trim_end()
            .to_owned()
    };
    let escape = |context: &mut ExecutionContext| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::Key {
                    pane,
                    input: test_key(KeyCode::Escape, Modifiers::default(), None),
                    text_follows: false,
                },
            )
            .expect("escape");
    };
    run(&mut context, &["set", "-g", "@prompts", "-"]);
    for (hook, tag) in [("pane-prompt-opened", "O"), ("pane-prompt-closed", "C")] {
        run(
            &mut context,
            &[
                "set-hook",
                "-g",
                hook,
                &format!(
                    "set -gaF @prompts '{tag}#{{hook_pane}}#{{hook_prompt_type}}#{{hook_event}}#{{hook_session}}+'"
                ),
            ],
        );
    }
    let pane_id = pane.to_string();
    run(
        &mut context,
        &[
            "command-prompt",
            "-P",
            "-T",
            "search",
            "-p",
            "find",
            "display-message %%",
        ],
    );
    assert_eq!(
        log(&shared),
        format!("-O{pane_id}searchpane-prompt-opened+")
    );
    escape(&mut context);
    assert!(prompt_state(&shared, client).is_none());
    assert_eq!(
        log(&shared),
        format!("-O{pane_id}searchpane-prompt-opened+C{pane_id}searchpane-prompt-closed+")
    );
    run(&mut context, &["set", "-g", "@prompts", "-"]);
    run(
        &mut context,
        &["command-prompt", "-P", "-p", "one", "display-message %%"],
    );
    run(
        &mut context,
        &["command-prompt", "-P", "-p", "two", "display-message %%"],
    );
    assert_eq!(
        log(&shared),
        format!("-O{pane_id}commandpane-prompt-opened+")
    );
    escape(&mut context);
    assert_eq!(
        log(&shared),
        format!("-O{pane_id}commandpane-prompt-opened+C{pane_id}commandpane-prompt-closed+")
    );
    run(&mut context, &["set", "-g", "@prompts", "-"]);
    run(
        &mut context,
        &["command-prompt", "-p", "status", "display-message %%"],
    );
    escape(&mut context);
    assert_eq!(log(&shared), "-");
}

#[test]
fn prompt_modes_read_keys_the_way_the_pin_does() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "prompt-modes", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");

    let press = |shared: &Arc<Shared>, context: &mut ExecutionContext, input: KeyInput| {
        let text_follows =
            matches!(input.key, KeyCode::Character(_)) && input.modifiers == Modifiers::default();
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::Key {
                    pane,
                    input,
                    text_follows,
                },
            )
            .expect("prompt key");
    };

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-1", "-p", "one", "rename-window %%"]),
        )
        .expect("single prompt");
    assert_eq!(
        prompt_state(&shared, client).expect("raised").mode,
        CommandPromptMode::Single
    );
    press(&shared, &mut context, chooser_key_press("z"));
    assert!(prompt_state(&shared, client).is_none());
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.expect("window")].name,
        "z"
    );
    assert_eq!(
        (*shared.inner.lock().clients[&client]
            .suppressed_text
            .as_ref()
            .unwrap()),
        BTreeMap::from([('z', 1)])
    );
    shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.suppressed_text.take());

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-k", "-p", "key", "rename-window %%"]),
        )
        .expect("key prompt");
    press(
        &shared,
        &mut context,
        test_key(
            KeyCode::Character('g'),
            Modifiers::new(false, true, false, false),
            None,
        ),
    );
    assert!(prompt_state(&shared, client).is_none());
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.expect("window")].name,
        "C-g"
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-k", "-p", "key", "rename-window %%"]),
        )
        .expect("key prompt");
    press(
        &shared,
        &mut context,
        test_key(
            KeyCode::Character('z'),
            Modifiers::new(false, false, false, true),
            None,
        ),
    );
    assert!(prompt_state(&shared, client).is_some());
    press(
        &shared,
        &mut context,
        test_key(KeyCode::Escape, Modifiers::default(), None),
    );
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.expect("window")].name,
        "Escape"
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "command-prompt",
                ["-1", "-I", "abc", "-p", "one", "rename-window %%"],
            ),
        )
        .expect("seeded single prompt");
    press(&shared, &mut context, chooser_key_press("z"));
    assert!(prompt_state(&shared, client).is_none());
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.expect("window")].name,
        "Escape",
        "a seeded -1 buffer closes without submitting anything"
    );
    shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.suppressed_text.take());

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("bind-key", ["-n", "z", "rename-window", "passed-through"]),
        )
        .expect("bind the pass-through key");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-N", "-p", "num", "select-window -t %%"]),
        )
        .expect("numeric prompt");
    press(&shared, &mut context, chooser_key_press("1"));
    press(&shared, &mut context, chooser_key_press("2"));
    assert_eq!(
        prompt_state(&shared, client).expect("still open").input,
        "12"
    );
    assert_eq!(
        (*shared.inner.lock().clients[&client]
            .suppressed_text
            .as_ref()
            .unwrap()),
        BTreeMap::from([('1', 1), ('2', 1)])
    );
    shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.suppressed_text.take());
    press(&shared, &mut context, chooser_key_press("z"));
    assert!(prompt_state(&shared, client).is_none());
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.expect("window")].name,
        "passed-through",
        "the first non-digit submits the prompt and still runs its own binding"
    );
}

#[test]
fn a_backspace_exit_prompt_ends_on_an_empty_backspace() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "bspace-exit", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    let backspace = |shared: &Arc<Shared>, context: &mut ExecutionContext| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::Key {
                    pane,
                    input: test_key(KeyCode::Backspace, Modifiers::default(), None),
                    text_follows: false,
                },
            )
            .expect("backspace");
    };

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-p", "plain"]),
        )
        .expect("plain prompt");
    backspace(&shared, &mut context);
    assert!(prompt_state(&shared, client).is_some());
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::CommandPrompt {
                action: CommandPromptAction::Close,
            },
        )
        .expect("close plain prompt");

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-e", "-I", "ab", "-p", "exit"]),
        )
        .expect("backspace-exit prompt");
    backspace(&shared, &mut context);
    backspace(&shared, &mut context);
    assert_eq!(prompt_state(&shared, client).expect("still open").input, "");
    backspace(&shared, &mut context);
    assert!(prompt_state(&shared, client).is_none());
}

#[test]
fn command_and_search_prompt_histories_never_mix() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "prompt-history", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    let submit =
        |shared: &Arc<Shared>, context: &mut ExecutionContext, args: &[&str], value: &str| {
            shared
                .execute(
                    client,
                    ClientKind::Interactive,
                    context,
                    &CommandInvocation::new("command-prompt", args.iter().copied()),
                )
                .expect("prompt");
            shared
                .input(
                    client,
                    ClientKind::Interactive,
                    context,
                    InputMessage::CommandPrompt {
                        action: CommandPromptAction::Submit {
                            input: value.to_owned(),
                        },
                    },
                )
                .expect("submit");
        };

    submit(
        &shared,
        &mut context,
        &["-T", "search", "list-panes"],
        "needle",
    );
    submit(&shared, &mut context, &["list-panes"], "ccc");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.history.command_history, ["ccc"]);
        assert_eq!(inner.history.search_history, ["needle"]);
    }

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-T", "search"]),
        )
        .expect("search prompt");
    let state = prompt_state(&shared, client).expect("search prompt");
    assert_eq!(state.prompt_type, CommandPromptType::Search);
    assert_eq!(state.history, ["needle"]);
    assert_eq!(state.kind, CommandPromptKind::Command);
}

#[test]
fn a_plain_prompt_freezes_publication_and_its_clear_resumes_it() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (session, _, pane) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("prompt-freeze")
        .expect("create prompt session");
    let terminal = Arc::new(TerminalSession::spawn_output_view(
        "prompt fixture".to_owned(),
        "fixture".to_owned(),
    ));
    shared
        .inner
        .lock()
        .terminals_mut()
        .insert(pane, Arc::clone(&terminal));
    shared
        .attach(client, session)
        .expect("attach prompt client");
    wait_for_viewport(
        &terminal,
        TerminalViewId(client.0),
        "prompt viewport never attached",
        |_| true,
    );
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    let viewport = terminal
        .latest_viewport_for(TerminalViewId(client.0))
        .unwrap_or_else(|| terminal.latest_viewport());

    for (flags, frozen) in [
        (&["-p", "plain"][..], true),
        (&["-C", "-p", "flowing"], false),
        (&["-i", "-p", "search", "list-panes"], false),
    ] {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("command-prompt", flags.iter().copied()),
            )
            .expect("prompt");
        assert_eq!(
            client_terminal_publication_frozen(&shared.inner.lock(), client),
            frozen,
            "flags {flags:?}"
        );
        drain_terminal_lane(&mailbox);
        shared.publish_terminal_for_pane(
            pane,
            client,
            None,
            &viewport,
            &terminal,
            &mut PaneFrameFanout::new(),
        );
        assert_eq!(
            mailbox.state.lock().terminals.contains_key(&pane),
            !frozen,
            "flags {flags:?}"
        );

        drain_terminal_lane(&mailbox);
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::CommandPrompt {
                    action: CommandPromptAction::Close,
                },
            )
            .expect("close prompt");
        assert!(prompt_state(&shared, client).is_none());
        assert!(!client_terminal_publication_frozen(
            &shared.inner.lock(),
            client
        ));
        assert_eq!(
            mailbox.state.lock().terminals.contains_key(&pane),
            frozen,
            "closing a frozen prompt owes one full latest viewport, flags {flags:?}"
        );
    }
}

#[test]
fn a_prompt_clears_the_message_it_covers_and_owns_the_freeze() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "prompt-over-message", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-N", "-d", "5000", "sticky"]),
        )
        .expect("display message");
    let armed = armed_client_message(&shared, client);
    assert!(armed.freeze);
    assert!(shared.read_client(client, |c| c.is_some_and(|c| c.message_ignore_keys)));
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-C", "-p", "flowing"]),
        )
        .expect("no-freeze prompt");
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.message.is_none())));
    assert!(shared.read_client(client, |c| c.is_some_and(|c| c.message_ignore_keys)));
    assert_eq!(
        cleared_message_ids(&take_reliable_messages(&mailbox)),
        vec![armed.token]
    );
    assert!(!client_terminal_publication_frozen(
        &shared.inner.lock(),
        client
    ));

    shared.detach(client);
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.command_prompt.is_none())));
    assert!(!client_terminal_publication_frozen(
        &shared.inner.lock(),
        client
    ));
}

#[test]
fn an_incremental_prompt_fires_on_entry_and_on_every_edit() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "incremental", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "command-prompt",
                ["-i", "-I", "seed", "-p", "search", "rename-window -- %%"],
            ),
        )
        .expect("incremental prompt");
    let window = context.window.expect("window");
    assert_eq!(prompt_state(&shared, client).expect("raised").input, "");
    assert_eq!(shared.inner.lock().engine.state.windows[&window].name, "=");

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: chooser_key_press("a"),
                text_follows: false,
            },
        )
        .expect("type into the incremental prompt");
    assert_eq!(prompt_state(&shared, client).expect("open").input, "a");
    assert_eq!(shared.inner.lock().engine.state.windows[&window].name, "=a");

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(
                    KeyCode::Character('r'),
                    Modifiers::new(false, true, false, false),
                    None,
                ),
                text_follows: false,
            },
        )
        .expect("reverse search");
    assert_eq!(shared.inner.lock().engine.state.windows[&window].name, "-a");

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(KeyCode::Enter, Modifiers::default(), None),
                text_follows: false,
            },
        )
        .expect("close the incremental prompt");
    assert!(prompt_state(&shared, client).is_none());
    assert_eq!(
        shared.inner.lock().engine.state.windows[&window].name,
        "-a",
        "Enter records history and closes without running the template again"
    );
    assert_eq!(shared.inner.lock().history.command_history, ["a"]);
}

#[test]
fn prompt_history_limit_file_format_and_legacy_loading_match_the_pin() {
    let mut history = Vec::new();
    assert!(add_command_prompt_history(&mut history, "one", 2));
    assert!(add_command_prompt_history(&mut history, "two", 2));
    assert!(!add_command_prompt_history(&mut history, "two", 2));
    assert!(add_command_prompt_history(&mut history, "three", 2));
    assert_eq!(history, ["two", "three"]);
    assert!(add_command_prompt_history(&mut history, "three", 1));
    assert_eq!(history, ["three"]);
    assert!(add_command_prompt_history(&mut history, "ignored", 0));
    assert!(history.is_empty());

    let directory = tempfile::tempdir().expect("prompt history directory");
    let path = directory.path().join("history");
    save_command_prompt_history(
        &path,
        &["alpha".to_owned(), "beta".to_owned()],
        &["needle".to_owned()],
    );
    assert_eq!(
        fs::read_to_string(&path).expect("saved prompt history"),
        "command:alpha\ncommand:beta\nsearch:needle\n"
    );
    fs::write(
        &path,
        "command:one\nsearch:skip\nlegacy:raw\nplain\ncommand:one\n",
    )
    .expect("legacy prompt history");
    let (command, search) = load_command_prompt_history(&path, 3);
    assert_eq!(command, ["legacy:raw", "plain", "one"]);
    assert_eq!(search, ["skip"]);
    assert!(prompt_history_path("relative/history").is_none());
    assert_eq!(prompt_history_path(path.to_str().unwrap()), Some(path));
}

#[test]
fn prompt_history_commands_match_the_pin_and_persist_clears() {
    let shared = Arc::new(Shared::new(1));
    let directory = tempfile::tempdir().expect("prompt history directory");
    let path = directory.path().join("history");
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-s", "history-file", path.to_str().unwrap()]),
        )
        .expect("set history file");
    {
        let mut inner = shared.inner.lock();
        inner.history.command_history = vec!["first".to_owned(), "second".to_owned()];
        inner.history.search_history = vec!["needle".to_owned()];
    }

    assert_eq!(
        shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-prompt-history", [] as [&str; 0]),
            )
            .expect("show both histories")
            .output,
        "History for command:\n\n1: first\n2: second\n\nHistory for search:\n\n1: needle\n\n"
    );
    assert_eq!(
        shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("showphist", ["-T", "search"]),
            )
            .expect("show search history")
            .output,
        "History for search:\n\n1: needle\n\n"
    );

    let invalid = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-prompt-history", ["-T", "target"]),
        )
        .expect_err("invalid prompt type accepted");
    assert!(matches!(
        invalid,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "invalid type: target"
    ));

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("clearphist", ["-T", "command"]),
        )
        .expect("clear command history");
    {
        let inner = shared.inner.lock();
        assert!(inner.history.command_history.is_empty());
        assert_eq!(inner.history.search_history, ["needle"]);
    }
    assert_eq!(
        fs::read_to_string(&path).expect("read selectively cleared history"),
        "search:needle\n"
    );
    assert_eq!(
        shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("showphist", ["-Tcommand"]),
            )
            .expect("show empty command history")
            .output,
        "History for command:\n\n\n"
    );

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("clear-prompt-history", [] as [&str; 0]),
        )
        .expect("clear all history");
    {
        let inner = shared.inner.lock();
        assert!(inner.history.command_history.is_empty());
        assert!(inner.history.search_history.is_empty());
    }
    assert_eq!(fs::read_to_string(&path).expect("read cleared history"), "");
}

#[test]
fn prompt_history_persistence_serializes_record_and_clear_interleavings() {
    let shared = Arc::new(Shared::new(1));
    let directory = tempfile::tempdir().expect("prompt history directory");
    let path = directory.path().join("history");
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-s", "history-file", path.to_str().unwrap()]),
        )
        .expect("set history file");

    {
        let mut inner = shared.inner.lock();
        inner.history.command_history = vec!["old".to_owned()];
        inner.history.search_history = vec!["needle".to_owned()];
    }
    let (record_locked_tx, record_locked_rx) = crossbeam_channel::bounded(1);
    let (resume_record_tx, resume_record_rx) = crossbeam_channel::bounded(1);
    let record_shared = Arc::clone(&shared);
    let record = thread::spawn(move || {
        record_shared.record_prompt_history_with_persist_observer(
            CommandPromptType::Command,
            "new",
            || {},
            || {
                record_locked_tx.send(()).expect("report record save lock");
                resume_record_rx.recv().expect("resume record save");
            },
        );
    });
    record_locked_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("record acquired save lock");

    let (clear_waiting_tx, clear_waiting_rx) = crossbeam_channel::bounded(1);
    let clear_shared = Arc::clone(&shared);
    let clear = thread::spawn(move || {
        clear_shared.clear_prompt_history_with_persist_observer(
            Some(CommandPromptType::Command),
            || clear_waiting_tx.send(()).expect("report selective clear"),
            || {},
        );
    });
    clear_waiting_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("selective clear reached save lock");
    {
        let inner = shared.inner.lock();
        assert!(inner.history.command_history.is_empty());
        assert_eq!(inner.history.search_history, ["needle"]);
    }
    resume_record_tx.send(()).expect("resume record save");
    record.join().expect("record history worker");
    clear.join().expect("selective clear worker");
    assert_eq!(
        fs::read_to_string(&path).expect("read selective-clear race result"),
        "search:needle\n"
    );

    {
        let mut inner = shared.inner.lock();
        inner.history.command_history = vec!["old".to_owned()];
        inner.history.search_history = vec!["needle".to_owned()];
    }
    let (clear_locked_tx, clear_locked_rx) = crossbeam_channel::bounded(1);
    let (resume_clear_tx, resume_clear_rx) = crossbeam_channel::bounded(1);
    let clear_shared = Arc::clone(&shared);
    let clear = thread::spawn(move || {
        clear_shared.clear_prompt_history_with_persist_observer(
            None,
            || {},
            || {
                clear_locked_tx
                    .send(())
                    .expect("report clear-all save lock");
                resume_clear_rx.recv().expect("resume clear-all save");
            },
        );
    });
    clear_locked_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("clear all acquired save lock");

    let (record_waiting_tx, record_waiting_rx) = crossbeam_channel::bounded(1);
    let record_shared = Arc::clone(&shared);
    let record = thread::spawn(move || {
        record_shared.record_prompt_history_with_persist_observer(
            CommandPromptType::Command,
            "new",
            || {
                record_waiting_tx
                    .send(())
                    .expect("report record after clear-all");
            },
            || {},
        );
    });
    record_waiting_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("record reached save lock after clear all");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.history.command_history, ["new"]);
        assert!(inner.history.search_history.is_empty());
    }
    resume_clear_tx.send(()).expect("resume clear-all save");
    clear.join().expect("clear-all worker");
    record.join().expect("record history worker");
    assert_eq!(
        fs::read_to_string(&path).expect("read clear-all race result"),
        "command:new\n"
    );
}

#[test]
fn command_prompt_focus_actions_match_tmux_prequeue() {
    for (focused, answer) in [(true, "FocusIn"), (false, "FocusOut")] {
        assert_eq!(
            command_prompt_focus_action(CommandPromptMode::Key, focused),
            PromptKeyAction::SubmitAnswer(answer.to_owned())
        );
        assert_eq!(
            command_prompt_focus_action(CommandPromptMode::Numeric, focused),
            PromptKeyAction::SubmitAndPass
        );
        for mode in [
            CommandPromptMode::Text,
            CommandPromptMode::Single,
            CommandPromptMode::Incremental,
            CommandPromptMode::BackspaceExit,
        ] {
            assert_eq!(
                command_prompt_focus_action(mode, focused),
                PromptKeyAction::Handled
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn stock_copy_search_uses_advertised_native_ui() {
    for (native_search, tty, command_output) in [
        (false, false, false),
        (false, true, false),
        (false, false, true),
        (false, true, true),
        (true, false, false),
        (true, true, false),
        (true, false, true),
        (true, true, true),
    ] {
        for (key, direction) in [
            ('/', SearchDirection::Forward),
            ('?', SearchDirection::Backward),
        ] {
            let (shared, client, mut context, pane, terminal, mailbox) =
                copy_mode_fixture("native-copy-search", ":");
            assert!(shared.read_client(client, |c| c.is_some_and(|c| c.has_terminal)));
            if native_search {
                shared
                    .inner
                    .lock()
                    .client_entry(client)
                    .native_terminal_search = true;
            }
            if tty {
                shared
                    .inner
                    .lock()
                    .client_entry(client)
                    .tty
                    .replace("/dev/pts/42".to_owned());
            }
            shared
                .execute(
                    client,
                    ClientKind::Interactive,
                    &mut context,
                    &CommandInvocation::new("set-window-option", ["mode-keys", "vi"]),
                )
                .expect("vi copy keys");
            if command_output {
                shared
                    .open_command_output(
                        client,
                        Some(pane),
                        "search fixture".to_owned(),
                        "one\ntwo",
                    )
                    .expect("open command output");
                take_command_output_message(&mailbox);
            } else {
                enter_observed_copy_mode(&shared, client, &mut context, pane, &terminal);
            }
            take_reliable_messages(&mailbox);
            let text = key.to_string();
            input_test_key(
                &shared,
                client,
                &mut context,
                pane,
                test_key(KeyCode::Character(key), Modifiers::default(), Some(&text)),
            );
            let messages = take_reliable_messages(&mailbox);
            let native = messages.iter().any(|message| {
                matches!(message,
                ProtocolMessage::Event(Event { payload: EventPayload::TerminalUiCommand {
                    pane: target, command: TerminalUiCommand::BeginSearch { direction: actual },
                }, .. }) if *target == pane && *actual == direction)
            });
            let prompt = messages.iter().any(|message| {
                matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::CommandPrompt { state: Some(_) },
                        ..
                    })
                )
            });
            assert_eq!(
                native, native_search,
                "key={key}, native_search={native_search}, tty={tty}, messages={messages:?}"
            );
            assert_eq!(prompt, !native_search);
            assert_eq!(
                shared.read_client(client, |client| client
                    .is_some_and(|c| c.command_prompt.is_some())),
                !native_search
            );
            if command_output {
                assert!(
                    shared.read_client(client, |c| c.is_some_and(|c| c.command_output.is_some()))
                );
            } else {
                assert!(
                    shared.read_client(client, |c| c.is_some_and(|c| c.copy_session.is_some()))
                );
            }
        }
    }
}

#[test]
fn native_copy_search_matches_only_standard_search_callbacks() {
    let steps = vec![CommandPromptStep {
        label: "(search down) ".to_owned(),
        input: String::new(),
    }];
    for (mode, action, direction) in [
        (
            CommandPromptMode::Text,
            "search-forward",
            zz_terminal::SearchDirection::Forward,
        ),
        (
            CommandPromptMode::Text,
            "search-backward",
            zz_terminal::SearchDirection::Backward,
        ),
        (
            CommandPromptMode::Incremental,
            "search-forward-incremental",
            zz_terminal::SearchDirection::Forward,
        ),
        (
            CommandPromptMode::Incremental,
            "search-backward-incremental",
            zz_terminal::SearchDirection::Backward,
        ),
    ] {
        let template = CommandPromptTemplate::Commands(vec![CommandInvocation::new(
            "send-keys",
            ["-X", action, "--", "%%"],
        )]);
        assert_eq!(
            native_copy_search_direction(&steps, Some(&template), CommandPromptType::Search, mode),
            Some(direction)
        );
        assert_eq!(
            native_copy_search_direction(&steps, Some(&template), CommandPromptType::Command, mode),
            None
        );
        let seeded = vec![CommandPromptStep {
            input: "needle".to_owned(),
            ..steps[0].clone()
        }];
        assert_eq!(
            native_copy_search_direction(&seeded, Some(&template), CommandPromptType::Search, mode),
            None
        );
    }
    for template in [
        CommandPromptTemplate::String("send-keys -X search-forward -- %%".to_owned()),
        CommandPromptTemplate::Commands(vec![CommandInvocation::new("display-message", ["%%"])]),
        CommandPromptTemplate::Commands(vec![CommandInvocation::new(
            "send-keys",
            ["-X", "-t", "%4", "search-forward", "--", "%%"],
        )]),
        CommandPromptTemplate::Commands(vec![
            CommandInvocation::new("send-keys", ["-X", "search-forward", "--", "%%"]),
            CommandInvocation::new("display-message", ["done"]),
        ]),
    ] {
        assert_eq!(
            native_copy_search_direction(
                &steps,
                Some(&template),
                CommandPromptType::Search,
                CommandPromptMode::Text
            ),
            None
        );
    }
}

#[test]
fn command_prompt_editor_preserves_unicode_boundaries_and_history_drafts() {
    let mut prompt = CommandPrompt::new(
        vec![CommandPromptStep {
            label: ":".to_owned(),
            input: "α beta".to_owned(),
        }],
        None,
        None,
        CommandPromptType::Command,
        CommandPromptMode::Text,
        false,
        None,
    );
    assert_eq!(prompt.state(&[]).cursor, 6);
    assert!(prompt.delete_previous_word(zz_terminal::DEFAULT_WORD_SEPARATORS));
    assert_eq!(prompt.input, "α ");
    assert!(prompt.insert("界"));
    assert_eq!(prompt.state(&[]).cursor, 3);
    assert!(prompt.move_left());
    assert_eq!(prompt.state(&[]).cursor, 2);
    assert!(prompt.delete_forward());
    assert_eq!(prompt.input, "α ");

    let history = ["first".to_owned(), "second".to_owned()];
    assert!(prompt.history_up(&history));
    assert_eq!(prompt.input, "second");
    assert!(prompt.history_up(&history));
    assert_eq!(prompt.input, "first");
    assert!(prompt.history_down(&history));
    assert_eq!(prompt.input, "second");
    assert!(prompt.history_down(&history));
    assert_eq!(prompt.input, "α ");

    let printable = test_key(KeyCode::Character('x'), Modifiers::default(), Some("x"));
    assert_eq!(
        command_prompt_key(&mut prompt, &printable, true, &history),
        PromptKeyAction::Handled
    );
    assert_eq!(prompt.input, "α ");
    assert_eq!(
        command_prompt_key(&mut prompt, &printable, false, &history),
        PromptKeyAction::Updated
    );
    assert_eq!(prompt.input, "α x");
}

#[test]
fn native_command_prompt_actions_persist_without_echo_and_submit_final_input() {
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
            &CommandInvocation::new("new-session", ["-s", "palette"]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    shared.attach(client, session).expect("attach session");
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-I", "list-"]),
        )
        .expect("open prompt");
    let opened = take_reliable_messages(&mailbox);
    assert!(opened.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::CommandPrompt { state: Some(state) },
            ..
        }) if state.kind == CommandPromptKind::Command && state.history.is_empty()
    )));

    let update = "rename-window café".to_owned();
    let update_cursor = u32::try_from(update.chars().count()).expect("short cursor");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::CommandPrompt {
                action: CommandPromptAction::Update {
                    input: update.clone(),
                    cursor: update_cursor,
                },
            },
        )
        .expect("persist prompt input");
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .all(|message| !matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::CommandPrompt { .. },
                    ..
                })
            )),
        "local prompt updates must not echo back into InputState"
    );
    {
        let inner = shared.inner.lock();
        let prompt = inner.clients[&client].command_prompt.as_ref().unwrap();
        assert_eq!(prompt.input, update);
        assert_eq!(prompt.cursor, prompt.input.len());
    }

    shared.send_resync(client, &mailbox);
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::CommandPrompt { state: Some(state) },
                    ..
                }) if state.input == update && state.cursor == update_cursor
            ))
    );

    let final_input = "new-window -n completed".to_owned();
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::CommandPrompt {
                action: CommandPromptAction::Submit {
                    input: final_input.clone(),
                },
            },
        )
        .expect("submit final prompt input");
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.command_prompt.is_none())
        );
        assert_eq!(inner.history.command_history.last(), Some(&final_input));
        assert!(
            inner
                .engine
                .state
                .windows
                .values()
                .any(|window| window.name == "completed")
        );
    }
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::CommandPrompt { state: None },
                    ..
                })
            ))
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-I", "draft", "rename-window -- '%%'"]),
        )
        .expect("open value prompt");
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::CommandPrompt { state: Some(state) },
                    ..
                }) if state.kind == CommandPromptKind::Value && state.history.is_empty()
            ))
    );
    assert!(
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::CommandPrompt {
                    action: CommandPromptAction::Update {
                        input: "α".to_owned(),
                        cursor: 2,
                    },
                },
            )
            .is_err()
    );
    assert_eq!(
        shared.inner.lock().clients[&client]
            .command_prompt
            .as_ref()
            .unwrap()
            .input,
        "draft"
    );
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::CommandPrompt {
                action: CommandPromptAction::Close,
            },
        )
        .expect("close prompt");
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.command_prompt.is_none())));
}

#[test]
fn command_prompt_output_is_joined_and_bounded_on_utf8_boundaries() {
    let mut output = String::new();
    assert!(!append_command_prompt_output(&mut output, "first"));
    assert!(!append_command_prompt_output(&mut output, "second"));
    assert_eq!(output, "first\nsecond");

    let content_limit =
        MAX_COMMAND_PROMPT_OUTPUT_BYTES.saturating_sub(COMMAND_PROMPT_OUTPUT_TRUNCATED.len() + 1);
    let mut output = "x".repeat(content_limit - 1);
    assert!(append_command_prompt_output(&mut output, "界"));
    assert_eq!(output.len(), content_limit - 1);
    output.push('\n');
    output.push_str(COMMAND_PROMPT_OUTPUT_TRUNCATED);
    assert!(output.len() <= MAX_COMMAND_PROMPT_OUTPUT_BYTES);
    assert!(output.is_char_boundary(output.len()));
}

#[test]
fn command_prompt_from_a_command_client_needs_a_target_client() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    for kind in [ClientKind::Command, ClientKind::Control] {
        assert!(
            matches!(
                shared.execute(
                    ClientId(7),
                    kind,
                    &mut context,
                    &CommandInvocation::new("command-prompt", ["-k"]),
                ),
                Err(DaemonError::Server(ServerError::InvalidCommand(message)))
                    if message == "no current client"
            ),
            "{kind:?}"
        );
        assert!(
            matches!(
                shared.execute(
                    ClientId(7),
                    kind,
                    &mut context,
                    &CommandInvocation::new("command-prompt", ["-t", "/dev/nope"]),
                ),
                Err(DaemonError::Server(ServerError::InvalidCommand(message)))
                    if message == "can't find client: /dev/nope"
            ),
            "{kind:?}"
        );
    }
}

#[test]
fn default_rename_bindings_prefill_and_submit_native_prompts() {
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
            &CommandInvocation::new("set-option", ["-g", "automatic-rename", "off"]),
        )
        .expect("hold window names");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "work"]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let window = context.window.expect("window");
    let pane = context.pane.expect("pane");
    shared.attach(client, session).expect("attach session");
    take_reliable_messages(&mailbox);

    let open_binding = |context: &mut ExecutionContext, binding: &str| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::Key {
                    pane,
                    input: test_key(
                        KeyCode::Character('b'),
                        Modifiers::new(false, true, false, false),
                        None,
                    ),
                    text_follows: false,
                },
            )
            .expect("prefix key");
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::Text {
                    pane,
                    text: binding.to_owned(),
                },
            )
            .expect("rename binding");
    };
    let replace_and_submit = |context: &mut ExecutionContext, name: &str| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::Key {
                    pane,
                    input: test_key(
                        KeyCode::Character('u'),
                        Modifiers::new(false, true, false, false),
                        None,
                    ),
                    text_follows: false,
                },
            )
            .expect("clear prompt");
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::Text {
                    pane,
                    text: name.to_owned(),
                },
            )
            .expect("replacement name");
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::Key {
                    pane,
                    input: test_key(KeyCode::Enter, Modifiers::default(), None),
                    text_follows: false,
                },
            )
            .expect("submit rename");
    };

    open_binding(&mut context, "$");
    {
        let inner = shared.inner.lock();
        let prompt = inner.clients[&client].command_prompt.as_ref().unwrap();
        assert_eq!(prompt.input, "work");
        assert_eq!(
            prompt.template.as_ref(),
            Some(&CommandPromptTemplate::String(
                "rename-session -- '%%'".to_owned()
            ))
        );
    }
    replace_and_submit(&mut context, "primary");
    assert_eq!(
        shared.inner.lock().engine.state.sessions[&session].name,
        "primary"
    );

    let expected_window_name = shared.inner.lock().engine.state.windows[&window]
        .name
        .clone();
    open_binding(&mut context, ",");
    {
        let inner = shared.inner.lock();
        let prompt = inner.clients[&client].command_prompt.as_ref().unwrap();
        assert_eq!(prompt.input, expected_window_name);
        assert_eq!(
            prompt.template.as_ref(),
            Some(&CommandPromptTemplate::String(
                "rename-window -- '%%'".to_owned()
            ))
        );
    }
    replace_and_submit(&mut context, "editor");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.state.windows[&window].name, "editor");
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.command_prompt.is_none())
        );
    }
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::Snapshot(snapshot),
            ..
        }) if snapshot.sessions[0].name == "primary"
            && snapshot.sessions[0].windows[0].name == "editor"
    )));
}

#[test]
fn prompts_carry_the_prompt_cursor_options_and_their_command_mode_pair() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    for (name, value) in [
        ("@cursor", "red"),
        ("@command-cursor", "#00ff00"),
        ("prompt-cursor-style", "bar"),
        ("prompt-cursor-colour", "#{@cursor}"),
        ("prompt-command-cursor-style", "blinking-underline"),
        ("prompt-command-cursor-colour", "#{@command-cursor}"),
        ("status-keys", "vi"),
    ] {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("set-option", ["-g", name, value]),
            )
            .unwrap_or_else(|error| panic!("set {name}: {error}"));
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "cursor"]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let pane = context.pane.expect("pane");
    shared.attach(client, session).expect("attach session");
    let key = |context: &mut ExecutionContext, code: KeyCode, ctrl: bool| {
        shared
            .input(
                client,
                ClientKind::Interactive,
                context,
                InputMessage::Key {
                    pane,
                    input: test_key(code, Modifiers::new(false, ctrl, false, false), None),
                    text_follows: false,
                },
            )
            .expect("prompt key");
    };
    key(&mut context, KeyCode::Character('b'), true);
    for text in [":", "abc"] {
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::Text {
                    pane,
                    text: text.to_owned(),
                },
            )
            .expect("open and type the prompt");
    }
    let state = |shared: &Shared| {
        command_prompt_state(&shared.inner.lock(), client).expect("an open prompt")
    };
    let typed = state(&shared);
    assert_eq!(typed.input, "abc");
    assert!(!typed.command_mode);
    assert_eq!(
        typed.prompt_cursor,
        PromptCursor {
            style: 6,
            colour: Some(zz_protocol::TmuxColour::Basic(1)),
        }
    );
    take_reliable_messages(&mailbox);
    key(&mut context, KeyCode::Escape, false);
    let command = state(&shared);
    assert!(command.command_mode);
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::CommandPrompt {
                        state: Some(published),
                    },
                    ..
                }) if published.command_mode && published.cursor == 2
            )),
        "entering command mode republishes the prompt"
    );
    key(&mut context, KeyCode::Character('0'), false);
    assert_eq!(state(&shared).cursor, 0);
    take_reliable_messages(&mailbox);
    key(&mut context, KeyCode::Character('I'), false);
    assert!(!state(&shared).command_mode);
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::CommandPrompt {
                        state: Some(published),
                    },
                    ..
                }) if !published.command_mode
                    && published.cursor == 0
                    && published.prompt_cursor.style == 6
            )),
        "leaving command mode at column zero republishes the prompt"
    );
    assert_eq!(
        command.prompt_cursor,
        PromptCursor {
            style: 3,
            colour: Some(zz_protocol::TmuxColour::Rgb(0x00_ff_00)),
        }
    );
    shared
        .inner
        .lock()
        .client_entry(client)
        .command_prompt
        .take();

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-option", ["-gu", "prompt-cursor-colour"]),
        )
        .expect("unset the colour");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "confirm-before",
                ["-b", "-p", "sure? ", "set-option -g @confirmed yes"],
            ),
        )
        .expect("confirm-before");
    let confirm = shared
        .inner
        .lock()
        .client(client)
        .and_then(|c| c.confirm.as_ref())
        .map(|confirm| confirm.state.clone())
        .expect("an open confirm prompt");
    assert_eq!(
        confirm.prompt_cursor,
        PromptCursor {
            style: 6,
            colour: None,
        }
    );
}

#[test]
fn typed_command_prompt_templates_execute_their_constructed_commands() {
    let (shared, client, _, mut context) = popup_test_workspace("prompt-alias");
    for (index, value) in [
        (90, "a10e=display-message"),
        (
            91,
            "display-message=set-environment -g COMMAND_PROMPT_DOUBLE yes",
        ),
    ] {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(
                    "set-option",
                    ["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .expect("set prompt alias");
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-I", "x", "{ a10e }"])
                .with_command_blocks([2]),
        )
        .expect("open typed prompt");
    let template = shared.inner.lock().clients[&client]
        .command_prompt
        .as_ref()
        .unwrap()
        .template
        .clone();
    assert!(matches!(
        &template,
        Some(CommandPromptTemplate::Commands(commands))
            if commands.len() == 1 && commands[0].name == "display-message"
    ));
    shared.submit_command_prompt(
        client,
        ClientKind::Interactive,
        &mut context,
        &CommandPromptSubmission {
            inputs: vec!["x".to_owned()],
            template,
            source: None,
        },
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("COMMAND_PROMPT_DOUBLE"),
        None
    );

    shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.command_prompt.take());
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "command-prompt",
                [
                    "-I",
                    "x",
                    "{ kill-session -t missing ; set-environment -g PROMPT_GROUP_SAME yes\nset-environment -g PROMPT_GROUP_NEXT yes }",
                ],
            )
            .with_command_blocks([2]),
        )
        .expect("open grouped typed prompt");
    let template = shared.inner.lock().clients[&client]
        .command_prompt
        .as_ref()
        .unwrap()
        .template
        .clone();
    shared.submit_command_prompt(
        client,
        ClientKind::Interactive,
        &mut context,
        &CommandPromptSubmission {
            inputs: vec!["x".to_owned()],
            template,
            source: None,
        },
    );
    let inner = shared.inner.lock();
    assert_eq!(
        inner
            .engine
            .global_environment_variable("PROMPT_GROUP_SAME"),
        None
    );
    assert_eq!(
        inner
            .engine
            .global_environment_variable("PROMPT_GROUP_NEXT"),
        Some("yes".to_owned())
    );
}

#[test]
fn typed_command_prompt_substitution_keeps_argument_boundaries() {
    let (shared, client, _, mut context) = popup_test_workspace("prompt-substitution");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "command-prompt",
                [
                    "-I",
                    "x",
                    "{ if-shell -F 1 { set-environment -g PROMPT_VALUE '%%:%%' } }",
                ],
            )
            .with_command_blocks([2]),
        )
        .expect("open nested typed prompt");
    let template = shared.inner.lock().clients[&client]
        .command_prompt
        .as_ref()
        .unwrap()
        .template
        .clone();
    let input = "x\" ; set-environment -g PROMPT_INJECTED yes ; display-message -p \"";
    shared.submit_command_prompt(
        client,
        ClientKind::Interactive,
        &mut context,
        &CommandPromptSubmission {
            inputs: vec![input.to_owned()],
            template,
            source: None,
        },
    );

    let inner = shared.inner.lock();
    assert_eq!(
        inner.engine.global_environment_variable("PROMPT_VALUE"),
        Some(format!("{input}:%%"))
    );
    assert_eq!(
        inner.engine.global_environment_variable("PROMPT_INJECTED"),
        None
    );
}

#[test]
fn string_command_prompt_preflights_the_substituted_source_before_execution() {
    let (shared, client, mailbox, mut context) = popup_test_workspace("prompt-preflight");
    shared.submit_command_prompt(
        client,
        ClientKind::Interactive,
        &mut context,
        &CommandPromptSubmission {
            inputs: vec!["x".to_owned()],
            template: Some(CommandPromptTemplate::String(
                "set-environment -g COMMAND_PROMPT_PRECHECK yes\nunknown-%1".to_owned(),
            )),
            source: Some(SourceSpan {
                source: "prompt-origin.conf".to_owned(),
                line: 37,
                column: 1,
            }),
        },
    );

    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("COMMAND_PROMPT_PRECHECK"),
        None
    );
    assert!(take_reliable_messages(&mailbox).iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientMessage {
                    kind: ClientMessageKind::Error,
                    text,
                    ..
                },
                ..
            }) if text == "prompt-origin.conf:38: unknown command: unknown-x"
        )
    }));
}

#[test]
fn command_prompt_typed_and_string_aliases_freeze_at_different_stages() {
    let (shared, client, _, mut context) = popup_test_workspace("prompt-alias-stages");
    let set_alias = |value: &str| {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("set-option", ["-s", "command-alias[94]", value]),
            )
            .expect("set prompt alias");
    };

    set_alias("cpalias10f=set-environment -g CP_ALIAS_FROZEN");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["{ cpalias10f typed }"])
                .with_command_blocks([0]),
        )
        .expect("open typed alias prompt");
    let typed = shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.command_prompt.take())
        .expect("typed prompt");
    set_alias("cpalias10f=set-environment -g CP_ALIAS_REPLACED");
    shared.submit_command_prompt(
        client,
        ClientKind::Interactive,
        &mut context,
        &CommandPromptSubmission {
            inputs: vec!["go".to_owned()],
            template: typed.template,
            source: typed.source,
        },
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("CP_ALIAS_FROZEN"),
        Some("typed".to_owned())
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("CP_ALIAS_REPLACED"),
        None
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["cpalias10f string"]),
        )
        .expect("open string alias prompt");
    let string = shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.command_prompt.take())
        .expect("string prompt");
    set_alias("cpalias10f=set-environment -g CP_ALIAS_FRESH");
    shared.submit_command_prompt(
        client,
        ClientKind::Interactive,
        &mut context,
        &CommandPromptSubmission {
            inputs: vec!["go".to_owned()],
            template: string.template,
            source: string.source,
        },
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("CP_ALIAS_FRESH"),
        Some("string".to_owned())
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("CP_ALIAS_REPLACED"),
        None
    );
}

#[test]
fn native_command_prompt_precedes_pane_input_and_runs_command_sequences() {
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
            &CommandInvocation::new("new-session", [] as [&str; 0]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let source = context.pane.expect("source pane");
    shared.attach(client, session).expect("attach session");
    take_reliable_messages(&mailbox);

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: source,
                input: test_key(
                    KeyCode::Character('b'),
                    Modifiers::new(false, true, false, false),
                    None,
                ),
                text_follows: false,
            },
        )
        .expect("prefix key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: source,
                input: test_key(KeyCode::Character(':'), Modifiers::default(), Some(":")),
                text_follows: true,
            },
        )
        .expect("command-prompt binding");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane: source,
                text: ":".to_owned(),
            },
        )
        .expect("suppress committed binding text");
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .command_prompt
                .as_ref()
                .unwrap()
                .input,
            "",
            "the binding character must not leak into the prompt"
        );
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.suppressed_text.is_none()),
            "consumed committed text must not leave suppression bookkeeping behind"
        );
    }
    let opened = take_reliable_messages(&mailbox);
    assert!(opened.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::CommandPrompt { state: Some(state) },
            ..
        }) if state.input.is_empty()
    )));
    shared.send_resync(client, &mailbox);
    let resync = take_reliable_messages(&mailbox);
    assert!(resync.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::Snapshot(_),
            ..
        })
    )));
    assert!(resync.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::CommandPrompt { state: Some(state) },
            ..
        }) if state.input.is_empty()
    )));

    let command = "new-window -n prompted; split-window -h";
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane: source,
                text: command.to_owned(),
            },
        )
        .expect("prompt text");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: source,
                input: test_key(KeyCode::Enter, Modifiers::default(), None),
                text_follows: false,
            },
        )
        .expect("submit prompt");

    {
        let inner = shared.inner.lock();
        let session_state = &inner.engine.state.sessions[&session];
        assert_eq!(session_state.windows.len(), 2);
        let active = &inner.engine.state.windows[&session_state.active_window];
        assert_eq!(active.name, "prompted");
        assert_eq!(active.panes.len(), 2);
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.command_prompt.is_none())
        );
        assert_eq!(
            inner.history.command_history.last().map(String::as_str),
            Some(command)
        );
    }
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::CommandPrompt { state: None },
            ..
        })
    )));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", [] as [&str; 0]),
        )
        .expect("reopen prompt");
    let pane = context.pane.expect("active pane");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(KeyCode::ArrowUp, Modifiers::default(), None),
                text_follows: false,
            },
        )
        .expect("prompt history");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .command_prompt
            .as_ref()
            .unwrap()
            .input,
        command
    );
}

#[test]
fn command_prompt_templates_and_output_use_native_view_mode() {
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
            &CommandInvocation::new("new-session", [] as [&str; 0]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    shared.attach(client, session).expect("attach session");
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-I", "scratch", "new-window -n %%"]),
        )
        .expect("template prompt");
    let pane = context.pane.expect("active pane");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(KeyCode::Enter, Modifiers::default(), None),
                text_follows: false,
            },
        )
        .expect("submit template");
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.unwrap()].name,
        "scratch"
    );

    take_reliable_messages(&mailbox);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("command-prompt", ["-I", "list-panes"]),
        )
        .expect("output prompt");
    let pane = context.pane.expect("active pane");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(KeyCode::Enter, Modifiers::default(), None),
                text_follows: false,
            },
        )
        .expect("submit output command");
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
        panic!("expected native command-output viewport");
    };
    assert_eq!(output_pane, pane);
    assert!(matches!(
        viewport.mode,
        zz_terminal::TerminalMode::View { .. }
    ));
    assert!(viewport_text(&viewport).contains("[history "));
    assert!(
        shared.inner.lock().clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table()
            .is_some()
    );

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeCommandOutput {
                columns: 20,
                rows: 4,
                cell_width_px: 8,
                cell_height_px: 18,
            },
        )
        .expect("resize output view");
    let resized = take_command_output_message(&mailbox);
    assert!(matches!(
        resized,
        ProtocolMessage::Event(Event {
            payload: EventPayload::CommandOutput {
                viewport: Some(TerminalViewport {
                    columns: 20,
                    rows: 4,
                    ..
                }),
                ..
            },
            ..
        })
    ));

    let output_id = current_command_output_id(&shared, client);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane,
                text: "q".to_owned(),
            },
        )
        .expect("close output view");
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
        None
    );
}
