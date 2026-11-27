use super::*;

impl Renderer {
    pub(super) fn place_active_cursor(&mut self, model: &Model) {
        if model.menu.is_some() || model.confirm.is_some() {
            self.hide_cursor();
            return;
        }
        if model.sidebar_edit.is_some() {
            self.place_sidebar_edit_cursor(model);
            return;
        }
        if model.sidebar_focused() {
            self.hide_cursor();
            return;
        }
        if model.display_panes.is_some() {
            write_cursor_position(&mut self.output, 0, 0);
            self.hide_cursor();
            return;
        }
        if let Some((prompt, content)) = pane_prompt_target(model) {
            let view = crate::overlay::prompt_view(
                &prompt.prompt,
                &prompt.input,
                prompt.cursor,
                content.width,
            );
            write_cursor_position(
                &mut self.output,
                content
                    .x
                    .saturating_add(view.cursor.min(content.width.saturating_sub(1))),
                pane_prompt_row_y(model, content),
            );
            self.output.extend_from_slice(b"\x1b[?25h");
            return;
        }
        if let Some(prompt) = &model.command_prompt {
            let column = prompt
                .prompt
                .chars()
                .count()
                .saturating_add(usize::try_from(prompt.cursor).unwrap_or(usize::MAX));
            let mut column = u16::try_from(column).unwrap_or(u16::MAX);
            let (start, width, row) = if model.sidebar_visible() {
                (0, sidebar::WIDTH, model.size.rows.saturating_sub(1))
            } else {
                let Some(row) = model.message_row_y() else {
                    self.hide_cursor();
                    return;
                };
                let (x, width) = model.status_area();
                column = crate::overlay::prompt_view(
                    &prompt.prompt,
                    &prompt.input,
                    prompt.cursor,
                    width,
                )
                .cursor;
                (x, width, row)
            };
            write_cursor_position(
                &mut self.output,
                start.saturating_add(column.min(width.saturating_sub(1))),
                row,
            );
            self.output.extend_from_slice(b"\x1b[?25h");
            return;
        }
        if model.command_output_search.is_some()
            && model.command_output_focus().is_some()
            && let Some((pane, viewport)) = &model.command_output
        {
            let rect = model.command_output_content_rect();
            self.place_command_output_cursor(*pane, viewport, rect, model);
            return;
        }
        self.place_pane_cursor(model);
        if client_message_hides_the_cursor(model) {
            self.hide_cursor();
        }
    }
    pub(super) fn place_pane_cursor(&mut self, model: &Model) {
        let Some(pane) = model.active_pane() else {
            self.hide_cursor();
            return;
        };
        let Some(entry) = model.pane_rect(pane) else {
            self.hide_cursor();
            return;
        };
        if let Some(mode) = model
            .pane_snapshot(pane)
            .and_then(|pane| pane.mode.as_ref())
        {
            let content = entry.content();
            let surface = pane_mode::surface(mode, entry.mode_rect(), &model.status.theme);
            let (Some(column), Some(row)) = (
                surface
                    .cursor
                    .0
                    .checked_sub(entry.source.0)
                    .filter(|column| entry.whole.is_none() || *column < content.width),
                surface
                    .cursor
                    .1
                    .checked_sub(entry.source.1)
                    .filter(|row| entry.whole.is_none() || *row < content.height),
            ) else {
                self.move_to(0, 0);
                self.hide_cursor();
                return;
            };
            let (column, row) = (
                content.x.saturating_add(column),
                content.y.saturating_add(row),
            );
            write_cursor_position(&mut self.output, column, row);
            if surface.cursor_visible && !model.covered_above(pane, column, row) {
                self.output.extend_from_slice(b"\x1b[?25h");
            } else {
                self.hide_cursor();
            }
            return;
        }
        let Some(viewport) = model.pane_viewport(pane) else {
            self.hide_cursor();
            return;
        };
        if let Some(gutter) = crate::mode_view::presentation(model, pane, viewport)
            .and_then(|mode| crate::mode_view::line_number_gutter(mode, viewport))
        {
            let rect = entry.content();
            match gutter.cursor {
                Some((column, row)) if row < rect.height => {
                    write_cursor_position(
                        &mut self.output,
                        rect.x
                            .saturating_add(gutter.cursor_column(column, rect.width)),
                        rect.y.saturating_add(row),
                    );
                    self.output.extend_from_slice(b"\x1b[?25h");
                }
                _ => self.hide_cursor(),
            }
            return;
        }
        let cropped = source_viewport(viewport, entry.source);
        if viewport.cursor.is_some() && cropped.cursor.is_none() {
            self.move_to(0, 0);
            self.hide_cursor();
            return;
        }
        self.place_viewport_cursor(pane, &cropped, entry.content(), model);
    }
    pub(super) fn place_menu_cursor(&mut self, model: &Model) {
        if let Some(state) = model.menu.as_ref()
            && let Some(layout) = resolve_floating(
                FloatingSpec {
                    left: state.left,
                    top: state.top,
                    width: state.width,
                    height: state.height,
                    client_columns: state.client_columns,
                    client_rows: state.client_rows,
                    border_lines: state.border_lines,
                },
                Rect {
                    x: 0,
                    y: 0,
                    width: model.size.columns,
                    height: model.size.rows,
                },
            )
        {
            let row = model.menu_selection.map_or(layout.frame.y, |choice| {
                layout
                    .frame
                    .y
                    .saturating_add(1)
                    .saturating_add(u16::try_from(choice).unwrap_or(u16::MAX))
            });
            write_cursor_position(&mut self.output, layout.frame.x.saturating_add(2), row);
        }
        self.hide_cursor();
    }
    pub(super) fn place_confirm_cursor(&mut self, model: &Model) {
        let Some(confirm) = model.confirm.as_ref() else {
            self.hide_cursor();
            return;
        };
        if model.sidebar_visible() && model.command_output.is_none() {
            self.hide_cursor();
            return;
        }
        let Some(row) = model.message_row_y() else {
            self.hide_cursor();
            return;
        };
        let (x, width) = if model.command_output.is_some() {
            (0, model.size.columns)
        } else {
            model.status_area()
        };
        let column = crate::overlay::prompt_view(&confirm.prompt, "", 0, width).cursor;
        write_cursor_position(
            &mut self.output,
            x.saturating_add(column.min(width.saturating_sub(1))),
            row,
        );
        self.output.extend_from_slice(b"\x1b[?25h");
    }
    fn place_command_output_cursor(
        &mut self,
        pane: PaneId,
        viewport: &TerminalViewport,
        rect: Rect,
        model: &Model,
    ) {
        let Some(query) = &model.command_output_search else {
            self.place_viewport_cursor(pane, viewport, rect, model);
            return;
        };
        let Some(row) = model.message_row_y() else {
            self.hide_cursor();
            return;
        };
        let (_, column) = command_output_search_display(query, model.size.columns);
        write_cursor_position(&mut self.output, column, row);
        self.output.extend_from_slice(b"\x1b[?25h");
    }
    fn place_sidebar_edit_cursor(&mut self, model: &Model) {
        let Some(edit) = &model.sidebar_edit else {
            self.hide_cursor();
            return;
        };
        let Some(row) = model.sidebar_edit_row() else {
            self.hide_cursor();
            return;
        };
        let Some(visible_row) = row.checked_sub(model.sidebar.scroll) else {
            self.hide_cursor();
            return;
        };
        if visible_row >= usize::from(model.sidebar_tree_height()) || !model.sidebar_visible() {
            self.hide_cursor();
            return;
        }
        let (_, column) = edit.viewport(sidebar::WIDTH);
        write_cursor_position(
            &mut self.output,
            column,
            u16::try_from(visible_row).unwrap_or(u16::MAX),
        );
        self.output.extend_from_slice(b"\x1b[?25h");
    }
    pub(super) fn place_viewport_cursor(
        &mut self,
        pane: PaneId,
        viewport: &TerminalViewport,
        rect: Rect,
        model: &Model,
    ) {
        let covered = |column: u16, row: u16| {
            model.covered_above(
                pane,
                rect.x.saturating_add(column),
                rect.y.saturating_add(row),
            )
        };
        if let Some(overlay) = copy_cursor_overlay(viewport) {
            if overlay.start < rect.width
                && overlay.row < rect.height
                && covered(overlay.start, overlay.row)
            {
                self.move_to(
                    rect.x.saturating_add(overlay.start),
                    rect.y.saturating_add(overlay.row),
                );
                self.hide_cursor();
                return;
            }
            if overlay.start >= rect.width || overlay.row >= rect.height {
                self.hide_cursor();
                return;
            }
            write_cursor_position(
                &mut self.output,
                rect.x.saturating_add(overlay.start),
                rect.y.saturating_add(overlay.row),
            );
            self.output.extend_from_slice(b"\x1b[?25h");
            return;
        }
        let Some(cursor) = viewport.cursor else {
            self.hide_cursor();
            return;
        };
        let column = cursor
            .column()
            .saturating_sub(u16::from(cursor.at_wide_tail()));
        if column >= rect.width || cursor.row() >= rect.height {
            self.hide_cursor();
            return;
        }
        if covered(column, cursor.row()) {
            self.move_to(
                rect.x.saturating_add(column),
                rect.y.saturating_add(cursor.row()),
            );
            self.hide_cursor();
            return;
        }
        self.move_to(
            rect.x.saturating_add(column),
            rect.y.saturating_add(cursor.row()),
        );
        if cursor.visible() {
            self.show_cursor();
        } else {
            self.hide_cursor();
        }
    }
    pub(super) fn hide_cursor(&mut self) {
        let state = self.tty_state();
        self.output.extend_from_slice(b"\x1b[?25l");
        if let Some(state) = state {
            self.set_tty_state(TtyState {
                visible: false,
                ..state
            });
        }
    }
}

fn copy_cursor_overlay(viewport: &TerminalViewport) -> Option<OverlaySpan> {
    viewport
        .overlays
        .iter()
        .copied()
        .find(|overlay| overlay.kind() == OverlayKind::CopyCursor)
}

pub(super) fn prompt_cursor(model: &Model) -> zz_protocol::PromptCursor {
    if model.menu.is_some() {
        return zz_protocol::PromptCursor::default();
    }
    if let Some(confirm) = &model.confirm {
        return confirm.prompt_cursor;
    }
    if let Some(prompt) = model
        .command_prompt
        .as_ref()
        .filter(|prompt| prompt.pane.is_none())
    {
        return prompt.prompt_cursor;
    }
    mode_prompt_cursor(model).unwrap_or_default()
}

fn mode_prompt_cursor(model: &Model) -> Option<zz_protocol::PromptCursor> {
    let chooser_prompt = |prompt: &str, search: bool| !prompt.is_empty() || search;
    if let Some(state) = &model.choose_tree {
        return chooser_prompt(&state.prompt, state.search.is_some())
            .then_some(model.chooser_presentation.as_ref())
            .flatten()
            .map(|presentation| presentation.prompt_cursor);
    }
    if let Some(state) = &model.choose_buffer {
        return chooser_prompt(&state.prompt, state.search.is_some())
            .then_some(model.chooser_presentation.as_ref())
            .flatten()
            .map(|presentation| presentation.prompt_cursor);
    }
    match model.pane_snapshot(model.active_pane()?)?.mode.as_ref()? {
        zz_protocol::PaneMode::Customize {
            presentation,
            prompt,
            ..
        } if !prompt.is_empty() => Some(presentation.prompt_cursor),
        zz_protocol::PaneMode::Switch { prompt_shape, .. } => Some(*prompt_shape),
        _ => None,
    }
}

pub(super) fn cursor_colour_rgb(
    colour: TmuxColour,
    theme: &zz_protocol::ThemeColours,
) -> Option<u32> {
    match colour {
        TmuxColour::Basic(index) | TmuxColour::Indexed(index) => {
            Some(zz_protocol::indexed_colour_rgb(index))
        }
        TmuxColour::Rgb(rgb) => Some(rgb),
        TmuxColour::Theme(index) => theme
            .0
            .get(usize::from(index))
            .copied()
            .filter(|slot| !matches!(slot, TmuxColour::Theme(_)))
            .and_then(|slot| cursor_colour_rgb(slot, theme)),
        TmuxColour::Default | TmuxColour::Terminal => None,
    }
}
