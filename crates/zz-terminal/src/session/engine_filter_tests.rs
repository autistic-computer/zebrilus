#[cfg(unix)]
use super::test_support::*;
use super::*;
#[cfg(unix)]
use crate::DEFAULT_HISTORY_LIMIT;

#[test]
fn terminal_facts_keep_primary_history_across_split_alternate_sequences() {
    let mut bytes = Vec::new();
    for line in 1..=100 {
        write!(bytes, "{line}\r\n").expect("terminal output fixture");
    }
    bytes.extend_from_slice(b"\x1b[?1049h");
    for chunk_size in [1, 7, bytes.len()] {
        let mut terminal = new_terminal(80, 24, 2000).expect("terminal");
        let mut filter = EngineFilter::default();
        let mut renames = Vec::new();
        let mut bar = None;
        for chunk in bytes.chunks(chunk_size) {
            filter.write(
                chunk,
                EngineKnobs::default(),
                &mut terminal,
                &mut renames,
                &mut bar,
                &mut None,
            );
        }
        assert_eq!(
            filter.facts(&terminal).expect("alternate facts"),
            TerminalFacts {
                history_size: 77,
                cursor_x: 0,
                cursor_y: 23,
                alternate_on: true,
                mouse_tracking: false,
                cursor_hidden: false,
                program_title_writes: 0,
                ..filter.facts(&terminal).expect("alternate facts")
            }
        );
        filter.write(
            b"\x1b[?1049h\x1b[?1049l",
            EngineKnobs::default(),
            &mut terminal,
            &mut renames,
            &mut bar,
            &mut None,
        );
        filter.write(
            &[b'x'; 80],
            EngineKnobs::default(),
            &mut terminal,
            &mut renames,
            &mut bar,
            &mut None,
        );
        assert_eq!(
            filter.facts(&terminal).expect("primary facts"),
            TerminalFacts {
                history_size: 77,
                cursor_x: 80,
                cursor_y: 23,
                alternate_on: false,
                mouse_tracking: false,
                cursor_hidden: false,
                program_title_writes: 0,
                ..filter.facts(&terminal).expect("primary facts")
            }
        );
    }
}

fn engine_filter_screen(knobs: EngineKnobs, chunks: &[&[u8]]) -> (String, Vec<String>) {
    let (text, renames, _) = engine_filter_run(knobs, chunks);
    (text, renames)
}

fn engine_filter_run(knobs: EngineKnobs, chunks: &[&[u8]]) -> (String, Vec<String>, ProgressBar) {
    let mut terminal = new_terminal(20, 4, 16).expect("terminal");
    let mut filter = EngineFilter::default();
    let mut renames = Vec::new();
    let mut bar = None;
    for chunk in chunks {
        filter.write(
            chunk,
            knobs,
            &mut terminal,
            &mut renames,
            &mut bar,
            &mut None,
        );
    }
    let revision = ModeRevision::capture(&terminal).expect("revision");
    let rows = revision.total_rows();
    (
        revision.capture_rows(0, rows.saturating_sub(1), false, false),
        renames,
        filter.bar,
    )
}

fn progress_after(sequences: &[&str]) -> ProgressBar {
    let chunks = sequences
        .iter()
        .map(|sequence| format!("\x1b]{sequence}\x07").into_bytes())
        .collect::<Vec<_>>();
    let borrowed = chunks
        .iter()
        .map(std::vec::Vec::as_slice)
        .collect::<Vec<_>>();
    let (_, _, bar) = engine_filter_run(EngineKnobs::default(), &borrowed);
    bar
}

#[test]
fn engine_filter_reads_the_pinned_progress_bar_walk() {
    assert_eq!(progress_after(&[]), ProgressBar::default());
    assert_eq!(
        progress_after(&["9;4;0"]),
        ProgressBar {
            state: ProgressBarState::Hidden,
            progress: 0,
        }
    );
    assert_eq!(
        progress_after(&["9;4;0", "9;4;1;50"]),
        ProgressBar {
            state: ProgressBarState::Normal,
            progress: 50,
        }
    );
    assert_eq!(
        progress_after(&["9;4;1;50", "9;4;2;30"]),
        ProgressBar {
            state: ProgressBarState::Error,
            progress: 30,
        }
    );
    assert_eq!(
        progress_after(&["9;4;2;30", "9;4;3"]),
        ProgressBar {
            state: ProgressBarState::Indeterminate,
            progress: 30,
        }
    );
    assert_eq!(
        progress_after(&["9;4;2;30", "9;4;4;10"]),
        ProgressBar {
            state: ProgressBarState::Paused,
            progress: 10,
        }
    );
}

#[test]
fn engine_filter_tracks_command_status_until_the_next_completion() {
    let mut terminal = new_terminal(20, 4, 16).expect("terminal");
    let mut filter = EngineFilter::default();
    let mut status = None;
    for (payload, expected) in [
        ("133;D;7", Some(CommandStatusUpdate::Exit(7))),
        ("133;A", None),
        ("133;B", None),
        ("133;C", None),
        ("0;title", None),
        ("9;4;1;50", None),
        ("133;D;garbage", None),
        ("133;D;", None),
        ("133;D;2147483648", None),
        ("133;D;7;extra", None),
        ("133;D;0", Some(CommandStatusUpdate::Exit(0))),
        ("133;D;-1", Some(CommandStatusUpdate::Exit(-1))),
        ("133;D", Some(CommandStatusUpdate::Unknown)),
    ] {
        let mut update = None;
        filter.write(
            format!("\x1b]{payload}\x07").as_bytes(),
            EngineKnobs::default(),
            &mut terminal,
            &mut Vec::new(),
            &mut None,
            &mut update,
        );
        assert_eq!(update, expected, "{payload}");
        if let Some(value) = update {
            status = value.code();
        }
        if expected.is_none() {
            assert_eq!(status, Some(7), "{payload}");
        }
    }
    assert_eq!(status, None);
}

#[test]
fn engine_filter_reads_command_status_across_writes() {
    let bytes = b"\x1b]133;D;7\x1b\\";
    for split in 1..bytes.len() {
        let mut terminal = new_terminal(20, 4, 16).expect("terminal");
        let mut filter = EngineFilter::default();
        let mut status = None;
        for chunk in [&bytes[..split], &bytes[split..]] {
            filter.write(
                chunk,
                EngineKnobs::default(),
                &mut terminal,
                &mut Vec::new(),
                &mut None,
                &mut status,
            );
        }
        assert_eq!(
            status,
            Some(CommandStatusUpdate::Exit(7)),
            "split at {split}"
        );
    }
}

#[test]
fn engine_filter_keeps_the_pins_osc_133_command_marks_and_private_modes() {
    let mut terminal = new_terminal(20, 4, 16).expect("terminal");
    let mut filter = EngineFilter::default();
    let mut write = |filter: &mut EngineFilter, bytes: &[u8]| {
        if !bytes.is_empty() {
            filter.count_output();
        }
        filter.write(
            bytes,
            EngineKnobs::default(),
            &mut terminal,
            &mut Vec::new(),
            &mut None,
            &mut None,
        );
        filter.facts(&terminal).expect("facts")
    };
    let fresh = write(&mut filter, b"");
    assert_eq!(fresh.output, PaneOutputFacts::default());
    assert_eq!(fresh.private_modes, 0b1_0100);
    let prompt = write(&mut filter, b"\x1b]133;A\x07$ ");
    assert_eq!(prompt.output.output_generation, 1);
    assert_ne!(prompt.output.last_output_time, 0);
    assert_ne!(prompt.output.last_prompt_time, 0);
    assert_eq!(prompt.output.command_start_time, 0);
    let running = write(&mut filter, b"\x1b]133;C\x07\x1b[?2004h\x1b[?1h");
    assert!(running.output.command_running);
    assert_eq!(running.output.command_status, None);
    assert_eq!(running.output.output_generation, 2);
    assert_eq!(running.private_modes, 0b1000_0001_0101);
    let blinking = write(&mut filter, b"\x1b[?12h");
    assert_eq!(blinking.private_modes, 0b1000_0001_1101);
    let still = write(&mut filter, b"\x1b[?12l");
    assert_eq!(still.private_modes, 0b1000_0001_0101);
    write(&mut filter, b"\x1b[?12h");
    let reset = write(&mut filter, b"\x1b[0 q");
    assert_eq!(reset.private_modes, 0b1000_0001_0101);
    write(&mut filter, b"\x1b[?12h");
    let bare = write(&mut filter, b"\x1b[ q");
    assert_eq!(bare.private_modes, 0b1000_0001_0101);
    write(&mut filter, b"\x1b[?12h");
    let steady = write(&mut filter, b"\x1b[2 q");
    assert_eq!(steady.private_modes & (1 << 3), 0);
    let blinking_block = write(&mut filter, b"\x1b[1 q");
    assert_ne!(blinking_block.private_modes & (1 << 3), 0);
    write(&mut filter, b"\x1b[0 q");
    let standard = write(&mut filter, b"\x1b[?1000h\x1b[?1002h");
    assert_eq!(standard.private_modes, 0b1000_0101_0101);
    let any = write(&mut filter, b"\x1b[?1003h\x1b[?1006h");
    assert_eq!(any.private_modes, 0b1100_1001_0101);
    let cleared = write(&mut filter, b"\x1b[?1000l");
    assert_eq!(cleared.private_modes, 0b1100_0001_0101);
    let normal = write(&mut filter, b"\x1b[?1000h\x1b[?1006l");
    assert_eq!(normal.private_modes, 0b1000_0011_0101);
    let reset = write(&mut filter, b"\x1bc");
    assert_eq!(reset.private_modes & 0b1110_0000, 0);
    for (mark, status) in [
        (&b"D"[..], 0),
        (b"D;3", 3),
        (b"D;256", 255),
        (b"D;x", 255),
        (b"D;3;aid=1", 3),
        (b"D;aid=1", 0),
        (b"D;", 0),
    ] {
        let mut bytes = b"\x1b]133;".to_vec();
        bytes.extend_from_slice(mark);
        bytes.push(0x07);
        let finished = write(&mut filter, &bytes);
        assert!(!finished.output.command_running);
        assert_eq!(
            finished.output.command_status,
            Some(status),
            "{}",
            String::from_utf8_lossy(mark)
        );
        assert_ne!(finished.output.command_end_time, 0);
    }
}

#[test]
fn pane_output_counts_bytes_the_passthrough_filter_drops() {
    let mut terminal = new_terminal(20, 4, 16).expect("terminal");
    let mut passthrough = PassthroughFilter::default();
    let mut filter = EngineFilter::default();
    let mut engine = EngineOutput {
        filter: &mut filter,
        knobs: EngineKnobs::default(),
        renames: &mut Vec::new(),
        bar: &mut None,
        last_command_status: &mut None,
    };
    feed_pty_output(
        &mut terminal,
        &mut passthrough,
        &mut engine,
        b"\x1bPtmux;\x1b\x1b]52;c;eA==\x07\x1b\\",
    );
    let facts = filter.facts(&terminal).expect("facts");
    assert_eq!(facts.output.output_generation, 1);
    assert_ne!(facts.output.last_output_time, 0);
}

#[test]
fn engine_filter_counts_title_writes_the_shell_integration_did_not_mark() {
    let long_title = format!("\x1b]2;{}\x07", "t".repeat(200));
    let bytes = [
        b"\x1b]2626\x07\x1b]2;bash\x07".as_slice(),
        b"\x1b]2;vim\x1b\\",
        b"\x1b]0;both\x07",
        b"\x1b]1;icon\x07\x1b]22;text\x07",
        b"\x1b]2626\x07\x1b]7;file://host/tmp\x07\x1b]2;after-cwd\x07",
        long_title.as_bytes(),
        b"\x1b]2626\x07\x1b]2;ls -la\x07",
    ]
    .concat();
    for split in 1..bytes.len() {
        let mut terminal = new_terminal(20, 4, 16).expect("terminal");
        let mut filter = EngineFilter::default();
        for chunk in [&bytes[..split], &bytes[split..]] {
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
            filter.facts(&terminal).expect("facts").program_title_writes,
            4,
            "split at {split}"
        );
        assert_eq!(terminal.title().expect("title"), "ls -la");
    }
}

#[cfg(unix)]
#[test]
fn progress_bar_is_visible_on_the_frame_carrying_the_osc() {
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            shell: Some("/bin/sh".to_owned()),
            command: Some(vec![
                "stty -echo; printf READY; while read -r state; do printf '\\033]9;4;%s\\007\\033[2J\\033[Hprogress:%s' \"$state\" \"$state\"; done"
                    .to_owned(),
            ]),
            ..TerminalSpawn::default()
        },
    );
    session.set_preview_watch(true);
    wait_for_test_viewport(&session, |viewport| {
        let mut contents = String::new();
        for cell in viewport.cells.iter() {
            viewport.push_glyph(*cell, &mut contents);
        }
        contents.contains("READY")
    });
    let events = session.events();
    while events.try_recv().is_ok() {}

    for (digit, expected) in [
        (3, ProgressBarState::Indeterminate),
        (0, ProgressBarState::Hidden),
        (3, ProgressBarState::Indeterminate),
        (0, ProgressBarState::Hidden),
    ] {
        session.send_text(format!("{digit}\n").as_str());
        let marker = format!("progress:{digit}");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            assert!(Instant::now() < deadline, "missing frame for {marker}");
            if let Ok(TerminalEvent::ViewportReady { .. }) = events.try_recv() {
                let latest = session.latest.read();
                let viewport = &latest.fallback;
                let mut contents = String::new();
                for cell in viewport.cells.iter() {
                    viewport.push_glyph(*cell, &mut contents);
                }
                if contents.contains(&marker) {
                    assert_eq!(latest.bar.state, expected);
                    break;
                }
            } else {
                thread::yield_now();
            }
        }
    }
}

#[test]
fn engine_filter_answers_and_keeps_program_status() {
    let effects = Rc::new(RefCell::new(PtyEffects::new()));
    let sink = Rc::clone(&effects);
    let mut terminal = new_terminal(20, 4, 16).expect("terminal");
    terminal
        .on_pty_write(move |_, bytes| sink.borrow_mut().push(bytes))
        .expect("pty write");
    register_device_attributes(&mut terminal).expect("device attributes");
    let mut write = |filter: &mut EngineFilter, bytes: &[u8]| {
        filter.write(
            bytes,
            EngineKnobs::default(),
            &mut terminal,
            &mut Vec::new(),
            &mut None,
            &mut None,
        );
    };
    let mut filter = EngineFilter {
        replies: Some(Rc::clone(&effects)),
        ..EngineFilter::default()
    };

    write(&mut filter, b"\x1b]7501;?\x1b\\\x1b[c");
    let replies = std::mem::take(&mut effects.borrow_mut().bytes);
    assert!(
        replies.starts_with(b"\x1b]7501;?\x1b\\\x1b[?"),
        "{:?}",
        String::from_utf8_lossy(&replies)
    );
    write(&mut filter, b"\x1b]7501;?\x07");
    assert_eq!(effects.borrow().bytes, b"\x1b]7501;?\x07");
    assert!(filter.take_program_status().is_none());

    write(&mut filter, b"\x1b]7501;state=working:id=sync:msg=U3lu");
    assert!(filter.take_program_status().is_none());
    write(&mut filter, b"Y2luZyBwaG90b3M=\x1b\\");
    let record = filter
        .take_program_status()
        .and_then(|status| status.headline())
        .expect("split report");
    assert_eq!(
        (record.id.as_str(), record.state, record.message.as_str()),
        ("sync", crate::ProgramState::Working, "Syncing photos")
    );

    let long = format!("\x1b]7501;state=working:msg={}\x07", "eHh4".repeat(500));
    write(&mut filter, long.as_bytes());
    let record = filter
        .take_program_status()
        .and_then(|status| status.headline())
        .expect("long report");
    assert_eq!(record.message, "xxx".repeat(500));

    write(
        &mut filter,
        format!(
            "\x1b]7501;state=done:msg=RG9uZQ\x07\x1b]7501;state=working:id=w\x07\x1b]133;A;aid={}\x07",
            "x".repeat(80)
        )
        .as_bytes(),
    );
    let status = filter.take_program_status().expect("prompt");
    assert_eq!(
        status
            .records()
            .iter()
            .map(|record| (record.state, record.message.as_str()))
            .collect::<Vec<_>>(),
        [(crate::ProgramState::Done, "Done")]
    );

    let padded = format!("\x1b]7501;{}state=clear\x07", "\t".repeat(4100));
    write(&mut filter, padded.as_bytes());
    assert!(filter.take_program_status().is_none());
    write(&mut filter, b"\x1b]7501;\t\tstate=clear\x07");
    let status = filter.take_program_status().expect("clear");
    assert!(status.records().is_empty());

    write(&mut filter, b"text\x1bcmore");
    let status = filter.take_program_status().expect("reset");
    assert!(status.records().is_empty() && !status.reported());

    let mut quiet = EngineFilter::default();
    write(&mut quiet, b"\x1b]7501;?\x07\x1b]7501;state=idle\x07");
    assert_eq!(effects.borrow().bytes, b"\x1b]7501;?\x07");
    assert!(quiet.take_program_status().is_some());
}

#[test]
fn engine_filter_holds_the_pinned_progress_bar_edges() {
    let normal = ProgressBar {
        state: ProgressBarState::Normal,
        progress: 50,
    };
    let indeterminate = ProgressBar {
        state: ProgressBarState::Indeterminate,
        progress: 50,
    };
    assert_eq!(progress_after(&["9;4;1;50"]), normal);
    assert_eq!(progress_after(&["9;4;1;50", "9;4;3;70"]), indeterminate);
    for ignored in ["9;4;5", "9;4;1;101", "9;4", "9;4;", "9;5;1", "9;4;1;5x"] {
        assert_eq!(
            progress_after(&["9;4;1;50", "9;4;3;70", ignored]),
            indeterminate,
            "{ignored} moved the progress bar"
        );
    }
    assert_eq!(
        progress_after(&["9;4;1;50", "9;4;3;70", "9;4;2;"]),
        ProgressBar {
            state: ProgressBarState::Error,
            progress: 50,
        }
    );
    assert_eq!(
        progress_after(&["9;4;1;100"]),
        ProgressBar {
            state: ProgressBarState::Normal,
            progress: 100,
        }
    );
}

#[test]
fn engine_filter_commits_an_osc_on_can_sub_and_drops_control_bytes() {
    let (_, _, bar) = engine_filter_run(EngineKnobs::default(), &[b"\x1b]9;4;1;50\x18".as_slice()]);
    assert_eq!(
        bar,
        ProgressBar {
            state: ProgressBarState::Normal,
            progress: 50,
        }
    );
    let (_, _, bar) = engine_filter_run(
        EngineKnobs::default(),
        &[b"\x1b]9;4;1;50\x18".as_slice(), b"\x1b]9;4;2;20\x1a"],
    );
    assert_eq!(
        bar,
        ProgressBar {
            state: ProgressBarState::Error,
            progress: 20,
        }
    );
    let (text, _, bar) = engine_filter_run(
        EngineKnobs::default(),
        &[b"\x1b]9;4;1;\x0077\x07z".as_slice()],
    );
    assert_eq!(
        bar,
        ProgressBar {
            state: ProgressBarState::Normal,
            progress: 77,
        }
    );
    assert!(text.starts_with('z'), "{text:?}");
}

#[test]
fn engine_filter_passes_an_osc_through_in_any_shape() {
    let (text, _, bar) = engine_filter_run(
        EngineKnobs::default(),
        &[b"\x1b]0;ti".as_slice(), b"tle\x1b\\a\x1b]9;4", b";1;7\x07b"],
    );
    assert_eq!(text, "ab\n\n\n");
    assert_eq!(
        bar,
        ProgressBar {
            state: ProgressBarState::Normal,
            progress: 7,
        }
    );
}

#[test]
fn engine_filter_rename_follows_the_pin_table() {
    let knobs = EngineKnobs {
        allow_rename: true,
        ..EngineKnobs::default()
    };
    let (text, renames) = engine_filter_screen(
        knobs,
        &[
            b"\x1bkESCCSI\x1b[0m",
            b"\x1bkCAN\x18",
            b"\x1bkA\x07B\x1b\\",
            b"\x1b",
            b"kSPLIT",
            b"\x1b",
            b"\\",
            b"\x1bkQUIET\x1b\\",
        ],
    );
    assert_eq!(renames, ["ESCCSI", "CAN", "AB", "SPLIT", "QUIET"]);
    assert_eq!(text.trim(), "");
    let (_, refused) = engine_filter_screen(EngineKnobs::default(), &[b"\x1bkNAME\x1b\\"]);
    assert!(
        refused.is_empty(),
        "allow-rename off still renamed: {refused:?}"
    );
}

#[test]
fn engine_filter_hands_an_aborted_csi_to_the_engine() {
    let (text, _) = engine_filter_screen(EngineKnobs::default(), &[b"hello \x1b[2\x1b[2Jworld"]);
    assert!(!text.contains("2J"), "the aborted CSI leaked: {text:?}");
    assert!(text.contains("world"), "{text:?}");
}

#[test]
fn engine_filter_passes_split_sequences_through_untouched() {
    let mut terminal = new_terminal(20, 4, 16).expect("terminal");
    let mut filter = EngineFilter::default();
    let mut renames = Vec::new();
    for chunk in [
        b"a\x1b[".as_slice(),
        b"1",
        b"mb\x1b[0m",
        b"c\x1b",
        b"[2",
        b"Cd",
    ] {
        filter.write(
            chunk,
            EngineKnobs::default(),
            &mut terminal,
            &mut renames,
            &mut None,
            &mut None,
        );
    }
    assert_eq!(terminal.cursor_x().expect("cursor"), 6);
    let revision = ModeRevision::capture(&terminal).expect("revision");
    assert_eq!(revision.capture_rows(0, 0, false, false), "abcd");
    assert!(revision.cell_matches_text(PointCoordinate { x: 5, y: 0 }, "d"));
}

#[test]
fn engine_filter_scrolls_a_split_clear_into_history() {
    let (text, _) = engine_filter_screen(
        EngineKnobs::default(),
        &[b"one\r\ntwo\r\n", b"\x1b[H\x1b[", b"2J", b"three"],
    );
    assert_eq!(text, "one\ntwo\nthree\n\n\n");
    let (kept, _) = engine_filter_screen(
        EngineKnobs {
            scroll_on_clear: false,
            ..EngineKnobs::default()
        },
        &[b"one\r\ntwo\r\n", b"\x1b[H\x1b[2Jthree"],
    );
    assert_eq!(kept.lines().next().unwrap_or(""), "three", "{kept:?}");
    assert!(!kept.contains("one"), "{kept:?}");
}

#[test]
#[ignore = "throughput measurement; run with --ignored --nocapture"]
fn engine_filter_throughput() {
    use std::time::Instant;
    let mut coloured = Vec::new();
    while coloured.len() < 26 << 20 {
        coloured.extend_from_slice(
            b"\x1b[0m\x1b[1;34mdrwxr-xr-x\x1b[0m  12 user staff   384 Sep  2 07:00 \x1b[1;36msome-directory\x1b[0m\r\n",
        );
    }
    let mut plain = Vec::new();
    while plain.len() < 26 << 20 {
        plain.extend_from_slice(b"the quick brown fox jumps over the lazy dog 0123456789\r\n");
    }
    let mut clears = Vec::new();
    while clears.len() < 512 << 10 {
        clears.extend_from_slice(b"\x1b[H\x1b[2J");
    }
    let mut fire = Vec::new();
    let mut seed = 0x2545_f491_u32;
    while fire.len() < 26 << 20 {
        fire.extend_from_slice(b"\x1b[H");
        for _ in 0..50 {
            for _ in 0..200 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                if !seed.is_multiple_of(4) {
                    fire.extend_from_slice(
                        format!(
                            "\x1b[48;5;{}m\x1b[38;5;{}m",
                            16 + seed % 216,
                            16 + (seed >> 8) % 216
                        )
                        .as_bytes(),
                    );
                }
                fire.extend_from_slice("\u{2580}".as_bytes());
            }
            fire.extend_from_slice(b"\r\n");
        }
    }
    for (name, input, chunk_size) in [
        ("coloured", &coloured, 65536),
        ("plain", &plain, 65536),
        ("clears", &clears, 65536),
        ("fire", &fire, 65536),
        ("fire", &fire, 1024),
    ] {
        for round in 0..3 {
            let mut terminal = new_terminal(200, 50, 1000).expect("terminal");
            let started = Instant::now();
            for chunk in input.chunks(chunk_size) {
                terminal.vt_write(chunk);
            }
            let direct = started.elapsed();
            let mut terminal = new_terminal(200, 50, 1000).expect("terminal");
            let mut passthrough = PassthroughFilter::default();
            let started = Instant::now();
            for chunk in input.chunks(chunk_size) {
                passthrough.write(chunk, |bytes| terminal.vt_write(bytes));
            }
            let base = started.elapsed();
            let mut terminal = new_terminal(200, 50, 1000).expect("terminal");
            let mut passthrough = PassthroughFilter::default();
            let mut filter = EngineFilter::default();
            let mut renames = Vec::new();
            let mut bar = None;
            let mut engine = EngineOutput {
                filter: &mut filter,
                knobs: EngineKnobs::default(),
                renames: &mut renames,
                bar: &mut bar,
                last_command_status: &mut None,
            };
            let started = Instant::now();
            for chunk in input.chunks(chunk_size) {
                feed_pty_output(&mut terminal, &mut passthrough, &mut engine, chunk);
            }
            let filtered = started.elapsed();
            println!(
                "{name}/{chunk_size} round {round}: direct {direct:?} passthrough {base:?} filtered {filtered:?}"
            );
        }
    }
}

fn filter_passthrough(mode: AllowPassthrough, chunks: &[&[u8]]) -> Vec<u8> {
    let mut filter = PassthroughFilter::default();
    filter.set_mode(mode);
    let mut output = Vec::new();
    for chunk in chunks {
        filter.write(chunk, |bytes| output.extend_from_slice(bytes));
    }
    output
}

#[test]
fn passthrough_fast_path_is_one_borrowed_write_without_allocation() {
    let input = b"ordinary terminal output without escapes";
    let mut filter = PassthroughFilter::default();
    let mut calls = 0;
    let written = filter.write(input, |output| {
        calls += 1;
        assert!(std::ptr::eq(output.as_ptr(), input.as_ptr()));
    });
    assert_eq!(written, input.len());
    assert_eq!(calls, 1);
    assert_eq!(filter.payload.capacity(), 0);
    assert_eq!(filter.state, PassthroughState::Ground);
}

#[test]
fn passthrough_hands_sequences_over_in_one_write() {
    let input = b"\x1b[38;5;1m\xe2\x96\x80\x1b[48;5;2m\x1b]0;title\x07\x1bP1$r\x1b\\\x1b[0m done";
    let mut filter = PassthroughFilter::default();
    let mut calls = 0;
    let written = filter.write(input, |output| {
        calls += 1;
        assert!(std::ptr::eq(output.as_ptr(), input.as_ptr()));
        assert_eq!(output.len(), input.len());
    });
    assert_eq!(written, input.len());
    assert_eq!(calls, 1);
}

#[test]
fn untracked_csi_finals_need_no_bookkeeping() {
    for final_byte in 0x40..=0x7e_u8 {
        if csi_is_tracked(final_byte) {
            continue;
        }
        for parameters in [
            &b""[..],
            b"?1049",
            b"2",
            b"?1000;1006",
            b"0 ",
            b">1",
            b"!",
            b"22",
        ] {
            assert!(!csi_touches_metadata(parameters, final_byte));
            assert!(!csi_needs_rewrite(
                parameters,
                final_byte,
                EngineKnobs::default()
            ));
        }
    }
}

#[test]
fn tmux_passthrough_unwraps_split_kitty_bytes_once() {
    let output = filter_passthrough(
        AllowPassthrough::All,
        &[
            b"\x1bPt",
            b"mux;\x1b\x1b_Ga=T,f=24,s=1,v=1,i=42;/wAA\x1b",
            b"\x1b\\\x1b\\",
        ],
    );
    assert_eq!(output, b"\x1b_Ga=T,f=24,s=1,v=1,i=42;/wAA\x1b\\");
}

#[test]
fn tmux_passthrough_off_consumes_the_complete_wrapper() {
    let output = filter_passthrough(
        AllowPassthrough::Off,
        &[b"before\x1bPtm", b"ux;hidden\x1b", b"\\after"],
    );
    assert_eq!(output, b"beforeafter");
}

#[test]
fn tmux_passthrough_un_doubles_payload_escapes() {
    let output = filter_passthrough(
        AllowPassthrough::All,
        &[b"\x1bPtmux;one\x1b", b"\x1b[31mtwo\x1b\\"],
    );
    assert_eq!(output, b"one\x1b[31mtwo");
}

#[test]
fn tmux_passthrough_drops_a_single_dcs_escape_byte() {
    let output = filter_passthrough(AllowPassthrough::All, &[b"\x1bPtmux;one\x1b[31mtwo\x1b\\"]);
    assert_eq!(output, b"one[31mtwo");
}

#[test]
fn oversized_tmux_passthrough_is_discarded_bounded_and_released() {
    for (name, mode) in [
        ("off", AllowPassthrough::Off),
        ("on", AllowPassthrough::All),
        ("all", AllowPassthrough::All),
    ] {
        let mut filter = PassthroughFilter::default();
        filter.set_mode(mode);
        let mut output = Vec::new();
        filter.write(TMUX_PASSTHROUGH_PREFIX, |bytes| {
            output.extend_from_slice(bytes);
        });
        let payload = vec![b'x'; MAX_TMUX_PASSTHROUGH_PAYLOAD_BYTES];
        filter.write(&payload, |bytes| output.extend_from_slice(bytes));
        assert_eq!(filter.payload.len(), MAX_TMUX_PASSTHROUGH_PAYLOAD_BYTES);
        filter.write(b"y", |bytes| output.extend_from_slice(bytes));
        assert!(
            matches!(filter.state, PassthroughState::Discard { .. }),
            "{name}"
        );
        assert_eq!(filter.payload.capacity(), 0, "{name}");
        filter.write(b"still hidden", |bytes| output.extend_from_slice(bytes));
        assert!(output.is_empty(), "{name}");
        filter.write(b"\x1b\\after", |bytes| output.extend_from_slice(bytes));
        assert_eq!(output, b"after", "{name}");
        assert_eq!(filter.state, PassthroughState::Ground, "{name}");
        assert_eq!(filter.payload.capacity(), 0, "{name}");
    }
}

#[test]
fn non_tmux_sixel_and_control_dcs_pass_through_in_every_effective_mode() {
    for input in [
        b"\x1bPq#0;2;0;0;0~\x1b\\".as_slice(),
        b"\x1bP1000pbegin %1 1 0\x1b\\".as_slice(),
    ] {
        for (name, mode) in [
            ("off", AllowPassthrough::Off),
            ("on", AllowPassthrough::All),
            ("all", AllowPassthrough::All),
        ] {
            assert_eq!(filter_passthrough(mode, &[input]), input, "{name}");
            for split in 0..=input.len() {
                assert_eq!(
                    filter_passthrough(mode, &[&input[..split], &input[split..]]),
                    input,
                    "{name} split={split}"
                );
            }
        }
    }
}

#[test]
fn clearing_the_whole_screen_keeps_history_where_the_pin_scrolls_into_it() {
    let mut terminal = new_terminal(20, 5, 1 << 16).expect("terminal");
    for line in 1..=12 {
        terminal.vt_write(format!("L{line:02}\r\n").as_bytes());
    }
    assert_eq!(terminal.scrollback_rows().expect("scrollback"), 8);

    terminal.vt_write(b"\x1b[H\x1b[2J");

    assert_eq!(
        terminal.scrollback_rows().expect("scrollback"),
        8,
        "pinned tmux answers history_size 12 here with scroll-on-clear on and 8 with it off; zz is unconditionally the off case"
    );
}
