use super::ctrl_test_support::{compact_registered, reliable_children};
use super::*;

#[test]
fn fanout_keeps_one_shared_encoded_frame() {
    let first = OutboundMailbox::new();
    let second = OutboundMailbox::new();
    let frame: Arc<[u8]> = Arc::from(
        zz_protocol::encode_protocol_message(&ProtocolMessage::TreeSync).expect("encode"),
    );
    assert!(first.enqueue_encoded_reliable(Arc::clone(&frame)));
    assert!(second.enqueue_encoded_reliable(Arc::clone(&frame)));
    let first = first.state.lock();
    let second = second.state.lock();
    let (OutboundFrame::Shared(left), OutboundFrame::Shared(right)) =
        (&first.reliable[0], &second.reliable[0])
    else {
        panic!("fanout copied its shared bytes")
    };
    assert!(Arc::ptr_eq(left, right));
}

#[test]
fn scoped_tree_group_keeps_shared_children_through_flush_and_writer_completion() {
    let shared = Arc::new(Shared::new(53));
    let mut context = ExecutionContext::default();
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "shared-tree"]),
        )
        .expect("model session");
    let subscriptions = zz_protocol::Subscriptions {
        tree: zz_protocol::TreeSubscription::All,
        status: false,
        options: 0,
        keys: zz_protocol::KeySubscription::None,
        pane_stream: false,
    };
    let (first_client, first) = compact_registered(&shared, subscriptions);
    let (second_client, second) = compact_registered(&shared, subscriptions);
    for (client, mailbox) in [(first_client, &first), (second_client, &second)] {
        shared.send_compact_state(client, mailbox, true);
        reliable_children(mailbox);
    }
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("rename-session", ["-t", "shared-tree", "renamed"]),
        )
        .expect("rename session");
    shared.publish_compact_trees();
    let children = [&first, &second].map(|mailbox| {
        let state = mailbox.state.lock();
        let Some(OutboundFrame::Grouped { frames, .. }) = state.reliable.front() else {
            panic!("tree publication was not grouped")
        };
        let OutboundFrame::Shared(frame) = &frames[0] else {
            panic!("tree child was copied")
        };
        Arc::clone(frame)
    });
    assert!(Arc::ptr_eq(&children[0], &children[1]));
    assert!(first.flush_control_batch(false));
    {
        let state = first.state.lock();
        let Some(OutboundFrame::Grouped { frames, .. }) = state.reliable.front() else {
            panic!("tree flush was not grouped")
        };
        let OutboundFrame::Shared(frame) = &frames[0] else {
            panic!("tree flush copied its child")
        };
        assert!(Arc::ptr_eq(frame, &children[1]));
    }
    let mut pending = Vec::new();
    for mailbox in [&first, &second] {
        assert!(mailbox.recv_batch(&mut pending, MAX_OUTBOUND_BYTES));
        let ProtocolMessage::Batch(batch) =
            zz_protocol::decode_protocol_frame(&pending[0]).expect("decode tree batch")
        else {
            panic!("missing tree batch")
        };
        assert!(matches!(
            batch.messages().expect("flat tree").as_slice(),
            [ProtocolMessage::Event(Event {
                payload: EventPayload::TreeDelta(_) | EventPayload::Snapshot(_),
                ..
            })]
        ));
        mailbox.finish_batch(&mut pending);
        let state = mailbox.state.lock();
        assert_eq!(state.recycled_frames.len(), 1);
        assert!(state.recycled_capacity <= MAX_RECYCLED_FRAME_CAPACITY);
    }
    assert_eq!(Arc::strong_count(&children[0]), 2);
}

#[test]
fn compact_batch_flush_keeps_existing_groups_flat() {
    let mailbox = OutboundMailbox::new();
    let buffer = Vec::with_capacity(4096);
    let allocation = buffer.as_ptr();
    mailbox.recycle_frame(buffer);
    let child = zz_protocol::encode_protocol_message(&ProtocolMessage::TreeSync).expect("encode");
    assert!(mailbox.enqueue_control_group(vec![child.into()]));
    assert!(mailbox.flush_control_batch(false));
    assert!(mailbox.flush_control_batch(false));
    assert!(matches!(
        mailbox.state.lock().reliable.front(),
        Some(OutboundFrame::Grouped { encoded, .. }) if encoded.as_ptr() == allocation
    ));
    let messages = tests::take_reliable_messages(&mailbox);
    let [ProtocolMessage::Batch(batch)] = messages.as_slice() else {
        panic!("expected one flat batch: {messages:?}")
    };
    assert!(matches!(
        batch.messages().expect("flat group").as_slice(),
        [ProtocolMessage::TreeSync]
    ));
}

#[test]
fn completed_group_reuses_its_buffers_after_the_writer_finishes() {
    let mailbox = OutboundMailbox::new();
    let outer = Vec::with_capacity(4096);
    let outer_capacity = outer.capacity();
    mailbox.recycle_frame(outer);
    let mut child = Vec::with_capacity(512);
    zz_protocol::encode_protocol_message_into(&ProtocolMessage::TreeSync, &mut child)
        .expect("encode child");
    let allocation = child.as_ptr();
    let child_capacity = child.capacity();
    assert!(mailbox.enqueue_control_group(vec![child.into()]));
    let mut frames = Vec::new();
    assert!(mailbox.recv_batch(&mut frames, MAX_OUTBOUND_BYTES));
    let wire_bytes = frames[0].len();
    {
        let state = mailbox.state.lock();
        assert_eq!(state.queued_bytes, 0);
        assert_eq!(state.written_bytes, 0);
        assert!(state.recycled_frames.is_empty());
    }
    let ProtocolMessage::Batch(batch) =
        zz_protocol::decode_protocol_frame(&frames[0]).expect("decode group")
    else {
        panic!("expected written group")
    };
    assert_eq!(
        batch.messages().expect("flat group"),
        [ProtocolMessage::TreeSync]
    );
    mailbox.finish_batch(&mut frames);
    assert!(frames.is_empty());
    {
        let state = mailbox.state.lock();
        assert_eq!(state.written_bytes, wire_bytes as u64);
        assert_eq!(state.recycled_frames.len(), 2);
        assert_eq!(state.recycled_capacity, outer_capacity + child_capacity);
    }
    let reused = mailbox
        .encode_message(&ProtocolMessage::TreeSync)
        .expect("reuse child");
    assert_eq!(reused.as_ptr(), allocation);
    assert_eq!(
        zz_protocol::decode_protocol_frame(&reused).expect("decode reused"),
        ProtocolMessage::TreeSync
    );
    assert_eq!(mailbox.state.lock().recycled_capacity, outer_capacity);
}

#[test]
fn grouped_buffer_recycling_keeps_limits_and_shared_ownership() {
    let mailbox = OutboundMailbox::new();
    let shared: Arc<[u8]> = Arc::from([1_u8, 2]);
    {
        let mut state = mailbox.state.lock();
        recycle_outbound_frame(&mut state, Arc::clone(&shared));
        assert_eq!(Arc::strong_count(&shared), 1);
        assert!(state.recycled_frames.is_empty());
        recycle_outbound_frame(
            &mut state,
            OutboundFrame::Grouped {
                encoded: Vec::with_capacity(MAX_RECYCLED_FRAME_CAPACITY + 1),
                frames: (0..=MAX_RECYCLED_FRAME_BUFFERS)
                    .map(|_| Vec::with_capacity(16).into())
                    .collect(),
            },
        );
        assert_eq!(state.recycled_frames.len(), MAX_RECYCLED_FRAME_BUFFERS);
        assert_eq!(state.recycled_capacity, MAX_RECYCLED_FRAME_BUFFERS * 16);
    }
    let mut bounded = OutboundState::default();
    recycle_outbound_frame(
        &mut bounded,
        OutboundFrame::Grouped {
            encoded: Vec::with_capacity(MAX_RECYCLED_FRAME_CAPACITY / 2),
            frames: vec![
                Vec::with_capacity(MAX_RECYCLED_FRAME_CAPACITY / 2).into(),
                Vec::with_capacity(MAX_RECYCLED_FRAME_CAPACITY / 2).into(),
            ],
        },
    );
    assert_eq!(bounded.recycled_frames.len(), 2);
    assert_eq!(bounded.recycled_capacity, MAX_RECYCLED_FRAME_CAPACITY);
    mailbox.close();
    let mut state = mailbox.state.lock();
    recycle_outbound_frame(
        &mut state,
        OutboundFrame::Grouped {
            encoded: Vec::with_capacity(16),
            frames: vec![Vec::with_capacity(16).into()],
        },
    );
    assert!(state.recycled_frames.is_empty());
    assert_eq!(state.recycled_capacity, 0);
}

#[test]
fn quiet_group_retains_its_children_without_an_intermediate_encoding() {
    let mailbox = OutboundMailbox::new();
    let buffer = Vec::with_capacity(4096);
    let allocation = buffer.as_ptr();
    mailbox.recycle_frame(buffer);
    let child = zz_protocol::encode_protocol_message(&ProtocolMessage::TreeSync).expect("child");
    let shared: Arc<[u8]> = Arc::from(child.clone());
    assert!(mailbox.collect_control_query());
    assert!(mailbox.enqueue_control_group(vec![child.into(), Arc::clone(&shared).into()]));
    {
        let mut state = mailbox.state.lock();
        let Some(OutboundFrame::DeferredGrouped {
            sequence,
            encoded_len,
            frames,
        }) = state.reliable.front()
        else {
            panic!("quiet group was encoded before collection")
        };
        let mut encoded = Vec::new();
        zz_protocol::encode_batch_frames_into(*sequence, frames, &mut encoded).expect("wire");
        assert_eq!(*encoded_len, encoded.len());
        assert_eq!(state.queued_bytes, encoded.len());
        assert_eq!(state.reliable.len(), 1);
        assert_eq!(state.recycled_frames.len(), 1);
        let OutboundFrame::Shared(retained) = &frames[1] else {
            panic!("shared child copied")
        };
        assert!(Arc::ptr_eq(retained, &shared));
        assert!(pop_ready_frame(&mut state).is_none());
    }
    mailbox.release_control_query();
    let mut pending = Vec::new();
    assert!(mailbox.recv_batch(&mut pending, MAX_OUTBOUND_BYTES));
    let OutboundFrame::Grouped { encoded, frames } = &pending[0] else {
        panic!("group missing")
    };
    assert_eq!(encoded.as_ptr(), allocation);
    let OutboundFrame::Shared(retained) = &frames[1] else {
        panic!("shared child copied on flush")
    };
    assert!(Arc::ptr_eq(retained, &shared));
    let ProtocolMessage::Batch(batch) = zz_protocol::decode_protocol_frame(encoded).expect("batch")
    else {
        panic!("batch missing")
    };
    assert_eq!(
        batch.messages().expect("flat group"),
        [ProtocolMessage::TreeSync, ProtocolMessage::TreeSync]
    );
    assert_eq!(Arc::strong_count(&shared), 2);
    assert!(mailbox.state.lock().recycled_frames.is_empty());
    mailbox.finish_batch(&mut pending);
    assert_eq!(Arc::strong_count(&shared), 1);
    assert_eq!(mailbox.state.lock().recycled_frames.len(), 2);
}

#[test]
fn buffered_groups_and_direct_drains_keep_their_original_wire() {
    let child = zz_protocol::encode_protocol_message(&ProtocolMessage::TreeSync).expect("child");
    let mailbox = OutboundMailbox::buffered();
    assert!(mailbox.collect_control_query());
    assert!(mailbox.enqueue_control_group(vec![child.clone().into()]));
    assert!(matches!(
        mailbox.state.lock().reliable.front(),
        Some(OutboundFrame::Grouped { .. })
    ));
    let mut output = Vec::new();
    mailbox.drain_reliable_into(&mut output);
    let ProtocolMessage::Batch(batch) =
        zz_protocol::decode_protocol_frame(&output).expect("buffered group")
    else {
        panic!("batch missing")
    };
    assert_eq!(
        batch.messages().expect("flat buffered group"),
        [ProtocolMessage::TreeSync]
    );

    let mailbox = OutboundMailbox::new();
    assert!(mailbox.collect_control_query());
    assert!(mailbox.enqueue_control_group(vec![child.clone().into()]));
    let sequence = match mailbox.state.lock().reliable.front() {
        Some(OutboundFrame::DeferredGrouped { sequence, .. }) => *sequence,
        _ => panic!("group was not deferred"),
    };
    let expected =
        zz_protocol::encode_protocol_message(&ProtocolMessage::Batch(zz_protocol::Batch {
            sequence,
            frames: vec![child.clone()],
        }))
        .expect("expected drain");
    let mut output = Vec::new();
    mailbox.drain_reliable_into(&mut output);
    assert_eq!(output, expected);
    assert_eq!(mailbox.state.lock().queued_bytes, 0);
    assert_eq!(mailbox.state.lock().recycled_frames.len(), 2);

    assert!(mailbox.enqueue_control_group(vec![child.into()]));
    let messages = tests::take_reliable_messages(&mailbox);
    assert!(matches!(messages.as_slice(), [ProtocolMessage::Batch(_)]));
    assert_eq!(mailbox.state.lock().queued_bytes, 0);
}

#[test]
fn deferred_groups_keep_encoded_byte_and_message_admission_limits() {
    let child = zz_protocol::encode_protocol_message(&ProtocolMessage::TreeSync).expect("child");
    let encoded =
        zz_protocol::encode_protocol_message(&ProtocolMessage::Batch(zz_protocol::Batch {
            sequence: 128,
            frames: vec![child.clone(), child.clone()],
        }))
        .expect("group");
    for (queued_bytes, reliable_count, admitted) in [
        (
            MAX_OUTBOUND_BYTES - encoded.len(),
            MAX_RELIABLE_MESSAGES - 1,
            true,
        ),
        (MAX_OUTBOUND_BYTES - encoded.len() + 1, 0, false),
        (0, MAX_RELIABLE_MESSAGES, false),
    ] {
        for deferred in [false, true] {
            let mailbox = OutboundMailbox::new();
            assert!(mailbox.collect_control_query());
            {
                let mut state = mailbox.state.lock();
                state.queued_bytes = queued_bytes;
                state
                    .reliable
                    .extend((0..reliable_count).map(|_| child.clone().into()));
            }
            let frame = if deferred {
                OutboundFrame::DeferredGrouped {
                    sequence: 128,
                    encoded_len: zz_protocol::batch_frames_encoded_len(128, &[&child, &child])
                        .expect("length"),
                    frames: vec![child.clone().into(), child.clone().into()],
                }
            } else {
                OutboundFrame::Grouped {
                    encoded: encoded.clone(),
                    frames: vec![child.clone().into(), child.clone().into()],
                }
            };
            assert_eq!(mailbox.enqueue_encoded_reliable(frame), admitted);
            let mut state = mailbox.state.lock();
            if admitted {
                assert_eq!(state.queued_bytes, queued_bytes + encoded.len());
                assert_eq!(state.reliable.len(), reliable_count + 1);
                assert!(!state.closed);
            } else {
                assert!(state.closed);
                assert_eq!(state.reliable.len(), 1);
                let reason = pop_ready_frame(&mut state).expect("overflow reason");
                assert!(
                    matches!(zz_protocol::decode_protocol_frame(&reason).expect("reason"),
                    ProtocolMessage::Event(Event { payload: EventPayload::ControlExit { reason }, .. }) if reason == "too far behind")
                );
            }
        }
    }
}
