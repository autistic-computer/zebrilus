use std::cell::RefCell;

use super::test_support::*;
use super::*;
use crate::terminal::view::GridSize;
use ala::{Modifiers, TestAppContext, point};

#[ala::test]
fn a_capture_modal_float_draws_over_a_scrim_and_takes_every_key(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let mux_slot = Rc::new(RefCell::new(None));
    let input_slot = Rc::new(RefCell::new(None));
    let captured_mux = Rc::clone(&mux_slot);
    let captured_input = Rc::clone(&input_slot);
    let (_workspace, cx) = cx.add_window_view(move |window, cx| {
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
        let input = mux.update(cx, |mux, _| mux.record_input_for_test());
        captured_mux.replace(Some(mux.clone()));
        captured_input.replace(Some(input));
        AppView::new(controller, agent_controller, mux, window, cx)
    });
    let mux: Entity<MuxClient> = mux_slot.borrow().clone().expect("captured mux");
    let input = input_slot.borrow().clone().expect("captured input");
    let tiled = PaneId(0);
    let modal = PaneId(1);
    let mut snapshot = one_pane_snapshot(1);
    let window = &mut snapshot.sessions[0].windows[0];
    let mut float_pane = window.panes[&tiled].clone();
    float_pane.id = modal;
    float_pane.title = modal.to_string();
    float_pane.border_status_text = "#[fg=red]modal".to_owned();
    window.panes.insert(modal, float_pane);
    window.floating = vec![FloatingPaneSnapshot {
        pane: modal,
        xoff: 4,
        yoff: 2,
        sx: 20,
        sy: 6,
        visible: true,
        border_lines: PaneBorderLines::Single,
        border_status: zz_protocol::PaneBorderStatus::Top,
    }];
    window.modal = Some(ModalPaneSnapshot {
        pane: modal,
        capture_keys: true,
        close_on_click: true,
        close_on_cancel: true,
    });
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
    });
    cx.run_until_parked();
    for _ in 0..2 {
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    let float = cx
        .debug_bounds("floating-pane-1")
        .expect("a visible float is drawn");
    let scrim = cx
        .debug_bounds("floating-modal-scrim")
        .expect("a modal draws its scrim");
    assert!(scrim.contains(&float.origin));
    assert!(cx.debug_bounds("floating-grip-1-move").is_some());
    assert!(cx.debug_bounds("floating-pane-0").is_none());

    input.borrow_mut().clear();
    cx.simulate_keystrokes("ctrl-a");
    let sent = input.borrow().clone();
    assert!(
        sent.iter().any(|message| matches!(
            message,
            InputMessage::Key { pane, input, .. }
                if *pane == modal
                    && input.key == zz_terminal::KeyCode::Character('a')
                    && input.modifiers.control()
        )),
        "the capture modal did not take the key: {sent:?}"
    );
    assert!(
        !sent
            .iter()
            .any(|message| matches!(message, InputMessage::Key { pane, .. } if *pane == tiled)),
        "the tiled pane under the modal got a key: {sent:?}"
    );
}

#[ala::test]
fn focusing_a_tile_slides_the_pip_past_the_edge_and_a_click_brings_it_back(
    cx: &mut TestAppContext,
) {
    let (workspace, mux, cx) = float_workspace_for_test(cx);
    let mut snapshot = tile_and_float_snapshot(1, false);
    let window = &mut snapshot.sessions[0].windows[0];
    window.sx = 80;
    window.sy = 24;
    window.floating[0].xoff = 58;
    mux.update(cx, |mux, cx| {
        mux.connect_for_test();
        mux.attach_snapshot_for_test(SessionId(0), snapshot.clone(), cx);
    });
    cx.run_until_parked();
    let settle = |cx: &mut ala::VisualTestContext| {
        for _ in 0..120 {
            cx.executor().advance_clock(Duration::from_millis(16));
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            if !workspace.read_with(cx, |workspace, _| workspace.pane_motion.moving()) {
                break;
            }
        }
        cx.debug_bounds("floating-pane-1")
            .expect("a visible float is drawn")
    };
    let resting = settle(cx);
    let canvas = workspace.read_with(cx, |workspace, _| workspace.pane_canvas_bounds.get());
    assert!(resting.right() <= canvas.right());
    snapshot.generation = 2;
    snapshot.sessions[0].windows[0].active_pane = PaneId(0);
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), snapshot.clone(), cx);
    });
    cx.run_until_parked();
    let stashed = settle(cx);
    assert_eq!(stashed.left(), canvas.right() - px(60.0));
    cx.simulate_click(
        point(canvas.right() - px(30.0), stashed.center().y),
        Modifiers::default(),
    );
    cx.run_until_parked();
    assert_eq!(
        settle(cx),
        resting,
        "a click on the peek brings the PiP back"
    );
    let sent = mux.read_with(cx, |mux, _| mux.executed_for_test());
    assert!(
        sent.iter()
            .any(|command| command.name == "select-pane" && command.args.contains(&"%1".into())),
        "{sent:?}"
    );
}

#[ala::test]
fn float_makes_a_pip_as_tall_as_the_pane_canvas(cx: &mut TestAppContext) {
    let (workspace, mux, cx) = float_workspace_for_test(cx);
    cx.simulate_resize(ala::size(px(1065.0), px(686.0)));
    let settle = |cx: &mut ala::VisualTestContext| {
        for _ in 0..60 {
            cx.executor().advance_clock(Duration::from_millis(16));
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        }
    };
    mux.update(cx, |mux, cx| {
        mux.connect_for_test();
        mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
    });
    cx.run_until_parked();
    settle(cx);
    let canvas = workspace.read_with(cx, |workspace, _| workspace.pane_canvas_bounds.get());
    let cell = cx.update(|window, cx| workspace.read(cx).float_cell_size(window, cx));
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a test window is a few hundred cells"
    )]
    let window_cells = (
        (canvas.size.width / cell.width).floor() as u16,
        (canvas.size.height / cell.height).floor() as u16,
    );
    let mut snapshot = two_pane_snapshot(PaneId(0));
    (
        snapshot.sessions[0].windows[0].sx,
        snapshot.sessions[0].windows[0].sy,
    ) = window_cells;
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
    });
    cx.run_until_parked();
    settle(cx);
    let float = cx
        .debug_bounds("pane-action-Float")
        .expect("the tiled header has Float");
    cx.simulate_click(float.center(), Modifiers::default());
    cx.run_until_parked();
    let sent = mux.read_with(cx, |mux, _| mux.executed_for_test());
    let rows: u16 = sent
        .iter()
        .find(|command| command.name == "break-pane")
        .and_then(|command| {
            let at = command.args.iter().position(|arg| arg == "-y")?;
            command.args.get(at + 1)?.parse().ok()
        })
        .unwrap_or_else(|| panic!("Float sent no break-pane -y: {sent:?}"));

    let mut floated = tile_and_float_snapshot(3, false);
    let window = &mut floated.sessions[0].windows[0];
    (window.sx, window.sy) = window_cells;
    let cells = zz_client::floating::FloatPlace::BottomRight.cells(
        FloatCells {
            xoff: 0,
            yoff: 0,
            sx: window_cells.0 / 3,
            sy: rows - 2,
        },
        window_cells,
        PaneBorderLines::Single,
    );
    let float = &mut window.floating[0];
    (float.xoff, float.yoff, float.sx, float.sy) = (cells.xoff, cells.yoff, cells.sx, cells.sy);
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), floated, cx);
    });
    cx.run_until_parked();
    settle(cx);
    let pip = cx
        .debug_bounds("floating-pane-1")
        .expect("the PiP is drawn");
    let gap = pip.top() - canvas.top();
    assert!(
        gap < cell.height + px(12.0),
        "the PiP {pip:?} stops {gap:?} under the canvas {canvas:?} top"
    );
    assert!(pip.bottom() <= canvas.bottom(), "{pip:?} {canvas:?}");
}

#[ala::test]
fn a_pip_resizes_from_its_top_edge_and_top_corners(cx: &mut TestAppContext) {
    let (_workspace, mux, cx) = float_workspace_for_test(cx);
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), tile_and_float_snapshot(1, false), cx);
    });
    cx.run_until_parked();
    for _ in 0..60 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }
    let card = cx
        .debug_bounds("floating-pane-1")
        .expect("the PiP is drawn");
    for (grip, bounds) in [
        ("top", cx.debug_bounds("floating-grip-1-top")),
        ("top-left", cx.debug_bounds("floating-grip-1-top-left")),
        ("top-right", cx.debug_bounds("floating-grip-1-top-right")),
    ] {
        let bounds = bounds.unwrap_or_else(|| panic!("the PiP has no {grip} grip"));
        assert!(
            bounds.top() < card.top() && card.top() < bounds.bottom(),
            "the {grip} grip {bounds:?} does not straddle the PiP's top edge {card:?}"
        );
    }
    assert!(cx.debug_bounds("floating-grip-1-move").is_none());
}

#[ala::test]
fn a_zoomed_float_reports_its_grid_like_a_tile(cx: &mut TestAppContext) {
    let (workspace, mux, cx) = float_workspace_for_test(cx);
    let floating = PaneId(1);
    let report = |cx: &mut ala::VisualTestContext| {
        let terminal =
            workspace.read_with(cx, |workspace, _| workspace.terminals[&floating].clone());
        let sent = mux.update(cx, |mux, _| mux.record_input_for_test());
        terminal.update(cx, |terminal, cx| {
            terminal.update_geometry(
                GridSize {
                    columns: 120,
                    rows: 24,
                    cell_width_px: 8,
                    cell_height_px: 16,
                },
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
        sent.borrow()
            .iter()
            .filter_map(|message| match message {
                InputMessage::ResizeTerminalV2 {
                    pane,
                    columns,
                    rows,
                    ..
                } => Some((*pane, *columns, *rows)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), tile_and_float_snapshot(1, false), cx);
    });
    cx.run_until_parked();
    assert_eq!(report(cx), Vec::new(), "a float's PTY follows its cells");

    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), tile_and_float_snapshot(2, true), cx);
    });
    cx.run_until_parked();
    for _ in 0..120 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        if !workspace.read_with(cx, |workspace, _| workspace.pane_motion.moving()) {
            break;
        }
    }
    assert_eq!(
        report(cx),
        vec![(floating, 120, 24)],
        "a zoomed float fills the window, so its size drives the window extent"
    );
}

#[ala::test]
fn display_panes_labels_a_float_in_its_own_frame(cx: &mut TestAppContext) {
    let (_workspace, mux, cx) = float_workspace_for_test(cx);
    let mut snapshot = tile_and_float_snapshot(1, false);
    snapshot.sessions[0].windows[0].active_pane = PaneId(0);
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), snapshot, cx);
    });
    cx.run_until_parked();
    let indicator = |pane: u64, index: u32| zz_protocol::PaneIndicator {
        pane: PaneId(pane),
        index,
        select_key: b'0' + u8::try_from(index).unwrap(),
        flags: if pane == 0 {
            zz_protocol::PaneIndicator::ACTIVE
        } else {
            0
        },
        label: index.to_string(),
    };
    mux.update(cx, |mux, cx| {
        mux.handle_message_for_test(
            zz_protocol::ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: zz_protocol::EventPayload::DisplayPanes {
                    state: Some(zz_protocol::DisplayPanesState {
                        indicators: vec![indicator(0, 0), indicator(1, 1)],
                        ..display_panes_state_for_test()
                    }),
                },
            }),
            cx,
        );
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    assert!(cx.debug_bounds("pane-indicator-0").is_some());
    let frame = cx
        .debug_bounds("floating-pane-1")
        .expect("a visible float is drawn");
    let label = cx
        .debug_bounds("pane-indicator-1")
        .expect("the float carries its display-panes label");
    assert_eq!(
        label.center(),
        frame.center(),
        "the label {label:?} sits in the middle of the float {frame:?}"
    );
}
