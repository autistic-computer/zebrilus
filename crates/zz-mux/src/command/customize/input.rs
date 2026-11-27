use super::*;

impl MuxEngine {
    pub fn customize_key(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        name: &str,
        history: PromptHistories<'_>,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let key = ModeKey::parse(name);
        let Some(lines) = self.customize_ready(pane, mode, expand) else {
            return CustomizeResult {
                close: true,
                commands: Vec::new(),
                menu: None,
                edit: None,
                stop_on_error: false,
                message: None,
                remembered: None,
            };
        };
        if let Some((prompt, _)) = &mut mode.prompt {
            let outcome = prompt.key_with_history(key, history.of(prompt.history_type()));
            let mut remembered = None;
            let (value, purpose) = match outcome {
                PromptOutcome::Done => {
                    let value = prompt.input();
                    remembered = prompt.remembered();
                    (Some(value), mode.prompt.take().map(|(_, purpose)| purpose))
                }
                PromptOutcome::Cancelled => (None, mode.prompt.take().map(|(_, purpose)| purpose)),
                PromptOutcome::Closed => {
                    mode.prompt = None;
                    return CustomizeResult::stay();
                }
                _ => return CustomizeResult::stay(),
            };
            let Some(purpose) = purpose else {
                return CustomizeResult::stay();
            };
            let mut result = self.customize_answer(pane, mode, purpose, value, expand);
            result.remembered = remembered;
            return result;
        }
        if mode.help {
            mode.help = false;
            return CustomizeResult::stay();
        }
        self.customize_tree_key(pane, mode, key, lines, expand)
    }

    fn customize_ready(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        expand: &mut CustomizeExpand<'_>,
    ) -> Option<usize> {
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        if lines.is_empty() {
            return None;
        }
        mode.current = mode.current.min(lines.len() - 1);
        Some(lines.len())
    }

    pub fn customize_mouse(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        name: &str,
        x: usize,
        y: usize,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let Some(size) = self.customize_ready(pane, mode, expand) else {
            return CustomizeResult {
                close: true,
                commands: Vec::new(),
                menu: None,
                edit: None,
                stop_on_error: false,
                message: None,
                remembered: None,
            };
        };
        let columns = self.customize_screen_columns(pane);
        let screen_rows = self.customize_screen_rows(pane);
        let prompt_top = self.customize_prompt_top(pane);
        if let Some((prompt, _)) = &mut mode.prompt {
            let row = if prompt_top {
                0
            } else {
                screen_rows.saturating_sub(1)
            };
            let handled = ModeMouseKey::is_press1(name)
                && y == row
                && prompt.mouse(x, columns) != PromptOutcome::NotHandled;
            if handled {
                return CustomizeResult::stay();
            }
        }
        if mode.help {
            return CustomizeResult::stay();
        }
        let button = ModeMouseKey::parse(name);
        if x > columns || y > mode.height {
            if button != ModeMouseKey::Down3 {
                return CustomizeResult::stay();
            }
            let line = if mode.offset + y < size {
                mode.offset + y
            } else {
                mode.current
            };
            return self.customize_menu_result(pane, mode, line, true, expand);
        }
        if mode.offset + y >= size {
            if button != ModeMouseKey::Down3 {
                return CustomizeResult::stay();
            }
            return self.customize_menu_result(pane, mode, mode.current, false, expand);
        }
        if matches!(
            button,
            ModeMouseKey::Down1 | ModeMouseKey::Down3 | ModeMouseKey::DoubleClick1
        ) {
            mode.current = mode.offset + y;
        }
        if button == ModeMouseKey::DoubleClick1 {
            return self.customize_tree_key(pane, mode, ModeKey::Char('\r'), size, expand);
        }
        if button != ModeMouseKey::Down3 {
            return CustomizeResult::stay();
        }
        self.customize_menu_result(pane, mode, mode.current, false, expand)
    }

    pub fn customize_menu_choice(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        line: usize,
        name: &str,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let Some(size) = self.customize_ready(pane, mode, expand) else {
            return CustomizeResult {
                close: true,
                commands: Vec::new(),
                menu: None,
                edit: None,
                stop_on_error: false,
                message: None,
                remembered: None,
            };
        };
        if line >= size {
            return CustomizeResult::stay();
        }
        mode.current = line;
        self.customize_key(pane, mode, name, PromptHistories::default(), expand)
    }

    fn customize_menu_result(
        &self,
        pane: PaneId,
        mode: &CustomizeMode,
        line: usize,
        outside: bool,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        let name = lines
            .get(line)
            .and_then(|index| rows.get(*index))
            .map_or_else(String::new, |row| row.name.clone());
        CustomizeResult {
            close: false,
            commands: Vec::new(),
            menu: Some(CustomizeMenu {
                line,
                outside,
                name,
            }),
            edit: None,
            stop_on_error: false,
            message: None,
            remembered: None,
        }
    }

    fn customize_tree_key(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        key: ModeKey,
        size: usize,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        if lines.is_empty() {
            return CustomizeResult {
                close: true,
                commands: Vec::new(),
                menu: None,
                edit: None,
                stop_on_error: false,
                message: None,
                remembered: None,
            };
        }
        let mut key = key;
        if let Some(choice) = (0..size.min(36)).find(|index| {
            key == if *index < 10 {
                ModeKey::Char(char::from(b'0' + *index as u8))
            } else {
                ModeKey::Meta(char::from(b'a' + (*index - 10) as u8))
            }
        }) {
            mode.current = choice;
            key = ModeKey::Char('\r');
        }
        let row_at = |mode: &CustomizeMode| &rows[lines[mode.current]];
        match key {
            ModeKey::Char('q' | '\u{1b}') | ModeKey::Ctrl('[' | 'g') => {
                return CustomizeResult {
                    close: true,
                    commands: Vec::new(),
                    menu: None,
                    edit: None,
                    stop_on_error: false,
                    message: None,
                    remembered: None,
                };
            }
            ModeKey::Char('e') => {
                if let Some(edit) = self.customize_start_edit(row_at(mode)) {
                    return CustomizeResult {
                        edit: Some(edit),
                        ..CustomizeResult::stay()
                    };
                }
            }
            ModeKey::F1 | ModeKey::Ctrl('h') => mode.help = true,
            ModeKey::Up | ModeKey::Char('k') | ModeKey::Ctrl('p') => mode.up(size, true),
            ModeKey::Down | ModeKey::Char('j') | ModeKey::Ctrl('n') => {
                mode.down(size, true);
            }
            ModeKey::PageUp | ModeKey::Ctrl('b') => {
                for _ in 0..mode.height {
                    if mode.current == 0 {
                        break;
                    }
                    mode.up(size, true);
                }
            }
            ModeKey::PageDown | ModeKey::Ctrl('f') => {
                for _ in 0..mode.height {
                    if mode.current == size - 1 {
                        break;
                    }
                    mode.down(size, true);
                }
            }
            ModeKey::Char('g') | ModeKey::Home => {
                mode.current = 0;
                mode.offset = 0;
            }
            ModeKey::Char('G') | ModeKey::End => {
                mode.current = size - 1;
                mode.offset_for_current();
            }
            ModeKey::Char('t') => {
                let row = row_at(mode);
                if !row.no_tag {
                    if mode.tagged.contains(&row.id) {
                        mode.tagged.remove(&row.id);
                    } else {
                        let mut parent = row.parent;
                        while let Some(index) = parent {
                            mode.tagged.remove(&rows[index].id);
                            parent = rows[index].parent;
                        }
                        let id = row.id.clone();
                        mode.tagged
                            .retain(|tagged| !customize_is_descendant(&rows, &id, tagged));
                        mode.tagged.insert(id);
                    }
                }
            }
            ModeKey::Char('T') => {
                for line in &lines {
                    mode.tagged.remove(&rows[*line].id);
                }
            }
            ModeKey::Ctrl('t') => {
                for line in &lines {
                    let row = &rows[*line];
                    let tag = match row.parent {
                        None => !row.no_tag,
                        Some(parent) => rows[parent].no_tag,
                    };
                    if tag {
                        mode.tagged.insert(row.id.clone());
                    } else {
                        mode.tagged.remove(&row.id);
                    }
                }
            }
            ModeKey::Char('O' | 'r') => {
                let tag = row_at(mode).id.clone();
                self.customize_build(pane, mode, Some(tag), expand);
            }
            ModeKey::Left | ModeKey::Char('h' | '-') => {
                let line = lines[mode.current];
                let flat = customize_flat(&rows, line);
                let mut target = Some(line);
                if flat || !mode.expanded.contains(&rows[line].id) {
                    target = rows[line].parent;
                }
                match target {
                    None => mode.up(size, false),
                    Some(target) => {
                        mode.expanded.remove(&rows[target].id);
                        if let Some(position) = lines.iter().position(|line| *line == target) {
                            mode.current = position;
                        }
                        let tag = row_at(mode).id.clone();
                        self.customize_build(pane, mode, Some(tag), expand);
                    }
                }
            }
            ModeKey::Right | ModeKey::Char('l' | '+') => {
                let line = lines[mode.current];
                if customize_flat(&rows, line) || mode.expanded.contains(&rows[line].id) {
                    mode.down(size, false);
                } else {
                    mode.expanded.insert(rows[line].id.clone());
                    let tag = rows[line].id.clone();
                    self.customize_build(pane, mode, Some(tag), expand);
                }
            }
            ModeKey::Meta('-') => {
                let tag = row_at(mode).id.clone();
                for row in rows.iter().filter(|row| row.parent.is_none()) {
                    mode.expanded.remove(&row.id);
                }
                self.customize_build(pane, mode, Some(tag), expand);
            }
            ModeKey::Meta('+') => {
                let tag = row_at(mode).id.clone();
                for row in rows.iter().filter(|row| row.parent.is_none()) {
                    mode.expanded.insert(row.id.clone());
                }
                self.customize_build(pane, mode, Some(tag), expand);
            }
            ModeKey::Char('?' | '/') | ModeKey::Ctrl('s') => {
                mode.prompt = Some((
                    self.customize_prompt_options(pane).search("(search) ", ""),
                    PromptPurpose::Search,
                ));
            }
            ModeKey::Char(direction @ ('n' | 'N')) => {
                self.customize_search_set(pane, mode, direction == 'N', expand);
            }
            ModeKey::Char('f') => {
                let input = mode.filter.clone().unwrap_or_default();
                mode.prompt = Some((
                    self.customize_prompt_options(pane)
                        .search("(filter) ", &input),
                    PromptPurpose::Filter,
                ));
            }
            ModeKey::Char('c') => {
                mode.prompt = None;
                mode.filter = None;
                let tag = row_at(mode).id.clone();
                self.customize_build(pane, mode, Some(tag), expand);
            }
            ModeKey::Char('v') => {
                mode.preview = match mode.preview {
                    Preview::Off => Preview::Big,
                    Preview::Normal => Preview::Off,
                    Preview::Big => Preview::Normal,
                };
                let tag = row_at(mode).id.clone();
                self.customize_build(pane, mode, Some(tag), expand);
            }
            _ => {}
        }
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        let Some(row) = lines.get(mode.current).map(|line| rows[*line].clone()) else {
            return CustomizeResult::stay();
        };
        let tag = Some(row.id.clone());
        let separators = self.customize_prompt_options(pane);
        match key {
            ModeKey::Char('a') => {
                if let Item::Option {
                    name,
                    array_key: Some(array_key),
                    target,
                    ..
                } = &row.item
                {
                    mode.prompt = Some((
                        separators.prompt(format!("({name}[{array_key}]) "), array_key),
                        PromptPurpose::ArrayKey {
                            name: name.clone(),
                            array_key: array_key.clone(),
                            target: *target,
                        },
                    ));
                }
            }
            ModeKey::Char('\r' | 's' | 'w' | 'S' | 'W') => {
                let global = matches!(key, ModeKey::Char('S' | 'W'));
                let pane_scope = matches!(key, ModeKey::Char('\r' | 's'));
                let mut commands = Vec::new();
                match &row.item {
                    Item::Section => {
                        if matches!(key, ModeKey::Char('\r' | 's')) {
                            self.customize_add_current(pane, mode, &row, &separators);
                            return CustomizeResult::stay();
                        }
                    }
                    Item::Environment { name, session, .. } => {
                        if matches!(key, ModeKey::Char('w')) {
                            return CustomizeResult::stay();
                        }
                        self.customize_set_environment(mode, name, *session, global, &separators);
                    }
                    Item::Key {
                        table,
                        key: binding,
                    }
                    | Item::KeyField {
                        table,
                        key: binding,
                    } => {
                        if !matches!(key, ModeKey::Char('\r' | 's')) {
                            return CustomizeResult::stay();
                        }
                        if let Some(command) =
                            self.customize_set_key(mode, &row, table, binding, &separators)
                        {
                            commands.push(command);
                        }
                    }
                    Item::Option { .. } => {
                        if let Some(command) = self.customize_set_option(
                            pane,
                            mode,
                            &row,
                            global,
                            pane_scope,
                            &separators,
                        ) {
                            commands.push(command);
                        }
                    }
                }
                return self.customize_commands(pane, mode, commands, tag, expand);
            }
            ModeKey::Char('d') => {
                if matches!(row.item, Item::Section | Item::Environment { .. })
                    || matches!(
                        row.item,
                        Item::Option {
                            array_key: Some(_),
                            ..
                        }
                    )
                {
                    return CustomizeResult::stay();
                }
                let name = customize_item_name(&row);
                return self.customize_confirm(
                    pane,
                    mode,
                    format!("Reset {name} to default? "),
                    PromptPurpose::Reset,
                    expand,
                );
            }
            ModeKey::Char('u') => {
                if matches!(row.item, Item::Section) {
                    return CustomizeResult::stay();
                }
                let label = match &row.item {
                    Item::Option {
                        name,
                        array_key: Some(array_key),
                        ..
                    } => format!("Unset {name}[{array_key}]? "),
                    _ => format!("Unset {}? ", customize_item_name(&row)),
                };
                return self.customize_confirm(pane, mode, label, PromptPurpose::Unset, expand);
            }
            ModeKey::Char(change @ ('D' | 'U')) => {
                let count = lines
                    .iter()
                    .filter(|line| mode.tagged.contains(&rows[**line].id))
                    .count();
                if count == 0 {
                    return CustomizeResult::stay();
                }
                let (label, purpose) = if change == 'D' {
                    (
                        format!("Reset {count} tagged to default? "),
                        PromptPurpose::ResetTagged,
                    )
                } else {
                    (
                        format!("Unset {count} tagged? "),
                        PromptPurpose::UnsetTagged,
                    )
                };
                return self.customize_confirm(pane, mode, label, purpose, expand);
            }
            ModeKey::Char('H') => {
                mode.hide_global = !mode.hide_global;
                self.customize_build(pane, mode, tag, expand);
            }
            ModeKey::Char('C') => {
                mode.changed_only = !mode.changed_only;
                self.customize_build(pane, mode, tag, expand);
            }
            _ => {}
        }
        CustomizeResult::stay()
    }
}

fn customize_is_descendant(rows: &[Row], ancestor: &str, id: &str) -> bool {
    let Some(mut row) = rows.iter().find(|row| row.id == id) else {
        return false;
    };
    while let Some(parent) = row.parent {
        if rows[parent].id == ancestor {
            return true;
        }
        row = &rows[parent];
    }
    false
}

fn customize_flat(rows: &[Row], line: usize) -> bool {
    let parent = rows[line].parent;
    let depth = rows[line].depth;
    !rows
        .iter()
        .any(|row| row.parent == parent && row.depth == depth && row.children)
}

fn customize_item_name(row: &Row) -> String {
    match &row.item {
        Item::Option { name, .. } | Item::Environment { name, .. } => name.clone(),
        Item::Key { key, .. } | Item::KeyField { key, .. } => key.clone(),
        Item::Section => row.name.clone(),
    }
}
