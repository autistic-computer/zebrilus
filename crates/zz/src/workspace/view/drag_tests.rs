use std::cell::RefCell;

use super::test_support::*;
use super::tests::*;
use super::*;
use crate::terminal::view::GridSize;
use ala::{Modifiers, TestAppContext, point};

fn three_pane_layout() -> LayoutNode {
    LayoutNode::Split {
        id: SplitId(1),
        axis: Axis::Horizontal,
        ratio: 0.4,
        first: Box::new(LayoutNode::Pane(PaneId(3))),
        second: Box::new(LayoutNode::Split {
            id: SplitId(2),
            axis: Axis::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Pane(PaneId(7))),
            second: Box::new(LayoutNode::Pane(PaneId(9))),
        }),
    }
}

fn three_pane_drag(source: PaneId) -> PaneDragState {
    PaneDragState::new(
        source,
        WindowId(5),
        &three_pane_layout(),
        ala::size(px(1_000.0), px(800.0)),
        px(8.0),
    )
    .expect("source belongs to the frozen layout")
}

#[test]
fn pane_drag_state_freezes_slots_and_rejects_foreign_layouts() {
    let layout = three_pane_layout();
    let canvas_size = ala::size(px(1_000.0), px(800.0));
    let frozen_slots = pane_rects(&layout)
        .into_iter()
        .map(|(pane, rect)| (pane, pane_bounds(rect, canvas_size)))
        .collect::<Vec<_>>();
    let drag = three_pane_drag(PaneId(3));

    assert_eq!(drag.slots, frozen_slots);
    assert_eq!(
        drag.slot(PaneId(3)),
        Some(Bounds::new(
            point(px(0.0), px(0.0)),
            ala::size(px(400.0), px(800.0)),
        ))
    );
    assert_eq!(
        drag.slot(PaneId(7)),
        Some(Bounds::new(
            point(px(400.0), px(0.0)),
            ala::size(px(600.0), px(400.0)),
        ))
    );
    assert!(drag.matches_layout(WindowId(5), &layout));
    assert!(!drag.matches_layout(WindowId(6), &layout));
    let changed_layout = LayoutNode::Split {
        id: SplitId(1),
        axis: Axis::Horizontal,
        ratio: 0.6,
        first: Box::new(LayoutNode::Pane(PaneId(3))),
        second: Box::new(LayoutNode::Split {
            id: SplitId(2),
            axis: Axis::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Pane(PaneId(7))),
            second: Box::new(LayoutNode::Pane(PaneId(9))),
        }),
    };
    assert!(!drag.matches_layout(WindowId(5), &changed_layout));
}

#[test]
fn pointer_targets_skip_the_source_and_carry_the_hovered_zone() {
    let drag = three_pane_drag(PaneId(3));

    assert_eq!(drag.target_at(point(px(200.0), px(400.0))), None);
    assert_eq!(drag.target_at(point(px(2_000.0), px(400.0))), None);
    assert_eq!(
        drag.target_at(point(px(950.0), px(200.0))),
        Some((PaneId(7), DropZone::Right))
    );
    assert_eq!(
        drag.target_at(point(px(700.0), px(40.0))),
        Some((PaneId(7), DropZone::Top))
    );
    assert_eq!(
        drag.target_at(point(px(450.0), px(600.0))),
        Some((PaneId(9), DropZone::Left))
    );
    let stacked = three_pane_drag(PaneId(7));
    assert_eq!(
        stacked.target_at(point(px(700.0), px(440.0))),
        Some((PaneId(9), DropZone::Center))
    );
}

#[test]
fn drop_previews_halve_the_target_slot_and_morph_from_what_is_on_screen() {
    let mut drag = three_pane_drag(PaneId(3));
    let blank = DropPreviewFrame::default();

    assert!(!drag.set_target(Some((PaneId(3), DropZone::Center)), blank));
    assert!(!drag.set_target(Some((PaneId(11), DropZone::Center)), blank));
    assert!(drag.set_target(Some((PaneId(7), DropZone::Right)), blank));

    let right_half = Bounds::new(point(px(712.0), px(0.0)), ala::size(px(288.0), px(400.0)));
    let preview = drag.preview.expect("a target shows a preview");
    assert_eq!(preview.to.bounds, right_half);
    assert_eq!(preview.to.opacity.to_bits(), 1.0_f32.to_bits());
    assert_eq!(preview.from.bounds, right_half);
    assert_eq!(preview.from.opacity.to_bits(), 0.0_f32.to_bits());
    assert_eq!(preview.duration, DROP_PREVIEW_MORPH);
    assert!(!drag.set_target(Some((PaneId(7), DropZone::Right)), blank));

    let mid_morph = DropPreviewFrame {
        bounds: Bounds::new(point(px(640.0), px(20.0)), ala::size(px(320.0), px(380.0))),
        opacity: 0.6,
    };
    assert!(drag.set_target(Some((PaneId(9), DropZone::Center)), mid_morph));
    let preview = drag.preview.expect("a target shows a preview");
    assert_eq!(preview.from, mid_morph);
    assert_eq!(
        preview.to.bounds,
        Bounds::new(point(px(408.0), px(408.0)), ala::size(px(592.0), px(392.0)))
    );
    assert_eq!(preview.sequence, 1);

    let showing = DropPreviewFrame {
        bounds: preview.to.bounds,
        opacity: 1.0,
    };
    assert!(drag.set_target(None, showing));
    let preview = drag.preview.expect("the fade-out is still a preview");
    assert_eq!(preview.from, showing);
    assert_eq!(preview.to.bounds, showing.bounds);
    assert_eq!(preview.to.opacity.to_bits(), 0.0_f32.to_bits());
    assert_eq!(preview.duration, DROP_PREVIEW_FADE);
    assert_eq!(preview.at(0.5).opacity.to_bits(), 0.5_f32.to_bits());
}

#[ala::test]
fn committed_split_drag_lives_until_same_generation_snapshot_arrives(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let controller = cx.new(|cx| {
            crate::browser::controller::BrowserController::new(
                Err(ala_cef::BrowserError::AlreadyShutdown),
                cx,
            )
        });
        let agent_controller = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "test client".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        captured_mux.replace(Some(mux.clone()));
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
    let snapshot = two_pane_snapshot(PaneId(2));
    let generation = snapshot.generation;
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
    });
    cx.run_until_parked();

    workspace.update(cx, |workspace, cx| {
        workspace.set_split_drag(Some(SplitDragState {
            drag: SplitDrag {
                window: WindowId(0),
                split: SplitId(0),
                start_ratio: 0.5,
                axis: Axis::Horizontal,
            },
            ratio: 0.6,
            committed_snapshot_revision: Some(workspace.snapshot_revision),
        }));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |workspace, _| workspace.split_drag.is_some()));

    let acknowledged = two_pane_snapshot(PaneId(2));
    assert_eq!(acknowledged.generation, generation);
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), acknowledged, cx);
    });
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |workspace, _| workspace.split_drag.is_none()));
}

fn settled_float_drag_drops_its_preview(
    cx: &mut TestAppContext,
    reply: impl Fn(u64) -> zz_protocol::CommandResponse,
) {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let controller = cx.new(|cx| {
            crate::browser::controller::BrowserController::new(
                Err(ala_cef::BrowserError::AlreadyShutdown),
                cx,
            )
        });
        let agent_controller = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "test client".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        captured_mux.replace(Some(mux.clone()));
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
    let tiled = PaneId(0);
    let floating = PaneId(1);
    let mut snapshot = one_pane_snapshot(1);
    let window = &mut snapshot.sessions[0].windows[0];
    let mut float_pane = window.panes[&tiled].clone();
    float_pane.id = floating;
    window.panes.insert(floating, float_pane);
    let float = FloatingPaneSnapshot {
        pane: floating,
        xoff: 4,
        yoff: 2,
        sx: 20,
        sy: 6,
        visible: true,
        border_lines: PaneBorderLines::Single,
        border_status: PaneBorderStatus::Off,
    };
    window.floating = vec![float];
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
    });
    cx.run_until_parked();
    let drawn = |cx: &mut ala::VisualTestContext| {
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.debug_bounds("floating-pane-1")
            .expect("a visible float is drawn")
            .origin
    };
    let resting = drawn(cx);

    workspace.update(cx, |workspace, cx| {
        workspace.float_drag = Some(FloatDragState {
            window: WindowId(0),
            pane: floating,
            grip: FloatGrip::MOVE,
            lines: PaneBorderLines::Single,
            area: FloatWindow {
                border_status: PaneBorderStatus::Off,
                rows: 24,
            },
            start: FloatCells::from(&float),
            press: Point::default(),
            cell: ala::size(px(8.0), px(16.0)),
            delta: (3, 2),
            started: true,
            committed_snapshot_revision: Some(workspace.snapshot_revision),
            request: Some(7),
            pip: None,
            limit: None,
        });
        cx.notify();
    });
    mux.read_with(cx, |mux, _| mux.watch_command_for_test(7));
    cx.run_until_parked();
    assert_ne!(drawn(cx), resting, "a committed drag previews the move");
    let revision = workspace.read_with(cx, |workspace, _| workspace.snapshot_revision);

    let settle = |request_id, cx: &mut ala::VisualTestContext| {
        mux.update(cx, |mux, cx| {
            mux.handle_message_for_test(
                zz_protocol::ProtocolMessage::CommandResponse(reply(request_id)),
                cx,
            );
        });
        cx.run_until_parked();
    };
    settle(6, cx);
    assert!(
        workspace.read_with(cx, |workspace, _| workspace.float_drag.is_some()),
        "another request's reply leaves the drag alone"
    );

    settle(7, cx);
    workspace.read_with(cx, |workspace, _| {
        assert!(workspace.float_drag.is_none());
        assert_eq!(workspace.snapshot_revision, revision);
    });
    assert_eq!(drawn(cx), resting, "the settled drag left its preview up");

    workspace.update(cx, |workspace, cx| {
        workspace.begin_float_drag(
            WindowId(0),
            PaneBorderStatus::Off,
            float,
            FloatGrip::MOVE,
            ala::size(px(8.0), px(16.0)),
            Point::default(),
            cx,
        );
        assert!(
            workspace
                .float_drag
                .is_some_and(|drag| drag.committed_snapshot_revision.is_none()),
            "a settled drag no longer blocks the next one"
        );
    });
}

#[ala::test]
fn a_refused_float_drag_drops_its_preview_without_a_snapshot(cx: &mut TestAppContext) {
    settled_float_drag_drops_its_preview(cx, |request_id| zz_protocol::CommandResponse::Error {
        request_id,
        error: zz_protocol::ServerError::InvalidCommand("client is read-only".to_owned()),
        output: zz_protocol::RawText::default(),
    });
}

#[ala::test]
fn an_accepted_float_drag_that_changed_nothing_drops_its_preview(cx: &mut TestAppContext) {
    settled_float_drag_drops_its_preview(cx, |request_id| zz_protocol::CommandResponse::Success {
        request_id,
        output: zz_protocol::RawText::default(),
        exit_code: 0,
        stderr: String::new(),
        stdout_claim: zz_protocol::StdoutClaim::None,
    });
}

#[ala::test]
fn a_float_drag_measures_rows_from_the_daemon_window(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let controller = cx.new(|cx| {
            crate::browser::controller::BrowserController::new(
                Err(ala_cef::BrowserError::AlreadyShutdown),
                cx,
            )
        });
        let agent_controller = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "test client".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        captured_mux.replace(Some(mux.clone()));
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
    let tiled = PaneId(0);
    let floating = PaneId(1);
    let mut snapshot = one_pane_snapshot(1);
    let window = &mut snapshot.sessions[0].windows[0];
    let mut float_pane = window.panes.remove(&tiled).expect("the one pane to float");
    float_pane.id = floating;
    window.panes.insert(floating, float_pane);
    window.layout = LayoutNode::Empty;
    window.active_pane = floating;
    window.pane_order = vec![floating];
    window.pane_z_order = vec![floating];
    window.sx = 100;
    window.sy = 37;
    let float = FloatingPaneSnapshot {
        pane: floating,
        xoff: 4,
        yoff: 30,
        sx: 20,
        sy: 6,
        visible: true,
        border_lines: PaneBorderLines::Single,
        border_status: PaneBorderStatus::Off,
    };
    window.floating = vec![float];
    window.pane_border_status = PaneBorderStatus::Bottom;
    window.layout_dump =
        r#"{"V":2,"L":{"t":"p","w":22,"h":8,"x":3,"y":29,"a":true,"i":0,"z":0,"I":"%1"}}"#
            .to_owned();
    window.visible_layout_dump.clone_from(&window.layout_dump);
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
    });
    cx.run_until_parked();

    workspace.update(cx, |workspace, cx| {
        workspace.begin_float_drag(
            WindowId(0),
            PaneBorderStatus::Bottom,
            float,
            FloatGrip::MOVE,
            ala::size(px(8.0), px(16.0)),
            Point::default(),
            cx,
        );
        assert_eq!(
            workspace.float_drag.map(|drag| drag.area.rows),
            Some(37),
            "a window with no tiled pane judges the bottom status row on the daemon's height"
        );
    });
}

#[ala::test]
fn a_dragged_pip_follows_the_pointer_and_lands_in_the_third_it_was_dropped_in(
    cx: &mut TestAppContext,
) {
    let (workspace, mux, cx) = float_workspace_for_test(cx);
    let mut snapshot = tile_and_float_snapshot(1, false);
    let window = &mut snapshot.sessions[0].windows[0];
    window.sx = 80;
    window.sy = 24;
    let float = window.floating[0];
    cx.update(|_, cx| {
        let mut config = config::AppConfig::default();
        config.pane_gaps.value = true;
        config.pane_margin.value = 32.0;
        cx.set_global(config);
    });
    mux.update(cx, |mux, cx| {
        mux.connect_for_test();
        mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
    });
    cx.run_until_parked();
    let drawn = |cx: &mut ala::VisualTestContext| {
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.debug_bounds("floating-pane-1")
            .expect("a visible float is drawn")
            .origin
    };
    for _ in 0..120 {
        cx.executor().advance_clock(Duration::from_millis(16));
        drawn(cx);
        if !workspace.read_with(cx, |workspace, _| workspace.pane_motion.moving()) {
            break;
        }
    }
    let resting = drawn(cx);
    assert!(
        workspace.read_with(cx, |workspace, _| workspace.pane_canvas_bounds.get().origin)
            != Point::default(),
        "the canvas sits off the window origin, as it does beside a sidebar"
    );
    let header = cx
        .debug_bounds("pip-header")
        .expect("a PiP draws the pane header in its top border");
    let title = point(header.left() + px(30.0), header.center().y);
    let grabbed = title + point(px(4.0), px(0.0));
    cx.simulate_mouse_down(title, MouseButton::Left, Modifiers::default());
    for nudge in [title + point(px(3.0), px(0.0)), grabbed] {
        cx.simulate_mouse_move(nudge, MouseButton::Left, Modifiers::default());
    }
    for step in 1..=5 {
        cx.simulate_mouse_move(
            grabbed + point(px(300.0 * step as f32), px(180.0 * step as f32)),
            MouseButton::Left,
            Modifiers::default(),
        );
    }
    assert_eq!(drawn(cx), resting + point(px(1500.0), px(900.0)));
    cx.simulate_mouse_up(
        grabbed + point(px(1500.0), px(900.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.run_until_parked();
    workspace.read_with(cx, |workspace, _| {
        assert!(workspace.float_drag.is_none());
        let landing = workspace
            .pip_landing
            .expect("a thrown PiP lands on its place");
        assert_eq!(
            landing.target,
            zz_client::floating::FloatPlace::BottomRight.cells(
                FloatCells::from(&float),
                (80, 24),
                PaneBorderLines::Single
            )
        );
    });
}

#[ala::test]
fn a_pip_is_thrown_by_the_title_in_its_pane_header(cx: &mut TestAppContext) {
    let (workspace, mux, cx) = float_workspace_for_test(cx);
    let mut snapshot = tile_and_float_snapshot(1, false);
    let window = &mut snapshot.sessions[0].windows[0];
    window.sx = 80;
    window.sy = 24;
    mux.update(cx, |mux, cx| {
        mux.connect_for_test();
        mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    let header = cx
        .debug_bounds("pip-header")
        .expect("a PiP draws the pane header in its top border");
    let title = point(header.left() + px(30.0), header.center().y);
    cx.simulate_mouse_down(title, MouseButton::Left, Modifiers::default());
    for step in 1..=6 {
        cx.simulate_mouse_move(
            title + point(px(100.0 * step as f32), px(60.0 * step as f32)),
            MouseButton::Left,
            Modifiers::default(),
        );
    }
    cx.simulate_mouse_up(
        title + point(px(600.0), px(360.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.run_until_parked();
    workspace.read_with(cx, |workspace, _| {
        assert!(workspace.float_drag.is_none());
        assert!(
            workspace.pip_landing.is_some(),
            "letting go of the title throws the PiP"
        );
    });
    let sent = mux.read_with(cx, |mux, _| mux.executed_for_test());
    assert!(
        sent.iter()
            .any(|command| command.name == "move-pane" && command.args.contains(&"-P".into())),
        "{sent:?}"
    );
}

#[ala::test]
fn split_drag_defers_terminal_resize_until_override_release(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
    let (workspace, cx) = cx.add_window_view(move |window, cx| {
        let controller = cx.new(|cx| {
            crate::browser::controller::BrowserController::new(
                Err(ala_cef::BrowserError::AlreadyShutdown),
                cx,
            )
        });
        let agent_controller = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "test client".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        captured_mux.replace(Some(mux.clone()));
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), two_pane_snapshot(PaneId(2)), cx);
    });
    cx.run_until_parked();

    let terminal = workspace.read_with(cx, |workspace, _| workspace.terminals[&PaneId(0)].clone());
    let sent = mux.update(cx, |mux, _| mux.record_input_for_test());
    let update_geometry = |grid_size, cx: &mut ala::VisualTestContext| {
        terminal.update(cx, |terminal, cx| {
            terminal.update_geometry(
                grid_size,
                Bounds::new(point(px(0.0), px(0.0)), ala::size(px(960.0), px(384.0))),
                Bounds::new(point(px(0.0), px(0.0)), ala::size(px(960.0), px(384.0))),
                px(8.0),
                px(16.0),
                px(0.0),
                None,
                None,
                cx,
            );
        });
    };
    let before = GridSize {
        columns: 80,
        rows: 24,
        cell_width_px: 8,
        cell_height_px: 16,
    };
    let during = GridSize {
        columns: 120,
        ..before
    };
    update_geometry(before, cx);
    sent.borrow_mut().clear();

    workspace.update(cx, |workspace, _| {
        workspace.set_split_drag(Some(SplitDragState {
            drag: SplitDrag {
                window: WindowId(0),
                split: SplitId(0),
                start_ratio: 0.5,
                axis: Axis::Horizontal,
            },
            ratio: 0.6,
            committed_snapshot_revision: Some(workspace.snapshot_revision),
        }));
    });
    update_geometry(during, cx);
    assert!(sent.borrow().is_empty());

    workspace.update(cx, |workspace, cx| {
        workspace.snapshot_revision = workspace.snapshot_revision.wrapping_add(1);
        workspace.reconcile_split_drag(None, cx);
    });
    update_geometry(during, cx);
    {
        let sent = sent.borrow();
        assert_eq!(sent.len(), 1);
        assert!(matches!(
            &sent[0],
            InputMessage::ResizeTerminalV2 {
                pane,
                columns,
                rows,
                cell_width_px,
                cell_height_px,
                layout_generation: 0,
            } if *pane == PaneId(0)
                && *columns == 120
                && *rows == 24
                && *cell_width_px == 8
                && *cell_height_px == 16
        ));
    }
    update_geometry(during, cx);
    assert_eq!(sent.borrow().len(), 1);
    sent.borrow_mut().clear();

    workspace.update(cx, |workspace, _| {
        workspace.set_split_drag(Some(SplitDragState {
            drag: SplitDrag {
                window: WindowId(0),
                split: SplitId(0),
                start_ratio: 0.5,
                axis: Axis::Horizontal,
            },
            ratio: 0.4,
            committed_snapshot_revision: Some(workspace.snapshot_revision),
        }));
    });
    update_geometry(before, cx);
    workspace.update(cx, |workspace, cx| {
        workspace.snapshot_revision = workspace.snapshot_revision.wrapping_add(1);
        workspace.reconcile_split_drag(None, cx);
    });
    update_geometry(during, cx);
    assert!(sent.borrow().is_empty());
}

#[ala::test]
fn pane_drop_dispatches_before_deferred_teardown(cx: &mut TestAppContext) {
    let steps = Rc::new(RefCell::new(Vec::new()));
    let captured_steps = steps.clone();
    let (_, cx) = cx.add_window_view(move |_, _| PaneReleaseOrderView {
        source: PaneId(3),
        steps: captured_steps,
    });

    cx.simulate_mouse_move(
        point(px(10.0), px(10.0)),
        None::<MouseButton>,
        Modifiers::default(),
    );
    cx.simulate_mouse_down(
        point(px(10.0), px(10.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        point(px(20.0), px(10.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        point(px(150.0), px(10.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        point(px(150.0), px(10.0)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.run_until_parked();

    assert_eq!(
        *steps.borrow(),
        vec![
            PaneReleaseStep::Drop(pane_swap_command(PaneId(3), PaneId(7))),
            PaneReleaseStep::Teardown(PaneId(3)),
        ]
    );
}

#[ala::test]
fn pane_handle_drop_and_escape_leave_keyboard_input_alone(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let (workspace, cx) = cx.add_window_view(|window, cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(ala_cef::BrowserError::AlreadyShutdown), cx));
        let agent_controller = cx.new(|_| AgentController::new());
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(zz_daemon_client::DaemonError::Thread(
                    "test client".to_owned(),
                )),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
    let snapshot = two_pane_snapshot(PaneId(0));
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), snapshot.clone(), cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let input = mux.update(cx, |mux, _| mux.record_input_for_test());
    let bounds = workspace.read_with(cx, |workspace, _| workspace.pane_canvas_bounds.get());
    let target = bounds.origin + point(bounds.size.width * 0.75, bounds.size.height * 0.5);
    cx.simulate_mouse_move(target, None::<MouseButton>, Modifiers::default());
    let payload = PaneDrag {
        pane: PaneId(0),
        requires_prefix: false,
    };
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.on_pane_drag_start(payload, cx);
        let state = workspace.pane_drag.as_mut().unwrap();
        let active_window = &snapshot.sessions[0].windows[0];
        assert!(state.matches(active_window, false));
        state.requires_prefix = true;
        assert!(!state.matches(active_window, false));
        assert!(state.matches(active_window, true));
        state.requires_prefix = false;
        let mut zoomed = active_window.clone();
        zoomed.zoomed_pane = Some(payload.pane);
        assert!(!state.matches(&zoomed, false));
        let mut floated = active_window.clone();
        let float = PaneId(9);
        let mut float_pane = floated.panes[&PaneId(0)].clone();
        float_pane.id = float;
        floated.panes.insert(float, float_pane);
        floated.floating = vec![FloatingPaneSnapshot {
            pane: float,
            xoff: 4,
            yoff: 2,
            sx: 20,
            sy: 6,
            visible: true,
            border_lines: PaneBorderLines::Single,
            border_status: PaneBorderStatus::Off,
        }];
        floated.active_pane = float;
        assert!(state.matches(&floated, false));
        assert!(state.target.is_none());
        workspace.on_pane_drop(payload, window, cx);
        assert!(workspace.pane_drag.is_none());
        assert_eq!(
            workspace.pane_layout_override.as_ref().unwrap().layout,
            predicted_drop_layout(
                &active_window.layout,
                PaneId(0),
                PaneId(2),
                DropZone::Center
            )
            .unwrap()
        );

        workspace.on_pane_drag_start(payload, cx);
        workspace.intercept_keystroke(
            &ala::KeystrokeEvent {
                keystroke: Keystroke {
                    key: "escape".to_owned(),
                    key_char: None,
                    modifiers: Modifiers::default(),
                },
                action: None,
                context_stack: Vec::new(),
                is_held: false,
            },
            window,
            cx,
        );
        assert!(workspace.pane_drag.is_none());
        assert!(!cx.has_active_drag());
    });
    assert!(
        !input
            .borrow()
            .iter()
            .any(|message| matches!(message, InputMessage::Key { .. }))
    );
    assert!(!mux.read_with(cx, |mux, _| mux.prefix_armed()));
}

#[test]
fn taking_a_completed_pane_drag_always_clears_drag_state() {
    let mut drag = Some(three_pane_drag(PaneId(3)));
    drag.as_mut()
        .expect("source belongs to the frozen layout")
        .set_target(
            Some((PaneId(7), DropZone::Top)),
            DropPreviewFrame::default(),
        );

    assert_eq!(
        take_pane_drag(&mut drag),
        Some(PaneDrag {
            pane: PaneId(3),
            requires_prefix: true
        })
    );
    assert!(drag.is_none());
    assert_eq!(take_pane_drag(&mut drag), None);
}

#[test]
fn an_optimistic_layout_lives_exactly_until_the_next_snapshot() {
    let pending = PaneLayoutOverride {
        window: WindowId(5),
        layout: three_pane_layout(),
        generation: 12,
    };
    let window = WindowSnapshot {
        id: WindowId(5),
        index: 0,
        name: "work".to_owned(),
        automatic_rename: true,
        active_pane: PaneId(3),
        zoomed_pane: None,
        layout: three_pane_layout(),
        panes: BTreeMap::new(),
        layout_dump: String::new(),
        visible_layout_dump: String::new(),
        status_label: String::new(),
        activity: false,
        silence: false,
        pane_border_status: zz_protocol::PaneBorderStatus::Off,
        pane_border_lines: zz_protocol::PaneBorderLines::Single,
        pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
        pane_order: Vec::new(),
        pane_z_order: Vec::new(),
        floating: Vec::new(),
        modal: None,
        sx: 0,
        sy: 0,
        pane_base_index: 0,
    };

    assert!(pending.still_predicts(Some(&window), 12));
    assert!(!pending.still_predicts(Some(&window), 13));
    assert!(!pending.still_predicts(None, 12));
    assert!(!pending.still_predicts(
        Some(&WindowSnapshot {
            id: WindowId(6),
            ..window.clone()
        }),
        12
    ));
    assert!(!pending.still_predicts(
        Some(&WindowSnapshot {
            zoomed_pane: Some(PaneId(3)),
            ..window
        }),
        12
    ));
}

#[test]
fn split_drag_ratio_tracks_each_axis_across_the_full_layout() {
    let bounds = Bounds::new(point(px(100.0), px(50.0)), ala::size(px(800.0), px(400.0)));
    assert_eq!(
        split_ratio_basis(split_ratio_from_pointer(
            Axis::Horizontal,
            point(px(500.0), px(80.0)),
            bounds,
        )),
        5_000
    );
    assert_eq!(
        split_ratio_basis(split_ratio_from_pointer(
            Axis::Vertical,
            point(px(120.0), px(350.0)),
            bounds,
        )),
        7_500
    );
    assert_eq!(
        split_ratio_basis(split_ratio_from_pointer(
            Axis::Horizontal,
            point(px(10.0), px(80.0)),
            bounds,
        )),
        0
    );
    assert_eq!(
        split_ratio_basis(split_ratio_from_pointer(
            Axis::Vertical,
            point(px(120.0), px(900.0)),
            bounds,
        )),
        10_000
    );
    assert_eq!(split_ratio_basis(0.456_74), 4_567);
}
