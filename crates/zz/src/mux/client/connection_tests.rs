use std::time::{Duration, Instant};

#[cfg(unix)]
use std::{
    path::PathBuf,
    sync::{atomic::Ordering, mpsc},
};

use ala::TestAppContext;
use zz_protocol::{SessionSnapshot, WindowId};
#[cfg(unix)]
use zz_protocol::{read_protocol_message, write_protocol_message};
use zz_terminal::SessionStatus;

use super::test_support::*;
use super::*;

#[cfg(unix)]
const M3_DAEMON_CHILD_SOCKET: &str = "ZZ_M3_DAEMON_CHILD_SOCKET";

#[cfg(unix)]
const M3_DAEMON_SANDBOX_BLOCKED_EXIT: i32 = 77;

#[cfg(unix)]
struct RunningProcessTestDaemon {
    socket: PathBuf,
    test_name: &'static str,
    child: Option<std::process::Child>,
}

#[cfg(unix)]
impl RunningProcessTestDaemon {
    fn start(socket: PathBuf, test_name: &'static str) -> Option<Self> {
        let mut daemon = Self {
            socket,
            test_name,
            child: None,
        };
        daemon.restart().then_some(daemon)
    }

    fn restart(&mut self) -> bool {
        self.stop();
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(self.test_name)
            .arg("--nocapture")
            .env(M3_DAEMON_CHILD_SOCKET, &self.socket)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn process-backed test daemon");
        self.child = Some(child);

        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match zz_daemon_client::CommandClient::connect(&self.socket) {
                Ok(_) => return true,
                Err(_) => {
                    if let Some(status) = self.child.as_mut().unwrap().try_wait().unwrap() {
                        let sandbox_blocked = status.code() == Some(M3_DAEMON_SANDBOX_BLOCKED_EXIT);
                        self.stop();
                        if sandbox_blocked {
                            return false;
                        }
                        panic!("process-backed test daemon exited during startup");
                    }
                }
            }
            if Instant::now() >= deadline {
                let Err(error) = zz_daemon_client::CommandClient::connect(&self.socket) else {
                    return true;
                };
                panic!("process-backed test daemon did not start: {error}");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn stop(&mut self) {
        let Some(mut child) = self.child.take() else {
            let _ = std::fs::remove_file(&self.socket);
            return;
        };
        if child.try_wait().expect("inspect test daemon").is_none() {
            child.kill().expect("kill process-backed test daemon");
        }
        child.wait().expect("reap process-backed test daemon");
        let _ = std::fs::remove_file(&self.socket);
    }
}

#[cfg(unix)]
impl Drop for RunningProcessTestDaemon {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(unix)]
fn connected_test_client() -> Option<(
    InteractiveClient,
    thread::JoinHandle<Result<ProtocolMessage, ProtocolError>>,
)> {
    use std::os::unix::net::UnixListener;

    let socket = std::env::temp_dir().join(format!(
        "z3b-{}-{}.sock",
        std::process::id(),
        TEST_SOCKET_ID.fetch_add(1, Ordering::Relaxed),
    ));
    let _ = std::fs::remove_file(&socket);
    let listener = match UnixListener::bind(&socket) {
        Ok(listener) => listener,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return None,
        Err(error) => panic!("bind test protocol socket: {error}"),
    };
    let server_socket = socket.clone();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept test client");
        assert!(matches!(
            read_protocol_message(&mut stream).expect("read ClientHello"),
            ProtocolMessage::ClientHello(_) | ProtocolMessage::Hello(_)
        ));
        write_protocol_message(
            &mut stream,
            &ProtocolMessage::ServerHello(Box::new(test_server_hello())),
        )
        .expect("write ServerHello");
        let resync = read_protocol_message(&mut stream);
        drop(stream);
        let _ = std::fs::remove_file(server_socket);
        resync
    });
    let client = InteractiveClient::connect_endpoint(
        &zz_daemon_client::Endpoint::Local(socket),
        TerminalColorScheme::Dark,
    )
    .expect("connect test client");
    Some((client, server))
}

#[ala::test]
fn found_machines_join_the_hosts_unless_one_is_already_configured(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(zz_ui::init);
    let host = |name: &str, peer: &str| zz_daemon_client::HostEntry {
        name: name.to_owned(),
        endpoint: Endpoint::Tailnet(zz_daemon_client::TailnetEndpoint {
            peer: peer.to_owned(),
            port: None,
        }),
    };
    let mux = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(vec![host("box", "192.0.2.1")], cx);
        cx.new(|cx| {
            MuxClient::new_inner(
                None,
                zz_daemon_client::default_socket_path(),
                TerminalColorScheme::Dark,
                cx,
            )
        })
    });
    cx.update(|cx| {
        crate::config::set_discovered(
            crate::config::DiscoveredHosts {
                entries: vec![
                    host("box", "192.0.2.2"),
                    host("desk", "192.0.2.1"),
                    host("xps", "192.0.2.3"),
                ],
                enabled: true,
                ..crate::config::DiscoveredHosts::default()
            },
            cx,
        );
    });
    cx.run_until_parked();
    let remote = mux.read_with(cx, |mux, _| {
        mux.fleet_hosts()
            .filter(|(id, _, _, _)| *id != HostId::LOCAL)
            .map(|(_, name, _, _)| name.to_owned())
            .collect::<Vec<_>>()
    });
    assert_eq!(remote, ["box", "xps"]);
}

#[ala::test]
fn initial_connection_stays_pending_until_completion(cx: &mut TestAppContext) {
    let completed = std::rc::Rc::new(std::cell::Cell::new(false));
    let mux = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new_inner(
                None,
                zz_daemon_client::default_socket_path(),
                TerminalColorScheme::Light,
                cx,
            )
        });
        assert_eq!(
            mux.read(cx).attached_connection().state,
            HostState::Connecting
        );
        assert!(mux.read(cx).error().is_none());
        assert!(mux.read(cx).stale_daemon().is_none());
        assert_eq!(mux.read(cx).color_scheme, TerminalColorScheme::Light);
        let completed = completed.clone();
        cx.subscribe(&mux, move |mux, _: &InitialConnectionFinished, cx| {
            assert_eq!(
                mux.read(cx).stale_daemon(),
                Some(StaleDaemonInfo::Protocol { daemon: Some(41) })
            );
            completed.set(true);
        })
        .detach();
        mux
    });
    assert!(!completed.get());
    cx.update(|cx| {
        mux.update(cx, |mux, cx| {
            mux.finish_initial_connection(
                Err(DaemonError::IncompatibleDaemon {
                    daemon: Some(41),
                    client: PROTOCOL_VERSION,
                }),
                cx,
            );
        });
        assert_eq!(
            mux.read(cx).stale_daemon(),
            Some(StaleDaemonInfo::Protocol { daemon: Some(41) })
        );
        assert_eq!(
            mux.read(cx).attached_connection().state,
            HostState::Incompatible {
                local: PROTOCOL_VERSION,
                remote: Some(41)
            }
        );
        assert!(
            mux.read(cx)
                .error()
                .unwrap()
                .contains("daemon speaks protocol v41")
        );
    });
    assert!(completed.get());
}

#[ala::test]
fn initial_connection_cannot_report_errors_after_detach(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new_inner(
                None,
                zz_daemon_client::default_socket_path(),
                TerminalColorScheme::Dark,
                cx,
            )
        });
        mux.update(cx, |mux, cx| {
            mux.detach();
            mux.finish_initial_connection(Err(DaemonError::Thread("late failure".to_owned())), cx);
            assert!(mux.error().is_none());
            assert!(mux.attached_connection().client.is_none());
        });
    });
}

#[test]
fn reconnect_backoff_caps_at_thirty_seconds() {
    assert_eq!(
        (1..=8).map(reconnect_delay).collect::<Vec<_>>(),
        [
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(4),
            Duration::from_secs(8),
            Duration::from_secs(16),
            Duration::from_secs(30),
            Duration::from_secs(30),
            Duration::from_secs(30),
        ]
    );
}

#[test]
fn stale_reconnect_generation_does_not_match_the_armed_timer() {
    let mut connection = HostConnection::disconnected(HostId::LOCAL);
    connection.state = HostState::Reconnecting { attempt: 2 };
    let armed = connection.bump_reconnect_generation();
    assert!(reconnect_timer_is_current(&connection, armed, 2));

    connection.bump_reconnect_generation();
    assert!(!reconnect_timer_is_current(&connection, armed, 2));
}

#[test]
fn ssh_dial_failures_reach_the_host_row_as_advice() {
    let missing = connect_result_state(&Err::<(), _>(DaemonError::from(
        EndpointError::RemoteBinaryMissing {
            target: "ssh://desk".to_owned(),
        },
    )));
    assert_eq!(missing, HostState::Missing);
    assert_eq!(
        missing.install_offer(),
        Some(crate::mux::hosts::InstallOffer::Install)
    );
    for (error, expected) in [
        (
            EndpointError::SshFailed {
                target: "ssh://desk".to_owned(),
                reason: "ssh rejected the login".to_owned(),
            },
            "ssh rejected the login",
        ),
        (
            EndpointError::RemoteDaemonUnavailable {
                target: "ssh://desk".to_owned(),
            },
            "never appeared",
        ),
    ] {
        let state = connect_result_state(&Err::<(), _>(DaemonError::from(error)));
        let HostState::Unreachable { reason } = state else {
            panic!("ssh dial failures belong in an unreachable row, got {state:?}");
        };
        assert!(
            reason.contains(expected),
            "{reason:?} should say {expected:?}"
        );
    }
}

#[test]
fn only_an_older_or_unknown_daemon_epoch_is_offered_a_restart() {
    let incompatible = |daemon| DaemonError::IncompatibleDaemon {
        daemon,
        client: 110,
    };
    assert_eq!(StaleDaemonInfo::from_error(&incompatible(Some(111))), None);
    for daemon in [Some(109), Some(110), None] {
        assert_eq!(
            StaleDaemonInfo::from_error(&incompatible(daemon)),
            Some(StaleDaemonInfo::Protocol { daemon }),
            "{daemon:?}"
        );
    }
}

#[test]
fn classified_daemon_versions_map_without_inventing_v0() {
    assert_eq!(
        connect_result_state(&Err::<(), _>(DaemonError::IncompatibleDaemon {
            daemon: Some(41),
            client: PROTOCOL_VERSION,
        })),
        HostState::Incompatible {
            local: PROTOCOL_VERSION,
            remote: Some(41),
        }
    );
    let state = connect_result_state(&Err::<(), _>(DaemonError::IncompatibleDaemon {
        daemon: None,
        client: PROTOCOL_VERSION,
    }));
    let HostState::Unreachable { reason } = state else {
        panic!("an unknown daemon protocol must remain readable, got {state:?}");
    };
    assert!(reason.contains("older than this zz"));
    assert!(!reason.contains("v0"));

    assert_eq!(
        connect_result_state(&Err::<(), _>(DaemonError::from(
            EndpointError::RemoteProtocolMismatch {
                target: "host".to_owned(),
                daemon: 40,
                client: PROTOCOL_VERSION,
            }
        ))),
        HostState::Incompatible {
            local: PROTOCOL_VERSION,
            remote: Some(40),
        }
    );

    let state = connect_result_state(&Err::<(), _>(DaemonError::from(
        EndpointError::RemoteProtocolUnknown {
            target: "host".to_owned(),
        },
    )));
    assert_eq!(
        state,
        HostState::Incompatible {
            local: PROTOCOL_VERSION,
            remote: None,
        }
    );
    assert_eq!(
        state.install_offer(),
        Some(crate::mux::hosts::InstallOffer::Update)
    );

    assert_eq!(
        connect_result_state(&Err::<(), _>(DaemonError::DaemonTooOld {
            daemon: 1,
            minimum: 2,
        })),
        HostState::DaemonTooOld {
            daemon: 1,
            minimum: 2
        }
    );

    assert_eq!(
        connect_result_state(&Err::<(), _>(DaemonError::Server(
            ServerError::ClientTooOld {
                client: 1,
                minimum: 2,
            },
        ))),
        HostState::ClientTooOld {
            client: 1,
            minimum: 2
        }
    );
}

#[ala::test]
fn initial_stale_daemon_state_is_structured_and_dismissal_adds_the_manual_hint(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::IncompatibleDaemon {
                    daemon: Some(41),
                    client: PROTOCOL_VERSION,
                }),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        assert_eq!(
            mux.read(cx).stale_daemon(),
            Some(StaleDaemonInfo::Protocol { daemon: Some(41) })
        );
        mux.update(cx, super::MuxClient::dismiss_stale_daemon);
        assert!(mux.read(cx).stale_daemon().is_none());
        let error = mux.read(cx).error().unwrap();
        assert!(error.contains("daemon speaks protocol v41"));
        assert!(error.contains("Run 'zz kill-server' to restart it manually."));
    });
}

fn fail_connect(
    mux: &mut MuxClient,
    host: HostId,
    error: DaemonError,
    cx: &mut Context<MuxClient>,
) {
    mux.connections.get_mut(&host).unwrap().state = HostState::Connecting;
    mux.handle_connect_result(host, Err(error), cx);
}

#[ala::test]
fn a_host_without_zz_installs_once_and_a_failed_install_keeps_the_row(cx: &mut TestAppContext) {
    let (mux, desk, (_notification_sink, notifications)) = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(vec![test_host("desk", "ssh://desk")], cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let notifications = record_notifications(&mux, cx);
        let desk = mux
            .read(cx)
            .registry
            .get_by_name("desk")
            .expect("desk host")
            .0;
        (mux, desk, notifications)
    });
    let missing = || {
        DaemonError::from(EndpointError::RemoteBinaryMissing {
            target: "ssh://desk".to_owned(),
        })
    };
    let state = |cx: &TestAppContext| {
        cx.read(|cx| mux.read(cx).connections.get(&desk).unwrap().state.clone())
    };
    let requests =
        |cx: &TestAppContext| cx.read(|cx| mux.read(cx).install_requests.clone().unwrap());
    let auto = !zz_protocol::app_identity::DEVELOPMENT;

    cx.update(|cx| mux.update(cx, |mux, cx| fail_connect(mux, desk, missing(), cx)));
    assert_eq!(requests(cx), if auto { vec![desk] } else { vec![] });
    assert_eq!(
        state(cx),
        if auto {
            HostState::Installing
        } else {
            HostState::Missing
        }
    );

    cx.update(|cx| {
        mux.update(cx, |mux, cx| {
            mux.connections.get_mut(&desk).unwrap().state = HostState::Installing;
            mux.finish_install(
                desk,
                Some(HostState::Missing),
                "desk",
                Err("Could not install zz on ssh://desk: curl is missing".to_owned()),
                cx,
            );
        });
    });
    assert_eq!(state(cx), HostState::Missing);
    assert_eq!(
        *notifications.borrow(),
        [(
            ClientMessageKind::Error,
            "Could not install zz on ssh://desk: curl is missing".to_owned()
        )]
    );

    cx.update(|cx| mux.update(cx, |mux, cx| fail_connect(mux, desk, missing(), cx)));
    assert_eq!(state(cx), HostState::Missing);
    assert_eq!(
        requests(cx).len(),
        usize::from(auto),
        "a host still missing after its install gets no second attempt this run"
    );

    cx.update(|cx| mux.update(cx, |mux, cx| mux.install_or_retry_host(desk, cx)));
    assert_eq!(
        requests(cx).last(),
        Some(&desk),
        "asking for the missing host installs at once, in dev builds too"
    );
    assert_eq!(state(cx), HostState::Installing);
}

#[ala::test]
fn handle_connect_result_maps_success_unreachable_and_version_skew(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    assert_eq!(
        connect_result_state(&Ok::<(), DaemonError>(())),
        HostState::Connected
    );
    #[cfg(unix)]
    let (connected, resync_reader) = connected_test_client()
        .map_or((None, None), |(client, reader)| {
            (Some(client), Some(reader))
        });
    #[cfg(not(unix))]
    let connected = None;
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-step3b-remote.sock")],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
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
            if let Some(connected) = connected {
                mux.connections.get_mut(&remote).unwrap().state = HostState::Connecting;
                mux.handle_connect_result(remote, Ok(connected), cx);
                assert_eq!(
                    mux.connections.get(&remote).unwrap().state,
                    HostState::Connected
                );
            }

            mux.connections.get_mut(&remote).unwrap().state = HostState::Connecting;
            mux.handle_connect_result(
                remote,
                Err(DaemonError::Thread("remote offline".to_owned())),
                cx,
            );
            assert_eq!(
                mux.connections.get(&remote).unwrap().state.label(),
                "unreachable"
            );
            let HostState::Unreachable { reason } = &mux.connections.get(&remote).unwrap().state
            else {
                panic!("generic failure should be unreachable");
            };
            assert!(reason.contains("remote offline"));

            mux.connections.get_mut(&remote).unwrap().state = HostState::Connecting;
            mux.handle_connect_result(
                remote,
                Err(DaemonError::Protocol(ProtocolError::VersionMismatch {
                    expected: PROTOCOL_VERSION,
                    received: 41,
                })),
                cx,
            );
            assert_eq!(
                mux.connections.get(&remote).unwrap().state,
                HostState::Incompatible {
                    local: PROTOCOL_VERSION,
                    remote: Some(41),
                }
            );

            mux.connections.get_mut(&remote).unwrap().state = HostState::Connecting;
            mux.handle_connect_result(
                remote,
                Err(DaemonError::Io(io::Error::other(
                    ProtocolError::VersionMismatch {
                        expected: PROTOCOL_VERSION,
                        received: 42,
                    },
                ))),
                cx,
            );
            assert_eq!(
                mux.connections.get(&remote).unwrap().state,
                HostState::Incompatible {
                    local: PROTOCOL_VERSION,
                    remote: Some(42),
                }
            );
        });
    });
    #[cfg(unix)]
    if let Some(resync_reader) = resync_reader {
        assert!(matches!(
            resync_reader
                .join()
                .expect("join test protocol server")
                .expect("read initial resync"),
            ProtocolMessage::Resync
        ));
    }
}

#[ala::test]
fn non_attached_host_disconnect_starts_a_capped_reconnect_loop(cx: &mut TestAppContext) {
    let (mux, remote, _sink, notifications) = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-step5-remote.sock")],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("local unavailable".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        mux.update(cx, |mux, _| {
            install_fake_connection(mux, remote);
            mux.error = Some(Arc::from("keep this error"));
            seed_snapshot(
                mux,
                MuxSnapshot {
                    generation: 7,
                    focused_window: None,
                    sessions: Vec::new(),
                },
            );
        });
        let (sink, notifications) = record_notifications(&mux, cx);
        mux.update(cx, |mux, cx| {
            mux.handle_host_disconnected(remote, cx);
        });
        (mux, remote, sink, notifications)
    });

    assert!(notifications.borrow().is_empty());
    cx.update(|cx| {
        let mux = mux.read(cx);
        assert_eq!(
            mux.connections.get(&remote).unwrap().state,
            HostState::Reconnecting { attempt: 1 }
        );
        assert_eq!(mux.attached_host, HostId::LOCAL);
        assert_eq!(mux.error.as_deref(), Some("keep this error"));
        assert_eq!(mux.core.snapshot().generation, 7);
        assert!(mux.error_after_next_attach.is_none());
    });
}

#[ala::test]
fn attached_remote_disconnect_freezes_the_frame_and_remembers_the_session(cx: &mut TestAppContext) {
    let (_mux, _sink, notifications) = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-step5-remote.sock")],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("initial error".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        let (sink, notifications) = record_notifications(&mux, cx);
        mux.update(cx, |mux, cx| {
            let local = install_fake_connection(mux, HostId::LOCAL);
            install_fake_connection(mux, remote);
            mux.attached_host = remote;
            seed_attachment(
                mux,
                SessionId(9),
                MuxSnapshot {
                    generation: 7,
                    focused_window: None,
                    sessions: Vec::new(),
                },
            );
            let snapshot = Arc::clone(mux.core.snapshot());
            let pane = PaneId(23);
            let viewport = Arc::new(RwLock::new(new_retained_viewport(
                TerminalViewport::blank(2, 2, SessionStatus::Running),
                &mut mux.next_row_revision,
            )));
            mux.viewports.insert(pane, Arc::clone(&viewport));
            seed_choose_tree(mux);
            mux.error = Some(Arc::from("stale daemon error"));

            mux.handle_host_disconnected(remote, cx);

            assert_eq!(
                mux.connections.get(&remote).unwrap().state,
                HostState::Reconnecting { attempt: 1 }
            );
            let connection = mux.connections.get(&remote).unwrap();
            assert_eq!(connection.last_attached_session, Some(SessionId(9)));
            assert!(connection.reconnect_generation > 0);
            assert_eq!(mux.attached_host, remote);
            assert_eq!(mux.core.attached_session(), Some(SessionId(9)));
            assert!(Arc::ptr_eq(mux.core.snapshot(), &snapshot));
            assert!(Arc::ptr_eq(
                mux.viewports.get(&pane).expect("frozen viewport"),
                &viewport
            ));
            assert!(mux.core.choose_tree().is_none());
            assert!(mux.error.is_none());
            assert!(mux.error_after_next_attach.is_none());
            assert!(
                !local.attached_default.get(),
                "an involuntary remote disconnect must not fall back to local"
            );
        });
        (mux, sink, notifications)
    });
    assert_eq!(
        &*notifications.borrow(),
        &[(
            ClientMessageKind::Warning,
            "connection to remote lost: reconnecting…".to_owned(),
        )]
    );
}

#[ala::test]
fn attached_remote_disconnect_reconnects_even_when_local_is_down(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-step5-remote.sock")],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("local unavailable".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        mux.update(cx, |mux, cx| {
            install_fake_connection(mux, remote);
            mux.attached_host = remote;
            seed_attachment(
                mux,
                SessionId(9),
                MuxSnapshot {
                    generation: 7,
                    focused_window: None,
                    sessions: Vec::new(),
                },
            );
            seed_choose_tree(mux);
            mux.error = None;

            mux.handle_host_disconnected(remote, cx);

            assert_eq!(
                mux.connections.get(&remote).unwrap().state,
                HostState::Reconnecting { attempt: 1 }
            );
            assert_eq!(mux.attached_host, remote);
            assert_eq!(mux.core.attached_session(), Some(SessionId(9)));
            assert_eq!(mux.core.snapshot().generation, 7);
            assert!(mux.core.choose_tree().is_none());
            assert!(mux.error.is_none());
            assert!(mux.error_after_next_attach.is_none());
        });
    });
}

#[ala::test]
fn reconnect_reingests_hello_and_reattaches_the_remembered_session(cx: &mut TestAppContext) {
    let (mux, remote, fake, _sink, notifications) = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-reconnect-fake.sock")],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("local unavailable".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        let (sink, notifications) = record_notifications(&mux, cx);
        let mut hello = test_server_hello();
        hello.status.left = "fresh reconnect hello".to_owned();
        let fake = Arc::new(FakeConnectedHost::new(hello));
        mux.update(cx, |mux, _| {
            let connection = mux.connections.get_mut(&remote).unwrap();
            connection.state = HostState::Connected;
            connection.fake_client = Some(Arc::clone(&fake));
            connection.last_attached_session = Some(SessionId(9));
            mux.attached_host = remote;
            seed_attachment(
                mux,
                SessionId(9),
                MuxSnapshot {
                    generation: 7,
                    focused_window: None,
                    sessions: Vec::new(),
                },
            );
            mux.seed_core(EventPayload::StatusChanged {
                status: StatusLine {
                    left: "stale".to_owned(),
                    ..StatusLine::default()
                },
            });
        });
        (mux, remote, fake, sink, notifications)
    });
    cx.update(|cx| {
        mux.update(cx, |mux, cx| {
            mux.reattach_after_reconnect(remote, cx);

            assert_eq!(fake.attached_session.get(), Some(SessionId(9)));
            assert!(!fake.attached_default.get());
            assert_eq!(mux.core.status().left, "fresh reconnect hello");
            assert_eq!(
                mux.connections.get(&remote).unwrap().reconnect_attach,
                Some(ReconnectAttachState::RememberedSession)
            );
            assert_eq!(
                (mux.core.snapshot().generation, mux.core.attached_session()),
                (7, Some(SessionId(9))),
                "re-ingesting the hello must not blank the frame the daemon has \
                 not re-attached yet",
            );

            mux.handle_message(
                remote,
                ProtocolMessage::Attached {
                    session: SessionId(9),
                    snapshot: MuxSnapshot {
                        generation: 12,
                        focused_window: None,
                        sessions: Vec::new(),
                    },
                    read_only: false,
                    client_flags: String::new(),
                },
                cx,
            );
            let connection = mux.connections.get(&remote).unwrap();
            assert_eq!(connection.last_attached_session, Some(SessionId(9)));
            assert!(connection.reconnect_attach.is_none());
            assert_eq!(mux.core.snapshot().generation, 12);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        &*notifications.borrow(),
        &[(
            ClientMessageKind::Success,
            "reconnected to remote".to_owned(),
        )]
    );
}

#[ala::test]
fn missing_remembered_session_falls_back_to_default_without_an_error_banner(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-reconnect-missing.sock")],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("local unavailable".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        let fake = Arc::new(FakeConnectedHost::new(test_server_hello()));
        mux.update(cx, |mux, cx| {
            let connection = mux.connections.get_mut(&remote).unwrap();
            connection.state = HostState::Connected;
            connection.fake_client = Some(Arc::clone(&fake));
            connection.last_attached_session = Some(SessionId(9));
            mux.attached_host = remote;
            seed_attachment(mux, SessionId(9), MuxSnapshot::default());
            mux.error = None;
            mux.reattach_after_reconnect(remote, cx);

            mux.handle_message(
                remote,
                ProtocolMessage::CommandResponse(zz_protocol::CommandResponse::Error {
                    request_id: 0,
                    error: ServerError::SessionNotFound("9".to_owned()),
                    output: zz_protocol::RawText::default(),
                }),
                cx,
            );

            assert_eq!(fake.attached_session.get(), Some(SessionId(9)));
            assert!(fake.attached_default.get());
            assert_eq!(
                mux.connections.get(&remote).unwrap().reconnect_attach,
                Some(ReconnectAttachState::DefaultSession)
            );
            assert!(mux.error.is_none());
            assert!(mux.error_after_next_attach.is_none());
        });
    });
}

#[ala::test]
fn an_unattached_host_keeps_retrying_until_it_asks_to_sign_in(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (mux, remote) = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host(
                "remote",
                "unix:///tmp/zz-reconnect-backoff-does-not-exist.sock",
            )],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("local unavailable".to_owned())),
                PathBuf::from("/tmp/zz-reconnect-backoff-local-does-not-exist.sock"),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        mux.update(cx, |mux, cx| {
            install_fake_connection(mux, remote);
            mux.handle_host_disconnected(remote, cx);
        });
        (mux, remote)
    });
    let state =
        |cx: &mut TestAppContext| cx.update(|cx| mux.read(cx).connections[&remote].state.clone());
    cx.run_until_parked();

    let mut delays = Vec::new();
    for attempt in 1..=9 {
        if attempt == 9 {
            cx.update(|cx| {
                mux.update(cx, |mux, _| {
                    mux.connections.get_mut(&remote).unwrap().asked_to_sign_in = true;
                });
            });
        }
        let mut waited = 0;
        while state(cx) == (HostState::Reconnecting { attempt }) {
            assert!(waited < 60, "attempt {attempt} never redialed");
            cx.executor().advance_clock(Duration::from_secs(1));
            cx.run_until_parked();
            waited += 1;
        }
        delays.push(waited);
        if attempt < 9 {
            wait_for_mux(cx, &mux, "the next reconnect rung", |mux| {
                mux.connections[&remote].state
                    == (HostState::Reconnecting {
                        attempt: attempt + 1,
                    })
            });
        }
    }
    assert_eq!(delays, [1, 2, 4, 8, 16, 30, 30, 30, 30]);
    wait_for_mux(cx, &mux, "a sign-in host giving up", |mux| {
        matches!(
            mux.connections[&remote].state,
            HostState::Unreachable { .. }
        )
    });

    cx.update(|cx| {
        mux.update(cx, |mux, cx| {
            mux.retry_host_now(remote, cx);
            let connection = mux.connections.get(&remote).unwrap();
            assert_eq!(connection.state, HostState::Connecting);
            assert_eq!(connection.reconnect_attempt_in_flight, Some(1));
        });
    });
    std::thread::sleep(Duration::from_millis(50));
    cx.run_until_parked();
    drop(mux);
}

#[ala::test]
fn a_dismissed_ssh_prompt_parks_the_host_until_reconnect_is_picked(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let mux = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host(
                "remote",
                "unix:///tmp/zz-askpass-declined-does-not-exist.sock",
            )],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("local unavailable".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        mux.update(cx, |mux, cx| {
            install_fake_connection(mux, remote);
            mux.handle_host_disconnected(remote, cx);

            let connection = mux.connections.get_mut(&remote).unwrap();
            assert_eq!(connection.state, HostState::Reconnecting { attempt: 1 });
            connection.bump_reconnect_generation();
            connection.state = HostState::Connecting;
            connection.reconnect_attempt_in_flight = Some(1);

            mux.note_ssh_auth_declined(remote, cx);
            mux.handle_connect_result(
                remote,
                Err(DaemonError::Thread("Permission denied".to_owned())),
                cx,
            );

            let HostState::Unreachable { reason } = &mux.connections.get(&remote).unwrap().state
            else {
                panic!("a dismissed prompt must park the host, not schedule another rung");
            };
            assert!(reason.contains("Authentication was cancelled"), "{reason}");

            mux.ensure_connected(remote, cx);
            assert!(matches!(
                mux.connections.get(&remote).unwrap().state,
                HostState::Unreachable { .. }
            ));

            mux.retry_host_now(remote, cx);
            assert_eq!(
                mux.connections.get(&remote).unwrap().state,
                HostState::Connecting,
            );
        });
        mux
    });
    std::thread::sleep(Duration::from_millis(50));
    cx.run_until_parked();
    drop(mux);
}

#[test]
fn the_prompt_dialog_names_the_ssh_destination() {
    let label = |uri: &str| {
        let Endpoint::Ssh(endpoint) = Endpoint::parse(uri).expect(uri) else {
            panic!("{uri} is an ssh endpoint");
        };
        ssh_destination_label(&endpoint)
    };
    assert_eq!(label("ssh://fabrico@xps"), "fabrico@xps");
    assert_eq!(label("ssh://xps"), "xps");
    assert_eq!(label("ssh://fabrico@xps:2222"), "fabrico@xps:2222");
    assert_eq!(label("ssh://[fe80::1]:2222"), "[fe80::1]:2222");
}

#[ala::test]
fn attached_remote_reconnect_never_gives_up(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-reconnect-give-up.sock")],
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
            install_fake_connection(mux, HostId::LOCAL);
            install_fake_connection(mux, remote);
            mux.attached_host = remote;
            seed_attachment(mux, SessionId(9), MuxSnapshot::default());
            mux.handle_host_disconnected(remote, cx);

            for attempt in 1..=(MAX_SIGN_IN_RECONNECT_ATTEMPTS + 2) {
                let connection = mux.connections.get_mut(&remote).unwrap();
                assert_eq!(connection.state, HostState::Reconnecting { attempt });
                connection.bump_reconnect_generation();
                connection.state = HostState::Connecting;
                connection.reconnect_attempt_in_flight = Some(attempt);
                mux.handle_connect_result(
                    remote,
                    Err(DaemonError::Thread(format!("failure {attempt}"))),
                    cx,
                );
            }

            assert_eq!(mux.attached_host, remote);
            assert_eq!(
                mux.connections[&remote].state,
                HostState::Reconnecting {
                    attempt: MAX_SIGN_IN_RECONNECT_ATTEMPTS + 3
                }
            );
        });
    });
}

#[ala::test]
fn an_attached_remote_that_outgrew_this_zz_stops_reconnecting(cx: &mut TestAppContext) {
    let newer = PROTOCOL_VERSION + 1;
    for (error, state, text) in [
        (
            DaemonError::IncompatibleDaemon {
                daemon: Some(newer),
                client: PROTOCOL_VERSION,
            },
            HostState::Incompatible {
                local: PROTOCOL_VERSION,
                remote: Some(newer),
            },
            format!(
                "This zz is too old for the running daemon. It speaks protocol \
                 v{PROTOCOL_VERSION}, and the daemon speaks v{newer}. Update zz."
            ),
        ),
        (
            DaemonError::Server(ServerError::ClientTooOld {
                client: 40,
                minimum: 41,
            }),
            HostState::ClientTooOld {
                client: 40,
                minimum: 41,
            },
            "This zz is too old for the running daemon. It is revision 40, and the daemon \
             needs revision 41 or newer. Update zz."
                .to_owned(),
        ),
    ] {
        cx.update(|cx| {
            crate::config::set_fleet_hosts_for_test(
                vec![test_host("remote", "unix:///tmp/zz-outgrown-remote.sock")],
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
                install_fake_connection(mux, HostId::LOCAL);
                install_fake_connection(mux, remote);
                mux.attached_host = remote;
                seed_attachment(mux, SessionId(9), MuxSnapshot::default());
                mux.handle_host_disconnected(remote, cx);

                let connection = mux.connections.get_mut(&remote).unwrap();
                assert_eq!(connection.state, HostState::Reconnecting { attempt: 1 });
                connection.bump_reconnect_generation();
                connection.state = HostState::Connecting;
                connection.reconnect_attempt_in_flight = Some(1);
                mux.handle_connect_result(remote, Err(error), cx);

                assert_eq!(mux.connections[&remote].state, state);
                assert_eq!(mux.error.as_deref(), Some(text.as_str()));
                assert!(mux.attached_client_too_old());
            });
        });
    }
}

#[ala::test]
fn abrupt_local_disconnect_preserves_snapshot_and_reconnects_unless_shutting_down(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("initial error".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        mux.update(cx, |mux, cx| {
            install_fake_connection(mux, HostId::LOCAL);
            let cached = Arc::new(MuxSnapshot {
                generation: 7,
                focused_window: None,
                sessions: Vec::new(),
            });
            mux.connections.get_mut(&HostId::LOCAL).unwrap().snapshot = Some(Arc::clone(&cached));
            mux.error = None;

            mux.handle_host_disconnected(HostId::LOCAL, cx);

            let connection = mux.connections.get(&HostId::LOCAL).unwrap();
            assert_eq!(connection.state, HostState::Reconnecting { attempt: 1 });
            assert!(
                connection
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| Arc::ptr_eq(snapshot, &cached))
            );
            assert!(connection.reconnect_generation > 0);
            assert_eq!(mux.attached_host, HostId::LOCAL);
            assert_eq!(mux.error.as_deref(), Some("fleet host local disconnected"));

            install_fake_connection(mux, HostId::LOCAL);
            mux.connections.get_mut(&HostId::LOCAL).unwrap().snapshot = Some(Arc::clone(&cached));
            let reconnect_generation = mux.connections[&HostId::LOCAL].reconnect_generation;
            mux.detach();
            mux.handle_host_disconnected(HostId::LOCAL, cx);

            let connection = mux.connections.get(&HostId::LOCAL).unwrap();
            assert_eq!(
                connection.state,
                HostState::Unreachable {
                    reason: "connection lost".to_owned(),
                }
            );
            assert!(connection.snapshot.is_none());
            assert_eq!(connection.reconnect_generation, reconnect_generation);
        });
    });
}

#[cfg(unix)]
#[ala::test]
fn attached_remote_stream_eof_reconnects_and_reattaches_the_same_session(cx: &mut TestAppContext) {
    use std::os::unix::net::UnixListener;

    cx.executor().allow_parking();
    let socket = std::env::temp_dir().join(format!(
        "z5-eof-{}-{}.sock",
        std::process::id(),
        TEST_SOCKET_ID.fetch_add(1, Ordering::Relaxed),
    ));
    let _ = std::fs::remove_file(&socket);
    let listener = match UnixListener::bind(&socket) {
        Ok(listener) => listener,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind fake remote socket: {error}"),
    };
    let remote_session = SessionId(41);
    let remote_snapshot = MuxSnapshot {
        generation: 2,
        focused_window: None,
        sessions: vec![SessionSnapshot {
            id: remote_session,
            name: "remote-default".to_owned(),
            active_window: WindowId(1),
            windows: Vec::new(),
            viewers: Vec::new(),
        }],
    };
    let (close_remote, close_remote_receiver) = mpsc::channel();
    let (remote_dropped, remote_dropped_receiver) = mpsc::channel();
    let (respawn_remote, respawn_remote_receiver) = mpsc::channel();
    let (finish_remote, finish_remote_receiver) = mpsc::channel();
    let server_socket = socket.clone();
    let remote_endpoint = format!("unix://{}", socket.display());
    let server_snapshot = remote_snapshot.clone();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept fake remote client");
        assert!(matches!(
            read_protocol_message(&mut stream).expect("read ClientHello"),
            ProtocolMessage::ClientHello(_) | ProtocolMessage::Hello(_)
        ));
        write_protocol_message(
            &mut stream,
            &ProtocolMessage::ServerHello(Box::new(test_server_hello())),
        )
        .expect("write ServerHello");
        assert!(matches!(
            read_protocol_message(&mut stream).expect("read initial Resync"),
            ProtocolMessage::Resync
        ));
        write_protocol_message(
            &mut stream,
            &ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: EventPayload::Snapshot(server_snapshot.clone()),
            }),
        )
        .expect("write connect-time snapshot");

        loop {
            match read_protocol_message(&mut stream).expect("read remote attach") {
                ProtocolMessage::Attach { session } => {
                    assert_eq!(session, remote_session.to_string());
                    break;
                }
                ProtocolMessage::SetColorScheme(_) | ProtocolMessage::SetConfigOverrides { .. } => {
                }
                message => panic!("unexpected message before remote attach: {message:?}"),
            }
        }
        write_protocol_message(
            &mut stream,
            &ProtocolMessage::Attached {
                session: remote_session,
                snapshot: server_snapshot.clone(),
                read_only: false,
                client_flags: String::new(),
            },
        )
        .expect("write remote Attached");
        write_protocol_message(
            &mut stream,
            &ProtocolMessage::Event(zz_protocol::Event {
                sequence: 2,
                payload: EventPayload::TerminalViewport {
                    pane: PaneId(77),
                    viewport: TerminalViewport::blank(2, 2, SessionStatus::Running),
                },
            }),
        )
        .expect("write first remote frame");

        close_remote_receiver
            .recv()
            .expect("wait to close attached remote stream");
        drop(stream);
        drop(listener);
        let _ = std::fs::remove_file(&server_socket);
        remote_dropped.send(()).expect("report remote drop");
        respawn_remote_receiver
            .recv()
            .expect("wait to respawn remote");

        let listener = UnixListener::bind(&server_socket).expect("rebind fake remote socket");
        let (mut stream, _) = listener.accept().expect("accept reconnected remote client");
        assert!(matches!(
            read_protocol_message(&mut stream).expect("read reconnect ClientHello"),
            ProtocolMessage::ClientHello(_) | ProtocolMessage::Hello(_)
        ));
        write_protocol_message(
            &mut stream,
            &ProtocolMessage::ServerHello(Box::new(test_server_hello())),
        )
        .expect("write reconnect ServerHello");
        assert!(matches!(
            read_protocol_message(&mut stream).expect("read reconnect Resync"),
            ProtocolMessage::Resync
        ));
        let mut reconnected_snapshot = server_snapshot;
        reconnected_snapshot.generation = 3;
        write_protocol_message(
            &mut stream,
            &ProtocolMessage::Event(zz_protocol::Event {
                sequence: 3,
                payload: EventPayload::Snapshot(reconnected_snapshot.clone()),
            }),
        )
        .expect("write reconnect snapshot");
        loop {
            match read_protocol_message(&mut stream).expect("read reconnect attach") {
                ProtocolMessage::Attach { session } => {
                    assert_eq!(session, remote_session.to_string());
                    break;
                }
                ProtocolMessage::SetColorScheme(_) | ProtocolMessage::SetConfigOverrides { .. } => {
                }
                message => panic!("unexpected reconnect message before attach: {message:?}"),
            }
        }
        write_protocol_message(
            &mut stream,
            &ProtocolMessage::Attached {
                session: remote_session,
                snapshot: reconnected_snapshot,
                read_only: false,
                client_flags: String::new(),
            },
        )
        .expect("write reconnect Attached");
        write_protocol_message(
            &mut stream,
            &ProtocolMessage::Event(zz_protocol::Event {
                sequence: 4,
                payload: EventPayload::TerminalViewport {
                    pane: PaneId(77),
                    viewport: TerminalViewport::blank(3, 2, SessionStatus::Running),
                },
            }),
        )
        .expect("write frame after reconnect");
        finish_remote_receiver
            .recv()
            .expect("wait to finish reconnected remote");
        drop(stream);
        drop(listener);
        let _ = std::fs::remove_file(&server_socket);
    });
    let remote_client = InteractiveClient::connect_endpoint(
        &zz_daemon_client::Endpoint::Local(socket),
        TerminalColorScheme::Dark,
    )
    .expect("connect fake remote client");

    let (mux, remote, local) = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(vec![test_host("remote", &remote_endpoint)], cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("local test transport".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        let local = mux.update(cx, |mux, cx| {
            let local = install_fake_connection(mux, HostId::LOCAL);
            mux.connections.get_mut(&remote).unwrap().state = HostState::Connecting;
            mux.handle_connect_result(remote, Ok(remote_client), cx);
            local
        });
        (mux, remote, local)
    });

    wait_for_mux(cx, &mux, "fake remote connect-time snapshot", |mux| {
        mux.connections.get(&remote).is_some_and(|connection| {
            connection.state == HostState::Connected
                && connection.snapshot.as_deref() == Some(&remote_snapshot)
        })
    });
    cx.update(|cx| {
        mux.update(cx, |mux, cx| {
            mux.attach_to_host(remote, remote_session, cx);
        });
    });
    wait_for_mux(cx, &mux, "fake remote Attached response", |mux| {
        mux.attached_host == remote
            && mux.core.attached_session() == Some(remote_session)
            && mux
                .viewports
                .get(&PaneId(77))
                .is_some_and(|viewport| viewport.read().viewport.columns == 2)
    });
    let (frozen_snapshot, frozen_viewport) = cx.update(|cx| {
        let mux = mux.read(cx);
        (
            Arc::clone(mux.core.snapshot()),
            Arc::clone(mux.viewports.get(&PaneId(77)).expect("first remote frame")),
        )
    });

    close_remote
        .send(())
        .expect("close attached fake remote stream");
    remote_dropped_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("remote socket dropped");
    wait_for_mux(cx, &mux, "frozen reconnect state", |mux| {
        mux.attached_host == remote
            && mux.core.attached_session() == Some(remote_session)
            && mux.error.is_none()
            && !local.attached_default.get()
            && Arc::ptr_eq(mux.core.snapshot(), &frozen_snapshot)
            && mux
                .viewports
                .get(&PaneId(77))
                .is_some_and(|viewport| Arc::ptr_eq(viewport, &frozen_viewport))
            && mux.connections.get(&remote).is_some_and(|connection| {
                connection.state == (HostState::Reconnecting { attempt: 1 })
                    && connection.last_attached_session == Some(remote_session)
            })
    });
    respawn_remote.send(()).expect("respawn fake remote");
    wait_for_mux(cx, &mux, "same-session reconnect frame", |mux| {
        mux.attached_host == remote
            && mux.core.attached_session() == Some(remote_session)
            && mux.core.snapshot().generation == 3
            && mux
                .viewports
                .get(&PaneId(77))
                .is_some_and(|viewport| viewport.read().viewport.columns == 3)
            && mux.connections.get(&remote).is_some_and(|connection| {
                connection.state == HostState::Connected
                    && connection.last_attached_session == Some(remote_session)
                    && connection.reconnect_attach.is_none()
            })
    });
    finish_remote.send(()).expect("finish fake remote");
    server.join().expect("join fake remote server");
}

#[cfg(unix)]
#[ala::test]
fn attached_remote_daemon_process_restart_reconnects_and_receives_new_frames(
    cx: &mut TestAppContext,
) {
    const TEST_NAME: &str = "mux::client::connection_tests::\
        attached_remote_daemon_process_restart_reconnects_and_receives_new_frames";
    if let Some(socket) = std::env::var_os(M3_DAEMON_CHILD_SOCKET) {
        cx.executor().allow_parking();
        let result = zz_daemon::Daemon::new(PathBuf::from(socket))
            .without_user_config()
            .run_foreground();
        match result {
            Err(DaemonError::Io(error)) if error.kind() == io::ErrorKind::PermissionDenied => {
                std::process::exit(M3_DAEMON_SANDBOX_BLOCKED_EXIT);
            }
            result => result.expect("run process-backed test daemon"),
        }
        return;
    }

    cx.executor().allow_parking();
    let socket = PathBuf::from(format!(
        "/tmp/zz-m3-process-{}-{}.sock",
        std::process::id(),
        TEST_SOCKET_ID.fetch_add(1, Ordering::Relaxed),
    ));
    let Some(mut remote_daemon) = RunningProcessTestDaemon::start(socket.clone(), TEST_NAME) else {
        return;
    };
    let remote_endpoint = format!("unix://{}", socket.display());
    let (mux, remote) = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(vec![test_host("remote", &remote_endpoint)], cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("local unavailable".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        mux.update(cx, |mux, cx| mux.ensure_connected(remote, cx));
        (mux, remote)
    });

    wait_for_mux(cx, &mux, "process-backed remote connection", |mux| {
        mux.connections.get(&remote).is_some_and(|connection| {
            connection.state == HostState::Connected && connection.snapshot.is_some()
        })
    });
    cx.update(|cx| {
        mux.update(cx, |mux, cx| {
            assert!(mux.attach_to_host_default(remote, cx));
        });
    });
    wait_for_mux(
        cx,
        &mux,
        "lazily created process-backed remote session",
        |mux| {
            mux.attached_host == remote
                && mux.core.attached_session().is_some()
                && !mux.core.snapshot().sessions.is_empty()
                && !mux.viewports.is_empty()
        },
    );
    let (session, session_name) = cx.update(|cx| {
        let mux = mux.read(cx);
        let session = mux.core.snapshot().sessions.first().unwrap();
        (session.id, session.name.clone())
    });
    let pane = cx.update(|cx| {
        let mux = mux.read(cx);
        *mux.viewports.first_key_value().unwrap().0
    });

    remote_daemon.stop();
    wait_for_mux(cx, &mux, "process-backed daemon reconnect state", |mux| {
        mux.attached_host == remote
            && mux.core.attached_session() == Some(session)
            && mux.error.is_none()
            && mux
                .core
                .snapshot()
                .sessions
                .iter()
                .any(|candidate| candidate.id == session)
            && mux.viewports.contains_key(&pane)
            && mux.connections.get(&remote).is_some_and(|connection| {
                matches!(connection.state, HostState::Reconnecting { .. })
                    && connection.last_attached_session == Some(session)
            })
    });
    let (frozen_snapshot, frozen_viewport) = cx.update(|cx| {
        let mux = mux.read(cx);
        (
            Arc::clone(mux.core.snapshot()),
            Arc::clone(mux.viewports.get(&pane).unwrap()),
        )
    });

    assert!(
        remote_daemon.restart(),
        "a daemon restart should remain permitted after its first successful start"
    );
    wait_for_mux(
        cx,
        &mux,
        "process-backed same-session reattach and fresh frame",
        |mux| {
            mux.attached_host == remote
                && mux.core.attached_session() == Some(session)
                && mux
                    .core
                    .snapshot()
                    .sessions
                    .iter()
                    .any(|candidate| candidate.id == session && candidate.name == session_name)
                && !Arc::ptr_eq(mux.core.snapshot(), &frozen_snapshot)
                && mux
                    .viewports
                    .get(&pane)
                    .is_some_and(|viewport| !Arc::ptr_eq(viewport, &frozen_viewport))
                && mux.connections.get(&remote).is_some_and(|connection| {
                    connection.state == HostState::Connected
                        && connection.last_attached_session == Some(session)
                        && connection.reconnect_attach.is_none()
                })
        },
    );

    drop(mux);
    remote_daemon.stop();
}

#[ala::test]
fn ensure_connected_is_a_noop_while_connecting_or_connected(cx: &mut TestAppContext) {
    let mux = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host(
                "remote",
                "unix:///tmp/zz-step3b-does-not-exist.sock",
            )],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
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
            mux.connections.get_mut(&remote).unwrap().state = HostState::Connecting;
            mux.ensure_connected(remote, cx);
        });
        (mux, remote)
    });

    std::thread::sleep(Duration::from_millis(50));
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            mux.0.read(cx).connections.get(&mux.1).unwrap().state,
            HostState::Connecting
        );
        mux.0.update(cx, |mux_client, cx| {
            mux_client.connections.get_mut(&mux.1).unwrap().state = HostState::Connected;
            mux_client.ensure_connected(mux.1, cx);
            assert_eq!(
                mux_client.connections.get(&mux.1).unwrap().state,
                HostState::Connected
            );
        });
    });
}

#[ala::test]
fn a_published_fleet_host_registers_and_starts_connecting(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let mux = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        })
    });
    cx.update(|cx| assert!(mux.read(cx).registry.get_by_name("studio").is_none()));

    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("studio", "unix:///tmp/zz-published-studio.sock")],
            cx,
        );
    });

    cx.update(|cx| {
        let mux = mux.read(cx);
        let studio = mux
            .registry
            .get_by_name("studio")
            .expect("publishing a fleet host registers it")
            .0;
        assert_ne!(
            mux.connections.get(&studio).unwrap().state,
            HostState::Disconnected,
            "a newly published host dials without waiting for a restart"
        );
    });
}

#[ala::test]
fn host_state_access_reconciles_changed_fleet_hosts_by_name(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (mux, server_id) = cx.update(|cx| {
        let desktop = test_host(
            "desktop",
            "unix:///tmp/zz-reconcile-desktop-does-not-exist.sock",
        );
        let server = test_host(
            "server",
            "unix:///tmp/zz-reconcile-server-does-not-exist.sock",
        );
        crate::config::set_fleet_hosts_for_test(vec![desktop, server.clone()], cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });

        mux.update(cx, |mux, _| {
            mux.connections.get_mut(&HostId::LOCAL).unwrap().state = HostState::Unreachable {
                reason: "local kept".to_owned(),
            };
            let desktop = mux.registry.get_by_name("desktop").unwrap().0;
            mux.connections.get_mut(&desktop).unwrap().state = HostState::Unreachable {
                reason: "desktop dropped".to_owned(),
            };
            let server = mux.registry.get_by_name("server").unwrap().0;
            mux.connections.get_mut(&server).unwrap().state = HostState::Unreachable {
                reason: "server kept".to_owned(),
            };
        });
        let (desktop_route, server_route) = {
            let mux = mux.read(cx);
            let desktop = mux.registry.get_by_name("desktop").unwrap().0;
            let server = mux.registry.get_by_name("server").unwrap().0;
            (
                Arc::clone(&mux.connections.get(&desktop).unwrap().route),
                Arc::clone(&mux.connections.get(&server).unwrap().route),
            )
        };

        crate::config::set_fleet_hosts_for_test(vec![server], cx);
        mux.update(cx, |mux, cx| {
            let states = mux
                .host_states(cx)
                .map(|(host, name, state)| (host, name.to_owned(), state.clone()))
                .collect::<Vec<_>>();
            assert_eq!(
                states,
                [
                    (
                        HostId::LOCAL,
                        "local".to_owned(),
                        HostState::Unreachable {
                            reason: "local kept".to_owned(),
                        },
                    ),
                    (
                        mux.registry.get_by_name("server").unwrap().0,
                        "server".to_owned(),
                        HostState::Connecting,
                    ),
                ]
            );
            assert!(mux.registry.get_by_name("desktop").is_none());
            assert_eq!(mux.connections.len(), 2);
            assert_eq!(*desktop_route.read(), None);
            assert_eq!(
                *server_route.read(),
                Some(mux.registry.get_by_name("server").unwrap().0)
            );
        });
        let server_id = mux.read(cx).registry.get_by_name("server").unwrap().0;
        (mux, server_id)
    });
    wait_for_mux(cx, &mux, "config-reconciled connection attempt", |mux| {
        matches!(
            mux.connections
                .get(&server_id)
                .map(|connection| &connection.state),
            Some(HostState::Unreachable { .. })
        )
    });
}

#[ala::test]
fn reconcile_retains_a_removed_attached_host_until_it_is_no_longer_attached(
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
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        let route = Arc::clone(&mux.read(cx).connections.get(&remote).unwrap().route);
        mux.update(cx, |mux, _| {
            mux.connections.get_mut(&remote).unwrap().state = HostState::Connected;
            mux.attached_host = remote;
        });

        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        mux.update(cx, |mux, cx| {
            let states = mux
                .host_states(cx)
                .map(|(_, name, state)| (name.to_owned(), state.clone()))
                .collect::<Vec<_>>();
            assert_eq!(
                states,
                [
                    ("local".to_owned(), HostState::Disconnected),
                    ("remote".to_owned(), HostState::Connected),
                ]
            );
            assert!(mux.registry.is_retained(mux.attached_host));

            mux.attached_host = HostId::LOCAL;
            assert_eq!(mux.host_states(cx).count(), 1);
            assert!(mux.registry.get_by_name("remote").is_none());
            assert_eq!(*route.read(), None);
        });
    });
}

#[cfg(unix)]
#[ala::test]
fn config_overrides_reach_only_the_local_daemon(cx: &mut TestAppContext) {
    use std::os::unix::net::UnixListener;

    fn serve(name: &str) -> Option<(PathBuf, mpsc::Receiver<ProtocolMessage>)> {
        let socket = std::env::temp_dir().join(format!(
            "zco-{name}-{}-{}.sock",
            std::process::id(),
            TEST_SOCKET_ID.fetch_add(1, Ordering::Relaxed),
        ));
        let _ = std::fs::remove_file(&socket);
        let listener = match UnixListener::bind(&socket) {
            Ok(listener) => listener,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return None,
            Err(error) => panic!("bind fake daemon socket: {error}"),
        };
        let (heard, hearing) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept test client");
            read_protocol_message(&mut stream).expect("read ClientHello");
            let mut hello = test_server_hello();
            hello.capabilities.push("config-overrides-v1".to_owned());
            write_protocol_message(&mut stream, &ProtocolMessage::ServerHello(Box::new(hello)))
                .expect("write ServerHello");
            while let Ok(message) = read_protocol_message(&mut stream) {
                if heard.send(message).is_err() {
                    break;
                }
            }
        });
        Some((socket, hearing))
    }

    fn heard_until(
        hearing: &mpsc::Receiver<ProtocolMessage>,
        last: &ProtocolMessage,
    ) -> Vec<ProtocolMessage> {
        let mut heard = Vec::new();
        while heard.last() != Some(last) {
            heard.push(
                hearing
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap_or_else(|_| panic!("waiting for {last:?}, heard {heard:?}")),
            );
        }
        heard
    }

    cx.executor().allow_parking();
    let (Some((local_socket, local_hearing)), Some((remote_socket, remote_hearing))) =
        (serve("local"), serve("remote"))
    else {
        return;
    };
    let connect = |socket: &Path| {
        InteractiveClient::connect_endpoint(
            &zz_daemon_client::Endpoint::Local(socket.to_path_buf()),
            TerminalColorScheme::Dark,
        )
        .expect("connect test client")
    };
    let local_client = connect(&local_socket);
    let remote_client = connect(&remote_socket);
    let overrides = vec![("font-size".to_owned(), "13".to_owned())];

    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host(
                "remote",
                &format!("unix://{}", remote_socket.display()),
            )],
            cx,
        );
        crate::config::set_daemon_config_overrides_for_test(overrides.clone(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("local comes up later".to_owned())),
                local_socket.clone(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;
        mux.update(cx, |mux, cx| {
            for (host, client) in [(HostId::LOCAL, local_client), (remote, remote_client)] {
                mux.connections.get_mut(&host).unwrap().state = HostState::Connecting;
                mux.handle_connect_result(host, Ok(client), cx);
            }
            assert!(mux.attach_to_host(remote, SessionId(5), cx));
        });
    });

    assert_eq!(
        heard_until(&local_hearing, &ProtocolMessage::Detach),
        [
            ProtocolMessage::Resync,
            ProtocolMessage::SetConfigOverrides { entries: overrides },
            ProtocolMessage::SetColorScheme(TerminalColorScheme::Dark),
            ProtocolMessage::Detach,
        ]
    );
    let attach = ProtocolMessage::Attach {
        session: SessionId(5).to_string(),
    };
    assert_eq!(
        heard_until(&remote_hearing, &attach),
        [
            ProtocolMessage::Resync,
            ProtocolMessage::SetColorScheme(TerminalColorScheme::Dark),
            attach.clone(),
        ]
    );
}
