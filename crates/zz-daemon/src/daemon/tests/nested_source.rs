use super::*;

#[test]
fn nested_source_misses_use_effective_declared_paths_across_client_channels() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let parent = directory
        .path()
        .join(literal_directory_name("entry parent"));
    fs::create_dir(&parent).expect("nested source parent");
    let loud_entry = parent.join("loud.conf");
    let quiet_entry = parent.join("quiet.conf");
    let invalid_entry = parent.join("invalid-entry.conf");
    let invalid_leaf = parent.join("invalid-leaf.conf");
    fs::write(
        &loud_entry,
        "source-file -F '#{session_name}-missing[1].conf'\n",
    )
    .expect("loud nested source fixture");
    fs::write(
        &quiet_entry,
        "source-file -Fq '#{session_name}-quiet-missing[1].conf'\n",
    )
    .expect("quiet nested source fixture");
    fs::write(&invalid_entry, "source-file invalid-leaf.conf\n")
        .expect("invalid nested source entry");
    fs::write(&invalid_leaf, "wibble\n").expect("invalid nested source leaf");
    let loud_entry_pattern = source_working_directory_glob_prefix(&parent)
        .join("loud.conf")
        .display()
        .to_string();
    let quiet_entry_pattern = source_working_directory_glob_prefix(&parent)
        .join("quiet.conf")
        .display()
        .to_string();
    let invalid_entry_pattern = source_working_directory_glob_prefix(&parent)
        .join("invalid-entry.conf")
        .display()
        .to_string();

    let shared = Arc::new(Shared::new(64));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    let parent_path = parent.display().to_string();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-c", &parent_path, "-s", "nested-diagnostic"],
            ),
        )
        .expect("nested diagnostic session");
    let expected = "No such file or directory: nested-diagnostic-missing[1].conf";

    let loud = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        1,
        &CommandInvocation::new("source-file", [&loud_entry_pattern]),
    );
    assert_eq!(
        loud,
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!("{expected}\n"),
            stdout_claim: StdoutClaim::None,
        }
    );

    let quiet = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        2,
        &CommandInvocation::new("source-file", [&quiet_entry_pattern]),
    );
    assert_eq!(
        quiet,
        CommandResponse::Success {
            request_id: 2,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );

    let interactive_mailbox = OutboundMailbox::new();
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&interactive_mailbox),
    );
    shared
        .inner
        .lock()
        .client_entry(interactive)
        .working_directory
        .replace(parent.clone());
    let mut interactive_context = ExecutionContext::default();
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut interactive_context,
            &CommandInvocation::new("attach-session", ["-t", "nested-diagnostic"]),
        )
        .expect("attach interactive source diagnostic client");
    take_reliable_messages(&interactive_mailbox);
    let response = shared.execute_command_request(
        interactive,
        ClientKind::Interactive,
        &mut interactive_context,
        3,
        &CommandInvocation::new("source-file", [&loud_entry_pattern]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 3,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert!(
        take_reliable_messages(&interactive_mailbox)
            .iter()
            .any(|message| {
                matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::ClientMessage {
                            kind: ClientMessageKind::Warning,
                            text,
                            ..
                        },
                        ..
                    }) if text == expected
                )
            })
    );
    let response = shared.execute_command_request(
        interactive,
        ClientKind::Interactive,
        &mut interactive_context,
        4,
        &CommandInvocation::new("source-file", [&quiet_entry_pattern]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 4,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert!(
        !take_reliable_messages(&interactive_mailbox)
            .iter()
            .any(|message| {
                matches!(
                    message,
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::ClientMessage {
                            kind: ClientMessageKind::Warning,
                            ..
                        },
                        ..
                    })
                )
            })
    );

    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    shared
        .inner
        .lock()
        .client_entry(control)
        .working_directory
        .replace(parent.clone());
    let mut control_context = ExecutionContext::default();
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new("attach-session", ["-t", "nested-diagnostic"]),
        )
        .expect("attach control source diagnostic client");
    take_reliable_messages(&control_mailbox);
    let response = shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut control_context,
        5,
        &CommandInvocation::new("source-file", [&loud_entry_pattern]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 5,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_guards(take_reliable_messages(&control_mailbox)),
        [(expected.to_owned(), true, false)]
    );
    let response = shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut control_context,
        6,
        &CommandInvocation::new("source-file", [&quiet_entry_pattern]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 6,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_guards(take_reliable_messages(&control_mailbox)),
        [(String::new(), false, false)]
    );
    let response = shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut control_context,
        7,
        &CommandInvocation::new("source-file", [&invalid_entry_pattern]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 7,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    let invalid = format!("{}:1: unknown command: wibble", invalid_leaf.display());
    let events = take_reliable_messages(&control_mailbox)
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
            }) => Some(("guard", String::from(output), error, sticky_failure)),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlConfigError { text },
                ..
            }) => Some(("warning", text, false, false)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        events,
        [
            ("guard", String::new(), false, false),
            ("warning", invalid, false, false),
        ]
    );
}

#[cfg(unix)]
#[test]
fn nested_source_errors_group_control_frames_by_source_command() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let grouped = directory.path().join("grouped.conf");
    let separate = directory.path().join("separate.conf");
    let mixed_read = directory.path().join("mixed-read.conf");
    let unreadable = directory.path().join("unreadable.conf");
    fs::write(&grouped, "source-file missing-a.conf missing-b.conf\n")
        .expect("grouped nested source fixture");
    fs::write(
        &separate,
        "source-file missing-c.conf\nsource-file missing-d.conf\n",
    )
    .expect("separate nested source fixture");
    fs::create_dir(&unreadable).expect("unreadable source fixture");
    fs::write(
        &mixed_read,
        format!(
            "source-file '{}' missing-after-unreadable.conf\n",
            unreadable.display()
        ),
    )
    .expect("mixed nested source fixture");
    let missing_a = "No such file or directory: missing-a.conf";
    let missing_b = "No such file or directory: missing-b.conf";
    let missing_c = "No such file or directory: missing-c.conf";
    let missing_d = "No such file or directory: missing-d.conf";
    let unreadable_error = format!("Is a directory: {}", unreadable.display());
    let missing_after_unreadable = "No such file or directory: missing-after-unreadable.conf";

    let shared = Arc::new(Shared::new(66));
    let command = ClientId(u64::from(u16::MAX));
    let mut command_context = ExecutionContext::default();
    let response = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut command_context,
        1,
        &CommandInvocation::new("source-file", [grouped.display().to_string()]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!("{missing_a}\n{missing_b}\n"),
            stdout_claim: StdoutClaim::None,
        }
    );

    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    take_reliable_messages(&control_mailbox);
    let mut control_context = ExecutionContext::default();
    let response = shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut control_context,
        2,
        &control_stdin_command("source-file", [grouped.display().to_string()]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 2,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_guards(take_reliable_messages(&control_mailbox)),
        [(format!("{missing_a}\n{missing_b}"), true, false)]
    );

    let response = shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut control_context,
        3,
        &control_stdin_command("source-file", [separate.display().to_string()]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 3,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_guards(take_reliable_messages(&control_mailbox)),
        [
            (missing_c.to_owned(), true, false),
            (missing_d.to_owned(), true, false),
        ]
    );

    let response = shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut control_context,
        4,
        &control_stdin_command("source-file", [mixed_read.display().to_string()]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 4,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    let messages = take_reliable_messages(&control_mailbox);
    assert!(matches!(
        messages.as_slice(),
        [
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlCommandGuard {
                    output,
                    error: false,
                    sticky_failure: false,
                    ..
                },
                ..
            }),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlSourceFile {
                    event: ControlSourceFileEvent::ReadError(text),
                },
                ..
            }),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlSourceFile {
                    event: ControlSourceFileEvent::Complete,
                },
                ..
            }),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlSourceFile {
                    event: ControlSourceFileEvent::Complete,
                },
                ..
            }),
        ] if output == missing_after_unreadable && text == &unreadable_error
    ));

    let response = shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut control_context,
        41,
        &control_stdin_command("source-file", [unreadable.display().to_string()]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 41,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert!(matches!(
        take_reliable_messages(&control_mailbox).as_slice(),
        [
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlSourceFile {
                    event: ControlSourceFileEvent::ReadError(text),
                },
                ..
            }),
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlSourceFile {
                    event: ControlSourceFileEvent::Complete,
                },
                ..
            }),
        ] if text == &unreadable_error
    ));

    #[cfg(unix)]
    {
        let colon_unreadable = directory.path().join("a: b");
        let colon_read = directory.path().join("colon-read.conf");
        fs::create_dir(&colon_unreadable).expect("colon-space unreadable fixture");
        fs::write(
            &colon_read,
            format!("source-file '{}'\n", colon_unreadable.display()),
        )
        .expect("colon-space source fixture");

        let response = shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut control_context,
            40,
            &control_stdin_command("source-file", [colon_read.display().to_string()]),
        );
        assert_eq!(
            response,
            CommandResponse::Success {
                request_id: 40,
                output: RawText::default(),
                exit_code: 1,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }
        );
        let messages = take_reliable_messages(&control_mailbox);
        assert!(matches!(
            messages.as_slice(),
            [
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlCommandGuard {
                        output,
                        error: false,
                        sticky_failure: false,
                        ..
                    },
                    ..
                }),
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlSourceFile {
                        event: ControlSourceFileEvent::ReadError(text),
                    },
                    ..
                }),
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlSourceFile {
                        event: ControlSourceFileEvent::Complete,
                    },
                    ..
                }),
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlSourceFile {
                        event: ControlSourceFileEvent::Complete,
                    },
                    ..
                }),
            ] if output.is_empty()
                && text == &format!(
                    "Is a directory: {}",
                    colon_unreadable.display()
                )
        ));
    }

    let interactive_mailbox = OutboundMailbox::new();
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&interactive_mailbox),
    );
    take_reliable_messages(&interactive_mailbox);
    let response = shared.execute_command_request(
        interactive,
        ClientKind::Interactive,
        &mut ExecutionContext::default(),
        5,
        &CommandInvocation::new("source-file", [grouped.display().to_string()]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 5,
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
    assert_eq!(warnings, [missing_a, missing_b]);
}

#[test]
fn nested_source_control_guards_precede_deeper_replay() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path().join("root.conf");
    let middle = directory.path().join("middle.conf");
    let leaf = directory.path().join("leaf.conf");
    fs::write(
        &root,
        format!(
            "source-file root-before-missing.conf '{}' root-after-missing.conf\n",
            middle.display()
        ),
    )
    .expect("root source fixture");
    fs::write(
        &middle,
        format!(
            "source-file middle-before-missing.conf '{}' middle-after-missing.conf\n",
            leaf.display()
        ),
    )
    .expect("middle source fixture");
    fs::write(&leaf, "display-message -p CONTROL_QUEUE_LEAF\n").expect("leaf source fixture");

    let shared = Arc::new(Shared::new(77));
    let command = ClientId(u64::from(u16::MAX));
    let mut command_context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("new-session", ["-d", "-s", "nested-control-queue"]),
        )
        .expect("nested control queue session");

    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    let mut control_context = ExecutionContext::default();
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new("attach-session", ["-t", "nested-control-queue"]),
        )
        .expect("attach nested control queue client");
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
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_guards(take_reliable_messages(&mailbox)),
        [
            (
                "No such file or directory: root-before-missing.conf\nNo such file or directory: root-after-missing.conf"
                    .to_owned(),
                false,
                false,
            ),
            (
                "No such file or directory: middle-before-missing.conf\nNo such file or directory: middle-after-missing.conf"
                    .to_owned(),
                false,
                false,
            ),
            ("CONTROL_QUEUE_LEAF".to_owned(), false, false),
        ]
    );
}

#[cfg(unix)]
#[test]
fn sourced_foreground_shell_output_follows_its_empty_guard() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("run-shell.conf");
    fs::write(
        &source,
        "run-shell 'printf CHILD; exit 3' ; display-message -p SAME\n",
    )
    .expect("source fixture");

    let shared = Arc::new(Shared::new(77));
    let command = ClientId(u64::from(u16::MAX));
    let mut command_context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new("new-session", ["-d", "-s", "source-run-shell"]),
        )
        .expect("source run-shell session");

    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    let mut control_context = ExecutionContext::default();
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new("attach-session", ["-t", "source-run-shell"]),
        )
        .expect("attach source run-shell client");
    take_reliable_messages(&mailbox);

    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut control_context,
            1,
            &control_stdin_command("source-file", [source.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 0,
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
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        timeline,
        [
            "guard:1:false:".to_owned(),
            "raw:CHILD\n'printf CHILD; exit 3' returned 3".to_owned(),
            "guard:1:false:SAME".to_owned(),
        ]
    );
}

#[test]
fn control_source_read_events_preserve_tree_order_and_completion_cardinality() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let first_unreadable = directory.path().join("first unreadable");
    let second_unreadable = directory.path().join("second unreadable");
    fs::create_dir(&first_unreadable).expect("first unreadable directory");
    fs::create_dir(&second_unreadable).expect("second unreadable directory");
    let child = directory.path().join("child.conf");
    fs::write(&child, "display-message -p CHILD\n").expect("child source fixture");
    let root = directory.path().join("root.conf");
    fs::write(
        &root,
        format!(
            "display-message -p ROOT_BEFORE\n\
                 source-file '{}' '{}' '{}'\n\
                 display-message -p ROOT_AFTER\n",
            first_unreadable.display(),
            second_unreadable.display(),
            child.display(),
        ),
    )
    .expect("root source fixture");
    let first_read_error =
        fs::read_to_string(&first_unreadable).expect_err("first source read fails");
    let first_error = source_read_error_warning(&first_unreadable, &first_read_error);
    let second_read_error =
        fs::read_to_string(&second_unreadable).expect_err("second source read fails");
    let second_error = source_read_error_warning(&second_unreadable, &second_read_error);

    let shared = Arc::new(Shared::new(78));
    let command = ClientId(u64::from(u16::MAX));
    let mut command_context = ExecutionContext::default();
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);
    let mut control_context = ExecutionContext::default();
    let timeline = |messages: Vec<ProtocolMessage>| {
        messages
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
                    payload:
                        EventPayload::ControlSourceFile {
                            event: ControlSourceFileEvent::ReadError(text),
                        },
                    ..
                }) => Some(format!("read:{text}")),
                ProtocolMessage::Event(Event {
                    payload:
                        EventPayload::ControlSourceFile {
                            event: ControlSourceFileEvent::Complete,
                        },
                    ..
                }) => Some("complete".to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

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
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        timeline(take_reliable_messages(&mailbox)),
        [
            "guard:1:false:ROOT_BEFORE".to_owned(),
            "guard:1:false:".to_owned(),
            format!("read:{first_error}"),
            format!("read:{second_error}"),
            "guard:1:false:CHILD".to_owned(),
            "complete".to_owned(),
            "guard:1:false:ROOT_AFTER".to_owned(),
            "complete".to_owned(),
        ]
    );

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
                    format!("source-file '{}'", root.display()),
                ],
            ),
        )
        .expect("source hook");
    take_reliable_messages(&mailbox);
    assert_eq!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut control_context,
            2,
            &control_stdin_command("display-message", ["-p", "TRIGGER"]),
        ),
        CommandResponse::Success {
            request_id: 2,
            output: "TRIGGER".into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        timeline(take_reliable_messages(&mailbox)),
        [
            "guard:0:false:".to_owned(),
            "guard:0:false:ROOT_BEFORE".to_owned(),
            "guard:0:false:".to_owned(),
            format!("read:{first_error}"),
            format!("read:{second_error}"),
            "guard:0:false:CHILD".to_owned(),
            "complete".to_owned(),
            "guard:0:false:ROOT_AFTER".to_owned(),
            "complete".to_owned(),
        ]
    );
}

#[test]
fn control_source_read_preserves_not_found_after_the_match_boundary() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let vanished = directory.path().join("vanished.conf");
    fs::write(&vanished, "display-message -p VANISHED\n").expect("vanished fixture");
    fs::remove_file(&vanished).expect("remove matched source");
    let shared = Shared::new(79);
    let mut report = ConfigLoadReport::default();
    assert!(matches!(
        shared.parse_config_file(
            &vanished,
            &mut report,
            SourceFileLoadOptions {
                control_target: Some((
                    ClientId(7),
                    CONTROL_COMMAND_FRAME_FLAGS_CONTROL,
                )),
                ..SourceFileLoadOptions::default()
            },
            true,
        ),
        Err(DaemonError::Io(error)) if error.kind() == ErrorKind::NotFound
    ));
}

#[cfg(unix)]
#[test]
fn nested_source_glob_errors_are_loud_with_quiet_and_use_the_declared_path() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let entry = directory.path().join("entry.conf");
    let declared = "invalid\0pattern.conf";
    fs::write(&entry, format!("source-file -q '{declared}'\n")).expect("nested glob error fixture");
    let glob_error = source_glob_matches(Path::new(declared))
        .errors
        .into_iter()
        .next()
        .expect("NUL glob error");
    let expected = source_glob_error_warning(Path::new(declared), &glob_error);

    let shared = Arc::new(Shared::new(65));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    let response = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        1,
        &CommandInvocation::new("source-file", [entry.display().to_string()]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!("{expected}\n"),
            stdout_claim: StdoutClaim::None,
        }
    );

    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);
    let response = shared.execute_command_request(
        client,
        ClientKind::Control,
        &mut context,
        2,
        &CommandInvocation::new("source-file", [entry.display().to_string()]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 2,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        control_command_guards(take_reliable_messages(&mailbox)),
        [(expected, true, false)]
    );
}

#[test]
fn runtime_source_invocations_release_between_sequential_commands() {
    fn fan_out(directory: &Path, name: &str, leaves: usize) -> PathBuf {
        let entry = directory.join(format!("{name}.conf"));
        let mut body = String::new();
        for index in 1..=leaves {
            let leaf = directory.join(format!("{name}-leaf{index}.conf"));
            fs::write(&leaf, format!("set-option -g @{name} {index}\n"))
                .expect("runtime source leaf");
            let _ = writeln!(body, "source-file '{}'", leaf.display());
        }
        fs::write(&entry, body).expect("runtime source entry");
        entry
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let shared = Arc::new(Shared::new(67));
    let client = ClientId(u64::MAX);
    let mut context = ExecutionContext::default();

    let breadth = fan_out(directory.path(), "runtime-breadth", 60);
    let response = shared.execute_command_request(
        client,
        ClientKind::Command,
        &mut context,
        1,
        &CommandInvocation::new("source-file", [breadth.display().to_string()]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(read_global_option(&shared, "@runtime-breadth"), "60");

    let half = MAX_SOURCE_INVOCATIONS - 10;
    for (request_id, name) in [(2, "runtime-first"), (3, "runtime-second")] {
        let entry = fan_out(directory.path(), name, half);
        let response = shared.execute_command_request(
            client,
            ClientKind::Command,
            &mut context,
            request_id,
            &CommandInvocation::new("source-file", [entry.display().to_string()]),
        );
        assert_eq!(
            response,
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }
        );
        assert_eq!(
            read_global_option(&shared, &format!("@{name}")),
            half.to_string()
        );
    }
}

#[test]
fn nested_source_invocations_stop_at_the_pinned_depth_limit_on_every_channel() {
    fn chain(directory: &Path, invocations: usize) -> PathBuf {
        for level in 1..=invocations {
            let nested = if level < invocations {
                format!(
                    "source-file '{}'\n",
                    directory.join(format!("f{}.conf", level + 1)).display()
                )
            } else {
                format!(
                    "source-file '{}' '{}'\nsource-file -q '{}'\n",
                    directory.join("leaf-a.conf").display(),
                    directory.join("leaf-b.conf").display(),
                    directory.join("leaf-a.conf").display()
                )
            };
            let body =
                format!("set-option -g @depth {level}\n{nested}set-option -g @after{level} yes\n");
            fs::write(directory.join(format!("f{level}.conf")), body).expect("chain level");
        }
        fs::write(directory.join("leaf-a.conf"), "set-option -g @leaf yes\n").expect("chain leaf");
        fs::write(directory.join("leaf-b.conf"), "set-option -g @leaf yes\n").expect("chain leaf");
        directory.join("f1.conf")
    }

    fn read_global(shared: &Arc<Shared>, name: &str) -> RawText {
        shared
            .execute(
                ClientId(u64::from(u16::MAX)),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("show-options", ["-gv", name]),
            )
            .expect("read global option")
            .output
    }

    fn source_messages(mailbox: &OutboundMailbox, expected_kind: ClientMessageKind) -> Vec<String> {
        take_reliable_messages(mailbox)
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ClientMessage { kind, text, .. },
                    ..
                }) if kind == expected_kind => Some(text),
                _ => None,
            })
            .collect()
    }

    let shared = Arc::new(Shared::new(67));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();

    let allowed_directory = tempfile::tempdir().expect("temporary directory");
    let allowed = chain(allowed_directory.path(), MAX_SOURCE_INVOCATIONS - 1);
    let response = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        1,
        &CommandInvocation::new("source-file", [allowed.display().to_string()]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        read_global(&shared, "@depth"),
        (MAX_SOURCE_INVOCATIONS - 1).to_string()
    );
    assert_eq!(read_global(&shared, "@leaf"), "yes");

    let refused_directory = tempfile::tempdir().expect("temporary directory");
    let refused = chain(refused_directory.path(), MAX_SOURCE_INVOCATIONS);
    let response = shared.execute_command_request(
        command,
        ClientKind::Command,
        &mut context,
        2,
        &CommandInvocation::new("source-file", [refused.display().to_string()]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 2,
            output: RawText::default(),
            exit_code: 1,
            stderr: format!("{NESTED_SOURCE_LIMIT_ERROR}\n{NESTED_SOURCE_LIMIT_ERROR}\n"),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        read_global(&shared, "@depth"),
        MAX_SOURCE_INVOCATIONS.to_string()
    );
    assert_eq!(
        read_global(&shared, &format!("@after{MAX_SOURCE_INVOCATIONS}")),
        "yes"
    );

    let interactive_mailbox = OutboundMailbox::new();
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&interactive_mailbox),
    );
    take_reliable_messages(&interactive_mailbox);
    let response = shared.execute_command_request(
        interactive,
        ClientKind::Interactive,
        &mut ExecutionContext::default(),
        3,
        &CommandInvocation::new("source-file", [refused.display().to_string()]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 3,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        source_messages(&interactive_mailbox, ClientMessageKind::Warning),
        ["Too many nested files", "Too many nested files"]
    );

    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    take_reliable_messages(&control_mailbox);
    let response = shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut ExecutionContext::default(),
        4,
        &CommandInvocation::new("source-file", [refused.display().to_string()]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 4,
            output: RawText::default(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    let failures = control_command_guards(take_reliable_messages(&control_mailbox))
        .into_iter()
        .filter(|(_, error, _)| *error)
        .collect::<Vec<_>>();
    assert_eq!(
        failures,
        [
            (NESTED_SOURCE_LIMIT_ERROR.to_owned(), true, false),
            (NESTED_SOURCE_LIMIT_ERROR.to_owned(), true, false),
        ]
    );
}
