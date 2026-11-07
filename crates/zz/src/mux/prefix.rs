use ala::Keystroke;
use zz_client::ChromeKey;
pub(crate) use zz_client::prefix_binding::{
    binds_input, is_all_keys_input, is_sidebar_picker_input,
};
use zz_terminal::{KeyAction, KeyCode, KeyInput, Modifiers as TerminalModifiers};

pub(crate) fn display_keystroke(canonical: &str) -> Option<Keystroke> {
    let key = ChromeKey::parse(canonical)?;
    Keystroke::parse(&crate::keymap::ala_source(&key)?).ok()
}

pub(crate) fn terminal_key_input(keystroke: &Keystroke, action: KeyAction) -> KeyInput {
    let key = terminal_key(&keystroke.key);
    let character = match key {
        KeyCode::Character(character) => Some(character),
        _ => None,
    };
    KeyInput {
        action,
        key,
        modifiers: TerminalModifiers::new(
            keystroke.modifiers.shift,
            keystroke.modifiers.control,
            keystroke.modifiers.alt,
            keystroke.modifiers.platform,
        ),
        text: keystroke
            .key_char
            .clone()
            .or_else(|| character.map(|value| value.to_string()))
            .map(String::into_boxed_str),
        unshifted_codepoint: character,
    }
}

fn terminal_key(key: &str) -> KeyCode {
    match key {
        "space" => KeyCode::Character(' '),
        "backspace" => KeyCode::Backspace,
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "escape" => KeyCode::Escape,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "up" => KeyCode::ArrowUp,
        "down" => KeyCode::ArrowDown,
        "left" => KeyCode::ArrowLeft,
        "right" => KeyCode::ArrowRight,
        value => {
            let mut characters = value.chars();
            if let (Some(character), None) = (characters.next(), characters.next()) {
                KeyCode::Character(character)
            } else {
                value
                    .strip_prefix('f')
                    .and_then(|number| number.parse::<u8>().ok())
                    .map_or(KeyCode::Unidentified, KeyCode::Function)
            }
        }
    }
}
