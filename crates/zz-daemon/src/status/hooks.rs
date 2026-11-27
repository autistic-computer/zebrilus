use super::*;

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
pub(super) const PANE_OUTPUT_FORMATS: &[&str] = &[
    "pane_command_duration",
    "pane_command_end_time",
    "pane_command_running",
    "pane_command_start_time",
    "pane_command_status",
    "pane_last_output_time",
    "pane_last_prompt_time",
    "pane_output_generation",
    "pane_private_modes",
];

pub(super) fn pane_output_variable(name: &str, facts: &TerminalFacts, now: u64) -> Option<String> {
    let output = &facts.output;
    let time = |value: u64| (value != 0).then(|| value.to_string());
    match name {
        "pane_output_generation" => Some(output.output_generation.to_string()),
        "pane_last_output_time" => time(output.last_output_time),
        "pane_last_prompt_time" => time(output.last_prompt_time),
        "pane_command_start_time" => time(output.command_start_time),
        "pane_command_end_time" => time(output.command_end_time),
        "pane_command_running" => Some(u8::from(output.command_running).to_string()),
        "pane_command_duration" => output.command_duration(now).map(|value| value.to_string()),
        "pane_command_status" => output.command_status.map(|status| status.to_string()),
        "pane_private_modes" => {
            let modes = PRIVATE_MODE_NUMBERS
                .iter()
                .enumerate()
                .filter(|(bit, _)| facts.private_modes & (1 << bit) != 0)
                .map(|(_, number)| number.to_string())
                .collect::<Vec<_>>();
            (!modes.is_empty()).then(|| modes.join(","))
        }
        _ => None,
    }
}

impl<'a> DaemonFormatHooks<'a> {
    pub(crate) fn command(facts: &'a dyn FormatFactSource) -> Self {
        Self::command_with_optional_variables(facts, None)
    }

    pub(crate) fn command_with_optional_variables(
        facts: &'a dyn FormatFactSource,
        variables: Option<&'a BTreeMap<String, String>>,
    ) -> Self {
        Self {
            status_client: None,
            facts,
            buffer_override: None,
            option_engine: None,
            status_context: None,
            variables,
            command_item: None,
            option_snapshot: None,
            cache: None,
            touched: None,
            refresh: false,
            now: format_second(),
            environment: None,
            default_terminal: None,
            startup: false,
            tmux_shim: None,
            zz_executable: None,
            job_waker: None,
            job_client: None,
            facts_withheld: false,
            pane_in_mode_override: None,
            cycle: false,
            live_process_facts: false,
        }
    }

    pub(crate) const fn with_cycle(mut self) -> Self {
        self.cycle = true;
        self
    }

    pub(crate) const fn with_live_process_facts(mut self) -> Self {
        self.live_process_facts = true;
        self
    }

    pub(crate) fn command_with_variables(
        facts: &'a dyn FormatFactSource,
        variables: &'a BTreeMap<String, String>,
    ) -> Self {
        Self::command_with_optional_variables(facts, Some(variables))
    }

    pub(crate) fn with_buffer(mut self, buffer: BufferFormatFacts) -> Self {
        self.buffer_override = Some(buffer);
        self
    }

    pub(crate) fn set_pane_in_mode_count(&mut self, pane: PaneId, count: usize) {
        self.pane_in_mode_override = Some((pane, count));
    }

    fn buffer(&self) -> Option<&BufferFormatFacts> {
        self.buffer_override
            .as_ref()
            .or_else(|| self.facts.buffer())
    }

    pub(crate) fn with_option_engine(mut self, engine: &'a MuxEngine) -> Self {
        self.option_engine = Some(engine);
        self
    }

    pub(crate) fn with_command_item(mut self, command: &'a str) -> Self {
        self.command_item = Some(command);
        self
    }

    pub(super) fn with_variables(mut self, variables: &'a BTreeMap<String, String>) -> Self {
        self.variables = Some(variables);
        self
    }

    pub(super) fn status(
        client: ClientId,
        facts: &'a dyn FormatFactSource,
        context: &'a StatusContext<'a>,
        option_snapshot: Option<&'a StatusRowVariables>,
        cache: &'a mut BTreeMap<ShellCacheKey, ShellCacheEntry>,
        touched: &'a mut BTreeSet<ShellCacheKey>,
        refresh: bool,
        now: i64,
        environment: &'a [(RawText, Option<RawText>)],
        default_terminal: &'a str,
        startup: bool,
        tmux_shim: Option<&'a std::path::Path>,
        zz_executable: Option<&'a std::path::Path>,
        job_waker: Option<&'a std::task::Waker>,
        job_client: &'a crate::daemon::status_jobs::StatusClient,
    ) -> Self {
        Self {
            status_client: Some(client),
            facts,
            buffer_override: None,
            option_engine: None,
            status_context: Some(context),
            variables: None,
            command_item: None,
            option_snapshot,
            cache: Some(cache),
            touched: Some(touched),
            refresh,
            now,
            environment: Some(environment),
            default_terminal: Some(default_terminal),
            startup,
            tmux_shim,
            zz_executable,
            job_waker,
            job_client: Some(job_client),
            facts_withheld: false,
            pane_in_mode_override: None,
            cycle: false,
            live_process_facts: false,
        }
    }

    pub(crate) fn withhold_facts(mut self, withheld: bool) -> Self {
        self.facts_withheld = withheld;
        self
    }

    fn expect_facts(&self) {
        debug_assert!(
            !self.facts_withheld,
            "a command whose facts were withheld expanded a format"
        );
    }
}

impl DaemonFormatHooks<'_> {
    fn copy_mode_rows(
        &self,
        context: &StatusContext,
    ) -> Option<&Vec<(String, Arc<CopyModeFacts>)>> {
        self.facts.copy_modes().get(&context.pane_id.parse().ok()?)
    }

    fn copy_mode_view(&self, context: &StatusContext) -> Option<&CopyModeFacts> {
        let rows = self.copy_mode_rows(context)?;
        let client = self.facts.client().map(|client| client.name.as_str());
        client
            .and_then(|client| rows.iter().find(|(name, _)| name == client))
            .or_else(|| rows.first())
            .map(|(_, facts)| facts.as_ref())
    }

    fn pane_mode_name(&self, context: &StatusContext) -> Option<&'static str> {
        self.facts
            .pane_modes()
            .get(&context.pane_id.parse().ok()?)
            .map(|(_, name)| *name)
    }

    fn copy_mode_variable(
        &self,
        name: &str,
        context: &StatusContext,
        line_option: &str,
    ) -> Option<String> {
        let view = self.copy_mode_view(context)?;
        let [
            cursor_line,
            cursor_word,
            cursor_x,
            cursor_y,
            line_numbers,
            rectangle_toggle,
            refresh_active,
            scroll_position,
            search_count,
            search_count_partial,
            search_match,
            search_present,
            search_timed_out,
            selection_active,
            selection_end_x,
            selection_end_y,
            selection_present,
            selection_start_x,
            selection_start_y,
        ] = COPY_MODE_CONTEXT_FORMATS;
        if name == selection_present {
            return Some(
                if view.selection.is_some_and(|selection| {
                    selection.start_x != selection.end_x || selection.start_y != selection.end_y
                }) {
                    "1"
                } else {
                    "0"
                }
                .to_owned(),
            );
        }
        match name {
            _ if name == line_numbers => Some(
                u8::from(zz_terminal::copy_line_number_mode(view.line_numbers, line_option) != 0)
                    .to_string(),
            ),
            _ if name == refresh_active => Some(u8::from(view.refresh_active).to_string()),
            _ if name == rectangle_toggle => Some(u8::from(view.rectangle_toggle).to_string()),
            _ if name == selection_active => Some(u8::from(view.selection_active).to_string()),
            _ if name == search_present => Some(u8::from(view.search_present).to_string()),
            _ if name == search_timed_out => Some(u8::from(view.search_timed_out).to_string()),
            _ if name == search_match => Some(view.search_match.clone()),
            _ if name == search_count => Some(view.search_count?.0.to_string()),
            _ if name == search_count_partial => Some(u8::from(view.search_count?.1).to_string()),
            _ if name == cursor_line => Some(view.cursor_line.clone()),
            _ if name == cursor_word => Some(view.cursor_word.clone()),
            _ if name == cursor_x => Some(view.cursor_x.to_string()),
            _ if name == cursor_y => Some(view.cursor_y.to_string()),
            _ if name == scroll_position => Some(view.scroll_position.to_string()),
            _ if name == selection_start_x => Some(view.selection?.start_x.to_string()),
            _ if name == selection_start_y => Some(view.selection?.start_y.to_string()),
            _ if name == selection_end_x => Some(view.selection?.end_x.to_string()),
            _ if name == selection_end_y => Some(view.selection?.end_y.to_string()),
            _ => None,
        }
    }
}

pub(super) fn cycle_clock_ms() -> u64 {
    static ANCHOR: LazyLock<std::time::Instant> = LazyLock::new(std::time::Instant::now);
    u64::try_from(ANCHOR.elapsed().as_millis()).unwrap_or(u64::MAX)
}

impl StatusHooks for DaemonFormatHooks<'_> {
    fn cycle_clock(&mut self) -> Option<u64> {
        (self.status_client.is_some() || self.cycle).then(cycle_clock_ms)
    }

    fn cycle_animates(&mut self) {
        CYCLE_ANIMATED.set(true);
    }

    fn stable_option_lookups(&self) -> bool {
        true
    }

    fn only_tmux_options(&self) -> bool {
        true
    }

    #[cfg(unix)]
    #[allow(unsafe_code)]
    fn strftime(&mut self, literal: &str) -> String {
        let Ok(format) = std::ffi::CString::new(literal) else {
            return literal.to_owned();
        };
        let time: libc::time_t = TryInto::try_into(self.now).unwrap_or(0);
        let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
        if unsafe { libc::localtime_r(&raw const time, &raw mut tm) }.is_null() {
            return literal.to_owned();
        }
        let mut buffer = [0u8; 8192];
        let written = unsafe {
            libc::strftime(
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                format.as_ptr(),
                &raw const tm,
            )
        };
        String::from_utf8_lossy(&buffer[..written]).into_owned()
    }

    #[cfg(not(unix))]
    fn strftime(&mut self, literal: &str) -> String {
        let Ok(items) = chrono::format::StrftimeItems::new(literal).parse() else {
            return literal.to_owned();
        };
        let Some(now) = zz_mux::local_time(self.now) else {
            return literal.to_owned();
        };
        let mut formatted = String::with_capacity(literal.len());
        if write!(&mut formatted, "{}", now.format_with_items(items.iter())).is_err() {
            return literal.to_owned();
        }
        formatted
    }

    fn shell(&mut self, command: &str, tag: &FormatJobTag) -> String {
        let (Some(context), Some(_)) = (self.status_context, self.facts.client()) else {
            return String::new();
        };
        let mut expansion = DaemonFormatHooks::command(self.facts);
        expansion.option_snapshot = self.option_snapshot;
        let expanded = zz_mux::expand_format_values(command, context, &mut expansion);
        let (Some(cache), Some(touched)) = (self.cache.as_deref_mut(), self.touched.as_deref_mut())
        else {
            return String::new();
        };
        let Some(client) = self.status_client else {
            return String::new();
        };
        let cwd = status_working_directory(context);
        let key = (client, tag.clone(), command.to_owned());
        let first_reference = touched.insert(key.clone());
        let entry = cache.entry(key).or_default();
        let now = shell_second();
        let force = entry.expanded.as_deref() != Some(expanded.as_str())
            || (self.refresh && first_reference);
        if force || (entry.job.is_none() && entry.last != now) {
            entry.job = None;
            entry.expanded = Some(expanded.clone());
            entry.last = now;
            entry.job = run_shell(
                &expanded,
                context,
                &cwd,
                self.environment.unwrap_or_default(),
                self.default_terminal.unwrap_or("tmux-256color"),
                self.startup,
                self.tmux_shim,
                self.zz_executable,
                self.job_waker.cloned(),
                self.job_client.expect("status job client"),
            );
            if entry.job.is_none() {
                entry.set_output(format!("<'{command}' didn't start>"));
            }
        } else if entry.job.is_some()
            && now.saturating_sub(entry.last) > 1
            && entry.output.is_none()
        {
            entry.set_output(format!("<'{command}' not ready>"));
        }
        if !context.format_universe_covers(entry.output_needs) {
            return String::new();
        }
        zz_mux::expand_format_values(
            entry.output.as_deref().unwrap_or_default(),
            context,
            &mut expansion,
        )
    }

    fn client_loop_rows(&mut self, context: &StatusContext) -> Vec<FormatClientRow> {
        self.expect_facts();
        self.facts.clients(context).to_vec()
    }

    fn client_environment_rows(&mut self) -> Vec<FormatEnvironRow> {
        self.expect_facts();
        client_environment_rows(self.facts.client_environment())
    }

    fn client_tty_term(&mut self) -> Option<Arc<TtyTerm>> {
        self.expect_facts();
        let client = self.facts.client()?;
        let terminal = client.terminal.as_ref()?;
        if client.utf8 == "1" && !terminal.has_feature("utf8") {
            return Some(Arc::new(terminal.as_ref().clone().with_client_utf8(true)));
        }
        Some(Arc::clone(terminal))
    }

    fn client_terminal_environment(&mut self) -> Vec<FormatEnvironRow> {
        self.expect_facts();
        client_environment_rows(
            self.facts
                .client()
                .and_then(|client| client.environment.as_ref()),
        )
    }

    fn tree_entries(&mut self) -> Vec<(String, String)> {
        self.expect_facts();
        let mut entries = self
            .variables
            .into_iter()
            .flatten()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<Vec<_>>();
        if let Some(command) = self.command_item {
            entries.push(("command".to_owned(), command.to_owned()));
        }
        entries
    }

    fn option_variable(&mut self, name: &str, context: &StatusContext) -> Option<RawText> {
        self.expect_facts();
        self.option_engine
            .and_then(|engine| engine.format_option_value(context, name))
            .or_else(|| {
                self.option_snapshot.and_then(|options| {
                    options.lookup(
                        &context.session_id,
                        &context.window_id,
                        &context.pane_id,
                        name,
                    )
                })
            })
            .map(RawText::from)
            .or_else(|| {
                name.starts_with('@')
                    .then(|| {
                        context
                            .engine()
                            .and_then(|engine| {
                                engine.format_user_option_bytes(
                                    &context.pane_id,
                                    &context.window_id,
                                    &context.session_id,
                                    name,
                                )
                            })
                            .or_else(|| {
                                self.facts.mux().user_option_bytes(
                                    &context.pane_id,
                                    &context.window_id,
                                    &context.session_id,
                                    name,
                                )
                            })
                            .cloned()
                    })
                    .flatten()
            })
    }

    fn tree_variable(&mut self, name: &str, _context: &StatusContext) -> Option<Cow<'_, str>> {
        self.expect_facts();
        self.variables
            .and_then(|variables| variables.get(name))
            .map(|value| Cow::Borrowed(value.as_str()))
            .or_else(|| {
                (name == "command")
                    .then(|| self.command_item.map(Cow::Borrowed))
                    .flatten()
            })
    }

    fn copy_line_numbers(&mut self, context: &StatusContext, option: &str) -> Option<String> {
        self.expect_facts();
        self.copy_mode_variable("copy_line_numbers", context, option)
    }

    fn variable(&mut self, name: &str, context: &StatusContext) -> Option<String> {
        self.expect_facts();
        let [list_clients_line] = LIST_CLIENTS_CONTEXT_FORMATS;
        let [message_number, message_text, message_time] = SHOW_MESSAGES_CONTEXT_FORMATS;
        if COPY_MODE_CONTEXT_FORMATS.contains(&name) {
            let line_option = if name == "copy_line_numbers" {
                self.option_variable("copy-mode-line-numbers", context)
                    .map(String::from)
                    .unwrap_or_default()
            } else {
                String::new()
            };
            return self.copy_mode_variable(name, context, &line_option);
        }
        match name {
            "zz_version" => Some(env!("CARGO_PKG_VERSION").to_owned()),
            "zz_revision" => Some(zz_protocol::REVISION.to_string()),
            "zz_owned_version" => Some(
                zz_config::owned::installed()
                    .map(|copy| copy.info.version.to_string())
                    .unwrap_or_default(),
            ),
            "zz_owned_epoch" => Some(
                zz_config::owned::installed()
                    .map(|copy| copy.info.epoch.to_string())
                    .unwrap_or_default(),
            ),
            "zz_exit_rule" => Some(
                context
                    .engine()
                    .map_or("", |engine| {
                        if engine.exit_unattached_explicit() == Some(true) {
                            "exit-unattached"
                        } else if engine.exit_empty_explicit() == Some(true) {
                            "exit-empty"
                        } else {
                            ""
                        }
                    })
                    .to_owned(),
            ),
            "config_files" => self
                .variables
                .and_then(|variables| variables.get(name))
                .cloned(),
            "mouse_pane" | "mouse_x" | "mouse_y" | "mouse_word" | "mouse_line"
            | "mouse_hyperlink" => Some(
                self.variables
                    .and_then(|variables| variables.get(name))
                    .cloned()
                    .unwrap_or_default(),
            ),
            "pane_in_mode" => Some(
                context
                    .pane_id
                    .parse()
                    .ok()
                    .map_or(0, |pane| match self.pane_in_mode_override {
                        Some((target, count)) if target == pane => count,
                        _ => self.facts.pane_in_mode_count(pane),
                    })
                    .to_string(),
            ),
            "pane_search_string" => Some(
                context
                    .pane_id
                    .parse::<PaneId>()
                    .ok()
                    .and_then(|pane| self.facts.terminals().get(&pane))
                    .map(|terminal| terminal.pane_search_string())
                    .unwrap_or_default(),
            ),
            "pane_current_command"
                if self.live_process_facts
                    && context.pane_dead != Some(true)
                    && context
                        .engine()
                        .and_then(|engine| engine.format_pane_kind(&context.pane_id))
                        .or_else(|| self.facts.mux().pane_kind(&context.pane_id))
                        != Some("agent") =>
            {
                context
                    .pane_id
                    .parse::<PaneId>()
                    .ok()
                    .and_then(|pane| self.facts.terminals().get(&pane))
                    .map(|terminal| crate::daemon::terminal_current_command(terminal))
                    .filter(|command| !command.is_empty())
            }
            "pane_mode" => Some(if let Some(name) = self.pane_mode_name(context) {
                name.to_owned()
            } else if self.copy_mode_view(context)?.view_mode {
                "view-mode".to_owned()
            } else {
                "copy-mode".to_owned()
            }),
            "agent_state" | "agent_pending_permission" => {
                let pane_kind = context
                    .engine()
                    .and_then(|engine| engine.format_pane_kind(&context.pane_id))
                    .or_else(|| self.facts.mux().pane_kind(&context.pane_id));
                if name == "agent_state" && pane_kind == Some("terminal") {
                    return Some(
                        context
                            .engine()
                            .and_then(|engine| {
                                engine.format_user_option(
                                    &context.pane_id,
                                    &context.window_id,
                                    &context.session_id,
                                    "@agent_state",
                                )
                            })
                            .or_else(|| {
                                self.facts.mux().user_option(
                                    &context.pane_id,
                                    &context.window_id,
                                    &context.session_id,
                                    "@agent_state",
                                )
                            })
                            .unwrap_or_default()
                            .to_owned(),
                    );
                }
                let chat = context
                    .engine()
                    .and_then(|engine| engine.agent_chat(context.pane_id.parse().ok()?))
                    .or_else(|| self.facts.mux().agent_chat(&context.pane_id))
                    .filter(|_| pane_kind == Some("agent"));
                Some(match (name, chat) {
                    ("agent_state", Some(chat)) => chat.state.to_owned(),
                    (_, Some(chat)) => chat
                        .pending_permission
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                    (_, None) => String::new(),
                })
            }
            "agent_parent" => Some(
                context
                    .engine()
                    .and_then(|engine| {
                        engine
                            .agent_parent(context.pane_id.parse().ok()?)
                            .map(|parent| parent.to_string())
                    })
                    .or_else(|| {
                        self.facts
                            .mux()
                            .agent_parent(&context.pane_id)
                            .map(str::to_owned)
                    })
                    .unwrap_or_default(),
            ),
            "pane_kind" => context
                .engine()
                .and_then(|engine| engine.format_pane_kind(&context.pane_id))
                .or_else(|| self.facts.mux().pane_kind(&context.pane_id))
                .map(str::to_owned),
            "browser_url" => Some(
                context
                    .engine()
                    .and_then(|engine| engine.format_browser_url(&context.pane_id))
                    .or_else(|| self.facts.mux().browser_url(&context.pane_id))
                    .unwrap_or_default()
                    .to_owned(),
            ),
            "buffer_created" => Some(
                self.buffer()?
                    .created
                    .duration_since(UNIX_EPOCH)
                    .ok()?
                    .as_secs()
                    .to_string(),
            ),
            "buffer_full" => Some(buffer_full(&self.buffer()?.data)),
            "buffer_name" => Some(self.buffer()?.name.clone()),
            "buffer_sample" => Some(buffer_sample(&self.buffer()?.data)),
            "buffer_size" => Some(self.buffer()?.data.len().to_string()),
            "client_activity" => Some(self.facts.client()?.activity.clone()),
            "client_cell_height" => Some(self.facts.client()?.cell_height.clone()),
            "client_cell_width" => Some(self.facts.client()?.cell_width.clone()),
            "client_colours" => Some(self.facts.client()?.colours.clone()),
            "client_control_mode" => Some(self.facts.client()?.control_mode.clone()),
            "client_created" => Some(self.facts.client()?.created.clone()),
            "client_discarded" => Some(self.facts.client()?.discarded.clone()),
            "client_flags" => Some(self.facts.client()?.flags.clone()),
            "client_height" => Some(self.facts.client()?.height.clone()),
            "client_key_table" => Some(self.facts.client()?.key_table.clone()),
            "client_last_session" => Some(self.facts.client()?.last_session.clone())
                .filter(|session| !session.is_empty()),
            "client_name" => Some(self.facts.client()?.name.clone()),
            "client_pid" => Some(self.facts.client()?.pid.clone()),
            "client_prefix" => Some(self.facts.client()?.prefix.clone()),
            "client_readonly" => Some(self.facts.client()?.readonly.clone()),
            "client_session" => {
                let client = self.facts.client()?;
                if client.session.is_empty()
                    && context.variable("session_active").as_deref() == Some("1")
                {
                    context.variable("session_name").map(Cow::into_owned)
                } else {
                    Some(client.session.clone())
                }
            }
            "client_termfeatures" => Some(self.facts.client()?.termfeatures.clone()),
            "client_termname" => Some(self.facts.client()?.termname.clone()),
            "client_termtype" => Some(self.facts.client()?.termtype.clone()),
            "client_theme" => {
                Some(self.facts.client()?.theme.clone()).filter(|theme| !theme.is_empty())
            }
            "client_tty" => Some(self.facts.client()?.tty.clone()),
            "client_uid" => Some(self.facts.client()?.uid.clone()),
            "client_user" => Some(self.facts.client()?.user.clone()),
            "client_utf8" => Some(self.facts.client()?.utf8.clone()),
            "client_width" => Some(self.facts.client()?.width.clone()),
            "client_written" => Some(self.facts.client()?.written.clone()),
            "window_bigger" => Some(
                if self.facts.client()?.viewport?.bigger() {
                    "1"
                } else {
                    "0"
                }
                .to_owned(),
            ),
            "window_offset_x" => Some(self.facts.client()?.viewport?.offsets()?.0.to_string()),
            "window_offset_y" => Some(self.facts.client()?.viewport?.offsets()?.1.to_string()),
            "window_cell_height" => {
                window_scoped(context).then(|| window_cell_pixels(self.facts.client(), false))
            }
            "window_cell_width" => {
                window_scoped(context).then(|| window_cell_pixels(self.facts.client(), true))
            }
            _ if name == list_clients_line => Some(self.facts.client()?.line.to_string()),
            _ if name == message_number => Some(self.facts.message()?.number.to_string()),
            _ if name == message_text => Some(self.facts.message()?.text.clone()),
            _ if name == message_time => Some(
                self.facts
                    .message()?
                    .time
                    .duration_since(UNIX_EPOCH)
                    .ok()?
                    .as_secs()
                    .to_string(),
            ),
            "pane_pipe" => Some(
                if self
                    .facts
                    .pane_pipes()
                    .contains_key(&context.pane_id.parse().ok()?)
                {
                    "1"
                } else {
                    "0"
                }
                .to_owned(),
            ),
            "pane_unseen_changes" => Some(
                if self
                    .facts
                    .unseen_changes()
                    .contains(&context.pane_id.parse().ok()?)
                {
                    "1"
                } else {
                    "0"
                }
                .to_owned(),
            ),
            "pane_pipe_pid" => self
                .facts
                .pane_pipes()
                .get(&context.pane_id.parse().ok()?)
                .map(u32::to_string),
            "history_size" | "cursor_x" | "cursor_y" | "alternate_on" | "mouse_any_flag"
            | "cursor_flag" => {
                let pane = context.pane_id.parse().ok()?;
                let facts = self
                    .facts
                    .terminals()
                    .get(&pane)
                    .map(|terminal| terminal.facts())
                    .unwrap_or_default();
                Some(match name {
                    "history_size" => facts.history_size.to_string(),
                    "cursor_x" => facts.cursor_x.to_string(),
                    "cursor_y" => facts.cursor_y.to_string(),
                    "alternate_on" => u8::from(facts.alternate_on).to_string(),
                    "cursor_flag" => u8::from(!facts.cursor_hidden).to_string(),
                    _ => u8::from(facts.mouse_tracking).to_string(),
                })
            }
            "pane_output_generation"
            | "pane_last_output_time"
            | "pane_last_prompt_time"
            | "pane_command_start_time"
            | "pane_command_end_time"
            | "pane_command_running"
            | "pane_command_duration"
            | "pane_command_status"
            | "pane_private_modes" => {
                let pane = context.pane_id.parse().ok()?;
                let facts = self
                    .facts
                    .terminals()
                    .get(&pane)
                    .map(|terminal| terminal.facts())
                    .unwrap_or_default();
                pane_output_variable(name, &facts, unix_now())
            }
            "pane_last_command_status" => Some(
                context
                    .pane_id
                    .parse()
                    .ok()
                    .and_then(|pane| self.facts.terminals().get(&pane))
                    .and_then(|terminal| terminal.last_command_status())
                    .map_or_else(String::new, |status| status.to_string()),
            ),
            "pane_pb_progress" => Some(
                pane_progress_bar(self.facts, &context.pane_id)?
                    .progress
                    .to_string(),
            ),
            "pane_pb_state" => Some(
                pane_progress_bar(self.facts, &context.pane_id)?
                    .state
                    .as_str()
                    .to_owned(),
            ),
            "pane_status_reported" => Some(
                u8::from(pane_program_status_reported(self.facts, &context.pane_id)).to_string(),
            ),
            "pane_status"
            | "pane_status_kind"
            | "pane_status_progress"
            | "pane_status_app"
            | "pane_status_title"
            | "pane_status_message"
            | "pane_status_raw_title"
            | "pane_status_raw_message" => Some(
                pane_program_status(self.facts, &context.pane_id)
                    .map(|record| match name {
                        "pane_status" => record.state.as_str().to_owned(),
                        "pane_status_kind" => {
                            record.kind.map_or("", ProgramBlockKind::as_str).to_owned()
                        }
                        "pane_status_progress" => record
                            .progress
                            .map(|progress| progress.to_string())
                            .unwrap_or_default(),
                        "pane_status_app" => record.app,
                        "pane_status_title" => record.title.replace('#', "##"),
                        "pane_status_message" => record.message.replace('#', "##"),
                        "pane_status_raw_title" => record.title,
                        _ => record.message,
                    })
                    .unwrap_or_default(),
            ),
            "window_active_clients" => Some(
                self.facts
                    .window_clients(context)
                    .get(&context.window_id.parse().ok()?)
                    .map_or(0, Vec::len)
                    .to_string(),
            ),
            "window_active_clients_list" => Some(
                self.facts
                    .window_clients(context)
                    .get(&context.window_id.parse().ok()?)
                    .map_or_else(String::new, |names| names.join(",")),
            ),
            "session_attached" => Some(
                self.facts
                    .session_attachments()
                    .get(&context.session_id.parse().ok()?)
                    .map_or(0, |(count, _)| *count)
                    .to_string(),
            ),
            "session_attached_list" => Some(
                self.facts
                    .session_attachments()
                    .get(&context.session_id.parse().ok()?)
                    .map_or_else(String::new, |(_, names)| names.clone()),
            ),
            "session_many_attached" => Some(
                if self
                    .facts
                    .session_attachments()
                    .get(&context.session_id.parse().ok()?)
                    .is_some_and(|(count, _)| *count > 1)
                {
                    "1"
                } else {
                    "0"
                }
                .to_owned(),
            ),
            "session_last_attached" => self
                .facts
                .session_last_attached()
                .get(&context.session_id.parse().ok()?)
                .copied()
                .filter(|time| *time != 0)
                .map(|time| time.to_string()),
            _ => None,
        }
    }

    fn pane_search(
        &mut self,
        pane: Option<PaneId>,
        pattern: &str,
        regex: bool,
        ignore_case: bool,
    ) -> usize {
        self.expect_facts();
        let Some(viewport) = pane
            .and_then(|pane| self.facts.terminals().get(&pane))
            .map(|terminal| terminal.latest_viewport())
        else {
            return 0;
        };
        search_viewport(&viewport, pattern, regex, ignore_case)
    }
}

fn pane_progress_bar(facts: &dyn FormatFactSource, pane: &str) -> Option<ProgressBar> {
    let pane = pane.parse().ok()?;
    Some(
        facts
            .terminals()
            .get(&pane)
            .map(|terminal| terminal.progress_bar())
            .unwrap_or_default(),
    )
}

fn pane_program_status_reported(facts: &dyn FormatFactSource, pane: &str) -> bool {
    pane.parse()
        .ok()
        .and_then(|pane| facts.terminals().get(&pane))
        .is_some_and(|terminal| terminal.program_status().reported())
}

fn pane_program_status(facts: &dyn FormatFactSource, pane: &str) -> Option<ProgramStatusRecord> {
    facts
        .terminals()
        .get(&pane.parse().ok()?)?
        .program_status()
        .headline()
}

pub(super) fn buffer_full(data: &[u8]) -> String {
    String::from_utf8_lossy(data).into_owned()
}

pub(super) fn buffer_sample(data: &[u8]) -> String {
    let mut output = vis_escape(&data[..data.len().min(200)]);
    let shortened = data.len() > 200 || output.len() > 200;
    if output.len() > 200 {
        let boundary = (0..=200)
            .rev()
            .find(|index| output.is_char_boundary(*index))
            .unwrap_or_default();
        output.truncate(boundary);
    }
    if shortened {
        output.push_str("...");
    }
    output
}

pub(crate) fn vis_escape(prefix: &[u8]) -> String {
    let mut output = String::new();
    let mut index = 0;
    while index < prefix.len() {
        let byte = prefix[index];
        match byte {
            b'\n' => output.push_str("\\n"),
            b'\r' => output.push_str("\\r"),
            b'\t' => output.push_str("\\t"),
            b'\x08' => output.push_str("\\b"),
            b'\x07' => output.push_str("\\a"),
            b'\x0b' => output.push_str("\\v"),
            b'\x0c' => output.push_str("\\f"),
            b'\\' => output.push_str("\\\\"),
            0 if prefix
                .get(index + 1)
                .is_some_and(|next| matches!(*next, b'0'..=b'7')) =>
            {
                output.push_str("\\000");
            }
            0 => output.push_str("\\0"),
            0x20..=0x7e => output.push(char::from(byte)),
            0x80..=0xff => {
                let valid = match std::str::from_utf8(&prefix[index..]) {
                    Ok(value) => value,
                    Err(error) if error.valid_up_to() > 0 => {
                        std::str::from_utf8(&prefix[index..index + error.valid_up_to()])
                            .expect("UTF-8 validator identified a valid prefix")
                    }
                    Err(_) => {
                        let _ = write!(&mut output, "\\{byte:03o}");
                        index += 1;
                        continue;
                    }
                };
                let character = valid.chars().next().expect("valid UTF-8 is nonempty");
                output.push(character);
                index += character.len_utf8();
                continue;
            }
            _ => {
                let _ = write!(&mut output, "\\{byte:03o}");
            }
        }
        index += 1;
    }
    output
}

fn search_viewport(
    viewport: &TerminalViewport,
    pattern: &str,
    regex: bool,
    ignore_case: bool,
) -> usize {
    let regex = regex.then(|| {
        RegexBuilder::new(pattern)
            .case_insensitive(ignore_case)
            .build()
            .ok()
    });
    if matches!(regex, Some(None)) {
        return 0;
    }
    let glob = regex
        .is_none()
        .then(|| Pattern::new(&format!("*{pattern}*")).ok());
    for row in 0..viewport.rows {
        let mut text = String::with_capacity(usize::from(viewport.columns));
        for cell in viewport.row(row).unwrap_or_default() {
            if matches!(cell.width(), CellWidth::SpacerHead | CellWidth::SpacerTail) {
                continue;
            }
            if cell.glyph() == 0 {
                text.push(' ');
            } else {
                viewport.push_glyph(*cell, &mut text);
            }
        }
        let text = text.trim_end_matches(|character: char| character.is_ascii_whitespace());
        let matched = if let Some(Some(regex)) = &regex {
            regex.is_match(text)
        } else if let Some(Some(glob)) = &glob {
            glob.matches_with(
                text,
                MatchOptions {
                    case_sensitive: !ignore_case,
                    require_literal_separator: false,
                    require_literal_leading_dot: false,
                },
            )
        } else {
            false
        };
        if matched {
            return usize::from(row) + 1;
        }
    }
    0
}
