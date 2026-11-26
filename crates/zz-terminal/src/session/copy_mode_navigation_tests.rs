use super::test_support::*;
use super::*;
#[cfg(unix)]
use crate::DEFAULT_HISTORY_LIMIT;

fn selection_lifecycle_fixture(
    text: &[u8],
    cols: u16,
    rows: u16,
) -> (Option<SelectionState>, CopyModeSlot) {
    let mut terminal = new_terminal(cols, rows, 16).expect("terminal");
    terminal.vt_write(text);
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    (selection, copy_mode)
}

fn run_copy_actions(
    text: &[u8],
    cols: u16,
    rows: u16,
    start: PointCoordinate,
    actions: &[CopyModeAction],
) -> CopyModeState {
    let mut terminal = new_terminal(cols, rows, 16).expect("terminal");
    terminal.vt_write(text);
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let row = copy_mode.as_ref().expect("mode").viewport_offset;
    copy_mode.as_mut().expect("mode").cursor = PointCoordinate {
        x: start.x,
        y: row.saturating_add(start.y),
    };
    let mut unseen_output = 0;
    for action in actions {
        apply_copy_mode_action(
            &mut terminal,
            &mut selection,
            &mut copy_mode,
            &mut unseen_output,
            action.clone(),
            &WordSeparators::default(),
            false,
        )
        .expect("copy action");
    }
    *copy_mode.expect("mode survives the sequence")
}

#[test]
fn selection_mode_word_widens_the_live_selection_to_whole_words() {
    let forward = run_copy_actions(
        b"alpha beta gamma",
        20,
        2,
        PointCoordinate { x: 2, y: 0 },
        &[
            CopyModeAction::StartSelection,
            CopyModeAction::SelectionMode(CopySelectionMode::Word),
            CopyModeAction::Right,
            CopyModeAction::Right,
            CopyModeAction::Right,
            CopyModeAction::Right,
            CopyModeAction::Right,
        ],
    );
    let row = forward.cursor.y;
    let selection = forward.selection.expect("selection");
    assert_eq!(forward.cursor.x, 7);
    assert_eq!(selection.anchor, PointCoordinate { x: 0, y: row });
    assert_eq!(selection.focus, PointCoordinate { x: 9, y: row });
    assert_eq!(selection.mode, SelectionMode::Word);

    let backward = run_copy_actions(
        b"alpha beta gamma",
        20,
        2,
        PointCoordinate { x: 7, y: 0 },
        &[
            CopyModeAction::StartSelection,
            CopyModeAction::SelectionMode(CopySelectionMode::Word),
            CopyModeAction::Left,
            CopyModeAction::Left,
            CopyModeAction::Left,
            CopyModeAction::Left,
            CopyModeAction::Left,
        ],
    );
    let row = backward.cursor.y;
    let selection = backward.selection.expect("selection");
    assert_eq!(backward.cursor.x, 2);
    assert_eq!(selection.anchor, PointCoordinate { x: 9, y: row });
    assert_eq!(selection.focus, PointCoordinate { x: 0, y: row });
}

#[test]
fn word_selection_resolves_a_whitespace_cursor_outward_like_the_pin() {
    let forward = run_copy_actions(
        b"alpha beta gamma",
        20,
        2,
        PointCoordinate { x: 7, y: 0 },
        &[CopyModeAction::SelectWord, CopyModeAction::Right],
    );
    let row = forward.cursor.y;
    let selection = forward.selection.expect("selection");
    assert_eq!(forward.cursor.x, 11);
    assert_eq!(selection.anchor, PointCoordinate { x: 6, y: row });
    assert_eq!(selection.focus, PointCoordinate { x: 15, y: row });

    let backward = run_copy_actions(
        b"alpha beta gamma",
        20,
        2,
        PointCoordinate { x: 7, y: 0 },
        &[
            CopyModeAction::SelectWord,
            CopyModeAction::Left,
            CopyModeAction::Left,
            CopyModeAction::Left,
            CopyModeAction::Left,
        ],
    );
    let row = backward.cursor.y;
    let selection = backward.selection.expect("selection");
    assert_eq!(backward.cursor.x, 6);
    assert_eq!(selection.anchor, PointCoordinate { x: 9, y: row });
    assert_eq!(selection.focus, PointCoordinate { x: 6, y: row });
}

#[test]
fn selection_mode_line_widens_the_live_selection_to_whole_lines() {
    let mode = run_copy_actions(
        b"one\r\ntwo\r\nthree",
        10,
        3,
        PointCoordinate { x: 1, y: 0 },
        &[
            CopyModeAction::StartSelection,
            CopyModeAction::SelectionMode(CopySelectionMode::Line),
            CopyModeAction::Down,
        ],
    );
    let selection = mode.selection.expect("selection");
    let origin = mode.cursor.y.saturating_sub(1);
    assert_eq!(selection.anchor, PointCoordinate { x: 0, y: origin });
    assert_eq!(
        selection.focus,
        PointCoordinate {
            x: mode.revision.columns - 1,
            y: mode.cursor.y,
        }
    );
    assert_eq!(selection.mode, SelectionMode::Line);
}

#[test]
fn begin_selection_and_other_end_reset_the_selection_unit_to_characters() {
    let mode = run_copy_actions(
        b"alpha beta gamma",
        20,
        2,
        PointCoordinate { x: 2, y: 0 },
        &[
            CopyModeAction::SelectionMode(CopySelectionMode::Word),
            CopyModeAction::StartSelection,
            CopyModeAction::Right,
            CopyModeAction::Right,
        ],
    );
    let row = mode.cursor.y;
    let selection = mode.selection.expect("selection");
    assert_eq!(selection.anchor, PointCoordinate { x: 2, y: row });
    assert_eq!(selection.focus, PointCoordinate { x: 4, y: row });
    assert_eq!(selection.mode, SelectionMode::Cell);

    let swapped = run_copy_actions(
        b"alpha beta gamma",
        20,
        2,
        PointCoordinate { x: 2, y: 0 },
        &[
            CopyModeAction::SelectWord,
            CopyModeAction::OtherEnd,
            CopyModeAction::Right,
        ],
    );
    let row = swapped.cursor.y;
    let selection = swapped.selection.expect("selection");
    assert_eq!(selection.anchor, PointCoordinate { x: 5, y: row });
    assert_eq!(selection.focus, PointCoordinate { x: 1, y: row });
}

#[test]
fn select_word_and_select_line_arm_their_own_selection_units() {
    let word = run_copy_actions(
        b"alpha beta gamma",
        20,
        2,
        PointCoordinate { x: 7, y: 0 },
        &[CopyModeAction::SelectWord],
    );
    let row = word.cursor.y;
    let selection = word.selection.expect("selection");
    assert_eq!(word.cursor, PointCoordinate { x: 10, y: row });
    assert_eq!(selection.anchor, PointCoordinate { x: 6, y: row });
    assert_eq!(selection.focus, PointCoordinate { x: 10, y: row });

    let line = run_copy_actions(
        b"one\r\ntwo\r\nthree",
        10,
        3,
        PointCoordinate { x: 1, y: 0 },
        &[CopyModeAction::SelectLine, CopyModeAction::Down],
    );
    let selection = line.selection.expect("selection");
    assert_eq!(selection.mode, SelectionMode::Line);
    assert_eq!(
        selection.focus,
        PointCoordinate {
            x: line.revision.columns - 1,
            y: line.cursor.y,
        }
    );
}

#[test]
fn stop_selection_freezes_the_range_while_clear_selection_drops_it() {
    let stopped = run_copy_actions(
        b"alpha beta gamma",
        20,
        2,
        PointCoordinate { x: 2, y: 0 },
        &[
            CopyModeAction::StartSelection,
            CopyModeAction::Right,
            CopyModeAction::StopSelection,
            CopyModeAction::Right,
            CopyModeAction::Right,
        ],
    );
    let row = stopped.cursor.y;
    let selection = stopped
        .selection
        .expect("selection outlives stop-selection");
    assert!(!stopped.selecting);
    assert_eq!(stopped.cursor.x, 5);
    assert_eq!(selection.anchor, PointCoordinate { x: 2, y: row });
    assert_eq!(selection.focus, PointCoordinate { x: 3, y: row });

    let cleared = run_copy_actions(
        b"alpha beta gamma",
        20,
        2,
        PointCoordinate { x: 2, y: 0 },
        &[
            CopyModeAction::StartSelection,
            CopyModeAction::Right,
            CopyModeAction::ClearSelection,
        ],
    );
    assert!(cleared.selection.is_none());
    assert!(!cleared.selecting);
}

#[test]
fn rectangle_toggles_keep_the_live_selection_and_survive_entry_defaults() {
    let (_, copy_mode) = selection_lifecycle_fixture(b"alpha beta", 20, 2);
    let mode = copy_mode.expect("mode");
    assert!(!mode.rectangle);
    assert_eq!(mode.selection_mode, CopySelectionMode::Char);

    let toggled = run_copy_actions(
        b"alpha beta gamma",
        20,
        2,
        PointCoordinate { x: 2, y: 0 },
        &[
            CopyModeAction::StartSelection,
            CopyModeAction::Right,
            CopyModeAction::ToggleRectangle,
        ],
    );
    assert!(toggled.rectangle);
    let selection = toggled.selection.expect("selection");
    assert!(selection.rectangle);
    assert!(toggled.selecting);

    let off = run_copy_actions(
        b"alpha beta gamma",
        20,
        2,
        PointCoordinate { x: 2, y: 0 },
        &[
            CopyModeAction::StartSelection,
            CopyModeAction::RectangleOn,
            CopyModeAction::RectangleOff,
        ],
    );
    assert!(!off.rectangle);
    assert!(!off.selection.expect("selection").rectangle);
}

fn copy_mode_after_counted_action(
    action: CopyModeAction,
    count: u32,
    starts_at_bottom: bool,
) -> Option<Box<CopyModeState>> {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"zero\r\none\r\ntwo\r\nthree\r\nfour\r\nfive");
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    {
        let mode = copy_mode.as_mut().expect("mode");
        let maximum = mode.revision.maximum_offset();
        assert!(maximum > 1);
        if !starts_at_bottom {
            mode.viewport_offset = maximum - 1;
            let page = u32::from(mode.revision.viewport_rows.max(1));
            mode.cursor.y = mode
                .viewport_offset
                .saturating_add(page.saturating_sub(1))
                .min(mode.revision.total_rows().saturating_sub(1));
        }
    }
    let mut unseen_output = 0;
    apply_counted_copy_mode_action(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        &mut unseen_output,
        action,
        count,
        &WordSeparators::default(),
        false,
    )
    .expect("counted copy action");
    copy_mode
}

#[test]
fn scroll_exit_toggles_rearm_the_latch_taken_at_copy_mode_entry() {
    for (action, expected) in [
        (CopyModeAction::ScrollExitOn, true),
        (CopyModeAction::ScrollExitOff, false),
        (CopyModeAction::ScrollExitToggle, true),
    ] {
        let mode = run_copy_actions(
            b"alpha beta",
            20,
            2,
            PointCoordinate { x: 0, y: 0 },
            std::slice::from_ref(&action),
        );
        assert_eq!(mode.scroll_exit, expected, "{action:?}");
    }
    let toggled_twice = run_copy_actions(
        b"alpha beta",
        20,
        2,
        PointCoordinate { x: 0, y: 0 },
        &[
            CopyModeAction::ScrollExitToggle,
            CopyModeAction::ScrollExitToggle,
        ],
    );
    assert!(!toggled_twice.scroll_exit);
}

#[test]
fn a_rearmed_scroll_exit_latch_drives_the_plain_downward_actions() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"zero\r\none\r\ntwo\r\nthree");
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let mut unseen_output = 0;
    for action in [CopyModeAction::ScrollExitOn, CopyModeAction::PageDown] {
        apply_copy_mode_action(
            &mut terminal,
            &mut selection,
            &mut copy_mode,
            &mut unseen_output,
            action,
            &WordSeparators::default(),
            false,
        )
        .expect("copy action");
    }
    assert!(copy_mode.is_none());
}

#[test]
fn the_and_cancel_actions_exit_without_the_scroll_exit_latch() {
    for action in [
        CopyModeAction::PageDownScrollExit,
        CopyModeAction::HalfPageDownScrollExit,
        CopyModeAction::ScrollDownAndCancel,
        CopyModeAction::CursorDownAndCancel,
    ] {
        assert!(
            copy_mode_after_counted_action(action.clone(), 1, true).is_none(),
            "{action:?} at bottom"
        );
    }
}

#[test]
fn scroll_down_and_cancel_ignores_a_live_selection_but_page_down_and_cancel_does_not() {
    assert!(
        copy_mode_survives_downward_action(CopyModeAction::PageDownScrollExit, false, true, true),
        "page-down-and-cancel keeps a selected mode"
    );
    assert!(
        !copy_mode_survives_downward_action(CopyModeAction::ScrollDownAndCancel, false, true, true),
        "scroll-down-and-cancel exits with a selection"
    );
}

#[test]
fn counted_cursor_down_and_cancel_only_exits_when_the_whole_run_was_stuck() {
    assert!(
        copy_mode_after_counted_action(CopyModeAction::CursorDownAndCancel, 3, false).is_some(),
        "a run that moves keeps copy mode"
    );
    assert!(
        copy_mode_after_counted_action(CopyModeAction::CursorDownAndCancel, 3, true).is_none(),
        "a run stuck at the bottom exits"
    );
}

fn copy_mode_over_thirty_lines() -> CopyModeSlot {
    let mut terminal = new_terminal(10, 6, 64).expect("terminal");
    let lines = (0..30)
        .map(|index| format!("l{index:02}"))
        .collect::<Vec<_>>()
        .join("\r\n");
    terminal.vt_write(lines.as_bytes());
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    copy_mode
}

#[test]
fn page_movement_moves_the_view_and_keeps_the_cursor_row() {
    let mut copy_mode = copy_mode_over_thirty_lines();
    let mode = copy_mode.as_mut().expect("mode");
    let bottom = mode.viewport_offset;
    assert_eq!(bottom, mode.revision.maximum_offset());
    mode.cursor = PointCoordinate {
        x: 0,
        y: bottom + 4,
    };
    let separators = WordSeparators::default();
    move_copy_cursor(mode, &CopyModeAction::HalfPageUp, &separators, true);
    assert_eq!(mode.viewport_offset, bottom - 3);
    assert_eq!(
        mode.cursor,
        PointCoordinate {
            x: 0,
            y: bottom - 3 + 4
        }
    );
    move_copy_cursor(mode, &CopyModeAction::PageUp, &separators, true);
    assert_eq!(mode.viewport_offset, bottom - 7);
    assert_eq!(
        mode.cursor,
        PointCoordinate {
            x: 0,
            y: bottom - 7 + 4
        }
    );
    move_copy_cursor(mode, &CopyModeAction::PageDown, &separators, true);
    assert_eq!(mode.viewport_offset, bottom - 3);
    move_copy_cursor(mode, &CopyModeAction::HalfPageDown, &separators, true);
    assert_eq!(mode.viewport_offset, bottom);
    assert_eq!(
        mode.cursor,
        PointCoordinate {
            x: 0,
            y: bottom + 4
        }
    );
}

#[test]
fn page_movement_pinned_at_either_end_moves_the_cursor_instead() {
    let mut copy_mode = copy_mode_over_thirty_lines();
    let mode = copy_mode.as_mut().expect("mode");
    let bottom = mode.viewport_offset;
    let separators = WordSeparators::default();
    mode.viewport_offset = 1;
    mode.cursor = PointCoordinate { x: 0, y: 5 };
    move_copy_cursor(mode, &CopyModeAction::PageUp, &separators, true);
    assert_eq!(mode.viewport_offset, 0);
    assert_eq!(mode.cursor, PointCoordinate { x: 0, y: 0 });
    mode.viewport_offset = bottom - 2;
    mode.cursor = PointCoordinate {
        x: 0,
        y: bottom - 2 + 1,
    };
    move_copy_cursor(mode, &CopyModeAction::PageDown, &separators, true);
    assert_eq!(mode.viewport_offset, bottom);
    assert_eq!(
        mode.cursor,
        PointCoordinate {
            x: 0,
            y: bottom + 5
        }
    );
}

#[test]
fn page_movement_pinned_at_the_bottom_keeps_the_remembered_column_unclamped() {
    let separators = WordSeparators::default();
    for (vi, start, landed) in [(true, 1, 1), (false, 1, 1), (true, 2, 0), (false, 2, 1)] {
        let mut terminal = new_terminal(10, 6, 64).expect("terminal");
        let lines = (0..30)
            .map(|index| {
                if index == 28 {
                    "x".to_owned()
                } else {
                    format!("l{index:02}")
                }
            })
            .collect::<Vec<_>>()
            .join("\r\n");
        terminal.vt_write(lines.as_bytes());
        let mut selection = None;
        let mut copy_mode = None;
        enter_copy_mode(
            &mut terminal,
            &mut selection,
            &mut copy_mode,
            false,
            false,
            None,
            vi,
        )
        .expect("copy mode");
        let mode = copy_mode.as_mut().expect("mode");
        let bottom = mode.viewport_offset;
        mode.cursor = PointCoordinate {
            x: start,
            y: bottom + 1,
        };
        move_copy_cursor(mode, &CopyModeAction::HalfPageDown, &separators, vi);
        assert_eq!(mode.viewport_offset, bottom);
        assert_eq!(
            mode.cursor,
            PointCoordinate {
                x: landed,
                y: bottom + 4
            },
            "vi={vi} from column {start}"
        );
    }
}

#[test]
fn search_landing_scrolls_the_view_a_quarter_screen_above_the_bottom() {
    let mut copy_mode = copy_mode_over_thirty_lines();
    let mode = copy_mode.as_mut().expect("mode");
    let bottom = mode.viewport_offset;
    mode.cursor = PointCoordinate {
        x: 0,
        y: bottom + 2,
    };
    scroll_copy_view_to_cursor(mode);
    assert_eq!(mode.viewport_offset, bottom);
    mode.cursor = PointCoordinate { x: 0, y: 2 };
    scroll_copy_view_to_cursor(mode);
    assert_eq!(mode.viewport_offset, 0);
    mode.cursor = PointCoordinate { x: 0, y: 12 };
    scroll_copy_view_to_cursor(mode);
    assert_eq!(mode.viewport_offset, 7);
    mode.viewport_offset = 0;
    mode.cursor = PointCoordinate {
        x: 0,
        y: bottom + 5,
    };
    scroll_copy_view_to_cursor(mode);
    assert_eq!(mode.viewport_offset, bottom);
}

#[cfg(unix)]
#[test]
fn the_pane_keeps_its_last_search_and_a_fresh_entry_searches_up_for_it() {
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            command: Some(vec![
                "printf 'alpha\\r\\nmatch-1\\r\\nbeta\\r\\nmatch-2\\r\\nomega\\r\\nZZ_SEARCH_READY'; sleep 30"
                    .to_owned(),
            ]),
            ..TerminalSpawn::default()
        },
    );
    let view = TerminalViewId(3);
    attach_streaming(&session, view);
    wait_for_test_viewport(&session, |viewport| {
        let mut contents = String::new();
        for cell in viewport.cells.iter() {
            viewport.push_glyph(*cell, &mut contents);
        }
        contents.contains("ZZ_SEARCH_READY")
    });
    let wait_for_facts = |expected: &str, predicate: &dyn Fn(Option<&CopyModeFacts>) -> bool| {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let facts = session.copy_mode_facts(view);
            if predicate(facts.as_deref()) {
                return facts;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for {expected}; last facts: {facts:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    };
    let on_line = |line: &'static str| {
        move |facts: Option<&CopyModeFacts>| facts.is_some_and(|facts| facts.cursor_line == line)
    };
    assert_eq!(session.pane_search_string(), "");

    session.view_action(view, TerminalViewAction::EnterCopyMode);
    wait_for_facts("copy mode", &on_line("ZZ_SEARCH_READY"));
    session.view_action(
        view,
        TerminalViewAction::CopyMode(CopyModeAction::Search(Box::new(CopyModeSearch {
            text: "match".to_owned(),
            direction: SearchDirection::Forward,
            regex: true,
            incremental: false,
        }))),
    );
    wait_for_facts("the forward search to wrap", &on_line("match-1"));
    assert_eq!(session.pane_search_string(), "match");

    session.view_action(view, TerminalViewAction::CopyMode(CopyModeAction::Cancel));
    wait_for_facts("the mode to close", &|facts| facts.is_none());
    assert_eq!(session.pane_search_string(), "match");

    session.view_action(view, TerminalViewAction::EnterCopyMode);
    let entered =
        wait_for_facts("a fresh entry", &on_line("ZZ_SEARCH_READY")).expect("fresh entry facts");
    assert!(!entered.search_present);
    assert_eq!(session.pane_search_string(), "match");
    session.view_action(
        view,
        TerminalViewAction::CopyMode(CopyModeAction::SearchAgain { reverse: false }),
    );
    let again = wait_for_facts("search-again to search up", &on_line("match-2"))
        .expect("search-again facts");
    assert!(again.search_present);
    assert_eq!(again.search_count, Some((2, false)));

    session.view_action(
        view,
        TerminalViewAction::SearchBegin(SearchQuery::default()),
    );
    session.view_action(
        view,
        TerminalViewAction::SearchUpdate(SearchQuery {
            text: "omega".to_owned(),
            ..SearchQuery::default()
        }),
    );
    wait_for_facts(
        "the live query to replace the old search",
        &on_line("omega"),
    );
    assert_eq!(session.pane_search_string(), "omega");
    session.view_action(view, TerminalViewAction::CopyMode(CopyModeAction::Cancel));
    wait_for_facts("live search mode to close", &|facts| facts.is_none());
    session.view_action(view, TerminalViewAction::EnterCopyMode);
    wait_for_facts("another fresh entry", &on_line("ZZ_SEARCH_READY"));
    session.view_action(
        view,
        TerminalViewAction::CopyMode(CopyModeAction::SearchAgain { reverse: false }),
    );
    wait_for_facts("repeat to use the live query", &on_line("omega"));
}

#[test]
fn cursor_centre_actions_park_the_cursor_mid_view_and_mid_row() {
    let mut terminal = new_terminal(9, 4, 16).expect("terminal");
    terminal.vt_write(b"zero\r\none\r\ntwo\r\nthree\r\nfour\r\nfive");
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let mode = copy_mode.as_mut().expect("mode");
    let viewport = mode.viewport_offset;
    mode.cursor = PointCoordinate { x: 1, y: viewport };
    move_copy_cursor(
        mode,
        &CopyModeAction::CursorCentreVertical,
        &WordSeparators::default(),
        false,
    );
    assert_eq!(
        mode.cursor,
        PointCoordinate {
            x: 1,
            y: viewport + 2,
        }
    );
    assert_eq!(mode.viewport_offset, viewport);
    move_copy_cursor(
        mode,
        &CopyModeAction::CursorCentreHorizontal,
        &WordSeparators::default(),
        false,
    );
    assert_eq!(
        mode.cursor,
        PointCoordinate {
            x: 4,
            y: viewport + 2,
        }
    );
    mode.cursor = PointCoordinate { x: 7, y: viewport };
    move_copy_cursor(
        mode,
        &CopyModeAction::CursorCentreVertical,
        &WordSeparators::default(),
        false,
    );
    assert_eq!(
        mode.cursor,
        PointCoordinate {
            x: 4,
            y: viewport + 2,
        }
    );
}

#[test]
fn scroll_placement_moves_the_view_and_leaves_the_cursor_line_alone() {
    let mut terminal = new_terminal(8, 3, 32).expect("terminal");
    terminal.vt_write(b"a\r\nb\r\nc\r\nd\r\ne\r\nf\r\ng\r\nh\r\ni");
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let mut unseen_output = 0;
    let anchor = {
        let mode = copy_mode.as_mut().expect("mode");
        let maximum = mode.revision.maximum_offset();
        assert!(maximum >= 4);
        mode.viewport_offset = maximum - 2;
        mode.cursor = PointCoordinate {
            x: 0,
            y: maximum - 1,
        };
        mode.cursor.y
    };
    for (action, screen_row) in [
        (CopyModeAction::ScrollTop, 0),
        (CopyModeAction::ScrollMiddle, 1),
        (CopyModeAction::ScrollBottom, 2),
    ] {
        apply_copy_mode_action(
            &mut terminal,
            &mut selection,
            &mut copy_mode,
            &mut unseen_output,
            action.clone(),
            &WordSeparators::default(),
            false,
        )
        .expect("copy action");
        let mode = copy_mode.as_ref().expect("mode");
        assert_eq!(mode.cursor.y, anchor, "{action:?} moved the cursor line");
        assert_eq!(
            mode.viewport_offset,
            anchor - screen_row,
            "{action:?} placed the wrong view"
        );
    }
}

#[test]
fn goto_line_scrolls_back_from_the_bottom_and_holds_the_cursor_screen_row() {
    let mut terminal = new_terminal(8, 3, 64).expect("terminal");
    for line in 0..20_u32 {
        terminal.vt_write(format!("L{line}\r\n").as_bytes());
    }
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let mut unseen_output = 0;
    let (maximum, screen_row) = {
        let mode = copy_mode.as_mut().expect("mode");
        let maximum = mode.revision.maximum_offset();
        assert!(maximum >= 8);
        mode.viewport_offset = maximum;
        mode.cursor = PointCoordinate {
            x: 0,
            y: maximum + 1,
        };
        (maximum, 1)
    };
    let goto = |copy_mode: &mut CopyModeSlot,
                terminal: &mut Terminal<'_, '_>,
                selection: &mut Option<SelectionState>,
                unseen_output: &mut u32,
                line: Option<i32>| {
        apply_copy_mode_action(
            terminal,
            selection,
            copy_mode,
            unseen_output,
            CopyModeAction::GotoLine {
                line,
                option_absolute: false,
            },
            &WordSeparators::default(),
            false,
        )
        .expect("goto-line");
    };
    {
        let mode = copy_mode.as_ref().expect("mode");
        assert_eq!(mode.viewport_offset, maximum);
    }
    goto(
        &mut copy_mode,
        &mut terminal,
        &mut selection,
        &mut unseen_output,
        None,
    );
    {
        let mode = copy_mode.as_ref().expect("mode");
        assert_eq!(
            mode.viewport_offset, maximum,
            "an argument the pin's strtonum rejects must move nothing"
        );
    }
    for line in [0_u32, 1, 5, maximum, maximum + 1, i32::MAX.unsigned_abs()] {
        goto(
            &mut copy_mode,
            &mut terminal,
            &mut selection,
            &mut unseen_output,
            i32::try_from(line).ok(),
        );
        let mode = copy_mode.as_ref().expect("mode");
        assert_eq!(
            mode.viewport_offset,
            maximum - line.min(maximum),
            "goto-line {line} placed the wrong view"
        );
        assert_eq!(
            mode.cursor.y - mode.viewport_offset,
            screen_row,
            "goto-line {line} moved the cursor screen row"
        );
    }
}

#[test]
fn goto_line_counts_from_the_top_under_absolute_line_numbers() {
    let mut terminal = new_terminal(8, 3, 64).expect("terminal");
    for line in 0..20_u32 {
        terminal.vt_write(format!("L{line}\r\n").as_bytes());
    }
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let maximum = copy_mode.as_ref().expect("mode").revision.maximum_offset();
    let mut unseen_output = 0;
    let mut run = |copy_mode: &mut CopyModeSlot, action: CopyModeAction| {
        apply_copy_mode_action(
            &mut terminal,
            &mut selection,
            copy_mode,
            &mut unseen_output,
            action,
            &WordSeparators::default(),
            false,
        )
        .expect("copy action");
        copy_mode.as_ref().expect("mode").viewport_offset
    };
    let goto = |line: i32| CopyModeAction::GotoLine {
        line: Some(line),
        option_absolute: true,
    };
    assert_eq!(run(&mut copy_mode, goto(1)), 0);
    assert_eq!(run(&mut copy_mode, goto(5)), 4);
    assert_eq!(run(&mut copy_mode, goto(-1)), 0);
    assert_eq!(run(&mut copy_mode, goto(i32::MAX)), maximum);
    run(&mut copy_mode, CopyModeAction::LineNumbersOff);
    assert_eq!(run(&mut copy_mode, goto(5)), maximum - 5);
}

#[test]
fn refresh_now_clears_the_selection_and_follows_to_the_line_end() {
    let mut terminal = new_terminal(8, 3, 16).expect("terminal");
    terminal.vt_write(b"a\r\nb\r\n");
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let mut unseen_output = 0;
    for action in [CopyModeAction::StartSelection, CopyModeAction::Up] {
        apply_copy_mode_action(
            &mut terminal,
            &mut selection,
            &mut copy_mode,
            &mut unseen_output,
            action,
            &WordSeparators::default(),
            false,
        )
        .expect("copy action");
    }
    assert!(copy_mode.as_ref().expect("mode").selection.is_some());
    apply_copy_mode_action(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::RefreshNow,
        &WordSeparators::default(),
        false,
    )
    .expect("refresh-now");
    let mode = copy_mode.as_ref().expect("mode");
    assert!(mode.selection.is_none() && !mode.selecting);

    let mut terminal = new_terminal(8, 3, 16).expect("terminal");
    terminal.vt_write(b"a\r\nb\r\n");
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    terminal.vt_write(b"three");
    for (vi, column) in [(false, 5), (true, 4)] {
        apply_copy_mode_action(
            &mut terminal,
            &mut selection,
            &mut copy_mode,
            &mut unseen_output,
            CopyModeAction::RefreshNow,
            &WordSeparators::default(),
            vi,
        )
        .expect("refresh-now");
        let mode = copy_mode.as_ref().expect("mode");
        assert_eq!(
            (mode.cursor.x, mode.cursor.y - mode.viewport_offset),
            (column, 2)
        );
    }
}

#[test]
fn refresh_now_searches_the_refreshed_text_again() {
    let mut terminal = new_terminal(8, 3, 16).expect("terminal");
    terminal.vt_write(b"foo\r\n");
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let view_id = TerminalViewId(4);
    let (mut worker, _results) = SearchWorker::spawn(ActorWake::none());
    let mut search = None;
    let mut search_origin = None;
    let mut search_snapshot = None;
    let mut pane_search = None;
    let mut unseen_output = 0;
    let find = |text: &str| CopyModeSearch {
        text: text.to_owned(),
        direction: SearchDirection::Backward,
        regex: false,
        incremental: false,
    };
    let matches = |search: &SearchSlot| {
        search.as_ref().map(|search| {
            search
                .matches
                .iter()
                .map(|found| (found.row, found.start))
                .collect::<Vec<_>>()
        })
    };

    run_copy_mode_search(
        view_id,
        &mut copy_mode,
        &mut search,
        &mut search_snapshot,
        &mut worker,
        &find("foo"),
        1,
        false,
        true,
        &mut pane_search,
    );
    assert_eq!(matches(&search), Some(vec![(0, 0)]));

    terminal.vt_write(b"\x1b[1;1Hbar");
    let before = copy_mode.as_ref().map(|mode| mode.revision.id);
    apply_copy_mode_action(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::RefreshNow,
        &WordSeparators::default(),
        false,
    )
    .expect("refresh-now");
    settle_copy_search(
        view_id,
        &mut worker,
        &mut copy_mode,
        &mut search,
        &mut search_origin,
        &mut search_snapshot,
        before,
    );
    assert_eq!(matches(&search), Some(Vec::new()));
    assert_eq!(
        copy_mode.as_ref().expect("mode").search_count,
        Some((0, false))
    );

    run_copy_mode_search(
        view_id,
        &mut copy_mode,
        &mut search,
        &mut search_snapshot,
        &mut worker,
        &find("bar"),
        1,
        false,
        true,
        &mut pane_search,
    );
    assert_eq!(matches(&search), Some(vec![(0, 0)]));
}

#[test]
fn scroll_placement_does_nothing_when_the_revision_cannot_reach_that_far() {
    let mut terminal = new_terminal(8, 3, 32).expect("terminal");
    terminal.vt_write(b"a\r\nb\r\nc\r\nd\r\ne\r\nf");
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let mut unseen_output = 0;
    {
        let mode = copy_mode.as_mut().expect("mode");
        mode.viewport_offset = 0;
        mode.cursor = PointCoordinate { x: 0, y: 0 };
    }
    for action in [CopyModeAction::ScrollMiddle, CopyModeAction::ScrollBottom] {
        apply_copy_mode_action(
            &mut terminal,
            &mut selection,
            &mut copy_mode,
            &mut unseen_output,
            action.clone(),
            &WordSeparators::default(),
            false,
        )
        .expect("copy action");
        let mode = copy_mode.as_ref().expect("mode");
        assert_eq!(mode.viewport_offset, 0, "{action:?}");
        assert_eq!(mode.cursor.y, 0, "{action:?}");
    }
}

#[test]
fn toggle_position_flips_the_published_position_readout() {
    let hidden = run_copy_actions(
        b"alpha beta",
        20,
        2,
        PointCoordinate { x: 0, y: 0 },
        &[CopyModeAction::TogglePosition],
    );
    assert!(hidden.hide_position);
    let shown = run_copy_actions(
        b"alpha beta",
        20,
        2,
        PointCoordinate { x: 0, y: 0 },
        &[
            CopyModeAction::TogglePosition,
            CopyModeAction::TogglePosition,
        ],
    );
    assert!(!shown.hide_position);
}

#[test]
fn recentre_cycles_middle_top_bottom_and_restarts_on_a_new_line() {
    let mut terminal = new_terminal(8, 3, 32).expect("terminal");
    terminal.vt_write(b"a\r\nb\r\nc\r\nd\r\ne\r\nf\r\ng\r\nh\r\ni");
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let mut unseen_output = 0;
    let anchor = {
        let mode = copy_mode.as_mut().expect("mode");
        let maximum = mode.revision.maximum_offset();
        assert!(maximum >= 4);
        mode.viewport_offset = maximum - 2;
        mode.cursor = PointCoordinate {
            x: 0,
            y: maximum - 1,
        };
        mode.cursor.y
    };
    let mut recentre = |copy_mode: &mut CopyModeSlot| {
        apply_copy_mode_action(
            &mut terminal,
            &mut selection,
            copy_mode,
            &mut unseen_output,
            CopyModeAction::RecentreTopBottom,
            &WordSeparators::default(),
            false,
        )
        .expect("recentre");
        let mode = copy_mode.as_ref().expect("mode");
        (mode.cursor.y, mode.viewport_offset)
    };
    assert_eq!(recentre(&mut copy_mode), (anchor, anchor - 1));
    assert_eq!(recentre(&mut copy_mode), (anchor, anchor));
    assert_eq!(recentre(&mut copy_mode), (anchor, anchor - 2));
    assert_eq!(recentre(&mut copy_mode), (anchor, anchor - 1));

    copy_mode.as_mut().expect("mode").cursor.y = anchor - 1;
    assert_eq!(recentre(&mut copy_mode), (anchor - 1, anchor - 2));
}

#[test]
fn recentre_clamps_instead_of_refusing_when_the_view_cannot_reach() {
    let mode = run_copy_actions(
        b"a\r\nb\r\nc",
        8,
        3,
        PointCoordinate { x: 0, y: 0 },
        &[CopyModeAction::RecentreTopBottom],
    );
    assert_eq!(mode.viewport_offset, 0);
    assert_eq!(mode.cursor.y, mode.revision.maximum_offset());
}

#[test]
fn previous_matching_bracket_walks_back_from_the_nearest_bracket() {
    let closer = run_copy_actions(
        b"(alpha [beta] gamma)",
        24,
        2,
        PointCoordinate { x: 12, y: 0 },
        &[CopyModeAction::PreviousMatchingBracket],
    );
    assert_eq!(closer.cursor.x, 7);

    let inside = run_copy_actions(
        b"(alpha [beta] gamma)",
        24,
        2,
        PointCoordinate { x: 10, y: 0 },
        &[CopyModeAction::PreviousMatchingBracket],
    );
    assert_eq!(inside.cursor.x, 10);

    let nested = run_copy_actions(
        b"(alpha [beta] gamma)",
        24,
        2,
        PointCoordinate { x: 19, y: 0 },
        &[CopyModeAction::PreviousMatchingBracket],
    );
    assert_eq!(nested.cursor.x, 0);
}

#[test]
fn previous_matching_bracket_stops_at_the_start_of_the_logical_line() {
    let stuck = run_copy_actions(
        b"alpha beta",
        24,
        2,
        PointCoordinate { x: 3, y: 0 },
        &[CopyModeAction::PreviousMatchingBracket],
    );
    assert_eq!(stuck.cursor.x, 3);

    let unmatched = run_copy_actions(
        b"alpha) beta",
        24,
        2,
        PointCoordinate { x: 8, y: 0 },
        &[CopyModeAction::PreviousMatchingBracket],
    );
    assert_eq!(unmatched.cursor.x, 8);
}

#[test]
fn the_matching_bracket_pair_scans_in_opposite_directions() {
    let forward = run_copy_actions(
        b"one (two) three",
        24,
        2,
        PointCoordinate { x: 1, y: 0 },
        &[CopyModeAction::NextMatchingBracket],
    );
    assert_eq!(forward.cursor.x, 8);

    let backward = run_copy_actions(
        b"one (two) three",
        24,
        2,
        PointCoordinate { x: 12, y: 0 },
        &[CopyModeAction::PreviousMatchingBracket],
    );
    assert_eq!(backward.cursor.x, 4);
}
