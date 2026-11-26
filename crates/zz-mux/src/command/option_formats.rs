use super::*;

impl MuxEngine {
    #[must_use]
    pub fn format_option_snapshot(&self) -> StatusRowVariables {
        let mut base = BTreeMap::new();
        self.extend_format_option_values(
            &mut base,
            TmuxOptionScope::Server,
            TmuxOptionTarget::Server,
        );
        self.extend_format_option_values(
            &mut base,
            TmuxOptionScope::Session,
            TmuxOptionTarget::GlobalSession,
        );
        self.extend_format_option_values(
            &mut base,
            TmuxOptionScope::Window,
            TmuxOptionTarget::GlobalWindow,
        );
        self.extend_format_option_values(
            &mut base,
            TmuxOptionScope::WindowPane,
            TmuxOptionTarget::GlobalWindow,
        );
        let mut sessions = BTreeMap::new();
        let mut session_active_windows = BTreeMap::new();
        for session in self.state.sessions.values() {
            let mut values = BTreeMap::new();
            self.extend_format_option_values(
                &mut values,
                TmuxOptionScope::Session,
                TmuxOptionTarget::Session(session.id),
            );
            sessions.insert(session.id.to_string(), values);
            session_active_windows
                .insert(session.id.to_string(), session.active_window.to_string());
        }
        let mut windows = BTreeMap::new();
        let mut window_active_panes = BTreeMap::new();
        let mut window_sessions = BTreeMap::new();
        let mut panes = BTreeMap::new();
        let mut pane_windows = BTreeMap::new();
        for window in self.state.windows.values() {
            let mut values = BTreeMap::new();
            self.extend_format_option_values(
                &mut values,
                TmuxOptionScope::Window,
                TmuxOptionTarget::Window(window.id),
            );
            windows.insert(window.id.to_string(), values);
            window_active_panes.insert(window.id.to_string(), window.active_pane.to_string());
            window_sessions.insert(window.id.to_string(), window.session.to_string());
            for pane in window.panes.keys() {
                let mut values = BTreeMap::new();
                self.extend_format_option_values(
                    &mut values,
                    TmuxOptionScope::WindowPane,
                    TmuxOptionTarget::Pane(*pane),
                );
                panes.insert(pane.to_string(), values);
                pane_windows.insert(pane.to_string(), window.id.to_string());
            }
        }
        StatusRowVariables {
            base,
            sessions,
            windows,
            panes,
            session_active_windows,
            window_active_panes,
            pane_windows,
            window_sessions,
        }
    }

    #[must_use]
    pub const fn format_options_generation(&self) -> u64 {
        self.format_options_generation
    }

    #[must_use]
    pub fn format_cache_identity(&self) -> Weak<()> {
        Arc::downgrade(&self.format_cache_identity)
    }

    #[must_use]
    pub fn format_cache_identity_matches(&self, identity: &Weak<()>) -> bool {
        identity.as_ptr() == Arc::as_ptr(&self.format_cache_identity)
    }

    #[must_use]
    pub fn cached_format_option_snapshot(&self) -> Arc<StatusRowVariables> {
        let key = (self.format_options_generation, self.state.generation());
        let mut cache = self.format_option_cache.lock();
        if let Some((options, tree, snapshot)) = cache.as_ref()
            && (*options, *tree) == key
        {
            return Arc::clone(snapshot);
        }
        let snapshot = Arc::new(self.format_option_snapshot());
        *cache = Some((key.0, key.1, Arc::clone(&snapshot)));
        snapshot
    }

    #[must_use]
    pub fn cached_format_needs<'t>(
        &self,
        templates: impl IntoIterator<Item = &'t str>,
    ) -> crate::FormatNeeds {
        let templates = templates.into_iter().collect::<Vec<_>>();
        let key =
            std::hash::BuildHasher::hash_one(&foldhash::fast::FixedState::default(), &templates);
        let mut cache = self.format_needs_cache.lock();
        if let Some((generation, sources, needs)) = cache.get(&key)
            && *generation == self.format_options_generation
            && sources
                .iter()
                .map(String::as_str)
                .eq(templates.iter().copied())
        {
            return *needs;
        }
        let needs = self.format_needs(templates.iter().copied());
        if cache.len() >= 128 {
            cache.clear();
        }
        cache.insert(
            key,
            (
                self.format_options_generation,
                templates.into_iter().map(str::to_owned).collect(),
                needs,
            ),
        );
        needs
    }

    fn extend_format_option_values(
        &self,
        values: &mut BTreeMap<String, String>,
        scope: TmuxOptionScope,
        target: TmuxOptionTarget,
    ) {
        for &option in tmux_consumer_options(scope) {
            if let Some(array) = self.format_option_array(target, option.name) {
                values.insert(
                    option.name.to_owned(),
                    array.values().cloned().collect::<Vec<_>>().join(" "),
                );
                for (index, value) in array {
                    values.insert(
                        format!("{}[{}]", option.name, index.display()),
                        value.clone(),
                    );
                }
            } else if let Some(value) = self.format_scalar_option(target, option) {
                values.insert(option.name.to_owned(), value);
            }
        }
    }

    fn format_option_array(&self, target: TmuxOptionTarget, name: &str) -> Option<&StringArray> {
        matches!(
            name,
            "command-alias"
                | "pane-colours"
                | "status-format"
                | "terminal-features"
                | "terminal-overrides"
                | "update-environment"
                | "user-keys"
        )
        .then(|| {
            self.array_option_readback(target, name, true)
                .map(|(array, _)| array)
        })
        .flatten()
    }

    fn format_scalar_option(&self, target: TmuxOptionTarget, option: TmuxOption) -> Option<String> {
        let value = self
            .tmux_option_readback(option, target, true)
            .ok()
            .flatten()
            .map(|(value, _)| value)?;
        Some(if tmux_option_format_is_flag(option.name) {
            match value.as_str() {
                "on" => "1".to_owned(),
                "off" => "0".to_owned(),
                _ => value,
            }
        } else {
            value
        })
    }

    pub fn format_option_value(&self, context: &StatusContext, name: &str) -> Option<String> {
        let (option, index) = parse_format_option(name)?;
        if matches!(index, FormatOptionIndex::Invalid) {
            return Some(String::new());
        }
        let target = self.format_option_target(context, option.scope);
        if let Some(array) = self.format_option_array(target, option.name) {
            return match index {
                FormatOptionIndex::Whole => {
                    Some(array.values().cloned().collect::<Vec<_>>().join(" "))
                }
                FormatOptionIndex::Indexed(index) => {
                    Some(array.get(&index).cloned().unwrap_or_default())
                }
                FormatOptionIndex::Invalid => unreachable!(),
            };
        }
        self.format_scalar_option(target, option)
    }

    pub(crate) fn format_option_always_answers(&self, name: &str) -> bool {
        let Some((option, index)) = parse_format_option(name) else {
            return false;
        };
        let global = match option.scope {
            TmuxOptionScope::Server => TmuxOptionTarget::Server,
            TmuxOptionScope::Session => TmuxOptionTarget::GlobalSession,
            TmuxOptionScope::Window | TmuxOptionScope::WindowPane => TmuxOptionTarget::GlobalWindow,
        };
        matches!(index, FormatOptionIndex::Invalid)
            || self.format_option_array(global, option.name).is_some()
            || self.format_scalar_option(global, option).is_some()
    }

    pub(crate) fn format_option_texts(&self, name: &str, visit: &mut dyn FnMut(&str)) -> bool {
        if name.starts_with('@') {
            let mut global = false;
            for options in [
                &self.server_user_options,
                &self.global_session_user_options,
                &self.global_window_user_options,
            ] {
                if let Some(value) = options.get(name) {
                    visit(value);
                    global = true;
                }
            }
            for options in self
                .session_user_options
                .values()
                .chain(self.window_user_options.values())
                .chain(self.pane_user_options.values())
            {
                if let Some(value) = options.get(name) {
                    visit(value);
                }
            }
            return global;
        }
        let Some((option, _)) = parse_format_option(name) else {
            return false;
        };
        let global = match option.scope {
            TmuxOptionScope::Server => TmuxOptionTarget::Server,
            TmuxOptionScope::Session => TmuxOptionTarget::GlobalSession,
            TmuxOptionScope::Window | TmuxOptionScope::WindowPane => TmuxOptionTarget::GlobalWindow,
        };
        let complete = self.visit_format_option_at(global, option, true, visit);
        match option.scope {
            TmuxOptionScope::Server => {}
            TmuxOptionScope::Session => {
                for session in self.state.sessions.keys() {
                    self.visit_format_option_at(
                        TmuxOptionTarget::Session(*session),
                        option,
                        false,
                        visit,
                    );
                }
            }
            TmuxOptionScope::Window => {
                for window in self.state.windows.keys() {
                    self.visit_format_option_at(
                        TmuxOptionTarget::Window(*window),
                        option,
                        false,
                        visit,
                    );
                }
            }
            TmuxOptionScope::WindowPane => {
                for window in self.state.windows.values() {
                    self.visit_format_option_at(
                        TmuxOptionTarget::Window(window.id),
                        option,
                        false,
                        visit,
                    );
                    for pane in window.panes.keys() {
                        self.visit_format_option_at(
                            TmuxOptionTarget::Pane(*pane),
                            option,
                            false,
                            visit,
                        );
                    }
                }
            }
        }
        complete
    }

    fn visit_format_option_at(
        &self,
        target: TmuxOptionTarget,
        option: TmuxOption,
        inherited: bool,
        visit: &mut dyn FnMut(&str),
    ) -> bool {
        if let Some((array, _)) = matches!(
            option.name,
            "command-alias"
                | "pane-colours"
                | "status-format"
                | "terminal-features"
                | "terminal-overrides"
                | "update-environment"
                | "user-keys"
        )
        .then(|| self.array_option_readback(target, option.name, inherited))
        .flatten()
        {
            for value in array.values() {
                visit(value);
            }
            return true;
        }
        let Ok(Some((value, _))) = self.tmux_option_readback(option, target, inherited) else {
            return false;
        };
        visit(&value);
        true
    }

    fn format_option_target(
        &self,
        context: &StatusContext,
        scope: TmuxOptionScope,
    ) -> TmuxOptionTarget {
        let pane = parse_pane_id(&context.pane_id)
            .filter(|pane| self.state.window_for_pane(*pane).is_some());
        let window = pane
            .and_then(|pane| self.state.window_for_pane(pane))
            .or_else(|| {
                parse_window_id(&context.window_id)
                    .filter(|window| self.state.windows.contains_key(window))
            });
        let session = window
            .and_then(|window| self.state.windows.get(&window).map(|window| window.session))
            .or_else(|| {
                parse_session_id(&context.session_id)
                    .filter(|session| self.state.sessions.contains_key(session))
            });
        let window = window.or_else(|| {
            session
                .and_then(|session| self.state.sessions.get(&session))
                .map(|session| session.active_window)
        });
        let pane = pane.or_else(|| {
            window
                .and_then(|window| self.state.windows.get(&window))
                .map(|window| window.active_pane)
        });
        match scope {
            TmuxOptionScope::Server => TmuxOptionTarget::Server,
            TmuxOptionScope::Session => {
                session.map_or(TmuxOptionTarget::GlobalSession, TmuxOptionTarget::Session)
            }
            TmuxOptionScope::Window => {
                window.map_or(TmuxOptionTarget::GlobalWindow, TmuxOptionTarget::Window)
            }
            TmuxOptionScope::WindowPane => {
                pane.map_or(TmuxOptionTarget::GlobalWindow, TmuxOptionTarget::Pane)
            }
        }
    }

    pub(crate) fn format_window_user_options(&self, window: WindowId) -> Vec<(String, String)> {
        self.window_user_options
            .get(&window)
            .map(|values| {
                values
                    .iter()
                    .map(|(name, value)| (name.clone(), value.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn format_option_rows(&self, target: TmuxOptionTarget) -> Vec<FormatOptionRow> {
        let mut rows = Vec::new();
        if let Some(values) = self.user_options_at_target(target) {
            for (name, value) in values {
                rows.push(FormatOptionRow {
                    name: name.clone(),
                    value: value.to_string(),
                    is_user: true,
                    ..FormatOptionRow::default()
                });
            }
        }
        for option in tmux_options_for_listing(target, true) {
            let is_hook = tmux_option_is_hook(option.name);
            if is_hook {
                if let Some((hook, _)) = self.hook_array_readback(target, option.name, false) {
                    let items = hook
                        .iter()
                        .map(|(index, commands)| {
                            (
                                index.display(),
                                commands
                                    .iter()
                                    .map(format_command)
                                    .collect::<Vec<_>>()
                                    .join(" ; "),
                            )
                        })
                        .collect::<Vec<_>>();
                    push_option_loop_array(&mut rows, option.name, &items, true);
                }
            } else if tmux_stored_array(option.name).is_some() {
                if let Some((array, _)) = self.array_option_readback(target, option.name, false) {
                    let items = array
                        .iter()
                        .map(|(index, value)| (index.display(), value.clone()))
                        .collect::<Vec<_>>();
                    push_option_loop_array(&mut rows, option.name, &items, false);
                }
            } else if let Ok(Some((value, _))) = self.tmux_option_readback(option, target, false) {
                rows.push(FormatOptionRow {
                    name: option.name.to_owned(),
                    value,
                    ..FormatOptionRow::default()
                });
            }
        }
        rows.sort_by(|left, right| left.name.cmp(&right.name));
        rows
    }

    #[must_use]
    pub fn pane_user_option(&self, pane: PaneId, name: &str) -> Option<&str> {
        self.pane_user_options
            .get(&pane)
            .and_then(|values| values.get(name))
            .map(RawText::as_str)
    }

    #[must_use]
    pub fn format_user_option(
        &self,
        pane: &str,
        window: &str,
        session: &str,
        name: &str,
    ) -> Option<&str> {
        self.format_user_option_bytes(pane, window, session, name)
            .map(RawText::as_str)
    }

    #[must_use]
    pub fn format_user_option_bytes(
        &self,
        pane: &str,
        window: &str,
        session: &str,
        name: &str,
    ) -> Option<&RawText> {
        let pane = pane.parse().ok();
        let window = pane
            .and_then(|pane| self.state.window_for_pane(pane))
            .or_else(|| window.parse().ok());
        let session = session.parse().ok();
        pane.and_then(|pane| self.pane_user_options.get(&pane))
            .and_then(|values| values.get(name))
            .or_else(|| {
                window
                    .and_then(|window| self.window_user_options.get(&window))
                    .and_then(|values| values.get(name))
            })
            .or_else(|| {
                session
                    .and_then(|session| self.session_user_options.get(&session))
                    .and_then(|values| values.get(name))
            })
            .or_else(|| self.global_window_user_options.get(name))
            .or_else(|| self.global_session_user_options.get(name))
            .or_else(|| self.server_user_options.get(name))
    }
}

pub(super) fn parse_format_option(input: &str) -> Option<(TmuxOption, FormatOptionIndex)> {
    let (name, index) = if let Some(open) = input.find('[') {
        let name = &input[..open];
        let rest = &input[open + 1..];
        let index = rest
            .strip_suffix(']')
            .filter(|value| !value.is_empty() && !value.contains('[') && !value.contains(']'))
            .and_then(|value| {
                if value.bytes().all(|byte| byte.is_ascii_digit()) {
                    value.parse::<u32>().ok().map(ArrayIndex::Numeric)
                } else {
                    Some(ArrayIndex::Named(value.to_owned()))
                }
            })
            .map_or(FormatOptionIndex::Invalid, FormatOptionIndex::Indexed);
        (name, index)
    } else {
        (input, FormatOptionIndex::Whole)
    };
    let (option, consumer) = tmux_option_with_consumer(name)?;
    if !consumer {
        return None;
    }
    if matches!(
        option.name,
        "command-alias"
            | "pane-colours"
            | "status-format"
            | "terminal-features"
            | "terminal-overrides"
            | "update-environment"
            | "user-keys"
    ) {
        return Some((option, index));
    }
    matches!(index, FormatOptionIndex::Whole).then_some((option, index))
}

pub(super) fn format_option_map_value(
    values: &BTreeMap<String, String>,
    name: &str,
    index: &FormatOptionIndex,
) -> Option<String> {
    match index {
        FormatOptionIndex::Whole => values.get(name).cloned(),
        FormatOptionIndex::Indexed(index) => Some(
            values
                .get(&format!("{name}[{}]", index.display()))
                .cloned()
                .unwrap_or_default(),
        ),
        FormatOptionIndex::Invalid => Some(String::new()),
    }
}

fn push_option_loop_array(
    rows: &mut Vec<FormatOptionRow>,
    name: &str,
    items: &[(String, String)],
    is_hook: bool,
) {
    if items.is_empty() {
        rows.push(FormatOptionRow {
            name: name.to_owned(),
            is_array: true,
            array_first: true,
            array_last: true,
            is_hook,
            ..FormatOptionRow::default()
        });
        return;
    }
    let last = items.len() - 1;
    for (index, (key, value)) in items.iter().enumerate() {
        rows.push(FormatOptionRow {
            name: name.to_owned(),
            value: value.clone(),
            is_array: true,
            array_key: key.clone(),
            array_index: key.clone(),
            array_first: index == 0,
            array_last: index == last,
            array_count: items.len(),
            is_hook,
            is_user: false,
        });
    }
}
