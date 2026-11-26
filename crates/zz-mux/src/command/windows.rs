use super::*;

impl MuxEngine {
    fn kill_window_state(&mut self, window: WindowId) -> Result<Vec<PaneId>, ServerError> {
        let session = self
            .state
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?
            .session;
        let marker = self.destroyed_session_marker(session)?;
        let panes = self.state.kill_window(window)?;
        self.record_destroyed_session(marker);
        Ok(panes)
    }

    pub(super) fn new_window(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("new-window", args)?;
        let (kind, brief) =
            self.spawn_pane_kind("new-window", context, &options, &positional, hooks)?;
        let command = matches!(kind, PaneKind::Terminal)
            .then(|| shell_command_positional(&positional))
            .flatten();
        let mut execution =
            self.new_window_with_options(context, &options, kind, command, hooks)?;
        queue_agent_brief(&mut execution, brief);
        Ok(execution)
    }

    fn new_window_with_options(
        &mut self,
        context: &mut ExecutionContext,
        options: &Options,
        kind: PaneKind,
        command: Option<Vec<String>>,
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let active_session = context.session;
        let format_client = context.format_client();
        let target_format_client = context.target_format_client();
        let destination = window_destination(&self.state, options.value("-t"), context)?;
        let session = destination.session;
        let empty = pane_spawn_empty(options, command.as_deref())?;
        let environment = spawn_environment(options);
        let selects = !options.has("-d");
        let name_context = FormatContext {
            session: Some(session),
            window: None,
            pane: None,
            active_session,
            format_client: context.format_client(),
            format_type: FormatType::Session,
        };
        let name = options
            .value("-n")
            .map(|name| {
                let expanded = expand_format_with_hooks(name, self, name_context, hooks);
                tmux_clean_name(&expanded, "window")
            })
            .transpose()?;
        if options.has("-S")
            && destination.index.is_none()
            && let Some(name) = name.as_deref()
        {
            let lookup_name = expand_format_with_hooks(name, self, name_context, hooks);
            let matches = self.state.sessions[&session]
                .windows
                .iter()
                .copied()
                .filter(|window| self.state.windows[window].name == lookup_name)
                .collect::<Vec<_>>();
            if matches.len() > 1 {
                return Err(ServerError::InvalidCommand(format!(
                    "multiple windows named {name}"
                )));
            }
            if let Some(existing) = matches.first().copied() {
                if selects {
                    self.state.select_window(session, existing)?;
                    context.retarget(&ExecutionContext::new(
                        Some(session),
                        Some(existing),
                        Some(window_active_pane(&self.state, existing)?),
                    ));
                }
                return Ok(Execution::effect(MuxEffect::SuppressAfterHook));
            }
        }
        let context_pane_in_session = context.pane.filter(|pane| {
            self.state
                .window_for_pane(*pane)
                .and_then(|window| self.state.windows.get(&window))
                .is_some_and(|window| window.session == session)
        });
        let origin = match context_pane_in_session {
            Some(pane) => Some(pane),
            None => Some(window_active_pane(
                &self.state,
                session_active_window(&self.state, session)?,
            )?),
        };
        let (inherit_cwd_from, cwd) =
            spawn_cwd_source(self, options, origin, &kind, format_client, hooks);
        let before = options.has("-b");
        let index = if options.has("-a") || before {
            let target = match destination.index {
                Some(index) => index,
                None => window_index(&self.state, session_active_window(&self.state, session)?)?,
            };
            if self.state.window_at_index(session, target).is_some() {
                let index = if before {
                    target
                } else {
                    target.checked_add(1).ok_or_else(|| {
                        ServerError::InvalidCommand("no free window index".to_owned())
                    })?
                };
                self.state.shift_windows_up(session, index)?;
                Some(index)
            } else {
                Some(target)
            }
        } else {
            destination.index
        };
        let replaced = match index {
            Some(index) if options.has("-k") => self.state.window_at_index(session, index),
            _ => None,
        };
        let snapshot_kind = pane_kind_snapshot(&kind);
        let mut effects = Vec::new();
        let has_name = name.is_some();
        let window_name = name.or_else(|| index.map(|index| index.to_string()));
        let base_index = self.base_index_for_session(session);
        if let Some(index) = index
            && replaced.is_none()
            && self.state.window_at_index(session, index).is_some()
        {
            return Err(ServerError::InvalidCommand(format!(
                "create window failed: index {index} in use"
            )));
        }
        let (window, pane) = self.state.create_window_at_with_base_index(
            session,
            index.filter(|_| replaced.is_none()),
            window_name,
            kind,
            selects,
            base_index,
        )?;
        if empty {
            self.state.mark_pane_empty(pane)?;
        }
        if has_name {
            self.state
                .set_window_automatic_rename(window, Some(false))?;
        }
        if let Some(replaced) = replaced {
            effects.push(MuxEffect::PanesRemoved(self.kill_window_state(replaced)?));
            self.state
                .set_window_index(window, index.expect("replacing an occupied index"))?;
        }
        if selects {
            context.retarget(&ExecutionContext::new(
                Some(session),
                Some(window),
                Some(pane),
            ));
        }
        effects.push(MuxEffect::PaneCreated {
            pane,
            kind: snapshot_kind,
            inherit_cwd_from,
            cwd,
            command,
            environment,
            empty,
        });
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
        let output = self.pane_creation_output(
            options,
            session,
            window,
            pane,
            active_session,
            target_format_client,
            hooks,
        );
        Ok(Execution { output, effects })
    }

    pub(super) fn list_windows(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("list-windows", args)?;
        reject_positionals("list-windows", &positional)?;
        if options.has("-a") && self.state.sessions.is_empty() {
            return Ok(Execution::default());
        }
        let target_session = self.resolve_session(options.value("-t"), context.session)?;
        let sort = TmuxSort::parse(options.value("-O"), options.has("-r"), None)?;
        let session_ids = if options.has("-a") {
            self.state
                .sessions_by_name()
                .into_iter()
                .map(|session| session.id)
                .collect::<Vec<_>>()
        } else {
            vec![target_session]
        };
        let format = options.value("-F").unwrap_or(if options.has("-a") {
            DEFAULT_LIST_WINDOWS_WITH_SESSION_FORMAT
        } else {
            DEFAULT_LIST_WINDOWS_FORMAT
        });
        let mut windows = Vec::new();
        for session_id in session_ids {
            let session = self
                .state
                .sessions
                .get(&session_id)
                .ok_or_else(|| ServerError::MissingTarget(session_id.to_string()))?;
            for window_id in &session.windows {
                if !self.state.windows.contains_key(window_id) {
                    continue;
                }
                windows.push((session_id, *window_id));
            }
        }
        sort.apply(&mut windows, |(_, left), (_, right)| {
            let left = &self.state.windows[left];
            let right = &self.state.windows[right];
            let ordering = match sort.order() {
                Some(TmuxSortOrder::Activity) => right.activity.cmp(&left.activity),
                Some(TmuxSortOrder::Creation) => left.created.cmp(&right.created),
                Some(TmuxSortOrder::Index) => left.index.cmp(&right.index),
                Some(TmuxSortOrder::Name) => left.name.cmp(&right.name),
                Some(TmuxSortOrder::Size) => {
                    let left_extent = left.layout.extent();
                    let right_extent = right.layout.extent();
                    (u32::from(left_extent.0) * u32::from(left_extent.1))
                        .cmp(&(u32::from(right_extent.0) * u32::from(right_extent.1)))
                }
                _ => std::cmp::Ordering::Equal,
            };
            ordering.then_with(|| left.name.cmp(&right.name))
        });
        let line = windows.len();
        let mut output = Vec::new();
        let format_client = context.format_client();
        let universe = self.format_universe(format_client);
        let mut dumps = LayoutDumps::default();
        for (session, window) in windows {
            let format_context = FormatContext {
                session: Some(session),
                window: Some(window),
                pane: None,
                active_session: context.session,
                format_client,
                format_type: FormatType::Window,
            };
            let prepared =
                PreparedFormat::with_universe(self, format_context, universe.clone(), &mut dumps);
            if let Some(filter) = options.value("-f") {
                let mut row_hooks = RowFormatHooks { inner: hooks, line };
                if !format_true(&prepared.expand(filter, &mut row_hooks)) {
                    continue;
                }
            }
            let mut row_hooks = RowFormatHooks { inner: hooks, line };
            output.push(if options.has("--json") {
                RawText::from(
                    prepared
                        .values()
                        .scoped_format_values("window", &mut row_hooks)
                        .to_string(),
                )
            } else {
                prepared.expand(format, &mut row_hooks)
            });
        }
        Ok(Execution::output(RawText::join(&output, b"\n")))
    }

    pub(super) fn rename_window(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("rename-window", args)?;
        let name = exactly_one_argument("rename-window", &positional)?;
        let window = self.resolve_window(options.value("-t"), context.session, context.window)?;
        let session = self.state.windows[&window].session;
        let name = expand_format_with_hooks(
            name,
            self,
            FormatContext {
                session: Some(session),
                window: Some(window),
                pane: None,
                active_session: context.session,
                format_client: context.target_format_client(),
                format_type: FormatType::Pane,
            },
            hooks,
        );
        let name = tmux_clean_name(&name, "window")?;
        self.state.rename_window(window, name)?;
        self.state
            .set_window_automatic_rename(window, Some(false))?;
        Ok(Execution::default())
    }

    pub(super) fn select_window(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("select-window", args)?;
        reject_positionals("select-window", &positional)?;
        let target = options.value("-t");
        if options.has("-n") || options.has("-p") {
            let session = self.session_of_window_target(target, context)?;
            let direction = if options.has("-n") { 1 } else { -1 };
            return self.step_window_in_session(context, session, direction, false);
        }
        if options.has("-l") {
            let session = self.session_of_window_target(target, context)?;
            return self.activate_last_window(context, session);
        }
        let window = self.resolve_window(target, context.session, context.window)?;
        let session = self
            .state
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?
            .session;
        if options.has("-T") && session_active_window(&self.state, session)? == window {
            return self.activate_last_window(context, session);
        }
        self.state.select_window(session, window)?;
        context.session = Some(session);
        context.window = Some(window);
        context.pane = Some(window_active_pane(&self.state, window)?);
        Ok(Execution::default())
    }

    fn session_of_window_target(
        &self,
        target: Option<&str>,
        context: &ExecutionContext,
    ) -> Result<SessionId, ServerError> {
        let Some(target) = target else {
            return self.resolve_session(None, context.session);
        };
        let window = self.resolve_window(Some(target), context.session, context.window)?;
        self.state
            .windows
            .get(&window)
            .map(|window| window.session)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))
    }

    pub(super) fn last_window(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, _) = parse_command_options("last-window", args)?;
        let session = self.resolve_session(options.value("-t"), context.session)?;
        self.activate_last_window(context, session)
    }

    fn activate_last_window(
        &mut self,
        context: &mut ExecutionContext,
        session: SessionId,
    ) -> Result<Execution, ServerError> {
        let window = self.state.last_window(session)?;
        self.state.select_window(session, window)?;
        context.session = Some(session);
        context.window = Some(window);
        context.pane = Some(window_active_pane(&self.state, window)?);
        Ok(Execution::default())
    }

    pub(super) fn step_window(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
        direction: isize,
    ) -> Result<Execution, ServerError> {
        let command = if direction > 0 {
            "next-window"
        } else {
            "previous-window"
        };
        let (options, positional) = parse_command_options(command, args)?;
        reject_positionals(command, &positional)?;
        let session = self.resolve_session(options.value("-t"), context.session)?;
        self.step_window_in_session(context, session, direction, options.has("-a"))
    }

    fn step_window_in_session(
        &mut self,
        context: &mut ExecutionContext,
        session: SessionId,
        direction: isize,
        alerted_only: bool,
    ) -> Result<Execution, ServerError> {
        let state = self
            .state
            .sessions
            .get(&session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?;
        let len = isize::try_from(state.windows.len()).expect("window count fits isize");
        if len == 0 {
            return Err(ServerError::MissingTarget(session.to_string()));
        }
        let current = state.active_window;
        let current_index = isize::try_from(
            state
                .windows
                .iter()
                .position(|window| *window == current)
                .unwrap_or(0),
        )
        .expect("index fits isize");
        let step_to = |steps: isize| {
            let index = (current_index + steps * direction).rem_euclid(len);
            state.windows[usize::try_from(index).expect("nonnegative index")]
        };
        let no_window = || {
            ServerError::InvalidCommand(format!(
                "no {} window",
                if direction > 0 { "next" } else { "previous" }
            ))
        };
        let window = if alerted_only {
            (1..=len)
                .map(step_to)
                .find(|window| self.window_alerted(*window))
                .ok_or_else(no_window)?
        } else {
            step_to(1)
        };
        if window == current {
            return Err(no_window());
        }
        self.state.select_window(session, window)?;
        context.session = Some(session);
        context.window = Some(window);
        context.pane = Some(window_active_pane(&self.state, window)?);
        Ok(Execution::default())
    }

    pub(super) fn window_alerted(&self, window: WindowId) -> bool {
        self.state.windows.get(&window).is_some_and(|window| {
            window.activity_flag
                || window.silence_flag
                || window.panes.values().any(|pane| pane.bell)
        })
    }

    pub(super) fn renumber_session_if_enabled(
        &mut self,
        session: SessionId,
    ) -> Result<(), ServerError> {
        if !self.state.sessions.contains_key(&session)
            || !self.renumber_windows_for_session(session)
        {
            return Ok(());
        }
        let base_index = self.base_index_for_session(session);
        self.state.renumber_windows(session, base_index)
    }

    pub(super) fn kill_window(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("kill-window", args)?;
        reject_positionals("kill-window", &positional)?;
        let window = self.resolve_window(options.value("-t"), context.session, context.window)?;
        if options.value("-f").is_some() && !options.has("-a") {
            return Err(ServerError::InvalidCommand(
                "-f only valid with -a".to_owned(),
            ));
        }
        let session = self
            .state
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?
            .session;
        let mut targets = if options.has("-a") {
            self.state
                .sessions
                .get(&session)
                .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?
                .windows
                .iter()
                .copied()
                .filter(|candidate| *candidate != window)
                .collect()
        } else {
            vec![window]
        };
        if let Some(filter) = options.value("-f") {
            targets.retain(|target| {
                format_true(&expand_format_with_hooks(
                    filter,
                    self,
                    FormatContext {
                        session: Some(session),
                        window: Some(*target),
                        pane: None,
                        active_session: context.session,
                        format_client: FormatClient::NoClient,
                        format_type: FormatType::Window,
                    },
                    hooks,
                ))
            });
        }
        let sessions = targets
            .iter()
            .filter_map(|target| self.state.windows.get(target).map(|window| window.session))
            .collect::<BTreeSet<_>>();
        for session in &sessions {
            if self.renumber_windows_for_session(*session) {
                let removed_windows = targets
                    .iter()
                    .filter(|target| {
                        self.state
                            .windows
                            .get(target)
                            .is_some_and(|window| window.session == *session)
                    })
                    .count();
                self.state.validate_renumber_capacity(
                    *session,
                    self.base_index_for_session(*session),
                    removed_windows,
                )?;
            }
        }
        let mut panes = Vec::new();
        for target in targets {
            panes.extend(self.kill_window_state(target)?);
        }
        for session in sessions {
            self.renumber_session_if_enabled(session)?;
        }
        Ok(Execution::effect(MuxEffect::PanesRemoved(panes)))
    }

    pub(super) fn move_window(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("move-window", args)?;
        reject_positionals("move-window", &positional)?;
        if options.has("-r") {
            if options.value("-s").is_some() {
                self.resolve_window(options.value("-s"), context.session, context.window)?;
            }
            let session = self.resolve_session(options.value("-t"), context.session)?;
            let base_index = self.base_index_for_session(session);
            self.state.renumber_windows(session, base_index)?;
            return Ok(Execution::default());
        }

        let source = self.resolve_window(options.value("-s"), context.session, context.window)?;
        let source_session = self.state.windows[&source].session;
        let source_panes = self.state.windows[&source].pane_order().to_vec();
        let destination = window_destination(&self.state, options.value("-t"), context)?;
        let destination_session = destination.session;
        let base_index = self.base_index_for_session(destination_session);
        if options.value("-s").is_none() && self.renumber_windows_for_session(source_session) {
            let removed_windows = usize::from(source_session != destination_session);
            self.state.validate_renumber_capacity(
                source_session,
                self.base_index_for_session(source_session),
                removed_windows,
            )?;
        }
        let destination_index = if options.has("-a") || options.has("-b") {
            shift_window_insertion_index(&mut self.state, &destination, options.has("-b"))?
        } else {
            destination.index.map_or_else(
                || {
                    self.state
                        .next_window_index(destination_session, base_index)
                },
                Ok,
            )?
        };

        let detached = options.has("-d");
        let original_context = context.clone();
        let destroyed_source = self.destroyed_session_marker(source_session)?;
        let removed = self.state.move_window(
            source,
            destination_session,
            destination_index,
            options.has("-k"),
            !detached,
        )?;
        self.record_destroyed_session(destroyed_source);
        if options.value("-s").is_none() {
            self.renumber_session_if_enabled(source_session)?;
        }

        if detached && original_context.window == Some(source) {
            let window = if self.state.sessions.contains_key(&source_session) {
                session_active_window(&self.state, source_session)?
            } else {
                session_active_window(&self.state, destination_session)?
            };
            let target =
                ExecutionContext::for_pane(&self.state, window_active_pane(&self.state, window)?)
                    .expect("moved window leaves a valid command context");
            context.retarget(&target);
        } else if !detached {
            let target =
                ExecutionContext::for_pane(&self.state, window_active_pane(&self.state, source)?)
                    .expect("moved window has an active pane");
            context.retarget(&target);
        }

        let mut effects = Vec::new();
        if !removed.is_empty() {
            effects.push(MuxEffect::PanesRemoved(removed));
        }
        if source_session == destination_session {
            effects.extend(
                source_panes
                    .into_iter()
                    .map(|pane| MuxEffect::TerminalKnobsChanged {
                        window: None,
                        pane: Some(pane),
                    }),
            );
        } else {
            effects.extend(
                source_panes
                    .into_iter()
                    .map(|pane| MuxEffect::PaneRelocated {
                        pane,
                        from: source_session,
                        to: destination_session,
                    }),
            );
        }
        Ok(Execution {
            output: RawText::default(),
            effects,
        })
    }

    pub(super) fn swap_window(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("swap-window", args)?;
        reject_positionals("swap-window", &positional)?;
        let source = self.resolve_window(options.value("-s"), context.session, context.window)?;
        let target = self.resolve_window(options.value("-t"), context.session, context.window)?;
        if source == target {
            return Ok(Execution::default());
        }
        let source_session = self.state.windows[&source].session;
        let target_session = self.state.windows[&target].session;
        let source_panes = self.state.windows[&source].pane_order().to_vec();
        let target_panes = self.state.windows[&target].pane_order().to_vec();
        self.state.swap_windows(source, target, options.has("-d"))?;

        let mut effects = Vec::new();
        if source_session == target_session {
            effects.extend(source_panes.into_iter().chain(target_panes).map(|pane| {
                MuxEffect::TerminalKnobsChanged {
                    window: None,
                    pane: Some(pane),
                }
            }));
        } else {
            effects.extend(
                source_panes
                    .into_iter()
                    .map(|pane| MuxEffect::PaneRelocated {
                        pane,
                        from: source_session,
                        to: target_session,
                    }),
            );
            effects.extend(
                target_panes
                    .into_iter()
                    .map(|pane| MuxEffect::PaneRelocated {
                        pane,
                        from: target_session,
                        to: source_session,
                    }),
            );
        }
        Ok(Execution {
            output: RawText::default(),
            effects,
        })
    }

    pub(super) fn find_window(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("find-window", args)?;
        if positional.len() != 1 {
            return Err(ServerError::CommandParse(
                "find-window requires exactly one match string".to_owned(),
            ));
        }
        let pattern = &positional[0];
        let suffix = match (options.has("-r"), options.has("-i")) {
            (true, true) => "/ri",
            (true, false) => "/r",
            (false, true) => "/i",
            (false, false) => "",
        };
        let star = if options.has("-r") { "" } else { "*" };
        let all = !options.has("-C") && !options.has("-N") && !options.has("-T");
        let mut predicates = Vec::new();
        if all || options.has("-C") {
            predicates.push(format!("#{{C{suffix}:{pattern}}}"));
        }
        for (flag, field) in [("-N", "window_name"), ("-T", "pane_title")] {
            if all || options.has(flag) {
                predicates.push(format!("#{{m{suffix}:{star}{pattern}{star},#{{{field}}}}}"));
            }
        }
        let filter = predicates
            .into_iter()
            .rev()
            .reduce(|right, left| format!("#{{||:{left},{right}}}"))
            .expect("find-window selects at least one match class");
        let mut chooser_args = vec![RawText::from("-f"), RawText::from(filter)];
        if let Some(target) = options.value("-t") {
            chooser_args.extend(["-t".into(), target.into()]);
        }
        if options.has("-Z") {
            chooser_args.push("-Z".into());
        }
        self.choose_tree(
            context,
            &CommandInvocation::new("choose-tree", chooser_args),
        )
    }

    pub(super) fn resize_window(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("resize-window", args)?;
        if positional.len() > 1 {
            return Err(ServerError::CommandParse(
                "resize-window accepts at most one adjustment".to_owned(),
            ));
        }
        let window = self.resolve_window(options.value("-t"), context.session, context.window)?;
        let adjustment = positional
            .first()
            .map(|value| parse_resize_window_adjustment(value))
            .transpose()?
            .unwrap_or(1);
        let (mut columns, mut rows) = self.state.windows[&window].layout.extent();
        if let Some(width) = options.value("-x") {
            columns = parse_window_extent(width, "width")?;
        }
        if let Some(height) = options.value("-y") {
            rows = parse_window_extent(height, "height")?;
        }
        let columns = u32::from(columns);
        let rows = u32::from(rows);
        let (columns, rows) = if options.has("-L") {
            (
                if columns >= adjustment {
                    columns - adjustment
                } else {
                    columns
                },
                rows,
            )
        } else if options.has("-R") {
            (columns.saturating_add(adjustment), rows)
        } else if options.has("-U") {
            (
                columns,
                if rows >= adjustment {
                    rows - adjustment
                } else {
                    rows
                },
            )
        } else if options.has("-D") {
            (columns, rows.saturating_add(adjustment))
        } else {
            (columns, rows)
        };
        let clamp = |extent: u32| {
            u16::try_from(extent.min(u32::from(PANE_MAXIMUM)))
                .expect("window extent is clamped to u16")
                .max(1)
        };
        if options.has("-A") || options.has("-a") {
            return Ok(Execution::effect(MuxEffect::ResizeWindowFromClients {
                window,
                mode: if options.has("-A") {
                    WindowSize::Largest
                } else {
                    WindowSize::Smallest
                },
            }));
        }
        self.set_manual_window_extent(window, clamp(columns), clamp(rows))?;
        Ok(Execution::effect(MuxEffect::WindowSizeChanged {
            window: Some(window),
        }))
    }

    pub fn set_manual_window_extent(
        &mut self,
        window: WindowId,
        columns: u16,
        rows: u16,
    ) -> Result<(), ServerError> {
        let changed_mode = self.window_size(window) != WindowSize::Manual;
        self.window_options.entry(window).or_default().insert(
            WindowOption::WindowSize,
            WindowSize::Manual.as_str().to_owned(),
        );
        let generation = self.state.generation();
        self.state.resize_window(window, columns, rows)?;
        if let Some(window) = self.state.windows.get_mut(&mut self.state.journal, &window) {
            window.manual_extent = window.layout.extent();
        }
        if changed_mode && self.state.generation() == generation {
            self.state.bump_generation();
        }
        Ok(())
    }

    pub(super) fn restore_manual_window_extents(&mut self, window: Option<WindowId>) {
        let candidates = window.map_or_else(
            || self.state.windows.keys().copied().collect::<Vec<_>>(),
            |window| vec![window],
        );
        for candidate in candidates {
            if self.window_size(candidate) != WindowSize::Manual {
                continue;
            }
            let Some(state) = self.state.windows.get(&candidate) else {
                continue;
            };
            let (columns, rows) = state.manual_extent;
            if state.layout.extent() == (columns, rows) {
                continue;
            }
            if self.state.resize_window(candidate, columns, rows).is_ok()
                && let Some(state) = self
                    .state
                    .windows
                    .get_mut(&mut self.state.journal, &candidate)
            {
                state.manual_extent = state.layout.extent();
            }
        }
    }

    pub fn resize_window_to_extent(&mut self, window: WindowId, columns: u16, rows: u16) -> bool {
        if self.window_size(window) == WindowSize::Manual {
            return false;
        }
        let Some(state) = self.state.windows.get(&window) else {
            return false;
        };
        if state.layout.extent() == (columns, rows) {
            return false;
        }
        self.state.resize_window(window, columns, rows).is_ok()
    }

    pub fn set_pane_geometry(&mut self, pane: PaneId, columns: u16, rows: u16) -> bool {
        let Some(window_id) = self.state.window_for_pane(pane) else {
            return false;
        };
        if self.window_size(window_id) == WindowSize::Manual {
            return false;
        }
        let status = self.pane_border_status(window_id);
        let changed = 'probe: {
            let window = self
                .state
                .windows
                .get_mut(&mut self.state.journal, &window_id)
                .expect("pane window exists");
            let probe = (pane, columns, rows);
            if let Some(zoomed_pane) = window.zoomed_pane {
                if zoomed_pane != pane || window.last_extent_probe == Some(probe) {
                    break 'probe false;
                }
                let measured = (columns, rows);
                if measured == window.layout.extent() {
                    break 'probe false;
                }
                window.layout.resize(measured.0, measured.1);
                window.last_extent_probe = Some(probe);
                break 'probe true;
            }
            let driver = if window.is_floating(window.active_pane) {
                window
                    .last_panes()
                    .iter()
                    .chain(window.pane_order())
                    .copied()
                    .find(|candidate| !window.is_floating(*candidate))
            } else {
                Some(window.active_pane)
            };
            if driver != Some(pane) {
                break 'probe false;
            }
            if window.last_extent_probe == Some(probe) {
                break 'probe false;
            }
            let Some(implied) =
                back_solved_window_extent(&window.layout, pane, status, columns, rows, true)
            else {
                debug_assert!(false, "window pane is missing from its layout");
                break 'probe false;
            };
            let current = window.layout.extent();
            if implied == current {
                break 'probe false;
            }
            window.layout.resize(implied.0, implied.1);
            window.last_extent_probe = Some(probe);
            true
        };
        if changed {
            self.state.bump_generation();
        }
        changed
    }

    #[must_use]
    pub fn pane_geometry(&self, pane: PaneId) -> Option<(u16, u16)> {
        let window = self.state.window_for_pane(pane)?;
        let status = self.displayed_pane_border_status(window);
        self.state
            .windows
            .get(&window)?
            .displayed_pane_geometry_with_border(pane, status)
    }

    #[must_use]
    pub fn pane_origin(&self, pane: PaneId) -> Option<(i32, i32)> {
        let window = self.state.window_for_pane(pane)?;
        let status = self.displayed_pane_border_status(window);
        self.state
            .windows
            .get(&window)?
            .displayed_pane_cell(pane, status)
            .map(|cell| (cell.xoff, cell.yoff))
    }

    #[must_use]
    pub fn pane_geometry_at_window_extent(
        &self,
        pane: PaneId,
        columns: u16,
        rows: u16,
    ) -> Option<(u16, u16)> {
        let window = self.state.window_for_pane(pane)?;
        let window = self.state.windows.get(&window)?;
        if let Some(zoomed) = window.zoomed_pane {
            return (zoomed == pane).then_some((columns, rows));
        }
        let mut layout = window.layout.clone();
        layout.resize(columns, rows);
        let geometry =
            layout.pane_geometry_with_border(pane, self.pane_border_status(window.id))?;
        Some((geometry.sx, geometry.sy))
    }

    #[must_use]
    pub fn window_extent_for_pane_geometry(
        &self,
        pane: PaneId,
        columns: u16,
        rows: u16,
    ) -> Option<(u16, u16)> {
        let window = self.state.window_for_pane(pane)?;
        let window = self.state.windows.get(&window)?;
        if let Some(zoomed) = window.zoomed_pane {
            return (zoomed == pane).then_some((columns, rows));
        }
        back_solved_window_extent(
            &window.layout,
            pane,
            self.pane_border_status(window.id),
            columns,
            rows,
            false,
        )
    }

    #[must_use]
    pub fn window_extent(&self, window: WindowId, axis: Axis) -> Option<u16> {
        let extent = self.state.windows.get(&window)?.layout.extent();
        Some(match axis {
            Axis::Horizontal => extent.0,
            Axis::Vertical => extent.1,
        })
    }
}

pub(super) fn initial_window_extent(
    options: &Options,
    default_size: &str,
    client_size: Option<(u16, u16)>,
) -> Result<(u16, u16), ServerError> {
    let default = parse_default_size(default_size).unwrap_or(DEFAULT_WINDOW_EXTENT);
    Ok((
        initial_window_dimension(
            options,
            "-x",
            default.0,
            client_size.map_or(DEFAULT_WINDOW_EXTENT.0, |size| size.0),
            "width",
        )?,
        initial_window_dimension(
            options,
            "-y",
            default.1,
            client_size.map_or(DEFAULT_WINDOW_EXTENT.1, |size| size.1),
            "height",
        )?,
    ))
}

pub(super) fn parse_default_size(value: &str) -> Option<(u16, u16)> {
    let (width, rest) = parse_unsigned_prefix(value)?;
    let (height, _) = parse_unsigned_prefix(rest.strip_prefix('x')?)?;
    let clamp = |dimension: u64| {
        u16::try_from(dimension.clamp(1, 10_000)).expect("default size is clamped")
    };
    Some((clamp(width), clamp(height)))
}

fn parse_unsigned_prefix(value: &str) -> Option<(u64, &str)> {
    let end = value.bytes().take_while(u8::is_ascii_digit).count();
    (end != 0).then(|| {
        (
            value[..end].parse::<u64>().unwrap_or(u64::MAX),
            &value[end..],
        )
    })
}

fn initial_window_dimension(
    options: &Options,
    option: &str,
    default: u16,
    dash: u16,
    dimension: &str,
) -> Result<u16, ServerError> {
    let Some(value) = options.value(option) else {
        return Ok(default);
    };
    if value == "-" {
        return Ok(dash);
    }
    match value.parse::<i128>() {
        Ok(number) if number < 1 => Err(ServerError::InvalidCommand(format!(
            "{dimension} too small"
        ))),
        Ok(number) if number > i128::from(u16::MAX) => Err(ServerError::InvalidCommand(format!(
            "{dimension} too large"
        ))),
        Ok(number) => Ok(u16::try_from(number).expect("bounded dimension fits u16")),
        Err(_) if decimal_digits(value.strip_prefix('-')) => Err(ServerError::InvalidCommand(
            format!("{dimension} too small"),
        )),
        Err(_) if decimal_digits(Some(value.strip_prefix('+').unwrap_or(value))) => Err(
            ServerError::InvalidCommand(format!("{dimension} too large")),
        ),
        Err(_) => Err(ServerError::InvalidCommand(format!("{dimension} invalid"))),
    }
}

pub(super) fn require_client_terminal(context: &ExecutionContext) -> Result<(), ServerError> {
    match context.client_terminal {
        ClientTerminal::Absent => Err(ServerError::InvalidCommand(
            "open terminal failed: not a terminal".to_owned(),
        )),
        ClientTerminal::NoClient | ClientTerminal::Present => Ok(()),
    }
}

fn implied_window_extent(measured: u16, window: u16, pane: u16) -> u16 {
    if pane == 0 {
        return window;
    }
    let numerator = u64::from(measured) * u64::from(window);
    let rounded = (numerator + u64::from(pane) / 2) / u64::from(pane);
    u16::try_from(rounded).unwrap_or(u16::MAX)
}

fn back_solved_window_extent(
    layout: &CellLayout,
    pane: PaneId,
    status: PaneBorderStatus,
    columns: u16,
    rows: u16,
    guard: bool,
) -> Option<(u16, u16)> {
    let cell = layout.pane_geometry(pane)?;
    let carved = layout.pane_geometry_with_border(pane, status)?;
    let window = layout.extent();
    Some((
        back_solved_extent_axis(columns, window.0, cell.sx, carved.sx, guard),
        back_solved_extent_axis(rows, window.1, cell.sy, carved.sy, guard),
    ))
}

fn back_solved_extent_axis(measured: u16, window: u16, cell: u16, pane: u16, guard: bool) -> u16 {
    if cell == window {
        return measured.saturating_add(window.saturating_sub(pane)).max(1);
    }
    if guard && pane.abs_diff(measured) <= 1 {
        return window;
    }
    implied_window_extent(measured, window, pane)
}

pub(super) fn window_destination(
    state: &MuxState,
    target: Option<&str>,
    context: &ExecutionContext,
) -> Result<WindowDestination, ServerError> {
    let (session, index) =
        state.resolve_window_index_target(target, context.session, context.window)?;
    Ok(WindowDestination { session, index })
}

pub(super) fn shift_window_insertion_index(
    state: &mut MuxState,
    destination: &WindowDestination,
    before: bool,
) -> Result<u32, ServerError> {
    let target = destination
        .index
        .and_then(|index| state.window_at_index(destination.session, index))
        .unwrap_or(state.sessions[&destination.session].active_window);
    let target_index = state.windows[&target].index;
    let index = if before {
        target_index
    } else {
        target_index
            .checked_add(1)
            .ok_or_else(|| ServerError::InvalidCommand("no free window index".to_owned()))?
    };
    state.shift_windows_up(destination.session, index)?;
    Ok(index)
}
