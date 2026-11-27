use super::*;

#[test]
fn raw_control_guards_preserve_bytes_even_when_deferred() {
    let mut writer = ControlWriter::new(Vec::new(), false);
    let frame = writer.begin_at(17, 1).unwrap();
    writer
        .control_command_guard_bytes_at(18, b"a\xffb\n", false, 1)
        .unwrap();
    writer.end(&frame, false).unwrap();
    assert_eq!(
        writer.output,
        b"%begin 17 1 1\n%end 17 1 1\n%begin 18 2 1\na\xffb\n%end 18 2 1\n"
    );
}

#[test]
fn native_frame_metadata_keeps_zero_max_and_raw_bytes() {
    println!(
        "Frame size={}, Option<Frame> size={}",
        std::mem::size_of::<Frame>(),
        std::mem::size_of::<Option<Frame>>()
    );
    for (time, number, flags, body) in [
        (0, 0, 0, b"0 0 0\n".as_slice()),
        (
            u64::MAX,
            u64::MAX,
            u8::MAX,
            b"18446744073709551615 18446744073709551615 255\n".as_slice(),
        ),
    ] {
        for error in [false, true] {
            let mut writer = ControlWriter::new(Vec::new(), false);
            writer.next_number = number;
            let frame = writer.begin_buffered_at(time, flags).unwrap();
            writer.payload(b"a\xffb").unwrap();
            writer.end(&frame, error).unwrap();
            assert_eq!(writer.next_number, number.saturating_add(1));

            let marker: &[u8] = if error { b"%error " } else { b"%end " };
            let mut expected = b"%begin ".to_vec();
            expected.extend_from_slice(body);
            expected.extend_from_slice(b"a\xffb\n");
            expected.extend_from_slice(marker);
            expected.extend_from_slice(body);
            if number == u64::MAX {
                writer
                    .control_command_guard_bytes_at(time, b"c\xfe", error, flags)
                    .unwrap();
                expected.extend_from_slice(b"%begin ");
                expected.extend_from_slice(body);
                expected.extend_from_slice(b"c\xfe\n");
                expected.extend_from_slice(marker);
                expected.extend_from_slice(body);
                assert_eq!(writer.next_number, u64::MAX);
            }
            assert_eq!(writer.output, expected);
        }
    }
}

#[test]
fn native_frame_metadata_preserves_partial_sink_errors() {
    let mut bytes = [0; 8];
    let mut writer = ControlWriter::new(bytes.as_mut_slice(), false);
    assert_eq!(
        writer.begin_at(0, 0).err().unwrap().kind(),
        io::ErrorKind::WriteZero
    );
    assert_eq!(writer.next_number, 2);
    drop(writer);
    assert_eq!(&bytes, b"%begin 0");

    for (error, expected) in [
        (false, b"%begin 0 1 0\n%end 0".as_slice()),
        (true, b"%begin 0 1 0\n%error 0".as_slice()),
    ] {
        let mut bytes = [0; 21];
        let mut writer = ControlWriter::new(&mut bytes[..expected.len()], false);
        let frame = writer.begin_at(0, 0).unwrap();
        assert_eq!(
            writer.end(&frame, error).unwrap_err().kind(),
            io::ErrorKind::WriteZero
        );
        drop(writer);
        assert_eq!(&bytes[..expected.len()], expected);
    }
}

#[test]
fn started_response_preserves_raw_output_before_callback_guards() {
    let mut writer = ControlWriter::new(Vec::new(), false);
    let frame = writer.begin_at(17, 1).unwrap();
    writer
        .control_command_guard_at(18, "child", false, 1)
        .unwrap();
    writer
        .response(
            &frame,
            CommandResponse::Success {
                request_id: 1,
                output: RawText::from_bytes(b"a\xffb\n".to_vec()),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
        )
        .unwrap();
    assert_eq!(
        writer.output,
        b"%begin 17 1 1\na\xffb\n%end 17 1 1\n%begin 18 2 1\nchild\n%end 18 2 1\n"
    );
}

#[test]
fn batched_control_frames_keep_guards_before_notifications_and_exit() {
    let frames = [
        ProtocolMessage::Event(zz_protocol::Event {
            sequence: 1,
            payload: EventPayload::ControlCommandGuard {
                output: "ready\n".into(),
                error: false,
                sticky_failure: false,
                flags: 1,
            },
        }),
        ProtocolMessage::Event(zz_protocol::Event {
            sequence: 2,
            payload: EventPayload::HookEvent {
                name: "session-created".to_owned(),
                variables: BTreeMap::new(),
            },
        }),
        ProtocolMessage::ExecExit(zz_protocol::ExecExit {
            server_id: 9,
            outcome: ExecOutcome::Ran,
        }),
    ]
    .iter()
    .map(|message| zz_protocol::encode_protocol_message(message).unwrap())
    .collect();
    let (events, receiver) = mpsc::sync_channel(4);
    forward_protocol_message(
        ProtocolMessage::Batch(zz_protocol::Batch {
            sequence: 2,
            frames,
        }),
        &events,
    )
    .unwrap();
    events
        .send(MainEvent::Stdin(StdinEvent::Line("next".to_owned())))
        .unwrap();
    let mut receiver = ControlReceiver::new(receiver);
    let mut writer = ControlWriter::new(Vec::new(), false);
    let mut state = ControlState {
        attached_session: Some(SessionId(1)),
        ..ControlState::default()
    };
    for _ in 0..2 {
        let MainEvent::Protocol(message) = receive_control_event(&mut receiver).unwrap() else {
            panic!("expected protocol message");
        };
        handle_protocol(*message, &mut state, &mut writer).unwrap();
    }
    let text = String::from_utf8(writer.output.clone()).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert!(lines[0].starts_with("%begin "));
    assert_eq!(lines[1], "ready");
    assert!(lines[2].starts_with("%end "));
    assert_eq!(lines[3], "%sessions-changed");
    assert!(
        matches!(receive_control_event(&mut receiver).unwrap(), MainEvent::Protocol(message)
        if matches!(*message, ProtocolMessage::ExecExit(_)))
    );
    assert!(matches!(
        receive_control_event(&mut receiver).unwrap(),
        MainEvent::Stdin(StdinEvent::Line(line)) if line == "next"
    ));
}

#[test]
fn control_batch_is_one_channel_item_and_retains_children_across_returns() {
    let started = |request_id| {
        ProtocolMessage::Event(zz_protocol::Event {
            sequence: request_id,
            payload: EventPayload::ControlCommandStarted {
                request_id,
                flags: 1,
                canonical_name: Some("display-message".to_owned()),
                guard: true,
            },
        })
    };
    let response = ProtocolMessage::CommandResponse(CommandResponse::Success {
        request_id: 1,
        output: "ready".into(),
        exit_code: 0,
        stderr: String::new(),
        stdout_claim: StdoutClaim::None,
    });
    let finished = ProtocolMessage::ExecExit(zz_protocol::ExecExit {
        server_id: 9,
        outcome: ExecOutcome::Ran,
    });
    let notification = ProtocolMessage::Event(zz_protocol::Event {
        sequence: 2,
        payload: EventPayload::HookEvent {
            name: "session-created".to_owned(),
            variables: BTreeMap::new(),
        },
    });
    let frames = [response, finished, notification]
        .iter()
        .map(|message| zz_protocol::encode_protocol_message(message).unwrap())
        .collect();
    let (events, receiver) = mpsc::sync_channel(8);
    forward_protocol_message(started(1), &events).unwrap();
    forward_protocol_message(
        ProtocolMessage::Batch(zz_protocol::Batch {
            sequence: 2,
            frames,
        }),
        &events,
    )
    .unwrap();
    forward_protocol_message(started(2), &events).unwrap();
    events.send(MainEvent::Stdin(StdinEvent::Eof)).unwrap();
    let mut receiver = ControlReceiver::new(receiver);
    assert!(matches!(
        receive_control_event(&mut receiver).unwrap(),
        MainEvent::Protocol(message) if matches!(
            *message,
            ProtocolMessage::Event(zz_protocol::Event {
                payload: EventPayload::ControlCommandStarted { request_id: 1, .. }, ..
            })
        )
    ));
    assert!(matches!(
        receive_control_event(&mut receiver).unwrap(),
        MainEvent::Protocol(message) if matches!(*message, ProtocolMessage::CommandResponse(_))
    ));
    assert_eq!(receiver.pending.len(), 2);
    assert!(matches!(
        receive_control_event(&mut receiver).unwrap(),
        MainEvent::Protocol(message) if matches!(*message, ProtocolMessage::ExecExit(_))
    ));
    assert_eq!(receiver.pending.len(), 1);
    assert!(matches!(
        receive_control_event(&mut receiver).unwrap(),
        MainEvent::Protocol(message) if matches!(
            *message,
            ProtocolMessage::Event(zz_protocol::Event {
                payload: EventPayload::HookEvent { .. }, ..
            })
        )
    ));
    assert!(matches!(
        receive_control_event(&mut receiver).unwrap(),
        MainEvent::Protocol(message) if matches!(
            *message,
            ProtocolMessage::Event(zz_protocol::Event {
                payload: EventPayload::ControlCommandStarted { request_id: 2, .. }, ..
            })
        )
    ));
    assert!(matches!(
        receive_control_event(&mut receiver).unwrap(),
        MainEvent::Stdin(StdinEvent::Eof)
    ));
}

#[test]
fn malformed_control_batches_deliver_no_partial_guard() {
    let guard = ProtocolMessage::Event(zz_protocol::Event {
        sequence: 1,
        payload: EventPayload::ControlCommandGuard {
            output: "partial".into(),
            error: false,
            sticky_failure: false,
            flags: 1,
        },
    });
    let nested =
        zz_protocol::encode_protocol_message(&ProtocolMessage::Batch(zz_protocol::Batch {
            sequence: 1,
            frames: vec![zz_protocol::encode_protocol_message(&guard).unwrap()],
        }))
        .unwrap();
    for invalid in [vec![0], nested] {
        let (events, receiver) = mpsc::sync_channel(1);
        forward_protocol_message(
            ProtocolMessage::Batch(zz_protocol::Batch {
                sequence: 2,
                frames: vec![
                    zz_protocol::encode_protocol_message(&guard).unwrap(),
                    invalid,
                ],
            }),
            &events,
        )
        .unwrap();
        let mut receiver = ControlReceiver::new(receiver);
        let Err(error) = receiver.receive(None) else {
            panic!("malformed batch must fail before delivering its guard");
        };
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(receiver.pending.is_empty());
    }
}

#[test]
fn a_control_tree_base_mismatch_requests_sync_without_adopting_it() {
    let mut state = ControlState::default();
    let mut writer = ControlWriter::new(Vec::new(), false);
    handle_protocol(
        ProtocolMessage::Event(zz_protocol::Event {
            sequence: 1,
            payload: EventPayload::TreeDelta(zz_protocol::TreeDelta {
                base: 5,
                version: 6,
                ops: Vec::new(),
            }),
        }),
        &mut state,
        &mut writer,
    )
    .unwrap();
    assert!(state.tree_sync_required);
    assert_eq!(state.tree.generation, 0);
    assert_eq!(state.snapshot.generation, 0);
    assert!(writer.output.is_empty());
}

#[test]
fn termination_closes_pending_frame_and_releases_deferred_exit() {
    let mut writer = ControlWriter::new(Vec::new(), true);
    writer.start().unwrap();
    writer.hold_exit();
    writer.begin_at(17, 1).unwrap();
    writer
        .control_source_file(ControlSourceFileEvent::ReadError(
            "Bad file descriptor: -".to_owned(),
        ))
        .unwrap();
    writer.emit_exit(None).unwrap();
    writer.terminate().unwrap();
    assert_eq!(
        writer.output,
        b"\x1bP1000p%begin 17 1 1\n%end 17 1 1\nBad file descriptor: -\n%exit\n\x1b\\"
    );
    let completed = writer.output.clone();
    writer.terminate().unwrap();
    assert_eq!(writer.output, completed);
}

#[test]
fn serializer_keeps_frame_identity_payload_and_error_shapes() {
    let mut writer = ControlWriter::new(Vec::new(), false);
    let first = writer.begin_at(17, 0).unwrap();
    assert_eq!(&first.body[..usize::from(first.length)], b"17 1 0\n");
    writer
        .response(
            &first,
            CommandResponse::Success {
                request_id: 1,
                output: "one\ntwo\n".into(),
                exit_code: 7,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
        )
        .unwrap();
    let second = writer.begin_at(18, 1).unwrap();
    writer
        .response(
            &second,
            CommandResponse::Error {
                request_id: 2,
                error: ServerError::SessionNotFound("gone".to_owned()),
                output: "hook\n\n".into(),
            },
        )
        .unwrap();
    let third = writer.begin_at(19, 1).unwrap();
    writer
        .response(
            &third,
            CommandResponse::Error {
                request_id: 3,
                error: ServerError::InvalidCommand("unknown command: bogus-command".to_owned()),
                output: RawText::default(),
            },
        )
        .unwrap();
    let fourth = writer.begin_at(20, 1).unwrap();
    writer
        .response(
            &fourth,
            CommandResponse::Error {
                request_id: 4,
                error: ServerError::UnsupportedCommand("new-pane".to_owned()),
                output: RawText::default(),
            },
        )
        .unwrap();
    let fifth = writer.begin_at(21, 0).unwrap();
    writer
        .response(
            &fifth,
            CommandResponse::Success {
                request_id: 5,
                output: "\n".into(),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
        )
        .unwrap();
    assert_eq!(
        writer.output,
        b"%begin 17 1 0\none\ntwo\n%end 17 1 0\n%begin 18 2 1\nhook\n\ncan't find session: gone\n%error 18 2 1\n%begin 19 3 1\nunknown command: bogus-command\n%error 19 3 1\n%begin 20 4 1\nunsupported command: new-pane\n%error 20 4 1\n%begin 21 5 0\n\n%end 21 5 0\n"
    );
}

#[test]
fn control_command_guards_defer_fifo_with_their_own_flags() {
    let mut writer = ControlWriter::new(Vec::new(), false);
    let direct = writer.begin_at(17, 1).unwrap();
    assert_eq!(
        writer
            .response(
                &direct,
                CommandResponse::Error {
                    request_id: 1,
                    error: ServerError::InvalidCommand(
                        "No such file or directory: direct.conf".to_owned(),
                    ),
                    output: RawText::default(),
                },
            )
            .unwrap(),
        1
    );
    let outer = writer.begin_at(18, 1).unwrap();
    writer.control_command_guard_at(19, "", false, 0).unwrap();
    writer.control_command_guard_at(20, "", false, 1).unwrap();
    writer
        .control_command_guard_at(21, "No such file or directory: partial.conf", false, 0)
        .unwrap();
    writer
        .control_command_guard_at(22, "can't find session: missing-runtime", true, 1)
        .unwrap();
    assert_eq!(
        writer
            .response(
                &outer,
                CommandResponse::Success {
                    request_id: 2,
                    output: RawText::default(),
                    exit_code: 3,
                    stderr: String::new(),
                    stdout_claim: StdoutClaim::None,
                },
            )
            .unwrap(),
        3
    );
    let fresh = writer.begin_at(23, 1).unwrap();
    writer
        .response(
            &fresh,
            CommandResponse::Success {
                request_id: 3,
                output: "fresh".into(),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
        )
        .unwrap();
    assert_eq!(
        writer.output,
        b"%begin 17 1 1\nNo such file or directory: direct.conf\n%error 17 1 1\n\
          %begin 18 2 1\n%end 18 2 1\n\
          %begin 19 3 0\n%end 19 3 0\n\
          %begin 20 4 1\n%end 20 4 1\n\
          %begin 21 5 0\nNo such file or directory: partial.conf\n%end 21 5 0\n\
          %begin 22 6 1\ncan't find session: missing-runtime\n%error 22 6 1\n\
          %begin 23 7 1\nfresh\n%end 23 7 1\n"
    );
}

#[test]
fn a_command_block_carries_the_output_bytes_it_was_given() {
    let needle: &[u8] = b"ZZBYTES=a\xffb";
    let mut writer = ControlWriter::new(Vec::new(), false);
    let frame = writer.begin(1).unwrap();
    writer
        .response(
            &frame,
            CommandResponse::Success {
                request_id: 9,
                output: RawText::from_bytes(b"ZZBYTES=a\xffb\n".to_vec()),
                stderr: String::new(),
                exit_code: 0,
                stdout_claim: StdoutClaim::None,
            },
        )
        .unwrap();
    assert!(
        writer
            .output
            .windows(needle.len())
            .any(|window| window == needle),
        "control payload dropped the byte: {:?}",
        writer.output
    );
}

#[test]
fn opaque_commands_render_children_and_only_fallback_before_them() {
    let mut empty = ControlWriter::new(Vec::new(), false);
    let empty_guard_frames = empty.command_guard_frames;
    assert_eq!(
        render_command_response(
            &mut empty,
            None,
            1,
            empty_guard_frames,
            CommandResponse::Success {
                request_id: 1,
                output: "outer output".into(),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
        )
        .unwrap(),
        0
    );
    assert!(empty.output.is_empty());

    let mut children = ControlWriter::new(Vec::new(), false);
    let child_guard_frames = children.command_guard_frames;
    children
        .control_command_guard_at(17, "first", false, 0)
        .unwrap();
    children
        .control_command_guard_at(18, "second", false, 1)
        .unwrap();
    let child_output = children.output.clone();
    assert_eq!(
        render_command_response(
            &mut children,
            None,
            1,
            child_guard_frames,
            CommandResponse::Error {
                request_id: 2,
                error: ServerError::InvalidCommand("outer failure".to_owned()),
                output: "outer output".into(),
            },
        )
        .unwrap(),
        1
    );
    assert_eq!(children.output, child_output);

    let mut response_failure = ControlWriter::new(Vec::new(), false);
    let response_guard_frames = response_failure.command_guard_frames;
    assert_eq!(
        render_command_response(
            &mut response_failure,
            None,
            1,
            response_guard_frames,
            CommandResponse::Error {
                request_id: 3,
                error: ServerError::InvalidCommand("outer failure".to_owned()),
                output: RawText::default(),
            },
        )
        .unwrap(),
        1
    );
    let lines = std::str::from_utf8(&response_failure.output)
        .unwrap()
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(lines[1], "outer failure");
    assert!(lines[0].ends_with(" 1 1"));
    assert_eq!(
        lines[0].strip_prefix("%begin "),
        lines[2].strip_prefix("%error ")
    );

    let mut submission_failure = ControlWriter::new(Vec::new(), false);
    let submission_guard_frames = submission_failure.command_guard_frames;
    render_command_failure(
        &mut submission_failure,
        None,
        0,
        submission_guard_frames,
        "request failed",
    )
    .unwrap();
    let lines = std::str::from_utf8(&submission_failure.output)
        .unwrap()
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(lines[1], "request failed");
    assert!(lines[0].ends_with(" 1 0"));
    assert_eq!(
        lines[0].strip_prefix("%begin "),
        lines[2].strip_prefix("%error ")
    );
}

#[test]
fn standalone_diagnostics_only_set_source_read_return_codes() {
    let source_read = format!(
        "stream did not contain valid UTF-8: {}",
        std::env::temp_dir().join("invalid-source.conf").display()
    );
    for kind in [
        zz_protocol::ClientMessageKind::Error,
        zz_protocol::ClientMessageKind::Warning,
    ] {
        let mut source_writer = ControlWriter::new(Vec::new(), false);
        let mut source_state = ControlState::default();
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: EventPayload::ClientMessage {
                    pane: None,
                    kind,
                    text: source_read.clone(),
                },
            }),
            &mut source_state,
            &mut source_writer,
        )
        .unwrap();
        assert_eq!(source_state.return_code, 1);
        assert_eq!(completed_exit_code(0, &source_state), 1);
        let source_lines = std::str::from_utf8(&source_writer.output)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();
        assert_eq!(source_lines[1], source_read);
        assert!(source_lines[2].starts_with("%error "));
    }

    let mut unrelated_writer = ControlWriter::new(Vec::new(), false);
    let mut unrelated_state = ControlState::default();
    handle_protocol(
        ProtocolMessage::Event(zz_protocol::Event {
            sequence: 2,
            payload: EventPayload::ClientMessage {
                pane: None,
                kind: zz_protocol::ClientMessageKind::Error,
                text: "background worker failed".to_owned(),
            },
        }),
        &mut unrelated_state,
        &mut unrelated_writer,
    )
    .unwrap();
    assert_eq!(unrelated_state.return_code, 0);
    assert_eq!(completed_exit_code(0, &unrelated_state), 0);
    assert!(
        std::str::from_utf8(&unrelated_writer.output)
            .unwrap()
            .contains("background worker failed")
    );
}

#[test]
fn control_command_guard_status_is_independent_of_its_terminator() {
    for (flags, sticky_failure, error, output, expected) in [
        (0, false, false, "diagnostic", 0),
        (1, false, true, "diagnostic", 0),
        (
            0,
            false,
            true,
            "No such file or directory: missing-source.conf",
            1,
        ),
        (1, true, false, "diagnostic", 1),
    ] {
        let mut writer = ControlWriter::new(Vec::new(), false);
        let mut state = ControlState::default();
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: EventPayload::ControlCommandGuard {
                    output: output.into(),
                    error,
                    sticky_failure,
                    flags,
                },
            }),
            &mut state,
            &mut writer,
        )
        .unwrap();
        assert_eq!(state.return_code, expected);
        let lines = std::str::from_utf8(&writer.output)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();
        assert_eq!(lines[1], output);
        assert!(lines[0].ends_with(&format!(" {flags}")));
        let terminator = if error { "%error " } else { "%end " };
        assert_eq!(
            lines[0].strip_prefix("%begin "),
            lines[2].strip_prefix(terminator)
        );
    }

    let mut writer = ControlWriter::new(Vec::new(), false);
    let mut state = ControlState::default();
    for (sequence, sticky_failure) in [(1, true), (2, false)] {
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence,
                payload: EventPayload::ControlCommandGuard {
                    output: "diagnostic".into(),
                    error: false,
                    sticky_failure,
                    flags: 0,
                },
            }),
            &mut state,
            &mut writer,
        )
        .unwrap();
        assert_eq!(state.return_code, 1);
    }
}

#[test]
fn control_source_file_events_defer_raw_errors_and_consume_hidden_numbers() {
    let mut writer = ControlWriter::new(Vec::new(), false);
    let mut state = ControlState::default();
    let direct = writer.begin_at(18, 1).unwrap();
    for (sequence, payload) in [
        (
            1,
            EventPayload::ControlCommandGuard {
                output: "".into(),
                error: false,
                sticky_failure: false,
                flags: 1,
            },
        ),
        (
            2,
            EventPayload::ControlSourceFile {
                event: ControlSourceFileEvent::ReadError("Is a directory: nested.conf".to_owned()),
            },
        ),
        (
            3,
            EventPayload::ControlSourceFile {
                event: ControlSourceFileEvent::Complete,
            },
        ),
        (
            4,
            EventPayload::ControlCommandGuard {
                output: "AFTER".into(),
                error: false,
                sticky_failure: false,
                flags: 1,
            },
        ),
    ] {
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event { sequence, payload }),
            &mut state,
            &mut writer,
        )
        .unwrap();
    }
    writer
        .response(
            &direct,
            CommandResponse::Success {
                request_id: 1,
                output: RawText::default(),
                exit_code: 1,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
        )
        .unwrap();
    assert_eq!(state.return_code, 1);
    let lines = std::str::from_utf8(&writer.output)
        .unwrap()
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 8);
    assert_eq!(lines[0], "%begin 18 1 1");
    assert_eq!(lines[1], "%end 18 1 1");
    assert!(lines[2].starts_with("%begin "));
    assert!(lines[2].ends_with(" 2 1"));
    assert_eq!(
        lines[2].strip_prefix("%begin "),
        lines[3].strip_prefix("%end ")
    );
    assert_eq!(lines[4], "Is a directory: nested.conf");
    assert!(lines[5].starts_with("%begin "));
    assert!(lines[5].ends_with(" 4 1"));
    assert_eq!(lines[6], "AFTER");
    assert_eq!(
        lines[5].strip_prefix("%begin "),
        lines[7].strip_prefix("%end ")
    );
}

#[test]
fn legacy_diagnostic_errors_still_defer_after_open_commands() {
    let mut writer = ControlWriter::new(Vec::new(), false);
    let nested = writer.begin_at(18, 1).unwrap();
    writer
        .diagnostic_error_at(
            19,
            "No such file or directory: nested-a.conf\nNo such file or directory: nested-b.conf",
        )
        .unwrap();
    assert_eq!(
        writer
            .response(
                &nested,
                CommandResponse::Success {
                    request_id: 2,
                    output: RawText::default(),
                    exit_code: 0,
                    stderr: String::new(),
                    stdout_claim: StdoutClaim::None,
                },
            )
            .unwrap(),
        0
    );
    assert_eq!(
        writer.output,
        b"%begin 18 1 1\n%end 18 1 1\n%begin 19 2 1\nNo such file or directory: nested-a.conf\nNo such file or directory: nested-b.conf\n%error 19 2 1\n"
    );
}

#[test]
fn generic_nonzero_success_ends_and_continues() {
    let success = CommandResponse::Success {
        request_id: 1,
        output: RawText::default(),
        exit_code: 3,
        stderr: String::new(),
        stdout_claim: StdoutClaim::None,
    };
    assert!(!response_aborts_line(&success));
    let mut writer = ControlWriter::new(Vec::new(), false);
    let frame = writer.begin_at(17, 1).unwrap();
    assert_eq!(writer.response(&frame, success).unwrap(), 3);
    assert_eq!(writer.output, b"%begin 17 1 1\n%end 17 1 1\n");
    assert!(response_aborts_line(&CommandResponse::Error {
        request_id: 2,
        error: ServerError::InvalidCommand("failed".to_owned()),
        output: RawText::default(),
    }));
    let parse = CommandResponse::Error {
        request_id: 3,
        error: ServerError::CommandParse("unknown flag -Z".to_owned()),
        output: RawText::default(),
    };
    assert!(response_aborts_line(&parse));
    assert!(!response_sets_return_code(Some("list-sessions"), &parse));
    assert!(response_sets_return_code(
        Some("kill-session"),
        &CommandResponse::Error {
            request_id: 4,
            error: ServerError::SessionNotFound("missing".to_owned()),
            output: RawText::default(),
        }
    ));
    let direct = CommandResponse::Error {
        request_id: 5,
        error: ServerError::InvalidCommand("can't find session: missing".to_owned()),
        output: RawText::default(),
    };
    assert!(!response_is_post_admission_callback_failure(&direct));
    let callback = CommandResponse::Error {
        request_id: 6,
        error: ServerError::PostAdmissionCallback(Box::new(ServerError::SessionNotFound(
            "missing".to_owned(),
        ))),
        output: RawText::default(),
    };
    assert!(response_is_post_admission_callback_failure(&callback));
    let nonzero = CommandResponse::Success {
        request_id: 7,
        output: RawText::default(),
        exit_code: 3,
        stderr: String::new(),
        stdout_claim: StdoutClaim::None,
    };
    assert!(!response_sets_return_code(Some("run-shell"), &nonzero));
    assert!(response_sets_return_code(Some("source-file"), &nonzero));
    assert!(!response_aborts_line(&nonzero));
    assert_eq!(completed_exit_code(3, &ControlState::default()), 3);
}

#[test]
fn initial_eof_waits_behind_every_queued_input() {
    let mut pending_return = None;
    let mut pending_stdin = VecDeque::new();
    let mut writer = ControlWriter::new(Vec::new(), false);
    capture_pending_return(
        StdinEvent::Line("run-shell 'sleep 1'".to_owned()),
        0,
        &mut pending_return,
        &mut pending_stdin,
        &mut writer,
    );
    capture_pending_return(
        StdinEvent::Line("display-message -p SECOND".to_owned()),
        0,
        &mut pending_return,
        &mut pending_stdin,
        &mut writer,
    );
    capture_pending_return(
        StdinEvent::Eof,
        0,
        &mut pending_return,
        &mut pending_stdin,
        &mut writer,
    );
    capture_pending_return(
        StdinEvent::Line(String::new()),
        1,
        &mut pending_return,
        &mut pending_stdin,
        &mut writer,
    );
    assert_eq!(pending_return.as_ref().map(PendingReturn::code), Some(0));
    assert!(
        pending_return
            .as_ref()
            .is_some_and(PendingReturn::has_preceding_input)
    );
    assert!(matches!(
        pending_stdin.pop_front(),
        Some(StdinEvent::Line(line)) if line == "run-shell 'sleep 1'"
    ));
    let pending_return = pending_return.as_mut().expect("pending return");
    pending_return.consume_preceding_input();
    assert!(pending_return.has_preceding_input());
    assert!(matches!(
        pending_stdin.pop_front(),
        Some(StdinEvent::Line(line)) if line == "display-message -p SECOND"
    ));
    assert!(matches!(
        pending_stdin.pop_front(),
        Some(StdinEvent::Line(line)) if line.is_empty()
    ));
    assert!(pending_stdin.is_empty());
    pending_return.consume_preceding_input();
    assert!(!pending_return.has_preceding_input());

    let mut state = ControlState {
        pending_return: Some(PendingReturn::Eof {
            code: 0,
            preceding_input: 2,
            observed_preceding_input: false,
        }),
        ..ControlState::default()
    };
    let mut queued = VecDeque::from([StdinEvent::Line("display-message -p LOST".to_owned())]);
    assert!(release_parked_queue_at_client_exit(&mut state, &mut queued));
    assert!(queued.is_empty());
    assert!(
        state
            .pending_return
            .as_ref()
            .is_some_and(|pending| !pending.has_preceding_input())
    );
}

#[test]
fn preparation_error_releases_eof_after_the_last_queued_input() {
    let invalid = "bind-key -T { set-environment -g BIND_CONTROL_REJECT_FORBIDDEN yes } F11 display-message -p forbidden";
    let mut pending_stdin = VecDeque::from([StdinEvent::Line(invalid.to_owned())]);
    let mut state = ControlState {
        pending_return: Some(PendingReturn::Eof {
            code: 0,
            preceding_input: 1,
            observed_preceding_input: false,
        }),
        ..ControlState::default()
    };

    let Some(StdinEvent::Line(line)) = pending_stdin.pop_front() else {
        panic!("missing retained input");
    };
    state
        .pending_return
        .as_mut()
        .expect("pending EOF")
        .consume_preceding_input();
    let ParsedLine::Commands(mut commands) = parse_line(&line, &BTreeMap::new(), &BTreeMap::new())
    else {
        panic!("invalid bind-key did not parse for preparation");
    };
    let prepared = PreparedCommand {
        invocation: commands.remove(0),
        canonical_name: Some("bind-key".to_owned()),
        alias_matched: false,
        result: PreparedCommandResult::Error(ServerError::CommandParse(
            "command bind-key: -T argument must be a string".to_owned(),
        )),
    };

    assert!(prepared_error(std::slice::from_ref(&prepared)).is_some());
    assert!(pending_stdin.is_empty());
    let mut prepared_return = None;
    assert!(matches!(
        settle_preparation_error_return(&mut prepared_return, &mut state.pending_return),
        Some(PendingReturn::Eof { code: 0, .. })
    ));
    assert!(state.pending_return.is_none());

    let mut prepared_return = Some(PendingReturn::Blank {
        code: 1,
        preceding_input: 1,
        observed_preceding_input: false,
    });
    assert!(
        settle_preparation_error_return(&mut prepared_return, &mut state.pending_return).is_none()
    );
    assert!(prepared_return.is_none());
    assert!(
        state
            .pending_return
            .as_ref()
            .is_some_and(PendingReturn::has_preceding_input)
    );

    state.pending_return = None;
    assert!(
        settle_preparation_error_return(&mut prepared_return, &mut state.pending_return).is_none()
    );
    assert!(state.pending_return.is_none());
}

#[test]
fn pending_return_waits_for_input_observed_before_it() {
    let mut pending_return = None;
    let mut pending_stdin = VecDeque::new();
    let mut writer = ControlWriter::new(Vec::new(), false);
    capture_pending_return(
        StdinEvent::Line("display-message -p queued".to_owned()),
        0,
        &mut pending_return,
        &mut pending_stdin,
        &mut writer,
    );
    capture_pending_return(
        StdinEvent::Line(String::new()),
        0,
        &mut pending_return,
        &mut pending_stdin,
        &mut writer,
    );

    let pending_return = pending_return.as_mut().expect("pending return");
    assert!(pending_return.has_preceding_input());
    pending_return.consume_preceding_input();
    assert!(!pending_return.has_preceding_input());
    pending_return.refresh_code_after_preceding_input(1);
    assert_eq!(pending_return.code(), 1);

    let Ok(mut in_flight) = PendingReturn::from_stdin(StdinEvent::Eof, 0, 0) else {
        panic!("EOF did not create a pending return");
    };
    in_flight.refresh_code_after_preceding_input(1);
    assert_eq!(in_flight.code(), 0);
    in_flight.observe_preceding_input();
    in_flight.refresh_code_after_preceding_input(1);
    assert_eq!(in_flight.code(), 1);
}

#[test]
fn authoritative_caller_detach_discards_return_observed_while_it_waits() {
    let mut state = ControlState {
        return_code: 1,
        ..ControlState::default()
    };
    let mut deferred_return = Some(PendingReturn::Eof {
        code: 1,
        preceding_input: 0,
        observed_preceding_input: false,
    });
    settle_deferred_return(true, &mut deferred_return, &mut state);
    assert!(deferred_return.is_none());
    assert!(state.pending_return.is_none());

    deferred_return = Some(PendingReturn::Eof {
        code: 1,
        preceding_input: 0,
        observed_preceding_input: false,
    });
    settle_deferred_return(false, &mut deferred_return, &mut state);
    assert!(deferred_return.is_none());
    assert_eq!(
        state.pending_return.as_ref().map(PendingReturn::code),
        Some(1)
    );
}

#[test]
fn legacy_nested_source_warning_defers_a_plain_error_without_config_error() {
    let mut writer = ControlWriter::new(Vec::new(), false);
    let frame = writer.begin_at(21, 1).unwrap();
    let mut state = ControlState::default();
    handle_protocol(
        ProtocolMessage::Event(zz_protocol::Event {
            sequence: 1,
            payload: EventPayload::ClientMessage {
                pane: None,
                kind: zz_protocol::ClientMessageKind::Warning,
                text: "No such file or directory: nested-a.conf\nNo such file or directory: nested-b.conf"
                    .to_owned(),
            },
        }),
        &mut state,
        &mut writer,
    )
    .unwrap();
    writer
        .response(
            &frame,
            CommandResponse::Success {
                request_id: 1,
                output: RawText::default(),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
        )
        .unwrap();

    let output = std::str::from_utf8(&writer.output).unwrap();
    assert!(!output.contains("%config-error"));
    let lines = output.lines().collect::<Vec<_>>();
    assert_eq!(lines[0], "%begin 21 1 1");
    assert_eq!(lines[1], "%end 21 1 1");
    assert!(lines[2].starts_with("%begin "));
    assert_eq!(lines[3], "No such file or directory: nested-a.conf");
    assert_eq!(lines[4], "No such file or directory: nested-b.conf");
    assert_eq!(
        lines[2].strip_prefix("%begin "),
        lines[5].strip_prefix("%error ")
    );
    assert_eq!(lines.len(), 6);
}

#[test]
fn typed_error_messages_use_standalone_frames_without_text_classification() {
    for text in [
        "No such file or directory: missing.conf",
        "Invalid argument: invalid.conf",
        "Cannot allocate memory: large.conf",
        "Pattern syntax error: invalid[.conf",
        "too many nested files",
        "/tmp/mux.conf:51: too many nested files",
        "Is a directory (os error 21): /tmp/a: b",
        "stream did not contain valid UTF-8: binary.conf",
        "No such file or directory: missing.conf\nstream did not contain valid UTF-8: binary.conf",
        "stream did not contain valid UTF-8: binary.conf\nNo such file or directory: missing.conf",
    ] {
        let mut writer = ControlWriter::new(Vec::new(), false);
        let mut state = ControlState::default();
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: EventPayload::ClientMessage {
                    pane: None,
                    kind: zz_protocol::ClientMessageKind::Error,
                    text: text.to_owned(),
                },
            }),
            &mut state,
            &mut writer,
        )
        .unwrap();

        let lines = std::str::from_utf8(&writer.output)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();
        let payload = text.lines().collect::<Vec<_>>();
        assert!(lines[0].starts_with("%begin "), "{text}");
        assert_eq!(&lines[1..=payload.len()], payload, "{text}");
        assert_eq!(
            lines[0].strip_prefix("%begin "),
            lines[payload.len() + 1].strip_prefix("%error "),
            "{text}"
        );
        assert_eq!(lines.len(), payload.len() + 2, "{text}");
    }
}

#[test]
fn config_errors_route_on_their_payload_type_and_not_on_their_prose() {
    for text in [
        "/tmp/mux.conf:51: unknown command: wibble",
        "skipped 1 unsupported tmux command: new-pane",
        "Is a directory (os error 21): relative-source.conf",
        "worker warning",
        "commande inconnue",
        "",
    ] {
        let mut writer = ControlWriter::new(Vec::new(), false);
        let mut state = ControlState::default();
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: EventPayload::ControlConfigError {
                    text: text.to_owned(),
                },
            }),
            &mut state,
            &mut writer,
        )
        .unwrap();
        assert_eq!(writer.output, format!("%config-error {text}\n").as_bytes());
        assert_eq!(state.return_code, 0);
        assert_eq!(completed_exit_code(0, &state), 0);
    }

    for text in [
        "/tmp/mux.conf:51: unknown command: wibble",
        "skipped 1 unsupported tmux command: new-pane",
        "stream did not contain valid UTF-8: binary.conf",
        "worker warning",
    ] {
        let mut writer = ControlWriter::new(Vec::new(), false);
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: EventPayload::ClientMessage {
                    pane: None,
                    kind: zz_protocol::ClientMessageKind::Warning,
                    text: text.to_owned(),
                },
            }),
            &mut ControlState::default(),
            &mut writer,
        )
        .unwrap();
        assert!(writer.output.is_empty(), "{text}");
    }
}

#[test]
fn separate_nested_source_warnings_defer_separate_error_frames() {
    let mut writer = ControlWriter::new(Vec::new(), false);
    let frame = writer.begin_at(17, 1).unwrap();
    writer
        .diagnostic_error_at(18, "No such file or directory: first.conf")
        .unwrap();
    writer
        .diagnostic_error_at(19, "No such file or directory: second.conf")
        .unwrap();
    writer
        .response(
            &frame,
            CommandResponse::Success {
                request_id: 1,
                output: RawText::default(),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
        )
        .unwrap();
    assert_eq!(
        writer.output,
        b"%begin 17 1 1\n%end 17 1 1\n%begin 18 2 1\nNo such file or directory: first.conf\n%error 18 2 1\n%begin 19 3 1\nNo such file or directory: second.conf\n%error 19 3 1\n"
    );
}
