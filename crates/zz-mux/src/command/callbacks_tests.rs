use super::test_support::command;
use super::*;

#[test]
fn static_command_chain_accepts_canonical_alias_and_unique_prefix_names() {
    let commands = [
        command("display-message", &["canonical"]),
        command("display", &["alias"]),
        command("capture-pan", &["-p"]),
    ];

    validate_static_command_chain(&commands).unwrap();
}

#[test]
fn static_command_chain_rejects_later_name_option_and_arity_errors() {
    for (invalid, expected) in [
        (command("does-not-exist", &[]), "unknown command"),
        (command("display-message", &["-Q"]), "unknown flag -Q"),
        (command("display-message", &["-t"]), "expects an argument"),
        (command("list-sessions", &["extra"]), "too many arguments"),
        (command("kill-s", &[]), "ambiguous command"),
    ] {
        let commands = [command("display-message", &["before"]), invalid];
        assert!(matches!(
            validate_static_command_chain(&commands),
            Err(ServerError::CommandParse(message)) if message.contains(expected)
        ));
    }
}

#[test]
fn static_command_chain_recurses_into_typed_callback_blocks() {
    let command = command(
        "if-shell",
        &["true", "{ display-message -Q }", "display-message ok"],
    )
    .with_command_blocks([1]);

    assert!(matches!(
        validate_static_command_chain(&[command]),
        Err(ServerError::CommandParse(message)) if message.contains("unknown flag -Q")
    ));
}

#[test]
fn a_bound_menu_item_s_unsupported_flag_loads_silently_and_refuses_on_selection() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let parsed = crate::parse_config(
        "cfg.in",
        "bind-key -T root MouseDown3Pane display-menu Trim t { resize-pane -T }",
    );
    assert!(parsed.diagnostics.is_empty());

    engine
        .execute(&mut context, &parsed.commands[0])
        .expect("a menu item's accepted gap is not a config error");
    let bound = engine
        .keys
        .get("root", "MouseDown3Pane")
        .expect("root binding")
        .commands
        .clone();
    assert!(
        format_callback_commands(&bound).contains("resize-pane -T"),
        "the item is kept as written: {}",
        format_callback_commands(&bound)
    );

    assert!(matches!(
        engine.execute(&mut context, &command("resize-pane", &["-T"])),
        Err(ServerError::UnsupportedCommand(message)) if message == "resize-pane -T"
    ));
}

#[test]
fn bound_menu_items_keep_alias_preparation_and_canonical_printing() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-s", "command-alias[90]", "menu-child=break-pane -W"],
            ),
        )
        .unwrap();
    let parsed = crate::parse_config(
        "cfg.in",
        "bind-key -T root F1 display-menu Float f { menu-child } Show s { display -p shown }",
    );
    assert!(parsed.diagnostics.is_empty());
    engine.execute(&mut context, &parsed.commands[0]).unwrap();
    let bound = &engine.keys.get("root", "F1").unwrap().commands;
    assert_eq!(
        format_callback_commands(bound),
        "display-menu Float f { break-pane -W } Show s { display-message -p shown }"
    );
}

#[test]
fn bound_menu_items_validate_names_flags_and_arity_at_load() {
    for (item, expected) in [
        ("break-pane -Q", "unknown flag -Q"),
        ("break-pane extra", "too many arguments (need at most 0)"),
        ("does-not-exist", "unknown command: does-not-exist"),
    ] {
        for tail in [
            format!("display-menu Float f {{ {item} }}"),
            format!("{{ display-menu Float f {{ {item} }} }}"),
        ] {
            let mut engine = MuxEngine::default();
            let mut context = ExecutionContext::default();
            let parsed =
                crate::parse_config("cfg.in", &format!("bind-key -T root MouseDown3Pane {tail}"));
            assert!(parsed.diagnostics.is_empty());
            let refused = engine
                .execute(&mut context, &parsed.commands[0])
                .expect_err("typed menu items validate syntax while loading");
            assert!(refused.tmux_message().contains(expected), "{refused:?}");
        }
    }
}

#[test]
fn static_command_chain_enforces_callback_argument_types() {
    let command = command("display-message", &["{ display-message ok }"]).with_command_blocks([0]);

    assert!(matches!(
        validate_static_command_chain(&[command]),
        Err(ServerError::CommandParse(message)) if message.contains("must be \"string\"")
    ));
}

#[test]
fn static_command_chain_leaves_capability_and_native_checks_to_runtime() {
    let commands = [
        command("customize-mode", &[]),
        command("capture-pane", &["-C", "-p"]),
        command("agent-send", &["--submit", "hello"]),
        command("agent-send", &["--wait", "--on-block", "fail", "hello"]),
        command("inspect", &["-t", "%1", "--json"]),
        command("agent-respond", &["-t", "%1", "--allow"]),
    ];

    validate_static_command_chain(&commands).unwrap();
}

#[test]
fn static_command_chain_validates_unimplemented_tmux_syntax() {
    for (name, args, expected) in [
        ("link-window", &["-Z"][..], "unknown flag -Z"),
        ("link-window", &["extra"][..], "too many arguments"),
        ("newp", &["-B"][..], "expects an argument"),
        ("suspend-c", &["extra"][..], "too many arguments"),
    ] {
        assert!(matches!(
            validate_static_command_chain(&[command(name, args)]),
            Err(ServerError::CommandParse(message)) if message.contains(expected)
        ));
    }

    validate_static_command_chain(&[
        command("link-window", &["-t", "@1"]),
        command("newp", &["-d", "printf", "hello"]),
        command("suspend-c", &["-t", "/dev/pts/1"]),
    ])
    .unwrap();
}

#[test]
fn static_command_chain_never_mutates_invocations() {
    let commands = vec![
        command("display", &["before"]),
        command("if", &["true", "{ display -Q }"]).with_command_blocks([1]),
    ];
    let original = commands.clone();

    assert!(validate_static_command_chain(&commands).is_err());
    assert_eq!(commands, original);
}

#[test]
fn reload_config_is_a_native_argument_free_effect() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    let execution = engine
        .execute(&mut context, &command("reload-config", &[]))
        .expect("reload effect");
    assert_eq!(execution.effects, [MuxEffect::ReloadConfig]);
    assert!(matches!(
        engine.execute(&mut context, &command("reload-config", &["unexpected"])),
        Err(ServerError::NativeCommandParse(_))
    ));
}

#[test]
fn import_tmux_config_accepts_an_optional_path() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    let execution = engine
        .execute(&mut context, &command("import-tmux-config", &[]))
        .expect("import effect");
    assert_eq!(execution.effects, [MuxEffect::ImportTmuxConfig(None)]);
    let execution = engine
        .execute(
            &mut context,
            &command("import-tmux-config", &["/tmp/my tmux.conf"]),
        )
        .expect("import with path");
    assert_eq!(
        execution.effects,
        [MuxEffect::ImportTmuxConfig(Some(
            "/tmp/my tmux.conf".to_owned()
        ))]
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("import-tmux-config", &["one", "two"])
        ),
        Err(ServerError::NativeCommandParse(_))
    ));
}

#[test]
fn stored_commands_reject_parse_errors_without_replacing_state() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("bind-key", &["F8", "display-message", "before"]),
        )
        .unwrap();
    let binding = engine
        .keys
        .get("prefix", "F8")
        .expect("baseline binding")
        .clone();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("bind-key", &["F8", "ls", "unexpected"]),
            )
            .unwrap_err(),
        ServerError::InvalidCommand(
            "command list-sessions: too many arguments (need at most 0)".to_owned()
        )
    );
    assert_eq!(engine.keys.get("prefix", "F8"), Some(&binding));
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "bind-key",
                    &["F8", "choose-buffer", "-F", "format", "one", "two"],
                ),
            )
            .unwrap_err(),
        ServerError::InvalidCommand(
            "command choose-buffer: too many arguments (need at most 1)".to_owned()
        )
    );
    assert_eq!(engine.keys.get("prefix", "F8"), Some(&binding));
    for (body, expected) in [
        (
            &["F8", "ls", "-0"][..],
            "command list-sessions: unknown flag -0",
        ),
        (
            &["F8", "display-message", "-?"][..],
            "usage: display-message [-aCIjlNpv] [-c target-client] [-d delay] [-F format] [-t target-pane] [message]",
        ),
        (
            &["F8", "lsk", "-O"][..],
            "command list-keys: -O expects an argument",
        ),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("bind-key", body))
                .expect_err("stored command parse error"),
            ServerError::InvalidCommand(expected.to_owned())
        );
        assert_eq!(engine.keys.get("prefix", "F8"), Some(&binding));
    }

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-s", "command-alias[20]", "badls=list-sessions"],
            ),
        )
        .expect("user alias");
    assert_eq!(
        engine
            .execute(&mut context, &command("bind-key", &["F8", "badls", "-0"]),)
            .expect_err("stored user alias error"),
        ServerError::InvalidCommand("command list-sessions: unknown flag -0".to_owned())
    );
    assert_eq!(engine.keys.get("prefix", "F8"), Some(&binding));

    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-g", "session-created[0]", "display-message before"],
            ),
        )
        .unwrap();
    let hook = engine
        .execute(
            &mut context,
            &command("show-hooks", &["-g", "session-created"]),
        )
        .unwrap()
        .output;

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("set-hook", &["-g", "session-created[0]", "lsk one two"],),
            )
            .unwrap_err(),
        ServerError::CommandParse(
            "command list-keys: too many arguments (need at most 1)".to_owned()
        )
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["-g", "session-created"]),
            )
            .unwrap()
            .output,
        hook
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("set-hook", &["-g", "session-created[0]", "lsk -@"]),
            )
            .expect_err("stored hook flag error"),
        ServerError::CommandParse("command list-keys: invalid flag -@".to_owned())
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["-g", "session-created"]),
            )
            .unwrap()
            .output,
        hook
    );
}

#[test]
fn callback_construction_freezes_aliases_and_tracks_physical_groups() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-s", "command-alias[91]", "c10e=display-message -p"],
            ),
        )
        .expect("callback alias");

    let typed = engine
        .prepare_callback_commands(
            "c10e one ; display -p two\ndisplay -p three",
            true,
            "confirm-before",
        )
        .expect("typed callbacks");
    assert_eq!(typed[0].name, "display-message");
    assert_eq!(typed[0].args, ["-p", "one"]);
    assert!(typed.iter().all(|command| command.source.is_some()));
    assert_eq!(
        typed[0].source.as_ref().map(|source| source.line),
        typed[1].source.as_ref().map(|source| source.line)
    );
    assert_ne!(
        typed[1].source.as_ref().map(|source| source.line),
        typed[2].source.as_ref().map(|source| source.line)
    );

    let string = engine
        .prepare_callback_commands("c10e one\ndisplay -p two", false, "confirm-before")
        .expect("string callbacks");
    assert!(string.iter().all(|command| command.source.is_none()));
    assert_eq!(
        engine
            .prepare_callback_commands("wibble", true, "confirm-before")
            .expect_err("typed unknown callback"),
        ServerError::CommandParse("unknown command: wibble".to_owned())
    );
    let mut expanded =
        CommandInvocation::new("confirm-before", ["{ wibble }"]).with_command_blocks([0]);
    assert_eq!(
        engine
            .prepare_expanded_callback_command(&mut expanded, "confirm-before", false)
            .expect_err("expanded typed unknown callback"),
        ServerError::CommandParse("unknown command: wibble".to_owned())
    );
}

#[test]
fn callback_construction_carries_one_alias_layer_per_branch() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    for (index, value) in [
        (90, "loop10e=confirm-before { loop10e }"),
        (91, "inner10e=display-message -p reached"),
        (92, "outer10e=confirm-before { inner10e }"),
        (93, "left10e=display-message -p left"),
        (94, "right10e=display-message -p right"),
        (95, "optionloop10e=set-option -g @loop { optionloop10e }"),
    ] {
        engine
            .execute(
                &mut context,
                &command(
                    "set-option",
                    &["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .expect("callback alias");
    }

    for input in ["loop10e", "outer10e"] {
        assert!(matches!(
            engine.prepare_callback_commands(input, true, "confirm-before"),
            Err(ServerError::CommandParse(message))
                if message == if input == "loop10e" {
                    "unknown command: loop10e"
                } else {
                    "unknown command: inner10e"
                }
        ));
    }

    let prepared = engine
        .prepare_callback_commands(
            "confirm-before { left10e ; right10e }",
            true,
            "confirm-before",
        )
        .expect("sibling aliases retain independent budgets");
    assert_eq!(
        prepared[0].args,
        ["{ display-message -p left ; display-message -p right }"]
    );

    assert_eq!(
        engine
            .execute(
                &mut context,
                &CommandInvocation::new("set-option", ["-g", "@loop", "{ optionloop10e }"])
                    .with_command_blocks([2]),
            )
            .expect_err("typed option aliases share the recursion budget"),
        ServerError::CommandParse("unknown command: optionloop10e".to_owned())
    );
}

#[test]
fn text_config_reparses_as_bytes_for_byte_variables_and_octal_escapes() {
    let mut engine = MuxEngine::default();
    engine.set_config_environment(
        "BYTES".to_owned(),
        RawText::from_bytes(b"a\xfdb".to_vec()),
        false,
    );
    let parsed = engine.parse_config(
        "<test>",
        "set-buffer -b one \"$BYTES\"\nset-buffer -b two \"\\303\\251\\376\"\n",
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.commands.len(), 2);
    assert_eq!(parsed.commands[0].args[2].as_bytes(), b"a\xfdb");
    assert_eq!(parsed.commands[1].args[2].as_bytes(), b"\xc3\xa9\xfe");
    let plain = engine.parse_config("<test>", "set-buffer -b three \"\\101\"\n");
    assert_eq!(plain.commands[0].args[2], "A");
    let assignment = engine.parse_config("<test>", "OCTAL=a\\375b\n");
    assert_eq!(assignment.environment[0].value.as_bytes(), b"a\xfdb");
}

#[test]
fn command_alias_expands_one_layer_before_canonical_lookup() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let window = context.window.expect("window");

    engine
        .execute(&mut context, &command("split-pane", &["-h"]))
        .expect("default alias reaches split-window");
    assert_eq!(engine.state.windows[&window].pane_order().len(), 2);

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-s", "command-alias[10]", "dm=display-message -p"],
            ),
        )
        .expect("indexed alias write");
    assert_eq!(
        engine
            .execute(&mut context, &command("dm", &["appended"]))
            .expect("alias appends caller arguments")
            .output,
        "appended"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-s",
                    "command-alias[16]",
                    "typed-if=if-shell -F 1 { display-message -p true }",
                ],
            ),
        )
        .expect("typed alias write");
    let invocation = CommandInvocation::new("typed-if", ["{ display-message -p false }"])
        .with_command_blocks([0]);
    let CommandAliasResolution::Expanded(expanded) = engine.resolve_command_alias(&invocation)
    else {
        panic!("typed alias must expand");
    };
    assert_eq!(
        expanded.args,
        [
            "-F",
            "1",
            "{ display-message -p true }",
            "{ display-message -p false }"
        ]
    );
    assert!(expanded.argument_is_command_block(2));
    assert!(expanded.argument_is_command_block(3));
    assert_eq!(
        format_command(&expanded),
        "if-shell -F 1 { display-message -p true } { display-message -p false }"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-s",
                    "command-alias[11]",
                    "list-sessions=display-message -p shadowed",
                ],
            ),
        )
        .expect("shadowing alias write");
    assert_eq!(
        engine
            .execute(&mut context, &command("list-sessions", &[]))
            .expect("alias lookup precedes the command table")
            .output,
        "shadowed"
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "command-alias[12]", "chain1=chain2"]),
        )
        .expect("chain alias write");
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-s",
                    "command-alias[13]",
                    "chain2=display-message -p reached",
                ],
            ),
        )
        .expect("chain target write");
    let error = engine
        .execute(&mut context, &command("chain1", &[]))
        .expect_err("the second alias layer must not expand");
    assert!(
        matches!(
            &error,
            ServerError::CommandParse(message) if message == "unknown command: chain2"
        ),
        "{error:?}"
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("chain2", &[]))
            .expect("the first layer still expands")
            .output,
        "reached"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-s",
                    "command-alias[14]",
                    "multi=display-message -p first ; display-message -p",
                ],
            ),
        )
        .expect("multi-command alias write");
    assert_eq!(
        engine
            .execute(&mut context, &command("multi", &["last"]))
            .expect("multi-command alias")
            .output,
        "first\nlast"
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "command-alias[15]", "empty="]),
        )
        .expect("empty alias write");
    let generation = engine.state.generation();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("empty", &["display-message", "-p", "wrong"]),
            )
            .expect("empty alias"),
        Execution::default()
    );
    assert_eq!(engine.state.generation(), generation);
}

#[test]
fn caller_stream_destination_is_checked_without_mutating_panes() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "stream"]),
        )
        .unwrap();
    let generation = engine.state.generation();
    assert!(
        engine
            .command_stdin_destination_ready(&context, &command("display-message", &["-I"]))
            .is_err()
    );
    assert!(
        !engine
            .command_stdin_destination_ready(
                &context,
                &command("display-message", &["-I", "-t", "%99999"])
            )
            .unwrap()
    );
    assert!(
        engine
            .command_stdin_destination_ready(
                &context,
                &command("split-window", &["-I", "-t", "%99999"])
            )
            .is_err()
    );
    assert!(
        engine
            .command_stdin_destination_ready(&context, &command("split-window", &["-I"]))
            .unwrap()
    );
    assert_eq!(engine.state.generation(), generation);
    engine
        .execute(&mut context, &command("split-window", &["-I"]))
        .unwrap();
    assert!(
        engine
            .command_stdin_destination_ready(&context, &command("display-message", &["-I"]))
            .unwrap()
    );
}

#[test]
fn caller_stream_survives_alias_expansion_and_is_spent_by_the_first_reader() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    for (body, expected) in [
        ("source-file -", vec![true]),
        (
            "display-message -p before ; source-file local.conf ; source-file - - ; source-file -",
            vec![true, false, false],
        ),
    ] {
        engine
            .execute(
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    ["-s", "command-alias[90]", &format!("stream={body}")],
                ),
            )
            .expect("install stream alias");
        for payload in ["", "set -g @stream yes"] {
            let mut invocation = command("stream", &[]);
            invocation.set_stdin(payload);
            let execution = engine
                .execute(&mut context, &invocation)
                .expect("stream alias");
            let streams = execution
                .effects
                .into_iter()
                .filter_map(|effect| match effect {
                    MuxEffect::SourceFile { path, stdin, .. } if path == "-" => Some(stdin),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(streams.len(), expected.len());
            for (stream, available) in streams.into_iter().zip(&expected) {
                assert_eq!(
                    stream,
                    Some(if *available {
                        SourceStream::Bytes(payload.into())
                    } else {
                        SourceStream::Spent
                    })
                );
            }
        }
        let execution = engine
            .execute(&mut context, &command("stream", &[]))
            .expect("callerless alias");
        assert!(
            execution
                .effects
                .iter()
                .all(|effect| !matches!(effect, MuxEffect::SourceFile { stdin: Some(_), .. }))
        );
    }
}

#[test]
fn command_alias_group_round_trips_option_boundaries_empty_args_and_blocks() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-s",
                    "command-alias[40]",
                    "dash=display-message -p -- -literal ; display-message -p ok",
                ],
            ),
        )
        .expect("dash alias write");

    let CommandAliasResolution::Expanded(expanded) =
        engine.resolve_command_alias(&command("dash", &[]))
    else {
        panic!("dash alias must expand");
    };
    assert_eq!(
        MuxEngine::command_alias_group_body(&expanded),
        Some("display-message -p -- -literal ; display-message -p ok")
    );
    let commands = MuxEngine::command_alias_group_commands(&expanded)
        .unwrap()
        .unwrap();
    assert_eq!(commands[0].args, ["-p", "--", "-literal"]);
    assert_eq!(commands[1].args, ["-p", "ok"]);
    assert_eq!(
        engine
            .execute(&mut context, &command("dash", &[]))
            .expect("dash alias execution")
            .output,
        "-literal\nok"
    );

    engine
        .execute(&mut context, &command("bind-key", &["F10", "dash"]))
        .expect("dash alias binding");
    assert_eq!(
        format_key_command(engine.keys.get("prefix", "F10").expect("dash binding")),
        "display-message -p -literal \\; display-message -p ok"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-s",
                    "command-alias[41]",
                    "typed=display-message -p first ; if-shell -F 1",
                ],
            ),
        )
        .expect("typed alias write");
    let block = "{ if-shell -F 1 { display-message -p -- -literal ; kill-session -t missing ; set-environment -g ALIAS_SAME yes\nset-environment -g ALIAS_NEXT yes } }";
    let invocation = CommandInvocation::new("typed", ["", block]).with_command_blocks([1]);
    let CommandAliasResolution::Expanded(mut expanded) = engine.resolve_command_alias(&invocation)
    else {
        panic!("typed alias must expand");
    };
    let commands = MuxEngine::command_alias_group_commands(&expanded)
        .unwrap()
        .unwrap();
    assert_eq!(commands[1].args, ["-F", "1", "", block]);
    assert!(commands[1].argument_is_command_block(3));
    let body = MuxEngine::command_alias_group_body(&expanded).expect("typed alias body");
    assert!(body.contains('\n'));
    assert!(!body.contains(" ;; "));
    engine
        .prepare_expanded_callback_command(&mut expanded, "typed", true)
        .expect("prepare typed alias");
    let commands = MuxEngine::command_alias_group_commands(&expanded)
        .unwrap()
        .unwrap();
    let outer_body =
        crate::parser::command_block_body(&commands[1].args[3]).expect("outer callback block");
    let outer = MuxEngine::parse_config_without_variable_expansion("<outer>", outer_body);
    assert!(outer.diagnostics.is_empty());
    let inner_body = crate::parser::command_block_body(&outer.commands[0].args[2])
        .expect("inner callback block");
    let inner = MuxEngine::parse_config_without_variable_expansion("<inner>", inner_body);
    assert!(inner.diagnostics.is_empty());
    assert_eq!(inner.commands[0].args, ["-p", "--", "-literal"]);
    assert_eq!(
        inner.commands[1].source.as_ref().map(|source| source.line),
        inner.commands[2].source.as_ref().map(|source| source.line),
    );
    assert_ne!(
        inner.commands[2].source.as_ref().map(|source| source.line),
        inner.commands[3].source.as_ref().map(|source| source.line),
    );

    let nested = CommandInvocation::new(
        COMMAND_ALIAS_GROUP_NAME,
        ["{ display-message -p -- -nested ; display-message -p done }"],
    )
    .with_command_blocks([0])
    .into_expanded_alias_group();
    let body = format_callback_commands_round_trip(&[
        command("display-message", &["-p", "before"]),
        nested,
    ]);
    assert_eq!(
        body,
        "display-message -p before ; display-message -p -- -nested ; display-message -p done"
    );
    let parsed = MuxEngine::parse_config_without_variable_expansion("<nested-alias>", &body);
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.commands[1].args, ["-p", "--", "-nested"]);
}

#[test]
fn a_parsed_alias_group_name_never_becomes_an_alias_group() {
    let forged = MuxEngine::parse_config_without_variable_expansion(
        "<forgery>",
        &format!("{COMMAND_ALIAS_GROUP_NAME} {{ display-message -p pwned }}"),
    );
    assert!(forged.diagnostics.is_empty());
    let forged = forged.commands.into_iter().next().expect("parsed command");
    assert_eq!(forged.name, COMMAND_ALIAS_GROUP_NAME);
    assert!(forged.argument_is_command_block(0));
    assert!(!forged.is_expanded_alias_group());
    assert!(!MuxEngine::is_command_alias_group(&forged));
    assert_eq!(MuxEngine::command_alias_group_body(&forged), None);
    assert_eq!(
        MuxEngine::command_alias_group_commands(&forged).expect("no group body"),
        None
    );
    assert!(matches!(
        resolve_command(&forged.name),
        CommandResolution::Unknown
    ));

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "w"]))
        .expect("session");
    let error = engine
        .execute(&mut context, &forged)
        .expect_err("forged alias group must not execute");
    assert_eq!(
        error,
        ServerError::CommandParse(format!("unknown command: {COMMAND_ALIAS_GROUP_NAME}"))
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-s",
                    "command-alias[40]",
                    "two=display-message -p one ; display-message -p two",
                ],
            ),
        )
        .expect("multi-command alias");
    let CommandAliasResolution::Expanded(genuine) =
        engine.resolve_command_alias(&command("two", &[]))
    else {
        panic!("multi-command alias must expand to a group");
    };
    assert_eq!(genuine.name, COMMAND_ALIAS_GROUP_NAME);
    assert!(genuine.is_expanded_alias_group());
    assert!(MuxEngine::is_command_alias_group(&genuine));
}

#[test]
fn unsupported_command_alias_bodies_never_fall_through() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "victim"]),
        )
        .expect("victim session");
    engine
        .execute(
            &mut context,
            &command("bind-key", &["F9", "display-message", "-p", "preserved"]),
        )
        .expect("baseline binding");
    let binding_before = engine
        .execute(
            &mut context,
            &command(
                "list-keys",
                &["-T", "prefix", "-F", "#{key_string}=#{key_command}"],
            ),
        )
        .unwrap()
        .output
        .lines()
        .find(|line| line.starts_with("F9="))
        .expect("baseline binding row")
        .to_owned();
    let option_before = engine
        .execute(
            &mut context,
            &command("show-options", &["-sv", "default-client-command"]),
        )
        .unwrap()
        .output;

    for (index, value) in [
        ("20", "kill-server=display-message \\"),
        ("21", "list-windows=display-message \\"),
        ("22", "lsw=display-message \\"),
        ("23", "kill-session=display-message \\"),
    ] {
        engine
            .execute(
                &mut context,
                &command(
                    "set-option",
                    &["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .expect("alias write");
    }

    for (name, args) in [
        ("kill-server", Vec::<&str>::new()),
        ("list-windows", Vec::<&str>::new()),
        ("lsw", Vec::<&str>::new()),
        ("kill-session", vec!["-t", "=victim"]),
    ] {
        let invocation = command(name, &args);
        assert_eq!(
            engine.resolve_command_alias(&invocation),
            CommandAliasResolution::MatchedUnsupported(CommandAliasBodyError::Unparsable)
        );
        let generation = engine.state.generation();
        assert_eq!(
            engine.execute(&mut context, &invocation).unwrap_err(),
            ServerError::CommandParse(format!("unknown command: {name}"))
        );
        assert_eq!(engine.state.generation(), generation);
    }
    assert!(session_named(&engine.state, "victim").is_some());

    assert_eq!(
        engine
            .execute(&mut context, &command("bind-key", &["F9", "kill-server"]))
            .unwrap_err(),
        ServerError::InvalidCommand("unknown command: kill-server".to_owned())
    );
    let binding_after = engine
        .execute(
            &mut context,
            &command(
                "list-keys",
                &["-T", "prefix", "-F", "#{key_string}=#{key_command}"],
            ),
        )
        .unwrap()
        .output
        .lines()
        .find(|line| line.starts_with("F9="))
        .expect("preserved binding row")
        .to_owned();
    assert_eq!(binding_after, binding_before);
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("set-hook", &["-g", "session-created", "kill-server"]),
            )
            .unwrap_err(),
        ServerError::CommandParse("unknown command: kill-server".to_owned())
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "set-option",
                    &["-s", "default-client-command", "kill-server"],
                ),
            )
            .unwrap_err(),
        ServerError::CommandParse("unknown command: kill-server".to_owned())
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "default-client-command"]),
            )
            .unwrap()
            .output,
        option_before
    );
}

#[test]
fn command_error_phase_separates_parse_shapes_from_runtime_failures() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "phase"]),
        )
        .expect("session");
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-s", "command-alias[30]", "unparsed=display-message \\"],
            ),
        )
        .expect("unparsable alias");

    let invocation = command("unparsed", &[]);
    assert_eq!(
        engine.resolve_command_alias(&invocation),
        CommandAliasResolution::MatchedUnsupported(CommandAliasBodyError::Unparsable)
    );

    for invocation in [
        command("bogus-command", &[]),
        command("set-option", &["-Z", "@one", "two"]),
        command("new-session", &["-t"]),
        command("break-pane", &["unexpected"]),
        command("rename-session", &[]),
        command("find-window", &[]),
        command("set-browser-profile", &[]),
        command("reload-config", &["extra"]),
        command("unparsed", &[]),
    ] {
        let error = engine
            .execute(&mut context, &invocation)
            .expect_err("syntax failure");
        assert!(
            error.is_command_parse(),
            "{invocation:?} returned {error:?}"
        );
    }

    for invocation in [
        command("set-option", &["-g", "nonexistent-option", "value"]),
        command("set-environment", &["-g", "", "value"]),
        command("new-session", &["-d", "-s", "phase"]),
        command("kill-session", &["-t", "=missing"]),
    ] {
        let error = engine
            .execute(&mut context, &invocation)
            .expect_err("runtime failure");
        assert!(
            !error.is_command_parse(),
            "{invocation:?} returned {error:?}"
        );
    }
}

#[test]
fn command_alias_satisfies_bind_hook_and_option_command_validation() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-s", "command-alias[10]", "dm=display-message -p"],
            ),
        )
        .expect("alias write");
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-s",
                    "command-alias[11]",
                    "multi=display-message -p first ; display-message -p",
                ],
            ),
        )
        .expect("multi-command alias write");
    engine
        .execute(&mut context, &command("bind-key", &["F4", "multi", "last"]))
        .expect("multi-command alias validates at bind time");
    engine
        .execute(
            &mut context,
            &command("bind-key", &["F5", "split-pane", "-h"]),
        )
        .expect("default alias validates at bind time");
    engine
        .execute(&mut context, &command("bind-key", &["F6", "dm", "hello"]))
        .expect("user alias validates at bind time");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &["-T", "prefix", "-F", "#{key_string}=#{key_command}"],
                ),
            )
            .unwrap()
            .output
            .lines()
            .filter(|line| {
                line.starts_with("F4=") || line.starts_with("F5=") || line.starts_with("F6=")
            })
            .collect::<Vec<_>>(),
        vec![
            "F4=display-message -p first \\; display-message -p last",
            "F5=split-window -h",
            "F6=display-message -p hello",
        ]
    );
    engine
        .execute(&mut context, &command("unbind-key", &["F6"]))
        .expect("aliased binding unbinds");
    engine
        .execute(&mut context, &command("bind-key", &["F7", "wibble"]))
        .expect_err("unknown names still fail bind validation");
    engine
        .execute(
            &mut context,
            &command("set-hook", &["-g", "session-created", "dm hooked"]),
        )
        .expect("user alias validates in hook payloads");
    assert_eq!(
        engine
            .execute(&mut context, &command("show-hooks", &["-g"]))
            .unwrap()
            .output
            .lines()
            .filter(|line| line.starts_with("session-created"))
            .collect::<Vec<_>>(),
        vec!["session-created[0] display-message -p hooked"]
    );
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-s", "default-client-command", "split-pane -h"],
            ),
        )
        .expect("option-command normalization expands the alias");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "default-client-command"]),
            )
            .unwrap()
            .output,
        "split-window -h"
    );

    engine
        .execute(
            &mut context,
            &command("bind-key", &["F8", "display-message", "preserved"]),
        )
        .expect("baseline typed binding");
    let error = engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                [
                    "F8",
                    "if-shell",
                    "-F",
                    "{ display-message condition }",
                    "display-message branch",
                ],
            )
            .with_command_blocks([3]),
        )
        .expect_err("unwrapped typed if-shell condition in binding");
    assert_eq!(
        error,
        ServerError::InvalidCommand("command if-shell: argument 1 must be \"string\"".to_owned())
    );
    let error = engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                [
                    "F8",
                    "{ if-shell -F { display-message condition } 'display-message branch' }",
                ],
            )
            .with_command_blocks([1]),
        )
        .expect_err("typed if-shell condition in binding");
    assert_eq!(
        error,
        ServerError::CommandParse("command if-shell: argument 1 must be \"string\"".to_owned())
    );
    assert!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &["-T", "prefix", "-F", "#{key_string}=#{key_command}"],
                ),
            )
            .expect("preserved typed binding")
            .output
            .lines()
            .any(|line| line == "F8=display-message preserved")
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-g", "session-closed", "display-message preserved"],
            ),
        )
        .expect("baseline typed hook");
    let error = engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "session-closed",
                    "{ if-shell -F { display-message condition } 'display-message branch' }",
                ],
            )
            .with_command_blocks([2]),
        )
        .expect_err("typed if-shell condition in hook");
    assert_eq!(
        error,
        ServerError::CommandParse("command if-shell: argument 1 must be \"string\"".to_owned())
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["-g", "session-closed"]),
            )
            .expect("preserved typed hook")
            .output,
        "session-closed[0] display-message preserved"
    );

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                [
                    "F9",
                    "{ if-shell -F 1 { display-message -p true } { display-message -p false } }",
                ],
            )
            .with_command_blocks([1]),
        )
        .expect("valid typed if-shell binding");
    assert!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &["-T", "prefix", "-F", "#{key_string}=#{key_command}"],
                ),
            )
            .expect("printed typed if-shell binding")
            .output
            .lines()
            .any(|line| {
                line == "F9=if-shell -F 1 { display-message -p true } { display-message -p false }"
            })
    );

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                [
                    "-T",
                    "prefix",
                    "F10",
                    "display-message",
                    "first;",
                    "if-shell",
                    "-F",
                    "1",
                    "{ display-message -p true }",
                    "{ display-message -p false }",
                ],
            )
            .with_command_blocks([8, 9]),
        )
        .expect("valid unwrapped typed if-shell binding chain");
    assert!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &["-T", "prefix", "-F", "#{key_string}=#{key_command}"],
                ),
            )
            .expect("printed unwrapped typed if-shell binding chain")
            .output
            .lines()
            .any(|line| {
                line
                    == "F10=display-message first \\; if-shell -F 1 { display-message -p true } { display-message -p false }"
            })
    );
}

#[test]
fn stored_run_shell_callback_types_preserve_prior_commands() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    engine
        .execute(
            &mut context,
            &command("bind-key", &["F11", "display-message", "preserved"]),
        )
        .expect("baseline binding");
    let error = engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                ["F11", "run-shell", "{ display-message -p forbidden }"],
            )
            .with_command_blocks([2]),
        )
        .expect_err("typed shell command in binding");
    assert_eq!(
        error,
        ServerError::InvalidCommand("command run-shell: argument 1 must be \"string\"".to_owned())
    );
    assert!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &["-T", "prefix", "-F", "#{key_string}=#{key_command}"],
                ),
            )
            .expect("preserved binding")
            .output
            .lines()
            .any(|line| line == "F11=display-message preserved")
    );

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                ["F12", "run-shell", "-C", "{ display-message -p stored }"],
            )
            .with_command_blocks([3]),
        )
        .expect("typed command-mode binding");
    assert!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-keys",
                    &["-T", "prefix", "-F", "#{key_string}=#{key_command}"],
                ),
            )
            .expect("printed typed command-mode binding")
            .output
            .lines()
            .any(|line| line == "F12=run-shell -C { display-message -p stored }")
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-g", "session-closed", "display-message preserved"],
            ),
        )
        .expect("baseline hook");
    let error = engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "session-closed",
                    "{ run-shell { display-message -p forbidden } }",
                ],
            )
            .with_command_blocks([2]),
        )
        .expect_err("typed shell command in hook");
    assert_eq!(
        error,
        ServerError::CommandParse("command run-shell: argument 1 must be \"string\"".to_owned())
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["-g", "session-closed"]),
            )
            .expect("preserved hook")
            .output,
        "session-closed[0] display-message preserved"
    );
}

#[test]
fn if_shell_truthiness_uses_the_first_byte() {
    for value in ["", "0", "0abc"] {
        assert!(!if_shell_truthy(value), "{value:?}");
    }
    for value in [" 0", "1", "false"] {
        assert!(if_shell_truthy(value), "{value:?}");
    }
}

#[test]
fn config_condition_formats_use_pin_truth_options_environment_and_no_jobs() {
    let mut engine = MuxEngine::default();
    engine.set_format_server_context("tower.local", "tower", "/tmp/zz.sock", 1);
    engine.seed_global_environment([("WAVE_ENV", "yes"), ("host", "environment-must-not-shadow")]);
    engine.set_config_environment("HIDDEN_WAVE".to_owned(), "yes".to_owned(), true);
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "prefix", "C-a"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "@grammar-wave", "yes"]),
        )
        .unwrap();

    for condition in [
        "00",
        "0.0",
        "false",
        " ",
        "#{==:#{host},tower.local}",
        "#{==:#{prefix},C-a}",
        "#{==:#{@grammar-wave},yes}",
        "#{==:#{WAVE_ENV},yes}",
        "#{==:#{HIDDEN_WAVE},yes}",
        "#{==:#(printf spawned),}",
    ] {
        assert!(engine.evaluate_config_condition(condition), "{condition}");
    }
    for condition in ["", "0", "#(printf spawned)", "#{session_name}"] {
        assert!(!engine.evaluate_config_condition(condition), "{condition}");
    }
}

#[test]
fn config_conditions_observe_preparse_state_not_same_file_commands() {
    let mut engine = MuxEngine::default();
    let parsed = engine.parse_config(
        "test.conf",
        "set-option -g @grammar-wave yes\n\
             %if #{@grammar-wave}\n\
             set-option -g @branch same-file\n\
             %else\n\
             set-option -g @branch preparse\n\
             %endif\n",
    );
    assert_eq!(parsed.commands.len(), 2);
    assert_eq!(parsed.commands[1].args[2], "preparse");

    engine
        .execute(
            &mut ExecutionContext::default(),
            &command("set-option", &["-g", "@grammar-wave", "yes"]),
        )
        .unwrap();
    let parsed = engine.parse_config(
        "test.conf",
        "%if #{@grammar-wave}\nset-option -g @branch preexisting\n%endif\n",
    );
    assert_eq!(parsed.commands.len(), 1);
    assert_eq!(parsed.commands[0].args[2], "preexisting");
}
