use super::*;

impl MuxState {
    pub(crate) fn float_pane_with(
        &mut self,
        target: PaneId,
        kind: PaneKind,
        spawn: &FloatSpawn,
    ) -> Result<PaneId, ServerError> {
        let window_id = self
            .window_for_pane(target)
            .ok_or_else(|| ServerError::MissingTarget(target.to_string()))?;
        let pane_id = PaneId(self.next_pane_id);
        let active_point = self.allocate_sort_point();
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
            .float(target, pane_id, spawn.geometry, &mut ids)
            .map_err(|error| split_layout_error(error, target))?;
        self.next_pane_id = self.next_pane_id.saturating_add(1);
        window.panes.insert(
            pane_id,
            Pane {
                id: pane_id,
                title: pane_title(&kind, &self.default_pane_title),
                kind,
                active_point: 0,
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
                over_zoom: spawn.over_zoom,
                input_options: InputOptions::default(),
            },
        );
        if spawn.before && !spawn.full_size {
            insert_pane_order(&mut window.pane_order, pane_id, target, true, false);
        } else if spawn.before {
            window.pane_order.insert(0, pane_id);
        } else {
            window.pane_order.push(pane_id);
        }
        window.insert_float_z(pane_id);
        let activate = if let Some(mut modal) = spawn.modal {
            modal.pane = pane_id;
            modal.last = Some(window.active_pane);
            window.floats.modal = Some(modal);
            true
        } else {
            !spawn.detached && window.floats.modal.is_none()
        };
        if activate && activate_window_pane(window, pane_id, true) {
            window
                .panes
                .get_mut(&pane_id)
                .expect("new pane exists")
                .active_point = active_point;
        }
        window.clear_pane_screen_extents();
        self.bump_generation();
        Ok(pane_id)
    }

    pub(crate) fn float_existing(
        &mut self,
        pane: PaneId,
        geometry: CellGeometry,
        detached: bool,
    ) -> Result<(), ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let active_point = self.allocate_sort_point();
        let window = self
            .windows
            .get_mut(&mut self.journal, &window_id)
            .expect("window exists");
        if !window.layout.float_tile(pane, geometry) {
            return Err(ServerError::InvalidCommand(
                "pane is already floating".to_owned(),
            ));
        }
        window.raise(pane);
        if !detached && activate_window_pane(window, pane, true) {
            window
                .panes
                .get_mut(&pane)
                .expect("floated pane exists")
                .active_point = active_point;
        }
        window.clear_pane_screen_extents();
        self.bump_generation();
        Ok(())
    }

    pub(crate) fn tile_floating(
        &mut self,
        pane: PaneId,
        detached: bool,
    ) -> Result<(), ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let active_point = self.allocate_sort_point();
        let window = self
            .windows
            .get_mut(&mut self.journal, &window_id)
            .expect("window exists");
        window
            .layout
            .tile_float(pane)
            .map_err(|error| pane_layout_error(error, pane))?;
        window.lower_to_tail(pane);
        if !detached && activate_window_pane(window, pane, true) {
            window
                .panes
                .get_mut(&pane)
                .expect("tiled pane exists")
                .active_point = active_point;
        }
        window.clear_pane_screen_extents();
        self.bump_generation();
        Ok(())
    }

    pub(crate) fn set_float_geometry(
        &mut self,
        pane: PaneId,
        geometry: CellGeometry,
    ) -> Result<bool, ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let window = self
            .windows
            .get_mut(&mut self.journal, &window_id)
            .expect("window exists");
        let changed = window.layout.set_float_geometry(pane, geometry);
        if changed {
            window.clear_pane_screen_extents();
            self.bump_generation();
        }
        Ok(changed)
    }

    pub(crate) fn move_float_z(
        &mut self,
        pane: PaneId,
        position: FloatZ,
    ) -> Result<(), ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        self.windows
            .get_mut(&mut self.journal, &window_id)
            .expect("window exists")
            .move_float_z(pane, position);
        self.bump_generation();
        Ok(())
    }

    pub(crate) fn push_zoom(&mut self, window: WindowId, always: bool, flag: bool) -> PushedZoom {
        let Some(window) = self.windows.get_mut(&mut self.journal, &window) else {
            return PushedZoom::No;
        };
        let zoomed = window.zoomed_pane.is_some();
        let pushed = window.push_zoom(always, flag);
        if zoomed {
            self.bump_generation();
        }
        pushed
    }

    pub(crate) fn pop_zoom(&mut self, window: WindowId, pushed: PushedZoom) -> bool {
        let active_point = self.allocate_sort_point();
        let Some(window) = self.windows.get_mut(&mut self.journal, &window) else {
            return false;
        };
        let before = window.active_pane;
        let zoomed = window.pop_zoom(pushed);
        if window.active_pane != before {
            let active = window.active_pane;
            if let Some(pane) = window.panes.get_mut(&active) {
                pane.active_point = active_point;
            }
        }
        if zoomed {
            self.bump_generation();
        }
        zoomed
    }
}
