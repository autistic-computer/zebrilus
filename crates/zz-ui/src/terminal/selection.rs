use super::*;

pub fn selection_autoscroll_lines(
    grid: Bounds<Pixels>,
    line_height: Pixels,
    position: Point<Pixels>,
) -> i32 {
    zz_client::terminal_view::selection_autoscroll_lines(
        f32::from(grid.origin.y),
        f32::from(grid.bottom()),
        f32::from(line_height),
        f32::from(position.y),
    )
}

pub(super) fn collect_overlays(
    viewport: &TerminalViewport,
    appearance: &TerminalAppearance,
    row_projection: RowProjection,
    grid_columns: u16,
    origin: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    scale: f32,
    output: &mut Vec<PaintQuad>,
    selection: &mut Vec<Bounds<Pixels>>,
) -> Option<Bounds<Pixels>> {
    let mut link_hover_bounds = None;
    for span in viewport
        .overlays
        .iter()
        .filter(|span| span.kind() == OverlayKind::CopyCursor)
    {
        let Some(display_row) = row_projection.display_row(span.row) else {
            continue;
        };
        output.push(fill(
            Bounds::new(
                point(origin.x, origin.y + line_height * display_row),
                size(cell_width * usize::from(grid_columns), line_height),
            ),
            copy_cursor_line_color(appearance),
        ));
    }

    for (index, span) in viewport.overlays.iter().enumerate() {
        let Some(display_row) = row_projection.display_row(span.row) else {
            continue;
        };
        if span.start >= span.end {
            continue;
        }
        let start = usize::from(span.start.min(viewport.columns));
        let end = usize::from(span.end.min(viewport.columns));
        let bounds = Bounds::new(
            point(
                origin.x + cell_width * start,
                origin.y + line_height * display_row,
            ),
            size(cell_width * end.saturating_sub(start), line_height),
        );
        if span.kind() == OverlayKind::Selection {
            selection.push(bounds);
        }
        let mut quad = fill(bounds, overlay_color(span.kind(), appearance));
        if appearance.rounded_selection
            && span.kind() == OverlayKind::Selection
            && span.flags() & OVERLAY_RECTANGLE == 0
        {
            quad = quad.corner_radii(selection_corners(
                viewport,
                index,
                device_pixel(scale) * 3.0,
            ));
        }
        output.push(quad);
        if span.kind() == OverlayKind::LinkHover {
            link_hover_bounds = Some(bounds);
            output.push(fill(
                Bounds::new(
                    point(bounds.origin.x, bounds.bottom() - device_pixel(scale)),
                    size(bounds.size.width, device_pixel(scale)),
                ),
                color(appearance.link_color, 1.0),
            ));
        }
    }
    link_hover_bounds
}

#[allow(
    clippy::disallowed_methods,
    reason = "terminal overlay colors come from the independent terminal appearance color system"
)]
pub(super) fn overlay_color(kind: OverlayKind, appearance: &TerminalAppearance) -> Hsla {
    match kind {
        OverlayKind::Selection => appearance_color(appearance.selection_background),
        OverlayKind::SearchMatch => appearance_color(appearance.search_match_color),
        OverlayKind::SearchCurrent => appearance_color(appearance.search_current_color),
        OverlayKind::LinkHover => appearance_color(AppearanceColor::rgba(
            appearance.link_color.r,
            appearance.link_color.g,
            appearance.link_color.b,
            72,
        )),
        OverlayKind::CopyCursor => appearance_color(appearance.copy_cursor_color),
    }
}

pub(super) fn copy_cursor_line_color(appearance: &TerminalAppearance) -> Hsla {
    let mut color = appearance_color(appearance.copy_cursor_color);
    color.a *= 0.25;
    color
}

pub(super) fn collect_selection_text(
    viewport: &TerminalViewport,
    row_revisions: &[u64],
    appearance: &TerminalAppearance,
    row_projection: RowProjection,
    hidden_composition_cells: Option<&(usize, Range<usize>)>,
    origin: Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    box_stroke: Pixels,
    font_size: Pixels,
    scale: f32,
    base_font: &Font,
    text_opacity: f32,
    window: &mut Window,
    row_cache: &mut RowRenderCache,
    text_output: &mut Vec<PositionedTextRow>,
    connector_output: &mut Vec<TerminalGraphicPaint>,
) {
    row_cache.live_selection_keys.clear();
    for span in viewport
        .overlays
        .iter()
        .filter(|span| span.kind() == OverlayKind::Selection)
    {
        let Some(display_row) = row_projection.display_row(span.row) else {
            continue;
        };
        if span.start >= span.end {
            continue;
        }
        let Some(row) = viewport.row(span.row) else {
            continue;
        };
        let start = usize::from(span.start.min(viewport.columns));
        let end = usize::from(span.end.min(viewport.columns));
        let Some(cells) = row.get(start..end).filter(|cells| !cells.is_empty()) else {
            continue;
        };
        let hidden_columns =
            selection_hidden_columns(display_row, start..end, hidden_composition_cells);
        let shape = |window: &mut Window| {
            Rc::new(shape_text_row(
                cells,
                viewport.dictionary.as_ref(),
                viewport.foreground,
                viewport.background,
                appearance,
                cell_width,
                font_size,
                scale,
                base_font,
                text_opacity,
                Some(appearance.selection_foreground),
                hidden_columns.as_ref(),
                window,
            ))
        };
        let selected = if hidden_columns.is_some() {
            shape(window)
        } else if let Some(revision) = row_revisions.get(usize::from(span.row)).copied() {
            let key = SelectionCacheKey {
                revision,
                start: span.start,
                end: span.end,
            };
            row_cache.live_selection_keys.insert(key);
            if let Some(cached) = row_cache.selection_rows.get(&key) {
                Rc::clone(cached)
            } else {
                let selected = shape(window);
                row_cache.selection_rows.insert(key, Rc::clone(&selected));
                selected
            }
        } else {
            shape(window)
        };
        push_box_drawing_connectors(
            &selected.box_connectors,
            origin.x + cell_width * start,
            origin.y + line_height * display_row,
            cell_width,
            line_height,
            box_stroke,
            scale,
            connector_output,
        );
        push_solid_blocks(
            &selected.solid_blocks,
            origin.x + cell_width * start,
            origin.y + line_height * display_row,
            cell_width,
            line_height,
            scale,
            connector_output,
        );
        if !selected.lines.is_empty() {
            text_output.push(PositionedTextRow {
                row: selected,
                origin: point(
                    origin.x + cell_width * start,
                    origin.y + line_height * display_row,
                ),
            });
        }
    }
    let live_selection_keys = &row_cache.live_selection_keys;
    row_cache
        .selection_rows
        .retain(|key, _| live_selection_keys.contains(key));
}

pub(super) fn selection_hidden_columns(
    display_row: usize,
    selection: Range<usize>,
    hidden_composition_cells: Option<&(usize, Range<usize>)>,
) -> Option<Range<usize>> {
    hidden_composition_cells
        .filter(|(hidden_row, _)| *hidden_row == display_row)
        .and_then(|(_, hidden)| {
            let hidden_start = hidden.start.max(selection.start);
            let hidden_end = hidden.end.min(selection.end);
            (hidden_start < hidden_end).then(|| {
                hidden_start.saturating_sub(selection.start)
                    ..hidden_end.saturating_sub(selection.start)
            })
        })
}

pub(super) fn selection_corners(
    viewport: &TerminalViewport,
    index: usize,
    radius: Pixels,
) -> Corners<Pixels> {
    let Some(span) = viewport.overlays.get(index).copied() else {
        return Corners::default();
    };
    let start = span.start;
    let end = span.end.saturating_sub(1);
    let above_start = span
        .row
        .checked_sub(1)
        .is_some_and(|row| selection_covers(viewport, row, start));
    let above_end = span
        .row
        .checked_sub(1)
        .is_some_and(|row| selection_covers(viewport, row, end));
    let below_start = span
        .row
        .checked_add(1)
        .is_some_and(|row| selection_covers(viewport, row, start));
    let below_end = span
        .row
        .checked_add(1)
        .is_some_and(|row| selection_covers(viewport, row, end));
    Corners {
        top_left: if above_start { px(0.0) } else { radius },
        top_right: if above_end { px(0.0) } else { radius },
        bottom_right: if below_end { px(0.0) } else { radius },
        bottom_left: if below_start { px(0.0) } else { radius },
    }
}

fn selection_covers(viewport: &TerminalViewport, row: u16, column: u16) -> bool {
    viewport.overlays.iter().any(|candidate| {
        candidate.kind() == OverlayKind::Selection
            && candidate.flags() & OVERLAY_RECTANGLE == 0
            && candidate.row == row
            && candidate.start <= column
            && column < candidate.end
    })
}
