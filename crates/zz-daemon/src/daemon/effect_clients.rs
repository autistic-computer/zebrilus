use super::*;

pub(super) fn defer_unowned_client_events(
    client: ClientId,
    events: &mut Vec<EventPayload>,
    deferred: &mut Vec<DeferredClientEvent>,
) {
    deferred.extend(
        events
            .drain(..)
            .map(|payload| DeferredClientEvent::Unowned {
                client,
                payload: Box::new(payload),
            }),
    );
}

pub(super) const INVOKING_CLIENT_COMMANDS: [&str; 4] =
    ["list-windows", "list-sessions", "list-panes", "new-session"];

#[derive(Clone, Copy)]
pub(super) enum ResolvedDetach {
    Client(ClientId),
    Others(ClientId),
    Session(SessionId),
}

pub(super) fn best_display_message_format_client(
    inner: &ServerState,
    target_session: SessionId,
) -> Option<ClientId> {
    best_client_on_session(inner, target_session).or_else(|| best_attached_client(inner))
}

pub(super) fn requested_detach_victims(
    inner: &ServerState,
    detach: ResolvedDetach,
) -> Vec<(SessionId, ClientId)> {
    match detach {
        ResolvedDetach::Client(target) => client_attached_session(inner, target)
            .map(|session| vec![(session, target)])
            .unwrap_or_default(),
        ResolvedDetach::Others(target) => inner
            .attachment
            .attached
            .iter()
            .flat_map(|(session, clients)| {
                clients
                    .iter()
                    .filter(move |client| **client != target)
                    .map(|client| (*session, *client))
            })
            .collect(),
        ResolvedDetach::Session(session) => inner
            .attachment
            .attached
            .get(&session)
            .into_iter()
            .flatten()
            .map(|client| (session, *client))
            .collect(),
    }
}

pub(super) fn resolve_display_message_client(
    inner: &ServerState,
    target: &str,
) -> Option<ClientId> {
    find_attached_client(inner, target)
}

pub(super) fn unattached_client_format_facts(
    inner: &ServerState,
    client: ClientId,
) -> ClientFormatFacts {
    let source = ClientFormatSource::from_inner(inner);
    let registered = source.clients.get(&client);
    let (written, discarded) = registered
        .and_then(|c| c.subscriber.as_ref())
        .map_or((0, 0), |subscriber| subscriber.stats());
    ClientFormatFacts {
        activity: client_format_time(registered.and_then(|c| c.activity_time)),
        control_mode: usize::from(registered.and_then(|c| c.kind) == Some(ClientKind::Control))
            .to_string(),
        created: client_format_time(registered.and_then(|c| c.created_time)),
        discarded: discarded.to_string(),
        flags: format_client_flags_from_source(&source, client),
        key_table: "root".to_owned(),
        name: client_format_name_from_source(&source, client),
        pid: registered
            .and_then(|c| c.pid)
            .filter(|pid| *pid != 0)
            .map(|pid| pid.to_string())
            .unwrap_or_default(),
        prefix: "0".to_owned(),
        readonly: usize::from(source.client_flags.contains(client)).to_string(),
        termfeatures: client_negotiated_features_from_source(&source, client),
        termname: client_environment_value_from_source(&source, client, "TERM")
            .filter(|term| !term.is_empty())
            .unwrap_or("unknown")
            .to_owned(),
        tty: registered
            .and_then(|c| c.tty.as_ref())
            .cloned()
            .unwrap_or_default(),
        uid: source.engine.format_uid().to_owned(),
        user: source.engine.format_user().to_owned(),
        utf8: usize::from(client_uses_utf8_from_source(&source, client)).to_string(),
        width: "80".to_owned(),
        written: written.to_string(),
        environment: registered.and_then(|c| c.environment.as_ref()).cloned(),
        viewport: Some(ClientViewportFacts::default()),
        ..ClientFormatFacts::default()
    }
}

pub(super) fn previewing_clients(inner: &ServerState) -> BTreeSet<ClientId> {
    inner
        .clients
        .iter()
        .filter(|(_, client)| client.terminal_preview)
        .map(|(id, _)| *id)
        .collect()
}

pub(super) fn selected_effect_client(
    inner: &ServerState,
    client: ClientId,
    target: Option<&str>,
) -> Option<ClientId> {
    match target {
        Some(target) => find_attached_client_with_aliases(inner, target, true),
        None => Some(client),
    }
}

pub(super) fn format_hook_facts_for_client(
    inner: &ServerState,
    client: ClientId,
    context: &ExecutionContext,
) -> FormatHookFacts {
    let mut facts = format_hook_facts(inner);
    if let Some(invoking) = format_provenance_client(context, client) {
        facts.client_environment = inner
            .client(invoking)
            .and_then(|c| c.environment.as_ref())
            .cloned();
    }
    let format_client = if context.has_no_client() {
        hook_body_format_client(inner)
    } else {
        format_provenance_client(context, client)
            .and_then(|client| current_format_client(inner, client))
    };
    if let Some(client) = format_client {
        facts.client = client_attached_session(inner, client)
            .map(|session| client_format_facts(inner, client, session));
    }
    facts
}
