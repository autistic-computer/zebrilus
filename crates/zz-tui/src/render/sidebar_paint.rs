use super::*;

impl Renderer {
    pub(super) fn paint_sidebar(&mut self, model: &Model, force: bool) {
        let rows = sidebar_rows(model);
        for (index, row) in rows.iter().enumerate() {
            if !force && self.sidebar_rows.get(index) == Some(row) {
                continue;
            }
            let (foreground, background) = if row.selected {
                (model.appearance.background, model.appearance.link_color)
            } else if row.status {
                (model.appearance.background, model.appearance.foreground)
            } else {
                (model.appearance.foreground, model.appearance.background)
            };
            write_styled_text(
                &mut self.output,
                0,
                u16::try_from(index).unwrap_or(u16::MAX),
                &row.text,
                foreground,
                background,
                &model.appearance,
            );
        }
        if force {
            for row in 0..model.size.rows {
                write_colored_text(
                    &mut self.output,
                    sidebar::WIDTH,
                    row,
                    "│",
                    model.appearance.foreground,
                    model.appearance.background,
                );
            }
        }
        self.sidebar_rows = rows;
    }
}

fn sidebar_rows(model: &Model) -> Vec<PaintedSidebarRow> {
    let tree = model.sidebar_rows();
    let edit_row = model.sidebar_edit_row();
    let tree_height = model.sidebar_tree_height();
    let mut rows = Vec::with_capacity(usize::from(model.size.rows));
    for visible_row in 0..tree_height {
        let index = model
            .sidebar
            .scroll
            .saturating_add(usize::from(visible_row));
        let text = if edit_row == Some(index) {
            model
                .sidebar_edit
                .as_ref()
                .map_or_else(String::new, |edit| edit.viewport(sidebar::WIDTH).0)
        } else {
            tree.get(index)
                .map_or_else(String::new, |row| row.text.clone())
        };
        rows.push(PaintedSidebarRow {
            text: padded_styled(&StyledLine::plain(&text), sidebar::WIDTH, ' '),
            selected: model.sidebar_focused() && index == model.sidebar.selected,
            status: false,
        });
    }
    for text in sidebar_status_lines(model) {
        rows.push(PaintedSidebarRow {
            text,
            selected: false,
            status: true,
        });
    }
    rows
}

pub(super) fn sidebar_status_lines(model: &Model) -> Vec<StyledLine> {
    let line_count = usize::from(model.size.rows.min(sidebar::STATUS_ROWS));
    if line_count == 0 {
        return Vec::new();
    }
    if let Some(confirm) = &model.confirm {
        let mut lines =
            vec![StyledLine::plain(&" ".repeat(usize::from(sidebar::WIDTH))); line_count];
        lines[line_count - 1] =
            padded_styled(&StyledLine::plain(&confirm.prompt), sidebar::WIDTH, ' ');
        return lines;
    }
    if let Some(prompt) = &model.command_prompt {
        let mut lines =
            vec![StyledLine::plain(&" ".repeat(usize::from(sidebar::WIDTH))); line_count];
        lines[line_count - 1] = padded_styled(
            &StyledLine::plain(&format!("{}{}", prompt.prompt, prompt.input)),
            sidebar::WIDTH,
            ' ',
        );
        return lines;
    }

    let mut base = combine_status(
        &base_status_left(model),
        &StyledLine::parsed(&model.status.right),
        sidebar::WIDTH,
    );
    base.resolve_theme(&model.status.theme);
    let indicators = padded_styled(
        &StyledLine::plain(&status_indicators(model)),
        sidebar::WIDTH,
        ' ',
    );
    let message = combine_status(
        &StyledLine::plain(
            model
                .client_message
                .as_ref()
                .map_or("", |message| message.text.as_str()),
        ),
        &StyledLine::plain(if model.status.customized {
            ""
        } else {
            "C-b d detach"
        }),
        sidebar::WIDTH,
    );
    [base, indicators, message]
        .into_iter()
        .skip(usize::from(sidebar::STATUS_ROWS).saturating_sub(line_count))
        .collect()
}
