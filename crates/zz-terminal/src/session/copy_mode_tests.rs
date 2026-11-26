use super::test_support::*;
use super::*;

#[test]
fn copy_mode_word_and_line_motions_use_terminal_cells() {
    let mut terminal = new_terminal(16, 2, 16).expect("terminal");
    terminal.vt_write(b"alpha beta!");
    let columns = terminal.cols().expect("columns");
    let rows = u32::try_from(terminal.total_rows().expect("rows")).expect("small fixture");

    let word = move_copy_word(
        &terminal,
        PointCoordinate { x: 0, y: 0 },
        columns,
        rows,
        &CopyModeAction::NextWord,
    )
    .expect("next word");
    assert_eq!(word.x, 6);
    let end = move_copy_word(&terminal, word, columns, rows, &CopyModeAction::NextWordEnd)
        .expect("word end");
    assert_eq!(end.x, 9);
    let previous = move_copy_word(
        &terminal,
        PointCoordinate { x: 10, y: 0 },
        columns,
        rows,
        &CopyModeAction::PreviousWord,
    )
    .expect("previous word");
    assert_eq!(previous.x, 6);
    assert_eq!(copy_line_end(&terminal, 0, columns).expect("line end"), 10);
}

#[test]
fn native_copy_mode_word_motion_honors_separator_runs_and_empty_values() {
    let mut terminal = new_terminal(16, 2, 16).expect("terminal");
    terminal.vt_write(b"foo..bar baz");
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
    let row = (0..mode.revision.total_rows())
        .find(|row| mode.revision.first_char(PointCoordinate { x: 0, y: *row }) == Some('f'))
        .expect("fixture row");

    mode.cursor = PointCoordinate { x: 0, y: row };
    move_copy_cursor(
        mode,
        &CopyModeAction::NextWord,
        &WordSeparators::default(),
        false,
    );
    assert_eq!(mode.cursor.x, 3, "separator run is its own word");
    move_copy_cursor(
        mode,
        &CopyModeAction::NextWord,
        &WordSeparators::default(),
        false,
    );
    assert_eq!(mode.cursor.x, 5, "next word follows the separator run");

    mode.cursor = PointCoordinate { x: 0, y: row };
    move_copy_cursor(
        mode,
        &CopyModeAction::NextWord,
        &WordSeparators::new(""),
        false,
    );
    assert_eq!(mode.cursor.x, 9, "empty separators stop only at whitespace");

    mode.cursor = PointCoordinate { x: 0, y: row };
    move_copy_cursor(
        mode,
        &CopyModeAction::NextSpaceEnd,
        &WordSeparators::default(),
        false,
    );
    assert_eq!(mode.cursor.x, 8, "E treats punctuation as part of the word");
    mode.cursor = PointCoordinate { x: 0, y: row };
    move_copy_cursor(
        mode,
        &CopyModeAction::NextSpaceEnd,
        &WordSeparators::default(),
        true,
    );
    assert_eq!(mode.cursor.x, 7, "vi stops on the word's last cell");
    move_copy_cursor(
        mode,
        &CopyModeAction::NextSpace,
        &WordSeparators::default(),
        false,
    );
    assert_eq!(
        mode.cursor.x, 9,
        "W advances to the next whitespace-delimited word"
    );
    move_copy_cursor(
        mode,
        &CopyModeAction::PreviousSpace,
        &WordSeparators::default(),
        false,
    );
    assert_eq!(
        mode.cursor.x, 0,
        "B returns across punctuation to the word start"
    );
}

#[test]
fn native_copy_mode_percent_matches_nested_brackets() {
    let mut terminal = new_terminal(24, 2, 16).expect("terminal");
    terminal.vt_write(b"foo..bar baz (a[b]c)");
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
    let row = (0..mode.revision.total_rows())
        .find(|row| mode.revision.first_char(PointCoordinate { x: 0, y: *row }) == Some('f'))
        .expect("fixture row");

    mode.cursor = PointCoordinate { x: 0, y: row };
    move_copy_cursor(
        mode,
        &CopyModeAction::NextMatchingBracket,
        &WordSeparators::default(),
        false,
    );
    assert_eq!(mode.cursor, PointCoordinate { x: 19, y: row });
    move_copy_cursor(
        mode,
        &CopyModeAction::NextMatchingBracket,
        &WordSeparators::default(),
        false,
    );
    assert_eq!(mode.cursor, PointCoordinate { x: 13, y: row });
}

#[test]
fn counted_copy_mode_actions_apply_pinned_parity_brackets_and_line_span() {
    let mut terminal = new_terminal(24, 4, 16).expect("terminal");
    terminal.vt_write(b"(a[b]c)\r\ntwo\r\nthree\r\nfour");
    let mut live_selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let first = (0..copy_mode.as_ref().expect("mode").revision.total_rows())
        .find(|row| {
            copy_mode
                .as_ref()
                .expect("mode")
                .revision
                .first_char(PointCoordinate { x: 0, y: *row })
                == Some('(')
        })
        .expect("first fixture row");
    let mut unseen_output = 0;
    let separators = WordSeparators::default();

    {
        let mode = copy_mode.as_mut().expect("mode");
        mode.cursor = PointCoordinate { x: 2, y: first };
        mode.selection = Some(ModeSelection {
            anchor: PointCoordinate { x: 0, y: first },
            focus: PointCoordinate { x: 2, y: first },
            mode: SelectionMode::Cell,
            rectangle: false,
        });
        mode.selecting = true;
    }
    apply_counted_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::OtherEnd,
        2,
        &separators,
        false,
    )
    .expect("even other-end");
    let mode = copy_mode.as_ref().expect("mode");
    let selection = mode.selection.expect("selection");
    assert_eq!(selection.anchor.x, 0);
    assert_eq!(selection.focus.x, 2);
    assert_eq!(mode.cursor.x, 2);

    apply_counted_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::OtherEnd,
        3,
        &separators,
        false,
    )
    .expect("odd other-end");
    let mode = copy_mode.as_ref().expect("mode");
    let selection = mode.selection.expect("selection");
    assert_eq!(selection.anchor.x, 2);
    assert_eq!(selection.focus.x, 0);
    assert_eq!(mode.cursor.x, 0);

    copy_mode.as_mut().expect("mode").cursor = PointCoordinate { x: 0, y: first };
    apply_counted_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::NextMatchingBracket,
        2,
        &separators,
        false,
    )
    .expect("two bracket transitions");
    assert_eq!(
        copy_mode.as_ref().expect("mode").cursor,
        PointCoordinate { x: 0, y: first }
    );
    apply_counted_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::NextMatchingBracket,
        3,
        &separators,
        false,
    )
    .expect("three bracket transitions");
    assert_eq!(
        copy_mode.as_ref().expect("mode").cursor,
        PointCoordinate { x: 6, y: first }
    );

    copy_mode.as_mut().expect("mode").cursor = PointCoordinate { x: 0, y: first };
    apply_counted_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::SelectLine,
        3,
        &separators,
        false,
    )
    .expect("three-line selection");
    let mode = copy_mode.as_ref().expect("mode");
    let selection = mode.selection.expect("line selection");
    assert_eq!(selection.anchor.y, first);
    assert_eq!(selection.focus.y, first + 2);
    assert_eq!(selection.mode, SelectionMode::Line);
    assert_eq!(mode.cursor.y, first + 2);
}

#[test]
fn counted_copy_mode_once_policy_runs_stateful_actions_once() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"abcdef");
    let mut live_selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let mut unseen_output = 0;
    let separators = WordSeparators::default();

    apply_counted_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::ToggleRectangle,
        2,
        &separators,
        false,
    )
    .expect("toggle once");
    assert!(copy_mode.as_ref().expect("mode").rectangle);

    apply_counted_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::StartSelection,
        2,
        &separators,
        false,
    )
    .expect("selection once");
    assert!(copy_mode.as_ref().expect("mode").selection.is_some());

    let copied = apply_counted_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::copy_selection(crate::CopyModeCopy {
            request_id: 41,
            clipboard: false,
            buffer: None,
            pipe: None,
            clear_selection: true,
            cancel: true,
        }),
        2,
        &separators,
        false,
    )
    .expect("copy once");
    assert!(matches!(
        copied,
        ViewActionResult::Copy(copy) if copy.request_id == 41
    ));
    assert!(copy_mode.is_none());

    enter_copy_mode(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode again");
    apply_counted_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::Cancel,
        2,
        &separators,
        false,
    )
    .expect("cancel once");
    assert!(copy_mode.is_none());
}

#[test]
fn copy_mode_tracks_a_native_cursor() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"one\r\ntwo\r\nthree");
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
    move_copy_cursor(mode, &CopyModeAction::Up, &WordSeparators::default(), false);
    let point = mode.cursor;
    assert!(point.y < u32::try_from(terminal.total_rows().expect("rows")).unwrap());
}

#[test]
fn copy_mode_fixed_rows_reset_column() {
    let mut terminal = new_terminal(8, 3, 16).expect("terminal");
    terminal.vt_write(b"abcdefgh\r\nmiddle\r\nxy");
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
    let page = u32::from(mode.revision.viewport_rows);

    for (action, y) in [
        (CopyModeAction::TopLine, viewport),
        (
            CopyModeAction::MiddleLine,
            viewport.saturating_add(page.saturating_sub(1) / 2),
        ),
        (
            CopyModeAction::BottomLine,
            viewport.saturating_add(page.saturating_sub(1)),
        ),
    ] {
        mode.cursor = PointCoordinate { x: 6, y: viewport };
        move_copy_cursor(mode, &action, &WordSeparators::default(), false);
        assert_eq!(mode.cursor, PointCoordinate { x: 0, y }, "{action:?}");
        assert_eq!(mode.viewport_offset, viewport, "{action:?}");
    }
}

#[test]
fn scroll_exit_cancels_at_and_when_reaching_the_bottom() {
    for action in [
        CopyModeAction::ScrollDown,
        CopyModeAction::PageDown,
        CopyModeAction::HalfPageDown,
    ] {
        assert!(
            !copy_mode_survives_downward_action(action.clone(), true, false, true),
            "{action:?} at bottom"
        );
        assert!(
            !copy_mode_survives_downward_action(action.clone(), true, false, false),
            "{action:?} reaching bottom"
        );
    }
}

#[test]
fn scroll_exit_preserves_copy_mode_with_a_selection() {
    for action in [
        CopyModeAction::ScrollDown,
        CopyModeAction::PageDown,
        CopyModeAction::HalfPageDown,
    ] {
        assert!(
            copy_mode_survives_downward_action(action.clone(), true, true, true),
            "{action:?}"
        );
    }
}

#[test]
fn scroll_exit_does_not_apply_to_cursor_down() {
    assert!(copy_mode_survives_downward_action(
        CopyModeAction::Down,
        true,
        false,
        true,
    ));
}

#[test]
fn copy_mode_without_scroll_exit_never_cancels_on_downward_actions() {
    for action in [
        CopyModeAction::ScrollDown,
        CopyModeAction::PageDown,
        CopyModeAction::HalfPageDown,
    ] {
        assert!(
            copy_mode_survives_downward_action(action.clone(), false, false, true),
            "{action:?}"
        );
    }
}

#[test]
fn one_shot_page_down_scroll_exit_works_in_existing_plain_copy_mode() {
    assert!(!copy_mode_survives_downward_action(
        CopyModeAction::PageDownScrollExit,
        false,
        false,
        true,
    ));
    assert!(!copy_mode_survives_downward_action(
        CopyModeAction::PageDownScrollExit,
        false,
        false,
        false,
    ));
    assert!(copy_mode_survives_downward_action(
        CopyModeAction::PageDownScrollExit,
        false,
        true,
        true,
    ));
}

#[test]
fn scroll_exit_latches_only_on_fresh_copy_mode_entry() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
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
    .expect("plain entry");
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        true,
        false,
        None,
        true,
    )
    .expect("repeated scroll-exit entry");
    assert!(!copy_mode.as_ref().expect("mode").scroll_exit);

    copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        true,
        false,
        None,
        true,
    )
    .expect("scroll-exit entry");
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("repeated plain entry");
    assert!(copy_mode.as_ref().expect("mode").scroll_exit);
}

#[test]
fn hide_position_latches_on_entry_and_leaves_scroll_exit_alone() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    for (scroll_exit, hide_position) in [(false, false), (true, false), (false, true), (true, true)]
    {
        let mut selection = None;
        let mut copy_mode = None;
        enter_copy_mode(
            &mut terminal,
            &mut selection,
            &mut copy_mode,
            scroll_exit,
            hide_position,
            None,
            true,
        )
        .expect("copy mode");
        let mode = copy_mode.as_ref().expect("mode");
        assert_eq!(mode.scroll_exit, scroll_exit);
        assert_eq!(mode.hide_position, hide_position);
        assert_eq!(mode.kind, FrozenModeKind::Copy);
    }
}

#[test]
fn absolute_scroll_clamps_and_restores_follow_bottom() {
    let mut terminal = new_terminal(8, 3, 32).expect("terminal");
    terminal.vt_write(b"zero\r\none\r\ntwo\r\nthree\r\nfour\r\nfive");
    let maximum = terminal
        .scrollbar()
        .expect("scrollbar")
        .total
        .saturating_sub(terminal.scrollbar().expect("scrollbar").len);
    assert!(maximum > 1);

    let mut view = TerminalViewState::for_screen(Screen::Primary);
    {
        let state = view.active_mut();
        state.unseen_output = 4;
        scroll_to_offset(
            &mut terminal,
            &mut state.copy_mode,
            &mut state.unseen_output,
            1,
        )
        .expect("absolute scroll");
    }
    assert_eq!(terminal.scrollbar().expect("scrollbar").offset, 1);
    sync_viewport_anchor(&terminal, &mut view).expect("pin viewport");
    assert!(matches!(view.viewport, ViewportAnchor::Pinned(_)));

    {
        let state = view.active_mut();
        scroll_to_offset(
            &mut terminal,
            &mut state.copy_mode,
            &mut state.unseen_output,
            u32::MAX,
        )
        .expect("clamped tail scroll");
    }
    let scrollbar = terminal.scrollbar().expect("scrollbar");
    assert_eq!(scrollbar.offset, maximum);
    assert_eq!(view.unseen_output, 0);
    sync_viewport_anchor(&terminal, &mut view).expect("follow viewport");
    assert!(matches!(view.viewport, ViewportAnchor::FollowBottom));

    terminal.vt_write(b"\r\nsix");
    restore_view_state(&mut terminal, &mut view, &WordSeparators::default())
        .expect("restore followed viewport");
    let scrollbar = terminal.scrollbar().expect("scrollbar after output");
    assert!(scrollbar.offset.saturating_add(scrollbar.len) >= scrollbar.total);

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
    let copy_maximum = copy_mode
        .as_ref()
        .expect("copy mode")
        .revision
        .maximum_offset();
    let mut unseen_output = 0;
    scroll_to_offset(&mut terminal, &mut copy_mode, &mut unseen_output, u32::MAX)
        .expect("copy mode absolute scroll");
    assert_eq!(
        copy_mode.as_ref().expect("copy mode").viewport_offset,
        copy_maximum
    );
}

#[test]
fn frozen_copy_formatter_preserves_code_whitespace_and_line_structure() {
    let mut terminal = new_terminal(16, 4, 16).expect("terminal");
    terminal.vt_write(b"  one\r\n\r\n    two");
    let revision = ModeRevision::capture(&terminal).expect("revision");
    let row_with = |needle| {
        (0..revision.total_rows())
            .find(|row| {
                (0..revision.columns).any(|column| {
                    revision.first_char(PointCoordinate { x: column, y: *row }) == Some(needle)
                })
            })
            .expect("fixture row")
    };
    let one = row_with('o');
    let two = row_with('t');

    assert_eq!(
        revision.format_selection(
            ModeSelection {
                anchor: PointCoordinate { x: 0, y: one },
                focus: PointCoordinate { x: 6, y: two },
                mode: SelectionMode::Cell,
                rectangle: false,
            },
            true,
        ),
        "  one\n\n    two"
    );
    assert_eq!(
        revision.format_selection(
            ModeSelection {
                anchor: PointCoordinate { x: 0, y: one },
                focus: PointCoordinate {
                    x: revision.columns - 1,
                    y: one,
                },
                mode: SelectionMode::Line,
                rectangle: false,
            },
            true,
        ),
        "  one\n"
    );
}

#[test]
fn frozen_copy_formatter_preserves_wrapped_and_rectangular_columns() {
    let mut wrapped = new_terminal(5, 3, 16).expect("wrapped terminal");
    wrapped.vt_write(b"ab  cdef");
    let revision = ModeRevision::capture(&wrapped).expect("wrapped revision");
    let first = (0..revision.total_rows())
        .find(|row| revision.first_char(PointCoordinate { x: 0, y: *row }) == Some('a'))
        .expect("wrapped row");
    assert!(revision.row(first).wrapped());
    assert_eq!(
        revision.format_selection(
            ModeSelection {
                anchor: PointCoordinate { x: 0, y: first },
                focus: PointCoordinate { x: 2, y: first + 1 },
                mode: SelectionMode::Cell,
                rectangle: false,
            },
            true,
        ),
        "ab  cdef"
    );

    let mut rectangle = new_terminal(8, 2, 16).expect("rectangle terminal");
    rectangle.vt_write(b"ab  z\r\nc   y");
    let revision = ModeRevision::capture(&rectangle).expect("rectangle revision");
    let first = (0..revision.total_rows())
        .find(|row| revision.first_char(PointCoordinate { x: 0, y: *row }) == Some('a'))
        .expect("first rectangle row");
    let second = (0..revision.total_rows())
        .find(|row| revision.first_char(PointCoordinate { x: 0, y: *row }) == Some('c'))
        .expect("second rectangle row");
    assert_eq!(
        revision.format_selection(
            ModeSelection {
                anchor: PointCoordinate { x: 1, y: first },
                focus: PointCoordinate { x: 3, y: second },
                mode: SelectionMode::Cell,
                rectangle: true,
            },
            true,
        ),
        "b  \n   "
    );
    assert_eq!(
        revision.format_selection(
            ModeSelection {
                anchor: PointCoordinate { x: 1, y: first },
                focus: PointCoordinate { x: 3, y: second },
                mode: SelectionMode::Cell,
                rectangle: true,
            },
            false,
        ),
        "b \n  "
    );
}

#[test]
fn vi_copy_keeps_the_final_newline_when_the_right_edge_passes_the_last_line() {
    let mut terminal = new_terminal(12, 4, 16).expect("terminal");
    terminal.vt_write(b"abc\r\ndef\r\n\r\nghi");
    let revision = ModeRevision::capture(&terminal).expect("revision");
    let first = (0..revision.total_rows())
        .find(|row| revision.first_char(PointCoordinate { x: 0, y: *row }) == Some('a'))
        .expect("first row");
    let rectangle = |focus_x, vi| {
        revision.format_selection(
            ModeSelection {
                anchor: PointCoordinate { x: 0, y: first },
                focus: PointCoordinate {
                    x: focus_x,
                    y: first + 1,
                },
                mode: SelectionMode::Cell,
                rectangle: true,
            },
            vi,
        )
    };
    assert_eq!(rectangle(5, true), "abc\ndef\n");
    assert_eq!(rectangle(5, false), "abc\ndef");
    assert_eq!(rectangle(1, true), "ab\nde");
    assert_eq!(rectangle(2, true), "abc\ndef");
    let onto_empty_line = |vi| {
        revision.format_selection(
            ModeSelection {
                anchor: PointCoordinate { x: 0, y: first + 1 },
                focus: PointCoordinate { x: 0, y: first + 2 },
                mode: SelectionMode::Cell,
                rectangle: false,
            },
            vi,
        )
    };
    assert_eq!(onto_empty_line(true), "def\n\n");
    assert_eq!(onto_empty_line(false), "def\n");
}

#[test]
fn copy_mode_text_round_trips_through_the_paste_encoder() {
    let expected = "    let  x = 1";
    let mut terminal = new_terminal(16, 2, 16).expect("terminal");
    terminal.vt_write(expected.as_bytes());
    let mut live_selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let row = (0..copy_mode.as_ref().expect("mode").revision.total_rows())
        .find(|row| {
            copy_mode
                .as_ref()
                .expect("mode")
                .revision
                .first_char(PointCoordinate { x: 4, y: *row })
                == Some('l')
        })
        .expect("fixture row");
    let mode = copy_mode.as_mut().expect("mode");
    mode.selection = Some(ModeSelection {
        anchor: PointCoordinate { x: 0, y: row },
        focus: PointCoordinate { x: 13, y: row },
        mode: SelectionMode::Cell,
        rectangle: false,
    });
    mode.selecting = true;
    let mut unseen_output = 0;

    let result = apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::copy_selection(crate::CopyModeCopy {
            request_id: 1,
            clipboard: true,
            buffer: None,
            pipe: None,
            clear_selection: false,
            cancel: false,
        }),
        &WordSeparators::default(),
        true,
    )
    .expect("copy");
    let ViewActionResult::Copy(copy) = result else {
        panic!("expected copied text");
    };
    assert_eq!(copy.text, expected);

    let mut pasted = Vec::new();
    write_paste_bytes(&terminal, copy.text.into_bytes(), true, &mut pasted)
        .expect("paste encoding");
    assert_eq!(pasted, expected.as_bytes());

    copy_mode.as_mut().expect("mode").cursor = PointCoordinate { x: 4, y: row };
    let result = apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::copy_end_of_line(crate::CopyModeCopy {
            request_id: 2,
            clipboard: true,
            buffer: None,
            pipe: None,
            clear_selection: false,
            cancel: false,
        }),
        &WordSeparators::default(),
        false,
    )
    .expect("copy to end of line");
    assert!(matches!(
        result,
        ViewActionResult::Copy(copy) if copy.text == "let  x = 1"
    ));
}

#[test]
fn counted_copy_mode_end_of_line_spans_the_requested_rows_once() {
    let mut terminal = new_terminal(8, 4, 16).expect("terminal");
    terminal.vt_write(b"abcdef\r\nghijkl\r\nmnopqr");
    let mut live_selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let first = (0..copy_mode.as_ref().expect("mode").revision.total_rows())
        .find(|row| {
            copy_mode
                .as_ref()
                .expect("mode")
                .revision
                .first_char(PointCoordinate { x: 0, y: *row })
                == Some('a')
        })
        .expect("first fixture row");
    copy_mode.as_mut().expect("mode").cursor = PointCoordinate { x: 2, y: first };
    let mut unseen_output = 0;

    let result = apply_counted_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::copy_end_of_line(crate::CopyModeCopy {
            request_id: 3,
            clipboard: true,
            buffer: None,
            pipe: None,
            clear_selection: false,
            cancel: false,
        }),
        3,
        &WordSeparators::default(),
        false,
    )
    .expect("counted copy to end of line");

    assert!(matches!(
        result,
        ViewActionResult::Copy(copy) if copy.text == "cdef\nghijkl\nmnopqr"
    ));
    assert_eq!(
        copy_mode.as_ref().expect("mode").cursor,
        PointCoordinate { x: 2, y: first }
    );
}

#[test]
fn copy_mode_copy_variants_preserve_clear_and_cancel_independently() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"abcdef");
    let mut live_selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let row = copy_mode.as_ref().expect("mode").cursor.y;
    let selection = ModeSelection {
        anchor: PointCoordinate { x: 0, y: row },
        focus: PointCoordinate { x: 2, y: row },
        mode: SelectionMode::Cell,
        rectangle: false,
    };
    {
        let mode = copy_mode.as_mut().expect("mode");
        mode.selection = Some(selection);
        mode.selecting = true;
    }
    let mut unseen_output = 4;
    let word_separators = WordSeparators::default();

    let result = apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::copy_selection(crate::CopyModeCopy {
            request_id: 7,
            clipboard: false,
            buffer: Some(PasteBufferAction::Create {
                prefix: Some("native".to_owned()),
            }),
            pipe: Some("cat".to_owned()),
            clear_selection: false,
            cancel: false,
        }),
        &word_separators,
        false,
    )
    .expect("copy without clear");
    assert!(matches!(
        result,
        ViewActionResult::Copy(copy)
            if copy.request_id == 7
                && copy.clipboard.is_none()
                && matches!(copy.buffer, Some(PasteBufferAction::Create { .. }))
                && copy.pipe.as_deref() == Some("cat")
                && !copy.view_changed
    ));
    assert_eq!(copy_mode.as_ref().expect("mode").selection, Some(selection));

    let result = apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::copy_selection(crate::CopyModeCopy {
            request_id: 8,
            clipboard: true,
            buffer: None,
            pipe: None,
            clear_selection: true,
            cancel: false,
        }),
        &word_separators,
        false,
    )
    .expect("copy and clear");
    assert!(matches!(
        result,
        ViewActionResult::Copy(copy)
            if copy.clipboard == Some(ClipboardTarget::Clipboard) && copy.view_changed
    ));
    assert!(copy_mode.as_ref().expect("mode").selection.is_none());
    assert_eq!(unseen_output, 4);

    {
        let mode = copy_mode.as_mut().expect("mode");
        mode.selection = Some(selection);
        mode.selecting = true;
    }
    let result = apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::copy_selection(crate::CopyModeCopy {
            request_id: 9,
            clipboard: false,
            buffer: Some(PasteBufferAction::Append),
            pipe: None,
            clear_selection: true,
            cancel: true,
        }),
        &word_separators,
        false,
    )
    .expect("append and cancel");
    assert!(matches!(
        result,
        ViewActionResult::Copy(copy)
            if matches!(copy.buffer, Some(PasteBufferAction::Append)) && copy.view_changed
    ));
    assert!(copy_mode.is_none());
    assert_eq!(unseen_output, 0);
}

#[test]
fn copy_mode_marks_paragraphs_and_selection_modes_use_the_frozen_revision() {
    let mut terminal = new_terminal(12, 4, 32).expect("terminal");
    terminal.vt_write(b"  one\r\n\r\ntwo\r\nthree\r\n\r\nfour");
    let mut live_selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let row_with = |mode: &CopyModeState, needle: char| {
        (0..mode.revision.total_rows())
            .find(|row| {
                (0..mode.revision.columns).any(|column| {
                    mode.revision
                        .first_char(PointCoordinate { x: column, y: *row })
                        .is_some_and(|character| character == needle)
                })
            })
            .expect("fixture row")
    };
    let one = row_with(copy_mode.as_ref().expect("mode"), 'o');
    let two = row_with(copy_mode.as_ref().expect("mode"), 't');
    let four = row_with(copy_mode.as_ref().expect("mode"), 'f');
    let three = (0..copy_mode.as_ref().expect("mode").revision.total_rows())
        .find(|row| {
            copy_mode
                .as_ref()
                .expect("mode")
                .revision
                .first_char(PointCoordinate { x: 1, y: *row })
                == Some('h')
        })
        .expect("three row");
    let mut unseen_output = 0;
    let word_separators = WordSeparators::default();

    {
        let mode = copy_mode.as_mut().expect("mode");
        mode.cursor = PointCoordinate { x: 3, y: four };
    }
    apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::SetMark,
        &word_separators,
        false,
    )
    .expect("set mark");
    copy_mode.as_mut().expect("mode").cursor = PointCoordinate { x: 0, y: one };
    apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::JumpToMark,
        &word_separators,
        false,
    )
    .expect("jump to mark");
    assert_eq!(
        copy_mode.as_ref().expect("mode").cursor,
        PointCoordinate { x: 3, y: four }
    );

    {
        let mode = copy_mode.as_mut().expect("mode");
        mode.cursor = PointCoordinate { x: 0, y: one };
        move_copy_cursor(
            mode,
            &CopyModeAction::BackToIndentation,
            &word_separators,
            false,
        );
        assert_eq!(mode.cursor.x, 2);
        mode.cursor = PointCoordinate { x: 0, y: two };
        move_copy_cursor(
            mode,
            &CopyModeAction::NextParagraph,
            &word_separators,
            false,
        );
        assert_eq!(mode.cursor.y, four);
        move_copy_cursor(
            mode,
            &CopyModeAction::PreviousParagraph,
            &word_separators,
            false,
        );
        assert_eq!(mode.cursor.y, two);
        mode.cursor = PointCoordinate { x: 3, y: one };
    }

    apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::SelectWord,
        &word_separators,
        false,
    )
    .expect("select word");
    let selection = copy_mode
        .as_ref()
        .expect("mode")
        .selection
        .expect("word selection");
    assert_eq!(selection.anchor.x, 2);
    assert_eq!(selection.focus.x, 5);

    apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::OtherEnd,
        &word_separators,
        false,
    )
    .expect("switch active end");
    assert_eq!(copy_mode.as_ref().expect("mode").cursor.x, 2);

    apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::ClearSelection,
        &word_separators,
        false,
    )
    .expect("clear selection");
    apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::ToggleRectangle,
        &word_separators,
        false,
    )
    .expect("enable rectangle before selection");
    apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::StartSelection,
        &word_separators,
        false,
    )
    .expect("start rectangle selection");
    assert!(
        copy_mode
            .as_ref()
            .expect("mode")
            .selection
            .expect("selection")
            .rectangle
    );

    copy_mode.as_mut().expect("mode").cursor = PointCoordinate { x: 0, y: three };
    apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::Jump(CopyJump {
            target: "e".to_owned(),
            direction: CopyJumpDirection::Forward,
            to: false,
        }),
        &word_separators,
        false,
    )
    .expect("jump forward");
    assert_eq!(copy_mode.as_ref().expect("mode").cursor.x, 3);
    apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::RepeatJump { reverse: false },
        &word_separators,
        false,
    )
    .expect("repeat jump");
    assert_eq!(copy_mode.as_ref().expect("mode").cursor.x, 4);
    apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        CopyModeAction::RepeatJump { reverse: true },
        &word_separators,
        false,
    )
    .expect("reverse jump");
    assert_eq!(copy_mode.as_ref().expect("mode").cursor.x, 3);
}

#[test]
fn incremental_copy_mode_search_keeps_its_original_anchor() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"one\r\ntwo\r\nthree");
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
    let mut origin = None;

    let initial = search_selection_policy(
        copy_mode.as_deref(),
        &mut origin,
        SearchDirection::Backward,
        true,
    );
    let SearchSelectionPolicy::From {
        point: initial_point,
        ..
    } = initial
    else {
        panic!("copy mode must establish a search origin");
    };

    move_copy_cursor(
        copy_mode.as_mut().expect("copy mode"),
        &CopyModeAction::Up,
        &WordSeparators::default(),
        false,
    );
    let moved_point = copy_mode.as_ref().expect("copy mode").cursor;
    assert_ne!(moved_point, initial_point);

    let updated = search_selection_policy(
        copy_mode.as_deref(),
        &mut origin,
        SearchDirection::Backward,
        false,
    );
    assert!(matches!(
        updated,
        SearchSelectionPolicy::From { point, .. } if point == initial_point
    ));
}

#[test]
fn copy_mode_revision_stays_frozen_across_output_resize_and_screen_switch() {
    let mut terminal = new_terminal(16, 3, 16).expect("terminal");
    terminal.vt_write(b"old-one\r\nold-two\r\n\x1b[1;31mfrozen-marker\x1b[0m");
    let mut selection = None;
    let mut mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let mut view = TerminalViewState::for_screen(Screen::Primary);
    view.copy_mode = mode;
    let mut render_state = RenderState::new().expect("render state");
    let mut row_iterator = RowIterator::new().expect("rows");
    let mut cell_iterator = CellIterator::new().expect("cells");
    let mut generations = ViewportGenerations::default();
    let mut dictionary = ViewportDictionary::default();

    let frozen = snapshot(
        &terminal,
        &mut render_state,
        &mut row_iterator,
        &mut cell_iterator,
        &mut generations,
        SnapshotChange::View,
        &mut dictionary,
        Some(&view),
        SessionStatus::Running,
        None,
    )
    .expect("frozen snapshot");
    terminal.vt_write(b"\r\nlive-one\r\nlive-two\x1b[?1049hALT-SCREEN");
    terminal.resize(22, 5, 8, 18).expect("resize live terminal");
    view.note_output(Screen::Alternate);
    reconcile_view_screen(&mut terminal, &mut view, &WordSeparators::default())
        .expect("frozen screen");

    let after = snapshot(
        &terminal,
        &mut render_state,
        &mut row_iterator,
        &mut cell_iterator,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        Some(&view),
        SessionStatus::Running,
        None,
    )
    .expect("snapshot after live changes");
    assert_eq!((after.columns, after.rows), (16, 3));
    assert!(Arc::ptr_eq(&frozen.cells, &after.cells));
    assert_eq!(frozen.cells, after.cells);
    assert_eq!(after.unseen_output, 1);
    assert_eq!(view.screen, Screen::Primary);
    let patch = TerminalViewport::diff(&frozen, &after).expect("metadata-only mode patch");
    assert!(patch.changed_rows.is_empty());

    let captured = capture_terminal(
        &terminal,
        view.copy_mode.as_deref(),
        CaptureOptions {
            mode: true,
            ..CaptureOptions::default()
        },
    )
    .expect("mode capture");
    assert!(captured.contains("frozen-marker"));
    assert!(!captured.contains("live-one"));
    assert!(!captured.contains("ALT-SCREEN"));
    let captured_vt = capture_terminal(
        &terminal,
        view.copy_mode.as_deref(),
        CaptureOptions {
            mode: true,
            escape_sequences: true,
            ..CaptureOptions::default()
        },
    )
    .expect("styled mode capture");
    assert!(captured_vt.contains("\x1b[1m\x1b[31mfrozen-marker\x1b[0m"));
    assert!(captured_vt.contains("frozen-marker"));
}
