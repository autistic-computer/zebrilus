use super::test_support::{drain, event, hello, snapshot_with};
use super::*;
use zz_protocol::{ClientId, PROTOCOL_VERSION};

#[test]
fn timed_messages_carry_their_identity_and_the_clear_arrives_on_its_own() {
    let mut core = ClientCore::new();
    core.handle_message(event(EventPayload::TimedClientMessage {
        pane: None,
        kind: ClientMessageKind::Info,
        text: "timed".to_owned(),
        duration_ms: 750,
        message_id: 12,
    }));
    core.handle_message(event(EventPayload::ClientMessage {
        pane: None,
        kind: ClientMessageKind::Warning,
        text: "untimed".to_owned(),
    }));
    core.handle_message(event(EventPayload::TimedClientMessageCleared {
        message_id: 12,
    }));
    assert_eq!(
        drain(&mut core),
        vec![
            CoreEvent::ClientMessage {
                pane: None,
                kind: ClientMessageKind::Info,
                text: "timed".to_owned(),
                duration_ms: Some(750),
                message_id: Some(12),
            },
            CoreEvent::ClientMessage {
                pane: None,
                kind: ClientMessageKind::Warning,
                text: "untimed".to_owned(),
                duration_ms: None,
                message_id: None,
            },
            CoreEvent::ClientMessageCleared { message_id: 12 },
        ]
    );
}

#[test]
fn prefix_cancel_ack_passes_through_with_its_request_id() {
    let mut core = ClientCore::new();
    core.handle_message(event(EventPayload::PrefixCancelled { request_id: 73 }));
    assert_eq!(
        drain(&mut core),
        vec![CoreEvent::PrefixCancelled { request_id: 73 }]
    );
}

#[test]
fn detached_reason_is_retained_without_changing_the_shell_event_shape() {
    let session = SessionId(7);
    let mut core = ClientCore::new();
    core.handle_message(event(EventPayload::detached_session_destroyed(session)));
    assert_eq!(
        drain(&mut core),
        vec![CoreEvent::Detached {
            session,
            by: None,
            action: ClientExitAction::Exit
        }]
    );
    assert!(core.last_detach_was_session_destroyed());
    assert!(!core.last_detach_was_server_stopping());
}

#[test]
fn copy_generation_survives_reattach() {
    let session = SessionId(7);
    let pane = PaneId(3);
    let copy = || {
        event(EventPayload::Clipboard {
            pane,
            request_id: 0,
            target: ClipboardTarget::Clipboard,
            text: "copied".to_owned(),
            producer: ClipboardProducer::Server,
        })
    };
    let attached = || ProtocolMessage::Attached {
        session,
        snapshot: snapshot_with(&[pane]),
        read_only: false,
        client_flags: String::new(),
    };
    let mut core = ClientCore::new();
    core.handle_message(attached());
    core.handle_message(copy());
    assert_eq!(core.copy_generation(pane), 1);

    core.handle_message(attached());
    assert_eq!(core.copy_generation(pane), 1);

    core.handle_message(copy());
    assert_eq!(core.copy_generation(pane), 2);
}

#[test]
fn attachment_options_track_the_daemon_and_survive_detach_for_reconnect() {
    let session = SessionId(7);
    let mut core = ClientCore::new();
    core.handle_message(ProtocolMessage::Attached {
        session,
        snapshot: snapshot_with(&[]),
        read_only: false,
        client_flags: "no-detach-on-destroy".to_owned(),
    });
    assert!(!core.attached_read_only());
    assert_eq!(core.attached_client_flags(), "no-detach-on-destroy");

    core.handle_message(ProtocolMessage::Attached {
        session,
        snapshot: snapshot_with(&[]),
        read_only: true,
        client_flags: "ignore-size,no-detach-on-destroy".to_owned(),
    });
    assert!(core.attached_read_only());
    assert_eq!(
        core.attached_client_flags(),
        "ignore-size,no-detach-on-destroy"
    );

    core.handle_message(event(EventPayload::detached_requested(session, None)));
    assert!(core.attached_read_only());
    assert_eq!(
        core.attached_client_flags(),
        "ignore-size,no-detach-on-destroy"
    );

    core.clear_attachment();
    assert!(!core.attached_read_only());
    assert_eq!(core.attached_client_flags(), "");
}

#[test]
fn welcome_clock_is_kept_and_reset_on_reconnect() {
    let mut core = ClientCore::new();
    assert_eq!(core.server_clock(), 0);
    core.handle_message(ProtocolMessage::Welcome(Welcome {
        protocol_version: PROTOCOL_VERSION,
        server_id: 1,
        client_id: ClientId(1),
        client_instance_id: zz_protocol::ClientInstanceId(1),
        caps: 0,
        clock: 1_760_000_000,
        platform: zz_protocol::HostPlatform::default(),
        revision: zz_protocol::REVISION,
    }));
    assert_eq!(drain(&mut core), vec![CoreEvent::HelloReceived]);
    assert_eq!(core.server_clock(), 1_760_000_000);

    core.reset_session();
    core.clear_attachment();
    assert_eq!(core.server_clock(), 1_760_000_000);

    core.handle_message(hello());
    assert_eq!(core.server_clock(), 0);

    let ProtocolMessage::ServerHello(mut next) = hello() else {
        unreachable!("the fixture is a server hello");
    };
    next.clock = 1_760_000_100;
    core.adopt_hello(*next);
    assert_eq!(core.server_clock(), 1_760_000_100);
}

#[test]
fn attachment_clears_session_state_and_notifies_changes() {
    let pane = PaneId(7);
    let mut core = ClientCore::new();
    core.prefix_armed = true;
    core.key_table = Some(("prefix".to_owned(), false));
    core.command_prompt = Some(CommandPromptState {
        prompt: ":".to_owned(),
        input: "echo".to_owned(),
        cursor: 4,
        kind: zz_protocol::CommandPromptKind::Command,
        history: Vec::new(),
        prompt_type: zz_protocol::CommandPromptType::Command,
        mode: zz_protocol::CommandPromptMode::Text,
        no_freeze: false,
        pane: None,
        command_mode: false,
        prompt_cursor: zz_protocol::PromptCursor::default(),
    });
    core.command_output = Some((
        3,
        pane,
        new_retained_viewport(
            TerminalViewport::blank(8, 4, zz_terminal::SessionStatus::Running),
            &mut 1,
        ),
    ));
    core.choose_tree = Some(ChooseTreeState {
        items: Vec::new(),
        search: None,
        selected: 0,
        kind: zz_protocol::ChooseTreeKind::Windows,
        filter_no_matches: false,
        prompt: String::new(),
        help: false,
    });
    core.choose_buffer = Some(ChooseBufferState {
        items: Vec::new(),
        search: None,
        selected: 0,
        filter_no_matches: false,
        help: false,
        prompt: String::new(),
    });
    core.display_panes = Some(DisplayPanesState {
        window: zz_protocol::WindowId(2),
        duration_ms: 1000,
        indicators: Vec::new(),
        colour: None,
        active_colour: None,
    });
    core.menu = Some(MenuState {
        left: 1,
        top: 1,
        width: 8,
        height: 4,
        client_columns: 80,
        client_rows: 24,
        cell_width_px: 8,
        cell_height_px: 18,
        title: String::new(),
        style: String::new(),
        selected_style: String::new(),
        border_style: String::new(),
        border_lines: zz_protocol::PopupBorderLines::Single,
        items: Vec::new(),
        selected: None,
        stay_open: false,
        mouse_keys: false,
    });
    core.confirm = Some(ConfirmState {
        prompt: "continue?".to_owned(),
        confirm_key: b'y',
        default_yes: false,
        prompt_cursor: zz_protocol::PromptCursor::default(),
    });

    core.handle_message(ProtocolMessage::Attached {
        session: SessionId(9),
        snapshot: snapshot_with(&[]),
        read_only: false,
        client_flags: String::new(),
    });

    assert!(!core.prefix_armed());
    assert_eq!(core.key_table(), None);
    assert!(core.command_prompt().is_none());
    assert!(core.command_output().is_none());
    assert!(core.choose_tree().is_none());
    assert!(core.choose_buffer().is_none());
    assert!(core.display_panes().is_none());
    assert!(core.menu().is_none());
    assert!(core.confirm().is_none());
    assert_eq!(
        drain(&mut core),
        vec![
            CoreEvent::Attached {
                session: SessionId(9)
            },
            CoreEvent::SnapshotChanged,
            CoreEvent::PrefixArmed { armed: false },
            CoreEvent::KeyTableChanged,
            CoreEvent::CommandPromptChanged,
            CoreEvent::CommandOutputChanged,
            CoreEvent::ChooseTreeChanged,
            CoreEvent::ChooseBufferChanged,
            CoreEvent::DisplayPanesChanged,
            CoreEvent::MenuChanged,
            CoreEvent::ConfirmChanged,
        ]
    );
}
