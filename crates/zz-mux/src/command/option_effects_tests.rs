use super::test_support::command;
use super::*;

#[test]
fn set_titles_readers_resolve_scope_and_emit_status_effects() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();

    assert!(!engine.set_titles_for_session(Some(session)));
    assert_eq!(
        engine.set_titles_string_for_session(Some(session)),
        "#S:#I:#W - \"#T\" #{session_alerts}"
    );

    let global = engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "set-titles", "on"]),
        )
        .unwrap();
    assert_eq!(
        global.effects,
        [MuxEffect::StatusFormatsChanged { session: None }]
    );
    assert!(engine.set_titles_for_session(Some(session)));
    assert!(engine.set_titles_for_session(None));

    let scoped = engine
        .execute(
            &mut context,
            &command("set-option", &["set-titles-string", "#S custom"]),
        )
        .unwrap();
    assert_eq!(
        scoped.effects,
        [MuxEffect::StatusFormatsChanged {
            session: Some(session),
        }]
    );
    assert_eq!(
        engine.set_titles_string_for_session(Some(session)),
        "#S custom"
    );
    assert_eq!(
        engine.set_titles_string_for_session(None),
        "#S:#I:#W - \"#T\" #{session_alerts}"
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-u", "set-titles-string"]),
        )
        .unwrap();
    assert_eq!(
        engine.set_titles_string_for_session(Some(session)),
        "#S:#I:#W - \"#T\" #{session_alerts}"
    );
}

#[test]
fn v71_option_writes_emit_scoped_mux_option_effects() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();

    let global = engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "mouse", "off"]),
        )
        .unwrap();
    assert_eq!(
        global.effects,
        [MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::Mouse,
            session: None,
        }]
    );
    assert_eq!(engine.mux_option_value(MuxOptionKey::Mouse), "off");
    assert!(!engine.effective_mouse(Some(session)));

    let scoped = engine
        .execute(&mut context, &command("set-option", &["mouse", "on"]))
        .unwrap();
    assert_eq!(
        scoped.effects,
        [MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::Mouse,
            session: Some(session),
        }]
    );
    assert_eq!(engine.mux_option_value(MuxOptionKey::Mouse), "off");
    assert!(engine.effective_mouse(Some(session)));
    assert!(!engine.effective_mouse(None));

    let escape = engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "escape-time", "50"]),
        )
        .unwrap();
    assert_eq!(
        escape.effects,
        [MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::EscapeTime,
            session: None,
        }]
    );
    assert_eq!(engine.mux_option_value(MuxOptionKey::EscapeTime), "50");

    let prefix2 = engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "prefix2", "C-a"]),
        )
        .unwrap();
    assert_eq!(
        prefix2.effects,
        [MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::Prefix2,
            session: None,
        }]
    );
    assert_eq!(engine.mux_option_value(MuxOptionKey::Prefix2), "C-a");
    assert_eq!(engine.keys.prefix2(), Some("C-a"));
    let sent = engine
        .execute(&mut context, &command("send-prefix", &["-2"]))
        .unwrap();
    assert_eq!(
        sent.effects,
        [MuxEffect::SendKeys {
            pane: context.pane.unwrap(),
            keys: vec![KeyToken::Named("C-a".to_owned())],
            repeat: 1,
        }]
    );

    let session_prefix2 = engine
        .execute(&mut context, &command("set-option", &["prefix2", "C-s"]))
        .unwrap();
    assert!(session_prefix2.effects.is_empty());
    assert_eq!(engine.mux_option_value(MuxOptionKey::Prefix2), "C-a");
    assert_eq!(engine.keys.prefix2(), Some("C-a"));

    let unset = engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "-u", "prefix2"]),
        )
        .unwrap();
    assert_eq!(
        unset.effects,
        [MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::Prefix2,
            session: None,
        }]
    );
    assert_eq!(engine.mux_option_value(MuxOptionKey::Prefix2), "None");
    assert_eq!(engine.keys.prefix2(), None);
    let unset_send = engine
        .execute(&mut context, &command("send-prefix", &["-2"]))
        .unwrap();
    assert!(unset_send.effects.is_empty());
    assert!(unset_send.output.is_empty());
}

#[test]
fn terminal_knob_effects_preserve_global_window_and_pane_scope() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let window = context.window.unwrap();
    let pane = context.pane.unwrap();

    let window_effect = engine
        .execute(
            &mut context,
            &command("set-window-option", &["wrap-search", "off"]),
        )
        .unwrap();
    assert_eq!(
        window_effect.effects,
        vec![MuxEffect::TerminalKnobsChanged {
            window: Some(window),
            pane: None,
        }]
    );

    let pane_effect = engine
        .execute(
            &mut context,
            &command("set-option", &["-p", "cursor-style", "bar"]),
        )
        .unwrap();
    assert_eq!(
        pane_effect.effects,
        vec![MuxEffect::TerminalKnobsChanged {
            window: None,
            pane: Some(pane),
        }]
    );

    let global_effect = engine
        .execute(
            &mut context,
            &command("set-window-option", &["-g", "allow-passthrough", "all"]),
        )
        .unwrap();
    assert_eq!(
        global_effect.effects,
        vec![MuxEffect::TerminalKnobsChanged {
            window: None,
            pane: None,
        }]
    );

    for name in ["allow-rename", "alternate-screen", "scroll-on-clear"] {
        let engine_knob = engine
            .execute(
                &mut context,
                &command("set-window-option", &["-g", name, "off"]),
            )
            .unwrap();
        let expected = if name == "allow-rename" {
            Vec::new()
        } else {
            vec![MuxEffect::TerminalKnobsChanged {
                window: None,
                pane: None,
            }]
        };
        assert_eq!(engine_knob.effects, expected, "{name}");
    }

    let backspace = engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "backspace", "C-h"]),
        )
        .unwrap();
    assert_eq!(
        backspace.effects,
        vec![MuxEffect::TerminalKnobsChanged {
            window: None,
            pane: None,
        }]
    );
    assert_eq!(engine.backspace_byte(), Some(0x08));
    assert_eq!(engine.verase_byte(), 0x7f);
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "backspace", "0x08"]),
        )
        .unwrap();
    assert_eq!(engine.backspace_byte(), Some(0x08));
    assert_eq!(engine.verase_byte(), 0x08);
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "backspace", "0x41"]),
        )
        .unwrap();
    assert_eq!(engine.backspace_byte(), Some(0x41));
    assert_eq!(engine.verase_byte(), 0x7f);
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "backspace", "Space"]),
        )
        .unwrap();
    assert_eq!(engine.backspace_byte(), Some(b' '));
    assert_eq!(engine.verase_byte(), b' ');
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "backspace", "C-w"]),
        )
        .unwrap();
    assert_eq!(engine.backspace_byte(), Some(0x17));
    engine
        .execute(&mut context, &command("set-option", &["-su", "backspace"]))
        .unwrap();
    assert_eq!(engine.backspace_byte(), Some(0x7f));
}

#[test]
fn same_session_reparenting_refreshes_terminal_worker_knobs() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-s", "work", "-n", "source"]),
        )
        .unwrap();
    let moving = context.pane.unwrap();
    let source_window = context.window.unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-t", &source_window.to_string(), "wrap-search", "off"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-p", "-t", &moving.to_string(), "cursor-style", "bar"],
            ),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-n", "destination"]))
        .unwrap();
    let target = context.pane.unwrap();
    let destination_window = context.window.unwrap();

    let joined = engine
        .execute(
            &mut context,
            &command(
                "join-pane",
                &["-d", "-s", &moving.to_string(), "-t", &target.to_string()],
            ),
        )
        .unwrap();
    assert!(joined.effects.contains(&MuxEffect::TerminalKnobsChanged {
        window: None,
        pane: Some(moving),
    }));
    let joined_options = engine.terminal_worker_options_for_pane(moving).unwrap();
    assert!(joined_options.wrap_search);
    assert_eq!(joined_options.cursor_style, "bar");

    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-t", &destination_window.to_string(), "wrap-search", "off"],
            ),
        )
        .unwrap();
    let broken = engine
        .execute(
            &mut context,
            &command("break-pane", &["-d", "-s", &moving.to_string()]),
        )
        .unwrap();
    assert!(broken.effects.contains(&MuxEffect::TerminalKnobsChanged {
        window: None,
        pane: Some(moving),
    }));
    let broken_window = engine.state.window_for_pane(moving).unwrap();
    let broken_options = engine.terminal_worker_options_for_pane(moving).unwrap();
    assert!(broken_options.wrap_search);
    assert_eq!(broken_options.cursor_style, "bar");

    let swapped = engine
        .execute(
            &mut context,
            &command(
                "swap-pane",
                &["-d", "-s", &moving.to_string(), "-t", &target.to_string()],
            ),
        )
        .unwrap();
    for pane in [moving, target] {
        assert!(swapped.effects.contains(&MuxEffect::TerminalKnobsChanged {
            window: None,
            pane: Some(pane),
        }));
    }
    assert_eq!(
        engine.state.window_for_pane(moving),
        Some(destination_window)
    );
    assert_eq!(engine.state.window_for_pane(target), Some(broken_window));
    assert!(
        !engine
            .terminal_worker_options_for_pane(moving)
            .unwrap()
            .wrap_search
    );
    assert!(
        engine
            .terminal_worker_options_for_pane(target)
            .unwrap()
            .wrap_search
    );
    assert_eq!(
        engine
            .terminal_worker_options_for_pane(moving)
            .unwrap()
            .cursor_style,
        "bar"
    );
}

#[test]
fn status_style_options_defer_expansion_and_append_with_commas() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    engine
        .execute(
            &mut context,
            &command("set-option", &["-ag", "status-style", "fg=white"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gv", "status-style"]),
            )
            .unwrap()
            .output,
        "bg=themegreen,fg=themeblack,fg=white"
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "status-left-style", "bold"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-ag", "status-left-style", "italics"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gv", "status-left-style"]),
            )
            .unwrap()
            .output,
        "bold,italics"
    );

    let dynamic = "fg=#{?client_prefix,red,green}";
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "status-right-style", dynamic]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-ag", "status-right-style", "bold"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gv", "status-right-style"]),
            )
            .unwrap()
            .output,
        format!("{dynamic},bold")
    );

    engine
        .execute(
            &mut context,
            &command("set-window-option", &["-g", "window-status-style", dynamic]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-ag", "window-status-style", "italics"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gwv", "window-status-style"]),
            )
            .unwrap()
            .output,
        format!("{dynamic},italics")
    );
}

#[test]
fn fed_mode_keys_default_precedes_explicit_configuration() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine.set_default_mode_keys("vi").unwrap();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let pane = context.pane.expect("pane");

    assert_eq!(
        engine.copy_mode_table_for_pane(pane).unwrap(),
        "copy-mode-vi"
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["-g", "mode-keys"]),)
            .unwrap()
            .output,
        "mode-keys vi"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-gw", "mode-keys", "emacs"]),
        )
        .unwrap();
    assert_eq!(engine.copy_mode_table_for_pane(pane).unwrap(), "copy-mode");
}

#[test]
fn explicit_status_writes_flip_and_clear_customized() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.expect("session id");
    assert!(!engine.status_customized_for_session(None));
    assert!(!engine.status_customized_for_session(Some(session)));

    let same_as_default = engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "status", "on"]),
        )
        .unwrap();
    assert_eq!(
        same_as_default.effects,
        [MuxEffect::StatusFormatsChanged { session: None }],
        "an explicit write equal to the default still publishes"
    );
    assert!(engine.status_customized_for_session(None));
    assert!(engine.status_customized_for_session(Some(session)));

    let cleared = engine
        .execute(&mut context, &command("set-option", &["-gu", "status"]))
        .unwrap();
    assert_eq!(
        cleared.effects,
        [MuxEffect::StatusFormatsChanged { session: None }]
    );
    assert!(!engine.status_customized_for_session(None));
    assert!(!engine.status_customized_for_session(Some(session)));

    engine
        .execute(
            &mut context,
            &command("set-option", &["status-left", "[#S] "]),
        )
        .unwrap();
    assert!(!engine.status_customized_for_session(None));
    assert!(engine.status_customized_for_session(Some(session)));
    engine
        .execute(&mut context, &command("set-option", &["-u", "status-left"]))
        .unwrap();
    assert!(!engine.status_customized_for_session(Some(session)));

    engine
        .execute(
            &mut context,
            &command("set-option", &["status-format[0]", "row"]),
        )
        .unwrap();
    assert!(engine.status_customized_for_session(Some(session)));
    engine
        .execute(
            &mut context,
            &command("set-option", &["-u", "status-format"]),
        )
        .unwrap();
    assert!(!engine.status_customized_for_session(Some(session)));
}

#[test]
fn status_format_writes_emit_status_effects_with_their_scope() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.expect("session id");

    let global = engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "status-format[0]", "global-row"]),
        )
        .unwrap();
    assert_eq!(
        global.effects,
        [MuxEffect::StatusFormatsChanged { session: None }]
    );

    let scoped = engine
        .execute(
            &mut context,
            &command("set-option", &["status-format[3]", "scoped-row"]),
        )
        .unwrap();
    assert_eq!(
        scoped.effects,
        [MuxEffect::StatusFormatsChanged {
            session: Some(session)
        }]
    );

    let unchanged = engine
        .execute(
            &mut context,
            &command("set-option", &["status-format[3]", "scoped-row"]),
        )
        .unwrap();
    assert!(unchanged.effects.is_empty());

    let scoped_unset = engine
        .execute(
            &mut context,
            &command("set-option", &["-u", "status-format"]),
        )
        .unwrap();
    assert_eq!(
        scoped_unset.effects,
        [MuxEffect::StatusFormatsChanged {
            session: Some(session)
        }]
    );

    let global_unset = engine
        .execute(
            &mut context,
            &command("set-option", &["-gu", "status-format"]),
        )
        .unwrap();
    assert_eq!(
        global_unset.effects,
        [MuxEffect::StatusFormatsChanged { session: None }]
    );
}

#[test]
fn message_line_reads_session_effective_values_and_publishes_changes() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.expect("session id");
    assert_eq!(engine.message_line_for_session(None), 0);
    assert_eq!(engine.message_line_for_session(Some(session)), 0);

    let global = engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "message-line", "4"]),
        )
        .unwrap();
    assert_eq!(
        global.effects,
        [MuxEffect::StatusFormatsChanged { session: None }]
    );
    assert_eq!(engine.message_line_for_session(Some(session)), 4);

    let scoped = engine
        .execute(&mut context, &command("set-option", &["message-line", "2"]))
        .unwrap();
    assert_eq!(
        scoped.effects,
        [MuxEffect::StatusFormatsChanged {
            session: Some(session)
        }]
    );
    assert_eq!(engine.message_line_for_session(Some(session)), 2);
    assert_eq!(engine.message_line_for_session(None), 4);
    assert!(
        !engine.status_customized_for_session(Some(session)),
        "message-line is not part of the status customization ledger"
    );

    let unset = engine
        .execute(
            &mut context,
            &command("set-option", &["-u", "message-line"]),
        )
        .unwrap();
    assert_eq!(
        unset.effects,
        [MuxEffect::StatusFormatsChanged {
            session: Some(session)
        }]
    );
    assert_eq!(engine.message_line_for_session(Some(session)), 4);
}

#[test]
fn renderer_style_writes_emit_their_consuming_effects() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "styles"]))
        .unwrap();
    let window = context.window.expect("window id");
    let pane = context.pane.expect("pane id");

    for (args, expected) in [
        (
            vec!["-g", "pane-border-style", "fg=red"],
            MuxEffect::SnapshotChanged,
        ),
        (
            vec!["-g", "pane-active-border-style", "fg=blue"],
            MuxEffect::SnapshotChanged,
        ),
        (
            vec!["-g", "display-panes-format", "#{pane_index}"],
            MuxEffect::SnapshotChanged,
        ),
        (
            vec!["-g", "window-style", "bg=colour236"],
            MuxEffect::TerminalKnobsChanged {
                window: None,
                pane: None,
            },
        ),
        (
            vec!["-g", "mode-style", "bg=red"],
            MuxEffect::ModeStylesChanged,
        ),
        (
            vec!["-g", "copy-mode-match-style", "bg=blue"],
            MuxEffect::ModeStylesChanged,
        ),
        (
            vec!["-g", "copy-mode-current-match-style", "bg=green"],
            MuxEffect::ModeStylesChanged,
        ),
        (
            vec!["-g", "copy-mode-mark-style", "bg=yellow"],
            MuxEffect::ModeStylesChanged,
        ),
    ] {
        let execution = engine
            .execute(&mut context, &command("set-option", &args))
            .unwrap();
        assert_eq!(execution.effects, vec![expected], "{args:?}");
    }

    let scoped = engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-t", "styles:0", "window-active-style", "bg=colour238"],
            ),
        )
        .unwrap();
    assert_eq!(
        scoped.effects,
        vec![MuxEffect::TerminalKnobsChanged {
            window: Some(window),
            pane: None,
        }]
    );
    let pane_scoped = engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-p",
                    "-t",
                    &pane.to_string(),
                    "window-style",
                    "bg=colour239",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        pane_scoped.effects,
        vec![MuxEffect::TerminalKnobsChanged {
            window: None,
            pane: Some(pane),
        }]
    );
    let unset = engine
        .execute(&mut context, &command("set-option", &["-gu", "mode-style"]))
        .unwrap();
    assert_eq!(unset.effects, vec![MuxEffect::ModeStylesChanged]);
}

#[test]
fn window_and_border_style_values_resolve_pane_window_global() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "chain"]))
        .unwrap();
    let window = context.window.expect("window id");
    engine
        .execute(&mut context, &command("split-window", &["-d"]))
        .unwrap();
    let order = engine.state.windows[&window].pane_order().to_vec();
    let (pane, second) = (order[0], order[1]);

    assert!(!engine.has_window_style_settings());
    assert!(!engine.has_pane_border_style_settings());
    assert_eq!(
        engine.window_style_values(pane),
        WindowStyleValues::default()
    );
    assert_eq!(
        engine.pane_border_style_values(pane),
        PaneBorderStyleValues::default()
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "window-style", "bg=colour236"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-p", "-t", &second.to_string(), "window-style", "bg=red"],
            ),
        )
        .unwrap();
    assert!(engine.has_window_style_settings());
    assert_eq!(
        engine.window_style_values(pane).style.as_deref(),
        Some("bg=colour236")
    );
    assert_eq!(
        engine.window_style_values(second).style.as_deref(),
        Some("bg=red")
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "pane-border-style", "fg=colour100"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-p",
                    "-t",
                    &second.to_string(),
                    "pane-active-border-style",
                    "fg=#112233",
                ],
            ),
        )
        .unwrap();
    assert!(engine.has_pane_border_style_settings());
    let first_borders = engine.pane_border_style_values(pane);
    assert_eq!(first_borders.border.as_deref(), Some("fg=colour100"));
    assert_eq!(first_borders.active_border, None);
    let second_borders = engine.pane_border_style_values(second);
    assert_eq!(second_borders.border.as_deref(), Some("fg=colour100"));
    assert_eq!(second_borders.active_border.as_deref(), Some("fg=#112233"));

    let options = engine
        .terminal_worker_options_for_pane(pane)
        .expect("worker options");
    assert_eq!(options.window_style.as_deref(), Some("bg=colour236"));
    assert_eq!(options.window_active_style, None);
    assert!(options.active_pane);
    let second_options = engine
        .terminal_worker_options_for_pane(second)
        .expect("worker options");
    assert_eq!(second_options.window_style.as_deref(), Some("bg=red"));
    assert!(!second_options.active_pane);
}

#[test]
fn display_panes_options_resolve_window_then_global_then_default() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "fmt"]))
        .unwrap();
    let window = context.window.expect("window id");
    let options = engine.display_panes_options(window);
    assert_eq!(
        options.format,
        "#[align=right]#{pane_unzoomed_width}x#{pane_unzoomed_height}"
    );
    assert_eq!(options.border_style, "bg=themedarkgrey,fg=themelightgrey");
    assert_eq!(options.time_ms, 1000);
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "display-panes-format", "GLOBAL"]),
        )
        .unwrap();
    assert_eq!(engine.display_panes_format_for_window(window), "GLOBAL");
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-w", "-t", "fmt:", "display-panes-format", "#{pane_index}"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-w", "-t", "fmt:", "display-panes-time", "250"],
            ),
        )
        .unwrap();
    let options = engine.display_panes_options(window);
    assert_eq!(options.format, "#{pane_index}");
    assert_eq!(options.time_ms, 250);
}

#[test]
fn word_separators_are_live_inherited_session_options() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "first"]))
        .expect("first session");
    let first_session = context.session.expect("first session id");
    let first_pane = context.pane.expect("first pane");
    assert_eq!(
        engine.word_separators_for_pane(first_pane).unwrap(),
        DEFAULT_WORD_SEPARATORS
    );

    let global = engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "word-separators", ""]),
        )
        .expect("empty global separators");
    assert_eq!(
        global.effects,
        [
            MuxEffect::WordSeparatorsChanged { session: None },
            MuxEffect::MuxOptionChanged {
                option: MuxOptionKey::WordSeparators,
                session: None,
            },
        ]
    );
    assert_eq!(engine.word_separators_for_session(first_session), "");

    let targeted = engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-t", &first_session.to_string(), "word-separators", "."],
            ),
        )
        .expect("session separators");
    assert_eq!(
        targeted.effects,
        [MuxEffect::WordSeparatorsChanged {
            session: Some(first_session)
        }]
    );
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-a",
                    "-t",
                    &first_session.to_string(),
                    "word-separators",
                    "λ",
                ],
            ),
        )
        .expect("append unicode separator");
    assert_eq!(engine.word_separators_for_session(first_session), ".λ");

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "word-separators", "|"]),
        )
        .expect("replace global separators");
    assert_eq!(engine.word_separators_for_session(first_session), ".λ");
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-u", "-t", &first_session.to_string(), "word-separators"],
            ),
        )
        .expect("restore inheritance");
    assert_eq!(engine.word_separators_for_session(first_session), "|");

    let oversized = "x".repeat(MAX_WORD_SEPARATORS_BYTES + 1);
    assert!(
        engine
            .execute(
                &mut context,
                &command("set-option", &["-g", "word-separators", &oversized]),
            )
            .is_err()
    );
    assert_eq!(engine.word_separators_for_session(first_session), "|");
}

#[test]
fn mode_keys_follow_window_scope_inheritance_and_live_effects() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "modes"]))
        .expect("first window");
    let first_window = context.window.expect("first window id");
    let first_pane = context.pane.expect("first pane");
    engine
        .execute(&mut context, &command("new-window", &["-n", "second"]))
        .expect("second window");
    let second_window = context.window.expect("second window id");
    let second_pane = context.pane.expect("second pane");
    assert_eq!(
        engine.copy_mode_table_for_pane(first_pane).unwrap(),
        "copy-mode"
    );
    assert_eq!(
        engine.copy_mode_table_for_pane(second_pane).unwrap(),
        "copy-mode"
    );

    let targeted = engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-t", &first_window.to_string(), "mode-keys", "vi"],
            ),
        )
        .expect("window mode table");
    assert_eq!(
        targeted.effects,
        [MuxEffect::ModeKeysChanged {
            window: Some(first_window)
        }]
    );
    assert_eq!(
        engine.copy_mode_table_for_pane(first_pane).unwrap(),
        "copy-mode-vi"
    );
    assert_eq!(
        engine.copy_mode_table_for_pane(second_pane).unwrap(),
        "copy-mode"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-t", &first_window.to_string(), "mode-keys"],
            ),
        )
        .expect("toggle choice");
    assert_eq!(
        engine.copy_mode_table_for_pane(first_pane).unwrap(),
        "copy-mode"
    );

    let global = engine
        .execute(
            &mut context,
            &command("set-window-option", &["-g", "mode-keys", "vi"]),
        )
        .expect("global window default");
    assert_eq!(
        global.effects,
        [
            MuxEffect::ModeKeysChanged { window: None },
            MuxEffect::MuxOptionChanged {
                option: MuxOptionKey::ModeKeys,
                session: None,
            },
        ]
    );
    assert_eq!(
        engine.copy_mode_table_for_pane(first_pane).unwrap(),
        "copy-mode"
    );
    assert_eq!(
        engine.copy_mode_table_for_pane(second_pane).unwrap(),
        "copy-mode-vi"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-u", "-t", &first_window.to_string(), "mode-keys"],
            ),
        )
        .expect("restore window inheritance");
    assert_eq!(
        engine.copy_mode_table_for_pane(first_pane).unwrap(),
        "copy-mode-vi"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-o", "-t", &second_window.to_string(), "mode-keys", "emacs"],
            ),
        )
        .expect("set inherited option once");
    let quiet = engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-oq", "-t", &second_window.to_string(), "mode-keys", "vi"],
            ),
        )
        .expect("quiet duplicate");
    assert!(quiet.effects.is_empty());
    assert_eq!(
        engine.copy_mode_table_for_pane(second_pane).unwrap(),
        "copy-mode"
    );

    for invalid in [
        command("set-window-option", &["-w", "mode-keys", "vi"]),
        command("set-option", &["mode-keys", "unknown"]),
    ] {
        assert!(engine.execute(&mut context, &invalid).is_err());
    }
    assert_eq!(
        engine.copy_mode_table_for_pane(second_pane).unwrap(),
        "copy-mode"
    );
}

#[test]
fn synchronize_panes_commands_preserve_native_option_inheritance() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .unwrap();
    let first = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let second = context.pane.unwrap();

    let changed = engine
        .execute(
            &mut context,
            &command("set-option", &["-w", "synchronize-panes", "on"]),
        )
        .unwrap();
    assert!(changed.effects.contains(&MuxEffect::SnapshotChanged));
    assert_eq!(
        engine.synchronized_input_targets(first).unwrap(),
        [first, second]
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-p", "-t", &second.to_string(), "synchronize-panes", "off"],
            ),
        )
        .unwrap();
    assert_eq!(engine.synchronized_input_targets(first).unwrap(), [first]);
    assert_eq!(engine.synchronized_input_targets(second).unwrap(), [second]);

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-p", "-u", "-t", &second.to_string(), "synchronize-panes"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine.synchronized_input_targets(second).unwrap(),
        [first, second]
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-w", "synchronize-panes"]),
        )
        .unwrap();
    assert_eq!(engine.synchronized_input_targets(first).unwrap(), [first]);

    engine
        .execute(
            &mut context,
            &command("setw", &["-g", "synchronize-panes", "on"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-p", "-t", &second.to_string(), "synchronize-panes", "off"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-U", "synchronize-panes"]),
        )
        .unwrap();
    assert_eq!(
        engine.synchronized_input_targets(first).unwrap(),
        [first, second]
    );
}

#[test]
fn copy_mode_current_line_style_republishes_the_status_formats() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let execution = engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-g", "copy-mode-current-line-style", "bg=red"],
            ),
        )
        .unwrap();
    assert!(
        execution
            .effects
            .contains(&MuxEffect::StatusFormatsChanged { session: None }),
        "{execution:?}"
    );
}

#[test]
fn popup_options_keep_window_inheritance_independent() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first = context.window.unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-n", "second"]))
        .unwrap();
    let second = context.window.unwrap();
    let second_target = second.to_string();
    assert_eq!(
        engine.popup_options_for_window(first).unwrap(),
        PopupOptions::default()
    );
    assert_eq!(
        engine.popup_options_for_window(second).unwrap(),
        PopupOptions::default()
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-g", "popup-style", "bg=blue,fg=white"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-o", "-t", &second_target, "popup-style", "bg=red"],
            ),
        )
        .unwrap();
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "set-window-option",
                &["-o", "-t", &second_target, "popup-style", "bg=green"]
            )
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "already set: popup-style"
    ));
    assert_eq!(
        engine.popup_options_for_window(first).unwrap().style,
        "bg=blue,fg=white"
    );
    assert_eq!(
        engine.popup_options_for_window(second).unwrap().style,
        "bg=red"
    );
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-u", "-t", &second_target, "popup-style"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine.popup_options_for_window(second).unwrap().style,
        "bg=blue,fg=white"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &[
                    "-t",
                    &second_target,
                    "popup-border-style",
                    "bg=black,fg=cyan",
                ],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-window-option", &["-g", "popup-border-lines", "double"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-t", &second_target, "popup-border-lines", "rounded"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine.popup_options_for_window(first).unwrap(),
        PopupOptions {
            style: "bg=blue,fg=white".to_owned(),
            border_style: PopupOptions::default().border_style,
            border_lines: PopupBorderLines::Double,
        }
    );
    assert_eq!(
        engine.popup_options_for_window(second).unwrap(),
        PopupOptions {
            style: "bg=blue,fg=white".to_owned(),
            border_style: "bg=black,fg=cyan".to_owned(),
            border_lines: PopupBorderLines::Rounded,
        }
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-window-options",
                    &["-t", &second_target, "-v", "popup-border-style"],
                ),
            )
            .unwrap()
            .output,
        "bg=black,fg=cyan"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-window-options",
                    &["-t", &second_target, "-v", "popup-border-lines"],
                ),
            )
            .unwrap()
            .output,
        "rounded"
    );
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-u", "-t", &second_target, "popup-border-lines"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .popup_options_for_window(second)
            .unwrap()
            .border_lines,
        PopupBorderLines::Double
    );
    let session_listing = engine
        .execute(&mut context, &command("show-options", &["-g"]))
        .unwrap()
        .output;
    let window_listing = engine
        .execute(&mut context, &command("show-options", &["-gw"]))
        .unwrap()
        .output;
    for option in ["popup-style", "popup-border-style"] {
        let prefix = format!("{option} ");
        assert!(
            !session_listing
                .lines()
                .any(|line| line.starts_with(&prefix))
        );
        assert!(window_listing.lines().any(|line| line.starts_with(&prefix)));
    }
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("set-window-option", &["popup-border-lines", "zigzag"])
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "unknown value: zigzag"
    ));
}

#[test]
fn menu_and_lock_options_store_inherit_validate_and_read_back() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let window = context.window.unwrap();
    assert_eq!(
        engine.menu_options_for_window(window).unwrap(),
        MenuOptions::default()
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gv", "lock-command"]),
            )
            .unwrap()
            .output,
        "lock -np"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gv", "lock-after-time"]),
            )
            .unwrap()
            .output,
        "0"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["lock-command", "secure-lock"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["lock-after-time", "90"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-v", "lock-command"]),
            )
            .unwrap()
            .output,
        "secure-lock"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-v", "lock-after-time"]),
            )
            .unwrap()
            .output,
        "90"
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "menu-style", "bg=blue,fg=white"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["menu-selected-style", "bg=yellow,fg=black"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["menu-border-style", "fg=cyan"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["menu-border-lines", "rounded"]),
        )
        .unwrap();
    assert_eq!(
        engine.menu_options_for_window(window).unwrap(),
        MenuOptions {
            style: "bg=blue,fg=white".to_owned(),
            selected_style: "bg=yellow,fg=black".to_owned(),
            border_style: "fg=cyan".to_owned(),
            border_lines: PopupBorderLines::Rounded,
        }
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-v", "menu-border-lines"]),
            )
            .unwrap()
            .output,
        "rounded"
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("set-option", &["menu-border-lines", "zigzag"])
        ),
        Err(ServerError::InvalidCommand(message)) if message == "unknown value: zigzag"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("set-option", &["menu-style", "fg=not-a-colour"])
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "invalid style: fg=not-a-colour"
    ));
}

#[test]
fn popup_style_options_reject_unknown_tokens_at_global_and_local_scope() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    for (scope, option, value) in [
        ("-gw", "popup-style", "fg=red,bg=#00ff7f,bold,nounderscore"),
        ("-gw", "popup-border-style", "fg=colour255,bg=black,italics"),
        ("-w", "popup-style", "default,none,bright|reverse"),
        ("-w", "popup-border-style", "fg=cyan,bg=colour0,noitalics"),
    ] {
        let args = vec![scope, option, value];
        engine
            .execute(&mut context, &command("set-option", &args))
            .unwrap();
    }

    for (scope, option, value) in [
        ("-gw", "popup-style", "bogus-not-a-style"),
        ("-gw", "popup-border-style", "fg=not-a-colour"),
        ("-w", "popup-style", "bold,unknown"),
        ("-w", "popup-border-style", "bg=#12345g"),
    ] {
        let args = vec![scope, option, value];
        assert!(matches!(
            engine.execute(&mut context, &command("set-option", &args)),
            Err(ServerError::InvalidCommand(message))
                if message == format!("invalid style: {value}")
        ));
    }
}

#[test]
fn style_validator_accepts_the_pinned_style_parse_keys() {
    for value in [
        "align=left",
        "align=centre",
        "align=right",
        "align=absolute-centre",
        "noalign",
        "fill=red",
        "us=blue",
        "list=on",
        "list=focus",
        "list=left-marker",
        "list=right-marker",
        "nolist",
        "range=left",
        "range=right",
        "range=control|9",
        "range=pane|%12",
        "range=window|12",
        "range=session|$12",
        "range=user|owner",
        "range=custom",
        "range=custom|value",
        "norange",
        "push-default",
        "pop-default",
        "set-default",
        "ignore",
        "noignore",
        "dim=50%",
        "width=80%",
        "pad=2",
        "link=https://example.com",
        "nolink",
        "nodefault",
        "nobold|underscore",
    ] {
        assert!(valid_style(value), "{value}");
    }
}

#[test]
fn style_validator_rejects_invalid_pinned_values() {
    for value in [
        "fg=colour256",
        "fg=#zzzzzz",
        "bogus",
        "fg=",
        "align=middle",
        "fill=",
        "us=",
        "list=bogus",
        "list=no",
        "range=control|10",
        "range=pane|12",
        "range=session|12",
        "range=user|",
        "dim=101",
        "width=-1",
        "pad=-1",
        "hyperlink",
        "nohyperlink",
        "bold|noitalics",
    ] {
        assert!(!valid_style(value), "{value}");
    }
}
