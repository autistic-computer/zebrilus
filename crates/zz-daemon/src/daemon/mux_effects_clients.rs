use super::mux_effects::{CommandAftermath, EffectContext};
use super::*;

pub(super) fn apply_client_effect(
    inner: &mut ServerState,
    cx: EffectContext<'_>,
    after: &mut CommandAftermath,
    effect: &MuxEffect,
) -> Result<(), DaemonError> {
    let EffectContext {
        client,
        kind,
        context,
        ..
    } = cx;
    match effect {
        MuxEffect::Attach {
            session,
            detach_others,
            detach_others_hangup,
            read_only,
            flags,
            update_environment,
        } => {
            after.attach = Some((
                *session,
                *detach_others,
                *detach_others_hangup,
                *read_only,
                flags.clone(),
                *update_environment,
            ));
        }
        MuxEffect::SuspendClient { target_client } => {
            let target = resolve_client_target(inner, client, kind, target_client.as_deref())?;
            if inner.attachment.client_flags.contains(client) {
                return Err(ServerError::InvalidCommand("client is read-only".to_owned()).into());
            }
            if inner.client(target).and_then(|c| c.kind) != Some(ClientKind::Control)
                && inner
                    .client(target)
                    .and_then(|c| c.tty.as_ref())
                    .is_some_and(|tty| !tty.is_empty())
            {
                #[cfg(unix)]
                if let Some(pid) = inner
                    .client(target)
                    .and_then(|c| c.pid.as_ref())
                    .and_then(|pid| rustix::process::Pid::from_raw(*pid as i32))
                {
                    rustix::process::kill_process(pid, rustix::process::Signal::TSTP)
                        .map_err(|error| ServerError::InvalidCommand(error.to_string()))?;
                    if let Some(registered) = inner.client_mut(target) {
                        registered.suspended = true;
                    }
                }
            }
        }
        MuxEffect::Detach(request) => {
            let target_client =
                resolve_client_target(inner, client, kind, request.target_client.as_deref())?;
            if inner.attachment.client_flags.contains(client)
                && (!matches!(request.scope, DetachScope::Client) || target_client != client)
            {
                return Err(ServerError::InvalidCommand("client is read-only".to_owned()).into());
            }
            let scope = match &request.scope {
                DetachScope::Client => Some(ResolvedDetach::Client(target_client)),
                DetachScope::Others => Some(ResolvedDetach::Others(target_client)),
                DetachScope::Session(target) => {
                    match inner.engine.resolve_session(Some(target), context.session) {
                        Ok(session) => Some(ResolvedDetach::Session(session)),
                        Err(ServerError::SessionNotFound(_)) => None,
                        Err(error) => return Err(error.into()),
                    }
                }
            };
            after.detach = scope.map(|scope| {
                (
                    scope,
                    DetachExit {
                        exec: request.exec.clone(),
                        parent_hangup: request.parent_hangup,
                    },
                )
            });
        }
        _ => unreachable!(),
    }
    Ok(())
}
