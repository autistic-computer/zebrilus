use super::test_support::{parse_tty, tty_sequence_samples, tty_text_samples};
use super::*;

#[test]
fn xterm_shift_sequences_reach_the_shared_key_contract() {
    for (bytes, expected) in [
        (b"\x1b[1;2A".as_slice(), "S-Up"),
        (b"\x1b[1;2B".as_slice(), "S-Down"),
        (b"\x1b[1;2C".as_slice(), "S-Right"),
        (b"\x1b[1;2D".as_slice(), "S-Left"),
        (b"\x1b[Z".as_slice(), "BTab"),
        (b"\x1b[1;2H".as_slice(), "S-Home"),
        (b"\x1b[1;2F".as_slice(), "S-End"),
        (b"\x1b[5;2~".as_slice(), "S-PPage"),
        (b"\x1b[6;2~".as_slice(), "S-NPage"),
        (b"\x1b[2;2~".as_slice(), "S-IC"),
        (b"\x1b[3;2~".as_slice(), "S-DC"),
        (b"\x1b[1;2P".as_slice(), "S-F1"),
        (b"\x1b[1;2Q".as_slice(), "S-F2"),
        (b"\x1b[1;2R".as_slice(), "S-F3"),
        (b"\x1b[1;2S".as_slice(), "S-F4"),
        (b"\x1b[24;2~".as_slice(), "S-F12"),
        (b"\x1bs".as_slice(), "M-s"),
        (b"\x1bS".as_slice(), "M-S"),
        (b"\x1c".as_slice(), "C-\\"),
    ] {
        let mut parser = crate::terminal_event::EventParser::default();
        let mut events = Vec::new();
        for byte in bytes {
            parser.push(&[*byte], &mut events);
        }
        let [TerminalEvent::Key(event)] = events.as_slice() else {
            panic!("{expected}: {events:?}");
        };
        assert_eq!(
            zz_protocol::input_key_name(&key_input(*event)).as_str(),
            expected
        );
    }
}

fn assert_tty_key_parses_here(bytes: &[u8]) -> Option<KeyInput> {
    let (daemon, width) = zz_protocol::tty_input_key(bytes)?;
    assert_eq!(width, bytes.len(), "{bytes:?}");
    let (events, idle) = parse_tty(bytes);
    let [TerminalEvent::Key(event)] = events.as_slice() else {
        panic!("{bytes:?}: {events:?}");
    };
    assert!(idle, "{bytes:?}");
    assert_eq!(key_input(*event), daemon, "{bytes:?}");
    let mut followed = bytes.to_vec();
    followed.push(b'x');
    assert_eq!(
        zz_protocol::tty_input_key(&followed),
        Some((daemon.clone(), width)),
        "{bytes:?}"
    );
    Some(daemon)
}

#[test]
fn every_key_the_daemon_decodes_parses_to_the_same_key_here() {
    let mut keyed = 0;
    for byte in 0..=u8::MAX {
        match assert_tty_key_parses_here(&[byte]) {
            Some(_) => keyed += 1,
            None => assert!(byte == 0x1b || byte >= 0x80, "{byte:#x}"),
        }
    }
    assert_eq!(keyed, 127);
    for character in tty_text_samples() {
        let encoded = character.to_string().into_bytes();
        assert_eq!(
            assert_tty_key_parses_here(&encoded).is_some(),
            !character.is_control(),
            "{character:?}"
        );
        for cut in 1..encoded.len() {
            assert_eq!(zz_protocol::tty_input_key(&encoded[..cut]), None);
            let (events, idle) = parse_tty(&encoded[..cut]);
            assert!(events.is_empty() && !idle, "{character:?}");
        }
    }
    let decoded = tty_sequence_samples()
        .iter()
        .filter_map(|sample| {
            assert_tty_key_parses_here(sample)
                .map(|key| (String::from_utf8_lossy(sample).into_owned(), key))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    for (sequence, name) in [
        ("\x1b[A", "Up"),
        ("\x1b[D", "Left"),
        ("\x1bOB", "Down"),
        ("\x1bOP", "F1"),
        ("\x1b[H", "Home"),
        ("\x1b[3~", "DC"),
        ("\x1b[5~", "PPage"),
        ("\x1b[24;8~", "C-M-S-F12"),
        ("\x1b[1;5C", "C-Right"),
        ("\x1b[1;3D", "M-Left"),
        ("\x1b[Z", "BTab"),
    ] {
        assert_eq!(
            decoded
                .get(sequence)
                .map(|key| zz_protocol::input_key_name(key).into_string()),
            Some(name.to_owned()),
            "{sequence:?}"
        );
    }
    for sequence in [
        "\x1b",
        "\x1b[",
        "\x1b[1;5",
        "\x1ba",
        "\x1b[I",
        "\x1b[O",
        "\x1b[200~",
        "\x1b[1;17A",
        "\x1b[1;A",
        "\x1b[1;2:3A",
        "\x1b[2A",
        "\x1b[<0;1;1M",
        "\x1b[M",
        "\x1b[97;5u",
        "\x1bOp",
    ] {
        assert_eq!(
            zz_protocol::tty_input_key(sequence.as_bytes()),
            None,
            "{sequence:?}"
        );
    }
}

#[test]
fn scalar_index_clamps_and_tracks_multibyte_text() {
    assert_eq!(scalar_byte_index("aéz", 0), 0);
    assert_eq!(scalar_byte_index("aéz", 1), 1);
    assert_eq!(scalar_byte_index("aéz", 2), 3);
    assert_eq!(scalar_byte_index("aéz", 99), 4);
}

#[test]
fn key_mapping_keeps_typed_text_and_normalizes_letter_identity() {
    let input = key_input(KeyEvent::new(
        TerminalKeyCode::Char('A'),
        KeyModifiers::SHIFT,
    ));
    assert_eq!(input.key, KeyCode::Character('a'));
    assert_eq!(input.text.as_deref(), Some("A"));
    assert!(input.modifiers.shift());
}
