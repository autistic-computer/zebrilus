use super::*;

impl MuxEngine {
    fn kill_pane_state(&mut self, pane: PaneId) -> Result<Vec<PaneId>, ServerError> {
        let session = self
            .state
            .window_for_pane(pane)
            .and_then(|window| self.state.windows.get(&window))
            .map(|window| window.session)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let marker = self.destroyed_session_marker(session)?;
        let panes = self.state.kill_pane(pane)?;
        self.record_destroyed_session(marker);
        Ok(panes)
    }

    pub(super) fn select_pane(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, _) = parse_command_options("select-pane", args)?;
        let target = options.value("-t");
        let start = match target {
            Some(":.+" | ".+" | ":+") => {
                let current = self.resolve_pane(None, context.window, context.pane)?;
                self.state.next_pane(current)?
            }
            Some(":.-" | ".-" | ":-") => {
                let current = self.resolve_pane(None, context.window, context.pane)?;
                self.state.previous_pane(current)?
            }
            _ => self.resolve_pane(target, context.window, context.pane)?,
        };
        let direction = if options.has("-L") {
            Some(PaneDirection::Left)
        } else if options.has("-R") {
            Some(PaneDirection::Right)
        } else if options.has("-U") {
            Some(PaneDirection::Up)
        } else if options.has("-D") {
            Some(PaneDirection::Down)
        } else {
            None
        };
        if options.has("-l") {
            let window = self
                .state
                .window_for_pane(start)
                .expect("resolved pane has a window");
            let pane = self.state.last_pane(window)?;
            if options.has("-e") {
                self.state.set_pane_input_off(pane, false)?;
                return Ok(Execution::effect(MuxEffect::SuppressAfterHook));
            }
            if options.has("-d") {
                self.state.set_pane_input_off(pane, true)?;
                return Ok(Execution::effect(MuxEffect::SuppressAfterHook));
            }
            self.select_pane_target(context, pane, options.has("-Z"), hooks)?;
            return Ok(Execution::effect(MuxEffect::SuppressAfterHook));
        }
        if options.has("-m") || options.has("-M") {
            if options.has("-m") && !self.pane_is_visible(start) {
                return Ok(Execution::effect(MuxEffect::SuppressAfterHook));
            }
            let old = self.state.marked_pane();
            if options.has("-M") {
                self.state.clear_marked_pane();
            } else {
                self.state.toggle_marked_pane(start)?;
            }
            let mut execution = self.marked_pane_changed(context, start, old);
            execution.effects.push(MuxEffect::SuppressAfterHook);
            return Ok(execution);
        }
        let mut execution = Execution::effect(MuxEffect::SuppressAfterHook);
        if let Some(style) = options.value("-P") {
            let style = style.to_owned();
            let pane_options = self.scalar_table_mut_or_insert(TmuxOptionTarget::Pane(start));
            pane_options.insert("window-style", style.clone());
            pane_options.insert("window-active-style", style);
            execution.effects.push(MuxEffect::TerminalKnobsChanged {
                window: None,
                pane: Some(start),
            });
        }
        if options.has("-g") {
            execution.output = self
                .scalar_option_effective(TmuxOptionTarget::Pane(start), "window-style")
                .unwrap_or_default()
                .into();
            return Ok(execution);
        }
        let pane = if let Some(direction) = direction {
            let Some(pane) = self.state.pane_in_direction(start, direction)? else {
                return Ok(execution);
            };
            pane
        } else {
            start
        };
        if options.has("-e") {
            self.state.set_pane_input_off(pane, false)?;
            return Ok(execution);
        }
        if options.has("-d") {
            self.state.set_pane_input_off(pane, true)?;
            return Ok(execution);
        }
        if let Some(title) = options.value("-T") {
            let target =
                ExecutionContext::for_pane(&self.state, start).expect("resolved pane exists");
            let title = self.expand_pane_format(
                title,
                &target,
                context.session,
                context.target_format_client(),
                hooks,
            );
            if let Some(title) = tmux_clean_title(&title) {
                self.state.pin_pane_title(pane, title)?;
            }
            return Ok(execution);
        }
        if self
            .state
            .window_for_pane(pane)
            .and_then(|window| self.state.windows.get(&window))
            .is_some_and(|window| window.active_pane != pane)
        {
            execution
                .effects
                .retain(|effect| *effect != MuxEffect::SuppressAfterHook);
        }
        self.select_pane_target(context, pane, options.has("-Z"), hooks)?;
        Ok(execution)
    }

    pub(super) fn pane_is_visible(&self, pane: PaneId) -> bool {
        self.state
            .window_for_pane(pane)
            .and_then(|window| self.state.windows.get(&window))
            .is_none_or(|window| window.zoomed_pane.is_none() || window.active_pane == pane)
    }

    fn select_pane_target(
        &mut self,
        context: &mut ExecutionContext,
        pane: PaneId,
        preserve_zoom: bool,
        hooks: &mut impl StatusHooks,
    ) -> Result<(), ServerError> {
        if self.state.select_pane_with_zoom(pane, preserve_zoom)? {
            let target =
                ExecutionContext::for_pane(&self.state, pane).expect("selected pane exists");
            context.retarget(&target);
            self.refresh_automatic_window_name_for_pane(pane, hooks);
        }
        Ok(())
    }

    pub(super) fn last_pane(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("last-pane", args)?;
        if !positional.is_empty() {
            return Err(ServerError::CommandParse(
                "last-pane supports only -d, -e, -t, and -Z".to_owned(),
            ));
        }
        let window = self.resolve_window(options.value("-t"), context.session, context.window)?;
        let pane = self.state.last_pane(window)?;
        if options.has("-e") {
            self.state.set_pane_input_off(pane, false)?;
            return Ok(Execution::default());
        }
        if options.has("-d") {
            self.state.set_pane_input_off(pane, true)?;
            return Ok(Execution::default());
        }
        self.select_pane_target(context, pane, options.has("-Z"), hooks)?;
        Ok(Execution::default())
    }

    pub(super) fn swap_pane(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("swap-pane", args)?;
        if !positional.is_empty() {
            return Err(ServerError::CommandParse(
                "swap-pane supports -d, -D, -U, -Z, -s, and -t".to_owned(),
            ));
        }
        let target = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let initial_source = match options.value("-s") {
            Some(source) => Some(self.resolve_pane(Some(source), context.window, context.pane)?),
            None => context.pane,
        };
        let is_modal = |pane: PaneId| {
            self.state
                .window_for_pane(pane)
                .is_some_and(|window| self.state.windows[&window].modal_pane() == Some(pane))
        };
        if is_modal(target) || initial_source.is_some_and(is_modal) {
            return Err(ServerError::InvalidCommand("pane is modal".to_owned()));
        }
        let target_window = self
            .state
            .window_for_pane(target)
            .expect("resolved target pane has a window");
        let target_floating = self.state.windows[&target_window].is_floating(target);
        let source = if options.has("-D") {
            if target_floating {
                return Err(ServerError::InvalidCommand(
                    "cannot swap down on floating pane".to_owned(),
                ));
            }
            self.state.next_tiled_pane(target, 1)?
        } else if options.has("-U") {
            if target_floating {
                return Err(ServerError::InvalidCommand(
                    "cannot swap up on floating pane".to_owned(),
                ));
            }
            self.state.next_tiled_pane(target, -1)?
        } else {
            let source = options.value("-s").ok_or_else(|| {
                ServerError::InvalidCommand(
                    "swap-pane needs -s when neither -U nor -D is used".to_owned(),
                )
            })?;
            self.resolve_pane(Some(source), context.window, context.pane)?
        };

        let source_window = self
            .state
            .window_for_pane(source)
            .expect("resolved source pane has a window");
        let target_window = self
            .state
            .window_for_pane(target)
            .expect("resolved target pane has a window");
        let source_session = self.state.windows[&source_window].session;
        let target_session = self.state.windows[&target_window].session;
        let zoomed_before = [source_window, target_window]
            .map(|window| self.state.windows[&window].zoomed_pane.is_some());
        self.state
            .swap_panes(source, target, options.has("-d"), options.has("-Z"))?;
        let zoom_cycles = [source_window, target_window]
            .into_iter()
            .zip(zoomed_before)
            .enumerate()
            .filter(|(index, (window, was_zoomed))| {
                *was_zoomed
                    && options.has("-Z")
                    && (*index == 0 || *window != source_window)
                    && self
                        .state
                        .windows
                        .get(window)
                        .is_some_and(|state| state.zoomed_pane.is_some())
            })
            .map(|(_, (window, _))| MuxEffect::ZoomCycled { window })
            .collect::<Vec<_>>();
        let active = self.state.windows[&target_window].active_pane;
        let context_target = ExecutionContext::for_pane(&self.state, active)
            .expect("the target window retains an active pane after a swap");
        context.retarget(&context_target);

        let mut execution = Execution::default();
        execution.effects.extend(zoom_cycles);
        if source_session != target_session && source != target {
            execution.effects.extend([
                MuxEffect::PaneRelocated {
                    pane: source,
                    from: source_session,
                    to: target_session,
                },
                MuxEffect::PaneRelocated {
                    pane: target,
                    from: target_session,
                    to: source_session,
                },
            ]);
        } else if source_window != target_window {
            execution.effects.extend([
                MuxEffect::TerminalKnobsChanged {
                    window: None,
                    pane: Some(source),
                },
                MuxEffect::TerminalKnobsChanged {
                    window: None,
                    pane: Some(target),
                },
            ]);
        }
        Ok(execution)
    }

    pub(super) fn list_panes(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("list-panes", args)?;
        reject_positionals("list-panes", &positional)?;
        if options.has("-a") && self.state.sessions.is_empty() {
            return Ok(Execution::default());
        }
        let target_window =
            self.resolve_window(options.value("-t"), context.session, context.window)?;
        let sort = TmuxSort::parse(options.value("-O"), options.has("-r"), None)?;
        let target_session = self.state.windows[&target_window].session;
        let window_ids = if options.has("-a") {
            self.state
                .sessions_by_name()
                .into_iter()
                .flat_map(|session| session.windows.iter().copied())
                .collect::<Vec<_>>()
        } else if options.has("-s") {
            self.state.sessions[&target_session].windows.clone()
        } else {
            vec![target_window]
        };
        let format = options.value("-F").unwrap_or(if options.has("-a") {
            DEFAULT_LIST_PANES_WITH_SERVER_FORMAT
        } else if options.has("-s") {
            DEFAULT_LIST_PANES_WITH_SESSION_FORMAT
        } else {
            DEFAULT_LIST_PANES_FORMAT
        });
        let mut output = Vec::new();
        let format_client = context.format_client();
        let universe = self.format_universe(format_client);
        let mut dumps = LayoutDumps::default();
        for window_id in window_ids {
            let window = self
                .state
                .windows
                .get(&window_id)
                .ok_or_else(|| ServerError::MissingTarget(window_id.to_string()))?;
            let border_status = self.pane_border_status(window_id);
            let mut panes = window.pane_order().to_vec();
            sort.apply(&mut panes, |left, right| {
                let left_pane = &window.panes[left];
                let right_pane = &window.panes[right];
                let left_index = window
                    .pane_order()
                    .iter()
                    .position(|pane| pane == left)
                    .unwrap_or_default();
                let right_index = window
                    .pane_order()
                    .iter()
                    .position(|pane| pane == right)
                    .unwrap_or_default();
                let ordering = match sort.order() {
                    Some(TmuxSortOrder::Activity) => {
                        left_pane.active_point.cmp(&right_pane.active_point)
                    }
                    Some(TmuxSortOrder::Creation) => left.cmp(right),
                    Some(TmuxSortOrder::Index) => left_index.cmp(&right_index),
                    Some(TmuxSortOrder::Name) => left_pane.title.cmp(&right_pane.title),
                    Some(TmuxSortOrder::Size) => {
                        let left_extent = window
                            .displayed_pane_geometry_with_border(*left, border_status)
                            .unwrap_or_default();
                        let right_extent = window
                            .displayed_pane_geometry_with_border(*right, border_status)
                            .unwrap_or_default();
                        (u32::from(left_extent.0) * u32::from(left_extent.1))
                            .cmp(&(u32::from(right_extent.0) * u32::from(right_extent.1)))
                    }
                    Some(TmuxSortOrder::Z) => window.pane_z(*left).cmp(&window.pane_z(*right)),
                    _ => std::cmp::Ordering::Equal,
                };
                ordering.then_with(|| left_pane.title.cmp(&right_pane.title))
            });
            let line = panes.len();
            for pane_id in panes {
                let Some(pane) = window.panes.get(&pane_id) else {
                    continue;
                };
                let format_context = FormatContext {
                    session: Some(window.session),
                    window: Some(window_id),
                    pane: Some(pane.id),
                    active_session: context.session,
                    format_client,
                    format_type: FormatType::Pane,
                };
                let prepared = PreparedFormat::with_universe(
                    self,
                    format_context,
                    universe.clone(),
                    &mut dumps,
                );
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
                            .scoped_format_values("pane", &mut row_hooks)
                            .to_string(),
                    )
                } else {
                    prepared.expand(format, &mut row_hooks)
                });
            }
        }
        let output = RawText::join(&output, b"\n");
        Ok(Execution::output(output))
    }

    pub(super) fn resize_pane(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("resize-pane", args)?;
        if positional.len() > 1 {
            return Err(ServerError::CommandParse(
                "resize-pane accepts at most one adjustment".to_owned(),
            ));
        }
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        if options.has("-M") {
            return self.resize_pane_from_mouse(context, pane);
        }
        if options.has("-Z") {
            self.state.toggle_zoom(pane)?;
            let window = self
                .state
                .window_for_pane(pane)
                .expect("resolved pane remains in its window");
            let session = self.state.windows[&window].session;
            if self.state.sessions[&session].active_window == window {
                let active = self.state.windows[&window].active_pane;
                let target = ExecutionContext::for_pane(&self.state, active)
                    .expect("zoomed window has an active pane context");
                context.retarget(&target);
            }
            return Ok(Execution::default());
        }
        let window = self
            .state
            .window_for_pane(pane)
            .expect("resolved pane has a window");
        if !self.state.windows[&window].shows_floating(pane)
            && self.state.windows[&window].zoomed_pane.is_some()
        {
            self.state
                .windows
                .get_mut(&mut self.state.journal, &window)
                .unwrap()
                .unzoom();
            self.state.bump_generation();
        }
        if self.state.windows[&window].is_floating(pane) {
            return self.resize_floating_pane(&options, &positional, window, pane);
        }
        let mut absolute = Vec::new();
        for (option, axis, dimension) in [
            ("-x", Axis::Horizontal, "width"),
            ("-y", Axis::Vertical, "height"),
        ] {
            let Some(value) = options.value(option) else {
                continue;
            };
            let window = self
                .state
                .window_for_pane(pane)
                .expect("resolved pane has a window");
            let extent = self
                .window_extent(window, axis)
                .expect("resolved window has a cell extent");
            let cells = parse_resize_size(value, extent, dimension)?;
            absolute.push((axis, cells));
        }
        let mut relative = Vec::new();
        for (option, axis, sign) in [
            ("-L", Axis::Horizontal, -1),
            ("-R", Axis::Horizontal, 1),
            ("-U", Axis::Vertical, -1),
            ("-D", Axis::Vertical, 1),
        ] {
            let attached = options
                .value(option)
                .map(parse_resize_adjustment)
                .transpose()?;
            if attached.is_none() && !options.has(option) {
                continue;
            }
            let cells = match attached {
                Some(cells) => cells,
                None => positional
                    .first()
                    .map(|value| parse_resize_adjustment(value))
                    .transpose()?
                    .unwrap_or(1),
            };
            relative.push((axis, cells.saturating_mul(sign)));
        }
        for (axis, cells) in absolute {
            self.state.resize_pane_to(pane, axis, cells)?;
        }
        for (axis, cells) in relative {
            self.state.resize_pane(pane, axis, cells)?;
        }
        Ok(Execution::default())
    }

    fn resize_floating_pane(
        &mut self,
        options: &Options,
        positional: &[RawText],
        window: WindowId,
        pane: PaneId,
    ) -> Result<Execution, ServerError> {
        let border = self.pane_lines(pane) != PaneBorderLines::None;
        let minimum = i64::from(PANE_MINIMUM);
        let maximum = i64::from(PANE_MAXIMUM);
        for (option, axis, dimension) in [
            ("-x", Axis::Horizontal, "width"),
            ("-y", Axis::Vertical, "height"),
        ] {
            let Some(value) = options.value(option) else {
                continue;
            };
            let extent = self
                .window_extent(window, axis)
                .expect("resolved window has a cell extent");
            let mut size = i64::from(parse_resize_size(value, extent, dimension)?);
            let mut cell = self.state.windows[&window]
                .layout
                .pane_geometry(pane)
                .expect("floating pane has a cell");
            if axis == Axis::Vertical
                && match self.pane_border_status(window) {
                    PaneBorderStatus::Top => cell.yoff == 1,
                    PaneBorderStatus::Bottom => {
                        cell.yoff + i32::from(cell.sy) == i32::from(extent) - 1
                    }
                    PaneBorderStatus::Off => false,
                }
            {
                size += 1;
            }
            if border && size >= minimum + 2 {
                size -= 2;
            }
            if !(minimum..=maximum).contains(&size) {
                return Err(ServerError::InvalidCommand(
                    "size size is too big or too small".to_owned(),
                ));
            }
            let size = u16::try_from(size).expect("pane size is bounded");
            match axis {
                Axis::Horizontal => cell.sx = size,
                Axis::Vertical => cell.sy = size,
            }
            self.state.set_float_geometry(pane, cell)?;
        }
        let mut opposite = false;
        for (option, axis) in [
            ("-U", Axis::Vertical),
            ("-D", Axis::Vertical),
            ("-L", Axis::Horizontal),
            ("-R", Axis::Horizontal),
        ] {
            let value = match options.value(option) {
                Some(value) => value,
                None if options.has(option) => positional.first().map_or("1", |value| value),
                None => continue,
            };
            let change = parse_strtonum(
                value,
                i64::from(i32::MIN),
                i64::from(i32::MAX),
                "adjustment",
            )?;
            if option == "-L" || option == "-U" {
                opposite = true;
            }
            if change == 0 {
                continue;
            }
            let mut cell = self.state.windows[&window]
                .layout
                .pane_geometry(pane)
                .expect("floating pane has a cell");
            let current = match axis {
                Axis::Horizontal => cell.sx,
                Axis::Vertical => cell.sy,
            };
            let size = i64::from(current) + change;
            if !(minimum..=maximum).contains(&size) {
                return Err(ServerError::InvalidCommand(
                    "adjustment change is too big or too small".to_owned(),
                ));
            }
            let size = u16::try_from(size).expect("pane size is bounded");
            let change = i32::try_from(change).expect("adjustment fits i32");
            match axis {
                Axis::Horizontal => {
                    cell.sx = size;
                    if opposite {
                        cell.xoff -= change;
                    }
                }
                Axis::Vertical => {
                    cell.sy = size;
                    if opposite {
                        cell.yoff -= change;
                    }
                }
            }
            self.state.set_float_geometry(pane, cell)?;
        }
        Ok(Execution::default())
    }

    fn resize_pane_from_mouse(
        &mut self,
        context: &ExecutionContext,
        pane: PaneId,
    ) -> Result<Execution, ServerError> {
        let Some(mouse) = context.invoking_mouse().cloned() else {
            return Ok(Execution::default());
        };
        if let Some(drag) = mouse.drag {
            let alive = match drag.kind {
                MouseDragKind::NewPane => self.new_pane_mouse_resize(drag.pane, &mouse)?,
                MouseDragKind::MovePane => self.move_float_from_mouse(drag.pane, &mouse)?,
                MouseDragKind::ResizeFloat => self.resize_float_from_mouse(drag.pane, &mouse)?,
                MouseDragKind::ResizeTiled => {
                    self.resize_tiled_from_mouse(drag.pane, &mouse)?;
                    true
                }
            };
            return Ok(Execution {
                effects: alive
                    .then(|| armed_mouse_drag(drag.kind, drag.pane, &mouse))
                    .into_iter()
                    .collect(),
                ..Execution::default()
            });
        }
        let floating = self
            .state
            .window_for_pane(pane)
            .is_some_and(|window| self.state.windows[&window].shows_floating(pane));
        let kind = if floating {
            self.state.select_pane_with_zoom(pane, true)?;
            self.resize_float_from_mouse(pane, &mouse)?;
            MouseDragKind::ResizeFloat
        } else {
            self.resize_tiled_from_mouse(pane, &mouse)?;
            MouseDragKind::ResizeTiled
        };
        Ok(Execution {
            effects: vec![armed_mouse_drag(kind, pane, &mouse)],
            ..Execution::default()
        })
    }

    fn resize_tiled_from_mouse(
        &mut self,
        pane: PaneId,
        mouse: &MouseEventTarget,
    ) -> Result<(), ServerError> {
        let Some(axis) = mouse.border else {
            return Ok(());
        };
        let Some(window) = self.state.window_for_pane(pane) else {
            return Ok(());
        };
        let Some(geometry) = self.state.windows[&window].layout.pane_geometry(pane) else {
            return Ok(());
        };
        let cells = match axis {
            Axis::Horizontal => i32::from(mouse.column) - geometry.xoff,
            Axis::Vertical => self.mouse_window_row(pane, mouse.row) - geometry.yoff,
        };
        let cells = u16::try_from(cells).ok();
        let Some(cells) = cells.filter(|cells| *cells > 0) else {
            return Ok(());
        };
        self.state.resize_pane_to(pane, axis, cells)?;
        Ok(())
    }

    fn mouse_window_row(&self, pane: PaneId, row: u16) -> i32 {
        let row = i32::from(row);
        let Some(window) = self.state.window_for_pane(pane) else {
            return row;
        };
        let session = self.state.windows[&window].session;
        let lines = i32::from(self.status_rows_for_session(Some(session)));
        if lines == 0 {
            return row;
        }
        if self.status_formats_for_session(Some(session)).position == crate::StatusPosition::Top {
            return if row >= lines { row - lines } else { row };
        }
        let at = i32::from(self.state.windows[&window].layout.extent().1);
        if row >= at { at - 1 } else { row }
    }

    fn mouse_cells(&self, pane: PaneId, mouse: &MouseEventTarget) -> [(i32, i32); 3] {
        let cell = |(x, y): (u16, u16)| (i32::from(x), self.mouse_window_row(pane, y));
        let (lx, ly) = mouse_last_cell(mouse);
        let (ax, ay) = mouse_anchor_cell(mouse);
        [
            cell((mouse.column, mouse.row)),
            cell((
                u16::try_from(lx).unwrap_or_default(),
                u16::try_from(ly).unwrap_or_default(),
            )),
            cell((
                u16::try_from(ax).unwrap_or_default(),
                u16::try_from(ay).unwrap_or_default(),
            )),
        ]
    }

    pub(super) fn mouse_float_geometry(&self, pane: PaneId) -> Option<CellGeometry> {
        let window = self.state.window_for_pane(pane)?;
        let window = &self.state.windows[&window];
        if !window.shows_floating(pane) {
            return None;
        }
        window.layout.pane_geometry(pane)
    }

    pub(super) fn move_float_from_mouse(
        &mut self,
        pane: PaneId,
        mouse: &MouseEventTarget,
    ) -> Result<bool, ServerError> {
        let Some(mut cell) = self.mouse_float_geometry(pane) else {
            return Ok(false);
        };
        let [(x, y), (lx, ly), _] = self.mouse_cells(pane, mouse);
        if x != lx || y != ly {
            cell.xoff += x - lx;
            cell.yoff += y - ly;
            self.state.set_float_geometry(pane, cell)?;
        }
        Ok(true)
    }

    fn resize_float_from_mouse(
        &mut self,
        pane: PaneId,
        mouse: &MouseEventTarget,
    ) -> Result<bool, ServerError> {
        let Some(cell) = self.mouse_float_geometry(pane) else {
            return Ok(false);
        };
        let [(x, y), (lx, ly), _] = self.mouse_cells(pane, mouse);
        let (sx, sy) = (i32::from(cell.sx), i32::from(cell.sy));
        let left = cell.xoff - 1;
        let right = cell.xoff + sx;
        let top = cell.yoff - 1;
        let bottom = cell.yoff + sy;
        let at_left = lx == left || lx == left + 1;
        let at_right = lx == right + 1 || lx == right;
        let next = if at_left && ly == top {
            Some(((sx + lx - x).max(1), (sy + ly - y).max(1), x + 1, y + 1))
        } else if at_right && ly == top {
            Some((
                (x - cell.xoff).max(1),
                (sy + ly - y).max(1),
                cell.xoff,
                y + 1,
            ))
        } else if at_left && ly == bottom {
            let new_sy = y - cell.yoff;
            (new_sy >= 1).then_some(((sx + lx - x).max(1), new_sy, x + 1, cell.yoff))
        } else if at_right && ly == bottom {
            Some((
                (x - cell.xoff).max(1),
                (y - cell.yoff).max(1),
                cell.xoff,
                cell.yoff,
            ))
        } else if lx == right {
            let new_sx = x - cell.xoff;
            (new_sx >= 1).then_some((new_sx, sy, cell.xoff, cell.yoff))
        } else if lx == left {
            let new_sx = sx + lx - x;
            (new_sx >= 1).then_some((new_sx, sy, x + 1, cell.yoff))
        } else if ly == bottom {
            let new_sy = y - cell.yoff;
            (new_sy >= 1).then_some((sx, new_sy, cell.xoff, cell.yoff))
        } else if ly == top {
            Some((sx, sy, cell.xoff + x - lx, y + 1))
        } else {
            None
        };
        if let Some((sx, sy, xoff, yoff)) = next {
            self.state.set_float_geometry(
                pane,
                CellGeometry {
                    sx: u16::try_from(sx).unwrap_or(u16::MAX),
                    sy: u16::try_from(sy).unwrap_or(u16::MAX),
                    xoff,
                    yoff,
                },
            )?;
        }
        Ok(true)
    }

    pub(super) fn new_pane_mouse_resize(
        &mut self,
        pane: PaneId,
        mouse: &MouseEventTarget,
    ) -> Result<bool, ServerError> {
        if self.mouse_float_geometry(pane).is_none() {
            return Ok(false);
        }
        let border = i32::from(self.pane_lines(pane) != PaneBorderLines::None);
        let [(x, y), _, (ax, ay)] = self.mouse_cells(pane, mouse);
        let span = |at: i32, anchor: i32| {
            if at >= anchor {
                (at - anchor + 1, anchor + border)
            } else {
                let size = anchor - at + 1;
                (size, anchor - size + 1 + border)
            }
        };
        let (sx, xoff) = span(x, ax);
        let (sy, yoff) = span(y, ay);
        let inner = |size: i32| {
            if border == 0 {
                size.max(1)
            } else if size <= 2 {
                1
            } else {
                size - 2
            }
        };
        self.state.set_float_geometry(
            pane,
            CellGeometry {
                sx: u16::try_from(inner(sx)).unwrap_or(u16::MAX),
                sy: u16::try_from(inner(sy)).unwrap_or(u16::MAX),
                xoff,
                yoff,
            },
        )?;
        Ok(true)
    }

    pub(super) fn select_layout(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
        command: &str,
    ) -> Result<Execution, ServerError> {
        self.state.legacy_layout_saves = context.legacy_layouts();
        let result = self.select_layout_with(context, args, command);
        self.state.legacy_layout_saves = false;
        result
    }

    fn select_layout_with(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
        command: &str,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options(command, args)?;
        if positional.len() > usize::from(command == "select-layout") {
            return Err(ServerError::CommandParse(format!(
                "{command} accepts {} layout names",
                usize::from(command == "select-layout")
            )));
        }

        let target = options.value("-t");
        let (window, pane) =
            if target.is_some_and(|target| target.starts_with('%') || target.contains('.')) {
                let pane = self.resolve_pane(target, context.window, context.pane)?;
                let window = self
                    .state
                    .window_for_pane(pane)
                    .expect("resolved pane has a window");
                (window, pane)
            } else {
                let window = self.resolve_window(target, context.session, context.window)?;
                (window, window_active_pane(&self.state, window)?)
            };

        if self
            .state
            .windows
            .get_mut(&mut self.state.journal, &window)
            .expect("window was resolved")
            .zoomed_pane
            .take()
            .is_some()
        {
            self.state.bump_generation();
        }
        let preset_options = self.preset_options_for_window(window);

        if command == "next-layout" || options.has("-n") {
            self.state.cycle_layout(window, 1, &preset_options)?;
        } else if command == "previous-layout" || options.has("-p") {
            self.state.cycle_layout(window, -1, &preset_options)?;
        } else if options.has("-E") {
            self.state.spread_layout(pane)?;
        } else if options.has("-o") {
            if !positional.is_empty() {
                return Err(ServerError::CommandParse(
                    "select-layout -o does not accept a layout name".to_owned(),
                ));
            }
            self.state.restore_previous_layout(window)?;
        } else if let Some(name) = positional.first() {
            if let Some(preset) = parse_layout_preset(name) {
                self.state.select_layout(window, preset, &preset_options)?;
            } else {
                self.state.select_layout_string(window, name)?;
            }
        } else if let Some(last) = self.state.last_layout(window)? {
            self.state.select_layout(window, last, &preset_options)?;
        }
        self.restore_manual_window_extents(Some(window));
        Ok(Execution::default())
    }

    pub(super) fn rotate_window(
        &mut self,
        context: &mut ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("rotate-window", args)?;
        if !positional.is_empty() {
            return Err(ServerError::CommandParse(
                "rotate-window does not accept positional arguments".to_owned(),
            ));
        }
        let window = self.resolve_window(options.value("-t"), context.session, context.window)?;
        let pane = self
            .state
            .rotate_window(window, options.has("-D"), options.has("-Z"))?;
        let target = ExecutionContext::for_pane(&self.state, pane)
            .expect("rotated window retains an active pane");
        context.retarget(&target);
        Ok(Execution::default())
    }

    pub(super) fn kill_pane(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("kill-pane", args)?;
        reject_positionals("kill-pane", &positional)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let window = self
            .state
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let session = self.state.windows[&window].session;
        if options.value("-f").is_some() && !options.has("-a") {
            return Err(ServerError::InvalidCommand(
                "-f only valid with -a".to_owned(),
            ));
        }
        let mut targets = if options.has("-a") {
            self.state.windows[&window]
                .pane_order()
                .iter()
                .copied()
                .filter(|candidate| *candidate != pane)
                .collect()
        } else {
            vec![pane]
        };
        if let Some(filter) = options.value("-f") {
            targets.retain(|target| {
                format_true(&expand_format_with_hooks(
                    filter,
                    self,
                    FormatContext {
                        session: Some(session),
                        window: Some(window),
                        pane: Some(*target),
                        active_session: context.session,
                        format_client: FormatClient::NoClient,
                        format_type: FormatType::Pane,
                    },
                    hooks,
                ))
            });
        }
        if targets.len() == self.state.windows[&window].panes.len()
            && self.renumber_windows_for_session(session)
        {
            self.state.validate_renumber_capacity(
                session,
                self.base_index_for_session(session),
                1,
            )?;
        }
        let mut panes = Vec::new();
        for target in targets {
            panes.extend(self.kill_pane_state(target)?);
        }
        if self.state.windows.contains_key(&window) {
            let active_pane = self.state.windows[&window].active_pane;
            self.refresh_automatic_window_name_for_pane(active_pane, hooks);
        } else {
            self.renumber_session_if_enabled(session)?;
        }
        Ok(Execution::effect(MuxEffect::PanesRemoved(panes)))
    }

    pub(super) fn respawn_pane(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("respawn-pane", args)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let pane_state = self
            .state
            .pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if !matches!(&pane_state.kind, PaneKind::Terminal) {
            return Err(ServerError::InvalidCommand(format!(
                "respawn pane failed: pane {pane} is not a terminal"
            )));
        }
        if !pane_state.dead && !pane_state.empty && !options.has("-k") {
            return Err(ServerError::InvalidCommand(format!(
                "respawn pane failed: pane {} still active",
                self.pane_target_description(pane)?
            )));
        }
        let (_, cwd) = spawn_cwd_source(
            self,
            &options,
            Some(pane),
            &PaneKind::Terminal,
            context.format_client(),
            hooks,
        );
        let environment = spawn_environment(&options);
        let command = shell_command_positional(&positional);
        let empty = options.has("-E");
        if empty {
            self.state.mark_pane_empty(pane)?;
        } else {
            self.state.revive_pane(pane)?;
        }
        Ok(Execution::effect(MuxEffect::PaneRespawned {
            pane,
            cwd,
            command,
            environment,
            empty,
        }))
    }

    pub(super) fn respawn_window(
        &mut self,
        context: &ExecutionContext,
        args: &[RawText],
        hooks: &mut impl StatusHooks,
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("respawn-window", args)?;
        let window = self.resolve_window(options.value("-t"), context.session, context.window)?;
        let window_state = self
            .state
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        if !options.has("-k")
            && window_state
                .panes
                .values()
                .any(|pane| !pane.dead && !pane.empty)
        {
            return Err(ServerError::InvalidCommand(format!(
                "respawn window failed: window {} still active",
                self.window_target_description(window)?
            )));
        }
        let pane = *window_state
            .pane_order()
            .first()
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        if !matches!(&window_state.panes[&pane].kind, PaneKind::Terminal) {
            return Err(ServerError::InvalidCommand(format!(
                "respawn window failed: pane {pane} is not a terminal"
            )));
        }
        let other_panes = window_state
            .pane_order()
            .iter()
            .copied()
            .filter(|candidate| *candidate != pane)
            .collect::<Vec<_>>();
        let (_, cwd) = spawn_cwd_source(
            self,
            &options,
            Some(pane),
            &PaneKind::Terminal,
            context.format_client(),
            hooks,
        );
        let environment = spawn_environment(&options);
        let command = shell_command_positional(&positional);
        let empty = options.has("-E");
        let mut removed = Vec::new();
        for other in other_panes {
            removed.extend(self.kill_pane_state(other)?);
        }
        if empty {
            self.state.mark_pane_empty(pane)?;
        } else {
            self.state.revive_pane(pane)?;
        }
        let mut effects = Vec::with_capacity(2);
        if !removed.is_empty() {
            effects.push(MuxEffect::PanesRemoved(removed));
        }
        effects.push(MuxEffect::PaneRespawned {
            pane,
            cwd,
            command,
            environment,
            empty,
        });
        Ok(Execution {
            output: RawText::default(),
            effects,
        })
    }
}

fn parse_layout_preset(value: &str) -> Option<LayoutPreset> {
    if let Some(exact) = LayoutPreset::ALL
        .into_iter()
        .find(|preset| preset.name() == value)
    {
        return Some(exact);
    }
    let mut matches = LayoutPreset::ALL
        .into_iter()
        .filter(|preset| preset.name().starts_with(value));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}
