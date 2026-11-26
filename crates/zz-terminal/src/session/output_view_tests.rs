use super::test_support::*;
use super::*;

#[test]
fn command_output_view_is_pty_free_reflowable_and_closes() {
    let text = (0..40)
        .map(|line| format!("\x1b[3{}mline {line:02} αβ\x1b[0m", line % 7 + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let session = TerminalSession::spawn_output_view("list-keys".to_owned(), text);
    let events = session.events();
    let view = TerminalViewId(77);
    attach_streaming(&session, view);

    let viewport = wait_for_test_viewport(&session, |viewport| {
        matches!(viewport.mode, TerminalMode::View { .. })
            && matches!(viewport.status, SessionStatus::Running)
    });
    assert_eq!(viewport.title(), "list-keys");
    assert_eq!(viewport.columns, INITIAL_COLUMNS);
    assert_eq!(viewport.rows, INITIAL_ROWS);
    let capture = session
        .capture(CaptureOptions {
            start: CaptureBoundary::HistoryStart,
            end: CaptureBoundary::Relative(i64::MAX),
            mode: true,
            ..CaptureOptions::default()
        })
        .expect("capture frozen command output");
    assert!(capture.contains("line 00 αβ"));
    assert!(capture.contains("line 39 αβ"));
    assert!(!capture.contains("\x1b["));

    session.resize(16, 5, 8, 18);
    let viewport = wait_for_test_viewport(&session, |viewport| {
        viewport.columns == 16
            && viewport.rows == 5
            && matches!(viewport.mode, TerminalMode::View { .. })
    });
    assert!(matches!(
        viewport.mode,
        TerminalMode::View { total, .. } if total > 5
    ));

    session.view_action(
        view,
        TerminalViewAction::SearchBegin(SearchQuery::literal("line 31")),
    );
    let viewport = wait_for_test_viewport(&session, |viewport| {
        viewport
            .search
            .is_some_and(|search| !search.pending() && search.total == 1)
    });
    assert_eq!(viewport.search.expect("completed search").total, 1);

    session.view_action(view, TerminalViewAction::SelectAll);
    session.view_action(
        view,
        TerminalViewAction::CopySelection {
            request_id: 9,
            target: ClipboardTarget::Clipboard,
        },
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        match events.try_recv() {
            Ok(TerminalEvent::CopyReady { copy, .. }) if copy.request_id == 9 => {
                assert!(copy.text.contains("line 00 αβ"));
                assert!(copy.text.contains("line 39 αβ"));
                break;
            }
            Ok(_) | Err(async_channel::TryRecvError::Empty) => {}
            Err(async_channel::TryRecvError::Closed) => {
                panic!("command-output actor closed before copying")
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for output selection"
        );
        thread::sleep(Duration::from_millis(10));
    }

    session.view_action(view, TerminalViewAction::CopyMode(CopyModeAction::Cancel));
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        match events.try_recv() {
            Ok(TerminalEvent::ViewClosed(_)) => break,
            Ok(_) | Err(async_channel::TryRecvError::Empty) => {}
            Err(async_channel::TryRecvError::Closed) => {
                panic!("command-output actor closed without a view-close event")
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for output view to close"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn ordinary_and_startup_output_views_use_their_distinct_scrollback_caps() {
    let ordinary = TerminalSession::spawn_output_view(String::new(), String::new());
    let startup = TerminalSession::spawn_startup_output_view_with_appearance(
        String::new(),
        String::new(),
        Arc::new(TerminalAppearance::default()),
    );

    assert_eq!(ordinary.max_scrollback(), 100_000);
    assert_eq!(startup.max_scrollback(), 64 * 1024 * 1024);
}

#[test]
fn empty_surface_is_live_pty_free_and_ignores_input() {
    let session =
        TerminalSession::spawn_empty_with_appearance(64, Arc::new(TerminalAppearance::default()));
    let view = TerminalViewId(91);
    attach_streaming(&session, view);

    let viewport = wait_for_test_viewport(&session, |viewport| {
        matches!(viewport.mode, TerminalMode::Live)
            && matches!(viewport.status, SessionStatus::Running)
    });
    assert_eq!(viewport.columns, INITIAL_COLUMNS);
    assert_eq!(viewport.rows, INITIAL_ROWS);
    assert_eq!(session.process_id(), None);
    assert_eq!(session.foreground_process_id(), None);
    assert_eq!(session.tty(), None);

    session.send_text("ZZ_EMPTY_MUST_STAY_BLANK");
    let capture = session
        .capture(CaptureOptions::default())
        .expect("capture empty surface");
    assert!(!capture.contains("ZZ_EMPTY_MUST_STAY_BLANK"));

    session.resize(23, 7, 8, 18);
    let viewport = wait_for_test_viewport(&session, |viewport| {
        viewport.columns == 23 && viewport.rows == 7 && matches!(viewport.mode, TerminalMode::Live)
    });
    assert_eq!(viewport.status, SessionStatus::Running);
}

#[test]
fn live_appearance_update_preserves_command_output_and_resets_colors() {
    let mut initial = TerminalAppearance {
        background: Color::rgb(0x11, 0x22, 0x33),
        ..TerminalAppearance::default()
    };
    initial.palette[3] = Color::rgb(0x44, 0x55, 0x66);
    let session = TerminalSession::spawn_output_view_with_appearance(
        "appearance".to_owned(),
        "preserved content".to_owned(),
        Arc::new(initial.clone()),
    );
    attach_streaming(&session, TerminalViewId(93));
    let before = wait_for_test_viewport(&session, |viewport| {
        matches!(viewport.status, SessionStatus::Running)
            && viewport.background == initial.background
    });
    let dictionary_generation = before.dictionary_generation;

    let mut next = initial;
    next.color_scheme = TerminalColorScheme::Light;
    next.background = Color::rgb(0xee, 0xdd, 0xcc);
    next.palette[3] = Color::rgb(0xaa, 0xbb, 0xcc);
    session.set_appearance(Arc::new(next.clone()));
    let after = wait_for_test_viewport(&session, |viewport| {
        viewport.background == next.background
            && viewport.dictionary_generation != dictionary_generation
    });
    assert_eq!(after.background, next.background);
    let captured = session
        .capture(CaptureOptions {
            start: CaptureBoundary::HistoryStart,
            end: CaptureBoundary::Relative(i64::MAX),
            mode: true,
            ..CaptureOptions::default()
        })
        .expect("capture after appearance update");
    assert!(captured.contains("preserved content"));
}

fn history_row_text(row: &[PackedCell], dictionary: &TerminalDictionary) -> String {
    let mut output = String::new();
    for cell in row {
        let glyph = cell.glyph();
        if glyph == 0 {
            continue;
        }
        if glyph & GRAPHEME_TABLE_BIT == 0 {
            if let Some(character) = char::from_u32(glyph) {
                output.push(character);
            }
            continue;
        }
        let index = usize::try_from(glyph & !GRAPHEME_TABLE_BIT).unwrap_or(usize::MAX);
        let Some((&start, &end)) = dictionary
            .grapheme_offsets
            .get(index)
            .zip(dictionary.grapheme_offsets.get(index.saturating_add(1)))
        else {
            continue;
        };
        let Some(bytes) = dictionary.grapheme_bytes.get(start as usize..end as usize) else {
            continue;
        };
        if let Ok(grapheme) = std::str::from_utf8(bytes) {
            output.push_str(grapheme);
        }
    }
    output
}

#[cfg(unix)]
#[test]
fn history_api_is_clamped_self_contained_and_does_not_move_a_view() {
    let session = TerminalSession::spawn(
        128,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            command: Some(vec![
                "read _; i=0; while [ $i -lt 20 ]; do printf 'ZZH%02d\\n' \"$i\"; i=$((i+1)); done; printf 'ZZ_HISTORY_DONE\\n'; read _"
                    .to_owned(),
            ]),
            ..TerminalSpawn::default()
        },
    );
    let view = TerminalViewId(96);
    session.resize(24, 4, 8, 18);
    attach_streaming(&session, view);
    wait_for_test_viewport(&session, |viewport| {
        viewport.columns == 24
            && viewport.rows == 4
            && matches!(viewport.status, SessionStatus::Running)
    });
    session.send_text("ready\n");
    wait_for_test_viewport(&session, |viewport| {
        let mut contents = String::new();
        for cell in viewport.cells.iter() {
            viewport.push_glyph(*cell, &mut contents);
        }
        contents.contains("ZZ_HISTORY_DONE") && viewport.scrollbar.total > viewport.scrollbar.len
    });

    session.view_action(view, TerminalViewAction::ScrollTop);
    let before = wait_for_test_viewport(&session, |viewport| viewport.scrollbar.offset == 0);
    let before_generation = (before.generation, before.view_generation, before.scrollbar);

    let (start, rows, dictionary, scrollbar, columns) =
        session.history(0, u32::MAX).expect("history");
    assert_eq!(start, 0);
    assert_eq!(columns, 24);
    assert_eq!(
        rows.len(),
        usize::try_from(scrollbar.total.saturating_sub(scrollbar.len))
            .expect("small fixture history")
    );
    assert!(rows.iter().all(|row| row.len() == usize::from(columns)));
    let history_text = rows
        .iter()
        .map(|row| history_row_text(row, &dictionary))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(history_text.contains("ZZH"), "{history_text:?}");

    let (clamped_start, clamped, _, clamped_scrollbar, _) =
        session.history(u32::MAX, 10).expect("clamped history");
    assert_eq!(
        clamped_start,
        clamped_scrollbar
            .total
            .saturating_sub(clamped_scrollbar.len)
    );
    assert!(clamped.is_empty());

    let after = session
        .latest_viewport_for(view)
        .expect("attached view remains published");
    assert_eq!(
        (after.generation, after.view_generation, after.scrollbar),
        before_generation
    );
}

fn assert_release_view_publishes_only_for_active_views(session: &TerminalSession) {
    let attached = TerminalViewId(11);
    let parked = TerminalViewId(12);
    session.resize(8, 2, 8, 18);
    attach_streaming(session, attached);
    session.attach_view(parked);
    session.detach_view(parked);
    wait_for_test_viewport(session, |viewport| {
        viewport.columns == 8
            && viewport.rows == 2
            && matches!(viewport.status, SessionStatus::Running)
    });
    let fence = |session: &TerminalSession| {
        session.history(0, 1).expect("worker round trip");
    };
    fence(session);
    let before = session.latest_viewport();
    assert!(session.latest_viewport_for(attached).is_some());
    assert!(session.latest_viewport_for(parked).is_none());

    session.release_view(TerminalViewId(13));
    fence(session);
    assert_eq!(
        session.latest_viewport().view_generation,
        before.view_generation,
        "releasing a view that was never attached must not publish"
    );

    session.release_view(parked);
    fence(session);
    assert_eq!(
        session.latest_viewport().view_generation,
        before.view_generation,
        "releasing a parked view must not publish"
    );
    assert!(session.latest_viewport_for(attached).is_some());

    session.release_view(attached);
    fence(session);
    assert!(
        session.latest_viewport_for(attached).is_none(),
        "releasing an active view must publish without it"
    );
    assert!(session.latest_viewports().is_empty());
}

#[test]
fn output_view_release_publishes_only_for_active_views() {
    let session = TerminalSession::spawn_output_view("release".to_owned(), "abc".to_owned());
    assert_release_view_publishes_only_for_active_views(&session);
}

#[cfg(unix)]
#[test]
fn terminal_release_publishes_only_for_active_views() {
    let session = TerminalSession::spawn(
        128,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            command: Some(vec!["read _".to_owned()]),
            ..TerminalSpawn::default()
        },
    );
    assert_release_view_publishes_only_for_active_views(&session);
}

#[test]
fn output_view_history_is_empty() {
    let session = TerminalSession::spawn_output_view("history".to_owned(), "one\ntwo".to_owned());
    let (_, rows, _, _, _) = session.history(0, 512).expect("empty output history");
    assert!(rows.is_empty());
}
