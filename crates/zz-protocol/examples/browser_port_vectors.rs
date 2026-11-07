use std::{error::Error, fs};
use zz_protocol::{
    BrowserCommand, Event, EventPayload, KeyToken, PaneId, ProtocolMessage, encode_protocol_message,
};
use zz_terminal::{KeyAction, KeyCode, KeyInput, Modifiers};

fn main() -> Result<(), Box<dyn Error>> {
    let mut commands = vec![
        BrowserCommand::Reload,
        BrowserCommand::Back,
        BrowserCommand::Forward,
    ];
    for value in ["", "https://x/a?b#c", "東京\0URL", "about:blank"] {
        commands.push(BrowserCommand::Navigate(value.to_owned()));
        commands.push(BrowserCommand::Screenshot {
            request_id: u64::MAX,
            path: value.to_owned(),
        });
    }
    let tokens = vec![
        KeyToken::Literal("é\0漢字".to_owned()),
        KeyToken::Named("C-Enter".to_owned()),
        KeyToken::Raw(255),
        KeyToken::Literal(String::new()),
    ];
    commands.push(BrowserCommand::SendKeys(tokens.clone()));
    for count in [0, 1, 9_999, 10_000, u32::MAX] {
        commands.push(BrowserCommand::SendKeysRepeated {
            keys: tokens.clone(),
            count,
        });
    }
    for action in [KeyAction::Press, KeyAction::Repeat, KeyAction::Release] {
        for code in [
            KeyCode::Character('λ'),
            KeyCode::Enter,
            KeyCode::Function(255),
            KeyCode::User(65535),
        ] {
            commands.push(BrowserCommand::Key(KeyInput {
                action,
                key: code,
                modifiers: Modifiers::new(true, true, false, false),
                text: Some("λ\0".into()),
                unshifted_codepoint: Some('λ'),
            }));
        }
    }
    let mut out = Vec::new();
    out.extend_from_slice(&u32::try_from(commands.len())?.to_le_bytes());
    for (index, command) in commands.into_iter().enumerate() {
        let bytes = encode_protocol_message(&ProtocolMessage::Event(Event {
            sequence: u64::MAX - u64::try_from(index)?,
            payload: EventPayload::BrowserCommand {
                pane: PaneId(0xf123_4567_89ab_cdef),
                command,
            },
        }))?;
        out.extend_from_slice(&u32::try_from(bytes.len())?.to_le_bytes());
        out.extend(bytes);
    }
    fs::write(std::env::args().nth(1).ok_or("missing output path")?, out)?;
    Ok(())
}
