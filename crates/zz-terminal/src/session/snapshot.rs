use super::*;

pub(super) fn refresh_copy_revision(
    terminal: &mut Terminal<'_, '_>,
    selection: &mut Option<SelectionState>,
    mode: &mut CopyModeState,
    mode_keys_vi: bool,
) -> Result<(), WorkerError> {
    let rows = u32::from(mode.revision.viewport_rows.saturating_sub(1));
    let follow = mode.viewport_offset == mode.revision.maximum_offset()
        && mode.cursor.y == mode.viewport_offset.saturating_add(rows);
    let offset_from_top = mode.viewport_offset;
    mode.revision = ModeRevision::capture(terminal)?;
    if follow {
        mode.viewport_offset = mode.revision.maximum_offset();
        mode.cursor = mode.revision.clamp_point(PointCoordinate {
            x: mode.cursor.x,
            y: mode
                .viewport_offset
                .saturating_add(u32::from(mode.revision.viewport_rows.saturating_sub(1))),
        });
        mode.cursor.x = copy_cursor_limit(&mode.revision, mode.cursor.y, mode_keys_vi, false);
    } else {
        mode.viewport_offset = offset_from_top.min(mode.revision.maximum_offset());
        mode.cursor = mode.revision.clamp_point(mode.cursor);
    }
    mode.recentre = None;
    *selection = None;
    terminal.set_selection(None)?;
    mode.selection = None;
    mode.selection_mode = CopySelectionMode::Char;
    mode.selecting = false;
    let limit = copy_cursor_limit(&mode.revision, mode.cursor.y, mode_keys_vi, mode.rectangle);
    mode.cursor.x = mode.cursor.x.min(limit);
    Ok(())
}

pub(super) fn copy_mode_facts(
    mode: &CopyModeState,
    search: Option<&SearchState>,
    word_separators: &WordSeparators,
) -> CopyModeFacts {
    let cursor = PointCoordinate {
        x: mode.cursor.x,
        y: mode
            .cursor
            .y
            .min(mode.revision.total_rows().saturating_sub(1)),
    };
    let reader = mode.revision.reader();
    CopyModeFacts {
        view_mode: mode.kind == FrozenModeKind::View,
        cursor_x: u32::from(cursor.x),
        cursor_y: cursor.y.saturating_sub(mode.viewport_offset),
        cursor_line: format_grid_line(&reader, cursor.y),
        cursor_word: format_grid_word(&reader, cursor, word_separators),
        scroll_position: mode
            .revision
            .maximum_offset()
            .saturating_sub(mode.viewport_offset),
        selection: mode.selection.map(|selection| CopyModeSelectionFacts {
            start_x: u32::from(selection.anchor.x),
            start_y: selection.anchor.y,
            end_x: u32::from(selection.focus.x),
            end_y: selection.focus.y,
        }),
        search_present: mode.search_marks,
        search_count: mode.search_count,
        search_timed_out: false,
        search_match: if mode.search_marks {
            copy_mode_search_match(mode, search, cursor)
        } else {
            String::new()
        },
        rectangle_toggle: mode.rectangle,
        selection_active: mode.selection.is_some() && mode.selecting,
        line_numbers: mode.line_numbers,
        refresh_active: mode.refresh,
    }
}

pub(super) fn copy_mode_search_match(
    mode: &CopyModeState,
    search: Option<&SearchState>,
    cursor: PointCoordinate,
) -> String {
    let contains = |search: &SearchState, point: PointCoordinate| {
        search
            .matches
            .iter()
            .find(|found| found.contains(point, mode.revision.columns))
            .copied()
    };
    let Some(found) = search.and_then(|search| {
        contains(search, cursor).or_else(|| {
            let back = if cursor.x > 0 {
                PointCoordinate {
                    x: cursor.x - 1,
                    y: cursor.y,
                }
            } else {
                PointCoordinate {
                    x: mode.revision.columns.saturating_sub(1),
                    y: cursor.y.checked_sub(1)?,
                }
            };
            contains(search, back)
        })
    }) else {
        return String::new();
    };
    let mut text = String::new();
    let reader = mode.revision.reader();
    for row in found.row..=found.end_row {
        let Some((start, end)) = found.span(row, mode.revision.columns) else {
            continue;
        };
        for column in start..end {
            let point = PointCoordinate { x: column, y: row };
            if matches!(
                reader.cell(point).width(),
                CellWidth::SpacerTail | CellWidth::SpacerHead
            ) {
                continue;
            }
            reader.push_text(point, &mut text);
        }
    }
    text
}

trait FormatGrid {
    fn grid_columns(&self) -> u16;
    fn grid_rows(&self) -> u32;
    fn grid_row_wrapped(&self, row: u32) -> bool;
    fn grid_cell_width(&self, point: PointCoordinate) -> CellWidth;
    fn grid_first_char(&self, point: PointCoordinate) -> Option<char>;
    fn grid_push_text(&self, point: PointCoordinate, output: &mut String);
}

impl FormatGrid for mode_revision::ModeRevisionReader<'_> {
    fn grid_columns(&self) -> u16 {
        self.columns()
    }

    fn grid_rows(&self) -> u32 {
        self.total_rows()
    }

    fn grid_row_wrapped(&self, row: u32) -> bool {
        self.row_meta(row).wrapped()
    }

    fn grid_cell_width(&self, point: PointCoordinate) -> CellWidth {
        self.cell(point).width()
    }

    fn grid_first_char(&self, point: PointCoordinate) -> Option<char> {
        self.first_char(point)
    }

    fn grid_push_text(&self, point: PointCoordinate, output: &mut String) {
        self.push_text(point, output);
    }
}

impl FormatGrid for ModeRevision {
    fn grid_columns(&self) -> u16 {
        self.columns
    }

    fn grid_rows(&self) -> u32 {
        self.total_rows()
    }

    fn grid_row_wrapped(&self, row: u32) -> bool {
        self.row(row).wrapped()
    }

    fn grid_cell_width(&self, point: PointCoordinate) -> CellWidth {
        self.cell(point).width()
    }

    fn grid_first_char(&self, point: PointCoordinate) -> Option<char> {
        self.first_char(point)
    }

    fn grid_push_text(&self, point: PointCoordinate, output: &mut String) {
        self.push_cell_text(self.cell(point), output);
    }
}

fn format_grid_line_length<G: FormatGrid + ?Sized>(grid: &G, row: u32) -> u32 {
    (0..grid.grid_columns())
        .rev()
        .find(|column| {
            let point = PointCoordinate { x: *column, y: row };
            !matches!(grid.grid_first_char(point), None | Some(' '))
        })
        .map_or(0, |column| u32::from(column).saturating_add(1))
}

fn format_grid_line<G: FormatGrid + ?Sized>(grid: &G, row: u32) -> String {
    let mut text = String::new();
    for column in 0..format_grid_line_length(grid, row) {
        let point = PointCoordinate {
            x: u16::try_from(column).unwrap_or(u16::MAX),
            y: row,
        };
        grid.grid_push_text(point, &mut text);
    }
    text
}

fn format_grid_is_word_separator<G: FormatGrid + ?Sized>(
    grid: &G,
    point: PointCoordinate,
    word_separators: &WordSeparators,
) -> bool {
    if matches!(
        grid.grid_cell_width(point),
        CellWidth::SpacerTail | CellWidth::SpacerHead
    ) {
        return false;
    }
    match grid.grid_first_char(point) {
        None => true,
        Some(character) => {
            character == ' ' || character == '\t' || word_separators.contains_separator(character)
        }
    }
}

fn format_grid_word<G: FormatGrid + ?Sized>(
    grid: &G,
    cursor: PointCoordinate,
    word_separators: &WordSeparators,
) -> String {
    let mut x = cursor.x;
    let mut y = cursor.y;
    let mut found = false;
    loop {
        if format_grid_is_word_separator(grid, PointCoordinate { x, y }, word_separators) {
            found = true;
            break;
        }
        if x == 0 {
            if y == 0 {
                break;
            }
            if !grid.grid_row_wrapped(y.saturating_sub(1)) {
                break;
            }
            y -= 1;
            let length = format_grid_line_length(grid, y);
            if length == 0 {
                break;
            }
            x = u16::try_from(length).unwrap_or(u16::MAX);
        }
        x -= 1;
    }
    let mut text = String::new();
    let last_row = grid.grid_rows().saturating_sub(1);
    loop {
        if found {
            let end = format_grid_line_length(grid, y);
            if end == 0 || u32::from(x) == end.saturating_sub(1) {
                if y == last_row || !grid.grid_row_wrapped(y) {
                    break;
                }
                y += 1;
                x = 0;
            } else {
                x += 1;
            }
        }
        found = true;
        let point = PointCoordinate { x, y };
        if matches!(
            grid.grid_cell_width(point),
            CellWidth::SpacerTail | CellWidth::SpacerHead
        ) {
            continue;
        }
        if format_grid_is_word_separator(grid, point, word_separators) {
            break;
        }
        grid.grid_push_text(point, &mut text);
    }
    text
}

pub(super) struct LiveGrid<'terminal, 'alloc: 'callbacks, 'callbacks> {
    terminal: &'terminal Terminal<'alloc, 'callbacks>,
    columns: u16,
    rows: u32,
}

impl<'terminal, 'alloc: 'callbacks, 'callbacks> LiveGrid<'terminal, 'alloc, 'callbacks> {
    pub(super) fn new(
        terminal: &'terminal Terminal<'alloc, 'callbacks>,
    ) -> Result<Self, WorkerError> {
        Ok(Self {
            terminal,
            columns: terminal.cols()?.max(1),
            rows: u32::try_from(terminal.total_rows()?)
                .unwrap_or(u32::MAX)
                .max(1),
        })
    }

    pub(super) fn active_base(&self) -> Result<u32, WorkerError> {
        Ok(self
            .rows
            .saturating_sub(u32::from(self.terminal.rows()?.max(1))))
    }

    fn reference(&self, point: PointCoordinate) -> Option<libghostty_vt::screen::GridRef<'_>> {
        if point.x >= self.columns || point.y >= self.rows {
            return None;
        }
        self.terminal.grid_ref(Point::Screen(point)).ok()
    }
}

impl FormatGrid for LiveGrid<'_, '_, '_> {
    fn grid_columns(&self) -> u16 {
        self.columns
    }

    fn grid_rows(&self) -> u32 {
        self.rows
    }

    fn grid_row_wrapped(&self, row: u32) -> bool {
        self.reference(PointCoordinate { x: 0, y: row })
            .and_then(|reference| reference.row().ok())
            .and_then(|row| row.is_wrapped().ok())
            .unwrap_or(false)
    }

    fn grid_cell_width(&self, point: PointCoordinate) -> CellWidth {
        self.reference(point)
            .and_then(|reference| reference.cell().ok())
            .and_then(|cell| cell.wide().ok())
            .map_or(CellWidth::Narrow, |wide| match wide {
                CellWide::Narrow => CellWidth::Narrow,
                CellWide::Wide => CellWidth::Wide,
                CellWide::SpacerTail => CellWidth::SpacerTail,
                CellWide::SpacerHead => CellWidth::SpacerHead,
            })
    }

    fn grid_first_char(&self, point: PointCoordinate) -> Option<char> {
        let codepoint = self.reference(point)?.cell().ok()?.codepoint().ok()?;
        char::from_u32(codepoint).filter(|character| *character != '\0')
    }

    fn grid_push_text(&self, point: PointCoordinate, output: &mut String) {
        if matches!(
            self.grid_cell_width(point),
            CellWidth::SpacerTail | CellWidth::SpacerHead
        ) {
            return;
        }
        let Some(reference) = self.reference(point) else {
            return;
        };
        let mut cluster = ['\0'; GRAPHEME_CLUSTER_SCRATCH];
        match reference.graphemes(&mut cluster) {
            Ok(length) => output.extend(cluster.iter().take(length)),
            Err(libghostty_vt::Error::OutOfSpace { required })
                if required > 0 && required <= MAX_GRAPHEME_CLUSTER_CODEPOINTS =>
            {
                let mut cluster = vec!['\0'; required];
                if let Ok(length) = reference.graphemes(&mut cluster) {
                    output.extend(cluster.iter().take(length));
                }
            }
            Err(_) => {}
        }
    }
}

pub(super) fn pointer_context(
    terminal: &Terminal<'_, '_>,
    mode: Option<&CopyModeState>,
    column: u16,
    row: u16,
    word_separators: &WordSeparators,
) -> Result<PointerContext, WorkerError> {
    let grid = LiveGrid::new(terminal)?;
    let row = u32::from(row);
    let point = PointCoordinate {
        x: column,
        y: grid.active_base()?.saturating_add(row),
    };
    let (word, line) = match mode {
        Some(mode) => {
            let frozen = PointCoordinate {
                x: column,
                y: mode.viewport_offset.saturating_add(row),
            };
            (
                format_grid_word(mode.revision.as_ref(), frozen, word_separators),
                format_grid_line(mode.revision.as_ref(), frozen.y),
            )
        }
        None => (
            format_grid_word(&grid, point, word_separators),
            format_grid_line(&grid, point.y),
        ),
    };
    Ok(PointerContext {
        word,
        line,
        hyperlink: pointer_hyperlink(&grid, point, &mut Vec::new())?,
    })
}

fn pointer_hyperlink(
    grid: &LiveGrid<'_, '_, '_>,
    mut point: PointCoordinate,
    scratch: &mut Vec<u8>,
) -> Result<String, WorkerError> {
    while matches!(
        grid.grid_cell_width(point),
        CellWidth::SpacerTail | CellWidth::SpacerHead
    ) {
        if point.x == 0 {
            return Ok(String::new());
        }
        point.x -= 1;
    }
    let Some(length) = hyperlink_uri_bytes_at(grid.terminal, Point::Screen(point), scratch)? else {
        return Ok(String::new());
    };
    Ok(std::str::from_utf8(&scratch[..length])
        .unwrap_or_default()
        .to_owned())
}

pub(super) fn snapshot<'alloc: 'callbacks, 'callbacks>(
    terminal: &Terminal<'alloc, 'callbacks>,
    render_state: &mut RenderState<'alloc>,
    rows: &mut RowIterator<'alloc>,
    cells: &mut CellIterator<'alloc>,
    generations: &mut ViewportGenerations,
    change: SnapshotChange,
    dictionary: &mut ViewportDictionary,
    view: Option<&TerminalViewState>,
    status: SessionStatus,
    bound: Option<(u16, u16)>,
) -> Result<TerminalViewport, WorkerError> {
    let started = diagnostic_timer();
    let result = build_snapshot(
        terminal,
        render_state,
        rows,
        cells,
        generations,
        change,
        dictionary,
        view,
        status,
        bound,
    );
    match &result {
        Ok(viewport) => log::trace!(
            target: "zz_terminal::diagnostics::snapshot",
            "build change={change:?} elapsed_us={} generation={} view_generation={} dictionary_generation={} columns={} rows={} cells={} cell_bytes={} styles={} graphemes={} grapheme_bytes={} overlays={} cursor={:?} scrollbar={:?} mode={:?} search={:?} unseen_output={} status={:?}",
            diagnostic_elapsed_us(started),
            viewport.generation,
            viewport.view_generation,
            viewport.dictionary_generation,
            viewport.columns,
            viewport.rows,
            viewport.cells.len(),
            std::mem::size_of_val(viewport.cells.as_ref()),
            viewport.dictionary.styles.len(),
            viewport.dictionary.grapheme_offsets.len(),
            viewport.dictionary.grapheme_bytes.len(),
            viewport.overlays.len(),
            viewport.cursor,
            viewport.scrollbar,
            viewport.mode,
            viewport.search,
            viewport.unseen_output,
            viewport.status,
        ),
        Err(error) => log::trace!(
            target: "zz_terminal::diagnostics::snapshot",
            "build change={change:?} elapsed_us={} error={error}",
            diagnostic_elapsed_us(started),
        ),
    }
    result
}

pub(super) fn bounded_extent((columns, rows): (u16, u16), bound: Option<(u16, u16)>) -> (u16, u16) {
    bound.map_or((columns, rows), |(most_columns, most_rows)| {
        (columns.min(most_columns.max(1)), rows.min(most_rows.max(1)))
    })
}

fn placement_starts_inside(placement: &KittyPlacement, columns: u16, rows: u16) -> bool {
    placement.viewport_col < i32::from(columns) && placement.viewport_row < i32::from(rows)
}

pub(super) fn reported_working_directory(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    Some(value.to_owned())
}

fn build_snapshot<'alloc: 'callbacks, 'callbacks>(
    terminal: &Terminal<'alloc, 'callbacks>,
    render_state: &mut RenderState<'alloc>,
    rows: &mut RowIterator<'alloc>,
    cells: &mut CellIterator<'alloc>,
    generations: &mut ViewportGenerations,
    change: SnapshotChange,
    dictionary: &mut ViewportDictionary,
    view: Option<&TerminalViewState>,
    status: SessionStatus,
    bound: Option<(u16, u16)>,
) -> Result<TerminalViewport, WorkerError> {
    if let Some(view) = view
        && let Some(copy_mode) = view.copy_mode.as_ref()
    {
        return Ok(copy_mode_snapshot(
            generations,
            change,
            dictionary,
            view,
            copy_mode,
            status,
            bound,
        ));
    }
    let copy_mode = view.and_then(|view| view.copy_mode.as_ref());
    let search = view.and_then(|view| view.search.as_ref());
    let hover_link = view.and_then(|view| view.hover_link.as_ref());
    let unseen_output = view.map_or(0, |view| view.unseen_output);
    render_state.set_clip(
        bound.map_or_else(Clip::default, |(most_columns, most_rows)| Clip {
            y: 0,
            rows: most_rows.max(1),
            cols: most_columns.max(1),
        }),
    )?;
    let snapshot = render_state.update(terminal)?;
    let dirty = snapshot.dirty()?;
    let full_dirty = dirty == Dirty::Full;
    let colors = snapshot.colors()?;
    let default_palette = terminal.default_color_palette()?.0;
    let hints = dictionary.class_hints.clone();
    let classes = Classifier::new(
        &hints,
        &colors.palette,
        &default_palette,
        [colors.foreground, colors.background],
    );
    let (columns, row_count) = bounded_extent((snapshot.cols()?, snapshot.rows()?), bound);
    let foreground = color(colors.foreground);
    let background = color(colors.background);
    let cursor = snapshot
        .cursor_viewport()?
        .filter(|position| position.x < columns && position.y < row_count)
        .map(|position| {
            Cursor::new(
                position.x,
                position.y,
                snapshot.cursor_visible().unwrap_or(true),
                snapshot.cursor_blinking().unwrap_or(false),
                position.at_wide_tail,
                match snapshot
                    .cursor_visual_style()
                    .unwrap_or(CursorVisualStyle::Block)
                {
                    CursorVisualStyle::Bar => CursorStyle::Bar,
                    CursorVisualStyle::Underline => CursorStyle::Underline,
                    CursorVisualStyle::BlockHollow => CursorStyle::BlockHollow,
                    _ => CursorStyle::Block,
                },
                color(colors.cursor.unwrap_or(colors.foreground)),
            )
        })
        .filter(|_| copy_mode.is_none());

    let default_style = classes.default_style(PackedStyle::new(
        foreground,
        background,
        None,
        0,
        UnderlineStyle::None,
    ));
    let cell_count = usize::from(columns).saturating_mul(usize::from(row_count));
    let previous_dictionary_generation = dictionary.generation;
    dictionary.ensure_default(default_style, &colors.palette);
    if dictionary.generation == previous_dictionary_generation && dictionary.should_compact_live() {
        dictionary.reset_live(default_style, &colors.palette);
    }
    let dictionary_reset = dictionary.generation != previous_dictionary_generation;
    let dimensions_changed =
        dictionary.shared_cells.len() != cell_count || dictionary.shared_columns != columns;
    let mut row_copy = std::mem::take(&mut dictionary.row_copy);
    let mut row_styles = std::mem::take(&mut dictionary.row_styles);
    let mut overlays = std::mem::take(&mut dictionary.overlay_scratch);
    let overlay_only = matches!(change, SnapshotChange::Overlay);
    let cell_plane_dirty =
        dimensions_changed || dictionary_reset || (!overlay_only && dirty != Dirty::Clean);
    let preserve_cells = !dimensions_changed && !dictionary_reset && !full_dirty;
    let mut next_cells =
        cell_plane_dirty.then(|| dictionary.acquire_cell_plane(cell_count, preserve_cells));
    let extraction_result = (|| -> Result<(), WorkerError> {
        let mut output_cells = next_cells
            .as_mut()
            .map(|cells| Arc::get_mut(cells).expect("acquired cell plane remains unique"));
        let mut visible_row = 0_u16;
        let mut row_iteration = rows.update(&snapshot)?;
        while visible_row < row_count
            && let Some(row) = row_iteration.next()
        {
            let row_start = usize::from(visible_row).saturating_mul(usize::from(columns));
            if let Some(selected) = row.selection()? {
                overlays.push(OverlaySpan::with_flags(
                    visible_row,
                    selected.start_x.min(columns),
                    selected.end_x.saturating_add(1).min(columns),
                    OverlayKind::Selection,
                    if view
                        .and_then(|view| view.selection.as_ref())
                        .is_some_and(|selection| selection.rectangle)
                    {
                        OVERLAY_RECTANGLE
                    } else {
                        0
                    },
                ));
            }
            let update_cells = output_cells.is_some()
                && (dimensions_changed || dictionary_reset || full_dirty || row.dirty()?);
            if update_cells {
                let row_end = row_start.saturating_add(usize::from(columns));
                let output_row = output_cells
                    .as_deref_mut()
                    .and_then(|cells| cells.get_mut(row_start..row_end))
                    .ok_or(WorkerError::ViewportMetadataTooLarge)?;
                cells
                    .update(row)?
                    .copy_trimmed_into(0, columns, &mut row_copy)?;
                let copied = row_copy.cells().len().min(output_row.len());
                output_row[copied..].fill(PackedCell::EMPTY);
                row_styles.clear();
                row_styles.resize(row_copy.style_count(), None);
                let mut previous = None;
                for (slot, cell) in output_row.iter_mut().zip(row_copy.cells().iter().copied()) {
                    let background = if let Some(index) = cell.bg_color_palette() {
                        CellBackground::Palette(index.0)
                    } else if let Some(value) = cell.bg_color_rgb() {
                        CellBackground::Rgb(value)
                    } else {
                        CellBackground::Style
                    };
                    let hyperlink = cell.has_hyperlink();
                    let style_index = cell.style_index();
                    let plain = background == CellBackground::Style && !hyperlink;
                    let cached = if plain {
                        row_styles.get(style_index).copied().flatten()
                    } else {
                        previous
                            .filter(|(key, _)| *key == (style_index, background, hyperlink))
                            .map(|(_, id)| id)
                    };
                    let style_id = if let Some(id) = cached {
                        id
                    } else {
                        let id = dictionary.intern_style(copied_cell_style(
                            &row_copy.style(style_index)?,
                            background,
                            hyperlink,
                            &colors,
                            &classes,
                        ));
                        if plain {
                            row_styles[style_index] = Some(id);
                        } else {
                            previous = Some(((style_index, background, hyperlink), id));
                        }
                        id
                    };
                    let glyph = match row_copy.grapheme(cell)? {
                        Some(text) => dictionary.encode_glyph(text),
                        None => cell.codepoint().map_or(0, u32::from),
                    };
                    let width = match cell.wide()? {
                        CellWide::Narrow => CellWidth::Narrow,
                        CellWide::Wide => CellWidth::Wide,
                        CellWide::SpacerTail => CellWidth::SpacerTail,
                        CellWide::SpacerHead => CellWidth::SpacerHead,
                    };
                    *slot = PackedCell::new(glyph, style_id, width);
                }
                row.set_dirty(false)?;
            }
            visible_row = visible_row.saturating_add(1);
        }
        Ok(())
    })();
    dictionary.row_copy = row_copy;
    dictionary.row_styles = row_styles;
    if let Err(error) = extraction_result {
        if let Some(cells) = next_cells {
            dictionary.retain_cell_plane(cells);
        }
        overlays.clear();
        dictionary.overlay_scratch = overlays;
        return Err(error);
    }
    if let Some(cells) = next_cells {
        dictionary.commit_cell_plane(cells);
        dictionary.shared_columns = columns;
    }
    snapshot.set_dirty(Dirty::Clean)?;

    if matches!(change, SnapshotChange::Content) {
        generations.content = generations.content.saturating_add(1);
    }
    generations.view = generations.view.saturating_add(1);
    let scrollbar = terminal.scrollbar()?;
    let scrollbar = ScrollbarState {
        total: u32::try_from(scrollbar.total).map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
        offset: u32::try_from(scrollbar.offset)
            .map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
        len: u32::try_from(scrollbar.len).map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
    };
    let kitty_placements = if let Some(kitty) = generations.kitty.as_mut() {
        kitty.placements(terminal, scrollbar)?
    } else {
        Arc::from([])
    };
    let kitty_placements = if kitty_placements
        .iter()
        .all(|placement| placement_starts_inside(placement, columns, row_count))
    {
        kitty_placements
    } else {
        kitty_placements
            .iter()
            .filter(|placement| placement_starts_inside(placement, columns, row_count))
            .cloned()
            .collect()
    };
    if let Some(search) = search {
        for (index, found) in search.matches.iter().enumerate() {
            let start_row = found.row.max(scrollbar.offset);
            let end_row = found.end_row.saturating_add(1).min(
                scrollbar
                    .offset
                    .saturating_add(scrollbar.len.min(u32::from(row_count))),
            );
            for row in start_row..end_row {
                let Some((start, end)) = found.span(row, columns) else {
                    continue;
                };
                overlays.push(OverlaySpan::new(
                    u16::try_from(row.saturating_sub(scrollbar.offset)).unwrap_or(u16::MAX),
                    start,
                    end,
                    if search.current == Some(index) {
                        OverlayKind::SearchCurrent
                    } else {
                        OverlayKind::SearchMatch
                    },
                ));
            }
        }
    }
    if let Some(link) = hover_link
        && link.row < row_count
        && link.start < link.end
    {
        overlays.push(OverlaySpan::new(
            link.row,
            link.start.min(columns),
            link.end.min(columns),
            OverlayKind::LinkHover,
        ));
    }
    let copy_cursor = copy_mode.and_then(|copy_mode| {
        let row = copy_mode.cursor.y.checked_sub(copy_mode.viewport_offset)?;
        (row < u32::from(row_count)).then_some(PointCoordinate {
            x: copy_mode.cursor.x,
            y: row,
        })
    });
    if let Some(point) = copy_cursor
        && point.y < u32::from(row_count)
    {
        overlays.push(OverlaySpan::new(
            u16::try_from(point.y).unwrap_or(u16::MAX),
            point.x.min(columns),
            point.x.saturating_add(1).min(columns),
            OverlayKind::CopyCursor,
        ));
    }
    let mode = if let Some(copy_mode) = copy_mode {
        let total = scrollbar.total.max(1);
        let position = copy_mode.cursor.y.saturating_add(1).min(total);
        match copy_mode.kind {
            FrozenModeKind::Copy => TerminalMode::Copy {
                position,
                total,
                hide_position: copy_mode.hide_position,
            },
            FrozenModeKind::View => TerminalMode::View { position, total },
        }
    } else {
        TerminalMode::Live
    };
    if dictionary_reset {
        dictionary.tune_live_compaction_limits(cell_count);
    }
    let shared_dictionary = dictionary.shared_dictionary();
    let title = terminal.title().unwrap_or("zz");
    let working_directory = terminal.pwd().ok().and_then(reported_working_directory);
    let presentation = dictionary.shared_presentation(
        title,
        working_directory.as_deref(),
        hover_link.map(|link| link.uri.as_str()),
    );
    let overlays = dictionary.finish_overlays(overlays);
    Ok(TerminalViewport {
        generation: generations.content,
        view_generation: generations.view,
        dictionary_generation: dictionary.generation,
        columns,
        rows: row_count,
        foreground,
        background,
        presentation,
        cells: Arc::clone(&dictionary.shared_cells),
        dictionary: shared_dictionary,
        overlays,
        kitty_placements,
        cursor,
        scrollbar,
        mode,
        search: search.map(|search| {
            SearchStatus::new(
                search
                    .current
                    .and_then(|current| u32::try_from(current.saturating_add(1)).ok())
                    .unwrap_or(0),
                u32::try_from(search.matches.len()).unwrap_or(u32::MAX),
            )
            .with_pending(search.pending)
            .with_invalid_pattern(search.invalid_pattern)
        }),
        unseen_output,
        kitty_keyboard: terminal
            .kitty_keyboard_flags()
            .is_ok_and(|flags| !flags.is_empty()),
        mouse_tracking: terminal.is_mouse_tracking()?,
        status,
    })
}

pub(super) fn copy_mode_snapshot(
    generations: &mut ViewportGenerations,
    change: SnapshotChange,
    dictionary: &mut ViewportDictionary,
    view: &TerminalViewState,
    mode: &CopyModeState,
    status: SessionStatus,
    bound: Option<(u16, u16)>,
) -> TerminalViewport {
    if matches!(change, SnapshotChange::Content) {
        generations.content = generations.content.saturating_add(1);
    }
    generations.view = generations.view.saturating_add(1);
    let revision = &mode.revision;
    let offset = mode.viewport_offset.min(revision.maximum_offset());
    let extent = bounded_extent((revision.columns, revision.viewport_rows), bound);
    let (columns, rows) = extent;
    let cells = dictionary.mode_cells(revision, offset, extent);
    let mut overlays = std::mem::take(&mut dictionary.overlay_scratch);
    if let Some(selection) = mode.selection {
        push_mode_selection_overlays(&mut overlays, extent, selection, offset);
    }
    if let Some(search) = view.search.as_ref().filter(|_| mode.search_marks) {
        let visible_end = offset.saturating_add(u32::from(rows));
        for (index, found) in search.matches.iter().enumerate() {
            let start_row = found.row.max(offset);
            let end_row = found.end_row.saturating_add(1).min(visible_end);
            for row in start_row..end_row {
                let Some((start, end)) = found.span(row, columns) else {
                    continue;
                };
                if start >= end {
                    continue;
                }
                overlays.push(OverlaySpan::new(
                    u16::try_from(row - offset).unwrap_or(u16::MAX),
                    start,
                    end,
                    if search.current == Some(index) {
                        OverlayKind::SearchCurrent
                    } else {
                        OverlayKind::SearchMatch
                    },
                ));
            }
        }
    }
    if let Some(row) = mode.cursor.y.checked_sub(offset)
        && row < u32::from(rows)
        && mode.cursor.x < columns
    {
        overlays.push(OverlaySpan::new(
            u16::try_from(row).unwrap_or(u16::MAX),
            mode.cursor.x,
            mode.cursor.x.saturating_add(1).min(columns),
            OverlayKind::CopyCursor,
        ));
    }
    let total = revision.total_rows().max(1);
    let overlays = dictionary.finish_overlays(overlays);
    let presentation = dictionary.shared_presentation(
        &revision.title,
        revision.working_directory.as_deref(),
        view.hover_link.as_ref().map(|link| link.uri.as_str()),
    );
    TerminalViewport {
        generation: generations.content,
        view_generation: generations.view,
        dictionary_generation: dictionary
            .generation
            .wrapping_add(revision.dictionary_generation()),
        columns,
        rows,
        foreground: revision.foreground,
        background: revision.background,
        presentation,
        cells,
        dictionary: revision.shared_dictionary(),
        overlays,
        kitty_placements: Arc::from([]),
        cursor: None,
        scrollbar: ScrollbarState {
            total,
            offset,
            len: u32::from(revision.viewport_rows),
        },
        mode: match mode.kind {
            FrozenModeKind::Copy => TerminalMode::Copy {
                position: mode.cursor.y.saturating_add(1).min(total),
                total,
                hide_position: mode.hide_position,
            },
            FrozenModeKind::View => TerminalMode::View {
                position: mode.cursor.y.saturating_add(1).min(total),
                total,
            },
        },
        search: view.search.as_ref().map(|search| {
            SearchStatus::new(
                search
                    .current
                    .and_then(|current| u32::try_from(current.saturating_add(1)).ok())
                    .unwrap_or(0),
                u32::try_from(search.matches.len()).unwrap_or(u32::MAX),
            )
            .with_pending(search.pending)
            .with_invalid_pattern(search.invalid_pattern)
        }),
        unseen_output: view.unseen_output,
        kitty_keyboard: false,
        mouse_tracking: false,
        status,
    }
}

fn push_mode_selection_overlays(
    overlays: &mut Vec<OverlaySpan>,
    (columns, rows): (u16, u16),
    selection: ModeSelection,
    offset: u32,
) {
    let (start, end) =
        if (selection.anchor.y, selection.anchor.x) <= (selection.focus.y, selection.focus.x) {
            (selection.anchor, selection.focus)
        } else {
            (selection.focus, selection.anchor)
        };
    let visible_end = offset.saturating_add(u32::from(rows));
    let first_row = start.y.max(offset);
    let last_row = end.y.min(visible_end.saturating_sub(1));
    if first_row > last_row {
        return;
    }
    for row in first_row..=last_row {
        let (span_start, span_end) = if selection.rectangle {
            (
                selection.anchor.x.min(selection.focus.x),
                selection.anchor.x.max(selection.focus.x).saturating_add(1),
            )
        } else if selection.mode == SelectionMode::Line {
            (0, columns)
        } else {
            (
                if row == start.y { start.x } else { 0 },
                if row == end.y {
                    end.x.saturating_add(1)
                } else {
                    columns
                },
            )
        };
        if span_start >= columns || span_start >= span_end {
            continue;
        }
        overlays.push(OverlaySpan::with_flags(
            u16::try_from(row - offset).unwrap_or(u16::MAX),
            span_start,
            span_end.min(columns),
            OverlayKind::Selection,
            if selection.rectangle {
                OVERLAY_RECTANGLE
            } else {
                0
            },
        ));
    }
}

pub(super) fn style_attributes(
    style: &libghostty_vt::style::Style,
    foreground_explicit_rgb: bool,
    hyperlink: bool,
) -> u16 {
    let mut attributes = 0;
    attributes |= u16::from(style.bold) * ATTR_BOLD;
    attributes |= u16::from(style.italic) * ATTR_ITALIC;
    attributes |= u16::from(style.faint) * ATTR_FAINT;
    attributes |= u16::from(style.blink) * ATTR_BLINK;
    attributes |= u16::from(style.invisible) * ATTR_INVISIBLE;
    attributes |= u16::from(style.strikethrough) * ATTR_STRIKETHROUGH;
    attributes |= u16::from(style.overline) * ATTR_OVERLINE;
    attributes |= u16::from(foreground_explicit_rgb) * ATTR_EXPLICIT_RGB;
    attributes |= u16::from(hyperlink) * ATTR_HYPERLINK;
    attributes
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CellBackground {
    Style,
    Palette(u8),
    Rgb(RgbColor),
}

fn copied_cell_style(
    style: &libghostty_vt::style::Style,
    background: CellBackground,
    hyperlink: bool,
    colors: &Colors,
    classes: &Classifier<'_>,
) -> PackedStyle {
    let palette = &colors.palette;
    let mut cell_foreground =
        resolve_style_color(style.fg_color, palette).unwrap_or(color(colors.foreground));
    let mut cell_background = match background {
        CellBackground::Style => resolve_style_color(style.bg_color, palette),
        CellBackground::Palette(index) => Some(color(palette[usize::from(index)])),
        CellBackground::Rgb(value) => Some(color(value)),
    }
    .unwrap_or(color(colors.background));
    let (foreground_class, background_class) = if style.inverse {
        std::mem::swap(&mut cell_foreground, &mut cell_background);
        (ColourClass::Resolved, ColourClass::Resolved)
    } else {
        (
            classes.ground(style.fg_color, 0),
            match background {
                CellBackground::Style => classes.ground(style.bg_color, 1),
                CellBackground::Palette(index) => classes.entry(index),
                CellBackground::Rgb(_) => ColourClass::Rgb,
            },
        )
    };
    let foreground_explicit_rgb = matches!(
        if style.inverse {
            style.bg_color
        } else {
            style.fg_color
        },
        StyleColor::Rgb(_)
    );
    PackedStyle::new(
        cell_foreground,
        cell_background,
        resolve_style_color(style.underline_color, palette),
        style_attributes(style, foreground_explicit_rgb, hyperlink),
        underline_style(style.underline),
    )
    .with_classes(foreground_class, background_class)
}

pub(super) const fn underline_style(underline: Underline) -> UnderlineStyle {
    match underline {
        Underline::None => UnderlineStyle::None,
        Underline::Double => UnderlineStyle::Double,
        Underline::Curly => UnderlineStyle::Curly,
        Underline::Dotted => UnderlineStyle::Dotted,
        Underline::Dashed => UnderlineStyle::Dashed,
        _ => UnderlineStyle::Single,
    }
}

pub(super) const fn color(value: RgbColor) -> Color {
    Color::rgb(value.r, value.g, value.b)
}

pub(super) fn resolve_style_color(value: StyleColor, palette: &[RgbColor; 256]) -> Option<Color> {
    match value {
        StyleColor::None => None,
        StyleColor::Palette(index) => palette.get(usize::from(index.0)).copied().map(color),
        StyleColor::Rgb(value) => Some(color(value)),
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ClassHints {
    palette: Vec<(u8, ColourClass)>,
    grounds: [Option<ColourClass>; 2],
    configured: Option<[RgbColor; 2]>,
}

impl ClassHints {
    pub(super) fn new(appearance: &TerminalAppearance) -> Self {
        Self {
            palette: appearance.palette_classes.clone(),
            grounds: appearance.default_classes,
            configured: Some([
                ghostty_color(appearance.foreground),
                ghostty_color(appearance.background),
            ]),
        }
    }
}

pub(super) struct Classifier<'a> {
    palette: &'a [RgbColor; 256],
    defaults: &'a [RgbColor; 256],
    entries: &'a [(u8, ColourClass)],
    grounds: [ColourClass; 2],
}

impl<'a> Classifier<'a> {
    pub(super) fn new(
        hints: &'a ClassHints,
        palette: &'a [RgbColor; 256],
        defaults: &'a [RgbColor; 256],
        current: [RgbColor; 2],
    ) -> Self {
        let grounds = [0, 1].map(|ground| {
            if hints
                .configured
                .as_ref()
                .is_some_and(|configured| configured[ground] != current[ground])
            {
                ColourClass::Rgb
            } else {
                hints.grounds[ground].unwrap_or(ColourClass::Default)
            }
        });
        Self {
            palette,
            defaults,
            entries: &hints.palette,
            grounds,
        }
    }

    pub(super) fn ground(&self, value: StyleColor, ground: usize) -> ColourClass {
        match value {
            StyleColor::None => self.grounds[ground],
            StyleColor::Palette(index) => self.entry(index.0),
            StyleColor::Rgb(_) => ColourClass::Rgb,
        }
    }

    pub(super) fn entry(&self, index: u8) -> ColourClass {
        let slot = usize::from(index);
        if self.palette[slot] != self.defaults[slot] {
            return ColourClass::Rgb;
        }
        self.entries
            .iter()
            .find(|(entry, _)| *entry == index)
            .map_or(ColourClass::Palette(index), |(_, class)| *class)
    }

    pub(super) const fn default_style(&self, style: PackedStyle) -> PackedStyle {
        style.with_classes(self.grounds[0], self.grounds[1])
    }
}
