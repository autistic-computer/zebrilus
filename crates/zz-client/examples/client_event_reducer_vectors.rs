use std::error::Error;

use serde_json::{Value, json};
use zz_client::{ClientCore, CoreEvent};
use zz_protocol::{Event, EventPayload, PaneId, ProtocolMessage, encode_protocol_message};
use zz_terminal::{SessionStatus, TerminalViewport};

fn open(pane: u64, id: u64, generation: u64) -> ProtocolMessage {
    let mut viewport = TerminalViewport::blank(2, 1, SessionStatus::Running);
    viewport.generation = generation;
    viewport.view_generation = generation + 100;
    ProtocolMessage::Event(Event {
        sequence: generation,
        payload: EventPayload::CommandOutput {
            pane: PaneId(pane),
            output_id: id,
            viewport: Some(viewport),
        },
    })
}

fn close(pane: u64, id: u64) -> ProtocolMessage {
    ProtocolMessage::Event(Event {
        sequence: 1,
        payload: EventPayload::CommandOutput {
            pane: PaneId(pane),
            output_id: id,
            viewport: None,
        },
    })
}

fn removed(pane: u64) -> ProtocolMessage {
    ProtocolMessage::Event(Event {
        sequence: 1,
        payload: EventPayload::PaneRemoved(PaneId(pane)),
    })
}

fn case(name: &str, events: Vec<ProtocolMessage>) -> Result<Value, Box<dyn Error>> {
    let mut core = ClientCore::new();
    let mut steps = Vec::new();
    let mut revision = 0;
    let mut watermark = 0;
    for event in events {
        if let ProtocolMessage::Event(Event {
            payload: EventPayload::CommandOutput { output_id, .. },
            ..
        }) = &event
        {
            watermark = watermark.max(*output_id);
        }
        let frame = encode_protocol_message(&event)?;
        core.handle_message(event);
        while let Some(event) = core.poll_event() {
            if matches!(event, CoreEvent::CommandOutputChanged) {
                revision += 1;
            }
        }
        let output = core.command_output();
        steps.push(json!({"bytes":frame,"id":core.command_output_id(),"pane":output.map(|(pane,_)|pane.0),"generation":output.map(|(_,viewport)|viewport.generation),"view_generation":output.map(|(_,viewport)|viewport.view_generation),"revision":revision,"watermark":watermark}));
    }
    Ok(json!({"name":name,"steps":steps}))
}

fn main() -> Result<(), Box<dyn Error>> {
    let cases = vec![
        case(
            "same-id-update",
            vec![open(1, 7, 1), open(1, 7, 2), open(2, 7, 3)],
        )?,
        case(
            "closed-id-cannot-reopen",
            vec![open(1, 7, 1), close(1, 7), open(2, 7, 2), open(2, 8, 3)],
        )?,
        case(
            "newer-close-prevents-stale-open",
            vec![close(1, 9), open(1, 8, 1), open(2, 9, 2), open(2, 10, 3)],
        )?,
        case(
            "older-close-does-not-clear",
            vec![open(1, 7, 1), close(1, 6), close(2, 7)],
        )?,
        case(
            "newer-close-clears-current",
            vec![open(1, 7, 1), close(2, 8), open(1, 7, 2), open(2, 8, 3)],
        )?,
        case(
            "zero-resync-preserves-watermark",
            vec![
                open(1, 7, 1),
                close(2, 0),
                open(2, 7, 2),
                open(2, 8, 3),
                close(2, 0),
                close(1, 0),
            ],
        )?,
        case(
            "pane-removal-preserves-watermark",
            vec![
                open(1, 7, 1),
                removed(2),
                removed(1),
                open(1, 7, 2),
                open(2, 8, 3),
            ],
        )?,
        case(
            "maximum-id-closed",
            vec![
                open(1, u64::MAX, 1),
                close(1, u64::MAX),
                open(2, u64::MAX, 2),
            ],
        )?,
        case(
            "empty-resync-and-stale-closes",
            vec![
                close(1, 0),
                close(1, 3),
                close(1, 2),
                close(2, 3),
                open(2, 3, 1),
            ],
        )?,
    ];
    println!(
        "{}",
        json!({"reference":"actual zz-client ClientCore command-output reducer and change events","cases":cases})
    );
    Ok(())
}
