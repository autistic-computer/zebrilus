use std::cell::RefCell;

use super::*;
use ala::TestAppContext;

pub(super) fn one_pane_snapshot(generation: u64) -> MuxSnapshot {
    let terminal = PaneId(0);
    let window = WindowSnapshot {
        id: WindowId(0),
        index: 0,
        name: "zz".to_owned(),
        automatic_rename: true,
        active_pane: terminal,
        zoomed_pane: None,
        layout: LayoutNode::Pane(terminal),
        panes: [(
            terminal,
            zz_protocol::PaneSnapshot {
                id: terminal,
                title: terminal.to_string(),
                kind: PaneKindSnapshot::Terminal,
                synchronized_input: false,
                bell: false,
                dead: false,
                dead_status: None,
                border_colour: None,
                active_border_colour: None,
                border_status_text: String::new(),
                mode: None,
                status: None,
                summary: zz_protocol::PaneSummary::default(),
            },
        )]
        .into_iter()
        .collect(),
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
    MuxSnapshot {
        generation,
        focused_window: Some(WindowId(0)),
        sessions: vec![zz_protocol::SessionSnapshot {
            id: SessionId(0),
            name: "zz".to_owned(),
            active_window: WindowId(0),
            windows: vec![window],
            viewers: Vec::new(),
        }],
    }
}

pub(super) fn choose_tree_state_for_test() -> zz_protocol::ChooseTreeState {
    let item = |label: &str, target| zz_protocol::ChooseTreeItem {
        label: label.to_owned(),
        detail: String::new(),
        target,
        depth: 0,
        flags: 0,
        pane_kind: None,
        key: String::new(),
        text: String::new(),
    };
    zz_protocol::ChooseTreeState {
        items: vec![
            item(
                "session",
                zz_protocol::ChooseTreeTarget::Session(SessionId(0)),
            ),
            item("window", zz_protocol::ChooseTreeTarget::Window(WindowId(0))),
        ],
        search: None,
        selected: 0,
        kind: zz_protocol::ChooseTreeKind::Panes,
        filter_no_matches: false,
        prompt: String::new(),
        help: false,
    }
}

pub(super) fn display_panes_state_for_test() -> zz_protocol::DisplayPanesState {
    zz_protocol::DisplayPanesState {
        window: WindowId(0),
        duration_ms: 1000,
        indicators: vec![zz_protocol::PaneIndicator {
            pane: PaneId(0),
            index: 0,
            select_key: b'0',
            flags: zz_protocol::PaneIndicator::ACTIVE,
            label: "0".to_owned(),
        }],
        colour: None,
        active_colour: None,
    }
}

#[cfg(unix)]
pub(super) struct InputTestServer {
    stream: std::os::unix::net::UnixStream,
    thread: Option<std::thread::JoinHandle<()>>,
    _directory: tempfile::TempDir,
}

#[cfg(unix)]
impl Drop for InputTestServer {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        self.thread.take().unwrap().join().unwrap();
    }
}

#[cfg(unix)]
pub(super) fn input_test_client() -> Option<(zz_daemon_client::InteractiveClient, InputTestServer)>
{
    use zz_protocol::{ProtocolMessage, read_protocol_message, write_protocol_message};

    let directory = tempfile::Builder::new()
        .prefix("zz-input-")
        .tempdir_in("/tmp")
        .unwrap();
    let socket = directory.path().join("s");
    let listener = match std::os::unix::net::UnixListener::bind(&socket) {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("SKIPPED: input routing fixture requires a Unix protocol socket: {error}");
            return None;
        }
        Err(error) => panic!("bind input routing fixture: {error}"),
    };
    let (sender, receiver) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        assert!(matches!(
            read_protocol_message(&mut stream).unwrap(),
            ProtocolMessage::ClientHello(_) | ProtocolMessage::Hello(_)
        ));
        sender.send(stream.try_clone().unwrap()).unwrap();
        let mut mux_options = zz_protocol::MuxOptions::default();
        mux_options.set(
            zz_protocol::MuxOptionKey::Prefix,
            "C-a",
            zz_protocol::MuxOptionSource::RuntimeCommand,
        );
        write_protocol_message(
            &mut stream,
            &ProtocolMessage::ServerHello(Box::new(zz_protocol::ServerHello {
                protocol_version: PROTOCOL_VERSION,
                server_id: 1,
                client_id: zz_protocol::ClientId(1),
                client_instance_id: zz_protocol::ClientInstanceId(1),
                capabilities: vec![
                    zz_protocol::PANE_FRAME_CAPABILITY.to_owned(),
                    zz_protocol::CONTROL_CAPABILITY.to_owned(),
                ],
                appearance: zz_terminal::TerminalAppearance::default(),
                appearance_provenance: zz_terminal::AppearanceProvenance::default(),
                mux_options,
                status: zz_protocol::StatusLine::default(),
                key_tables: zz_protocol::KeyTables::default().snapshot(),
                clock: 0,
                platform: zz_protocol::HostPlatform::default(),
                revision: zz_protocol::REVISION,
            })),
        )
        .unwrap();
        while read_protocol_message(&mut stream).is_ok() {}
    });
    let client = zz_daemon_client::InteractiveClient::connect(&socket).unwrap();
    let stream = receiver.recv().unwrap();
    Some((
        client,
        InputTestServer {
            stream,
            thread: Some(thread),
            _directory: directory,
        },
    ))
}

#[cfg(unix)]
pub(super) fn input_test_workspace(
    cx: &mut TestAppContext,
    client: zz_daemon_client::InteractiveClient,
) -> (Entity<AppView>, &mut ala::VisualTestContext) {
    cx.update(zz_ui::init);
    let workspace_slot = Rc::new(RefCell::new(None));
    let captured_workspace = Rc::clone(&workspace_slot);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let controller =
            cx.new(|cx| BrowserController::new(Err(ala_cef::BrowserError::AlreadyShutdown), cx));
        let agent_controller = cx.new(|_| AgentController::new());
        let mux =
            cx.new(|cx| MuxClient::new(Ok(client), zz_daemon_client::default_socket_path(), cx));
        let workspace = cx.new(|cx| {
            AppView::new(
                controller.clone(),
                agent_controller.clone(),
                mux,
                window,
                cx,
            )
        });
        captured_workspace.replace(Some(workspace.clone()));
        let shell = cx
            .new(|cx| crate::app_shell::AppShell::new(workspace, controller, agent_controller, cx));
        crate::build_root(shell, window, cx)
    });
    let workspace = workspace_slot.borrow().clone().unwrap();
    let mux = workspace.read_with(cx, |workspace, _| workspace.mux.clone());
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(0), one_pane_snapshot(1), cx);
    });
    cx.run_until_parked();
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.focus_active_pane(window, cx);
    });
    (workspace, cx)
}

pub(super) fn two_pane_snapshot(active: PaneId) -> MuxSnapshot {
    two_pane_snapshot_of(active, PaneKindSnapshot::Picker)
}

pub(super) fn two_pane_snapshot_of(active: PaneId, second: PaneKindSnapshot) -> MuxSnapshot {
    let terminal = PaneId(0);
    let picker = PaneId(2);
    let generation_bias = u64::from(second != PaneKindSnapshot::Picker) * 100;
    let pane_snapshot = |id: PaneId, kind: PaneKindSnapshot| zz_protocol::PaneSnapshot {
        id,
        title: id.to_string(),
        kind,
        synchronized_input: false,
        bell: false,
        dead: false,
        dead_status: None,
        border_colour: None,
        active_border_colour: None,
        border_status_text: String::new(),
        mode: None,
        status: None,
        summary: zz_protocol::PaneSummary::default(),
    };
    let window = WindowSnapshot {
        id: WindowId(0),
        index: 0,
        name: "zz".to_owned(),
        automatic_rename: true,
        active_pane: active,
        zoomed_pane: None,
        layout: LayoutNode::Split {
            id: SplitId(0),
            axis: Axis::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Pane(terminal)),
            second: Box::new(LayoutNode::Pane(picker)),
        },
        panes: [
            (
                terminal,
                pane_snapshot(terminal, PaneKindSnapshot::Terminal),
            ),
            (picker, pane_snapshot(picker, second)),
        ]
        .into_iter()
        .collect(),
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
    MuxSnapshot {
        generation: 10 + active.0 + generation_bias,
        focused_window: Some(WindowId(0)),
        sessions: vec![zz_protocol::SessionSnapshot {
            id: SessionId(0),
            name: "zz".to_owned(),
            active_window: WindowId(0),
            windows: vec![window],
            viewers: Vec::new(),
        }],
    }
}

pub(super) fn float_workspace_for_test(
    cx: &mut TestAppContext,
) -> (
    Entity<AppView>,
    Entity<MuxClient>,
    &mut ala::VisualTestContext,
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
    (workspace, mux, cx)
}

pub(super) fn tile_and_float_snapshot(generation: u64, zoomed: bool) -> MuxSnapshot {
    let tiled = PaneId(0);
    let floating = PaneId(1);
    let mut snapshot = one_pane_snapshot(generation);
    let window = &mut snapshot.sessions[0].windows[0];
    let mut float_pane = window.panes[&tiled].clone();
    float_pane.id = floating;
    window.panes.insert(floating, float_pane);
    window.floating = vec![FloatingPaneSnapshot {
        pane: floating,
        xoff: 4,
        yoff: 2,
        sx: 20,
        sy: 6,
        visible: !zoomed,
        border_lines: PaneBorderLines::Single,
        border_status: PaneBorderStatus::Off,
    }];
    window.active_pane = floating;
    window.zoomed_pane = zoomed.then_some(floating);
    snapshot
}
