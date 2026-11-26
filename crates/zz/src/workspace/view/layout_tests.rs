use std::cell::RefCell;

use super::test_support::*;
use super::*;
use ala::{TestAppContext, point};

#[cfg(unix)]
#[ala::test]
fn panes_keep_the_gap_above_them_inside_the_workspace_under_the_titlebar(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let Some((client, _server)) = input_test_client() else {
        return;
    };
    let (workspace, cx) = input_test_workspace(cx, client);
    let gap = px(8.);
    cx.update(|_, cx| {
        let mut config = config::AppConfig::default();
        config.pane_gaps.value = true;
        config.pane_margin.value = 8.;
        cx.set_global(config);
        crate::theme::refresh_current_theme(cx);
        cx.set_reduce_motion(true);
    });
    let sidebar = workspace.read_with(cx, |workspace, _| workspace.sidebar.clone());
    for titlebar in [false, true] {
        if titlebar {
            sidebar.update(cx, WorkspaceSidebar::toggle_mode);
        }
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let root = cx.debug_bounds("app-root").expect("the workspace is drawn");
        let canvas = workspace.read_with(cx, |workspace, _| workspace.pane_canvas_bounds.get());
        assert_eq!(canvas.top() - root.top(), gap, "titlebar {titlebar}");
        assert_eq!(canvas.left() - root.left(), gap, "titlebar {titlebar}");
        if titlebar {
            let bar = cx.update(|_, cx| cx.theme().title_bar_over_panes_height());
            assert_eq!(canvas.top(), bar, "the panes start where the titlebar ends");
        }
    }
}

#[ala::test]
fn attached_pane_measurement_floors_below_the_laid_out_box(cx: &mut TestAppContext) {
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

    let cell_width = px(8.0);
    let line_height = px(16.0);
    let laid_out = zz_ui::terminal::terminal_grid_size(
        ala::size(px(960.0), px(384.0)),
        cell_width,
        line_height,
        1.0,
    );
    assert_eq!((laid_out.columns, laid_out.rows), (120, 24));

    let drawn = zz_ui::terminal::terminal_grid_size(
        ala::size(px(959.5), px(383.5)),
        cell_width,
        line_height,
        1.0,
    );
    assert_eq!((drawn.columns, drawn.rows), (119, 23));

    let terminal = workspace.read_with(cx, |workspace, _| workspace.terminals[&PaneId(0)].clone());
    let sent = mux.update(cx, |mux, _| mux.record_input_for_test());
    terminal.update(cx, |terminal, cx| {
        terminal.update_geometry(
            drawn,
            Bounds::new(point(px(0.0), px(0.0)), ala::size(px(959.5), px(383.5))),
            Bounds::new(point(px(0.0), px(0.0)), ala::size(px(959.5), px(383.5))),
            cell_width,
            line_height,
            px(0.0),
            None,
            None,
            cx,
        );
    });

    let sent = sent.borrow();
    assert_eq!(sent.len(), 1);
    assert!(matches!(
        &sent[0],
        InputMessage::ResizeTerminalV2 {
            pane,
            columns,
            rows,
            layout_generation: 0,
            ..
        } if *pane == PaneId(0) && *columns == 119 && *rows == 23
    ));
}
