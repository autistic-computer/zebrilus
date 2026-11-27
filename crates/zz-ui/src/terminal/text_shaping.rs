use super::*;

pub fn terminal_font_for_style(
    appearance: &TerminalAppearance,
    fallback: &str,
    bold: bool,
    italic: bool,
) -> Font {
    let configured_families = match (bold, italic) {
        (true, true) => &appearance.font_families_bold_italic,
        (true, false) => &appearance.font_families_bold,
        (false, true) => &appearance.font_families_italic,
        (false, false) => &appearance.font_families,
    };
    let families = if configured_families.is_empty() {
        &appearance.font_families
    } else {
        configured_families
    };
    let primary = families.first().map_or(fallback, String::as_str);
    let mut font = font(primary.to_owned());
    let mut features = Vec::with_capacity(appearance.font_features.len() + 1);
    features.push(("liga".to_owned(), 1));
    features.extend(
        appearance
            .font_features
            .iter()
            .map(|feature| (feature.tag_string(), feature.value)),
    );
    font.features = FontFeatures(Arc::new(features));
    let fallbacks = families.iter().skip(1).cloned().collect::<Vec<_>>();
    font.fallbacks = (!fallbacks.is_empty()).then(|| FontFallbacks::from_fonts(fallbacks));
    font.weight = FontWeight(
        (f32::from(appearance.font_weight) + if bold { 300.0 } else { 0.0 })
            .min(FontWeight::BLACK.0),
    );
    font.style = if italic {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    font
}

pub(super) fn push_background(
    batch: &mut Option<BackgroundBatch>,
    column: usize,
    background: Color,
    default: Color,
    output: &mut Vec<CachedBackground>,
) {
    match *batch {
        Some(previous) if previous.color == background => {}
        Some(previous) => {
            push_background_run(previous, column, default, output);
            *batch = Some(BackgroundBatch {
                start_column: column,
                color: background,
            });
        }
        None => {
            *batch = Some(BackgroundBatch {
                start_column: column,
                color: background,
            });
        }
    }
}

pub(super) fn finish_backgrounds(
    batch: Option<BackgroundBatch>,
    columns: usize,
    default: Color,
    output: &mut Vec<CachedBackground>,
) {
    if let Some(batch) = batch {
        push_background_run(batch, columns, default, output);
    }
}

fn push_background_run(
    batch: BackgroundBatch,
    end_column: usize,
    default: Color,
    output: &mut Vec<CachedBackground>,
) {
    if batch.color != default && batch.start_column < end_column {
        output.push(CachedBackground {
            start_column: batch.start_column,
            cell_count: end_column - batch.start_column,
            color: batch.color,
        });
    }
}

pub(super) fn shape_text_row(
    cells: &[PackedCell],
    dictionary: &TerminalDictionary,
    default_foreground: Color,
    default_background: Color,
    appearance: &TerminalAppearance,
    cell_width: Pixels,
    font_size: Pixels,
    scale: f32,
    base_font: &Font,
    text_opacity: f32,
    forced_foreground: Option<Color>,
    hidden_columns: Option<&Range<usize>>,
    window: &mut Window,
) -> CachedTextRow {
    let mut output = CachedTextRow::default();
    let mut background_batch = None;
    let mut batch: Option<TextBatch> = None;
    for (column, cell) in cells.iter().copied().enumerate() {
        let style = style_for_dictionary(
            dictionary,
            default_foreground,
            default_background,
            appearance.local_colors.as_ref(),
            cell,
        );
        let glyph = glyph_for_dictionary(dictionary, cell);
        let decorative = decorative_glyph(glyph);
        let foreground = forced_foreground.map_or_else(
            || resolved_foreground(style, appearance, decorative),
            |foreground| faint_foreground(style, foreground),
        );
        push_background(
            &mut background_batch,
            column,
            style.background(),
            default_background,
            &mut output.backgrounds,
        );
        if hidden_columns.is_some_and(|hidden| hidden.contains(&column)) {
            flush_cached_batch(
                batch.take(),
                cell_width,
                font_size,
                window,
                &mut output.lines,
            );
            continue;
        }
        if style.overline() && !matches!(cell.width(), CellWidth::SpacerTail) {
            output.overlines.push(CachedOverline {
                start_column: column,
                cell_count: if matches!(cell.width(), CellWidth::Wide) {
                    2
                } else {
                    1
                },
                color: foreground,
                alpha: text_opacity,
            });
        }
        if matches!(
            style.underline(),
            TerminalUnderlineStyle::Double
                | TerminalUnderlineStyle::Dotted
                | TerminalUnderlineStyle::Dashed
        ) && !matches!(cell.width(), CellWidth::SpacerTail)
        {
            output.underlines.push(CachedUnderline {
                start_column: column,
                cell_count: if matches!(cell.width(), CellWidth::Wide) {
                    2
                } else {
                    1
                },
                color: style.underline_color().unwrap_or(foreground),
                alpha: text_opacity,
                kind: style.underline(),
            });
        }

        if matches!(cell.width(), CellWidth::SpacerTail | CellWidth::SpacerHead) {
            continue;
        }
        if matches!(glyph, Glyph::Empty) || style.invisible() {
            flush_cached_batch(
                batch.take(),
                cell_width,
                font_size,
                window,
                &mut output.lines,
            );
            continue;
        }
        if let Some(drawing) = connected_light_box_drawing(glyph) {
            output.box_connectors.push(CachedBoxConnector {
                column,
                drawing,
                color: foreground,
                alpha: text_opacity,
            });
            let font_decorates_glyph = style.strikethrough()
                || style.hyperlink()
                || matches!(
                    style.underline(),
                    TerminalUnderlineStyle::Single | TerminalUnderlineStyle::Curly
                );
            if !font_decorates_glyph {
                flush_cached_batch(
                    batch.take(),
                    cell_width,
                    font_size,
                    window,
                    &mut output.lines,
                );
                continue;
            }
        }
        if let Some(element) = connected_solid_block_element(glyph) {
            output.solid_blocks.push(CachedSolidBlock {
                column,
                element,
                color: foreground,
                alpha: text_opacity,
            });
            let font_decorates_glyph = style.strikethrough()
                || style.hyperlink()
                || matches!(
                    style.underline(),
                    TerminalUnderlineStyle::Single | TerminalUnderlineStyle::Curly
                );
            if !font_decorates_glyph {
                flush_cached_batch(
                    batch.take(),
                    cell_width,
                    font_size,
                    window,
                    &mut output.lines,
                );
                continue;
            }
        }

        let width = if matches!(cell.width(), CellWidth::Wide) {
            2
        } else {
            1
        };
        let font_style = TerminalFontStyle::from_packed(style);
        if let Glyph::Grapheme(text) = glyph {
            flush_cached_batch(
                batch.take(),
                cell_width,
                font_size,
                window,
                &mut output.lines,
            );
            let run = text_run(
                style,
                text.len(),
                base_font,
                foreground,
                appearance,
                scale,
                text_opacity,
            );
            let glyph_options = glyph_render_options(font_style, &run.font, appearance, window);
            let line =
                window
                    .text_system()
                    .shape_line(text.to_owned().into(), font_size, &[run], None);
            output.lines.push(CachedLine {
                line,
                start_column: column,
                glyph_options,
                raster: RefCell::new(RasterState::Untried),
            });
            continue;
        }

        let can_append = batch.as_ref().is_some_and(|batch| {
            batch.start_column + batch.cell_count == column && batch.font_style == font_style
        });
        if !can_append {
            flush_cached_batch(
                batch.take(),
                cell_width,
                font_size,
                window,
                &mut output.lines,
            );
            batch = Some(TextBatch {
                text: String::new(),
                start_column: column,
                cell_count: 0,
                runs: SmallVec::new(),
                last_style: None,
                last_foreground: None,
                font_style,
                glyph_options: glyph_render_options(
                    font_style,
                    &font_for_style(font_style, base_font, appearance),
                    appearance,
                    window,
                ),
            });
        }
        let batch = batch.as_mut().expect("created above");
        let text_start = batch.text.len();
        if let Glyph::Scalar(value) = glyph {
            batch.text.push(value);
        }
        if width == 2 {
            batch.text.push(' ');
        }
        let appended_len = batch.text.len() - text_start;
        push_cell_text_run(
            &mut batch.runs,
            &mut batch.last_style,
            &mut batch.last_foreground,
            style,
            foreground,
            appended_len,
            base_font,
            appearance,
            scale,
            text_opacity,
        );
        batch.cell_count += width;
    }

    finish_backgrounds(
        background_batch,
        cells.len(),
        default_background,
        &mut output.backgrounds,
    );
    flush_cached_batch(batch, cell_width, font_size, window, &mut output.lines);
    output
}

fn flush_cached_batch(
    batch: Option<TextBatch>,
    cell_width: Pixels,
    font_size: Pixels,
    window: &mut Window,
    output: &mut Vec<CachedLine>,
) {
    let Some(batch) = batch else {
        return;
    };
    let TextBatch {
        text,
        start_column,
        runs,
        glyph_options,
        ..
    } = batch;
    let mut line = window
        .text_system()
        .shape_line(text.into(), font_size, &runs, None);
    align_line_to_cells(&mut line, cell_width);
    output.push(CachedLine {
        line,
        start_column,
        glyph_options,
        raster: RefCell::new(RasterState::Untried),
    });
}

pub(super) fn align_line_to_cells(line: &mut ShapedLine, cell_width: Pixels) {
    let columns: SmallVec<[usize; 128]> =
        line.text.char_indices().map(|(index, _)| index).collect();
    let mut runs = line.runs.clone();
    let mut cluster = None;
    for glyph in runs.iter_mut().flat_map(|run| &mut run.glyphs) {
        let offset = match cluster {
            Some((index, offset)) if index == glyph.index => offset,
            _ => {
                let column = columns.partition_point(|index| *index < glyph.index);
                let offset = cell_width * column - glyph.position.x;
                cluster = Some((glyph.index, offset));
                offset
            }
        };
        glyph.position.x += offset;
    }
    **line = Arc::new(ala::LineLayout {
        font_size: line.font_size,
        width: cell_width * columns.len(),
        ascent: line.ascent,
        descent: line.descent,
        runs,
        len: line.len(),
    });
}

fn push_text_run(runs: &mut SmallVec<[TextRun; 4]>, mut run: TextRun, len: usize) {
    run.len = len;
    if let Some(previous) = runs.last_mut()
        && same_visual_style(previous, &run)
    {
        previous.len += len;
    } else {
        runs.push(run);
    }
}

pub(super) fn push_cell_text_run(
    runs: &mut SmallVec<[TextRun; 4]>,
    last_style: &mut Option<PackedStyle>,
    last_foreground: &mut Option<Color>,
    style: PackedStyle,
    foreground: Color,
    len: usize,
    base_font: &Font,
    appearance: &TerminalAppearance,
    scale: f32,
    text_opacity: f32,
) {
    if *last_style == Some(style)
        && *last_foreground == Some(foreground)
        && let Some(previous) = runs.last_mut()
    {
        previous.len += len;
        return;
    }

    push_text_run(
        runs,
        text_run(
            style,
            len,
            base_font,
            foreground,
            appearance,
            scale,
            text_opacity,
        ),
        len,
    );
    *last_style = Some(style);
    *last_foreground = Some(foreground);
}

pub(super) fn position_text_row(
    cached: &Rc<CachedTextRow>,
    origin: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    box_stroke: Pixels,
    scale: f32,
    output: &mut PaintBuffers,
) {
    let y = origin.y;
    output
        .backgrounds
        .extend(cached.backgrounds.iter().map(|background| {
            fill(
                Bounds::new(
                    point(origin.x + cell_width * background.start_column, y),
                    size(cell_width * background.cell_count, line_height),
                ),
                color(background.color, 1.0),
            )
        }));
    push_box_drawing_connectors(
        &cached.box_connectors,
        origin.x,
        y,
        cell_width,
        line_height,
        box_stroke,
        scale,
        &mut output.box_connectors,
    );
    push_solid_blocks(
        &cached.solid_blocks,
        origin.x,
        y,
        cell_width,
        line_height,
        scale,
        &mut output.box_connectors,
    );
    if !cached.lines.is_empty() {
        output.text.push(PositionedTextRow {
            row: Rc::clone(cached),
            origin: point(origin.x, y),
        });
    }
    output
        .decorations
        .extend(cached.overlines.iter().map(|overline| {
            fill(
                Bounds::new(
                    point(origin.x + cell_width * overline.start_column, y),
                    size(cell_width * overline.cell_count, device_pixel(scale)),
                ),
                color(overline.color, overline.alpha),
            )
        }));
    for underline in &cached.underlines {
        push_explicit_underline(
            underline,
            origin.x,
            y,
            cell_width,
            line_height,
            scale,
            &mut output.decorations,
        );
    }
}

fn push_explicit_underline(
    underline: &CachedUnderline,
    row_x: Pixels,
    row_y: Pixels,
    cell_width: Pixels,
    line_height: Pixels,
    scale: f32,
    output: &mut Vec<PaintQuad>,
) {
    let stroke = device_pixel(scale);
    let start = row_x + cell_width * underline.start_column;
    let end = start + cell_width * underline.cell_count;
    let bottom = row_y + line_height - stroke;
    let paint_segment = |x: Pixels, y: Pixels, width: Pixels| {
        fill(
            Bounds::new(point(x, y), size(width, stroke)),
            color(underline.color, underline.alpha),
        )
    };

    match underline.kind {
        TerminalUnderlineStyle::Double => {
            output.push(paint_segment(start, bottom, end - start));
            output.push(paint_segment(
                start,
                (bottom - stroke * 2.0).max(row_y),
                end - start,
            ));
        }
        TerminalUnderlineStyle::Dotted | TerminalUnderlineStyle::Dashed => {
            let segment = if matches!(underline.kind, TerminalUnderlineStyle::Dotted) {
                stroke
            } else {
                stroke * 3.0
            };
            let gap = if matches!(underline.kind, TerminalUnderlineStyle::Dotted) {
                stroke
            } else {
                stroke * 2.0
            };
            let mut x = start;
            while x < end {
                output.push(paint_segment(x, bottom, segment.min(end - x)));
                x += segment + gap;
            }
        }
        _ => {}
    }
}

fn font_for_style(
    style: TerminalFontStyle,
    base_font: &Font,
    appearance: &TerminalAppearance,
) -> Font {
    let configured_families = match (style.bold, style.italic) {
        (true, true) => &appearance.font_families_bold_italic,
        (true, false) => &appearance.font_families_bold,
        (false, true) => &appearance.font_families_italic,
        (false, false) => return base_font.clone(),
    };
    if configured_families.is_empty() {
        let mut font = base_font.clone();
        if style.bold {
            font.weight = ala::FontWeight((font.weight.0 + 300.0).min(ala::FontWeight::BLACK.0));
        }
        if style.italic {
            font.style = ala::FontStyle::Italic;
        }
        font
    } else {
        terminal_font_for_style(
            appearance,
            base_font.family.as_ref(),
            style.bold,
            style.italic,
        )
    }
}

pub(super) fn base_glyph_render_options(appearance: &TerminalAppearance) -> GlyphRenderOptions {
    GlyphRenderOptions {
        font_smoothing: if appearance.font_thicken {
            FontSmoothing::Enabled(appearance.font_thicken_strength)
        } else {
            FontSmoothing::Disabled
        },
        synthetic_bold: false,
        synthetic_italic: false,
    }
}

pub(super) fn glyph_render_options(
    style: TerminalFontStyle,
    font: &Font,
    appearance: &TerminalAppearance,
    window: &mut Window,
) -> GlyphRenderOptions {
    let mut options = base_glyph_render_options(appearance);
    if !appearance
        .font_synthetic_style
        .allows(style.bold, style.italic)
    {
        return options;
    }

    let resolved = window.text_system().resolve_font(font);
    if style.bold {
        let mut without_bold = font.clone();
        without_bold.weight = FontWeight(f32::from(appearance.font_weight));
        options.synthetic_bold = resolved == window.text_system().resolve_font(&without_bold);
    }
    if style.italic {
        let mut without_italic = font.clone();
        without_italic.style = FontStyle::Normal;
        options.synthetic_italic = resolved == window.text_system().resolve_font(&without_italic);
    }
    options
}

pub(super) fn styled_font(
    style: PackedStyle,
    base_font: &Font,
    appearance: &TerminalAppearance,
) -> Font {
    font_for_style(TerminalFontStyle::from_packed(style), base_font, appearance)
}

pub(super) fn text_run(
    style: PackedStyle,
    text_len: usize,
    base_font: &Font,
    resolved_foreground: Color,
    appearance: &TerminalAppearance,
    scale: f32,
    text_opacity: f32,
) -> TextRun {
    let font = styled_font(style, base_font, appearance);
    let foreground = color(resolved_foreground, text_opacity);
    TextRun {
        len: text_len,
        font,
        color: foreground,
        background_color: None,
        underline: match style.underline() {
            TerminalUnderlineStyle::Single | TerminalUnderlineStyle::Curly => {
                Some(UnderlineStyle {
                    thickness: device_pixel(scale),
                    color: Some(color(
                        style.underline_color().unwrap_or(resolved_foreground),
                        text_opacity,
                    )),
                    wavy: matches!(style.underline(), TerminalUnderlineStyle::Curly),
                })
            }
            TerminalUnderlineStyle::None if style.hyperlink() => Some(UnderlineStyle {
                thickness: device_pixel(scale),
                color: Some(color(appearance.link_color, text_opacity)),
                wavy: false,
            }),
            _ => None,
        },
        strikethrough: style.strikethrough().then_some(StrikethroughStyle {
            thickness: device_pixel(scale),
            color: Some(foreground),
        }),
    }
}

fn style_for_dictionary(
    dictionary: &TerminalDictionary,
    default_foreground: Color,
    default_background: Color,
    local: Option<&LocalColors>,
    cell: PackedCell,
) -> PackedStyle {
    dictionary
        .styles
        .get(usize::from(cell.style_id()))
        .map_or_else(
            || {
                PackedStyle::new(
                    default_foreground,
                    default_background,
                    None,
                    0,
                    TerminalUnderlineStyle::None,
                )
            },
            |style| local.map_or(*style, |local| local.recolor(*style)),
        )
}

pub(super) fn glyph_for_dictionary(dictionary: &TerminalDictionary, cell: PackedCell) -> Glyph<'_> {
    let glyph = cell.glyph();
    if glyph == 0 {
        return Glyph::Empty;
    }
    if glyph & GRAPHEME_TABLE_BIT == 0 {
        return char::from_u32(glyph).map_or(Glyph::Empty, Glyph::Scalar);
    }

    let index = usize::try_from(glyph & !GRAPHEME_TABLE_BIT).unwrap_or(usize::MAX);
    let Some((&start, &end)) = dictionary
        .grapheme_offsets
        .get(index)
        .zip(dictionary.grapheme_offsets.get(index.saturating_add(1)))
    else {
        return Glyph::Empty;
    };
    let Some(bytes) = usize::try_from(start)
        .ok()
        .zip(usize::try_from(end).ok())
        .and_then(|(start, end)| dictionary.grapheme_bytes.get(start..end))
    else {
        return Glyph::Empty;
    };
    std::str::from_utf8(bytes).map_or(Glyph::Empty, Glyph::Grapheme)
}

pub(super) fn style_for(
    viewport: &TerminalViewport,
    local: Option<&LocalColors>,
    cell: PackedCell,
) -> PackedStyle {
    style_for_dictionary(
        &viewport.dictionary,
        viewport.foreground,
        viewport.background,
        local,
        cell,
    )
}

pub(super) fn glyph_for(viewport: &TerminalViewport, cell: PackedCell) -> Glyph<'_> {
    glyph_for_dictionary(&viewport.dictionary, cell)
}

pub(super) fn same_visual_style(left: &TextRun, right: &TextRun) -> bool {
    left.font == right.font
        && left.color == right.color
        && left.background_color == right.background_color
        && left.underline == right.underline
        && left.strikethrough == right.strikethrough
}
