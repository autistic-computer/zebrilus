use std::time::{Duration, Instant};

use ala::TestAppContext;
use zz_client::scrollback::{HISTORY_REQUEST_RETRY, MAX_HISTORY_CHUNK_ROWS};
use zz_protocol::MuxOptions;
use zz_terminal::{CellWidth, PackedCell, ScrollbarState, SessionStatus};

use super::test_support::*;
use super::*;

fn retained_history_ids(retained: &RetainedTerminalViewport) -> Vec<u32> {
    retained
        .history
        .rows
        .iter()
        .map(|row| row.cells[0].glyph() - 0xe000)
        .collect()
}

fn chunk_rows(ids: &[u32]) -> Vec<Vec<PackedCell>> {
    ids.iter()
        .map(|id| vec![PackedCell::new(0xe000 + *id, 0, CellWidth::Narrow)])
        .collect()
}

fn history_backfill_debounce_fixture(
    cx: &mut ala::App,
    pane: PaneId,
) -> (
    ala::Entity<MuxClient>,
    Arc<FakeConnectedHost>,
    TerminalViewport,
) {
    let mux = cx.new(|cx| {
        MuxClient::new(
            Err(DaemonError::Thread("fixture".to_owned())),
            zz_daemon_client::default_socket_path(),
            cx,
        )
    });
    let fake = mux.update(cx, |mux, _| {
        let fake = install_fake_connection(mux, HostId::LOCAL);
        let mut options = MuxOptions::default();
        options.set(
            MuxOptionKey::HistoryTrickle,
            "600",
            zz_protocol::MuxOptionSource::RuntimeCommand,
        );
        mux.seed_core(EventPayload::MuxOptionsChanged { options });
        fake
    });
    let viewport = history_fixture_viewport(&[1_200, 1_201, 1_202], 1, 1_203, 1_200);
    mux.update(cx, |mux, cx| {
        mux.handle_message(
            HostId::LOCAL,
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: EventPayload::TerminalViewport {
                    pane,
                    viewport: viewport.clone(),
                },
            }),
            cx,
        );
    });
    assert_eq!(&*fake.history_requests.borrow(), &[(pane, 688, 512)]);
    (mux, fake, viewport)
}

fn apply_history_output_scroll(
    mux: &ala::Entity<MuxClient>,
    cx: &mut ala::App,
    pane: PaneId,
    previous: &TerminalViewport,
    generation: u64,
) -> TerminalViewport {
    let generation_u32 = u32::try_from(generation).expect("small fixture generation");
    let first = 1_199 + generation_u32;
    let next =
        history_fixture_viewport(&[first, first + 1, first + 2], generation, first + 3, first);
    let patch = TerminalViewport::diff(previous, &next).expect("compatible output scroll");
    assert_eq!(patch.scroll, -1);
    mux.update(cx, |mux, cx| {
        mux.handle_message(
            HostId::LOCAL,
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: generation,
                payload: EventPayload::TerminalPatch { pane, patch },
            }),
            cx,
        );
    });
    next
}

fn defer_initial_history_backfill(
    mux: &ala::Entity<MuxClient>,
    cx: &mut ala::App,
    pane: PaneId,
    initial: &TerminalViewport,
) -> TerminalViewport {
    let current = apply_history_output_scroll(mux, cx, pane, initial, 2);
    mux.update(cx, |mux, cx| {
        mux.handle_message(
            HostId::LOCAL,
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 3,
                payload: EventPayload::HistoryChunk {
                    pane,
                    start: 688,
                    total: 1_203,
                    offset: 1_200,
                    columns: 1,
                    rows: chunk_rows(&vec![0; 512]),
                    dictionary: initial.dictionary.as_ref().clone(),
                },
            }),
            cx,
        );
    });

    let mux = mux.read(cx);
    assert_eq!(
        retained_history_ids(&mux.viewports[&pane].read()),
        vec![1_200]
    );
    assert_eq!(mux.attached_connection().history.deferred(pane), Some(1));
    current
}

#[ala::test]
fn discarded_history_backfill_defers_and_suppresses_patch_retries(cx: &mut TestAppContext) {
    let pane = PaneId(74);
    let (mux, fake, initial) = cx.update(|cx| history_backfill_debounce_fixture(cx, pane));

    cx.update(|cx| {
        let current = defer_initial_history_backfill(&mux, cx, pane, &initial);
        apply_history_output_scroll(&mux, cx, pane, &current, 3);

        let mux = mux.read(cx);
        let retained = mux.viewports[&pane].read();
        assert_eq!(retained_history_ids(&retained), vec![1_200, 1_201]);
        assert_eq!(retained.history_mutations, 2);
        assert_eq!(mux.attached_connection().history.deferred(pane), Some(1));
    });

    assert_eq!(&*fake.history_requests.borrow(), &[(pane, 688, 512)]);
}

#[ala::test]
fn quiet_history_backfill_debounce_resumes_and_applies_chunk(cx: &mut TestAppContext) {
    let pane = PaneId(75);
    let (mux, fake, initial) = cx.update(|cx| history_backfill_debounce_fixture(cx, pane));
    let current = cx.update(|cx| defer_initial_history_backfill(&mux, cx, pane, &initial));

    cx.run_until_parked();
    cx.executor()
        .advance_clock(HISTORY_BACKFILL_QUIET + Duration::from_millis(1));
    cx.run_until_parked();

    assert_eq!(
        &*fake.history_requests.borrow(),
        &[(pane, 688, 512), (pane, 688, 512)]
    );
    cx.update(|cx| {
        assert!(
            mux.read(cx)
                .attached_connection()
                .history
                .deferred(pane)
                .is_none()
        );
        let ids = (688..1_200).collect::<Vec<_>>();
        mux.update(cx, |mux, cx| {
            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 4,
                    payload: EventPayload::HistoryChunk {
                        pane,
                        start: 688,
                        total: 1_204,
                        offset: 1_201,
                        columns: 1,
                        rows: chunk_rows(&ids),
                        dictionary: current.dictionary.as_ref().clone(),
                    },
                }),
                cx,
            );
        });

        let mux = mux.read(cx);
        let retained = mux.viewports[&pane].read();
        let retained_ids = retained_history_ids(&retained);
        assert_eq!(retained_ids.len(), 513);
        assert_eq!(retained_ids.first(), Some(&688));
        assert_eq!(retained_ids.last(), Some(&1_200));
    });
}

#[ala::test]
fn history_backfill_debounce_rearms_while_mutations_continue(cx: &mut TestAppContext) {
    let pane = PaneId(76);
    let (mux, fake, initial) = cx.update(|cx| history_backfill_debounce_fixture(cx, pane));
    let current = cx.update(|cx| defer_initial_history_backfill(&mux, cx, pane, &initial));
    cx.run_until_parked();

    let current = cx.update(|cx| apply_history_output_scroll(&mux, cx, pane, &current, 3));
    cx.executor()
        .advance_clock(HISTORY_BACKFILL_QUIET + Duration::from_millis(1));
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            mux.read(cx).attached_connection().history.deferred(pane),
            Some(2)
        );
    });
    assert_eq!(fake.history_requests.borrow().len(), 1);

    cx.update(|cx| {
        apply_history_output_scroll(&mux, cx, pane, &current, 4);
    });
    cx.executor()
        .advance_clock(HISTORY_BACKFILL_QUIET + Duration::from_millis(1));
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(
            mux.read(cx).attached_connection().history.deferred(pane),
            Some(3)
        );
    });
    assert_eq!(fake.history_requests.borrow().len(), 1);

    cx.executor()
        .advance_clock(HISTORY_BACKFILL_QUIET + Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(
        &*fake.history_requests.borrow(),
        &[(pane, 688, 512), (pane, 688, 512)]
    );
}

#[ala::test]
fn a_batch_keeps_terminal_patches_on_the_retained_history_path(cx: &mut TestAppContext) {
    let pane = PaneId(78);
    let (mux, _, initial) = cx.update(|cx| history_backfill_debounce_fixture(cx, pane));
    let next = history_fixture_viewport(&[1_201, 1_202, 1_203], 2, 1_204, 1_201);
    let patch = TerminalViewport::diff(&initial, &next).unwrap();
    let batch = zz_protocol::Batch::from_messages(
        2,
        [
            ProtocolMessage::Event(Event {
                sequence: 2,
                payload: EventPayload::TerminalPatch { pane, patch },
            }),
            ProtocolMessage::Event(Event {
                sequence: 3,
                payload: EventPayload::StatusChanged {
                    status: StatusLine::default(),
                },
            }),
        ],
    )
    .unwrap();
    cx.update(|cx| {
        mux.update(cx, |mux, cx| {
            mux.handle_message(HostId::LOCAL, ProtocolMessage::Batch(batch), cx);
            let retained = mux.viewports[&pane].read();
            assert_eq!(retained.viewport, next);
            assert_eq!(retained_history_ids(&retained), vec![1_200]);
            assert!(mux.core.viewport(pane).is_none());
        });
    });
}

#[ala::test]
fn a_tree_change_keeps_the_history_chunk_in_flight(cx: &mut TestAppContext) {
    let pane = PaneId(78);
    let (mux, fake, initial) = cx.update(|cx| history_backfill_debounce_fixture(cx, pane));
    cx.update(|cx| {
        mux.update(cx, |mux, cx| {
            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 2,
                    payload: EventPayload::Snapshot(MuxSnapshot {
                        generation: 5,
                        ..MuxSnapshot::default()
                    }),
                }),
                cx,
            );
            assert!(mux.attached_connection().history.pending(pane).is_some());
        });
    });
    assert_eq!(&*fake.history_requests.borrow(), &[(pane, 688, 512)]);

    let ids = (688..1_200).collect::<Vec<_>>();
    cx.update(|cx| {
        mux.update(cx, |mux, cx| {
            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 3,
                    payload: EventPayload::HistoryChunk {
                        pane,
                        start: 688,
                        total: 1_203,
                        offset: 1_200,
                        columns: 1,
                        rows: chunk_rows(&ids),
                        dictionary: initial.dictionary.as_ref().clone(),
                    },
                }),
                cx,
            );
        });
        let mux = mux.read(cx);
        let retained_ids = retained_history_ids(&mux.viewports[&pane].read());
        assert_eq!(retained_ids.len(), 512);
        assert_eq!(retained_ids.first(), Some(&688));
    });
}

#[ala::test]
fn a_history_reply_that_never_came_is_requested_again(cx: &mut TestAppContext) {
    let pane = PaneId(79);
    let (mux, fake, _) = cx.update(|cx| history_backfill_debounce_fixture(cx, pane));
    cx.update(|cx| {
        mux.update(cx, |mux, _| {
            mux.request_history_backfill(pane);
            assert_eq!(fake.history_requests.borrow().len(), 1);
            mux.request_history(
                pane,
                None,
                Instant::now() + HISTORY_REQUEST_RETRY + Duration::from_millis(1),
            );
        });
    });
    assert_eq!(
        &*fake.history_requests.borrow(),
        &[(pane, 688, 512), (pane, 688, 512)]
    );
}

#[ala::test]
fn history_prefetch_bypasses_deferred_backfill(cx: &mut TestAppContext) {
    let pane = PaneId(77);
    let (mux, fake, initial) = cx.update(|cx| history_backfill_debounce_fixture(cx, pane));
    cx.update(|cx| {
        defer_initial_history_backfill(&mux, cx, pane, &initial);
        mux.update(cx, |mux, _| {
            assert!(mux.attached_connection().history.deferred(pane).is_some());
            mux.request_history_prefetch(pane, 1_199);
            assert_eq!(
                mux.attached_connection().history.pending(pane).flatten(),
                Some(1_199)
            );
            assert!(mux.attached_connection().history.deferred(pane).is_some());
        });
    });

    assert_eq!(
        &*fake.history_requests.borrow(),
        &[(pane, 688, 512), (pane, 1_196, 4)]
    );
}

#[ala::test]
fn history_trickle_requests_nearest_chunks_until_budget_and_zero_disables(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let fake = mux.update(cx, |mux, _| {
            let fake = install_fake_connection(mux, HostId::LOCAL);
            let mut options = MuxOptions::default();
            options.set(
                MuxOptionKey::HistoryTrickle,
                "600",
                zz_protocol::MuxOptionSource::RuntimeCommand,
            );
            mux.seed_core(EventPayload::MuxOptionsChanged { options });
            fake
        });
        let pane = PaneId(71);
        let mut viewport = TerminalViewport::blank(1, 3, SessionStatus::Running);
        viewport.scrollbar = ScrollbarState {
            total: 1_203,
            offset: 1_200,
            len: 3,
        };
        let dictionary = viewport.dictionary.as_ref().clone();

        mux.update(cx, |mux, cx| {
            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 1,
                    payload: EventPayload::TerminalViewport { pane, viewport },
                }),
                cx,
            );
        });
        assert_eq!(&*fake.history_requests.borrow(), &[(pane, 688, 512)]);

        mux.update(cx, |mux, cx| {
            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 2,
                    payload: EventPayload::HistoryChunk {
                        pane,
                        start: 688,
                        total: 1_202,
                        offset: 1_200,
                        columns: 1,
                        rows: chunk_rows(&vec![0; 512]),
                        dictionary: dictionary.clone(),
                    },
                }),
                cx,
            );
        });
        assert_eq!(
            &*fake.history_requests.borrow(),
            &[(pane, 688, 512), (pane, 688, 512)]
        );

        mux.update(cx, |mux, cx| {
            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 3,
                    payload: EventPayload::HistoryChunk {
                        pane,
                        start: 688,
                        total: 1_203,
                        offset: 1_200,
                        columns: 1,
                        rows: chunk_rows(&vec![1; 512]),
                        dictionary: dictionary.clone(),
                    },
                }),
                cx,
            );
        });
        assert_eq!(
            &*fake.history_requests.borrow(),
            &[(pane, 688, 512), (pane, 688, 512), (pane, 600, 88)]
        );

        mux.update(cx, |mux, cx| {
            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 4,
                    payload: EventPayload::HistoryChunk {
                        pane,
                        start: 600,
                        total: 1_203,
                        offset: 1_200,
                        columns: 1,
                        rows: chunk_rows(&vec![2; 88]),
                        dictionary,
                    },
                }),
                cx,
            );
        });
        assert_eq!(fake.history_requests.borrow().len(), 3);
        assert_eq!(mux.read(cx).viewports[&pane].read().history.len(), 600);

        mux.update(cx, |mux, cx| {
            let mut options = MuxOptions::default();
            options.set(
                MuxOptionKey::HistoryTrickle,
                "0",
                zz_protocol::MuxOptionSource::RuntimeCommand,
            );
            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 5,
                    payload: EventPayload::MuxOptionsChanged { options },
                }),
                cx,
            );
            let disabled = PaneId(72);
            let mut viewport = TerminalViewport::blank(1, 3, SessionStatus::Running);
            viewport.scrollbar = ScrollbarState {
                total: 103,
                offset: 100,
                len: 3,
            };
            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 6,
                    payload: EventPayload::TerminalViewport {
                        pane: disabled,
                        viewport,
                    },
                }),
                cx,
            );
        });
        assert_eq!(fake.history_requests.borrow().len(), 3);
    });
}

#[ala::test]
fn local_scroll_prefetch_is_bounded_and_coalesces_while_in_flight(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let fake = mux.update(cx, |mux, _| {
            let fake = install_fake_connection(mux, HostId::LOCAL);
            let mut options = MuxOptions::default();
            options.set(
                MuxOptionKey::HistoryTrickle,
                "0",
                zz_protocol::MuxOptionSource::RuntimeCommand,
            );
            mux.seed_core(EventPayload::MuxOptionsChanged { options });
            fake
        });
        let pane = PaneId(73);
        let mut viewport = TerminalViewport::blank(1, 3, SessionStatus::Running);
        viewport.scrollbar = ScrollbarState {
            total: 1_203,
            offset: 1_200,
            len: 3,
        };
        let dictionary = viewport.dictionary.as_ref().clone();
        mux.update(cx, |mux, cx| {
            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 1,
                    payload: EventPayload::TerminalViewport { pane, viewport },
                }),
                cx,
            );
            mux.request_history_prefetch(pane, 1_199);
            mux.request_history_prefetch(pane, 1_198);
        });
        assert_eq!(&*fake.history_requests.borrow(), &[(pane, 1_196, 4)]);

        mux.update(cx, |mux, cx| {
            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 2,
                    payload: EventPayload::HistoryChunk {
                        pane,
                        start: 1_196,
                        total: 1_203,
                        offset: 1_200,
                        columns: 1,
                        rows: chunk_rows(&[1, 2, 3, 4]),
                        dictionary,
                    },
                }),
                cx,
            );
        });

        assert_eq!(
            &*fake.history_requests.borrow(),
            &[(pane, 1_196, 4), (pane, 1_195, 1)]
        );
        assert!(
            fake.history_requests
                .borrow()
                .iter()
                .all(|(_, _, count)| *count <= MAX_HISTORY_CHUNK_ROWS)
        );
        let retained = mux.read(cx).viewports[&pane].read();
        assert_eq!(retained.history.len(), 4);
        let revisions = retained
            .history
            .rows
            .iter()
            .map(|row| row.revision)
            .collect::<BTreeSet<_>>();
        assert_eq!(revisions.len(), retained.history.len());
    });
}
