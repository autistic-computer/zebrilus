use super::*;

#[test]
fn double_control_wraps_the_stream() {
    let mut writer = ControlWriter::new(Vec::new(), true);
    writer.start().unwrap();
    writer.emit_exit(None).unwrap();
    writer.finish().unwrap();
    assert_eq!(writer.output, b"\x1bP1000p%exit\n\x1b\\");
}

#[test]
fn notifications_defer_while_a_block_is_open() {
    let mut writer = ControlWriter::new(Vec::new(), false);
    writer.notify(b"%sessions-changed").unwrap();
    let frame = writer.begin_at(21, 1).unwrap();
    writer.notify(b"%window-add @3").unwrap();
    writer.notify(b"%window-renamed @3 shell").unwrap();
    writer
        .response(
            &frame,
            CommandResponse::Success {
                request_id: 5,
                output: "body\n".into(),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
        )
        .unwrap();
    writer.notify(b"%session-renamed $1 dev").unwrap();
    assert_eq!(
        writer.output,
        b"%sessions-changed\n%begin 21 1 1\nbody\n%end 21 1 1\n%window-add @3\n%window-renamed @3 shell\n%session-renamed $1 dev\n"
    );
}

#[test]
fn blank_and_eof_discard_queued_and_future_pane_output_only() {
    for stdin in [StdinEvent::Line(String::new()), StdinEvent::Eof] {
        let pane = zz_protocol::PaneId(7);
        let mut state = ControlState::default();
        let mut writer = ControlWriter::new(Vec::new(), false);
        let event =
            |sequence, payload| ProtocolMessage::Event(zz_protocol::Event { sequence, payload });

        handle_protocol(
            event(
                1,
                EventPayload::PaneOutput {
                    pane,
                    bytes: b"before".to_vec(),
                },
            ),
            &mut state,
            &mut writer,
        )
        .unwrap();
        let frame = writer.begin_at(21, 1).unwrap();
        for (sequence, payload) in [
            (
                2,
                EventPayload::PaneOutput {
                    pane,
                    bytes: b"queued".to_vec(),
                },
            ),
            (
                3,
                EventPayload::PaneOutputAged {
                    pane,
                    age_ms: 42,
                    bytes: b"queued-aged".to_vec(),
                },
            ),
            (4, EventPayload::PaneOutputState { pane, paused: true }),
        ] {
            handle_protocol(event(sequence, payload), &mut state, &mut writer).unwrap();
        }
        writer.notify(b"%window-add @3").unwrap();
        writer.diagnostic_error_at(22, "diagnostic").unwrap();
        writer
            .control_command_guard_at(23, "guard", false, 0)
            .unwrap();
        writer
            .control_source_file(ControlSourceFileEvent::ReadError("source".to_owned()))
            .unwrap();
        writer.control_command_output("command").unwrap();

        let mut pending_return = None;
        let mut pending_stdin = VecDeque::new();
        capture_pending_return(
            stdin,
            0,
            &mut pending_return,
            &mut pending_stdin,
            &mut writer,
        );
        for (sequence, payload) in [
            (
                5,
                EventPayload::PaneOutput {
                    pane,
                    bytes: b"after".to_vec(),
                },
            ),
            (
                6,
                EventPayload::PaneOutputAged {
                    pane,
                    age_ms: 84,
                    bytes: b"after-aged".to_vec(),
                },
            ),
            (
                7,
                EventPayload::PaneOutputState {
                    pane,
                    paused: false,
                },
            ),
        ] {
            handle_protocol(event(sequence, payload), &mut state, &mut writer).unwrap();
        }
        writer
            .response(
                &frame,
                CommandResponse::Success {
                    request_id: 1,
                    output: "body\n".into(),
                    exit_code: 0,
                    stderr: String::new(),
                    stdout_claim: StdoutClaim::None,
                },
            )
            .unwrap();
        writer.emit_exit(None).unwrap();

        assert!(pending_return.is_some());
        assert!(pending_stdin.is_empty());
        assert_eq!(
            writer.output,
            b"%output %7 before\n\
              %begin 21 1 1\nbody\n%end 21 1 1\n\
              %pause %7\n%window-add @3\n\
              %begin 22 2 1\ndiagnostic\n%error 22 2 1\n\
              %begin 23 3 0\nguard\n%end 23 3 0\n\
              source\ncommand\n%continue %7\n%exit\n"
        );
    }
}

#[test]
fn input_error_keeps_pane_output_enabled() {
    let mut writer = ControlWriter::new(Vec::new(), false);
    let frame = writer.begin_at(21, 1).unwrap();
    writer.pane_output(b"%output %7 queued").unwrap();
    let mut pending_return = None;
    let mut pending_stdin = VecDeque::new();
    capture_pending_return(
        StdinEvent::Error("input failed".to_owned()),
        0,
        &mut pending_return,
        &mut pending_stdin,
        &mut writer,
    );
    writer
        .pane_output(b"%extended-output %7 42 : after")
        .unwrap();
    writer.end(&frame, false).unwrap();

    assert!(matches!(
        pending_return,
        Some(PendingReturn::InputError { .. })
    ));
    assert_eq!(
        writer.output,
        b"%begin 21 1 1\n%end 21 1 1\n\
          %output %7 queued\n%extended-output %7 42 : after\n"
    );
}

#[test]
fn control_command_output_follows_the_guard_raw_and_command_mode_stays_inside() {
    let mut writer = ControlWriter::new(Vec::new(), false);
    let mut state = ControlState::default();
    let shell = writer.begin_at(21, 1).unwrap();
    writer.notify(b"%window-renamed @3 shell").unwrap();
    handle_protocol(
        ProtocolMessage::Event(zz_protocol::Event {
            sequence: 1,
            payload: EventPayload::ControlCommandOutput {
                output: "child output\n%begin-fake\n%exit\n'exit 3' returned 3\npartial".to_owned(),
            },
        }),
        &mut state,
        &mut writer,
    )
    .unwrap();
    writer
        .response(
            &shell,
            CommandResponse::Success {
                request_id: 5,
                output: RawText::default(),
                exit_code: 3,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
        )
        .unwrap();
    let command_mode = writer.begin_at(22, 1).unwrap();
    writer
        .response(
            &command_mode,
            CommandResponse::Success {
                request_id: 6,
                output: "command mode".into(),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
        )
        .unwrap();
    assert_eq!(
        writer.output,
        b"%begin 21 1 1\n%end 21 1 1\n%window-renamed @3 shell\n\
          child output\n%begin-fake\n%exit\n'exit 3' returned 3\npartial\n\
          %begin 22 2 1\ncommand mode\n%end 22 2 1\n"
    );
}

#[test]
fn startup_config_causes_wait_for_the_open_block_and_prefix_each_element_once() {
    let mut writer = ControlWriter::new(Vec::new(), false);
    let mut state = ControlState::default();
    let event = |sequence, causes| {
        ProtocolMessage::Event(zz_protocol::Event {
            sequence,
            payload: EventPayload::StartupConfigCauses { causes },
        })
    };

    assert_eq!(
        handle_protocol(
            event(1, vec!["first\ncontinued".to_owned(), "second".to_owned()],),
            &mut state,
            &mut writer,
        )
        .unwrap(),
        ExitSignal::None
    );
    let frame = writer.begin_at(21, 1).unwrap();
    writer.notify(b"%window-add @3").unwrap();
    assert_eq!(
        handle_protocol(
            event(2, vec!["inside\nstill inside".to_owned()]),
            &mut state,
            &mut writer,
        )
        .unwrap(),
        ExitSignal::None
    );
    writer.notify(b"%session-changed $0 s").unwrap();
    writer
        .response(
            &frame,
            CommandResponse::Success {
                request_id: 5,
                output: "body\n".into(),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            },
        )
        .unwrap();

    assert_eq!(
        writer.output,
        b"%config-error first\ncontinued\n%config-error second\n\
          %begin 21 1 1\nbody\n%end 21 1 1\n%window-add @3\n\
          %session-changed $0 s\n%config-error inside\nstill inside\n"
    );
}

#[test]
fn window_notifications_use_live_membership_instead_of_the_event_session() {
    let mut state = layout_notification_state();
    let variables = BTreeMap::from([
        ("hook_session".to_owned(), SessionId(2).to_string()),
        ("hook_window".to_owned(), WindowId(3).to_string()),
        ("hook_window_name".to_owned(), "shell".to_owned()),
    ]);
    for (hook, expected) in [
        ("window-linked", "%window-add @3"),
        ("window-unlinked", "%window-close @3"),
        ("window-renamed", "%window-renamed @3 shell"),
    ] {
        assert_eq!(
            render_hook(&state, hook, &variables).as_deref(),
            Some(expected)
        );
    }
    state.snapshot.sessions[0].windows.clear();
    for (hook, expected) in [
        ("window-linked", "%unlinked-window-add @3"),
        ("window-unlinked", "%unlinked-window-close @3"),
        ("window-renamed", "%unlinked-window-renamed @3 shell"),
    ] {
        assert_eq!(
            render_hook(&state, hook, &variables).as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn layout_notifications_ignore_windows_only_linked_to_another_session() {
    let mut state = layout_notification_state();
    state.attached_session = Some(SessionId(2));
    let variables = BTreeMap::from([("hook_window".to_owned(), "@3".to_owned())]);
    assert!(render_hook(&state, "window-layout-changed", &variables).is_none());
}

#[test]
fn output_escaping_preserves_del_and_raw_eight_bit_bytes() {
    assert_eq!(
        render_pane_output(zz_protocol::PaneId(7), &[0, 0x1f, b'\\', 0x7f, 0x80, 0xff]),
        b"%output %7 \\000\\037\\134\x7f\x80\xff"
    );
    assert_eq!(
        render_pane_output_aged(
            zz_protocol::PaneId(7),
            42,
            &[0, 0x1f, b'\\', 0x7f, 0x80, 0xff]
        ),
        b"%extended-output %7 42 : \\000\\037\\134\x7f\x80\xff"
    );
    let mut writer = ControlWriter::new(Vec::new(), false);
    let frame = writer.begin_at(22, 1).unwrap();
    writer.notify(&[b'%', 0xff]).unwrap();
    writer.end(&frame, false).unwrap();
    assert_eq!(writer.output, b"%begin 22 1 1\n%end 22 1 1\n%\xff\n");
}

#[test]
fn pane_output_state_and_flags_follow_the_control_event_channel() {
    let pane = zz_protocol::PaneId(7);
    let mut state = ControlState::default();
    let mut writer = ControlWriter::new(Vec::new(), false);
    for payload in [
        EventPayload::PaneOutputState { pane, paused: true },
        EventPayload::PaneOutputState {
            pane,
            paused: false,
        },
        EventPayload::ControlFlags {
            wait_exit: true,
            pause_after_ms: Some(1000),
            no_output: false,
            new_layouts: true,
        },
    ] {
        assert_eq!(
            handle_protocol(
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 1,
                    payload,
                }),
                &mut state,
                &mut writer,
            )
            .unwrap(),
            ExitSignal::None
        );
    }
    assert!(state.wait_exit);
    assert!(state.new_layouts);
    assert_eq!(writer.output, b"%pause %7\n%continue %7\n");
}

fn layout_notification_state() -> ControlState {
    let pane = zz_protocol::PaneId(5);
    let window = zz_protocol::WindowSnapshot {
        id: WindowId(3),
        index: 0,
        name: "shell".to_owned(),
        automatic_rename: true,
        active_pane: pane,
        zoomed_pane: Some(pane),
        layout: zz_protocol::LayoutNode::Pane(pane),
        panes: BTreeMap::from([(
            pane,
            zz_protocol::PaneSnapshot {
                id: pane,
                title: "shell".to_owned(),
                kind: zz_protocol::PaneKindSnapshot::Terminal,
                synchronized_input: false,
                bell: true,
                dead: false,
                dead_status: None,
                border_colour: None,
                active_border_colour: None,
                border_status_text: String::new(),
                mode: None,
                status: None,
                summary: zz_protocol::PaneSummary::default(),
            },
        )]),
        layout_dump: "abcd,80x24,0,0,5".to_owned(),
        visible_layout_dump: "ef01,80x24,0,0,5".to_owned(),
        status_label: String::new(),
        activity: false,
        silence: false,
        pane_border_status: zz_protocol::PaneBorderStatus::Off,
        pane_border_lines: zz_protocol::PaneBorderLines::Single,
        pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
        pane_order: Vec::new(),
        pane_z_order: Vec::new(),
        floating: Vec::new(),
        modal: None,
        sx: 0,
        sy: 0,
        pane_base_index: 0,
    };
    let mut state = ControlState::default();
    state.attach(
        SessionId(1),
        MuxSnapshot {
            generation: 1,
            sessions: vec![zz_protocol::SessionSnapshot {
                id: SessionId(1),
                name: "work".to_owned(),
                active_window: window.id,
                windows: vec![window],
                viewers: vec![zz_protocol::SessionViewer {
                    name: "device-9".to_owned(),
                    window: WindowId(3),
                    is_self: true,
                }],
            }],
            focused_window: Some(WindowId(3)),
        },
    );
    state.last_windows.insert(SessionId(1), WindowId(3));
    state
}

#[test]
fn layout_notifications_use_snapshot_dumps_and_raw_flag_order() {
    let state = layout_notification_state();
    let variables = BTreeMap::from([("hook_window".to_owned(), "@3".to_owned())]);
    assert_eq!(
        render_hook(&state, "window-layout-changed", &variables).as_deref(),
        Some("%layout-change @3 abcd,80x24,0,0,5 ef01,80x24,0,0,5 !*-Z")
    );
}

#[test]
fn layout_notifications_print_v1_until_the_client_sets_new_layouts() {
    let mut state = layout_notification_state();
    let layout = r#"{"V":2,"L":{"t":"h","w":80,"h":24,"x":0,"y":0,"c":[{"t":"p","w":40,"h":24,"x":0,"y":0,"l":0,"i":0,"I":"%0"},{"t":"p","w":39,"h":24,"x":41,"y":0,"a":true,"i":1,"I":"%5"}]}}"#;
    let visible = r#"{"V":2,"L":{"t":"p","w":80,"h":24,"x":0,"y":0,"a":true,"i":1,"I":"%5"}}"#;
    let window = &mut state.snapshot.sessions[0].windows[0];
    window.layout_dump = layout.to_owned();
    window.visible_layout_dump = visible.to_owned();
    let variables = BTreeMap::from([("hook_window".to_owned(), "@3".to_owned())]);
    assert_eq!(
        render_hook(&state, "window-layout-changed", &variables).as_deref(),
        Some("%layout-change @3 8207,80x24,0,0{40x24,0,0,0,39x24,41,0,5} b262,80x24,0,0,5 !*-Z")
    );
    state.new_layouts = true;
    assert_eq!(
        render_hook(&state, "window-layout-changed", &variables),
        Some(format!("%layout-change @3 {layout} {visible} !*-Z"))
    );
}

#[test]
fn layout_notifications_reduce_each_compact_flag_change() {
    for (kind, expected) in [
        ("activity", "#"),
        ("bell", "!"),
        ("silence", "~"),
        ("zoom", "Z"),
        ("last", "-"),
        ("current", "*"),
    ] {
        let mut state = layout_notification_state();
        let mut before = state.tree.clone();
        let session = &mut before.sessions[0];
        session.active_window = if kind == "last" {
            WindowId(3)
        } else {
            WindowId(7)
        };
        let window = &mut session.windows[0];
        window.activity = false;
        window.silence = false;
        window.zoomed_pane = None;
        window.panes.get_mut(&zz_protocol::PaneId(5)).unwrap().bell = false;
        state.last_windows.clear();
        state.attach(SessionId(1), before.clone());
        state.last_windows.clear();
        let mut after = before.clone();
        after.generation += 1;
        let session = &mut after.sessions[0];
        let window = &mut session.windows[0];
        match kind {
            "activity" => window.activity = true,
            "bell" => window.panes.get_mut(&zz_protocol::PaneId(5)).unwrap().bell = true,
            "silence" => window.silence = true,
            "zoom" => window.zoomed_pane = Some(zz_protocol::PaneId(5)),
            "last" => session.active_window = WindowId(7),
            "current" => session.active_window = WindowId(3),
            _ => unreachable!(),
        }
        let delta = zz_protocol::TreeDelta::between(&before, &after);
        let mut output = ControlWriter::new(Vec::new(), false);
        for payload in [
            EventPayload::TreeDelta(delta),
            EventPayload::HookEvent {
                name: "window-layout-changed".to_owned(),
                variables: BTreeMap::from([("hook_window".to_owned(), "@3".to_owned())]),
            },
        ] {
            let encoded =
                zz_protocol::encode_protocol_message(&ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 1,
                    payload,
                }))
                .unwrap();
            let message = zz_protocol::decode_protocol_frame(&encoded).unwrap();
            handle_protocol(message, &mut state, &mut output).unwrap();
        }
        assert_eq!(
            std::str::from_utf8(&output.output).unwrap(),
            format!("%layout-change @3 abcd,80x24,0,0,5 ef01,80x24,0,0,5 {expected}\n"),
            "{kind}",
        );
    }
}

#[test]
fn an_unzoom_step_renders_the_layout_it_fired_with() {
    let mut state = layout_notification_state();
    let mut writer = ControlWriter::new(Vec::new(), false);
    for variables in [
        BTreeMap::from([
            ("hook_window".to_owned(), "@3".to_owned()),
            (
                zz_protocol::UNZOOMED_LAYOUT_VARIABLE.to_owned(),
                "1234,80x24,0,0,5".to_owned(),
            ),
        ]),
        BTreeMap::from([("hook_window".to_owned(), "@3".to_owned())]),
    ] {
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: EventPayload::HookEvent {
                    name: "window-layout-changed".to_owned(),
                    variables,
                },
            }),
            &mut state,
            &mut writer,
        )
        .unwrap();
    }
    assert_eq!(
        std::str::from_utf8(&writer.output).unwrap(),
        "%layout-change @3 1234,80x24,0,0,5 1234,80x24,0,0,5 !*-\n\
         %layout-change @3 abcd,80x24,0,0,5 ef01,80x24,0,0,5 !*-Z\n"
    );
}

#[test]
fn a_blank_line_closes_notifications_and_end_of_file_keeps_them() {
    for (stdin, closed) in [
        (StdinEvent::Line(String::new()), true),
        (StdinEvent::Eof, false),
    ] {
        let mut state = layout_notification_state();
        let mut writer = ControlWriter::new(Vec::new(), false);
        let mut pending_return = None;
        let mut pending_stdin = VecDeque::new();
        capture_pending_return(
            stdin,
            0,
            &mut pending_return,
            &mut pending_stdin,
            &mut writer,
        );
        let mut snapshot = state.snapshot.clone();
        let window = &mut snapshot.sessions[0].windows[0];
        window.layout_dump = "aafd,120x40,0,0,0".to_owned();
        window.visible_layout_dump = "aafd,120x40,0,0,0".to_owned();
        for (sequence, payload) in [
            (1, EventPayload::Snapshot(snapshot.clone())),
            (
                2,
                EventPayload::HookEvent {
                    name: "window-layout-changed".to_owned(),
                    variables: BTreeMap::from([("hook_window".to_owned(), "@3".to_owned())]),
                },
            ),
            (
                3,
                EventPayload::HookEvent {
                    name: "window-renamed".to_owned(),
                    variables: BTreeMap::from([
                        ("hook_window".to_owned(), "@3".to_owned()),
                        ("hook_window_name".to_owned(), "shell".to_owned()),
                        ("hook_session".to_owned(), "$1".to_owned()),
                    ]),
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
        assert!(pending_return.is_some());
        assert_eq!(
            state.snapshot.sessions[0].windows[0].layout_dump,
            "aafd,120x40,0,0,0"
        );
        let expected: &[u8] = if closed {
            b""
        } else {
            b"%layout-change @3 aafd,120x40,0,0,0 aafd,120x40,0,0,0 !*-Z\n%window-renamed @3 shell\n"
        };
        assert_eq!(writer.output, expected);
    }
}

#[test]
fn message_escaping_is_separate_from_output_escaping() {
    assert_eq!(
        render_message("a\\b\t\n\r\u{7}\u{1b}é"),
        b"a\\b\t\n\\r\\a\\033\xc3\xa9"
    );
}

#[test]
fn daemon_source_read_failures_keep_their_own_channel() {
    let root = std::env::temp_dir();
    for text in [
        "No such file or directory: /tmp/mux.conf".to_owned(),
        "Invalid argument: /tmp/mux.conf".to_owned(),
        "Cannot allocate memory: /tmp/mux.conf".to_owned(),
        "Pattern syntax error: /tmp/[".to_owned(),
        "too many nested files".to_owned(),
        format!(
            "Is a directory (os error 21): {}: b",
            root.join("a").display()
        ),
        format!(
            "stream did not contain valid UTF-8: {}",
            root.join("binary.conf").display()
        ),
    ] {
        assert!(is_source_error_message(&text), "{text}");
    }
    assert!(!is_source_error_message(
        "/tmp/mux.conf:51: too many nested files"
    ));
    for text in [
        "stream did not contain valid UTF-8: binary.conf",
        "worker warning (os error 21)",
        "No such file or directory: missing.conf\nstream did not contain valid UTF-8: binary.conf",
    ] {
        assert!(!is_source_error_message(text), "{text}");
    }
}

#[test]
fn ready_control_response_shares_a_flush_and_waiting_exposes_begin() {
    struct Recorded {
        bytes: Vec<u8>,
        flushes: usize,
        flushed: Option<mpsc::Sender<()>>,
    }
    impl Write for Recorded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if let Some(flushed) = self.flushed.take() {
                flushed.send(()).unwrap();
            }
            Ok(())
        }
    }
    let mut writer = ControlWriter::new(
        Recorded {
            bytes: Vec::new(),
            flushes: 0,
            flushed: None,
        },
        false,
    );
    let frame = writer.begin_buffered_at(21, 1).unwrap();
    let (sender, receiver) = mpsc::channel();
    sender
        .send(MainEvent::Protocol(Box::new(
            ProtocolMessage::CommandResponse(CommandResponse::Success {
                request_id: 1,
                output: RawText::from("body"),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }),
        )))
        .unwrap();
    let mut receiver = ControlReceiver::new(receiver);
    let MainEvent::Protocol(message) =
        receive_buffered_control_event(&mut receiver, &mut writer).unwrap()
    else {
        panic!("response expected");
    };
    let ProtocolMessage::CommandResponse(response) = *message else {
        panic!("response expected");
    };
    assert_eq!(writer.output.flushes, 0);
    writer.response(&frame, response).unwrap();
    assert_eq!(writer.output.flushes, 1);
    assert_eq!(writer.output.bytes, b"%begin 21 1 1\nbody\n%end 21 1 1\n");

    let (flushed, flush_receiver) = mpsc::channel();
    writer.output.flushed = Some(flushed);
    writer.begin_buffered_at(22, 1).unwrap();
    let producer = thread::spawn(move || {
        flush_receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        sender.send(MainEvent::Disconnected).unwrap();
    });
    assert!(matches!(
        receive_buffered_control_event(&mut receiver, &mut writer).unwrap(),
        MainEvent::Disconnected
    ));
    producer.join().unwrap();
    assert_eq!(writer.output.flushes, 2);
    assert!(writer.output.bytes.ends_with(b"%begin 22 2 1\n"));
}
