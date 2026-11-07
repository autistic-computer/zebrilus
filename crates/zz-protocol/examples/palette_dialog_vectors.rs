use std::error::Error;

use serde_json::{Value, json};
use zz_protocol::{
    ChooseTreeAction, ChooseTreeItem, ChooseTreeKind, ChooseTreePaneKind, ChooseTreeSearchState,
    ChooseTreeState, ChooseTreeTarget, ClientId, CommandPromptAction, CommandPromptKind,
    CommandPromptMode, CommandPromptState, CommandPromptType, Event, EventPayload, InputMessage,
    PaneId, PromptCursor, ProtocolMessage, SessionId, TmuxColour, WindowId, decode_protocol_frame,
    encode_protocol_message,
};
use zz_terminal::{KeyAction, KeyCode, KeyInput, Modifiers};

fn frame(message: &ProtocolMessage) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = encode_protocol_message(&ProtocolMessage::TreeSync)?;
    bytes.truncate(8);
    bytes.extend(postcard::to_stdvec(message)?);
    let length = u32::try_from(bytes.len() - 4)?;
    bytes[..4].copy_from_slice(&length.to_le_bytes());
    Ok(bytes)
}

fn record(name: &str, kind: u8, bytes: Vec<u8>, refusal: bool) -> Result<Value, Box<dyn Error>> {
    let decoded = decode_protocol_frame(&bytes);
    let expected = decoded
        .ok()
        .map(|value| encode_protocol_message(&value))
        .transpose()?;
    let mut result = json!({"name":name,"kind":kind,"expected":expected,"refusal":refusal});
    result["bytes"] = Value::Array(bytes.into_iter().map(Value::from).collect());
    Ok(result)
}

fn event(payload: EventPayload) -> ProtocolMessage {
    ProtocolMessage::Event(Event {
        sequence: u64::MAX,
        payload,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    for kind in [CommandPromptKind::Command, CommandPromptKind::Value] {
        for prompt_type in [CommandPromptType::Command, CommandPromptType::Search] {
            for mode in [
                CommandPromptMode::Text,
                CommandPromptMode::Single,
                CommandPromptMode::Numeric,
                CommandPromptMode::Incremental,
                CommandPromptMode::Key,
                CommandPromptMode::BackspaceExit,
            ] {
                for flags in 0_u8..8 {
                    let state = CommandPromptState {
                        prompt: "prompt λ\0\n".to_owned(),
                        input: "input 世界 🦀".to_owned(),
                        cursor: if flags & 1 == 0 { 0 } else { u32::MAX },
                        kind,
                        history: vec![String::new(), "first\nsecond".to_owned(), "é🦀".to_owned()],
                        prompt_type,
                        mode,
                        no_freeze: flags & 1 != 0,
                        pane: (flags & 2 != 0).then_some(PaneId(if flags & 1 == 0 {
                            0
                        } else {
                            u64::MAX
                        })),
                        command_mode: flags & 4 != 0,
                        prompt_cursor: PromptCursor {
                            style: flags * 31,
                            colour: match flags {
                                0 => None,
                                1 => Some(TmuxColour::Basic(255)),
                                2 => Some(TmuxColour::Indexed(0)),
                                3 => Some(TmuxColour::Rgb(0xff_ffff)),
                                4 => Some(TmuxColour::Default),
                                5 => Some(TmuxColour::Terminal),
                                _ => Some(TmuxColour::Theme(255)),
                            },
                        },
                    };
                    cases.push(record(
                        "prompt-state",
                        0,
                        frame(&event(EventPayload::CommandPrompt { state: Some(state) }))?,
                        false,
                    )?);
                }
            }
        }
    }
    cases.push(record(
        "prompt-close",
        0,
        frame(&event(EventPayload::CommandPrompt { state: None }))?,
        false,
    )?);
    for input in ["", "echo λ\0\n世界", "🦀"] {
        for cursor in [0, 1, u32::MAX] {
            cases.push(record(
                "prompt-update",
                1,
                frame(&ProtocolMessage::Input(InputMessage::CommandPrompt {
                    action: CommandPromptAction::Update {
                        input: input.to_owned(),
                        cursor,
                    },
                }))?,
                false,
            )?);
        }
        cases.push(record(
            "prompt-submit",
            1,
            frame(&ProtocolMessage::Input(InputMessage::CommandPrompt {
                action: CommandPromptAction::Submit {
                    input: input.to_owned(),
                },
            }))?,
            false,
        )?);
    }
    cases.push(record(
        "prompt-action-close",
        1,
        frame(&ProtocolMessage::Input(InputMessage::CommandPrompt {
            action: CommandPromptAction::Close,
        }))?,
        false,
    )?);
    for kind in [
        ChooseTreeKind::Windows,
        ChooseTreeKind::Panes,
        ChooseTreeKind::Clients,
    ] {
        for flags in 0_u8..=255 {
            let targets = [
                ChooseTreeTarget::Session(SessionId(0)),
                ChooseTreeTarget::Window(WindowId(u64::MAX)),
                ChooseTreeTarget::Pane(PaneId(128)),
                ChooseTreeTarget::Client(ClientId(16384)),
            ];
            let items = targets
                .into_iter()
                .enumerate()
                .map(|(index, target)| ChooseTreeItem {
                    label: format!("row {index} λ"),
                    detail: "detail\0\n世界".to_owned(),
                    target,
                    depth: flags,
                    flags,
                    pane_kind: match (usize::from(flags) + index) % 5 {
                        0 => None,
                        1 => Some(ChooseTreePaneKind::Terminal),
                        2 => Some(ChooseTreePaneKind::Browser),
                        3 => Some(ChooseTreePaneKind::Editor),
                        _ => Some(ChooseTreePaneKind::Agent),
                    },
                    key: "k".repeat(if flags == 255 { 64 } else { index }),
                    text: "t".repeat(if flags == 255 { 512 } else { index }),
                })
                .collect();
            let state = ChooseTreeState {
                items,
                search: (flags & 1 != 0).then(|| ChooseTreeSearchState {
                    query: "query é🦀".to_owned(),
                    reverse: flags & 2 != 0,
                }),
                selected: if flags & 1 == 0 { u32::MAX } else { 0 },
                kind,
                filter_no_matches: flags & 4 != 0,
                prompt: "prompt".repeat(if flags == 255 { 85 } else { 1 }),
                help: flags & 8 != 0,
            };
            cases.push(record(
                "chooser-state",
                2,
                frame(&event(EventPayload::ChooseTree { state: Some(state) }))?,
                false,
            )?);
        }
    }
    cases.push(record(
        "chooser-close",
        2,
        frame(&event(EventPayload::ChooseTree { state: None }))?,
        false,
    )?);
    for selected in [0, 1, 128, u32::MAX] {
        for search in [
            None,
            Some(ChooseTreeSearchState {
                query: String::new(),
                reverse: false,
            }),
            Some(ChooseTreeSearchState {
                query: "世界".to_owned(),
                reverse: true,
            }),
        ] {
            cases.push(record(
                "chooser-update",
                2,
                frame(&event(EventPayload::ChooseTreeUpdate { search, selected }))?,
                false,
            )?);
        }
    }
    let actions = vec![
        ChooseTreeAction::Previous,
        ChooseTreeAction::Next,
        ChooseTreeAction::PagePrevious,
        ChooseTreeAction::PageNext,
        ChooseTreeAction::First,
        ChooseTreeAction::Last,
        ChooseTreeAction::Collapse,
        ChooseTreeAction::Expand,
        ChooseTreeAction::Activate,
        ChooseTreeAction::Select(u32::MAX),
        ChooseTreeAction::ActivateIndex(0),
        ChooseTreeAction::SearchStart { reverse: true },
        ChooseTreeAction::SearchAppend("é🦀\0".to_owned()),
        ChooseTreeAction::SearchBackspace,
        ChooseTreeAction::SearchAccept,
        ChooseTreeAction::SearchCancel,
        ChooseTreeAction::SearchNext { reverse: false },
        ChooseTreeAction::Close,
        ChooseTreeAction::Key(KeyInput {
            action: KeyAction::Press,
            key: KeyCode::Enter,
            modifiers: Modifiers::default(),
            text: None,
            unshifted_codepoint: None,
        }),
        ChooseTreeAction::Tag,
        ChooseTreeAction::TagNone,
        ChooseTreeAction::TagAll,
        ChooseTreeAction::KillCurrent,
        ChooseTreeAction::KillTagged,
        ChooseTreeAction::SwapUp,
        ChooseTreeAction::SwapDown,
        ChooseTreeAction::SortNext,
        ChooseTreeAction::SortReverse,
        ChooseTreeAction::Help,
        ChooseTreeAction::CollapseAll,
        ChooseTreeAction::ExpandAll,
        ChooseTreeAction::Mark,
        ChooseTreeAction::MarkClear,
        ChooseTreeAction::CommandPrompt,
        ChooseTreeAction::PreviewCycle,
        ChooseTreeAction::FilterPrompt,
        ChooseTreeAction::ClearFilter,
        ChooseTreeAction::ClientDetach,
        ChooseTreeAction::ClientDetachTagged,
        ChooseTreeAction::ClientInfo,
    ];
    for action in actions {
        cases.push(record(
            "chooser-action",
            3,
            frame(&ProtocolMessage::Input(InputMessage::ChooseTree { action }))?,
            false,
        )?);
    }
    for key in [
        KeyCode::Character('λ'),
        KeyCode::Character('🦀'),
        KeyCode::Function(255),
        KeyCode::User(u16::MAX),
    ] {
        for action in [KeyAction::Press, KeyAction::Repeat, KeyAction::Release] {
            for bits in 0..16 {
                let input = KeyInput {
                    action,
                    key,
                    modifiers: Modifiers::from_bits(bits).ok_or("modifiers")?,
                    text: Some("text 世界".into()),
                    unshifted_codepoint: Some('é'),
                };
                cases.push(record(
                    "chooser-key",
                    3,
                    frame(&ProtocolMessage::Input(InputMessage::ChooseTree {
                        action: ChooseTreeAction::Key(input),
                    }))?,
                    false,
                )?);
            }
        }
    }
    for count in [4096, 4097] {
        let state = CommandPromptState {
            prompt: String::new(),
            input: String::new(),
            cursor: 0,
            kind: CommandPromptKind::Command,
            history: vec![String::new(); count],
            prompt_type: CommandPromptType::Command,
            mode: CommandPromptMode::Text,
            no_freeze: false,
            pane: None,
            command_mode: false,
            prompt_cursor: PromptCursor::default(),
        };
        cases.push(record(
            "prompt-history-bound",
            0,
            frame(&event(EventPayload::CommandPrompt { state: Some(state) }))?,
            count > 4096,
        )?);
    }
    for count in [65536, 65537] {
        let item = ChooseTreeItem {
            label: String::new(),
            detail: String::new(),
            target: ChooseTreeTarget::Session(SessionId(0)),
            depth: 0,
            flags: 0,
            pane_kind: None,
            key: String::new(),
            text: String::new(),
        };
        let state = ChooseTreeState {
            items: vec![item; count],
            search: None,
            selected: 0,
            kind: ChooseTreeKind::Windows,
            filter_no_matches: false,
            prompt: String::new(),
            help: false,
        };
        cases.push(record(
            "chooser-items-bound",
            2,
            frame(&event(EventPayload::ChooseTree { state: Some(state) }))?,
            count > 65536,
        )?);
    }
    for size in [1024 * 1024, 1024 * 1024 + 1] {
        cases.push(record(
            "prompt-text-bound",
            1,
            frame(&ProtocolMessage::Input(InputMessage::CommandPrompt {
                action: CommandPromptAction::Submit {
                    input: "x".repeat(size),
                },
            }))?,
            size > 1024 * 1024,
        )?);
        cases.push(record(
            "chooser-search-bound",
            3,
            frame(&ProtocolMessage::Input(InputMessage::ChooseTree {
                action: ChooseTreeAction::SearchAppend("x".repeat(size)),
            }))?,
            size > 1024 * 1024,
        )?);
    }
    for (kind, original) in [
        (0, cases[0]["bytes"].clone()),
        (2, cases[206]["bytes"].clone()),
        (3, cases[cases.len() - 9]["bytes"].clone()),
    ] {
        let original: Vec<u8> = serde_json::from_value(original)?;
        for length in 8..original.len() {
            let mut bytes = original[..length].to_vec();
            bytes[..4].copy_from_slice(&u32::try_from(length - 4)?.to_le_bytes());
            cases.push(record("truncated", kind, bytes, false)?);
        }
    }
    println!("{}", json!({"cases":cases}));
    Ok(())
}
