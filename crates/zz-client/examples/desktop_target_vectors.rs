use std::{error::Error, fs};
use zz_client::{
    SurfaceKind,
    desktop_input::{desktop_active_pane, desktop_snapshot_target},
};
use zz_protocol::{
    Event, EventPayload, ModalPaneSnapshot, MuxSnapshot, PaneId, PaneKindSnapshot, ProtocolMessage,
    SessionId, WindowId, decode_protocol_frame, encode_protocol_message,
};

fn optional(output: &mut Vec<u8>, value: Option<u64>) {
    output.push(u8::from(value.is_some()));
    output.extend_from_slice(&value.unwrap_or_default().to_le_bytes());
}

fn emit(
    output: &mut Vec<u8>,
    snapshot: &MuxSnapshot,
    attached: Option<SessionId>,
) -> Result<(), Box<dyn Error>> {
    let frame = encode_protocol_message(&ProtocolMessage::Event(Event {
        sequence: 0,
        payload: EventPayload::Snapshot(snapshot.clone()),
    }))?;
    output.extend_from_slice(&u32::try_from(frame.len())?.to_le_bytes());
    output.extend(frame);
    optional(output, attached.map(|id| id.0));
    optional(
        output,
        desktop_active_pane(snapshot, attached).map(|id| id.0),
    );
    let target = desktop_snapshot_target(snapshot, attached);
    optional(output, target.active.map(|(id, _)| id.0));
    output.push(target.active.map_or(0, |(_, kind)| match kind {
        SurfaceKind::Terminal => 0,
        SurfaceKind::Browser => 1,
        SurfaceKind::Other => 2,
    }));
    optional(output, target.capture.map(|id| id.0));
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    let seed: serde_json::Value =
        serde_json::from_slice(&fs::read(args.get(1).ok_or("missing seed")?)?)?;
    let bytes = seed["messages"]
        .as_array()
        .ok_or("missing messages")?
        .iter()
        .find(|message| message["name"] == "snapshot-full")
        .ok_or("missing rich snapshot")?["bytes"]
        .as_array()
        .ok_or("missing frame bytes")?
        .iter()
        .map(|byte| {
            byte.as_u64()
                .and_then(|value| u8::try_from(value).ok())
                .ok_or("invalid frame byte")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let ProtocolMessage::Event(Event {
        payload: EventPayload::Snapshot(base),
        ..
    }) = decode_protocol_frame(&bytes)?
    else {
        return Err("seed is not a snapshot".into());
    };
    let original = base.sessions.first().ok_or("missing seed session")?;
    let session_id = original.id;
    let original_window = original.windows.first().ok_or("missing seed window")?;
    let window_id = original_window.id;
    let keys = original_window.panes.keys().copied().collect::<Vec<_>>();
    let mut output = Vec::new();
    emit(&mut output, &MuxSnapshot::default(), None)?;
    for scenario in 0_u32..256 {
        let mut snapshot = base.clone();
        snapshot.sessions[0].active_window = if scenario & 1 == 0 {
            window_id
        } else {
            WindowId(61)
        };
        snapshot.focused_window = match (scenario >> 1) & 3 {
            0 => None,
            1 => Some(window_id),
            2 => Some(WindowId(61)),
            _ => Some(WindowId(62)),
        };
        let active = match (scenario >> 3) & 7 {
            0 => PaneId(0),
            1 => PaneId(u64::MAX),
            n => keys[usize::try_from(n - 2)? % keys.len()],
        };
        snapshot.sessions[0].windows[0].active_pane = active;
        snapshot.sessions[0].windows[0].modal = Some(ModalPaneSnapshot {
            pane: PaneId(if scenario & 32 == 0 { 0 } else { u64::MAX }),
            capture_keys: scenario & 64 != 0,
            close_on_click: false,
            close_on_cancel: false,
        });
        let mut duplicate = snapshot.sessions[0].windows[0].clone();
        duplicate.id = WindowId(61);
        duplicate.active_pane = keys[0];
        duplicate.modal = None;
        if scenario & 128 != 0 {
            snapshot.sessions[0].windows[0].panes.remove(&active);
            if let Some(pane) = duplicate.panes.get_mut(&active) {
                pane.id = PaneId(317);
                pane.kind = PaneKindSnapshot::Terminal;
            }
        }
        snapshot.sessions[0].windows.push(duplicate);
        for attached in [
            None,
            Some(session_id),
            Some(SessionId(0)),
            Some(SessionId(u64::MAX)),
        ] {
            emit(&mut output, &snapshot, attached)?;
        }
    }
    for scenario in 0_u32..64 {
        let mut snapshot = base.clone();
        let mut duplicate = snapshot.sessions[0].clone();
        snapshot.sessions[0].windows[0].active_pane = PaneId(211);
        snapshot.sessions[0].windows[0].panes.clear();
        let mut pane = original_window
            .panes
            .values()
            .next()
            .ok_or("empty panes")?
            .clone();
        pane.id = PaneId(212);
        pane.kind = PaneKindSnapshot::Terminal;
        duplicate.windows[0].panes.clear();
        duplicate.windows[0].panes.insert(PaneId(211), pane);
        duplicate.windows[0].id = window_id;
        if scenario & 1 != 0 {
            duplicate.id = SessionId(218);
        }
        if scenario & 2 != 0 {
            snapshot.sessions[0].windows[0].id = WindowId(219);
        }
        if scenario & 4 != 0 {
            snapshot.sessions[0].windows.clear();
        }
        if scenario & 8 != 0 {
            snapshot.sessions[0]
                .windows
                .iter_mut()
                .for_each(|window| window.modal = None);
        }
        snapshot.sessions.push(duplicate);
        if scenario & 16 != 0 {
            snapshot.sessions.reverse();
        }
        if scenario & 32 != 0 {
            snapshot.sessions[0].windows.reverse();
        }
        emit(&mut output, &snapshot, Some(session_id))?;
    }
    fs::write(args.get(2).ok_or("missing output")?, output)?;
    Ok(())
}
