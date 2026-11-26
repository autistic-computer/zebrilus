use super::*;

impl Shared {
    pub(super) fn freeze_response_admissions_and_wait(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.response_admissions.lock();
        state.frozen = true;
        while state.active != 0 {
            if self
                .response_admissions_changed
                .wait_until(&mut state, deadline)
                .timed_out()
            {
                log::warn!(
                    "shutdown proceeding before {} command responses were admitted",
                    state.active
                );
                return false;
            }
        }
        true
    }

    pub(super) fn drain_client_writers_for_shutdown(&self, timeout: Duration) -> bool {
        let mailboxes = self
            .client_writers
            .lock()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let deadline = Instant::now() + timeout;
        for mailbox in &mailboxes {
            mailbox.close_after_flush();
        }
        let mut drained = true;
        for mailbox in &mailboxes {
            if !mailbox.wait_writer_finished(deadline) {
                log::warn!("shutdown proceeding before a client writer drained");
                drained = false;
            }
        }
        drained
    }

    pub(super) fn request_shutdown(self: &Arc<Self>) {
        self.shutdown_forced.store(true, Ordering::Release);
        self.request_shutdown_with_hooks(true);
    }

    pub(super) fn request_shutdown_without_hooks(self: &Arc<Self>) {
        self.request_shutdown_with_hooks(false);
    }

    pub(super) fn request_shutdown_with_hooks(self: &Arc<Self>, run_hooks: bool) {
        #[cfg(unix)]
        if self.loop_active.load(Ordering::Acquire) {
            self.shutdown_pending.store(true, Ordering::Release);
            self.lifecycle.shutdown(run_hooks);
            self.accept_wake.wake();
            return;
        }
        self.stop_with_hooks(run_hooks);
    }

    fn stop_with_hooks(self: &Arc<Self>, run_hooks: bool) {
        self.shutdown_pending.store(true, Ordering::Release);
        if run_hooks {
            self.enter_queue_shutdown_phase();
        }
        if !self.close_shutdown_blockers() {
            return;
        }
        self.begin_stopping(run_hooks);
    }

    pub(super) fn begin_stopping(self: &Arc<Self>, run_hooks: bool) {
        #[cfg(unix)]
        if self.shutdown_cleanup_started.swap(true, Ordering::AcqRel) {
            return;
        }
        #[cfg(test)]
        if let Some(hook) = self.begin_stopping_hook.lock().take() {
            let _ = hook.reached.send(());
            let _ = hook.release.recv();
        }
        self.stopping.store(true, Ordering::Release);
        self.accept_wake.wake();
        self.startup_changed.notify_all();
        self.drop_pending_execs();
        let events = self.stop_shutdown_resources(true, run_hooks);
        if run_hooks && !self.shutdown_drops_event_hooks.load(Ordering::Acquire) {
            self.run_shutdown_event_hooks(events);
        }
        #[cfg(unix)]
        self.shutdown_cleanup_complete
            .store(true, Ordering::Release);
        self.accept_wake.wake();
    }

    #[cfg(unix)]
    pub(super) fn force_shutdown(self: &Arc<Self>) {
        if self.stopping.load(Ordering::Acquire) {
            return;
        }
        log::warn!(
            "stopping past {} active shutdown blockers",
            self.active_shutdown_blockers()
        );
        self.shutdown_forced.store(true, Ordering::Release);
        self.shutdown_pending.store(true, Ordering::Release);
        self.close_shutdown_blockers();
        self.begin_stopping(true);
    }

    pub(super) fn stop_shutdown_resources(
        self: &Arc<Self>,
        terminate_shell_jobs: bool,
        destroy_sessions: bool,
    ) -> Vec<PendingHookEvent> {
        let (events, terminals, wakes, pipes, shell_jobs, confirm_waiters) = {
            let mut inner = self.inner.lock();
            let mut terminals = std::mem::take(&mut inner.terminals)
                .values()
                .cloned()
                .inspect(|terminal| retire_terminal(terminal))
                .collect::<Vec<_>>();
            terminals.extend(
                inner
                    .clients
                    .values()
                    .filter_map(|c| c.command_output.as_ref())
                    .map(|output| Arc::clone(&output.terminal)),
            );
            let mut events = Vec::new();
            if destroy_sessions {
                let mut sessions = inner
                    .engine
                    .state
                    .sessions
                    .values()
                    .map(|session| (session.name.clone(), session.id, session.windows.clone()))
                    .collect::<Vec<_>>();
                sessions.sort_by(|left, right| left.0.cmp(&right.0));
                for (_, session, windows) in sessions {
                    let before = MuxHookSnapshot::capture(&inner.engine);
                    if inner.engine.state.kill_session(session).is_ok()
                        && let Some(session_state) = before.sessions.get(&session)
                    {
                        events.push(PendingHookEvent::session(
                            "session-closed",
                            session,
                            session_state,
                            &before,
                        ));
                        for window in windows {
                            if let Some(window_state) = before.windows.get(&window) {
                                events.push(PendingHookEvent::winlink(
                                    "window-unlinked",
                                    session,
                                    session_state,
                                    window,
                                    window_state,
                                    &before,
                                ));
                            }
                        }
                    }
                }
            }
            for pane in inner
                .panes
                .pane_read_observations
                .keys()
                .copied()
                .collect::<Vec<_>>()
            {
                terminal_reads::pane_changed(&mut inner, pane);
            }
            inner.panes.pane_read_observations.clear();
            inner.panes.terminal_spawns.clear();
            inner.panes.terminal_geometries.clear();
            inner.panes.reported_pane_places.clear();
            inner.attachment.attached.clear();
            for client in inner.clients.values_mut() {
                client.visible_terminals = None;
                client.streamed_terminals = None;
                client.terminal_preview = false;
                client.visible_agents = None;
                client.focused_window = None;
            }
            let mut wakes = take_all_wait_wakes(&mut inner.waits.wait_channels);
            wakes.extend(hook_events::take_event_waiter_wakes(
                &mut inner.waits.event_waiters,
                None,
            ));
            pane_exit::cancel_all(&mut inner);
            let pipes = std::mem::take(&mut inner.jobs.pane_pipes)
                .into_values()
                .collect::<Vec<_>>();
            inner.attachment.control_routes.clear();
            let shell_jobs = if terminate_shell_jobs {
                std::mem::take(&mut inner.jobs.shell_jobs)
                    .into_values()
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            for client in inner.clients.values_mut() {
                client.menu.take();
            }
            let confirm_waiters = inner
                .clients
                .values_mut()
                .filter_map(|c| c.confirm.take())
                .filter_map(|confirm| match confirm.execution {
                    ConfirmExecution::Blocking { waiter } => Some(waiter),
                    ConfirmExecution::Deferred { .. } | ConfirmExecution::Background { .. } => None,
                })
                .collect::<Vec<_>>();
            (events, terminals, wakes, pipes, shell_jobs, confirm_waiters)
        };
        for terminal in terminals {
            terminal.terminate();
        }
        self.wake_wait_items(wakes);
        for pipe in pipes {
            stop_pane_pipe(pipe);
        }
        for process in shell_jobs {
            #[cfg(unix)]
            if let Some(process) = process.lock().take() {
                process.cancel.store(true, Ordering::Release);
                if let Some(pid) = rustix::process::Pid::from_raw(process.pid as i32) {
                    let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
                }
                self.pipe_jobs.notify();
            }
            #[cfg(not(unix))]
            terminate_managed_process(&process);
        }
        for waiter in confirm_waiters {
            waiter.complete(false);
        }
        events
    }

    pub(super) fn begin_queue_shutdown(&self) {
        self.shutdown_forced.store(true, Ordering::Release);
        self.shutdown_pending.store(true, Ordering::Release);
    }

    pub(super) fn enter_queue_shutdown_phase(self: &Arc<Self>) {
        self.shutdown_forced.store(true, Ordering::Release);
        if self.shutdown_blockers.lock().detached != 0 {
            self.shutdown_drops_event_hooks
                .store(true, Ordering::Release);
        }
        let events = self.stop_shutdown_resources(false, true);
        if !self.shutdown_drops_event_hooks.load(Ordering::Acquire) {
            self.run_shutdown_event_hooks(events);
        }
    }

    pub(super) fn announce_shutdown(&self) {
        let mut announced = self.shutdown_announced.lock();
        if std::mem::replace(&mut *announced, true) {
            return;
        }
        let clients = self
            .inner
            .lock()
            .clients
            .iter()
            .filter_map(|(id, client)| client.subscriber.as_ref().map(|_| id))
            .copied()
            .collect::<Vec<_>>();
        for client in clients {
            self.publish_to_client(client, EventPayload::ServerStopping);
        }
    }

    pub(super) fn request_shutdown_after_blockers(self: &Arc<Self>) {
        if self.close_shutdown_blockers() {
            self.request_shutdown_with_hooks(self.shutdown_forced.load(Ordering::Acquire));
        }
    }

    pub(super) fn close_shutdown_blockers(&self) -> bool {
        let mut blockers = self.shutdown_blockers.lock();
        blockers.closed = true;
        blockers.active == 0
    }

    pub(super) fn shutdown_closed(&self) -> bool {
        self.stopping.load(Ordering::Acquire) || self.shutdown_blockers.lock().closed
    }

    pub(super) fn active_shutdown_blockers(&self) -> usize {
        self.shutdown_blockers.lock().active
    }

    pub(super) fn should_shutdown_if_empty(&self, state: &ServerState) -> bool {
        if !state.clients.values().all(|c| c.subscriber.is_none()) {
            return false;
        }
        let sessions_empty = state.engine.state.sessions.is_empty();
        let exit_empty = state.engine.exit_empty_explicit();
        let exit_unattached = state.engine.exit_unattached_explicit() == Some(true);
        if exit_empty.is_none() && !exit_unattached {
            return self.exit_empty_armed.load(Ordering::Acquire) && sessions_empty;
        }
        if exit_empty == Some(false) || !*self.startup_ready.lock() {
            return false;
        }
        exit_unattached || sessions_empty
    }
}

fn take_all_wait_wakes(
    channels: &mut BTreeMap<String, WaitChannel>,
) -> Vec<cmdq::WaitContinuation> {
    let mut wakes = Vec::new();
    for (_, mut channel) in std::mem::take(channels) {
        wakes.extend(channel.waiters.drain(..).map(|item| item.continuation));
        wakes.extend(channel.lockers.drain(..).map(|item| item.continuation));
        wakes.extend(channel.granted.take().map(|item| item.continuation));
    }
    wakes
}
