use super::*;

impl Renderer {
    pub(super) fn paint_card(
        &mut self,
        rect: Rect,
        kind: &str,
        title: &str,
        detail: &str,
        model: &Model,
    ) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        self.paint_background(rect, model);
        if rect.width < 4 || rect.height < 3 {
            return;
        }
        let top = format!(
            "┌{}┐",
            "─".repeat(usize::from(rect.width.saturating_sub(2)))
        );
        let bottom = format!(
            "└{}┘",
            "─".repeat(usize::from(rect.width.saturating_sub(2)))
        );
        write_colored_text(
            &mut self.output,
            rect.x,
            rect.y,
            &top,
            model.appearance.foreground,
            model.appearance.background,
        );
        for row in 1..rect.height.saturating_sub(1) {
            write_colored_text(
                &mut self.output,
                rect.x,
                rect.y.saturating_add(row),
                "│",
                model.appearance.foreground,
                model.appearance.background,
            );
            write_colored_text(
                &mut self.output,
                rect.x.saturating_add(rect.width.saturating_sub(1)),
                rect.y.saturating_add(row),
                "│",
                model.appearance.foreground,
                model.appearance.background,
            );
        }
        write_colored_text(
            &mut self.output,
            rect.x,
            rect.y.saturating_add(rect.height.saturating_sub(1)),
            &bottom,
            model.appearance.foreground,
            model.appearance.background,
        );
        for (offset, text) in [kind, title, detail, "open in the zz app"]
            .into_iter()
            .enumerate()
        {
            let row = rect
                .y
                .saturating_add(1 + u16::try_from(offset).unwrap_or(u16::MAX));
            if row >= rect.y.saturating_add(rect.height.saturating_sub(1)) {
                break;
            }
            let text = truncate(text, rect.width.saturating_sub(4));
            write_colored_text(
                &mut self.output,
                rect.x.saturating_add(2),
                row,
                &text,
                model.appearance.foreground,
                model.appearance.background,
            );
        }
    }
    pub(super) fn paint_picker_card(&mut self, rect: Rect, selected: usize, model: &Model) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        for row in 0..rect.height {
            write_colored_text(
                &mut self.output,
                rect.x,
                rect.y.saturating_add(row),
                &" ".repeat(usize::from(rect.width)),
                model.appearance.foreground,
                model.appearance.background,
            );
        }

        let card_width = rect.width.min(38);
        let card_height = rect.height.min(8);
        if card_width < 4 || card_height < 3 {
            return;
        }
        let card = Rect {
            x: rect
                .x
                .saturating_add(rect.width.saturating_sub(card_width) / 2),
            y: rect
                .y
                .saturating_add(rect.height.saturating_sub(card_height) / 2),
            width: card_width,
            height: card_height,
        };
        let inner_width = card.width.saturating_sub(2);
        let top = format!(
            "┌{}┐",
            padded_segment(" Select pane kind ", inner_width, '─')
        );
        let bottom = format!("└{}┘", "─".repeat(usize::from(inner_width)));
        write_colored_text(
            &mut self.output,
            card.x,
            card.y,
            &top,
            model.appearance.link_color,
            model.appearance.background,
        );
        for row in 1..card.height.saturating_sub(1) {
            let absolute_row = card.y.saturating_add(row);
            write_colored_text(
                &mut self.output,
                card.x,
                absolute_row,
                "│",
                model.appearance.foreground,
                model.appearance.background,
            );
            write_colored_text(
                &mut self.output,
                card.x.saturating_add(card.width.saturating_sub(1)),
                absolute_row,
                "│",
                model.appearance.foreground,
                model.appearance.background,
            );
        }
        for (index, choice) in picker::CHOICES.into_iter().enumerate() {
            let row = card
                .y
                .saturating_add(1 + u16::try_from(index).unwrap_or(u16::MAX));
            if row >= card.y.saturating_add(card.height.saturating_sub(1)) {
                break;
            }
            let is_selected = index == selected;
            let line = padded_segment(
                &format!("{} {}", if is_selected { '>' } else { ' ' }, choice.label()),
                inner_width,
                ' ',
            );
            write_colored_text(
                &mut self.output,
                card.x.saturating_add(1),
                row,
                &line,
                if is_selected {
                    model.appearance.background
                } else {
                    model.appearance.foreground
                },
                if is_selected {
                    model.appearance.link_color
                } else {
                    model.appearance.background
                },
            );
        }
        if card.height >= 7 {
            let hint = padded_segment(" ↑/↓ or j/k · Enter · Esc", inner_width, ' ');
            write_colored_text(
                &mut self.output,
                card.x.saturating_add(1),
                card.y.saturating_add(card.height.saturating_sub(2)),
                &hint,
                model.appearance.foreground,
                model.appearance.background,
            );
        }
        write_colored_text(
            &mut self.output,
            card.x,
            card.y.saturating_add(card.height.saturating_sub(1)),
            &bottom,
            model.appearance.link_color,
            model.appearance.background,
        );
    }
    pub(super) fn paint_menu(&mut self, model: &Model) {
        let Some(state) = model.menu.as_ref() else {
            return;
        };
        let Some(layout) = resolve_floating(
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
        ) else {
            return;
        };
        let rect = layout.frame;
        if rect.width < 4 || rect.height < 2 {
            return;
        }
        let border = floating_border(state.border_lines);
        let border_style =
            crate::overlay::grounded(parse_style(&state.border_style).unwrap_or_default());
        let horizontal = border
            .horizontal
            .repeat(usize::from(rect.width.saturating_sub(2)));
        let mut top = StyledLine::default();
        top.push_segment(
            &format!("{}{}{}", border.top_left, horizontal, border.top_right),
            border_style.clone(),
        );
        write_styled_text(
            &mut self.output,
            rect.x,
            rect.y,
            &top,
            model.appearance.foreground,
            model.appearance.background,
            &model.appearance,
        );

        let content_width = rect.width.saturating_sub(4);
        if !state.title.is_empty() && content_width > 0 {
            let title = crate::overlay::compose_over(
                &state.title,
                content_width,
                &state.border_style,
                border.horizontal,
                &border_style,
            );
            write_styled_text(
                &mut self.output,
                rect.x.saturating_add(2),
                rect.y,
                &StyledLine::from_segments(title),
                model.appearance.foreground,
                model.appearance.background,
                &model.appearance,
            );
        }

        for offset in 0..rect.height.saturating_sub(2) {
            let index = usize::from(offset);
            let row = rect.y.saturating_add(offset).saturating_add(1);
            match state.items.get(index) {
                Some(None) => {
                    let mut line = StyledLine::default();
                    line.push_segment(
                        &format!(
                            "{}{}{}",
                            border.separator_left, horizontal, border.separator_right
                        ),
                        border_style.clone(),
                    );
                    write_styled_text(
                        &mut self.output,
                        rect.x,
                        row,
                        &line,
                        model.appearance.foreground,
                        model.appearance.background,
                        &model.appearance,
                    );
                }
                Some(Some(item)) => {
                    let selected = item.enabled && model.menu_selection == Some(index);
                    let base_style = if selected {
                        state.selected_style.clone()
                    } else if item.enabled {
                        state.style.clone()
                    } else {
                        format!("{},dim", state.style)
                    };
                    let content = item
                        .annotation
                        .as_deref()
                        .filter(|key| !key.is_empty())
                        .map_or_else(
                            || item.name.clone(),
                            |key| format!("{}#[default] #[align=right]({key})", item.name),
                        );
                    let padding_style = crate::overlay::grounded(
                        parse_style(if selected {
                            &state.selected_style
                        } else {
                            &state.style
                        })
                        .unwrap_or_default(),
                    );
                    let content = crate::overlay::compose_over(
                        &content,
                        content_width,
                        &base_style,
                        " ",
                        &padding_style,
                    );
                    let mut line = StyledLine::default();
                    line.push_segment(border.vertical, border_style.clone());
                    line.push_segment(" ", padding_style.clone());
                    line.append(&StyledLine::from_segments(content));
                    line.push_segment(" ", padding_style);
                    line.push_segment(border.vertical, border_style.clone());
                    write_styled_text(
                        &mut self.output,
                        rect.x,
                        row,
                        &line,
                        model.appearance.foreground,
                        model.appearance.background,
                        &model.appearance,
                    );
                }
                None => {
                    let base_style =
                        crate::overlay::grounded(parse_style(&state.style).unwrap_or_default());
                    let mut line = StyledLine::default();
                    line.push_segment(border.vertical, border_style.clone());
                    line.push_segment(
                        &" ".repeat(usize::from(rect.width.saturating_sub(2))),
                        base_style,
                    );
                    line.push_segment(border.vertical, border_style.clone());
                    write_styled_text(
                        &mut self.output,
                        rect.x,
                        row,
                        &line,
                        model.appearance.foreground,
                        model.appearance.background,
                        &model.appearance,
                    );
                }
            }
        }

        let mut bottom = StyledLine::default();
        bottom.push_segment(
            &format!(
                "{}{}{}",
                border.bottom_left, horizontal, border.bottom_right
            ),
            border_style,
        );
        write_styled_text(
            &mut self.output,
            rect.x,
            rect.y.saturating_add(rect.height.saturating_sub(1)),
            &bottom,
            model.appearance.foreground,
            model.appearance.background,
            &model.appearance,
        );
    }
}

pub(super) fn floating_border(lines: PopupBorderLines) -> FloatingBorder {
    match lines {
        PopupBorderLines::Single => FloatingBorder {
            top_left: "┌",
            top_right: "┐",
            bottom_left: "└",
            bottom_right: "┘",
            horizontal: "─",
            vertical: "│",
            separator_left: "├",
            separator_right: "┤",
        },
        PopupBorderLines::Double => FloatingBorder {
            top_left: "╔",
            top_right: "╗",
            bottom_left: "╚",
            bottom_right: "╝",
            horizontal: "═",
            vertical: "║",
            separator_left: "╠",
            separator_right: "╣",
        },
        PopupBorderLines::Heavy => FloatingBorder {
            top_left: "┏",
            top_right: "┓",
            bottom_left: "┗",
            bottom_right: "┛",
            horizontal: "━",
            vertical: "┃",
            separator_left: "┣",
            separator_right: "┫",
        },
        PopupBorderLines::Simple => FloatingBorder {
            top_left: "+",
            top_right: "+",
            bottom_left: "+",
            bottom_right: "+",
            horizontal: "-",
            vertical: "|",
            separator_left: "+",
            separator_right: "+",
        },
        PopupBorderLines::Rounded => FloatingBorder {
            top_left: "╭",
            top_right: "╮",
            bottom_left: "╰",
            bottom_right: "╯",
            horizontal: "─",
            vertical: "│",
            separator_left: "├",
            separator_right: "┤",
        },
        PopupBorderLines::Padded | PopupBorderLines::None => FloatingBorder {
            top_left: " ",
            top_right: " ",
            bottom_left: " ",
            bottom_right: " ",
            horizontal: " ",
            vertical: " ",
            separator_left: " ",
            separator_right: " ",
        },
    }
}
