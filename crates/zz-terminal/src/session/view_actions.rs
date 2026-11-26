use super::*;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WheelRoute {
    ApplicationMouse,
    AlternateScroll,
    Viewport,
}

pub(super) enum ViewActionResult {
    None,
    Snapshot,
    OverlaySnapshot,
    ContentSnapshot,
    Copy(Box<TerminalCopyReady>),
    OpenUri(String),
}

pub(super) fn sync_viewport_anchor(
    terminal: &Terminal<'_, '_>,
    view: &mut TerminalViewState,
) -> Result<(), WorkerError> {
    if view.copy_mode.is_some() {
        return Ok(());
    }
    let scrollbar = terminal.scrollbar()?;
    if scrollbar.offset.saturating_add(scrollbar.len) >= scrollbar.total {
        view.viewport = ViewportAnchor::FollowBottom;
        view.unseen_output = 0;
    } else {
        view.viewport = ViewportAnchor::Pinned(
            terminal.track_grid_ref(Point::Viewport(PointCoordinate { x: 0, y: 0 }))?,
        );
    }
    Ok(())
}

pub(super) fn scroll_to_offset(
    terminal: &mut Terminal<'_, '_>,
    copy_mode: &mut CopyModeSlot,
    unseen_output: &mut u32,
    target: u32,
) -> Result<(), WorkerError> {
    if let Some(mode) = copy_mode.as_mut() {
        mode.viewport_offset = target.min(mode.revision.maximum_offset());
        return Ok(());
    }

    let scrollbar = terminal.scrollbar()?;
    let target = u64::from(target).min(scrollbar.total.saturating_sub(scrollbar.len));
    let delta = i128::from(target).saturating_sub(i128::from(scrollbar.offset));
    terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize_i128(delta)));
    let landed = terminal.scrollbar()?;
    if landed.offset.saturating_add(landed.len) >= landed.total {
        *unseen_output = 0;
    }
    Ok(())
}

pub(super) fn install_view_selection(
    terminal: &Terminal<'_, '_>,
    view: &mut TerminalViewState,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    let result = install_view_selection_inner(terminal, view, word_separators);
    finish_view_selection_install(terminal, view, result)
}

pub(super) fn finish_view_selection_install(
    terminal: &Terminal<'_, '_>,
    view: &mut TerminalViewState,
    result: Result<(), WorkerError>,
) -> Result<(), WorkerError> {
    if warn_and_skip_view_ghostty("selection reinstall", result)?.is_none() {
        view.selection = None;
        if let Err(error) = terminal.set_selection(None) {
            log::warn!("failed to clear invalid terminal selection: {error}");
        }
    }
    Ok(())
}

fn install_view_selection_inner(
    terminal: &Terminal<'_, '_>,
    view: &mut TerminalViewState,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    let Some(state) = view.selection.as_ref() else {
        terminal.set_selection(None)?;
        return Ok(());
    };
    let Some(anchor) = state.anchor.snapshot(terminal)? else {
        view.selection = None;
        terminal.set_selection(None)?;
        return Ok(());
    };
    let Some(focus) = state.focus.snapshot(terminal)? else {
        view.selection = None;
        terminal.set_selection(None)?;
        return Ok(());
    };
    let cell_selection_has_extent = if state.mode == SelectionMode::Cell {
        terminal
            .point_from_grid_ref(&anchor, PointSpace::Screen)?
            .zip(terminal.point_from_grid_ref(&focus, PointSpace::Screen)?)
            .is_some_and(|(anchor, focus)| anchor != focus)
    } else {
        true
    };
    let selection = match state.mode {
        SelectionMode::Cell => {
            cell_selection_has_extent.then(|| Selection::new(anchor, focus, state.rectangle))
        }
        SelectionMode::Word => {
            let start = select_native_word_between(terminal, &anchor, &focus, word_separators)?;
            let end = select_native_word_between(terminal, &focus, &anchor, word_separators)?;
            start
                .zip(end)
                .map(|(start, end)| Selection::new(start.start(), end.end(), false))
        }
        SelectionMode::Line => {
            let start = terminal.select_line(SelectLineOptions::new(anchor))?;
            let end = terminal.select_line(SelectLineOptions::new(focus))?;
            start
                .zip(end)
                .map(|(start, end)| Selection::new(start.start(), end.end(), false))
        }
    };
    terminal.set_selection(selection.as_ref())?;
    Ok(())
}

pub(super) fn restore_view_state(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    if warn_and_skip_view_ghostty(
        "view-state restore",
        restore_view_state_inner(terminal, view, word_separators),
    )?
    .is_none()
    {
        view.selection = None;
        view.viewport = ViewportAnchor::FollowBottom;
        terminal.scroll_viewport(ScrollViewport::Bottom);
        if let Err(error) = terminal.set_selection(None) {
            log::warn!("failed to clear selection after view-state restore error: {error}");
        }
    }
    Ok(())
}

fn restore_view_state_inner(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    if view.copy_mode.is_some() {
        terminal.set_selection(None)?;
        return Ok(());
    }
    match &view.viewport {
        ViewportAnchor::FollowBottom => terminal.scroll_viewport(ScrollViewport::Bottom),
        ViewportAnchor::Pinned(anchor) => {
            if let Some(point) = anchor.point(PointSpace::Screen)? {
                terminal.scroll_viewport(ScrollViewport::Top);
                terminal
                    .scroll_viewport(ScrollViewport::Delta(saturating_isize(i64::from(point.y))));
            } else {
                view.viewport = ViewportAnchor::FollowBottom;
                terminal.scroll_viewport(ScrollViewport::Bottom);
            }
        }
    }
    install_view_selection(terminal, view, word_separators)
}

pub(super) fn reconcile_view_screen(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
    word_separators: &WordSeparators,
) -> Result<bool, WorkerError> {
    Ok(warn_and_skip_view_ghostty(
        "view-screen reconciliation",
        reconcile_view_screen_inner(terminal, view, word_separators),
    )?
    .unwrap_or(false))
}

pub(super) fn note_output_and_revalidate_image_hover(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
    output_screen: Screen,
    word_separators: &WordSeparators,
    bound_pasted_images: &HashSet<u32>,
) -> Result<(), WorkerError> {
    let previous_screen = view.screen;
    let previous_hover = view
        .active()
        .hover_link
        .as_ref()
        .filter(|link| pasted_image_number(&link.uri).is_some())
        .cloned();
    view.note_output(output_screen);
    reconcile_view_screen(terminal, view, word_separators)?;
    let Some(previous_hover) = previous_hover.filter(|_| view.screen == previous_screen) else {
        return Ok(());
    };
    restore_view_state(terminal, view, word_separators)?;
    let current = image_placeholder_at(
        terminal,
        PointerCellEvent {
            column: previous_hover.start,
            row: previous_hover.row,
            click_count: 1,
            rectangle: false,
        },
        terminal.cols()?,
        bound_pasted_images,
    )?;
    if current.as_ref() == Some(&previous_hover) {
        view.active_mut().hover_link = current;
    }
    Ok(())
}

fn reconcile_view_screen_inner(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
    word_separators: &WordSeparators,
) -> Result<bool, WorkerError> {
    if view.copy_mode.is_some() {
        return Ok(false);
    }
    let screen = terminal.active_screen()?;
    if !view.switch_screen(screen) {
        return Ok(false);
    }
    terminal.set_selection(None)?;
    restore_view_state(terminal, view, word_separators)?;
    Ok(true)
}

pub(super) fn activate_view(
    terminal: &mut Terminal<'_, '_>,
    view_id: TerminalViewId,
    active: &mut ActiveTerminalViews,
    inactive: &mut InactiveTerminalViews,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    if active.contains_key(&view_id) {
        return Ok(());
    }

    let screen = terminal.active_screen()?;
    let mut state = inactive
        .remove(&view_id)
        .unwrap_or_else(|| Box::new(TerminalViewState::for_screen(screen)));
    state.switch_screen(screen);
    restore_view_state(terminal, &mut state, word_separators)?;
    refresh_frozen_view_appearance(terminal, &mut state)?;
    active.insert(view_id, state);
    Ok(())
}

pub(super) fn deactivate_view(
    terminal: &mut Terminal<'_, '_>,
    view_id: TerminalViewId,
    active: &mut ActiveTerminalViews,
    inactive: &mut InactiveTerminalViews,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    let Some(mut state) = active.remove(&view_id) else {
        return Ok(());
    };
    restore_view_state(terminal, &mut state, word_separators)?;
    sync_viewport_anchor(terminal, &mut state)?;
    terminal.set_selection(None)?;
    terminal.scroll_viewport(ScrollViewport::Bottom);
    inactive.insert(view_id, state);
    Ok(())
}

pub(super) fn release_view(
    terminal: &mut Terminal<'_, '_>,
    view_id: TerminalViewId,
    active: &mut ActiveTerminalViews,
    inactive: &mut InactiveTerminalViews,
) -> Result<bool, WorkerError> {
    inactive.remove(&view_id);
    if active.remove(&view_id).is_none() {
        return Ok(false);
    }
    terminal.set_selection(None)?;
    terminal.scroll_viewport(ScrollViewport::Bottom);
    Ok(true)
}

#[cfg(unix)]
pub(super) fn view_takes_direct_input(view: &TerminalViewState) -> bool {
    view.copy_mode.is_none()
        && view.selection.is_none()
        && view.search.is_none()
        && view.search_origin.is_none()
        && view.search_snapshot.is_none()
        && view.hover_link.is_none()
        && matches!(view.viewport, ViewportAnchor::FollowBottom)
        && view.unseen_output == 0
        && !view.mouse_button_pressed
}

pub(super) fn prepare_live_input(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
) -> Result<bool, WorkerError> {
    if view.copy_mode.is_some() {
        return Ok(false);
    }

    let scrollbar = terminal.scrollbar()?;
    let mut changed = scrollbar.offset.saturating_add(scrollbar.len) < scrollbar.total;
    if changed {
        terminal.scroll_viewport(ScrollViewport::Bottom);
    }
    if view.selection.take().is_some() {
        terminal.set_selection(None)?;
        changed = true;
    }
    if view.search.take().is_some() {
        changed = true;
    }
    view.search_origin = None;
    if view.hover_link.take().is_some() {
        changed = true;
    }
    view.search_snapshot = None;
    view.viewport = ViewportAnchor::FollowBottom;
    view.unseen_output = 0;
    view.mouse_button_pressed = false;
    Ok(changed)
}

pub(super) fn apply_view_action(
    terminal: &mut Terminal<'_, '_>,
    view_id: TerminalViewId,
    view: &mut TerminalViewState,
    action: TerminalViewAction,
    geometry: Geometry,
    writer: &mut dyn Write,
    mouse_encoder: &mut mouse::Encoder<'_>,
    mouse_event: &mut mouse::Event<'_>,
    input_bytes: &mut Vec<u8>,
    search_worker: &mut SearchWorker,
    wrap_search: bool,
    mode_keys_vi: bool,
    word_separators: &WordSeparators,
    bound_pasted_images: &HashSet<u32>,
    pending_copy_source: &mut Option<Box<CapturedCopySource>>,
    pane_search: &mut Option<CopyModeSearch>,
) -> Result<ViewActionResult, WorkerError> {
    let view_screen = view.screen;
    let TerminalScreenViewState {
        viewport: _,
        selection,
        copy_mode,
        search,
        search_origin,
        search_snapshot,
        hover_link,
        unseen_output,
        mouse_button_pressed,
    } = view.active_mut();
    let hover_cleared = !matches!(
        &action,
        TerminalViewAction::Mouse(_) | TerminalViewAction::CopySelection { .. }
    ) && hover_link.take().is_some();
    let result = match action {
        TerminalViewAction::ScrollWheel { lines, input } => {
            if let Some(mode) = copy_mode.as_mut() {
                scroll_copy_mode(mode, i64::from(lines));
                Ok(ViewActionResult::Snapshot)
            } else {
                let route = wheel_route(terminal, input.force_selection())?;
                match route {
                    WheelRoute::ApplicationMouse => {
                        let selection_changed = selection.take().is_some();
                        if selection_changed {
                            terminal.set_selection(None)?;
                        }
                        for _ in 0..lines.unsigned_abs().min(MAX_WHEEL_REPEAT) {
                            let _ = route_mouse_input(
                                terminal,
                                selection,
                                hover_link,
                                None,
                                input,
                                writer,
                                mouse_encoder,
                                mouse_event,
                                mouse_button_pressed,
                                input_bytes,
                                word_separators,
                                bound_pasted_images,
                            )?;
                        }
                        Ok(if selection_changed {
                            ViewActionResult::OverlaySnapshot
                        } else {
                            ViewActionResult::None
                        })
                    }
                    WheelRoute::AlternateScroll => {
                        let selection_changed = selection.take().is_some();
                        terminal.set_selection(None)?;
                        write_alternate_scroll(terminal, lines, writer)?;
                        Ok(if selection_changed {
                            ViewActionResult::OverlaySnapshot
                        } else {
                            ViewActionResult::None
                        })
                    }
                    WheelRoute::Viewport => {
                        let scrollbar = terminal.scrollbar()?;
                        if scrollbar.total > scrollbar.len {
                            terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize(
                                i64::from(lines),
                            )));
                            Ok(ViewActionResult::Snapshot)
                        } else {
                            Ok(ViewActionResult::None)
                        }
                    }
                }
            }
        }
        TerminalViewAction::ScrollLines(lines) => {
            if let Some(mode) = copy_mode.as_mut() {
                scroll_copy_mode(mode, i64::from(lines));
            } else {
                terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize(i64::from(lines))));
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::ScrollPages(pages) => {
            let rows = copy_mode.as_ref().map_or_else(
                || i64::from(geometry.rows),
                |mode| i64::from(mode.revision.viewport_rows),
            );
            let delta = i64::from(pages).saturating_mul(rows);
            if let Some(mode) = copy_mode.as_mut() {
                scroll_copy_mode(mode, delta);
            } else {
                terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize(delta)));
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::ScrollTop => {
            if let Some(mode) = copy_mode.as_mut() {
                mode.viewport_offset = 0;
            } else {
                terminal.scroll_viewport(ScrollViewport::Top);
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::ScrollBottom => {
            if let Some(mode) = copy_mode.as_mut() {
                mode.viewport_offset = mode.revision.maximum_offset();
            } else {
                terminal.scroll_viewport(ScrollViewport::Bottom);
                *unseen_output = 0;
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::ScrollToFraction(fraction) => {
            if let Some(mode) = copy_mode.as_mut() {
                mode.viewport_offset = u32::try_from(
                    u128::from(mode.revision.maximum_offset()).saturating_mul(u128::from(fraction))
                        / u128::from(u32::MAX),
                )
                .unwrap_or(mode.revision.maximum_offset());
                return Ok(ViewActionResult::Snapshot);
            }
            let scrollbar = terminal.scrollbar()?;
            let maximum = scrollbar.total.saturating_sub(scrollbar.len);
            let target = u64::try_from(
                u128::from(maximum).saturating_mul(u128::from(fraction)) / u128::from(u32::MAX),
            )
            .unwrap_or(maximum);
            let delta = i128::from(target).saturating_sub(i128::from(scrollbar.offset));
            terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize_i128(delta)));
            if target == maximum {
                *unseen_output = 0;
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::ScrollToOffset(target) => {
            scroll_to_offset(terminal, copy_mode, unseen_output, target)?;
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::SelectionPress(event) => {
            if let Some(mode) = copy_mode.as_mut() {
                mode_selection_press(mode, event, word_separators);
            } else {
                selection_press(terminal, selection, event, word_separators)?;
            }
            Ok(ViewActionResult::OverlaySnapshot)
        }
        TerminalViewAction::SelectionDrag(event) => {
            if let Some(mode) = copy_mode.as_mut() {
                mode_selection_drag(mode, event, word_separators);
            } else {
                selection_drag(terminal, selection, event, word_separators)?;
            }
            Ok(ViewActionResult::OverlaySnapshot)
        }
        TerminalViewAction::SelectionAutoscroll { lines, pointer } => {
            if let Some(mode) = copy_mode.as_mut() {
                scroll_copy_mode(mode, i64::from(lines));
                mode_selection_drag(mode, pointer, word_separators);
            } else {
                terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize(i64::from(lines))));
                selection_drag(terminal, selection, pointer, word_separators)?;
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::SelectionRelease(_) | TerminalViewAction::ClearLinkHover => {
            Ok(ViewActionResult::None)
        }
        TerminalViewAction::Mouse(input) => route_mouse_input(
            terminal,
            selection,
            hover_link,
            copy_mode.as_deref_mut(),
            input,
            writer,
            mouse_encoder,
            mouse_event,
            mouse_button_pressed,
            input_bytes,
            word_separators,
            bound_pasted_images,
        ),
        TerminalViewAction::SelectAll => {
            if let Some(mode) = copy_mode.as_mut() {
                mode.selection = Some(ModeSelection {
                    anchor: PointCoordinate { x: 0, y: 0 },
                    focus: PointCoordinate {
                        x: mode.revision.columns.saturating_sub(1),
                        y: mode.revision.total_rows().saturating_sub(1),
                    },
                    mode: SelectionMode::Cell,
                    rectangle: false,
                });
                mode.selecting = true;
            } else {
                select_all_history(terminal, selection)?;
            }
            Ok(ViewActionResult::OverlaySnapshot)
        }
        TerminalViewAction::ClearHistory => {
            search_worker.cancel(view_id);
            terminal.vt_write(b"\x1b[3J");
            *selection = None;
            *copy_mode = None;
            *search = None;
            *search_origin = None;
            *search_snapshot = None;
            *hover_link = None;
            *unseen_output = 0;
            terminal.set_selection(None)?;
            terminal.scroll_viewport(ScrollViewport::Bottom);
            Ok(ViewActionResult::ContentSnapshot)
        }
        TerminalViewAction::ClearSelection => {
            if let Some(mode) = copy_mode.as_mut() {
                mode.selection = None;
                mode.selecting = false;
            } else {
                *selection = None;
                terminal.set_selection(None)?;
            }
            Ok(ViewActionResult::OverlaySnapshot)
        }
        enter_action @ (TerminalViewAction::EnterCopyMode
        | TerminalViewAction::EnterCopyModeScrollExit
        | TerminalViewAction::EnterCopyModeWith { .. }) => {
            let fresh = copy_mode.is_none();
            if fresh {
                drop_view_search(
                    view_id,
                    search_worker,
                    search,
                    search_origin,
                    search_snapshot,
                );
            }
            enter_copy_mode(
                terminal,
                selection,
                copy_mode,
                matches!(
                    enter_action,
                    TerminalViewAction::EnterCopyModeScrollExit
                        | TerminalViewAction::EnterCopyModeWith {
                            scroll_exit: true,
                            ..
                        }
                ),
                matches!(
                    enter_action,
                    TerminalViewAction::EnterCopyModeWith {
                        hide_position: true,
                        ..
                    }
                ),
                pending_copy_source.take(),
                mode_keys_vi,
            )?;
            if fresh && let Some(mode) = copy_mode.as_deref_mut() {
                mode.search = pane_search.clone().map(|stored| CopyModeSearch {
                    direction: SearchDirection::Backward,
                    ..stored
                });
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::CopyModeCounted { action, count } => {
            let revision_before = copy_mode.as_ref().map(|mode| mode.revision.id);
            let result = if let CopyModeAction::Search(spec) = &action {
                run_copy_mode_search(
                    view_id,
                    copy_mode,
                    search,
                    search_snapshot,
                    search_worker,
                    spec,
                    count,
                    mode_keys_vi,
                    wrap_search,
                    pane_search,
                );
                ViewActionResult::Snapshot
            } else if let CopyModeAction::SearchAgain { reverse } = action {
                if let Some(spec) = copy_mode_search_again(copy_mode.as_deref(), reverse) {
                    run_copy_mode_search(
                        view_id,
                        copy_mode,
                        search,
                        search_snapshot,
                        search_worker,
                        &spec,
                        count,
                        mode_keys_vi,
                        wrap_search,
                        pane_search,
                    );
                } else {
                    let forward = search
                        .as_ref()
                        .is_none_or(|search| search.query.direction == SearchDirection::Forward);
                    let direction = if forward ^ reverse { 1 } else { -1 };
                    for _ in 0..count {
                        step_search(search, direction, wrap_search);
                    }
                    sync_copy_cursor_to_search(copy_mode, search.as_deref());
                    reveal_search_for_view(terminal, copy_mode.as_deref(), search.as_deref())?;
                }
                ViewActionResult::Snapshot
            } else {
                apply_counted_copy_mode_action(
                    terminal,
                    selection,
                    copy_mode,
                    unseen_output,
                    action,
                    count,
                    word_separators,
                    mode_keys_vi,
                )?
            };
            settle_copy_search(
                view_id,
                search_worker,
                copy_mode,
                search,
                search_origin,
                search_snapshot,
                revision_before,
            );
            Ok(result)
        }
        TerminalViewAction::CopyMode(CopyModeAction::Search(spec)) => {
            run_copy_mode_search(
                view_id,
                copy_mode,
                search,
                search_snapshot,
                search_worker,
                &spec,
                1,
                mode_keys_vi,
                wrap_search,
                pane_search,
            );
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::CopyMode(CopyModeAction::SearchAgain { reverse }) => {
            if let Some(spec) = copy_mode_search_again(copy_mode.as_deref(), reverse) {
                run_copy_mode_search(
                    view_id,
                    copy_mode,
                    search,
                    search_snapshot,
                    search_worker,
                    &spec,
                    1,
                    mode_keys_vi,
                    wrap_search,
                    pane_search,
                );
            } else {
                let forward = search
                    .as_ref()
                    .is_none_or(|search| search.query.direction == SearchDirection::Forward);
                let direction = if forward ^ reverse { 1 } else { -1 };
                step_search(search, direction, wrap_search);
                sync_copy_cursor_to_search(copy_mode, search.as_deref());
                reveal_search_for_view(terminal, copy_mode.as_deref(), search.as_deref())?;
            }
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::CopyMode(CopyModeAction::SearchCursorWord { direction }) => {
            let text = copy_mode
                .as_deref()
                .and_then(|mode| mode_cursor_word(&mode.revision, mode.cursor, word_separators));
            if let Some(text) = text {
                run_copy_mode_search(
                    view_id,
                    copy_mode,
                    search,
                    search_snapshot,
                    search_worker,
                    &CopyModeSearch {
                        text,
                        direction,
                        regex: false,
                        incremental: false,
                    },
                    1,
                    mode_keys_vi,
                    wrap_search,
                    pane_search,
                );
                Ok(ViewActionResult::Snapshot)
            } else {
                Ok(ViewActionResult::None)
            }
        }
        TerminalViewAction::CopyMode(action) => {
            let revision_before = copy_mode.as_ref().map(|mode| mode.revision.id);
            let result = apply_copy_mode_action(
                terminal,
                selection,
                copy_mode,
                unseen_output,
                action,
                word_separators,
                mode_keys_vi,
            )?;
            settle_copy_search(
                view_id,
                search_worker,
                copy_mode,
                search,
                search_origin,
                search_snapshot,
                revision_before,
            );
            Ok(result)
        }
        TerminalViewAction::SearchBegin(query) => {
            remember_copy_mode_search(copy_mode.as_deref_mut(), pane_search, &query);
            let selection =
                search_selection_policy(copy_mode.as_deref(), search_origin, query.direction, true);
            if search_snapshot.is_none()
                && let Some(mode) = copy_mode.as_ref()
            {
                *search_snapshot = Some(Arc::clone(&mode.revision.search));
            }
            queue_search(
                terminal,
                SearchTarget {
                    view_id,
                    screen: view_screen,
                    search,
                    snapshot: search_snapshot,
                },
                search_worker,
                query,
                selection,
            )?;
            Ok(ViewActionResult::OverlaySnapshot)
        }
        TerminalViewAction::SearchUpdate(query) => {
            remember_copy_mode_search(copy_mode.as_deref_mut(), pane_search, &query);
            let selection = search_selection_policy(
                copy_mode.as_deref(),
                search_origin,
                query.direction,
                false,
            );
            if search_snapshot.is_none()
                && let Some(mode) = copy_mode.as_ref()
            {
                *search_snapshot = Some(Arc::clone(&mode.revision.search));
            }
            queue_search(
                terminal,
                SearchTarget {
                    view_id,
                    screen: view_screen,
                    search,
                    snapshot: search_snapshot,
                },
                search_worker,
                query,
                selection,
            )?;
            Ok(ViewActionResult::OverlaySnapshot)
        }
        TerminalViewAction::SearchNext => {
            step_search(search, 1, wrap_search);
            sync_copy_cursor_to_search(copy_mode, search.as_deref());
            reveal_search_for_view(terminal, copy_mode.as_deref(), search.as_deref())?;
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::SearchPrevious => {
            step_search(search, -1, wrap_search);
            sync_copy_cursor_to_search(copy_mode, search.as_deref());
            reveal_search_for_view(terminal, copy_mode.as_deref(), search.as_deref())?;
            Ok(ViewActionResult::Snapshot)
        }
        TerminalViewAction::SearchClose => {
            search_worker.cancel(view_id);
            *search = None;
            *search_origin = None;
            *search_snapshot = None;
            Ok(ViewActionResult::OverlaySnapshot)
        }
        TerminalViewAction::CopySelection { request_id, target } => {
            let text = copy_mode.as_ref().map_or_else(
                || format_selection_text(terminal),
                |mode| {
                    Ok(mode.selection.map_or_else(String::new, |selection| {
                        mode.revision.format_selection(selection, mode_keys_vi)
                    }))
                },
            )?;
            Ok(ViewActionResult::Copy(Box::new(TerminalCopyReady {
                request_id,
                clipboard: Some(target),
                buffer: None,
                pipe: None,
                text,
                view_changed: false,
            })))
        }
        TerminalViewAction::Focus(focused) => {
            write_focus_event(terminal, focused, writer)?;
            Ok(ViewActionResult::None)
        }
        TerminalViewAction::Paste(text) => {
            write_paste_bytes(terminal, text.into_bytes(), true, writer)?;
            Ok(ViewActionResult::None)
        }
    };
    let result = match warn_and_skip_view_ghostty("view action", result)? {
        Some(result) => result,
        None if hover_cleared => return Ok(ViewActionResult::OverlaySnapshot),
        None => return Ok(ViewActionResult::None),
    };
    Ok(
        if hover_cleared && matches!(result, ViewActionResult::None) {
            ViewActionResult::OverlaySnapshot
        } else {
            result
        },
    )
}

pub(super) fn write_paste_bytes(
    terminal: &Terminal<'_, '_>,
    mut source: Vec<u8>,
    bracketed: bool,
    writer: &mut dyn Write,
) -> Result<(), WorkerError> {
    if source.is_empty() {
        return Ok(());
    }
    let mut encoded = vec![0_u8; source.len().saturating_add(16)];
    let bracketed = bracketed && terminal.mode(Mode::BRACKETED_PASTE)?;
    let written = libghostty_vt::paste::encode(&mut source, bracketed, &mut encoded)?;
    writer.write_all(&encoded[..written])?;
    writer.flush()?;
    Ok(())
}

pub(super) fn write_prepared_paste_bytes(
    terminal: &Terminal<'_, '_>,
    source: &[u8],
    bracketed: bool,
    writer: &mut dyn Write,
) -> Result<(), WorkerError> {
    let bracketed = bracketed && terminal.mode(Mode::BRACKETED_PASTE)?;
    if source.is_empty() && !bracketed {
        return Ok(());
    }
    if bracketed {
        writer.write_all(b"\x1b[200~")?;
    }
    writer.write_all(source)?;
    if bracketed {
        writer.write_all(b"\x1b[201~")?;
    }
    writer.flush()?;
    Ok(())
}

fn scroll_copy_mode(mode: &mut CopyModeState, delta: i64) {
    let next = i128::from(mode.viewport_offset).saturating_add(i128::from(delta));
    mode.viewport_offset = u32::try_from(next.max(0))
        .unwrap_or(u32::MAX)
        .min(mode.revision.maximum_offset());
}

pub(super) fn mode_pointer_point(mode: &CopyModeState, event: PointerCellEvent) -> PointCoordinate {
    mode.revision.clamp_point(PointCoordinate {
        x: event.column,
        y: mode.viewport_offset.saturating_add(u32::from(event.row)),
    })
}

fn mode_selection_press(
    mode: &mut CopyModeState,
    event: PointerCellEvent,
    word_separators: &WordSeparators,
) {
    let point = mode_pointer_point(mode, event);
    mode.cursor = point;
    mode.rectangle = event.rectangle;
    let selection_mode = match event.click_count {
        0 | 1 => SelectionMode::Cell,
        2 => SelectionMode::Word,
        _ => SelectionMode::Line,
    };
    let (anchor, focus) = match selection_mode {
        SelectionMode::Cell => (point, point),
        SelectionMode::Word => mode_word_bounds(&mode.revision, point, word_separators),
        SelectionMode::Line => (
            PointCoordinate { x: 0, y: point.y },
            PointCoordinate {
                x: mode.revision.columns.saturating_sub(1),
                y: point.y,
            },
        ),
    };
    mode.selection = Some(ModeSelection {
        anchor,
        focus,
        mode: selection_mode,
        rectangle: mode.rectangle,
    });
    mode.selecting = true;
}

fn mode_selection_drag(
    mode: &mut CopyModeState,
    event: PointerCellEvent,
    word_separators: &WordSeparators,
) {
    let point = mode_pointer_point(mode, event);
    mode.cursor = point;
    if let Some(selection) = mode.selection.as_mut() {
        selection.focus = match selection.mode {
            SelectionMode::Cell => point,
            SelectionMode::Word => {
                let (start, end) = mode_word_bounds(&mode.revision, point, word_separators);
                if (point.y, point.x) < (selection.anchor.y, selection.anchor.x) {
                    start
                } else {
                    end
                }
            }
            SelectionMode::Line => PointCoordinate {
                x: mode.revision.columns.saturating_sub(1),
                y: point.y,
            },
        };
        mode.rectangle |= event.rectangle;
        selection.rectangle = mode.rectangle;
    }
}

pub(super) fn mode_word_edge(
    revision: &ModeRevision,
    point: PointCoordinate,
    word_separators: &WordSeparators,
    forward: bool,
) -> PointCoordinate {
    let mut at = point;
    while revision_cell_is_whitespace(revision, at) {
        if forward {
            if at.x.saturating_add(1) >= revision.columns {
                return point;
            }
            at.x += 1;
        } else {
            if at.x == 0 {
                return point;
            }
            at.x -= 1;
        }
    }
    let (start, end) = mode_word_bounds(revision, at, word_separators);
    if forward { end } else { start }
}

pub(super) fn mode_word_bounds(
    revision: &ModeRevision,
    point: PointCoordinate,
    word_separators: &WordSeparators,
) -> (PointCoordinate, PointCoordinate) {
    let class = revision_word_class(revision, point, word_separators);
    let mut start = point.x;
    while start > 0
        && revision_word_class(
            revision,
            PointCoordinate {
                x: start - 1,
                y: point.y,
            },
            word_separators,
        ) == class
    {
        start -= 1;
    }
    let mut end = point.x;
    while end + 1 < revision.columns
        && revision_word_class(
            revision,
            PointCoordinate {
                x: end + 1,
                y: point.y,
            },
            word_separators,
        ) == class
    {
        end += 1;
    }
    (
        PointCoordinate {
            x: start,
            y: point.y,
        },
        PointCoordinate { x: end, y: point.y },
    )
}

fn mode_cursor_word(
    revision: &ModeRevision,
    point: PointCoordinate,
    word_separators: &WordSeparators,
) -> Option<String> {
    if revision_cell_is_whitespace(revision, point) {
        return None;
    }
    let (anchor, focus) = mode_word_bounds(revision, point, word_separators);
    let text = revision.format_selection(
        ModeSelection {
            anchor,
            focus,
            mode: SelectionMode::Word,
            rectangle: false,
        },
        true,
    );
    (!text.is_empty()).then_some(text)
}

pub(super) fn capture_copy_source(
    terminal: &mut Terminal<'_, '_>,
) -> Result<CapturedCopySource, WorkerError> {
    let revision = ModeRevision::capture(terminal)?;
    let last_used = used_screen_rows(terminal).unwrap_or(0);
    let cursor_row = terminal.cursor_y()?;
    let (x, y) = if cursor_row + 1 > last_used {
        (0, last_used.saturating_sub(1))
    } else {
        (
            terminal.cursor_x()?.min(terminal.cols()?.saturating_sub(1)),
            cursor_row,
        )
    };
    let cursor = terminal.grid_ref(Point::Active(PointCoordinate { x, y: u32::from(y) }))?;
    let cursor = terminal
        .point_from_grid_ref(&cursor, PointSpace::Screen)?
        .unwrap_or(PointCoordinate { x, y: u32::from(y) });
    let viewport_offset = revision.maximum_offset();
    Ok(CapturedCopySource {
        revision,
        cursor,
        viewport_offset,
    })
}

pub(super) fn enter_copy_mode(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    copy_mode: &mut CopyModeSlot,
    scroll_exit: bool,
    hide_position: bool,
    source: Option<Box<CapturedCopySource>>,
    mode_keys_vi: bool,
) -> Result<(), WorkerError> {
    if copy_mode.is_some() {
        return Ok(());
    }
    terminal.scroll_viewport(ScrollViewport::Bottom);
    terminal.set_selection(None)?;
    *selection = None;
    let sourced = source.is_some();
    let (revision, cursor, viewport_offset) = if let Some(source) = source {
        let CapturedCopySource {
            revision,
            cursor,
            viewport_offset,
        } = *source;
        (revision, cursor, viewport_offset)
    } else {
        let x = terminal.cursor_x()?.min(terminal.cols()?.saturating_sub(1));
        let y = terminal.cursor_y()?;
        let cursor = terminal.grid_ref(Point::Active(PointCoordinate { x, y: u32::from(y) }))?;
        let cursor = terminal
            .point_from_grid_ref(&cursor, PointSpace::Screen)?
            .unwrap_or(PointCoordinate { x, y: u32::from(y) });
        let revision = ModeRevision::capture(terminal)?;
        let viewport_offset = u32::try_from(terminal.scrollbar()?.offset).unwrap_or(u32::MAX);
        (revision, cursor, viewport_offset)
    };
    *copy_mode = Some(Box::new(CopyModeState {
        revision,
        cursor,
        viewport_offset,
        scroll_exit,
        hide_position,
        selection: None,
        selection_mode: CopySelectionMode::Char,
        selection_origin: PointCoordinate { x: 0, y: 0 },
        recentre: None,
        mark: None,
        last_jump: None,
        selecting: false,
        rectangle: false,
        refresh: false,
        sourced,
        kind: FrozenModeKind::Copy,
        last_cx: 0,
        last_sx: 0,
        mode_keys_vi_at_entry: mode_keys_vi,
        search_marks: false,
        search_count: Some((0, false)),
        incremental_origin: None,
        search: None,
        search_all: true,
        line_numbers: 1,
    }));
    Ok(())
}

pub(super) fn drop_view_search(
    view_id: TerminalViewId,
    search_worker: &mut SearchWorker,
    search: &mut SearchSlot,
    search_origin: &mut Option<PointCoordinate>,
    search_snapshot: &mut Option<Arc<HistorySearchSnapshot>>,
) {
    search_worker.cancel(view_id);
    *search = None;
    *search_origin = None;
    *search_snapshot = None;
}

pub(super) fn wheel_route(
    terminal: &Terminal<'_, '_>,
    force_selection: bool,
) -> Result<WheelRoute, WorkerError> {
    if force_selection {
        return Ok(WheelRoute::Viewport);
    }
    if terminal.is_mouse_tracking()? {
        return Ok(WheelRoute::ApplicationMouse);
    }
    if terminal.active_screen()? == Screen::Alternate && terminal.mode(Mode::ALT_SCROLL)? {
        return Ok(WheelRoute::AlternateScroll);
    }
    Ok(WheelRoute::Viewport)
}

pub(super) fn write_alternate_scroll(
    terminal: &Terminal<'_, '_>,
    lines: i32,
    writer: &mut dyn Write,
) -> Result<(), WorkerError> {
    let sequence = if terminal.mode(Mode::DECCKM)? {
        if lines < 0 { b"\x1bOA" } else { b"\x1bOB" }
    } else if lines < 0 {
        b"\x1b[A"
    } else {
        b"\x1b[B"
    };
    let repetitions = usize::try_from(lines.unsigned_abs().min(MAX_WHEEL_REPEAT)).unwrap_or(0);
    let mut bytes = [0_u8; 96];
    let length = repetitions.saturating_mul(sequence.len());
    for chunk in bytes[..length].chunks_exact_mut(sequence.len()) {
        chunk.copy_from_slice(sequence);
    }
    if length != 0 {
        writer.write_all(&bytes[..length])?;
        writer.flush()?;
    }
    Ok(())
}

#[allow(
    clippy::cast_precision_loss,
    reason = "libghostty's mouse ABI uses f32 pixel positions"
)]
pub(super) fn route_mouse_input(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    hover_link: &mut Option<HoverLink>,
    copy_mode: Option<&mut CopyModeState>,
    input: TerminalMouseInput,
    writer: &mut dyn Write,
    encoder: &mut mouse::Encoder<'_>,
    event: &mut mouse::Event<'_>,
    button_pressed: &mut bool,
    input_bytes: &mut Vec<u8>,
    word_separators: &WordSeparators,
    bound_pasted_images: &HashSet<u32>,
) -> Result<ViewActionResult, WorkerError> {
    let phase = input.phase();
    let button = input.button();
    let modifiers = input.modifiers();
    let force_selection = input.force_selection();
    if let Some(mode) = copy_mode {
        *hover_link = None;
        if button != Some(TerminalMouseButton::Left) {
            return Ok(ViewActionResult::None);
        }
        match phase {
            TerminalMousePhase::Press => {
                mode_selection_press(mode, input.cell, word_separators);
            }
            TerminalMousePhase::Motion => {
                mode_selection_drag(mode, input.cell, word_separators);
            }
            TerminalMousePhase::Release => {}
        }
        return Ok(ViewActionResult::OverlaySnapshot);
    }
    if phase == TerminalMousePhase::Press
        && button == Some(TerminalMouseButton::Left)
        && input.cell.click_count >= 3
        && (modifiers.control() || modifiers.platform())
        && select_semantic_output_at(terminal, selection, input.cell)?
    {
        *hover_link = None;
        return Ok(ViewActionResult::OverlaySnapshot);
    }

    let link_modifier = modifiers.control() || modifiers.platform();
    let plain_image_motion = !force_selection
        && modifiers.bits() == 0
        && phase == TerminalMousePhase::Motion
        && button.is_none();
    let plain_image_hover_changed = if plain_image_motion {
        let columns = terminal.cols()?;
        let next_hover = image_placeholder_at(terminal, input.cell, columns, bound_pasted_images)?;
        let changed = *hover_link != next_hover;
        *hover_link = next_hover;
        Some(changed)
    } else {
        None
    };

    if !force_selection && terminal.is_mouse_tracking()? {
        let hover_changed =
            plain_image_hover_changed.unwrap_or_else(|| hover_link.take().is_some());
        let is_wheel = matches!(
            button,
            Some(TerminalMouseButton::ScrollUp | TerminalMouseButton::ScrollDown)
        );
        *button_pressed = match phase {
            TerminalMousePhase::Press if !is_wheel => true,
            TerminalMousePhase::Press | TerminalMousePhase::Release => false,
            TerminalMousePhase::Motion => *button_pressed,
        };
        let mut modifiers = key::Mods::empty();
        modifiers.set(key::Mods::SHIFT, input.modifiers().shift());
        modifiers.set(key::Mods::CTRL, input.modifiers().control());
        modifiers.set(key::Mods::ALT, input.modifiers().alt());
        modifiers.set(key::Mods::SUPER, input.modifiers().platform());
        event
            .set_action(match phase {
                TerminalMousePhase::Press => mouse::Action::Press,
                TerminalMousePhase::Release => mouse::Action::Release,
                TerminalMousePhase::Motion => mouse::Action::Motion,
            })
            .set_button(button.map(|button| match button {
                TerminalMouseButton::Left => mouse::Button::Left,
                TerminalMouseButton::Middle => mouse::Button::Middle,
                TerminalMouseButton::Right => mouse::Button::Right,
                TerminalMouseButton::ScrollUp => mouse::Button::Four,
                TerminalMouseButton::ScrollDown => mouse::Button::Five,
            }))
            .set_mods(modifiers)
            .set_position(mouse::Position {
                x: input.x as f32,
                y: input.y as f32,
            });
        encoder
            .set_options_from_terminal(terminal)
            .set_size(EncoderSize {
                screen_width: input.screen_width,
                screen_height: input.screen_height,
                cell_width: input.cell_width.max(1),
                cell_height: input.cell_height.max(1),
                padding_top: 0,
                padding_bottom: 0,
                padding_right: 0,
                padding_left: 0,
            })
            .set_any_button_pressed(*button_pressed)
            .set_track_last_cell(true);
        input_bytes.clear();
        encoder.encode_to_vec(event, input_bytes)?;
        if !input_bytes.is_empty() {
            writer.write_all(input_bytes)?;
            writer.flush()?;
        }
        return Ok(if hover_changed {
            ViewActionResult::OverlaySnapshot
        } else {
            ViewActionResult::None
        });
    }

    if !force_selection
        && phase == TerminalMousePhase::Press
        && button == Some(TerminalMouseButton::Left)
        && (modifiers.control() || modifiers.platform())
    {
        if let Some(link) = hover_link.as_ref().filter(|link| {
            link.contains(input.cell) && hover_link_is_bound(link, bound_pasted_images)
        }) {
            return Ok(ViewActionResult::OpenUri(link.uri.clone()));
        }
        if let Some(link) = hover_link_at(terminal, input.cell, input_bytes, bound_pasted_images)? {
            let uri = link.uri.clone();
            *hover_link = Some(link);
            return Ok(ViewActionResult::OpenUri(uri));
        }
    }

    let keep_cached_hover = !plain_image_motion
        && link_modifier
        && phase == TerminalMousePhase::Motion
        && button.is_none()
        && hover_link
            .as_ref()
            .is_some_and(|link| link.contains(input.cell));
    let hover_changed = if let Some(changed) = plain_image_hover_changed {
        changed
    } else if keep_cached_hover {
        false
    } else {
        let next_hover = (link_modifier && phase == TerminalMousePhase::Motion && button.is_none())
            .then(|| hover_link_at(terminal, input.cell, input_bytes, bound_pasted_images))
            .transpose()?
            .flatten();
        let changed = *hover_link != next_hover;
        if phase == TerminalMousePhase::Motion {
            *hover_link = next_hover;
        } else if phase == TerminalMousePhase::Press {
            *hover_link = None;
        }
        changed
    };
    if button != Some(TerminalMouseButton::Left) {
        return Ok(if hover_changed {
            ViewActionResult::OverlaySnapshot
        } else {
            ViewActionResult::None
        });
    }
    match phase {
        TerminalMousePhase::Press => {
            selection_press(terminal, selection, input.cell, word_separators)?;
        }
        TerminalMousePhase::Motion => {
            selection_drag(terminal, selection, input.cell, word_separators)?;
        }
        TerminalMousePhase::Release => {}
    }
    Ok(ViewActionResult::OverlaySnapshot)
}

pub(super) fn hover_link_at(
    terminal: &Terminal<'_, '_>,
    point: PointerCellEvent,
    scratch: &mut Vec<u8>,
    bound_pasted_images: &HashSet<u32>,
) -> Result<Option<HoverLink>, WorkerError> {
    let columns = terminal.cols()?;
    let rows = terminal.rows()?;
    if point.column >= columns || point.row >= rows {
        return Ok(None);
    }

    if let Some(length) = hyperlink_uri_bytes(terminal, point.row, point.column, scratch)? {
        let Ok(uri) = std::str::from_utf8(&scratch[..length]) else {
            return Ok(None);
        };
        if !is_safe_link_uri(uri) {
            return Ok(None);
        }
        let uri = uri.to_owned();

        let mut start = point.column;
        while start > 0
            && cell_has_hyperlink_uri(terminal, point.row, start - 1, uri.as_bytes(), scratch)?
        {
            start -= 1;
        }
        let mut end = point.column.saturating_add(1);
        while end < columns
            && cell_has_hyperlink_uri(terminal, point.row, end, uri.as_bytes(), scratch)?
        {
            end += 1;
        }

        return Ok(Some(HoverLink {
            row: point.row,
            start,
            end,
            uri,
        }));
    }

    if let Some(link) = image_placeholder_at(terminal, point, columns, bound_pasted_images)? {
        return Ok(Some(link));
    }

    plain_uri_at(terminal, point, columns, scratch)
}

pub(super) fn image_placeholder_at(
    terminal: &Terminal<'_, '_>,
    point: PointerCellEvent,
    columns: u16,
    bound_pasted_images: &HashSet<u32>,
) -> Result<Option<HoverLink>, WorkerError> {
    let rows = terminal.rows()?;
    if point.column >= columns || point.row >= rows {
        return Ok(None);
    }
    let point_index = usize::from(point.column);
    let window_start = point_index.saturating_sub(MAX_PLACEHOLDER_CELLS - 1);
    let window_end = point_index
        .saturating_add(MAX_PLACEHOLDER_CELLS)
        .min(usize::from(columns));
    let mut cells = [b' '; 2 * MAX_PLACEHOLDER_CELLS - 1];
    for (cell, column) in cells.iter_mut().zip(window_start..window_end) {
        *cell = ascii_terminal_cell(
            terminal,
            point.row,
            u16::try_from(column).unwrap_or(u16::MAX),
        )?;
    }
    let cells = &cells[..window_end.saturating_sub(window_start)];
    let pointer = point_index.saturating_sub(window_start);

    for (prefix, suffix) in IMAGE_PLACEHOLDERS {
        for start in (0..=pointer.min(cells.len().saturating_sub(1))).rev() {
            let Some(rest) = cells[start..].strip_prefix(prefix.as_bytes()) else {
                continue;
            };
            let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
            if !rest[digits..].starts_with(suffix.as_bytes()) {
                continue;
            }
            let Some(number) = std::str::from_utf8(&rest[..digits])
                .ok()
                .and_then(|digits| digits.parse::<u32>().ok())
            else {
                continue;
            };
            if !bound_pasted_images.contains(&number) {
                continue;
            }
            let end = start + prefix.len() + digits + suffix.len();
            if pointer >= end {
                continue;
            }
            return Ok(Some(HoverLink {
                row: point.row,
                start: u16::try_from(window_start + start).unwrap_or(u16::MAX),
                end: u16::try_from(window_start + end).unwrap_or(u16::MAX),
                uri: format!("{IMAGE_PLACEHOLDER_SCHEME}://{number}"),
            }));
        }
    }
    Ok(None)
}

fn hover_link_is_bound(link: &HoverLink, bound_pasted_images: &HashSet<u32>) -> bool {
    pasted_image_number(&link.uri).is_none_or(|number| bound_pasted_images.contains(&number))
}

fn pasted_image_number(uri: &str) -> Option<u32> {
    uri.strip_prefix(IMAGE_PLACEHOLDER_SCHEME)?
        .strip_prefix("://")?
        .parse()
        .ok()
}

pub(super) fn image_placeholder_occurrences(
    terminal: &Terminal<'_, '_>,
) -> Result<HashMap<u32, usize>, WorkerError> {
    let columns = terminal.cols()?;
    let rows = terminal.rows()?;
    let first_row = u32::try_from(terminal.scrollback_rows()?).unwrap_or(u32::MAX);
    let mut occurrences = HashMap::new();
    let mut cells = Vec::with_capacity(usize::from(columns));
    for row in 0..rows {
        cells.clear();
        for column in 0..columns {
            cells.push(ascii_terminal_screen_cell(
                terminal,
                first_row.saturating_add(u32::from(row)),
                column,
            )?);
        }
        for start in 0..cells.len() {
            for (prefix, suffix) in IMAGE_PLACEHOLDERS {
                let Some(rest) = cells[start..].strip_prefix(prefix.as_bytes()) else {
                    continue;
                };
                let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
                if !rest[digits..].starts_with(suffix.as_bytes()) {
                    continue;
                }
                let Some(number) = std::str::from_utf8(&rest[..digits])
                    .ok()
                    .and_then(|digits| digits.parse::<u32>().ok())
                else {
                    continue;
                };
                *occurrences.entry(number).or_insert(0) += 1;
                break;
            }
        }
    }
    Ok(occurrences)
}

fn plain_uri_at(
    terminal: &Terminal<'_, '_>,
    point: PointerCellEvent,
    columns: u16,
    scratch: &mut Vec<u8>,
) -> Result<Option<HoverLink>, WorkerError> {
    let started = diagnostic_timer();
    scratch.clear();
    let point_index = usize::from(point.column);
    let result = (|| {
        if point.column >= columns
            || ascii_terminal_cell(terminal, point.row, point.column)?.is_ascii_whitespace()
        {
            return Ok(None);
        }

        let mut start = point_index;
        while start > 0 && point_index.saturating_sub(start) < MAX_LINK_URI_BYTES - 1 {
            let previous = ascii_terminal_cell(
                terminal,
                point.row,
                u16::try_from(start - 1).unwrap_or_default(),
            )?;
            if previous.is_ascii_whitespace() {
                break;
            }
            start -= 1;
        }
        let mut end = point_index.saturating_add(1);
        while end < usize::from(columns) && end.saturating_sub(start) < MAX_LINK_URI_BYTES {
            let next =
                ascii_terminal_cell(terminal, point.row, u16::try_from(end).unwrap_or(u16::MAX))?;
            if next.is_ascii_whitespace() {
                break;
            }
            end += 1;
        }
        if (start > 0
            && !ascii_terminal_cell(
                terminal,
                point.row,
                u16::try_from(start - 1).unwrap_or_default(),
            )?
            .is_ascii_whitespace())
            || (end < usize::from(columns)
                && !ascii_terminal_cell(
                    terminal,
                    point.row,
                    u16::try_from(end).unwrap_or(u16::MAX),
                )?
                .is_ascii_whitespace())
        {
            return Ok(None);
        }

        scratch.reserve(end.saturating_sub(start));
        for column in start..end {
            scratch.push(ascii_terminal_cell(
                terminal,
                point.row,
                u16::try_from(column).unwrap_or(u16::MAX),
            )?);
        }
        let mut local_start = 0;
        let mut local_end = scratch.len();
        while local_start < local_end
            && matches!(
                scratch[local_start],
                b'(' | b'[' | b'{' | b'<' | b'\'' | b'"'
            )
        {
            local_start += 1;
        }
        while local_start < local_end
            && matches!(
                scratch[local_end - 1],
                b'.' | b',' | b';' | b'!' | b'?' | b')' | b']' | b'}' | b'>' | b'\'' | b'"'
            )
        {
            local_end -= 1;
        }
        let local_point = point_index.saturating_sub(start);
        if !(local_start..local_end).contains(&local_point) {
            return Ok(None);
        }
        let Ok(uri) = std::str::from_utf8(&scratch[local_start..local_end]) else {
            return Ok(None);
        };
        if !is_safe_link_uri(uri) {
            return Ok(None);
        }
        Ok(Some(HoverLink {
            row: point.row,
            start: u16::try_from(start.saturating_add(local_start)).unwrap_or(u16::MAX),
            end: u16::try_from(start.saturating_add(local_end)).unwrap_or(u16::MAX),
            uri: uri.to_owned(),
        }))
    })();
    log::trace!(
        target: "zz_terminal::diagnostics::link",
        "plain_uri_lookup row={} column={} columns={} scanned_bytes={} hit={} elapsed_us={}",
        point.row,
        point.column,
        columns,
        scratch.len(),
        result.as_ref().is_ok_and(Option::is_some),
        diagnostic_elapsed_us(started),
    );
    result
}

fn ascii_terminal_cell(
    terminal: &Terminal<'_, '_>,
    row: u16,
    column: u16,
) -> Result<u8, WorkerError> {
    let grid_ref = terminal.grid_ref(Point::Viewport(PointCoordinate {
        x: column,
        y: u32::from(row),
    }))?;
    let codepoint = grid_ref.cell()?.codepoint()?;
    Ok(u8::try_from(codepoint)
        .ok()
        .filter(u8::is_ascii_graphic)
        .unwrap_or(b' '))
}

fn ascii_terminal_screen_cell(
    terminal: &Terminal<'_, '_>,
    row: u32,
    column: u16,
) -> Result<u8, WorkerError> {
    let grid_ref = terminal.grid_ref(Point::Screen(PointCoordinate { x: column, y: row }))?;
    let codepoint = grid_ref.cell()?.codepoint()?;
    Ok(u8::try_from(codepoint)
        .ok()
        .filter(u8::is_ascii_graphic)
        .unwrap_or(b' '))
}

fn cell_has_hyperlink_uri(
    terminal: &Terminal<'_, '_>,
    row: u16,
    column: u16,
    expected: &[u8],
    scratch: &mut Vec<u8>,
) -> Result<bool, WorkerError> {
    Ok(hyperlink_uri_bytes(terminal, row, column, scratch)?
        .is_some_and(|length| scratch[..length] == *expected))
}

fn hyperlink_uri_bytes(
    terminal: &Terminal<'_, '_>,
    row: u16,
    column: u16,
    scratch: &mut Vec<u8>,
) -> Result<Option<usize>, WorkerError> {
    hyperlink_uri_bytes_at(
        terminal,
        Point::Viewport(PointCoordinate {
            x: column,
            y: u32::from(row),
        }),
        scratch,
    )
}

pub(super) fn hyperlink_uri_bytes_at(
    terminal: &Terminal<'_, '_>,
    point: Point,
    scratch: &mut Vec<u8>,
) -> Result<Option<usize>, WorkerError> {
    if scratch.len() < LINK_URI_SCRATCH_BYTES {
        scratch.resize(LINK_URI_SCRATCH_BYTES, 0);
    }
    let grid_ref = terminal.grid_ref(point)?;
    match grid_ref.hyperlink_uri(scratch) {
        Ok(0) => Ok(None),
        Ok(length) => Ok(Some(length)),
        Err(libghostty_vt::Error::OutOfSpace { required })
            if required > 0 && required <= MAX_LINK_URI_BYTES =>
        {
            scratch.resize(required, 0);
            let length = grid_ref.hyperlink_uri(scratch)?;
            Ok((length > 0).then_some(length))
        }
        Err(libghostty_vt::Error::OutOfSpace { .. }) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn is_safe_link_uri(uri: &str) -> bool {
    if uri.len() > MAX_LINK_URI_BYTES
        || uri
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return false;
    }
    let Some((scheme, rest)) = uri.split_once(':') else {
        return false;
    };
    if scheme.eq_ignore_ascii_case("mailto") {
        return !rest.is_empty();
    }
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "http" | "https" | "file" | "ssh"
    ) && rest.starts_with("//")
}

fn grid_ref_first_character(
    grid_ref: &libghostty_vt::screen::GridRef<'_>,
) -> Result<Option<char>, WorkerError> {
    if !grid_ref.cell()?.has_text()? {
        return Ok(None);
    }
    let mut stack = ['\0'; 8];
    match grid_ref.graphemes(&mut stack) {
        Ok(count) => Ok(stack
            .get(..count)
            .and_then(|values| values.first())
            .copied()),
        Err(libghostty_vt::Error::OutOfSpace { required }) => {
            let mut graphemes = vec!['\0'; required];
            let count = grid_ref.graphemes(&mut graphemes)?;
            Ok(graphemes
                .get(..count)
                .and_then(|values| values.first())
                .copied())
        }
        Err(error) => Err(error.into()),
    }
}

fn select_native_word<'terminal>(
    terminal: &'terminal Terminal<'_, '_>,
    grid_ref: libghostty_vt::screen::GridRef<'terminal>,
    word_separators: &WordSeparators,
) -> Result<Option<Selection<'terminal>>, WorkerError> {
    let Some(character) = grid_ref_first_character(&grid_ref)? else {
        return Ok(None);
    };
    let boundaries = match character_word_class(Some(character), word_separators) {
        CopyWordClass::Whitespace => WordSeparators::whitespace_codepoints(),
        CopyWordClass::Separator => word_separators.separator_codepoints(),
        CopyWordClass::Word => word_separators.boundary_codepoints(),
    };
    Ok(terminal
        .select_word(SelectWordOptions::new(grid_ref).with_boundary_codepoints(boundaries))?)
}

fn select_native_word_between<'terminal>(
    terminal: &'terminal Terminal<'_, '_>,
    start: &libghostty_vt::screen::GridRef<'terminal>,
    end: &libghostty_vt::screen::GridRef<'terminal>,
    word_separators: &WordSeparators,
) -> Result<Option<Selection<'terminal>>, WorkerError> {
    let Some(start) = terminal.point_from_grid_ref(start, PointSpace::Screen)? else {
        return Ok(None);
    };
    let Some(end) = terminal.point_from_grid_ref(end, PointSpace::Screen)? else {
        return Ok(None);
    };
    let columns = u64::from(terminal.cols()?);
    let mut index = u64::from(start.y)
        .saturating_mul(columns)
        .saturating_add(u64::from(start.x));
    let end = u64::from(end.y)
        .saturating_mul(columns)
        .saturating_add(u64::from(end.x));
    loop {
        let grid_ref = terminal.grid_ref(Point::Screen(PointCoordinate {
            x: u16::try_from(index % columns).unwrap_or(0),
            y: u32::try_from(index / columns).unwrap_or(u32::MAX),
        }))?;
        if grid_ref.cell()?.has_text()? {
            return select_native_word(terminal, grid_ref, word_separators);
        }
        if index == end {
            return Ok(None);
        }
        if index < end {
            index = index.saturating_add(1);
        } else {
            index = index.saturating_sub(1);
        }
    }
}

pub(super) fn selection_press(
    terminal: &Terminal<'_, '_>,
    state: &mut Option<SelectionState>,
    event: PointerCellEvent,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    let point = viewport_point(terminal, event)?;
    let anchor = terminal.track_grid_ref(point)?;
    let focus = terminal.track_grid_ref(point)?;
    let grid_ref = terminal.grid_ref(point)?;
    let mode = match event.click_count {
        0 | 1 => SelectionMode::Cell,
        2 => SelectionMode::Word,
        _ => SelectionMode::Line,
    };
    let selection = match mode {
        SelectionMode::Cell => None,
        SelectionMode::Word => select_native_word(terminal, grid_ref, word_separators)?,
        SelectionMode::Line => terminal.select_line(SelectLineOptions::new(grid_ref))?,
    };
    terminal.set_selection(selection.as_ref())?;
    *state = Some(SelectionState {
        anchor,
        focus,
        mode,
        rectangle: event.rectangle,
    });
    Ok(())
}

pub(super) fn select_all_history(
    terminal: &Terminal<'_, '_>,
    state: &mut Option<SelectionState>,
) -> Result<(), WorkerError> {
    let columns = terminal.cols()?;
    let rows = u32::try_from(terminal.total_rows()?).unwrap_or(u32::MAX);
    let mut end = None;
    'rows: for row in (0..rows).rev() {
        for column in (0..columns).rev() {
            let point = PointCoordinate { x: column, y: row };
            if terminal
                .grid_ref(Point::Screen(point))?
                .cell()?
                .has_text()?
            {
                end = Some(point);
                break 'rows;
            }
        }
    }
    let Some(end) = end else {
        *state = None;
        terminal.set_selection(None)?;
        return Ok(());
    };
    let start = PointCoordinate { x: 0, y: 0 };
    let anchor = terminal.track_grid_ref(Point::Screen(start))?;
    let focus = terminal.track_grid_ref(Point::Screen(end))?;
    let start_ref = terminal.grid_ref(Point::Screen(start))?;
    let end_ref = terminal.grid_ref(Point::Screen(end))?;
    terminal.set_selection(Some(&Selection::new(start_ref, end_ref, false)))?;
    *state = Some(SelectionState {
        anchor,
        focus,
        mode: SelectionMode::Cell,
        rectangle: false,
    });
    Ok(())
}

pub(super) fn selection_drag(
    terminal: &Terminal<'_, '_>,
    state: &mut Option<SelectionState>,
    event: PointerCellEvent,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    let current = terminal.grid_ref(viewport_point(terminal, event)?)?;
    selection_drag_to(terminal, state, current, word_separators)
}

fn selection_drag_to(
    terminal: &Terminal<'_, '_>,
    state: &mut Option<SelectionState>,
    current: libghostty_vt::screen::GridRef<'_>,
    word_separators: &WordSeparators,
) -> Result<(), WorkerError> {
    let Some(selection_state) = state.as_ref() else {
        return Ok(());
    };
    let Some(current_point) = terminal.point_from_grid_ref(&current, PointSpace::Screen)? else {
        return Ok(());
    };
    let Some(anchor) = selection_state.anchor.snapshot(terminal)? else {
        *state = None;
        terminal.set_selection(None)?;
        return Ok(());
    };
    let anchor_point = terminal.point_from_grid_ref(&anchor, PointSpace::Screen)?;
    let next = match selection_state.mode {
        SelectionMode::Cell => anchor_point
            .is_some_and(|anchor| anchor != current_point)
            .then(|| Selection::new(anchor, current, selection_state.rectangle)),
        SelectionMode::Word => {
            let start = select_native_word_between(terminal, &anchor, &current, word_separators)?;
            let end = select_native_word_between(terminal, &current, &anchor, word_separators)?;
            start
                .zip(end)
                .map(|(start, end)| Selection::new(start.start(), end.end(), false))
        }
        SelectionMode::Line => {
            let start = terminal.select_line(SelectLineOptions::new(anchor))?;
            let end = terminal.select_line(SelectLineOptions::new(current))?;
            start
                .zip(end)
                .map(|(start, end)| Selection::new(start.start(), end.end(), false))
        }
    };
    terminal.set_selection(next.as_ref())?;
    let focus = terminal.track_grid_ref(Point::Screen(current_point))?;
    if let Some(selection_state) = state.as_mut() {
        selection_state.focus = focus;
    }
    Ok(())
}

pub(super) fn viewport_point(
    terminal: &Terminal<'_, '_>,
    event: PointerCellEvent,
) -> Result<Point, WorkerError> {
    let column = event.column.min(terminal.cols()?.saturating_sub(1));
    let row = event.row.min(terminal.rows()?.saturating_sub(1));
    Ok(Point::Viewport(PointCoordinate {
        x: column,
        y: u32::from(row),
    }))
}

pub(super) fn saturating_isize(value: i64) -> isize {
    isize::try_from(value).unwrap_or(if value.is_negative() {
        isize::MIN
    } else {
        isize::MAX
    })
}

fn saturating_isize_i128(value: i128) -> isize {
    isize::try_from(value).unwrap_or(if value.is_negative() {
        isize::MIN
    } else {
        isize::MAX
    })
}
