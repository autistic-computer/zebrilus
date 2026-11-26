use super::*;

impl MuxState {
    #[must_use]
    pub fn global_synchronize_panes(&self) -> bool {
        self.input_options.synchronize_panes().unwrap_or(false)
    }

    #[must_use]
    pub fn global_automatic_rename(&self) -> bool {
        self.input_options.automatic_rename().unwrap_or(true)
    }

    pub fn set_global_automatic_rename(&mut self, value: Option<bool>) {
        if self.input_options.automatic_rename() != value {
            self.input_options.set_automatic_rename(value);
            self.bump_generation();
        }
    }

    #[must_use]
    pub fn global_aggressive_resize(&self) -> bool {
        self.input_options.aggressive_resize().unwrap_or(false)
    }

    pub fn set_global_aggressive_resize(&mut self, value: Option<bool>) {
        if self.input_options.aggressive_resize() != value {
            self.input_options.set_aggressive_resize(value);
            self.bump_generation();
        }
    }

    pub fn window_aggressive_resize(&self, window: WindowId) -> Result<bool, ServerError> {
        let window = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        Ok(window
            .input_options
            .aggressive_resize()
            .unwrap_or_else(|| self.global_aggressive_resize()))
    }

    pub fn window_aggressive_resize_override(
        &self,
        window: WindowId,
    ) -> Result<Option<bool>, ServerError> {
        self.windows
            .get(&window)
            .map(|window| window.input_options.aggressive_resize())
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))
    }

    pub fn set_window_aggressive_resize(
        &mut self,
        window: WindowId,
        value: Option<bool>,
    ) -> Result<(), ServerError> {
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        if window.input_options.aggressive_resize() != value {
            window.input_options.set_aggressive_resize(value);
            self.bump_generation();
        }
        Ok(())
    }

    pub fn window_automatic_rename(&self, window: WindowId) -> Result<bool, ServerError> {
        let window = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        Ok(window
            .input_options
            .automatic_rename()
            .unwrap_or_else(|| self.global_automatic_rename()))
    }

    pub fn window_automatic_rename_override(
        &self,
        window: WindowId,
    ) -> Result<Option<bool>, ServerError> {
        self.windows
            .get(&window)
            .map(|window| window.input_options.automatic_rename())
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))
    }

    pub fn set_window_automatic_rename(
        &mut self,
        window: WindowId,
        value: Option<bool>,
    ) -> Result<(), ServerError> {
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        if window.input_options.automatic_rename() != value {
            window.input_options.set_automatic_rename(value);
            self.bump_generation();
        }
        Ok(())
    }

    pub fn set_global_synchronize_panes(&mut self, value: bool) {
        if self.global_synchronize_panes() != value {
            self.input_options.set_synchronize_panes(Some(value));
            self.bump_generation();
        }
    }

    pub fn window_synchronize_panes(&self, window: WindowId) -> Result<bool, ServerError> {
        let window = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        Ok(window
            .input_options
            .synchronize_panes()
            .unwrap_or_else(|| self.global_synchronize_panes()))
    }

    pub fn window_synchronize_override(
        &self,
        window: WindowId,
    ) -> Result<Option<bool>, ServerError> {
        self.windows
            .get(&window)
            .map(|window| window.input_options.synchronize_panes())
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))
    }

    pub fn set_window_synchronize_panes(
        &mut self,
        window: WindowId,
        value: Option<bool>,
    ) -> Result<(), ServerError> {
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        if window.input_options.synchronize_panes() != value {
            window.input_options.set_synchronize_panes(value);
            self.bump_generation();
        }
        Ok(())
    }

    pub fn pane_synchronize_panes(&self, pane: PaneId) -> Result<bool, ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        self.pane_synchronize_panes_in(window_id, pane)
    }

    pub(crate) fn pane_synchronize_panes_in(
        &self,
        window_id: WindowId,
        pane: PaneId,
    ) -> Result<bool, ServerError> {
        let window = &self.windows[&window_id];
        let pane = &window.panes[&pane];
        Ok(pane
            .input_options
            .synchronize_panes()
            .unwrap_or(self.window_synchronize_panes(window_id)?))
    }

    pub fn pane_synchronize_override(&self, pane: PaneId) -> Result<Option<bool>, ServerError> {
        self.pane(pane)
            .map(|pane| pane.input_options.synchronize_panes())
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))
    }

    pub fn set_pane_synchronize_panes(
        &mut self,
        pane: PaneId,
        value: Option<bool>,
    ) -> Result<(), ServerError> {
        let pane_state = self
            .pane_mut(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        if pane_state.input_options.synchronize_panes() != value {
            pane_state.input_options.set_synchronize_panes(value);
            self.bump_generation();
        }
        Ok(())
    }

    pub fn clear_pane_synchronize_overrides(
        &mut self,
        window: WindowId,
    ) -> Result<(), ServerError> {
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        let mut changed = false;
        for pane in window.panes.values_mut() {
            if pane.input_options.synchronize_panes().is_some() {
                pane.input_options.set_synchronize_panes(None);
                changed = true;
            }
        }
        if changed {
            self.bump_generation();
        }
        Ok(())
    }

    pub fn synchronized_input_targets(&self, source: PaneId) -> Result<Vec<PaneId>, ServerError> {
        let window_id = self
            .window_for_pane(source)
            .ok_or_else(|| ServerError::MissingTarget(source.to_string()))?;
        let window = &self.windows[&window_id];
        let synchronized = window
            .input_options
            .synchronize_panes()
            .unwrap_or_else(|| self.global_synchronize_panes());
        let source_synchronized = window.panes[&source]
            .input_options
            .synchronize_panes()
            .unwrap_or(synchronized);
        if !source_synchronized {
            return Ok(vec![source]);
        }
        Ok(window
            .panes
            .iter()
            .filter_map(|(id, pane)| {
                pane.input_options
                    .synchronize_panes()
                    .unwrap_or(synchronized)
                    .then_some(*id)
            })
            .collect())
    }
}
