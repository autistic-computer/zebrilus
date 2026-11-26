use super::test_support::command;
use super::*;

fn expand_option(
    engine: &MuxEngine,
    context: FormatContext,
    name: &str,
    hooks: &mut impl StatusHooks,
) -> String {
    expand_format_with_hooks(&format!("#{{{name}}}"), engine, context, hooks).to_string()
}

#[test]
fn status_format_array_exposes_sparse_session_effective_entries() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.expect("session id");

    let defaults = engine.status_format_array_for_session(None);
    assert_eq!(
        defaults.keys().copied().collect::<Vec<_>>(),
        [0, 1, 2],
        "the pinned defaults materialize three rows"
    );
    assert_eq!(
        engine.status_format_array_for_session(Some(session)),
        defaults,
        "sessions without overrides inherit the global array"
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "status-format[4]", "tail"]),
        )
        .unwrap();
    let sparse = engine.status_format_array_for_session(None);
    assert_eq!(sparse.keys().copied().collect::<Vec<_>>(), [0, 1, 2, 4]);
    assert!(!sparse.contains_key(&3), "missing indices stay missing");

    engine
        .execute(
            &mut context,
            &command("set-option", &["status-format[3]", "local"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["status-format[1]", ""]),
        )
        .unwrap();
    let scoped = engine.status_format_array_for_session(Some(session));
    assert_eq!(
        scoped,
        BTreeMap::from([(1, String::new()), (3, "local".to_owned())]),
        "a session array overrides whole and keeps explicit-empty rows distinct from unset"
    );
    assert_eq!(
        engine.status_format_array_for_session(None),
        sparse,
        "the global array is untouched by session writes"
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-u", "status-format"]),
        )
        .unwrap();
    assert_eq!(
        engine.status_format_array_for_session(Some(session)),
        sparse
    );
}

#[test]
fn every_consumer_option_resolves_as_a_format() {
    let engine = MuxEngine::default();
    let context = StatusContext::default();
    let snapshot = engine.format_option_snapshot();
    assert_eq!(TMUX_OPTION_CONSUMERS.len(), 167);
    for name in TMUX_OPTION_CONSUMERS {
        let direct = engine
            .format_option_value(&context, name)
            .unwrap_or_else(|| panic!("missing direct option format: {name}"));
        assert_eq!(
            snapshot.lookup("", "", "", name),
            Some(direct.clone()),
            "snapshot option format: {name}"
        );
        let mut hooks = CommandHooks::new(0);
        assert_eq!(
            expand_option(&engine, FormatContext::default(), name, &mut hooks),
            direct,
            "ordinary option format: {name}"
        );
    }
}

#[test]
fn option_formats_are_exact_alias_aware_and_precede_hook_values() {
    struct Marker;

    impl StatusHooks for Marker {
        fn strftime(&mut self, literal: &str) -> String {
            literal.to_owned()
        }

        fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
            String::new()
        }

        fn variable(&mut self, name: &str, _context: &StatusContext) -> Option<String> {
            Some(format!("hook:{name}"))
        }
    }

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "cursor-colour", "red"]),
        )
        .unwrap();
    let mut hooks = Marker;
    assert_eq!(
        expand_option(&engine, FormatContext::default(), "base-index", &mut hooks),
        "0"
    );
    assert_eq!(
        expand_option(
            &engine,
            FormatContext::default(),
            "cursor-color",
            &mut hooks
        ),
        "red"
    );
    assert_eq!(
        expand_option(&engine, FormatContext::default(), "base-ind", &mut hooks),
        "hook:base-ind"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-gw", "pane-colours[0]", "red"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-gw", "pane-colours[1]", "#123456"]),
        )
        .unwrap();
    assert_eq!(
        expand_option(&engine, FormatContext::default(), "pane-colors", &mut hooks),
        "red #123456"
    );
    assert_eq!(
        expand_option(
            &engine,
            FormatContext::default(),
            "pane-colours",
            &mut hooks
        ),
        "red #123456"
    );
    assert_eq!(
        expand_option(
            &engine,
            FormatContext::default(),
            "pane-colors[0]",
            &mut hooks
        ),
        "red"
    );
    assert_eq!(
        expand_option(
            &engine,
            FormatContext::default(),
            "lock-command",
            &mut hooks
        ),
        "hook:lock-command"
    );
    assert_eq!(
        expand_option(
            &engine,
            FormatContext::default(),
            "pane-border-status",
            &mut hooks
        ),
        "off"
    );
}

#[test]
fn option_formats_render_flags_as_numbers_and_other_types_raw() {
    let engine = MuxEngine::default();
    let context = StatusContext::default();
    for (name, expected) in [
        ("renumber-windows", "0"),
        ("aggressive-resize", "0"),
        ("wrap-search", "1"),
        ("focus-events", "0"),
        ("allow-set-title", "1"),
        ("synchronize-panes", "0"),
        ("automatic-rename", "1"),
        ("mouse", "1"),
        ("set-titles", "0"),
        ("exit-empty", "1"),
        ("exit-unattached", "0"),
        ("monitor-activity", "0"),
        ("monitor-bell", "1"),
    ] {
        assert_eq!(
            engine.format_option_value(&context, name).as_deref(),
            Some(expected),
            "{name}"
        );
    }
    for (name, expected) in [
        ("detach-on-destroy", "on"),
        ("allow-passthrough", "off"),
        ("visual-bell", "off"),
        ("status", "on"),
        ("prefix", "C-b"),
        ("cursor-style", "default"),
    ] {
        assert_eq!(
            engine.format_option_value(&context, name).as_deref(),
            Some(expected),
            "{name}"
        );
    }
}

#[test]
fn option_format_arrays_join_index_normalize_and_shadow_whole() {
    let mut engine = MuxEngine::default();
    let (session, window, pane) = engine.state.create_session("work").unwrap();
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    for args in [
        &["-s", "command-alias", "zero,one"][..],
        &["-s", "command-alias[10]", "ten"],
        &["-s", "command-alias[2]", "two"],
        &["-s", "command-alias[beta]", "B"],
        &["-s", "command-alias[alpha]", "A"],
        &["-g", "status-format", "g0,g1"],
        &["status-format[000]", "local0"],
        &["-g", "update-environment", "ZZ_B ZZ_A"],
    ] {
        engine
            .execute(&mut context, &command("set-option", args))
            .unwrap();
    }
    let values = engine.format_status_context(Some(session), Some(window), Some(pane));
    assert_eq!(
        engine
            .format_option_value(&values, "command-alias")
            .as_deref(),
        Some("zero one two ten A B")
    );
    assert_eq!(
        engine
            .format_option_value(&values, "command-alias[000]")
            .as_deref(),
        Some("zero")
    );
    assert_eq!(
        engine
            .format_option_value(&values, "update-environment")
            .as_deref(),
        Some("ZZ_B ZZ_A")
    );
    assert_eq!(
        engine
            .format_option_value(&values, "status-format")
            .as_deref(),
        Some("local0")
    );
    assert_eq!(
        engine
            .format_option_value(&values, "status-format[000]")
            .as_deref(),
        Some("local0")
    );
    for name in [
        "status-format[1]",
        "status-format[]",
        "status-format[4294967296]",
        "status-format[0]tail",
        "status-format[",
    ] {
        assert_eq!(
            engine.format_option_value(&values, name).as_deref(),
            Some(""),
            "{name}"
        );
    }
    let global = StatusContext::default();
    assert_eq!(
        engine
            .format_option_value(&global, "status-format")
            .as_deref(),
        Some("g0 g1")
    );
    let snapshot = engine.format_option_snapshot();
    assert_eq!(
        snapshot.lookup(
            &session.to_string(),
            &window.to_string(),
            &pane.to_string(),
            "status-format[1]",
        ),
        Some(String::new())
    );
}

#[test]
fn option_format_targets_follow_scope_active_children_and_attachment() {
    let mut engine = MuxEngine::default();
    let (alpha, first_window, first_pane) = engine.state.create_session("alpha").unwrap();
    let second_pane = engine
        .state
        .split_pane(first_pane, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let (second_window, _) = engine
        .state
        .create_window_at(
            alpha,
            None,
            Some("second".to_owned()),
            PaneKind::Terminal,
            false,
        )
        .unwrap();
    let (beta, beta_window, beta_pane) = engine.state.create_session("beta").unwrap();
    let mut alpha_context =
        ExecutionContext::new(Some(alpha), Some(first_window), Some(second_pane));
    let mut beta_context = ExecutionContext::new(Some(beta), Some(beta_window), Some(beta_pane));
    for args in [
        &["-s", "escape-time", "77"][..],
        &["-g", "status-left", "GLOBAL"],
        &["status-left", "ALPHA"],
        &["-g", "window-status-format", "GLOBAL-WINDOW"],
        &["window-status-format", "FIRST-WINDOW"],
        &["-g", "cursor-colour", "red"],
        &["cursor-colour", "blue"],
    ] {
        engine
            .execute(&mut alpha_context, &command("set-option", args))
            .unwrap();
    }
    engine
        .execute(
            &mut beta_context,
            &command("set-option", &["status-left", "BETA"]),
        )
        .unwrap();
    let mut first_pane_context =
        ExecutionContext::new(Some(alpha), Some(first_window), Some(first_pane));
    engine
        .execute(
            &mut first_pane_context,
            &command("set-option", &["-p", "cursor-colour", "green"]),
        )
        .unwrap();
    let mut hooks = CommandHooks::new(0);
    let pane_target = FormatContext {
        session: None,
        window: None,
        pane: Some(first_pane),
        active_session: Some(alpha),
        format_client: FormatClient::NoClient,
        format_type: FormatType::Pane,
    };
    assert_eq!(
        expand_option(&engine, pane_target, "cursor-colour", &mut hooks),
        "green"
    );
    let window_target = FormatContext {
        pane: None,
        window: Some(first_window),
        ..pane_target
    };
    assert_eq!(
        expand_option(&engine, window_target, "cursor-colour", &mut hooks),
        "blue"
    );
    assert_eq!(
        expand_option(
            &engine,
            FormatContext {
                session: Some(alpha),
                window: None,
                pane: None,
                ..pane_target
            },
            "cursor-colour",
            &mut hooks,
        ),
        "blue"
    );
    assert_eq!(
        expand_option(
            &engine,
            FormatContext {
                session: None,
                window: None,
                pane: None,
                format_client: FormatClient::Attached(alpha),
                ..pane_target
            },
            "cursor-colour",
            &mut hooks,
        ),
        "blue"
    );
    assert_eq!(
        expand_option(&engine, pane_target, "status-left", &mut hooks),
        "ALPHA"
    );
    assert_eq!(
        expand_option(&engine, pane_target, "window-status-format", &mut hooks),
        "FIRST-WINDOW"
    );
    assert_eq!(
        expand_option(&engine, pane_target, "escape-time", &mut hooks),
        "77"
    );
    let mut display_context =
        ExecutionContext::new(Some(alpha), Some(first_window), Some(second_pane));
    assert_eq!(
        engine
            .execute(
                &mut display_context,
                &command(
                    "display-message",
                    &[
                        "-p",
                        "-t",
                        &first_pane.to_string(),
                        "#{cursor-colour}|#{status-left}|#{window-status-format}|#{escape-time}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "green|ALPHA|FIRST-WINDOW|77"
    );
    let snapshot = engine.format_option_snapshot();
    assert_eq!(
        snapshot.lookup(
            &alpha.to_string(),
            &first_window.to_string(),
            "",
            "cursor-colour",
        ),
        Some("blue".to_owned())
    );
    assert_eq!(
        snapshot.lookup(&alpha.to_string(), "", "", "cursor-colour"),
        Some("blue".to_owned())
    );
    assert_eq!(
        snapshot.lookup("", "", &first_pane.to_string(), "cursor-colour"),
        Some("green".to_owned())
    );
    assert_eq!(
        expand_option(
            &engine,
            FormatContext {
                session: None,
                window: None,
                pane: None,
                active_session: None,
                format_client: FormatClient::NoClient,
                format_type: FormatType::None,
            },
            "cursor-colour",
            &mut hooks,
        ),
        "red"
    );
    assert!(engine.state.windows.contains_key(&second_window));
}

#[test]
fn option_formats_stay_raw_until_expand_or_time_expand() {
    let mut engine = MuxEngine::default();
    let (session, window, pane) = engine.state.create_session("work").unwrap();
    let mut command_context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    engine
        .execute(
            &mut command_context,
            &command("set-option", &["-g", "status-left", "#{session_name}"]),
        )
        .unwrap();
    let context = FormatContext {
        session: Some(session),
        window: Some(window),
        pane: Some(pane),
        active_session: Some(session),
        format_client: FormatClient::Attached(session),
        format_type: FormatType::Pane,
    };
    let mut hooks = CommandHooks::new(0);
    assert_eq!(
        expand_format_with_hooks(
            "#{status-left}|#{E:status-left}|#{T:status-left}",
            &engine,
            context,
            &mut hooks,
        ),
        "#{session_name}|work|work"
    );
}

#[test]
fn option_formats_retarget_for_session_window_and_pane_loops() {
    let mut engine = MuxEngine::default();
    let (alpha, first_window, first_pane) = engine.state.create_session("alpha").unwrap();
    let second_pane = engine
        .state
        .split_pane(first_pane, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let (second_window, second_window_pane) = engine
        .state
        .create_window_at(
            alpha,
            None,
            Some("second".to_owned()),
            PaneKind::Terminal,
            false,
        )
        .unwrap();
    let (beta, beta_window, beta_pane) = engine.state.create_session("beta").unwrap();
    for (mut context, args) in [
        (
            ExecutionContext::new(Some(alpha), Some(first_window), Some(second_pane)),
            vec!["status-left", "A"],
        ),
        (
            ExecutionContext::new(Some(beta), Some(beta_window), Some(beta_pane)),
            vec!["status-left", "B"],
        ),
        (
            ExecutionContext::new(Some(alpha), Some(first_window), Some(second_pane)),
            vec!["window-status-format", "W0"],
        ),
        (
            ExecutionContext::new(Some(alpha), Some(second_window), Some(second_window_pane)),
            vec!["window-status-format", "W1"],
        ),
        (
            ExecutionContext::new(Some(alpha), Some(first_window), Some(first_pane)),
            vec!["-p", "cursor-colour", "red"],
        ),
        (
            ExecutionContext::new(Some(alpha), Some(first_window), Some(second_pane)),
            vec!["-p", "cursor-colour", "blue"],
        ),
    ] {
        engine
            .execute(&mut context, &command("set-option", &args))
            .unwrap();
    }
    let context = FormatContext {
        session: Some(alpha),
        window: Some(first_window),
        pane: Some(second_pane),
        active_session: Some(alpha),
        format_client: FormatClient::Attached(alpha),
        format_type: FormatType::Pane,
    };
    let mut hooks = CommandHooks::new(0);
    assert_eq!(
        expand_format_with_hooks(
            "#{S:#{session_name}=#{status-left},[#{session_name}=#{status-left}]} ",
            &engine,
            context,
            &mut hooks,
        ),
        "[alpha=A]beta=B "
    );
    assert_eq!(
        expand_format_with_hooks(
            "#{W:#{window_name}=#{window-status-format},[#{window_name}=#{window-status-format}]} ",
            &engine,
            context,
            &mut hooks,
        ),
        "[0=W0]second=W1 "
    );
    assert_eq!(
        expand_format_with_hooks(
            "#{P:#{pane_index}=#{cursor-colour},[#{pane_index}=#{cursor-colour}]} ",
            &engine,
            context,
            &mut hooks,
        ),
        "0=red[1=blue] "
    );
}

#[test]
fn status_row_variables_carry_session_effective_option_values() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.expect("session id");
    engine
        .execute(
            &mut context,
            &command("set-option", &["status-left", "LOCAL "]),
        )
        .unwrap();

    let global = engine.status_row_variables_for_session(None);
    assert_eq!(
        global.base.get("status-left").map(String::as_str),
        Some(crate::tmux_options::STATUS_LEFT_DEFAULT)
    );
    assert_eq!(
        global.base.get("status-justify").map(String::as_str),
        Some("left")
    );
    assert_eq!(
        global.base.get("window-status-format").map(String::as_str),
        Some(crate::DEFAULT_WINDOW_STATUS_FORMAT)
    );
    assert_eq!(
        global
            .base
            .get("pane-status-current-style")
            .map(String::as_str),
        Some("underscore")
    );
    assert_eq!(
        global.base.get("mode-style").map(String::as_str),
        Some("noattr,bg=themeyellow,fg=themeblack")
    );
    assert_eq!(
        global.base.get("display-panes-format").map(String::as_str),
        Some("#[align=right]#{pane_unzoomed_width}x#{pane_unzoomed_height}")
    );

    let scoped = engine.status_row_variables_for_session(Some(session));
    assert_eq!(
        scoped.lookup(&session.to_string(), "", "", "status-left"),
        Some("LOCAL ".to_owned())
    );
}

#[test]
fn per_window_status_overrides_reach_the_label_surface_and_row_variables() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.expect("session id");
    let window = context.window.expect("window id");
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-t", "work:0", "window-status-current-format", "OVERRIDE"],
            ),
        )
        .unwrap();

    let formats = engine.window_status_formats(window);
    assert_eq!(
        formats.current_format, "OVERRIDE",
        "the status_label surface honors the per-window override"
    );
    let variables = engine.status_row_variables_for_session(Some(session));
    assert_eq!(
        variables
            .base
            .get("window-status-current-format")
            .map(String::as_str),
        Some(crate::DEFAULT_WINDOW_STATUS_FORMAT),
        "the session layer keeps the global value for windows without overrides"
    );
    assert_eq!(
        variables.lookup(
            &session.to_string(),
            &window.to_string(),
            "",
            "window-status-current-format",
        ),
        Some("OVERRIDE".to_owned()),
        "the row-loop surface resolves the loop window's override: both surfaces agree"
    );
    engine
        .execute(
            &mut context,
            &command("setw", &["-t", "work:0", "mode-style", "bg=red"]),
        )
        .unwrap();
    let variables = engine.status_row_variables_for_session(Some(session));
    assert_eq!(
        variables.lookup(&session.to_string(), &window.to_string(), "", "mode-style"),
        Some("bg=red".to_owned()),
        "window-scoped stored scalars layer per window too"
    );
}
