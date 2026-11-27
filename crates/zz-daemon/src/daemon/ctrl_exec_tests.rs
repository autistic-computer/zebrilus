use super::ctrl_test_support::{
    compact_command, compact_exec_request, compact_hello, compact_registered, reliable_children,
};
use super::*;

#[test]
fn disconnected_control_exec_cannot_detach_the_next_active_client() {
    let shared = Arc::new(Shared::new(41));
    let mut context = ExecutionContext::default();
    compact_command(
        &shared,
        &mut context,
        "new-session",
        &["-d", "-s", "ctrl-cancel", "sleep 30"],
    );
    let (old, old_mailbox) = compact_registered(&shared, zz_protocol::Subscriptions::control());
    shared
        .inner
        .lock()
        .client_entry(old)
        .kind
        .replace(ClientKind::Control);
    shared
        .attach_target(old, ClientKind::Control, &mut context, "ctrl-cancel")
        .expect("attach old control");
    let cancel = Arc::new(AtomicBool::new(false));
    shared
        .command_queue_cancels
        .lock()
        .insert(old, Arc::clone(&cancel));
    let directory = tempfile::Builder::new()
        .prefix("zz-ctrl-cancel-")
        .tempdir()
        .expect("barrier directory");
    let ready = directory.path().join("ready");
    let release = directory.path().join("release");
    let script = if cfg!(windows) {
        format!(
            "type nul > \"{}\" & for /L %i in (0,0,1) do @if exist \"{}\" exit 0",
            ready.display(),
            release.display()
        )
    } else {
        format!(
            "touch {}; while test ! -e {}; do sleep 0.01; done",
            ready.display(),
            release.display()
        )
    };
    let request = compact_exec_request(vec![
        CommandInvocation::new("run-shell", [script]),
        CommandInvocation::new("detach-client", [] as [&str; 0]),
    ]);
    let worker = {
        let shared = Arc::clone(&shared);
        let old_mailbox = Arc::clone(&old_mailbox);
        thread::spawn(move || {
            shared.execute_compact_request(
                old,
                ClientKind::Control,
                &mut context,
                request,
                &old_mailbox,
            );
        })
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    while !ready.exists() {
        assert!(
            Instant::now() < deadline,
            "foreground control command did not start"
        );
        thread::sleep(Duration::from_millis(1));
    }
    cancel.store(true, Ordering::Release);
    shared.detach(old);
    shared.unregister(old);
    let (next, next_mailbox) = compact_registered(&shared, zz_protocol::Subscriptions::control());
    shared
        .inner
        .lock()
        .client_entry(next)
        .kind
        .replace(ClientKind::Control);
    let mut next_context = ExecutionContext::default();
    let (session, _) = shared
        .attach_target(next, ClientKind::Control, &mut next_context, "ctrl-cancel")
        .expect("attach next control");
    reliable_children(&next_mailbox);
    fs::write(release, []).expect("release foreground command");
    worker.join().expect("cancelled execution");
    shared.execute_compact_request(
        old,
        ClientKind::Control,
        &mut ExecutionContext::default(),
        compact_exec_request(vec![CommandInvocation::new(
            "detach-client",
            [] as [&str; 0],
        )]),
        &old_mailbox,
    );
    assert_eq!(
        client_attached_session(&shared.inner.lock(), next),
        Some(session)
    );
    assert!(
        !reliable_children(&next_mailbox)
            .iter()
            .any(|message| matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Detached { .. },
                    ..
                })
            ))
    );
    shared.command_queue_cancels.lock().remove(&old);
    compact_command(
        &shared,
        &mut next_context,
        "kill-session",
        &["-t", "ctrl-cancel"],
    );
}

#[test]
fn compact_raw_control_preflights_line_and_resolves_daemon_environment() {
    let shared = Arc::new(Shared::new(34));
    let mut hello = compact_hello(ClientKind::Control);
    hello.subscriptions = zz_protocol::Subscriptions::control();
    let (client, _) = shared.register_welcome(&hello).expect("register");
    let mailbox = OutboundMailbox::new();
    shared.subscribe(client, Arc::clone(&mailbox));
    if let Some(client) = shared.inner.lock().client_mut(client) {
        client.ctrl_initializing = false;
    }
    let mut context = ExecutionContext::default();
    let request = |line: &str| zz_protocol::ExecRequest {
        protocol_version: PROTOCOL_VERSION,
        flags: zz_protocol::ExecFlags::default(),
        client_instance_id: ClientInstanceId(919),
        origin: None,
        working_directory: None,
        tty: None,
        size: None,
        features: 0,
        startup_reentry: None,
        spawned_server_id: None,
        expect_server_id: None,
        process_id: std::process::id(),
        environment: ClientEnvironmentBlob::default(),
        commands: Vec::new(),
        raw_control_line: Some(line.to_owned()),
    };
    shared.execute_compact_request(
        client,
        ClientKind::Control,
        &mut context,
        request("set-environment -g CTRL_MUST_NOT_RUN yes ; unknown-ctrl-command"),
        &mailbox,
    );
    assert!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("CTRL_MUST_NOT_RUN")
            .is_none()
    );
    assert!(matches!(
        reliable_children(&mailbox).as_slice(),
        [ProtocolMessage::ExecExit(zz_protocol::ExecExit {
            outcome: zz_protocol::ExecOutcome::Rejected(_),
            ..
        })]
    ));
    shared.execute_compact_request(
        client,
        ClientKind::Control,
        &mut context,
        request(r"set-environment -g CTRL_MUST_NOT_RUN yes ; display-message -p \400"),
        &mailbox,
    );
    assert!(matches!(
        reliable_children(&mailbox).as_slice(),
        [ProtocolMessage::ExecExit(zz_protocol::ExecExit {
            outcome: zz_protocol::ExecOutcome::Rejected(ServerError::CommandParse(message)),
            ..
        })] if message == "invalid octal escape"
    ));
    assert!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("CTRL_MUST_NOT_RUN")
            .is_none()
    );
    shared
        .inner
        .lock()
        .engine
        .seed_global_environment([("CTRL_EXPANSION", "server-value"), ("HOME", "/server-home")]);
    shared.execute_compact_request(
        client,
        ClientKind::Control,
        &mut context,
        request("display-message -p ~/$CTRL_EXPANSION"),
        &mailbox,
    );
    assert!(matches!(
        mailbox.state.lock().reliable.back(),
        Some(OutboundFrame::Grouped { frames, .. }) if frames.len() == 3
    ));
    let messages = reliable_children(&mailbox);
    let started = messages.iter().position(|message| matches!(message, ProtocolMessage::Event(Event { payload: EventPayload::ControlCommandStarted { request_id: 1, flags: 1, guard: true, canonical_name: Some(name) }, .. }) if name == "display-message")).unwrap_or_else(|| panic!("command start missing: {messages:?}"));
    let response = messages
        .iter()
        .position(|message| {
            matches!(
                message,
                ProtocolMessage::CommandResponse(CommandResponse::Success { output, .. }) if output.as_bytes() == b"/server-home/server-value"
            )
        })
        .expect("response");
    assert!(started < response);
    assert!(matches!(
        messages.last(),
        Some(ProtocolMessage::ExecExit(_))
    ));
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[90]",
                    "ctrlalias=display-message -p \"~/$CTRL_EXPANSION\"",
                ],
            ),
        )
        .expect("alias whose body needs expansion");
    for (line, expected) in [
        ("ctrlalias", "/server-home/server-value"),
        (
            "set-environment -g CTRL_EXPANSION changed ; ctrlalias",
            "/server-home/server-value",
        ),
        ("ctrlalias", "/server-home/changed"),
    ] {
        assert!(!line.contains(['$', '~']));
        shared.execute_compact_request(
            client,
            ClientKind::Control,
            &mut context,
            request(line),
            &mailbox,
        );
        assert!(
            reliable_children(&mailbox).iter().any(|message| matches!(
                message,
                ProtocolMessage::CommandResponse(CommandResponse::Success { output, .. })
                    if output.as_bytes() == expected.as_bytes()
            )),
            "{line}"
        );
    }
}

#[test]
fn empty_control_expansion_resolvers_complete_without_the_state_lock() {
    let shared = Arc::new(Shared::new(52));
    let inner = shared.inner.lock();
    let (finished, completion) = crossbeam_channel::bounded(1);
    let worker = {
        let shared = Arc::clone(&shared);
        thread::spawn(move || {
            let values = shared.resolve_environment(&[]);
            let homes = shared.resolve_home_directories(&[]);
            let _ = finished.send((values, homes));
        })
    };
    let result = completion.recv_timeout(Duration::from_secs(2));
    drop(inner);
    worker.join().expect("empty expansion worker");
    let (values, homes) = result.expect("empty resolvers do not acquire the state lock");
    assert!(values.is_empty() && homes.is_empty());
}

#[cfg(unix)]
#[test]
fn exec_resume_upgrades_the_same_socket_and_places_tail_error_after_attachment() {
    use std::os::unix::net::UnixStream;
    let shared = Arc::new(Shared::new(35));
    let (mut client, server) = UnixStream::pair().expect("pair");
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("timeout");
    let worker_shared = Arc::clone(&shared);
    let worker = thread::spawn(move || handle_connection(server, &worker_shared));
    let mut request = compact_exec_request(vec![
        CommandInvocation::new("display-message", ["-p", "prefix"]),
        CommandInvocation::new("new-session", ["-s", "ctrl-upgrade", "sleep 30"]),
        CommandInvocation::new("select-window", ["-t", "missing-ctrl-window"]),
    ]);
    request.flags.set(zz_protocol::ExecFlags::RESUME, true);
    request.flags.set(zz_protocol::ExecFlags::LAST, true);
    zz_protocol::write_protocol_message(&mut client, &ProtocolMessage::Exec(request))
        .expect("exec");
    let ProtocolMessage::ExecExit(zz_protocol::ExecExit {
        outcome: zz_protocol::ExecOutcome::Resume(resume),
        ..
    }) = zz_protocol::read_protocol_message(&mut client).expect("resume")
    else {
        panic!("expected resume")
    };
    let mut hello = compact_hello(ClientKind::Interactive);
    hello.subscriptions = zz_protocol::Subscriptions::terminal();
    hello.viewport = Some(zz_protocol::ClientViewport {
        columns: 80,
        rows: 24,
        cell_width_px: 8,
        cell_height_px: 16,
    });
    hello.attach = Some(zz_protocol::AttachOperation::Commands(resume.commands));
    zz_protocol::write_protocol_message(&mut client, &ProtocolMessage::Hello(hello))
        .expect("upgrade hello");
    assert!(matches!(
        zz_protocol::read_protocol_message(&mut client).expect("welcome"),
        ProtocolMessage::Welcome(_)
    ));
    let ProtocolMessage::Batch(batch) =
        zz_protocol::read_protocol_message(&mut client).expect("batch")
    else {
        panic!("expected batch")
    };
    let messages = batch.messages().expect("children");
    let prefix = messages.iter().position(|message| matches!(message, ProtocolMessage::CommandResponse(CommandResponse::Success { request_id: 1, output, .. }) if output.as_bytes() == b"prefix")).expect("prefix output");
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
        .expect("attachment");
    let tail = messages
        .iter()
        .position(|message| {
            matches!(
                message,
                ProtocolMessage::CommandResponse(CommandResponse::Error { request_id: 3, .. })
            )
        })
        .expect("tail error");
    assert!(prefix < attached);
    assert!(attached < tail);
    assert_eq!(shared.inner.lock().clients.len(), 1);
    drop(client);
    worker.join().expect("worker").expect("connection");
    compact_command(
        &shared,
        &mut ExecutionContext::default(),
        "kill-session",
        &["-t", "ctrl-upgrade"],
    );
}

#[test]
fn hook_body_control_notification_follows_output_guard_without_recursive_hooks() {
    let shared = Arc::new(Shared::new(36));
    let mut context = ExecutionContext::default();
    compact_command(
        &shared,
        &mut context,
        "new-session",
        &["-d", "-s", "ctrl-hook", "-n", "initial", "sleep 30"],
    );
    compact_command(
        &shared,
        &mut context,
        "set-hook",
        &[
            "-g",
            "after-display-message",
            "rename-window -t ctrl-hook:0 hook-renamed",
        ],
    );
    compact_command(
        &shared,
        &mut context,
        "set-hook",
        &[
            "-g",
            "window-renamed",
            "set-environment -g CTRL_RECURSIVE_HOOK bad",
        ],
    );
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
    shared
        .attach_target(client, ClientKind::Control, &mut context, "ctrl-hook")
        .expect("attach");
    reliable_children(&mailbox);
    shared.execute_compact_request(
        client,
        ClientKind::Control,
        &mut context,
        compact_exec_request(vec![CommandInvocation::new(
            "display-message",
            ["-p", "outer"],
        )]),
        &mailbox,
    );
    let messages = reliable_children(&mailbox);
    let started = messages
        .iter()
        .position(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlCommandStarted { guard: true, .. },
                    ..
                })
            )
        })
        .unwrap_or_else(|| panic!("command start missing: {messages:?}"));
    let notification = messages.iter().position(|message| matches!(message, ProtocolMessage::Event(Event { payload: EventPayload::HookEvent { name, .. }, .. }) if name == "window-renamed")).unwrap_or_else(|| panic!("rename notification missing: {messages:?}"));
    assert!(started < notification, "{messages:?}");
    assert!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("CTRL_RECURSIVE_HOOK")
            .is_none()
    );
    compact_command(&shared, &mut context, "kill-session", &["-t", "ctrl-hook"]);
}
