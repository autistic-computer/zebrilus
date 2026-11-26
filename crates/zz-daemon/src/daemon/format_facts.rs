use super::*;

fn unseen_change_panes(inner: &ServerState) -> BTreeSet<PaneId> {
    inner
        .clients
        .values()
        .filter_map(|c| c.copy_session.as_ref())
        .filter(|session| !session.exiting && session.unseen)
        .map(|session| session.pane)
        .collect()
}

pub(super) fn pane_mode_format_facts(
    inner: &ServerState,
) -> BTreeMap<PaneId, (usize, &'static str)> {
    inner
        .modes
        .pane_modes
        .iter()
        .filter_map(|(pane, modes)| {
            Some((
                *pane,
                (
                    modes.len(),
                    match modes.last()? {
                        PaneModeRequest::Clock => "clock-mode",
                        PaneModeRequest::Customize(_) => "options-mode",
                        PaneModeRequest::Switch(_) => "switch-mode",
                        PaneModeRequest::Panes(_) => "panes-mode",
                    },
                ),
            ))
        })
        .collect()
}

#[derive(Clone, Copy)]
pub(super) struct ClientFormatFields<'a> {
    pub(super) clients: &'a BTreeMap<ClientId, Box<Client>>,
    pub(super) attached: &'a BTreeMap<SessionId, BTreeSet<ClientId>>,
    pub(super) client_flags: &'a ClientFlags,
    pub(super) terminal_geometries: &'a BTreeMap<PaneId, BTreeMap<ClientId, TerminalGeometry>>,
    pub(super) terminals: &'a Arc<BTreeMap<PaneId, Arc<TerminalSession>>>,
}

impl<'a> ClientFormatFields<'a> {
    pub(super) fn from_inner(inner: &'a ServerState) -> Self {
        Self {
            clients: &inner.clients,
            attached: &inner.attachment.attached,
            client_flags: &inner.attachment.client_flags,
            terminal_geometries: &inner.panes.terminal_geometries,
            terminals: &inner.terminals,
        }
    }
}

pub(super) struct ClientFormatSource<'a> {
    pub(super) engine: &'a MuxEngine,
    pub(super) fields: ClientFormatFields<'a>,
}

impl<'a> ClientFormatSource<'a> {
    pub(super) fn from_inner(inner: &'a ServerState) -> Self {
        Self {
            engine: &inner.engine,
            fields: ClientFormatFields::from_inner(inner),
        }
    }
}

impl<'a> std::ops::Deref for ClientFormatSource<'a> {
    type Target = ClientFormatFields<'a>;
    fn deref(&self) -> &Self::Target {
        &self.fields
    }
}

#[derive(Default)]
pub(super) struct CommandFormatSeed {
    pub(super) client: Option<ClientFormatFacts>,
    pub(super) invoking: Option<ClientId>,
}

#[derive(Default)]
pub(super) struct LazyDerivedFormatFacts {
    pub(super) clients: std::cell::OnceCell<Vec<zz_mux::FormatClientRow>>,
    pub(super) buffer: std::cell::OnceCell<Option<BufferFormatFacts>>,
    pub(super) pane_pipes: std::cell::OnceCell<BTreeMap<PaneId, u32>>,
    pub(super) session_attachments: std::cell::OnceCell<BTreeMap<SessionId, (usize, String)>>,
    pub(super) unseen_changes: std::cell::OnceCell<BTreeSet<PaneId>>,
    pub(super) window_clients: std::cell::OnceCell<BTreeMap<WindowId, Vec<String>>>,
    pub(super) copy_modes:
        std::cell::OnceCell<BTreeMap<PaneId, Vec<(String, Arc<zz_terminal::CopyModeFacts>)>>>,
    pub(super) pane_modes: std::cell::OnceCell<BTreeMap<PaneId, (usize, &'static str)>>,
}

pub(super) struct BorrowedFormatHookFacts<'a> {
    pub(super) client_fields: ClientFormatFields<'a>,
    pub(super) paste_buffers: &'a [PasteBuffer],
    pub(super) terminals: &'a BTreeMap<PaneId, Arc<TerminalSession>>,
    pub(super) pane_pipes: &'a BTreeMap<PaneId, PanePipe>,
    pub(super) attached: &'a BTreeMap<SessionId, BTreeSet<ClientId>>,
    pub(super) clients: &'a BTreeMap<ClientId, Box<Client>>,
    pub(super) session_last_attached: &'a BTreeMap<SessionId, u64>,
    pub(super) pane_modes: &'a BTreeMap<PaneId, Vec<PaneModeRequest>>,
    pub(super) seed: CommandFormatSeed,
    pub(super) derived: LazyDerivedFormatFacts,
}

impl BorrowedFormatHookFacts<'_> {
    pub(super) fn client_name(&self, client: ClientId) -> String {
        self.clients
            .get(&client)
            .and_then(|c| c.tty.as_ref())
            .filter(|tty| !tty.is_empty())
            .cloned()
            .or_else(|| {
                self.clients
                    .get(&client)
                    .and_then(|c| c.name.as_ref())
                    .filter(|name| !name.is_empty())
                    .cloned()
            })
            .unwrap_or_else(|| {
                let pid = self
                    .clients
                    .get(&client)
                    .and_then(|c| c.pid)
                    .unwrap_or_default();
                if pid == 0 {
                    format!("device-{}", client.0)
                } else {
                    format!("client-{pid}")
                }
            })
    }
}

pub(super) fn pane_in_mode_count_from_parts(
    pane: PaneId,
    pane_modes: &BTreeMap<PaneId, Vec<PaneModeRequest>>,
    clients: &BTreeMap<ClientId, Box<Client>>,
    terminals: &BTreeMap<PaneId, Arc<TerminalSession>>,
) -> usize {
    pane_modes.get(&pane).map_or(0, Vec::len)
        + usize::from(
            clients
                .iter()
                .filter_map(|(id, client)| {
                    client.copy_session.as_ref().map(|session| (id, session))
                })
                .any(|(client, session)| {
                    session.pane == pane
                        && !session.exiting
                        && terminals.get(&pane).is_some_and(|terminal| {
                            terminal.copy_mode_facts(TerminalViewId(client.0)).is_some()
                        })
                }),
        )
}

impl crate::status::FormatFactSource for BorrowedFormatHookFacts<'_> {
    fn terminals(&self) -> &BTreeMap<PaneId, Arc<TerminalSession>> {
        self.terminals
    }
    fn session_last_attached(&self) -> &BTreeMap<SessionId, u64> {
        self.session_last_attached
    }
    fn buffer(&self) -> Option<&BufferFormatFacts> {
        self.derived
            .buffer
            .get_or_init(|| {
                self.paste_buffers
                    .iter()
                    .find(|buffer| buffer.automatic)
                    .map(buffer_format_facts)
            })
            .as_ref()
    }
    fn client(&self) -> Option<&ClientFormatFacts> {
        self.seed.client.as_ref()
    }
    fn clients(&self, context: &zz_mux::StatusContext) -> &[zz_mux::FormatClientRow] {
        self.derived.clients.get_or_init(|| {
            let Some(engine) = context.engine() else {
                return Vec::new();
            };
            let source = ClientFormatSource {
                engine,
                fields: self.client_fields,
            };
            loop_clients_in_connect_order(self.attached, engine)
                .into_iter()
                .map(|(client, session)| {
                    client_format_facts_from_source(&source, client, session).loop_row(
                        self.client_fields
                            .clients
                            .get(&client)
                            .and_then(|c| c.activity)
                            .unwrap_or_default(),
                    )
                })
                .collect()
        })
    }
    fn client_environment(&self) -> Option<&Arc<ClientEnvironmentBlob>> {
        self.seed.invoking.and_then(|client| {
            self.client_fields
                .clients
                .get(&client)
                .and_then(|c| c.environment.as_ref())
        })
    }
    fn message(&self) -> Option<&crate::status::MessageFormatFacts> {
        None
    }
    fn mux(&self) -> &zz_mux::FormatFacts {
        &hook_events::unread_format_facts().mux
    }

    fn pane_pipes(&self) -> &BTreeMap<PaneId, u32> {
        self.derived.pane_pipes.get_or_init(|| {
            self.pane_pipes
                .iter()
                .map(|(pane, pipe)| (*pane, pipe.pid))
                .collect()
        })
    }

    fn session_attachments(&self) -> &BTreeMap<SessionId, (usize, String)> {
        self.derived.session_attachments.get_or_init(|| {
            self.attached
                .iter()
                .map(|(session, clients)| {
                    let clients = clients
                        .iter()
                        .filter(|client| !self.clients.get(*client).is_some_and(|c| c.suspended))
                        .copied()
                        .collect::<Vec<_>>();
                    let names = clients
                        .iter()
                        .map(|client| self.client_name(*client))
                        .collect::<Vec<_>>()
                        .join(",");
                    (*session, (clients.len(), names))
                })
                .collect()
        })
    }

    fn unseen_changes(&self) -> &BTreeSet<PaneId> {
        self.derived.unseen_changes.get_or_init(|| {
            self.clients
                .values()
                .filter_map(|c| c.copy_session.as_ref())
                .filter(|session| !session.exiting && session.unseen)
                .map(|session| session.pane)
                .collect()
        })
    }

    fn window_clients(&self, context: &zz_mux::StatusContext) -> &BTreeMap<WindowId, Vec<String>> {
        self.derived.window_clients.get_or_init(|| {
            let Some(engine) = context.engine() else {
                return BTreeMap::new();
            };
            let source = ClientFormatSource {
                engine,
                fields: self.client_fields,
            };
            self.attached.values().flatten().fold(
                BTreeMap::<WindowId, Vec<String>>::new(),
                |mut windows, client| {
                    if let Some(session) = client_attached_session_from_source(&source, *client)
                        .and_then(|session| engine.state.sessions.get(&session))
                    {
                        let window = client_focused_window_from_source(&source, *client, session);
                        windows
                            .entry(window)
                            .or_default()
                            .push(self.client_name(*client));
                    }
                    windows
                },
            )
        })
    }

    fn copy_modes(&self) -> &BTreeMap<PaneId, Vec<(String, Arc<zz_terminal::CopyModeFacts>)>> {
        self.derived.copy_modes.get_or_init(|| {
            let mut panes =
                BTreeMap::<PaneId, Vec<(String, Arc<zz_terminal::CopyModeFacts>)>>::new();
            for (client, session) in self.clients.iter().filter_map(|(id, client)| {
                client.copy_session.as_ref().map(|session| (id, session))
            }) {
                if session.exiting {
                    continue;
                }
                if let Some(facts) = self
                    .terminals
                    .get(&session.pane)
                    .and_then(|terminal| terminal.copy_mode_facts(TerminalViewId(client.0)))
                {
                    let name = self
                        .clients
                        .get(client)
                        .and_then(|c| c.name.as_ref())
                        .cloned()
                        .unwrap_or_default();
                    panes.entry(session.pane).or_default().push((name, facts));
                }
            }
            panes
        })
    }

    fn pane_modes(&self) -> &BTreeMap<PaneId, (usize, &'static str)> {
        self.derived.pane_modes.get_or_init(|| {
            self.pane_modes
                .iter()
                .filter_map(|(pane, modes)| {
                    Some((
                        *pane,
                        (
                            modes.len(),
                            match modes.last()? {
                                PaneModeRequest::Clock => "clock-mode",
                                PaneModeRequest::Customize(_) => "options-mode",
                                PaneModeRequest::Switch(_) => "switch-mode",
                                PaneModeRequest::Panes(_) => "panes-mode",
                            },
                        ),
                    ))
                })
                .collect()
        })
    }

    fn pane_in_mode_count(&self, pane: PaneId) -> usize {
        pane_in_mode_count_from_parts(pane, self.pane_modes, self.clients, self.terminals)
    }
}

pub(super) fn split_borrowed_format_hook_facts(
    inner: &mut ServerState,
    seed: CommandFormatSeed,
) -> (&mut MuxEngine, BorrowedFormatHookFacts<'_>) {
    let ServerState {
        engine,
        attachment:
            Attachment {
                attached,
                client_flags,
                session_last_attached,
                ..
            },
        clients,

        modes: Modes { pane_modes, .. },
        jobs: Jobs { pane_pipes, .. },
        buffers: Buffers { paste_buffers, .. },
        panes: PaneTables {
            terminal_geometries,
            ..
        },
        terminals,
        ..
    } = inner;
    let facts = BorrowedFormatHookFacts {
        terminals,
        pane_pipes,
        attached,
        clients,
        session_last_attached,

        pane_modes,
        paste_buffers,
        seed,
        derived: LazyDerivedFormatFacts::default(),
        client_fields: ClientFormatFields {
            clients,
            attached,
            client_flags,

            terminal_geometries,
            terminals,
        },
    };
    (engine, facts)
}

pub(super) fn readonly_borrowed_format_hook_facts(
    inner: &ServerState,
    seed: CommandFormatSeed,
) -> BorrowedFormatHookFacts<'_> {
    BorrowedFormatHookFacts {
        client_fields: ClientFormatFields::from_inner(inner),
        paste_buffers: &inner.buffers.paste_buffers,
        terminals: &inner.terminals,
        pane_pipes: &inner.jobs.pane_pipes,
        attached: &inner.attachment.attached,
        clients: &inner.clients,
        session_last_attached: &inner.attachment.session_last_attached,
        pane_modes: &inner.modes.pane_modes,
        seed,
        derived: LazyDerivedFormatFacts::default(),
    }
}

pub(super) fn borrowed_format_hook_facts(inner: &ServerState) -> BorrowedFormatHookFacts<'_> {
    readonly_borrowed_format_hook_facts(inner, CommandFormatSeed::default())
}

pub(super) fn borrowed_format_hook_facts_for_client<'a>(
    inner: &'a ServerState,
    client: ClientId,
    context: &ExecutionContext,
) -> BorrowedFormatHookFacts<'a> {
    readonly_borrowed_format_hook_facts(inner, command_format_seed(inner, client, context))
}

pub(super) fn command_format_seed(
    inner: &ServerState,
    client: ClientId,
    context: &ExecutionContext,
) -> CommandFormatSeed {
    let invoking = format_provenance_client(context, client);
    let format_client = if context.has_no_client() {
        hook_body_format_client(inner)
    } else {
        invoking.and_then(|client| current_format_client(inner, client))
    };
    CommandFormatSeed {
        client: format_client.and_then(|client| {
            client_attached_session(inner, client)
                .map(|session| client_format_facts(inner, client, session))
        }),
        invoking,
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static OWNED_FORMAT_FACT_BUILDS: Cell<usize> = const { Cell::new(0) };
}

#[derive(Clone, Copy, Default)]
pub(super) struct StatusFactSelection {
    full: bool,
    mux: bool,
    terminals: bool,
    pane_pipes: bool,
    session_attachments: bool,
    session_last_attached: bool,
    window_clients: bool,
    unseen_changes: bool,
    buffer: bool,
    copy_modes: bool,
    pane_modes: bool,
}

impl StatusFactSelection {
    pub(super) fn from_references(references: &BTreeSet<String>) -> Self {
        #[cfg(test)]
        FORMAT_FACT_SELECTION_BUILDS.with(|count| count.set(count.get() + 1));
        let mut selection = Self::default();
        for name in references {
            match name.as_str() {
                "*" => {
                    selection.full = true;
                    return selection;
                }
                "pane_kind"
                | "browser_url"
                | "agent_state"
                | "agent_pending_permission"
                | "agent_parent" => {
                    selection.mux = true;
                }
                "pane_search_string"
                | "history_size"
                | "cursor_x"
                | "cursor_y"
                | "alternate_on"
                | "mouse_any_flag"
                | "cursor_flag"
                | "pane_last_command_status"
                | "pane_output_generation"
                | "pane_last_output_time"
                | "pane_last_prompt_time"
                | "pane_command_start_time"
                | "pane_command_end_time"
                | "pane_command_running"
                | "pane_command_duration"
                | "pane_command_status"
                | "pane_private_modes"
                | "pane_pb_progress"
                | "pane_pb_state"
                | "pane_status"
                | "pane_status_kind"
                | "pane_status_progress"
                | "pane_status_app"
                | "pane_status_title"
                | "pane_status_message"
                | "pane_status_raw_title"
                | "pane_status_raw_message"
                | "pane_status_reported" => selection.terminals = true,
                "pane_pipe" | "pane_pipe_pid" => selection.pane_pipes = true,
                "session_attached" | "session_attached_list" | "session_many_attached" => {
                    selection.session_attachments = true;
                }
                "session_last_attached" => selection.session_last_attached = true,
                "window_active_clients" | "window_active_clients_list" => {
                    selection.window_clients = true;
                }
                "pane_unseen_changes" => selection.unseen_changes = true,
                "buffer_created" | "buffer_full" | "buffer_name" | "buffer_sample"
                | "buffer_size" => {
                    selection.buffer = true;
                }
                "pane_in_mode" | "pane_mode" => {
                    selection.copy_modes = true;
                    selection.pane_modes = true;
                }
                name if name.starts_with("mode_") => {
                    selection.full = true;
                    return selection;
                }
                name if name.starts_with('@') => selection.mux = true,
                name if crate::status::COPY_MODE_CONTEXT_FORMATS.contains(&name) => {
                    selection.copy_modes = true;
                }
                _ => {}
            }
        }
        selection
    }
}

pub(super) fn selected_status_format_facts(
    inner: &ServerState,
    context: &zz_mux::StatusContext,
    references: &BTreeSet<String>,
) -> FormatHookFacts {
    selected_status_format_facts_with_selection(
        inner,
        context,
        &StatusFactSelection::from_references(references),
    )
}

pub(super) fn selected_status_format_facts_with_selection(
    inner: &ServerState,
    context: &zz_mux::StatusContext,
    selection: &StatusFactSelection,
) -> FormatHookFacts {
    use crate::status::FormatFactSource;

    if selection.full {
        return format_hook_facts(inner);
    }
    let source = readonly_borrowed_format_hook_facts(inner, CommandFormatSeed::default());
    let mut facts = FormatHookFacts::shared_empty();
    if selection.mux {
        facts.mux = Arc::new(inner.engine.format_facts());
    }
    if selection.terminals {
        facts.terminals = Arc::clone(&inner.terminals);
    }
    if selection.pane_pipes {
        facts.pane_pipes = Arc::new(source.pane_pipes().clone());
    }
    if selection.session_attachments {
        facts.session_attachments = Arc::new(source.session_attachments().clone());
    }
    if selection.session_last_attached {
        facts.session_last_attached = Arc::new(inner.attachment.session_last_attached.clone());
    }
    if selection.window_clients {
        facts.window_clients = Arc::new(source.window_clients(context).clone());
    }
    if selection.unseen_changes {
        facts.unseen_changes = Arc::new(source.unseen_changes().clone());
    }
    if selection.buffer {
        facts.buffer = source.buffer().cloned();
    }
    if selection.copy_modes {
        facts.copy_modes = Arc::new(source.copy_modes().clone());
    }
    if selection.pane_modes {
        facts.pane_modes = Arc::new(source.pane_modes().clone());
    }
    facts
}

pub(super) fn format_hook_facts(inner: &ServerState) -> FormatHookFacts {
    #[cfg(test)]
    OWNED_FORMAT_FACT_BUILDS.with(|count| count.set(count.get() + 1));
    FormatHookFacts {
        mux: Arc::new(inner.engine.format_facts()),
        copy_modes: Arc::new(copy_mode_format_facts(inner)),
        pane_modes: Arc::new(pane_mode_format_facts(inner)),
        unseen_changes: Arc::new(unseen_change_panes(inner)),
        terminals: Arc::clone(&inner.terminals),
        pane_pipes: Arc::new(
            inner
                .jobs
                .pane_pipes
                .iter()
                .map(|(pane, pipe)| (*pane, pipe.pid))
                .collect(),
        ),
        session_attachments: Arc::new(
            inner
                .attachment
                .attached
                .iter()
                .map(|(session, clients)| {
                    let clients = clients
                        .iter()
                        .filter(|client| !inner.client(**client).is_some_and(|c| c.suspended))
                        .copied()
                        .collect::<Vec<_>>();
                    let names = clients
                        .iter()
                        .map(|client| client_format_name(inner, *client))
                        .collect::<Vec<_>>()
                        .join(",");
                    (*session, (clients.len(), names))
                })
                .collect(),
        ),
        window_clients: Arc::new(inner.attachment.attached.values().flatten().fold(
            BTreeMap::<WindowId, Vec<String>>::new(),
            |mut windows, client| {
                if let Some(window) = client_focused_window_for_attachment(inner, *client) {
                    windows
                        .entry(window)
                        .or_default()
                        .push(client_format_name(inner, *client));
                }
                windows
            },
        )),
        session_last_attached: Arc::new(inner.attachment.session_last_attached.clone()),
        clients: Arc::new(
            loop_clients_in_connect_order(&inner.attachment.attached, &inner.engine)
                .into_iter()
                .map(|(client, session)| {
                    client_format_facts(inner, client, session).loop_row(
                        inner
                            .client(client)
                            .and_then(|c| c.activity)
                            .unwrap_or_default(),
                    )
                })
                .collect(),
        ),
        buffer: inner
            .buffers
            .paste_buffers
            .iter()
            .find(|buffer| buffer.automatic)
            .map(buffer_format_facts),
        ..FormatHookFacts::default()
    }
}

fn loop_clients_in_connect_order(
    attached: &BTreeMap<SessionId, BTreeSet<ClientId>>,
    engine: &MuxEngine,
) -> Vec<(ClientId, SessionId)> {
    let mut clients = attached
        .iter()
        .filter(|(session, _)| engine.state.sessions.contains_key(session))
        .flat_map(|(session, clients)| clients.iter().map(move |client| (*client, *session)))
        .collect::<Vec<_>>();
    clients.sort_unstable();
    clients
}

pub(super) fn hook_body_format_client(inner: &ServerState) -> Option<ClientId> {
    inner
        .engine
        .state
        .most_recent_context()
        .and_then(|(session, _, _)| best_client_on_session(inner, session))
        .or_else(|| best_attached_client(inner))
}

pub(super) fn format_provenance_client(
    context: &ExecutionContext,
    invoking_client: ClientId,
) -> Option<ClientId> {
    context
        .replay_client()
        .or_else(|| context.control_command_target().map(|(client, _)| client))
        .or((invoking_client != ClientId(u64::MAX)).then_some(invoking_client))
}

pub(super) fn current_format_client(
    inner: &ServerState,
    invoking_client: ClientId,
) -> Option<ClientId> {
    if client_attached_session(inner, invoking_client).is_some() {
        return Some(invoking_client);
    }
    if let Some(session) = inner
        .client(invoking_client)
        .and_then(|c| c.origin.as_ref())
        .and_then(|pane| inner.engine.state.window_for_pane(*pane))
        .and_then(|window| inner.engine.state.windows.get(&window))
        .map(|window| window.session)
    {
        return best_client_on_session(inner, session).or_else(|| best_attached_client(inner));
    }
    inner
        .engine
        .state
        .most_recent_context()
        .and_then(|(session, _, _)| best_client_on_session(inner, session))
        .or_else(|| best_attached_client(inner))
}
