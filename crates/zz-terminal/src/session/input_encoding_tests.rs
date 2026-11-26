#[cfg(unix)]
use super::test_support::*;
use super::*;
#[cfg(unix)]
use crate::DEFAULT_HISTORY_LIMIT;

fn test_pointer_input(phase: TerminalMousePhase, column: u16) -> TerminalMouseInput {
    TerminalMouseInput::new(
        phase,
        (phase == TerminalMousePhase::Press).then_some(TerminalMouseButton::Left),
        PointerCellEvent {
            column: column % 80,
            row: column % 24,
            click_count: 1,
            rectangle: false,
        },
        u32::from(column % 80) * 8 + 1,
        u32::from(column % 24) * 18 + 1,
        640,
        432,
        8,
        18,
        crate::Modifiers::default(),
        false,
    )
}

#[test]
fn byte_paste_uses_terminal_mode_without_forcing_utf8() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    let mut output = Vec::new();

    write_paste_bytes(&terminal, b"a\nb\xff".to_vec(), false, &mut output).expect("literal paste");
    assert_eq!(output, b"a\rb\xff");

    output.clear();
    write_paste_bytes(&terminal, b"a\nb\xff".to_vec(), true, &mut output)
        .expect("requested paste without terminal mode");
    assert_eq!(output, b"a\rb\xff");

    terminal.vt_write(b"\x1b[?2004h");
    output.clear();
    write_paste_bytes(&terminal, b"a\nb\xff".to_vec(), true, &mut output).expect("bracketed paste");
    assert_eq!(output, b"\x1b[200~a\nb\xff\x1b[201~");
}

#[test]
fn prepared_buffer_paste_preserves_exact_bytes_and_only_adds_requested_brackets() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    let mut output = Vec::new();

    write_prepared_paste_bytes(&terminal, b"a\nb\0\xff", true, &mut output)
        .expect("unbracketed prepared paste");
    assert_eq!(output, b"a\nb\0\xff");

    terminal.vt_write(b"\x1b[?2004h");
    output.clear();
    write_prepared_paste_bytes(&terminal, b"a\rb\0\xff", true, &mut output)
        .expect("bracketed prepared paste");
    assert_eq!(output, b"\x1b[200~a\rb\0\xff\x1b[201~");

    output.clear();
    write_prepared_paste_bytes(&terminal, b"", true, &mut output).expect("empty bracketed paste");
    assert_eq!(output, b"\x1b[200~\x1b[201~");
}

#[test]
fn key_mapping_covers_terminal_navigation() {
    assert_eq!(ghostty_key(KeyCode::Character('c')), key::Key::C);
    assert_eq!(ghostty_key(KeyCode::ArrowLeft), key::Key::ArrowLeft);
    assert_eq!(ghostty_key(KeyCode::Function(12)), key::Key::F12);
}

#[test]
fn a_user_key_reaches_the_pane_as_nothing() {
    let terminal = new_terminal(80, 24, 16).expect("terminal");
    let mut key_encoder = key::Encoder::new().expect("key encoder");
    let mut key_event = key::Event::new().expect("key event");
    let mut written = Vec::new();
    let mut input_bytes = Vec::new();
    encode_key(
        &terminal,
        &mut key_encoder,
        &mut key_event,
        KeyInput {
            action: KeyAction::Press,
            key: KeyCode::User(3),
            modifiers: crate::Modifiers::default(),
            text: None,
            unshifted_codepoint: None,
        },
        Some(0x7f),
        &mut written,
        &mut input_bytes,
    )
    .expect("encode user key");
    assert!(written.is_empty() && input_bytes.is_empty());
}

#[test]
fn backspace_encodes_the_pinned_default_erase_byte() {
    let terminal = new_terminal(80, 24, 16).expect("terminal");
    let mut key_encoder = key::Encoder::new().expect("key encoder");
    let mut key_event = key::Event::new().expect("key event");
    let mut writer: Box<dyn Write + Send> = Box::new(std::io::sink());
    let mut input_bytes = Vec::new();

    encode_key(
        &terminal,
        &mut key_encoder,
        &mut key_event,
        KeyInput {
            action: KeyAction::Press,
            key: KeyCode::Backspace,
            modifiers: crate::Modifiers::default(),
            text: None,
            unshifted_codepoint: None,
        },
        Some(0x7f),
        &mut writer,
        &mut input_bytes,
    )
    .expect("encode backspace");

    assert_eq!(
        input_bytes.as_slice(),
        b"\x7f",
        "pinned tmux writes 0x7f for send-keys BSpace while the backspace option holds its C-? default"
    );
}

#[test]
fn key_and_mouse_encoding_reuse_the_actor_scratch_buffer() {
    let mut terminal = new_terminal(80, 24, 16).expect("terminal");
    terminal.vt_write(b"\x1b[?1000h\x1b[?1006h");

    let mut key_encoder = key::Encoder::new().expect("key encoder");
    let mut key_event = key::Event::new().expect("key event");
    let mut mouse_encoder = mouse::Encoder::new().expect("mouse encoder");
    let mut mouse_event = mouse::Event::new().expect("mouse event");
    let mut writer: Box<dyn Write + Send> = Box::new(std::io::sink());
    let mut input_bytes = Vec::with_capacity(LINK_URI_SCRATCH_BYTES);
    let mut selection = None;
    let mut hover_link = None;
    let mut button_pressed = false;
    let word_separators = WordSeparators::default();

    encode_key(
        &terminal,
        &mut key_encoder,
        &mut key_event,
        KeyInput {
            action: KeyAction::Press,
            key: KeyCode::ArrowUp,
            modifiers: crate::Modifiers::default(),
            text: None,
            unshifted_codepoint: None,
        },
        Some(0x7f),
        &mut writer,
        &mut input_bytes,
    )
    .expect("warm key encoding");
    route_mouse_input(
        &mut terminal,
        &mut selection,
        &mut hover_link,
        None,
        test_pointer_input(TerminalMousePhase::Press, 0),
        &mut writer,
        &mut mouse_encoder,
        &mut mouse_event,
        &mut button_pressed,
        &mut input_bytes,
        &word_separators,
        &HashSet::new(),
    )
    .expect("warm mouse encoding");

    let allocation = input_bytes.as_ptr();
    let capacity = input_bytes.capacity();
    for column in 1..=128_u16 {
        encode_key(
            &terminal,
            &mut key_encoder,
            &mut key_event,
            KeyInput {
                action: KeyAction::Press,
                key: KeyCode::Character('x'),
                modifiers: crate::Modifiers::default(),
                text: Some(Box::from("x")),
                unshifted_codepoint: Some('x'),
            },
            Some(0x7f),
            &mut writer,
            &mut input_bytes,
        )
        .expect("key encoding");
        route_mouse_input(
            &mut terminal,
            &mut selection,
            &mut hover_link,
            None,
            test_pointer_input(TerminalMousePhase::Motion, column),
            &mut writer,
            &mut mouse_encoder,
            &mut mouse_event,
            &mut button_pressed,
            &mut input_bytes,
            &word_separators,
            &HashSet::new(),
        )
        .expect("mouse encoding");
    }

    assert_eq!(input_bytes.as_ptr(), allocation);
    assert_eq!(input_bytes.capacity(), capacity);
}

#[test]
fn boxed_key_text_moves_into_libghostty_without_reallocation() {
    let text = Box::<str>::from("composed λ");
    let allocation = text.as_ptr();
    let mut event = key::Event::new().expect("key event");

    event.set_utf8(Some(text));

    assert_eq!(event.utf8().expect("stored key text").as_ptr(), allocation);
}

#[test]
fn focus_events_only_reach_applications_that_enable_reporting() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    let mut output = Vec::new();
    write_focus_event(&terminal, true, &mut output).expect("disabled focus event");
    assert!(output.is_empty());

    terminal.vt_write(b"\x1b[?1004h");
    write_focus_event(&terminal, true, &mut output).expect("focus gained");
    write_focus_event(&terminal, false, &mut output).expect("focus lost");
    assert_eq!(output, b"\x1b[I\x1b[O");
}

#[cfg(unix)]
#[test]
fn prepared_paste_reaches_a_pty_while_its_client_view_is_hidden() {
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn::default(),
    );
    let view = TerminalViewId(42);
    session.paste_prepared_bytes(
        Some(view),
        Arc::from(b"printf 'ZZ_HIDDEN_PASTE_OK\\n'\r".as_slice()),
        false,
    );
    attach_streaming(&session, view);

    let viewport = wait_for_test_viewport(&session, |viewport| {
        let mut contents = String::new();
        for cell in viewport.cells.iter() {
            viewport.push_glyph(*cell, &mut contents);
        }
        contents.contains("ZZ_HIDDEN_PASTE_OK")
    });
    assert!(matches!(viewport.status, SessionStatus::Running));
}
