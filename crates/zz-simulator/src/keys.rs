pub const SHIFT: u32 = 0xE1;

pub fn key_usage(key: &str) -> Option<u32> {
    let usage = match key {
        "enter" => 0x28,
        "escape" => 0x29,
        "backspace" => 0x2A,
        "tab" => 0x2B,
        "space" | " " => 0x2C,
        "-" => 0x2D,
        "=" => 0x2E,
        "[" => 0x2F,
        "]" => 0x30,
        "\\" => 0x31,
        ";" => 0x33,
        "'" => 0x34,
        "`" => 0x35,
        "," => 0x36,
        "." => 0x37,
        "/" => 0x38,
        "home" => 0x4A,
        "pageup" => 0x4B,
        "delete" => 0x4C,
        "end" => 0x4D,
        "pagedown" => 0x4E,
        "right" => 0x4F,
        "left" => 0x50,
        "down" => 0x51,
        "up" => 0x52,
        _ => {
            let mut characters = key.chars();
            let (Some(character), None) = (characters.next(), characters.next()) else {
                return None;
            };
            match character.to_ascii_lowercase() {
                letter @ 'a'..='z' => 0x04 + (u32::from(letter) - u32::from('a')),
                digit @ '1'..='9' => 0x1E + (u32::from(digit) - u32::from('1')),
                '0' => 0x27,
                _ => return None,
            }
        }
    };
    Some(usage)
}

pub fn char_usage(character: char) -> Option<(u32, bool)> {
    let unshifted = match character {
        '!' => '1',
        '@' => '2',
        '#' => '3',
        '$' => '4',
        '%' => '5',
        '^' => '6',
        '&' => '7',
        '*' => '8',
        '(' => '9',
        ')' => '0',
        '_' => '-',
        '+' => '=',
        '{' => '[',
        '}' => ']',
        '|' => '\\',
        ':' => ';',
        '"' => '\'',
        '~' => '`',
        '<' => ',',
        '>' => '.',
        '?' => '/',
        'A'..='Z' => character.to_ascii_lowercase(),
        '\n' => return Some((0x28, false)),
        '\t' => return Some((0x2B, false)),
        _ => {
            return key_usage(character.encode_utf8(&mut [0; 4]))
                .filter(|_| character.is_ascii_graphic() || character == ' ')
                .map(|usage| (usage, false));
        }
    };
    key_usage(unshifted.encode_utf8(&mut [0; 4])).map(|usage| (usage, true))
}

#[cfg(test)]
mod tests {
    use super::{char_usage, key_usage};

    #[test]
    fn text_maps_to_usages_with_shift_where_the_key_needs_it() {
        assert_eq!(char_usage('a'), Some((0x04, false)));
        assert_eq!(char_usage('A'), Some((0x04, true)));
        assert_eq!(char_usage('@'), Some((0x1F, true)));
        assert_eq!(char_usage('?'), Some((0x38, true)));
        assert_eq!(char_usage(' '), Some((0x2C, false)));
        assert_eq!(char_usage('\n'), Some((0x28, false)));
        assert_eq!(char_usage('é'), None);
    }

    #[test]
    fn keys_map_to_their_keyboard_usage_codes() {
        assert_eq!(key_usage("a"), Some(0x04));
        assert_eq!(key_usage("Z"), Some(0x1D));
        assert_eq!(key_usage("1"), Some(0x1E));
        assert_eq!(key_usage("9"), Some(0x26));
        assert_eq!(key_usage("0"), Some(0x27));
        assert_eq!(key_usage("enter"), Some(0x28));
        assert_eq!(key_usage("left"), Some(0x50));
        assert_eq!(key_usage("f5"), None);
    }
}
