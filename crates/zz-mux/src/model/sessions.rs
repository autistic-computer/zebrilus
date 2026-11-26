use super::*;

impl MuxState {
    #[must_use]
    pub fn session_working_directory(&self, session: SessionId) -> Option<&Path> {
        self.sessions
            .get(&session)
            .and_then(|session| session.working_directory.as_deref())
    }

    pub fn set_session_working_directory(
        &mut self,
        session: SessionId,
        working_directory: PathBuf,
    ) -> Result<bool, ServerError> {
        let state = self
            .sessions
            .get_mut(&mut self.journal, &session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?;
        if state.working_directory.as_ref() == Some(&working_directory) {
            return Ok(false);
        }
        state.working_directory = Some(working_directory);
        self.bump_generation();
        Ok(true)
    }

    pub fn create_session(
        &mut self,
        name: impl Into<String>,
    ) -> Result<(SessionId, WindowId, PaneId), ServerError> {
        self.create_session_with_extent(name, DEFAULT_WINDOW_EXTENT)
    }

    pub(crate) fn create_session_with_extent(
        &mut self,
        name: impl Into<String>,
        extent: (u16, u16),
    ) -> Result<(SessionId, WindowId, PaneId), ServerError> {
        self.create_session_with_extent_at(name, extent, 0)
    }

    pub(crate) fn create_session_with_extent_at(
        &mut self,
        name: impl Into<String>,
        extent: (u16, u16),
        index: u32,
    ) -> Result<(SessionId, WindowId, PaneId), ServerError> {
        let name = name.into();
        if self.sessions.values().any(|session| session.name == name) {
            return Err(ServerError::InvalidCommand(format!(
                "duplicate session: {name}"
            )));
        }
        let session_id = self.allocate_session_id();
        let window_id = self.allocate_window_id();
        let pane_id = self.allocate_pane_id();
        let created = self.allocate_sort_point();
        let activity_time = self.format_time();
        let active_point = self.allocate_sort_point();
        let pane = Pane {
            id: pane_id,
            title: pane_title(&PaneKind::Terminal, &self.default_pane_title),
            kind: PaneKind::Terminal,
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
            session: session_id,
            index,
            name: index.to_string(),
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
        self.sessions.insert(
            &mut self.journal,
            session_id,
            Session {
                id: session_id,
                name,
                working_directory: None,
                created: None,
                activity: None,
                sort_created: created,
                sort_activity: created,
                windows: vec![window_id],
                active_window: window_id,
                last_window: None,
            },
        );
        self.last_active_session = Some(session_id);
        self.bump_generation();
        Ok((session_id, window_id, pane_id))
    }

    pub fn rename_session(
        &mut self,
        session: SessionId,
        name: impl Into<String>,
    ) -> Result<(), ServerError> {
        let name = name.into();
        let current = self
            .sessions
            .get(&session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?;
        if current.name == name {
            return Ok(());
        }
        if self
            .sessions
            .values()
            .any(|candidate| candidate.name == name)
        {
            return Err(ServerError::InvalidCommand(format!(
                "duplicate session: {name}"
            )));
        }
        self.sessions
            .get_mut(&mut self.journal, &session)
            .expect("session target was resolved")
            .name = name;
        self.bump_generation();
        Ok(())
    }

    pub fn kill_session(&mut self, session: SessionId) -> Result<Vec<PaneId>, ServerError> {
        let session = self
            .sessions
            .remove(&mut self.journal, &session)
            .ok_or_else(|| ServerError::MissingTarget(session.to_string()))?;
        let mut panes = Vec::new();
        for window in session.windows {
            if let Some(window) = self.windows.remove(&mut self.journal, &window) {
                panes.extend(window.pane_order);
            }
        }
        self.bump_generation();
        Ok(panes)
    }

    pub(crate) fn mark_session_active_at(&mut self, session: SessionId, now: u64) {
        if self.sessions.contains_key(&session) {
            let activity = self.allocate_sort_point();
            let state = self
                .sessions
                .get_mut(&mut self.journal, &session)
                .expect("active session exists");
            state.activity = i64::try_from(now).ok();
            state.sort_activity = activity;
            self.last_active_session = Some(session);
        }
    }
}
