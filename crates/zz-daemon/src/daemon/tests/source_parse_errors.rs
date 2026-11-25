use super::*;

#[test]
fn source_file_callback_parse_errors_preserve_replay_groups() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let shared = Arc::new(Shared::new(64));
    let client = ClientId(u64::from(u16::MAX));

    for (request_id, name, first, before, same, next, keeps_same) in [
        (
            1,
            "if-shell.conf",
            "if-shell -F 1 \"{ display-message -p quoted }\"",
            "@callback-if-before",
            "@callback-if-same",
            "@callback-if-next",
            false,
        ),
        (
            2,
            "run-shell.conf",
            "run-shell -C \"{ display-message -p quoted }\"",
            "@callback-run-before",
            "@callback-run-same",
            "@callback-run-next",
            true,
        ),
        (
            3,
            "nested-if-shell.conf",
            "if-shell -F 1 { if-shell -F 1 \"{ display-message -p quoted }\" }",
            "@nested-callback-if-before",
            "@nested-callback-if-same",
            "@nested-callback-if-next",
            true,
        ),
        (
            4,
            "nested-run-shell.conf",
            "run-shell -C { run-shell -C \"{ display-message -p quoted }\" }",
            "@nested-callback-run-before",
            "@nested-callback-run-same",
            "@nested-callback-run-next",
            true,
        ),
    ] {
        let path = directory.path().join(name);
        fs::write(
            &path,
            format!(
                "set-option -g {before} yes ; {first} ; set-option -g {same} yes\n\
                     set-option -g {next} yes\n"
            ),
        )
        .expect("source fixture");
        let path = path.display().to_string();
        let mut context = ExecutionContext::default();

        assert_eq!(
            shared.execute_command_request(
                client,
                ClientKind::Command,
                &mut context,
                request_id,
                &CommandInvocation::new("source-file", [&path]),
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 1,
                stderr: format!("{path}:1: syntax error\n"),
                stdout_claim: StdoutClaim::None,
            }
        );
        assert_eq!(read_global_option(&shared, before), "yes", "{name} prefix");
        assert_eq!(
            read_global_option(&shared, same),
            if keeps_same { "yes" } else { "" },
            "{name} same-line group"
        );
        assert_eq!(read_global_option(&shared, next), "yes", "{name} next line");
    }

    let quoted = directory.path().join("quoted-nested-run-shell.conf");
    fs::write(
        &quoted,
        r#"set-option -g @quoted-outer-before yes ; run-shell -C "set-option -g @quoted-inner-before yes ; run-shell -C \"{ display-message -p quoted }\" ; set-option -g @quoted-inner-same yes" ; set-option -g @quoted-outer-same yes
set-option -g @quoted-next yes
"#,
    )
    .expect("quoted nested run-shell source fixture");
    let quoted = quoted.display().to_string();
    let mut context = ExecutionContext::default();

    assert_eq!(
        shared.execute_command_request(
            client,
            ClientKind::Command,
            &mut context,
            5,
            &CommandInvocation::new("source-file", [&quoted]),
        ),
        CommandResponse::Success {
            request_id: 5,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!("{quoted}:1: syntax error\n"),
            stdout_claim: StdoutClaim::None,
        }
    );
    for name in [
        "@quoted-outer-before",
        "@quoted-inner-before",
        "@quoted-inner-same",
        "@quoted-outer-same",
        "@quoted-next",
    ] {
        assert_eq!(read_global_option(&shared, name), "yes", "{name} state");
    }

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-s", "command-alias[100]", "rr=run-shell -C"],
            ),
        )
        .expect("runtime run-shell alias");
    let aliased = directory.path().join("aliased-nested-run-shell.conf");
    fs::write(
        &aliased,
        r#"run-shell -C "set-option -g @alias-inner-before yes ; rr \"{ display-message -p quoted }\" ; set-option -g @alias-inner-same yes"
set-option -g @alias-next yes
"#,
    )
    .expect("aliased nested run-shell source fixture");
    let aliased = aliased.display().to_string();

    assert_eq!(
        shared.execute_command_request(
            client,
            ClientKind::Command,
            &mut context,
            6,
            &CommandInvocation::new("source-file", [&aliased]),
        ),
        CommandResponse::Success {
            request_id: 6,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!("{aliased}:1: syntax error\n"),
            stdout_claim: StdoutClaim::None,
        }
    );
    for name in ["@alias-inner-before", "@alias-inner-same", "@alias-next"] {
        assert_eq!(read_global_option(&shared, name), "yes", "{name} state");
    }

    for (index, alias) in [
        "aliasif=if-shell -F 1 \"{ display-message -p quoted }\" ; set-option -g @alias-if-same yes",
        "aliasrun=run-shell -C \"{ display-message -p quoted }\" ; set-option -g @alias-run-same yes",
    ]
    .into_iter()
    .enumerate()
    {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    [
                        "-s",
                        &format!("command-alias[{}]", index + 101),
                        alias,
                    ],
                ),
            )
            .expect("callback command alias group");
    }
    for (request_id, name, command, same, next, keeps_same) in [
        (
            7,
            "alias-group-if-shell.conf",
            "aliasif",
            "@alias-if-same",
            "@alias-if-next",
            false,
        ),
        (
            8,
            "alias-group-run-shell.conf",
            "aliasrun",
            "@alias-run-same",
            "@alias-run-next",
            true,
        ),
    ] {
        let path = directory.path().join(name);
        fs::write(&path, format!("{command}\nset-option -g {next} yes\n"))
            .expect("callback alias group source fixture");
        let path = path.display().to_string();

        assert_eq!(
            shared.execute_command_request(
                client,
                ClientKind::Command,
                &mut context,
                request_id,
                &CommandInvocation::new("source-file", [&path]),
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 1,
                stderr: format!("{path}:1: syntax error\n"),
                stdout_claim: StdoutClaim::None,
            }
        );
        assert_eq!(
            read_global_option(&shared, same),
            if keeps_same { "yes" } else { "" },
            "{name} same-group state"
        );
        assert_eq!(read_global_option(&shared, next), "yes", "{name} next line");
    }

    let alias_if_group = directory.path().join("alias-if-caller-group.conf");
    fs::write(
        &alias_if_group,
        "set-option -g @alias-if-caller-before yes ; aliasif ; set-option -g @alias-if-caller-same yes\n\
             set-option -g @alias-if-caller-next yes\n",
    )
    .expect("if-shell alias caller group source fixture");
    let alias_if_group = alias_if_group.display().to_string();
    assert_eq!(
        shared.execute_command_request(
            client,
            ClientKind::Command,
            &mut context,
            9,
            &CommandInvocation::new("source-file", [&alias_if_group]),
        ),
        CommandResponse::Success {
            request_id: 9,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!("{alias_if_group}:1: syntax error\n"),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        read_global_option(&shared, "@alias-if-caller-before"),
        "yes"
    );
    assert_eq!(read_global_option(&shared, "@alias-if-caller-same"), "");
    assert_eq!(read_global_option(&shared, "@alias-if-caller-next"), "yes");

    let alias_run_string = directory.path().join("alias-run-caller-string.conf");
    fs::write(
        &alias_run_string,
        r#"run-shell -C "set-option -g @alias-run-caller-before yes ; aliasrun ; set-option -g @alias-run-caller-same yes"
set-option -g @alias-run-caller-next yes
"#,
    )
    .expect("run-shell alias caller string source fixture");
    let alias_run_string = alias_run_string.display().to_string();
    assert_eq!(
        shared.execute_command_request(
            client,
            ClientKind::Command,
            &mut context,
            10,
            &CommandInvocation::new("source-file", [&alias_run_string]),
        ),
        CommandResponse::Success {
            request_id: 10,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!("{alias_run_string}:1: syntax error\n"),
            stdout_claim: StdoutClaim::None,
        }
    );
    for name in [
        "@alias-run-caller-before",
        "@alias-run-caller-same",
        "@alias-run-caller-next",
    ] {
        assert_eq!(read_global_option(&shared, name), "yes", "{name} state");
    }

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[103]",
                    "aliasmixed=run-shell -C \"{ display-message -p quoted }\" ; if-shell -F 1 \"{ display-message -p quoted }\" ; set-option -g @alias-mixed-body yes",
                ],
            ),
        )
        .expect("mixed callback command alias");
    let alias_mixed = directory.path().join("alias-mixed-caller-string.conf");
    fs::write(
        &alias_mixed,
        r#"run-shell -C "set-option -g @alias-mixed-before yes ; aliasmixed ; set-option -g @alias-mixed-outer yes"
set-option -g @alias-mixed-next yes
"#,
    )
    .expect("mixed callback alias caller string fixture");
    let alias_mixed = alias_mixed.display().to_string();
    let diagnostic = format!("{alias_mixed}:1: syntax error");

    assert_eq!(
        shared.execute_command_request(
            client,
            ClientKind::Command,
            &mut context,
            11,
            &CommandInvocation::new("source-file", [&alias_mixed]),
        ),
        CommandResponse::Success {
            request_id: 11,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!("{diagnostic}\n{diagnostic}\n"),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(read_global_option(&shared, "@alias-mixed-before"), "yes");
    assert_eq!(read_global_option(&shared, "@alias-mixed-body"), "");
    assert_eq!(read_global_option(&shared, "@alias-mixed-outer"), "");
    assert_eq!(read_global_option(&shared, "@alias-mixed-next"), "yes");
}

#[test]
fn source_mixed_depth_alias_callback_failures_prune_caller_group() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let shared = Arc::new(Shared::new(64));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "mixed-depth-alias"]),
        )
        .expect("mixed-depth alias session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[100]",
                    r#"deepmixed=run-shell -C "run-shell -C \"{ display-message -p quoted }\"" ; if-shell -F 1 "{ display-message -p quoted }" ; set-option -g @deep-mixed-alias-body yes"#,
                ],
            ),
        )
        .expect("mixed-depth callback alias");

    let command_path = directory.path().join("command.conf");
    fs::write(
        &command_path,
        "set-option -g @deep-mixed-command-before yes ; deepmixed ; set-option -g @deep-mixed-command-same yes\n\
             set-option -g @deep-mixed-command-later yes\n",
    )
    .expect("mixed-depth Command source fixture");
    let command_path = command_path.display().to_string();
    let diagnostic = format!("{command_path}:1: syntax error");
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            1,
            &CommandInvocation::new("source-file", [&command_path]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!("{diagnostic}\n{diagnostic}\n"),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        read_global_option(&shared, "@deep-mixed-command-before"),
        "yes"
    );
    assert_eq!(read_global_option(&shared, "@deep-mixed-alias-body"), "");
    assert_eq!(read_global_option(&shared, "@deep-mixed-command-same"), "");
    assert_eq!(
        read_global_option(&shared, "@deep-mixed-command-later"),
        "yes"
    );

    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(
            control,
            context.session.expect("mixed-depth alias session id"),
        )
        .expect("attach mixed-depth Control client");
    take_reliable_messages(&mailbox);

    for (direct_argv, request_id) in [(false, 2), (true, 3)] {
        let prefix = if direct_argv { "argv" } else { "stdin" };
        let before = format!("@deep-mixed-{prefix}-before");
        let same = format!("@deep-mixed-{prefix}-same");
        let later = format!("@deep-mixed-{prefix}-later");
        let path = directory.path().join(format!("control-{prefix}.conf"));
        fs::write(
            &path,
            format!(
                "set-option -g {before} yes ; deepmixed ; set-option -g {same} yes\n\
                     set-option -g {later} yes\n"
            ),
        )
        .expect("mixed-depth Control source fixture");
        let invocation = if direct_argv {
            CommandInvocation::new("source-file", [path.display().to_string()])
        } else {
            control_stdin_command("source-file", [path.display().to_string()])
        };
        assert_eq!(
            shared.execute_command_request(
                control,
                ClientKind::Control,
                &mut context,
                request_id,
                &invocation,
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 1,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }
        );
        assert_eq!(
            read_global_option(&shared, &before),
            "yes",
            "{prefix} before"
        );
        assert_eq!(read_global_option(&shared, &same), "", "{prefix} same");
        assert_eq!(read_global_option(&shared, &later), "yes", "{prefix} later");
        take_reliable_messages(&mailbox);
    }

    assert!(
        shared
            .inner
            .lock()
            .clients
            .values()
            .all(|c| c.command_streams.is_none())
    );
}

#[test]
fn source_nested_only_alias_callback_failure_keeps_caller_groups() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let shared = Arc::new(Shared::new(64));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "nested-only-alias"]),
        )
        .expect("nested-only alias session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[100]",
                    r#"nestedonly=if-shell -F 1 { if-shell -F 1 "{ display-message -p quoted }" } ; set-option -g @nested-only-alias-body yes"#,
                ],
            ),
        )
        .expect("nested-only callback alias");

    let command_path = directory.path().join("command.conf");
    fs::write(
        &command_path,
        "set-option -g @nested-only-command-before yes ; nestedonly ; set-option -g @nested-only-command-same yes\n\
             set-option -g @nested-only-command-next yes\n",
    )
    .expect("nested-only Command source fixture");
    let command_path = command_path.display().to_string();
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            1,
            &CommandInvocation::new("source-file", [&command_path]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!("{command_path}:1: syntax error\n"),
            stdout_claim: StdoutClaim::None,
        }
    );
    for name in [
        "@nested-only-command-before",
        "@nested-only-alias-body",
        "@nested-only-command-same",
        "@nested-only-command-next",
    ] {
        assert_eq!(read_global_option(&shared, name), "yes", "{name}");
    }

    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(
            control,
            context.session.expect("nested-only alias session id"),
        )
        .expect("attach nested-only Control client");
    take_reliable_messages(&mailbox);

    for (direct_argv, request_id, flags) in [(false, 2, 1), (true, 3, 0)] {
        let prefix = if direct_argv { "argv" } else { "stdin" };
        let before = format!("@nested-only-{prefix}-before");
        let same = format!("@nested-only-{prefix}-same");
        let next = format!("@nested-only-{prefix}-next");
        let path = directory.path().join(format!("control-{prefix}.conf"));
        fs::write(
            &path,
            format!(
                "set-option -g {before} yes ; nestedonly ; set-option -g {same} yes\n\
                     set-option -g {next} yes\n"
            ),
        )
        .expect("nested-only Control source fixture");
        let invocation = if direct_argv {
            CommandInvocation::new("source-file", [path.display().to_string()])
        } else {
            control_stdin_command("source-file", [path.display().to_string()])
        };
        assert_eq!(
            shared.execute_command_request(
                control,
                ClientKind::Control,
                &mut context,
                request_id,
                &invocation,
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 1,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }
        );
        let diagnostic = format!("{}:1: syntax error", path.display());
        let messages = take_reliable_messages(&mailbox);
        assert_eq!(
            messages
                .iter()
                .filter(|message| matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::ControlCommandGuard { output, .. },
                        ..
                    }) if output == &diagnostic
                ) || matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::ControlCommandOutput { output },
                        ..
                    }) if output == &diagnostic
                ))
                .count(),
            1,
            "{prefix} diagnostic"
        );
        assert!(
            messages
                .iter()
                .filter_map(|message| match message {
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::ControlCommandGuard { flags, .. },
                        ..
                    }) => Some(*flags),
                    _ => None,
                })
                .all(|event_flags| event_flags == flags)
        );
        for name in [&before, "@nested-only-alias-body", &same, &next] {
            assert_eq!(read_global_option(&shared, name), "yes", "{prefix} {name}");
        }
    }

    for (mode, request_id) in [("command", 4), ("stdin", 5), ("argv", 6)] {
        let marker = format!("@nested-string-{mode}-same");
        let body = format!(
            r#"if-shell -F 1 "if-shell -F 1 \"{{ display-message -p quoted }}\"" ; set-option -g {marker} yes"#
        );
        let (request_client, kind, invocation) = match mode {
            "command" => (
                command,
                ClientKind::Command,
                CommandInvocation::new("run-shell", ["-C", &body]),
            ),
            "stdin" => (
                control,
                ClientKind::Control,
                control_stdin_command("run-shell", ["-C", &body]),
            ),
            "argv" => (
                control,
                ClientKind::Control,
                CommandInvocation::new("run-shell", ["-C", &body]),
            ),
            _ => unreachable!(),
        };
        let response = shared.execute_command_request(
            request_client,
            kind,
            &mut context,
            request_id,
            &invocation,
        );
        assert!(matches!(
            response,
            CommandResponse::Error {
                request_id: response_id,
                error,
                output,
            } if response_id == request_id
                && output.is_empty()
                && server_post_admission_callback_parse_depth(&error) == 3
        ));
        assert_eq!(read_global_option(&shared, &marker), "yes", "{mode}");
        if kind == ClientKind::Control {
            assert_eq!(
                take_reliable_messages(&mailbox)
                    .into_iter()
                    .filter(|message| matches!(
                        message,
                        ProtocolMessage::Event(Event {
                            payload: EventPayload::ControlCommandGuard { output, .. },
                            ..
                        }) if output == "syntax error"
                    ) || matches!(
                        message,
                        ProtocolMessage::Event(Event {
                            payload: EventPayload::ControlCommandOutput { output },
                            ..
                        }) if output == "syntax error"
                    ))
                    .count(),
                1,
                "{mode} diagnostic"
            );
        }
    }

    assert!(
        shared
            .inner
            .lock()
            .clients
            .values()
            .all(|c| c.command_streams.is_none())
    );
}

#[test]
fn control_source_callback_parse_errors_preserve_framing() {
    fn timeline(messages: Vec<ProtocolMessage>) -> Vec<String> {
        messages
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload:
                        EventPayload::ControlCommandGuard {
                            output,
                            error,
                            sticky_failure,
                            flags,
                        },
                    ..
                }) => Some(format!("guard:{flags}:{error}:{sticky_failure}:{output}")),
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
            .collect()
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let shared = Arc::new(Shared::new(64));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "callback-framing"]),
        )
        .expect("Control framing session");
    for (index, alias) in [
        "rr=run-shell -C",
        "aliasif=if-shell -F 1 \"{ display-message -p quoted }\" ; display-message -p ALIAS_IF_SAME",
        "aliasrun=run-shell -C \"{ display-message -p quoted }\" ; display-message -p ALIAS_RUN_SAME",
        "aliasmixed=run-shell -C \"{ display-message -p quoted }\" ; if-shell -F 1 \"{ display-message -p quoted }\" ; display-message -p ALIAS_MIXED_BODY",
    ]
    .into_iter()
    .enumerate()
    {
        shared
            .execute(
                command,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    [
                        "-s",
                        &format!("command-alias[{}]", index + 100),
                        alias,
                    ],
                ),
            )
            .expect("Control callback alias");
    }
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(
            control,
            context.session.expect("Control framing session id"),
        )
        .expect("attach Control client");
    take_reliable_messages(&mailbox);

    for (direct_argv, flags) in [(false, 1), (true, 0)] {
        for (request_id, name, first, framing, after) in [
            (
                1_u64,
                "if-shell.conf",
                "if-shell -F 1 \"{ display-message -p quoted }\"",
                "if-shell",
                "AFTER_IF_CALLBACK",
            ),
            (
                2,
                "run-shell.conf",
                "run-shell -C \"{ display-message -p quoted }\"",
                "run-shell",
                "AFTER_RUN_CALLBACK",
            ),
            (
                3,
                "nested-if-shell.conf",
                "if-shell -F 1 { if-shell -F 1 \"{ display-message -p quoted }\" }",
                "nested-if-shell",
                "AFTER_NESTED_IF_CALLBACK",
            ),
            (
                4,
                "nested-run-shell.conf",
                "run-shell -C { run-shell -C \"{ display-message -p quoted }\" }",
                "nested-run-shell",
                "AFTER_NESTED_RUN_CALLBACK",
            ),
            (
                5,
                "multi-run-shell.conf",
                "run-shell -C { run-shell -C \"{ display-message -p quoted }\" ; run-shell -C \"{ display-message -p quoted }\" }",
                "multi-run-shell",
                "AFTER_MULTI_RUN_CALLBACK",
            ),
            (
                6,
                "aliased-run-shell.conf",
                "run-shell -C \"display-message -p ALIAS_INNER_BEFORE ; rr \\\"{ display-message -p quoted }\\\" ; display-message -p ALIAS_INNER_SAME\"",
                "aliased-run-shell",
                "AFTER_ALIASED_RUN_CALLBACK",
            ),
            (
                7,
                "alias-group-if-shell.conf",
                "aliasif",
                "alias-group-if-shell",
                "AFTER_ALIAS_GROUP_IF_CALLBACK",
            ),
            (
                8,
                "alias-group-run-shell.conf",
                "aliasrun",
                "alias-group-run-shell",
                "AFTER_ALIAS_GROUP_RUN_CALLBACK",
            ),
        ] {
            let path = directory.path().join(format!(
                "{}-{name}",
                if direct_argv { "argv" } else { "stdin" }
            ));
            fs::write(&path, format!("{first}\ndisplay-message -p {after}\n"))
                .expect("Control source fixture");
            let diagnostic = format!("{}:1: syntax error", path.display());
            let command = if direct_argv {
                CommandInvocation::new("source-file", [path.display().to_string()])
            } else {
                control_stdin_command("source-file", [path.display().to_string()])
            };
            let request_id = request_id + if direct_argv { 10 } else { 0 };

            assert_eq!(
                shared.execute_command_request(
                    control,
                    ClientKind::Control,
                    &mut context,
                    request_id,
                    &command,
                ),
                CommandResponse::Success {
                    request_id,
                    output: RawText::default(),
                    exit_code: 1,
                    stderr: String::new(),
                    stdout_claim: StdoutClaim::None,
                }
            );
            let timeline = timeline(take_reliable_messages(&mailbox));
            let expected = match framing {
                "if-shell" | "alias-group-if-shell" => vec![
                    format!("guard:{flags}:true:false:{diagnostic}"),
                    format!("guard:{flags}:false:false:{after}"),
                    "complete".to_owned(),
                ],
                "run-shell" => vec![
                    format!("guard:{flags}:false:false:"),
                    format!("raw:{diagnostic}"),
                    format!("guard:{flags}:false:false:{after}"),
                    "complete".to_owned(),
                ],
                "nested-if-shell" => vec![
                    format!("guard:{flags}:false:false:"),
                    format!("guard:{flags}:true:false:{diagnostic}"),
                    format!("guard:{flags}:false:false:{after}"),
                    "complete".to_owned(),
                ],
                "nested-run-shell" => vec![
                    format!("guard:{flags}:false:false:"),
                    format!("guard:{flags}:false:false:"),
                    format!("raw:{diagnostic}"),
                    format!("guard:{flags}:false:false:{after}"),
                    "complete".to_owned(),
                ],
                "multi-run-shell" => vec![
                    format!("guard:{flags}:false:false:"),
                    format!("guard:{flags}:false:false:"),
                    format!("raw:{diagnostic}"),
                    format!("guard:{flags}:false:false:"),
                    format!("raw:{diagnostic}"),
                    format!("guard:{flags}:false:false:{after}"),
                    "complete".to_owned(),
                ],
                "aliased-run-shell" => vec![
                    format!("guard:{flags}:false:false:"),
                    format!("guard:{flags}:false:false:ALIAS_INNER_BEFORE"),
                    format!("guard:{flags}:false:false:"),
                    format!("raw:{diagnostic}"),
                    format!("guard:{flags}:false:false:ALIAS_INNER_SAME"),
                    format!("guard:{flags}:false:false:{after}"),
                    "complete".to_owned(),
                ],
                "alias-group-run-shell" => vec![
                    format!("guard:{flags}:false:false:"),
                    format!("raw:{diagnostic}"),
                    format!("guard:{flags}:false:false:ALIAS_RUN_SAME"),
                    format!("guard:{flags}:false:false:{after}"),
                    "complete".to_owned(),
                ],
                _ => unreachable!(),
            };
            assert_eq!(timeline, expected);
        }
    }

    for (direct_argv, flags) in [(false, 1), (true, 0)] {
        let prefix = if direct_argv { "argv" } else { "stdin" };
        let inner = directory
            .path()
            .join(format!("{prefix}-abbreviated-inner.conf"));
        fs::write(
            &inner,
            "run-shell -C \"{ display-message -p quoted }\"\n\
                 display-message -p ABBREVIATED_INNER_AFTER\n",
        )
        .expect("abbreviated inner source fixture");
        let outer = directory
            .path()
            .join(format!("{prefix}-abbreviated-outer.conf"));
        fs::write(
            &outer,
            format!(
                "source-f '{}'\ndisplay-message -p ABBREVIATED_OUTER_AFTER\n",
                inner.display()
            ),
        )
        .expect("abbreviated outer source fixture");
        let command = if direct_argv {
            CommandInvocation::new("source-file", [outer.display().to_string()])
        } else {
            control_stdin_command("source-file", [outer.display().to_string()])
        };
        let request_id = if direct_argv { 31 } else { 30 };

        assert_eq!(
            shared.execute_command_request(
                control,
                ClientKind::Control,
                &mut context,
                request_id,
                &command,
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 1,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }
        );
        assert_eq!(
            timeline(take_reliable_messages(&mailbox)),
            [
                format!("guard:{flags}:false:false:"),
                format!("guard:{flags}:false:false:"),
                format!("raw:{}:1: syntax error", inner.display()),
                format!("guard:{flags}:false:false:ABBREVIATED_INNER_AFTER"),
                "complete".to_owned(),
                format!("guard:{flags}:false:false:ABBREVIATED_OUTER_AFTER"),
                "complete".to_owned(),
            ]
        );

        let alias_mixed = directory
            .path()
            .join(format!("{prefix}-alias-mixed-string.conf"));
        fs::write(
            &alias_mixed,
            "run-shell -C \"display-message -p ALIAS_MIXED_BEFORE ; aliasmixed ; display-message -p ALIAS_MIXED_OUTER\"\n\
                 display-message -p ALIAS_MIXED_NEXT\n",
        )
        .expect("Control mixed callback alias caller string fixture");
        let diagnostic = format!("{}:1: syntax error", alias_mixed.display());
        let command = if direct_argv {
            CommandInvocation::new("source-file", [alias_mixed.display().to_string()])
        } else {
            control_stdin_command("source-file", [alias_mixed.display().to_string()])
        };
        let request_id = if direct_argv { 45 } else { 44 };
        assert_eq!(
            shared.execute_command_request(
                control,
                ClientKind::Control,
                &mut context,
                request_id,
                &command,
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 1,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }
        );
        assert_eq!(
            timeline(take_reliable_messages(&mailbox)),
            [
                format!("guard:{flags}:false:false:"),
                format!("guard:{flags}:false:false:ALIAS_MIXED_BEFORE"),
                format!("guard:{flags}:false:false:"),
                format!("raw:{diagnostic}"),
                format!("guard:{flags}:true:false:{diagnostic}"),
                format!("guard:{flags}:false:false:ALIAS_MIXED_NEXT"),
                "complete".to_owned(),
            ]
        );
    }

    for (direct_argv, flags) in [(false, 1), (true, 0)] {
        let prefix = if direct_argv { "argv" } else { "stdin" };
        let alias_if = directory
            .path()
            .join(format!("{prefix}-alias-if-group.conf"));
        fs::write(
            &alias_if,
            "display-message -p ALIAS_IF_CALLER_BEFORE ; aliasif ; display-message -p ALIAS_IF_CALLER_SAME\n\
                 display-message -p ALIAS_IF_CALLER_NEXT\n",
        )
        .expect("Control if-shell alias caller group fixture");
        let diagnostic = format!("{}:1: syntax error", alias_if.display());
        let command = if direct_argv {
            CommandInvocation::new("source-file", [alias_if.display().to_string()])
        } else {
            control_stdin_command("source-file", [alias_if.display().to_string()])
        };
        let request_id = if direct_argv { 41 } else { 40 };
        assert_eq!(
            shared.execute_command_request(
                control,
                ClientKind::Control,
                &mut context,
                request_id,
                &command,
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 1,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }
        );
        assert_eq!(
            timeline(take_reliable_messages(&mailbox)),
            [
                format!("guard:{flags}:false:false:ALIAS_IF_CALLER_BEFORE"),
                format!("guard:{flags}:true:false:{diagnostic}"),
                format!("guard:{flags}:false:false:ALIAS_IF_CALLER_NEXT"),
                "complete".to_owned(),
            ]
        );

        let alias_run = directory
            .path()
            .join(format!("{prefix}-alias-run-string.conf"));
        fs::write(
            &alias_run,
            "run-shell -C \"display-message -p ALIAS_RUN_CALLER_BEFORE ; aliasrun ; display-message -p ALIAS_RUN_CALLER_SAME\"\n\
                 display-message -p ALIAS_RUN_CALLER_NEXT\n",
        )
        .expect("Control run-shell alias caller string fixture");
        let diagnostic = format!("{}:1: syntax error", alias_run.display());
        let command = if direct_argv {
            CommandInvocation::new("source-file", [alias_run.display().to_string()])
        } else {
            control_stdin_command("source-file", [alias_run.display().to_string()])
        };
        let request_id = if direct_argv { 43 } else { 42 };
        assert_eq!(
            shared.execute_command_request(
                control,
                ClientKind::Control,
                &mut context,
                request_id,
                &command,
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 1,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }
        );
        assert_eq!(
            timeline(take_reliable_messages(&mailbox)),
            [
                format!("guard:{flags}:false:false:"),
                format!("guard:{flags}:false:false:ALIAS_RUN_CALLER_BEFORE"),
                format!("guard:{flags}:false:false:"),
                format!("raw:{diagnostic}"),
                format!("guard:{flags}:false:false:ALIAS_RUN_SAME"),
                format!("guard:{flags}:false:false:ALIAS_RUN_CALLER_SAME"),
                format!("guard:{flags}:false:false:ALIAS_RUN_CALLER_NEXT"),
                "complete".to_owned(),
            ]
        );
    }

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            3,
            &control_stdin_command("display-message", ["-p", "AFTER_CALLBACK_ERROR"]),
        ),
        CommandResponse::Success {
            request_id: 3,
            output: "AFTER_CALLBACK_ERROR".into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert!(
        shared
            .inner
            .lock()
            .clients
            .values()
            .all(|c| c.command_streams.is_none())
    );
}

#[test]
fn control_source_run_shell_command_parse_failures_use_source_owner() {
    fn timeline(messages: Vec<ProtocolMessage>) -> Vec<String> {
        messages
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload:
                        EventPayload::ControlCommandGuard {
                            output,
                            error,
                            sticky_failure,
                            flags,
                        },
                    ..
                }) => Some(format!("guard:{flags}:{error}:{sticky_failure}:{output}")),
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
            .collect()
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let shared = Arc::new(Shared::new(64));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "callback-command-parse"]),
        )
        .expect("Control callback command parse session");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(
            control,
            context
                .session
                .expect("Control callback command parse session id"),
        )
        .expect("attach callback command parse Control client");
    take_reliable_messages(&mailbox);

    let mut request_id = 0;
    for (direct_argv, flags) in [(false, 1), (true, 0)] {
        for (name, callback, message, marker) in [
            (
                "unknown",
                "wibble",
                "unknown command: wibble",
                "AFTER_UNKNOWN_CALLBACK",
            ),
            (
                "set-option-arity",
                "set-option -g",
                "command set-option: too few arguments (need at least 1)",
                "AFTER_SET_OPTION_ARITY",
            ),
            (
                "set-environment-arity",
                "set-environment -g",
                "command set-environment: too few arguments (need at least 1)",
                "AFTER_SET_ENVIRONMENT_ARITY",
            ),
        ] {
            request_id += 1;
            let path = directory.path().join(format!(
                "{}-{name}.conf",
                if direct_argv { "argv" } else { "stdin" }
            ));
            fs::write(
                &path,
                format!("run-shell -C \"{callback}\"\ndisplay-message -p {marker}\n"),
            )
            .expect("Control callback command parse fixture");
            let command = if direct_argv {
                CommandInvocation::new("source-file", [path.display().to_string()])
            } else {
                control_stdin_command("source-file", [path.display().to_string()])
            };

            assert_eq!(
                shared.execute_command_request(
                    control,
                    ClientKind::Control,
                    &mut context,
                    request_id,
                    &command,
                ),
                CommandResponse::Success {
                    request_id,
                    output: RawText::default(),
                    exit_code: 1,
                    stderr: String::new(),
                    stdout_claim: StdoutClaim::None,
                }
            );
            assert_eq!(
                timeline(take_reliable_messages(&mailbox)),
                [
                    format!("guard:{flags}:false:false:"),
                    format!("raw:{}:1: {message}", path.display()),
                    format!("guard:{flags}:false:false:{marker}"),
                    "complete".to_owned(),
                ]
            );
        }
    }
}

#[test]
fn sourced_direct_callback_command_parse_hooks_match_wrapper_ownership() {
    fn timeline(messages: Vec<ProtocolMessage>) -> Vec<String> {
        messages
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload:
                        EventPayload::ControlCommandGuard {
                            output,
                            error,
                            sticky_failure,
                            flags,
                        },
                    ..
                }) => Some(format!("guard:{flags}:{error}:{sticky_failure}:{output}")),
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
            .collect()
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let run = directory.path().join("run-shell.conf");
    fs::write(
        &run,
        "run-shell -C \"wibble\"\ndisplay-message -p RUN_LATER\n",
    )
    .expect("run-shell callback hook fixture");
    let condition = directory.path().join("if-shell.conf");
    fs::write(
        &condition,
        "if-shell -F 1 \"wibble\"\ndisplay-message -p IF_LATER\n",
    )
    .expect("if-shell callback hook fixture");

    let shared = Arc::new(Shared::new(64));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "callback-hook-owner", QUIET_PANE_COMMAND],
            ),
        )
        .expect("callback hook owner session");
    wait_for_quiet_panes(&shared, &[context.pane.expect("callback hook owner pane")]);
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "command-error",
                    "set-option -ga @callback-hook-hits x",
                ],
            ),
        )
        .expect("callback command-error hook");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(
            control,
            context.session.expect("callback hook owner session id"),
        )
        .expect("attach callback hook owner Control client");
    take_reliable_messages(&mailbox);

    let mut request_id = 0;
    for (wrapper, path, marker, expected_hits) in [
        ("run-shell", &run, "RUN_LATER", ""),
        ("if-shell", &condition, "IF_LATER", "x"),
    ] {
        let diagnostic = format!("{}:1: unknown command: wibble", path.display());
        for (mode, flags) in [("command", None), ("argv", Some(0)), ("stdin", Some(1))] {
            shared
                .execute(
                    command,
                    ClientKind::Command,
                    &mut context,
                    &CommandInvocation::new("set-option", ["-gu", "@callback-hook-hits"]),
                )
                .expect("reset callback hook hits");
            request_id += 1;
            if let Some(flags) = flags {
                let invocation = if mode == "argv" {
                    CommandInvocation::new("source-file", [path.display().to_string()])
                } else {
                    control_stdin_command("source-file", [path.display().to_string()])
                };
                assert_eq!(
                    shared.execute_command_request(
                        control,
                        ClientKind::Control,
                        &mut context,
                        request_id,
                        &invocation,
                    ),
                    CommandResponse::Success {
                        request_id,
                        output: RawText::default(),
                        exit_code: 1,
                        stderr: String::new(),
                        stdout_claim: StdoutClaim::None,
                    },
                    "{wrapper} {mode}",
                );
                let expected = if wrapper == "run-shell" {
                    vec![
                        format!("guard:{flags}:false:false:"),
                        format!("raw:{diagnostic}"),
                        format!("guard:{flags}:false:false:{marker}"),
                        "complete".to_owned(),
                    ]
                } else {
                    vec![
                        format!("guard:{flags}:true:false:{diagnostic}"),
                        "guard:0:false:false:".to_owned(),
                        format!("guard:{flags}:false:false:{marker}"),
                        "complete".to_owned(),
                    ]
                };
                assert_eq!(
                    timeline(take_reliable_messages(&mailbox)),
                    expected,
                    "{wrapper} {mode}",
                );
            } else {
                assert_eq!(
                    shared.execute_command_request(
                        command,
                        ClientKind::Command,
                        &mut context,
                        request_id,
                        &CommandInvocation::new("source-file", [path.display().to_string()],),
                    ),
                    CommandResponse::Success {
                        request_id,
                        output: format!("{marker}\n").into(),
                        exit_code: 1,
                        stderr: format!("{diagnostic}\n"),
                        stdout_claim: StdoutClaim::Print,
                    },
                    "{wrapper} {mode}",
                );
            }
            assert_eq!(
                read_global_option(&shared, "@callback-hook-hits"),
                expected_hits,
                "{wrapper} {mode}",
            );
        }
    }
    assert!(
        shared
            .inner
            .lock()
            .clients
            .values()
            .all(|c| c.command_streams.is_none())
    );
}

#[test]
fn sourced_prepared_runtime_command_parse_is_unlocated_for_direct_and_alias_commands() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let direct = directory.path().join("direct-runtime-parse.conf");
    fs::write(
        &direct,
        "display-message -F MATCH one\ndisplay-message -p DIRECT_LATER\n",
    )
    .expect("direct runtime parse source fixture");
    let alias = directory.path().join("alias-runtime-parse.conf");
    fs::write(
        &alias,
        "directruntimealias\ndisplay-message -p ALIAS_LATER\n",
    )
    .expect("alias runtime parse source fixture");

    let shared = Arc::new(Shared::new(64));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "prepared-runtime-parse"]),
        )
        .expect("prepared runtime parse session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[120]",
                    "directruntimealias=display-message -F MATCH one",
                ],
            ),
        )
        .expect("direct runtime parse alias");

    let diagnostic = "only one of -F or argument must be given";
    for (request_id, path, marker) in [(1, &direct, "DIRECT_LATER"), (2, &alias, "ALIAS_LATER")] {
        assert_eq!(
            shared.execute_command_request(
                command,
                ClientKind::Command,
                &mut context,
                request_id,
                &CommandInvocation::new("source-file", [path.display().to_string()]),
            ),
            CommandResponse::Success {
                request_id,
                output: format!("{marker}\n").into(),
                exit_code: 1,
                stderr: format!("{diagnostic}\n"),
                stdout_claim: StdoutClaim::Print,
            }
        );
    }
    assert!(
        shared
            .inner
            .lock()
            .clients
            .values()
            .all(|c| c.command_streams.is_none())
    );
}

#[test]
fn sourced_runtime_command_parse_preserves_control_frames_hook_and_continuation() {
    fn timeline(messages: Vec<ProtocolMessage>) -> Vec<String> {
        messages
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload:
                        EventPayload::ControlCommandGuard {
                            output,
                            error,
                            sticky_failure,
                            flags,
                        },
                    ..
                }) => Some(format!("guard:{flags}:{error}:{sticky_failure}:{output}")),
                ProtocolMessage::Event(Event {
                    payload:
                        EventPayload::ControlSourceFile {
                            event: ControlSourceFileEvent::Complete,
                        },
                    ..
                }) => Some("complete".to_owned()),
                _ => None,
            })
            .collect()
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("runtime-command-parse.conf");
    fs::write(
        &source,
        "run-shell -C \"display-message -F MATCH one\"\ndisplay-message -p LATER\n",
    )
    .expect("runtime command parse source fixture");
    let diagnostic = "only one of -F or argument must be given";
    let shared = Arc::new(Shared::new(64));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "runtime-command-parse"]),
        )
        .expect("runtime command parse session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "command-error",
                    "set-option -ga @runtime-command-parse-hook x",
                ],
            ),
        )
        .expect("runtime command parse hook");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(
            control,
            context.session.expect("runtime command parse session id"),
        )
        .expect("attach runtime command parse Control client");
    take_reliable_messages(&mailbox);

    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            1,
            &CommandInvocation::new("source-file", [source.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: "LATER\n".into(),
            exit_code: 1,
            stderr: format!("{diagnostic}\n"),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        read_global_option(&shared, "@runtime-command-parse-hook"),
        "x"
    );
    take_reliable_messages(&mailbox);

    for (request_id, invocation, flags) in [
        (
            2,
            CommandInvocation::new("source-file", [source.display().to_string()]),
            CONTROL_COMMAND_FRAME_FLAGS_NONE,
        ),
        (
            3,
            control_stdin_command("source-file", [source.display().to_string()]),
            CONTROL_COMMAND_FRAME_FLAGS_CONTROL,
        ),
    ] {
        shared
            .execute(
                command,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-option", ["-gu", "@runtime-command-parse-hook"]),
            )
            .expect("reset runtime command parse hook");
        take_reliable_messages(&mailbox);
        assert_eq!(
            shared.execute_command_request(
                control,
                ClientKind::Control,
                &mut context,
                request_id,
                &invocation,
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 1,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }
        );
        assert_eq!(
            timeline(take_reliable_messages(&mailbox)),
            [
                format!("guard:{flags}:false:false:"),
                format!("guard:{flags}:true:false:{diagnostic}"),
                "guard:0:false:false:".to_owned(),
                format!("guard:{flags}:false:false:LATER"),
                "complete".to_owned(),
            ]
        );
        assert_eq!(
            read_global_option(&shared, "@runtime-command-parse-hook"),
            "x"
        );
    }

    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                ["-g", "command-error", "display-message -F HOOK one"],
            ),
        )
        .expect("failing runtime command parse hook");
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            4,
            &CommandInvocation::new("source-file", [source.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 4,
            output: "LATER\n".into(),
            exit_code: 1,
            stderr: format!("{diagnostic}\n{diagnostic}\n"),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert!(
        shared
            .inner
            .lock()
            .clients
            .values()
            .all(|c| c.command_streams.is_none())
    );
}

#[test]
fn unregistered_control_argv_source_callback_error_returns_failure() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("control-argv-callback.conf");
    fs::write(&source, "if-shell -F 1 \"{ display-message -p quoted }\"\n")
        .expect("Control argv callback source fixture");
    let shared = Arc::new(Shared::new(64));
    let control = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            1,
            &CommandInvocation::new("source-file", [source.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    let inner = shared.inner.lock();
    assert!(inner.client(control).is_none_or(|c| c.kind.is_none()));
    assert!(inner.clients.values().all(|c| c.command_streams.is_none()));
}

#[cfg(unix)]
#[test]
fn nested_callback_diagnostic_does_not_rewrite_matching_shell_output() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("matching-shell-output.conf");
    fs::write(
        &source,
        "run-shell -C { run-shell -C \"{ display-message -p quoted }\" ; run-shell \"printf 'syntax error'\" }\n\
             display-message -p AFTER_MATCHING_OUTPUT\n",
    )
    .expect("matching shell output source fixture");
    let shared = Arc::new(Shared::new(64));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "matching-output"]),
        )
        .expect("matching output session");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(
            control,
            context.session.expect("matching output session id"),
        )
        .expect("attach matching output Control client");
    take_reliable_messages(&mailbox);

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            1,
            &control_stdin_command("source-file", [source.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
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
            "guard:1:false:".to_owned(),
            "guard:1:false:".to_owned(),
            format!("raw:{}:1: syntax error", source.display()),
            "guard:1:false:".to_owned(),
            "raw:syntax error".to_owned(),
            "guard:1:false:AFTER_MATCHING_OUTPUT".to_owned(),
            "complete".to_owned(),
        ]
    );
}

#[test]
fn interactive_source_callback_parse_errors_are_warned_once_each() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("interactive-callback.conf");
    fs::write(
        &source,
        "if-shell -F 1 \"{ display-message -p quoted }\"\n\
             run-shell -C \"{ display-message -p quoted }\"\n",
    )
    .expect("interactive callback source fixture");
    let shared = Arc::new(Shared::new(64));
    let mailbox = OutboundMailbox::new();
    let (interactive, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);
    let mut context = ExecutionContext::default();

    assert_eq!(
        shared.execute_command_request(
            interactive,
            ClientKind::Interactive,
            &mut context,
            1,
            &CommandInvocation::new("source-file", [source.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    let warnings = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ClientMessage {
                        kind: ClientMessageKind::Warning,
                        text,
                        ..
                    },
                ..
            }) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        warnings,
        [
            format!("{}:1: syntax error", source.display()),
            format!("{}:2: syntax error", source.display()),
        ]
    );
}

#[test]
fn source_file_retains_every_continuing_callback_parse_error() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("multiple-callback-errors.conf");
    fs::write(
        &source,
        "run-shell -C { run-shell -C \"{ display-message -p quoted }\" ; run-shell -C \"{ display-message -p quoted }\" }\n\
             set-option -g @after-multiple-callback-errors yes\n",
    )
    .expect("multiple callback error source fixture");
    let shared = Arc::new(Shared::new(64));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    let diagnostic = format!("{}:1: syntax error", source.display());

    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            1,
            &CommandInvocation::new("source-file", [source.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!("{diagnostic}\n{diagnostic}\n"),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        read_global_option(&shared, "@after-multiple-callback-errors"),
        "yes"
    );

    let mailbox = OutboundMailbox::new();
    let (interactive, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);

    assert_eq!(
        shared.execute_command_request(
            interactive,
            ClientKind::Interactive,
            &mut context,
            2,
            &CommandInvocation::new("source-file", [source.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 2,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    let warnings = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ClientMessage {
                        kind: ClientMessageKind::Warning,
                        text,
                        ..
                    },
                ..
            }) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(warnings, [diagnostic.clone(), diagnostic]);
}
