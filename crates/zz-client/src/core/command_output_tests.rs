use super::test_support::{drain, event, hello};
use super::*;

fn command_output_frame(pane: PaneId, output_id: u64, generation: u64) -> ProtocolMessage {
    let mut viewport = TerminalViewport::blank(80, 24, zz_terminal::SessionStatus::Running);
    viewport.generation = generation;
    event(EventPayload::CommandOutput {
        pane,
        output_id,
        viewport: Some(viewport),
    })
}

fn command_output_close(pane: PaneId, output_id: u64) -> ProtocolMessage {
    event(EventPayload::CommandOutput {
        pane,
        output_id,
        viewport: None,
    })
}

#[test]
fn command_output_actor_updates_replace_and_ignore_stale_traffic() {
    let pane = PaneId(5);
    let mut core = ClientCore::new();

    core.handle_message(command_output_frame(pane, 10, 1));
    assert_eq!(drain(&mut core), vec![CoreEvent::CommandOutputChanged]);
    assert_eq!(core.command_output_id(), Some(10));
    assert_eq!(
        core.command_output()
            .map(|(output_pane, viewport)| (output_pane, viewport.generation)),
        Some((pane, 1))
    );

    core.handle_message(command_output_frame(pane, 10, 2));
    assert_eq!(drain(&mut core), vec![CoreEvent::CommandOutputChanged]);
    assert_eq!(
        core.command_output()
            .map(|(output_pane, viewport)| (output_pane, viewport.generation)),
        Some((pane, 2))
    );

    core.handle_message(command_output_frame(PaneId(4), 9, 3));
    core.handle_message(command_output_close(pane, 9));
    assert!(drain(&mut core).is_empty());
    assert_eq!(core.command_output_id(), Some(10));

    core.handle_message(command_output_frame(pane, 11, 2));
    assert_eq!(drain(&mut core), vec![CoreEvent::CommandOutputChanged]);
    assert_eq!(core.command_output_id(), Some(11));

    core.handle_message(command_output_close(pane, 10));
    assert!(drain(&mut core).is_empty());
    assert_eq!(core.command_output_id(), Some(11));

    core.handle_message(command_output_close(pane, 11));
    assert_eq!(drain(&mut core), vec![CoreEvent::CommandOutputChanged]);
    assert_eq!(core.command_output_id(), None);
}

#[test]
fn command_output_newer_close_and_zero_resync_prevent_resurrection() {
    let pane = PaneId(5);
    let mut core = ClientCore::new();

    core.handle_message(command_output_frame(pane, 5, 1));
    drain(&mut core);
    core.handle_message(command_output_close(pane, 7));
    assert_eq!(drain(&mut core), vec![CoreEvent::CommandOutputChanged]);
    assert_eq!(core.command_output_id(), None);

    core.handle_message(command_output_frame(pane, 6, 2));
    core.handle_message(command_output_close(pane, 6));
    core.handle_message(command_output_frame(pane, 7, 3));
    assert!(drain(&mut core).is_empty());
    assert_eq!(core.command_output_id(), None);

    core.handle_message(command_output_frame(pane, 8, 4));
    assert_eq!(drain(&mut core), vec![CoreEvent::CommandOutputChanged]);
    assert_eq!(core.command_output_id(), Some(8));

    core.handle_message(command_output_close(pane, 0));
    assert_eq!(drain(&mut core), vec![CoreEvent::CommandOutputChanged]);
    assert_eq!(core.command_output_id(), None);

    core.handle_message(command_output_frame(pane, 8, 5));
    assert!(drain(&mut core).is_empty());
    assert_eq!(core.command_output_id(), None);

    core.handle_message(command_output_frame(pane, 9, 6));
    assert_eq!(drain(&mut core), vec![CoreEvent::CommandOutputChanged]);
    assert_eq!(core.command_output_id(), Some(9));
}

#[test]
fn command_output_watermark_resets_with_the_connection() {
    let pane = PaneId(5);
    let mut core = ClientCore::new();

    core.handle_message(command_output_frame(pane, 20, 1));
    drain(&mut core);
    core.handle_message(hello());
    assert_eq!(drain(&mut core), vec![CoreEvent::HelloReceived]);
    assert_eq!(core.command_output_id(), None);

    core.handle_message(command_output_frame(pane, 1, 2));
    assert_eq!(drain(&mut core), vec![CoreEvent::CommandOutputChanged]);
    assert_eq!(core.command_output_id(), Some(1));
}

#[test]
fn command_output_watermark_resets_when_adopting_a_handshake() {
    let pane = PaneId(5);
    let mut core = ClientCore::new();

    core.handle_message(command_output_frame(pane, 20, 1));
    drain(&mut core);
    let ProtocolMessage::ServerHello(hello) = hello() else {
        unreachable!();
    };
    core.adopt_hello(*hello);
    assert_eq!(core.command_output_id(), Some(20));

    core.handle_message(command_output_frame(pane, 1, 2));
    assert_eq!(drain(&mut core), vec![CoreEvent::CommandOutputChanged]);
    assert_eq!(core.command_output_id(), Some(1));
}
