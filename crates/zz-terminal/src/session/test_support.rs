use super::*;

pub(super) fn snapshot_fixture(terminal: &Terminal<'_, '_>) -> TerminalViewport {
    let mut render_state = RenderState::new().expect("render state");
    let mut rows = RowIterator::new().expect("rows");
    let mut cells = CellIterator::new().expect("cells");
    let mut generations = ViewportGenerations::default();
    let mut dictionary = ViewportDictionary::default();
    snapshot(
        terminal,
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
    .expect("snapshot")
}

pub(super) fn copy_mode_survives_downward_action(
    action: CopyModeAction,
    scroll_exit: bool,
    selected: bool,
    starts_at_bottom: bool,
) -> bool {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"zero\r\none\r\ntwo\r\nthree\r\nfour\r\nfive");
    let mut live_selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        scroll_exit,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let mode = copy_mode.as_mut().expect("mode");
    let maximum = mode.revision.maximum_offset();
    assert!(maximum > 0);
    if starts_at_bottom {
        assert_eq!(mode.viewport_offset, maximum);
    } else {
        let page = u32::from(mode.revision.viewport_rows.max(1));
        mode.viewport_offset = maximum - 1;
        mode.cursor.y = mode
            .viewport_offset
            .saturating_add(page.saturating_sub(1))
            .min(mode.revision.total_rows().saturating_sub(1));
    }
    if selected {
        mode.selection = Some(ModeSelection {
            anchor: mode.cursor,
            focus: mode.cursor,
            mode: SelectionMode::Cell,
            rectangle: false,
        });
        mode.selecting = true;
    }
    let mut unseen_output = 7;
    let result = apply_copy_mode_action(
        &mut terminal,
        &mut live_selection,
        &mut copy_mode,
        &mut unseen_output,
        action,
        &WordSeparators::default(),
        false,
    )
    .expect("copy mode action");
    assert!(matches!(result, ViewActionResult::Snapshot));
    let survived = copy_mode.is_some();
    assert_eq!(unseen_output, if survived { 7 } else { 0 });
    survived
}

pub(super) fn wait_for_test_capture(
    session: &TerminalSession,
    accept: impl Fn(&str) -> bool,
) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let capture = session
            .capture(CaptureOptions::default())
            .expect("capture the fed surface");
        if accept(&capture) {
            return capture;
        }
        assert!(
            Instant::now() < deadline,
            "capture never converged: {capture:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn attach_streaming(session: &TerminalSession, view: TerminalViewId) {
    session.attach_view(view);
    session.set_view_stream(view, ViewStream::Foreground);
}

pub(super) fn wait_for_test_viewport(
    session: &TerminalSession,
    mut predicate: impl FnMut(&TerminalViewport) -> bool,
) -> Arc<TerminalViewport> {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let viewport = session.latest_viewport();
        if predicate(&viewport) {
            return viewport;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for terminal viewport; last title: {:?}, mode: {:?}, background: {:?}, dictionary_generation: {}, status: {:?}",
            viewport.title(),
            viewport.mode,
            viewport.background,
            viewport.dictionary_generation,
            viewport.status,
        );
        thread::sleep(Duration::from_millis(10));
    }
}
