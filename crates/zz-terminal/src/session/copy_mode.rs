use super::*;

pub(super) fn apply_copy_mode_action(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    copy_mode: &mut CopyModeSlot,
    unseen_output: &mut u32,
    action: CopyModeAction,
    word_separators: &WordSeparators,
    mode_keys_vi: bool,
) -> Result<ViewActionResult, WorkerError> {
    let Some(mut mode) = copy_mode.take() else {
        return Ok(ViewActionResult::None);
    };

    if mode.search_marks && action.clears_search_marks(mode_keys_vi) {
        mode.search_marks = false;
        mode.search_count = None;
        mode.incremental_origin = None;
    }

    match action {
        ref movement @ (CopyModeAction::Left
        | CopyModeAction::Right
        | CopyModeAction::Up
        | CopyModeAction::Down
        | CopyModeAction::PageUp
        | CopyModeAction::PageDown
        | CopyModeAction::PageDownScrollExit
        | CopyModeAction::HalfPageUp
        | CopyModeAction::HalfPageDown
        | CopyModeAction::HalfPageDownScrollExit
        | CopyModeAction::Top
        | CopyModeAction::Bottom
        | CopyModeAction::TopLine
        | CopyModeAction::MiddleLine
        | CopyModeAction::BottomLine
        | CopyModeAction::StartOfLine
        | CopyModeAction::BackToIndentation
        | CopyModeAction::EndOfLine
        | CopyModeAction::NextWord
        | CopyModeAction::PreviousWord
        | CopyModeAction::NextWordEnd
        | CopyModeAction::NextSpace
        | CopyModeAction::PreviousSpace
        | CopyModeAction::NextSpaceEnd
        | CopyModeAction::NextParagraph
        | CopyModeAction::PreviousParagraph
        | CopyModeAction::NextMatchingBracket
        | CopyModeAction::PreviousMatchingBracket
        | CopyModeAction::NextPrompt { .. }
        | CopyModeAction::PreviousPrompt { .. }
        | CopyModeAction::CursorCentreVertical
        | CopyModeAction::CursorCentreHorizontal
        | CopyModeAction::JumpToMark) => {
            let scroll_exit = match movement {
                CopyModeAction::PageDownScrollExit | CopyModeAction::HalfPageDownScrollExit => true,
                CopyModeAction::PageDown | CopyModeAction::HalfPageDown => mode.scroll_exit,
                _ => false,
            };
            if scroll_exit && copy_mode_scroll_exit_ready(&mode) {
                return cancel_copy_mode(terminal, selection, unseen_output);
            }
            move_copy_cursor(&mut mode, movement, word_separators, mode_keys_vi);
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            if scroll_exit && copy_mode_scroll_exit_ready(&mode) {
                return cancel_copy_mode(terminal, selection, unseen_output);
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::ScrollDownAndCancel => {
            scroll_copy_cursor(&mut mode, false, mode_keys_vi);
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            if mode.viewport_offset == mode.revision.maximum_offset() {
                return cancel_copy_mode(terminal, selection, unseen_output);
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::CursorDownAndCancel => {
            *copy_mode = Some(mode);
            cursor_down_and_cancel(
                terminal,
                selection,
                copy_mode,
                unseen_output,
                1,
                word_separators,
                mode_keys_vi,
            )
        }
        scroll_exit_action @ (CopyModeAction::ScrollExitOn
        | CopyModeAction::ScrollExitOff
        | CopyModeAction::ScrollExitToggle) => {
            mode.scroll_exit = match scroll_exit_action {
                CopyModeAction::ScrollExitOn => true,
                CopyModeAction::ScrollExitOff => false,
                _ => !mode.scroll_exit,
            };
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        scroll @ (CopyModeAction::ScrollUp | CopyModeAction::ScrollDown) => {
            let scrolls_toward_bottom = matches!(scroll, CopyModeAction::ScrollDown);
            if scrolls_toward_bottom && mode.scroll_exit && copy_mode_scroll_exit_ready(&mode) {
                return cancel_copy_mode(terminal, selection, unseen_output);
            }
            scroll_copy_cursor(&mut mode, !scrolls_toward_bottom, mode_keys_vi);
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            if scrolls_toward_bottom && mode.scroll_exit && copy_mode_scroll_exit_ready(&mode) {
                return cancel_copy_mode(terminal, selection, unseen_output);
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        placement @ (CopyModeAction::ScrollTop
        | CopyModeAction::ScrollMiddle
        | CopyModeAction::ScrollBottom) => {
            let last = u32::from(mode.revision.viewport_rows.saturating_sub(1));
            let row = match placement {
                CopyModeAction::ScrollTop => 0,
                CopyModeAction::ScrollBottom => last,
                _ => last / 2,
            };
            scroll_copy_view_to_screen_row(&mut mode, row);
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::GotoLine {
            line,
            option_absolute,
        } => {
            let absolute = option_absolute && mode.line_numbers != 0;
            goto_copy_line(&mut mode, line, absolute);
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::RecentreTopBottom => {
            let row = mode.cursor.y;
            let target = match mode.recentre {
                Some((line, target)) if line == row => target,
                _ => RecentreTarget::Middle,
            };
            let last = u32::from(mode.revision.viewport_rows.saturating_sub(1));
            let (screen_row, next) = match target {
                RecentreTarget::Middle => (last / 2, RecentreTarget::Top),
                RecentreTarget::Top => (0, RecentreTarget::Bottom),
                RecentreTarget::Bottom => (last, RecentreTarget::Middle),
            };
            mode.recentre = Some((row, next));
            mode.viewport_offset = row
                .saturating_sub(screen_row)
                .min(mode.revision.maximum_offset());
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::TogglePosition => {
            mode.hide_position = !mode.hide_position;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        toggle @ (CopyModeAction::RefreshOn
        | CopyModeAction::RefreshOff
        | CopyModeAction::RefreshToggle) => {
            let start = match toggle {
                CopyModeAction::RefreshOn => true,
                CopyModeAction::RefreshOff => false,
                _ => !mode.refresh,
            };
            mode.refresh = start && mode.kind == FrozenModeKind::Copy && !mode.sourced;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::LineNumbersOn { option_off } => {
            mode.line_numbers = if option_off { 2 } else { 1 };
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::LineNumbersOff => {
            mode.line_numbers = 0;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::LineNumbersToggle { option_off } => {
            let active = mode.line_numbers == 2 || (mode.line_numbers == 1 && !option_off);
            mode.line_numbers = match (active, option_off) {
                (true, _) => 0,
                (false, true) => 2,
                (false, false) => 1,
            };
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::RefreshNow => {
            if mode.kind != FrozenModeKind::Copy || mode.sourced {
                *copy_mode = Some(mode);
                return Ok(ViewActionResult::None);
            }
            refresh_copy_revision(terminal, selection, &mut mode, mode_keys_vi)?;
            *unseen_output = 0;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::RefreshRevision => {
            if !mode.refresh
                || mode.kind != FrozenModeKind::Copy
                || mode.selection.is_some()
                || mode.selecting
                || *unseen_output == 0
            {
                *copy_mode = Some(mode);
                return Ok(ViewActionResult::None);
            }
            refresh_copy_revision(terminal, selection, &mut mode, mode_keys_vi)?;
            *unseen_output = 0;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::StartSelection => {
            mode.selection = Some(ModeSelection {
                anchor: mode.cursor,
                focus: mode.cursor,
                mode: SelectionMode::Cell,
                rectangle: mode.rectangle,
            });
            mode.selection_mode = CopySelectionMode::Char;
            mode.selection_origin = mode.cursor;
            mode.selecting = true;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::SelectWord => {
            let (anchor, mut focus) =
                mode_word_bounds(&mode.revision, mode.cursor, word_separators);
            if !mode_keys_vi {
                focus.x = focus
                    .x
                    .saturating_add(1)
                    .min(mode.revision.columns.saturating_sub(1));
            }
            mode.selection = Some(ModeSelection {
                anchor,
                focus,
                mode: SelectionMode::Word,
                rectangle: false,
            });
            mode.selection_mode = CopySelectionMode::Word;
            mode.selection_origin = mode.cursor;
            mode.cursor = focus;
            mode.selecting = true;
            mode.rectangle = false;
            reveal_copy_cursor(&mut mode);
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::SelectLine => {
            select_copy_mode_lines(&mut mode, 1, mode_keys_vi);
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::ClearSelection => {
            *selection = None;
            terminal.set_selection(None)?;
            mode.selection = None;
            mode.selection_mode = CopySelectionMode::Char;
            mode.selecting = false;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::ClearSelectionOrCancel if mode.selection.is_some() || mode.selecting => {
            *selection = None;
            terminal.set_selection(None)?;
            mode.selection = None;
            mode.selection_mode = CopySelectionMode::Char;
            mode.selecting = false;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::ToggleRectangle => {
            mode.rectangle = !mode.rectangle;
            if let Some(selection) = mode.selection.as_mut() {
                selection.rectangle = mode.rectangle;
                mode.selecting = true;
            }
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        rectangle_action @ (CopyModeAction::RectangleOn | CopyModeAction::RectangleOff) => {
            mode.rectangle = matches!(rectangle_action, CopyModeAction::RectangleOn);
            if let Some(selection) = mode.selection.as_mut() {
                selection.rectangle = mode.rectangle;
                mode.selecting = true;
            }
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::OtherEnd => {
            mode.selection_mode = CopySelectionMode::Char;
            if let Some(selection) = mode.selection.as_mut() {
                std::mem::swap(&mut selection.anchor, &mut selection.focus);
                mode.cursor = selection.focus;
                mode.selection_origin = mode.cursor;
                mode.selecting = true;
                reveal_copy_cursor(&mut mode);
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::SetMark => {
            mode.mark = Some(mode.cursor);
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::Jump(jump) => {
            let latched_vi = mode.mode_keys_vi_at_entry;
            apply_copy_jump(&mut mode, &jump, latched_vi);
            mode.last_jump = Some(jump);
            if mode.selecting {
                update_copy_selection(&mut mode, Some(word_separators));
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::RepeatJump { reverse } => {
            if let Some(mut jump) = mode.last_jump.clone() {
                if reverse {
                    jump.direction = match jump.direction {
                        CopyJumpDirection::Forward => CopyJumpDirection::Backward,
                        CopyJumpDirection::Backward => CopyJumpDirection::Forward,
                    };
                }
                let latched_vi = mode.mode_keys_vi_at_entry;
                apply_copy_jump(&mut mode, &jump, latched_vi);
                if mode.selecting {
                    update_copy_selection(&mut mode, Some(word_separators));
                }
            }
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::CopySelection(copy) => {
            let copy = *copy;
            let text = mode.selection.map_or_else(String::new, |selection| {
                mode.revision.format_selection(selection, mode_keys_vi)
            });
            let view_changed = copy.clear_selection || copy.cancel;
            if copy.cancel {
                *selection = None;
                terminal.set_selection(None)?;
                terminal.scroll_viewport(ScrollViewport::Bottom);
                *unseen_output = 0;
            } else {
                if copy.clear_selection {
                    mode.selection = None;
                    mode.selecting = false;
                }
                *copy_mode = Some(mode);
            }
            Ok(ViewActionResult::Copy(Box::new(TerminalCopyReady {
                request_id: copy.request_id,
                clipboard: copy.clipboard.then_some(ClipboardTarget::Clipboard),
                buffer: copy.buffer,
                pipe: copy.pipe,
                text,
                view_changed,
            })))
        }
        CopyModeAction::CopyEndOfLine(copy) => {
            select_copy_mode_to_line_end(&mut mode, 1, mode_keys_vi);
            *copy_mode = Some(mode);
            apply_copy_mode_action(
                terminal,
                selection,
                copy_mode,
                unseen_output,
                CopyModeAction::CopySelection(copy),
                word_separators,
                mode_keys_vi,
            )
        }
        CopyModeAction::CopyLine(copy) => {
            select_copy_mode_whole_line(&mut mode, 1, mode_keys_vi);
            *copy_mode = Some(mode);
            apply_copy_mode_action(
                terminal,
                selection,
                copy_mode,
                unseen_output,
                CopyModeAction::CopySelection(copy),
                word_separators,
                mode_keys_vi,
            )
        }
        CopyModeAction::SelectionMode(unit) => {
            mode.selection_mode = unit;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::MouseCursor(cell) => {
            mode.cursor = mode_pointer_point(&mode, cell);
            reveal_copy_cursor(&mut mode);
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::StopSelection => {
            mode.selection_mode = CopySelectionMode::Char;
            mode.selecting = false;
            *copy_mode = Some(mode);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeAction::SearchAgain { .. }
        | CopyModeAction::SearchCursorWord { .. }
        | CopyModeAction::Search(_) => {
            *copy_mode = Some(mode);
            Ok(ViewActionResult::None)
        }
        CopyModeAction::Cancel | CopyModeAction::ClearSelectionOrCancel => {
            cancel_copy_mode(terminal, selection, unseen_output)
        }
    }
}

fn select_copy_mode_to_line_end(mode: &mut CopyModeState, count: u32, mode_keys_vi: bool) {
    if count == 0 {
        return;
    }
    let focus_y = mode
        .cursor
        .y
        .saturating_add(count - 1)
        .min(mode.revision.total_rows().saturating_sub(1));
    mode.selection = Some(ModeSelection {
        anchor: mode.cursor,
        focus: PointCoordinate {
            x: copy_cursor_limit(&mode.revision, focus_y, mode_keys_vi, false),
            y: focus_y,
        },
        mode: SelectionMode::Cell,
        rectangle: false,
    });
    mode.selecting = true;
}

pub(super) fn apply_counted_copy_mode_action(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    copy_mode: &mut CopyModeSlot,
    unseen_output: &mut u32,
    action: CopyModeAction,
    count: u32,
    word_separators: &WordSeparators,
    mode_keys_vi: bool,
) -> Result<ViewActionResult, WorkerError> {
    if count == 0 {
        return Ok(ViewActionResult::None);
    }
    match action.count_policy() {
        CopyModeCountPolicy::Repeat => {
            if copy_mode.is_none() {
                return Ok(ViewActionResult::None);
            }
            for _ in 0..count {
                apply_copy_mode_action(
                    terminal,
                    selection,
                    copy_mode,
                    unseen_output,
                    action.clone(),
                    word_separators,
                    mode_keys_vi,
                )?;
                if copy_mode.is_none() {
                    break;
                }
            }
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeCountPolicy::OtherEnd if count.is_multiple_of(2) => Ok(ViewActionResult::Snapshot),
        CopyModeCountPolicy::OtherEnd | CopyModeCountPolicy::Once => apply_copy_mode_action(
            terminal,
            selection,
            copy_mode,
            unseen_output,
            action,
            word_separators,
            mode_keys_vi,
        ),
        CopyModeCountPolicy::CursorDownAndCancel => cursor_down_and_cancel(
            terminal,
            selection,
            copy_mode,
            unseen_output,
            count,
            word_separators,
            mode_keys_vi,
        ),
        CopyModeCountPolicy::SelectLine => {
            let Some(mode) = copy_mode.as_mut() else {
                return Ok(ViewActionResult::None);
            };
            select_copy_mode_lines(mode, count, mode_keys_vi);
            Ok(ViewActionResult::Snapshot)
        }
        CopyModeCountPolicy::CopyEndOfLine => {
            let CopyModeAction::CopyEndOfLine(copy) = action else {
                unreachable!("copy-end policy is only assigned to copy-end-of-line")
            };
            let Some(mut mode) = copy_mode.take() else {
                return Ok(ViewActionResult::None);
            };
            select_copy_mode_to_line_end(&mut mode, count, mode_keys_vi);
            *copy_mode = Some(mode);
            apply_copy_mode_action(
                terminal,
                selection,
                copy_mode,
                unseen_output,
                CopyModeAction::CopySelection(copy),
                word_separators,
                mode_keys_vi,
            )
        }
        CopyModeCountPolicy::CopyLine => {
            let CopyModeAction::CopyLine(copy) = action else {
                unreachable!("copy-line policy is only assigned to copy-line")
            };
            let Some(mut mode) = copy_mode.take() else {
                return Ok(ViewActionResult::None);
            };
            select_copy_mode_whole_line(&mut mode, count, mode_keys_vi);
            *copy_mode = Some(mode);
            apply_copy_mode_action(
                terminal,
                selection,
                copy_mode,
                unseen_output,
                CopyModeAction::CopySelection(copy),
                word_separators,
                mode_keys_vi,
            )
        }
    }
}

fn select_copy_mode_whole_line(mode: &mut CopyModeState, count: u32, mode_keys_vi: bool) {
    if count == 0 {
        return;
    }
    let (anchor, _) = mode_logical_line_bounds(&mode.revision, mode.cursor.y);
    let row = mode
        .cursor
        .y
        .saturating_add(count.saturating_sub(1))
        .min(mode.revision.total_rows().saturating_sub(1));
    let (_, end) = mode_logical_line_bounds(&mode.revision, row);
    let focus = PointCoordinate {
        x: copy_cursor_limit(&mode.revision, end.y, mode_keys_vi, false),
        y: end.y,
    };
    mode.selection_mode = CopySelectionMode::Char;
    mode.selection = Some(ModeSelection {
        anchor,
        focus,
        mode: SelectionMode::Cell,
        rectangle: false,
    });
    mode.selecting = true;
}

fn select_copy_mode_lines(mode: &mut CopyModeState, count: u32, mode_keys_vi: bool) {
    if count == 0 {
        return;
    }
    let origin = mode.cursor;
    let (anchor, mut focus) = mode_logical_line_bounds(&mode.revision, mode.cursor.y);
    for _ in 1..count {
        let next = focus.y.saturating_add(1);
        if next >= mode.revision.total_rows() {
            break;
        }
        let (_, next_focus) = mode_logical_line_bounds(&mode.revision, next);
        if next_focus.y == focus.y {
            break;
        }
        focus = next_focus;
    }
    focus.x = copy_cursor_limit(&mode.revision, focus.y, mode_keys_vi, false);
    mode.cursor = PointCoordinate {
        x: revision_copy_line_end(&mode.revision, focus.y),
        y: focus.y,
    };
    mode.selection = Some(ModeSelection {
        anchor,
        focus,
        mode: SelectionMode::Line,
        rectangle: false,
    });
    mode.selection_mode = CopySelectionMode::Line;
    mode.selection_origin = origin;
    mode.selecting = true;
    mode.rectangle = false;
    reveal_copy_cursor(mode);
}

fn goto_copy_line(mode: &mut CopyModeState, line: Option<i32>, absolute: bool) {
    let Some(line) = line else {
        return;
    };
    let maximum = mode.revision.maximum_offset();
    let offset = if absolute {
        u32::try_from(line.max(1) - 1).unwrap_or(0).min(maximum)
    } else {
        maximum.saturating_sub(u32::try_from(line).unwrap_or(maximum).min(maximum))
    };
    let delta = i64::from(offset) - i64::from(mode.viewport_offset);
    mode.viewport_offset = offset;
    let last = i64::from(mode.revision.total_rows().saturating_sub(1));
    let row = (i64::from(mode.cursor.y) + delta).clamp(0, last);
    mode.cursor.y = u32::try_from(row).unwrap_or(0);
    mode.cursor = mode.revision.clamp_point(mode.cursor);
}

fn scroll_copy_view_to_screen_row(mode: &mut CopyModeState, row: u32) {
    let Some(target) = mode.cursor.y.checked_sub(row) else {
        return;
    };
    if target <= mode.revision.maximum_offset() {
        mode.viewport_offset = target;
    }
}

fn cursor_down_and_cancel(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    copy_mode: &mut CopyModeSlot,
    unseen_output: &mut u32,
    count: u32,
    word_separators: &WordSeparators,
    mode_keys_vi: bool,
) -> Result<ViewActionResult, WorkerError> {
    let Some(start) = copy_mode.as_deref().map(|mode| mode.cursor.y) else {
        return Ok(ViewActionResult::None);
    };
    for _ in 0..count {
        apply_copy_mode_action(
            terminal,
            selection,
            copy_mode,
            unseen_output,
            CopyModeAction::Down,
            word_separators,
            mode_keys_vi,
        )?;
    }
    let Some(mode) = copy_mode.as_deref() else {
        return Ok(ViewActionResult::Snapshot);
    };
    if mode.cursor.y == start && mode.viewport_offset == mode.revision.maximum_offset() {
        *copy_mode = None;
        return cancel_copy_mode(terminal, selection, unseen_output);
    }
    Ok(ViewActionResult::Snapshot)
}

fn copy_mode_scroll_exit_ready(mode: &CopyModeState) -> bool {
    mode.viewport_offset == mode.revision.maximum_offset()
        && mode.selection.is_none()
        && !mode.selecting
}

fn cancel_copy_mode(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    unseen_output: &mut u32,
) -> Result<ViewActionResult, WorkerError> {
    *selection = None;
    terminal.set_selection(None)?;
    terminal.scroll_viewport(ScrollViewport::Bottom);
    *unseen_output = 0;
    Ok(ViewActionResult::Snapshot)
}

fn reader_jump_forward(
    revision: &ModeRevision,
    mut point: PointCoordinate,
    target: &str,
) -> Option<PointCoordinate> {
    let last = revision.total_rows().saturating_sub(1);
    loop {
        let length = revision_line_length(revision, point.y);
        while point.x < length {
            if revision.cell_matches_text(point, target) {
                return Some(point);
            }
            point.x += 1;
        }
        if point.y >= last || !revision.row(point.y).wrapped() {
            return None;
        }
        point.y += 1;
        point.x = 0;
    }
}

fn reader_jump_backward(
    revision: &ModeRevision,
    mut point: PointCoordinate,
    target: &str,
) -> Option<PointCoordinate> {
    let mut edge = point.x.saturating_add(1);
    loop {
        let mut column = edge;
        while column > 0 {
            column -= 1;
            let candidate = PointCoordinate {
                x: column,
                y: point.y,
            };
            if revision.cell_matches_text(candidate, target) {
                return Some(candidate);
            }
        }
        if point.y == 0 || !revision.row(point.y - 1).wrapped() {
            return None;
        }
        point.y -= 1;
        edge = revision_line_length(revision, point.y);
    }
}

fn apply_copy_jump(mode: &mut CopyModeState, jump: &CopyJump, vi: bool) {
    let start = mode.cursor;
    let found = match (jump.direction, jump.to) {
        (CopyJumpDirection::Forward, false) => reader_jump_forward(
            &mode.revision,
            PointCoordinate {
                x: start.x.saturating_add(1),
                y: start.y,
            },
            &jump.target,
        ),
        (CopyJumpDirection::Forward, true) => reader_jump_forward(
            &mode.revision,
            PointCoordinate {
                x: start.x.saturating_add(2),
                y: start.y,
            },
            &jump.target,
        )
        .map(|mut point| {
            reader_cursor_left(&mode.revision, &mut point, true);
            point
        }),
        (CopyJumpDirection::Backward, false) => {
            let mut point = start;
            reader_cursor_left(&mode.revision, &mut point, false);
            reader_jump_backward(&mode.revision, point, &jump.target)
        }
        (CopyJumpDirection::Backward, true) => {
            let mut point = start;
            reader_cursor_left(&mode.revision, &mut point, false);
            reader_cursor_left(&mode.revision, &mut point, false);
            reader_jump_backward(&mode.revision, point, &jump.target).map(|mut point| {
                reader_cursor_right(&mode.revision, &mut point, true, false, !vi);
                point
            })
        }
    };
    if let Some(point) = found {
        mode.cursor = point;
        reveal_copy_cursor(mode);
    }
}
pub(super) fn move_copy_cursor(
    mode: &mut CopyModeState,
    action: &CopyModeAction,
    word_separators: &WordSeparators,
    vi: bool,
) {
    let mut point = mode.cursor;
    let total = mode.revision.total_rows();
    let page = u32::from(mode.revision.viewport_rows);
    match action {
        CopyModeAction::Left => {
            reader_cursor_left(&mode.revision, &mut point, true);
        }
        CopyModeAction::Right => {
            let all = mode.selection.is_some() && mode.rectangle;
            reader_cursor_right(&mode.revision, &mut point, true, all, !vi);
        }
        CopyModeAction::Up => move_copy_cursor_row(mode, &mut point, false, vi),
        CopyModeAction::Down => move_copy_cursor_row(mode, &mut point, true, vi),
        CopyModeAction::PageUp => return page_copy_cursor(mode, true, false, vi),
        CopyModeAction::PageDown | CopyModeAction::PageDownScrollExit => {
            return page_copy_cursor(mode, false, false, vi);
        }
        CopyModeAction::HalfPageUp => return page_copy_cursor(mode, true, true, vi),
        CopyModeAction::HalfPageDown | CopyModeAction::HalfPageDownScrollExit => {
            return page_copy_cursor(mode, false, true, vi);
        }
        CopyModeAction::Top => {
            point.x = 0;
            point.y = 0;
        }
        CopyModeAction::Bottom => point.y = total - 1,
        CopyModeAction::CursorCentreVertical => {
            let middle = u32::from(mode.revision.viewport_rows) / 2;
            point.y = mode
                .viewport_offset
                .saturating_add(middle)
                .min(total.saturating_sub(1));
        }
        CopyModeAction::CursorCentreHorizontal => {
            point.x = (mode.revision.columns / 2).min(mode.revision.columns.saturating_sub(1));
        }
        CopyModeAction::TopLine => {
            point.x = 0;
            point.y = mode.viewport_offset;
        }
        CopyModeAction::MiddleLine => {
            point.x = 0;
            point.y = mode
                .viewport_offset
                .saturating_add(page.saturating_sub(1) / 2)
                .min(total - 1);
        }
        CopyModeAction::BottomLine => {
            point.x = 0;
            point.y = mode
                .viewport_offset
                .saturating_add(page.saturating_sub(1))
                .min(total - 1);
        }
        CopyModeAction::StartOfLine => {
            point.y = reader_logical_line_start(&mode.revision, point.y);
            point.x = 0;
        }
        CopyModeAction::BackToIndentation => {
            point.y = reader_logical_line_start(&mode.revision, point.y);
            point.x = revision_first_non_whitespace(&mode.revision, point.y);
        }
        CopyModeAction::EndOfLine => {
            point.y = reader_logical_line_end(&mode.revision, point.y);
            point.x = revision_line_length(&mode.revision, point.y);
        }
        CopyModeAction::NextWordEnd => {
            point = move_revision_word_end(&mode.revision, point, Some(word_separators), vi);
        }
        CopyModeAction::NextSpaceEnd => {
            point = move_revision_word_end(&mode.revision, point, None, vi);
        }
        CopyModeAction::NextWord
        | CopyModeAction::PreviousWord
        | CopyModeAction::NextSpace
        | CopyModeAction::PreviousSpace => {
            point = move_revision_word(&mode.revision, point, action, word_separators);
        }
        CopyModeAction::PreviousMatchingBracket => {
            let latched_vi = mode.mode_keys_vi_at_entry;
            if let Some(target) = revision_matching_bracket(&mode.revision, point, true, latched_vi)
            {
                point = target;
            } else if !latched_vi {
                point = move_revision_word(
                    &mode.revision,
                    point,
                    &CopyModeAction::PreviousWord,
                    &WordSeparators::new(CLOSING_BRACKETS),
                );
            }
        }
        CopyModeAction::NextMatchingBracket => {
            let latched_vi = mode.mode_keys_vi_at_entry;
            if let Some(target) =
                revision_matching_bracket(&mode.revision, point, false, latched_vi)
            {
                point = target;
            } else if !latched_vi {
                point = move_revision_word_end(
                    &mode.revision,
                    point,
                    Some(&WordSeparators::new(OPENING_BRACKETS)),
                    latched_vi,
                );
            }
        }
        CopyModeAction::NextParagraph => {
            point.y = revision_paragraph_target(&mode.revision, point.y, 1);
        }
        CopyModeAction::PreviousParagraph => {
            point.y = revision_paragraph_target(&mode.revision, point.y, -1);
        }
        CopyModeAction::NextPrompt { output } => {
            if let Some(target) =
                revision_semantic_prompt_target(&mode.revision, point.y, 1, *output)
            {
                point = target;
            }
        }
        CopyModeAction::PreviousPrompt { output } => {
            if let Some(target) =
                revision_semantic_prompt_target(&mode.revision, point.y, -1, *output)
            {
                point = target;
            }
        }
        CopyModeAction::JumpToMark => {
            if let Some(mark) = mode.mark {
                point = mark;
            }
        }
        _ => return,
    }
    place_copy_cursor(mode, point, vi);
    reveal_copy_cursor(mode);
}

fn scroll_copy_cursor(mode: &mut CopyModeState, up: bool, vi: bool) {
    let previous = mode.viewport_offset;
    let page = u32::from(mode.revision.viewport_rows.max(1));
    if up {
        mode.viewport_offset = previous.saturating_sub(1);
        if mode.viewport_offset < previous
            && (!vi || mode.cursor.y >= previous.saturating_add(page.saturating_sub(1)))
        {
            mode.cursor.y = mode.cursor.y.saturating_sub(1);
        }
    } else {
        mode.viewport_offset = previous
            .saturating_add(1)
            .min(mode.revision.maximum_offset());
        if mode.viewport_offset > previous && (!vi || mode.cursor.y <= previous) {
            mode.cursor.y = mode
                .cursor
                .y
                .saturating_add(1)
                .min(mode.revision.total_rows().saturating_sub(1));
        }
    }
}

fn page_copy_cursor(mode: &mut CopyModeState, up: bool, half: bool, vi: bool) {
    let rows = u32::from(mode.revision.viewport_rows);
    let length = revision_line_length(&mode.revision, mode.cursor.y);
    if mode.cursor.x != length {
        mode.last_cx = mode.cursor.x;
        mode.last_sx = length;
    }
    let step = match (rows > 2, half) {
        (false, _) => 1,
        (true, true) => rows / 2,
        (true, false) => rows - 2,
    };
    let history = mode.revision.maximum_offset();
    let mut from_bottom = history.saturating_sub(mode.viewport_offset);
    let mut row = mode.cursor.y.saturating_sub(mode.viewport_offset);
    if up {
        if from_bottom.saturating_add(step) > history {
            from_bottom = history;
            row = row.saturating_sub(step);
        } else {
            from_bottom += step;
        }
    } else if from_bottom < step {
        from_bottom = 0;
        row = row.saturating_add(step).min(rows.saturating_sub(1));
    } else {
        from_bottom -= step;
    }
    mode.viewport_offset = history - from_bottom;
    let y = mode
        .viewport_offset
        .saturating_add(row)
        .min(mode.revision.total_rows().saturating_sub(1));
    let x = mode.last_cx;
    if !(mode.selection.is_some() && mode.rectangle) {
        let end = revision_line_length(&mode.revision, y);
        if (x >= mode.last_sx && x != end) || x > end {
            place_copy_cursor(mode, PointCoordinate { x: end, y }, vi);
            return;
        }
    }
    mode.cursor = PointCoordinate { x, y };
}

pub(super) fn scroll_copy_view_to_cursor(mode: &mut CopyModeState) {
    let rows = u32::from(mode.revision.viewport_rows);
    let history = mode.revision.maximum_offset();
    let y = mode.cursor.y;
    if y >= mode.viewport_offset && y < mode.viewport_offset.saturating_add(rows) {
        return;
    }
    let gap = rows / 4;
    mode.viewport_offset = if y < rows {
        0
    } else if y > history.saturating_add(rows).saturating_sub(gap) {
        history
    } else {
        y.saturating_add(gap).saturating_sub(rows).min(history)
    };
}

pub(super) fn reveal_copy_cursor(mode: &mut CopyModeState) {
    let page = u32::from(mode.revision.viewport_rows);
    if mode.cursor.y < mode.viewport_offset {
        mode.viewport_offset = mode.cursor.y;
    } else if mode.cursor.y >= mode.viewport_offset.saturating_add(page) {
        mode.viewport_offset = mode.cursor.y.saturating_sub(page.saturating_sub(1));
    }
    mode.viewport_offset = mode.viewport_offset.min(mode.revision.maximum_offset());
}

fn revision_copy_line_end(revision: &ModeRevision, row: u32) -> u16 {
    for column in (0..revision.columns).rev() {
        if !revision_cell_is_whitespace(revision, PointCoordinate { x: column, y: row }) {
            return column;
        }
    }
    0
}

fn revision_line_length(revision: &ModeRevision, row: u32) -> u16 {
    for column in (0..revision.columns).rev() {
        if !revision_cell_is_whitespace(revision, PointCoordinate { x: column, y: row }) {
            return column.saturating_add(1);
        }
    }
    0
}

pub(super) fn copy_cursor_limit(
    revision: &ModeRevision,
    row: u32,
    vi: bool,
    allow_onemore: bool,
) -> u16 {
    let length = revision_line_length(revision, row);
    if allow_onemore || !vi {
        length
    } else {
        length.saturating_sub(1)
    }
}

fn copy_cursor_clamp(mode: &CopyModeState, point: PointCoordinate, vi: bool) -> PointCoordinate {
    let y = point.y.min(mode.revision.total_rows().saturating_sub(1));
    if mode.rectangle {
        return PointCoordinate {
            x: point.x.min(mode.revision.columns),
            y,
        };
    }
    PointCoordinate {
        x: point.x.min(copy_cursor_limit(&mode.revision, y, vi, false)),
        y,
    }
}

pub(super) fn place_copy_cursor(mode: &mut CopyModeState, point: PointCoordinate, vi: bool) {
    mode.cursor = copy_cursor_clamp(mode, point, vi);
}

fn revision_cell_is_padding(revision: &ModeRevision, point: PointCoordinate) -> bool {
    matches!(
        revision.cell(point).width(),
        CellWidth::SpacerTail | CellWidth::SpacerHead
    )
}

fn reader_cursor_right(
    revision: &ModeRevision,
    point: &mut PointCoordinate,
    wrap: bool,
    all: bool,
    onemore: bool,
) {
    let limit = if all {
        revision.columns
    } else {
        copy_cursor_limit(revision, point.y, !onemore, false)
    };
    if wrap && point.x >= limit && point.y.saturating_add(1) < revision.total_rows() {
        point.x = 0;
        point.y += 1;
    } else if point.x < limit {
        point.x += 1;
        while point.x < limit && revision_cell_is_padding(revision, *point) {
            point.x += 1;
        }
    }
}

fn reader_cursor_left(revision: &ModeRevision, point: &mut PointCoordinate, wrap: bool) {
    while point.x > 0 && revision_cell_is_padding(revision, *point) {
        point.x -= 1;
    }
    if point.x == 0 && point.y > 0 && (wrap || revision.row(point.y - 1).wrapped()) {
        point.y -= 1;
        while point.x > 0 && revision_cell_is_padding(revision, *point) {
            point.x -= 1;
        }
        point.x = revision_line_length(revision, point.y);
    } else if point.x > 0 {
        point.x -= 1;
    }
}

fn reader_logical_line_end(revision: &ModeRevision, row: u32) -> u32 {
    let mut row = row;
    let last = revision.total_rows().saturating_sub(1);
    while row < last && revision.row(row).wrapped() {
        row += 1;
    }
    row
}

fn reader_logical_line_start(revision: &ModeRevision, row: u32) -> u32 {
    let mut row = row;
    while row > 0 && revision.row(row - 1).wrapped() {
        row -= 1;
    }
    row
}

fn reader_handle_wrap(
    revision: &ModeRevision,
    point: &mut PointCoordinate,
    bound: &mut u16,
    last_row: u32,
) -> bool {
    while point.x > *bound {
        if point.y >= last_row {
            return false;
        }
        point.x = 0;
        point.y += 1;
        *bound = reader_row_bound(revision, point.y);
    }
    true
}

fn reader_row_bound(revision: &ModeRevision, row: u32) -> u16 {
    if revision.row(row).wrapped() {
        revision.columns.saturating_sub(1)
    } else {
        revision_line_length(revision, row)
    }
}

fn reader_next_word_end(
    revision: &ModeRevision,
    point: &mut PointCoordinate,
    separators: Option<&WordSeparators>,
) {
    let last_row = revision.total_rows().saturating_sub(1);
    let mut bound = reader_row_bound(revision, point.y);
    let is_blank = |point: PointCoordinate| revision_cell_is_whitespace(revision, point);
    let is_separator = |point: PointCoordinate| {
        separators.is_some_and(|separators| {
            revision
                .first_char(point)
                .is_some_and(|character| separators.contains_separator(character))
        })
    };
    while reader_handle_wrap(revision, point, &mut bound, last_row) {
        if is_blank(*point) {
            let Some(next) = point.x.checked_add(1) else {
                return;
            };
            point.x = next;
        } else if is_separator(*point) {
            loop {
                let Some(next) = point.x.checked_add(1) else {
                    return;
                };
                point.x = next;
                if !(reader_handle_wrap(revision, point, &mut bound, last_row)
                    && is_separator(*point)
                    && !is_blank(*point))
                {
                    return;
                }
            }
        } else {
            loop {
                let Some(next) = point.x.checked_add(1) else {
                    return;
                };
                point.x = next;
                if !(reader_handle_wrap(revision, point, &mut bound, last_row)
                    && !(is_blank(*point) || is_separator(*point)))
                {
                    return;
                }
            }
        }
    }
}

fn move_revision_word_end(
    revision: &ModeRevision,
    mut point: PointCoordinate,
    separators: Option<&WordSeparators>,
    vi: bool,
) -> PointCoordinate {
    if vi {
        if !revision_cell_is_whitespace(revision, point) {
            reader_cursor_right(revision, &mut point, false, false, false);
        }
        reader_next_word_end(revision, &mut point, separators);
        reader_cursor_left(revision, &mut point, true);
    } else {
        reader_next_word_end(revision, &mut point, separators);
    }
    point
}

fn move_copy_cursor_row(
    mode: &mut CopyModeState,
    point: &mut PointCoordinate,
    down: bool,
    vi: bool,
) {
    let rectangle_selection = mode.selection.is_some() && mode.rectangle;
    let length = revision_line_length(&mode.revision, point.y);
    if !rectangle_selection && point.x != length {
        mode.last_cx = point.x;
        mode.last_sx = length;
    }
    let total = mode.revision.total_rows();
    point.y = if down {
        point.y.saturating_add(1).min(total.saturating_sub(1))
    } else {
        point.y.saturating_sub(1)
    };
    if rectangle_selection {
        return;
    }
    point.x = mode.last_cx;
    *point = copy_cursor_clamp(mode, *point, vi);
    let end = revision_line_length(&mode.revision, point.y);
    if (point.x >= mode.last_sx && point.x != end) || point.x > end {
        point.x = end;
    }
}

const CLOSING_BRACKETS: &str = "}])";
const OPENING_BRACKETS: &str = "{[(";

fn revision_matching_bracket(
    revision: &ModeRevision,
    point: PointCoordinate,
    backward: bool,
    vi: bool,
) -> Option<PointCoordinate> {
    let columns = u64::from(revision.columns);
    let total = columns.saturating_mul(u64::from(revision.total_rows()));
    let to_point = |index: u64| PointCoordinate {
        x: u16::try_from(index % columns).unwrap_or(0),
        y: u32::try_from(index / columns).unwrap_or(u32::MAX),
    };
    let char_at = |index: u64| revision.first_char(to_point(index));
    let origin = u64::from(point.y)
        .saturating_mul(columns)
        .saturating_add(u64::from(point.x))
        .min(total.saturating_sub(1));

    let mut bracket = origin;
    let is_target = |character: Option<char>| {
        if backward {
            matches!(character, Some(')' | ']' | '}'))
        } else if vi {
            matches!(character, Some('(' | ')' | '[' | ']' | '{' | '}'))
        } else {
            matches!(character, Some('(' | '[' | '{'))
        }
    };
    if !vi {
        if is_target(char_at(origin)) {
            bracket = origin;
        } else {
            let neighbour = if backward {
                (point.x > 0).then(|| origin.saturating_sub(1))
            } else {
                (point.x.saturating_add(1) < revision.columns).then(|| origin.saturating_add(1))
            };
            let neighbour = neighbour.filter(|index| is_target(char_at(*index)))?;
            bracket = neighbour;
        }
    }
    while !is_target(char_at(bracket)) {
        let at = to_point(bracket);
        let reaches_further = if backward {
            at.x > 0 || (at.y > 0 && revision.row(at.y).continuation())
        } else {
            at.x.saturating_add(1) < revision.columns
                || (revision.row(at.y).wrapped() && at.y.saturating_add(1) < revision.total_rows())
        };
        if !reaches_further {
            return None;
        }
        bracket = if backward {
            bracket.saturating_sub(1)
        } else {
            bracket.saturating_add(1)
        };
    }

    let found = char_at(bracket)?;
    let (open, close, forward) = match found {
        '(' => ('(', ')', true),
        '[' => ('[', ']', true),
        '{' => ('{', '}', true),
        ')' => ('(', ')', false),
        ']' => ('[', ']', false),
        '}' => ('{', '}', false),
        _ => return None,
    };
    let mut depth = 1_u32;
    if forward {
        for index in bracket.saturating_add(1)..total {
            match char_at(index) {
                Some(character) if character == open => depth = depth.saturating_add(1),
                Some(character) if character == close => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(to_point(index));
                    }
                }
                _ => {}
            }
        }
    } else {
        for index in (0..bracket).rev() {
            match char_at(index) {
                Some(character) if character == close => depth = depth.saturating_add(1),
                Some(character) if character == open => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(to_point(index));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn revision_first_non_whitespace(revision: &ModeRevision, row: u32) -> u16 {
    (0..revision.columns)
        .find(|column| {
            !revision_cell_is_whitespace(revision, PointCoordinate { x: *column, y: row })
        })
        .unwrap_or(0)
}

fn revision_row_is_blank(revision: &ModeRevision, row: u32) -> bool {
    (0..revision.columns)
        .all(|column| revision_cell_is_whitespace(revision, PointCoordinate { x: column, y: row }))
}

pub(super) fn revision_cell_is_whitespace(revision: &ModeRevision, point: PointCoordinate) -> bool {
    revision.first_char(point).is_none_or(char::is_whitespace)
}

fn revision_paragraph_target(revision: &ModeRevision, row: u32, direction: i8) -> u32 {
    let total = revision.total_rows();
    if direction.is_negative() {
        let mut target = row.saturating_sub(1);
        while target > 0 && revision_row_is_blank(revision, target) {
            target -= 1;
        }
        while target > 0 && !revision_row_is_blank(revision, target - 1) {
            target -= 1;
        }
        target
    } else {
        let mut target = row.saturating_add(1).min(total - 1);
        while target + 1 < total && !revision_row_is_blank(revision, target) {
            target += 1;
        }
        while target + 1 < total && revision_row_is_blank(revision, target) {
            target += 1;
        }
        target
    }
}

fn mode_logical_line_bounds(
    revision: &ModeRevision,
    row: u32,
) -> (PointCoordinate, PointCoordinate) {
    let mut start = row;
    while start > 0 && revision.row(start).continuation() {
        start -= 1;
    }
    let mut end = row;
    while end + 1 < revision.total_rows() && revision.row(end).wrapped() {
        end += 1;
    }
    (
        PointCoordinate { x: 0, y: start },
        PointCoordinate {
            x: revision.columns.saturating_sub(1),
            y: end,
        },
    )
}

pub(super) fn revision_word_class(
    revision: &ModeRevision,
    point: PointCoordinate,
    word_separators: &WordSeparators,
) -> CopyWordClass {
    character_word_class(revision.first_char(point), word_separators)
}

pub(super) fn character_word_class(
    character: Option<char>,
    word_separators: &WordSeparators,
) -> CopyWordClass {
    match character {
        None => CopyWordClass::Whitespace,
        Some(character) if character.is_whitespace() => CopyWordClass::Whitespace,
        Some(character) if word_separators.contains_separator(character) => {
            CopyWordClass::Separator
        }
        Some(_) => CopyWordClass::Word,
    }
}

fn move_revision_word(
    revision: &ModeRevision,
    point: PointCoordinate,
    action: &CopyModeAction,
    word_separators: &WordSeparators,
) -> PointCoordinate {
    let columns = u64::from(revision.columns);
    let total = columns
        .saturating_mul(u64::from(revision.total_rows()))
        .max(1);
    let index = u64::from(point.y)
        .saturating_mul(columns)
        .saturating_add(u64::from(point.x))
        .min(total - 1);
    let spaces_only = matches!(
        action,
        CopyModeAction::NextSpace | CopyModeAction::PreviousSpace | CopyModeAction::NextSpaceEnd
    );
    let class_at = |index: u64| {
        let point = PointCoordinate {
            x: u16::try_from(index % columns).unwrap_or(0),
            y: u32::try_from(index / columns).unwrap_or(u32::MAX),
        };
        if spaces_only {
            match revision.first_char(point) {
                None => CopyWordClass::Whitespace,
                Some(character) if character.is_whitespace() => CopyWordClass::Whitespace,
                Some(_) => CopyWordClass::Word,
            }
        } else {
            revision_word_class(revision, point, word_separators)
        }
    };
    let destination = match action {
        CopyModeAction::NextWord | CopyModeAction::NextSpace => {
            let current = class_at(index);
            let mut next = index;
            if current != CopyWordClass::Whitespace {
                next = next.saturating_add(1).min(total - 1);
                while next < total && class_at(next) == current {
                    next += 1;
                }
            }
            while next < total && class_at(next) == CopyWordClass::Whitespace {
                next += 1;
            }
            next.min(total - 1)
        }
        CopyModeAction::PreviousWord | CopyModeAction::PreviousSpace => {
            let mut previous = index.saturating_sub(1);
            while previous > 0 && class_at(previous) == CopyWordClass::Whitespace {
                previous -= 1;
            }
            let class = class_at(previous);
            while previous > 0 && class_at(previous - 1) == class {
                previous -= 1;
            }
            previous
        }
        CopyModeAction::NextWordEnd | CopyModeAction::NextSpaceEnd => {
            let mut end = index.saturating_add(1).min(total - 1);
            while end + 1 < total && class_at(end) == CopyWordClass::Whitespace {
                end += 1;
            }
            let class = class_at(end);
            while end + 1 < total && class_at(end + 1) == class {
                end += 1;
            }
            end
        }
        _ => index,
    };
    PointCoordinate {
        x: u16::try_from(destination % columns).unwrap_or(0),
        y: u32::try_from(destination / columns).unwrap_or(u32::MAX),
    }
}

fn revision_semantic_prompt_target(
    revision: &ModeRevision,
    current_row: u32,
    direction: i8,
    output: bool,
) -> Option<PointCoordinate> {
    let total = revision.total_rows();
    let prompt_row = if direction.is_negative() {
        (0..current_row)
            .rev()
            .find(|row| revision.row(*row).prompt())
    } else {
        (current_row.saturating_add(1)..total).find(|row| revision.row(*row).prompt())
    }?;
    if output {
        return revision_semantic_output_target(revision, prompt_row);
    }
    Some(PointCoordinate {
        x: (0..revision.columns)
            .find(|column| {
                revision.is_prompt(PointCoordinate {
                    x: *column,
                    y: prompt_row,
                })
            })
            .unwrap_or(0),
        y: prompt_row,
    })
}

fn revision_semantic_output_target(
    revision: &ModeRevision,
    prompt_row: u32,
) -> Option<PointCoordinate> {
    let mut saw_input = false;
    for row in prompt_row..revision.total_rows() {
        if row > prompt_row && revision.row(row).prompt() {
            break;
        }
        for column in 0..revision.columns {
            let point = PointCoordinate { x: column, y: row };
            if revision.first_char(point).is_none() {
                continue;
            }
            if revision.is_input(point) {
                saw_input = true;
            } else if saw_input && revision.is_output(point) {
                return Some(point);
            }
        }
    }
    None
}

#[cfg(test)]
pub(super) fn semantic_prompt_target(
    terminal: &Terminal<'_, '_>,
    current_row: u32,
    direction: i8,
    output: bool,
) -> Result<Option<PointCoordinate>, WorkerError> {
    let total = u32::try_from(terminal.total_rows()?).unwrap_or(u32::MAX);
    let prompt_row = if direction.is_negative() {
        let mut row = current_row;
        let mut found = None;
        while row > 0 {
            row -= 1;
            if row_semantic_prompt(terminal, row)? == RowSemanticPrompt::Prompt {
                found = Some(row);
                break;
            }
        }
        found
    } else {
        let mut row = current_row.saturating_add(1);
        let mut found = None;
        while row < total {
            if row_semantic_prompt(terminal, row)? == RowSemanticPrompt::Prompt {
                found = Some(row);
                break;
            }
            row += 1;
        }
        found
    };
    let Some(prompt_row) = prompt_row else {
        return Ok(None);
    };
    if output {
        return semantic_output_target(terminal, prompt_row, total);
    }
    Ok(Some(PointCoordinate {
        x: first_semantic_cell(terminal, prompt_row, CellSemanticContent::Prompt)?.unwrap_or(0),
        y: prompt_row,
    }))
}

fn semantic_output_target(
    terminal: &Terminal<'_, '_>,
    prompt_row: u32,
    total: u32,
) -> Result<Option<PointCoordinate>, WorkerError> {
    let columns = terminal.cols()?;
    let mut saw_input = false;
    for row in prompt_row..total {
        if row > prompt_row && row_semantic_prompt(terminal, row)? == RowSemanticPrompt::Prompt {
            break;
        }
        for column in 0..columns {
            let grid_ref =
                terminal.grid_ref(Point::Screen(PointCoordinate { x: column, y: row }))?;
            let cell = grid_ref.cell()?;
            match cell.semantic_content()? {
                CellSemanticContent::Input if cell.has_text()? => saw_input = true,
                CellSemanticContent::Output if saw_input && cell.has_text()? => {
                    return Ok(Some(PointCoordinate { x: column, y: row }));
                }
                CellSemanticContent::Output
                | CellSemanticContent::Input
                | CellSemanticContent::Prompt => {}
            }
        }
    }
    Ok(None)
}

pub(super) fn row_semantic_prompt(
    terminal: &Terminal<'_, '_>,
    row: u32,
) -> Result<RowSemanticPrompt, WorkerError> {
    Ok(terminal
        .grid_ref(Point::Screen(PointCoordinate { x: 0, y: row }))?
        .row()?
        .semantic_prompt()?)
}

#[cfg(test)]
fn first_semantic_cell(
    terminal: &Terminal<'_, '_>,
    row: u32,
    semantic: CellSemanticContent,
) -> Result<Option<u16>, WorkerError> {
    for column in 0..terminal.cols()? {
        let grid_ref = terminal.grid_ref(Point::Screen(PointCoordinate { x: column, y: row }))?;
        let cell = grid_ref.cell()?;
        if cell.has_text()? && cell.semantic_content()? == semantic {
            return Ok(Some(column));
        }
    }
    Ok(None)
}

pub(super) fn select_semantic_output_at(
    terminal: &Terminal<'_, '_>,
    selection_state: &mut Option<SelectionState>,
    event: PointerCellEvent,
) -> Result<bool, WorkerError> {
    let clicked = terminal.grid_ref(viewport_point(terminal, event)?)?;
    let Some(clicked) = terminal.point_from_grid_ref(&clicked, PointSpace::Screen)? else {
        return Ok(false);
    };
    let mut prompt_row = clicked.y;
    loop {
        if row_semantic_prompt(terminal, prompt_row)? == RowSemanticPrompt::Prompt {
            break;
        }
        if prompt_row == 0 {
            return Ok(false);
        }
        prompt_row -= 1;
    }
    let total = u32::try_from(terminal.total_rows()?).unwrap_or(u32::MAX);
    let Some(start) = semantic_output_target(terminal, prompt_row, total)? else {
        return Ok(false);
    };
    let columns = terminal.cols()?;
    let mut end = None;
    for row in start.y..total {
        if row > start.y && row_semantic_prompt(terminal, row)? == RowSemanticPrompt::Prompt {
            break;
        }
        let first_column = if row == start.y { start.x } else { 0 };
        for column in first_column..columns {
            let grid_ref =
                terminal.grid_ref(Point::Screen(PointCoordinate { x: column, y: row }))?;
            let cell = grid_ref.cell()?;
            if cell.has_text()? && cell.semantic_content()? == CellSemanticContent::Output {
                end = Some(PointCoordinate { x: column, y: row });
            }
        }
    }
    let Some(end) = end else {
        return Ok(false);
    };
    let anchor = terminal.track_grid_ref(Point::Screen(start))?;
    let focus = terminal.track_grid_ref(Point::Screen(end))?;
    let start_ref = terminal.grid_ref(Point::Screen(start))?;
    let end_ref = terminal.grid_ref(Point::Screen(end))?;
    terminal.set_selection(Some(&Selection::new(start_ref, end_ref, false)))?;
    *selection_state = Some(SelectionState {
        anchor,
        focus,
        mode: SelectionMode::Cell,
        rectangle: false,
    });
    Ok(true)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CopyWordClass {
    Whitespace,
    Word,
    Separator,
}

#[cfg(test)]
pub(super) fn copy_line_end(
    terminal: &Terminal<'_, '_>,
    row: u32,
    columns: u16,
) -> Result<u16, WorkerError> {
    for column in (0..columns).rev() {
        if copy_word_class(terminal, PointCoordinate { x: column, y: row })?
            != CopyWordClass::Whitespace
        {
            return Ok(column);
        }
    }
    Ok(0)
}

#[cfg(test)]
pub(super) fn move_copy_word(
    terminal: &Terminal<'_, '_>,
    point: PointCoordinate,
    columns: u16,
    rows: u32,
    action: &CopyModeAction,
) -> Result<PointCoordinate, WorkerError> {
    let columns_u64 = u64::from(columns);
    let total = columns_u64.saturating_mul(u64::from(rows)).max(1);
    let index = u64::from(point.y)
        .saturating_mul(columns_u64)
        .saturating_add(u64::from(point.x))
        .min(total - 1);
    let class_at = |index: u64| {
        copy_word_class(
            terminal,
            PointCoordinate {
                x: u16::try_from(index % columns_u64).unwrap_or(0),
                y: u32::try_from(index / columns_u64).unwrap_or(u32::MAX),
            },
        )
    };

    let destination = match action {
        CopyModeAction::NextWord => {
            let current = class_at(index)?;
            let mut next = index;
            if current != CopyWordClass::Whitespace {
                next = next.saturating_add(1).min(total - 1);
                while next < total && class_at(next)? == current {
                    next += 1;
                }
            }
            while next < total && class_at(next)? == CopyWordClass::Whitespace {
                next += 1;
            }
            next.min(total - 1)
        }
        CopyModeAction::PreviousWord => {
            let mut previous = index.saturating_sub(1);
            while previous > 0 && class_at(previous)? == CopyWordClass::Whitespace {
                previous -= 1;
            }
            let class = class_at(previous)?;
            while previous > 0 && class_at(previous - 1)? == class {
                previous -= 1;
            }
            previous
        }
        CopyModeAction::NextWordEnd => {
            let mut end = index.saturating_add(1).min(total - 1);
            while end + 1 < total && class_at(end)? == CopyWordClass::Whitespace {
                end += 1;
            }
            let class = class_at(end)?;
            while end + 1 < total && class_at(end + 1)? == class {
                end += 1;
            }
            end
        }
        _ => index,
    };
    Ok(PointCoordinate {
        x: u16::try_from(destination % columns_u64).unwrap_or(0),
        y: u32::try_from(destination / columns_u64).unwrap_or(u32::MAX),
    })
}

#[cfg(test)]
fn copy_word_class(
    terminal: &Terminal<'_, '_>,
    point: PointCoordinate,
) -> Result<CopyWordClass, WorkerError> {
    let grid_ref = terminal.grid_ref(Point::Screen(point))?;
    let mut stack = ['\0'; 8];
    let first = match grid_ref.graphemes(&mut stack) {
        Ok(count) => stack
            .get(..count)
            .and_then(|graphemes| graphemes.first())
            .copied(),
        Err(libghostty_vt::Error::OutOfSpace { required }) => {
            let mut graphemes = vec!['\0'; required];
            let count = grid_ref.graphemes(&mut graphemes)?;
            graphemes
                .get(..count)
                .and_then(|values| values.first())
                .copied()
        }
        Err(error) => return Err(error.into()),
    };
    Ok(match first {
        None => CopyWordClass::Whitespace,
        Some(character) if character.is_whitespace() => CopyWordClass::Whitespace,
        Some(character) if WordSeparators::default().contains_separator(character) => {
            CopyWordClass::Separator
        }
        Some(_) => CopyWordClass::Word,
    })
}

pub(super) fn update_copy_selection(
    mode: &mut CopyModeState,
    word_separators: Option<&WordSeparators>,
) {
    let unit = mode.selection_mode;
    let origin = mode.selection_origin;
    let cursor = mode.cursor;
    let revision = Arc::clone(&mode.revision);
    let Some(selection) = mode.selection.as_mut() else {
        return;
    };
    selection.focus = cursor;
    match unit {
        CopySelectionMode::Char => {}
        CopySelectionMode::Word => {
            let Some(separators) = word_separators else {
                return;
            };
            if (cursor.y, cursor.x) >= (origin.y, origin.x) {
                selection.anchor = mode_word_edge(&revision, origin, separators, false);
                selection.focus = mode_word_edge(&revision, cursor, separators, true);
            } else {
                selection.anchor = mode_word_edge(&revision, origin, separators, true);
                selection.focus = mode_word_edge(&revision, cursor, separators, false);
            }
            selection.mode = SelectionMode::Word;
        }
        CopySelectionMode::Line => {
            let (origin_start, origin_end) = mode_logical_line_bounds(&revision, origin.y);
            let (cursor_start, cursor_end) = mode_logical_line_bounds(&revision, cursor.y);
            if cursor.y >= origin.y {
                selection.anchor = origin_start;
                selection.focus = cursor_end;
            } else {
                selection.anchor = origin_end;
                selection.focus = cursor_start;
            }
            selection.mode = SelectionMode::Line;
        }
    }
}

pub(super) fn format_selection_text(terminal: &Terminal<'_, '_>) -> Result<String, WorkerError> {
    let options = FormatOptions::new()
        .with_emit_format(Format::Plain)
        .with_unwrap(true)
        .with_trim(true);
    Ok(terminal
        .format_selection_alloc(None, options)?
        .map_or_else(String::new, |bytes| {
            String::from_utf8_lossy(bytes.as_ref()).into_owned()
        }))
}
