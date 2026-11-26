use super::*;

impl<H: StatusHooks> StatusHooks for RowFormatHooks<'_, H> {
    fn copy_line_numbers(&mut self, context: &StatusContext, option: &str) -> Option<String> {
        self.inner.copy_line_numbers(context, option)
    }

    fn stable_option_lookups(&self) -> bool {
        self.inner.stable_option_lookups()
    }

    fn only_tmux_options(&self) -> bool {
        self.inner.only_tmux_options()
    }

    fn option_variable(&mut self, name: &str, context: &StatusContext) -> Option<RawText> {
        self.inner.option_variable(name, context)
    }

    fn tree_variable(&mut self, name: &str, context: &StatusContext) -> Option<Cow<'_, str>> {
        if ROW_CONTEXT_FORMATS.contains(&name) {
            Some(Cow::Owned(self.line.to_string()))
        } else {
            self.inner.tree_variable(name, context)
        }
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.inner.strftime(literal)
    }

    fn shell(&mut self, command: &str, tag: &FormatJobTag) -> String {
        self.inner.shell(command, tag)
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        if ROW_CONTEXT_FORMATS.contains(&name) {
            None
        } else {
            self.inner.variable(name, context)
        }
    }

    fn window_activity(&mut self, window: WindowId) -> u64 {
        self.inner.window_activity(window)
    }

    fn pane_activity(&mut self, pane: PaneId) -> u64 {
        self.inner.pane_activity(pane)
    }

    fn pane_search(
        &mut self,
        pane: Option<PaneId>,
        pattern: &str,
        regex: bool,
        ignore_case: bool,
    ) -> usize {
        self.inner.pane_search(pane, pattern, regex, ignore_case)
    }

    fn client_loop_rows(&mut self, context: &StatusContext) -> Vec<FormatClientRow> {
        self.inner.client_loop_rows(context)
    }

    fn client_environment_rows(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_environment_rows()
    }

    fn client_tty_term(&mut self) -> Option<Arc<TtyTerm>> {
        self.inner.client_tty_term()
    }

    fn client_terminal_environment(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_terminal_environment()
    }
}

impl<H: StatusHooks> StatusHooks for ShownOptionHooks<'_, H> {
    fn copy_line_numbers(&mut self, context: &StatusContext, option: &str) -> Option<String> {
        self.inner.copy_line_numbers(context, option)
    }

    fn stable_option_lookups(&self) -> bool {
        self.inner.stable_option_lookups()
    }

    fn only_tmux_options(&self) -> bool {
        self.inner.only_tmux_options()
    }

    fn option_variable(&mut self, name: &str, context: &StatusContext) -> Option<RawText> {
        self.inner.option_variable(name, context)
    }

    fn tree_variable(&mut self, name: &str, context: &StatusContext) -> Option<Cow<'_, str>> {
        match self.values.iter().find(|(key, _)| *key == name) {
            Some((_, value)) => Some(Cow::Borrowed(value.as_str())),
            None => self.inner.tree_variable(name, context),
        }
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.inner.strftime(literal)
    }

    fn shell(&mut self, command: &str, tag: &FormatJobTag) -> String {
        self.inner.shell(command, tag)
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        if self.values.iter().any(|(key, _)| *key == name) {
            None
        } else {
            self.inner.variable(name, context)
        }
    }

    fn window_activity(&mut self, window: WindowId) -> u64 {
        self.inner.window_activity(window)
    }

    fn pane_activity(&mut self, pane: PaneId) -> u64 {
        self.inner.pane_activity(pane)
    }

    fn pane_search(
        &mut self,
        pane: Option<PaneId>,
        pattern: &str,
        regex: bool,
        ignore_case: bool,
    ) -> usize {
        self.inner.pane_search(pane, pattern, regex, ignore_case)
    }

    fn client_loop_rows(&mut self, context: &StatusContext) -> Vec<FormatClientRow> {
        self.inner.client_loop_rows(context)
    }

    fn client_environment_rows(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_environment_rows()
    }

    fn client_tty_term(&mut self) -> Option<Arc<TtyTerm>> {
        self.inner.client_tty_term()
    }

    fn client_terminal_environment(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_terminal_environment()
    }
}

impl StatusHooks for ConfigConditionHooks<'_> {
    fn stable_option_lookups(&self) -> bool {
        self.inner.stable_option_lookups()
    }

    fn only_tmux_options(&self) -> bool {
        self.inner.only_tmux_options()
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.inner.strftime(literal)
    }

    fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
        String::new()
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        let option = if name.starts_with('@') {
            self.engine
                .server_user_options
                .get(name)
                .or_else(|| self.engine.global_window_user_options.get(name))
                .or_else(|| self.engine.global_session_user_options.get(name))
                .map(ToString::to_string)
        } else {
            self.engine.global_tmux_option_value(name)
        };
        option.or_else(|| {
            context.variable(name).is_none().then(|| {
                self.engine
                    .global_environment
                    .get(name)
                    .and_then(|entry| entry.value.as_ref().map(ToString::to_string))
            })?
        })
    }
}

impl<H: StatusHooks> StatusHooks for CommandItemHooks<'_, H> {
    fn copy_line_numbers(&mut self, context: &StatusContext, option: &str) -> Option<String> {
        self.inner.copy_line_numbers(context, option)
    }

    fn stable_option_lookups(&self) -> bool {
        self.inner.stable_option_lookups()
    }

    fn only_tmux_options(&self) -> bool {
        self.inner.only_tmux_options()
    }

    fn option_variable(&mut self, name: &str, context: &StatusContext) -> Option<RawText> {
        self.inner.option_variable(name, context)
    }

    fn tree_variable(&mut self, name: &str, context: &StatusContext) -> Option<Cow<'_, str>> {
        if !COMMAND_ITEM_CONTEXT_FORMATS.contains(&name) {
            return self.inner.tree_variable(name, context);
        }
        let command = self.command?;
        self.inner
            .tree_variable(name, context)
            .or(Some(Cow::Borrowed(command)))
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.inner.strftime(literal)
    }

    fn shell(&mut self, command: &str, tag: &FormatJobTag) -> String {
        self.inner.shell(command, tag)
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        self.inner.variable(name, context)
    }

    fn window_activity(&mut self, window: WindowId) -> u64 {
        self.inner.window_activity(window)
    }

    fn pane_activity(&mut self, pane: PaneId) -> u64 {
        self.inner.pane_activity(pane)
    }

    fn pane_search(
        &mut self,
        pane: Option<PaneId>,
        pattern: &str,
        regex: bool,
        ignore_case: bool,
    ) -> usize {
        self.inner.pane_search(pane, pattern, regex, ignore_case)
    }

    fn client_loop_rows(&mut self, context: &StatusContext) -> Vec<FormatClientRow> {
        self.inner.client_loop_rows(context)
    }

    fn client_environment_rows(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_environment_rows()
    }

    fn client_tty_term(&mut self) -> Option<Arc<TtyTerm>> {
        self.inner.client_tty_term()
    }

    fn client_terminal_environment(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_terminal_environment()
    }

    fn tree_entries(&mut self) -> Vec<(String, String)> {
        let mut entries = self.inner.tree_entries();
        if let Some(command) = self.command
            && !entries.iter().any(|(name, _)| name == "command")
        {
            entries.push(("command".to_owned(), command.to_owned()));
        }
        entries
    }
}

impl<H: StatusHooks> StatusHooks for ListCommandHooks<'_, H> {
    fn copy_line_numbers(&mut self, context: &StatusContext, option: &str) -> Option<String> {
        self.inner.copy_line_numbers(context, option)
    }

    fn stable_option_lookups(&self) -> bool {
        self.inner.stable_option_lookups()
    }

    fn only_tmux_options(&self) -> bool {
        self.inner.only_tmux_options()
    }

    fn option_variable(&mut self, name: &str, context: &StatusContext) -> Option<RawText> {
        self.inner.option_variable(name, context)
    }

    fn tree_variable(&mut self, name: &str, context: &StatusContext) -> Option<Cow<'_, str>> {
        if !LIST_COMMAND_CONTEXT_FORMATS.contains(&name) {
            return self.inner.tree_variable(name, context);
        }
        Some(Cow::Borrowed(match name {
            "command_list_name" => self.spec.name,
            "command_list_alias" => self.spec.aliases.first().copied().unwrap_or_default(),
            "command_list_usage" => self.spec.usage,
            _ => unreachable!(),
        }))
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.inner.strftime(literal)
    }

    fn shell(&mut self, command: &str, tag: &FormatJobTag) -> String {
        self.inner.shell(command, tag)
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        if LIST_COMMAND_CONTEXT_FORMATS.contains(&name) {
            None
        } else {
            self.inner.variable(name, context)
        }
    }

    fn pane_search(
        &mut self,
        pane: Option<PaneId>,
        pattern: &str,
        regex: bool,
        ignore_case: bool,
    ) -> usize {
        self.inner.pane_search(pane, pattern, regex, ignore_case)
    }

    fn client_loop_rows(&mut self, context: &StatusContext) -> Vec<FormatClientRow> {
        self.inner.client_loop_rows(context)
    }

    fn client_environment_rows(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_environment_rows()
    }

    fn client_tty_term(&mut self) -> Option<Arc<TtyTerm>> {
        self.inner.client_tty_term()
    }

    fn client_terminal_environment(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_terminal_environment()
    }
}

impl<H: StatusHooks> StatusHooks for ListKeyHooks<'_, H> {
    fn copy_line_numbers(&mut self, context: &StatusContext, option: &str) -> Option<String> {
        self.inner.copy_line_numbers(context, option)
    }

    fn stable_option_lookups(&self) -> bool {
        self.inner.stable_option_lookups()
    }

    fn only_tmux_options(&self) -> bool {
        self.inner.only_tmux_options()
    }

    fn option_variable(&mut self, name: &str, context: &StatusContext) -> Option<RawText> {
        let option_context = if context.session_id.is_empty()
            && context.window_id.is_empty()
            && context.pane_id.is_empty()
        {
            self.fallback
        } else {
            context
        };
        if let Some(value) = self.options.lookup(
            &option_context.session_id,
            &option_context.window_id,
            &option_context.pane_id,
            name,
        ) {
            return Some(value.into());
        }
        if name.starts_with('@') {
            return self
                .user_options
                .user_option_bytes(
                    &option_context.pane_id,
                    &option_context.window_id,
                    &option_context.session_id,
                    name,
                )
                .cloned();
        }
        if self.inner.only_tmux_options()
            && *LIST_KEY_FORMAT_NAMES_ARE_NOT_OPTIONS
            && (LIST_KEY_BINDING_CONTEXT_FORMATS.contains(&name)
                || LIST_KEY_SUMMARY_CONTEXT_FORMATS.contains(&name))
        {
            return None;
        }
        self.inner.option_variable(name, context)
    }

    fn tree_variable(&mut self, name: &str, context: &StatusContext) -> Option<Cow<'_, str>> {
        if !LIST_KEY_BINDING_CONTEXT_FORMATS.contains(&name)
            && !LIST_KEY_SUMMARY_CONTEXT_FORMATS.contains(&name)
        {
            return self.inner.tree_variable(name, context);
        }
        Some(match name {
            "notes_only" => Cow::Borrowed(if self.notes_only { "1" } else { "0" }),
            "key_repeat" => Cow::Borrowed(if self.binding.repeat { "1" } else { "0" }),
            "key_note" => Cow::Borrowed(self.binding.note.as_deref().unwrap_or_default()),
            "key_prefix" => Cow::Borrowed(self.prefix),
            "key_table" => Cow::Borrowed(self.table),
            "key_string" => Cow::Borrowed(self.key),
            "key_command" => Cow::Owned(format_key_command(self.binding)),
            "key_has_repeat" => Cow::Borrowed(if self.has_repeat { "1" } else { "0" }),
            "key_string_width" => Cow::Borrowed(self.key_width),
            "key_table_width" => Cow::Borrowed(self.table_width),
            _ => unreachable!(),
        })
    }

    fn strftime(&mut self, literal: &str) -> String {
        self.inner.strftime(literal)
    }

    fn shell(&mut self, command: &str, tag: &FormatJobTag) -> String {
        self.inner.shell(command, tag)
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        if LIST_KEY_BINDING_CONTEXT_FORMATS.contains(&name)
            || LIST_KEY_SUMMARY_CONTEXT_FORMATS.contains(&name)
        {
            None
        } else {
            self.inner.variable(name, context)
        }
    }

    fn pane_search(
        &mut self,
        pane: Option<PaneId>,
        pattern: &str,
        regex: bool,
        ignore_case: bool,
    ) -> usize {
        self.inner.pane_search(pane, pattern, regex, ignore_case)
    }

    fn client_loop_rows(&mut self, context: &StatusContext) -> Vec<FormatClientRow> {
        self.inner.client_loop_rows(context)
    }

    fn client_environment_rows(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_environment_rows()
    }

    fn client_tty_term(&mut self) -> Option<Arc<TtyTerm>> {
        self.inner.client_tty_term()
    }

    fn client_terminal_environment(&mut self) -> Vec<FormatEnvironRow> {
        self.inner.client_terminal_environment()
    }
}
