use super::*;

impl MuxState {
    #[must_use]
    pub fn pane_base_index(&self, window: WindowId) -> u32 {
        self.window_pane_base_indices
            .get(&window)
            .copied()
            .unwrap_or(self.global_pane_base_index)
    }

    pub fn split_pane(
        &mut self,
        target: PaneId,
        axis: Axis,
        kind: PaneKind,
    ) -> Result<PaneId, ServerError> {
        self.split_pane_with(target, axis, kind, SplitPlacement::default())
    }

    pub fn split_pane_with(
        &mut self,
        target: PaneId,
        axis: Axis,
        kind: PaneKind,
        placement: SplitPlacement,
    ) -> Result<PaneId, ServerError> {
        let window_id = self
            .window_for_pane(target)
            .ok_or_else(|| ServerError::MissingTarget(target.to_string()))?;
        let pane_id = PaneId(self.next_pane_id);
        let active_point = if placement.detached {
            0
        } else {
            self.allocate_sort_point()
        };
        let next_pane_id = &mut self.next_pane_id;
        let next_split_id = &mut self.next_split_id;
        let mut ids = || {
            let id = SplitId(*next_split_id);
            *next_split_id = (*next_split_id).saturating_add(1);
            id
        };
        let window = self
            .windows
            .get_mut(&mut self.journal, &window_id)
            .expect("window exists");
        window
            .layout
            .split(
                target,
                axis,
                placement.size,
                placement.before,
                placement.full_size,
                pane_id,
                &mut ids,
            )
            .map_err(|error| split_layout_error(error, target))?;
        *next_pane_id = (*next_pane_id).saturating_add(1);
        window.panes.insert(
            pane_id,
            Pane {
                id: pane_id,
                title: pane_title(&kind, &self.default_pane_title),
                kind,
                active_point,
                bell: false,
                status: None,
                summary: PaneSummary::default(),
                dead: false,
                dead_status: None,
                dead_time: None,
                input_off: false,
                screen_extent: None,
                empty: false,
                title_pinned: false,
                over_zoom: false,
                input_options: InputOptions::default(),
            },
        );
        insert_pane_order(
            &mut window.pane_order,
            pane_id,
            target,
            placement.before,
            placement.full_size,
        );
        window.z_order.push(pane_id);
        if !placement.detached {
            activate_window_pane(window, pane_id, false);
        }
        window.clear_pane_screen_extents();
        self.bump_generation();
        Ok(pane_id)
    }

    pub(crate) fn set_last_new_pane(&mut self, window: WindowId, cascade: (i32, i32)) {
        if let Some(window) = self.windows.get_mut(&mut self.journal, &window) {
            window.floats.last_new_pane = cascade;
        }
    }

    pub fn kill_pane(&mut self, pane: PaneId) -> Result<Vec<PaneId>, ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if self.windows[&window_id].panes.len() <= 1 {
            return self.kill_window(window_id);
        }
        let window = self
            .windows
            .get_mut(&mut self.journal, &window_id)
            .expect("window exists");
        let over_zoom = window.is_over_zoom(pane);
        let previous_zoom = window.zoomed_pane;
        let pushed = window.push_zoom(false, over_zoom);
        match window.layout.remove(pane) {
            Ok(()) => {}
            Err(LayoutError::LastPane) => return self.kill_window(window_id),
            Err(error) => {
                window.zoomed_pane = previous_zoom;
                return Err(pane_layout_error(error, pane));
            }
        }
        window.panes.remove(&pane);
        repair_window_after_pane_removal(window, pane);
        window.pop_zoom(pushed);
        self.journal.note_removal();
        self.bump_generation();
        Ok(vec![pane])
    }

    pub fn select_pane(&mut self, pane: PaneId) -> Result<(), ServerError> {
        self.select_pane_with_zoom(pane, false).map(|_| ())
    }

    pub fn select_pane_with_zoom(
        &mut self,
        pane: PaneId,
        preserve_zoom: bool,
    ) -> Result<bool, ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let active_point = self.allocate_sort_point();
        let window = self
            .windows
            .get_mut(&mut self.journal, &window_id)
            .expect("window exists");
        if window.active_pane == pane {
            return Ok(false);
        }
        let pane_changed = activate_window_pane(window, pane, preserve_zoom);
        if pane_changed {
            window
                .panes
                .get_mut(&pane)
                .expect("selected pane exists")
                .active_point = active_point;
            self.bump_generation();
        }
        Ok(pane_changed)
    }

    pub fn toggle_zoom(&mut self, pane: PaneId) -> Result<(), ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let active_point = self.allocate_sort_point();
        let window = self
            .windows
            .get_mut(&mut self.journal, &window_id)
            .expect("window exists");
        if window.panes.len() <= 1 {
            return Ok(());
        }
        window.clear_pane_screen_extents();
        let before = window.active_pane;
        if !window.unzoom() {
            window.zoom(pane);
        }
        if window.active_pane != before {
            let active = window.active_pane;
            window
                .panes
                .get_mut(&active)
                .expect("active pane exists")
                .active_point = active_point;
        }
        self.bump_generation();
        Ok(())
    }

    pub fn touch_window_activity_for_pane(&mut self, pane: PaneId) {
        let Some(window) = self.window_for_pane(pane) else {
            return;
        };
        self.touch_window_activity(window);
    }

    fn touch_pane_active_point(&mut self, pane: PaneId) {
        let Some(window) = self.window_for_pane(pane) else {
            return;
        };
        let active_point = self.allocate_sort_point();
        if let Some(pane) = self
            .windows
            .get_mut(&mut self.journal, &window)
            .and_then(|window| window.panes.get_mut(&pane))
        {
            pane.active_point = active_point;
        }
    }

    #[must_use]
    pub fn pane(&self, pane: PaneId) -> Option<&Pane> {
        self.windows
            .values()
            .find_map(|window| window.panes.get(&pane))
    }

    pub fn pane_mut(&mut self, pane: PaneId) -> Option<&mut Pane> {
        let window = self.window_for_pane(pane)?;
        self.windows
            .get_mut(&mut self.journal, &window)?
            .panes
            .get_mut(&pane)
    }

    pub fn mark_pane_dead(
        &mut self,
        pane: PaneId,
        status: Option<u32>,
    ) -> Result<bool, ServerError> {
        let pane = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if pane.dead && pane.dead_status == status && !pane.empty {
            return Ok(false);
        }
        pane.dead = true;
        pane.dead_status = status;
        pane.empty = false;
        self.bump_generation();
        Ok(true)
    }

    pub fn revive_pane(&mut self, pane: PaneId) -> Result<bool, ServerError> {
        let pane = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if !pane.dead
            && pane.dead_status.is_none()
            && pane.dead_time.is_none()
            && !pane.empty
            && pane.summary.command.is_none()
            && !pane.summary.unseen
        {
            return Ok(false);
        }
        pane.dead = false;
        pane.dead_status = None;
        pane.dead_time = None;
        pane.empty = false;
        pane.summary.command = None;
        pane.summary.unseen = false;
        self.bump_generation();
        Ok(true)
    }

    pub fn mark_pane_empty(&mut self, pane: PaneId) -> Result<bool, ServerError> {
        let pane = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if !pane.dead && pane.dead_status.is_none() && pane.dead_time.is_none() && pane.empty {
            return Ok(false);
        }
        pane.dead = false;
        pane.dead_status = None;
        pane.dead_time = None;
        pane.empty = true;
        self.bump_generation();
        Ok(true)
    }

    #[must_use]
    pub fn window_for_pane(&self, pane: PaneId) -> Option<WindowId> {
        self.windows
            .iter()
            .find_map(|(id, window)| window.panes.contains_key(&pane).then_some(*id))
    }

    #[must_use]
    pub fn cwd_donor(&self, target: PaneId) -> Option<PaneId> {
        let window = &self.windows[&self.window_for_pane(target)?];
        let has_cwd = |pane: &PaneId| match window.panes.get(pane).map(|pane| &pane.kind) {
            Some(PaneKind::Terminal) => true,
            Some(PaneKind::Agent(agent)) => agent.cwd.is_some(),
            _ => false,
        };
        if has_cwd(&target) {
            return Some(target);
        }
        std::iter::once(&window.active_pane)
            .chain(&window.last_panes)
            .chain(&window.pane_order)
            .find(|pane| **pane != target && has_cwd(pane))
            .copied()
    }

    pub fn pane_in_direction(
        &self,
        pane: PaneId,
        direction: PaneDirection,
    ) -> Result<Option<PaneId>, ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let window = &self.windows[&window_id];
        if window.has_floating_panes() {
            return Ok(pane_in_direction_with_floats(window, pane, direction));
        }
        let (window_columns, window_rows) = window.layout.extent();
        let rects = window
            .layout
            .tiled_panes()
            .into_iter()
            .map(|pane| {
                let geometry = window
                    .layout
                    .pane_geometry(pane)
                    .expect("layout order contains a pane geometry");
                let left = u16::try_from(geometry.xoff.max(0)).unwrap_or(u16::MAX);
                let top = u16::try_from(geometry.yoff.max(0)).unwrap_or(u16::MAX);
                let right = left
                    .saturating_add(geometry.sx)
                    .saturating_add(1)
                    .min(window_columns);
                let bottom = top
                    .saturating_add(geometry.sy)
                    .saturating_add(1)
                    .min(window_rows);
                (
                    pane,
                    PaneRect {
                        left: normalize_cell_coordinate(left, window_columns),
                        top: normalize_cell_coordinate(top, window_rows),
                        right: normalize_cell_coordinate(right, window_columns),
                        bottom: normalize_cell_coordinate(bottom, window_rows),
                    },
                )
            })
            .collect::<Vec<_>>();
        let current = rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == pane).then_some(*rect))
            .expect("validated layout contains every window pane");
        let candidates = directional_candidates(&rects, pane, current, direction);
        for recent in &window.last_panes {
            if candidates.contains(recent) {
                return Ok(Some(*recent));
            }
        }
        Ok(candidates.first().copied())
    }

    pub fn previous_pane(&self, pane: PaneId) -> Result<PaneId, ServerError> {
        self.pane_at_order_offset(pane, -1)
    }

    pub fn next_pane(&self, pane: PaneId) -> Result<PaneId, ServerError> {
        self.pane_at_order_offset(pane, 1)
    }

    pub fn next_tiled_pane(&self, pane: PaneId, step: isize) -> Result<PaneId, ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let window = &self.windows[&window_id];
        let order = &window.pane_order;
        let index = order
            .iter()
            .position(|candidate| *candidate == pane)
            .expect("validated pane order contains every window pane");
        let len = isize::try_from(order.len()).expect("pane count fits isize");
        let start = isize::try_from(index).expect("pane index fits isize");
        Ok((1..=len)
            .map(|distance| {
                order[usize::try_from((start + distance * step).rem_euclid(len))
                    .expect("wrapped index is nonnegative")]
            })
            .find(|candidate| !window.is_floating(*candidate))
            .unwrap_or(pane))
    }

    fn pane_at_order_offset(&self, pane: PaneId, offset: isize) -> Result<PaneId, ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let window = &self.windows[&window_id];
        let index = window
            .pane_order
            .iter()
            .position(|candidate| *candidate == pane)
            .expect("validated pane order contains every window pane");
        let len = isize::try_from(window.pane_order.len()).expect("pane count fits isize");
        let next =
            (isize::try_from(index).expect("pane index fits isize") + offset).rem_euclid(len);
        Ok(window.pane_order[usize::try_from(next).expect("wrapped pane index is nonnegative")])
    }

    pub fn rotate_window(
        &mut self,
        window: WindowId,
        reverse: bool,
        preserve_zoom: bool,
    ) -> Result<PaneId, ServerError> {
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        let active = window.active_pane;
        let was_zoomed = window.zoomed_pane.is_some();
        if window.pane_order.len() <= 1 {
            if was_zoomed && !preserve_zoom {
                window.zoomed_pane = None;
                self.bump_generation();
            }
            return Ok(active);
        }

        let previous_order = window.pane_order.clone();
        let mut next_order = previous_order.clone();
        if reverse {
            next_order.rotate_right(1);
        } else {
            next_order.rotate_left(1);
        }
        let replacements = previous_order
            .into_iter()
            .zip(next_order.iter().copied())
            .collect::<BTreeMap<_, _>>();
        window.layout.remap(&replacements);
        window.pane_order = next_order;
        let next_active = replacements[&active];
        let pane_changed = activate_window_pane(window, next_active, false);
        window.zoomed_pane = (preserve_zoom && was_zoomed).then_some(next_active);
        if pane_changed {
            self.touch_pane_active_point(next_active);
        }
        self.bump_generation();
        Ok(next_active)
    }

    pub fn swap_panes(
        &mut self,
        source: PaneId,
        target: PaneId,
        detached: bool,
        preserve_zoom: bool,
    ) -> Result<(), ServerError> {
        let source_window = self
            .window_for_pane(source)
            .ok_or_else(|| ServerError::MissingTarget(source.to_string()))?;
        let target_window = self
            .window_for_pane(target)
            .ok_or_else(|| ServerError::MissingTarget(target.to_string()))?;

        let pushed_target = self.push_zoom(target_window, false, preserve_zoom);
        if source == target {
            self.pop_zoom(target_window, pushed_target);
            return Ok(());
        }
        let pushed_source = (source_window != target_window)
            .then(|| self.push_zoom(source_window, false, preserve_zoom));
        if source_window == target_window {
            {
                let window = self
                    .windows
                    .get_mut(&mut self.journal, &source_window)
                    .expect("window exists");
                let swapped = window.layout.swap(source, target);
                debug_assert!(swapped);
                swap_pane_order(&mut window.pane_order, source, target);
                swap_pane_order(&mut window.z_order, source, target);
            }
            if detached {
                if self.windows[&source_window].active_pane == source {
                    let changed = activate_window_pane(
                        self.windows
                            .get_mut(&mut self.journal, &source_window)
                            .expect("window exists"),
                        target,
                        false,
                    );
                    if changed {
                        self.touch_pane_active_point(target);
                    }
                }
                if self.windows[&source_window].active_pane == target {
                    let changed = activate_window_pane(
                        self.windows
                            .get_mut(&mut self.journal, &source_window)
                            .expect("window exists"),
                        source,
                        false,
                    );
                    if changed {
                        self.touch_pane_active_point(source);
                    }
                }
            } else {
                let changed = activate_window_pane(
                    self.windows
                        .get_mut(&mut self.journal, &source_window)
                        .expect("window exists"),
                    target,
                    false,
                );
                if changed {
                    self.touch_pane_active_point(target);
                }
            }
            self.pop_zoom(source_window, pushed_target);
            self.bump_generation();
            return Ok(());
        }

        let source_active = self.windows[&source_window].active_pane;
        let target_active = self.windows[&target_window].active_pane;

        let mut source_state = self
            .windows
            .remove(&mut self.journal, &source_window)
            .expect("source window exists");
        let target_state = self
            .windows
            .get_mut(&mut self.journal, &target_window)
            .expect("target window exists");
        let source_replaced = source_state.layout.replace(source, target);
        debug_assert!(source_replaced);
        let target_replaced = target_state.layout.replace(target, source);
        debug_assert!(target_replaced);
        let source_pane = source_state
            .panes
            .remove(&source)
            .expect("source window contains source pane");
        let target_pane = target_state
            .panes
            .remove(&target)
            .expect("target window contains target pane");
        source_state.panes.insert(target, target_pane);
        target_state.panes.insert(source, source_pane);
        replace_pane_order(&mut source_state.pane_order, source, target);
        replace_pane_order(&mut target_state.pane_order, target, source);
        replace_pane_order(&mut source_state.z_order, source, target);
        replace_pane_order(&mut target_state.z_order, target, source);

        let next_source_active = if detached && source_active != source {
            source_active
        } else {
            target
        };
        let next_target_active = if detached && target_active != target {
            target_active
        } else {
            source
        };
        let source_changed =
            activate_relocated_window_pane(&mut source_state, next_source_active, source);
        let target_changed =
            activate_relocated_window_pane(target_state, next_target_active, target);
        self.windows
            .insert(&mut self.journal, source_window, source_state);
        if source_changed {
            self.touch_pane_active_point(next_source_active);
        }
        if target_changed {
            self.touch_pane_active_point(next_target_active);
        }
        if let Some(pushed_source) = pushed_source {
            self.pop_zoom(source_window, pushed_source);
        }
        self.pop_zoom(target_window, pushed_target);
        self.bump_generation();
        Ok(())
    }

    pub fn break_pane(
        &mut self,
        pane: PaneId,
        destination_session: SessionId,
        destination_index: Option<u32>,
        name: Option<String>,
        detached: bool,
    ) -> Result<WindowId, ServerError> {
        self.break_pane_with_base_index(
            pane,
            destination_session,
            destination_index,
            name,
            String::new(),
            detached,
            0,
        )
    }

    pub(crate) fn break_pane_with_base_index(
        &mut self,
        pane: PaneId,
        destination_session: SessionId,
        destination_index: Option<u32>,
        name: Option<String>,
        default_name: String,
        detached: bool,
        base_index: u32,
    ) -> Result<WindowId, ServerError> {
        let source_window = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if !self.sessions.contains_key(&destination_session) {
            return Err(ServerError::MissingTarget(destination_session.to_string()));
        }
        let source_session = self.windows[&source_window].session;
        let index = self.claim_window_index(destination_session, destination_index, base_index)?;
        if self.windows[&source_window].panes.len() == 1 {
            self.move_window(source_window, destination_session, index, false, !detached)?;
            if let Some(name) = name {
                self.windows
                    .get_mut(&mut self.journal, &source_window)
                    .expect("moved source window exists")
                    .name = name;
            }
            return Ok(source_window);
        }
        let inherited_extent = self
            .session_active_window_extent(destination_session)
            .unwrap_or(DEFAULT_WINDOW_EXTENT);
        let window_id = self.allocate_window_id();
        let created = self.allocate_sort_point();
        let activity_time = self.format_time();
        let mut source = self
            .windows
            .remove(&mut self.journal, &source_window)
            .expect("source window exists");
        let source_will_close = match source.layout.remove(pane) {
            Ok(()) => false,
            Err(LayoutError::LastPane) => true,
            Err(error) => {
                self.windows
                    .insert(&mut self.journal, source_window, source);
                return Err(pane_layout_error(error, pane));
            }
        };
        let pane_state = source
            .panes
            .remove(&pane)
            .expect("source window contains pane");
        let activated = if source_will_close {
            self.sessions
                .get_mut(&mut self.journal, &source_session)
                .expect("source session exists")
                .forget_window(source_window)
        } else {
            repair_window_after_pane_removal(&mut source, pane);
            self.windows
                .insert(&mut self.journal, source_window, source);
            None
        };
        let window_name = name.unwrap_or(default_name);
        self.windows.insert(
            &mut self.journal,
            window_id,
            Window {
                id: window_id,
                session: destination_session,
                index,
                name: window_name,
                created,
                activity: created,
                activity_time,
                activity_flag: false,
                silence_flag: false,
                active_pane: pane,
                zoomed_pane: None,
                layout: CellLayout::new(pane, inherited_extent.0, inherited_extent.1),
                panes: BTreeMap::from([(pane, pane_state)]),
                pane_order: vec![pane],
                z_order: vec![pane],
                last_panes: Vec::new(),
                last_layout: None,
                previous_layout: None,
                last_extent_probe: None,
                manual_extent: inherited_extent,
                floats: WindowFloats::default(),
                input_options: InputOptions::default(),
            },
        );
        let destination_was_empty = {
            let destination = self
                .sessions
                .get_mut(&mut self.journal, &destination_session)
                .expect("destination session exists");
            let was_empty = destination.windows.is_empty();
            destination.windows.push(window_id);
            if was_empty {
                destination.active_window = window_id;
                destination.last_window = None;
            }
            was_empty
        };
        if !destination_was_empty && !detached {
            self.activate_window(destination_session, window_id);
        }
        if let Some(window) = activated {
            self.touch_window_activity(window);
            self.clear_window_alerts(window);
        }
        if source_session != destination_session
            && self.sessions[&source_session].windows.is_empty()
        {
            self.sessions.remove(&mut self.journal, &source_session);
        }
        self.sort_session_windows(destination_session);
        self.bump_generation();
        Ok(window_id)
    }

    pub fn join_pane(
        &mut self,
        source: PaneId,
        target: PaneId,
        axis: Axis,
        size: SplitSize,
        before: bool,
        full_size: bool,
        detached: bool,
    ) -> Result<(), ServerError> {
        if source == target {
            return Err(ServerError::InvalidCommand(
                "source and target panes must be different".to_owned(),
            ));
        }
        let source_window = self
            .window_for_pane(source)
            .ok_or_else(|| ServerError::MissingTarget(source.to_string()))?;
        let target_window = self
            .window_for_pane(target)
            .ok_or_else(|| ServerError::MissingTarget(target.to_string()))?;
        let source_session = self.windows[&source_window].session;
        let target_session = self.windows[&target_window].session;
        if source_window != target_window
            && self.windows[&source_window].panes.len() == 1
            && source_session != target_session
            && self.sessions[&source_session].windows.len() == 1
        {
            return Err(ServerError::InvalidCommand(
                "cannot move the last window out of a session".to_owned(),
            ));
        }
        if source_window == target_window {
            let next_split_id = &mut self.next_split_id;
            let mut ids = || {
                let id = SplitId(*next_split_id);
                *next_split_id = (*next_split_id).saturating_add(1);
                id
            };
            let window = self
                .windows
                .get_mut(&mut self.journal, &source_window)
                .expect("window exists");
            let original_layout = window.layout.clone();
            window
                .layout
                .remove(source)
                .map_err(|error| pane_layout_error(error, source))?;
            if let Err(error) = window
                .layout
                .split(target, axis, size, before, full_size, source, &mut ids)
            {
                window.layout = original_layout;
                return Err(split_layout_error(error, target));
            }
            lose_window_pane(window, source);
            window.pane_order.retain(|pane| *pane != source);
            insert_pane_order(&mut window.pane_order, source, target, false, false);
            window.z_order.retain(|pane| *pane != source);
            insert_pane_order(&mut window.z_order, source, target, before, false);
            window.zoomed_pane = None;
            let pane_changed = !detached && activate_window_pane(window, source, false);
            normalize_window_history(window);
            if pane_changed {
                self.touch_pane_active_point(source);
            }
            self.bump_generation();
            return Ok(());
        }

        let mut source_state = self
            .windows
            .remove(&mut self.journal, &source_window)
            .expect("source window exists");
        let source_backup = source_state.clone();
        let source_will_close = match source_state.layout.remove(source) {
            Ok(()) => false,
            Err(LayoutError::LastPane) => true,
            Err(error) => {
                self.windows
                    .insert(&mut self.journal, source_window, source_state);
                return Err(pane_layout_error(error, source));
            }
        };
        let pane_state = source_state
            .panes
            .remove(&source)
            .expect("source window contains source pane");
        if !source_will_close {
            repair_window_after_pane_removal(&mut source_state, source);
        }

        let next_split_id = &mut self.next_split_id;
        let mut ids = || {
            let id = SplitId(*next_split_id);
            *next_split_id = (*next_split_id).saturating_add(1);
            id
        };
        let split_result = self
            .windows
            .get_mut(&mut self.journal, &target_window)
            .expect("target window exists")
            .layout
            .split(target, axis, size, before, full_size, source, &mut ids);
        if let Err(error) = split_result {
            self.windows
                .insert(&mut self.journal, source_window, source_backup);
            return Err(split_layout_error(error, target));
        }
        let target_state = self
            .windows
            .get_mut(&mut self.journal, &target_window)
            .expect("target window exists");
        target_state.panes.insert(source, pane_state);
        insert_pane_order(&mut target_state.pane_order, source, target, false, false);
        insert_pane_order(&mut target_state.z_order, source, target, before, false);
        target_state.zoomed_pane = None;
        let target_changed = !detached && activate_window_pane(target_state, source, false);
        normalize_window_history(target_state);
        if target_changed {
            self.touch_pane_active_point(source);
        }

        if !detached {
            self.activate_window(target_session, target_window);
        }

        let activated = if source_will_close {
            self.sessions
                .get_mut(&mut self.journal, &source_session)
                .expect("source session exists")
                .forget_window(source_window)
        } else {
            self.windows
                .insert(&mut self.journal, source_window, source_state);
            None
        };
        if let Some(window) = activated {
            self.touch_window_activity(window);
            self.clear_window_alerts(window);
        }
        self.bump_generation();
        Ok(())
    }

    pub fn last_pane(&self, window: WindowId) -> Result<PaneId, ServerError> {
        let window = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        if let Some(pane) = window.last_panes.first() {
            return Ok(*pane);
        }
        if window.panes.len() == 2 {
            return window
                .panes
                .keys()
                .find(|pane| **pane != window.active_pane)
                .copied()
                .ok_or_else(|| ServerError::InvalidCommand("no last pane".to_owned()));
        }
        Err(ServerError::InvalidCommand("no last pane".to_owned()))
    }

    pub(crate) fn pane_input_off(&self, pane: PaneId) -> Result<bool, ServerError> {
        self.pane(pane)
            .map(|target| target.input_off)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))
    }

    pub(crate) fn set_pane_input_off(
        &mut self,
        pane: PaneId,
        input_off: bool,
    ) -> Result<bool, ServerError> {
        let target = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if target.input_off == input_off {
            return Ok(false);
        }
        target.input_off = input_off;
        self.bump_generation();
        Ok(true)
    }
}

pub(super) fn split_layout_error(error: LayoutError, target: PaneId) -> ServerError {
    match error {
        LayoutError::NoSpace => ServerError::InvalidCommand("no space for a new pane".to_owned()),
        LayoutError::Floating => {
            ServerError::InvalidCommand("can't split a floating pane".to_owned())
        }
        LayoutError::UnknownPane => ServerError::MissingTarget(target.to_string()),
        LayoutError::LastPane | LayoutError::UnknownDivider => {
            ServerError::Internal(format!("unexpected split layout error: {error:?}"))
        }
    }
}

pub(super) fn activate_window_pane(window: &mut Window, pane: PaneId, preserve_zoom: bool) -> bool {
    if window.active_pane == pane {
        return false;
    }
    if window.floats.modal.is_some_and(|modal| modal.pane != pane) {
        return false;
    }
    debug_assert!(window.panes.contains_key(&pane));
    if window.layout.is_floating(pane) {
        window.raise(pane);
    }
    let previous = window.active_pane;
    let was_zoomed = window.zoomed_pane.is_some();
    let visible = window.pane_is_visible(pane);
    window
        .last_panes
        .retain(|candidate| *candidate != pane && *candidate != previous);
    window.last_panes.insert(0, previous);
    window
        .last_panes
        .truncate(window.panes.len().saturating_sub(1));
    window.active_pane = pane;
    if was_zoomed && !visible {
        window.unzoom();
        if preserve_zoom {
            window.zoom(pane);
        }
    }
    window.clear_pane_screen_extents();
    true
}

pub(super) fn apply_window_selection(
    window: &mut Window,
    active: Option<PaneId>,
    last_panes: Vec<PaneId>,
    active_point: Option<u64>,
) {
    if let (Some(active), Some(active_point)) = (active, active_point)
        && activate_window_pane(window, active, false)
    {
        window
            .panes
            .get_mut(&active)
            .expect("selected pane belongs to the window")
            .active_point = active_point;
    }
    window.last_panes = last_panes;
}

fn repair_window_after_pane_removal(window: &mut Window, pane: PaneId) {
    window.zoomed_pane = None;
    window.clear_pane_screen_extents();
    lose_window_pane(window, pane);
    window.pane_order.retain(|candidate| *candidate != pane);
    window.z_order.retain(|candidate| *candidate != pane);
    normalize_window_history(window);
}

fn lose_window_pane(window: &mut Window, pane: PaneId) {
    if let Some(modal) = &mut window.floats.modal
        && modal.last == Some(pane)
    {
        modal.last = None;
    }
    window.last_panes.retain(|candidate| *candidate != pane);
    let modal = window.floats.modal.filter(|modal| modal.pane == pane);
    if modal.is_some() {
        window.floats.modal = None;
    }
    if window.active_pane != pane {
        return;
    }
    let position = window
        .pane_order
        .iter()
        .position(|candidate| *candidate == pane)
        .expect("relocated pane belongs to the window");
    let next = modal
        .and_then(|modal| modal.last)
        .filter(|last| *last != pane && window.panes.contains_key(last))
        .or_else(|| window.last_panes.first().copied())
        .or_else(|| {
            position
                .checked_sub(1)
                .and_then(|position| window.pane_order.get(position).copied())
        })
        .or_else(|| window.pane_order.get(position + 1).copied())
        .expect("losing a pane leaves another pane");
    window.active_pane = next;
    window.last_panes.retain(|candidate| *candidate != next);
}

fn normalize_window_history(window: &mut Window) {
    let panes = &window.panes;
    window
        .last_panes
        .retain(|candidate| panes.contains_key(candidate));
}

pub(super) fn insert_pane_order(
    order: &mut Vec<PaneId>,
    pane: PaneId,
    target: PaneId,
    before: bool,
    full_size: bool,
) {
    debug_assert!(!order.contains(&pane));
    if full_size {
        if before {
            order.insert(0, pane);
        } else {
            order.push(pane);
        }
        return;
    }
    let target = order
        .iter()
        .position(|candidate| *candidate == target)
        .expect("validated pane order contains the target");
    order.insert(if before { target } else { target + 1 }, pane);
}

fn replace_pane_order(order: &mut [PaneId], pane: PaneId, replacement: PaneId) {
    let slot = order
        .iter_mut()
        .find(|candidate| **candidate == pane)
        .expect("validated pane order contains the replaced pane");
    *slot = replacement;
}

fn swap_pane_order(order: &mut [PaneId], first: PaneId, second: PaneId) {
    for pane in order {
        if *pane == first {
            *pane = second;
        } else if *pane == second {
            *pane = first;
        }
    }
}

fn activate_relocated_window_pane(window: &mut Window, pane: PaneId, outgoing: PaneId) -> bool {
    let previous = window.active_pane;
    window.last_panes.retain(|candidate| {
        *candidate != outgoing && *candidate != pane && window.panes.contains_key(candidate)
    });
    if previous != outgoing && previous != pane && window.panes.contains_key(&previous) {
        window.last_panes.retain(|candidate| *candidate != previous);
        window.last_panes.insert(0, previous);
    }
    window
        .last_panes
        .truncate(window.panes.len().saturating_sub(1));
    window.active_pane = pane;
    previous != pane
}

fn normalize_cell_coordinate(coordinate: u16, extent: u16) -> u32 {
    if extent == 0 {
        return 0;
    }
    let numerator = u64::from(coordinate) * u64::from(LAYOUT_COORDINATE_MAX);
    let rounded = (numerator + u64::from(extent) / 2) / u64::from(extent);
    u32::try_from(rounded).unwrap_or(LAYOUT_COORDINATE_MAX)
}

fn directional_candidates(
    rects: &[(PaneId, PaneRect)],
    pane: PaneId,
    current: PaneRect,
    direction: PaneDirection,
) -> Vec<PaneId> {
    rects
        .iter()
        .filter_map(|(candidate, rect)| {
            if *candidate == pane {
                return None;
            }
            let adjacent = match direction {
                PaneDirection::Left => {
                    let edge = if current.left == 0 {
                        LAYOUT_COORDINATE_MAX
                    } else {
                        current.left
                    };
                    rect.right == edge
                        && ranges_overlap(current.top, current.bottom, rect.top, rect.bottom)
                }
                PaneDirection::Right => {
                    let edge = if current.right == LAYOUT_COORDINATE_MAX {
                        0
                    } else {
                        current.right
                    };
                    rect.left == edge
                        && ranges_overlap(current.top, current.bottom, rect.top, rect.bottom)
                }
                PaneDirection::Up => {
                    let edge = if current.top == 0 {
                        LAYOUT_COORDINATE_MAX
                    } else {
                        current.top
                    };
                    rect.bottom == edge
                        && ranges_overlap(current.left, current.right, rect.left, rect.right)
                }
                PaneDirection::Down => {
                    let edge = if current.bottom == LAYOUT_COORDINATE_MAX {
                        0
                    } else {
                        current.bottom
                    };
                    rect.top == edge
                        && ranges_overlap(current.left, current.right, rect.left, rect.right)
                }
            };
            adjacent.then_some(*candidate)
        })
        .collect()
}

fn pane_in_direction_with_floats(
    window: &Window,
    pane: PaneId,
    direction: PaneDirection,
) -> Option<PaneId> {
    let (columns, rows) = window.layout.extent();
    let (columns, rows) = (i32::from(columns), i32::from(rows));
    let current = window.layout.pane_geometry(pane)?;
    let (xoff, yoff) = (current.xoff, current.yoff);
    let (sx, sy) = (i32::from(current.sx), i32::from(current.sy));
    let (edge, low, high) = match direction {
        PaneDirection::Up => (if yoff == 0 { rows + 1 } else { yoff }, xoff, xoff + sx),
        PaneDirection::Down => {
            let edge = yoff + sy + 1;
            (if edge >= rows { 0 } else { edge }, xoff, xoff + sx)
        }
        PaneDirection::Left => (if xoff == 0 { columns + 1 } else { xoff }, yoff, yoff + sy),
        PaneDirection::Right => {
            let edge = xoff + sx + 1;
            (if edge >= columns { 0 } else { edge }, yoff, yoff + sy)
        }
    };
    let mut best: Option<PaneId> = None;
    for candidate in &window.pane_order {
        if *candidate == pane {
            continue;
        }
        let Some(geometry) = window.layout.pane_geometry(*candidate) else {
            continue;
        };
        let (x, y) = (geometry.xoff, geometry.yoff);
        let (w, h) = (i32::from(geometry.sx), i32::from(geometry.sy));
        let (on_edge, start, size) = match direction {
            PaneDirection::Up => (y + h + 1 == edge, x, w),
            PaneDirection::Down => (y == edge, x, w),
            PaneDirection::Left => (x + w + 1 == edge, y, h),
            PaneDirection::Right => (x == edge, y, h),
        };
        let end = start + size - 1;
        let found = (start < low && end > high)
            || (start >= low && start <= high)
            || (end >= low && end <= high);
        if !on_edge || !found {
            continue;
        }
        let point = |pane: PaneId| window.panes.get(&pane).map_or(0, |pane| pane.active_point);
        if best.is_none_or(|best| point(*candidate) > point(best)) {
            best = Some(*candidate);
        }
    }
    best
}

fn ranges_overlap(first_start: u32, first_end: u32, second_start: u32, second_end: u32) -> bool {
    first_start.max(second_start) < first_end.min(second_end)
}
