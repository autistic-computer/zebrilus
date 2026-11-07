use std::{error::Error, io};

use ala::{Keystroke, SharedString};
use penna::{blocks::palette::PaletteShortcut, components::kbd::Kbd};
use serde_json::json;
use zz_ui::command::{tmux_keystroke, tmux_shortcut};

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

fn key(bytes: &mut Vec<u8>, value: Keystroke) {
    let modifiers = u64::from(value.modifiers.shift)
        | u64::from(value.modifiers.control) << 1
        | u64::from(value.modifiers.alt) << 2
        | u64::from(value.modifiers.platform) << 3
        | u64::from(value.modifiers.function) << 4;
    uint(bytes, modifiers);
    text(bytes, &value.key);
    uint(bytes, u64::from(value.key_char.is_some()));
    if let Some(value) = value.key_char {
        text(bytes, &value);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let cases: Vec<String> = serde_json::from_reader(io::stdin().lock())?;
    let mut bytes = Vec::new();
    let mut ends = Vec::new();
    let mut labels = Vec::new();
    for case in cases {
        let value = tmux_keystroke(&case);
        let label = value.as_ref().map(|value| Kbd::format_key(value, true));
        uint(&mut bytes, u64::from(value.is_some()));
        if let Some(value) = value {
            key(&mut bytes, value);
        }
        let shortcut = tmux_shortcut(&SharedString::from(case));
        let sequence = match &shortcut {
            PaletteShortcut::Keys(keys) => Some(
                keys.iter()
                    .map(|key| Kbd::format_key(key, true))
                    .collect::<Vec<_>>(),
            ),
            PaletteShortcut::Text(_) => None,
        };
        labels.push(json!({"key":label,"keys":sequence}));
        match shortcut {
            PaletteShortcut::Keys(keys) => {
                uint(&mut bytes, 0);
                text(&mut bytes, "");
                uint(&mut bytes, keys.len() as u64);
                for value in keys {
                    key(&mut bytes, value);
                }
            }
            PaletteShortcut::Text(value) => {
                uint(&mut bytes, 1);
                text(&mut bytes, &value);
                uint(&mut bytes, 0);
            }
        }
        ends.push(bytes.len());
    }
    println!(
        "{}",
        serde_json::to_string(
            &json!({"bytes":bytes,"ends":ends,"format":labels,"apple":cfg!(any(target_os="macos",target_os="ios"))})
        )?
    );
    Ok(())
}
