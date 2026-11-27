#[cfg(unix)]
use super::ctrl_test_support::compact_command;
use super::ctrl_test_support::{compact_hello, compact_registered, reliable_children};
use super::*;

#[test]
fn initializing_status_waits_for_explicit_refresh_while_existing_clients_update() {
    let shared = Arc::new(Shared::new(61));
    let mut context = ExecutionContext::default();
    {
        let mut inner = shared.inner.lock();
        for command in [
            CommandInvocation::new("new-session", ["-d", "-s", "initial-status"]),
            CommandInvocation::new(
                "set-option",
                ["-t", "initial-status", "status-format[0]", "FIRST"],
            ),
        ] {
            inner
                .engine
                .execute(&mut context, &command)
                .expect("model command");
        }
    }
    let session = context.session.expect("session");
    let existing = OutboundMailbox::new();
    let (existing_client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&existing));
    let hello = compact_hello(ClientKind::Interactive);
    let (initial_client, _) = shared.register_welcome(&hello).expect("welcome");
    let initial = OutboundMailbox::new();
    shared.subscribe(initial_client, Arc::clone(&initial));
    {
        let mut inner = shared.inner.lock();
        inner
            .attachment
            .attached
            .entry(session)
            .or_default()
            .extend([existing_client, initial_client]);
        inner.client_entry(existing_client).size.replace((80, 24));
        inner.client_entry(initial_client).size.replace((97, 31));
    }
    for label in ["FIRST", "SECOND"] {
        shared
            .inner
            .lock()
            .engine
            .execute(
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    ["-t", "initial-status", "status-format[0]", label],
                ),
            )
            .expect("status format");
        shared.publish_mux_snapshots_except(true, true, None);
        shared.refresh_status_for_sessions(Some(&BTreeSet::from([session])));
        assert!(
            reliable_children(&existing)
                .iter()
                .any(|message| matches!(message,
            ProtocolMessage::Event(Event { payload: EventPayload::StatusChanged { status }, .. })
                if status.rows == [label]))
        );
        assert!(!reliable_children(&initial).iter().any(|message| matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::StatusChanged { .. },
                ..
            })
        )));
        assert!(shared.read_client(initial_client, |c| {
            c.is_none_or(|c| c.status_rows.is_none())
        }));
    }
    shared.refresh_status_filtered(None, Some(&BTreeSet::from([initial_client])));
    let statuses = reliable_children(&initial)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::StatusChanged { status },
                ..
            }) => Some(status),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].rows, ["SECOND"]);
    assert!(shared.read_client(initial_client, |c| {
        c.is_some_and(|c| c.status_rows.is_some())
    }));
}

#[test]
fn caller_input_flags_keep_packed_escaped_and_false_positive_spellings() {
    for name in ["display-message", "split-window"] {
        for arguments in [vec!["-I"], vec!["-It%7"]] {
            let command = CommandInvocation::new(name, arguments);
            assert_eq!(
                command_stdin_sink(name, &command.args),
                Some(CommandStdinSink::PaneInput)
            );
        }
        for arguments in [vec!["--", "-I"], vec!["-t", "targetI"], vec!["textI"]] {
            let command = CommandInvocation::new(name, arguments);
            assert_eq!(command_stdin_sink(name, &command.args), None);
        }
    }
    let command = CommandInvocation::new("display-message", ["-pI"]);
    assert_eq!(
        command_stdin_sink(&command.name, &command.args),
        Some(CommandStdinSink::PaneInput)
    );
    for line in [
        r"display-message -\111; split-window -\111",
        r#"display-message '-I'; split-window "-I""#,
    ] {
        let parsed = zz_mux::parse_config_with_expansions(
            "<control>",
            line,
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.commands.len(), 2);
        for command in parsed.commands {
            assert_eq!(
                command_stdin_sink(&command.name, &command.args),
                Some(CommandStdinSink::PaneInput)
            );
        }
    }
}

#[test]
fn display_alias_absence_preserves_packed_targets_literal_markers_and_errors() {
    let shared = Arc::new(Shared::new(63));
    let mut context = ExecutionContext::default();
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "alias-absence"]),
        )
        .expect("model session");
    let session = context.session.expect("session");
    let pane = context.pane.expect("pane");
    let (client, _) = compact_registered(&shared, zz_protocol::Subscriptions::terminal());
    shared
        .inner
        .lock()
        .attachment
        .attached
        .entry(session)
        .or_default()
        .insert(client);
    for target in ["@", "{active}", "{current}"] {
        let command = CommandInvocation::new(
            "display-message",
            [format!("-pt{target}"), "#{pane_id}".to_owned()],
        );
        let route = shared
            .display_message_client_alias(client, &command)
            .expect("alias route");
        assert!(route.command.is_some());
        let output = shared
            .execute(client, ClientKind::Interactive, &mut context, &command)
            .expect("packed alias");
        assert_eq!(output.output, pane.to_string());
    }
    for text in ["ordinary", "literal @ {active} I"] {
        let command = CommandInvocation::new("display-message", ["-pl", text]);
        assert!(
            shared
                .display_message_client_alias(client, &command)
                .is_none()
        );
        assert_eq!(
            shared
                .execute(client, ClientKind::Interactive, &mut context, &command)
                .expect("literal display")
                .output,
            text
        );
    }
    for text in ["ordinary", "literal @ {active} I"] {
        let error = shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("display-message", ["-pZ", text]),
            )
            .expect_err("unknown option");
        assert!(error.to_string().contains("unknown flag -Z"), "{error}");
    }
}

#[cfg(unix)]
#[test]
fn a_remote_hello_attaches_without_rewriting_the_session_environment() {
    use std::os::unix::net::UnixStream;

    for (remote, expected) in [(false, "/caller/agent"), (true, "/host/agent")] {
        let shared = Arc::new(Shared::new(72));
        let mut context = ExecutionContext::default();
        for (name, args) in [
            ("new-session", ["-d", "-s", "remote-env"].as_slice()),
            (
                "set-environment",
                &["-t", "remote-env", "SSH_AUTH_SOCK", "/host/agent"],
            ),
        ] {
            shared
                .inner
                .lock()
                .engine
                .execute(
                    &mut context,
                    &CommandInvocation::new(name, args.iter().copied()),
                )
                .expect("model session");
        }
        let (mut client, server) = UnixStream::pair().expect("socket pair");
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read deadline");
        let server_shared = Arc::clone(&shared);
        let worker = thread::spawn(move || handle_connection(server, &server_shared));
        let mut hello = compact_hello(ClientKind::Interactive);
        if remote {
            hello
                .client
                .capabilities
                .push(ClientHello::CLIENT_REMOTE_CAPABILITY.to_owned());
        }
        hello.environment =
            ClientEnvironmentBlob::from_bytes(b"SSH_AUTH_SOCK=/caller/agent\0".to_vec());
        hello.subscriptions = zz_protocol::Subscriptions::terminal();
        hello.attach = Some(zz_protocol::AttachOperation::Session(
            "remote-env".to_owned(),
        ));
        zz_protocol::write_protocol_message(&mut client, &ProtocolMessage::Hello(hello))
            .expect("compact hello");
        assert!(matches!(
            zz_protocol::read_protocol_message(&mut client).expect("welcome"),
            ProtocolMessage::Welcome(_)
        ));
        zz_protocol::read_protocol_message(&mut client).expect("initial batch");
        drop(client);
        worker
            .join()
            .expect("connection thread")
            .expect("connection cleanup");
        let inner = shared.inner.lock();
        let session = inner
            .engine
            .resolve_session(Some("remote-env"), None)
            .expect("session");
        assert_eq!(
            inner
                .engine
                .environment_for_session(session)
                .expect("session environment")
                .into_iter()
                .find(|(name, _)| name == "SSH_AUTH_SOCK"),
            Some(("SSH_AUTH_SOCK".into(), Some(expected.into()))),
            "remote={remote}"
        );
    }
}

#[cfg(unix)]
#[test]
fn compact_startup_actor_follows_attachment_in_its_initial_batch() {
    use std::os::unix::net::UnixStream;

    let shared = Arc::new(Shared::new(71));
    let mut context = ExecutionContext::default();
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "startup-actor"]),
        )
        .expect("model session");
    *shared.startup_config_causes.lock() = Some(vec![
        "root.conf:1: INTERACTIVE_DIRECT".to_owned(),
        "child.conf:1: INTERACTIVE_NESTED".to_owned(),
    ]);
    let (mut client, server) = UnixStream::pair().expect("socket pair");
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read deadline");
    let server_shared = Arc::clone(&shared);
    let worker = thread::spawn(move || handle_connection(server, &server_shared));
    let mut hello = compact_hello(ClientKind::Interactive);
    hello.subscriptions = zz_protocol::Subscriptions::terminal();
    hello.viewport = Some(zz_protocol::ClientViewport {
        columns: 97,
        rows: 31,
        cell_width_px: 8,
        cell_height_px: 16,
    });
    hello.attach = Some(zz_protocol::AttachOperation::Session(
        "startup-actor".to_owned(),
    ));
    zz_protocol::write_protocol_message(&mut client, &ProtocolMessage::Hello(hello))
        .expect("compact hello");
    let welcome = zz_protocol::read_protocol_message(&mut client).expect("welcome");
    let initial = zz_protocol::read_protocol_message(&mut client).expect("initial batch");
    drop(client);
    worker
        .join()
        .expect("connection thread")
        .expect("connection cleanup");
    assert!(matches!(welcome, ProtocolMessage::Welcome(_)));
    let ProtocolMessage::Batch(batch) = &initial else {
        panic!("initial state must be one batch")
    };
    let messages = batch.messages().expect("flat initial batch");
    let viewport = messages
        .iter()
        .find_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::CommandOutput {
                        viewport: Some(viewport),
                        ..
                    },
                ..
            }) => Some(viewport),
            _ => None,
        })
        .expect("initial startup actor viewport");
    let text = viewport
        .cells
        .iter()
        .map(|cell| viewport.cell_text(*cell))
        .collect::<String>();
    assert_eq!(text.matches("root.conf:1: INTERACTIVE_DIRECT").count(), 1);
    assert_eq!(text.matches("child.conf:1: INTERACTIVE_NESTED").count(), 1);
    assert!(
        text.find("INTERACTIVE_DIRECT").expect("direct row")
            < text.find("INTERACTIVE_NESTED").expect("nested row")
    );
    let attached = messages
        .iter()
        .position(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ClientView(zz_protocol::ClientView {
                        session: Some(_),
                        ..
                    }),
                    ..
                })
            )
        })
        .expect("attached view");
    let output_positions = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::CommandOutput {
                        viewport: Some(_),
                        ..
                    },
                    ..
                })
            )
            .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(output_positions.len(), 1);
    assert!(attached < output_positions[0]);
    assert!(shared.startup_config_causes.lock().is_none());
}

#[cfg(unix)]
#[test]
fn compact_attach_sends_one_initial_batch_with_every_final_viewport() {
    assert_initial_compact_attach(false);
    assert_initial_compact_attach(true);
}

#[cfg(unix)]
fn assert_initial_compact_attach(commands: bool) {
    use std::os::unix::net::UnixStream;
    let shared = Arc::new(Shared::new(31));
    let mut context = ExecutionContext::default();
    compact_command(
        &shared,
        &mut context,
        "new-session",
        &[
            "-d",
            "-s",
            "ctrl-initial",
            "printf CTRL_INITIAL_MARKER; exec sleep 30",
        ],
    );
    for _ in 0..3 {
        compact_command(
            &shared,
            &mut context,
            "split-window",
            &[
                "-d",
                "-t",
                "ctrl-initial",
                "printf CTRL_INITIAL_MARKER; exec sleep 30",
            ],
        );
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ready = shared.inner.lock().terminals.values().all(|terminal| {
            let viewport = terminal.latest_viewport();
            viewport
                .cells
                .iter()
                .map(|cell| viewport.cell_text(*cell))
                .collect::<String>()
                .contains("CTRL_INITIAL_MARKER")
        });
        if ready {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "initial pane content did not arrive"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let (mut client, server) = UnixStream::pair().expect("pair");
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("timeout");
    let worker_shared = Arc::clone(&shared);
    let worker = thread::spawn(move || handle_connection(server, &worker_shared));
    let mut hello = compact_hello(ClientKind::Interactive);
    hello.subscriptions = zz_protocol::Subscriptions::terminal();
    hello.viewport = Some(zz_protocol::ClientViewport {
        columns: 97,
        rows: 31,
        cell_width_px: 8,
        cell_height_px: 16,
    });
    hello.attach = Some(if commands {
        zz_protocol::AttachOperation::Commands(
            [
                CommandInvocation::new("attach-session", ["-t", "ctrl-initial"]),
                CommandInvocation::new("set-option", ["-t", "ctrl-initial", "status", "2"]),
                CommandInvocation::new(
                    "set-option",
                    [
                        "-t",
                        "ctrl-initial",
                        "status-format[0]",
                        "CTRL_FINAL_#{client_width}x#{client_height}:#{pane_height}",
                    ],
                ),
                CommandInvocation::new(
                    "set-option",
                    ["-t", "ctrl-initial", "status-format[1]", "CTRL_SECOND"],
                ),
            ]
            .into_iter()
            .map(|invocation| PreparedCommand {
                canonical_name: Some(invocation.name.clone()),
                invocation,
                alias_matched: false,
                result: PreparedCommandResult::Ready,
            })
            .collect(),
        )
    } else {
        zz_protocol::AttachOperation::Session("ctrl-initial".to_owned())
    });
    zz_protocol::write_protocol_message(&mut client, &ProtocolMessage::Hello(hello))
        .expect("hello");
    assert!(matches!(
        zz_protocol::read_protocol_message(&mut client).expect("welcome"),
        ProtocolMessage::Welcome(_)
    ));
    let ProtocolMessage::Batch(batch) =
        zz_protocol::read_protocol_message(&mut client).expect("initial batch")
    else {
        panic!("initial state is not a batch")
    };
    let messages = batch.messages().expect("messages");
    let statuses = messages
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::StatusChanged { status },
                ..
            }) => Some(status),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        statuses.len(),
        1,
        "commands={commands}, initial status was rendered before the final command: {statuses:?}"
    );
    if commands {
        let inner = shared.inner.lock();
        let client = inner
            .clients
            .iter()
            .find_map(|(id, client)| client.subscriber.as_ref().map(|_| id))
            .copied()
            .expect("attached client");
        let pane = client_context_pane(&inner, client).expect("active pane");
        let (_, height) = inner.engine.pane_geometry(pane).expect("active geometry");
        assert_eq!(
            statuses[0].rows,
            [
                format!("CTRL_FINAL_97x31:{height}"),
                "CTRL_SECOND".to_owned()
            ]
        );
    }
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Snapshot(_),
                    ..
                })
            ))
            .count(),
        1
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ClientView(zz_protocol::ClientView {
                        session: Some(_),
                        ..
                    }),
                    ..
                })
            ))
            .count(),
        1,
        "{messages:?}"
    );
    let viewports = messages
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::TerminalViewport { pane, viewport },
                ..
            }) => Some((*pane, viewport)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        viewports.len(),
        4,
        "initial batch must include all visible pane frames: {messages:?}"
    );
    for (pane, viewport) in viewports {
        let geometry = shared
            .inner
            .lock()
            .engine
            .pane_geometry(pane)
            .expect("geometry");
        assert_eq!((viewport.columns, viewport.rows), geometry);
        assert!(
            viewport
                .cells
                .iter()
                .map(|cell| viewport.cell_text(*cell))
                .collect::<String>()
                .contains("CTRL_INITIAL_MARKER")
        );
    }
    drop(client);
    worker
        .join()
        .expect("connection thread")
        .expect("connection");
    compact_command(
        &shared,
        &mut context,
        "kill-session",
        &["-t", "ctrl-initial"],
    );
}

#[test]
fn forced_attachment_stays_after_an_older_pending_publication() {
    let shared = Arc::new(Shared::new(39));
    let mut context = ExecutionContext::default();
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "ctrl-order"]),
        )
        .expect("model session");
    let session = context.session.expect("session");
    let (client, mailbox) = compact_registered(
        &shared,
        zz_protocol::Subscriptions {
            tree: zz_protocol::TreeSubscription::All,
            ..zz_protocol::Subscriptions::control()
        },
    );
    shared
        .inner
        .lock()
        .client_entry(client)
        .kind
        .replace(ClientKind::Control);
    let publication_lock = shared.snapshot_order.lock();
    let older = shared.compact_tree_messages(client, true);
    {
        let mut inner = shared.inner.lock();
        inner
            .attachment
            .attached
            .entry(session)
            .or_default()
            .insert(client);
        inner.client_entry(client).ctrl_attachment.replace(1);
    }
    let (started, ready) = crossbeam_channel::bounded(1);
    let (finished, completion) = crossbeam_channel::bounded(1);
    let force = {
        let shared = Arc::clone(&shared);
        let mailbox = Arc::clone(&mailbox);
        thread::spawn(move || {
            started.send(()).expect("start forced attachment");
            shared.send_compact_state(client, &mailbox, true);
            finished.send(()).expect("finish forced attachment");
        })
    };
    ready
        .recv_timeout(Duration::from_secs(2))
        .expect("forced sender started");
    thread::sleep(Duration::from_millis(20));
    let still_unattached = shared.inner.lock().clients[&client]
        .ctrl_view
        .as_ref()
        .unwrap()
        .session
        .is_none();
    let older = older
        .iter()
        .map(|message| {
            zz_protocol::encode_protocol_message(message)
                .expect("old publication")
                .into()
        })
        .collect();
    assert!(mailbox.enqueue_control_group(older));
    drop(publication_lock);
    completion
        .recv_timeout(Duration::from_secs(2))
        .expect("forced attachment completed");
    force.join().expect("forced attachment");
    let views = reliable_children(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientView(view),
                ..
            }) => Some(view),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        still_unattached,
        "forced attachment advanced the cursor before the older publication queued"
    );
    assert_eq!(views.len(), 2, "{views:?}");
    assert!(views[0].session.is_none());
    assert!(
        views[1..]
            .iter()
            .all(|view| view.session == Some(session) && view.attachment_generation == 1),
        "{views:?}"
    );
}
