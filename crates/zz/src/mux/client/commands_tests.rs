use std::rc::Rc;

use ala::TestAppContext;

use super::test_support::*;
use super::*;

fn tracked_commands(mux: &MuxClient, host: HostId) -> Vec<(u64, String)> {
    mux.connections
        .get(&host)
        .expect("host connection")
        .in_flight_commands
        .read()
        .iter()
        .cloned()
        .collect()
}

#[ala::test]
fn a_rejected_command_toasts_instead_of_setting_the_connection_error(cx: &mut TestAppContext) {
    let (_mux, _sink, notifications) = cx.update(|cx| {
        let mux = test_mux(cx);
        let (sink, notifications) = record_notifications(&mux, cx);
        mux.update(cx, |mux, cx| {
            let fake = install_fake_connection(mux, HostId::LOCAL);
            mux.error = None;
            mux.execute(CommandInvocation::new(
                "swap-pane",
                ["-s", "%1", "-t", "%2"],
            ));
            let request_id = fake.next_request_id.get() - 1;
            assert_eq!(
                tracked_commands(mux, HostId::LOCAL),
                [(request_id, "swap-pane".to_owned())]
            );

            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::CommandResponse(zz_protocol::CommandResponse::Error {
                    request_id,
                    error: ServerError::PaneNotFound("%2".to_owned()),
                    output: zz_protocol::RawText::default(),
                }),
                cx,
            );

            assert!(mux.error.is_none());
            assert!(tracked_commands(mux, HostId::LOCAL).is_empty());
        });
        (mux, sink, notifications)
    });

    assert_eq!(
        &*notifications.borrow(),
        &[(
            ClientMessageKind::Error,
            "swap-pane: can't find pane: %2".to_owned(),
        )]
    );
}

#[ala::test]
fn a_close_racing_the_panes_own_exit_stays_silent(cx: &mut TestAppContext) {
    let (_mux, _sink, notifications) = cx.update(|cx| {
        let mux = test_mux(cx);
        let (sink, notifications) = record_notifications(&mux, cx);
        mux.update(cx, |mux, cx| {
            for late_error in [
                ServerError::PaneExited(PaneId(0)),
                ServerError::PaneNotAttached(PaneId(0)),
            ] {
                let fake = install_fake_connection(mux, HostId::LOCAL);
                mux.error = None;
                mux.execute(CommandInvocation::new("kill-pane", ["-t", "%0"]));
                let request_id = fake.next_request_id.get() - 1;

                mux.handle_message(
                    HostId::LOCAL,
                    ProtocolMessage::CommandResponse(zz_protocol::CommandResponse::Error {
                        request_id,
                        error: late_error.clone(),
                        output: zz_protocol::RawText::default(),
                    }),
                    cx,
                );

                assert!(mux.error.is_none(), "{late_error:?}");
                assert!(tracked_commands(mux, HostId::LOCAL).is_empty());
            }
        });
        (mux, sink, notifications)
    });

    assert!(notifications.borrow().is_empty());
}

#[ala::test]
fn command_tracking_prunes_on_success_and_stays_bounded(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mux = test_mux(cx);
        mux.update(cx, |mux, cx| {
            install_fake_connection(mux, HostId::LOCAL);
            mux.execute(CommandInvocation::new("select-pane", ["-t", "%1"]));
            mux.execute(CommandInvocation::new("resize-pane", ["-t", "%1"]));
            assert_eq!(
                tracked_commands(mux, HostId::LOCAL),
                [(1, "select-pane".to_owned()), (2, "resize-pane".to_owned())]
            );

            mux.handle_message(
                HostId::LOCAL,
                ProtocolMessage::CommandResponse(zz_protocol::CommandResponse::Success {
                    request_id: 1,
                    output: zz_protocol::RawText::default(),
                    exit_code: 0,
                    stderr: String::new(),
                    stdout_claim: zz_protocol::StdoutClaim::None,
                }),
                cx,
            );
            assert_eq!(
                tracked_commands(mux, HostId::LOCAL),
                [(2, "resize-pane".to_owned())]
            );

            for _ in 0..MAX_TRACKED_COMMANDS {
                mux.execute(CommandInvocation::new("select-pane", ["-t", "%1"]));
            }
            let tracked = tracked_commands(mux, HostId::LOCAL);
            assert_eq!(tracked.len(), MAX_TRACKED_COMMANDS);
            assert_eq!(tracked.first().map(|(id, _)| *id), Some(3));
        });
    });
}

#[ala::test]
fn successful_commands_only_notify_when_clearing_a_visible_error(cx: &mut TestAppContext) {
    let mux = cx.update(|cx| {
        let mux = test_mux(cx);
        mux.update(cx, |mux, _| {
            install_fake_connection(mux, HostId::LOCAL);
            mux.error = None;
        });
        mux
    });
    cx.run_until_parked();
    let notifications = Rc::new(std::cell::Cell::new(0));
    let observed = Rc::clone(&notifications);
    let _subscription =
        cx.update(|cx| cx.observe(&mux, move |_, _| observed.set(observed.get() + 1)));
    let success = |request_id| {
        ProtocolMessage::CommandResponse(CommandResponse::Success {
            request_id,
            output: zz_protocol::RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: zz_protocol::StdoutClaim::None,
        })
    };
    mux.update(cx, |mux, cx| {
        for request_id in 1..=10 {
            mux.execute(CommandInvocation::new("select-pane", ["-t", "%1"]));
            mux.handle_message(HostId::LOCAL, success(request_id), cx);
        }
        assert!(tracked_commands(mux, HostId::LOCAL).is_empty());
        assert_eq!(mux.pending_commands_revision(), 0);
    });
    cx.run_until_parked();
    assert_eq!(notifications.get(), 0);

    mux.update(cx, |mux, cx| {
        mux.handle_message(
            HostId::LOCAL,
            ProtocolMessage::CommandResponse(CommandResponse::Error {
                request_id: 0,
                error: ServerError::Internal("fixture error".to_owned()),
                output: zz_protocol::RawText::default(),
            }),
            cx,
        );
        assert!(mux.error.is_some());
    });
    cx.run_until_parked();
    assert_eq!(notifications.get(), 1);

    mux.update(cx, |mux, cx| {
        mux.handle_message(HostId::LOCAL, success(0), cx);
        assert!(mux.error.is_none());
    });
    cx.run_until_parked();
    assert_eq!(notifications.get(), 2);

    mux.update(cx, |mux, cx| {
        mux.handle_message(
            HostId::LOCAL,
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 1,
                payload: EventPayload::Snapshot(MuxSnapshot {
                    generation: 9,
                    ..MuxSnapshot::default()
                }),
            }),
            cx,
        );
        assert_eq!(mux.snapshot().generation, 9);
    });
    cx.run_until_parked();
    assert_eq!(notifications.get(), 3);
}

#[ala::test]
fn a_rejected_command_on_a_non_attached_host_is_reported(cx: &mut TestAppContext) {
    let (_mux, _sink, notifications) = cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host(
                "remote",
                "unix:///tmp/zz-fleet-command-error.sock",
            )],
            cx,
        );
        let mux = test_mux(cx);
        let (sink, notifications) = record_notifications(&mux, cx);
        mux.update(cx, |mux, cx| {
            let remote = mux.registry.get_by_name("remote").unwrap().0;
            let fake = install_fake_connection(mux, remote);
            mux.error = None;
            mux.execute_on_host(remote, CommandInvocation::new("kill-session", ["-t", "9"]));
            let request_id = fake.next_request_id.get() - 1;

            mux.handle_message(
                remote,
                ProtocolMessage::CommandResponse(zz_protocol::CommandResponse::Error {
                    request_id,
                    error: ServerError::SessionNotFound("9".to_owned()),
                    output: zz_protocol::RawText::default(),
                }),
                cx,
            );

            assert!(mux.error.is_none());
            assert!(tracked_commands(mux, remote).is_empty());
        });
        (mux, sink, notifications)
    });

    assert_eq!(
        &*notifications.borrow(),
        &[(
            ClientMessageKind::Error,
            "kill-session: can't find session: 9".to_owned(),
        )]
    );
}
