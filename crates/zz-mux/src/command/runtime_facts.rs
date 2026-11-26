use super::*;

impl MuxEngine {
    pub fn set_format_server_context(
        &mut self,
        host: impl Into<String>,
        host_short: impl Into<String>,
        socket_path: impl Into<String>,
        start_time: u64,
    ) {
        self.format_host = host.into();
        self.format_host_short = host_short.into();
        self.format_socket_path = socket_path.into();
        self.format_start_time = start_time;
        self.state.set_default_pane_title(self.format_host.clone());
        self.state.set_format_now(start_time);
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
    }

    pub const fn set_format_now(&mut self, now: u64) {
        self.state.set_format_now(now);
    }

    pub fn set_format_server_identity(
        &mut self,
        pid: u32,
        uid: impl Into<String>,
        user: impl Into<String>,
    ) {
        self.format_pid = pid;
        self.format_uid = uid.into();
        self.format_user = user.into();
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
    }

    pub fn mark_session_active_at(&mut self, session: SessionId, now: u64) {
        let before = self
            .state
            .sessions
            .get(&session)
            .map(|session| (session.activity, session.sort_activity));
        self.state.mark_session_active_at(session, now);
        let after = self
            .state
            .sessions
            .get(&session)
            .map(|session| (session.activity, session.sort_activity));
        if before != after {
            self.format_data_generation = self.format_data_generation.wrapping_add(1);
        }
    }

    pub fn touch_window_activity_for_pane(&mut self, pane: PaneId) {
        let window = self.state.window_for_pane(pane);
        let before = window
            .and_then(|window| self.state.windows.get(&window))
            .map(|window| (window.activity, window.activity_time));
        self.state.touch_window_activity_for_pane(pane);
        let after = window
            .and_then(|window| self.state.windows.get(&window))
            .map(|window| (window.activity, window.activity_time));
        if before != after {
            self.format_data_generation = self.format_data_generation.wrapping_add(1);
        }
    }

    pub fn set_pane_runtime_facts(&mut self, pane: PaneId, facts: PaneRuntimeFacts) -> bool {
        let mut hooks = CommandHooks::new(self.format_now());
        self.set_pane_runtime_facts_with_hooks(pane, facts, &mut hooks)
    }

    pub fn set_pane_runtime_facts_with_hooks(
        &mut self,
        pane: PaneId,
        facts: PaneRuntimeFacts,
        hooks: &mut impl StatusHooks,
    ) -> bool {
        self.set_pane_runtime_facts_at(pane, facts, hooks, Instant::now())
    }

    pub fn set_pane_runtime_facts_at(
        &mut self,
        pane: PaneId,
        mut facts: PaneRuntimeFacts,
        hooks: &mut impl StatusHooks,
        now: Instant,
    ) -> bool {
        if self.state.pane(pane).is_none() {
            return false;
        }
        facts.dead_signal = tmux_signal_name(&facts.dead_signal);
        let command_changed = self
            .pane_runtime_facts
            .get(&pane)
            .map_or(!facts.current_command.is_empty(), |previous| {
                previous.current_command != facts.current_command
            });
        if self.pane_runtime_facts.get(&pane) == Some(&facts) {
            return false;
        }
        self.pane_runtime_facts.insert(pane, facts);
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
        if command_changed {
            self.refresh_automatic_window_name_throttled(pane, hooks, now);
        }
        true
    }

    pub fn set_automatic_rename_throttle(&mut self, enabled: bool) {
        self.automatic_rename_throttle = enabled;
        if !enabled {
            self.pending_window_renames.clear();
        }
    }

    fn refresh_automatic_window_name_throttled(
        &mut self,
        pane: PaneId,
        hooks: &mut impl StatusHooks,
        now: Instant,
    ) -> bool {
        if !self.automatic_rename_throttle {
            return self.refresh_automatic_window_name_for_pane(pane, hooks);
        }
        let Some(window) = self.automatic_rename_window(pane) else {
            return false;
        };
        if self.window_name_waits(window, now) {
            self.pending_window_renames.insert(window);
            return false;
        }
        self.window_name_times.insert(window, now);
        self.pending_window_renames.remove(&window);
        self.refresh_automatic_window_name_for_pane(pane, hooks)
    }

    fn automatic_rename_window(&self, pane: PaneId) -> Option<WindowId> {
        let window = self.state.window_for_pane(pane)?;
        (self.state.windows[&window].active_pane == pane
            && self
                .state
                .window_automatic_rename(window)
                .unwrap_or_default())
        .then_some(window)
    }

    fn window_name_waits(&self, window: WindowId, now: Instant) -> bool {
        self.window_name_times
            .get(&window)
            .is_some_and(|last| now.saturating_duration_since(*last) < NAME_INTERVAL)
    }

    #[must_use]
    pub fn automatic_rename_due(&self, pane: PaneId, now: Instant) -> bool {
        self.automatic_rename_window(pane).is_some_and(|window| {
            !self.automatic_rename_throttle || !self.window_name_waits(window, now)
        })
    }

    #[must_use]
    pub fn next_window_rename_deadline(&self) -> Option<Instant> {
        self.pending_window_renames
            .iter()
            .filter_map(|window| self.window_name_times.get(window))
            .map(|last| *last + NAME_INTERVAL)
            .min()
    }

    pub fn note_automatic_rename_output(&mut self, pane: PaneId, now: Instant) {
        if !self.automatic_rename_throttle {
            return;
        }
        let Some(window) = self.automatic_rename_window(pane) else {
            return;
        };
        match self.window_name_times.get(&window) {
            Some(last) if *last == now => {}
            Some(last) if now.saturating_duration_since(*last) < NAME_INTERVAL => {
                self.pending_window_renames.insert(window);
            }
            _ => {
                self.window_name_times.insert(window, now);
            }
        }
    }

    #[must_use]
    pub fn due_window_rename_panes(&self, now: Instant) -> Vec<PaneId> {
        self.pending_window_renames
            .iter()
            .filter(|window| !self.window_name_waits(**window, now))
            .filter_map(|window| self.state.windows.get(window))
            .map(|window| window.active_pane)
            .collect()
    }

    pub fn apply_due_window_renames(&mut self, now: Instant, hooks: &mut impl StatusHooks) -> bool {
        self.window_name_times
            .retain(|window, _| self.state.windows.contains_key(window));
        let due = self
            .pending_window_renames
            .iter()
            .copied()
            .filter(|window| {
                self.window_name_times
                    .get(window)
                    .is_none_or(|last| now.saturating_duration_since(*last) >= NAME_INTERVAL)
            })
            .collect::<Vec<_>>();
        let mut renamed = false;
        for window in due {
            self.pending_window_renames.remove(&window);
            let Some(pane) = self
                .state
                .windows
                .get(&window)
                .map(|state| state.active_pane)
            else {
                continue;
            };
            self.window_name_times.insert(window, now);
            renamed |= self.refresh_automatic_window_name_for_pane(pane, hooks);
        }
        renamed
    }

    #[must_use]
    pub fn runtime_facts_reach_presentation(&self) -> bool {
        self.presentation_templates(true)
            .into_iter()
            .any(template_reads_runtime_facts)
    }

    #[must_use]
    pub fn window_labels_follow_the_clock(&self) -> bool {
        self.presentation_templates(false)
            .into_iter()
            .any(|template| {
                template.contains('#')
                    && CLOCK_MARKERS.iter().any(|marker| template.contains(marker))
                    || template.contains('%')
            })
    }

    fn presentation_templates(&self, status: bool) -> Vec<&str> {
        const LABEL_SCALARS: [&str; 5] = [
            "pane-border-format",
            "pane-border-style",
            "pane-active-border-style",
            "window-pane-status-format",
            "window-pane-current-status-format",
        ];
        const STATUS_SCALARS: [&str; 3] =
            ["window-style", "window-active-style", "set-titles-string"];
        let names = || {
            LABEL_SCALARS
                .iter()
                .chain(status.then_some(&STATUS_SCALARS).into_iter().flatten())
        };
        let window_status = &self.window_status;
        let mut templates = vec![
            window_status.format.as_str(),
            window_status.current_format.as_str(),
            window_status.separator.as_str(),
            window_status.style.as_str(),
            window_status.current_style.as_str(),
            window_status.last_style.as_str(),
            window_status.bell_style.as_str(),
            window_status.activity_style.as_str(),
        ];
        templates.extend(
            self.window_status_options
                .values()
                .flat_map(BTreeMap::values)
                .map(String::as_str),
        );
        templates.extend(
            names().filter_map(|name| tmux_stored_scalar(name).map(|metadata| metadata.default)),
        );
        let scalar_tables = std::iter::once(&self.stored_scalars.global_session)
            .chain(self.stored_scalars.sessions.values())
            .chain(std::iter::once(&self.stored_scalars.global_window))
            .chain(self.stored_scalars.windows.values())
            .chain(self.stored_scalars.panes.values());
        for table in scalar_tables {
            templates.extend(
                names()
                    .filter_map(|name| table.get(name))
                    .map(String::as_str),
            );
        }
        if !status {
            return templates;
        }
        let formats = &self.status;
        templates.extend([
            formats.left.as_str(),
            formats.right.as_str(),
            formats.style.as_str(),
            formats.background.as_str(),
            formats.foreground.as_str(),
            formats.left_style.as_str(),
            formats.right_style.as_str(),
        ]);
        templates.extend(
            self.session_status_options
                .values()
                .flat_map(BTreeMap::values)
                .map(String::as_str),
        );
        if !self.explicit_status_options.is_empty()
            || !self.session_explicit_status_options.is_empty()
        {
            let default = default_array("status-format");
            for table in std::iter::once(&self.stored_arrays.global_session)
                .chain(self.stored_arrays.sessions.values())
            {
                if let Some(array) = table.get("status-format")
                    && *array != default
                {
                    templates.extend(array.values().map(String::as_str));
                }
            }
        }
        templates
    }

    pub fn mark_pane_dead(
        &mut self,
        pane: PaneId,
        status: Option<u32>,
        signal: Option<&str>,
    ) -> Result<bool, ServerError> {
        let mut hooks = CommandHooks::new(self.format_now());
        self.mark_pane_dead_with_hooks(pane, status, signal, &mut hooks)
    }

    pub fn mark_pane_dead_with_hooks(
        &mut self,
        pane: PaneId,
        status: Option<u32>,
        signal: Option<&str>,
        hooks: &mut impl StatusHooks,
    ) -> Result<bool, ServerError> {
        if self.state.pane(pane).is_none() {
            return Err(ServerError::MissingTarget(pane.to_string()));
        }
        let dead_signal = signal.map_or_else(String::new, tmux_signal_name);
        let facts = self.pane_runtime_facts.entry(pane).or_default();
        let facts_changed = facts.dead_signal != dead_signal;
        facts.dead_signal = dead_signal;
        let state_changed = self.state.mark_pane_dead(pane, status)?;
        if state_changed {
            let format_now = self.format_now();
            self.state
                .pane_mut(pane)
                .expect("dead pane exists")
                .dead_time = (format_now != 0).then_some(format_now);
        }
        if facts_changed && !state_changed {
            self.state.bump_generation();
        }
        let name_changed = self.refresh_automatic_window_name_for_pane(pane, hooks);
        Ok(facts_changed || state_changed || name_changed)
    }

    #[must_use]
    pub fn pane_runtime_facts(&self, pane: PaneId) -> Option<&PaneRuntimeFacts> {
        self.pane_runtime_facts.get(&pane)
    }

    pub fn set_agent_chat(&mut self, pane: PaneId, facts: Option<AgentChatFacts>) -> bool {
        let before = match facts {
            Some(facts) if self.state.pane(pane).is_some() => self.agent_chats.insert(pane, facts),
            Some(_) => return false,
            None => self.agent_chats.remove(&pane),
        };
        before.as_ref() != self.agent_chats.get(&pane)
    }

    #[must_use]
    pub fn agent_chat(&self, pane: PaneId) -> Option<&AgentChatFacts> {
        self.agent_chats.get(&pane)
    }

    pub fn set_agent_parent(&mut self, child: PaneId, parent: PaneId) {
        if self.state.pane(child).is_some() {
            self.agent_parents.insert(child, parent);
        }
    }

    #[must_use]
    pub fn agent_parent(&self, child: PaneId) -> Option<PaneId> {
        self.agent_parents.get(&child).copied()
    }

    #[must_use]
    pub const fn agent_parents(&self) -> &BTreeMap<PaneId, PaneId> {
        &self.agent_parents
    }

    #[must_use]
    pub fn agent_children(&self, parent: PaneId) -> Vec<PaneId> {
        self.agent_parents
            .iter()
            .filter(|(_, candidate)| **candidate == parent)
            .map(|(child, _)| *child)
            .collect()
    }

    pub fn format_host(&self) -> &str {
        &self.format_host
    }

    pub fn format_host_short(&self) -> &str {
        &self.format_host_short
    }

    pub(crate) fn format_socket_path(&self) -> &str {
        &self.format_socket_path
    }

    pub(crate) const fn format_pid(&self) -> u32 {
        self.format_pid
    }

    pub(crate) const fn format_start_time(&self) -> u64 {
        self.format_start_time
    }

    pub(crate) const fn format_now(&self) -> u64 {
        self.state.format_now()
    }

    #[must_use]
    pub fn format_uid(&self) -> &str {
        &self.format_uid
    }

    #[must_use]
    pub fn format_user(&self) -> &str {
        &self.format_user
    }

    pub(super) fn refresh_automatic_window_name_for_pane(
        &mut self,
        pane: PaneId,
        hooks: &mut impl StatusHooks,
    ) -> bool {
        let Some(window) = self.state.window_for_pane(pane) else {
            return false;
        };
        let Some(window_state) = self.state.windows.get(&window) else {
            return false;
        };
        if window_state.active_pane != pane
            || !self
                .state
                .window_automatic_rename(window)
                .unwrap_or_default()
        {
            return false;
        }
        let session = window_state.session;
        let pane_state = &window_state.panes[&pane];
        let pane_dead = pane_state.dead;
        let format = self.automatic_rename_format_for_window(window).to_owned();
        let agent_title = (format == DEFAULT_AUTOMATIC_RENAME_FORMAT
            && matches!(pane_state.kind, PaneKind::Agent(_)))
        .then(|| RawText::from(pane_state.title.clone()));
        if agent_title.is_none()
            && !pane_dead
            && !self.pane_runtime_facts.contains_key(&pane)
            && format == DEFAULT_AUTOMATIC_RENAME_FORMAT
        {
            return false;
        }
        let mut hooks = CommandItemHooks {
            inner: hooks,
            command: None,
        };
        let name = agent_title.unwrap_or_else(|| {
            expand_format_with_hooks(
                &format,
                self,
                FormatContext {
                    session: Some(session),
                    window: Some(window),
                    pane: None,
                    active_session: Some(session),
                    format_client: FormatClient::NoClient,
                    format_type: FormatType::Window,
                },
                &mut hooks,
            )
        });
        let changed = self.state.windows[&window].name != name;
        if changed {
            self.state
                .rename_window(window, name)
                .expect("automatic rename window exists");
        }
        changed
    }

    pub fn update_agent_title(&mut self, pane: PaneId, title: Option<&str>) -> bool {
        if !self
            .state
            .update_agent_title(pane, title)
            .unwrap_or_default()
        {
            return false;
        }
        let mut hooks = CommandHooks::new(self.format_now());
        self.refresh_automatic_window_name_for_pane(pane, &mut hooks);
        true
    }

    #[must_use]
    pub fn format_pane_kind(&self, pane: &str) -> Option<&'static str> {
        self.state
            .pane(pane.parse().ok()?)
            .map(|pane| pane_kind_name(&pane.kind))
    }

    #[must_use]
    pub fn format_browser_url(&self, pane: &str) -> Option<&str> {
        match &self.state.pane(pane.parse().ok()?)?.kind {
            PaneKind::Browser(browser) => Some(browser.url()),
            _ => None,
        }
    }

    #[must_use]
    pub fn format_facts(&self) -> FormatFacts {
        let mut pane_kinds = BTreeMap::new();
        let mut browser_urls = BTreeMap::new();
        let mut pane_windows = BTreeMap::new();
        let agent_chats = self
            .agent_chats
            .iter()
            .map(|(pane, facts)| (pane.to_string(), facts.clone()))
            .collect();
        let agent_parents = self
            .agent_parents
            .iter()
            .map(|(child, parent)| (child.to_string(), parent.to_string()))
            .collect();
        for (window, entry) in &self.state.windows {
            for (pane, entry) in &entry.panes {
                pane_kinds.insert(pane.to_string(), pane_kind_name(&entry.kind));
                if let PaneKind::Browser(browser) = &entry.kind {
                    browser_urls.insert(pane.to_string(), browser.url().to_owned());
                }
                pane_windows.insert(pane.to_string(), window.to_string());
            }
        }
        fn keyed<K: std::fmt::Display>(
            options: &BTreeMap<K, SharedUserOptions>,
        ) -> BTreeMap<String, SharedUserOptions> {
            options
                .iter()
                .filter(|(_, values)| !values.is_empty())
                .map(|(id, values)| (id.to_string(), Arc::clone(values)))
                .collect()
        }
        FormatFacts {
            pane_kinds,
            browser_urls,
            agent_chats,
            agent_parents,
            pane_windows,
            server: Arc::clone(&self.server_user_options),
            global_session: Arc::clone(&self.global_session_user_options),
            sessions: keyed(&self.session_user_options),
            global_window: Arc::clone(&self.global_window_user_options),
            windows: keyed(&self.window_user_options),
            panes: keyed(&self.pane_user_options),
        }
    }
}

fn pane_kind_name(kind: &PaneKind) -> &'static str {
    match kind {
        PaneKind::Picker { .. } => "picker",
        PaneKind::Terminal => "terminal",
        PaneKind::Browser(_) => "browser",
        PaneKind::Editor(_) => "editor",
        PaneKind::Agent(_) => "agent",
        PaneKind::View(_) => "view",
        PaneKind::Simulator(_) => SIMULATOR_PANE_KIND,
    }
}

pub(super) fn tmux_clean_name(name: &str, kind: &str) -> Result<String, ServerError> {
    if name.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(ServerError::InvalidCommand(format!(
            "invalid {kind} name: {name}"
        )));
    }
    Ok(tmux_vis(name, false))
}

pub(crate) fn parse_window_name(input: &str) -> String {
    let mut name = input.strip_prefix('"').unwrap_or(input);
    if let Some(end) = name.find('"') {
        name = &name[..end];
    }
    name = name.strip_prefix("exec ").unwrap_or(name);
    name = name.trim_start_matches([' ', '-']);
    if let Some(end) = name.find(' ') {
        name = &name[..end];
    }
    let bytes = name.as_bytes();
    let mut end = bytes.len();
    while end > 1
        && !(bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1].is_ascii_punctuation())
    {
        end -= 1;
    }
    let Ok(name) = std::str::from_utf8(&bytes[..end]) else {
        return String::new();
    };
    let name = if name.starts_with('/') {
        let trimmed = name.trim_end_matches('/');
        if trimmed.is_empty() {
            "/"
        } else {
            trimmed.rsplit('/').next().unwrap_or(trimmed)
        }
    } else {
        name
    };
    tmux_vis(name, false)
}

pub(super) fn tmux_clean_title(title: &str) -> Option<String> {
    if title.bytes().any(|byte| byte.is_ascii_control()) {
        return None;
    }
    Some(tmux_vis(title, false))
}
