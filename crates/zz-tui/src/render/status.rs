use super::*;

impl Renderer {
    pub(super) fn paint_pane_prompt(&mut self, model: &Model) {
        let Some((prompt, content)) = pane_prompt_target(model) else {
            return;
        };
        let view = crate::overlay::prompt_view(
            &prompt.prompt,
            &prompt.input,
            prompt.cursor,
            content.width,
        );
        let style = crate::mode_view::message_style(model, false);
        let mut line = StyledLine::from_segments(crate::mode_view::over_underlay(
            &crate::mode_view::message_front(&view.text, content.width, &style),
            None,
            &[],
            content.width,
        ));
        line.resolve_theme(&model.status.theme);
        write_styled_text(
            &mut self.output,
            content.x,
            pane_prompt_row_y(model, content),
            &line,
            model.appearance.foreground,
            model.appearance.background,
            &model.appearance,
        );
    }
    pub(super) fn paint_status_area(&mut self, model: &Model) {
        if model.sidebar_visible() {
            let tree_height = usize::from(model.sidebar_tree_height());
            for (offset, text) in sidebar_status_lines(model).into_iter().enumerate() {
                let index = tree_height.saturating_add(offset);
                let row = PaintedSidebarRow {
                    text,
                    selected: false,
                    status: true,
                };
                if self.sidebar_rows.get(index) == Some(&row) {
                    continue;
                }
                write_styled_text(
                    &mut self.output,
                    0,
                    u16::try_from(index).unwrap_or(u16::MAX),
                    &row.text,
                    model.appearance.background,
                    model.appearance.foreground,
                    &model.appearance,
                );
                if let Some(cached) = self.sidebar_rows.get_mut(index) {
                    *cached = row;
                }
            }
        }
        self.paint_status_block(model, false);
    }
    pub(super) fn paint_status_block(&mut self, model: &Model, force: bool) {
        let (x, width) = model.status_area();
        self.paint_status_block_in(model, x, width, force);
    }
    fn paint_status_block_in(&mut self, model: &Model, x: u16, width: u16, force: bool) {
        if model.size.rows == 0 || width == 0 {
            return;
        }
        let block = usize::from(model.status_block_rows());
        let overlay = status_overlay(model, width);
        let origin = model.status_origin_y();
        let geometry = (x, origin, width);
        if !force
            && overlay.is_none()
            && self.status_geometry == Some(geometry)
            && let Some(source) = self.status_source.as_mut()
            && (Arc::ptr_eq(source, &model.status) || **source == *model.status)
        {
            *source = Arc::clone(&model.status);
            return;
        }
        self.status_source = overlay.is_none().then(|| Arc::clone(&model.status));
        let mut lines = Vec::with_capacity(block);
        for index in 0..block {
            let row = model.status.rows.get(index).map_or("", String::as_str);
            let composed = zz_client::compose_status_row(row, width, &model.status.base_style);
            let mut line = StyledLine::from_segments(composed.segments);
            if usize::from(model.status.message_line).min(block.saturating_sub(1)) == index
                && let Some(StatusOverlay { front, fill }) = &overlay
            {
                line = StyledLine::from_segments(crate::mode_view::over_underlay(
                    &front.segments,
                    *fill,
                    &line.segments,
                    width,
                ));
            }
            line.resolve_theme(&model.status.theme);
            lines.push(line);
        }
        let force = force || self.status_geometry != Some(geometry);
        for (index, line) in lines.iter().enumerate() {
            let row = origin.saturating_add(u16::try_from(index).unwrap_or(u16::MAX));
            let previous = self.status_rows.get(index).filter(|_| !force);
            if previous == Some(line) {
                continue;
            }
            write_styled_text(
                &mut self.output,
                x,
                row,
                line,
                model.appearance.foreground,
                model.appearance.background,
                &model.appearance,
            );
        }
        self.status_rows = lines;
        self.status_geometry = Some(geometry);
        let virtual_row = overlay.map(|StatusOverlay { front, fill }| {
            StyledLine::from_segments(crate::mode_view::over_underlay(
                &front.segments,
                fill,
                &[],
                width,
            ))
        });
        if block == 0
            && let Some(mut line) = virtual_row
            && let Some(y) = model.message_row_y()
        {
            line.resolve_theme(&model.status.theme);
            write_styled_text(
                &mut self.output,
                x,
                y,
                &line,
                model.appearance.foreground,
                model.appearance.background,
                &model.appearance,
            );
        }
    }
}

pub(super) fn client_message_hides_the_cursor(model: &Model) -> bool {
    model.client_message.is_some() && !model.sidebar_visible()
}

pub(super) fn pane_prompt_target(
    model: &Model,
) -> Option<(&zz_protocol::CommandPromptState, Rect)> {
    let prompt = model.command_prompt.as_ref()?;
    let content = model.pane_rect(prompt.pane?)?.content();
    (content.width > 0 && content.height > 0).then_some((prompt, content))
}

pub(super) fn pane_prompt_row_y(model: &Model, content: Rect) -> u16 {
    if model.status_top() {
        content.y
    } else {
        content.y.saturating_add(content.height).saturating_sub(1)
    }
}

pub(super) fn status_overlay(model: &Model, width: u16) -> Option<StatusOverlay> {
    let filled = model.status_block_rows() > 0;
    let message = |text: &str, command: bool| {
        let message_style = crate::mode_view::message_style(model, command);
        StatusOverlay {
            front: StyledLine::from_segments(crate::mode_view::message_front(
                if filled {
                    text.trim_end_matches(' ')
                } else {
                    text
                },
                width,
                &message_style,
            )),
            fill: message_style.fill,
        }
    };
    if let Some(confirm) = &model.confirm {
        if model.sidebar_visible() {
            return None;
        }
        let view = crate::overlay::prompt_view(&confirm.prompt, "", 0, width);
        return Some(message(&view.text, false));
    }
    if model.command_output_focus().is_some()
        && let Some(query) = &model.command_output_search
    {
        return Some(message(
            &truncate(
                &command_output_search_prompt(query),
                width.saturating_sub(1),
            ),
            false,
        ));
    }
    if model.sidebar_visible() {
        return None;
    }
    if let Some(prompt) = &model.command_prompt
        && pane_prompt_target(model).is_none()
    {
        let view = crate::overlay::prompt_view(&prompt.prompt, &prompt.input, prompt.cursor, width);
        return Some(message(&view.text, prompt.command_mode));
    }
    model
        .client_message
        .as_ref()
        .map(|client_message| message(&client_message.text, false))
}

pub(super) fn base_status_left(model: &Model) -> StyledLine {
    let mut left = StyledLine::parsed(&model.status.left);
    if let Some(session) = model.session() {
        if !left.is_empty() {
            left.push_plain(" ");
        }
        left.push_plain(&format!("[{}]", session.name));
        for window in &session.windows {
            append_status_label(&mut left, &window.status_label);
        }
    }
    left
}

pub(super) fn append_status_label(line: &mut StyledLine, value: &str) {
    let label = StyledLine::parsed(value);
    if label.is_empty() {
        return;
    }
    line.push_plain(" ");
    line.append(&label);
}

pub(super) fn mode_indicator(mode: TerminalMode) -> String {
    match mode {
        TerminalMode::Live => String::new(),
        TerminalMode::Copy {
            hide_position: true,
            ..
        } => "COPY".to_owned(),
        TerminalMode::Copy {
            position, total, ..
        } => format!("COPY {position}/{total}"),
        TerminalMode::View { position, total } => format!("VIEW {position}/{total}"),
    }
}

pub(super) fn mode_indicators(model: &Model) -> String {
    let mut indicators = String::new();
    let viewport = model
        .command_output_focus()
        .and_then(|pane| model.pane_viewport(pane))
        .or_else(|| model.active_viewport());
    if let Some(viewport) = viewport {
        match viewport.mode {
            TerminalMode::Live => {}
            TerminalMode::Copy { .. } | TerminalMode::View { .. } => {
                indicators.push_str(&mode_indicator(viewport.mode));
            }
        }
        if let Some(search) = viewport.search {
            if !indicators.is_empty() {
                indicators.push_str("  ");
            }
            write!(indicators, "search {}/{}", search.current(), search.total)
                .expect("writing to String cannot fail");
        }
    }
    indicators
}

pub(super) fn status_indicators(model: &Model) -> String {
    let mut indicators = mode_indicators(model);
    if model.prefix_armed {
        if !indicators.is_empty() {
            indicators.push_str("  ");
        }
        indicators.push_str("PREFIX");
    }
    indicators
}

pub(super) fn command_output_search_prompt(query: &SearchQuery) -> String {
    let prefix = match query.direction {
        SearchDirection::Forward => '/',
        SearchDirection::Backward => '?',
    };
    format!("{prefix}{}", query.text)
}

pub(super) fn command_output_search_display(query: &SearchQuery, width: u16) -> (String, u16) {
    let prompt = command_output_search_prompt(query);
    let visible = truncate(&prompt, width.saturating_sub(1));
    let cursor = u16::try_from(text_display_width(&visible)).unwrap_or(u16::MAX);
    (padded_segment(&visible, width, ' '), cursor)
}

pub(super) fn combine_status(left: &StyledLine, right: &StyledLine, width: u16) -> StyledLine {
    let width = usize::from(width);
    let right = right.truncate(width);
    let right_len = right.len();
    let left_width = width.saturating_sub(right_len + usize::from(!right.is_empty()));
    let mut left = left.truncate(left_width);
    let left_len = left.len();
    let gap = width.saturating_sub(left_len + right_len);
    left.push_plain(&" ".repeat(gap));
    left.append(&right);
    left
}

pub(super) fn padded_styled(line: &StyledLine, width: u16, fill: char) -> StyledLine {
    let mut line = line.truncate(usize::from(width));
    let padding = usize::from(width).saturating_sub(line.len());
    line.push_plain(&fill_cells(fill, padding));
    line
}

pub(super) fn padded_segment(text: &str, width: u16, fill: char) -> String {
    let text = truncate(text, width);
    let padding = usize::from(width).saturating_sub(text_display_width(&text));
    format!("{text}{}", fill_cells(fill, padding))
}

pub(super) fn truncate(text: &str, width: u16) -> String {
    truncate_display_cells(text, usize::from(width))
}

pub(super) fn truncate_display_cells(text: &str, width: usize) -> String {
    let mut used = 0_usize;
    text.chars()
        .take_while(|character| {
            let next = used.saturating_add(character.width().unwrap_or(0));
            if next > width {
                false
            } else {
                used = next;
                true
            }
        })
        .collect()
}

pub(super) fn text_display_width(text: &str) -> usize {
    text.chars()
        .map(|character| character.width().unwrap_or(0))
        .sum()
}

fn fill_cells(fill: char, width: usize) -> String {
    let fill_width = fill.width().unwrap_or(0);
    if fill_width == 0 {
        return " ".repeat(width);
    }
    let count = width / fill_width;
    let remainder = width % fill_width;
    format!(
        "{}{}",
        fill.to_string().repeat(count),
        " ".repeat(remainder)
    )
}
