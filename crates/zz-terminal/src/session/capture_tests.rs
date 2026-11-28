use super::test_support::*;
use super::*;

#[test]
fn capture_reads_visible_rows_and_full_canonical_history() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"old-0\r\nold-1\r\nview-0\r\nview-1");

    let visible =
        capture_terminal(&terminal, None, CaptureOptions::default()).expect("visible capture");
    assert_eq!(visible, "view-0\nview-1");

    let full = capture_terminal(
        &terminal,
        None,
        CaptureOptions {
            start: CaptureBoundary::HistoryStart,
            ..CaptureOptions::default()
        },
    )
    .expect("history capture");
    assert_eq!(full, "old-0\nold-1\nview-0\nview-1");
}

#[test]
fn capture_can_join_soft_wrapped_rows() {
    let mut terminal = new_terminal(4, 2, 16).expect("terminal");
    terminal.vt_write(b"abcdefgh");

    let output = capture_terminal(
        &terminal,
        None,
        CaptureOptions {
            join_wrapped: true,
            ..CaptureOptions::default()
        },
    )
    .expect("joined capture");
    assert_eq!(output, "abcdefgh");
}

#[test]
fn capture_returns_the_spaces_a_tab_left() {
    for (input, expected) in [
        ("ABC\t\r\nNEXT", "ABC\nNEXT\n\n\n"),
        ("ABC\tDEF\r\nNEXT", "ABC     DEF\nNEXT\n\n\n"),
        ("界\t\r\nNEXT", "界\nNEXT\n\n\n"),
        ("ABC\tDEF\r\x1b[4G\x1b[P\r\nNEXT", "ABC    DEF\nNEXT\n\n\n"),
    ] {
        let mut terminal = new_terminal(80, 24, 64).unwrap();
        terminal.vt_write(input.as_bytes());
        let options = CaptureOptions {
            end: CaptureBoundary::Relative(4),
            ..CaptureOptions::default()
        };
        assert_eq!(
            capture_terminal(&terminal, None, options).unwrap(),
            expected,
            "{input:?}"
        );
    }
}

#[test]
fn capture_clears_the_cells_a_wide_insert_crosses() {
    let mut terminal = new_terminal(80, 24, 64).unwrap();
    terminal.vt_write(b"\x1b[71GABCDEFGHIJ\r\x1b[70G\x1b[6@\x1b[5;1HNEXT");
    let options = CaptureOptions {
        end: CaptureBoundary::Relative(4),
        ..CaptureOptions::default()
    };
    assert_eq!(
        capture_terminal(&terminal, None, options).unwrap(),
        format!("{}ABCD\n\n\n\nNEXT", " ".repeat(76))
    );
}

#[test]
fn styled_capture_emits_colour_and_attribute_transitions() {
    for (input, expected) in [
        (
            "\x1b[44mABC\tDEF\x1b[0m\r\x1b[6G\x1b[41mX\x1b[0m",
            "\x1b[44mABC\x1b[49m  \x1b[41mX\x1b[49m  \x1b[44mDEF\x1b[49m",
        ),
        ("\x1b[31mRED\x1b[0m", "\x1b[31mRED\x1b[39m"),
        ("\x1b[38;5;1mRED\x1b[0m", "\x1b[31mRED\x1b[39m"),
        (
            "\x1b[31mA\x1b[38;5;1mB\x1b[31mC\x1b[0m",
            "\x1b[31mABC\x1b[39m",
        ),
        ("\x1b[48;5;1mA\x1b[41mB\x1b[0m", "\x1b[41mAB\x1b[49m"),
        ("\x1b[48;5;1m\x1b[2K\x1b[0m", "\x1b[41m"),
        ("\x1b[38;5;196mRED\x1b[0m", "\x1b[38;5;196mRED\x1b[39m"),
        ("\x1b[38;2;1;2;3mRGB\x1b[0m", "\x1b[38;2;1;2;3mRGB\x1b[39m"),
        ("\x1b[1mBOLD\x1b[0m", "\x1b[1mBOLD\x1b[0m"),
        ("\x1b[4mUNDER\x1b[0m", "\x1b[4mUNDER\x1b[0m"),
        ("\x1b[41m\x1b[2K\x1b[0m", "\x1b[41m"),
        ("\x1b[48;2;4;5;6m\x1b[2K\x1b[0m", "\x1b[48;2;4;5;6m"),
        (
            "\x1b[1;4;31mONE\x1b[22mTWO\x1b[0m",
            "\x1b[1;4m\x1b[31mONE\x1b[0;4m\x1b[31mTWO\x1b[0m",
        ),
    ] {
        let mut terminal = new_terminal(80, 24, 64).expect("terminal");
        terminal.vt_write(input.as_bytes());
        for escaped in [false, true] {
            let output = capture_terminal(
                &terminal,
                None,
                CaptureOptions {
                    end: CaptureBoundary::Relative(0),
                    escape_sequences: true,
                    escape_nonprintable: escaped,
                    ..CaptureOptions::default()
                },
            )
            .expect("capture");
            let expected = if escaped {
                expected.replace('\x1b', "\\033")
            } else {
                expected.to_owned()
            };
            assert_eq!(output, expected, "{input:?}");
        }
    }
}

#[test]
fn styled_numbered_capture_preserves_the_row_after_three_wrapped_rows() {
    let mut terminal = new_terminal(80, 24, 64).expect("terminal");
    terminal.vt_write(format!("\x1b[31m{}\x1b[0m\r\nNEXT", "A".repeat(170)).as_bytes());
    let output = capture_terminal(
        &terminal,
        None,
        CaptureOptions {
            end: CaptureBoundary::Relative(3),
            escape_sequences: true,
            number_lines: true,
            join_wrapped: true,
            preserve_trailing: true,
            ..CaptureOptions::default()
        },
    )
    .expect("capture");
    assert_eq!(
        output,
        format!(
            "0 \x1b[31m{}1 {}2 {}\n3 \x1b[39mNEXT",
            "A".repeat(80),
            "A".repeat(80),
            "A".repeat(10)
        )
    );
}

#[test]
fn styled_capture_trims_erased_backgrounds_to_text_extent() {
    for erase in ["\x1b[2K", "\x1b[2J"] {
        let mut terminal = new_terminal(80, 24, 64).expect("terminal");
        terminal.vt_write(format!("\x1b[41m{erase}\x1b[0m\r\nNEXT").as_bytes());
        for join_wrapped in [false, true] {
            let output = capture_terminal(
                &terminal,
                None,
                CaptureOptions {
                    end: CaptureBoundary::Relative(4),
                    escape_sequences: true,
                    trim_positions: !join_wrapped,
                    join_wrapped,
                    number_lines: join_wrapped,
                    ..CaptureOptions::default()
                },
            )
            .expect("capture");
            assert_eq!(
                output,
                if join_wrapped {
                    "0 \n1 NEXT\n2 \n3 \n4 "
                } else {
                    "\nNEXT\n\n\n"
                },
                "{erase:?} join={join_wrapped}"
            );
        }
    }
}

#[test]
fn styled_capture_preserves_default_padding_before_wide_wraps() {
    for cols in [80, 100] {
        let mut terminal = new_terminal(cols, 24, 64).expect("terminal");
        let text = "A".repeat(usize::from(cols - 1));
        terminal.vt_write(format!("\x1b[31m{text}界界\x1b[0mNEXT").as_bytes());
        for preserve_trailing in [false, true] {
            let output = capture_terminal(
                &terminal,
                None,
                CaptureOptions {
                    end: CaptureBoundary::Relative(1),
                    escape_sequences: true,
                    preserve_trailing,
                    ..CaptureOptions::default()
                },
            )
            .expect("capture");
            let padding = if preserve_trailing { " " } else { "" };
            let tail = if preserve_trailing {
                " ".repeat(usize::from(cols / 4 - 8))
            } else {
                String::new()
            };
            assert_eq!(
                output,
                format!("\x1b[31m{text}\x1b[39m{padding}\n\x1b[31m界界\x1b[39mNEXT{tail}")
            );
        }
    }
}

#[test]
fn numbered_capture_uses_physical_rows_for_styled_wraps() {
    let rows = "\x1b[31mAAAA\x1b[0m\n\x1b[31mAAAA\x1b[0m\n\x1b[31mAA\x1b[0m\nNEXT";
    assert_eq!(
        number_capture(rows, Some(&[true, true, false, false]), (0, 0), true, None),
        "0 \x1b[31mAAAA\x1b[0m1 \x1b[31mAAAA\x1b[0m2 \x1b[31mAA\x1b[0m\n3 NEXT"
    );
    assert_eq!(
        number_capture("old\nnew", None, (2, 3), true, None),
        "-1 old\n0 new"
    );
}

#[test]
fn live_styled_capture_keeps_sgr_and_uses_lf_between_rows() {
    let mut terminal = new_terminal(4, 3, 16).expect("terminal");
    terminal.vt_write(b"\x1b[31mabcdef\r\ngh");
    for (join_wrapped, lines) in [(false, 3), (true, 2)] {
        let captured = capture_terminal(
            &terminal,
            None,
            CaptureOptions {
                escape_sequences: true,
                join_wrapped,
                ..CaptureOptions::default()
            },
        )
        .expect("styled capture");
        assert!(captured.contains("\x1b["), "{captured:?}");
        assert!(!captured.contains('\r'), "{captured:?}");
        assert_eq!(captured.lines().count(), lines, "{captured:?}");
    }
}

#[test]
fn osc133_prompt_navigation_finds_prompts_and_command_output() {
    let mut terminal = new_terminal(24, 8, 16).expect("terminal");
    terminal.vt_write(
        b"\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\one\x1b]133;C\x1b\\\r\nout-one\r\n\x1b]133;D;0\x1b\\\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\two\x1b]133;C\x1b\\\r\nout-two",
    );

    assert_eq!(
        semantic_prompt_target(&terminal, 3, -1, false).expect("previous prompt"),
        Some(PointCoordinate { x: 0, y: 2 })
    );
    assert_eq!(
        semantic_prompt_target(&terminal, 2, -1, false).expect("older prompt"),
        Some(PointCoordinate { x: 0, y: 0 })
    );
    assert_eq!(
        semantic_prompt_target(&terminal, 0, 1, false).expect("next prompt"),
        Some(PointCoordinate { x: 0, y: 2 })
    );
    assert_eq!(
        semantic_prompt_target(&terminal, 3, -1, true).expect("previous output"),
        Some(PointCoordinate { x: 0, y: 3 })
    );

    let mut selection = None;
    assert!(
        select_semantic_output_at(
            &terminal,
            &mut selection,
            PointerCellEvent {
                column: 2,
                row: 1,
                click_count: 3,
                rectangle: false,
            },
        )
        .expect("semantic output selection")
    );
    assert_eq!(
        format_selection_text(&terminal).expect("selected output"),
        "out-one"
    );
}

#[test]
fn last_command_capture_skips_the_fresh_prompt_and_keeps_the_previous_output() {
    let mut terminal = new_terminal(24, 8, 64).expect("terminal");
    terminal.vt_write(
        b"\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\one\x1b]133;C\x1b\\\r\nout-one\r\n\x1b]133;D;0\x1b\\\
              \x1b]133;A\x1b\\$ \x1b]133;B\x1b\\two --flag\x1b]133;C\x1b\\\r\nout-two\r\nmore-two\r\n\x1b]133;D;1\x1b\\\
              \x1b]133;A\x1b\\$ ",
    );

    let capture = capture_last_command(&terminal).expect("last command");
    assert_eq!(capture.command, "two --flag");
    assert_eq!(capture.output, "out-two\nmore-two");
    assert_eq!(capture.truncated_rows, 0);
}

#[test]
fn last_command_capture_joins_soft_wrapped_rows() {
    let mut terminal = new_terminal(24, 8, 64).expect("terminal");
    let command = "echo abcdefghijklmnopqrstuvwxyz0123456789";
    let output = "o".repeat(30);
    terminal.vt_write(
        format!(
            "\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\{command}\x1b]133;C\x1b\\\r\n{output}\r\n\x1b]133;D;0\x1b\\\x1b]133;A\x1b\\$ "
        )
        .as_bytes(),
    );

    let capture = capture_last_command(&terminal).expect("last command");
    assert_eq!(capture.command, command);
    assert_eq!(capture.output, output);
}

#[test]
fn last_command_capture_reports_a_running_command() {
    let session =
        TerminalSession::spawn_empty_with_appearance(64, Arc::new(TerminalAppearance::default()));
    assert!(session.feed(Arc::from(
        b"\x1b]133;A\x07$ \x1b]133;B\x07one\x1b]133;C\x07\r\nout\r\n\x1b]133;D;1\x07\x1b]133;A\x07$ ".as_slice()
    )));
    let capture = session.capture_last_command().expect("last command");
    assert_eq!((capture.command.as_str(), capture.running), ("one", false));
    assert!(session.feed(Arc::from(
        b"\x1b]133;B\x07sleep 9\x1b]133;C\x07\r\n".as_slice()
    )));
    let capture = session.capture_last_command().expect("running command");
    assert_eq!(
        (capture.command.as_str(), capture.running),
        ("sleep 9", true)
    );
}

#[test]
fn skip_input_blanks_prompt_and_typed_input_lines() {
    let options = CaptureOptions {
        start: CaptureBoundary::HistoryStart,
        join_wrapped: true,
        preserve_trailing: true,
        ..CaptureOptions::default()
    };
    let skip = CaptureOptions {
        skip_input: true,
        ..options
    };
    let mut terminal = new_terminal(24, 8, 64).expect("terminal");
    terminal.vt_write(
        b"\x1b]133;A\x07$ \x1b]133;B\x07echo DONE\x1b]133;C\x07\r\nDONE\r\n\
              \x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07echo 'a\r\n> b'\x1b]133;C\x07\r\nout\r\n",
    );
    let plain = capture_terminal(&terminal, None, options).expect("capture");
    let skipped = capture_terminal(&terminal, None, skip).expect("capture");
    assert_eq!(
        plain.lines().take(5).collect::<Vec<_>>(),
        ["$ echo DONE", "DONE", "$ echo 'a", "> b'", "out"]
    );
    assert_eq!(
        skipped.lines().take(5).collect::<Vec<_>>(),
        ["", "DONE", "", "", "out"]
    );
    assert_eq!(skipped.lines().count(), plain.lines().count());
    let mut bare = new_terminal(24, 8, 64).expect("terminal");
    bare.vt_write(b"$ echo DONE\r\nDONE\r\n");
    assert_eq!(
        capture_terminal(&bare, None, skip),
        capture_terminal(&bare, None, options)
    );
}

#[test]
fn skip_input_keeps_output_printed_without_a_command_mark() {
    let skip = CaptureOptions {
        start: CaptureBoundary::HistoryStart,
        join_wrapped: true,
        preserve_trailing: true,
        skip_input: true,
        ..CaptureOptions::default()
    };
    let mut terminal = new_terminal(24, 8, 64).expect("terminal");
    terminal.vt_write(
        b"\x1b]133;A\x07$ \x1b]133;B\x07(echo DONE)\r\nDONE\r\n\
              \x1b]133;A\x07$ \x1b]133;B\x07",
    );
    let skipped = capture_terminal(&terminal, None, skip).expect("capture");
    assert_eq!(
        skipped.lines().take(3).collect::<Vec<_>>(),
        ["", "DONE", ""]
    );
}

#[test]
fn keep_idle_prompt_keeps_only_output_below_the_newest_prompt() {
    let strict = CaptureOptions {
        start: CaptureBoundary::HistoryStart,
        join_wrapped: true,
        preserve_trailing: true,
        skip_input: true,
        ..CaptureOptions::default()
    };
    let idle = CaptureOptions {
        keep_idle_prompt: true,
        ..strict
    };
    let first = |bytes: &[u8], options| {
        let mut terminal = new_terminal(24, 8, 64).expect("terminal");
        terminal.vt_write(b"\x1b]133;A\x07$ \x1b]133;B\x07");
        terminal.vt_write(bytes);
        capture_terminal(&terminal, None, options)
            .expect("capture")
            .lines()
            .take(3)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(first(b"READY\r\n", strict), ["", "", ""]);
    assert_eq!(first(b"READY\r\n", idle), ["$ READY", "", ""]);
    assert_eq!(
        first(b"READY\r\nL2\r\nL3\r\n", idle),
        ["$ READY", "L2", "L3"]
    );
    assert_eq!(first(b"READY\r\nL2\r\nL3\r\n", strict), ["", "L2", "L3"]);
    assert_eq!(first(b"cmd DONE\r\n", strict), ["", "", ""]);
    assert_eq!(first(b"cmd DONE\r\n", idle), ["$ cmd DONE", "", ""]);
    for typed in [
        b"cmd DONE".as_slice(),
        b"cmd DONE\r\n\x1b]133;C\x07",
        b"cmd DONE\r\n\x1b]133;C\x07\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07",
        b"cmd DONE^C\r\n\x1b]133;A\x07$ \x1b]133;B\x07",
        b"echo 'DONE\r\n> ",
    ] {
        let lines = first(typed, idle);
        assert!(
            lines.iter().all(|line| !line.contains("DONE")),
            "{typed:?}: {lines:?}"
        );
    }
}

#[test]
fn skip_input_blanks_a_wrapped_prompt_split_across_capture_steps() {
    let session =
        TerminalSession::spawn_empty_with_appearance(4096, Arc::new(TerminalAppearance::default()));
    session.resize(40, 10, 8, 16);
    let mut bytes = "x\r\n"
        .repeat(usize::try_from(CAPTURE_ROWS_PER_STEP).unwrap() - 1)
        .into_bytes();
    let command = format!("echo {}", "y".repeat(40));
    bytes.extend(
        format!("\x1b]133;A\x07\x1b]133;B\x07{command}\x1b]133;C\x07\r\nafter\r\n").bytes(),
    );
    assert!(session.feed(Arc::from(bytes)));
    let options = CaptureOptions {
        start: CaptureBoundary::HistoryStart,
        join_wrapped: true,
        preserve_trailing: true,
        ..CaptureOptions::default()
    };
    let plain = session.capture(options).expect("capture");
    assert!(plain.contains(&command), "{plain:?}");
    let skipped = session
        .capture(CaptureOptions {
            skip_input: true,
            ..options
        })
        .expect("capture");
    assert!(!skipped.contains('y'), "{skipped:?}");
    assert!(skipped.contains("after"));
    assert_eq!(skipped.lines().count(), plain.lines().count());
}

#[test]
fn keep_idle_prompt_reaches_a_prompt_past_the_first_capture_step() {
    let session =
        TerminalSession::spawn_empty_with_appearance(4096, Arc::new(TerminalAppearance::default()));
    session.resize(40, 10, 8, 16);
    let mut bytes = "x\r\n"
        .repeat(usize::try_from(CAPTURE_ROWS_PER_STEP).unwrap() + 8)
        .into_bytes();
    bytes.extend(b"\x1b]133;A\x07$ \x1b]133;B\x07READY\r\n");
    assert!(session.feed(Arc::from(bytes)));
    let strict = CaptureOptions {
        start: CaptureBoundary::HistoryStart,
        join_wrapped: true,
        preserve_trailing: true,
        skip_input: true,
        ..CaptureOptions::default()
    };
    let skipped = session.capture(strict).expect("capture");
    assert!(!skipped.contains("READY"), "{skipped:?}");
    let kept = session
        .capture(CaptureOptions {
            keep_idle_prompt: true,
            ..strict
        })
        .expect("capture");
    assert!(kept.contains("$ READY"), "{kept:?}");
    assert_eq!(kept.lines().count(), skipped.lines().count());
}

#[test]
fn session_reports_the_last_completed_command_status() {
    let session =
        TerminalSession::spawn_empty_with_appearance(64, Arc::new(TerminalAppearance::default()));
    assert_eq!(session.last_command_status(), None);
    for (bytes, command, status) in [
        (
            b"\x1b]133;A\x07$ \x1b]133;B\x07one\x1b]133;C\x07\r\nout-one\r\n\x1b]133;D;0\x07\x1b]133;A\x07$ ".as_slice(),
            "one",
            Some(0),
        ),
        (
            b"\x1b]133;B\x07two --flag\x1b]133;C\x07\r\nout-two\r\nmore-two\r\n\x1b]133;D;1\x07\x1b]133;A\x07$ ".as_slice(),
            "two --flag",
            Some(1),
        ),
        (b"\x1b]133;D\x07".as_slice(), "two --flag", None),
    ] {
        assert!(session.feed(Arc::from(bytes)));
        let capture = session.capture_last_command().expect("last command");
        assert_eq!(capture.command, command);
        assert_eq!(session.last_command_status(), status);
    }
}

#[test]
fn scroll_on_clear_keeps_the_prompt_mark_of_the_erased_origin_row() {
    let mut terminal = new_terminal(20, 4, 16).expect("terminal");
    let mut filter = EngineFilter::default();
    let mut renames = Vec::new();
    let mut bar = None;
    for chunk in [
        b"banner\r\n\x1b[H".as_slice(),
        b"\x1b]133;A\x07\r\x1b[J$ \x1b]133;B\x07echo hi\r\n",
        b"\x1b]133;C\x07hi\r\n\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07",
    ] {
        filter.write(
            chunk,
            EngineKnobs::default(),
            &mut terminal,
            &mut renames,
            &mut bar,
            &mut None,
        );
    }
    let capture = capture_last_command(&terminal).expect("marks survive the scroll");
    assert_eq!(capture.command, "echo hi");
    assert_eq!(capture.output, "hi");
}

#[test]
fn scroll_on_clear_keeps_a_lone_prompt_out_of_history() {
    let mut terminal = new_terminal(20, 4, 16).expect("terminal");
    let mut filter = EngineFilter::default();
    let mut renames = Vec::new();
    let mut bar = None;
    for chunk in [b"\x1b]133;A\x07> \x1b]133;B\x07".as_slice(), b"\r\x1b[J> "] {
        filter.write(
            chunk,
            EngineKnobs::default(),
            &mut terminal,
            &mut renames,
            &mut bar,
            &mut None,
        );
    }
    assert_eq!(terminal.scrollback_rows().expect("scrollback rows"), 0);
}

#[test]
fn scroll_on_clear_keeps_shell_startup_drawing_out_of_history() {
    let startup = b"\x1b7\r\n~ >      16:08\x1b[?25l\x1b8\x1b[J".as_slice();
    let prompt = b"\x1b]133;A;redraw=1\x07\r\n~ >      16:08\r\x1b[4C\x1b]133;B\x07";
    for (shell_starting, history) in [(true, 0), (false, 2)] {
        let mut terminal = new_terminal(20, 4, 16).expect("terminal");
        let mut filter = EngineFilter {
            shell_starting,
            ..EngineFilter::default()
        };
        for chunk in [startup, prompt] {
            filter.write(
                chunk,
                EngineKnobs::default(),
                &mut terminal,
                &mut Vec::new(),
                &mut None,
                &mut None,
            );
        }
        assert_eq!(
            terminal.scrollback_rows().expect("scrollback rows"),
            history,
            "shell_starting {shell_starting}"
        );
        assert_eq!(
            capture_terminal(&terminal, None, CaptureOptions::default()).expect("screen"),
            "\n~ >      16:08\n\n"
        );
    }
}

#[test]
fn last_command_capture_tolerates_duplicate_osc133_marks() {
    let mut terminal = new_terminal(80, 8, 64).expect("terminal");
    terminal.vt_write(
        b"\x1b]133;A\x07\x1b]133;A\x07$ \x1b]133;B\x07\x1b]133;B\x07echo hi\r\n\
              \x1b]133;C\x07\x1b]133;C\x07hi\r\n\x1b]133;D;0\x07\x1b]133;D;0\x07\
              \x1b]133;A\x07\x1b]133;A\x07$ \x1b]133;B\x07\x1b]133;B\x07",
    );

    let capture = capture_last_command(&terminal).expect("last command with duplicate marks");
    assert_eq!(capture.command, "echo hi");
    assert_eq!(capture.output, "hi");
    assert_eq!(capture.truncated_rows, 0);

    terminal.vt_write(b"\r\n\x1b]133;A\x07\x1b]133;A\x07$ \x1b]133;B\x07\x1b]133;B\x07");
    assert_eq!(
        capture_last_command(&terminal).expect("empty prompt"),
        capture
    );
}

#[test]
fn last_command_capture_reports_missing_shell_integration() {
    let mut terminal = new_terminal(24, 4, 16).expect("terminal");
    terminal.vt_write(b"$ ls\r\nfile-a\r\n$ ");

    assert_eq!(
        capture_last_command(&terminal),
        Err(TerminalCaptureError::NoSemanticMarks)
    );
}

#[test]
fn last_command_capture_caps_long_output_by_rows() {
    let mut terminal = new_terminal(24, 8, 4096).expect("terminal");
    let mut bytes = b"\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\loop\x1b]133;C\x1b\\\r\n".to_vec();
    for index in 0..(MAX_LAST_COMMAND_LINES + 25) {
        bytes.extend_from_slice(format!("line-{index}\r\n").as_bytes());
    }
    bytes.extend_from_slice(b"\x1b]133;D;0\x1b\\\x1b]133;A\x1b\\$ ");
    terminal.vt_write(&bytes);

    let capture = capture_last_command(&terminal).expect("last command");
    assert_eq!(capture.command, "loop");
    assert_eq!(capture.output.lines().count(), MAX_LAST_COMMAND_LINES);
    assert!(capture.truncated_rows >= 25);
    assert!(capture.output.starts_with("line-25"));
    assert!(
        capture
            .output
            .ends_with(&format!("line-{}", MAX_LAST_COMMAND_LINES + 24))
    );
}

#[test]
fn last_command_capture_walks_past_prompts_the_user_only_sat_on() {
    let mut terminal = new_terminal(24, 8, 64).expect("terminal");
    terminal.vt_write(
        b"\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\one\x1b]133;C\x1b\\\r\nout-one\r\n\x1b]133;D;0\x1b\\\
              \x1b]133;A\x1b\\$ \x1b]133;B\x1b\\\r\n\
              \x1b]133;A\x1b\\$ ",
    );

    let capture = capture_last_command(&terminal).expect("last command");
    assert_eq!(capture.command, "one");
    assert_eq!(capture.output, "out-one");
    assert_eq!(capture.truncated_rows, 0);
}

#[test]
fn output_byte_cap_trims_whole_leading_lines() {
    let line = "x".repeat(1024);
    let mut output = String::new();
    for _ in 0..400 {
        output.push_str(&line);
        output.push('\n');
    }
    output.push_str(&line);

    let (clamped, dropped) = clamp_output_bytes(output);
    assert_eq!(dropped, 146);
    assert_eq!(clamped, vec![line.as_str(); 255].join("\n"));
}

#[test]
fn pty_free_surface_feeds_bytes_into_capture_marks_output_and_bell() {
    let session =
        TerminalSession::spawn_empty_with_appearance(64, Arc::new(TerminalAppearance::default()));
    let events = session.events();
    let (_sink, tapped) = TestOutputSink::install(&session);

    assert!(session.feed(Arc::from(
        b"\x1b]133;A\x07> \x1b]133;B\x07say hi\x1b]133;C\x07\r\nhello from the agent\r\n\x1b]133;D;0\x07\x07"
            .as_slice()
    )));
    let capture =
        wait_for_test_capture(&session, |capture| capture.contains("hello from the agent"));
    assert!(capture.contains("> say hi"), "{capture:?}");
    let last = session
        .capture_last_command()
        .expect("OSC 133 marks land from fed bytes");
    assert_eq!(last.command, "say hi");
    assert_eq!(last.output, "hello from the agent");

    let tapped = tapped
        .recv_timeout(Duration::from_secs(5))
        .expect("the output sink sees the fed bytes");
    assert!(tapped.starts_with(b"\x1b]133;A"));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match events.recv_blocking().expect("events stay open") {
            TerminalEvent::Bell => break,
            _ => assert!(Instant::now() < deadline, "no bell from the fed BEL"),
        }
    }
}
