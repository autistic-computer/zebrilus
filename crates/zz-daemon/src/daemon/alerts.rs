use super::*;

impl Shared {
    fn publish_window_alert_notifications(
        self: &Arc<Self>,
        notifications: Vec<WindowAlertNotification>,
    ) {
        for notification in notifications {
            for event in notification.events {
                self.publish_to_client(notification.client, event);
            }
            if let Some(message) = notification.message {
                self.publish_owned_client_message(message);
            }
            if let Some(retired) = notification.retired {
                self.retire_client_message(notification.client, retired, true);
            }
            if let Some(deadline) = notification.deadline {
                let _ = self.timer_tx.send(timers::TimerInput::ClientMessage(
                    ClientMessageDeadlineCommand::Schedule(deadline),
                ));
            }
        }
    }

    pub(super) fn raise_shell_marks(self: &Arc<Self>, pane: PaneId, terminal: &TerminalSession) {
        let marks = terminal.take_shell_marks();
        if marks.is_empty() {
            return;
        }
        let (events, summary_changed, finished) = {
            let mut inner = self.inner.lock();
            let mut events = Vec::new();
            let mut summary_changed = false;
            let mut finished = false;
            for mark in marks {
                summary_changed |= pane_summary::note_shell_mark(&mut inner, pane, &mark);
                finished |= mark.kind == zz_terminal::ShellMarkKind::CommandFinished;
                let name = match mark.kind {
                    zz_terminal::ShellMarkKind::Prompt => "pane-shell-prompt",
                    zz_terminal::ShellMarkKind::CommandStarted => "pane-command-started",
                    zz_terminal::ShellMarkKind::CommandFinished => "pane-command-finished",
                };
                if let Some(mut event) = pane_event_if_observed(&mut inner, name, pane) {
                    if mark.kind != zz_terminal::ShellMarkKind::Prompt {
                        add_command_payload(&inner, &mut event, pane, &mark.facts);
                    }
                    events.push(event);
                }
            }
            (events, summary_changed, finished)
        };
        if summary_changed {
            if let Some(effects) = &self.watcher_effects {
                effects
                    .lock()
                    .push(Box::new(Shared::publish_unpublished_tree));
            } else {
                self.request_publish(timers::PublishReason::Tree);
            }
        }
        if finished {
            self.request_pane_location(pane);
        }
        self.run_event_hooks(events);
    }

    pub(super) fn raise_pane_bell(self: &Arc<Self>, pane: PaneId) {
        let pane_hook = {
            let mut inner = self.inner.lock();
            pane_event_if_observed(&mut inner, "pane-bell", pane)
        };
        if let Some(hook) = pane_hook {
            self.run_event_hooks(vec![hook]);
        }
        let (snapshot_changed, hook, notifications) = {
            let mut inner = self.inner.lock();
            let Some(window) = inner.engine.state.window_for_pane(pane) else {
                return;
            };
            let Some(window_state) = inner.engine.state.windows.get(&window) else {
                return;
            };
            let session = window_state.session;
            let window_index = window_state.index;
            if inner.engine.monitor_bell_for_window(window) {
                let current = inner.engine.state.sessions[&session].active_window == window;
                let snapshot_changed = !pane_summary::pane_window_watched(&inner, pane)
                    && inner.engine.state.set_pane_bell(pane, true);
                if snapshot_changed {
                    pane_summary::stamp_pane(&mut inner, pane, unix_timestamp());
                }
                let action = inner.engine.bell_action_for_session(session);
                let visual = inner.engine.visual_bell_for_session(session);
                let (hook, notifications) = window_alert_notifications(
                    &mut inner,
                    WindowAlert {
                        session,
                        pane,
                        window_index,
                        label: "Bell",
                        hook_name: "alert-bell",
                        action,
                        visual,
                        session_current: current,
                    },
                );
                (snapshot_changed, hook, notifications)
            } else {
                (false, None, Vec::new())
            }
        };
        if snapshot_changed {
            self.publish_snapshot();
        }
        self.publish_window_alert_notifications(notifications);
        if let Some(hook) = hook {
            self.run_event_hooks(vec![hook]);
        }
    }

    pub(super) fn raise_window_activity(self: &Arc<Self>, window: WindowId, pane: PaneId) {
        let (snapshot_changed, hook, notifications) = {
            let mut inner = self.inner.lock();
            let pending = inner.panes.control_activity_pending.remove(&pane);
            if !inner.engine.monitor_activity_for_window(window) {
                return;
            }
            let Some(window_state) = inner.engine.state.windows.get(&window) else {
                return;
            };
            if window_state.activity_flag && !pending {
                return;
            }
            let session = window_state.session;
            let window_index = window_state.index;
            let Some(session_state) = inner.engine.state.sessions.get(&session) else {
                return;
            };
            let current = session_state.active_window == window;
            let attached = inner
                .attachment
                .attached
                .get(&session)
                .is_some_and(|clients| !clients.is_empty());
            let suppressed = current && attached;
            let snapshot_changed = if suppressed || pending {
                false
            } else {
                inner.engine.state.set_window_activity_flag(window, true)
            };
            let action = inner.engine.activity_action_for_session(session);
            let visual = inner.engine.visual_activity_for_session(session);
            let (hook, notifications) = window_alert_notifications(
                &mut inner,
                WindowAlert {
                    session,
                    pane,
                    window_index,
                    label: "Activity",
                    hook_name: "alert-activity",
                    action,
                    visual,
                    session_current: current,
                },
            );
            (snapshot_changed, hook, notifications)
        };
        if snapshot_changed {
            self.publish_snapshot();
        }
        self.publish_window_alert_notifications(notifications);
        if let Some(hook) = hook {
            self.run_event_hooks(vec![hook]);
        }
    }

    pub(super) fn expire_window_silence(
        self: &Arc<Self>,
        scheduled: SilenceDeadline,
        now: Instant,
    ) {
        let (snapshot_changed, hook, notifications, rearm) = {
            let mut inner = self.inner.lock();
            let due = inner
                .panes
                .silence_deadlines
                .get(&scheduled.window)
                .is_some_and(|current| *current == scheduled && scheduled.deadline <= now);
            if !due {
                return;
            }
            inner.panes.silence_deadlines.remove(&scheduled.window);
            let seconds = inner.engine.monitor_silence_for_window(scheduled.window);
            if seconds == 0 {
                return;
            }
            if let Some(quiet_until) = window_last_output(&inner, scheduled.window)
                .map(|last| last + Duration::from_secs(u64::from(seconds)))
                .filter(|quiet_until| *quiet_until > now)
            {
                let rearm = arm_silence_deadline_at(&mut inner, scheduled.window, quiet_until);
                drop(inner);
                let _ = self.timer_tx.send(timers::TimerInput::Silence(
                    SilenceDeadlineCommand::Schedule(rearm),
                ));
                return;
            }
            let Some(window_state) = inner.engine.state.windows.get(&scheduled.window) else {
                return;
            };
            let session = window_state.session;
            let window_index = window_state.index;
            let pane = window_state.active_pane;
            let latched = window_state.silence_flag;
            let rearm = arm_silence_deadline(&mut inner, scheduled.window, seconds);
            if latched {
                (false, None, Vec::new(), Some(rearm))
            } else {
                let Some(session_state) = inner.engine.state.sessions.get(&session) else {
                    return;
                };
                let current = session_state.active_window == scheduled.window;
                let attached = inner
                    .attachment
                    .attached
                    .get(&session)
                    .is_some_and(|clients| !clients.is_empty());
                let suppressed = current && attached;
                let snapshot_changed = if suppressed {
                    false
                } else {
                    inner
                        .engine
                        .state
                        .set_window_silence_flag(scheduled.window, true)
                };
                let action = inner.engine.silence_action_for_session(session);
                let visual = inner.engine.visual_silence_for_session(session);
                let (hook, notifications) = window_alert_notifications(
                    &mut inner,
                    WindowAlert {
                        session,
                        pane,
                        window_index,
                        label: "Silence",
                        hook_name: "alert-silence",
                        action,
                        visual,
                        session_current: current,
                    },
                );
                (snapshot_changed, hook, notifications, Some(rearm))
            }
        };
        if let Some(rearm) = rearm {
            let _ = self.timer_tx.send(timers::TimerInput::Silence(
                SilenceDeadlineCommand::Schedule(rearm),
            ));
        }
        if snapshot_changed {
            self.publish_snapshot();
        }
        self.publish_window_alert_notifications(notifications);
        if let Some(hook) = hook {
            self.enqueue_event_hooks(vec![hook]);
        }
    }

    pub(super) fn reset_all_silence_timers(&self) {
        let (schedules, cancels) = {
            let mut inner = self.inner.lock();
            let windows = inner
                .engine
                .state
                .windows
                .keys()
                .copied()
                .collect::<Vec<_>>();
            let mut schedules = Vec::new();
            let mut cancels = inner
                .panes
                .silence_deadlines
                .keys()
                .filter(|window| !inner.engine.state.windows.contains_key(window))
                .copied()
                .collect::<Vec<_>>()
                .into_iter()
                .filter_map(|window| inner.panes.silence_deadlines.remove(&window))
                .collect::<Vec<_>>();
            for window in windows {
                let seconds = inner.engine.monitor_silence_for_window(window);
                if seconds == 0 {
                    if let Some(deadline) = inner.panes.silence_deadlines.remove(&window) {
                        cancels.push(deadline);
                    }
                } else {
                    schedules.push(arm_silence_deadline(&mut inner, window, seconds));
                }
            }
            (schedules, cancels)
        };
        for deadline in cancels {
            let _ = self.timer_tx.send(timers::TimerInput::Silence(
                SilenceDeadlineCommand::Cancel {
                    window: deadline.window,
                    token: deadline.token,
                },
            ));
        }
        for deadline in schedules {
            let _ = self.timer_tx.send(timers::TimerInput::Silence(
                SilenceDeadlineCommand::Schedule(deadline),
            ));
        }
    }

    pub(super) fn clear_pane_alerts(&self, pane: PaneId) -> bool {
        self.inner.lock().engine.state.clear_pane_alerts(pane)
    }
}

#[derive(Clone, Copy)]
struct WindowAlert {
    session: SessionId,
    pane: PaneId,
    window_index: u32,
    label: &'static str,
    hook_name: &'static str,
    action: zz_mux::BellAction,
    visual: zz_mux::VisualBell,
    session_current: bool,
}

struct WindowAlertNotification {
    client: ClientId,
    events: Vec<EventPayload>,
    message: Option<OwnedClientMessagePublication>,
    retired: Option<ActiveClientMessage>,
    deadline: Option<ClientMessageDeadline>,
}

fn add_command_payload(
    inner: &ServerState,
    event: &mut PendingHookEvent,
    pane: PaneId,
    facts: &zz_terminal::PaneOutputFacts,
) {
    if let Some(window) = inner
        .engine
        .state
        .window_for_pane(pane)
        .and_then(|window| inner.engine.state.windows.get(&window))
    {
        add_winlink_payload(inner, event, window.session, window.index);
    }
    if let Some(status) = facts.command_status {
        event
            .variables
            .insert("hook_command_status".to_owned(), status.to_string());
    }
    if facts.command_start_time != 0 {
        event.variables.insert(
            "hook_command_start_time".to_owned(),
            facts.command_start_time.to_string(),
        );
    }
    if facts.command_end_time != 0 {
        event.variables.insert(
            "hook_command_end_time".to_owned(),
            facts.command_end_time.to_string(),
        );
    }
    if let Some(duration) = facts.command_duration(unix_timestamp()) {
        event
            .variables
            .insert("hook_command_duration".to_owned(), duration.to_string());
    }
}

fn add_winlink_payload(
    inner: &ServerState,
    event: &mut PendingHookEvent,
    session: SessionId,
    index: u32,
) {
    event
        .variables
        .insert(HOOK_SESSION_CONTEXT_FORMAT.to_owned(), session.to_string());
    if let Some(state) = inner.engine.state.sessions.get(&session) {
        event.variables.insert(
            HOOK_SESSION_NAME_CONTEXT_FORMAT.to_owned(),
            state.name.clone(),
        );
    }
    event
        .variables
        .insert("hook_window_index".to_owned(), index.to_string());
}

fn window_alert_notifications(
    inner: &mut ServerState,
    alert: WindowAlert,
) -> (Option<PendingHookEvent>, Vec<WindowAlertNotification>) {
    let WindowAlert {
        session,
        pane,
        window_index,
        label,
        hook_name,
        action,
        visual,
        session_current,
    } = alert;
    let applies = action.applies(session_current);
    let hook = applies
        .then(|| PendingHookEvent::live_pane(hook_name, pane, &inner.engine))
        .flatten()
        .map(|mut event| {
            add_winlink_payload(inner, &mut event, session, window_index);
            event.variables.remove(HOOK_PANE_CONTEXT_FORMAT);
            event
        });
    let duration_ms = inner.engine.display_time_for_session(session);
    let clients = if applies {
        inner
            .attachment
            .attached
            .get(&session)
            .into_iter()
            .flatten()
            .filter(|client| {
                inner.client(**client).and_then(|c| c.kind) == Some(ClientKind::Interactive)
                    && inner
                        .client(**client)
                        .is_some_and(|c| c.subscriber.is_some())
            })
            .copied()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let notifications = clients
        .into_iter()
        .map(|client| {
            let mut events = Vec::with_capacity(2);
            if visual.rings() {
                events.push(EventPayload::Bell { pane });
            }
            let (message, retired, deadline) = if visual.shows_message() {
                let text = if session_current {
                    format!("{label} in current window")
                } else {
                    format!("{label} in window {window_index}")
                };
                let message_id = next_timed_message_id(inner);
                let client_name = server_log_client_name(inner, client);
                push_server_message(inner, format!("{client_name} message: {text}"));
                if duration_ms != 0 {
                    let _ = inner
                        .client_mut(client)
                        .is_some_and(|client| std::mem::take(&mut client.message_ignore_keys));
                }
                let (retired, deadline) =
                    arm_client_message(inner, client, message_id, duration_ms, true);
                let message = OwnedClientMessagePublication {
                    client,
                    pane: Some(pane),
                    kind: ClientMessageKind::Info,
                    text,
                    duration_ms,
                    message_id,
                };
                (Some(message), retired, deadline)
            } else {
                (None, None, None)
            };
            WindowAlertNotification {
                client,
                events,
                message,
                retired,
                deadline,
            }
        })
        .collect();
    (hook, notifications)
}

fn window_last_output(inner: &ServerState, window: WindowId) -> Option<Instant> {
    inner
        .engine
        .state
        .windows
        .get(&window)?
        .panes
        .keys()
        .filter_map(|pane| inner.terminals.get(pane)?.last_output())
        .max()
}
