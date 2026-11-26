use super::*;

#[test]
fn search_stepping_clamps_when_wrapping_is_disabled() {
    let matches = vec![
        SearchMatch {
            row: 0,
            end_row: 0,
            start: 0,
            end: 1,
        },
        SearchMatch {
            row: 1,
            end_row: 1,
            start: 0,
            end: 1,
        },
        SearchMatch {
            row: 2,
            end_row: 2,
            start: 0,
            end: 1,
        },
    ];
    let mut search = Some(Box::new(SearchState {
        matches,
        current: Some(2),
        ..SearchState::default()
    }));

    step_search(&mut search, 1, false);
    assert_eq!(search.as_ref().and_then(|search| search.current), Some(2));
    step_search(&mut search, -4, false);
    assert_eq!(search.as_ref().and_then(|search| search.current), Some(0));
    step_search(&mut search, -1, true);
    assert_eq!(search.as_ref().and_then(|search| search.current), Some(2));
}

#[test]
fn incremental_search_reuses_its_control_record() {
    let mut slot = SearchSlot::default();
    store_search_state(
        &mut slot,
        SearchState {
            query: SearchQuery::literal("first"),
            ..SearchState::default()
        },
    );
    let address = std::ptr::from_ref(slot.as_deref().expect("first state"));

    store_search_state(
        &mut slot,
        SearchState {
            query: SearchQuery::literal("second"),
            request_id: 2,
            ..SearchState::default()
        },
    );

    assert_eq!(
        std::ptr::from_ref(slot.as_deref().expect("updated state")),
        address
    );
    assert_eq!(slot.as_deref().expect("updated state").query.text, "second");
}

#[test]
fn history_search_maps_matches_back_to_cells() {
    let mut terminal = new_terminal(12, 2, 32).expect("terminal");
    terminal.vt_write(b"alpha\r\nbeta target\r\ngamma\r\n");
    let search = search_history(&terminal, "target").expect("search");
    assert_eq!(search.matches.len(), 1);
    assert_eq!(search.matches[0].start, 5);
    assert_eq!(search.matches[0].end, 11);
    assert_eq!(std::mem::size_of::<SearchCellOffset>(), 12);
    assert_eq!(std::mem::align_of::<SearchCellOffset>(), 4);
}

#[test]
fn history_snapshot_reuses_oversized_grapheme_scratch_across_rows() {
    let mut terminal = new_terminal(4, 2, 16).expect("terminal");
    let cluster = format!("a{}", "\u{301}".repeat(12));
    terminal.vt_write(format!("{cluster}\r\n{cluster}").as_bytes());

    let mut text = String::new();
    let mut offsets = Vec::new();
    let mut grapheme_scratch = Vec::new();
    append_history_row(
        &terminal,
        0,
        4,
        &mut text,
        &mut offsets,
        &mut grapheme_scratch,
    )
    .expect("first history row");
    assert!(grapheme_scratch.len() > 8);
    let allocation = grapheme_scratch.as_ptr();
    let capacity = grapheme_scratch.capacity();

    append_history_row(
        &terminal,
        1,
        4,
        &mut text,
        &mut offsets,
        &mut grapheme_scratch,
    )
    .expect("second history row");
    assert_eq!(grapheme_scratch.as_ptr(), allocation);
    assert_eq!(grapheme_scratch.capacity(), capacity);
    assert_eq!(text.matches('a').count(), 2);
}

#[test]
fn history_search_supports_regex_case_modes_and_cancellation() {
    let mut terminal = new_terminal(16, 3, 16).expect("terminal");
    terminal.vt_write(b"Alpha 123\r\nalpha 456\r\nomega");
    let snapshot = HistorySearchSnapshot::capture(&terminal).expect("search snapshot");
    let mut query = SearchQuery {
        text: r"alpha \d+".to_owned(),
        mode: SearchMode::Regex,
        case: SearchCase::Smart,
        direction: SearchDirection::Forward,
    };
    let smart = snapshot
        .search(&query, 1, || false)
        .expect("smart-case search");
    assert_eq!(smart.matches.len(), 2);

    query.case = SearchCase::Sensitive;
    let sensitive = snapshot
        .search(&query, 2, || false)
        .expect("case-sensitive search");
    assert_eq!(sensitive.matches.len(), 1);
    assert_eq!(sensitive.matches[0].row, 1);

    query.text = "(".to_owned();
    let invalid = snapshot
        .search(&query, 3, || false)
        .expect("invalid regex result");
    assert!(invalid.invalid_pattern);
    assert!(invalid.matches.is_empty());
    assert!(snapshot.search(&query, 4, || true).is_none());
}

#[test]
fn incremental_history_search_reuses_match_storage() {
    let mut terminal = new_terminal(16, 2, 16).expect("terminal");
    terminal.vt_write(b"target target\r\ntarget");
    let snapshot = HistorySearchSnapshot::capture(&terminal).expect("snapshot");
    let query = SearchQuery::literal("target");
    let mut scratch = Vec::with_capacity(16);
    let allocation = scratch.as_ptr();
    let capacity = scratch.capacity();

    let first = snapshot
        .search_reusing(&query, 1, &mut scratch, || false)
        .expect("first result");
    assert_eq!(first.matches.len(), 3);
    assert_eq!(first.matches.as_ptr(), allocation);
    assert_eq!(first.matches.capacity(), capacity);

    scratch = first.matches;
    let second = snapshot
        .search_reusing(&query, 2, &mut scratch, || false)
        .expect("second result");
    assert_eq!(second.matches.as_ptr(), allocation);
    assert_eq!(second.matches.capacity(), capacity);

    scratch = second.matches;
    assert!(
        snapshot
            .search_reusing(&query, 3, &mut scratch, || true)
            .is_none()
    );
    assert_eq!(scratch.as_ptr(), allocation);
    assert_eq!(scratch.capacity(), capacity);
}

#[test]
fn completed_search_replaces_stale_pending_result_and_recycles_its_storage() {
    let (results, result_rx) = crossbeam_channel::bounded(1);
    let discard_results = result_rx.clone();
    let view = TerminalViewId(1);
    let mut stale_matches = Vec::with_capacity(16);
    stale_matches.push(SearchMatch {
        row: 1,
        end_row: 1,
        start: 2,
        end: 3,
    });
    let stale_allocation = stale_matches.as_ptr();
    let stale_capacity = stale_matches.capacity();
    let mut pending = SearchResults::default();
    pending.by_view.insert(
        view,
        SearchResult {
            request_id: 1,
            view_id: view,
            screen: Screen::Primary,
            state: SearchState {
                query: SearchQuery::literal("stale"),
                matches: stale_matches,
                request_id: 1,
                ..SearchState::default()
            },
        },
    );
    results.try_send(pending).expect("initial pending result");

    let latest_match = SearchMatch {
        row: 4,
        end_row: 4,
        start: 5,
        end: 6,
    };
    let mut latest = SearchResults::default();
    latest.by_view.insert(
        view,
        SearchResult {
            request_id: 2,
            view_id: view,
            screen: Screen::Primary,
            state: SearchState {
                query: SearchQuery::literal("latest"),
                matches: vec![latest_match],
                request_id: 2,
                ..SearchState::default()
            },
        },
    );
    let mut match_scratch = Vec::new();
    assert!(send_latest_search_results(
        &results,
        &discard_results,
        latest,
        &mut match_scratch,
    ));

    let mut received = result_rx.try_recv().expect("latest pending result");
    let received = received.by_view.remove(&view).expect("view result");
    assert_eq!(received.request_id, 2);
    assert_eq!(received.state.matches, [latest_match]);
    assert_eq!(match_scratch.as_ptr(), stale_allocation);
    assert_eq!(match_scratch.capacity(), stale_capacity);
}

#[test]
fn completed_search_coalescing_preserves_results_for_other_views() {
    let (results, result_rx) = crossbeam_channel::bounded(1);
    let discard_results = result_rx.clone();
    let first = TerminalViewId(1);
    let second = TerminalViewId(2);
    let result = |view_id, request_id| SearchResult {
        request_id,
        view_id,
        screen: Screen::Primary,
        state: SearchState {
            query: SearchQuery::literal(format!("view-{}", view_id.0)),
            request_id,
            ..SearchState::default()
        },
    };
    let mut pending = SearchResults::default();
    pending.by_view.insert(first, result(first, 1));
    results.try_send(pending).expect("first view result");

    let mut latest = SearchResults::default();
    latest.by_view.insert(second, result(second, 2));
    assert!(send_latest_search_results(
        &results,
        &discard_results,
        latest,
        &mut Vec::new(),
    ));

    let received = result_rx.try_recv().expect("coalesced view results");
    assert_eq!(received.by_view.len(), 2);
    assert_eq!(received.by_view[&first].request_id, 1);
    assert_eq!(received.by_view[&second].request_id, 2);
}

#[test]
fn cancelling_queued_search_retains_its_match_storage() {
    let (mut worker, _results) = SearchWorker::spawn(ActorWake::none());
    let matches = Vec::with_capacity(16);
    let allocation = matches.as_ptr();
    let capacity = matches.capacity();
    let view = TerminalViewId(1);
    let (request_id, latest_request) = worker.next_request(view);
    let job = SearchJob {
        request_id,
        view_id: view,
        screen: Screen::Primary,
        query: SearchQuery::literal("queued"),
        snapshot: Arc::new(
            HistorySearchSnapshot::capture(&new_terminal(1, 1, 1).expect("terminal"))
                .expect("snapshot"),
        ),
        selection: SearchSelectionPolicy::Last,
        match_scratch: matches,
        latest_request,
    };
    worker
        .jobs
        .try_send(SearchJobs {
            by_view: HashMap::from([(view, job)]),
        })
        .expect("queued search");

    assert!(worker.cancel(view) > request_id);
    assert_eq!(worker.match_scratch.as_ptr(), allocation);
    assert_eq!(worker.match_scratch.capacity(), capacity);
}

#[test]
fn search_match_mapping_uses_ordered_utf8_cell_offsets() {
    let offsets = [
        SearchCellOffset {
            start: 0,
            end: 1,
            column: 0,
            width: 1,
        },
        SearchCellOffset {
            start: 1,
            end: 4,
            column: 1,
            width: 2,
        },
        SearchCellOffset {
            start: 4,
            end: 5,
            column: 3,
            width: 1,
        },
    ];

    assert_eq!(search_match_span(&offsets, 0, 5, 4), Some((0, 4)));
    assert_eq!(search_match_span(&offsets, 2, 3, 4), Some((1, 3)));
    assert_eq!(search_match_span(&offsets, 4, 5, 4), Some((3, 4)));
    assert_eq!(search_match_span(&offsets, 1, 1, 4), None);
    assert_eq!(search_match_span(&offsets, 5, 6, 4), None);
}

#[test]
fn copy_mode_search_selects_relative_to_cursor_and_wraps() {
    let matches = vec![
        SearchMatch {
            row: 0,
            end_row: 0,
            start: 1,
            end: 4,
        },
        SearchMatch {
            row: 1,
            end_row: 1,
            start: 2,
            end: 5,
        },
        SearchMatch {
            row: 2,
            end_row: 2,
            start: 3,
            end: 6,
        },
    ];
    let mut search = SearchState {
        matches: matches.clone(),
        ..SearchState::default()
    };

    select_search_result(
        &mut search,
        SearchSelectionPolicy::From {
            point: PointCoordinate { x: 1, y: 0 },
            direction: SearchDirection::Forward,
        },
    );
    assert_eq!(search.current, Some(1));

    search.current = None;
    select_search_result(
        &mut search,
        SearchSelectionPolicy::From {
            point: PointCoordinate { x: 1, y: 0 },
            direction: SearchDirection::Backward,
        },
    );
    assert_eq!(search.current, Some(2));

    search.current = None;
    select_search_result(
        &mut search,
        SearchSelectionPolicy::From {
            point: PointCoordinate {
                x: u16::MAX,
                y: u32::MAX,
            },
            direction: SearchDirection::Forward,
        },
    );
    assert_eq!(search.current, Some(0));

    search.matches = matches;
    search.current = None;
    select_search_result(
        &mut search,
        SearchSelectionPolicy::From {
            point: PointCoordinate { x: 0, y: 0 },
            direction: SearchDirection::Backward,
        },
    );
    assert_eq!(search.current, Some(2));
}

#[test]
fn stale_background_search_results_cannot_mutate_a_newer_view_request() {
    let mut terminal = new_terminal(12, 2, 16).expect("terminal");
    terminal.vt_write(b"target\r\nother");
    let snapshot = HistorySearchSnapshot::capture(&terminal).expect("snapshot");
    let query = SearchQuery::literal("target");
    let view_id = TerminalViewId(9);
    let mut view = TerminalViewState::for_screen(Screen::Primary);
    store_search_state(
        &mut view.search,
        SearchState {
            query: query.clone(),
            request_id: 2,
            pending: true,
            ..SearchState::default()
        },
    );
    let mut active = ActiveTerminalViews::from([(view_id, Box::new(view))]);
    let mut inactive = InactiveTerminalViews::new();
    let (mut worker, _results) = SearchWorker::spawn(ActorWake::none());
    worker
        .latest_requests
        .insert(view_id, Arc::new(AtomicU64::new(2)));
    worker.next_request = 2;

    let stale = snapshot.search(&query, 1, || false).expect("stale result");
    let stale_allocation = stale.matches.as_ptr();
    let stale_capacity = stale.matches.capacity();
    assert!(
        !apply_search_result(
            &mut terminal,
            &mut active,
            &mut inactive,
            &mut worker,
            SearchResult {
                request_id: 1,
                view_id,
                screen: Screen::Primary,
                state: stale,
            },
        )
        .expect("reject stale")
    );
    let pending = active
        .get(&view_id)
        .expect("active")
        .search
        .as_ref()
        .expect("pending search");
    assert_eq!(pending.request_id, 2);
    assert!(pending.matches.is_empty());
    assert_eq!(worker.match_scratch.as_ptr(), stale_allocation);
    assert_eq!(worker.match_scratch.capacity(), stale_capacity);

    let mut fresh = snapshot.search(&query, 2, || false).expect("fresh result");
    select_search_result(&mut fresh, SearchSelectionPolicy::Last);
    assert!(
        apply_search_result(
            &mut terminal,
            &mut active,
            &mut inactive,
            &mut worker,
            SearchResult {
                request_id: 2,
                view_id,
                screen: Screen::Primary,
                state: fresh,
            },
        )
        .expect("accept fresh")
    );
    let search = active
        .get(&view_id)
        .expect("active")
        .search
        .as_ref()
        .expect("completed search");
    assert!(!search.pending);
    assert_eq!(search.matches.len(), 1);
    assert_eq!(worker.match_scratch.as_ptr(), stale_allocation);
    assert_eq!(worker.match_scratch.capacity(), stale_capacity);
}

#[test]
fn current_search_result_for_a_detached_view_recycles_its_match_storage() {
    let mut terminal = new_terminal(80, 24, 16).expect("terminal");
    let mut active = ActiveTerminalViews::new();
    let mut inactive = InactiveTerminalViews::new();
    let view_id = TerminalViewId(99);
    let (mut worker, _results) = SearchWorker::spawn(ActorWake::none());
    worker
        .latest_requests
        .insert(view_id, Arc::new(AtomicU64::new(1)));
    worker.next_request = 1;
    let mut matches = Vec::with_capacity(16);
    matches.push(SearchMatch {
        row: 0,
        end_row: 0,
        start: 0,
        end: 1,
    });
    let allocation = matches.as_ptr();
    let capacity = matches.capacity();

    assert!(
        !apply_search_result(
            &mut terminal,
            &mut active,
            &mut inactive,
            &mut worker,
            SearchResult {
                request_id: 1,
                view_id,
                screen: Screen::Primary,
                state: SearchState {
                    query: SearchQuery::literal("orphaned"),
                    matches,
                    request_id: 1,
                    ..SearchState::default()
                },
            },
        )
        .expect("reject detached result")
    );
    assert_eq!(worker.match_scratch.as_ptr(), allocation);
    assert_eq!(worker.match_scratch.capacity(), capacity);
}
