use super::*;

pub(super) fn client_attached_session(inner: &ServerState, client: ClientId) -> Option<SessionId> {
    client_attached_session_from_source(&ClientFormatSource::from_inner(inner), client)
}

pub(super) fn client_attached_session_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
) -> Option<SessionId> {
    inner
        .attached
        .iter()
        .find_map(|(session, clients)| clients.contains(&client).then_some(*session))
}

pub(super) fn client_tty_fact(capabilities: &[String]) -> Option<String> {
    capabilities.iter().find_map(|capability| {
        capability
            .strip_prefix(ClientHello::CLIENT_TTY_CAPABILITY_PREFIX)
            .filter(|tty| !tty.is_empty())
            .map(str::to_owned)
    })
}

pub(super) fn client_nested_fact(capabilities: &[String]) -> bool {
    capabilities
        .iter()
        .any(|capability| capability == ClientHello::CLIENT_NESTED_CAPABILITY)
}

pub(super) fn client_remote_fact(capabilities: &[String]) -> bool {
    capabilities
        .iter()
        .any(|capability| capability == ClientHello::CLIENT_REMOTE_CAPABILITY)
}

pub(super) fn client_utf8_fact(capabilities: &[String]) -> bool {
    capabilities
        .iter()
        .any(|capability| capability == ClientHello::CLIENT_UTF8_CAPABILITY)
}

pub(super) fn client_features_fact(capabilities: &[String]) -> u32 {
    terminal_feature_mask(capabilities.iter().filter_map(|capability| {
        capability.strip_prefix(ClientHello::CLIENT_FEATURES_CAPABILITY_PREFIX)
    }))
}

pub(super) fn client_size_fact(capabilities: &[String]) -> Option<(u16, u16)> {
    capabilities.iter().find_map(|capability| {
        let value = capability.strip_prefix(ClientHello::CLIENT_SIZE_CAPABILITY_PREFIX)?;
        let (columns, rows) = value.split_once('x')?;
        let columns = columns.parse::<u16>().ok().filter(|columns| *columns > 0)?;
        let rows = rows.parse::<u16>().ok().filter(|rows| *rows > 0)?;
        Some((columns, rows))
    })
}

pub(super) fn client_working_directory_fact(
    working_directory: Option<&ClientPath>,
) -> Option<PathBuf> {
    working_directory
        .and_then(ClientPath::to_path_buf)
        .filter(|working_directory| working_directory.is_absolute())
}

pub(super) fn client_environment_fact(entries: &[RawText]) -> Arc<ClientEnvironmentBlob> {
    Arc::new(ClientEnvironmentBlob::from_map(
        entries
            .iter()
            .filter_map(|entry| entry.split_once_byte(b'='))
            .collect(),
    ))
}

pub(super) fn effective_mux_options(inner: &ServerState, client: ClientId) -> MuxOptions {
    let mut options = inner.config.mux_options.clone();
    let session = client_attached_session(inner, client);
    let mouse = inner.engine.effective_mouse(session);
    let source = options
        .get(MuxOptionKey::Mouse)
        .map_or(MuxOptionSource::Default, |value| value.source);
    options.set(
        MuxOptionKey::Mouse,
        if mouse { "on" } else { "off" },
        source,
    );
    let focus_follows_mouse = inner.engine.effective_focus_follows_mouse(session);
    let source = options
        .get(MuxOptionKey::FocusFollowsMouse)
        .map_or(MuxOptionSource::Default, |value| value.source);
    options.set(
        MuxOptionKey::FocusFollowsMouse,
        if focus_follows_mouse { "on" } else { "off" },
        source,
    );
    options
}

#[cfg(test)]
pub(super) fn format_client_flags(inner: &ServerState, client: ClientId) -> String {
    format_client_flags_from_source(&ClientFormatSource::from_inner(inner), client)
}

pub(super) fn format_client_flags_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
) -> String {
    let mut flags = Vec::new();
    let requested = inner.client_flags.get(client);
    if client_attached_session_from_source(inner, client).is_some() {
        flags.push("attached".to_owned());
    }
    if inner
        .clients
        .get(&client)
        .and_then(|c| c.focused)
        .unwrap_or(true)
    {
        flags.push("focused".to_owned());
    }
    if inner.clients.get(&client).and_then(|c| c.kind) == Some(ClientKind::Control) {
        flags.push("control-mode".to_owned());
    }
    if client_ignores_size_from_source(inner, client) {
        flags.push("ignore-size".to_owned());
    }
    if requested.no_detach_on_destroy {
        flags.push("no-detach-on-destroy".to_owned());
    }
    if inner.clients.get(&client).and_then(|c| c.kind) == Some(ClientKind::Control)
        && let Some(output) = inner
            .clients
            .get(&client)
            .and_then(|c| c.control_output.as_ref())
    {
        if output.no_output {
            flags.push("no-output".to_owned());
        }
        if output.wait_exit {
            flags.push("wait-exit".to_owned());
        }
        if output.new_layouts {
            flags.push("new-layouts".to_owned());
        }
        if let Some(pause_after_ms) = output.pause_after_ms {
            flags.push(format!("pause-after={}", pause_after_ms / 1000));
        }
    }
    if requested.read_only {
        flags.push("read-only".to_owned());
    }
    if client_uses_utf8_from_source(inner, client) {
        flags.push("UTF-8".to_owned());
    }
    flags.join(",")
}

pub(super) fn client_environment_value<'a>(
    inner: &'a ServerState,
    client: ClientId,
    name: &str,
) -> Option<&'a str> {
    client_environment_value_from_source(&ClientFormatSource::from_inner(inner), client, name)
}

pub(super) fn client_environment_value_from_source<'a>(
    inner: &ClientFormatSource<'a>,
    client: ClientId,
    name: &str,
) -> Option<&'a str> {
    inner
        .fields
        .clients
        .get(&client)
        .and_then(|c| c.environment.as_ref())?
        .map()
        .get(name)
        .map(RawText::as_str)
}

pub(super) fn client_uses_utf8_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
) -> bool {
    if inner.clients.get(&client).is_some_and(|c| c.utf8) {
        return true;
    }
    if client_environment_value_from_source(inner, client, "TMUX")
        .is_some_and(|value| !value.is_empty())
    {
        return true;
    }
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .find_map(|name| {
            client_environment_value_from_source(inner, client, name)
                .filter(|value| !value.is_empty())
        })
        .is_some_and(|locale| {
            let locale = locale.to_ascii_lowercase();
            locale.contains("utf-8") || locale.contains("utf8")
        })
}

pub(super) fn client_colour_count_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
) -> Option<u32> {
    if inner.clients.get(&client).and_then(|c| c.kind) != Some(ClientKind::Interactive)
        || !inner.clients.get(&client).is_some_and(|c| c.has_terminal)
    {
        return None;
    }
    client_colour_count_with_from_source(
        inner,
        client,
        client_feature_mask_from_source(inner, client),
    )
}

fn client_terminal_features_option_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
) -> Vec<String> {
    let mut features = inner.engine.terminal_features_option();
    let term = client_environment_value_from_source(inner, client, "TERM").unwrap_or_default();
    let learned = terminal_features_list(
        inner
            .clients
            .get(&client)
            .and_then(|c| c.features)
            .unwrap_or(0),
    );
    if !term.is_empty() && !learned.is_empty() {
        features.push(format!("{term}:{}", learned.replace(',', ":")));
    }
    features
}

pub(super) fn client_negotiated_features_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
) -> String {
    terminal_features_list(
        inner
            .clients
            .get(&client)
            .and_then(|c| c.features)
            .unwrap_or(0),
    )
}

#[cfg(all(test, unix))]
pub(super) fn client_feature_mask(inner: &ServerState, client: ClientId) -> u32 {
    client_feature_mask_from_source(&ClientFormatSource::from_inner(inner), client)
}

fn client_feature_mask_from_source(inner: &ClientFormatSource<'_>, client: ClientId) -> u32 {
    let mut features = inner
        .clients
        .get(&client)
        .and_then(|c| c.features)
        .unwrap_or(0);
    if let Some(term) = client_terminal_facts(
        client_environment_value_from_source(inner, client, "TERM").unwrap_or_default(),
        client_environment_value_from_source(inner, client, "COLORTERM"),
        &client_negotiated_features_from_source(inner, client),
        &inner.engine.terminal_features_option(),
        &inner.engine.terminal_overrides_option(),
    ) {
        features |= terminal_feature_mask(term.requested_features());
        features &= !terminal_feature_mask(term.removed_features());
    }
    features
}

pub(super) type TerminalNegotiation = (Vec<String>, Vec<String>, Vec<String>);

pub(super) fn client_terminal_negotiation(
    inner: &ServerState,
    client: ClientId,
) -> Option<TerminalNegotiation> {
    let source = ClientFormatSource::from_inner(inner);
    client_colour_count_from_source(&source, client)?;
    let features = terminal_features_list(client_feature_mask_from_source(&source, client));
    let application_escape = client_terminal_facts(
        client_environment_value_from_source(&source, client, "TERM").unwrap_or_default(),
        client_environment_value_from_source(&source, client, "COLORTERM"),
        &features,
        &inner.engine.terminal_features_option(),
        &inner.engine.terminal_overrides_option(),
    )
    .map(|term| {
        ["Enesc", "Dsesc"]
            .map(|name| term.string_capability(name).to_owned())
            .to_vec()
    })
    .unwrap_or_default();
    Some((
        features
            .split(',')
            .filter(|feature| !feature.is_empty())
            .map(str::to_owned)
            .collect(),
        inner.engine.user_keys_option(),
        application_escape,
    ))
}

pub(super) fn take_terminal_negotiation(
    inner: &mut ServerState,
    client: ClientId,
) -> Option<EventPayload> {
    let negotiation = client_terminal_negotiation(inner, client)?;
    if inner
        .client(client)
        .and_then(|c| c.published_terminal_negotiation.as_ref())
        == Some(&negotiation)
    {
        return None;
    }
    inner
        .client_entry(client)
        .published_terminal_negotiation
        .replace(negotiation.clone());
    let (features, user_keys, application_escape) = negotiation;
    Some(EventPayload::TerminalNegotiation {
        features,
        user_keys,
        application_escape,
    })
}

fn client_colour_count_with_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    requested: u32,
) -> Option<u32> {
    if inner.clients.get(&client).and_then(|c| c.kind) != Some(ClientKind::Interactive)
        || !inner.clients.get(&client).is_some_and(|c| c.has_terminal)
    {
        return None;
    }
    let term = client_environment_value_from_source(inner, client, "TERM").unwrap_or_default();
    let colour_term =
        client_environment_value_from_source(inner, client, "COLORTERM").unwrap_or_default();
    Some(terminal_colour_count(term, colour_term, requested))
}

#[cfg(test)]
pub(super) fn client_term_features(inner: &ServerState, client: ClientId) -> String {
    client_term_features_from_source(&ClientFormatSource::from_inner(inner), client)
}

fn client_term_features_from_source(inner: &ClientFormatSource<'_>, client: ClientId) -> String {
    if client_colour_count_from_source(inner, client).is_none() {
        return String::new();
    }
    terminal_features_list(client_feature_mask_from_source(inner, client))
}

pub(super) fn client_format_geometry_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    window: WindowId,
) -> Option<TerminalGeometry> {
    let pane = inner.engine.state.windows.get(&window)?.active_pane;
    inner
        .terminal_geometries
        .get(&pane)
        .and_then(|geometries| geometries.get(&client))
        .copied()
        .or_else(|| {
            inner
                .terminal_geometries
                .values()
                .find_map(|geometries| geometries.get(&client).copied())
        })
}

pub(super) fn client_format_time(time: Option<u64>) -> String {
    time.filter(|time| *time != 0)
        .map_or_else(String::new, |time| time.to_string())
}

pub(super) fn client_format_facts(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
) -> ClientFormatFacts {
    client_format_facts_from_source(&ClientFormatSource::from_inner(inner), client, session)
}

#[derive(Clone, Copy, Default)]
pub(super) struct ClientFactSelection {
    pub(super) full: bool,
    activity: bool,
    cell_height: bool,
    cell_width: bool,
    colours: bool,
    control_mode: bool,
    created: bool,
    discarded: bool,
    flags: bool,
    height: bool,
    key_table: bool,
    last_session: bool,
    name: bool,
    pid: bool,
    prefix: bool,
    readonly: bool,
    session: bool,
    termfeatures: bool,
    termname: bool,
    termtype: bool,
    theme: bool,
    tty: bool,
    uid: bool,
    user: bool,
    utf8: bool,
    width: bool,
    written: bool,
    pub(super) viewport: bool,
}

#[cfg(test)]
thread_local! {
    pub(super) static FORMAT_FACT_SELECTION_BUILDS: Cell<usize> = const { Cell::new(0) };
}

impl ClientFactSelection {
    pub(super) fn complete() -> Self {
        Self {
            full: true,
            ..Self::default()
        }
    }

    pub(super) fn from_references(references: &BTreeSet<String>) -> Self {
        #[cfg(test)]
        FORMAT_FACT_SELECTION_BUILDS.with(|count| count.set(count.get() + 1));
        let mut selection = Self {
            width: true,
            ..Self::default()
        };
        for name in references {
            match name.as_str() {
                "*" => return Self::complete(),
                "client_activity" => selection.activity = true,
                "client_cell_height" | "window_cell_height" => selection.cell_height = true,
                "client_cell_width" | "window_cell_width" => selection.cell_width = true,
                "client_colours" => selection.colours = true,
                "client_control_mode" => selection.control_mode = true,
                "client_created" => selection.created = true,
                "client_discarded" => selection.discarded = true,
                "client_flags" => selection.flags = true,
                "client_height" => selection.height = true,
                "client_key_table" => selection.key_table = true,
                "client_last_session" => selection.last_session = true,
                "client_name" => selection.name = true,
                "client_pid" => selection.pid = true,
                "client_prefix" => selection.prefix = true,
                "client_readonly" => selection.readonly = true,
                "client_session" => selection.session = true,
                "client_termfeatures" => selection.termfeatures = true,
                "client_termname" => selection.termname = true,
                "client_termtype" => selection.termtype = true,
                "client_theme" => selection.theme = true,
                "client_tty" => selection.tty = true,
                "client_uid" => selection.uid = true,
                "client_user" => selection.user = true,
                "client_utf8" => selection.utf8 = true,
                "client_width" => selection.width = true,
                "client_written" => selection.written = true,
                "window_bigger" | "window_offset_x" | "window_offset_y" => {
                    selection.viewport = true;
                }
                name if name.starts_with("mode_") => return Self::complete(),
                _ => {}
            }
        }
        selection
    }
}

pub(super) fn client_format_facts_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    session: SessionId,
) -> ClientFormatFacts {
    client_format_facts_requested(inner, client, session, &ClientFactSelection::complete())
}

#[cfg(test)]
pub(super) fn selected_client_format_facts(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    references: &BTreeSet<String>,
) -> ClientFormatFacts {
    let border_references = inner.engine.cached_format_references_for_templates([
        "#{E:pane-border-style}",
        "#{E:pane-active-border-style}",
    ]);
    let references = references
        .iter()
        .chain(border_references.iter())
        .cloned()
        .collect();
    selected_client_format_facts_with_references(inner, client, session, &references)
}

pub(super) fn selected_client_format_facts_with_references(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    references: &BTreeSet<String>,
) -> ClientFormatFacts {
    selected_client_format_facts_with_selection(
        inner,
        client,
        session,
        &ClientFactSelection::from_references(references),
    )
}

pub(super) fn selected_client_format_facts_with_selection(
    inner: &ServerState,
    client: ClientId,
    session: SessionId,
    selection: &ClientFactSelection,
) -> ClientFormatFacts {
    client_format_facts_requested(
        &ClientFormatSource::from_inner(inner),
        client,
        session,
        selection,
    )
}

fn client_format_facts_requested(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    session: SessionId,
    selection: &ClientFactSelection,
) -> ClientFormatFacts {
    macro_rules! capture {
        ($field:ident, $value:expr) => {
            if selection.full || selection.$field {
                $value
            } else {
                String::new()
            }
        };
    }
    let Some(session_state) = inner.engine.state.sessions.get(&session) else {
        return ClientFormatFacts::default();
    };
    let registered = inner.clients.get(&client);
    let kind = registered.and_then(|c| c.kind);
    let has_terminal =
        kind == Some(ClientKind::Interactive) && registered.is_some_and(|c| c.has_terminal);
    let cell_height = selection.full || selection.cell_height;
    let cell_width = selection.full || selection.cell_width;
    let viewport = selection.full || selection.viewport;
    let window = (cell_height || cell_width || viewport || kind == Some(ClientKind::Control))
        .then(|| client_focused_window_from_source(inner, client, session_state));
    let control_geometry = (kind == Some(ClientKind::Control))
        .then(|| control_client_geometry_from_source(inner, client, window.unwrap()))
        .flatten();
    let terminal_geometry = (has_terminal && (cell_height || cell_width))
        .then(|| client_format_geometry_from_source(inner, client, window.unwrap()))
        .flatten();
    let retained_size = registered.and_then(|c| c.size);
    let width = if kind == Some(ClientKind::Control) {
        control_geometry.map_or(80, |geometry| geometry.columns)
    } else {
        retained_size.map_or(80, |size| size.0)
    };
    let (key_table, prefix) = if selection.full || selection.key_table || selection.prefix {
        let default_key_table = inner.engine.key_table_for_session(session);
        let key_table = inner
            .clients
            .get(&client)
            .and_then(|c| c.key_engine.as_ref())
            .and_then(KeyEngine::active_table)
            .unwrap_or(default_key_table.as_str());
        (
            capture!(key_table, key_table.to_owned()),
            capture!(
                prefix,
                usize::from(key_table != default_key_table).to_string()
            ),
        )
    } else {
        (String::new(), String::new())
    };
    let (written, discarded) = if selection.full || selection.written || selection.discarded {
        inner
            .clients
            .get(&client)
            .and_then(|c| c.subscriber.as_ref())
            .map_or((0, 0), |subscriber| subscriber.stats())
    } else {
        (0, 0)
    };
    ClientFormatFacts {
        activity: capture!(
            activity,
            client_format_time(registered.and_then(|c| c.activity_time))
        ),
        cell_height: if cell_height {
            terminal_geometry
                .map_or_else(
                    || has_terminal.then_some(0),
                    |geometry| Some(geometry.cell_height_px),
                )
                .map_or_else(String::new, |height| height.to_string())
        } else {
            String::new()
        },
        cell_width: if cell_width {
            terminal_geometry
                .map_or_else(
                    || has_terminal.then_some(0),
                    |geometry| Some(geometry.cell_width_px),
                )
                .map_or_else(String::new, |width| width.to_string())
        } else {
            String::new()
        },
        colours: capture!(
            colours,
            client_colour_count_from_source(inner, client)
                .map_or_else(String::new, |colours| colours.to_string())
        ),
        control_mode: capture!(
            control_mode,
            usize::from(kind == Some(ClientKind::Control)).to_string()
        ),
        created: capture!(
            created,
            client_format_time(registered.and_then(|c| c.created_time))
        ),
        discarded: capture!(discarded, discarded.to_string()),
        flags: capture!(flags, format_client_flags_from_source(inner, client)),
        height: capture!(
            height,
            has_terminal
                .then(|| retained_size.map_or(24, |size| size.1))
                .map_or_else(String::new, |height| height.to_string())
        ),
        key_table,
        last_session: capture!(
            last_session,
            registered
                .and_then(|c| c.last_session.as_ref())
                .and_then(|last| inner.engine.state.sessions.get(last))
                .map(|session| session.name.clone())
                .unwrap_or_default()
        ),
        name: capture!(name, client_format_name_from_source(inner, client)),
        pid: capture!(
            pid,
            registered
                .and_then(|c| c.pid.as_ref())
                .filter(|pid| **pid != 0)
                .map(u32::to_string)
                .unwrap_or_default()
        ),
        prefix,
        readonly: capture!(
            readonly,
            usize::from(inner.client_flags.contains(client)).to_string()
        ),
        session: capture!(session, session_state.name.clone()),
        termfeatures: capture!(
            termfeatures,
            client_term_features_from_source(inner, client)
        ),
        termname: capture!(
            termname,
            client_environment_value_from_source(inner, client, "TERM")
                .filter(|term| !term.is_empty())
                .unwrap_or("unknown")
                .to_owned()
        ),
        termtype: capture!(
            termtype,
            registered
                .and_then(|c| c.terminal_type.as_ref())
                .cloned()
                .unwrap_or_default()
        ),
        theme: capture!(
            theme,
            registered
                .and_then(|c| c.color_scheme.as_ref())
                .map(|scheme| scheme.as_str().to_owned())
                .unwrap_or_default()
        ),
        tty: capture!(
            tty,
            registered
                .and_then(|c| c.tty.as_ref())
                .cloned()
                .unwrap_or_default()
        ),
        uid: capture!(uid, inner.engine.format_uid().to_owned()),
        user: capture!(user, inner.engine.format_user().to_owned()),
        utf8: capture!(
            utf8,
            usize::from(client_uses_utf8_from_source(inner, client)).to_string()
        ),
        width: capture!(width, width.to_string()),
        written: capture!(written, written.to_string()),
        line: 0,
        environment: selection
            .full
            .then(|| registered.and_then(|c| c.environment.as_ref()).cloned())
            .flatten(),
        terminal: (selection.full && has_terminal)
            .then(|| {
                client_terminal_facts(
                    client_environment_value_from_source(inner, client, "TERM").unwrap_or_default(),
                    client_environment_value_from_source(inner, client, "COLORTERM"),
                    &client_negotiated_features_from_source(inner, client),
                    &client_terminal_features_option_from_source(inner, client),
                    &inner.engine.terminal_overrides_option(),
                )
            })
            .flatten(),
        viewport: (has_terminal && viewport)
            .then(|| client_viewport_facts_from_source(inner, client, session, window.unwrap()))
            .flatten(),
    }
}

pub(super) fn client_viewport_facts_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
    session: SessionId,
    window: WindowId,
) -> Option<ClientViewportFacts> {
    let (columns, rows) =
        interactive_client_window_extent_from_source(inner, client, session, window)?;
    let pane = inner.engine.state.windows.get(&window)?.active_pane;
    let geometry = inner
        .engine
        .pane_format_geometry(Some(session), Some(window), Some(pane));
    let cursor = inner.terminals.get(&pane).and_then(|terminal| {
        let viewport = terminal.latest_viewport();
        let (column, row) = if (viewport.columns, viewport.rows) == terminal.size() {
            let cursor = viewport.cursor.filter(|cursor| cursor.visible())?;
            (cursor.column(), cursor.row())
        } else {
            let facts = terminal.facts();
            if facts.cursor_hidden {
                return None;
            }
            (facts.cursor_x, facts.cursor_y)
        };
        Some((
            geometry.pane_left?.saturating_add(column),
            geometry.pane_top?.saturating_add(row),
        ))
    });
    Some(ClientViewportFacts {
        columns,
        rows,
        window_width: geometry.window_width?,
        window_height: geometry.window_height?,
        cursor,
    })
}

pub(super) fn server_log_client_name(inner: &ServerState, client: ClientId) -> String {
    inner
        .client(client)
        .and_then(|c| c.tty.as_ref())
        .filter(|tty| !tty.is_empty())
        .or_else(|| {
            inner
                .client(client)
                .and_then(|c| c.name.as_ref())
                .filter(|name| !name.is_empty())
        })
        .cloned()
        .unwrap_or_else(|| format!("device-{}", client.0))
}

pub(super) fn client_format_name(inner: &ServerState, client: ClientId) -> String {
    client_format_name_from_source(&ClientFormatSource::from_inner(inner), client)
}

pub(super) fn client_format_name_from_source(
    inner: &ClientFormatSource<'_>,
    client: ClientId,
) -> String {
    inner
        .clients
        .get(&client)
        .and_then(|c| c.tty.as_ref())
        .filter(|tty| !tty.is_empty())
        .cloned()
        .or_else(|| {
            inner
                .clients
                .get(&client)
                .and_then(|c| c.name.as_ref())
                .filter(|name| !name.is_empty())
                .cloned()
        })
        .unwrap_or_else(|| {
            let pid = inner
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
