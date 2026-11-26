use super::*;

pub(super) fn write_focus_event(
    terminal: &Terminal<'_, '_>,
    focused: bool,
    writer: &mut dyn Write,
) -> Result<(), WorkerError> {
    if !terminal.mode(Mode::FOCUS_EVENT)? {
        return Ok(());
    }
    let event = if focused {
        focus::Event::Gained
    } else {
        focus::Event::Lost
    };
    let mut encoded = [0_u8; 8];
    let written = event.encode(&mut encoded)?;
    writer.write_all(&encoded[..written])?;
    writer.flush()?;
    Ok(())
}

#[cfg(unix)]
pub(super) fn direct_key_bytes(input: &KeyInput) -> Option<&[u8]> {
    if !matches!(input.action, KeyAction::Press | KeyAction::Repeat)
        || input.modifiers != Modifiers::default()
    {
        return None;
    }
    match input.key {
        KeyCode::Character(_) => input
            .text
            .as_deref()
            .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
            .map(str::as_bytes),
        KeyCode::Enter if input.text.is_none() => Some(b"\r"),
        _ => None,
    }
}

pub(super) fn encode_key(
    terminal: &Terminal<'_, '_>,
    encoder: &mut key::Encoder<'_>,
    event: &mut key::Event<'_>,
    input: KeyInput,
    erase_byte: Option<u8>,
    writer: &mut dyn Write,
    input_bytes: &mut Vec<u8>,
) -> Result<(), WorkerError> {
    if matches!(input.key, KeyCode::User(_)) {
        return Ok(());
    }
    let mut modifiers = key::Mods::empty();
    modifiers.set(key::Mods::SHIFT, input.modifiers.shift());
    modifiers.set(key::Mods::CTRL, input.modifiers.control());
    modifiers.set(key::Mods::ALT, input.modifiers.alt());
    modifiers.set(key::Mods::SUPER, input.modifiers.platform());

    event
        .set_action(match input.action {
            KeyAction::Press => key::Action::Press,
            KeyAction::Repeat => key::Action::Repeat,
            KeyAction::Release => key::Action::Release,
        })
        .set_key(ghostty_key(input.key))
        .set_mods(modifiers)
        .set_composing(false)
        .set_utf8(input.text)
        .set_unshifted_codepoint(input.unshifted_codepoint.unwrap_or('\0'));

    encoder.set_options_from_terminal(terminal);
    #[cfg(target_os = "macos")]
    encoder.set_macos_option_as_alt(key::OptionAsAlt::True);

    input_bytes.clear();
    encoder.encode_to_vec(event, input_bytes)?;
    if input.key == KeyCode::Backspace
        && input.modifiers == Modifiers::default()
        && input_bytes.as_slice() == [0x7f]
    {
        input_bytes.clear();
        input_bytes.extend(erase_byte);
    }
    if !input_bytes.is_empty() {
        writer.write_all(input_bytes)?;
        writer.flush()?;
    }
    Ok(())
}

pub(super) fn ghostty_key(key: KeyCode) -> key::Key {
    match key {
        KeyCode::Character(character) => match character.to_ascii_lowercase() {
            '`' => key::Key::Backquote,
            '\\' => key::Key::Backslash,
            '[' => key::Key::BracketLeft,
            ']' => key::Key::BracketRight,
            ',' => key::Key::Comma,
            '0' => key::Key::Digit0,
            '1' => key::Key::Digit1,
            '2' => key::Key::Digit2,
            '3' => key::Key::Digit3,
            '4' => key::Key::Digit4,
            '5' => key::Key::Digit5,
            '6' => key::Key::Digit6,
            '7' => key::Key::Digit7,
            '8' => key::Key::Digit8,
            '9' => key::Key::Digit9,
            '=' => key::Key::Equal,
            'a' => key::Key::A,
            'b' => key::Key::B,
            'c' => key::Key::C,
            'd' => key::Key::D,
            'e' => key::Key::E,
            'f' => key::Key::F,
            'g' => key::Key::G,
            'h' => key::Key::H,
            'i' => key::Key::I,
            'j' => key::Key::J,
            'k' => key::Key::K,
            'l' => key::Key::L,
            'm' => key::Key::M,
            'n' => key::Key::N,
            'o' => key::Key::O,
            'p' => key::Key::P,
            'q' => key::Key::Q,
            'r' => key::Key::R,
            's' => key::Key::S,
            't' => key::Key::T,
            'u' => key::Key::U,
            'v' => key::Key::V,
            'w' => key::Key::W,
            'x' => key::Key::X,
            'y' => key::Key::Y,
            'z' => key::Key::Z,
            '-' => key::Key::Minus,
            '.' => key::Key::Period,
            '\'' => key::Key::Quote,
            ';' => key::Key::Semicolon,
            '/' => key::Key::Slash,
            ' ' => key::Key::Space,
            _ => key::Key::Unidentified,
        },
        KeyCode::Backspace => key::Key::Backspace,
        KeyCode::Enter => key::Key::Enter,
        KeyCode::Tab => key::Key::Tab,
        KeyCode::Escape => key::Key::Escape,
        KeyCode::Delete => key::Key::Delete,
        KeyCode::Insert => key::Key::Insert,
        KeyCode::Home => key::Key::Home,
        KeyCode::End => key::Key::End,
        KeyCode::PageUp => key::Key::PageUp,
        KeyCode::PageDown => key::Key::PageDown,
        KeyCode::ArrowUp => key::Key::ArrowUp,
        KeyCode::ArrowDown => key::Key::ArrowDown,
        KeyCode::ArrowLeft => key::Key::ArrowLeft,
        KeyCode::ArrowRight => key::Key::ArrowRight,
        KeyCode::Function(number) => match number {
            1 => key::Key::F1,
            2 => key::Key::F2,
            3 => key::Key::F3,
            4 => key::Key::F4,
            5 => key::Key::F5,
            6 => key::Key::F6,
            7 => key::Key::F7,
            8 => key::Key::F8,
            9 => key::Key::F9,
            10 => key::Key::F10,
            11 => key::Key::F11,
            12 => key::Key::F12,
            13 => key::Key::F13,
            14 => key::Key::F14,
            15 => key::Key::F15,
            16 => key::Key::F16,
            17 => key::Key::F17,
            18 => key::Key::F18,
            19 => key::Key::F19,
            20 => key::Key::F20,
            21 => key::Key::F21,
            22 => key::Key::F22,
            23 => key::Key::F23,
            24 => key::Key::F24,
            _ => key::Key::Unidentified,
        },
        KeyCode::Unidentified | KeyCode::User(_) => key::Key::Unidentified,
    }
}
