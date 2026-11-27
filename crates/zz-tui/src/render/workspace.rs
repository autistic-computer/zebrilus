use super::*;

impl Renderer {
    pub(super) fn paint_workspace(&mut self, model: &Model, force: bool, cleared_to_default: bool) {
        let lines = model.pane_border_lines();
        let indicators = model.pane_border_indicators();
        let border_changed = self.border_chrome.as_mut().is_none_or(|cached| {
            let changed = cached.0 != model.pane_border_status()
                || cached.1 != lines
                || cached.2 != indicators
                || !Arc::ptr_eq(&cached.3, &model.status)
                    && (cached.3.pane_borders != model.status.pane_borders
                        || cached.3.theme != model.status.theme);
            if !changed {
                cached.3 = Arc::clone(&model.status);
            }
            changed
        });
        let force = force || border_changed;
        if border_changed {
            self.border_chrome = Some((
                model.pane_border_status(),
                lines,
                indicators,
                Arc::clone(&model.status),
            ));
        }
        if force {
            let cells = divider_cells(&model.layout.dividers);
            let mut border = BTreeMap::new();
            for divider in &model.layout.dividers {
                let highlighted = divider.highlighted;
                let fallback = if highlighted {
                    model.appearance.link_color
                } else {
                    model.appearance.foreground
                };
                let color = divider
                    .style_pane
                    .and_then(|pane| model.pane_border_colour(pane, highlighted))
                    .map_or(fallback, |colour| {
                        resolve_tmux_colour(colour, fallback, &model.appearance)
                    });
                let style = divider
                    .style_pane
                    .and_then(|pane| model.pane_border_style(pane));
                let mut sgr = Vec::new();
                if let Some(style) = &style {
                    write_border_sgr(&mut sgr, style, &model.appearance);
                } else {
                    write_colored_sgr(&mut sgr, color, model.appearance.background);
                }
                let sgr = Rc::new(sgr);
                let index = divider.style_pane.and_then(|pane| model.pane_index(pane));
                for row in divider.rect.y..divider.rect.y.saturating_add(divider.rect.height) {
                    for column in divider.rect.x..divider.rect.x.saturating_add(divider.rect.width)
                    {
                        let mut mask = 0;
                        for (present, bit) in [
                            (cells.contains(&(column.wrapping_sub(1), row)), BORDER_L),
                            (cells.contains(&(column.saturating_add(1), row)), BORDER_R),
                            (cells.contains(&(column, row.wrapping_sub(1))), BORDER_U),
                            (cells.contains(&(column, row.saturating_add(1))), BORDER_D),
                        ] {
                            if present {
                                mask |= bit;
                            }
                        }
                        let cell_type = cell_type_of(mask);
                        let glyph = border_arrow(model, indicators, column, row)
                            .map_or_else(|| border_glyph(lines, cell_type, index), str::to_owned);
                        border.insert((row, column), (Rc::clone(&sgr), glyph));
                    }
                }
            }
            write_border_runs(&mut self.output, &border, model.size.columns);
        }

        if force {
            self.paint_empty_window(model);
        }
        let tiled = model.layout.tiled().len();
        let mut touched: Vec<Rect> = Vec::new();
        for (index, entry) in model.layout.panes.iter().enumerate() {
            let float = index
                .checked_sub(tiled)
                .and_then(|index| model.layout.floats.get(index));
            let entry_force = force
                || float.is_some_and(|float| {
                    touched.iter().any(|rect| rects_overlap(*rect, float.frame))
                });
            let mark = self.output.len();
            if let Some(float) = float
                && entry_force
            {
                self.paint_float_border(float, model);
            }
            self.paint_entry(
                model,
                entry,
                entry_force,
                cleared_to_default && float.is_none(),
            );
            if self.output.len() != mark {
                touched.push(float.map_or_else(|| entry_extent(entry), |float| float.frame));
            }
        }
        self.painted
            .retain(|pane, _| model.layout.panes.iter().any(|entry| entry.pane == *pane));
        self.damage
            .retain(|pane, _| model.layout.panes.iter().any(|entry| entry.pane == *pane));
        self.picker_cards
            .retain(|pane, _| model.layout.panes.iter().any(|entry| entry.pane == *pane));
        self.cards
            .retain(|pane, _| model.layout.panes.iter().any(|entry| entry.pane == *pane));
        self.browser_painted
            .retain(|pane, _| model.layout.panes.iter().any(|entry| entry.pane == *pane));
        self.pane_modes_painted
            .retain(|pane, _| model.layout.panes.iter().any(|entry| entry.pane == *pane));

        if let Some(display) = &model.display_panes {
            for indicator in &display.indicators {
                if let Some(entry) = model.pane_rect(indicator.pane) {
                    for write in
                        crate::overlay::display_panes_writes(display, indicator, entry.content())
                    {
                        let mut line = StyledLine::from_segments(vec![write.segment]);
                        line.resolve_theme(&model.status.theme);
                        write_styled_text(
                            &mut self.output,
                            write.column,
                            write.row,
                            &line,
                            model.appearance.foreground,
                            model.appearance.background,
                            &model.appearance,
                        );
                    }
                }
            }
        }
    }
    pub(super) fn paint_entry(
        &mut self,
        model: &Model,
        entry: &PaneRect,
        force: bool,
        cleared_to_default: bool,
    ) {
        let active = model.active_pane() == Some(entry.pane);
        let floating = model.is_float(entry.pane);
        let lines = model.pane_lines(entry.pane);
        let Some(pane) = model.pane_snapshot(entry.pane) else {
            return;
        };
        let header_changed = if entry.border_status.is_on() {
            let header = pane.border_status_text.clone();
            let changed = self.headers.get(&entry.pane) != Some(&header);
            if force || changed {
                self.paint_border_status_row(
                    entry.status_row(),
                    &header,
                    active,
                    entry.pane,
                    lines,
                    model.pane_index(entry.pane),
                    model,
                );
                self.headers.insert(entry.pane, header);
            }
            changed
        } else {
            self.headers.remove(&entry.pane).is_some()
        };
        let content = entry.content();
        let known_blank = !floating && self.default_blank.remove(&entry.pane) == Some(content);
        let browser_live = matches!(pane.kind, PaneKindSnapshot::Browser(_))
            && self.browser_frame_live(entry.pane);
        let browser_state_changed = if matches!(pane.kind, PaneKindSnapshot::Browser(_)) {
            self.browser_painted.insert(entry.pane, browser_live) != Some(browser_live)
        } else {
            self.browser_painted.remove(&entry.pane);
            false
        };
        match &pane.kind {
            PaneKindSnapshot::Terminal => {
                self.picker_cards.remove(&entry.pane);
                let pane_mode_changed = self
                    .pane_modes_painted
                    .insert(entry.pane, pane.mode.is_some())
                    != Some(pane.mode.is_some());
                if let Some(pane_mode) = pane.mode.as_ref() {
                    self.damage.remove(&entry.pane);
                    self.painted.remove(&entry.pane);
                    self.paint_pane_mode(pane_mode, entry, model);
                    return;
                }
                let force = force || pane_mode_changed;
                if let Some(viewport) = model.pane_viewport(entry.pane) {
                    let viewport = &*source_viewport(viewport, entry.source);
                    let damage = self
                        .damage
                        .remove(&entry.pane)
                        .map(|damage| source_damage(damage, entry.source.1));
                    let mode = crate::mode_view::presentation(model, entry.pane, viewport);
                    self.selection_style = mode.and_then(|mode| {
                        crate::mode_view::resolved_style(&mode.selection_style, &model.status.theme)
                    });
                    self.selection_trim = mode
                        .filter(|mode| !mode.vi_keys)
                        .and_then(|_| crate::mode_view::emacs_selection_trim(viewport));
                    self.match_styles = mode.map_or([None, None], |mode| {
                        [&mode.match_style, &mode.current_match_style].map(|style| {
                            crate::mode_view::resolved_style(style, &model.status.theme)
                        })
                    });
                    self.current_line = mode.and_then(|mode| {
                        crate::mode_view::current_line(mode, viewport, &model.status.theme)
                    });
                    let gutter =
                        mode.and_then(|mode| crate::mode_view::line_number_gutter(mode, viewport));
                    let body = gutter.map_or(content, |gutter| gutter.body(content));
                    self.blank_is_default = cleared_to_default || known_blank;
                    self.terminal_defaults = model.pane_window_style(entry.pane);
                    self.paint_terminal(entry.pane, viewport, body, force, damage.as_ref());
                    self.terminal_defaults = TmuxStyle::default();
                    self.blank_is_default = false;
                    if let Some(mode) = mode {
                        self.paint_copy_chrome(mode, gutter, viewport, content, model);
                    }
                    self.selection_style = None;
                    self.selection_trim = None;
                    self.match_styles = [None, None];
                    self.current_line = None;
                } else if floating {
                    self.painted.remove(&entry.pane);
                    if force {
                        self.paint_background(content, model);
                    }
                } else if cleared_to_default || known_blank {
                    self.painted.remove(&entry.pane);
                    self.default_blank.insert(entry.pane, content);
                }
            }
            PaneKindSnapshot::Browser(_) if browser_live => {
                self.picker_cards.remove(&entry.pane);
                if force || header_changed || browser_state_changed {
                    self.paint_background(content, model);
                }
            }
            PaneKindSnapshot::Picker if active => {
                let card_state = (content, model.picker_selection);
                if force || self.picker_cards.get(&entry.pane) != Some(&card_state) {
                    self.paint_picker_card(content, model.picker_selection, model);
                    self.picker_cards.insert(entry.pane, card_state);
                }
            }
            kind => {
                self.picker_cards.remove(&entry.pane);
                let (label, detail) = placeholder_text(kind);
                let card = (content, label, pane.title.clone(), detail);
                if force
                    || header_changed
                    || browser_state_changed
                    || self.cards.get(&entry.pane) != Some(&card)
                {
                    self.paint_card(content, card.1, &card.2, &card.3, model);
                    self.cards.insert(entry.pane, card);
                }
            }
        }
    }
    fn paint_float_border(&mut self, float: &FloatFrame, model: &Model) {
        let style = model.pane_border_style(float.pane);
        let fallback = if float.active {
            model.appearance.link_color
        } else {
            model.appearance.foreground
        };
        let color = model
            .pane_border_colour(float.pane, float.active)
            .map_or(fallback, |colour| {
                resolve_tmux_colour(colour, fallback, &model.appearance)
            });
        let mut sgr = Vec::new();
        if let Some(style) = &style {
            write_border_sgr(&mut sgr, style, &model.appearance);
        } else {
            write_colored_sgr(&mut sgr, color, model.appearance.background);
        }
        let sgr = Rc::new(sgr);
        let index = model.pane_index(float.pane);
        let border: BTreeMap<(u16, u16), (Rc<Vec<u8>>, String)> = float
            .border_cells()
            .into_iter()
            .map(|(column, row, cell_type)| {
                (
                    (row, column),
                    (Rc::clone(&sgr), border_glyph(float.lines, cell_type, index)),
                )
            })
            .collect();
        write_border_runs(&mut self.output, &border, model.size.columns);
    }
    fn paint_empty_window(&mut self, model: &Model) {
        let Some(window) = model.window() else {
            return;
        };
        if window.zoomed_pane.is_some() || !matches!(window.layout, zz_protocol::LayoutNode::Empty)
        {
            return;
        }
        let canvas = model.canvas();
        if canvas.width == 0 {
            return;
        }
        let mut line = StyledLine::parsed(&format!(
            "#[bg=themedarkgrey]{}",
            " ".repeat(usize::from(canvas.width))
        ));
        line.resolve_theme(&model.status.theme);
        for row in canvas.y..canvas.y.saturating_add(canvas.height) {
            write_styled_text(
                &mut self.output,
                canvas.x,
                row,
                &line,
                model.appearance.foreground,
                model.appearance.background,
                &model.appearance,
            );
        }
    }
    fn paint_border_status_row(
        &mut self,
        rect: Rect,
        expanded: &str,
        active: bool,
        pane: PaneId,
        lines: PaneBorderLines,
        index: Option<u32>,
        model: &Model,
    ) {
        if rect.height == 0 || rect.width == 0 {
            return;
        }
        let fallback = if active {
            model.appearance.link_color
        } else {
            model.appearance.foreground
        };
        let color = model
            .pane_border_colour(pane, active)
            .map_or(fallback, |colour| {
                resolve_tmux_colour(colour, fallback, &model.appearance)
            });
        let glyph = border_glyph(lines, CELL_LR, index);
        let lead = 2.min(rect.width);
        let width = rect.width.saturating_sub(lead);
        let underlay = vec![glyph.clone(); usize::from(width)];
        let composed = zz_client::compose_status_row_over(expanded, &underlay, "");
        let mut line = StyledLine::default();
        for _ in 0..lead {
            line.push_plain(&glyph);
        }
        line.append(&StyledLine::from_segments(composed.segments));
        let line = line.truncate(usize::from(rect.width));
        if let Some(style) = model.pane_border_style(pane) {
            write_styled_text_over(
                &mut self.output,
                rect.x,
                rect.y,
                &line,
                &style,
                &model.appearance,
            );
            return;
        }
        write_styled_text(
            &mut self.output,
            rect.x,
            rect.y,
            &line,
            color,
            model.appearance.background,
            &model.appearance,
        );
    }
    pub(super) fn paint_background(&mut self, rect: Rect, model: &Model) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        let line = " ".repeat(usize::from(rect.width));
        for row in 0..rect.height {
            write_colored_text(
                &mut self.output,
                rect.x,
                rect.y.saturating_add(row),
                &line,
                model.appearance.foreground,
                model.appearance.background,
            );
        }
    }
}

pub(super) fn source_viewport(
    viewport: &TerminalViewport,
    source: (u16, u16),
) -> std::borrow::Cow<'_, TerminalViewport> {
    if source == (0, 0) {
        return std::borrow::Cow::Borrowed(viewport);
    }
    let (left, top) = source;
    let columns = viewport.columns.saturating_sub(left);
    let rows = viewport.rows.saturating_sub(top);
    let mut cropped = viewport.clone();
    cropped.columns = columns;
    cropped.rows = rows;
    cropped.cells = (top..viewport.rows)
        .filter_map(|row| viewport.row(row))
        .flat_map(|cells| cells.iter().skip(usize::from(left)).copied())
        .collect();
    cropped.overlays = viewport
        .overlays
        .iter()
        .filter(|overlay| overlay.row >= top && overlay.end > left)
        .map(|overlay| {
            let mut overlay = *overlay;
            overlay.row -= top;
            overlay.start = overlay.start.saturating_sub(left);
            overlay.end -= left;
            overlay
        })
        .collect();
    cropped.kitty_placements = Arc::from([]);
    cropped.cursor = viewport.cursor.and_then(|cursor| {
        let column = cursor.column().checked_sub(left)?;
        let row = cursor.row().checked_sub(top)?;
        Some(zz_terminal::Cursor::new(
            column,
            row,
            cursor.visible(),
            cursor.blinking(),
            cursor.at_wide_tail(),
            cursor.style(),
            cursor.color(),
        ))
    });
    std::borrow::Cow::Owned(cropped)
}

fn source_damage(damage: FrameDamage, top: u16) -> FrameDamage {
    match damage {
        FrameDamage::Rows(rows) if top != 0 => FrameDamage::Rows(
            rows.into_iter()
                .filter_map(|row| row.checked_sub(top))
                .collect(),
        ),
        damage => damage,
    }
}

pub(super) const fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x.saturating_add(b.width)
        && b.x < a.x.saturating_add(a.width)
        && a.y < b.y.saturating_add(b.height)
        && b.y < a.y.saturating_add(a.height)
}

fn entry_extent(entry: &PaneRect) -> Rect {
    let status = entry.status_row();
    if status.height == 0 {
        return entry.rect;
    }
    let top = entry.rect.y.min(status.y);
    let bottom = entry
        .rect
        .y
        .saturating_add(entry.rect.height)
        .max(status.y.saturating_add(1));
    Rect {
        x: entry.rect.x,
        y: top,
        width: entry.rect.width,
        height: bottom - top,
    }
}

fn divider_cells(dividers: &[Divider]) -> BTreeSet<(u16, u16)> {
    let mut cells = BTreeSet::new();
    for divider in dividers {
        for row in divider.rect.y..divider.rect.y.saturating_add(divider.rect.height) {
            for column in divider.rect.x..divider.rect.x.saturating_add(divider.rect.width) {
                cells.insert((column, row));
            }
        }
    }
    cells
}

fn border_arrow(
    model: &Model,
    indicators: PaneBorderIndicators,
    column: u16,
    row: u16,
) -> Option<&'static str> {
    if !indicators.arrows() {
        return None;
    }
    let marked = model.layout.panes.iter().any(|entry| {
        let rect = entry.rect;
        let vertical = column == rect.x.saturating_add(1)
            && ((rect.y > 0 && row == rect.y.saturating_sub(1))
                || row == rect.y.saturating_add(rect.height));
        let horizontal = row == rect.y.saturating_add(1)
            && ((rect.x > 0 && column == rect.x.saturating_sub(1))
                || column == rect.x.saturating_add(rect.width));
        vertical || horizontal
    });
    if !marked {
        return None;
    }
    let rect = model.pane_rect(model.active_pane()?)?.rect;
    let within_columns =
        column >= rect.x.saturating_sub(1) && column <= rect.x.saturating_add(rect.width);
    let within_rows = row >= rect.y.saturating_sub(1) && row <= rect.y.saturating_add(rect.height);
    if column == rect.x.saturating_add(rect.width) && within_rows {
        return Some("←");
    }
    if rect.x > 0 && column == rect.x.saturating_sub(1) && within_rows {
        return Some("→");
    }
    if row == rect.y.saturating_add(rect.height) && within_columns {
        return Some("↑");
    }
    if rect.y > 0 && row == rect.y.saturating_sub(1) && within_columns {
        return Some("↓");
    }
    None
}

fn placeholder_text(kind: &PaneKindSnapshot) -> (&'static str, String) {
    match kind {
        PaneKindSnapshot::Picker => ("Pane picker", "choose a pane in the zz app".to_owned()),
        PaneKindSnapshot::Browser(browser) => ("Browser", browser.url().to_owned()),
        PaneKindSnapshot::Agent(agent) => (
            "Agent",
            agent.cwd.as_ref().map_or_else(
                || agent.provider.label().to_owned(),
                |cwd| format!("{} · {}", agent.provider.label(), cwd.display()),
            ),
        ),
        PaneKindSnapshot::Editor(editor) => (
            "Editor",
            editor.path.clone().unwrap_or_else(|| editor.cwd.clone()),
        ),
        PaneKindSnapshot::Terminal => ("Terminal", String::new()),
        PaneKindSnapshot::View(view) => (
            view.content.kind_name(),
            format!("{} · open it in the zz app", view.title),
        ),
        PaneKindSnapshot::Other { kind } => {
            ("Unknown pane", zz_client::navigation::newer_pane_text(kind))
        }
    }
}
