use super::*;

impl MuxEngine {
    pub fn customize_presentation(
        &self,
        pane: PaneId,
        mode: &CustomizeMode,
        expand: &mut CustomizeExpand<'_>,
    ) -> (
        ChooseTreeState,
        ChooserPresentation,
        Option<(String, u16, bool)>,
    ) {
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        let selected = mode.current.min(lines.len().saturating_sub(1));
        let columns = self.customize_screen_columns(pane);
        let screen_rows = self.customize_screen_rows(pane);
        let preview = lines.get(selected).map(|line| {
            let row = &rows[*line];
            let row = if matches!(row.item, Item::KeyField { .. }) {
                row.parent.map_or(row, |parent| &rows[parent])
            } else {
                row
            };
            let width = columns.saturating_sub(4);
            let height = screen_rows.saturating_sub(mode.height).saturating_sub(2);
            let mut writer = PreviewWriter::new(width, height);
            match &row.item {
                Item::Option { .. } => self.customize_draw_option(pane, row, &mut writer, expand),
                Item::Key { table, key } | Item::KeyField { table, key } => {
                    self.customize_draw_key(table, key, &mut writer);
                }
                Item::Environment { name, session, .. } => {
                    self.customize_draw_environment(name, *session, &mut writer);
                }
                Item::Section => {}
            }
            ChooserPreview::Markup {
                lines: writer.finish(),
            }
        });
        let prompt = mode.prompt.as_ref().map(|(prompt, _)| {
            let (text, cursor) = prompt.draw(u16::try_from(columns).unwrap_or(u16::MAX));
            (text, cursor, self.customize_prompt_top(pane))
        });
        let presentation_rows = lines
            .iter()
            .map(|line| {
                let row = &rows[*line];
                ChooserRow {
                    name: row.name.clone(),
                    text: match &row.text {
                        None => String::new(),
                        Some(text) if text.is_empty() => "#[default]".to_owned(),
                        Some(text) => text.clone(),
                    },
                    align: false,
                }
            })
            .collect::<Vec<_>>();
        (
            ChooseTreeState {
                items: lines
                    .iter()
                    .enumerate()
                    .map(|(index, line)| {
                        let row = &rows[*line];
                        let title = if matches!(row.item, Item::KeyField { .. }) {
                            row.parent.map_or(&row.name, |parent| &rows[parent].name)
                        } else {
                            &row.name
                        };
                        ChooseTreeItem {
                            label: row.name.clone(),
                            detail: title.clone(),
                            target: ChooseTreeTarget::Pane(pane),
                            depth: row.depth,
                            flags: if row.children {
                                ChooseTreeItem::HAS_CHILDREN
                            } else {
                                0
                            } | if mode.expanded.contains(&row.id) {
                                ChooseTreeItem::EXPANDED
                            } else {
                                0
                            } | if mode.tagged.contains(&row.id) {
                                ChooseTreeItem::TAGGED
                            } else {
                                0
                            },
                            pane_kind: None,
                            key: if index < 10 {
                                index.to_string()
                            } else if index < 36 {
                                format!("M-{}", char::from(b'a' + (index - 10) as u8))
                            } else {
                                String::new()
                            },
                            text: String::new(),
                        }
                    })
                    .collect(),
                search: None,
                selected: selected as u32,
                kind: ChooseTreeKind::Panes,
                filter_no_matches: false,
                prompt: String::new(),
                help: mode.help,
            },
            ChooserPresentation {
                selected: selected as u32,
                rows: presentation_rows,
                sort: String::new(),
                view: String::new(),
                filter: mode.filter.is_some(),
                selection_style: String::new(),
                border_style: String::new(),
                prompt_style: String::new(),
                preview_size: match mode.preview {
                    Preview::Normal => ChooserPreviewSize::Normal,
                    Preview::Off => ChooserPreviewSize::Off,
                    Preview::Big => ChooserPreviewSize::Big,
                },
                preview,
                prompt_cursor: zz_protocol::PromptCursor::default(),
                prompt_column: 0,
                prompt_line: String::new(),
            },
            prompt,
        )
    }

    #[must_use]
    pub fn customize_offset(mode: &CustomizeMode) -> u32 {
        u32::try_from(mode.offset).unwrap_or(u32::MAX)
    }

    pub(super) fn customize_prompt_top(&self, pane: PaneId) -> bool {
        self.state
            .window_for_pane(pane)
            .and_then(|window| self.state.windows.get(&window))
            .is_some_and(|window| {
                self.status_formats_for_session(Some(window.session))
                    .position
                    == crate::StatusPosition::Top
            })
    }

    fn customize_draw_key(&self, table: &str, key: &str, writer: &mut PreviewWriter) {
        let Some(binding) = self.keys.get(table, key) else {
            return;
        };
        let note = binding
            .note
            .as_deref()
            .unwrap_or("There is no note for this key.");
        let period = if !note.is_empty() && !note.ends_with('.') {
            "."
        } else {
            ""
        };
        if !writer.text(false, "", &format!("{note}{period}")) || !writer.skip_line() {
            return;
        }
        if !writer.text(false, "", &format!("This key is in the {table} table."))
            || !writer.value("Repeat: ", if binding.repeat { "on" } else { "off" })
            || !writer.skip_line()
        {
            return;
        }
        let command = customize_command_print(binding);
        if !writer.value("Command: ", &command) {
            return;
        }
        if let Some(default) = KeyTables::default().get(table, key) {
            let default_command = customize_command_print(default);
            if default_command != command {
                writer.value("The default is: ", &default_command);
            }
        }
    }

    fn customize_draw_environment(
        &self,
        name: &str,
        session: Option<SessionId>,
        writer: &mut PreviewWriter,
    ) {
        let entries = self.customize_environment_entries(session);
        let Some((_, value, hidden)) = entries.iter().find(|(entry, _, _)| entry == name) else {
            return;
        };
        let scope = if session.is_none() {
            "global"
        } else {
            "session"
        };
        if !writer.text(
            false,
            "",
            &format!("This is a {scope} environment variable."),
        ) {
            return;
        }
        if *hidden && !writer.text(false, "", "This variable is hidden.") {
            return;
        }
        if !writer.skip_line() {
            return;
        }
        let shown = match value {
            None => writer.text(false, "", "Variable is removed."),
            Some(value) => writer.value("Variable value: ", value),
        };
        if !shown || session.is_none() {
            return;
        }
        let global = self.customize_environment_entries(None);
        let Some((_, parent, _)) = global.iter().find(|(entry, _, _)| entry == name) else {
            return;
        };
        match parent {
            None => writer.text(false, "", "Global variable is removed."),
            Some(parent) => writer.value("Global value: ", parent),
        };
    }

    fn customize_draw_option(
        &self,
        pane: PaneId,
        row: &Row,
        writer: &mut PreviewWriter,
        expand: &mut CustomizeExpand<'_>,
    ) {
        let Item::Option {
            name,
            array_key,
            target,
            metadata,
            value,
            hook,
            monitor,
            ..
        } = &row.item
        else {
            return;
        };
        let is_hook = *hook && metadata.is_some();
        let is_user_hook = *hook && metadata.is_none();
        let fire = monitor
            .as_ref()
            .and_then(|_| self.customize_monitor(*target, name))
            .map_or_else(|| self.hook_fire(*target, name), |monitor| monitor.fire);
        let (space, unit) = match metadata.map(|option| option.metadata.unit) {
            Some(unit) if !unit.is_empty() => (" ", unit),
            _ => ("", ""),
        };
        let description = match metadata {
            Some(option) if !option.metadata.description.is_empty() => option.metadata.description,
            _ if monitor.is_some() => "This hook runs when a monitor changes.",
            _ if is_user_hook => "This hook doesn't have a description.",
            _ => "This option doesn't have a description.",
        };
        if !writer.text(false, "", description) || !writer.skip_line() {
            return;
        }
        let scope = metadata.map_or("user", |option| match option.scope {
            TmuxOptionScope::Server => "server",
            TmuxOptionScope::Session => "session",
            TmuxOptionScope::Window => "window",
            TmuxOptionScope::WindowPane => "window and pane",
        });
        let kind_line = if monitor.is_some() {
            "This is a monitor hook.".to_owned()
        } else if is_user_hook {
            "This is a user hook.".to_owned()
        } else if is_hook {
            format!("This is a {scope} hook.")
        } else {
            format!("This is a {scope} option.")
        };
        if !writer.text(false, "", &kind_line) {
            return;
        }
        if let Some(monitor) = monitor
            && !writer.value("Monitor: ", monitor)
        {
            return;
        }
        if metadata.is_some_and(|option| option.is_array) {
            if is_hook {
                if array_key.is_none() {
                    if writer.text(false, "", "This is an array hook.") {
                        customize_draw_hook_fire(fire, writer);
                    }
                    return;
                }
            } else {
                let line = array_key.as_ref().map_or_else(
                    || "This is an array option.".to_owned(),
                    |key| format!("This is an array option, key {key}."),
                );
                if !writer.text(false, "", &line) || array_key.is_none() {
                    return;
                }
            }
        }
        if !writer.skip_line() {
            return;
        }
        let default_value = metadata
            .filter(|_| array_key.is_none())
            .and_then(customize_default_value)
            .filter(|default| default != value);
        if is_hook || is_user_hook {
            if !writer.value("Hook command: ", &format!("{value}{space}{unit}"))
                || !customize_draw_hook_fire(fire, writer)
            {
                return;
            }
        } else if !writer.value("Option value: ", &format!("{value}{space}{unit}")) {
            return;
        }
        let kind = metadata.map(|option| option.metadata.kind);
        let expanded = expand(value, &BTreeMap::new());
        if matches!(kind, None | Some(TmuxOptionKind::String))
            && expanded != *value
            && !writer.value("This expands to: ", &expanded)
        {
            return;
        }
        if let Some(option) = metadata
            && option.metadata.kind == TmuxOptionKind::Choice
            && !writer.value(
                "Available values are: ",
                &option.metadata.choices.join(", "),
            )
        {
            return;
        }
        if matches!(kind, Some(TmuxOptionKind::Colour))
            || CUSTOMIZE_COLOUR_FLAG_OPTIONS.contains(&name.as_str())
        {
            let style = if matches!(kind, Some(TmuxOptionKind::Colour)) {
                format!("fg={value}")
            } else {
                format!("fg={expanded}")
            };
            if !writer.text(true, "", "This is a colour option: ")
                || !writer.text(false, &customize_example_style(&style), "EXAMPLE")
            {
                return;
            }
        }
        if matches!(kind, Some(TmuxOptionKind::Style))
            && (!writer.text(true, "", "This is a style option: ")
                || !writer.text(false, &customize_example_style(&expanded), "EXAMPLE"))
        {
            return;
        }
        if let Some(default) = default_value
            && !writer.value("The default is: ", &format!("{default}{space}{unit}"))
        {
            return;
        }
        if !writer.skip_line_strict() || metadata.is_some_and(|option| option.is_array) {
            return;
        }
        let option = match metadata {
            Some(option) => *option,
            None => return,
        };
        let window = self.state.window_for_pane(pane);
        if let TmuxOptionTarget::Pane(_) = target
            && let Some(window) = window
            && let Ok(Some((parent, _))) =
                self.tmux_option_readback(option, TmuxOptionTarget::Window(window), false)
        {
            let index = self
                .state
                .windows
                .get(&window)
                .map_or(0, |entry| entry.index);
            if !writer.value(
                &format!("Window value (from window {index}): "),
                &format!("{parent}{space}{unit}"),
            ) {
                return;
            }
        }
        let global = match target {
            TmuxOptionTarget::Pane(_) | TmuxOptionTarget::Window(_) => {
                Some(TmuxOptionTarget::GlobalWindow)
            }
            TmuxOptionTarget::Session(_) => Some(TmuxOptionTarget::GlobalSession),
            _ => None,
        };
        if let Some(global) = global
            && let Ok(Some((parent, _))) = self.tmux_option_readback(option, global, false)
        {
            writer.value("Global value: ", &format!("{parent}{space}{unit}"));
        }
    }
}

fn customize_draw_hook_fire(fire: HookFire, writer: &mut PreviewWriter) -> bool {
    if fire.time != 0 {
        let now = i64::try_from(unix_seconds()).unwrap_or(i64::MAX);
        let time = crate::formats::pretty_time(i64::try_from(fire.time).unwrap_or(i64::MAX), now);
        return writer.text(
            false,
            "",
            &format!(
                "This hook has been fired {} times, last {time}.",
                fire.count
            ),
        );
    }
    writer.text(
        false,
        "",
        &format!("This hook has been fired {} times.", fire.count),
    )
}

fn customize_example_style(value: &str) -> String {
    value
        .split([' ', ','])
        .filter(|token| {
            zz_protocol::parse_style(token).is_some_and(|style| {
                style.align.is_none()
                    && style.fill.is_none()
                    && style.list.is_none()
                    && style.range.is_none()
                    && style.width.is_none()
                    && style.pad.is_none()
                    && style.default_type.is_none()
                    && style.ignore.is_none()
                    && style.link.is_none()
            })
        })
        .collect::<Vec<_>>()
        .join(",")
}

impl PreviewWriter {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            lines: Vec::new(),
            x: 0,
            y: 0,
        }
    }

    fn put(&mut self, style: &str, text: &str) {
        if text.is_empty() || self.y >= self.height {
            return;
        }
        while self.lines.len() <= self.y {
            self.lines.push(String::new());
        }
        let line = &mut self.lines[self.y];
        let escaped = text.replace('#', "##");
        if style.is_empty() {
            line.push_str(&escaped);
        } else {
            let _ = write!(line, "#[{style}]{escaped}#[default]");
        }
    }

    fn text(&mut self, more: bool, style: &str, text: &str) -> bool {
        use unicode_width::UnicodeWidthChar as _;
        if self.height == 0 || self.width == 0 {
            return false;
        }
        let characters = text.chars().collect::<Vec<_>>();
        let start_row = self.y;
        let lines = self.height.saturating_sub(start_row);
        let mut index = 0;
        let mut left = self.width.saturating_sub(self.x);
        loop {
            let mut at = 0;
            let mut end = index;
            while end < characters.len() {
                if characters[end] == '\n' {
                    break;
                }
                let width = characters[end].width().unwrap_or(0);
                if at + width > left {
                    break;
                }
                at += width;
                end += 1;
            }
            let next = if end == characters.len() {
                end
            } else if characters[end] == '\n' || characters[end] == ' ' {
                end + 1
            } else {
                match (index + 1..=end)
                    .rev()
                    .find(|position| characters[*position] == ' ')
                {
                    Some(space) => {
                        end = space;
                        space + 1
                    }
                    None => end,
                }
            };
            let piece = characters[index..end].iter().collect::<String>();
            let width = piece
                .chars()
                .map(|character| character.width().unwrap_or(0))
                .sum::<usize>();
            self.put(style, &piece);
            self.x += width;
            index = next;
            if self.y + 1 == start_row + lines || index == characters.len() {
                break;
            }
            self.y += 1;
            self.x = 0;
            left = self.width;
        }
        if (self.y + 1 == start_row + lines && (!more || self.x == self.width))
            || index != characters.len()
        {
            return false;
        }
        if !more || self.x == self.width {
            self.y += 1;
            self.x = 0;
        }
        true
    }

    fn value(&mut self, label: &str, value: &str) -> bool {
        if self.y >= self.height || !self.text(true, "", label) {
            return false;
        }
        if self.y >= self.height {
            return false;
        }
        self.text(false, "fg=themelightgrey", value)
    }

    fn skip_line(&mut self) -> bool {
        self.y += 1;
        self.x = 0;
        self.y + 1 < self.height
    }

    fn skip_line_strict(&mut self) -> bool {
        self.y += 1;
        self.x = 0;
        self.y < self.height
    }

    fn finish(self) -> Vec<String> {
        self.lines
    }
}
