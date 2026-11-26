use super::test_support::command;
use super::*;

#[test]
fn option_value_error_text_matches_the_pinned_matrix() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "error-shapes"]),
        )
        .unwrap();
    let mut hooks = CommandHooks::new(0);
    let mut valid_shell = |shell: &str| shell != "/not/a/shell";
    for (arguments, expected) in [
        (
            &["-g", "display-time", "-5"] as &[&str],
            "value is too small: -5",
        ),
        (&["-g", "display-time", "abc"], "value is invalid: abc"),
        (&["-g", "display-time"], "empty value"),
        (&["-g", "@novalue"], "empty value"),
        (&["-g", "status-keys", "bogus"], "unknown value: bogus"),
        (&["-g", "focus-events", "maybe"], "bad value: maybe"),
        (&["-g", "status-bg", "xxxyyy"], "bad colour: xxxyyy"),
        (
            &["-g", "status-style", "bg=xxxyyy"],
            "invalid style: bg=xxxyyy",
        ),
        (&["-g", "prefix", "boguskey"], "bad key: boguskey"),
        (
            &["-g", "default-shell", "/not/a/shell"],
            "not a suitable shell: /not/a/shell",
        ),
        (&["-g", "default-client-command", "if -x {"], "syntax error"),
    ] {
        assert_eq!(
            engine
                .execute_with_shell_validator(
                    &mut context,
                    &command("set-option", arguments),
                    &mut hooks,
                    &mut valid_shell,
                )
                .unwrap_err(),
            ServerError::InvalidCommand(expected.to_owned()),
            "{arguments:?}"
        );
    }
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "@once", "first"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("set-option", &["-go", "@once", "second"]),
            )
            .unwrap_err(),
        ServerError::InvalidCommand("already set: @once".to_owned())
    );
}

#[test]
fn buffer_limit_is_a_validated_global_server_option() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    let changed = engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "buffer-limit", "3"]),
        )
        .expect("set buffer limit");
    assert_eq!(
        changed.effects,
        [
            MuxEffect::BufferLimitChanged(3),
            MuxEffect::MuxOptionChanged {
                option: MuxOptionKey::BufferLimit,
                session: None,
            },
        ]
    );
    assert_eq!(engine.buffer_limit(), 3);

    let reset = engine
        .execute(
            &mut context,
            &command("set-option", &["-gu", "buffer-limit"]),
        )
        .expect("reset buffer limit");
    assert_eq!(
        reset.effects,
        [
            MuxEffect::BufferLimitChanged(DEFAULT_BUFFER_LIMIT),
            MuxEffect::MuxOptionChanged {
                option: MuxOptionKey::BufferLimit,
                session: None,
            },
        ]
    );
    assert_eq!(engine.buffer_limit(), DEFAULT_BUFFER_LIMIT);

    assert!(
        engine
            .execute(&mut context, &command("set-option", &["buffer-limit", "0"]),)
            .is_err()
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-p", "buffer-limit", "2"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-window-option", &["buffer-limit", "3"]),
        )
        .unwrap();
    assert_eq!(engine.buffer_limit(), 3);
}

#[test]
fn message_limit_is_a_live_server_option() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    assert_eq!(engine.message_limit(), DEFAULT_MESSAGE_LIMIT);
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("set-option", &["-s", "message-limit", "3"]),
            )
            .unwrap(),
        Execution::default()
    );
    assert_eq!(engine.message_limit(), 3);
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "message-limit"]),
            )
            .unwrap()
            .output,
        "3"
    );

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("set-option", &["-su", "message-limit"]),
            )
            .unwrap(),
        Execution::default()
    );
    assert_eq!(engine.message_limit(), DEFAULT_MESSAGE_LIMIT);
}

#[test]
fn history_trickle_is_a_validated_global_server_option() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    let changed = engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "history-trickle", "500"]),
        )
        .expect("set history trickle");
    assert_eq!(
        changed.effects,
        [MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::HistoryTrickle,
            session: None,
        }]
    );
    assert_eq!(engine.history_trickle(), 500);
    assert_eq!(engine.mux_option_value(MuxOptionKey::HistoryTrickle), "500");

    let disabled = engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "history-trickle", "0"]),
        )
        .expect("disable history trickle");
    assert_eq!(
        disabled.effects,
        [MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::HistoryTrickle,
            session: None,
        }]
    );
    assert_eq!(engine.history_trickle(), 0);

    engine
        .execute(
            &mut context,
            &command("set-option", &["-gu", "history-trickle"]),
        )
        .expect("reset history trickle");
    assert_eq!(engine.history_trickle(), DEFAULT_HISTORY_TRICKLE);

    for command in [
        command(
            "set-option",
            &["history-trickle", &(MAX_HISTORY_TRICKLE + 1).to_string()],
        ),
        command("set-option", &["-p", "history-trickle", "2"]),
        command("set-window-option", &["history-trickle", "2"]),
    ] {
        assert!(engine.execute(&mut context, &command).is_err());
    }
}

#[test]
fn history_limit_inherits_by_session_and_only_configures_new_panes() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "history-limit", "7"]),
        )
        .expect("global history limit");
    engine
        .execute(&mut context, &command("new-session", &["-s", "first"]))
        .expect("first session");
    let first_session = context.session.expect("first session id");
    let first_pane = context.pane.expect("first pane");
    assert_eq!(engine.history_limit_for_pane(first_pane).unwrap(), 7);

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-t", &first_session.to_string(), "history-limit", "3"],
            ),
        )
        .expect("session history limit");
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .expect("session pane");
    let overridden_pane = context.pane.expect("overridden pane");
    assert_eq!(engine.history_limit_for_pane(overridden_pane).unwrap(), 3);

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "history-limit", "9"]),
        )
        .expect("update global limit");
    assert_eq!(engine.history_limit_for_session(first_session), 3);
    engine
        .execute(&mut context, &command("new-session", &["-s", "second"]))
        .expect("second session");
    assert_eq!(
        engine
            .history_limit_for_pane(context.pane.unwrap())
            .unwrap(),
        9
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-u", "-t", &first_session.to_string(), "history-limit"],
            ),
        )
        .expect("restore session inheritance");
    assert_eq!(engine.history_limit_for_session(first_session), 9);

    let invalid = command(
        "set-option",
        &["-g", "history-limit", &(MAX_HISTORY_LIMIT + 1).to_string()],
    );
    assert!(engine.execute(&mut context, &invalid).is_err());
}

#[test]
fn aggressive_resize_stores_inherits_and_unsets_window_values() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let window = context.window.unwrap();

    let global = engine
        .execute(
            &mut context,
            &command("set-window-option", &["-g", "aggressive-resize", "on"]),
        )
        .unwrap();
    assert_eq!(
        global.effects,
        vec![
            MuxEffect::AggressiveResizeChanged { window: None },
            MuxEffect::SnapshotChanged,
        ]
    );
    assert!(engine.state.global_aggressive_resize());
    assert!(engine.state.window_aggressive_resize(window).unwrap());
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-window-options", &["-gv", "aggressive-resize"]),
            )
            .unwrap()
            .output,
        "on"
    );

    let local = engine
        .execute(
            &mut context,
            &command("set-window-option", &["aggressive-resize", "off"]),
        )
        .unwrap();
    assert_eq!(
        local.effects,
        vec![
            MuxEffect::AggressiveResizeChanged {
                window: Some(window),
            },
            MuxEffect::SnapshotChanged,
        ]
    );
    assert!(!engine.state.window_aggressive_resize(window).unwrap());
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-window-options", &["-v", "aggressive-resize"]),
            )
            .unwrap()
            .output,
        "off"
    );

    engine
        .execute(
            &mut context,
            &command("set-window-option", &["-u", "aggressive-resize"]),
        )
        .unwrap();
    assert!(engine.state.window_aggressive_resize(window).unwrap());
    assert_eq!(
        engine
            .state
            .window_aggressive_resize_override(window)
            .unwrap(),
        None
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-wAv", "aggressive-resize"]),
            )
            .unwrap()
            .output,
        "on"
    );

    let error = engine
        .execute(
            &mut context,
            &command("set-window-option", &["aggressive-resize", "maybe"]),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ServerError::InvalidCommand(message) if message == "bad value: maybe"
    ));
}

#[test]
fn behavior_options_store_inherit_unset_and_validate_typed_values() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();

    for args in [
        &["mouse", "on"] as &[&str],
        &["display-time", "1200"],
        &["initial-repeat-time", "900"],
        &["repeat-time", "650"],
    ] {
        engine
            .execute(&mut context, &command("set-option", args))
            .unwrap();
    }
    for (name, expected) in [
        ("mouse", "on"),
        ("display-time", "1200"),
        ("initial-repeat-time", "900"),
        ("repeat-time", "650"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", &["-v", name]),)
                .unwrap()
                .output,
            expected
        );
    }
    assert_eq!(engine.initial_repeat_time_for_session(session), 900);
    assert_eq!(engine.repeat_time_for_session(session), 650);

    engine
        .execute(
            &mut context,
            &command("set-window-option", &["automatic-rename", "off"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["automatic-rename-format", "#{pane_title}"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-window-option", &["remain-on-exit", "on"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-p", "remain-on-exit", "failed"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-pv", "remain-on-exit"]),
            )
            .unwrap()
            .output,
        "failed"
    );
    let pane = context.pane.unwrap();
    assert!(!engine.retain_exited_pane(pane, false).unwrap());
    assert!(engine.retain_exited_pane(pane, true).unwrap());
    engine
        .execute(
            &mut context,
            &command("set-option", &["-pu", "remain-on-exit"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-pA", "remain-on-exit"]),
            )
            .unwrap()
            .output,
        "remain-on-exit* on"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-p", "remain-on-exit", "key"]),
        )
        .unwrap();
    engine.state.mark_pane_dead(pane, Some(0)).unwrap();
    assert!(engine.dead_pane_dismisses_on_key(pane));
    engine.state.revive_pane(pane).unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-p", "remain-on-exit", "failed-key"]),
        )
        .unwrap();
    assert!(!engine.retain_exited_pane(pane, false).unwrap());
    assert!(engine.retain_exited_pane(pane, true).unwrap());
    engine.state.mark_pane_dead(pane, Some(1)).unwrap();
    assert!(engine.dead_pane_dismisses_on_key(pane));
    engine.state.revive_pane(pane).unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-p", "remain-on-exit"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-pv", "remain-on-exit"]),
            )
            .unwrap()
            .output,
        "failed-key"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-pu", "remain-on-exit"]),
        )
        .unwrap();

    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "default-terminal", "zz-term"]),
        )
        .unwrap();
    assert_eq!(engine.default_terminal_for_spawn(), "zz-term");
    engine
        .execute(
            &mut context,
            &command("set-option", &["-su", "default-terminal"]),
        )
        .unwrap();
    assert_eq!(engine.default_terminal_for_spawn(), "tmux-256color");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "default-terminal"]),
            )
            .unwrap()
            .output,
        "tmux-256color"
    );

    for (args, expected) in [
        (vec!["-g", "mouse", "maybe"], "bad value: maybe"),
        (
            vec!["-gw", "remain-on-exit", "maybe"],
            "unknown value: maybe",
        ),
        (vec!["-s", "escape-time", "-1"], "value is too small: -1"),
        (
            vec!["-g", "initial-repeat-time", "2000001"],
            "value is too large: 2000001",
        ),
        (
            vec!["-g", "repeat-time", "2000001"],
            "value is too large: 2000001",
        ),
    ] {
        assert!(matches!(
            engine.execute(&mut context, &command("set-option", &args)),
            Err(ServerError::InvalidCommand(message)) if message == expected
        ));
    }

    for name in [
        "mouse",
        "display-time",
        "initial-repeat-time",
        "repeat-time",
    ] {
        engine
            .execute(&mut context, &command("set-option", &["-u", name]))
            .unwrap();
    }
    assert!(engine.mouse_for_session(session));
    assert_eq!(engine.display_time_for_session(session), 750);
    assert_eq!(engine.initial_repeat_time_for_session(session), 0);
    assert_eq!(engine.repeat_time_for_session(session), 500);
}

#[test]
fn bare_flag_and_choice_options_toggle_like_the_pin() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    for expected in ["off", "on"] {
        engine
            .execute(&mut context, &command("set-option", &["-g", "mouse"]))
            .unwrap();
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", &["-gv", "mouse"]))
                .unwrap()
                .output,
            expected
        );
    }

    for expected in ["on", "off"] {
        engine
            .execute(
                &mut context,
                &command("set-window-option", &["-g", "remain-on-exit"]),
            )
            .unwrap();
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command("show-window-options", &["-gv", "remain-on-exit"]),
                )
                .unwrap()
                .output,
            expected
        );
    }
    engine
        .execute(
            &mut context,
            &command("set-window-option", &["-g", "remain-on-exit", "failed"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-window-option", &["-g", "remain-on-exit"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-window-options", &["-gv", "remain-on-exit"]),
            )
            .unwrap()
            .output,
        "failed"
    );

    for expected in ["on", "off"] {
        engine
            .execute(
                &mut context,
                &command("set-window-option", &["-g", "allow-passthrough"]),
            )
            .unwrap();
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command("show-window-options", &["-gv", "allow-passthrough"],),
                )
                .unwrap()
                .output,
            expected
        );
    }
    engine
        .execute(
            &mut context,
            &command("set-window-option", &["-g", "allow-passthrough", "all"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-window-option", &["-g", "allow-passthrough"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-window-options", &["-gv", "allow-passthrough"],),
            )
            .unwrap()
            .output,
        "all"
    );

    for expected in ["blinking-block", "default"] {
        engine
            .execute(
                &mut context,
                &command("set-window-option", &["-g", "cursor-style"]),
            )
            .unwrap();
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command("show-window-options", &["-gv", "cursor-style"]),
                )
                .unwrap()
                .output,
            expected
        );
    }
    engine
        .execute(
            &mut context,
            &command("set-window-option", &["-g", "cursor-style", "bar"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-window-option", &["-g", "cursor-style"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-window-options", &["-gv", "cursor-style"]),
            )
            .unwrap()
            .output,
        "bar"
    );

    for expected in ["off", "external"] {
        engine
            .execute(
                &mut context,
                &command("set-option", &["-s", "set-clipboard"]),
            )
            .unwrap();
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command("show-options", &["-sv", "set-clipboard"]),
                )
                .unwrap()
                .output,
            expected
        );
    }
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "set-clipboard", "on"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "set-clipboard"]),
        )
        .unwrap();
    assert_eq!(engine.mux_option_value(MuxOptionKey::SetClipboard), "on");

    engine
        .execute(&mut context, &command("set-option", &["-g", "status", "2"]))
        .unwrap();
    engine
        .execute(&mut context, &command("set-option", &["-g", "status"]))
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["-gv", "status"]))
            .unwrap()
            .output,
        "2"
    );
}

#[test]
fn honest_knob_families_store_inherit_read_back_and_reject_pin_values() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    let window = context.window.unwrap();
    let pane = context.pane.unwrap();

    for (args, expected) in [
        (vec!["-sv", "focus-events"], "off"),
        (vec!["-sv", "history-file"], "\n"),
        (vec!["-sv", "prefix-timeout"], "0"),
        (vec!["-sv", "prompt-history-limit"], "100"),
        (vec!["-gv", "bell-action"], "any"),
        (vec!["-gv", "default-size"], "80x24"),
        (vec!["-gv", "display-panes-time"], "1000"),
        (vec!["-gv", "key-table"], "root"),
        (vec!["-gv", "visual-bell"], "off"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", &args))
                .unwrap()
                .output,
            expected
        );
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-s", "history-file"]),
            )
            .unwrap()
            .output,
        "history-file ''"
    );
    for (name, expected) in [
        ("main-pane-height", "24"),
        ("main-pane-width", "80"),
        ("other-pane-height", "0"),
        ("other-pane-width", "0"),
        ("tiled-layout-max-columns", "0"),
        ("window-size", "latest"),
        ("wrap-search", "on"),
        ("allow-passthrough", "off"),
        ("allow-rename", "off"),
        ("allow-set-title", "on"),
        ("alternate-screen", "on"),
        ("cursor-colour", "\n"),
        ("cursor-style", "default"),
        ("scroll-on-clear", "on"),
    ] {
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command("show-window-options", &["-gv", name]),
                )
                .unwrap()
                .output,
            expected
        );
    }

    for args in [
        &["-s", "focus-events", "on"] as &[&str],
        &["-s", "history-file", "/tmp/history"],
        &["-s", "prefix-timeout", "250"],
        &["-s", "prompt-history-limit", "3"],
        &["bell-action", "other"],
        &["default-size", "132x43"],
        &["display-panes-time", "1400"],
        &["key-table", "custom"],
        &["visual-bell", "both"],
    ] {
        engine
            .execute(&mut context, &command("set-option", args))
            .unwrap();
    }
    for args in [
        &["main-pane-height", "40%"] as &[&str],
        &["main-pane-width", "120"],
        &["other-pane-height", "8"],
        &["other-pane-width", "12%"],
        &["tiled-layout-max-columns", "2"],
        &["window-size", "largest"],
        &["wrap-search", "off"],
    ] {
        engine
            .execute(&mut context, &command("set-window-option", args))
            .unwrap();
    }
    for args in [
        &["-p", "allow-passthrough", "all"] as &[&str],
        &["-p", "allow-rename", "on"] as &[&str],
        &["-p", "allow-set-title", "off"],
        &["-p", "alternate-screen", "off"],
        &["-p", "cursor-colour", "sky blue"],
        &["-p", "cursor-style", "blinking-underline"],
        &["-p", "scroll-on-clear", "off"],
    ] {
        engine
            .execute(&mut context, &command("set-option", args))
            .unwrap();
    }

    assert!(engine.focus_events());
    assert_eq!(engine.history_file(), "/tmp/history");
    assert_eq!(engine.prefix_timeout_ms(), 250);
    assert_eq!(engine.prompt_history_limit(), 3);
    assert_eq!(engine.bell_action_for_session(session), BellAction::Other);
    assert_eq!(engine.visual_bell_for_session(session), VisualBell::Both);
    assert_eq!(engine.key_table_for_session(session), "custom");
    assert_eq!(engine.display_panes_options(window).time_ms, 1400);
    assert_eq!(engine.window_size(window), WindowSize::Largest);
    assert!(!engine.allow_set_title(pane));
    assert_eq!(
        engine.terminal_worker_options_for_pane(pane).unwrap(),
        TerminalWorkerOptions {
            allow_passthrough: true,
            allow_rename: true,
            alternate_screen: false,
            scroll_on_clear: false,
            erase_byte: Some(0x7f),
            verase_byte: 0x7f,
            mode_keys_vi: false,
            remain_on_exit_format: concat!(
                "Pane is dead (#{?#{!=:#{pane_dead_status},},",
                "status #{pane_dead_status},}",
                "#{?#{!=:#{pane_dead_signal},},signal #{pane_dead_signal},}, ",
                "#{t:pane_dead_time})",
            )
            .to_owned(),
            wrap_search: false,
            cursor_style: "blinking-underline",
            cursor_colour: "sky blue".to_owned(),
            window_style: None,
            window_active_style: None,
            active_pane: true,
            pane_colours: Vec::new(),
        }
    );
    assert_eq!(
        engine.preset_options_for_window(window).main_pane_height,
        "40%"
    );

    for (args, expected) in [
        (vec!["-s", "focus-events", "maybe"], "bad value: maybe"),
        (vec!["-s", "prefix-timeout"], "empty value"),
        (
            vec!["-s", "prompt-history-limit", "-1"],
            "value is too small: -1",
        ),
        (vec!["-g", "bell-action", "maybe"], "unknown value: maybe"),
        (vec!["-g", "default-size", "bad"], "value is invalid: bad"),
        (
            vec!["-g", "display-panes-time", "0"],
            "value is too small: 0",
        ),
        (vec!["-g", "visual-bell", "maybe"], "unknown value: maybe"),
        (
            vec!["-gw", "tiled-layout-max-columns", "65536"],
            "value is too large: 65536",
        ),
        (vec!["-gw", "window-size", "maybe"], "unknown value: maybe"),
        (vec!["-gw", "wrap-search", "maybe"], "bad value: maybe"),
        (
            vec!["-gp", "allow-passthrough", "maybe"],
            "unknown value: maybe",
        ),
        (vec!["-gp", "allow-set-title", "maybe"], "bad value: maybe"),
        (vec!["-gp", "alternate-screen", "maybe"], "bad value: maybe"),
        (
            vec!["-gp", "cursor-colour", "not-a-colour"],
            "invalid colour: not-a-colour",
        ),
        (vec!["-gp", "cursor-colour"], "empty value"),
        (
            vec!["-gp", "cursor-style", "Blinking-Bar"],
            "unknown value: Blinking-Bar",
        ),
        (vec!["-gp", "scroll-on-clear", "maybe"], "bad value: maybe"),
    ] {
        assert!(matches!(
            engine.execute(&mut context, &command("set-option", &args)),
            Err(ServerError::InvalidCommand(message)) if message == expected
        ));
    }

    engine
        .execute(&mut context, &command("set-option", &["-u", "key-table"]))
        .unwrap();
    assert_eq!(engine.key_table_for_session(session), "root");
}

#[test]
fn lane2_scalar_families_store_read_back_unset_and_validate() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "lane2"]))
        .unwrap();
    for (flags, name, expected) in [
        ("-sv", "backspace", "C-?"),
        ("-sv", "default-client-command", "new-session"),
        ("-sv", "editor", "/usr/bin/vi"),
        ("-sv", "extended-keys", "off"),
        ("-sv", "extended-keys-format", "xterm"),
        ("-sv", "get-clipboard", "buffer"),
        ("-sv", "input-buffer-size", "1048576"),
        ("-sv", "variation-selector-always-wide", "on"),
        ("-gv", "assume-paste-time", "1"),
        ("-gv", "message-line", "0"),
        ("-gv", "prompt-cursor-colour", "\n"),
        ("-gv", "prompt-command-cursor-colour", "\n"),
        ("-gv", "prompt-cursor-style", "default"),
        ("-gv", "prompt-command-cursor-style", "default"),
        ("-gwv", "clock-mode-colour", "themeblue"),
        ("-gwv", "clock-mode-style", "24"),
        (
            "-gwv",
            "fill-character",
            crate::tmux_options::FILL_CHARACTER_DEFAULT,
        ),
        ("-gwv", "pane-border-indicators", "colour"),
        ("-gwv", "pane-border-lines", "single"),
        ("-gwv", "pane-scrollbars", "off"),
        ("-gwv", "pane-scrollbars-timeout", "500"),
        (
            "-gwv",
            "pane-scrollbars-style",
            PANE_SCROLLBARS_STYLE_DEFAULT,
        ),
        ("-gwv", "pane-scrollbars-position", "right"),
        ("-gwv", "xterm-keys", "on"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", &[flags, name]))
                .unwrap()
                .output,
            expected,
            "{name}"
        );
    }
    for (name, expected) in [
        ("message-command-style", MESSAGE_COMMAND_STYLE_DEFAULT),
        ("message-format", MESSAGE_FORMAT_DEFAULT),
        ("message-style", MESSAGE_STYLE_DEFAULT),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", &["-gv", name]))
                .unwrap()
                .output,
            expected,
            "{name}"
        );
    }

    for args in [
        &["-s", "backspace", "BSpace"] as &[&str],
        &["-s", "default-client-command", "neww -d"],
        &["-s", "editor", "nvim"],
        &["-s", "extended-keys", "always"],
        &["-s", "extended-keys-format", "csi-u"],
        &["-s", "get-clipboard", "both"],
        &["-s", "input-buffer-size", "1048577"],
        &["-s", "variation-selector-always-wide", "off"],
        &["-g", "assume-paste-time", "9"],
        &["-g", "message-command-style", "fg=red"],
        &["-g", "message-format", "message"],
        &["-g", "message-line", "4"],
        &["-g", "message-style", "bg=blue"],
        &["-g", "prompt-cursor-colour", "red"],
        &["-g", "prompt-command-cursor-colour", "blue"],
        &["-g", "prompt-cursor-style", "block"],
        &["-g", "prompt-command-cursor-style", "bar"],
        &["-gw", "clock-mode-colour", "cyan"],
        &["-gw", "clock-mode-style", "12-with-seconds"],
        &["-gw", "fill-character", "x"],
        &["-gw", "pane-border-indicators", "arrows"],
        &["-gw", "pane-border-lines", "heavy"],
        &["-gw", "pane-scrollbars", "auto-hide"],
        &["-gw", "pane-scrollbars-timeout", "750"],
        &["-gw", "pane-scrollbars-style", "fg=white"],
        &["-gw", "pane-scrollbars-position", "left"],
        &["-gw", "xterm-keys", "off"],
    ] {
        engine
            .execute(&mut context, &command("set-option", args))
            .unwrap();
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "default-client-command"]),
            )
            .unwrap()
            .output,
        "new-window -d"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-ag", "message-style", "bold"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gv", "message-style"]),
            )
            .unwrap()
            .output,
        "bg=blue,bold"
    );
    engine
        .execute(&mut context, &command("set-option", &["-su", "editor"]))
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["-sv", "editor"]))
            .unwrap()
            .output,
        "/usr/bin/vi"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-p", "pane-border-lines", "double"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-wU", "pane-border-lines"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-pA", "pane-border-lines"]),
            )
            .unwrap()
            .output,
        "pane-border-lines* heavy"
    );

    for (args, expected) in [
        (
            &["-s", "backspace", "not-a-key"] as &[&str],
            "bad key: not-a-key",
        ),
        (
            &["-s", "extended-keys", "sometimes"],
            "unknown value: sometimes",
        ),
        (
            &["-s", "input-buffer-size", "1048575"],
            "value is too small: 1048575",
        ),
        (
            &["-s", "variation-selector-always-wide", "maybe"],
            "bad value: maybe",
        ),
        (
            &["-g", "prompt-cursor-colour", "not-a-colour"],
            "invalid colour: not-a-colour",
        ),
        (
            &["-gw", "pane-scrollbars-style", "not-a-style"],
            "invalid style: not-a-style",
        ),
        (
            &["-gw", "pane-scrollbars-timeout", "nope"],
            "value is invalid: nope",
        ),
    ] {
        assert!(matches!(
            engine.execute(&mut context, &command("set-option", args)),
            Err(ServerError::InvalidCommand(message)) if message == expected
        ));
    }
}

#[test]
fn every_remaining_named_scalar_stores_and_bare_listings_cover_the_pin_table() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-s", "all-options"]),
        )
        .unwrap();

    let storage_only = tmux_options()
        .filter(|option| tmux_stored_scalar(option.name).is_some())
        .collect::<Vec<_>>();
    assert_eq!(storage_only.len(), 66);
    for option in storage_only {
        let metadata = tmux_stored_scalar(option.name).expect("storage-only metadata");
        let alternate = match metadata.kind {
            TmuxStoredScalarKind::String => "stored",
            TmuxStoredScalarKind::Style => "fg=red",
            TmuxStoredScalarKind::Colour => "red",
            TmuxStoredScalarKind::Flag => {
                if metadata.default == "on" {
                    "off"
                } else {
                    "on"
                }
            }
            TmuxStoredScalarKind::Choice(choices) => choices
                .iter()
                .copied()
                .find(|choice| *choice != metadata.default)
                .expect("choice has an alternate"),
            TmuxStoredScalarKind::Key => "C-a",
        };
        let set_flag = match option.scope {
            TmuxOptionScope::Server => "-s",
            TmuxOptionScope::Session => "-g",
            TmuxOptionScope::Window | TmuxOptionScope::WindowPane => "-gw",
        };
        engine
            .execute(
                &mut context,
                &command("set-option", &[set_flag, option.name, alternate]),
            )
            .unwrap_or_else(|error| panic!("{}: {error}", option.name));
        let show_flag = match option.scope {
            TmuxOptionScope::Server => "-sv",
            TmuxOptionScope::Session => "-gv",
            TmuxOptionScope::Window | TmuxOptionScope::WindowPane => "-gwv",
        };
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command("show-options", &[show_flag, option.name]),
                )
                .unwrap()
                .output,
            alternate,
            "{}",
            option.name
        );
        let unset_flag = match option.scope {
            TmuxOptionScope::Server => "-su",
            TmuxOptionScope::Session => "-gu",
            TmuxOptionScope::Window | TmuxOptionScope::WindowPane => "-gwu",
        };
        engine
            .execute(
                &mut context,
                &command("set-option", &[unset_flag, option.name]),
            )
            .unwrap();
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command("show-options", &[show_flag, option.name]),
                )
                .unwrap()
                .output,
            metadata.default,
            "{}",
            option.name
        );
    }

    let mut listed = BTreeSet::new();
    for flags in [["-s"].as_slice(), ["-g"].as_slice(), ["-gw"].as_slice()] {
        for line in engine
            .execute(&mut context, &command("show-options", flags))
            .unwrap()
            .output
            .lines()
        {
            let name = line
                .split_ascii_whitespace()
                .next()
                .expect("listed option name")
                .split('[')
                .next()
                .expect("base option name");
            listed.insert(name.to_owned());
        }
    }
    let expected = tmux_options()
        .filter(|option| !tmux_option_is_hook(option.name))
        .map(|option| option.name)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    assert_eq!(listed, expected);

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "destroy-unattached", "keep-last"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["destroy-unattached", "on"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-v", "destroy-unattached"]),
            )
            .unwrap()
            .output,
        "on"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-u", "destroy-unattached"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-Av", "destroy-unattached"]),
            )
            .unwrap()
            .output,
        "keep-last"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-gw", "window-style", "fg=red"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-p", "window-style", "fg=blue"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-wU", "window-style"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-pAv", "window-style"]),
            )
            .unwrap()
            .output,
        "fg=red"
    );

    for (args, expected) in [
        (
            &["-s", "theme", "automatic"] as &[&str],
            "unknown value: automatic",
        ),
        (
            &["-g", "display-panes-colour", "not-a-colour"],
            "invalid colour: not-a-colour",
        ),
        (
            &["-gw", "copy-mode-match-style", "not-a-style"],
            "invalid style: not-a-style",
        ),
        (&["-g", "prefix2", "not-a-key"], "bad key: not-a-key"),
    ] {
        assert!(matches!(
            engine.execute(&mut context, &command("set-option", args)),
            Err(ServerError::InvalidCommand(message)) if message == expected
        ));
    }
}

#[test]
fn monitor_options_store_read_back_unset_validate_and_emit_the_silence_effect() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");

    for (flags, name, expected) in [
        ("-gv", "activity-action", "other"),
        ("-gv", "silence-action", "other"),
        ("-gwv", "monitor-activity", "off"),
        ("-gwv", "monitor-bell", "on"),
        ("-gwv", "monitor-silence", "0"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", &[flags, name]))
                .unwrap()
                .output,
            expected,
            "{name}"
        );
    }

    for args in [
        &["-g", "activity-action", "any"] as &[&str],
        &["-g", "silence-action", "none"],
        &["-gw", "monitor-activity", "on"],
        &["-gw", "monitor-bell", "off"],
    ] {
        let execution = engine
            .execute(&mut context, &command("set-option", args))
            .unwrap();
        assert!(execution.effects.is_empty());
    }
    let silence = engine
        .execute(
            &mut context,
            &command("set-option", &["-gw", "monitor-silence", "30"]),
        )
        .unwrap();
    assert_eq!(silence.effects, [MuxEffect::MonitorSilenceChanged]);
    let repeated = engine
        .execute(
            &mut context,
            &command("set-option", &["-gw", "monitor-silence", "30"]),
        )
        .expect("repeat monitor-silence");
    assert_eq!(repeated.effects, [MuxEffect::MonitorSilenceChanged]);
    let quiet_global = engine
        .execute(
            &mut context,
            &command("set-option", &["-gwoq", "monitor-silence", "30"]),
        )
        .expect("quiet existing global monitor-silence");
    assert!(quiet_global.effects.is_empty());
    assert!(
        engine
            .execute(
                &mut context,
                &command("set-option", &["-gwo", "monitor-silence", "30"]),
            )
            .is_err()
    );
    for args in [
        &["-w", "monitor-silence", "30"] as &[&str],
        &["-w", "monitor-silence", "30"],
    ] {
        let execution = engine
            .execute(&mut context, &command("set-option", args))
            .expect("write local monitor-silence");
        assert_eq!(execution.effects, [MuxEffect::MonitorSilenceChanged]);
    }
    let quiet_local = engine
        .execute(
            &mut context,
            &command("set-option", &["-woq", "monitor-silence", "30"]),
        )
        .expect("quiet existing local monitor-silence");
    assert!(quiet_local.effects.is_empty());
    assert!(
        engine
            .execute(
                &mut context,
                &command("set-option", &["-woU", "monitor-silence"]),
            )
            .is_err()
    );
    let uppercase_removed = engine
        .execute(
            &mut context,
            &command("set-option", &["-wU", "monitor-silence"]),
        )
        .expect("uppercase unset existing local monitor-silence");
    assert_eq!(
        uppercase_removed.effects,
        [MuxEffect::MonitorSilenceChanged]
    );
    let uppercase_missing = engine
        .execute(
            &mut context,
            &command("set-option", &["-wU", "monitor-silence"]),
        )
        .expect("uppercase unset missing local monitor-silence");
    assert!(uppercase_missing.effects.is_empty());
    let created_equal = engine
        .execute(
            &mut context,
            &command("set-option", &["-wo", "monitor-silence", "30"]),
        )
        .expect("create equal local monitor-silence");
    assert_eq!(created_equal.effects, [MuxEffect::MonitorSilenceChanged]);
    let removed_equal = engine
        .execute(
            &mut context,
            &command("set-option", &["-wu", "monitor-silence"]),
        )
        .expect("remove equal local monitor-silence");
    assert_eq!(removed_equal.effects, [MuxEffect::MonitorSilenceChanged]);
    for (flags, name, expected) in [
        ("-gv", "activity-action", "any"),
        ("-gv", "silence-action", "none"),
        ("-gwv", "monitor-activity", "on"),
        ("-gwv", "monitor-bell", "off"),
        ("-gwv", "monitor-silence", "30"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", &[flags, name]))
                .unwrap()
                .output,
            expected,
            "{name}"
        );
    }

    for args in [
        &["-gu", "activity-action"] as &[&str],
        &["-gu", "silence-action"],
        &["-gwu", "monitor-activity"],
        &["-gwu", "monitor-bell"],
    ] {
        engine
            .execute(&mut context, &command("set-option", args))
            .unwrap();
    }
    let silence_unset = engine
        .execute(
            &mut context,
            &command("set-option", &["-gwu", "monitor-silence"]),
        )
        .expect("unset global monitor-silence");
    assert_eq!(silence_unset.effects, [MuxEffect::MonitorSilenceChanged]);
    let repeated_silence_unset = engine
        .execute(
            &mut context,
            &command("set-option", &["-gwu", "monitor-silence"]),
        )
        .expect("repeat global monitor-silence reset");
    assert_eq!(
        repeated_silence_unset.effects,
        [MuxEffect::MonitorSilenceChanged]
    );
    let uppercase_global_unset = engine
        .execute(
            &mut context,
            &command("set-option", &["-gwU", "monitor-silence"]),
        )
        .expect("uppercase unset global monitor-silence");
    assert_eq!(
        uppercase_global_unset.effects,
        [MuxEffect::MonitorSilenceChanged]
    );
    for (flags, name, expected) in [
        ("-gv", "activity-action", "other"),
        ("-gv", "silence-action", "other"),
        ("-gwv", "monitor-activity", "off"),
        ("-gwv", "monitor-bell", "on"),
        ("-gwv", "monitor-silence", "0"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", &[flags, name]))
                .unwrap()
                .output,
            expected,
            "{name}"
        );
    }

    for args in [
        &["-g", "activity-action", "all"] as &[&str],
        &["-g", "silence-action", "ALL"],
        &["-gw", "monitor-activity", "maybe"],
        &["-gw", "monitor-bell", "maybe"],
        &["-gw", "monitor-silence", "-1"],
    ] {
        assert!(
            engine
                .execute(&mut context, &command("set-option", args))
                .is_err()
        );
    }
}

#[test]
fn default_shell_is_concrete_validated_and_unsets_to_the_table_default() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gv", "default-shell"]),
            )
            .unwrap()
            .output,
        "/bin/sh"
    );
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    let mut hooks = CommandHooks::new(0);
    let mut valid = |shell: &str| matches!(shell, "/bin/sh" | "/valid-shell");

    engine
        .execute_with_shell_validator(
            &mut context,
            &command("set-option", &["-g", "default-shell", "/valid-shell"]),
            &mut hooks,
            &mut valid,
        )
        .unwrap();
    assert_eq!(
        engine.default_shell_for_session(session).unwrap(),
        "/valid-shell"
    );

    assert!(matches!(
        engine.execute_with_shell_validator(
            &mut context,
            &command("set-option", &["-g", "default-shell", "/invalid"]),
            &mut hooks,
            &mut valid,
        ),
        Err(ServerError::InvalidCommand(message)) if message == "not a suitable shell: /invalid"
    ));
    assert_eq!(engine.global_default_shell(), "/valid-shell");

    assert!(matches!(
        engine.execute_with_shell_validator(
            &mut context,
            &command("set-option", &["-ga", "default-shell", "-invalid"]),
            &mut hooks,
            &mut valid,
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "not a suitable shell: /valid-shell-invalid"
    ));
    assert_eq!(engine.global_default_shell(), "/valid-shell");

    engine
        .execute_with_shell_validator(
            &mut context,
            &command("set-option", &["default-shell", "/bin/sh"]),
            &mut hooks,
            &mut valid,
        )
        .unwrap();
    assert_eq!(
        engine.default_shell_for_session(session).unwrap(),
        "/bin/sh"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-u", "default-shell"]),
        )
        .unwrap();
    assert_eq!(
        engine.default_shell_for_session(session).unwrap(),
        "/valid-shell"
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-gu", "default-shell"]),
        )
        .unwrap();
    assert_eq!(engine.global_default_shell(), "/bin/sh");
    assert_eq!(
        engine.default_shell_for_session(session).unwrap(),
        "/bin/sh"
    );
}

#[test]
fn session_spawn_string_append_uses_only_the_sessions_own_value() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "default-command", "AAA"]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("new-session", &["-s", "q"]))
        .unwrap();
    let session = context.session.unwrap();

    engine
        .execute(
            &mut context,
            &command("set-option", &["-at", "q", "default-command", "BBB"]),
        )
        .unwrap();
    assert_eq!(engine.default_command_for_session(session).unwrap(), "BBB");

    engine
        .execute(
            &mut context,
            &command("set-option", &["-t", "q", "default-command", "SSS"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-at", "q", "default-command", "BBB"]),
        )
        .unwrap();
    assert_eq!(
        engine.default_command_for_session(session).unwrap(),
        "SSSBBB"
    );

    let mut hooks = CommandHooks::new(0);
    let mut valid = |shell: &str| shell == "/bin";
    engine
        .execute_with_shell_validator(
            &mut context,
            &command("set-option", &["-g", "default-shell", "/bin"]),
            &mut hooks,
            &mut valid,
        )
        .unwrap();
    assert!(matches!(
        engine.execute_with_shell_validator(
            &mut context,
            &command("set-option", &["-at", "q", "default-shell", "/sh"]),
            &mut hooks,
            &mut valid,
        ),
        Err(ServerError::InvalidCommand(message)) if message == "not a suitable shell: /sh"
    ));
}

#[test]
fn status_options_store_inherit_unset_and_validate() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.expect("session");
    let window = context.window.expect("window");

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "status-left-length", "12"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-t", "work", "status-left-length", "5"]),
        )
        .unwrap();
    assert_eq!(
        engine.status_formats_for_session(Some(session)).left_length,
        5
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-ut", "work", "status-left-length"]),
        )
        .unwrap();
    assert_eq!(
        engine.status_formats_for_session(Some(session)).left_length,
        12
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-g", "window-status-format", "plain:#I:#W"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-t", "work:0", "window-status-current-format", "current:#W"],
            ),
        )
        .unwrap();
    let formats = engine.window_status_formats(window);
    assert_eq!(formats.format, "plain:#I:#W");
    assert_eq!(formats.current_format, "current:#W");

    for (option, value, error) in [
        ("status-style", "bogus", "invalid style: bogus"),
        ("status-bg", "bogus", "bad colour: bogus"),
        ("status-left-length", "-1", "value is too small: -1"),
        ("status-right-length", "32768", "value is too large: 32768"),
        ("status-justify", "middle", "unknown value: middle"),
        ("status-position", "middle", "unknown value: middle"),
    ] {
        assert!(matches!(
            engine.execute(
                &mut context,
                &command("set-option", &["-g", option, value]),
            ),
            Err(ServerError::InvalidCommand(message)) if message == error
        ));
    }
}

#[test]
fn user_options_keep_bytes_that_are_not_utf8() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let mut run = |name: &str, args: Vec<RawText>| {
        engine
            .execute(&mut context, &CommandInvocation::new(name, args))
            .unwrap()
            .output
            .as_bytes()
            .to_vec()
    };
    run(
        "set-option",
        vec![
            "-g".into(),
            "@bytes".into(),
            RawText::from_bytes(b"a\xfeb".to_vec()),
        ],
    );
    run(
        "set-option",
        vec![
            "-ga".into(),
            "@bytes".into(),
            RawText::from_bytes(b"\xfd".to_vec()),
        ],
    );
    assert_eq!(
        run("show-options", vec!["-g".into(), "@bytes".into()]),
        b"@bytes a\\376b\\375"
    );
    assert_eq!(
        run("show-options", vec!["-gv".into(), "@bytes".into()]),
        b"a\xfeb\xfd"
    );
    assert_eq!(
        run("show-options", vec!["-g".into()])
            .split(|byte| *byte == b'\n')
            .find(|line| line.starts_with(b"@bytes "))
            .expect("listed"),
        b"@bytes a\\376b\\375"
    );
}

#[test]
fn pane_user_option_reads_only_the_pane_scope() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let pane = context.pane.unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-w", "@agent_state", "working"]),
        )
        .unwrap();
    assert_eq!(engine.pane_user_option(pane, "@agent_state"), None);
    engine
        .execute(
            &mut context,
            &command("set-option", &["-p", "@agent_state", "done"]),
        )
        .unwrap();
    assert_eq!(engine.pane_user_option(pane, "@agent_state"), Some("done"));
    assert_eq!(engine.pane_user_option(PaneId(9999), "@agent_state"), None);
}

#[test]
fn user_options_are_exact_pure_storage_at_every_scope() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    for args in [
        &["-s", "@scope", "server"] as &[&str],
        &["-g", "@scope", "global-session"],
        &["@scope", "session"],
        &["-gw", "@scope", "global-window"],
        &["-w", "@scope", "window"],
        &["-p", "@scope", "pane"],
    ] {
        engine
            .execute(&mut context, &command("set-option", args))
            .unwrap();
    }
    for (args, expected) in [
        (&["-sv", "@scope"] as &[&str], "server"),
        (&["-gv", "@scope"], "global-session"),
        (&["-v", "@scope"], "session"),
        (&["-gwv", "@scope"], "global-window"),
        (&["-wv", "@scope"], "window"),
        (&["-pv", "@scope"], "pane"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", args))
                .unwrap()
                .output,
            expected
        );
    }

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "@plugin", "one two"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-ga", "@plugin", " three"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "@plug", "exact"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["-g", "@plugin"]),)
            .unwrap()
            .output,
        "@plugin \"one two three\""
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["-gv", "@plug"]),)
            .unwrap()
            .output,
        "exact"
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "@inherited", "parent"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-A", "@inherited"]),
            )
            .unwrap()
            .output,
        "@inherited* parent"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["@inherited", "child"]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("set-option", &["-u", "@inherited"]))
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-A", "@inherited"]),
            )
            .unwrap()
            .output,
        "@inherited* parent"
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["-q", "@missing"]),)
            .unwrap()
            .output,
        ""
    );
}

#[test]
fn set_option_command_block_values_normalize_before_format_expansion() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine.seed_global_environment([("FOO", "hello world")]);
    engine
        .execute(
            &mut context,
            &command("new-session", &["-s", "work", "-n", "main"]),
        )
        .expect("session");

    for (name, args, block, show, expected) in [
        (
            "set-option",
            vec!["-g", "@single", "{ display -p alias }"],
            2,
            vec!["-gv", "@single"],
            "display-message -p alias",
        ),
        (
            "set-window-option",
            vec!["-g", "@multi", "{ display-mes -p one ; splitw -d }"],
            2,
            vec!["-gv", "@multi"],
            "display-message -p one ; split-window -d",
        ),
        (
            "set-option",
            vec!["-g", "@empty", "{}"],
            2,
            vec!["-gv", "@empty"],
            "\n",
        ),
        (
            "set-option",
            vec![
                "-g",
                "@nested",
                "{ if -F 1 { display -p 'yes ok' } { display -p no } }",
            ],
            2,
            vec!["-gv", "@nested"],
            "if-shell -F 1 { display-message -p \"yes ok\" } { display-message -p no }",
        ),
        (
            "set-option",
            vec!["-g", "@nested-empty", "{ if-shell -F 1 {} {} }"],
            2,
            vec!["-gv", "@nested-empty"],
            "if-shell -F 1 {  } {  }",
        ),
        (
            "set-option",
            vec!["-g", "@groups", "{\n display -p one\n display -p two\n}"],
            2,
            vec!["-gv", "@groups"],
            "display-message -p one ;; display-message -p two",
        ),
        (
            "set-option",
            vec![
                "-gF",
                "@formatted",
                "{ display -p '#{session_name}:#{window_name}' }",
            ],
            2,
            vec!["-gv", "@formatted"],
            "display-message -p \"work:main\"",
        ),
        (
            "set-option",
            vec![
                "-g",
                "@unexpanded",
                "{ display -p '#{session_name}:#{window_name}' }",
            ],
            2,
            vec!["-gv", "@unexpanded"],
            "display-message -p \"#{session_name}:#{window_name}\"",
        ),
    ] {
        engine
            .execute(
                &mut context,
                &CommandInvocation::new(name, args).with_command_blocks([block]),
            )
            .expect("typed option value");
        let show_name = if name == "set-window-option" {
            "show-window-options"
        } else {
            "show-options"
        };
        assert_eq!(
            engine
                .execute(&mut context, &command(show_name, &show))
                .expect("typed option readback")
                .output,
            expected
        );
    }

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "@quoted", "{ display -p quoted }"]),
        )
        .expect("quoted braces");
    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["-gv", "@quoted"]),)
            .expect("quoted readback")
            .output,
        "{ display -p quoted }"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-s", "command-alias[80]", "flash=display-message -p"],
            ),
        )
        .expect("command alias");
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "@user-alias", "{ flash expanded }"])
                .with_command_blocks([2]),
        )
        .expect("typed user alias");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gv", "@user-alias"]),
            )
            .expect("user alias readback")
            .output,
        "display-message -p expanded"
    );

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "default-client-command",
                    "{ display -p client ; neww -d }",
                ],
            )
            .with_command_blocks([2]),
        )
        .expect("typed command option");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "default-client-command"]),
            )
            .expect("command option readback")
            .output,
        "display-message -p client ; new-window -d"
    );

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "default-client-command",
                    "{ display-message -p $FOO }",
                ],
            )
            .with_command_blocks([2]),
        )
        .expect("environment-backed command option");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "default-client-command"]),
            )
            .expect("environment-backed command option readback")
            .output,
        "display-message -p \"hello world\""
    );

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "default-client-command",
                    "{\n display -p typed-first\n display -p typed-second\n}",
                ],
            )
            .with_command_blocks([2]),
        )
        .expect("multiline typed command option");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "default-client-command"]),
            )
            .expect("multiline typed command readback")
            .output,
        "display-message -p typed-first ; display-message -p typed-second"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-s",
                    "default-client-command",
                    "display -p first\ndisplay -p second",
                ],
            ),
        )
        .expect("multiline string command option");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "default-client-command"]),
            )
            .expect("multiline command option readback")
            .output,
        "display-message -p first ; display-message -p second"
    );
}

#[test]
fn set_option_command_block_rejections_preserve_existing_state() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "@kept", "original"]),
        )
        .expect("baseline option");

    for (invocation, expected) in [
        (
            CommandInvocation::new(
                "set-option",
                ["-g", "{ set-option -g @kept forbidden }", "replacement"],
            )
            .with_command_blocks([1]),
            "command set-option: argument 1 must be \"string\"",
        ),
        (
            CommandInvocation::new(
                "set-window-option",
                [
                    "-t",
                    "{ set-option -g @kept forbidden }",
                    "@value",
                    "replacement",
                ],
            )
            .with_command_blocks([1]),
            "command set-window-option: -t argument must be a string",
        ),
        (
            CommandInvocation::new(
                "set-option",
                ["@value", "replacement", "{ set-option -g @kept forbidden }"],
            )
            .with_command_blocks([2]),
            "command set-option: argument 3 must be \"string\"",
        ),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &invocation)
                .expect_err("rejected typed argument")
                .tmux_message(),
            expected
        );
    }
    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["-gv", "@kept"]),)
            .expect("preserved option")
            .output,
        "original"
    );
}

#[test]
fn set_options_expand_names_and_dash_f_expands_values() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "@#{session_name}", "#{window_name}"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["-gv", "@work"]),)
            .unwrap()
            .output,
        "#{window_name}"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-Fg",
                    "@#{session_name}",
                    "#{session_name}:#{window_index}.#{pane_index}",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["-gv", "@work"]),)
            .unwrap()
            .output,
        "work:0.0"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &[
                    "-F",
                    "@window_#{window_index}",
                    "#{window_name}:#{pane_index}",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-window-options", &["-v", "@window_0"]),
            )
            .unwrap()
            .output,
        "0:0"
    );

    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "work:1", "-n", "logs"]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("split-window", &[]))
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-F", "-t", "1", "@target", "#{window_index}.#{pane_index}"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-window-options", &["-t", "1", "-v", "@target"]),
            )
            .unwrap()
            .output,
        "1.0"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-gF",
                    "-t",
                    "missing",
                    "@fallback",
                    "#{session_name}:#{window_index}.#{pane_index}",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gv", "@fallback"]),
            )
            .unwrap()
            .output,
        "work:0.1"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-gF", "history-trickle", "#{?session_name,2001,0}"],
            ),
        )
        .unwrap();
    assert_eq!(engine.history_trickle(), 2001);
}

#[test]
fn lifecycle_trio_accessors_report_only_explicit_writes() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let session = context.session.expect("session");

    assert_eq!(engine.exit_empty_explicit(), None);
    assert_eq!(engine.exit_unattached_explicit(), None);
    assert!(!engine.destroy_unattached_explicit_anywhere());
    assert_eq!(engine.destroy_unattached_policy_for_session(session), None);

    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "exit-empty", "on"]),
        )
        .expect("explicit default write");
    assert_eq!(engine.exit_empty_explicit(), Some(true));
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "exit-empty", "off"]),
        )
        .expect("explicit off write");
    assert_eq!(engine.exit_empty_explicit(), Some(false));
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "-u", "exit-empty"]),
        )
        .expect("unset");
    assert_eq!(engine.exit_empty_explicit(), None);

    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "exit-unattached", "on"]),
        )
        .expect("explicit write");
    assert_eq!(engine.exit_unattached_explicit(), Some(true));

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "destroy-unattached", "keep-last"]),
        )
        .expect("global write");
    assert!(engine.destroy_unattached_explicit_anywhere());
    assert_eq!(
        engine
            .destroy_unattached_policy_for_session(session)
            .as_deref(),
        Some("keep-last")
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-t", "work", "destroy-unattached", "on"]),
        )
        .expect("session write");
    assert_eq!(
        engine
            .destroy_unattached_policy_for_session(session)
            .as_deref(),
        Some("on")
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "-u", "destroy-unattached"]),
        )
        .expect("global unset");
    assert_eq!(
        engine
            .destroy_unattached_policy_for_session(session)
            .as_deref(),
        Some("on")
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-u", "-t", "work", "destroy-unattached"]),
        )
        .expect("session unset");
    assert_eq!(engine.destroy_unattached_policy_for_session(session), None);
    assert!(!engine.destroy_unattached_explicit_anywhere());
}

#[test]
fn table_option_scope_ignores_command_spelling_and_unrelated_scope_flags() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "first"]))
        .unwrap();
    let first_session = context.session.unwrap();
    let first_window = context.window.unwrap();
    let first_pane = context.pane.unwrap();
    engine
        .execute(&mut context, &command("new-session", &["-s", "second"]))
        .unwrap();
    let second_session = context.session.unwrap();
    let second_window = context.window.unwrap();
    let second_pane = context.pane.unwrap();

    engine
        .execute(&mut context, &command("setw", &["-g", "base-index", "1"]))
        .unwrap();
    assert_eq!(engine.base_index_for_session(first_session), 1);
    assert_eq!(engine.base_index_for_session(second_session), 1);
    for (flag, value) in [("-w", "2"), ("-s", "3"), ("-p", "4")] {
        engine
            .execute(&mut context, &command("set", &[flag, "base-index", value]))
            .unwrap();
    }
    engine
        .execute(&mut context, &command("setw", &["base-index", "5"]))
        .unwrap();
    assert_eq!(engine.base_index_for_session(first_session), 1);
    assert_eq!(engine.base_index_for_session(second_session), 5);

    for (flag, value) in [("-w", "2"), ("-s", "3"), ("-p", "4")] {
        engine
            .execute(
                &mut context,
                &command("set", &[flag, "pane-base-index", value]),
            )
            .unwrap();
    }
    engine
        .execute(&mut context, &command("setw", &["pane-base-index", "5"]))
        .unwrap();
    assert_eq!(engine.pane_index(second_window, second_pane), Some(5));

    engine
        .execute(
            &mut context,
            &command("set", &["-t", &first_pane.to_string(), "base-index", "7"]),
        )
        .unwrap();
    assert_eq!(engine.base_index_for_session(first_session), 7);
    engine
        .execute(
            &mut context,
            &command(
                "set",
                &["-s", "-t", &first_window.to_string(), "base-index", "8"],
            ),
        )
        .unwrap();
    assert_eq!(engine.base_index_for_session(first_session), 8);
    engine
        .execute(
            &mut context,
            &command(
                "set",
                &["-p", "-t", &first_pane.to_string(), "pane-base-index", "6"],
            ),
        )
        .unwrap();
    assert_eq!(engine.pane_index(first_window, first_pane), Some(6));
    engine
        .execute(
            &mut context,
            &command(
                "setw",
                &["-t", &first_window.to_string(), "pane-base-index", "7"],
            ),
        )
        .unwrap();
    assert_eq!(engine.pane_index(first_window, first_pane), Some(7));
}

#[test]
fn set_option_matches_full_table_prefixes_and_quiets_unknown_names() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();

    engine
        .execute(&mut context, &command("set", &["-g", "base-ind", "6"]))
        .unwrap();
    assert_eq!(engine.base_index_for_session(session), 6);

    let error = engine
        .execute(&mut context, &command("set", &["-g", "status-l", "value"]))
        .unwrap_err();
    assert!(matches!(
        error,
        ServerError::InvalidCommand(message) if message == "ambiguous option: status-l"
    ));
    engine
        .execute(&mut context, &command("set", &["-g", "escape-t", "17"]))
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "escape-time"])
            )
            .unwrap()
            .output,
        "17"
    );
    let error = engine
        .execute(&mut context, &command("set", &["-g", "not-an-option", "1"]))
        .unwrap_err();
    assert!(matches!(
        error,
        ServerError::InvalidCommand(message) if message == "invalid option: not-an-option"
    ));
    for args in [
        &["-gq", "not-an-option", "1"][..],
        &["-gq", "status-l", "value"][..],
    ] {
        assert_eq!(
            engine.execute(&mut context, &command("set", args)).unwrap(),
            Execution::default()
        );
    }
}

#[test]
fn tmux_boolean_options_reject_true_and_false() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    for option in ["renumber-windows", "synchronize-panes"] {
        for value in ["true", "false"] {
            let error = engine
                .execute(&mut context, &command("set", &[option, value]))
                .unwrap_err();
            assert!(matches!(
                error,
                ServerError::InvalidCommand(message)
                    if message == format!("bad value: {value}")
            ));
        }
    }
}
