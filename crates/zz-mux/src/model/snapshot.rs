use super::*;

impl MuxState {
    #[must_use]
    pub fn snapshot(&self) -> MuxSnapshot {
        let sessions = self
            .sessions
            .values()
            .map(|session| SessionSnapshot {
                id: session.id,
                name: session.name.clone(),
                active_window: session.active_window,
                windows: session
                    .windows
                    .iter()
                    .filter_map(|id| self.windows.get(id))
                    .map(|window| self.window_snapshot(window))
                    .collect(),
                viewers: Vec::new(),
            })
            .collect();
        MuxSnapshot {
            generation: self.generation,
            sessions,
            focused_window: None,
        }
    }

    fn window_snapshot(&self, window: &Window) -> WindowSnapshot {
        let synchronized = window
            .input_options
            .synchronize_panes()
            .unwrap_or_else(|| self.global_synchronize_panes());
        let pane_base_index = self.pane_base_index(window.id);
        let layout_dump = window.layout_string(LayoutFormat::V2, pane_base_index);
        let visible_layout_dump = window.visible_layout_string(LayoutFormat::V2, pane_base_index);
        let (sx, sy) = window.layout.extent();
        WindowSnapshot {
            id: window.id,
            index: window.index,
            name: window.name.clone(),
            automatic_rename: window
                .input_options
                .automatic_rename()
                .unwrap_or_else(|| self.global_automatic_rename()),
            active_pane: window.active_pane,
            zoomed_pane: window.zoomed_pane,
            layout: window.layout.project(),
            panes: window
                .panes
                .iter()
                .map(|(id, pane)| {
                    (
                        *id,
                        PaneSnapshot {
                            id: pane.id,
                            title: pane.title.clone(),
                            kind: pane.kind.snapshot(),
                            synchronized_input: pane
                                .input_options
                                .synchronize_panes()
                                .unwrap_or(synchronized),
                            bell: pane.bell,
                            dead: pane.dead,
                            dead_status: pane.dead_status,
                            border_colour: None,
                            active_border_colour: None,
                            border_status_text: String::new(),
                            mode: None,
                            status: pane.status.clone(),
                            summary: pane.summary.clone(),
                        },
                    )
                })
                .collect(),
            layout_dump,
            visible_layout_dump,
            status_label: String::new(),
            activity: window.activity_flag,
            silence: window.silence_flag,
            pane_border_status: PaneBorderStatus::Off,
            pane_border_lines: PaneBorderLines::Single,
            pane_border_indicators: PaneBorderIndicators::Colour,
            pane_order: window.pane_order.clone(),
            pane_z_order: window.z_order.clone(),
            floating: window
                .z_order
                .iter()
                .filter(|pane| window.layout.is_floating(**pane))
                .filter_map(|pane| {
                    let geometry = window.layout.pane_geometry(*pane)?;
                    Some(FloatingPaneSnapshot {
                        pane: *pane,
                        xoff: geometry.xoff,
                        yoff: geometry.yoff,
                        sx: geometry.sx,
                        sy: geometry.sy,
                        visible: window.shows_floating(*pane),
                        border_lines: PaneBorderLines::Single,
                        border_status: PaneBorderStatus::Off,
                    })
                })
                .collect(),
            modal: window.floats.modal.map(|modal| ModalPaneSnapshot {
                pane: modal.pane,
                capture_keys: modal.capture_keys,
                close_on_click: modal.close_on_click,
                close_on_cancel: modal.close_on_cancel,
            }),
            sx,
            sy,
            pane_base_index,
        }
    }
}
