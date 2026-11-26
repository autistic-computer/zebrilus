use super::*;

impl Shared {
    pub(super) fn register_overlay_wait(
        self: &Arc<Self>,
        client: ClientId,
        commands: Option<Vec<CommandInvocation>>,
        queue_execution: Option<&CommandQueueExecution>,
    ) -> OverlayTicket {
        let continuation = cmdq::WaitContinuation::new(
            self.command_item
                .as_ref()
                .and_then(|item| item.lock().wait()),
            self.client_writers.lock().get(&client).map(Arc::downgrade),
        );
        let waiter = OverlayWait(Arc::new(OverlayCompletion {
            client,
            continuation: continuation.clone(),
            accepted: Arc::new(AtomicBool::new(false)),
            owner: Arc::downgrade(&self.server_owner()),
            completing: Mutex::new(()),
        }));
        let mut item = self.command_item.as_ref().expect("command item").lock();
        #[cfg(unix)]
        let loop_leaf = item.loop_leaf;
        #[cfg(not(unix))]
        let loop_leaf = false;
        if (item.loop_wait || loop_leaf)
            && queue_execution.is_some_and(|queue| queue.frame_active.get())
        {
            item.pending_wait = Some(Box::new(RegisteredWait {
                name: String::new(),
                continuation,
                #[cfg(unix)]
                shell: None,
                overlay: commands.map(|commands| OverlayCommands {
                    commands,
                    accepted: Arc::clone(&waiter.0.accepted),
                }),
                leaf: None,
                guard: None,
                terminal: None,
                file: None,
            }));
        }
        let ticket = OverlayTicket {
            continuation: waiter.0.continuation.clone(),
            accepted: Arc::clone(&waiter.0.accepted),
        };
        item.raising_overlay = Some(waiter);
        ticket
    }

    pub(super) fn finish_overlay_wait(&self, waiter: &OverlayTicket, succeeded: bool) {
        let queued = {
            let mut item = self.command_item.as_ref().expect("command item").lock();
            item.raising_overlay.take();
            if !succeeded {
                item.pending_wait.take();
            }
            item.pending_wait.is_some()
        };
        if !succeeded {
            return;
        }
        self.report_command_queue_park();
        if queued || waiter.continuation.ready() {
            return;
        }
        #[cfg(any(test, windows))]
        waiter.continuation.wait();
        #[cfg(all(unix, not(test)))]
        let _ = waiter;
    }

    pub(super) fn display_popup(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        command_name: &str,
        args: &[RawText],
        mux_source: MuxOptionSource,
        invoking_client_terminal: ClientTerminal,
        queue_execution: Option<&CommandQueueExecution>,
        format_facts_unread: Option<bool>,
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_display_popup_args(args)?;
        if kind == ClientKind::Control {
            return Ok(Execution::default());
        }
        let (target_client, mut target, window) = {
            let inner = self.inner.lock();
            let target_client =
                resolve_popup_client(&inner, client, context, parsed.target_client.as_deref())?;
            let pane = inner.engine.resolve_pane(
                parsed.target_pane.as_deref(),
                context.window,
                context.pane,
            )?;
            let mut target = ExecutionContext::for_pane(&inner.engine.state, pane)
                .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
            target.set_format_client_session(client_attached_session(&inner, target_client));
            target
                .format_variables
                .clone_from(&context.format_variables);
            let window = target
                .window
                .ok_or_else(|| ServerError::MissingTarget("current window".to_owned()))?;
            (target_client, target, window)
        };
        let modal = self.inner.lock().engine.state.windows[&window].modal_pane();
        if parsed.clear {
            let mut events = Vec::new();
            let mut outputs = Vec::new();
            let mut resume_terminals = false;
            dismiss_overlays(
                &mut self.inner.lock(),
                target_client,
                None,
                &mut events,
                &mut outputs,
                &mut resume_terminals,
            );
            for event in events {
                self.publish_to_client(target_client, event);
            }
            if resume_terminals {
                self.resume_client_terminals(target_client);
            }
            for (owner, output) in outputs {
                self.retire_command_output(owner, output);
            }
            let Some(modal) = modal else {
                return Ok(Execution::default());
            };
            let kill = CommandInvocation::new("kill-pane", ["-t".to_owned(), modal.to_string()]);
            return self.execute_with_mux_source_inner(
                client,
                kind,
                context,
                &kill,
                mux_source,
                invoking_client_terminal,
                queue_execution,
                format_facts_unread,
            );
        }
        if modal.is_some() {
            return Ok(Execution::default());
        }
        let (new_pane, remain_on_exit) = {
            let inner = self.inner.lock();
            let columns = inner
                .engine
                .window_extent(window, zz_protocol::Axis::Horizontal)
                .unwrap_or_default();
            let rows = inner
                .engine
                .window_extent(window, zz_protocol::Axis::Vertical)
                .unwrap_or_default();
            let width = parse_popup_dimension("width", parsed.width.as_deref(), columns)?;
            let height = parse_popup_dimension("height", parsed.height.as_deref(), rows)?;
            let defaults = inner.engine.popup_options_for_window(window)?;
            let border_lines = if parsed.borderless {
                PopupBorderLines::None
            } else if let Some(value) = parsed.border_lines.as_deref() {
                value.parse().map_err(|()| {
                    ServerError::InvalidCommand(format!(
                        "popup-border-lines unknown value: {value}"
                    ))
                })?
            } else {
                defaults.border_lines
            };
            let minimum = if border_lines == PopupBorderLines::None {
                1
            } else {
                3
            };
            if width < minimum || height < minimum {
                return Ok(Execution::default());
            }
            let variables = popup_position_variables(
                &inner.engine,
                &target,
                context.invoking_mouse(),
                columns,
                rows,
                width,
                height,
            );
            target.format_variables.extend(variables);
            let expand = |value: &str| {
                expand_popup_value(
                    &inner,
                    &target,
                    context.session,
                    Some(command_name),
                    value,
                    Some(target_client),
                )
            };
            let (left, top) = popup_position(
                parsed.x.as_deref(),
                parsed.y.as_deref(),
                columns,
                rows,
                width,
                height,
                expand,
            );
            let escape = |value: String| value.replace('#', "##");
            let mut arguments = vec![
                "-O".to_owned(),
                "-K".to_owned(),
                "-t".to_owned(),
                target
                    .pane
                    .ok_or_else(|| ServerError::MissingTarget("current pane".to_owned()))?
                    .to_string(),
                "-x".to_owned(),
                width.to_string(),
                "-y".to_owned(),
                height.to_string(),
                "-X".to_owned(),
                left.to_string(),
                "-Y".to_owned(),
                top.to_string(),
                "-B".to_owned(),
                popup_pane_border_lines(border_lines).to_owned(),
                "-s".to_owned(),
                parsed.style.clone().unwrap_or(defaults.style),
            ];
            let border_style = parsed.border_style.clone().unwrap_or(defaults.border_style);
            arguments.extend([
                "-S".to_owned(),
                border_style.clone(),
                "-R".to_owned(),
                border_style,
            ]);
            if parsed.close_on_exit_count == 0 {
                arguments.push("-D".to_owned());
            }
            if let Some(title) = parsed.title.as_deref() {
                arguments.extend(["-T".to_owned(), escape(expand(title))]);
            }
            if let Some(directory) = parsed.start_directory.as_deref() {
                arguments.extend(["-c".to_owned(), escape(expand(directory))]);
            }
            for assignment in &parsed.environment {
                arguments.extend(["-e".to_owned(), assignment.clone()]);
            }
            if matches!(kind, ClientKind::Command) {
                arguments.push("-W".to_owned());
            }
            let command = match parsed.command.as_slice() {
                [only] if only.is_empty() => &[][..],
                command => command,
            };
            if !command.is_empty() {
                arguments.push("--".to_owned());
                arguments.extend(command.iter().cloned());
            }
            let remain_on_exit = match (parsed.close_on_exit_count, parsed.close_on_any_key) {
                (0, false) => "on",
                (0 | 1, true) => "key",
                (1, false) => "off",
                (_, true) => "failed-key",
                (_, false) => "failed",
            };
            (
                CommandInvocation::new("new-pane", arguments),
                remain_on_exit,
            )
        };
        let mut spawn_options = vec![
            ("remain-on-exit", remain_on_exit),
            ("remain-on-exit-format", ""),
        ];
        if parsed.title.is_some() {
            spawn_options.extend([
                ("pane-border-status", "top"),
                ("pane-border-format", "#{pane_title}"),
            ]);
        }
        let mut new_context = target.clone();
        new_context.set_spawns_popup(true);
        new_context.set_spawn_pane_options(
            spawn_options
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
        );
        let execution = self.execute_with_mux_source_inner(
            client,
            kind,
            &mut new_context,
            &new_pane,
            mux_source,
            invoking_client_terminal,
            queue_execution,
            format_facts_unread,
        )?;
        self.publish_snapshot();
        Ok(execution)
    }

    pub(super) fn spawn_editor(
        self: &Arc<Self>,
        client: ClientId,
        pane: PaneId,
        text: &[u8],
        target: EditorTarget,
    ) {
        let (columns, rows, lines, editor) = {
            let inner = self.inner.lock();
            let Some(window) = inner.engine.state.window_for_pane(pane) else {
                return;
            };
            if inner.engine.state.windows[&window].modal_pane().is_some() {
                return;
            }
            let columns = inner
                .engine
                .window_extent(window, zz_protocol::Axis::Horizontal)
                .unwrap_or_default();
            let rows = inner
                .engine
                .window_extent(window, zz_protocol::Axis::Vertical)
                .unwrap_or_default();
            (
                columns,
                rows,
                inner.engine.pane_border_lines(window),
                inner.engine.editor_option(),
            )
        };
        let Ok(file) = tempfile::Builder::new()
            .prefix("tmux.")
            .rand_bytes(8)
            .tempfile_in("/tmp")
        else {
            return;
        };
        let text = if text.is_empty() {
            b"\n".as_slice()
        } else {
            text
        };
        if std::fs::write(file.path(), text).is_err() {
            return;
        }
        let Ok(path) = file.into_temp_path().keep() else {
            return;
        };
        let sx = i32::from(columns) * 9 / 10;
        let sy = i32::from(rows) * 9 / 10;
        let xoff = i32::from(columns) / 2 - sx / 2;
        let yoff = i32::from(rows) / 2 - sy / 2;
        let border = i32::from(lines != zz_protocol::PaneBorderLines::None);
        let new_pane = CommandInvocation::new(
            "new-pane",
            [
                "-O".to_owned(),
                "-t".to_owned(),
                pane.to_string(),
                "-c".to_owned(),
                "/tmp".to_owned(),
                "-x".to_owned(),
                (sx + 2 * border).to_string(),
                "-y".to_owned(),
                (sy + 2 * border).to_string(),
                "-X".to_owned(),
                (xoff - border).to_string(),
                "-Y".to_owned(),
                (yoff - border).to_string(),
                "--".to_owned(),
                format!("{editor} {}", path.display()),
            ],
        );
        let Some(mut context) = ExecutionContext::for_pane(&self.inner.lock().engine.state, pane)
        else {
            return;
        };
        context.set_spawn_pane_options(vec![("remain-on-exit".to_owned(), "off".to_owned())]);
        let created = self
            .execute(client, ClientKind::Interactive, &mut context, &new_pane)
            .ok()
            .and_then(|execution| {
                execution.effects.iter().find_map(|effect| match effect {
                    MuxEffect::PaneCreated { pane, .. } => Some(*pane),
                    _ => None,
                })
            });
        let Some(editor_pane) = created else {
            let _ = std::fs::remove_file(&path);
            return;
        };
        self.inner.lock().modes.editors.insert(
            editor_pane,
            EditorSession {
                client,
                path,
                target,
            },
        );
        self.publish_snapshot();
    }

    pub(super) fn finish_editor(self: &Arc<Self>, pane: PaneId, exit_code: u8) {
        let Some(session) = self.inner.lock().modes.editors.remove(&pane) else {
            return;
        };
        let text = (exit_code == 0)
            .then(|| std::fs::read(&session.path).ok())
            .flatten()
            .filter(|text| !text.is_empty());
        let _ = std::fs::remove_file(&session.path);
        let Some(mut text) = text else {
            return;
        };
        match session.target {
            EditorTarget::Buffer { name, data } => {
                {
                    let mut inner = self.inner.lock();
                    let Some(buffer) = inner
                        .buffers
                        .paste_buffers
                        .iter_mut()
                        .find(|buffer| buffer.name == name && Arc::ptr_eq(&buffer.data, &data))
                    else {
                        return;
                    };
                    if buffer.data.last().is_some_and(|last| *last != b'\n')
                        && text.last() == Some(&b'\n')
                    {
                        text.pop();
                    }
                    if text.is_empty() {
                        return;
                    }
                    buffer.data = Arc::from(text);
                }
                self.run_event_hooks(vec![PendingHookEvent::paste_buffer(
                    "paste-buffer-changed",
                    name,
                )]);
                self.refresh_choose_buffers();
            }
            EditorTarget::Option { pane, edit } => {
                if text.last() == Some(&b'\n') {
                    text.pop();
                }
                let value = String::from_utf8_lossy(&text).into_owned();
                let Some(PaneModeRequest::Customize(mut mode)) = self
                    .inner
                    .lock()
                    .modes
                    .pane_modes
                    .get(&pane)
                    .and_then(|modes| modes.last())
                    .cloned()
                else {
                    return;
                };
                let result = {
                    let inner = self.inner.lock();
                    let facts = borrowed_format_hook_facts(&inner);
                    let mut expand = customize_expander(&inner, pane, &facts);
                    inner
                        .engine
                        .customize_edited(pane, &mut mode, &edit, &value, &mut expand)
                };
                let Some(mut context) =
                    ExecutionContext::for_pane(&self.inner.lock().engine.state, pane)
                else {
                    return;
                };
                self.apply_customize_result(session.client, &mut context, pane, mode, &result);
            }
        }
    }

    pub(super) fn refit_client_overlays(self: &Arc<Self>, client: ClientId) {
        let Ok(Some(geometry)) = popup_client_geometry(&self.inner.lock(), client) else {
            return;
        };
        self.refit_client_menu(client, geometry);
    }

    fn refit_client_menu(self: &Arc<Self>, client: ClientId, geometry: TerminalGeometry) {
        let menu = {
            let mut inner = self.inner.lock();
            let Some(menu) = inner.client_mut(client).and_then(|c| c.menu.as_mut()) else {
                return;
            };
            let refit = MenuState {
                left: overlay_origin_for_viewport(
                    menu.state.left,
                    menu.state.width,
                    geometry.columns,
                ),
                top: overlay_origin_for_viewport(menu.state.top, menu.state.height, geometry.rows),
                client_columns: geometry.columns,
                client_rows: geometry.rows,
                cell_width_px: geometry.cell_width_px,
                cell_height_px: geometry.cell_height_px,
                ..menu.state.clone()
            };
            if refit == menu.state {
                return;
            }
            menu.state = refit.clone();
            refit
        };
        self.publish_to_client(client, EventPayload::Menu { state: Some(menu) });
    }

    pub(super) fn restyle_client_overlays(self: &Arc<Self>) {
        let clients = {
            let inner = self.inner.lock();
            inner
                .clients
                .iter()
                .filter_map(|(id, client)| client.menu.as_ref().map(|_| id))
                .copied()
                .collect::<BTreeSet<_>>()
        };
        for client in clients {
            self.restyle_client_menu(client);
        }
    }

    fn restyle_client_menu(self: &Arc<Self>, client: ClientId) {
        let state = {
            let mut inner = self.inner.lock();
            let Some(window) = client_overlay_style_window(&inner, client) else {
                return;
            };
            let Ok(options) = inner.engine.menu_options_for_window(window) else {
                return;
            };
            let Some(menu) = inner.client_mut(client).and_then(|c| c.menu.as_mut()) else {
                return;
            };
            let style = overlay_style(&options.style, menu.styles.style.as_deref());
            let selected_style = overlay_style(
                &options.selected_style,
                menu.styles.selected_style.as_deref(),
            );
            let border_style =
                overlay_style(&options.border_style, menu.styles.border_style.as_deref());
            if style == menu.state.style
                && selected_style == menu.state.selected_style
                && border_style == menu.state.border_style
            {
                return;
            }
            menu.state.style = style;
            menu.state.selected_style = selected_style;
            menu.state.border_style = border_style;
            menu.state.clone()
        };
        self.publish_to_client(client, EventPayload::Menu { state: Some(state) });
    }

    pub(super) fn display_menu(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        command_name: &str,
        command: &CommandInvocation,
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_display_menu_args(&command.args)?;
        if kind == ClientKind::Control {
            resolve_popup_client(
                &self.inner.lock(),
                client,
                context,
                parsed.target_client.as_deref(),
            )?;
            validate_display_menu_items(&parsed.items)?;
            return Ok(Execution::default());
        }
        let (target_client, mut target) = {
            let inner = self.inner.lock();
            let target_client =
                resolve_popup_client(&inner, client, context, parsed.target_client.as_deref())?;
            let pane = inner.engine.resolve_pane(
                parsed.target_pane.as_deref(),
                context.window,
                context.pane,
            )?;
            let mut target = ExecutionContext::for_pane(&inner.engine.state, pane)
                .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
            target.set_format_client_session(client_attached_session(&inner, target_client));
            target
                .format_variables
                .clone_from(&context.format_variables);
            (target_client, target)
        };
        if any_overlay_present(&self.inner.lock(), target_client) {
            return Ok(Execution::default());
        }
        let starting_choice = menu_starting_number(parsed.starting_choice.as_deref())?;
        let (state, commands) = {
            let inner = self.inner.lock();
            let Some(geometry) = popup_client_geometry(&inner, target_client)? else {
                return Ok(Execution::default());
            };
            let window = target
                .window
                .ok_or_else(|| ServerError::MissingTarget("current window".to_owned()))?;
            let defaults = inner.engine.menu_options_for_window(
                client_overlay_style_window(&inner, target_client).unwrap_or(window),
            )?;
            let title = parsed.title.as_deref().map_or_else(String::new, |title| {
                expand_popup_value(
                    &inner,
                    &target,
                    context.session,
                    Some(command_name),
                    title,
                    Some(target_client),
                )
            });
            let row_room = usize::from(geometry.columns.saturating_sub(MENU_ROW_MARGIN));
            let mut items = Vec::new();
            let mut commands = Vec::new();
            let mut index = 0;
            while index < parsed.items.len() {
                let name = &parsed.items[index];
                index = index.saturating_add(1);
                if name.is_empty() {
                    if items.last().is_some_and(Option::is_some) {
                        items.push(None);
                        commands.push(None);
                    }
                    continue;
                }
                if parsed.items.len().saturating_sub(index) < 2 {
                    return Err(ServerError::CommandParse("not enough arguments".to_owned()).into());
                }
                let key = &parsed.items[index];
                let command_index = index.saturating_add(1);
                let action = &parsed.items[command_index];
                index = index.saturating_add(2);
                let name = expand_popup_value(
                    &inner,
                    &target,
                    context.session,
                    Some(command_name),
                    name,
                    Some(target_client),
                );
                if name.is_empty() {
                    continue;
                }
                let enabled = !name.starts_with('-');
                let key = enabled.then(|| menu_shortcut(key)).flatten();
                let layout = layout_menu_row(&name, key.as_deref(), row_room);
                let name = if enabled {
                    layout.name
                } else {
                    layout
                        .name
                        .strip_prefix('-')
                        .unwrap_or(&layout.name)
                        .to_owned()
                };
                let annotation = layout.annotation;
                let action = if command.argument_is_command_block(
                    parsed.positional_start.saturating_add(command_index),
                ) {
                    command_block_body(action).unwrap_or(action).trim()
                } else {
                    action
                };
                let action = expand_popup_value(
                    &inner,
                    &target,
                    context.session,
                    Some(command_name),
                    action,
                    Some(target_client),
                );
                items.push(Some(MenuItem {
                    name,
                    key,
                    annotation,
                    enabled,
                }));
                commands.push(Some(action));
            }
            if items.is_empty() {
                return Ok(Execution::default());
            }
            let width = items
                .iter()
                .flatten()
                .map(|item| menu_row_cells(&item.name, item.annotation.as_deref()))
                .max()
                .unwrap_or_default()
                .max(menu_row_width(&title))
                .saturating_add(usize::from(MENU_ROW_MARGIN));
            let height = items.len().saturating_add(2);
            let (Ok(width), Ok(height)) = (u16::try_from(width), u16::try_from(height)) else {
                return Ok(Execution::default());
            };
            if width > geometry.columns || height > geometry.rows {
                return Ok(Execution::default());
            }
            let selected = (!parsed.mouse && context.invoking_mouse().is_none())
                .then(|| menu_starting_selection(&starting_choice, &items))
                .flatten();
            let border_lines = if let Some(value) = parsed.border_lines.as_deref() {
                value.parse().map_err(|()| {
                    ServerError::InvalidCommand(format!("menu-border-lines unknown value: {value}"))
                })?
            } else {
                defaults.border_lines
            };
            let style = overlay_style(&defaults.style, parsed.style.as_deref());
            let selected_style =
                overlay_style(&defaults.selected_style, parsed.selected_style.as_deref());
            let border_style =
                overlay_style(&defaults.border_style, parsed.border_style.as_deref());
            let frame = menu_window_frame(&inner.engine, &target, geometry.columns, geometry.rows);
            let mut variables = popup_position_variables(
                &inner.engine,
                &target,
                None,
                frame.columns,
                frame.rows,
                width,
                height,
            );
            if let Some(mouse) = context.invoking_mouse() {
                variables.extend(
                    menu_mouse_position_values(&inner.engine, &target, mouse, frame, width, height)
                        .into_iter()
                        .map(|(name, value)| (name.to_owned(), value.to_string())),
                );
            }
            if variables.contains_key(POPUP_STATUS_LINE_Y_CONTEXT_FORMAT) {
                variables.insert(
                    POPUP_STATUS_LINE_Y_CONTEXT_FORMAT.to_owned(),
                    if frame.top > 0 { height } else { frame.rows }.to_string(),
                );
            }
            target.format_variables.extend(variables);
            let (left, top) = popup_position(
                parsed.x.as_deref(),
                parsed.y.as_deref(),
                frame.columns,
                frame.rows,
                width,
                height,
                |value| {
                    expand_popup_value(
                        &inner,
                        &target,
                        context.session,
                        Some(command_name),
                        value,
                        Some(target_client),
                    )
                },
            );
            let top = top.saturating_add(frame.top);
            (
                MenuState {
                    left,
                    top,
                    width,
                    height,
                    client_columns: geometry.columns,
                    client_rows: geometry.rows,
                    cell_width_px: geometry.cell_width_px,
                    cell_height_px: geometry.cell_height_px,
                    title,
                    style,
                    selected_style,
                    border_style,
                    border_lines,
                    items,
                    selected,
                    stay_open: parsed.stay_open,
                    mouse_keys: parsed.mouse || context.invoking_mouse().is_some(),
                },
                commands,
            )
        };
        {
            let mut inner = self.inner.lock();
            if any_overlay_present(&inner, target_client) {
                return Ok(Execution::default());
            }
            inner.client_entry(target_client).menu.replace(MenuSession {
                state: state.clone(),
                commands,
                target,
                mode_tree: None,
                styles: OverlayStyleOverrides {
                    style: parsed.style.clone(),
                    selected_style: parsed.selected_style.clone(),
                    border_style: parsed.border_style.clone(),
                },
            });
        }
        self.publish_to_client(target_client, EventPayload::Menu { state: Some(state) });
        Ok(Execution::default())
    }

    pub(super) fn raise_mode_tree_menu(
        &self,
        client: ClientId,
        pane: PaneId,
        menu: &CustomizeMenu,
        x: usize,
        y: usize,
    ) {
        let state = {
            let inner = self.inner.lock();
            if inner.client(client).is_some_and(|c| c.menu.is_some()) {
                return;
            }
            let Ok(Some(geometry)) = popup_client_geometry(&inner, client) else {
                return;
            };
            let table: &[Option<CustomizeMenuItem>] = if menu.outside {
                &CUSTOMIZE_OUTSIDE_MENU_ITEMS
            } else {
                &CUSTOMIZE_MENU_ITEMS
            };
            let row_room = usize::from(geometry.columns.saturating_sub(MENU_ROW_MARGIN));
            let mut items: Vec<Option<MenuItem>> = Vec::new();
            for entry in table {
                let Some(item) = entry else {
                    items.push(None);
                    continue;
                };
                let layout = layout_menu_row(item.name, Some(item.annotation), row_room);
                items.push(Some(MenuItem {
                    name: layout.name,
                    key: Some(item.key.to_owned()),
                    annotation: layout.annotation,
                    enabled: true,
                }));
            }
            let title = if menu.outside {
                String::new()
            } else {
                format!("#[align=centre]{}", menu.name)
            };
            let width = items
                .iter()
                .flatten()
                .map(|item| menu_row_cells(&item.name, item.annotation.as_deref()))
                .max()
                .unwrap_or_default()
                .max(menu_row_width(&title))
                .saturating_add(usize::from(MENU_ROW_MARGIN));
            let height = items.len().saturating_add(2);
            let (Ok(width), Ok(height)) = (u16::try_from(width), u16::try_from(height)) else {
                return;
            };
            if width > geometry.columns || height > geometry.rows {
                return;
            }
            let Some(window) = client_overlay_style_window(&inner, client)
                .or_else(|| inner.engine.state.window_for_pane(pane))
            else {
                return;
            };
            let Ok(defaults) = inner.engine.menu_options_for_window(window) else {
                return;
            };
            let Some(target) = ExecutionContext::for_pane(&inner.engine.state, pane) else {
                return;
            };
            let frame = menu_window_frame(&inner.engine, &target, geometry.columns, geometry.rows);
            let (xoff, yoff) = inner.engine.pane_origin(pane).unwrap_or((0, 0));
            let place = |origin: usize, offset: i32, extent: u16, available: u16| {
                let origin = i64::try_from(origin)
                    .unwrap_or(i64::MAX)
                    .saturating_add(i64::from(offset))
                    .max(0);
                let origin = u16::try_from(origin).unwrap_or(u16::MAX);
                if extent >= available {
                    0
                } else {
                    overlay_origin_for_viewport(origin, extent, available)
                }
            };
            let left = place(
                x.saturating_sub(usize::from(width) / 2),
                xoff,
                width,
                frame.columns,
            );
            let top = place(y, yoff, height, frame.rows).saturating_add(frame.top);
            MenuState {
                left,
                top,
                width,
                height,
                client_columns: geometry.columns,
                client_rows: geometry.rows,
                cell_width_px: geometry.cell_width_px,
                cell_height_px: geometry.cell_height_px,
                title,
                style: overlay_style(&defaults.style, None),
                selected_style: overlay_style(&defaults.selected_style, None),
                border_style: overlay_style(&defaults.border_style, None),
                border_lines: defaults.border_lines,
                items,
                selected: None,
                stay_open: false,
                mouse_keys: true,
            }
        };
        {
            let mut inner = self.inner.lock();
            if inner.client(client).is_some_and(|c| c.menu.is_some()) {
                return;
            }
            let Some(target) = ExecutionContext::for_pane(&inner.engine.state, pane) else {
                return;
            };
            inner.client_entry(client).menu.replace(MenuSession {
                state: state.clone(),
                commands: vec![None; state.items.len()],
                target,
                styles: OverlayStyleOverrides::default(),
                mode_tree: Some(ModeTreeMenu {
                    pane,
                    line: menu.line,
                    outside: menu.outside,
                }),
            });
        }
        self.publish_to_client(client, EventPayload::Menu { state: Some(state) });
    }

    pub(super) fn input_menu(
        self: &Arc<Self>,
        client: ClientId,
        context: &ExecutionContext,
        action: MenuAction,
    ) {
        let session = {
            let mut inner = self.inner.lock();
            let Some(menu) = inner.client(client).and_then(|c| c.menu.as_ref()) else {
                return;
            };
            if let MenuAction::Choose(index) = action {
                let selectable = usize::try_from(index)
                    .ok()
                    .and_then(|index| menu.state.items.get(index))
                    .and_then(Option::as_ref)
                    .is_some_and(|item| item.enabled);
                if !selectable && menu.state.stay_open {
                    return;
                }
            }
            inner
                .client_mut(client)
                .and_then(|c| c.menu.take())
                .expect("menu was present")
        };
        self.publish_to_client(client, EventPayload::Menu { state: None });
        let MenuAction::Choose(index) = action else {
            return;
        };
        if let Some(owner) = session.mode_tree {
            self.mode_tree_menu_choice(client, context, &owner, index);
            return;
        }
        let Some(command) = usize::try_from(index)
            .ok()
            .and_then(|index| session.commands.get(index))
            .and_then(Option::as_ref)
        else {
            return;
        };
        if !session
            .state
            .items
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .and_then(Option::as_ref)
            .is_some_and(|item| item.enabled)
        {
            return;
        }
        let mut target = session.target;
        let target_alive = target
            .pane
            .is_some_and(|pane| self.inner.lock().engine.state.pane(pane).is_some());
        if !target_alive {
            target = context.clone();
        }
        self.execute_overlay_source_with_error_case(
            client,
            &mut target,
            command,
            "display-menu",
            true,
            None,
        );
    }

    pub(super) fn execute_overlay_source_with_error_case(
        self: &Arc<Self>,
        client: ClientId,
        context: &mut ExecutionContext,
        command: &str,
        title: &str,
        uppercase_error: bool,
        then: Option<OverlaySourceThen>,
    ) {
        let source = InsertedCommandSource::String(command.to_owned());
        #[cfg(unix)]
        if self.loop_active.load(Ordering::Acquire) {
            let label = format!("<{title}>");
            let title = title.to_owned();
            self.enqueue_inserted_task(
                client,
                ClientKind::Interactive,
                context,
                &source,
                &label,
                None,
                false,
                None,
                Box::new(move |shared, context, result| {
                    match result {
                        Ok(result) => shared.route_background_inserted_output(
                            client,
                            ClientKind::Interactive,
                            context,
                            title,
                            &result.output,
                        ),
                        Err(error) => shared.publish_background_command_error(
                            client,
                            context,
                            &error,
                            uppercase_error,
                        ),
                    }
                    if let Some(then) = then {
                        then(shared);
                    }
                }),
            );
            return;
        }
        match self.execute_inserted_commands(
            client,
            ClientKind::Interactive,
            context,
            &source,
            &format!("<{title}>"),
        ) {
            Ok(result) => self.route_background_inserted_output(
                client,
                ClientKind::Interactive,
                context,
                title.to_owned(),
                &result.output,
            ),
            Err(error) => {
                self.publish_background_command_error(client, context, &error, uppercase_error);
            }
        }
        if let Some(then) = then {
            then(self);
        }
    }
}

#[derive(Clone)]
pub(super) struct OverlayTicket {
    continuation: cmdq::WaitContinuation,
    pub(super) accepted: Arc<AtomicBool>,
}

fn client_overlay_style_window(inner: &ServerState, client: ClientId) -> Option<WindowId> {
    let session = client_attached_session(inner, client)?;
    inner
        .engine
        .state
        .sessions
        .get(&session)
        .map(|state| state.active_window)
}

pub(super) fn overlay_style(option: &str, explicit: Option<&str>) -> String {
    let Some(explicit) = explicit.filter(|explicit| zz_protocol::valid_style(explicit)) else {
        return option.to_owned();
    };
    let mut foreground = Some("fg=default");
    let mut background = Some("bg=default");
    for token in explicit
        .split([' ', ',', '\n'])
        .filter(|token| !token.is_empty())
    {
        if token.eq_ignore_ascii_case("default") {
            foreground = None;
            background = None;
        } else if style_field_is(token, "fg=") {
            foreground = Some(token);
        } else if style_field_is(token, "bg=") {
            background = Some(token);
        }
    }
    let mut composed = option.to_owned();
    for field in [foreground, background].into_iter().flatten() {
        if !composed.is_empty() {
            composed.push(',');
        }
        composed.push_str(field);
    }
    composed
}

fn style_field_is(token: &str, field: &str) -> bool {
    token.len() > field.len()
        && token.as_bytes()[..field.len()].eq_ignore_ascii_case(field.as_bytes())
}

fn client_format_geometry(
    inner: &ServerState,
    client: ClientId,
    window: WindowId,
) -> Option<TerminalGeometry> {
    client_format_geometry_from_source(&ClientFormatSource::from_inner(inner), client, window)
}

pub(super) fn popup_client_geometry(
    inner: &ServerState,
    client: ClientId,
) -> Result<Option<TerminalGeometry>, ServerError> {
    let session_id = client_attached_session(inner, client)
        .ok_or_else(|| ServerError::InvalidCommand("no current client".to_owned()))?;
    let session = inner
        .engine
        .state
        .sessions
        .get(&session_id)
        .ok_or_else(|| ServerError::MissingTarget(session_id.to_string()))?;
    let window_id = client_focused_window(inner, client, session);
    if inner.client(client).and_then(|c| c.kind) == Some(ClientKind::Control) {
        return Ok(control_client_geometry(inner, client, window_id));
    }
    inner
        .engine
        .state
        .windows
        .get(&window_id)
        .ok_or_else(|| ServerError::MissingTarget(window_id.to_string()))?;
    let measured = client_format_geometry(inner, client, window_id);
    let (columns, rows) = inner
        .client(client)
        .and_then(|c| c.size)
        .unwrap_or_else(|| {
            (
                inner
                    .engine
                    .window_extent(window_id, zz_protocol::Axis::Horizontal)
                    .unwrap_or(80),
                inner
                    .engine
                    .window_extent(window_id, zz_protocol::Axis::Vertical)
                    .unwrap_or(24),
            )
        });
    Ok(Some(measured.map_or(
        TerminalGeometry {
            columns,
            rows,
            cell_width_px: CONTROL_CELL_WIDTH_PX,
            cell_height_px: CONTROL_CELL_HEIGHT_PX,
        },
        |geometry| TerminalGeometry {
            columns,
            rows,
            ..geometry
        },
    )))
}

fn popup_position_variables(
    engine: &MuxEngine,
    target: &ExecutionContext,
    mouse: Option<&MouseEventTarget>,
    columns: u16,
    rows: u16,
    width: u16,
    height: u16,
) -> BTreeMap<String, String> {
    let status = engine.pane_format_geometry(target.session, target.window, target.pane);
    let status_row = engine.status_formats_for_session(target.session);
    let status_lines = if status_row.enabled {
        u16::from(status_row.lines)
    } else {
        0
    };
    let status_line_y = (status_lines > 0).then(|| {
        if status_row.position == zz_protocol::StatusPosition::Top {
            i64::from(status_lines) + i64::from(height)
        } else {
            i64::from(rows.saturating_sub(status_lines))
        }
    });
    let centre_x = i64::from(columns.saturating_sub(1)) / 2 - i64::from(width) / 2;
    let centre_x = centre_x.max(0);
    let centre_y = i64::from(rows.saturating_sub(1)) / 2 + i64::from(height) / 2;
    let centre_y = if centre_y >= i64::from(rows) {
        i64::from(rows.saturating_sub(height))
    } else {
        centre_y
    };
    let pane_left = status.pane_left.unwrap_or_default();
    let pane_top = status.pane_top.unwrap_or_default();
    let pane_width = status.pane_width.unwrap_or(columns);
    let pane_height = status.pane_height.unwrap_or(rows);
    let pane_popup_top = i64::from(pane_top) + i64::from(height);
    let pane_popup_top = if pane_popup_top >= i64::from(rows) {
        i64::from(rows.saturating_sub(height))
    } else {
        pane_popup_top
    };
    let pane_right = i64::from(pane_left) + i64::from(pane_width) - i64::from(width);
    let mut variables = BTreeMap::from([
        (POPUP_WIDTH_CONTEXT_FORMAT.to_owned(), width.to_string()),
        (POPUP_HEIGHT_CONTEXT_FORMAT.to_owned(), height.to_string()),
        (
            POPUP_CENTRE_X_CONTEXT_FORMAT.to_owned(),
            centre_x.to_string(),
        ),
        (
            POPUP_CENTRE_Y_CONTEXT_FORMAT.to_owned(),
            centre_y.to_string(),
        ),
        (
            POPUP_PANE_TOP_CONTEXT_FORMAT.to_owned(),
            pane_popup_top.to_string(),
        ),
        (
            POPUP_PANE_BOTTOM_CONTEXT_FORMAT.to_owned(),
            pane_top.saturating_add(pane_height).to_string(),
        ),
        (
            POPUP_PANE_LEFT_CONTEXT_FORMAT.to_owned(),
            pane_left.to_string(),
        ),
        (
            POPUP_PANE_RIGHT_CONTEXT_FORMAT.to_owned(),
            pane_right.max(0).to_string(),
        ),
        (POPUP_LAST_X_CONTEXT_FORMAT.to_owned(), "0".to_owned()),
        (POPUP_LAST_Y_CONTEXT_FORMAT.to_owned(), height.to_string()),
    ]);
    if let Some(status_line_y) = status_line_y {
        variables.insert(
            POPUP_STATUS_LINE_Y_CONTEXT_FORMAT.to_owned(),
            status_line_y.to_string(),
        );
    }
    if let Some(mouse) = mouse {
        for (name, value) in popup_mouse_position_values(engine, target, mouse, rows, width, height)
        {
            variables.insert(name.to_owned(), value.to_string());
        }
    }
    variables
}

fn popup_mouse_position_values(
    engine: &MuxEngine,
    target: &ExecutionContext,
    mouse: &MouseEventTarget,
    rows: u16,
    width: u16,
    height: u16,
) -> Vec<(&'static str, i64)> {
    let (rows, width, height) = (i64::from(rows), i64::from(width), i64::from(height));
    let (x, y) = (i64::from(mouse.column), i64::from(mouse.row));
    let centre_y = y - height / 2;
    let mut values = vec![
        (POPUP_MOUSE_X_CONTEXT_FORMAT, x),
        (POPUP_MOUSE_Y_CONTEXT_FORMAT, y),
        (POPUP_MOUSE_CENTRE_X_CONTEXT_FORMAT, (x - width / 2).max(0)),
        (
            POPUP_MOUSE_CENTRE_Y_CONTEXT_FORMAT,
            if centre_y + height >= rows {
                rows - height
            } else {
                centre_y
            },
        ),
        (
            POPUP_MOUSE_TOP_CONTEXT_FORMAT,
            if y + height >= rows {
                rows - 1
            } else {
                y + height
            },
        ),
        (POPUP_MOUSE_BOTTOM_CONTEXT_FORMAT, (y - height).max(0)),
    ];
    if let Some(start) = mouse.status_range_start {
        let top = engine.status_formats_for_session(target.session).position
            == zz_protocol::StatusPosition::Top;
        values.push((POPUP_WINDOW_STATUS_LINE_X_CONTEXT_FORMAT, i64::from(start)));
        values.push((
            POPUP_WINDOW_STATUS_LINE_Y_CONTEXT_FORMAT,
            if top { y + 1 + height } else { y },
        ));
    }
    values
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MenuFrame {
    columns: u16,
    rows: u16,
    top: u16,
    status_row: Option<u16>,
    visible_rows: u16,
}

fn menu_window_frame(
    engine: &MuxEngine,
    target: &ExecutionContext,
    client_columns: u16,
    client_rows: u16,
) -> MenuFrame {
    let status = engine.status_formats_for_session(target.session);
    let lines = if status.enabled {
        u16::from(status.lines).min(client_rows)
    } else {
        0
    };
    let status_top = status.position == zz_protocol::StatusPosition::Top;
    let available = client_rows.saturating_sub(lines);
    let extent = |axis| {
        target
            .window
            .and_then(|window| engine.window_extent(window, axis))
    };
    let columns = extent(zz_protocol::Axis::Horizontal).unwrap_or(client_columns);
    let rows = extent(zz_protocol::Axis::Vertical).unwrap_or(available);
    let visible_rows = if client_columns >= columns && available >= rows {
        rows
    } else {
        available
    };
    MenuFrame {
        columns,
        rows,
        top: if status_top { lines } else { 0 },
        status_row: (!status_top && lines > 0).then_some(available),
        visible_rows,
    }
}

fn menu_mouse_position_values(
    engine: &MuxEngine,
    target: &ExecutionContext,
    mouse: &MouseEventTarget,
    frame: MenuFrame,
    width: u16,
    height: u16,
) -> Vec<(&'static str, i64)> {
    let row = if frame.top > 0 {
        mouse.row.saturating_sub(frame.top)
    } else if frame.status_row.is_some_and(|status| mouse.row >= status) {
        frame.visible_rows.saturating_sub(1)
    } else {
        mouse.row
    };
    let moved = MouseEventTarget {
        row,
        status_range_start: None,
        ..mouse.clone()
    };
    let mut values = popup_mouse_position_values(engine, target, &moved, frame.rows, width, height);
    if let Some(start) = mouse.status_range_start {
        values.push((POPUP_WINDOW_STATUS_LINE_X_CONTEXT_FORMAT, i64::from(start)));
        values.push((
            POPUP_WINDOW_STATUS_LINE_Y_CONTEXT_FORMAT,
            if frame.top > 0 {
                i64::from(height)
            } else {
                i64::from(frame.rows)
            },
        ));
    }
    values
}

pub(super) fn popup_position(
    x: Option<&str>,
    y: Option<&str>,
    columns: u16,
    rows: u16,
    width: u16,
    height: u16,
    mut expand: impl FnMut(&str) -> String,
) -> (u16, u16) {
    let x = match x.unwrap_or("C") {
        "C" => "#{popup_centre_x}",
        "R" => "#{popup_pane_right}",
        "P" => "#{popup_pane_left}",
        "M" => "#{popup_mouse_centre_x}",
        "L" => "#{popup_last_x}",
        "W" => "#{popup_window_status_line_x}",
        value => value,
    };
    let y = match y.unwrap_or("C") {
        "C" => "#{popup_centre_y}",
        "P" => "#{popup_pane_bottom}",
        "M" => "#{popup_mouse_top}",
        "L" => "#{popup_last_y}",
        "S" => "#{popup_status_line_y}",
        "W" => "#{popup_window_status_line_y}",
        value => value,
    };
    let left = popup_position_value(&expand(x), columns);
    let left = left.clamp(0, i64::from(columns.saturating_sub(width)));
    let bottom = popup_position_value(&expand(y), rows);
    let top = if bottom < i64::from(height) {
        0
    } else {
        bottom - i64::from(height)
    };
    let top = top.clamp(0, i64::from(rows.saturating_sub(height)));
    (
        u16::try_from(left).unwrap_or_default(),
        u16::try_from(top).unwrap_or_default(),
    )
}

fn popup_position_value(value: &str, dimension: u16) -> i64 {
    if let Some(percent) = value.strip_suffix('%') {
        let percent = percent.parse::<u64>().unwrap_or_default();
        return i64::try_from(
            u64::from(dimension)
                .saturating_mul(percent)
                .checked_div(100)
                .unwrap_or_default(),
        )
        .unwrap_or(i64::MAX);
    }
    value.parse().unwrap_or_default()
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ParsedDisplayPopup {
    pub(super) borderless: bool,
    pub(super) clear: bool,
    pub(super) close_on_exit_count: u8,
    pub(super) close_on_any_key: bool,
    pub(super) reset_close_flags: bool,
    pub(super) border_lines: Option<String>,
    pub(super) target_client: Option<String>,
    pub(super) start_directory: Option<String>,
    pub(super) environment: Vec<String>,
    pub(super) height: Option<String>,
    pub(super) style: Option<String>,
    pub(super) border_style: Option<String>,
    pub(super) target_pane: Option<String>,
    pub(super) title: Option<String>,
    pub(super) width: Option<String>,
    pub(super) x: Option<String>,
    pub(super) y: Option<String>,
    pub(super) command: Vec<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ParsedDisplayMenu {
    mouse: bool,
    stay_open: bool,
    border_lines: Option<String>,
    target_client: Option<String>,
    starting_choice: Option<String>,
    selected_style: Option<String>,
    style: Option<String>,
    border_style: Option<String>,
    target_pane: Option<String>,
    title: Option<String>,
    x: Option<String>,
    y: Option<String>,
    items: Vec<String>,
    positional_start: usize,
}

pub(super) fn parse_display_popup_args(
    args: &[RawText],
) -> Result<ParsedDisplayPopup, ServerError> {
    let mut parsed = ParsedDisplayPopup::default();
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--" {
            parsed.command.extend(
                args[index.saturating_add(1)..]
                    .iter()
                    .map(ToString::to_string),
            );
            break;
        }
        if !argument.starts_with('-') || argument == "-" {
            parsed
                .command
                .extend(args[index..].iter().map(ToString::to_string));
            break;
        }
        for (offset, flag) in argument[1..].char_indices() {
            match flag {
                'B' => parsed.borderless = true,
                'C' => parsed.clear = true,
                'E' => {
                    parsed.close_on_exit_count = parsed.close_on_exit_count.saturating_add(1);
                }
                'k' => parsed.close_on_any_key = true,
                'N' => parsed.reset_close_flags = true,
                'b' | 'c' | 'd' | 'e' | 'h' | 's' | 'S' | 't' | 'T' | 'w' | 'x' | 'y' => {
                    let value_start = 1_usize
                        .saturating_add(offset)
                        .saturating_add(flag.len_utf8());
                    let value = if value_start < argument.len() {
                        argument[value_start..].to_owned()
                    } else {
                        index = index.saturating_add(1);
                        args.get(index).map(ToString::to_string).ok_or_else(|| {
                            ServerError::CommandParse(format!(
                                "display-popup -{flag} requires a value"
                            ))
                        })?
                    };
                    match flag {
                        'b' => parsed.border_lines = Some(value),
                        'c' => parsed.target_client = Some(value),
                        'd' => parsed.start_directory = Some(value),
                        'e' => parsed.environment.push(value),
                        'h' => parsed.height = Some(value),
                        's' => parsed.style = Some(value),
                        'S' => parsed.border_style = Some(value),
                        't' => parsed.target_pane = Some(value),
                        'T' => parsed.title = Some(value),
                        'w' => parsed.width = Some(value),
                        'x' => parsed.x = Some(value),
                        'y' => parsed.y = Some(value),
                        _ => unreachable!("valued popup option is matched"),
                    }
                    break;
                }
                _ => {
                    return Err(ServerError::CommandParse(format!(
                        "unsupported command: display-popup -{flag}"
                    )));
                }
            }
        }
        index = index.saturating_add(1);
    }
    Ok(parsed)
}

fn parse_display_menu_args(args: &[RawText]) -> Result<ParsedDisplayMenu, ServerError> {
    let mut parsed = ParsedDisplayMenu::default();
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--" {
            parsed.items.extend(
                args[index.saturating_add(1)..]
                    .iter()
                    .map(ToString::to_string),
            );
            break;
        }
        if !argument.starts_with('-') || argument == "-" {
            parsed
                .items
                .extend(args[index..].iter().map(ToString::to_string));
            break;
        }
        for (offset, flag) in argument[1..].char_indices() {
            match flag {
                'M' => parsed.mouse = true,
                'O' => parsed.stay_open = true,
                'b' | 'c' | 'C' | 'H' | 's' | 'S' | 't' | 'T' | 'x' | 'y' => {
                    let value_start = 1_usize
                        .saturating_add(offset)
                        .saturating_add(flag.len_utf8());
                    let value = if value_start < argument.len() {
                        argument[value_start..].to_owned()
                    } else {
                        index = index.saturating_add(1);
                        args.get(index).map(ToString::to_string).ok_or_else(|| {
                            ServerError::CommandParse(format!(
                                "display-menu -{flag} requires a value"
                            ))
                        })?
                    };
                    match flag {
                        'b' => parsed.border_lines = Some(value),
                        'c' => parsed.target_client = Some(value),
                        'C' => parsed.starting_choice = Some(value),
                        'H' => parsed.selected_style = Some(value),
                        's' => parsed.style = Some(value),
                        'S' => parsed.border_style = Some(value),
                        't' => parsed.target_pane = Some(value),
                        'T' => parsed.title = Some(value),
                        'x' => parsed.x = Some(value),
                        'y' => parsed.y = Some(value),
                        _ => unreachable!("valued menu option is matched"),
                    }
                    break;
                }
                _ => {
                    return Err(ServerError::CommandParse(format!(
                        "unsupported command: display-menu -{flag}"
                    )));
                }
            }
        }
        index = index.saturating_add(1);
    }
    zz_protocol::catalog_command_spec("display-menu")
        .expect("display-menu has catalog metadata")
        .validate_positional_minimum(parsed.items.len())?;
    parsed.positional_start = args.len().saturating_sub(parsed.items.len());
    Ok(parsed)
}

fn validate_display_menu_items(items: &[String]) -> Result<(), ServerError> {
    let mut index = 0;
    while index < items.len() {
        let name = &items[index];
        index = index.saturating_add(1);
        if name.is_empty() {
            continue;
        }
        if items.len().saturating_sub(index) < 2 {
            return Err(ServerError::InvalidCommand(
                "not enough arguments".to_owned(),
            ));
        }
        index = index.saturating_add(2);
    }
    Ok(())
}

pub(super) fn parse_popup_dimension(
    name: &str,
    value: Option<&str>,
    dimension: u16,
) -> Result<u16, ServerError> {
    let Some(value) = value else {
        return Ok(dimension / 2);
    };
    let error = |reason: &str| ServerError::InvalidCommand(format!("{name} {reason}"));
    if value.is_empty() {
        return Err(error("empty"));
    }
    if let Some(percent) = value.strip_suffix('%') {
        if percent.is_empty() {
            return Err(error("invalid"));
        }
        let percent = parse_popup_number(percent, 0, 1000).map_err(error)?;
        let cells = u64::from(dimension)
            .saturating_mul(percent)
            .checked_div(100)
            .unwrap_or_default();
        if cells == 0 {
            return Err(error("too small"));
        }
        if cells > u64::from(dimension) {
            return Err(error("too large"));
        }
        return Ok(u16::try_from(cells).unwrap_or(dimension));
    }
    let cells = parse_popup_number(value, 1, u64::from(dimension)).map_err(error)?;
    Ok(u16::try_from(cells).unwrap_or(dimension))
}

fn parse_popup_number(value: &str, minimum: u64, maximum: u64) -> Result<u64, &'static str> {
    if value.starts_with('-') {
        return Err("too small");
    }
    match value.parse::<u64>() {
        Ok(value) if value < minimum => Err("too small"),
        Ok(value) if value > maximum => Err("too large"),
        Ok(value) => Ok(value),
        Err(_) if value.chars().all(|character| character.is_ascii_digit()) => Err("too large"),
        Err(_) => Err("invalid"),
    }
}

fn popup_pane_border_lines(lines: PopupBorderLines) -> &'static str {
    match lines {
        PopupBorderLines::Rounded => "single",
        PopupBorderLines::Padded => "spaces",
        other => other.as_str(),
    }
}

fn overlay_origin_for_viewport(origin: u16, extent: u16, available: u16) -> u16 {
    if origin.saturating_add(extent) > available {
        available.saturating_sub(extent)
    } else {
        origin
    }
}

fn menu_shortcut(value: &str) -> Option<String> {
    let key = zz_protocol::canonical_key(&zz_mux::parse_tmux_key(value)?);
    (is_key_name(&key) || key == MENU_BACK_TAB_KEY).then_some(key)
}

enum MenuStartingChoice {
    Default,
    Unselected,
    At(u64),
}

fn menu_starting_number(value: Option<&str>) -> Result<MenuStartingChoice, ServerError> {
    let Some(value) = value else {
        return Ok(MenuStartingChoice::Default);
    };
    if value == "-" {
        return Ok(MenuStartingChoice::Unselected);
    }
    parse_popup_number(value, 0, u64::from(u32::MAX))
        .map(MenuStartingChoice::At)
        .map_err(|cause| ServerError::InvalidCommand(format!("starting choice {cause}")))
}

fn menu_starting_selection(
    starting_choice: &MenuStartingChoice,
    items: &[Option<MenuItem>],
) -> Option<u32> {
    match starting_choice {
        MenuStartingChoice::Default => menu_selectable_from(items, 0, false),
        MenuStartingChoice::Unselected => None,
        MenuStartingChoice::At(start) => {
            let start = usize::try_from(*start).unwrap_or(usize::MAX);
            if start >= items.len() {
                menu_selectable_from(items, items.len().saturating_sub(1), true)
            } else {
                menu_selectable_from(items, start, false)
            }
        }
    }
}

fn menu_selectable_from(items: &[Option<MenuItem>], start: usize, reverse: bool) -> Option<u32> {
    if items.is_empty() {
        return None;
    }
    (0..items.len()).find_map(|offset| {
        let index = if reverse {
            (start + items.len() - offset % items.len()) % items.len()
        } else {
            (start + offset) % items.len()
        };
        items[index]
            .as_ref()
            .is_some_and(|item| item.enabled)
            .then(|| u32::try_from(index).unwrap_or(u32::MAX))
    })
}
