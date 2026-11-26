use super::*;

pub(super) const BORDER_FORMAT_CACHE_BYTES: usize = 1024 * 1024;

pub(super) const BORDER_FORMAT_TEMPLATES: [&str; 4] = [
    "#{E:pane-border-style}",
    "#{E:pane-active-border-style}",
    "#{E:window-style}",
    "#{E:window-active-style}",
];

pub(super) struct CachedBorderPresentations {
    revision: (u64, u64, u64, u64),
    second: u64,
    client: ClientId,
    session: SessionId,
    window: WindowId,
    option_snapshot: Arc<zz_mux::StatusRowVariables>,
    references: Arc<BTreeSet<String>>,
    pub(super) callbacks: Vec<String>,
    pub(super) panes: Vec<CachedBorderPane>,
    presentations: Arc<Vec<zz_protocol::PaneBorderPresentation>>,
    pub(super) clock_dependent: bool,
}

pub(super) struct CachedBorderPane {
    context: zz_mux::StatusContext<'static>,
    pub(super) callback_values: Vec<Option<String>>,
    presentation: zz_protocol::PaneBorderPresentation,
    pub(super) pane_in_mode: Option<usize>,
}

impl CachedBorderPresentations {
    pub(super) fn matches(
        &self,
        revision: (u64, u64, u64, u64),
        second: u64,
        target: (ClientId, SessionId, WindowId),
        option_snapshot: &Arc<zz_mux::StatusRowVariables>,
    ) -> bool {
        (self.revision.0, self.revision.1, self.revision.2) == (revision.0, revision.1, revision.2)
            && (!self.clock_dependent || (self.revision.3 == revision.3 && self.second == second))
            && (self.client, self.session, self.window) == target
            && Arc::ptr_eq(&self.option_snapshot, option_snapshot)
    }

    fn same_callbacks(&self, facts: &dyn crate::status::FormatFactSource) -> bool {
        if !self.panes.iter().all(|pane| {
            pane.pane_in_mode
                .is_none_or(|count| facts.pane_in_mode_count(pane.presentation.pane) == count)
        }) {
            return false;
        }
        if self.callbacks.is_empty() {
            return true;
        }
        let mut hooks = DaemonFormatHooks::command(facts);
        self.panes.iter().all(|pane| {
            self.callbacks
                .iter()
                .zip(&pane.callback_values)
                .all(|(name, expected)| hooks.variable(name, &pane.context) == *expected)
        })
    }

    fn presentations(&self) -> Arc<Vec<zz_protocol::PaneBorderPresentation>> {
        Arc::clone(&self.presentations)
    }

    pub(super) fn retained_bytes(&self) -> usize {
        let mut bytes =
            std::mem::size_of::<Self>().saturating_add(self.option_snapshot.retained_bytes());
        for reference in self.references.iter().chain(&self.callbacks) {
            bytes = bytes
                .saturating_add(std::mem::size_of::<String>())
                .saturating_add(reference.capacity());
        }
        bytes = bytes.saturating_add(
            self.panes
                .capacity()
                .saturating_mul(std::mem::size_of::<CachedBorderPane>()),
        );
        for pane in &self.panes {
            bytes = bytes
                .saturating_add(pane.context.retained_bytes())
                .saturating_add(pane.presentation.style.capacity())
                .saturating_add(pane.presentation.window_style.capacity())
                .saturating_add(pane.presentation.window_active_style.capacity())
                .saturating_add(
                    pane.callback_values
                        .capacity()
                        .saturating_mul(std::mem::size_of::<Option<String>>()),
                );
            for value in pane.callback_values.iter().flatten() {
                bytes = bytes.saturating_add(value.capacity());
            }
        }
        bytes = bytes.saturating_add(
            self.presentations
                .capacity()
                .saturating_mul(std::mem::size_of::<zz_protocol::PaneBorderPresentation>()),
        );
        for presentation in self.presentations.iter() {
            bytes = bytes.saturating_add(presentation.style.capacity());
        }
        bytes.saturating_mul(2).saturating_add(4096)
    }
}

fn border_format_callbacks(
    engine: &MuxEngine,
    context: &zz_mux::StatusContext,
    option_snapshot: &zz_mux::StatusRowVariables,
    references: &BTreeSet<String>,
) -> Option<Vec<String>> {
    let needs = engine.cached_format_needs(BORDER_FORMAT_TEMPLATES);
    if references.contains("*")
        || [
            FormatNeeds::SESSIONS,
            FormatNeeds::WINDOWS,
            FormatNeeds::PANES,
            FormatNeeds::OPTIONS,
        ]
        .iter()
        .any(|need| needs.contains(*need))
    {
        return None;
    }
    let mut callbacks = Vec::new();
    for name in references.iter().map(String::as_str) {
        if matches!(name, "window_active_clients" | "window_active_clients_list") {
            return None;
        }
        let factual = (name.starts_with("client_")
            && zz_mux::delegated_format_variable_names().any(|known| known == name))
            || matches!(
                name,
                "pane_in_mode"
                    | "pane_mode"
                    | "pane_unseen_changes"
                    | "pane_pipe"
                    | "pane_pipe_pid"
                    | "session_last_attached"
                    | "session_attached"
                    | "session_attached_list"
                    | "session_many_attached"
                    | "window_bigger"
                    | "window_offset_x"
                    | "window_offset_y"
                    | "window_cell_height"
                    | "window_cell_width"
            );
        if factual {
            callbacks.push(name.to_owned());
            continue;
        }
        let option_name = name
            .strip_prefix("E:")
            .or_else(|| name.strip_prefix("T:"))
            .unwrap_or(name);
        if name != "config_files"
            && (zz_mux::format_variable_is_captured(name)
                || option_snapshot
                    .lookup(
                        &context.session_id,
                        &context.window_id,
                        &context.pane_id,
                        option_name,
                    )
                    .is_some()
                || (option_name.starts_with('@')
                    && engine
                        .format_user_option(
                            &context.pane_id,
                            &context.window_id,
                            &context.session_id,
                            option_name,
                        )
                        .is_some()))
        {
            continue;
        }
        return None;
    }
    Some(callbacks)
}

fn border_format_clock_dependent(
    engine: &MuxEngine,
    context: &zz_mux::StatusContext,
    option_snapshot: &zz_mux::StatusRowVariables,
    references: &BTreeSet<String>,
) -> bool {
    BORDER_FORMAT_TEMPLATES
        .into_iter()
        .any(zz_mux::format_clock_dependent)
        || references.iter().any(|name| {
            if name == "*" || name.starts_with("T:") {
                return true;
            }
            let Some(name) = name.strip_prefix("E:") else {
                return false;
            };
            if let Some(source) = option_snapshot.lookup(
                &context.session_id,
                &context.window_id,
                &context.pane_id,
                name,
            ) {
                return zz_mux::format_clock_dependent(&source);
            }
            engine
                .format_user_option(
                    &context.pane_id,
                    &context.window_id,
                    &context.session_id,
                    name,
                )
                .is_none_or(zz_mux::format_clock_dependent)
        })
}

#[cfg(test)]
thread_local! {
    pub(super) static BORDER_FORMAT_EXPANSIONS: Cell<usize> = const { Cell::new(0) };
}

fn border_format_style(
    format: &str,
    context: &zz_mux::StatusContext,
    hooks: &mut DaemonFormatHooks<'_>,
) -> String {
    #[cfg(test)]
    BORDER_FORMAT_EXPANSIONS.with(|count| count.set(count.get() + 1));
    crate::status::expand_style(format, context, hooks)
}

fn window_style_presentation(
    engine: &MuxEngine,
    active: bool,
    context: &zz_mux::StatusContext,
    hooks: &mut DaemonFormatHooks<'_>,
) -> (String, String) {
    if !engine.has_window_style_settings() {
        return (String::new(), String::new());
    }
    let window_style = crate::status::expand_style(BORDER_FORMAT_TEMPLATES[2], context, hooks);
    let window_active_style = if active {
        crate::status::expand_style(BORDER_FORMAT_TEMPLATES[3], context, hooks)
    } else {
        String::new()
    };
    (window_style, window_active_style)
}

pub(super) fn border_presentations(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    facts: &dyn crate::status::FormatFactSource,
) -> Arc<Vec<zz_protocol::PaneBorderPresentation>> {
    let second = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    border_presentations_at(inner, client, session, facts, second)
}

pub(super) fn cached_live_border_presentations(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    window: Option<WindowId>,
    revision: (u64, u64, u64, u64),
    option_snapshot: &Arc<zz_mux::StatusRowVariables>,
    second: impl FnOnce() -> u64,
) -> Option<Arc<Vec<zz_protocol::PaneBorderPresentation>>> {
    let window = window?;
    inner.engine.state.windows.get(&window)?;
    let cache = inner.publication.border_presentations_cache.lock();
    let cached = cache.as_ref()?;
    let second = if cached.clock_dependent {
        second()
    } else {
        cached.second
    };
    (cached.callbacks.is_empty()
        && cached.matches(revision, second, (client, session, window), option_snapshot)
        && cached.panes.iter().all(|pane| {
            pane.pane_in_mode.is_none_or(|count| {
                pane_in_mode_count_from_parts(
                    pane.presentation.pane,
                    &inner.modes.pane_modes,
                    &inner.clients,
                    &inner.terminals,
                ) == count
            })
        }))
    .then(|| cached.presentations())
}

pub(super) fn border_presentations_at(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    facts: &dyn crate::status::FormatFactSource,
    second: u64,
) -> Arc<Vec<zz_protocol::PaneBorderPresentation>> {
    let revision = inner.engine.format_cache_revision();
    let Some(session_state) = inner.engine.state.sessions.get(&session) else {
        return Arc::default();
    };
    let window = client_focused_window(inner, client, session_state);
    let Some(window_state) = inner.engine.state.windows.get(&window) else {
        return Arc::default();
    };
    let option_snapshot = inner.engine.cached_format_option_snapshot();
    {
        let cache = inner.publication.border_presentations_cache.lock();
        if let Some(cached) = cache.as_ref()
            && cached.matches(
                revision,
                second,
                (client, session, window),
                &option_snapshot,
            )
            && cached.same_callbacks(facts)
        {
            return cached.presentations();
        }
    }
    let references = inner
        .engine
        .cached_format_references_for_templates(BORDER_FORMAT_TEMPLATES);
    let contexts = inner
        .engine
        .format_context_snapshot(FormatClient::Attached(session));
    let first_context =
        contexts.status_context(Some(session), Some(window), Some(window_state.active_pane));
    let Some(mut callbacks) =
        border_format_callbacks(&inner.engine, &first_context, &option_snapshot, &references)
    else {
        *inner.publication.border_presentations_cache.lock() = None;
        return uncached_border_presentations(inner, client, session, facts);
    };
    let pane_in_mode = callbacks.iter().any(|name| name == "pane_in_mode");
    callbacks.retain(|name| name != "pane_in_mode");
    let mut hooks = DaemonFormatHooks::command(facts).with_option_engine(&inner.engine);
    let mut clock_dependent = false;
    let panes: Vec<CachedBorderPane> = window_state
        .panes
        .keys()
        .take(zz_protocol::MAX_PANE_BORDER_PRESENTATIONS)
        .map(|pane| {
            let context = contexts.status_context(Some(session), Some(window), Some(*pane));
            clock_dependent |= border_format_clock_dependent(
                &inner.engine,
                &context,
                &option_snapshot,
                &references,
            );
            let pane_in_mode = pane_in_mode.then(|| facts.pane_in_mode_count(*pane));
            if let Some(count) = pane_in_mode {
                hooks.set_pane_in_mode_count(*pane, count);
            }
            let active = *pane == window_state.active_pane;
            let format = BORDER_FORMAT_TEMPLATES[usize::from(active)];
            let style = border_format_style(format, &context, &mut hooks);
            let (window_style, window_active_style) =
                window_style_presentation(&inner.engine, active, &context, &mut hooks);
            let presentation = zz_protocol::PaneBorderPresentation {
                pane: *pane,
                style,
                window_style,
                window_active_style,
            };
            let mut callback_context = zz_mux::StatusContext::default();
            callback_context.session_id.clone_from(&context.session_id);
            callback_context.window_id.clone_from(&context.window_id);
            callback_context.pane_id.clone_from(&context.pane_id);
            let callback_values = callbacks
                .iter()
                .map(|name| hooks.variable(name, &context))
                .collect();
            CachedBorderPane {
                context: callback_context,
                callback_values,
                presentation,
                pane_in_mode,
            }
        })
        .collect();
    let presentations = Arc::new(panes.iter().map(|pane| pane.presentation.clone()).collect());
    let cached = CachedBorderPresentations {
        revision,
        second,
        client,
        session,
        window,
        option_snapshot,
        references,
        callbacks,
        panes,
        presentations,
        clock_dependent,
    };
    let result = cached.presentations();
    *inner.publication.border_presentations_cache.lock() =
        (cached.retained_bytes() <= BORDER_FORMAT_CACHE_BYTES).then_some(cached);
    result
}

pub(super) fn uncached_border_presentations(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    facts: &dyn crate::status::FormatFactSource,
) -> Arc<Vec<zz_protocol::PaneBorderPresentation>> {
    let Some(session_state) = inner.engine.state.sessions.get(&session) else {
        return Arc::default();
    };
    let window = client_focused_window(inner, client, session_state);
    let Some(window_state) = inner.engine.state.windows.get(&window) else {
        return Arc::default();
    };
    let mut hooks = DaemonFormatHooks::command(facts).with_option_engine(&inner.engine);
    let contexts = inner
        .engine
        .format_context_snapshot(FormatClient::Attached(session));
    Arc::new(
        window_state
            .panes
            .keys()
            .take(zz_protocol::MAX_PANE_BORDER_PRESENTATIONS)
            .map(|pane| {
                let context = contexts.status_context(Some(session), Some(window), Some(*pane));
                let active = *pane == window_state.active_pane;
                let format = BORDER_FORMAT_TEMPLATES[usize::from(active)];
                let style = border_format_style(format, &context, &mut hooks);
                let (window_style, window_active_style) =
                    window_style_presentation(&inner.engine, active, &context, &mut hooks);
                zz_protocol::PaneBorderPresentation {
                    pane: *pane,
                    style,
                    window_style,
                    window_active_style,
                }
            })
            .collect(),
    )
}
