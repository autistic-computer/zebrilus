use super::*;

#[test]
fn maps_ala_key_names() {
    assert_eq!(key_code("left"), KeyCode::ArrowLeft);
    assert_eq!(key_code("f12"), KeyCode::Function(12));
    assert_eq!(key_code("x"), KeyCode::Character('x'));
    assert_eq!(key_code("space"), KeyCode::Character(' '));
    assert_eq!(key_code("tab"), KeyCode::Tab);
}

#[test]
fn platform_shifted_special_keys_keep_their_daemon_names() {
    for (key, expected) in [
        ("left", "S-Left"),
        ("right", "S-Right"),
        ("up", "S-Up"),
        ("down", "S-Down"),
        ("tab", "BTab"),
        ("home", "S-Home"),
        ("end", "S-End"),
        ("pageup", "S-PPage"),
        ("pagedown", "S-NPage"),
        ("insert", "S-IC"),
        ("delete", "S-DC"),
        ("f1", "S-F1"),
        ("f12", "S-F12"),
    ] {
        let event = KeyDownEvent {
            keystroke: Keystroke {
                key: key.to_owned(),
                key_char: None,
                modifiers: ala::Modifiers {
                    shift: true,
                    ..Default::default()
                },
            },
            is_held: false,
            prefer_character_input: false,
        };
        let message = InputMessage::Key {
            pane: PaneId(7),
            input: key_input(&event.keystroke, key_code(key), KeyAction::Press),
            text_follows: false,
        };
        let InputMessage::Key { input, .. } = message else {
            unreachable!()
        };
        assert_eq!(zz_protocol::input_key_name(&input).as_str(), expected);
        let prefix_input =
            crate::mux::prefix::terminal_key_input(&event.keystroke, KeyAction::Press);
        assert_eq!(
            zz_protocol::input_key_name(&prefix_input).as_str(),
            expected
        );
    }
}

#[test]
fn ala_terminal_commits_text_as_one_standalone_message() {
    assert_eq!(
        terminal_text_input(PaneId(7), None, "typed"),
        InputMessage::Text {
            pane: PaneId(7),
            text: "typed".to_owned(),
        }
    );
}

#[test]
fn kitty_text_key_carries_the_platform_committed_text() {
    let event = KeyDownEvent {
        keystroke: Keystroke {
            key: "e".to_owned(),
            key_char: Some("e".to_owned()),
            modifiers: ala::Modifiers::default(),
        },
        is_held: false,
        prefer_character_input: false,
    };

    assert_eq!(
        terminal_text_input(PaneId(7), Some(&event), "é"),
        InputMessage::Key {
            pane: PaneId(7),
            input: KeyInput {
                action: KeyAction::Press,
                key: KeyCode::Character('e'),
                modifiers: Modifiers::default(),
                text: Some(Box::from("é")),
                unshifted_codepoint: Some('e'),
            },
            text_follows: false,
        }
    );
}

#[test]
fn space_release_preserves_its_unshifted_codepoint_for_kitty_encoding() {
    let keystroke = Keystroke {
        key: "space".to_owned(),
        key_char: Some(" ".to_owned()),
        modifiers: ala::Modifiers::default(),
    };

    assert_eq!(
        key_input(&keystroke, KeyCode::Character(' '), KeyAction::Release),
        KeyInput {
            action: KeyAction::Release,
            key: KeyCode::Character(' '),
            modifiers: Modifiers::default(),
            text: Some(Box::from(" ")),
            unshifted_codepoint: Some(' '),
        }
    );
}

fn chrome_action(source: &str) -> Option<ChromeAction> {
    let keystroke = Keystroke::parse(source).expect("valid test keystroke");
    crate::keymap::test_resolve(TERMINAL_TABLE, &keystroke)
}

#[test]
fn terminal_font_shortcuts_accept_minus_equal_and_shifted_plus() {
    for (source, action) in [
        ("ctrl--", ChromeAction::TerminalFontDecrease),
        ("ctrl-=", ChromeAction::TerminalFontIncrease),
        ("ctrl-+", ChromeAction::TerminalFontIncrease),
        ("ctrl-shift-=", ChromeAction::TerminalFontIncrease),
    ] {
        assert_eq!(chrome_action(source), Some(action), "{source}");
    }
}

#[test]
fn terminal_chrome_leaves_every_other_chord_to_the_pane() {
    for source in ["-", "ctrl-alt-=", "cmd-=", "ctrl-c", "ctrl-f", "ctrl-v"] {
        assert_eq!(chrome_action(source), None, "{source}");
    }
}

#[test]
fn terminal_chrome_claims_the_shifted_control_chords() {
    for (source, action) in [
        ("ctrl-shift-f", ChromeAction::TerminalSearch),
        ("ctrl-shift-c", ChromeAction::TerminalCopy),
        ("ctrl-shift-a", ChromeAction::TerminalSelectAll),
        ("ctrl-shift-v", ChromeAction::TerminalPaste),
        ("cmd-f", ChromeAction::TerminalSearch),
        ("cmd-c", ChromeAction::TerminalCopy),
        ("cmd-a", ChromeAction::TerminalSelectAll),
        ("cmd-v", ChromeAction::TerminalPaste),
    ] {
        assert_eq!(chrome_action(source), Some(action), "{source}");
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn terminal_font_zoom_suppresses_application_ui_scaling() {
    let mut bindings =
        crate::ui_scale::key_bindings(&crate::keymap::test_chords(zz_client::UI_TABLE));
    bindings.extend(terminal_key_bindings(&crate::keymap::test_chords(
        TERMINAL_TABLE,
    )));
    let keymap = ala::Keymap::new(bindings);
    let contexts = [
        ala::KeyContext::parse(penna::ROOT_KEY_CONTEXT).expect("valid zz root context"),
        ala::KeyContext::parse(TERMINAL_KEY_CONTEXT).expect("valid terminal context"),
    ];

    for source in ["ctrl-=", "ctrl-+", "ctrl--"] {
        let keystroke = Keystroke::parse(source).expect("valid terminal zoom shortcut");
        let (bindings, pending) = keymap.bindings_for_input(&[keystroke], &contexts);
        assert!(!pending);
        assert!(
            bindings.is_empty(),
            "{source} must stay on the terminal's raw key path"
        );
    }
}
