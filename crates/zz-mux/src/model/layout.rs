use super::*;

impl MuxState {
    pub fn resize_pane(
        &mut self,
        pane: PaneId,
        axis: Axis,
        delta_cells: i32,
    ) -> Result<(), ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let window = self
            .windows
            .get_mut(&mut self.journal, &window_id)
            .expect("window exists");
        window
            .layout
            .resize_pane(pane, axis, delta_cells)
            .map_err(|error| pane_layout_error(error, pane))?;
        window.zoomed_pane = None;
        self.bump_generation();
        Ok(())
    }

    pub fn resize_pane_to(
        &mut self,
        pane: PaneId,
        axis: Axis,
        cells: u16,
    ) -> Result<(), ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let window = self
            .windows
            .get_mut(&mut self.journal, &window_id)
            .expect("window exists");
        window
            .layout
            .resize_pane_to(pane, axis, cells)
            .map_err(|error| pane_layout_error(error, pane))?;
        window.zoomed_pane = None;
        self.bump_generation();
        Ok(())
    }

    pub(crate) fn resize_window(
        &mut self,
        window: WindowId,
        columns: u16,
        rows: u16,
    ) -> Result<(), ServerError> {
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        let before = window.layout.extent();
        window.layout.resize(columns, rows);
        window.last_extent_probe = None;
        window.clear_pane_screen_extents();
        if window.layout.extent() != before {
            self.bump_generation();
        }
        Ok(())
    }

    pub fn resize_split(
        &mut self,
        window: WindowId,
        split: SplitId,
        ratio: f32,
    ) -> Result<bool, ServerError> {
        if !ratio.is_finite() {
            return Err(ServerError::InvalidCommand(
                "split ratio must be finite".to_owned(),
            ));
        }
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        let changed = window
            .layout
            .set_divider_ratio(split, ratio)
            .map_err(|error| divider_layout_error(error, split))?;
        if changed {
            window.zoomed_pane = None;
            self.bump_generation();
        }
        Ok(changed)
    }

    pub fn select_layout(
        &mut self,
        window: WindowId,
        preset: LayoutPreset,
        options: &PresetOptions,
    ) -> Result<(), ServerError> {
        let panes = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?
            .pane_order
            .clone();
        debug_assert!(!panes.is_empty(), "validated windows are never empty");
        let next_split_id = &mut self.next_split_id;
        let mut ids = || {
            let id = SplitId(*next_split_id);
            *next_split_id = (*next_split_id).saturating_add(1);
            id
        };

        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .expect("window was resolved");
        let previous = window.saved_layout(self.legacy_layout_saves);
        window
            .layout
            .apply_preset(preset, &panes, options, &mut ids);
        window.previous_layout = Some(Box::new(previous));
        window.last_layout = Some(preset);
        self.bump_generation();
        Ok(())
    }

    pub fn select_layout_string(
        &mut self,
        window: WindowId,
        layout: &str,
    ) -> Result<(), ServerError> {
        let window_state = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
        let all_panes = window_state.pane_order.clone();
        let floats = all_panes
            .iter()
            .copied()
            .filter(|pane| window_state.is_floating(*pane))
            .filter_map(|pane| Some((pane, window_state.layout.pane_geometry(pane)?)))
            .collect::<Vec<_>>();
        let old_extent = window_state.layout.extent();
        let invalid = |cause: &str| ServerError::InvalidCommand(format!("{cause}: {layout}"));
        let mut parsed = CellLayout::parse(layout).map_err(|error| invalid(error.cause()))?;
        let legacy = parsed.version() == 1;
        let panes = if legacy {
            all_panes
                .iter()
                .copied()
                .filter(|pane| !floats.iter().any(|(float, _)| float == pane))
                .collect::<Vec<_>>()
        } else {
            all_panes.clone()
        };
        if panes.is_empty() {
            return Err(invalid(&format!("window {window} has no panes")));
        }
        let cells = parsed.pane_count();
        if panes.len() > cells {
            return Err(invalid(&format!(
                "have {} panes but need {cells}",
                panes.len()
            )));
        }
        while parsed.pane_count() > panes.len() {
            parsed.trim_bottom_right();
        }
        parsed
            .check_sizes()
            .map_err(|error| invalid(error.cause()))?;
        let split_ids = (0..all_panes.len().saturating_sub(1))
            .map(|_| self.allocate_split_id())
            .collect::<Vec<_>>();
        let mut split_ids = split_ids.into_iter();
        let mut ids = || {
            split_ids
                .next()
                .expect("parsed layout has one split ID per edge")
        };
        let (mut next, selection) = parsed.into_layout(&panes, &mut ids);
        if legacy {
            next.link_floats(&floats, &mut ids);
        }
        if !next.has_tiled() {
            next.keep_extent(old_extent);
        }
        let split_ids_exhausted = split_ids.next().is_none();
        debug_assert!(
            split_ids_exhausted,
            "parsed layout consumes one fresh ID per split"
        );

        let active_point = selection
            .as_ref()
            .and_then(|selection| selection.active)
            .map(|_| self.allocate_sort_point());
        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .expect("window was resolved");
        let previous = window.saved_layout(self.legacy_layout_saves);
        let old_z = window.z_order.clone();
        window.layout = next;
        if let Some(selection) = &selection {
            let mut z_order = selection.floats.clone();
            z_order.extend(
                old_z
                    .iter()
                    .copied()
                    .filter(|pane| !window.layout.is_floating(*pane)),
            );
            window.z_order = z_order;
        }
        window.previous_layout = Some(Box::new(previous));
        window.last_extent_probe = None;
        if let Some(selection) = selection {
            apply_window_selection(window, selection.active, selection.last_panes, active_point);
        }
        self.bump_generation();
        Ok(())
    }

    pub fn cycle_layout(
        &mut self,
        window: WindowId,
        offset: isize,
        options: &PresetOptions,
    ) -> Result<LayoutPreset, ServerError> {
        let last = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?
            .last_layout;
        let preset = match last {
            Some(last) => last.at_offset(offset),
            None if offset < 0 => LayoutPreset::Tiled,
            None => LayoutPreset::EvenHorizontal,
        };
        self.select_layout(window, preset, options)?;
        Ok(preset)
    }

    pub fn restore_previous_layout(&mut self, window: WindowId) -> Result<(), ServerError> {
        let has_previous = self
            .windows
            .get(&window)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?
            .previous_layout
            .is_some();
        if !has_previous {
            let window = self
                .windows
                .get_mut(&mut self.journal, &window)
                .expect("window was resolved");
            window.previous_layout = Some(Box::new(window.saved_layout(self.legacy_layout_saves)));
            return Ok(());
        }
        let (pane_order, split_count) = {
            let window_state = self
                .windows
                .get(&window)
                .ok_or_else(|| ServerError::MissingTarget(window.to_string()))?;
            let previous = window_state
                .previous_layout
                .as_deref()
                .expect("previous layout was checked");
            if previous.layout.pane_count() != window_state.pane_order.len() {
                return Err(ServerError::InvalidCommand(format!(
                    "window {window} previous layout no longer matches its panes"
                )));
            }
            (
                window_state.pane_order.clone(),
                window_state.pane_order.len().saturating_sub(1),
            )
        };
        let split_ids = (0..split_count)
            .map(|_| self.allocate_split_id())
            .collect::<Vec<_>>();
        let active_point = self.allocate_sort_point();

        let window = self
            .windows
            .get_mut(&mut self.journal, &window)
            .expect("window was resolved");
        let current = window.saved_layout(self.legacy_layout_saves);
        let SavedLayout {
            layout: mut restored,
            selection,
        } = *window
            .previous_layout
            .take()
            .expect("previous layout was validated");
        let moved = |pane: PaneId, saved: &SavedSelection| {
            saved
                .pane_order
                .iter()
                .position(|candidate| *candidate == pane)
                .and_then(|position| pane_order.get(position))
                .copied()
        };
        let order = selection.as_ref().map_or_else(
            || Some(pane_order.clone()),
            |saved| {
                restored
                    .panes_in_order()
                    .into_iter()
                    .map(|pane| moved(pane, saved))
                    .collect::<Option<Vec<_>>>()
            },
        );
        let replaced = restored.replace_panes_in_order(order.as_deref().unwrap_or(&pane_order));
        debug_assert!(replaced);
        let mut split_ids = split_ids.into_iter();
        let mut ids = || {
            split_ids
                .next()
                .expect("restored layout has one ID per edge")
        };
        restored.refresh_divider_ids(&mut ids);
        let split_ids_exhausted = split_ids.next().is_none();
        debug_assert!(
            split_ids_exhausted,
            "restored layout consumes one fresh ID per split"
        );
        window.layout = restored;
        window.previous_layout = Some(Box::new(current));
        window.zoomed_pane = None;
        if let Some(saved) = selection.filter(|_| order.is_some()) {
            let active = moved(saved.active, &saved);
            let last_panes = saved
                .last_panes
                .iter()
                .filter_map(|pane| moved(*pane, &saved))
                .collect();
            apply_window_selection(window, active, last_panes, Some(active_point));
            let mut z_order = window.modal_pane().into_iter().collect::<Vec<_>>();
            for pane in saved.z_order.iter().filter_map(|pane| moved(*pane, &saved)) {
                if window.layout.is_floating(pane) && !z_order.contains(&pane) {
                    z_order.push(pane);
                }
            }
            for pane in window.z_order.clone() {
                if !z_order.contains(&pane) {
                    z_order.push(pane);
                }
            }
            window.z_order = z_order;
        }
        self.bump_generation();
        Ok(())
    }

    pub fn spread_layout(&mut self, pane: PaneId) -> Result<(), ServerError> {
        let window_id = self
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let window = self
            .windows
            .get_mut(&mut self.journal, &window_id)
            .expect("pane window exists");
        let previous = window.saved_layout(self.legacy_layout_saves);
        window
            .layout
            .spread(pane)
            .map_err(|error| pane_layout_error(error, pane))?;
        window.previous_layout = Some(Box::new(previous));
        self.bump_generation();
        Ok(())
    }

    pub fn last_layout(&self, window: WindowId) -> Result<Option<LayoutPreset>, ServerError> {
        self.windows
            .get(&window)
            .map(|window| window.last_layout)
            .ok_or_else(|| ServerError::MissingTarget(window.to_string()))
    }
}

pub(super) fn pane_layout_error(error: LayoutError, pane: PaneId) -> ServerError {
    match error {
        LayoutError::UnknownPane => ServerError::MissingTarget(pane.to_string()),
        LayoutError::NoSpace => ServerError::InvalidCommand("no space for a new pane".to_owned()),
        LayoutError::Floating => ServerError::InvalidCommand("pane is floating".to_owned()),
        LayoutError::LastPane | LayoutError::UnknownDivider => {
            ServerError::Internal(format!("unexpected pane layout error: {error:?}"))
        }
    }
}

fn divider_layout_error(error: LayoutError, split: SplitId) -> ServerError {
    match error {
        LayoutError::UnknownDivider => ServerError::MissingTarget(split.to_string()),
        LayoutError::LastPane
        | LayoutError::NoSpace
        | LayoutError::UnknownPane
        | LayoutError::Floating => {
            ServerError::Internal(format!("unexpected divider layout error: {error:?}"))
        }
    }
}

#[cfg(test)]
pub(super) fn collect_pane_rects(
    node: &LayoutNode,
    bounds: PaneRect,
    output: &mut Vec<(PaneId, PaneRect)>,
) {
    match node {
        LayoutNode::Pane(pane) => output.push((*pane, bounds)),
        LayoutNode::Empty => {}
        LayoutNode::Split {
            axis,
            ratio,
            first,
            second,
            ..
        } => match axis {
            Axis::Horizontal => {
                let split = split_coordinate(bounds.left, bounds.right, *ratio);
                collect_pane_rects(
                    first,
                    PaneRect {
                        right: split,
                        ..bounds
                    },
                    output,
                );
                collect_pane_rects(
                    second,
                    PaneRect {
                        left: split,
                        ..bounds
                    },
                    output,
                );
            }
            Axis::Vertical => {
                let split = split_coordinate(bounds.top, bounds.bottom, *ratio);
                collect_pane_rects(
                    first,
                    PaneRect {
                        bottom: split,
                        ..bounds
                    },
                    output,
                );
                collect_pane_rects(
                    second,
                    PaneRect {
                        top: split,
                        ..bounds
                    },
                    output,
                );
            }
        },
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "validated split ratios map a bounded one-million-unit logical extent to u32"
)]
#[cfg(test)]
fn split_coordinate(start: u32, end: u32, ratio: f32) -> u32 {
    let extent = end.saturating_sub(start);
    if extent <= 1 {
        return start.saturating_add(extent);
    }
    let offset = (f64::from(extent) * f64::from(ratio)).round() as u32;
    start + offset.clamp(1, extent - 1)
}
