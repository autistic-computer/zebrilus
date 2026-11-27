use super::*;

impl StatusRenderer {
    pub(crate) fn job_client(&self) -> crate::daemon::status_jobs::StatusClient {
        self.job_client.clone()
    }

    pub(crate) fn set_job_waker(&mut self, waker: std::task::Waker) {
        #[cfg(windows)]
        self.job_client.set_notification(waker.clone());
        self.job_waker = Some(waker);
    }

    pub(crate) fn request_mode_refresh(&mut self, clients: BTreeSet<ClientId>) {
        self.pending_modes.extend(clients);
        if let Some(waker) = &self.job_waker {
            waker.wake_by_ref();
        }
    }

    pub(crate) fn take_pending_modes(&mut self) -> BTreeSet<ClientId> {
        std::mem::take(&mut self.pending_modes)
    }

    pub(crate) fn republish_modes(
        &mut self,
        client: ClientId,
        modes: Vec<zz_protocol::ModePresentation>,
    ) -> Option<StatusLine> {
        let published = self.published.get_mut(&client)?;
        if published.modes == modes {
            return None;
        }
        if self
            .completed
            .as_ref()
            .is_some_and(|entry| entry.client == client)
        {
            self.completed = None;
        }
        Arc::make_mut(published).modes = modes;
        Some(published.as_ref().clone())
    }

    pub(crate) fn job_summaries(&self) -> Vec<String> {
        let mut jobs = self
            .shell_cache
            .values()
            .filter_map(|entry| {
                let job = entry.job.as_ref()?;
                Some((
                    job.serial,
                    entry.expanded.as_deref().unwrap_or_default(),
                    job.fd,
                    job.pid,
                ))
            })
            .collect::<Vec<_>>();
        jobs.sort_by_key(|(serial, ..)| std::cmp::Reverse(*serial));
        jobs.iter()
            .enumerate()
            .map(|(number, (_, command, fd, pid))| {
                format!("Job {number}: {command} [fd={fd}, pid={pid}, status=0]")
            })
            .collect()
    }

    pub(crate) fn job_needs(&self) -> StatusJobNeeds {
        Arc::clone(&self.job_needs)
    }

    pub(crate) fn poll_jobs(&mut self) -> BTreeSet<ClientId> {
        let now = shell_second();
        self.shell_cache
            .retain(|_, entry| now.saturating_sub(entry.last) < 3600);
        let mut changed = std::mem::take(&mut self.uncovered_jobs);
        for ((client, _, command), entry) in &mut self.shell_cache {
            if entry.poll(command) {
                changed.insert(*client);
            }
        }
        for client in &changed {
            let needs = self
                .shell_cache
                .iter()
                .filter(|((cached, _, _), _)| cached == client)
                .fold(FormatNeeds::NONE, |needs, (_, entry)| {
                    needs | entry.output_needs
                });
            let mut job_needs = self.job_needs.lock();
            if needs.is_empty() {
                job_needs.remove(client);
            } else {
                job_needs.insert(*client, needs);
            }
        }
        changed
    }

    #[cfg(test)]
    pub(crate) fn render_forced(&mut self, request: &StatusRequest) -> StatusLine {
        self.render_forced_at(request, format_second())
    }

    pub(crate) fn render_forced_shared(&mut self, request: &Arc<StatusRequest>) -> StatusLine {
        self.render_forced_shared_at(request, format_second())
    }

    pub(super) fn render_forced_shared_at(
        &mut self,
        request: &Arc<StatusRequest>,
        now: i64,
    ) -> StatusLine {
        self.render_forced_with_identity(request, Some(request), now)
    }

    #[cfg(test)]
    pub(super) fn render_forced_at(&mut self, request: &StatusRequest, now: i64) -> StatusLine {
        self.render_forced_with_identity(request, None, now)
    }

    fn render_forced_with_identity(
        &mut self,
        request: &StatusRequest,
        identity: Option<&Arc<StatusRequest>>,
        now: i64,
    ) -> StatusLine {
        let status = self
            .cached_request_output(identity, now)
            .unwrap_or_else(|| {
                let mut touched = BTreeSet::new();
                let status = self.render_request(request, &mut touched, true, now, identity);
                if !touched.is_empty() {
                    self.note_uncovered_jobs(request, &touched);
                }
                status
            });
        if let Some(published) = self.published.get_mut(&request.client) {
            if !Arc::ptr_eq(published, &status) {
                *published = Arc::clone(&status);
            }
        } else {
            self.published.insert(request.client, Arc::clone(&status));
        }
        status.as_ref().clone()
    }

    fn cached_request_output(
        &self,
        identity: Option<&Arc<StatusRequest>>,
        now: i64,
    ) -> Option<Arc<StatusLine>> {
        let identity = identity?;
        self.completed
            .as_ref()
            .filter(|completed| {
                completed.now == now
                    && completed.request_identity.as_ptr() == Arc::as_ptr(identity)
                    && completed.cycle_tick.is_none_or(|tick| tick == cycle_tick())
            })
            .map(|completed| Arc::clone(&completed.status))
    }

    pub(super) fn render_request(
        &mut self,
        request: &StatusRequest,
        touched: &mut BTreeSet<ShellCacheKey>,
        refresh: bool,
        now: i64,
        identity: Option<&Arc<StatusRequest>>,
    ) -> Arc<StatusLine> {
        if self
            .completed
            .as_ref()
            .and_then(|completed| completed.cycle_tick)
            .is_some_and(|tick| tick != cycle_tick())
        {
            self.completed = None;
        }
        if let Some(status) = self.cached_request_output(identity, now) {
            return status;
        }
        let mut callback_names = None;
        let mut parts = None;
        if request.modes.is_empty()
            && let Some(completed) = &mut self.completed
            && completed.same_templates(request)
        {
            let fresh_callbacks = (completed.fact_identity.as_ptr() != Arc::as_ptr(&request.facts))
                .then(|| status_callback_values(request, &completed.callback_names));
            let callbacks = fresh_callbacks.as_deref().unwrap_or(&completed.callbacks);
            if completed.matches(request, callbacks, now, true) {
                if let Some(identity) = identity {
                    completed.request_identity = Arc::downgrade(identity);
                }
                return Arc::clone(&completed.status);
            }
            if status_parts_enabled() && completed.matches(request, callbacks, now, false) {
                parts.clone_from(&completed.parts);
            }
            if completed.same_lookup_scope(request) {
                callback_names = Some(completed.callback_names.clone());
            }
        }
        let callback_names = callback_names.or_else(|| status_cache_callbacks(request));
        if status_parts_enabled()
            && parts.is_none()
            && callback_names.is_some()
            && request.context.same_detached_data(&request.context)
        {
            parts = Some(Arc::new(StatusParts::new(request)));
        }
        let option_bytes = callback_names.as_ref().map_or(0, |_| {
            self.completed
                .as_ref()
                .filter(|completed| {
                    Arc::ptr_eq(&completed.option_snapshot, &request.option_snapshot)
                })
                .map_or_else(
                    || request.option_snapshot.retained_bytes(),
                    |completed| completed.option_bytes,
                )
        });
        self.owned_clients.insert(request.client);
        self.owned_client_high_watermark = Some(
            self.owned_client_high_watermark
                .map_or(request.client, |highest| highest.max(request.client)),
        );
        #[cfg(test)]
        {
            self.expansions += 1;
        }
        CYCLE_ANIMATED.set(false);
        let status = Arc::new(render(
            &mut self.shell_cache,
            touched,
            request,
            refresh,
            now,
            self.tmux_shim.as_deref(),
            self.zz_executable.as_deref(),
            self.job_waker.as_ref(),
            &self.job_client,
            parts.as_deref(),
        ));
        let animated = CYCLE_ANIMATED.replace(false);
        if animated {
            self.animation_started |= self.animating.insert(request.client);
        } else {
            self.animating.remove(&request.client);
        }
        let context_bytes = callback_names.as_ref().map_or(0, |_| {
            self.completed
                .as_ref()
                .filter(|completed| {
                    completed.context.same_detached_data(&request.context)
                        && completed.context.session_id.capacity()
                            == request.context.session_id.capacity()
                        && completed.context.window_id.capacity()
                            == request.context.window_id.capacity()
                        && completed.context.pane_id.capacity()
                            == request.context.pane_id.capacity()
                })
                .map_or_else(
                    || request.context.retained_bytes(),
                    |completed| completed.context_bytes,
                )
        });
        self.completed = callback_names.and_then(|names| {
            let callbacks = status_callback_values(request, &names);
            (completed_status_bytes_with_capture_bytes(
                request,
                &names,
                &callbacks,
                &status,
                option_bytes,
                context_bytes,
            )
            .saturating_add(
                parts
                    .as_ref()
                    .map_or(0, |parts| parts.retained_bytes().saturating_mul(2)),
            ) <= COMPLETED_STATUS_MAX_BYTES)
                .then(|| {
                    let mut completed = CompletedStatus::new(
                        request,
                        names,
                        callbacks,
                        now,
                        Arc::clone(&status),
                        identity,
                        option_bytes,
                        parts,
                        context_bytes,
                    );
                    completed.cycle_tick = animated.then(cycle_tick);
                    completed
                })
        });
        status
    }

    pub(crate) fn animating_clients(&self) -> BTreeSet<ClientId> {
        self.animating.clone()
    }

    pub(crate) fn take_animation_started(&mut self) -> bool {
        std::mem::take(&mut self.animation_started)
    }

    fn note_uncovered_jobs(
        &mut self,
        request: &StatusRequest,
        touched: &BTreeSet<ShellCacheKey>,
    ) -> bool {
        let uncovered = touched
            .iter()
            .filter(|(client, _, _)| *client == request.client)
            .filter_map(|key| self.shell_cache.get(key))
            .any(|entry| !request.context.format_universe_covers(entry.output_needs));
        if uncovered {
            self.uncovered_jobs.insert(request.client);
            if let Some(waker) = &self.job_waker {
                waker.wake_by_ref();
            }
        }
        uncovered
    }

    pub(crate) fn render_changed(
        &mut self,
        requests: &[StatusRequest],
    ) -> Vec<(ClientId, StatusLine)> {
        let mut touched = BTreeSet::new();
        let mut changed = Vec::new();
        for request in requests {
            let status = self.render_request(request, &mut touched, false, format_second(), None);
            if self.note_uncovered_jobs(request, &touched)
                || self.published.get(&request.client) == Some(&status)
            {
                continue;
            }
            self.published.insert(request.client, Arc::clone(&status));
            changed.push((request.client, status.as_ref().clone()));
        }
        changed
    }

    pub(crate) fn render_initial(&mut self, request: &StatusRequest) -> StatusLine {
        let mut touched = BTreeSet::new();
        let status = self.render_request(request, &mut touched, false, format_second(), None);
        self.note_uncovered_jobs(request, &touched);
        self.published.insert(request.client, Arc::clone(&status));
        status.as_ref().clone()
    }

    pub(crate) fn forget(&mut self, client: ClientId) {
        if self
            .owned_client_high_watermark
            .is_none_or(|highest| client > highest)
            || !self.owned_clients.remove(&client)
        {
            return;
        }
        self.published.remove(&client);
        if self
            .completed
            .as_ref()
            .is_some_and(|entry| entry.client == client)
        {
            self.completed = None;
        }
        self.shell_cache
            .retain(|(cached, _, _), _| *cached != client);
        self.job_needs.lock().remove(&client);
        self.uncovered_jobs.remove(&client);
        self.animating.remove(&client);
    }

    pub(crate) fn set_tmux_shim(&mut self, directory: PathBuf, executable: PathBuf) {
        self.tmux_shim = Some(directory);
        self.zz_executable = Some(executable);
    }
}

pub(crate) fn status_context<'e>(
    snapshot: &MuxSnapshot,
    engine: &'e MuxEngine,
    attached: Option<SessionId>,
    focused_window: Option<WindowId>,
) -> StatusContext<'e> {
    let mut context = base_status_context(engine, attached, focused_window);
    let Some(session) = snapshot
        .sessions
        .iter()
        .find(|session| Some(session.id) == attached)
    else {
        return context;
    };
    let focused_window = focused_window
        .filter(|focused| session.windows.iter().any(|window| window.id == *focused))
        .unwrap_or(session.active_window);
    set_status_context_active(&mut context);
    context.set_format_value("session_attached", session.viewers.len().to_string());
    context.set_format_value(
        "session_many_attached",
        if session.viewers.len() > 1 { "1" } else { "0" },
    );
    context.set_format_value(
        "session_attached_list",
        session
            .viewers
            .iter()
            .map(|viewer| viewer.name.as_str())
            .collect::<Vec<_>>()
            .join(","),
    );
    let active_viewers = session
        .viewers
        .iter()
        .filter(|viewer| viewer.window == focused_window)
        .collect::<Vec<_>>();
    context.set_format_value("window_active_clients", active_viewers.len().to_string());
    context.set_format_value(
        "window_active_clients_list",
        active_viewers
            .iter()
            .map(|viewer| viewer.name.as_str())
            .collect::<Vec<_>>()
            .join(","),
    );
    context
}

pub(crate) fn live_status_context(
    engine: &MuxEngine,
    attached: Option<SessionId>,
    focused_window: Option<WindowId>,
) -> StatusContext<'_> {
    let mut context = base_status_context(engine, attached, focused_window);
    if attached.is_some_and(|session| engine.state.sessions.contains_key(&session)) {
        set_status_context_active(&mut context);
        for (name, value) in [
            ("session_attached", "0"),
            ("session_many_attached", "0"),
            ("session_attached_list", ""),
            ("window_active_clients", "0"),
            ("window_active_clients_list", ""),
        ] {
            context.set_format_value(name, value);
        }
    }
    context
}

fn base_status_context(
    engine: &MuxEngine,
    attached: Option<SessionId>,
    focused_window: Option<WindowId>,
) -> StatusContext<'_> {
    let mut context = attached.map_or_else(
        || engine.format_status_context(None, focused_window, None),
        |client_session| {
            engine.format_status_context_for_client(
                Some(client_session),
                focused_window,
                None,
                client_session,
            )
        },
    );
    if context.variable("host").is_none_or(|host| host.is_empty()) {
        let (host, host_short) = host_names();
        context.set_format_value("host", host.clone());
        context.set_format_value("host_short", host_short.clone());
    }
    context
}

fn set_status_context_active(context: &mut StatusContext<'_>) {
    for name in ["window_active", "pane_active"] {
        if context
            .variable(name)
            .is_some_and(|value| !value.is_empty())
        {
            context.set_format_value(name, "1");
        }
    }
}

pub(crate) fn host_names() -> &'static (String, String) {
    static HOST: OnceLock<(String, String)> = OnceLock::new();
    HOST.get_or_init(|| {
        let host = zz_daemon_client::process_info::host_name()
            .map(|host| host.trim().to_owned())
            .filter(|host| !host.is_empty())
            .unwrap_or_else(|| "localhost".to_owned());
        let short = host
            .split('.')
            .next()
            .filter(|short| !short.is_empty())
            .unwrap_or(host.as_str())
            .to_owned();
        (host, short)
    })
}

pub(super) fn render(
    cache: &mut BTreeMap<ShellCacheKey, ShellCacheEntry>,
    touched: &mut BTreeSet<ShellCacheKey>,
    request: &StatusRequest,
    refresh: bool,
    now: i64,
    tmux_shim: Option<&std::path::Path>,
    zz_executable: Option<&std::path::Path>,
    job_waker: Option<&std::task::Waker>,
    job_client: &crate::daemon::status_jobs::StatusClient,
    parts: Option<&StatusParts>,
) -> StatusLine {
    let title = request
        .title_format
        .as_ref()
        .as_ref()
        .map_or_else(String::new, |format| {
            let mut hooks = DaemonFormatHooks::status(
                request.client,
                request.facts.as_ref(),
                &request.context,
                Some(&request.option_snapshot),
                cache,
                touched,
                refresh,
                now,
                &request.environment,
                &request.default_terminal,
                request.startup,
                tmux_shim,
                zz_executable,
                job_waker,
                job_client,
            );
            clamp_status_text(expand_status(format, &request.context, &mut hooks))
        });
    let theme = {
        let mut hooks = DaemonFormatHooks::status(
            request.client,
            request.facts.as_ref(),
            &request.context,
            Some(&request.option_snapshot),
            cache,
            touched,
            refresh,
            now,
            &request.environment,
            &request.default_terminal,
            request.startup,
            tmux_shim,
            zz_executable,
            job_waker,
            job_client,
        );
        if let Some(theme) = parts.and_then(|parts| parts.theme.as_ref()) {
            *theme.get_or_init(|| resolve_theme_colours(request, &mut hooks))
        } else {
            resolve_theme_colours(request, &mut hooks)
        }
    };
    let (message_style, message_command_style) = {
        let mut hooks = DaemonFormatHooks::status(
            request.client,
            request.facts.as_ref(),
            &request.context,
            Some(&request.option_snapshot),
            cache,
            touched,
            refresh,
            now,
            &request.environment,
            &request.default_terminal,
            request.startup,
            tmux_shim,
            zz_executable,
            job_waker,
            job_client,
        );
        let mut expand = || {
            (
                expand_style(&request.message_styles.0, &request.context, &mut hooks),
                expand_style(&request.message_styles.1, &request.context, &mut hooks),
            )
        };
        if let Some(styles) = parts.and_then(|parts| parts.message_styles.as_ref()) {
            styles.get_or_init(&mut expand).clone()
        } else {
            expand()
        }
    };
    let modes = request
        .modes
        .iter()
        .take(zz_protocol::MAX_MODE_PRESENTATIONS)
        .map(|mode| {
            let hooks = DaemonFormatHooks::status(
                request.client,
                request.facts.as_ref(),
                &mode.context,
                Some(&request.option_snapshot),
                cache,
                touched,
                refresh,
                now,
                &request.environment,
                &request.default_terminal,
                request.startup,
                tmux_shim,
                zz_executable,
                job_waker,
                job_client,
            );
            let empty = BTreeMap::new();
            let mut hooks = hooks.with_variables(&empty);
            let line_numbers = mode_line_numbers(mode, &mut hooks);
            let variables = mode_position_variables(mode, line_numbers);
            let mut hooks = hooks.with_variables(&variables);
            mode_presentation(mode, line_numbers, &mut hooks)
        })
        .collect::<Vec<_>>();
    let pane_borders = request.pane_borders.as_ref().clone();
    if !request.formats.enabled {
        return StatusLine {
            title,
            position: request.formats.position,
            customized: request.customized,
            theme,
            message_style,
            message_command_style,
            modes,
            pane_borders,
            ..StatusLine::default()
        };
    }
    let (left, right) = {
        let mut hooks = DaemonFormatHooks::status(
            request.client,
            request.facts.as_ref(),
            &request.context,
            Some(&request.option_snapshot),
            cache,
            touched,
            refresh,
            now,
            &request.environment,
            &request.default_terminal,
            request.startup,
            tmux_shim,
            zz_executable,
            job_waker,
            job_client,
        );
        (
            expand_status_parts(
                &request.formats.left,
                &request.context,
                &mut hooks,
                parts.map(|parts| parts.left.as_slice()),
            ),
            expand_status_parts(
                &request.formats.right,
                &request.context,
                &mut hooks,
                parts.map(|parts| parts.right.as_slice()),
            ),
        )
    };
    let mut hooks = DaemonFormatHooks::status(
        request.client,
        request.facts.as_ref(),
        &request.context,
        Some(&request.option_snapshot),
        cache,
        touched,
        refresh,
        now,
        &request.environment,
        &request.default_terminal,
        request.startup,
        tmux_shim,
        zz_executable,
        job_waker,
        job_client,
    );
    let base_style = expand_base_status_style(&request.formats, &request.context, &mut hooks);
    let lines = usize::from(request.formats.lines).min(MAX_STATUS_ROWS);
    let rows = (0..lines)
        .map(|index| {
            let index = u32::try_from(index).expect("status row index fits u32");
            request
                .row_formats
                .get(&index)
                .map_or_else(String::new, |format| {
                    clamp_status_text(expand_status_parts(
                        format,
                        &request.context,
                        &mut hooks,
                        parts
                            .and_then(|parts| parts.rows.get(&index))
                            .map(Vec::as_slice),
                    ))
                })
        })
        .collect::<Vec<_>>();
    let message_line = if rows.is_empty() {
        0
    } else {
        request
            .message_line
            .min(u8::try_from(rows.len() - 1).expect("status row count fits u8"))
    };
    StatusLine {
        left: wrap_status_style(
            &request.formats,
            &trim_status_left(&left, usize::from(request.formats.left_length)),
            &request.formats.left_style,
        ),
        right: wrap_status_style(
            &request.formats,
            &trim_status_left(&right, usize::from(request.formats.right_length)),
            &request.formats.right_style,
        ),
        title,
        base_style,
        rows,
        position: request.formats.position,
        message_line,
        customized: request.customized,
        theme,
        message_style,
        message_command_style,
        modes,
        pane_borders,
    }
}

pub(super) fn clamp_status_text(mut value: String) -> String {
    if value.len() <= MAX_STATUS_TEXT_BYTES {
        return value;
    }
    let boundary = (0..=MAX_STATUS_TEXT_BYTES)
        .rev()
        .find(|index| value.is_char_boundary(*index))
        .unwrap_or_default();
    value.truncate(boundary);
    value
}

fn expand_base_status_style(
    formats: &StatusFormats,
    context: &StatusContext,
    hooks: &mut DaemonFormatHooks<'_>,
) -> String {
    let mut style = clamp_status_text(expand_status(&formats.style, context, hooks));
    if zz_protocol::parse_style(&style).is_none() {
        style = String::new();
    }
    let mut appended = false;
    for (key, value) in [("fg", &formats.foreground), ("bg", &formats.background)] {
        if value.as_str() != "default" {
            let separator = if style.is_empty() { "" } else { "," };
            let addition = format!("{separator}{key}={value}");
            if style.len() + addition.len() <= MAX_STATUS_TEXT_BYTES {
                style.push_str(&addition);
                appended = true;
            }
        }
    }
    if appended && zz_protocol::parse_style(&style).is_none() {
        return String::new();
    }
    style
}

fn wrap_status_style(formats: &StatusFormats, text: &str, side_style: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut base = formats.style.clone();
    if formats.foreground != "default" {
        if !base.is_empty() {
            base.push(',');
        }
        base.push_str("fg=");
        base.push_str(&formats.foreground);
    }
    if formats.background != "default" {
        if !base.is_empty() {
            base.push(',');
        }
        base.push_str("bg=");
        base.push_str(&formats.background);
    }
    if base.is_empty() && matches!(side_style, "" | "default") {
        return clamp_status_text(text.to_owned());
    }
    let mut output = String::with_capacity(
        (base.len() + side_style.len() + text.len() + 32).min(MAX_STATUS_TEXT_BYTES),
    );
    let carries_base = if base.is_empty() {
        false
    } else {
        let marker_len = base.len().saturating_add(3);
        if marker_len.saturating_add("#[push-default]".len()) <= MAX_STATUS_TEXT_BYTES {
            output.push_str("#[");
            output.push_str(&base);
            output.push(']');
            true
        } else {
            false
        }
    };
    if !side_style.is_empty() {
        let marker_len = side_style.len().saturating_add(3);
        let reserved = if carries_base {
            "#[push-default]".len()
        } else {
            0
        };
        if output
            .len()
            .saturating_add(marker_len)
            .saturating_add(reserved)
            <= MAX_STATUS_TEXT_BYTES
        {
            output.push_str("#[");
            output.push_str(side_style);
            output.push(']');
        }
    }
    if carries_base {
        output.push_str("#[push-default]");
    }
    let remaining = MAX_STATUS_TEXT_BYTES - output.len();
    let boundary = (0..=remaining.min(text.len()))
        .rev()
        .find(|index| text.is_char_boundary(*index))
        .unwrap_or_default();
    output.push_str(&text[..boundary]);
    output
}

pub(super) fn trim_status_left(value: &str, limit: usize) -> String {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    let mut width = 0;
    while index < bytes.len() && width < limit {
        if bytes[index] == b'#' {
            let start = index;
            while bytes.get(index) == Some(&b'#') {
                index += 1;
            }
            let hashes = index - start;
            let marker = bytes.get(index) == Some(&b'[') && hashes % 2 == 1;
            let leading_width = if bytes.get(index) == Some(&b'[') {
                hashes / 2
            } else {
                hashes.div_ceil(2)
            };
            let copy_width = leading_width.min(limit - width);
            if copy_width != 0 {
                if hashes == 1 {
                    output.push('#');
                } else {
                    output.extend(std::iter::repeat_n('#', copy_width * 2));
                }
                width += copy_width;
            }
            if marker {
                let marker_start = index - 1;
                let Some(end) = status_style_end(value, index + 1) else {
                    break;
                };
                output.push_str(&value[marker_start..=end]);
                index = end + 1;
            }
            continue;
        }
        let character = value[index..]
            .chars()
            .next()
            .expect("status trim index is on a character boundary");
        let character_width = display_width(character.encode_utf8(&mut [0; 4]));
        if width + character_width <= limit {
            output.push(character);
        }
        width += character_width;
        index += character.len_utf8();
    }
    output
}

fn status_style_end(value: &str, start: usize) -> Option<usize> {
    let bytes = value.as_bytes();
    let mut position = start;
    let mut formats = 0;
    while position < bytes.len() {
        if bytes[position] == b'#' && bytes.get(position + 1) == Some(&b'{') {
            formats += 1;
            position += 2;
            continue;
        }
        if bytes[position] == b'}' && formats != 0 {
            formats -= 1;
            position += 1;
            continue;
        }
        if bytes[position] == b']' && formats == 0 {
            return Some(position);
        }
        position += 1;
    }
    None
}
