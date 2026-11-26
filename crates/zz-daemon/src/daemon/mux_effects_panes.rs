use super::mux_effects::{CommandAftermath, EffectContext};
use super::*;

impl Shared {
    pub(super) fn apply_pane_effect(
        &self,
        inner: &mut ServerState,
        cx: EffectContext<'_>,
        after: &mut CommandAftermath,
        hooks: &mut DaemonFormatHooks<'_>,
        effect: &MuxEffect,
    ) -> Result<(), DaemonError> {
        let EffectContext { context, .. } = cx;
        match effect {
            MuxEffect::PaneCreated {
                pane,
                kind: PaneKindSnapshot::Terminal,
                inherit_cwd_from,
                cwd,
                command,
                ..
            }
            | MuxEffect::PaneMaterialized {
                pane,
                kind: PaneKindSnapshot::Terminal,
                inherit_cwd_from,
                cwd,
                command,
            } => {
                let (pane_environment, empty) = match effect {
                    MuxEffect::PaneCreated {
                        environment, empty, ..
                    } => (environment.as_slice(), *empty),
                    MuxEffect::PaneMaterialized { .. } => (&[][..], false),
                    _ => unreachable!(),
                };
                let history_limit = inner.engine.history_limit_for_pane(*pane)?;
                let word_separators =
                    WordSeparators::new(inner.engine.word_separators_for_pane(*pane)?);
                let working_directory = match cwd.as_deref().map(PathBuf::from) {
                    Some(path) if path.is_dir() => Some(path),
                    Some(_) => std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .filter(|path| path.is_dir())
                        .or_else(|| Some(PathBuf::from("/"))),
                    None => inherit_cwd_from
                        .and_then(|source| donor_working_directory(inner, source))
                        .or_else(|| std::env::current_dir().ok()),
                };
                let start_path = working_directory
                    .as_deref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let terminal_options = terminal_worker_options(
                    &inner.engine,
                    &inner.config.appearance,
                    &inner.config.config_files,
                    *pane,
                )?;
                let appearance = Arc::clone(&terminal_options.appearance);
                let pane_session = inner
                    .engine
                    .state
                    .window_for_pane(*pane)
                    .map(|window| inner.engine.state.windows[&window].session)
                    .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
                let start_command =
                    if let Some(command) = command.clone().filter(|command| !command.is_empty()) {
                        Some(command)
                    } else {
                        let default = inner.engine.default_command_for_session(pane_session)?;
                        (!default.is_empty()).then(|| vec![default.to_owned()])
                    };
                let command = if empty {
                    command.clone()
                } else {
                    start_command.clone()
                };
                inner
                    .engine
                    .set_pane_start_command(*pane, start_command.unwrap_or_default())?;
                let shell = terminal_shell_for_session(&inner.engine, pane_session)?;
                inner.engine.set_pane_shell(*pane, shell.clone())?;
                let shell = Some(shell);
                let env = if empty {
                    Vec::new()
                } else {
                    let mut env = terminal_environment_for_session(&inner.engine, pane_session)?;
                    env.push((
                        "TMUX".into(),
                        Some(tmux_environment(&self.socket_path, Some(pane_session)).into()),
                    ));
                    env.extend(
                        pane_environment
                            .iter()
                            .map(|(name, value)| (name.into(), Some(value.into()))),
                    );
                    env.extend([
                        (OsString::from("ZZ_PANE"), Some(pane.to_string().into())),
                        (
                            "ZZ_SOCKET".into(),
                            Some(self.socket_path.as_os_str().to_owned()),
                        ),
                        ("ZZ_SESSION".into(), Some(pane_session.to_string().into())),
                        ("TMUX_PANE".into(), Some(pane.to_string().into())),
                    ]);
                    #[cfg(unix)]
                    if let Some(shim) = self.tmux_shim.lock().as_ref() {
                        crate::configure_pane_tmux_environment(
                            &mut env,
                            &shim.directory,
                            &shim.executable,
                        );
                    }
                    if let Some(path) = &working_directory {
                        env.push(("PWD".into(), Some(path.as_os_str().to_owned())));
                    }
                    env
                };
                let spawn = TerminalSpawn {
                    knobs: terminal_options.knobs,
                    working_directory: working_directory.clone(),
                    command,
                    shell,
                    terminal_type: Some(inner.engine.default_terminal_for_spawn().to_owned()),
                    initial_size: inner
                        .engine
                        .pane_geometry(*pane)
                        .map(|(columns, rows)| TerminalSize::cells(columns, rows)),
                    non_login_shell: false,
                    env,
                    word_separators: Some(word_separators.clone()),
                    allow_passthrough: Some(terminal_options.allow_passthrough),
                    wrap_search: Some(terminal_options.wrap_search),
                };
                let session = Arc::new(if empty {
                    TerminalSession::spawn_empty_pane(history_limit, appearance, spawn.initial_size)
                } else {
                    TerminalSession::spawn(history_limit, appearance, spawn.clone())
                });
                let current_path = if empty {
                    start_path.clone()
                } else {
                    terminal_working_directory(&session).map_or_else(
                        || start_path.clone(),
                        |path| path.to_string_lossy().into_owned(),
                    )
                };
                if empty {
                    session.set_word_separators(word_separators);
                    session.set_wrap_search(terminal_options.wrap_search);
                }
                if let Some(previous) = inner.terminals_mut().insert(*pane, Arc::clone(&session)) {
                    retire_terminal(&previous);
                }
                inner.panes.terminal_spawns.insert(*pane, spawn);
                if context.spawns_popup() {
                    inner.panes.popup_panes.insert(*pane);
                }
                inner.engine.set_pane_runtime_facts_with_hooks(
                    *pane,
                    PaneRuntimeFacts {
                        current_path,
                        start_path,
                        pid: session.process_id(),
                        tty: session
                            .tty()
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        ..PaneRuntimeFacts::default()
                    },
                    hooks,
                );
                let attached_clients = inner
                    .engine
                    .state
                    .window_for_pane(*pane)
                    .map(|window| inner.engine.state.windows[&window].session)
                    .and_then(|session| inner.attachment.attached.get(&session))
                    .cloned()
                    .unwrap_or_default();
                for client in attached_clients {
                    after
                        .deferred_terminal_commands
                        .push(DeferredTerminalCommand::AttachView {
                            terminal: Arc::clone(&session),
                            view: TerminalViewId(client.0),
                        });
                }
                after.terminals_to_watch.push((*pane, session));
            }
            MuxEffect::PaneRespawned {
                pane,
                cwd,
                command,
                environment,
                empty,
            } => {
                #[cfg(feature = "agent")]
                if inner.agents.agent_panes.remove(pane) {
                    after.agent_panes_closed.push(*pane);
                }
                clear_pane_modes(inner, *pane);
                let previous = inner
                    .panes
                    .terminal_spawns
                    .get(pane)
                    .cloned()
                    .unwrap_or_default();
                let history_limit = inner.engine.history_limit_for_pane(*pane)?;
                let word_separators =
                    WordSeparators::new(inner.engine.word_separators_for_pane(*pane)?);
                let working_directory = match cwd.as_deref().map(PathBuf::from) {
                    Some(path) => {
                        let path = if path.is_absolute() {
                            path
                        } else if let Some(base) = &previous.working_directory {
                            base.join(&path)
                        } else {
                            path
                        };
                        if path.is_dir() {
                            Some(path)
                        } else {
                            std::env::var_os("HOME")
                                .map(PathBuf::from)
                                .filter(|path| path.is_dir())
                                .or_else(|| Some(PathBuf::from("/")))
                        }
                    }
                    None => previous
                        .working_directory
                        .clone()
                        .or_else(|| std::env::current_dir().ok()),
                };
                let start_path = working_directory
                    .as_deref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let terminal_options = terminal_worker_options(
                    &inner.engine,
                    &inner.config.appearance,
                    &inner.config.config_files,
                    *pane,
                )?;
                let appearance = Arc::clone(&terminal_options.appearance);
                let pane_session = inner
                    .engine
                    .state
                    .window_for_pane(*pane)
                    .map(|window| inner.engine.state.windows[&window].session)
                    .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
                let command = command
                    .clone()
                    .filter(|command| !command.is_empty())
                    .or_else(|| previous.command.clone());
                let shell = match previous.shell.clone() {
                    Some(shell) => shell,
                    None => terminal_shell_for_session(&inner.engine, pane_session)?,
                };
                inner.engine.set_pane_shell(*pane, shell.clone())?;
                let shell = Some(shell);
                let mut env = terminal_environment_for_session(&inner.engine, pane_session)?;
                env.extend([
                    (OsString::from("ZZ_PANE"), Some(pane.to_string().into())),
                    (
                        "ZZ_SOCKET".into(),
                        Some(self.socket_path.as_os_str().to_owned()),
                    ),
                    ("ZZ_SESSION".into(), Some(pane_session.to_string().into())),
                    (
                        "TMUX".into(),
                        Some(tmux_environment(&self.socket_path, Some(pane_session)).into()),
                    ),
                ]);
                env.extend(
                    environment
                        .iter()
                        .map(|(name, value)| (name.into(), Some(value.into()))),
                );
                env.push(("TMUX_PANE".into(), Some(pane.to_string().into())));
                #[cfg(unix)]
                if let Some(shim) = self.tmux_shim.lock().as_ref() {
                    crate::configure_pane_tmux_environment(
                        &mut env,
                        &shim.directory,
                        &shim.executable,
                    );
                }
                if let Some(path) = &working_directory {
                    env.push(("PWD".into(), Some(path.as_os_str().to_owned())));
                }
                let spawn = TerminalSpawn {
                    knobs: terminal_options.knobs,
                    working_directory: working_directory.clone(),
                    command,
                    shell,
                    terminal_type: Some(inner.engine.default_terminal_for_spawn().to_owned()),
                    initial_size: inner
                        .engine
                        .pane_geometry(*pane)
                        .map(|(columns, rows)| TerminalSize::cells(columns, rows)),
                    non_login_shell: false,
                    env,
                    word_separators: Some(word_separators.clone()),
                    allow_passthrough: Some(terminal_options.allow_passthrough),
                    wrap_search: Some(terminal_options.wrap_search),
                };
                let session = Arc::new(if *empty {
                    TerminalSession::spawn_empty_pane(history_limit, appearance, spawn.initial_size)
                } else {
                    TerminalSession::spawn(history_limit, appearance, spawn.clone())
                });
                let current_path = if *empty {
                    start_path.clone()
                } else {
                    terminal_working_directory(&session).map_or_else(
                        || start_path.clone(),
                        |path| path.to_string_lossy().into_owned(),
                    )
                };
                if *empty {
                    session.set_word_separators(word_separators);
                    session.set_wrap_search(terminal_options.wrap_search);
                }
                Self::wake_pane_exit_wait(inner, *pane, 0);
                if let Some(entry) = inner.panes.pane_exit_waits.get_mut(pane) {
                    let command_wait = entry.command_wait.take();
                    *entry = PaneExitWait::new(&session);
                    entry.command_wait = command_wait;
                }
                if let Some(previous) = inner.terminals_mut().insert(*pane, Arc::clone(&session)) {
                    retire_terminal(&previous);
                }
                inner.panes.terminal_spawns.insert(*pane, spawn);
                if context.spawns_popup() {
                    inner.panes.popup_panes.insert(*pane);
                }
                for streamed in inner
                    .clients
                    .values_mut()
                    .filter_map(|c| c.streamed_terminals.as_mut())
                {
                    streamed.remove(pane);
                }
                for feed in inner
                    .clients
                    .values()
                    .filter_map(|c| c.subscriber.as_ref())
                    .filter_map(|subscriber| subscriber.control_feed())
                {
                    feed.reset_pane(*pane);
                }
                inner.panes.preview_watched.remove(pane);
                after.respawned_terminals = true;
                inner.engine.set_pane_runtime_facts_with_hooks(
                    *pane,
                    PaneRuntimeFacts {
                        current_path,
                        start_path,
                        ..PaneRuntimeFacts::default()
                    },
                    hooks,
                );
                let attached_clients = inner
                    .attachment
                    .attached
                    .get(&pane_session)
                    .cloned()
                    .unwrap_or_default();
                for client in attached_clients {
                    after
                        .deferred_terminal_commands
                        .push(DeferredTerminalCommand::AttachView {
                            terminal: Arc::clone(&session),
                            view: TerminalViewId(client.0),
                        });
                }
                if let Some((_, geometry)) = terminal_resize_for_pane(inner, *pane) {
                    after
                        .deferred_terminal_commands
                        .push(DeferredTerminalCommand::Resize {
                            terminal: Arc::clone(&session),
                            geometry,
                        });
                }
                if let Some(pipe) = inner.jobs.pane_pipes.get_mut(pane) {
                    *pipe.terminal.lock() = Arc::clone(&session);
                    #[cfg(unix)]
                    pipe.jobs.wake.wake();
                }
                after.terminals_to_watch.push((*pane, session));
            }
            MuxEffect::PaneCreated {
                pane,
                kind: PaneKindSnapshot::Agent(descriptor),
                inherit_cwd_from,
                ..
            }
            | MuxEffect::PaneMaterialized {
                pane,
                kind: PaneKindSnapshot::Agent(descriptor),
                inherit_cwd_from,
                ..
            } => {
                let working_directory = descriptor
                    .cwd
                    .clone()
                    .filter(|path| path.is_dir())
                    .or_else(|| {
                        inherit_cwd_from.and_then(|source| donor_working_directory(inner, source))
                    })
                    .or_else(|| std::env::current_dir().ok());
                let folder = working_directory
                    .as_deref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default();
                inner
                    .engine
                    .state
                    .update_agent(*pane, |agent| agent.cwd = working_directory)?;
                inner.engine.set_pane_runtime_facts(
                    *pane,
                    PaneRuntimeFacts {
                        current_path: folder.clone(),
                        start_path: folder,
                        ..PaneRuntimeFacts::default()
                    },
                );
                let replaced = inner.terminals_mut().remove(pane);
                if let Some(terminal) = &replaced {
                    clear_pane_modes(inner, *pane);
                    pane_summary::forget_pane_program(inner, *pane);
                    inner.panes.program_status_panes.remove(pane);
                    inner.panes.pane_read_observations.remove(pane);
                    for streamed in inner
                        .clients
                        .values_mut()
                        .filter_map(|c| c.streamed_terminals.as_mut())
                    {
                        streamed.remove(pane);
                    }
                    terminal.terminate();
                    retire_terminal(terminal);
                }
                after.agent_panes_opened.push((*pane, replaced));
            }
            MuxEffect::AgentForked { pane, at } => {
                #[cfg(feature = "agent")]
                inner.agents.agent_forks.insert(*pane, at.clone());
                #[cfg(not(feature = "agent"))]
                let _ = (pane, at);
            }
            MuxEffect::AgentBrief { pane, text } => {
                after.agent_briefs.push((*pane, text.clone()));
            }
            MuxEffect::PaneCreated {
                kind:
                    PaneKindSnapshot::Browser(_)
                    | PaneKindSnapshot::Picker
                    | PaneKindSnapshot::View(_)
                    | PaneKindSnapshot::Other { .. },
                ..
            }
            | MuxEffect::PaneMaterialized {
                kind:
                    PaneKindSnapshot::Browser(_)
                    | PaneKindSnapshot::Picker
                    | PaneKindSnapshot::View(_)
                    | PaneKindSnapshot::Other { .. },
                ..
            } => {}
            MuxEffect::PaneWaitForExit { pane } => {
                if let Some(terminal) = inner.terminals.get(pane).cloned() {
                    let entry = inner
                        .panes
                        .pane_exit_waits
                        .entry(*pane)
                        .or_insert_with(|| PaneExitWait::new(&terminal));
                    entry.command_wait = Some(Arc::clone(&entry.current));
                }
            }
            MuxEffect::PaneCreated {
                pane,
                kind: PaneKindSnapshot::Editor(_),
                inherit_cwd_from,
                ..
            }
            | MuxEffect::PaneMaterialized {
                pane,
                kind: PaneKindSnapshot::Editor(_),
                inherit_cwd_from,
                ..
            } => {
                let working_directory = inherit_cwd_from
                    .and_then(|source| donor_working_directory(inner, source))
                    .or_else(|| std::env::current_dir().ok());
                if let Some(working_directory) = working_directory {
                    inner.engine.state.update_editor_cwd(
                        *pane,
                        working_directory.to_string_lossy().into_owned(),
                    )?;
                }
            }
            MuxEffect::PanesRemoved(panes) => {
                let clients = inner
                    .clients
                    .iter()
                    .filter_map(|(id, client)| {
                        client.command_output.as_ref().map(|value| (id, value))
                    })
                    .filter_map(|(client, output)| panes.contains(&output.pane).then_some(*client))
                    .collect::<Vec<_>>();
                for client in clients {
                    if let Some(output) = take_command_output(inner, client) {
                        after.retired_command_outputs.push((client, output));
                    }
                }
                for pane in panes {
                    if let Some(pipe) = inner.jobs.pane_pipes.remove(pane) {
                        after.pipes_to_close.push(pipe);
                    }
                    if let Some(editor) = inner.modes.editors.remove(pane) {
                        let _ = std::fs::remove_file(&editor.path);
                    }
                    Self::release_removed_pane_wait(inner, *pane);
                    if let Some(terminal) = inner.terminals_mut().remove(pane) {
                        retire_terminal(&terminal);
                    }
                    inner.panes.name_checks.remove(pane);
                    pane_summary::forget_pane_program(inner, *pane);
                    inner.panes.pane_read_observations.remove(pane);
                    inner.panes.control_activity_pending.remove(pane);
                    inner.panes.program_status_panes.remove(pane);
                    inner.panes.popup_panes.remove(pane);
                    #[cfg(all(feature = "agent", unix))]
                    inner.agents.claude_peer_states.remove(pane);
                    inner.panes.terminal_spawns.remove(pane);
                    inner.panes.terminal_geometries.remove(pane);
                    inner.panes.reported_pane_places.remove(pane);
                    inner.modes.pane_modes.remove(pane);
                    inner.modes.pane_mode_zooms.remove(pane);
                    inner.modes.panes_mode_frames.remove(pane);
                    inner.engine.set_pane_status_hidden(*pane, false);
                    inner
                        .transfers
                        .paste_uploads
                        .retain(|_, upload| upload.pane != *pane);
                    after.removed_panes.push(*pane);
                }
            }
            MuxEffect::PaneRelocated { pane, from, to } => {
                if let Some(terminal) = inner.terminals.get(pane).cloned() {
                    let terminal_options = terminal_worker_options(
                        &inner.engine,
                        &inner.config.appearance,
                        &inner.config.config_files,
                        *pane,
                    )?;
                    after.deferred_terminal_commands.push(
                        DeferredTerminalCommand::SetWordSeparators {
                            terminal: Arc::clone(&terminal),
                            separators: WordSeparators::new(
                                inner.engine.word_separators_for_session(*to),
                            ),
                        },
                    );
                    after.deferred_terminal_commands.push(
                        DeferredTerminalCommand::SetAllowPassthrough {
                            terminal: Arc::clone(&terminal),
                            enabled: terminal_options.allow_passthrough,
                        },
                    );
                    after
                        .deferred_terminal_commands
                        .push(DeferredTerminalCommand::SetWrapSearch {
                            terminal: Arc::clone(&terminal),
                            enabled: terminal_options.wrap_search,
                        });
                    after
                        .deferred_terminal_commands
                        .push(DeferredTerminalCommand::SetAppearance {
                            terminal: Arc::clone(&terminal),
                            appearance: terminal_options.appearance,
                        });
                    let previewing = previewing_clients(inner);
                    after.relocated_terminal_views.push((
                        terminal,
                        inner
                            .attachment
                            .attached
                            .get(from)
                            .cloned()
                            .unwrap_or_default()
                            .union(&previewing)
                            .copied()
                            .collect::<BTreeSet<_>>(),
                        inner
                            .attachment
                            .attached
                            .get(to)
                            .cloned()
                            .unwrap_or_default()
                            .union(&previewing)
                            .copied()
                            .collect::<BTreeSet<_>>(),
                    ));
                }
                for output in inner
                    .clients
                    .values()
                    .filter_map(|c| c.command_output.as_ref())
                    .filter(|output| output.pane == *pane)
                {
                    let terminal_options = terminal_worker_options(
                        &inner.engine,
                        &inner.config.appearance,
                        &inner.config.config_files,
                        *pane,
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
            _ => unreachable!(),
        }
        Ok(())
    }
}
