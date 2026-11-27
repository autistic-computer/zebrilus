use super::*;

impl AppView {
    pub(super) fn reconcile_split_drag(
        &mut self,
        active_window: Option<&WindowSnapshot>,
        cx: &App,
    ) {
        let Some(mut drag) = self.split_drag else {
            return;
        };
        if drag.committed_snapshot_revision.is_none() && !cx.has_active_drag() {
            let ratio_basis_points = split_ratio_basis(drag.ratio);
            if ratio_basis_points == split_ratio_basis(drag.drag.start_ratio) {
                self.set_split_drag(None);
                return;
            }
            drag.ratio = f32::from(ratio_basis_points) / f32::from(SPLIT_RATIO_BASIS);
            drag.committed_snapshot_revision = Some(self.snapshot_revision);
            self.set_split_drag(Some(drag));
            self.mux
                .read(cx)
                .send_input(zz_protocol::InputMessage::ResizeSplit {
                    window: drag.drag.window,
                    split: drag.drag.split,
                    ratio_basis_points,
                });
        }
        if drag
            .committed_snapshot_revision
            .is_some_and(|revision| revision != self.snapshot_revision)
        {
            self.set_split_drag(None);
            return;
        }
        let Some(window) = active_window
            .filter(|window| window.id == drag.drag.window && window.zoomed_pane.is_none())
        else {
            self.set_split_drag(None);
            return;
        };
        if !window.layout.contains_split(drag.drag.split) {
            self.set_split_drag(None);
        }
    }

    pub(super) fn begin_float_drag(
        &mut self,
        window: WindowId,
        border_status: PaneBorderStatus,
        float: FloatingPaneSnapshot,
        grip: FloatGrip,
        cell: Size<Pixels>,
        press: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let mux = self.mux.read(cx);
        mux.execute(pane_select_command(float.pane));
        if self
            .float_drag
            .is_some_and(|drag| drag.committed_snapshot_revision.is_some())
        {
            return;
        }
        let snapshot = mux.snapshot();
        let window_snapshot = snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find(|snapshot| snapshot.id == window);
        let (columns, rows) = window_snapshot.map_or((0, 0), |snapshot| (snapshot.sx, snapshot.sy));
        let modal = window_snapshot
            .and_then(|snapshot| snapshot.modal)
            .is_some_and(|modal| modal.pane == float.pane);
        let landing = self
            .pip_landing
            .take_if(|landing| landing.window == window && landing.pane == float.pane);
        let start = landing.map_or_else(|| FloatCells::from(&float), |landing| landing.target);
        let layout = PipLayout {
            window: (columns, rows),
            cell,
            canvas: self.pane_canvas_bounds.get().size,
            lines: float.border_lines,
            header: cx.theme().pane_header_height(),
        };
        let shown = self
            .pane_motion
            .drawn(float.pane)
            .map_or_else(Point::default, |drawn| {
                drawn.origin - self.pane_canvas_bounds.get().origin - layout.frame(start).origin
            });
        let pip = (!modal && grip.is_move()).then(|| PipDrag::new(start, layout, press, shown));
        if pip.is_some() {
            self.pip_throw = PipThrow::default();
            self.pip_throw.track(cx.background_executor().now(), press);
        }
        self.float_drag = Some(FloatDragState {
            window,
            pane: float.pane,
            grip,
            lines: float.border_lines,
            area: FloatWindow {
                border_status,
                rows,
            },
            start,
            press,
            cell,
            delta: (0, 0),
            started: false,
            committed_snapshot_revision: None,
            request: None,
            pip,
            limit: (!modal).then_some(layout),
        });
    }

    pub(super) fn on_pip_grab_move(
        &mut self,
        event: &DragMoveEvent<PipGrab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = event.drag(cx).pane;
        let window_handle = window;
        if self.float_drag.is_none_or(|drag| drag.pane != pane) {
            let snapshot = self.mux.read(cx).snapshot();
            let Some((window, float)) = snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .find_map(|window| {
                    let float = window.floating.iter().find(|float| float.pane == pane)?;
                    Some((window, *float))
                })
            else {
                return;
            };
            let (id, border_status) = (window.id, window.pane_border_status);
            let cell = self.float_cell_size(window_handle, cx);
            self.begin_float_drag(
                id,
                border_status,
                float,
                FloatGrip::MOVE,
                cell,
                event.event.position,
                cx,
            );
        }
        let Some(drag) = self
            .float_drag
            .as_mut()
            .filter(|drag| drag.pane == pane && drag.committed_snapshot_revision.is_none())
        else {
            return;
        };
        let Some(pip) = drag.pip.as_mut() else {
            return;
        };
        drag.started = true;
        pip.moved(event.event.position);
        self.pip_throw
            .track(cx.background_executor().now(), event.event.position);
        cx.notify();
        cx.stop_propagation();
    }

    pub(super) fn on_float_drag_move(
        &mut self,
        event: &DragMoveEvent<FloatDrag>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = event.drag(cx).pane;
        let Some(drag) = self
            .float_drag
            .as_mut()
            .filter(|drag| drag.pane == pane && drag.committed_snapshot_revision.is_none())
        else {
            return;
        };
        if let Some(pip) = drag.pip.as_mut() {
            drag.started = true;
            pip.moved(event.event.position);
            self.pip_throw
                .track(cx.background_executor().now(), event.event.position);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        let offset = event.event.position - drag.press;
        let delta = (
            (offset.x / drag.cell.width).round() as i32,
            (offset.y / drag.cell.height).round() as i32,
        );
        drag.started = true;
        if drag.delta != delta {
            drag.delta = delta;
            cx.notify();
        }
        cx.stop_propagation();
    }

    pub(super) fn on_float_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left
            || self
                .float_drag
                .is_none_or(|drag| drag.committed_snapshot_revision.is_some())
        {
            return;
        }
        self.commit_float_drag(cx);
        cx.notify();
    }

    fn commit_float_drag(&mut self, cx: &App) {
        let Some(mut drag) = self.float_drag else {
            return;
        };
        if drag.throwing() {
            self.float_drag = None;
            if let Some(pip) = drag.thrown() {
                self.throw_pip(drag.window, drag.pane, pip, cx);
            }
            return;
        }
        let target = drag.target();
        if !drag.started || target == drag.start {
            self.float_drag = None;
            return;
        }
        drag.committed_snapshot_revision = Some(self.snapshot_revision);
        drag.request = self.mux.read(cx).execute_tracked(float_drag_command(
            drag.pane, drag.grip, drag.start, target, drag.lines, drag.area,
        ));
        self.float_drag = drag.request.is_some().then_some(drag);
    }

    fn throw_pip(&mut self, window: WindowId, pane: PaneId, pip: PipDrag, cx: &App) {
        let velocity = self.pip_throw.velocity(cx.background_executor().now());
        let landing = pip.release(velocity);
        if landing.stash.is_some() {
            self.pip_stage.stash(pane);
            let mux = self.mux.read(cx);
            if let Some(parent) = mux.agent_parent(mux.attached_host(), pane) {
                mux.execute(pane_select_command(parent));
            }
        }
        self.pane_motion
            .fling(pane, velocity, landing.stash.is_none());
        let request = self
            .mux
            .read(cx)
            .execute_tracked(landing.place.command(pane));
        self.pip_landing = Some(PipLandingState {
            window,
            pane,
            target: landing.target,
            request,
        });
    }

    pub(super) fn settle_pip_landing(&mut self, cx: &App) {
        if self.pip_landing.is_some_and(|landing| {
            landing
                .request
                .is_none_or(|request| self.mux.read(cx).command_settled(request))
        }) {
            self.pip_landing = None;
        }
    }

    pub(super) fn reconcile_float_drag(
        &mut self,
        active_window: Option<&WindowSnapshot>,
        cx: &App,
    ) {
        let Some(drag) = self.float_drag else {
            return;
        };
        if drag.committed_snapshot_revision.is_none() && drag.started && !cx.has_active_drag() {
            self.commit_float_drag(cx);
        }
        let Some(drag) = self.float_drag else {
            return;
        };
        let current = active_window
            .filter(|window| window.id == drag.window)
            .and_then(|window| {
                window
                    .floating
                    .iter()
                    .find(|float| float.pane == drag.pane && float.visible)
            })
            .map(FloatCells::from);
        let Some(current) = current else {
            self.float_drag = None;
            return;
        };
        if drag
            .committed_snapshot_revision
            .is_some_and(|revision| revision != self.snapshot_revision)
            && drag.halfway() != Some(current)
        {
            self.float_drag = None;
        }
    }

    pub(super) fn set_split_drag(&mut self, split_drag: Option<SplitDragState>) {
        self.split_drag = split_drag;
        self.terminal_resize_suppressed
            .set(self.split_drag.is_some() || self.terminal_resizes_held);
    }

    pub(crate) fn hold_terminal_resizes(&mut self, hold: bool, cx: &mut Context<Self>) {
        self.terminal_resizes_held = hold;
        self.terminal_resize_suppressed
            .set(self.split_drag.is_some() || hold);
        if !hold {
            for terminal in self.terminals.values() {
                terminal.update(cx, |_, cx| cx.notify());
            }
        }
    }

    pub(super) fn reconcile_pane_layout_override(
        &mut self,
        active_window: Option<&WindowSnapshot>,
        generation: u64,
    ) {
        if !self
            .pane_layout_override
            .as_ref()
            .is_some_and(|pending| pending.still_predicts(active_window, generation))
        {
            self.pane_layout_override = None;
        }
    }

    pub(super) fn reconcile_pane_drag(
        &mut self,
        active_window: Option<&WindowSnapshot>,
        prefix_armed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.pane_drag.as_ref() else {
            return;
        };
        if !cx.has_active_drag() {
            let drag = take_pane_drag(&mut self.pane_drag).unwrap();
            self.finish_pane_drag(drag, cx);
            return;
        }
        let valid_window = active_window.is_some_and(|window| drag.matches(window, prefix_armed));
        if !valid_window {
            self.pane_drag = None;
            cx.stop_active_drag(window);
            cx.notify();
        }
    }

    fn pane_drag_state(&self, source: PaneId, cx: &App) -> Option<PaneDragState> {
        let mux = self.mux.read(cx);
        let attached = mux.attached_session()?;
        let snapshot = mux.snapshot();
        let session = snapshot
            .sessions
            .iter()
            .find(|session| session.id == attached)?;
        let focused_window = snapshot.focused_window_for(session);
        let window = session
            .windows
            .iter()
            .find(|window| window.id == focused_window)?;
        PaneDragState::new(
            source,
            window.id,
            &window.layout,
            self.pane_canvas_bounds.get().size,
            pane_split_slot(config::pane_margin(cx)),
        )
    }

    pub(super) fn on_pane_drag_start(&mut self, payload: PaneDrag, cx: &mut Context<Self>) {
        let pane = payload.pane;
        if self
            .pane_drag
            .as_ref()
            .is_some_and(|drag| drag.source == pane)
        {
            return;
        }
        if let Some(mut drag) = self.pane_drag_state(pane, cx) {
            drag.requires_prefix = payload.requires_prefix;
            self.pane_drop_preview.set(DropPreviewFrame::default());
            self.pane_drag = Some(drag);
            cx.notify();
        }
    }

    pub(super) fn on_pane_drag_move(
        &mut self,
        event: &DragMoveEvent<PaneDrag>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = event.drag(cx).pane;
        let position = event.event.position - event.bounds.origin;
        let rendered = self.pane_drop_preview.get();
        let Some(drag) = self.pane_drag.as_mut().filter(|drag| drag.source == source) else {
            return;
        };
        let target = drag.target_at(position);
        if drag.set_target(target, rendered) {
            cx.notify();
        }
    }

    pub(super) fn on_pane_drop(
        &mut self,
        drag: PaneDrag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut state) = self.pane_drag.take_if(|state| state.source == drag.pane) else {
            return;
        };
        state.target =
            state.target_at(window.mouse_position() - self.pane_canvas_bounds.get().origin);
        if let Some((target, zone)) = state.target
            && let Some(command) = pane_drop_command(state.source, target, zone)
        {
            self.pane_layout_override =
                predicted_drop_layout(&state.layout, state.source, target, zone).map(|layout| {
                    PaneLayoutOverride {
                        window: state.window,
                        layout,
                        generation: self.mux.read(cx).snapshot().generation,
                    }
                });
            self.mux.read(cx).execute(command);
        }
        self.finish_pane_drag(drag, cx);
    }

    fn finish_pane_drag(&self, drag: PaneDrag, cx: &mut Context<Self>) {
        if drag.requires_prefix && self.mux.read(cx).prefix_armed() {
            self.dismiss_armed_prefix(drag.pane, cx);
        }
        cx.notify();
    }

    pub(super) fn on_pane_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left || self.pane_drag.is_none() {
            return;
        }
        cx.defer_in(window, |view, _window, cx| {
            let Some(source) = take_pane_drag(&mut view.pane_drag) else {
                return;
            };
            view.finish_pane_drag(source, cx);
        });
    }

    pub(super) fn on_pane_mouse_exit(
        &mut self,
        _: &MouseExitEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rendered = self.pane_drop_preview.get();
        if self
            .pane_drag
            .as_mut()
            .is_some_and(|drag| drag.set_target(None, rendered))
        {
            cx.notify();
        }
    }

    pub(super) fn on_split_drag_move(
        &mut self,
        event: &DragMoveEvent<SplitDrag>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let drag = *event.drag(cx);
        if self.split_drag.is_some_and(|state| {
            state.drag.split == drag.split && state.committed_snapshot_revision.is_some()
        }) {
            return;
        }
        let ratio = split_ratio_from_pointer(drag.axis, event.event.position, event.bounds);
        if self.split_drag.is_none_or(|state| {
            state.drag.split != drag.split || state.ratio.to_bits() != ratio.to_bits()
        }) {
            self.set_split_drag(Some(SplitDragState {
                drag,
                ratio,
                committed_snapshot_revision: None,
            }));
            cx.notify();
        }
        cx.stop_propagation();
    }

    pub(super) fn on_split_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        let Some(mut state) = self.split_drag else {
            return;
        };
        let ratio_basis_points = split_ratio_basis(state.ratio);
        if ratio_basis_points == split_ratio_basis(state.drag.start_ratio) {
            self.set_split_drag(None);
        } else {
            state.ratio = f32::from(ratio_basis_points) / f32::from(SPLIT_RATIO_BASIS);
            state.committed_snapshot_revision = Some(self.snapshot_revision);
            self.set_split_drag(Some(state));
            self.mux
                .read(cx)
                .send_input(zz_protocol::InputMessage::ResizeSplit {
                    window: state.drag.window,
                    split: state.drag.split,
                    ratio_basis_points,
                });
        }
        cx.notify();
        cx.stop_propagation();
    }

    pub(super) fn pane_drag_handle(
        pane: PaneId,
        title: String,
        state: PaneDragOverlayState,
        radii: Corners<Pixels>,
        cx: &mut Context<Self>,
    ) -> ala::Stateful<ala::Div> {
        let app_view = cx.entity();
        pane_drag_overlay(("pane-drag-overlay", pane.0), state, radii, cx)
            .on_click(cx.listener(move |view, _, _, cx| {
                view.on_pane_click(pane, cx);
            }))
            .on_drag(
                PaneDrag {
                    pane,
                    requires_prefix: true,
                },
                move |drag, grab, _, cx| {
                    app_view.update(cx, |view, cx| {
                        view.on_pane_drag_start(*drag, cx);
                    });
                    pane_drag_preview(drag.pane, title.clone(), grab, cx)
                },
            )
            .on_drop(cx.listener(move |view, drag: &PaneDrag, window, cx| {
                view.on_pane_drop(*drag, window, cx);
            }))
    }

    pub(super) fn pane_drop_preview_element(
        &self,
        drag: &PaneDragState,
        cx: &App,
    ) -> Option<AnyElement> {
        let preview = drag.preview?;
        let radius = config::pane_content_radii(cx, WindowCorners::NONE).top_left;
        let border = PaneLook::global(cx).edge_width(cx.theme()).max(px(1.0));
        let rendered = self.pane_drop_preview.clone();
        Some(
            pane_drop_preview("pane-drop-preview", radius, border, cx)
                .with_animation(
                    ("pane-drop-preview", preview.sequence),
                    Animation::new(preview.duration).with_easing(ease_out_quint()),
                    move |element, delta| {
                        let frame = preview.at(delta);
                        rendered.set(frame);
                        element
                            .left(frame.bounds.origin.x)
                            .top(frame.bounds.origin.y)
                            .w(frame.bounds.size.width)
                            .h(frame.bounds.size.height)
                            .opacity(frame.opacity)
                    },
                )
                .into_any_element(),
        )
    }
}

impl FloatDragState {
    fn target(&self) -> FloatCells {
        let target = float_drag_preview(self.start, self.grip, self.delta);
        self.limit
            .map_or(target, |layout| layout.within(target, self.grip))
    }

    pub(super) fn preview(&self) -> Option<FloatCells> {
        self.started.then(|| self.target())
    }

    pub(super) const fn throwing(&self) -> bool {
        self.pip.is_some()
    }

    pub(super) fn thrown(&self) -> Option<PipDrag> {
        self.pip.filter(|_| self.started)
    }

    fn halfway(&self) -> Option<FloatCells> {
        (!self.grip.is_move() && self.grip.moves_origin()).then(|| FloatCells {
            xoff: self.start.xoff,
            yoff: self.start.yoff,
            ..self.target()
        })
    }
}

impl PaneDragState {
    pub(super) fn new(
        source: PaneId,
        window: WindowId,
        layout: &LayoutNode,
        canvas_size: Size<Pixels>,
        divider: Pixels,
    ) -> Option<Self> {
        if canvas_size.width <= px(0.0) || canvas_size.height <= px(0.0) {
            return None;
        }
        let slots = pane_rects(layout)
            .into_iter()
            .map(|(pane, rect)| (pane, pane_bounds(rect, canvas_size)))
            .collect::<Vec<_>>();
        slots.iter().any(|(pane, _)| *pane == source).then(|| Self {
            source,
            requires_prefix: true,
            window,
            layout: layout.clone(),
            slots,
            target: None,
            preview: None,
            divider,
        })
    }

    pub(super) fn target_at(&self, position: Point<Pixels>) -> Option<(PaneId, DropZone)> {
        let (target, slot) = self
            .slots
            .iter()
            .find(|(pane, slot)| *pane != self.source && slot.contains(&position))?;
        let zone = drop_zone_at(
            PaneRect {
                x: slot.origin.x.into(),
                y: slot.origin.y.into(),
                width: slot.size.width.into(),
                height: slot.size.height.into(),
            },
            (position.x.into(), position.y.into()),
        );
        Some((
            *target,
            coerced_drop_zone(&self.layout, self.source, *target, zone),
        ))
    }

    pub(super) fn set_target(
        &mut self,
        target: Option<(PaneId, DropZone)>,
        rendered: DropPreviewFrame,
    ) -> bool {
        let target = target.filter(|(target, _)| {
            *target != self.source && self.slots.iter().any(|(pane, _)| pane == target)
        });
        if self.target == target {
            return false;
        }
        self.target = target;
        let sequence = self.preview.map_or(0, |preview| preview.sequence + 1);
        self.preview = Some(match target {
            Some((pane, zone)) => {
                let to = self.slot(pane).map_or(rendered.bounds, |slot| {
                    let rect = drop_preview_bounds(
                        PaneRect {
                            x: slot.origin.x.into(),
                            y: slot.origin.y.into(),
                            width: slot.size.width.into(),
                            height: slot.size.height.into(),
                        },
                        zone,
                        self.divider.into(),
                    );
                    Bounds::new(
                        ala::point(px(rect.x), px(rect.y)),
                        ala::size(px(rect.width), px(rect.height)),
                    )
                });
                DropPreview {
                    from: if rendered.opacity > 0.0 {
                        rendered
                    } else {
                        DropPreviewFrame {
                            bounds: to,
                            opacity: 0.0,
                        }
                    },
                    to: DropPreviewFrame {
                        bounds: to,
                        opacity: 1.0,
                    },
                    sequence,
                    duration: DROP_PREVIEW_MORPH,
                }
            }
            None => DropPreview {
                from: rendered,
                to: DropPreviewFrame {
                    bounds: rendered.bounds,
                    opacity: 0.0,
                },
                sequence,
                duration: DROP_PREVIEW_FADE,
            },
        });
        true
    }

    pub(super) fn slot(&self, pane: PaneId) -> Option<Bounds<Pixels>> {
        self.slots
            .iter()
            .find_map(|(candidate, rect)| (*candidate == pane).then_some(*rect))
    }

    pub(super) fn matches_layout(&self, window: WindowId, layout: &LayoutNode) -> bool {
        self.window == window && self.layout == *layout
    }

    pub(super) fn matches(&self, window: &WindowSnapshot, prefix_armed: bool) -> bool {
        (!self.requires_prefix || prefix_armed)
            && window.zoomed_pane.is_none()
            && self.slots.len() > 1
            && self.matches_layout(window.id, &window.layout)
            && self
                .slots
                .iter()
                .all(|(pane, _)| window.panes.contains_key(pane))
    }
}

impl PaneLayoutOverride {
    pub(super) fn still_predicts(
        &self,
        active_window: Option<&WindowSnapshot>,
        generation: u64,
    ) -> bool {
        self.generation == generation
            && active_window
                .is_some_and(|window| window.id == self.window && window.zoomed_pane.is_none())
    }
}

pub(super) fn take_pane_drag(state: &mut Option<PaneDragState>) -> Option<PaneDrag> {
    let state = state.take()?;
    Some(PaneDrag {
        pane: state.source,
        requires_prefix: state.requires_prefix,
    })
}

pub(super) fn split_ratio_from_pointer(
    axis: Axis,
    pointer: Point<Pixels>,
    bounds: Bounds<Pixels>,
) -> f32 {
    let (offset, extent) = match axis {
        Axis::Horizontal => (
            f32::from(pointer.x - bounds.origin.x),
            f32::from(bounds.size.width),
        ),
        Axis::Vertical => (
            f32::from(pointer.y - bounds.origin.y),
            f32::from(bounds.size.height),
        ),
    };
    if extent <= 0.0 {
        return 0.5;
    }
    (offset / extent).clamp(0.0, 1.0)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the clamped ratio is converted to a bounded protocol fixed-point value"
)]
pub(super) fn split_ratio_basis(ratio: f32) -> u16 {
    (ratio.clamp(0.0, 1.0) * f32::from(SPLIT_RATIO_BASIS)).round() as u16
}
