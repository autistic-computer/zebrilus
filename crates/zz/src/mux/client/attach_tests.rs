use std::time::{Duration, Instant};

#[cfg(unix)]
use std::{path::PathBuf, sync::atomic::Ordering};

use ala::TestAppContext;
use zz_protocol::{SessionSnapshot, WindowId};
use zz_terminal::SessionStatus;

use super::test_support::*;
use super::*;

#[cfg(unix)]
struct RunningTestDaemon {
    socket: PathBuf,
    command: Option<zz_daemon_client::CommandClient>,
    thread: Option<thread::JoinHandle<Result<(), DaemonError>>>,
}

#[cfg(unix)]
impl RunningTestDaemon {
    fn start() -> Option<Self> {
        let socket = PathBuf::from(format!(
            "/tmp/zz-it-{}-{}.sock",
            std::process::id(),
            TEST_SOCKET_ID.fetch_add(1, Ordering::Relaxed),
        ));
        let _ = std::fs::remove_file(&socket);
        let daemon = zz_daemon::Daemon::new(&socket).without_user_config();
        let thread = thread::spawn(move || daemon.run_foreground());
        let deadline = Instant::now() + Duration::from_secs(30);
        let command = loop {
            match zz_daemon_client::CommandClient::connect(&socket) {
                Ok(client) => break client,
                Err(_) if thread.is_finished() => {
                    let result = thread.join().expect("join failed test daemon");
                    if matches!(
                        &result,
                        Err(DaemonError::Io(error))
                            if error.kind() == io::ErrorKind::PermissionDenied
                    ) {
                        return None;
                    }
                    panic!("test daemon exited during startup: {result:?}");
                }
                Err(error) if Instant::now() >= deadline => {
                    panic!("daemon did not start: {error}")
                }
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
        };
        Some(Self {
            socket,
            command: Some(command),
            thread: Some(thread),
        })
    }

    fn connect_interactive(&self) -> InteractiveClient {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match InteractiveClient::connect(&self.socket) {
                Ok(client) => return client,
                Err(error) if Instant::now() >= deadline => {
                    panic!("daemon did not accept an interactive client: {error}")
                }
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
        }
    }

    fn stop(&mut self) {
        if self.thread.is_none() {
            return;
        }
        self.command
            .as_mut()
            .expect("running daemon command client")
            .execute(CommandInvocation::new("kill-server", [] as [&str; 0]))
            .expect("stop test daemon");
        self.command = None;
        self.thread
            .take()
            .expect("running daemon thread")
            .join()
            .expect("join test daemon")
            .expect("test daemon exits cleanly");
        let _ = std::fs::remove_file(&self.socket);
    }
}

#[cfg(unix)]
impl Drop for RunningTestDaemon {
    fn drop(&mut self) {
        if self.thread.is_some() {
            if let Some(command) = &mut self.command {
                let _ = command.execute(CommandInvocation::new("kill-server", [] as [&str; 0]));
            }
            self.command = None;
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

#[ala::test]
fn sidebar_target_attachment_sends_one_request_on_local_and_remote_hosts(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-sidebar-target.sock")],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("sidebar target fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        let (local_client, remote_client) = mux.update(cx, |mux, _| {
            let local = install_fake_connection(mux, HostId::LOCAL);
            let remote = install_fake_connection(mux, remote);
            seed_attachment(mux, SessionId(1), MuxSnapshot::default());
            (local, remote)
        });

        for (host, client) in [(HostId::LOCAL, local_client), (remote, remote_client)] {
            crate::mux::nav::activate_nav(
                &mux,
                crate::mux::nav::NavActivation::AttachAt {
                    host,
                    target: "$2:@13.%21".to_owned(),
                },
                cx,
            );

            assert_eq!(mux.read(cx).attached_host(), host);
            assert_eq!(client.attached_session.get(), None);
            assert!(!client.attached_default.get());
            assert_eq!(
                *client.commands.borrow(),
                [CommandInvocation::new(
                    "attach-session",
                    ["-t", "$2:@13.%21"]
                )],
            );
        }
    });
}

#[ala::test]
fn attaching_elsewhere_cancels_the_pending_reconnect_generation(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-reconnect-cancel.sock")],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        mux.update(cx, |mux, cx| {
            let local = install_fake_connection(mux, HostId::LOCAL);
            install_fake_connection(mux, remote);
            mux.attached_host = remote;
            seed_attachment(mux, SessionId(9), MuxSnapshot::default());
            mux.handle_host_disconnected(remote, cx);
            let armed_generation = mux.connections.get(&remote).unwrap().reconnect_generation;

            assert!(mux.attach_to_host_target(HostId::LOCAL, None, None, cx));

            let connection = mux.connections.get(&remote).unwrap();
            assert_eq!(
                connection.state,
                HostState::Unreachable {
                    reason: "connection lost".to_owned(),
                }
            );
            assert!(!reconnect_timer_is_current(connection, armed_generation, 1));
            assert!(local.attached_default.get());
            assert_eq!(mux.attached_host, HostId::LOCAL);
        });
    });
}

#[cfg(unix)]
#[ala::test]
fn fleet_attach_two_real_daemons_keeps_remote_frames_on_server_stopping(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let Some(mut local_daemon) = RunningTestDaemon::start() else {
        return;
    };
    let Some(mut remote_daemon) = RunningTestDaemon::start() else {
        return;
    };
    let local_client = local_daemon.connect_interactive();
    let local_socket = local_daemon.socket.clone();
    let remote_endpoint = format!("unix://{}", remote_daemon.socket.display());
    let (mux, remote) = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(vec![test_host("remote", &remote_endpoint)], cx);
        let mux = cx.new(|cx| MuxClient::new(Ok(local_client), local_socket.clone(), cx));
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        mux.update(cx, |mux, cx| mux.ensure_connected(remote, cx));
        (mux, remote)
    });

    wait_for_mux(
        cx,
        &mux,
        "remote connection and connect-time snapshot",
        |mux| {
            let connection = mux.connections.get(&remote).unwrap();
            connection.state == HostState::Connected && connection.snapshot.is_some()
        },
    );

    cx.update(|cx| {
        mux.update(cx, |mux, cx| {
            assert!(mux.attach_to_host_default(remote, cx));
        });
    });
    wait_for_mux(cx, &mux, "remote Attached response", |mux| {
        mux.attached_host == remote && mux.core.attached_session().is_some()
    });
    let remote_session = cx.update(|cx| {
        let mux = mux.read(cx);
        mux.core
            .attached_session()
            .expect("lazily created remote session")
    });
    cx.update(|cx| {
        let mux = mux.read(cx);
        let local = &mux.connections[&HostId::LOCAL];
        assert_eq!(local.state, HostState::Connected);
        assert_eq!(local.reconnect_attempt_in_flight, None);
    });
    cx.update(|cx| {
        mux.update(cx, |mux, _| seed_choose_tree(mux));
    });

    remote_daemon.stop();
    wait_for_mux(cx, &mux, "remote ServerStopping event", |mux| {
        mux.core.choose_tree().is_none()
            && mux.error.is_none()
            && mux.attached_host == remote
            && mux.core.attached_session() == Some(remote_session)
    });
    local_daemon.stop();
}

#[ala::test]
fn releasing_the_attached_host_lets_its_removal_end_the_connection(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(vec![test_host("remote", "ssh://remote")], cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        let (local, route) = mux.update(cx, |mux, cx| {
            let local = install_fake_connection(mux, HostId::LOCAL);
            install_fake_connection(mux, remote);
            mux.attached_host = remote;
            seed_attachment(mux, SessionId(9), MuxSnapshot::default());
            let route = Arc::clone(&mux.connections.get(&remote).unwrap().route);
            mux.release_host(HostId::LOCAL, cx);
            assert_eq!(mux.attached_host, remote);
            mux.release_host(remote, cx);
            (local, route)
        });
        assert!(local.attached_default.get());

        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        mux.update(cx, |mux, cx| {
            assert_eq!(mux.host_states(cx).count(), 1);
            assert!(mux.registry.get_by_name("remote").is_none());
            assert_eq!(mux.attached_host, HostId::LOCAL);
            assert!(
                mux.registry
                    .iter()
                    .all(|(host, _)| !mux.registry.is_retained(host)),
                "nothing is kept alive behind the tree's back"
            );
            assert_eq!(*route.read(), None);
        });
    });
}

#[ala::test]
fn removing_an_attached_reconnecting_host_cancels_its_route_and_gives_up_to_local(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-removed-reconnect.sock")],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        let (local, route) = mux.update(cx, |mux, cx| {
            let local = install_fake_connection(mux, HostId::LOCAL);
            install_fake_connection(mux, remote);
            mux.attached_host = remote;
            seed_attachment(mux, SessionId(9), MuxSnapshot::default());
            let route = Arc::clone(&mux.connections.get(&remote).unwrap().route);
            mux.handle_host_disconnected(remote, cx);
            (local, route)
        });

        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        mux.update(cx, |mux, cx| {
            assert_eq!(mux.host_states(cx).count(), 1);
            assert!(mux.registry.get_by_name("remote").is_none());
            assert_eq!(mux.attached_host, HostId::LOCAL);
            assert!(local.attached_default.get());
            assert!(mux.core.snapshot().sessions.is_empty());
            assert!(mux.core.attached_session().is_none());
            assert_eq!(mux.error.as_deref(), Some("fleet host remote disconnected"));
            assert_eq!(*route.read(), None);
        });
    });
}

#[ala::test]
fn attach_to_non_connected_host_is_a_noop(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(vec![test_host("remote", "ssh://remote")], cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("initial error".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux
            .read(cx)
            .registry
            .get_by_name("remote")
            .expect("remote host")
            .0;
        mux.update(cx, |mux, cx| {
            mux.connections.get_mut(&remote).unwrap().state = HostState::Unreachable {
                reason: "offline".to_owned(),
            };
            seed_attachment(
                mux,
                SessionId(7),
                MuxSnapshot {
                    generation: 9,
                    focused_window: None,
                    sessions: Vec::new(),
                },
            );
            let error = mux.error.clone();

            mux.attach_to_host(remote, SessionId(11), cx);

            assert_eq!(mux.attached_host, HostId::LOCAL);
            assert_eq!(mux.core.attached_session(), Some(SessionId(7)));
            assert_eq!(mux.core.snapshot().generation, 9);
            assert_eq!(mux.error, error);
        });
    });
}

#[ala::test]
fn compact_machine_row_attach_asks_the_daemon_for_its_default_session(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(vec![test_host("remote", "ssh://remote")], cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("initial error".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        let fake_remote = Arc::new(FakeConnectedHost::new(test_server_hello()));

        mux.update(cx, |mux, cx| {
            let connection = mux.connections.get_mut(&remote).unwrap();
            connection.state = HostState::Connected;
            connection.fake_client = Some(Arc::clone(&fake_remote));

            assert!(mux.attach_to_host_default(remote, cx));
            assert_eq!(mux.attached_host, remote);
            assert!(fake_remote.attached_default.get());
            assert_eq!(fake_remote.attached_session.get(), None);
        });
    });
}

#[ala::test]
fn a_chat_on_a_machine_with_no_sessions_makes_its_own_and_never_asks_for_a_default(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(vec![test_host("remote", "ssh://remote")], cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("initial error".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        let agent = zz_client::new_chat::AGENT_CHOICES[0];
        let landing = zz_client::new_chat::landing_chat(
            &MuxSnapshot::default(),
            "~/dev/app",
            agent,
            &zz_client::agent_launch::AgentLaunches::default(),
        );

        mux.update(cx, |mux, cx| {
            let fake_remote = install_fake_connection(mux, remote);

            assert!(mux.land(remote, landing, cx));
            assert_eq!(mux.attached_host, remote);
            assert!(!fake_remote.attached_default.get());
            assert_eq!(fake_remote.attached_session.get(), None);
            assert_eq!(
                *fake_remote.commands.borrow(),
                [
                    CommandInvocation::new("new-session", ["-d", "-s", "app", "-c", "~/dev/app"]),
                    CommandInvocation::new(
                        "select-pane-kind",
                        ["-t", "=app:", "-c", "~/dev/app", "agent", "claude"]
                    ),
                    CommandInvocation::new("attach-session", ["-t", "=app"]),
                ]
            );
        });
    });
}

#[ala::test]
fn attaching_remote_preserves_local_snapshot_and_connection(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host(
                "remote",
                "unix:///tmp/zz-attach-redial-remote.sock",
            )],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                std::path::PathBuf::from("/tmp/zz-attach-redial-local-does-not-exist.sock"),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;

        mux.update(cx, |mux, cx| {
            install_fake_connection(mux, HostId::LOCAL);
            let remote_client = install_fake_connection(mux, remote);
            seed_attachment(
                mux,
                SessionId(7),
                MuxSnapshot {
                    generation: 19,
                    focused_window: None,
                    sessions: Vec::new(),
                },
            );
            let local_snapshot = Arc::clone(mux.core.snapshot());

            assert!(mux.attach_to_host(remote, SessionId(11), cx));

            assert_eq!(mux.attached_host, remote);
            assert_eq!(remote_client.attached_session.get(), Some(SessionId(11)));
            let local = &mux.connections[&HostId::LOCAL];
            assert_eq!(local.state, HostState::Connected);
            assert_eq!(local.reconnect_attempt_in_flight, None);
            assert!(
                local
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| Arc::ptr_eq(snapshot, &local_snapshot))
            );
        });
    });
}

#[ala::test]
fn attach_to_connected_host_clears_old_pane_state_before_remote_attach(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(vec![test_host("remote", "ssh://remote")], cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("initial error".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux
            .read(cx)
            .registry
            .get_by_name("remote")
            .expect("remote host")
            .0;
        let fake_remote = Arc::new(FakeConnectedHost::new(test_server_hello()));

        mux.update(cx, |mux, cx| {
            let connection = mux.connections.get_mut(&remote).expect("remote slot");
            connection.state = HostState::Connected;
            connection.fake_client = Some(Arc::clone(&fake_remote));

            let pane = PaneId(23);
            let retained = Arc::new(RwLock::new(new_retained_viewport(
                TerminalViewport::blank(1, 1, SessionStatus::Running),
                &mut mux.next_row_revision,
            )));
            seed_attachment(
                mux,
                SessionId(7),
                MuxSnapshot {
                    generation: 9,
                    focused_window: None,
                    sessions: Vec::new(),
                },
            );
            mux.viewports.insert(pane, Arc::clone(&retained));
            mux.browser_commands
                .entry(pane)
                .or_default()
                .push(BrowserCommand::Reload);
            mux.screenshot_requests
                .push((pane, 32, "/tmp/old-host.png".to_owned()));
            mux.terminal_commands
                .entry(pane)
                .or_default()
                .push(TerminalUiCommand::BeginSearch {
                    direction: zz_terminal::SearchDirection::Forward,
                });
            mux.pane_images.insert(pane, PaneImageSnapshots::default());
            mux.pending_commands_revision = 17;
            mux.command_output = Some(CommandOutputModel { pane, retained });

            mux.attach_to_host(remote, SessionId(11), cx);

            assert_eq!(fake_remote.attached_session.get(), Some(SessionId(11)));
            assert_eq!(mux.attached_host, remote);
            assert_eq!(mux.core.attached_session(), None);
            assert_eq!(mux.core.snapshot().generation, 0);
            assert!(mux.core.snapshot().sessions.is_empty());
            assert!(mux.viewports.is_empty());
            assert!(mux.browser_commands.is_empty());
            assert!(mux.screenshot_requests.is_empty());
            assert!(mux.terminal_commands.is_empty());
            assert!(mux.pane_images.is_empty());
            assert!(mux.command_output.is_none());
            assert_eq!(mux.pending_commands_revision, 18);
        });
    });
}

#[ala::test]
fn a_switched_to_host_projects_its_cache_until_its_daemon_attaches(cx: &mut TestAppContext) {
    fn projected(mux: &MuxClient) -> Vec<(HostId, Option<u64>)> {
        mux.fleet_hosts()
            .map(|(host, _, _, snapshot)| (host, snapshot.map(|snapshot| snapshot.generation)))
            .collect()
    }

    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(vec![test_host("remote", "ssh://remote")], cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                std::path::PathBuf::from("/tmp/zz-fleet-switch-local.sock"),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;

        mux.update(cx, |mux, cx| {
            install_fake_connection(mux, HostId::LOCAL);
            install_fake_connection(mux, remote);
            mux.connections.get_mut(&remote).unwrap().snapshot = Some(Arc::new(MuxSnapshot {
                generation: 31,
                focused_window: None,
                sessions: vec![SessionSnapshot {
                    id: SessionId(11),
                    name: "builds".to_owned(),
                    active_window: WindowId(1),
                    windows: Vec::new(),
                    viewers: Vec::new(),
                }],
            }));
            seed_attachment(
                mux,
                SessionId(7),
                MuxSnapshot {
                    generation: 19,
                    focused_window: None,
                    sessions: Vec::new(),
                },
            );

            assert!(mux.attach_to_host(remote, SessionId(11), cx));

            assert_eq!(
                projected(mux),
                vec![(HostId::LOCAL, Some(19)), (remote, Some(31))],
            );

            mux.handle_message(
                remote,
                ProtocolMessage::Attached {
                    session: SessionId(11),
                    snapshot: MuxSnapshot {
                        generation: 41,
                        focused_window: None,
                        sessions: Vec::new(),
                    },
                    read_only: false,
                    client_flags: String::new(),
                },
                cx,
            );

            assert_eq!(
                projected(mux),
                vec![(HostId::LOCAL, Some(19)), (remote, Some(41))],
            );
            assert!(
                mux.fleet_hosts()
                    .any(|(host, _, _, snapshot)| host == remote
                        && snapshot.is_some_and(|snapshot| snapshot.sessions.is_empty()))
            );
        });
    });
}

#[test]
fn new_session_falls_back_to_an_explicit_attach_for_older_daemons() {
    assert_eq!(
        new_session_commands(HostId::LOCAL, &[], None),
        vec![
            CommandInvocation::new("new-session", [] as [&str; 0]),
            CommandInvocation::new("attach-session", [] as [&str; 0]),
        ]
    );
    assert_eq!(
        new_session_commands(
            HostId::LOCAL,
            &[NEW_SESSION_ATTACH_CAPABILITY.to_owned()],
            None,
        ),
        vec![CommandInvocation::new("new-session", [] as [&str; 0],)]
    );
}

#[test]
fn new_local_session_can_start_in_the_gui_working_directory() {
    assert_eq!(
        new_session_commands(
            HostId::LOCAL,
            &[NEW_SESSION_ATTACH_CAPABILITY.to_owned()],
            Some(Path::new("/tmp/a project")),
        ),
        vec![CommandInvocation::new(
            "new-session",
            ["-c", "/tmp/a project"],
        )]
    );
}

#[ala::test]
fn new_session_routes_to_the_requested_host_and_keeps_remote_creation_detached(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(vec![test_host("remote", "ssh://remote")], cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let (local_client, remote_client) = mux.update(cx, |mux, _cx| {
            let remote = mux.registry.get_by_name("remote").unwrap().0;
            let local_client = install_fake_connection(mux, HostId::LOCAL);
            let remote_client = install_fake_connection(mux, remote);

            mux.new_session(HostId::LOCAL);
            mux.new_session(remote);
            (local_client, remote_client)
        });

        let working_directory = std::env::current_dir()
            .expect("test working directory")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            local_client.commands.borrow().as_slice(),
            [
                CommandInvocation::new("new-session", ["-c".to_owned(), working_directory],),
                CommandInvocation::new("attach-session", [] as [&str; 0]),
            ]
        );
        assert_eq!(
            remote_client.commands.borrow().as_slice(),
            [CommandInvocation::new("new-session", ["-d"])]
        );
    });
}

#[ala::test]
fn an_empty_daemon_attach_miss_recovers_through_resync(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let client = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        client.update(cx, |client, cx| {
            client.handle_message(
                HostId::LOCAL,
                ProtocolMessage::CommandResponse(zz_protocol::CommandResponse::Error {
                    request_id: 0,
                    error: ServerError::SessionNotFound(String::new()),
                    output: zz_protocol::RawText::default(),
                }),
                cx,
            );
            client.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 1,
                    payload: EventPayload::Snapshot(MuxSnapshot {
                        generation: 7,
                        focused_window: None,
                        sessions: Vec::new(),
                    }),
                }),
                cx,
            );
        });

        assert!(client.read(cx).error().is_none());
        assert_eq!(client.read(cx).snapshot().generation, 7);
        assert!(client.read(cx).snapshot().sessions.is_empty());
    });
}
