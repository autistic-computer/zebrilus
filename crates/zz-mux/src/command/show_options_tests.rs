use super::test_support::command;
use super::*;

#[test]
fn missing_show_options_target_uses_an_empty_argument_format_context() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-s", "target-session", "-n", "target-window"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-s", "current-session", "-n", "current-window"],
            ),
        )
        .unwrap();
    for (name, value) in [
        ("@target-session", "target"),
        ("@current-session", "current"),
        ("@missing-session", "missing"),
    ] {
        engine
            .execute(&mut context, &command("set-option", &["-g", name, value]))
            .unwrap();
    }

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "-v", "@#{session_name}"]),
            )
            .unwrap()
            .output,
        "current"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-options",
                    &["-g", "-v", "-t", "=target-session:0.0", "@#{session_name}",],
                ),
            )
            .unwrap()
            .output,
        "target"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-options",
                    &[
                        "-g",
                        "-v",
                        "-t",
                        "=absent",
                        "@#{?session_name,current-session,missing-session}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "missing"
    );
}

#[test]
fn missing_show_window_options_target_uses_an_empty_argument_format_context() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-s", "target-session", "-n", "target-window"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-s", "current-session", "-n", "current-window"],
            ),
        )
        .unwrap();
    for (name, value) in [
        ("@target-window", "target"),
        ("@current-window", "current"),
        ("@missing-window", "missing"),
    ] {
        engine
            .execute(
                &mut context,
                &command("set-window-option", &["-g", name, value]),
            )
            .unwrap();
    }

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-window-options", &["-g", "-v", "@#{window_name}"],),
            )
            .unwrap()
            .output,
        "current"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-window-options",
                    &["-g", "-v", "-t", "=target-session:0", "@#{window_name}",],
                ),
            )
            .unwrap()
            .output,
        "target"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-window-options",
                    &[
                        "-g",
                        "-v",
                        "-t",
                        "=absent:0",
                        "@#{?window_name,current-window,missing-window}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "missing"
    );
}

#[test]
fn show_options_uses_pin_escaping_and_declared_scope_readback() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    assert_eq!(tmux_args_escape(""), "''");
    assert_eq!(tmux_args_escape("#"), "\\#");
    assert_eq!(tmux_args_escape("a b"), "\"a b\"");
    assert_eq!(tmux_args_escape("a\"b"), "'a\"b'");
    assert_eq!(tmux_args_escape("~value"), "\\~value");
    assert_eq!(tmux_args_escape("${value}"), "\"\\${value}\"");
    assert_eq!(tmux_args_escape("line\nnext"), "line\\nnext");
    assert_eq!(tmux_args_escape("界"), "界");

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "base-index", "1"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["base-index"]))
            .unwrap()
            .output,
        ""
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-A", "base-index"]),
            )
            .unwrap()
            .output,
        "base-index* 1"
    );
    engine
        .execute(&mut context, &command("set-option", &["base-index", "2"]))
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-window-options", &["-v", "base-index"]),
            )
            .unwrap()
            .output,
        "2"
    );

    let status_left = "left ${session_name} right";
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "status-left", status_left]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "status-left"]),
            )
            .unwrap()
            .output,
        "status-left \"left \\${session_name} right\""
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gv", "status-left"]),
            )
            .unwrap()
            .output,
        status_left
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gq", "not-an-option"]),
            )
            .unwrap()
            .output,
        ""
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gA", "escape-time"]),
            )
            .unwrap()
            .output,
        "escape-time 10"
    );
}

#[test]
fn behavior_option_defaults_match_pin_readback() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    for (command_name, args, expected) in [
        ("show-options", vec!["-gv", "mouse"], "on"),
        ("show-options", vec!["-sv", "escape-time"], "10"),
        (
            "show-window-options",
            vec!["-gv", "aggressive-resize"],
            "off",
        ),
        ("show-window-options", vec!["-gv", "automatic-rename"], "on"),
        (
            "show-window-options",
            vec!["-gv", "automatic-rename-format"],
            DEFAULT_AUTOMATIC_RENAME_FORMAT,
        ),
        ("show-window-options", vec!["-gv", "remain-on-exit"], "off"),
        (
            "show-options",
            vec!["-sv", "default-terminal"],
            "tmux-256color",
        ),
        ("show-options", vec!["-gv", "display-time"], "750"),
        ("show-options", vec!["-gv", "initial-repeat-time"], "0"),
        ("show-options", vec!["-gv", "repeat-time"], "500"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command(command_name, &args))
                .unwrap()
                .output,
            expected
        );
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-window-options", &["-g", "automatic-rename-format"],),
            )
            .unwrap()
            .output,
        format!("automatic-rename-format \"{DEFAULT_AUTOMATIC_RENAME_FORMAT}\"")
    );
    assert_eq!(engine.default_terminal_for_spawn(), "tmux-256color");
}

#[test]
fn boot_resolved_editor_is_visible_through_option_readback() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine.initialize_default_editor("nvim");

    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["-sv", "editor"]),)
            .unwrap()
            .output,
        "nvim"
    );
    assert!(
        engine
            .execute(&mut context, &command("show-options", &["-s"]))
            .unwrap()
            .output
            .lines()
            .any(|line| line == "editor nvim")
    );
}

#[test]
fn option_table_defaults_match_the_engine_except_history_limit() {
    let engine = MuxEngine::default();
    let mismatches = tmux_options()
        .filter_map(|option| option.default.map(|default| (option, default)))
        .filter_map(|(option, default)| {
            let expected = default.value();
            let runtime = engine
                .global_tmux_option_value(option.name)
                .unwrap_or_else(|| panic!("missing runtime default for {}", option.name));
            (runtime != expected).then_some(option.name)
        })
        .collect::<BTreeSet<_>>();

    assert_eq!(mismatches, BTreeSet::from(["history-limit"]));
}

#[test]
fn status_readback_uses_the_pin_defaults() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "status-left"]),
            )
            .unwrap()
            .output,
        "status-left \"[#{session_name}] \""
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "status-right"]),
            )
            .unwrap()
            .output,
        "status-right \"#{?window_bigger,[#{window_offset_x}#,#{window_offset_y}] ,}\\\"#{=21:pane_title}\\\" %H:%M %d-%b-%y\""
    );
    for (option, expected) in [
        ("status-style", "bg=themegreen,fg=themeblack"),
        ("status-bg", "default"),
        ("status-fg", "default"),
        ("status-left-style", "default"),
        ("status-right-style", "default"),
        ("status-left-length", "10"),
        ("status-right-length", "40"),
        ("status-justify", "left"),
        ("status-position", "bottom"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", &["-gv", option]))
                .unwrap()
                .output,
            expected,
            "{option}"
        );
    }
    for (option, expected) in [
        (
            "window-status-format",
            "#I:#W#{?window_flags,#{window_flags}, }",
        ),
        (
            "window-status-current-format",
            "#I:#W#{?window_flags,#{window_flags}, }",
        ),
        ("window-status-separator", " "),
        ("window-status-style", "default"),
        ("window-status-current-style", "underscore"),
        ("window-status-last-style", "default"),
        ("window-status-bell-style", "reverse"),
        ("window-status-activity-style", "reverse"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", &["-gwv", option]))
                .unwrap()
                .output,
            expected,
            "{option}"
        );
    }
}

#[test]
fn bare_status_option_listings_include_the_full_wave_a_family() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    let session = engine
        .execute(&mut context, &command("show-options", &["-g"]))
        .unwrap()
        .output;
    for name in [
        "status-style",
        "status-bg",
        "status-fg",
        "status-left-style",
        "status-right-style",
        "status-left-length",
        "status-right-length",
        "status-justify",
        "status-position",
    ] {
        let prefix = format!("{name} ");
        assert!(
            session.lines().any(|line| line.starts_with(&prefix)),
            "bare session listing omitted {name}"
        );
    }

    let window = engine
        .execute(&mut context, &command("show-options", &["-gw"]))
        .unwrap()
        .output;
    for name in [
        "window-status-format",
        "window-status-current-format",
        "window-status-separator",
        "window-status-style",
        "window-status-current-style",
        "window-status-last-style",
        "window-status-bell-style",
        "window-status-activity-style",
    ] {
        let prefix = format!("{name} ");
        assert!(
            window.lines().any(|line| line.starts_with(&prefix)),
            "bare window listing omitted {name}"
        );
    }
}

#[test]
fn plain_option_listings_expose_only_tmux_and_user_names() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    for args in [
        &["-s", "@listed", "server"] as &[&str],
        &["-g", "@listed", "global-session"],
        &["@listed", "session"],
        &["-gw", "@listed", "global-window"],
        &["-w", "@listed", "window"],
        &["-p", "@listed", "pane"],
    ] {
        engine
            .execute(&mut context, &command("set-option", args))
            .unwrap();
    }
    let tmux_names = tmux_options()
        .map(|option| option.name)
        .collect::<BTreeSet<_>>();
    for args in [&["-s"] as &[&str], &["-g"], &[], &["-gw"], &["-w"], &["-p"]] {
        let output = engine
            .execute(&mut context, &command("show-options", args))
            .unwrap()
            .output;
        assert!(output.lines().any(|line| line.starts_with("@listed ")));
        for line in output.lines() {
            let name = line
                .split_ascii_whitespace()
                .next()
                .expect("listed option name")
                .trim_end_matches('*');
            let name = name.split('[').next().expect("option name before index");
            assert!(name.starts_with('@') || tmux_names.contains(name), "{line}");
            assert!(!is_native_option(name), "{line}");
            assert!(!tmux_option_is_hook(name), "{line}");
        }
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-s", "history-trickle"]),
            )
            .unwrap()
            .output,
        "history-trickle 2000"
    );
}

#[test]
fn show_options_format_rows_match_the_pins_print_paths() {
    let mut engine = MuxEngine::default();
    let (session, window, pane) = engine.state.create_session("work").unwrap();
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let mut run = |args: &[&str]| {
        engine
            .execute(&mut context, &command(args[0], &args[1..]))
            .unwrap()
            .output
    };
    run(&["set-option", "-g", "@x", "hello"]);
    assert_eq!(
        run(&[
            "show-hooks",
            "-g",
            "-F",
            "#{option_name}=#{option_value}",
            "@x"
        ]),
        "@x=hello"
    );
    assert_eq!(
        run(&[
            "show-hooks",
            "-g",
            "-F",
            "#{option_name}=#{option_value}",
            "status-keys"
        ]),
        "status-keys=emacs"
    );
    assert_eq!(run(&["show-options", "-g", "-F", "", "status"]), "\n");
    assert_eq!(
        run(&["show-options", "-g", "-F", "", "status-format"]),
        "\n\n\n"
    );
    run(&["set-option", "-g", "update-environment", ""]);
    assert_eq!(
        run(&[
            "show-options",
            "-A",
            "-F",
            "#{option_is_parent}:#{option_has_value}",
            "update-environment"
        ]),
        "0:0"
    );
    assert_eq!(
        run(&["show-options", "-A", "update-environment"]),
        "update-environment"
    );
    run(&["set-hook", "-B", "@watch::#{session_name}"]);
    assert_eq!(run(&["show-hooks", "-B", "-F", "", "@watch"]), "\n");
    assert_eq!(run(&["show-hooks", "-B", "-F", ""]), "\n");
    assert_eq!(
        run(&["show-hooks", "-B", "-F", "#{hook_monitor_format}", "@watch"]),
        "#{session_name}"
    );
}
