use super::*;

#[cfg(unix)]
#[test]
fn direct_input_preserves_split_utf8_cr_and_trailing_eof_once() {
    let (read, write) = rustix::pipe::pipe().unwrap();
    let mut input = ControlInput::new(read);
    rustix::io::write(&write, b"display-message -p \xc3").unwrap();
    input.read();
    assert!(!input.has_event());
    assert!(input.event().is_none());
    rustix::io::write(&write, b"\xa9\r\n\nlast line").unwrap();
    input.read();
    assert!(matches!(input.event(), Some(StdinEvent::Line(line))
        if line == "display-message -p é\r"));
    assert!(matches!(input.event(), Some(StdinEvent::Line(line)) if line.is_empty()));
    assert!(input.event().is_none());
    drop(write);
    input.read();
    assert!(matches!(input.event(), Some(StdinEvent::Line(line)) if line == "last line"));
    assert!(matches!(input.event(), Some(StdinEvent::Eof)));
    assert!(input.event().is_none());
    assert!(!input.can_read());
}

#[cfg(unix)]
#[test]
fn direct_input_invalid_utf8_stops_before_later_lines_and_eof() {
    let (read, write) = rustix::pipe::pipe().unwrap();
    let mut input = ControlInput::new(read);
    rustix::io::write(&write, b"\xff\nnever execute\n").unwrap();
    input.read();
    assert!(matches!(input.event(), Some(StdinEvent::Error(_))));
    assert!(input.event().is_none());
    assert!(!input.has_event());
    assert!(!input.can_read());
}

#[cfg(unix)]
fn direct_control_fixture() -> (
    tempfile::TempDir,
    std::os::unix::net::UnixStream,
    ControlReceiver,
    OwnedFd,
) {
    use std::os::unix::net::UnixListener;
    let directory = tempfile::Builder::new()
        .prefix("zz-pump-")
        .tempdir_in("/tmp")
        .unwrap();
    let socket = directory.path().join("s");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        assert!(matches!(
            zz_protocol::read_protocol_message(&mut stream).unwrap(),
            ProtocolMessage::Hello(_)
        ));
        let capabilities = [
            zz_protocol::PANE_FRAME_CAPABILITY.to_owned(),
            zz_protocol::CONTROL_CAPABILITY.to_owned(),
        ];
        let welcome = ProtocolMessage::Welcome(zz_protocol::Welcome {
            protocol_version: zz_protocol::PROTOCOL_VERSION,
            server_id: 1,
            client_id: zz_protocol::ClientId(1),
            client_instance_id: zz_protocol::ClientInstanceId(1),
            caps: zz_protocol::Welcome::caps_from_strings(&capabilities),
            clock: 0,
            platform: zz_protocol::HostPlatform::default(),
            revision: zz_protocol::REVISION,
        });
        let initial = ProtocolMessage::Batch(
            zz_protocol::Batch::from_messages(
                1,
                [ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 1,
                    payload: EventPayload::ClientView(zz_protocol::ClientView::default()),
                })],
            )
            .unwrap(),
        );
        for message in [welcome, initial] {
            stream
                .write_all(&zz_protocol::encode_protocol_message(&message).unwrap())
                .unwrap();
        }
        stream
    });
    let client = Arc::new(InteractiveClient::connect_control(&socket).unwrap());
    let server = server.join().unwrap();
    let (input, writer) = rustix::pipe::pipe().unwrap();
    let receiver = ControlReceiver {
        source: ControlSource::Direct(DirectControl::with_input(client, input).unwrap()),
        pending: VecDeque::new(),
    };
    (directory, server, receiver, writer)
}

#[cfg(unix)]
#[test]
fn direct_pump_handles_ready_stdin_while_a_protocol_frame_is_incomplete() {
    let (_directory, mut server, mut receiver, input) = direct_control_fixture();
    assert!(
        matches!(receiver.receive(None).unwrap(), Some(MainEvent::Protocol(message))
        if matches!(*message, ProtocolMessage::Event(zz_protocol::Event { payload: EventPayload::ClientView(_), .. })))
    );
    let messages = [
        ProtocolMessage::Attach {
            session: "first".to_owned(),
        },
        ProtocolMessage::Attach {
            session: "second".to_owned(),
        },
    ];
    let frame = zz_protocol::encode_protocol_message(&ProtocolMessage::Batch(
        zz_protocol::Batch::from_messages(2, messages.clone()).unwrap(),
    ))
    .unwrap();
    server.write_all(&frame[..2]).unwrap();
    rustix::io::write(&input, b"ready input\n").unwrap();
    assert!(
        matches!(receiver.receive(Some(std::time::Duration::from_millis(20))).unwrap(),
        Some(MainEvent::Stdin(StdinEvent::Line(line))) if line == "ready input")
    );
    assert!(receiver.receive(None).unwrap().is_none());
    server.write_all(&frame[2..]).unwrap();
    rustix::io::write(&input, b"next input\n").unwrap();
    for expected in messages {
        assert!(
            matches!(receiver.receive(Some(std::time::Duration::from_millis(20))).unwrap(),
            Some(MainEvent::Protocol(message)) if *message == expected)
        );
    }
    assert!(
        matches!(receiver.receive(None).unwrap(), Some(MainEvent::Stdin(StdinEvent::Line(line)))
        if line == "next input")
    );
    drop(input);
    assert!(matches!(
        receiver
            .receive(Some(std::time::Duration::from_millis(20)))
            .unwrap(),
        Some(MainEvent::Stdin(StdinEvent::Eof))
    ));
}

#[cfg(unix)]
#[test]
fn direct_pump_polls_fresh_stdin_between_buffered_protocol_frames() {
    for buffered in [false, true] {
        let (_directory, mut server, mut receiver, input) = direct_control_fixture();
        assert!(matches!(
            receiver.receive(None).unwrap(),
            Some(MainEvent::Protocol(_))
        ));
        rustix::io::write(&input, b"first command\n").unwrap();
        assert!(
            matches!(receiver.receive(None).unwrap(), Some(MainEvent::Stdin(StdinEvent::Line(line)))
            if line == "first command")
        );
        let messages: Vec<_> = (0..4)
            .map(|index| ProtocolMessage::Attach {
                session: format!("frame{index}"),
            })
            .collect();
        let bytes: Vec<_> = messages
            .iter()
            .flat_map(|message| zz_protocol::encode_protocol_message(message).unwrap())
            .collect();
        server.write_all(&bytes).unwrap();
        let before = std::time::Instant::now();
        assert!(
            matches!(receiver.receive(Some(std::time::Duration::from_secs(1))).unwrap(),
            Some(MainEvent::Protocol(message)) if *message == messages[0])
        );
        assert!(before.elapsed() < std::time::Duration::from_millis(500));
        rustix::io::write(&input, b"newly ready\n").unwrap();
        let mut writer = ControlWriter::new(Vec::new(), false);
        let event = if buffered {
            Some(receive_buffered_control_event(&mut receiver, &mut writer).unwrap())
        } else {
            receiver.receive(None).unwrap()
        };
        assert!(
            matches!(event, Some(MainEvent::Stdin(StdinEvent::Line(line)))
            if line == "newly ready")
        );
        for expected in &messages[1..] {
            assert!(
                matches!(receiver.receive(None).unwrap(), Some(MainEvent::Protocol(message))
                if *message == *expected)
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn cached_stdin_keeps_fresh_protocol_priority_with_open_and_closed_guards() {
    for (buffered, block_open) in [(false, false), (false, true), (true, false), (true, true)] {
        let (_directory, mut server, mut receiver, input) = direct_control_fixture();
        assert!(matches!(
            receiver.receive(None).unwrap(),
            Some(MainEvent::Protocol(_))
        ));
        rustix::io::write(&input, b"first command\nsecond command\n").unwrap();
        assert!(matches!(
            receiver.receive(None).unwrap(),
            Some(MainEvent::Stdin(StdinEvent::Line(line))) if line == "first command"
        ));
        let ControlSource::Direct(direct) = &receiver.source else {
            unreachable!();
        };
        assert!(direct.input.has_event());
        assert!(!direct.prefer_stdin);
        let messages = [
            ProtocolMessage::CommandResponse(CommandResponse::Success {
                request_id: 1,
                output: "ready".into(),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }),
            ProtocolMessage::ExecExit(zz_protocol::ExecExit {
                server_id: 1,
                outcome: ExecOutcome::Ran,
            }),
        ];
        server
            .write_all(
                &zz_protocol::encode_protocol_message(&ProtocolMessage::Batch(
                    zz_protocol::Batch::from_messages(2, messages.clone()).unwrap(),
                ))
                .unwrap(),
            )
            .unwrap();
        let mut writer = ControlWriter::new(Vec::new(), false);
        if block_open {
            writer.begin_buffered_at(21, 1).unwrap();
        }
        let mut next = || {
            if buffered {
                Some(receive_buffered_control_event(&mut receiver, &mut writer).unwrap())
            } else {
                receiver.receive(None).unwrap()
            }
        };
        for expected in messages {
            assert!(
                matches!(next(), Some(MainEvent::Protocol(message)) if *message == expected),
                "buffered={buffered}, block_open={block_open}"
            );
        }
        assert!(
            matches!(next(), Some(MainEvent::Stdin(StdinEvent::Line(line)))
            if line == "second command")
        );
    }
}

#[cfg(unix)]
#[test]
fn an_open_guard_joins_a_kernel_ready_response_before_pending_exit_and_eof() {
    #[derive(Default)]
    struct Writes(Vec<Vec<u8>>);
    impl Write for Writes {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.push(bytes.to_vec());
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let (_directory, mut server, mut receiver, _input) = direct_control_fixture();
    assert!(matches!(
        receiver.receive(None).unwrap(),
        Some(MainEvent::Protocol(_))
    ));
    let response = ProtocolMessage::CommandResponse(CommandResponse::Success {
        request_id: 1,
        output: "body".into(),
        exit_code: 0,
        stderr: String::new(),
        stdout_claim: StdoutClaim::None,
    });
    let finished = ProtocolMessage::ExecExit(zz_protocol::ExecExit {
        server_id: 1,
        outcome: ExecOutcome::Ran,
    });
    let notification = ProtocolMessage::Attach {
        session: "after-exit".to_owned(),
    };
    server
        .write_all(
            &zz_protocol::encode_protocol_message(&ProtocolMessage::Batch(
                zz_protocol::Batch::from_messages(
                    2,
                    [response, finished.clone(), notification.clone()],
                )
                .unwrap(),
            ))
            .unwrap(),
        )
        .unwrap();
    server.shutdown(std::net::Shutdown::Write).unwrap();
    let mut writer = ControlWriter::new(io::BufWriter::new(Writes::default()), false);
    let frame = writer.begin_buffered_at(21, 1).unwrap();
    let MainEvent::Protocol(message) =
        receive_buffered_control_event(&mut receiver, &mut writer).unwrap()
    else {
        panic!("response expected");
    };
    let ProtocolMessage::CommandResponse(response) = *message else {
        panic!("response expected");
    };
    assert!(writer.output.get_ref().0.is_empty());
    writer.response(&frame, response).unwrap();
    assert_eq!(
        writer.output.get_ref().0,
        [b"%begin 21 1 1\nbody\n%end 21 1 1\n".to_vec()]
    );
    for expected in [finished, notification] {
        assert!(matches!(
            receive_buffered_control_event(&mut receiver, &mut writer).unwrap(),
            MainEvent::Protocol(message) if *message == expected
        ));
    }
    assert!(matches!(
        receive_buffered_control_event(&mut receiver, &mut writer).unwrap(),
        MainEvent::Disconnected
    ));
    assert_eq!(writer.output.get_ref().0.len(), 1);
}

#[cfg(unix)]
#[test]
fn an_empty_probe_continuation_receives_stdin_and_protocol_ready_at_flush() {
    struct FlushReady {
        bytes: Vec<u8>,
        flushes: usize,
        ready: Option<Box<dyn FnOnce()>>,
    }
    impl Write for FlushReady {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if let Some(ready) = self.ready.take() {
                ready();
            }
            Ok(())
        }
    }

    for (stdin_ready, block_open) in [(false, false), (true, false), (false, true), (true, true)] {
        let (_directory, mut server, mut receiver, input) = direct_control_fixture();
        assert!(matches!(
            receiver.receive(None).unwrap(),
            Some(MainEvent::Protocol(_))
        ));
        assert!(receiver.receive(None).unwrap().is_none());
        let message = ProtocolMessage::Attach {
            session: "after-flush".to_owned(),
        };
        let frame = zz_protocol::encode_protocol_message(&message).unwrap();
        let continuation = frame[2..].to_vec();
        let mut later_server = server.try_clone().unwrap();
        let ready_input = input.as_fd().try_clone_to_owned().unwrap();
        let mut writer = ControlWriter::new(
            FlushReady {
                bytes: Vec::new(),
                flushes: 0,
                ready: Some(Box::new(move || {
                    if stdin_ready {
                        server.write_all(&frame[..2]).unwrap();
                        rustix::io::write(&ready_input, b"after empty\n").unwrap();
                    } else {
                        server.write_all(&frame).unwrap();
                    }
                })),
            },
            false,
        );
        if block_open {
            writer.begin_buffered_at(21, 1).unwrap();
        }
        let event = receive_buffered_control_event(&mut receiver, &mut writer).unwrap();
        assert_eq!(writer.output.flushes, 1);
        assert_eq!(
            writer.output.bytes,
            if block_open {
                b"%begin 21 1 1\n".as_slice()
            } else {
                b"".as_slice()
            }
        );
        if stdin_ready {
            assert!(matches!(event, MainEvent::Stdin(StdinEvent::Line(line))
                if line == "after empty"));
            assert!(receiver.receive(None).unwrap().is_none());
            later_server.write_all(&continuation).unwrap();
            assert!(matches!(receive_control_event(&mut receiver).unwrap(),
                MainEvent::Protocol(packet) if *packet == message));
        } else {
            assert!(matches!(event, MainEvent::Protocol(packet) if *packet == message));
        }
    }
}

#[test]
fn pipelined_lines_preserve_order_and_stop_at_the_pending_return() {
    let pending = VecDeque::from([
        StdinEvent::Line("set-environment -g ORDER first".to_owned()),
        StdinEvent::Line("display-message -p $ORDER".to_owned()),
        StdinEvent::Line("set-environment -g ORDER after-return".to_owned()),
    ]);
    let mut submitted = 0;
    let mut lines = Vec::new();
    submit_pending_control_lines(&pending, &mut submitted, Some(2), |line| {
        lines.push(line.to_owned());
        Ok(())
    })
    .unwrap();
    submit_pending_control_lines(&pending, &mut submitted, Some(2), |line| {
        lines.push(line.to_owned());
        Ok(())
    })
    .unwrap();
    assert_eq!(
        lines,
        [
            "set-environment -g ORDER first",
            "display-message -p $ORDER"
        ]
    );
    assert_eq!(submitted, 2);
}

#[test]
fn pipelined_lines_bound_outstanding_requests_and_keep_ignored_input_in_order() {
    let pending = (0..40)
        .map(|index| StdinEvent::Line(format!("display-message -p {index}")))
        .collect();
    let mut submitted = 0;
    let mut lines = Vec::new();
    submit_pending_control_lines(&pending, &mut submitted, None, |line| {
        lines.push(line.to_owned());
        Ok(())
    })
    .unwrap();
    assert_eq!(lines.len(), 32);
    assert_eq!(lines.last().unwrap(), "display-message -p 31");
    let pending = VecDeque::from([
        StdinEvent::Line("# ignored".to_owned()),
        StdinEvent::Line("display-message -p later".to_owned()),
    ]);
    submitted = 0;
    submit_pending_control_lines(&pending, &mut submitted, None, |_| {
        panic!("later input must wait for the ignored line to be consumed")
    })
    .unwrap();
    assert_eq!(submitted, 0);
}

#[test]
fn parser_distinguishes_return_ignores_chains_and_errors() {
    assert_eq!(
        parse_line("", &BTreeMap::new(), &BTreeMap::new()),
        ParsedLine::Return
    );
    assert_eq!(
        parse_line("   ", &BTreeMap::new(), &BTreeMap::new()),
        ParsedLine::Ignore
    );
    assert_eq!(
        parse_line(" # ignored", &BTreeMap::new(), &BTreeMap::new()),
        ParsedLine::Ignore
    );
    let ParsedLine::Commands(commands) =
        parse_line("ls ; list-panes", &BTreeMap::new(), &BTreeMap::new())
    else {
        panic!("semicolon chain was not parsed");
    };
    assert_eq!(
        commands
            .iter()
            .map(|command| command.name.as_str())
            .collect::<Vec<_>>(),
        ["ls", "list-panes"]
    );
    let ParsedLine::Commands(commands) =
        parse_line("bogus-command", &BTreeMap::new(), &BTreeMap::new())
    else {
        panic!("unknown command was rejected before live preparation");
    };
    assert_eq!(commands[0].name, "bogus-command");
    let ParsedLine::Commands(commands) =
        parse_line("set 'oops", &BTreeMap::new(), &BTreeMap::new())
    else {
        panic!("open quote at EOF was rejected");
    };
    assert_eq!(commands[0].name, "set");
    assert_eq!(commands[0].args, ["oops"]);
    let ParsedLine::Commands(commands) = parse_line(
        "set-environment -g CONTROL_LITERAL $FOO",
        &BTreeMap::new(),
        &BTreeMap::new(),
    ) else {
        panic!("variable command was not parsed");
    };
    assert_eq!(
        commands[0].args,
        ["-g", "CONTROL_LITERAL", ""],
        "the pin's lexer expands an unset $NAME to nothing while the server parses the line"
    );
}

#[test]
fn prepared_response_matching_ignores_stale_request_ids() {
    let stale = ProtocolMessage::PreparedCommandList {
        request_id: 8,
        commands: Vec::new(),
    };
    assert!(matches!(
        match_prepared_response(stale, 9),
        Err(ProtocolMessage::PreparedCommandList { request_id: 8, .. })
    ));
    let command = PreparedCommand {
        invocation: CommandInvocation::new("list-sessions", [] as [&str; 0]),
        canonical_name: Some("list-sessions".to_owned()),
        alias_matched: false,
        result: PreparedCommandResult::Ready,
    };
    assert_eq!(
        match_prepared_response(
            ProtocolMessage::PreparedCommandList {
                request_id: 9,
                commands: vec![command.clone()],
            },
            9,
        )
        .unwrap(),
        [command]
    );
}

#[test]
fn expansion_answers_are_matched_by_kind_and_request_id() {
    let homes = PendingExpansion {
        variables: false,
        request_id: 9,
    };
    let variables = PendingExpansion {
        variables: true,
        request_id: 9,
    };
    let home_answer = ProtocolMessage::HomeDirectoryResponse {
        request_id: 9,
        homes: vec![Some("/server/home".to_owned())],
    };
    let variable_answer = ProtocolMessage::EnvironmentResponse {
        request_id: 9,
        values: vec![Some("EXPANDED".to_owned())],
    };
    assert!(homes.answers(&home_answer));
    assert!(!homes.answers(&variable_answer));
    assert!(variables.answers(&variable_answer));
    assert!(!variables.answers(&home_answer));
    assert!(!homes.answers(&ProtocolMessage::HomeDirectoryResponse {
        request_id: 8,
        homes: Vec::new(),
    }));
    assert!(!variables.answers(&ProtocolMessage::EnvironmentResponse {
        request_id: 8,
        values: Vec::new(),
    }));
    assert_eq!(
        expansion_answer(home_answer),
        [Some("/server/home".to_owned())]
    );
    assert_eq!(
        expansion_answer(variable_answer),
        [Some("EXPANDED".to_owned())]
    );
}

#[test]
fn control_lines_expand_variables_from_the_daemon_global_environment() {
    let variables = BTreeMap::from([("NOTIFY_ENV".to_owned(), "EXPANDED".to_owned())]);
    let ParsedLine::Commands(commands) = parse_line(
        "display-message -p \"$NOTIFY_ENV\" $NOTIFY_ENV '$NOTIFY_ENV' \"$NOPE_UNSET\"",
        &BTreeMap::new(),
        &variables,
    ) else {
        panic!("variable line was not parsed");
    };
    assert_eq!(
        commands[0].args,
        ["-p", "EXPANDED", "EXPANDED", "$NOTIFY_ENV", ""]
    );
}

#[test]
fn control_lines_expand_tildes_from_the_daemon_answer() {
    let homes = BTreeMap::from([
        (String::new(), "/server/home".to_owned()),
        ("alice".to_owned(), "/users/alice".to_owned()),
    ]);
    let ParsedLine::Commands(commands) = parse_line(
        "display-message -p ~ ~/x ~alice/y $UNSET",
        &homes,
        &BTreeMap::new(),
    ) else {
        panic!("tilde line was not parsed");
    };
    assert_eq!(
        commands[0].args,
        ["-p", "/server/home", "/server/home/x", "/users/alice/y", ""]
    );
    assert_eq!(
        parse_line("display-message -p ~nobody", &homes, &BTreeMap::new()),
        ParsedLine::Error("parse error: syntax error".to_owned())
    );
    let ParsedLine::Commands(commands) =
        parse_line("display-message -p '~'", &homes, &BTreeMap::new())
    else {
        panic!("single-quoted tilde was not parsed");
    };
    assert_eq!(commands[0].args, ["-p", "~"]);
}
