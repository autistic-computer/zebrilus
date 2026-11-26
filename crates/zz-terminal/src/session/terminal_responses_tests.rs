use super::*;

#[test]
fn terminal_responses_reuse_one_ordered_actor_buffer() {
    let effects = Rc::new(RefCell::new(PtyEffects::new()));
    let effect_sink = Rc::clone(&effects);
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal
        .on_pty_write(move |_, bytes| effect_sink.borrow_mut().push(bytes))
        .expect("PTY response callback");

    terminal.vt_write(b"\x1b[5n\x1b[6n");
    assert_eq!(effects.borrow().bytes, b"\x1b[0n\x1b[1;1R");
    let allocation = effects.borrow().bytes.as_ptr();
    let capacity = effects.borrow().bytes.capacity();
    let mut output = Vec::new();
    drain_effects(&effects, &mut output).expect("first response drain");
    assert_eq!(output, b"\x1b[0n\x1b[1;1R");
    assert!(effects.borrow().bytes.is_empty());

    terminal.vt_write(b"\x1b[5n\x1b[6n");
    drain_effects(&effects, &mut output).expect("second response drain");
    assert_eq!(output, b"\x1b[0n\x1b[1;1R\x1b[0n\x1b[1;1R");
    assert_eq!(effects.borrow().bytes.as_ptr(), allocation);
    assert_eq!(effects.borrow().bytes.capacity(), capacity);
}

#[test]
fn device_attributes_answer_da1_after_kitty_probe_queries() {
    let effects = Rc::new(RefCell::new(PtyEffects::new()));
    let effect_sink = Rc::clone(&effects);
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal
        .on_pty_write(move |_, bytes| effect_sink.borrow_mut().push(bytes))
        .expect("PTY response callback");
    register_device_attributes(&mut terminal).expect("device attributes callback");

    terminal.vt_write(b"\x1b[c");
    assert_eq!(effects.borrow().bytes, b"\x1b[?62;22c");
}

#[test]
fn title_report_queries_stay_unanswered_like_the_pinned_tmux() {
    let effects = Rc::new(RefCell::new(PtyEffects::new()));
    let effect_sink = Rc::clone(&effects);
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal
        .on_pty_write(move |_, bytes| effect_sink.borrow_mut().push(bytes))
        .expect("PTY response callback");

    terminal.vt_write(b"\x1b]2;echo pwned\x07\x1b[21t");
    assert_eq!(terminal.title().expect("title"), "echo pwned");
    assert!(effects.borrow().bytes.is_empty());
}

#[test]
fn terminal_reports_configured_colors_for_osc_10_and_11_queries() {
    let effects = Rc::new(RefCell::new(PtyEffects::new()));
    let effect_sink = Rc::clone(&effects);
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal
        .on_pty_write(move |_, bytes| effect_sink.borrow_mut().push(bytes))
        .expect("PTY response callback");
    apply_terminal_appearance(&mut terminal, &TerminalAppearance::default())
        .expect("terminal appearance");

    terminal.vt_write(b"\x1b]10;?\x1b\\\x1b]11;?\x07");

    assert_eq!(
        effects.borrow().bytes,
        b"\x1b]10;rgb:d8d8/dede/e9e9\x1b\\\x1b]11;rgb:1010/1313/1818\x07"
    );
}

#[test]
fn osc_52_writes_become_pane_scoped_clipboard_events() {
    let event_state = Arc::new(EventQueueState::new());
    let (event_tx, events) = terminal_event_channel(&event_state);
    let publisher = Publisher {
        event_tx,
        latest: Arc::new(RwLock::new(PublishedViewports::new(
            TerminalViewport::blank(1, 1, SessionStatus::Running),
        ))),
        state: Arc::clone(&event_state),
    };
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    register_clipboard_write(&mut terminal, publisher).expect("clipboard write callback");

    terminal.vt_write(b"\x1b]52;c;enogY2xpcGJvYXJk\x07");
    assert!(matches!(
        events.try_recv().expect("clipboard write event"),
        TerminalEvent::ClipboardSet {
            target: ClipboardTarget::Clipboard,
            text,
        } if text == "zz clipboard"
    ));

    terminal.vt_write(b"\x1b]52;p;cHJpbWFyeSBwaWNr\x1b\\");
    assert!(matches!(
        events.try_recv().expect("primary selection write event"),
        TerminalEvent::ClipboardSet {
            target: ClipboardTarget::Primary,
            text,
        } if text == "primary pick"
    ));

    terminal.vt_write(b"\x1b]52;c;?\x07");
    assert!(matches!(
        events.try_recv(),
        Err(async_channel::TryRecvError::Empty)
    ));
}

#[test]
fn bel_raises_each_occurrence() {
    let event_state = Arc::new(EventQueueState::new());
    let (event_tx, events) = terminal_event_channel(&event_state);
    let publisher = Publisher {
        event_tx,
        latest: Arc::new(RwLock::new(PublishedViewports::new(
            TerminalViewport::blank(1, 1, SessionStatus::Running),
        ))),
        state: Arc::clone(&event_state),
    };
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    register_bell(&mut terminal, publisher).expect("bell callback");

    terminal.vt_write(b"\x07\x07");
    assert!(matches!(
        events.try_recv().expect("first bell event"),
        TerminalEvent::Bell
    ));
    assert!(matches!(
        events.try_recv().expect("second bell event"),
        TerminalEvent::Bell
    ));
    assert!(matches!(
        events.try_recv(),
        Err(async_channel::TryRecvError::Empty)
    ));
}

#[test]
fn clipboard_writes_prefer_plain_text_and_refuse_oversize_payloads() {
    let html = ClipboardContent {
        mime: "text/html",
        data: b"<b>hi</b>",
    };
    let plain = ClipboardContent {
        mime: CLIPBOARD_TEXT_MIME,
        data: b"hi",
    };

    assert_eq!(
        clipboard_write_request(ClipboardLocation::Standard, [html, plain].into_iter()),
        Ok((ClipboardTarget::Clipboard, "hi".to_owned()))
    );
    assert_eq!(
        clipboard_write_request(ClipboardLocation::Selection, std::iter::once(html)),
        Ok((ClipboardTarget::Primary, "<b>hi</b>".to_owned()))
    );
    assert_eq!(
        clipboard_write_request(ClipboardLocation::Standard, std::iter::empty()),
        Err(ClipboardWriteError::Unsupported)
    );

    let oversize = "x".repeat(MAX_CLIPBOARD_WRITE_BYTES + 1);
    assert_eq!(
        clipboard_write_request(
            ClipboardLocation::Standard,
            std::iter::once(ClipboardContent {
                mime: CLIPBOARD_TEXT_MIME,
                data: oversize.as_bytes(),
            })
        ),
        Err(ClipboardWriteError::Denied)
    );
}

#[test]
fn terminal_response_limit_never_appends_a_partial_sequence() {
    let mut effects = PtyEffects::new();
    effects.bytes.resize(MAX_PTY_RESPONSE_BYTES - 2, b'x');
    effects.push(b"abc");
    assert!(effects.overflowed);
    assert_eq!(effects.bytes.len(), MAX_PTY_RESPONSE_BYTES - 2);
    effects.push(b"z");
    assert_eq!(effects.bytes.len(), MAX_PTY_RESPONSE_BYTES - 2);
}
