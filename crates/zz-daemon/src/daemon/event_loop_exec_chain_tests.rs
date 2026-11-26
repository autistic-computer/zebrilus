use super::*;
use zz_protocol::{ExecFlags, ExecRequest, encode_protocol_message};

fn pair(event_loop: &mut EventLoop) -> (Token, UnixStream) {
    let (peer, server) = UnixStream::pair().unwrap();
    peer.set_nonblocking(true).unwrap();
    let token = event_loop.insert(server.receive_fd().unwrap()).unwrap();
    (token, peer)
}

fn request(commands: Vec<CommandInvocation>, last: bool) -> ProtocolMessage {
    let mut flags = ExecFlags::default();
    flags.set(ExecFlags::LAST, last);
    ProtocolMessage::Exec(ExecRequest {
        protocol_version: PROTOCOL_VERSION,
        flags,
        client_instance_id: ClientInstanceId(202),
        origin: None,
        working_directory: None,
        tty: None,
        size: None,
        features: 0,
        startup_reentry: None,
        spawned_server_id: None,
        expect_server_id: None,
        process_id: std::process::id(),
        environment: ClientEnvironmentBlob::default(),
        commands,
        raw_control_line: None,
    })
}

fn display(text: &str) -> CommandInvocation {
    CommandInvocation::new("display-message", ["-p", text])
}

fn outputs(messages: &[ProtocolMessage]) -> Vec<(u64, String)> {
    messages
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::CommandResponse(CommandResponse::Success {
                request_id,
                output,
                ..
            }) => Some((*request_id, output.to_string())),
            _ => None,
        })
        .collect()
}

fn turn_until(
    event_loop: &mut EventLoop,
    shared: &Arc<Shared>,
    mut done: impl FnMut(&EventLoop) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(event_loop) {
        assert!(Instant::now() < deadline, "command queue progress deadline");
        event_loop.turn(shared).unwrap();
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn twenty_idle_exec_connections_and_a_query_chain_use_no_execution_workers() {
    let shared = Arc::new(Shared::new(202));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let mut peers = Vec::new();
    shared
        .connection_threads
        .fail_next
        .store(true, Ordering::Release);
    for index in 0..20 {
        let (token, mut peer) = pair(&mut event_loop);
        peer.write_all(
            &encode_protocol_message(&request(vec![display(&index.to_string())], false)).unwrap(),
        )
        .unwrap();
        event_loop.read_ready(token, &shared);
        peers.push((token, peer));
    }
    event_loop.turn(&shared).unwrap();
    for (_, peer) in &mut peers {
        let received = io_tests::messages(peer, &mut Inbound::default());
        assert_eq!(outputs(&received).len(), 1);
        assert!(matches!(
            received.last(),
            Some(ProtocolMessage::ExecExit(_))
        ));
    }
    assert_eq!(shared.inner.lock().clients.len(), 20);
    assert_eq!(shared.connection_threads.worker_count(), 0);
    let (token, peer) = &mut peers[0];
    peer.write_all(
        &encode_protocol_message(&request(
            (0..200).map(|index| display(&index.to_string())).collect(),
            false,
        ))
        .unwrap(),
    )
    .unwrap();
    event_loop.read_ready(*token, &shared);
    let mut input = Inbound::default();
    let mut received = Vec::new();
    turn_until(&mut event_loop, &shared, |_| {
        received.extend(io_tests::messages(peer, &mut input));
        received
            .iter()
            .any(|message| matches!(message, ProtocolMessage::ExecExit(_)))
    });
    assert_eq!(
        outputs(&received),
        (0..200)
            .map(|index| (index + 1, index.to_string()))
            .collect::<Vec<_>>()
    );
    assert_eq!(shared.connection_threads.worker_count(), 0);
    assert!(
        shared
            .connection_threads
            .fail_next
            .swap(false, Ordering::AcqRel)
    );
    for (token, mut peer) in peers {
        peer.write_all(&encode_protocol_message(&request(Vec::new(), true)).unwrap())
            .unwrap();
        event_loop.read_ready(token, &shared);
    }
    event_loop.turn(&shared).unwrap();
    assert_eq!(shared.connection_threads.worker_count(), 0);
    assert!(event_loop.connections.is_empty());
}

#[test]
fn a_legacy_leaf_returns_to_the_loop_without_replaying_queries() {
    let shared = Arc::new(Shared::new(203));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let (token, mut peer) = pair(&mut event_loop);
    let chain = request(
        vec![
            display("before"),
            CommandInvocation::new("wait-for", ["e02-leaf"]),
            display("after"),
        ],
        true,
    );
    peer.write_all(&encode_protocol_message(&chain).unwrap())
        .unwrap();
    event_loop.read_ready(token, &shared);
    assert!(!event_loop.connections[&token].busy);
    assert_eq!(shared.connection_threads.worker_count(), 0);
    let item = event_loop.connections[&token].command.as_ref().unwrap().id;
    assert!(matches!(
        event_loop.connections[&token]
            .command
            .as_ref()
            .unwrap()
            .state(),
        cmdq::State::Waiting(_)
    ));
    turn_until(&mut event_loop, &shared, |_| {
        shared
            .inner
            .lock()
            .waits
            .wait_channels
            .get("e02-leaf")
            .is_some_and(|channel| !channel.waiters.is_empty())
    });
    event_loop.turn(&shared).unwrap();
    let mut input = Inbound::default();
    let mut received = io_tests::messages(&mut peer, &mut input);
    assert_eq!(outputs(&received), [(1, "before".to_owned())]);
    shared
        .connection_threads
        .fail_next
        .store(true, Ordering::Release);
    shared
        .execute(
            ClientId(900),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("wait-for", ["-S", "e02-leaf"]),
        )
        .unwrap();
    assert_eq!(
        event_loop.connections[&token].command.as_ref().unwrap().id,
        item
    );
    turn_until(&mut event_loop, &shared, |_| {
        received.extend(io_tests::messages(&mut peer, &mut input));
        received
            .iter()
            .any(|message| matches!(message, ProtocolMessage::ExecExit(_)))
    });
    assert_eq!(
        outputs(&received),
        [
            (1, "before".to_owned()),
            (2, String::new()),
            (3, "after".to_owned())
        ]
    );
    assert!(
        shared
            .connection_threads
            .fail_next
            .swap(false, Ordering::AcqRel)
    );
}

fn control(event_loop: &mut EventLoop, shared: &Arc<Shared>) -> (Token, UnixStream) {
    let (token, mut peer) = pair(event_loop);
    let ProtocolMessage::ClientHello(mut hello) = io_tests::hello() else {
        unreachable!()
    };
    hello.kind = ClientKind::Control;
    let connection = event_loop.connections.get_mut(&token).unwrap();
    let session = connection::Session::register(
        shared,
        ProtocolMessage::ClientHello(hello),
        &connection.outbound,
        &connection.cancel,
    )
    .unwrap()
    .unwrap();
    connection.client = Some(session.client);
    connection.kind = Some(ClientKind::Control);
    connection.released = Some(Arc::clone(&session.released));
    connection.session = Some(Box::new(session));
    connection.initialized = true;
    event_loop.turn(shared).unwrap();
    io_tests::messages(&mut peer, &mut Inbound::default());
    (token, peer)
}

fn flatten(messages: Vec<ProtocolMessage>) -> Vec<ProtocolMessage> {
    messages
        .into_iter()
        .flat_map(|message| match message {
            ProtocolMessage::Batch(batch) => flatten(batch.messages().unwrap()),
            message => vec![message],
        })
        .collect()
}

#[test]
fn control_output_pressure_parks_and_preserves_command_and_exit_order() {
    let shared = Arc::new(Shared::new(204));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let (token, mut peer) = control(&mut event_loop, &shared);
    let wide = "w".repeat(4000);
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "@e02-wide", wide.as_str()]),
        )
        .unwrap();
    peer.write_all(
        &encode_protocol_message(&request(
            (0..100).map(|_| display("#{@e02-wide}")).collect(),
            false,
        ))
        .unwrap(),
    )
    .unwrap();
    shared
        .connection_threads
        .fail_next
        .store(true, Ordering::Release);
    event_loop.read_ready(token, &shared);
    assert!(event_loop.connections[&token].output_wait);
    assert!(matches!(
        event_loop.connections[&token]
            .command
            .as_ref()
            .unwrap()
            .state(),
        cmdq::State::Waiting(_)
    ));
    assert_eq!(shared.connection_threads.worker_count(), 0);
    let mut input = Inbound::default();
    let mut received = Vec::new();
    turn_until(&mut event_loop, &shared, |_| {
        received.extend(flatten(io_tests::messages(&mut peer, &mut input)));
        received
            .iter()
            .any(|message| matches!(message, ProtocolMessage::ExecExit(_)))
    });
    assert_eq!(
        outputs(&received),
        (1..=100)
            .map(|index| (index, wide.clone()))
            .collect::<Vec<_>>()
    );
    let started = received
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlCommandStarted { request_id, .. },
                ..
            }) => Some(*request_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(started, (1..=100).collect::<Vec<_>>());
    assert!(matches!(
        received.last(),
        Some(ProtocolMessage::ExecExit(_))
    ));
    assert!(
        shared
            .connection_threads
            .fail_next
            .swap(false, Ordering::AcqRel)
    );
    assert_eq!(shared.connection_threads.worker_count(), 0);
    event_loop.remove(token, &shared);
}

#[test]
fn hello_queries_park_before_the_legacy_resync_and_keep_welcome_first() {
    let shared = Arc::new(Shared::new(205));
    let wide = "h".repeat(4000);
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "@e02-hello", wide.as_str()]),
        )
        .unwrap();
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let (token, mut peer) = pair(&mut event_loop);
    let ProtocolMessage::ClientHello(mut client) = io_tests::hello() else {
        unreachable!()
    };
    client.kind = ClientKind::Control;
    let mut hello = zz_protocol::Hello::from_client(client);
    hello.attach = Some(zz_protocol::AttachOperation::Commands(
        shared.prepare_command_list((0..100).map(|_| display("#{@e02-hello}")).collect()),
    ));
    let connection = event_loop.connections.get_mut(&token).unwrap();
    let session = connection::Session::register(
        &shared,
        ProtocolMessage::Hello(hello),
        &connection.outbound,
        &connection.cancel,
    )
    .unwrap()
    .unwrap();
    connection.client = Some(session.client);
    connection.kind = Some(ClientKind::Control);
    connection.released = Some(Arc::clone(&session.released));
    connection.session = Some(Box::new(session));
    connection.start_command();
    event_loop.turn(&shared).unwrap();
    assert_eq!(shared.connection_threads.worker_count(), 0);
    assert!(
        event_loop.connections[&token]
            .session
            .as_ref()
            .unwrap()
            .message_pending()
    );
    let mut input = Inbound::default();
    let mut received = Vec::new();
    turn_until(&mut event_loop, &shared, |event_loop| {
        received.extend(flatten(io_tests::messages(&mut peer, &mut input)));
        let connection = &event_loop.connections[&token];
        connection.initialized && !connection.initializing && connection.command.is_none()
    });
    received.extend(flatten(io_tests::messages(&mut peer, &mut input)));
    assert!(matches!(
        received.first(),
        Some(ProtocolMessage::Welcome(_))
    ));
    assert_eq!(
        outputs(&received),
        (1..=100)
            .map(|index| (index, wide.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        !received
            .iter()
            .any(|message| matches!(message, ProtocolMessage::ExecExit(_)))
    );
    event_loop.remove(token, &shared);
}

#[test]
fn gui_replies_bypass_a_waiting_control_queue() {
    let shared = Arc::new(Shared::new(206));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let (token, mut peer) = control(&mut event_loop, &shared);
    peer.write_all(
        &encode_protocol_message(&request(
            vec![
                CommandInvocation::new("wait-for", ["e02-gui"]),
                display("after"),
            ],
            false,
        ))
        .unwrap(),
    )
    .unwrap();
    event_loop.read_ready(token, &shared);
    turn_until(&mut event_loop, &shared, |_| {
        shared
            .inner
            .lock()
            .waits
            .wait_channels
            .get("e02-gui")
            .is_some_and(|channel| !channel.waiters.is_empty())
    });
    let client = event_loop.connections[&token].client.unwrap();
    let (reply, replied) = crossbeam_channel::bounded(1);
    shared.inner.lock().transfers.pending_gui_requests.insert(
        202,
        PendingGuiRequest {
            client,
            reply: reply.into(),
            screenshot: None,
        },
    );
    let encoded = [
        request(vec![display("queued")], false),
        ProtocolMessage::GuiResponse(GuiResponse::Success {
            request_id: 202,
            output: "gui".to_owned(),
        }),
    ]
    .iter()
    .flat_map(|message| encode_protocol_message(message).unwrap())
    .collect::<Vec<_>>();
    peer.write_all(&encoded).unwrap();
    event_loop.read_ready(token, &shared);
    assert_eq!(replied.try_recv().unwrap(), Ok("gui".to_owned()));
    assert_eq!(event_loop.connections[&token].pending.len(), 1);
    assert!(matches!(
        event_loop.connections[&token]
            .command
            .as_ref()
            .unwrap()
            .state(),
        cmdq::State::Waiting(_)
    ));
    shared
        .execute(
            ClientId(900),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("wait-for", ["-S", "e02-gui"]),
        )
        .unwrap();
    let mut input = Inbound::default();
    let mut received = Vec::new();
    turn_until(&mut event_loop, &shared, |_| {
        received.extend(flatten(io_tests::messages(&mut peer, &mut input)));
        received
            .iter()
            .filter(|message| matches!(message, ProtocolMessage::ExecExit(_)))
            .count()
            == 2
    });
    assert_eq!(
        outputs(&received),
        [
            (1, String::new()),
            (2, "after".to_owned()),
            (1, "queued".to_owned())
        ]
    );
    event_loop.remove(token, &shared);
}

#[test]
fn shutdown_finishes_an_admitted_output_continuation_with_its_exit() {
    let shared = Arc::new(Shared::new(207));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let (token, mut peer) = pair(&mut event_loop);
    peer.write_all(
        &encode_protocol_message(&request(
            (0..100).map(|index| display(&index.to_string())).collect(),
            true,
        ))
        .unwrap(),
    )
    .unwrap();
    event_loop.read_ready(token, &shared);
    assert!(event_loop.connections[&token].output_wait);
    assert_eq!(shared.response_admissions.lock().active, 1);
    shared.shutdown_pending.store(true, Ordering::Release);
    event_loop.turn(&shared).unwrap();
    let received = io_tests::messages(&mut peer, &mut Inbound::default());
    assert_eq!(
        outputs(&received),
        (0..64)
            .map(|index| (index + 1, index.to_string()))
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        received.get(64),
        Some(ProtocolMessage::CommandResponse(CommandResponse::Error {
            request_id: 65,
            ..
        }))
    ));
    assert!(matches!(
        received.last(),
        Some(ProtocolMessage::ExecExit(_))
    ));
    assert_eq!(shared.response_admissions.lock().active, 0);
    assert_eq!(shared.connection_threads.worker_count(), 0);
}

#[test]
fn a_final_legacy_shutdown_command_admits_its_exit_before_completion() {
    let shared = Arc::new(Shared::new(208));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let (token, mut peer) = pair(&mut event_loop);
    peer.write_all(
        &encode_protocol_message(&request(
            vec![
                display("before"),
                CommandInvocation::new("kill-server", [] as [&str; 0]),
            ],
            true,
        ))
        .unwrap(),
    )
    .unwrap();
    event_loop.read_ready(token, &shared);
    let mut input = Inbound::default();
    let mut received = Vec::new();
    turn_until(&mut event_loop, &shared, |_| {
        received.extend(io_tests::messages(&mut peer, &mut input));
        received
            .iter()
            .any(|message| matches!(message, ProtocolMessage::ExecExit(_)))
    });
    assert_eq!(
        outputs(&received),
        [(1, "before".to_owned()), (2, String::new())]
    );
    assert!(matches!(
        received.last(),
        Some(ProtocolMessage::ExecExit(_))
    ));
    assert_eq!(shared.response_admissions.lock().active, 0);
}

#[test]
fn client_updates_keep_their_worker_path_under_output_pressure() {
    let shared = Arc::new(Shared::new(209));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let (token, mut peer) = pair(&mut event_loop);
    let ProtocolMessage::ClientHello(mut hello) = io_tests::hello() else {
        unreachable!()
    };
    hello.kind = ClientKind::Interactive;
    let connection = event_loop.connections.get_mut(&token).unwrap();
    let session = connection::Session::register(
        &shared,
        ProtocolMessage::ClientHello(hello),
        &connection.outbound,
        &connection.cancel,
    )
    .unwrap()
    .unwrap();
    let client = session.client;
    connection.client = Some(client);
    connection.kind = Some(ClientKind::Interactive);
    connection.released = Some(Arc::clone(&session.released));
    connection.session = Some(Box::new(session));
    connection.initialized = true;
    assert!(
        connection
            .outbound
            .enqueue_reliable(&ProtocolMessage::CommandResponse(
                CommandResponse::Success {
                    request_id: 0,
                    output: RawText::from("x".repeat(128 * 1024)),
                    exit_code: 0,
                    stderr: String::new(),
                    stdout_claim: StdoutClaim::default(),
                }
            ))
    );
    assert!(exec::output_pending(&connection.outbound));
    peer.write_all(
        &encode_protocol_message(&ProtocolMessage::SetColorScheme(TerminalColorScheme::Light))
            .unwrap(),
    )
    .unwrap();
    event_loop.read_ready(token, &shared);
    assert!(event_loop.connections[&token].busy);
    assert!(!event_loop.connections[&token].output_wait);
    turn_until(&mut event_loop, &shared, |_| {
        shared.inner.lock().client(client).unwrap().color_scheme == Some(TerminalColorScheme::Light)
    });
    event_loop.remove(token, &shared);
}

#[test]
fn formatted_split_wait_resumes_its_pane_wait_on_the_loop() {
    let shared = Arc::new(Shared::new(408));
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(ClientKind::Command, None, None, mailbox);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "e04wait", "exec sleep 30"]),
        )
        .unwrap();
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let mut task = wait_queue::CommandTask::new(
        &shared,
        client,
        ClientKind::Command,
        &context,
        408,
        &CommandInvocation::new(
            "split-window",
            [
                "-d",
                "-P",
                "-F",
                "#{pane_index}",
                "-W",
                "-t",
                "e04wait:",
                "sleep 0.2; exit 5",
            ],
        ),
        false,
    )
    .unwrap_or_else(|_| panic!("split wait task"));
    assert!(matches!(task.run(true), wait_queue::Progress::Waiting));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !task.ready() {
        assert!(Instant::now() < deadline);
        event_loop.turn(&shared).unwrap();
        let mut query = context.clone();
        let output = shared
            .execute(
                client,
                ClientKind::Command,
                &mut query,
                &CommandInvocation::new("display-message", ["-p", "responsive"]),
            )
            .unwrap()
            .output;
        assert_eq!(output, "responsive");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(matches!(task.run(true), wait_queue::Progress::Done));
    let (response, _, _, _) = task.finish();
    assert!(
        matches!(response, CommandResponse::Success { output, exit_code: 5, .. } if output == "1\n")
    );
    shared.request_shutdown();
}

#[test]
fn formatted_hook_split_wait_keeps_the_loop_running() {
    let shared = Arc::new(Shared::new(409));
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(ClientKind::Command, None, None, mailbox);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "e04hook", "sleep 30"]),
        )
        .unwrap();
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let mut hooks = hook_queue::LoopHooks::new();
    hooks.monitor(
        &shared,
        context.clone(),
        vec![vec![
            CommandInvocation::new(
                "split-window",
                ["-d", "-P", "-W", "-t", "e04hook:", "sleep 0.2"],
            ),
            CommandInvocation::new("set-option", ["-g", "@e04after", "completed"]),
        ]],
        BTreeMap::new(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !hooks.is_empty() {
        assert!(Instant::now() < deadline);
        hooks.turn(&shared, &event_loop.waker).unwrap();
        event_loop.turn(&shared).unwrap();
        thread::sleep(Duration::from_millis(1));
    }
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-options", ["-gqv", "@e04after"]),
        )
        .unwrap()
        .output;
    assert_eq!(output, "completed");
    shared.request_shutdown();
}

fn until(event_loop: &mut EventLoop, shared: &Arc<Shared>, done: impl Fn(&EventLoop) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(event_loop) {
        assert!(Instant::now() < deadline);
        event_loop.poll_test_turn(shared, Duration::from_millis(1));
    }
}

#[test]
fn shutdown_runs_close_hooks_before_freezing_admissions() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("closed");
    let shared = Arc::new(Shared::new(721));
    let mut context = ExecutionContext::default();
    for command in [
        CommandInvocation::new("new-session", ["-d", "-s", "e21", "sleep 1000000"]),
        CommandInvocation::new(
            "set-hook",
            [
                "-g",
                "session-closed",
                &format!("run-shell 'sleep 0.05; touch {}'", marker.display()),
            ],
        ),
    ] {
        shared
            .execute(ClientId(900), ClientKind::Command, &mut context, &command)
            .unwrap();
    }
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    event_loop.signals = Some(SignalPipes::new(&event_loop.poll).unwrap());
    shared.request_shutdown();
    event_loop.turn(&shared).unwrap();
    assert!(!shared.response_admissions.lock().frozen);
    until(&mut event_loop, &shared, EventLoop::shutdown_completed);
    assert!(marker.exists());
    assert!(shared.inner.lock().engine.state.sessions.is_empty());
    assert_eq!(shared.connection_threads.worker_count(), 0);
}

#[test]
fn removal_releases_the_writer_before_the_owner_cleans_client_state() {
    let shared = Arc::new(Shared::new(722));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let (token, _peer, client, outbound) =
        shutdown_tests::registered(&mut event_loop, &shared, ClientKind::Command);
    {
        let inner = shared.inner.lock();
        event_loop.remove(token, &shared);
        assert!(outbound.state.lock().writer_finished);
        assert!(inner.clients.contains_key(&client));
    }
    event_loop.turn(&shared).unwrap();
    assert!(!shared.inner.lock().clients.contains_key(&client));
    assert_eq!(shared.connection_threads.worker_count(), 0);
}
