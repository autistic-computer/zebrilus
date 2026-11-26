use super::*;

impl MuxEngine {
    #[must_use]
    pub fn format_cache_revision(&self) -> (u64, u64, u64, u64) {
        let revision = FormatCaptureRevision::new(self);
        (
            revision.state,
            revision.options,
            revision.data,
            revision.now,
        )
    }

    #[must_use]
    pub fn cached_detached_format_context<'a>(
        &self,
        target: (Option<SessionId>, Option<WindowId>, Option<PaneId>),
        format_client: FormatClient,
        needs: FormatNeeds,
        references: &BTreeSet<String>,
        sorted_overrides: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Option<StatusContext<'static>> {
        self.cached_detached_format_context_inner(
            target,
            format_client,
            needs,
            references,
            None,
            sorted_overrides,
        )
    }

    #[must_use]
    pub fn cached_detached_format_context_with_shared_references<'a>(
        &self,
        target: (Option<SessionId>, Option<WindowId>, Option<PaneId>),
        format_client: FormatClient,
        needs: FormatNeeds,
        references: &Arc<BTreeSet<String>>,
        sorted_overrides: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Option<StatusContext<'static>> {
        self.cached_detached_format_context_inner(
            target,
            format_client,
            needs,
            references,
            Some(references),
            sorted_overrides,
        )
    }

    fn cached_detached_format_context_inner<'a>(
        &self,
        target: (Option<SessionId>, Option<WindowId>, Option<PaneId>),
        format_client: FormatClient,
        needs: FormatNeeds,
        references: &BTreeSet<String>,
        shared: Option<&Arc<BTreeSet<String>>>,
        sorted_overrides: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Option<StatusContext<'static>> {
        if !tree::borrowed_formats() {
            return None;
        }
        let target_ids = self.format_target(target.0, target.1, target.2);
        let mut cache = self.format_context_cache.lock();
        let cached = cache.as_mut()?;
        let revision = FormatCaptureRevision::new(self);
        (cached.target == target_ids
            && cached.needs == needs
            && cached.matches_references(Some(references), shared)
            && cached
                .context
                .capture_revision
                .as_deref()
                .is_some_and(|captured| captured.same_data(&revision))
            && cached.context.format_client == format_client
            && format_id_matches(&cached.context.session_id, target_ids.0.map(|id| id.0), '$')
            && format_id_matches(&cached.context.window_id, target_ids.1.map(|id| id.0), '@')
            && format_id_matches(&cached.context.pane_id, target_ids.2.map(|id| id.0), '%')
            && {
                let mut overrides = sorted_overrides.into_iter();
                cached.input_variables.iter().all(|(name, value)| {
                    overrides
                        .next()
                        .is_some_and(|(expected_name, expected_value)| {
                            name == expected_name && value == expected_value
                        })
                }) && overrides.next().is_none()
            })
        .then(|| {
            cached.adopt_reference_identity(shared);
            cached.context.with_capture_clock(
                revision,
                i64::try_from(revision.now).ok().filter(|now| *now != 0),
            )
        })
    }

    #[must_use]
    pub fn cached_format_references(&self, template: &str) -> Arc<BTreeSet<String>> {
        let generation = self.format_options_generation();
        {
            let cache = self.format_reference_cache.lock();
            if let Some((cached_generation, entries)) = cache.as_ref()
                && *cached_generation == generation
                && let Some(references) = entries.get(template)
            {
                return Arc::clone(references);
            }
        }
        let mut references = format_references(template)
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut followed = BTreeSet::new();
        while !references.contains("*") {
            let indirect = references
                .iter()
                .filter_map(|name| name.strip_prefix("E:").or_else(|| name.strip_prefix("T:")))
                .filter(|name| !followed.contains(*name))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if indirect.is_empty() {
                break;
            }
            for name in indirect {
                followed.insert(name.clone());
                if !self.format_option_texts(&name, &mut |text| {
                    references.extend(format_references(text).iter().cloned());
                }) {
                    references.insert("*".to_owned());
                }
            }
        }
        let references = Arc::new(references);
        let mut cache = self.format_reference_cache.lock();
        if cache
            .as_ref()
            .is_none_or(|(cached, _)| *cached != generation)
        {
            *cache = Some((generation, BTreeMap::new()));
        }
        let entries = &mut cache.as_mut().expect("initialized reference cache").1;
        if entries.len() >= 128 {
            entries.clear();
        }
        entries.insert(template.to_owned(), Arc::clone(&references));
        references
    }

    #[must_use]
    pub fn cached_format_references_for_templates<'t>(
        &self,
        templates: impl IntoIterator<Item = &'t str>,
    ) -> Arc<BTreeSet<String>> {
        let sources = templates.into_iter().collect::<Vec<_>>();
        let generation = self.format_options_generation();
        {
            let cache = self.format_reference_union_cache.lock();
            if let Some(cache) = cache.as_ref()
                && cache.generation == generation
                && cache
                    .sources
                    .iter()
                    .map(String::as_str)
                    .eq(sources.iter().copied())
            {
                return Arc::clone(&cache.references);
            }
        }
        let mut references = BTreeSet::new();
        for source in &sources {
            references.extend(self.cached_format_references(source).iter().cloned());
        }
        let references = Arc::new(references);
        let bytes = reference_bytes(&references).saturating_add(sources.iter().fold(
            0usize,
            |bytes, source| {
                bytes
                    .saturating_add(std::mem::size_of::<String>())
                    .saturating_add(source.len())
            },
        ));
        *self.format_reference_union_cache.lock() =
            (bytes <= FORMAT_CAPTURE_CACHE_BYTES).then(|| FormatReferenceUnionCache {
                generation,
                sources: sources.into_iter().map(str::to_owned).collect(),
                references: Arc::clone(&references),
            });
        references
    }

    #[must_use]
    pub fn format_needs<'t>(&self, templates: impl IntoIterator<Item = &'t str>) -> FormatNeeds {
        scan_format_needs(Some(self), templates)
    }
}

impl MuxEngine {
    #[must_use]
    pub fn format_context_snapshot(
        &self,
        format_client: FormatClient,
    ) -> FormatContextSnapshot<'_> {
        FormatContextSnapshot {
            engine: self,
            format_client,
            universe: FormatUniverseRef::deferred(self, format_client),
        }
    }

    #[must_use]
    pub fn format_universe(&self, format_client: FormatClient) -> FormatUniverseRef<'_> {
        FormatUniverseRef::deferred(self, format_client)
    }

    #[must_use]
    pub fn format_status_context(
        &self,
        session: Option<SessionId>,
        window: Option<WindowId>,
        pane: Option<PaneId>,
    ) -> StatusContext<'_> {
        FormatContext {
            session,
            window,
            pane,
            active_session: session,
            format_client: FormatClient::NoClient,
            format_type: FormatType::None,
        }
        .resolve(self)
        .values
    }

    #[must_use]
    pub fn format_status_context_for_client(
        &self,
        session: Option<SessionId>,
        window: Option<WindowId>,
        pane: Option<PaneId>,
        client_session: SessionId,
    ) -> StatusContext<'_> {
        FormatContext {
            session,
            window,
            pane,
            active_session: Some(client_session),
            format_client: FormatClient::Attached(client_session),
            format_type: FormatType::None,
        }
        .resolve(self)
        .values
    }

    #[must_use]
    pub fn format_status_context_with_format_client(
        &self,
        session: Option<SessionId>,
        window: Option<WindowId>,
        pane: Option<PaneId>,
        active_session: Option<SessionId>,
        format_client: FormatClient,
    ) -> StatusContext<'_> {
        FormatContext {
            session,
            window,
            pane,
            active_session,
            format_client,
            format_type: FormatType::None,
        }
        .resolve(self)
        .values
    }

    pub(super) fn format_target(
        &self,
        session: Option<SessionId>,
        window: Option<WindowId>,
        pane: Option<PaneId>,
    ) -> (Option<SessionId>, Option<WindowId>, Option<PaneId>) {
        let state = &self.state;
        let pane_window =
            pane.and_then(|pane| state.window_for_pane(pane).map(|window| (pane, window)));
        let pane = pane_window.map(|(pane, _)| pane);
        let window = pane_window
            .map(|(_, window)| window)
            .or_else(|| window.filter(|window| state.windows.contains_key(window)));
        let session = window
            .and_then(|window| state.windows.get(&window).map(|window| window.session))
            .or_else(|| session.filter(|session| state.sessions.contains_key(session)));
        let window = window.or_else(|| {
            session
                .and_then(|session| state.sessions.get(&session))
                .map(|session| session.active_window)
                .filter(|window| state.windows.contains_key(window))
        });
        let pane = pane.or_else(|| {
            window
                .and_then(|window| state.windows.get(&window))
                .map(|window| window.active_pane)
        });
        (session, window, pane)
    }

    #[must_use]
    pub fn pane_format_geometry(
        &self,
        session: Option<SessionId>,
        window: Option<WindowId>,
        pane: Option<PaneId>,
    ) -> PaneFormatGeometry {
        let (_, window, pane) = self.format_target(session, window, pane);
        let Some(window) = window.and_then(|window| self.state.windows.get(&window)) else {
            return PaneFormatGeometry::default();
        };
        let (width, height) = window.layout.extent();
        let mut geometry = PaneFormatGeometry {
            window_width: Some(width),
            window_height: Some(height),
            ..PaneFormatGeometry::default()
        };
        let Some(cell) = pane
            .filter(|pane| window.panes.contains_key(pane))
            .and_then(|pane| {
                window.displayed_pane_cell(pane, self.displayed_pane_border_status(window.id))
            })
        else {
            return geometry;
        };
        let left = u16::try_from(cell.xoff.max(0)).unwrap_or(u16::MAX);
        let top = u16::try_from(cell.yoff.max(0)).unwrap_or(u16::MAX);
        geometry.pane_left = Some(left);
        geometry.pane_top = Some(top);
        geometry.pane_width = Some(cell.sx);
        geometry.pane_height = Some(cell.sy);
        geometry.pane_right = left
            .checked_add(cell.sx)
            .and_then(|right| right.checked_sub(1));
        geometry.pane_bottom = top
            .checked_add(cell.sy)
            .and_then(|bottom| bottom.checked_sub(1));
        geometry
    }

    pub(super) fn format_session_items(&self, format_client: FormatClient) -> Vec<FormatLoopItem> {
        let active_session = format_client.attached_session();
        self.state
            .sessions
            .values()
            .map(|session| {
                let active_window = self.state.windows.get(&session.active_window);
                FormatLoopItem {
                    active: active_session == Some(session.id),
                    metadata: self.loop_metadata(
                        Some(session.id),
                        active_window.map(|window| window.id),
                        active_window.map(|window| window.active_pane),
                    ),
                    context: self.loop_status_values(
                        Some(session.id),
                        active_window.map(|window| window.id),
                        active_window.map(|window| window.active_pane),
                        format_client,
                        &mut LayoutDumps::default(),
                    ),
                }
            })
            .collect()
    }

    pub(super) fn format_window_items(
        &self,
        session: SessionId,
        format_client: FormatClient,
    ) -> Option<Vec<FormatLoopItem>> {
        let session = self.state.sessions.get(&session)?;
        let mut items = session
            .windows
            .iter()
            .filter_map(|window| self.state.windows.get(window))
            .map(|window| FormatLoopItem {
                active: window.id == session.active_window,
                metadata: self.loop_metadata(
                    Some(session.id),
                    Some(window.id),
                    Some(window.active_pane),
                ),
                context: self.loop_status_values(
                    Some(session.id),
                    Some(window.id),
                    Some(window.active_pane),
                    format_client,
                    &mut LayoutDumps::default(),
                ),
            })
            .collect::<Vec<_>>();
        items.sort_by_key(|item| item.metadata.window_index);
        Some(items)
    }

    pub(super) fn format_pane_items(
        &self,
        window: WindowId,
        format_client: FormatClient,
    ) -> Option<Vec<FormatLoopItem>> {
        if !self.format_window_listed(window) {
            return None;
        }
        let window = self.state.windows.get(&window)?;
        let mut dumps = LayoutDumps::default();
        let mut items = window
            .panes
            .values()
            .map(|pane| FormatLoopItem {
                active: pane.id == window.active_pane,
                metadata: self.loop_metadata(Some(window.session), Some(window.id), Some(pane.id)),
                context: self.loop_status_values(
                    Some(window.session),
                    Some(window.id),
                    Some(pane.id),
                    format_client,
                    &mut dumps,
                ),
            })
            .collect::<Vec<_>>();
        items.sort_by_key(|item| pane_number(&item.context.pane_id));
        Some(items)
    }

    pub(super) fn format_window_listed(&self, window: WindowId) -> bool {
        self.state
            .windows
            .get(&window)
            .and_then(|entry| self.state.sessions.get(&entry.session))
            .is_some_and(|session| session.windows.contains(&window))
    }

    pub(super) fn format_option_rows_for(
        &self,
        key: OptionRowsKey,
    ) -> Option<Vec<FormatOptionRow>> {
        let target = match key {
            OptionRowsKey::Server => TmuxOptionTarget::Server,
            OptionRowsKey::GlobalSession => TmuxOptionTarget::GlobalSession,
            OptionRowsKey::GlobalWindow => TmuxOptionTarget::GlobalWindow,
            OptionRowsKey::Session(session) => {
                self.state.sessions.contains_key(&session).then_some(())?;
                TmuxOptionTarget::Session(session)
            }
            OptionRowsKey::Window(window) => {
                self.format_window_listed(window).then_some(())?;
                TmuxOptionTarget::Window(window)
            }
            OptionRowsKey::Pane(pane) => {
                let window = self.state.window_for_pane(pane)?;
                self.format_window_listed(window).then_some(())?;
                TmuxOptionTarget::Pane(pane)
            }
        };
        Some(self.format_option_rows(target))
    }

    fn loop_status_values(
        &self,
        session_id: Option<SessionId>,
        window_id: Option<WindowId>,
        pane_id: Option<PaneId>,
        format_client: FormatClient,
        dumps: &mut LayoutDumps,
    ) -> StatusContext<'static> {
        let mut context = if tree::borrowed_formats() {
            StatusContext {
                session_id: session_id.map(|id| id.to_string()).unwrap_or_default(),
                window_id: window_id.map(|id| id.to_string()).unwrap_or_default(),
                pane_id: pane_id.map(|id| id.to_string()).unwrap_or_default(),
                ..StatusContext::default()
            }
        } else {
            self.build_status_values(session_id, window_id, pane_id, format_client, dumps)
                .into()
        };
        context.format_client = format_client;
        context
    }

    fn loop_metadata(
        &self,
        session: Option<SessionId>,
        window: Option<WindowId>,
        pane: Option<PaneId>,
    ) -> LoopMetadata {
        let session = session.and_then(|id| self.state.sessions.get(&id));
        let window = window.and_then(|id| self.state.windows.get(&id));
        LoopMetadata {
            session_name: session
                .map(|session| session.name.clone())
                .unwrap_or_default(),
            session_sort_activity: session.map_or(0, |session| session.sort_activity),
            window_name: window.map(|window| window.name.clone()).unwrap_or_default(),
            window_index: window.map_or(0, |window| window.index),
            pane_index: window
                .and_then(|window| pane.and_then(|pane| self.pane_index(window.id, pane)))
                .unwrap_or_default(),
            pane_z: window.and_then(|window| {
                pane.and_then(|pane| window.pane_z(pane))
                    .and_then(|z| usize::try_from(z).ok())
            }),
        }
    }

    pub(super) fn build_status_values(
        &self,
        session_id: Option<SessionId>,
        window_id: Option<WindowId>,
        pane_id: Option<PaneId>,
        format_client: FormatClient,
        dumps: &mut LayoutDumps,
    ) -> StatusValues {
        let mut context = StatusValues {
            host: self.format_host().to_owned(),
            host_short: self.format_host_short().to_owned(),
            next_session_id: self.state.next_session_id().to_string(),
            pid: self.format_pid(),
            server_sessions: self.state.sessions.len(),
            socket_path: self.format_socket_path().to_owned(),
            start_time: i64::try_from(self.format_start_time())
                .ok()
                .filter(|time| *time != 0),
            uid: self.format_uid().to_owned(),
            user: self.format_user().to_owned(),
            version: CommandSpec::TMUX_VERSION_OUTPUT
                .strip_prefix("tmux ")
                .expect("tmux version output prefix")
                .to_owned(),
            format_now: i64::try_from(self.format_now())
                .ok()
                .filter(|time| *time != 0),
            ..StatusValues::default()
        };
        let marked_pane = self.state.marked_pane();
        let marked_window = marked_pane.and_then(|pane| self.state.window_for_pane(pane));
        let marked_session = marked_window.map(|window| self.state.windows[&window].session);
        let Some(session) = session_id.and_then(|id| self.state.sessions.get(&id)) else {
            return context;
        };
        context.session_marked = Some(marked_session == Some(session.id));
        context.session_id = session.id.to_string();
        context.session_name.clone_from(&session.name);
        self.state
            .session_working_directory(session.id)
            .and_then(|path| path.to_str())
            .unwrap_or_default()
            .clone_into(&mut context.session_path);
        context.session_activity = session.activity;
        context.session_created = session.created;
        context.session_sort_activity = session.sort_activity;
        context.session_windows = session.windows.len();
        context.session_active = format_client.session_active(session.id);
        context.active_window_index = self
            .state
            .windows
            .get(&session.active_window)
            .map(|window| window.index);
        context.last_window_index = session
            .windows
            .iter()
            .filter_map(|window| self.state.windows.get(window).map(|window| window.index))
            .max();
        let mut alert_windows = session
            .windows
            .iter()
            .filter_map(|window| self.state.windows.get(window))
            .filter_map(|window| {
                let bell = window.panes.values().any(|pane| pane.bell);
                (window.activity_flag || bell || window.silence_flag).then_some((
                    window.index,
                    window.activity_flag,
                    bell,
                    window.silence_flag,
                ))
            })
            .collect::<Vec<_>>();
        alert_windows.sort_unstable_by_key(|(index, ..)| *index);
        context.session_activity_flag = alert_windows.iter().any(|(_, activity, ..)| *activity);
        context.session_bell = alert_windows.iter().any(|(_, _, bell, _)| *bell);
        context.session_silence_flag = alert_windows.iter().any(|(.., silence)| *silence);
        for (_, activity, bell, silence) in &alert_windows {
            if *activity && !context.session_alert.contains('#') {
                context.session_alert.push('#');
            }
            if *bell && !context.session_alert.contains('!') {
                context.session_alert.push('!');
            }
            if *silence && !context.session_alert.contains('~') {
                context.session_alert.push('~');
            }
        }
        context.session_alerts = alert_windows
            .iter()
            .map(|(index, activity, bell, silence)| {
                let mut entry = index.to_string();
                if *activity {
                    entry.push('#');
                }
                if *bell {
                    entry.push('!');
                }
                if *silence {
                    entry.push('~');
                }
                entry
            })
            .collect::<Vec<_>>()
            .join(",");
        let mut stack = context
            .active_window_index
            .map(|index| index.to_string())
            .into_iter()
            .collect::<Vec<_>>();
        if let Some(last) = session
            .last_window()
            .and_then(|window| self.state.windows.get(&window))
        {
            stack.push(last.index.to_string());
        }
        context.session_stack = stack.join(",");
        let Some(window) = window_id.and_then(|id| self.state.windows.get(&id)) else {
            return context;
        };
        context.window_marked_flag = Some(marked_window == Some(window.id));
        context.window_id = window.id.to_string();
        context.window_index = window.index;
        context.window_name.clone_from(&window.name);
        context.window_panes = window.panes.len();
        let (width, height) = window.layout.extent();
        context.window_width = Some(width);
        context.window_height = Some(height);
        if self.window_size(window.id) == WindowSize::Manual {
            context.window_manual_width = Some(window.manual_extent.0);
            context.window_manual_height = Some(window.manual_extent.1);
        }
        if dumps.window != Some(window.id) {
            let pane_base_index = self.state.pane_base_index(window.id);
            dumps.layout = window.layout_string(LayoutFormat::V2, pane_base_index);
            dumps.visible = window.visible_layout_string(LayoutFormat::V2, pane_base_index);
            dumps.window = Some(window.id);
        }
        context.window_layout.clone_from(&dumps.layout);
        context.window_active = Some(session.active_window == window.id);
        context.window_zoomed = window.zoomed_pane.is_some();
        context.window_modal_pane = window
            .modal_pane()
            .map(|pane| pane.to_string())
            .unwrap_or_default();
        context.window_visible_layout.clone_from(&dumps.visible);
        context.window_bell = window.panes.values().any(|pane| pane.bell);
        context.window_activity = window.activity_time;
        context.window_activity_alert = window.activity_flag;
        context.window_silence_alert = window.silence_flag;
        context.window_last = Some(session.last_window() == Some(window.id));
        context.window_start = Some(session.windows.first() == Some(&window.id));
        context.window_end = Some(session.windows.last() == Some(&window.id));
        context.window_stack_index = usize::from(context.window_last == Some(true));
        context.window_linked_sessions = 1;
        context
            .window_linked_sessions_list
            .clone_from(&session.name);
        if context.window_active == Some(true) {
            context.window_active_sessions = 1;
            context
                .window_active_sessions_list
                .clone_from(&session.name);
        }
        let Some(pane) = pane_id.and_then(|id| window.panes.get(&id)) else {
            return context;
        };
        context.history_limit = Some(self.history_limit_for_session(window.session));
        context.pane_active = Some(window.active_pane == pane.id);
        context.pane_dead = Some(pane.dead);
        context.pane_dead_status = pane.dead_status;
        context.pane_dead_time = pane
            .dead_time
            .and_then(|dead_time| i64::try_from(dead_time).ok());
        context.pane_input_off = Some(pane.input_off);
        context.pane_marked = Some(marked_pane == Some(pane.id));
        context.pane_marked_set = Some(marked_pane.is_some());
        context.pane_id = pane.id.to_string();
        context.pane_index = self.pane_index(window.id, pane.id).unwrap_or_default();
        context.pane_last = Some(window.last_pane() == Some(pane.id));
        context.pane_synchronized = self
            .state
            .pane_synchronize_panes_in(window.id, pane.id)
            .unwrap_or_default();
        context.pane_title.clone_from(&pane.title);
        context.pane_zoomed = window.zoomed_pane == Some(pane.id);
        if let Some(command) = self.pane_start_command(pane.id) {
            context.pane_start_command = command
                .iter()
                .map(|argument| quote_argument(argument))
                .collect::<Vec<_>>()
                .join(" ");
            context.pane_start_command_list = command
                .iter()
                .map(|argument| quote_single(argument.as_bytes()).to_string())
                .collect::<Vec<_>>()
                .join(" ");
        }
        if let Some(facts) = self.pane_runtime_facts(pane.id) {
            if !pane.dead {
                context
                    .pane_current_command
                    .clone_from(&facts.current_command);
            }
            if pane.dead {
                context.pane_current_path.clear();
            } else {
                context.pane_current_path.clone_from(&facts.current_path);
            }
            context.pane_path.clone_from(&facts.reported_path);
            context.pane_start_path.clone_from(&facts.start_path);
            context.pane_pid = facts.pid;
            context.pane_tty.clone_from(&facts.tty);
            context.pane_dead_signal.clone_from(&facts.dead_signal);
        } else {
            context.pane_current_path = match &pane.kind {
                PaneKind::Agent(agent) => agent
                    .cwd
                    .as_deref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                PaneKind::Editor(editor) => editor.cwd.clone(),
                PaneKind::Picker { .. }
                | PaneKind::Terminal
                | PaneKind::Browser(_)
                | PaneKind::View(_)
                | PaneKind::Simulator(_) => String::new(),
            };
            context
                .pane_start_path
                .clone_from(&context.pane_current_path);
        }
        match &pane.kind {
            PaneKind::Agent(agent) => {
                agent
                    .provider
                    .command_name()
                    .clone_into(&mut context.pane_current_command);
            }
            PaneKind::Terminal if context.pane_current_command.is_empty() => {
                context.pane_current_command = self.pane_command_fallback(pane.id);
            }
            _ => {}
        }
        let border_status = self.pane_border_status(window.id);
        if let Some(cell) = window
            .layout
            .pane_geometry_with_border(pane.id, border_status)
        {
            context.pane_unzoomed_width = Some(cell.sx);
            context.pane_unzoomed_height = Some(cell.sy);
        }
        let border_status = self.displayed_pane_border_status(window.id);
        let geometry = window
            .displayed_pane_cell(pane.id, border_status)
            .map(|geometry| (geometry.sx, geometry.sy, geometry.xoff, geometry.yoff));
        if let Some((sx, sy, xoff, yoff)) = geometry {
            context.pane_width = Some(sx);
            context.pane_height = Some(sy);
            context.pane_left = Some(xoff);
            context.pane_top = Some(yoff);
            context.pane_x = Some(xoff);
            context.pane_y = Some(yoff);
            context.pane_right = Some(xoff + i32::from(sx) - 1);
            context.pane_bottom = Some(yoff + i32::from(sy) - 1);
            context.pane_at_left = Some(xoff == 0);
            context.pane_at_top = Some(match border_status {
                PaneBorderStatus::Top => yoff == 1,
                PaneBorderStatus::Off | PaneBorderStatus::Bottom => yoff == 0,
            });
            context.pane_at_right = Some(xoff + i32::from(sx) == i32::from(width));
            context.pane_at_bottom = Some(match border_status {
                PaneBorderStatus::Bottom => {
                    yoff + i32::from(sy) == i32::from(height) - 1 && height > 0
                }
                PaneBorderStatus::Off | PaneBorderStatus::Top => {
                    yoff + i32::from(sy) == i32::from(height)
                }
            });
        }
        context.pane_z = window.pane_z(pane.id).and_then(|z| usize::try_from(z).ok());
        context.pane_floating = window.shows_floating(pane.id);
        context.pane_over_zoom = pane.over_zoom;
        context.pane_modal = window.modal_pane() == Some(pane.id);
        if context.pane_active == Some(true) {
            context.pane_flags.push('*');
        }
        if context.pane_last == Some(true) {
            context.pane_flags.push('-');
        }
        if context.pane_zoomed {
            context.pane_flags.push('Z');
        }
        if context.pane_floating {
            context.pane_flags.push('F');
        }
        if context.pane_over_zoom {
            context.pane_flags.push('A');
        }
        if context.pane_modal {
            context.pane_flags.push('O');
        }
        context
    }
}
