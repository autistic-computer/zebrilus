use super::*;

impl AppView {
    pub(super) fn render_layout(
        &self,
        node: &LayoutNode,
        window: &WindowSnapshot,
        corners: WindowCorners,
        drag_layer: PaneDragLayer,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match node {
            LayoutNode::Pane(pane) => {
                let radii = config::pane_content_radii(cx, corners);
                let active = *pane == window.active_pane;
                let inactive = !active;
                let behind_pip = window
                    .floating
                    .iter()
                    .any(|float| float.visible && float.pane == window.active_pane);
                let dimmed = inactive && !behind_pip;
                let inactive_opacity = config::pane_inactive_opacity(cx);
                let pane_snapshot = window.panes.get(pane);
                let synchronized = pane_snapshot.is_some_and(|pane| pane.synchronized_input);
                let dead_label = dead_label(pane_snapshot);
                let terminal_overlay = self
                    .command_output
                    .as_ref()
                    .filter(|(output_pane, _)| output_pane == pane)
                    .map(|(_, output)| output)
                    .or_else(|| {
                        self.terminals
                            .get(pane)
                            .filter(|_| !self.pickers.contains_key(pane))
                    });
                let floats = window.floating.iter().any(|float| float.pane == *pane);
                let header_place = pane_snapshot
                    .filter(|_| {
                        !self.pickers.contains_key(pane) && self.terminal_headers.contains_key(pane)
                    })
                    .map_or(HeaderPlace::Stacked, |held| {
                        HeaderPlace::of(&held.kind, floats)
                    });
                let header_band = header_place.band(
                    zz_ui::pane::tiled_pane_surface(
                        crate::theme::chrome_background(cx),
                        crate::theme::shell_frame(cx),
                        config::pane_gaps(cx),
                        active,
                        cx,
                    ),
                    cx,
                );
                if let Some(terminal) = terminal_overlay {
                    terminal.update(cx, |terminal, cx| {
                        terminal.set_pane_status(dead_label.clone(), synchronized, cx);
                        terminal.set_header_band(header_band, cx);
                    });
                }
                if let Some(agent) = self.agents.get(pane) {
                    agent.update(cx, |agent, cx| agent.set_header_band(header_band, cx));
                }
                let pane_content = self.pane_content(*pane, dimmed, inactive_opacity, cx);
                let newer_kind = pane_snapshot.and_then(|held| match &held.kind {
                    PaneKindSnapshot::Other { kind } => Some(kind.as_str()),
                    _ => None,
                });
                let waiting = pane_content.is_none() && newer_kind.is_none();
                let content = pane_content.as_ref().map_or_else(
                    || {
                        crate::window::corners::round_div_radii(
                            div().size_full().flex().children(newer_kind.map(|kind| {
                                penna::blocks::feedback::empty_state(
                                    None,
                                    zz_client::navigation::newer_pane_text(kind),
                                    cx,
                                )
                            })),
                            radii,
                        )
                        .into_any_element()
                    },
                    PaneContent::element,
                );
                let terminal_header = pane_snapshot.filter(|held| {
                    matches!(
                        held.kind,
                        PaneKindSnapshot::Terminal
                            | PaneKindSnapshot::Agent(_)
                            | PaneKindSnapshot::View(_)
                    ) && !self.pickers.contains_key(pane)
                        || self.simulators.contains_key(pane)
                });
                let header_view = terminal_header
                    .and_then(|snapshot| Some((snapshot, self.terminal_headers.get(pane)?)));
                let content = if let Some((snapshot, header_view)) = header_view {
                    let heading = zz_client::window_summary::pane_heading(snapshot);
                    let simulator = self.simulators.get(pane).map(|view| view.read(cx).title());
                    let state = TerminalHeaderState {
                        icon: if simulator.is_some() {
                            crate::simulator::PHONE
                        } else {
                            zz_ui::pane::pane_kind_icon(&snapshot.kind)
                        },
                        program: heading.program,
                        active,
                        title: simulator.map_or(heading.title, Into::into),
                        can_drag: self.mux.read(cx).is_connected()
                            && window.zoomed_pane.is_none()
                            && window.panes.len() > 1,
                        place: zz_ui::pane::PaneState {
                            floats,
                            lone: window.panes.len() == 1,
                            zoomed: window.zoomed_pane == Some(*pane),
                        },
                        folder: self.pane_folder(snapshot, cx),
                        flip: self.pane_flip(snapshot),
                        keys: self.pane_action_keys(cx),
                        buttons: self.pane_kind_buttons(*pane, cx),
                        menu: self.pane_title_menu(*pane, cx),
                        trailing_inset: window_controls_inset(
                            self.window_controls_pane == Some(*pane),
                            cx.theme().pane_header_height(),
                            cx,
                        ),
                        radii,
                        opacity: if dimmed { inactive_opacity } else { 1.0 },
                    };
                    header_view.update(cx, |header, cx| header.sync(state, cx));
                    zz_ui::pane::headed_pane(
                        header_place,
                        AnyView::from(header_view.clone())
                            .cached(
                                StyleRefinement::default()
                                    .h(cx.theme().pane_header_height())
                                    .w_full()
                                    .flex_none(),
                            )
                            .into_any_element(),
                        content,
                        self.tile_size_report(*pane, pane_snapshot),
                        cx,
                    )
                    .into_any_element()
                } else if let Some(report) = self.tile_size_report(*pane, pane_snapshot) {
                    div()
                        .relative()
                        .size_full()
                        .child(content)
                        .child(report)
                        .into_any_element()
                } else {
                    content
                };
                let mut status_tags: Vec<AnyElement> = Vec::new();
                status_tags.extend(self.flip_overlay(*pane, cx));
                if terminal_overlay.is_none() {
                    if let Some(label) = dead_label {
                        status_tags.push(
                            zz_ui::pane::pane_status_badge(penna::IconName::CircleX, label, cx)
                                .into_any_element(),
                        );
                    }
                    if waiting {
                        status_tags.push(
                            pane_waiting_state(format!("Waiting for {pane}"), cx)
                                .into_any_element(),
                        );
                    }
                    if synchronized {
                        status_tags.push(pane_sync_badge(cx).into_any_element());
                    }
                }
                let mut overlays: Vec<AnyElement> = Vec::with_capacity(3);
                if !status_tags.is_empty() {
                    overlays.push(
                        pane_overlay_stack(PaneOverlayCorner::TopRight, status_tags)
                            .when(terminal_header.is_some(), |stack| {
                                stack.top(cx.theme().pane_header_height() + px(8.0))
                            })
                            .into_any_element(),
                    );
                }
                if let Some(indicator) = self
                    .pane_indicators
                    .get(pane)
                    .and_then(|indicator| self.pane_indicator(indicator, cx))
                {
                    overlays.push(indicator.into_any_element());
                }
                let drag_state = match drag_layer {
                    PaneDragLayer::Idle => None,
                    PaneDragLayer::Dragging(source) if source == *pane => {
                        Some(PaneDragOverlayState::Source)
                    }
                    PaneDragLayer::Armed => Some(PaneDragOverlayState::Armed),
                    PaneDragLayer::Dragging(_) => Some(PaneDragOverlayState::Target),
                };
                if let Some(drag_state) = drag_state {
                    let title = window
                        .panes
                        .get(pane)
                        .map_or_else(String::new, |pane| pane.title.clone());
                    overlays.push(
                        Self::pane_drag_handle(*pane, title, drag_state, radii, cx)
                            .into_any_element(),
                    );
                }
                let surface_dimmed = dimmed
                    && pane_content
                        .as_ref()
                        .is_none_or(|content| content.inactive_style == PaneInactiveStyle::Surface);
                let covers_frame = pane_content
                    .as_ref()
                    .is_some_and(|content| content.covers_frame);
                let gaps = config::pane_gaps(cx);
                let follows_pointer = inactive && self.mux.read(cx).focus_follows_mouse();
                let hovered_pane = *pane;
                let shown = if cx.reduce_motion() {
                    1.0
                } else {
                    self.pane_arrivals
                        .shown(*pane, cx.background_executor().now())
                };
                let surface = pane_surface(
                    ("mux-pane", pane.0),
                    content,
                    overlays,
                    PaneChrome::new(radii, gaps)
                        .active(active)
                        .dimmed(surface_dimmed, inactive_opacity)
                        .rim_over_content(covers_frame)
                        .frame(gaps.then(|| {
                            PaneLook::global(cx).frame(
                                cx.theme(),
                                crate::theme::chrome_background(cx),
                                active,
                            )
                        })),
                    cx,
                )
                .key_context(pane_key_context(*pane))
                .when(shown < 1.0, |surface| surface.opacity(shown))
                .when(follows_pointer, |surface| {
                    surface.on_mouse_move(cx.listener(
                        move |view, event: &MouseMoveEvent, _, cx| {
                            if event.pressed_button.is_none() {
                                view.on_pane_pointer_focus(hovered_pane, cx);
                            }
                        },
                    ))
                });
                moving_pane(*pane, &self.pane_motion, surface)
                    .hold(
                        self.terminals
                            .get(pane)
                            .map(|terminal| terminal.read(cx).moving()),
                    )
                    .into_any_element()
            }
            LayoutNode::Empty => div().size_full().into_any_element(),
            LayoutNode::Split {
                id,
                axis,
                ratio,
                first,
                second,
            } => {
                let snapshot_ratio = *ratio;
                let ratio = self
                    .split_drag
                    .filter(|drag| drag.drag.window == window.id && drag.drag.split == *id)
                    .map_or(snapshot_ratio, |drag| drag.ratio);
                let (first_corners, second_corners) = corners.split(*axis);
                let first_element =
                    self.render_layout(first, window, first_corners, drag_layer, cx);
                let second_element =
                    self.render_layout(second, window, second_corners, drag_layer, cx);
                let resizing = self
                    .split_drag
                    .is_some_and(|drag| drag.drag.window == window.id && drag.drag.split == *id);
                let ratio_override = self
                    .split_drag
                    .filter(|drag| drag.drag.window == window.id)
                    .map(|drag| (drag.drag.split, drag.ratio));
                let highlight =
                    pane_separator(node, window.active_pane, ratio_override).map(|separator| {
                        let span = separator.span();
                        PaneSplitHighlight::new(
                            span.start(),
                            span.length(),
                            match separator.side() {
                                SeparatorSide::First => PaneSplitSide::First,
                                SeparatorSide::Second => PaneSplitSide::Second,
                            },
                            cx.theme().accent,
                        )
                    });
                let split_axis = match axis {
                    Axis::Horizontal => PaneSplitAxis::Horizontal,
                    Axis::Vertical => PaneSplitAxis::Vertical,
                };
                let drag = SplitDrag {
                    window: window.id,
                    split: *id,
                    start_ratio: snapshot_ratio,
                    axis: *axis,
                };
                let hit_target: AnyElement = if matches!(drag_layer, PaneDragLayer::Dragging(_)) {
                    div().absolute().into_any_element()
                } else {
                    pane_split_hit_target(
                        ("split-divider", id.0),
                        split_axis,
                        ratio,
                        config::pane_margin(cx),
                    )
                    .on_drag(drag, |_: &SplitDrag, _, _, cx| cx.new(|_| SplitDragPreview))
                    .into_any_element()
                };
                let split = *id;
                pane_split_surface(
                    ("mux-split", id.0),
                    split_axis,
                    ratio,
                    resizing,
                    config::pane_gaps(cx),
                    config::pane_margin(cx),
                    None,
                    highlight,
                    first_element,
                    second_element,
                    hit_target,
                    cx,
                )
                .on_drag_move::<SplitDrag>(cx.listener(
                    move |view, event: &DragMoveEvent<SplitDrag>, window, cx| {
                        if event.drag(cx).split == split {
                            view.on_split_drag_move(event, window, cx);
                        }
                    },
                ))
                .into_any_element()
            }
        }
    }

    pub(super) fn synchronize_pane_corners(
        &mut self,
        active_window: Option<&WindowSnapshot>,
        layout: Option<&LayoutNode>,
        corners: WindowCorners,
        cx: &mut Context<Self>,
    ) {
        let mut by_pane = BTreeMap::new();
        if let Some(active_window) = active_window {
            if let Some(zoomed) = active_window.zoomed_pane {
                by_pane.insert(zoomed, corners);
            } else if let Some(layout) = layout {
                collect_pane_corners(layout, corners, &mut by_pane);
            }
        }

        for (pane, picker) in &self.pickers {
            let corners = by_pane.get(pane).copied().unwrap_or(WindowCorners::NONE);
            picker.update(cx, |picker, cx| {
                picker.set_window_corners(corners, cx);
            });
        }
        for (pane, terminal) in &self.terminals {
            let corners = by_pane.get(pane).copied().unwrap_or(WindowCorners::NONE);
            terminal.update(cx, |terminal, cx| {
                terminal.set_window_corners(corners, cx);
            });
        }
        for (pane, browser) in &self.browsers {
            let corners = by_pane.get(pane).copied().unwrap_or(WindowCorners::NONE);
            browser.update(cx, |browser, cx| {
                browser.set_window_corners(corners, cx);
            });
        }
        for (pane, agent) in &self.agents {
            let corners = by_pane.get(pane).copied().unwrap_or(WindowCorners::NONE);
            agent.update(cx, |agent, cx| {
                agent.set_window_corners(corners, cx);
            });
        }
        for (pane, editor) in &self.editors {
            let corners = by_pane.get(pane).copied().unwrap_or(WindowCorners::NONE);
            editor.update(cx, |editor, cx| {
                editor.set_window_corners(corners, cx);
            });
        }
        if let Some((pane, output)) = &self.command_output {
            let corners = by_pane.get(pane).copied().unwrap_or(WindowCorners::NONE);
            output.update(cx, |output, cx| {
                output.set_window_corners(corners, cx);
            });
        }
    }

    pub(super) fn synchronize_window_controls_pane(
        &mut self,
        pane: Option<PaneId>,
        cx: &mut Context<Self>,
    ) {
        self.window_controls_pane = pane;
        for (id, browser) in &self.browsers {
            let inset = window_controls_inset(
                Some(*id) == pane,
                zz_ui::browser::BrowserHeader::TAB_ROW_HEIGHT,
                cx,
            );
            browser.update(cx, |browser, cx| {
                browser.set_header_trailing_inset(inset, cx);
            });
        }
    }

    pub(super) fn pane_indicator(
        &self,
        indicator: &PaneIndicator,
        cx: &Context<Self>,
    ) -> Option<ala::Div> {
        let state = self.mux.read(cx).display_panes()?;
        let mux = self.mux.clone();
        let pane = indicator.pane;
        let chip = pane_indicator(("pane-indicator", pane.0), state, indicator, cx)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                mux.read(cx)
                    .send_input(zz_protocol::InputMessage::DisplayPanes {
                        action: DisplayPanesAction::Select(pane),
                    });
                cx.stop_propagation();
            });
        Some(pane_indicator_overlay(chip))
    }

    pub(super) fn float_cell_size(&self, window: &Window, cx: &App) -> Size<Pixels> {
        float_cell_size(self.mux.read(cx), window)
    }

    fn tile_size_report(
        &self,
        pane: PaneId,
        snapshot: Option<&zz_protocol::PaneSnapshot>,
    ) -> Option<AnyElement> {
        snapshot.filter(|snapshot| {
            matches!(
                snapshot.kind,
                PaneKindSnapshot::Agent(_)
                    | PaneKindSnapshot::View(_)
                    | PaneKindSnapshot::Browser(_)
                    | PaneKindSnapshot::Editor(_)
            ) && !self.terminals.contains_key(&pane)
                || self.simulators.contains_key(&pane)
        })?;
        let mux = self.mux.clone();
        let sent = Rc::clone(&self.tile_reports);
        let held = Rc::clone(&self.terminal_resize_suppressed);
        let moving = self.pane_motion.moving();
        Some(
            canvas(
                move |bounds, window, cx| {
                    if held.get() || moving {
                        return;
                    }
                    let mux = mux.read(cx);
                    let cell = float_cell_size(mux, window);
                    let grid = zz_ui::terminal::terminal_grid_size(
                        bounds.size,
                        cell.width,
                        cell.height,
                        window.scale_factor(),
                    );
                    let generation = mux.layout_generation();
                    let report = (grid.columns.max(1), grid.rows.max(1), generation);
                    if sent.borrow().get(&pane) == Some(&report) {
                        return;
                    }
                    sent.borrow_mut().insert(pane, report);
                    mux.send_input(InputMessage::ResizeTerminalV2 {
                        pane,
                        columns: report.0,
                        rows: report.1,
                        cell_width_px: grid.cell_width_px,
                        cell_height_px: grid.cell_height_px,
                        layout_generation: generation,
                    });
                },
                |_, (), _, _| {},
            )
            .absolute()
            .inset_0()
            .into_any_element(),
        )
    }
}
