use std::{error::Error, fs::File};

use serde_json::{Value, json};
use zz_client::chooser::ChooserInput;
use zz_terminal::Modifiers;

#[derive(Clone)]
enum Operation {
    Synchronize(u64, u32, bool),
    Mark(String),
    Replace(String),
    Unmark,
}

fn step(input: &mut ChooserInput, operation: &Operation) -> Value {
    let (request, effect) = match operation {
        Operation::Synchronize(revision, selected, search) => {
            let update = input.synchronize(*revision, *selected, *search);
            (
                json!([0, revision, selected, search]),
                json!([update.changed, update.selection_changed]),
            )
        }
        Operation::Mark(text) => {
            input.mark_text(text);
            (json!([1, text]), Value::Null)
        }
        Operation::Replace(text) => (json!([2, text]), json!(input.replace_text(text))),
        Operation::Unmark => (json!([3]), json!(input.unmark())),
    };
    let mut keys = 0_u32;
    for (character_index, character) in [false, true].into_iter().enumerate() {
        for bits in 0_u8..16 {
            let modifiers =
                Modifiers::new(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0);
            if input.key_is_text(character, modifiers) {
                keys |= 1 << (character_index * 16 + usize::from(bits));
            }
        }
    }
    json!({
        "request":request,
        "effect":effect,
        "revision":input.revision(),
        "selected":input.selected(),
        "search":input.search_active(),
        "marked":input.marked_text(),
        "utf16":input.marked_text().map(|text|text.encode_utf16().count()),
        "keys":keys,
    })
}

fn trace(operations: &[Operation]) -> Vec<Value> {
    let mut input = ChooserInput::default();
    operations
        .iter()
        .map(|operation| step(&mut input, operation))
        .collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let alphabet = [
        Operation::Synchronize(0, 0, false),
        Operation::Synchronize(1, 0, true),
        Operation::Synchronize(1, u32::MAX, false),
        Operation::Synchronize(2, u32::MAX, false),
        Operation::Synchronize(u64::MAX, 2, true),
        Operation::Synchronize(u64::MAX, 2, false),
        Operation::Mark(String::new()),
        Operation::Mark("a".into()),
        Operation::Mark("é\0😀".into()),
        Operation::Mark("e\u{301}🦀👩\u{200d}💻".into()),
        Operation::Replace(String::new()),
        Operation::Replace("λ\n世界".into()),
        Operation::Replace("\0committed😀".into()),
        Operation::Unmark,
    ];
    let mut traces = Vec::new();
    for first in &alphabet {
        for second in &alphabet {
            for third in &alphabet {
                traces.push(trace(&[first.clone(), second.clone(), third.clone()]));
            }
        }
    }
    for text in ["x".repeat(65_536), "😀".repeat(16_384)] {
        traces.push(trace(&[
            Operation::Synchronize(1, u32::MAX, true),
            Operation::Mark(text.clone()),
            Operation::Synchronize(1, 0, false),
            Operation::Replace(text),
            Operation::Unmark,
        ]));
    }
    let path = std::env::args()
        .nth(1)
        .ok_or("chooser runtime output path required")?;
    serde_json::to_writer(File::create(path)?, &traces)?;
    Ok(())
}
