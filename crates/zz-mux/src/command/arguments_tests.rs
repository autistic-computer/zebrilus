use super::test_support::command;
use super::*;

#[test]
fn usage_exit_status_distinguishes_native_errors_and_tmux_errors() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "usage-contract"]),
        )
        .unwrap();
    for (name, args, status) in [
        ("list-panes", vec!["-Z"], 1),
        ("list-panes", vec!["--json", "-Z"], 1),
        ("list-panes", vec!["--json", "-F", "x"], 2),
        ("split-window", vec!["--kind", "browser", "-I"], 2),
        ("set-browser-url", vec![], 2),
        ("reload-config", vec!["extra"], 2),
        ("set-option", vec!["-a", "history-trickle", "1"], 2),
        (
            "set-option",
            vec!["-a", "experimental-editor-pane", "on"],
            2,
        ),
        ("rename-window", vec![], 1),
        ("no-such-command", vec![], 1),
        ("clock-mode", vec!["-Q"], 1),
    ] {
        let error = engine
            .execute(&mut context, &command(name, &args))
            .unwrap_err();
        assert_eq!(error.exit_code(), status, "{name} {args:?}: {error}");
    }
}

#[test]
fn unknown_and_unimplemented_commands_keep_distinct_error_classes() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    assert_eq!(
        engine
            .execute(&mut context, &command("wibble", &[]))
            .unwrap_err(),
        ServerError::CommandParse("unknown command: wibble".to_owned())
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("link-window", &[]))
            .unwrap_err(),
        ServerError::UnsupportedCommand("link-window".to_owned())
    );
}

#[test]
fn option_parsing_follows_getopt_word_semantics() {
    let parse = |args: &[&str], value_options: &[&str]| {
        let args = args
            .iter()
            .map(|arg| RawText::from(*arg))
            .collect::<Vec<_>>();
        parse_options(&args, value_options, &[]).expect("parsed options")
    };

    let (options, positional) = parse(&["-As", "main", "htop"], &["-s", "-c", "-n"]);
    assert!(options.has("-A"));
    assert_eq!(options.value("-s"), Some("main"));
    assert_eq!(positional, ["htop"]);

    let (options, positional) = parse(&["-dc", "#{pane_path}"], &["-s", "-c", "-n"]);
    assert!(options.has("-d"));
    assert_eq!(options.value("-c"), Some("#{pane_path}"));
    assert!(positional.is_empty());

    let (options, positional) = parse(&["-n1", "date"], &["-n"]);
    assert_eq!(options.value("-n"), Some("1"));
    assert_eq!(positional, ["date"]);

    let (options, positional) = parse(&["-c", "-foo", "-g"], &["-c"]);
    assert_eq!(options.value("-c"), Some("-foo"));
    assert!(options.has("-g"));
    assert!(positional.is_empty());

    let (options, positional) = parse(&["-g", "--", "-s", "value"], &["-s"]);
    assert!(options.has("-g"));
    assert_eq!(options.value("-s"), None);
    assert_eq!(positional, ["-s", "value"]);

    let (options, positional) = parse(&["ls", "-la", "Enter"], &["-t"]);
    assert!(options.flags.is_empty());
    assert_eq!(positional, ["ls", "-la", "Enter"]);

    let (options, positional) = parse(&["-", "-t", "1"], &["-t"]);
    assert!(options.flags.is_empty());
    assert_eq!(positional, ["-", "-t", "1"]);

    assert!(matches!(
        parse_options(&[RawText::from("-t")], &["-t"], &[]),
        Err(ServerError::CommandParse(message)) if message == "-t requires an argument"
    ));

    let (options, positional) = parse_options(
        &[RawText::from("-R10"), "-U5".into(), "-R".into()],
        &[],
        &["-R", "-U"],
    )
    .expect("parsed options");
    assert_eq!(options.value("-R"), Some("10"));
    assert_eq!(options.value("-U"), Some("5"));
    assert!(options.has("-R"));
    assert!(positional.is_empty());
}

#[test]
fn tmux_option_errors_and_optional_values_follow_the_pin() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    for (spelling, arguments, expected) in [
        (
            "ls",
            &["-0".to_owned()][..],
            "command list-sessions: unknown flag -0",
        ),
        (
            "list-sessions",
            &["-@".to_owned()][..],
            "command list-sessions: invalid flag -@",
        ),
        (
            "list-sessions",
            &["--bogus".to_owned()][..],
            "command list-sessions: invalid flag --",
        ),
        (
            "list-sessions",
            &["-?".to_owned()][..],
            "usage: list-sessions [-r] [-F format] [-f filter] [-O order]",
        ),
        (
            "lsk",
            &["-O".to_owned()][..],
            "command list-keys: -O expects an argument",
        ),
        (
            "attach",
            &["-xQ".to_owned()][..],
            "command attach-session: unknown flag -Q",
        ),
    ] {
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &CommandInvocation::new(spelling, arguments.to_vec()),
                )
                .expect_err("tmux option error"),
            ServerError::CommandParse(expected.to_owned()),
            "{spelling}"
        );
    }
    assert_eq!(
        engine
            .execute(&mut context, &command("attach", &["-x"]))
            .expect_err("no session to steal"),
        ServerError::SessionNotFound("current session".to_owned())
    );

    engine
        .execute(&mut context, &command("new-session", &[]))
        .expect("session");
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-s", "command-alias[20]", "userls=list-sessions"],
            ),
        )
        .expect("user alias");
    assert_eq!(
        engine
            .execute(&mut context, &command("userls", &["-0"]))
            .expect_err("user alias flag error"),
        ServerError::CommandParse("command list-sessions: unknown flag -0".to_owned())
    );
    for value in ["-?", "-@"] {
        assert_eq!(
            engine
                .execute(&mut context, &command("resizep", &["-D", value]))
                .expect_err("optional adjustment"),
            ServerError::InvalidCommand("adjustment invalid".to_owned())
        );
    }
    assert_eq!(
        engine
            .execute(&mut context, &command("resize-pane", &["-D", "--bogus"]))
            .expect_err("optional long flag"),
        ServerError::CommandParse("command resize-pane: invalid flag --".to_owned())
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("resize-pane", &["-D", "-Q"]))
            .expect_err("optional next flag"),
        ServerError::CommandParse("command resize-pane: unknown flag -Q".to_owned())
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("list-sessions", &["-F", "-?"]))
            .expect("required value consumes help spelling")
            .output,
        "-?"
    );
}

#[test]
fn catalogued_positional_maximums_precede_mux_targets_and_effects() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    for (spelling, canonical, arguments, maximum) in [
        ("ls", "list-sessions", &["unexpected"][..], 0),
        ("breakp", "break-pane", &["unexpected"][..], 0),
        (
            "choose-buffer",
            "choose-buffer",
            &["-F", "format", "one", "two"][..],
            1,
        ),
        (
            "choose-tree",
            "choose-tree",
            &["-F", "format", "one", "two"][..],
            1,
        ),
        (
            "display",
            "display-message",
            &["-t", "=missing", "one", "two"][..],
            1,
        ),
        (
            "displayp",
            "display-panes",
            &["-t", "missing", "one", "two"][..],
            1,
        ),
        ("selectp", "select-pane", &["-d", "one"][..], 0),
        (
            "rename",
            "rename-session",
            &["-t", "=missing", "one", "two"][..],
            1,
        ),
        ("lsk", "list-keys", &["one", "two"][..], 1),
        ("attach", "attach-session", &["-x", "unexpected"][..], 0),
        ("kill-server", "kill-server", &["unexpected"][..], 0),
    ] {
        let error = engine
            .execute(&mut context, &command(spelling, arguments))
            .expect_err("positional maximum");
        assert_eq!(
            error,
            ServerError::CommandParse(format!(
                "command {canonical}: too many arguments (need at most {maximum})"
            ))
        );
        assert!(engine.state.sessions.is_empty());
    }
}

#[test]
fn catalogued_positional_minimums_precede_mux_targets_and_effects() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    for (spelling, canonical, arguments, minimum) in [
        ("bind", "bind-key", &[][..], 1),
        ("findw", "find-window", &["-t", "=missing"][..], 1),
        ("rename", "rename-session", &["-t", "=missing"][..], 1),
        ("renamew", "rename-window", &["-t", "=missing"][..], 1),
        ("setenv", "set-environment", &["-t", "=missing"][..], 1),
        ("set", "set-option", &["-t", "=missing"][..], 1),
        ("setw", "set-window-option", &["-t", "=missing"][..], 1),
        ("source", "source-file", &["-t", "=missing"][..], 1),
    ] {
        let error = engine
            .execute(&mut context, &command(spelling, arguments))
            .expect_err("positional minimum");
        assert_eq!(
            error,
            ServerError::CommandParse(format!(
                "command {canonical}: too few arguments (need at least {minimum})"
            ))
        );
        assert!(engine.state.sessions.is_empty());
    }
}

#[test]
fn commands_keep_dash_arguments_after_their_first_positional() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let created = engine
        .execute(
            &mut context,
            &command("new-window", &["echo", "-n", "hello"]),
        )
        .unwrap();
    assert!(matches!(
        created.effects.first(),
        Some(MuxEffect::PaneCreated {
            command: Some(command),
            ..
        }) if command.as_slice() == ["echo", "-n", "hello"]
    ));
    let window = &engine.state.windows[&context.window.unwrap()];
    assert_eq!(window.name, window.index.to_string());

    let single = engine
        .execute(
            &mut context,
            &command("new-window", &["printf '%s' \"$HOME\""]),
        )
        .unwrap();
    assert!(matches!(
        single.effects.first(),
        Some(MuxEffect::PaneCreated {
            command: Some(command),
            ..
        }) if command.as_slice() == ["printf '%s' \"$HOME\""]
    ));

    let sent = engine
        .execute(&mut context, &command("send-keys", &["ls", "-la", "Enter"]))
        .unwrap();
    assert!(matches!(
        sent.effects.first(),
        Some(MuxEffect::SendKeys { keys, .. })
            if keys == &vec![
                KeyToken::Literal("ls".to_owned()),
                KeyToken::Literal("-la".to_owned()),
                KeyToken::Named("Enter".to_owned())
            ]
    ));

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "--", "prefix", "C-z"]),
        )
        .unwrap();
    assert_eq!(engine.keys.prefix(), "C-z");

    engine
        .execute(
            &mut context,
            &command("set", &["-g", "status-left", "-foo"]),
        )
        .unwrap();
    assert_eq!(engine.status.format(StatusOption::Left), Some("-foo"));
}
