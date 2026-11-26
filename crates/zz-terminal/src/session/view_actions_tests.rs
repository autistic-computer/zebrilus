use super::test_support::*;
use super::*;

#[test]
fn view_action_size_limits_are_nonfatal() {
    assert!(matches!(
        normalize_view_action_result(Err(WorkerError::SearchSnapshotTooLarge)),
        Ok(ViewActionResult::Snapshot)
    ));
    assert!(matches!(
        normalize_view_action_result(Err(WorkerError::Thread("stopped".to_owned()))),
        Err(WorkerError::Thread(message)) if message == "stopped"
    ));
}

#[test]
fn single_click_stays_hidden_until_selection_crosses_a_cell() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"hello");
    let mut selection = None;
    let pointer = |column| PointerCellEvent {
        column,
        row: 0,
        click_count: 1,
        rectangle: false,
    };
    selection_press(
        &terminal,
        &mut selection,
        pointer(1),
        &WordSeparators::default(),
    )
    .expect("anchor selection");
    assert_eq!(format_selection_text(&terminal).expect("copy"), "");

    let mut render_state = RenderState::new().expect("render state");
    let mut rows = RowIterator::new().expect("rows");
    let mut cells = CellIterator::new().expect("cells");
    let mut generations = ViewportGenerations::default();
    let mut dictionary = ViewportDictionary::default();
    let viewport = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("snapshot");
    assert!(viewport.overlays.is_empty());

    selection_drag(
        &terminal,
        &mut selection,
        pointer(1),
        &WordSeparators::default(),
    )
    .expect("drag inside anchor cell");
    assert_eq!(format_selection_text(&terminal).expect("copy"), "");

    selection_drag(
        &terminal,
        &mut selection,
        pointer(3),
        &WordSeparators::default(),
    )
    .expect("extend selection");
    assert_eq!(format_selection_text(&terminal).expect("copy"), "ell");
    let moved = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Overlay,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("selection-only snapshot");
    assert!(Arc::ptr_eq(&viewport.cells, &moved.cells));
    assert_eq!(moved.generation, viewport.generation);
    assert_eq!(moved.view_generation, viewport.view_generation + 1);
    assert_eq!(
        moved.overlays.as_ref(),
        [OverlaySpan::new(0, 1, 4, OverlayKind::Selection)]
    );
    assert!(dictionary.cell_pool.is_empty());

    let scratch_allocation = dictionary.overlay_scratch.as_ptr();
    let scratch_capacity = dictionary.overlay_scratch.capacity();
    let unchanged = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::View,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("unchanged overlay snapshot");
    assert!(Arc::ptr_eq(&moved.overlays, &unchanged.overlays));
    assert!(Arc::ptr_eq(&moved.cells, &unchanged.cells));
    assert_eq!(dictionary.overlay_scratch.as_ptr(), scratch_allocation);
    assert_eq!(dictionary.overlay_scratch.capacity(), scratch_capacity);
}

#[test]
fn out_of_range_pointer_selection_clamps_and_pty_free_worker_survives() {
    let mut terminal = new_terminal(3, 2, 4).expect("terminal");
    terminal.vt_write(b"abc\r\ndef");
    let event = PointerCellEvent {
        column: u16::MAX,
        row: u16::MAX,
        click_count: 1,
        rectangle: false,
    };
    let Point::Viewport(point) = viewport_point(&terminal, event).expect("clamped point") else {
        panic!("pointer selection must remain viewport-relative");
    };
    assert_eq!(point, PointCoordinate { x: 2, y: 1 });
    let mut selection = None;
    selection_press(&terminal, &mut selection, event, &WordSeparators::default())
        .expect("clamped selection");
    assert!(selection.is_some());

    let session = TerminalSession::spawn_output_view("selection".to_owned(), "abcdef".to_owned());
    let view = TerminalViewId(78);
    session.resize(3, 2, 8, 18);
    attach_streaming(&session, view);
    let before = wait_for_test_viewport(&session, |viewport| {
        viewport.columns == 3
            && viewport.rows == 2
            && matches!(viewport.status, SessionStatus::Running)
    });
    session.view_action(view, TerminalViewAction::SelectionPress(event));
    let selected = wait_for_test_viewport(&session, |viewport| {
        viewport.view_generation > before.view_generation
    });
    assert!(matches!(selected.status, SessionStatus::Running));
}

#[test]
fn out_of_space_selection_reinstall_clears_pending_selection_without_error() {
    let mut terminal = new_terminal(3, 2, 4).expect("terminal");
    terminal.vt_write(b"abc");
    let mut view = TerminalViewState::for_screen(Screen::Primary);
    selection_press(
        &terminal,
        &mut view.selection,
        PointerCellEvent {
            column: 0,
            row: 0,
            click_count: 1,
            rectangle: false,
        },
        &WordSeparators::default(),
    )
    .expect("selection fixture");
    assert!(view.selection.is_some());

    finish_view_selection_install(
        &terminal,
        &mut view,
        Err(WorkerError::Ghostty(libghostty_vt::Error::OutOfSpace {
            required: 1,
        })),
    )
    .expect("selection reinstall failure is nonfatal");
    assert!(view.selection.is_none());
}

#[test]
fn desktop_word_selection_uses_the_compiled_tmux_boundaries() {
    let mut terminal = new_terminal(16, 2, 16).expect("terminal");
    terminal.vt_write(b"foo.bar baz");
    let mut selection = None;
    let event = PointerCellEvent {
        column: 1,
        row: 0,
        click_count: 2,
        rectangle: false,
    };

    selection_press(&terminal, &mut selection, event, &WordSeparators::default())
        .expect("default word selection");
    assert_eq!(
        format_selection_text(&terminal).expect("default copy"),
        "foo"
    );

    selection = None;
    terminal.set_selection(None).expect("clear selection");
    selection_press(&terminal, &mut selection, event, &WordSeparators::new(""))
        .expect("whitespace-only word selection");
    assert_eq!(
        format_selection_text(&terminal).expect("whitespace-only copy"),
        "foo.bar"
    );

    let mut adjacent = new_terminal(16, 2, 16).expect("adjacent terminal");
    adjacent.vt_write(b"foo. bar");
    let mut adjacent_selection = None;
    selection_press(
        &adjacent,
        &mut adjacent_selection,
        PointerCellEvent { column: 3, ..event },
        &WordSeparators::default(),
    )
    .expect("separator selection");
    assert_eq!(
        format_selection_text(&adjacent).expect("separator copy"),
        "."
    );
    selection_press(
        &adjacent,
        &mut adjacent_selection,
        PointerCellEvent { column: 4, ..event },
        &WordSeparators::default(),
    )
    .expect("whitespace selection");
    assert_eq!(
        format_selection_text(&adjacent).expect("space copy"),
        "",
        "libghostty intentionally does not select an unstyled empty-space cell"
    );
}

#[test]
fn client_views_restore_isolated_viewport_and_selection_state() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"zero\r\none\r\ntwo\r\nthree");

    let first_id = TerminalViewId(1);
    let second_id = TerminalViewId(2);
    let word_separators = WordSeparators::default();
    let mut active = ActiveTerminalViews::new();
    let mut inactive = InactiveTerminalViews::new();
    activate_view(
        &mut terminal,
        first_id,
        &mut active,
        &mut inactive,
        &word_separators,
    )
    .expect("activate first view");
    terminal.scroll_viewport(ScrollViewport::Top);
    let first = active.get_mut(&first_id).expect("first active");
    selection_press(
        &terminal,
        &mut first.selection,
        PointerCellEvent {
            column: 0,
            row: 0,
            click_count: 1,
            rectangle: false,
        },
        &word_separators,
    )
    .expect("select in first view");
    selection_drag(
        &terminal,
        &mut first.selection,
        PointerCellEvent {
            column: 1,
            row: 0,
            click_count: 1,
            rectangle: false,
        },
        &word_separators,
    )
    .expect("extend selection in first view");
    first.unseen_output = 7;
    sync_viewport_anchor(&terminal, first).expect("pin first viewport");
    let first_offset = terminal.scrollbar().expect("scrollbar").offset;
    let first_address = std::ptr::from_ref(active[&first_id].as_ref());
    assert_eq!(
        format_selection_text(&terminal).expect("first selection"),
        "ze"
    );

    activate_view(
        &mut terminal,
        second_id,
        &mut active,
        &mut inactive,
        &word_separators,
    )
    .expect("activate second view");
    assert_eq!(active.len(), 2, "activating a view must not park its peers");
    let second_scrollbar = terminal.scrollbar().expect("second scrollbar");
    assert_eq!(
        second_scrollbar.offset.saturating_add(second_scrollbar.len),
        second_scrollbar.total
    );
    assert_eq!(
        format_selection_text(&terminal).expect("second selection"),
        ""
    );

    restore_view_state(
        &mut terminal,
        active.get_mut(&first_id).expect("first remains active"),
        &word_separators,
    )
    .expect("restore first view");
    assert_eq!(
        terminal.scrollbar().expect("restored scrollbar").offset,
        first_offset
    );
    assert_eq!(
        format_selection_text(&terminal).expect("restored selection"),
        "ze"
    );
    assert_eq!(active[&first_id].unseen_output, 7);
    assert_eq!(
        std::ptr::from_ref(active[&first_id].as_ref()),
        first_address,
        "plural activation must preserve the boxed view state"
    );

    deactivate_view(
        &mut terminal,
        second_id,
        &mut active,
        &mut inactive,
        &word_separators,
    )
    .expect("park second view");
    assert!(
        release_view(&mut terminal, first_id, &mut active, &mut inactive)
            .expect("release first view")
    );
    assert!(active.is_empty());
    assert!(!inactive.contains_key(&first_id));
    assert!(inactive.contains_key(&second_id));
    assert!(
        !release_view(&mut terminal, second_id, &mut active, &mut inactive)
            .expect("release parked view")
    );
    assert!(inactive.is_empty());
    assert!(
        !release_view(&mut terminal, first_id, &mut active, &mut inactive)
            .expect("release unknown view")
    );
}

#[test]
fn primary_and_alternate_screens_keep_independent_native_view_state() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"zero\r\none\r\ntwo\r\nthree");
    let mut view = TerminalViewState::for_screen(Screen::Primary);
    let word_separators = WordSeparators::default();
    terminal.scroll_viewport(ScrollViewport::Top);
    selection_press(
        &terminal,
        &mut view.selection,
        PointerCellEvent {
            column: 0,
            row: 0,
            click_count: 1,
            rectangle: false,
        },
        &word_separators,
    )
    .expect("primary selection");
    selection_drag(
        &terminal,
        &mut view.selection,
        PointerCellEvent {
            column: 1,
            row: 0,
            click_count: 1,
            rectangle: false,
        },
        &word_separators,
    )
    .expect("extend primary selection");
    store_search_state(
        &mut view.search,
        SearchState {
            query: SearchQuery::literal("zero"),
            ..SearchState::default()
        },
    );
    sync_viewport_anchor(&terminal, &mut view).expect("primary anchor");
    view.unseen_output = 5;
    let primary_offset = terminal.scrollbar().expect("primary scrollbar").offset;

    terminal.vt_write(b"\x1b[?1049hALT");
    view.note_output(terminal.active_screen().expect("alternate screen"));
    assert!(
        reconcile_view_screen(&mut terminal, &mut view, &word_separators).expect("enter alternate")
    );
    assert_eq!(view.screen, Screen::Alternate);
    assert!(view.selection.is_none());
    assert!(view.search.is_none());
    assert_eq!(
        format_selection_text(&terminal).expect("alternate selection"),
        ""
    );

    terminal.vt_write(b"\x1b[?1049l");
    assert!(
        reconcile_view_screen(&mut terminal, &mut view, &word_separators).expect("restore primary")
    );
    assert_eq!(view.screen, Screen::Primary);
    assert_eq!(
        terminal.scrollbar().expect("restored scrollbar").offset,
        primary_offset
    );
    assert_eq!(
        format_selection_text(&terminal).expect("primary restored"),
        "ze"
    );
    assert_eq!(
        view.search.as_ref().expect("primary search").query.text,
        "zero"
    );
    assert_eq!(view.unseen_output, 5);
}

#[test]
fn select_all_covers_retained_history_without_trailing_blank_rows() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"old\r\nmid\r\nnew");
    let mut selection = None;
    select_all_history(&terminal, &mut selection).expect("select all");
    assert_eq!(
        format_selection_text(&terminal).expect("copy all"),
        "old\nmid\nnew"
    );
}

#[test]
fn history_limit_counts_retained_lines() {
    let retained = |limit: usize| {
        let mut terminal = new_terminal(80, 24, limit).expect("terminal");
        let mut output = Vec::new();
        for line in 0..30_000 {
            output.extend_from_slice(format!("line {line}\r\n").as_bytes());
        }
        terminal.vt_write(&output);
        terminal.scrollback_rows().expect("history")
    };
    let default_limit = retained(10_000);
    let small_limit = retained(2_000);
    assert!(
        (9_000..=10_000).contains(&default_limit),
        "history-limit 10000 kept {default_limit} rows"
    );
    assert!(
        (1_000..=2_000).contains(&small_limit),
        "history-limit 2000 kept {small_limit} rows"
    );
}

#[test]
fn xterm_history_erase_keeps_visible_content() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"old\r\nvisible\r\nlast");
    assert!(terminal.scrollback_rows().expect("history") > 0);
    terminal.vt_write(b"\x1b[3J");
    assert_eq!(terminal.scrollback_rows().expect("cleared history"), 0);
    assert_eq!(
        capture_terminal(&terminal, None, CaptureOptions::default()).expect("visible"),
        "visible\nlast"
    );
}

#[test]
fn osc8_hover_uses_terminal_link_metadata_and_contiguous_span() {
    let mut terminal = new_terminal(12, 2, 16).expect("terminal");
    terminal.vt_write(b"\x1b]8;;https://example.com/docs\x1b\\link\x1b]8;;\x1b\\ plain");
    let mut scratch = Vec::with_capacity(LINK_URI_SCRATCH_BYTES);
    let allocation = scratch.as_ptr();
    let capacity = scratch.capacity();

    let link = hover_link_at(
        &terminal,
        PointerCellEvent {
            column: 2,
            row: 0,
            click_count: 1,
            rectangle: false,
        },
        &mut scratch,
        &HashSet::new(),
    )
    .expect("hover lookup")
    .expect("OSC 8 link");
    assert_eq!(link.uri, "https://example.com/docs");
    assert_eq!((link.row, link.start, link.end), (0, 0, 4));
    assert_eq!(
        hover_link_at(
            &terminal,
            PointerCellEvent {
                column: 5,
                row: 0,
                click_count: 1,
                rectangle: false,
            },
            &mut scratch,
            &HashSet::new(),
        )
        .expect("plain cell lookup"),
        None
    );
    assert_eq!(scratch.as_ptr(), allocation);
    assert_eq!(scratch.capacity(), capacity);
    let viewport = snapshot_fixture(&terminal);
    assert!(
        viewport
            .style(viewport.cell(0, 0).expect("link cell"))
            .expect("link style")
            .hyperlink()
    );
}

#[test]
fn osc8_hover_rejects_active_content_schemes() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"\x1b]8;;javascript:alert(1)\x1b\\bad\x1b]8;;\x1b\\");
    let mut scratch = Vec::with_capacity(LINK_URI_SCRATCH_BYTES);

    assert_eq!(
        hover_link_at(
            &terminal,
            PointerCellEvent {
                column: 1,
                row: 0,
                click_count: 1,
                rectangle: false,
            },
            &mut scratch,
            &HashSet::new(),
        )
        .expect("unsafe lookup"),
        None
    );
}

#[test]
fn plain_uri_hover_trims_wrappers_and_trailing_punctuation() {
    let mut terminal = new_terminal(40, 1, 16).expect("terminal");
    let uri = "https://example.com/docs";
    terminal.vt_write(format!("({uri}), after").as_bytes());
    let mut scratch = Vec::new();

    let link = hover_link_at(
        &terminal,
        PointerCellEvent {
            column: 10,
            row: 0,
            click_count: 1,
            rectangle: false,
        },
        &mut scratch,
        &HashSet::new(),
    )
    .expect("plain URI lookup")
    .expect("plain URI");
    assert_eq!(link.uri, uri);
    assert_eq!(link.start, 1);
    assert_eq!(usize::from(link.end), 1 + uri.len());
}

#[test]
fn image_placeholder_hover_spans_the_bracketed_text_and_carries_the_number() {
    let mut terminal = new_terminal(40, 1, 16).expect("terminal");
    terminal.vt_write(b"read [Image #12] now");
    let mut scratch = Vec::new();
    let bound = HashSet::from([12]);

    let link = hover_link_at(
        &terminal,
        PointerCellEvent {
            column: 11,
            row: 0,
            click_count: 1,
            rectangle: false,
        },
        &mut scratch,
        &bound,
    )
    .expect("placeholder lookup")
    .expect("placeholder");
    assert_eq!(link.uri, "zz-image://12");
    assert_eq!((link.row, link.start, link.end), (0, 5, 16));

    for column in [4, 16] {
        assert_eq!(
            hover_link_at(
                &terminal,
                PointerCellEvent {
                    column,
                    row: 0,
                    click_count: 1,
                    rectangle: false,
                },
                &mut scratch,
                &bound,
            )
            .expect("outside lookup"),
            None,
            "column {column} sits outside the placeholder"
        );
    }
}

#[test]
fn image_placeholder_hover_ignores_other_bracketed_text() {
    let mut terminal = new_terminal(40, 2, 16).expect("terminal");
    terminal.vt_write(b"[Pasted text #1]\r\n[Image #]");
    let mut scratch = Vec::new();

    for row in [0, 1] {
        assert_eq!(
            hover_link_at(
                &terminal,
                PointerCellEvent {
                    column: 3,
                    row,
                    click_count: 1,
                    rectangle: false,
                },
                &mut scratch,
                &HashSet::new(),
            )
            .expect("non-placeholder lookup"),
            None,
            "row {row} is not an image placeholder"
        );
    }
}

#[test]
fn pending_paste_baseline_ignores_existing_placeholder_occurrences() {
    let mut terminal = new_terminal(40, 3, 16).expect("terminal");
    terminal.vt_write(b"[Image #1]");
    let now = Instant::now();
    let mut bindings = PastedImageBindings::default();

    bindings.open(&terminal, 7, now).expect("open window");

    assert_eq!(bindings.observe(&terminal).expect("observe baseline"), None);
    assert!(bindings.bound_numbers().is_empty());
}

#[test]
fn pending_paste_binds_the_highest_new_placeholder_number() {
    let mut terminal = new_terminal(40, 3, 16).expect("terminal");
    let mut bindings = PastedImageBindings::default();
    bindings
        .open(&terminal, 8, Instant::now())
        .expect("open window");

    terminal.vt_write(b"[Image #2] [Image #5]");

    assert_eq!(bindings.observe(&terminal).expect("observe"), Some((8, 5)));
    assert_eq!(bindings.bound_numbers(), &HashSet::from([5]));
}

#[test]
fn pending_paste_rebinds_when_an_existing_number_count_increases() {
    let mut terminal = new_terminal(40, 3, 16).expect("terminal");
    terminal.vt_write(b"[Image #1]");
    let mut bindings = PastedImageBindings::default();
    bindings
        .open(&terminal, 9, Instant::now())
        .expect("open window");

    terminal.vt_write(b"\r\n[Image #1]");

    assert_eq!(bindings.observe(&terminal).expect("observe"), Some((9, 1)));
}

#[test]
fn pending_paste_counts_the_live_grid_even_when_a_viewport_is_scrolled_back() {
    let mut terminal = new_terminal(40, 2, 16).expect("terminal");
    terminal.vt_write(b"old\r\nlive\r\n[Image #7]");
    terminal.scroll_viewport(ScrollViewport::Top);
    let mut bindings = PastedImageBindings::default();
    bindings
        .open(&terminal, 70, Instant::now())
        .expect("open window");

    terminal.vt_write(b"\r\n[Image #7]");

    assert_eq!(bindings.observe(&terminal).expect("observe"), Some((70, 7)));
}

#[test]
fn placeholder_output_outside_a_pending_window_never_binds() {
    let mut terminal = new_terminal(40, 2, 16).expect("terminal");
    terminal.vt_write(b"response echoes [Image #2]");
    let mut bindings = PastedImageBindings::default();

    assert_eq!(bindings.observe(&terminal).expect("observe"), None);
    assert!(bindings.bound_numbers().is_empty());
}

#[test]
fn pending_paste_expiry_returns_its_token_without_binding() {
    let terminal = new_terminal(40, 2, 16).expect("terminal");
    let now = Instant::now();
    let mut bindings = PastedImageBindings::default();
    bindings.open(&terminal, 10, now).expect("open window");
    let deadline = now + PENDING_PASTE_WINDOW;

    assert!(
        bindings
            .expire(
                deadline
                    .checked_sub(Duration::from_millis(1))
                    .expect("one millisecond before the deadline"),
            )
            .is_empty()
    );
    assert_eq!(bindings.expire(deadline), [10]);
    assert!(bindings.bound_numbers().is_empty());
}

#[test]
fn unbinding_a_pasted_image_removes_its_link() {
    let mut terminal = new_terminal(40, 2, 16).expect("terminal");
    let mut bindings = PastedImageBindings::default();
    bindings
        .open(&terminal, 11, Instant::now())
        .expect("open window");
    terminal.vt_write(b"[Image #3]");
    assert_eq!(bindings.observe(&terminal).expect("observe"), Some((11, 3)));
    let point = PointerCellEvent {
        column: 4,
        row: 0,
        click_count: 1,
        rectangle: false,
    };
    assert!(
        image_placeholder_at(&terminal, point, 40, bindings.bound_numbers())
            .expect("bound lookup")
            .is_some()
    );

    assert!(bindings.unbind(3));

    assert_eq!(
        image_placeholder_at(&terminal, point, 40, bindings.bound_numbers())
            .expect("unbound lookup"),
        None
    );
}

#[test]
fn plain_uri_hover_rejects_unsafe_and_overlong_tokens() {
    let mut unsafe_terminal = new_terminal(32, 1, 16).expect("terminal");
    unsafe_terminal.vt_write(b"javascript:alert(1)");
    let mut scratch = Vec::new();
    let point = PointerCellEvent {
        column: 5,
        row: 0,
        click_count: 1,
        rectangle: false,
    };
    assert_eq!(
        hover_link_at(&unsafe_terminal, point, &mut scratch, &HashSet::new())
            .expect("unsafe lookup"),
        None
    );

    let columns = u16::try_from(MAX_LINK_URI_BYTES + 2).expect("bounded terminal width");
    let mut long_terminal = new_terminal(columns, 1, 16).expect("terminal");
    long_terminal.vt_write(format!("https://{}", "a".repeat(MAX_LINK_URI_BYTES)).as_bytes());
    assert_eq!(
        hover_link_at(&long_terminal, point, &mut scratch, &HashSet::new()).expect("long lookup"),
        None
    );
    assert!(scratch.len() <= MAX_LINK_URI_BYTES);
}

#[test]
fn unmodified_motion_hovers_only_bound_image_placeholders() {
    let mut terminal = new_terminal(40, 2, 16).expect("terminal");
    terminal.vt_write(b"read [Image #4]");
    let input = TerminalMouseInput::new(
        TerminalMousePhase::Motion,
        None,
        PointerCellEvent {
            column: 10,
            row: 0,
            click_count: 1,
            rectangle: false,
        },
        81,
        1,
        320,
        36,
        8,
        18,
        crate::Modifiers::default(),
        false,
    );
    let mut selection = None;
    let mut hover_link = None;
    let mut writer: Box<dyn Write + Send> = Box::new(std::io::sink());
    let mut encoder = mouse::Encoder::new().expect("mouse encoder");
    let mut event = mouse::Event::new().expect("mouse event");
    let mut button_pressed = false;
    let mut scratch = Vec::new();

    let result = route_mouse_input(
        &mut terminal,
        &mut selection,
        &mut hover_link,
        None,
        input,
        &mut writer,
        &mut encoder,
        &mut event,
        &mut button_pressed,
        &mut scratch,
        &WordSeparators::default(),
        &HashSet::from([4]),
    )
    .expect("bound motion");

    assert!(matches!(result, ViewActionResult::OverlaySnapshot));
    assert_eq!(hover_link.expect("bound hover").uri, "zz-image://4");

    let mut hover_link = None;
    let result = route_mouse_input(
        &mut terminal,
        &mut selection,
        &mut hover_link,
        None,
        input,
        &mut writer,
        &mut encoder,
        &mut event,
        &mut button_pressed,
        &mut scratch,
        &WordSeparators::default(),
        &HashSet::new(),
    )
    .expect("unbound motion");
    assert!(matches!(result, ViewActionResult::None));
    assert!(hover_link.is_none());

    let modified = TerminalMouseInput::new(
        TerminalMousePhase::Motion,
        None,
        input.cell,
        81,
        1,
        320,
        36,
        8,
        18,
        crate::Modifiers::new(false, false, true, false),
        false,
    );
    let result = route_mouse_input(
        &mut terminal,
        &mut selection,
        &mut hover_link,
        None,
        modified,
        &mut writer,
        &mut encoder,
        &mut event,
        &mut button_pressed,
        &mut scratch,
        &WordSeparators::default(),
        &HashSet::from([4]),
    )
    .expect("modified motion");
    assert!(matches!(result, ViewActionResult::None));
    assert!(hover_link.is_none());
}

#[test]
fn unmodified_image_hover_survives_alternate_screen_mouse_passthrough() {
    let mut terminal = new_terminal(40, 2, 16).expect("terminal");
    terminal.vt_write(b"\x1b[?1049h\x1b[?1000h\x1b[?1006h[Image #6]");
    let input = TerminalMouseInput::new(
        TerminalMousePhase::Motion,
        None,
        PointerCellEvent {
            column: 5,
            row: 0,
            click_count: 1,
            rectangle: false,
        },
        41,
        1,
        320,
        36,
        8,
        18,
        crate::Modifiers::default(),
        false,
    );
    let mut selection = None;
    let mut hover_link = None;
    let mut writer: Box<dyn Write + Send> = Box::new(std::io::sink());
    let mut encoder = mouse::Encoder::new().expect("mouse encoder");
    let mut event = mouse::Event::new().expect("mouse event");
    let mut button_pressed = false;
    let mut scratch = Vec::new();

    let result = route_mouse_input(
        &mut terminal,
        &mut selection,
        &mut hover_link,
        None,
        input,
        &mut writer,
        &mut encoder,
        &mut event,
        &mut button_pressed,
        &mut scratch,
        &WordSeparators::default(),
        &HashSet::from([6]),
    )
    .expect("mouse-tracking motion");

    assert!(matches!(result, ViewActionResult::OverlaySnapshot));
    assert_eq!(hover_link.expect("alternate hover").uri, "zz-image://6");
}

#[test]
fn wheel_routing_prioritizes_application_mouse_then_alternate_scroll() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    assert_eq!(
        wheel_route(&terminal, false).expect("primary route"),
        WheelRoute::Viewport
    );

    terminal.vt_write(b"\x1b[?1007h\x1b[?1049h");
    assert_eq!(
        wheel_route(&terminal, false).expect("alternate route"),
        WheelRoute::AlternateScroll
    );

    terminal.vt_write(b"\x1b[?1000h");
    assert_eq!(
        wheel_route(&terminal, false).expect("application mouse route"),
        WheelRoute::ApplicationMouse
    );
    assert_eq!(
        wheel_route(&terminal, true).expect("forced local route"),
        WheelRoute::Viewport
    );

    terminal.vt_write(b"\x1b[?1000l\x1b[?1007l");
    assert_eq!(
        wheel_route(&terminal, false).expect("disabled alternate scroll"),
        WheelRoute::Viewport
    );
}

#[test]
fn a_grid_without_history_reports_nothing_to_scroll_into() {
    let mut terminal = new_terminal(8, 3, 32).expect("terminal");
    terminal.vt_write(b"zero\r\none\r\ntwo");
    let scrollbar = terminal.scrollbar().expect("scrollbar");
    assert_eq!(scrollbar.total, scrollbar.len);

    terminal.vt_write(b"\r\nthree\r\nfour");
    let scrollbar = terminal.scrollbar().expect("scrollbar");
    assert!(scrollbar.total > scrollbar.len);

    terminal.vt_write(b"\x1b[?1049h");
    let scrollbar = terminal.scrollbar().expect("alternate scrollbar");
    assert_eq!(scrollbar.total, scrollbar.len);
}

#[test]
fn alternate_scroll_emits_bounded_normal_and_application_cursor_keys() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"\x1b[?1049h");
    let mut output = Vec::new();

    write_alternate_scroll(&terminal, -2, &mut output).expect("normal cursor up");
    write_alternate_scroll(&terminal, 2, &mut output).expect("normal cursor down");
    assert_eq!(output, b"\x1b[A\x1b[A\x1b[B\x1b[B");

    terminal.vt_write(b"\x1b[?1h");
    output.clear();
    write_alternate_scroll(&terminal, -1, &mut output).expect("application cursor up");
    write_alternate_scroll(&terminal, 1, &mut output).expect("application cursor down");
    assert_eq!(output, b"\x1bOA\x1bOB");

    output.clear();
    write_alternate_scroll(&terminal, i32::MIN, &mut output).expect("bounded repeat");
    assert_eq!(
        output.len(),
        usize::try_from(MAX_WHEEL_REPEAT).unwrap_or(0) * 3
    );
    assert!(output.as_chunks::<3>().0.iter().all(|s| s == b"\x1bOA"));
}
