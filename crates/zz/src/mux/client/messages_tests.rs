use std::{rc::Rc, time::Instant};

use ala::TestAppContext;

use super::test_support::*;
use super::tests::*;
use super::*;

#[ala::test]
fn chooser_close_revision_survives_following_open_and_tracks_resets(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("chooser close revision".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        mux.update(cx, |mux, cx| {
            assert_eq!(mux.choose_tree_closed_revision(), 0);
            seed_choose_tree(mux);
            let state = mux.choose_tree().cloned().unwrap();
            let publish = |mux: &mut MuxClient, payload, cx: &mut Context<MuxClient>| {
                mux.handle_message(
                    HostId::LOCAL,
                    ProtocolMessage::Event(zz_protocol::Event {
                        sequence: 1,
                        payload,
                    }),
                    cx,
                );
            };
            publish(
                mux,
                EventPayload::ChooseTree {
                    state: Some(state.clone()),
                },
                cx,
            );
            let opened = mux.choose_tree_revision();
            publish(
                mux,
                EventPayload::ChooseTreeUpdate {
                    search: None,
                    selected: 0,
                },
                cx,
            );
            assert!(mux.choose_tree_revision() > opened);
            assert_eq!(mux.choose_tree_closed_revision(), 0);
            publish(mux, EventPayload::ChooseTree { state: None }, cx);
            let closed = mux.choose_tree_closed_revision();
            assert_eq!(closed, mux.choose_tree_revision());
            assert!(closed > opened);
            publish(
                mux,
                EventPayload::ChooseTree {
                    state: Some(state.clone()),
                },
                cx,
            );
            assert!(mux.choose_tree().is_some());
            assert!(mux.choose_tree_revision() > closed);
            assert_eq!(mux.choose_tree_closed_revision(), closed);

            mux.reset_session_state(cx);
            assert!(mux.choose_tree().is_none());
            let reset = mux.choose_tree_closed_revision();
            assert!(reset > closed);
            assert_eq!(reset, mux.choose_tree_revision());
            publish(mux, EventPayload::ChooseTree { state: Some(state) }, cx);
            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Attached {
                    session: SessionId(1),
                    snapshot: MuxSnapshot::default(),
                    read_only: false,
                    client_flags: String::new(),
                },
                cx,
            );
            assert!(mux.choose_tree().is_none());
            assert!(mux.choose_tree_closed_revision() > reset);
            assert_eq!(
                mux.choose_tree_closed_revision(),
                mux.choose_tree_revision()
            );
        });
    });
}

#[ala::test]
fn non_attached_hosts_cache_snapshots_and_ignore_attached_only_events(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-step4-remote.sock")],
            cx,
        );
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = mux.read(cx).registry.get_by_name("remote").unwrap().0;

        mux.update(cx, |mux, cx| {
            seed_snapshot(
                mux,
                MuxSnapshot {
                    generation: 3,
                    focused_window: None,
                    sessions: Vec::new(),
                },
            );
            seed_choose_tree(mux);
            let revision = mux.choose_tree_revision;

            mux.handle_message(
                remote,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 9,
                    payload: EventPayload::Snapshot(MuxSnapshot {
                        generation: 11,
                        focused_window: None,
                        sessions: Vec::new(),
                    }),
                }),
                cx,
            );

            assert_eq!(mux.core.snapshot().generation, 3);
            assert_eq!(
                mux.connections
                    .get(&remote)
                    .unwrap()
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .generation,
                11
            );
            assert_eq!(
                mux.choose_tree_revision, revision,
                "remote snapshots now refresh the sidebar without perturbing the chooser",
            );

            mux.sidebar_focus_revision = 4;
            mux.handle_message(
                remote,
                ProtocolMessage::Event(zz_protocol::Event {
                    sequence: 10,
                    payload: EventPayload::FocusSidebar,
                }),
                cx,
            );
            mux.handle_message(
                remote,
                ProtocolMessage::Attached {
                    session: SessionId(99),
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

            assert_eq!(mux.sidebar_focus_revision, 4);
            assert_eq!(mux.core.attached_session(), None);
            assert_eq!(
                mux.connections
                    .get(&remote)
                    .unwrap()
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .generation,
                11
            );
        });
    });
}

#[ala::test]
fn snapshot_handles_share_one_allocation_until_replaced(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let client = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let first = client.read(cx).snapshot();
        let same = client.read(cx).snapshot();
        assert!(Arc::ptr_eq(&first, &same));

        client.update(cx, |client, cx| {
            client.handle_message(
                HostId::LOCAL,
                ProtocolMessage::Attached {
                    session: SessionId(7),
                    snapshot: MuxSnapshot {
                        generation: 42,
                        focused_window: None,
                        sessions: Vec::new(),
                    },
                    read_only: false,
                    client_flags: String::new(),
                },
                cx,
            );
        });

        let replacement = client.read(cx).snapshot();
        assert!(!Arc::ptr_eq(&first, &replacement));
        assert_eq!(first.generation, 0);
        assert_eq!(replacement.generation, 42);
    });
}

#[ala::test]
fn focus_sidebar_events_advance_the_client_revision(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let client = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        assert_eq!(client.read(cx).sidebar_focus_revision(), 0);

        for (sequence, expected) in [(1, 1), (2, 2)] {
            client.update(cx, |client, cx| {
                client.handle_message(
                    HostId::LOCAL,
                    ProtocolMessage::Event(zz_protocol::Event {
                        sequence,
                        payload: EventPayload::FocusSidebar,
                    }),
                    cx,
                );
            });
            assert_eq!(client.read(cx).sidebar_focus_revision(), expected);
        }
    });
}

#[ala::test]
fn bell_events_from_attached_and_background_hosts_advance_the_client_revision(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-bell-remote.sock")],
            cx,
        );
        let client = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        let remote = client.read(cx).registry.get_by_name("remote").unwrap().0;
        assert_eq!(client.read(cx).bell_revision(), 0);

        for (host, sequence, expected) in
            [(HostId::LOCAL, 1, 1), (remote, 2, 2), (HostId::LOCAL, 3, 3)]
        {
            client.update(cx, |client, cx| {
                client.handle_message(
                    host,
                    ProtocolMessage::Event(zz_protocol::Event {
                        sequence,
                        payload: EventPayload::Bell { pane: PaneId(3) },
                    }),
                    cx,
                );
            });
            assert_eq!(client.read(cx).bell_revision(), expected);
        }
    });
}

#[test]
fn decoded_message_bridge_applies_backpressure_after_one_pending_message() {
    let (messages, incoming) = decoded_message_channel();
    messages
        .try_send(ProtocolMessage::Detach)
        .expect("first decoded message");
    assert!(matches!(
        messages.try_send(ProtocolMessage::Resync),
        Err(async_channel::TrySendError::Full(ProtocolMessage::Resync))
    ));
    assert_eq!(
        incoming.try_recv().expect("pending decoded message"),
        ProtocolMessage::Detach
    );
}

#[test]
fn a_decoded_backlog_drains_in_bounded_batches() {
    let (sender, incoming) = async_channel::unbounded();
    for message in 1..200 {
        sender.try_send(message).unwrap();
    }
    let mut handled = Vec::new();
    let cut = drain_decoded_batch(0, &incoming, Instant::now(), |message| {
        handled.push(message);
        true
    });
    assert_eq!(cut, Some(true));
    assert_eq!(handled, (0..64).collect::<Vec<_>>());
    assert_eq!(incoming.len(), 200 - 64);

    let (sender, incoming) = async_channel::unbounded();
    sender.try_send(1).unwrap();
    let mut handled = Vec::new();
    let cut = drain_decoded_batch(0, &incoming, Instant::now(), |message| {
        handled.push(message);
        true
    });
    assert_eq!(cut, Some(false));
    assert_eq!(handled, vec![0, 1]);

    let stale = Instant::now().checked_sub(MAX_DECODED_BATCH_TIME).unwrap();
    let (sender, incoming) = async_channel::unbounded();
    sender.try_send(1).unwrap();
    let cut = drain_decoded_batch(0, &incoming, stale, |_| true);
    assert_eq!(cut, Some(true));
    assert_eq!(incoming.len(), 1);

    assert_eq!(
        drain_decoded_batch(0, &incoming, Instant::now(), |_| false),
        None
    );
}

#[ala::test]
fn terminal_frames_reach_only_their_pane_view(cx: &mut TestAppContext) {
    use crate::terminal::view::TerminalView;
    use std::cell::Cell;

    let mux = cx.update(|cx| {
        zz_ui::init(cx);
        let mux = test_mux(cx);
        mux.update(cx, |mux, _| {
            install_fake_connection(mux, HostId::LOCAL);
        });
        mux
    });
    let (root, cx) = cx.add_window_view(|window, cx| {
        let terminal = cx.new(|cx| {
            TerminalView::new(PaneId(7), mux.clone(), Rc::new(Cell::new(true)), window, cx)
        });
        let other = cx.new(|cx| {
            TerminalView::new(PaneId(8), mux.clone(), Rc::new(Cell::new(true)), window, cx)
        });
        KittyPaneLifecycleRoot {
            terminal: Some(terminal),
            other: Some(other),
            shown: true,
        }
    });
    let (terminal, other) = root.read_with(cx, |root, _| {
        (root.terminal.clone().unwrap(), root.other.clone().unwrap())
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();

    let counts = Rc::new([Cell::new(0), Cell::new(0), Cell::new(0)]);
    let _subscriptions = cx.update(|_, cx| {
        let mux_count = Rc::clone(&counts);
        let terminal_count = Rc::clone(&counts);
        let other_count = Rc::clone(&counts);
        [
            cx.observe(&mux, move |_, _| mux_count[0].set(mux_count[0].get() + 1)),
            cx.observe(&terminal, move |_, _| {
                terminal_count[1].set(terminal_count[1].get() + 1);
            }),
            cx.observe(&other, move |_, _| {
                other_count[2].set(other_count[2].get() + 1);
            }),
        ]
    });
    let seen = |counts: &[Cell<usize>; 3]| counts.each_ref().map(Cell::get);
    let generation = |view: &ala::Entity<TerminalView>, cx: &mut ala::VisualTestContext| {
        view.read_with(cx, |view, _| view.retained().read().viewport.generation)
    };

    let first = history_fixture_viewport(&[1, 2, 3], 5, 3, 0);
    mux.update(cx, |mux, cx| {
        mux.handle_message(
            HostId::LOCAL,
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: EventPayload::TerminalViewport {
                    pane: PaneId(7),
                    viewport: first.clone(),
                },
            }),
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(seen(&counts), [0, 1, 0]);
    assert_eq!(generation(&terminal, cx), 5);

    let next = history_fixture_viewport(&[1, 2, 4], 6, 3, 0);
    let patch = TerminalViewport::diff(&first, &next).expect("compatible frames");
    mux.update(cx, |mux, cx| {
        mux.handle_message(
            HostId::LOCAL,
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 2,
                payload: EventPayload::TerminalPatch {
                    pane: PaneId(7),
                    patch,
                },
            }),
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(seen(&counts), [0, 2, 0]);
    assert_eq!(generation(&terminal, cx), 6);
    assert_eq!(generation(&other, cx), 0);
}
