use super::*;

impl MuxEngine {
    pub(super) fn hook_array(&self, target: TmuxOptionTarget, name: &str) -> Option<&HookArray> {
        self.hook_table(target).and_then(|hooks| hooks.get(name))
    }

    pub(super) fn hook_array_readback(
        &self,
        target: TmuxOptionTarget,
        name: &str,
        include_inherited: bool,
    ) -> Option<(&HookArray, bool)> {
        if let Some(hook) = self.hook_array(target, name) {
            return Some((hook, false));
        }
        if !include_inherited {
            return None;
        }
        let inherited = match target {
            TmuxOptionTarget::Session(_) => self.global_hooks.get(name),
            TmuxOptionTarget::Window(_) => self.global_window_hooks.get(name),
            TmuxOptionTarget::Pane(pane) => self
                .state
                .window_for_pane(pane)
                .and_then(|window| self.window_hooks.get(&window))
                .and_then(|hooks| hooks.get(name))
                .or_else(|| self.global_window_hooks.get(name)),
            _ => None,
        }?;
        Some((inherited, true))
    }

    fn hook_array_mut(&mut self, target: TmuxOptionTarget, name: &str) -> Option<&mut HookArray> {
        self.hook_table_mut(target)
            .and_then(|hooks| hooks.get_mut(name))
    }

    pub(super) fn hook_array_mut_or_insert(
        &mut self,
        target: TmuxOptionTarget,
        name: &str,
    ) -> &mut HookArray {
        match target {
            TmuxOptionTarget::GlobalSession => self
                .global_hooks
                .get_mut(name)
                .expect("global session hook table is complete"),
            TmuxOptionTarget::Session(session) => self
                .session_hooks
                .entry(session)
                .or_default()
                .entry(name.to_owned())
                .or_default(),
            TmuxOptionTarget::GlobalWindow => self
                .global_window_hooks
                .get_mut(name)
                .expect("global window hook table is complete"),
            TmuxOptionTarget::Window(window) => self
                .window_hooks
                .entry(window)
                .or_default()
                .entry(name.to_owned())
                .or_default(),
            TmuxOptionTarget::Pane(pane) => self
                .pane_hooks
                .entry(pane)
                .or_default()
                .entry(name.to_owned())
                .or_default(),
            TmuxOptionTarget::Server => unreachable!("hooks are not server scoped"),
        }
    }

    pub(super) fn hook_table(&self, target: TmuxOptionTarget) -> Option<&HookTable> {
        match target {
            TmuxOptionTarget::GlobalSession => Some(&self.global_hooks),
            TmuxOptionTarget::Session(session) => self.session_hooks.get(&session),
            TmuxOptionTarget::GlobalWindow => Some(&self.global_window_hooks),
            TmuxOptionTarget::Window(window) => self.window_hooks.get(&window),
            TmuxOptionTarget::Pane(pane) => self.pane_hooks.get(&pane),
            TmuxOptionTarget::Server => None,
        }
    }

    fn hook_table_mut(&mut self, target: TmuxOptionTarget) -> Option<&mut HookTable> {
        match target {
            TmuxOptionTarget::GlobalSession => Some(&mut self.global_hooks),
            TmuxOptionTarget::Session(session) => self.session_hooks.get_mut(&session),
            TmuxOptionTarget::GlobalWindow => Some(&mut self.global_window_hooks),
            TmuxOptionTarget::Window(window) => self.window_hooks.get_mut(&window),
            TmuxOptionTarget::Pane(pane) => self.pane_hooks.get_mut(&pane),
            TmuxOptionTarget::Server => None,
        }
    }

    pub(super) fn unset_hook_array(
        &mut self,
        target: TmuxOptionTarget,
        name: &str,
        index: Option<&ArrayIndex>,
    ) {
        if let Some(index) = index {
            if let Some(hook) = self.hook_array_mut(target, name) {
                hook.remove(index);
            }
        } else if matches!(
            target,
            TmuxOptionTarget::GlobalSession | TmuxOptionTarget::GlobalWindow
        ) {
            self.hook_array_mut(target, name)
                .expect("global hook table is complete")
                .clear();
            self.forget_hook_fires(target, name);
        } else if let Some(hooks) = self.hook_table_mut(target) {
            hooks.remove(name);
            self.forget_hook_fires(target, name);
        }
    }

    pub(super) fn remove_pane_hook_overrides(
        &mut self,
        window: WindowId,
        name: &str,
        index: Option<&ArrayIndex>,
    ) {
        let panes = self
            .state
            .windows
            .get(&window)
            .map(|window| window.pane_order().to_vec())
            .unwrap_or_default();
        for pane in panes {
            self.unset_hook_array(TmuxOptionTarget::Pane(pane), name, index);
        }
    }

    fn scalar_table(&self, target: TmuxOptionTarget) -> Option<&ScalarTable> {
        match target {
            TmuxOptionTarget::Server => Some(&self.stored_scalars.server),
            TmuxOptionTarget::GlobalSession => Some(&self.stored_scalars.global_session),
            TmuxOptionTarget::Session(session) => self.stored_scalars.sessions.get(&session),
            TmuxOptionTarget::GlobalWindow => Some(&self.stored_scalars.global_window),
            TmuxOptionTarget::Window(window) => self.stored_scalars.windows.get(&window),
            TmuxOptionTarget::Pane(pane) => self.stored_scalars.panes.get(&pane),
        }
    }

    pub(super) fn scalar_table_mut_or_insert(
        &mut self,
        target: TmuxOptionTarget,
    ) -> &mut ScalarTable {
        match target {
            TmuxOptionTarget::Server => &mut self.stored_scalars.server,
            TmuxOptionTarget::GlobalSession => &mut self.stored_scalars.global_session,
            TmuxOptionTarget::Session(session) => {
                self.stored_scalars.sessions.entry(session).or_default()
            }
            TmuxOptionTarget::GlobalWindow => &mut self.stored_scalars.global_window,
            TmuxOptionTarget::Window(window) => {
                self.stored_scalars.windows.entry(window).or_default()
            }
            TmuxOptionTarget::Pane(pane) => self.stored_scalars.panes.entry(pane).or_default(),
        }
    }

    pub(super) fn scalar_option_explicit(
        &self,
        target: TmuxOptionTarget,
        name: &str,
    ) -> Option<&str> {
        if let Some(value) = self
            .scalar_table(target)
            .and_then(|options| options.get(name))
        {
            return Some(value);
        }
        let inherited = match target {
            TmuxOptionTarget::Session(_) => self.stored_scalars.global_session.get(name),
            TmuxOptionTarget::Window(_) => self.stored_scalars.global_window.get(name),
            TmuxOptionTarget::Pane(pane) => self
                .state
                .window_for_pane(pane)
                .and_then(|window| self.stored_scalars.windows.get(&window))
                .and_then(|options| options.get(name))
                .or_else(|| self.stored_scalars.global_window.get(name)),
            TmuxOptionTarget::Server
            | TmuxOptionTarget::GlobalSession
            | TmuxOptionTarget::GlobalWindow => None,
        };
        inherited.map(String::as_str)
    }

    pub(super) fn scalar_option_effective(
        &self,
        target: TmuxOptionTarget,
        name: &str,
    ) -> Option<&str> {
        self.scalar_option_explicit(target, name)
            .or_else(|| tmux_stored_scalar(name).map(|metadata| metadata.default))
    }

    pub(super) fn scalar_option_readback(
        &self,
        target: TmuxOptionTarget,
        name: &str,
        include_inherited: bool,
    ) -> Option<(String, bool)> {
        if let Some(value) = self
            .scalar_table(target)
            .and_then(|options| options.get(name))
        {
            return Some((tmux_option_key_display(name, value), false));
        }
        if matches!(
            target,
            TmuxOptionTarget::Server
                | TmuxOptionTarget::GlobalSession
                | TmuxOptionTarget::GlobalWindow
        ) {
            return tmux_stored_scalar(name)
                .map(|metadata| (tmux_option_key_display(name, metadata.default), false));
        }
        include_inherited
            .then(|| self.scalar_option_effective(target, name))
            .flatten()
            .map(|value| (tmux_option_key_display(name, value), true))
    }

    pub(super) fn set_stored_scalar_option(
        &mut self,
        name: &'static str,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        self.format_options_generation = self.format_options_generation.wrapping_add(1);
        let metadata = tmux_stored_scalar(name).expect("stored scalar metadata");
        let unset = option_is_unset(options);
        let locally_set = match target {
            TmuxOptionTarget::Server
            | TmuxOptionTarget::GlobalSession
            | TmuxOptionTarget::GlobalWindow => true,
            _ => self
                .scalar_table(target)
                .is_some_and(|values| values.contains_key(name)),
        };
        if options.has("-o") && !unset && locally_set {
            return already_set_or_quiet(options, name);
        }
        if options.has("-U")
            && let TmuxOptionTarget::Window(window) = target
        {
            let panes = self
                .state
                .windows
                .get(&window)
                .map(|window| window.pane_order().to_vec())
                .unwrap_or_default();
            for pane in panes {
                remove_named_option_override(&mut self.stored_scalars.panes, pane, name);
            }
        }
        if unset {
            match target {
                TmuxOptionTarget::Server => {
                    self.stored_scalars.server.remove(name);
                }
                TmuxOptionTarget::GlobalSession => {
                    self.stored_scalars.global_session.remove(name);
                }
                TmuxOptionTarget::Session(session) => {
                    remove_named_option_override(&mut self.stored_scalars.sessions, session, name);
                }
                TmuxOptionTarget::GlobalWindow => {
                    self.stored_scalars.global_window.remove(name);
                }
                TmuxOptionTarget::Window(window) => {
                    remove_named_option_override(&mut self.stored_scalars.windows, window, name);
                }
                TmuxOptionTarget::Pane(pane) => {
                    remove_named_option_override(&mut self.stored_scalars.panes, pane, name);
                }
            }
            self.sync_prefix2_from_store(name);
            return Ok(stored_scalar_execution(name, target));
        }
        let current = self
            .scalar_option_effective(target, name)
            .expect("stored scalar has a default");
        let appended = options
            .has("-a")
            .then(|| {
                metadata
                    .kind
                    .append_separator()
                    .and_then(|separator| value.map(|value| format!("{current}{separator}{value}")))
            })
            .flatten();
        let next =
            normalize_stored_scalar_value(metadata.kind, appended.as_deref().or(value), current)?;
        self.scalar_table_mut_or_insert(target).insert(name, next);
        self.sync_prefix2_from_store(name);
        Ok(stored_scalar_execution(name, target))
    }

    fn sync_prefix2_from_store(&mut self, name: &str) {
        if name != "prefix2" {
            return;
        }
        let effective = self
            .scalar_option_effective(TmuxOptionTarget::GlobalSession, "prefix2")
            .map(str::to_owned);
        self.keys.set_prefix2(effective.as_deref());
    }

    fn array_table(&self, target: TmuxOptionTarget) -> Option<&ArrayTable> {
        match target {
            TmuxOptionTarget::Server => Some(&self.stored_arrays.server),
            TmuxOptionTarget::GlobalSession => Some(&self.stored_arrays.global_session),
            TmuxOptionTarget::Session(session) => self.stored_arrays.sessions.get(&session),
            TmuxOptionTarget::GlobalWindow => Some(&self.stored_arrays.global_window),
            TmuxOptionTarget::Window(window) => self.stored_arrays.windows.get(&window),
            TmuxOptionTarget::Pane(pane) => self.stored_arrays.panes.get(&pane),
        }
    }

    fn array_table_mut(&mut self, target: TmuxOptionTarget) -> Option<&mut ArrayTable> {
        match target {
            TmuxOptionTarget::Server => Some(&mut self.stored_arrays.server),
            TmuxOptionTarget::GlobalSession => Some(&mut self.stored_arrays.global_session),
            TmuxOptionTarget::Session(session) => self.stored_arrays.sessions.get_mut(&session),
            TmuxOptionTarget::GlobalWindow => Some(&mut self.stored_arrays.global_window),
            TmuxOptionTarget::Window(window) => self.stored_arrays.windows.get_mut(&window),
            TmuxOptionTarget::Pane(pane) => self.stored_arrays.panes.get_mut(&pane),
        }
    }

    fn array_table_mut_or_insert(&mut self, target: TmuxOptionTarget) -> &mut ArrayTable {
        match target {
            TmuxOptionTarget::Server => &mut self.stored_arrays.server,
            TmuxOptionTarget::GlobalSession => &mut self.stored_arrays.global_session,
            TmuxOptionTarget::Session(session) => {
                self.stored_arrays.sessions.entry(session).or_default()
            }
            TmuxOptionTarget::GlobalWindow => &mut self.stored_arrays.global_window,
            TmuxOptionTarget::Window(window) => {
                self.stored_arrays.windows.entry(window).or_default()
            }
            TmuxOptionTarget::Pane(pane) => self.stored_arrays.panes.entry(pane).or_default(),
        }
    }

    pub(super) fn array_option(
        &self,
        target: TmuxOptionTarget,
        name: &str,
    ) -> Option<&StringArray> {
        self.array_table(target)
            .and_then(|options| options.get(name))
    }

    pub(super) fn array_option_readback(
        &self,
        target: TmuxOptionTarget,
        name: &str,
        include_inherited: bool,
    ) -> Option<(&StringArray, bool)> {
        if let Some(array) = self.array_option(target, name) {
            return Some((array, false));
        }
        if !include_inherited {
            return None;
        }
        let inherited = match target {
            TmuxOptionTarget::Session(_) => self.stored_arrays.global_session.get(name),
            TmuxOptionTarget::Window(_) => self.stored_arrays.global_window.get(name),
            TmuxOptionTarget::Pane(pane) => self
                .state
                .window_for_pane(pane)
                .and_then(|window| self.stored_arrays.windows.get(&window))
                .and_then(|options| options.get(name))
                .or_else(|| self.stored_arrays.global_window.get(name)),
            _ => None,
        }?;
        Some((inherited, true))
    }

    pub(super) fn set_array_option(
        &mut self,
        target: TmuxOptionTarget,
        name: &'static str,
        index: Option<String>,
        value: Option<&str>,
        options: &Options,
    ) -> Result<Execution, ServerError> {
        self.format_options_generation = self.format_options_generation.wrapping_add(1);
        let metadata = tmux_stored_array(name).expect("stored array metadata");
        let index = index.map(ArrayIndex::parse);
        let unset = option_is_unset(options);
        let status_format_before =
            (name == "status-format").then(|| self.array_option(target, name).cloned());
        let already = self
            .array_option(target, name)
            .is_some_and(|array| index.as_ref().is_none_or(|index| array.contains_key(index)));
        if options.has("-o") && !unset && already {
            let displayed_index = index.as_ref().map(ArrayIndex::display);
            let option = indexed_option_name(name, displayed_index.as_deref());
            return already_set_or_quiet(options, &option);
        }
        if options.has("-U")
            && let TmuxOptionTarget::Window(window) = target
        {
            self.remove_pane_array_overrides(window, name, index.as_ref());
        }
        if unset {
            let whole = index.is_none();
            let stored = self.array_option(target, name).is_some();
            self.unset_array_option(target, name, index.as_ref());
            if let Some(before) = status_format_before {
                return Ok(self.status_format_execution(
                    target,
                    before.as_ref(),
                    whole.then_some(false),
                ));
            }
            return Ok(stored_array_execution(name, stored));
        }
        let value = value.ok_or_else(|| ServerError::InvalidCommand("empty value".to_owned()))?;
        if let Some(index) = index {
            validate_array_value(metadata.value, value)?;
            let array = self
                .array_table_mut_or_insert(target)
                .entry(name)
                .or_default();
            if options.has("-a") && metadata.value == TmuxArrayValue::String {
                array
                    .entry(index)
                    .and_modify(|current| current.push_str(value))
                    .or_insert_with(|| value.to_owned());
            } else {
                array.insert(index, value.to_owned());
            }
            if let Some(before) = status_format_before {
                return Ok(self.status_format_execution(target, before.as_ref(), Some(true)));
            }
            return Ok(stored_array_execution(name, true));
        }
        let append = options.has("-a");
        if !append {
            self.array_table_mut_or_insert(target)
                .entry(name)
                .or_default()
                .clear();
        }
        for item in value
            .split(|character| metadata.separators.contains(character))
            .filter(|item| !item.is_empty())
        {
            validate_array_value(metadata.value, item)?;
            let array = self
                .array_table_mut_or_insert(target)
                .entry(name)
                .or_default();
            let index = first_free_array_index(array.keys())?;
            array.insert(ArrayIndex::Numeric(index), item.to_owned());
        }
        if let Some(before) = status_format_before {
            return Ok(self.status_format_execution(target, before.as_ref(), Some(true)));
        }
        Ok(stored_array_execution(name, true))
    }

    fn unset_array_option(
        &mut self,
        target: TmuxOptionTarget,
        name: &'static str,
        index: Option<&ArrayIndex>,
    ) {
        self.format_options_generation = self.format_options_generation.wrapping_add(1);
        if let Some(index) = index {
            if let Some(array) = self
                .array_table_mut(target)
                .and_then(|options| options.get_mut(name))
            {
                array.remove(index);
            }
            return;
        }
        if matches!(
            target,
            TmuxOptionTarget::Server
                | TmuxOptionTarget::GlobalSession
                | TmuxOptionTarget::GlobalWindow
        ) {
            let defaults = default_array(name);
            self.array_table_mut(target)
                .expect("global array table")
                .insert(name, defaults);
        } else if let Some(options) = self.array_table_mut(target) {
            options.remove(name);
        }
    }

    fn remove_pane_array_overrides(
        &mut self,
        window: WindowId,
        name: &'static str,
        index: Option<&ArrayIndex>,
    ) {
        let panes = self
            .state
            .windows
            .get(&window)
            .map(|window| window.pane_order().to_vec())
            .unwrap_or_default();
        for pane in panes {
            self.unset_array_option(TmuxOptionTarget::Pane(pane), name, index);
        }
    }

    pub(super) fn resolve_user_option_target(
        &self,
        context: &ExecutionContext,
        options: &Options,
        force_window: bool,
    ) -> Result<TmuxOptionTarget, ServerError> {
        if options.has("-s") {
            return Ok(TmuxOptionTarget::Server);
        }
        if options.has("-p") {
            return self
                .resolve_pane(options.value("-t"), context.window, context.pane)
                .map(TmuxOptionTarget::Pane);
        }
        if force_window || options.has("-w") {
            if options.has("-g") {
                return Ok(TmuxOptionTarget::GlobalWindow);
            }
            return self
                .resolve_option_window(context, options, force_window)
                .map(TmuxOptionTarget::Window);
        }
        if options.has("-g") {
            return Ok(TmuxOptionTarget::GlobalSession);
        }
        let window = self.resolve_option_window(context, options, false)?;
        Ok(TmuxOptionTarget::Session(
            self.state.windows[&window].session,
        ))
    }

    pub(super) fn user_option_at_target(
        &self,
        target: TmuxOptionTarget,
        name: &str,
    ) -> Option<&RawText> {
        self.user_options_at_target(target)?.get(name)
    }

    pub(super) fn user_options_at_target(&self, target: TmuxOptionTarget) -> Option<&UserOptions> {
        match target {
            TmuxOptionTarget::Server => Some(&self.server_user_options),
            TmuxOptionTarget::GlobalSession => Some(&self.global_session_user_options),
            TmuxOptionTarget::Session(session) => self.session_user_options.get(&session),
            TmuxOptionTarget::GlobalWindow => Some(&self.global_window_user_options),
            TmuxOptionTarget::Window(window) => self.window_user_options.get(&window),
            TmuxOptionTarget::Pane(pane) => self.pane_user_options.get(&pane),
        }
        .map(Arc::as_ref)
    }

    pub(super) fn user_options_at_target_mut(
        &mut self,
        target: TmuxOptionTarget,
    ) -> &mut UserOptions {
        self.format_options_generation = self.format_options_generation.wrapping_add(1);
        Arc::make_mut(match target {
            TmuxOptionTarget::Server => &mut self.server_user_options,
            TmuxOptionTarget::GlobalSession => &mut self.global_session_user_options,
            TmuxOptionTarget::Session(session) => {
                self.session_user_options.entry(session).or_default()
            }
            TmuxOptionTarget::GlobalWindow => &mut self.global_window_user_options,
            TmuxOptionTarget::Window(window) => self.window_user_options.entry(window).or_default(),
            TmuxOptionTarget::Pane(pane) => self.pane_user_options.entry(pane).or_default(),
        })
    }
}

pub(super) fn remove_option_override<K: Copy + Ord, O: Copy + Ord>(
    values: &mut BTreeMap<K, BTreeMap<O, String>>,
    target: K,
    option: O,
) {
    if let Some(options) = values.get_mut(&target) {
        options.remove(&option);
        if options.is_empty() {
            values.remove(&target);
        }
    }
}

fn stored_array_execution(name: &str, changed: bool) -> Execution {
    if name == "command-alias" && changed {
        return Execution::effect(MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::CommandAlias,
            session: None,
        });
    }
    if name == "pane-colours" && changed {
        return Execution::effect(MuxEffect::TerminalKnobsChanged {
            window: None,
            pane: None,
        });
    }
    if matches!(
        name,
        "user-keys" | "terminal-features" | "terminal-overrides"
    ) && changed
    {
        return Execution::effect(MuxEffect::ClientTerminalChanged);
    }
    Execution::default()
}

fn stored_scalar_execution(name: &str, target: TmuxOptionTarget) -> Execution {
    if is_theme_option(name) {
        return Execution::effect(MuxEffect::StatusFormatsChanged { session: None });
    }
    if name == "prefix2" && target == TmuxOptionTarget::GlobalSession {
        return Execution::effect(MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::Prefix2,
            session: None,
        });
    }
    if name == "clear-on-attach" {
        return Execution::effect(MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::ClearOnAttach,
            session: None,
        });
    }
    if name == "focus-follows-mouse" {
        return Execution::effect(MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::FocusFollowsMouse,
            session: match target {
                TmuxOptionTarget::Session(session) => Some(session),
                _ => None,
            },
        });
    }
    if matches!(
        name,
        "set-titles"
            | "set-titles-string"
            | "copy-mode-position-format"
            | "copy-mode-position-style"
            | "copy-mode-selection-style"
            | "copy-mode-current-line-style"
    ) {
        let session = match target {
            TmuxOptionTarget::Session(session) => Some(session),
            _ => None,
        };
        return Execution::effect(MuxEffect::StatusFormatsChanged { session });
    }
    if matches!(
        name,
        "pane-border-style"
            | "pane-active-border-style"
            | "display-panes-format"
            | "pane-border-format"
            | "pane-border-lines"
            | "pane-border-indicators"
    ) {
        return Execution::effect(MuxEffect::SnapshotChanged);
    }
    if name == "pane-border-status" {
        return Execution {
            output: RawText::default(),
            effects: vec![
                MuxEffect::WindowSizeChanged {
                    window: match target {
                        TmuxOptionTarget::Window(window) => Some(window),
                        _ => None,
                    },
                },
                MuxEffect::SnapshotChanged,
            ],
        };
    }
    if matches!(name, "window-style" | "window-active-style") {
        let (window, pane) = match target {
            TmuxOptionTarget::Pane(pane) => (None, Some(pane)),
            TmuxOptionTarget::Window(window) => (Some(window), None),
            _ => (None, None),
        };
        return Execution::effect(MuxEffect::TerminalKnobsChanged { window, pane });
    }
    if matches!(
        name,
        "mode-style"
            | "copy-mode-match-style"
            | "copy-mode-current-match-style"
            | "copy-mode-mark-style"
    ) {
        return Execution::effect(MuxEffect::ModeStylesChanged);
    }
    Execution::default()
}

fn remove_named_option_override<K: Copy + Ord>(
    values: &mut BTreeMap<K, ScalarTable>,
    target: K,
    option: &str,
) {
    if let Some(options) = values.get_mut(&target) {
        options.remove(option);
        if options.is_empty() {
            values.remove(&target);
        }
    }
}

fn normalize_stored_scalar_value(
    kind: TmuxStoredScalarKind,
    value: Option<&str>,
    current: &str,
) -> Result<String, ServerError> {
    match kind {
        TmuxStoredScalarKind::String => value
            .map(str::to_owned)
            .ok_or_else(|| ServerError::InvalidCommand("empty value".to_owned())),
        TmuxStoredScalarKind::Style => {
            let value =
                value.ok_or_else(|| ServerError::InvalidCommand("empty value".to_owned()))?;
            if value.contains("#{") || valid_style(value) {
                Ok(value.to_owned())
            } else {
                Err(ServerError::InvalidCommand(format!(
                    "invalid style: {value}"
                )))
            }
        }
        TmuxStoredScalarKind::Colour => {
            let value =
                value.ok_or_else(|| ServerError::InvalidCommand("empty value".to_owned()))?;
            if value.is_empty()
                || value.contains("#{")
                || parse_tmux_colour(value).is_some()
                || zz_terminal::parse_x11_color(value).is_some()
            {
                Ok(value.to_owned())
            } else {
                Err(ServerError::InvalidCommand(format!(
                    "invalid colour: {value}"
                )))
            }
        }
        TmuxStoredScalarKind::Flag => {
            let enabled = match value {
                None | Some("") => current != "on",
                Some("1") => true,
                Some("0") => false,
                Some(value)
                    if value.eq_ignore_ascii_case("on") || value.eq_ignore_ascii_case("yes") =>
                {
                    true
                }
                Some(value)
                    if value.eq_ignore_ascii_case("off") || value.eq_ignore_ascii_case("no") =>
                {
                    false
                }
                Some(value) => {
                    return Err(ServerError::InvalidCommand(format!("bad value: {value}")));
                }
            };
            Ok(tmux_flag(enabled).to_owned())
        }
        TmuxStoredScalarKind::Choice(choices) => {
            if let Some(value) = value {
                return choices
                    .contains(&value)
                    .then(|| value.to_owned())
                    .ok_or_else(|| ServerError::InvalidCommand(format!("unknown value: {value}")));
            }
            Ok(match choices.iter().position(|choice| *choice == current) {
                Some(0) => choices[1].to_owned(),
                Some(1) => choices[0].to_owned(),
                _ => current.to_owned(),
            })
        }
        TmuxStoredScalarKind::Key => {
            let value =
                value.ok_or_else(|| ServerError::InvalidCommand("empty value".to_owned()))?;
            parse_tmux_key(value)
                .ok_or_else(|| ServerError::InvalidCommand(format!("bad key: {value}")))
        }
    }
}

pub(super) fn first_free_array_index<'a>(
    keys: impl IntoIterator<Item = &'a ArrayIndex>,
) -> Result<u32, ServerError> {
    let mut next = 0_u32;
    for key in keys {
        match key {
            ArrayIndex::Numeric(index) if *index == next => {
                next = next
                    .checked_add(1)
                    .ok_or_else(|| ServerError::InvalidCommand("no free array index".to_owned()))?;
            }
            ArrayIndex::Numeric(index) if *index < next => {}
            _ => break,
        }
    }
    Ok(next)
}

pub(super) fn default_array(name: &'static str) -> StringArray {
    tmux_stored_array(name)
        .expect("stored array metadata")
        .defaults
        .iter()
        .enumerate()
        .map(|(index, value)| {
            (
                ArrayIndex::Numeric(u32::try_from(index).expect("array default index fits u32")),
                (*value).to_owned(),
            )
        })
        .collect()
}

pub(super) fn default_array_table(scope: TmuxOptionScope) -> ArrayTable {
    tmux_options()
        .filter(|option| option.scope == scope && tmux_stored_array(option.name).is_some())
        .map(|option| (option.name, default_array(option.name)))
        .collect()
}

fn validate_array_value(kind: TmuxArrayValue, value: &str) -> Result<(), ServerError> {
    if kind == TmuxArrayValue::Colour
        && parse_tmux_colour(value).is_none()
        && zz_terminal::parse_x11_color(value).is_none()
    {
        return Err(ServerError::InvalidCommand(format!("bad colour: {value}")));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ArrayIndex {
    Numeric(u32),
    Named(String),
}

impl ArrayIndex {
    pub(super) fn parse(value: String) -> Self {
        value
            .parse()
            .map_or_else(|_| Self::Named(value), Self::Numeric)
    }

    pub(super) fn display(&self) -> String {
        match self {
            Self::Numeric(index) => index.to_string(),
            Self::Named(index) => index.clone(),
        }
    }
}

pub(super) type StringArray = BTreeMap<ArrayIndex, String>;

type ArrayTable = BTreeMap<&'static str, StringArray>;

pub(super) type HookArray = BTreeMap<ArrayIndex, Vec<CommandInvocation>>;

pub(super) type HookTable = BTreeMap<String, HookArray>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StoredArrays {
    pub(super) server: ArrayTable,
    pub(super) global_session: ArrayTable,
    pub(super) sessions: BTreeMap<SessionId, ArrayTable>,
    global_window: ArrayTable,
    windows: BTreeMap<WindowId, ArrayTable>,
    panes: BTreeMap<PaneId, ArrayTable>,
}

pub(super) type ScalarTable = BTreeMap<&'static str, String>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct StoredScalars {
    server: ScalarTable,
    pub(super) global_session: ScalarTable,
    pub(super) sessions: BTreeMap<SessionId, ScalarTable>,
    pub(super) global_window: ScalarTable,
    pub(super) windows: BTreeMap<WindowId, ScalarTable>,
    pub(super) panes: BTreeMap<PaneId, ScalarTable>,
}

impl Default for StoredArrays {
    fn default() -> Self {
        Self {
            server: default_array_table(TmuxOptionScope::Server),
            global_session: default_array_table(TmuxOptionScope::Session),
            sessions: BTreeMap::new(),
            global_window: default_array_table(TmuxOptionScope::WindowPane),
            windows: BTreeMap::new(),
            panes: BTreeMap::new(),
        }
    }
}
