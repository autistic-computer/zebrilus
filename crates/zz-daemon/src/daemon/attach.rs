#[cfg(any(windows, test))]
use std::io::{self, IoSlice, Write};

use super::*;

pub(super) const MAX_BATCHED_WRITE_BYTES: usize = 256 * 1024;

#[cfg(windows)]
const INBOUND_BUFFER_BYTES: usize = 8 * 1024;

#[cfg(windows)]
pub(super) fn inbound_reader<S: io::Read>(stream: S) -> io::BufReader<S> {
    io::BufReader::with_capacity(INBOUND_BUFFER_BYTES, stream)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ResyncScope {
    Attach,
    Full,
}

impl ResyncScope {
    pub(super) fn sends_everything(self) -> bool {
        self == Self::Full
    }
}

pub(super) fn terminal_option_capabilities(engine: &MuxEngine) -> [String; 2] {
    [
        format!(
            "{SERVER_OPTION_CAPABILITY_PREFIX}extended-keys={}",
            engine.extended_keys()
        ),
        format!(
            "{SERVER_OPTION_CAPABILITY_PREFIX}focus-events={}",
            if engine.focus_events() { "on" } else { "off" }
        ),
    ]
}

pub(super) fn client_cell_fact(capabilities: &[String]) -> Option<(u32, u32)> {
    capabilities.iter().find_map(|capability| {
        let value = capability.strip_prefix(ClientHello::CLIENT_CELL_CAPABILITY_PREFIX)?;
        let (width, height) = value.split_once('x')?;
        let width = width.parse::<u32>().ok().filter(|width| *width > 0)?;
        let height = height.parse::<u32>().ok().filter(|height| *height > 0)?;
        Some((width, height))
    })
}

pub(super) fn client_exits_on_detach_fact(capabilities: &[String]) -> bool {
    capabilities
        .iter()
        .any(|capability| capability == zz_daemon_client::CLIENT_EXITS_ON_DETACH_CAPABILITY)
}

pub(super) fn presize_client_terminals(
    inner: &mut ServerState,
    client: ClientId,
    session: SessionId,
) -> BTreeSet<PaneId> {
    let mut seeded = BTreeSet::new();
    if inner.client(client).and_then(|c| c.kind) != Some(ClientKind::Interactive)
        || !inner.client(client).is_some_and(|c| c.has_terminal)
    {
        return seeded;
    }
    let Some((cell_width_px, cell_height_px)) = inner.client(client).and_then(|c| c.cell_pixels)
    else {
        return seeded;
    };
    let Some(session_state) = inner.engine.state.sessions.get(&session) else {
        return seeded;
    };
    let window = client_focused_window(inner, client, session_state);
    let Some((columns, rows)) = inner
        .client(client)
        .and_then(|c| c.size.as_ref())
        .and_then(|_| interactive_client_window_extent(inner, client, session, window))
    else {
        return seeded;
    };
    let extent = (columns, rows);
    let panes = inner
        .client(client)
        .and_then(|c| c.visible_terminals.as_ref())
        .into_iter()
        .flatten()
        .copied()
        .filter(|pane| inner.terminals.contains_key(pane))
        .filter(|pane| inner.engine.state.window_for_pane(*pane) == Some(window))
        .filter(|pane| {
            inner
                .panes
                .terminal_geometries
                .get(pane)
                .is_none_or(|geometries| !geometries.contains_key(&client))
        })
        .filter_map(|pane| {
            inner
                .engine
                .pane_geometry_at_window_extent(pane, columns, rows)
                .map(|geometry| (pane, geometry))
        })
        .collect::<Vec<_>>();
    for (pane, (columns, rows)) in panes {
        inner
            .panes
            .terminal_geometries
            .entry(pane)
            .or_default()
            .insert(
                client,
                TerminalGeometry {
                    columns,
                    rows,
                    cell_width_px,
                    cell_height_px,
                },
            );
        record_report_place(inner, client, pane, Some(extent));
        seeded.insert(pane);
    }
    seeded
}

pub(super) fn attach_frame_superseded(
    inner: &ServerState,
    pane: PaneId,
    terminal: &TerminalSession,
) -> bool {
    if inner.engine.state.pane(pane).is_none_or(|pane| pane.dead) {
        return false;
    }
    terminal_resize_for_pane(inner, pane).is_some_and(|(_, geometry)| {
        (geometry.columns.max(1), geometry.rows.max(1)) != terminal.size()
    })
}

#[cfg(any(windows, test))]
pub(super) fn write_frames(stream: &mut impl Write, frames: &[impl AsRef<[u8]>]) -> io::Result<()> {
    let mut slices = frames
        .iter()
        .map(|frame| IoSlice::new(frame.as_ref()))
        .collect::<Vec<_>>();
    let mut remaining = &mut slices[..];
    while !remaining.is_empty() {
        match stream.write_vectored(remaining) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write whole outbound batch",
                ));
            }
            Ok(written) => IoSlice::advance_slices(&mut remaining, written),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    stream.flush()
}

impl Shared {
    pub(super) fn hold_attach_terminals(&self, client: ClientId) -> AttachHold {
        let subscriber =
            self.read_client(client, |c| c.and_then(|c| c.subscriber.as_ref()).cloned());
        if let Some(subscriber) = &subscriber {
            subscriber.hold_terminals();
        }
        AttachHold(subscriber)
    }

    pub(super) fn request_full(&self, client: ClientId, pane: PaneId, outbound: &OutboundMailbox) {
        outbound.forget_delivered_terminal(pane);
        self.send_full(client, pane, outbound);
    }
}

pub(super) struct AttachHold(Option<Arc<OutboundMailbox>>);

impl AttachHold {
    pub(super) fn release(&mut self) {
        if let Some(held) = self.0.take() {
            held.release_terminals();
        }
    }
}

impl Drop for AttachHold {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(windows)]
pub(super) struct WriterThread(crossbeam_channel::Receiver<()>);

#[cfg(windows)]
impl WriterThread {
    pub(super) fn join(self) -> Result<(), ()> {
        self.0.recv().map_err(drop)
    }
}

#[cfg(windows)]
pub(super) fn spawn_writer(
    threads: &Arc<exec::ConnectionThreads>,
    write: impl FnOnce() + Send + 'static,
) -> std::io::Result<WriterThread> {
    let (done, finished) = crossbeam_channel::bounded(1);
    threads.run(Box::new(move || {
        write();
        let _ = done.send(());
    }))?;
    Ok(WriterThread(finished))
}

impl Shared {
    pub(super) fn enforce_destroy_unattached_if_changed(
        self: &Arc<Self>,
        before: Option<BTreeMap<SessionId, BTreeSet<ClientId>>>,
    ) {
        if let Some(before) = before
            && self.inner.lock().attachment.attached != before
        {
            self.enforce_destroy_unattached();
        }
    }

    pub(super) fn attach(
        self: &Arc<Self>,
        client: ClientId,
        session: SessionId,
    ) -> Result<MuxSnapshot, ServerError> {
        self.attach_with_event_hooks(client, session, true)
    }

    fn attach_with_event_hooks(
        self: &Arc<Self>,
        client: ClientId,
        session: SessionId,
        event_hooks_enabled: bool,
    ) -> Result<MuxSnapshot, ServerError> {
        let (snapshot, events) =
            self.attach_collect_event_hooks(client, session, event_hooks_enabled)?;
        self.enforce_destroy_unattached();
        self.run_event_hooks(events);
        Ok(snapshot)
    }

    pub(super) fn attach_collect_event_hooks(
        &self,
        client: ClientId,
        session: SessionId,
        event_hooks_enabled: bool,
    ) -> Result<(MuxSnapshot, Vec<PendingHookEvent>), ServerError> {
        let _wakes = zz_terminal::hold_actor_wakes();
        let mut inner = self.inner.lock();
        if !inner.engine.state.sessions.contains_key(&session)
            || inner.attachment.destroying_unattached.contains(&session)
        {
            return Err(ServerError::MissingTarget(session.to_string()));
        }
        let hook_state_before = event_hooks_enabled.then(|| {
            (
                hook_events::HookScope::open(&mut inner.engine),
                active_copy_mode_panes(&inner),
            )
        });
        let pane_focus_before =
            event_hooks_enabled.then(|| hook_events::FocusProbeScope::open(&mut inner));
        let previous_session = client_attached_session(&inner, client);
        let client_name = client_format_name(&inner, client);
        let previous_sessions = inner
            .attachment
            .attached
            .iter()
            .filter_map(|(attached_session, clients)| {
                (clients.contains(&client) && *attached_session != session)
                    .then_some(*attached_session)
            })
            .collect::<Vec<_>>();
        let switches_session = !previous_sessions.is_empty();
        let previous = previous_sessions
            .iter()
            .flat_map(|attached_session| session_terminals(&inner, *attached_session))
            .collect::<Vec<_>>();
        let command_output = switches_session
            .then(|| take_command_output(&mut inner, client))
            .flatten();
        let choose_tree_closed = switches_session
            && inner
                .client_mut(client)
                .and_then(|c| c.choose_tree.take())
                .is_some();
        let choose_buffer_closed = switches_session
            && inner
                .client_mut(client)
                .and_then(|c| c.choose_buffer.take())
                .is_some();
        let display_panes_closed =
            switches_session && take_display_panes(&mut inner, client).is_some();
        let previous_streamed = inner
            .client(client)
            .and_then(|c| c.streamed_terminals.as_ref())
            .map(|streamed| streamed.keys().copied().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        let mut affected_panes = inner
            .client(client)
            .and_then(|c| c.visible_terminals.as_ref())
            .cloned()
            .unwrap_or_default();
        affected_panes.extend(control_client_sized_panes(&inner, client));
        if switches_session {
            affected_panes.extend(remove_client_terminal_geometries(&mut inner, client));
            inner
                .client_mut(client)
                .and_then(|c| c.focused_window.take());
        }
        for (attached_session, clients) in &mut inner.attachment.attached {
            if *attached_session != session {
                clients.remove(&client);
            }
        }
        inner
            .attachment
            .attached
            .retain(|_, clients| !clients.is_empty());
        inner
            .attachment
            .attached
            .entry(session)
            .or_default()
            .insert(client);
        let _ = inner
            .client_mut(client)
            .is_some_and(|client| std::mem::take(&mut client.terminal_preview));
        if previous_session != Some(session)
            && let Some(previous_session) = previous_session
            && inner.engine.state.sessions.contains_key(&previous_session)
            && let Some(registered) = inner.client_mut(client)
        {
            registered.last_session = Some(previous_session);
        }
        let now = unix_timestamp();
        inner.attachment.activity_sequence = inner.attachment.activity_sequence.saturating_add(1);
        let activity = inner.attachment.activity_sequence;
        if let Some(registered) = inner.client_mut(client) {
            registered.detached = false;
            registered.activity = Some(activity);
            registered.activity_time = Some(now);
        }
        inner.attachment.session_last_attached.insert(session, now);
        if let Some(window) = client_focused_window_for_attachment(&inner, client) {
            inner
                .attachment
                .window_latest_clients
                .insert(window, client);
        }
        if let Some(feed) = inner
            .client(client)
            .and_then(|c| c.subscriber.as_ref())
            .and_then(|subscriber| subscriber.control_feed())
        {
            feed.clear();
        }
        inner.engine.mark_session_active_at(session, now);
        if let Some(active_window) = inner
            .engine
            .state
            .sessions
            .get(&session)
            .map(|state| state.active_window)
        {
            let panes = inner.engine.state.windows[&active_window]
                .panes
                .keys()
                .copied()
                .collect::<Vec<_>>();
            for pane in panes {
                inner.engine.state.clear_pane_alerts(pane);
            }
            inner
                .engine
                .state
                .set_window_activity_flag(active_window, false);
            inner
                .engine
                .state
                .set_window_silence_flag(active_window, false);
        }
        let visible = visible_terminal_panes(&inner, client, session);
        let streamed = streamed_terminal_panes(&inner, client, &visible);
        let next_streamed = streamed.keys().copied().collect::<BTreeSet<_>>();
        let removed_streamed = previous_streamed
            .difference(&next_streamed)
            .copied()
            .collect::<Vec<_>>();
        affected_panes.extend(visible.iter().copied());
        let previous_kinds = inner
            .client(client)
            .and_then(|c| c.streamed_terminals.as_ref())
            .cloned()
            .unwrap_or_default();
        inner
            .client_entry(client)
            .visible_terminals
            .replace(visible);
        inner
            .client_entry(client)
            .streamed_terminals
            .replace(streamed);
        mark_client_terminal_latest(&mut inner, client);
        affected_panes.extend(attach::presize_client_terminals(
            &mut inner, client, session,
        ));
        affected_panes.extend(control_client_sized_panes(&inner, client));
        #[cfg(feature = "agent")]
        {
            let visible = visible_agent_panes(&inner, client, session);
            inner.client_entry(client).visible_agents.replace(visible);
        }
        let terminals = session_terminals(&inner, session);
        let subscriber = inner
            .client(client)
            .and_then(|c| c.subscriber.as_ref())
            .cloned();
        let unfocused_copy_mode_exits = unfocused_copy_sessions(&mut inner);
        write_back_terminal_geometries(&mut inner, &affected_panes);
        apply_terminal_resizes(terminal_resizes_for_panes(&inner, &affected_panes));
        if let Some(streamed) = inner
            .client(client)
            .and_then(|c| c.streamed_terminals.as_ref())
        {
            apply_view_streams(&inner, TerminalViewId(client.0), &previous_kinds, streamed);
        }
        let mut snapshot = inner.engine.state.snapshot();
        let presence = snapshot_presence(&inner);
        stamp_snapshot_for_client(&inner, client, &mut snapshot, &presence);
        if let Some(registered) = inner.client_mut(client) {
            registered.focused = Some(true);
        }
        let mut hook_events = Vec::new();
        if let Some((hook_scope, copy_modes_before)) = hook_state_before {
            let pane_focus_before = pane_focus_before.map(|scope| scope.close(&inner));
            let mode_events = {
                let mut diff = hook_scope.finish(&inner.engine, "");
                hook_events.append(&mut diff.events);
                let copy_modes_after = active_copy_mode_panes(&inner);
                hook_events::pane_mode_hook_events(
                    &inner.engine,
                    &diff.before,
                    &copy_modes_before,
                    &copy_modes_after,
                )
            };
            if let Some(pane_focus_before) = pane_focus_before.as_ref() {
                let focus = pane_focus_hook_events(&mut inner, pane_focus_before);
                splice_pane_focus_events(&mut hook_events, 0, focus);
            }
            hook_events.extend(mode_events);
            if previous_session != Some(session) {
                let context = hook_events::live_session_context(&inner.engine.state, session);
                hook_events.push(
                    PendingHookEvent::client(
                        "client-session-changed",
                        context.clone(),
                        client,
                        Some(client_name.as_str()),
                    )
                    .with_session_change(
                        &inner.engine.state,
                        previous_session,
                        session,
                    ),
                );
                if previous_session.is_none() {
                    hook_events.push(PendingHookEvent::client(
                        "client-attached",
                        context,
                        client,
                        Some(client_name.as_str()),
                    ));
                }
            }
        }
        sync_control_routes(&mut inner, &self.control_wake);
        drop(inner);
        if let Some(subscriber) = subscriber {
            for pane in removed_streamed {
                subscriber.suspend_terminal(pane);
            }
        }
        if let Some(output) = command_output {
            self.retire_command_output(client, output);
        }
        if choose_tree_closed {
            self.publish_to_client(client, EventPayload::ChooseTree { state: None });
        }
        if choose_buffer_closed {
            self.publish_to_client(client, EventPayload::ChooseBuffer { state: None });
        }
        if display_panes_closed {
            self.publish_to_client(client, EventPayload::DisplayPanes { state: None });
        }
        let view = TerminalViewId(client.0);
        for (frozen_client, terminal) in unfocused_copy_mode_exits {
            terminal.view_action(
                TerminalViewId(frozen_client.0),
                zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Cancel),
            );
        }
        for terminal in previous {
            terminal.detach_view(view);
        }
        for terminal in terminals {
            terminal.attach_view(view);
        }
        self.nudge_client_timers();
        Ok((snapshot, hook_events))
    }

    pub(super) fn attach_target(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        target: &str,
    ) -> Result<(SessionId, MuxSnapshot), ServerError> {
        self.attach_target_with_materialization_observer(
            client,
            kind,
            context,
            target,
            || {},
            || {},
        )
    }

    pub(super) fn attach_target_with_materialization_observer<BeforeLock, BeforeCreate>(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        target: &str,
        before_lock: BeforeLock,
        before_create: BeforeCreate,
    ) -> Result<(SessionId, MuxSnapshot), ServerError>
    where
        BeforeLock: FnOnce(),
        BeforeCreate: FnOnce(),
    {
        if matches!(kind, ClientKind::Interactive | ClientKind::Control) && target.is_empty() {
            before_lock();
            let _default_attach = self.default_attach_effects.lock();
            if self.inner.lock().engine.state.sessions.is_empty() {
                before_create();
                let execution = match self.execute(
                    client,
                    ClientKind::Command,
                    context,
                    &CommandInvocation::new("new-session", ["-A", "-d"]),
                ) {
                    Ok(execution) => execution,
                    Err(error) => {
                        let error = daemon_server_error(error);
                        let materialized_elsewhere =
                            matches!(
                                &error,
                                ServerError::InvalidCommand(message)
                                    if message == "open terminal failed: not a terminal"
                            ) && !self.inner.lock().engine.state.sessions.is_empty();
                        if !materialized_elsewhere {
                            return Err(error);
                        }
                        Execution::default()
                    }
                };
                if !execution.output.is_empty() {
                    self.open_command_output(
                        client,
                        context.pane,
                        "new-session".to_owned(),
                        &execution.output,
                    )
                    .map_err(daemon_server_error)?;
                }
            }
        }
        let session = {
            let inner = self.inner.lock();
            if let Some(refusal) = nested_attach_refusal(&inner, client) {
                return Err(refusal);
            }
            inner.engine.resolve_session(
                (!target.is_empty()).then_some(target),
                inner
                    .engine
                    .state
                    .default_context()
                    .map(|context| context.0),
            )?
        };
        let (window, pane) = {
            let inner = self.inner.lock();
            let session_state = inner
                .engine
                .state
                .sessions
                .get(&session)
                .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?;
            let window = session_state.active_window;
            let pane = inner.engine.state.windows[&window].active_pane;
            (window, pane)
        };
        context.retarget(&ExecutionContext::new(
            Some(session),
            Some(window),
            Some(pane),
        ));
        {
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
        let snapshot = self.attach(client, session)?;
        Ok((session, snapshot))
    }

    pub(super) fn detach(self: &Arc<Self>, client: ClientId) {
        self.detach_with_event_hooks(client, true);
    }

    pub(super) fn detach_with_event_hooks(
        self: &Arc<Self>,
        client: ClientId,
        event_hooks_enabled: bool,
    ) {
        let (detached, events) = self.detach_client_state(client, event_hooks_enabled);
        if detached {
            self.publish_snapshot_after_detach(client);
        }
        self.enforce_destroy_unattached();
        self.run_event_hooks(events);
    }

    pub(super) fn enforce_destroy_unattached(self: &Arc<Self>) {
        let candidates = {
            let inner = self.inner.lock();
            if !inner.engine.destroy_unattached_explicit_anywhere() {
                return;
            }
            inner
                .engine
                .state
                .sessions
                .keys()
                .filter(|session| {
                    !inner.attachment.attached.contains_key(session)
                        && !inner.attachment.destroying_unattached.contains(session)
                })
                .filter(|session| {
                    matches!(
                        inner
                            .engine
                            .destroy_unattached_policy_for_session(**session)
                            .as_deref(),
                        Some("on" | "keep-group")
                    )
                })
                .copied()
                .collect::<Vec<_>>()
        };
        for session in candidates {
            {
                let mut inner = self.inner.lock();
                let eligible = inner.engine.state.sessions.contains_key(&session)
                    && !inner.attachment.attached.contains_key(&session);
                if !eligible {
                    continue;
                }
                inner.attachment.destroying_unattached.insert(session);
            }
            #[cfg(unix)]
            if self.loop_active.load(Ordering::Acquire) {
                let target = session.to_string();
                let command = CommandInvocation::new("kill-session", ["-t", target.as_str()]);
                self.enqueue_inserted_task(
                    ClientId(u64::MAX),
                    ClientKind::Command,
                    &ExecutionContext::default(),
                    &InsertedCommandSource::Commands(vec![command]),
                    "<destroy-unattached>",
                    None,
                    false,
                    None,
                    Box::new(move |shared, _, result| {
                        shared
                            .inner
                            .lock()
                            .attachment
                            .destroying_unattached
                            .remove(&session);
                        if let Err(error) = result {
                            log::warn!("destroy-unattached could not destroy {session}: {error}");
                        }
                    }),
                );
                continue;
            }
            #[cfg(test)]
            if let Some(hook) = self.destroy_unattached_hook.lock().take() {
                let _ = hook.reached.send(());
                let _ = hook.release.recv();
            }
            let target = session.to_string();
            let command = CommandInvocation::new("kill-session", ["-t", target.as_str()]);
            let mut context = ExecutionContext::default();
            let result = self.execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &command,
            );
            self.inner
                .lock()
                .attachment
                .destroying_unattached
                .remove(&session);
            if let Err(error) = result {
                log::warn!("destroy-unattached could not destroy {target}: {error}");
            }
        }
    }

    pub(super) fn evict_clients_with_event_hooks(
        self: &Arc<Self>,
        session: Option<SessionId>,
        stealer: ClientId,
        event_hooks_enabled: bool,
        action: &ClientExitAction,
    ) {
        let (victims, by) = {
            let inner = self.inner.lock();
            let victims = inner
                .attachment
                .attached
                .iter()
                .filter(|(attached, _)| session.is_none_or(|wanted| wanted == **attached))
                .flat_map(|(attached, clients)| {
                    clients.iter().map(move |client| (*attached, *client))
                })
                .filter(|(_, victim)| *victim != stealer)
                .collect::<Vec<_>>();
            let by = inner
                .client(stealer)
                .and_then(|c| c.name.as_ref())
                .cloned()
                .unwrap_or_else(|| format!("device-{}", stealer.0));
            (victims, by)
        };
        let evicted = !victims.is_empty();
        for (session, victim) in victims {
            self.publish_to_client(
                victim,
                EventPayload::detached_evicted_with_action(
                    session,
                    Some(by.clone()),
                    action.clone(),
                ),
            );
            self.detach_with_event_hooks(victim, event_hooks_enabled);
        }
        if evicted {
            self.publish_snapshot();
        }
    }

    pub(super) fn detach_client_state(
        &self,
        client: ClientId,
        event_hooks_enabled: bool,
    ) -> (bool, Vec<PendingHookEvent>) {
        let _wakes = zz_terminal::hold_actor_wakes();
        let mut inner = self.inner.lock();
        let sessions = inner
            .attachment
            .attached
            .iter()
            .filter_map(|(session, clients)| clients.contains(&client).then_some(*session))
            .collect::<Vec<_>>();
        let was_attached = !sessions.is_empty();
        let leaving = inner.client(client).is_some_and(|c| c.exits_on_detach);
        let event_hooks_enabled = event_hooks_enabled && !detach_is_inert(&inner, client);
        let hook_state_before = event_hooks_enabled.then(|| {
            (
                hook_events::HookScope::open(&mut inner.engine),
                active_copy_mode_panes(&inner),
            )
        });
        let pane_focus_before =
            event_hooks_enabled.then(|| hook_events::FocusProbeScope::open(&mut inner));
        let client_name =
            (event_hooks_enabled && was_attached).then(|| client_format_name(&inner, client));
        let mut terminals = sessions
            .iter()
            .flat_map(|session| session_terminals(&inner, *session))
            .collect::<Vec<_>>();
        let mut affected_panes = inner
            .client(client)
            .and_then(|c| c.visible_terminals.as_ref())
            .cloned()
            .unwrap_or_default();
        affected_panes.extend(control_client_sized_panes(&inner, client));
        affected_panes.extend(remove_client_terminal_geometries(&mut inner, client));
        for clients in inner.attachment.attached.values_mut() {
            clients.remove(&client);
        }
        inner
            .attachment
            .attached
            .retain(|_, clients| !clients.is_empty());
        let client_active_events =
            promote_window_latest_clients(&mut inner, client, event_hooks_enabled);
        if let Some(feed) = inner
            .client(client)
            .and_then(|c| c.subscriber.as_ref())
            .and_then(|subscriber| subscriber.control_feed())
        {
            feed.clear();
        }
        inner
            .client_mut(client)
            .and_then(|c| c.visible_terminals.take());
        let streamed = inner
            .client_mut(client)
            .and_then(|c| c.streamed_terminals.take())
            .map(|streamed| streamed.into_keys().collect::<Vec<_>>())
            .unwrap_or_default();
        for pane in &streamed {
            if let Some(terminal) = inner.terminals.get(pane) {
                terminal.set_view_stream(TerminalViewId(client.0), ViewStream::Off);
                sync_view_sink(&inner, terminal, client, *pane, None);
                if !terminals.iter().any(|known| Arc::ptr_eq(known, terminal)) {
                    terminals.push(Arc::clone(terminal));
                }
            }
        }
        let _ = inner
            .client_mut(client)
            .is_some_and(|client| std::mem::take(&mut client.terminal_preview));
        let subscriber = inner
            .client(client)
            .and_then(|c| c.subscriber.as_ref())
            .cloned();
        inner
            .client_mut(client)
            .and_then(|c| c.visible_agents.take());
        inner
            .client_mut(client)
            .and_then(|c| c.focused_window.take());
        inner
            .client_mut(client)
            .and_then(|c| c.terminal_input_sequence.take());
        inner.client_mut(client).and_then(|c| c.key_engine.take());
        if let Some(registered) = inner.client_mut(client) {
            registered.detached |= was_attached;
            registered.last_session = None;
            registered.activity = None;
            registered.activity_time = None;
        }
        let copy_session = inner
            .client_mut(client)
            .and_then(|c| c.copy_session.take())
            .and_then(|session| inner.terminals.get(&session.pane).cloned());
        inner
            .client_mut(client)
            .and_then(|c| c.swallowed_keys.take());
        inner
            .client_mut(client)
            .and_then(|c| c.suppressed_text.take());
        inner
            .client_mut(client)
            .and_then(|c| c.pending_committed_text.take());
        inner
            .client_mut(client)
            .and_then(|c| c.command_prompt.take());
        inner.client_mut(client).and_then(|c| c.choose_tree.take());
        inner
            .client_mut(client)
            .and_then(|c| c.choose_buffer.take());
        let _ = take_display_panes(&mut inner, client);
        if let Some(message) = take_client_message(&mut inner, client)
            && message.deadline.is_some()
        {
            let _ = self.timer_tx.send(timers::TimerInput::ClientMessage(
                ClientMessageDeadlineCommand::Cancel {
                    client,
                    token: message.token,
                },
            ));
        }
        let command_output = take_command_output(&mut inner, client);
        let menu = inner.client_mut(client).and_then(|c| c.menu.take());
        let confirm = inner.client_mut(client).and_then(|c| c.confirm.take());
        write_back_terminal_geometries(&mut inner, &affected_panes);
        let resizes = terminal_resizes_for_panes(&inner, &affected_panes);
        let mut events = Vec::new();
        if let Some((hook_scope, copy_modes_before)) = hook_state_before {
            let pane_focus_before = pane_focus_before.map(|scope| scope.close(&inner));
            let (mode_events, detached_context) = {
                let mut diff = hook_scope.finish(&inner.engine, "");
                events.append(&mut diff.events);
                let copy_modes_after = active_copy_mode_panes(&inner);
                (
                    hook_events::pane_mode_hook_events(
                        &inner.engine,
                        &diff.before,
                        &copy_modes_before,
                        &copy_modes_after,
                    ),
                    sessions
                        .first()
                        .map(|session| diff.before_session_context(*session)),
                )
            };
            if let Some(pane_focus_before) = pane_focus_before.as_ref() {
                let focus = pane_focus_hook_events(&mut inner, pane_focus_before);
                splice_pane_focus_events(&mut events, 0, focus);
            }
            events.extend(mode_events);
            events.extend(client_active_events);
            if let Some(context) = detached_context {
                events.push(PendingHookEvent::client(
                    "client-detached",
                    context,
                    client,
                    client_name.as_deref(),
                ));
            }
        }
        drop(inner);
        if was_attached {
            self.nudge_client_timers();
        }
        if let Some(subscriber) = subscriber {
            for pane in streamed {
                subscriber.suspend_terminal(pane);
            }
        }
        self.fail_gui_requests_for(client);
        let view = TerminalViewId(client.0);
        if let Some(command_output) = command_output {
            self.retire_command_output(client, command_output);
        }
        if menu.is_some() {
            self.publish_to_client(client, EventPayload::Menu { state: None });
        }
        if let Some(confirm) = confirm {
            if let ConfirmExecution::Blocking { waiter } = confirm.execution {
                waiter.complete(false);
            }
            self.publish_to_client(client, EventPayload::Confirm { state: None });
        }
        if let Some(terminal) = copy_session {
            terminal.view_action(
                view,
                zz_terminal::TerminalViewAction::CopyMode(zz_terminal::CopyModeAction::Cancel),
            );
        }
        for terminal in terminals {
            if leaving {
                terminal.release_view(view);
            } else {
                terminal.detach_view(view);
            }
        }
        apply_terminal_resizes(resizes);
        self.sync_key_table(client, false);
        (was_attached, events)
    }

    pub(super) fn detach_removed_sessions(self: &Arc<Self>) {
        let destroyed = {
            let mut inner = self.inner.lock();
            let mut destroyed = inner
                .engine
                .take_destroyed_sessions()
                .into_iter()
                .map(|(session, name, policy)| (session, (name, policy)))
                .collect::<BTreeMap<_, _>>();
            for session in inner
                .attachment
                .attached
                .keys()
                .copied()
                .filter(|session| !inner.engine.state.sessions.contains_key(session))
                .collect::<Vec<_>>()
            {
                destroyed
                    .entry(session)
                    .or_insert_with(|| (session.to_string(), "on".to_owned()));
            }
            destroyed
                .into_iter()
                .map(|(session, (name, policy))| {
                    for client in inner.clients.values_mut() {
                        if client.last_session == Some(session) {
                            client.last_session = None;
                        }
                    }
                    let survivor = destroyed_session_survivor(&inner, &name, &policy);
                    let fallback = match (survivor, policy.as_str()) {
                        (None, "on" | "no-detached") => newest_session_matching(&inner, |_| true),
                        _ => None,
                    };
                    let clients = inner
                        .attachment
                        .attached
                        .get(&session)
                        .into_iter()
                        .flatten()
                        .map(|client| {
                            let client_survivor = if survivor.is_none()
                                && inner
                                    .attachment
                                    .client_flags
                                    .get(*client)
                                    .no_detach_on_destroy
                            {
                                fallback
                            } else {
                                survivor
                            };
                            (*client, client_survivor)
                        })
                        .collect::<Vec<_>>();
                    inner.attachment.session_last_attached.remove(&session);
                    (session, clients)
                })
                .collect::<Vec<_>>()
        };
        for (session, clients) in destroyed {
            for (client, survivor) in clients {
                if let Some(survivor) = survivor {
                    let _held = self.hold_attach_terminals(client);
                    match self.attach_collect_event_hooks(client, survivor, true) {
                        Ok((snapshot, events)) => {
                            let mut inner = self.inner.lock();
                            if let Some(registered) = inner.client_mut(client) {
                                registered.last_session = None;
                            }
                            inner.waits.deferred_event_hooks.extend(events);
                            let outbound = inner
                                .client(client)
                                .and_then(|c| c.subscriber.as_ref())
                                .cloned();
                            drop(inner);
                            if let Some(outbound) = outbound {
                                self.send_attached(client, &outbound, survivor, snapshot);
                            }
                            continue;
                        }
                        Err(error) => {
                            log::warn!(
                                target: "zz_daemon::detach_on_destroy",
                                "client {client} could not move to survivor {survivor} after session {session} died: {error}"
                            );
                        }
                    }
                }
                self.publish_to_client(client, EventPayload::detached_session_destroyed(session));
                let _ = self.detach_client_state(client, false);
            }
        }
    }
}

fn newest_session_matching(
    inner: &ServerState,
    predicate: impl Fn(SessionId) -> bool,
) -> Option<SessionId> {
    inner
        .engine
        .state
        .sessions
        .values()
        .filter(|session| predicate(session.id))
        .max_by_key(|session| {
            (
                session.sort_activity,
                session.sort_created,
                session.name.clone(),
            )
        })
        .map(|session| session.id)
}

fn destroyed_session_survivor(
    inner: &ServerState,
    destroyed_name: &str,
    policy: &str,
) -> Option<SessionId> {
    match policy {
        "off" => newest_session_matching(inner, |_| true),
        "no-detached" => newest_session_matching(inner, |session| {
            inner
                .attachment
                .attached
                .get(&session)
                .is_none_or(BTreeSet::is_empty)
        }),
        "previous" => {
            let sessions = inner.engine.state.sessions_by_name();
            sessions
                .iter()
                .rev()
                .find(|session| session.name.as_str() < destroyed_name)
                .or_else(|| sessions.last())
                .map(|session| session.id)
        }
        "next" => {
            let sessions = inner.engine.state.sessions_by_name();
            sessions
                .iter()
                .find(|session| session.name.as_str() > destroyed_name)
                .or_else(|| sessions.first())
                .map(|session| session.id)
        }
        _ => None,
    }
}

fn promote_window_latest_clients(
    inner: &mut ServerState,
    detached: ClientId,
    event_hooks_enabled: bool,
) -> Vec<PendingHookEvent> {
    let windows = inner
        .attachment
        .window_latest_clients
        .iter()
        .filter_map(|(window, client)| (*client == detached).then_some(*window))
        .collect::<Vec<_>>();
    let mut events = Vec::new();
    for window in windows {
        let survivor = inner
            .engine
            .state
            .windows
            .get(&window)
            .and_then(|window_state| inner.attachment.attached.get(&window_state.session))
            .into_iter()
            .flatten()
            .filter(|client| client_focused_window_for_attachment(inner, **client) == Some(window))
            .copied()
            .max_by_key(|client| {
                (
                    inner
                        .client(*client)
                        .and_then(|c| c.activity)
                        .unwrap_or_default(),
                    Reverse(client.0),
                )
            });
        if let Some(survivor) = survivor {
            inner
                .attachment
                .window_latest_clients
                .insert(window, survivor);
            if event_hooks_enabled
                && let Some(event) =
                    client_hook_event_for_window(inner, "client-active", survivor, window)
            {
                events.push(event);
            }
        } else {
            inner.attachment.window_latest_clients.remove(&window);
        }
    }
    events
}

fn session_terminals(inner: &ServerState, session: SessionId) -> Vec<Arc<TerminalSession>> {
    inner
        .engine
        .state
        .sessions
        .get(&session)
        .into_iter()
        .flat_map(|session| session.windows.iter())
        .filter_map(|window| inner.engine.state.windows.get(window))
        .flat_map(|window| window.panes.keys())
        .filter_map(|pane| inner.terminals.get(pane).cloned())
        .collect()
}

fn remove_client_terminal_geometries(
    inner: &mut ServerState,
    client: ClientId,
) -> BTreeSet<PaneId> {
    let mut affected = BTreeSet::new();
    inner.panes.terminal_geometries.retain(|pane, geometries| {
        if geometries.remove(&client).is_some() {
            affected.insert(*pane);
        }
        !geometries.is_empty()
    });
    inner.panes.reported_pane_places.retain(|_, places| {
        places.remove(&client);
        !places.is_empty()
    });
    for pane in &affected {
        sync_view_area(inner, *pane, client);
    }
    affected
}
