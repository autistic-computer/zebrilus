use super::*;

pub(super) fn status_targets(
    inner: &ServerState,
    sessions: Option<&BTreeSet<SessionId>>,
    clients: Option<&BTreeSet<ClientId>>,
) -> Vec<ClientId> {
    inner
        .clients
        .iter()
        .filter_map(|(id, client)| client.subscriber.as_ref().map(|_| id))
        .copied()
        .filter(|client| {
            inner
                .client(*client)
                .and_then(|c| c.ctrl_subscriptions.as_ref())
                .is_none_or(|subscription| subscription.status)
                && clients.is_none_or(|clients| clients.contains(client))
                && sessions.is_none_or(|sessions| {
                    client_attached_session(inner, *client)
                        .is_some_and(|session| sessions.contains(&session))
                })
        })
        .collect()
}

pub(super) fn status_request_with_selected_options(
    inner: &ServerState,
    client: ClientId,
    mut option_snapshot: Option<Arc<zz_mux::StatusRowVariables>>,
    startup_ready: bool,
    job_needs: FormatNeeds,
) -> Arc<StatusRequest> {
    let revision = (zz_mux::borrowed_formats_enabled()
        && inner.client(client).and_then(|c| c.kind) != Some(ClientKind::Control)
        && job_needs.is_empty()
        && inner
            .client(client)
            .is_none_or(|c| c.copy_session.is_none())
        && inner
            .client(client)
            .is_none_or(|c| c.command_output.is_none()))
    .then(|| inner.engine.format_cache_revision());
    let attached = client_attached_session(inner, client);
    let window = attached
        .and_then(|session| inner.engine.state.sessions.get(&session))
        .map(|session| client_focused_window(inner, client, session));
    let mut reuse_admission = false;
    if let Some(revision) = revision {
        let mut cache = inner.publication.status_preparation_cache.lock();
        let options = option_snapshot.get_or_insert_with(|| {
            cache
                .as_ref()
                .filter(|cached| {
                    cached.request.client == client
                        && (cached.revision.0, cached.revision.1, cached.revision.2)
                            == (revision.0, revision.1, revision.2)
                        && inner
                            .engine
                            .format_cache_identity_matches(&cached.engine_identity)
                })
                .map_or_else(
                    || inner.engine.cached_format_option_snapshot(),
                    |cached| Arc::clone(&cached.request.option_snapshot),
                )
        });
        let cached_matches = cache.as_ref().is_some_and(|cached| {
            cached.matches(
                inner,
                client,
                attached,
                window,
                revision,
                options,
                startup_ready,
            )
        });
        let context = if cached_matches {
            None
        } else {
            cache.as_ref().and_then(|cached| {
                reuse_admission = cached.matches(
                    inner,
                    client,
                    attached,
                    window,
                    (revision.0, revision.1, revision.2, cached.revision.3),
                    options,
                    startup_ready,
                );
                if !reuse_admission {
                    return None;
                }
                let parameters = status_parameters(inner, attached, options);
                cached_live_status_context(
                    inner,
                    attached,
                    window,
                    parameters.needs,
                    &cached.request.references,
                    cached.config_files_requested,
                )
                .filter(|context| context.same_detached_data(&cached.request.context))
                .map(Arc::new)
            })
        };
        if let Some(cached) = cache.as_ref()
            && (cached_matches || context.is_some())
        {
            let request = Arc::clone(&cached.request);
            let pane_borders = attached.map_or_else(
                || Arc::clone(&request.pane_borders),
                |session| {
                    cached_live_border_presentations(
                        inner,
                        client,
                        session,
                        window,
                        revision,
                        options,
                        || {
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map_or(0, |duration| duration.as_secs())
                        },
                    )
                    .unwrap_or_else(|| {
                        let facts = readonly_borrowed_format_hook_facts(
                            inner,
                            CommandFormatSeed {
                                client: cached
                                    .border_client
                                    .then(|| request.facts.client.clone())
                                    .flatten(),
                                invoking: Some(client),
                            },
                        );
                        border_presentations(inner, client, session, &facts)
                    })
                },
            );
            if context.is_none() && Arc::ptr_eq(&request.pane_borders, &pane_borders) {
                return request;
            }
            let request = Arc::new(StatusRequest {
                context: context.unwrap_or_else(|| Arc::clone(&request.context)),
                pane_borders,
                ..request.as_ref().clone()
            });
            let cached = cache.as_mut().unwrap();
            let same_storage = request.context.same_detached_data(&cached.request.context)
                && request.context.session_id.capacity()
                    == cached.request.context.session_id.capacity()
                && request.context.window_id.capacity()
                    == cached.request.context.window_id.capacity()
                && request.context.pane_id.capacity() == cached.request.context.pane_id.capacity()
                && Arc::ptr_eq(&request.pane_borders, &cached.request.pane_borders);
            cached.revision = revision;
            cached.request = Arc::clone(&request);
            if !same_storage {
                cached.retained_bytes = cached.retained_bytes();
            }
            if cached.retained_bytes > STATUS_PREPARATION_MAX_BYTES {
                *cache = None;
            }
            return request;
        }
    }
    let option_snapshot =
        option_snapshot.unwrap_or_else(|| inner.engine.cached_format_option_snapshot());
    let facts = (inner.client(client).and_then(|c| c.kind) == Some(ClientKind::Control))
        .then(|| format_hook_facts(inner));
    let snapshot = facts.is_some().then(|| inner.engine.state.snapshot());
    let request = Arc::new(status_request_with_facts(
        inner,
        client,
        snapshot.as_ref(),
        option_snapshot,
        facts,
        startup_ready,
        job_needs,
        &mut BTreeMap::new(),
    ));
    if let Some(revision) = revision {
        let reusable = reuse_admission || {
            let parameters = status_parameters(inner, attached, &request.option_snapshot);
            let client_dependencies_safe = !parameters.client_fact_selection.full
                && parameters.client_references.iter().all(|name| {
                    (!name.starts_with("client_")
                        || matches!(name.as_str(), "client_width" | "client_colours"))
                        && !matches!(name.as_str(), "window_cell_height" | "window_cell_width")
                });
            crate::status::status_cache_callbacks(&request).is_some_and(|names| {
                names.iter().all(|name| {
                    matches!(
                        name.as_str(),
                        "client_width"
                            | "client_colours"
                            | "window_bigger"
                            | "window_offset_x"
                            | "window_offset_y"
                    )
                })
            }) && client_dependencies_safe
                && !request.references.iter().any(|name| name.starts_with('@'))
        };
        let cached = reusable.then(|| {
            CachedStatusPreparation::new(inner, attached, window, revision, request.clone())
        });
        *inner.publication.status_preparation_cache.lock() =
            cached.filter(|cached| cached.retained_bytes <= STATUS_PREPARATION_MAX_BYTES);
    }
    request
}

pub(super) const STATUS_PREPARATION_MAX_BYTES: usize = 1024 * 1024;

pub(super) struct CachedStatusPreparation {
    pub(super) revision: (u64, u64, u64, u64),
    attached: Option<SessionId>,
    window: Option<WindowId>,
    size: Option<(u16, u16)>,
    kind: Option<ClientKind>,
    terminal: bool,
    features: Option<u32>,
    pub(super) environment: Option<Weak<ClientEnvironmentBlob>>,
    pub(super) config_files: String,
    pub(super) request: Arc<StatusRequest>,
    viewport_requested: bool,
    border_client: bool,
    pub(super) retained_bytes: usize,
    engine_identity: Weak<()>,
    pub(super) config_files_requested: bool,
}

impl CachedStatusPreparation {
    pub(super) fn new(
        inner: &ServerState,
        attached: Option<SessionId>,
        window: Option<WindowId>,
        revision: (u64, u64, u64, u64),
        request: Arc<StatusRequest>,
    ) -> Self {
        let client = request.client;
        let viewport_requested = status_parameters(inner, attached, &request.option_snapshot)
            .client_fact_selection
            .viewport;
        let border_references = inner
            .engine
            .cached_format_references_for_templates(BORDER_FORMAT_TEMPLATES);
        let border_client = border_references.iter().any(|name| {
            name.starts_with("client_")
                || matches!(
                    name.as_str(),
                    "window_bigger" | "window_offset_x" | "window_offset_y"
                )
        });
        let config_files_requested = !zz_mux::borrowed_formats_enabled()
            || !request.modes.is_empty()
            || request.references.contains("*")
            || request.references.contains("config_files")
            || border_references.contains("*")
            || border_references.contains("config_files");
        let mut cached = Self {
            revision,
            attached,
            window,
            size: inner.client(client).and_then(|c| c.size),
            kind: inner.client(client).and_then(|c| c.kind),
            terminal: inner.client(client).is_some_and(|c| c.has_terminal),
            features: inner.client(client).and_then(|c| c.features),
            environment: inner
                .client(client)
                .and_then(|c| c.environment.as_ref())
                .map(Arc::downgrade),
            config_files: if config_files_requested {
                inner.config.config_files.clone()
            } else {
                String::new()
            },
            request,
            viewport_requested,
            border_client,
            retained_bytes: 0,
            engine_identity: inner.engine.format_cache_identity(),
            config_files_requested,
        };
        cached.retained_bytes = cached.retained_bytes();
        cached
    }

    pub(super) fn matches(
        &self,
        inner: &ServerState,
        client: ClientId,
        attached: Option<SessionId>,
        window: Option<WindowId>,
        revision: (u64, u64, u64, u64),
        options: &Arc<zz_mux::StatusRowVariables>,
        startup_ready: bool,
    ) -> bool {
        self.revision == revision
            && self.request.client == client
            && self.attached == attached
            && self.window == window
            && Arc::ptr_eq(&self.request.option_snapshot, options)
            && self.request.startup != startup_ready
            && self.request.client_scheme == inner.client(client).and_then(|c| c.color_scheme)
            && self.size == inner.client(client).and_then(|c| c.size)
            && self.kind == inner.client(client).and_then(|c| c.kind)
            && self.terminal == inner.client(client).is_some_and(|c| c.has_terminal)
            && self.features == inner.client(client).and_then(|c| c.features)
            && (!self.config_files_requested || self.config_files == inner.config.config_files)
            && (!self.viewport_requested
                || (self.size.is_some()
                    && self
                        .request
                        .facts
                        .client
                        .as_ref()
                        .and_then(|c| c.viewport)
                        .is_some_and(|viewport| !viewport.bigger()))
                || {
                    let current = (self.kind == Some(ClientKind::Interactive) && self.terminal)
                        .then(|| {
                            client_viewport_facts_from_source(
                                &ClientFormatSource::from_inner(inner),
                                client,
                                attached?,
                                window?,
                            )
                        })
                        .flatten();
                    let values =
                        |viewport: ClientViewportFacts| (viewport.bigger(), viewport.offsets());
                    current.map(values)
                        == self
                            .request
                            .facts
                            .client
                            .as_ref()
                            .and_then(|c| c.viewport)
                            .map(values)
                })
            && match (
                &self.environment,
                inner.client(client).and_then(|c| c.environment.as_ref()),
            ) {
                (None, None) => true,
                (Some(cached), Some(current)) => cached.as_ptr() == Arc::as_ptr(current),
                _ => false,
            }
    }

    pub(super) fn retained_bytes(&self) -> usize {
        let environment = self.environment.as_ref().map_or(0, |_| {
            std::mem::size_of::<ClientEnvironmentBlob>()
                .saturating_add(std::mem::size_of::<usize>().saturating_mul(2))
        });
        let job_environment =
            self.request
                .environment
                .iter()
                .fold(0_usize, |bytes, (name, value)| {
                    bytes
                        .saturating_add(zz_mux::cloned_raw_text_bytes(name))
                        .saturating_add(value.as_ref().map_or(0, zz_mux::cloned_raw_text_bytes))
                        .saturating_add(128)
                });
        crate::status::completed_status_bytes(
            &self.request,
            &[],
            &[],
            &zz_protocol::StatusLine::default(),
        )
        .saturating_add(environment)
        .saturating_add(job_environment.saturating_mul(2))
        .saturating_add(self.config_files.capacity().saturating_mul(2))
        .saturating_add(self.request.default_terminal.capacity().saturating_mul(2))
        .saturating_add(std::mem::size_of::<Self>())
        .saturating_add(std::mem::size_of::<usize>().saturating_mul(2))
        .saturating_add(4096)
    }
}

pub(super) fn status_request(
    inner: &ServerState,
    client: ClientId,
    snapshot: &MuxSnapshot,
    option_snapshot: Arc<zz_mux::StatusRowVariables>,
    facts: FormatHookFacts,
    startup_ready: bool,
    job_needs: FormatNeeds,
) -> StatusRequest {
    status_request_with(
        inner,
        client,
        snapshot,
        option_snapshot,
        facts,
        startup_ready,
        job_needs,
        &mut BTreeMap::new(),
    )
}

pub(super) fn status_request_with(
    inner: &ServerState,
    client: ClientId,
    snapshot: &MuxSnapshot,
    option_snapshot: Arc<zz_mux::StatusRowVariables>,
    facts: FormatHookFacts,
    startup_ready: bool,
    job_needs: FormatNeeds,
    line_needs: &mut BTreeMap<Option<SessionId>, FormatNeeds>,
) -> StatusRequest {
    status_request_with_facts(
        inner,
        client,
        Some(snapshot),
        option_snapshot,
        Some(facts),
        startup_ready,
        job_needs,
        line_needs,
    )
}

pub(super) struct StatusParameters {
    pub(super) formats: Arc<zz_mux::StatusFormats>,
    pub(super) row_formats: Arc<BTreeMap<u32, String>>,
    pub(super) title_format: Arc<Option<String>>,
    pub(super) environment: Arc<Vec<(RawText, Option<RawText>)>>,
    pub(super) default_terminal: Arc<String>,
    pub(super) message_styles: Arc<(String, String)>,
    pub(super) message_line: u8,
    customized: bool,
    pub(super) needs: FormatNeeds,
    pub(super) references: Arc<BTreeSet<String>>,
    pub(super) client_references: Arc<BTreeSet<String>>,
    pub(super) fact_selection: StatusFactSelection,
    pub(super) client_fact_selection: ClientFactSelection,
}

pub(super) struct CachedStatusParameters {
    key: (Option<SessionId>, u64, u64, u64),
    parameters: Arc<StatusParameters>,
    option_snapshot: Arc<zz_mux::StatusRowVariables>,
}

impl StatusParameters {
    pub(super) fn retained_bytes(&self) -> usize {
        let strings = [
            &self.formats.left,
            &self.formats.right,
            &self.formats.style,
            &self.formats.background,
            &self.formats.foreground,
            &self.formats.left_style,
            &self.formats.right_style,
            &self.message_styles.0,
            &self.message_styles.1,
            self.default_terminal.as_ref(),
        ];
        let text = strings
            .into_iter()
            .chain(self.row_formats.values())
            .chain(self.title_format.iter())
            .chain(self.references.iter())
            .chain(self.client_references.iter())
            .fold(0_usize, |bytes, value| {
                bytes.saturating_add(std::mem::size_of::<String>() + value.capacity())
            });
        let environment = self
            .environment
            .iter()
            .fold(0_usize, |bytes, (name, value)| {
                bytes
                    .saturating_add(std::mem::size_of::<(RawText, Option<RawText>)>())
                    .saturating_add(zz_mux::cloned_raw_text_bytes(name))
                    .saturating_add(value.as_ref().map_or(0, zz_mux::cloned_raw_text_bytes))
            });
        text.saturating_add(environment)
            .saturating_add(std::mem::size_of::<Self>())
            .saturating_mul(2)
            .saturating_add(4096)
    }
}

pub(super) fn status_parameters(
    inner: &ServerState,
    attached: Option<SessionId>,
    option_snapshot: &Arc<zz_mux::StatusRowVariables>,
) -> Arc<StatusParameters> {
    let (state, options, data, _) = inner.engine.format_cache_revision();
    let key = (attached, state, options, data);
    if let Some(cached) = inner.publication.status_parameters_cache.lock().as_ref()
        && cached.key == key
        && Arc::ptr_eq(&cached.option_snapshot, option_snapshot)
    {
        return Arc::clone(&cached.parameters);
    }
    let formats = Arc::new(inner.engine.status_formats_for_session(attached));
    let row_formats = Arc::new(inner.engine.status_format_array_for_session(attached));
    let title_format = Arc::new(
        (attached.is_some() && inner.engine.set_titles_for_session(attached))
            .then(|| inner.engine.set_titles_string_for_session(attached)),
    );
    let message_styles = Arc::new(inner.engine.message_styles_for_session(attached));
    let needs = crate::status::status_line_needs(
        &inner.engine,
        &formats,
        &row_formats,
        title_format.as_deref(),
        &message_styles,
    );
    let (references, client_references) = {
        let references = inner.engine.cached_format_references_for_templates(
            crate::status::status_line_templates(
                &formats,
                &row_formats,
                title_format.as_deref(),
                &message_styles,
            ),
        );
        let border_references = inner.engine.cached_format_references_for_templates([
            "#{E:pane-border-style}",
            "#{E:pane-active-border-style}",
        ]);
        let client_references = Arc::new(
            references
                .iter()
                .chain(border_references.iter())
                .cloned()
                .collect(),
        );
        (references, client_references)
    };
    let fact_selection = StatusFactSelection::from_references(&references);
    let client_fact_selection = ClientFactSelection::from_references(&client_references);
    let parameters = Arc::new(StatusParameters {
        formats,
        row_formats,
        title_format,
        environment: Arc::new(inner.engine.job_environment(None)),
        default_terminal: Arc::new(inner.engine.default_terminal_for_spawn().to_owned()),
        message_styles,
        message_line: inner.engine.message_line_for_session(attached),
        customized: inner.engine.status_customized_for_session(attached),
        needs,
        references,
        client_references,
        fact_selection,
        client_fact_selection,
    });
    *inner.publication.status_parameters_cache.lock() = (parameters
        .retained_bytes()
        .saturating_add(option_snapshot.retained_bytes())
        <= 1024 * 1024)
        .then(|| CachedStatusParameters {
            key,
            parameters: Arc::clone(&parameters),
            option_snapshot: Arc::clone(option_snapshot),
        });
    parameters
}

pub(super) fn cached_live_status_context(
    inner: &ServerState,
    attached: Option<SessionId>,
    focused_window: Option<WindowId>,
    needs: FormatNeeds,
    references: &Arc<BTreeSet<String>>,
    capture_config_files: bool,
) -> Option<zz_mux::StatusContext<'static>> {
    if references.contains("window_active_clients")
        || references.contains("window_active_clients_list")
    {
        return None;
    }
    let attached = attached.filter(|session| inner.engine.state.sessions.contains_key(session));
    let host = inner.engine.format_host().is_empty().then(host_names);
    let overrides = [
        capture_config_files.then_some(("config_files", inner.config.config_files.as_str())),
        host.map(|host| ("host", host.0.as_str())),
        host.map(|host| ("host_short", host.1.as_str())),
        attached.map(|_| ("pane_active", "1")),
        attached.map(|_| ("session_attached", "0")),
        attached.map(|_| ("session_attached_list", "")),
        attached.map(|_| ("session_many_attached", "0")),
        attached.map(|_| ("window_active", "1")),
        attached.map(|_| ("window_active_clients", "0")),
        attached.map(|_| ("window_active_clients_list", "")),
    ];
    inner
        .engine
        .cached_detached_format_context_with_shared_references(
            (attached, focused_window, None),
            attached.map_or(FormatClient::NoClient, FormatClient::Attached),
            needs,
            references,
            overrides.into_iter().flatten(),
        )
}

pub(super) fn status_request_with_facts(
    inner: &ServerState,
    client: ClientId,
    snapshot: Option<&MuxSnapshot>,
    option_snapshot: Arc<zz_mux::StatusRowVariables>,
    facts: Option<FormatHookFacts>,
    startup_ready: bool,
    job_needs: FormatNeeds,
    _line_needs: &mut BTreeMap<Option<SessionId>, FormatNeeds>,
) -> StatusRequest {
    let attached = client_attached_session(inner, client);
    let focused_window = attached
        .and_then(|session| inner.engine.state.sessions.get(&session))
        .map(|session| client_focused_window(inner, client, session));
    let parameters = status_parameters(inner, attached, &option_snapshot);
    let needs = parameters.needs | job_needs;
    let references = if job_needs.is_empty() {
        Arc::clone(&parameters.references)
    } else {
        Arc::new(BTreeSet::from(["*".to_owned()]))
    };
    let modes = attached.map_or_else(Vec::new, |session| {
        mode_requests(inner, client, session, job_needs)
    });
    let capture_config_files = snapshot.is_some()
        || facts.is_some()
        || !modes.is_empty()
        || !job_needs.is_empty()
        || !zz_mux::borrowed_formats_enabled()
        || parameters.client_references.contains("*")
        || parameters.client_references.contains("config_files");
    let cached_context = snapshot
        .is_none()
        .then(|| {
            cached_live_status_context(
                inner,
                attached,
                focused_window,
                needs,
                &references,
                capture_config_files,
            )
        })
        .flatten();
    let live_context = cached_context.is_none().then(|| {
        let mut context = snapshot.map_or_else(
            || live_status_context(&inner.engine, attached, focused_window),
            |snapshot| status_context(snapshot, &inner.engine, attached, focused_window),
        );
        if capture_config_files {
            context.set_format_value("config_files", inner.config.config_files.clone());
        }
        context
    });
    let context = cached_context
        .as_ref()
        .or(live_context.as_ref())
        .expect("status context");
    let selected = facts.is_none();
    let mut facts = facts.unwrap_or_else(|| {
        if modes.is_empty() && job_needs.is_empty() {
            return selected_status_format_facts_with_selection(
                inner,
                context,
                &parameters.fact_selection,
            );
        }
        let mode_references;
        let fact_references = if modes.is_empty() {
            references.as_ref()
        } else {
            mode_references = references
                .iter()
                .cloned()
                .chain(
                    inner
                        .engine
                        .cached_format_references_for_templates(crate::status::MODE_FORMATS)
                        .iter()
                        .cloned(),
                )
                .collect::<BTreeSet<_>>();
            &mode_references
        };
        selected_status_format_facts(inner, context, fact_references)
    });
    if !selected || references.contains("*") || !modes.is_empty() {
        facts.client_environment = inner
            .client(client)
            .and_then(|c| c.environment.as_ref())
            .cloned();
    }
    if let Some(session) = attached {
        facts.client = Some(if selected && modes.is_empty() && job_needs.is_empty() {
            selected_client_format_facts_with_selection(
                inner,
                client,
                session,
                &parameters.client_fact_selection,
            )
        } else if selected && modes.is_empty() {
            selected_client_format_facts_with_references(inner, client, session, &references)
        } else {
            client_format_facts(inner, client, session)
        });
    }
    let pane_borders = if selected {
        let mut borrowed = readonly_borrowed_format_hook_facts(
            inner,
            CommandFormatSeed {
                client: facts.client.take(),
                invoking: Some(client),
            },
        );
        let borders = attached.map_or_else(Arc::default, |session| {
            border_presentations(inner, client, session, &borrowed)
        });
        facts.client = borrowed.seed.client.take();
        borders
    } else {
        attached.map_or_else(Arc::default, |session| {
            border_presentations(inner, client, session, &facts)
        })
    };
    let context = cached_context.unwrap_or_else(|| {
        live_context
            .expect("uncached status context")
            .detach_with_shared_references(needs, &references)
    });
    StatusRequest {
        client,
        formats: Arc::clone(&parameters.formats),
        row_formats: Arc::clone(&parameters.row_formats),
        option_snapshot,
        message_line: parameters.message_line,
        customized: parameters.customized,
        title_format: Arc::clone(&parameters.title_format),
        environment: Arc::clone(&parameters.environment),
        default_terminal: Arc::clone(&parameters.default_terminal),
        startup: !startup_ready,
        context: Arc::new(context),
        facts: Arc::new(facts),
        client_scheme: inner.client(client).and_then(|c| c.color_scheme),
        message_styles: Arc::clone(&parameters.message_styles),
        modes,
        pane_borders,
        references,
    }
}
