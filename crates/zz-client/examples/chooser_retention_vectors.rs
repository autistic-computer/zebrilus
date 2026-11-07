use std::error::Error;

use serde_json::{Value, json};
use zz_client::{ClientCore, CoreEvent, chooser::next_chooser_revision};
use zz_protocol::{
    Batch, ChooseBufferItem, ChooseBufferSearchState, ChooseBufferState, ChooseTreeItem,
    ChooseTreeKind, ChooseTreeSearchState, ChooseTreeState, ChooseTreeTarget, ChooserPresentation,
    ChooserPreview, ChooserPreviewSize, ChooserRow, Event, EventPayload, PaneId, PromptCursor,
    ProtocolMessage, TmuxColour, decode_protocol_frame, encode_protocol_message,
};

fn operation(kind: u8) -> ProtocolMessage {
    let payload = match kind {
        0 => EventPayload::ChooseTree { state: None },
        1 | 2 => EventPayload::ChooseTree {
            state: Some(ChooseTreeState {
                items: if kind == 1 {
                    vec![ChooseTreeItem {
                        label: "pane λ".into(),
                        detail: "detail".into(),
                        target: ChooseTreeTarget::Pane(PaneId(u64::MAX)),
                        depth: 255,
                        flags: 255,
                        pane_kind: None,
                        key: String::new(),
                        text: String::new(),
                    }]
                } else {
                    vec![]
                },
                search: (kind == 2).then(|| ChooseTreeSearchState {
                    query: "query".into(),
                    reverse: true,
                }),
                selected: u32::MAX,
                kind: ChooseTreeKind::Panes,
                filter_no_matches: true,
                prompt: "prompt".into(),
                help: true,
            }),
        },
        3 | 4 => EventPayload::ChooseTreeUpdate {
            search: (kind == 4).then(|| ChooseTreeSearchState {
                query: "λ\0😀".into(),
                reverse: true,
            }),
            selected: u32::MAX,
        },
        5 => EventPayload::ChooseBuffer { state: None },
        6 | 10 => EventPayload::ChooseBuffer {
            state: Some(ChooseBufferState {
                items: if kind == 6 {
                    vec![
                        ChooseBufferItem {
                            name: "buffer λ\0".into(),
                            preview: "first\nline😀".into(),
                            size_bytes: u64::MAX,
                            created_unix_seconds: u64::MAX,
                            key: "λ".into(),
                            text: "#[fg=red]name".into(),
                            tagged: true,
                        },
                        ChooseBufferItem {
                            name: String::new(),
                            preview: String::new(),
                            size_bytes: 0,
                            created_unix_seconds: 0,
                            key: String::new(),
                            text: String::new(),
                            tagged: false,
                        },
                    ]
                } else {
                    vec![]
                },
                search: Some(ChooseBufferSearchState {
                    query: "buffer?".into(),
                    reverse: false,
                }),
                selected: u32::MAX,
                filter_no_matches: true,
                help: true,
                prompt: "buffer prompt\0λ".into(),
            }),
        },
        7 | 11 => EventPayload::ChooseBufferUpdate {
            search: (kind == 11).then(|| ChooseBufferSearchState {
                query: "search\0😀".into(),
                reverse: true,
            }),
            selected: u32::MAX,
        },
        8 => EventPayload::ChooserPresentation { presentation: None },
        _ => EventPayload::ChooserPresentation {
            presentation: Some(Box::new(ChooserPresentation {
                selected: u32::MAX,
                rows: vec![ChooserRow {
                    name: "row\0λ".into(),
                    text: "#[bold]body".into(),
                    align: true,
                }],
                sort: "time".into(),
                view: "preview".into(),
                filter: true,
                selection_style: "reverse".into(),
                border_style: "double".into(),
                prompt_style: "bold".into(),
                preview_size: ChooserPreviewSize::Big,
                preview: Some(ChooserPreview::Markup {
                    lines: vec!["line1".into(), "line2".into()],
                }),
                prompt_cursor: PromptCursor {
                    style: 255,
                    colour: Some(TmuxColour::Indexed(255)),
                },
                prompt_column: u16::MAX,
                prompt_line: "prompt λ\0".into(),
            })),
        },
    };
    ProtocolMessage::Event(Event {
        sequence: u64::from(kind),
        payload,
    })
}

fn tree_value(core: &ClientCore) -> Value {
    core.choose_tree().map_or(
        Value::Null,
        |state| json!({"rows":state.items.len(),"selected":state.selected,"search":state.search}),
    )
}

fn plane_value(core: &ClientCore) -> Value {
    core.chooser_presentation().map_or(Value::Null, |plane|json!({"selected":plane.selected,"rows":plane.rows.len(),"sort":plane.sort,"view":plane.view,"prompt_style":plane.prompt_style,"prompt_line":plane.prompt_line}))
}

fn buffer_cases() -> Result<Vec<Value>, Box<dyn Error>> {
    let mut cases = Vec::new();
    let mut record = |mut bytes: Vec<u8>, refusal: bool| -> Result<(), Box<dyn Error>> {
        let size = u32::try_from(bytes.len() - 4)?;
        bytes[..4].copy_from_slice(&size.to_le_bytes());
        let decoded = decode_protocol_frame(&bytes).ok();
        let target = matches!(
            decoded.as_ref(),
            Some(ProtocolMessage::Event(Event {
                payload: EventPayload::ChooseBuffer { .. }
                    | EventPayload::ChooseBufferUpdate { .. },
                ..
            }))
        );
        let status = if refusal || decoded.is_none() {
            0
        } else if target {
            1
        } else {
            2
        };
        cases.push(json!({"bytes":bytes,"status":status,"refusal":refusal,"expected":if status==1 { decoded } else { None }}));
        Ok(())
    };
    for kind in [5, 6, 7, 10, 11] {
        let base = encode_protocol_message(&operation(kind))?;
        record(base.clone(), false)?;
        for length in 11..base.len() {
            record(base[..length].to_vec(), false)?;
        }
        for offset in 11..base.len() {
            for value in [0, 2, 127, 255] {
                let mut bytes = base.clone();
                bytes[offset] = value;
                record(bytes, false)?;
            }
        }
        let mut bytes = base;
        bytes.extend([0, 255, 127]);
        record(bytes, false)?;
    }
    for field in 0..4 {
        let maximum = match field {
            0 => 64,
            1 => 512,
            _ => 1024 * 1024,
        };
        for size in [maximum, maximum + 1] {
            let ProtocolMessage::Event(mut event) = operation(6) else {
                unreachable!()
            };
            let EventPayload::ChooseBuffer { state: Some(state) } = &mut event.payload else {
                unreachable!()
            };
            let value = "x".repeat(size);
            match field {
                0 => state.items[0].key = value,
                1 => state.items[0].text = value,
                2 => state.items[0].name = value,
                _ => state.prompt = value,
            }
            record(
                encode_protocol_message(&ProtocolMessage::Event(event))?,
                field >= 2 && size > maximum,
            )?;
        }
    }
    for count in [65536, 65537] {
        let ProtocolMessage::Event(mut event) = operation(6) else {
            unreachable!()
        };
        let EventPayload::ChooseBuffer { state: Some(state) } = &mut event.payload else {
            unreachable!()
        };
        state.items = vec![
            ChooseBufferItem {
                name: String::new(),
                preview: String::new(),
                size_bytes: 0,
                created_unix_seconds: 0,
                key: String::new(),
                text: String::new(),
                tagged: false
            };
            count
        ];
        record(
            encode_protocol_message(&ProtocolMessage::Event(event))?,
            count > 65536,
        )?;
    }
    Ok(cases)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut traces = Vec::new();
    for initial in [0_u64, u64::MAX - 1, u64::MAX] {
        let mut core = ClientCore::new();
        let mut revision = initial;
        let mut closed = initial;
        let mut buffer_revision = initial;
        let mut cases = Vec::new();
        let mut push = |message: ProtocolMessage| -> Result<(), Box<dyn Error>> {
            let frame = encode_protocol_message(&message)?;
            core.handle_message(message);
            while let Some(event) = core.poll_event() {
                match event {
                    CoreEvent::ChooseTreeChanged => {
                        revision = next_chooser_revision(revision);
                        if core.choose_tree().is_none() {
                            closed = revision;
                        }
                    }
                    CoreEvent::ChooseBufferChanged => {
                        buffer_revision = next_chooser_revision(buffer_revision);
                    }
                    _ => {}
                }
            }
            cases.push(json!({"bytes":frame,"revision":revision.to_string(),"closed":closed.to_string(),"buffer_revision":buffer_revision.to_string(),"tree":tree_value(&core),"buffer":core.choose_buffer(),"plane":plane_value(&core)}));
            Ok(())
        };
        for kind in 0..12 {
            push(operation(kind))?;
        }
        for first in 0..12 {
            for second in 0..12 {
                push(ProtocolMessage::Batch(Batch::from_messages(
                    0,
                    [operation(first), operation(second)],
                )?))?;
            }
        }
        for sequence in [
            vec![1, 9, 0, 1],
            vec![0, 9, 6, 9],
            vec![1, 0, 9, 6, 9, 1, 9],
            vec![0, 5, 9, 2, 4, 8, 9, 0],
            vec![6, 11, 9, 5, 9],
            vec![1, 9, 1, 9, 0, 9],
            vec![9, 1, 9, 5, 9, 0, 9],
            vec![2, 9, 3, 9, 4, 8, 0],
        ] {
            push(ProtocolMessage::Batch(Batch::from_messages(
                128,
                sequence.into_iter().map(operation),
            )?))?;
        }
        traces.push(json!({"initial":initial.to_string(),"cases":cases}));
    }
    let path = std::env::args().nth(1).ok_or("missing output path")?;
    std::fs::write(
        path,
        serde_json::to_vec(&json!({"traces":traces,"codec":buffer_cases()?}))?,
    )?;
    Ok(())
}
