use std::error::Error;

use serde_json::json;
use zz_client::{
    ChromeAction, ChromeKeymap, ChromeProfile, Effect, InputEvent, InputOwner, InputRouter,
    PrefixView, SurfaceKind,
};
use zz_protocol::{InputMessage, PaneId, ProtocolMessage, encode_protocol_message};
use zz_terminal::{KeyAction, KeyCode, KeyInput, Modifiers};

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    let keys = [
        (KeyCode::Character('k'), None),
        (KeyCode::Character('p'), None),
        (KeyCode::Character('K'), None),
        (KeyCode::Character('P'), None),
        (KeyCode::Character('q'), Some("p")),
        (KeyCode::Character('\u{212a}'), Some("k")),
        (KeyCode::Character('\u{b}'), Some("k")),
        (KeyCode::Unidentified, Some("k")),
        (KeyCode::Unidentified, Some("p")),
        (KeyCode::Unidentified, Some("K")),
        (KeyCode::Unidentified, Some("pq")),
        (KeyCode::Enter, Some("p")),
    ];
    for apple in [false, true] {
        for overlay in [false, true] {
            for capture in [false, true] {
                for prefix in [false, true] {
                    for action in [KeyAction::Press, KeyAction::Repeat, KeyAction::Release] {
                        for bits in 0..16 {
                            for (key, text) in keys {
                                let mut router =
                                    InputRouter::new(ChromeKeymap::for_profile(if apple {
                                        ChromeProfile::DesktopApple
                                    } else {
                                        ChromeProfile::Desktop
                                    }));
                                router.event(InputEvent::SetActivePane(
                                    PaneId(0),
                                    SurfaceKind::Terminal,
                                ));
                                router.event(InputEvent::NativeFocusObserved(if overlay {
                                    InputOwner::Overlay
                                } else {
                                    InputOwner::Pane(PaneId(0), SurfaceKind::Terminal)
                                }));
                                router.set_capture(capture.then_some(PaneId(0)));
                                let input = KeyInput {
                                    action,
                                    key,
                                    modifiers: Modifiers::from_bits(bits)
                                        .ok_or("modifier fixture")?,
                                    text: text.map(Into::into),
                                    unshifted_codepoint: Some('p'),
                                };
                                router.key(
                                    &input,
                                    PrefixView {
                                        armed: prefix,
                                        claimed: prefix,
                                    },
                                );
                                let opened = router.drain_effects().iter().any(|effect| {
                                    matches!(
                                        effect,
                                        Effect::Chrome(ChromeAction::OpenCommandPalette)
                                    )
                                });
                                let frame = encode_protocol_message(&ProtocolMessage::Input(
                                    InputMessage::Key {
                                        pane: PaneId(0),
                                        input,
                                        text_follows: false,
                                    },
                                ))?;
                                cases.push(json!({"apple":apple,"overlay":overlay,"capture":capture,"prefix":prefix,"opened":opened,"frame":frame}));
                            }
                        }
                    }
                }
            }
        }
    }
    println!("{}", serde_json::to_string(&cases)?);
    Ok(())
}
