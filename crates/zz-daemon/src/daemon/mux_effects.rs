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

        self.drain_server_effects(
            client,
            kind,
            context,
            queue_execution,
            &mut execution,
            after.import_tmux_config,
            &mut after.reload_config,
            after.force_shutdown_requested,
            after.immediate_hooks,
            recheck_shutdown_requested,
        )?;
        self.drain_terminal_effects(
            client,
            format_variables,
            after.pipes_to_close,
            after.key_listing,
            after.deferred_terminal_commands,
            &mut after.terminal_wait,
            after.respawned_terminals,
            after.snapshot_changed,
        );
        self.drain_mode_effects(
            client,
            kind,
            context,
            after.pane_mode_keys,
            after.pane_mode_pointers,
            after.injected_client_keys,
            after.mode_table_keys,
            after.refresh_armed,
            after.unfocused_copy_mode_exits,
            after.retired_command_outputs,
            after.relocated_terminal_views,
        )?;
        self.drain_detach_effect(after.detach, event_hooks_enabled);
        self.drain_attach_effect(
            client,
            invoking_client_terminal,
            event_hooks_enabled,
            after.attach,
            &mut after.pending_hook_events,
        )?;
        self.drain_watched_terminals(after.terminals_to_watch, &mut after.snapshot_changed)?;
        self.drain_client_events(
            client,
            after.client_events,
            after.direct_events,
            after.deferred_direct_events,
            after.targeted_events,
            after.client_message_retires,
            after.prompt_cleared_message,
            after.resume_client_terminals,
            after.client_message_schedule,
            after.panes_mode_overlays,
            after.panes_mode_deadline,
        );
        self.drain_alert_effects(
            after.monitor_silence_changed,
            after.silence_schedules,
            after.activity_requeues,
        );
        #[cfg(feature = "agent")]
        self.drain_agent_effects(
            client,
            kind,
            context,
            command_name,
            &execution,
            after.agent_options_changed,
            &after.removed_panes,
            &after.agent_panes_closed,
            after.agent_panes_opened,
            agent_children,
            after.agent_briefs,
        );
        #[cfg(not(feature = "agent"))]
        let _ = (after.agent_panes_opened, after.agent_briefs);
        self.drain_publication_effects(
            after.removed_panes,
            after.client_terminal_changed,
            mux_options_event,
            &after.mux_option_refresh_sessions,
            after.mode_styles_changed,
            after.option_signals,
            after.snapshot_changed,
            after.status_formats_changed,
            &after.status_refresh_sessions,
        );
        let replay_client = context.replay_client();
        let source_client = replay_client.unwrap_or(client);
        let cwd_client = replay_client
            .or_else(|| context.control_command_target().map(|(client, _)| client))
            .unwrap_or(client);
        self.drain_stream_inputs(
            client,
            context,
            &mut execution,
            split_caller_stream,
            source_client,
            after.pane_stream_inputs,
        )?;
        self.run_source_effects(
            client,
            kind,
            context,
            queue_execution,
            invoking_client_terminal,
            execution,
            replay_client,
            source_client,
            cwd_client,
            read_only,
            notifications_only,
            client_timers_changed,
            after.source_files,
            after.reload_config,
            after.status_formats_changed,
            after.status_refresh_sessions,
            after.pending_hook_events,
            after.incremental_start,
            after.terminal_wait,
        )
    }
}
