use super::*;

impl MuxState {
    pub fn create_window(
        &mut self,
        session: SessionId,
        name: Option<String>,
        kind: PaneKind,
    ) -> Result<(WindowId, PaneId), ServerError> {
        self.create_window_at(session, None, name, kind, true)
    }

    pub fn create_window_at(
        &mut self,
        session: SessionId,
        index: Option<u32>,
        name: Option<String>,
        kind: PaneKind,
        activate: bool,
    ) -> Result<(WindowId, PaneId), ServerError> {
        self.create_window_at_with_base_index(session, index, name, kind, activate, 0)
    }

    pub(crate) fn create_window_at_with_base_index(
        &mut self,
        session: SessionId,
        index: Option<u32>,
        name: Option<String>,
        kind: PaneKind,
        activate: bool,
        base_index: u32,
    ) -> Result<(WindowId, PaneId), ServerError> {
        let extent = self
            .session_active_window_extent(session)
            .unwrap_or(DEFAULT_WINDOW_EXTENT);
        let index = self.claim_window_index(session, index, base_index)?;
        let window_id = self.allocate_window_id();
        let pane_id = self.allocate_pane_id();
        let created = self.allocate_sort_point();
        let activity_time = self.format_time();
        let active_point = self.allocate_sort_point();
        let pane = Pane {
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
        };
        let window = Window {
            id: window_id,
            session,
            index,
            name: name.unwrap_or_else(|| index.to_string()),
            created,
            activity: created,
            activity_time,
            activity_flag: false,
            silence_flag: false,
            active_pane: pane_id,
            zoomed_pane: None,
            layout: CellLayout::new(pane_id, extent.0, extent.1),
            panes: BTreeMap::from([(pane_id, pane)]),
            pane_order: vec![pane_id],
            z_order: vec![pane_id],
            last_panes: Vec::new(),
            last_layout: None,
            previous_layout: None,
            last_extent_probe: None,
            manual_extent: extent,
            floats: WindowFloats::default(),
            input_options: InputOptions::default(),
        };
        self.windows.insert(&mut self.journal, window_id, window);
        self.sessions
            .get_mut(&mut self.journal, &session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?
            .windows
            .push(window_id);
        if activate {
            self.activate_window(session, window_id);
        }
        self.sort_session_windows(session);
        self.bump_generation();
        Ok((window_id, pane_id))
    }

    pub(super) fn session_active_window_extent(&self, session: SessionId) -> Option<(u16, u16)> {
        let window = self.sessions.get(&session)?.active_window;
        Some(self.windows.get(&window)?.layout.extent())
    }

    pub(super) fn claim_window_index(
        &self,
        session: SessionId,
        index: Option<u32>,
        base_index: u32,
    ) -> Result<u32, ServerError> {
        let Some(index) = index else {
            return self.next_window_index(session, base_index);
        };
        if self.window_at_index(session, index).is_some() {
            return Err(ServerError::InvalidCommand(format!(
                "index in use: {index}"
            )));
        }
        if self.sessions.contains_key(&session) {
            Ok(index)
        } else {
            Err(ServerError::MissingTarget(session.to_string()))
        }
    }

    #[must_use]
    pub fn window_at_index(&self, session: SessionId, index: u32) -> Option<WindowId> {
        self.window_matching(session, |window| window.index == index)
    }

    #[must_use]
    pub fn window_named(&self, session: SessionId, name: &str) -> Option<WindowId> {
        self.window_matching(session, |window| window.name == name)
    }

    fn window_matching(
        &self,
        session: SessionId,
        predicate: impl Fn(&Window) -> bool,
    ) -> Option<WindowId> {
        self.sessions
            .get(&session)?
            .windows
            .iter()
            .copied()
            .find(|window| self.windows.get(window).is_some_and(&predicate))
    }

    pub fn shift_windows_up(&mut self, session: SessionId, index: u32) -> Result<(), ServerError> {
        let state = self
            .sessions
            .get(&session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?;
        let used = state
            .windows
            .iter()
            .filter_map(|window| self.windows.get(window).map(|window| window.index))
            .collect::<BTreeSet<_>>();
        let mut free = index;
        while used.contains(&free) {
            free = free
                .checked_add(1)
                .ok_or_else(|| ServerError::InvalidCommand("no free window index".to_owned()))?;
        }
        let moved = state
            .windows
            .iter()
            .copied()
            .filter(|window| {
                self.windows
                    .get(window)
                    .is_some_and(|window| (index..free).contains(&window.index))
            })
            .collect::<Vec<_>>();
        if moved.is_empty() {
            return Ok(());
        }
        for window in moved {
            self.windows
                .get_mut(&mut self.journal, &window)
                .expect("shifted window exists")
                .index += 1;
        }
        self.sort_session_windows(session);
        self.bump_generation();
        Ok(())
    }

    pub fn set_window_index(&mut self, window: WindowId, index: u32) -> Result<(), ServerError> {
        let session = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?
            .session;
        if self
            .window_at_index(session, index)
            .is_some_and(|occupant| occupant != window)
        {
            return Err(ServerError::InvalidCommand(format!(
                "index in use: {index}"
            )));
        }
        self.windows
            .get_mut(&mut self.journal, &window)
            .expect("window target was resolved")
            .index = index;
        self.sort_session_windows(session);
        self.bump_generation();
        Ok(())
    }

    pub fn move_window(
        &mut self,
        source: WindowId,
        destination_session: SessionId,
        destination_index: u32,
        kill: bool,
        select: bool,
    ) -> Result<Vec<PaneId>, ServerError> {
        let source_session = self
            .windows
            .get(&source)
            .ok_or_else(|| ServerError::MissingTarget(source.to_string()))?
            .session;
        let source_index = self.windows[&source].index;
        let source_was_active = self.sessions[&source_session].active_window == source;
        if !self.sessions.contains_key(&destination_session) {
            return Err(ServerError::MissingTarget(destination_session.to_string()));
        }
        let occupant = self.window_at_index(destination_session, destination_index);
        if occupant == Some(source) {
            return Err(ServerError::InvalidCommand(format!(
                "same index: {destination_index}"
            )));
        }
        if occupant.is_some() && !kill {
            return Err(ServerError::InvalidCommand(format!(
                "index in use: {destination_index}"
            )));
        }

        let force_select = occupant
            .is_some_and(|occupant| self.sessions[&destination_session].active_window == occupant);
        let mut removed_panes = Vec::new();
        if let Some(occupant) = occupant {
            let removed = self
                .windows
                .remove(&mut self.journal, &occupant)
                .expect("destination occupant exists");
            removed_panes.extend(removed.pane_order);
            self.sessions
                .get_mut(&mut self.journal, &destination_session)
                .expect("destination session exists")
                .forget_window(occupant);
        }

        let source_fallback = if source_was_active {
            let session = &self.sessions[&source_session];
            session
                .last_window()
                .filter(|window| *window != source && session.windows.contains(window))
                .or_else(|| {
                    let candidates = session
                        .windows
                        .iter()
                        .copied()
                        .filter_map(|window| {
                            if window == source {
                                (source_session == destination_session)
                                    .then_some((window, destination_index))
                            } else {
                                Some((window, self.windows[&window].index))
                            }
                        })
                        .collect::<Vec<_>>();
                    candidates
                        .iter()
                        .filter(|(_, index)| *index < source_index)
                        .max_by_key(|(_, index)| *index)
                        .or_else(|| candidates.iter().max_by_key(|(_, index)| *index))
                        .map(|(window, _)| *window)
                })
        } else {
            None
        };
        if source_session != destination_session {
            let source_state = self
                .sessions
                .get_mut(&mut self.journal, &source_session)
                .expect("source session exists");
            source_state.forget_window(source);
            if let Some(fallback) = source_fallback {
                source_state.active_window = fallback;
                source_state.last_window = None;
            }
        }
        {
            let window = self
                .windows
                .get_mut(&mut self.journal, &source)
                .expect("source window exists");
            window.session = destination_session;
            window.index = destination_index;
        }
        if source_session != destination_session {
            self.sessions
                .get_mut(&mut self.journal, &destination_session)
                .expect("destination session exists")
                .windows
                .push(source);
        }

        let destination_was_empty = self.sessions[&destination_session].windows.len() == 1
            && self.sessions[&destination_session].windows[0] == source;
        if destination_was_empty {
            let destination = self
                .sessions
                .get_mut(&mut self.journal, &destination_session)
                .expect("destination session exists");
            destination.active_window = source;
            destination.last_window = None;
            self.force_activate_window(destination_session, source);
        } else if select || force_select {
            self.force_activate_window(destination_session, source);
        } else if source_session == destination_session
            && source_was_active
            && let Some(fallback) = source_fallback
        {
            let session = self
                .sessions
                .get_mut(&mut self.journal, &source_session)
                .expect("source session exists");
            session.active_window = fallback;
            session.last_window = None;
            self.touch_window_activity(fallback);
            self.clear_window_alerts(fallback);
        }

        if source_session != destination_session {
            let source_empty = self.sessions[&source_session].windows.is_empty();
            if source_empty {
                self.sessions.remove(&mut self.journal, &source_session);
            } else if let Some(fallback) = source_fallback {
                self.touch_window_activity(fallback);
                self.clear_window_alerts(fallback);
            }
        }
        self.sort_session_windows(destination_session);
        if source_session != destination_session && self.sessions.contains_key(&source_session) {
            self.sort_session_windows(source_session);
        }
        self.bump_generation();
        Ok(removed_panes)
    }

    pub fn swap_windows(
        &mut self,
        source: WindowId,
        target: WindowId,
        select_destination: bool,
    ) -> Result<(), ServerError> {
        if source == target {
            return Ok(());
        }
        let source_state = self
            .windows
            .get(&source)
            .ok_or_else(|| ServerError::MissingTarget(source.to_string()))?;
        let target_state = self
            .windows
            .get(&target)
            .ok_or_else(|| ServerError::MissingTarget(target.to_string()))?;
        let source_session = source_state.session;
        let source_index = source_state.index;
        let target_session = target_state.session;
        let target_index = target_state.index;

        if source_session == target_session {
            let session = self
                .sessions
                .get_mut(&mut self.journal, &source_session)
                .expect("window session exists");
            session.active_window = swap_window_id(session.active_window, source, target);
            session.last_window = session
                .last_window
                .map(|window| swap_window_id(window, source, target));
        } else {
            let source_state = self
                .sessions
                .get_mut(&mut self.journal, &source_session)
                .expect("source session exists");
            for window in &mut source_state.windows {
                if *window == source {
                    *window = target;
                }
            }
            if source_state.active_window == source {
                source_state.active_window = target;
            }
            if source_state.last_window == Some(source) {
                source_state.last_window = Some(target);
            }

            let target_state = self
                .sessions
                .get_mut(&mut self.journal, &target_session)
                .expect("target session exists");
            for window in &mut target_state.windows {
                if *window == target {
                    *window = source;
                }
            }
            if target_state.active_window == target {
                target_state.active_window = source;
            }
            if target_state.last_window == Some(target) {
                target_state.last_window = Some(source);
            }
        }

        {
            let source_state = self
                .windows
                .get_mut(&mut self.journal, &source)
                .expect("source window exists");
            source_state.session = target_session;
            source_state.index = target_index;
        }
        {
            let target_state = self
                .windows
                .get_mut(&mut self.journal, &target)
                .expect("target window exists");
            target_state.session = source_session;
            target_state.index = source_index;
        }
        self.sort_session_windows(source_session);
        if source_session != target_session {
            self.sort_session_windows(target_session);
        }
        if select_destination {
            self.activate_window(target_session, source);
            if source_session != target_session {
                self.activate_window(source_session, target);
            }
        }
        self.bump_generation();
        Ok(())
    }

    pub fn swap_windows_keeping_current(
        &mut self,
        current: WindowId,
        other: WindowId,
    ) -> Result<(), ServerError> {
        let sessions = [current, other]
            .into_iter()
            .filter_map(|window| self.windows.get(&window).map(|window| window.session))
            .collect::<Vec<_>>();
        let held = sessions
            .iter()
            .filter_map(|session| {
                self.sessions
                    .get(session)
                    .map(|state| (*session, state.active_window))
            })
            .collect::<Vec<_>>();
        self.swap_windows(current, other, false)?;
        for (session, active) in held {
            if let Some(state) = self.sessions.get_mut(&mut self.journal, &session) {
                state.active_window = active;
                if active == current {
                    state.last_window = Some(other);
                } else if active == other {
                    state.last_window = Some(current);
                }
            }
        }
        Ok(())
    }

    pub fn renumber_windows(
        &mut self,
        session: SessionId,
        base_index: u32,
    ) -> Result<(), ServerError> {
        let mut windows = self
            .sessions
            .get(&session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?
            .windows
            .clone();
        windows.sort_by_key(|window| self.windows.get(window).map(|window| window.index));
        validate_window_index_run(base_index, windows.len())?;

        let mut assignments = Vec::with_capacity(windows.len());
        for (position, window) in windows.iter().copied().enumerate() {
            let index = base_index
                .checked_add(u32::try_from(position).expect("validated window count fits u32"))
                .expect("validated window index run fits u32");
            assignments.push((window, index));
        }

        let changed = assignments
            .iter()
            .any(|(window, index)| self.windows[window].index != *index);
        for (window, index) in assignments {
            self.windows
                .get_mut(&mut self.journal, &window)
                .expect("renumbered window exists")
                .index = index;
        }
        self.sessions
            .get_mut(&mut self.journal, &session)
            .expect("renumbered session exists")
            .windows = windows;
        if changed {
            self.bump_generation();
        }
        Ok(())
    }

    pub(super) fn sort_session_windows(&mut self, session: SessionId) {
        let Some(state) = self.sessions.get(&session) else {
            return;
        };
        let mut windows = state.windows.clone();
        windows.sort_by_key(|window| self.windows.get(window).map(|window| window.index));
        self.sessions
            .get_mut(&mut self.journal, &session)
            .expect("session was just read")
            .windows = windows;
    }

    pub fn rename_window(
        &mut self,
        window: WindowId,
        name: impl Into<String>,
    ) -> Result<(), ServerError> {
        let name = name.into();
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        if window.name != name {
            window.name = name;
            self.bump_generation();
        }
        Ok(())
    }

    pub fn kill_window(&mut self, window: WindowId) -> Result<Vec<PaneId>, ServerError> {
        let removed = self
            .windows
            .remove(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        let removed_panes = removed.pane_order.clone();
        let (activated, session_empty) = {
            let session = self
                .sessions
                .get_mut(&mut self.journal, &removed.session)
                .expect("window session exists");
            let activated = session.forget_window(window);
            (activated, session.windows.is_empty())
        };
        if session_empty {
            self.sessions.remove(&mut self.journal, &removed.session);
        } else if let Some(window) = activated {
            self.touch_window_activity(window);
            self.clear_window_alerts(window);
        }
        self.bump_generation();
        Ok(removed_panes)
    }

    pub fn select_window(
        &mut self,
        session: SessionId,
        window: WindowId,
    ) -> Result<(), ServerError> {
        let session_state = self
            .sessions
            .get(&session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?;
        if !session_state.windows.contains(&window) {
            return Err(ServerError::MissingTarget(window.to_string()));
        }
        self.activate_window(session, window);
        self.bump_generation();
        Ok(())
    }

    pub fn validate_renumber_capacity(
        &self,
        session: SessionId,
        base_index: u32,
        removed_windows: usize,
    ) -> Result<(), ServerError> {
        let count = self
            .sessions
            .get(&session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?
            .windows
            .len()
            .checked_sub(removed_windows)
            .ok_or_else(|| ServerError::Internal("invalid removed window count".to_owned()))?;
        validate_window_index_run(base_index, count)
    }

    pub(super) fn activate_window(&mut self, session: SessionId, window: WindowId) -> bool {
        let changed = self
            .sessions
            .get_mut(&mut self.journal, &session)
            .expect("session exists")
            .activate_window(window);
        if changed {
            self.touch_window_activity(window);
            self.clear_window_alerts(window);
        }
        changed
    }

    fn force_activate_window(&mut self, session: SessionId, window: WindowId) {
        self.sessions
            .get_mut(&mut self.journal, &session)
            .expect("session exists")
            .activate_window(window);
        self.touch_window_activity(window);
        self.clear_window_alerts(window);
    }

    pub(super) fn touch_window_activity(&mut self, window: WindowId) {
        let activity = self.allocate_sort_point();
        let activity_time = self.format_time();
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .expect("active window exists");
        window.activity = activity;
        window.activity_time = activity_time;
    }

    pub(super) fn clear_window_alerts(&mut self, window: WindowId) {
        let panes = self.windows[&window].pane_order.clone();
        for pane in panes {
            self.clear_pane_alerts(pane);
        }
        self.set_window_activity_flag(window, false);
        self.set_window_silence_flag(window, false);
    }

    pub fn set_window_activity_flag(&mut self, window: WindowId, raised: bool) -> bool {
        let Some(window_state) = self.windows.get_mut(&mut self.journal, &window) else {
            return false;
        };
        if window_state.activity_flag == raised {
            return false;
        }
        window_state.activity_flag = raised;
        self.bump_generation();
        true
    }

    pub fn set_window_silence_flag(&mut self, window: WindowId, raised: bool) -> bool {
        let Some(window_state) = self.windows.get_mut(&mut self.journal, &window) else {
            return false;
        };
        if window_state.silence_flag == raised {
            return false;
        }
        window_state.silence_flag = raised;
        self.bump_generation();
        true
    }

    pub fn last_window(&self, session: SessionId) -> Result<WindowId, ServerError> {
        let session = self
            .sessions
            .get(&session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?;
        session
            .last_window()
            .filter(|window| session.windows.contains(window))
            .ok_or_else(|| ServerError::InvalidCommand("no last window".to_owned()))
    }

    pub(crate) fn next_window_index(
        &self,
        session: SessionId,
        base_index: u32,
    ) -> Result<u32, ServerError> {
        if base_index > MAX_WINDOW_INDEX {
            return Err(ServerError::InvalidCommand(
                "no free window index".to_owned(),
            ));
        }
        let session = self
            .sessions
            .get(&session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?;
        let used = session
            .windows
            .iter()
            .map(|window| self.windows[window].index)
            .collect::<BTreeSet<_>>();
        (base_index..=MAX_WINDOW_INDEX)
            .chain(0..base_index)
            .find(|index| !used.contains(index))
            .ok_or_else(|| ServerError::InvalidCommand("no free window index".to_owned()))
    }
}

fn swap_window_id(value: WindowId, source: WindowId, target: WindowId) -> WindowId {
    if value == source {
        target
    } else if value == target {
        source
    } else {
        value
    }
}

fn validate_window_index_run(base_index: u32, count: usize) -> Result<(), ServerError> {
    let last_offset = u32::try_from(count.saturating_sub(1))
        .ok()
        .filter(|offset| {
            base_index
                .checked_add(*offset)
                .is_some_and(|last| last <= MAX_WINDOW_INDEX)
        });
    if base_index > MAX_WINDOW_INDEX || (count != 0 && last_offset.is_none()) {
        return Err(ServerError::InvalidCommand(
            "no free window index".to_owned(),
        ));
    }
    Ok(())
}
