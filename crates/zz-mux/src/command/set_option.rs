use super::*;

impl MuxEngine {
    fn mark_explicit_status_option(
        &mut self,
        session: Option<SessionId>,
        name: &'static str,
        explicit: bool,
    ) -> bool {
        match (session, explicit) {
            (None, true) => self.explicit_status_options.insert(name),
            (None, false) => self.explicit_status_options.remove(name),
            (Some(session), true) => self
                .session_explicit_status_options
                .entry(session)
                .or_default()
                .insert(name),
            (Some(session), false) => {
                let Some(options) = self.session_explicit_status_options.get_mut(&session) else {
                    return false;
                };
                let removed = options.remove(name);
                if options.is_empty() {
                    self.session_explicit_status_options.remove(&session);
                }
                removed
            }
        }
    }

    pub(super) fn status_format_execution(
        &mut self,
        target: TmuxOptionTarget,
        before: Option<&StringArray>,
        explicit: Option<bool>,
    ) -> Execution {
        let session = match target {
            TmuxOptionTarget::GlobalSession => None,
            TmuxOptionTarget::Session(session) => Some(session),
            _ => return Execution::default(),
        };
        let mut changed = self.array_option(target, "status-format") != before;
        if let Some(explicit) = explicit {
            changed |= self.mark_explicit_status_option(session, "status-format", explicit);
        }
        if changed {
            Execution::effect(MuxEffect::StatusFormatsChanged { session })
        } else {
            Execution::default()
        }
    }

    pub(super) fn set_option(
        &mut self,
        context: &ExecutionContext,
        invocation: &CommandInvocation,
        force_window: bool,
        hooks: &mut impl StatusHooks,
        default_shell_is_valid: &mut impl FnMut(&str) -> bool,
    ) -> Result<Execution, ServerError> {
        self.format_options_generation = self.format_options_generation.wrapping_add(1);
        let command_name = if force_window {
            "set-window-option"
        } else {
            "set-option"
        };
        let spec = command_spec(command_name).expect("executable command has catalog metadata");
        let parsed_options = parse_tmux_command_options(spec, invocation)?;
        let positional_start = invocation
            .args
            .len()
            .saturating_sub(parsed_options.positionals.len());
        let (options, positional) = parse_command_options(command_name, &invocation.args)?;
        let Some(option) = positional.first() else {
            return Err(ServerError::CommandParse(
                "set-option needs an option".to_owned(),
            ));
        };
        if positional.len() > 2 {
            return Err(ServerError::CommandParse(
                "set-option accepts at most one value".to_owned(),
            ));
        }
        let format_target = match options.value("-t") {
            Some(target) if force_window => self
                .resolve_window(Some(target), context.session, context.window)
                .ok()
                .and_then(|window| {
                    let pane = self.state.windows.get(&window)?.active_pane;
                    ExecutionContext::for_pane(&self.state, pane)
                })
                .unwrap_or_else(|| context.clone()),
            Some(target) => self
                .resolve_pane(Some(target), context.window, context.pane)
                .ok()
                .and_then(|pane| ExecutionContext::for_pane(&self.state, pane))
                .unwrap_or_else(|| context.clone()),
            None => context.clone(),
        };
        let format_context = FormatContext {
            session: format_target.session,
            window: format_target.window,
            pane: format_target.pane,
            active_session: context.session,
            format_client: context.target_format_client(),
            format_type: FormatType::Pane,
        };
        let expanded_option = (!crate::formats::plain_format(option, false))
            .then(|| expand_format_with_hooks(option, self, format_context, hooks));
        let option = expanded_option.as_deref().unwrap_or(option.as_str());
        let parsed = match parse_tmux_option(option) {
            Ok(parsed) => parsed,
            Err(()) if options.has("-q") => return Ok(Execution::default()),
            Err(()) => {
                return Err(ServerError::InvalidCommand(format!(
                    "invalid option: {option}"
                )));
            }
        };
        let value = positional
            .get(1)
            .map(|value| {
                let value = if invocation.argument_is_command_block(positional_start + 1) {
                    RawText::from(normalize_typed_command_block(
                        self,
                        crate::parser::command_block_body(value).unwrap_or(value),
                    )?)
                } else {
                    value.clone()
                };
                Ok(if options.has("-F") {
                    expand_format_with_hooks(&value, self, format_context, hooks)
                } else {
                    value
                })
            })
            .transpose()?;
        if parsed.index.is_some() && (parsed.name.starts_with('@') || is_native_option(parsed.name))
        {
            return Err(ServerError::InvalidCommand(format!(
                "not an array: {option}"
            )));
        }
        if parsed.name.starts_with('@') {
            return self.set_user_option(
                context,
                parsed.name,
                value.as_ref(),
                &options,
                force_window,
            );
        }
        let value = value.as_deref();
        if is_native_option(parsed.name) {
            return self.set_native_option(parsed.name, value, &options, force_window);
        }
        let table_option = match match_tmux_option(parsed.name) {
            Ok(Some(option)) => option,
            Ok(None) | Err(()) if options.has("-q") => return Ok(Execution::default()),
            Ok(None) => {
                return Err(ServerError::InvalidCommand(format!(
                    "invalid option: {option}"
                )));
            }
            Err(()) => {
                return Err(ServerError::InvalidCommand(format!(
                    "ambiguous option: {option}"
                )));
            }
        };
        if table_option.is_array {
            if tmux_option_is_hook(table_option.name)
                || tmux_stored_array(table_option.name).is_some()
            {
                let target = match self.resolve_tmux_option_target(
                    context,
                    &options,
                    force_window,
                    table_option.scope,
                ) {
                    Ok(target) => target,
                    Err(_) if options.has("-q") => return Ok(Execution::default()),
                    Err(error) => return Err(error),
                };
                return if tmux_option_is_hook(table_option.name) {
                    self.set_hook_array_option(
                        target,
                        table_option.name,
                        parsed.index,
                        value,
                        &options,
                    )
                } else {
                    self.set_array_option(target, table_option.name, parsed.index, value, &options)
                };
            }
            return Ok(Execution::default());
        }
        if table_option.default.is_none()
            && StatusOption::from_name(table_option.name).is_none()
            && WindowStatusOption::from_name(table_option.name).is_none()
            && ServerOption::from_name(table_option.name).is_none()
            && SessionOption::from_name(table_option.name).is_none()
            && WindowOption::from_name(table_option.name).is_none()
            && PaneOption::from_name(table_option.name).is_none()
            && parsed.index.is_none()
        {
            validate_unimplemented_option_value(table_option.name, value)?;
            return Err(ServerError::UnsupportedCommand(format!(
                "set-option {}",
                table_option.name
            )));
        }
        let target = match self.resolve_tmux_option_target(
            context,
            &options,
            force_window,
            table_option.scope,
        ) {
            Ok(target) => target,
            Err(_) if options.has("-q") => return Ok(Execution::default()),
            Err(error) => return Err(error),
        };
        if parsed.index.is_some() {
            return Err(ServerError::InvalidCommand(format!(
                "not an array: {option}"
            )));
        }
        match table_option.name {
            "aggressive-resize" => self.set_aggressive_resize(value, &options, target),
            "synchronize-panes" => self.set_synchronize_panes(value, &options, target),
            "mouse" => self.set_mouse(value, &options, target),
            "escape-time" => self.set_escape_time(value, &options),
            "automatic-rename" => self.set_automatic_rename(value, &options, target, hooks),
            "automatic-rename-format" => self.set_automatic_rename_format(value, &options, target),
            "remain-on-exit" => self.set_remain_on_exit(value, &options, target),
            "default-terminal" => self.set_default_terminal(value, &options),
            "default-command" | "default-shell" => self.set_spawn_string_option(
                table_option.name,
                value,
                &options,
                target,
                default_shell_is_valid,
            ),
            "display-time" | "initial-repeat-time" | "repeat-time" => {
                self.set_behavior_time(table_option.name, value, &options, target)
            }
            "buffer-limit" => self.set_buffer_limit(value, &options),
            "message-limit" => self.set_message_limit(value, &options),
            "history-limit" => self.set_history_limit(value, &options, target),
            "base-index" => self.set_base_index(value, &options, target),
            "renumber-windows" => self.set_renumber_windows(value, &options, target),
            "pane-base-index" => self.set_pane_base_index(value, &options, target),
            "word-separators" => self.set_word_separators(value, &options, target),
            "mode-keys" => self.set_mode_keys(value, &options, target),
            "popup-style" | "popup-border-style" | "popup-border-lines" => {
                self.set_popup_option(table_option.name, value, &options, target)
            }
            "menu-style" | "menu-selected-style" | "menu-border-style" | "menu-border-lines" => {
                self.set_menu_option(table_option.name, value, &options, target)
            }
            "lock-command" | "lock-after-time" => {
                self.set_lock_option(table_option.name, value, &options, target)
            }
            "prefix" | "set-clipboard" | "copy-command" => {
                self.set_scalar_tmux_option(table_option.name, value, &options)
            }
            option if let Some(option) = StatusOption::from_name(option) => {
                self.set_status_option(option, value, &options, target)
            }
            option if let Some(option) = WindowStatusOption::from_name(option) => {
                self.set_window_status_option(option, value, &options, target)
            }
            option if let Some(option) = ServerOption::from_name(option) => {
                self.set_server_option(option, value, &options, target)
            }
            option if let Some(option) = SessionOption::from_name(option) => {
                self.set_session_option(option, value, &options, target)
            }
            option if let Some(option) = WindowOption::from_name(option) => {
                self.set_window_option(option, value, &options, target)
            }
            option if let Some(option) = PaneOption::from_name(option) => {
                self.set_pane_option(option, value, &options, target)
            }
            option if tmux_stored_scalar(option).is_some() => {
                self.set_stored_scalar_option(option, value, &options, target)
            }
            option => unreachable!("implemented option {option} has a setter"),
        }
    }

    pub(super) fn set_user_option(
        &mut self,
        context: &ExecutionContext,
        option: &str,
        value: Option<&RawText>,
        options: &Options,
        force_window: bool,
    ) -> Result<Execution, ServerError> {
        let target = match self.resolve_user_option_target(context, options, force_window) {
            Ok(target) => target,
            Err(_) if options.has("-q") => return Ok(Execution::default()),
            Err(error) => return Err(error),
        };
        let unset = option_is_unset(options);
        if options.has("-o") && !unset && self.user_option_at_target(target, option).is_some() {
            return already_set_or_quiet(options, option);
        }
        if unset {
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
                    if let Some(values) = self.pane_user_options.get_mut(&pane) {
                        Arc::make_mut(values).remove(option);
                    }
                    self.forget_hook_fires(TmuxOptionTarget::Pane(pane), option);
                }
            }
            if self
                .user_options_at_target_mut(target)
                .remove(option)
                .is_none()
            {
                return Ok(Execution::default());
            }
            self.forget_hook_fires(target, option);
            return Ok(self.user_option_changed(context, target, option));
        }
        let value = value.ok_or_else(|| ServerError::InvalidCommand("empty value".to_owned()))?;
        let values = self.user_options_at_target_mut(target);
        if options.has("-a")
            && let Some(current) = values.get_mut(option)
        {
            current.push_bytes(value.as_bytes());
        } else {
            values.insert(option.to_owned(), value.clone());
        }
        Ok(self.user_option_changed(context, target, option))
    }

    pub(super) fn user_option_changed(
        &self,
        context: &ExecutionContext,
        target: TmuxOptionTarget,
        option: &str,
    ) -> Execution {
        let label = match target {
            TmuxOptionTarget::Server => "server".to_owned(),
            TmuxOptionTarget::GlobalSession => "global-session".to_owned(),
            TmuxOptionTarget::Session(session) => session.to_string(),
            TmuxOptionTarget::GlobalWindow => "global-window".to_owned(),
            TmuxOptionTarget::Window(window) => window.to_string(),
            TmuxOptionTarget::Pane(pane) => pane.to_string(),
        };
        let mut effects = vec![MuxEffect::UserOptionChanged {
            channel: format!("{option}@{label}"),
        }];
        if !context.no_hooks {
            let mut hook_context = match target {
                TmuxOptionTarget::Pane(pane) => {
                    ExecutionContext::for_pane(&self.state, pane).unwrap_or_else(|| context.clone())
                }
                TmuxOptionTarget::Window(window) => ExecutionContext {
                    window: Some(window),
                    ..context.clone()
                },
                TmuxOptionTarget::Session(session) => ExecutionContext {
                    session: Some(session),
                    ..context.clone()
                },
                TmuxOptionTarget::Server
                | TmuxOptionTarget::GlobalSession
                | TmuxOptionTarget::GlobalWindow => context.clone(),
            };
            if let Some(commands) = self.user_hook_commands(&hook_context, "@option-changed") {
                hook_context
                    .format_variables
                    .insert("hook_option".to_owned(), option.to_owned());
                hook_context
                    .format_variables
                    .insert("hook_target".to_owned(), label);
                effects.push(MuxEffect::RunHook {
                    name: "@option-changed".to_owned(),
                    commands,
                    context: hook_context,
                });
            }
        }
        Execution {
            output: RawText::default(),
            effects,
        }
    }

    fn set_popup_option(
        &mut self,
        option: &str,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        match option {
            "popup-style" | "popup-border-style" => {
                let default = PopupOptions::default();
                let (global, locals, local_key) = if option == "popup-style" {
                    (
                        &mut self.global_popup_style,
                        &mut self.window_popup_styles,
                        match target {
                            TmuxOptionTarget::Window(window) => Some(window),
                            TmuxOptionTarget::GlobalWindow => None,
                            _ => unreachable!("popup style is window scoped"),
                        },
                    )
                } else {
                    (
                        &mut self.global_popup_border_style,
                        &mut self.window_popup_border_styles,
                        match target {
                            TmuxOptionTarget::Window(window) => Some(window),
                            TmuxOptionTarget::GlobalWindow => None,
                            _ => unreachable!("popup style is window scoped"),
                        },
                    )
                };
                if options.has("-o")
                    && !unset
                    && local_key.is_none_or(|key| locals.contains_key(&key))
                {
                    return already_set_or_quiet(options, option);
                }
                if unset {
                    if let Some(key) = local_key {
                        locals.remove(&key);
                    } else {
                        *global = if option == "popup-style" {
                            default.style
                        } else {
                            default.border_style
                        };
                    }
                    return Ok(Execution::default());
                }
                let next = {
                    let value = value.ok_or_else(|| {
                        ServerError::InvalidCommand(format!("set-option {option} needs a value"))
                    })?;
                    if options.has("-a") {
                        let existing = local_key.and_then(|key| locals.get(&key)).unwrap_or(global);
                        format!("{existing}{value}")
                    } else {
                        value.to_owned()
                    }
                };
                if !valid_style(&next) {
                    return Err(ServerError::InvalidCommand(format!(
                        "invalid style: {next}"
                    )));
                }
                if let Some(key) = local_key {
                    locals.insert(key, next);
                } else {
                    *global = next;
                }
            }
            "popup-border-lines" => {
                let local_key = match target {
                    TmuxOptionTarget::Window(window) => Some(window),
                    TmuxOptionTarget::GlobalWindow => None,
                    _ => unreachable!("popup border lines is window scoped"),
                };
                if options.has("-o")
                    && !unset
                    && local_key.is_none_or(|key| self.window_popup_border_lines.contains_key(&key))
                {
                    return already_set_or_quiet(options, option);
                }
                if unset {
                    if let Some(key) = local_key {
                        self.window_popup_border_lines.remove(&key);
                    } else {
                        self.global_popup_border_lines = PopupBorderLines::Single;
                    }
                    return Ok(Execution::default());
                }
                let next = {
                    let value = value.ok_or_else(|| {
                        ServerError::InvalidCommand(
                            "set-option popup-border-lines needs a value".to_owned(),
                        )
                    })?;
                    value.parse().map_err(|()| {
                        ServerError::InvalidCommand(format!("unknown value: {value}"))
                    })?
                };
                if let Some(key) = local_key {
                    self.window_popup_border_lines.insert(key, next);
                } else {
                    self.global_popup_border_lines = next;
                }
            }
            _ => unreachable!("popup option is catalogued"),
        }
        Ok(Execution::default())
    }

    fn set_menu_option(
        &mut self,
        option: &str,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        let local_key = match target {
            TmuxOptionTarget::Window(window) => Some(window),
            TmuxOptionTarget::GlobalWindow => None,
            _ => unreachable!("menu options are window scoped"),
        };
        if option == "menu-border-lines" {
            if options.has("-o")
                && !unset
                && local_key.is_none_or(|key| self.window_menu_border_lines.contains_key(&key))
            {
                return already_set_or_quiet(options, option);
            }
            if unset {
                if let Some(key) = local_key {
                    self.window_menu_border_lines.remove(&key);
                } else {
                    self.global_menu_border_lines = PopupBorderLines::Single;
                }
                return Ok(Execution::default());
            }
            let value = value.ok_or_else(|| {
                ServerError::InvalidCommand(format!("set-option {option} needs a value"))
            })?;
            let next = value
                .parse()
                .map_err(|()| ServerError::InvalidCommand(format!("unknown value: {value}")))?;
            if let Some(key) = local_key {
                self.window_menu_border_lines.insert(key, next);
            } else {
                self.global_menu_border_lines = next;
            }
            return Ok(Execution::default());
        }
        let defaults = MenuOptions::default();
        let (global, locals, default) = match option {
            "menu-style" => (
                &mut self.global_menu_style,
                &mut self.window_menu_styles,
                defaults.style,
            ),
            "menu-selected-style" => (
                &mut self.global_menu_selected_style,
                &mut self.window_menu_selected_styles,
                defaults.selected_style,
            ),
            "menu-border-style" => (
                &mut self.global_menu_border_style,
                &mut self.window_menu_border_styles,
                defaults.border_style,
            ),
            _ => unreachable!("menu option is catalogued"),
        };
        if options.has("-o") && !unset && local_key.is_none_or(|key| locals.contains_key(&key)) {
            return already_set_or_quiet(options, option);
        }
        if unset {
            if let Some(key) = local_key {
                locals.remove(&key);
            } else {
                *global = default;
            }
            return Ok(Execution::default());
        }
        let value = value.ok_or_else(|| {
            ServerError::InvalidCommand(format!("set-option {option} needs a value"))
        })?;
        let next = if options.has("-a") {
            format!(
                "{}{value}",
                local_key.and_then(|key| locals.get(&key)).unwrap_or(global)
            )
        } else {
            value.to_owned()
        };
        if !valid_style(&next) {
            return Err(ServerError::InvalidCommand(format!(
                "invalid style: {next}"
            )));
        }
        if let Some(key) = local_key {
            locals.insert(key, next);
        } else {
            *global = next;
        }
        Ok(Execution::default())
    }

    fn set_lock_option(
        &mut self,
        option: &str,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        let local_key = match target {
            TmuxOptionTarget::Session(session) => Some(session),
            TmuxOptionTarget::GlobalSession => None,
            _ => unreachable!("lock options are session scoped"),
        };
        if option == "lock-after-time" {
            if options.has("-o")
                && !unset
                && local_key.is_none_or(|key| self.session_lock_after_times.contains_key(&key))
            {
                return already_set_or_quiet(options, option);
            }
            let next = if unset {
                0
            } else {
                parse_index_option(
                    value.ok_or_else(|| {
                        ServerError::InvalidCommand(format!("set-option {option} needs a value"))
                    })?,
                    i32::MAX.cast_unsigned(),
                )?
            };
            if let Some(key) = local_key {
                if unset {
                    self.session_lock_after_times.remove(&key);
                } else {
                    self.session_lock_after_times.insert(key, next);
                }
            } else {
                self.global_lock_after_time = next;
            }
            return Ok(Execution::default());
        }
        if options.has("-o")
            && !unset
            && local_key.is_none_or(|key| self.session_lock_commands.contains_key(&key))
        {
            return already_set_or_quiet(options, option);
        }
        if unset {
            if let Some(key) = local_key {
                self.session_lock_commands.remove(&key);
            } else {
                "lock -np".clone_into(&mut self.global_lock_command);
            }
            return Ok(Execution::default());
        }
        let value = value.ok_or_else(|| {
            ServerError::InvalidCommand(format!("set-option {option} needs a value"))
        })?;
        let next = if options.has("-a") {
            format!(
                "{}{value}",
                local_key
                    .and_then(|key| self.session_lock_commands.get(&key))
                    .unwrap_or(&self.global_lock_command)
            )
        } else {
            value.to_owned()
        };
        if let Some(key) = local_key {
            self.session_lock_commands.insert(key, next);
        } else {
            self.global_lock_command = next;
        }
        Ok(Execution::default())
    }

    fn set_native_option(
        &mut self,
        option: &str,
        value: Option<&str>,
        options: &Options,
        force_window: bool,
    ) -> Result<Execution, ServerError> {
        if option == "history-trickle" {
            return self.set_history_trickle(value, options, force_window);
        }
        if let Some(flag) = options
            .flags
            .iter()
            .find(|flag| !matches!(flag.as_str(), "-F" | "-g" | "-o" | "-q"))
        {
            return Err(ServerError::NativeUnsupportedCommand(format!(
                "set-option {flag} {option}"
            )));
        }
        if options.has("-o") {
            return already_set_or_quiet(options, option);
        }
        let changed = match option {
            "experimental-editor-pane" => {
                self.experimental_editor_pane =
                    parse_flag_value(value, self.experimental_editor_pane)?;
                MuxOptionKey::ExperimentalEditorPane
            }
            "agent-command" | "agent-claude-code-command" => {
                let value = value.ok_or_else(|| {
                    ServerError::InvalidCommand(format!("set-option {option} needs a value"))
                })?;
                if value.len() > MAX_AGENT_COMMAND_BYTES {
                    return Err(ServerError::InvalidCommand(format!(
                        "{option} exceeds {MAX_AGENT_COMMAND_BYTES} bytes"
                    )));
                }
                if value.trim().is_empty() {
                    return Err(ServerError::InvalidCommand(format!(
                        "{option} needs an adapter command"
                    )));
                }
                if option == "agent-command" {
                    value.clone_into(&mut self.agent.command);
                    MuxOptionKey::AgentCommand
                } else {
                    value.clone_into(&mut self.agent.claude_code_command);
                    MuxOptionKey::AgentClaudeCodeCommand
                }
            }
            "agent-auto-approve" => {
                let value = value.ok_or_else(|| {
                    ServerError::InvalidCommand(
                        "set-option agent-auto-approve needs a value".to_owned(),
                    )
                })?;
                self.agent.auto_approve = AgentAutoApprove::parse(value).ok_or_else(|| {
                    ServerError::InvalidCommand(format!(
                        "invalid agent-auto-approve value: {value}"
                    ))
                })?;
                MuxOptionKey::AgentAutoApprove
            }
            _ => unreachable!("native option is catalogued"),
        };
        Ok(Execution::effect(MuxEffect::MuxOptionChanged {
            option: changed,
            session: None,
        }))
    }

    fn set_scalar_tmux_option(
        &mut self,
        option: &str,
        value: Option<&str>,
        options: &Options,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            return already_set_or_quiet(options, option);
        }
        let changed = match option {
            "prefix" => {
                let value = if unset {
                    DEFAULT_PREFIX
                } else {
                    value.ok_or_else(|| {
                        ServerError::InvalidCommand("set-option prefix needs a value".to_owned())
                    })?
                };
                let value = parse_tmux_key(value)
                    .ok_or_else(|| ServerError::InvalidCommand(format!("bad key: {value}")))?;
                self.keys.set_prefix(value);
                MuxOptionKey::Prefix
            }
            "set-clipboard" => {
                self.set_clipboard = if unset {
                    SetClipboard::default()
                } else {
                    match value {
                        None | Some("") => self.set_clipboard.toggled(),
                        Some(value) => match value {
                            "on" => SetClipboard::On,
                            "external" => SetClipboard::External,
                            "off" => SetClipboard::Off,
                            value => {
                                return Err(ServerError::InvalidCommand(format!(
                                    "invalid set-clipboard value: {value}"
                                )));
                            }
                        },
                    }
                };
                MuxOptionKey::SetClipboard
            }
            "copy-command" => {
                let next = if unset {
                    String::new()
                } else {
                    let value = value.ok_or_else(|| {
                        ServerError::InvalidCommand(
                            "set-option copy-command needs a value".to_owned(),
                        )
                    })?;
                    if options.has("-a") {
                        format!("{}{value}", self.copy_command)
                    } else {
                        value.to_owned()
                    }
                };
                if next.len() > MAX_COPY_COMMAND_BYTES {
                    return Err(ServerError::InvalidCommand(format!(
                        "copy-command exceeds {MAX_COPY_COMMAND_BYTES} bytes"
                    )));
                }
                self.copy_command = next;
                MuxOptionKey::CopyCommand
            }
            _ => unreachable!("scalar tmux option is catalogued"),
        };
        Ok(Execution::effect(MuxEffect::MuxOptionChanged {
            option: changed,
            session: None,
        }))
    }

    pub(super) fn resolve_tmux_option_target(
        &self,
        context: &ExecutionContext,
        options: &Options,
        force_window: bool,
        scope: TmuxOptionScope,
    ) -> Result<TmuxOptionTarget, ServerError> {
        let missing = |slot: &str| {
            ServerError::InvalidCommand(match options.value("-t") {
                Some(target) => format!("no such {slot}: {target}"),
                None => format!("no current {slot}"),
            })
        };
        match scope {
            TmuxOptionScope::Server => Ok(TmuxOptionTarget::Server),
            TmuxOptionScope::Session if options.has("-g") => Ok(TmuxOptionTarget::GlobalSession),
            TmuxOptionScope::Session => self
                .resolve_option_window(context, options, force_window)
                .map(|window| TmuxOptionTarget::Session(self.state.windows[&window].session))
                .map_err(|_| missing("session")),
            TmuxOptionScope::WindowPane if options.has("-p") => self
                .resolve_pane(options.value("-t"), context.window, context.pane)
                .map(TmuxOptionTarget::Pane)
                .map_err(|_| missing("pane")),
            TmuxOptionScope::Window | TmuxOptionScope::WindowPane if options.has("-g") => {
                Ok(TmuxOptionTarget::GlobalWindow)
            }
            TmuxOptionScope::Window | TmuxOptionScope::WindowPane => self
                .resolve_option_window(context, options, force_window)
                .map(TmuxOptionTarget::Window)
                .map_err(|_| missing("window")),
        }
    }

    pub(super) fn resolve_option_window(
        &self,
        context: &ExecutionContext,
        options: &Options,
        force_window: bool,
    ) -> Result<WindowId, ServerError> {
        if force_window {
            self.resolve_window(options.value("-t"), context.session, context.window)
        } else {
            let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
            self.state
                .window_for_pane(pane)
                .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))
        }
    }

    fn set_buffer_limit(
        &mut self,
        value: Option<&str>,
        options: &Options,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            return already_set_or_quiet(options, "buffer-limit");
        }
        let limit = if unset {
            DEFAULT_BUFFER_LIMIT
        } else {
            let value = value.ok_or_else(|| {
                ServerError::InvalidCommand("set-option buffer-limit needs a value".to_owned())
            })?;
            let limit = value.parse::<usize>().map_err(|_| {
                ServerError::InvalidCommand(format!("invalid buffer-limit value: {value}"))
            })?;
            if !(1..=MAX_BUFFER_LIMIT).contains(&limit) {
                return Err(ServerError::InvalidCommand(format!(
                    "buffer-limit must be between 1 and {MAX_BUFFER_LIMIT}"
                )));
            }
            limit
        };
        self.buffer_limit = limit;
        Ok(Execution {
            output: RawText::default(),
            effects: vec![
                MuxEffect::BufferLimitChanged(limit),
                MuxEffect::MuxOptionChanged {
                    option: MuxOptionKey::BufferLimit,
                    session: None,
                },
            ],
        })
    }

    fn set_message_limit(
        &mut self,
        value: Option<&str>,
        options: &Options,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            return already_set_or_quiet(options, "message-limit");
        }
        let limit = if unset {
            DEFAULT_MESSAGE_LIMIT
        } else {
            let value = value.ok_or_else(|| {
                ServerError::InvalidCommand("set-option message-limit needs a value".to_owned())
            })?;
            let limit = value.parse::<usize>().map_err(|_| {
                ServerError::InvalidCommand(format!("invalid message-limit value: {value}"))
            })?;
            if limit > MAX_MESSAGE_LIMIT {
                return Err(ServerError::InvalidCommand(format!(
                    "message-limit must be between 0 and {MAX_MESSAGE_LIMIT}"
                )));
            }
            limit
        };
        self.message_limit = limit;
        Ok(Execution::default())
    }

    fn set_history_trickle(
        &mut self,
        value: Option<&str>,
        options: &Options,
        force_window: bool,
    ) -> Result<Execution, ServerError> {
        if force_window {
            return Err(ServerError::InvalidCommand(
                "history-trickle is a global server option".to_owned(),
            ));
        }
        if let Some(flag) = options
            .flags
            .iter()
            .find(|flag| !matches!(flag.as_str(), "-F" | "-g" | "-o" | "-q" | "-u"))
        {
            return Err(ServerError::NativeUnsupportedCommand(format!(
                "set-option {flag} history-trickle"
            )));
        }
        if options.has("-o") {
            if options.has("-u") {
                return Err(ServerError::InvalidCommand(
                    "history-trickle cannot combine set-once and unset".to_owned(),
                ));
            }
            return already_set_or_quiet(options, "history-trickle");
        }
        let limit = if options.has("-u") {
            if value.is_some() {
                return Err(ServerError::InvalidCommand(
                    "unsetting history-trickle does not accept a value".to_owned(),
                ));
            }
            DEFAULT_HISTORY_TRICKLE
        } else {
            let value = value.ok_or_else(|| {
                ServerError::InvalidCommand("set-option history-trickle needs a value".to_owned())
            })?;
            let limit = value.parse::<usize>().map_err(|_| {
                ServerError::InvalidCommand(format!("invalid history-trickle value: {value}"))
            })?;
            if limit > MAX_HISTORY_TRICKLE {
                return Err(ServerError::InvalidCommand(format!(
                    "history-trickle must be between 0 and {MAX_HISTORY_TRICKLE}"
                )));
            }
            limit
        };
        self.history_trickle = limit;
        Ok(Execution::effect(MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::HistoryTrickle,
            session: None,
        }))
    }

    fn set_history_limit(
        &mut self,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            match target {
                TmuxOptionTarget::GlobalSession => {
                    return already_set_or_quiet(options, "history-limit");
                }
                TmuxOptionTarget::Session(session)
                    if self.session_history_limits.contains_key(&session) =>
                {
                    return already_set_or_quiet(options, "history-limit");
                }
                _ => {}
            }
        }
        let limit = if unset {
            DEFAULT_HISTORY_LIMIT
        } else {
            parse_history_limit(value.ok_or_else(|| {
                ServerError::InvalidCommand("set-option history-limit needs a value".to_owned())
            })?)?
        };
        match target {
            TmuxOptionTarget::GlobalSession => {
                self.global_history_limit = limit;
                Ok(Execution::effect(MuxEffect::MuxOptionChanged {
                    option: MuxOptionKey::HistoryLimit,
                    session: None,
                }))
            }
            TmuxOptionTarget::Session(session) => {
                if unset {
                    self.session_history_limits.remove(&session);
                } else {
                    self.session_history_limits.insert(session, limit);
                }
                Ok(Execution::default())
            }
            _ => unreachable!("history-limit has session scope"),
        }
    }

    fn set_base_index(
        &mut self,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            match target {
                TmuxOptionTarget::GlobalSession => {
                    return already_set_or_quiet(options, "base-index");
                }
                TmuxOptionTarget::Session(session)
                    if self.session_base_indices.contains_key(&session) =>
                {
                    return already_set_or_quiet(options, "base-index");
                }
                _ => {}
            }
        }
        let index = if unset {
            DEFAULT_BASE_INDEX
        } else {
            parse_index_option(
                value.ok_or_else(|| {
                    ServerError::InvalidCommand("set-option base-index needs a value".to_owned())
                })?,
                MAX_BASE_INDEX,
            )?
        };
        match target {
            TmuxOptionTarget::GlobalSession => self.global_base_index = index,
            TmuxOptionTarget::Session(session) => {
                if unset {
                    self.session_base_indices.remove(&session);
                } else {
                    self.session_base_indices.insert(session, index);
                }
            }
            _ => unreachable!("base-index has session scope"),
        }
        Ok(Execution::default())
    }

    fn set_renumber_windows(
        &mut self,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            match target {
                TmuxOptionTarget::GlobalSession => {
                    return already_set_or_quiet(options, "renumber-windows");
                }
                TmuxOptionTarget::Session(session)
                    if self.session_renumber_windows.contains_key(&session) =>
                {
                    return already_set_or_quiet(options, "renumber-windows");
                }
                _ => {}
            }
        }
        match target {
            TmuxOptionTarget::GlobalSession => {
                self.global_renumber_windows = if unset {
                    DEFAULT_RENUMBER_WINDOWS
                } else {
                    parse_tmux_flag_value(value, self.global_renumber_windows)?
                };
            }
            TmuxOptionTarget::Session(session) => {
                if unset {
                    self.session_renumber_windows.remove(&session);
                } else {
                    let next =
                        parse_tmux_flag_value(value, self.renumber_windows_for_session(session))?;
                    self.session_renumber_windows.insert(session, next);
                }
            }
            _ => unreachable!("renumber-windows has session scope"),
        }
        Ok(Execution::default())
    }

    fn set_pane_base_index(
        &mut self,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            match target {
                TmuxOptionTarget::GlobalWindow => {
                    return already_set_or_quiet(options, "pane-base-index");
                }
                TmuxOptionTarget::Window(window)
                    if self.state.window_pane_base_indices.contains_key(&window) =>
                {
                    return already_set_or_quiet(options, "pane-base-index");
                }
                _ => {}
            }
        }
        if target == TmuxOptionTarget::GlobalWindow {
            if options.has("-o") && !unset {
                return already_set_or_quiet(options, "pane-base-index");
            }
            let next = if unset {
                DEFAULT_PANE_BASE_INDEX
            } else {
                parse_index_option(
                    value.ok_or_else(|| {
                        ServerError::InvalidCommand(
                            "set-option pane-base-index needs a value".to_owned(),
                        )
                    })?,
                    MAX_PANE_BASE_INDEX,
                )?
            };
            if self.state.global_pane_base_index != next {
                self.state.global_pane_base_index = next;
                self.state.bump_generation();
            }
            return Ok(Execution::default());
        }

        let TmuxOptionTarget::Window(window) = target else {
            unreachable!("pane-base-index has window scope")
        };
        let previous = self.pane_base_index_for_window(window);
        if unset {
            self.state.window_pane_base_indices.remove(&window);
        } else {
            let next = parse_index_option(
                value.ok_or_else(|| {
                    ServerError::InvalidCommand(
                        "set-option pane-base-index needs a value".to_owned(),
                    )
                })?,
                MAX_PANE_BASE_INDEX,
            )?;
            self.state.window_pane_base_indices.insert(window, next);
        }
        if self.pane_base_index_for_window(window) != previous {
            self.state.bump_generation();
        }
        Ok(Execution::default())
    }

    fn set_status_option(
        &mut self,
        option: StatusOption,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        let session = match target {
            TmuxOptionTarget::GlobalSession => None,
            TmuxOptionTarget::Session(session) => Some(session),
            _ => unreachable!("status options have session scope"),
        };
        let already_set = session.is_none_or(|session| {
            self.session_status_options
                .get(&session)
                .is_some_and(|values| values.contains_key(&option))
        });
        if options.has("-o") && !unset && already_set {
            return already_set_or_quiet(options, option.as_str());
        }
        let previous = self.status_formats_for_session(session);
        if unset {
            if let Some(session) = session {
                if let Some(values) = self.session_status_options.get_mut(&session) {
                    values.remove(&option);
                    if values.is_empty() {
                        self.session_status_options.remove(&session);
                    }
                }
            } else {
                self.status
                    .set(option, None)
                    .map_err(ServerError::InvalidCommand)?;
            }
            let current = self.status_formats_for_session(session);
            let changed = current != previous;
            let unmarked = self.mark_explicit_status_option(session, option.as_str(), false);
            return Ok(status_option_execution(
                session,
                changed || unmarked,
                current.rows() != previous.rows(),
            ));
        }

        let mut next = previous.clone();
        let toggled = if option == StatusOption::Enabled && value.is_none_or(str::is_empty) {
            next.toggle_enabled_choice();
            Some(next.value(option))
        } else if option == StatusOption::Justify && value.is_none() {
            Some(
                match next.justify {
                    crate::StatusJustify::Left => crate::StatusJustify::Centre,
                    crate::StatusJustify::Centre => crate::StatusJustify::Left,
                    value => value,
                }
                .as_str()
                .to_owned(),
            )
        } else if option == StatusOption::Position && value.is_none() {
            Some(
                match next.position {
                    crate::StatusPosition::Top => crate::StatusPosition::Bottom,
                    crate::StatusPosition::Bottom => crate::StatusPosition::Top,
                }
                .as_str()
                .to_owned(),
            )
        } else {
            None
        };
        let value = toggled.as_deref().or(value);
        let appended = (!unset && options.has("-a"))
            .then(|| previous.format(option))
            .flatten()
            .zip(value)
            .map(|(current, value)| {
                let separator = if option.is_style() && !current.is_empty() && !value.is_empty() {
                    ","
                } else {
                    ""
                };
                format!("{current}{separator}{value}")
            });
        let value = appended.as_deref().or(value).ok_or_else(|| {
            ServerError::InvalidCommand(format!("set-option {} needs a value", option.as_str()))
        })?;
        next.set(option, Some(value))
            .map_err(ServerError::InvalidCommand)?;
        let changed = next != previous;
        let rows_changed = next.rows() != previous.rows();
        if let Some(session) = session {
            self.session_status_options
                .entry(session)
                .or_default()
                .insert(option, next.value(option));
        } else {
            self.status = next;
        }
        let marked = self.mark_explicit_status_option(session, option.as_str(), true);
        Ok(status_option_execution(
            session,
            changed || marked,
            rows_changed,
        ))
    }

    fn set_window_status_option(
        &mut self,
        option: WindowStatusOption,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        let window = match target {
            TmuxOptionTarget::GlobalWindow => None,
            TmuxOptionTarget::Window(window) => Some(window),
            _ => unreachable!("window status options have window scope"),
        };
        let already_set = window.is_none_or(|window| {
            self.window_status_options
                .get(&window)
                .is_some_and(|values| values.contains_key(&option))
        });
        if options.has("-o") && !unset && already_set {
            return already_set_or_quiet(options, option.as_str());
        }
        let previous = window
            .and_then(|window| self.window_status_options.get(&window))
            .and_then(|values| values.get(&option))
            .map_or_else(|| self.window_status.value(option), String::as_str);
        let changed = if unset {
            if let Some(window) = window {
                let changed = previous != self.window_status.value(option);
                if let Some(values) = self.window_status_options.get_mut(&window) {
                    values.remove(&option);
                    if values.is_empty() {
                        self.window_status_options.remove(&window);
                    }
                }
                changed
            } else {
                self.window_status
                    .set(option, None)
                    .map_err(ServerError::InvalidCommand)?
            }
        } else {
            let value = value.ok_or_else(|| {
                ServerError::InvalidCommand(format!("set-option {} needs a value", option.as_str()))
            })?;
            let appended = options.has("-a").then(|| {
                let separator = if option.is_style() && !previous.is_empty() && !value.is_empty() {
                    ","
                } else {
                    ""
                };
                format!("{previous}{separator}{value}")
            });
            let value = appended.as_deref().unwrap_or(value);
            if let Some(window) = window {
                let next = WindowStatusFormats::parse_value(option, value)
                    .map_err(ServerError::InvalidCommand)?;
                let changed = previous != next;
                self.window_status_options
                    .entry(window)
                    .or_default()
                    .insert(option, next);
                changed
            } else {
                self.window_status
                    .set(option, Some(value))
                    .map_err(ServerError::InvalidCommand)?
            }
        };
        if changed {
            self.state.bump_generation();
            Ok(Execution::effect(MuxEffect::SnapshotChanged))
        } else {
            Ok(Execution::default())
        }
    }

    fn set_server_option(
        &mut self,
        option: ServerOption,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        assert_eq!(target, TmuxOptionTarget::Server);
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            return already_set_or_quiet(options, option.as_str());
        }
        if unset {
            self.server_options.reset(option);
            return Ok(server_option_execution(option));
        }
        let normalized = match option {
            ServerOption::Backspace => value
                .map(|value| {
                    parse_tmux_key(value)
                        .ok_or_else(|| ServerError::InvalidCommand(format!("bad key: {value}")))
                })
                .transpose()?,
            ServerOption::DefaultClientCommand => value
                .map(|value| normalize_option_command(self, value))
                .transpose()?,
            _ => None,
        };
        let value = normalized.as_deref().or(value);
        let appended = (options.has("-a"))
            .then(|| {
                option.append_separator().and_then(|separator| {
                    value.map(|value| {
                        format!("{}{separator}{value}", self.server_options.value(option))
                    })
                })
            })
            .flatten();
        self.server_options
            .set_command(option, appended.as_deref().or(value))
            .map_err(ServerError::InvalidCommand)?;
        Ok(server_option_execution(option))
    }

    fn set_session_option(
        &mut self,
        option: SessionOption,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        let session = match target {
            TmuxOptionTarget::GlobalSession => None,
            TmuxOptionTarget::Session(session) => Some(session),
            _ => unreachable!("session options have session scope"),
        };
        let locally_set = session.is_none_or(|session| {
            self.session_options
                .get(&session)
                .is_some_and(|values| values.contains_key(&option))
        });
        if options.has("-o") && !unset && locally_set {
            return already_set_or_quiet(options, option.as_str());
        }
        let message_line_before =
            (option == SessionOption::MessageLine).then(|| self.message_line_for_session(session));
        let restyles_messages = matches!(
            option,
            SessionOption::MessageStyle | SessionOption::MessageCommandStyle
        );
        if unset {
            if let Some(session) = session {
                remove_option_override(&mut self.session_options, session, option);
            } else {
                self.global_session_options.reset(option);
            }
            return Ok(self.session_option_execution(
                session,
                message_line_before,
                restyles_messages,
            ));
        }
        let previous = session.map_or_else(
            || self.global_session_options.clone(),
            |session| self.session_knobs(session),
        );
        let appended = options
            .has("-a")
            .then(|| {
                option.append_separator().and_then(|separator| {
                    value.map(|value| format!("{}{separator}{value}", previous.value(option)))
                })
            })
            .flatten();
        let mut next = previous;
        next.set_command(option, appended.as_deref().or(value))
            .map_err(ServerError::InvalidCommand)?;
        if let Some(session) = session {
            self.session_options
                .entry(session)
                .or_default()
                .insert(option, next.value(option));
        } else {
            self.global_session_options = next;
        }
        Ok(self.session_option_execution(session, message_line_before, restyles_messages))
    }

    fn session_option_execution(
        &self,
        session: Option<SessionId>,
        message_line_before: Option<u8>,
        restyles_messages: bool,
    ) -> Execution {
        if restyles_messages
            || message_line_before
                .is_some_and(|before| self.message_line_for_session(session) != before)
        {
            Execution::effect(MuxEffect::StatusFormatsChanged { session })
        } else {
            Execution::default()
        }
    }

    fn set_window_option(
        &mut self,
        option: WindowOption,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        let window = match target {
            TmuxOptionTarget::GlobalWindow => None,
            TmuxOptionTarget::Window(window) => Some(window),
            _ => unreachable!("window options have window scope"),
        };
        let locally_set = window.is_none_or(|window| {
            self.window_options
                .get(&window)
                .is_some_and(|values| values.contains_key(&option))
        });
        if options.has("-o") && !options.has("-u") && locally_set {
            return already_set_or_quiet(options, option.as_str());
        }
        let previous = window.map_or_else(
            || self.global_window_options.clone(),
            |window| self.window_knobs(window),
        );
        let monitor_silence_written =
            option == WindowOption::MonitorSilence && (!unset || locally_set);
        if unset {
            if let Some(window) = window {
                remove_option_override(&mut self.window_options, window, option);
            } else {
                self.global_window_options.reset(option);
            }
        } else {
            let appended = options
                .has("-a")
                .then(|| {
                    option.append_separator().and_then(|separator| {
                        value.map(|value| format!("{}{separator}{value}", previous.value(option)))
                    })
                })
                .flatten();
            let mut next = previous.clone();
            next.set_command(option, appended.as_deref().or(value))
                .map_err(ServerError::InvalidCommand)?;
            if let Some(window) = window {
                self.window_options
                    .entry(window)
                    .or_default()
                    .insert(option, next.value(option));
            } else {
                self.global_window_options = next;
            }
        }
        let next = window.map_or_else(
            || self.global_window_options.clone(),
            |window| self.window_knobs(window),
        );
        if monitor_silence_written {
            return Ok(Execution::effect(MuxEffect::MonitorSilenceChanged));
        }
        if next == previous {
            return Ok(Execution::default());
        }
        match option {
            WindowOption::WindowSize => {
                self.restore_manual_window_extents(window);
                Ok(Execution::effect(MuxEffect::WindowSizeChanged { window }))
            }
            WindowOption::WrapSearch => Ok(Execution::effect(MuxEffect::TerminalKnobsChanged {
                window,
                pane: None,
            })),
            WindowOption::PaneBorderIndicators => Ok(Execution::effect(MuxEffect::SnapshotChanged)),
            WindowOption::MonitorSilence => unreachable!(),
            _ => Ok(Execution::default()),
        }
    }

    fn set_pane_option(
        &mut self,
        option: PaneOption,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        let locally_set = match target {
            TmuxOptionTarget::GlobalWindow => true,
            TmuxOptionTarget::Window(window) => self
                .window_pane_options
                .get(&window)
                .is_some_and(|values| values.contains_key(&option)),
            TmuxOptionTarget::Pane(pane) => self
                .pane_options
                .get(&pane)
                .is_some_and(|values| values.contains_key(&option)),
            _ => unreachable!("pane options have window-pane scope"),
        };
        if options.has("-o") && !unset && locally_set {
            return already_set_or_quiet(options, option.as_str());
        }
        let pane_overrides_removed = match target {
            TmuxOptionTarget::Window(window) if options.has("-U") => {
                self.remove_pane_option_overrides(window, option)
            }
            _ => false,
        };
        let previous = match target {
            TmuxOptionTarget::GlobalWindow => self.global_pane_options.clone(),
            TmuxOptionTarget::Window(window) => self.pane_knobs_for_window(window),
            TmuxOptionTarget::Pane(pane) => self.pane_knobs(pane),
            _ => unreachable!("pane options have window-pane scope"),
        };
        if unset {
            match target {
                TmuxOptionTarget::GlobalWindow => {
                    self.global_pane_options.reset(option);
                }
                TmuxOptionTarget::Window(window) => {
                    remove_option_override(&mut self.window_pane_options, window, option);
                }
                TmuxOptionTarget::Pane(pane) => {
                    remove_option_override(&mut self.pane_options, pane, option);
                }
                _ => unreachable!("pane options have window-pane scope"),
            }
        } else {
            let appended = options
                .has("-a")
                .then(|| {
                    option.append_separator().and_then(|separator| {
                        value.map(|value| format!("{}{separator}{value}", previous.value(option)))
                    })
                })
                .flatten();
            let mut next = previous.clone();
            next.set_command(option, appended.as_deref().or(value))
                .map_err(ServerError::InvalidCommand)?;
            match target {
                TmuxOptionTarget::GlobalWindow => self.global_pane_options = next,
                TmuxOptionTarget::Window(window) => {
                    self.window_pane_options
                        .entry(window)
                        .or_default()
                        .insert(option, next.value(option));
                }
                TmuxOptionTarget::Pane(pane) => {
                    self.pane_options
                        .entry(pane)
                        .or_default()
                        .insert(option, next.value(option));
                }
                _ => unreachable!("pane options have window-pane scope"),
            }
        }
        let next = match target {
            TmuxOptionTarget::GlobalWindow => self.global_pane_options.clone(),
            TmuxOptionTarget::Window(window) => self.pane_knobs_for_window(window),
            TmuxOptionTarget::Pane(pane) => self.pane_knobs(pane),
            _ => unreachable!("pane options have window-pane scope"),
        };
        if next == previous && !pane_overrides_removed {
            return Ok(Execution::default());
        }
        if option == PaneOption::PaneBorderLines {
            return Ok(Execution::effect(MuxEffect::SnapshotChanged));
        }
        if !option.updates_terminal_worker() {
            return Ok(Execution::default());
        }
        let (window, pane) = match target {
            TmuxOptionTarget::GlobalWindow => (None, None),
            TmuxOptionTarget::Window(window) => (Some(window), None),
            TmuxOptionTarget::Pane(pane) => (None, Some(pane)),
            _ => unreachable!("pane options have window-pane scope"),
        };
        Ok(Execution::effect(MuxEffect::TerminalKnobsChanged {
            window,
            pane,
        }))
    }

    fn remove_pane_option_overrides(&mut self, window: WindowId, option: PaneOption) -> bool {
        let panes = self
            .state
            .windows
            .get(&window)
            .map(|window| window.pane_order().to_vec())
            .unwrap_or_default();
        let mut removed = false;
        for pane in panes {
            removed |= self
                .pane_options
                .get(&pane)
                .is_some_and(|values| values.contains_key(&option));
            remove_option_override(&mut self.pane_options, pane, option);
        }
        removed
    }

    fn set_word_separators(
        &mut self,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            match target {
                TmuxOptionTarget::GlobalSession => {
                    return already_set_or_quiet(options, "word-separators");
                }
                TmuxOptionTarget::Session(session)
                    if self.session_word_separators.contains_key(&session) =>
                {
                    return already_set_or_quiet(options, "word-separators");
                }
                _ => {}
            }
        }

        if target == TmuxOptionTarget::GlobalSession {
            let next = if unset {
                DEFAULT_WORD_SEPARATORS.to_owned()
            } else {
                let value = value.ok_or_else(|| {
                    ServerError::InvalidCommand(
                        "set-option word-separators needs a value".to_owned(),
                    )
                })?;
                if options.has("-a") {
                    let mut appended = self.global_word_separators.clone();
                    appended.push_str(value);
                    appended
                } else {
                    value.to_owned()
                }
            };
            validate_word_separators(&next)?;
            self.global_word_separators = next;
            return Ok(Execution {
                output: RawText::default(),
                effects: vec![
                    MuxEffect::WordSeparatorsChanged { session: None },
                    MuxEffect::MuxOptionChanged {
                        option: MuxOptionKey::WordSeparators,
                        session: None,
                    },
                ],
            });
        }

        let TmuxOptionTarget::Session(session) = target else {
            unreachable!("word-separators has session scope")
        };
        if unset {
            self.session_word_separators.remove(&session);
        } else {
            let value = value.ok_or_else(|| {
                ServerError::InvalidCommand("set-option word-separators needs a value".to_owned())
            })?;
            let next = if options.has("-a") {
                let mut appended = self.word_separators_for_session(session).to_owned();
                appended.push_str(value);
                appended
            } else {
                value.to_owned()
            };
            validate_word_separators(&next)?;
            self.session_word_separators.insert(session, next);
        }
        Ok(Execution::effect(MuxEffect::WordSeparatorsChanged {
            session: Some(session),
        }))
    }

    fn set_mode_keys(
        &mut self,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            if target == TmuxOptionTarget::GlobalWindow {
                return already_set_or_quiet(options, "mode-keys");
            }
            if let TmuxOptionTarget::Window(window) = target
                && self.window_mode_keys.contains_key(&window)
            {
                return already_set_or_quiet(options, "mode-keys");
            }
        }

        if target == TmuxOptionTarget::GlobalWindow {
            let next = if unset {
                ModeKeys::default()
            } else {
                parse_mode_keys(value, self.global_mode_keys)?
            };
            self.global_mode_keys = next;
            return Ok(Execution {
                output: RawText::default(),
                effects: vec![
                    MuxEffect::ModeKeysChanged { window: None },
                    MuxEffect::MuxOptionChanged {
                        option: MuxOptionKey::ModeKeys,
                        session: None,
                    },
                ],
            });
        }

        let TmuxOptionTarget::Window(window) = target else {
            unreachable!("mode-keys has window scope")
        };
        if unset {
            self.window_mode_keys.remove(&window);
        } else {
            let next = parse_mode_keys(value, self.mode_keys_for_window(window))?;
            self.window_mode_keys.insert(window, next);
        }
        Ok(Execution::effect(MuxEffect::ModeKeysChanged {
            window: Some(window),
        }))
    }

    pub(super) fn set_synchronize_panes(
        &mut self,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            let already = match target {
                TmuxOptionTarget::GlobalWindow => true,
                TmuxOptionTarget::Window(window) => {
                    self.state.window_synchronize_override(window)?.is_some()
                }
                TmuxOptionTarget::Pane(pane) => {
                    self.state.pane_synchronize_override(pane)?.is_some()
                }
                _ => unreachable!("synchronize-panes has window or pane scope"),
            };
            if already {
                return already_set_or_quiet(options, "synchronize-panes");
            }
        }
        match target {
            TmuxOptionTarget::GlobalWindow => {
                let next = if unset {
                    false
                } else {
                    parse_tmux_flag_value(value, self.state.global_synchronize_panes())?
                };
                self.state.set_global_synchronize_panes(next);
                Ok(Execution::effect(MuxEffect::MuxOptionChanged {
                    option: MuxOptionKey::SynchronizePanes,
                    session: None,
                }))
            }
            TmuxOptionTarget::Pane(pane) => {
                let next = if unset {
                    None
                } else {
                    Some(parse_tmux_flag_value(
                        value,
                        self.state.pane_synchronize_panes(pane)?,
                    )?)
                };
                self.state.set_pane_synchronize_panes(pane, next)?;
                Ok(Execution::default())
            }
            TmuxOptionTarget::Window(window) => {
                if options.has("-U") {
                    self.state.clear_pane_synchronize_overrides(window)?;
                    self.state.set_window_synchronize_panes(window, None)?;
                } else {
                    let next = if unset {
                        None
                    } else {
                        Some(parse_tmux_flag_value(
                            value,
                            self.state.window_synchronize_panes(window)?,
                        )?)
                    };
                    self.state.set_window_synchronize_panes(window, next)?;
                }
                Ok(Execution::default())
            }
            _ => unreachable!("synchronize-panes has window or pane scope"),
        }
    }

    fn set_mouse(
        &mut self,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            let already = match target {
                TmuxOptionTarget::GlobalSession => true,
                TmuxOptionTarget::Session(session) => self.session_mouse.contains_key(&session),
                _ => unreachable!("mouse has session scope"),
            };
            if already {
                return already_set_or_quiet(options, "mouse");
            }
        }
        let scope = match target {
            TmuxOptionTarget::GlobalSession => {
                self.global_mouse = if unset {
                    DEFAULT_MOUSE
                } else {
                    parse_tmux_flag_value(value, self.global_mouse)?
                };
                None
            }
            TmuxOptionTarget::Session(session) => {
                if unset {
                    self.session_mouse.remove(&session);
                } else {
                    let next = parse_tmux_flag_value(value, self.mouse_for_session(session))?;
                    self.session_mouse.insert(session, next);
                }
                Some(session)
            }
            _ => unreachable!("mouse has session scope"),
        };
        Ok(Execution::effect(MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::Mouse,
            session: scope,
        }))
    }

    fn set_escape_time(
        &mut self,
        value: Option<&str>,
        options: &Options,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            return already_set_or_quiet(options, "escape-time");
        }
        self.escape_time_ms = if unset {
            DEFAULT_ESCAPE_TIME_MS
        } else {
            parse_index_option(
                value.ok_or_else(|| {
                    ServerError::InvalidCommand("set-option escape-time needs a value".to_owned())
                })?,
                i32::MAX.cast_unsigned(),
            )?
        };
        Ok(Execution::effect(MuxEffect::MuxOptionChanged {
            option: MuxOptionKey::EscapeTime,
            session: None,
        }))
    }

    pub(super) fn set_automatic_rename(
        &mut self,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            let already = match target {
                TmuxOptionTarget::GlobalWindow => true,
                TmuxOptionTarget::Window(window) => self
                    .state
                    .window_automatic_rename_override(window)?
                    .is_some(),
                _ => unreachable!("automatic-rename has window scope"),
            };
            if already {
                return already_set_or_quiet(options, "automatic-rename");
            }
        }
        match target {
            TmuxOptionTarget::GlobalWindow => {
                let next = if unset {
                    None
                } else {
                    Some(parse_tmux_flag_value(
                        value,
                        self.state.global_automatic_rename(),
                    )?)
                };
                self.state.set_global_automatic_rename(next);
            }
            TmuxOptionTarget::Window(window) => {
                let next = if unset {
                    None
                } else {
                    Some(parse_tmux_flag_value(
                        value,
                        self.state.window_automatic_rename(window)?,
                    )?)
                };
                self.state.set_window_automatic_rename(window, next)?;
            }
            _ => unreachable!("automatic-rename has window scope"),
        }
        let panes = match target {
            TmuxOptionTarget::GlobalWindow => self
                .state
                .windows
                .values()
                .filter(|window| {
                    self.state
                        .window_automatic_rename(window.id)
                        .unwrap_or_default()
                })
                .map(|window| window.active_pane)
                .collect::<Vec<_>>(),
            TmuxOptionTarget::Window(window)
                if self
                    .state
                    .window_automatic_rename(window)
                    .unwrap_or_default() =>
            {
                vec![self.state.windows[&window].active_pane]
            }
            TmuxOptionTarget::Window(_) => Vec::new(),
            _ => unreachable!("automatic-rename has window scope"),
        };
        for pane in panes {
            self.refresh_automatic_window_name_for_pane(pane, hooks);
        }
        Ok(Execution::default())
    }

    pub(super) fn set_aggressive_resize(
        &mut self,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            let already = match target {
                TmuxOptionTarget::GlobalWindow => true,
                TmuxOptionTarget::Window(window) => self
                    .state
                    .window_aggressive_resize_override(window)?
                    .is_some(),
                _ => unreachable!("aggressive-resize has window scope"),
            };
            if already {
                return already_set_or_quiet(options, "aggressive-resize");
            }
        }
        let window = match target {
            TmuxOptionTarget::GlobalWindow => {
                let next = if unset {
                    None
                } else {
                    Some(parse_tmux_flag_value(
                        value,
                        self.state.global_aggressive_resize(),
                    )?)
                };
                self.state.set_global_aggressive_resize(next);
                None
            }
            TmuxOptionTarget::Window(window) => {
                let next = if unset {
                    None
                } else {
                    Some(parse_tmux_flag_value(
                        value,
                        self.state.window_aggressive_resize(window)?,
                    )?)
                };
                self.state.set_window_aggressive_resize(window, next)?;
                Some(window)
            }
            _ => unreachable!("aggressive-resize has window scope"),
        };
        Ok(Execution::effect(MuxEffect::AggressiveResizeChanged {
            window,
        }))
    }

    fn set_automatic_rename_format(
        &mut self,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            let already = match target {
                TmuxOptionTarget::GlobalWindow => true,
                TmuxOptionTarget::Window(window) => {
                    self.window_automatic_rename_formats.contains_key(&window)
                }
                _ => unreachable!("automatic-rename-format has window scope"),
            };
            if already {
                return already_set_or_quiet(options, "automatic-rename-format");
            }
        }
        match target {
            TmuxOptionTarget::GlobalWindow => {
                self.global_automatic_rename_format = if unset {
                    DEFAULT_AUTOMATIC_RENAME_FORMAT.to_owned()
                } else {
                    let value = value.ok_or_else(|| {
                        ServerError::InvalidCommand(
                            "set-option automatic-rename-format needs a value".to_owned(),
                        )
                    })?;
                    if options.has("-a") {
                        format!("{}{value}", self.global_automatic_rename_format)
                    } else {
                        value.to_owned()
                    }
                };
            }
            TmuxOptionTarget::Window(window) => {
                if unset {
                    self.window_automatic_rename_formats.remove(&window);
                } else {
                    let value = value.ok_or_else(|| {
                        ServerError::InvalidCommand(
                            "set-option automatic-rename-format needs a value".to_owned(),
                        )
                    })?;
                    let next = if options.has("-a") {
                        format!("{}{value}", self.automatic_rename_format_for_window(window))
                    } else {
                        value.to_owned()
                    };
                    self.window_automatic_rename_formats.insert(window, next);
                }
            }
            _ => unreachable!("automatic-rename-format has window scope"),
        }
        Ok(Execution::default())
    }

    fn set_remain_on_exit(
        &mut self,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            let already = match target {
                TmuxOptionTarget::GlobalWindow => true,
                TmuxOptionTarget::Window(window) => {
                    self.window_remain_on_exit.contains_key(&window)
                }
                TmuxOptionTarget::Pane(pane) => self.pane_remain_on_exit.contains_key(&pane),
                _ => unreachable!("remain-on-exit has window or pane scope"),
            };
            if already {
                return already_set_or_quiet(options, "remain-on-exit");
            }
        }
        match target {
            TmuxOptionTarget::GlobalWindow => {
                self.global_remain_on_exit = if unset {
                    RemainOnExit::Off
                } else {
                    parse_remain_on_exit(value, self.global_remain_on_exit)?
                };
            }
            TmuxOptionTarget::Window(window) => {
                if options.has("-U") {
                    let panes = self
                        .state
                        .windows
                        .get(&window)
                        .map(|window| window.pane_order().to_vec())
                        .unwrap_or_default();
                    for pane in panes {
                        self.pane_remain_on_exit.remove(&pane);
                    }
                    self.window_remain_on_exit.remove(&window);
                } else if unset {
                    self.window_remain_on_exit.remove(&window);
                } else {
                    let next = parse_remain_on_exit(value, self.remain_on_exit_for_window(window))?;
                    self.window_remain_on_exit.insert(window, next);
                }
            }
            TmuxOptionTarget::Pane(pane) => {
                if unset {
                    self.pane_remain_on_exit.remove(&pane);
                } else {
                    let next = parse_remain_on_exit(value, self.remain_on_exit_for_pane(pane)?)?;
                    self.pane_remain_on_exit.insert(pane, next);
                }
            }
            _ => unreachable!("remain-on-exit has window or pane scope"),
        }
        Ok(Execution::default())
    }

    fn set_default_terminal(
        &mut self,
        value: Option<&str>,
        options: &Options,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            return already_set_or_quiet(options, "default-terminal");
        }
        self.default_terminal = if unset {
            None
        } else {
            let value = value.ok_or_else(|| {
                ServerError::InvalidCommand("set-option default-terminal needs a value".to_owned())
            })?;
            Some(if options.has("-a") {
                format!(
                    "{}{value}",
                    self.default_terminal.as_deref().unwrap_or(DEFAULT_TERMINAL)
                )
            } else {
                value.to_owned()
            })
        };
        Ok(Execution::default())
    }

    fn set_spawn_string_option(
        &mut self,
        option: &str,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
        default_shell_is_valid: &mut impl FnMut(&str) -> bool,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            let already = match (option, target) {
                ("default-command" | "default-shell", TmuxOptionTarget::GlobalSession) => true,
                ("default-command", TmuxOptionTarget::Session(session)) => {
                    self.session_default_commands.contains_key(&session)
                }
                ("default-shell", TmuxOptionTarget::Session(session)) => {
                    self.session_default_shells.contains_key(&session)
                }
                _ => unreachable!("spawn strings have session scope"),
            };
            if already {
                return already_set_or_quiet(options, option);
            }
        }
        let value = if unset {
            None
        } else {
            Some(value.ok_or_else(|| {
                ServerError::InvalidCommand(format!("set-option {option} needs a value"))
            })?)
        };
        match (option, target) {
            ("default-command", TmuxOptionTarget::GlobalSession) => {
                if let Some(value) = value {
                    if options.has("-a") {
                        self.global_default_command.push_str(value);
                    } else {
                        value.clone_into(&mut self.global_default_command);
                    }
                } else {
                    self.global_default_command.clear();
                }
            }
            ("default-command", TmuxOptionTarget::Session(session)) => {
                if let Some(value) = value {
                    let next = if options.has("-a") {
                        format!(
                            "{}{value}",
                            self.session_default_commands
                                .get(&session)
                                .map(String::as_str)
                                .unwrap_or_default()
                        )
                    } else {
                        value.to_owned()
                    };
                    self.session_default_commands.insert(session, next);
                } else {
                    self.session_default_commands.remove(&session);
                }
            }
            ("default-shell", TmuxOptionTarget::GlobalSession) => {
                if let Some(value) = value {
                    let next = if options.has("-a") {
                        format!("{}{value}", self.global_default_shell)
                    } else {
                        value.to_owned()
                    };
                    if !default_shell_is_valid(&next) {
                        return Err(ServerError::InvalidCommand(format!(
                            "not a suitable shell: {next}"
                        )));
                    }
                    self.global_default_shell = next;
                } else {
                    DEFAULT_SHELL.clone_into(&mut self.global_default_shell);
                }
            }
            ("default-shell", TmuxOptionTarget::Session(session)) => {
                if let Some(value) = value {
                    let next = if options.has("-a") {
                        format!(
                            "{}{value}",
                            self.session_default_shells
                                .get(&session)
                                .map(String::as_str)
                                .unwrap_or_default()
                        )
                    } else {
                        value.to_owned()
                    };
                    if !default_shell_is_valid(&next) {
                        return Err(ServerError::InvalidCommand(format!(
                            "not a suitable shell: {next}"
                        )));
                    }
                    self.session_default_shells.insert(session, next);
                } else {
                    self.session_default_shells.remove(&session);
                }
            }
            _ => unreachable!("spawn strings have session scope"),
        }
        Ok(Execution::default())
    }

    fn set_behavior_time(
        &mut self,
        option: &str,
        value: Option<&str>,
        options: &Options,
        target: TmuxOptionTarget,
    ) -> Result<Execution, ServerError> {
        let unset = option_is_unset(options);
        if options.has("-o") && !unset {
            let already = match (option, target) {
                (
                    "display-time" | "initial-repeat-time" | "repeat-time",
                    TmuxOptionTarget::GlobalSession,
                ) => true,
                ("display-time", TmuxOptionTarget::Session(session)) => {
                    self.session_display_time_ms.contains_key(&session)
                }
                ("initial-repeat-time", TmuxOptionTarget::Session(session)) => {
                    self.session_initial_repeat_time_ms.contains_key(&session)
                }
                ("repeat-time", TmuxOptionTarget::Session(session)) => {
                    self.session_repeat_time_ms.contains_key(&session)
                }
                _ => unreachable!("behavior time has session scope"),
            };
            if already {
                return already_set_or_quiet(options, option);
            }
        }
        let maximum = if matches!(option, "initial-repeat-time" | "repeat-time") {
            MAX_REPEAT_TIME_MS
        } else {
            i32::MAX.cast_unsigned()
        };
        let default = match option {
            "initial-repeat-time" => DEFAULT_INITIAL_REPEAT_TIME_MS,
            "repeat-time" => DEFAULT_REPEAT_TIME_MS,
            _ => DEFAULT_DISPLAY_TIME_MS,
        };
        let parsed = if unset {
            default
        } else {
            parse_index_option(
                value.ok_or_else(|| ServerError::InvalidCommand("empty value".to_owned()))?,
                maximum,
            )?
        };
        match (option, target) {
            ("display-time", TmuxOptionTarget::GlobalSession) => {
                self.global_display_time_ms = parsed;
            }
            ("display-time", TmuxOptionTarget::Session(session)) => {
                if unset {
                    self.session_display_time_ms.remove(&session);
                } else {
                    self.session_display_time_ms.insert(session, parsed);
                }
            }
            ("initial-repeat-time", TmuxOptionTarget::GlobalSession) => {
                self.global_initial_repeat_time_ms = parsed;
            }
            ("initial-repeat-time", TmuxOptionTarget::Session(session)) => {
                if unset {
                    self.session_initial_repeat_time_ms.remove(&session);
                } else {
                    self.session_initial_repeat_time_ms.insert(session, parsed);
                }
            }
            ("repeat-time", TmuxOptionTarget::GlobalSession) => {
                self.global_repeat_time_ms = parsed;
            }
            ("repeat-time", TmuxOptionTarget::Session(session)) => {
                if unset {
                    self.session_repeat_time_ms.remove(&session);
                } else {
                    self.session_repeat_time_ms.insert(session, parsed);
                }
            }
            _ => unreachable!("behavior time has session scope"),
        }
        Ok(Execution::default())
    }
}

pub(super) fn is_native_option(option: &str) -> bool {
    NATIVE_OPTIONS.contains(&option)
}

pub(super) fn option_scope_matches_target(
    scope: TmuxOptionScope,
    target: TmuxOptionTarget,
) -> bool {
    matches!(
        (scope, target),
        (TmuxOptionScope::Server, TmuxOptionTarget::Server)
            | (
                TmuxOptionScope::Session,
                TmuxOptionTarget::GlobalSession | TmuxOptionTarget::Session(_)
            )
            | (
                TmuxOptionScope::Window,
                TmuxOptionTarget::GlobalWindow | TmuxOptionTarget::Window(_)
            )
            | (
                TmuxOptionScope::WindowPane,
                TmuxOptionTarget::GlobalWindow
                    | TmuxOptionTarget::Window(_)
                    | TmuxOptionTarget::Pane(_)
            )
    )
}

fn server_option_execution(option: ServerOption) -> Execution {
    let mux_option = match option {
        ServerOption::Backspace => {
            return Execution::effect(MuxEffect::TerminalKnobsChanged {
                window: None,
                pane: None,
            });
        }
        ServerOption::ExtendedKeys => MuxOptionKey::ExtendedKeys,
        ServerOption::FocusEvents => MuxOptionKey::FocusEvents,
        _ => return Execution::default(),
    };
    Execution::effect(MuxEffect::MuxOptionChanged {
        option: mux_option,
        session: None,
    })
}

pub(super) fn is_theme_option(name: &str) -> bool {
    name == "theme" || name.starts_with("dark-theme-") || name.starts_with("light-theme-")
}

fn status_option_execution(
    session: Option<SessionId>,
    changed: bool,
    rows_changed: bool,
) -> Execution {
    let mut effects = Vec::new();
    if changed {
        effects.push(MuxEffect::StatusFormatsChanged { session });
    }
    if rows_changed {
        effects.push(MuxEffect::StatusRowsChanged { session });
    }
    Execution {
        output: RawText::default(),
        effects,
    }
}

pub(super) fn tmux_option_is_implemented(option: TmuxOption) -> bool {
    option.default.is_some()
        || StatusOption::from_name(option.name).is_some()
        || WindowStatusOption::from_name(option.name).is_some()
        || ServerOption::from_name(option.name).is_some()
        || SessionOption::from_name(option.name).is_some()
        || WindowOption::from_name(option.name).is_some()
        || PaneOption::from_name(option.name).is_some()
}

pub(super) fn already_set_or_quiet(
    options: &Options,
    option: &str,
) -> Result<Execution, ServerError> {
    if options.has("-q") {
        Ok(Execution::default())
    } else {
        Err(ServerError::InvalidCommand(format!(
            "already set: {option}"
        )))
    }
}

pub(super) fn quiet_command_error(
    quiet: bool,
    message: impl Into<String>,
) -> Result<Execution, ServerError> {
    if quiet {
        Ok(Execution::default())
    } else {
        Err(ServerError::InvalidCommand(message.into()))
    }
}

fn validate_unimplemented_option_value(
    option: &str,
    value: Option<&str>,
) -> Result<(), ServerError> {
    let Some(value) = value else {
        return Ok(());
    };
    match option {
        "status-keys" if !matches!(value, "emacs" | "vi") => Err(ServerError::InvalidCommand(
            format!("unknown value: {value}"),
        )),
        "status-bg" if parse_tmux_colour(value).is_none() => {
            Err(ServerError::InvalidCommand(format!("bad colour: {value}")))
        }
        "status-style" if !valid_style(value) => Err(ServerError::InvalidCommand(format!(
            "invalid style: {value}"
        ))),
        "default-client-command"
            if !crate::parse_config("<set-option>", value)
                .diagnostics
                .is_empty() =>
        {
            Err(ServerError::InvalidCommand("syntax error".to_owned()))
        }
        _ => Ok(()),
    }
}
