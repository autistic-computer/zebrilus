use super::mux_effects::{CommandAftermath, EffectContext};
use super::*;

pub(super) fn apply_key_effect(
    inner: &mut ServerState,
    cx: EffectContext<'_>,
    after: &mut CommandAftermath,
    effect: &MuxEffect,
) -> Result<(), DaemonError> {
    let EffectContext {
        client,
        command,
        command_name,
        ..
    } = cx;
    match effect {
        MuxEffect::SendKeys { pane, keys, repeat } => {
            if is_agent_chat(inner, *pane) {
                return Err(agent_chat_refusal(command_name, *pane).into());
            }
            if inner.modes.pane_modes.contains_key(pane) {
                let target = if command_name == "send-keys" {
                    send_keys_target_client(&command.args)?
                } else {
                    None
                };
                let selected = match target {
                    Some(target) => find_attached_client_with_aliases(inner, &target, true),
                    None => current_format_client(inner, client),
                };
                if let Some(selected) = selected {
                    after
                        .pane_mode_keys
                        .push((selected, *pane, keys.clone(), *repeat));
                }
                return Ok(());
            }
            let owners = copy_mode_key_owners(inner, client, *pane);
            if !owners.is_empty() {
                after
                    .mode_table_keys
                    .push((owners, *pane, keys.clone(), *repeat));
                return Ok(());
            }
            let sinks = resolve_input_sinks(inner, *pane)?;
            let mut terminals = Vec::new();
            for sink in sinks {
                match sink {
                    PaneSink::Terminal(terminal) => {
                        terminals.push(terminal);
                    }
                    PaneSink::Browser(target) => {
                        after.client_events.push(EventPayload::BrowserCommand {
                            pane: target,
                            command: if *repeat == 1 {
                                BrowserCommand::SendKeys(keys.clone())
                            } else {
                                BrowserCommand::SendKeysRepeated {
                                    keys: keys.clone(),
                                    count: (*repeat).min(MAX_BROWSER_KEY_REPEAT),
                                }
                            },
                        });
                    }
                }
            }
            if !terminals.is_empty() {
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::SendTokens {
                        terminals,
                        keys: keys.clone(),
                        repeat: *repeat,
                    });
            }
        }
        MuxEffect::SendClientKeys {
            target_client,
            keys,
            repeat,
        } => {
            let selected = match target_client.as_deref() {
                Some(target) => find_attached_client_with_aliases(inner, target, true),
                None => current_format_client(inner, client),
            };
            if let Some(selected) = selected {
                after
                    .injected_client_keys
                    .push((selected, keys.clone(), *repeat));
            }
        }
        MuxEffect::ResetPane { pane } => {
            if is_agent_chat(inner, *pane) {
                return Err(agent_chat_refusal(command_name, *pane).into());
            }
            let mut terminals = Vec::new();
            for sink in resolve_input_sinks(inner, *pane)? {
                if let PaneSink::Terminal(terminal) = sink {
                    terminals.push(terminal);
                }
            }
            if !terminals.is_empty() {
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::ResetScreen { terminals });
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
