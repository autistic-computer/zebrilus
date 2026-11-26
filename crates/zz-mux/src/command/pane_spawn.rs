use super::*;

impl MuxEngine {
    pub fn set_pane_start_command(
        &mut self,
        pane: PaneId,
        command: Vec<String>,
    ) -> Result<(), ServerError> {
        if self.state.pane(pane).is_none() {
            return Err(ServerError::MissingTarget(pane.to_string()));
        }
        self.pane_start_commands.insert(pane, command);
        self.format_data_generation = self.format_data_generation.wrapping_add(1);
        Ok(())
    }

    pub(crate) fn pane_start_command(&self, pane: PaneId) -> Option<&[String]> {
        self.pane_start_commands.get(&pane).map(Vec::as_slice)
    }

    pub fn set_pane_shell(&mut self, pane: PaneId, shell: String) -> Result<(), ServerError> {
        if self.state.pane(pane).is_none() {
            return Err(ServerError::MissingTarget(pane.to_string()));
        }
        if self.pane_shells.get(&pane) != Some(&shell) {
            self.pane_shells.insert(pane, shell);
            self.format_data_generation = self.format_data_generation.wrapping_add(1);
        }
        Ok(())
    }

    fn default_window_name(&self, pane: PaneId) -> String {
        match self.state.pane(pane) {
            Some(state) if !matches!(state.kind, PaneKind::Terminal) => state.title.clone(),
            Some(_) => self.pane_command_fallback(pane),
            None => String::new(),
        }
    }

    pub(crate) fn pane_command_fallback(&self, pane: PaneId) -> String {
        let command = self
            .pane_start_command(pane)
            .unwrap_or_default()
            .iter()
            .map(|argument| crate::formats::quote_argument(argument))
            .collect::<Vec<_>>()
            .join(" ");
        if !command.is_empty() {
            return parse_window_name(&command);
        }
        self.pane_shells
            .get(&pane)
            .map(|shell| parse_window_name(shell))
            .unwrap_or_default()
    }

    pub(super) fn retain_pane_start_commands(&mut self, effects: &[MuxEffect]) {
        for effect in effects {
            match effect {
                MuxEffect::PaneCreated { pane, command, .. }
                | MuxEffect::PaneMaterialized { pane, command, .. } => {
                    let command = command.clone().unwrap_or_default();
                    if self.pane_start_commands.get(pane) != Some(&command) {
                        self.pane_start_commands.insert(*pane, command);
                        self.format_data_generation = self.format_data_generation.wrapping_add(1);
                    }
                }
                MuxEffect::PaneRespawned {
                    pane,
                    command: Some(command),
                    ..
                } if !command.is_empty() => {
                    if self.pane_start_commands.get(pane) != Some(command) {
                        self.pane_start_commands.insert(*pane, command.clone());
                        self.format_data_generation = self.format_data_generation.wrapping_add(1);
                    }
                }
                MuxEffect::PanesRemoved(panes) => {
                    for pane in panes {
                        self.pane_shells.remove(pane);
                        if self.pane_start_commands.remove(pane).is_some() {
                            self.format_data_generation =
                                self.format_data_generation.wrapping_add(1);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn split_window(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
        stdin: Option<&RawText>,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("split-window", args)?;
        self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let (kind, brief) =
            self.spawn_pane_kind("split-window", context, &options, &positional, hooks)?;
        let command = matches!(kind, PaneKind::Terminal)
            .then(|| shell_command_positional(&positional))
            .flatten();
        pane_spawn_empty(&options, command.as_deref())?;
        check_spawn_border_lines(&options)?;
        let floating = match self.split_floating_target(context, &options)? {
            Some(Err(cause)) => return Err(ServerError::InvalidCommand(cause)),
            Some(Ok((target, old, spawn))) => {
                let _ = self.state.set_float_geometry(target, old);
                Some(spawn)
            }
            None => None,
        };
        let mut execution = self.split_window_with_options(
            context,
            &options,
            kind,
            command,
            split_size(&options),
            true,
            floating,
            hooks,
        )?;
        if options.has("-I")
            && let Some(bytes) = stdin
            && let Some(pane) = execution.effects.iter().find_map(|effect| match effect {
                MuxEffect::PaneCreated { pane, .. } => Some(*pane),
                _ => None,
            })
        {
            execution.effects.push(MuxEffect::PaneStreamInput {
                pane,
                bytes: bytes.clone(),
            });
        }
        queue_agent_brief(&mut execution, brief);
        Ok(execution)
    }

    pub(super) fn new_pane(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
        stdin: Option<&RawText>,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (mut options, positional) = parse_command_options("new-pane", args)?;
        let floating = !options.has("-L");
        let target = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        if let Some(target_context) = ExecutionContext::for_pane(&self.state, target) {
            self.expand_float_values(&mut options, &target_context, context, hooks);
        }
        let window = self
            .state
            .window_for_pane(target)
            .expect("resolved pane has a window");
        if options.has("-O") {
            if !floating {
                return Err(ServerError::InvalidCommand(
                    "modal pane must be floating".to_owned(),
                ));
            }
            if self.state.windows[&window].modal_pane().is_some() {
                return Err(ServerError::InvalidCommand(
                    "window already has a modal pane".to_owned(),
                ));
            }
        }
        let drag_mouse = context.invoking_mouse().cloned();
        if options.has("-M") && floating && drag_mouse.is_none() {
            return Ok(Execution::default());
        }
        let (kind, brief) =
            self.spawn_pane_kind("new-pane", context, &options, &positional, hooks)?;
        if floating && matches!(kind, PaneKind::Agent(_)) {
            for (flag, size) in [("-x", AGENT_FLOAT_WIDTH), ("-y", AGENT_FLOAT_HEIGHT)] {
                if options.value(flag).is_none() {
                    options.values.push((flag.to_owned(), size.to_owned()));
                }
            }
        }
        let place = (floating && matches!(kind, PaneKind::Simulator(_))).then(|| {
            let (columns, rows) = self.state.windows[&window].layout.extent();
            for (flag, size) in [
                ("-x", SIMULATOR_FLOAT_WIDTH.min(columns)),
                ("-y", SIMULATOR_FLOAT_HEIGHT.min(rows)),
            ] {
                if options.value(flag).is_none() {
                    options.values.push((flag.to_owned(), size.to_string()));
                }
            }
            SIMULATOR_FLOAT_PLACE
        });
        let command = matches!(kind, PaneKind::Terminal)
            .then(|| shell_command_positional(&positional))
            .flatten();
        pane_spawn_empty(&options, command.as_deref())?;
        check_spawn_border_lines(&options)?;
        let spawn = if floating {
            let lines = match options.value("-B") {
                Some(lines) => PaneBorderLines::parse(lines),
                None => self.pane_border_lines(window),
            };
            let border = lines != PaneBorderLines::None;
            let geometry = self
                .float_geometry(window, &options, border, None, place)
                .map_err(ServerError::InvalidCommand)?;
            let modal = options.has("-O").then(|| Modal {
                capture_keys: options.has("-K"),
                close_on_click: options.has("-C"),
                close_on_cancel: options.has("-D"),
                ..Modal::default()
            });
            Some(FloatSpawn {
                geometry,
                over_zoom: options.has("-A") || options.has("-O"),
                modal,
                detached: options.has("-d"),
                before: options.has("-b"),
                full_size: options.has("-f"),
            })
        } else {
            None
        };
        let spawn_options = std::mem::take(&mut context.spawn_pane_options);
        let mut execution = self.split_window_with_options(
            context,
            &options,
            kind,
            command,
            split_size(&options),
            true,
            spawn,
            hooks,
        )?;
        let created = execution.effects.iter().find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        });
        if let Some(pane) = created {
            for (name, value) in spawn_options {
                if let Some(mut option_context) = ExecutionContext::for_pane(&self.state, pane) {
                    let _ = self.execute(
                        &mut option_context,
                        &CommandInvocation::new(
                            "set-option",
                            ["-p", "-t", &pane.to_string(), &name, &value],
                        ),
                    );
                }
            }
        }
        if options.has("-M")
            && floating
            && let Some(pane) = created
            && let Some(mouse) = drag_mouse
            && self.new_pane_mouse_resize(pane, &mouse)?
        {
            execution
                .effects
                .push(armed_mouse_drag(MouseDragKind::NewPane, pane, &mouse));
        }
        if options.has("-I")
            && let Some(bytes) = stdin
            && let Some(pane) = created
        {
            execution.effects.push(MuxEffect::PaneStreamInput {
                pane,
                bytes: bytes.clone(),
            });
        }
        queue_agent_brief(&mut execution, brief);
        Ok(execution)
    }

    fn split_floating_target(
        &self,
        context: &ExecutionContext,
        options: &Options,
    ) -> Result<Option<Result<(PaneId, CellGeometry, FloatSpawn), String>>, ServerError> {
        let target = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let window = self
            .state
            .window_for_pane(target)
            .expect("resolved pane has a window");
        let window_state = &self.state.windows[&window];
        if !window_state.is_floating(target) {
            return Ok(None);
        }
        let lines = match options.value("-B") {
            Some(lines) => PaneBorderLines::parse(lines),
            None => self.pane_border_lines(window),
        };
        let old = window_state
            .layout
            .pane_geometry(target)
            .expect("floating pane has a cell");
        let Some((old, new)) = split_floating_cell(
            old,
            window_state.layout.extent(),
            lines != PaneBorderLines::None,
            options.has("-h"),
            options.has("-b"),
            options.has("-f"),
        ) else {
            return Ok(Some(Err("no space for a new pane".to_owned())));
        };
        Ok(Some(Ok((
            target,
            old,
            FloatSpawn {
                geometry: new,
                detached: options.has("-d"),
                before: options.has("-b"),
                full_size: options.has("-f"),
                ..FloatSpawn::default()
            },
        ))))
    }

    fn expand_float_values(
        &self,
        options: &mut Options,
        target: &ExecutionContext,
        context: &ExecutionContext,
        hooks: &mut impl StatusHooks,
    ) {
        for (name, value) in &mut options.values {
            if !matches!(name.as_str(), "-x" | "-y" | "-X" | "-Y") || !value.contains('#') {
                continue;
            }
            let (body, percent) = value
                .strip_suffix('%')
                .map_or((value.as_str(), ""), |body| (body, "%"));
            let expanded = self.expand_target_format(
                body,
                target,
                context.session,
                context.target_format_client(),
                hooks,
            );
            *value = format!("{expanded}{percent}");
        }
    }

    fn float_geometry(
        &mut self,
        window: WindowId,
        options: &Options,
        border: bool,
        defaults: Option<CellGeometry>,
        place: Option<&str>,
    ) -> Result<CellGeometry, String> {
        let window_state = &self.state.windows[&window];
        let (wsx, wsy) = window_state.layout.extent();
        let (wsx, wsy) = (i64::from(wsx), i64::from(wsy));
        let mut sx = defaults.map_or(wsx / 2, |cell| i64::from(cell.sx));
        let mut sy = defaults.map_or(wsy / 4, |cell| i64::from(cell.sy));
        let mut ox = defaults.map(|cell| i64::from(cell.xoff));
        let mut oy = defaults.map(|cell| i64::from(cell.yoff));
        let position = |cause: String| format!("position {cause}");
        let pane_maximum = i64::from(PANE_MAXIMUM);
        if let Some(value) = options.value("-x") {
            sx = percentage_value(value, 0, pane_maximum, wsx).map_err(position)?;
            if border {
                sx -= 2;
            }
        }
        if let Some(value) = options.value("-y") {
            sy = percentage_value(value, 0, pane_maximum, wsy).map_err(position)?;
            if border {
                sy -= 2;
            }
        }
        if let Some(value) = options.value("-X") {
            ox = Some(percentage_value(value, -sx, wsx, wsx).map_err(position)?);
        }
        if let Some(value) = options.value("-Y") {
            oy = Some(percentage_value(value, -sy, wsy, wsy).map_err(position)?);
        }
        let cells = |value: i64| i32::try_from(value).unwrap_or(i32::MAX);
        if let Some((x, y)) = place.and_then(|place| {
            float_place(
                place,
                (cells(wsx), cells(wsy)),
                (cells(sx), cells(sy)),
                i32::from(border),
            )
        }) {
            ox = ox.or(Some(i64::from(x)));
            oy = oy.or(Some(i64::from(y)));
        }
        let (mut last_x, mut last_y) = if window_state.has_floating_panes() {
            window_state.floats.last_new_pane
        } else {
            (0, 0)
        };
        let pad = i64::from(border);
        let xoff = match ox {
            None => {
                let mut xoff = 4;
                if last_x != 0 {
                    xoff = i64::from(last_x) + 4;
                    if xoff + sx + pad > wsx {
                        xoff = 4;
                    }
                }
                last_x = i32::try_from(xoff).unwrap_or(i32::MAX);
                xoff
            }
            Some(xoff) if options.value("-X").is_some() && border => xoff + 1,
            Some(xoff) => xoff,
        };
        let yoff = match oy {
            None => {
                let mut yoff = 2;
                if last_y != 0 {
                    yoff = i64::from(last_y) + 2;
                    if yoff + sy + pad > wsy {
                        yoff = 2;
                    }
                }
                last_y = i32::try_from(yoff).unwrap_or(i32::MAX);
                yoff
            }
            Some(yoff) if options.value("-Y").is_some() && border => yoff + 1,
            Some(yoff) => yoff,
        };
        self.state.set_last_new_pane(window, (last_x, last_y));
        let minimum = i64::from(PANE_MINIMUM);
        if !(minimum..=pane_maximum).contains(&sx) {
            return Err("invalid width".to_owned());
        }
        if !(minimum..=pane_maximum).contains(&sy) {
            return Err("invalid height".to_owned());
        }
        Ok(CellGeometry {
            sx: u16::try_from(sx).unwrap_or(u16::MAX),
            sy: u16::try_from(sy).unwrap_or(u16::MAX),
            xoff: i32::try_from(xoff).unwrap_or(i32::MAX),
            yoff: i32::try_from(yoff).unwrap_or(i32::MAX),
        })
    }

    pub(super) fn spawn_pane_kind(
        &self,
        command: &str,
        context: &ExecutionContext,
        options: &Options,
        positional: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<(PaneKind, Option<String>), ServerError> {
        let kind = validate_spawn_kind(command, options, positional)?;
        if kind == "agent" {
            let origin = if command == "new-window" {
                context.pane
            } else {
                Some(self.resolve_pane(options.value("-t"), context.window, context.pane)?)
            };
            let (_, cwd) = spawn_cwd_source(
                self,
                options,
                origin,
                &PaneKind::Terminal,
                context.format_client(),
                hooks,
            );
            let (agent, brief) = agent_spawn(command, options, positional, cwd.as_deref())?;
            return Ok((PaneKind::Agent(agent), brief));
        }
        let kind = match kind {
            "terminal" => Ok(PaneKind::Terminal),
            "browser" => browser_from_args(options, positional).map(PaneKind::Browser),
            SIMULATOR_PANE_KIND => {
                simulator_descriptor(positional.first().map_or("", RawText::as_str))
                    .map(PaneKind::Simulator)
            }
            "picker" => {
                let target =
                    self.resolve_pane(options.value("-t"), context.window, context.pane)?;
                let (inherit_cwd_from, _) = spawn_cwd_source(
                    self,
                    options,
                    Some(target),
                    &PaneKind::Terminal,
                    context.format_client(),
                    hooks,
                );
                Ok(PaneKind::Picker { inherit_cwd_from })
            }
            _ => unreachable!(),
        };
        kind.map(|kind| (kind, None))
    }

    pub(super) fn select_pane_kind(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("select-pane-kind", args)?;
        let Some((selection, command)) = positional.split_first() else {
            return Err(ServerError::CommandParse(
                "select-pane-kind requires exactly one of: terminal, browser, editor, agent, simulator"
                    .to_owned(),
            ));
        };
        if !matches!(selection.as_str(), "terminal" | "agent")
            && (!command.is_empty() || options.value("-c").is_some())
        {
            return Err(ServerError::CommandParse(
                "select-pane-kind: -c and a command need the terminal kind".to_owned(),
            ));
        }
        if selection.as_str() != "agent"
            && (["--resume", "--model", "--effort", "--at", "--account"]
                .iter()
                .any(|option| options.value(option).is_some())
                || options.has("--fork"))
        {
            return Err(ServerError::CommandParse(
                "select-pane-kind: --resume, --fork, --at, --model, --effort and --account need the agent kind"
                    .to_owned(),
            ));
        }
        if options.value("--at").is_some() && !options.has("--fork") {
            return Err(ServerError::CommandParse(
                "select-pane-kind: --at needs --fork".to_owned(),
            ));
        }
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let fork = options.has("--fork").then(|| MuxEffect::AgentForked {
            pane,
            at: options.value("--at").map(str::to_owned),
        });
        let words = || {
            (!command.is_empty()).then(|| {
                command
                    .iter()
                    .map(|word| word.as_str().to_owned())
                    .collect::<Vec<_>>()
            })
        };
        match (
            self.state.pane(pane).map(|pane| &pane.kind),
            selection.as_str(),
        ) {
            (Some(PaneKind::Terminal), "agent") => {
                let agent =
                    agent_selection("select-pane-kind", &options, command, options.value("-c"))?;
                self.state
                    .convert_pane(pane, PaneKind::Agent(agent.clone()))?;
                self.refresh_automatic_window_name_for_pane(pane, hooks);
                return Ok(agent_opened(
                    MuxEffect::PaneMaterialized {
                        pane,
                        kind: PaneKindSnapshot::Agent(agent),
                        inherit_cwd_from: Some(pane),
                        cwd: options.value("-c").map(expand_home),
                        command: None,
                    },
                    fork,
                ));
            }
            (Some(PaneKind::Agent(_)), "terminal") => {
                self.state.convert_pane(pane, PaneKind::Terminal)?;
                return Ok(Execution::effect(MuxEffect::PaneRespawned {
                    pane,
                    cwd: options.value("-c").map(expand_home),
                    command: words(),
                    environment: Vec::new(),
                    empty: false,
                }));
            }
            _ => {}
        }
        let mut command = command;
        let kind = match selection.as_str() {
            "terminal" => PaneKind::Terminal,
            "agent" => {
                let agent =
                    agent_selection("select-pane-kind", &options, command, options.value("-c"))?;
                command = &[];
                PaneKind::Agent(agent)
            }
            "browser" => PaneKind::Browser(BrowserDescriptor::single(
                "about:blank".to_owned(),
                DEFAULT_BROWSER_PROFILE.to_owned(),
            )),
            SIMULATOR_PANE_KIND => PaneKind::Simulator(SimulatorDescriptor::default()),
            "editor" => {
                if !self.experimental_editor_pane {
                    return Err(ServerError::InvalidCommand(
                        "editor panes are experimental; enable experimental-editor-pane in \
                         Settings → Advanced first"
                            .to_owned(),
                    ));
                }
                let cwd = std::env::current_dir()
                    .map_err(|error| {
                        ServerError::Internal(format!(
                            "could not resolve editor working directory: {error}"
                        ))
                    })?
                    .to_string_lossy()
                    .into_owned();
                PaneKind::Editor(EditorDescriptor { path: None, cwd })
            }
            _ => {
                return Err(ServerError::InvalidCommand(format!(
                    "unknown pane kind {selection:?}; expected terminal, browser, editor, agent, or simulator"
                )));
            }
        };
        let snapshot_kind = pane_kind_snapshot(&kind);
        let inherit_cwd_from = self.state.materialize_pane(pane, kind)?;
        if matches!(snapshot_kind, PaneKindSnapshot::Agent(_)) {
            self.refresh_automatic_window_name_for_pane(pane, hooks);
        }
        Ok(agent_opened(
            MuxEffect::PaneMaterialized {
                pane,
                kind: snapshot_kind,
                inherit_cwd_from,
                cwd: options.value("-c").map(expand_home),
                command: (!command.is_empty()).then(|| {
                    command
                        .iter()
                        .map(|word| word.as_str().to_owned())
                        .collect()
                }),
            },
            fork,
        ))
    }

    pub(super) fn break_pane(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (mut options, positional) = parse_command_options("break-pane", args)?;
        if !positional.is_empty() {
            return Err(ServerError::CommandParse(
                "break-pane does not accept positional arguments".to_owned(),
            ));
        }
        let source = self.resolve_pane(options.value("-s"), context.window, context.pane)?;
        let source_window = self
            .state
            .window_for_pane(source)
            .expect("resolved source pane has a window");
        if self.state.windows[&source_window].modal_pane() == Some(source) {
            return Err(ServerError::InvalidCommand("pane is modal".to_owned()));
        }
        if options.has("-W") {
            let target_context = context.clone();
            self.expand_float_values(&mut options, &target_context, context, hooks);
            return self.break_pane_float(context, &options, source_window, source);
        }
        let source_session = self.state.windows[&source_window].session;
        let destination = match options.value("-t") {
            Some(target) => window_destination(&self.state, Some(target), context)?,
            None => WindowDestination {
                session: source_session,
                index: None,
            },
        };
        let destination_session = destination.session;
        let name = options
            .value("-n")
            .map(|name| tmux_clean_name(name, "window"))
            .transpose()?;
        let has_name = name.is_some();
        let destination_index = if options.has("-a") || options.has("-b") {
            Some(shift_window_insertion_index(
                &mut self.state,
                &destination,
                options.has("-b"),
            )?)
        } else {
            destination.index
        };
        let detached = options.has("-d");
        let original_context = context.clone();
        let base_index = self.base_index_for_session(destination_session);
        let destroyed_source = self.destroyed_session_marker(source_session)?;
        let default_name = self.default_window_name(source);
        let window = self.state.break_pane_with_base_index(
            source,
            destination_session,
            destination_index,
            name,
            default_name,
            detached,
            base_index,
        )?;
        if has_name {
            self.state
                .set_window_automatic_rename(window, Some(false))?;
        }
        self.record_destroyed_session(destroyed_source);
        if window != source_window {
            self.refresh_automatic_window_name_for_pane(source, hooks);
        }
        if detached {
            if original_context.window == Some(source_window)
                && original_context.pane == Some(source)
            {
                let context_window = if self
                    .state
                    .windows
                    .get(&source_window)
                    .is_some_and(|window| window.session == source_session)
                {
                    source_window
                } else if self.state.sessions.contains_key(&source_session) {
                    session_active_window(&self.state, source_session)?
                } else {
                    window
                };
                let pane = self.state.windows[&context_window].active_pane;
                let target = ExecutionContext::for_pane(&self.state, pane)
                    .expect("break-pane retains a valid command context");
                context.retarget(&target);
            }
        } else {
            let target = ExecutionContext::for_pane(&self.state, source)
                .expect("broken pane belongs to its new window");
            context.retarget(&target);
        }
        let mut execution = Execution::default();
        if source_session != destination_session {
            execution.effects.push(MuxEffect::PaneRelocated {
                pane: source,
                from: source_session,
                to: destination_session,
            });
        } else if source_window != window {
            execution.effects.push(MuxEffect::TerminalKnobsChanged {
                window: None,
                pane: Some(source),
            });
        }
        debug_assert_eq!(self.state.window_for_pane(source), Some(window));
        execution.output = self.pane_creation_output(
            &options,
            destination_session,
            window,
            source,
            original_context.session,
            original_context.target_format_client(),
            hooks,
        );
        Ok(execution)
    }

    fn break_pane_float(
        &mut self,
        context: &mut ExecutionContext,
        options: &Options,
        window: WindowId,
        pane: PaneId,
    ) -> Result<Execution, ServerError> {
        let window_state = &self.state.windows[&window];
        if window_state.is_floating(pane) {
            return Err(ServerError::InvalidCommand(
                "pane is already floating".to_owned(),
            ));
        }
        if window_state.zoomed_pane.is_some() {
            return Err(ServerError::InvalidCommand(
                "can't float a pane while window is zoomed".to_owned(),
            ));
        }
        let border = self.pane_border_lines(window) != PaneBorderLines::None;
        let defaults = window_state.layout.saved_float(pane);
        let geometry = self
            .float_geometry(window, options, border, defaults, None)
            .map_err(|cause| {
                ServerError::InvalidCommand(format!("failed to float pane: {cause}"))
            })?;
        let detached = options.has("-d");
        self.state.float_existing(pane, geometry, detached)?;
        if !detached {
            let target = ExecutionContext::for_pane(&self.state, pane)
                .expect("floated pane keeps its window");
            context.retarget(&target);
        }
        Ok(Execution::default())
    }

    fn move_pane_float(
        &mut self,
        options: &Options,
        target: PaneId,
    ) -> Result<Execution, ServerError> {
        let window = self
            .state
            .window_for_pane(target)
            .expect("resolved pane has a window");
        let window_state = &self.state.windows[&window];
        if !window_state.shows_floating(target) {
            return Err(ServerError::InvalidCommand(
                "pane is not floating".to_owned(),
            ));
        }
        let mut cell = window_state
            .layout
            .pane_geometry(target)
            .expect("floating pane has a cell");
        let (wsx, wsy) = window_state.layout.extent();
        let (wx, wy) = (i32::from(wsx), i32::from(wsy));
        let (px, py) = (i32::from(cell.sx), i32::from(cell.sy));
        let border = i32::from(self.pane_lines(target) != PaneBorderLines::None);
        if let Some(position) = options.value("-P") {
            let z = match position {
                "front" => Some(FloatZ::Front),
                "back" => Some(FloatZ::Back),
                "forward" => Some(FloatZ::Forward),
                "backward" => Some(FloatZ::Backward),
                "forward-loop" => Some(FloatZ::ForwardLoop),
                "backward-loop" => Some(FloatZ::BackwardLoop),
                _ => None,
            };
            if let Some(z) = z {
                self.state.move_float_z(target, z)?;
                return Ok(Execution::default());
            }
            let Some((xoff, yoff)) = float_place(position, (wx, wy), (px, py), border) else {
                return Err(ServerError::InvalidCommand(format!(
                    "unknown position: {position}"
                )));
            };
            cell.xoff = xoff;
            cell.yoff = yoff;
            self.state.set_float_geometry(target, cell)?;
            return Ok(Execution::default());
        }
        if let Some(value) = options.value("-z") {
            let z = parse_strtonum(value, 0, i64::from(u32::MAX), "z-index")?;
            self.state
                .move_float_z(target, FloatZ::Index(u32::try_from(z).unwrap_or(u32::MAX)))?;
            return Ok(Execution::default());
        }
        let position = |cause: String| ServerError::InvalidCommand(format!("position {cause}"));
        if let Some(value) = options.value("-X") {
            let x = percentage_value(value, -i64::from(wsx), i64::from(wsx), i64::from(wsx))
                .map_err(position)?;
            cell.xoff = i32::try_from(x).unwrap_or_default() + border;
        }
        if let Some(value) = options.value("-Y") {
            let y = percentage_value(value, -i64::from(wsy), i64::from(wsy), i64::from(wsy))
                .map_err(position)?;
            cell.yoff = i32::try_from(y).unwrap_or_default() + border;
        }
        for flag in ["-U", "-D", "-L", "-R"] {
            if !options.has(flag) && options.value(flag).is_none() {
                continue;
            }
            let adjust = match options.value(flag) {
                Some(value) => {
                    parse_strtonum(value, i64::from(i32::MIN), i64::from(i32::MAX), "offset")?
                }
                None => 1,
            };
            let adjust = i32::try_from(adjust).unwrap_or_default();
            match flag {
                "-U" => cell.yoff -= adjust,
                "-D" => cell.yoff += adjust,
                "-L" => cell.xoff -= adjust,
                _ => cell.xoff += adjust,
            }
        }
        self.state.set_float_geometry(target, cell)?;
        Ok(Execution::default())
    }

    #[must_use]
    pub fn floating_pane_border_status(&self, pane: PaneId) -> PaneBorderStatus {
        match self.scalar_option_effective(TmuxOptionTarget::Pane(pane), "pane-border-status") {
            Some("top" | "top-floating") => PaneBorderStatus::Top,
            Some("bottom" | "bottom-floating") => PaneBorderStatus::Bottom,
            _ => PaneBorderStatus::Off,
        }
    }

    #[must_use]
    pub fn pane_lines(&self, pane: PaneId) -> PaneBorderLines {
        let Some(window) = self.state.window_for_pane(pane) else {
            return PaneBorderLines::Single;
        };
        if self.state.windows[&window].is_floating(pane)
            && let Some(lines) = self
                .pane_options
                .get(&pane)
                .and_then(|options| options.get(&PaneOption::PaneBorderLines))
        {
            return PaneBorderLines::parse(lines);
        }
        self.pane_border_lines(window)
    }

    pub(super) fn join_pane(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
        command: &str,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options(command, args)?;
        if !positional.is_empty() {
            return Err(ServerError::CommandParse(format!(
                "{command} does not accept positional arguments"
            )));
        }
        if command == "move-pane" {
            if options.has("-M") {
                let Some(mouse) = context.invoking_mouse().cloned() else {
                    return Ok(Execution::default());
                };
                let Some(pane) = mouse.pane else {
                    return Ok(Execution::default());
                };
                if self.mouse_float_geometry(pane).is_none() {
                    return Ok(Execution::default());
                }
                self.state.select_pane_with_zoom(pane, true)?;
                self.move_float_from_mouse(pane, &mouse)?;
                return Ok(Execution {
                    effects: vec![armed_mouse_drag(MouseDragKind::MovePane, pane, &mouse)],
                    ..Execution::default()
                });
            }
            if ["-P", "-z", "-X", "-Y", "-U", "-D", "-L", "-R"]
                .iter()
                .any(|flag| options.has(flag) || options.value(flag).is_some())
            {
                let target =
                    self.resolve_pane(options.value("-t"), context.window, context.pane)?;
                let mut options = options;
                if let Some(target_context) = ExecutionContext::for_pane(&self.state, target) {
                    self.expand_float_values(&mut options, &target_context, context, hooks);
                }
                return self.move_pane_float(&options, target);
            }
        }
        let source = self.resolve_pane(options.value("-s"), context.window, context.pane)?;
        let target = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let source_window = self
            .state
            .window_for_pane(source)
            .expect("resolved source pane has a window");
        let target_window = self
            .state
            .window_for_pane(target)
            .expect("resolved target pane has a window");
        if self.state.windows[&source_window].modal_pane() == Some(source)
            || self.state.windows[&target_window].modal_pane() == Some(target)
        {
            return Err(ServerError::InvalidCommand("pane is modal".to_owned()));
        }
        for window in [target_window, source_window] {
            if self
                .state
                .windows
                .get_mut(&mut self.state.journal, &window)
                .expect("resolved pane window exists")
                .unzoom()
            {
                self.state.bump_generation();
            }
        }
        if source == target {
            if self.state.windows[&source_window].is_floating(source) {
                let detached = options.has("-d");
                self.state.tile_floating(source, detached).map_err(|_| {
                    ServerError::InvalidCommand("no space for a new pane".to_owned())
                })?;
                if !detached {
                    let target = ExecutionContext::for_pane(&self.state, source)
                        .expect("tiled pane keeps its window");
                    context.retarget(&target);
                }
                return Ok(Execution::default());
            }
            return Err(ServerError::InvalidCommand(
                "source and target panes must be different".to_owned(),
            ));
        }
        if self.state.windows[&target_window].is_floating(target) {
            return Err(ServerError::InvalidCommand(
                "size or position can't split a floating pane".to_owned(),
            ));
        }
        let source_session = self.state.windows[&source_window].session;
        let target_session = self.state.windows[&target_window].session;
        let original_context = context.clone();
        let detached = options.has("-d");
        let axis = if options.has("-h") {
            Axis::Horizontal
        } else {
            Axis::Vertical
        };
        let size = self.join_pane_size(
            context,
            target,
            axis,
            options.has("-f"),
            split_size(&options),
            hooks,
        )?;
        if source_window != target_window
            && self.state.windows[&source_window].panes.len() == 1
            && self.renumber_windows_for_session(source_session)
        {
            self.state.validate_renumber_capacity(
                source_session,
                self.base_index_for_session(source_session),
                1,
            )?;
        }
        self.state.join_pane(
            source,
            target,
            axis,
            size,
            options.has("-b"),
            options.has("-f"),
            detached,
        )?;
        if !self.state.windows.contains_key(&source_window) {
            self.renumber_session_if_enabled(source_session)?;
        }

        if detached {
            if original_context.window == Some(source_window)
                && original_context.pane == Some(source)
            {
                let window = if self.state.windows.contains_key(&source_window) {
                    source_window
                } else {
                    session_active_window(&self.state, source_session)?
                };
                let pane = window_active_pane(&self.state, window)?;
                let target = ExecutionContext::for_pane(&self.state, pane)
                    .expect("detached join retains a valid source context");
                context.retarget(&target);
            }
        } else {
            let target = ExecutionContext::for_pane(&self.state, source)
                .expect("joined pane belongs to the target window");
            context.retarget(&target);
        }
        let mut execution = Execution::default();
        if source_session != target_session {
            execution.effects.push(MuxEffect::PaneRelocated {
                pane: source,
                from: source_session,
                to: target_session,
            });
        } else if source_window != target_window {
            execution.effects.push(MuxEffect::TerminalKnobsChanged {
                window: None,
                pane: Some(source),
            });
        } else {
            execution
                .effects
                .push(MuxEffect::PaneMovedInWindow { pane: source });
        }
        Ok(execution)
    }

    fn join_pane_size(
        &self,
        context: &ExecutionContext,
        target: PaneId,
        axis: Axis,
        full_size: bool,
        size: Option<SplitSize<'_>>,
        hooks: &mut impl StatusHooks,
    ) -> Result<LayoutSplitSize, ServerError> {
        let Some(size) = size else {
            return Ok(LayoutSplitSize::Default);
        };
        let target_context = ExecutionContext::for_pane(&self.state, target)
            .expect("resolved target pane has a context");
        let format_context = FormatContext {
            session: target_context.session,
            window: target_context.window,
            pane: target_context.pane,
            active_session: context.session,
            format_client: context.target_format_client(),
            format_type: FormatType::Pane,
        };
        let target_window = self
            .state
            .window_for_pane(target)
            .expect("resolved target pane has a window");
        let basis = if full_size {
            self.window_extent(target_window, axis)
        } else {
            self.pane_geometry(target)
                .map(|(columns, rows)| match axis {
                    Axis::Horizontal => columns,
                    Axis::Vertical => rows,
                })
        }
        .expect("resolved target pane has geometry");
        let label = "size or position invalid tiled geometry";
        let cells = match size {
            SplitSize::Percentage(value) => {
                let expanded = expand_format_with_hooks(value, self, format_context, hooks);
                let percentage = parse_strtonum(&expanded, 0, 100, label)?;
                u64::from(basis) * u64::try_from(percentage).expect("percentage is nonnegative")
                    / 100
            }
            SplitSize::Cells(value) => {
                if let Some(percentage) = value.strip_suffix('%') {
                    let expanded =
                        expand_format_with_hooks(percentage, self, format_context, hooks);
                    let percentage = parse_strtonum(&expanded, 0, 1000, label)?;
                    u64::from(basis) * u64::try_from(percentage).expect("percentage is nonnegative")
                        / 100
                } else {
                    let expanded = expand_format_with_hooks(value, self, format_context, hooks);
                    u64::try_from(parse_strtonum(&expanded, 0, i64::from(i32::MAX), label)?)
                        .expect("pane size is nonnegative")
                }
            }
        };
        Ok(LayoutSplitSize::Cells(
            u16::try_from(cells).unwrap_or(u16::MAX),
        ))
    }

    pub(super) fn set_browser_url(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("set-browser-url", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let url = positional
            .first()
            .ok_or_else(|| ServerError::CommandParse("set-browser-url needs a URL".to_owned()))?;
        self.state.update_browser_url(pane, url.to_string())?;
        Ok(Execution::default())
    }

    pub(super) fn set_browser_tabs(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("set-browser-tabs", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let active_tab = options
            .value("-a")
            .map_or(Ok(0), str::parse)
            .map_err(|_| ServerError::InvalidCommand("-a needs a numeric tab index".to_owned()))?;
        if positional.is_empty() {
            return Err(ServerError::CommandParse(
                "set-browser-tabs needs at least one URL".to_owned(),
            ));
        }
        self.state.update_browser_tabs(
            pane,
            positional.iter().map(ToString::to_string).collect(),
            active_tab,
        )?;
        Ok(Execution::default())
    }

    pub(super) fn set_browser_profile(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("set-browser-profile", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        if positional.len() != 1 {
            return Err(ServerError::CommandParse(
                "set-browser-profile needs exactly one profile name".to_owned(),
            ));
        }
        self.state.update_browser_profile(pane, &positional[0])?;
        Ok(Execution::default())
    }

    pub(super) fn set_simulator_device(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("set-simulator-device", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let [device] = positional.as_slice() else {
            return Err(ServerError::CommandParse(
                "set-simulator-device needs exactly one device".to_owned(),
            ));
        };
        self.state.update_simulator_device(pane, device)?;
        Ok(Execution::default())
    }

    pub(super) fn set_editor_path(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("set-editor-path", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let path = match positional.as_slice() {
            [] => None,
            [path] => Some(path.clone()),
            _ => {
                return Err(ServerError::CommandParse(
                    "set-editor-path accepts at most one absolute path".to_owned(),
                ));
            }
        };
        self.state
            .update_editor_path(pane, path.map(|path| path.to_string()))?;
        Ok(Execution::default())
    }

    pub(super) fn view_pane(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("view-pane", args)?;
        let origin = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let text = match positional.as_slice() {
            [] => String::new(),
            [text] => text.to_string(),
            _ => {
                return Err(ServerError::CommandParse(
                    "view-pane takes its content as one argument".to_owned(),
                ));
            }
        };
        let shown = view_content(&options, text)?;
        let descriptor = ViewDescriptor {
            origin: Some(origin),
            title: options
                .value("-T")
                .map_or_else(|| shown.kind_name().to_owned(), ToOwned::to_owned),
            content: shown,
        };
        descriptor
            .validate()
            .map_err(|error| ServerError::InvalidCommand(error.to_string()))?;
        let window = self
            .state
            .window_for_pane(origin)
            .ok_or_else(|| ServerError::MissingTarget(origin.to_string()))?;
        let open = self.state.windows[&window]
            .panes
            .iter()
            .find_map(|(pane, entry)| {
                matches!(&entry.kind, PaneKind::View(view) if view.origin == Some(origin))
                    .then_some(*pane)
            });
        if let Some(pane) = open {
            self.state.update_view(pane, descriptor)?;
            let select = CommandInvocation::new("select-pane", ["-t", &pane.to_string()]);
            return self.select_pane(context, &select.args, hooks);
        }
        let area = self.state.windows[&window]
            .layout
            .pane_geometry(origin)
            .ok_or_else(|| ServerError::MissingTarget(origin.to_string()))?;
        let (wide, tall) = self.state.windows[&window].layout.extent();
        let (wide, tall) = (i64::from(wide), i64::from(tall));
        let sx = i64::from(area.sx);
        let width = (sx * 11 / 20).max(sx.min(48));
        let height = match options.value("-y") {
            Some(rows) => parse_strtonum(rows, 1, i64::from(PANE_MAXIMUM), "rows")?.min(tall),
            None => tall,
        };
        let beside = i64::from(area.xoff) + sx - width;
        let corner = [0, (wide - width) / 2, wide - width]
            .into_iter()
            .min_by_key(|column| (column - beside).abs())
            .unwrap_or(0);
        let place = CommandInvocation::new(
            "new-pane",
            [
                "-t".to_owned(),
                origin.to_string(),
                "-x".to_owned(),
                width.to_string(),
                "-y".to_owned(),
                height.to_string(),
                "-X".to_owned(),
                corner.to_string(),
                "-Y".to_owned(),
                (tall - height).to_string(),
            ],
        );
        let (place, _) = parse_command_options("new-pane", &place.args)?;
        let border = self.pane_border_lines(window) != PaneBorderLines::None;
        let geometry = self
            .float_geometry(window, &place, border, None, None)
            .map_err(ServerError::InvalidCommand)?;
        self.split_window_with_options(
            context,
            &place,
            PaneKind::View(descriptor),
            None,
            None,
            true,
            Some(FloatSpawn {
                geometry,
                ..FloatSpawn::default()
            }),
            hooks,
        )
    }

    fn split_window_with_options(
        &mut self,
        context: &mut ExecutionContext,
        options: &Options,
        kind: PaneKind,
        command: Option<Vec<String>>,
        size: Option<SplitSize<'_>>,
        apply_tmux_zoom: bool,
        floating: Option<FloatSpawn>,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let active_session = context.session;
        let format_client = context.format_client();
        let target_format_client = context.target_format_client();
        let target = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let empty = pane_spawn_empty(options, command.as_deref())?;
        let environment = spawn_environment(options);
        check_spawn_border_lines(options)?;
        let axis = if options.has("-h") {
            Axis::Horizontal
        } else {
            Axis::Vertical
        };
        let placement = if floating.is_some() {
            None
        } else if apply_tmux_zoom {
            let size = match size {
                None => LayoutSplitSize::Default,
                Some(SplitSize::Percentage(value)) => LayoutSplitSize::Percent(
                    u8::try_from(parse_strtonum(value, 0, 100, "invalid tiled geometry")?)
                        .expect("bounded percentage"),
                ),
                Some(SplitSize::Cells(value)) => {
                    if let Some(value) = value.strip_suffix('%') {
                        LayoutSplitSize::Percent(
                            u8::try_from(parse_strtonum(value, 0, 100, "invalid tiled geometry")?)
                                .expect("bounded percentage"),
                        )
                    } else {
                        LayoutSplitSize::Cells(
                            u16::try_from(parse_strtonum(
                                value,
                                0,
                                i64::from(i32::MAX),
                                "invalid tiled geometry",
                            )?)
                            .unwrap_or(u16::MAX),
                        )
                    }
                }
            };
            Some(SplitPlacement {
                size,
                before: options.has("-b"),
                full_size: options.has("-f"),
                detached: options.has("-d"),
            })
        } else {
            Some(self.split_placement(options, size)?)
        };
        let snapshot_kind = pane_kind_snapshot(&kind);
        let (inherit_cwd_from, cwd) =
            spawn_cwd_source(self, options, Some(target), &kind, format_client, hooks);
        let over_zoom = floating.as_ref().is_some_and(|spawn| spawn.over_zoom);
        let target_window = self
            .state
            .window_for_pane(target)
            .expect("resolved pane has a window");
        let mut float_zoom = None;
        if apply_tmux_zoom && floating.is_some() {
            let window_state = &self.state.windows[&target_window];
            let active_over_zoom = window_state.active_is_over_zoom();
            let restore = (active_over_zoom && window_state.pane_is_visible(target))
                || (window_state.zoomed_pane.is_some() && over_zoom);
            let pushed = if over_zoom || active_over_zoom {
                self.state.push_zoom(target_window, false, true)
            } else {
                self.state.push_zoom(target_window, true, options.has("-Z"))
            };
            float_zoom = Some((restore, pushed));
        } else if apply_tmux_zoom && self.state.windows[&target_window].zoomed_pane.is_some() {
            self.state.toggle_zoom(target)?;
        }
        let is_floating = floating.is_some();
        let detached = placement
            .as_ref()
            .map_or(options.has("-d"), |placement| placement.detached);
        let pane = match (floating, placement) {
            (Some(spawn), _) => self.state.float_pane_with(target, kind, &spawn)?,
            (None, Some(placement)) => self.state.split_pane_with(target, axis, kind, placement)?,
            (None, None) => unreachable!("a tiled split has a placement"),
        };
        if let Some((true, pushed)) = float_zoom {
            self.state.pop_zoom(target_window, pushed);
        }
        if empty {
            self.state.mark_pane_empty(pane)?;
        }
        if !detached {
            let target =
                ExecutionContext::for_pane(&self.state, pane).expect("new pane has a context");
            context.retarget(&target);
        }
        let window = self
            .state
            .window_for_pane(pane)
            .expect("new pane belongs to a window");
        if apply_tmux_zoom && !is_floating {
            let active_pane = self.state.windows[&window].active_pane;
            let state = self
                .state
                .windows
                .get_mut(&mut self.state.journal, &window)
                .expect("split window");
            state.zoomed_pane = options.has("-Z").then_some(active_pane);
            if state.zoomed_pane.is_some_and(|zoomed| zoomed != pane) {
                let extent = state.layout.extent();
                if let Some(state) = state.panes.get_mut(&pane) {
                    state.screen_extent = Some(extent);
                }
            }
        }
        if options.value("-s").is_some()
            || options.value("-S").is_some()
            || options.value("-R").is_some()
        {
            let pane_options = self.scalar_table_mut_or_insert(TmuxOptionTarget::Pane(pane));
            if let Some(style) = options.value("-s") {
                pane_options.insert("window-style", style.to_owned());
                pane_options.insert("window-active-style", style.to_owned());
            }
            if let Some(style) = options.value("-S") {
                pane_options.insert("pane-active-border-style", style.to_owned());
            }
            if let Some(style) = options.value("-R") {
                pane_options.insert("pane-border-style", style.to_owned());
            }
        }
        if let Some(lines) = options.value("-B") {
            self.pane_options
                .entry(pane)
                .or_default()
                .insert(PaneOption::PaneBorderLines, lines.to_owned());
        }
        if options.has("-k") || options.value("-m").is_some() {
            self.pane_remain_on_exit.insert(pane, RemainOnExit::Key);
            if let Some(message) = options.value("-m") {
                self.scalar_table_mut_or_insert(TmuxOptionTarget::Pane(pane))
                    .insert("remain-on-exit-format", message.to_owned());
            }
        }
        if let Some(title) = options.value("-T") {
            let target =
                ExecutionContext::for_pane(&self.state, target).expect("resolved pane exists");
            let title = self.expand_pane_format(
                title,
                &target,
                active_session,
                target_format_client,
                hooks,
            );
            if let Some(title) = tmux_clean_title(&title) {
                self.state.pin_pane_title(pane, title)?;
            }
        }
        let session = self.state.windows[&window].session;
        let mut effects = vec![MuxEffect::PaneCreated {
            pane,
            kind: snapshot_kind,
            inherit_cwd_from,
            cwd,
            command,
            environment,
            empty,
        }];
        if options.has("-P") {
            effects.push(MuxEffect::PaneFormatOutput {
                pane,
                format: options
                    .value("-F")
                    .unwrap_or(DEFAULT_PANE_CREATION_FORMAT)
                    .to_owned(),
                active_session,
                format_client: target_format_client,
            });
        }
        if options.has("-W") {
            effects.push(MuxEffect::PaneWaitForExit { pane });
        }
        Ok(Execution {
            output: self.pane_creation_output(
                options,
                session,
                window,
                pane,
                active_session,
                target_format_client,
                hooks,
            ),
            effects,
        })
    }

    pub(super) fn pane_creation_output(
        &self,
        options: &Options,
        session: SessionId,
        window: WindowId,
        pane: PaneId,
        active_session: Option<SessionId>,
        format_client: FormatClient,
        hooks: &mut impl StatusHooks,
    ) -> RawText {
        if !options.has("-P") {
            return RawText::default();
        }
        let mut output = expand_format_with_hooks(
            options.value("-F").unwrap_or(DEFAULT_PANE_CREATION_FORMAT),
            self,
            FormatContext {
                session: Some(session),
                window: Some(window),
                pane: Some(pane),
                active_session,
                format_client,
                format_type: FormatType::Pane,
            },
            hooks,
        );
        output.push_bytes(b"\n");
        output
    }

    pub(super) fn split_placement(
        &self,
        options: &Options,
        size: Option<SplitSize<'_>>,
    ) -> Result<SplitPlacement, ServerError> {
        let full_size = options.has("-f");
        let size = match size {
            None => LayoutSplitSize::Default,
            Some(SplitSize::Percentage(value)) => {
                LayoutSplitSize::Percent(parse_pane_percentage(value)?)
            }
            Some(SplitSize::Cells(value)) => {
                if let Some(percentage) = value.strip_suffix('%') {
                    LayoutSplitSize::Percent(parse_pane_percentage(percentage)?)
                } else {
                    let cells = value.parse::<u16>().map_err(|_| {
                        ServerError::InvalidCommand(format!("invalid pane size: {value}"))
                    })?;
                    LayoutSplitSize::Cells(cells)
                }
            }
        };
        Ok(SplitPlacement {
            size,
            before: options.has("-b"),
            full_size,
            detached: options.has("-d"),
        })
    }
}

#[derive(Clone, Copy)]
pub(super) enum SplitSize<'a> {
    Percentage(&'a str),
    Cells(&'a str),
}

pub(super) fn split_size(options: &Options) -> Option<SplitSize<'_>> {
    options.value("-l").map_or_else(
        || options.value("-p").map(SplitSize::Percentage),
        |size| Some(SplitSize::Cells(size)),
    )
}

fn parse_pane_percentage(value: &str) -> Result<u8, ServerError> {
    let percentage = value
        .parse::<u8>()
        .map_err(|_| ServerError::InvalidCommand(format!("invalid pane percentage: {value}")))?;
    if percentage > 100 {
        return Err(ServerError::InvalidCommand(
            "pane percentage must be between 0 and 100".to_owned(),
        ));
    }
    Ok(percentage)
}

fn check_spawn_border_lines(options: &Options) -> Result<(), ServerError> {
    match options.value("-B") {
        Some(lines)
            if !crate::tmux_option_metadata::tmux_option_metadata("pane-border-lines")
                .choices
                .contains(&lines) =>
        {
            Err(ServerError::InvalidCommand(format!(
                "pane-border-lines unknown value: {lines}"
            )))
        }
        _ => Ok(()),
    }
}

pub(super) fn pane_spawn_empty(
    options: &Options,
    command: Option<&[String]>,
) -> Result<bool, ServerError> {
    let empty_command = matches!(command, Some([value]) if value.is_empty());
    let empty_flag = options.has("-E") || options.has("-I");
    if empty_flag && command.is_some() && !empty_command {
        return Err(ServerError::InvalidCommand(
            "command cannot be given for empty pane".to_owned(),
        ));
    }
    Ok(empty_flag || empty_command)
}

fn view_content(options: &Options, text: String) -> Result<ViewContent, ServerError> {
    let language = options.value("-l").map(ToOwned::to_owned);
    let item = || {
        options
            .value("-i")
            .map(ToOwned::to_owned)
            .ok_or_else(|| ServerError::CommandParse("view-pane -k needs -i item".to_owned()))
    };
    Ok(match options.value("-k").unwrap_or("code") {
        "diagram" => ViewContent::Diagram {
            language: language.unwrap_or_else(|| "mermaid".to_owned()),
            source: text,
        },
        "code" => ViewContent::Code { language, text },
        "markdown" => ViewContent::Markdown { text },
        "image" => ViewContent::Image {
            item: item()?,
            index: options
                .value("-n")
                .map_or(Ok(0), str::parse)
                .map_err(|_| ServerError::CommandParse("view-pane -n takes a number".to_owned()))?,
        },
        "subagent" => ViewContent::Subagent { item: item()? },
        other => {
            return Err(ServerError::CommandParse(format!(
                "view-pane cannot show {other}; use diagram, code, markdown, image or subagent"
            )));
        }
    })
}

pub(super) fn pane_kind_snapshot(kind: &PaneKind) -> PaneKindSnapshot {
    match kind {
        PaneKind::Picker { .. } => PaneKindSnapshot::Picker,
        PaneKind::Terminal => PaneKindSnapshot::Terminal,
        PaneKind::Browser(browser) => PaneKindSnapshot::Browser(browser.clone()),
        PaneKind::Editor(editor) => PaneKindSnapshot::Editor(editor.clone()),
        PaneKind::Agent(agent) => PaneKindSnapshot::Agent(agent.clone()),
        PaneKind::View(view) => PaneKindSnapshot::View(view.clone()),
        PaneKind::Simulator(_) => PaneKindSnapshot::Other {
            kind: SIMULATOR_PANE_KIND.to_owned(),
        },
    }
}

pub(super) fn queue_agent_brief(execution: &mut Execution, brief: Option<String>) {
    let pane = execution.effects.iter().find_map(|effect| match effect {
        MuxEffect::PaneCreated { pane, .. } => Some(*pane),
        _ => None,
    });
    if let (Some(pane), Some(text)) = (pane, brief) {
        execution.effects.push(MuxEffect::AgentBrief { pane, text });
    }
}

fn agent_opened(opened: MuxEffect, fork: Option<MuxEffect>) -> Execution {
    let mut effects = vec![opened];
    effects.extend(fork);
    Execution {
        output: RawText::default(),
        effects,
    }
}

fn agent_selection(
    command: &str,
    options: &Options,
    args: &[RawText],
    cwd: Option<&str>,
) -> Result<AgentDescriptor, ServerError> {
    let usage = || {
        ServerError::CommandParse(
            "usage: select-pane-kind [-t pane] [-c start-directory] agent claude|codex|agy|grok|cursor \
             [--resume session-id]"
                .to_owned(),
        )
    };
    let (provider, session_id) = match args {
        [provider] => (provider, options.value("--resume")),
        [provider, flag, session_id]
            if flag.as_str() == "--resume" && options.value("--resume").is_none() =>
        {
            (provider, Some(session_id.as_str()))
        }
        _ => return Err(usage()),
    };
    let provider = provider
        .as_str()
        .parse::<AgentProvider>()
        .map_err(ServerError::CommandParse)?;
    if session_id.is_some_and(|session_id| {
        session_id.is_empty()
            || session_id.len() > MAX_AGENT_SESSION_ID_BYTES
            || session_id.chars().any(char::is_control)
    }) {
        return Err(ServerError::InvalidCommand(format!(
            "agent session ID must be 1..={MAX_AGENT_SESSION_ID_BYTES} non-control bytes"
        )));
    }
    let setting = |option: &str| -> Result<Option<String>, ServerError> {
        options
            .value(option)
            .map(|value| {
                if value.is_empty()
                    || value.len() > MAX_AGENT_OPTION_BYTES
                    || value.starts_with('-')
                    || value
                        .chars()
                        .any(|character| character.is_control() || character.is_whitespace())
                {
                    Err(ServerError::InvalidCommand(format!(
                        "{command}: {option} takes one word"
                    )))
                } else {
                    Ok(value.to_owned())
                }
            })
            .transpose()
    };
    let model = setting("--model")?;
    let effort = setting("--effort")?;
    let account =
        setting("--account")?.filter(|account| account != zz_protocol::DEFAULT_AGENT_ACCOUNT);
    if account
        .as_deref()
        .is_some_and(|account| !zz_protocol::valid_agent_account(account))
    {
        return Err(ServerError::InvalidCommand(format!(
            "{command}: --account takes an account id (a-z, 0-9, - and _, starting with a letter or digit, at most 32 bytes) or default"
        )));
    }
    if options.has("--fork") && session_id.is_none() {
        return Err(ServerError::CommandParse(
            "select-pane-kind: --fork needs --resume".to_owned(),
        ));
    }
    if let Some(at) = options.value("--at")
        && (at.is_empty()
            || at.len() > MAX_AGENT_SESSION_ID_BYTES
            || !at
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-'))
    {
        return Err(ServerError::InvalidCommand(
            "select-pane-kind: --at takes a message id".to_owned(),
        ));
    }
    let cwd = cwd.map(|cwd| PathBuf::from(expand_home(cwd)));
    if cwd.as_ref().is_some_and(|cwd| {
        !cwd.is_absolute() || cwd.as_os_str().as_encoded_bytes().len() > MAX_GUI_TEXT_BYTES
    }) {
        return Err(ServerError::InvalidCommand(
            "agent working directory must be absolute and stay inside the wire limit".to_owned(),
        ));
    }
    Ok(AgentDescriptor {
        provider,
        cwd,
        session_id: session_id.map(str::to_owned),
        model,
        effort,
        account,
    })
}

const AGENT_SPAWN_FLAGS: [&str; 4] = ["--model", "--effort", "--account", "--resume"];

fn agent_spawn(
    command: &str,
    options: &Options,
    words: &[RawText],
    cwd: Option<&str>,
) -> Result<(AgentDescriptor, Option<String>), ServerError> {
    let usage = |cause: &str| {
        ServerError::NativeCommandParse(format!(
            "{cause}\nusage: {command} [-d] [-P -F format] [-c start-directory] [-t target] \
             --kind agent claude|codex|agy|grok|cursor [--model M] [--effort E] [--account A] [--resume ID] \
             [BRIEF]"
        ))
    };
    let Some((provider, mut rest)) = words.split_first() else {
        return Err(usage(&format!(
            "{command}: --kind agent needs claude or codex"
        )));
    };
    let mut options = options.clone();
    while let Some(flag) = rest.first().and_then(|word| {
        AGENT_SPAWN_FLAGS
            .into_iter()
            .find(|flag| word.as_str() == *flag)
    }) {
        let Some(value) = rest.get(1) else {
            return Err(usage(&format!("{command}: {flag} needs a value")));
        };
        if options.value(flag).is_some() {
            return Err(usage(&format!("{command}: {flag} is given twice")));
        }
        options.values.push((flag.to_owned(), value.to_string()));
        rest = &rest[2..];
    }
    let agent = agent_selection(command, &options, std::slice::from_ref(provider), cwd)
        .map_err(|error| usage(&error.tmux_message()))?;
    if rest.is_empty() {
        return Ok((agent, None));
    }
    let brief = rest
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    let brief = brief.trim_end_matches(['\n', '\r']);
    check_plain_text(command, brief)?;
    Ok((agent, Some(brief.to_owned())))
}

pub fn check_plain_text(command: &str, text: &str) -> Result<(), ServerError> {
    if text.trim().is_empty() {
        return Err(ServerError::InvalidCommand(format!(
            "{command} needs text on the command line or on standard input"
        )));
    }
    if text.len() > MAX_AGENT_SEND_BYTES {
        return Err(ServerError::InvalidCommand(format!(
            "{command} payload is {} bytes; the limit is {MAX_AGENT_SEND_BYTES}",
            text.len()
        )));
    }
    if text
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(ServerError::InvalidCommand(format!(
            "{command} payload contains control characters; pipe plain text"
        )));
    }
    Ok(())
}

pub(super) fn spawn_cwd_source(
    engine: &MuxEngine,
    options: &Options,
    origin: Option<PaneId>,
    kind: &PaneKind,
    format_client: FormatClient,
    hooks: &mut impl StatusHooks,
) -> (Option<PaneId>, Option<String>) {
    if !matches!(kind, PaneKind::Terminal | PaneKind::Agent(_)) {
        return (None, None);
    }
    let inherit_cwd_from = origin.and_then(|origin| engine.state.cwd_donor(origin));
    let cwd = options.value("-c").and_then(|value| {
        let format_context = origin
            .and_then(|pane| ExecutionContext::for_pane(&engine.state, pane))
            .map_or_else(FormatContext::default, |origin| FormatContext {
                session: origin.session,
                window: origin.window,
                pane: origin.pane,
                active_session: origin.session,
                format_client,
                format_type: FormatType::Pane,
            });
        let expanded = expand_format_with_hooks(value, engine, format_context, hooks);
        (!expanded.is_empty()).then(|| expanded.to_string())
    });
    (inherit_cwd_from, cwd)
}

pub(super) fn expand_home(path: &str) -> String {
    let home = || std::env::var("HOME").ok().filter(|home| !home.is_empty());
    match path.strip_prefix('~') {
        Some("") => home().unwrap_or_else(|| path.to_owned()),
        Some(rest) if rest.starts_with('/') => {
            home().map_or_else(|| path.to_owned(), |home| format!("{home}{rest}"))
        }
        _ => path.to_owned(),
    }
}

pub(super) fn validate_spawn_kind<'a>(
    command: &str,
    options: &'a Options,
    positional: &[RawText],
) -> Result<&'a str, ServerError> {
    let kind = options.value("--kind").unwrap_or("terminal");
    if !(matches!(kind, "terminal" | "browser")
        || matches!(command, "split-window" | "new-pane" | "new-window")
            && matches!(kind, "picker" | "agent" | SIMULATOR_PANE_KIND))
    {
        return Err(ServerError::NativeCommandParse(format!(
            "{command}: invalid pane kind {kind:?}"
        )));
    }
    if options.value("--profile").is_some() && kind != "browser" {
        return Err(ServerError::NativeCommandParse(
            "--profile requires --kind browser".to_owned(),
        ));
    }
    if kind != "agent"
        && let Some(flag) = AGENT_SPAWN_FLAGS
            .into_iter()
            .find(|flag| options.value(flag).is_some())
    {
        return Err(ServerError::NativeCommandParse(format!(
            "{flag} requires --kind agent"
        )));
    }
    if kind != "terminal" {
        for flag in ["-e", "-I", "-k"] {
            if flag == "-k" && command == "new-window" {
                continue;
            }
            if options.has(flag) || options.value(flag).is_some() {
                return Err(ServerError::NativeCommandParse(format!(
                    "{flag} requires --kind terminal"
                )));
            }
        }
        if kind == "browser" && positional.len() > 1 {
            return Err(ServerError::NativeCommandParse(
                "--kind browser accepts at most one URL".to_owned(),
            ));
        }
        if kind == SIMULATOR_PANE_KIND && positional.len() > 1 {
            return Err(ServerError::NativeCommandParse(
                "--kind simulator accepts at most one device".to_owned(),
            ));
        }
        if kind == "picker" && !positional.is_empty() {
            return Err(ServerError::NativeCommandParse(format!(
                "--kind {kind} does not accept positional arguments"
            )));
        }
    }
    Ok(kind)
}

fn float_place(
    position: &str,
    (wx, wy): (i32, i32),
    (px, py): (i32, i32),
    border: i32,
) -> Option<(i32, i32)> {
    Some(match position {
        "top-left" => (border, border),
        "top-centre" | "top-center" => ((wx - px) / 2, border),
        "top-right" => (wx - px - border, border),
        "centre-left" | "center-left" => (border, (wy - py) / 2),
        "centre" | "center" => ((wx - px) / 2, (wy - py) / 2),
        "centre-right" | "center-right" => (wx - px - border, (wy - py) / 2),
        "bottom-left" => (border, wy - py - border),
        "bottom-centre" | "bottom-center" => ((wx - px) / 2, wy - py - border),
        "bottom-right" => (wx - px - border, wy - py - border),
        "top-left-centre" | "top-left-center" => (wx / 4 - px / 2, wy / 4 - py / 2),
        "top-right-centre" | "top-right-center" => ((3 * wx) / 4 - px / 2, wy / 4 - py / 2),
        "bottom-left-centre" | "bottom-left-center" => (wx / 4 - px / 2, (3 * wy) / 4 - py / 2),
        "bottom-right-centre" | "bottom-right-center" => {
            ((3 * wx) / 4 - px / 2, (3 * wy) / 4 - py / 2)
        }
        _ => return None,
    })
}

fn browser_from_args(
    options: &Options,
    positional: &[RawText],
) -> Result<BrowserDescriptor, ServerError> {
    let profile = normalize_browser_profile_name(
        options
            .value("--profile")
            .unwrap_or(DEFAULT_BROWSER_PROFILE),
    )
    .map_err(|error| ServerError::InvalidCommand(error.to_string()))?;
    Ok(BrowserDescriptor::single(
        positional
            .first()
            .map_or_else(|| "about:blank".to_owned(), ToString::to_string),
        profile,
    ))
}
