use super::mux_effects::{CommandAftermath, EffectContext};
use super::*;

pub(super) fn apply_message_effect(
    inner: &mut ServerState,
    cx: EffectContext<'_>,
    after: &mut CommandAftermath,
    output: &mut RawText,
    effect: &MuxEffect,
) {
    let EffectContext {
        client, context, ..
    } = cx;
    match effect {
        MuxEffect::DisplayMessage {
            pane,
            target_client,
            text,
            duration_ms,
            freeze,
            ignore_keys,
        } => {
            let message_client = match target_client.as_deref() {
                Some(target) => resolve_display_message_client(inner, target),
                None => Some(client),
            };
            let Some(message_client) = message_client else {
                return;
            };
            let duration_ms = duration_ms.unwrap_or_else(|| {
                inner
                    .engine
                    .display_time_for_client_session(client_attached_session(inner, message_client))
            });
            push_server_message(inner, text.clone());
            let message_id = next_timed_message_id(inner);
            let publication = OwnedClientMessagePublication {
                client: message_client,
                pane: *pane,
                kind: ClientMessageKind::Info,
                text: text.clone(),
                duration_ms,
                message_id,
            };
            if inner.client(message_client).and_then(|c| c.kind) == Some(ClientKind::Interactive) {
                if duration_ms != 0 {
                    if *ignore_keys {
                        inner.client_entry(message_client).message_ignore_keys = true;
                    } else {
                        let _ = inner
                            .client_mut(message_client)
                            .is_some_and(|client| std::mem::take(&mut client.message_ignore_keys));
                    }
                }
                let (previous, schedule) =
                    arm_client_message(inner, message_client, message_id, duration_ms, *freeze);
                if let Some(previous) = previous {
                    after
                        .client_message_retires
                        .push((message_client, previous));
                }
                after.client_message_schedule = schedule;
                let event = DeferredClientEvent::Owned(publication);
                if message_client == client {
                    defer_unowned_client_events(
                        client,
                        &mut after.direct_events,
                        &mut after.deferred_direct_events,
                    );
                    after.deferred_direct_events.push(event);
                } else {
                    after.targeted_events.push(event);
                }
            } else {
                let event = EventPayload::TimedClientMessage {
                    pane: publication.pane,
                    kind: publication.kind,
                    text: publication.text,
                    duration_ms: publication.duration_ms,
                    message_id: publication.message_id,
                };
                if message_client == client {
                    after.direct_events.push(event);
                } else {
                    after.targeted_events.push(DeferredClientEvent::Unowned {
                        client: message_client,
                        payload: Box::new(event),
                    });
                }
            }
        }
        MuxEffect::PrintOrMessage {
            pane,
            text,
            duration_ms,
            freeze,
        } => {
            let message_client = (!context.has_no_client())
                .then(|| format_provenance_client(context, client))
                .flatten()
                .and_then(|client| current_format_client(inner, client))
                .filter(|client| {
                    inner.client(*client).and_then(|c| c.kind) == Some(ClientKind::Interactive)
                });
            if let Some(message_client) = message_client {
                push_server_message(inner, text.clone());
                let message_id = next_timed_message_id(inner);
                let publication = OwnedClientMessagePublication {
                    client: message_client,
                    pane: *pane,
                    kind: ClientMessageKind::Info,
                    text: text.clone(),
                    duration_ms: *duration_ms,
                    message_id,
                };
                if *duration_ms != 0 {
                    let _ = inner
                        .client_mut(message_client)
                        .is_some_and(|client| std::mem::take(&mut client.message_ignore_keys));
                }
                let (previous, schedule) =
                    arm_client_message(inner, message_client, message_id, *duration_ms, *freeze);
                if let Some(previous) = previous {
                    after
                        .client_message_retires
                        .push((message_client, previous));
                }
                after.client_message_schedule = schedule;
                let event = DeferredClientEvent::Owned(publication);
                if message_client == client {
                    defer_unowned_client_events(
                        client,
                        &mut after.direct_events,
                        &mut after.deferred_direct_events,
                    );
                    after.deferred_direct_events.push(event);
                } else {
                    after.targeted_events.push(event);
                }
            } else {
                append_inserted_output(output, text);
            }
        }
        _ => unreachable!(),
    }
}
