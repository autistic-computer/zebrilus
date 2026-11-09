use std::error::Error;

use serde_json::{Value, json};
use zz_client::{AgentAttentionStatus, agent_attention_status};
use zz_protocol::{
    AgentConnectionPhase, AgentPaneWire, AgentPermissionWire, Axis, LayoutNode, PROTOCOL_VERSION,
    PaneId, ProtocolError, SplitId, decode_protocol_frame,
    layout::{joined_layout, swapped_layout},
};

fn terminal_vectors() -> Value {
    use zz_client::terminal_view::{
        PointerGrid, accumulate_scroll, cursor_should_blink, scroll_fraction, scrollbar_strip_hit,
        selection_autoscroll_lines,
    };
    use zz_terminal::{
        Color, Cursor, CursorBlinkPolicy, CursorStyle, Modifiers, ScrollbarState,
        TerminalMousePhase,
    };

    let mut pointers = Vec::new();
    for (width, height, cell_width, line_height, offset, scale, columns, rows) in [
        (80.0, 32.0, 8.0, 16.0, 0.0, 2.0, 10, 2),
        (80.5, 32.5, 8.05, 16.25, 3.0, 1.5, 10, 2),
        (0.0, 0.0, 8.0, 16.0, 0.0, 1.0, 0, 0),
        (900.0, 600.0, 7.2, 15.3, 2.75, 1.25, 125, 39),
        (1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1, 1),
    ] {
        let grid = PointerGrid {
            left: 10.0,
            top: 20.0,
            width,
            height,
            cell_width,
            line_height,
            content_offset: offset,
            scale,
            columns,
            rows,
        };
        for x in [
            -100.0, 0.0, 10.0, 10.499_999, 10.5, 11.5, 27.0, 90.0, 1000.0,
        ] {
            for y in [-100.0, 0.0, 20.0, 20.5, 21.5, 45.0, 52.0, 1000.0] {
                let input = grid.input(
                    x,
                    y,
                    TerminalMousePhase::Motion,
                    None,
                    9,
                    Modifiers::default(),
                    false,
                );
                pointers.push(json!({
                    "grid": [grid.left, grid.top, width, height, cell_width, line_height, offset, scale],
                    "size": [columns, rows], "point": [x, y],
                    "expected": [u32::from(input.cell.column), u32::from(input.cell.row),
                        u32::from(input.cell.click_count), input.x, input.y, input.screen_width,
                        input.screen_height, input.cell_width, input.cell_height],
                    "contains": grid.contains(x, y),
                }));
            }
        }
    }
    let mut scrolling = Vec::new();
    for line_height in [0.0, 1.0, 15.3, 16.0, 32.0] {
        for y in [-1000.0, 3.0, 19.999, 20.0, 20.5, 52.0, 52.001, 68.0, 1000.0] {
            scrolling.push(json!({
                "args": [20.0, 52.0, line_height, y],
                "expected": selection_autoscroll_lines(20.0, 52.0, line_height, y),
            }));
        }
    }
    let mut fractions = Vec::new();
    for height in [-1.0, 0.0, 1.0, 32.5, 100.0, 600.0] {
        for y in [-1000.0, 19.0, 20.0, 20.5, 21.0, 70.0, 120.0, 1000.0] {
            fractions.push(
                json!({"args": [20.0, height, y], "expected": scroll_fraction(20.0, height, y)}),
            );
        }
    }
    let mut accumulations = Vec::new();
    for initial in [-0.75, -0.5, 0.0, 0.5, 0.75] {
        for delta in [
            -200.5, -2.25, -1.0, -0.01, 0.0, 0.01, 0.75, 1.0, 2.25, 200.5,
        ] {
            let mut remainder = initial;
            let lines = accumulate_scroll(&mut remainder, delta);
            accumulations
                .push(json!({"args": [initial, delta], "lines": lines, "remainder": remainder}));
        }
    }
    let mut cursors = Vec::new();
    for visible in [false, true] {
        for blinking in [false, true] {
            for focused in [false, true] {
                for (policy_index, policy) in [
                    CursorBlinkPolicy::Off,
                    CursorBlinkPolicy::On,
                    CursorBlinkPolicy::Terminal,
                ]
                .into_iter()
                .enumerate()
                {
                    let cursor = Cursor::new(
                        0,
                        0,
                        visible,
                        blinking,
                        false,
                        CursorStyle::Block,
                        Color::rgb(0, 0, 0),
                    );
                    cursors.push(json!({
                        "visible": visible, "blinking": blinking, "focused": focused, "policy": policy_index,
                        "expected": cursor_should_blink(Some(cursor), policy, focused),
                    }));
                }
            }
        }
    }
    let mut strips = Vec::new();
    for total in [10, 20, 100] {
        for x in [79.99, 80.0, 85.0, 90.0, 90.01] {
            for y in [19.99, 20.0, 35.0, 52.0, 52.01] {
                strips.push(json!({
                    "total": total, "point": [x, y],
                    "expected": scrollbar_strip_hit(90.0, 20.0, 52.0, 10.0,
                        ScrollbarState { total, offset: 0, len: 20 }, x, y),
                }));
            }
        }
    }
    json!({"pointers": pointers, "scrolling": scrolling, "fractions": fractions,
        "accumulations": accumulations, "cursors": cursors, "strips": strips})
}

fn id_json(id: u64) -> Value {
    json!({"$": "Id", "high": id >> 32, "low": id & u64::from(u32::MAX)})
}

fn layout_json(node: &LayoutNode) -> Value {
    match node {
        LayoutNode::Empty => json!({"$": "Empty"}),
        LayoutNode::Pane(id) => json!({"$": "Pane", "id": id_json(id.0)}),
        LayoutNode::Split {
            id,
            axis,
            ratio,
            first,
            second,
        } => json!({
            "$": "Split", "id": id_json(id.0),
            "axis": {"$": if *axis == Axis::Horizontal {"Horizontal"} else {"Vertical"}},
            "ratio_bits": ratio.to_bits(), "first": layout_json(first), "second": layout_json(second)
        }),
    }
}

fn next_random(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    *seed
}

fn random_layout(depth: u32, seed: &mut u64) -> LayoutNode {
    let selector = next_random(seed);
    if depth == 0 || selector.is_multiple_of(5) {
        if selector.is_multiple_of(13) {
            LayoutNode::Empty
        } else {
            LayoutNode::Pane(PaneId(selector % 17 + ((selector % 3) << 60)))
        }
    } else {
        LayoutNode::Split {
            id: SplitId(next_random(seed)),
            axis: if selector.is_multiple_of(2) {
                Axis::Horizontal
            } else {
                Axis::Vertical
            },
            ratio: f32::from(u16::try_from(selector % 1000).expect("ratio is bounded")) / 1000.0,
            first: Box::new(random_layout(depth - 1, seed)),
            second: Box::new(random_layout(depth - 1, seed)),
        }
    }
}

fn layout_vectors() -> Vec<Value> {
    let mut seed = 0x5a5a_1234_fedc_9876;
    (0..256)
        .map(|index| {
            let node = random_layout(index % 7, &mut seed);
            let mut before = Vec::new();
            node.panes(&mut before);
            let source = before.first().copied().unwrap_or(PaneId(0));
            let target = if index % 3 == 0 {
                source
            } else {
                before.last().copied().unwrap_or(PaneId(u64::MAX))
            };
            let swapped = swapped_layout(&node, source, target);
            let joins = [false, true].map(|before| {
                let split = SplitId(u64::MAX - u64::from(index));
                let axis = if before { Axis::Horizontal } else { Axis::Vertical };
                let ratio = 0.35_f32;
                let joined = joined_layout(&node, source, target, split, axis, ratio, before);
                json!({
                    "split": id_json(split.0), "axis": {"$": if before {"Horizontal"} else {"Vertical"}},
                    "ratio": ratio.to_bits(), "before": before,
                    "result": joined.as_ref().map(layout_json)
                })
            });
            json!({
                "node": layout_json(&node), "source": id_json(source.0), "target": id_json(target.0),
                "swapped": layout_json(&swapped), "joins": joins, "contains": node.contains(source),
                "panes": before.into_iter().map(|id| id_json(id.0)).collect::<Vec<_>>()
            })
        })
        .collect()
}

fn envelope_status(total: usize, declared: u32, lane: u8, flags: u8, version: u16) -> u32 {
    let mut frame = vec![255; total];
    if let Some(prefix) = frame.get_mut(..4) {
        prefix.copy_from_slice(&declared.to_le_bytes());
    }
    if let Some(value) = frame.get_mut(4) {
        *value = lane;
    }
    if let Some(value) = frame.get_mut(5) {
        *value = flags;
    }
    if let Some(bytes) = frame.get_mut(6..8) {
        bytes.copy_from_slice(&version.to_le_bytes());
    }
    match decode_protocol_frame(&frame) {
        Err(ProtocolError::Truncated) => 1,
        Err(ProtocolError::FrameTooLarge(_)) => 2,
        Err(ProtocolError::LengthMismatch) => 3,
        Err(ProtocolError::UnsupportedLane(_)) => 4,
        Err(ProtocolError::VersionMismatch { .. }) => 5,
        Err(ProtocolError::UnsupportedFlags(_)) => 6,
        _ => 0,
    }
}

fn envelope_vectors() -> Vec<Value> {
    let mut result = Vec::new();
    for total in [0, 1, 3, 4, 5, 7, 8, 9, 16, 32, 127] {
        for declared in [0, 1, 3, 4, 5, 12, 28, 123, 67_108_864, 67_108_865, u32::MAX] {
            for lane in [0, 1, 2, 255] {
                for flags in [0, 1, 255] {
                    for version in [0, PROTOCOL_VERSION - 1, PROTOCOL_VERSION, u16::MAX] {
                        result.push(json!({
                            "args": [total, usize::try_from(declared).expect("u32 fits usize"), usize::from(lane), usize::from(flags), usize::from(version)],
                            "status": envelope_status(total, declared, lane, flags, version)
                        }));
                    }
                }
            }
        }
    }
    for (total, declared) in [
        (67_108_868, 67_108_864),
        (67_108_868, 67_108_865),
        (67_108_867, 67_108_864),
    ] {
        result.push(json!({
            "args": [total, declared, 0, 0, PROTOCOL_VERSION],
            "status": envelope_status(total, declared, 0, 0, PROTOCOL_VERSION)
        }));
    }

    result
}

fn attention_vectors() -> Vec<Value> {
    let phases = [
        AgentConnectionPhase::Starting,
        AgentConnectionPhase::Ready,
        AgentConnectionPhase::Running,
        AgentConnectionPhase::AwaitingPermission,
        AgentConnectionPhase::Failed {
            message: "failed".to_owned(),
        },
    ];
    phases
        .into_iter()
        .enumerate()
        .flat_map(|(phase_index, phase)| {
            [false, true].into_iter().map(move |permission| {
                let state = AgentPaneWire {
                    phase: phase.clone(),
                    pending_permission: permission.then(|| AgentPermissionWire {
                        request_id: 7,
                        payload: "{}".to_owned(),
                    }),
                    ..AgentPaneWire::default()
                };
                let status = match agent_attention_status(&state) {
                    AgentAttentionStatus::Idle => "Idle",
                    AgentAttentionStatus::Working => "Working",
                    AgentAttentionStatus::NeedsInput => "NeedsInput",
                    AgentAttentionStatus::Failed => "AttentionFailed",
                };
                json!({"phase": phase_index, "permission": permission, "status": status})
            })
        })
        .collect()
}

fn cursor_json(cursor: &zz_client::agent_lens::AgentCursor, journal: &[(u64, Vec<u8>)]) -> Value {
    json!({"$": "Cursor", "last": id_json(cursor.last_seq), "pending": cursor.replay_pending,
        "bytes": id_json(u64::try_from(cursor.bytes).expect("usize fits u64")),
        "journal": journal.iter().map(|(seq, bytes)| json!({"$":"Entry", "sequence":id_json(*seq), "bytes":bytes.len()})).collect::<Vec<_>>()})
}

fn replay_vectors() -> Vec<Value> {
    let mut output = Vec::new();
    for last in [
        0_u64,
        1,
        u64::from(u32::MAX) - 1,
        u64::from(u32::MAX),
        u64::from(u32::MAX) + 1,
        1_u64 << 63,
        u64::MAX - 2,
        u64::MAX,
    ] {
        for pending in [false, true] {
            for bytes in [4_usize, usize::MAX - 50] {
                let mut cursor = zz_client::agent_lens::AgentCursor::default();
                cursor.last_seq = last;
                cursor.replay_pending = pending;
                cursor.bytes = bytes;
                let mut journal = vec![(last, b"seed".to_vec())];
                for action in 0..9 {
                    let initial = cursor_json(&cursor, &journal);
                    let old = cursor.last_seq;
                    let first = if action == 6 {
                        old.saturating_add(5)
                    } else {
                        old.saturating_add(1)
                    };
                    let specifications = match action {
                        0 => vec![(false, old, false, false)],
                        1 => vec![(false, old.saturating_add(3), false, false)],
                        2 => vec![(false, old.saturating_add(1), false, false)],
                        3 => vec![
                            (true, 0, false, false),
                            (false, old.saturating_add(2), false, false),
                        ],
                        4 => vec![(false, old.saturating_add(100), true, true)],
                        5 => vec![
                            (false, old, false, false),
                            (false, old.saturating_add(3), false, false),
                            (false, old.saturating_add(1), false, false),
                        ],
                        6 => vec![(true, 0, false, false)],
                        7 => vec![
                            (false, old.saturating_add(1), true, false),
                            (false, old.saturating_add(2), false, false),
                        ],
                        _ => Vec::new(),
                    };
                    let mut items = Vec::new();
                    let mut normalized = Vec::new();
                    for (offset, (invalid, seq, reset, restoring)) in
                        specifications.into_iter().enumerate()
                    {
                        if invalid {
                            items.push(b"{broken".to_vec());
                            normalized.push(json!({"$":"JournalInvalid", "position":id_json(first.saturating_add(u64::try_from(offset).expect("offset fits u64")))}));
                        } else {
                            let item = serde_json::to_vec(&json!({"seq":seq,"item":if reset {"sessionReset"} else {"update"},"restoring":restoring})).expect("serialize envelope");
                            normalized.push(json!({"$":"JournalUpdate", "sequence":id_json(seq), "reset":reset, "restoring":restoring, "bytes":item.len()}));
                            items.push(item);
                        }
                    }
                    let (request, invalid) = cursor.apply(&mut journal, first, &items);
                    output.push(json!({"initial": initial, "items": normalized, "cursor": cursor_json(&cursor,&journal), "request": request.map(id_json), "invalid": invalid}));
                }
            }
        }
    }
    output
}

fn main() -> Result<(), Box<dyn Error>> {
    println!(
        "{}",
        serde_json::to_string(&json!({
            "protocol_version": PROTOCOL_VERSION,
            "envelopes": envelope_vectors(), "layouts": layout_vectors(),
            "attention": attention_vectors(), "replay": replay_vectors(), "terminal": terminal_vectors()
        }))?
    );
    Ok(())
}
