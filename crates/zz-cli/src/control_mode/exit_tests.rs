use super::*;

#[test]
fn detach_and_server_stop_keep_distinct_exit_signals() {
    let session = SessionId(7);
    let mut state = ControlState {
        attached_session: Some(session),
        ..ControlState::default()
    };
    let mut writer = ControlWriter::new(Vec::new(), false);
    assert_eq!(
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: EventPayload::detached_requested(session, None),
            }),
            &mut state,
            &mut writer,
        )
        .unwrap(),
        ExitSignal::Detached
    );
    assert_eq!(state.attached_session, None);

    let mut writer = ControlWriter::new(Vec::new(), false);
    assert_eq!(
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 2,
                payload: EventPayload::ServerStopping,
            }),
            &mut state,
            &mut writer,
        )
        .unwrap(),
        ExitSignal::Clean
    );
    writer
        .control_command_guard_at(3, "late child", false, 0)
        .unwrap();
    writer.emit_exit(None).unwrap();
    assert_eq!(
        writer.output,
        b"%exit\n%begin 3 1 0\nlate child\n%end 3 1 0\n"
    );
}

#[test]
fn logical_command_holds_server_exit_until_child_events_finish() {
    let mut state = ControlState::default();
    let mut writer = ControlWriter::new(Vec::new(), false);
    writer.hold_exit();
    writer
        .control_command_guard_at(1, "source child one", false, 0)
        .unwrap();
    writer
        .control_command_guard_at(2, "source child two", false, 0)
        .unwrap();
    assert_eq!(
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 3,
                payload: EventPayload::ServerStopping,
            }),
            &mut state,
            &mut writer,
        )
        .unwrap(),
        ExitSignal::Clean
    );
    assert!(!writer.output.ends_with(b"%exit\n"));
    writer
        .control_command_guard_at(4, "blocked child", false, 0)
        .unwrap();
    writer.notify(b"%sessions-changed").unwrap();
    writer.release_exit().unwrap();
    writer.emit_exit(None).unwrap();
    assert_eq!(
        writer.output,
        b"%begin 1 1 0\nsource child one\n%end 1 1 0\n\
          %begin 2 2 0\nsource child two\n%end 2 2 0\n\
          %begin 4 3 0\nblocked child\n%end 4 3 0\n\
          %sessions-changed\n%exit\n"
    );
}

#[test]
fn clean_control_exit_releases_only_the_initiating_alias() {
    let mut state = ControlState::default();
    let mut writer = ControlWriter::new(Vec::new(), false);
    writer.hold_exit();
    assert_eq!(
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: EventPayload::ControlExit {
                    reason: "unknown".to_owned(),
                },
            }),
            &mut state,
            &mut writer,
        )
        .unwrap(),
        ExitSignal::None
    );
    writer
        .control_command_guard_at(2, "kill child", false, 0)
        .unwrap();
    assert_eq!(
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 3,
                payload: EventPayload::ControlExit {
                    reason: String::new(),
                },
            }),
            &mut state,
            &mut writer,
        )
        .unwrap(),
        ExitSignal::Clean
    );
    assert_eq!(
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 4,
                payload: EventPayload::ServerStopping,
            }),
            &mut state,
            &mut writer,
        )
        .unwrap(),
        ExitSignal::Clean
    );
    writer
        .control_command_guard_at(5, "late child", false, 0)
        .unwrap();
    assert_eq!(
        writer.output,
        b"%begin 2 1 0\nkill child\n%end 2 1 0\n%exit\n\
          %begin 5 2 0\nlate child\n%end 5 2 0\n"
    );
}

#[test]
fn open_guard_holds_server_exit_until_deferred_children_finish() {
    let mut state = ControlState::default();
    let mut writer = ControlWriter::new(Vec::new(), false);
    let frame = writer.begin_at(4, 1).unwrap();
    assert_eq!(
        handle_protocol(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 3,
                payload: EventPayload::ServerStopping,
            }),
            &mut state,
            &mut writer,
        )
        .unwrap(),
        ExitSignal::Clean
    );
    assert_eq!(writer.output, b"%begin 4 1 1\n");
    writer
        .control_command_guard_at(5, "child", false, 0)
        .unwrap();
    writer.emit_exit(None).unwrap();
    writer.end(&frame, false).unwrap();
    writer.emit_exit(None).unwrap();
    assert_eq!(
        writer.output,
        b"%begin 4 1 1\n%end 4 1 1\n%begin 5 2 0\nchild\n%end 5 2 0\n%exit\n"
    );
}

#[test]
fn subscription_changes_render_the_three_pin_shapes_byte_exact() {
    let session = SessionId(1);
    let window = WindowId(2);
    let pane = zz_protocol::PaneId(3);
    assert_eq!(
        render_subscription_changed(
            "pane-watch",
            session,
            Some(window),
            Some(4),
            Some(pane),
            "one\ntwo",
        ),
        "%subscription-changed pane-watch $1 @2 4 %3 : one\ntwo"
    );
    assert_eq!(
        render_subscription_changed(
            "window-watch",
            session,
            Some(window),
            Some(4),
            None,
            "value",
        ),
        "%subscription-changed window-watch $1 @2 4 - : value"
    );
    assert_eq!(
        render_subscription_changed("session-watch", session, None, None, None, "value"),
        "%subscription-changed session-watch $1 - - - : value"
    );
}

#[test]
fn exit_drain_keeps_wait_exit_acknowledgements() {
    let (sender, receiver) = mpsc::sync_channel(32);
    sender
        .send(MainEvent::Stdin(StdinEvent::Line(String::new())))
        .unwrap();
    sender.send(MainEvent::Stdin(StdinEvent::Eof)).unwrap();
    drop(sender);
    let mut receiver = ControlReceiver::new(receiver);
    let mut pending = VecDeque::new();
    let mut state = ControlState::default();
    let mut output = ControlWriter::new(Vec::new(), false);
    drain_before_exit(&mut receiver, &mut state, &mut output, &mut pending).unwrap();
    assert_eq!(pending.len(), 2);
    wait_for_exit_input(&mut receiver, &mut pending);
    assert!(matches!(pending.pop_front(), Some(StdinEvent::Eof)));
}

#[test]
fn wait_exit_releases_on_empty_line_and_eof() {
    let (_sender, receiver) = mpsc::sync_channel(32);
    let mut receiver = ControlReceiver::new(receiver);
    let mut pending = VecDeque::from([
        StdinEvent::Line("keep draining".to_owned()),
        StdinEvent::Line(String::new()),
    ]);
    wait_for_exit_input(&mut receiver, &mut pending);
    assert!(pending.is_empty());

    let mut pending = VecDeque::from([StdinEvent::Eof]);
    wait_for_exit_input(&mut receiver, &mut pending);
    assert!(pending.is_empty());
}

#[test]
fn dropping_a_double_writer_without_exit_still_terminates_the_dcs() {
    let output = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    struct Shared(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl Write for Shared {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buffer);
            Ok(buffer.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = ControlWriter::new(Shared(std::sync::Arc::clone(&output)), true);
    writer.start().unwrap();
    drop(writer);
    assert_eq!(*output.lock().unwrap(), b"\x1bP1000p\x1b\\");
}
