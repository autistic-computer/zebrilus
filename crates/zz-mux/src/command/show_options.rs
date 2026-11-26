use super::*;

impl MuxEngine {
    pub(super) fn show_options_format_target(
        &self,
        context: &ExecutionContext,
        options: &Options,
        force_window: bool,
    ) -> (ExecutionContext, FormatClient) {
        match options.value("-t") {
            Some(target) if force_window => self
                .resolve_window(Some(target), context.session, context.window)
                .ok()
                .and_then(|window| {
                    let pane = self.state.windows.get(&window)?.active_pane;
                    ExecutionContext::for_pane(&self.state, pane)
                })
                .map_or_else(
                    || (ExecutionContext::default(), FormatClient::NoClient),
                    |target| (target, context.target_format_client()),
                ),
            Some(target) => self
                .resolve_pane(Some(target), context.window, context.pane)
                .ok()
                .and_then(|pane| ExecutionContext::for_pane(&self.state, pane))
                .map_or_else(
                    || (ExecutionContext::default(), FormatClient::NoClient),
                    |target| (target, context.target_format_client()),
                ),
            None => (context.clone(), context.target_format_client()),
        }
    }

    pub(super) fn shown_options_execution(
        &self,
        context: &ExecutionContext,
        options: &Options,
        force_window: bool,
        lines: &ShownOptions,
        hooks: &mut impl StatusHooks,
    ) -> Execution {
        let Some(template) = options.value("-F").filter(|_| lines.values.is_none()) else {
            return Execution::output(shown_options_output(lines));
        };
        let (target, client) = self.show_options_format_target(context, options, force_window);
        let value_only = options.has("-v");
        let output = lines
            .rows
            .iter()
            .map(|row| {
                let values = row.context(value_only);
                let mut row_hooks = ShownOptionHooks {
                    inner: &mut *hooks,
                    values: &values,
                };
                self.expand_pane_format(template, &target, context.session, client, &mut row_hooks)
            })
            .collect::<Vec<_>>();
        Execution::output(printed_lines(&output))
    }

    pub(super) fn show_options(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        force_window: bool,
        argument_expansion: ShowOptionArgument,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let command = if force_window {
            "show-window-options"
        } else {
            "show-options"
        };
        let (options, positional) = parse_command_options(command, args)?;
        if positional.len() > 1 {
            return Err(ServerError::CommandParse(
                "show-options accepts at most one option".to_owned(),
            ));
        }
        let value_only = options.has("-v");
        let include_inherited = options.has("-A");
        let include_hooks = !force_window && options.has("-H");
        let Some(argument) = positional.first() else {
            let target = match self.resolve_user_option_target(context, &options, force_window) {
                Ok(target) => target,
                Err(_) if options.has("-q") => return Ok(Execution::default()),
                Err(error) => return Err(error),
            };
            let mut lines = ShownOptions::new(options.has("--json"));
            if let Some(values) = self.user_options_at_target(target) {
                for (name, value) in values {
                    push_shown_option(&mut lines, name, value, true, false, value_only);
                }
            }
            for option in tmux_options_for_listing(target, include_hooks) {
                if tmux_option_is_hook(option.name) {
                    if let Some((hook, inherited)) =
                        self.hook_array_readback(target, option.name, include_inherited)
                    {
                        push_shown_hook_option(
                            &mut lines,
                            option.name,
                            hook,
                            None,
                            inherited,
                            true,
                            value_only,
                        );
                    }
                } else if let Some(metadata) = tmux_stored_array(option.name) {
                    if let Some((array, inherited)) =
                        self.array_option_readback(target, option.name, include_inherited)
                    {
                        push_shown_array(
                            &mut lines,
                            option.name,
                            array,
                            None,
                            metadata.value == TmuxArrayValue::String,
                            inherited,
                            value_only,
                        );
                    }
                } else if let Some((value, inherited)) =
                    self.tmux_option_readback(option, target, include_inherited)?
                {
                    push_shown_option(
                        &mut lines,
                        option.name,
                        &value,
                        tmux_option_value_is_string(option),
                        inherited,
                        value_only,
                    );
                }
            }
            return Ok(self.shown_options_execution(
                context,
                &options,
                force_window,
                &lines,
                hooks,
            ));
        };

        let argument = match argument_expansion {
            ShowOptionArgument::Expand => {
                let target = self.show_options_format_target(context, &options, force_window);
                self.expand_pane_format(argument, &target.0, context.session, target.1, hooks)
            }
            ShowOptionArgument::HookAlreadyExpanded => argument.to_string(),
        };
        let parsed = match parse_tmux_option(&argument) {
            Ok(parsed) => parsed,
            Err(()) if options.has("-q") => return Ok(Execution::default()),
            Err(()) => {
                return Err(ServerError::InvalidCommand(format!(
                    "invalid option: {argument}"
                )));
            }
        };
        if parsed.name.starts_with('@') {
            let target = match self.resolve_user_option_target(context, &options, force_window) {
                Ok(target) => target,
                Err(_) if options.has("-q") => return Ok(Execution::default()),
                Err(error) => return Err(error),
            };
            if let Some((value, inherited)) =
                self.user_option_readback(target, parsed.name, include_inherited)
            {
                let mut lines = ShownOptions::new(options.has("--json"));
                let name = indexed_option_name(parsed.name, parsed.index.as_deref());
                push_shown_option(&mut lines, &name, value, true, inherited, value_only);
                if matches!(argument_expansion, ShowOptionArgument::HookAlreadyExpanded) {
                    let owner = self.user_option_owner(target, parsed.name);
                    self.stamp_hook_fires(owner, &mut lines);
                }
                return Ok(self.shown_options_execution(
                    context,
                    &options,
                    force_window,
                    &lines,
                    hooks,
                ));
            }
            if options.has("-q") {
                return Ok(Execution::default());
            }
            return Err(ServerError::InvalidCommand(format!(
                "invalid option: {argument}"
            )));
        }
        if is_native_option(parsed.name) {
            let (value, is_string) = self.native_option_readback(parsed.name);
            let mut lines = ShownOptions::new(options.has("--json"));
            let name = indexed_option_name(parsed.name, parsed.index.as_deref());
            push_shown_option(&mut lines, &name, &value, is_string, false, value_only);
            return Ok(self.shown_options_execution(
                context,
                &options,
                force_window,
                &lines,
                hooks,
            ));
        }
        let option = match match_tmux_option(parsed.name) {
            Ok(Some(option)) => option,
            Ok(None) | Err(()) if options.has("-q") => return Ok(Execution::default()),
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
        if option.is_array {
            if tmux_option_is_hook(option.name) || tmux_stored_array(option.name).is_some() {
                let target = match self.resolve_tmux_option_target(
                    context,
                    &options,
                    force_window,
                    option.scope,
                ) {
                    Ok(target) => target,
                    Err(_) if options.has("-q") => return Ok(Execution::default()),
                    Err(error) => return Err(error),
                };
                let requested = parsed.index.map(ArrayIndex::parse);
                let mut lines = ShownOptions::new(options.has("--json"));
                if tmux_option_is_hook(option.name) {
                    if let Some((hook, inherited)) =
                        self.hook_array_readback(target, option.name, include_inherited)
                    {
                        push_shown_hook_option(
                            &mut lines,
                            option.name,
                            hook,
                            requested.as_ref(),
                            inherited,
                            false,
                            value_only,
                        );
                    }
                } else if let Some((array, inherited)) =
                    self.array_option_readback(target, option.name, include_inherited)
                {
                    let metadata = tmux_stored_array(option.name).expect("stored array metadata");
                    push_shown_array(
                        &mut lines,
                        option.name,
                        array,
                        requested.as_ref(),
                        metadata.value == TmuxArrayValue::String,
                        inherited && (requested.is_some() || !array.is_empty()),
                        value_only,
                    );
                }
                if matches!(argument_expansion, ShowOptionArgument::HookAlreadyExpanded) {
                    self.stamp_hook_fires(target, &mut lines);
                }
                return Ok(self.shown_options_execution(
                    context,
                    &options,
                    force_window,
                    &lines,
                    hooks,
                ));
            }
            return Ok(Execution::default());
        }
        if !tmux_option_is_implemented(option) {
            return Ok(Execution::default());
        }
        let target =
            match self.resolve_tmux_option_target(context, &options, force_window, option.scope) {
                Ok(target) => target,
                Err(_) if options.has("-q") => return Ok(Execution::default()),
                Err(error) => return Err(error),
            };
        let Some((value, inherited)) =
            self.tmux_option_readback(option, target, include_inherited)?
        else {
            return Ok(Execution::default());
        };
        let mut lines = ShownOptions::new(options.has("--json"));
        let name = indexed_option_name(option.name, parsed.index.as_deref());
        push_shown_option(
            &mut lines,
            &name,
            &value,
            tmux_option_value_is_string(option),
            inherited,
            value_only,
        );
        if matches!(argument_expansion, ShowOptionArgument::HookAlreadyExpanded) {
            self.stamp_hook_fires(target, &mut lines);
        }
        Ok(self.shown_options_execution(context, &options, force_window, &lines, hooks))
    }

    pub(super) fn user_option_readback<'a>(
        &'a self,
        target: TmuxOptionTarget,
        name: &str,
        include_inherited: bool,
    ) -> Option<(&'a RawText, bool)> {
        if let Some(value) = self.user_option_at_target(target, name) {
            return Some((value, false));
        }
        if !include_inherited {
            return None;
        }
        let parent = match target {
            TmuxOptionTarget::Session(_) => self.global_session_user_options.get(name),
            TmuxOptionTarget::Window(_) => self.global_window_user_options.get(name),
            TmuxOptionTarget::Pane(pane) => self
                .state
                .window_for_pane(pane)
                .and_then(|window| self.window_user_options.get(&window))
                .and_then(|values| values.get(name))
                .or_else(|| self.global_window_user_options.get(name)),
            _ => None,
        }?;
        Some((parent, true))
    }

    pub(super) fn tmux_option_readback(
        &self,
        option: TmuxOption,
        target: TmuxOptionTarget,
        include_inherited: bool,
    ) -> Result<Option<(String, bool)>, ServerError> {
        if tmux_stored_scalar(option.name).is_some() {
            return Ok(self.scalar_option_readback(target, option.name, include_inherited));
        }
        let inherited = || {
            include_inherited
                .then(|| {
                    self.global_tmux_option_value(option.name)
                        .or_else(|| option.default.map(|default| default.value().to_owned()))
                })
                .flatten()
                .map(|value| (value, true))
        };
        let value = match target {
            TmuxOptionTarget::Server
            | TmuxOptionTarget::GlobalSession
            | TmuxOptionTarget::GlobalWindow => self
                .global_tmux_option_value(option.name)
                .or_else(|| option.default.map(|default| default.value().to_owned()))
                .map(|value| (value, false)),
            TmuxOptionTarget::Session(session) => match option.name {
                name if SessionOption::from_name(name).is_some() => {
                    let option = SessionOption::from_name(name).expect("guarded session option");
                    self.session_options
                        .get(&session)
                        .and_then(|values| values.get(&option))
                        .map(|value| (value.clone(), false))
                        .or_else(inherited)
                }
                name if StatusOption::from_name(name).is_some() => {
                    let option = StatusOption::from_name(name).expect("guarded status option");
                    self.session_status_options
                        .get(&session)
                        .and_then(|values| values.get(&option))
                        .map(|value| (value.clone(), false))
                        .or_else(inherited)
                }
                "lock-command" => self
                    .session_lock_commands
                    .get(&session)
                    .map(|value| (value.clone(), false))
                    .or_else(inherited),
                "lock-after-time" => self
                    .session_lock_after_times
                    .get(&session)
                    .map(|value| (value.to_string(), false))
                    .or_else(inherited),
                "default-command" => self
                    .session_default_commands
                    .get(&session)
                    .map(|value| (value.clone(), false))
                    .or_else(inherited),
                "default-shell" => self
                    .session_default_shells
                    .get(&session)
                    .map(|value| (value.clone(), false))
                    .or_else(inherited),
                "mouse" => self
                    .session_mouse
                    .get(&session)
                    .map(|value| (tmux_flag(*value).to_owned(), false))
                    .or_else(inherited),
                "display-time" => self
                    .session_display_time_ms
                    .get(&session)
                    .map(|value| (value.to_string(), false))
                    .or_else(inherited),
                "initial-repeat-time" => self
                    .session_initial_repeat_time_ms
                    .get(&session)
                    .map(|value| (value.to_string(), false))
                    .or_else(inherited),
                "repeat-time" => self
                    .session_repeat_time_ms
                    .get(&session)
                    .map(|value| (value.to_string(), false))
                    .or_else(inherited),
                "history-limit" => self
                    .session_history_limits
                    .get(&session)
                    .map(|value| (value.to_string(), false))
                    .or_else(inherited),
                "base-index" => self
                    .session_base_indices
                    .get(&session)
                    .map(|value| (value.to_string(), false))
                    .or_else(inherited),
                "renumber-windows" => self
                    .session_renumber_windows
                    .get(&session)
                    .map(|value| (tmux_flag(*value).to_owned(), false))
                    .or_else(inherited),
                "word-separators" => self
                    .session_word_separators
                    .get(&session)
                    .map(|value| (value.clone(), false))
                    .or_else(inherited),
                _ => inherited(),
            },
            TmuxOptionTarget::Window(window) => match option.name {
                name if WindowOption::from_name(name).is_some() => {
                    let option = WindowOption::from_name(name).expect("guarded window option");
                    self.window_options
                        .get(&window)
                        .and_then(|values| values.get(&option))
                        .map(|value| (value.clone(), false))
                        .or_else(inherited)
                }
                name if PaneOption::from_name(name).is_some() => {
                    let option = PaneOption::from_name(name).expect("guarded pane option");
                    self.window_pane_options
                        .get(&window)
                        .and_then(|values| values.get(&option))
                        .map(|value| (value.clone(), false))
                        .or_else(inherited)
                }
                name if WindowStatusOption::from_name(name).is_some() => {
                    let option =
                        WindowStatusOption::from_name(name).expect("guarded window status option");
                    self.window_status_options
                        .get(&window)
                        .and_then(|values| values.get(&option))
                        .map(|value| (value.clone(), false))
                        .or_else(inherited)
                }
                "aggressive-resize" => self
                    .state
                    .window_aggressive_resize_override(window)?
                    .map(|value| (tmux_flag(value).to_owned(), false))
                    .or_else(inherited),
                "automatic-rename" => self
                    .state
                    .window_automatic_rename_override(window)?
                    .map(|value| (tmux_flag(value).to_owned(), false))
                    .or_else(inherited),
                "automatic-rename-format" => self
                    .window_automatic_rename_formats
                    .get(&window)
                    .map(|value| (value.clone(), false))
                    .or_else(inherited),
                "remain-on-exit" => self
                    .window_remain_on_exit
                    .get(&window)
                    .map(|value| (value.as_str().to_owned(), false))
                    .or_else(inherited),
                "mode-keys" => self
                    .window_mode_keys
                    .get(&window)
                    .map(|value| (value.as_str().to_owned(), false))
                    .or_else(inherited),
                "pane-base-index" => self
                    .state
                    .window_pane_base_indices
                    .get(&window)
                    .map(|value| (value.to_string(), false))
                    .or_else(inherited),
                "synchronize-panes" => self
                    .state
                    .window_synchronize_override(window)?
                    .map(|value| (tmux_flag(value).to_owned(), false))
                    .or_else(inherited),
                "popup-style" => self
                    .window_popup_styles
                    .get(&window)
                    .map(|value| (value.clone(), false))
                    .or_else(inherited),
                "popup-border-style" => self
                    .window_popup_border_styles
                    .get(&window)
                    .map(|value| (value.clone(), false))
                    .or_else(inherited),
                "popup-border-lines" => self
                    .window_popup_border_lines
                    .get(&window)
                    .map(|value| (value.as_str().to_owned(), false))
                    .or_else(inherited),
                "menu-style" => self
                    .window_menu_styles
                    .get(&window)
                    .map(|value| (value.clone(), false))
                    .or_else(inherited),
                "menu-selected-style" => self
                    .window_menu_selected_styles
                    .get(&window)
                    .map(|value| (value.clone(), false))
                    .or_else(inherited),
                "menu-border-style" => self
                    .window_menu_border_styles
                    .get(&window)
                    .map(|value| (value.clone(), false))
                    .or_else(inherited),
                "menu-border-lines" => self
                    .window_menu_border_lines
                    .get(&window)
                    .map(|value| (value.as_str().to_owned(), false))
                    .or_else(inherited),
                _ => inherited(),
            },
            TmuxOptionTarget::Pane(pane) => match option.name {
                name if PaneOption::from_name(name).is_some() => {
                    let option = PaneOption::from_name(name).expect("guarded pane option");
                    self.pane_options
                        .get(&pane)
                        .and_then(|values| values.get(&option))
                        .map(|value| (value.clone(), false))
                        .or_else(|| {
                            include_inherited.then(|| (self.pane_knobs(pane).value(option), true))
                        })
                }
                "remain-on-exit" => self
                    .pane_remain_on_exit
                    .get(&pane)
                    .map(|value| (value.as_str().to_owned(), false))
                    .or_else(|| {
                        include_inherited.then(|| {
                            let window = self
                                .state
                                .window_for_pane(pane)
                                .expect("pane target was resolved");
                            (
                                self.remain_on_exit_for_window(window).as_str().to_owned(),
                                true,
                            )
                        })
                    }),
                "synchronize-panes" => self
                    .state
                    .pane_synchronize_override(pane)?
                    .map(|value| (tmux_flag(value).to_owned(), false))
                    .or_else(|| {
                        include_inherited.then(|| {
                            (
                                tmux_flag(
                                    self.state
                                        .pane_synchronize_panes(pane)
                                        .expect("pane target was resolved"),
                                )
                                .to_owned(),
                                true,
                            )
                        })
                    }),
                _ => None,
            },
        };
        Ok(value)
    }

    pub(super) fn global_tmux_option_value(&self, name: &str) -> Option<String> {
        if let Some(option) = ServerOption::from_name(name) {
            let value = self.server_options.value(option);
            return Some(if option == ServerOption::Backspace {
                tmux_key_display(&value)
            } else {
                value
            });
        }
        if let Some(option) = SessionOption::from_name(name) {
            return Some(self.global_session_options.value(option));
        }
        if let Some(option) = WindowOption::from_name(name) {
            return Some(self.global_window_options.value(option));
        }
        if let Some(option) = PaneOption::from_name(name) {
            return Some(self.global_pane_options.value(option));
        }
        if let Some(option) = StatusOption::from_name(name) {
            return Some(self.status.value(option));
        }
        if let Some(option) = WindowStatusOption::from_name(name) {
            return Some(self.window_status.value(option).to_owned());
        }
        Some(match name {
            "default-command" => self.global_default_command.clone(),
            "default-shell" => self.global_default_shell.clone(),
            "default-terminal" => self
                .default_terminal
                .clone()
                .unwrap_or_else(|| DEFAULT_TERMINAL.to_owned()),
            "escape-time" => self.escape_time_ms.to_string(),
            "base-index" => self.global_base_index.to_string(),
            "buffer-limit" => self.buffer_limit.to_string(),
            "copy-command" => self.copy_command.clone(),
            "history-limit" => self.global_history_limit.to_string(),
            "display-time" => self.global_display_time_ms.to_string(),
            "initial-repeat-time" => self.global_initial_repeat_time_ms.to_string(),
            "message-limit" => self.message_limit.to_string(),
            "mode-keys" => self.global_mode_keys.as_str().to_owned(),
            "mouse" => tmux_flag(self.global_mouse).to_owned(),
            "pane-base-index" => self.state.global_pane_base_index.to_string(),
            "prefix" => tmux_key_display(self.keys.prefix()),
            "renumber-windows" => tmux_flag(self.global_renumber_windows).to_owned(),
            "repeat-time" => self.global_repeat_time_ms.to_string(),
            "set-clipboard" => self.set_clipboard.as_str().to_owned(),
            "synchronize-panes" => tmux_flag(self.state.global_synchronize_panes()).to_owned(),
            "update-environment" => self
                .update_environment_names()
                .collect::<Vec<_>>()
                .join(" "),
            "word-separators" => self.global_word_separators.clone(),
            "aggressive-resize" => tmux_flag(self.state.global_aggressive_resize()).to_owned(),
            "automatic-rename" => tmux_flag(self.state.global_automatic_rename()).to_owned(),
            "automatic-rename-format" => self.global_automatic_rename_format.clone(),
            "remain-on-exit" => self.global_remain_on_exit.as_str().to_owned(),
            "popup-style" => self.global_popup_style.clone(),
            "popup-border-style" => self.global_popup_border_style.clone(),
            "popup-border-lines" => self.global_popup_border_lines.as_str().to_owned(),
            "menu-style" => self.global_menu_style.clone(),
            "menu-selected-style" => self.global_menu_selected_style.clone(),
            "menu-border-style" => self.global_menu_border_style.clone(),
            "menu-border-lines" => self.global_menu_border_lines.as_str().to_owned(),
            "lock-command" => self.global_lock_command.clone(),
            "lock-after-time" => self.global_lock_after_time.to_string(),
            name => {
                let target = match match_tmux_option(name).ok().flatten()?.scope {
                    TmuxOptionScope::Server => TmuxOptionTarget::Server,
                    TmuxOptionScope::Session => TmuxOptionTarget::GlobalSession,
                    TmuxOptionScope::Window | TmuxOptionScope::WindowPane => {
                        TmuxOptionTarget::GlobalWindow
                    }
                };
                return self
                    .scalar_option_effective(target, name)
                    .map(str::to_owned);
            }
        })
    }

    fn native_option_readback(&self, name: &str) -> (String, bool) {
        match name {
            "history-trickle" => (self.history_trickle.to_string(), false),
            "experimental-editor-pane" => {
                (tmux_flag(self.experimental_editor_pane).to_owned(), false)
            }
            "agent-command" => (self.agent.command.clone(), true),
            "agent-claude-code-command" => (self.agent.claude_code_command.clone(), true),
            "agent-auto-approve" => (self.agent.auto_approve.as_str().to_owned(), false),
            _ => unreachable!("native option is catalogued"),
        }
    }
}

static TMUX_OPTION_LISTINGS: LazyLock<[Vec<TmuxOption>; 12]> = LazyLock::new(|| {
    std::array::from_fn(|slot| {
        let include_hooks = slot % 2 == 1;
        let target = match slot / 2 {
            0 => TmuxOptionTarget::Server,
            1 => TmuxOptionTarget::GlobalSession,
            2 => TmuxOptionTarget::Session(SessionId(0)),
            3 => TmuxOptionTarget::GlobalWindow,
            4 => TmuxOptionTarget::Window(WindowId(0)),
            _ => TmuxOptionTarget::Pane(PaneId(0)),
        };
        let mut options = tmux_options()
            .filter(|option| option_scope_matches_target(option.scope, target))
            .filter(|option| {
                if tmux_option_is_hook(option.name) {
                    include_hooks
                } else {
                    tmux_stored_array(option.name).is_some()
                        || !option.is_array && tmux_option_is_implemented(*option)
                }
            })
            .collect::<Vec<_>>();
        options.sort_unstable_by_key(|option| tmux_option_table_order(option.name));
        options
    })
});

pub(super) fn tmux_options_for_listing(
    target: TmuxOptionTarget,
    include_hooks: bool,
) -> impl Iterator<Item = TmuxOption> {
    let slot = match target {
        TmuxOptionTarget::Server => 0,
        TmuxOptionTarget::GlobalSession => 1,
        TmuxOptionTarget::Session(_) => 2,
        TmuxOptionTarget::GlobalWindow => 3,
        TmuxOptionTarget::Window(_) => 4,
        TmuxOptionTarget::Pane(_) => 5,
    };
    TMUX_OPTION_LISTINGS[slot * 2 + usize::from(include_hooks)]
        .iter()
        .copied()
}

fn tmux_option_value_is_string(option: TmuxOption) -> bool {
    option
        .default
        .is_some_and(crate::tmux_options::TmuxOptionDefault::is_string)
        || StatusOption::from_name(option.name).is_some_and(StatusOption::is_string)
        || WindowStatusOption::from_name(option.name).is_some()
        || ServerOption::from_name(option.name).is_some_and(ServerOption::is_string)
        || SessionOption::from_name(option.name).is_some_and(SessionOption::is_string)
        || WindowOption::from_name(option.name).is_some_and(WindowOption::is_string)
        || PaneOption::from_name(option.name).is_some_and(PaneOption::is_string)
        || tmux_stored_scalar(option.name).is_some_and(|metadata| metadata.kind.is_string())
}

pub(super) fn push_shown_option(
    lines: &mut ShownOptions,
    name: &str,
    value: impl Into<RawText>,
    is_string: bool,
    inherited: bool,
    value_only: bool,
) {
    let value = value.into();
    lines.record(name, &value);
    lines.row(name, &value, is_string, inherited, true);
    if value_only {
        lines.lines.push(value);
        return;
    }
    let name = if inherited {
        format!("{name}*")
    } else {
        name.to_owned()
    };
    let value = if is_string {
        tmux_args_escape_bytes(value.as_bytes())
    } else {
        value.to_string()
    };
    lines.lines.push(format!("{name} {value}").into());
}

fn push_shown_array(
    lines: &mut ShownOptions,
    name: &str,
    array: &StringArray,
    requested: Option<&ArrayIndex>,
    is_string: bool,
    inherited: bool,
    value_only: bool,
) {
    if let Some(index) = requested {
        let name = indexed_option_name(name, Some(&index.display()));
        push_shown_option(
            lines,
            &name,
            array.get(index).map_or("", String::as_str),
            is_string,
            inherited,
            value_only,
        );
        return;
    }
    if array.is_empty() {
        lines.record(name, "");
        lines.row(name, "", is_string, inherited, false);
        if !value_only {
            lines.lines.push(
                if inherited {
                    format!("{name}*")
                } else {
                    name.to_owned()
                }
                .into(),
            );
        }
        return;
    }
    for (index, value) in array {
        let name = indexed_option_name(name, Some(&index.display()));
        push_shown_option(lines, &name, value, is_string, inherited, value_only);
    }
}

pub(super) fn printed_lines(lines: &[String]) -> String {
    let mut output = lines.join("\n");
    if lines.last().is_some_and(String::is_empty) {
        output.push('\n');
    }
    output
}

fn shown_options_output(lines: &ShownOptions) -> RawText {
    if let Some(values) = &lines.values {
        return serde_json::to_string(values)
            .expect("option values serialize")
            .into();
    }
    let mut output = RawText::join(&lines.lines, b"\n");
    if lines.lines.last().is_some_and(|line| line.is_empty()) {
        output.push_bytes(b"\n");
    }
    output
}

pub(super) fn indexed_option_name(name: &str, index: Option<&str>) -> String {
    index.map_or_else(|| name.to_owned(), |index| format!("{name}[{index}]"))
}

pub(super) fn tmux_args_escape(value: &str) -> String {
    tmux_args_escape_bytes(value.as_bytes())
}

pub(super) fn tmux_args_escape_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "''".to_owned();
    }
    let double_quoted = bytes.iter().any(|byte| b" #';${}%".contains(byte));
    let single_quoted = !double_quoted && bytes.iter().any(|byte| b" \"".contains(byte));
    if bytes.len() == 1 && bytes[0] != b' ' && (double_quoted || single_quoted || bytes[0] == b'~')
    {
        return format!("\\{}", char::from(bytes[0]));
    }

    let mut escaped = String::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        escaped.push_str(&tmux_vis(chunk.valid(), double_quoted));
        for byte in chunk.invalid() {
            write!(escaped, "\\{byte:03o}").expect("writing to a string cannot fail");
        }
    }
    if single_quoted {
        format!("'{escaped}'")
    } else if double_quoted {
        if escaped.starts_with('~') {
            format!("\"\\{escaped}\"")
        } else {
            format!("\"{escaped}\"")
        }
    } else if escaped.starts_with('~') {
        format!("\\{escaped}")
    } else {
        escaped
    }
}

pub(super) fn tmux_vis(value: &str, double_quoted: bool) -> String {
    let mut escaped = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if !character.is_ascii() {
            escaped.push(character);
            continue;
        }
        let byte = character as u8;
        if byte == b'$'
            && double_quoted
            && characters
                .peek()
                .is_some_and(|next| next.is_ascii_alphabetic() || matches!(*next, '_' | '{'))
        {
            escaped.push('\\');
            escaped.push('$');
            continue;
        }
        if byte == b'\\' || byte == b'"' && double_quoted {
            escaped.push('\\');
            escaped.push(character);
            continue;
        }
        if byte == b' ' || byte.is_ascii_graphic() {
            escaped.push(character);
            continue;
        }
        match byte {
            b'\n' => escaped.push_str("\\n"),
            b'\r' => escaped.push_str("\\r"),
            0x08 => escaped.push_str("\\b"),
            0x07 => escaped.push_str("\\a"),
            0x0b => escaped.push_str("\\v"),
            b'\t' => escaped.push_str("\\t"),
            0x0c => escaped.push_str("\\f"),
            0 => {
                escaped.push_str("\\0");
                if characters
                    .peek()
                    .is_some_and(|next| matches!(*next, '0'..='7'))
                {
                    escaped.push_str("00");
                }
            }
            _ => write!(escaped, "\\{byte:03o}").expect("writing to a string cannot fail"),
        }
    }
    escaped
}

pub(super) fn option_is_unset(options: &Options) -> bool {
    options.has("-u") || options.has("-U")
}
