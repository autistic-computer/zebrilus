use super::*;

impl Shared {
    pub(super) fn watch_terminal(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
    ) -> Result<(), DaemonError> {
        #[cfg(any(test, windows))]
        self.start_watcher_consumer()?;
        terminal.install_frame_sink(Arc::new(shard_sink::PaneSink::new(
            pane,
            Arc::clone(&self.terminal_frames),
            terminal.output_wake(),
        )));
        #[cfg(windows)]
        {
            let weak = Arc::downgrade(&self.server_owner());
            self.control_wake.install_pump(move || {
                if let Some(shared) = weak.upgrade() {
                    let owner = Arc::clone(&shared);
                    let _ = shared.spawn_delay(Duration::ZERO, move || {
                        owner.turn_control_output(false);
                    });
                }
            });
        }
        {
            let mut inner = self.inner.lock();
            for (client, state) in &inner.clients {
                if let Some(kind) = state
                    .streamed_terminals
                    .as_ref()
                    .and_then(|streamed| streamed.get(&pane))
                {
                    sync_view_sink(&inner, terminal, *client, pane, Some(*kind));
                }
            }
            sync_control_routes(&mut inner, &self.control_wake);
            if let Some(sink) = shard_sink::PaneSink::of(terminal) {
                for route in inner.attachment.control_routes.values() {
                    if route.panes.contains(&pane) {
                        sink.set_control(&route.mailbox, true);
                    }
                }
                if let Some(output) = inner
                    .jobs
                    .pane_pipes
                    .get(&pane)
                    .and_then(|pipe| pipe.tap_output.as_ref())
                {
                    sink.set_pipe(Some(Arc::clone(output)));
                }
            }
        }
        self.watcher_tx.register(
            watchers::Watcher::terminal(pane, terminal),
            terminal.events(),
            VecDeque::new(),
        )
    }

    pub(super) fn close_exited_terminal(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
    ) {
        let status = terminal.latest_viewport().status.clone();
        if matches!(
            status,
            zz_terminal::SessionStatus::Starting | zz_terminal::SessionStatus::Running
        ) {
            return;
        }
        let exit_code = pane_wait_exit_code(terminal, &status);
        {
            let mut inner = self.inner.lock();
            if !inner
                .terminals
                .get(&pane)
                .is_some_and(|current| Arc::ptr_eq(current, terminal))
            {
                return;
            }
            Self::wake_pane_exit_wait(&mut inner, pane, exit_code);
        }
        self.finish_editor(pane, exit_code);
        let (failed, dead_status, dead_signal) = match status {
            zz_terminal::SessionStatus::Exited(status) => {
                let failed = status.code != 0 || status.signal.is_some();
                let dead_status = status.signal.is_none().then_some(status.code);
                (failed, dead_status, status.signal.clone())
            }
            zz_terminal::SessionStatus::Failed(_) => (true, None, None),
            zz_terminal::SessionStatus::Starting | zz_terminal::SessionStatus::Running => return,
        };
        self.raise_shell_marks(pane, terminal);
        let pipe = {
            let mut inner = self.inner.lock();
            inner
                .terminals
                .get(&pane)
                .is_some_and(|current| Arc::ptr_eq(current, terminal))
                .then(|| inner.jobs.pane_pipes.remove(&pane))
                .flatten()
        };
        if let Some(pipe) = pipe {
            stop_pane_pipe(pipe);
        }
        let Some((mut context, retained, changed, mut events, hook_scope, pane_focus_before)) = ({
            let mut inner = self.inner.lock();
            if !inner
                .terminals
                .get(&pane)
                .is_some_and(|current| Arc::ptr_eq(current, terminal))
            {
                return;
            }
            let Some(context) = ExecutionContext::for_pane(&inner.engine.state, pane) else {
                return;
            };
            inner.engine.set_format_now(unix_timestamp());
            let hook_scope = hook_events::HookScope::open(&mut inner.engine);
            let pane_focus_before = hook_events::FocusProbeScope::open(&mut inner);
            let exited = PendingHookEvent::live_pane("pane-exited", pane, &inner.engine)
                .map(|event| event.with_exit_status(dead_status, dead_signal.as_deref(), !failed));
            let retained = inner
                .engine
                .retain_exited_pane(pane, failed)
                .unwrap_or(false);
            let changed = if retained {
                let (engine, facts) =
                    split_borrowed_format_hook_facts(&mut inner, CommandFormatSeed::default());
                let mut hooks = DaemonFormatHooks::command(&facts);
                engine
                    .mark_pane_dead_with_hooks(
                        pane,
                        dead_status,
                        dead_signal.as_deref(),
                        &mut hooks,
                    )
                    .unwrap_or(false)
            } else {
                false
            };
            if changed {
                pane_summary::stamp_pane(&mut inner, pane, unix_timestamp());
            }
            let (mut events, hook_scope) = if retained {
                let mut events = hook_scope.finish(&inner.engine, "").events;
                if changed
                    && let Some(event) =
                        PendingHookEvent::live_pane("pane-died", pane, &inner.engine)
                {
                    events.push(event.with_exit_status(
                        dead_status,
                        dead_signal.as_deref(),
                        !failed,
                    ));
                }
                (events, None)
            } else {
                (exited.into_iter().collect(), Some(hook_scope))
            };
            if !changed && retained {
                events.clear();
            }
            Some((
                context,
                retained,
                changed,
                events,
                hook_scope,
                pane_focus_before,
            ))
        }) else {
            return;
        };

        if retained {
            log::debug!(
                target: "zz_daemon::diagnostics::terminal",
                "terminal worker finished pane={pane}; retaining dead pane"
            );
            let notice = {
                let inner = self.inner.lock();
                let template = inner.engine.remain_on_exit_format_for_pane(pane);
                if template.is_empty() {
                    Arc::<str>::from("")
                } else {
                    let window = inner.engine.state.window_for_pane(pane);
                    let session = window
                        .and_then(|window| inner.engine.state.windows.get(&window))
                        .map(|window| window.session);
                    let expanded = expanded_style_value(
                        &inner.engine,
                        &inner.config.config_files,
                        &template,
                        session,
                        window,
                        Some(pane),
                    );
                    Arc::<str>::from(dead_notice_text(&expanded))
                }
            };
            terminal.write_dead_notice(Some(notice));
            if changed {
                self.publish_snapshot();
            }
            self.run_event_hooks(events);
            return;
        }

        log::debug!(
            target: "zz_daemon::diagnostics::terminal",
            "terminal worker finished pane={pane}; closing pane"
        );
        terminal.write_dead_notice(None);
        let target = pane.to_string();
        let command = CommandInvocation::new("kill-pane", ["-t", target.as_str()]);
        context.no_hooks = true;
        let result = self.execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &command,
        );
        if let Err(error) = &result
            && self.is_current_terminal(pane, terminal)
        {
            log::error!(
                target: "zz_daemon::diagnostics::terminal",
                "failed to close exited terminal pane={pane}: {error}"
            );
        }
        if result.is_ok()
            && let Some(hook_scope) = hook_scope
        {
            let mut inner = self.inner.lock();
            let pane_focus_before = pane_focus_before.close(&inner);
            let anchor = events.len();
            events.extend(hook_scope.finish(&inner.engine, "").events);
            let focus = pane_focus_hook_events(&mut inner, &pane_focus_before);
            splice_pane_focus_events(&mut events, anchor, focus);
        }
        if result.is_ok() || !self.is_current_terminal(pane, terminal) {
            self.run_event_hooks(events);
        }
    }

    pub(super) fn is_current_terminal(
        &self,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
    ) -> bool {
        let current = !terminal.is_retired();
        debug_assert!(
            !current
                || self.inner.try_lock().is_none_or(|inner| inner
                    .terminals
                    .get(&pane)
                    .is_none_or(|mapped| Arc::ptr_eq(mapped, terminal))),
            "a live terminal is mapped to another pane's session"
        );
        current
    }

    pub(super) fn synchronize_pane_progress(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        current_command: &str,
        bar_state: ProgressBarState,
    ) {
        if terminal.program_status().reported() {
            return;
        }
        let value = match bar_state {
            ProgressBarState::Indeterminate => "working",
            ProgressBarState::Hidden => "idle",
            ProgressBarState::Normal | ProgressBarState::Error | ProgressBarState::Paused => return,
        };
        let Some(command) = Path::new(current_command)
            .file_name()
            .and_then(OsStr::to_str)
        else {
            return;
        };
        let target = pane.to_string();
        {
            let inner = self.inner.lock();
            if !inner
                .terminals
                .get(&pane)
                .is_some_and(|current| Arc::ptr_eq(current, terminal))
                || !matches!(
                    inner.engine.state.pane(pane).map(|pane| &pane.kind),
                    Some(PaneKind::Terminal)
                )
            {
                return;
            }
            let Some(window) = inner.engine.state.window_for_pane(pane) else {
                return;
            };
            let session = inner.engine.state.windows[&window].session.to_string();
            let window = window.to_string();
            let facts = inner.engine.format_facts();
            let commands = facts
                .user_option(&target, &window, &session, "@agent-progress-commands")
                .unwrap_or("claude");
            if !commands
                .split(|character: char| character.is_whitespace() || character == ',')
                .any(|listed| !listed.is_empty() && listed == command)
            {
                return;
            }
        }
        self.write_pane_agent_state(pane, value);
    }

    pub(super) fn synchronize_pane_program_status(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        status: &ProgramStatus,
        worked: bool,
    ) {
        let headline = status.headline().map(|record| PaneStatus {
            state: match record.state {
                ProgramState::Idle => PaneStatusState::Idle,
                ProgramState::Working => PaneStatusState::Working,
                ProgramState::Done => PaneStatusState::Done,
                ProgramState::Blocked => PaneStatusState::Blocked,
                ProgramState::Error => PaneStatusState::Error,
            },
            kind: record.kind.map(|kind| match kind {
                ProgramBlockKind::Permission => PaneStatusKind::Permission,
                ProgramBlockKind::Question => PaneStatusKind::Question,
                ProgramBlockKind::Auth => PaneStatusKind::Auth,
            }),
            progress: record.progress,
            app: record.app,
            title: record.title,
            message: record.message,
        });
        let reported = status.reported();
        let (changed, agent_state) = {
            let mut inner = self.inner.lock();
            if !inner
                .terminals
                .get(&pane)
                .is_some_and(|current| Arc::ptr_eq(current, terminal))
            {
                return;
            }
            let owned = if reported {
                !inner.panes.program_status_panes.insert(pane)
            } else {
                inner.panes.program_status_panes.remove(&pane)
            };
            #[cfg(all(feature = "agent", unix))]
            if owned != reported {
                inner.agents.claude_peer_states.remove(&pane);
            }
            let agent_state = (reported || owned).then(|| {
                match headline.as_ref().map(|headline| headline.state) {
                    Some(PaneStatusState::Working) => "working",
                    Some(PaneStatusState::Blocked) => "blocked",
                    Some(PaneStatusState::Error) => "failed",
                    Some(PaneStatusState::Idle | PaneStatusState::Done) | None => "idle",
                }
            });
            (
                pane_summary::apply_pane_status(&mut inner, pane, headline, unix_timestamp()),
                agent_state,
            )
        };
        if changed {
            self.publish_snapshot();
        }
        if let Some(value) = agent_state {
            if worked && value == "idle" {
                self.write_pane_agent_state(pane, "working");
            }
            self.write_pane_agent_state(pane, value);
        }
    }

    pub(super) fn synchronize_pane_title(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        title: &str,
        program_wrote_title: bool,
    ) {
        let event = {
            let mut inner = self.inner.lock();
            if !inner
                .terminals
                .get(&pane)
                .is_some_and(|current| Arc::ptr_eq(current, terminal))
            {
                return;
            }
            if !inner.engine.allow_set_title(pane) {
                return;
            }
            if title.is_empty() {
                return;
            }
            let changed = inner
                .engine
                .state
                .update_pane_title_from_terminal(pane, title, program_wrote_title)
                .unwrap_or(false);
            changed
                .then(|| PendingHookEvent::live_pane("pane-title-changed", pane, &inner.engine))
                .flatten()
                .map(|mut event| {
                    event
                        .variables
                        .insert("hook_new_title".to_owned(), title.to_owned());
                    event
                })
        };
        if let Some(event) = event {
            self.request_publish(timers::PublishReason::Tree);
            self.run_event_hooks(vec![event]);
        }
    }

    pub(super) fn rename_window_from_pane(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        name: &str,
    ) {
        if !self.is_current_terminal(pane, terminal) {
            return;
        }
        let target = pane.to_string();
        let mut context = ExecutionContext::default();
        let commands = if name.is_empty() {
            vec![CommandInvocation::new(
                "set-option",
                ["-w", "-u", "-t", target.as_str(), "automatic-rename"],
            )]
        } else {
            vec![
                CommandInvocation::new(
                    "set-option",
                    ["-w", "-t", target.as_str(), "automatic-rename", "off"],
                ),
                CommandInvocation::new("rename-window", ["-t", target.as_str(), name]),
            ]
        };
        let queue = self.command_queue_execution(CommandExecutionState {
            draining: false,
            wait_yields: false,
            detached: false,
            deferred_shutdown: Cell::new(DeferredShutdown::None),
            deferred_control_exit: Cell::new(None),
            yielded: Cell::new(CommandQueueYield::None),
            yield_boundary: false,
            shutdown_blocker: RefCell::new(None),
            pending_event_hooks: RefCell::new(Vec::new()),
            deferred_shell_jobs: RefCell::new(Vec::new()),
            callback_parse_failures: RefCell::new(Vec::new()),
            deferred_config_replay_issues: RefCell::new(Vec::new()),
            reported_failures: Cell::new(false),
            frame_active: Cell::new(false),
            child: RefCell::new(None),
            suppress_after_hooks: Cell::new(true),
            suppress_output: Cell::new(false),
        });
        for command in &commands {
            if let Err(error) = self.execute_with_mux_source_in_queue(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                command,
                MuxOptionSource::RuntimeCommand,
                Some(&queue),
            ) {
                log::warn!(
                    target: "zz_daemon::diagnostics::terminal",
                    "allow-rename request failed pane={pane} name={name:?} error={error}"
                );
                break;
            }
        }
        let events = queue
            .pending_event_hooks
            .borrow_mut()
            .drain(..)
            .collect::<Vec<_>>();
        let jobs = queue
            .deferred_shell_jobs
            .borrow_mut()
            .drain(..)
            .collect::<Vec<_>>();
        self.request_publish(timers::PublishReason::Tree);
        self.run_event_hooks(events);
        for launch in jobs {
            launch(self);
        }
    }

    pub(super) fn note_pane_output(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        now: Instant,
    ) {
        let (check, silence_schedule, alert_window, refresh_activity_choosers, activity_hook) = {
            let mut inner = self.inner.lock();
            if !inner
                .terminals
                .get(&pane)
                .is_some_and(|current| Arc::ptr_eq(current, terminal))
            {
                return;
            }
            let activity_hook = pane_event_if_observed(&mut inner, "pane-activity", pane);
            let mut silence_schedule = None;
            let mut alert_window = None;
            terminal_reads::pane_changed(&mut inner, pane);
            inner.engine.set_format_now(unix_timestamp());
            for session in inner
                .clients
                .values_mut()
                .filter_map(|c| c.copy_session.as_mut())
            {
                if session.pane == pane && !session.exiting {
                    session.unseen = true;
                }
            }
            inner.engine.touch_window_activity_for_pane(pane);
            if let Some(window) = inner.engine.state.window_for_pane(pane) {
                if !inner.panes.silence_deadlines.contains_key(&window) {
                    silence_schedule = schedule_window_silence(&mut inner, window);
                }
                if inner.panes.control_activity_pending.contains(&pane)
                    || (inner.engine.monitor_activity_for_window(window)
                        && !inner
                            .engine
                            .state
                            .windows
                            .get(&window)
                            .is_some_and(|state| state.activity_flag))
                {
                    alert_window = Some(window);
                }
            }
            let refresh_activity_choosers = inner
                .clients
                .values()
                .filter_map(|c| c.choose_tree.as_ref())
                .any(|chooser| chooser.sort.order() == Some(TmuxSortOrder::Activity));
            let check = self.admit_name_check(&mut inner, pane, terminal, now, false);
            (
                check,
                silence_schedule,
                alert_window,
                refresh_activity_choosers,
                activity_hook,
            )
        };
        if let Some(hook) = activity_hook {
            self.run_event_hooks(vec![hook]);
        }
        self.raise_shell_marks(pane, terminal);
        if let Some(deadline) = silence_schedule {
            let _ = self.timer_tx.send(timers::TimerInput::Silence(
                SilenceDeadlineCommand::Schedule(deadline),
            ));
        }
        if let Some(window) = alert_window {
            self.raise_window_activity(window, pane);
        }
        if refresh_activity_choosers {
            self.refresh_choose_trees();
        }
        if check {
            self.check_pane_runtime(pane, terminal, true, now);
        }
    }

    pub(super) fn note_pane_frame(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        now: Instant,
    ) {
        let check = {
            let mut inner = self.inner.lock();
            inner
                .terminals
                .get(&pane)
                .is_some_and(|current| Arc::ptr_eq(current, terminal))
                && self.admit_name_check(&mut inner, pane, terminal, now, true)
        };
        if check {
            self.check_pane_runtime(pane, terminal, false, now);
        }
    }

    pub(super) fn check_pane_runtime(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        output: bool,
        now: Instant,
    ) {
        terminal.take_output_since_check();
        let (foreground, current_command, live_path) = terminal_foreground_facts(terminal);
        self.synchronize_pane_runtime(
            pane,
            terminal,
            foreground,
            &current_command,
            live_path,
            output,
            now,
        );
    }

    pub(super) fn synchronize_pane_reported_path(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        reported_path: &str,
    ) {
        let changed = {
            let mut inner = self.inner.lock();
            if !inner
                .terminals
                .get(&pane)
                .is_some_and(|current| Arc::ptr_eq(current, terminal))
            {
                return;
            }
            let mut runtime = inner
                .engine
                .pane_runtime_facts(pane)
                .cloned()
                .unwrap_or_default();
            if runtime.reported_path == reported_path {
                return;
            }
            reported_path.clone_into(&mut runtime.reported_path);
            inner.engine.set_pane_runtime_facts(pane, runtime)
        };
        if changed {
            self.request_publish(if self.locate_moved_pane(pane) {
                timers::PublishReason::Tree
            } else {
                timers::PublishReason::RuntimeFacts
            });
        }
    }

    pub(super) fn synchronize_pane_runtime(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        foreground: Option<u32>,
        current_command: &str,
        live_path: Option<String>,
        output: bool,
        now: Instant,
    ) {
        let events = self.apply_pane_runtime(
            pane,
            terminal,
            foreground,
            current_command,
            live_path,
            output,
            now,
        );
        self.run_event_hooks(events);
    }

    pub(super) fn apply_pane_runtime(
        self: &Arc<Self>,
        pane: PaneId,
        terminal: &Arc<TerminalSession>,
        foreground: Option<u32>,
        current_command: &str,
        live_path: Option<String>,
        output: bool,
        now: Instant,
    ) -> Vec<PendingHookEvent> {
        let pid = terminal.process_id();
        let tty = terminal
            .tty()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ((changed, events), relocated, program, shell) = {
            let mut inner = self.inner.lock();
            if !inner
                .terminals
                .get(&pane)
                .is_some_and(|current| Arc::ptr_eq(current, terminal))
            {
                return Vec::new();
            }
            let previous = inner
                .engine
                .pane_runtime_facts(pane)
                .cloned()
                .unwrap_or_default();
            let current_path = live_path.unwrap_or_else(|| previous.start_path.clone());
            let relocated =
                previous.reported_path.is_empty() && previous.current_path != current_path;
            let runtime = PaneRuntimeFacts {
                current_command: current_command.to_owned(),
                current_path,
                dead_signal: previous.dead_signal,
                reported_path: previous.reported_path,
                start_path: previous.start_path,
                pid,
                tty,
            };
            let result = if inner.engine.pane_runtime_facts(pane) == Some(&runtime) {
                (None, Vec::new())
            } else {
                let rename_due = previous.current_command != runtime.current_command
                    && inner.engine.automatic_rename_due(pane, now);
                let (engine, facts) =
                    split_borrowed_format_hook_facts(&mut inner, CommandFormatSeed::default());
                let mut hooks = DaemonFormatHooks::command(&facts).withhold_facts(!rename_due);
                let scope = rename_due.then(|| hook_events::HookScope::open(engine));
                let generation = engine.state.generation();
                let changed = engine.set_pane_runtime_facts_at(pane, runtime, &mut hooks, now);
                let renamed = engine.state.generation() != generation;
                let events = match scope {
                    Some(scope) if renamed => scope.finish(engine, "").events,
                    _ => Vec::new(),
                };
                (
                    changed.then_some(if renamed {
                        timers::PublishReason::Tree
                    } else {
                        timers::PublishReason::RuntimeFacts
                    }),
                    events,
                )
            };
            if output {
                inner.engine.note_automatic_rename_output(pane, now);
            }
            self.schedule_window_renames(&mut inner);
            let program =
                pane_summary::sight_program(&mut inner, pane, foreground, current_command, now);
            let shell = pane_summary::sight_shell(&mut inner, pane, foreground, current_command);
            (result, relocated, program, shell)
        };
        if let pane_summary::ShellSighting::Young(wait) = shell {
            self.settle_pane_program(pane, terminal, wait);
        }
        let changed = if (relocated && self.locate_moved_pane(pane))
            || program == pane_summary::ProgramSighting::Cleared
            || shell == pane_summary::ShellSighting::Named
        {
            Some(timers::PublishReason::Tree)
        } else {
            changed
        };
        if let pane_summary::ProgramSighting::Changed(foreground) = program {
            self.watch_pane_program(pane, terminal, foreground, pid, current_command, true);
        }
        if (output || changed.is_some()) && pid.is_some() {
            self.request_peer_probe();
        }
        if let Some(reason) = changed {
            self.request_publish(reason);
        }
        events
    }
}

fn dead_notice_text(expanded: &str) -> String {
    let mut output = String::with_capacity(expanded.len());
    for segment in zz_protocol::parse_styled_segments(expanded) {
        if segment.text.is_empty() {
            continue;
        }
        let mut parameters = Vec::new();
        let attributes = &segment.style.attributes;
        if attributes.noattr != zz_protocol::TmuxAttributeState::On {
            for (state, code) in [
                (attributes.bold, "1"),
                (attributes.dim, "2"),
                (attributes.italics, "3"),
                (attributes.underscore, "4"),
                (attributes.double_underscore, "4:2"),
                (attributes.curly_underscore, "4:3"),
                (attributes.dotted_underscore, "4:4"),
                (attributes.dashed_underscore, "4:5"),
                (attributes.blink, "5"),
                (attributes.reverse, "7"),
                (attributes.hidden, "8"),
                (attributes.strikethrough, "9"),
                (attributes.overline, "53"),
            ] {
                if state == zz_protocol::TmuxAttributeState::On {
                    parameters.push(code.to_owned());
                }
            }
        }
        if let Some(colour) = segment.style.fg {
            push_notice_colour(&mut parameters, colour, 30);
        }
        if let Some(colour) = segment.style.bg {
            push_notice_colour(&mut parameters, colour, 40);
        }
        output.push_str("\x1b[0");
        for parameter in &parameters {
            output.push(';');
            output.push_str(parameter);
        }
        output.push('m');
        output.push_str(&segment.text);
    }
    output
}

fn push_notice_colour(parameters: &mut Vec<String>, colour: zz_protocol::TmuxColour, base: u8) {
    match colour {
        zz_protocol::TmuxColour::Basic(index) if index < 8 => {
            parameters.push((base + index).to_string());
        }
        zz_protocol::TmuxColour::Basic(index) => {
            parameters.push((base + 60 + index.saturating_sub(8)).to_string());
        }
        zz_protocol::TmuxColour::Indexed(index) => {
            parameters.push(format!("{};5;{index}", base + 8));
        }
        zz_protocol::TmuxColour::Rgb(rgb) => parameters.push(format!(
            "{};2;{};{};{}",
            base + 8,
            (rgb >> 16) & 0xff,
            (rgb >> 8) & 0xff,
            rgb & 0xff
        )),
        zz_protocol::TmuxColour::Default | zz_protocol::TmuxColour::Terminal => {
            parameters.push((base + 9).to_string());
        }
        zz_protocol::TmuxColour::Theme(_) => {}
    }
}
