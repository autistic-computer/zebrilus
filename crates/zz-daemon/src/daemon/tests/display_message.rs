use super::*;

#[test]
fn display_message_is_published_only_to_the_requesting_client() {
    let shared = Arc::new(Shared::new(1));
    let requesting_mailbox = OutboundMailbox::new();
    let observing_mailbox = OutboundMailbox::new();
    let (requesting, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&requesting_mailbox),
    );
    let (_observing, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&observing_mailbox),
    );
    take_reliable_messages(&requesting_mailbox);
    take_reliable_messages(&observing_mailbox);

    shared
        .execute(
            requesting,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("display-message", ["hello client"]),
        )
        .expect("display message");

    assert!(
        take_reliable_messages(&requesting_mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::TimedClientMessage {
                        pane: None,
                        kind: ClientMessageKind::Info,
                        text,
                        duration_ms: 750,
                        ..
                    },
                    ..
                }) if text == "hello client"
            ))
    );
    assert!(
        take_reliable_messages(&observing_mailbox)
            .iter()
            .all(|message| !matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::TimedClientMessage { .. },
                    ..
                })
            ))
    );
}

#[test]
fn display_message_target_client_routes_and_selects_format_client() {
    let shared = Arc::new(Shared::new(1));
    let caller_mailbox = OutboundMailbox::new();
    let same_mailbox = OutboundMailbox::new();
    let fallback_mailbox = OutboundMailbox::new();
    let cross_mailbox = OutboundMailbox::new();
    let control_mailbox = OutboundMailbox::new();
    let (caller, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("caller-client".to_owned()),
        None,
        Arc::clone(&caller_mailbox),
    );
    let (same, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("same-client-one".to_owned()),
        None,
        Arc::clone(&same_mailbox),
    );
    let (fallback, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("same-client-two".to_owned()),
        None,
        Arc::clone(&fallback_mailbox),
    );
    let (cross, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("cross-client".to_owned()),
        None,
        Arc::clone(&cross_mailbox),
    );
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control-client".to_owned()),
        None,
        Arc::clone(&control_mailbox),
    );
    let (target_session, target_pane, _) = attached_message_fixture(
        &shared,
        "target-session",
        &[caller, same, fallback, control],
    );
    let (cross_session, _, _) = attached_message_fixture(&shared, "cross-session", &[cross]);
    {
        let mut inner = shared.inner.lock();
        inner.client_entry(caller).activity.replace(10);
        inner.client_entry(same).activity.replace(20);
        inner.client_entry(control).activity.replace(30);
        inner.client_entry(fallback).activity.replace(100);
        inner.client_entry(same).size.replace((101, 31));
        inner.client_entry(fallback).size.replace((88, 22));
        inner.client_entry(cross).size.replace((77, 17));
        inner
            .client_entry(same)
            .tty
            .replace("/dev/pts/7".to_owned());
    }
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, target_pane)
        .expect("target context");
    for mailbox in [
        &caller_mailbox,
        &same_mailbox,
        &fallback_mailbox,
        &cross_mailbox,
        &control_mailbox,
    ] {
        take_reliable_messages(mailbox);
    }
    let format = "#{client_name}|#{client_session}|#{client_width}x#{client_height}|#{session_name}|#{pane_id}|#{session_active}";

    let implicit_output = shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                ["-p", "-t", &target_pane.to_string(), format],
            ),
        )
        .expect("print with implicit target-session client facts");
    assert_eq!(
        implicit_output.output,
        format!("same-client-two|target-session|88x22|target-session|{target_pane}|1")
    );
    shared.inner.lock().client_entry(same).activity.replace(100);
    let tied_output = shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                ["-p", "-t", &target_pane.to_string(), format],
            ),
        )
        .expect("print with tied target-session client activity");
    assert_eq!(
        tied_output.output,
        format!("/dev/pts/7|target-session|101x31|target-session|{target_pane}|1")
    );
    shared.inner.lock().client_entry(same).activity.replace(20);

    let same_output = shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                [
                    "-p",
                    "-c",
                    "same-client-one:",
                    "-t",
                    &target_pane.to_string(),
                    format,
                ],
            ),
        )
        .expect("print with same-session client facts");
    assert_eq!(
        same_output.output,
        format!("/dev/pts/7|target-session|101x31|target-session|{target_pane}|1")
    );

    let partial_output = shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                [
                    "-p",
                    "-c",
                    "cross-client",
                    "-t",
                    "=target-session:missing-window",
                    format,
                ],
            ),
        )
        .expect("print with partial target context");
    assert_eq!(
        partial_output.output,
        format!("same-client-two|target-session|88x22|target-session|{target_pane}|1")
    );

    for target in ["cross-client", "missing-client"] {
        let output = shared
            .execute(
                caller,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new(
                    "display-message",
                    ["-p", "-c", target, "-t", &target_pane.to_string(), format],
                ),
            )
            .expect("print with best target-session client facts");
        assert_eq!(
            output.output,
            format!("same-client-two|target-session|88x22|target-session|{target_pane}|1")
        );
    }
    assert_eq!(
        client_attached_session(&shared.inner.lock(), fallback),
        Some(target_session)
    );

    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-c", "same-client", "ambiguous prefix"]),
        )
        .expect("quiet unmatched client prefix");
    for mailbox in [&caller_mailbox, &same_mailbox, &fallback_mailbox] {
        assert!(take_timed_client_messages(mailbox).is_empty());
    }
    {
        let inner = shared.inner.lock();
        assert!(inner.client(caller).is_none_or(|c| c.message.is_none()));
        assert!(inner.client(same).is_none_or(|c| c.message.is_none()));
        assert!(inner.client(fallback).is_none_or(|c| c.message.is_none()));
    }

    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                ["-c", "missing-client", "-t", "=missing:", "message"],
            ),
        )
        .expect("missing pane and client stay quiet");
    for mailbox in [&caller_mailbox, &same_mailbox, &fallback_mailbox] {
        assert!(take_timed_client_messages(mailbox).is_empty());
    }
    let missing_output = shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                [
                    "-p",
                    "-c",
                    "missing-client",
                    "-t",
                    "=missing:",
                    "#{session_name}|#{window_name}|#{pane_id}|#{client_name}|#{session_active}",
                ],
            ),
        )
        .expect("print empty facts for missing targets");
    assert_eq!(missing_output.output, "||||");
    let error = shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                ["-c", "missing-client", "-d", "not-a-delay", "message"],
            ),
        )
        .expect_err("delay error follows quiet client miss");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message)) if message == "delay invalid"
    ));

    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                [
                    "-p",
                    "-N",
                    "-c",
                    "cross-client",
                    "-t",
                    &target_pane.to_string(),
                    format,
                ],
            ),
        )
        .expect("printing remains non-displaying");
    assert!(!shared.read_client(cross, |c| c.is_some_and(|c| c.message_ignore_keys)));
    assert!(take_timed_client_messages(&cross_mailbox).is_empty());

    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-t", "=target-session:", "display-time", "1111"],
            ),
        )
        .expect("set pane-target display time");
    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-option", ["-t", "=cross-session:", "display-time", "0"]),
        )
        .expect("set destination display time to zero");
    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                [
                    "-N",
                    "-c",
                    "cross-client",
                    "-t",
                    &target_pane.to_string(),
                    "destination-zero",
                ],
            ),
        )
        .expect("inherit destination zero duration");
    assert_eq!(
        take_timed_client_messages(&cross_mailbox),
        [(Some(target_pane), "destination-zero".to_owned(), 0)]
    );
    {
        let inner = shared.inner.lock();
        assert_eq!(client_attached_session(&inner, cross), Some(cross_session));
        assert_eq!(
            inner.clients[&cross].message.as_ref().unwrap().deadline,
            None
        );
        assert!(!inner.client(cross).is_some_and(|c| c.message_ignore_keys));
    }
    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-t", "=cross-session:", "display-time", "4321"],
            ),
        )
        .expect("set positive destination display time");
    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                [
                    "-N",
                    "-c",
                    "cross-client",
                    "-t",
                    &target_pane.to_string(),
                    "destination-positive",
                ],
            ),
        )
        .expect("inherit positive destination duration");
    assert_eq!(
        take_timed_client_messages(&cross_mailbox),
        [(Some(target_pane), "destination-positive".to_owned(), 4321)]
    );
    {
        let inner = shared.inner.lock();
        assert!(
            inner.clients[&cross]
                .message
                .as_ref()
                .unwrap()
                .deadline
                .is_some()
        );
        assert!(inner.client(cross).is_some_and(|c| c.message_ignore_keys));
    }

    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                [
                    "-C",
                    "-N",
                    "-c",
                    "cross-client",
                    "-d",
                    "5000",
                    "-t",
                    &target_pane.to_string(),
                    "#{client_name}|#{client_session}|#{session_name}|#{session_active}",
                ],
            ),
        )
        .expect("route cross-session message");
    assert!(take_timed_client_messages(&caller_mailbox).is_empty());
    assert_eq!(
        take_timed_client_messages(&cross_mailbox),
        [(
            Some(target_pane),
            "same-client-two|target-session|target-session|1".to_owned(),
            5000
        )]
    );
    {
        let inner = shared.inner.lock();
        assert!(inner.client(caller).is_none_or(|c| c.message.is_none()));
        let armed = *inner.clients[&cross].message.as_ref().unwrap();
        assert!(!armed.freeze);
        assert!(armed.deadline.is_some());
        assert!(inner.client(cross).is_some_and(|c| c.message_ignore_keys));
    }

    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                ["-c", "cross-client", "-d", "5000", "reset"],
            ),
        )
        .expect("reset targeted ignore-keys state");
    {
        let inner = shared.inner.lock();
        assert!(inner.clients[&cross].message.as_ref().unwrap().freeze);
        assert!(!inner.client(cross).is_some_and(|c| c.message_ignore_keys));
    }
    take_timed_client_messages(&cross_mailbox);

    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                ["-c", "control-client", "-d", "0", "control-target"],
            ),
        )
        .expect("route control message");
    assert_eq!(
        take_timed_client_messages(&control_mailbox),
        [(Some(target_pane), "control-target".to_owned(), 0)]
    );
    {
        let inner = shared.inner.lock();
        assert!(inner.client(control).is_none_or(|c| c.message.is_none()));
        assert!(!inner.client(control).is_some_and(|c| c.message_ignore_keys));
    }

    {
        shared.inner.lock().attachment.client_flags.insert(same);
    }
    shared
        .execute(
            caller,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                ["-c", "pts/7:", "-d", "5000", "read-only-target"],
            ),
        )
        .expect("route message to read-only destination");
    assert_eq!(
        take_timed_client_messages(&same_mailbox),
        [(Some(target_pane), "read-only-target".to_owned(), 5000)]
    );
    let same_message = *shared.inner.lock().clients[&same].message.as_ref().unwrap();

    shared.inner.lock().attachment.client_flags.insert(caller);
    let response = shared.execute_command_request(
        caller,
        ClientKind::Interactive,
        &mut context,
        41,
        &CommandInvocation::new("display-message", ["-c", "same-client-one", "blocked"]),
    );
    assert!(matches!(
        response,
        CommandResponse::Error {
            request_id: 41,
            error: ServerError::InvalidCommand(message),
            output,
        } if message == "client is read-only" && output.is_empty()
    ));
    assert_eq!(
        (*shared.inner.lock().clients[&same].message.as_ref().unwrap()),
        same_message
    );
    assert!(take_timed_client_messages(&same_mailbox).is_empty());
}

#[test]
fn display_message_unattached_target_uses_global_format_client() {
    let shared = Arc::new(Shared::new(1));
    let alpha_mailbox = OutboundMailbox::new();
    let beta_mailbox = OutboundMailbox::new();
    let (alpha, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("alpha-client".to_owned()),
        None,
        Arc::clone(&alpha_mailbox),
    );
    let (beta, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("beta-client".to_owned()),
        None,
        Arc::clone(&beta_mailbox),
    );
    attached_message_fixture(&shared, "attached-alpha", &[alpha]);
    attached_message_fixture(&shared, "attached-beta", &[beta]);
    let (_, target_pane, _) = attached_message_fixture(&shared, "unattached-target", &[]);
    {
        let mut inner = shared.inner.lock();
        inner.client_entry(alpha).activity.replace(10);
        inner.client_entry(beta).activity.replace(20);
    }
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, target_pane)
        .expect("unattached target context");
    let format =
        "#{client_name}|#{client_session}|#{session_name}|#{pane_id}|#{session_active}".to_owned();

    for target_client in [None, Some("alpha-client"), Some("missing-client")] {
        let mut arguments = vec!["-p".to_owned()];
        if let Some(target_client) = target_client {
            arguments.extend(["-c".to_owned(), target_client.to_owned()]);
        }
        arguments.extend(["-t".to_owned(), target_pane.to_string(), format.clone()]);
        let output = shared
            .execute(
                alpha,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("display-message", arguments),
            )
            .expect("print with global format client");
        assert_eq!(
            output.output,
            format!("beta-client|attached-beta|unattached-target|{target_pane}|0")
        );
    }

    shared.inner.lock().client_entry(alpha).activity.replace(30);
    for target_client in [None, Some("beta-client"), Some("missing-client")] {
        let mut arguments = vec!["-p".to_owned()];
        if let Some(target_client) = target_client {
            arguments.extend(["-c".to_owned(), target_client.to_owned()]);
        }
        arguments.extend(["-t".to_owned(), target_pane.to_string(), format.clone()]);
        let output = shared
            .execute(
                beta,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("display-message", arguments),
            )
            .expect("print after global client activity change");
        assert_eq!(
            output.output,
            format!("alpha-client|attached-alpha|unattached-target|{target_pane}|0")
        );
    }

    let no_clients = Arc::new(Shared::new(1));
    let (_, solo_pane, _) = attached_message_fixture(&no_clients, "unattached-solo", &[]);
    let mut solo_context =
        ExecutionContext::for_pane(&no_clients.inner.lock().engine.state, solo_pane)
            .expect("solo target context");
    for target_client in [None, Some("missing-client")] {
        let mut arguments = vec!["-p".to_owned()];
        if let Some(target_client) = target_client {
            arguments.extend(["-c".to_owned(), target_client.to_owned()]);
        }
        arguments.extend([
            "-t".to_owned(),
            solo_pane.to_string(),
            "#{client_name}|#{client_session}|#{session_name}|#{pane_id}|#{session_active}"
                .to_owned(),
        ]);
        let output = no_clients
            .execute(
                ClientId(u64::MAX - 1),
                ClientKind::Command,
                &mut solo_context,
                &CommandInvocation::new("display-message", arguments),
            )
            .expect("print without attached clients");
        assert_eq!(output.output, format!("||unattached-solo|{solo_pane}|"));
    }
}

#[test]
fn list_keys_single_routes_to_the_selected_client_or_stdout() {
    let shared = Arc::new(Shared::new(1));
    let interactive_mailbox = OutboundMailbox::new();
    let control_mailbox = OutboundMailbox::new();
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("interactive-client".to_owned()),
        None,
        Arc::clone(&interactive_mailbox),
    );
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control-client".to_owned()),
        None,
        Arc::clone(&control_mailbox),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "list-keys-single"]),
        )
        .expect("create list-keys session");
    let session = context.session.expect("session");
    shared
        .attach(interactive, session)
        .expect("attach interactive");
    shared.attach(control, session).expect("attach control");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["display-time", "1234"]),
        )
        .expect("set display time");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("bind-key", ["-T", "zzlk", "a", "display-message", "unused"]),
        )
        .expect("bind list key");
    take_reliable_messages(&interactive_mailbox);
    take_reliable_messages(&control_mailbox);
    let command = CommandInvocation::new(
        "list-keys",
        [
            "-1",
            "-T",
            "zzlk",
            "-F",
            "#{client_name}|#{session_active}|#{key_table}|#{key_string}|#{key_repeat}",
        ],
    );

    let interactive_execution = shared
        .execute(interactive, ClientKind::Interactive, &mut context, &command)
        .expect("interactive list keys");
    assert!(interactive_execution.output.is_empty());
    assert!(
        take_reliable_messages(&interactive_mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::TimedClientMessage {
                        pane: Some(_),
                        text,
                        duration_ms: 1234,
                        ..
                    },
                    ..
                }) if text == "interactive-client|1|zzlk|a|0"
            ))
    );
    {
        let inner = shared.inner.lock();
        assert!(inner.clients[&interactive].message.as_ref().unwrap().freeze);
        assert!(
            inner
                .client(interactive)
                .is_none_or(|c| c.command_output.is_none())
        );
    }

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                ["-n", "C-g", "list-keys", "-1", "-T", "zzlk", "-F", "BOUND"],
            ),
        )
        .expect("bind interactive list keys");
    take_reliable_messages(&interactive_mailbox);
    let pane = context.pane.expect("pane");
    shared
        .input(
            interactive,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(
                    KeyCode::Character('g'),
                    Modifiers::new(false, true, false, false),
                    None,
                ),
                text_follows: false,
            },
        )
        .expect("run bound list keys");
    assert!(
        take_reliable_messages(&interactive_mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::TimedClientMessage { text, .. },
                    ..
                }) if text == "BOUND"
            ))
    );

    let command_execution = shared
        .execute(
            ClientId(u64::MAX - 1),
            ClientKind::Command,
            &mut context,
            &command,
        )
        .expect("command list keys");
    assert!(command_execution.output.is_empty());
    let command_messages = take_reliable_messages(&interactive_mailbox);
    assert!(command_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::TimedClientMessage { text, .. },
            ..
        }) if text == "interactive-client|1|zzlk|a|0"
    )));
    assert!(command_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::TimedClientMessageCleared { .. },
            ..
        })
    )));

    let control_execution = shared
        .execute(control, ClientKind::Control, &mut context, &command)
        .expect("control list keys");
    assert_eq!(control_execution.output, "control-client|1|zzlk|a|0");
    assert!(shared.read_client(control, |c| c.is_none_or(|c| c.message.is_none())));
    assert!(
        take_reliable_messages(&control_mailbox)
            .iter()
            .all(|message| !matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::TimedClientMessage { .. },
                    ..
                })
            ))
    );
}

#[test]
fn timed_message_ids_increase_monotonically() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);

    for text in ["first", "second"] {
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("display-message", [text]),
            )
            .expect("display message");
    }

    let ids = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::TimedClientMessage { message_id, .. },
                ..
            }) => Some(message_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, [1, 2]);
}

#[test]
fn stale_owned_publication_cannot_outlive_the_authoritative_message() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "owned-race", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    take_reliable_messages(&mailbox);

    let (first, second) = {
        let mut inner = shared.inner.lock();
        let first_id = next_timed_message_id(&mut inner);
        arm_client_message(&mut inner, client, first_id, 0, true);
        let first = OwnedClientMessagePublication {
            client,
            pane: Some(pane),
            kind: ClientMessageKind::Info,
            text: "first".to_owned(),
            duration_ms: 0,
            message_id: first_id,
        };
        let second_id = next_timed_message_id(&mut inner);
        arm_client_message(&mut inner, client, second_id, 0, true);
        let second = OwnedClientMessagePublication {
            client,
            pane: Some(pane),
            kind: ClientMessageKind::Info,
            text: "second".to_owned(),
            duration_ms: 0,
            message_id: second_id,
        };
        (first, second)
    };
    let second_id = second.message_id;
    shared.publish_owned_client_message(second);
    shared.publish_owned_client_message(first);
    let visible = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::TimedClientMessage {
                        text, message_id, ..
                    },
                ..
            }) => Some((text, message_id)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(visible, [("second".to_owned(), second_id)]);
    assert_eq!(armed_client_message(&shared, client).token, second_id);

    shared.dismiss_client_message(client);
    assert_eq!(
        cleared_message_ids(&take_reliable_messages(&mailbox)),
        [second_id]
    );
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.message.is_none())));

    shared.inner.lock().attachment.client_flags.insert(client);
    shared
        .execute_key_commands(
            client,
            ClientKind::Interactive,
            &mut context,
            pane,
            &[CommandInvocation::new("rename-session", ["blocked"])],
            false,
        )
        .expect("reject read-only command");
    assert!(
        take_reliable_messages(&mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::TimedClientMessage {
                        kind: ClientMessageKind::Error,
                        text,
                        ..
                    },
                    ..
                }) if text == "client is read-only"
            ))
    );
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.message.is_none())));
}

#[test]
fn mixed_command_alert_chain_clears_every_predecessor_identity() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "predecessor-chain", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    for arguments in [
        ["-g", "display-time", "0"],
        ["-g", "bell-action", "any"],
        ["-g", "visual-bell", "on"],
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-option", arguments),
            )
            .expect("configure alert");
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "0", "old"]),
        )
        .expect("publish old message");
    let old = armed_client_message(&shared, client);
    take_reliable_messages(&mailbox);

    let (unpublished, predecessor) = {
        let mut inner = shared.inner.lock();
        let message_id = next_timed_message_id(&mut inner);
        let (predecessor, deadline) = arm_client_message(&mut inner, client, message_id, 0, true);
        assert!(deadline.is_none());
        let unpublished = OwnedClientMessagePublication {
            client,
            pane: Some(pane),
            kind: ClientMessageKind::Info,
            text: "unpublished".to_owned(),
            duration_ms: 0,
            message_id,
        };
        (unpublished, predecessor.expect("old predecessor"))
    };
    let unpublished_id = unpublished.message_id;
    assert_eq!(predecessor, old);

    shared.raise_pane_bell(pane);
    let current = armed_client_message(&shared, client);
    shared.publish_owned_client_message(unpublished);
    shared.retire_client_message(client, predecessor, true);
    assert_eq!(
        message_lifecycle_events(&take_reliable_messages(&mailbox)),
        [
            (true, current.token),
            (false, unpublished_id),
            (false, old.token)
        ]
    );
    assert_eq!(armed_client_message(&shared, client), current);

    shared.dismiss_client_message(client);
    assert_eq!(
        cleared_message_ids(&take_reliable_messages(&mailbox)),
        [current.token]
    );
}

#[test]
fn a_replaced_message_survives_the_deadline_it_replaced() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("display-message", ["-d", "5000", "first"]),
        )
        .expect("first message");
    let first = armed_client_message(&shared, client);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("display-message", ["-d", "5000", "second"]),
        )
        .expect("second message");
    let second = armed_client_message(&shared, client);
    assert_ne!(first.token, second.token);
    assert_eq!(
        cleared_message_ids(&take_reliable_messages(&mailbox)),
        [first.token]
    );

    let stale = ClientMessageDeadline {
        client,
        token: first.token,
        deadline: first.deadline.expect("first message armed a deadline"),
    };
    shared.expire_client_message(stale, Instant::now());
    assert_eq!(armed_client_message(&shared, client), second);
    assert!(cleared_message_ids(&take_reliable_messages(&mailbox)).is_empty());

    let live = ClientMessageDeadline {
        client,
        token: second.token,
        deadline: second.deadline.expect("second message armed a deadline"),
    };
    shared.expire_client_message(live, live.deadline);
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.message.is_none())));
    assert_eq!(
        cleared_message_ids(&take_reliable_messages(&mailbox)),
        vec![second.token]
    );
}

#[test]
fn an_ordinary_message_freezes_only_the_requesting_client() {
    let shared = Arc::new(Shared::new(1));
    let frozen_mailbox = OutboundMailbox::new();
    let live_mailbox = OutboundMailbox::new();
    let (frozen, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&frozen_mailbox),
    );
    let (live, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&live_mailbox),
    );
    let (session, pane, terminal) = attached_message_fixture(&shared, "freeze", &[frozen, live]);

    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    shared
        .execute(
            frozen,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "5000", "frozen"]),
        )
        .expect("display message");
    let armed = armed_client_message(&shared, frozen);
    assert!(armed.freeze);
    {
        let inner = shared.inner.lock();
        assert!(client_terminal_publication_frozen(&inner, frozen));
        assert!(!client_terminal_publication_frozen(&inner, live));
    }

    let viewport = terminal
        .latest_viewport_for(TerminalViewId(frozen.0))
        .unwrap_or_else(|| terminal.latest_viewport());
    drain_terminal_lane(&frozen_mailbox);
    drain_terminal_lane(&live_mailbox);
    shared.publish_terminal_for_pane(
        pane,
        frozen,
        None,
        &viewport,
        &terminal,
        &mut PaneFrameFanout::new(),
    );
    shared.send_full(frozen, pane, &frozen_mailbox);
    shared.send_resync(frozen, &frozen_mailbox);
    shared.publish_terminal_for_pane(
        pane,
        live,
        None,
        &viewport,
        &terminal,
        &mut PaneFrameFanout::new(),
    );
    assert!(frozen_mailbox.state.lock().terminals.is_empty());
    assert!(live_mailbox.state.lock().terminals.contains_key(&pane));

    drain_terminal_lane(&frozen_mailbox);
    take_reliable_messages(&frozen_mailbox);
    let live_deadline = ClientMessageDeadline {
        client: frozen,
        token: armed.token,
        deadline: armed.deadline.expect("armed deadline"),
    };
    shared.expire_client_message(live_deadline, live_deadline.deadline);
    assert_eq!(
        cleared_message_ids(&take_reliable_messages(&frozen_mailbox)),
        vec![armed.token]
    );
    assert!(
        frozen_mailbox.state.lock().terminals.contains_key(&pane),
        "clearing a frozen message publishes one full latest viewport"
    );
    assert!(!client_terminal_publication_frozen(
        &shared.inner.lock(),
        frozen
    ));

    drain_terminal_lane(&frozen_mailbox);
    shared.publish_terminal_for_pane(
        pane,
        frozen,
        None,
        &viewport,
        &terminal,
        &mut PaneFrameFanout::new(),
    );
    assert!(frozen_mailbox.state.lock().terminals.contains_key(&pane));
    let _ = session;
}

#[test]
fn display_message_dash_c_never_freezes_and_still_times_out() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, terminal) = attached_message_fixture(&shared, "no-freeze", &[client]);

    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-C", "-d", "5000", "flowing"]),
        )
        .expect("display message");
    let armed = armed_client_message(&shared, client);
    assert!(!armed.freeze);
    assert!(armed.deadline.is_some());
    assert!(!client_terminal_publication_frozen(
        &shared.inner.lock(),
        client
    ));

    let viewport = terminal
        .latest_viewport_for(TerminalViewId(client.0))
        .unwrap_or_else(|| terminal.latest_viewport());
    drain_terminal_lane(&mailbox);
    shared.publish_terminal_for_pane(
        pane,
        client,
        None,
        &viewport,
        &terminal,
        &mut PaneFrameFanout::new(),
    );
    assert!(mailbox.state.lock().terminals.contains_key(&pane));
}

#[test]
fn a_zero_duration_message_installs_no_timer_and_a_key_clears_it() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "zero-delay", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "0", "pinned"]),
        )
        .expect("display message");
    let armed = armed_client_message(&shared, client);
    assert_eq!(armed.deadline, None);
    assert!(armed.freeze);
    take_reliable_messages(&mailbox);

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: chooser_key_press("q"),
                text_follows: false,
            },
        )
        .expect("deliver key");
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.message.is_none())));
    assert_eq!(
        cleared_message_ids(&take_reliable_messages(&mailbox)),
        vec![armed.token]
    );
}

#[test]
fn display_message_ignore_keys_is_sticky_only_after_positive_interactive_messages() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let control_mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    let (session, pane, _) = attached_message_fixture(&shared, "ignore-sticky", &[client]);
    shared
        .attach(control, session)
        .expect("attach control client");
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("message context");

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-option", ["display-time", "0"]),
        )
        .expect("set inherited zero duration");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-N", "inherited zero"]),
        )
        .expect("display inherited zero message");
    assert_eq!(armed_client_message(&shared, client).deadline, None);
    assert!(!shared.read_client(client, |c| c.is_some_and(|c| c.message_ignore_keys)));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-N", "-d", "5000", "ignore"]),
        )
        .expect("enable ignored keys");
    let ignored = armed_client_message(&shared, client);
    assert!(shared.read_client(client, |c| c.is_some_and(|c| c.message_ignore_keys)));
    let deadline = ignored.deadline.expect("positive message deadline");
    shared.expire_client_message(
        ClientMessageDeadline {
            client,
            token: ignored.token,
            deadline,
        },
        deadline,
    );
    {
        let inner = shared.inner.lock();
        assert!(inner.client(client).is_none_or(|c| c.message.is_none()));
        assert!(inner.client(client).is_some_and(|c| c.message_ignore_keys));
    }

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "0", "plain zero"]),
        )
        .expect("display explicit zero message");
    assert!(shared.read_client(client, |c| c.is_some_and(|c| c.message_ignore_keys)));
    shared.dismiss_client_message(client);
    {
        let inner = shared.inner.lock();
        assert!(inner.client(client).is_none_or(|c| c.message.is_none()));
        assert!(inner.client(client).is_some_and(|c| c.message_ignore_keys));
    }

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "5000", "reset"]),
        )
        .expect("reset ignored keys");
    assert!(!shared.read_client(client, |c| c.is_some_and(|c| c.message_ignore_keys)));
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-N", "-d", "5000", "retained"]),
        )
        .expect("restore ignored keys");
    let retained = armed_client_message(&shared, client);

    let printed = shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-p", "-N", "-d", "5000", "printed"]),
        )
        .expect("print ignored message");
    assert_eq!(printed.output, "printed");
    assert_eq!(armed_client_message(&shared, client), retained);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                ["-N", "-t", "=missing-ignore-client", "missing"],
            ),
        )
        .expect("missing pane target is canfail");
    let canfail = armed_client_message(&shared, client);
    assert_ne!(canfail.token, retained.token);
    assert_eq!(canfail.deadline, None);
    assert!(
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("display-message", ["-N", "-I", "input"]),
            )
            .is_err()
    );
    {
        let inner = shared.inner.lock();
        assert_eq!((*inner.clients[&client].message.as_ref().unwrap()), canfail);
        assert!(inner.client(client).is_some_and(|c| c.message_ignore_keys));
    }

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                ["-T", "ignore-list", "k", "display-message", "listed"],
            ),
        )
        .expect("bind listed key");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("list-keys", ["-1", "-T", "ignore-list"]),
        )
        .expect("display zero-duration list result");
    assert!(shared.read_client(client, |c| c.is_some_and(|c| c.message_ignore_keys)));
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-option", ["display-time", "5000"]),
        )
        .expect("set positive inherited duration");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("list-keys", ["-1", "-T", "ignore-list"]),
        )
        .expect("display positive-duration list result");
    assert!(!shared.read_client(client, |c| c.is_some_and(|c| c.message_ignore_keys)));

    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("display-message", ["-N", "-d", "5000", "control"]),
        )
        .expect("control display message");
    assert!(!shared.read_client(control, |c| c.is_some_and(|c| c.message_ignore_keys)));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-N", "-d", "5000", "detach"]),
        )
        .expect("restore ignored keys before detach");

    shared.detach(client);
    assert!(shared.read_client(client, |c| c.is_some_and(|c| c.message_ignore_keys)));
    shared.unregister(client);
    assert!(!shared.read_client(client, |c| c.is_some_and(|c| c.message_ignore_keys)));
}

#[test]
fn display_message_ignore_keys_preempts_writable_terminal_input() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (session, pane, _) = attached_message_fixture(&shared, "ignore-input", &[client]);
    let mut context = ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane)
        .expect("message context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("bind-key", ["-n", "x", "rename-session", "dispatched"]),
        )
        .expect("bind ignored key");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "browser", "about:blank"]),
        )
        .expect("create browser pane");
    let browser = context.pane.expect("browser pane");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "focus-events", "on"]),
        )
        .expect("enable focus events");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-panes", ["-d", "0"]),
        )
        .expect("open display panes");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-N", "-d", "5000", "ignore input"]),
        )
        .expect("arm ignored input message");
    let armed = armed_client_message(&shared, client);
    let unchanged = {
        let inner = shared.inner.lock();
        (
            inner.attachment.activity_sequence,
            inner.panes.terminal_input_sequence,
            inner.engine.state.sessions[&session].sort_activity,
            inner.clients[&client].display_panes.as_ref().unwrap().token,
        )
    };
    take_reliable_messages(&mailbox);

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: chooser_key_press("x"),
                text_follows: true,
            },
        )
        .expect("ignore paired key");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.state.sessions[&session].name, "ignore-input");
        assert_eq!(
            (
                inner.attachment.activity_sequence,
                inner.panes.terminal_input_sequence,
                inner.engine.state.sessions[&session].sort_activity,
                inner.clients[&client].display_panes.as_ref().unwrap().token,
            ),
            unchanged
        );
        assert_eq!(
            inner.clients[&client]
                .pending_committed_text
                .as_ref()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            (*inner.clients[&client].suppressed_text.as_ref().unwrap())[&'x'],
            1
        );
    }
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane,
                text: "x".to_owned(),
            },
        )
        .expect("ignore paired text");
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.pending_committed_text.is_none())
        );
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.suppressed_text.is_none())
        );
    }
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: chooser_key_press("y"),
                text_follows: true,
            },
        )
        .expect("ignore empty paired key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane,
                text: String::new(),
            },
        )
        .expect("ignore empty paired text");
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.pending_committed_text.is_none())
        );
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.suppressed_text.is_none())
        );
    }
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: chooser_key_press("z"),
                text_follows: false,
            },
        )
        .expect("ignore key without committed text");
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.pending_committed_text.is_none())
        );
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.suppressed_text.is_none())
        );
    }

    shared
        .inner
        .lock()
        .client_entry(client)
        .swallowed_keys
        .get_or_insert_default()
        .insert("x".to_owned());
    let mut release = chooser_key_press("x");
    release.action = KeyAction::Release;
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: release,
                text_follows: false,
            },
        )
        .expect("ignore swallowed key release");
    assert!(!shared.read_client(client, |c| {
        c.and_then(|c| c.swallowed_keys.as_ref())
            .is_some_and(|keys| keys.contains("x"))
    }));

    let mouse = TerminalMouseInput::new(
        TerminalMousePhase::Press,
        Some(TerminalMouseButton::Left),
        PointerCellEvent {
            column: 1,
            row: 1,
            click_count: 1,
            rectangle: false,
        },
        8,
        18,
        128,
        72,
        8,
        18,
        Modifiers::default(),
        false,
    );
    let wheel = TerminalMouseInput::new(
        TerminalMousePhase::Press,
        Some(TerminalMouseButton::ScrollUp),
        PointerCellEvent {
            column: 1,
            row: 1,
            click_count: 1,
            rectangle: false,
        },
        8,
        18,
        128,
        72,
        8,
        18,
        Modifiers::default(),
        false,
    );
    for action in [
        TerminalViewAction::Mouse(mouse),
        TerminalViewAction::ScrollWheel {
            lines: -1,
            input: wheel,
        },
    ] {
        shared
            .input(
                client,
                ClientKind::Interactive,
                &mut context,
                InputMessage::TerminalView { pane, action },
            )
            .expect("ignore mouse message input");
    }

    for input in [
        InputMessage::Text {
            pane,
            text: "standalone".to_owned(),
        },
        InputMessage::TerminalView {
            pane: browser,
            action: zz_terminal::TerminalViewAction::Paste("pasted".to_owned()),
        },
        InputMessage::ClientFocus { focused: true },
    ] {
        shared
            .input(client, ClientKind::Interactive, &mut context, input)
            .expect("ignore writable message input");
    }
    {
        let inner = shared.inner.lock();
        assert_eq!((*inner.clients[&client].message.as_ref().unwrap()), armed);
        assert!(client_terminal_publication_frozen(&inner, client));
        assert_eq!(
            (
                inner.attachment.activity_sequence,
                inner.panes.terminal_input_sequence,
                inner.engine.state.sessions[&session].sort_activity,
                inner.clients[&client].display_panes.as_ref().unwrap().token,
            ),
            unchanged
        );
    }
    let ignored_messages = take_reliable_messages(&mailbox);
    assert!(cleared_message_ids(&ignored_messages).is_empty());
    assert!(!ignored_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::DisplayPanes { state: None }
                | EventPayload::BrowserCommand { .. },
            ..
        })
    )));

    {
        let mut inner = shared.inner.lock();
        take_display_panes(&mut inner, client).expect("retire display panes");
        inner
            .client_entry(client)
            .command_prompt
            .replace(CommandPrompt::new(
                vec![CommandPromptStep {
                    label: "prompt".to_owned(),
                    input: "kept".to_owned(),
                }],
                None,
                None,
                CommandPromptType::Command,
                CommandPromptMode::Text,
                false,
                None,
            ));
    }
    let prompt_activity = shared.inner.lock().attachment.activity_sequence;
    for input in [
        InputMessage::Text {
            pane,
            text: "prompt text".to_owned(),
        },
        InputMessage::ClientFocus { focused: false },
    ] {
        shared
            .input(client, ClientKind::Interactive, &mut context, input)
            .expect("ignore prompt input");
    }
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner.clients[&client]
                .command_prompt
                .as_ref()
                .unwrap()
                .input,
            "kept"
        );
        assert_eq!(inner.attachment.activity_sequence, prompt_activity);
        assert_eq!((*inner.clients[&client].message.as_ref().unwrap()), armed);
    }

    shared
        .inner
        .lock()
        .client_mut(client)
        .and_then(|c| c.command_prompt.take());
    take_reliable_messages(&mailbox);
    let native_activity = shared.inner.lock().attachment.activity_sequence;
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceKey {
                pane: browser,
                input: chooser_key_press("n"),
                text_follows: true,
            },
        )
        .expect("deliver native browser key");
    let shifted_question = test_key(
        KeyCode::Character('/'),
        Modifiers::new(true, false, false, false),
        Some("?"),
    );
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: shifted_question,
                text_follows: true,
            },
        )
        .expect("ignore shifted terminal key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceText {
                pane: browser,
                text: "n".to_owned(),
            },
        )
        .expect("deliver native browser text");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.attachment.activity_sequence, native_activity + 1);
        assert_eq!(
            inner.clients[&client]
                .pending_committed_text
                .as_ref()
                .unwrap()
                .len(),
            1
        );
        let pending = inner.clients[&client]
            .pending_committed_text
            .as_ref()
            .unwrap()
            .front()
            .expect("terminal committed text");
        assert_eq!(pending.pane, pane);
        assert_eq!(pending.lane, CommittedTextLane::Terminal);
        assert_eq!(pending.suppressed_character, Some('?'));
        assert_eq!(
            (*inner.clients[&client].suppressed_text.as_ref().unwrap()),
            BTreeMap::from([('?', 1)])
        );
    }
    assert_eq!(armed_client_message(&shared, client), armed);
    assert_eq!(
        take_reliable_messages(&mailbox)
            .iter()
            .filter(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::BrowserCommand { pane, .. },
                    ..
                }) if *pane == browser
            ))
            .count(),
        2
    );

    let deadline = armed.deadline.expect("ignored message deadline");
    shared.expire_client_message(
        ClientMessageDeadline {
            client,
            token: armed.token,
            deadline,
        },
        deadline,
    );
    let trailing_activity = shared.inner.lock().attachment.activity_sequence;
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane,
                text: "?".to_owned(),
            },
        )
        .expect("retire shifted trailing text");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.attachment.activity_sequence, trailing_activity);
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.pending_committed_text.is_none())
        );
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.suppressed_text.is_none())
        );
        assert!(inner.client(client).is_none_or(|c| c.message.is_none()));
        assert!(inner.client(client).is_some_and(|c| c.message_ignore_keys));
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-N", "-d", "5000", "reverse"]),
        )
        .expect("rearm reverse-order message");
    let reverse = armed_client_message(&shared, client);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: test_key(
                    KeyCode::Character('/'),
                    Modifiers::new(true, false, false, false),
                    Some("?"),
                ),
                text_follows: true,
            },
        )
        .expect("ignore terminal key before browser key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceKey {
                pane: browser,
                input: chooser_key_press("n"),
                text_follows: true,
            },
        )
        .expect("deliver browser key after ignored terminal key");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::BrowserSurfaceText {
                pane: browser,
                text: "n".to_owned(),
            },
        )
        .expect("retire stale terminal debt before browser text");
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.pending_committed_text.is_none())
        );
        assert!(
            inner
                .client(client)
                .is_none_or(|c| c.suppressed_text.is_none())
        );
        assert_eq!((*inner.clients[&client].message.as_ref().unwrap()), reverse);
    }
    let reverse_deadline = reverse.deadline.expect("reverse message deadline");
    shared.expire_client_message(
        ClientMessageDeadline {
            client,
            token: reverse.token,
            deadline: reverse_deadline,
        },
        reverse_deadline,
    );
    let reverse_trailing_activity = shared.inner.lock().attachment.activity_sequence;
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane,
                text: "?".to_owned(),
            },
        )
        .expect("deliver retired reverse-order terminal text");
    assert!(shared.inner.lock().attachment.activity_sequence > reverse_trailing_activity);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-N", "-d", "5000", "read only"]),
        )
        .expect("rearm ignored message");
    let armed = armed_client_message(&shared, client);

    let read_only_activity = {
        let mut inner = shared.inner.lock();
        inner.attachment.client_flags.insert(client);
        inner.attachment.activity_sequence
    };
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: chooser_key_press("q"),
                text_follows: false,
            },
        )
        .expect("pass read-only key");
    {
        let inner = shared.inner.lock();
        assert!(inner.attachment.activity_sequence > read_only_activity);
        assert_eq!((*inner.clients[&client].message.as_ref().unwrap()), armed);
    }
    let read_only_focus_activity = shared.inner.lock().attachment.activity_sequence;
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ClientFocus { focused: true },
        )
        .expect("pass read-only focus");
    {
        let inner = shared.inner.lock();
        assert!(inner.attachment.activity_sequence > read_only_focus_activity);
        assert_eq!((*inner.clients[&client].message.as_ref().unwrap()), armed);
        assert!(inner.client(client).is_some_and(|c| c.message_ignore_keys));
    }
}

#[test]
fn a_bound_key_that_displays_a_message_survives_its_own_trailing_text() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "bound-key", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                ["-n", "d", "display-message", "-d", "5000", "bound"],
            ),
        )
        .expect("bind the key");

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane,
                input: chooser_key_press("d"),
                text_follows: true,
            },
        )
        .expect("press the bound key");
    let armed = armed_client_message(&shared, client);

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane,
                text: "d".to_owned(),
            },
        )
        .expect("deliver the trailing text");
    assert_eq!(armed_client_message(&shared, client), armed);
}

#[test]
fn surviving_text_and_explicit_paste_dismiss_messages_but_read_only_input_does_not() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "message-input", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "browser", "about:blank"]),
        )
        .expect("browser sink");
    let browser = context.pane.expect("browser pane");
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "0", "text"]),
        )
        .expect("text message");
    let text_message = armed_client_message(&shared, client);
    take_reliable_messages(&mailbox);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane,
                text: "ordinary".to_owned(),
            },
        )
        .expect("ordinary text");
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.message.is_none())));
    assert_eq!(
        cleared_message_ids(&take_reliable_messages(&mailbox)),
        [text_message.token]
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "0", "paste"]),
        )
        .expect("paste message");
    let paste_message = armed_client_message(&shared, client);
    take_reliable_messages(&mailbox);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::TerminalView {
                pane: browser,
                action: zz_terminal::TerminalViewAction::Paste("pasted".to_owned()),
            },
        )
        .expect("explicit paste");
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.message.is_none())));
    let paste_messages = take_reliable_messages(&mailbox);
    assert_eq!(cleared_message_ids(&paste_messages), [paste_message.token]);
    assert!(paste_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::BrowserCommand {
                pane,
                command: BrowserCommand::SendKeys(keys),
            },
            ..
        }) if *pane == browser
            && keys == &[zz_protocol::KeyToken::Literal("pasted".to_owned())]
    )));

    let mouse_input = |phase, button| {
        TerminalMouseInput::new(
            phase,
            button,
            PointerCellEvent {
                column: 1,
                row: 1,
                click_count: 1,
                rectangle: false,
            },
            8,
            18,
            128,
            72,
            8,
            18,
            Modifiers::default(),
            false,
        )
    };
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "0", "mouse"]),
        )
        .expect("mouse message");
    let mouse_message = armed_client_message(&shared, client);
    take_reliable_messages(&mailbox);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::TerminalView {
                pane,
                action: TerminalViewAction::Mouse(mouse_input(TerminalMousePhase::Motion, None)),
            },
        )
        .expect("bare mouse hover");
    assert_eq!(armed_client_message(&shared, client), mouse_message);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::TerminalView {
                pane,
                action: TerminalViewAction::Mouse(mouse_input(
                    TerminalMousePhase::Press,
                    Some(TerminalMouseButton::Left),
                )),
            },
        )
        .expect("mouse press");
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.message.is_none())));
    assert_eq!(
        cleared_message_ids(&take_reliable_messages(&mailbox)),
        [mouse_message.token]
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "0", "wheel"]),
        )
        .expect("wheel message");
    let wheel_message = armed_client_message(&shared, client);
    take_reliable_messages(&mailbox);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::TerminalView {
                pane,
                action: TerminalViewAction::ScrollWheel {
                    lines: -1,
                    input: mouse_input(
                        TerminalMousePhase::Press,
                        Some(TerminalMouseButton::ScrollUp),
                    ),
                },
            },
        )
        .expect("mouse wheel");
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.message.is_none())));
    assert_eq!(
        cleared_message_ids(&take_reliable_messages(&mailbox)),
        [wheel_message.token]
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "0", "read-only"]),
        )
        .expect("read-only message");
    let read_only_message = armed_client_message(&shared, client);
    shared.inner.lock().attachment.client_flags.insert(client);
    take_reliable_messages(&mailbox);
    for input in [
        InputMessage::Key {
            pane,
            input: chooser_key_press("q"),
            text_follows: false,
        },
        InputMessage::Text {
            pane,
            text: "blocked".to_owned(),
        },
        InputMessage::TerminalView {
            pane: browser,
            action: zz_terminal::TerminalViewAction::Paste("blocked".to_owned()),
        },
    ] {
        shared
            .input(client, ClientKind::Interactive, &mut context, input)
            .expect("read-only input");
        assert_eq!(armed_client_message(&shared, client), read_only_message);
    }
    let read_only_messages = take_reliable_messages(&mailbox);
    assert!(cleared_message_ids(&read_only_messages).is_empty());
    assert!(!read_only_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::BrowserCommand { pane, .. },
            ..
        }) if *pane == browser
    )));
}

#[test]
fn modal_consumed_text_and_paste_still_dismiss_writable_messages() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "modal-message-input", &[client]);
    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "browser", "about:blank"]),
        )
        .expect("browser sink");
    let browser = context.pane.expect("browser pane");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-buffer", ["-b", "covered", "buffer"]),
        )
        .expect("chooser buffer");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "0", "chooser"]),
        )
        .expect("chooser message");
    let chooser_message = armed_client_message(&shared, client);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("choose-buffer", std::iter::empty::<&str>()),
        )
        .expect("open chooser");
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.choose_buffer.is_some())
        );
        assert_eq!(
            (*inner.clients[&client].message.as_ref().unwrap()),
            chooser_message
        );
    }
    take_reliable_messages(&mailbox);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane: browser,
                text: "covered text".to_owned(),
            },
        )
        .expect("chooser-consumed text");
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.choose_buffer.is_some())
        );
        assert!(inner.client(client).is_none_or(|c| c.message.is_none()));
    }
    let text_messages = take_reliable_messages(&mailbox);
    assert_eq!(cleared_message_ids(&text_messages), [chooser_message.token]);
    assert!(!text_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::BrowserCommand { pane, .. },
            ..
        }) if *pane == browser
    )));
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ChooseBuffer {
                action: ChooseBufferAction::Close,
            },
        )
        .expect("close chooser");
    take_reliable_messages(&mailbox);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "0", "display-panes"]),
        )
        .expect("display-panes message");
    let paste_message = armed_client_message(&shared, client);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-panes", ["-d", "1000"]),
        )
        .expect("open display panes");
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.display_panes.is_some())
        );
        assert_eq!(
            (*inner.clients[&client].message.as_ref().unwrap()),
            paste_message
        );
    }
    take_reliable_messages(&mailbox);
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::TerminalView {
                pane: browser,
                action: zz_terminal::TerminalViewAction::Paste("covered paste".to_owned()),
            },
        )
        .expect("display-panes-consumed paste");
    {
        let inner = shared.inner.lock();
        assert!(
            inner
                .client(client)
                .is_some_and(|c| c.display_panes.is_some())
        );
        assert!(inner.client(client).is_none_or(|c| c.message.is_none()));
    }
    let paste_messages = take_reliable_messages(&mailbox);
    assert_eq!(cleared_message_ids(&paste_messages), [paste_message.token]);
    assert!(!paste_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::BrowserCommand { pane, .. },
            ..
        }) if *pane == browser
    )));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "0", "read-only"]),
        )
        .expect("read-only modal message");
    let read_only_message = armed_client_message(&shared, client);
    shared.inner.lock().attachment.client_flags.insert(client);
    take_reliable_messages(&mailbox);
    for input in [
        InputMessage::Text {
            pane: browser,
            text: "blocked text".to_owned(),
        },
        InputMessage::TerminalView {
            pane: browser,
            action: zz_terminal::TerminalViewAction::Paste("blocked paste".to_owned()),
        },
    ] {
        shared
            .input(client, ClientKind::Interactive, &mut context, input)
            .expect("read-only modal input");
        assert_eq!(armed_client_message(&shared, client), read_only_message);
    }
    let read_only_messages = take_reliable_messages(&mailbox);
    assert!(cleared_message_ids(&read_only_messages).is_empty());
    assert!(!read_only_messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::BrowserCommand { pane, .. },
            ..
        }) if *pane == browser
    )));
}

#[test]
fn detaching_drops_the_message_and_its_deadline_stays_inert() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (_, pane, _) = attached_message_fixture(&shared, "detach-message", &[client]);

    let mut context =
        ExecutionContext::for_pane(&shared.inner.lock().engine.state, pane).expect("pane context");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("display-message", ["-d", "5000", "going"]),
        )
        .expect("display message");
    let armed = armed_client_message(&shared, client);
    shared.detach(client);
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.message.is_none())));
    take_reliable_messages(&mailbox);

    shared.expire_client_message(
        ClientMessageDeadline {
            client,
            token: armed.token,
            deadline: armed.deadline.expect("armed deadline"),
        },
        Instant::now(),
    );
    assert!(cleared_message_ids(&take_reliable_messages(&mailbox)).is_empty());
}

#[test]
fn a_control_client_gets_no_timer_and_no_freeze() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));

    shared
        .execute(
            client,
            ClientKind::Control,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("display-message", ["-d", "5000", "control"]),
        )
        .expect("display message");
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.message.is_none())));
}
