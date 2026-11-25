use super::*;

#[test]
fn display_menu_builds_pin_grammar_and_fires_against_the_saved_target() {
    let (shared, client, mailbox, mut context) = popup_test_workspace("desktop");
    let first = context.pane.expect("first pane");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-h"]),
        )
        .expect("second pane");
    let second = context.pane.expect("second pane");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-p", "-t", &first.to_string(), "allow-set-title", "off"],
            ),
        )
        .expect("pin the first pane title");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-menu",
                [
                    "-C",
                    "2",
                    "-T",
                    "#{command}:#{session_active}",
                    "-x",
                    "#{?#{==:#{command},display-menu},1,0}",
                    "-y",
                    "#{?#{==:#{command},display-menu},9,0}",
                    "-t",
                    &first.to_string(),
                    "First-#{command}",
                    "f",
                    "select-pane -T first-choice",
                    "",
                    "-Disabled-#{command}",
                    "d",
                    "select-pane -T disabled-choice",
                    "Last-#{command}",
                    "l",
                    "select-pane -T chosen-#{command}",
                    "",
                ],
            ),
        )
        .expect("open menu");
    let state = shared.inner.lock().clients[&client]
        .menu
        .as_ref()
        .unwrap()
        .state
        .clone();
    assert_eq!(
        (state.left, state.top, state.width, state.height),
        (1, 2, 26, 7)
    );
    assert_eq!(state.title, "display-menu:1");
    assert_eq!(state.selected, Some(3));
    assert_eq!(state.items.len(), 5);
    assert!(state.items[1].is_none());
    assert!(state.items[4].is_none());
    assert!(matches!(
        &state.items[2],
        Some(MenuItem { name, enabled: false, key: None, annotation: None })
            if name == "Disabled-display-menu"
    ));
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Menu { state: Some(_) },
                    ..
                })
            ))
    );

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Menu {
                action: MenuAction::Choose(3),
            },
        )
        .expect("choose menu item");
    let inner = shared.inner.lock();
    assert_eq!(
        inner.engine.state.pane(first).unwrap().title,
        "chosen-display-menu"
    );
    assert_ne!(
        inner.engine.state.pane(second).unwrap().title,
        "chosen-display-menu"
    );
    assert!(inner.client(client).is_none_or(|c| c.menu.is_none()));
}

#[test]
fn display_menu_strips_only_structured_action_wrappers() {
    let (shared, client, _, mut context) = popup_test_workspace("desktop");
    let pane = context.pane.expect("active pane");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-p", "-t", &pane.to_string(), "allow-set-title", "off"],
            ),
        )
        .expect("pin the pane title");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-menu",
                [
                    "Typed",
                    "t",
                    "{ select-pane -T typed-choice }",
                    "Literal",
                    "l",
                    "{ display-message literal }",
                ],
            )
            .with_command_blocks([2]),
        )
        .expect("open menu");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .menu
            .as_ref()
            .unwrap()
            .commands,
        [
            Some("select-pane -T typed-choice".to_owned()),
            Some("{ display-message literal }".to_owned()),
        ]
    );

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Menu {
                action: MenuAction::Choose(0),
            },
        )
        .expect("choose typed menu action");
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .expect("live pane")
            .title,
        "typed-choice"
    );
}

#[test]
fn control_display_menu_validates_item_shapes_before_ignoring_the_overlay() {
    let empty = Arc::new(Shared::new(1));
    let error = empty
        .execute(
            ClientId(u64::MAX),
            ClientKind::Control,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("display-menu", ["Incomplete", "i"]),
        )
        .expect_err("unattached control menu");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "no current client"
    ));

    let (shared, client, _, mut context) = popup_test_workspace("desktop");

    for items in [
        vec!["Incomplete"],
        vec!["Incomplete", "i"],
        vec!["", "Incomplete", "i"],
    ] {
        let error = shared
            .execute(
                client,
                ClientKind::Control,
                &mut context,
                &CommandInvocation::new("display-menu", items),
            )
            .expect_err("incomplete control menu item");
        assert!(matches!(
            error,
            DaemonError::Server(ServerError::InvalidCommand(message))
                if message == "not enough arguments"
        ));
    }

    shared
        .execute(
            client,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new(
                "display-menu",
                ["", "Complete", "c", "display-message complete"],
            ),
        )
        .expect("complete control menu");
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.menu.is_none())));
}

#[test]
fn display_menu_drops_empty_expansions_and_obeys_overlay_and_size_noops() {
    let (shared, client, _, mut context) = popup_test_workspace("desktop");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-menu",
                [
                    "Kept",
                    "k",
                    "display-message kept",
                    "",
                    "#{missing}",
                    "x",
                    "display-message dropped",
                    "",
                ],
            ),
        )
        .expect("open compacted menu");
    let state = shared.inner.lock().clients[&client]
        .menu
        .as_ref()
        .unwrap()
        .state
        .clone();
    assert_eq!(state.items.len(), 2);
    assert!(state.items[1].is_none());
    shared.input_menu(client, &context, MenuAction::Cancel);
    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-menu",
                ["-C", "-1", "Item", "i", "display-message nope"]
            ),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "starting choice too small"
    ));
    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-menu",
                ["-b", "zigzag", "Item", "i", "display-message nope"]
            ),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "menu-border-lines unknown value: zigzag"
    ));

    shared
        .inner
        .lock()
        .client_entry(client)
        .command_prompt
        .replace(CommandPrompt::new(
            vec![CommandPromptStep {
                label: "prompt".to_owned(),
                input: String::new(),
            }],
            None,
            None,
            CommandPromptType::Command,
            CommandPromptMode::Text,
            false,
            None,
        ));
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-menu", ["Incomplete"]),
        )
        .expect("existing overlay wins before item validation");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-menu",
                ["-C", "junk", "Item", "i", "display-message nope"],
            ),
        )
        .expect("existing overlay wins before starting-choice validation");
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.menu.is_none())));
    shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.command_prompt.take());

    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-menu", ["-C", "junk", "Incomplete"]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "starting choice invalid"
    ));
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-menu",
                ["-s", "bogus", "Styled", "s", "display-message nope"],
            ),
        )
        .expect("invalid inline styles fall back silently");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .menu
            .as_ref()
            .unwrap()
            .state
            .style,
        "bg=themedarkgrey,fg=themewhite".to_owned()
    );
    shared.input_menu(client, &context, MenuAction::Cancel);

    let pane = context.pane.expect("menu pane");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminal {
                pane,
                columns: 4,
                rows: 2,
                cell_width_px: 8,
                cell_height_px: 18,
            },
        )
        .expect("shrink menu client");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-menu", ["Too wide", "t", "display-message nope"]),
        )
        .expect("too-small menu is silent");
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.menu.is_none())));
}

#[test]
fn display_menu_returns_before_the_menu_closes_and_disabled_stay_open_is_defensive() {
    let (shared, interactive, _, context) = popup_test_workspace("desktop");
    let mut command_context = context.clone();
    assert_eq!(
        shared
            .execute(
                ClientId(900),
                ClientKind::Command,
                &mut command_context,
                &CommandInvocation::new(
                    "display-menu",
                    [
                        "-c",
                        "desktop",
                        "-O",
                        "--",
                        "-Disabled",
                        "d",
                        "display-message nope",
                    ],
                ),
            )
            .expect("menu opens"),
        Execution::default()
    );
    assert!(shared.read_client(interactive, |c| c.is_some_and(|c| c.menu.is_some())));
    shared.input_menu(interactive, &context, MenuAction::Choose(0));
    assert!(shared.read_client(interactive, |c| c.is_some_and(|c| c.menu.is_some())));
    shared.input_menu(interactive, &context, MenuAction::Cancel);
    assert!(shared.read_client(interactive, |c| c.is_none_or(|c| c.menu.is_none())));
}

#[test]
fn native_callback_usage_preserves_runtime_phase() {
    let shared = Shared::new(1);
    for typed in [false, true] {
        let error = shared
            .parse_confirm_commands("agent-send --bogus", typed)
            .unwrap_err();
        let DaemonError::Server(server_error) = &error else {
            panic!("unexpected callback error: {error}");
        };
        assert_eq!(server_error.exit_code(), 2);
        assert_eq!(server_error.is_command_parse(), typed);
        assert_eq!(
            control_command_guard_error(&error),
            (
                true,
                !typed,
                "agent-send does not support --bogus".to_owned()
            )
        );
    }
}

#[test]
fn confirm_before_parses_first_shapes_prompts_and_uses_one_reply() {
    let (shared, client, _, mut context) = popup_test_workspace("desktop");
    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("confirm-before", ["-c", "bad", "{ broken"]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message != "invalid confirm key"
    ));
    for key in ["", "yy", "é", "\n"] {
        assert!(matches!(
            shared.execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new(
                    "confirm-before",
                    ["-c", key, "display-message valid"]
                ),
            ),
            Err(DaemonError::Server(ServerError::InvalidCommand(message)))
                if message == "invalid confirm key"
        ));
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("confirm-before", ["-c", "Y", "renamew confirmed"]),
        )
        .expect("open default confirm");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .confirm
            .as_ref()
            .unwrap()
            .state
            .prompt,
        "Confirm 'rename-window'? (Y/n) "
    );
    shared.input_confirm(client, &context, ConfirmAction::Reply(false));
    assert_ne!(
        shared.inner.lock().engine.state.windows[&context.window.unwrap()].name,
        "confirmed"
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("confirm-before", ["renamew '#{command}'"]),
        )
        .expect("open string confirm");
    shared.input_confirm(client, &context, ConfirmAction::Reply(true));
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.unwrap()].name,
        "confirm-before"
    );

    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("confirm-before", ["{ renamew quoted }"]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "syntax error"
    ));
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.confirm.is_none())));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("confirm-before", ["{ renamew '#{command}' }"])
                .with_command_blocks([0]),
        )
        .expect("open typed-block confirm");
    shared.input_confirm(client, &context, ConfirmAction::Reply(true));
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.unwrap()].name,
        "rename-window"
    );

    context
        .format_variables
        .insert("question".to_owned(), "Proceed?".to_owned());
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "confirm-before",
                [
                    "-p",
                    "#{question}|#{command} {confirm-key}",
                    "renamew accepted",
                ],
            ),
        )
        .expect("open custom confirm");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .confirm
            .as_ref()
            .unwrap()
            .state
            .prompt,
        "Proceed?| {confirm-key} "
    );
    shared.input_confirm(client, &context, ConfirmAction::Reply(true));
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.unwrap()].name,
        "accepted"
    );
}

#[test]
fn confirm_before_types_freeze_environment_aliases_and_error_channels() {
    let (shared, client, _, mut context) = popup_test_workspace("desktop");
    for option in ["-c", "-p", "-t"] {
        let invocation = CommandInvocation::new(
            "confirm-before",
            [
                option,
                "{ display-message option }",
                "{ display-message action }",
            ],
        )
        .with_command_blocks([1, 2]);
        assert!(matches!(
            shared.execute(client, ClientKind::Interactive, &mut context, &invocation),
            Err(DaemonError::Server(ServerError::CommandParse(message)))
                if message == format!(
                    "command confirm-before: {option} argument must be a string"
                )
        ));
    }

    for child in ["not-a-command", "display-"] {
        let typed = CommandInvocation::new("confirm-before", [format!("{{ {child} }}")])
            .with_command_blocks([0]);
        let typed_error = shared
            .execute(client, ClientKind::Interactive, &mut context, &typed)
            .expect_err("typed child construction error");
        assert!(
            matches!(
                &typed_error,
                DaemonError::Server(ServerError::CommandParse(_))
            ),
            "{typed_error:?}"
        );
        assert!(matches!(
            shared.execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("confirm-before", [child]),
            ),
            Err(DaemonError::Server(ServerError::InvalidCommand(_)))
        ));
    }

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-environment", ["-g", "C10E_CONFIRM_VALUE", "typed-old"]),
        )
        .expect("seed typed environment");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("confirm-before", ["{ rename-window $C10E_CONFIRM_VALUE }"])
                .with_command_blocks([0]),
        )
        .expect("open typed environment confirm");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-environment", ["-g", "C10E_CONFIRM_VALUE", "typed-new"]),
        )
        .expect("mutate typed environment");
    shared.input_confirm(client, &context, ConfirmAction::Reply(true));
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.unwrap()].name,
        "typed-old"
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "set-environment",
                ["-g", "C10E_CONFIRM_VALUE", "string-old"],
            ),
        )
        .expect("seed string environment");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("confirm-before", ["rename-window $C10E_CONFIRM_VALUE"]),
        )
        .expect("open string environment confirm");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "set-environment",
                ["-g", "C10E_CONFIRM_VALUE", "string-new"],
            ),
        )
        .expect("mutate string environment");
    shared.input_confirm(client, &context, ConfirmAction::Reply(true));
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.unwrap()].name,
        "string-old"
    );

    let set_alias = |value: &str| {
        let mut alias_context = ExecutionContext::default();
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut alias_context,
                &CommandInvocation::new("set-option", ["-s", "command-alias[90]", value]),
            )
            .expect("set confirm alias");
    };
    set_alias("c10e=rename-window");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("confirm-before", ["{ c10e typed-alias }"])
                .with_command_blocks([0]),
        )
        .expect("open typed alias confirm");
    set_alias("c10e=display-message -p changed");
    shared.input_confirm(client, &context, ConfirmAction::Reply(true));
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.unwrap()].name,
        "typed-alias"
    );

    set_alias("c10e=rename-window");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[91]",
                    "rename-window=set-environment -g C10E_DOUBLE yes",
                ],
            ),
        )
        .expect("set canonical shadow");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("confirm-before", ["{ c10e typed-shadow }"])
                .with_command_blocks([0]),
        )
        .expect("open typed canonical shadow confirm");
    shared.input_confirm(client, &context, ConfirmAction::Reply(true));
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.unwrap()].name,
        "typed-shadow"
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("C10E_DOUBLE"),
        None
    );

    set_alias("c10e=rename-window");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("confirm-before", ["c10e string-alias"]),
        )
        .expect("open string alias confirm");
    set_alias("c10e=display-message -p changed");
    shared.input_confirm(client, &context, ConfirmAction::Reply(true));
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.unwrap()].name,
        "string-alias"
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "@confirm-payload", "\" ; wibble"]),
        )
        .expect("set confirm format payload");
    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "confirm-before",
                ["-c", "xx", "display-message -p \"#{@confirm-payload}\""],
            ),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "unknown command: wibble"
    ));
    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "confirm-before",
                ["-c", "xx", "{ display-message -p \"#{@confirm-payload}\" }"],
            )
            .with_command_blocks([2]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "invalid confirm key"
    ));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("confirm-before", ["-p", "Empty", "{}"])
                .with_command_blocks([2]),
        )
        .expect("open empty typed custom confirm");
    shared.input_confirm(client, &context, ConfirmAction::Reply(true));
}

#[test]
fn confirm_before_command_groups_and_blocking_failures_match_pin() {
    let (shared, client, mailbox, mut context) = popup_test_workspace("desktop");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "confirm-before",
                [
                    "{ kill-pane -t missing ; set-environment -g C10E_SAME_LINE no\nset-environment -g C10E_NEXT_LINE yes }",
                ],
            )
            .with_command_blocks([0]),
        )
        .expect("open grouped typed confirm");
    shared.input_confirm(client, &context, ConfirmAction::Reply(true));
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.engine.global_environment_variable("C10E_SAME_LINE"),
            None
        );
        assert_eq!(
            inner
                .engine
                .global_environment_variable("C10E_NEXT_LINE")
                .as_deref(),
            Some("yes")
        );
    }

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "confirm-before",
                ["kill-pane -t missing\nset-environment -g C10E_STRING_CONTINUED no"],
            ),
        )
        .expect("open multiline string confirm");
    shared.input_confirm(client, &context, ConfirmAction::Reply(true));
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("C10E_STRING_CONTINUED"),
        None
    );
    take_reliable_messages(&mailbox);

    let sender = Arc::clone(&shared);
    let mut command_context = context.clone();
    let (send, receive) = crossbeam_channel::bounded(1);
    let worker = thread::spawn(move || {
        let result = sender.execute(
            ClientId(901),
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new(
                "confirm-before",
                [
                    "-t",
                    "desktop",
                    "display-message -p before ; kill-pane -t missing",
                ],
            ),
        );
        send.send(result).expect("return blocking confirm result");
    });
    wait_for_confirm_state(&shared, client);
    shared.input_confirm(client, &context, ConfirmAction::Reply(true));
    let error = receive
        .recv_timeout(Duration::from_secs(5))
        .expect("blocking confirm result")
        .expect_err("blocking child failure");
    worker.join().expect("blocking confirm worker");
    let DaemonError::CommandFailed { output, error } = error else {
        panic!("expected command failure with callback provenance");
    };
    assert_eq!(output, "before");
    assert!(matches!(
        *error,
        DaemonError::Server(ServerError::PostAdmissionCallback(error))
            if matches!(&*error, ServerError::PaneNotFound(target) if target == "missing")
    ));
    assert!(
        !take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ClientMessage {
                        kind: ClientMessageKind::Error,
                        ..
                    },
                    ..
                })
            ))
    );
}

#[test]
fn confirm_before_reported_source_failure_keeps_inner_and_outer_groups() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let child = directory.path().join("confirm-source-child.conf");
    fs::write(
        &child,
        "run-shell -C \"{ display-message -p quoted }\"\n\
             set-environment -g CONFIRM_CHILD_LATER yes\n",
    )
    .expect("confirm source child fixture");
    let confirm_body = format!(
        "source-file '{}' ; set-environment -g CONFIRM_INNER_AFTER yes",
        tmux_path(&child)
    );
    let outer_body = format!(
        "confirm-before -t desktop \"{confirm_body}\" ; set-environment -g CONFIRM_OUTER_AFTER yes"
    );
    let (shared, interactive, _, context) = popup_test_workspace("desktop");
    let sender = Arc::clone(&shared);
    let mut command_context = context.clone();
    let (send, receive) = crossbeam_channel::bounded(1);
    let worker = thread::spawn(move || {
        let result = sender.execute(
            ClientId(901),
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("run-shell", ["-C", &outer_body]),
        );
        send.send(result).expect("return nested confirm result");
    });
    wait_for_confirm_state(&shared, interactive);
    shared.input_confirm(interactive, &context, ConfirmAction::Reply(true));
    let result = receive
        .recv_timeout(Duration::from_secs(5))
        .expect("nested confirm result");
    worker.join().expect("nested confirm worker");
    assert!(matches!(
        result,
        Err(DaemonError::ReportedCommandExit { exit_code: 1, .. })
    ));
    let inner = shared.inner.lock();
    for name in [
        "CONFIRM_CHILD_LATER",
        "CONFIRM_INNER_AFTER",
        "CONFIRM_OUTER_AFTER",
    ] {
        assert_eq!(
            inner.engine.global_environment_variable(name).as_deref(),
            Some("yes"),
            "{name}"
        );
    }
}

#[test]
fn confirm_before_mixed_depth_callbacks_preserve_physical_groups() {
    let (shared, interactive, _, context) = popup_test_workspace("desktop");
    for (index, body, marker, expected) in [
        (
            0,
            r#"if-shell -F 1 { if-shell -F 1 "{ display-message -p quoted }" } ; set-environment -g CONFIRM_NESTED_ONLY_SAME yes"#,
            "CONFIRM_NESTED_ONLY_SAME",
            Some("yes"),
        ),
        (
            1,
            r#"run-shell -C "run-shell -C \"{ display-message -p quoted }\"" ; if-shell -F 1 "{ display-message -p quoted }" ; set-environment -g CONFIRM_MIXED_DEPTH_SAME yes"#,
            "CONFIRM_MIXED_DEPTH_SAME",
            None,
        ),
    ] {
        let sender = Arc::clone(&shared);
        let mut command_context = context.clone();
        let (send, receive) = crossbeam_channel::bounded(1);
        let worker = thread::spawn(move || {
            let result = sender.execute(
                ClientId(901 + index),
                ClientKind::Command,
                &mut command_context,
                &CommandInvocation::new("confirm-before", ["-t", "desktop", body]),
            );
            send.send(result)
                .expect("return mixed-depth confirm result");
        });
        wait_for_confirm_state(&shared, interactive);
        shared.input_confirm(interactive, &context, ConfirmAction::Reply(true));
        let error = receive
            .recv_timeout(Duration::from_secs(5))
            .expect("mixed-depth confirm result")
            .expect_err("mixed-depth confirm callback failure");
        worker.join().expect("mixed-depth confirm worker");
        assert_eq!(post_admission_callback_parse_depth(&error), 3);
        assert_eq!(
            shared
                .inner
                .lock()
                .engine
                .global_environment_variable(marker)
                .as_deref(),
            expected,
            "{marker}"
        );
    }
}

#[test]
fn direct_control_confirm_source_failure_preserves_guard_order() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let child = directory.path().join("confirm-source-child.conf");
    fs::write(
        &child,
        "run-shell -C \"{ display-message -p quoted }\"\n\
             display-message -p CONFIRM_CHILD_LATER\n",
    )
    .expect("confirm source child fixture");
    let body = format!(
        "source-file '{}' ; display-message -p CONFIRM_INNER_AFTER",
        child.display()
    );
    let (shared, interactive, _, context) = popup_test_workspace("desktop");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(control, context.session.expect("popup test session"))
        .expect("attach confirm Control client");
    take_reliable_messages(&mailbox);

    let sender = Arc::clone(&shared);
    let mut control_context = context.clone();
    let (send, receive) = crossbeam_channel::bounded(1);
    let worker = thread::spawn(move || {
        let response = sender.execute_command_request(
            control,
            ClientKind::Control,
            &mut control_context,
            1,
            &CommandInvocation::new("confirm-before", ["-t", "desktop", &body]),
        );
        send.send(response).expect("return direct confirm response");
    });
    wait_for_confirm_state(&shared, interactive);
    shared.input_confirm(interactive, &context, ConfirmAction::Reply(true));
    assert_eq!(
        receive
            .recv_timeout(Duration::from_secs(5))
            .expect("direct confirm response"),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    worker.join().expect("direct confirm worker");

    let timeline = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ControlCommandGuard {
                        output,
                        error,
                        flags,
                        ..
                    },
                ..
            }) => Some(format!("guard:{flags}:{error}:{output}")),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlCommandOutput { output },
                ..
            }) => Some(format!("raw:{output}")),
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ControlSourceFile {
                        event: ControlSourceFileEvent::Complete,
                    },
                ..
            }) => Some("complete".to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        timeline,
        [
            "guard:0:false:".to_owned(),
            "guard:0:false:".to_owned(),
            format!("raw:{}:1: syntax error", child.display()),
            "guard:0:false:CONFIRM_CHILD_LATER".to_owned(),
            "complete".to_owned(),
            "guard:0:false:CONFIRM_INNER_AFTER".to_owned(),
        ]
    );
}

#[test]
fn confirm_before_blocking_status_background_state_and_overlay_clearing_match_pin() {
    let (shared, interactive, _, mut context) = popup_test_workspace("desktop");
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-menu", ["Old", "o", "display-message old"]),
        )
        .expect("open old menu");
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("confirm-before", ["display-message replacement"]),
        )
        .expect("replace menu with confirm");
    assert!(shared.read_client(interactive, |c| c.is_none_or(|c| c.menu.is_none())));
    shared.input_confirm(interactive, &context, ConfirmAction::Reply(false));

    let run_blocking = |accepted| {
        let sender = Arc::clone(&shared);
        let command_context = context.clone();
        let (send, receive) = crossbeam_channel::bounded(1);
        let worker = thread::spawn(move || {
            let mut command_context = command_context;
            let result = sender.execute(
                ClientId(901),
                ClientKind::Command,
                &mut command_context,
                &CommandInvocation::new("confirm-before", ["-t", "desktop", "renamew blocking"]),
            );
            send.send(result).expect("return confirm result");
        });
        wait_for_confirm_state(&shared, interactive);
        shared.input_confirm(interactive, &context, ConfirmAction::Reply(accepted));
        let result = receive
            .recv_timeout(Duration::from_secs(5))
            .expect("blocking confirm result");
        worker.join().expect("confirm command worker");
        result
    };
    assert!(matches!(
        run_blocking(false),
        Err(DaemonError::CommandExit { exit_code: 1, .. })
    ));
    run_blocking(true).expect("accepted blocking confirm");
    assert_eq!(
        shared.inner.lock().engine.state.windows[&context.window.unwrap()].name,
        "blocking"
    );

    let original = context.pane.expect("original pane");
    let original_window = context.window.expect("original window");
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-window", ["-n", "fresh"]),
        )
        .expect("fresh target window");
    let fresh_window = context.window.expect("fresh window");
    let mut original_context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, original)
            .expect("original context");
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut original_context,
            &CommandInvocation::new("confirm-before", ["-b", "rename-window background-fresh"]),
        )
        .expect("open background confirm");
    shared.input_confirm(interactive, &context, ConfirmAction::Reply(true));
    let inner = shared.inner.lock();
    assert_eq!(
        inner.engine.state.windows[&fresh_window].name,
        "background-fresh"
    );
    assert_ne!(
        inner.engine.state.windows[&original_window].name,
        "background-fresh"
    );
}

#[test]
fn lock_commands_store_only_and_keep_clientless_error_parity() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("lock-server", [] as [&str; 0]),
        )
        .expect("clientless lock-server");
    assert!(matches!(
        shared.execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("lock-client", [] as [&str; 0]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "no current client"
    ));
    assert!(matches!(
        shared.execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("lock-session", ["-t", "missing"]),
        ),
        Err(DaemonError::Server(ServerError::SessionNotFound(message))) if message == "missing"
    ));

    shared
        .execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "work"]),
        )
        .expect("lock test session");
    shared
        .execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                ["-g", "after-lock-server", "set-option -g @locked yes"],
            ),
        )
        .expect("lock hook");
    shared
        .execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("lock-server", [] as [&str; 0]),
        )
        .expect("lock hook fires");
    assert_eq!(
        shared
            .execute(
                ClientId(99),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-options", ["-gv", "@locked"]),
            )
            .expect("lock hook option")
            .output,
        "yes"
    );
    shared
        .execute(
            ClientId(99),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("lock-session", ["-t", "work"]),
        )
        .expect("existing lock session");
}

#[test]
fn a_command_line_overlay_style_replaces_only_the_option_colours() {
    assert_eq!(overlay_style("bold,bg=colour17", None), "bold,bg=colour17");
    assert_eq!(
        overlay_style("bold,bg=colour17", Some("fg=colour160")),
        "bold,bg=colour17,fg=colour160,bg=default"
    );
    assert_eq!(
        overlay_style("bold,bg=colour17", Some("bg=colour52,dim")),
        "bold,bg=colour17,fg=default,bg=colour52"
    );
    assert_eq!(
        overlay_style("bold,bg=colour17", Some("default")),
        "bold,bg=colour17"
    );
    assert_eq!(
        overlay_style("bold,bg=colour17", Some("bogus")),
        "bold,bg=colour17"
    );
    assert_eq!(overlay_style("", Some("fg=red")), "fg=red,bg=default");
}

#[cfg(unix)]
#[test]
fn overlay_styles_follow_their_options_while_the_overlay_is_up() {
    let (shared, client, mailbox, mut context) = popup_test_workspace("restyle");
    for (option, value) in [
        ("menu-style", "bg=colour17"),
        ("menu-selected-style", "fg=colour46"),
        ("menu-border-style", "fg=colour201"),
    ] {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("set-window-option", [option, value]),
            )
            .expect("set overlay style option");
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-menu",
                [
                    "-x",
                    "0",
                    "-y",
                    "0",
                    "alpha",
                    "a",
                    "display-message -p alpha",
                ],
            ),
        )
        .expect("open menu");
    let state = wait_for_menu_state(&shared, client);
    assert_eq!(state.style, "bg=colour17");
    assert_eq!(state.selected_style, "fg=colour46");
    assert_eq!(state.border_style, "fg=colour201");
    assert_eq!(state.border_lines, PopupBorderLines::Single);
    let _ = take_reliable_messages(&mailbox);

    for (option, value) in [
        ("menu-style", "bg=colour52"),
        ("menu-selected-style", "fg=colour214"),
        ("menu-border-style", "fg=colour45"),
    ] {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("set-window-option", [option, value]),
            )
            .expect("restyle the live menu");
    }
    let state = shared.inner.lock().clients[&client]
        .menu
        .as_ref()
        .unwrap()
        .state
        .clone();
    assert_eq!(state.style, "bg=colour52");
    assert_eq!(state.selected_style, "fg=colour214");
    assert_eq!(state.border_style, "fg=colour45");
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Menu { state: Some(_) },
                    ..
                })
            ))
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-window-option", ["menu-border-lines", "double"]),
        )
        .expect("set menu border lines");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .menu
            .as_ref()
            .unwrap()
            .state
            .border_lines,
        PopupBorderLines::Single
    );
    shared.input_menu(client, &context, MenuAction::Cancel);
}
