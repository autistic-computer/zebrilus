use super::*;

impl MuxEngine {
    #[must_use]
    pub fn is_hook_event(name: &str) -> bool {
        name.starts_with('@') || tmux_option_is_hook(name)
    }

    #[must_use]
    pub fn pane_created_payload(&self, pane: PaneId) -> (Option<String>, Option<String>) {
        let command = self
            .pane_start_command(pane)
            .map(|command| {
                command
                    .iter()
                    .map(|argument| crate::formats::quote_argument(argument))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|command| !command.is_empty())
            .or_else(|| self.pane_shells.get(&pane).cloned())
            .or_else(|| {
                let window = self.state.window_for_pane(pane)?;
                let session = self.state.windows.get(&window)?.session;
                self.default_shell_for_session(session)
                    .ok()
                    .map(str::to_owned)
            });
        let path = self
            .pane_runtime_facts(pane)
            .map(|facts| facts.start_path.clone())
            .filter(|path| !path.is_empty());
        (command, path)
    }

    #[must_use]
    pub fn after_command_hook(command: &str) -> Option<&'static str> {
        let command = if command == "new-pane" {
            "split-window"
        } else {
            command
        };
        HOOK_NAMES
            .binary_search_by(|hook| {
                hook.strip_prefix("after-")
                    .map_or(Ordering::Greater, |name| name.cmp(command))
            })
            .ok()
            .map(|index| HOOK_NAMES[index])
    }

    fn effective_hook(&self, session: Option<SessionId>, name: &str) -> Option<&HookArray> {
        session
            .and_then(|session| self.session_hooks.get(&session))
            .and_then(|hooks| hooks.get(name))
            .or_else(|| self.global_hooks.get(name))
    }

    pub fn note_hook_fired(&mut self, session: Option<SessionId>, name: &str, now: u64) {
        if !tmux_option_is_hook(name) {
            return;
        }
        let target = session
            .filter(|session| {
                self.session_hooks
                    .get(session)
                    .is_some_and(|hooks| hooks.contains_key(name))
            })
            .map_or(TmuxOptionTarget::GlobalSession, TmuxOptionTarget::Session);
        self.count_hook_fire(Some(target), name, now);
    }

    pub fn note_event_hook_fired(&mut self, context: &ExecutionContext, name: &str, now: u64) {
        if name.starts_with('@') {
            self.note_user_hook_fired(context, name, now);
            return;
        }
        let Some(option) = match_tmux_option(name).ok().flatten() else {
            return;
        };
        if !tmux_option_is_hook(option.name) {
            return;
        }
        if option.scope == TmuxOptionScope::Session {
            self.note_hook_fired(context.session, option.name, now);
            return;
        }
        let window = context
            .pane
            .and_then(|pane| self.state.window_for_pane(pane))
            .or(context.window);
        let has =
            |hooks: Option<&HookTable>| hooks.is_some_and(|hooks| hooks.contains_key(option.name));
        let target = context
            .pane
            .filter(|pane| {
                option.scope == TmuxOptionScope::WindowPane && has(self.pane_hooks.get(pane))
            })
            .map(TmuxOptionTarget::Pane)
            .or_else(|| {
                window
                    .filter(|window| has(self.window_hooks.get(window)))
                    .map(TmuxOptionTarget::Window)
            })
            .unwrap_or(TmuxOptionTarget::GlobalWindow);
        self.count_hook_fire(Some(target), option.name, now);
    }

    fn note_user_hook_fired(&mut self, context: &ExecutionContext, name: &str, now: u64) {
        let window = context
            .pane
            .and_then(|pane| self.state.window_for_pane(pane))
            .or(context.window);
        let session_chain = context
            .session
            .map(TmuxOptionTarget::Session)
            .into_iter()
            .chain([TmuxOptionTarget::GlobalSession]);
        let pane_chain = context
            .pane
            .map(TmuxOptionTarget::Pane)
            .into_iter()
            .chain(window.map(TmuxOptionTarget::Window))
            .chain([TmuxOptionTarget::GlobalWindow]);
        let target = session_chain
            .chain(pane_chain)
            .find(|target| self.user_option_at_target(*target, name).is_some());
        self.count_hook_fire(target, name, now);
    }

    pub(super) fn user_option_owner(
        &self,
        target: TmuxOptionTarget,
        name: &str,
    ) -> TmuxOptionTarget {
        let window = match target {
            TmuxOptionTarget::Pane(pane) => self.state.window_for_pane(pane),
            TmuxOptionTarget::Window(window) => Some(window),
            _ => None,
        };
        let parents = match target {
            TmuxOptionTarget::Session(_) => vec![TmuxOptionTarget::GlobalSession],
            TmuxOptionTarget::Pane(_) | TmuxOptionTarget::Window(_) => window
                .filter(|_| matches!(target, TmuxOptionTarget::Pane(_)))
                .map(TmuxOptionTarget::Window)
                .into_iter()
                .chain([TmuxOptionTarget::GlobalWindow])
                .collect(),
            _ => Vec::new(),
        };
        std::iter::once(target)
            .chain(parents)
            .find(|owner| self.user_option_at_target(*owner, name).is_some())
            .unwrap_or(target)
    }

    pub(super) fn forget_hook_fires(&mut self, target: TmuxOptionTarget, name: &str) {
        self.hook_fires.remove(&(target, name.to_owned()));
    }

    pub(super) fn count_hook_fire(
        &mut self,
        target: Option<TmuxOptionTarget>,
        name: &str,
        now: u64,
    ) {
        if let Some(target) = target {
            self.hook_fires
                .entry((target, name.to_owned()))
                .or_default()
                .fired(now);
        }
    }

    pub(super) fn hook_fire(&self, target: TmuxOptionTarget, name: &str) -> HookFire {
        self.hook_fires
            .get(&(target, name.to_owned()))
            .copied()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn has_hook_commands(&self, session: Option<SessionId>, name: &str) -> bool {
        self.effective_hook(session, name)
            .is_some_and(|hooks| !hooks.is_empty())
    }

    #[must_use]
    pub fn has_hook_commands_in_any_session(&self, name: &str) -> bool {
        self.global_hooks
            .get(name)
            .into_iter()
            .chain(
                self.session_hooks
                    .values()
                    .filter_map(|hooks| hooks.get(name)),
            )
            .any(|hooks| !hooks.is_empty())
    }

    #[must_use]
    pub fn hook_commands(
        &self,
        session: Option<SessionId>,
        name: &str,
    ) -> Option<Vec<Vec<CommandInvocation>>> {
        self.effective_hook(session, name)
            .map(|hooks| hooks.values().cloned().collect())
    }

    #[must_use]
    pub fn event_hook_commands(
        &self,
        context: &ExecutionContext,
        name: &str,
    ) -> Option<Vec<Vec<CommandInvocation>>> {
        let option = match_tmux_option(name).ok().flatten()?;
        if !tmux_option_is_hook(option.name) {
            return None;
        }
        if option.scope == TmuxOptionScope::Session {
            return self.hook_commands(context.session, option.name);
        }
        let window = context
            .pane
            .and_then(|pane| self.state.window_for_pane(pane))
            .or(context.window);
        let pane_hook = if option.scope == TmuxOptionScope::WindowPane {
            context
                .pane
                .and_then(|pane| self.pane_hooks.get(&pane))
                .and_then(|hooks| hooks.get(option.name))
        } else {
            None
        };
        pane_hook
            .or_else(|| {
                window
                    .and_then(|window| self.window_hooks.get(&window))
                    .and_then(|hooks| hooks.get(option.name))
            })
            .or_else(|| self.global_window_hooks.get(option.name))
            .map(|hooks| hooks.values().cloned().collect())
    }

    pub(super) fn marked_pane_changed(
        &mut self,
        context: &ExecutionContext,
        start: PaneId,
        old: Option<PaneId>,
    ) -> Execution {
        let mut execution = Execution::default();
        if context.no_hooks {
            return execution;
        }
        let marked = self.state.marked_pane();
        let pane = marked.or(old).unwrap_or(start);
        let Some(mut hook_context) = ExecutionContext::for_pane(&self.state, pane) else {
            return execution;
        };
        let commands = self
            .event_hook_commands(&hook_context, "marked-pane-changed")
            .unwrap_or_default();
        let mut variables = BTreeMap::from([
            ("hook_event".to_owned(), "marked-pane-changed".to_owned()),
            (HOOK_PANE_CONTEXT_FORMAT.to_owned(), pane.to_string()),
            (
                "hook_marked".to_owned(),
                u8::from(marked.is_some()).to_string(),
            ),
        ]);
        if let Some(window) = self.state.window_for_pane(pane) {
            variables.insert(HOOK_WINDOW_CONTEXT_FORMAT.to_owned(), window.to_string());
            if let Some(state) = self.state.windows.get(&window) {
                variables.insert(
                    HOOK_WINDOW_NAME_CONTEXT_FORMAT.to_owned(),
                    state.name.clone(),
                );
            }
        }
        if let Some(marked) = marked {
            variables.insert("hook_new_pane".to_owned(), marked.to_string());
        }
        if let Some(old) = old {
            variables.insert("hook_old_pane".to_owned(), old.to_string());
        }
        hook_context.format_variables = variables;
        execution.effects.push(MuxEffect::RunHook {
            name: "marked-pane-changed".to_owned(),
            commands,
            context: hook_context,
        });
        execution
    }

    pub(super) fn user_hook_commands(
        &self,
        context: &ExecutionContext,
        name: &str,
    ) -> Option<Vec<Vec<CommandInvocation>>> {
        let session = context
            .session
            .and_then(|session| {
                self.user_option_readback(TmuxOptionTarget::Session(session), name, true)
            })
            .map(|(value, _)| value);
        let pane = context
            .pane
            .and_then(|pane| self.user_option_readback(TmuxOptionTarget::Pane(pane), name, true))
            .map(|(value, _)| value);
        let window = context
            .pane
            .and_then(|pane| self.state.window_for_pane(pane))
            .or(context.window)
            .and_then(|window| {
                self.user_option_readback(TmuxOptionTarget::Window(window), name, true)
            })
            .map(|(value, _)| value);
        let commands = parse_hook_commands(self, session.or(pane).or(window)?).ok()?;
        Some(vec![commands])
    }

    #[must_use]
    pub fn has_format_monitors(&self) -> bool {
        !self.format_monitors.is_empty()
    }

    pub(super) fn set_hook(
        &mut self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
        hooks: &mut impl StatusHooks,
        default_shell_is_valid: &mut impl FnMut(&str) -> bool,
    ) -> Result<Execution, ServerError> {
        let spec = command_spec("set-hook").expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = invocation
            .args
            .len()
            .saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options("set-hook", &invocation.args)?;
        if options.has("-E") {
            return self.set_hook_event(context, &options, &positional, hooks);
        }
        if let Some(subscription) = options.value("-B") {
            let subscription = subscription.to_owned();
            if positional.len() > 1 {
                return Err(ServerError::CommandParse("too many arguments".to_owned()));
            }
            let body = positional
                .first()
                .map(|value| {
                    if invocation.argument_is_command_block(positional_start) {
                        normalize_typed_command_block(
                            self,
                            crate::parser::command_block_body(value).unwrap_or(value),
                        )
                    } else {
                        Ok(value.to_string())
                    }
                })
                .transpose()?;
            return self.set_hook_monitor(context, &options, body.as_deref(), &subscription);
        }
        let Some(argument) = positional.first() else {
            return Err(ServerError::CommandParse("missing argument".to_owned()));
        };
        if positional.len() > 2 {
            return Err(ServerError::CommandParse("too many arguments".to_owned()));
        }
        let (argument, target_context) =
            self.expand_hook_name(context, &options, argument, hooks)?;
        let parsed = parse_tmux_option(&argument)
            .map_err(|()| ServerError::InvalidCommand(format!("invalid option: {argument}")))?;
        let value = positional
            .get(1)
            .map(|value| {
                if invocation.argument_is_command_block(positional_start + 1) {
                    normalize_typed_command_block(
                        self,
                        crate::parser::command_block_body(value).unwrap_or(value),
                    )
                } else {
                    Ok(value.to_string())
                }
            })
            .transpose()?;
        let value = value.as_deref();
        if parsed.name.starts_with('@') {
            if options.has("-R") {
                let Some(commands) = self.user_hook_commands(&target_context, &argument) else {
                    self.note_event_hook_fired(&target_context, &argument, unix_seconds());
                    return Ok(Execution::default());
                };
                return Ok(Execution::effect(MuxEffect::RunHook {
                    name: argument,
                    commands,
                    context: target_context,
                }));
            }
            if parsed.index.is_some() {
                return Err(ServerError::InvalidCommand(format!(
                    "not an array: {argument}"
                )));
            }
            let value = value.map(RawText::from);
            let execution =
                self.set_user_option(context, parsed.name, value.as_ref(), &options, false)?;
            if !options.has("-u")
                && !options.has("-U")
                && execution
                    .effects
                    .iter()
                    .any(|effect| matches!(effect, MuxEffect::UserOptionChanged { .. }))
            {
                self.hook_events.insert(parsed.name.to_owned());
            }
            return Ok(execution);
        }
        let table_option = match match_tmux_option(parsed.name) {
            Ok(Some(option)) => option,
            Ok(None) | Err(()) if options.has("-R") => return Ok(Execution::default()),
            Ok(None) => {
                return Err(ServerError::InvalidCommand(format!(
                    "invalid option: {argument}"
                )));
            }
            Err(()) => {
                return Err(ServerError::InvalidCommand(format!(
                    "ambiguous option: {argument}"
                )));
            }
        };
        if options.has("-R") {
            if !tmux_option_is_hook(table_option.name) {
                return Ok(Execution::default());
            }
            let Some(commands) = self
                .event_hook_commands(&target_context, table_option.name)
                .filter(|commands| !commands.is_empty())
            else {
                self.note_event_hook_fired(&target_context, table_option.name, unix_seconds());
                return Ok(Execution::default());
            };
            return Ok(Execution::effect(MuxEffect::RunHook {
                name: table_option.name.to_owned(),
                commands,
                context: target_context,
            }));
        }
        if !tmux_option_is_hook(table_option.name) {
            let mut forwarded = Vec::new();
            for flag in ["-a", "-g", "-p", "-u", "-w"] {
                if options.has(flag) {
                    forwarded.push(flag.to_owned());
                }
            }
            if let Some(target) = options.value("-t") {
                forwarded.extend(["-t".to_owned(), target.to_owned()]);
            }
            forwarded.extend(["--".to_owned(), argument]);
            if let Some(value) = value {
                forwarded.push(value.to_owned());
            }
            let forwarded = CommandInvocation::new("set-option", forwarded);
            return self.set_option(context, &forwarded, false, hooks, default_shell_is_valid);
        }
        let target =
            self.resolve_tmux_option_target(context, &options, false, table_option.scope)?;
        self.set_hook_array_option(target, table_option.name, parsed.index, value, &options)
    }

    fn set_hook_event(
        &self,
        context: &ExecutionContext,
        options: &Options,
        positional: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let Some(argument) = positional.first() else {
            return Err(ServerError::InvalidCommand("missing argument".to_owned()));
        };
        if positional.len() != 1 {
            return Err(ServerError::InvalidCommand("too many arguments".to_owned()));
        }
        let (name, target_context) = self.expand_hook_name(context, options, argument, hooks)?;
        if !name.starts_with('@') {
            return Err(ServerError::InvalidCommand(
                "event name must start with @".to_owned(),
            ));
        }
        let commands = self
            .hook_events
            .contains(&name)
            .then(|| self.user_hook_commands(&target_context, &name))
            .flatten()
            .unwrap_or_default();
        let mut hook_context = target_context;
        let mut variables = BTreeMap::from([("hook_event".to_owned(), name.clone())]);
        if let Some(session) = hook_context.session {
            variables.insert(HOOK_SESSION_CONTEXT_FORMAT.to_owned(), session.to_string());
            if let Some(state) = self.state.sessions.get(&session) {
                variables.insert(
                    HOOK_SESSION_NAME_CONTEXT_FORMAT.to_owned(),
                    state.name.clone(),
                );
            }
        }
        if let Some(window) = hook_context.window
            && let Some(state) = self.state.windows.get(&window)
        {
            variables.insert(HOOK_WINDOW_CONTEXT_FORMAT.to_owned(), window.to_string());
            variables.insert(
                HOOK_WINDOW_NAME_CONTEXT_FORMAT.to_owned(),
                state.name.clone(),
            );
            variables.insert(
                HOOK_WINDOW_INDEX_CONTEXT_FORMAT.to_owned(),
                state.index.to_string(),
            );
        }
        if let Some(pane) = hook_context.pane {
            variables.insert(HOOK_PANE_CONTEXT_FORMAT.to_owned(), pane.to_string());
        }
        hook_context.format_variables = variables;
        Ok(Execution::effect(MuxEffect::RunHook {
            name,
            commands,
            context: hook_context,
        }))
    }

    fn set_hook_monitor(
        &mut self,
        context: &ExecutionContext,
        options: &Options,
        body: Option<&str>,
        subscription: &str,
    ) -> Result<Execution, ServerError> {
        let unset = options.has("-u");
        let (name, scope, format) = match parse_format_monitor(subscription) {
            Some(parsed) => parsed,
            None if unset => (
                subscription.to_owned(),
                FormatMonitorScope::Session,
                String::new(),
            ),
            None => {
                return Err(ServerError::InvalidCommand(format!(
                    "invalid subscription: {subscription}"
                )));
            }
        };
        if !name.starts_with('@') {
            return Err(ServerError::InvalidCommand(
                "monitor hook name must start with @".to_owned(),
            ));
        }
        let target = self.resolve_user_option_target(context, options, false)?;
        if unset {
            self.format_monitors
                .retain(|monitor| monitor.target != target || monitor.name != name);
            return Ok(Execution::default());
        }
        let mut execution = Execution::default();
        if let Some(body) = body {
            let already = self.user_option_at_target(target, &name).is_some();
            if !options.has("-o") || !already {
                let value = if options.has("-a") {
                    let mut value = self
                        .user_option_at_target(target, &name)
                        .cloned()
                        .unwrap_or_default();
                    value.push_bytes(body.as_bytes());
                    value
                } else {
                    RawText::from(body)
                };
                self.user_options_at_target_mut(target)
                    .insert(name.clone(), value);
                execution = self.user_option_changed(context, target, &name);
            }
        } else if self.user_option_at_target(target, &name).is_none() {
            self.user_options_at_target_mut(target)
                .insert(name.clone(), RawText::default());
        }
        self.format_monitors
            .retain(|monitor| monitor.target != target || monitor.name != name);
        let id = self.next_format_monitor_id;
        self.next_format_monitor_id += 1;
        self.format_monitors.push(FormatMonitorEntry {
            id,
            name,
            target,
            scope,
            format,
            session: context.session,
            notify_true: options.has("-T"),
            previous: BTreeMap::new(),
            fire: HookFire::default(),
        });
        Ok(execution)
    }

    fn show_hooks_monitor(
        &self,
        context: &ExecutionContext,
        options: &Options,
        argument: Option<&str>,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let target = self.hook_listing_target(context, options)?;
        let mut monitors = self
            .format_monitors
            .iter()
            .filter(|monitor| monitor.target == target)
            .collect::<Vec<_>>();
        monitors.sort_by(|left, right| left.name.cmp(&right.name));
        let Some(argument) = argument else {
            let lines = monitors
                .into_iter()
                .map(|monitor| self.shown_monitor(context, options, monitor, hooks))
                .collect::<Vec<_>>();
            return Ok(Execution::output(printed_lines(&lines)));
        };
        let parsed = parse_tmux_option(argument)
            .map_err(|()| ServerError::InvalidCommand(format!("invalid option: {argument}")))?;
        if let Some(monitor) = monitors
            .into_iter()
            .find(|monitor| monitor.name == parsed.name)
        {
            return Ok(Execution::output(printed_lines(&[
                self.shown_monitor(context, options, monitor, hooks)
            ])));
        }
        if parsed.name.starts_with('@') && self.user_option_at_target(target, parsed.name).is_none()
        {
            return Err(ServerError::InvalidCommand(format!(
                "invalid option: {argument}"
            )));
        }
        Ok(Execution::default())
    }

    fn shown_monitor(
        &self,
        context: &ExecutionContext,
        options: &Options,
        monitor: &FormatMonitorEntry,
        hooks: &mut impl StatusHooks,
    ) -> String {
        let value = format_monitor_display(&monitor.name, monitor.scope, &monitor.format);
        let Some(template) = options.value("-F") else {
            return value;
        };
        let target = match monitor.scope {
            FormatMonitorScope::Session => String::new(),
            FormatMonitorScope::Pane(pane) => format!("%{}", pane.0),
            FormatMonitorScope::AllPanes => "%*".to_owned(),
            FormatMonitorScope::Window(window) => format!("@{}", window.0),
            FormatMonitorScope::AllWindows => "@*".to_owned(),
        };
        let values = [
            ("option_name", monitor.name.clone()),
            ("option_value", value),
            ("option_value_only", "0".to_owned()),
            ("option_is_parent", "0".to_owned()),
            ("option_is_array", "0".to_owned()),
            ("option_is_string", "1".to_owned()),
            ("option_is_hook", "1".to_owned()),
            ("option_is_user", "1".to_owned()),
            ("option_has_value", "1".to_owned()),
            ("option_array_key", String::new()),
            ("option_has_array_key", "0".to_owned()),
            ("hook_monitor_target", target),
            ("hook_monitor_format", monitor.format.clone()),
        ]
        .into_iter()
        .chain(monitor.fire.context())
        .collect::<Vec<_>>();
        let (format_target, client) = self.show_options_format_target(context, options, false);
        let mut row_hooks = ShownOptionHooks {
            inner: hooks,
            values: &values,
        };
        self.expand_pane_format(
            template,
            &format_target,
            context.session,
            client,
            &mut row_hooks,
        )
    }

    #[must_use]
    pub fn format_monitors(&self) -> Vec<FormatMonitor> {
        self.format_monitors
            .iter()
            .map(|monitor| FormatMonitor {
                id: monitor.id,
                name: monitor.name.clone(),
                scope: monitor.scope,
                format: monitor.format.clone(),
                session: monitor.session,
            })
            .collect()
    }

    pub fn record_format_monitor_sample(
        &mut self,
        id: u64,
        target: FormatMonitorTarget,
        value: &str,
    ) -> Option<String> {
        let monitor = self
            .format_monitors
            .iter_mut()
            .find(|monitor| monitor.id == id)?;
        match monitor.previous.insert(target, value.to_owned()) {
            Some(last) if last != value && (!monitor.notify_true || format_true(value)) => {
                let now = self.state.format_now();
                monitor.fire.fired(now);
                let owner = (monitor.target, monitor.name.clone());
                if self.user_option_at_target(owner.0, &owner.1).is_some() {
                    self.hook_fires.entry(owner).or_default().fired(now);
                }
                Some(last)
            }
            Some(_) | None => None,
        }
    }

    pub fn sweep_format_monitor_targets(&mut self, id: u64, seen: &BTreeSet<FormatMonitorTarget>) {
        let Some(monitor) = self
            .format_monitors
            .iter_mut()
            .find(|monitor| monitor.id == id)
        else {
            return;
        };
        if !matches!(
            monitor.scope,
            FormatMonitorScope::AllPanes | FormatMonitorScope::AllWindows
        ) {
            return;
        }
        monitor.previous.retain(|target, _| seen.contains(target));
    }

    #[must_use]
    pub fn format_monitor_hook_body(&self, id: u64) -> Option<String> {
        let monitor = self
            .format_monitors
            .iter()
            .find(|monitor| monitor.id == id)?;
        let value = self.user_option_at_target(monitor.target, &monitor.name)?;
        if value.is_empty() {
            return None;
        }
        Some(value.to_string())
    }

    #[must_use]
    pub fn parse_monitor_hook_body(&self, body: &str) -> Option<Vec<Vec<CommandInvocation>>> {
        if body.is_empty() {
            return None;
        }
        let commands = parse_hook_commands(self, body).ok()?;
        Some(vec![commands])
    }

    #[must_use]
    pub fn format_monitor_hook_variables(
        &self,
        name: &str,
        value: &str,
        last: &str,
        target: FormatMonitorTarget,
    ) -> BTreeMap<String, String> {
        let mut variables = BTreeMap::from([
            (HOOK_CONTEXT_FORMAT.to_owned(), name.to_owned()),
            (HOOK_VALUE_CONTEXT_FORMAT.to_owned(), value.to_owned()),
            (HOOK_LAST_CONTEXT_FORMAT.to_owned(), last.to_owned()),
        ]);
        if let Some(session) = target.session {
            variables.insert(HOOK_SESSION_CONTEXT_FORMAT.to_owned(), session.to_string());
            if let Some(state) = self.state.sessions.get(&session) {
                variables.insert(
                    HOOK_SESSION_NAME_CONTEXT_FORMAT.to_owned(),
                    state.name.clone(),
                );
            }
        }
        if let Some(window) = target.window {
            variables.insert(HOOK_WINDOW_CONTEXT_FORMAT.to_owned(), window.to_string());
            if let Some(state) = self.state.windows.get(&window) {
                variables.insert(
                    HOOK_WINDOW_NAME_CONTEXT_FORMAT.to_owned(),
                    state.name.clone(),
                );
                variables.insert(
                    HOOK_WINDOW_INDEX_CONTEXT_FORMAT.to_owned(),
                    state.index.to_string(),
                );
            }
        }
        if let Some(pane) = target.pane {
            variables.insert(HOOK_PANE_CONTEXT_FORMAT.to_owned(), pane.to_string());
        }
        variables
    }

    pub(super) fn set_hook_array_option(
        &mut self,
        target: TmuxOptionTarget,
        name: &'static str,
        index: Option<String>,
        value: Option<&str>,
        options: &Options,
    ) -> Result<Execution, ServerError> {
        self.format_options_generation = self.format_options_generation.wrapping_add(1);
        let index = index.map(ArrayIndex::parse);
        let unset = option_is_unset(options);
        let already = self
            .hook_array(target, name)
            .is_some_and(|hook| index.as_ref().is_none_or(|index| hook.contains_key(index)));
        if options.has("-o") && !unset && already {
            let displayed_index = index.as_ref().map(ArrayIndex::display);
            let option = indexed_option_name(name, displayed_index.as_deref());
            return already_set_or_quiet(options, &option);
        }
        if options.has("-U")
            && let TmuxOptionTarget::Window(window) = target
        {
            self.remove_pane_hook_overrides(window, name, index.as_ref());
        }
        if unset {
            self.unset_hook_array(target, name, index.as_ref());
            return Ok(Execution::default());
        }
        let value = value.ok_or_else(|| ServerError::InvalidCommand("empty value".to_owned()))?;
        self.hook_array_mut_or_insert(target, name);
        if index.is_none() && !options.has("-a") {
            self.hook_array_mut_or_insert(target, name).clear();
        }
        if index.is_none() && value.is_empty() {
            return Ok(Execution::default());
        }
        let commands = parse_hook_commands(self, value)?;
        let hook = self.hook_array_mut_or_insert(target, name);
        if let Some(index) = index {
            hook.insert(index, commands);
        } else if options.has("-a") {
            let index = first_free_array_index(hook.keys())?;
            hook.insert(ArrayIndex::Numeric(index), commands);
        } else {
            hook.insert(ArrayIndex::Numeric(0), commands);
        }
        Ok(Execution::default())
    }

    fn hook_listing_target(
        &self,
        context: &ExecutionContext,
        options: &Options,
    ) -> Result<TmuxOptionTarget, ServerError> {
        if options.has("-p") {
            return self
                .resolve_pane(options.value("-t"), context.window, context.pane)
                .map(TmuxOptionTarget::Pane);
        }
        if options.has("-w") {
            if options.has("-g") {
                return Ok(TmuxOptionTarget::GlobalWindow);
            }
            return self
                .resolve_option_window(context, options, false)
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

    pub(super) fn hook_names_for_target(target: TmuxOptionTarget) -> Vec<&'static str> {
        let mut names = tmux_options()
            .filter(|option| tmux_option_is_hook(option.name))
            .filter(|option| match target {
                TmuxOptionTarget::GlobalSession | TmuxOptionTarget::Session(_) => {
                    option.scope == TmuxOptionScope::Session
                }
                TmuxOptionTarget::GlobalWindow | TmuxOptionTarget::Window(_) => matches!(
                    option.scope,
                    TmuxOptionScope::Window | TmuxOptionScope::WindowPane
                ),
                TmuxOptionTarget::Pane(_) => option.scope == TmuxOptionScope::WindowPane,
                TmuxOptionTarget::Server => false,
            })
            .map(|option| option.name)
            .collect::<Vec<_>>();
        names.sort_unstable_by_key(|name| {
            HOOK_NAMES
                .iter()
                .position(|hook| hook == name)
                .expect("hook table entry")
        });
        names
    }

    pub(super) fn show_hooks(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("show-hooks", args)?;
        if options.has("-B") {
            if positional.len() > 1 {
                return Err(ServerError::CommandParse("too many arguments".to_owned()));
            }
            return self.show_hooks_monitor(
                context,
                &options,
                positional.first().map(RawText::as_str),
                hooks,
            );
        }
        if positional.len() > 1 {
            return Err(ServerError::CommandParse("too many arguments".to_owned()));
        }
        let Some(argument) = positional.first() else {
            let target = self.hook_listing_target(context, &options)?;
            let mut lines = ShownOptions::default();
            if let Some(stored) = self.hook_table(target) {
                for name in Self::hook_names_for_target(target) {
                    if let Some(hook) = stored.get(name) {
                        push_shown_hook(&mut lines, name, hook, None);
                    }
                }
            }
            self.stamp_hook_fires(target, &mut lines);
            return Ok(self.shown_options_execution(context, &options, false, &lines, hooks));
        };
        let (argument, _) = self.expand_hook_name(context, &options, argument, hooks)?;
        let parsed = parse_tmux_option(&argument)
            .map_err(|()| ServerError::InvalidCommand(format!("invalid option: {argument}")))?;
        if parsed.name.starts_with('@') {
            if parsed.index.is_some() {
                return Err(ServerError::InvalidCommand(format!(
                    "not an array: {argument}"
                )));
            }
            let mut forwarded: Vec<RawText> = Vec::new();
            for flag in ["-g", "-p", "-w"] {
                if options.has(flag) {
                    forwarded.push(flag.into());
                }
            }
            if let Some(target) = options.value("-t") {
                forwarded.extend(["-t".into(), target.into()]);
            }
            if let Some(format) = options.value("-F") {
                forwarded.extend(["-F".into(), format.into()]);
            }
            forwarded.extend(["--".into(), argument.into()]);
            return self.show_options(
                context,
                &forwarded,
                false,
                ShowOptionArgument::HookAlreadyExpanded,
                hooks,
            );
        }
        let table_option = match match_tmux_option(parsed.name) {
            Ok(Some(option)) => option,
            Ok(None) => {
                return Err(ServerError::InvalidCommand(format!(
                    "invalid option: {argument}"
                )));
            }
            Err(()) => {
                return Err(ServerError::InvalidCommand(format!(
                    "ambiguous option: {argument}"
                )));
            }
        };
        if !tmux_option_is_hook(table_option.name) {
            let mut forwarded: Vec<RawText> = Vec::new();
            for flag in ["-g", "-p", "-w"] {
                if options.has(flag) {
                    forwarded.push(flag.into());
                }
            }
            if let Some(target) = options.value("-t") {
                forwarded.extend(["-t".into(), target.into()]);
            }
            if let Some(format) = options.value("-F") {
                forwarded.extend(["-F".into(), format.into()]);
            }
            forwarded.extend(["--".into(), argument.into()]);
            return self.show_options(
                context,
                &forwarded,
                false,
                ShowOptionArgument::HookAlreadyExpanded,
                hooks,
            );
        }
        let target =
            self.resolve_tmux_option_target(context, &options, false, table_option.scope)?;
        let hook = self.hook_array(target, table_option.name);
        let Some(hook) = hook else {
            return Ok(Execution::default());
        };
        let mut lines = ShownOptions::default();
        let index = parsed.index.map(ArrayIndex::parse);
        push_shown_hook(&mut lines, table_option.name, hook, index.as_ref());
        self.stamp_hook_fires(target, &mut lines);
        Ok(self.shown_options_execution(context, &options, false, &lines, hooks))
    }

    pub(super) fn stamp_hook_fires(&self, target: TmuxOptionTarget, lines: &mut ShownOptions) {
        for row in &mut lines.rows {
            row.fire = Some(self.hook_fire(target, &row.name));
        }
    }

    fn expand_hook_name(
        &self,
        context: &ExecutionContext,
        options: &Options,
        argument: &str,
        hooks: &mut impl StatusHooks,
    ) -> Result<(String, ExecutionContext), ServerError> {
        let target = self.hook_target_context(context, options)?;
        let argument = expand_format_with_hooks(
            argument,
            self,
            FormatContext {
                session: target.session,
                window: target.window,
                pane: target.pane,
                active_session: context.session,
                format_client: context.target_format_client(),
                format_type: FormatType::Pane,
            },
            hooks,
        );
        Ok((argument.to_string(), target))
    }

    fn hook_target_context(
        &self,
        context: &ExecutionContext,
        options: &Options,
    ) -> Result<ExecutionContext, ServerError> {
        let pane = match options.value("-t") {
            Some(target) => Some(self.resolve_pane(Some(target), context.window, context.pane)?),
            None => context
                .pane
                .filter(|pane| self.state.pane(*pane).is_some())
                .or_else(|| self.resolve_pane(None, context.window, context.pane).ok()),
        };
        let control_command_target = context.control_command_target();
        let mut target = pane
            .and_then(|pane| ExecutionContext::for_pane(&self.state, pane))
            .unwrap_or_else(|| context.clone());
        target.set_control_command_target(control_command_target);
        Ok(target)
    }
}

fn scanned_object_id(text: &str) -> Option<u64> {
    let digits = text
        .char_indices()
        .take_while(|(_, character)| character.is_ascii_digit())
        .count();
    if digits == 0 {
        return None;
    }
    text[..digits].parse().ok()
}

fn format_monitor_scope(what: &str) -> FormatMonitorScope {
    if what == "%*" {
        return FormatMonitorScope::AllPanes;
    }
    if let Some(rest) = what.strip_prefix('%')
        && let Some(id) = scanned_object_id(rest)
    {
        return FormatMonitorScope::Pane(PaneId(id));
    }
    if what == "@*" {
        return FormatMonitorScope::AllWindows;
    }
    if let Some(rest) = what.strip_prefix('@')
        && let Some(id) = scanned_object_id(rest)
    {
        return FormatMonitorScope::Window(WindowId(id));
    }
    FormatMonitorScope::Session
}

pub(super) fn parse_format_monitor(value: &str) -> Option<(String, FormatMonitorScope, String)> {
    let mut fields = value.splitn(3, ':');
    let name = fields.next()?;
    let what = fields.next()?;
    let format = fields.next()?;
    Some((
        name.to_owned(),
        format_monitor_scope(what),
        format.to_owned(),
    ))
}

pub(super) fn format_monitor_display(
    name: &str,
    scope: FormatMonitorScope,
    format: &str,
) -> String {
    let what = match scope {
        FormatMonitorScope::Session => String::new(),
        FormatMonitorScope::Pane(pane) => format!("%{}", pane.0),
        FormatMonitorScope::AllPanes => "%*".to_owned(),
        FormatMonitorScope::Window(window) => format!("@{}", window.0),
        FormatMonitorScope::AllWindows => "@*".to_owned(),
    };
    format!("{name}:{what}:{format}")
}

pub(super) fn global_hook_table(scope: TmuxOptionScope) -> HookTable {
    tmux_options()
        .filter(|option| tmux_option_is_hook(option.name))
        .filter(|option| match scope {
            TmuxOptionScope::Session => option.scope == TmuxOptionScope::Session,
            TmuxOptionScope::Window => matches!(
                option.scope,
                TmuxOptionScope::Window | TmuxOptionScope::WindowPane
            ),
            TmuxOptionScope::Server | TmuxOptionScope::WindowPane => false,
        })
        .map(|option| (option.name.to_owned(), HookArray::new()))
        .collect()
}

fn push_shown_hook(
    lines: &mut ShownOptions,
    name: &str,
    hook: &HookArray,
    requested: Option<&ArrayIndex>,
) {
    push_shown_hook_option(lines, name, hook, requested, false, false, false);
}

pub(super) fn push_shown_hook_option(
    lines: &mut ShownOptions,
    name: &str,
    hook: &HookArray,
    requested: Option<&ArrayIndex>,
    inherited: bool,
    mark_inherited_empty: bool,
    value_only: bool,
) {
    if let Some(index) = requested {
        let value = hook.get(index).map_or_else(String::new, |commands| {
            commands
                .iter()
                .map(format_command)
                .collect::<Vec<_>>()
                .join(" ; ")
        });
        let name = indexed_option_name(name, Some(&index.display()));
        push_shown_option(lines, &name, &value, false, inherited, value_only);
        return;
    }
    if hook.is_empty() {
        lines.record(name, "");
        lines.row(name, "", false, inherited && mark_inherited_empty, false);
        if !value_only {
            lines.lines.push(
                if inherited && mark_inherited_empty {
                    format!("{name}*")
                } else {
                    name.to_owned()
                }
                .into(),
            );
        }
        return;
    }
    for (index, commands) in hook {
        let value = commands
            .iter()
            .map(format_command)
            .collect::<Vec<_>>()
            .join(" ; ");
        let name = indexed_option_name(name, Some(&index.display()));
        push_shown_option(lines, &name, &value, false, inherited, value_only);
    }
}

#[must_use]
pub fn hook_format_variables(command: &CommandInvocation, hook: &str) -> BTreeMap<String, String> {
    let mut variables = BTreeMap::from([
        (HOOK_CONTEXT_FORMAT.to_owned(), hook.to_owned()),
        ("hook_event".to_owned(), hook.to_owned()),
        (
            HOOK_ARGUMENTS_CONTEXT_FORMAT.to_owned(),
            format_command_arguments(command),
        ),
    ]);
    let name = canonical_command(&command.name);
    let spec = catalog_command_spec(name);
    let (options, positional) = spec
        .and_then(|spec| parse_options_for_spec(&command.args, spec).ok())
        .unwrap_or_else(|| (Options::default(), command.args.clone()));
    for (index, argument) in positional.iter().enumerate() {
        variables.insert(
            format!("{HOOK_ARGUMENT_CONTEXT_PREFIX}{index}"),
            argument.to_string(),
        );
    }
    for flag in &options.flags {
        let flag = flag.trim_start_matches('-');
        variables.insert(format!("{HOOK_FLAG_CONTEXT_PREFIX}{flag}"), "1".to_owned());
    }
    let mut counts = BTreeMap::<&str, usize>::new();
    for (flag, value) in &options.values {
        let flag = flag.trim_start_matches('-');
        variables.insert(format!("{HOOK_FLAG_CONTEXT_PREFIX}{flag}"), value.clone());
        let index = counts.entry(flag).or_default();
        variables.insert(
            format!("{HOOK_FLAG_CONTEXT_PREFIX}{flag}_{index}"),
            value.clone(),
        );
        *index += 1;
    }
    variables
}
