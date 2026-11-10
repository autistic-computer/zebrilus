use std::error::Error;

use serde_json::{Value, json};
use zz_protocol::{
    Event, EventPayload, InputMessage, MuxOptionKey, MuxOptionSource, MuxOptionValue, MuxOptions,
    PaneId, ProtocolMessage, decode_protocol_frame, encode_protocol_message,
};
use zz_terminal::{
    KeyAction, KeyCode, KeyInput, Modifiers, PointerCellEvent, TerminalMouseButton,
    TerminalMouseInput, TerminalMousePhase, TerminalViewAction,
};

fn uint(output: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        output.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    output.push(value as u8);
}

fn optional_text(output: &mut Vec<u8>, text: Option<&str>) {
    uint(output, u64::from(text.is_some()));
    if let Some(text) = text {
        uint(output, text.len() as u64);
        output.extend_from_slice(text.as_bytes());
    }
}

fn cell_request(output: &mut Vec<u8>, cell: PointerCellEvent) {
    for value in [
        u64::from(cell.column),
        u64::from(cell.row),
        u64::from(cell.click_count),
        u64::from(cell.rectangle),
    ] {
        uint(output, value);
    }
}

fn mouse_request(output: &mut Vec<u8>, input: TerminalMouseInput) {
    uint(output, input.phase() as u64);
    uint(output, input.button().map_or(0, |button| button as u64 + 1));
    cell_request(output, input.cell);
    for value in [
        input.x,
        input.y,
        input.screen_width,
        input.screen_height,
        input.cell_width,
        input.cell_height,
    ] {
        uint(output, u64::from(value));
    }
    uint(output, u64::from(input.modifiers().bits()));
    uint(output, u64::from(input.force_selection()));
}

fn lines(output: &mut Vec<u8>, value: i32) {
    uint(
        output,
        u64::from(((value as u32) << 1) ^ ((value >> 31) as u32)),
    );
}

fn request(message: &InputMessage) -> Vec<u8> {
    let mut output = Vec::new();
    match message {
        InputMessage::Text { pane, text } | InputMessage::BrowserSurfaceText { pane, text } => {
            uint(
                &mut output,
                if matches!(message, InputMessage::Text { .. }) {
                    0
                } else {
                    2
                },
            );
            uint(&mut output, pane.0);
            uint(&mut output, text.len() as u64);
            output.extend_from_slice(text.as_bytes());
        }
        InputMessage::Key {
            pane,
            input,
            text_follows,
        }
        | InputMessage::BrowserSurfaceKey {
            pane,
            input,
            text_follows,
        } => {
            uint(
                &mut output,
                if matches!(message, InputMessage::Key { .. }) {
                    1
                } else {
                    3
                },
            );
            uint(&mut output, pane.0);
            uint(&mut output, input.action as u64);
            let (kind, character, value) = match input.key {
                KeyCode::Character(c) => (0, u64::from(c), 0),
                KeyCode::Backspace => (1, 0, 0),
                KeyCode::Enter => (2, 0, 0),
                KeyCode::Tab => (3, 0, 0),
                KeyCode::Escape => (4, 0, 0),
                KeyCode::Delete => (5, 0, 0),
                KeyCode::Insert => (6, 0, 0),
                KeyCode::Home => (7, 0, 0),
                KeyCode::End => (8, 0, 0),
                KeyCode::PageUp => (9, 0, 0),
                KeyCode::PageDown => (10, 0, 0),
                KeyCode::ArrowUp => (11, 0, 0),
                KeyCode::ArrowDown => (12, 0, 0),
                KeyCode::ArrowLeft => (13, 0, 0),
                KeyCode::ArrowRight => (14, 0, 0),
                KeyCode::Function(value) => (15, 0, u64::from(value)),
                KeyCode::Unidentified => (16, 0, 0),
                KeyCode::User(value) => (17, 0, u64::from(value)),
            };
            for value in [kind, character, value, u64::from(input.modifiers.bits())] {
                uint(&mut output, value);
            }
            optional_text(&mut output, input.text.as_deref());
            uint(&mut output, u64::from(input.unshifted_codepoint.is_some()));
            if let Some(value) = input.unshifted_codepoint {
                uint(&mut output, u64::from(value));
            }
            uint(&mut output, u64::from(*text_follows));
        }
        InputMessage::TerminalView { action, .. } | InputMessage::CommandOutputView { action } => {
            if let InputMessage::TerminalView { pane, .. } = message {
                uint(&mut output, 5);
                uint(&mut output, pane.0);
            } else {
                uint(&mut output, 7);
                uint(&mut output, 0);
            }
            match action {
                TerminalViewAction::ScrollLines(value) => {
                    uint(&mut output, 0);
                    lines(&mut output, *value);
                }
                TerminalViewAction::ScrollPages(value) => {
                    uint(&mut output, 1);
                    lines(&mut output, *value);
                }
                TerminalViewAction::ScrollTop => uint(&mut output, 2),
                TerminalViewAction::ScrollBottom => uint(&mut output, 3),
                TerminalViewAction::ScrollToFraction(value) => {
                    uint(&mut output, 4);
                    uint(&mut output, u64::from(*value));
                }
                TerminalViewAction::SelectionPress(cell) => {
                    uint(&mut output, 5);
                    cell_request(&mut output, *cell);
                }
                TerminalViewAction::SelectionDrag(cell) => {
                    uint(&mut output, 6);
                    cell_request(&mut output, *cell);
                }
                TerminalViewAction::SelectionAutoscroll {
                    lines: count,
                    pointer,
                } => {
                    uint(&mut output, 7);
                    lines(&mut output, *count);
                    cell_request(&mut output, *pointer);
                }
                TerminalViewAction::SelectionRelease(cell) => {
                    uint(&mut output, 8);
                    cell_request(&mut output, *cell);
                }
                TerminalViewAction::Mouse(input) => {
                    uint(&mut output, 9);
                    mouse_request(&mut output, *input);
                }
                TerminalViewAction::ClearLinkHover => uint(&mut output, 10),
                TerminalViewAction::ScrollWheel {
                    lines: count,
                    input,
                } => {
                    uint(&mut output, 11);
                    lines(&mut output, *count);
                    mouse_request(&mut output, *input);
                }
                TerminalViewAction::SelectAll => uint(&mut output, 12),
                TerminalViewAction::ClearHistory => uint(&mut output, 13),
                TerminalViewAction::ClearSelection => uint(&mut output, 14),
                TerminalViewAction::EnterCopyMode => uint(&mut output, 15),
                _ => unreachable!("fixture only includes supported input actions"),
            }
        }
        _ => unreachable!("fixture only includes text, key and terminal view inputs"),
    }
    output
}

fn case(name: &str, input: InputMessage) -> Result<Value, Box<dyn Error>> {
    Ok(
        json!({"name": name, "request": request(&input), "bytes": encode_protocol_message(&ProtocolMessage::Input(input))?}),
    )
}

fn mapping_case(
    name: &str,
    args: [u64; 5],
    supplied: Option<&str>,
    expected: KeyInput,
) -> Result<Value, Box<dyn Error>> {
    let mut request = Vec::new();
    for value in args {
        uint(&mut request, value);
    }
    optional_text(&mut request, supplied);
    Ok(
        json!({"name":name,"request":request,"bytes":encode_protocol_message(&ProtocolMessage::Input(InputMessage::Key { pane: PaneId(7), input: expected, text_follows:false }))?}),
    )
}

fn routing_boundaries() -> Result<Vec<Value>, Box<dyn Error>> {
    let mut full = MuxOptions::default();
    full.set(MuxOptionKey::Prefix, "a", MuxOptionSource::RuntimeCommand);
    let sparse = MuxOptions::from_entries([(
        MuxOptionKey::Prefix,
        MuxOptionValue {
            value: "Ctrl-a".to_owned(),
            source: MuxOptionSource::Override,
        },
    )]);
    let mut output = Vec::new();
    for (name, options, patched) in [
        ("full-options", full.clone(), false),
        ("sparse-full-rejected", sparse.clone(), false),
        ("sparse-patched", sparse, true),
    ] {
        let payload = if patched {
            EventPayload::MuxOptionsPatched { options }
        } else {
            EventPayload::MuxOptionsChanged { options }
        };
        let message = ProtocolMessage::Event(Event {
            sequence: 7,
            payload,
        });
        let mut bytes = encode_protocol_message(&ProtocolMessage::TreeSync)?;
        bytes.truncate(8);
        bytes.extend(postcard::to_stdvec(&message)?);
        let length = u32::try_from(bytes.len() - 4)?;
        bytes[..4].copy_from_slice(&length.to_le_bytes());
        output.push(routing_boundary(name, &bytes));
    }
    let mut duplicate = encode_protocol_message(&ProtocolMessage::Event(Event {
        sequence: 7,
        payload: EventPayload::MuxOptionsChanged { options: full },
    }))?;
    duplicate[11] = 22;
    duplicate.extend(postcard::to_stdvec(&(
        MuxOptionKey::Prefix,
        MuxOptionValue {
            value: "M-b".to_owned(),
            source: MuxOptionSource::Override,
        },
    ))?);
    let length = u32::try_from(duplicate.len() - 4)?;
    duplicate[..4].copy_from_slice(&length.to_le_bytes());
    output.push(routing_boundary("full-duplicate-key-last-wins", &duplicate));
    Ok(output)
}

fn routing_boundary(name: &str, bytes: &[u8]) -> Value {
    match decode_protocol_frame(bytes) {
        Err(error) => {
            json!({"name":name,"bytes":bytes,"accepted":false,"error":error.to_string()})
        }
        Ok(ProtocolMessage::Event(Event { sequence, payload })) => {
            let (kind, options) = match payload {
                EventPayload::MuxOptionsChanged { options } => (2, options),
                EventPayload::MuxOptionsPatched { options } => (60, options),
                _ => unreachable!("routing fixture event"),
            };
            let fields = options
                .iter()
                .map(|(key, value)| json!([key as u8, value.value, value.source as u8]))
                .collect::<Vec<_>>();
            json!({"name":name,"bytes":bytes,"accepted":true,"kind":kind,"sequence":sequence,"options":fields})
        }
        Ok(_) => unreachable!("routing fixture message"),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let actions = [KeyAction::Press, KeyAction::Repeat, KeyAction::Release];
    let keys = [
        KeyCode::Character('🦀'),
        KeyCode::Backspace,
        KeyCode::Enter,
        KeyCode::Tab,
        KeyCode::Escape,
        KeyCode::Delete,
        KeyCode::Insert,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::Function(255),
        KeyCode::Unidentified,
        KeyCode::User(u16::MAX),
    ];
    let mut cases = Vec::new();
    for action in actions {
        for key in keys {
            for bits in 0..16 {
                let input = KeyInput {
                    action,
                    key,
                    modifiers: Modifiers::from_bits(bits).expect("four fixture flags"),
                    text: (bits % 3 != 0).then(|| if bits % 3 == 1 { "é🦀" } else { "" }.into()),
                    unshifted_codepoint: (bits % 2 == 1).then_some('é'),
                };
                let message = if bits % 2 == 0 {
                    InputMessage::Key {
                        pane: PaneId(u64::MAX),
                        input,
                        text_follows: bits % 4 == 0,
                    }
                } else {
                    InputMessage::BrowserSurfaceKey {
                        pane: PaneId(128),
                        input,
                        text_follows: bits % 3 == 0,
                    }
                };
                cases.push(case(&format!("key-{action:?}-{key:?}-{bits}"), message)?);
            }
        }
    }
    for (name, text) in [
        ("empty", String::new()),
        ("ascii", "hello".to_owned()),
        ("scalars", "é🦀\n\u{1b}[200~".to_owned()),
        ("two-byte-length", "zz ".repeat(100)),
    ] {
        cases.push(case(
            &format!("text-{name}"),
            InputMessage::Text {
                pane: PaneId(u64::MAX),
                text: text.clone(),
            },
        )?);
        cases.push(case(
            &format!("browser-text-{name}"),
            InputMessage::BrowserSurfaceText {
                pane: PaneId(128),
                text,
            },
        )?);
    }
    let cell = PointerCellEvent {
        column: u16::MAX,
        row: 128,
        click_count: u8::MAX,
        rectangle: true,
    };
    for phase in [
        TerminalMousePhase::Press,
        TerminalMousePhase::Release,
        TerminalMousePhase::Motion,
    ] {
        for button in [
            None,
            Some(TerminalMouseButton::Left),
            Some(TerminalMouseButton::Middle),
            Some(TerminalMouseButton::Right),
            Some(TerminalMouseButton::ScrollUp),
            Some(TerminalMouseButton::ScrollDown),
        ] {
            for bits in 0..16 {
                let input = TerminalMouseInput::new(
                    phase,
                    button,
                    cell,
                    u32::MAX,
                    128,
                    65535,
                    800,
                    9,
                    18,
                    Modifiers::from_bits(bits).expect("four fixture flags"),
                    bits % 2 == 1,
                );
                cases.push(case(
                    &format!("mouse-{phase:?}-{button:?}-{bits}"),
                    InputMessage::TerminalView {
                        pane: PaneId(u64::MAX),
                        action: TerminalViewAction::Mouse(input),
                    },
                )?);
                if bits == 0 {
                    for lines in [i32::MIN, -1, 0, 1, i32::MAX] {
                        cases.push(case(
                            &format!("wheel-{phase:?}-{button:?}-{lines}"),
                            InputMessage::TerminalView {
                                pane: PaneId(0),
                                action: TerminalViewAction::ScrollWheel { lines, input },
                            },
                        )?);
                    }
                }
            }
        }
    }
    for action in [
        TerminalViewAction::ScrollLines(i32::MIN),
        TerminalViewAction::ScrollPages(i32::MAX),
        TerminalViewAction::ScrollTop,
        TerminalViewAction::ScrollBottom,
        TerminalViewAction::ScrollToFraction(u32::MAX),
        TerminalViewAction::SelectionPress(cell),
        TerminalViewAction::SelectionDrag(cell),
        TerminalViewAction::SelectionAutoscroll {
            lines: i32::MIN,
            pointer: cell,
        },
        TerminalViewAction::SelectionRelease(cell),
        TerminalViewAction::ClearLinkHover,
        TerminalViewAction::SelectAll,
        TerminalViewAction::ClearHistory,
        TerminalViewAction::ClearSelection,
        TerminalViewAction::EnterCopyMode,
    ] {
        cases.push(case(
            &format!("view-{action:?}"),
            InputMessage::TerminalView {
                pane: PaneId(300),
                action,
            },
        )?);
    }
    let mut mappings = Vec::new();
    for (index, key) in [
        KeyCode::Enter,
        KeyCode::Tab,
        KeyCode::Backspace,
        KeyCode::Escape,
        KeyCode::Delete,
        KeyCode::Insert,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
    ]
    .into_iter()
    .enumerate()
    {
        for action in actions {
            for bits in [0, 1, 2, 4, 8, 15] {
                mappings.push(mapping_case(
                    &format!("named-{index}-{action:?}-{bits}"),
                    [
                        0x100 + index as u64,
                        0,
                        0,
                        u64::from(bits) << 17,
                        action as u64,
                    ],
                    None,
                    KeyInput {
                        action,
                        key,
                        modifiers: Modifiers::from_bits(bits).expect("four fixture flags"),
                        text: None,
                        unshifted_codepoint: None,
                    },
                )?);
            }
        }
    }
    for number in 1..=24 {
        mappings.push(mapping_case(
            &format!("function-{number}"),
            [0x180 + u64::from(number) - 1, 0, 0, 0, 0],
            None,
            KeyInput {
                action: KeyAction::Press,
                key: KeyCode::Function(number),
                modifiers: Modifiers::default(),
                text: None,
                unshifted_codepoint: None,
            },
        )?);
    }
    for (name, key, ch, unshifted, character, text, supplied) in [
        ("ascii", 97, 97, 97, 'a', Some("a"), None),
        ("shift", 97, 65, 97, 'a', Some("A"), None),
        ("non-ascii", 97, 233, 233, 'é', Some("é"), None),
        ("space", 32, 32, 32, ' ', Some(" "), None),
        ("control-char", 97, 1, 97, 'a', None, None),
        ("control-text", 97, 97, 97, 'a', None, Some("a\n")),
        (
            "composed",
            97,
            97,
            97,
            'a',
            Some("e\u{301}"),
            Some("e\u{301}"),
        ),
        ("empty-text", 97, 97, 97, 'a', Some(""), Some("")),
    ] {
        for action in actions {
            for bits in [0, 1, 2, 4, 8, 15] {
                mappings.push(mapping_case(
                    &format!("{name}-{action:?}-{bits}"),
                    [key, ch, unshifted, u64::from(bits) << 17, action as u64],
                    supplied,
                    KeyInput {
                        action,
                        key: KeyCode::Character(character),
                        modifiers: Modifiers::from_bits(
                            if bits & 1 != 0 && !character.is_ascii_lowercase() {
                                bits & !1
                            } else {
                                bits
                            },
                        )
                        .expect("four fixture flags"),
                        text: text.map(Into::into),
                        unshifted_codepoint: Some(character),
                    },
                )?);
            }
        }
    }
    for (name, key, ch, plain, character) in [
        ("percent", 53, 37, 53, '%'),
        ("quote", 39, 34, 39, '"'),
        ("plus", 61, 43, 61, '+'),
    ] {
        for bits in [1, 3, 5, 9, 15] {
            mappings.push(mapping_case(
                name,
                [key, ch, plain, u64::from(bits) << 17, 0],
                None,
                KeyInput {
                    action: KeyAction::Press,
                    key: KeyCode::Character(character),
                    modifiers: Modifiers::from_bits(bits & !1).expect("four fixture flags"),
                    text: Some(character.to_string().into_boxed_str()),
                    unshifted_codepoint: Some(character),
                },
            )?);
        }
    }
    let base = encode_protocol_message(&ProtocolMessage::Input(InputMessage::Key {
        pane: PaneId(0),
        input: KeyInput {
            action: KeyAction::Press,
            key: KeyCode::Enter,
            modifiers: Modifiers::default(),
            text: None,
            unshifted_codepoint: None,
        },
        text_follows: false,
    }))?;
    let mut rejected = Vec::new();
    for (name, index, value) in [
        ("key-action", 11, 3),
        ("key-code", 12, 18),
        ("modifiers", 13, 16),
        ("text-option", 14, 2),
        ("unshifted-option", 15, 2),
        ("text-follows", 16, 2),
    ] {
        let mut bytes = base.clone();
        bytes[index] = value;
        if decode_protocol_frame(&bytes).is_err() {
            rejected.push(json!({"name":name,"bytes":bytes}));
        }
    }
    for end in 8..base.len() {
        let mut bytes = base[..end].to_vec();
        let length = u32::try_from(bytes.len() - 4)?;
        bytes[..4].copy_from_slice(&length.to_le_bytes());
        if decode_protocol_frame(&bytes).is_err() {
            rejected.push(json!({"name":format!("truncated-{end}"),"bytes":bytes}));
        }
    }
    let character = encode_protocol_message(&ProtocolMessage::Input(InputMessage::Key {
        pane: PaneId(0),
        input: KeyInput {
            action: KeyAction::Press,
            key: KeyCode::Character('a'),
            modifiers: Modifiers::default(),
            text: None,
            unshifted_codepoint: None,
        },
        text_follows: false,
    }))?;
    let mut accepted = Vec::new();
    for (name, replacement) in [
        ("multi-scalar-character", vec![2, b'a', b'b']),
        ("empty-character", vec![0]),
        ("oversized-character", vec![5, b'a', b'b', b'c', b'd', b'e']),
        ("invalid-character-utf8", vec![1, 255]),
        ("surrogate-character", vec![3, 0xed, 0xa0, 0x80]),
    ] {
        let mut bytes = character.clone();
        bytes.splice(13..15, replacement);
        let length = u32::try_from(bytes.len() - 4)?;
        bytes[..4].copy_from_slice(&length.to_le_bytes());
        match decode_protocol_frame(&bytes) {
            Ok(message) => accepted.push(
                json!({"name":name,"source":bytes,"bytes":encode_protocol_message(&message)?}),
            ),
            Err(_) => rejected.push(json!({"name":name,"bytes":bytes})),
        }
    }
    let mut trailing = base.clone();
    trailing.push(255);
    let length = u32::try_from(trailing.len() - 4)?;
    trailing[..4].copy_from_slice(&length.to_le_bytes());
    if let Ok(message) = decode_protocol_frame(&trailing) {
        accepted.push(json!({"name":"trailing-control-byte","source":trailing,"bytes":encode_protocol_message(&message)?}));
    }
    let mut output_cases = Vec::new();
    for value in &cases {
        let bytes: Vec<u8> = serde_json::from_value(value["bytes"].clone())?;
        if let ProtocolMessage::Input(InputMessage::TerminalView { action, .. }) =
            decode_protocol_frame(&bytes)?
        {
            output_cases.push(case(
                &format!("output-{}", value["name"].as_str().unwrap_or_default()),
                InputMessage::CommandOutputView { action },
            )?);
        }
    }
    cases.extend(output_cases);
    println!(
        "{}",
        json!({"reference":"actual zz-protocol native input serde and zz-terminal input contract","cases":cases,"mappings":mappings,"rejected":rejected,"routing_boundaries":routing_boundaries()?,"accepted":accepted})
    );
    Ok(())
}
