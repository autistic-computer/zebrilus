use ala::TestAppContext;

use super::test_support::*;
use super::*;

#[ala::test]
fn a_windows_host_is_never_asked_for_agent_accounts(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        for (platform, expected) in [
            (zz_protocol::HostPlatform::Windows, vec!["providers"]),
            (
                zz_protocol::HostPlatform::Linux(zz_protocol::LinuxDistro::Arch),
                vec!["providers", "agent-accounts"],
            ),
        ] {
            let sent = mux.update(cx, |mux, _| {
                let fake = install_fake_connection(mux, HostId::LOCAL);
                mux.connections
                    .get_mut(&HostId::LOCAL)
                    .expect("host connection")
                    .platform = platform;
                mux.request_providers(true);
                fake.commands
                    .borrow()
                    .iter()
                    .map(|command| command.name.clone())
                    .collect::<Vec<_>>()
            });
            assert_eq!(sent, expected, "{platform:?}");
        }
    });
}

#[cfg(feature = "agent-pane")]
fn agent_blob(seq: u64) -> Vec<u8> {
    serde_json::to_vec(&zz_daemon::AgentStreamItem {
        seq,
        payload: zz_daemon::AgentStreamPayload::PromptAccepted { turn_id: seq },
    })
    .expect("a stream item encodes")
}

#[cfg(feature = "agent-pane")]
fn agent_reset(seq: u64) -> Vec<u8> {
    serde_json::to_vec(&zz_daemon::AgentStreamItem {
        seq,
        payload: zz_daemon::AgentStreamPayload::SessionReset { restoring: true },
    })
    .expect("a reset item encodes")
}

#[cfg(feature = "agent-pane")]
fn applied_agent_seqs(mux: &mut MuxClient, pane: PaneId) -> Vec<u64> {
    mux.take_agent_events_for(&BTreeSet::from([pane]))
        .items
        .into_iter()
        .flat_map(|(_, items)| items.into_iter().map(|item| item.seq))
        .collect()
}

#[cfg(feature = "agent-pane")]
#[cfg(feature = "agent-pane")]
#[ala::test]
fn an_agent_pane_tracks_its_stream_only_while_watched(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        mux.update(cx, |mux, _| {
            let pane = PaneId(4);
            let sink = mux.record_agent_requests_for_test();
            mux.apply_agent_updates(pane, 1, vec![agent_blob(1)]);
            assert!(applied_agent_seqs(mux, pane).is_empty());

            mux.watch_agent(pane, 0);
            mux.watch_agent(pane, 0);
            assert_eq!(
                sink.borrow().as_slice(),
                [(pane, AgentRequest::Replay { from_seq: 0 })],
                "opening asks for the whole journal once"
            );
            mux.apply_agent_updates(pane, 1, vec![agent_blob(1), agent_blob(2)]);
            mux.unwatch_agent(pane);
            assert!(applied_agent_seqs(mux, pane).is_empty());
            mux.apply_agent_updates(pane, 3, vec![agent_blob(3)]);
            assert!(applied_agent_seqs(mux, pane).is_empty());

            mux.watch_agent(pane, 0);
            assert_eq!(
                sink.borrow().last(),
                Some(&(pane, AgentRequest::Replay { from_seq: 0 })),
                "reopening replays from the start"
            );
        });
    });
}

#[cfg(feature = "agent-pane")]
#[ala::test]
fn the_agent_stream_is_filtered_by_seq_and_a_gap_asks_for_a_replay(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        mux.update(cx, |mux, cx| {
            let pane = PaneId(3);
            mux.watched_agents.insert(pane);
            let sink = mux.record_agent_requests_for_test();
            let blob = agent_blob;
            let reset = agent_reset;
            let applied = |mux: &mut MuxClient| applied_agent_seqs(mux, pane);

            mux.apply_agent_updates(pane, 1, vec![blob(1), blob(2)]);
            assert_eq!(applied(mux), [1, 2]);

            mux.apply_agent_updates(pane, 1, vec![blob(1), blob(2), blob(3)]);
            assert_eq!(
                applied(mux),
                [3],
                "a replay overlaps the live tail and only the new item survives"
            );

            mux.apply_agent_updates(pane, 9, vec![blob(9)]);
            assert!(
                applied(mux).is_empty(),
                "a batch past the cursor is a hole, not something to buffer across"
            );
            assert_eq!(mux.agent_cursors.get(&pane), Some(&3));
            assert_eq!(
                &*sink.borrow(),
                &[(pane, AgentRequest::Replay { from_seq: 3 })]
            );

            sink.borrow_mut().clear();
            mux.request_agent_replay_from_cursor(pane);
            assert_eq!(
                &*sink.borrow(),
                &[(pane, AgentRequest::Replay { from_seq: 3 })]
            );

            sink.borrow_mut().clear();
            mux.clear_agent_streams();
            mux.request_agent_replay_from_cursor(pane);
            assert_eq!(
                &*sink.borrow(),
                &[(pane, AgentRequest::Replay { from_seq: 0 })],
                "an attach forgets the cursor so the stream replays from the top"
            );

            sink.borrow_mut().clear();
            mux.apply_agent_updates(pane, 40, vec![reset(40), blob(41)]);
            assert_eq!(applied(mux), [40, 41]);
            assert_eq!(mux.agent_cursors.get(&pane), Some(&41));
            assert!(sink.borrow().is_empty());

            let mut attaching = false;
            mux.apply_core_event(
                HostId::LOCAL,
                CoreEvent::AgentLagged { pane, next_seq: 42 },
                None,
                &mut attaching,
                cx,
            );
            assert_eq!(
                &*sink.borrow(),
                &[(pane, AgentRequest::Replay { from_seq: 41 })],
                "an overflow replays from the shell's cursor, not the daemon's drop point"
            );
        });
    });
}

#[cfg(feature = "agent-pane")]
#[ala::test]
fn a_hole_asks_for_one_replay_until_the_stream_lands_again(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        mux.update(cx, |mux, _| {
            let pane = PaneId(3);
            mux.watched_agents.insert(pane);
            let sink = mux.record_agent_requests_for_test();
            let applied = |mux: &mut MuxClient| applied_agent_seqs(mux, pane);

            mux.apply_agent_updates(pane, 1, vec![agent_blob(1), agent_blob(2)]);
            assert_eq!(applied(mux), [1, 2]);

            mux.apply_agent_updates(pane, 7, vec![agent_blob(7)]);
            mux.apply_agent_updates(pane, 8, vec![agent_blob(8)]);
            mux.apply_agent_updates(pane, 9, vec![agent_blob(9), agent_blob(10)]);
            assert!(
                applied(mux).is_empty(),
                "nothing across the hole is applied while the replay is outstanding"
            );
            assert_eq!(
                &*sink.borrow(),
                &[(pane, AgentRequest::Replay { from_seq: 2 })],
                "three sightings of one hole are one ask"
            );

            mux.apply_agent_updates(pane, 2, vec![agent_blob(2), agent_blob(3)]);
            assert_eq!(
                applied(mux),
                [3],
                "the replay overlaps the cursor and lands the item the hole hid"
            );
            assert_eq!(
                sink.borrow().len(),
                1,
                "landing the replay is not itself a reason to ask again"
            );

            mux.apply_agent_updates(pane, 20, vec![agent_blob(20)]);
            assert!(applied(mux).is_empty());
            assert_eq!(
                &*sink.borrow(),
                &[
                    (pane, AgentRequest::Replay { from_seq: 2 }),
                    (pane, AgentRequest::Replay { from_seq: 3 })
                ],
                "a hole after the replay landed is a new hole, asked from the new cursor"
            );
        });
    });
}

#[cfg(feature = "agent-pane")]
#[ala::test]
fn a_journal_floor_replay_clears_the_outstanding_ask(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        mux.update(cx, |mux, _| {
            let pane = PaneId(3);
            mux.watched_agents.insert(pane);
            let sink = mux.record_agent_requests_for_test();

            mux.apply_agent_updates(pane, 1, vec![agent_blob(1), agent_blob(2)]);
            mux.apply_agent_updates(pane, 7, vec![agent_blob(7)]);
            assert_eq!(sink.borrow().len(), 1);
            let _ = applied_agent_seqs(mux, pane);

            mux.apply_agent_updates(pane, 40, vec![agent_reset(40), agent_blob(41)]);
            assert_eq!(
                applied_agent_seqs(mux, pane),
                [40, 41],
                "the daemon's journal-floor replay is stamped past the hole and still applies"
            );
            assert!(!mux.agent_replays_pending.contains(&pane));

            mux.apply_agent_updates(pane, 50, vec![agent_blob(50)]);
            assert_eq!(
                sink.borrow().last(),
                Some(&(pane, AgentRequest::Replay { from_seq: 41 })),
                "the next hole asks again once the reset landed"
            );
        });
    });
}

#[cfg(feature = "agent-pane")]
#[ala::test]
fn a_reattach_forgets_that_a_pane_was_waiting_on_a_replay(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        mux.update(cx, |mux, _| {
            let pane = PaneId(3);
            mux.watched_agents.insert(pane);
            let sink = mux.record_agent_requests_for_test();

            mux.apply_agent_updates(pane, 1, vec![agent_blob(1)]);
            mux.apply_agent_updates(pane, 5, vec![agent_blob(5)]);
            assert!(mux.agent_replays_pending.contains(&pane));

            mux.clear_agent_streams();
            assert!(mux.agent_replays_pending.is_empty());

            sink.borrow_mut().clear();
            mux.apply_agent_updates(pane, 5, vec![agent_blob(5)]);
            assert_eq!(
                &*sink.borrow(),
                &[(pane, AgentRequest::Replay { from_seq: 0 })],
                "a fresh connection asks from the top rather than staying silent"
            );

            mux.forget_pane(pane);
            assert!(mux.agent_replays_pending.is_empty());
        });
    });
}

#[cfg(feature = "agent-pane")]
#[ala::test]
fn an_undecodable_agent_item_is_consumed_rather_than_left_as_a_hole(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        mux.update(cx, |mux, _| {
            let pane = PaneId(3);
            mux.watched_agents.insert(pane);
            let sink = mux.record_agent_requests_for_test();

            mux.apply_agent_updates(
                pane,
                1,
                vec![
                    agent_blob(1),
                    b"{\"seq\":2,\"payload\":{}}".to_vec(),
                    agent_blob(3),
                ],
            );

            assert_eq!(
                applied_agent_seqs(mux, pane),
                [1, 3],
                "the item after the undecodable one is still applied"
            );
            assert_eq!(mux.agent_cursors.get(&pane), Some(&3));
            assert!(
                sink.borrow().is_empty(),
                "a skipped seq counts as consumed, so it is not a gap to replay"
            );
        });
    });
}
