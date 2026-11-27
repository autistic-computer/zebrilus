use super::*;
use zz_terminal::CursorBlinkPolicy;

#[ala::test]
fn hidden_and_blurred_terminals_stop_blinking_without_another_render(cx: &mut ala::TestAppContext) {
    cx.update(zz_ui::init);
    let (terminal, cx) = cx.add_window_view(|window, cx| {
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "test client".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        TerminalView::new(PaneId(0), mux, Rc::new(Cell::new(false)), window, cx)
    });
    cx.update(|window, cx| {
        window.activate_window();
        terminal.update(cx, |view, cx| {
            window.focus(&view.focus_handle, cx);
            view.retained.write().viewport.cursor = Some(zz_terminal::Cursor::new(
                0,
                0,
                true,
                true,
                false,
                zz_terminal::CursorStyle::Block,
                zz_terminal::Color::rgb(1, 2, 3),
            ));
            view.cursor_focused = true;
            view.ensure_cursor_blink(cx);
            assert!(view.cursor_blink_active);
            view.set_visible(false, cx);
            assert!(!view.cursor_blink_active);
            assert!(view.cursor_blink_visible);
        });
    });
    cx.run_until_parked();
    terminal.update(cx, |view, cx| {
        assert!(!view.cursor_blink_active);
        view.set_visible(true, cx);
        assert!(view.cursor_blink_active);
    });
    cx.deactivate_window();
    terminal.update(cx, |view, _| assert!(!view.cursor_blink_active));
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    terminal.update(cx, |view, _| assert!(view.cursor_blink_active));
    cx.update(|window, cx| {
        let other_focus = cx.focus_handle();
        window.focus(&other_focus, cx);
    });
    terminal.update(cx, |view, _| {
        assert!(!view.cursor_blink_active);
        assert!(!view.cursor_focused);
    });
}

#[ala::test]
fn cursor_blinking_settles_visible_after_idle_until_input(cx: &mut ala::TestAppContext) {
    cx.update(zz_ui::init);
    let (terminal, cx) = cx.add_window_view(|window, cx| {
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "test client".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        TerminalView::new(PaneId(0), mux, Rc::new(Cell::new(false)), window, cx)
    });
    cx.update(|window, cx| {
        window.activate_window();
        terminal.update(cx, |view, cx| {
            window.focus(&view.focus_handle, cx);
            view.retained.write().viewport.cursor = Some(zz_terminal::Cursor::new(
                0,
                0,
                true,
                true,
                false,
                zz_terminal::CursorStyle::Block,
                zz_terminal::Color::rgb(1, 2, 3),
            ));
            view.cursor_focused = true;
            view.reset_cursor_blink(cx);
            assert!(view.cursor_blink_active);
        });
    });
    for _ in 0..(CURSOR_BLINK_TIMEOUT.as_millis() / 250 + 8) {
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();
    }
    terminal.update(cx, |view, cx| {
        assert!(!view.cursor_blink_active);
        assert!(view.cursor_blink_expired);
        assert!(view.cursor_blink_visible);
        view.ensure_cursor_blink(cx);
        assert!(!view.cursor_blink_active);
        view.reset_cursor_blink(cx);
        assert!(view.cursor_blink_active);
        assert!(!view.cursor_blink_expired);
    });
}

#[test]
fn cursor_blinking_is_suspended_while_unfocused() {
    let cursor = zz_terminal::Cursor::new(
        0,
        0,
        true,
        true,
        false,
        zz_terminal::CursorStyle::Block,
        zz_terminal::Color::rgb(1, 2, 3),
    );
    assert!(cursor_should_blink(
        Some(cursor),
        CursorBlinkPolicy::Terminal,
        true
    ));
    assert!(!cursor_should_blink(
        Some(cursor),
        CursorBlinkPolicy::Terminal,
        false
    ));
    assert!(!cursor_should_blink(
        Some(cursor),
        CursorBlinkPolicy::Off,
        true
    ));
}
