use super::test_support::command;
use super::*;

#[test]
fn display_message_session_active_tracks_the_format_client() {
    let mut engine = MuxEngine::default();
    let (first, first_window, first_pane) = engine.state.create_session("first").unwrap();
    let (second, _, second_pane) = engine.state.create_session("second").unwrap();
    let active =
        |engine: &mut MuxEngine, context: &mut ExecutionContext, pane: PaneId| -> RawText {
            engine
                .execute(
                    context,
                    &command(
                        "display-message",
                        &["-p", "-t", &pane.to_string(), "#{session_active}"],
                    ),
                )
                .unwrap()
                .output
        };

    let mut attached = ExecutionContext::new(Some(first), Some(first_window), Some(first_pane));
    attached.set_attached_client_context(Some((first, first_window, first_pane)));
    assert_eq!(active(&mut engine, &mut attached, first_pane), "1");
    assert_eq!(active(&mut engine, &mut attached, second_pane), "0");

    let mut unattached = attached.clone();
    unattached.set_client_attached(false);
    assert_eq!(active(&mut engine, &mut unattached, first_pane), "0");
    assert_eq!(active(&mut engine, &mut unattached, second_pane), "0");

    let mut no_client = attached.clone();
    no_client.set_no_client();
    assert_eq!(active(&mut engine, &mut no_client, first_pane), "\n");
    assert_eq!(active(&mut engine, &mut no_client, second_pane), "\n");

    let saved = attached.clone();
    attached.set_format_client_session(Some(second));
    assert_eq!(active(&mut engine, &mut attached, first_pane), "0");
    assert_eq!(active(&mut engine, &mut attached, second_pane), "1");
    attached.set_format_client_session(None);
    assert_eq!(active(&mut engine, &mut attached, first_pane), "\n");
    attached.copy_client_attachment(&saved);
    assert_eq!(active(&mut engine, &mut attached, first_pane), "1");

    let mut empty = MuxEngine::default();
    assert_eq!(
        empty
            .execute(
                &mut ExecutionContext::default(),
                &command("display-message", &["-p", "#{session_active}"]),
            )
            .unwrap()
            .output,
        "\n"
    );
}

#[test]
fn display_message_prints_or_targets_the_requesting_client() {
    struct FixedTime;

    impl StatusHooks for FixedTime {
        fn strftime(&mut self, literal: &str) -> String {
            literal.replace("%H:%M %d-%b-%y", "09:41 23-Aug-26")
        }

        fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
            String::new()
        }
    }

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(&mut context, &command("rename-window", &["editor"]))
        .unwrap();
    let first = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();

    assert_eq!(
        engine
            .execute_with_format_hooks(
                &mut context,
                &command("display-message", &["-p"]),
                &mut FixedTime,
            )
            .unwrap()
            .output,
        "[work] 0:editor, current pane 1 - (09:41 23-Aug-26)"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &["-p", "#{pane_format}|#{window_format}|#{session_format}"],
                ),
            )
            .unwrap()
            .output,
        "1|0|0"
    );
    let displayed = engine
        .execute(
            &mut context,
            &command(
                "display",
                &["-t", &first.to_string(), "hello #{pane_id} #P"],
            ),
        )
        .unwrap();
    assert_eq!(
        displayed.effects,
        [MuxEffect::DisplayMessage {
            pane: Some(first),
            target_client: None,
            text: format!("hello {first} 0"),
            duration_ms: None,
            freeze: true,
            ignore_keys: false,
        }]
    );

    let mut empty = MuxEngine::default();
    assert_eq!(
        empty
            .execute_with_format_hooks(
                &mut ExecutionContext::default(),
                &command("display-message", &[]),
                &mut FixedTime,
            )
            .unwrap()
            .effects,
        [MuxEffect::DisplayMessage {
            pane: None,
            target_client: None,
            text: "[] :, current pane  - (09:41 23-Aug-26)".to_owned(),
            duration_ms: None,
            freeze: true,
            ignore_keys: false,
        }]
    );
}

#[test]
fn display_message_keeps_client_and_pane_targets_independent() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "source"]))
        .expect("create source session");
    let source_pane = context.pane.expect("source pane");
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "other"]),
        )
        .expect("create other session");

    let execution = engine
        .execute(
            &mut context,
            &command(
                "display-message",
                &[
                    "-c",
                    "destination:",
                    "-t",
                    &source_pane.to_string(),
                    "#{session_name}:#{pane_id}",
                ],
            ),
        )
        .expect("display targeted message");
    assert!(matches!(
        execution.effects.as_slice(),
        [MuxEffect::DisplayMessage {
            pane: Some(pane),
            target_client: Some(target_client),
            text,
            ..
        }] if *pane == source_pane
            && target_client == "destination:"
            && text == &format!("source:{source_pane}")
    ));
}

#[test]
fn display_message_falls_forward_from_partial_can_fail_targets() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "alpha"]))
        .expect("create target session");
    engine
        .execute(&mut context, &command("rename-window", &["current"]))
        .expect("name current window");
    let current_pane = context.pane.expect("current pane");
    engine
        .execute(&mut context, &command("new-window", &["-n", "requested"]))
        .expect("create requested window");
    let requested_pane = context.pane.expect("requested pane");
    engine
        .execute(
            &mut context,
            &command("select-window", &["-t", "alpha:current"]),
        )
        .expect("restore current window");

    for (target, expected) in [
        ("alpha:missing", format!("alpha|current|{current_pane}")),
        (
            "alpha:requested.999",
            format!("alpha|requested|{requested_pane}"),
        ),
        ("=missing:requested", "||".to_owned()),
        ("%999", "||".to_owned()),
    ] {
        let output = engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &[
                        "-p",
                        "-t",
                        target,
                        "#{session_name}|#{window_name}|#{pane_id}",
                    ],
                ),
            )
            .expect("display partial target")
            .output;
        assert_eq!(output, expected, "target {target}");
    }

    let effect = engine
        .execute(
            &mut context,
            &command("display-message", &["-t", "alpha:missing", "message"]),
        )
        .expect("display through partial target");
    assert!(matches!(
        effect.effects.as_slice(),
        [MuxEffect::DisplayMessage {
            pane: Some(pane),
            ..
        }] if *pane == current_pane
    ));
}

#[test]
fn display_message_takes_the_pins_no_freeze_flag_and_delay_value() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "timed"]))
        .expect("create session");

    for (arguments, duration_ms, freeze, ignore_keys) in [
        (vec!["plain"], None, true, false),
        (vec!["-C", "flowing"], None, false, false),
        (vec!["-N", "ignored"], None, true, true),
        (vec!["-d", "1500", "delayed"], Some(1500), true, false),
        (vec!["-N", "-d", "0", "pinned"], Some(0), true, true),
        (
            vec!["-C", "-N", "-d", "4294967295", "both"],
            Some(u32::MAX),
            false,
            true,
        ),
        (vec!["-d", "+5", "signed"], Some(5), true, false),
    ] {
        let execution = engine
            .execute(&mut context, &command("display-message", &arguments))
            .expect("display message");
        assert!(
            matches!(
                execution.effects.as_slice(),
                [MuxEffect::DisplayMessage {
                    duration_ms: actual,
                    freeze: actual_freeze,
                    ignore_keys: actual_ignore_keys,
                    ..
                }] if *actual == duration_ms
                    && *actual_freeze == freeze
                    && *actual_ignore_keys == ignore_keys
            ),
            "display-message {arguments:?} produced {:?}",
            execution.effects
        );
    }
}

#[test]
fn display_message_delay_reports_the_pins_strtonum_failures() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "timed"]))
        .expect("create session");

    for (arguments, message) in [
        (vec!["-d", "abc", "x"], "delay invalid"),
        (vec!["-d", "5.5", "x"], "delay invalid"),
        (vec!["-d", "0x10", "x"], "delay invalid"),
        (vec!["-d", "", "x"], "delay invalid"),
        (vec!["-d", "-5", "x"], "delay too small"),
        (vec!["-d", "4294967296", "x"], "delay too large"),
        (
            vec!["-d", "99999999999999999999999999", "x"],
            "delay too large",
        ),
        (vec!["-p", "-d", "abc", "x"], "delay invalid"),
    ] {
        assert!(
            matches!(
                engine.execute(&mut context, &command("display-message", &arguments)),
                Err(ServerError::InvalidCommand(ref actual)) if actual == message
            ),
            "display-message {arguments:?} did not answer {message}"
        );
    }
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("display-message", &["-d", "abc", "-F", "x", "y"]),
        ),
        Err(ServerError::CommandParse(ref actual))
            if actual == "only one of -F or argument must be given"
    ));
}

#[test]
fn display_message_keeps_empty_and_existing_line_terminators() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    for (text, expected) in [("", "\n"), ("\n", "\n\n"), ("line\n", "line\n\n")] {
        let output = engine
            .execute(&mut context, &command("display-message", &["-p", text]))
            .expect("printed message")
            .output;
        assert_eq!(output.as_bytes(), expected.as_bytes());
    }
}

#[test]
fn display_message_format_option_selects_the_print_template() {
    let mut engine = MuxEngine::default();
    engine.set_format_server_context("tower.local", "tower", "/tmp/zz.sock", 40);
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("display-message", &["-p", "-F", "#{start_time}|#{version}"]),
            )
            .unwrap()
            .output,
        "40|3.8-zz"
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("display-message", &["-p", "-F", "#{start_time}", "message"]),
        ),
        Err(ServerError::CommandParse(message))
            if message == "only one of -F or argument must be given"
    ));
}

#[test]
fn display_message_literal_keeps_target_conflict_and_delay_precedence() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("display-message", &["-p", "-l", "#{session_name}|%H"]),
            )
            .unwrap()
            .output,
        "#{session_name}|%H"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("display-message", &["-p", "-l", "-F", "#{pane_id}"]),
            )
            .unwrap()
            .output,
        "#{pane_id}"
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("display-message", &["-p", "-l"]),)
            .unwrap()
            .output,
        DEFAULT_DISPLAY_MESSAGE
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "display-message",
                &["-l", "-t", "missing", "-d", "bad", "-F", "x", "y"],
            ),
        ),
        Err(ServerError::CommandParse(message))
            if message == "only one of -F or argument must be given"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("display-message", &["-l", "-d", "bad", "-F", "x", "y"]),
        ),
        Err(ServerError::CommandParse(message))
            if message == "only one of -F or argument must be given"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("display-message", &["-l", "-d", "bad", "x"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "delay invalid"
    ));

    assert!(matches!(
        engine
            .execute(
                &mut context,
                &command("display-message", &["-t", "=missing:", "quiet"]),
            )
            .unwrap()
            .effects
            .as_slice(),
        [MuxEffect::DisplayMessage {
            pane: None,
            text,
            duration_ms: None,
            ..
        }] if text == "quiet"
    ));
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &[
                        "-p",
                        "-c",
                        "missing",
                        "-t",
                        "=missing:",
                        "#{session_name}|#{window_name}|#{pane_id}|#{client_name}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "|||"
    );
}

#[test]
fn display_message_verbose_prints_the_pinned_format_log_lines() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "trace"]))
        .expect("session");
    engine
        .execute(&mut context, &command("select-pane", &["-T", "macbook"]))
        .expect("title");

    let traced = engine
        .execute(
            &mut context,
            &command(
                "display-message",
                &["-v", "-p", "#{?pane_dead,dead,#{=3:pane_title}}"],
            ),
        )
        .expect("verbose expansion")
        .output;
    assert_eq!(
        traced,
        concat!(
            "# expanding format: #{?pane_dead,dead,#{=3:pane_title}}\n",
            "# found #{}: ?pane_dead,dead,#{=3:pane_title}\n",
            "# condition is: pane_dead\n",
            "# condition 'pane_dead' found: 0\n",
            "# condition 'pane_dead' is false\n",
            "# no condition matched in 'pane_dead,dead,#{=3:pane_title}'; using last arg\n",
            "#  expanding format: #{=3:pane_title}\n",
            "#  found #{}: =3:pane_title\n",
            "#   expanding format: 3\n",
            "#   result is: 3\n",
            "#  modifier 0 is =\n",
            "#  modifier 0 argument 0: 3\n",
            "#  format 'pane_title' found: macbook\n",
            "#  applied length limit 3: mac\n",
            "#  replaced '=3:pane_title' with 'mac'\n",
            "#  result is: mac\n",
            "# replaced '?pane_dead,dead,#{=3:pane_title}' with 'mac'\n",
            "# result is: mac\n",
            "mac"
        )
    );

    let literal = engine
        .execute(
            &mut context,
            &command(
                "display-message",
                &["-v", "-p", "#{=#{pane_title}:pane_title}"],
            ),
        )
        .expect("failed modifier parse")
        .output;
    assert_eq!(
        literal,
        concat!(
            "# expanding format: #{=#{pane_title}:pane_title}\n",
            "# found #{}: =#{pane_title}:pane_title\n",
            "# expanding inner format '=#{pane_title}:pane_title'\n",
            "#  expanding format: =#{pane_title}:pane_title\n",
            "#  found #{}: pane_title\n",
            "#  format 'pane_title' found: macbook\n",
            "#  replaced 'pane_title' with 'macbook'\n",
            "#  result is: =macbook:pane_title\n",
            "# replaced '=#{pane_title}:pane_title' with '=macbook:pane_title'\n",
            "# result is: =macbook:pane_title\n",
            "=macbook:pane_title"
        )
    );

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("display-message", &["-v", "-p", "-l", "#{pane_title}"]),
            )
            .expect("literal message")
            .output,
        "#{pane_title}"
    );
}

#[test]
fn command_prompt_builds_a_native_client_effect() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    let execution = engine
        .execute(
            &mut context,
            &command(
                "command-prompt",
                &[
                    "-b",
                    "-p",
                    "window name",
                    "-I",
                    "scratch",
                    "new-window -n %%",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        execution.effects,
        vec![MuxEffect::CommandPrompt {
            steps: vec![CommandPromptStep {
                label: "window name ".to_owned(),
                input: "scratch".to_owned(),
            }],
            template: Some(CommandPromptTemplate::String("new-window -n %%".to_owned(),)),
            source: None,
            prompt_type: CommandPromptType::Command,
            mode: CommandPromptMode::Text,
            no_freeze: false,
            pane: None,
        }]
    );

    engine
        .execute(&mut context, &command("new-session", &["-s", "work tree"]))
        .expect("prompt context session");
    engine
        .execute(&mut context, &command("rename-window", &["editor pane"]))
        .expect("prompt context window");
    let execution = engine
        .execute(
            &mut context,
            &command(
                "command-prompt",
                &["-I", "#S / #W", "rename-window -- '%%'"],
            ),
        )
        .expect("expanded prompt input");
    assert_eq!(
        execution.effects,
        vec![MuxEffect::CommandPrompt {
            steps: vec![CommandPromptStep {
                label: "(rename-window) ".to_owned(),
                input: "work tree / editor pane".to_owned(),
            }],
            template: Some(CommandPromptTemplate::String(
                "rename-window -- '%%'".to_owned(),
            )),
            source: None,
            prompt_type: CommandPromptType::Command,
            mode: CommandPromptMode::Text,
            no_freeze: false,
            pane: None,
        }]
    );
    let with_pane_flag = engine
        .execute(
            &mut context,
            &command(
                "command-prompt",
                &[
                    "-P",
                    "-I",
                    "work tree / editor pane",
                    "rename-window -- '%%'",
                ],
            ),
        )
        .expect("-P raises the prompt on the pane it targets");
    assert_eq!(
        with_pane_flag.effects,
        vec![MuxEffect::CommandPrompt {
            steps: vec![CommandPromptStep {
                label: "(rename-window) ".to_owned(),
                input: "work tree / editor pane".to_owned(),
            }],
            template: Some(CommandPromptTemplate::String(
                "rename-window -- '%%'".to_owned(),
            )),
            source: None,
            prompt_type: CommandPromptType::Command,
            mode: CommandPromptMode::Text,
            no_freeze: false,
            pane: context.pane,
        }]
    );
    assert!(context.pane.is_some());

    for (index, value) in [
        (90, "a10e=display-message"),
        (
            91,
            "display-message=set-environment -g COMMAND_PROMPT_DOUBLE yes",
        ),
    ] {
        engine
            .execute(
                &mut context,
                &command(
                    "set-option",
                    &["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .expect("set prompt alias");
    }
    let parsed = engine.parse_config("prompt.conf", "command-prompt -I x { a10e }");
    assert!(parsed.diagnostics.is_empty());
    let execution = engine
        .execute(&mut context, &parsed.commands[0])
        .expect("typed prompt effect");
    assert!(matches!(
        execution.effects.as_slice(),
        [MuxEffect::CommandPrompt {
            template: Some(CommandPromptTemplate::Commands(commands)),
            ..
        }] if commands.len() == 1 && commands[0].name == "display-message"
    ));
}

#[test]
fn command_prompt_input_expands_formats_in_the_target_context() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    let steps = |engine: &mut MuxEngine, context: &mut ExecutionContext, flags: &[&str]| {
        let execution = engine
            .execute(context, &command("command-prompt", flags))
            .expect("prompt effect");
        match execution.effects.into_iter().next().expect("one effect") {
            MuxEffect::CommandPrompt { steps, .. } => steps
                .into_iter()
                .map(|step| (step.label, step.input))
                .collect::<Vec<_>>(),
            other => panic!("unexpected effect {other:?}"),
        }
    };
    let owned = |pairs: &[(&str, &str)]| {
        pairs
            .iter()
            .map(|(label, input)| ((*label).to_owned(), (*input).to_owned()))
            .collect::<Vec<_>>()
    };

    assert_eq!(
        steps(&mut engine, &mut context, &["-I", "#S:#{pane_title}"]),
        owned(&[(":", ":")])
    );

    engine
        .execute(&mut context, &command("new-session", &["-s", "work tree"]))
        .expect("prompt context session");
    engine
        .execute(&mut context, &command("rename-window", &["editor pane"]))
        .expect("prompt context window");
    engine
        .execute(&mut context, &command("select-pane", &["-T", "build log"]))
        .expect("prompt context pane title");

    assert_eq!(
        steps(
            &mut engine,
            &mut context,
            &["-I", "#{pane_title}", "-p", "x:", "display-message %%"],
        ),
        owned(&[("x: ", "build log")])
    );
    assert_eq!(
        steps(
            &mut engine,
            &mut context,
            &[
                "-p",
                "a,b,c",
                "-I",
                "#S,#{window_name}",
                "display-message %1/%2/%3"
            ],
        ),
        owned(&[("a ", "work tree"), ("b ", "editor pane"), ("c ", "")])
    );
    assert_eq!(
        steps(
            &mut engine,
            &mut context,
            &["-l", "-p", "a,b", "-I", "#{?pane_active,on,off} ## #W"],
        ),
        owned(&[("a,b", "on # editor pane")])
    );
    assert_eq!(
        steps(&mut engine, &mut context, &["-P", "-I", "#{pane_title}"]),
        owned(&[(":", "build log")])
    );
}

#[test]
fn command_prompt_substitution_preserves_structured_arguments() {
    let engine = MuxEngine::default();
    let parsed = engine.parse_config(
        "prompt.conf",
        "if-shell -F 1 { set-environment -g PROMPT_VALUE '%%:%%' }",
    );
    assert!(parsed.diagnostics.is_empty());
    let mut commands = parsed.commands;
    engine
        .prepare_frozen_callback_invocations(&mut commands, "command-prompt")
        .expect("prepare typed prompt commands");
    let input = "x\" ; set-environment -g PROMPT_INJECTED yes ; display-message -p \"";
    engine
        .substitute_command_prompt_commands(&mut commands, &[input])
        .expect("substitute typed prompt input");

    let body =
        crate::parser::command_block_body(&commands[0].args[2]).expect("structured branch body");
    let nested = MuxEngine::parse_config_without_variable_expansion("prompt.conf", body);
    assert!(nested.diagnostics.is_empty());
    assert_eq!(nested.commands.len(), 1);
    assert_eq!(nested.commands[0].name, "set-environment");
    assert_eq!(
        nested.commands[0].args,
        ["-g", "PROMPT_VALUE", &format!("{input}:%%")]
    );
}

#[test]
fn command_prompt_substitution_matches_tmux_placeholders_and_quoting() {
    assert_eq!(
        MuxEngine::substitute_command_prompt_template("%%:%%:%1:%1:%2", &["abc"]),
        "abc:%%:abc:abc:%2"
    );
    assert_eq!(
        MuxEngine::substitute_command_prompt_template("%%%|%1%", &["\"\\$;~"]),
        r#"\"\\\$\;\~|\"\\\$\;\~"#
    );
    assert_eq!(
        MuxEngine::substitute_command_prompt_template("'%%'|%1|%1%|%%", &["it's \"x\""]),
        r#"'it'\''s "x"'|it's "x"|it's \"x\"|%%"#
    );
    assert_eq!(
        MuxEngine::substitute_command_prompt_template("%%%|%%", &["a'b"]),
        "a'b|%%"
    );

    let engine = MuxEngine::default();
    let parsed = engine.parse_config("prompt.conf", "display-message -p '%%:%%:%1:%1:%2:%%%:%1%'");
    assert!(parsed.diagnostics.is_empty());
    let mut commands = parsed.commands;
    engine
        .substitute_command_prompt_commands(&mut commands, &["abc"])
        .expect("substitute typed prompt placeholders");
    assert_eq!(commands[0].args[1], "abc:%%:abc:abc:%2:%%%:abc");
}

#[test]
fn command_prompt_resolves_the_pinned_mode_ladder() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    let mode = |engine: &mut MuxEngine, context: &mut ExecutionContext, flags: &[&str]| {
        let execution = engine
            .execute(context, &command("command-prompt", flags))
            .expect("prompt effect");
        match execution.effects.into_iter().next().expect("one effect") {
            MuxEffect::CommandPrompt {
                mode,
                prompt_type,
                no_freeze,
                ..
            } => (mode, prompt_type, no_freeze),
            other => panic!("unexpected effect {other:?}"),
        }
    };

    for (flags, expected) in [
        (
            &["-1", "-N", "-i", "-k", "-e"][..],
            CommandPromptMode::Single,
        ),
        (&["-N", "-i", "-k", "-e"], CommandPromptMode::Numeric),
        (&["-i", "-k", "-e"], CommandPromptMode::Incremental),
        (&["-k", "-e"], CommandPromptMode::Key),
        (&["-e"], CommandPromptMode::BackspaceExit),
        (&[], CommandPromptMode::Text),
    ] {
        let (resolved, prompt_type, no_freeze) = mode(&mut engine, &mut context, flags);
        assert_eq!(resolved, expected, "flags {flags:?}");
        assert_eq!(prompt_type, CommandPromptType::Command, "flags {flags:?}");
        assert!(!no_freeze, "flags {flags:?}");
    }

    assert_eq!(
        mode(&mut engine, &mut context, &["-1", "-C"]),
        (CommandPromptMode::Single, CommandPromptType::Command, true),
    );
    assert_eq!(
        mode(&mut engine, &mut context, &["-T", "search"]),
        (CommandPromptMode::Text, CommandPromptType::Search, false),
    );
    assert_eq!(
        mode(&mut engine, &mut context, &["-T", "command", "-k"]),
        (CommandPromptMode::Key, CommandPromptType::Command, false),
    );
    assert!(matches!(
        engine.execute(&mut context, &command("command-prompt", &["-T", "bogus"])),
        Err(ServerError::InvalidCommand(message)) if message == "unknown type: bogus"
    ));
    assert!(
        engine
            .execute(&mut context, &command("command-prompt", &["-P", "-1"]))
            .expect("-P with no pane is cmd_command_prompt_exec's silent NORMAL")
            .effects
            .is_empty()
    );
    engine
        .execute(
            &mut context,
            &command("new-session", &["-s", "prompt pane"]),
        )
        .expect("a pane for the pane prompt");
    assert_eq!(
        mode(&mut engine, &mut context, &["-P", "-1"]),
        (CommandPromptMode::Single, CommandPromptType::Command, false),
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("command-prompt", &["-t", "%1"]))
            .expect("target client is the daemon's routing, not a mux rejection")
            .effects
            .len(),
        1
    );
}
