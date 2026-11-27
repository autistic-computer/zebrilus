use super::*;

impl Renderer {
    pub(super) fn reconcile_kitty_images(&mut self, model: &Model) {
        let browser_placements = &self.browser_placements;
        let cropped: HashMap<PaneId, Vec<zz_terminal::KittyPlacement>> = model
            .layout
            .panes
            .iter()
            .filter(|entry| entry.source != (0, 0))
            .filter_map(|entry| {
                let viewport = model.pane_viewport(entry.pane)?;
                Some((
                    entry.pane,
                    source_placements(&viewport.kitty_placements, entry.source),
                ))
            })
            .collect();
        let tiled = model.layout.tiled().len();
        let panes = model
            .layout
            .panes
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let front = if index < tiled { 0 } else { index - tiled + 1 };
                let covered = model.layout.floats.get(front..).is_some_and(|floats| {
                    floats
                        .iter()
                        .any(|float| rects_overlap(float.frame, entry.content()))
                });
                if covered {
                    return None;
                }
                let pane = model.pane_snapshot(entry.pane)?;
                match &pane.kind {
                    PaneKindSnapshot::Terminal => {
                        let viewport = model.pane_viewport(entry.pane)?;
                        let placements = cropped
                            .get(&entry.pane)
                            .map_or_else(|| viewport.kitty_placements.as_ref(), Vec::as_slice);
                        Some((entry.pane, entry.content(), placements))
                    }
                    PaneKindSnapshot::Browser(_) => {
                        let placement = browser_placements.get(&entry.pane)?;
                        Some((entry.pane, entry.content(), std::slice::from_ref(placement)))
                    }
                    _ => None,
                }
            });
        self.kitty.reconcile(panes, &mut self.output);
    }
    pub(super) fn paint_terminal(
        &mut self,
        pane: PaneId,
        viewport: &TerminalViewport,
        rect: Rect,
        force: bool,
        damage: Option<&FrameDamage>,
    ) {
        let defaults = [self.terminal_defaults.fg, self.terminal_defaults.bg];
        if !force
            && self.painted.get(&pane).is_some_and(|previous| {
                previous.rect == rect
                    && previous.defaults == defaults
                    && previous.viewport.columns == viewport.columns
                    && previous.viewport.rows == viewport.rows
                    && previous.viewport.foreground == viewport.foreground
                    && previous.viewport.background == viewport.background
                    && Arc::ptr_eq(&previous.viewport.cells, &viewport.cells)
                    && Arc::ptr_eq(&previous.viewport.overlays, &viewport.overlays)
                    && Arc::ptr_eq(&previous.viewport.dictionary, &viewport.dictionary)
            })
        {
            return;
        }
        let previous = self.painted.remove(&pane);
        let structural_change = previous.as_ref().is_none_or(|previous| {
            previous.rect != rect
                || previous.defaults != defaults
                || previous.viewport.columns != viewport.columns
                || previous.viewport.rows != viewport.rows
                || previous.viewport.foreground != viewport.foreground
                || previous.viewport.background != viewport.background
        });
        if force || structural_change {
            for row in 0..rect.height {
                self.blit_row(viewport, row, rect);
            }
        } else if let Some(FrameDamage::Rows(rows)) = damage {
            for row in rows.iter().copied().filter(|row| *row < rect.height) {
                if let Some(columns) = previous.as_ref().map_or_else(
                    || Some(0..rect.width),
                    |previous| changed_columns(&previous.viewport, viewport, row, rect.width),
                ) {
                    self.blit_columns(viewport, row, rect, columns);
                }
            }
        } else {
            for row in 0..rect.height {
                if let Some(columns) = previous.as_ref().map_or_else(
                    || Some(0..rect.width),
                    |previous| changed_columns(&previous.viewport, viewport, row, rect.width),
                ) {
                    self.blit_columns(viewport, row, rect, columns);
                }
            }
        }
        self.painted.insert(
            pane,
            PaintedPane {
                viewport: viewport.clone(),
                rect,
                defaults,
            },
        );
    }
    fn write_match_sgr(&mut self, matched: u8) -> bool {
        let Some(style) = matched
            .checked_sub(1)
            .and_then(|index| self.match_styles.get(usize::from(index)))
            .and_then(Option::as_ref)
        else {
            return false;
        };
        self.output.extend_from_slice(b"\x1b[0m");
        write_selection_sgr(&mut self.output, style);
        true
    }
    pub(super) fn write_terminal_sgr(
        &mut self,
        style: PackedStyle,
        reverse: bool,
        viewport: &TerminalViewport,
        reset: bool,
    ) -> bool {
        let key = (
            style,
            reverse,
            [viewport.foreground, viewport.background],
            [self.terminal_defaults.fg, self.terminal_defaults.bg],
            crate::tty::terminal_colours(),
        );
        if self
            .terminal_sgr
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            let mut bytes = self
                .terminal_sgr
                .take()
                .map_or_else(Vec::new, |(_, mut bytes)| {
                    bytes.clear();
                    bytes
                });
            write_sgr(
                &mut bytes,
                style,
                reverse,
                viewport.foreground,
                viewport.background,
                &self.terminal_defaults,
            );
            self.terminal_sgr = Some((key, bytes));
        }
        let Some((_, bytes)) = &self.terminal_sgr else {
            return false;
        };
        let equivalent = sgr_is_reset(bytes);
        if !(reset && equivalent) {
            self.output.extend_from_slice(bytes);
        }
        equivalent
    }
    pub(super) fn blit_row(&mut self, viewport: &TerminalViewport, row: u16, rect: Rect) {
        self.blit_columns(viewport, row, rect, 0..rect.width);
    }
    fn blit_columns(
        &mut self,
        viewport: &TerminalViewport,
        row: u16,
        rect: Rect,
        columns: std::ops::Range<u16>,
    ) {
        if rect.width == 0 {
            return;
        }
        self.overlay_mask.resize(usize::from(rect.width), false);
        self.overlay_mask.fill(false);
        self.selection_mask.resize(usize::from(rect.width), false);
        self.selection_mask.fill(false);
        self.match_mask.resize(usize::from(rect.width), 0);
        self.match_mask.fill(0);
        for overlay in viewport
            .overlays
            .iter()
            .filter(|overlay| overlay.row == row && overlay.kind() != OverlayKind::CopyCursor)
        {
            let start = usize::from(overlay.start.min(rect.width));
            let end = usize::from(overlay.end.min(rect.width));
            let matched = match overlay.kind() {
                OverlayKind::SearchMatch => 1,
                OverlayKind::SearchCurrent => 2,
                _ => 0,
            };
            if matched != 0 && self.match_styles[usize::from(matched - 1)].is_some() {
                for cell in &mut self.match_mask[start..end] {
                    *cell = (*cell).max(matched);
                }
            }
            if overlay.kind() == OverlayKind::Selection && self.selection_style.is_some() {
                let end = match self.selection_trim {
                    Some((trim_row, trim_end)) if trim_row == row => {
                        end.min(usize::from(trim_end)).max(start)
                    }
                    _ => end,
                };
                self.selection_mask[start..end].fill(true);
            } else {
                self.overlay_mask[start..end].fill(true);
            }
        }
        let selection_style = self.selection_style.take();
        let line_style = self
            .current_line
            .as_ref()
            .filter(|(line, _)| *line == row)
            .map(|(_, style)| style.clone());

        let default_style = viewport.styles().first().copied().unwrap_or_else(|| {
            PackedStyle::new(
                viewport.foreground,
                viewport.background,
                None,
                0,
                UnderlineStyle::None,
            )
        });
        let grounded_defaults =
            self.terminal_defaults.fg.is_some() || self.terminal_defaults.bg.is_some();
        let clear_from = if grounded_defaults || columns.end < rect.width || line_style.is_some() {
            None
        } else {
            trailing_clear(
                viewport,
                row,
                rect.width,
                default_style,
                &self.overlay_mask,
                &self.selection_mask,
            )
            .map(|start| start.max(columns.start))
        };
        let ground = match default_style.background_class() {
            ColourClass::Default => None,
            ColourClass::Resolved if default_style.background() == viewport.background => None,
            class => Some(class),
        };
        let erase_from = clear_from.filter(|_| !self.blank_is_default || ground.is_some());
        if clear_from == Some(0) && erase_from.is_none() {
            self.selection_style = selection_style;
            return;
        }
        self.move_to(
            rect.x.saturating_add(columns.start),
            rect.y.saturating_add(row),
        );
        let entry = self.tty_state();
        let mut sgr_reset = entry.is_some_and(|state| state.sgr_reset);
        let visible = entry.is_some_and(|state| state.visible);
        let mut current_style = None;
        let mut terminal_column = columns.start;
        for column in columns.start..clear_from.unwrap_or(columns.end) {
            let cell = viewport.cell(row, column).unwrap_or(PackedCell::EMPTY);
            let style = viewport.style(cell).unwrap_or(default_style);
            let reverse = self.overlay_mask[usize::from(column)];
            let selected = self.selection_mask[usize::from(column)];
            let matched = self.match_mask[usize::from(column)];
            if matches!(cell.width(), CellWidth::SpacerTail | CellWidth::SpacerHead) {
                if terminal_column <= column {
                    if terminal_column != column {
                        write_cursor_position(
                            &mut self.output,
                            rect.x.saturating_add(column),
                            rect.y.saturating_add(row),
                        );
                    }
                    if current_style != Some((style, reverse, selected, matched)) {
                        sgr_reset = self.write_terminal_sgr(style, reverse, viewport, sgr_reset);
                        if let Some(line) = &line_style {
                            write_line_sgr(&mut self.output, line);
                            sgr_reset = false;
                        }
                        if self.write_match_sgr(matched) {
                            sgr_reset = false;
                        }
                        if selected && let Some(selection) = &selection_style {
                            write_selection_sgr(&mut self.output, selection);
                            sgr_reset = false;
                        }
                        current_style = Some((style, reverse, selected, matched));
                    }
                    self.output.push(b' ');
                    terminal_column = column.saturating_add(1);
                }
                continue;
            }
            if terminal_column != column {
                write_cursor_position(
                    &mut self.output,
                    rect.x.saturating_add(column),
                    rect.y.saturating_add(row),
                );
            }
            if current_style != Some((style, reverse, selected, matched)) {
                sgr_reset = self.write_terminal_sgr(style, reverse, viewport, sgr_reset);
                if let Some(line) = &line_style {
                    write_line_sgr(&mut self.output, line);
                    sgr_reset = false;
                }
                if self.write_match_sgr(matched) {
                    sgr_reset = false;
                }
                if selected && let Some(selection) = &selection_style {
                    write_selection_sgr(&mut self.output, selection);
                    sgr_reset = false;
                }
                current_style = Some((style, reverse, selected, matched));
            }
            let advance = if cell.width() == CellWidth::Wide {
                2
            } else {
                1
            };
            if advance == 2 && column.saturating_add(1) >= rect.width {
                self.output.push(b' ');
                terminal_column = column.saturating_add(1);
                continue;
            }
            write_glyph(&mut self.output, viewport.glyph(cell), advance);
            terminal_column = column.saturating_add(advance);
        }
        let mut reset = !sgr_reset;
        if let Some(start) = erase_from {
            if terminal_column != start {
                write_cursor_position(
                    &mut self.output,
                    rect.x.saturating_add(start),
                    rect.y.saturating_add(row),
                );
            }
            self.output.extend_from_slice(b"\x1b[0m");
            reset = ground.is_some();
            if let Some(class) = ground {
                write_cell_ground(
                    &mut self.output,
                    class,
                    default_style.background(),
                    viewport.background,
                    Ground::Background,
                );
            }
            self.output.extend_from_slice(b"\x1b[");
            write_decimal(&mut self.output, rect.width - start);
            self.output.push(b'X');
            terminal_column = start;
        }
        if reset {
            self.output.extend_from_slice(b"\x1b[0m");
        }
        self.set_tty_state(TtyState {
            cursor: (terminal_column < rect.width).then(|| {
                (
                    rect.x.saturating_add(terminal_column),
                    rect.y.saturating_add(row),
                )
            }),
            sgr_reset: true,
            visible,
        });
        self.selection_style = selection_style;
    }
    fn paint_line_numbers(
        &mut self,
        mode: &zz_protocol::ModePresentation,
        gutter: crate::mode_view::LineNumberGutter,
        rect: Rect,
        model: &Model,
    ) {
        let width = gutter.width.min(rect.width);
        for row in 0..rect.height {
            let (text, current) = gutter.label(row);
            let segment =
                crate::mode_view::line_number_segment(mode, text, current, &model.status.theme);
            let mut line = StyledLine::from_segments(vec![segment]);
            line.resolve_theme(&model.status.theme);
            let line = line.truncate(usize::from(width));
            write_styled_text(
                &mut self.output,
                rect.x,
                rect.y.saturating_add(row),
                &line,
                model.appearance.foreground,
                model.appearance.background,
                &model.appearance,
            );
        }
    }
    pub(super) fn paint_copy_chrome(
        &mut self,
        mode: &zz_protocol::ModePresentation,
        gutter: Option<crate::mode_view::LineNumberGutter>,
        viewport: &TerminalViewport,
        rect: Rect,
        model: &Model,
    ) {
        let Some(gutter) = gutter else {
            self.paint_mode_position(mode, viewport, rect, model);
            return;
        };
        self.paint_line_numbers(mode, gutter, rect, model);
        self.paint_mode_position(mode, viewport, gutter.body(rect), model);
        if let Some((column, row)) = gutter.cursor
            && row < rect.height
            && column >= gutter.content_width(rect.width)
        {
            let line = StyledLine::from_segments(vec![StyledSegment {
                text: "$".to_owned(),
                style: TmuxStyle {
                    fg: Some(TmuxColour::Default),
                    bg: Some(TmuxColour::Default),
                    ..TmuxStyle::default()
                },
            }]);
            write_styled_text(
                &mut self.output,
                rect.x.saturating_add(rect.width.saturating_sub(1)),
                rect.y.saturating_add(row),
                &line,
                model.appearance.foreground,
                model.appearance.background,
                &model.appearance,
            );
        }
    }
    fn paint_mode_position(
        &mut self,
        mode: &zz_protocol::ModePresentation,
        viewport: &TerminalViewport,
        rect: Rect,
        model: &Model,
    ) {
        if rect.height == 0 {
            return;
        }
        self.blit_row(viewport, 0, rect);
        for (column, segments) in crate::mode_view::position_runs(mode, rect.width) {
            let mut line = StyledLine::from_segments(segments);
            line.resolve_theme(&model.status.theme);
            write_styled_text(
                &mut self.output,
                rect.x.saturating_add(column),
                rect.y,
                &line,
                viewport.foreground,
                viewport.background,
                &model.appearance,
            );
        }
    }
}

pub(super) fn changed_columns(
    previous: &TerminalViewport,
    current: &TerminalViewport,
    row: u16,
    width: u16,
) -> Option<std::ops::Range<u16>> {
    if (!Arc::ptr_eq(&previous.dictionary, &current.dictionary)
        && previous.dictionary.as_ref() != current.dictionary.as_ref())
        || previous
            .overlays
            .iter()
            .filter(|overlay| overlay.row == row)
            .ne(current.overlays.iter().filter(|overlay| overlay.row == row))
    {
        return Some(0..width);
    }
    let (Some(before), Some(after)) = (
        viewport_row(previous, row, width),
        viewport_row(current, row, width),
    ) else {
        return Some(0..width);
    };
    if before.len() != after.len() {
        return Some(0..width);
    }
    let mut cells = before.iter().zip(after);
    let start = cells.position(|(before, after)| before.bits() != after.bits())?;
    let end = cells
        .rposition(|(before, after)| before.bits() != after.bits())
        .map_or(start + 1, |last| start + last + 2);
    if before[start..end]
        .iter()
        .chain(&after[start..end])
        .any(|cell| cell.width() != CellWidth::Narrow)
    {
        Some(0..width)
    } else {
        Some(u16::try_from(start).ok()?..u16::try_from(end).ok()?)
    }
}

fn viewport_row(viewport: &TerminalViewport, row: u16, width: u16) -> Option<&[PackedCell]> {
    viewport
        .row(row)
        .map(|cells| &cells[..cells.len().min(usize::from(width))])
}

pub(super) fn source_placements(
    placements: &[zz_terminal::KittyPlacement],
    source: (u16, u16),
) -> Vec<zz_terminal::KittyPlacement> {
    placements
        .iter()
        .filter_map(|placement| {
            let mut placement = placement.clone();
            placement.viewport_col -= i32::from(source.0);
            placement.viewport_row -= i32::from(source.1);
            (placement.viewport_col >= 0 && placement.viewport_row >= 0).then_some(placement)
        })
        .collect()
}
