use super::*;

#[cfg(unix)]
#[test]
fn prepare_rpc_echoes_request_ids_for_every_client_kind() {
    let shared = Arc::new(Shared::new(1));
    for (index, kind) in [
        ClientKind::Interactive,
        ClientKind::Command,
        ClientKind::Control,
    ]
    .into_iter()
    .enumerate()
    {
        let (mut client, server) = std::os::unix::net::UnixStream::pair().unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let server_shared = Arc::clone(&shared);
        let connection = thread::spawn(move || handle_connection(server, &server_shared));
        zz_protocol::write_protocol_message(
            &mut client,
            &ProtocolMessage::ClientHello(ClientHello {
                protocol_version: PROTOCOL_VERSION,
                client_instance_id: ClientInstanceId(index as u64 + 1),
                kind,
                device_name: None,
                capabilities: Vec::new(),
                color_scheme: None,
                origin: None,
                environment: Vec::new(),
                working_directory: None,
                process_id: u32::try_from(index).expect("client index") + 100,
                revision: zz_protocol::REVISION,
            }),
        )
        .unwrap();
        let ProtocolMessage::ServerHello(hello) =
            zz_protocol::read_protocol_message(&mut client).unwrap()
        else {
            panic!("missing server hello");
        };
        assert_eq!(
            shared
                .inner
                .lock()
                .client(hello.client_id)
                .and_then(|c| c.pid.as_ref()),
            Some(&(u32::try_from(index).expect("client index") + 100))
        );
        let request_id = index as u64 + 41;
        zz_protocol::write_protocol_message(
            &mut client,
            &ProtocolMessage::PrepareCommandList {
                request_id,
                commands: vec![CommandInvocation::new("list-sess", [] as [&str; 0])],
            },
        )
        .unwrap();
        assert!(matches!(
            zz_protocol::read_protocol_message(&mut client).unwrap(),
            ProtocolMessage::PreparedCommandList {
                request_id: response_id,
                commands,
            } if response_id == request_id
                && commands.len() == 1
                && commands[0].canonical_name.as_deref() == Some("list-sessions")
                && commands[0].result == PreparedCommandResult::Ready
        ));
        drop(client);
        connection.join().unwrap().unwrap();
    }
}

#[cfg(unix)]
#[test]
fn kill_server_waits_for_response_admission_before_stopping_client_writers() {
    for (index, kind) in [ClientKind::Control, ClientKind::Command]
        .into_iter()
        .enumerate()
    {
        let shared = Arc::new(Shared::new(index as u64 + 1));
        let (reached_tx, reached_rx) = crossbeam_channel::bounded(1);
        let (release_tx, release_rx) = crossbeam_channel::bounded(1);
        *shared.response_admission_hook.lock() = Some(ResponseAdmissionHook {
            reached: reached_tx,
            release: release_rx,
        });

        let (mut client, server) = std::os::unix::net::UnixStream::pair().expect("pair");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout");
        let server_shared = Arc::clone(&shared);
        let connection = thread::spawn(move || handle_connection(server, &server_shared));
        zz_protocol::write_protocol_message(
            &mut client,
            &ProtocolMessage::ClientHello(ClientHello {
                protocol_version: PROTOCOL_VERSION,
                client_instance_id: ClientInstanceId(index as u64 + 1),
                kind,
                device_name: None,
                capabilities: Vec::new(),
                color_scheme: None,
                origin: None,
                environment: Vec::new(),
                working_directory: None,
                process_id: 0,
                revision: zz_protocol::REVISION,
            }),
        )
        .expect("hello");
        assert!(matches!(
            zz_protocol::read_protocol_message(&mut client).expect("server hello"),
            ProtocolMessage::ServerHello(_)
        ));

        zz_protocol::write_protocol_message(
            &mut client,
            &ProtocolMessage::CommandRequest(CommandRequest {
                request_id: 41,
                command: CommandInvocation::new("kill-server", [] as [&str; 0]),
                prepared: false,
            }),
        )
        .expect("kill-server request");
        reached_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("response reached admission hook");
        assert!(shared.shutdown_pending.load(Ordering::Acquire));
        assert_eq!(shared.response_admissions.lock().active, 1);
        assert!(!shared.freeze_response_admissions_and_wait(Duration::ZERO));

        let shutdown_shared = Arc::clone(&shared);
        let shutdown = thread::spawn(move || {
            assert!(shutdown_shared.freeze_response_admissions_and_wait(Duration::from_secs(2)));
            shutdown_shared.announce_shutdown();
            assert!(shutdown_shared.drain_client_writers_for_shutdown(Duration::from_secs(2)));
        });
        release_tx.send(()).expect("release response admission");

        let mut saw_clean_control_exit = false;
        loop {
            let response =
                zz_protocol::read_protocol_message(&mut client).expect("kill-server response");
            assert!(!matches!(
                &response,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ServerStopping,
                    ..
                })
            ));
            if matches!(
                &response,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlExit { reason },
                    ..
                }) if reason.is_empty()
            ) {
                assert_eq!(kind, ClientKind::Control);
                assert!(!saw_clean_control_exit);
                saw_clean_control_exit = true;
                continue;
            }
            if matches!(
                                &response,
                                ProtocolMessage::CommandResponse(CommandResponse::Success {
                                    request_id: 41,
                                    output,
                                    exit_code: 0,
                                    stderr,
                                                        ..
            }) if output.is_empty() && stderr.is_empty()
                            )
            {
                break;
            }
        }
        assert_eq!(saw_clean_control_exit, kind == ClientKind::Control);
        if kind == ClientKind::Control {
            loop {
                if matches!(
                    zz_protocol::read_protocol_message(&mut client).expect("server stopping"),
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::ServerStopping,
                        ..
                    })
                ) {
                    break;
                }
            }
        }
        let mut byte = [0_u8; 1];
        assert_eq!(client.read(&mut byte).expect("connection close"), 0);

        shutdown.join().expect("shutdown phase");
        connection
            .join()
            .expect("connection thread")
            .expect("connection result");
        assert!(shared.client_writers.lock().is_empty());
    }
}

#[test]
fn stalled_writer_and_failed_response_admission_remain_bounded() {
    let shared = Arc::new(Shared::new(1));
    let stalled = OutboundMailbox::new();
    let mut registration =
        ClientWriterRegistrationGuard::new(&shared, ClientId(7), Arc::clone(&stalled));
    assert!(!shared.drain_client_writers_for_shutdown(Duration::ZERO));
    assert!(!stalled.is_open());
    registration.unregister();

    let disconnected = OutboundMailbox::new();
    disconnected.close();
    let admitted = shared.execute_command_request_with_prepared_into(
        &disconnected,
        ClientId(8),
        ClientKind::Command,
        &mut ExecutionContext::default(),
        9,
        &CommandInvocation::new("kill-server", [] as [&str; 0]),
        false,
    );
    assert!(!admitted);
    assert!(shared.freeze_response_admissions_and_wait(Duration::ZERO));
}

#[test]
fn frozen_response_admission_rejects_late_control_and_command_execution() {
    for (index, kind) in [ClientKind::Control, ClientKind::Command]
        .into_iter()
        .enumerate()
    {
        let shared = Arc::new(Shared::new(index as u64 + 1));
        assert!(shared.freeze_response_admissions_and_wait(Duration::ZERO));
        let outbound = OutboundMailbox::new();
        assert!(!shared.execute_command_request_with_prepared_into(
            &outbound,
            ClientId(index as u64 + 1),
            kind,
            &mut ExecutionContext::default(),
            41,
            &CommandInvocation::new("kill-server", [] as [&str; 0]),
            false,
        ));
        assert!(!shared.stopping.load(Ordering::Acquire));
        let refusals = take_reliable_messages(&outbound);
        if kind == ClientKind::Command {
            assert_eq!(refusals, [server_stopping_response(41)]);
        } else {
            assert!(refusals.is_empty());
        }
        let state = shared.response_admissions.lock();
        assert!(state.frozen);
        assert_eq!(state.active, 0);
    }
}

#[cfg(unix)]
#[test]
fn replacement_bind_waits_for_listener_held_cleanup() {
    let directory = tempfile::tempdir().expect("temporary socket directory");
    let path = directory.path().join("daemon.sock");
    let listener = LocalTransport::bind(&path).expect("old listener");
    let mut socket_guard = SocketGuard::new(path.clone());
    let shared = Arc::new(Shared::new(1));
    let stalled = OutboundMailbox::new();
    let mut registration =
        ClientWriterRegistrationGuard::new(&shared, ClientId(7), Arc::clone(&stalled));

    let shutdown_shared = Arc::clone(&shared);
    let shutdown = thread::spawn(move || {
        assert!(shutdown_shared.freeze_response_admissions_and_wait(Duration::ZERO));
        shutdown_shared.publish(EventPayload::ServerStopping);
        assert!(shutdown_shared.drain_client_writers_for_shutdown(Duration::from_secs(2)));
        socket_guard.release();
        drop(socket_guard);
        drop(listener);
    });

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut state = stalled.state.lock();
    while !state.closed {
        assert!(
            !stalled.ready.wait_until(&mut state, deadline).timed_out(),
            "writer drain did not start"
        );
    }
    drop(state);
    let Err(replacement_error) = LocalTransport::bind(&path) else {
        panic!("replacement bound early");
    };
    assert_eq!(replacement_error.kind(), ErrorKind::AddrInUse);
    stalled.mark_writer_finished();
    shutdown.join().expect("old shutdown");

    let replacement = LocalTransport::bind(&path).expect("replacement listener");
    let client = LocalTransport::connect(&path).expect("connect replacement");
    let server = replacement.accept().expect("accept replacement client");
    drop(client);
    drop(server);
    registration.unregister();
}

#[cfg(unix)]
#[test]
fn envelope_mismatch_gets_a_legible_reply_before_disconnect() {
    let shared = Arc::new(Shared::new(1));
    let (mut client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    let server_shared = Arc::clone(&shared);
    let connection = thread::spawn(move || handle_connection(server, &server_shared));
    let stale = PROTOCOL_VERSION.saturating_sub(1);
    let mut hello = encode_protocol_message(&ProtocolMessage::ClientHello(ClientHello {
        protocol_version: stale,
        client_instance_id: ClientInstanceId(1),
        kind: ClientKind::Command,
        device_name: None,
        capabilities: Vec::new(),
        color_scheme: None,
        origin: None,
        environment: Vec::new(),
        working_directory: None,
        process_id: 0,
        revision: zz_protocol::REVISION,
    }))
    .unwrap();
    hello[6..8].copy_from_slice(&stale.to_le_bytes());
    client.write_all(&hello).unwrap();
    assert_eq!(
        zz_protocol::read_protocol_message(&mut client).unwrap(),
        ProtocolMessage::CommandResponse(CommandResponse::Error {
            request_id: 0,
            error: ServerError::ProtocolMismatch {
                client: stale,
                server: PROTOCOL_VERSION,
            },
            output: RawText::default(),
        })
    );
    assert!(matches!(
        connection.join().unwrap(),
        Err(DaemonError::Protocol(ProtocolError::VersionMismatch {
            received,
            ..
        })) if received == stale
    ));
}

#[cfg(unix)]
#[test]
fn client_hello_mismatch_gets_a_legible_reply_before_disconnect() {
    let shared = Arc::new(Shared::new(1));
    let (mut client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    let server_shared = Arc::clone(&shared);
    let connection = thread::spawn(move || handle_connection(server, &server_shared));
    let stale = PROTOCOL_VERSION.saturating_sub(1);
    zz_protocol::write_protocol_message(
        &mut client,
        &ProtocolMessage::ClientHello(ClientHello {
            protocol_version: stale,
            client_instance_id: ClientInstanceId(1),
            kind: ClientKind::Command,
            device_name: None,
            capabilities: Vec::new(),
            color_scheme: None,
            origin: None,
            environment: Vec::new(),
            working_directory: None,
            process_id: 0,
            revision: zz_protocol::REVISION,
        }),
    )
    .unwrap();
    assert_eq!(
        zz_protocol::read_protocol_message(&mut client).unwrap(),
        ProtocolMessage::CommandResponse(CommandResponse::Error {
            request_id: 0,
            error: ServerError::ProtocolMismatch {
                client: stale,
                server: PROTOCOL_VERSION,
            },
            output: RawText::default(),
        })
    );
    assert!(matches!(
        connection.join().unwrap(),
        Err(DaemonError::Server(ServerError::ProtocolMismatch {
            client,
            server: PROTOCOL_VERSION,
        })) if client == stale
    ));
}

#[cfg(unix)]
#[test]
fn hello_below_the_client_revision_floor_is_refused_with_both_revisions() {
    let shared = Arc::new(Shared::new(1));
    let (mut client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    let server_shared = Arc::clone(&shared);
    let connection = thread::spawn(move || handle_connection(server, &server_shared));
    let stale = zz_protocol::MIN_CLIENT_REVISION.saturating_sub(1);
    zz_protocol::write_protocol_message(
        &mut client,
        &ProtocolMessage::Hello(zz_protocol::Hello::from_client(ClientHello {
            protocol_version: PROTOCOL_VERSION,
            client_instance_id: ClientInstanceId(1),
            kind: ClientKind::Interactive,
            device_name: None,
            capabilities: Vec::new(),
            color_scheme: None,
            origin: None,
            environment: Vec::new(),
            working_directory: None,
            process_id: 0,
            revision: stale,
        })),
    )
    .unwrap();
    let refusal = ServerError::ClientTooOld {
        client: stale,
        minimum: zz_protocol::MIN_CLIENT_REVISION,
    };
    assert_eq!(
        zz_protocol::read_protocol_message(&mut client).unwrap(),
        ProtocolMessage::CommandResponse(CommandResponse::Error {
            request_id: 0,
            error: refusal.clone(),
            output: RawText::default(),
        })
    );
    assert!(matches!(
        connection.join().unwrap(),
        Err(DaemonError::Server(error)) if error == refusal
    ));
}

#[cfg(unix)]
#[test]
fn command_connection_waits_without_hiding_peer_disconnects() {
    fn connect(
        shared: &Arc<Shared>,
        instance: u64,
    ) -> (
        std::os::unix::net::UnixStream,
        thread::JoinHandle<Result<(), DaemonError>>,
        ClientId,
    ) {
        let (mut client, server) = std::os::unix::net::UnixStream::pair().expect("pair");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout");
        let server_shared = Arc::clone(shared);
        let connection = thread::spawn(move || handle_connection(server, &server_shared));
        zz_protocol::write_protocol_message(
            &mut client,
            &ProtocolMessage::ClientHello(ClientHello {
                protocol_version: PROTOCOL_VERSION,
                client_instance_id: ClientInstanceId(instance),
                kind: ClientKind::Command,
                device_name: None,
                capabilities: Vec::new(),
                color_scheme: None,
                origin: None,
                environment: Vec::new(),
                working_directory: None,
                process_id: 0,
                revision: zz_protocol::REVISION,
            }),
        )
        .expect("hello");
        let ProtocolMessage::ServerHello(hello) =
            zz_protocol::read_protocol_message(&mut client).expect("server hello")
        else {
            panic!("missing server hello");
        };
        (client, connection, hello.client_id)
    }

    let shared = Arc::new(Shared::new(1));
    let (mut waiter, waiter_connection, _) = connect(&shared, 1);
    let (mut signaler, signaler_connection, _) = connect(&shared, 2);
    zz_protocol::write_protocol_message(
        &mut waiter,
        &ProtocolMessage::CommandRequest(CommandRequest {
            request_id: 1,
            command: CommandInvocation::new("wait-for", ["connection"]),
            prepared: false,
        }),
    )
    .expect("wait request");
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared
        .inner
        .lock()
        .wait_channels
        .get("connection")
        .map_or(0, |channel| channel.waiters.len())
        != 1
    {
        assert!(Instant::now() < deadline, "connection wait did not queue");
        thread::yield_now();
    }
    zz_protocol::write_protocol_message(
        &mut signaler,
        &ProtocolMessage::CommandRequest(CommandRequest {
            request_id: 2,
            command: CommandInvocation::new("wait-for", ["-S", "connection"]),
            prepared: false,
        }),
    )
    .expect("signal request");
    for (stream, request_id) in [(&mut signaler, 2), (&mut waiter, 1)] {
        assert!(matches!(
            zz_protocol::read_protocol_message(stream).expect("wait response"),
            ProtocolMessage::CommandResponse(CommandResponse::Success {
                request_id: response_id,
                ..
            }) if response_id == request_id
        ));
    }
    drop(waiter);
    drop(signaler);
    waiter_connection
        .join()
        .expect("waiter handler")
        .expect("waiter connection");
    signaler_connection
        .join()
        .expect("signaler handler")
        .expect("signaler connection");

    let (mut dropped, dropped_connection, dropped_client) = connect(&shared, 3);
    zz_protocol::write_protocol_message(
        &mut dropped,
        &ProtocolMessage::CommandRequest(CommandRequest {
            request_id: 3,
            command: CommandInvocation::new("wait-for", ["dropped"]),
            prepared: false,
        }),
    )
    .expect("dropped wait request");
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared
        .inner
        .lock()
        .wait_channels
        .get("dropped")
        .map_or(0, |channel| channel.waiters.len())
        != 1
    {
        assert!(Instant::now() < deadline, "dropped wait did not queue");
        thread::yield_now();
    }
    drop(dropped);
    dropped_connection
        .join()
        .expect("dropped handler")
        .expect("dropped connection");
    assert!(shared.read_client(dropped_client, |c| {
        c.is_none_or(|c| c.instance_id.is_none())
    }));
    assert!(
        shared
            .inner
            .lock()
            .wait_channels
            .get("dropped")
            .is_none_or(|channel| channel.waiters.is_empty())
    );
}

#[test]
fn client_registration_guard_cleans_up_automatic_and_explicit_exits() {
    let shared = Arc::new(Shared::new(1));

    let automatic = {
        let (client, _) = shared.register_subscribed(
            ClientKind::Interactive,
            Some("automatic".to_owned()),
            None,
            OutboundMailbox::new(),
        );
        let guard = ClientRegistrationGuard::new(&shared, client);
        assert!(shared.read_client(client, |c| c.is_some_and(|c| c.subscriber.is_some())));
        drop(guard);
        client
    };
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(automatic)
                .is_none_or(|c| c.subscriber.is_none())
        );
        assert!(!inner.clients.contains_key(&automatic));
    }

    let (explicit, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("explicit".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let mut guard = ClientRegistrationGuard::new(&shared, explicit);
    guard.unregister();
    drop(guard);
    let inner = shared.inner.lock();
    assert!(
        inner
            .client(explicit)
            .is_none_or(|c| c.subscriber.is_none())
    );
    assert!(!inner.clients.contains_key(&explicit));
}

#[test]
fn client_terminal_facts_are_parsed_and_stored() {
    assert_eq!(
        client_tty_fact(&["client-tty-v1:/dev/pts/3".to_owned()]),
        Some("/dev/pts/3".to_owned())
    );
    assert_eq!(client_tty_fact(&["client-tty-v1:".to_owned()]), None);
    assert_eq!(client_tty_fact(&["client-terminal-v1".to_owned()]), None);
    assert!(client_nested_fact(&["client-nested-v1".to_owned()]));
    assert!(!client_nested_fact(&["client-nested-v1:".to_owned()]));
    assert!(client_remote_fact(&["client-remote-v1".to_owned()]));
    assert!(!client_remote_fact(&["client-nested-v1".to_owned()]));
    assert_eq!(
        client_size_fact(&["client-size-v1:120x40".to_owned()]),
        Some((120, 40))
    );
    assert_eq!(client_size_fact(&["client-size-v1:0x40".to_owned()]), None);
    assert_eq!(client_size_fact(&["client-size-v1:120".to_owned()]), None);
    assert_eq!(client_size_fact(&["client-terminal-v1".to_owned()]), None);
    let features = |tokens: &[&str]| {
        terminal_features_list(client_features_fact(
            &tokens
                .iter()
                .map(|token| format!("client-features-v1:{token}"))
                .collect::<Vec<_>>(),
        ))
    };
    assert_eq!(features(&["256"]), "256");
    assert_eq!(features(&["sixel:rgb", "256"]), "256,RGB,sixel");
    assert_eq!(features(&["title,bogus,sixel"]), "title");
    assert_eq!(features(&["", "sixel"]), "sixel");
    assert_eq!(client_features_fact(&["client-terminal-v1".to_owned()]), 0);
    assert_eq!(
        terminal_features_list(u32::MAX),
        zz_daemon_client::TERMINAL_FEATURES.join(",")
    );
    let client_cwd = std::env::temp_dir().join("client cwd");
    assert_eq!(
        client_working_directory_fact(ClientPath::from_path(&client_cwd).as_ref()),
        Some(client_cwd)
    );
    assert_eq!(
        client_working_directory_fact(
            ClientPath::from_path(Path::new("relative/client-cwd")).as_ref()
        ),
        None
    );
    assert_eq!(client_working_directory_fact(None), None);
    #[cfg(unix)]
    {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt as _};

        let bytes = PathBuf::from(OsString::from_vec(
            [b"/tmp/client-".as_slice(), &[0xff], b"-cwd".as_slice()].concat(),
        ));
        assert_eq!(bytes.to_str(), None);
        assert_eq!(
            client_working_directory_fact(ClientPath::from_path(&bytes).as_ref()),
            Some(bytes)
        );
        let relative = PathBuf::from(OsString::from_vec(vec![0xff, b'/', b'x']));
        assert_eq!(
            client_working_directory_fact(ClientPath::from_path(&relative).as_ref()),
            None
        );
    }
    #[cfg(unix)]
    assert_eq!(
        resolve_source_path(
            &RawText::from("configs/*.conf"),
            Some(Path::new("/tmp/client cwd"))
        ),
        PathBuf::from("/tmp/client\\ cwd/configs/*.conf")
    );
    #[cfg(not(unix))]
    assert_eq!(
        resolve_source_path(
            &RawText::from("configs/*.conf"),
            Some(Path::new("/tmp/client cwd"))
        ),
        PathBuf::from("/tmp/client cwd/configs/*.conf")
    );
    let absolute = std::env::temp_dir().join("absolute.conf");
    assert_eq!(
        resolve_source_path(
            &RawText::from(absolute.to_str().expect("UTF-8 temp directory")),
            Some(Path::new("/ignored"))
        ),
        absolute
    );
    #[cfg(unix)]
    assert_eq!(
        resolve_source_path(
            &RawText::from("configs/*.conf"),
            Some(Path::new("/tmp/client [literal]*? cwd"))
        ),
        PathBuf::from("/tmp/client\\ \\[literal\\]\\*\\?\\ cwd/configs/*.conf")
    );
    #[cfg(not(unix))]
    assert_eq!(
        resolve_source_path(
            &RawText::from("configs/*.conf"),
            Some(Path::new("/tmp/client [literal]*? cwd"))
        ),
        PathBuf::from("/tmp/client [[]literal[]][*][?] cwd/configs/*.conf")
    );

    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            InputMessage::ClientTerminalSize {
                columns: 132,
                rows: 50,
            },
        )
        .expect("client size input");
    assert_eq!(
        shared
            .inner
            .lock()
            .client(client)
            .and_then(|c| c.size.as_ref()),
        Some(&(132, 50))
    );
}

#[test]
fn client_environment_entries_become_an_immutable_name_value_map() {
    let environment = client_environment_fact(&[
        "EMPTY=".into(),
        "EXACT=first".into(),
        "EXACT=last".into(),
        "VALUE=contains=equals".into(),
    ]);
    assert_eq!(
        environment.map().get("EMPTY").map(RawText::as_str),
        Some("")
    );
    assert_eq!(
        environment.map().get("EXACT").map(RawText::as_str),
        Some("last")
    );
    assert_eq!(
        environment.map().get("VALUE").map(RawText::as_str),
        Some("contains=equals")
    );
}
