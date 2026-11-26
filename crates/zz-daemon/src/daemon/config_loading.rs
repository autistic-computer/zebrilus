use super::*;

impl Shared {
    pub(super) fn try_deliver_startup_config_causes(
        self: &Arc<Self>,
        client: ClientId,
        outbound: &Arc<OutboundMailbox>,
        registration_owner: bool,
    ) -> StartupConfigDelivery {
        let Some((kind, pane)) = self.startup_config_delivery_target(client, registration_owner)
        else {
            return StartupConfigDelivery::Ineligible;
        };
        let mut pending = self.startup_config_causes.lock();
        let Some(causes) = pending.as_ref() else {
            return StartupConfigDelivery::NoPending;
        };
        let delivery = self.deliver_startup_config_causes(client, kind, pane, causes, outbound);
        if delivery.delivered() {
            pending.take();
        }
        delivery
    }

    pub(super) fn startup_config_delivery_target(
        &self,
        client: ClientId,
        registration_owner: bool,
    ) -> Option<(ClientKind, Option<PaneId>)> {
        let (kind, pane, attached) = {
            let inner = self.inner.lock();
            (
                inner.client(client).and_then(|c| c.kind),
                client_context_pane(&inner, client),
                client_attached_session(&inner, client).is_some(),
            )
        };
        let eligible = matches!(kind, Some(ClientKind::Interactive)) && attached
            || matches!(kind, Some(ClientKind::Control)) && (registration_owner || attached);
        if !eligible {
            return None;
        }
        kind.map(|kind| (kind, pane))
    }

    pub(super) fn deliver_startup_config_causes(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        pane: Option<PaneId>,
        causes: &[String],
        outbound: &Arc<OutboundMailbox>,
    ) -> StartupConfigDelivery {
        match kind {
            ClientKind::Control
                if outbound.enqueue_reliable(&Self::event(EventPayload::StartupConfigCauses {
                    causes: causes.to_vec(),
                })) =>
            {
                StartupConfigDelivery::Delivered
            }
            ClientKind::Interactive => {
                let text = startup_config_preview(causes);
                match self.open_startup_command_output(
                    client,
                    pane,
                    "configuration errors".to_owned(),
                    &text,
                    outbound,
                ) {
                    Ok(output_id) => StartupConfigDelivery::InteractiveOutput(output_id),
                    Err(error) => {
                        log::warn!(
                            "could not deliver startup configuration errors to {client}: {error}"
                        );
                        StartupConfigDelivery::AdmissionFailed
                    }
                }
            }
            ClientKind::Control | ClientKind::Command => StartupConfigDelivery::AdmissionFailed,
        }
    }

    pub(super) fn publish_control_source_read_error(
        &self,
        target: (ClientId, u8),
        pane: Option<PaneId>,
        kind: ErrorKind,
        text: String,
    ) {
        let client = target.0;
        if target.1 == CONTROL_COMMAND_FRAME_FLAGS_CONTROL {
            self.inner
                .lock()
                .client_entry(client)
                .command_streams
                .get_or_insert_default()
                .exit_code = 1;
        }
        let payload = if kind == ErrorKind::InvalidData {
            EventPayload::ClientMessage {
                pane,
                kind: ClientMessageKind::Error,
                text,
            }
        } else {
            EventPayload::ControlSourceFile {
                event: ControlSourceFileEvent::ReadError(text),
            }
        };
        self.publish_to_client(client, payload);
    }

    pub(super) fn publish_control_source_complete(&self, target: Option<(ClientId, u8)>) {
        if let Some((client, _)) = target {
            self.publish_to_client(
                client,
                EventPayload::ControlSourceFile {
                    event: ControlSourceFileEvent::Complete,
                },
            );
        }
    }

    pub(super) fn publish_sourced_config_warning(
        &self,
        client: Option<ClientId>,
        command: &CommandInvocation,
        message: &str,
    ) {
        if let Some(client) = client {
            self.publish_to_client(
                client,
                EventPayload::ControlConfigError {
                    text: config_command_error(command, message),
                },
            );
        }
    }

    pub(super) fn publish_deferred_control_config_warnings(
        &self,
        warnings: Vec<DeferredControlConfigWarning>,
    ) {
        for warning in warnings {
            self.publish_to_client(
                warning.client,
                EventPayload::ControlConfigError { text: warning.text },
            );
        }
    }

    pub(super) fn route_source_error(
        &self,
        client: ClientId,
        kind: ClientKind,
        pane: Option<PaneId>,
        error: &str,
    ) {
        match kind {
            ClientKind::Interactive => {
                let mut text = error.to_owned();
                uppercase_first(&mut text);
                self.publish_to_client(
                    client,
                    EventPayload::ClientMessage {
                        pane,
                        kind: ClientMessageKind::Warning,
                        text,
                    },
                );
            }
            ClientKind::Command => {
                self.record_command_stderr(client, error);
                self.record_command_failure(client);
            }
            ClientKind::Control => {
                let mut inner = self.inner.lock();
                let streams = inner
                    .client_entry(client)
                    .command_streams
                    .get_or_insert_default();
                if !streams.control_error.is_empty() {
                    streams.control_error.push('\n');
                }
                streams.control_error.push_str(error);
            }
        }
    }

    fn route_source_verbose(
        &self,
        client: ClientId,
        kind: ClientKind,
        pane: Option<PaneId>,
        lines: &[String],
    ) {
        match kind {
            ClientKind::Interactive => {
                for line in lines {
                    self.publish_to_client(
                        client,
                        EventPayload::ClientMessage {
                            pane,
                            kind: ClientMessageKind::Info,
                            text: line.clone(),
                        },
                    );
                }
            }
            ClientKind::Command => {
                for line in lines {
                    self.record_command_stdout(client, line);
                }
            }
            ClientKind::Control => {}
        }
    }

    pub(super) fn route_config_replay_errors(
        &self,
        client: ClientId,
        kind: ClientKind,
        pane: Option<PaneId>,
        report: &mut ConfigLoadReport,
    ) {
        let delivered = report.delivered_replay_issues;
        for issue in &report.replay_issues()[delivered..] {
            match issue {
                ConfigReplayIssue::SourceErrors(index) => {
                    let errors = &report.source_error_groups()[*index];
                    if kind == ClientKind::Control {
                        self.inner
                            .lock()
                            .client_entry(client)
                            .command_streams
                            .get_or_insert_default()
                            .exit_code = 1;
                        if !report.control_guarded {
                            self.publish_to_client(
                                client,
                                EventPayload::ClientMessage {
                                    pane,
                                    kind: ClientMessageKind::Error,
                                    text: errors.join("\n"),
                                },
                            );
                        }
                    } else {
                        for error in errors {
                            self.route_source_error(client, kind, pane, error);
                        }
                    }
                }
                ConfigReplayIssue::CommandError { source, message } => {
                    if let Some(source) = source {
                        log::debug!(
                            "routing sourced command error from {}:{}",
                            source.source,
                            source.line
                        );
                    }
                    if kind == ClientKind::Control {
                        self.inner
                            .lock()
                            .client_entry(client)
                            .command_streams
                            .get_or_insert_default()
                            .exit_code = 1;
                        if !report.control_guarded {
                            self.publish_to_client(
                                client,
                                EventPayload::ClientMessage {
                                    pane,
                                    kind: ClientMessageKind::Error,
                                    text: message.clone(),
                                },
                            );
                        }
                    } else {
                        self.route_source_error(client, kind, pane, message);
                    }
                }
            }
        }
        report.delivered_replay_issues = report.replay_issues().len();
    }

    pub(super) fn import_tmux_configuration(
        &self,
        donor: Option<&str>,
        context: &ExecutionContext,
    ) -> Result<String, DaemonError> {
        let source = resolve_tmux_import_path(donor, context)?;
        let target = self
            .zz_mux_config_path
            .lock()
            .clone()
            .or_else(mux_config_write_path)
            .ok_or_else(|| {
                ServerError::InvalidCommand("no zz/mux.conf path available".to_owned())
            })?;
        let donor_text = self.helpers.import_source(&source).map_err(|error| {
            ServerError::InvalidCommand(match error {
                DaemonError::Io(error) => format!("{}: {}", source.display(), error.kind()),
                error => error.to_string(),
            })
        })?;
        let (copied, commands, unsupported) =
            prepare_tmux_import(&self.inner.lock().engine, &source, &donor_text);
        let existing = match self.helpers.import_source(&target) {
            Ok(source) => source,
            Err(DaemonError::Io(error)) if error.kind() == ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error),
        };
        let edited = replace_tmux_import(&existing, &source, &copied)?;
        zz_daemon_client::atomic_write(&target, edited.as_bytes())?;
        let names = if unsupported.is_empty() || unsupported.len() > 5 {
            String::new()
        } else {
            format!(" ({})", unsupported.join(", "))
        };
        Ok(format!(
            "Imported {} into {}: {commands} commands copied, {} commented out as unsupported{names}",
            source.display(),
            target.display(),
            unsupported.len()
        ))
    }

    pub(super) fn selected_mux_config_files(&self) -> Vec<PathBuf> {
        let selection = self.mux_config_selection.lock();
        startup_mux_config_files(selection.0, selection.1.as_deref(), || {
            self.zz_mux_config_path
                .lock()
                .clone()
                .or_else(default_mux_config)
        })
    }

    pub(super) fn replay_mux_config_files(
        self: &Arc<Self>,
        files: &[PathBuf],
        context: &mut ExecutionContext,
        report: &mut ConfigLoadReport,
        terminal: ClientTerminal,
        source_client_base: Option<&Path>,
        options: SourceFileLoadOptions,
    ) -> Result<(), DaemonError> {
        self.inner.lock().config.config_files = format_config_files(files);
        let explicit = self.mux_config_selection.lock().1.is_some();
        let mut parsed_roots = Vec::new();
        for config in files {
            let parsed = if report.startup_causes.is_some() {
                self.parse_startup_config_file(config, report, explicit)
            } else {
                self.parse_config_file(config, report, options, true)?
            };
            if let Some(parsed) = parsed {
                parsed_roots.push((config, parsed));
            }
        }
        let mut source_invocations = SourceInvocationAccounting::Startup { used: 0 };
        let mut warnings = Vec::new();
        let result = (|| {
            for (config, parsed) in parsed_roots {
                self.replay_config_file(
                    config,
                    parsed,
                    context,
                    0,
                    report,
                    terminal,
                    source_client_base,
                    &mut source_invocations,
                    options,
                    &mut warnings,
                )?;
            }
            Ok(())
        })();
        self.publish_deferred_control_config_warnings(warnings);
        result
    }

    pub(super) fn reload_user_config_with_source_base(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
        source_client_base: Option<&Path>,
        options: SourceFileLoadOptions,
    ) -> Result<(), DaemonError> {
        self.reload_user_config_with_files_and_source_base(
            client,
            context,
            &self.selected_mux_config_files(),
            source_client_base,
            options,
        )
    }

    #[cfg(test)]
    pub(super) fn reload_user_config_with_mux_file(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
        mux_config: Option<&Path>,
    ) -> Result<(), DaemonError> {
        self.reload_user_config_with_files_and_source_base(
            client,
            context,
            &mux_config
                .into_iter()
                .map(Path::to_owned)
                .collect::<Vec<_>>(),
            None,
            SourceFileLoadOptions::default(),
        )
    }

    pub(super) fn reload_user_config_with_files_and_source_base(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
        config_files: &[PathBuf],
        source_client_base: Option<&Path>,
        options: SourceFileLoadOptions,
    ) -> Result<(), DaemonError> {
        let (kind, color_scheme, appearance_config_overrides, config_client_terminal) = {
            let inner = self.inner.lock();
            let kind = inner
                .client(client)
                .and_then(|c| c.kind)
                .unwrap_or(ClientKind::Command);
            (
                kind,
                inner
                    .client(client)
                    .and_then(|c| c.color_scheme)
                    .unwrap_or(inner.config.active_color_scheme),
                inner.config.appearance_config_overrides.clone(),
                client_terminal(&inner, client, kind),
            )
        };
        let load = resolve_appearance(color_scheme, &appearance_config_overrides);
        log_appearance_load("reload", &load);
        self.inner.lock().engine.keys = KeyTables::default();
        let mut report = ConfigLoadReport::default();
        self.replay_mux_config_files(
            config_files,
            context,
            &mut report,
            config_client_terminal,
            source_client_base,
            options,
        )?;
        self.apply_stored_mux_config_overrides("reload-mux-replay");
        self.route_source_verbose(client, kind, context.pane, report.verbose_lines());

        let appearance = Arc::new(load.appearance);
        let provenance = load.provenance;
        let (terminals, published) = {
            let mut inner = self.inner.lock();
            if inner.config.appearance_config_overrides != appearance_config_overrides {
                return Ok(());
            }
            inner.config.active_color_scheme = color_scheme;
            inner.config.appearance = Arc::clone(&appearance);
            inner.config.appearance_provenance.clone_from(&provenance);
            let published = published_appearance(&inner);
            inner.config.published_style_appearance = Some(Arc::clone(&published));
            (terminal_appearance_updates(&inner), published)
        };
        for (terminal, pane_appearance) in terminals {
            terminal.set_appearance(pane_appearance);
        }
        self.publish(EventPayload::AppearanceChanged {
            appearance: Box::new((*published).clone()),
            provenance,
        });
        self.route_config_replay_errors(client, kind, context.pane, &mut report);
        if kind == ClientKind::Control && report.control_guarded {
            if let Some(text) = report.skipped_summary() {
                self.publish_to_client(
                    client,
                    EventPayload::ClientMessage {
                        pane: context.pane,
                        kind: ClientMessageKind::Warning,
                        text,
                    },
                );
            }
        } else {
            let (kind, text) = match report.summary() {
                Some(summary) => (
                    ClientMessageKind::Warning,
                    format!("{CONFIG_RELOADED_MESSAGE}; {summary}"),
                ),
                None => (
                    ClientMessageKind::Success,
                    CONFIG_RELOADED_MESSAGE.to_owned(),
                ),
            };
            self.publish_to_client(
                client,
                EventPayload::ClientMessage {
                    pane: context.pane,
                    kind,
                    text,
                },
            );
        }
        self.publish_key_tables_if_changed();
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn load_config_file(
        self: &Arc<Self>,
        path: &Path,
        context: &mut ExecutionContext,
        depth: usize,
    ) -> Result<(), DaemonError> {
        let mut source_invocations = SourceInvocationAccounting::default();
        self.load_config_file_with_report_for_terminal(
            path,
            context,
            depth,
            &mut ConfigLoadReport::default(),
            ClientTerminal::NoClient,
            None,
            &mut source_invocations,
        )
    }

    #[cfg(test)]
    pub(super) fn load_config_file_with_report(
        self: &Arc<Self>,
        path: &Path,
        context: &mut ExecutionContext,
        depth: usize,
        report: &mut ConfigLoadReport,
    ) -> Result<(), DaemonError> {
        let mut source_invocations = SourceInvocationAccounting::default();
        self.load_config_file_with_report_for_terminal(
            path,
            context,
            depth,
            report,
            ClientTerminal::NoClient,
            None,
            &mut source_invocations,
        )
    }

    #[cfg(test)]
    pub(super) fn load_config_file_with_report_for_terminal(
        self: &Arc<Self>,
        path: &Path,
        context: &mut ExecutionContext,
        depth: usize,
        report: &mut ConfigLoadReport,
        client_terminal: ClientTerminal,
        source_client_base: Option<&Path>,
        source_invocations: &mut SourceInvocationAccounting,
    ) -> Result<(), DaemonError> {
        self.load_config_file_with_report_for_terminal_and_options(
            path,
            context,
            depth,
            report,
            client_terminal,
            source_client_base,
            source_invocations,
            SourceFileLoadOptions::default(),
        )
    }

    #[cfg(test)]
    fn load_config_file_with_report_for_terminal_and_options(
        self: &Arc<Self>,
        path: &Path,
        context: &mut ExecutionContext,
        depth: usize,
        report: &mut ConfigLoadReport,
        client_terminal: ClientTerminal,
        source_client_base: Option<&Path>,
        source_invocations: &mut SourceInvocationAccounting,
        options: SourceFileLoadOptions,
    ) -> Result<(), DaemonError> {
        let Some(parsed) = self.parse_config_file(path, report, options, depth == 0)? else {
            return Ok(());
        };
        let mut deferred_control_config_warnings = Vec::new();
        let replayed = self.replay_config_file(
            path,
            parsed,
            context,
            depth,
            report,
            client_terminal,
            source_client_base,
            source_invocations,
            options,
            &mut deferred_control_config_warnings,
        );
        self.publish_deferred_control_config_warnings(deferred_control_config_warnings);
        replayed
    }

    pub(super) fn parse_config_file(
        &self,
        path: &Path,
        report: &mut ConfigLoadReport,
        options: SourceFileLoadOptions,
        top_level: bool,
    ) -> Result<Option<PreparedConfig>, DaemonError> {
        report.control_guarded |= options.control_target.is_some();
        let bytes = match self.helpers.read(path) {
            Ok(bytes) => bytes,
            Err(error)
                if error.kind() == ErrorKind::NotFound && options.control_target.is_none() =>
            {
                log::warn!("ignoring missing config file: {}", path.display());
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        let input = ConfigInput::sourced(&bytes);
        self.parse_config_input(path, input, report, options, top_level)
            .map(Some)
            .ok_or_else(|| non_utf8_config_error().into())
    }

    pub(super) fn parse_startup_config_file(
        &self,
        path: &Path,
        report: &mut ConfigLoadReport,
        explicit: bool,
    ) -> Option<PreparedConfig> {
        let bytes = match self.helpers.read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                log::warn!(
                    "ignoring unreadable config file {}: {error}",
                    path.display()
                );
                report.note_startup_root_read_error(path, &error, explicit);
                return None;
            }
        };
        let input = ConfigInput::startup(&bytes);
        let parsed =
            self.parse_config_input(path, input, report, SourceFileLoadOptions::default(), true);
        if parsed.is_none() {
            let error = non_utf8_config_error();
            log::warn!(
                "ignoring unreadable config file {}: {error}",
                path.display()
            );
            report.note_startup_root_read_error(path, &error, explicit);
        }
        parsed
    }

    pub(super) fn parse_config_input(
        &self,
        path: &Path,
        input: ConfigInput<'_>,
        report: &mut ConfigLoadReport,
        options: SourceFileLoadOptions,
        top_level: bool,
    ) -> Option<PreparedConfig> {
        let source = path.display().to_string();
        let non_empty = !input.is_empty();
        let (parsed, construction, verbose_groups) = {
            let mut inner = self.inner.lock();
            let mut parsed = match input {
                ConfigInput::Text(text) => {
                    let parsed = if options.parse_only {
                        inner.engine.parse_config_parse_only(source, text)
                    } else {
                        inner.engine.parse_config(source, text)
                    };
                    ConfigParse::from_text(parsed)
                }
                ConfigInput::FileBytes(bytes) => {
                    let parsed = if options.parse_only {
                        inner
                            .engine
                            .parse_config_file_bytes_parse_only(source, bytes)
                    } else {
                        inner.engine.parse_config_file_bytes(source, bytes)
                    };
                    ConfigParse::from_bytes(parsed)?
                }
                ConfigInput::BufferBytes(bytes) => {
                    ConfigParse::from_bytes(inner.engine.parse_config_buffer_bytes(source, bytes))?
                }
            };
            if !options.parse_only {
                for assignment in &parsed.environment {
                    inner.engine.set_config_environment(
                        assignment.name.clone(),
                        assignment.value.clone(),
                        assignment.hidden,
                    );
                }
            }
            let mut commands = Vec::new();
            let mut verbose_groups = Vec::new();
            let mut failure = None;
            let group_lengths = parsed
                .commands
                .chunk_by(|left, right| {
                    left.source
                        .as_ref()
                        .map(|source| (&source.source, source.line))
                        == right
                            .source
                            .as_ref()
                            .map(|source| (&source.source, source.line))
                })
                .map(<[CommandInvocation]>::len)
                .collect::<Vec<_>>();
            let mut parsed_commands = std::mem::take(&mut parsed.commands).into_iter();
            'groups: for length in group_lengths {
                let mut prepared_group = Vec::with_capacity(length);
                for command in parsed_commands.by_ref().take(length) {
                    match prepare_config_command(&inner.engine, &command) {
                        Ok((routed, alias_matched)) => {
                            if options.verbose && alias_matched {
                                verbose_groups.push(vec![routed.clone()]);
                            }
                            prepared_group.push(PreparedConfigCommand {
                                original: command,
                                routed,
                            });
                        }
                        Err(error) => {
                            failure = Some(PreparedConfigFailure {
                                original: command,
                                message: error.tmux_message(),
                            });
                            break 'groups;
                        }
                    }
                }
                if options.verbose {
                    verbose_groups.push(
                        prepared_group
                            .iter()
                            .map(|prepared| prepared.routed.clone())
                            .collect(),
                    );
                }
                commands.extend(prepared_group);
            }
            let construction = failure.map_or_else(|| Ok(commands), Err);
            (parsed, construction, verbose_groups)
        };
        for diagnostic in &parsed.diagnostics {
            log::warn!(
                "{}:{}:{}: {}",
                diagnostic.source,
                diagnostic.line,
                diagnostic.column,
                diagnostic.message
            );
            report.note_parse_diagnostic(
                &diagnostic.source,
                diagnostic.line,
                &diagnostic.message,
                top_level,
            );
        }
        if options.verbose {
            for group in &verbose_groups {
                report.note_verbose_commands(group, top_level);
            }
        }
        let empty_command_item = non_empty
            && parsed.diagnostics.is_empty()
            && construction.as_ref().is_ok_and(Vec::is_empty);
        Some(PreparedConfig {
            construction,
            empty_command_item,
        })
    }
}

pub(super) const MAX_STARTUP_CONFIG_PREVIEW_BYTES: usize = 64 * 1024;

const STARTUP_CONFIG_PREVIEW_TRUNCATED: &str =
    "... startup diagnostics truncated; restart in Control mode for full output";

pub(super) fn startup_mux_config_files(
    load_user_config: bool,
    explicit: Option<&[PathBuf]>,
    default: impl FnOnce() -> Option<PathBuf>,
) -> Vec<PathBuf> {
    if !load_user_config {
        return Vec::new();
    }
    explicit.map_or_else(|| default().into_iter().collect(), <[PathBuf]>::to_vec)
}

pub(super) const MUX_IMPORT_START: &str = "# zz-import-tmux-begin: ";

pub(super) fn resolve_tmux_import_path(
    donor: Option<&str>,
    context: &ExecutionContext,
) -> Result<PathBuf, DaemonError> {
    let source = match donor {
        Some(donor) => {
            let path = if donor == "~" || donor.starts_with("~/") {
                home_directory()
                    .ok_or_else(|| {
                        ServerError::InvalidCommand(format!(
                            "cannot resolve {donor}: home directory unavailable"
                        ))
                    })?
                    .join(donor.strip_prefix("~/").unwrap_or(""))
            } else {
                PathBuf::from(donor)
            };
            if path.is_absolute() {
                path
            } else {
                context
                    .client_working_directory()
                    .map(Path::to_owned)
                    .map_or_else(std::env::current_dir, Ok)?
                    .join(path)
            }
        }
        None => discover_tmux_config()
            .ok_or_else(|| ServerError::InvalidCommand("no tmux configuration found".to_owned()))?,
    };
    Ok(source)
}

pub(super) fn prepare_tmux_import(
    engine: &MuxEngine,
    path: &Path,
    source: &str,
) -> (String, usize, Vec<String>) {
    let parsed = engine.parse_config(path.display().to_string(), source);
    let mut output = String::new();
    let mut pending = String::new();
    let mut first_line = 1;
    let mut copied = 0;
    let mut unsupported = Vec::new();
    let lines = source.split_inclusive('\n').collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        pending.push_str(line);
        if line
            .trim_end_matches(['\r', '\n'])
            .chars()
            .rev()
            .take_while(|character| *character == '\\')
            .count()
            % 2
            == 1
            && index + 1 != lines.len()
        {
            continue;
        }
        let unit = MuxEngine::parse_config_without_variable_expansion("import", &pending);
        if !unit.diagnostics.is_empty() && index + 1 != lines.len() {
            continue;
        }
        let commands = if parsed.diagnostics.is_empty() {
            parsed
                .commands
                .iter()
                .filter(|command| {
                    command.source.as_ref().is_some_and(|span| {
                        usize::try_from(span.line)
                            .is_ok_and(|line| line >= first_line && line <= index + 1)
                    })
                })
                .collect::<Vec<_>>()
        } else {
            unit.commands.iter().collect::<Vec<_>>()
        };
        let mut rejected = commands
            .iter()
            .filter_map(|command| match prepare_config_command(engine, command) {
                Ok((command, _))
                    if !zz_protocol::CommandSpec::UNIMPLEMENTED_TMUX_COMMANDS
                        .contains(&canonical_command(&command.name)) =>
                {
                    None
                }
                Ok((command, _)) => Some(command.name),
                Err(_) => Some(command.name.clone()),
            })
            .collect::<Vec<_>>();
        if !unit.diagnostics.is_empty() {
            rejected.push("invalid configuration construct".to_owned());
        }
        if rejected.is_empty() {
            copied += commands.len();
            output.push_str(&pending);
        } else {
            unsupported.extend(rejected);
            for line in pending.split_inclusive('\n') {
                output.push_str("# zz-unsupported: ");
                output.push_str(line);
            }
        }
        pending.clear();
        first_line = index + 2;
    }
    (output, copied, unsupported)
}

pub(super) fn replace_tmux_import(
    existing: &str,
    source: &Path,
    copied: &str,
) -> Result<String, DaemonError> {
    let source_label = source.display().to_string().replace(['\r', '\n'], " ");
    let mut label = source_label.clone();
    let mut discriminator = 0;
    while copied
        .lines()
        .any(|line| line == format!("{MUX_IMPORT_END}{label}"))
    {
        discriminator += 1;
        label = format!("{source_label} [{discriminator}]");
    }
    let block = format!(
        "{MUX_IMPORT_START}{label}\n{copied}{}{MUX_IMPORT_END}{label}\n",
        if copied.is_empty() || copied.ends_with('\n') {
            ""
        } else {
            "\n"
        }
    );
    let mut offset = 0;
    let mut start = None;
    let mut range = None;
    let mut closing = None;
    for line in existing.split_inclusive('\n') {
        if let Some(label) = line.strip_prefix(MUX_IMPORT_START)
            && start.is_none()
        {
            start = Some(offset);
            closing = Some(format!(
                "{MUX_IMPORT_END}{}",
                label.trim_end_matches(['\r', '\n'])
            ));
        }
        offset += line.len();
        if closing.as_deref() == Some(line.trim_end_matches(['\r', '\n']))
            && let Some(start) = start
        {
            range = Some(start..offset);
            break;
        }
    }
    if start.is_some() && range.is_none() {
        return Err(ServerError::InvalidCommand(
            "zz/mux.conf has an unfinished import block".to_owned(),
        )
        .into());
    }
    let mut edited = existing.to_owned();
    edited.replace_range(range.unwrap_or(0..0), &block);
    if edited.len() > MUX_IMPORT_MAX_BYTES {
        return Err(ServerError::InvalidCommand(
            "imported configuration exceeds the 1 MiB limit".to_owned(),
        )
        .into());
    }
    Ok(edited)
}

struct ConfigEnvironmentEntry {
    name: String,
    value: RawText,
    hidden: bool,
}

struct ConfigParse {
    commands: Vec<CommandInvocation>,
    environment: Vec<ConfigEnvironmentEntry>,
    diagnostics: Vec<ConfigDiagnostic>,
}

impl ConfigParse {
    fn from_text(parsed: ParsedConfig) -> Self {
        Self {
            commands: parsed.commands,
            environment: parsed
                .environment
                .into_iter()
                .map(|assignment| ConfigEnvironmentEntry {
                    name: assignment.name,
                    value: assignment.value,
                    hidden: assignment.hidden,
                })
                .collect(),
            diagnostics: parsed.diagnostics,
        }
    }

    pub(super) fn from_bytes(parsed: ParsedConfigBytes) -> Option<Self> {
        let mut commands = Vec::with_capacity(parsed.commands.len());
        for command in parsed.commands {
            let blocks = (0..command.args.len())
                .filter(|index| command.argument_is_command_block(*index))
                .collect::<Vec<_>>();
            let name = String::from_utf8_lossy(&command.name).into_owned();
            let args = command.args.into_iter().map(RawText::from_bytes);
            let invocation = CommandInvocation::new(name, args).with_command_blocks(blocks);
            commands.push(match command.source {
                Some(source) => invocation.with_source(source),
                None => invocation,
            });
        }
        let mut environment = Vec::with_capacity(parsed.environment.len());
        for assignment in parsed.environment {
            environment.push(ConfigEnvironmentEntry {
                name: String::from_utf8(assignment.name).ok()?,
                value: RawText::from_bytes(assignment.value),
                hidden: assignment.hidden,
            });
        }
        Some(Self {
            commands,
            environment,
            diagnostics: parsed.diagnostics,
        })
    }
}

#[derive(Default)]
pub(super) struct ConfigLoadReport {
    skipped_count: usize,
    skipped: Vec<String>,
    invalid_count: usize,
    invalid: Vec<String>,
    diagnostics: Vec<String>,
    pub(super) replayed_diagnostics: BTreeSet<usize>,
    verbose_lines: Vec<String>,
    stdout_transcript: Option<Vec<ConfigStdoutTranscript>>,
    #[cfg(test)]
    source_errors: Vec<String>,
    source_error_groups: Vec<Vec<String>>,
    pub(super) replay_issues: Vec<ConfigReplayIssue>,
    pub(super) reported_failure: bool,
    pub(super) source_callback_failure: bool,
    pub(super) delivered_replay_issues: usize,
    pub(super) control_guarded: bool,
    startup_causes: Option<Vec<String>>,
}

#[derive(Default)]
struct ConfigStdoutTranscript {
    top_level_verbose: RawText,
    replay: RawText,
    diagnostics: RawText,
    raw_claimed: bool,
}

impl ConfigLoadReport {
    const SUMMARY_NAMES: usize = 6;

    pub(super) fn startup() -> Self {
        Self {
            startup_causes: Some(Vec::new()),
            ..Self::default()
        }
    }

    pub(super) fn take_startup_causes(&mut self) -> Option<Vec<String>> {
        self.startup_causes
            .take()
            .filter(|causes| !causes.is_empty())
    }

    pub(super) fn note_startup_cause(&mut self, cause: &str) {
        let Some(causes) = self.startup_causes.as_mut() else {
            return;
        };
        if cause.is_empty() || causes.len() >= MAX_STARTUP_CONFIG_CAUSES {
            return;
        }
        let total = causes.iter().map(String::len).sum::<usize>();
        let available = MAX_STARTUP_CONFIG_CAUSES_BYTES.saturating_sub(total);
        let limit = MAX_STARTUP_CONFIG_CAUSE_BYTES.min(available);
        if limit == 0 {
            return;
        }
        let mut end = cause.len().min(limit);
        while !cause.is_char_boundary(end) {
            end -= 1;
        }
        if end != 0 {
            causes.push(cause[..end].to_owned());
        }
    }

    pub(super) fn note_startup_root_read_error(
        &mut self,
        path: &Path,
        error: &std::io::Error,
        explicit: bool,
    ) {
        if !explicit && error.kind() == ErrorKind::NotFound {
            return;
        }
        let message = format!("{}: {}", path.display(), filesystem_error_message(error));
        self.note_startup_cause(&message);
    }

    pub(super) fn note_startup_display(&mut self, command: &CommandInvocation, output: &RawText) {
        if !output.as_bytes().is_empty() {
            self.note_startup_command_cause(command, output);
        }
    }

    pub(super) fn note_startup_command_cause(&mut self, command: &CommandInvocation, cause: &str) {
        self.note_startup_cause(&config_command_error(command, cause));
    }

    pub(super) fn with_stdout_transcript() -> Self {
        Self {
            stdout_transcript: Some(vec![ConfigStdoutTranscript::default()]),
            ..Self::default()
        }
    }

    pub(super) fn note_stdout(&mut self, output: &RawText, raw: bool) -> ReplayStdoutWrite {
        let Some(transcripts) = self.stdout_transcript.as_mut() else {
            return ReplayStdoutWrite::Written;
        };
        if output.as_bytes().is_empty() || transcripts.is_empty() {
            return ReplayStdoutWrite::Written;
        }
        let claimed = transcripts.iter().any(|frame| frame.raw_claimed);
        let written = transcripts
            .iter()
            .any(|frame| !frame.replay.as_bytes().is_empty());
        let transcript = transcripts
            .last_mut()
            .expect("stdout transcript has a frame");
        if raw {
            if claimed || written {
                return ReplayStdoutWrite::Denied;
            }
            transcript.raw_claimed = true;
            transcript.replay.push_bytes(output.as_bytes());
            return ReplayStdoutWrite::Written;
        }
        if claimed {
            return ReplayStdoutWrite::Dropped;
        }
        append_inserted_output(&mut transcript.replay, output);
        if !transcript.replay.as_bytes().ends_with(b"\n") {
            transcript.replay.push_bytes(b"\n");
        }
        ReplayStdoutWrite::Written
    }

    fn note_stdout_line(&mut self, line: &str, top_level: bool) {
        if let Some(transcript) = self
            .stdout_transcript
            .as_mut()
            .and_then(|transcripts| transcripts.last_mut())
        {
            let output = if top_level {
                &mut transcript.top_level_verbose
            } else {
                &mut transcript.replay
            };
            append_inserted_output(output, line);
            output.push_bytes(b"\n");
        }
    }

    pub(super) fn stdout_raw_claimed(&self) -> bool {
        self.stdout_transcript
            .as_ref()
            .is_some_and(|transcripts| transcripts.iter().any(|frame| frame.raw_claimed))
    }

    pub(super) fn stdout_transcript(&self) -> Option<(&RawText, &RawText)> {
        self.stdout_transcript.as_ref().map(|transcripts| {
            let transcript = transcripts
                .first()
                .expect("stdout transcript has a root frame");
            (&transcript.top_level_verbose, &transcript.replay)
        })
    }

    pub(super) fn note_skip(&mut self, name: &str) {
        self.skipped_count += 1;
        if !self.skipped.iter().any(|existing| existing == name) {
            self.skipped.push(name.to_owned());
        }
    }

    pub(super) fn note_skip_command(&mut self, command: &CommandInvocation, name: &str) {
        self.note_skip(name);
        self.note_startup_cause(&config_command_error(
            command,
            &format!("unsupported tmux command: {name}"),
        ));
    }

    fn note_invalid(&mut self, message: &str) -> usize {
        self.invalid_count += 1;
        self.diagnostics.push(message.to_owned());
        if !self.invalid.iter().any(|existing| existing == message) {
            self.invalid.push(message.to_owned());
        }
        self.diagnostics.len() - 1
    }

    #[cfg(test)]
    pub(super) fn note_invalid_at(&mut self, source: &str, line: u32, message: &str) {
        self.note_invalid(&format!("{source}:{line}: {message}"));
    }

    fn note_parse_diagnostic(&mut self, source: &str, line: u32, message: &str, top_level: bool) {
        let diagnostic = format!("{source}:{line}: {message}");
        self.note_startup_cause(&diagnostic);
        self.reported_failure = true;
        let index = self.note_invalid(&diagnostic);
        if self.stdout_transcript.is_some() {
            self.replayed_diagnostics.insert(index);
            if top_level {
                let transcript = self
                    .stdout_transcript
                    .as_mut()
                    .and_then(|transcripts| transcripts.last_mut())
                    .expect("stdout transcript has a current frame");
                append_inserted_output(&mut transcript.diagnostics, &diagnostic);
                transcript.diagnostics.push_bytes(b"\n");
            } else {
                let transcript = self
                    .stdout_transcript
                    .as_mut()
                    .and_then(|transcripts| transcripts.last_mut())
                    .expect("stdout transcript has a current frame");
                append_inserted_output(&mut transcript.replay, &diagnostic);
                transcript.replay.push_bytes(b"\n");
            }
        }
    }

    pub(super) fn note_invalid_command(&mut self, command: &CommandInvocation, message: &str) {
        let diagnostic = command.source.as_ref().map_or_else(
            || message.to_owned(),
            |source| format!("{}:{}: {message}", source.source, source.line),
        );
        self.note_startup_cause(&diagnostic);
        self.reported_failure = true;
        let index = self.note_invalid(&diagnostic);
        if let Some(transcript) = self
            .stdout_transcript
            .as_mut()
            .and_then(|transcripts| transcripts.last_mut())
        {
            self.replayed_diagnostics.insert(index);
            append_inserted_output(&mut transcript.diagnostics, &diagnostic);
            transcript.diagnostics.push_bytes(b"\n");
        }
    }

    fn note_invalid_command_error(&mut self, command: &CommandInvocation, message: &str) {
        let diagnostic = config_command_error(command, message);
        self.note_startup_cause(&diagnostic);
        self.reported_failure = true;
        self.replay_issues.push(ConfigReplayIssue::CommandError {
            source: command.source.clone(),
            message: diagnostic,
        });
    }

    pub(super) fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    pub(super) fn stdout_diagnostics(&self) -> Option<&str> {
        self.stdout_transcript.as_ref().map(|transcripts| {
            transcripts
                .first()
                .expect("stdout transcript has a root frame")
                .diagnostics
                .as_str()
        })
    }

    pub(super) fn push_stdout_frame(&mut self) {
        if let Some(transcripts) = &mut self.stdout_transcript {
            transcripts.push(ConfigStdoutTranscript::default());
        }
    }

    pub(super) fn pop_stdout_frame(&mut self) {
        let Some(transcripts) = &mut self.stdout_transcript else {
            return;
        };
        if transcripts.len() == 1 {
            return;
        }
        let transcript = transcripts.pop().expect("nested stdout frame exists");
        let mut output = RawText::default();
        append_inserted_output(&mut output, &transcript.top_level_verbose);
        append_inserted_output(&mut output, &transcript.replay);
        append_inserted_output(&mut output, &transcript.diagnostics);
        let parent = transcripts
            .last_mut()
            .expect("nested stdout frame has a parent");
        append_inserted_output(&mut parent.replay, &output);
        parent.raw_claimed |= transcript.raw_claimed;
    }

    fn note_verbose_commands(&mut self, commands: &[CommandInvocation], top_level: bool) {
        for group in commands.chunk_by(|left, right| {
            left.source
                .as_ref()
                .map(|source| (&source.source, source.line))
                == right
                    .source
                    .as_ref()
                    .map(|source| (&source.source, source.line))
        }) {
            let Some(source) = group.first().and_then(|command| command.source.as_ref()) else {
                continue;
            };
            let commands = group
                .iter()
                .map(format_command)
                .collect::<Vec<_>>()
                .join(" ; ");
            let line = format!("{}:{}: {commands}", source.source, source.line);
            self.verbose_lines.push(line.clone());
            self.note_stdout_line(&line, top_level);
        }
    }

    pub(super) fn verbose_lines(&self) -> &[String] {
        &self.verbose_lines
    }

    fn push_source_error(&mut self, group: &mut Option<usize>, error: &str) {
        #[cfg(test)]
        self.source_errors.push(error.to_owned());
        let index = if let Some(index) = *group {
            index
        } else {
            let index = self.source_error_groups.len();
            self.source_error_groups.push(Vec::new());
            self.replay_issues
                .push(ConfigReplayIssue::SourceErrors(index));
            *group = Some(index);
            index
        };
        self.source_error_groups[index].push(error.to_owned());
    }

    pub(super) fn note_source_error(&mut self, group: &mut Option<usize>, error: &str) {
        self.push_source_error(group, error);
        self.note_startup_cause(error);
    }

    pub(super) fn note_located_source_error(
        &mut self,
        command: &CommandInvocation,
        group: &mut Option<usize>,
        error: &str,
    ) {
        self.push_source_error(group, error);
        self.note_startup_cause(&config_command_error(command, error));
    }

    pub(super) fn note_command_error(&mut self, command: &CommandInvocation, message: &str) {
        self.note_startup_cause(&config_command_error(command, message));
        self.replay_issues.push(ConfigReplayIssue::CommandError {
            source: command.source.clone(),
            message: message.to_owned(),
        });
    }

    pub(super) fn note_unlocated_command_error(&mut self, message: &str) {
        self.note_startup_cause(message);
        self.reported_failure = true;
        self.replay_issues.push(ConfigReplayIssue::CommandError {
            source: None,
            message: message.to_owned(),
        });
    }

    pub(super) fn note_callback_failures(
        &mut self,
        path: &Path,
        command: &CommandInvocation,
        failures: Vec<CallbackParseFailure>,
    ) -> Vec<CallbackParseFailure> {
        let mut source_failures = Vec::new();
        for mut failure in failures {
            match failure.owner {
                CallbackFailureOwner::Source => {
                    self.source_callback_failure = true;
                    let message = std::mem::take(&mut failure.message);
                    log::warn!(
                        "{}: invalid inserted tmux command: {message}",
                        path.display()
                    );
                    self.note_invalid_command_error(command, &message);
                    failure.message = config_command_error(command, &message);
                    source_failures.push(failure);
                }
                CallbackFailureOwner::RuntimeCommand => {
                    self.source_callback_failure = true;
                    log::warn!(
                        "{}: invalid inserted tmux command: {}",
                        path.display(),
                        failure.message
                    );
                    self.note_unlocated_command_error(&failure.message);
                    source_failures.push(failure);
                }
                CallbackFailureOwner::Hook | CallbackFailureOwner::CommandErrorHook => {
                    self.note_unlocated_command_error(&failure.message);
                }
            }
        }
        source_failures
    }

    #[cfg(test)]
    pub(super) fn source_errors(&self) -> &[String] {
        &self.source_errors
    }

    fn source_error_groups(&self) -> &[Vec<String>] {
        &self.source_error_groups
    }

    pub(super) fn replay_issues(&self) -> &[ConfigReplayIssue] {
        &self.replay_issues
    }

    pub(super) fn take_undelivered_replay_issues(&mut self) -> Vec<ConfigReplayIssue> {
        let mut issues = Vec::new();
        for issue in &self.replay_issues[self.delivered_replay_issues..] {
            match issue {
                ConfigReplayIssue::SourceErrors(index) => {
                    for message in &self.source_error_groups[*index] {
                        issues.push(ConfigReplayIssue::CommandError {
                            source: None,
                            message: message.clone(),
                        });
                    }
                }
                ConfigReplayIssue::CommandError { source, message } => {
                    issues.push(ConfigReplayIssue::CommandError {
                        source: source.clone(),
                        message: message.clone(),
                    });
                }
            }
        }
        self.delivered_replay_issues = self.replay_issues.len();
        issues
    }

    fn append_deferred_replay_issues(&mut self, issues: Vec<ConfigReplayIssue>) {
        if issues.is_empty() {
            return;
        }
        self.reported_failure = true;
        for issue in issues {
            if let ConfigReplayIssue::CommandError { message, .. } = &issue {
                self.note_startup_cause(message);
            }
            self.replay_issues.push(issue);
        }
    }

    pub(super) fn note_deferred_config_replay_issues(
        &mut self,
        path: &Path,
        command: &CommandInvocation,
        issues: Vec<DeferredConfigReplayIssue>,
    ) {
        for issue in issues {
            match issue {
                DeferredConfigReplayIssue::Callback(failure) => {
                    let _ = self.note_callback_failures(path, command, vec![failure]);
                }
                DeferredConfigReplayIssue::Replay(issue) => {
                    self.append_deferred_replay_issues(vec![issue]);
                }
            }
        }
    }

    pub(super) fn message(&self) -> Option<String> {
        if self.skipped_count == 0 && self.invalid_count == 1 {
            return self.invalid.first().cloned();
        }
        self.summary()
    }

    pub(super) fn skipped_summary(&self) -> Option<String> {
        (self.skipped_count != 0).then(|| {
            format!(
                "skipped {} unsupported tmux command{}: {}",
                self.skipped_count,
                if self.skipped_count == 1 { "" } else { "s" },
                Self::summary_names(&self.skipped),
            )
        })
    }

    pub(super) fn summary(&self) -> Option<String> {
        if self.skipped_count == 0 && self.invalid_count == 0 {
            return None;
        }
        let mut parts = Vec::new();
        if let Some(skipped) = self.skipped_summary() {
            parts.push(skipped);
        }
        if self.invalid_count != 0 {
            parts.push(format!(
                "{} invalid line{}: {}",
                self.invalid_count,
                if self.invalid_count == 1 { "" } else { "s" },
                Self::summary_names(&self.invalid),
            ));
        }
        Some(parts.join("; "))
    }

    fn summary_names(entries: &[String]) -> String {
        let mut names =
            entries
                .iter()
                .take(Self::SUMMARY_NAMES)
                .fold(String::new(), |mut names, name| {
                    if !names.is_empty() {
                        names.push_str(", ");
                    }
                    names.push_str(name);
                    names
                });
        if entries.len() > Self::SUMMARY_NAMES {
            names.push_str(", …");
        }
        names
    }
}

pub(super) fn startup_config_preview(causes: &[String]) -> String {
    let joined = causes.join("\n");
    let mut preview = String::with_capacity(joined.len().min(MAX_STARTUP_CONFIG_PREVIEW_BYTES));
    for character in joined.chars() {
        if character.is_control() && !matches!(character, '\n' | '\t') {
            preview.push('\u{fffd}');
        } else {
            preview.push(character);
        }
    }
    if preview.len() <= MAX_STARTUP_CONFIG_PREVIEW_BYTES {
        return preview;
    }
    let mut end =
        MAX_STARTUP_CONFIG_PREVIEW_BYTES.saturating_sub(STARTUP_CONFIG_PREVIEW_TRUNCATED.len() + 1);
    while !preview.is_char_boundary(end) {
        end -= 1;
    }
    preview.truncate(end);
    preview.push('\n');
    preview.push_str(STARTUP_CONFIG_PREVIEW_TRUNCATED);
    preview
}
