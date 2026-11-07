use std::{error::Error, io};

use ala::{Keystroke, SharedString};
use penna::blocks::palette::PaletteShortcut;
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
    for case in cases {
        let value = tmux_keystroke(&case);
        uint(&mut bytes, u64::from(value.is_some()));
        if let Some(value) = value {
            key(&mut bytes, value);
        }
        match tmux_shortcut(&SharedString::from(case)) {
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
        serde_json::to_string(&json!({"bytes":bytes,"ends":ends}))?
    );
    Ok(())
}
