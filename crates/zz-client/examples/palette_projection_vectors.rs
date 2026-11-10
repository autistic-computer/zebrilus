use std::{collections::BTreeMap, error::Error};

use serde_json::{Value, json};
use zz_client::{
    navigation,
    program::{self, Program},
    window_summary::pane_program,
};
use zz_protocol::{
    AgentDescriptor, AgentProvider, Axis, BrowserDescriptor, EditorDescriptor, Event, EventPayload,
    LayoutNode, MuxSnapshot, PaneBorderIndicators, PaneBorderLines, PaneBorderStatus, PaneId,
    PaneKindSnapshot, PaneSnapshot, PaneSummary, ProtocolMessage, SessionId, SessionSnapshot,
    SplitId, WindowId, WindowSnapshot, decode_protocol_frame, encode_protocol_message,
};

fn uint(bytes: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        bytes.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    bytes.push(value as u8);
}

fn text(bytes: &mut Vec<u8>, value: &str) {
    uint(bytes, value.len() as u64);
    bytes.extend_from_slice(value.as_bytes());
}

fn optional_text(bytes: &mut Vec<u8>, value: Option<&str>) {
    uint(bytes, u64::from(value.is_some()));
    if let Some(value) = value {
        text(bytes, value);
    }
}

fn record(name: &str, request: &[u8], expected: &[u8]) -> Value {
    json!({"name":name,"request":request,"expected":expected})
}

fn pane(id: u64, kind: usize, phase: usize) -> PaneSnapshot {
    let kind = match kind % 6 {
        0 => PaneKindSnapshot::Picker,
        1 => PaneKindSnapshot::Terminal,
        2 => PaneKindSnapshot::Browser(BrowserDescriptor {
            tabs: match phase % 3 {
                0 => Vec::new(),
                1 => vec![
                    " \u{2003}".to_owned(),
                    " https://example.test/λ ".to_owned(),
                ],
                _ => vec!["url".to_owned()],
            },
            active_tab: if phase.is_multiple_of(2) { 99 } else { 1 },
            profile: String::new(),
        }),
        3 => PaneKindSnapshot::Editor(EditorDescriptor {
            path: None,
            cwd: "/tmp".to_owned(),
        }),
        4 | 5 => PaneKindSnapshot::Agent(AgentDescriptor {
            provider: if kind % 6 == 4 {
                AgentProvider::Codex
            } else {
                AgentProvider::ClaudeCode
            },
            cwd: None,
            session_id: None,
            model: None,
            effort: None,
        }),
        _ => unreachable!(),
    };
    PaneSnapshot {
        id: PaneId(id),
        title: match phase % 3 {
            0 => format!("  pane {id} λ  "),
            1 => " \u{2003}".to_owned(),
            _ => " agent ".to_owned(),
        },
        kind,
        synchronized_input: false,
        bell: false,
        dead: false,
        dead_status: None,
        border_colour: None,
        active_border_colour: None,
        border_status_text: String::new(),
        mode: None,
        status: None,
        summary: PaneSummary {
            repo: match phase % 4 {
                0 => "/repo/one/",
                1 => r"C:\work\two\",
                2 => " ",
                _ => "",
            }
            .to_owned(),
            cwd: if phase.is_multiple_of(2) {
                "/cwd/three"
            } else {
                "////"
            }
            .to_owned(),
            process: match phase % 4 {
                0 => vec!["/usr/bin/python3.12".to_owned()],
                1 => vec!["unknown".to_owned(), "C:\\bin\\NVIM.EXE".to_owned()],
                2 => vec!["codex".to_owned(), "node".to_owned()],
                _ => Vec::new(),
            },
            ..PaneSummary::default()
        },
    }
}

fn window(id: u64, base: u32, phase: usize) -> WindowSnapshot {
    let panes: BTreeMap<_, _> = (0..6)
        .map(|i| {
            let pane = pane(id * 10 + i as u64, i, phase + i);
            (pane.id, pane)
        })
        .collect();
    let first = PaneId(id * 10 + 3);
    WindowSnapshot {
        id: WindowId(id),
        index: phase as u32,
        name: format!("window{phase}"),
        automatic_rename: false,
        active_pane: PaneId(id * 10 + (phase % 6) as u64),
        zoomed_pane: None,
        layout: LayoutNode::Split {
            id: SplitId(id),
            axis: Axis::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Pane(first)),
            second: Box::new(LayoutNode::Split {
                id: SplitId(id + 100),
                axis: Axis::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::Pane(first)),
                second: Box::new(LayoutNode::Pane(PaneId(99999))),
            }),
        },
        panes,
        layout_dump: String::new(),
        visible_layout_dump: String::new(),
        status_label: String::new(),
        activity: false,
        silence: false,
        pane_border_status: PaneBorderStatus::default(),
        pane_border_lines: PaneBorderLines::default(),
        pane_border_indicators: PaneBorderIndicators::default(),
        pane_order: if phase.is_multiple_of(3) {
            Vec::new()
        } else {
            vec![
                PaneId(99999),
                PaneId(id * 10 + 5),
                PaneId(id * 10 + 1),
                PaneId(id * 10 + 5),
            ]
        },
        pane_z_order: Vec::new(),
        floating: Vec::new(),
        modal: None,
        sx: 0,
        sy: 0,
        pane_base_index: base,
    }
}

fn snapshots() -> Result<Vec<Value>, Box<dyn Error>> {
    let mut output = Vec::new();
    for (name_index, name) in [
        "",
        "0",
        "08",
        " 08 ",
        "work",
        " work ",
        "\u{2003}",
        "١",
        "00000000000000000001",
    ]
    .into_iter()
    .enumerate()
    {
        for base in [0, 1, u32::MAX - 1, u32::MAX] {
            for phase in 0..4 {
                let mut snapshot = MuxSnapshot {
                    generation: 3,
                    sessions: vec![SessionSnapshot {
                        id: SessionId(name_index as u64),
                        name: name.to_owned(),
                        active_window: WindowId(10 + phase as u64 % 2),
                        windows: vec![window(10, base, phase), window(11, base, phase + 3)],
                        viewers: Vec::new(),
                    }],
                    focused_window: Some(WindowId(11)),
                };
                if phase == 3 {
                    snapshot.sessions[0].windows[0].panes.clear();
                    snapshot.sessions[0].windows[0].layout = LayoutNode::Empty;
                }
                let frame = encode_protocol_message(&ProtocolMessage::Event(Event {
                    sequence: 0,
                    payload: EventPayload::Snapshot(snapshot.clone()),
                }))?;
                let decoded = decode_protocol_frame(&frame)?;
                if decoded
                    != ProtocolMessage::Event(Event {
                        sequence: 0,
                        payload: EventPayload::Snapshot(snapshot.clone()),
                    })
                {
                    return Err("snapshot source round trip changed".into());
                }
                let mut request = Vec::new();
                uint(&mut request, 0);
                uint(&mut request, frame.len() as u64);
                request.extend(frame);
                let mut expected = Vec::new();
                uint(&mut expected, snapshot.sessions.len() as u64);
                for session in &snapshot.sessions {
                    text(
                        &mut expected,
                        &navigation::session_label(session).to_string(),
                    );
                    uint(&mut expected, session.windows.len() as u64);
                    for window in &session.windows {
                        let ordered = navigation::ordered_panes(window);
                        uint(&mut expected, ordered.len() as u64);
                        for pane in ordered {
                            uint(&mut expected, pane.id.0);
                        }
                        for id in [100, 101, 103, 105, 110, 111, 113, 115, 99999, 0, u64::MAX] {
                            uint(
                                &mut expected,
                                u64::from(navigation::pane_index(window, PaneId(id))),
                            );
                        }
                        uint(&mut expected, window.panes.len() as u64);
                        for pane in window.panes.values() {
                            uint(&mut expected, pane.id.0);
                            uint(
                                &mut expected,
                                u64::from(navigation::pane_index(window, pane.id)),
                            );
                            text(&mut expected, &navigation::pane_label(pane));
                            optional_text(
                                &mut expected,
                                pane_program(pane).map(|program| program.name),
                            );
                        }
                    }
                }
                output.push(record(
                    &format!("snapshot-{name_index}-{base}-{phase}"),
                    &request,
                    &expected,
                ));
            }
        }
    }
    Ok(output)
}

fn programs() -> Vec<Value> {
    let mut inputs = vec![
        Vec::new(),
        vec!["unknown".to_owned(), "cargo".to_owned()],
        vec!["node".to_owned(), "claude".to_owned()],
        vec!["my-codex-helper".to_owned()],
        vec!["Python3.12".to_owned()],
        vec![format!("{}λ/nvim", "x".repeat(200))],
        vec!["λ".repeat(100)],
        vec!["x".repeat(1024 * 1024)],
    ];
    for program in Program::all() {
        for command in program.commands {
            inputs.extend([
                vec![(*command).to_owned()],
                vec![format!("/opt/bin/{command}")],
                vec![format!(" \u{2003}{command}\u{a0}")],
                vec![command.to_uppercase()],
                vec![format!("C:\\bin\\{}.EXE", command.replace('/', "\\"))],
                vec![format!("/prefix/{command}.99")],
                vec![format!("x{command}")],
                vec![format!("/prefix/{command}.exe")],
            ]);
        }
    }
    inputs
        .into_iter()
        .enumerate()
        .map(|(index, candidates)| {
            let mut request = Vec::new();
            uint(&mut request, 1);
            uint(&mut request, candidates.len() as u64);
            for candidate in &candidates {
                text(&mut request, candidate);
            }
            let value = program::resolve(candidates.iter().map(String::as_str));
            let mut expected = Vec::new();
            optional_text(&mut expected, value.map(|program| program.id));
            optional_text(&mut expected, value.map(|program| program.name));
            record(&format!("program-{index}"), &request, &expected)
        })
        .collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = snapshots()?;
    cases.extend(programs());
    let catalog: Vec<_> = Program::all()
        .iter()
        .map(|program| json!([program.id, program.name, program.commands]))
        .collect();
    println!(
        "{}",
        serde_json::to_string(&json!({"cases":cases,"catalog":catalog}))?
    );
    Ok(())
}
