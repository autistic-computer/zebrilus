#[cfg(unix)]
use super::test_support::*;
use super::*;
#[cfg(unix)]
use crate::DEFAULT_HISTORY_LIMIT;

#[test]
fn actor_command_mailbox_backpressures_after_one_pending_command() {
    let (commands, pending) = command_channel();
    commands
        .try_send(Command::Shutdown)
        .expect("first pending command");
    assert!(matches!(
        commands.try_send(Command::Shutdown),
        Err(crossbeam_channel::TrySendError::Full(Command::Shutdown))
    ));
    assert!(matches!(
        pending.try_recv().expect("pending command"),
        Command::Shutdown
    ));
}

#[cfg(unix)]
fn term_ignoring_test_session(view: TerminalViewId, command: &str) -> TerminalSession {
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            command: Some(vec![command.to_owned()]),
            ..TerminalSpawn::default()
        },
    );
    attach_streaming(&session, view);
    wait_for_test_viewport(&session, |viewport| {
        let mut contents = String::new();
        for cell in viewport.cells.iter() {
            viewport.push_glyph(*cell, &mut contents);
        }
        contents.contains("TERMINATION_READY")
    });
    session
}

#[cfg(unix)]
fn kill_test_process_group(process_id: u32) {
    if let Some(group) = i32::try_from(process_id)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
}

#[cfg(unix)]
#[test]
fn terminate_escalates_while_session_is_retained() {
    let session = term_ignoring_test_session(
        TerminalViewId(301),
        "sh -c 'trap \"\" TERM HUP; while :; do sleep 1; done' & printf 'TERMINATION_READY\\r\\n'",
    );
    let process_id = session.process_id().expect("shell process id");
    session.terminate();
    let deadline = Instant::now() + Duration::from_secs(2);
    while session.completion().is_none() {
        if Instant::now() >= deadline {
            kill_test_process_group(process_id);
            panic!("terminal did not escalate termination");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
#[test]
fn dropping_a_terminating_session_never_waits_forever() {
    let session = term_ignoring_test_session(
        TerminalViewId(302),
        "trap '' TERM HUP; printf 'TERMINATION_READY\\r\\n'; while :; do sleep 1; done",
    );
    let process_id = session.process_id().expect("shell process id");
    let events = session.events();
    session.terminate();
    drop(session);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !events.receiver.is_closed() {
        if Instant::now() >= deadline {
            kill_test_process_group(process_id);
            panic!("terminating terminal worker did not stop");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn pty_input_past_the_slot_cap_waits_in_order_and_stays_byte_bounded() {
    assert!(
        std::mem::size_of::<(Command, usize)>() + std::mem::size_of::<KeyInput>()
            <= PTY_INPUT_OVERFLOW_ENTRY_BYTES
    );
    let (input, pending) = input_channel_with_limits(2, PTY_INPUT_COMMAND_FLOOR_BYTES * 4);
    for token in 1..=5 {
        input
            .try_send(Command::PendingPasteOpened { token })
            .expect("input past the slot cap");
    }
    assert_eq!(pending.len(), 2);
    assert_eq!(
        input.pending(),
        (
            5,
            PTY_INPUT_COMMAND_FLOOR_BYTES * 2 + PTY_INPUT_OVERFLOW_ENTRY_BYTES * 3
        )
    );
    let mut order = Vec::new();
    while let Ok(queued) = pending.try_recv() {
        let Command::PendingPasteOpened { token } = queued.command else {
            panic!("unexpected input");
        };
        order.push(token);
        if token == 4 {
            input
                .try_send(Command::PendingPasteOpened { token: 6 })
                .expect("input behind the overflow");
        }
    }
    assert_eq!(order, [1, 2, 3, 4, 5, 6]);
    assert_eq!(input.pending(), (0, 0));
    input
        .try_send(Command::PendingPasteOpened { token: 7 })
        .expect("drained overflow returns to the slots");
    assert_eq!(pending.len(), 1);
    drop(pending);
    assert_eq!(input.pending(), (0, 0));

    let (input, pending) = input_channel_with_limits(
        1,
        PTY_INPUT_COMMAND_FLOOR_BYTES + PTY_INPUT_OVERFLOW_ENTRY_BYTES * 2,
    );
    for token in 1..=3 {
        input
            .try_send(Command::PendingPasteOpened { token })
            .expect("input within the byte budget");
    }
    assert!(matches!(
        input.try_send(Command::PendingPasteOpened { token: 4 }),
        Err(crossbeam_channel::TrySendError::Full(
            Command::PendingPasteOpened { token: 4 }
        ))
    ));
    drop(pending);
    assert_eq!(input.pending(), (0, 0));
    assert!(matches!(
        input.try_send(Command::PendingPasteOpened { token: 5 }),
        Err(crossbeam_channel::TrySendError::Disconnected(_))
    ));

    let (input, pending) = input_channel_with_limits(8, PTY_INPUT_COMMAND_FLOOR_BYTES * 2);
    assert!(matches!(
        input.try_send(Command::Text {
            view: None,
            text: Arc::from("x".repeat(PTY_INPUT_COMMAND_FLOOR_BYTES + 1)),
        }),
        Err(crossbeam_channel::TrySendError::Full(Command::Text { .. }))
    ));
    assert!(pending.is_empty());
    assert_eq!(input.pending(), (0, 0));
}

#[test]
fn request_stranded_behind_a_stopped_worker_fails_without_waiting_out_the_timeout() {
    let (control, stranded) = command_channel();
    let (alive, liveness) = crossbeam_channel::bounded(0);
    let commands = CommandSender {
        queues: Arc::new(CommandQueues {
            control,
            input: None,
            liveness,
            slot: Arc::default(),
            wake: ActorWake::none(),
        }),
    };
    drop(alive);

    let started = Instant::now();
    assert_eq!(
        commands.request(|reply| Command::Capture(Box::new(CaptureRequest {
            options: CaptureOptions::default(),
            reply: reply.into(),
        }))),
        Err(ActorRequestError::ActorStopped)
    );
    assert!(started.elapsed() < CAPTURE_TIMEOUT);
    assert_eq!(stranded.len(), 1);
}

#[test]
fn pty_input_lane_preserves_paste_order_and_keeps_view_actions_on_control() {
    let (control, control_rx) = command_channel();
    let (input, input_rx) = input_channel();
    let commands = CommandSender {
        queues: Arc::new(CommandQueues {
            control,
            input: Some(input),
            liveness: crossbeam_channel::never(),
            slot: Arc::default(),
            wake: ActorWake::none(),
        }),
    };
    commands
        .send(Command::PendingPasteOpened { token: 7 })
        .expect("open pending paste");
    commands
        .send(Command::ViewAction {
            view: TerminalViewId(1),
            action: TerminalViewAction::Paste("image.png".to_owned()),
        })
        .expect("paste path");
    commands
        .send(Command::ViewAction {
            view: TerminalViewId(1),
            action: TerminalViewAction::ScrollTop,
        })
        .expect("scroll view");

    assert!(matches!(
        input_rx.recv().expect("pending paste input").command,
        Command::PendingPasteOpened { token: 7 }
    ));
    assert!(matches!(
        input_rx.recv().expect("paste input").command,
        Command::ViewAction {
            action: TerminalViewAction::Paste(_),
            ..
        }
    ));
    assert!(matches!(
        control_rx.recv().expect("view control"),
        Command::ViewAction {
            action: TerminalViewAction::ScrollTop,
            ..
        }
    ));
}

#[test]
fn terminal_event_queue_has_one_coalesced_and_four_reliable_slots() {
    let event_state = Arc::new(EventQueueState::new());
    let (event_tx, events) = terminal_event_channel(&event_state);
    let publisher = Publisher {
        event_tx,
        latest: Arc::new(RwLock::new(PublishedViewports::new(
            TerminalViewport::blank(1, 1, SessionStatus::Running),
        ))),
        state: Arc::clone(&event_state),
    };

    for _ in 0..8 {
        publisher.publish(TerminalViewport::blank(1, 1, SessionStatus::Running));
    }
    assert_eq!(
        events.receiver.capacity(),
        Some(MAX_PENDING_TERMINAL_EVENTS)
    );
    assert_eq!(events.receiver.len(), 1);

    for index in 0..MAX_PENDING_RELIABLE_EVENTS {
        publisher
            .send_reliable(TerminalEvent::OpenUri(Box::new(TerminalOpenUri {
                view: TerminalViewId(1),
                uri: format!("https://example.com/{index}"),
            })))
            .expect("reliable event within physical queue budget");
    }
    assert_eq!(events.receiver.len(), MAX_PENDING_TERMINAL_EVENTS);
    assert!(events.receiver.is_full());
    assert!(matches!(
        publisher.send_reliable(TerminalEvent::OpenUri(Box::new(TerminalOpenUri {
            view: TerminalViewId(1),
            uri: "https://example.com/overflow".to_owned(),
        }))),
        Err(WorkerError::EventBackpressure)
    ));

    assert!(matches!(
        events.try_recv().expect("coalesced viewport event"),
        TerminalEvent::ViewportReady {
            output_activity: false
        }
    ));
    assert!(!event_state.notification_pending.load(Ordering::Acquire));
    publisher.mark_output_activity();
    publisher.publish(TerminalViewport::blank(1, 1, SessionStatus::Running));
    assert_eq!(events.receiver.len(), MAX_PENDING_TERMINAL_EVENTS);
    assert!(events.receiver.is_full());

    let mut reliable_events = 0;
    let mut viewport_events = 0;
    let mut output_activity_events = 0;
    while let Ok(event) = events.try_recv() {
        match event {
            TerminalEvent::ViewportReady { output_activity } => {
                viewport_events += 1;
                output_activity_events += usize::from(output_activity);
            }
            TerminalEvent::OpenUri(_) => reliable_events += 1,
            TerminalEvent::ViewClosed(_)
            | TerminalEvent::CopyReady { .. }
            | TerminalEvent::ClipboardSet { .. }
            | TerminalEvent::Bell
            | TerminalEvent::RenameWindow(_)
            | TerminalEvent::PlaceholderBound { .. }
            | TerminalEvent::PendingPasteExpired { .. } => {
                panic!("unexpected event in queue invariant test")
            }
        }
    }
    assert_eq!(viewport_events, 1);
    assert_eq!(output_activity_events, 1);
    assert_eq!(reliable_events, MAX_PENDING_RELIABLE_EVENTS);
    assert_eq!(event_state.pending_reliable.load(Ordering::Acquire), 0);
    assert_eq!(
        event_state.pending_reliable_bytes.load(Ordering::Acquire),
        0
    );
    assert!(!event_state.notification_pending.load(Ordering::Acquire));
}

#[test]
fn reliable_terminal_event_backlog_is_bounded_and_released_on_receive() {
    let event_state = Arc::new(EventQueueState::new());
    let (event_tx, events) = terminal_event_channel(&event_state);
    let publisher = Publisher {
        event_tx,
        latest: Arc::new(RwLock::new(PublishedViewports::new(
            TerminalViewport::blank(1, 1, SessionStatus::Running),
        ))),
        state: Arc::clone(&event_state),
    };

    for index in 0..MAX_PENDING_RELIABLE_EVENTS {
        publisher
            .send_reliable(TerminalEvent::OpenUri(Box::new(TerminalOpenUri {
                view: TerminalViewId(1),
                uri: format!("https://example.com/{index}"),
            })))
            .expect("reliable event within budget");
    }
    assert_eq!(
        event_state.pending_reliable.load(Ordering::Acquire),
        MAX_PENDING_RELIABLE_EVENTS
    );
    assert!(
        event_state.pending_reliable_bytes.load(Ordering::Acquire)
            <= MAX_PENDING_RELIABLE_EVENT_BYTES
    );
    assert!(matches!(
        publisher.send_reliable(TerminalEvent::OpenUri(Box::new(TerminalOpenUri {
            view: TerminalViewId(1),
            uri: "https://example.com/overflow".to_owned(),
        }))),
        Err(WorkerError::EventBackpressure)
    ));

    let pending_bytes_before_receive = event_state.pending_reliable_bytes.load(Ordering::Acquire);
    let event = events.try_recv().expect("first reliable event");
    let received_bytes = reliable_event_bytes(&event);
    assert!(matches!(event, TerminalEvent::OpenUri(_)));
    assert_eq!(
        event_state.pending_reliable.load(Ordering::Acquire),
        MAX_PENDING_RELIABLE_EVENTS - 1
    );
    assert_eq!(
        event_state.pending_reliable_bytes.load(Ordering::Acquire),
        pending_bytes_before_receive - received_bytes
    );
    publisher
        .send_reliable(TerminalEvent::OpenUri(Box::new(TerminalOpenUri {
            view: TerminalViewId(1),
            uri: "https://example.com/reused-slot".to_owned(),
        })))
        .expect("released reliable-event slot");
    assert_eq!(
        event_state.pending_reliable.load(Ordering::Acquire),
        MAX_PENDING_RELIABLE_EVENTS
    );
}

#[test]
fn reliable_terminal_event_byte_limit_rolls_back_reserved_message_slot() {
    let event_state = Arc::new(EventQueueState::new());
    let (event_tx, events) = terminal_event_channel(&event_state);
    let publisher = Publisher {
        event_tx,
        latest: Arc::new(RwLock::new(PublishedViewports::new(
            TerminalViewport::blank(1, 1, SessionStatus::Running),
        ))),
        state: Arc::clone(&event_state),
    };
    let event = TerminalEvent::OpenUri(Box::new(TerminalOpenUri {
        view: TerminalViewId(1),
        uri: "x".to_owned(),
    }));
    let occupied_bytes = MAX_PENDING_RELIABLE_EVENT_BYTES
        .saturating_sub(reliable_event_bytes(&event))
        .saturating_add(1);
    event_state
        .pending_reliable_bytes
        .store(occupied_bytes, Ordering::Release);

    assert!(matches!(
        publisher.send_reliable(TerminalEvent::OpenUri(Box::new(TerminalOpenUri {
            view: TerminalViewId(1),
            uri: "x".to_owned(),
        }))),
        Err(WorkerError::EventBackpressure)
    ));
    assert_eq!(event_state.pending_reliable.load(Ordering::Acquire), 0);
    assert_eq!(
        event_state.pending_reliable_bytes.load(Ordering::Acquire),
        occupied_bytes
    );
    assert!(matches!(
        events.try_recv(),
        Err(async_channel::TryRecvError::Empty)
    ));
}

#[test]
fn user_actions_drop_backpressure_without_stopping_the_worker() {
    let event_state = Arc::new(EventQueueState::new());
    let (event_tx, events) = terminal_event_channel(&event_state);
    let publisher = Publisher {
        event_tx,
        latest: Arc::new(RwLock::new(PublishedViewports::new(
            TerminalViewport::blank(1, 1, SessionStatus::Running),
        ))),
        state: Arc::clone(&event_state),
    };

    for index in 0..MAX_PENDING_RELIABLE_EVENTS {
        publisher
            .send_reliable(TerminalEvent::OpenUri(Box::new(TerminalOpenUri {
                view: TerminalViewId(1),
                uri: format!("https://example.com/{index}"),
            })))
            .expect("fill reliable queue");
    }

    publisher
        .open_uri(TerminalViewId(1), "https://example.com/dropped".to_owned())
        .expect("URI backpressure is nonfatal");
    publisher
        .copy_ready(
            TerminalViewId(1),
            Box::new(TerminalCopyReady {
                request_id: 7,
                clipboard: None,
                buffer: None,
                pipe: None,
                text: "dropped copy".to_owned(),
                view_changed: false,
            }),
        )
        .expect("copy backpressure is nonfatal");
    assert_eq!(
        event_state.pending_reliable.load(Ordering::Acquire),
        MAX_PENDING_RELIABLE_EVENTS
    );
    assert_eq!(events.receiver.len(), MAX_PENDING_RELIABLE_EVENTS);
}

#[test]
fn stopped_event_consumer_remains_fatal_for_user_actions() {
    let event_state = Arc::new(EventQueueState::new());
    let (event_tx, events) = terminal_event_channel(&event_state);
    let publisher = Publisher {
        event_tx,
        latest: Arc::new(RwLock::new(PublishedViewports::new(
            TerminalViewport::blank(1, 1, SessionStatus::Running),
        ))),
        state: Arc::clone(&event_state),
    };
    drop(events);

    assert!(matches!(
        publisher.open_uri(TerminalViewId(1), "https://example.com".to_owned()),
        Err(WorkerError::EventConsumerStopped)
    ));
    assert_eq!(event_state.pending_reliable.load(Ordering::Acquire), 0);
    assert_eq!(
        event_state.pending_reliable_bytes.load(Ordering::Acquire),
        0
    );
}

#[cfg(target_pointer_width = "64")]
#[test]
fn actor_event_handles_share_compact_queue_state() {
    let word = std::mem::size_of::<usize>();
    assert!(std::mem::size_of::<EventQueueState>() <= 8 * word);
    assert_eq!(std::mem::size_of::<Publisher>(), 3 * word);
    assert_eq!(std::mem::size_of::<TerminalEvents>(), 3 * word);
    assert!(
        std::mem::size_of::<TerminalSession>() <= 16 * word,
        "{}",
        std::mem::size_of::<TerminalSession>()
    );
}
