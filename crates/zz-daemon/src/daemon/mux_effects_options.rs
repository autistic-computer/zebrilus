use super::mux_effects::{CommandAftermath, EffectContext};
use super::*;

pub(super) fn apply_option_effect(
    inner: &mut ServerState,
    cx: EffectContext<'_>,
    after: &mut CommandAftermath,
    effect: &MuxEffect,
) -> Result<(), DaemonError> {
    let EffectContext { mux_source, .. } = cx;
    match effect {
        MuxEffect::BufferLimitChanged(limit) => {
            inner.buffers.automatic_paste_buffer_limit = AutomaticPasteBufferLimit(*limit);
        }
        MuxEffect::WordSeparatorsChanged { session } => {
            for (pane, terminal) in inner.terminals.iter() {
                let Some(window) = inner.engine.state.window_for_pane(*pane) else {
                    continue;
                };
                let pane_session = inner.engine.state.windows[&window].session;
                if session.is_some_and(|target| target != pane_session) {
                    continue;
                }
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::SetWordSeparators {
                        terminal: Arc::clone(terminal),
                        separators: WordSeparators::new(
                            inner.engine.word_separators_for_session(pane_session),
                        ),
                    });
            }
            for output in inner
                .clients
                .values()
                .filter_map(|c| c.command_output.as_ref())
            {
                let Some(window) = inner.engine.state.window_for_pane(output.pane) else {
                    continue;
                };
                let output_session = inner.engine.state.windows[&window].session;
                if session.is_some_and(|target| target != output_session) {
                    continue;
                }
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::SetWordSeparators {
                        terminal: Arc::clone(&output.terminal),
                        separators: WordSeparators::new(
                            inner.engine.word_separators_for_session(output_session),
                        ),
                    });
            }
        }
        MuxEffect::TerminalKnobsChanged { window, pane } => {
            for (candidate, terminal) in inner.terminals.iter() {
                if pane.is_some_and(|pane| pane != *candidate)
                    || window.is_some_and(|window| {
                        inner.engine.state.window_for_pane(*candidate) != Some(window)
                    })
                {
                    continue;
                }
                let terminal_options = terminal_worker_options(
                    &inner.engine,
                    &inner.config.appearance,
                    &inner.config.config_files,
                    *candidate,
                )?;
                after.deferred_terminal_commands.push(
                    DeferredTerminalCommand::SetAllowPassthrough {
                        terminal: Arc::clone(terminal),
                        enabled: terminal_options.allow_passthrough,
                    },
                );
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::SetWrapSearch {
                        terminal: Arc::clone(terminal),
                        enabled: terminal_options.wrap_search,
                    });
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::SetEngineKnobs {
                        terminal: Arc::clone(terminal),
                        knobs: terminal_options.knobs,
                    });
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::SetAppearance {
                        terminal: Arc::clone(terminal),
                        appearance: terminal_options.appearance,
                    });
            }
            for output in inner
                .clients
                .values()
                .filter_map(|c| c.command_output.as_ref())
            {
                if pane.is_some_and(|pane| pane != output.pane)
                    || window.is_some_and(|window| {
                        inner.engine.state.window_for_pane(output.pane) != Some(window)
                    })
                {
                    continue;
                }
                let terminal_options = terminal_worker_options(
                    &inner.engine,
                    &inner.config.appearance,
                    &inner.config.config_files,
                    output.pane,
                )?;
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::SetWrapSearch {
                        terminal: Arc::clone(&output.terminal),
                        enabled: terminal_options.wrap_search,
                    });
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::SetAppearance {
                        terminal: Arc::clone(&output.terminal),
                        appearance: terminal_options.appearance,
                    });
            }
        }
        MuxEffect::ModeKeysChanged { window } => {
            retarget_copy_mode_tables(inner, *window);
            let windows = window.map_or_else(
                || inner.engine.state.windows.keys().copied().collect(),
                |window| BTreeSet::from([window]),
            );
            for pane in panes_for_windows(inner, &windows) {
                let Some(terminal) = inner.terminals.get(&pane) else {
                    continue;
                };
                let options = terminal_worker_options(
                    &inner.engine,
                    &inner.config.appearance,
                    &inner.config.config_files,
                    pane,
                )?;
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::SetEngineKnobs {
                        terminal: Arc::clone(terminal),
                        knobs: options.knobs,
                    });
            }
        }
        MuxEffect::ResizeWindowFromClients { window, mode } => {
            let (columns, rows) = attached_client_window_extent(inner, *window, *mode)
                .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
            let generation = inner.engine.state.generation();
            inner
                .engine
                .set_manual_window_extent(*window, columns, rows)?;
            after.snapshot_changed |= inner.engine.state.generation() != generation;
            let windows = BTreeSet::from([*window]);
            let panes = panes_for_windows(inner, &windows);
            after.snapshot_changed |= write_back_terminal_geometries(inner, &panes);
            for (terminal, geometry) in terminal_resizes_for_panes(inner, &panes) {
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::Resize { terminal, geometry });
            }
        }
        MuxEffect::AggressiveResizeChanged { window } | MuxEffect::WindowSizeChanged { window } => {
            let windows = window.map_or_else(
                || inner.engine.state.windows.keys().copied().collect(),
                |window| BTreeSet::from([window]),
            );
            let panes = panes_for_windows(inner, &windows);
            after.snapshot_changed |= write_back_terminal_geometries(inner, &panes);
            for (terminal, geometry) in terminal_resizes_for_panes(inner, &panes) {
                after
                    .deferred_terminal_commands
                    .push(DeferredTerminalCommand::Resize { terminal, geometry });
            }
        }
        MuxEffect::MuxOptionChanged { option, session } => {
            if let Some(session) = session {
                after.mux_option_refresh_sessions.insert(*session);
            } else {
                let value = inner.engine.mux_option_value(*option);
                after.mux_options_changed |=
                    inner
                        .config
                        .mux_options
                        .set(*option, value.clone(), mux_source);
                if mux_source != MuxOptionSource::Override {
                    inner
                        .config
                        .mux_option_underlay
                        .set(*option, value, mux_source);
                }
            }
            #[cfg(feature = "agent")]
            {
                after.agent_options_changed |= matches!(
                    option,
                    MuxOptionKey::AgentCommand
                        | MuxOptionKey::AgentClaudeCodeCommand
                        | MuxOptionKey::AgentAutoApprove
                );
            }
        }
        MuxEffect::MonitorSilenceChanged => after.monitor_silence_changed = true,
        MuxEffect::ClientTerminalChanged => after.client_terminal_changed = true,
        MuxEffect::StatusFormatsChanged { session } => match session {
            Some(session) => {
                after.status_refresh_sessions.insert(*session);
            }
            None => after.status_formats_changed = true,
        },
        MuxEffect::StatusRowsChanged { session } => {
            let sessions = session.map_or_else(
                || inner.engine.state.sessions.keys().copied().collect(),
                |session| vec![session],
            );
            let mut resized = BTreeSet::new();
            for session in sessions {
                resized.extend(recalculate_window_extents(inner, session));
            }
            if !resized.is_empty() {
                after.snapshot_changed = true;
                let panes = panes_for_windows(inner, &resized);
                for (terminal, geometry) in terminal_resizes_for_panes(inner, &panes) {
                    after
                        .deferred_terminal_commands
                        .push(DeferredTerminalCommand::Resize { terminal, geometry });
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
