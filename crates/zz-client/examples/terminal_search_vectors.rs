use serde_json::{Value, json};
use zz_client::terminal_view::{
    mode_text, search_case_label, search_key_action, search_result_text, terminal_shortcut,
};
use zz_client::{ChromeAction, ChromeKeymap, ChromeProfile};
use zz_protocol::{
    Event, EventPayload, InputMessage, PaneId, ProtocolMessage, TerminalUiCommand,
    encode_protocol_message,
};
use zz_terminal::{
    CopyJump, CopyJumpDirection, CopyModeAction, CopyModeCopy, CopyModeSearch, CopySelectionMode,
    KeyAction, KeyCode, KeyInput, Modifiers, PasteBufferAction, PointerCellEvent, SearchCase,
    SearchDirection, SearchMode, SearchQuery, SearchStatus, TerminalMode, TerminalViewAction,
};

fn wire(pane: PaneId, output: bool, action: TerminalViewAction) -> Value {
    let args = serde_json::to_value(&action).expect("terminal action has JSON representation");
    let input = if output {
        InputMessage::CommandOutputView { action }
    } else {
        InputMessage::TerminalView { pane, action }
    };
    let expected = encode_protocol_message(&ProtocolMessage::Input(input))
        .expect("terminal search action encodes");
    json!({"kind":"wire", "pane":pane.0, "output":output, "action":args, "expected":expected})
}

fn copy_request(index: u8) -> CopyModeCopy {
    CopyModeCopy {
        request_id: if index == 0 { 0 } else { u64::MAX },
        clipboard: index & 1 != 0,
        buffer: match index % 4 {
            0 => None,
            1 => Some(PasteBufferAction::Append),
            2 => Some(PasteBufferAction::Create { prefix: None }),
            _ => Some(PasteBufferAction::Create {
                prefix: Some("search-🦓".to_owned()),
            }),
        },
        pipe: (index & 2 != 0).then(|| "cat > /tmp/selection".to_owned()),
        clear_selection: index & 4 != 0,
        cancel: index & 8 != 0,
    }
}

fn copy_actions() -> Vec<CopyModeAction> {
    use CopyModeAction as A;
    let mut actions = vec![
        A::Left,
        A::Right,
        A::Up,
        A::Down,
        A::PageUp,
        A::PageDown,
        A::HalfPageUp,
        A::HalfPageDown,
        A::Top,
        A::Bottom,
        A::TopLine,
        A::MiddleLine,
        A::BottomLine,
        A::StartOfLine,
        A::BackToIndentation,
        A::EndOfLine,
        A::NextWord,
        A::PreviousWord,
        A::NextWordEnd,
        A::NextParagraph,
        A::PreviousParagraph,
        A::StartSelection,
        A::SelectWord,
        A::SelectLine,
        A::ClearSelection,
        A::ClearSelectionOrCancel,
        A::ToggleRectangle,
        A::RectangleOn,
        A::RectangleOff,
        A::OtherEnd,
        A::SetMark,
        A::JumpToMark,
        A::Cancel,
        A::NextSpace,
        A::PreviousSpace,
        A::NextSpaceEnd,
        A::ScrollUp,
        A::ScrollDown,
        A::ScrollMiddle,
        A::NextMatchingBracket,
        A::PageDownScrollExit,
        A::StopSelection,
        A::HalfPageDownScrollExit,
        A::ScrollDownAndCancel,
        A::CursorDownAndCancel,
        A::ScrollExitOn,
        A::ScrollExitOff,
        A::ScrollExitToggle,
        A::CursorCentreVertical,
        A::CursorCentreHorizontal,
        A::ScrollTop,
        A::ScrollBottom,
        A::TogglePosition,
        A::RecentreTopBottom,
        A::PreviousMatchingBracket,
        A::RefreshOn,
        A::RefreshOff,
        A::RefreshToggle,
        A::RefreshNow,
        A::LineNumbersOff,
        A::RefreshRevision,
    ];
    for flag in [false, true] {
        actions.extend([
            A::NextPrompt { output: flag },
            A::PreviousPrompt { output: flag },
            A::SearchAgain { reverse: flag },
            A::RepeatJump { reverse: flag },
            A::LineNumbersOn { option_off: flag },
            A::LineNumbersToggle { option_off: flag },
        ]);
        for line in [None, Some(i32::MIN), Some(0), Some(i32::MAX)] {
            actions.push(A::GotoLine {
                line,
                option_absolute: flag,
            });
        }
    }
    for mode in [
        CopySelectionMode::Char,
        CopySelectionMode::Word,
        CopySelectionMode::Line,
    ] {
        actions.push(A::SelectionMode(mode));
    }
    for direction in [SearchDirection::Forward, SearchDirection::Backward] {
        actions.push(A::SearchCursorWord { direction });
        for regex in [false, true] {
            for incremental in [false, true] {
                actions.push(A::Search(Box::new(CopyModeSearch {
                    text: "needle.*é🦓".to_owned(),
                    direction,
                    regex,
                    incremental,
                })));
            }
        }
    }
    for direction in [CopyJumpDirection::Forward, CopyJumpDirection::Backward] {
        for to in [false, true] {
            actions.push(A::Jump(CopyJump {
                target: "界".to_owned(),
                direction,
                to,
            }));
        }
    }
    for index in 0..16 {
        actions.extend([
            A::CopySelection(Box::new(copy_request(index))),
            A::CopyEndOfLine(Box::new(copy_request(index))),
            A::CopyLine(Box::new(copy_request(index))),
        ]);
    }
    actions.push(A::MouseCursor(PointerCellEvent {
        column: u16::MAX,
        row: u16::MAX,
        click_count: 3,
        rectangle: true,
    }));
    actions
}

fn main() {
    let mut cases = Vec::new();
    let copies = copy_actions();
    for pane in [PaneId(0), PaneId(u64::MAX)] {
        for output in [false, true] {
            for action in &copies {
                cases.push(wire(
                    pane,
                    output,
                    TerminalViewAction::CopyMode(action.clone()),
                ));
                for count in [0, 1, u32::MAX] {
                    cases.push(wire(
                        pane,
                        output,
                        TerminalViewAction::CopyModeCounted {
                            action: action.clone(),
                            count,
                        },
                    ));
                }
            }
            for action in [
                TerminalViewAction::SearchNext,
                TerminalViewAction::SearchPrevious,
                TerminalViewAction::SearchClose,
                TerminalViewAction::EnterCopyModeScrollExit,
            ] {
                cases.push(wire(pane, output, action));
            }
            for scroll_exit in [false, true] {
                for hide_position in [false, true] {
                    cases.push(wire(
                        pane,
                        output,
                        TerminalViewAction::EnterCopyModeWith {
                            scroll_exit,
                            hide_position,
                        },
                    ));
                }
            }
            for mode in [SearchMode::Literal, SearchMode::Regex] {
                for case in [
                    SearchCase::Smart,
                    SearchCase::Sensitive,
                    SearchCase::Insensitive,
                ] {
                    for direction in [SearchDirection::Forward, SearchDirection::Backward] {
                        for text in ["", "needle", "a\0[é🦓]"] {
                            let query = SearchQuery {
                                text: text.to_owned(),
                                mode,
                                case,
                                direction,
                            };
                            cases.push(wire(
                                pane,
                                output,
                                TerminalViewAction::SearchBegin(query.clone()),
                            ));
                            cases.push(wire(pane, output, TerminalViewAction::SearchUpdate(query)));
                        }
                    }
                }
            }
        }
        for direction in [SearchDirection::Forward, SearchDirection::Backward] {
            let message = ProtocolMessage::Event(Event {
                sequence: u64::MAX,
                payload: EventPayload::TerminalUiCommand {
                    pane,
                    command: TerminalUiCommand::BeginSearch { direction },
                },
            });
            let frame = encode_protocol_message(&message).expect("search UI command encodes");
            cases.push(
                json!({"kind":"event", "request":frame, "pane":pane.0,"direction":direction}),
            );
        }
    }
    let desktop = ChromeKeymap::for_profile(ChromeProfile::Desktop);
    let apple = ChromeKeymap::for_profile(ChromeProfile::DesktopApple);
    for character in ['r', 'R', 'c', 'C', 'f', 'F', 'x', 'é'] {
        for modifiers in 0..16_u8 {
            for action in [KeyAction::Press, KeyAction::Repeat, KeyAction::Release] {
                let input = KeyInput {
                    action,
                    key: KeyCode::Character(character),
                    modifiers: Modifiers::new(
                        modifiers & 1 != 0,
                        modifiers & 2 != 0,
                        modifiers & 4 != 0,
                        modifiers & 8 != 0,
                    ),
                    text: None,
                    unshifted_codepoint: None,
                };
                for mode in [SearchMode::Literal, SearchMode::Regex] {
                    for case in [
                        SearchCase::Smart,
                        SearchCase::Sensitive,
                        SearchCase::Insensitive,
                    ] {
                        for direction in [SearchDirection::Forward, SearchDirection::Backward] {
                            let initial = SearchQuery {
                                text: "needle".to_owned(),
                                mode,
                                case,
                                direction,
                            };
                            let mut query = initial.clone();
                            let result = search_key_action(&mut query, &input);
                            cases.push(json!({"kind":"key", "query":initial,"character":character as u32,"modifiers":modifiers,"action":action,"expected_query":query,"changed":result.is_some()}));
                        }
                    }
                }
                for prefix in [false, true] {
                    for modal in [false, true] {
                        let result = terminal_shortcut(
                            [&desktop, &apple],
                            prefix,
                            modal.then_some(PaneId(0)),
                            &input,
                        );
                        cases.push(json!({"kind":"shortcut", "character":character as u32,"modifiers":modifiers,"action":action,"prefix":prefix,"modal":modal,"expected":matches!(result,Some(ChromeAction::TerminalSearch))}));
                    }
                }
            }
        }
    }
    for (key, key_tag, character) in [
        (KeyCode::Unidentified, 16, 0),
        (KeyCode::Escape, 4, 0),
        (KeyCode::Character('F'), 0, u32::from('F')),
    ] {
        for text in [None, Some(""), Some("f"), Some("F"), Some("ff")] {
            for modifiers in 0..16_u8 {
                let input = KeyInput {
                    action: KeyAction::Press,
                    key,
                    modifiers: Modifiers::new(
                        modifiers & 1 != 0,
                        modifiers & 2 != 0,
                        modifiers & 4 != 0,
                        modifiers & 8 != 0,
                    ),
                    text: text.map(Into::into),
                    unshifted_codepoint: None,
                };
                for prefix in [false, true] {
                    for modal in [false, true] {
                        let result = terminal_shortcut(
                            [&desktop, &apple],
                            prefix,
                            modal.then_some(PaneId(0)),
                            &input,
                        );
                        cases.push(json!({"kind":"shortcut","key":key_tag,"character":character,"text":text,"modifiers":modifiers,"action":"Press","prefix":prefix,"modal":modal,"expected":matches!(result,Some(ChromeAction::TerminalSearch))}));
                    }
                }
            }
        }
    }
    for current in [0, 1, u32::MAX] {
        for total in [0, 1, 24, u32::MAX] {
            for pending in [false, true] {
                for invalid in [false, true] {
                    let status = SearchStatus::new(current, total)
                        .with_pending(pending)
                        .with_invalid_pattern(invalid);
                    cases.push(json!({"kind":"status", "current":status.current(),"total":status.total,"pending":status.pending(),"invalid":status.invalid_pattern(),"expected":search_result_text(Some(status))}));
                }
            }
        }
    }
    cases.push(json!({"kind":"status","current":0,"total":0,"pending":false,"invalid":false,"expected":search_result_text(None)}));
    for case in [
        SearchCase::Smart,
        SearchCase::Sensitive,
        SearchCase::Insensitive,
    ] {
        cases.push(json!({"kind":"case","case":case,"expected":search_case_label(case)}));
    }
    for unseen in [0, 1, u32::MAX] {
        cases.push(json!({"kind":"mode","mode":0,"position":0,"total":0,"hide":false,"unseen":unseen,"expected":mode_text(TerminalMode::Live, unseen)}));
        for position in [0, 1, u32::MAX] {
            for total in [0, 1, u32::MAX] {
                for hide_position in [false, true] {
                    let mode = TerminalMode::Copy {
                        position,
                        total,
                        hide_position,
                    };
                    cases.push(json!({"kind":"mode","mode":1,"position":position,"total":total,"hide":hide_position,"unseen":unseen,"expected":mode_text(mode, unseen)}));
                }
                let mode = TerminalMode::View { position, total };
                cases.push(json!({"kind":"mode","mode":2,"position":position,"total":total,"hide":false,"unseen":unseen,"expected":mode_text(mode, unseen)}));
            }
        }
    }
    println!(
        "{}",
        json!({"source":"actual terminal action serializer and client search helpers","cases":cases})
    );
}
