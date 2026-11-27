use super::*;

impl MuxEngine {
    pub(super) fn customize_rows(
        &self,
        pane: PaneId,
        mode: &CustomizeMode,
        expand: &mut CustomizeExpand<'_>,
    ) -> Vec<Row> {
        let Some(window) = self
            .state
            .window_for_pane(pane)
            .and_then(|id| self.state.windows.get(&id))
        else {
            return Vec::new();
        };
        let format = mode.format.clone();
        let mut rows = Vec::new();
        let session_targets = vec![
            TmuxOptionTarget::Session(window.session),
            TmuxOptionTarget::GlobalSession,
        ];
        let window_targets = vec![
            TmuxOptionTarget::Pane(pane),
            TmuxOptionTarget::Window(window.id),
            TmuxOptionTarget::GlobalWindow,
        ];
        for (index, title, targets) in [
            (0, "Server Options", vec![TmuxOptionTarget::Server]),
            (1, "Session Options", session_targets.clone()),
            (2, "Window & Pane Options", window_targets.clone()),
            (3, "Session Hooks", session_targets),
            (4, "Window & Pane Hooks", window_targets),
        ] {
            self.customize_option_rows(
                &mut rows,
                mode,
                index,
                title,
                &targets,
                format.as_deref(),
                expand,
            );
        }
        if !mode.changed_only {
            self.customize_environment_rows(&mut rows, mode, None, format.as_deref(), expand);
            self.customize_environment_rows(
                &mut rows,
                mode,
                Some(window.session),
                format.as_deref(),
                expand,
            );
        }
        self.customize_key_rows(&mut rows, mode, format.as_deref(), expand);
        for index in 0..rows.len() {
            rows[index].children = rows
                .get(index + 1)
                .is_some_and(|next| next.depth > rows[index].depth);
        }
        rows
    }

    #[allow(clippy::too_many_arguments)]
    fn customize_option_rows(
        &self,
        rows: &mut Vec<Row>,
        mode: &CustomizeMode,
        index: usize,
        title: &str,
        targets: &[TmuxOptionTarget],
        format: Option<&str>,
        expand: &mut CustomizeExpand<'_>,
    ) {
        let hooks = index >= 3;
        let section = rows.len();
        let section_id = format!("options:{index}");
        rows.push(Row {
            id: section_id.clone(),
            name: title.to_owned(),
            text: None,
            depth: 0,
            parent: None,
            children: false,
            no_tag: true,
            item: Item::Section,
        });
        let mut user = Vec::<String>::new();
        for target in targets.iter().rev() {
            let mut names = self
                .user_options_at_target(*target)
                .map(|options| options.keys().cloned().collect::<BTreeSet<_>>())
                .unwrap_or_default();
            names.extend(
                self.format_monitors
                    .iter()
                    .filter(|monitor| monitor.target == *target)
                    .map(|monitor| monitor.name.clone()),
            );
            for name in names {
                if !user.contains(&name) {
                    user.push(name);
                }
            }
        }
        let table = tmux_options()
            .filter(|option| tmux_option_is_hook(option.name) == hooks)
            .filter(|option| match option.scope {
                TmuxOptionScope::Server => index == 0,
                TmuxOptionScope::Session => index == 1 || index == 3,
                TmuxOptionScope::Window | TmuxOptionScope::WindowPane => index == 2 || index == 4,
            })
            .map(|option| (option.name.to_owned(), option))
            .collect::<BTreeMap<_, _>>();
        let names = user
            .into_iter()
            .map(|name| (name, None))
            .chain(table.into_iter().map(|(name, option)| (name, Some(option))));
        let mut count = 0;
        for (name, metadata) in names {
            let (owner, value, entries) = self.customize_option_value(targets, &name, metadata);
            if metadata.is_none()
                && (self.hook_events.contains(&name)
                    || self.customize_monitor(owner, &name).is_some())
                    != hooks
            {
                continue;
            }
            let global = matches!(
                owner,
                TmuxOptionTarget::Server
                    | TmuxOptionTarget::GlobalSession
                    | TmuxOptionTarget::GlobalWindow
            );
            if mode.hide_global && global {
                continue;
            }
            if mode.changed_only && !customize_option_changed(metadata, &value, &entries, None) {
                continue;
            }
            let array = metadata.is_some_and(|option| option.is_array);
            let scope = self.customize_scope(owner);
            let unit = metadata.map_or("", |option| option.metadata.unit);
            let monitor = metadata
                .is_none()
                .then(|| self.customize_monitor(owner, &name))
                .flatten()
                .map(|monitor| {
                    format_monitor_display(&monitor.name, monitor.scope, &monitor.format)
                });
            let mut vars = BTreeMap::from([
                ("is_option".to_owned(), "1".to_owned()),
                ("is_key".to_owned(), "0".to_owned()),
                ("is_environment".to_owned(), "0".to_owned()),
                ("option_name".to_owned(), name.clone()),
                ("option_is_global".to_owned(), u8::from(global).to_string()),
                ("option_is_array".to_owned(), u8::from(array).to_string()),
                (
                    "option_is_hook".to_owned(),
                    u8::from(hooks && metadata.is_some()).to_string(),
                ),
                (
                    "option_is_monitor".to_owned(),
                    u8::from(monitor.is_some()).to_string(),
                ),
                ("option_scope".to_owned(), scope.clone()),
                ("option_unit".to_owned(), unit.to_owned()),
                (
                    "option_monitor".to_owned(),
                    monitor.clone().unwrap_or_default(),
                ),
            ]);
            if !array {
                vars.insert("option_value".to_owned(), value.clone());
            }
            if let Some(filter) = &mode.filter
                && !format_true(&expand(filter, &vars))
            {
                continue;
            }
            let text = |vars: &BTreeMap<String, String>, expand: &mut CustomizeExpand<'_>| {
                format.map_or_else(
                    || {
                        format!(
                            "{}#[fg=themelightgrey]#[ignore]{}{}",
                            if global {
                                String::new()
                            } else {
                                format!("#[reverse]({scope})#[default] ")
                            },
                            vars.get("option_value").map_or("", String::as_str),
                            if unit.is_empty() {
                                String::new()
                            } else {
                                format!(" {unit}")
                            }
                        )
                    },
                    |format| expand(format, vars),
                )
            };
            let option_row = rows.len();
            let row_id = format!("{section_id}/{name}");
            rows.push(Row {
                id: row_id.clone(),
                name: name.clone(),
                text: (!array).then(|| text(&vars, expand)),
                depth: 1,
                parent: Some(section),
                children: false,
                no_tag: false,
                item: Item::Option {
                    name: name.clone(),
                    array_key: None,
                    target: owner,
                    metadata,
                    value: value.clone(),
                    global,
                    hook: hooks,
                    monitor: monitor.clone(),
                },
            });
            count += 1;
            if array {
                for (key, entry) in &entries {
                    if mode.changed_only
                        && !customize_option_changed(metadata, entry, &entries, Some(key))
                    {
                        continue;
                    }
                    let full_name = format!("{name}[{key}]");
                    vars.insert("option_name".to_owned(), full_name.clone());
                    vars.insert("option_value".to_owned(), entry.clone());
                    rows.push(Row {
                        id: format!("{row_id}/{key}"),
                        name: full_name,
                        text: Some(text(&vars, expand)),
                        depth: 2,
                        parent: Some(option_row),
                        children: false,
                        no_tag: false,
                        item: Item::Option {
                            name: name.clone(),
                            array_key: Some(key.clone()),
                            target: owner,
                            metadata,
                            value: entry.clone(),
                            global,
                            hook: hooks,
                            monitor: None,
                        },
                    });
                    count += 1;
                }
            }
        }
        if mode.changed_only && count == 0 {
            rows.truncate(section);
        }
    }

    fn customize_option_value(
        &self,
        targets: &[TmuxOptionTarget],
        name: &str,
        metadata: Option<TmuxOption>,
    ) -> (TmuxOptionTarget, String, Vec<(String, String)>) {
        let last = *targets.last().expect("a section has a target");
        if let Some(option) = metadata
            && tmux_option_is_hook(option.name)
        {
            let owner = targets
                .iter()
                .copied()
                .find(|target| self.hook_array(*target, name).is_some())
                .unwrap_or(last);
            let entries = self.customize_array_entries(owner, name);
            let value = entries
                .iter()
                .map(|(_, value)| value.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            return (owner, value, entries);
        }
        let mut owner = last;
        let mut value = None;
        for target in targets {
            let found = if let Some(option) = metadata {
                if option.is_array {
                    self.array_option(*target, name)
                        .map(|values| values.values().cloned().collect::<Vec<_>>().join(" "))
                } else {
                    self.tmux_option_readback(option, *target, false)
                        .ok()
                        .flatten()
                        .map(|(value, _)| value)
                }
            } else {
                self.user_option_at_target(*target, name)
                    .map(ToString::to_string)
                    .or_else(|| self.customize_monitor(*target, name).map(|_| String::new()))
            };
            if let Some(found) = found {
                owner = *target;
                value = Some(found);
                break;
            }
        }
        let array = metadata.is_some_and(|option| option.is_array);
        let entries = if array {
            self.customize_array_entries(owner, name)
        } else {
            Vec::new()
        };
        let value = value.unwrap_or_else(|| {
            metadata
                .and_then(|option| {
                    if option.is_array {
                        Some(
                            entries
                                .iter()
                                .map(|(_, value)| value.as_str())
                                .collect::<Vec<_>>()
                                .join(" "),
                        )
                    } else {
                        self.tmux_option_readback(option, owner, true)
                            .ok()
                            .flatten()
                            .map(|(value, _)| value)
                    }
                })
                .unwrap_or_default()
        });
        (owner, value, entries)
    }

    pub(super) fn customize_option_chain(&self, target: TmuxOptionTarget) -> Vec<TmuxOptionTarget> {
        let window = |pane| {
            self.state
                .window_for_pane(pane)
                .map(TmuxOptionTarget::Window)
        };
        match target {
            TmuxOptionTarget::Pane(pane) => [Some(target), window(pane)]
                .into_iter()
                .flatten()
                .chain([TmuxOptionTarget::GlobalWindow])
                .collect(),
            TmuxOptionTarget::Window(_) => vec![target, TmuxOptionTarget::GlobalWindow],
            TmuxOptionTarget::Session(_) => vec![target, TmuxOptionTarget::GlobalSession],
            _ => vec![target],
        }
    }

    pub(super) fn customize_holds(&self, name: &str, target: TmuxOptionTarget) -> bool {
        if self.customize_monitor(target, name).is_some()
            || self.user_option_at_target(target, name).is_some()
        {
            return true;
        }
        let Some(option) = tmux_options().find(|option| option.name == name) else {
            return false;
        };
        if tmux_option_is_hook(name) {
            return self.hook_array(target, name).is_some();
        }
        if option.is_array {
            return self.array_option(target, name).is_some();
        }
        self.tmux_option_readback(option, target, false)
            .ok()
            .flatten()
            .is_some()
    }

    pub(super) fn customize_remove_option(
        &self,
        name: &str,
        target: TmuxOptionTarget,
    ) -> Vec<CommandInvocation> {
        let mut commands = Vec::new();
        if target != TmuxOptionTarget::Server && self.customize_monitor(target, name).is_some() {
            let mut args = vec!["-u".to_owned()];
            args.extend(customize_target_args(target));
            args.extend(["-B".to_owned(), name.to_owned()]);
            commands.push(CommandInvocation::new("set-hook", args));
        }
        commands.push(customize_unset_command(name, target));
        commands
    }

    pub(super) fn customize_monitor(
        &self,
        target: TmuxOptionTarget,
        name: &str,
    ) -> Option<&FormatMonitorEntry> {
        self.format_monitors
            .iter()
            .find(|monitor| monitor.target == target && monitor.name == name)
    }

    pub(super) fn customize_array_entries(
        &self,
        target: TmuxOptionTarget,
        name: &str,
    ) -> Vec<(String, String)> {
        if tmux_option_is_hook(name) {
            return self
                .hook_array(target, name)
                .map(|hook| {
                    hook.iter()
                        .map(|(key, commands)| {
                            (
                                key.display(),
                                commands
                                    .iter()
                                    .map(format_command)
                                    .collect::<Vec<_>>()
                                    .join(" ; "),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        self.customize_array_values(target, name)
            .into_iter()
            .map(|(key, value)| (key.display(), value))
            .collect()
    }

    pub(super) fn customize_environment_entries(
        &self,
        session: Option<SessionId>,
    ) -> Vec<(String, Option<String>, bool)> {
        let collect = |environment: &Environment| {
            environment
                .iter()
                .map(|(name, entry)| {
                    (
                        name.to_string(),
                        entry.value.as_ref().map(ToString::to_string),
                        entry.hidden,
                    )
                })
                .collect::<Vec<_>>()
        };
        match session {
            None => collect(&self.global_environment),
            Some(session) => self
                .session_environments
                .get(&session)
                .map(|retained| collect(&retained.inner.lock()))
                .unwrap_or_default(),
        }
    }

    fn customize_environment_rows(
        &self,
        rows: &mut Vec<Row>,
        mode: &CustomizeMode,
        session: Option<SessionId>,
        format: Option<&str>,
        expand: &mut CustomizeExpand<'_>,
    ) {
        let section = rows.len();
        let (title, section_id) = match session {
            None => ("Global Environment", "environment:global"),
            Some(_) => ("Session Environment", "environment:session"),
        };
        rows.push(Row {
            id: section_id.to_owned(),
            name: title.to_owned(),
            text: None,
            depth: 0,
            parent: None,
            children: false,
            no_tag: true,
            item: Item::Section,
        });
        let scope = session.map_or_else(String::new, |session| {
            self.customize_scope(TmuxOptionTarget::Session(session))
        });
        let mut vars = BTreeMap::from([
            ("is_option".to_owned(), "0".to_owned()),
            ("is_key".to_owned(), "0".to_owned()),
            ("is_environment".to_owned(), "1".to_owned()),
            (
                "environment_is_global".to_owned(),
                u8::from(session.is_none()).to_string(),
            ),
            ("environment_scope".to_owned(), scope),
        ]);
        for (name, value, hidden) in self.customize_environment_entries(session) {
            vars.insert("environment_name".to_owned(), name.clone());
            vars.insert(
                "environment_hidden".to_owned(),
                u8::from(hidden).to_string(),
            );
            vars.insert(
                "environment_removed".to_owned(),
                u8::from(value.is_none()).to_string(),
            );
            vars.insert(
                "environment_value".to_owned(),
                value.clone().unwrap_or_default(),
            );
            if let Some(filter) = &mode.filter
                && !format_true(&expand(filter, &vars))
            {
                continue;
            }
            let (row_name, text) = match &value {
                None => (format!("-{name}"), None),
                Some(value) => (
                    name.clone(),
                    Some(format.map_or_else(
                        || format!("#[fg=themelightgrey]#[ignore]{value}"),
                        |format| expand(format, &vars),
                    )),
                ),
            };
            rows.push(Row {
                id: format!("{section_id}/{name}"),
                name: row_name,
                text,
                depth: 1,
                parent: Some(section),
                children: false,
                no_tag: false,
                item: Item::Environment {
                    name,
                    session,
                    value,
                    hidden,
                },
            });
        }
    }

    fn customize_key_rows(
        &self,
        rows: &mut Vec<Row>,
        mode: &CustomizeMode,
        format: Option<&str>,
        expand: &mut CustomizeExpand<'_>,
    ) {
        let defaults = mode.changed_only.then(KeyTables::default);
        let tables = self
            .keys
            .table_names()
            .filter(|table| !matches!(*table, "choose-tree" | "choose-buffer" | "choose-client"))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        for table in tables {
            let bindings = listed_keys(self.keys.list(Some(&table)));
            if bindings.is_empty() {
                continue;
            }
            let section = rows.len();
            let id = format!("keys:{table}");
            rows.push(Row {
                id: id.clone(),
                name: format!("Key Table - {table}"),
                text: None,
                depth: 0,
                parent: None,
                children: false,
                no_tag: true,
                item: Item::Section,
            });
            let mut count = 0;
            for listed in bindings {
                if let Some(defaults) = &defaults
                    && !customize_key_changed(defaults.get(&table, &listed.key), listed.binding)
                {
                    continue;
                }
                let key = listed.key.clone();
                let mut vars = BTreeMap::from([
                    ("is_option".to_owned(), "0".to_owned()),
                    ("is_key".to_owned(), "1".to_owned()),
                    ("is_environment".to_owned(), "0".to_owned()),
                    ("key".to_owned(), key.clone()),
                ]);
                if let Some(note) = &listed.binding.note {
                    vars.insert("key_note".to_owned(), note.clone());
                }
                if let Some(filter) = &mode.filter
                    && !format_true(&expand(filter, &vars))
                {
                    continue;
                }
                let key_row = rows.len();
                let key_id = format!("{id}\u{0}{key}");
                rows.push(Row {
                    id: key_id.clone(),
                    name: format.map_or_else(|| key.clone(), |format| expand(format, &vars)),
                    text: None,
                    depth: 1,
                    parent: Some(section),
                    children: false,
                    no_tag: false,
                    item: Item::Key {
                        table: table.clone(),
                        key: key.clone(),
                    },
                });
                count += 1;
                let command = customize_command_print(listed.binding);
                for (field, text) in [
                    ("Command", format!("#[fg=themelightgrey]#[ignore]{command}")),
                    (
                        "Note",
                        listed
                            .binding
                            .note
                            .as_ref()
                            .map_or_else(String::new, |note| {
                                format!("#[fg=themelightgrey]#[ignore]{note}")
                            }),
                    ),
                    (
                        "Repeat",
                        format!(
                            "#[fg=themelightgrey]#[ignore]{}",
                            if listed.binding.repeat { "on" } else { "off" }
                        ),
                    ),
                ] {
                    rows.push(Row {
                        id: format!("{key_id}\u{0}{field}"),
                        name: field.to_owned(),
                        text: Some(text),
                        depth: 2,
                        parent: Some(key_row),
                        children: false,
                        no_tag: true,
                        item: Item::KeyField {
                            table: table.clone(),
                            key: key.clone(),
                        },
                    });
                }
            }
            if defaults.is_some() && count == 0 {
                rows.truncate(section);
            }
        }
    }

    pub(super) fn customize_scope(&self, target: TmuxOptionTarget) -> String {
        match target {
            TmuxOptionTarget::Session(id) => format!(
                "session {}",
                self.state
                    .sessions
                    .get(&id)
                    .map_or("", |session| session.name.as_str())
            ),
            TmuxOptionTarget::Window(id) => format!(
                "window {}",
                self.state.windows.get(&id).map_or(0, |window| window.index)
            ),
            TmuxOptionTarget::Pane(id) => format!(
                "pane {}",
                self.state
                    .window_for_pane(id)
                    .and_then(|window| self.pane_index(window, id))
                    .unwrap_or(0)
            ),
            _ => String::new(),
        }
    }

    pub(super) fn customize_search_set(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        backward: bool,
        expand: &mut CustomizeExpand<'_>,
    ) {
        let Some(search) = mode.search.clone() else {
            return;
        };
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        let Some(&start) = lines.get(mode.current) else {
            return;
        };
        let icase = search.chars().all(|character| !character.is_uppercase());
        let needle = search.to_lowercase();
        let count = rows.len();
        let found = (1..count)
            .map(|step| {
                if backward {
                    (start + count - step) % count
                } else {
                    (start + step) % count
                }
            })
            .find(|index| {
                if icase {
                    rows[*index].name.to_lowercase().contains(&needle)
                } else {
                    rows[*index].name.contains(&search)
                }
            });
        let Some(found) = found else {
            return;
        };
        let mut parent = rows[found].parent;
        while let Some(index) = parent {
            mode.expanded.insert(rows[index].id.clone());
            parent = rows[index].parent;
        }
        let tag = rows[lines[mode.current]].id.clone();
        let found = rows[found].id.clone();
        self.customize_build(pane, mode, Some(tag), expand);
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        if let Some(position) = lines.iter().position(|line| rows[*line].id == found) {
            mode.current = position;
            mode.check_selected(lines.len());
        }
    }
}

fn customize_key_changed(default: Option<&Binding>, binding: &Binding) -> bool {
    default.is_none_or(|default| {
        default.repeat != binding.repeat
            || default.note != binding.note
            || customize_command_print(default) != customize_command_print(binding)
    })
}
