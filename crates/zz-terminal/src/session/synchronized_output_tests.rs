use super::test_support::*;
use super::*;
#[cfg(unix)]
use crate::DEFAULT_HISTORY_LIMIT;

fn synchronized_output_session() -> (TerminalSession, Arc<TerminalViewport>) {
    let session =
        TerminalSession::spawn_empty_with_appearance(64, Arc::new(TerminalAppearance::default()));
    assert!(session.feed(Arc::from(b"BEFORE".as_slice())));
    let before = wait_for_test_viewport(&session, |viewport| {
        viewport
            .cells
            .first()
            .is_some_and(|cell| cell.glyph() == u32::from(b'B'))
    });
    assert!(session.feed(Arc::from(b"\x1b[?2026h\rAFTER!".as_slice())));
    wait_for_test_capture(&session, |capture| capture.contains("AFTER!"));
    assert!(Arc::ptr_eq(&before, &session.latest_viewport()));
    (session, before)
}

#[cfg(unix)]
#[test]
fn live_synchronized_output_holds_the_frame_and_flushes_on_release() {
    let session = TerminalSession::spawn(
        64,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            command: Some(vec![
                "stty -echo; printf BEFORE; read marker; printf '\\033[?2026h\\rAFTER!'; read marker; printf '\\033[?2026l'; read marker".to_owned(),
            ]),
            ..TerminalSpawn::default()
        },
    );
    let before = wait_for_test_viewport(&session, |viewport| {
        viewport
            .cells
            .first()
            .is_some_and(|cell| cell.glyph() == u32::from(b'B'))
    });
    session.send_text("begin\n");
    wait_for_test_capture(&session, |capture| capture.contains("AFTER!"));
    thread::sleep(Duration::from_millis(50));
    assert!(Arc::ptr_eq(&before, &session.latest_viewport()));
    session.send_text("finish\n");
    wait_for_test_viewport(&session, |viewport| {
        viewport
            .cells
            .first()
            .is_some_and(|cell| cell.glyph() == u32::from(b'A'))
    });
}

#[cfg(unix)]
#[test]
fn live_synchronized_output_times_out_without_more_output() {
    let session = TerminalSession::spawn(
        64,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            command: Some(vec![
                "stty -echo; printf BEFORE; read marker; printf '\\033[?2026h\\rAFTER!'; read marker".to_owned(),
            ]),
            ..TerminalSpawn::default()
        },
    );
    let before = wait_for_test_viewport(&session, |viewport| {
        viewport
            .cells
            .first()
            .is_some_and(|cell| cell.glyph() == u32::from(b'B'))
    });
    session.send_text("begin\n");
    wait_for_test_capture(&session, |capture| capture.contains("AFTER!"));
    thread::sleep(Duration::from_millis(50));
    assert!(Arc::ptr_eq(&before, &session.latest_viewport()));
    wait_for_test_viewport(&session, |viewport| {
        matches!(viewport.status, SessionStatus::Running)
            && viewport
                .cells
                .first()
                .is_some_and(|cell| cell.glyph() == u32::from(b'A'))
    });
}

#[test]
fn synchronized_output_holds_the_frame_until_release() {
    let (session, before) = synchronized_output_session();
    attach_streaming(&session, TerminalViewId(991));
    wait_for_test_capture(&session, |capture| capture.contains("AFTER!"));
    assert!(Arc::ptr_eq(&before, &session.latest_viewport()));
    assert!(session.feed(Arc::from(b"\x1b[?2026l".as_slice())));
    wait_for_test_viewport(&session, |viewport| {
        viewport
            .cells
            .first()
            .is_some_and(|cell| cell.glyph() == u32::from(b'A'))
    });
}

#[test]
fn synchronized_output_times_out_without_more_output() {
    let (session, _) = synchronized_output_session();
    wait_for_test_viewport(&session, |viewport| {
        viewport
            .cells
            .first()
            .is_some_and(|cell| cell.glyph() == u32::from(b'A'))
    });
    assert!(session.feed(Arc::from(b"\rLATER!".as_slice())));
    wait_for_test_viewport(&session, |viewport| {
        viewport
            .cells
            .first()
            .is_some_and(|cell| cell.glyph() == u32::from(b'L'))
    });
}

#[test]
fn synchronized_output_is_released_by_resize() {
    let (session, _) = synchronized_output_session();
    session.resize(100, 30, 8, 18);
    wait_for_test_viewport(&session, |viewport| {
        viewport.columns == 100
            && viewport
                .cells
                .first()
                .is_some_and(|cell| cell.glyph() == u32::from(b'A'))
    });
}

#[cfg(unix)]
#[test]
fn live_shell_round_trip_reaches_render_snapshot() {
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn::default(),
    );
    let view = TerminalViewId(42);
    attach_streaming(&session, view);
    session.send_text("printf 'ZZ_TERMINAL_ROUND_TRIP\\n'\n");

    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        while session.events().try_recv().is_ok() {}
        let viewport = session.latest_viewport();
        let mut contents = String::new();
        for cell in viewport.cells.iter() {
            viewport.push_glyph(*cell, &mut contents);
        }
        if contents.contains("ZZ_TERMINAL_ROUND_TRIP") {
            assert!(matches!(viewport.status, SessionStatus::Running));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "shell output never reached a terminal snapshot; last status: {:?}",
            viewport.status
        );
        thread::sleep(Duration::from_millis(20));
    }

    session.view_action(
        view,
        TerminalViewAction::SearchUpdate(SearchQuery {
            text: "ZZ_TERMINAL_[A-Z_]+".to_owned(),
            mode: SearchMode::Regex,
            case: SearchCase::Sensitive,
            direction: SearchDirection::Forward,
        }),
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        while session.events().try_recv().is_ok() {}
        let viewport = session.latest_viewport();
        if viewport.search.is_some_and(|search| {
            !search.pending() && !search.invalid_pattern() && search.total > 0
        }) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "background regex search never completed: {:?}",
            viewport.search
        );
        thread::sleep(Duration::from_millis(20));
    }
}
