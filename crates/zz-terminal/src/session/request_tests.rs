use super::*;

fn sender(capacity: usize) -> (CommandSender, Receiver<Command>, Sender<Infallible>) {
    let (control, receiver) = crossbeam_channel::bounded(capacity);
    let (alive, liveness) = crossbeam_channel::bounded(0);
    (
        CommandSender {
            queues: Arc::new(CommandQueues {
                control,
                input: None,
                liveness,
                slot: Arc::default(),
                wake: ActorWake::none(),
            }),
        },
        receiver,
        alive,
    )
}

#[test]
fn request_tokens_do_not_wait_for_acknowledgement() {
    let (sender, receiver, _alive) = sender(4);
    let notified = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&notified);
    let _round_trips = forbid_actor_round_trips();
    let mut token = sender.request_token(
        Arc::new(move || {
            count.fetch_add(1, Ordering::Relaxed);
        }),
        |reply| Command::Settle { reply },
    );
    assert!(token.poll(Instant::now()).is_none());
    let Command::Settle { reply } = receiver.try_recv().unwrap() else {
        panic!()
    };
    reply.send(()).unwrap();
    assert_eq!(notified.load(Ordering::Acquire), 1);
    assert_eq!(token.poll(Instant::now()), Some(Ok(())));
}

#[test]
fn full_request_queue_delivers_in_order_without_blocking_or_polling() {
    let (sender, receiver, _alive) = sender(1);
    sender.try_send(Command::ResetScreen).unwrap();
    let mut token = sender.request_token(Arc::new(|| {}), |reply| Command::Settle { reply });
    assert_eq!(token.next_poll(), token.deadline);
    assert!(token.poll(Instant::now()).is_none());
    let woke_by = receiver.try_recv().unwrap();
    assert!(receiver.try_recv().is_err());
    let mut commands = take_control_slot(&sender.queues.slot, Some(woke_by), true).into_iter();
    assert!(matches!(commands.next(), Some(Command::ResetScreen)));
    let Some(Command::Settle { reply }) = commands.next() else {
        panic!()
    };
    assert!(commands.next().is_none());
    reply.send(()).unwrap();
    assert_eq!(token.poll(Instant::now()), Some(Ok(())));
}

#[test]
fn a_request_deferred_after_the_queue_drained_wakes_the_actor() {
    let (sender, receiver, _alive) = sender(1);
    let (reply, mut token) = sender.reply_token(CAPTURE_TIMEOUT, Arc::new(|| {}));
    sender.defer(Command::Settle { reply });
    let woke_by = receiver.try_recv().unwrap();
    assert!(matches!(woke_by, Command::Wake));
    let mut commands = take_control_slot(&sender.queues.slot, Some(woke_by), true).into_iter();
    let Some(Command::Settle { reply }) = commands.next() else {
        panic!()
    };
    reply.send(()).unwrap();
    assert_eq!(token.poll(Instant::now()), Some(Ok(())));
}

#[test]
fn request_tokens_keep_the_two_second_deadline_and_reject_late_replies() {
    let (sender, receiver, _alive) = sender(2);
    let started = Instant::now();
    let mut token = sender.request_token(Arc::new(|| {}), |reply| Command::Settle { reply });
    assert!(token.deadline >= started + CAPTURE_TIMEOUT);
    assert_eq!(
        token.poll(token.deadline),
        Some(Err(TerminalRequestError::TimedOut))
    );
    let Command::Settle { mut reply } = receiver.try_recv().unwrap() else {
        panic!()
    };
    if let ActorReply::Async { deadline, .. } = &mut reply {
        *deadline = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .unwrap();
    }
    reply.send(()).unwrap();
    assert_eq!(
        token.poll(Instant::now()),
        Some(Err(TerminalRequestError::TimedOut))
    );
}

#[test]
fn stopped_actor_completes_a_request_token() {
    let (sender, receiver, alive) = sender(2);
    let mut token = sender.request_token(Arc::new(|| {}), |reply| Command::Settle { reply });
    drop(alive);
    assert_eq!(
        token.poll(Instant::now()),
        Some(Err(TerminalRequestError::ActorStopped))
    );
    drop(receiver);
}

#[test]
fn identity_registration_cannot_miss_readiness() {
    let (sender, _receiver, _alive) = sender(2);
    let state = EventQueueState::new();
    let (reply, mut token) = sender.reply_token(Duration::from_secs(2), Arc::new(|| {}));
    state.identity.state.lock().replies.push(reply);
    assert!(token.poll(Instant::now()).is_none());
    state.resolve_identity();
    assert_eq!(token.poll(Instant::now()), Some(Ok(true)));
    assert!(state.identity.state.lock().ready);
    assert!(state.identity.state.lock().replies.is_empty());
}

fn finish<T>(mut request: TerminalRequest<T>) -> Result<T, TerminalRequestError> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(result) = request.poll(Instant::now()) {
            return result;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn asynchronous_capture_and_history_preserve_the_canonical_rows_and_flags() {
    let terminal =
        TerminalSession::spawn_empty_with_appearance(64, Arc::new(TerminalAppearance::default()));
    terminal.resize(8, 3, 8, 16);
    assert!(terminal.feed(Arc::from(
        b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\n".as_slice()
    )));
    let options = CaptureOptions {
        start: CaptureBoundary::HistoryStart,
        join_wrapped: true,
        preserve_trailing: true,
        ..CaptureOptions::default()
    };
    let expected = terminal.capture(options).unwrap();
    let history = terminal.history(0, 10).unwrap();
    let _round_trips = forbid_actor_round_trips();
    assert_eq!(
        finish(terminal.capture_request(options, Arc::new(|| {})))
            .unwrap()
            .unwrap(),
        expected
    );
    let captured = finish(terminal.history_request(0, 10, Arc::new(|| {})))
        .unwrap()
        .unwrap();
    assert_eq!(captured.0, history.0);
    assert_eq!(captured.1, history.1);
    assert_eq!(captured.2, history.2);
    assert_eq!(captured.3, history.3);
    assert_eq!(captured.4, history.4);
}

#[test]
fn asynchronous_semantic_and_pointer_reads_use_the_actor_owned_grid() {
    let terminal =
        TerminalSession::spawn_empty_with_appearance(64, Arc::new(TerminalAppearance::default()));
    assert!(terminal.feed(Arc::from(b"\x1b]133;A\x07$ \x1b]133;B\x07echo answer\x1b]133;C\x07\r\nanswer\r\n\x1b]133;D;0\x07\x1b]133;A\x07$ ".as_slice())));
    let expected = terminal.capture_last_command().unwrap();
    let pointer = terminal.pointer_context(TerminalViewId(0), 3, 0).unwrap();
    let _round_trips = forbid_actor_round_trips();
    let captured = finish(terminal.capture_last_command_request(Arc::new(|| {})))
        .unwrap()
        .unwrap();
    assert_eq!(captured.command, expected.command);
    assert_eq!(captured.output, expected.output);
    assert_eq!(
        finish(terminal.pointer_context_request(TerminalViewId(0), 3, 0, Arc::new(|| {}))).unwrap(),
        pointer
    );
}

#[test]
fn asynchronous_kitty_reads_keep_pixel_and_storage_generations_together() {
    let terminal =
        TerminalSession::spawn_empty_with_appearance(64, Arc::new(TerminalAppearance::default()));
    let _round_trips = forbid_actor_round_trips();
    assert!(terminal.feed(Arc::from(
        b"\x1b_Ga=T,f=24,s=1,v=1,i=77;/wAA\x1b\\".as_slice()
    )));
    let image = finish(terminal.kitty_image_request(77, Arc::new(|| {})))
        .unwrap()
        .unwrap();
    assert_eq!(image.bgra, [0, 0, 255, 255]);
    assert_eq!(
        finish(terminal.kitty_image_generation_request(77, Arc::new(|| {}))).unwrap(),
        Some(image.generation)
    );
    assert!(terminal.feed(Arc::from(
        b"\x1b_Ga=T,f=24,s=1,v=1,i=77;AP8A\x1b\\".as_slice()
    )));
    let replacement = finish(terminal.kitty_image_request(77, Arc::new(|| {})))
        .unwrap()
        .unwrap();
    assert_ne!(replacement.generation, image.generation);
    assert_eq!(replacement.bgra, [0, 255, 0, 255]);
    assert_eq!(
        finish(terminal.kitty_image_generation_request(77, Arc::new(|| {}))).unwrap(),
        Some(replacement.generation)
    );
}
