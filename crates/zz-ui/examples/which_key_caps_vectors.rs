use std::{collections::BTreeSet, error::Error, fmt::Write as _, fs, path::PathBuf};

use ala::Keystroke;
use penna::components::kbd::Kbd;
use serde_json::json;
use zz_client::ChromeKey;
use zz_protocol::KeyTables;

fn stroke(key: &str) -> Option<Keystroke> {
    Keystroke::parse(&zz_config::keymap::ala_source(&ChromeKey::parse(key)?)?).ok()
}

fn main() -> Result<(), Box<dyn Error>> {
    let directory = PathBuf::from(std::env::args().nth(1).ok_or("expected output directory")?);
    fs::create_dir_all(&directory)?;
    let mut bases: BTreeSet<String> = KeyTables::default()
        .snapshot()
        .into_iter()
        .flat_map(|table| table.bindings.into_iter().map(|binding| binding.key))
        .collect();
    for base in [
        "",
        " ",
        "Space",
        "None",
        "control",
        "ctrl",
        "shift",
        "alt",
        "cmd",
        "super",
        "win",
        "secondary",
        "fn",
        "fn-ctrl-a",
        "shift-X",
        "a->B",
        "a->B-z",
        "a--z-",
        "foo-bar",
        "C-",
        "A",
        "Z",
        "Return",
        "PageUp",
        "PPage",
        "F1",
        "F12",
        "F256",
        "🙂",
        "e\u{301}",
        "bad-name",
        "µfoo",
        "ßfoo",
        "ΣFOO",
        "AΣ",
        "ΑΣ",
        "AΣB",
        "AΣ́",
        "Μάϊος",
        "ΟΣ",
        "OΣ'",
        "Iİ",
        "İΣ",
        "AΣ'",
        "AΣ:B",
        "AΣ́B",
        "AΣ\u{345}",
        "AΣ\u{345}B",
        "A\u{345}Σ",
        "AΣ1",
        "A1Σ",
        "ßΣ",
        "µΣ",
        "\u{85}a\u{2029}",
    ] {
        bases.insert(base.into());
    }
    let mut ranges: [String; 3] = std::array::from_fn(|_| String::new());
    let mut starts = [None; 3];
    let mut lasts = [0; 3];
    for point in 0..=0x0010_ffff {
        let Some(character) = char::from_u32(point) else {
            continue;
        };
        let value = character.to_string();
        if character.to_lowercase().to_string() != value
            || character.to_uppercase().to_string() != value
        {
            bases.insert(value.clone());
            bases.insert(format!("{value}Σ"));
        }
        let cased = format!("AΣ{character}").to_lowercase().chars().nth(1) == Some('σ');
        let through = format!("AΣ{character}A").to_lowercase().chars().nth(1) == Some('σ');
        for (index, present) in [character.is_alphanumeric(), cased, through && !cased]
            .into_iter()
            .enumerate()
        {
            if present {
                starts[index].get_or_insert(point);
                lasts[index] = point;
            } else if let Some(first) = starts[index].take() {
                write!(ranges[index], "{first:08x}{:08x}", lasts[index])?;
            }
        }
    }
    for (index, name) in ["alphanumeric", "cased", "ignorable"]
        .into_iter()
        .enumerate()
    {
        if let Some(first) = starts[index] {
            write!(ranges[index], "{first:08x}{:08x}", lasts[index])?;
        }
        fs::write(directory.join(format!("{name}.txt")), &ranges[index])?;
    }
    let mut cases = Vec::new();
    for prefix in ["", "C-", "M-", "S-", "D-", "C-M-S-", "Ctrl-Alt-", "M-C-C-"] {
        for base in &bases {
            let input = format!("{prefix}{base}");
            let parsed = stroke(&input);
            cases.push(json!({"input":input,"value":parsed.as_ref().map(|key| json!({"key":key.key,"key_char":key.key_char,"modifiers":u8::from(key.modifiers.shift) | u8::from(key.modifiers.control)<<1 | u8::from(key.modifiers.alt)<<2 | u8::from(key.modifiers.platform)<<3 | u8::from(key.modifiers.function)<<4,"text":Kbd::format_key(key,true)}))}));
        }
    }
    fs::write(directory.join("caps.json"), serde_json::to_vec(&cases)?)?;
    println!("{}", cases.len());
    Ok(())
}
