use super::*;

fn history_scrollbar_state(
    terminal: &Terminal<'_, '_>,
) -> Result<ScrollbarState, TerminalCaptureError> {
    let scrollbar = terminal.scrollbar().map_err(capture_failure)?;
    Ok(ScrollbarState {
        total: u32::try_from(scrollbar.total).map_err(|_| {
            TerminalCaptureError::Failed("terminal history is too large".to_owned())
        })?,
        offset: u32::try_from(scrollbar.offset).map_err(|_| {
            TerminalCaptureError::Failed("terminal history is too large".to_owned())
        })?,
        len: u32::try_from(scrollbar.len).map_err(|_| {
            TerminalCaptureError::Failed("terminal history is too large".to_owned())
        })?,
    })
}

pub(super) fn capture_history(
    terminal: &Terminal<'_, '_>,
    start: u32,
    count: u32,
    hints: &ClassHints,
    bound: Option<(u16, u16)>,
) -> Result<HistoryCapture, TerminalCaptureError> {
    let history_rows =
        u32::try_from(terminal.scrollback_rows().map_err(capture_failure)?).unwrap_or(u32::MAX);
    let start = start.min(history_rows);
    let end = start.saturating_add(count).min(history_rows);
    let (columns, _) = bounded_extent((terminal.cols().map_err(capture_failure)?.max(1), 1), bound);
    let foreground = terminal
        .fg_color()
        .map_err(capture_failure)?
        .unwrap_or_default();
    let background = terminal
        .bg_color()
        .map_err(capture_failure)?
        .unwrap_or_default();
    let palette = terminal.color_palette().map_err(capture_failure)?.0;
    let default_palette = terminal.default_color_palette().map_err(capture_failure)?.0;
    let classes = Classifier::new(hints, &palette, &default_palette, [foreground, background]);
    let default_style = classes.default_style(PackedStyle::new(
        color(foreground),
        color(background),
        None,
        0,
        UnderlineStyle::None,
    ));
    let mut dictionary = ViewportDictionary::default();
    dictionary.ensure_default(default_style, &palette);
    let mut rows = Vec::with_capacity(usize::try_from(end - start).unwrap_or(0));
    let mut stack = ['\0'; 8];
    let mut grapheme_scratch = Vec::new();
    let mut grapheme_text = String::with_capacity(8);

    for row in start..end {
        let mut output = Vec::with_capacity(usize::from(columns));
        for column in 0..columns {
            let grid_ref = terminal
                .grid_ref(Point::Screen(PointCoordinate { x: column, y: row }))
                .map_err(capture_failure)?;
            let raw_style = grid_ref.style().map_err(capture_failure)?;
            let raw_cell = grid_ref.cell().map_err(capture_failure)?;
            let mut cell_foreground = resolve_style_color(raw_style.fg_color, &palette)
                .unwrap_or_else(|| color(foreground));
            let mut cell_background = resolve_style_color(raw_style.bg_color, &palette)
                .unwrap_or_else(|| color(background));
            cell_background = match raw_cell.content_tag().map_err(capture_failure)? {
                CellContentTag::BgColorPalette => color(
                    palette[usize::from(raw_cell.bg_color_palette().map_err(capture_failure)?.0)],
                ),
                CellContentTag::BgColorRgb => {
                    color(raw_cell.bg_color_rgb().map_err(capture_failure)?)
                }
                CellContentTag::Codepoint | CellContentTag::CodepointGrapheme => cell_background,
            };
            let classes = if raw_style.inverse {
                (ColourClass::Resolved, ColourClass::Resolved)
            } else {
                (
                    classes.ground(raw_style.fg_color, 0),
                    match raw_cell.content_tag().map_err(capture_failure)? {
                        CellContentTag::BgColorPalette => {
                            classes.entry(raw_cell.bg_color_palette().map_err(capture_failure)?.0)
                        }
                        CellContentTag::BgColorRgb => ColourClass::Rgb,
                        CellContentTag::Codepoint | CellContentTag::CodepointGrapheme => {
                            classes.ground(raw_style.bg_color, 1)
                        }
                    },
                )
            };
            if raw_style.inverse {
                std::mem::swap(&mut cell_foreground, &mut cell_background);
            }

            grapheme_text.clear();
            match grid_ref.graphemes(&mut stack) {
                Ok(count) => grapheme_text.extend(stack[..count].iter().copied()),
                Err(libghostty_vt::Error::OutOfSpace { required }) => {
                    if grapheme_scratch.len() < required {
                        grapheme_scratch.resize(required, '\0');
                    }
                    let count = grid_ref
                        .graphemes(&mut grapheme_scratch)
                        .map_err(capture_failure)?;
                    grapheme_text.extend(grapheme_scratch[..count].iter().copied());
                }
                Err(error) => return Err(capture_failure(error)),
            }

            let width = match raw_cell.wide().map_err(capture_failure)? {
                CellWide::Narrow => CellWidth::Narrow,
                CellWide::Wide => CellWidth::Wide,
                CellWide::SpacerTail => CellWidth::SpacerTail,
                CellWide::SpacerHead => CellWidth::SpacerHead,
            };
            let foreground_explicit_rgb = matches!(
                if raw_style.inverse {
                    raw_style.bg_color
                } else {
                    raw_style.fg_color
                },
                StyleColor::Rgb(_)
            );
            let style = PackedStyle::new(
                cell_foreground,
                cell_background,
                resolve_style_color(raw_style.underline_color, &palette),
                style_attributes(
                    &raw_style,
                    foreground_explicit_rgb,
                    raw_cell.has_hyperlink().map_err(capture_failure)?,
                ),
                underline_style(raw_style.underline),
            )
            .with_classes(classes.0, classes.1);
            output.push(PackedCell::new(
                dictionary.encode_glyph(&grapheme_text),
                dictionary.intern_style(style),
                width,
            ));
        }
        rows.push(output);
    }

    Ok((
        start,
        rows,
        dictionary.shared_dictionary().as_ref().clone(),
        history_scrollbar_state(terminal)?,
        columns,
    ))
}

pub(super) const CAPTURE_ROWS_PER_STEP: u64 = 512;

pub(super) trait CaptureGrid: GridRead {
    fn capture_total(&self) -> Result<usize, libghostty_vt::Error>;
    fn capture_rows(&self) -> Result<u16, libghostty_vt::Error>;
    fn capture_columns(&self) -> Result<u16, libghostty_vt::Error>;
    fn capture_history_rows(&self) -> Result<usize, libghostty_vt::Error>;
    fn capture_screen(&self) -> Result<Screen, libghostty_vt::Error>;
    fn capture_format(
        &self,
        options: FormatterOptions<'_, '_>,
    ) -> Result<String, TerminalCaptureError>;
}

impl CaptureGrid for Terminal<'_, '_> {
    fn capture_total(&self) -> Result<usize, libghostty_vt::Error> {
        self.total_rows()
    }
    fn capture_rows(&self) -> Result<u16, libghostty_vt::Error> {
        self.rows()
    }
    fn capture_columns(&self) -> Result<u16, libghostty_vt::Error> {
        self.cols()
    }
    fn capture_history_rows(&self) -> Result<usize, libghostty_vt::Error> {
        self.scrollback_rows()
    }
    fn capture_screen(&self) -> Result<Screen, libghostty_vt::Error> {
        self.active_screen()
    }
    fn capture_format(
        &self,
        options: FormatterOptions<'_, '_>,
    ) -> Result<String, TerminalCaptureError> {
        let mut formatter = Formatter::new(self, options).map_err(capture_failure)?;
        let length = match formatter.format_len() {
            Ok(length) => length,
            Err(libghostty_vt::Error::InvalidValue) => return Ok(String::new()),
            Err(error) => return Err(capture_failure(error)),
        };
        if length > MAX_CAPTURE_BYTES {
            return Err(TerminalCaptureError::TooLarge);
        }
        if length == 0 {
            return Ok(String::new());
        }
        let mut output = vec![0_u8; length];
        let written = formatter.format_buf(&mut output).map_err(capture_failure)?;
        output.truncate(written);
        String::from_utf8(output).map_err(|error| TerminalCaptureError::Failed(error.to_string()))
    }
}

impl CaptureGrid for ScreenSnapshot {
    fn capture_total(&self) -> Result<usize, libghostty_vt::Error> {
        self.total_rows()
    }
    fn capture_rows(&self) -> Result<u16, libghostty_vt::Error> {
        self.rows()
    }
    fn capture_columns(&self) -> Result<u16, libghostty_vt::Error> {
        self.cols()
    }
    fn capture_history_rows(&self) -> Result<usize, libghostty_vt::Error> {
        Ok(self.total_rows()?.saturating_sub(usize::from(self.rows()?)))
    }
    fn capture_screen(&self) -> Result<Screen, libghostty_vt::Error> {
        self.active_screen()
    }
    fn capture_format(
        &self,
        options: FormatterOptions<'_, '_>,
    ) -> Result<String, TerminalCaptureError> {
        let bytes = match self.format_alloc(options) {
            Ok(bytes) => bytes,
            Err(libghostty_vt::Error::InvalidValue) => return Ok(String::new()),
            Err(error) => return Err(capture_failure(error)),
        };
        if bytes.len() > MAX_CAPTURE_BYTES {
            return Err(TerminalCaptureError::TooLarge);
        }
        String::from_utf8(bytes.to_vec())
            .map_err(|error| TerminalCaptureError::Failed(error.to_string()))
    }
}

pub(super) enum CaptureSource {
    Screen(ScreenSnapshot),
    Mode(Arc<ModeRevision>),
}

#[derive(Default)]
pub(super) struct CaptureCarry {
    style: libghostty_vt::style::Style,
    link: Option<Vec<u8>>,
    idle_prompt: Option<u64>,
}

impl CaptureWork {
    pub(super) fn start(
        terminal: &Terminal<'_, '_>,
        mode: Option<&CopyModeState>,
        filter: &EngineFilter,
        request: CaptureRequest,
    ) -> Option<Self> {
        let CaptureRequest { options, reply } = request;
        let options = CaptureOptions {
            mode: options.mode && mode.is_some(),
            ..options
        };
        let output_rows = if options.line_flags {
            filter.output_rows(terminal)
        } else {
            Vec::new()
        };
        let dimensions = if options.mode
            && let Some(mode) = mode
        {
            Ok((
                u64::from(mode.revision.total_rows()),
                u64::from(mode.revision.viewport_rows),
                u64::from(mode.revision.maximum_offset()),
            ))
        } else {
            terminal
                .total_rows()
                .and_then(|total| {
                    Ok((
                        total as u64,
                        u64::from(terminal.rows()?),
                        terminal.scrollback_rows()? as u64,
                    ))
                })
                .map_err(capture_failure)
        };
        let result = dimensions.and_then(|(total, rows, visible_start)| {
            let visible_end = visible_start
                .saturating_add(rows.saturating_sub(1))
                .min(total.saturating_sub(1));
            let start = resolve_capture_boundary(options.start, visible_start, visible_end, total);
            let end = resolve_capture_boundary(options.end, visible_start, visible_end, total);
            if start > end || end - start < CAPTURE_ROWS_PER_STEP || options.hyperlinks {
                return capture_terminal_marked(terminal, mode, options, &output_rows).map(Err);
            }
            let source = if options.mode
                && let Some(mode) = mode
            {
                if options.alternate && mode.revision.screen != Screen::Alternate {
                    return Err(TerminalCaptureError::AlternateUnavailable);
                }
                CaptureSource::Mode(Arc::clone(&mode.revision))
            } else {
                if options.alternate
                    && terminal.active_screen().map_err(capture_failure)? != Screen::Alternate
                {
                    return Err(TerminalCaptureError::AlternateUnavailable);
                }
                CaptureSource::Screen(terminal.clone_screen().map_err(capture_failure)?)
            };
            let idle_prompt = idle_prompt_row(terminal, options)?;
            Ok(Ok((source, start, end, visible_start, idle_prompt)))
        });
        match result {
            Ok(Ok((source, next, end, visible_start, idle_prompt))) => Some(Self {
                source,
                options,
                output_rows,
                next,
                end,
                visible_start,
                output: String::new(),
                separator: false,
                previous: CaptureCarry {
                    idle_prompt,
                    ..CaptureCarry::default()
                },
                reply,
            }),
            result => {
                let _ = reply.send(match result {
                    Ok(Err(output)) => Ok(output),
                    Err(error) => Err(error),
                    Ok(Ok(_)) => unreachable!(),
                });
                None
            }
        }
    }

    fn wrapped(&self, row: u64) -> Result<bool, TerminalCaptureError> {
        match &self.source {
            CaptureSource::Screen(screen) => screen
                .grid_ref(Point::Screen(PointCoordinate {
                    x: 0,
                    y: u32::try_from(row).unwrap_or(u32::MAX),
                }))
                .and_then(|grid| grid.row())
                .and_then(libghostty_vt::screen::Row::is_wrapped)
                .map_err(capture_failure),
            CaptureSource::Mode(revision) => Ok(revision
                .row(u32::try_from(row).unwrap_or(u32::MAX))
                .wrapped()),
        }
    }

    pub(super) fn step(&mut self) -> Result<bool, TerminalCaptureError> {
        let start = self.next;
        let end = start
            .saturating_add(CAPTURE_ROWS_PER_STEP - 1)
            .min(self.end);
        let options = CaptureOptions {
            start: CaptureBoundary::Relative(start as i64 - self.visible_start as i64),
            end: CaptureBoundary::Relative(end as i64 - self.visible_start as i64),
            ..self.options
        };
        let continuing = end < self.end && self.options.join_wrapped && self.wrapped(end)?;
        let keep_tail = continuing
            && !options.number_lines
            && !options.line_flags
            && !options.escape_sequences
            && !options.preserve_trailing;
        let mut output = match &self.source {
            CaptureSource::Screen(screen) => capture_grid(
                screen,
                CaptureOptions {
                    preserve_trailing: options.preserve_trailing || keep_tail,
                    ..options
                },
                &mut self.previous,
                &self.output_rows,
            )?,
            CaptureSource::Mode(revision) => capture_revision(
                revision,
                self.visible_start as u32,
                options,
                &mut self.previous,
            )?,
        };
        if keep_tail && matches!(self.source, CaptureSource::Screen(_)) {
            let mut lines = output.split('\n').peekable();
            let mut trimmed = String::with_capacity(output.len());
            while let Some(line) = lines.next() {
                if lines.peek().is_some() {
                    trimmed.push_str(line.trim_end());
                    trimmed.push('\n');
                } else {
                    trimmed.push_str(line);
                }
            }
            output = trimmed;
        }
        if self.separator {
            self.output.push('\n');
        }
        self.output.push_str(&output);
        if self.output.len() > MAX_CAPTURE_BYTES {
            return Err(TerminalCaptureError::TooLarge);
        }
        self.separator = !(self.options.join_wrapped && self.wrapped(end)?);
        self.next = end + 1;
        Ok(end == self.end)
    }
}

pub(super) fn step_capture_work(work: &mut VecDeque<CaptureWork>) {
    let Some(mut capture) = work.pop_front() else {
        return;
    };
    match capture.step() {
        Ok(true) => {
            let _ = capture.reply.send(Ok(capture.output));
        }
        Ok(false) => work.push_front(capture),
        Err(error) => {
            let _ = capture.reply.send(Err(error));
        }
    }
}

pub(super) fn stamp_copy_mode_marks(
    terminal: &Terminal<'_, '_>,
    filter: &EngineFilter,
    views: &ActiveTerminalViews,
) {
    for mode in views.values().filter_map(|view| view.copy_mode.as_deref()) {
        mode.revision
            .stamp_output_rows(|| filter.output_rows(terminal));
    }
}

pub(super) fn capture_terminal(
    terminal: &Terminal<'_, '_>,
    mode: Option<&CopyModeState>,
    options: CaptureOptions,
) -> Result<String, TerminalCaptureError> {
    capture_terminal_marked(terminal, mode, options, &[])
}

pub(super) fn capture_terminal_marked(
    terminal: &Terminal<'_, '_>,
    mode: Option<&CopyModeState>,
    options: CaptureOptions,
    output_rows: &[u64],
) -> Result<String, TerminalCaptureError> {
    if options.mode
        && let Some(mode) = mode
    {
        if options.alternate && mode.revision.screen != Screen::Alternate {
            return Err(TerminalCaptureError::AlternateUnavailable);
        }
        if options.hyperlinks {
            return Ok(String::new());
        }
        return capture_mode_revision(mode, options);
    }
    capture_grid(
        terminal,
        CaptureOptions {
            mode: false,
            ..options
        },
        &mut CaptureCarry {
            idle_prompt: idle_prompt_row(terminal, options)?,
            ..CaptureCarry::default()
        },
        output_rows,
    )
}

fn idle_prompt_row(
    terminal: &Terminal<'_, '_>,
    options: CaptureOptions,
) -> Result<Option<u64>, TerminalCaptureError> {
    let find = || -> Result<Option<u64>, libghostty_vt::Error> {
        let mut row = u32::try_from(terminal.scrollback_rows()?)
            .unwrap_or(u32::MAX)
            .saturating_add(u32::from(terminal.cursor_y()?));
        let cursor = terminal.grid_row(Point::Screen(PointCoordinate { x: 0, y: row }))?;
        if cursor.row()?.semantic_prompt()? != RowSemanticPrompt::Continuation {
            return Ok(None);
        }
        for column in 0..terminal.cols()? {
            if let Some(cell) = cursor.cell(column)
                && cell.cell()?.has_text()?
            {
                return Ok(None);
            }
        }
        while let Some(above) = row.checked_sub(1) {
            row = above;
            match terminal
                .grid_ref(Point::Screen(PointCoordinate { x: 0, y: row }))?
                .row()?
                .semantic_prompt()?
            {
                RowSemanticPrompt::Continuation => {}
                RowSemanticPrompt::Prompt => return Ok(Some(u64::from(row))),
                RowSemanticPrompt::None => return Ok(None),
            }
        }
        Ok(None)
    };
    if options.skip_input && options.keep_idle_prompt {
        find().map_err(capture_failure)
    } else {
        Ok(None)
    }
}

pub(super) fn capture_grid(
    terminal: &impl CaptureGrid,
    options: CaptureOptions,
    previous: &mut CaptureCarry,
    output_rows: &[u64],
) -> Result<String, TerminalCaptureError> {
    let active_screen = terminal.capture_screen().map_err(capture_failure)?;
    if options.alternate && active_screen != Screen::Alternate {
        return Err(TerminalCaptureError::AlternateUnavailable);
    }
    let total = terminal.capture_total().map_err(capture_failure)?;
    if total == 0 {
        return Ok(String::new());
    }
    let total = u64::try_from(total).unwrap_or(u64::MAX);
    let rows = u64::from(terminal.capture_rows().map_err(capture_failure)?);
    let visible_start = u64::try_from(terminal.capture_history_rows().map_err(capture_failure)?)
        .unwrap_or(u64::MAX)
        .min(total.saturating_sub(1));
    let visible_end = visible_start
        .saturating_add(rows.saturating_sub(1))
        .min(total.saturating_sub(1));
    let start = resolve_capture_boundary(options.start, visible_start, visible_end, total);
    let end = resolve_capture_boundary(options.end, visible_start, visible_end, total);
    if start > end {
        return Ok(String::new());
    }
    let requested_rows = usize::try_from(end.saturating_sub(start).saturating_add(1)).unwrap_or(1);

    let columns = terminal.capture_columns().map_err(capture_failure)?;
    if options.hyperlinks {
        return capture_hyperlinks(
            terminal,
            options,
            (start, end),
            visible_start,
            columns,
            output_rows,
        );
    }
    if options.escape_sequences {
        return capture_styled_terminal(
            terminal,
            options,
            (start, end),
            visible_start,
            columns,
            previous,
            output_rows,
        );
    }
    let head = terminal
        .grid_ref(Point::Screen(PointCoordinate {
            x: 0,
            y: u32::try_from(start).unwrap_or(u32::MAX),
        }))
        .map_err(capture_failure)?;
    let tail = terminal
        .grid_ref(Point::Screen(PointCoordinate {
            x: columns.saturating_sub(1),
            y: u32::try_from(end).unwrap_or(u32::MAX),
        }))
        .map_err(capture_failure)?;
    let selection = Selection::new(head, tail, false);
    let format_range = |join_wrapped: bool| -> Result<String, TerminalCaptureError> {
        let formatter_options = FormatterOptions::new()
            .with_format(Format::Plain)
            .with_unwrap(join_wrapped)
            .with_trim(!options.preserve_trailing)
            .with_selection(&selection);
        terminal.capture_format(formatter_options)
    };
    let per_row = options.number_lines || options.line_flags;
    let output = format_range(options.join_wrapped && !per_row)?;
    let output = if options.skip_input {
        blank_prompt_lines(
            terminal,
            &output,
            (start, end),
            options.join_wrapped && !per_row,
            previous.idle_prompt,
        )?
    } else {
        output
    };
    let rows = if per_row && options.join_wrapped {
        Some(
            (start..=end)
                .map(|row| {
                    terminal
                        .grid_ref(Point::Screen(PointCoordinate {
                            x: 0,
                            y: u32::try_from(row).unwrap_or(u32::MAX),
                        }))
                        .and_then(|grid| grid.row())
                        .and_then(libghostty_vt::screen::Row::is_wrapped)
                        .map_err(capture_failure)
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
    } else {
        None
    };
    let written_rows = if options.join_wrapped {
        measure_written_rows(terminal, &selection)?
    } else if output.is_empty() {
        0
    } else {
        output.split('\n').count()
    };
    let trailing_rows = requested_rows.saturating_sub(written_rows);
    let output = pad_capture_rows(&output, trailing_rows, columns, options);
    let flags = if options.line_flags {
        Some(
            (start..=end)
                .map(|row| grid_line_flags(terminal, row, output_rows))
                .collect::<Result<Vec<_>, _>>()?,
        )
    } else {
        None
    };
    Ok(decorate_capture(
        output,
        rows.as_deref(),
        options,
        start,
        visible_start,
        flags.as_deref(),
    ))
}

fn blank_prompt_lines(
    terminal: &impl CaptureGrid,
    output: &str,
    (start, end): (u64, u64),
    joined: bool,
    idle_prompt: Option<u64>,
) -> Result<String, TerminalCaptureError> {
    let row_at = |row: u64| {
        terminal
            .grid_ref(Point::Screen(PointCoordinate {
                x: 0,
                y: u32::try_from(row).unwrap_or(u32::MAX),
            }))
            .and_then(|grid| grid.row())
            .map_err(capture_failure)
    };
    let mut row = start;
    let lines = output.split('\n').collect::<Vec<_>>();
    let mut kinds = Vec::with_capacity(lines.len());
    for _ in &lines {
        kinds.push(if row <= end {
            let first = row_at(row)?;
            let kind = match first.semantic_prompt().map_err(capture_failure)? {
                RowSemanticPrompt::Prompt if idle_prompt == Some(row) => {
                    RowSemanticPrompt::Continuation
                }
                kind => kind,
            };
            let mut wrapped = first.is_wrapped().map_err(capture_failure)?;
            row += 1;
            while joined && wrapped && row <= end {
                wrapped = row_at(row)?.is_wrapped().map_err(capture_failure)?;
                row += 1;
            }
            kind
        } else {
            RowSemanticPrompt::None
        });
    }
    let mut output_follows = false;
    let mut input = vec![false; lines.len()];
    for (index, kind) in kinds.iter().enumerate().rev() {
        input[index] = match kind {
            RowSemanticPrompt::None => {
                output_follows = true;
                false
            }
            RowSemanticPrompt::Prompt => {
                output_follows = false;
                true
            }
            RowSemanticPrompt::Continuation => output_follows,
        };
    }
    let mut blanked = String::with_capacity(output.len());
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            blanked.push('\n');
        }
        if !input[index] {
            blanked.push_str(line);
        }
    }
    Ok(blanked)
}

const LINE_FLAG_HYPERLINK: u8 = 1 << 0;
const LINE_FLAG_OUTPUT: u8 = 1 << 1;
const LINE_FLAG_PROMPT: u8 = 1 << 2;
const LINE_FLAG_WRAPPED: u8 = 1 << 3;
const MAX_HYPERLINK_URI: usize = 1024;

fn push_line_flags(output: &mut String, flags: u8) {
    for (bit, letter) in [
        (LINE_FLAG_HYPERLINK, 'H'),
        (LINE_FLAG_OUTPUT, 'O'),
        (LINE_FLAG_PROMPT, 'P'),
        (LINE_FLAG_WRAPPED, 'W'),
    ] {
        if flags & bit != 0 {
            output.push(letter);
        }
    }
    if flags == 0 {
        output.push('-');
    }
    output.push(' ');
}

fn grid_line_flags(
    terminal: &impl GridRead,
    row: u64,
    output_rows: &[u64],
) -> Result<u8, TerminalCaptureError> {
    let line = terminal
        .grid_ref(Point::Screen(PointCoordinate {
            x: 0,
            y: u32::try_from(row).unwrap_or(u32::MAX),
        }))
        .and_then(|grid| grid.row())
        .map_err(capture_failure)?;
    let mut flags = 0;
    if line.has_hyperlink().map_err(capture_failure)? {
        flags |= LINE_FLAG_HYPERLINK;
    }
    if output_rows.binary_search(&row).is_ok() {
        flags |= LINE_FLAG_OUTPUT;
    }
    if line.semantic_prompt().map_err(capture_failure)? == RowSemanticPrompt::Prompt {
        flags |= LINE_FLAG_PROMPT;
    }
    if line.is_wrapped().map_err(capture_failure)? {
        flags |= LINE_FLAG_WRAPPED;
    }
    Ok(flags)
}

fn capture_hyperlinks(
    terminal: &impl GridRead,
    options: CaptureOptions,
    (start, end): (u64, u64),
    history_rows: u64,
    columns: u16,
    output_rows: &[u64],
) -> Result<String, TerminalCaptureError> {
    let mut output = String::new();
    let mut found = 0_usize;
    let mut carried: Option<Vec<u8>> = None;
    let mut buffer = vec![0_u8; 256];
    for row in start..=end {
        let y = u32::try_from(row).unwrap_or(u32::MAX);
        let line = terminal
            .grid_ref(Point::Screen(PointCoordinate { x: 0, y }))
            .and_then(|grid| grid.row())
            .map_err(capture_failure)?;
        let wrapped = line.is_wrapped().map_err(capture_failure)?;
        let mut current = carried.take();
        if !line.has_hyperlink().map_err(capture_failure)? {
            continue;
        }
        let mut uris = Vec::new();
        for x in 0..columns {
            let grid = terminal
                .grid_ref(Point::Screen(PointCoordinate { x, y }))
                .map_err(capture_failure)?;
            let cell = grid.cell().map_err(capture_failure)?;
            if matches!(
                cell.wide().map_err(capture_failure)?,
                CellWide::SpacerTail | CellWide::SpacerHead
            ) {
                continue;
            }
            if !cell.has_hyperlink().map_err(capture_failure)? {
                current = None;
                continue;
            }
            let length = capture_link_uri(&grid, &mut buffer)?;
            let uri = &buffer[..length];
            if current.as_deref() == Some(uri) {
                continue;
            }
            let visible = visible_uri(uri);
            if uri.is_empty() || visible.len() > MAX_HYPERLINK_URI {
                current = None;
                continue;
            }
            current = Some(uri.to_vec());
            if found == usize::from(columns) {
                break;
            }
            found += 1;
            uris.push(visible);
        }
        if wrapped {
            carried = current;
        }
        if uris.is_empty() {
            continue;
        }
        if options.number_lines {
            push_capture_line_number(&mut output, row, history_rows);
        }
        if options.line_flags {
            push_line_flags(&mut output, grid_line_flags(terminal, row, output_rows)?);
        }
        output.push_str(&uris.join(" "));
        if !(options.join_wrapped && wrapped) {
            output.push('\n');
        }
        if output.len() > MAX_CAPTURE_BYTES {
            return Err(TerminalCaptureError::TooLarge);
        }
    }
    if output.ends_with('\n') {
        output.pop();
    }
    Ok(output)
}

fn visible_uri(uri: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(uri.len());
    for chunk in uri.utf8_chunks() {
        for character in chunk.valid().chars() {
            match character {
                '\\' => output.push_str("\\\\"),
                '\t' | '\n' => output.push(character),
                character if character.is_ascii_control() => {
                    let _ = write!(output, "\\{:03o}", u32::from(character));
                }
                character => output.push(character),
            }
        }
        for byte in chunk.invalid() {
            let _ = write!(output, "\\{byte:03o}");
        }
    }
    output
}

fn capture_styled_terminal(
    terminal: &impl GridRead,
    options: CaptureOptions,
    (start, end): (u64, u64),
    history_rows: u64,
    columns: u16,
    previous: &mut CaptureCarry,
    output_rows: &[u64],
) -> Result<String, TerminalCaptureError> {
    let escaped = options.escape_nonprintable;
    let mut output = String::new();
    let mut graphemes = vec!['\0'; 8];
    let mut uri = vec![0_u8; 256];
    for row in start..=end {
        let y = u32::try_from(row).unwrap_or(u32::MAX);
        let mut used = 0;
        let mut text_end = 0;
        for x in 0..columns {
            let cell = terminal
                .grid_ref(Point::Screen(PointCoordinate { x, y }))
                .and_then(|grid| grid.cell())
                .map_err(capture_failure)?;
            if cell.has_text().map_err(capture_failure)? {
                text_end = x + 1;
            }
            if cell.has_text().map_err(capture_failure)?
                || cell.has_styling().map_err(capture_failure)?
                || matches!(
                    cell.content_tag().map_err(capture_failure)?,
                    CellContentTag::BgColorPalette | CellContentTag::BgColorRgb
                )
            {
                used = x + 1;
            }
        }
        let width = if options.join_wrapped || options.trim_positions {
            text_end
        } else {
            u16::try_from(allocated_row_width(usize::from(used), usize::from(columns)))
                .unwrap_or(columns)
        };
        let mut line = String::new();
        let mut code = String::new();
        let mut has_link = false;
        for x in 0..width {
            let grid = terminal
                .grid_ref(Point::Screen(PointCoordinate { x, y }))
                .map_err(capture_failure)?;
            let cell = grid.cell().map_err(capture_failure)?;
            let wide = cell.wide().map_err(capture_failure)?;
            if wide == CellWide::SpacerTail
                || (wide == CellWide::SpacerHead && options.join_wrapped)
            {
                continue;
            }
            let mut style = if wide == CellWide::SpacerHead {
                libghostty_vt::style::Style::default()
            } else {
                grid.style().map_err(capture_failure)?
            };
            match cell.content_tag().map_err(capture_failure)? {
                CellContentTag::BgColorPalette => {
                    style.bg_color =
                        StyleColor::Palette(cell.bg_color_palette().map_err(capture_failure)?);
                }
                CellContentTag::BgColorRgb => {
                    style.bg_color = StyleColor::Rgb(cell.bg_color_rgb().map_err(capture_failure)?);
                }
                CellContentTag::Codepoint | CellContentTag::CodepointGrapheme => {}
            }
            code.clear();
            push_capture_sgr(&mut code, previous.style, style);
            if escaped {
                code = code.replace('\u{1b}', "\\033");
            }
            previous.style = style;
            let length = if !options.mode
                && wide != CellWide::SpacerHead
                && cell.has_hyperlink().map_err(capture_failure)?
            {
                capture_link_uri(&grid, &mut uri)?
            } else {
                0
            };
            let cell_uri = (length > 0).then(|| &uri[..length]);
            if cell_uri != previous.link.as_deref() {
                if let Some(visible) = cell_uri
                    .map(visible_uri)
                    .filter(|visible| visible.len() <= MAX_HYPERLINK_URI)
                {
                    push_capture_link(&mut code, &visible, escaped);
                    has_link = true;
                    previous.link = cell_uri.map(<[u8]>::to_vec);
                } else {
                    if has_link {
                        push_capture_link(&mut code, "", escaped);
                        has_link = false;
                    }
                    previous.link = None;
                }
            }
            line.push_str(&code);
            let count = match grid.graphemes(&mut graphemes) {
                Ok(count) => count,
                Err(libghostty_vt::Error::OutOfSpace { required }) => {
                    if required > MAX_CAPTURE_BYTES / std::mem::size_of::<char>() {
                        return Err(TerminalCaptureError::TooLarge);
                    }
                    graphemes.resize(required, '\0');
                    grid.graphemes(&mut graphemes).map_err(capture_failure)?
                }
                Err(error) => return Err(capture_failure(error)),
            };
            if count == 0 {
                line.push(' ');
            } else if escaped && count == 1 && graphemes[0] == '\\' {
                line.push_str("\\\\");
            } else {
                line.extend(graphemes[..count].iter());
            }
        }
        if has_link {
            line.push_str(&code);
            push_capture_link(&mut line, "", escaped);
        }
        if options.number_lines {
            push_capture_line_number(&mut output, row, history_rows);
        }
        if options.line_flags {
            push_line_flags(&mut output, grid_line_flags(terminal, row, output_rows)?);
        }
        if !options.join_wrapped && !options.preserve_trailing {
            output.push_str(line.trim_end_matches(' '));
        } else {
            output.push_str(&line);
        }
        let wrapped = terminal
            .grid_ref(Point::Screen(PointCoordinate { x: 0, y }))
            .and_then(|grid| grid.row())
            .and_then(libghostty_vt::screen::Row::is_wrapped)
            .map_err(capture_failure)?;
        if row < end && !(options.join_wrapped && wrapped) {
            output.push('\n');
        }
        if output.len() > MAX_CAPTURE_BYTES {
            return Err(TerminalCaptureError::TooLarge);
        }
    }
    Ok(decorate_capture(
        output,
        None,
        CaptureOptions {
            number_lines: false,
            escape_nonprintable: false,
            ..options
        },
        start,
        history_rows,
        None,
    ))
}

fn capture_link_uri(
    grid: &libghostty_vt::screen::GridRef<'_>,
    buffer: &mut Vec<u8>,
) -> Result<usize, TerminalCaptureError> {
    match grid.hyperlink_uri(buffer) {
        Ok(length) => Ok(length),
        Err(libghostty_vt::Error::OutOfSpace { required }) => {
            buffer.resize(required, 0);
            grid.hyperlink_uri(buffer).map_err(capture_failure)
        }
        Err(error) => Err(capture_failure(error)),
    }
}

fn push_capture_link(output: &mut String, uri: &str, escaped: bool) {
    let escape = if escaped { "\\033" } else { "\u{1b}" };
    output.push_str(escape);
    output.push_str("]8;;");
    output.push_str(uri);
    output.push_str(escape);
    output.push_str(if escaped { "\\\\" } else { "\\" });
}

fn capture_sgr_attributes(style: libghostty_vt::style::Style) -> [(bool, &'static str); 13] {
    [
        (style.bold, "1"),
        (style.faint, "2"),
        (style.italic, "3"),
        (style.underline == Underline::Single, "4"),
        (style.blink, "5"),
        (style.inverse, "7"),
        (style.invisible, "8"),
        (style.strikethrough, "9"),
        (style.underline == Underline::Double, "4:2"),
        (style.underline == Underline::Curly, "4:3"),
        (style.underline == Underline::Dotted, "4:4"),
        (style.underline == Underline::Dashed, "4:5"),
        (style.overline, "5:3"),
    ]
}

fn push_capture_sgr(
    output: &mut String,
    previous: libghostty_vt::style::Style,
    style: libghostty_vt::style::Style,
) {
    let old = capture_sgr_attributes(previous);
    let new = capture_sgr_attributes(style);
    let reset = old
        .iter()
        .zip(&new)
        .any(|((old, _), (new, _))| *old && !new)
        || (previous.underline_color != StyleColor::None
            && style.underline_color == StyleColor::None);
    let mut attributes = Vec::new();
    if reset {
        attributes.push("0");
    }
    for ((old, _), (new, code)) in old.iter().zip(&new) {
        if *new && (reset || !old) {
            attributes.push(*code);
        }
    }
    if !attributes.is_empty() {
        output.push_str("\x1b[");
        output.push_str(&attributes.join(";"));
        output.push('m');
    }
    for (old, new, base) in [
        (previous.fg_color, style.fg_color, 30),
        (previous.bg_color, style.bg_color, 40),
        (previous.underline_color, style.underline_color, 50),
    ] {
        if (reset && new != StyleColor::None) || (!reset && old != new) {
            push_capture_colour(output, new, base);
        }
    }
}

fn push_capture_colour(output: &mut String, colour: StyleColor, base: u16) {
    use std::fmt::Write as _;

    match colour {
        StyleColor::None if base == 50 => {}
        StyleColor::None => {
            let _ = write!(output, "\x1b[{}m", base + 9);
        }
        StyleColor::Palette(index) if base != 50 && index.0 < 8 => {
            let _ = write!(output, "\x1b[{}m", base + u16::from(index.0));
        }
        StyleColor::Palette(index) if base != 50 && index.0 < 16 => {
            let _ = write!(output, "\x1b[{}m", base + 60 + u16::from(index.0) - 8);
        }
        StyleColor::Palette(index) => {
            let _ = write!(output, "\x1b[{};5;{}m", base + 8, index.0);
        }
        StyleColor::Rgb(rgb) => {
            let _ = write!(output, "\x1b[{};2;{};{};{}m", base + 8, rgb.r, rgb.g, rgb.b);
        }
    }
}

fn measure_written_rows(
    terminal: &impl CaptureGrid,
    selection: &Selection<'_>,
) -> Result<usize, TerminalCaptureError> {
    let options = FormatterOptions::new()
        .with_format(Format::Plain)
        .with_unwrap(false)
        .with_trim(true)
        .with_selection(selection);
    let output = terminal.capture_format(options)?;
    Ok(if output.is_empty() {
        0
    } else {
        output.split('\n').count()
    })
}

fn allocated_row_width(used: usize, columns: usize) -> usize {
    if used == 0 {
        return 0;
    }
    if used < columns / 4 {
        columns / 4
    } else if used < columns / 2 {
        columns / 2
    } else if used < columns {
        columns
    } else {
        used
    }
}

fn pad_capture_rows(
    output: &str,
    trailing_rows: usize,
    columns: u16,
    options: CaptureOptions,
) -> String {
    let empty_cells = !options.join_wrapped && !options.trim_positions;
    let trim_spaces = !options.join_wrapped && !options.preserve_trailing;
    let mut lines = if output.is_empty() {
        Vec::new()
    } else {
        output.split('\n').map(str::to_owned).collect::<Vec<_>>()
    };
    if empty_cells && options.preserve_trailing {
        let columns = usize::from(columns);
        for line in &mut lines {
            let width = line.chars().count();
            let allocated = allocated_row_width(width, columns);
            for _ in width..allocated {
                line.push(' ');
            }
        }
    }
    if trim_spaces {
        for line in &mut lines {
            while line.ends_with(' ') {
                line.pop();
            }
        }
    }
    lines.resize(lines.len().saturating_add(trailing_rows), String::new());
    lines.join("\n")
}

fn decorate_capture(
    text: String,
    rows: Option<&[bool]>,
    options: CaptureOptions,
    first_row: u64,
    history_rows: u64,
    flags: Option<&[u8]>,
) -> String {
    let text = if options.number_lines || flags.is_some() {
        number_capture(
            &text,
            rows,
            (first_row, history_rows),
            options.number_lines,
            flags,
        )
    } else {
        text
    };
    if options.escape_nonprintable {
        text.replace('\\', "\\\\").replace('\u{1b}', "\\033")
    } else {
        text
    }
}

fn push_capture_line_number(output: &mut String, row: u64, history_rows: u64) {
    let number = i64::try_from(row)
        .unwrap_or(i64::MAX)
        .saturating_sub(i64::try_from(history_rows).unwrap_or(i64::MAX));
    output.push_str(&number.to_string());
    output.push(' ');
}

pub(super) fn number_capture(
    text: &str,
    rows: Option<&[bool]>,
    (first_row, history_rows): (u64, u64),
    numbers: bool,
    flags: Option<&[u8]>,
) -> String {
    let mut output = String::with_capacity(text.len());
    for (offset, line) in text.split('\n').enumerate() {
        if offset > 0 && rows.is_none_or(|rows| rows.get(offset - 1) != Some(&true)) {
            output.push('\n');
        }
        if numbers {
            push_capture_line_number(
                &mut output,
                first_row.saturating_add(offset as u64),
                history_rows,
            );
        }
        if let Some(flags) = flags {
            push_line_flags(&mut output, flags.get(offset).copied().unwrap_or(0));
        }
        output.push_str(line);
    }
    output
}

pub(super) fn capture_viewport(
    viewport: &TerminalViewport,
    options: CaptureOptions,
) -> Result<String, TerminalCaptureError> {
    if options.alternate {
        return Err(TerminalCaptureError::AlternateUnavailable);
    }
    let total = u64::from(viewport.rows);
    if total == 0 {
        return Ok(String::new());
    }
    let visible_end = total.saturating_sub(1);
    let start = resolve_capture_boundary(options.start, 0, visible_end, total);
    let end = resolve_capture_boundary(options.end, 0, visible_end, total);
    if start > end {
        return Ok(String::new());
    }

    let mut output = String::new();
    for row in start..=end {
        if options.escape_sequences {
            capture_viewport_row_vt(
                viewport,
                u16::try_from(row).unwrap_or(u16::MAX),
                options.preserve_trailing,
                &mut output,
            );
        } else {
            capture_viewport_row(
                viewport,
                u16::try_from(row).unwrap_or(u16::MAX),
                options.preserve_trailing,
                &mut output,
            );
        }
        if row < end {
            output.push('\n');
        }
        if output.len() > MAX_CAPTURE_BYTES {
            return Err(TerminalCaptureError::TooLarge);
        }
    }
    if options.hyperlinks {
        return Ok(String::new());
    }
    let flags = options
        .line_flags
        .then(|| vec![0; usize::try_from(end - start + 1).unwrap_or(0)]);
    Ok(decorate_capture(
        output,
        None,
        options,
        start,
        0,
        flags.as_deref(),
    ))
}

fn capture_viewport_row(
    viewport: &TerminalViewport,
    row: u16,
    preserve_trailing: bool,
    output: &mut String,
) {
    let start = output.len();
    for cell in viewport.row(row).unwrap_or_default() {
        push_viewport_cell(viewport, *cell, output);
    }
    if !preserve_trailing {
        let trimmed = output[start..].trim_end().len();
        output.truncate(start.saturating_add(trimmed));
    }
}

fn capture_viewport_row_vt(
    viewport: &TerminalViewport,
    row: u16,
    preserve_trailing: bool,
    output: &mut String,
) {
    let cells = viewport.row(row).unwrap_or_default();
    let last = if preserve_trailing {
        cells.len().checked_sub(1)
    } else {
        cells
            .iter()
            .rposition(|cell| viewport_cell_has_non_whitespace(viewport, *cell))
    };
    let Some(last) = last else {
        return;
    };
    let mut active_style = None;
    for cell in &cells[..=last] {
        if matches!(cell.width(), CellWidth::SpacerTail | CellWidth::SpacerHead) {
            continue;
        }
        if active_style != Some(cell.style_id()) {
            if let Some(style) = viewport.style(*cell) {
                mode_revision::push_sgr(output, style);
            }
            active_style = Some(cell.style_id());
        }
        push_viewport_cell(viewport, *cell, output);
    }
    if active_style.is_some() {
        output.push_str("\x1b[0m");
    }
}

fn viewport_cell_has_non_whitespace(viewport: &TerminalViewport, cell: PackedCell) -> bool {
    !matches!(cell.width(), CellWidth::SpacerTail | CellWidth::SpacerHead)
        && cell.glyph() != 0
        && viewport
            .cell_text(cell)
            .chars()
            .any(|character| !character.is_whitespace())
}

fn push_viewport_cell(viewport: &TerminalViewport, cell: PackedCell, output: &mut String) {
    if matches!(cell.width(), CellWidth::SpacerTail | CellWidth::SpacerHead) {
        return;
    }
    if cell.glyph() == 0 {
        output.push(' ');
    } else {
        viewport.push_glyph(cell, output);
    }
}

fn capture_mode_revision(
    mode: &CopyModeState,
    options: CaptureOptions,
) -> Result<String, TerminalCaptureError> {
    capture_revision(
        &mode.revision,
        mode.revision.maximum_offset(),
        options,
        &mut CaptureCarry::default(),
    )
}

pub(super) fn capture_revision(
    revision: &ModeRevision,
    viewport_offset: u32,
    options: CaptureOptions,
    previous: &mut CaptureCarry,
) -> Result<String, TerminalCaptureError> {
    let total = u64::from(revision.total_rows());
    let visible_start = u64::from(viewport_offset).min(total.saturating_sub(1));
    let visible_end = visible_start
        .saturating_add(u64::from(revision.viewport_rows).saturating_sub(1))
        .min(total.saturating_sub(1));
    let start = resolve_capture_boundary(options.start, visible_start, visible_end, total);
    let end = resolve_capture_boundary(options.end, visible_start, visible_end, total);
    if start > end {
        return Ok(String::new());
    }
    if options.escape_sequences {
        let screen = revision.screen();
        let screen = screen.lock();
        return capture_styled_terminal(
            &*screen,
            CaptureOptions {
                mode: true,
                ..options
            },
            (start, end),
            visible_start,
            revision.columns,
            previous,
            revision.output_rows(),
        );
    }
    let head = u32::try_from(start).unwrap_or(u32::MAX);
    let tail = u32::try_from(end).unwrap_or(u32::MAX);
    let per_row = options.number_lines || options.line_flags;
    let output = revision.capture_rows(
        head,
        tail,
        options.join_wrapped && !per_row,
        options.preserve_trailing,
    );
    if output.len() > MAX_CAPTURE_BYTES {
        return Err(TerminalCaptureError::TooLarge);
    }
    let rows = (per_row && options.join_wrapped).then(|| {
        (head..=tail)
            .map(|row| revision.row(row).wrapped())
            .collect::<Vec<_>>()
    });
    let flags = options.line_flags.then(|| {
        let output_rows = revision.output_rows();
        (head..=tail)
            .map(|row| {
                let meta = revision.row(row);
                (u8::from(revision.row_has_hyperlink(row)) * LINE_FLAG_HYPERLINK)
                    | (u8::from(output_rows.binary_search(&u64::from(row)).is_ok())
                        * LINE_FLAG_OUTPUT)
                    | (u8::from(meta.prompt()) * LINE_FLAG_PROMPT)
                    | (u8::from(meta.wrapped()) * LINE_FLAG_WRAPPED)
            })
            .collect::<Vec<_>>()
    });
    Ok(decorate_capture(
        output,
        rows.as_deref(),
        options,
        start,
        visible_start,
        flags.as_deref(),
    ))
}

fn resolve_capture_boundary(
    boundary: CaptureBoundary,
    visible_start: u64,
    visible_end: u64,
    total: u64,
) -> u64 {
    match boundary {
        CaptureBoundary::HistoryStart => 0,
        CaptureBoundary::VisibleEnd => visible_end,
        CaptureBoundary::Relative(offset) => {
            let resolved = i128::from(visible_start).saturating_add(i128::from(offset));
            u64::try_from(resolved.max(0))
                .unwrap_or(u64::MAX)
                .min(total.saturating_sub(1))
        }
    }
}

fn capture_failure(error: libghostty_vt::Error) -> TerminalCaptureError {
    TerminalCaptureError::Failed(error.to_string())
}

fn semantic_failure(error: &WorkerError) -> TerminalCaptureError {
    TerminalCaptureError::Failed(error.to_string())
}

pub(super) fn capture_last_command(
    terminal: &Terminal<'_, '_>,
) -> Result<LastCommandCapture, TerminalCaptureError> {
    let total = u32::try_from(terminal.total_rows().map_err(capture_failure)?).unwrap_or(u32::MAX);
    if total == 0 {
        return Err(TerminalCaptureError::NoSemanticMarks);
    }
    let columns = terminal.cols().map_err(capture_failure)?;
    let mut saw_prompt = false;
    let mut block_end = total;
    let mut row = total;
    while row > 0 {
        row -= 1;
        if row_semantic_prompt(terminal, row).map_err(|error| semantic_failure(&error))?
            != RowSemanticPrompt::Prompt
        {
            continue;
        }
        saw_prompt = true;
        let input = input_span(terminal, row, block_end, columns)
            .map_err(|error| semantic_failure(&error))?;
        let Some((command_start, command_end, input_rows_end)) = input else {
            block_end = row;
            continue;
        };
        let command = format_span(terminal, command_start, command_end)?;
        if command.trim().is_empty() {
            block_end = row;
            continue;
        }
        let span = output_span(terminal, input_rows_end, block_end, columns)
            .map_err(|error| semantic_failure(&error))?;
        let (output, truncated_rows) = match span {
            Some((start, end, dropped_rows)) => {
                let output = format_span(terminal, start, end)?;
                let (output, dropped) = clamp_output_bytes(output);
                (output, dropped_rows.saturating_add(dropped))
            }
            None => (String::new(), 0),
        };
        return Ok(LastCommandCapture {
            command,
            output,
            truncated_rows,
            running: false,
        });
    }
    if saw_prompt {
        return Ok(LastCommandCapture::default());
    }
    Err(TerminalCaptureError::NoSemanticMarks)
}

fn row_content_span(
    terminal: &Terminal<'_, '_>,
    row: u32,
    columns: u16,
    content: CellSemanticContent,
) -> Result<Option<(u16, u16)>, WorkerError> {
    let mut span: Option<(u16, u16)> = None;
    for column in 0..columns {
        let cell = terminal
            .grid_ref(Point::Screen(PointCoordinate { x: column, y: row }))?
            .cell()?;
        if !cell.has_text()? || cell.semantic_content()? != content {
            continue;
        }
        span = Some(span.map_or((column, column), |(start, _)| (start, column)));
    }
    Ok(span)
}

fn input_span(
    terminal: &Terminal<'_, '_>,
    prompt_row: u32,
    block_end: u32,
    columns: u16,
) -> Result<Option<(PointCoordinate, PointCoordinate, u32)>, WorkerError> {
    let mut span: Option<(PointCoordinate, PointCoordinate)> = None;
    let mut row = prompt_row;
    while row < block_end {
        let Some((first, last)) =
            row_content_span(terminal, row, columns, CellSemanticContent::Input)?
        else {
            break;
        };
        let start = PointCoordinate { x: first, y: row };
        let end = PointCoordinate { x: last, y: row };
        span = Some(span.map_or((start, end), |(existing, _)| (existing, end)));
        row = row.saturating_add(1);
    }
    Ok(span.map(|(start, end)| (start, end, row)))
}

fn output_span(
    terminal: &Terminal<'_, '_>,
    first_row: u32,
    block_end: u32,
    columns: u16,
) -> Result<Option<(PointCoordinate, PointCoordinate, usize)>, WorkerError> {
    let mut last = None;
    let mut row = block_end;
    while row > first_row {
        row -= 1;
        if let Some((_, column)) =
            row_content_span(terminal, row, columns, CellSemanticContent::Output)?
        {
            last = Some(PointCoordinate { x: column, y: row });
            break;
        }
    }
    let Some(last) = last else {
        return Ok(None);
    };
    let mut first = last;
    for row in first_row..=last.y {
        if let Some((column, _)) =
            row_content_span(terminal, row, columns, CellSemanticContent::Output)?
        {
            first = PointCoordinate { x: column, y: row };
            break;
        }
    }
    let limit = u32::try_from(MAX_LAST_COMMAND_LINES).unwrap_or(u32::MAX);
    let rows = last.y.saturating_sub(first.y).saturating_add(1);
    if rows <= limit {
        return Ok(Some((first, last, 0)));
    }
    let dropped = usize::try_from(rows - limit).unwrap_or(usize::MAX);
    let start = PointCoordinate {
        x: 0,
        y: last.y.saturating_sub(limit.saturating_sub(1)),
    };
    Ok(Some((start, last, dropped)))
}

pub(super) fn clamp_output_bytes(output: String) -> (String, usize) {
    if output.len() <= MAX_LAST_COMMAND_BYTES {
        return (output, 0);
    }
    let mut cut = output.len() - MAX_LAST_COMMAND_BYTES;
    while cut < output.len() && !output.is_char_boundary(cut) {
        cut += 1;
    }
    let start = output
        .match_indices('\n')
        .find(|(offset, _)| *offset >= cut)
        .map_or(cut, |(offset, _)| offset.saturating_add(1));
    let dropped = output[..start].matches('\n').count();
    (output[start..].to_owned(), dropped)
}

fn format_span(
    terminal: &Terminal<'_, '_>,
    start: PointCoordinate,
    end: PointCoordinate,
) -> Result<String, TerminalCaptureError> {
    let start = terminal
        .grid_ref(Point::Screen(start))
        .map_err(capture_failure)?;
    let end = terminal
        .grid_ref(Point::Screen(end))
        .map_err(capture_failure)?;
    let selection = Selection::new(start, end, false);
    let options = FormatterOptions::new()
        .with_format(Format::Plain)
        .with_unwrap(true)
        .with_trim(true)
        .with_selection(&selection);
    let mut formatter = Formatter::new(terminal, options).map_err(capture_failure)?;
    let length = match formatter.format_len() {
        Ok(length) => length,
        Err(libghostty_vt::Error::InvalidValue) => return Ok(String::new()),
        Err(error) => return Err(capture_failure(error)),
    };
    if length > MAX_CAPTURE_BYTES {
        return Err(TerminalCaptureError::TooLarge);
    }
    if length == 0 {
        return Ok(String::new());
    }
    let mut output = vec![0_u8; length];
    let written = formatter.format_buf(&mut output).map_err(capture_failure)?;
    output.truncate(written);
    String::from_utf8(output).map_err(|error| TerminalCaptureError::Failed(error.to_string()))
}
