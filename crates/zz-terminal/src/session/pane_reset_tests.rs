use super::test_support::*;
use super::*;

#[test]
fn pane_reset_scrolls_the_used_rows_into_history_like_the_pinned_send_keys_r() {
    let mut terminal = new_terminal(20, 6, 1 << 16).expect("terminal");
    for line in 1..=20 {
        terminal.vt_write(format!("L{line:02}\r\n").as_bytes());
    }
    assert_eq!(terminal.scrollback_rows().expect("scrollback"), 15);

    reset_pane_screen(&mut terminal).expect("reset");

    assert_eq!(
        terminal.scrollback_rows().expect("scrollback"),
        20,
        "pinned tmux answers history_size 15 before send-keys -R and 20 after on this pane"
    );
    assert_eq!(terminal.cursor_x().expect("cursor x"), 0);
    assert_eq!(terminal.cursor_y().expect("cursor y"), 0);
    let history = capture_terminal(
        &terminal,
        None,
        CaptureOptions {
            start: CaptureBoundary::HistoryStart,
            ..CaptureOptions::default()
        },
    )
    .expect("capture history");
    let lines = history.split('\n').collect::<Vec<_>>();
    assert_eq!(lines.first().copied(), Some("L01"));
    assert_eq!(lines.get(19).copied(), Some("L20"));
    assert!(
        lines[20..].iter().all(|line| line.is_empty()),
        "the pin leaves the visible rows blank after -R: {lines:?}"
    );
}

#[test]
fn pane_reset_restores_the_pinned_default_tab_stops() {
    let mut terminal = new_terminal(40, 6, 1 << 16).expect("terminal");
    terminal.vt_write(b"\x1b[3g\x1b[1;5H\x1bH\x1b[2;1H\t");
    assert_eq!(terminal.cursor_x().expect("cursor x"), 4);

    reset_pane_screen(&mut terminal).expect("reset");
    terminal.vt_write(b"\t");

    assert_eq!(
        terminal.cursor_x().expect("cursor x"),
        8,
        "pinned tmux answers cursor_x 4 with a custom tab stop and 8 after send-keys -R"
    );
}

#[test]
fn pane_reset_clears_the_pane_palette_and_the_active_pen() {
    let mut terminal = new_terminal(20, 4, 1 << 16).expect("terminal");
    terminal.vt_write(b"\x1b]4;42;rgb:12/34/56\x1b\\\x1b[38;5;42mA");
    let before = snapshot_fixture(&terminal);
    let cell = before.row(0).expect("first row")[0];
    assert_eq!(
        before.style(cell).expect("style").foreground(),
        Color::rgb(0x12, 0x34, 0x56)
    );

    reset_pane_screen(&mut terminal).expect("reset");
    terminal.vt_write(b"B\x1b[38;5;42mC");

    let mut fresh = new_terminal(20, 4, 1 << 16).expect("terminal");
    fresh.vt_write(b"B\x1b[38;5;42mC");
    let expected = snapshot_fixture(&fresh);
    let expected_row = expected.row(0).expect("first row");

    let after = snapshot_fixture(&terminal);
    let row = after.row(0).expect("first row");
    assert_eq!(
        after.style(row[0]).expect("style").foreground(),
        expected.style(expected_row[0]).expect("style").foreground(),
        "the pin's input_reset_cell puts the pen back to the default cell"
    );
    assert_eq!(
        after.style(row[1]).expect("style").foreground(),
        expected.style(expected_row[1]).expect("style").foreground(),
        "the pin's colour_palette_clear drops the pane's OSC 4 overrides"
    );
    assert_ne!(
        after.style(row[1]).expect("style").foreground(),
        Color::rgb(0x12, 0x34, 0x56)
    );
}

#[test]
fn pane_reset_drops_the_scroll_region_and_abandons_a_partial_sequence() {
    let mut terminal = new_terminal(20, 6, 1 << 16).expect("terminal");
    terminal.vt_write(b"one\r\ntwo\r\n\x1b[2;4r\x1bPtmux;partial");
    let before = terminal.scrollback_rows().expect("scrollback");

    reset_pane_screen(&mut terminal).expect("reset");
    terminal.vt_write(b"after");

    assert_eq!(
        terminal.scrollback_rows().expect("scrollback"),
        before + 2,
        "the pin's screen_write_scrollregion restores the full region before the clear scrolls"
    );
    let visible =
        capture_terminal(&terminal, None, CaptureOptions::default()).expect("capture visible");
    assert_eq!(
        visible.trim_end_matches('\n'),
        "after",
        "the pin's input_reset abandons the half-parsed sequence and returns to ground"
    );
}

#[test]
fn pane_reset_drops_the_modes_the_pin_clears_with_send_keys_r() {
    let mut terminal = new_terminal(20, 4, 1 << 16).expect("terminal");
    let mut key_encoder = key::Encoder::new().expect("key encoder");
    let mut key_event = key::Event::new().expect("key event");
    let mut writer: Box<dyn Write + Send> = Box::new(std::io::sink());
    let up = |terminal: &Terminal<'_, '_>,
              key_encoder: &mut key::Encoder<'_>,
              key_event: &mut key::Event<'_>,
              writer: &mut Box<dyn Write + Send>,
              key: KeyCode| {
        let mut input_bytes = Vec::new();
        encode_key(
            terminal,
            key_encoder,
            key_event,
            KeyInput {
                action: KeyAction::Press,
                key,
                modifiers: crate::Modifiers::default(),
                text: None,
                unshifted_codepoint: None,
            },
            Some(0x7f),
            writer.as_mut(),
            &mut input_bytes,
        )
        .expect("encode key");
        input_bytes
    };

    terminal.vt_write(b"\x1b[?1h\x1b[4h\x1b[?6h");
    assert_eq!(
        up(
            &terminal,
            &mut key_encoder,
            &mut key_event,
            &mut writer,
            KeyCode::ArrowUp
        ),
        b"\x1bOA"
    );
    terminal.vt_write(b"\x1b[=1u");
    assert_eq!(
        up(
            &terminal,
            &mut key_encoder,
            &mut key_event,
            &mut writer,
            KeyCode::Escape
        ),
        b"\x1b[27u"
    );

    reset_pane_screen(&mut terminal).expect("reset");

    assert_eq!(
        up(
            &terminal,
            &mut key_encoder,
            &mut key_event,
            &mut writer,
            KeyCode::ArrowUp
        ),
        b"\x1b[A",
        "pinned tmux answers keypad_cursor_flag 1 before send-keys -R and 0 after, and Up then echoes ^[[A"
    );
    assert_eq!(
        up(
            &terminal,
            &mut key_encoder,
            &mut key_event,
            &mut writer,
            KeyCode::Escape
        ),
        b"\x1b",
        "the pin's mode reset drops MODE_KEYS_EXTENDED with the rest"
    );
    terminal.vt_write(b"ab\rX");
    let visible =
        capture_terminal(&terminal, None, CaptureOptions::default()).expect("capture visible");
    assert_eq!(
        visible.trim_end_matches('\n'),
        "Xb",
        "pinned tmux answers insert_flag 1 before send-keys -R and 0 after, so X overwrites a"
    );
    terminal.vt_write(b"\x1b[3;5H\x1b8Y");
    assert_eq!(terminal.cursor_x().expect("cursor x"), 1);
    assert_eq!(
        terminal.cursor_y().expect("cursor y"),
        0,
        "the pin's input_reset_cell puts the saved cursor back at 0,0"
    );
}
