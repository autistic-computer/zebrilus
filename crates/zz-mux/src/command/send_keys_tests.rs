use super::test_support::command;
use super::*;

#[test]
fn send_keys_preserves_extended_named_tokens_and_explicit_literals() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .unwrap();
    for name in ["BTab", "btab", "KPEnter", "kpenter", "KP0", "PageUp"] {
        let execution = engine
            .execute(&mut context, &command("send-keys", &[name]))
            .unwrap();
        assert!(
            matches!(
                execution.effects.as_slice(),
                [MuxEffect::SendKeys { keys, .. }]
                    if keys == &[KeyToken::Named(canonical_named_key(name).unwrap().to_owned())]
            ),
            "{name}: {:?}",
            execution.effects
        );
        let literal = engine
            .execute(&mut context, &command("send-keys", &["-l", name]))
            .unwrap();
        assert!(matches!(
            literal.effects.as_slice(),
            [MuxEffect::SendKeys { keys, .. }]
                if keys == &[KeyToken::Literal(name.to_owned())]
        ));
    }
}

#[test]
fn send_keys_reset_runs_before_the_requested_key_delivery() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .expect("session");

    let reset_only = engine
        .execute(&mut context, &command("send-keys", &["-R"]))
        .expect("reset only");
    let [MuxEffect::ResetPane { pane }] = reset_only.effects.as_slice() else {
        panic!("send-keys -R resets the pane and sends nothing: {reset_only:?}");
    };
    let pane = *pane;

    let plain = engine
        .execute(&mut context, &command("send-keys", &["hello"]))
        .expect("plain keys");
    let reset_and_keys = engine
        .execute(&mut context, &command("send-keys", &["-R", "hello"]))
        .expect("reset then keys");
    let mut expected = vec![MuxEffect::ResetPane { pane }];
    expected.extend(plain.effects.iter().cloned());
    assert_eq!(
        reset_and_keys.effects, expected,
        "pinned cmd-send-keys.c resets, skips the count==0 early return, then injects the strings"
    );

    let copy_action = engine
        .execute(
            &mut context,
            &command("send-keys", &["-R", "-X", "copy-selection"]),
        )
        .expect("copy action");
    assert!(
        !copy_action
            .effects
            .iter()
            .any(|effect| matches!(effect, MuxEffect::ResetPane { .. })),
        "the pin returns inside the -X block before it reaches the -R block"
    );
}

#[test]
fn send_keys_inject_routes_to_the_client_and_replays_the_invoking_key() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .expect("session");

    let injected = engine
        .execute(
            &mut context,
            &command("send-keys", &["-c", "/dev/ttys9", "-K", "Z"]),
        )
        .expect("client injection");
    assert_eq!(
        injected.effects,
        [MuxEffect::SendClientKeys {
            target_client: Some("/dev/ttys9".to_owned()),
            keys: vec![KeyToken::Literal("Z".to_owned())],
            repeat: 1,
        }]
    );

    let repeated = engine
        .execute(
            &mut context,
            &command("send-keys", &["-c", "/dev/ttys9", "-N", "3", "-K", "Z"]),
        )
        .expect("repeated injection");
    assert_eq!(
        repeated.effects,
        [MuxEffect::SendClientKeys {
            target_client: Some("/dev/ttys9".to_owned()),
            keys: vec![KeyToken::Literal("Z".to_owned())],
            repeat: 3,
        }]
    );

    context.set_invoking_key(Some("F5".to_owned()));
    let replayed = engine
        .execute(
            &mut context,
            &command("send-keys", &["-c", "/dev/ttys9", "-K"]),
        )
        .expect("replayed injection");
    assert_eq!(
        replayed.effects,
        [MuxEffect::SendClientKeys {
            target_client: Some("/dev/ttys9".to_owned()),
            keys: vec![KeyToken::Named("F5".to_owned())],
            repeat: 1,
        }]
    );

    let counted = engine
        .execute(
            &mut context,
            &command("send-keys", &["-c", "/dev/ttys9", "-N", "2", "-K"]),
        )
        .expect("counted no-key form");
    assert!(
        !counted
            .effects
            .iter()
            .any(|effect| matches!(effect, MuxEffect::SendClientKeys { .. })),
        "a -N with no positional key injects nothing: {counted:?}"
    );

    context.set_invoking_key(None);
    let unqueued = engine
        .execute(
            &mut context,
            &command("send-keys", &["-c", "/dev/ttys9", "-K"]),
        )
        .expect("no key to replay");
    assert_eq!(unqueued.effects, []);
}

#[test]
fn send_keys_rejects_outer_copy_action_flags_with_pin_errors() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    for flag in ["-C", "-P", "-o"] {
        assert_eq!(
            engine.execute(&mut context, &command("send-keys", &[flag])),
            Err(ServerError::CommandParse(format!(
                "command send-keys: unknown flag {flag}"
            )))
        );
        assert_eq!(
            engine.execute(
                &mut context,
                &command("send-keys", &["-X", flag, "copy-selection"]),
            ),
            Err(ServerError::CommandParse(format!(
                "command send-keys: unknown flag {flag}"
            )))
        );
    }
}

#[test]
fn send_keys_repeat_count_uses_the_copy_mode_action_policy() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .expect("session");

    for arguments in [
        &["-N", "3", "-X", "next-space-end"][..],
        &["-N", "3", "-X", "jump-forward", "x"][..],
        &["-N", "3", "-X", "next-matching-bracket"][..],
        &["-N", "3", "-X", "select-line"][..],
        &["-N", "3", "-X", "other-end"][..],
        &["-N", "3", "-X", "copy-pipe-end-of-line-and-cancel"][..],
    ] {
        let execution = engine
            .execute(&mut context, &command("send-keys", arguments))
            .expect("counted copy action");
        assert!(
            matches!(
                execution.effects.as_slice(),
                [MuxEffect::TerminalView {
                    action: TerminalViewAction::CopyModeCounted { count: 3, .. },
                    ..
                }]
            ),
            "{arguments:?}"
        );
    }

    for action in [
        "rectangle-toggle",
        "begin-selection",
        "clear-selection",
        "copy-pipe-and-cancel",
        "cancel",
    ] {
        let execution = engine
            .execute(
                &mut context,
                &command("send-keys", &["-N", "2", "-X", action]),
            )
            .expect("nonrepeating copy action");
        assert!(
            matches!(
                execution.effects.as_slice(),
                [MuxEffect::TerminalView {
                    action: TerminalViewAction::CopyMode(_),
                    ..
                }]
            ),
            "{action}"
        );
    }
}

#[test]
fn send_keys_repeat_count_matches_pinned_bounds_duplicates_and_clusters() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .expect("session");

    for arguments in [
        &["-N2", "-X", "cursor-right"][..],
        &["-XN2", "cursor-right"][..],
        &["-FXN2", "cursor-right"][..],
        &["-XN", "2", "cursor-right"][..],
        &["-N", "0", "-N", "2", "-X", "cursor-right"][..],
    ] {
        let execution = engine
            .execute(&mut context, &command("send-keys", arguments))
            .expect("valid repeat spelling");
        assert!(
            matches!(
                execution.effects.as_slice(),
                [MuxEffect::TerminalView {
                    action: TerminalViewAction::CopyModeCounted {
                        action,
                        count: 2,
                    },
                    ..
                }] if matches!(action, CopyModeAction::Right)
            ),
            "{arguments:?}"
        );
    }

    for name in ["send", "send-k"] {
        let execution = engine
            .execute(
                &mut context,
                &CommandInvocation::new(name, ["-XN2", "cursor-right"]),
            )
            .expect("alias or unique prefix");
        assert!(matches!(
            execution.effects.as_slice(),
            [MuxEffect::TerminalView {
                action: TerminalViewAction::CopyModeCounted { count: 2, .. },
                ..
            }]
        ));
    }

    let expanded = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &["-N", "#{window_width}", "-X", "cursor-right"],
            ),
        )
        .expect("formatted repeat count");
    assert!(matches!(
        expanded.effects.as_slice(),
        [MuxEffect::TerminalView {
            action: TerminalViewAction::CopyModeCounted { count: 80, .. },
            ..
        }]
    ));

    for (arguments, expected) in [
        (
            &["-N", "0", "-X", "cursor-right"][..],
            "repeat count too small",
        ),
        (
            &["-N", "4294967296", "-X", "cursor-right"][..],
            "repeat count too large",
        ),
        (&["-NX2", "cursor-right"][..], "repeat count invalid"),
        (
            &["-N", "2", "-N", "0", "-X", "cursor-right"][..],
            "repeat count too small",
        ),
    ] {
        assert_eq!(
            engine.execute(&mut context, &command("send-keys", arguments)),
            Err(ServerError::InvalidCommand(expected.to_owned())),
            "{arguments:?}"
        );
    }

    let maximum = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &["-N", "4294967295", "-X", "next-matching-bracket"],
            ),
        )
        .expect("maximum copy repeat");
    assert!(matches!(
        maximum.effects.as_slice(),
        [MuxEffect::TerminalView {
            action: TerminalViewAction::CopyModeCounted {
                count: u32::MAX,
                ..
            },
            ..
        }]
    ));

    let raw = engine
        .execute(
            &mut context,
            &command("send-keys", &["-N", "4294967295", "x"]),
        )
        .expect("maximum raw repeat");
    assert!(matches!(
        raw.effects.as_slice(),
        [MuxEffect::SendKeys {
            keys,
            repeat: u32::MAX,
            ..
        }] if keys == &[KeyToken::Literal("x".to_owned())]
    ));

    let clear = engine
        .execute(
            &mut context,
            &command("send-keys", &["-N", "4294967295", "-X", "clear-selection"]),
        )
        .expect("maximum ignored repeat");
    assert!(matches!(
        clear.effects.as_slice(),
        [MuxEffect::TerminalView {
            action: TerminalViewAction::CopyMode(CopyModeAction::ClearSelection),
            ..
        }]
    ));
}

#[test]
fn read_only_send_keys_accepts_only_typed_safe_copy_actions() {
    let args = |values: &[&str]| {
        values
            .iter()
            .map(|value| RawText::from(*value))
            .collect::<Vec<_>>()
    };

    assert!(!send_keys_is_read_only_safe(&args(&["literal", "Enter"])).unwrap());
    assert_eq!(
        send_keys_is_read_only_safe(&args(&["-N", "bogus"])),
        Ok(false)
    );
    assert_eq!(send_keys_is_read_only_safe(&args(&["-M"])), Ok(false));
    assert!(send_keys_is_read_only_safe(&args(&["-X"])).unwrap());
    assert!(send_keys_is_read_only_safe(&args(&["-X", "unknown-action"])).unwrap());
    assert!(send_keys_is_read_only_safe(&args(&["-X", "cursor-left", "extra"])).unwrap());
    assert!(send_keys_is_read_only_safe(&args(&["-X", "goto-line"])).unwrap());
    assert!(!send_keys_is_read_only_safe(&args(&["-X", "begin-selection", "extra"])).unwrap());
    assert!(!send_keys_is_read_only_safe(&args(&["-X", "jump-forward"])).unwrap());
    for arguments in [
        &["-X", "cursor-left"][..],
        &["-X", "page-down"][..],
        &["-X", "history-top"][..],
        &["-X", "top-line"][..],
        &["-X", "next-word"][..],
        &["-X", "next-prompt", "-o"][..],
        &["-X", "set-mark"][..],
        &["-X", "jump-to-mark"][..],
        &["-X", "cancel"][..],
        &["-X", "next-matching-bracket"][..],
        &["-X", "goto-line", "12"][..],
    ] {
        assert!(
            send_keys_is_read_only_safe(&args(arguments)).unwrap(),
            "{arguments:?}"
        );
    }
    for arguments in [
        &["-X", "begin-selection"][..],
        &["-X", "copy-selection"][..],
        &["-X", "search-again"][..],
        &["-X", "jump-forward", "x"][..],
        &["-X", "rectangle-toggle"][..],
        &["-X", "clear-selection"][..],
        &["-X", "search-forward-cursor-word"][..],
        &["-X", "append-selection"][..],
    ] {
        assert!(
            !send_keys_is_read_only_safe(&args(arguments)).unwrap(),
            "{arguments:?}"
        );
    }
    for entry in crate::copy_actions::missing_copy_mode_actions() {
        assert_eq!(
            send_keys_is_read_only_safe(&args(&["-X", entry.name])),
            Ok(entry.read_only),
            "{}",
            entry.name
        );
    }
}

#[test]
fn send_keys_parses_action_local_flags_and_double_dash() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .unwrap();
    let terminal = context.pane.expect("new session terminal");
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "set-clipboard", "external"]),
        )
        .unwrap();

    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &["-t", &terminal.to_string(), "-X", "copy-selection", "-CP"],
            ),
        )
        .unwrap();
    let [
        MuxEffect::TerminalView {
            action: TerminalViewAction::CopyMode(CopyModeAction::CopySelection(copy)),
            ..
        },
    ] = execution.effects.as_slice()
    else {
        panic!("expected one copy-mode effect");
    };
    assert!(!copy.clipboard);
    assert_eq!(copy.buffer, None);

    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &[
                    "-t",
                    &terminal.to_string(),
                    "-X",
                    "copy-selection",
                    "--",
                    "-C",
                ],
            ),
        )
        .unwrap();
    let [
        MuxEffect::TerminalView {
            action: TerminalViewAction::CopyMode(CopyModeAction::CopySelection(copy)),
            ..
        },
    ] = execution.effects.as_slice()
    else {
        panic!("expected one copy-mode effect");
    };
    assert!(copy.clipboard);
    assert_eq!(
        copy.buffer,
        Some(PasteBufferAction::Create {
            prefix: Some("-C".to_owned()),
        })
    );

    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &["-t", &terminal.to_string(), "-X", "copy-pipe", "true", "-C"],
            ),
        )
        .unwrap();
    let [
        MuxEffect::TerminalView {
            action: TerminalViewAction::CopyMode(CopyModeAction::CopySelection(copy)),
            ..
        },
    ] = execution.effects.as_slice()
    else {
        panic!("expected one copy-mode effect");
    };
    assert!(copy.clipboard);
    assert_eq!(
        copy.buffer,
        Some(PasteBufferAction::Create {
            prefix: Some("-C".to_owned()),
        })
    );
    assert_eq!(copy.pipe.as_deref(), Some("true"));
}

#[test]
fn send_keys_swallows_invalid_action_local_grammar() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .unwrap();
    let terminal = context.pane.expect("new session terminal");
    for (action, flag) in [
        ("copy-selection", "-o"),
        ("previous-prompt", "-C"),
        ("cursor-left", "-P"),
        ("pipe", "-C"),
        ("pipe-no-clear", "-P"),
        ("pipe-and-cancel", "-CP"),
    ] {
        let execution = engine
            .execute(
                &mut context,
                &command(
                    "send-keys",
                    &["-t", &terminal.to_string(), "-X", action, flag],
                ),
            )
            .unwrap();
        assert_eq!(
            execution.effects,
            [MuxEffect::CopyModeRepeat {
                pane: terminal,
                count: 1,
                target_client: None,
                require_mode: true,
            }],
            "{action} accepted {flag}"
        );
    }
    let execution = engine
        .execute(&mut context, &command("send-keys", &["-X"]))
        .unwrap();
    assert!(matches!(
        execution.effects.as_slice(),
        [MuxEffect::CopyModeRepeat {
            count: 0,
            require_mode: true,
            ..
        }]
    ));
    for arguments in [
        &["-X", "unknown-action"][..],
        &["-X", "copy-selection", "one", "two"][..],
        &["-X", "next-prompt", "extra"][..],
        &["-X", "cursor-left", "extra"][..],
        &["-X", "next-prompt", "--", "-o"][..],
        &["-X", "goto-line"][..],
        &["-X", "goto-line", "5", "9"][..],
        &["-X", "jump-forward", ""][..],
    ] {
        let execution = engine
            .execute(&mut context, &command("send-keys", arguments))
            .unwrap();
        assert_eq!(
            execution.effects,
            [MuxEffect::CopyModeRepeat {
                pane: terminal,
                count: 1,
                target_client: None,
                require_mode: true,
            }]
        );
    }
}

#[test]
fn send_keys_preserves_copy_pipe_command_size_errors() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .unwrap();
    let pipe = "x".repeat(MAX_COPY_COMMAND_BYTES + 1);

    assert_eq!(
        engine.execute(
            &mut context,
            &command("send-keys", &["-X", "copy-pipe", pipe.as_str()]),
        ),
        Err(ServerError::InvalidCommand(format!(
            "copy pipe command exceeds {MAX_COPY_COMMAND_BYTES} bytes"
        )))
    );
}

#[test]
fn browser_commands_and_send_keys_produce_server_effects() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .unwrap();
    let execution = engine
        .execute(
            &mut context,
            &command(
                "split-window",
                &["--kind", "browser", "-h", "https://zed.dev"],
            ),
        )
        .unwrap();
    assert!(matches!(
        execution.effects[0],
        MuxEffect::PaneCreated {
            kind: PaneKindSnapshot::Browser(_),
            ..
        }
    ));
    let terminal = engine.state.default_context().unwrap().2;
    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &["-t", &terminal.to_string(), "cargo test", "Enter"],
            ),
        )
        .unwrap();
    assert!(matches!(
        &execution.effects[0],
        MuxEffect::SendKeys { keys, .. }
            if keys == &vec![
                KeyToken::Literal("cargo test".to_owned()),
                KeyToken::Named("Enter".to_owned())
            ]
    ));

    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &["-t", &terminal.to_string(), "-X", "previous-prompt", "-o"],
            ),
        )
        .unwrap();
    assert!(matches!(
        execution.effects.as_slice(),
        [MuxEffect::TerminalView {
            action: TerminalViewAction::CopyMode(CopyModeAction::PreviousPrompt { output: true }),
            ..
        }]
    ));

    let execution = engine
        .execute(
            &mut context,
            &command("clearhist", &["-t", &terminal.to_string()]),
        )
        .unwrap();
    assert!(matches!(
        execution.effects.as_slice(),
        [MuxEffect::TerminalView {
            action: TerminalViewAction::ClearHistory,
            ..
        }]
    ));
    assert!(matches!(
        engine
            .execute(
                &mut context,
                &command("clear-history", &["-H", "-t", &terminal.to_string()]),
            )
            .expect("clear-history -H")
            .effects
            .as_slice(),
        [MuxEffect::TerminalView {
            action: TerminalViewAction::ClearHistory,
            ..
        }]
    ));

    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &["-t", &terminal.to_string(), "-X", "jump-to-forward", "é"],
            ),
        )
        .unwrap();
    assert!(matches!(
        execution.effects.as_slice(),
        [MuxEffect::TerminalView {
            action: TerminalViewAction::CopyMode(CopyModeAction::Jump(CopyJump {
                target,
                direction: CopyJumpDirection::Forward,
                to: true,
            })),
            ..
        }] if target == "é"
    ));
}

#[test]
fn copy_mode_search_commands_produce_native_terminal_effects() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .unwrap();
    let terminal = context.pane.expect("new session terminal");

    let execution = engine
        .execute(
            &mut context,
            &command(
                "copy-mode-search-prompt",
                &["-b", "-t", &terminal.to_string()],
            ),
        )
        .unwrap();
    assert_eq!(
        execution.effects,
        vec![MuxEffect::TerminalUi {
            pane: terminal,
            command: TerminalUiCommand::BeginSearch {
                direction: SearchDirection::Backward,
            },
        }]
    );

    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &["-t", &terminal.to_string(), "-X", "search-again"],
            ),
        )
        .unwrap();
    assert_eq!(
        execution.effects,
        vec![MuxEffect::TerminalView {
            pane: terminal,
            action: TerminalViewAction::CopyMode(CopyModeAction::SearchAgain { reverse: false }),
            target_client: None,
            require_mode: true,
        }]
    );
}

#[test]
fn emacs_copy_mode_navigation_actions_produce_native_terminal_effects() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .expect("session");
    let terminal = context.pane.expect("terminal");

    for (name, expected) in [
        ("scroll-down", CopyModeAction::ScrollDown),
        ("next-prompt", CopyModeAction::NextPrompt { output: false }),
        (
            "previous-prompt",
            CopyModeAction::PreviousPrompt { output: false },
        ),
        ("next-matching-bracket", CopyModeAction::NextMatchingBracket),
        ("scroll-up", CopyModeAction::ScrollUp),
        ("end-of-line", CopyModeAction::EndOfLine),
        ("start-of-line", CopyModeAction::StartOfLine),
        ("history-top", CopyModeAction::Top),
        ("history-bottom", CopyModeAction::Bottom),
        ("halfpage-down", CopyModeAction::HalfPageDown),
        ("top-line", CopyModeAction::TopLine),
        ("halfpage-up", CopyModeAction::HalfPageUp),
        ("page-down", CopyModeAction::PageDown),
    ] {
        let execution = engine
            .execute(
                &mut context,
                &command("send-keys", &["-t", &terminal.to_string(), "-X", name]),
            )
            .unwrap_or_else(|error| panic!("execute {name}: {error}"));
        assert!(matches!(
            execution.effects.as_slice(),
            [MuxEffect::TerminalView {
                action: TerminalViewAction::CopyMode(action),
                ..
            }] if action == &expected
        ));
    }
}

#[test]
fn emacs_copy_mode_non_navigation_actions_produce_native_terminal_effects() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .expect("session");
    let terminal = context.pane.expect("terminal");

    for (name, expected) in [
        ("cancel", CopyModeAction::Cancel),
        (
            "search-reverse",
            CopyModeAction::SearchAgain { reverse: true },
        ),
        ("rectangle-toggle", CopyModeAction::ToggleRectangle),
        (
            "search-again",
            CopyModeAction::SearchAgain { reverse: false },
        ),
    ] {
        let execution = engine
            .execute(
                &mut context,
                &command("send-keys", &["-t", &terminal.to_string(), "-X", name]),
            )
            .unwrap_or_else(|error| panic!("execute {name}: {error}"));
        assert!(matches!(
            execution.effects.as_slice(),
            [MuxEffect::TerminalView {
                action: TerminalViewAction::CopyMode(action),
                ..
            }] if action == &expected
        ));
    }

    for (name, end_of_line) in [
        ("copy-pipe-and-cancel", false),
        ("copy-pipe-end-of-line-and-cancel", true),
    ] {
        let execution = engine
            .execute(
                &mut context,
                &command("send-keys", &["-t", &terminal.to_string(), "-X", name]),
            )
            .unwrap_or_else(|error| panic!("execute {name}: {error}"));
        let action = match execution.effects.as_slice() {
            [
                MuxEffect::TerminalView {
                    action: TerminalViewAction::CopyMode(action),
                    ..
                },
            ] => action,
            effects => panic!("unexpected {name} effects: {effects:?}"),
        };
        let copy = match action {
            CopyModeAction::CopySelection(copy) if !end_of_line => copy,
            CopyModeAction::CopyEndOfLine(copy) if end_of_line => copy,
            action => panic!("unexpected {name} action: {action:?}"),
        };
        assert!(copy.clear_selection);
        assert!(copy.cancel);
        assert!(copy.buffer.is_some());
        assert!(copy.pipe.is_none());
    }
}

#[test]
fn vi_copy_mode_actions_parse_to_native_terminal_effects() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .expect("session");
    let terminal = context.pane.expect("terminal");

    for (name, expected) in [
        ("next-space", CopyModeAction::NextSpace),
        ("previous-space", CopyModeAction::PreviousSpace),
        ("next-space-end", CopyModeAction::NextSpaceEnd),
        ("scroll-up", CopyModeAction::ScrollUp),
        ("scroll-down", CopyModeAction::ScrollDown),
        ("scroll-middle", CopyModeAction::ScrollMiddle),
        ("next-matching-bracket", CopyModeAction::NextMatchingBracket),
    ] {
        let execution = engine
            .execute(
                &mut context,
                &command("send-keys", &["-t", &terminal.to_string(), "-X", name]),
            )
            .unwrap_or_else(|error| panic!("parse {name}: {error}"));
        assert!(matches!(
            execution.effects.as_slice(),
            [MuxEffect::TerminalView {
                action: TerminalViewAction::CopyMode(action),
                ..
            }] if action == &expected
        ));
    }

    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &[
                    "-t",
                    &terminal.to_string(),
                    "-X",
                    "copy-pipe-end-of-line-and-cancel",
                ],
            ),
        )
        .expect("copy to end of line");
    assert!(matches!(
        execution.effects.as_slice(),
        [MuxEffect::TerminalView {
            action: TerminalViewAction::CopyMode(CopyModeAction::CopyEndOfLine(copy)),
            ..
        }] if copy.cancel && copy.clear_selection
    ));

    for (name, direction) in [
        ("search-forward-cursor-word", SearchDirection::Forward),
        ("search-backward-cursor-word", SearchDirection::Backward),
    ] {
        let execution = engine
            .execute(
                &mut context,
                &command("send-keys", &["-t", &terminal.to_string(), "-X", name]),
            )
            .expect("cursor word search");
        assert!(matches!(
            execution.effects.as_slice(),
            [MuxEffect::TerminalView {
                action: TerminalViewAction::CopyMode(CopyModeAction::SearchCursorWord {
                    direction: actual,
                }),
                ..
            }] if *actual == direction
        ));
    }

    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &["-t", &terminal.to_string(), "-X", "goto-line", "42"],
            ),
        )
        .expect("goto line");
    assert!(matches!(
        execution.effects.as_slice(),
        [MuxEffect::TerminalView {
            action: TerminalViewAction::CopyMode(CopyModeAction::GotoLine { line: Some(42), .. }),
            ..
        }]
    ));
}

#[test]
fn goto_line_reads_the_pins_strtonum_grammar_and_range() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .unwrap();
    let terminal = context.pane.expect("new session terminal");
    let goto = |engine: &mut MuxEngine, context: &mut ExecutionContext, argument: &str| {
        engine
            .execute(
                context,
                &command(
                    "send-keys",
                    &[
                        "-t",
                        &terminal.to_string(),
                        "-X",
                        "goto-line",
                        "--",
                        argument,
                    ],
                ),
            )
            .expect("goto-line always exits zero on the pin")
    };

    for (argument, line) in [
        ("0", 0_i32),
        ("5", 5),
        ("+6", 6),
        (" 13", 13),
        ("2147483647", 2_147_483_647),
        ("-1", -1),
    ] {
        let execution = goto(&mut engine, &mut context, argument);
        assert!(
            matches!(
                execution.effects.as_slice(),
                [MuxEffect::TerminalView {
                    action: TerminalViewAction::CopyMode(CopyModeAction::GotoLine {
                        line: Some(actual),
                        ..
                    }),
                    ..
                }] if *actual == line
            ),
            "goto-line {argument:?}"
        );
    }

    for argument in [
        "",
        "abc",
        "3x",
        "0x10",
        "13 ",
        "2147483648",
        "-2",
        "-5",
        "1.0",
        "\u{661}\u{663}",
    ] {
        let execution = goto(&mut engine, &mut context, argument);
        assert!(
            matches!(
                execution.effects.as_slice(),
                [MuxEffect::TerminalView {
                    action: TerminalViewAction::CopyMode(CopyModeAction::GotoLine {
                        line: None,
                        ..
                    }),
                    ..
                }]
            ),
            "goto-line {argument:?} must still reach the mode with no target"
        );
    }
}

#[test]
fn copy_mode_copy_variants_keep_destinations_and_lifecycle_independent() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .unwrap();
    let terminal = context.pane.expect("new session terminal");

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "set-clipboard", "off"]),
        )
        .unwrap();
    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &[
                    "-t",
                    &terminal.to_string(),
                    "-X",
                    "copy-selection-no-clear",
                    "named",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        execution.effects,
        vec![MuxEffect::TerminalView {
            pane: terminal,
            action: TerminalViewAction::CopyMode(CopyModeAction::copy_selection(CopyModeCopy {
                request_id: 0,
                clipboard: false,
                buffer: Some(PasteBufferAction::Create {
                    prefix: Some("named".to_owned()),
                }),
                pipe: None,
                clear_selection: false,
                cancel: false,
            },)),
            target_client: None,
            require_mode: true,
        }]
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "set-clipboard", "external"]),
        )
        .unwrap();
    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &[
                    "-t",
                    &terminal.to_string(),
                    "-X",
                    "copy-selection-and-cancel",
                    "-P",
                ],
            ),
        )
        .unwrap();
    let [
        MuxEffect::TerminalView {
            action: TerminalViewAction::CopyMode(CopyModeAction::CopySelection(copy)),
            ..
        },
    ] = execution.effects.as_slice()
    else {
        panic!("expected one copy-mode effect");
    };
    assert!(copy.clipboard);
    assert_eq!(copy.buffer, None);
    assert_eq!(copy.pipe, None);
    assert!(copy.clear_selection);
    assert!(copy.cancel);

    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &["-t", &terminal.to_string(), "-X", "append-selection"],
            ),
        )
        .unwrap();
    let [
        MuxEffect::TerminalView {
            action: TerminalViewAction::CopyMode(CopyModeAction::CopySelection(copy)),
            ..
        },
    ] = execution.effects.as_slice()
    else {
        panic!("expected one copy-mode effect");
    };
    assert!(!copy.clipboard);
    assert_eq!(copy.buffer, Some(PasteBufferAction::Append));
    assert_eq!(copy.pipe, None);
    assert!(!copy.cancel);

    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &[
                    "-t",
                    &terminal.to_string(),
                    "-X",
                    "copy-pipe-no-clear",
                    "-C",
                    "cat >/tmp/zz-copy",
                    "named",
                ],
            ),
        )
        .unwrap();
    let [
        MuxEffect::TerminalView {
            action: TerminalViewAction::CopyMode(CopyModeAction::CopySelection(copy)),
            ..
        },
    ] = execution.effects.as_slice()
    else {
        panic!("expected one copy-mode effect");
    };
    assert!(!copy.clipboard);
    assert_eq!(
        copy.buffer,
        Some(PasteBufferAction::Create {
            prefix: Some("named".to_owned()),
        })
    );
    assert_eq!(copy.pipe.as_deref(), Some("cat >/tmp/zz-copy"));
    assert!(!copy.clear_selection);
    assert!(!copy.cancel);

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-g", "copy-command", "cat >/tmp/zz-default"],
            ),
        )
        .unwrap();
    let execution = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &["-t", &terminal.to_string(), "-X", "pipe-and-cancel"],
            ),
        )
        .unwrap();
    let [
        MuxEffect::TerminalView {
            action: TerminalViewAction::CopyMode(CopyModeAction::CopySelection(copy)),
            ..
        },
    ] = execution.effects.as_slice()
    else {
        panic!("expected one copy-mode effect");
    };
    assert!(!copy.clipboard);
    assert_eq!(copy.buffer, None);
    assert_eq!(copy.pipe.as_deref(), Some("cat >/tmp/zz-default"));
    assert!(copy.clear_selection);
    assert!(copy.cancel);
}
