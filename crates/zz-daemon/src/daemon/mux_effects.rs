use super::*;

pub(super) struct CommandAftermath {
    pub(super) terminals_to_watch: Vec<(PaneId, Arc<TerminalSession>)>,
    pub(super) client_events: Vec<EventPayload>,
    pub(super) direct_events: Vec<EventPayload>,
    pub(super) deferred_direct_events: Vec<DeferredClientEvent>,
    pub(super) targeted_events: Vec<DeferredClientEvent>,
    pub(super) source_files: Vec<SourceFileRequest>,
    pub(super) pane_stream_inputs: Vec<(PaneId, RawText)>,
    pub(super) removed_panes: Vec<PaneId>,
    pub(super) agent_panes_opened: Vec<(PaneId, Option<Arc<TerminalSession>>)>,
    pub(super) agent_briefs: Vec<(PaneId, String)>,
    #[cfg(feature = "agent")]
    pub(super) agent_panes_closed: Vec<PaneId>,
    pub(super) relocated_terminal_views:
        Vec<(Arc<TerminalSession>, BTreeSet<ClientId>, BTreeSet<ClientId>)>,
    pub(super) retired_command_outputs: Vec<(ClientId, RetiredCommandOutput)>,
    pub(super) deferred_terminal_commands: Vec<DeferredTerminalCommand>,
    pub(super) terminal_wait: Option<terminal_requests::CommandWait>,
    pub(super) key_listing: Option<(zz_mux::KeyListing, FormatHookFacts)>,
    pub(super) injected_client_keys: Vec<(ClientId, Vec<zz_protocol::KeyToken>, u32)>,
    pub(super) mode_table_keys: Vec<(Vec<ClientId>, PaneId, Vec<zz_protocol::KeyToken>, u32)>,
    pub(super) pane_mode_keys: Vec<(ClientId, PaneId, Vec<zz_protocol::KeyToken>, u32)>,
    pub(super) pane_mode_pointers: Vec<(PaneId, String, usize, usize)>,
    pub(super) refresh_armed: bool,
    pub(super) unfocused_copy_mode_exits: Vec<(ClientId, Arc<TerminalSession>)>,
    pub(super) pipes_to_close: Vec<PanePipe>,
    pub(super) panes_mode_deadline: Option<(PaneId, u64, Instant)>,
    pub(super) panes_mode_overlays: Vec<(ClientId, Option<DisplayPanesState>)>,
    pub(super) client_message_retires: Vec<(ClientId, ActiveClientMessage)>,
    pub(super) client_message_schedule: Option<ClientMessageDeadline>,
    pub(super) resume_client_terminals: bool,
    pub(super) incremental_start: Option<CommandPromptSubmission>,
    pub(super) prompt_cleared_message: Option<ActiveClientMessage>,
    pub(super) activity_requeues: Vec<(WindowId, PaneId)>,
    pub(super) silence_schedules: Vec<SilenceDeadline>,
    pub(super) monitor_silence_changed: bool,
    pub(super) client_terminal_changed: bool,
    pub(super) attach: Option<(SessionId, bool, bool, bool, Option<String>, bool)>,
    pub(super) detach: Option<(ResolvedDetach, DetachExit)>,
    #[expect(clippy::option_option, reason = "the effect carries an optional path")]
    pub(super) import_tmux_config: Option<Option<String>>,
    pub(super) reload_config: bool,
    pub(super) snapshot_changed: bool,
    pub(super) respawned_terminals: bool,
    pub(super) mux_options_changed: bool,
    pub(super) mux_option_refresh_sessions: BTreeSet<SessionId>,
    #[cfg(feature = "agent")]
    pub(super) agent_options_changed: bool,
    pub(super) status_formats_changed: bool,
    pub(super) status_refresh_sessions: BTreeSet<SessionId>,
    pub(super) mode_styles_changed: bool,
    pub(super) force_shutdown_requested: bool,
    pub(super) immediate_hooks: Vec<(String, Vec<Vec<CommandInvocation>>, ExecutionContext)>,
    pub(super) option_signals: Vec<String>,
    pub(super) pending_hook_events: Vec<PendingHookEvent>,
}

impl CommandAftermath {
    fn new() -> Self {
        Self {
            terminals_to_watch: Vec::new(),
            client_events: Vec::new(),
            direct_events: Vec::new(),
            deferred_direct_events: Vec::new(),
            targeted_events: Vec::new(),
            source_files: Vec::new(),
            pane_stream_inputs: Vec::new(),
            removed_panes: Vec::new(),
            agent_panes_opened: Vec::new(),
            agent_briefs: Vec::new(),
            #[cfg(feature = "agent")]
            agent_panes_closed: Vec::new(),
            relocated_terminal_views: Vec::new(),
            retired_command_outputs: Vec::new(),
            deferred_terminal_commands: Vec::new(),
            terminal_wait: None,
            key_listing: None,
            injected_client_keys: Vec::new(),
            mode_table_keys: Vec::new(),
            pane_mode_keys: Vec::new(),
            pane_mode_pointers: Vec::new(),
            refresh_armed: false,
            unfocused_copy_mode_exits: Vec::new(),
            pipes_to_close: Vec::new(),
            panes_mode_deadline: None,
            panes_mode_overlays: Vec::new(),
            client_message_retires: Vec::new(),
            client_message_schedule: None,
            resume_client_terminals: false,
            incremental_start: None,
            prompt_cleared_message: None,
            activity_requeues: Vec::new(),
            silence_schedules: Vec::new(),
            monitor_silence_changed: false,
            client_terminal_changed: false,
            attach: None,
            detach: None,
            import_tmux_config: None,
            reload_config: false,
            snapshot_changed: false,
            respawned_terminals: false,
            mux_options_changed: false,
            mux_option_refresh_sessions: BTreeSet::new(),
            #[cfg(feature = "agent")]
            agent_options_changed: false,
            status_formats_changed: false,
            status_refresh_sessions: BTreeSet::new(),
            mode_styles_changed: false,
            force_shutdown_requested: false,
            immediate_hooks: Vec::new(),
            option_signals: Vec::new(),
            pending_hook_events: Vec::new(),
        }
    }
}
#[derive(Clone, Copy)]
pub(super) struct EffectContext<'a> {
    pub(super) client: ClientId,
    pub(super) kind: ClientKind,
    pub(super) context: &'a ExecutionContext,
    pub(super) command: &'a CommandInvocation,
    pub(super) command_name: &'a str,
    pub(super) mux_source: MuxOptionSource,
}

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
        let mut after = CommandAftermath::new();
        #[cfg(feature = "agent")]
        let agent_children;
        let client_timers_changed;
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
                after.key_listing = Some((
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
                    after.snapshot_changed = true;
                }
            }
            for pane in belled_panes_before {
                if inner.engine.state.pane(pane).is_none_or(|pane| !pane.bell) {
                    after.snapshot_changed = true;
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
                    after.silence_schedules.push(deadline);
                }
                if inner.engine.monitor_activity_for_window(*window)
                    && let Some(state) = inner.engine.state.windows.get(window)
                {
                    after.activity_requeues.push((*window, state.active_pane));
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
            after.snapshot_changed |= write_back_terminal_geometries(&mut inner, &resized_panes);
            let mut pane_format_output = None;
            let cx = EffectContext {
                client,
                kind,
                context: &*context,
                command,
                command_name,
                mux_source,
            };
            for effect in &execution.effects {
                match effect {
                    MuxEffect::PaneCreated { .. }
                    | MuxEffect::PaneMaterialized { .. }
                    | MuxEffect::PaneRespawned { .. }
                    | MuxEffect::AgentForked { .. }
                    | MuxEffect::AgentBrief { .. }
                    | MuxEffect::PaneWaitForExit { .. }
                    | MuxEffect::PanesRemoved(_)
                    | MuxEffect::PaneRelocated { .. } => {
                        self.apply_pane_effect(&mut inner, cx, &mut after, &mut hooks, effect)?;
                    }
                    MuxEffect::SuppressAfterHook
                    | MuxEffect::PaneMovedInWindow { .. }
                    | MuxEffect::ZoomCycled { .. } => {}
                    MuxEffect::ArmMouseDrag(_)
                    | MuxEffect::ArmCopyModeSource { .. }
                    | MuxEffect::ArmCopyModeKill { .. }
                    | MuxEffect::CopyModeRepeat { .. }
                    | MuxEffect::TerminalView { .. }
                    | MuxEffect::TerminalUi { .. } => {
                        mux_effects_views::apply_view_effect(&mut inner, cx, &mut after, effect)?;
                    }
                    MuxEffect::SendKeys { .. }
                    | MuxEffect::SendClientKeys { .. }
                    | MuxEffect::ResetPane { .. } => {
                        mux_effects_keys::apply_key_effect(&mut inner, cx, &mut after, effect)?;
                    }
                    MuxEffect::FocusSidebar { .. }
                    | MuxEffect::ChoosePath { .. }
                    | MuxEffect::CommandPrompt { .. }
                    | MuxEffect::ChooseTree { .. }
                    | MuxEffect::ChooseBuffer { .. }
                    | MuxEffect::DisplayPanes { .. } => {
                        self.apply_overlay_effect(&mut inner, cx, &mut after, effect)?;
                    }
                    MuxEffect::DisplayMessage { .. } | MuxEffect::PrintOrMessage { .. } => {
                        mux_effects_messages::apply_message_effect(
                            &mut inner,
                            cx,
                            &mut after,
                            &mut execution.output,
                            effect,
                        );
                    }
                    MuxEffect::BufferLimitChanged(_)
                    | MuxEffect::WordSeparatorsChanged { .. }
                    | MuxEffect::TerminalKnobsChanged { .. }
                    | MuxEffect::ModeKeysChanged { .. }
                    | MuxEffect::ResizeWindowFromClients { .. }
                    | MuxEffect::AggressiveResizeChanged { .. }
                    | MuxEffect::WindowSizeChanged { .. }
                    | MuxEffect::MuxOptionChanged { .. }
                    | MuxEffect::MonitorSilenceChanged
                    | MuxEffect::ClientTerminalChanged
                    | MuxEffect::StatusFormatsChanged { .. }
                    | MuxEffect::StatusRowsChanged { .. } => {
                        mux_effects_options::apply_option_effect(
                            &mut inner, cx, &mut after, effect,
                        )?;
                    }
                    MuxEffect::Attach { .. }
                    | MuxEffect::SuspendClient { .. }
                    | MuxEffect::Detach(_) => mux_effects_clients::apply_client_effect(
                        &mut inner, cx, &mut after, effect,
                    )?,
                    MuxEffect::SourceFile { .. }
                    | MuxEffect::PaneStreamInput { .. }
                    | MuxEffect::RunHook { .. }
                    | MuxEffect::PaneFormatOutput { .. }
                    | MuxEffect::ReloadConfig
                    | MuxEffect::ImportTmuxConfig(_)
                    | MuxEffect::KillServer
                    | MuxEffect::SnapshotChanged
                    | MuxEffect::ModeStylesChanged
                    | MuxEffect::UserOptionChanged { .. }
                    | MuxEffect::PaneModeChanged { .. } => mux_effects_flags::apply_flag_effect(
                        &mut inner,
                        &mut after,
                        &mut pane_format_output,
                        effect,
                    ),
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
                    let wait = after
                        .terminal_wait
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
            if after.snapshot_changed {
                after.unfocused_copy_mode_exits = unfocused_copy_sessions(&mut inner);
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
                        after.retired_command_outputs.push((client, output));
                    }
                }
            }
            debug_assert!(
                !read_only || inner.engine.state.generation() == generation_before,
                "read-only {command_name} moved the tree generation"
            );
            after.pending_hook_events.extend(stale_mode_events);
            let prompt_events = hook_events::pane_prompt_hook_events(&mut inner);
            if event_hooks_enabled {
                after.pending_hook_events.extend(prompt_events);
            }
            let hook_events_before = after.pending_hook_events.len();
            if event_hooks_enabled && let Some(scope) = hook_scope {
                let pane_focus_before = pane_focus_before.map(|scope| scope.close(&inner));
                let anchor = after.pending_hook_events.len();
                {
                    let transitions = std::mem::take(&mut inner.modes.pane_mode_transitions);
                    let mut diff = scope.finish(&inner.engine, command_name);
                    hook_events::apply_operation_events(
                        &inner.engine,
                        &execution.effects,
                        &mut diff.events,
                    );
                    after.pending_hook_events.append(&mut diff.events);
                    after
                        .pending_hook_events
                        .extend(hook_events::server_mode_hook_events(
                            &inner.engine,
                            transitions,
                        ));
                    for session in diff.closed_sessions(&inner.engine.state) {
                        if let Some(clients) = inner.attachment.attached.get(&session) {
                            for attached_client in clients {
                                let client_name = client_format_name(&inner, *attached_client);
                                after.pending_hook_events.push(PendingHookEvent::client(
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
                        after
                            .pending_hook_events
                            .extend(hook_events::pane_mode_hook_events(
                                &inner.engine,
                                &diff.before,
                                copy_modes_before,
                                &copy_modes_after,
                            ));
                    }
                }
                if let Some(pane_focus_before) = pane_focus_before.as_ref() {
                    let focus = pane_focus_hook_events(&mut inner, pane_focus_before);
                    splice_pane_focus_events(&mut after.pending_hook_events, anchor, focus);
                }
            }
            inner.modes.pane_mode_transitions.clear();
            debug_assert!(
                !read_only || after.pending_hook_events.len() == hook_events_before,
                "read-only {command_name} raised hook events"
            );
            client_timers_changed = monitors_before != inner.engine.has_format_monitors()
                || refresh_before.is_some_and(|before| before != copy_mode_refresh_needed(&inner))
                || clock_before.is_some_and(|before| before != clock_mode_timer_needed(&inner));
            (
                execution,
                after.mux_options_changed,
                recheck_shutdown_requested,
            )
        };

        if let Some(path) = after.import_tmux_config {
            let output = self.import_tmux_configuration(path.as_deref(), context)?;
            append_inserted_output(&mut execution.output, &output);
            after.reload_config = true;
        }

        if after.force_shutdown_requested {
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

        for (name, commands, context) in after.immediate_hooks {
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

        let deferred_shutdown_request = if after.force_shutdown_requested {
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

        for pipe in after.pipes_to_close {
            stop_pane_pipe(pipe);
        }

        if let Some((listing, facts)) = after.key_listing {
            let wait = after
                .terminal_wait
                .get_or_insert_with(|| terminal_requests::CommandWait::new(self));
            cmdq::render_key_listing(self, client, listing, facts, format_variables, wait.start());
        }
        if after.deferred_terminal_commands.iter().any(|command| {
            matches!(command, DeferredTerminalCommand::ArmCopySource { .. })
                || command.copy_mode_terminal().is_some()
        }) {
            let wait = after
                .terminal_wait
                .get_or_insert_with(|| terminal_requests::CommandWait::new(self));
            wait.effects(self, after.deferred_terminal_commands);
        } else {
            for command in after.deferred_terminal_commands {
                #[cfg(test)]
                let wrap_search = command.wrap_search();
                command.run();
                #[cfg(test)]
                if let Some(enabled) = wrap_search {
                    self.delivered_wrap_search_commands.lock().push(enabled);
                }
            }
        }
        if after.respawned_terminals && !after.snapshot_changed {
            self.refresh_terminal_visibility();
        }
        for (selected, pane, keys, repeat) in after.pane_mode_keys {
            self.inject_pane_mode_keys(selected, context, pane, &keys, repeat)?;
        }
        for (pane, key, x, y) in after.pane_mode_pointers {
            self.pane_mode_mouse(client, context, pane, &key, x, y);
        }
        self.reap_pane_mode_kill_panes(client, kind, context);
        for (target, keys, repeat) in after.injected_client_keys {
            self.inject_client_keys(target, &keys, repeat);
        }
        for (owners, pane, keys, repeat) in after.mode_table_keys {
            self.inject_mode_table_keys(&owners, pane, &keys, repeat);
        }
        if after.refresh_armed {
            self.arm_copy_mode_refresh();
        }
        if clock_modes_are_open(&self.inner.lock()) {
            self.arm_pane_mode_clock();
        }
        for (client, terminal) in after.unfocused_copy_mode_exits {
            terminal.view_action(
                TerminalViewId(client.0),
                zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Cancel),
            );
        }
        for (client, output) in after.retired_command_outputs {
            self.retire_command_output(client, output);
        }
        for (terminal, previous, next) in after.relocated_terminal_views {
            for client in previous.difference(&next) {
                terminal.detach_view(TerminalViewId(client.0));
            }
            for client in next.difference(&previous) {
                terminal.attach_view(TerminalViewId(client.0));
            }
        }
        match after.detach {
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
        )) = after.attach
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
                after.pending_hook_events.extend(attach_hook_events);
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
        for (pane, terminal) in after.terminals_to_watch {
            self.watch_terminal(pane, &terminal)?;
            after.snapshot_changed |= self.locate_moved_pane(pane);
        }
        for event in after.client_events {
            let pane = match &event {
                EventPayload::BrowserCommand { pane, .. }
                | EventPayload::TerminalUiCommand { pane, .. } => *pane,
                _ => continue,
            };
            self.publish_for_pane(pane, &event);
        }
        defer_unowned_client_events(
            client,
            &mut after.direct_events,
            &mut after.deferred_direct_events,
        );
        for event in after.deferred_direct_events {
            self.publish_deferred_client_event(event);
        }
        for event in after.targeted_events {
            self.publish_deferred_client_event(event);
        }
        for (target, retired) in after.client_message_retires {
            self.retire_client_message(target, retired, true);
        }
        if let Some(retired) = after.prompt_cleared_message {
            self.retire_client_message(client, retired, true);
        }
        if after.resume_client_terminals {
            self.resume_client_terminals(client);
        }
        if let Some(deadline) = after.client_message_schedule {
            let _ = self.timer_tx.send(timers::TimerInput::ClientMessage(
                ClientMessageDeadlineCommand::Schedule(deadline),
            ));
        }
        for (viewer, state) in after.panes_mode_overlays {
            self.publish_to_client(viewer, EventPayload::DisplayPanes { state });
        }
        if let Some((pane, token, deadline)) = after.panes_mode_deadline {
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
        if after.monitor_silence_changed {
            self.reset_all_silence_timers();
        }
        for deadline in after.silence_schedules {
            let _ = self.timer_tx.send(timers::TimerInput::Silence(
                SilenceDeadlineCommand::Schedule(deadline),
            ));
        }
        for (window, pane) in after.activity_requeues {
            self.raise_window_activity(window, pane);
        }
        #[cfg(feature = "agent")]
        {
            if after.agent_options_changed {
                self.reconfigure_agents();
            }
            self.close_agent_panes(&after.removed_panes);
            self.close_agent_panes(&after.agent_panes_closed);
            for (pane, replaced) in after.agent_panes_opened {
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
                self.open_created_chat(
                    client,
                    kind,
                    context,
                    command_name,
                    created,
                    after.agent_briefs,
                );
            }
            self.close_orphaned_chats(&after.removed_panes);
        }
        #[cfg(not(feature = "agent"))]
        let _ = (after.agent_panes_opened, after.agent_briefs);
        for pane in after.removed_panes {
            self.cancel_pane_files(pane);
            self.publish(EventPayload::PaneRemoved(pane));
        }
        if after.client_terminal_changed {
            self.publish_terminal_negotiation();
        }
        if mux_options_event {
            self.publish_effective_mux_options(None);
        } else if !after.mux_option_refresh_sessions.is_empty() {
            self.publish_effective_mux_options(Some(&after.mux_option_refresh_sessions));
        }
        if after.mode_styles_changed {
            self.refresh_published_appearance();
        }
        self.restyle_client_overlays();
        for channel in after.option_signals {
            if let Some(pane) = channel
                .strip_prefix("@agent_state@")
                .filter(|target| target.starts_with('%'))
                .and_then(|target| target.parse::<PaneId>().ok())
            {
                after.snapshot_changed |= pane_summary::mirror_agent_state(
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
                after.snapshot_changed |=
                    pane_summary::mirror_pane_name(&mut self.inner.lock(), pane);
                #[cfg(all(feature = "agent", unix))]
                self.update_terminal_peer(pane);
            }
            self.signal_wait_channel(&channel);
        }
        if after.snapshot_changed {
            self.publish_snapshot();
        } else if after.status_formats_changed {
            self.refresh_status();
        } else if !after.status_refresh_sessions.is_empty() {
            self.refresh_status_for_sessions(Some(&after.status_refresh_sessions));
        }
        let replay_client = context.replay_client();
        let source_client = replay_client.unwrap_or(client);
        let cwd_client = replay_client
            .or_else(|| context.control_command_target().map(|(client, _)| client))
            .unwrap_or(client);
        for (pane, bytes) in after.pane_stream_inputs {
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
            let source_session_working_directory = (after.reload_config
                || !after.source_files.is_empty())
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
        let source_working_directory = (after.reload_config || !after.source_files.is_empty())
            .then(|| {
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
        let source_invocation = !after.source_files.is_empty();
        let mut reported_source_failure = false;
        let mut reported_source_callback_failure = false;
        let control_source_invocation = control_target.is_some() && source_invocation;
        let suppress_source_replay_output = source_kind == ClientKind::Command
            && queue_execution.is_some_and(CommandQueueExecution::is_draining);
        let captured_control_source = control_client.is_some_and(|client| {
            !after.source_files.is_empty() && self.is_capturing_control_command_events(client)
        });
        for request in after.source_files {
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
                reload_config: after.reload_config,
                read_only,
                client_timers_changed,
                status_formats_changed: after.status_formats_changed,
                status_refresh_sessions: after.status_refresh_sessions,
                pending_hook_events: after.pending_hook_events,
                notifications_only,
                incremental_start: after.incremental_start,
                terminal_wait: after.terminal_wait,
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
        if after.reload_config
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
        if client_timers_changed
            || after.status_formats_changed
            || !after.status_refresh_sessions.is_empty()
        {
            self.nudge_client_timers();
        }
        after.pending_hook_events.extend(std::mem::take(
            &mut self.inner.lock().waits.deferred_event_hooks,
        ));
        if !after.pending_hook_events.is_empty() {
            self.wake_control_queue(client, kind);
        }
        if notifications_only {
            self.run_event_hooks(after.pending_hook_events);
        } else if let Some(queue_execution) = queue_execution {
            self.publish_event_control_notifications(&mut after.pending_hook_events);
            queue_execution
                .pending_event_hooks
                .borrow_mut()
                .extend(after.pending_hook_events);
        } else {
            self.run_event_hooks(after.pending_hook_events);
        }
        if let Some(submission) = after.incremental_start {
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
            None => match after.terminal_wait {
                Some(wait) => wait.finish(self, execution),
                None => Ok(execution),
            },
        }
    }
}
