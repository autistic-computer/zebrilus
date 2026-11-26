use super::test_support::command;
use super::*;

#[test]
fn config_commands_update_prefix_and_bindings() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("set", &["-g", "prefix", "C-a"]))
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("bind", &["-n", "F2", "new-window", "-n", "scratch"]),
        )
        .unwrap();
    assert_eq!(engine.keys.prefix(), "C-a");
    assert_eq!(
        engine.keys.get("root", "F2").unwrap().commands[0].name,
        "new-window"
    );
}

#[test]
fn bind_key_blocks_expand_the_live_global_environment() {
    let mut engine = MuxEngine::default();
    engine.seed_global_environment([("FOO", "hello world")]);
    engine
        .execute(
            &mut ExecutionContext::default(),
            &CommandInvocation::new("bind-key", ["Q", "{ send-keys $FOO }"])
                .with_command_blocks([1]),
        )
        .expect("environment-backed binding");

    assert_eq!(
        engine.keys.get("prefix", "Q").expect("binding").commands,
        [CommandInvocation::new("send-keys", ["hello world"])]
    );
    assert_eq!(
        engine
            .execute(
                &mut ExecutionContext::default(),
                &CommandInvocation::new(
                    "bind-key",
                    ["{ display-message -p $FOO }", "display-message"],
                )
                .with_command_blocks([0]),
            )
            .expect_err("typed key is normalized before lookup")
            .tmux_message(),
        "unknown key: display-message -p \"hello world\""
    );
}

#[test]
fn bind_key_distinguishes_typed_and_string_single_command_tails() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    engine
        .execute(
            &mut context,
            &CommandInvocation::new("bind-key", ["F1", "{ display -p typed ; list-s }"])
                .with_command_blocks([1]),
        )
        .expect("typed command tail");
    assert_eq!(
        format_key_command(engine.keys.get("prefix", "F1").expect("typed binding")),
        "display-message -p typed \\; list-sessions"
    );

    engine
        .execute(
            &mut context,
            &command("bind-key", &["F2", "display -p string"]),
        )
        .expect("string command tail");
    assert_eq!(
        format_key_command(engine.keys.get("prefix", "F2").expect("string binding")),
        "display-message -p string"
    );

    for (key, invocation) in [
        (
            "F3",
            CommandInvocation::new("bind-key", ["F3", "{}"]).with_command_blocks([1]),
        ),
        ("F4", CommandInvocation::new("bind-key", ["F4", ""])),
    ] {
        engine
            .execute(&mut context, &invocation)
            .expect("empty command tail");
        assert!(
            engine
                .keys
                .get("prefix", key)
                .expect("empty binding")
                .commands
                .is_empty()
        );
    }

    engine
        .execute(
            &mut context,
            &CommandInvocation::new("bind-key", ["F6", "{ display -p ignored }", "extra"])
                .with_command_blocks([1]),
        )
        .expect("typed first tail with extra argument");
    assert!(
        engine
            .keys
            .get("prefix", "F6")
            .expect("discarded typed tail")
            .commands
            .is_empty()
    );

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                [
                    "F6",
                    "display-message",
                    "first",
                    ";",
                    "{ display-message ignored }",
                ],
            )
            .with_command_blocks([4]),
        )
        .expect("later typed command name");
    assert_eq!(
        engine
            .keys
            .get("prefix", "F6")
            .expect("filtered typed group")
            .commands,
        [CommandInvocation::new("display-message", ["first"])]
    );

    engine
        .execute(
            &mut context,
            &command("bind-key", &["F5", "display-message", "preserved"]),
        )
        .expect("baseline binding");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("bind-key", &["F5", "{ display -p quoted }"]),
            )
            .expect_err("quoted braces stay string")
            .tmux_message(),
        "syntax error"
    );
    assert_eq!(
        engine
            .keys
            .get("prefix", "F5")
            .expect("preserved binding")
            .commands,
        [CommandInvocation::new("display-message", ["preserved"])]
    );

    engine
        .execute(
            &mut context,
            &command(
                "bind-key",
                &["-T", "late", "F10", "display-message", "kept"],
            ),
        )
        .expect("late flag baseline");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &CommandInvocation::new(
                    "bind-key",
                    ["-T", "late", "F10", "-r", "{ display-message ignored }"],
                )
                .with_command_blocks([4]),
            )
            .expect_err("late flag is a child command"),
        ServerError::InvalidCommand("unknown command: -r".to_owned())
    );
    assert_eq!(
        engine
            .keys
            .get("late", "F10")
            .expect("late flag prior binding")
            .commands,
        [CommandInvocation::new("display-message", ["kept"])]
    );

    engine
        .execute(
            &mut context,
            &command("bind-key", &["F7", "display-message", "preserved"]),
        )
        .expect("typed child baseline");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &CommandInvocation::new(
                    "bind-key",
                    ["F7", "display-message", "{ display -p argument }"],
                )
                .with_command_blocks([2]),
            )
            .expect_err("generic child string position")
            .tmux_message(),
        "command display-message: argument 1 must be \"string\""
    );
    assert_eq!(
        engine
            .keys
            .get("prefix", "F7")
            .expect("typed child preserved")
            .commands,
        [CommandInvocation::new("display-message", ["preserved"])]
    );

    for (body, expected) in [
        (
            "{ display -p one ; display -p two }",
            "unknown key: display-message -p one ; display-message -p two",
        ),
        (
            "{\n display -p one\n display -p two\n}",
            "unknown key: display-message -p one ;; display-message -p two",
        ),
    ] {
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &CommandInvocation::new("bind-key", [body, "display-message"])
                        .with_command_blocks([0]),
                )
                .expect_err("typed key is printed before validation")
                .tmux_message(),
            expected
        );
    }

    for body in ["{ wibble }", "{ list- }"] {
        assert!(matches!(
            engine.execute(
                &mut context,
                &CommandInvocation::new("bind-key", [body, "display-message"])
                    .with_command_blocks([0]),
            ),
            Err(ServerError::CommandParse(_))
        ));
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &CommandInvocation::new("bind-key", ["{}", "display-message"])
                    .with_command_blocks([0]),
            )
            .expect_err("empty typed key is rejected"),
        ServerError::InvalidCommand("unknown key: ".to_owned())
    );

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                [
                    "F11",
                    "{\n kill-pane -t missing\n set-environment -g TYPED yes\n}",
                ],
            )
            .with_command_blocks([1]),
        )
        .expect("typed physical-line groups");
    engine
        .execute(
            &mut context,
            &command(
                "bind-key",
                ["F12", "kill-pane -t missing\nset-environment -g STRING yes"].as_slice(),
            ),
        )
        .expect("string one-group tail");
    let typed = &engine
        .keys
        .get("prefix", "F11")
        .expect("typed group binding")
        .commands;
    let string = &engine
        .keys
        .get("prefix", "F12")
        .expect("string group binding")
        .commands;
    assert_ne!(typed[0].source, typed[1].source);
    assert!(typed.iter().all(|command| command.source.is_some()));
    assert!(string.iter().all(|command| command.source.is_none()));
}

#[test]
fn bind_key_constructs_nested_confirm_blocks_before_storage() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    let parsed = engine.parse_config(
        "nested-confirm.conf",
        "bind-key -T zzconfirm F1 confirm-before { display -p one ; list-sessions\n display -p two }",
    );
    assert!(parsed.diagnostics.is_empty());
    engine
        .execute(&mut context, &parsed.commands[0])
        .expect("nested typed confirm binding");

    let stored = &engine
        .keys
        .get("zzconfirm", "F1")
        .expect("confirm binding")
        .commands[0];
    assert_eq!(stored.name, "confirm-before");
    assert_eq!(
        stored.args,
        ["{ display-message -p one ; list-sessions\ndisplay-message -p two }"]
    );
    assert!(stored.argument_is_command_block(0));

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-s", "command-alias[92]", "zzconfirm=confirm-before -c x"],
            ),
        )
        .expect("confirm alias");
    let aliased = engine.parse_config(
        "aliased-confirm.conf",
        "bind-key -T zzconfirm F2 zzconfirm { display -p alias }",
    );
    assert!(aliased.diagnostics.is_empty());
    engine
        .execute(&mut context, &aliased.commands[0])
        .expect("aliased confirm binding");
    let stored = &engine
        .keys
        .get("zzconfirm", "F2")
        .expect("aliased binding")
        .commands[0];
    assert_eq!(stored.name, "confirm-before");
    assert_eq!(stored.args, ["-c", "x", "{ display-message -p alias }"]);
    assert!(stored.argument_is_command_block(2));

    engine
        .execute(
            &mut context,
            &command(
                "bind-key",
                &["-T", "zzconfirm", "F3", "display-message", "preserved"],
            ),
        )
        .expect("baseline binding");
    let error = engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                [
                    "-T",
                    "zzconfirm",
                    "F3",
                    "confirm-before",
                    "{ display-message forbidden }",
                    "-y",
                ],
            )
            .with_command_blocks([4]),
        )
        .expect_err("late confirm option");
    assert_eq!(
        error,
        ServerError::InvalidCommand(
            "command confirm-before: too many arguments (need at most 1)".to_owned()
        )
    );
    assert_eq!(
        engine
            .keys
            .get("zzconfirm", "F3")
            .expect("preserved binding")
            .commands,
        [CommandInvocation::new("display-message", ["preserved"])]
    );
}

#[test]
fn bind_key_recursively_constructs_and_prints_confirm_blocks() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    for (index, value) in [
        (96, "nested10e=display-message -p nested"),
        (97, "za10e=display-message -p staged"),
        (98, "zconfirm10e=confirm-before"),
        (99, "ztailchain10e=zconfirm10e"),
    ] {
        engine
            .execute(
                &mut context,
                &command(
                    "set-option",
                    &["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .expect("nested alias");
    }

    let parsed = engine.parse_config(
        "recursive-confirm.conf",
        "bind-key -T zzconfirm F4 confirm-before -p ok {}\n\
             bind-key -T zzconfirm F5 confirm-before {\n\
              display -p one\n\
              display -p two\n\
             }\n\
             bind-key -T zzconfirm F6 confirm-before { if-shell -F 1 { nested10e } }",
    );
    assert!(parsed.diagnostics.is_empty());
    for command in &parsed.commands {
        engine
            .execute(&mut context, command)
            .expect("recursive confirm binding");
    }

    for (key, expected) in [
        ("F4", "confirm-before -p ok {  }"),
        (
            "F5",
            "confirm-before { display-message -p one ;; display-message -p two }",
        ),
        (
            "F6",
            "confirm-before { if-shell -F 1 { display-message -p nested } }",
        ),
    ] {
        assert_eq!(
            format_key_command(
                engine
                    .keys
                    .get("zzconfirm", key)
                    .expect("recursive binding"),
            ),
            expected
        );
    }
    assert!(engine.keys.get("zzconfirm", "F5").unwrap().commands[0].args[0].contains('\n'));

    let invalid = engine.parse_config(
        "invalid-recursive-confirm.conf",
        "bind-key -T zzconfirm F7 confirm-before { if-shell -F 1 { bind-key -T { display-message bad-table } F2 display-message } }",
    );
    assert!(invalid.diagnostics.is_empty());
    assert_eq!(
        engine
            .execute(&mut context, &invalid.commands[0])
            .expect_err("nested typed option is constructed eagerly"),
        ServerError::CommandParse("command bind-key: -T argument must be a string".to_owned())
    );
    assert!(engine.keys.get("zzconfirm", "F7").is_none());

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-s",
                    "command-alias[100]",
                    "display-message=set-environment -g DOUBLE_HIT yes",
                ],
            ),
        )
        .expect("canonical shadow alias");

    let staged = engine.parse_config(
        "staged-confirm.conf",
        "bind-key -T zzconfirm F8 zconfirm10e { za10e }\n\
             bind-key -T zzconfirm F9 ztailchain10e { za10e }\n\
             bind-key -T zzconfirm F10 { zconfirm10e { za10e } }\n\
             bind-key -T zzconfirm F11 { za10e }",
    );
    assert!(staged.diagnostics.is_empty());
    engine
        .execute(&mut context, &staged.commands[0])
        .expect("independent bind tail stage");
    assert_eq!(
        format_key_command(engine.keys.get("zzconfirm", "F8").unwrap()),
        "confirm-before { display-message -p staged }"
    );
    assert_eq!(
        engine
            .execute(&mut context, &staged.commands[1])
            .expect_err("bind tail aliases consume one layer"),
        ServerError::InvalidCommand("unknown command: zconfirm10e".to_owned())
    );
    assert_eq!(
        engine
            .execute(&mut context, &staged.commands[2])
            .expect_err("typed tail aliases share one layer"),
        ServerError::CommandParse("unknown command: za10e".to_owned())
    );
    engine
        .execute(&mut context, &staged.commands[3])
        .expect("frozen typed tail");
    assert_eq!(
        engine.keys.get("zzconfirm", "F11").unwrap().commands,
        [CommandInvocation::new("display-message", ["-p", "staged"])]
    );
}

#[test]
fn bind_key_chains_execute_in_order_and_list_keys_round_trips_them() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let parsed = crate::parse_config(
        "chain.conf",
        r"bind x new-window -n one \; new-window -n two",
    );
    assert!(parsed.diagnostics.is_empty());
    engine
        .execute(&mut context, &parsed.commands[0])
        .expect("bind command chain");

    let commands = engine
        .keys
        .get("prefix", "x")
        .expect("chained binding")
        .commands
        .clone();
    assert_eq!(commands.len(), 2);
    for command in commands {
        engine
            .execute(&mut context, &command)
            .expect("execute bound command");
    }
    let session = context.session.unwrap();
    assert_eq!(engine.state.sessions[&session].windows.len(), 3);

    let listed = engine
        .execute(&mut context, &command("list-keys", &["-T", "prefix"]))
        .unwrap()
        .output;
    assert!(listed.lines().any(|line| {
        line == "bind-key    -T prefix x       new-window -n one \\; new-window -n two"
    }));
    engine
        .execute(
            &mut context,
            &command("bind-key", &["-r", "z", "new-window"]),
        )
        .expect("repeat binding");
    assert!(
        engine
            .execute(&mut context, &command("list-keys", &["-T", "prefix"]))
            .unwrap()
            .output
            .lines()
            .any(|line| line == "bind-key -r -T prefix z       new-window")
    );

    for args in [
        &["x", ";", "new-window"][..],
        &["x", "new-window", ";", ";", "new-window"][..],
    ] {
        engine
            .execute(&mut context, &command("bind-key", args))
            .expect("empty chain segments are dropped like the pin");
    }
    assert!(
        engine
            .execute(&mut context, &command("list-keys", &["-T", "prefix"]))
            .unwrap()
            .output
            .lines()
            .any(|line| line == "bind-key    -T prefix x       new-window \\; new-window")
    );
    engine
        .execute(
            &mut context,
            &command("bind-key", &["y", "new-window", ";"]),
        )
        .expect("trailing separator");
    assert_eq!(
        engine.keys.get("prefix", "y").expect("binding").commands,
        [CommandInvocation::new("new-window", [] as [&str; 0])]
    );
    engine
        .execute(
            &mut context,
            &command("bind-key", &["p", "pipe-pane", "cat"]),
        )
        .expect("pipe-pane binding");
    assert_eq!(
        engine.keys.get("prefix", "p").expect("binding").commands,
        [CommandInvocation::new("pipe-pane", ["cat"])]
    );
}

#[test]
fn list_keys_format_expands_the_pinned_per_binding_facts() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command(
                "bind-key",
                &[
                    "-r",
                    "-T",
                    "phase7d",
                    "-N",
                    "sample note",
                    "\"",
                    "split-window",
                    "-h",
                ],
            ),
        )
        .unwrap();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &[
                        "-T",
                        "phase7d",
                        "-F",
                        "#{notes_only}|#{key_repeat}|#{key_note}|#{key_prefix}|#{key_table}|#{key_string}|#{key_command}|#{key_has_repeat}|#{key_string_width}|#{key_table_width}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "0|1|sample note|C-b|phase7d|\"|split-window -h|1|1|7"
    );
}

#[test]
fn list_keys_uses_the_selected_target_format_client() {
    let mut engine = MuxEngine::default();
    let (session, window, pane) = engine.state.create_session("work").unwrap();
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    context.set_client_attached(false);
    context.set_format_client(FormatClient::Attached(session));

    let attached = engine
        .execute(
            &mut context,
            &command("list-keys", &["-T", "prefix", "-F", "#{session_active}"]),
        )
        .unwrap();
    assert!(attached.output.lines().all(|line| line == "1"));
    assert!(!attached.output.is_empty());

    context.set_format_client(FormatClient::Unattached);
    let unattached = engine
        .execute(
            &mut context,
            &command("list-keys", &["-T", "prefix", "-F", "#{session_active}"]),
        )
        .unwrap();
    assert!(unattached.output.lines().all(|line| line == "0"));
    assert!(!unattached.output.is_empty());

    context.set_format_client(FormatClient::NoClient);
    assert!(
        engine
            .execute(
                &mut context,
                &command("list-keys", &["-T", "prefix", "-F", "#{session_active}"],),
            )
            .unwrap()
            .output
            .is_empty()
    );
}

#[test]
fn list_keys_sorts_key_and_order_and_only_reverses_an_explicit_order() {
    let mut engine = MuxEngine {
        keys: KeyTables::empty(),
        ..MuxEngine::default()
    };
    for (key, repeat) in [
        ("a", false),
        ("b", false),
        ("x", true),
        ("z", false),
        ("M-a", false),
        ("C-a", false),
    ] {
        engine.keys.bind(
            "zzlk",
            key,
            Binding {
                commands: vec![CommandInvocation::new("display-message", [key])],
                repeat,
                note: Some(key.to_owned()),
            },
        );
    }
    let mut context = ExecutionContext::default();
    let mut listed = |engine: &mut MuxEngine, args: &[&str]| {
        engine
            .execute(&mut context, &command("list-keys", args))
            .unwrap()
            .output
    };

    assert_eq!(
        listed(&mut engine, &["-T", "zzlk", "-F", "#{key_string}"]),
        "a\nb\nx\nz\nM-a\nC-a"
    );
    assert_eq!(
        listed(&mut engine, &["-r", "-T", "zzlk", "-F", "#{key_string}"]),
        "a\nb\nx\nz\nM-a\nC-a"
    );
    assert_eq!(
        listed(
            &mut engine,
            &["-O", "order", "-T", "zzlk", "-F", "#{key_string}"]
        ),
        "a\nb\nx\nz\nM-a\nC-a"
    );
    assert_eq!(
        listed(
            &mut engine,
            &["-O", "order", "-r", "-T", "zzlk", "-F", "#{key_string}",]
        ),
        "C-a\nM-a\nz\nx\nb\na"
    );
    assert_eq!(
        listed(
            &mut engine,
            &["-O", "key", "-T", "zzlk", "-F", "#{key_string}"]
        ),
        "a\nM-a\nC-a\nb\nx\nz"
    );
    assert_eq!(
        listed(
            &mut engine,
            &["-O", "key", "-r", "-T", "zzlk", "-F", "#{key_string}",]
        ),
        "z\nx\nb\nC-a\nM-a\na"
    );
    assert_eq!(
        listed(
            &mut engine,
            &[
                "-O",
                "key",
                "-O",
                "order",
                "-T",
                "zzlk",
                "-F",
                "#{key_string}",
            ]
        ),
        "a\nb\nx\nz\nM-a\nC-a"
    );
}

#[test]
fn list_keys_notes_sort_prefix_and_root_independently() {
    let mut engine = MuxEngine {
        keys: KeyTables::empty(),
        ..MuxEngine::default()
    };
    for (table, key) in [
        ("prefix", "z"),
        ("prefix", "a"),
        ("root", "y"),
        ("root", "b"),
    ] {
        engine.keys.bind(
            table,
            key,
            Binding {
                commands: vec![CommandInvocation::new("display-message", [key])],
                repeat: false,
                note: Some(key.to_owned()),
            },
        );
    }
    let mut context = ExecutionContext::default();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &["-N", "-O", "key", "-F", "#{key_table}:#{key_string}",],
                ),
            )
            .unwrap()
            .output,
        "prefix:a\nprefix:z\nroot:b\nroot:y"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &["-N", "-O", "key", "-r", "-F", "#{key_table}:#{key_string}",],
                ),
            )
            .unwrap()
            .output,
        "prefix:z\nprefix:a\nroot:y\nroot:b"
    );
}

#[test]
fn list_keys_filters_before_single_and_computes_aggregate_facts_after() {
    let mut engine = MuxEngine {
        keys: KeyTables::empty(),
        ..MuxEngine::default()
    };
    for (key, repeat) in [("a", false), ("C-a", true)] {
        engine.keys.bind(
            "zzlk",
            key,
            Binding {
                commands: vec![CommandInvocation::new("display-message", [key])],
                repeat,
                note: Some(key.to_owned()),
            },
        );
    }
    let mut context = ExecutionContext::default();
    let format =
        "#{key_string}|#{key_repeat}|#{key_has_repeat}|#{key_string_width}|#{key_table_width}";
    let single = engine
        .execute(
            &mut context,
            &command(
                "list-keys",
                &["-1", "-O", "key", "-T", "zzlk", "-F", format],
            ),
        )
        .unwrap();
    assert_eq!(single.output, "");
    assert!(matches!(
        single.effects.as_slice(),
        [MuxEffect::PrintOrMessage { text, freeze: true, .. }]
            if text == "a|0|0|1|4"
    ));

    let filtered = engine
        .execute(
            &mut context,
            &command("list-keys", &["-1", "-T", "zzlk", "-F", format, "C-a"]),
        )
        .unwrap();
    assert!(matches!(
        filtered.effects.as_slice(),
        [MuxEffect::PrintOrMessage { text, .. }] if text == "C-a|1|1|3|4"
    ));

    let empty_format = engine
        .execute(
            &mut context,
            &command("list-keys", &["-1", "-T", "zzlk", "-F", ""]),
        )
        .unwrap();
    assert!(matches!(
        empty_format.effects.as_slice(),
        [MuxEffect::PrintOrMessage { text, .. }] if text.is_empty()
    ));

    engine.keys.bind(
        "other",
        "a",
        Binding {
            commands: vec![CommandInvocation::new("display-message", ["other"])],
            repeat: false,
            note: Some("other".to_owned()),
        },
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-keys", &["-F", "#{key_table}:#{key_string}", "a"],),
            )
            .unwrap()
            .output,
        "other:a\nzzlk:a"
    );
}

#[test]
fn list_keys_key_filter_errors_follow_the_pinned_precedence_and_shapes() {
    let mut engine = MuxEngine {
        keys: KeyTables::empty(),
        ..MuxEngine::default()
    };
    engine.keys.bind(
        "zzlk",
        "a",
        Binding {
            commands: vec![CommandInvocation::new("display-message", ["a"])],
            repeat: false,
            note: None,
        },
    );
    let mut context = ExecutionContext::default();
    fn error(engine: &mut MuxEngine, context: &mut ExecutionContext, args: &[&str]) -> ServerError {
        engine
            .execute(context, &command("list-keys", args))
            .unwrap_err()
    }

    assert_eq!(
        error(
            &mut engine,
            &mut context,
            &["-O", "bad", "-T", "missing", "SPELLING"]
        ),
        ServerError::InvalidCommand("invalid key: SPELLING".to_owned())
    );
    assert_eq!(
        error(
            &mut engine,
            &mut context,
            &["-O", "bad", "-T", "missing", "q"],
        ),
        ServerError::InvalidCommand("invalid sort order".to_owned())
    );
    assert_eq!(
        error(
            &mut engine,
            &mut context,
            &["-O", "key", "-T", "missing", "q"],
        ),
        ServerError::InvalidCommand("table missing doesn't exist".to_owned())
    );
    assert_eq!(
        error(&mut engine, &mut context, &["-T", "zzlk", "q"]),
        ServerError::InvalidCommand("unknown key: q".to_owned())
    );
    assert_eq!(
        error(&mut engine, &mut context, &["-N", "-T", "zzlk", "a"],),
        ServerError::InvalidCommand("unknown key: a".to_owned())
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-keys", &["-T", "zzlk", "--", "a"]),
            )
            .unwrap()
            .output,
        "bind-key  -T zzlk a display-message a"
    );
    assert_eq!(
        error(&mut engine, &mut context, &["--", "-r"]),
        ServerError::InvalidCommand("invalid key: -r".to_owned())
    );
    assert_eq!(
        error(&mut engine, &mut context, &["a", "-F", "#{key_string}"],),
        ServerError::CommandParse(
            "command list-keys: too many arguments (need at most 1)".to_owned()
        )
    );
    assert_eq!(
        error(&mut engine, &mut context, &["-O"]),
        ServerError::CommandParse("command list-keys: -O expects an argument".to_owned())
    );
}

#[test]
fn tmux_key_identity_packs_two_and_three_byte_utf8() {
    assert_eq!(tmux_key_identity("é").unwrap().base, 0x4200_a9c3);
    assert_eq!(tmux_key_identity("界").unwrap().base, 0x638c_95e7);
    assert_eq!(tmux_key_identity("😀").unwrap().base, u32::from('😀'));
    assert_eq!(tmux_key_identity("0x41").unwrap().base, 0x4100_0041);
    assert!(
        !tmux_key_identity("0x41")
            .unwrap()
            .masked_eq(&tmux_key_identity("A").unwrap())
    );

    let space = tmux_key_identity("Space").unwrap();
    let mut equivalent = tmux_key_identity(" ").unwrap();
    equivalent.flags = u8::MAX;
    assert!(space.masked_eq(&equivalent));
}

#[test]
fn list_keys_renders_and_filters_canonical_space_names() {
    let mut engine = MuxEngine::default();
    engine.keys.bind(
        "zzspace",
        "Space",
        Binding {
            commands: vec![CommandInvocation::new("display-message", ["space"])],
            repeat: false,
            note: None,
        },
    );
    assert_eq!(
        engine
            .keys
            .list(Some("zzspace"))
            .next()
            .map(|(_, key, _)| key),
        Some(" ")
    );
    assert!(
        engine
            .keys
            .list(Some("copy-mode"))
            .any(|(_, key, _)| key == "C- ")
    );

    let mut context = ExecutionContext::default();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &["-T", "zzspace", "-F", "#{key_string}|#{key_string_width}"],
                ),
            )
            .unwrap()
            .output,
        "Space|5"
    );
    for filter in ["Space", " "] {
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command(
                        "list-keys",
                        &[
                            "-T",
                            "zzspace",
                            "-F",
                            "#{key_string}|#{key_string_width}",
                            filter,
                        ],
                    ),
                )
                .unwrap()
                .output,
            "Space|5"
        );
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &[
                        "-T",
                        "copy-mode",
                        "-F",
                        "#{key_string}|#{key_string_width}",
                        "C-Space",
                    ],
                ),
            )
            .unwrap()
            .output,
        "C-Space|7"
    );
}

#[test]
fn list_keys_notes_only_matches_the_pinned_tables_filter_and_fallback() {
    let mut engine = MuxEngine {
        keys: KeyTables::empty(),
        ..MuxEngine::default()
    };
    for (table, key, note, message) in [
        ("prefix", "z", Some("prefix-note"), "prefix"),
        ("root", "a", Some("root-note"), "root"),
        ("root", "b", None, "absent-note"),
        ("root", "c", Some(""), "empty-note"),
        ("ignored", "x", Some("ignored-note"), "ignored"),
    ] {
        engine.keys.bind(
            table,
            key,
            Binding {
                commands: vec![CommandInvocation::new("display-message", [message])],
                repeat: false,
                note: note.map(str::to_owned),
            },
        );
    }

    let mut context = ExecutionContext::default();
    assert_eq!(
        engine
            .execute(&mut context, &command("list-keys", &["-N"]))
            .unwrap()
            .output,
        "C-b z prefix-note\nC-b a root-note\nC-b c display-message empty-note"
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("list-keys", &["-N", "-a"]))
            .unwrap()
            .output,
        concat!(
            "C-b z prefix-note\n",
            "C-b a root-note\n",
            "C-b b display-message absent-note\n",
            "C-b c display-message empty-note",
        )
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-keys", &["-N", "-T", "ignored"]),
            )
            .unwrap()
            .output,
        "C-b x ignored-note"
    );

    let bare = engine
        .execute(&mut context, &command("list-keys", &[]))
        .unwrap()
        .output;
    let all = engine
        .execute(&mut context, &command("list-keys", &["-a"]))
        .unwrap()
        .output;
    assert_eq!(all, bare);
}

#[test]
fn list_keys_notes_only_uses_post_filter_facts_and_literal_prefix_override() {
    let mut engine = MuxEngine {
        keys: KeyTables::empty(),
        ..MuxEngine::default()
    };
    engine.keys.bind(
        "probe",
        "a",
        Binding {
            commands: vec![CommandInvocation::new("display-message", ["alpha"])],
            repeat: false,
            note: Some("one".to_owned()),
        },
    );
    engine.keys.bind(
        "probe",
        "C-a",
        Binding {
            commands: vec![CommandInvocation::new("display-message", ["beta"])],
            repeat: true,
            note: None,
        },
    );

    let mut context = ExecutionContext::default();
    let facts = "#{notes_only}|#{key_prefix}|#{key_table}|#{key_string}|#{key_note}|#{key_command}|#{key_has_repeat}|#{key_string_width}|#{key_table_width}";
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-keys", &["-N", "-T", "probe", "-F", facts]),
            )
            .unwrap()
            .output,
        "1|C-b|probe|a|one|display-message alpha|0|1|5"
    );
    let all = engine
        .execute(
            &mut context,
            &command("list-keys", &["-N", "-a", "-T", "probe", "-F", facts]),
        )
        .unwrap()
        .output;
    assert_eq!(
        all.lines().collect::<std::collections::BTreeSet<_>>(),
        [
            "1|C-b|probe|C-a||display-message beta|1|3|5",
            "1|C-b|probe|a|one|display-message alpha|1|3|5",
        ]
        .into_iter()
        .collect()
    );

    engine.keys.set_prefix("C-a");
    for (override_prefix, expected) in [
        (None, "C-a a one"),
        (Some("two words"), "two words a one"),
        (Some(""), " a one"),
        (Some("#{session_name}"), "#{session_name} a one"),
    ] {
        let mut args = vec!["-N", "-T", "probe"];
        if let Some(prefix) = override_prefix {
            args.extend(["-P", prefix]);
        }
        assert_eq!(
            engine
                .execute(&mut context, &command("list-keys", &args))
                .unwrap()
                .output,
            expected
        );
    }
    engine.keys.set_prefix("None");
    assert_eq!(
        engine
            .execute(&mut context, &command("list-keys", &["-N", "-T", "probe"]),)
            .unwrap()
            .output,
        " a one"
    );
}

#[test]
fn list_keys_note_flags_preserve_parser_and_table_error_precedence() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    assert_eq!(
        engine
            .execute(&mut context, &command("list-keys", &["-N", "-P"]))
            .unwrap_err(),
        ServerError::CommandParse("command list-keys: -P expects an argument".to_owned())
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-keys", &["-N", "-a", "-P", "ZZ", "-T", "missing"],),
            )
            .unwrap_err(),
        ServerError::InvalidCommand("table missing doesn't exist".to_owned())
    );
}

#[test]
fn list_keys_default_pads_repeat_table_and_key_columns_like_the_pin() {
    let mut engine = MuxEngine {
        keys: KeyTables::empty(),
        ..MuxEngine::default()
    };
    engine.keys.bind(
        "long-table",
        "C-a",
        Binding {
            commands: vec![CommandInvocation::new("new-window", [] as [&str; 0])],
            repeat: true,
            note: None,
        },
    );
    engine.keys.bind(
        "x",
        "1",
        Binding {
            commands: vec![CommandInvocation::new("kill-pane", [] as [&str; 0])],
            repeat: false,
            note: None,
        },
    );

    assert_eq!(
        engine
            .execute(&mut ExecutionContext::default(), &command("list-keys", &[]),)
            .unwrap()
            .output,
        "bind-key -r -T long-table C-a new-window\nbind-key    -T x          1   kill-pane"
    );
    assert_eq!(
        engine
            .execute(
                &mut ExecutionContext::default(),
                &command("list-keys", &["-T", "x"]),
            )
            .unwrap()
            .output,
        "bind-key  -T x 1 kill-pane"
    );
}

#[test]
fn list_keys_key_command_uses_the_pinned_escaped_command_shape() {
    let mut engine = MuxEngine::default();
    engine.keys.bind(
        "phase7d",
        "X",
        Binding {
            commands: vec![
                CommandInvocation::new(
                    "new-pane",
                    ["-E", "-X", "0", "-Y", "0", "-x", "75%", "-y", "30%"],
                ),
                CommandInvocation::new("move-pane", ["-P", "bottom-centre"]),
            ],
            repeat: false,
            note: None,
        },
    );

    assert_eq!(
        engine
            .execute(
                &mut ExecutionContext::default(),
                &command(
                    "list-keys",
                    &[
                        "-T",
                        "phase7d",
                        "-F",
                        "#{key_repeat}|#{key_note}|#{key_prefix}|#{key_table}|#{key_string}|#{key_command}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "0||C-b|phase7d|X|new-pane -E -X 0 -Y 0 -x \"75%\" -y \"30%\" \\; move-pane -P bottom-centre"
    );
}

#[test]
fn list_keys_key_command_orders_flags_like_the_pinned_args_print() {
    let mut engine = MuxEngine::default();
    let vectors: [(&str, &str, &[&str], &str); 8] = [
        (
            "a",
            "split-window",
            &["-c", "#{pane_current_path}", "-v"],
            "split-window -v -c \"#{pane_current_path}\"",
        ),
        (
            "b",
            "new-window",
            &["-d", "-n", "foo", "-c", "/tmp", "-P"],
            "new-window -Pd -c /tmp -n foo",
        ),
        (
            "c",
            "send-keys",
            &["-l", "-t", "x", "hello", "world"],
            "send-keys -l -t x hello world",
        ),
        (
            "d",
            "run-shell",
            &["-b", "-d", "1", "echo hi"],
            "run-shell -b -d 1 \"echo hi\"",
        ),
        ("f", "copy-mode", &["-e", "-u"], "copy-mode -eu"),
        (
            "g",
            "send-keys",
            &["-X", "-N", "5", "cursor-down"],
            "send-keys -X -N 5 cursor-down",
        ),
        (
            "h",
            "splitw",
            &["-h", "-l", "20%", "-c", "/tmp"],
            "split-window -h -c /tmp -l \"20%\"",
        ),
        (
            "i",
            "agent-send",
            &[
                "--context=src/lib.rs",
                "--submit",
                "-t",
                "%1",
                "hello world",
            ],
            "agent-send --submit -t \"%1\" --context src/lib.rs \"hello world\"",
        ),
    ];
    for (key, name, args, _) in vectors {
        engine.keys.bind(
            "probe",
            key,
            Binding {
                commands: vec![CommandInvocation::new(name, args.iter().copied())],
                repeat: false,
                note: None,
            },
        );
    }

    let output = engine
        .execute(
            &mut ExecutionContext::default(),
            &command(
                "list-keys",
                &["-T", "probe", "-F", "#{key_string}|#{key_command}"],
            ),
        )
        .unwrap()
        .output;
    let expected = vectors
        .iter()
        .map(|(key, _, _, rendered)| format!("{key}|{rendered}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(output, expected);
}

#[test]
fn bind_key_validates_and_prints_native_long_options_without_short_clustering() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command(
                "bind-key",
                &[
                    "F8",
                    "agent-send",
                    "--context=src/lib.rs",
                    "--submit",
                    "-t",
                    "%1",
                    "hello world",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &["-T", "prefix", "-F", "#{key_string}|#{key_command}"],
                ),
            )
            .unwrap()
            .output
            .lines()
            .find(|line| line.starts_with("F8|")),
        Some("F8|agent-send --submit -t \"%1\" --context src/lib.rs \"hello world\"")
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("bind-key", &["F9", "agent-send", "--bogus"]),
            )
            .unwrap_err(),
        ServerError::NativeInvalidCommand("agent-send does not support --bogus".to_owned())
    );
}

#[test]
fn unbind_key_all_and_quiet_match_tmux_table_and_error_semantics() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("bind-key", &["-T", "custom", "F1", "new-window"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("bind-key", &["-T", "custom", "F2", "new-window"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("bind-key", &["-T", "root", "F3", "new-window"]),
        )
        .unwrap();

    engine
        .execute(
            &mut context,
            &command("unbind-key", &["-a", "-n", "-T", "custom"]),
        )
        .unwrap();
    assert!(engine.keys.table_names().all(|table| table != "custom"));
    assert!(engine.keys.table_names().any(|table| table == "root"));

    for (args, message) in [
        (&["-a", "-T", "custom"][..], "table custom doesn't exist"),
        (&["-a", "F1"][..], "key given with -a"),
        (&[][..], "missing key"),
        (&["not-a-key"][..], "unknown key: not-a-key"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("unbind-key", args))
                .unwrap_err(),
            ServerError::InvalidCommand(message.to_owned())
        );
        let mut quiet = vec!["-q"];
        quiet.extend_from_slice(args);
        engine
            .execute(&mut context, &command("unbind-key", &quiet))
            .unwrap();
    }

    for args in [&["F1", "F2"][..], &["-q", "F1", "F2"][..]] {
        assert_eq!(
            engine
                .execute(&mut context, &command("unbind-key", args))
                .unwrap_err(),
            ServerError::CommandParse(
                "command unbind-key: too many arguments (need at most 1)".to_owned()
            )
        );
    }

    engine
        .execute(
            &mut context,
            &command("bind-key", &["-T", "single", "F1", "new-window"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("unbind-key", &["-T", "single", "F1"]),
        )
        .unwrap();
    assert!(engine.keys.table_names().all(|table| table != "single"));

    for table in ["prefix", "copy-mode", "copy-mode-vi"] {
        engine
            .execute(&mut context, &command("unbind-key", &["-a", "-T", table]))
            .unwrap();
        assert_eq!(
            engine
                .execute(&mut context, &command("list-keys", &["-T", table]))
                .unwrap_err(),
            ServerError::InvalidCommand(format!("table {table} doesn't exist"))
        );
    }
    engine
        .execute(&mut context, &command("unbind-key", &["-a", "-T", "root"]))
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("list-keys", &["-T", "root"]))
            .unwrap()
            .output,
        ""
    );
}

#[test]
fn bind_key_without_a_command_only_updates_existing_metadata() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    let prefix_commands = engine
        .keys
        .get("prefix", "c")
        .expect("default prefix binding")
        .commands
        .clone();
    engine
        .execute(
            &mut context,
            &command("bind-key", &["-N", "prefix-note", "c"]),
        )
        .expect("default table metadata update");
    let prefix = engine.keys.get("prefix", "c").expect("prefix binding");
    assert_eq!(prefix.commands, prefix_commands);
    assert_eq!(prefix.note.as_deref(), Some("prefix-note"));

    engine
        .execute(
            &mut context,
            &command("bind-key", &["-n", "F3", "display-message", "root"]),
        )
        .expect("root binding");
    engine
        .execute(&mut context, &command("bind-key", &["-n", "-r", "F3"]))
        .expect("root metadata update");
    assert!(engine.keys.get("root", "F3").expect("root binding").repeat);

    engine
        .execute(
            &mut context,
            &command(
                "bind-key",
                &[
                    "-T",
                    "metadata",
                    "-N",
                    "original",
                    "F1",
                    "display-message",
                    "preserved",
                ],
            ),
        )
        .expect("baseline binding");

    engine
        .execute(
            &mut context,
            &command("bind-key", &["-T", "metadata", "F1"]),
        )
        .expect("bare binding");
    engine
        .execute(
            &mut context,
            &command(
                "bind-key",
                &["-T", "metadata", "-N", "replacement", "-r", "F1"],
            ),
        )
        .expect("metadata update");
    let binding = engine.keys.get("metadata", "F1").expect("binding remains");
    assert_eq!(
        binding.commands,
        vec![CommandInvocation::new("display-message", ["preserved"])]
    );
    assert_eq!(binding.note.as_deref(), Some("replacement"));
    assert!(binding.repeat);

    engine
        .execute(
            &mut context,
            &command("bind-key", &["-T", "metadata-empty", "-r", "F2"]),
        )
        .expect("missing binding is a no-op");
    assert!(
        engine
            .keys
            .table_names()
            .any(|table| table == "metadata-empty")
    );
    assert!(engine.keys.get("metadata-empty", "F2").is_none());

    engine
        .execute(
            &mut context,
            &command(
                "bind-key",
                &["-T", "metadata", "F1", "display-message", "replaced"],
            ),
        )
        .expect("command replacement");
    let binding = engine.keys.get("metadata", "F1").expect("replacement");
    assert_eq!(
        binding.commands,
        vec![CommandInvocation::new("display-message", ["replaced"])]
    );
    assert_eq!(binding.note, None);
    assert!(!binding.repeat);
}

#[test]
fn unbind_key_accepts_tmux_extended_key_names() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    for (key, canonical) in [
        ("mousedown1pane", "MouseDown1Pane"),
        ("m-mousedown3statusleft", "M-MouseDown3StatusLeft"),
        ("wheelupscrollbarslider", "WheelUpScrollbarSlider"),
        ("s-up", "S-Up"),
        ("c-s-down", "C-S-Down"),
        ("btab", "BTab"),
        ("pageup", "PPage"),
        ("pgdn", "NPage"),
        ("kpenter", "KPEnter"),
        ("kp0", "KP0"),
        ("kp/", "KP/"),
        ("^A", "C-a"),
        ("c-b", "C-b"),
        ("0x41", "0x41"),
        ("0x41junk", "0x41"),
        ("0x1g", "[SOH]"),
        ("0x100000041", "0x41"),
        ("User0", "User0"),
        ("User+1", "User1"),
        ("User 1", "User1"),
        ("User4294967296", "User0"),
        ("User-4294967295", "User1"),
    ] {
        engine
            .execute(
                &mut context,
                &command(
                    "bind-key",
                    &["-T", "extended", key, "display-message", "-p", "bound"],
                ),
            )
            .unwrap();
        assert!(
            engine.keys.get("extended", canonical).is_some(),
            "{key} -> {canonical}"
        );
        engine
            .execute(
                &mut context,
                &command("unbind-key", &["-T", "extended", key]),
            )
            .unwrap();
        assert!(
            engine.keys.get("extended", canonical).is_none(),
            "{key} -> {canonical}"
        );
    }
}

#[test]
fn tmux_key_parser_matches_pinned_modifier_and_named_key_vocabulary() {
    for (key, canonical) in [
        ("^A", "C-a"),
        ("^Escape", "C-Escape"),
        ("BTab", "BTab"),
        ("btab", "BTab"),
        ("KP/", "KP/"),
        ("KPEnter", "KPEnter"),
        ("KP5", "KP5"),
        ("C-KPEnter", "C-KPEnter"),
        ("c-b", "C-b"),
        ("M-C-a", "C-M-a"),
        ("C-M-S-Up", "C-M-S-Up"),
        ("S-BTab", "S-BTab"),
        ("C-Space", "C-Space"),
        ("F12", "F12"),
        ("Any", "Any"),
        ("None", "None"),
    ] {
        assert_eq!(parse_tmux_key(key).as_deref(), Some(canonical), "{key}");
    }
    for key in [
        "F13", "Ctrl-a", "Alt-a", "C-Ctrl-a", "M-Alt-a", "C-zz", "M-zz", "C-", "FocusIn",
    ] {
        assert_eq!(parse_tmux_key(key), None, "{key}");
    }
}

#[test]
fn tmux_key_parser_matches_pinned_scanf_prefixes_and_bounds() {
    for (key, canonical) in [
        ("F1", "F1"),
        ("f12", "F12"),
        ("User0001", "User1"),
        ("User+1junk", "User1"),
        ("User 1", "User1"),
        ("User\t1", "User1"),
        ("User-0", "User0"),
        ("User1000", "User1000"),
        ("User4294967296", "User0"),
        ("User4294968296", "User1000"),
        ("User-4294967295", "User1"),
        ("User-18446744073709551615", "User1"),
        ("0x041", "0x41"),
        ("0x+41", "0x41"),
        ("0x41junk", "0x41"),
        ("0x1g", "[SOH]"),
        ("0x 41", "0x41"),
        ("0x\t41", "0x41"),
        ("0x0x41", "0x41"),
        ("0x0x", "[NUL]"),
        ("0x-ffffffff", "[SOH]"),
        ("0x100000041", "0x41"),
        ("0xffffffff00000000", "[NUL]"),
    ] {
        assert_eq!(parse_tmux_key(key).as_deref(), Some(canonical), "{key}");
    }
    for key in [
        "F01",
        "F0012",
        "F+1",
        "F-1",
        "F0",
        "F13",
        "F1junk",
        "user1",
        "USER1",
        "User",
        "User+",
        "User-1",
        "User1001",
        "User18446744073709551616",
        "User-18446744073709551616",
        "0X41",
        "0x",
        "0xg1",
        "0x-1",
        "0xd800",
        "0x110000",
        "0x10000000000000000",
    ] {
        assert_eq!(parse_tmux_key(key), None, "{key}");
    }
}

#[test]
fn tmux_key_parser_accepts_literal_control_bytes_after_caret() {
    for code in 1_u8..=31 {
        let key = format!("^{}", char::from(code));
        let expected = format!("C-{}", TMUX_CONTROL_KEY_NAMES[usize::from(code)]);
        assert_eq!(parse_tmux_key(&key).as_deref(), Some(expected.as_str()));
        assert_eq!(
            parse_tmux_key_details(&key).map(|parsed| parsed.rendered),
            Some(expected)
        );
    }
    assert_eq!(parse_tmux_key("\u{1}"), None);
    assert_eq!(parse_tmux_key("C-\u{1}"), None);
}

#[test]
fn literal_delete_spellings_keep_distinct_identity_rendering_and_effects() {
    let raw = "\x7f";
    let caret = "^\x7f";
    let control_raw = "C-\x7f";
    let hex = "0x7f";
    for (key, canonical, rendered) in [
        (raw, raw, "C-?"),
        (caret, control_raw, "C-C-?"),
        (hex, hex, raw),
    ] {
        assert_eq!(parse_tmux_key(key).as_deref(), Some(canonical));
        assert_eq!(
            parse_tmux_key_details(key).map(|parsed| parsed.rendered),
            Some(rendered.to_owned())
        );
    }

    let raw_identity = tmux_key_identity(raw).unwrap();
    let control_identity = tmux_key_identity(control_raw).unwrap();
    let hex_identity = tmux_key_identity(hex).unwrap();
    assert_eq!(raw_identity.base, 0x7f);
    assert_eq!(raw_identity.modifiers, 0);
    assert_eq!(control_identity.base, 0x7f);
    assert_eq!(control_identity.modifiers, 2);
    assert_eq!(hex_identity.base, 0x4100_007f);
    assert!(!raw_identity.masked_eq(&control_identity));
    assert!(!raw_identity.masked_eq(&hex_identity));
    assert!(!control_identity.masked_eq(&hex_identity));

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "literal-delete"]),
        )
        .unwrap();
    for (key, label) in [
        (raw, "raw-delete"),
        (caret, "caret-delete"),
        (hex, "hex-delete"),
    ] {
        engine
            .execute(
                &mut context,
                &command(
                    "bind-key",
                    &["-T", "literal-delete", key, "display-message", label],
                ),
            )
            .unwrap();
    }
    assert!(engine.keys.get("literal-delete", raw).is_some());
    assert!(engine.keys.get("literal-delete", control_raw).is_some());
    assert!(engine.keys.get("literal-delete", hex).is_some());
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &["-T", "literal-delete", "-F", "#{key_string}|#{key_command}",],
                ),
            )
            .unwrap()
            .output,
        "C-?|display-message raw-delete\n\
             \x7f|display-message hex-delete\n\
             C-C-?|display-message caret-delete"
    );
    for (filter, expected) in [
        (raw, "C-?|display-message raw-delete"),
        (caret, "C-C-?|display-message caret-delete"),
        (hex, "\x7f|display-message hex-delete"),
    ] {
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command(
                        "list-keys",
                        &[
                            "-T",
                            "literal-delete",
                            "-F",
                            "#{key_string}|#{key_command}",
                            filter,
                        ],
                    ),
                )
                .unwrap()
                .output,
            expected
        );
    }

    for (key, remaining) in [(caret, &[raw, hex][..]), (raw, &[hex][..]), (hex, &[][..])] {
        engine
            .execute(
                &mut context,
                &command("unbind-key", &["-T", "literal-delete", key]),
            )
            .unwrap();
        for remaining in remaining {
            assert!(engine.keys.get("literal-delete", remaining).is_some());
        }
        assert!(engine.keys.get("literal-delete", key).is_none());
    }

    for (key, stored, rendered, sends_delete) in [
        (raw, raw, "C-?", true),
        (caret, control_raw, "C-C-?", false),
        (hex, hex, raw, true),
    ] {
        for (scope, option) in [("-g", "prefix"), ("-g", "prefix2"), ("-s", "backspace")] {
            engine
                .execute(&mut context, &command("set-option", &[scope, option, key]))
                .unwrap();
        }
        assert_eq!(engine.keys.prefix(), stored);
        assert_eq!(engine.keys.prefix2(), Some(stored));
        assert_eq!(engine.server_options.backspace, stored);
        for args in [
            &["-gv", "prefix"] as &[&str],
            &["-gv", "prefix2"],
            &["-sv", "backspace"],
        ] {
            assert_eq!(
                engine
                    .execute(&mut context, &command("show-options", args))
                    .unwrap()
                    .output,
                rendered
            );
        }
        for args in [&[] as &[&str], &["-2"]] {
            let execution = engine
                .execute(&mut context, &command("send-prefix", args))
                .unwrap();
            if sends_delete {
                assert!(matches!(
                    execution.effects.as_slice(),
                    [MuxEffect::SendKeys { keys, .. }]
                        if matches!(keys.as_slice(), [KeyToken::Literal(key)] if key == raw)
                ));
            } else {
                assert!(execution.effects.is_empty());
            }
        }
        let backspace = engine
            .execute(&mut context, &command("send-keys", &["BSpace"]))
            .unwrap();
        if sends_delete {
            assert!(matches!(
                backspace.effects.as_slice(),
                [MuxEffect::SendKeys { keys, .. }]
                    if matches!(keys.as_slice(), [KeyToken::Literal(key)] if key == raw)
            ));
        } else {
            assert!(backspace.effects.is_empty());
        }
    }
}

#[test]
fn hex_ascii_keys_keep_distinct_tmux_identity_and_render_like_the_pin() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "hex-ascii"]),
        )
        .unwrap();
    for (key, label) in [
        ("A", "literal-a"),
        ("0x41", "hex-a"),
        ("Space", "literal-space"),
        ("0x20", "hex-space"),
    ] {
        engine
            .execute(
                &mut context,
                &command(
                    "bind-key",
                    &["-T", "hex-ascii", key, "display-message", label],
                ),
            )
            .unwrap();
    }
    assert!(engine.keys.get("hex-ascii", "A").is_some());
    assert!(engine.keys.get("hex-ascii", "0x41").is_some());
    assert!(engine.keys.get("hex-ascii", "Space").is_some());
    assert!(engine.keys.get("hex-ascii", "0x20").is_some());
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &["-T", "hex-ascii", "-F", "#{key_string}|#{key_command}",],
                ),
            )
            .unwrap()
            .output,
        "Space|display-message literal-space\nA|display-message literal-a\n |display-message hex-space\nA|display-message hex-a"
    );
    for (filter, expected) in [
        ("A", "A|display-message literal-a"),
        ("0x41", "A|display-message hex-a"),
        ("Space", "Space|display-message literal-space"),
        ("0x20", " |display-message hex-space"),
    ] {
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command(
                        "list-keys",
                        &[
                            "-T",
                            "hex-ascii",
                            "-F",
                            "#{key_string}|#{key_command}",
                            filter,
                        ],
                    ),
                )
                .unwrap()
                .output,
            expected
        );
    }

    engine
        .execute(
            &mut context,
            &command("unbind-key", &["-T", "hex-ascii", "0x41"]),
        )
        .unwrap();
    assert!(engine.keys.get("hex-ascii", "A").is_some());
    assert!(engine.keys.get("hex-ascii", "0x41").is_none());
    engine
        .execute(
            &mut context,
            &command("unbind-key", &["-T", "hex-ascii", "Space"]),
        )
        .unwrap();
    assert!(engine.keys.get("hex-ascii", "0x20").is_some());

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "prefix", "0x41"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "prefix2", "0x20"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "backspace", "0x41"]),
        )
        .unwrap();
    assert_eq!(engine.keys.prefix(), "0x41");
    assert_eq!(engine.keys.prefix2(), Some("0x20"));
    assert_eq!(engine.server_options.backspace, "0x41");
    assert_eq!(engine.mux_option_value(MuxOptionKey::Prefix), "0x41");
    assert_eq!(engine.mux_option_value(MuxOptionKey::Prefix2), "0x20");
    for (args, expected) in [
        (&["-gv", "prefix"] as &[&str], "A"),
        (&["-gv", "prefix2"], " "),
        (&["-sv", "backspace"], "A"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", args))
                .unwrap()
                .output,
            expected
        );
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &["-T", "hex-ascii", "-F", "#{key_prefix}", "A"],
                ),
            )
            .unwrap()
            .output,
        "A"
    );
    for (args, expected) in [(&[] as &[&str], "A"), (&["-2"], " ")] {
        let sent = engine
            .execute(&mut context, &command("send-prefix", args))
            .unwrap();
        assert!(matches!(
            sent.effects.as_slice(),
            [MuxEffect::SendKeys { keys, .. }]
                if matches!(keys.as_slice(), [KeyToken::Named(key)] if key == expected)
        ));
    }
}

#[test]
fn literal_control_caret_keys_work_across_key_callers_without_state_drift() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command(
                "bind-key",
                &[
                    "-T",
                    "literal-control",
                    "C-a",
                    "display-message",
                    "sentinel",
                ],
            ),
        )
        .unwrap();
    let tables = engine.keys.snapshot();
    let prefix = engine.keys.prefix().to_owned();
    let prefix2 = engine.keys.prefix2().map(str::to_owned);
    let backspace = engine
        .execute(
            &mut context,
            &command("show-options", &["-sv", "backspace"]),
        )
        .unwrap()
        .output;
    let soh = format!("^{}", char::from(1));
    let tab = format!("^{}", char::from(9));
    let enter = format!("^{}", char::from(13));
    let escape = format!("^{}", char::from(27));

    engine
        .execute(
            &mut context,
            &command(
                "bind-key",
                &[
                    "-T",
                    "literal-control",
                    soh.as_str(),
                    "display-message",
                    "literal",
                ],
            ),
        )
        .unwrap();
    let listed = engine
        .execute(
            &mut context,
            &command(
                "list-keys",
                &[
                    "-1",
                    "-T",
                    "literal-control",
                    "-F",
                    "#{key_string}",
                    soh.as_str(),
                ],
            ),
        )
        .unwrap();
    assert!(matches!(
        listed.effects.as_slice(),
        [MuxEffect::PrintOrMessage { text, .. }] if text == "C-[SOH]"
    ));
    engine
        .execute(
            &mut context,
            &command("unbind-key", &["-T", "literal-control", soh.as_str()]),
        )
        .unwrap();
    assert_eq!(engine.keys.snapshot(), tables);

    for (name, key) in [("prefix", tab.as_str()), ("prefix2", enter.as_str())] {
        engine
            .execute(&mut context, &command("set-option", &["-g", name, key]))
            .unwrap();
    }
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "backspace", escape.as_str()]),
        )
        .unwrap();
    assert_eq!(engine.keys.prefix(), "C-Tab");
    assert_eq!(engine.keys.prefix2(), Some("C-Enter"));
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "backspace"]),
            )
            .unwrap()
            .output,
        "C-Escape"
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "prefix", prefix.as_str()]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-g", "prefix2", prefix2.as_deref().unwrap_or("None")],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "backspace", backspace.as_str()]),
        )
        .unwrap();
    assert_eq!(engine.keys.snapshot(), tables);
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "backspace"]),
            )
            .unwrap()
            .output,
        backspace
    );
}

#[test]
fn invalid_tmux_key_spellings_fail_before_key_state_changes() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "strict-keys"]),
        )
        .unwrap();
    for (key, label) in [("C-a", "control"), ("M-a", "meta")] {
        engine
            .execute(
                &mut context,
                &command("bind-key", &["-T", "strict", key, "display-message", label]),
            )
            .unwrap();
    }
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "prefix2", "C-s"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "backspace", "BSpace"]),
        )
        .unwrap();

    let tables = engine.keys.snapshot();
    let prefix = engine.keys.prefix().to_owned();
    let prefix2 = engine.keys.prefix2().map(str::to_owned);
    for (invocation, expected) in [
        (
            command(
                "bind-key",
                &["-T", "strict", "Ctrl-a", "display-message", "replacement"],
            ),
            "unknown key: Ctrl-a",
        ),
        (
            command(
                "bind-key",
                &["-T", "strict", "F01", "display-message", "replacement"],
            ),
            "unknown key: F01",
        ),
        (
            command(
                "bind-key",
                &["-T", "strict", "User1001", "display-message", "replacement"],
            ),
            "unknown key: User1001",
        ),
        (
            command("unbind-key", &["-T", "strict", "Alt-a"]),
            "unknown key: Alt-a",
        ),
        (
            command("list-keys", &["-T", "strict", "F0012"]),
            "invalid key: F0012",
        ),
        (
            command("set-option", &["-g", "prefix", "F+1"]),
            "bad key: F+1",
        ),
        (
            command("set-option", &["-g", "prefix2", "User-1"]),
            "bad key: User-1",
        ),
        (
            command("set-option", &["-s", "backspace", "0X41"]),
            "bad key: 0X41",
        ),
    ] {
        assert_eq!(
            engine.execute(&mut context, &invocation).unwrap_err(),
            ServerError::InvalidCommand(expected.to_owned())
        );
    }

    assert_eq!(engine.keys.snapshot(), tables);
    assert_eq!(engine.keys.prefix(), prefix);
    assert_eq!(engine.keys.prefix2(), prefix2.as_deref());
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "backspace"]),
            )
            .unwrap()
            .output,
        "BSpace"
    );
}

#[test]
fn key_options_share_tmux_canonicalization_while_bind_and_unbind_reject_none() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("bind-key", &["None", "display-message"]),
            )
            .unwrap_err(),
        ServerError::InvalidCommand("unknown key: None".to_owned())
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("unbind-key", &["None"]))
            .unwrap_err(),
        ServerError::InvalidCommand("unknown key: None".to_owned())
    );
    for (key, canonical) in [
        ("^A", "C-a"),
        ("c-b", "C-b"),
        ("pageup", "PPage"),
        ("0x41", "0x41"),
        ("0x41junk", "0x41"),
        ("0x1g", "[SOH]"),
        ("User0", "User0"),
        ("User+1", "User1"),
        ("User4294967296", "User0"),
        ("none", "None"),
    ] {
        engine
            .execute(&mut context, &command("set-option", &["-g", "prefix", key]))
            .unwrap();
        assert_eq!(engine.mux_option_value(MuxOptionKey::Prefix), canonical);
    }
}

#[test]
fn unbind_key_treats_a_fresh_root_table_as_implicit() {
    let mut engine = MuxEngine {
        keys: KeyTables::empty(),
        ..MuxEngine::default()
    };
    let mut context = ExecutionContext::default();

    for args in [
        &["-a", "-n"][..],
        &["-a", "-T", "root"][..],
        &["-T", "root", "F1"][..],
    ] {
        engine
            .execute(&mut context, &command("unbind-key", args))
            .unwrap();
    }
    assert_eq!(
        engine
            .execute(&mut context, &command("list-keys", &["-T", "root"]))
            .unwrap()
            .output,
        ""
    );
}

#[test]
fn list_commands_formats_without_a_client() {
    let mut engine = MuxEngine::default();
    let (session, window, pane) = engine.state.create_session("work").unwrap();
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    context.set_attached_client_context(Some((session, window, pane)));
    context.set_format_client(FormatClient::Attached(session));

    let listed = engine
        .execute(
            &mut context,
            &command(
                "list-commands",
                &[
                    "-F",
                    "x#{session_active}x|#{S:normal,active}",
                    "new-session",
                ],
            ),
        )
        .unwrap();
    assert_eq!(listed.output, "xx|normal");
}
