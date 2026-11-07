use serde_json::json;
use zz_client::terminal_view::terminal_shortcut;
use zz_client::{ChromeAction, ChromeKeymap, ChromeProfile};
use zz_protocol::PaneId;
use zz_terminal::{KeyAction, KeyCode, KeyInput, Modifiers};

fn main() {
    let desktop = ChromeKeymap::for_profile(ChromeProfile::Desktop);
    let apple = ChromeKeymap::for_profile(ChromeProfile::DesktopApple);
    let mut cases = Vec::new();
    for character in ['a', 'A', 'c', 'C', 'v', 'V', 'x', 'é'] {
        for modifiers in 0..16_u8 {
            for action in [KeyAction::Press, KeyAction::Repeat, KeyAction::Release] {
                for unidentified in [false, true] {
                    for prefix in [false, true] {
                        for modal in [false, true] {
                            let input = KeyInput {
                                action,
                                key: if unidentified {
                                    KeyCode::Unidentified
                                } else {
                                    KeyCode::Character(character)
                                },
                                modifiers: Modifiers::new(
                                    modifiers & 1 != 0,
                                    modifiers & 2 != 0,
                                    modifiers & 4 != 0,
                                    modifiers & 8 != 0,
                                ),
                                text: unidentified.then(|| character.to_string().into_boxed_str()),
                                unshifted_codepoint: None,
                            };
                            let action = terminal_shortcut(
                                [&desktop, &apple],
                                prefix,
                                modal.then_some(PaneId(0)),
                                &input,
                            );
                            let expected = match action {
                                Some(ChromeAction::TerminalCopy) => 1,
                                Some(ChromeAction::TerminalPaste) => 2,
                                Some(ChromeAction::TerminalSelectAll) => 3,
                                _ => 0,
                            };
                            cases.push(json!({"args":[u8::from(unidentified) * 16, character as u32, modifiers, input.action as u8, prefix, modal],"expected":expected}));
                        }
                    }
                }
            }
        }
    }
    println!(
        "{}",
        json!({"source":"actual zz-client terminal_shortcut and both desktop chrome profiles","cases":cases})
    );
}
