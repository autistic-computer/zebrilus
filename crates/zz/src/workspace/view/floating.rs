use super::*;

impl AppView {
    pub(super) fn floating_layer(
        &self,
        window_snapshot: &WindowSnapshot,
        cell: Size<Pixels>,
        canvas: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let floats = window_snapshot
            .floating
            .iter()
            .filter(|float| float.visible && !self.pip_stage.hidden(float.pane))
            .collect::<Vec<_>>();
        let cards = floats
            .iter()
            .filter(|float| {
                window_snapshot
                    .modal
                    .is_none_or(|modal| modal.pane != float.pane)
            })
            .filter_map(|float| {
                let (frame, held) = self.float_frame(window_snapshot, float, cell, canvas, cx)?;
                Some(PipCard {
                    pane: float.pane,
                    frame,
                    held,
                })
            })
            .collect::<Vec<_>>();
        let placed = self
            .pip_stage
            .arrange(&cards, canvas, cx.theme().pane_header_height());
        zz_ui::pip::show_pips(cx.entity_id(), &placed, cx);
        let placed = placed
            .into_iter()
            .map(|placed| (placed.pane, placed))
            .collect::<BTreeMap<_, _>>();
        let mut layer = Vec::new();
        for float in floats.into_iter().rev() {
            if let Some(modal) = window_snapshot
                .modal
                .filter(|modal| modal.pane == float.pane)
            {
                layer.push(self.modal_scrim(modal, cx));
            }
            layer.extend(self.floating_pane(
                window_snapshot,
                float,
                cell,
                canvas,
                placed.get(&float.pane).copied(),
                cx,
            ));
        }
        for (pane, rest) in self.pane_motion.departures() {
            let Some(departing) = self.departing.get(&pane) else {
                continue;
            };
            let card = zz_ui::pip::departing_pip(
                pane,
                penna::Icon::new(departing.face.icon.clone()),
                departing.face.label.clone().into(),
                departing.view.clone(),
                departing.face.inset,
                cx,
            );
            layer.push(
                div()
                    .absolute()
                    .left(rest.origin.x)
                    .top(rest.origin.y)
                    .w(rest.size.width)
                    .h(rest.size.height)
                    .child(moving_pane(pane, &self.pane_motion, card))
                    .into_any_element(),
            );
        }
        (!layer.is_empty()).then(|| {
            div()
                .absolute()
                .inset_0()
                .overflow_hidden()
                .children(layer)
                .into_any_element()
        })
    }

    fn modal_scrim(&self, modal: ModalPaneSnapshot, cx: &App) -> AnyElement {
        let mux = self.mux.clone();
        div()
            .id(("floating-modal-scrim", modal.pane.0))
            .debug_selector(|| "floating-modal-scrim".to_owned())
            .absolute()
            .inset_0()
            .bg(cx.theme().scrim)
            .occlude()
            .on_any_mouse_down(move |_, _, cx| {
                if modal.close_on_click {
                    mux.read(cx).execute(CommandInvocation::new(
                        "kill-pane",
                        ["-t", &modal.pane.to_string()],
                    ));
                }
                cx.stop_propagation();
            })
            .into_any_element()
    }

    fn float_frame(
        &self,
        window_snapshot: &WindowSnapshot,
        float: &FloatingPaneSnapshot,
        cell: Size<Pixels>,
        canvas: Size<Pixels>,
        cx: &App,
    ) -> Option<(Bounds<Pixels>, bool)> {
        let pane = float.pane;
        let modal = window_snapshot
            .modal
            .is_some_and(|modal| modal.pane == pane);
        let drag = self
            .float_drag
            .filter(|drag| drag.window == window_snapshot.id && drag.pane == pane);
        let throw = drag.and_then(|drag| drag.thrown());
        let landing = self
            .pip_landing
            .filter(|landing| landing.window == window_snapshot.id && landing.pane == pane);
        let cells = match (throw, landing) {
            (Some(pip), _) => pip.start,
            (None, Some(landing)) => landing.target,
            (None, None) => drag
                .filter(|drag| !drag.throwing())
                .and_then(|drag| drag.preview())
                .unwrap_or_else(|| FloatCells::from(float)),
        };
        let bordered = float.border_lines != PaneBorderLines::None;
        if canvas.width > Pixels::ZERO
            && canvas.height > Pixels::ZERO
            && float_pixels(
                cells,
                bordered,
                (f32::from(cell.width), f32::from(cell.height)),
                (f32::from(canvas.width), f32::from(canvas.height)),
            )
            .is_none()
        {
            return None;
        }
        let pad = i32::from(bordered);
        let mut frame = if modal {
            Bounds::new(
                point(
                    cell.width * (cells.xoff - pad) as f32,
                    cell.height * (cells.yoff - pad) as f32,
                ),
                ala::size(
                    cell.width * (i32::from(cells.sx) + 2 * pad) as f32,
                    cell.height * (i32::from(cells.sy) + 2 * pad) as f32,
                ),
            )
        } else {
            PipLayout {
                window: (window_snapshot.sx, window_snapshot.sy),
                cell,
                canvas,
                lines: float.border_lines,
                header: cx.theme().pane_header_height(),
            }
            .frame(cells)
        };
        if let Some(pip) = throw {
            frame.origin = pip.drawn_origin();
        }
        Some((frame, throw.is_some()))
    }

    fn floating_pane(
        &self,
        window_snapshot: &WindowSnapshot,
        float: &FloatingPaneSnapshot,
        cell: Size<Pixels>,
        canvas: Size<Pixels>,
        placed: Option<PipPlaced>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pane = float.pane;
        let modal = window_snapshot
            .modal
            .is_some_and(|modal| modal.pane == pane);
        let (mut frame, held) = self.float_frame(window_snapshot, float, cell, canvas, cx)?;
        if let Some(placed) = placed {
            frame = placed.frame;
        }
        let stashed = placed.and_then(|placed| placed.stashed);
        let bordered = float.border_lines != PaneBorderLines::None;
        let active = pane == window_snapshot.active_pane;
        let snapshot = window_snapshot.panes.get(&pane);
        if let Some(terminal) = self.terminals.get(&pane) {
            let dead = dead_label(snapshot);
            let synchronized = snapshot.is_some_and(|pane| pane.synchronized_input);
            terminal.update(cx, |terminal, cx| {
                terminal.set_pane_status(dead, synchronized, cx);
            });
        }
        let inactive_opacity = config::pane_inactive_opacity(cx);
        let content = self
            .pane_content(pane, !active, inactive_opacity, cx)
            .map_or_else(
                || div().size_full().into_any_element(),
                |content| content.element(),
            );
        let title = snapshot
            .filter(|_| float.border_status.is_on())
            .map(|pane| float_title(&pane.border_status_text))
            .unwrap_or_default();
        let inset = if bordered { cell } else { Size::default() };
        let grips = if bordered && stashed.is_none() {
            self.float_grips(
                window_snapshot.id,
                window_snapshot.pane_border_status,
                *float,
                cell,
                frame.size,
                !modal,
                cx,
            )
        } else {
            Vec::new()
        };
        let surface = if modal {
            FloatingSurface::new(("floating-pane", pane.0), content, cx)
                .title(title.left)
                .title_right(title.right)
                .content_inset(inset.width, inset.height)
                .edge_color(pane_border_color(active, cx))
                .bordered(bordered)
                .into_any_element()
        } else {
            let own_header =
                snapshot.is_some_and(|pane| matches!(pane.kind, PaneKindSnapshot::Browser(_)));
            let fill = self.terminals.get(&pane).map_or_else(
                || pane_color(cx.theme(), cx.theme().background),
                |terminal| terminal.read(cx).fill(cx),
            );
            let surface = PipSurface::new(("floating-pane", pane.0), content)
                .bordered(bordered)
                .fill(fill)
                .stashed(
                    stashed.is_some(),
                    cx.listener(move |view, _: &ala::ClickEvent, _, cx| {
                        view.pip_stage.restore(pane);
                        view.mux.read(cx).execute(pane_select_command(pane));
                        cx.notify();
                    }),
                );
            if own_header {
                surface.into_any_element()
            } else {
                let label = if title.left.is_empty() {
                    snapshot
                        .map(|pane| zz_client::window_summary::pane_heading(pane).title)
                        .unwrap_or_default()
                } else {
                    title.left
                };
                let opacity = if active { 1.0 } else { inactive_opacity };
                if self.pane_motion.launched(pane) {
                    self.pip_faces.borrow_mut().insert(
                        pane,
                        PipFace {
                            label: label.clone(),
                            icon: snapshot.map_or(penna::IconName::SquareTerminal, |pane| {
                                zz_ui::pane::pane_kind_icon(&pane.kind)
                            }),
                            inset,
                        },
                    );
                }
                surface
                    .content_inset(inset.width, inset.height)
                    .header(self.pip_header(pane, snapshot, label, active, opacity, cx))
                    .into_any_element()
            }
        };
        let card = div()
            .relative()
            .size_full()
            .child(surface)
            .children(grips)
            .children(
                self.pane_indicators
                    .get(&pane)
                    .and_then(|indicator| self.pane_indicator(indicator, cx)),
            );
        Some(
            div()
                .absolute()
                .left(frame.origin.x)
                .top(frame.origin.y)
                .w(frame.size.width)
                .h(frame.size.height)
                .debug_selector(move || format!("floating-pane-{}", pane.0))
                .key_context(pane_key_context(pane))
                .map(|slot| {
                    if modal {
                        slot.child(card)
                    } else {
                        slot.child(
                            moving_pane(pane, &self.pane_motion, card)
                                .held(held)
                                .launches(),
                        )
                    }
                })
                .into_any_element(),
        )
    }

    fn pip_header(
        &self,
        pane: PaneId,
        snapshot: Option<&zz_protocol::PaneSnapshot>,
        label: String,
        active: bool,
        opacity: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connected = self.mux.read(cx).is_connected();
        let program =
            snapshot.and_then(|pane| zz_client::window_summary::pane_heading(pane).program);
        if let Some(header_view) = self.terminal_headers.get(&pane) {
            let simulator = self.simulators.get(&pane).map(|view| view.read(cx).title());
            let state = TerminalHeaderState {
                icon: if simulator.is_some() {
                    crate::simulator::PHONE
                } else {
                    snapshot.map_or(penna::IconName::SquareTerminal, |pane| {
                        zz_ui::pane::pane_kind_icon(&pane.kind)
                    })
                },
                program,
                active,
                title: simulator.map_or(label, Into::into),
                can_drag: connected,
                place: zz_ui::pane::PaneState {
                    floats: true,
                    ..zz_ui::pane::PaneState::default()
                },
                folder: snapshot.and_then(|snapshot| self.pane_folder(snapshot, cx)),
                flip: snapshot.and_then(|snapshot| self.pane_flip(snapshot)),
                keys: self.pane_action_keys(cx),
                buttons: self.pane_kind_buttons(pane, cx),
                menu: self.pane_title_menu(pane, cx),
                trailing_inset: px(0.0),
                radii: Corners::default(),
                opacity,
            };
            header_view.update(cx, |header, cx| header.sync(state, cx));
            return AnyView::from(header_view.clone())
                .cached(
                    StyleRefinement::default()
                        .h(cx.theme().pane_header_height())
                        .w_full(),
                )
                .into_any_element();
        }
        let mux = self.mux.clone();
        let icon = snapshot.map_or(penna::IconName::SquareTerminal, |pane| {
            zz_ui::pane::pane_kind_icon(&pane.kind)
        });
        div()
            .opacity(opacity)
            .child(zz_ui::pane::pane_header(
                zz_ui::pane::pane_heading_icon(icon, program),
                active,
                label.clone(),
                snapshot
                    .and_then(|snapshot| self.pane_folder(snapshot, cx))
                    .map(Into::into),
                PaneDragSource::new(pane, label, connected, |_, _, _| {}),
                zz_ui::pane::PaneState {
                    floats: true,
                    ..zz_ui::pane::PaneState::default()
                },
                None,
                &[],
                &self.pane_action_keys(cx),
                px(0.0),
                move |action, _, cx| {
                    for command in action.commands(pane, None) {
                        mux.read(cx).execute(command);
                    }
                },
                cx,
            ))
            .into_any_element()
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "a grip needs the window, its status row, the float and its frame"
    )]
    fn float_grips(
        &self,
        window: WindowId,
        border_status: PaneBorderStatus,
        float: FloatingPaneSnapshot,
        cell: Size<Pixels>,
        frame: Size<Pixels>,
        pip: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let (edge, near) = if pip {
            (ala::size(px(PIP_GRIP), px(PIP_GRIP)), px(-PIP_GRIP / 2.0))
        } else {
            (cell, Pixels::ZERO)
        };
        zz_ui::pip::float_grip_areas(frame, edge, point(near, near), pip)
            .into_iter()
            .map(
                |zz_ui::pip::FloatGripArea {
                     name,
                     grip,
                     bounds,
                     cursor,
                 }| {
                    div()
                        .id(ala::SharedString::from(format!(
                            "floating-grip-{}-{name}",
                            float.pane.0
                        )))
                        .debug_selector(move || format!("floating-grip-{}-{name}", float.pane.0))
                        .absolute()
                        .occlude()
                        .left(bounds.origin.x)
                        .top(bounds.origin.y)
                        .w(bounds.size.width)
                        .h(bounds.size.height)
                        .when_some(cursor, ala::Styled::cursor)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                                view.begin_float_drag(
                                    window,
                                    border_status,
                                    float,
                                    grip,
                                    cell,
                                    event.position,
                                    cx,
                                );
                            }),
                        )
                        .on_drag(FloatDrag { pane: float.pane }, |_, _, _, cx| {
                            cx.new(|_| SplitDragPreview)
                        })
                        .into_any_element()
                },
            )
            .collect()
    }
}
