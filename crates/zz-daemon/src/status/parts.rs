use super::*;

impl StatusParts {
    pub(super) fn new(request: &StatusRequest) -> Self {
        let mut options = BTreeMap::new();
        let mut split = |source: &str| {
            let parts = zz_mux::format_parts(source);
            if source.len() >= 8192
                || parts
                    .iter()
                    .any(|part| !request.context.format_loops_available(part.required_loops))
            {
                return vec![StatusPart {
                    range: 0..source.len(),
                    value: None,
                }];
            }
            parts
                .iter()
                .map(|part| StatusPart {
                    range: part.range.clone(),
                    value: (!part.unconditional_clock
                        && !part.references.iter().any(|name| {
                            name == "*"
                                || name
                                    .strip_prefix("E:")
                                    .or_else(|| name.strip_prefix("T:"))
                                    .is_some_and(|name| {
                                        status_option_clock_dependent(
                                            request,
                                            name,
                                            &mut options,
                                            &mut BTreeSet::new(),
                                        )
                                    })
                        }))
                    .then(OnceLock::new),
                })
                .collect::<Vec<_>>()
        };
        let left = split(&request.formats.left);
        let right = split(&request.formats.right);
        let rows = request
            .row_formats
            .iter()
            .filter(|(index, _)| {
                usize::try_from(**index).is_ok_and(|index| {
                    request.formats.enabled
                        && index < usize::from(request.formats.lines).min(MAX_STATUS_ROWS)
                })
            })
            .map(|(index, source)| (*index, split(source)))
            .collect();
        let theme = theme_formats()
            .iter()
            .all(|source| split(source).iter().all(|part| part.value.is_some()))
            .then(OnceLock::new);
        let message_styles = [&request.message_styles.0, &request.message_styles.1]
            .into_iter()
            .all(|source| split(source).iter().all(|part| part.value.is_some()))
            .then(OnceLock::new);
        Self {
            left,
            right,
            rows,
            theme,
            message_styles,
        }
    }

    pub(super) fn retained_bytes(&self) -> usize {
        let mut bytes = std::mem::size_of::<Self>()
            .saturating_add(std::mem::size_of::<usize>().saturating_mul(2))
            .saturating_add(self.rows.len().saturating_mul(128));
        for parts in [&self.left, &self.right]
            .into_iter()
            .chain(self.rows.values())
        {
            bytes = bytes.saturating_add(
                parts
                    .capacity()
                    .saturating_mul(std::mem::size_of::<StatusPart>()),
            );
            for part in parts {
                bytes = bytes.saturating_add(
                    part.value
                        .as_ref()
                        .and_then(OnceLock::get)
                        .map_or(0, String::capacity),
                );
            }
        }
        if let Some((style, command_style)) = self.message_styles.as_ref().and_then(OnceLock::get) {
            bytes = bytes
                .saturating_add(style.capacity())
                .saturating_add(command_style.capacity());
        }
        bytes
    }
}

fn status_option_clock_dependent(
    request: &StatusRequest,
    name: &str,
    known: &mut BTreeMap<String, bool>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    if let Some(dependent) = known.get(name) {
        return *dependent;
    }
    if visiting.len() >= 64 || !visiting.insert(name.to_owned()) {
        return true;
    }
    let context = &request.context;
    let options = &request.option_snapshot;
    let scopes = [
        (
            context.session_id.as_str(),
            context.window_id.as_str(),
            context.pane_id.as_str(),
        ),
        ("", "", ""),
    ]
    .into_iter()
    .chain(options.sessions.keys().map(|id| (id.as_str(), "", "")))
    .chain(options.windows.keys().map(|id| ("", id.as_str(), "")))
    .chain(options.panes.keys().map(|id| ("", "", id.as_str())));
    let mut sources = BTreeSet::new();
    let mut dependent = scopes.into_iter().any(|(session, window, pane)| {
        options
            .lookup(session, window, pane, name)
            .is_none_or(|source| {
                sources.insert(source);
                false
            })
    });
    if !dependent {
        dependent = sources.iter().any(|source| {
            zz_mux::format_parts(source).iter().any(|part| {
                part.unconditional_clock
                    || part.references.iter().any(|name| {
                        name == "*"
                            || name
                                .strip_prefix("E:")
                                .or_else(|| name.strip_prefix("T:"))
                                .is_some_and(|name| {
                                    status_option_clock_dependent(request, name, known, visiting)
                                })
                    })
            })
        });
    }
    visiting.remove(name);
    known.insert(name.to_owned(), dependent);
    dependent
}

pub(super) fn expand_status_parts(
    source: &str,
    context: &StatusContext,
    hooks: &mut DaemonFormatHooks<'_>,
    parts: Option<&[StatusPart]>,
) -> String {
    let Some(parts) = parts.filter(|_| status_parts_enabled()) else {
        return expand_status(source, context, hooks);
    };
    let mut output = String::new();
    for part in parts {
        let source = &source[part.range.clone()];
        if let Some(value) = &part.value {
            output.push_str(value.get_or_init(|| expand_status(source, context, hooks)));
        } else {
            output.push_str(&expand_status(source, context, hooks));
        }
    }
    output
}

pub(super) fn status_parts_enabled() -> bool {
    zz_mux::borrowed_formats_enabled()
}

pub(crate) fn completed_status_bytes(
    request: &StatusRequest,
    names: &[String],
    callbacks: &[Option<String>],
    status: &StatusLine,
) -> usize {
    completed_status_bytes_with_capture_bytes(
        request,
        names,
        callbacks,
        status,
        request.option_snapshot.retained_bytes(),
        request.context.retained_bytes(),
    )
}

pub(super) fn completed_status_bytes_with_capture_bytes(
    request: &StatusRequest,
    names: &[String],
    callbacks: &[Option<String>],
    status: &StatusLine,
    option_bytes: usize,
    context_bytes: usize,
) -> usize {
    let mut bytes = std::mem::size_of::<CompletedStatus>()
        .saturating_add(std::mem::size_of::<StatusRequest>())
        .saturating_add(context_bytes)
        .saturating_add(option_bytes)
        .saturating_add(std::mem::size_of::<FormatHookFacts>())
        .saturating_add(std::mem::size_of::<Vec<(RawText, Option<RawText>)>>())
        .saturating_add(
            request
                .environment
                .capacity()
                .saturating_mul(std::mem::size_of::<(RawText, Option<RawText>)>()),
        )
        .saturating_add(std::mem::size_of::<String>())
        .saturating_add(request.default_terminal.capacity())
        .saturating_add(std::mem::size_of::<usize>().saturating_mul(4));
    for (name, value) in request.environment.iter() {
        bytes = bytes
            .saturating_add(zz_mux::cloned_raw_text_bytes(name))
            .saturating_add(value.as_ref().map_or(0, zz_mux::cloned_raw_text_bytes));
    }
    for value in [
        &request.formats.left,
        &request.formats.right,
        &request.formats.style,
        &request.formats.background,
        &request.formats.foreground,
        &request.formats.left_style,
        &request.formats.right_style,
        &request.message_styles.0,
        &request.message_styles.1,
        &status.left,
        &status.right,
        &status.title,
        &status.base_style,
        &status.message_style,
        &status.message_command_style,
    ]
    .into_iter()
    .chain(request.title_format.iter())
    .chain(names)
    .chain(callbacks.iter().flatten())
    .chain(status.rows.iter())
    {
        bytes = bytes
            .saturating_add(std::mem::size_of::<String>())
            .saturating_add(value.capacity());
    }
    for value in request.row_formats.values() {
        bytes = bytes
            .saturating_add(std::mem::size_of::<(u32, String)>())
            .saturating_add(value.capacity());
    }
    for value in request.references.iter() {
        bytes = bytes
            .saturating_add(std::mem::size_of::<String>())
            .saturating_add(value.capacity());
    }
    for border in request.pane_borders.iter().chain(&status.pane_borders) {
        bytes = bytes
            .saturating_add(std::mem::size_of::<zz_protocol::PaneBorderPresentation>())
            .saturating_add(border.style.capacity())
            .saturating_add(border.window_style.capacity())
            .saturating_add(border.window_active_style.capacity());
    }
    bytes.saturating_mul(2).saturating_add(4096)
}

impl CompletedStatus {
    pub(super) fn same_templates(&self, request: &StatusRequest) -> bool {
        Arc::ptr_eq(&self.option_snapshot, &request.option_snapshot)
            && (Arc::ptr_eq(&self.references, &request.references)
                || self.references == request.references)
            && (Arc::ptr_eq(&self.formats, &request.formats) || self.formats == request.formats)
            && (Arc::ptr_eq(&self.row_formats, &request.row_formats)
                || self.row_formats == request.row_formats)
            && (Arc::ptr_eq(&self.title_format, &request.title_format)
                || self.title_format == request.title_format)
            && (Arc::ptr_eq(&self.message_styles, &request.message_styles)
                || self.message_styles == request.message_styles)
    }

    pub(super) fn same_lookup_scope(&self, request: &StatusRequest) -> bool {
        self.context.session_id == request.context.session_id
            && self.context.window_id == request.context.window_id
            && self.context.pane_id == request.context.pane_id
            && self.context.has_captured_environment() == request.context.has_captured_environment()
    }

    pub(super) fn matches(
        &self,
        request: &StatusRequest,
        callbacks: &[Option<String>],
        now: i64,
        clock: bool,
    ) -> bool {
        self.client == request.client
            && (!clock || self.now == now)
            && self.message_line == request.message_line
            && self.customized == request.customized
            && self.client_scheme == request.client_scheme
            && self.pane_borders == request.pane_borders
            && (std::ptr::eq(self.callbacks.as_slice(), callbacks) || self.callbacks == callbacks)
            && if clock {
                Arc::ptr_eq(&self.context, &request.context)
                    || self.context.same_detached(&request.context)
            } else {
                self.context.same_detached_data(&request.context)
                    && self.same_lookup_scope(request)
                    && self.startup == request.startup
                    && self.environment_identity.as_ptr() == Arc::as_ptr(&request.environment)
                    && self.default_terminal_identity.as_ptr()
                        == Arc::as_ptr(&request.default_terminal)
            }
    }

    pub(super) fn new(
        request: &StatusRequest,
        callback_names: Vec<String>,
        callbacks: Vec<Option<String>>,
        now: i64,
        status: Arc<StatusLine>,
        identity: Option<&Arc<StatusRequest>>,
        option_bytes: usize,
        parts: Option<Arc<StatusParts>>,
        context_bytes: usize,
    ) -> Self {
        Self {
            client: request.client,
            formats: request.formats.clone(),
            row_formats: request.row_formats.clone(),
            option_snapshot: Arc::clone(&request.option_snapshot),
            references: Arc::clone(&request.references),
            message_line: request.message_line,
            customized: request.customized,
            title_format: request.title_format.clone(),
            context: request.context.clone(),
            client_scheme: request.client_scheme,
            message_styles: request.message_styles.clone(),
            pane_borders: request.pane_borders.clone(),
            callback_names,
            callbacks,
            now,
            status,
            fact_identity: Arc::downgrade(&request.facts),
            request_identity: identity.map_or_else(Weak::new, Arc::downgrade),
            option_bytes,
            parts,
            startup: request.startup,
            environment_identity: Arc::downgrade(&request.environment),
            default_terminal_identity: Arc::downgrade(&request.default_terminal),
            context_bytes,
            cycle_tick: None,
        }
    }
}

pub(crate) fn status_cache_callbacks(request: &StatusRequest) -> Option<Vec<String>> {
    if !request.modes.is_empty()
        || request.references.contains("*")
        || status_line_templates(
            &request.formats,
            &request.row_formats,
            request.title_format.as_deref(),
            &request.message_styles,
        )
        .any(|template| {
            zz_mux::format_references(template)
                .iter()
                .any(|name| !request.references.contains(name))
        })
    {
        return None;
    }
    let mut callbacks = Vec::new();
    for name in request.references.iter().map(String::as_str) {
        if matches!(
            name,
            "session_attached"
                | "session_attached_list"
                | "session_many_attached"
                | "window_active_clients"
                | "window_active_clients_list"
        ) {
            return None;
        }
        let option_name = name
            .strip_prefix("E:")
            .or_else(|| name.strip_prefix("T:"))
            .unwrap_or(name);
        if zz_mux::format_variable_is_captured(name)
            || matches!(name, "loop_index" | "loop_last_flag")
            || request
                .option_snapshot
                .lookup(
                    &request.context.session_id,
                    &request.context.window_id,
                    &request.context.pane_id,
                    option_name,
                )
                .is_some()
        {
            continue;
        }
        if (name.starts_with("client_")
            && zz_mux::delegated_format_variable_names().any(|known| known == name))
            || matches!(
                name,
                "window_bigger"
                    | "window_offset_x"
                    | "window_offset_y"
                    | "window_cell_height"
                    | "window_cell_width"
            )
        {
            callbacks.push(name.to_owned());
        } else if zz_mux::format_variable_is_known(name)
            || matches!(
                name,
                "zz_version"
                    | "zz_revision"
                    | "zz_exit_rule"
                    | "zz_owned_version"
                    | "zz_owned_epoch"
                    | "pane_kind"
                    | "agent_state"
                    | "agent_pending_permission"
                    | "agent_parent"
                    | "browser_url"
                    | "pane_last_command_status"
                    | "pane_status"
                    | "pane_status_kind"
                    | "pane_status_progress"
                    | "pane_status_app"
                    | "pane_status_title"
                    | "pane_status_message"
                    | "pane_status_raw_title"
                    | "pane_status_raw_message"
                    | "pane_status_reported"
            )
            || COPY_MODE_CONTEXT_FORMATS.contains(&name)
            || LIST_CLIENTS_CONTEXT_FORMATS.contains(&name)
            || SHOW_MESSAGES_CONTEXT_FORMATS.contains(&name)
            || name.starts_with("E:")
            || name.starts_with("T:")
            || !request.context.has_captured_environment()
        {
            return None;
        }
    }
    Some(callbacks)
}

pub(super) fn status_callback_values(
    request: &StatusRequest,
    names: &[String],
) -> Vec<Option<String>> {
    let mut hooks = DaemonFormatHooks::command(request.facts.as_ref());
    names
        .iter()
        .map(|name| {
            if matches!(name.as_str(), "window_cell_height" | "window_cell_width") {
                Some(window_cell_pixels(
                    request.facts.client.as_ref(),
                    name == "window_cell_width",
                ))
            } else {
                hooks.variable(name, &request.context)
            }
        })
        .collect()
}
