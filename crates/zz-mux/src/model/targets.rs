use super::*;

impl MuxState {
    #[must_use]
    pub fn default_context(&self) -> Option<(SessionId, WindowId, PaneId)> {
        self.sessions.values().find_map(|session| {
            let window = self.windows.get(&session.active_window)?;
            Some((session.id, window.id, window.active_pane))
        })
    }

    #[must_use]
    pub fn sessions_by_name(&self) -> Vec<&Session> {
        let mut sessions = self.sessions.values().collect::<Vec<_>>();
        sessions.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
        sessions
    }

    #[must_use]
    pub fn most_recent_context(&self) -> Option<(SessionId, WindowId, PaneId)> {
        self.last_active_session
            .and_then(|session| self.sessions.get(&session))
            .and_then(|session| {
                let window = self.windows.get(&session.active_window)?;
                Some((session.id, window.id, window.active_pane))
            })
            .or_else(|| self.default_context())
    }

    pub fn resolve_session(
        &self,
        target: Option<&str>,
        current: Option<SessionId>,
    ) -> Result<SessionId, ServerError> {
        self.resolve_session_with_pane_index(target, current, &|window, index| {
            self.windows
                .get(&window)?
                .pane_order()
                .get(usize::try_from(index).ok()?)
                .copied()
        })
    }

    pub(crate) fn resolve_session_with_pane_index(
        &self,
        target: Option<&str>,
        current: Option<SessionId>,
        pane_at_index: &impl Fn(WindowId, u32) -> Option<PaneId>,
    ) -> Result<SessionId, ServerError> {
        if target.is_some_and(is_marked_target) {
            return Ok(self.windows[&self.marked_window()?].session);
        }
        self.resolve_named_session_with_pane_index(
            target,
            current,
            TargetSlot::Session,
            pane_at_index,
        )
    }

    fn resolve_named_session(
        &self,
        target: Option<&str>,
        current: Option<SessionId>,
        slot: TargetSlot,
    ) -> Result<SessionId, ServerError> {
        self.resolve_named_session_with_pane_index(target, current, slot, &|window, index| {
            self.windows
                .get(&window)?
                .pane_order()
                .get(usize::try_from(index).ok()?)
                .copied()
        })
    }

    fn resolve_named_session_with_pane_index(
        &self,
        target: Option<&str>,
        current: Option<SessionId>,
        slot: TargetSlot,
        pane_at_index: &impl Fn(WindowId, u32) -> Option<PaneId>,
    ) -> Result<SessionId, ServerError> {
        let target = target.map(|target| {
            if target == "=" && slot != TargetSlot::PaneFallback {
                ""
            } else {
                target
            }
        });
        let Some(target) = target.filter(|target| !target.is_empty()) else {
            return current
                .filter(|session| self.sessions.contains_key(session))
                .or_else(|| self.sessions.keys().next().copied())
                .ok_or_else(|| ServerError::SessionNotFound("current session".to_owned()));
        };
        if slot == TargetSlot::Session {
            if let Some((session, window)) = target.split_once(':')
                && (session == "=" || window == "=" || window.starts_with("=."))
            {
                let session = if session == "=" { "" } else { session };
                let window = if window == "=" || window.starts_with("=.") {
                    &window[1..]
                } else {
                    window
                };
                return self.resolve_named_session_with_pane_index(
                    Some(&format!("{session}:{window}")),
                    current,
                    slot,
                    pane_at_index,
                );
            }
            let window_target = target.split_once(':').map_or(target, |(_, window)| window);
            let current_window = current
                .and_then(|session| self.sessions.get(&session))
                .map(|session| session.active_window);
            if let Some((session_target, pane_target)) = target.split_once(":.")
                && pane_target.starts_with('%')
            {
                let session = if session_target.is_empty() {
                    None
                } else {
                    Some(self.resolve_named_session_with_pane_index(
                        Some(session_target),
                        current,
                        slot,
                        pane_at_index,
                    )?)
                };
                let pane = self.resolve_pane_with_index(
                    Some(pane_target),
                    current_window,
                    None,
                    pane_at_index,
                )?;
                let window = self
                    .window_for_pane(pane)
                    .ok_or_else(|| ServerError::PaneNotFound(pane_target.to_owned()))?;
                let pane_session = self.windows[&window].session;
                let Some(session) = session else {
                    return Ok(pane_session);
                };
                return (pane_session == session)
                    .then_some(session)
                    .ok_or_else(|| ServerError::PaneNotFound(pane_target.to_owned()));
            }
            if window_target.contains('.') {
                let pane = self.resolve_pane_with_index(
                    Some(target),
                    current_window,
                    None,
                    pane_at_index,
                )?;
                let window = self
                    .window_for_pane(pane)
                    .ok_or_else(|| ServerError::PaneNotFound(target.to_owned()))?;
                return Ok(self.windows[&window].session);
            }
            if target.contains(':') || target.starts_with(['%', '@']) {
                let window = self.resolve_window_with_pane_index(
                    Some(target),
                    current,
                    current_window,
                    pane_at_index,
                )?;
                return Ok(self.windows[&window].session);
            }
        }
        let (target, exact) = if slot == TargetSlot::PaneFallback {
            (target, false)
        } else {
            target
                .strip_prefix('=')
                .map_or((target, false), |target| (target, true))
        };
        if target.starts_with('$') {
            let id = target
                .parse::<SessionId>()
                .map_err(|_| ServerError::SessionNotFound(target.to_owned()))?;
            return self
                .sessions
                .contains_key(&id)
                .then_some(id)
                .ok_or_else(|| ServerError::SessionNotFound(target.to_owned()));
        }
        if let Some(session) = self
            .sessions
            .values()
            .find(|session| session.name == target)
            .map(|session| session.id)
        {
            return Ok(session);
        }
        if exact {
            return Err(ServerError::SessionNotFound(target.to_owned()));
        }
        match unique_candidate(
            self.sessions
                .values()
                .filter(|session| session.name.starts_with(target))
                .map(|session| session.id),
        ) {
            Ok(Some(session)) => return Ok(session),
            Err(()) => return Err(ServerError::SessionNotFound(target.to_owned())),
            Ok(None) => {}
        }
        match unique_candidate(
            self.sessions
                .values()
                .filter(|session| fnmatch(target, &session.name))
                .map(|session| session.id),
        ) {
            Ok(Some(session)) => Ok(session),
            Ok(None) | Err(()) => Err(ServerError::SessionNotFound(target.to_owned())),
        }
    }

    pub fn resolve_window(
        &self,
        target: Option<&str>,
        current_session: Option<SessionId>,
        current_window: Option<WindowId>,
    ) -> Result<WindowId, ServerError> {
        self.resolve_window_with_pane_index(
            target,
            current_session,
            current_window,
            &|window, index| {
                let index = usize::try_from(index).ok()?;
                self.windows.get(&window)?.pane_order().get(index).copied()
            },
        )
    }

    pub(crate) fn resolve_window_with_pane_index(
        &self,
        target: Option<&str>,
        current_session: Option<SessionId>,
        current_window: Option<WindowId>,
        pane_at_index: &impl Fn(WindowId, u32) -> Option<PaneId>,
    ) -> Result<WindowId, ServerError> {
        if target.is_some_and(is_marked_target) {
            return self.marked_window();
        }
        if let Some(target) = target
            && target.starts_with('%')
        {
            let pane =
                self.resolve_pane_with_index(Some(target), current_window, None, pane_at_index)?;
            return self
                .window_for_pane(pane)
                .ok_or_else(|| ServerError::PaneNotFound(target.to_owned()));
        }
        match self.resolve_window_core(
            target,
            current_session,
            current_window,
            TargetSlot::Classified,
        ) {
            Ok(window) => Ok(window),
            Err(error) => {
                let Some(target) = target else {
                    return Err(error);
                };
                let window_target = target.split_once(':').map_or(target, |(_, window)| window);
                if !window_target.contains('.') {
                    return Err(error);
                }
                let pane = self.resolve_pane_with_index(
                    Some(target),
                    current_window,
                    None,
                    pane_at_index,
                )?;
                self.window_for_pane(pane)
                    .ok_or_else(|| ServerError::PaneNotFound(target.to_owned()))
            }
        }
    }

    fn resolve_window_core(
        &self,
        target: Option<&str>,
        current_session: Option<SessionId>,
        current_window: Option<WindowId>,
        slot: TargetSlot,
    ) -> Result<WindowId, ServerError> {
        self.resolve_window_target_core(target, current_session, current_window, false, slot)?
            .window
            .ok_or_else(|| ServerError::WindowNotFound(target.unwrap_or_default().to_owned()))
    }

    pub(crate) fn resolve_window_index_target(
        &self,
        target: Option<&str>,
        current_session: Option<SessionId>,
        current_window: Option<WindowId>,
    ) -> Result<(SessionId, Option<u32>), ServerError> {
        if target.is_some_and(is_marked_target) {
            let window = self.marked_window()?;
            let state = &self.windows[&window];
            return Ok((state.session, Some(state.index)));
        }
        let resolved = self.resolve_window_target_core(
            target,
            current_session,
            current_window,
            true,
            TargetSlot::Classified,
        )?;
        Ok((resolved.session, resolved.index))
    }

    fn resolve_window_target_core(
        &self,
        target: Option<&str>,
        current_session: Option<SessionId>,
        current_window: Option<WindowId>,
        index_mode: bool,
        slot: TargetSlot,
    ) -> Result<WindowTargetResolution, ServerError> {
        let current_session = current_session
            .filter(|session| self.sessions.contains_key(session))
            .or_else(|| {
                current_window
                    .and_then(|window| self.windows.get(&window).map(|window| window.session))
            });
        let Some(target) = target.filter(|target| !target.is_empty()) else {
            let window = current_window.filter(|window| self.windows.contains_key(window));
            let session = window
                .map(|window| self.windows[&window].session)
                .map_or_else(
                    || self.resolve_named_session(None, current_session, slot),
                    Ok,
                )?;
            let window = window.unwrap_or(self.sessions[&session].active_window);
            return Ok(WindowTargetResolution {
                session,
                window: Some(window),
                index: (!index_mode).then_some(self.windows[&window].index),
            });
        };
        if target.starts_with('$') && !target.contains([':', '.']) {
            let session = self.resolve_named_session(Some(target), current_session, slot)?;
            return Ok(WindowTargetResolution {
                session,
                window: Some(self.sessions[&session].active_window),
                index: None,
            });
        }
        if target.starts_with('@') {
            let window = target
                .parse::<WindowId>()
                .ok()
                .filter(|window| self.windows.contains_key(window))
                .ok_or_else(|| ServerError::WindowNotFound(target.to_owned()))?;
            let state = &self.windows[&window];
            return Ok(WindowTargetResolution {
                session: state.session,
                window: Some(window),
                index: Some(state.index),
            });
        }
        let (session_target, window_target) = target
            .split_once(':')
            .map_or((None, target), |(session, window)| (Some(session), window));
        let session = match session_target {
            Some("") | None => self.resolve_named_session(None, current_session, slot)?,
            Some(session) => self.resolve_named_session(Some(session), current_session, slot)?,
        };
        if window_target.is_empty() {
            return Ok(WindowTargetResolution {
                session,
                window: Some(self.sessions[&session].active_window),
                index: None,
            });
        }
        let window_error =
            match self.resolve_window_in_session(window_target, session, index_mode, slot) {
                Ok(window) => {
                    return Ok(WindowTargetResolution {
                        session,
                        window: window.window,
                        index: Some(window.index),
                    });
                }
                Err(error) => error,
            };
        if session_target.is_some() {
            return Err(window_error);
        }
        let (fallback, _) = normalize_window_target(window_target, slot);
        if let Ok(session) = self.resolve_named_session(Some(fallback), current_session, slot) {
            return Ok(WindowTargetResolution {
                session,
                window: Some(self.sessions[&session].active_window),
                index: None,
            });
        }
        Err(window_error)
    }

    fn resolve_window_in_session(
        &self,
        target: &str,
        session: SessionId,
        index_mode: bool,
        slot: TargetSlot,
    ) -> Result<WindowInSessionResolution, ServerError> {
        let state = self
            .sessions
            .get(&session)
            .ok_or_else(|| ServerError::SessionNotFound(session.to_string()))?;
        let (target, exact) = normalize_window_target(target, slot);
        let not_found = || ServerError::WindowNotFound(target.to_owned());
        if target.starts_with('@') {
            let window = target
                .parse::<WindowId>()
                .ok()
                .filter(|window| {
                    self.windows
                        .get(window)
                        .is_some_and(|window| window.session == session)
                })
                .ok_or_else(not_found)?;
            return Ok(WindowInSessionResolution {
                window: Some(window),
                index: self.windows[&window].index,
            });
        }
        if !exact && let Some(offset) = parse_offset(target) {
            let (forward, offset) = offset.map_err(|()| not_found())?;
            if index_mode {
                let current = self.windows[&state.active_window].index;
                let index = if forward {
                    current.checked_add(offset)
                } else {
                    current.checked_sub(offset)
                }
                .filter(|index| *index <= MAX_WINDOW_INDEX)
                .ok_or_else(not_found)?;
                return Ok(WindowInSessionResolution {
                    window: self.window_at_index(session, index),
                    index,
                });
            }
            let current = state
                .windows
                .iter()
                .position(|window| *window == state.active_window)
                .ok_or_else(not_found)?;
            let count = state.windows.len();
            if count == 0 {
                return Err(not_found());
            }
            let offset = usize::try_from(offset).map_err(|_| not_found())? % count;
            let position = if forward {
                (current + offset) % count
            } else {
                (current + count - offset) % count
            };
            let window = state.windows[position];
            return Ok(WindowInSessionResolution {
                window: Some(window),
                index: self.windows[&window].index,
            });
        }
        if !exact {
            let special = match target {
                "!" => state.last_window(),
                "^" => state.windows.first().copied(),
                "$" => state.windows.last().copied(),
                _ => None,
            };
            if matches!(target, "!" | "^" | "$") {
                let window = special.ok_or_else(not_found)?;
                return Ok(WindowInSessionResolution {
                    window: Some(window),
                    index: self.windows[&window].index,
                });
            }
        }
        if !target.starts_with(['+', '-'])
            && let Ok(index) = target.parse::<u32>()
            && index <= MAX_WINDOW_INDEX
        {
            let window = self.window_at_index(session, index);
            if window.is_some() || index_mode {
                return Ok(WindowInSessionResolution { window, index });
            }
        }
        match unique_candidate(state.windows.iter().copied().filter(|window| {
            self.windows
                .get(window)
                .is_some_and(|window| window.name == target)
        })) {
            Ok(Some(window)) => {
                return Ok(WindowInSessionResolution {
                    window: Some(window),
                    index: self.windows[&window].index,
                });
            }
            Err(()) => return Err(not_found()),
            Ok(None) => {}
        }
        if exact {
            return Err(not_found());
        }
        match unique_candidate(state.windows.iter().copied().filter(|window| {
            self.windows
                .get(window)
                .is_some_and(|window| window.name.starts_with(target))
        })) {
            Ok(Some(window)) => {
                return Ok(WindowInSessionResolution {
                    window: Some(window),
                    index: self.windows[&window].index,
                });
            }
            Err(()) => return Err(not_found()),
            Ok(None) => {}
        }
        match unique_candidate(state.windows.iter().copied().filter(|window| {
            self.windows
                .get(window)
                .is_some_and(|window| fnmatch(target, &window.name))
        })) {
            Ok(Some(window)) => Ok(WindowInSessionResolution {
                window: Some(window),
                index: self.windows[&window].index,
            }),
            Ok(None) | Err(()) => Err(not_found()),
        }
    }

    pub fn resolve_pane(
        &self,
        target: Option<&str>,
        current_window: Option<WindowId>,
        current_pane: Option<PaneId>,
    ) -> Result<PaneId, ServerError> {
        self.resolve_pane_with_index(target, current_window, current_pane, &|window, index| {
            let index = usize::try_from(index).ok()?;
            self.windows.get(&window)?.pane_order().get(index).copied()
        })
    }

    pub(crate) fn resolve_pane_with_index(
        &self,
        target: Option<&str>,
        current_window: Option<WindowId>,
        current_pane: Option<PaneId>,
        pane_at_index: &impl Fn(WindowId, u32) -> Option<PaneId>,
    ) -> Result<PaneId, ServerError> {
        if target.is_some_and(is_marked_target) {
            return self.marked_pane().ok_or_else(no_marked_target);
        }
        let Some(target) = target.filter(|target| !target.is_empty()) else {
            if let Some(pane) = current_pane
                && self.window_for_pane(pane).is_some()
            {
                return Ok(pane);
            }
            let window = current_window
                .and_then(|window| self.windows.get(&window))
                .or_else(|| {
                    self.default_context()
                        .and_then(|(_, window, _)| self.windows.get(&window))
                })
                .ok_or_else(|| ServerError::PaneNotFound("current pane".to_owned()))?;
            return Ok(window.active_pane);
        };
        if target.starts_with('%') {
            let id = target
                .parse::<PaneId>()
                .map_err(|_| ServerError::PaneNotFound(target.to_owned()))?;
            return self
                .window_for_pane(id)
                .map(|_| id)
                .ok_or_else(|| ServerError::PaneNotFound(target.to_owned()));
        }

        let current_window = current_pane
            .and_then(|pane| self.window_for_pane(pane))
            .or_else(|| current_window.filter(|window| self.windows.contains_key(window)));
        let current_session = current_window
            .and_then(|window| self.windows.get(&window).map(|window| window.session));
        let pane_target = normalize_pane_target(target);
        let pane_error = ServerError::PaneNotFound(pane_target.to_owned());

        let Some((window_target, pane_target)) = target.split_once('.') else {
            if !target.contains(':') {
                let window = self.resolve_window_core(
                    None,
                    current_session,
                    current_window,
                    TargetSlot::Classified,
                )?;
                if let Ok(pane) = self.resolve_pane_in_window(target, window, pane_at_index) {
                    return Ok(pane);
                }
            }
            let (fallback, slot) = if target.contains(':') {
                (target, TargetSlot::Classified)
            } else {
                (pane_target, TargetSlot::PaneFallback)
            };
            let window = match self.resolve_window_core(
                Some(fallback),
                current_session,
                current_window,
                slot,
            ) {
                Ok(window) => window,
                Err(_) if !target.contains(':') => return Err(pane_error),
                Err(error) => return Err(error),
            };
            return self
                .windows
                .get(&window)
                .map(|window| window.active_pane)
                .ok_or_else(|| ServerError::WindowNotFound(target.to_owned()));
        };
        let window = if window_target.is_empty() {
            self.resolve_window_core(
                None,
                current_session,
                current_window,
                TargetSlot::Classified,
            )?
        } else {
            self.resolve_window_core(
                Some(window_target),
                current_session,
                current_window,
                TargetSlot::Classified,
            )?
        };
        if pane_target.is_empty() {
            return Ok(self.windows[&window].active_pane);
        }
        self.resolve_pane_in_window(pane_target, window, pane_at_index)
    }

    fn resolve_pane_in_window(
        &self,
        target: &str,
        window: WindowId,
        pane_at_index: &impl Fn(WindowId, u32) -> Option<PaneId>,
    ) -> Result<PaneId, ServerError> {
        let target = normalize_pane_target(target);
        let not_found = || ServerError::PaneNotFound(target.to_owned());
        let state = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::WindowNotFound(window.to_string()))?;
        if target.starts_with('%') {
            let pane = target.parse::<PaneId>().map_err(|_| not_found())?;
            return state
                .panes
                .contains_key(&pane)
                .then_some(pane)
                .ok_or_else(not_found);
        }
        if target == "!" {
            return state.last_panes.first().copied().ok_or_else(not_found);
        }
        if let Some((_, direction)) = PANE_DIRECTION_TARGETS
            .iter()
            .find(|(name, _)| *name == target)
        {
            return self
                .pane_in_direction(state.active_pane, *direction)?
                .ok_or_else(not_found);
        }
        if let Some(offset) = parse_offset(target) {
            let (forward, offset) = offset.map_err(|()| not_found())?;
            let current = state
                .pane_order
                .iter()
                .position(|pane| *pane == state.active_pane)
                .ok_or_else(not_found)?;
            let count = state.pane_order.len();
            if count == 0 {
                return Err(not_found());
            }
            let offset = usize::try_from(offset).map_err(|_| not_found())? % count;
            let position = if forward {
                (current + offset) % count
            } else {
                (current + count - offset) % count
            };
            return Ok(state.pane_order[position]);
        }
        if let Ok(index) = target.parse::<u32>()
            && let Some(pane) = pane_at_index(window, index)
        {
            return Ok(pane);
        }
        self.pane_at_compass_point(window, target)
            .ok_or_else(not_found)
    }

    fn pane_at_compass_point(&self, window: WindowId, target: &str) -> Option<PaneId> {
        let state = self.windows.get(&window)?;
        let compass = PANE_COMPASS_TARGETS
            .into_iter()
            .find(|name| target.eq_ignore_ascii_case(name))?;
        let (columns, rows) = state.layout.extent();
        let (last_column, last_row) = (columns.saturating_sub(1), rows.saturating_sub(1));
        let (x, y) = match compass {
            "top" => (columns / 2, 0),
            "bottom" => (columns / 2, last_row),
            "left" => (0, rows / 2),
            "right" => (last_column, rows / 2),
            "top-left" => (0, 0),
            "top-right" => (last_column, 0),
            "bottom-left" => (0, last_row),
            _ => (last_column, last_row),
        };
        let (x, y) = (i32::from(x), i32::from(y));
        let float_contains = |pane: PaneId| {
            state.layout.pane_geometry(pane).is_some_and(|geometry| {
                (geometry.xoff - 1..=geometry.xoff + i32::from(geometry.sx)).contains(&x)
                    && (geometry.yoff - 1..=geometry.yoff + i32::from(geometry.sy)).contains(&y)
            })
        };
        if let Some(modal) = state.modal_pane() {
            return float_contains(modal).then_some(modal);
        }
        let visible = |pane: PaneId| match state.zoomed_pane {
            Some(zoomed) => {
                zoomed != pane && state.panes.get(&pane).is_some_and(|pane| pane.over_zoom)
            }
            None => true,
        };
        if let Some(pane) = state
            .z_order
            .iter()
            .copied()
            .find(|pane| state.is_floating(*pane) && visible(*pane) && float_contains(*pane))
        {
            return Some(pane);
        }
        if let Some(zoomed) = state.zoomed_pane {
            return Some(zoomed);
        }
        state.layout.tiled_panes().into_iter().find(|pane| {
            state.layout.pane_geometry(*pane).is_some_and(|geometry| {
                (geometry.xoff..=geometry.xoff + i32::from(geometry.sx)).contains(&x)
                    && (geometry.yoff..=geometry.yoff + i32::from(geometry.sy)).contains(&y)
            })
        })
    }

    #[must_use]
    pub fn marked_pane(&self) -> Option<PaneId> {
        let (session, window, pane) = self.marked_pane?;
        let window = self.windows.get(&window)?;
        (window.session == session && window.panes.contains_key(&pane)).then_some(pane)
    }

    fn marked_window(&self) -> Result<WindowId, ServerError> {
        self.marked_pane()
            .and_then(|pane| self.window_for_pane(pane))
            .ok_or_else(no_marked_target)
    }

    pub(crate) fn toggle_marked_pane(&mut self, pane: PaneId) -> Result<(), ServerError> {
        let window = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let session = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?
            .session;
        let next = (self.marked_pane() != Some(pane)).then_some((session, window, pane));
        if self.marked_pane != next {
            self.marked_pane = next;
            self.bump_generation();
        }
        Ok(())
    }

    pub fn set_marked_pane(&mut self, pane: PaneId) -> Result<(), ServerError> {
        let window = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let session = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?
            .session;
        let next = Some((session, window, pane));
        if self.marked_pane != next {
            self.marked_pane = next;
            self.bump_generation();
        }
        Ok(())
    }

    pub fn clear_marked_pane(&mut self) {
        if self.marked_pane.is_some() {
            self.marked_pane = None;
            self.bump_generation();
        }
    }
}

#[derive(Clone, Copy)]
struct WindowTargetResolution {
    session: SessionId,
    window: Option<WindowId>,
    index: Option<u32>,
}

#[derive(Clone, Copy)]
struct WindowInSessionResolution {
    window: Option<WindowId>,
    index: u32,
}

fn unique_candidate<T: Copy>(candidates: impl IntoIterator<Item = T>) -> Result<Option<T>, ()> {
    let mut candidates = candidates.into_iter();
    let first = candidates.next();
    if first.is_some() && candidates.next().is_some() {
        return Err(());
    }
    Ok(first)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetSlot {
    Session,
    Classified,
    PaneFallback,
}

fn normalize_window_target(target: &str, slot: TargetSlot) -> (&str, bool) {
    if slot == TargetSlot::PaneFallback {
        return (target, false);
    }
    let (target, exact) = target
        .strip_prefix('=')
        .map_or((target, false), |target| (target, true));
    let target = match target {
        "{start}" => "^",
        "{last}" => "!",
        "{end}" => "$",
        "{next}" => "+",
        "{previous}" => "-",
        _ => target,
    };
    (target, exact)
}

pub(crate) const NO_MARKED_TARGET: &str = "no marked target";

pub(crate) fn is_marked_target(target: &str) -> bool {
    matches!(target, "~" | "{marked}")
}

fn no_marked_target() -> ServerError {
    ServerError::InvalidCommand(NO_MARKED_TARGET.to_owned())
}

const PANE_DIRECTION_TARGETS: [(&str, PaneDirection); 4] = [
    ("{up-of}", PaneDirection::Up),
    ("{down-of}", PaneDirection::Down),
    ("{left-of}", PaneDirection::Left),
    ("{right-of}", PaneDirection::Right),
];

const PANE_COMPASS_TARGETS: [&str; 8] = [
    "top",
    "bottom",
    "left",
    "right",
    "top-left",
    "top-right",
    "bottom-left",
    "bottom-right",
];

fn normalize_pane_target(target: &str) -> &str {
    match target {
        "{last}" => "!",
        "{next}" => "+",
        "{previous}" => "-",
        "{top}" => "top",
        "{bottom}" => "bottom",
        "{left}" => "left",
        "{right}" => "right",
        "{top-left}" => "top-left",
        "{top-right}" => "top-right",
        "{bottom-left}" => "bottom-left",
        "{bottom-right}" => "bottom-right",
        _ => target,
    }
}

fn parse_offset(target: &str) -> Option<Result<(bool, u32), ()>> {
    let (forward, offset) = match target.as_bytes().first() {
        Some(b'+') => (true, &target[1..]),
        Some(b'-') => (false, &target[1..]),
        _ => return None,
    };
    if offset.is_empty() {
        return Some(Ok((forward, 1)));
    }
    let offset = offset
        .trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r'])
        .parse::<i64>()
        .ok()
        .and_then(|offset| u32::try_from(offset).ok())
        .filter(|offset| (1..=MAX_WINDOW_INDEX).contains(offset));
    Some(offset.map(|offset| (forward, offset)).ok_or(()))
}

#[derive(Clone)]
enum GlobToken {
    AnySequence,
    AnyCharacter,
    Literal(char),
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
}

#[derive(Clone, Copy)]
struct GlobClassCharacter {
    value: char,
    escaped: bool,
}

pub(crate) fn fnmatch(pattern: &str, value: &str) -> bool {
    GlobPattern::new(pattern).matches(value)
}

#[derive(Clone)]
pub(crate) struct GlobPattern(Option<Vec<GlobToken>>);

impl GlobPattern {
    pub(crate) fn new(pattern: &str) -> Self {
        Self(glob_tokens(pattern))
    }

    pub(crate) fn literal(&self) -> Option<String> {
        self.0
            .as_ref()?
            .iter()
            .map(|token| match token {
                GlobToken::Literal(character) => Some(*character),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn matches(&self, value: &str) -> bool {
        let Some(tokens) = &self.0 else {
            return false;
        };
        let mut rest = value.chars();
        for (index, token) in tokens.iter().enumerate() {
            let GlobToken::Literal(expected) = token else {
                return glob_matches(&tokens[index..], rest.as_str());
            };
            if rest.next() != Some(*expected) {
                return false;
            }
        }
        rest.next().is_none()
    }
}

fn glob_matches(tokens: &[GlobToken], value: &str) -> bool {
    let value = value.chars().collect::<Vec<_>>();
    let mut matched = vec![false; value.len() + 1];
    matched[0] = true;
    for token in tokens {
        let mut next = vec![false; value.len() + 1];
        match token {
            GlobToken::AnySequence => {
                next[0] = matched[0];
                for index in 1..=value.len() {
                    next[index] = matched[index] || next[index - 1];
                }
            }
            GlobToken::AnyCharacter => {
                next[1..].copy_from_slice(&matched[..value.len()]);
            }
            GlobToken::Literal(expected) => {
                for (index, character) in value.iter().copied().enumerate() {
                    next[index + 1] = matched[index] && character == *expected;
                }
            }
            GlobToken::Class { negated, ranges } => {
                for (index, character) in value.iter().copied().enumerate() {
                    let contains = ranges
                        .iter()
                        .any(|(start, end)| *start <= character && character <= *end);
                    next[index + 1] = matched[index] && contains != *negated;
                }
            }
        }
        matched = next;
    }
    matched[value.len()]
}

fn glob_tokens(pattern: &str) -> Option<Vec<GlobToken>> {
    let characters = pattern.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        match characters[index] {
            '*' => {
                if !matches!(tokens.last(), Some(GlobToken::AnySequence)) {
                    tokens.push(GlobToken::AnySequence);
                }
                index += 1;
            }
            '?' => {
                tokens.push(GlobToken::AnyCharacter);
                index += 1;
            }
            '\\' => {
                index += 1;
                let character = characters.get(index).copied()?;
                tokens.push(GlobToken::Literal(character));
                index += 1;
            }
            '[' => {
                if let Some((token, next)) = glob_class(&characters, index + 1) {
                    tokens.push(token);
                    index = next;
                } else {
                    tokens.push(GlobToken::Literal('['));
                    index += 1;
                }
            }
            character => {
                tokens.push(GlobToken::Literal(character));
                index += 1;
            }
        }
    }
    Some(tokens)
}

fn glob_class(characters: &[char], mut index: usize) -> Option<(GlobToken, usize)> {
    let negated = matches!(characters.get(index), Some('!' | '^'));
    if negated {
        index += 1;
    }
    let mut members: Vec<GlobClassCharacter> = Vec::new();
    while index < characters.len() {
        let character = characters[index];
        if character == ']' && !members.is_empty() {
            let mut ranges = Vec::new();
            let mut member = 0;
            while member < members.len() {
                if member + 2 < members.len()
                    && members[member + 1].value == '-'
                    && !members[member + 1].escaped
                {
                    ranges.push((members[member].value, members[member + 2].value));
                    member += 3;
                } else {
                    ranges.push((members[member].value, members[member].value));
                    member += 1;
                }
            }
            return Some((GlobToken::Class { negated, ranges }, index + 1));
        }
        if character == '\\' {
            index += 1;
            members.push(GlobClassCharacter {
                value: characters.get(index).copied()?,
                escaped: true,
            });
        } else {
            members.push(GlobClassCharacter {
                value: character,
                escaped: false,
            });
        }
        index += 1;
    }
    None
}
