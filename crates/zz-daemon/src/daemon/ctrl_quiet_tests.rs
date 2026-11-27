#[cfg(unix)]
use super::ctrl_test_support::compact_hello;
use super::ctrl_test_support::{compact_exec_request, compact_registered, reliable_children};
use super::*;

#[test]
fn quiet_control_query_sends_one_flat_completion_batch() {
    let shared = Arc::new(Shared::new(50));
    let (client, mailbox) = compact_registered(&shared, zz_protocol::Subscriptions::control());
    shared
        .inner
        .lock()
        .client_entry(client)
        .kind
        .replace(ClientKind::Control);
    shared.execute_compact_request(
        client,
        ClientKind::Control,
        &mut ExecutionContext::default(),
        compact_exec_request(vec![CommandInvocation::new(
            "display-message",
            ["-p", "quiet result"],
        )]),
        &mailbox,
    );
    let messages = tests::take_reliable_messages(&mailbox);
    let [ProtocolMessage::Batch(batch)] = messages.as_slice() else {
        panic!("expected one completion batch: {messages:?}")
    };
    assert!(matches!(
        batch.messages().expect("flat completion").as_slice(),
        [
            ProtocolMessage::Event(Event {
                payload: EventPayload::ControlCommandStarted { request_id: 1, .. },
                ..
            }),
            ProtocolMessage::CommandResponse(CommandResponse::Success { output, .. }),
            ProtocolMessage::ExecExit(zz_protocol::ExecExit {
                outcome: zz_protocol::ExecOutcome::Ran,
                ..
            }),
        ] if output.as_bytes() == b"quiet result"
    ));
}

#[test]
fn quiet_control_release_cannot_flush_an_attach_collector() {
    let shared = Arc::new(Shared::new(51));
    let (client, mailbox) = compact_registered(&shared, zz_protocol::Subscriptions::control());
    shared
        .client_writers
        .lock()
        .insert(client, Arc::clone(&mailbox));
    assert!(mailbox.collect_control_query());
    assert!(mailbox.enqueue_reliable_with_wakeup(&ProtocolMessage::TreeSync, false));
    assert!(mailbox.enqueue_control_group(vec![
            zz_protocol::encode_protocol_message(&ProtocolMessage::TreeSync)
                .expect("deferred child")
                .into(),
        ]));
    assert!(matches!(
        mailbox.state.lock().reliable.back(),
        Some(OutboundFrame::DeferredGrouped { .. })
    ));
    mailbox.hold_terminals();
    mailbox.release_terminals();
    {
        let mut state = mailbox.state.lock();
        assert!(state.ctrl_collecting == ControlCollection::Quiet);
        assert!(pop_ready_frame(&mut state).is_none());
    }
    mailbox.collect_control_attach();
    assert!(!mailbox.collect_control_query());
    for index in 0..MAX_RELIABLE_MESSAGES / 2 {
        let admitted = if index % 2 == 0 {
            mailbox.enqueue_reliable(&ProtocolMessage::TreeSync)
        } else {
            mailbox.enqueue_control_group(vec![
                zz_protocol::encode_protocol_message(&ProtocolMessage::TreeSync)
                    .expect("encode")
                    .into(),
            ])
        };
        assert!(admitted);
    }
    mailbox.hold_terminals();
    mailbox.release_terminals();
    shared.wake_control_queue(client, ClientKind::Control);
    mailbox.release_control_query();
    {
        let mut state = mailbox.state.lock();
        assert!(state.ctrl_collecting == ControlCollection::Attach);
        assert!(pop_ready_frame(&mut state).is_none());
    }
    assert!(mailbox.flush_control_batch(false));
    let messages = tests::take_reliable_messages(&mailbox);
    let [ProtocolMessage::Batch(batch)] = messages.as_slice() else {
        panic!("attach collection was split: {messages:?}")
    };
    let children = batch.messages().expect("flat attach collection");
    assert_eq!(children.len(), MAX_RELIABLE_MESSAGES / 2 + 2);
    assert!(
        children
            .iter()
            .all(|message| *message == ProtocolMessage::TreeSync)
    );
}

#[test]
fn quiet_control_close_and_overflow_release_the_held_frames() {
    let mailbox = OutboundMailbox::new();
    assert!(mailbox.collect_control_query());
    assert!(mailbox.enqueue_reliable_with_wakeup(&ProtocolMessage::TreeSync, false));
    assert!(mailbox.enqueue_control_group(vec![
            zz_protocol::encode_protocol_message(&ProtocolMessage::TreeSync)
                .expect("deferred child")
                .into(),
        ]));
    mailbox.close_after_flush();
    let encoded = mailbox.recv().expect("closed collection drains");
    let ProtocolMessage::Batch(batch) =
        zz_protocol::decode_protocol_frame(&encoded).expect("decode")
    else {
        panic!("expected final collection")
    };
    assert_eq!(
        batch.messages().expect("flat group"),
        [ProtocolMessage::TreeSync, ProtocolMessage::TreeSync]
    );
    assert!(mailbox.recv().is_none());

    let mailbox = OutboundMailbox::new();
    assert!(mailbox.collect_control_query());
    assert!(mailbox.enqueue_control_group(vec![
            zz_protocol::encode_protocol_message(&ProtocolMessage::TreeSync)
                .expect("overflow child")
                .into(),
        ]));
    for _ in 1..MAX_RELIABLE_MESSAGES {
        assert!(mailbox.enqueue_reliable_with_wakeup(&ProtocolMessage::TreeSync, false));
    }
    assert!(!mailbox.enqueue_reliable(&ProtocolMessage::TreeSync));
    let encoded = mailbox.recv().expect("overflow collection drains");
    assert!(matches!(
        zz_protocol::decode_protocol_frame(&encoded).expect("decode"),
        ProtocolMessage::Event(Event {
            payload: EventPayload::ControlExit { reason },
            ..
        }) if reason == "too far behind"
    ));
    assert!(mailbox.recv().is_none());
}

#[test]
fn legacy_control_guard_preserves_raw_output() {
    let shared = Arc::new(Shared::new(38));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared.publish_control_command_guard(
        Some((client, 1)),
        RawText::from_bytes(vec![b'a', 0xfe, b'b', b'\n']),
        false,
        false,
    );
    let messages = super::tests::take_reliable_messages(&mailbox);
    assert!(
        matches!(messages.as_slice(), [ProtocolMessage::Event(Event { payload: EventPayload::ControlCommandGuard { output, flags: 1, .. }, .. })] if output.as_bytes() == [b'a', 0xfe, b'b', b'\n']),
        "{messages:?}"
    );
}

#[test]
fn compact_callback_guard_preserves_raw_output() {
    let shared = Arc::new(Shared::new(38));
    let (client, mailbox) = compact_registered(&shared, zz_protocol::Subscriptions::control());
    shared.publish_control_command_guard(
        Some((client, 1)),
        RawText::from_bytes(vec![0xff, b'\n']),
        false,
        false,
    );
    let messages = reliable_children(&mailbox);
    assert!(
        matches!(messages.as_slice(), [ProtocolMessage::Event(Event { payload: EventPayload::ControlCommandGuardRaw { output, flags: 1, .. }, .. })] if output.as_bytes() == [0xff, b'\n'])
    );
}

#[test]
fn control_query_wakeup_excludes_hooks_and_pending_events() {
    let shared = Shared::new(47);
    let mut context = ExecutionContext::default();
    let command = |name: &str, args: &[&str]| PreparedCommand {
        invocation: CommandInvocation::new(name, args.iter().copied()),
        canonical_name: Some(name.to_owned()),
        alias_matched: false,
        result: PreparedCommandResult::Ready,
    };
    let query = command("display-message", &["-p", "#{session_name}"]);
    let mut inner = shared.inner.lock();
    for name in [
        "has-session",
        "list-buffers",
        "list-clients",
        "list-commands",
        "list-keys",
        "list-panes",
        "list-sessions",
        "list-windows",
        "show-buffer",
        "show-environment",
        "show-hooks",
        "show-messages",
        "show-options",
        "show-prompt-history",
        "show-window-options",
        "start-server",
    ] {
        assert!(
            ctrl::control_query_can_defer_wakeup(&inner, &context, &command(name, &[])),
            "{name}"
        );
    }
    assert!(ctrl::control_query_can_defer_wakeup(
        &inner, &context, &query
    ));
    assert!(ctrl::control_query_can_defer_wakeup(
        &inner,
        &context,
        &command("capture-pane", &["-p"]),
    ));
    for (name, args) in [
        ("display-message", &["-I"][..]),
        ("display-message", &["-d", "100", "x"][..]),
        ("capture-pane", &[][..]),
        ("wait-for", &["release"][..]),
        ("run-shell", &["sleep 30"][..]),
        ("agent-send", &["--wait", "x"][..]),
        ("capture-browser", &[][..]),
        ("load-browser-url", &["about:blank"][..]),
    ] {
        assert!(!ctrl::control_query_can_defer_wakeup(
            &inner,
            &context,
            &command(name, args),
        ));
    }
    for hook in ["after-display-message", "command-error"] {
        inner
            .engine
            .execute(
                &mut context,
                &CommandInvocation::new("set-hook", ["-g", hook, "wait-for release"]),
            )
            .expect("set blocking hook");
        assert!(!ctrl::control_query_can_defer_wakeup(
            &inner, &context, &query,
        ));
        inner
            .engine
            .execute(
                &mut context,
                &CommandInvocation::new("set-hook", ["-gu", hook]),
            )
            .expect("unset hook");
    }
    inner.waits.deferred_event_hooks.push(PendingHookEvent {
        name: "session-renamed",
        context: context.clone(),
        variables: BTreeMap::new(),
        exclude_client: None,
        control_notified: false,
    });
    assert!(!ctrl::control_query_can_defer_wakeup(
        &inner, &context, &query,
    ));
}

#[cfg(unix)]
#[test]
fn control_query_started_is_visible_before_a_blocking_after_hook_releases() {
    use std::os::unix::net::UnixStream;

    let shared = Arc::new(Shared::new(48));
    let (client, mailbox) = compact_registered(&shared, zz_protocol::Subscriptions::control());
    shared
        .client_writers
        .lock()
        .insert(client, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                ["-g", "after-display-message", "wait-for ctrl-hook-release"],
            ),
        )
        .expect("blocking after hook");
    let (mut reader, mut server) = UnixStream::pair().expect("pair");
    reader
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read deadline");
    let writer = {
        let mailbox = Arc::clone(&mailbox);
        thread::spawn(move || write_outbound(&mut server, &mailbox, &Weak::new(), client))
    };
    let (finished, completion) = crossbeam_channel::bounded(1);
    let worker = {
        let shared = Arc::clone(&shared);
        let mailbox = Arc::clone(&mailbox);
        thread::spawn(move || {
            shared.execute_compact_request(
                client,
                ClientKind::Control,
                &mut context,
                compact_exec_request(vec![CommandInvocation::new(
                    "display-message",
                    ["-p", "query result"],
                )]),
                &mailbox,
            );
            let _ = finished.send(());
        })
    };
    assert!(matches!(
        zz_protocol::read_protocol_message(&mut reader).expect("visible Started"),
        ProtocolMessage::Event(Event {
            payload: EventPayload::ControlCommandStarted { request_id: 1, .. },
            ..
        })
    ));
    assert!(matches!(
        zz_protocol::read_protocol_message(&mut reader).expect("after hook parked"),
        ProtocolMessage::CommandQueueParked { request_id: 1 }
    ));
    assert!(matches!(
        completion.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    ));
    shared.signal_wait_channel("ctrl-hook-release");
    completion
        .recv_timeout(Duration::from_secs(2))
        .expect("query completed after release");
    worker.join().expect("query worker");
    mailbox.close_after_flush();
    writer.join().expect("socket writer");
}

#[test]
fn a_late_gui_hook_wakes_quiet_started_before_its_reply() {
    let shared = Arc::new(Shared::new(49));
    let (client, mailbox) = compact_registered(&shared, zz_protocol::Subscriptions::control());
    shared
        .client_writers
        .lock()
        .insert(client, Arc::clone(&mailbox));
    let gui_mailbox = OutboundMailbox::new();
    let (gui_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&gui_mailbox),
    );
    let (session, pane, mut context) = {
        let mut inner = shared.inner.lock();
        let (session, _, pane) = inner
            .engine
            .state
            .create_session("late-gui-hook")
            .expect("model session");
        inner.engine.state.pane_mut(pane).expect("pane").kind = PaneKind::Browser(
            zz_protocol::BrowserDescriptor::single("about:blank".to_owned(), "default".to_owned()),
        );
        let context = ExecutionContext::for_pane(&inner.engine.state, pane).expect("context");
        (session, pane, context)
    };
    shared.attach(gui_client, session).expect("GUI attach");
    reliable_children(&gui_mailbox);
    reliable_children(&mailbox);
    let query = PreparedCommand {
        invocation: CommandInvocation::new("display-message", ["-p", "query result"]),
        canonical_name: Some("display-message".to_owned()),
        alias_matched: false,
        result: PreparedCommandResult::Ready,
    };
    assert!(ctrl::control_query_can_defer_wakeup(
        &shared.inner.lock(),
        &context,
        &query,
    ));
    let (armed, waiting) = crossbeam_channel::bounded(1);
    let (observable, visible) = crossbeam_channel::bounded(1);
    let observer = {
        let mailbox = Arc::clone(&mailbox);
        thread::spawn(move || {
            let mut state = mailbox.state.lock();
            let _ = armed.send(());
            let frame = if mailbox
                .ready
                .wait_for(&mut state, Duration::from_secs(2))
                .timed_out()
            {
                None
            } else {
                pop_ready_frame(&mut state)
            };
            let _ = observable.send(frame);
        })
    };
    waiting
        .recv_timeout(Duration::from_secs(2))
        .expect("observer waiting");
    assert!(mailbox.collect_control_query());
    assert!(mailbox.enqueue_reliable_with_wakeup(
        &Shared::event(EventPayload::ControlCommandStarted {
            request_id: 1,
            flags: 1,
            canonical_name: query.canonical_name.clone(),
            guard: true,
        }),
        false,
    ));
    let hook = format!(
        "capture-browser -t {pane} -o {}",
        tmux_path(&std::env::temp_dir().join("zz-ctrl-late-hook.png"))
    );
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-hook", ["-g", "after-display-message", &hook]),
        )
        .expect("install hook after eligibility");
    let worker = {
        let shared = Arc::clone(&shared);
        thread::spawn(move || {
            shared.run_command_hook(
                client,
                ClientKind::Control,
                &context,
                &query.invocation,
                "after-display-message",
                None,
            )
        })
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    let request_id = loop {
        let request =
            reliable_children(&gui_mailbox)
                .into_iter()
                .find_map(|message| match message {
                    ProtocolMessage::Event(Event {
                        payload:
                            EventPayload::BrowserCommand {
                                command: BrowserCommand::Screenshot { request_id, .. },
                                ..
                            },
                        ..
                    }) => Some(request_id),
                    _ => None,
                });
        if request.is_some() || Instant::now() >= deadline {
            break request;
        }
        thread::yield_now();
    };
    let started = visible.recv_timeout(Duration::from_secs(2)).ok().flatten();
    let pending = request_id.is_some_and(|request_id| {
        shared
            .inner
            .lock()
            .transfers
            .pending_gui_requests
            .contains_key(&request_id)
    });
    if let Some(request_id) = request_id {
        shared.complete_gui_request(
            gui_client,
            GuiResponse::Success {
                request_id,
                output: "saved".to_owned(),
            },
        );
    } else {
        shared.fail_gui_requests_for(gui_client);
    }
    worker.join().expect("hook worker");
    observer.join().expect("Started observer");
    assert!(pending, "hook was waiting for its GUI reply");
    let Some(ProtocolMessage::Batch(batch)) =
        started.map(|frame| zz_protocol::decode_protocol_frame(&frame).expect("decode Started"))
    else {
        panic!("late hook did not release the quiet batch")
    };
    assert!(matches!(
        batch.messages().expect("flat Started").as_slice(),
        [ProtocolMessage::Event(Event {
            payload: EventPayload::ControlCommandStarted { request_id: 1, .. },
            ..
        })]
    ));
}

#[cfg(unix)]
mod quiet_socket {
    use super::*;
    use std::os::{fd::AsFd as _, unix::net::UnixStream};

    fn socket_pair(mailbox: &OutboundMailbox) -> (UnixStream, UnixStream) {
        let (reader, server) = UnixStream::pair().expect("socket pair");
        reader
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("reader deadline");
        server
            .set_write_timeout(Some(Duration::from_secs(2)))
            .expect("writer deadline");
        rustix::net::sockopt::set_socket_send_buffer_size(&server, 4096)
            .expect("small send buffer");
        assert!(
            !rustix::fs::fcntl_getfl(&server)
                .expect("descriptor flags")
                .contains(rustix::fs::OFlags::NONBLOCK)
        );
        mailbox.state.lock().quiet_socket = Some(
            server
                .as_fd()
                .try_clone_to_owned()
                .expect("socket duplicate"),
        );
        (reader, server)
    }

    fn completion(output: Vec<u8>) -> [ProtocolMessage; 3] {
        [
            ProtocolMessage::Event(Event {
                sequence: 1,
                payload: EventPayload::ControlCommandStarted {
                    request_id: 1,
                    flags: 0,
                    canonical_name: Some("display-message".to_owned()),
                    guard: true,
                },
            }),
            ProtocolMessage::CommandResponse(CommandResponse::Success {
                request_id: 1,
                output: RawText::from_bytes(output),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: zz_protocol::StdoutClaim::Raw,
            }),
            ProtocolMessage::ExecExit(zz_protocol::ExecExit {
                server_id: 1,
                outcome: zz_protocol::ExecOutcome::Ran,
            }),
        ]
    }

    fn queue_completion(mailbox: &OutboundMailbox, messages: &[ProtocolMessage; 3]) -> Arc<[u8]> {
        assert!(mailbox.collect_control_query());
        let started: Arc<[u8]> = Arc::from(
            zz_protocol::encode_protocol_message(&messages[0]).expect("Started encoding"),
        );
        assert!(mailbox.enqueue_encoded_reliable(Arc::clone(&started)));
        assert!(
            mailbox.enqueue_control_group(
                messages[1..]
                    .iter()
                    .map(|message| {
                        zz_protocol::encode_protocol_message(message)
                            .expect("completion encoding")
                            .into()
                    })
                    .collect()
            )
        );
        started
    }

    fn read_frame(reader: &mut UnixStream) -> (ProtocolMessage, Vec<u8>) {
        let mut following = Vec::new();
        let message = zz_protocol::read_protocol_message_into(reader, &mut following)
            .expect("complete socket frame");
        let mut wire = u32::try_from(following.len())
            .expect("frame length")
            .to_le_bytes()
            .to_vec();
        wire.extend(following);
        assert_eq!(
            wire,
            zz_protocol::encode_protocol_message(&message).expect("owned wire encoding")
        );
        (message, wire)
    }

    fn no_socket_bytes(reader: &UnixStream) {
        assert_eq!(
            rustix::net::recv(reader, &mut [0_u8; 1], rustix::net::RecvFlags::DONTWAIT)
                .expect_err("no direct socket write"),
            rustix::io::Errno::AGAIN
        );
    }

    fn partial_wire(mailbox: &OutboundMailbox) -> (Vec<u8>, usize) {
        let state = mailbox.state.lock();
        let Some(OutboundFrame::Partial { frame, offset }) = state.reliable.front() else {
            panic!("expected a real partial socket write")
        };
        let wire = frame.as_ref().as_ref().to_vec();
        assert!(*offset > 0 && *offset < wire.len());
        assert_eq!(
            state.queued_bytes,
            state.reliable.iter().map(OutboundFrame::len).sum::<usize>()
        );
        assert_eq!(state.written_bytes, *offset as u64);
        assert_eq!(state.discarded_bytes, 0);
        assert_eq!(state.writer_inflight_bytes, 0);
        (wire, *offset)
    }

    fn writer(mailbox: &Arc<OutboundMailbox>, mut server: UnixStream) -> thread::JoinHandle<()> {
        let mailbox = Arc::clone(mailbox);
        thread::spawn(move || {
            write_outbound(&mut server, &mailbox, &Weak::new(), ClientId(0));
        })
    }

    fn writer_finished(mailbox: &OutboundMailbox) {
        let state = mailbox.state.lock();
        assert!(state.writer_finished);
        assert_eq!(state.writer_inflight_bytes, 0);
        assert!(state.quiet_socket.is_none());
    }

    #[test]
    fn full_completion_keeps_exact_wire_raw_output_and_recycled_buffers() {
        let mailbox = OutboundMailbox::new();
        let outer = Vec::with_capacity(4096);
        let allocation = outer.as_ptr();
        mailbox.recycle_frame(outer);
        let (mut reader, server) = socket_pair(&mailbox);
        let flags = rustix::fs::fcntl_getfl(&server).expect("blocking flags");
        let messages = completion(vec![0xff, b'\n']);
        let started = queue_completion(&mailbox, &messages);
        mailbox.finish_control_query();
        assert_eq!(
            rustix::fs::fcntl_getfl(&server).expect("flags after send"),
            flags
        );
        let (ProtocolMessage::Batch(batch), wire) = read_frame(&mut reader) else {
            panic!("expected a single flat completion")
        };
        assert_eq!(batch.messages().expect("flat completion"), messages);
        no_socket_bytes(&reader);
        assert_eq!(Arc::strong_count(&started), 1);
        let state = mailbox.state.lock();
        assert!(state.reliable.is_empty());
        assert_eq!(state.queued_bytes, 0);
        assert_eq!(state.written_bytes, wire.len() as u64);
        assert_eq!(state.discarded_bytes, 0);
        assert_eq!(state.writer_inflight_bytes, 0);
        assert_eq!(state.recycled_frames.len(), 3);
        assert!(
            state
                .recycled_frames
                .iter()
                .any(|frame| frame.as_ptr() == allocation)
        );
        assert!(state.recycled_capacity <= MAX_RECYCLED_FRAME_CAPACITY);
    }

    #[test]
    fn would_block_keeps_the_complete_group_for_the_normal_writer() {
        let mailbox = OutboundMailbox::new();
        let (mut reader, server) = socket_pair(&mailbox);
        let flags = rustix::fs::fcntl_getfl(&server).expect("blocking flags");
        let filler = [0xa5_u8; 4096];
        let mut occupied_bytes = 0;
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            assert!(Instant::now() < deadline && occupied_bytes < 4 * 1024 * 1024);
            match rustix::net::send(&server, &filler, rustix::net::SendFlags::DONTWAIT) {
                Ok(written) => {
                    assert!(written > 0);
                    occupied_bytes += written;
                }
                Err(error) if error == rustix::io::Errno::INTR => {}
                Err(error) if error == rustix::io::Errno::AGAIN => break,
                Err(error) => panic!("send-buffer fill failed: {error}"),
            }
        }
        let messages = completion(b"blocked result".to_vec());
        let started = queue_completion(&mailbox, &messages);
        mailbox.finish_control_query();
        assert_eq!(
            rustix::fs::fcntl_getfl(&server).expect("flags after block"),
            flags
        );
        let expected = {
            let state = mailbox.state.lock();
            let Some(frame @ OutboundFrame::Grouped { .. }) = state.reliable.front() else {
                panic!("blocked send changed the queued wire")
            };
            assert_eq!(state.written_bytes, 0);
            assert_eq!(state.queued_bytes, frame.len());
            assert!(state.recycled_frames.is_empty());
            frame.as_ref().to_vec()
        };
        let mut drained = vec![0; occupied_bytes];
        reader.read_exact(&mut drained).expect("drain filler");
        assert!(drained.iter().all(|byte| *byte == 0xa5));
        let writing = writer(&mailbox, server);
        let (ProtocolMessage::Batch(batch), wire) = read_frame(&mut reader) else {
            panic!("expected preserved completion")
        };
        assert_eq!(wire, expected);
        assert_eq!(batch.messages().expect("completion"), messages);
        mailbox.close_after_flush();
        writing.join().expect("normal writer");
        writer_finished(&mailbox);
        assert_eq!(mailbox.stats(), (wire.len() as u64, 0));
        assert_eq!(Arc::strong_count(&started), 1);
    }

    #[test]
    fn partial_tail_precedes_quiet_attach_and_sync_without_child_replay() {
        let mailbox = OutboundMailbox::new();
        let (mut reader, mut server) = socket_pair(&mailbox);
        let flags = rustix::fs::fcntl_getfl(&server).expect("blocking flags");
        let first = completion(vec![b'p'; 256 * 1024]);
        let retained = queue_completion(&mailbox, &first);
        mailbox.finish_control_query();
        assert_eq!(
            rustix::fs::fcntl_getfl(&server).expect("flags after partial"),
            flags
        );
        let (original, offset) = partial_wire(&mailbox);
        assert_eq!(Arc::strong_count(&retained), 2);
        let next = completion(b"next result".to_vec());
        let _next_started = queue_completion(&mailbox, &next);
        mailbox.finish_control_query();
        assert_eq!(partial_wire(&mailbox), (original.clone(), offset));
        mailbox.collect_control_attach();
        mailbox.state.lock().attach_batch = true;
        let snapshot = Shared::event(EventPayload::Snapshot(MuxSnapshot {
            generation: 7,
            ..MuxSnapshot::default()
        }));
        let view = Shared::event(EventPayload::ClientView(zz_protocol::ClientView {
            attachment_generation: 1,
            layout_generation: 1,
            ..zz_protocol::ClientView::default()
        }));
        assert!(mailbox.enqueue_reliable(&snapshot));
        assert!(mailbox.enqueue_reliable(&view));
        let mut in_flight = Vec::new();
        assert!(mailbox.recv_batch(&mut in_flight, MAX_OUTBOUND_BYTES));
        assert_eq!(in_flight.len(), 1);
        assert_eq!(in_flight[0].as_ref(), &original[offset..]);
        {
            let state = mailbox.state.lock();
            assert_eq!(state.writer_inflight_bytes, original.len() - offset);
            assert!(state.ctrl_collecting == ControlCollection::Attach);
            assert!(state.attach_batch);
        }
        assert!(mailbox.flush_control_batch(false));
        let sending = {
            let mailbox = Arc::clone(&mailbox);
            thread::spawn(move || {
                attach::write_frames(&mut server, &in_flight).expect("partial remainder");
                mailbox.finish_batch(&mut in_flight);
                write_outbound(&mut server, &mailbox, &Weak::new(), ClientId(0));
            })
        };
        let (ProtocolMessage::Batch(batch), wire) = read_frame(&mut reader) else {
            panic!("expected the original batch")
        };
        assert_eq!(wire, original);
        assert_eq!(batch.messages().expect("original children"), first);
        let (ProtocolMessage::Batch(batch), final_wire) = read_frame(&mut reader) else {
            panic!("expected the later flat batch")
        };
        let mut expected = next.to_vec();
        expected.extend([snapshot, view]);
        assert_eq!(batch.messages().expect("new children only"), expected);
        mailbox.close_after_flush();
        sending.join().expect("partial writer");
        writer_finished(&mailbox);
        assert_eq!(mailbox.stats(), ((wire.len() + final_wire.len()) as u64, 0));
        assert_eq!(Arc::strong_count(&retained), 1);
    }

    #[test]
    fn an_empty_queue_with_an_inflight_writer_refuses_direct_completion() {
        let mailbox = OutboundMailbox::new();
        let (mut reader, mut server) = socket_pair(&mailbox);
        assert!(mailbox.enqueue_reliable(&ProtocolMessage::TreeSync));
        let mut in_flight = Vec::new();
        assert!(mailbox.recv_batch(&mut in_flight, MAX_OUTBOUND_BYTES));
        assert!(mailbox.state.lock().reliable.is_empty());
        assert_eq!(
            mailbox.state.lock().writer_inflight_bytes,
            in_flight.iter().map(OutboundFrame::len).sum::<usize>()
        );
        let messages = completion(b"after old frame".to_vec());
        let _started = queue_completion(&mailbox, &messages);
        mailbox.finish_control_query();
        no_socket_bytes(&reader);
        assert!(matches!(
            mailbox.state.lock().reliable.front(),
            Some(OutboundFrame::Grouped { .. })
        ));
        attach::write_frames(&mut server, &in_flight).expect("old writer bytes");
        mailbox.finish_batch(&mut in_flight);
        assert_eq!(mailbox.state.lock().writer_inflight_bytes, 0);
        mailbox.close_after_flush();
        write_outbound(&mut server, &mailbox, &Weak::new(), ClientId(0));
        let (first, old_wire) = read_frame(&mut reader);
        assert_eq!(first, ProtocolMessage::TreeSync);
        let (ProtocolMessage::Batch(batch), wire) = read_frame(&mut reader) else {
            panic!("expected queued completion")
        };
        assert_eq!(batch.messages().expect("completion"), messages);
        assert_eq!(mailbox.stats(), ((old_wire.len() + wire.len()) as u64, 0));
        writer_finished(&mailbox);
    }

    #[test]
    fn closing_a_partial_frame_accounts_only_its_remaining_bytes() {
        let mailbox = OutboundMailbox::new();
        let (mut reader, mut server) = socket_pair(&mailbox);
        let messages = completion(vec![b'c'; 256 * 1024]);
        let started = queue_completion(&mailbox, &messages);
        mailbox.finish_control_query();
        let (original, offset) = partial_wire(&mailbox);
        mailbox.close();
        let mut received = Vec::new();
        reader.read_to_end(&mut received).expect("hard-close EOF");
        assert_eq!(received, original[..offset]);
        assert_eq!(
            mailbox.stats(),
            (offset as u64, (original.len() - offset) as u64)
        );
        {
            let state = mailbox.state.lock();
            assert!(state.closed && state.quiet_socket.is_none());
            assert!(state.reliable.is_empty());
            assert_eq!(state.queued_bytes, 0);
        }
        write_outbound(&mut server, &mailbox, &Weak::new(), ClientId(0));
        assert_eq!(Arc::strong_count(&started), 1);
        writer_finished(&mailbox);
    }

    #[test]
    fn overflow_preserves_a_partial_tail_before_its_exit_marker() {
        let mailbox = OutboundMailbox::new();
        let (mut reader, server) = socket_pair(&mailbox);
        let messages = completion(vec![b'o'; 256 * 1024]);
        let started = queue_completion(&mailbox, &messages);
        mailbox.finish_control_query();
        let (original, offset) = partial_wire(&mailbox);
        let child = zz_protocol::encode_protocol_message(&ProtocolMessage::TreeSync)
            .expect("overflow filler");
        for _ in 1..MAX_RELIABLE_MESSAGES {
            assert!(mailbox.enqueue_encoded_reliable(child.clone()));
        }
        assert!(!mailbox.enqueue_encoded_reliable(child.clone()));
        {
            let state = mailbox.state.lock();
            assert!(state.closed);
            assert_eq!(state.reliable.len(), 2);
            assert!(state.reliable.front().expect("tail").is_partial());
            assert_eq!(state.written_bytes, offset as u64);
            assert_eq!(
                state.discarded_bytes,
                ((MAX_RELIABLE_MESSAGES - 1) * child.len()) as u64
            );
            assert!(state.queued_bytes <= MAX_OUTBOUND_BYTES);
        }
        let sending = writer(&mailbox, server);
        let (ProtocolMessage::Batch(batch), wire) = read_frame(&mut reader) else {
            panic!("expected completed partial group")
        };
        assert_eq!(wire, original);
        assert_eq!(batch.messages().expect("no replay"), messages);
        let (exit, exit_wire) = read_frame(&mut reader);
        assert!(matches!(exit, ProtocolMessage::Event(Event {
            payload: EventPayload::ControlExit { reason }, ..
        }) if reason == "too far behind"));
        assert_eq!(reader.read(&mut [0_u8; 1]).expect("exit EOF"), 0);
        sending.join().expect("overflow writer");
        writer_finished(&mailbox);
        assert_eq!(mailbox.stats().0, (wire.len() + exit_wire.len()) as u64);
        assert_eq!(mailbox.state.lock().queued_bytes, 0);
        assert_eq!(Arc::strong_count(&started), 1);
    }

    #[test]
    fn an_exit_marker_that_cannot_fit_shuts_down_without_appending_wire() {
        let mailbox = OutboundMailbox::new();
        let (mut reader, mut server) = socket_pair(&mailbox);
        server.write_all(&[0]).expect("already sent prefix");
        {
            let mut state = mailbox.state.lock();
            state.written_bytes = 1;
            state.queued_bytes = MAX_OUTBOUND_BYTES - 1;
            state.reliable.push_back(OutboundFrame::Partial {
                frame: Box::new(OutboundFrame::Grouped {
                    encoded: vec![0; MAX_OUTBOUND_BYTES],
                    frames: Vec::new(),
                }),
                offset: 1,
            });
        }
        assert!(!mailbox.enqueue_reliable(&ProtocolMessage::TreeSync));
        let mut received = Vec::new();
        reader
            .read_to_end(&mut received)
            .expect("capacity shutdown");
        assert_eq!(received, [0]);
        let state = mailbox.state.lock();
        assert!(state.closed && state.quiet_socket.is_none());
        assert!(state.reliable.is_empty());
        assert_eq!(state.queued_bytes, 0);
        assert_eq!(state.written_bytes, 1);
        assert_eq!(state.discarded_bytes, (MAX_OUTBOUND_BYTES - 1) as u64);
    }

    #[test]
    fn disabled_buffered_and_unsupported_sockets_keep_existing_drains() {
        for (buffered, socket) in [(true, true), (false, false), (false, true)] {
            if !buffered && socket {
                continue;
            }
            let mailbox = if buffered {
                OutboundMailbox::buffered()
            } else {
                OutboundMailbox::new()
            };
            let (reader, _server) = socket_pair(&mailbox);
            if !socket {
                mailbox.state.lock().quiet_socket = None;
            }
            let messages = completion(b"normal drain".to_vec());
            let _started = queue_completion(&mailbox, &messages);
            mailbox.finish_control_query();
            no_socket_bytes(&reader);
            assert_eq!(mailbox.stats(), (0, 0));
            let mut wire = Vec::new();
            mailbox.drain_reliable_into(&mut wire);
            let ProtocolMessage::Batch(batch) =
                zz_protocol::decode_protocol_frame(&wire).expect("existing direct drain")
            else {
                panic!("expected an encoded group")
            };
            assert_eq!(batch.messages().expect("drained completion"), messages);
            assert_eq!(mailbox.state.lock().queued_bytes, 0);
            assert_eq!(mailbox.state.lock().writer_inflight_bytes, 0);
        }
    }

    #[test]
    fn hook_release_never_uses_the_final_completion_socket_attempt() {
        let mailbox = OutboundMailbox::new();
        let (reader, _server) = socket_pair(&mailbox);
        let messages = completion(b"hook yield".to_vec());
        let _started = queue_completion(&mailbox, &messages);
        mailbox.release_control_query();
        no_socket_bytes(&reader);
        assert_eq!(mailbox.stats(), (0, 0));
        assert_eq!(reliable_children(&mailbox), messages);
    }

    #[test]
    fn a_writer_error_releases_a_partial_frame_without_counting_its_prefix_twice() {
        let mailbox = OutboundMailbox::new();
        let (reader, mut server) = socket_pair(&mailbox);
        let messages = completion(vec![b'e'; 256 * 1024]);
        let started = queue_completion(&mailbox, &messages);
        mailbox.finish_control_query();
        let (original, offset) = partial_wire(&mailbox);
        drop(reader);
        write_outbound(&mut server, &mailbox, &Weak::new(), ClientId(0));
        writer_finished(&mailbox);
        assert_eq!(
            mailbox.stats(),
            (offset as u64, (original.len() - offset) as u64)
        );
        let state = mailbox.state.lock();
        assert!(state.closed && state.reliable.is_empty());
        assert_eq!(state.queued_bytes, 0);
        assert_eq!(Arc::strong_count(&started), 1);
    }

    #[test]
    fn panic_after_writer_dequeue_closes_the_socket_and_releases_ownership() {
        let mailbox = OutboundMailbox::new();
        let (mut reader, _server) = socket_pair(&mailbox);
        let messages = completion(vec![b'p'; 256 * 1024]);
        let started = queue_completion(&mailbox, &messages);
        mailbox.finish_control_query();
        let (original, offset) = partial_wire(&mailbox);
        let writing = {
            let mailbox = Arc::clone(&mailbox);
            thread::spawn(move || {
                let _finished = OutboundWriterGuard(&mailbox);
                let mut batch = Vec::new();
                assert!(mailbox.recv_batch(&mut batch, MAX_OUTBOUND_BYTES));
                assert_eq!(
                    mailbox.state.lock().writer_inflight_bytes,
                    batch.iter().map(OutboundFrame::len).sum::<usize>()
                );
                panic!("controlled writer panic");
            })
        };
        assert!(writing.join().is_err());
        let mut received = Vec::new();
        reader.read_to_end(&mut received).expect("panic EOF");
        assert_eq!(received, original[..offset]);
        writer_finished(&mailbox);
        let state = mailbox.state.lock();
        assert!(state.closed && state.reliable.is_empty());
        assert_eq!(state.queued_bytes, 0);
        assert_eq!(state.written_bytes, offset as u64);
        assert_eq!(state.discarded_bytes, (original.len() - offset) as u64);
        assert_eq!(Arc::strong_count(&started), 1);
    }

    #[test]
    fn closing_after_a_successful_write_does_not_discard_the_inflight_bytes() {
        let mailbox = OutboundMailbox::new();
        let (mut reader, mut server) = socket_pair(&mailbox);
        let messages = completion(b"completed before close".to_vec());
        let started = queue_completion(&mailbox, &messages);
        mailbox.release_control_query();
        let finished = OutboundWriterGuard(&mailbox);
        let mut batch = Vec::new();
        assert!(mailbox.recv_batch(&mut batch, MAX_OUTBOUND_BYTES));
        attach::write_frames(&mut server, &batch).expect("successful writer bytes");
        mailbox.close();
        mailbox.finish_batch(&mut batch);
        drop(finished);
        let (ProtocolMessage::Batch(batch), wire) = read_frame(&mut reader) else {
            panic!("expected completed writer frame")
        };
        assert_eq!(batch.messages().expect("completed children"), messages);
        assert_eq!(reader.read(&mut [0_u8; 1]).expect("closed EOF"), 0);
        assert_eq!(mailbox.stats(), (wire.len() as u64, 0));
        writer_finished(&mailbox);
        assert_eq!(Arc::strong_count(&started), 1);
    }

    #[test]
    fn compact_control_keeps_socket_writes_on_the_loop() {
        let directory = tempfile::Builder::new()
            .prefix("zzqf.")
            .tempdir_in("/tmp")
            .expect("local socket directory");
        let path = directory.path().join("s");
        let listener = LocalTransport::bind(&path).expect("local listener");
        let shared = Arc::new(Shared::new(61));
        let worker = {
            let shared = Arc::clone(&shared);
            thread::spawn(move || {
                let server = listener.accept().expect("local connection");
                server
                    .set_timeout(Some(Duration::from_secs(2)))
                    .expect("server deadline");
                handle_connection(server, &shared)
            })
        };
        let mut client = LocalTransport::connect(&path).expect("local client");
        client
            .set_timeout(Some(Duration::from_secs(2)))
            .expect("client deadline");
        let mut hello = compact_hello(ClientKind::Control);
        hello.subscriptions = zz_protocol::Subscriptions::control();
        zz_protocol::write_protocol_message(&mut client, &ProtocolMessage::Hello(hello))
            .expect("compact hello");
        let ProtocolMessage::Welcome(welcome) =
            zz_protocol::read_protocol_message(&mut client).expect("local welcome")
        else {
            panic!("expected compact welcome")
        };
        assert!(matches!(
            zz_protocol::read_protocol_message(&mut client).expect("initial batch"),
            ProtocolMessage::Batch(_)
        ));
        let mailbox = shared
            .client_writers
            .lock()
            .get(&welcome.client_id)
            .cloned()
            .expect("registered local writer");
        assert!(mailbox.state.lock().quiet_socket.is_some());
        assert!(mailbox.loop_waker.lock().is_some());
        zz_protocol::write_protocol_message(
            &mut client,
            &ProtocolMessage::Exec(compact_exec_request(vec![CommandInvocation::new(
                "display-message",
                ["-p", "local socket output"],
            )])),
        )
        .expect("local query");
        let ProtocolMessage::Batch(batch) =
            zz_protocol::read_protocol_message(&mut client).expect("local completion")
        else {
            panic!("expected joined quiet completion")
        };
        let messages = batch.messages().expect("local children");
        assert!(matches!(
            messages.as_slice(),
            [
                ProtocolMessage::Event(Event { payload: EventPayload::ControlCommandStarted { .. }, .. }),
                ProtocolMessage::CommandResponse(CommandResponse::Success { output, .. }),
                ProtocolMessage::ExecExit(_),
            ] if output.as_bytes() == b"local socket output"
        ));
        drop(client);
        let _ = worker.join().expect("local connection cleanup");
        writer_finished(&mailbox);
    }
}
