use super::*;
use std::collections::BTreeMap;
use zz_protocol::{
    ClientId, LayoutNode, PROTOCOL_VERSION, PaneKindSnapshot, PaneSnapshot, PaneSummary,
    SessionSnapshot, WindowId, WindowSnapshot,
};
use zz_terminal::TerminalAppearance;

pub(super) fn event(payload: EventPayload) -> ProtocolMessage {
    ProtocolMessage::Event(Event {
        sequence: 0,
        payload,
    })
}

pub(super) fn hello() -> ProtocolMessage {
    ProtocolMessage::ServerHello(Box::new(ServerHello {
        protocol_version: PROTOCOL_VERSION,
        server_id: 1,
        client_id: ClientId(1),
        client_instance_id: zz_protocol::ClientInstanceId(1),
        capabilities: Vec::new(),
        appearance: TerminalAppearance::default(),
        appearance_provenance: AppearanceProvenance::default(),
        mux_options: MuxOptions::default(),
        status: StatusLine::default(),
        key_tables: Vec::new(),
        clock: 0,
        platform: zz_protocol::HostPlatform::default(),
        revision: zz_protocol::REVISION,
    }))
}

pub(super) fn snapshot_with(panes: &[PaneId]) -> MuxSnapshot {
    let entries: BTreeMap<PaneId, PaneSnapshot> = panes
        .iter()
        .map(|pane| {
            (
                *pane,
                PaneSnapshot {
                    id: *pane,
                    title: String::new(),
                    kind: PaneKindSnapshot::Terminal,
                    synchronized_input: false,
                    bell: false,
                    dead: false,
                    dead_status: None,
                    border_colour: None,
                    active_border_colour: None,
                    border_status_text: String::new(),
                    mode: None,
                    status: None,
                    summary: PaneSummary::default(),
                },
            )
        })
        .collect();
    let first = panes.first().copied().unwrap_or_default();
    MuxSnapshot {
        generation: 1,
        sessions: vec![SessionSnapshot {
            id: SessionId(0),
            name: "0".to_owned(),
            active_window: WindowId(0),
            windows: vec![WindowSnapshot {
                id: WindowId(0),
                index: 0,
                name: "win".to_owned(),
                automatic_rename: true,
                active_pane: first,
                zoomed_pane: None,
                layout: LayoutNode::Pane(first),
                panes: entries,
                layout_dump: String::new(),
                visible_layout_dump: String::new(),
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
            }],
            viewers: Vec::new(),
        }],
        focused_window: None,
    }
}

pub(super) fn drain(core: &mut ClientCore) -> Vec<CoreEvent> {
    let mut events = Vec::new();
    while let Some(event) = core.poll_event() {
        events.push(event);
    }
    events
}
