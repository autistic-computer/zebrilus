use super::*;

impl Shared {
    pub(super) fn execute_with_mux_source_inner(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command: &CommandInvocation,
        mux_source: MuxOptionSource,
        invoking_client_terminal: ClientTerminal,
        queue_execution: Option<&CommandQueueExecution>,
        format_facts_unread: Option<bool>,
    ) -> Result<Execution, DaemonError> {
        let mut format_variables = context.format_variables.clone();
        let notifications_only = context.no_hooks && context.format_variables.contains_key("hook");
        let _notifications_only = ctrl::HookNotificationsOnlyScope::new(self, notifications_only);
        let event_hooks_enabled = !context.no_hooks || notifications_only;
        let command_name = canonical_command(&command.name);
        if command_name == "command-prompt" {
            self.ensure_prompt_history();
        }
        #[cfg(feature = "agent")]
        if matches!(command_name, "new-pane" | "split-window" | "new-window") {
            self.refuse_grandchild_chat(client, context, command_name, &command.args)?;
        }
        let read_only = hook_events::command_is_read_only(command_name, &command.args);
        let split_caller_stream = matches!(command_name, "split-window" | "new-pane")
            && command_stdin_sink(command_name, &command.args) == Some(CommandStdinSink::PaneInput);
        let mut terminals_to_watch = Vec::new();
        let mut client_events = Vec::new();
        let mut direct_events = Vec::new();
        let mut deferred_direct_events = Vec::new();
        let mut targeted_events = Vec::new();
        let mut source_files = Vec::new();
        let mut pane_stream_inputs: Vec<(PaneId, RawText)> = Vec::new();
        let mut removed_panes = Vec::new();
        let mut agent_panes_opened: Vec<(PaneId, Option<Arc<TerminalSession>>)> = Vec::new();
        let mut agent_briefs: Vec<(PaneId, String)> = Vec::new();
        #[cfg(feature = "agent")]
        let mut agent_panes_closed = Vec::new();
        #[cfg(feature = "agent")]
        let agent_children;
        let mut relocated_terminal_views = Vec::new();
        let mut retired_command_outputs = Vec::new();
        let mut deferred_terminal_commands = Vec::new();
        let mut terminal_wait = None;
        let mut key_listing = None;
        let mut injected_client_keys = Vec::new();
        let mut mode_table_keys = Vec::new();
        let mut pane_mode_keys = Vec::new();
        let mut pane_mode_pointers = Vec::new();
        let mut refresh_armed = false;
        let mut unfocused_copy_mode_exits = Vec::new();
        let mut pipes_to_close = Vec::new();
        let mut panes_mode_deadline: Option<(PaneId, u64, Instant)> = None;
        let mut panes_mode_overlays: Vec<(ClientId, Option<DisplayPanesState>)> = Vec::new();
        let mut client_message_retires = Vec::new();
        let mut client_message_schedule = None;
        let mut resume_client_terminals = false;
        let mut incremental_start = None;
        let mut prompt_cleared_message = None;
        let mut activity_requeues = Vec::new();
        let mut silence_schedules = Vec::new();
        let mut monitor_silence_changed = false;
        let mut client_terminal_changed = false;
        let mut attach = None;
        let mut detach = None;
        let mut import_tmux_config = None;
        let mut reload_config = false;
        let mut snapshot_changed = false;
        let mut respawned_terminals = false;
        let mut mux_options_changed = false;
        let mut mux_option_refresh_sessions = BTreeSet::new();
        #[cfg(feature = "agent")]
        let mut agent_options_changed = false;
        let mut status_formats_changed = false;
        let mut status_refresh_sessions = BTreeSet::new();
        let client_timers_changed;
        let mut mode_styles_changed = false;
        let mut force_shutdown_requested = false;
        let mut immediate_hooks = Vec::new();
        let mut option_signals = Vec::new();
        let mut pending_hook_events = Vec::new();
        let (mut execution, mux_options_event, recheck_shutdown_requested) = {
            let mut inner = self.inner.lock();
            let stale_mode_events = take_pane_mode_hook_events(&mut inner);
            format_variables.insert("config_files".to_owned(), inner.config.config_files.clone());
            let captures = !read_only || cfg!(debug_assertions);
            let generation_before = inner.engine.state.generation();
            let monitors_before = inner.engine.has_format_monitors();
            let refresh_before = (!read_only).then(|| copy_mode_refresh_needed(&inner));
            let clock_before = (!read_only).then(|| clock_mode_timer_needed(&inner));
            let hook_scope = captures.then(|| {
                hook_events::HookScope::open(&mut inner.engine)
                    .watching_borders(&inner.engine, command_name)
            });
            let pane_focus_before = (event_hooks_enabled && captures).then(|| {
                hook_events::FocusProbeScope::open_within(
                    &mut inner,
                    hook_scope
                        .as_ref()
                        .and_then(hook_events::HookScope::change_window),
                )
            });
            let copy_modes_before =
                (event_hooks_enabled && captures).then(|| active_copy_mode_panes(&inner));
            let captured_active = (captures && cfg!(debug_assertions)).then(|| {
                (
                    inner
                        .engine
                        .state
                        .sessions
                        .iter()
                        .map(|(session, state)| (*session, state.active_window))
                        .collect::<BTreeMap<_, _>>(),
                    inner
                        .engine
                        .state
                        .windows
                        .iter()
                        .map(|(window, state)| (*window, state.active_pane))
                        .collect::<BTreeMap<_, _>>(),
                    inner
                        .engine
                        .state
                        .windows
                        .values()
                        .flat_map(|window| window.panes.values())
                        .filter(|pane| pane.bell)
                        .map(|pane| pane.id)
                        .collect::<BTreeSet<_>>(),
                )
            });
            let mut focused_windows_before = BTreeMap::new();
            for (session, clients) in inner.attachment.attached.iter().filter(|_| captures) {
                let Some(state) = inner.engine.state.sessions.get(session) else {
                    continue;
                };
                for attached_client in clients {
                    focused_windows_before.insert(
                        (*session, *attached_client),
                        client_focused_window(&inner, *attached_client, state),
                    );
                }
            }
            let facts_unread = format_facts_unread
                .unwrap_or_else(|| hook_events::format_facts_unread(command_name, &command.args));
            let mut command_seed =
                (!facts_unread).then(|| command_format_seed(&inner, client, context));
            let mut built_facts = (!facts_unread
                && command_name == "new-session"
                && kind != ClientKind::Control
                && nested_attach_refusal(&inner, client).is_some())
            .then(|| format_hook_facts_for_client(&inner, client, context));
            if INVOKING_CLIENT_COMMANDS.contains(&command_name)
                && let Some(seed) = command_seed.as_mut()
            {
                match context.format_client() {
                    FormatClient::Attached(_) => {}
                    FormatClient::Unattached => {
                        seed.client = format_provenance_client(context, client)
                            .filter(|client| inner.client(*client).is_some())
                            .map(|client| match client_attached_session(&inner, client) {
                                Some(session) => client_format_facts(&inner, client, session),
                                None => unattached_client_format_facts(&inner, client),
                            });
                    }
                    FormatClient::NoClient => seed.client = None,
                }
            }
            if command_name == "display-message" && !facts_unread {
                let (target, target_client) = inner
                    .engine
                    .display_message_format_target(context, &command.args)?;
                let target_session = target.and_then(|target| target.session);
                let destination = target_client
                    .as_deref()
                    .and_then(|target| resolve_display_message_client(&inner, target));
                let format_client = target_session.and_then(|session| {
                    destination
                        .filter(|client| client_attached_session(&inner, *client) == Some(session))
                        .or_else(|| best_display_message_format_client(&inner, session))
                        .and_then(|client| {
                            client_attached_session(&inner, client)
                                .map(|client_session| (client, client_session))
                        })
                });
                if let Some(facts) = built_facts.as_mut() {
                    facts.client = format_client.map(|(client, client_session)| {
                        client_format_facts(&inner, client, client_session)
                    });
                }
                if let Some(seed) = command_seed.as_mut() {
                    seed.client = format_client.map(|(client, client_session)| {
                        client_format_facts(&inner, client, client_session)
                    });
                }
                context.set_format_client_session(
                    format_client.map(|(_, client_session)| client_session),
                );
            }
            let facts = built_facts
                .as_ref()
                .unwrap_or_else(|| hook_events::unread_format_facts());
            let mut hooks = DaemonFormatHooks::command_with_optional_variables(
                facts,
                (!format_variables.is_empty()).then_some(&format_variables),
            )
            .withhold_facts(facts_unread)
            .with_live_process_facts();
            inner.engine.set_format_now(unix_timestamp());
            let mut prospective_context = context.clone();
            set_context_client_terminal(&mut prospective_context, invoking_client_terminal);
            let nested_attach_guard = match (command_name, nested_attach_refusal(&inner, client)) {
                ("attach-session", Some(refusal)) => return Err(refusal.into()),
                ("new-session", Some(refusal))
                    if kind != ClientKind::Control
                        && inner.engine.new_session_needs_attach_preflight(
                            &prospective_context,
                            &command.args,
                            &mut hooks,
                        )? =>
                {
                    return Err(refusal.into());
                }
                (_, Some(refusal)) => Some((
                    refusal,
                    inner
                        .engine
                        .state
                        .sessions
                        .keys()
                        .copied()
                        .collect::<BTreeSet<_>>(),
                )),
                (_, None) => None,
            };
            context.set_client_size(inner.client(client).and_then(|c| c.size));
            context.set_client_working_directory(
                inner
                    .client(client)
                    .and_then(|c| c.working_directory.as_ref())
                    .cloned(),
            );
            if let Some(registered) = inner.client(client) {
                registered.seed_context_environment(context);
            }
            let previous_client_terminal = context_client_terminal(context);
            let previous_refuse_new_session_attach = context.refuses_new_session_attach();
            set_context_client_terminal(context, invoking_client_terminal);
            context.set_refuse_new_session_attach(nested_attach_guard.is_some());
            let mut pending_listing = None;
            let execution = if let Some(seed) = command_seed {
                let (engine, facts) = split_borrowed_format_hook_facts(&mut inner, seed);
                let mut hooks = DaemonFormatHooks::command_with_optional_variables(
                    &facts,
                    (!format_variables.is_empty()).then_some(&format_variables),
                )
                .with_live_process_facts();
                if command_name == "list-keys" {
                    hooks = hooks.with_command_item("list-keys");
                    cmdq::start_key_listing(
                        engine,
                        context,
                        &command.args,
                        &mut hooks,
                        &mut pending_listing,
                    )
                } else {
                    engine.execute_without_alias_expansion(
                        context,
                        command,
                        &mut hooks,
                        &mut |shell| shell_is_valid(Path::new(shell)),
                    )
                }
            } else if command_name == "list-keys" {
                hooks = hooks.with_command_item("list-keys");
                cmdq::start_key_listing(
                    &inner.engine,
                    context,
                    &command.args,
                    &mut hooks,
                    &mut pending_listing,
                )
            } else {
                inner.engine.execute_without_alias_expansion(
                    context,
                    command,
                    &mut hooks,
                    &mut |shell| shell_is_valid(Path::new(shell)),
                )
            };
            if let Some(listing) = pending_listing {
                key_listing = Some((
                    listing,
                    format_hook_facts_for_client(&inner, client, context),
                ));
            }
            context.set_refuse_new_session_attach(previous_refuse_new_session_attach);
            set_context_client_terminal(context, previous_client_terminal);
            let mut execution = execution?;
            if let Some((refusal, existing_sessions)) = nested_attach_guard
                && execution.effects.iter().any(|effect| {
                    matches!(
                        effect,
                        MuxEffect::Attach { session, .. } if existing_sessions.contains(session)
                    )
                })
            {
                context.retarget(&prospective_context);
                return Err(refusal.into());
            }
            if !inner.engine.state.sessions.is_empty() {
                self.exit_empty_armed.store(true, Ordering::Release);
            }
            let journal_active = hook_scope
                .as_ref()
                .and_then(|scope| scope.changes(&inner.engine))
                .map(|changes| {
                    (
                        hook_events::journal_active_windows(&changes),
                        hook_events::journal_active_panes(&changes),
                        hook_events::journal_belled_panes(&changes),
                    )
                });
            let (active_windows_before, active_panes_before, belled_panes_before) =
                match (captured_active, journal_active) {
                    (Some(full), Some(journal)) => {
                        hook_events::assert_same_active_changes(
                            &inner.engine.state,
                            &full,
                            &journal,
                        );
                        journal
                    }
                    (Some(full), None) => full,
                    (None, Some(journal)) => journal,
                    (None, None) => Default::default(),
                };
            let selected_panes = active_panes_before
                .iter()
                .filter_map(|(window, previous)| {
                    let state = inner.engine.state.windows.get(window)?;
                    (*previous != state.active_pane).then_some(state.active_pane)
                })
                .collect::<Vec<_>>();
            debug_assert!(
                !read_only || selected_panes.is_empty(),
                "read-only {command_name} selected a pane"
            );
            for pane in selected_panes {
                if inner.engine.state.clear_pane_alerts(pane) {
                    snapshot_changed = true;
                }
            }
            for pane in belled_panes_before {
                if inner.engine.state.pane(pane).is_none_or(|pane| !pane.bell) {
                    snapshot_changed = true;
                }
            }
            let attaching_session = execution.effects.iter().find_map(|effect| match effect {
                MuxEffect::Attach { session, .. }
                    if invoking_client_terminal == ClientTerminal::Present =>
                {
                    Some(*session)
                }
                _ => None,
            });
            let changed_windows = inner
                .engine
                .state
                .sessions
                .iter()
                .filter_map(|(session, state)| {
                    let client_selected_window = (matches!(
                        command_name,
                        "select-window" | "next-window" | "previous-window" | "last-window"
                    ) || attaching_session == Some(*session))
                        && context.session == Some(*session)
                        && context.window == Some(state.active_window)
                        && focused_windows_before
                            .get(&(*session, client))
                            .is_some_and(|previous| *previous != state.active_window);
                    (client_selected_window
                        || active_windows_before
                            .get(session)
                            .is_some_and(|previous| *previous != state.active_window))
                    .then_some((*session, state.active_window))
                })
                .collect::<Vec<_>>();
            debug_assert!(
                !read_only || changed_windows.is_empty(),
                "read-only {command_name} changed a window"
            );
            for (_, window) in &changed_windows {
                if let Some(deadline) = schedule_window_silence(&mut inner, *window) {
                    silence_schedules.push(deadline);
                }
                if inner.engine.monitor_activity_for_window(*window)
                    && let Some(state) = inner.engine.state.windows.get(window)
                {
                    activity_requeues.push((*window, state.active_pane));
                }
            }
            for (session, focused_window) in changed_windows {
                let attached = inner
                    .attachment
                    .attached
                    .get(&session)
                    .cloned()
                    .unwrap_or_default();
                if attached.contains(&client) || attaching_session == Some(session) {
                    for attached_client in attached {
                        let focus = if attached_client == client {
                            focused_window
                        } else {
                            focused_windows_before
                                .get(&(session, attached_client))
                                .copied()
                                .unwrap_or(focused_window)
                        };
                        inner
                            .client_entry(attached_client)
                            .focused_window
                            .replace(focus);
                    }
                } else {
                    for attached_client in attached {
                        inner
                            .client_entry(attached_client)
                            .focused_window
                            .replace(focused_window);
                    }
                }
            }
            let resized_windows = focused_windows_before
                .iter()
                .filter_map(|((session, attached_client), previous)| {
                    let state = inner.engine.state.sessions.get(session)?;
                    let current = client_focused_window(&inner, *attached_client, state);
                    (*previous != current).then_some([*previous, current])
                })
                .flatten()
                .collect::<BTreeSet<_>>();
            debug_assert!(
                !read_only || resized_windows.is_empty(),
                "read-only {command_name} moved a client's window"
            );
            let resized_panes = panes_for_windows(&inner, &resized_windows);
            snapshot_changed |= write_back_terminal_geometries(&mut inner, &resized_panes);
            let mut pane_format_output = None;
            for effect in &execution.effects {
                match effect {
                    MuxEffect::PaneCreated {
                        pane,
                        kind: PaneKindSnapshot::Terminal,
                        inherit_cwd_from,
                        cwd,
                        command,
                        ..
                    }
                    | MuxEffect::PaneMaterialized {
                        pane,
                        kind: PaneKindSnapshot::Terminal,
                        inherit_cwd_from,
                        cwd,
                        command,
                    } => {
                        let (pane_environment, empty) = match effect {
                            MuxEffect::PaneCreated {
                                environment, empty, ..
                            } => (environment.as_slice(), *empty),
                            MuxEffect::PaneMaterialized { .. } => (&[][..], false),
                            _ => unreachable!(),
                        };
                        let history_limit = inner.engine.history_limit_for_pane(*pane)?;
                        let word_separators =
                            WordSeparators::new(inner.engine.word_separators_for_pane(*pane)?);
                        let working_directory = match cwd.as_deref().map(PathBuf::from) {
                            Some(path) if path.is_dir() => Some(path),
                            Some(_) => std::env::var_os("HOME")
                                .map(PathBuf::from)
                                .filter(|path| path.is_dir())
                                .or_else(|| Some(PathBuf::from("/"))),
                            None => inherit_cwd_from
                                .and_then(|source| donor_working_directory(&inner, source))
                                .or_else(|| std::env::current_dir().ok()),
                        };
                        let start_path = working_directory
                            .as_deref()
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let terminal_options = terminal_worker_options(
                            &inner.engine,
                            &inner.config.appearance,
                            &inner.config.config_files,
                            *pane,
                        )?;
                        let appearance = Arc::clone(&terminal_options.appearance);
                        let pane_session = inner
                            .engine
                            .state
                            .window_for_pane(*pane)
                            .map(|window| inner.engine.state.windows[&window].session)
                            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
                        let start_command = if let Some(command) =
                            command.clone().filter(|command| !command.is_empty())
                        {
                            Some(command)
                        } else {
                            let default = inner.engine.default_command_for_session(pane_session)?;
                            (!default.is_empty()).then(|| vec![default.to_owned()])
                        };
                        let command = if empty {
                            command.clone()
                        } else {
                            start_command.clone()
                        };
                        inner
                            .engine
                            .set_pane_start_command(*pane, start_command.unwrap_or_default())?;
                        let shell = terminal_shell_for_session(&inner.engine, pane_session)?;
                        inner.engine.set_pane_shell(*pane, shell.clone())?;
                        let shell = Some(shell);
                        let env = if empty {
                            Vec::new()
                        } else {
                            let mut env =
                                terminal_environment_for_session(&inner.engine, pane_session)?;
                            env.push((
                                "TMUX".into(),
                                Some(
                                    tmux_environment(&self.socket_path, Some(pane_session)).into(),
                                ),
                            ));
                            env.extend(
                                pane_environment
                                    .iter()
                                    .map(|(name, value)| (name.into(), Some(value.into()))),
                            );
                            env.extend([
                                (OsString::from("ZZ_PANE"), Some(pane.to_string().into())),
                                (
                                    "ZZ_SOCKET".into(),
                                    Some(self.socket_path.as_os_str().to_owned()),
                                ),
                                ("ZZ_SESSION".into(), Some(pane_session.to_string().into())),
                                ("TMUX_PANE".into(), Some(pane.to_string().into())),
                            ]);
                            #[cfg(unix)]
                            if let Some(shim) = self.tmux_shim.lock().as_ref() {
                                crate::configure_pane_tmux_environment(
                                    &mut env,
                                    &shim.directory,
                                    &shim.executable,
                                );
                            }
                            if let Some(path) = &working_directory {
                                env.push(("PWD".into(), Some(path.as_os_str().to_owned())));
                            }
                            env
                        };
                        let spawn = TerminalSpawn {
                            knobs: terminal_options.knobs,
                            working_directory: working_directory.clone(),
                            command,
                            shell,
                            terminal_type: Some(
                                inner.engine.default_terminal_for_spawn().to_owned(),
                            ),
                            initial_size: inner
                                .engine
                                .pane_geometry(*pane)
                                .map(|(columns, rows)| TerminalSize::cells(columns, rows)),
                            non_login_shell: false,
                            env,
                            word_separators: Some(word_separators.clone()),
                            allow_passthrough: Some(terminal_options.allow_passthrough),
                            wrap_search: Some(terminal_options.wrap_search),
                        };
                        let session = Arc::new(if empty {
                            TerminalSession::spawn_empty_pane(
                                history_limit,
                                appearance,
                                spawn.initial_size,
                            )
                        } else {
                            TerminalSession::spawn(history_limit, appearance, spawn.clone())
                        });
                        let current_path = if empty {
                            start_path.clone()
                        } else {
                            terminal_working_directory(&session).map_or_else(
                                || start_path.clone(),
                                |path| path.to_string_lossy().into_owned(),
                            )
                        };
                        if empty {
                            session.set_word_separators(word_separators);
                            session.set_wrap_search(terminal_options.wrap_search);
                        }
                        if let Some(previous) =
                            inner.terminals_mut().insert(*pane, Arc::clone(&session))
                        {
                            retire_terminal(&previous);
                        }
                        inner.panes.terminal_spawns.insert(*pane, spawn);
                        if context.spawns_popup() {
                            inner.panes.popup_panes.insert(*pane);
                        }
                        inner.engine.set_pane_runtime_facts_with_hooks(
                            *pane,
                            PaneRuntimeFacts {
                                current_path,
                                start_path,
                                pid: session.process_id(),
                                tty: session
                                    .tty()
                                    .map(|path| path.to_string_lossy().into_owned())
                                    .unwrap_or_default(),
                                ..PaneRuntimeFacts::default()
                            },
                            &mut hooks,
                        );
                        let attached_clients = inner
                            .engine
                            .state
                            .window_for_pane(*pane)
                            .map(|window| inner.engine.state.windows[&window].session)
                            .and_then(|session| inner.attachment.attached.get(&session))
                            .cloned()
                            .unwrap_or_default();
                        for client in attached_clients {
                            deferred_terminal_commands.push(DeferredTerminalCommand::AttachView {
                                terminal: Arc::clone(&session),
                                view: TerminalViewId(client.0),
                            });
                        }
                        terminals_to_watch.push((*pane, session));
                    }
                    MuxEffect::PaneRespawned {
                        pane,
                        cwd,
                        command,
                        environment,
                        empty,
                    } => {
                        #[cfg(feature = "agent")]
                        if inner.agents.agent_panes.remove(pane) {
                            agent_panes_closed.push(*pane);
                        }
                        clear_pane_modes(&mut inner, *pane);
                        let previous = inner
                            .panes
                            .terminal_spawns
                            .get(pane)
                            .cloned()
                            .unwrap_or_default();
                        let history_limit = inner.engine.history_limit_for_pane(*pane)?;
                        let word_separators =
                            WordSeparators::new(inner.engine.word_separators_for_pane(*pane)?);
                        let working_directory = match cwd.as_deref().map(PathBuf::from) {
                            Some(path) => {
                                let path = if path.is_absolute() {
                                    path
                                } else if let Some(base) = &previous.working_directory {
                                    base.join(&path)
                                } else {
                                    path
                                };
                                if path.is_dir() {
                                    Some(path)
                                } else {
                                    std::env::var_os("HOME")
                                        .map(PathBuf::from)
                                        .filter(|path| path.is_dir())
                                        .or_else(|| Some(PathBuf::from("/")))
                                }
                            }
                            None => previous
                                .working_directory
                                .clone()
                                .or_else(|| std::env::current_dir().ok()),
                        };
                        let start_path = working_directory
                            .as_deref()
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let terminal_options = terminal_worker_options(
                            &inner.engine,
                            &inner.config.appearance,
                            &inner.config.config_files,
                            *pane,
                        )?;
                        let appearance = Arc::clone(&terminal_options.appearance);
                        let pane_session = inner
                            .engine
                            .state
                            .window_for_pane(*pane)
                            .map(|window| inner.engine.state.windows[&window].session)
                            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
                        let command = command
                            .clone()
                            .filter(|command| !command.is_empty())
                            .or_else(|| previous.command.clone());
                        let shell = match previous.shell.clone() {
                            Some(shell) => shell,
                            None => terminal_shell_for_session(&inner.engine, pane_session)?,
                        };
                        inner.engine.set_pane_shell(*pane, shell.clone())?;
                        let shell = Some(shell);
                        let mut env =
                            terminal_environment_for_session(&inner.engine, pane_session)?;
                        env.extend([
                            (OsString::from("ZZ_PANE"), Some(pane.to_string().into())),
                            (
                                "ZZ_SOCKET".into(),
                                Some(self.socket_path.as_os_str().to_owned()),
                            ),
                            ("ZZ_SESSION".into(), Some(pane_session.to_string().into())),
                            (
                                "TMUX".into(),
                                Some(
                                    tmux_environment(&self.socket_path, Some(pane_session)).into(),
                                ),
                            ),
                        ]);
                        env.extend(
                            environment
                                .iter()
                                .map(|(name, value)| (name.into(), Some(value.into()))),
                        );
                        env.push(("TMUX_PANE".into(), Some(pane.to_string().into())));
                        #[cfg(unix)]
                        if let Some(shim) = self.tmux_shim.lock().as_ref() {
                            crate::configure_pane_tmux_environment(
                                &mut env,
                                &shim.directory,
                                &shim.executable,
                            );
                        }
                        if let Some(path) = &working_directory {
                            env.push(("PWD".into(), Some(path.as_os_str().to_owned())));
                        }
                        let spawn = TerminalSpawn {
                            knobs: terminal_options.knobs,
                            working_directory: working_directory.clone(),
                            command,
                            shell,
                            terminal_type: Some(
                                inner.engine.default_terminal_for_spawn().to_owned(),
                            ),
                            initial_size: inner
                                .engine
                                .pane_geometry(*pane)
                                .map(|(columns, rows)| TerminalSize::cells(columns, rows)),
                            non_login_shell: false,
                            env,
                            word_separators: Some(word_separators.clone()),
                            allow_passthrough: Some(terminal_options.allow_passthrough),
                            wrap_search: Some(terminal_options.wrap_search),
                        };
                        let session = Arc::new(if *empty {
                            TerminalSession::spawn_empty_pane(
                                history_limit,
                                appearance,
                                spawn.initial_size,
                            )
                        } else {
                            TerminalSession::spawn(history_limit, appearance, spawn.clone())
                        });
                        let current_path = if *empty {
                            start_path.clone()
                        } else {
                            terminal_working_directory(&session).map_or_else(
                                || start_path.clone(),
                                |path| path.to_string_lossy().into_owned(),
                            )
                        };
                        if *empty {
                            session.set_word_separators(word_separators);
                            session.set_wrap_search(terminal_options.wrap_search);
                        }
                        Self::wake_pane_exit_wait(&mut inner, *pane, 0);
                        if let Some(entry) = inner.panes.pane_exit_waits.get_mut(pane) {
                            let command_wait = entry.command_wait.take();
                            *entry = PaneExitWait::new(&session);
                            entry.command_wait = command_wait;
                        }
                        if let Some(previous) =
                            inner.terminals_mut().insert(*pane, Arc::clone(&session))
                        {
                            retire_terminal(&previous);
                        }
                        inner.panes.terminal_spawns.insert(*pane, spawn);
                        if context.spawns_popup() {
                            inner.panes.popup_panes.insert(*pane);
                        }
                        for streamed in inner
                            .clients
                            .values_mut()
                            .filter_map(|c| c.streamed_terminals.as_mut())
                        {
                            streamed.remove(pane);
                        }
                        for feed in inner
                            .clients
                            .values()
                            .filter_map(|c| c.subscriber.as_ref())
                            .filter_map(|subscriber| subscriber.control_feed())
                        {
                            feed.reset_pane(*pane);
                        }
                        inner.panes.preview_watched.remove(pane);
                        respawned_terminals = true;
                        inner.engine.set_pane_runtime_facts_with_hooks(
                            *pane,
                            PaneRuntimeFacts {
                                current_path,
                                start_path,
                                ..PaneRuntimeFacts::default()
                            },
                            &mut hooks,
                        );
                        let attached_clients = inner
                            .attachment
                            .attached
                            .get(&pane_session)
                            .cloned()
                            .unwrap_or_default();
                        for client in attached_clients {
                            deferred_terminal_commands.push(DeferredTerminalCommand::AttachView {
                                terminal: Arc::clone(&session),
                                view: TerminalViewId(client.0),
                            });
                        }
                        if let Some((_, geometry)) = terminal_resize_for_pane(&inner, *pane) {
                            deferred_terminal_commands.push(DeferredTerminalCommand::Resize {
                                terminal: Arc::clone(&session),
                                geometry,
                            });
                        }
                        if let Some(pipe) = inner.jobs.pane_pipes.get_mut(pane) {
                            *pipe.terminal.lock() = Arc::clone(&session);
                            #[cfg(unix)]
                            pipe.jobs.wake.wake();
                        }
                        terminals_to_watch.push((*pane, session));
                    }
                    MuxEffect::PaneCreated {
                        pane,
                        kind: PaneKindSnapshot::Agent(descriptor),
                        inherit_cwd_from,
                        ..
                    }
                    | MuxEffect::PaneMaterialized {
                        pane,
                        kind: PaneKindSnapshot::Agent(descriptor),
                        inherit_cwd_from,
                        ..
                    } => {
                        let working_directory = descriptor
                            .cwd
                            .clone()
                            .filter(|path| path.is_dir())
                            .or_else(|| {
                                inherit_cwd_from
                                    .and_then(|source| donor_working_directory(&inner, source))
                            })
                            .or_else(|| std::env::current_dir().ok());
                        let folder = working_directory
                            .as_deref()
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        inner
                            .engine
                            .state
                            .update_agent(*pane, |agent| agent.cwd = working_directory)?;
                        inner.engine.set_pane_runtime_facts(
                            *pane,
                            PaneRuntimeFacts {
                                current_path: folder.clone(),
                                start_path: folder,
                                ..PaneRuntimeFacts::default()
                            },
                        );
                        let replaced = inner.terminals_mut().remove(pane);
                        if let Some(terminal) = &replaced {
                            clear_pane_modes(&mut inner, *pane);
                            pane_summary::forget_pane_program(&mut inner, *pane);
                            inner.panes.program_status_panes.remove(pane);
                            inner.panes.pane_read_observations.remove(pane);
                            for streamed in inner
                                .clients
                                .values_mut()
                                .filter_map(|c| c.streamed_terminals.as_mut())
                            {
                                streamed.remove(pane);
                            }
                            terminal.terminate();
                            retire_terminal(terminal);
                        }
                        agent_panes_opened.push((*pane, replaced));
                    }
                    MuxEffect::AgentForked { pane, at } => {
                        #[cfg(feature = "agent")]
                        inner.agents.agent_forks.insert(*pane, at.clone());
                        #[cfg(not(feature = "agent"))]
                        let _ = (pane, at);
                    }
                    MuxEffect::AgentBrief { pane, text } => {
                        agent_briefs.push((*pane, text.clone()));
                    }
                    MuxEffect::PaneCreated {
                        kind:
                            PaneKindSnapshot::Browser(_)
                            | PaneKindSnapshot::Picker
                            | PaneKindSnapshot::View(_)
                            | PaneKindSnapshot::Other { .. },
                        ..
                    }
                    | MuxEffect::PaneMaterialized {
                        kind:
                            PaneKindSnapshot::Browser(_)
                            | PaneKindSnapshot::Picker
                            | PaneKindSnapshot::View(_)
                            | PaneKindSnapshot::Other { .. },
                        ..
                    }
                    | MuxEffect::SuppressAfterHook
                    | MuxEffect::PaneMovedInWindow { .. }
                    | MuxEffect::ZoomCycled { .. } => {}
                    MuxEffect::ArmMouseDrag(drag) => {
                        if let Some(registered) = inner.client_mut(client) {
                            registered.mouse_drag = Some(*drag);
                        }
                    }
                    MuxEffect::PaneWaitForExit { pane } => {
                        if let Some(terminal) = inner.terminals.get(pane).cloned() {
                            let entry = inner
                                .panes
                                .pane_exit_waits
                                .entry(*pane)
                                .or_insert_with(|| PaneExitWait::new(&terminal));
                            entry.command_wait = Some(Arc::clone(&entry.current));
                        }
                    }
                    MuxEffect::PaneCreated {
                        pane,
                        kind: PaneKindSnapshot::Editor(_),
                        inherit_cwd_from,
                        ..
                    }
                    | MuxEffect::PaneMaterialized {
                        pane,
                        kind: PaneKindSnapshot::Editor(_),
                        inherit_cwd_from,
                        ..
                    } => {
                        let working_directory = inherit_cwd_from
                            .and_then(|source| donor_working_directory(&inner, source))
                            .or_else(|| std::env::current_dir().ok());
                        if let Some(working_directory) = working_directory {
                            inner.engine.state.update_editor_cwd(
                                *pane,
                                working_directory.to_string_lossy().into_owned(),
                            )?;
                        }
                    }
                    MuxEffect::PanesRemoved(panes) => {
                        let clients = inner
                            .clients
                            .iter()
                            .filter_map(|(id, client)| {
                                client.command_output.as_ref().map(|value| (id, value))
                            })
                            .filter_map(|(client, output)| {
                                panes.contains(&output.pane).then_some(*client)
                            })
                            .collect::<Vec<_>>();
                        for client in clients {
                            if let Some(output) = take_command_output(&mut inner, client) {
                                retired_command_outputs.push((client, output));
                            }
                        }
                        for pane in panes {
                            if let Some(pipe) = inner.jobs.pane_pipes.remove(pane) {
                                pipes_to_close.push(pipe);
                            }
                            if let Some(editor) = inner.modes.editors.remove(pane) {
                                let _ = std::fs::remove_file(&editor.path);
                            }
                            Self::release_removed_pane_wait(&mut inner, *pane);
                            if let Some(terminal) = inner.terminals_mut().remove(pane) {
                                retire_terminal(&terminal);
                            }
                            inner.panes.name_checks.remove(pane);
                            pane_summary::forget_pane_program(&mut inner, *pane);
                            inner.panes.pane_read_observations.remove(pane);
                            inner.panes.control_activity_pending.remove(pane);
                            inner.panes.program_status_panes.remove(pane);
                            inner.panes.popup_panes.remove(pane);
                            #[cfg(all(feature = "agent", unix))]
                            inner.agents.claude_peer_states.remove(pane);
                            inner.panes.terminal_spawns.remove(pane);
                            inner.panes.terminal_geometries.remove(pane);
                            inner.panes.reported_pane_places.remove(pane);
                            inner.modes.pane_modes.remove(pane);
                            inner.modes.pane_mode_zooms.remove(pane);
                            inner.modes.panes_mode_frames.remove(pane);
                            inner.engine.set_pane_status_hidden(*pane, false);
                            inner
                                .transfers
                                .paste_uploads
                                .retain(|_, upload| upload.pane != *pane);
                            removed_panes.push(*pane);
                        }
                    }
                    MuxEffect::PaneRelocated { pane, from, to } => {
                        if let Some(terminal) = inner.terminals.get(pane).cloned() {
                            let terminal_options = terminal_worker_options(
                                &inner.engine,
                                &inner.config.appearance,
                                &inner.config.config_files,
                                *pane,
                            )?;
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetWordSeparators {
                                    terminal: Arc::clone(&terminal),
                                    separators: WordSeparators::new(
                                        inner.engine.word_separators_for_session(*to),
                                    ),
                                },
                            );
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetAllowPassthrough {
                                    terminal: Arc::clone(&terminal),
                                    enabled: terminal_options.allow_passthrough,
                                },
                            );
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetWrapSearch {
                                    terminal: Arc::clone(&terminal),
                                    enabled: terminal_options.wrap_search,
                                },
                            );
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetAppearance {
                                    terminal: Arc::clone(&terminal),
                                    appearance: terminal_options.appearance,
                                },
                            );
                            let previewing = previewing_clients(&inner);
                            relocated_terminal_views.push((
                                terminal,
                                inner
                                    .attachment
                                    .attached
                                    .get(from)
                                    .cloned()
                                    .unwrap_or_default()
                                    .union(&previewing)
                                    .copied()
                                    .collect::<BTreeSet<_>>(),
                                inner
                                    .attachment
                                    .attached
                                    .get(to)
                                    .cloned()
                                    .unwrap_or_default()
                                    .union(&previewing)
                                    .copied()
                                    .collect::<BTreeSet<_>>(),
                            ));
                        }
                        for output in inner
                            .clients
                            .values()
                            .filter_map(|c| c.command_output.as_ref())
                            .filter(|output| output.pane == *pane)
                        {
                            let terminal_options = terminal_worker_options(
                                &inner.engine,
                                &inner.config.appearance,
                                &inner.config.config_files,
                                *pane,
                            )?;
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetWrapSearch {
                                    terminal: Arc::clone(&output.terminal),
                                    enabled: terminal_options.wrap_search,
                                },
                            );
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetAppearance {
                                    terminal: Arc::clone(&output.terminal),
                                    appearance: terminal_options.appearance,
                                },
                            );
                        }
                    }
                    MuxEffect::SendKeys { pane, keys, repeat } => {
                        if is_agent_chat(&inner, *pane) {
                            return Err(agent_chat_refusal(command_name, *pane).into());
                        }
                        if inner.modes.pane_modes.contains_key(pane) {
                            let target = if command_name == "send-keys" {
                                send_keys_target_client(&command.args)?
                            } else {
                                None
                            };
                            let selected = match target {
                                Some(target) => {
                                    find_attached_client_with_aliases(&inner, &target, true)
                                }
                                None => current_format_client(&inner, client),
                            };
                            if let Some(selected) = selected {
                                pane_mode_keys.push((selected, *pane, keys.clone(), *repeat));
                            }
                            continue;
                        }
                        let owners = copy_mode_key_owners(&inner, client, *pane);
                        if !owners.is_empty() {
                            mode_table_keys.push((owners, *pane, keys.clone(), *repeat));
                            continue;
                        }
                        let sinks = resolve_input_sinks(&inner, *pane)?;
                        let mut terminals = Vec::new();
                        for sink in sinks {
                            match sink {
                                PaneSink::Terminal(terminal) => {
                                    terminals.push(terminal);
                                }
                                PaneSink::Browser(target) => {
                                    client_events.push(EventPayload::BrowserCommand {
                                        pane: target,
                                        command: if *repeat == 1 {
                                            BrowserCommand::SendKeys(keys.clone())
                                        } else {
                                            BrowserCommand::SendKeysRepeated {
                                                keys: keys.clone(),
                                                count: (*repeat).min(MAX_BROWSER_KEY_REPEAT),
                                            }
                                        },
                                    });
                                }
                            }
                        }
                        if !terminals.is_empty() {
                            deferred_terminal_commands.push(DeferredTerminalCommand::SendTokens {
                                terminals,
                                keys: keys.clone(),
                                repeat: *repeat,
                            });
                        }
                    }
                    MuxEffect::SendClientKeys {
                        target_client,
                        keys,
                        repeat,
                    } => {
                        let selected = match target_client.as_deref() {
                            Some(target) => find_attached_client_with_aliases(&inner, target, true),
                            None => current_format_client(&inner, client),
                        };
                        if let Some(selected) = selected {
                            injected_client_keys.push((selected, keys.clone(), *repeat));
                        }
                    }
                    MuxEffect::ResetPane { pane } => {
                        if is_agent_chat(&inner, *pane) {
                            return Err(agent_chat_refusal(command_name, *pane).into());
                        }
                        let mut terminals = Vec::new();
                        for sink in resolve_input_sinks(&inner, *pane)? {
                            if let PaneSink::Terminal(terminal) = sink {
                                terminals.push(terminal);
                            }
                        }
                        if !terminals.is_empty() {
                            deferred_terminal_commands
                                .push(DeferredTerminalCommand::ResetScreen { terminals });
                        }
                    }
                    MuxEffect::ArmCopyModeSource { pane, source } => {
                        if !inner.terminals.contains_key(source) {
                            return Err(ServerError::PaneExited(*source).into());
                        }
                        inner.modes.pending_copy_source = Some((*pane, *source));
                    }
                    MuxEffect::ArmCopyModeKill { pane } => {
                        if !inner
                            .clients
                            .values()
                            .filter_map(|c| c.copy_session.as_ref())
                            .any(|session| session.pane == *pane && !session.exiting)
                        {
                            inner.modes.pending_copy_kill = Some(*pane);
                        }
                    }
                    MuxEffect::CopyModeRepeat {
                        pane,
                        count,
                        target_client,
                        require_mode,
                    } => {
                        if *require_mode && !pane_carries_a_mode_command(&inner, client, *pane) {
                            return Err(
                                ServerError::InvalidCommand("not in a mode".to_owned()).into()
                            );
                        }
                        if *count == 0 {
                            continue;
                        }
                        let selected =
                            selected_effect_client(&inner, client, target_client.as_deref());
                        let owners: Vec<ClientId> = match selected {
                            Some(selected)
                                if inner
                                    .client(selected)
                                    .and_then(|c| c.copy_session.as_ref())
                                    .is_some_and(|session| session.pane == *pane) =>
                            {
                                vec![selected]
                            }
                            _ => inner
                                .clients
                                .iter()
                                .filter_map(|(id, client)| {
                                    client.copy_session.as_ref().map(|value| (id, value))
                                })
                                .filter(|(_, session)| session.pane == *pane)
                                .map(|(owner, _)| *owner)
                                .collect(),
                        };
                        for owner in owners {
                            inner
                                .client_entry(owner)
                                .key_engine
                                .get_or_insert_default()
                                .set_repeat_count(*count);
                        }
                    }
                    MuxEffect::TerminalView {
                        pane,
                        action,
                        target_client,
                        require_mode,
                    } => {
                        if command_name == "send-keys"
                            && let Some(mouse) = context.invoking_mouse().cloned()
                            && !require_mode
                            && consume_pane_mode_mouse(&mut inner, *pane, Some(action))
                        {
                            if let Some(key) = context.invoking_key()
                                && let Some((x, y)) = mouse_pane_cell(&inner, *pane, &mouse)
                            {
                                pane_mode_pointers.push((*pane, key.to_owned(), x, y));
                            }
                            snapshot_changed = true;
                            continue;
                        }
                        if command_name == "send-keys"
                            && context.invoking_mouse().is_some()
                            && !require_mode
                            && context
                                .invoking_key()
                                .is_some_and(zz_mux::mouse_key_is_a_replayed_double_click)
                        {
                            continue;
                        }
                        if matches!(action, zz_terminal::TerminalViewAction::ClearHistory)
                            && clear_pane_modes(&mut inner, *pane)
                        {
                            snapshot_changed = true;
                        }
                        let armed_copy_kill = inner.modes.pending_copy_kill.take();
                        let armed_copy_source = inner.modes.pending_copy_source.take();
                        if *require_mode && !pane_carries_a_mode_command(&inner, client, *pane) {
                            return Err(
                                ServerError::InvalidCommand("not in a mode".to_owned()).into()
                            );
                        }
                        let command_output = inner
                            .client(client)
                            .and_then(|c| c.command_output.as_ref())
                            .map(|output| Arc::clone(&output.terminal));
                        if let Some(terminal) = command_output {
                            deferred_terminal_commands.push(DeferredTerminalCommand::ViewAction {
                                terminal,
                                view: TerminalViewId(client.0),
                                action: action.clone(),
                            });
                            continue;
                        }
                        let terminal = inner
                            .terminals
                            .get(pane)
                            .cloned()
                            .ok_or(ServerError::PaneExited(*pane))?;
                        let caller_attached = client_is_attached_to_pane(&inner, client, *pane);
                        if inner.attachment.client_flags.contains(client) && !caller_attached {
                            return Err(ServerError::InvalidCommand(
                                "client is read-only".to_owned(),
                            )
                            .into());
                        }
                        let selected =
                            selected_effect_client(&inner, client, target_client.as_deref());
                        let targets: Vec<ClientId> = match selected {
                            Some(selected)
                                if client_is_attached_to_pane(&inner, selected, *pane) =>
                            {
                                vec![selected]
                            }
                            _ => attached_clients_for_pane(&inner, *pane)
                                .map(|clients| clients.iter().copied().collect())
                                .unwrap_or_default(),
                        };
                        if targets.is_empty() {
                            if matches!(
                                action,
                                zz_terminal::TerminalViewAction::CopyMode(
                                    zz_terminal::CopyModeAction::Cancel
                                )
                            ) {
                                continue;
                            }
                            return Err(ServerError::PaneNotAttached(*pane).into());
                        }
                        inner.modes.pending_copy_kill = armed_copy_kill;
                        let copy_source = armed_copy_source
                            .filter(|(armed, _)| armed == pane)
                            .and_then(|(_, source)| inner.terminals.get(&source).cloned());
                        for target in targets {
                            let action = &counted_copy_mode_action(&mut inner, target, action);
                            if let Some(source) = &copy_source
                                && terminal_view_action_enters_copy_mode(action)
                            {
                                deferred_terminal_commands.push(
                                    DeferredTerminalCommand::ArmCopySource {
                                        terminal: Arc::clone(&terminal),
                                        source: Arc::clone(source),
                                    },
                                );
                            }
                            deferred_terminal_commands.push(DeferredTerminalCommand::ViewAction {
                                terminal: Arc::clone(&terminal),
                                view: TerminalViewId(target.0),
                                action: action.clone(),
                            });
                            if terminal_view_action_enters_copy_mode(action) {
                                enter_copy_session(&mut inner, target, *pane)?;
                                if copy_source.is_some()
                                    && let Some(session) = inner
                                        .client_mut(target)
                                        .and_then(|c| c.copy_session.as_mut())
                                {
                                    session.sourced = true;
                                    session.refresh = false;
                                }
                            } else if terminal_view_action_exits_copy_mode(action) {
                                begin_exit_copy_session(&mut inner, target);
                            } else if terminal_view_action_arms_scroll_exit(action)
                                && let Some(session) = inner
                                    .client_mut(target)
                                    .and_then(|c| c.copy_session.as_mut())
                                && session.pane == *pane
                            {
                                session.scroll_exit = true;
                            } else if terminal_view_action_refreshes_now(action)
                                && let Some(session) = inner
                                    .client_mut(target)
                                    .and_then(|c| c.copy_session.as_mut())
                                && session.pane == *pane
                                && !session.sourced
                            {
                                session.unseen = false;
                            } else if let Some(refresh) =
                                terminal_view_action_refresh_request(action)
                                && let Some(session) = inner
                                    .client_mut(target)
                                    .and_then(|c| c.copy_session.as_mut())
                                && session.pane == *pane
                            {
                                session.refresh =
                                    !session.sourced && refresh.applied(session.refresh);
                                refresh_armed |= session.refresh;
                            }
                        }
                    }
                    MuxEffect::TerminalUi { pane, command } => {
                        if let Some(output) =
                            inner.client(client).and_then(|c| c.command_output.as_ref())
                        {
                            client_events.push(EventPayload::TerminalUiCommand {
                                pane: output.pane,
                                command: *command,
                            });
                        } else if !inner.terminals.contains_key(pane) {
                            return Err(ServerError::PaneExited(*pane).into());
                        } else {
                            client_events.push(EventPayload::TerminalUiCommand {
                                pane: *pane,
                                command: *command,
                            });
                        }
                    }
                    MuxEffect::FocusSidebar { pane } => {
                        if kind != ClientKind::Interactive
                            || inner.client(client).is_none_or(|c| c.subscriber.is_none())
                        {
                            return Err(ServerError::InvalidCommand(
                                "focus-sidebar requires an interactive client".to_owned(),
                            )
                            .into());
                        }
                        if !client_is_attached_to_pane(&inner, client, *pane) {
                            return Err(ServerError::PaneNotAttached(*pane).into());
                        }
                        dismiss_overlays(
                            &mut inner,
                            client,
                            None,
                            &mut direct_events,
                            &mut retired_command_outputs,
                            &mut resume_client_terminals,
                        );
                        inner
                            .client_mut(client)
                            .and_then(|c| c.swallowed_keys.take());
                        direct_events.push(EventPayload::FocusSidebar);
                    }
                    MuxEffect::ChoosePath { pane, start_dir } => {
                        if kind != ClientKind::Interactive
                            || inner.client(client).is_none_or(|c| c.subscriber.is_none())
                        {
                            return Err(ServerError::InvalidCommand(
                                "choose-path requires an interactive client".to_owned(),
                            )
                            .into());
                        }
                        if !inner.client(client).is_some_and(|c| c.path_picker) {
                            return Err(ServerError::InvalidCommand(
                                "choose-path needs the zz desktop app".to_owned(),
                            )
                            .into());
                        }
                        if !client_is_attached_to_pane(&inner, client, *pane)
                            || inner.engine.state.window_for_pane(*pane)
                                != client_focused_window_for_attachment(&inner, client)
                        {
                            return Err(ServerError::PaneNotAttached(*pane).into());
                        }
                        if inner.attachment.client_flags.contains(client) {
                            return Err(ServerError::InvalidCommand(
                                "client is read-only".to_owned(),
                            )
                            .into());
                        }
                        if inner
                            .client(client)
                            .and_then(|c| c.copy_session.as_ref())
                            .is_some_and(|session| session.pane == *pane)
                        {
                            return Err(ServerError::InvalidCommand(
                                "choose-path cannot run while the pane is in copy mode".to_owned(),
                            )
                            .into());
                        }
                        if !inner.terminals.contains_key(pane) {
                            return Err(ServerError::InvalidCommand(
                                "choose-path needs a terminal pane".to_owned(),
                            )
                            .into());
                        }
                        if start_dir.as_deref().is_some_and(|start_dir| {
                            start_dir.len() > zz_protocol::MAX_PATH_LIST_TEXT_BYTES
                                || start_dir.chars().any(char::is_control)
                        }) {
                            return Err(ServerError::InvalidCommand(
                                "choose-path start directory cannot be listed".to_owned(),
                            )
                            .into());
                        }
                        dismiss_overlays(
                            &mut inner,
                            client,
                            None,
                            &mut direct_events,
                            &mut retired_command_outputs,
                            &mut resume_client_terminals,
                        );
                        inner
                            .client_mut(client)
                            .and_then(|c| c.swallowed_keys.take());
                        inner
                            .client_mut(client)
                            .and_then(|c| c.path_list_root.take());
                        direct_events.push(EventPayload::OpenPathPicker {
                            pane: *pane,
                            start_dir: start_dir.clone(),
                        });
                    }
                    MuxEffect::CommandPrompt {
                        steps,
                        template,
                        source,
                        prompt_type,
                        mode,
                        no_freeze,
                        pane,
                    } => {
                        if kind != ClientKind::Interactive
                            || inner.client(client).is_none_or(|c| c.subscriber.is_none())
                        {
                            return Err(ServerError::InvalidCommand(
                                "command-prompt requires an interactive client".to_owned(),
                            )
                            .into());
                        }
                        if inner
                            .client(client)
                            .is_some_and(|c| c.native_terminal_search)
                            && inner
                                .client(client)
                                .is_none_or(|c| c.command_prompt.is_none())
                            && let Some(direction) = native_copy_search_direction(
                                steps,
                                template.as_ref(),
                                *prompt_type,
                                *mode,
                            )
                            && let Some(pane) = inner
                                .client(client)
                                .and_then(|c| c.command_output.as_ref())
                                .map(|output| output.pane)
                                .or_else(|| {
                                    inner
                                        .client(client)
                                        .and_then(|c| c.copy_session.as_ref())
                                        .map(|session| session.pane)
                                })
                        {
                            client_events.push(EventPayload::TerminalUiCommand {
                                pane,
                                command: zz_protocol::TerminalUiCommand::BeginSearch { direction },
                            });
                            continue;
                        }
                        dismiss_overlays(
                            &mut inner,
                            client,
                            Some(Overlay::CommandPrompt),
                            &mut direct_events,
                            &mut retired_command_outputs,
                            &mut resume_client_terminals,
                        );
                        if inner
                            .client(client)
                            .is_none_or(|c| c.command_prompt.is_none())
                        {
                            let (vi_keys, word_separators) = inner
                                .engine
                                .prompt_key_options(client_attached_session(&inner, client));
                            let mut prompt = CommandPrompt::new(
                                steps.clone(),
                                template.clone(),
                                source.clone(),
                                *prompt_type,
                                *mode,
                                *no_freeze,
                                *pane,
                            )
                            .with_key_options(vi_keys, word_separators)
                            .with_cursors(prompt_cursors(
                                &inner,
                                client_attached_session(&inner, client),
                            ));
                            prompt.waiter = self
                                .command_item
                                .as_ref()
                                .and_then(|item| item.lock().raising_overlay.take());
                            prompt_cleared_message = take_client_message(&mut inner, client);
                            let state = prompt.state(prompt_history(&inner, prompt.prompt_type));
                            let fired =
                                (prompt.mode == CommandPromptMode::Incremental).then(|| {
                                    CommandPromptSubmission {
                                        inputs: vec![format!("={}", prompt.input)],
                                        template: prompt.template.clone(),
                                        source: prompt.source.clone(),
                                    }
                                });
                            inner.client_entry(client).command_prompt.replace(prompt);
                            sync_terminal_freeze(&inner, client);
                            direct_events.push(EventPayload::CommandPrompt { state: Some(state) });
                            incremental_start = fired;
                        }
                    }
                    MuxEffect::ChooseTree {
                        pane,
                        preview,
                        kind: tree_kind,
                        info_preview,
                        sessions_only,
                        filter,
                        format,
                        hide_source,
                        kill_source,
                        prompt_accept,
                        sort,
                        key_format,
                        template,
                        zoom,
                    } => {
                        if kind != ClientKind::Interactive
                            || inner.client(client).is_none_or(|c| c.subscriber.is_none())
                        {
                            if command_name == "find-window" {
                                continue;
                            }
                            return Err(ServerError::InvalidCommand(format!(
                                "{command_name} requires an interactive client"
                            ))
                            .into());
                        }
                        if *tree_kind == ChooseTreeKind::Clients
                            && inner.attachment.attached.is_empty()
                        {
                            continue;
                        }
                        let attached_session = client_attached_session(&inner, client);
                        let facts = borrowed_format_hook_facts(&inner);
                        let client_rows = if *tree_kind == ChooseTreeKind::Clients {
                            chooser_presentation::client_chooser_rows(
                                &inner,
                                format.as_deref(),
                                filter.as_deref(),
                            )
                        } else {
                            Vec::new()
                        };
                        let mut chooser = ChooseTreeSession::new(
                            *tree_kind,
                            *info_preview,
                            client_rows,
                            *sessions_only,
                            *pane,
                            &inner.engine,
                            attached_session,
                            &facts,
                            filter.clone(),
                            format.clone(),
                            *hide_source,
                            *kill_source,
                            *prompt_accept,
                            *sort,
                            key_format.clone(),
                        )?;
                        chooser.template = template.clone().or_else(|| {
                            (*tree_kind == ChooseTreeKind::Clients)
                                .then(|| CLIENT_MODE_DEFAULT_COMMAND.to_owned())
                        });
                        chooser.preview_size = *preview;
                        if attached_session != Some(chooser.source_session) {
                            return Err(ServerError::PaneNotAttached(*pane).into());
                        }
                        dismiss_overlays(
                            &mut inner,
                            client,
                            Some(Overlay::ChooseTree),
                            &mut direct_events,
                            &mut retired_command_outputs,
                            &mut resume_client_terminals,
                        );
                        inner
                            .client_mut(client)
                            .and_then(|c| c.swallowed_keys.take());
                        if hold_chooser_zoom(&mut inner, client, *pane, *zoom) {
                            snapshot_changed = true;
                            let facts = borrowed_format_hook_facts(&inner);
                            chooser.rebuild(&inner.engine, attached_session, &facts);
                        }
                        let state = chooser.rendered.clone();
                        inner.client_entry(client).chooser_under =
                            chooser_under(&inner, client, *pane);
                        inner.client_entry(client).choose_tree.replace(chooser);
                        inner.client_entry(client).published_chooser = Default::default();
                        direct_events.push(EventPayload::ChooseTree {
                            state: chooser_shown(&inner, client).then_some(state),
                        });
                        direct_events.push(EventPayload::ChooserPresentation {
                            presentation: chooser_presentation::chooser_presentation(
                                &inner, client,
                            )
                            .map(Box::new),
                        });
                    }
                    MuxEffect::ChooseBuffer {
                        pane,
                        preview,
                        filter,
                        format,
                        kill_source,
                        sort,
                        key_format,
                        template,
                        zoom,
                    } => {
                        if kind != ClientKind::Interactive
                            || inner.client(client).is_none_or(|c| c.subscriber.is_none())
                        {
                            return Err(ServerError::InvalidCommand(
                                "choose-buffer requires an interactive client".to_owned(),
                            )
                            .into());
                        }
                        let attached_session = client_attached_session(&inner, client);
                        let facts = borrowed_format_hook_facts(&inner);
                        let Some(mut chooser) = ChooseBufferSession::new(
                            *pane,
                            &inner.engine,
                            &inner.buffers.paste_buffers,
                            attached_session,
                            &facts,
                            filter.clone(),
                            format.clone(),
                            *kill_source,
                            *sort,
                            key_format.clone(),
                        )?
                        else {
                            continue;
                        };
                        chooser.template.clone_from(template);
                        chooser.preview_size = *preview;
                        if attached_session != Some(chooser.source_session) {
                            return Err(ServerError::PaneNotAttached(*pane).into());
                        }
                        dismiss_overlays(
                            &mut inner,
                            client,
                            Some(Overlay::ChooseBuffer),
                            &mut direct_events,
                            &mut retired_command_outputs,
                            &mut resume_client_terminals,
                        );
                        inner
                            .client_mut(client)
                            .and_then(|c| c.swallowed_keys.take());
                        if hold_chooser_zoom(&mut inner, client, *pane, *zoom) {
                            snapshot_changed = true;
                        }
                        let state = chooser.rendered.clone();
                        inner.client_entry(client).chooser_under =
                            chooser_under(&inner, client, *pane);
                        inner.client_entry(client).choose_buffer.replace(chooser);
                        inner.client_entry(client).published_chooser = Default::default();
                        direct_events.push(EventPayload::ChooseBuffer {
                            state: chooser_shown(&inner, client).then_some(state),
                        });
                        direct_events.push(EventPayload::ChooserPresentation {
                            presentation: chooser_presentation::chooser_presentation(
                                &inner, client,
                            )
                            .map(Box::new),
                        });
                    }
                    MuxEffect::DisplayPanes {
                        pane,
                        delay,
                        selectable,
                        template,
                        source,
                        source_window,
                        source_session,
                        kill_source,
                        zoom,
                    } => {
                        if matches!(
                            inner
                                .modes
                                .pane_modes
                                .get(pane)
                                .and_then(|modes| modes.last()),
                            Some(PaneModeRequest::Panes(_))
                        ) {
                            continue;
                        }
                        let host_window = inner
                            .engine
                            .state
                            .window_for_pane(*pane)
                            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
                        let duration_ms = inner
                            .engine
                            .display_panes_delay(host_window, delay.as_deref())?;
                        let viewers = panes_mode_viewers(&inner, host_window);
                        let native_only =
                            !viewers.is_empty() && viewers.iter().all(|(_, native)| *native);
                        inner.modes.next_display_panes_token =
                            inner.modes.next_display_panes_token.wrapping_add(1).max(1);
                        let token = inner.modes.next_display_panes_token;
                        let mode = PanesMode {
                            source_session: *source_session,
                            source_window: *source_window,
                            duration_ms,
                            ignore_keys: !*selectable,
                            template: template.clone(),
                            source: source.clone(),
                            kill_source: *kill_source,
                            zoom: *zoom && !native_only,
                            token,
                        };
                        if !push_pane_mode(
                            &mut inner,
                            *pane,
                            PaneModeRequest::Panes(Box::new(mode)),
                        ) {
                            continue;
                        }
                        snapshot_changed = true;
                        if duration_ms != 0 {
                            panes_mode_deadline = Some((
                                *pane,
                                token,
                                Instant::now() + Duration::from_millis(u64::from(duration_ms)),
                            ));
                        }
                        panes_mode_overlays.extend(sync_display_panes_overlays(&mut inner));
                    }
                    MuxEffect::DisplayMessage {
                        pane,
                        target_client,
                        text,
                        duration_ms,
                        freeze,
                        ignore_keys,
                    } => {
                        let message_client = match target_client.as_deref() {
                            Some(target) => resolve_display_message_client(&inner, target),
                            None => Some(client),
                        };
                        let Some(message_client) = message_client else {
                            continue;
                        };
                        let duration_ms =
                            duration_ms.unwrap_or_else(|| {
                                inner.engine.display_time_for_client_session(
                                    client_attached_session(&inner, message_client),
                                )
                            });
                        push_server_message(&mut inner, text.clone());
                        let message_id = next_timed_message_id(&mut inner);
                        let publication = OwnedClientMessagePublication {
                            client: message_client,
                            pane: *pane,
                            kind: ClientMessageKind::Info,
                            text: text.clone(),
                            duration_ms,
                            message_id,
                        };
                        if inner.client(message_client).and_then(|c| c.kind)
                            == Some(ClientKind::Interactive)
                        {
                            if duration_ms != 0 {
                                if *ignore_keys {
                                    inner.client_entry(message_client).message_ignore_keys = true;
                                } else {
                                    let _ =
                                        inner.client_mut(message_client).is_some_and(|client| {
                                            std::mem::take(&mut client.message_ignore_keys)
                                        });
                                }
                            }
                            let (previous, schedule) = arm_client_message(
                                &mut inner,
                                message_client,
                                message_id,
                                duration_ms,
                                *freeze,
                            );
                            if let Some(previous) = previous {
                                client_message_retires.push((message_client, previous));
                            }
                            client_message_schedule = schedule;
                            let event = DeferredClientEvent::Owned(publication);
                            if message_client == client {
                                defer_unowned_client_events(
                                    client,
                                    &mut direct_events,
                                    &mut deferred_direct_events,
                                );
                                deferred_direct_events.push(event);
                            } else {
                                targeted_events.push(event);
                            }
                        } else {
                            let event = EventPayload::TimedClientMessage {
                                pane: publication.pane,
                                kind: publication.kind,
                                text: publication.text,
                                duration_ms: publication.duration_ms,
                                message_id: publication.message_id,
                            };
                            if message_client == client {
                                direct_events.push(event);
                            } else {
                                targeted_events.push(DeferredClientEvent::Unowned {
                                    client: message_client,
                                    payload: Box::new(event),
                                });
                            }
                        }
                    }
                    MuxEffect::PrintOrMessage {
                        pane,
                        text,
                        duration_ms,
                        freeze,
                    } => {
                        let message_client = (!context.has_no_client())
                            .then(|| format_provenance_client(context, client))
                            .flatten()
                            .and_then(|client| current_format_client(&inner, client))
                            .filter(|client| {
                                inner.client(*client).and_then(|c| c.kind)
                                    == Some(ClientKind::Interactive)
                            });
                        if let Some(message_client) = message_client {
                            push_server_message(&mut inner, text.clone());
                            let message_id = next_timed_message_id(&mut inner);
                            let publication = OwnedClientMessagePublication {
                                client: message_client,
                                pane: *pane,
                                kind: ClientMessageKind::Info,
                                text: text.clone(),
                                duration_ms: *duration_ms,
                                message_id,
                            };
                            if *duration_ms != 0 {
                                let _ = inner.client_mut(message_client).is_some_and(|client| {
                                    std::mem::take(&mut client.message_ignore_keys)
                                });
                            }
                            let (previous, schedule) = arm_client_message(
                                &mut inner,
                                message_client,
                                message_id,
                                *duration_ms,
                                *freeze,
                            );
                            if let Some(previous) = previous {
                                client_message_retires.push((message_client, previous));
                            }
                            client_message_schedule = schedule;
                            let event = DeferredClientEvent::Owned(publication);
                            if message_client == client {
                                defer_unowned_client_events(
                                    client,
                                    &mut direct_events,
                                    &mut deferred_direct_events,
                                );
                                deferred_direct_events.push(event);
                            } else {
                                targeted_events.push(event);
                            }
                        } else {
                            append_inserted_output(&mut execution.output, text);
                        }
                    }
                    MuxEffect::BufferLimitChanged(limit) => {
                        inner.buffers.automatic_paste_buffer_limit =
                            AutomaticPasteBufferLimit(*limit);
                    }
                    MuxEffect::WordSeparatorsChanged { session } => {
                        for (pane, terminal) in inner.terminals.iter() {
                            let Some(window) = inner.engine.state.window_for_pane(*pane) else {
                                continue;
                            };
                            let pane_session = inner.engine.state.windows[&window].session;
                            if session.is_some_and(|target| target != pane_session) {
                                continue;
                            }
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetWordSeparators {
                                    terminal: Arc::clone(terminal),
                                    separators: WordSeparators::new(
                                        inner.engine.word_separators_for_session(pane_session),
                                    ),
                                },
                            );
                        }
                        for output in inner
                            .clients
                            .values()
                            .filter_map(|c| c.command_output.as_ref())
                        {
                            let Some(window) = inner.engine.state.window_for_pane(output.pane)
                            else {
                                continue;
                            };
                            let output_session = inner.engine.state.windows[&window].session;
                            if session.is_some_and(|target| target != output_session) {
                                continue;
                            }
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetWordSeparators {
                                    terminal: Arc::clone(&output.terminal),
                                    separators: WordSeparators::new(
                                        inner.engine.word_separators_for_session(output_session),
                                    ),
                                },
                            );
                        }
                    }
                    MuxEffect::TerminalKnobsChanged { window, pane } => {
                        for (candidate, terminal) in inner.terminals.iter() {
                            if pane.is_some_and(|pane| pane != *candidate)
                                || window.is_some_and(|window| {
                                    inner.engine.state.window_for_pane(*candidate) != Some(window)
                                })
                            {
                                continue;
                            }
                            let terminal_options = terminal_worker_options(
                                &inner.engine,
                                &inner.config.appearance,
                                &inner.config.config_files,
                                *candidate,
                            )?;
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetAllowPassthrough {
                                    terminal: Arc::clone(terminal),
                                    enabled: terminal_options.allow_passthrough,
                                },
                            );
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetWrapSearch {
                                    terminal: Arc::clone(terminal),
                                    enabled: terminal_options.wrap_search,
                                },
                            );
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetEngineKnobs {
                                    terminal: Arc::clone(terminal),
                                    knobs: terminal_options.knobs,
                                },
                            );
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetAppearance {
                                    terminal: Arc::clone(terminal),
                                    appearance: terminal_options.appearance,
                                },
                            );
                        }
                        for output in inner
                            .clients
                            .values()
                            .filter_map(|c| c.command_output.as_ref())
                        {
                            if pane.is_some_and(|pane| pane != output.pane)
                                || window.is_some_and(|window| {
                                    inner.engine.state.window_for_pane(output.pane) != Some(window)
                                })
                            {
                                continue;
                            }
                            let terminal_options = terminal_worker_options(
                                &inner.engine,
                                &inner.config.appearance,
                                &inner.config.config_files,
                                output.pane,
                            )?;
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetWrapSearch {
                                    terminal: Arc::clone(&output.terminal),
                                    enabled: terminal_options.wrap_search,
                                },
                            );
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetAppearance {
                                    terminal: Arc::clone(&output.terminal),
                                    appearance: terminal_options.appearance,
                                },
                            );
                        }
                    }
                    MuxEffect::ModeKeysChanged { window } => {
                        retarget_copy_mode_tables(&mut inner, *window);
                        let windows = window.map_or_else(
                            || inner.engine.state.windows.keys().copied().collect(),
                            |window| BTreeSet::from([window]),
                        );
                        for pane in panes_for_windows(&inner, &windows) {
                            let Some(terminal) = inner.terminals.get(&pane) else {
                                continue;
                            };
                            let options = terminal_worker_options(
                                &inner.engine,
                                &inner.config.appearance,
                                &inner.config.config_files,
                                pane,
                            )?;
                            deferred_terminal_commands.push(
                                DeferredTerminalCommand::SetEngineKnobs {
                                    terminal: Arc::clone(terminal),
                                    knobs: options.knobs,
                                },
                            );
                        }
                    }
                    MuxEffect::ResizeWindowFromClients { window, mode } => {
                        let (columns, rows) = attached_client_window_extent(&inner, *window, *mode)
                            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
                        let generation = inner.engine.state.generation();
                        inner
                            .engine
                            .set_manual_window_extent(*window, columns, rows)?;
                        snapshot_changed |= inner.engine.state.generation() != generation;
                        let windows = BTreeSet::from([*window]);
                        let panes = panes_for_windows(&inner, &windows);
                        snapshot_changed |= write_back_terminal_geometries(&mut inner, &panes);
                        for (terminal, geometry) in terminal_resizes_for_panes(&inner, &panes) {
                            deferred_terminal_commands
                                .push(DeferredTerminalCommand::Resize { terminal, geometry });
                        }
                    }
                    MuxEffect::AggressiveResizeChanged { window }
                    | MuxEffect::WindowSizeChanged { window } => {
                        let windows = window.map_or_else(
                            || inner.engine.state.windows.keys().copied().collect(),
                            |window| BTreeSet::from([window]),
                        );
                        let panes = panes_for_windows(&inner, &windows);
                        snapshot_changed |= write_back_terminal_geometries(&mut inner, &panes);
                        for (terminal, geometry) in terminal_resizes_for_panes(&inner, &panes) {
                            deferred_terminal_commands
                                .push(DeferredTerminalCommand::Resize { terminal, geometry });
                        }
                    }
                    MuxEffect::MuxOptionChanged { option, session } => {
                        if let Some(session) = session {
                            mux_option_refresh_sessions.insert(*session);
                        } else {
                            let value = inner.engine.mux_option_value(*option);
                            mux_options_changed |=
                                inner
                                    .config
                                    .mux_options
                                    .set(*option, value.clone(), mux_source);
                            if mux_source != MuxOptionSource::Override {
                                inner
                                    .config
                                    .mux_option_underlay
                                    .set(*option, value, mux_source);
                            }
                        }
                        #[cfg(feature = "agent")]
                        {
                            agent_options_changed |= matches!(
                                option,
                                MuxOptionKey::AgentCommand
                                    | MuxOptionKey::AgentClaudeCodeCommand
                                    | MuxOptionKey::AgentAutoApprove
                            );
                        }
                    }
                    MuxEffect::MonitorSilenceChanged => monitor_silence_changed = true,
                    MuxEffect::ClientTerminalChanged => client_terminal_changed = true,
                    MuxEffect::StatusFormatsChanged { session } => match session {
                        Some(session) => {
                            status_refresh_sessions.insert(*session);
                        }
                        None => status_formats_changed = true,
                    },
                    MuxEffect::StatusRowsChanged { session } => {
                        let sessions = session.map_or_else(
                            || inner.engine.state.sessions.keys().copied().collect(),
                            |session| vec![session],
                        );
                        let mut resized = BTreeSet::new();
                        for session in sessions {
                            resized.extend(recalculate_window_extents(&mut inner, session));
                        }
                        if !resized.is_empty() {
                            snapshot_changed = true;
                            let panes = panes_for_windows(&inner, &resized);
                            for (terminal, geometry) in terminal_resizes_for_panes(&inner, &panes) {
                                deferred_terminal_commands
                                    .push(DeferredTerminalCommand::Resize { terminal, geometry });
                            }
                        }
                    }
                    MuxEffect::Attach {
                        session,
                        detach_others,
                        detach_others_hangup,
                        read_only,
                        flags,
                        update_environment,
                    } => {
                        attach = Some((
                            *session,
                            *detach_others,
                            *detach_others_hangup,
                            *read_only,
                            flags.clone(),
                            *update_environment,
                        ));
                    }
                    MuxEffect::SuspendClient { target_client } => {
                        let target =
                            resolve_client_target(&inner, client, kind, target_client.as_deref())?;
                        if inner.attachment.client_flags.contains(client) {
                            return Err(ServerError::InvalidCommand(
                                "client is read-only".to_owned(),
                            )
                            .into());
                        }
                        if inner.client(target).and_then(|c| c.kind) != Some(ClientKind::Control)
                            && inner
                                .client(target)
                                .and_then(|c| c.tty.as_ref())
                                .is_some_and(|tty| !tty.is_empty())
                        {
                            #[cfg(unix)]
                            if let Some(pid) = inner
                                .client(target)
                                .and_then(|c| c.pid.as_ref())
                                .and_then(|pid| rustix::process::Pid::from_raw(*pid as i32))
                            {
                                rustix::process::kill_process(pid, rustix::process::Signal::TSTP)
                                    .map_err(|error| ServerError::InvalidCommand(error.to_string()))?;
                                if let Some(registered) = inner.client_mut(target) {
                                    registered.suspended = true;
                                }
                            }
                        }
                    }
                    MuxEffect::Detach(request) => {
                        let target_client = resolve_client_target(
                            &inner,
                            client,
                            kind,
                            request.target_client.as_deref(),
                        )?;
                        if inner.attachment.client_flags.contains(client)
                            && (!matches!(request.scope, DetachScope::Client)
                                || target_client != client)
                        {
                            return Err(ServerError::InvalidCommand(
                                "client is read-only".to_owned(),
                            )
                            .into());
                        }
                        let scope = match &request.scope {
                            DetachScope::Client => Some(ResolvedDetach::Client(target_client)),
                            DetachScope::Others => Some(ResolvedDetach::Others(target_client)),
                            DetachScope::Session(target) => {
                                match inner.engine.resolve_session(Some(target), context.session) {
                                    Ok(session) => Some(ResolvedDetach::Session(session)),
                                    Err(ServerError::SessionNotFound(_)) => None,
                                    Err(error) => return Err(error.into()),
                                }
                            }
                        };
                        detach = scope.map(|scope| {
                            (
                                scope,
                                DetachExit {
                                    exec: request.exec.clone(),
                                    parent_hangup: request.parent_hangup,
                                },
                            )
                        });
                    }
                    MuxEffect::SourceFile {
                        path,
                        quiet,
                        parse_only,
                        verbose,
                        context,
                        stdin,
                    } => source_files.push(SourceFileRequest {
                        path: path.clone(),
                        quiet: *quiet,
                        parse_only: *parse_only,
                        verbose: *verbose,
                        context: context.clone(),
                        stdin: stdin.clone(),
                    }),
                    MuxEffect::PaneStreamInput { pane, bytes } => {
                        pane_stream_inputs.push((*pane, bytes.clone()));
                    }
                    MuxEffect::RunHook {
                        name,
                        commands,
                        context,
                    } => {
                        inner
                            .engine
                            .note_event_hook_fired(context, name, unix_timestamp());
                        immediate_hooks.push((name.clone(), commands.clone(), context.clone()));
                    }
                    MuxEffect::PaneFormatOutput {
                        pane,
                        format,
                        active_session,
                        format_client,
                    } => {
                        pane_format_output =
                            Some((*pane, format.clone(), *active_session, *format_client));
                    }
                    MuxEffect::ReloadConfig => {
                        reload_config = true;
                    }
                    MuxEffect::ImportTmuxConfig(path) => {
                        import_tmux_config = Some(path.clone());
                    }
                    MuxEffect::KillServer => force_shutdown_requested = true,
                    MuxEffect::SnapshotChanged => snapshot_changed = true,
                    MuxEffect::ModeStylesChanged => mode_styles_changed = true,
                    MuxEffect::UserOptionChanged { channel } => {
                        option_signals.push(channel.clone());
                    }
                    MuxEffect::PaneModeChanged { pane, mode } => {
                        let changed = match mode {
                            Some(mode) => push_pane_mode(&mut inner, *pane, mode.clone()),
                            None => clear_pane_modes(&mut inner, *pane),
                        };
                        if changed {
                            snapshot_changed = true;
                        }
                    }
                }
            }
            #[cfg(feature = "agent")]
            {
                agent_children = record_agent_parents(
                    &mut inner,
                    client,
                    context,
                    command_name,
                    &execution.effects,
                );
            }
            if let Some((pane, format, active_session, format_client)) = pane_format_output {
                if let Some(terminal) = inner.terminals.get(&pane).cloned()
                    && !split_caller_stream
                {
                    execution.output = RawText::default();
                    drop(inner);
                    let wait = terminal_wait
                        .get_or_insert_with(|| terminal_requests::CommandWait::new(self));
                    wait.pane_output(
                        self,
                        pane,
                        terminal,
                        format,
                        active_session,
                        format_client,
                        format_variables.clone(),
                        command_name.to_owned(),
                    );
                    inner = self.inner.lock();
                } else {
                    let target = ExecutionContext::for_pane(&inner.engine.state, pane)
                        .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
                    let facts = borrowed_format_hook_facts(&inner);
                    let mut hooks = DaemonFormatHooks::command_with_optional_variables(
                        &facts,
                        (!format_variables.is_empty()).then_some(&format_variables),
                    )
                    .with_command_item(command_name);
                    let mut output = inner.engine.expand_pane_format(
                        &format,
                        &target,
                        active_session,
                        format_client,
                        &mut hooks,
                    );
                    output.push('\n');
                    execution.output = output.into();
                }
            }
            let recheck_shutdown_requested = self.should_shutdown_if_empty(&inner);
            if snapshot_changed {
                unfocused_copy_mode_exits = unfocused_copy_sessions(&mut inner);
                let clients = inner
                    .clients
                    .iter()
                    .filter_map(|(id, client)| {
                        client.command_output.as_ref().map(|value| (id, value))
                    })
                    .filter_map(|(client, output)| {
                        (client_context_pane(&inner, *client) != Some(output.pane))
                            .then_some(*client)
                    })
                    .collect::<Vec<_>>();
                for client in clients {
                    if let Some(output) = take_command_output(&mut inner, client) {
                        retired_command_outputs.push((client, output));
                    }
                }
            }
            debug_assert!(
                !read_only || inner.engine.state.generation() == generation_before,
                "read-only {command_name} moved the tree generation"
            );
            pending_hook_events.extend(stale_mode_events);
            let prompt_events = hook_events::pane_prompt_hook_events(&mut inner);
            if event_hooks_enabled {
                pending_hook_events.extend(prompt_events);
            }
            let hook_events_before = pending_hook_events.len();
            if event_hooks_enabled && let Some(scope) = hook_scope {
                let pane_focus_before = pane_focus_before.map(|scope| scope.close(&inner));
                let anchor = pending_hook_events.len();
                {
                    let transitions = std::mem::take(&mut inner.modes.pane_mode_transitions);
                    let mut diff = scope.finish(&inner.engine, command_name);
                    hook_events::apply_operation_events(
                        &inner.engine,
                        &execution.effects,
                        &mut diff.events,
                    );
                    pending_hook_events.append(&mut diff.events);
                    pending_hook_events.extend(hook_events::server_mode_hook_events(
                        &inner.engine,
                        transitions,
                    ));
                    for session in diff.closed_sessions(&inner.engine.state) {
                        if let Some(clients) = inner.attachment.attached.get(&session) {
                            for attached_client in clients {
                                let client_name = client_format_name(&inner, *attached_client);
                                pending_hook_events.push(PendingHookEvent::client(
                                    "client-detached",
                                    diff.before_session_context(session),
                                    *attached_client,
                                    Some(client_name.as_str()),
                                ));
                            }
                        }
                    }
                    if let Some(copy_modes_before) = copy_modes_before.as_ref() {
                        let copy_modes_after = active_copy_mode_panes(&inner);
                        pending_hook_events.extend(hook_events::pane_mode_hook_events(
                            &inner.engine,
                            &diff.before,
                            copy_modes_before,
                            &copy_modes_after,
                        ));
                    }
                }
                if let Some(pane_focus_before) = pane_focus_before.as_ref() {
                    let focus = pane_focus_hook_events(&mut inner, pane_focus_before);
                    splice_pane_focus_events(&mut pending_hook_events, anchor, focus);
                }
            }
            inner.modes.pane_mode_transitions.clear();
            debug_assert!(
                !read_only || pending_hook_events.len() == hook_events_before,
                "read-only {command_name} raised hook events"
            );
            client_timers_changed = monitors_before != inner.engine.has_format_monitors()
                || refresh_before.is_some_and(|before| before != copy_mode_refresh_needed(&inner))
                || clock_before.is_some_and(|before| before != clock_mode_timer_needed(&inner));
            (execution, mux_options_changed, recheck_shutdown_requested)
        };

        if let Some(path) = import_tmux_config {
            let output = self.import_tmux_configuration(path.as_deref(), context)?;
            append_inserted_output(&mut execution.output, &output);
            reload_config = true;
        }

        if force_shutdown_requested {
            if let Some((control, _)) = context.control_command_target() {
                if let Some(queue_execution) = queue_execution
                    && queue_execution.detached
                {
                    queue_execution.deferred_control_exit.set(Some(control));
                } else {
                    self.publish_to_client(
                        control,
                        EventPayload::ControlExit {
                            reason: String::new(),
                        },
                    );
                }
            }
            if let Some(queue_execution) = queue_execution
                && !queue_execution.ensure_shutdown_blocker(self)
            {
                return Err(
                    ServerError::InvalidCommand("server is shutting down".to_owned()).into(),
                );
            }
            self.response_admissions.lock().frozen = true;
            if queue_execution.is_some() {
                self.begin_queue_shutdown();
            }
        }

        for (name, commands, context) in immediate_hooks {
            let mut variables = context.format_variables.clone();
            if name.starts_with('@')
                && variables.get("hook_event") == Some(&name)
                && client != ClientId(u64::MAX)
            {
                let client_name = client_format_name(&self.inner.lock(), client);
                variables.insert("hook_client".to_owned(), client_name);
            }
            if variables.get("hook_event") == Some(&name) {
                self.feed_event_waiters(&name, &variables);
            }
            variables.insert("hook".to_owned(), name);
            let hook_output = self.run_hook_commands(
                client,
                kind,
                &context,
                commands,
                &variables,
                queue_execution,
            );
            append_inserted_output(&mut execution.output, &hook_output);
        }

        let deferred_shutdown_request = if force_shutdown_requested {
            DeferredShutdown::Force
        } else if recheck_shutdown_requested {
            DeferredShutdown::Recheck
        } else {
            DeferredShutdown::None
        };
        if deferred_shutdown_request != DeferredShutdown::None {
            if let Some(queue_execution) = queue_execution {
                queue_execution.deferred_shutdown.set(
                    queue_execution
                        .deferred_shutdown
                        .get()
                        .max(deferred_shutdown_request),
                );
            } else if deferred_shutdown_request == DeferredShutdown::Force {
                self.request_shutdown();
            } else {
                self.request_shutdown_without_hooks();
            }
        }

        for pipe in pipes_to_close {
            stop_pane_pipe(pipe);
        }

        if let Some((listing, facts)) = key_listing {
            let wait =
                terminal_wait.get_or_insert_with(|| terminal_requests::CommandWait::new(self));
            cmdq::render_key_listing(self, client, listing, facts, format_variables, wait.start());
        }
        if deferred_terminal_commands.iter().any(|command| {
            matches!(command, DeferredTerminalCommand::ArmCopySource { .. })
                || command.copy_mode_terminal().is_some()
        }) {
            let wait =
                terminal_wait.get_or_insert_with(|| terminal_requests::CommandWait::new(self));
            wait.effects(self, deferred_terminal_commands);
        } else {
            for command in deferred_terminal_commands {
                #[cfg(test)]
                let wrap_search = command.wrap_search();
                command.run();
                #[cfg(test)]
                if let Some(enabled) = wrap_search {
                    self.delivered_wrap_search_commands.lock().push(enabled);
                }
            }
        }
        if respawned_terminals && !snapshot_changed {
            self.refresh_terminal_visibility();
        }
        for (selected, pane, keys, repeat) in pane_mode_keys {
            self.inject_pane_mode_keys(selected, context, pane, &keys, repeat)?;
        }
        for (pane, key, x, y) in pane_mode_pointers {
            self.pane_mode_mouse(client, context, pane, &key, x, y);
        }
        self.reap_pane_mode_kill_panes(client, kind, context);
        for (target, keys, repeat) in injected_client_keys {
            self.inject_client_keys(target, &keys, repeat);
        }
        for (owners, pane, keys, repeat) in mode_table_keys {
            self.inject_mode_table_keys(&owners, pane, &keys, repeat);
        }
        if refresh_armed {
            self.arm_copy_mode_refresh();
        }
        if clock_modes_are_open(&self.inner.lock()) {
            self.arm_pane_mode_clock();
        }
        for (client, terminal) in unfocused_copy_mode_exits {
            terminal.view_action(
                TerminalViewId(client.0),
                zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Cancel),
            );
        }
        for (client, output) in retired_command_outputs {
            self.retire_command_output(client, output);
        }
        for (terminal, previous, next) in relocated_terminal_views {
            for client in previous.difference(&next) {
                terminal.detach_view(TerminalViewId(client.0));
            }
            for client in next.difference(&previous) {
                terminal.attach_view(TerminalViewId(client.0));
            }
        }
        match detach {
            None => {}
            Some((_, exit)) if exit.exec.as_deref() == Some("") => {}
            Some((scope, exit)) => {
                let victims = requested_detach_victims(&self.inner.lock(), scope);
                for (session, victim) in victims {
                    if exit.exec.is_some()
                        && self.read_client(victim, |c| c.and_then(|c| c.kind))
                            == Some(ClientKind::Control)
                    {
                        continue;
                    }
                    let action = self.client_exit_action(session, &exit);
                    self.publish_to_client(
                        victim,
                        EventPayload::detached_requested_with_action(session, None, action),
                    );
                    self.detach_with_event_hooks(victim, event_hooks_enabled);
                }
            }
        }
        if let Some((
            session,
            detach_others,
            detach_others_hangup,
            read_only,
            flags,
            update_environment,
        )) = attach
        {
            let eviction = if detach_others_hangup {
                ClientExitAction::ParentHangup
            } else {
                ClientExitAction::Exit
            };
            self.set_requested_client_flags(client, flags.as_deref(), read_only);
            if invoking_client_terminal == ClientTerminal::Absent {
                return Err(ServerError::InvalidCommand(
                    "open terminal failed: not a terminal".to_owned(),
                )
                .into());
            }
            if update_environment {
                let mut inner = self.inner.lock();
                if let Some(environment) = inner
                    .client(client)
                    .and_then(Client::local_environment)
                    .cloned()
                {
                    inner
                        .engine
                        .update_session_environment_from_client(session, environment.map())?;
                }
            }
            if invoking_client_terminal == ClientTerminal::Present {
                let _held = self.hold_attach_terminals(client);
                let (mut snapshot, attach_hook_events) =
                    self.attach_collect_event_hooks(client, session, event_hooks_enabled)?;
                pending_hook_events.extend(attach_hook_events);
                if detach_others {
                    self.evict_clients_with_event_hooks(
                        Some(session),
                        client,
                        event_hooks_enabled,
                        &eviction,
                    );
                    let inner = self.inner.lock();
                    snapshot = inner.engine.state.snapshot();
                    let presence = snapshot_presence(&inner);
                    stamp_snapshot_for_client(&inner, client, &mut snapshot, &presence);
                }
                let outbound =
                    self.read_client(client, |c| c.and_then(|c| c.subscriber.as_ref()).cloned());
                if let Some(outbound) = outbound {
                    self.send_attached(client, &outbound, session, snapshot);
                }
                self.publish_snapshot();
            } else if detach_others {
                self.evict_clients_with_event_hooks(
                    Some(session),
                    client,
                    event_hooks_enabled,
                    &eviction,
                );
                self.publish_snapshot();
            }
        }
        for (pane, terminal) in terminals_to_watch {
            self.watch_terminal(pane, &terminal)?;
            snapshot_changed |= self.locate_moved_pane(pane);
        }
        for event in client_events {
            let pane = match &event {
                EventPayload::BrowserCommand { pane, .. }
                | EventPayload::TerminalUiCommand { pane, .. } => *pane,
                _ => continue,
            };
            self.publish_for_pane(pane, &event);
        }
        defer_unowned_client_events(client, &mut direct_events, &mut deferred_direct_events);
        for event in deferred_direct_events {
            self.publish_deferred_client_event(event);
        }
        for event in targeted_events {
            self.publish_deferred_client_event(event);
        }
        for (target, retired) in client_message_retires {
            self.retire_client_message(target, retired, true);
        }
        if let Some(retired) = prompt_cleared_message {
            self.retire_client_message(client, retired, true);
        }
        if resume_client_terminals {
            self.resume_client_terminals(client);
        }
        if let Some(deadline) = client_message_schedule {
            let _ = self.timer_tx.send(timers::TimerInput::ClientMessage(
                ClientMessageDeadlineCommand::Schedule(deadline),
            ));
        }
        for (viewer, state) in panes_mode_overlays {
            self.publish_to_client(viewer, EventPayload::DisplayPanes { state });
        }
        if let Some((pane, token, deadline)) = panes_mode_deadline {
            let shared = Arc::downgrade(&self.server_owner());
            let _ = self.timer_tx.send(timers::TimerInput::Callback {
                deadline: Some(deadline),
                callback: Box::new(move || {
                    if let Some(shared) = shared.upgrade() {
                        shared.expire_panes_mode(pane, token);
                    }
                }),
            });
        }
        if monitor_silence_changed {
            self.reset_all_silence_timers();
        }
        for deadline in silence_schedules {
            let _ = self.timer_tx.send(timers::TimerInput::Silence(
                SilenceDeadlineCommand::Schedule(deadline),
            ));
        }
        for (window, pane) in activity_requeues {
            self.raise_window_activity(window, pane);
        }
        #[cfg(feature = "agent")]
        {
            if agent_options_changed {
                self.reconfigure_agents();
            }
            self.close_agent_panes(&removed_panes);
            self.close_agent_panes(&agent_panes_closed);
            for (pane, replaced) in agent_panes_opened {
                self.open_agent_pane(pane, replaced);
                self.request_pane_location(pane);
            }
            for pane in agent_children {
                self.approve_child_tools(pane);
            }
            if matches!(command_name, "new-pane" | "split-window" | "new-window") {
                let created = execution.effects.iter().find_map(|effect| match effect {
                    MuxEffect::PaneCreated {
                        pane,
                        kind: PaneKindSnapshot::Agent(_),
                        ..
                    } => Some(*pane),
                    _ => None,
                });
                self.open_created_chat(client, kind, context, command_name, created, agent_briefs);
            }
            self.close_orphaned_chats(&removed_panes);
        }
        #[cfg(not(feature = "agent"))]
        let _ = (agent_panes_opened, agent_briefs);
        for pane in removed_panes {
            self.cancel_pane_files(pane);
            self.publish(EventPayload::PaneRemoved(pane));
        }
        if client_terminal_changed {
            self.publish_terminal_negotiation();
        }
        if mux_options_event {
            self.publish_effective_mux_options(None);
        } else if !mux_option_refresh_sessions.is_empty() {
            self.publish_effective_mux_options(Some(&mux_option_refresh_sessions));
        }
        if mode_styles_changed {
            self.refresh_published_appearance();
        }
        self.restyle_client_overlays();
        for channel in option_signals {
            if let Some(pane) = channel
                .strip_prefix("@agent_state@")
                .filter(|target| target.starts_with('%'))
                .and_then(|target| target.parse::<PaneId>().ok())
            {
                snapshot_changed |= pane_summary::mirror_agent_state(
                    &mut self.inner.lock(),
                    pane,
                    unix_timestamp(),
                );
                agent_waits::state_changed(self, pane);
                let event = {
                    let inner = self.inner.lock();
                    PendingHookEvent::live_pane("agent-state-changed", pane, &inner.engine).map(
                        |mut event| {
                            let value = inner
                                .engine
                                .format_facts()
                                .user_option(
                                    &pane.to_string(),
                                    &event
                                        .context
                                        .window
                                        .map(|window| window.to_string())
                                        .unwrap_or_default(),
                                    &event
                                        .context
                                        .session
                                        .map(|session| session.to_string())
                                        .unwrap_or_default(),
                                    "@agent_state",
                                )
                                .unwrap_or_default()
                                .to_owned();
                            event.variables.insert("agent_state".to_owned(), value);
                            event
                                .variables
                                .insert("agent_pending_permission".to_owned(), String::new());
                            event
                        },
                    )
                };
                if let Some(event) = event {
                    self.run_event_hooks(vec![event]);
                }
            }
            if let Some(pane) = channel
                .strip_prefix("@name@")
                .and_then(|pane| pane.parse().ok())
            {
                snapshot_changed |= pane_summary::mirror_pane_name(&mut self.inner.lock(), pane);
                #[cfg(all(feature = "agent", unix))]
                self.update_terminal_peer(pane);
            }
            self.signal_wait_channel(&channel);
        }
        if snapshot_changed {
            self.publish_snapshot();
        } else if status_formats_changed {
            self.refresh_status();
        } else if !status_refresh_sessions.is_empty() {
            self.refresh_status_for_sessions(Some(&status_refresh_sessions));
        }
        let replay_client = context.replay_client();
        let source_client = replay_client.unwrap_or(client);
        let cwd_client = replay_client
            .or_else(|| context.control_command_target().map(|(client, _)| client))
            .unwrap_or(client);
        for (pane, bytes) in pane_stream_inputs {
            self.feed_pane_stream_input(pane, bytes.as_bytes());
        }
        if split_caller_stream
            && let Some(pane) = execution.effects.iter().find_map(|effect| match effect {
                MuxEffect::PaneCreated { pane, .. } => Some(*pane),
                _ => None,
            })
        {
            self.release_command_stdout(source_client, &mut execution.output);
            let input = CommandInvocation::new("display-message", ["-I", "-t", &pane.to_string()]);
            if let Some(input) = self.command_with_caller_stdin(client, context, &input)?
                && let Some(bytes) = input.stdin()
            {
                self.feed_pane_stream_input(pane, bytes.as_bytes());
            }
        }
        let (
            source_kind,
            startup_source_client_working_directory,
            source_session_working_directory,
            source_client_working_directory,
            source_client_registered,
            source_client_terminal,
        ) = {
            let inner = self.inner.lock();
            let registered = inner.client(cwd_client).is_some_and(|c| c.kind.is_some());
            let source_kind = if source_client == client {
                kind
            } else {
                inner
                    .client(source_client)
                    .and_then(|c| c.kind)
                    .unwrap_or(kind)
            };
            let source_session_working_directory = (reload_config || !source_files.is_empty())
                .then(|| {
                    client_attached_session(&inner, cwd_client)
                        .and_then(|session| inner.engine.state.session_working_directory(session))
                        .map(Path::to_owned)
                })
                .flatten();
            let source_client_terminal = if context.has_no_client() {
                ClientTerminal::NoClient
            } else if replay_client.is_some() && registered {
                client_terminal(&inner, source_client, source_kind)
            } else {
                invoking_client_terminal
            };
            (
                source_kind,
                inner.config.startup_source_client_working_directory.clone(),
                source_session_working_directory,
                inner
                    .client(cwd_client)
                    .and_then(|c| c.working_directory.as_ref())
                    .cloned(),
                registered,
                source_client_terminal,
            )
        };
        let source_working_directory = (reload_config || !source_files.is_empty()).then(|| {
            startup_source_client_working_directory
                .clone()
                .or(source_session_working_directory)
                .or(source_client_working_directory)
                .or_else(|| context.client_working_directory().map(Path::to_owned))
                .or_else(home_directory)
                .unwrap_or_else(|| PathBuf::from("/"))
        });
        let source_client_base = startup_source_client_working_directory.or_else(|| {
            if cwd_client != ClientId(u64::MAX) && source_client_registered {
                source_working_directory.clone()
            } else {
                None
            }
        });
        let control_target = context.control_command_target().or_else(|| {
            (source_kind == ClientKind::Control)
                .then_some((source_client, CONTROL_COMMAND_FRAME_FLAGS_CONTROL))
        });
        let control_client = control_target.map(|(client, _)| client);
        let mut source_file_error = None;
        let mut source_path_error = false;
        let mut source_path_matched = false;
        let mut control_source_errors = Vec::new();
        let mut control_source_read_errors = Vec::new();
        let mut control_source_matched = false;
        let mut source_verbose_output = RawText::default();
        let mut source_replay_output = RawText::default();
        let mut source_diagnostics_output = RawText::default();
        let mut source_invocations = SourceInvocationAccounting::default();
        let mut pending_source_files = Vec::new();
        let source_invocation = !source_files.is_empty();
        let mut reported_source_failure = false;
        let mut reported_source_callback_failure = false;
        let control_source_invocation = control_target.is_some() && source_invocation;
        let suppress_source_replay_output = source_kind == ClientKind::Command
            && queue_execution.is_some_and(CommandQueueExecution::is_draining);
        let captured_control_source = control_client.is_some_and(|client| {
            !source_files.is_empty() && self.is_capturing_control_command_events(client)
        });
        for request in source_files {
            let path = request.path;
            if path == "-" {
                if let Some(SourceStream::Bytes(stream)) = request.stdin {
                    source_path_matched = true;
                    control_source_matched |= control_target.is_some();
                    pending_source_files.push(PendingConfigFile {
                        path: PathBuf::from("-"),
                        context: request.context.clone(),
                        stdin: Some(stream),
                        options: SourceFileLoadOptions {
                            parse_only: request.parse_only,
                            verbose: request.verbose && control_target.is_none(),
                            suppress_verbose: control_target.is_some(),
                            control_target,
                            replay_client: (source_client != ClientId(u64::MAX))
                                .then_some(source_client),
                            suppress_replay_output: suppress_source_replay_output,
                            command_client: source_kind == ClientKind::Command,
                        },
                    });
                    continue;
                }
                let read_failure = request.stdin.is_some() || control_target.is_some();
                let text = if read_failure {
                    self.take_caller_stdin_error(source_client)
                } else {
                    STANDARD_INPUT_SOURCE_WARNING.to_owned()
                };
                if read_failure {
                    reported_source_failure = true;
                } else {
                    source_path_error = true;
                }
                if read_failure && control_target.is_some() {
                    control_source_read_errors.push(text);
                    self.record_command_failure(control_client.unwrap_or(source_client));
                    continue;
                }
                if captured_control_source {
                    control_source_errors.push(text.clone());
                } else {
                    self.publish_to_client(
                        control_client.unwrap_or(source_client),
                        EventPayload::ClientMessage {
                            pane: request.context.pane,
                            kind: ClientMessageKind::Warning,
                            text: text.clone(),
                        },
                    );
                }
                if control_target.is_none() && source_kind == ClientKind::Command {
                    self.record_command_stderr(source_client, &text);
                    self.record_command_failure(source_client);
                }
                continue;
            }
            let declared_path = PathBuf::from(path.to_os_string());
            let pattern = resolve_source_path(&path, source_working_directory.as_deref());
            let matches = source_glob_matches(&pattern);
            source_path_error |= !matches.errors.is_empty();
            for error in &matches.errors {
                let text = source_glob_error_warning(&declared_path, error);
                if control_target.is_some() {
                    control_source_errors.push(text);
                } else {
                    self.route_source_error(
                        source_client,
                        source_kind,
                        request.context.pane,
                        &text,
                    );
                }
            }
            if matches.paths.is_empty() && matches.errors.is_empty() {
                if !request.quiet {
                    source_path_error = true;
                    let error = missing_source_error(&declared_path);
                    if control_target.is_some() {
                        control_source_errors.push(error);
                    } else {
                        self.route_source_error(
                            source_client,
                            source_kind,
                            request.context.pane,
                            &error,
                        );
                    }
                }
                continue;
            }
            source_path_matched |= !matches.paths.is_empty();
            control_source_matched |= control_target.is_some() && !matches.paths.is_empty();
            for path in matches.paths {
                pending_source_files.push(PendingConfigFile {
                    path,
                    context: request.context.clone(),
                    stdin: None,
                    options: SourceFileLoadOptions {
                        parse_only: request.parse_only,
                        verbose: request.verbose && control_target.is_none(),
                        suppress_verbose: control_target.is_some(),
                        control_target,
                        replay_client: (source_client != ClientId(u64::MAX))
                            .then_some(source_client),
                        suppress_replay_output: suppress_source_replay_output,
                        command_client: source_kind == ClientKind::Command,
                    },
                });
            }
        }
        if captured_control_source {
            let source_command_error = !control_source_errors.is_empty();
            let frame_flags = control_target
                .map(|(_, flags)| flags)
                .expect("captured Control source target");
            if source_command_error && frame_flags == CONTROL_COMMAND_FRAME_FLAGS_CONTROL {
                self.inner
                    .lock()
                    .client_entry(control_client.expect("captured Control source client"))
                    .command_streams
                    .get_or_insert_default()
                    .exit_code = 1;
            }
            self.publish_control_command_guard(
                control_target,
                control_source_errors.join("\n").into(),
                source_command_error && !control_source_matched,
                source_command_error,
            );
            control_source_errors.clear();
        }
        for output in control_source_read_errors {
            self.publish_control_source_read_error(
                control_target.expect("source read errors have a control target"),
                context.pane,
                ErrorKind::Other,
                output,
            );
        }
        if source_invocation && queue_execution.is_some_and(CommandQueueExecution::is_draining) {
            self.enter_queue_shutdown_phase();
        }
        if source_invocation
            && let Some(queue_execution) = queue_execution
            && queue_execution.wait_yields
            && queue_execution.is_draining()
        {
            queue_execution.yield_queue();
            pending_source_files.clear();
        }
        if source_invocation
            && queue_execution.is_some()
            && self
                .command_item
                .as_ref()
                .is_some_and(|item| item.lock().loop_wait)
        {
            return source_queue::SourceExecution {
                client,
                kind,
                execution,
                source_client,
                source_kind,
                source_client_terminal,
                source_client_base,
                control_target,
                control_client,
                source_file_error,
                source_path_error,
                source_path_matched,
                control_source_errors,
                control_source_matched,
                source_verbose_output,
                source_replay_output,
                source_diagnostics_output,
                source_invocations,
                source_invocation,
                reported_source_failure,
                reported_source_callback_failure,
                control_source_invocation,
                suppress_source_replay_output,
                reload_config,
                read_only,
                client_timers_changed,
                status_formats_changed,
                status_refresh_sessions,
                pending_hook_events,
                notifications_only,
                incremental_start,
                terminal_wait,
                pending: pending_source_files.into(),
                parsed: VecDeque::new(),
                read: None,
                replay: None,
                replay_pending: None,
                deferred_control_config_warnings: Vec::new(),
            }
            .run(self, context, queue_execution);
        }
        let mut parsed_source_files = Vec::new();
        let mut deferred_control_config_warnings = Vec::new();
        for pending in pending_source_files {
            let mut report = if control_target.is_some() || source_client == ClientId(u64::MAX) {
                ConfigLoadReport::default()
            } else {
                ConfigLoadReport::with_stdout_transcript()
            };
            let parsed = if let Some(stream) = &pending.stdin {
                self.parse_config_input(
                    &pending.path,
                    ConfigInput::sourced(stream.as_bytes()),
                    &mut report,
                    pending.options,
                    true,
                )
                .map(Some)
                .ok_or_else(|| DaemonError::from(non_utf8_config_error()))
            } else {
                self.parse_config_file(&pending.path, &mut report, pending.options, true)
            };
            match parsed {
                Ok(parsed) => parsed_source_files.push((pending, parsed, report)),
                Err(DaemonError::Io(error)) => {
                    if !pending
                        .context
                        .format_variables
                        .contains_key(HOOK_CONTEXT_FORMAT)
                    {
                        reported_source_failure = true;
                    }
                    let warning = if control_target.is_some() {
                        source_read_error_warning(&pending.path, &error)
                    } else {
                        client_source_read_error_warning(
                            source_kind == ClientKind::Command,
                            &pending.path,
                            &error,
                        )
                    };
                    if let Some(target) = control_target {
                        self.publish_control_source_read_error(
                            target,
                            pending.context.pane,
                            error.kind(),
                            warning,
                        );
                    } else {
                        self.route_source_error(
                            source_client,
                            source_kind,
                            pending.context.pane,
                            &warning,
                        );
                    }
                }
                Err(error) => {
                    if source_file_error.is_none() {
                        source_file_error = Some(error);
                    }
                }
            }
        }
        for (pending, parsed, mut report) in parsed_source_files {
            if let Some((client, _)) = control_target {
                deferred_control_config_warnings.extend(report.diagnostics().iter().map(
                    |diagnostic| DeferredControlConfigWarning {
                        client,
                        text: diagnostic.clone(),
                    },
                ));
            }
            let PendingConfigFile {
                path,
                mut context,
                options,
                stdin: _,
            } = pending;
            let defer_command_error_hook_replay_issues = control_target.is_none()
                && queue_execution.is_some()
                && context.replay_client().is_some()
                && context
                    .format_variables
                    .get(HOOK_CONTEXT_FORMAT)
                    .is_some_and(|hook| hook == "command-error");
            let replayed = parsed.map_or(Ok(()), |parsed| {
                self.replay_config_file_with_parent_queue(
                    &path,
                    parsed,
                    &mut context,
                    0,
                    &mut report,
                    source_client_terminal,
                    source_client_base.as_deref(),
                    &mut source_invocations,
                    options,
                    &mut deferred_control_config_warnings,
                    queue_execution,
                )
            });
            match replayed {
                Ok(()) => {}
                Err(DaemonError::Io(error)) => {
                    if !context.format_variables.contains_key(HOOK_CONTEXT_FORMAT) {
                        reported_source_failure = true;
                    }
                    let warning = if control_target.is_some() {
                        source_read_error_warning(&path, &error)
                    } else {
                        client_source_read_error_warning(
                            source_kind == ClientKind::Command,
                            &path,
                            &error,
                        )
                    };
                    if !options.suppress_replay_output {
                        if let Some(target) = control_target {
                            self.publish_control_source_read_error(
                                target,
                                context.pane,
                                error.kind(),
                                warning,
                            );
                        } else {
                            self.route_source_error(
                                source_client,
                                source_kind,
                                context.pane,
                                &warning,
                            );
                        }
                    }
                }
                Err(error) => {
                    if source_file_error.is_none() {
                        source_file_error = Some(error);
                    }
                }
            }
            let hook_owned_replay = context.format_variables.contains_key(HOOK_CONTEXT_FORMAT);
            reported_source_callback_failure |= report.source_callback_failure;
            if report.reported_failure || !hook_owned_replay && !report.replay_issues().is_empty() {
                reported_source_failure = true;
                if let Some((client, _)) = control_target
                    && let Some(streams) = self
                        .inner
                        .lock()
                        .client_mut(client)
                        .and_then(|c| c.command_streams.as_mut())
                {
                    streams.exit_code = 1;
                }
            }
            if !options.parse_only {
                self.apply_stored_mux_config_overrides("source-file-replay");
            }
            if !options.suppress_replay_output {
                if let Some((verbose, replay)) = report.stdout_transcript() {
                    if control_target.is_none() && source_kind == ClientKind::Command {
                        let claim = if report.stdout_raw_claimed() {
                            Some(StdoutClaim::Raw)
                        } else if verbose.is_empty() && replay.is_empty() {
                            None
                        } else {
                            Some(StdoutClaim::Print)
                        };
                        if let Some(claim) = claim {
                            self.record_command_stdout_claim(source_client, claim);
                        }
                    }
                    append_inserted_output(&mut source_verbose_output, verbose);
                    append_inserted_output(&mut source_replay_output, replay);
                }
                if let Some(diagnostics) = report.stdout_diagnostics() {
                    append_inserted_output(&mut source_diagnostics_output, diagnostics);
                }
                if control_target.is_none() && source_kind == ClientKind::Command {
                    for (index, diagnostic) in report.diagnostics().iter().enumerate() {
                        if !report.replayed_diagnostics.contains(&index) {
                            self.record_command_stdout(source_client, diagnostic);
                        }
                        self.record_command_failure(source_client);
                    }
                }
                if defer_command_error_hook_replay_issues {
                    queue_execution
                        .expect("deferred replay issue queue")
                        .deferred_config_replay_issues
                        .borrow_mut()
                        .extend(
                            report
                                .take_undelivered_replay_issues()
                                .into_iter()
                                .map(DeferredConfigReplayIssue::Replay),
                        );
                } else {
                    if let Some((client, flags)) = control_target {
                        if flags == CONTROL_COMMAND_FRAME_FLAGS_CONTROL {
                            self.route_config_replay_errors(
                                client,
                                ClientKind::Control,
                                context.pane,
                                &mut report,
                            );
                        } else {
                            report.delivered_replay_issues = report.replay_issues().len();
                        }
                    } else {
                        self.route_config_replay_errors(
                            source_client,
                            source_kind,
                            context.pane,
                            &mut report,
                        );
                    }
                    if control_target.is_none()
                        && source_kind == ClientKind::Command
                        && let Some(skipped) = report.skipped_summary()
                    {
                        self.record_command_stderr(source_client, &skipped);
                    }
                    let summary = if control_target.is_some() && report.control_guarded {
                        report.skipped_summary()
                    } else {
                        report.message()
                    };
                    if let Some(summary) = summary {
                        self.publish_to_client(
                            control_client.unwrap_or(source_client),
                            EventPayload::ClientMessage {
                                pane: context.pane,
                                kind: ClientMessageKind::Warning,
                                text: summary,
                            },
                        );
                    }
                }
            }
        }
        self.publish_deferred_control_config_warnings(deferred_control_config_warnings);
        if control_source_invocation {
            self.publish_control_source_complete(control_target);
        }
        append_inserted_output(&mut execution.output, &source_verbose_output);
        append_inserted_output(&mut execution.output, &source_replay_output);
        append_inserted_output(&mut execution.output, &source_diagnostics_output);
        if !control_source_errors.is_empty() {
            let mut inner = self.inner.lock();
            let target = control_target.expect("Control source errors have a target");
            let streams = inner
                .client_entry(target.0)
                .command_streams
                .get_or_insert_default();
            if control_source_matched {
                for error in control_source_errors {
                    streams.stdout.push_str(&error);
                    streams.stdout.push('\n');
                }
                streams.exit_code = 1;
            } else {
                streams.control_error = control_source_errors.join("\n");
            }
        }
        if reload_config
            && let Err(error) = self.reload_user_config_with_source_base(
                source_client,
                context,
                source_client_base.as_deref(),
                SourceFileLoadOptions::default(),
            )
            && source_file_error.is_none()
        {
            source_file_error = Some(error);
        }
        if !read_only {
            self.publish_key_tables_if_changed();
        }
        if client_timers_changed || status_formats_changed || !status_refresh_sessions.is_empty() {
            self.nudge_client_timers();
        }
        pending_hook_events.extend(std::mem::take(
            &mut self.inner.lock().waits.deferred_event_hooks,
        ));
        if !pending_hook_events.is_empty() {
            self.wake_control_queue(client, kind);
        }
        if notifications_only {
            self.run_event_hooks(pending_hook_events);
        } else if let Some(queue_execution) = queue_execution {
            self.publish_event_control_notifications(&mut pending_hook_events);
            queue_execution
                .pending_event_hooks
                .borrow_mut()
                .extend(pending_hook_events);
        } else {
            self.run_event_hooks(pending_hook_events);
        }
        if let Some(submission) = incremental_start {
            self.submit_command_prompt(source_client, source_kind, context, &submission);
        }
        if suppress_source_replay_output
            && source_path_matched
            && let Some(queue_execution) = queue_execution
        {
            queue_execution.suppress_output.set(true);
        }
        let report_sets_request_status = reported_source_failure
            && (source_kind == ClientKind::Command
                || control_target.is_some_and(|(_, flags)| {
                    flags == CONTROL_COMMAND_FRAME_FLAGS_NONE
                        || flags == CONTROL_COMMAND_FRAME_FLAGS_CONTROL
                            && reported_source_callback_failure
                }));
        if report_sets_request_status && let Some(queue_execution) = queue_execution {
            queue_execution.note_reported_failure();
        }
        match source_file_error {
            Some(error) => Err(prepend_command_output(execution.output, error)),
            None if source_invocation
                && report_sets_request_status
                && queue_execution.is_none()
                && matches!(source_kind, ClientKind::Command | ClientKind::Control) =>
            {
                Err(DaemonError::ReportedCommandExit {
                    output: execution.output,
                    exit_code: 1,
                })
            }
            None if source_path_error && !source_path_matched => Err(DaemonError::CommandExit {
                output: execution.output,
                exit_code: 1,
            }),
            None => match terminal_wait {
                Some(wait) => wait.finish(self, execution),
                None => Ok(execution),
            },
        }
    }
}
