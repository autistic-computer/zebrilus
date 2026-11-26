use super::*;

impl MuxEngine {
    pub fn take_destroyed_sessions(&mut self) -> Vec<(SessionId, String, String)> {
        std::mem::take(&mut self.destroyed_sessions)
    }

    pub(super) fn destroyed_session_marker(
        &self,
        session: SessionId,
    ) -> Result<(SessionId, String, String), ServerError> {
        let policy = self.detach_on_destroy_for_session(session);
        let name = self
            .state
            .sessions
            .get(&session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?
            .name
            .clone();
        Ok((session, name, policy))
    }

    pub(super) fn record_destroyed_session(&mut self, marker: (SessionId, String, String)) {
        if !self.state.sessions.contains_key(&marker.0)
            && !self
                .destroyed_sessions
                .iter()
                .any(|(session, _, _)| *session == marker.0)
        {
            self.destroyed_sessions.push(marker);
        }
    }

    fn kill_session_state(&mut self, session: SessionId) -> Result<Vec<PaneId>, ServerError> {
        let marker = self.destroyed_session_marker(session)?;
        let panes = self.state.kill_session(session)?;
        self.record_destroyed_session(marker);
        Ok(panes)
    }

    pub fn new_session_attaches(args: &[RawText]) -> Result<bool, ServerError> {
        let (options, _) = parse_new_session_attach_options(args)?;
        Ok(options.has("-A") || !options.has("-d"))
    }

    pub fn new_session_needs_attach_preflight(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<bool, ServerError> {
        let (options, positional) = parse_new_session_attach_options(args)?;
        if !context.has_client_terminal()
            || options.has("-d")
            || context.attached_client_context().is_some()
        {
            return Ok(false);
        }
        let mut item_hooks = CommandItemHooks {
            inner: hooks,
            command: Some("new-session"),
        };
        let hooks = &mut item_hooks;
        if options.value("-t").is_some()
            && (!positional.is_empty() || options.value("-n").is_some())
        {
            return Err(ServerError::InvalidCommand(
                "command or window name given with target".to_owned(),
            ));
        }
        options
            .value("-n")
            .map(|name| self.expand_new_session_item_name(context, name, "window", hooks))
            .transpose()?;
        let requested_name = options
            .value("-s")
            .map(|name| self.expand_new_session_item_name(context, name, "session", hooks))
            .transpose()?;
        if options.has("-A") {
            let existing = match requested_name.as_deref() {
                Some(name) => session_named(&self.state, name),
                None => self
                    .resolve_session(options.value("-t"), context.session)
                    .ok(),
            };
            if existing.is_some() {
                return Ok(true);
            }
        }
        let name = requested_name.unwrap_or_else(|| next_session_name(&self.state));
        if session_named(&self.state, &name).is_some() {
            return Err(ServerError::InvalidCommand(format!(
                "duplicate session: {name}"
            )));
        }
        if let Some(target) = options.value("-t")
            && self.resolve_session(Some(target), context.session).is_err()
        {
            tmux_clean_name(target, "session group")?;
        }
        self.prepare_new_session_cwd(&options, context, context.format_client(), hooks);
        Ok(true)
    }

    pub(super) fn new_session(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) =
            parse_command_options("new-session", args).map_err(|error| match error {
                ServerError::UnsupportedCommand(message) if message == "new-session -t" => {
                    ServerError::UnsupportedCommand(NEW_SESSION_GROUP_REFUSAL.to_owned())
                }
                error => error,
            })?;
        let format_client = context.format_client();
        let active_session = context.session;
        let command = shell_command_positional(&positional);
        let detached = options.has("-d") || context.client_terminal == ClientTerminal::NoClient;
        let requested_window_name = options
            .value("-n")
            .map(|name| self.expand_new_session_item_name(context, name, "window", hooks))
            .transpose()?;
        let requested_name = options
            .value("-s")
            .map(|name| self.expand_new_session_item_name(context, name, "session", hooks))
            .transpose()?;
        if options.has("-A") {
            let existing = match requested_name.as_deref() {
                Some(name) => session_named(&self.state, name),
                None => self.resolve_session(None, context.session).ok(),
            };
            if let Some(session) = existing {
                if context.has_no_client() {
                    return Ok(Execution::default());
                }
                if context.refuses_new_session_attach() {
                    return Err(ServerError::InvalidCommand(
                        "sessions should be nested with care, unset $TMUX to force".to_owned(),
                    ));
                }
                let window = session_active_window(&self.state, session)?;
                let pane = window_active_pane(&self.state, window)?;
                let active_session = context.session;
                self.retarget_session_attach(
                    context,
                    session,
                    window,
                    pane,
                    options.value("-c"),
                    active_session,
                    hooks,
                )?;
                return Ok(Execution::effect(MuxEffect::Attach {
                    session,
                    detach_others: options.has("-D") || options.has("-X"),
                    detach_others_hangup: options.has("-X"),
                    read_only: false,
                    flags: options.value("-f").map(str::to_owned),
                    update_environment: !options.has("-E"),
                }));
            }
        }
        let name = requested_name.unwrap_or_else(|| next_session_name(&self.state));
        if session_named(&self.state, &name).is_some() {
            return Err(ServerError::InvalidCommand(format!(
                "duplicate session: {name}"
            )));
        }
        if !detached {
            require_client_terminal(context)?;
        }
        let extent =
            initial_window_extent(&options, self.global_default_size(), context.client_size())?;
        let session_cwd = self.prepare_new_session_cwd(&options, context, format_client, hooks);
        let inherit_cwd_from = context.pane.and_then(|pane| self.state.cwd_donor(pane));
        let inherited_session_working_directory = context
            .attached_client_context()
            .and_then(|(session, _, _)| session)
            .and_then(|session| {
                self.state
                    .session_working_directory(session)
                    .map(Path::to_owned)
            })
            .or_else(|| context.client_working_directory().map(Path::to_owned));
        let environment = session_creation_environment(&options);
        let base_index = self.global_base_index;
        let (session, window, pane) = self
            .state
            .create_session_with_extent_at(name, extent, base_index)?;
        let cwd = options.value("-c").and_then(|cwd| {
            let target = ExecutionContext::new(Some(session), Some(window), Some(pane));
            let expanded =
                self.expand_pane_format(cwd, &target, active_session, format_client, hooks);
            (!expanded.is_empty()).then_some(expanded)
        });
        let session_working_directory = options
            .value("-c")
            .map(|_| PathBuf::from(session_cwd.as_deref().unwrap_or_default()))
            .or(inherited_session_working_directory);
        if let Some(session_working_directory) = session_working_directory {
            self.state
                .set_session_working_directory(session, session_working_directory)?;
        }
        self.seed_session_environment(
            session,
            !options.has("-E"),
            context.client_environment(),
            environment,
        );
        let created = i64::try_from(self.format_now())
            .ok()
            .filter(|created| *created != 0);
        let session_state = self
            .state
            .sessions
            .get_mut(&mut self.state.journal, &session)
            .expect("new session exists");
        session_state.created = created;
        session_state.activity = created;
        if let Some(window_name) = requested_window_name {
            window_name.clone_into(
                &mut self
                    .state
                    .windows
                    .get_mut(&mut self.state.journal, &window)
                    .expect("new window exists")
                    .name,
            );
            self.state
                .set_window_automatic_rename(window, Some(false))?;
        }
        context.retarget(&ExecutionContext::new(
            Some(session),
            Some(window),
            Some(pane),
        ));
        let output_format_client = if detached {
            format_client
        } else {
            FormatClient::Attached(session)
        };
        let output = if options.has("-P") {
            expand_format_with_hooks(
                options.value("-F").unwrap_or(DEFAULT_NEW_SESSION_FORMAT),
                self,
                FormatContext {
                    session: Some(session),
                    window: Some(window),
                    pane: None,
                    active_session: Some(session),
                    format_client: output_format_client,
                    format_type: FormatType::None,
                },
                hooks,
            )
        } else {
            RawText::default()
        };
        let mut effects = vec![MuxEffect::PaneCreated {
            pane,
            kind: PaneKindSnapshot::Terminal,
            inherit_cwd_from,
            cwd,
            command,
            environment: Vec::new(),
            empty: false,
        }];
        if !detached {
            effects.push(MuxEffect::Attach {
                session,
                detach_others: false,
                detach_others_hangup: false,
                read_only: false,
                flags: options.value("-f").map(str::to_owned),
                update_environment: false,
            });
        }
        Ok(Execution { output, effects })
    }

    fn prepare_new_session_cwd(
        &self,
        options: &Options,
        context: &ExecutionContext,
        format_client: FormatClient,
        hooks: &mut impl StatusHooks,
    ) -> Option<String> {
        let value = options.value("-c")?;
        let target = match format_client {
            FormatClient::Attached(session) => self
                .state
                .sessions
                .get(&session)
                .and_then(|session| self.state.windows.get(&session.active_window))
                .and_then(|window| ExecutionContext::for_pane(&self.state, window.active_pane))
                .unwrap_or_default(),
            FormatClient::NoClient | FormatClient::Unattached => ExecutionContext::default(),
        };
        Some(self.expand_pane_format(value, &target, context.session, format_client, hooks))
    }

    fn expand_new_session_item_name(
        &self,
        context: &ExecutionContext,
        name: &str,
        kind: &str,
        hooks: &mut impl StatusHooks,
    ) -> Result<String, ServerError> {
        let (session, window, pane) = context
            .attached_client_context()
            .unwrap_or((None, None, None));
        let expanded = expand_format_with_hooks(
            name,
            self,
            FormatContext {
                session,
                window,
                pane,
                active_session: session,
                format_client: context.format_client(),
                format_type: FormatType::None,
            },
            hooks,
        );
        tmux_clean_name(&expanded, kind)
    }

    pub(super) fn list_sessions(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("list-sessions", args)?;
        reject_positionals("list-sessions", &positional)?;
        let sort = TmuxSort::parse(options.value("-O"), options.has("-r"), None)?;
        let format = options.value("-F").unwrap_or(DEFAULT_LIST_SESSIONS_FORMAT);
        let mut sessions = self
            .state
            .sessions_by_name()
            .into_iter()
            .map(|session| session.id)
            .collect::<Vec<_>>();
        sort.apply(&mut sessions, |left, right| {
            let left = &self.state.sessions[left];
            let right = &self.state.sessions[right];
            let ordering = match sort.order() {
                Some(TmuxSortOrder::Activity) => right.sort_activity.cmp(&left.sort_activity),
                Some(TmuxSortOrder::Creation) => left.sort_created.cmp(&right.sort_created),
                Some(TmuxSortOrder::Index) => left.id.cmp(&right.id),
                Some(TmuxSortOrder::Name) => left.name.cmp(&right.name),
                _ => std::cmp::Ordering::Equal,
            };
            ordering.then_with(|| left.name.cmp(&right.name))
        });
        let mut output = Vec::new();
        let format_client = context.format_client();
        let universe = self.format_universe(format_client);
        let mut dumps = LayoutDumps::default();
        for (line, session) in sessions.into_iter().enumerate() {
            let format_context = FormatContext {
                session: Some(session),
                window: None,
                pane: None,
                active_session: context.session,
                format_client,
                format_type: FormatType::Session,
            };
            let prepared =
                PreparedFormat::with_universe(self, format_context, universe.clone(), &mut dumps);
            if let Some(filter) = options.value("-f") {
                let mut row_hooks = RowFormatHooks { inner: hooks, line };
                if !format_true(&prepared.expand(filter, &mut row_hooks)) {
                    continue;
                }
            }
            let mut row_hooks = RowFormatHooks { inner: hooks, line };
            output.push(if options.has("--json") {
                RawText::from(
                    prepared
                        .values()
                        .scoped_format_values("session", &mut row_hooks)
                        .to_string(),
                )
            } else {
                prepared.expand(format, &mut row_hooks)
            });
        }
        Ok(Execution::output(RawText::join(&output, b"\n")))
    }

    pub(super) fn rename_session(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("rename-session", args)?;
        let name = exactly_one_argument("rename-session", &positional)?;
        let session = self.resolve_session(options.value("-t"), context.session)?;
        let name = expand_format_with_hooks(
            name,
            self,
            FormatContext {
                session: Some(session),
                window: None,
                pane: None,
                active_session: context.session,
                format_client: context.target_format_client(),
                format_type: FormatType::Pane,
            },
            hooks,
        );
        let name = tmux_clean_name(&name, "session")?;
        self.state.rename_session(session, name)?;
        Ok(Execution::default())
    }

    pub(super) fn kill_session(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("kill-session", args)?;
        reject_positionals("kill-session", &positional)?;
        let session = self.resolve_session(options.value("-t"), context.session)?;
        if options.value("-f").is_some() && (!options.has("-a") || options.has("-C")) {
            return Err(ServerError::InvalidCommand(
                "-f only valid with -a".to_owned(),
            ));
        }
        if options.has("-C") {
            let panes = self
                .state
                .sessions
                .get(&session)
                .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?
                .windows
                .iter()
                .filter_map(|window| self.state.windows.get(window))
                .flat_map(|window| window.pane_order().to_vec())
                .collect::<Vec<_>>();
            for pane in panes {
                self.state.clear_pane_alerts(pane);
            }
            return Ok(Execution::default());
        }
        let mut targets = if options.has("-a") {
            self.state
                .sessions
                .keys()
                .copied()
                .filter(|candidate| *candidate != session)
                .collect()
        } else {
            vec![session]
        };
        if let Some(filter) = options.value("-f") {
            targets.retain(|target| {
                format_true(&expand_format_with_hooks(
                    filter,
                    self,
                    FormatContext {
                        session: Some(*target),
                        window: None,
                        pane: None,
                        active_session: context.session,
                        format_client: FormatClient::NoClient,
                        format_type: FormatType::Session,
                    },
                    hooks,
                ))
            });
        }
        let mut panes = Vec::new();
        for target in targets {
            panes.extend(self.kill_session_state(target)?);
        }
        Ok(Execution::effect(MuxEffect::PanesRemoved(panes)))
    }

    pub(super) fn attach_session(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("attach-session", args)?;
        reject_positionals("attach-session", &positional)?;
        if context.has_no_client() {
            return Ok(Execution::default());
        }
        let detach_others = options.has("-d") || options.has("-x");
        let active_session = context.session;
        let (session, window, pane) = match options.value("-t") {
            Some(target) if target.contains([':', '.']) => {
                let pane = self.resolve_pane(Some(target), context.window, context.pane)?;
                let window = self
                    .state
                    .window_for_pane(pane)
                    .expect("resolved pane has a window");
                (self.state.windows[&window].session, window, pane)
            }
            target => {
                let session = self.resolve_session(target, context.session)?;
                let window = session_active_window(&self.state, session)?;
                let pane = window_active_pane(&self.state, window)?;
                (session, window, pane)
            }
        };
        if self.state.windows[&window].active_pane != pane {
            self.state.select_pane(pane)?;
            self.refresh_automatic_window_name_for_pane(pane, hooks);
        }
        if self.state.sessions[&session].active_window != window {
            self.state.select_window(session, window)?;
        }
        self.retarget_session_attach(
            context,
            session,
            window,
            pane,
            options.value("-c"),
            active_session,
            hooks,
        )?;
        Ok(Execution::effect(MuxEffect::Attach {
            session,
            detach_others,
            detach_others_hangup: options.has("-x"),
            read_only: options.has("-r"),
            flags: options.value("-f").map(str::to_owned),
            update_environment: !options.has("-E"),
        }))
    }

    fn retarget_session_attach(
        &mut self,
        context: &mut ExecutionContext,
        session: SessionId,
        window: WindowId,
        pane: PaneId,
        working_directory: Option<&str>,
        active_session: Option<SessionId>,
        hooks: &mut impl StatusHooks,
    ) -> Result<(), ServerError> {
        let format_client = context.format_client();
        let target = ExecutionContext::new(Some(session), Some(window), Some(pane));
        context.retarget(&target);
        if let Some(working_directory) = working_directory {
            let working_directory = self.expand_pane_format(
                working_directory,
                &target,
                active_session,
                format_client,
                hooks,
            );
            self.state
                .set_session_working_directory(session, PathBuf::from(working_directory))?;
        }
        Ok(())
    }

    pub(super) fn suspend_client(&self, args: &[RawText]) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("suspend-client", args)?;
        reject_positionals("suspend-client", &positional)?;
        Ok(Execution::effect(MuxEffect::SuspendClient {
            target_client: options.value("-t").map(str::to_owned),
        }))
    }

    pub(super) fn detach_client(
        &self,
        _context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("detach-client", args)?;
        reject_positionals("detach-client", &positional)?;
        let scope = match options.value("-s") {
            Some(target) => DetachScope::Session(target.to_owned()),
            None if options.has("-a") => DetachScope::Others,
            None => DetachScope::Client,
        };
        Ok(Execution::effect(MuxEffect::Detach(DetachRequest {
            target_client: options.value("-t").map(str::to_owned),
            scope,
            exec: options.value("-E").map(str::to_owned),
            parent_hangup: options.has("-P"),
        })))
    }

    pub(super) fn has_session(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("has-session", args)?;
        reject_positionals("has-session", &positional)?;
        self.resolve_session(options.value("-t"), context.session)?;
        Ok(Execution::default())
    }

    pub(super) fn server_access(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("server-access", args)?;
        let owner = server_socket_owner();
        if options.has("-l") {
            return Ok(Execution::output(format!("{owner} (U,W)")));
        }
        let Some(argument) = positional.first() else {
            return Err(ServerError::InvalidCommand(
                "missing user or group argument".to_owned(),
            ));
        };
        let argument = self.expand_pane_format(
            argument,
            context,
            context.session,
            context.target_format_client(),
            hooks,
        );
        let group = options.has("-g");
        let kind = if group { "group" } else { "user" };
        let Some((name, owns)) = server_access_identity(&argument, group) else {
            return Err(ServerError::InvalidCommand(format!(
                "unknown {kind}: {argument}"
            )));
        };
        if owns {
            return Err(ServerError::InvalidCommand(format!(
                "{name} owns the server, can't change access"
            )));
        }
        if options.has("-a") && options.has("-d") {
            return Err(ServerError::InvalidCommand(
                "-a and -d cannot be used together".to_owned(),
            ));
        }
        if options.has("-w") && options.has("-r") {
            return Err(ServerError::InvalidCommand(
                "-r and -w cannot be used together".to_owned(),
            ));
        }
        if options.has("-d") {
            return Err(ServerError::InvalidCommand(format!(
                "{kind} {name} not found"
            )));
        }
        if options.has("-a") || options.has("-r") || options.has("-w") {
            return Err(ServerError::InvalidCommand(format!(
                "zz has no socket access list: the daemon socket admits {owner} alone"
            )));
        }
        Ok(Execution::default())
    }
}

#[cfg(unix)]
pub(super) fn server_socket_owner() -> String {
    use nix::unistd::{Uid, User};

    User::from_uid(Uid::current())
        .ok()
        .flatten()
        .map_or_else(|| Uid::current().to_string(), |user| user.name)
}

#[cfg(not(unix))]
pub(super) fn server_socket_owner() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default()
}

#[cfg(unix)]
fn server_access_identity(argument: &str, group: bool) -> Option<(String, bool)> {
    use nix::unistd::{Group, Uid, User};

    if group {
        let entry = Group::from_name(argument).ok().flatten()?;
        return Some((entry.name, false));
    }
    let entry = User::from_name(argument).ok().flatten()?;
    let owns = entry.uid.is_root() || entry.uid == Uid::current();
    Some((entry.name, owns))
}

#[cfg(not(unix))]
fn server_access_identity(_argument: &str, _group: bool) -> Option<(String, bool)> {
    None
}

fn parse_new_session_attach_options(
    args: &[RawText],
) -> Result<(Options, Vec<RawText>), ServerError> {
    let spec = command_spec("new-session").expect("new-session has catalog metadata");
    let (options, positional) = parse_options_for_spec(args, spec)?;
    validate_options_allowing_unsupported("new-session", spec, &options, Some("-t"))?;
    spec.validate_positional_minimum(positional.len())?;
    spec.validate_positional_maximum(positional.len())?;
    Ok((options, positional))
}

pub(super) fn next_session_name(state: &MuxState) -> String {
    let mut used = vec![false; state.sessions.len().saturating_add(1)];
    for session in state.sessions.values() {
        let bytes = session.name.as_bytes();
        if bytes.is_empty()
            || (bytes.len() > 1 && bytes[0] == b'0')
            || !bytes.iter().all(u8::is_ascii_digit)
        {
            continue;
        }
        let Ok(candidate) = session.name.parse::<usize>() else {
            continue;
        };
        if let Some(slot) = used.get_mut(candidate) {
            *slot = true;
        }
    }
    used.iter()
        .position(|used| !used)
        .expect("one of n + 1 numeric names is unused")
        .to_string()
}
