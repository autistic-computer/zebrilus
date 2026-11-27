use super::ctrl_test_support::{compact_command, compact_registered};
use super::*;

#[test]
fn compact_resize_validates_generation_before_skipping_its_exact_report() {
    let shared = Arc::new(Shared::new(59));
    let (client, _) = compact_registered(&shared, zz_protocol::Subscriptions::terminal());
    let pane = PaneId(1);
    let geometry = TerminalGeometry {
        columns: 97,
        rows: 31,
        cell_width_px: 8,
        cell_height_px: 16,
    };
    let mut inner = shared.inner.lock();
    inner
        .panes
        .terminal_geometries
        .entry(pane)
        .or_default()
        .insert(client, geometry);
    assert!(inner.client(client).is_none_or(|c| c.ctrl_layout.is_none()));
    let input = InputMessage::ResizeTerminalV2 {
        pane,
        columns: geometry.columns,
        rows: geometry.rows,
        cell_width_px: geometry.cell_width_px,
        cell_height_px: geometry.cell_height_px,
        layout_generation: 1,
    };
    let normalized = ctrl::normalize_resize(&mut inner, client, input);
    assert_eq!(inner.clients[&client].ctrl_layout.as_ref().unwrap().1, 1);
    assert!(normalized.is_none());
    assert_eq!(inner.panes.terminal_geometries[&pane][&client], geometry);
    assert!(matches!(
        ctrl::normalize_resize(
            &mut inner,
            client,
            InputMessage::ResizeTerminal {
                pane,
                columns: geometry.columns,
                rows: geometry.rows,
                cell_width_px: geometry.cell_width_px,
                cell_height_px: geometry.cell_height_px,
            },
        ),
        Some(InputMessage::ResizeTerminal { .. })
    ));
}

#[test]
fn compact_resize_forwards_each_changed_geometry_field_and_unknown_reports() {
    let shared = Arc::new(Shared::new(61));
    let (client, _) = compact_registered(&shared, zz_protocol::Subscriptions::terminal());
    let pane = PaneId(1);
    let geometry = TerminalGeometry {
        columns: 97,
        rows: 31,
        cell_width_px: 8,
        cell_height_px: 16,
    };
    let mut inner = shared.inner.lock();
    inner
        .panes
        .terminal_geometries
        .entry(pane)
        .or_default()
        .insert(client, geometry);
    for (columns, rows, cell_width_px, cell_height_px) in [
        (98, 31, 8, 16),
        (97, 32, 8, 16),
        (97, 31, 9, 16),
        (97, 31, 8, 17),
    ] {
        assert!(matches!(
            ctrl::normalize_resize(
                &mut inner,
                client,
                InputMessage::ResizeTerminalV2 {
                    pane,
                    columns,
                    rows,
                    cell_width_px,
                    cell_height_px,
                    layout_generation: 1,
                },
            ),
            Some(InputMessage::ResizeTerminal {
                columns: next_columns,
                rows: next_rows,
                cell_width_px: next_width,
                cell_height_px: next_height,
                ..
            }) if (next_columns, next_rows, next_width, next_height)
                == (columns, rows, cell_width_px, cell_height_px)
        ));
        assert_eq!(inner.panes.terminal_geometries[&pane][&client], geometry);
    }
    inner.panes.terminal_geometries.remove(&pane);
    assert!(matches!(
        ctrl::normalize_resize(
            &mut inner,
            client,
            InputMessage::ResizeTerminalV2 {
                pane,
                columns: geometry.columns,
                rows: geometry.rows,
                cell_width_px: geometry.cell_width_px,
                cell_height_px: geometry.cell_height_px,
                layout_generation: 1,
            },
        ),
        Some(InputMessage::ResizeTerminal { .. })
    ));
}

#[test]
fn legacy_resize_accepts_v2_reports_without_a_compact_generation() {
    let shared = Arc::new(Shared::new(43));
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(ClientKind::Interactive, None, None, mailbox);
    let mut inner = shared.inner.lock();
    inner
        .panes
        .terminal_geometries
        .entry(PaneId(1))
        .or_default()
        .insert(
            client,
            TerminalGeometry {
                columns: 97,
                rows: 31,
                cell_width_px: 8,
                cell_height_px: 16,
            },
        );
    assert!(matches!(
        ctrl::normalize_resize(
            &mut inner,
            client,
            InputMessage::ClientTerminalSizeV2 {
                columns: 97,
                rows: 31,
                layout_generation: 0,
            },
        ),
        Some(InputMessage::ClientTerminalSize {
            columns: 97,
            rows: 31
        })
    ));
    assert!(matches!(
        ctrl::normalize_resize(
            &mut inner,
            client,
            InputMessage::ResizeTerminalV2 {
                pane: PaneId(1),
                columns: 97,
                rows: 31,
                cell_width_px: 8,
                cell_height_px: 16,
                layout_generation: 0,
            },
        ),
        Some(InputMessage::ResizeTerminal {
            pane: PaneId(1),
            columns: 97,
            rows: 31,
            cell_width_px: 8,
            cell_height_px: 16,
        })
    ));
    assert!(inner.client(client).is_none_or(|c| c.ctrl_layout.is_none()));
}

#[test]
fn stale_layout_size_report_is_dropped_before_geometry_changes() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "ctrl-resize", "sleep 30"]),
        )
        .expect("session");
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(ClientKind::Interactive, None, None, mailbox);
    shared
        .attach_target(client, ClientKind::Interactive, &mut context, "ctrl-resize")
        .expect("attach");
    let pane = context.pane.expect("pane");
    {
        let mut inner = shared.inner.lock();
        inner
            .client_entry(client)
            .ctrl_subscriptions
            .replace(zz_protocol::Subscriptions::terminal());
        ctrl::normalize_resize(
            &mut inner,
            client,
            InputMessage::ClientTerminalSizeV2 {
                columns: 80,
                rows: 24,
                layout_generation: 1,
            },
        )
        .expect("initial generation");
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-d", "sleep 30"]),
        )
        .expect("split");
    compact_command(
        &shared,
        &mut context,
        "resize-pane",
        &["-Z", "-t", &pane.to_string()],
    );
    shared.compact_tree_messages(client, true);
    let generation = shared.inner.lock().clients[&client]
        .ctrl_layout
        .as_ref()
        .unwrap()
        .1;
    compact_command(
        &shared,
        &mut context,
        "resize-pane",
        &["-Z", "-t", &pane.to_string()],
    );
    let geometry = shared.inner.lock().engine.pane_geometry(pane);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeTerminalV2 {
                pane,
                columns: 1,
                rows: 1,
                cell_width_px: 8,
                cell_height_px: 16,
                layout_generation: generation,
            },
        )
        .expect("stale report");
    assert_eq!(shared.inner.lock().engine.pane_geometry(pane), geometry);
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "ctrl-resize"]),
        )
        .expect("cleanup");
}

fn resize_application_fixture() -> (Arc<Shared>, ClientId, PaneId, ExecutionContext, u64) {
    let shared = Arc::new(Shared::new(62));
    let (client, _) = compact_registered(&shared, zz_protocol::Subscriptions::terminal());
    let mut context = ExecutionContext::default();
    let session = {
        let mut inner = shared.inner.lock();
        for command in [
            CommandInvocation::new("new-session", ["-d", "-s", "resize-admission"]),
            CommandInvocation::new("split-window", ["-h"]),
        ] {
            inner
                .engine
                .execute(&mut context, &command)
                .expect("model layout");
        }
        let pane = context.pane.expect("active pane");
        inner.terminals_mut().insert(
            pane,
            Arc::new(TerminalSession::spawn_output_view(
                "resize admission".to_owned(),
                String::new(),
            )),
        );
        context.session.expect("session")
    };
    shared
        .attach(client, session)
        .expect("attach model terminal");
    let pane = context.pane.expect("pane");
    {
        let mut inner = shared.inner.lock();
        let (columns, rows) = inner.engine.pane_geometry(pane).expect("pane geometry");
        inner
            .panes
            .terminal_geometries
            .entry(pane)
            .or_default()
            .insert(
                client,
                TerminalGeometry {
                    columns,
                    rows,
                    cell_width_px: 8,
                    cell_height_px: 16,
                },
            );
        inner.client_entry(client).size.replace((80, 24));
    }
    context.no_hooks = true;
    shared.compact_tree_messages(client, true);
    let generation = shared.inner.lock().clients[&client]
        .ctrl_layout
        .as_ref()
        .unwrap()
        .1;
    (shared, client, pane, context, generation)
}

#[test]
fn terminal_resize_is_revalidated_after_initial_admission() {
    let (shared, client, pane, _, generation) = resize_application_fixture();
    let report = TerminalGeometry {
        columns: 1,
        rows: 1,
        cell_width_px: 9,
        cell_height_px: 17,
    };
    let (stored, laid_out, terminal, viewport) = {
        let mut inner = shared.inner.lock();
        assert!(
            ctrl::normalize_resize(
                &mut inner,
                client,
                InputMessage::ResizeTerminalV2 {
                    pane,
                    columns: report.columns,
                    rows: report.rows,
                    cell_width_px: report.cell_width_px,
                    cell_height_px: report.cell_height_px,
                    layout_generation: generation,
                },
            )
            .is_some()
        );
        inner
            .engine
            .state
            .toggle_zoom(pane)
            .expect("layout changed after admission");
        let terminal = Arc::clone(&inner.terminals[&pane]);
        (
            inner.panes.terminal_geometries[&pane][&client],
            inner.engine.pane_geometry(pane),
            Arc::clone(&terminal),
            terminal.fresh_viewport(),
        )
    };
    assert!(
        !shared
            .apply_terminal_size_report(client, pane, report, Some(generation))
            .expect("stale application")
    );
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.panes.terminal_geometries[&pane][&client], stored);
        assert_eq!(inner.engine.pane_geometry(pane), laid_out);
    }
    let current = terminal.fresh_viewport();
    assert_eq!(
        (current.columns, current.rows),
        (viewport.columns, viewport.rows)
    );
    let generation = shared.inner.lock().clients[&client]
        .ctrl_layout
        .as_ref()
        .unwrap()
        .1;
    let current = TerminalGeometry {
        columns: 82,
        rows: 26,
        ..report
    };
    assert!(
        shared
            .apply_terminal_size_report(client, pane, current, Some(generation))
            .expect("current application")
    );
    assert_eq!(
        shared.inner.lock().panes.terminal_geometries[&pane][&client],
        current
    );
    shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.ctrl_subscriptions.take());
    assert!(
        shared
            .apply_terminal_size_report(client, pane, report, Some(0))
            .expect("legacy application")
    );
    assert_eq!(
        shared.inner.lock().panes.terminal_geometries[&pane][&client],
        report
    );
}

#[test]
fn client_resize_is_revalidated_after_initial_admission() {
    let (shared, client, pane, context, generation) = resize_application_fixture();
    let (stored, laid_out) = {
        let mut inner = shared.inner.lock();
        assert!(
            ctrl::normalize_resize(
                &mut inner,
                client,
                InputMessage::ClientTerminalSizeV2 {
                    columns: 1,
                    rows: 1,
                    layout_generation: generation,
                },
            )
            .is_some()
        );
        inner
            .engine
            .state
            .toggle_zoom(pane)
            .expect("layout changed after admission");
        (
            inner.clients[&client].size.unwrap(),
            inner.engine.pane_geometry(pane),
        )
    };
    assert!(!shared.apply_client_size_report(
        client,
        ClientKind::Interactive,
        &context,
        1,
        1,
        Some(generation),
    ));
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.clients[&client].size.unwrap(), stored);
        assert_eq!(inner.engine.pane_geometry(pane), laid_out);
    }
    let generation = shared.inner.lock().clients[&client]
        .ctrl_layout
        .as_ref()
        .unwrap()
        .1;
    assert!(shared.apply_client_size_report(
        client,
        ClientKind::Interactive,
        &context,
        81,
        25,
        Some(generation),
    ));
    assert_eq!(shared.inner.lock().clients[&client].size.unwrap(), (81, 25));
    shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.ctrl_subscriptions.take());
    assert!(shared.apply_client_size_report(
        client,
        ClientKind::Interactive,
        &context,
        1,
        1,
        Some(0)
    ));
    assert_eq!(shared.inner.lock().clients[&client].size.unwrap(), (1, 1));
}

#[test]
fn a_window_holding_no_terminal_takes_its_size_from_a_gui_tile_report() {
    let shared = Arc::new(Shared::new(62));
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let (session, window, pane) = {
        let mut inner = shared.inner.lock();
        let (session, window, pane) = inner
            .engine
            .state
            .create_session("chat-only")
            .expect("model session");
        inner.engine.state.pane_mut(pane).expect("pane").kind = PaneKind::Browser(
            zz_protocol::BrowserDescriptor::single("about:blank".to_owned(), "default".to_owned()),
        );
        (session, window, pane)
    };
    shared.attach(client, session).expect("GUI attach");
    let geometry = TerminalGeometry {
        columns: 150,
        rows: 40,
        cell_width_px: 8,
        cell_height_px: 16,
    };
    assert!(
        shared
            .apply_terminal_size_report(client, pane, geometry, None)
            .expect("a tile with no PTY still reports its size")
    );
    let inner = shared.inner.lock();
    assert_eq!(inner.engine.pane_geometry(pane), Some((150, 40)));
    assert_eq!(
        (
            inner
                .engine
                .window_extent(window, zz_protocol::Axis::Horizontal),
            inner
                .engine
                .window_extent(window, zz_protocol::Axis::Vertical)
        ),
        (Some(150), Some(40))
    );
}
