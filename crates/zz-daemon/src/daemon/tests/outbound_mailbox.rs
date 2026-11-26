use super::*;

#[test]
fn outbound_mailbox_promotes_overlapping_terminal_updates() {
    let mailbox = OutboundMailbox::new();
    let pane = PaneId(9);
    let first = terminal_test_message(pane, 1, 1);
    let second = terminal_test_message(pane, 2, 2);

    assert_eq!(
        mailbox.enqueue_terminal(pane, &first),
        TerminalEnqueue::Queued
    );
    assert_eq!(
        mailbox.enqueue_terminal(pane, &second),
        TerminalEnqueue::NeedsFull
    );
    assert!(mailbox.replace_terminal(pane, &second));

    let encoded = mailbox.recv().expect("coalesced terminal update");
    assert_eq!(decode_protocol_frame(&encoded).expect("decode"), second);
    mailbox.close();
    assert!(mailbox.recv().is_none());
}

#[test]
fn outbound_mailbox_keeps_the_newest_full_frame_when_an_older_one_lands_late() {
    let mailbox = OutboundMailbox::new();
    let pane = PaneId(9);
    let newer = terminal_test_message(pane, 1, 2);
    let older = terminal_test_message(pane, 2, 1);

    assert_eq!(
        mailbox.enqueue_terminal(pane, &newer),
        TerminalEnqueue::Queued
    );
    let encoded = mailbox.recv().expect("newer frame");
    assert_eq!(decode_protocol_frame(&encoded).expect("decode"), newer);

    assert!(!mailbox.replace_terminal(pane, &older));
    assert_eq!(
        mailbox.enqueue_terminal(pane, &older),
        TerminalEnqueue::Dropped
    );
    assert!(mailbox.state.lock().terminals.is_empty());
    assert_eq!(
        mailbox.enqueue_terminal(pane, &terminal_patch_test_message(pane, 3, 2, 3)),
        TerminalEnqueue::Queued,
        "the client still holds generation 2, so a patch from it applies"
    );
    assert!(mailbox.recv().is_some());

    let pending = terminal_test_message(pane, 4, 5);
    assert!(mailbox.replace_terminal(pane, &pending));
    assert!(!mailbox.replace_terminal(pane, &terminal_test_message(pane, 5, 4)));
    let encoded = mailbox.recv().expect("pending frame");
    assert_eq!(decode_protocol_frame(&encoded).expect("decode"), pending);
    mailbox.close();
}

#[test]
fn outbound_mailbox_drops_preview_pressure_before_foreground() {
    let mailbox = OutboundMailbox::new();
    let oversized = terminal_test_message(PaneId(1), 1, 1);
    let transition = terminal_transition(PaneId(1), &oversized).expect("terminal transition");
    assert_eq!(
        mailbox.enqueue_terminal_with(
            PaneId(1),
            transition,
            TerminalDelivery::Preview {
                foreground_panes: 1,
            },
            || Ok(Arc::from(vec![0; MAX_PREVIEW_OUTBOUND_BYTES + 1])),
        ),
        TerminalEnqueue::Dropped
    );
    assert!(!mailbox.state.lock().closed);

    for index in 0..MAX_PENDING_TERMINALS - 1 {
        let pane = PaneId(u64::try_from(index + 10).unwrap());
        let message = terminal_test_message(pane, 2, 2);
        assert_eq!(
            mailbox.enqueue_terminal_preview(pane, &message, 1),
            TerminalEnqueue::Queued
        );
    }
    let rejected = PaneId(10_000);
    assert_eq!(
        mailbox.enqueue_terminal_preview(rejected, &terminal_test_message(rejected, 3, 3), 1,),
        TerminalEnqueue::Dropped
    );
    let foreground = PaneId(20_000);
    assert_eq!(
        mailbox.enqueue_terminal(foreground, &terminal_test_message(foreground, 4, 4),),
        TerminalEnqueue::Queued
    );
    let state = mailbox.state.lock();
    assert_eq!(state.terminals.len(), MAX_PENDING_TERMINALS);
    assert!(state.terminals.contains_key(&foreground));
    assert!(state.queued_bytes <= MAX_OUTBOUND_BYTES);
    assert!(!state.closed);
}

#[test]
fn outbound_mailbox_delivers_kitty_pixels_before_viewports_and_resets_per_pane() {
    let mailbox = OutboundMailbox::new();
    let pane = PaneId(9);
    let image_id = 4;
    let generation = 2;
    let controls = [
        ProtocolMessage::Event(Event {
            sequence: 1,
            payload: EventPayload::KittyImageBegin {
                pane,
                image_id,
                generation,
                width: 1,
                height: 1,
                total_bytes: 4,
            },
        }),
        ProtocolMessage::Event(Event {
            sequence: 2,
            payload: EventPayload::KittyImageChunk {
                pane,
                image_id,
                generation,
                bytes: vec![1, 2, 3, 4],
            },
        }),
    ];
    let frames = controls
        .iter()
        .map(|message| encode_protocol_message(message).expect("encode Kitty control"))
        .collect::<Vec<_>>();
    assert_eq!(
        mailbox.enqueue_kitty_image(pane, image_id, generation, &frames),
        KittyImageEnqueue::Queued
    );
    let viewport = terminal_test_message(pane, 3, 1);
    assert_eq!(
        mailbox.enqueue_terminal(pane, &viewport),
        TerminalEnqueue::Queued
    );

    for expected in &controls {
        let frame = mailbox.recv().expect("reliable Kitty frame");
        assert_eq!(
            decode_protocol_frame(&frame).expect("decode Kitty"),
            *expected
        );
    }
    let frame = mailbox.recv().expect("viewport after Kitty pixels");
    assert_eq!(
        decode_protocol_frame(&frame).expect("decode viewport"),
        viewport
    );
    assert_eq!(
        mailbox.state.lock().delivered_images[&pane][&image_id],
        generation
    );

    assert_eq!(
        mailbox.enqueue_kitty_image(pane, image_id, generation, &frames),
        KittyImageEnqueue::AlreadyDelivered
    );
    assert!(mailbox.state.lock().reliable.is_empty());
    mailbox.reset_kitty_images();
    assert!(!mailbox.state.lock().delivered_images.contains_key(&pane));
    assert_eq!(
        mailbox.enqueue_kitty_image(pane, image_id, generation, &frames),
        KittyImageEnqueue::Queued
    );
    assert_eq!(take_reliable_messages(&mailbox).len(), 2);

    mailbox.suspend_terminal(pane);
    assert_eq!(
        mailbox.state.lock().delivered_images[&pane][&image_id],
        generation
    );
    assert_eq!(
        mailbox.enqueue_kitty_image(pane, image_id, generation, &frames),
        KittyImageEnqueue::AlreadyDelivered
    );
    assert!(mailbox.state.lock().reliable.is_empty());

    mailbox.cancel_terminal(pane);
    assert!(!mailbox.state.lock().delivered_images.contains_key(&pane));
    assert_eq!(
        mailbox.enqueue_kitty_image(pane, image_id, generation, &frames),
        KittyImageEnqueue::Queued
    );
    assert_eq!(take_reliable_messages(&mailbox).len(), 2);

    mailbox.enqueue_kitty_images_removed(pane, &[image_id]);
    assert!(matches!(
        take_reliable_messages(&mailbox).as_slice(),
        [ProtocolMessage::Event(Event {
            payload: EventPayload::KittyImagesRemoved { pane: target, image_ids },
            ..
        })] if *target == pane && image_ids == &[image_id]
    ));
    assert!(!mailbox.state.lock().delivered_images.contains_key(&pane));
}

#[test]
fn outbound_mailbox_batches_kitty_image_removals_without_losing_ids() {
    for count in [
        zz_protocol::MAX_KITTY_IMAGE_REMOVALS,
        zz_protocol::MAX_KITTY_IMAGE_REMOVALS + 1,
    ] {
        let mailbox = OutboundMailbox::new();
        let pane = PaneId(9);
        let other_pane = PaneId(10);
        let image_ids = (1..=count)
            .map(|id| u32::try_from(id).unwrap())
            .collect::<Vec<_>>();
        {
            let mut state = mailbox.state.lock();
            state
                .delivered_images
                .insert(pane, image_ids.iter().copied().map(|id| (id, 1)).collect());
            state.delivered_images.insert(other_pane, [(1, 1)].into());
        }
        let mut requested = image_ids.clone();
        requested.push(u32::MAX);
        mailbox.enqueue_kitty_images_removed(pane, &requested);
        let messages = take_reliable_messages(&mailbox);
        assert_eq!(
            messages.len(),
            count.div_ceil(zz_protocol::MAX_KITTY_IMAGE_REMOVALS)
        );
        let mut removed = Vec::new();
        let mut last_sequence = None;
        for message in messages {
            let ProtocolMessage::Event(Event {
                sequence,
                payload:
                    EventPayload::KittyImagesRemoved {
                        pane: target,
                        image_ids,
                    },
            }) = message
            else {
                panic!("expected Kitty image removals");
            };
            assert_eq!(target, pane);
            assert!(image_ids.len() <= zz_protocol::MAX_KITTY_IMAGE_REMOVALS);
            assert!(last_sequence.is_none_or(|previous| previous < sequence));
            last_sequence = Some(sequence);
            removed.extend(image_ids);
        }
        assert_eq!(removed, image_ids);
        let state = mailbox.state.lock();
        assert!(!state.delivered_images.contains_key(&pane));
        assert_eq!(state.delivered_images[&other_pane][&1], 1);
        assert!(!state.closed);
    }
}

#[test]
fn pasted_image_fetch_uses_encoded_frames_and_a_per_mailbox_delivery_ledger() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let pane = {
        let mut inner = shared.inner.lock();
        let (session, _, pane) = inner
            .engine
            .state
            .create_session("pasted-image-fetch")
            .expect("session");
        inner
            .attachment
            .attached
            .entry(session)
            .or_default()
            .insert(client);
        pane
    };
    let number = 4;
    let token = 17;
    let bytes = Arc::<[u8]>::from(vec![0x89, 0x50, 0x4e, 0x47]);
    {
        let mut panes = shared.pasted_images.lock();
        let images = panes.entry(pane).or_default();
        images.stored_bytes = bytes.len();
        images.order.push_back((number, token));
        images.images.insert(
            number,
            StoredPastedImage {
                token,
                format: PastedImageFormat::Png,
                bytes: Arc::clone(&bytes),
                frames: None,
            },
        );
    }

    shared.fetch_pasted_image(client, pane, number);
    assert_eq!(
        take_reliable_messages(&mailbox),
        vec![
            ProtocolMessage::PastedImageBegin {
                pane,
                number,
                format: PastedImageFormat::Png,
                total_bytes: 4,
            },
            ProtocolMessage::PastedImageChunk {
                pane,
                number,
                bytes: bytes.to_vec(),
            },
        ]
    );
    assert!(
        shared.pasted_images.lock()[&pane].images[&number]
            .frames
            .is_some()
    );
    shared.fetch_pasted_image(client, pane, number);
    assert!(take_reliable_messages(&mailbox).is_empty());

    mailbox.cancel_terminal(pane);
    shared.fetch_pasted_image(client, pane, number);
    assert_eq!(take_reliable_messages(&mailbox).len(), 2);
    shared.fetch_pasted_image(client, pane, number + 1);
    assert_eq!(
        take_reliable_messages(&mailbox),
        vec![ProtocolMessage::PastedImageUnavailable {
            pane,
            number: number + 1,
        }]
    );
}

#[test]
fn pasted_image_store_caps_pending_and_bound_bytes_together() {
    let mut images = PanePastedImages::default();
    let bytes = Arc::<[u8]>::from(vec![0; 4 * 1024 * 1024]);
    for token in 1..=6 {
        let admission = images.push_pending(PendingPastedImage {
            token,
            format: PastedImageFormat::Png,
            bytes: Arc::clone(&bytes),
        });
        assert!(admission.evicted_numbers.is_empty());
        assert!(admission.retained);
        assert_eq!(
            images.bind(token, u32::try_from(token).unwrap()),
            Some(Vec::new())
        );
    }
    assert_eq!(images.stored_bytes, MAX_PASTED_IMAGE_BYTES_PER_PANE);

    let admission = images.push_pending(PendingPastedImage {
        token: 7,
        format: PastedImageFormat::Png,
        bytes,
    });
    assert_eq!(admission.evicted_numbers, [1]);
    assert!(admission.retained);
    assert!(!images.images.contains_key(&1));
    assert_eq!(images.pending.front().map(|pending| pending.token), Some(7));
    assert_eq!(
        images.stored_bytes + images.pending_bytes,
        MAX_PASTED_IMAGE_BYTES_PER_PANE
    );
    assert_eq!(images.images.len() + images.pending.len(), 6);
}

#[test]
fn pasted_image_store_rejects_the_newest_pending_window_when_only_windows_remain() {
    let mut images = PanePastedImages::default();
    let bytes = Arc::<[u8]>::from(vec![0; 3 * 1024 * 1024]);
    for token in 1..=MAX_PASTED_IMAGES_PER_PANE as u64 {
        let admission = images.push_pending(PendingPastedImage {
            token,
            format: PastedImageFormat::Png,
            bytes: Arc::clone(&bytes),
        });
        assert!(admission.retained);
        assert!(admission.evicted_numbers.is_empty());
    }
    let admission = images.push_pending(PendingPastedImage {
        token: 99,
        format: PastedImageFormat::Png,
        bytes,
    });
    assert!(!admission.retained);
    assert!(admission.evicted_numbers.is_empty());
    assert_eq!(images.pending.len(), MAX_PASTED_IMAGES_PER_PANE);
    assert!(images.pending.iter().all(|pending| pending.token != 99));
}

#[test]
fn outbound_mailbox_reuses_completed_frame_buffers() {
    let mailbox = OutboundMailbox::new();
    let pane = PaneId(9);
    let bell = |sequence| {
        ProtocolMessage::Event(Event {
            sequence,
            payload: EventPayload::Bell { pane },
        })
    };
    let first = bell(1);
    assert!(mailbox.enqueue_reliable(&first));
    let first_frame = mailbox.recv().expect("first reliable message");
    let allocation = first_frame.as_ptr();
    let capacity = first_frame.capacity();
    assert_eq!(decode_protocol_frame(&first_frame).expect("decode"), first);
    mailbox.recycle_frame(first_frame);

    {
        let state = mailbox.state.lock();
        assert_eq!(state.recycled_frames.len(), 1);
        assert_eq!(state.recycled_capacity, capacity);
    }

    let second = bell(2);
    assert!(mailbox.enqueue_reliable(&second));
    let second_frame = mailbox.recv().expect("second reliable message");
    assert_eq!(second_frame.as_ptr(), allocation);
    assert_eq!(second_frame.capacity(), capacity);
    assert_eq!(
        decode_protocol_frame(&second_frame).expect("decode"),
        second
    );
}

#[test]
fn outbound_mailbox_bounds_recycled_frame_storage() {
    let mailbox = OutboundMailbox::new();
    mailbox.recycle_frame(Vec::with_capacity(MAX_RECYCLED_FRAME_CAPACITY + 1));
    assert!(mailbox.state.lock().recycled_frames.is_empty());

    for _ in 0..=MAX_RECYCLED_FRAME_BUFFERS {
        mailbox.recycle_frame(Vec::with_capacity(16));
    }
    let state = mailbox.state.lock();
    assert_eq!(state.recycled_frames.len(), MAX_RECYCLED_FRAME_BUFFERS);
    assert_eq!(state.recycled_capacity, MAX_RECYCLED_FRAME_BUFFERS * 16);
}

#[test]
fn outbound_mailbox_promotes_a_patch_after_visibility_reset() {
    let mailbox = OutboundMailbox::new();
    let pane = PaneId(9);
    let first = terminal_test_message(pane, 1, 1);
    assert_eq!(
        mailbox.enqueue_terminal(pane, &first),
        TerminalEnqueue::Queued
    );
    let _ = mailbox.recv().expect("initial full viewport");
    mailbox.cancel_terminal(pane);

    let patch = terminal_patch_test_message(pane, 2, 1, 2);
    assert_eq!(
        mailbox.enqueue_terminal(pane, &patch),
        TerminalEnqueue::NeedsFull
    );
    let full = terminal_test_message(pane, 3, 2);
    assert_eq!(
        mailbox.enqueue_terminal(pane, &full),
        TerminalEnqueue::Queued
    );
    assert_eq!(
        decode_protocol_frame(&mailbox.recv().expect("replacement full viewport"))
            .expect("decode replacement"),
        full
    );
    mailbox.close();
}

#[test]
fn request_full_enqueues_only_the_requested_visible_pane() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "request-full", QUIET_PANE_COMMAND]),
        )
        .expect("new session");
    let session = context.session.expect("session");
    let first = context.pane.expect("first pane");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-h", QUIET_PANE_COMMAND]),
        )
        .expect("split pane");
    let second = context.pane.expect("second pane");
    shared.attach(client, session).expect("attach session");

    let deadline = Instant::now() + Duration::from_secs(30);
    for pane in [first, second] {
        loop {
            let ready = shared.inner.lock().terminals[&pane]
                .latest_viewport_for(TerminalViewId(client.0))
                .is_some();
            if ready {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "terminal {pane} did not publish its attached viewport"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    thread::sleep(Duration::from_millis(50));
    loop {
        let available = {
            let state = mailbox.state.lock();
            !state.reliable.is_empty()
                || state.command_output.is_some()
                || !state.terminals.is_empty()
        };
        if !available {
            break;
        }
        let _ = mailbox.recv().expect("available pre-request message");
    }

    let queued_fulls = || {
        let state = mailbox.state.lock();
        state
            .terminals
            .iter()
            .filter(|(_, pending)| {
                matches!(
                    decode_protocol_frame(&pending.encoded).expect("decode pending terminal frame"),
                    ProtocolMessage::Event(Event {
                        payload: EventPayload::TerminalViewport { .. },
                        ..
                    })
                )
            })
            .map(|(pane, _)| *pane)
            .collect::<Vec<_>>()
    };

    shared.request_full(client, first, &mailbox);
    assert_eq!(queued_fulls(), [first]);

    shared.request_full(client, PaneId(9_999_999), &mailbox);
    assert_eq!(queued_fulls(), [first]);
}

#[test]
fn outbound_mailbox_coalesces_command_output_and_close_cancels_it() {
    let mailbox = OutboundMailbox::new();
    let pane = PaneId(9);
    let first = command_output_test_message(pane, 7, 1, 1);
    let second = command_output_test_message(pane, 7, 2, 2);

    assert!(mailbox.replace_command_output(&first));
    assert!(mailbox.replace_command_output(&second));
    let close = ProtocolMessage::Event(Event {
        sequence: 3,
        payload: EventPayload::CommandOutput {
            output_id: 7,
            pane,
            viewport: None,
        },
    });
    assert!(mailbox.enqueue_reliable(&close));

    let encoded = mailbox.recv().expect("reliable command-output close");
    assert_eq!(decode_protocol_frame(&encoded).expect("decode"), close);
    assert!(mailbox.state.lock().command_output.is_none());
    mailbox.close();
}

#[test]
fn stale_command_output_close_preserves_the_newer_pending_frame() {
    let mailbox = OutboundMailbox::new();
    let pane = PaneId(9);
    let old = command_output_test_message(pane, 10, 1, 1);
    let current = command_output_test_message(pane, 11, 2, 2);
    assert!(mailbox.replace_command_output(&old));
    assert!(mailbox.replace_command_output(&current));
    let stale_close = ProtocolMessage::Event(Event {
        sequence: 3,
        payload: EventPayload::CommandOutput {
            output_id: 10,
            pane,
            viewport: None,
        },
    });
    assert!(mailbox.enqueue_reliable(&stale_close));

    let encoded = mailbox.recv().expect("stale command-output close");
    assert_eq!(
        decode_protocol_frame(&encoded).expect("decode"),
        stale_close
    );
    let encoded = mailbox.recv().expect("newer command-output viewport");
    assert_eq!(decode_protocol_frame(&encoded).expect("decode"), current);
    mailbox.close();
}

#[test]
fn stale_command_output_frame_preserves_the_newer_pending_frame() {
    let mailbox = OutboundMailbox::new();
    let pane = PaneId(9);
    let current = command_output_test_message(pane, 11, 2, 2);
    let stale = command_output_test_message(pane, 10, 3, 3);
    assert!(mailbox.replace_command_output(&current));
    assert!(mailbox.replace_command_output(&stale));

    let encoded = mailbox.recv().expect("newer command-output viewport");
    assert_eq!(decode_protocol_frame(&encoded).expect("decode"), current);
    mailbox.close();
}

#[test]
fn newer_command_output_close_cancels_an_older_pending_frame() {
    let mailbox = OutboundMailbox::new();
    let pane = PaneId(9);
    let old = command_output_test_message(pane, 10, 1, 1);
    assert!(mailbox.replace_command_output(&old));
    let newer_close = ProtocolMessage::Event(Event {
        sequence: 2,
        payload: EventPayload::CommandOutput {
            output_id: 11,
            pane,
            viewport: None,
        },
    });
    assert!(mailbox.enqueue_reliable(&newer_close));

    let encoded = mailbox.recv().expect("newer command-output close");
    assert_eq!(
        decode_protocol_frame(&encoded).expect("decode"),
        newer_close
    );
    assert!(mailbox.state.lock().command_output.is_none());
    mailbox.close();
}

#[test]
fn resync_skips_only_the_reliably_admitted_command_output_id() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (session, _, pane) = switch_test_session(&shared, "resync-command-output-skip");
    shared.attach(client, session).expect("attach interactive");

    shared
        .open_command_output(client, Some(pane), "first".to_owned(), "first")
        .expect("open first output");
    let admitted = command_output_message_id(&take_command_output_message(&mailbox));
    shared.send_resync_inner(client, &mailbox, Some(admitted));
    assert!(mailbox.state.lock().command_output.is_none());
    take_reliable_messages(&mailbox);

    shared
        .open_command_output(client, Some(pane), "second".to_owned(), "second")
        .expect("replace output");
    let replacement = take_command_output_message(&mailbox);
    assert_ne!(command_output_message_id(&replacement), admitted);
    take_reliable_messages(&mailbox);

    shared.send_resync_inner(client, &mailbox, Some(admitted));
    let replayed = take_command_output_message(&mailbox);
    assert_eq!(
        command_output_message_id(&replayed),
        command_output_message_id(&replacement)
    );
}

#[test]
fn startup_rollback_does_not_retire_a_replacement_command_output() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (session, _, pane) = switch_test_session(&shared, "startup-output-replacement");
    shared.attach(client, session).expect("attach interactive");
    take_reliable_messages(&mailbox);

    let startup_id = shared
        .open_startup_command_output(
            client,
            Some(pane),
            "configuration errors".to_owned(),
            "startup",
            &mailbox,
        )
        .expect("open startup output");
    shared
        .open_command_output(client, Some(pane), "replacement".to_owned(), "replacement")
        .expect("replace startup output");
    let replacement_id = shared.inner.lock().clients[&client]
        .command_output
        .as_ref()
        .unwrap()
        .output_id;
    assert_ne!(startup_id, replacement_id);

    shared.retire_command_output_if_exact(client, startup_id);
    assert_eq!(
        shared.inner.lock().clients[&client]
            .command_output
            .as_ref()
            .unwrap()
            .output_id,
        replacement_id
    );
}

#[test]
fn reliable_overflow_delivers_the_control_exit_reason_before_closing() {
    let mailbox = OutboundMailbox::new();
    for sequence in 0..MAX_RELIABLE_MESSAGES {
        assert!(mailbox.enqueue_reliable(&ProtocolMessage::Event(Event {
            sequence: u64::try_from(sequence).unwrap(),
            payload: EventPayload::ServerStopping,
        })));
    }
    assert!(!mailbox.enqueue_reliable(&ProtocolMessage::Event(Event {
        sequence: u64::try_from(MAX_RELIABLE_MESSAGES).unwrap(),
        payload: EventPayload::ServerStopping,
    })));
    let encoded = mailbox.recv().expect("control overflow exit");
    assert_eq!(
        decode_protocol_frame(&encoded).expect("decode control overflow exit"),
        ProtocolMessage::Event(Event {
            sequence: 0,
            payload: EventPayload::ControlExit {
                reason: "too far behind".to_owned(),
            },
        })
    );
    assert!(mailbox.recv().is_none());
}

#[cfg(unix)]
#[test]
fn reliable_overflow_flushes_the_reason_and_closes_the_transport() {
    let mailbox = OutboundMailbox::new();
    for sequence in 0..MAX_RELIABLE_MESSAGES {
        assert!(mailbox.enqueue_reliable(&ProtocolMessage::Event(Event {
            sequence: u64::try_from(sequence).unwrap(),
            payload: EventPayload::ServerStopping,
        })));
    }
    assert!(!mailbox.enqueue_reliable(&ProtocolMessage::Event(Event {
        sequence: u64::try_from(MAX_RELIABLE_MESSAGES).unwrap(),
        payload: EventPayload::ServerStopping,
    })));
    let (mut client, mut server) =
        std::os::unix::net::UnixStream::pair().expect("paired overflow transport");
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("overflow read timeout");
    let writer_mailbox = Arc::clone(&mailbox);
    let writer = thread::spawn(move || {
        write_outbound(&mut server, &writer_mailbox, &Weak::new(), ClientId(0));
    });

    assert_eq!(
        zz_protocol::read_protocol_message(&mut client).expect("control overflow exit"),
        ProtocolMessage::Event(Event {
            sequence: 0,
            payload: EventPayload::ControlExit {
                reason: "too far behind".to_owned(),
            },
        })
    );
    let mut byte = [0_u8; 1];
    assert_eq!(client.read(&mut byte).expect("transport EOF"), 0);
    writer.join().expect("overflow writer");
}

#[test]
fn command_output_encoding_releases_state_lock_and_revalidates_before_enqueue() {
    let shared = Shared::new(1);
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let pane = PaneId(9);
    let terminal = Arc::new(TerminalSession::spawn_output_view(
        "command output".to_owned(),
        "fixture".to_owned(),
    ));
    shared
        .inner
        .lock()
        .client_entry(client)
        .command_output
        .replace(CommandOutputSession {
            output_id: 1,
            pane,
            terminal: Arc::clone(&terminal),
            previous_key_table: None,
            parked: false,
        });
    let viewport = terminal.latest_viewport();
    let mut encoded_without_state_lock = false;

    shared.publish_command_output_with_encoder(
        client,
        pane,
        &terminal,
        &viewport,
        |subscriber, message| {
            let removed = {
                let mut inner = shared
                    .inner
                    .try_lock()
                    .expect("command-output encoding must not hold ServerState");
                encoded_without_state_lock = true;
                inner
                    .client_mut(client)
                    .and_then(|c| c.command_output.take())
            }
            .expect("current command output");
            assert!(Arc::ptr_eq(&removed.terminal, &terminal));
            subscriber.encode_message(message)
        },
    );

    assert!(encoded_without_state_lock);
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.command_output.is_none())));
    assert!(mailbox.state.lock().command_output.is_none());
}
