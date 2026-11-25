use super::*;

#[test]
fn source_file_hook_callback_failures_keep_identity_output_and_status() {
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
    let combined = directory.path().join("hook-and-source-callback.conf");
    fs::write(
        &combined,
        r#"run-shell -C "display-message -p TRIGGER ; run-shell -C \"{ display-message -p quoted }\""
"#,
    )
    .expect("combined hook and source callback fixture");
    let hook_only = directory.path().join("hook-callback-only.conf");
    fs::write(
        &hook_only,
        "display-message -p TRIGGER\ndisplay-message -p AFTER\n",
    )
    .expect("hook callback only fixture");

    let shared = Arc::new(Shared::new(64));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "hook-callback-status"]),
        )
        .expect("hook callback session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "after-display-message",
                    "run-shell -C \"{ display-message -p quoted }\"",
                ],
            ),
        )
        .expect("callback parse hook");

    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            1,
            &CommandInvocation::new("source-file", [combined.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: "TRIGGER\n".into(),
            exit_code: 1,
            stderr: format!("syntax error\n{}:1: syntax error\n", combined.display()),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            2,
            &CommandInvocation::new("source-file", [hook_only.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 2,
            output: "TRIGGER\nAFTER\n".into(),
            exit_code: 1,
            stderr: "syntax error\nsyntax error\n".to_owned(),
            stdout_claim: StdoutClaim::Print,
        }
    );

    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(control, context.session.expect("hook callback session id"))
        .expect("attach Control client");
    take_reliable_messages(&mailbox);

    for (direct_argv, flags) in [(false, 1), (true, 0)] {
        let command = if direct_argv {
            CommandInvocation::new("source-file", [combined.display().to_string()])
        } else {
            control_stdin_command("source-file", [combined.display().to_string()])
        };
        let request_id = if direct_argv { 4 } else { 3 };
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
                format!("guard:{flags}:false:false:TRIGGER"),
                "guard:0:false:false:".to_owned(),
                "raw:syntax error".to_owned(),
                format!("guard:{flags}:false:false:"),
                format!("raw:{}:1: syntax error", combined.display()),
                "complete".to_owned(),
            ]
        );
    }

    for (direct_argv, flags) in [(false, 1), (true, 0)] {
        let command = if direct_argv {
            CommandInvocation::new("source-file", [hook_only.display().to_string()])
        } else {
            control_stdin_command("source-file", [hook_only.display().to_string()])
        };
        let request_id = if direct_argv { 6 } else { 5 };
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
                format!("guard:{flags}:false:false:TRIGGER"),
                "guard:0:false:false:".to_owned(),
                "raw:syntax error".to_owned(),
                format!("guard:{flags}:false:false:AFTER"),
                "guard:0:false:false:".to_owned(),
                "raw:syntax error".to_owned(),
                "complete".to_owned(),
            ]
        );
    }

    let hook_source_child = directory.path().join("hook-source-child.conf");
    fs::write(
        &hook_source_child,
        "run-shell -C \"{ display-message -p quoted }\"\n",
    )
    .expect("hook source child fixture");
    let hook_source_outer = directory.path().join("hook-source-outer.conf");
    fs::write(
        &hook_source_outer,
        "display-message -p SOURCE_HOOK_TRIGGER\n",
    )
    .expect("hook source outer fixture");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g".to_owned(),
                    "after-display-message".to_owned(),
                    format!("source-file '{}'", hook_source_child.display()),
                ],
            ),
        )
        .expect("nested source hook");

    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            7,
            &CommandInvocation::new("source-file", [hook_source_outer.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 7,
            output: "SOURCE_HOOK_TRIGGER\n".into(),
            exit_code: 1,
            stderr: format!("{}:1: syntax error\n", hook_source_child.display()),
            stdout_claim: StdoutClaim::Print,
        }
    );

    for (direct_argv, flags) in [(false, 1), (true, 0)] {
        let command = if direct_argv {
            CommandInvocation::new("source-file", [hook_source_outer.display().to_string()])
        } else {
            control_stdin_command("source-file", [hook_source_outer.display().to_string()])
        };
        let request_id = if direct_argv { 9 } else { 8 };
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
                format!("guard:{flags}:false:false:SOURCE_HOOK_TRIGGER"),
                "guard:0:false:false:".to_owned(),
                "guard:0:false:false:".to_owned(),
                format!("raw:{}:1: syntax error", hook_source_child.display()),
                "complete".to_owned(),
                "complete".to_owned(),
            ]
        );
    }

    let hook_group = directory.path().join("hook-callback-group.conf");
    fs::write(&hook_group, "display-message -p HOOK_GROUP_TRIGGER\n")
        .expect("hook callback group fixture");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "after-display-message",
                    "run-shell -C \"{ display-message -p quoted }\" ; set-option -g @hook-after yes",
                ],
            ),
        )
        .expect("continuing callback hook group");

    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            10,
            &CommandInvocation::new("source-file", [hook_group.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 10,
            output: "HOOK_GROUP_TRIGGER\n".into(),
            exit_code: 1,
            stderr: "syntax error\n".to_owned(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(read_global_option(&shared, "@hook-after"), "yes");

    for (direct_argv, flags) in [(false, 1), (true, 0)] {
        shared
            .execute(
                command,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-option", ["-gu", "@hook-after"]),
            )
            .expect("clear hook continuation marker");
        let command = if direct_argv {
            CommandInvocation::new("source-file", [hook_group.display().to_string()])
        } else {
            control_stdin_command("source-file", [hook_group.display().to_string()])
        };
        let request_id = if direct_argv { 12 } else { 11 };
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
                format!("guard:{flags}:false:false:HOOK_GROUP_TRIGGER"),
                "guard:0:false:false:".to_owned(),
                "raw:syntax error".to_owned(),
                "guard:0:false:false:".to_owned(),
                "complete".to_owned(),
            ]
        );
        assert_eq!(read_global_option(&shared, "@hook-after"), "yes");
    }

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
                    "set-option -g @wrapper-command-error yes",
                ],
            ),
        )
        .expect("wrapper command-error marker hook");
    let hook_child = directory.path().join("hook-continuation-child.conf");
    fs::write(&hook_child, "display-message -p HOOK_CONTINUATION_CHILD\n")
        .expect("hook continuation child fixture");
    let hook_outer = directory.path().join("hook-continuation-outer.conf");
    fs::write(
        &hook_outer,
        format!(
            "run-shell -C \"set-option -g @hook-continuation-before yes ; source-file '{}' ; set-option -g @hook-continuation-after yes\"\n\
                 set-option -g @hook-continuation-later yes\n",
            tmux_path(&hook_child)
        ),
    )
    .expect("hook continuation outer fixture");
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            15,
            &CommandInvocation::new("source-file", [hook_outer.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 15,
            output: "HOOK_CONTINUATION_CHILD\n".into(),
            exit_code: 1,
            stderr: "syntax error\n".to_owned(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    for marker in [
        "@hook-continuation-before",
        "@hook-continuation-after",
        "@hook-continuation-later",
    ] {
        assert_eq!(read_global_option(&shared, marker), "yes", "{marker}");
    }
    assert_eq!(read_global_option(&shared, "@wrapper-command-error"), "");

    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-hook", ["-gu", "after-display-message"]),
        )
        .expect("clear display hook");
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
                    "run-shell -C \"{ display-message -p quoted }\"",
                ],
            ),
        )
        .expect("callback command-error hook");
    let runtime_error = directory.path().join("command-error-hook-runtime.conf");
    fs::write(
        &runtime_error,
        "kill-session -t missing-indirect\ndisplay-message -p AFTER_RUNTIME_ERROR\n",
    )
    .expect("runtime command-error hook fixture");
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            13,
            &CommandInvocation::new("source-file", [runtime_error.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 13,
            output: "AFTER_RUNTIME_ERROR\n".into(),
            exit_code: 1,
            stderr: "can't find session: missing-indirect\nsyntax error\n".to_owned(),
            stdout_claim: StdoutClaim::Print,
        }
    );

    let callback_error = directory.path().join("command-error-hook-callback.conf");
    fs::write(
        &callback_error,
        "if-shell -F 1 \"{ display-message -p quoted }\"\n",
    )
    .expect("callback command-error hook fixture");
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            14,
            &CommandInvocation::new("source-file", [callback_error.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 14,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!(
                "{}:1: syntax error\nsyntax error\n",
                callback_error.display()
            ),
            stdout_claim: StdoutClaim::None,
        }
    );

    let command_error_child = directory.path().join("command-error-hook-child.conf");
    fs::write(
        &command_error_child,
        "run-shell -C \"{ display-message -p quoted }\"\n",
    )
    .expect("command-error hook child fixture");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g".to_owned(),
                    "command-error".to_owned(),
                    format!("source-file '{}'", command_error_child.display()),
                ],
            ),
        )
        .expect("sourced callback command-error hook");
    let nested_hook_error = directory.path().join("command-error-hook-outer.conf");
    fs::write(
        &nested_hook_error,
        "if-shell -F 1 \"{ display-message -p quoted }\"\n",
    )
    .expect("nested command-error hook fixture");
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            16,
            &CommandInvocation::new("source-file", [nested_hook_error.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 16,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!(
                "{}:1: syntax error\n{}:1: syntax error\n",
                nested_hook_error.display(),
                command_error_child.display()
            ),
            stdout_claim: StdoutClaim::None,
        }
    );

    for (request_id, hook, stderr) in [
        (
            17,
            format!(
                "source-file '{}' ; run-shell -C \"{{ display-message -p quoted }}\"",
                command_error_child.display()
            ),
            format!(
                "{}:1: syntax error\n{}:1: syntax error\nsyntax error\n",
                nested_hook_error.display(),
                command_error_child.display()
            ),
        ),
        (
            18,
            format!(
                "run-shell -C \"{{ display-message -p quoted }}\" ; source-file '{}'",
                command_error_child.display()
            ),
            format!(
                "{}:1: syntax error\nsyntax error\n{}:1: syntax error\n",
                nested_hook_error.display(),
                command_error_child.display()
            ),
        ),
    ] {
        shared
            .execute(
                command,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-hook", ["-g", "command-error", &hook]),
            )
            .expect("ordered command-error hook");
        assert_eq!(
            shared.execute_command_request(
                command,
                ClientKind::Command,
                &mut context,
                request_id,
                &CommandInvocation::new("source-file", [nested_hook_error.display().to_string()],),
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 1,
                stderr,
                stdout_claim: StdoutClaim::None,
            }
        );
    }
}

#[test]
fn control_argv_source_status_and_frames_compose_through_groups() {
    fn timeline(messages: Vec<ProtocolMessage>) -> Vec<String> {
        messages
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlCommandGuard { output, .. },
                    ..
                }) => Some(format!("guard:{output}")),
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
    let child = directory.path().join("control-wrapper-child.conf");
    fs::write(
        &child,
        "run-shell -C \"{ display-message -p quoted }\"\n\
             display-message -p CHILD_LATER\n",
    )
    .expect("Control wrapper child fixture");
    let child = child.display().to_string();
    let body = format!(
        "display-message -p OUTER_BEFORE ; source-file '{child}' ; display-message -p OUTER_AFTER"
    );
    let runtime_child = directory.path().join("control-wrapper-runtime.conf");
    fs::write(
        &runtime_child,
        "kill-session -t missing-control-runtime\n\
             display-message -p RUNTIME_CHILD_LATER\n",
    )
    .expect("Control wrapper runtime fixture");
    let runtime_child = runtime_child.display().to_string();
    let runtime_body = format!(
        "display-message -p RUNTIME_OUTER_BEFORE ; source-file '{runtime_child}' ; display-message -p RUNTIME_OUTER_AFTER"
    );

    let shared = Arc::new(Shared::new(64));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "control-wrapper-status"]),
        )
        .expect("Control wrapper session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[110]".to_owned(),
                    format!("controlwrap={body}"),
                ],
            ),
        )
        .expect("Control wrapper alias");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[111]".to_owned(),
                    format!("controlruntime={runtime_body}"),
                ],
            ),
        )
        .expect("Control wrapper runtime alias");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(
            control,
            context.session.expect("Control wrapper session id"),
        )
        .expect("attach Control wrapper client");
    take_reliable_messages(&mailbox);

    for (request_id, invocation) in [
        (
            1,
            CommandInvocation::new("run-shell", ["-C".to_owned(), body.clone()]),
        ),
        (
            2,
            CommandInvocation::new("if-shell", ["-F".to_owned(), "1".to_owned(), body.clone()]),
        ),
        (
            3,
            CommandInvocation::new("controlwrap", Vec::<String>::new()),
        ),
    ] {
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
                "guard:OUTER_BEFORE".to_owned(),
                "guard:".to_owned(),
                "guard:".to_owned(),
                format!("raw:{child}:1: syntax error"),
                "guard:CHILD_LATER".to_owned(),
                "complete".to_owned(),
                "guard:OUTER_AFTER".to_owned(),
            ],
            "{}",
            invocation.name,
        );
    }

    let held_run_shell = control_stdin_command("run-shell", ["-C".to_owned(), body.clone()]);
    assert_eq!(
        shared.execute_command_request_with_prepared(
            control,
            ClientKind::Control,
            &mut context,
            20,
            &held_run_shell,
            true,
        ),
        CommandResponse::Success {
            request_id: 20,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    let held_messages = take_reliable_messages(&mailbox);
    assert!(
        held_messages
            .iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlCommandGuard { flags, .. },
                    ..
                }) => Some(*flags),
                _ => None,
            })
            .all(|flags| flags == CONTROL_COMMAND_FRAME_FLAGS_CONTROL)
    );
    assert_eq!(
        held_messages
            .iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload:
                        EventPayload::ControlCommandGuard {
                            output,
                            sticky_failure,
                            ..
                        },
                    ..
                }) => Some((output.as_str(), *sticky_failure)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [
            ("OUTER_BEFORE", false),
            ("", true),
            ("", false),
            ("CHILD_LATER", false),
            ("OUTER_AFTER", false),
        ]
    );
    assert_eq!(
        timeline(held_messages),
        [
            "guard:OUTER_BEFORE".to_owned(),
            "guard:".to_owned(),
            "guard:".to_owned(),
            format!("raw:{child}:1: syntax error"),
            "guard:CHILD_LATER".to_owned(),
            "complete".to_owned(),
            "guard:OUTER_AFTER".to_owned(),
        ]
    );

    for (request_id, invocation) in [
        (
            4,
            CommandInvocation::new("run-shell", ["-C".to_owned(), runtime_body.clone()]),
        ),
        (
            5,
            CommandInvocation::new(
                "if-shell",
                ["-F".to_owned(), "1".to_owned(), runtime_body.clone()],
            ),
        ),
        (
            6,
            CommandInvocation::new("controlruntime", Vec::<String>::new()),
        ),
    ] {
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
                "guard:RUNTIME_OUTER_BEFORE".to_owned(),
                "guard:".to_owned(),
                "guard:can't find session: missing-control-runtime".to_owned(),
                "guard:RUNTIME_CHILD_LATER".to_owned(),
                "complete".to_owned(),
                "guard:RUNTIME_OUTER_AFTER".to_owned(),
            ],
            "{}",
            invocation.name,
        );
    }

    let unreadable = directory.path().join("control-wrapper-unreadable");
    fs::create_dir(&unreadable).expect("Control wrapper unreadable fixture");
    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            7,
            &CommandInvocation::new("source-file", [unreadable.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 7,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    let messages = take_reliable_messages(&mailbox);
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlSourceFile {
                        event: ControlSourceFileEvent::ReadError(_),
                    },
                    ..
                })
            ))
            .count(),
        1
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlSourceFile {
                        event: ControlSourceFileEvent::Complete,
                    },
                    ..
                })
            ))
            .count(),
        1
    );

    let read_body = format!(
        "source-file '{}' ; display-message -p READ_AFTER",
        unreadable.display()
    );
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s".to_owned(),
                    "command-alias[112]".to_owned(),
                    format!("controlread={read_body}"),
                ],
            ),
        )
        .expect("Control wrapper read alias");
    for (request_id, invocation) in [
        (
            8,
            CommandInvocation::new("run-shell", ["-C".to_owned(), read_body.clone()]),
        ),
        (
            9,
            CommandInvocation::new(
                "if-shell",
                ["-F".to_owned(), "1".to_owned(), read_body.clone()],
            ),
        ),
        (
            10,
            CommandInvocation::new("controlread", Vec::<String>::new()),
        ),
    ] {
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
        let messages = take_reliable_messages(&mailbox);
        assert!(
            timeline(messages.clone()).contains(&"guard:READ_AFTER".to_owned()),
            "{}",
            invocation.name
        );
        assert_eq!(
            messages
                .iter()
                .filter(|message| matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::ControlSourceFile {
                            event: ControlSourceFileEvent::ReadError(_),
                        },
                        ..
                    })
                ))
                .count(),
            1,
            "{}",
            invocation.name
        );
    }

    let parser = directory.path().join("control-wrapper-parser.conf");
    fs::write(&parser, "%endif\ndisplay-message -p PARSE_LATER\n").expect("Control parser fixture");
    let parser_diagnostic = format!("{}:1: syntax error", parser.display());
    for (request_id, args, executes) in [
        (11, vec![parser.display().to_string()], false),
        (
            12,
            vec!["-n".to_owned(), parser.display().to_string()],
            false,
        ),
    ] {
        assert_eq!(
            shared.execute_command_request(
                control,
                ClientKind::Control,
                &mut context,
                request_id,
                &CommandInvocation::new("source-file", args),
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 1,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }
        );
        let messages = take_reliable_messages(&mailbox);
        assert_eq!(
            messages
                .iter()
                .filter_map(|message| match message {
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::ControlConfigError { text },
                        ..
                    }) => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [parser_diagnostic.as_str()]
        );
        assert_eq!(
            timeline(messages).contains(&"guard:PARSE_LATER".to_owned()),
            executes
        );
    }
}

#[test]
fn source_file_replayed_runtime_errors_reach_each_client_channel() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let runtime = directory.path().join("runtime-errors.conf");
    fs::write(
        &runtime,
        "kill-session -t missing-runtime\n\
             set-option -g nonexistent-option value\n\
             set-environment -g \"\" value\n\
             set-option -g @runtime-after yes\n",
    )
    .expect("runtime error source");
    let inner = directory.path().join("inner.conf");
    fs::write(
        &inner,
        "kill-session -t missing-inner\nset-option -g @inner-after yes\n",
    )
    .expect("inner runtime error source");
    let outer = directory.path().join("outer.conf");
    fs::write(
        &outer,
        format!(
            "source-file '{}'\nset-option -g @outer-after yes\n",
            inner.display()
        ),
    )
    .expect("outer runtime error source");

    let shared = Arc::new(Shared::new(63));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "replay-runtime-errors"]),
        )
        .expect("runtime error session");

    let command_response = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        1,
        &CommandInvocation::new("source-file", [runtime.display().to_string()]),
    );
    assert_eq!(
        command_response,
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 1,
            stderr: "can't find session: missing-runtime\n\
                         invalid option: nonexistent-option\n\
                         empty variable name\n"
                .to_owned(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(read_global_option(&shared, "@runtime-after"), "yes");

    let outer_response = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        2,
        &CommandInvocation::new("source-file", [outer.display().to_string()]),
    );
    assert_eq!(
        outer_response,
        CommandResponse::Success {
            request_id: 2,
            output: RawText::default(),
            exit_code: 1,
            stderr: "can't find session: missing-inner\n".to_owned(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(read_global_option(&shared, "@inner-after"), "yes");
    assert_eq!(read_global_option(&shared, "@outer-after"), "yes");

    let interactive_mailbox = OutboundMailbox::new();
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&interactive_mailbox),
    );
    let mut interactive_context = ExecutionContext::default();
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut interactive_context,
            &CommandInvocation::new("attach-session", ["-t", "replay-runtime-errors"]),
        )
        .expect("attach interactive runtime error client");
    take_reliable_messages(&interactive_mailbox);
    let interactive_response = shared.execute_command_request(
        interactive,
        ClientKind::Interactive,
        &mut interactive_context,
        3,
        &CommandInvocation::new("source-file", [runtime.display().to_string()]),
    );
    assert_eq!(
        interactive_response,
        CommandResponse::Success {
            request_id: 3,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    let warnings = take_reliable_messages(&interactive_mailbox)
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
            "Can't find session: missing-runtime",
            "Invalid option: nonexistent-option",
            "Empty variable name",
        ]
    );

    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    let mut control_context = ExecutionContext::default();
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new("attach-session", ["-t", "replay-runtime-errors"]),
        )
        .expect("attach control runtime error client");
    take_reliable_messages(&control_mailbox);
    let control_response = shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut control_context,
        4,
        &CommandInvocation::new("source-file", [runtime.display().to_string()]),
    );
    assert_eq!(
        control_response,
        CommandResponse::Success {
            request_id: 4,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_guards(take_reliable_messages(&control_mailbox)),
        [
            ("can't find session: missing-runtime".to_owned(), true, true,),
            ("invalid option: nonexistent-option".to_owned(), true, true),
            ("empty variable name".to_owned(), true, true),
            (String::new(), false, false),
        ]
    );

    let alias_root = directory.path().join("source-alias.conf");
    fs::write(
        &alias_root,
        "source ignored.conf\nset-option -g @source-alias-after yes\n",
    )
    .expect("source alias root");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-s", "command-alias[100]", "source=new-window -t missing:"],
            ),
        )
        .expect("source alias");
    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            5,
            &CommandInvocation::new("source-file", [alias_root.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 5,
            output: RawText::default(),
            exit_code: 1,
            stderr: "can't find session: missing\n".to_owned(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(read_global_option(&shared, "@source-alias-after"), "yes");

    let mut report = ConfigLoadReport::default();
    shared
        .load_config_file_with_report(&alias_root, &mut context, 0, &mut report)
        .expect("reload source alias report");
    assert!(matches!(
        report.replay_issues(),
        [ConfigReplayIssue::CommandError {
            source: Some(source),
            message,
        }] if source.source == alias_root.display().to_string()
            && source.line == 1
            && message == "can't find session: missing"
    ));
}

#[test]
fn source_construction_failure_aborts_file_before_replay() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("typed-phase.conf");
    fs::write(
        &source,
        "set-environment -g\n\
             set-option -g @after-parse yes\n\
             kill-session -t missing-phase\n\
             set-option -g @after-runtime yes\n",
    )
    .expect("typed phase source");
    let shared = Arc::new(Shared::new(63));
    let client = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "typed-phase"]),
        )
        .expect("session");

    assert_eq!(
        shared.execute_command_request(
            client,
            ClientKind::Command,
            &mut context,
            1,
            &CommandInvocation::new("source-file", [source.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: format!(
                "{}:1: command set-environment: too few arguments (need at least 1)\n",
                source.display()
            )
            .into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert!(read_global_option(&shared, "@after-parse").is_empty());
    assert!(read_global_option(&shared, "@after-runtime").is_empty());

    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);
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
    let events = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlCommandGuard { .. },
                ..
            }) => Some("guard".to_owned()),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlConfigError { text },
                ..
            }) => Some(format!("warning:{text}")),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        events,
        [format!(
            "warning:{}:1: command set-environment: too few arguments (need at least 1)",
            source.display()
        )]
    );
}

#[test]
fn control_construction_warning_follows_top_level_source_batch_replay() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let good = directory.path().join("good.conf");
    let bad = directory.path().join("bad.conf");
    let later = directory.path().join("later.conf");
    fs::write(&good, "display-message -p TOP_GOOD\n").expect("good source");
    fs::write(
        &bad,
        "set-option -g @top-bad-before wrong\n\
             wibble\n\
             set-option -g @top-bad-after wrong\n",
    )
    .expect("bad source");
    fs::write(&later, "display-message -p TOP_LATER\n").expect("later source");

    let shared = Arc::new(Shared::new(76));
    let command = ClientId(u64::from(u16::MAX));
    let mut command_context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("new-session", ["-d", "-s", "top-control-batch"]),
        )
        .expect("top-level Control batch session");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    let mut control_context = ExecutionContext::default();
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new("attach-session", ["-t", "top-control-batch"]),
        )
        .expect("attach top-level Control batch client");
    take_reliable_messages(&mailbox);

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut control_context,
            1,
            &control_stdin_command(
                "source-file",
                [
                    good.display().to_string(),
                    bad.display().to_string(),
                    later.display().to_string(),
                ],
            ),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_config_timeline(take_reliable_messages(&mailbox)),
        [
            "guard:1:false:false:TOP_GOOD".to_owned(),
            "guard:1:false:false:TOP_LATER".to_owned(),
            format!("warning:{}:2: unknown command: wibble", bad.display()),
            "complete".to_owned(),
        ]
    );
    assert!(read_global_option(&shared, "@top-bad-before").is_empty());
    assert!(read_global_option(&shared, "@top-bad-after").is_empty());
}

#[test]
fn control_construction_warning_follows_nested_source_batch_replay() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let good = directory.path().join("nested-good.conf");
    let bad = directory.path().join("nested-bad.conf");
    let later = directory.path().join("nested-later.conf");
    let root = directory.path().join("root.conf");
    fs::write(&good, "display-message -p NESTED_GOOD\n").expect("nested good source");
    fs::write(&bad, "wibble\n").expect("nested bad source");
    fs::write(&later, "display-message -p NESTED_LATER\n").expect("nested later source");
    fs::write(
        &root,
        format!(
            "source-file '{}' '{}' '{}'\n\
                 display-message -p ROOT_AFTER\n",
            good.display(),
            bad.display(),
            later.display(),
        ),
    )
    .expect("nested batch root");

    let shared = Arc::new(Shared::new(76));
    let command = ClientId(u64::from(u16::MAX));
    let mut command_context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("new-session", ["-d", "-s", "nested-control-batch"]),
        )
        .expect("nested Control batch session");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    let mut control_context = ExecutionContext::default();
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new("attach-session", ["-t", "nested-control-batch"]),
        )
        .expect("attach nested Control batch client");
    take_reliable_messages(&mailbox);

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut control_context,
            1,
            &control_stdin_command("source-file", [root.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_config_timeline(take_reliable_messages(&mailbox)),
        [
            "guard:1:false:false:".to_owned(),
            "guard:1:false:false:NESTED_GOOD".to_owned(),
            "guard:1:false:false:NESTED_LATER".to_owned(),
            format!("warning:{}:1: unknown command: wibble", bad.display()),
            "complete".to_owned(),
            "guard:1:false:false:ROOT_AFTER".to_owned(),
            "complete".to_owned(),
        ]
    );
}

#[test]
fn control_parse_warning_follows_nested_source_frame_once() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let child = directory.path().join("nested-parser.conf");
    let root = directory.path().join("nested-parser-root.conf");
    fs::write(&child, "%endif\n").expect("nested parser source");
    fs::write(
        &root,
        format!(
            "source-file '{}'\n\
                 display-message -p NESTED_PARSE_OUTER_AFTER\n",
            child.display(),
        ),
    )
    .expect("nested parser root");

    let shared = Arc::new(Shared::new(76));
    let command = ClientId(u64::from(u16::MAX));
    let mut command_context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("new-session", ["-d", "-s", "nested-control-parser"]),
        )
        .expect("nested parser session");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    let mut control_context = ExecutionContext::default();
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new("attach-session", ["-t", "nested-control-parser"]),
        )
        .expect("attach nested parser client");
    take_reliable_messages(&mailbox);

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut control_context,
            1,
            &control_stdin_command("source-file", [root.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_config_timeline(take_reliable_messages(&mailbox)),
        [
            "guard:1:false:false:".to_owned(),
            format!("warning:{}:1: syntax error", child.display()),
            "complete".to_owned(),
            "guard:1:false:false:NESTED_PARSE_OUTER_AFTER".to_owned(),
            "complete".to_owned(),
        ]
    );
}

#[test]
fn control_hook_parse_warning_reaches_the_request_status() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let child = directory.path().join("hook-parser.conf");
    fs::write(&child, "%endif\n").expect("hook parser source");

    let shared = Arc::new(Shared::new(76));
    let command = ClientId(u64::from(u16::MAX));
    let mut command_context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("new-session", ["-d", "-s", "control-hook-parser"]),
        )
        .expect("hook parser session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g".to_owned(),
                    "after-display-message".to_owned(),
                    format!("source-file '{}'", child.display()),
                ],
            ),
        )
        .expect("hook parser command");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    let mut control_context = ExecutionContext::default();
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new("attach-session", ["-t", "control-hook-parser"]),
        )
        .expect("attach hook parser client");
    take_reliable_messages(&mailbox);

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut control_context,
            1,
            &CommandInvocation::new("display-message", ["-p", "HOOK_PARSE_TRIGGER"]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: "HOOK_PARSE_TRIGGER".into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        control_config_timeline(take_reliable_messages(&mailbox)),
        [
            "guard:0:false:false:".to_owned(),
            format!("warning:{}:1: syntax error", child.display()),
            "complete".to_owned(),
        ]
    );
}

#[test]
fn control_source_name_failures_are_config_warnings_without_guards() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("name-errors.conf");
    fs::write(
        &source,
        "source ignored.conf\n\
             wibble\n\
             kill-s\n\
             badalias\n\
             set-environment -g\n\
             new-pane\n\
             set-option -g @after-name-errors yes\n",
    )
    .expect("name error source");
    let shared = Arc::new(Shared::new(76));
    let mut context = ExecutionContext::default();
    for (index, alias) in ["source=display-message \\", "badalias=display-message \\"]
        .into_iter()
        .enumerate()
    {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    ["-s", &format!("command-alias[{}]", index + 90), alias],
                ),
            )
            .expect("unsupported command alias");
    }
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);
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
    let events = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ControlCommandGuard {
                        output,
                        error,
                        sticky_failure,
                        ..
                    },
                ..
            }) => Some(format!("guard:{error}:{sticky_failure}:{output}")),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlConfigError { text },
                ..
            }) => Some(format!("warning:{text}")),
            _ => None,
        })
        .collect::<Vec<_>>();
    let source = source.display();
    assert_eq!(
        events,
        [format!("warning:{source}:1: unknown command: source")]
    );
    assert!(read_global_option(&shared, "@after-name-errors").is_empty());
}

#[test]
fn control_config_diagnostics_are_typed_rather_than_recognized_by_prose() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("typed-config.conf");
    fs::write(&source, "wibble\n").expect("typed config source");
    let shared = Arc::new(Shared::new(76));
    let mut context = ExecutionContext::default();
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);
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

    let mut config_errors = Vec::new();
    let mut client_messages = Vec::new();
    for message in take_reliable_messages(&mailbox) {
        match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlConfigError { text },
                ..
            }) => config_errors.push(text),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientMessage { kind, text, .. },
                ..
            }) => client_messages.push(format!("{kind:?}:{text}")),
            _ => {}
        }
    }
    assert_eq!(
        config_errors,
        [format!("{}:1: unknown command: wibble", source.display())]
    );
    assert!(client_messages.is_empty(), "{client_messages:?}");
}

#[test]
fn control_config_alias_snapshot_freezes_deferred_error_classification() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mutating = directory.path().join("mutating-alias.conf");
    let failing = directory.path().join("frozen-alias-error.conf");
    fs::write(
        &mutating,
        "set-option -s command-alias[90] 'badname=display-message -p live'\n",
    )
    .expect("mutating Control alias source");
    fs::write(
        &failing,
        "badname\nset-option -g @after-frozen-control-alias yes\n",
    )
    .expect("frozen Control alias failure");
    let shared = Arc::new(Shared::new(76));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-s", "command-alias[90]", "badname=display-message \\"],
            ),
        )
        .expect("seed frozen Control alias");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            1,
            &CommandInvocation::new(
                "source-file",
                [
                    mutating.display().to_string(),
                    failing.display().to_string(),
                ],
            ),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    let events = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ControlCommandGuard {
                        output,
                        error,
                        sticky_failure,
                        ..
                    },
                ..
            }) => Some(format!("guard:{error}:{sticky_failure}:{output}")),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlConfigError { text },
                ..
            }) => Some(format!("warning:{text}")),
            _ => None,
        })
        .collect::<Vec<_>>();
    let failing = failing.display();
    assert_eq!(
        events,
        [
            "guard:false:false:".to_owned(),
            format!("warning:{failing}:1: unknown command: badname"),
        ]
    );
    assert!(read_global_option(&shared, "@after-frozen-control-alias").is_empty());
    assert_eq!(
        shared
            .execute(
                control,
                ClientKind::Control,
                &mut context,
                &CommandInvocation::new("show-options", ["-sv", "command-alias[90]"]),
            )
            .expect("read replayed alias")
            .output,
        "badname=display-message -p live"
    );
}

#[test]
fn control_sourced_unknown_command_does_not_run_command_error_hook() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("unknown.conf");
    fs::write(&source, "wibble\n").expect("unknown source");
    let shared = Arc::new(Shared::new(80));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "command-error",
                    "set-option -g @unexpected-command-error yes",
                ],
            ),
        )
        .expect("command-error hook");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);

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
    let events = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlCommandGuard { .. },
                ..
            }) => Some("guard".to_owned()),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlConfigError { text },
                ..
            }) => Some(format!("warning:{text}")),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        events,
        [format!(
            "warning:{}:1: unknown command: wibble",
            source.display()
        )]
    );
    assert_eq!(read_global_option(&shared, "@unexpected-command-error"), "");
}

#[test]
fn source_file_flags_preserve_parse_target_verbose_and_client_semantics() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let parse_only = directory.path().join("parse-only.conf");
    fs::write(
        &parse_only,
        "PARSE_ASSIGN=changed\n\
             set-option -g @parse-only changed\n\
             set-environment -g PARSE_COMMAND changed\n",
    )
    .expect("parse-only source");
    let invalid = directory.path().join("invalid.conf");
    fs::write(&invalid, "PARSE_INVALID=changed\nset 'oops\n").expect("invalid source");
    let beta = directory.path().join("beta.conf");
    fs::write(&beta, "set-option -g @verbose-order beta\n").expect("beta source");
    let alpha = directory.path().join("alpha.conf");
    fs::write(&alpha, "set-option -g @verbose-order alpha\n").expect("alpha source");
    let target = directory.path().join("target.conf");
    fs::write(&target, "set-environment SOURCE_TARGET selected\n").expect("target source");
    let formatted = directory.path().join("source-one.conf");
    fs::write(
        &formatted,
        "set-environment SOURCE_FORMAT_TARGET selected\n",
    )
    .expect("formatted target source");
    let targetless = directory.path().join("targetless.conf");
    fs::write(&targetless, "set-option -g @targetless loaded\n").expect("targetless source");

    let shared = Arc::new(Shared::new(63));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    for session in ["source-one", "source-two"] {
        shared
            .execute(
                command,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("new-session", ["-d", "-s", session]),
            )
            .expect("source target session");
    }

    let parsed = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        1,
        &CommandInvocation::new("source-file", ["-nv", &parse_only.display().to_string()]),
    );
    assert_eq!(
        parsed,
        CommandResponse::Success {
            request_id: 1,
            output: format!(
                "{}:2: set-option -g @parse-only changed\n\
                     {}:3: set-environment -g PARSE_COMMAND changed\n",
                parse_only.display(),
                parse_only.display()
            )
            .into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.engine.global_environment_variable("PARSE_ASSIGN"),
            None
        );
        assert_eq!(
            inner.engine.global_environment_variable("PARSE_COMMAND"),
            None
        );
    }
    assert!(read_global_option(&shared, "@parse-only").is_empty());

    let invalid_response = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        2,
        &CommandInvocation::new("source-file", ["-n", &invalid.display().to_string()]),
    );
    assert_eq!(
        invalid_response,
        CommandResponse::Success {
            request_id: 2,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("PARSE_INVALID"),
        None
    );

    let verbose = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        3,
        &CommandInvocation::new(
            "source-file",
            [
                "-v",
                &beta.display().to_string(),
                &alpha.display().to_string(),
            ],
        ),
    );
    assert_eq!(
        verbose,
        CommandResponse::Success {
            request_id: 3,
            output: format!(
                "{}:1: set-option -g @verbose-order beta\n\
                     {}:1: set-option -g @verbose-order alpha\n",
                beta.display(),
                alpha.display()
            )
            .into(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(read_global_option(&shared, "@verbose-order"), "alpha");

    let targeted = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        4,
        &CommandInvocation::new(
            "source-file",
            ["-t", "source-one:0", &target.display().to_string()],
        ),
    );
    assert!(matches!(
        targeted,
        CommandResponse::Success { exit_code: 0, .. }
    ));
    let source_one = shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-environment", ["-t", "source-one", "SOURCE_TARGET"]),
        )
        .expect("target environment")
        .output;
    assert_eq!(source_one, "SOURCE_TARGET=selected");
    assert!(matches!(
        shared.execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "show-environment",
                ["-t", "source-two", "SOURCE_TARGET"],
            ),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "unknown variable: SOURCE_TARGET"
    ));

    let formatted_path = directory
        .path()
        .join("#{session_name}.conf")
        .display()
        .to_string();
    assert!(matches!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            5,
            &CommandInvocation::new("source-file", ["-F", "-t", "source-one:0", &formatted_path],),
        ),
        CommandResponse::Success { exit_code: 0, .. }
    ));
    let formatted_target = shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "show-environment",
                ["-t", "source-one", "SOURCE_FORMAT_TARGET"],
            ),
        )
        .expect("formatted target environment")
        .output;
    assert_eq!(formatted_target, "SOURCE_FORMAT_TARGET=selected");

    let targetless_response = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        6,
        &CommandInvocation::new(
            "source-file",
            ["-t", "missing:0", &targetless.display().to_string()],
        ),
    );
    assert!(matches!(
        targetless_response,
        CommandResponse::Success { exit_code: 0, .. }
    ));
    assert_eq!(read_global_option(&shared, "@targetless"), "loaded");

    let reload_leaf = directory.path().join("reload-leaf.conf");
    fs::write(&reload_leaf, "set-option -g @reload-verbose loaded\n").expect("reload verbose leaf");
    let reload_root = directory.path().join("reload-root.conf");
    fs::write(&reload_root, "source-file reload-leaf.conf\n").expect("reload verbose root");
    shared
        .inner
        .lock()
        .client_entry(command)
        .command_streams
        .get_or_insert_default();
    shared
        .reload_user_config_with_files_and_source_base(
            command,
            &mut context,
            std::slice::from_ref(&reload_root),
            Some(directory.path()),
            SourceFileLoadOptions {
                verbose: true,
                ..SourceFileLoadOptions::default()
            },
        )
        .expect("verbose native reload");
    let reload_output = shared
        .inner
        .lock()
        .client_mut(command)
        .and_then(|c| c.command_streams.take())
        .expect("verbose native reload stream")
        .stdout;
    assert_eq!(
        reload_output,
        format!(
            "{}:1: source-file reload-leaf.conf\n\
                 {}:1: set-option -g @reload-verbose loaded\n",
            reload_root.display(),
            reload_leaf.display()
        )
    );
    assert_eq!(read_global_option(&shared, "@reload-verbose"), "loaded");

    let control = ClientId(u64::from(u16::MAX) - 1);
    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            7,
            &CommandInvocation::new("source-file", ["-v", &alpha.display().to_string()],),
        ),
        CommandResponse::Success {
            request_id: 7,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
}
