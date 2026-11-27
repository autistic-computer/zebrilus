use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::sync::atomic::AtomicU64;

use ala::TestAppContext;
use zz_protocol::MuxOptions;
use zz_terminal::{AppearanceProvenance, CellWidth, PackedCell, ScrollbarState, SessionStatus};

use super::*;

#[cfg(unix)]
pub(super) static TEST_SOCKET_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn test_host(name: &str, endpoint: &str) -> crate::config::HostEntry {
    crate::config::HostEntry {
        name: name.to_owned(),
        endpoint: zz_daemon_client::Endpoint::parse(endpoint).expect("test endpoint"),
    }
}

pub(super) fn test_server_hello() -> ServerHello {
    ServerHello {
        protocol_version: PROTOCOL_VERSION,
        server_id: 1,
        client_id: zz_protocol::ClientId(1),
        client_instance_id: zz_protocol::ClientInstanceId(1),
        capabilities: vec![
            zz_protocol::PANE_FRAME_CAPABILITY.to_owned(),
            zz_protocol::CONTROL_CAPABILITY.to_owned(),
        ],
        appearance: TerminalAppearance::default(),
        appearance_provenance: AppearanceProvenance::default(),
        mux_options: MuxOptions::default(),
        status: StatusLine::default(),
        key_tables: Vec::new(),
        clock: 0,
        platform: zz_protocol::HostPlatform::default(),
        revision: zz_protocol::REVISION,
    }
}

pub(super) fn seed_snapshot(mux: &mut MuxClient, snapshot: MuxSnapshot) {
    mux.seed_core(EventPayload::Snapshot(snapshot));
}

pub(super) fn seed_attachment(mux: &mut MuxClient, session: SessionId, snapshot: MuxSnapshot) {
    mux.core.handle_message(ProtocolMessage::Attached {
        session,
        snapshot,
        read_only: false,
        client_flags: String::new(),
    });
    mux.discard_core_effects();
}

pub(super) fn seed_choose_tree(mux: &mut MuxClient) {
    mux.seed_core(EventPayload::ChooseTree {
        state: Some(ChooseTreeState {
            items: Vec::new(),
            search: None,
            selected: 0,
            kind: zz_protocol::ChooseTreeKind::Windows,
            filter_no_matches: false,
            prompt: String::new(),
            help: false,
        }),
    });
}

pub(super) fn install_fake_connection(mux: &mut MuxClient, host: HostId) -> Arc<FakeConnectedHost> {
    let fake = Arc::new(FakeConnectedHost::new(test_server_hello()));
    let connection = mux.connections.get_mut(&host).expect("host connection");
    connection.client = None;
    connection.fake_client = Some(Arc::clone(&fake));
    connection.reader_thread = None;
    connection.state = HostState::Connected;
    fake
}

pub(super) fn record_notifications(
    mux: &ala::Entity<MuxClient>,
    cx: &mut ala::App,
) -> (
    ala::Entity<()>,
    Rc<RefCell<Vec<(ClientMessageKind, String)>>>,
) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&events);
    let mux = mux.clone();
    let sink = cx.new(|cx| {
        cx.subscribe(&mux, move |(), _, event: &ClientNotification, _| {
            observed.borrow_mut().push((event.kind, event.text.clone()));
        })
        .detach();
    });
    (sink, events)
}

pub(super) fn wait_for_mux(
    cx: &mut TestAppContext,
    mux: &ala::Entity<MuxClient>,
    description: &str,
    mut predicate: impl FnMut(&MuxClient) -> bool,
) {
    const DEADLINE: Duration = Duration::from_secs(20);

    let deadline = Instant::now() + DEADLINE;
    loop {
        cx.run_until_parked();
        if cx.update(|cx| predicate(mux.read(cx))) {
            return;
        }
        if Instant::now() >= deadline {
            let details = cx.update(|cx| {
                let mux = mux.read(cx);
                let hosts: Vec<String> = mux
                    .connections
                    .iter()
                    .map(|(host, connection)| {
                        format!(
                            "{host:?}: {:?} reconnect_in_flight={:?} snapshot_sessions={:?}",
                            connection.state,
                            connection.reconnect_attempt_in_flight,
                            connection
                                .snapshot
                                .as_ref()
                                .map(|snapshot| snapshot.sessions.len()),
                        )
                    })
                    .collect();
                format!(
                    "error={:?}; attached={:?}; chooser={}; {}",
                    mux.error,
                    mux.core.attached_session(),
                    mux.core.choose_tree().is_some(),
                    hosts.join("; ")
                )
            });
            panic!("timed out waiting for {description}: {details}");
        }
        cx.executor().advance_clock(Duration::from_millis(100));
        thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn test_mux(cx: &mut ala::App) -> ala::Entity<MuxClient> {
    cx.new(|cx| {
        MuxClient::new(
            Err(DaemonError::Thread("test client".to_owned())),
            zz_daemon_client::default_socket_path(),
            cx,
        )
    })
}

pub(super) fn history_fixture_viewport(
    row_ids: &[u32],
    generation: u64,
    total: u32,
    offset: u32,
) -> TerminalViewport {
    let mut viewport = TerminalViewport::blank(
        1,
        u16::try_from(row_ids.len()).expect("small fixture"),
        SessionStatus::Running,
    );
    viewport.generation = generation;
    viewport.view_generation = generation;
    viewport.scrollbar = ScrollbarState {
        total,
        offset,
        len: u32::try_from(row_ids.len()).expect("small fixture"),
    };
    for (cell, id) in Arc::make_mut(&mut viewport.cells).iter_mut().zip(row_ids) {
        *cell = PackedCell::new(0xe000 + *id, 0, CellWidth::Narrow);
    }
    viewport
}
