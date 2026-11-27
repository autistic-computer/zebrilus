use ala::TestAppContext;
use zz_protocol::MuxOptions;

use super::test_support::*;
use super::*;

#[ala::test]
fn focus_follows_mouse_follows_the_published_option(cx: &mut TestAppContext) {
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
            install_fake_connection(mux, HostId::LOCAL);
        });
        assert!(
            !mux.read(cx).focus_follows_mouse(),
            "the option's own default is off"
        );
        for (value, expected) in [("on", true), ("off", false)] {
            mux.update(cx, |mux, _| {
                let mut options = MuxOptions::default();
                options.set(
                    MuxOptionKey::FocusFollowsMouse,
                    value,
                    zz_protocol::MuxOptionSource::RuntimeCommand,
                );
                mux.seed_core(EventPayload::MuxOptionsChanged { options });
            });
            assert_eq!(mux.read(cx).focus_follows_mouse(), expected, "{value}");
        }
    });
}

#[ala::test]
fn a_copy_selection_reaches_the_clipboard_unless_set_clipboard_is_off(cx: &mut TestAppContext) {
    fn clipboard_write(
        mux: &mut MuxClient,
        producer: ClipboardProducer,
        request_id: u64,
        text: &str,
        cx: &mut Context<MuxClient>,
    ) {
        mux.handle_message_for_test(
            ProtocolMessage::Event(zz_protocol::Event {
                sequence: 0,
                payload: EventPayload::Clipboard {
                    pane: PaneId(1),
                    request_id,
                    target: ClipboardTarget::Primary,
                    text: text.to_owned(),
                    producer,
                },
            }),
            cx,
        );
    }

    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("clipboard fixture".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        mux.update(cx, |mux, _| {
            install_fake_connection(mux, HostId::LOCAL);
        });
        assert!(
            mux.read(cx).core.set_clipboard_writes(),
            "the pin's set-clipboard default is external, which writes"
        );

        for (value, mirrored) in [("external", true), ("on", true), ("off", false)] {
            mux.update(cx, |mux, cx| {
                let mut options = MuxOptions::default();
                options.set(
                    MuxOptionKey::SetClipboard,
                    value,
                    zz_protocol::MuxOptionSource::RuntimeCommand,
                );
                mux.seed_core(EventPayload::MuxOptionsChanged { options });
                clipboard_write(mux, ClipboardProducer::Server, 7, value, cx);
            });
            assert_eq!(
                mux.read(cx).core.set_clipboard_writes(),
                mirrored,
                "{value}"
            );
            let clipboard = cx.read_from_clipboard().and_then(|item| item.text());
            if mirrored {
                assert_eq!(
                    clipboard.as_deref(),
                    Some(value),
                    "set-clipboard {value} writes the selection the pin would have sent \
                     through the empty OSC 52 selection field"
                );
            } else {
                assert_ne!(
                    clipboard.as_deref(),
                    Some(value),
                    "set-clipboard off leaves the system clipboard alone, as the pin does"
                );
            }
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            assert_eq!(
                cx.read_from_primary()
                    .and_then(|item| item.text())
                    .as_deref(),
                Some(value),
                "PRIMARY always takes the drag selection, at every set-clipboard value"
            );
        }

        mux.update(cx, |mux, cx| {
            let mut options = MuxOptions::default();
            options.set(
                MuxOptionKey::SetClipboard,
                "on",
                zz_protocol::MuxOptionSource::RuntimeCommand,
            );
            mux.seed_core(EventPayload::MuxOptionsChanged { options });
            clipboard_write(mux, ClipboardProducer::Server, 0, "server-zero-id", cx);
        });
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("server-zero-id"),
            "a server write carries the empty field whatever its request id is, and \
             deliver_buffer_clipboard_write publishes one with id zero"
        );

        mux.update(cx, |mux, cx| {
            let mut options = MuxOptions::default();
            options.set(
                MuxOptionKey::SetClipboard,
                "on",
                zz_protocol::MuxOptionSource::RuntimeCommand,
            );
            mux.seed_core(EventPayload::MuxOptionsChanged { options });
            clipboard_write(
                mux,
                ClipboardProducer::Application,
                0,
                "application-osc52",
                cx,
            );
        });
        assert_ne!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("application-osc52"),
            "an application's own OSC 52 keeps the selection field it named, the way \
             input_osc_52 forwards it"
        );
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        assert_eq!(
            cx.read_from_primary()
                .and_then(|item| item.text())
                .as_deref(),
            Some("application-osc52"),
            "an application asking for PRIMARY still gets PRIMARY"
        );
    });
}

#[ala::test]
fn client_window_focus_waits_for_initial_attach_and_deduplicates(cx: &mut TestAppContext) {
    let mux = cx.new(|cx| {
        MuxClient::new(
            Err(DaemonError::Thread("focus test client".to_owned())),
            zz_daemon_client::default_socket_path(),
            cx,
        )
    });
    let input = mux.update(cx, |mux, _| mux.record_input_for_test());

    mux.update(cx, |mux, cx| {
        mux.begin_client_focus_attach();
        mux.set_client_window_focused(true);
        mux.set_client_window_focused(true);
        assert!(input.borrow().is_empty());
        mux.handle_message_for_test(
            ProtocolMessage::Attached {
                session: SessionId(1),
                snapshot: MuxSnapshot {
                    generation: 1,
                    ..MuxSnapshot::default()
                },
                read_only: false,
                client_flags: String::new(),
            },
            cx,
        );
        mux.set_client_window_focused(true);
    });

    assert_eq!(
        &*input.borrow(),
        &[InputMessage::ClientFocus { focused: true }]
    );
}

#[ala::test]
fn rejected_session_attach_recovers_focus_on_the_retained_session(cx: &mut TestAppContext) {
    let mux = cx.new(|cx| {
        MuxClient::new(
            Err(DaemonError::Thread("focus attach failure".to_owned())),
            zz_daemon_client::default_socket_path(),
            cx,
        )
    });
    let input = mux.update(cx, |mux, _| {
        let fake = install_fake_connection(mux, HostId::LOCAL);
        let input = mux.record_input_for_test();
        mux.set_client_window_focused(true);
        (input, fake)
    });

    mux.update(cx, |mux, cx| {
        mux.handle_message(
            HostId::LOCAL,
            ProtocolMessage::Attached {
                session: SessionId(1),
                snapshot: MuxSnapshot {
                    generation: 1,
                    ..MuxSnapshot::default()
                },
                read_only: false,
                client_flags: String::new(),
            },
            cx,
        );
        input.0.borrow_mut().clear();

        mux.attach(SessionId(99));
        assert_eq!(input.1.attached_session.get(), Some(SessionId(99)));
        mux.set_client_window_focused(false);
        assert!(input.0.borrow().is_empty());

        mux.handle_command_response(
            HostId::LOCAL,
            CommandResponse::Error {
                request_id: 0,
                error: ServerError::SessionNotFound("99".to_owned()),
                output: zz_protocol::RawText::default(),
            },
            cx,
        );
        mux.set_client_window_focused(false);
        mux.set_client_window_focused(true);
    });

    assert_eq!(
        &*input.0.borrow(),
        &[
            InputMessage::ClientFocus { focused: false },
            InputMessage::ClientFocus { focused: true },
        ]
    );
}

#[ala::test]
fn successful_session_attach_replays_focus_for_the_new_epoch(cx: &mut TestAppContext) {
    let mux = cx.new(|cx| {
        MuxClient::new(
            Err(DaemonError::Thread("focus session switch".to_owned())),
            zz_daemon_client::default_socket_path(),
            cx,
        )
    });
    let (input, fake) = mux.update(cx, |mux, _| {
        let fake = install_fake_connection(mux, HostId::LOCAL);
        (mux.record_input_for_test(), fake)
    });

    mux.update(cx, |mux, cx| {
        mux.set_client_window_focused(true);
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
        input.borrow_mut().clear();

        mux.attach(SessionId(2));
        mux.set_client_window_focused(false);
        assert!(input.borrow().is_empty());
        mux.handle_message(
            HostId::LOCAL,
            ProtocolMessage::Attached {
                session: SessionId(2),
                snapshot: MuxSnapshot::default(),
                read_only: false,
                client_flags: String::new(),
            },
            cx,
        );
    });

    assert_eq!(fake.attached_session.get(), Some(SessionId(2)));
    assert_eq!(
        &*input.borrow(),
        &[InputMessage::ClientFocus { focused: false }]
    );
}

#[ala::test]
fn unrelated_request_zero_error_does_not_close_the_ready_focus_epoch(cx: &mut TestAppContext) {
    let mux = cx.new(|cx| {
        MuxClient::new(
            Err(DaemonError::Thread("focus request error".to_owned())),
            zz_daemon_client::default_socket_path(),
            cx,
        )
    });
    let input = mux.update(cx, |mux, _| mux.record_input_for_test());

    mux.update(cx, |mux, cx| {
        mux.set_client_window_focused(true);
        mux.handle_message_for_test(
            ProtocolMessage::Attached {
                session: SessionId(1),
                snapshot: MuxSnapshot {
                    generation: 1,
                    ..MuxSnapshot::default()
                },
                read_only: false,
                client_flags: String::new(),
            },
            cx,
        );
        input.borrow_mut().clear();

        mux.handle_command_response(
            HostId::LOCAL,
            CommandResponse::Error {
                request_id: 0,
                error: ServerError::InvalidCommand("unrelated input error".to_owned()),
                output: zz_protocol::RawText::default(),
            },
            cx,
        );
        mux.set_client_window_focused(false);
    });

    assert_eq!(
        &*input.borrow(),
        &[InputMessage::ClientFocus { focused: false }]
    );
}

#[ala::test]
fn unrelated_request_zero_error_does_not_close_a_pending_focus_epoch(cx: &mut TestAppContext) {
    let mux = cx.new(|cx| {
        MuxClient::new(
            Err(DaemonError::Thread(
                "focus pending request error".to_owned(),
            )),
            zz_daemon_client::default_socket_path(),
            cx,
        )
    });
    let (input, fake) = mux.update(cx, |mux, _| {
        let fake = install_fake_connection(mux, HostId::LOCAL);
        (mux.record_input_for_test(), fake)
    });

    mux.update(cx, |mux, cx| {
        mux.set_client_window_focused(true);
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
        input.borrow_mut().clear();

        mux.attach(SessionId(99));
        mux.set_client_window_focused(false);
        mux.handle_command_response(
            HostId::LOCAL,
            CommandResponse::Error {
                request_id: 0,
                error: ServerError::InvalidCommand("unrelated input error".to_owned()),
                output: zz_protocol::RawText::default(),
            },
            cx,
        );
        assert!(input.borrow().is_empty());

        mux.handle_command_response(
            HostId::LOCAL,
            CommandResponse::Error {
                request_id: 0,
                error: ServerError::SessionNotFound("99".to_owned()),
                output: zz_protocol::RawText::default(),
            },
            cx,
        );
    });

    assert_eq!(fake.attached_session.get(), Some(SessionId(99)));
    assert_eq!(
        &*input.borrow(),
        &[InputMessage::ClientFocus { focused: false }]
    );
}

#[ala::test]
fn client_window_focus_replays_latest_pending_state_after_host_switch(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-focus-host-switch.sock")],
            cx,
        );
    });
    let mux = cx.new(|cx| {
        MuxClient::new(
            Err(DaemonError::Thread("focus test client".to_owned())),
            zz_daemon_client::default_socket_path(),
            cx,
        )
    });
    let input = mux.update(cx, |mux, _| mux.record_input_for_test());

    mux.update(cx, |mux, cx| {
        install_fake_connection(mux, HostId::LOCAL);
        let remote = mux.registry.get_by_name("remote").expect("remote host").0;
        install_fake_connection(mux, remote);
        mux.set_client_window_focused(true);
        mux.handle_message(
            HostId::LOCAL,
            ProtocolMessage::Attached {
                session: SessionId(1),
                snapshot: MuxSnapshot {
                    generation: 1,
                    ..MuxSnapshot::default()
                },
                read_only: false,
                client_flags: String::new(),
            },
            cx,
        );
        input.borrow_mut().clear();

        assert!(mux.attach_to_host(remote, SessionId(2), cx));
        mux.set_client_window_focused(false);
        mux.set_client_window_focused(true);
        mux.set_client_window_focused(false);
        assert!(input.borrow().is_empty());
        mux.handle_message(
            remote,
            ProtocolMessage::Attached {
                session: SessionId(2),
                snapshot: MuxSnapshot {
                    generation: 2,
                    ..MuxSnapshot::default()
                },
                read_only: false,
                client_flags: String::new(),
            },
            cx,
        );
        mux.set_client_window_focused(false);
    });

    assert_eq!(
        &*input.borrow(),
        &[InputMessage::ClientFocus { focused: false }]
    );
}

#[ala::test]
fn client_window_focus_replays_after_reconnect_attach(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::config::set_fleet_hosts_for_test(
            vec![test_host("remote", "unix:///tmp/zz-focus-reconnect.sock")],
            cx,
        );
    });
    let mux = cx.new(|cx| {
        MuxClient::new(
            Err(DaemonError::Thread("focus reconnect".to_owned())),
            zz_daemon_client::default_socket_path(),
            cx,
        )
    });
    let input = mux.update(cx, |mux, _| {
        let remote = mux.registry.get_by_name("remote").expect("remote host").0;
        install_fake_connection(mux, remote);
        mux.attached_host = remote;
        mux.record_input_for_test()
    });

    mux.update(cx, |mux, cx| {
        let remote = mux.registry.get_by_name("remote").expect("remote host").0;
        mux.set_client_window_focused(true);
        mux.handle_message(
            remote,
            ProtocolMessage::Attached {
                session: SessionId(9),
                snapshot: MuxSnapshot::default(),
                read_only: false,
                client_flags: String::new(),
            },
            cx,
        );
        input.borrow_mut().clear();

        mux.reattach_after_reconnect(remote, cx);
        mux.set_client_window_focused(false);
        assert!(input.borrow().is_empty());
        mux.handle_message(
            remote,
            ProtocolMessage::Attached {
                session: SessionId(9),
                snapshot: MuxSnapshot::default(),
                read_only: false,
                client_flags: String::new(),
            },
            cx,
        );
    });

    assert_eq!(
        &*input.borrow(),
        &[InputMessage::ClientFocus { focused: false }]
    );
}

#[ala::test]
fn late_input_for_an_exited_last_pane_does_not_mask_the_empty_workspace(cx: &mut TestAppContext) {
    cx.update(|cx| {
        for late_error in [
            ServerError::PaneExited(PaneId(0)),
            ServerError::PaneNotAttached(PaneId(0)),
        ] {
            for error_before_snapshot in [true, false] {
                let client = cx.new(|cx| {
                    MuxClient::new(
                        Err(DaemonError::Thread("test client".to_owned())),
                        zz_daemon_client::default_socket_path(),
                        cx,
                    )
                });
                client.update(cx, |client, cx| {
                    client.error = None;
                    let late_input = || {
                        ProtocolMessage::CommandResponse(zz_protocol::CommandResponse::Error {
                            request_id: 0,
                            error: late_error.clone(),
                            output: zz_protocol::RawText::default(),
                        })
                    };
                    let empty_snapshot = || {
                        ProtocolMessage::Event(zz_protocol::Event {
                            sequence: 1,
                            payload: EventPayload::Snapshot(MuxSnapshot {
                                generation: 8,
                                focused_window: None,
                                sessions: Vec::new(),
                            }),
                        })
                    };

                    if error_before_snapshot {
                        client.handle_message(HostId::LOCAL, late_input(), cx);
                        client.handle_message(HostId::LOCAL, empty_snapshot(), cx);
                    } else {
                        client.handle_message(HostId::LOCAL, empty_snapshot(), cx);
                        client.handle_message(HostId::LOCAL, late_input(), cx);
                    }
                });

                assert!(client.read(cx).error().is_none(), "{late_error:?}");
                assert_eq!(client.read(cx).snapshot().generation, 8);
                assert!(client.read(cx).snapshot().sessions.is_empty());
            }
        }
    });
}
