use super::*;

impl MuxEngine {
    pub(super) fn customize_prompt_options(&self, pane: PaneId) -> ModePromptOptions {
        let session = self
            .state
            .window_for_pane(pane)
            .map(|window| self.state.windows[&window].session);
        let (vi, separators) = self.prompt_key_options(session);
        ModePromptOptions { vi, separators }
    }

    pub(super) fn customize_confirm(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        label: String,
        purpose: PromptPurpose,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        if mode.accept {
            return self.customize_answer(pane, mode, purpose, Some("y".to_owned()), expand);
        }
        mode.prompt = Some((self.customize_prompt_options(pane).single(label), purpose));
        CustomizeResult::stay()
    }

    pub(super) fn customize_commands(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        commands: Vec<CommandInvocation>,
        tag: Option<String>,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        if commands.is_empty() {
            self.customize_build(pane, mode, tag, expand);
        } else {
            mode.rebuild = true;
            mode.rebuild_tag = tag;
        }
        CustomizeResult {
            close: false,
            commands,
            menu: None,
            edit: None,
            stop_on_error: false,
            message: None,
            remembered: None,
        }
    }

    pub(super) fn customize_start_edit(&self, row: &Row) -> Option<CustomizeEdit> {
        match &row.item {
            Item::Option {
                name,
                array_key,
                target,
                metadata,
                value,
                ..
            } => {
                if metadata.is_some_and(|option| {
                    matches!(
                        option.metadata.kind,
                        TmuxOptionKind::Flag | TmuxOptionKind::Choice
                    )
                }) {
                    return None;
                }
                Some(CustomizeEdit {
                    value: value.clone(),
                    kind: EditKind::Option {
                        name: name.clone(),
                        array_key: array_key.clone(),
                        target: *target,
                    },
                })
            }
            Item::KeyField { table, key } => {
                let binding = self.keys.get(table, key)?;
                let (value, kind) = match row.name.as_str() {
                    "Command" => (
                        customize_command_print(binding),
                        EditKind::KeyCommand {
                            table: table.clone(),
                            key: key.clone(),
                        },
                    ),
                    "Note" => (
                        binding.note.clone().unwrap_or_default(),
                        EditKind::KeyNote {
                            table: table.clone(),
                            key: key.clone(),
                        },
                    ),
                    _ => return None,
                };
                Some(CustomizeEdit { value, kind })
            }
            Item::Environment {
                name,
                session,
                value: Some(value),
                ..
            } => Some(CustomizeEdit {
                value: value.clone(),
                kind: EditKind::Environment {
                    name: name.clone(),
                    session: *session,
                },
            }),
            _ => None,
        }
    }

    pub fn customize_edited(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        edit: &CustomizeEdit,
        value: &str,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        let tag = lines.get(mode.current).map(|line| rows[*line].id.clone());
        let command = match &edit.kind {
            EditKind::Option {
                name,
                array_key,
                target,
            } => self.customize_option_set_command(name, array_key.as_deref(), *target, value),
            EditKind::KeyCommand { table, key } => self.keys.get(table, key).map(|binding| {
                customize_bind(table, key, binding.repeat, binding.note.as_deref(), value)
            }),
            EditKind::KeyNote { table, key } => self.keys.get(table, key).map(|binding| {
                customize_bind(
                    table,
                    key,
                    binding.repeat,
                    (!value.is_empty()).then_some(value),
                    &customize_command_print(binding),
                )
            }),
            EditKind::Environment { name, session } => {
                Some(self.customize_environment_set(name, *session, value, false))
            }
        };
        self.customize_commands(pane, mode, command.into_iter().collect(), tag, expand)
    }

    fn customize_option_set_command(
        &self,
        name: &str,
        array_key: Option<&str>,
        target: TmuxOptionTarget,
        value: &str,
    ) -> Option<CommandInvocation> {
        let array = tmux_options().any(|option| option.name == name && option.is_array);
        if !array {
            return Some(customize_set_command(name, target, value));
        }
        let key = match array_key {
            Some(key) => key.to_owned(),
            None if tmux_option_is_hook(name) => {
                let used = self
                    .hook_array(target, name)
                    .map(|hook| hook.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                first_free_array_index(used.iter()).ok()?.to_string()
            }
            None => {
                let values = self.customize_array_values(target, name);
                first_free_array_index(values.keys()).ok()?.to_string()
            }
        };
        Some(customize_set_command(
            &format!("{name}[{key}]"),
            target,
            value,
        ))
    }

    fn customize_environment_set(
        &self,
        name: &str,
        session: Option<SessionId>,
        value: &str,
        hidden: bool,
    ) -> CommandInvocation {
        let hidden = self
            .customize_environment_entries(session)
            .iter()
            .find(|(entry, _, _)| entry == name)
            .map_or(hidden, |(_, _, hidden)| *hidden);
        let mut args = Vec::new();
        if hidden {
            args.push("-h".to_owned());
        }
        args.extend(customize_environment_target(session));
        args.extend([name.to_owned(), value.to_owned()]);
        CommandInvocation::new("set-environment", args)
    }

    pub(super) fn customize_set_key(
        &self,
        mode: &mut CustomizeMode,
        row: &Row,
        table: &str,
        key: &str,
        separators: &ModePromptOptions,
    ) -> Option<CommandInvocation> {
        let binding = self.keys.get(table, key)?;
        match row.name.as_str() {
            "Repeat" if matches!(row.item, Item::KeyField { .. }) => Some(customize_bind(
                table,
                key,
                !binding.repeat,
                binding.note.as_deref(),
                &customize_command_print(binding),
            )),
            "Command" if matches!(row.item, Item::KeyField { .. }) => {
                mode.prompt = Some((
                    separators.prompt(format!("({key}) "), &customize_command_print(binding)),
                    PromptPurpose::Command {
                        table: table.to_owned(),
                        key: key.to_owned(),
                    },
                ));
                None
            }
            "Note" if matches!(row.item, Item::KeyField { .. }) => {
                mode.prompt = Some((
                    separators.prompt(
                        format!("({key}) "),
                        binding.note.as_deref().unwrap_or_default(),
                    ),
                    PromptPurpose::Note {
                        table: table.to_owned(),
                        key: key.to_owned(),
                    },
                ));
                None
            }
            _ => None,
        }
    }

    pub(super) fn customize_add_current(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        row: &Row,
        separators: &ModePromptOptions,
    ) {
        let Some(window) = self.state.window_for_pane(pane) else {
            return;
        };
        let session = self.state.windows[&window].session;
        let (label, input, purpose) = match row.name.as_str() {
            "Server Options" => (
                "New user option: ".to_owned(),
                "@",
                PromptPurpose::AddOption {
                    target: TmuxOptionTarget::Server,
                    hook: false,
                },
            ),
            "Session Options" | "Session Hooks" => {
                let hook = row.name == "Session Hooks";
                (
                    format!("New user {}: ", if hook { "hook" } else { "option" }),
                    "@",
                    PromptPurpose::AddOption {
                        target: TmuxOptionTarget::Session(session),
                        hook,
                    },
                )
            }
            "Window & Pane Options" | "Window & Pane Hooks" => {
                let hook = row.name == "Window & Pane Hooks";
                (
                    format!("New user {}: ", if hook { "hook" } else { "option" }),
                    "@",
                    PromptPurpose::AddOption {
                        target: TmuxOptionTarget::Pane(pane),
                        hook,
                    },
                )
            }
            "Global Environment" => (
                "New environment: ".to_owned(),
                "",
                PromptPurpose::AddEnvironment { session: None },
            ),
            "Session Environment" => (
                "New environment: ".to_owned(),
                "",
                PromptPurpose::AddEnvironment {
                    session: Some(session),
                },
            ),
            name => {
                let Some(table) = name.strip_prefix("Key Table - ") else {
                    return;
                };
                (
                    format!("New key in {table}: "),
                    "",
                    PromptPurpose::AddKey {
                        table: table.to_owned(),
                    },
                )
            }
        };
        mode.prompt = Some((separators.prompt(label, input), purpose));
    }

    pub(super) fn customize_set_environment(
        &self,
        mode: &mut CustomizeMode,
        name: &str,
        session: Option<SessionId>,
        global: bool,
        separators: &ModePromptOptions,
    ) {
        let entries = self.customize_environment_entries(session);
        let Some((_, value, hidden)) = entries.iter().find(|(entry, _, _)| entry == name) else {
            return;
        };
        let session = if global { None } else { session };
        let label = match session {
            Some(session) => format!(
                "({name}, for {}) ",
                self.customize_scope(TmuxOptionTarget::Session(session))
            ),
            None => format!("({name}, global) "),
        };
        mode.prompt = Some((
            separators.prompt(label, value.as_deref().unwrap_or_default()),
            PromptPurpose::Environment {
                name: name.to_owned(),
                session,
                hidden: *hidden,
            },
        ));
    }

    pub(super) fn customize_set_option(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        row: &Row,
        global: bool,
        pane_scope: bool,
        separators: &ModePromptOptions,
    ) -> Option<CommandInvocation> {
        let Item::Option {
            name,
            array_key,
            target: owner,
            metadata,
            ..
        } = &row.item
        else {
            return None;
        };
        let window = self.state.window_for_pane(pane)?;
        let session = self.state.windows.get(&window)?.session;
        let pane_scope =
            pane_scope && metadata.is_none_or(|option| option.scope == TmuxOptionScope::WindowPane);
        let array = metadata.is_some_and(|option| option.is_array);
        let local_window = if pane_scope {
            TmuxOptionTarget::Pane(pane)
        } else {
            TmuxOptionTarget::Window(window)
        };
        let target = if array {
            *owner
        } else if global {
            match owner {
                TmuxOptionTarget::Session(_) => TmuxOptionTarget::GlobalSession,
                TmuxOptionTarget::Window(_) | TmuxOptionTarget::Pane(_) => {
                    TmuxOptionTarget::GlobalWindow
                }
                other => *other,
            }
        } else {
            match owner {
                TmuxOptionTarget::GlobalSession => TmuxOptionTarget::Session(session),
                TmuxOptionTarget::Window(_)
                | TmuxOptionTarget::Pane(_)
                | TmuxOptionTarget::GlobalWindow => local_window,
                other => *other,
            }
        };
        if let Some(option) = metadata {
            let current = || {
                self.tmux_option_readback(*option, target, true)
                    .ok()
                    .flatten()
                    .map(|(value, _)| value)
                    .unwrap_or_default()
            };
            match option.metadata.kind {
                TmuxOptionKind::Flag => {
                    return Some(customize_set_command(
                        name,
                        target,
                        if current() == "on" { "off" } else { "on" },
                    ));
                }
                TmuxOptionKind::Choice if !option.metadata.choices.is_empty() => {
                    let choices = option.metadata.choices;
                    let value = current();
                    let index = choices.iter().position(|choice| *choice == value);
                    let next = index.map_or(0, |index| (index + 1) % choices.len());
                    return Some(customize_set_command(name, target, choices[next]));
                }
                _ => {}
            }
        }
        let scope = self.customize_scope(target);
        let space = if !scope.is_empty() {
            ", for "
        } else if target == TmuxOptionTarget::Server {
            ""
        } else {
            ", global"
        };
        let label = match (array, array_key) {
            (true, None) => format!("({name}[+]{space}{scope}) "),
            (true, Some(key)) => format!("({name}[{key}]{space}{scope}) "),
            (false, _) => format!("({name}{space}{scope}) "),
        };
        let value = match &row.item {
            Item::Option { value, .. } => value.clone(),
            _ => String::new(),
        };
        mode.prompt = Some((
            separators.prompt(label, &value),
            PromptPurpose::Option {
                name: name.clone(),
                array_key: array_key.clone(),
                target,
            },
        ));
        None
    }

    pub(super) fn customize_answer(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        purpose: PromptPurpose,
        value: Option<String>,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        let current = lines.get(mode.current).map(|line| &rows[*line]);
        let tag = current.map(|row| row.id.clone());
        match purpose {
            PromptPurpose::Search => {
                mode.search = value.filter(|value| !value.is_empty());
                if mode.search.is_some() {
                    self.customize_search_set(pane, mode, false, expand);
                }
                CustomizeResult::stay()
            }
            PromptPurpose::Filter => {
                mode.filter = value.filter(|value| !value.is_empty());
                self.customize_build(pane, mode, tag, expand);
                CustomizeResult::stay()
            }
            PromptPurpose::Option {
                name,
                array_key,
                target,
            } => {
                let Some(value) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                let Some(command) =
                    self.customize_option_set_command(&name, array_key.as_deref(), target, &value)
                else {
                    return CustomizeResult::stay();
                };
                self.customize_commands(pane, mode, vec![command], tag, expand)
            }
            PromptPurpose::ArrayKey {
                name,
                array_key,
                target,
            } => {
                let Some(new_key) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                let Some(new_key) = customize_array_key(&new_key) else {
                    return CustomizeResult::message(format!("Bad array key: {new_key}"));
                };
                let values = self.customize_array_entries(target, &name);
                if values.iter().any(|(key, _)| *key == new_key) {
                    return CustomizeResult::stay();
                }
                let Some(entry) = values
                    .iter()
                    .find(|(key, _)| *key == array_key)
                    .map(|(_, entry)| entry.clone())
                else {
                    return CustomizeResult::stay();
                };
                let mut result = self.customize_commands(
                    pane,
                    mode,
                    vec![
                        customize_set_command(&format!("{name}[{new_key}]"), target, &entry),
                        customize_unset_command(&format!("{name}[{array_key}]"), target),
                    ],
                    tag,
                    expand,
                );
                result.stop_on_error = true;
                result
            }
            PromptPurpose::Environment {
                name,
                session,
                hidden,
            } => {
                let Some(value) = value else {
                    return CustomizeResult::stay();
                };
                let command = self.customize_environment_set(&name, session, &value, hidden);
                self.customize_commands(pane, mode, vec![command], tag, expand)
            }
            PromptPurpose::AddOption { target, hook } => {
                let Some(value) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                match customize_add_option(&value, target, hook) {
                    Ok(command) => self.customize_commands(pane, mode, vec![command], tag, expand),
                    Err(message) => CustomizeResult::message(message),
                }
            }
            PromptPurpose::AddEnvironment { session } => {
                let Some(value) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                match customize_add_environment(&value, session) {
                    Ok(command) => self.customize_commands(pane, mode, vec![command], tag, expand),
                    Err(message) => CustomizeResult::message(message),
                }
            }
            PromptPurpose::AddKey { table } => {
                let Some(value) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                match customize_add_key(&value, &table) {
                    Ok(command) => self.customize_commands(pane, mode, vec![command], tag, expand),
                    Err(message) => CustomizeResult::message(message),
                }
            }
            PromptPurpose::Command { table, key } => {
                let Some(command) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                let Some(binding) = self.keys.get(&table, &key) else {
                    return CustomizeResult::stay();
                };
                let bind = customize_bind(
                    &table,
                    &key,
                    binding.repeat,
                    binding.note.as_deref(),
                    &command,
                );
                self.customize_commands(pane, mode, vec![bind], tag, expand)
            }
            PromptPurpose::Note { table, key } => {
                let Some(note) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                let Some(binding) = self.keys.get(&table, &key) else {
                    return CustomizeResult::stay();
                };
                let bind = customize_bind(
                    &table,
                    &key,
                    binding.repeat,
                    Some(&note),
                    &customize_command_print(binding),
                );
                self.customize_commands(pane, mode, vec![bind], tag, expand)
            }
            PromptPurpose::Reset
            | PromptPurpose::Unset
            | PromptPurpose::ResetTagged
            | PromptPurpose::UnsetTagged => {
                if !value.is_some_and(|value| value.eq_ignore_ascii_case("y")) {
                    return CustomizeResult::stay();
                }
                let reset = matches!(purpose, PromptPurpose::Reset | PromptPurpose::ResetTagged);
                let targets = if matches!(purpose, PromptPurpose::Reset | PromptPurpose::Unset) {
                    current.map(|row| vec![row.clone()]).unwrap_or_default()
                } else {
                    lines
                        .iter()
                        .map(|line| &rows[*line])
                        .filter(|row| mode.tagged.contains(&row.id))
                        .cloned()
                        .collect()
                };
                let current_id = current.map(|row| row.id.clone());
                let mut commands = Vec::new();
                for row in targets {
                    let is_current = current_id.as_ref() == Some(&row.id)
                        || current.is_some_and(|current| customize_same_key(current, &row));
                    commands.extend(
                        self.customize_change(mode, &rows, &lines, &row, reset, is_current),
                    );
                }
                let tag = lines.get(mode.current).map(|line| rows[*line].id.clone());
                self.customize_commands(pane, mode, commands, tag, expand)
            }
        }
    }

    fn customize_change(
        &self,
        mode: &mut CustomizeMode,
        rows: &[Row],
        lines: &[usize],
        row: &Row,
        reset: bool,
        is_current: bool,
    ) -> Vec<CommandInvocation> {
        match &row.item {
            Item::Section => Vec::new(),
            Item::Environment { name, session, .. } => {
                if reset {
                    return Vec::new();
                }
                if is_current {
                    mode.up(lines.len(), false);
                }
                let mut args = vec!["-u".to_owned()];
                args.extend(customize_environment_target(*session));
                args.push(name.clone());
                vec![CommandInvocation::new("set-environment", args)]
            }
            Item::Option {
                name,
                array_key,
                target,
                ..
            } => {
                if reset && array_key.is_some() {
                    return Vec::new();
                }
                if !reset && array_key.is_some() && is_current {
                    mode.up(lines.len(), false);
                }
                if let Some(key) = array_key {
                    return vec![customize_unset_command(&format!("{name}[{key}]"), *target)];
                }
                if !reset {
                    return self.customize_remove_option(name, *target);
                }
                self.customize_option_chain(*target)
                    .into_iter()
                    .filter(|target| self.customize_holds(name, *target))
                    .flat_map(|target| self.customize_remove_option(name, target))
                    .collect()
            }
            Item::Key { table, key } | Item::KeyField { table, key } => {
                let Some(binding) = self.keys.get(table, key) else {
                    return Vec::new();
                };
                let default = KeyTables::default().get(table, key).cloned();
                if reset && let Some(default) = &default {
                    if default.commands == binding.commands {
                        return Vec::new();
                    }
                    return vec![customize_bind(
                        table,
                        key,
                        default.repeat,
                        default.note.as_deref(),
                        &customize_command_print(default),
                    )];
                }
                if is_current {
                    if let Some(line) = lines.get(mode.current) {
                        mode.expanded.remove(&rows[*line].id);
                    }
                    mode.up(lines.len(), false);
                }
                vec![CommandInvocation::new(
                    "unbind-key",
                    ["-T".to_owned(), table.clone(), key.clone()],
                )]
            }
        }
    }
}

fn customize_same_key(left: &Row, right: &Row) -> bool {
    match (&left.item, &right.item) {
        (
            Item::Key { table, key } | Item::KeyField { table, key },
            Item::Key {
                table: other_table,
                key: other_key,
            }
            | Item::KeyField {
                table: other_table,
                key: other_key,
            },
        ) => table == other_table && key == other_key,
        _ => false,
    }
}

fn customize_array_key(key: &str) -> Option<String> {
    if key.contains(']') {
        return None;
    }
    if !key.bytes().all(|byte| byte.is_ascii_digit()) {
        return Some(key.to_owned());
    }
    key.parse::<u32>().ok().map(|index| index.to_string())
}

fn customize_split_pair(value: &str) -> Option<(&str, &str)> {
    let end = value.find([' ', '\t'])?;
    let rest = value[end..].trim_start_matches([' ', '\t']);
    (end != 0 && !rest.is_empty()).then(|| (&value[..end], rest))
}

fn customize_add_option(
    value: &str,
    target: TmuxOptionTarget,
    hook: bool,
) -> Result<CommandInvocation, String> {
    let Some((name, value)) = customize_split_pair(value) else {
        return Err("User option must be @name value".to_owned());
    };
    if !name.starts_with('@') || name.contains('[') {
        return Err(format!(
            "User {} name must start with @",
            if hook { "hook" } else { "option" }
        ));
    }
    if !hook {
        return Ok(customize_set_command(name, target, value));
    }
    let mut args = customize_target_args(target);
    args.extend([name.to_owned(), value.to_owned()]);
    Ok(CommandInvocation::new("set-hook", args))
}

fn customize_add_environment(
    value: &str,
    session: Option<SessionId>,
) -> Result<CommandInvocation, String> {
    let mut args = customize_environment_target(session);
    if let Some(name) = value.strip_prefix('-') {
        if name.is_empty() || name.contains('=') {
            return Err(format!("Bad environment variable: {value}"));
        }
        args.extend(["-r".to_owned(), name.to_owned()]);
    } else {
        let Some((name, value)) = value.split_once('=').filter(|(name, _)| !name.is_empty()) else {
            return Err("Environment variable must be NAME=value".to_owned());
        };
        args.extend([name.to_owned(), value.to_owned()]);
    }
    Ok(CommandInvocation::new("set-environment", args))
}

fn customize_add_key(value: &str, table: &str) -> Result<CommandInvocation, String> {
    let Some((key, command)) = customize_split_pair(value) else {
        return Err("Key binding must be key command".to_owned());
    };
    if parse_tmux_key_details(key).is_none() {
        return Err(format!("Unknown key: {key}"));
    }
    Ok(CommandInvocation::new(
        "bind-key",
        [
            "-T".to_owned(),
            table.to_owned(),
            key.to_owned(),
            command.to_owned(),
        ],
    ))
}
