use super::*;

#[test]
fn terminal_watcher_does_not_retain_idle_session_after_pane_removal() {
    let shared = Arc::new(Shared::new(1));
    let pane = PaneId(9_999_998);
    let terminal = Arc::new(TerminalSession::spawn_output_view(
        "idle watcher fixture".to_owned(),
        String::new(),
    ));
    let terminal_weak = Arc::downgrade(&terminal);
    shared
        .inner
        .lock()
        .terminals_mut()
        .insert(pane, Arc::clone(&terminal));
    shared
        .watch_terminal(pane, &terminal)
        .expect("start terminal watcher");

    let deadline = Instant::now() + Duration::from_secs(2);
    while terminal.diagnostics().event_queue_len != 0 || Arc::strong_count(&terminal) != 2 {
        assert!(
            Instant::now() < deadline,
            "only the test and terminal map should own an idle session, found {} owners",
            Arc::strong_count(&terminal)
        );
        thread::sleep(Duration::from_millis(5));
    }

    shared.inner.lock().terminals_mut().remove(&pane);
    drop(terminal);
    let deadline = Instant::now() + Duration::from_secs(2);
    while terminal_weak.upgrade().is_some() {
        assert!(
            Instant::now() < deadline,
            "idle terminal remained owned after pane removal"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn terminal_watcher_persists_dynamic_viewport_titles_in_mux_snapshots() {
    let shared = Arc::new(Shared::new(1));
    let pane = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("title-fixture")
        .expect("session")
        .2;
    let terminal = Arc::new(TerminalSession::spawn_output_view(
        "dynamic terminal title".to_owned(),
        String::new(),
    ));
    shared
        .inner
        .lock()
        .terminals_mut()
        .insert(pane, Arc::clone(&terminal));
    shared
        .watch_terminal(pane, &terminal)
        .expect("start terminal watcher");
    terminal.attach_view(TerminalViewId(u64::MAX - 1));

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let title = shared
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .map(|pane| pane.title.clone());
        if title.as_deref() == Some("dynamic terminal title") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "terminal title did not reach mux state; last title was {title:?}"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let snapshot = shared.inner.lock().engine.state.snapshot();
    assert_eq!(
        snapshot.sessions[0].windows[0].panes[&pane].title,
        "dynamic terminal title"
    );

    let replacement = Arc::new(TerminalSession::spawn_output_view(
        "replacement terminal".to_owned(),
        String::new(),
    ));
    shared
        .inner
        .lock()
        .terminals_mut()
        .insert(pane, Arc::clone(&replacement));
    shared.synchronize_pane_title(pane, &terminal, "stale watcher title", true);
    assert_eq!(
        shared.inner.lock().engine.state.pane(pane).unwrap().title,
        "dynamic terminal title",
        "a retired watcher must not rename its replacement"
    );

    shared.inner.lock().terminals_mut().remove(&pane);
}

#[test]
fn terminal_watcher_classifies_failed_workers_as_reapable() {
    assert!(terminal_status_should_close(&SessionStatus::exited(
        0, None
    )));
    assert!(terminal_status_should_close(&SessionStatus::failed(
        "emulation failure"
    )));
    assert!(!terminal_status_should_close(&SessionStatus::Running));
}

#[test]
fn terminal_process_exit_removes_its_owning_pane_and_requests_shutdown_with_no_client() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "exit-fixture", "exit"]),
        )
        .expect("create terminal pane");
    shared.exit_empty_armed.store(true, Ordering::Release);
    let pane = context.pane.expect("terminal pane");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let inner = shared.inner.lock();
        let pane_exists = inner.engine.state.window_for_pane(pane).is_some();
        let terminal_exists = inner.terminals.contains_key(&pane);
        drop(inner);
        let stopping = shared.stopping.load(Ordering::Acquire);
        if !pane_exists && !terminal_exists && stopping {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "exited terminal pane remained in mux state ({pane_exists}, {terminal_exists}) or no shutdown was requested ({stopping})"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn remain_on_exit_retains_and_respawns_the_same_terminal_pane() {
    let sleeper = if cfg!(windows) {
        "ping -n 31 127.0.0.1 >nul"
    } else {
        "sleep 30"
    };
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-window-option", ["-g", "remain-on-exit", "on"]),
        )
        .expect("enable retained exits");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "remain-on-exit-format", ""]),
        )
        .expect("silence the dead notice");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d",
                    "-s",
                    "dead-fixture",
                    if cfg!(windows) {
                        "echo ZZ_DEAD_FRAME& exit 7"
                    } else {
                        "printf 'ZZ_DEAD_FRAME\\n'; exit 7"
                    },
                ],
            ),
        )
        .expect("create exiting terminal pane");
    let session = context.session.expect("terminal session");
    let pane = context.pane.expect("terminal pane");
    let window = context.window.expect("terminal window");
    let layout = shared.inner.lock().engine.state.windows[&window]
        .layout
        .project();

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let inner = shared.inner.lock();
        if inner.engine.state.pane(pane).is_some_and(|pane| {
            pane.dead && pane.dead_status == Some(7) && pane.dead_time.is_some()
        }) {
            assert!(inner.terminals.contains_key(&pane));
            break;
        }
        drop(inner);
        assert!(
            Instant::now() < deadline,
            "exited terminal did not become retained-dead"
        );
        thread::sleep(Duration::from_millis(10));
    }

    let output = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                [
                    "-p",
                    "-t",
                    &pane.to_string(),
                    "#{pane_id}:#{pane_dead}:#{pane_dead_status}:#{?pane_dead_time,set,}",
                ],
            ),
        )
        .expect("read dead facts")
        .output;
    assert_eq!(output, format!("{pane}:1:7:set"));
    let dead_time = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                ["-p", "-t", &pane.to_string(), "#{pane_dead_time}"],
            ),
        )
        .expect("read dead time")
        .output
        .parse::<u64>()
        .expect("dead time epoch");
    assert!(dead_time > 0);

    let mailbox = OutboundMailbox::new();
    let (late_client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    shared
        .attach(late_client, session)
        .expect("attach after terminal exit");
    shared.send_resync(late_client, &mailbox);
    let frozen = {
        let state = mailbox.state.lock();
        state
            .terminals
            .get(&pane)
            .map(|pending| pending.encoded.clone())
            .expect("retained frame queued for late client")
    };
    let ProtocolMessage::Event(Event {
        payload:
            EventPayload::TerminalViewport {
                pane: delivered,
                viewport,
            },
        ..
    }) = decode_protocol_frame(&frozen).expect("decode retained frame")
    else {
        panic!("late client did not receive a full terminal viewport");
    };
    assert_eq!(delivered, pane);
    assert_eq!(viewport.status, SessionStatus::exited(7, None));
    assert!(viewport_text(&viewport).contains("ZZ_DEAD_FRAME"));
    let captured = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("capture-pane", ["-p", "-t", &pane.to_string()]),
        )
        .expect("capture retained dead pane")
        .output;
    assert_eq!(captured.trim_end(), "ZZ_DEAD_FRAME");

    let previous = Arc::clone(&shared.inner.lock().terminals[&pane]);
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("respawn-pane", ["-E", "-t", &pane.to_string(), sleeper]),
        )
        .expect("respawn dead pane empty");
    let empty = {
        let inner = shared.inner.lock();
        let replacement = Arc::clone(&inner.terminals[&pane]);
        assert!(!Arc::ptr_eq(&previous, &replacement));
        assert!(!inner.engine.state.pane(pane).unwrap().dead);
        assert_eq!(inner.engine.state.pane(pane).unwrap().dead_time, None);
        assert_eq!(inner.engine.state.windows[&window].layout.project(), layout);
        assert_eq!(replacement.process_id(), None);
        assert_eq!(replacement.foreground_process_id(), None);
        assert_eq!(
            inner.panes.terminal_spawns[&pane].command.as_deref(),
            Some([sleeper.to_owned()].as_slice())
        );
        replacement
    };

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let viewport = empty.latest_viewport_for(TerminalViewId(late_client.0));
        if viewport.as_ref().is_some_and(|viewport| {
            matches!(viewport.status, SessionStatus::Running)
                && matches!(viewport.mode, TerminalMode::Live)
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "empty pane did not publish a live viewport"
        );
        thread::sleep(Duration::from_millis(10));
    }

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("respawn-pane", ["-t", &pane.to_string()]),
        )
        .expect("respawn empty pane without kill");
    {
        let inner = shared.inner.lock();
        let replacement = &inner.terminals[&pane];
        assert!(!Arc::ptr_eq(&empty, replacement));
        assert!(!inner.engine.state.pane(pane).unwrap().dead);
        assert_eq!(inner.engine.state.windows[&window].layout.project(), layout);
        assert_eq!(
            inner.panes.terminal_spawns[&pane].command.as_deref(),
            Some([sleeper.to_owned()].as_slice())
        );
    }

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-pane", ["-t", &pane.to_string()]),
        )
        .expect("clean up respawned pane");
}

#[cfg(unix)]
#[test]
fn direct_spawn_failure_is_retained_with_status_one_and_no_output() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-window-option", ["-g", "remain-on-exit", "on"]),
        )
        .expect("enable retained exits");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "spawn-failure-fixture", "--", "sleep", "30"],
            ),
        )
        .expect("create fixture session");
    let execution = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-window",
                [
                    "-d",
                    "-t",
                    "spawn-failure-fixture",
                    "--",
                    "nosuchcmd-zz",
                    "extraarg",
                ],
            ),
        )
        .expect("create direct spawn failure");
    let pane = execution
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .expect("failed pane id");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if shared
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .is_some_and(|pane| pane.dead && pane.dead_status == Some(1))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "spawn failure did not become retained-dead with status one"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "list-panes",
                    [
                        "-t",
                        &pane.to_string(),
                        "-F",
                        "#{pane_dead}:#{pane_dead_status}",
                    ],
                ),
            )
            .expect("read failed pane status")
            .output,
        "1:1"
    );
    assert!(
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("capture-pane", ["-p", "-t", &pane.to_string()]),
            )
            .expect("capture failed pane")
            .output
            .trim()
            .is_empty()
    );
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "spawn-failure-fixture"]),
        )
        .expect("remove spawn failure fixture");
}

#[cfg(unix)]
#[test]
fn retained_signal_exit_exposes_the_tmux_signal_name() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-window-option", ["-g", "remain-on-exit", "on"]),
        )
        .expect("enable retained exits");
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "signal-fixture", "kill -TERM $$"],
            ),
        )
        .expect("create signalled terminal pane");
    let pane = context.pane.expect("terminal pane");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if shared
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .is_some_and(|pane| pane.dead)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "signalled terminal did not become retained-dead"
        );
        thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "display-message",
                    [
                        "-p",
                        "-t",
                        &pane.to_string(),
                        "#{pane_dead}:#{pane_dead_status}:#{pane_dead_signal}",
                    ],
                ),
            )
            .expect("read signal facts")
            .output,
        if cfg!(target_os = "linux") {
            "1::15"
        } else {
            "1::term"
        }
    );
    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-pane", ["-t", &pane.to_string()]),
        )
        .expect("clean up retained signal pane");
}

#[test]
fn unchanged_pane_runtime_facts_do_not_bump_the_mux_generation() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "a", QUIET_PANE_COMMAND]),
        )
        .expect("session");
    let pane = context.pane.expect("session pane");
    wait_for_pane_runtime_facts(&shared, &[pane]);
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    let (facts, generation) = {
        let inner = shared.inner.lock();
        (
            inner
                .engine
                .pane_runtime_facts(pane)
                .cloned()
                .expect("settled facts"),
            inner.engine.state.generation(),
        )
    };

    shared.synchronize_pane_runtime(
        pane,
        &terminal,
        None,
        &facts.current_command,
        Some(facts.current_path.clone()),
        false,
        Instant::now(),
    );
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.state.generation(), generation);
        assert_eq!(inner.engine.pane_runtime_facts(pane), Some(&facts));
    }

    shared.synchronize_pane_runtime(
        pane,
        &terminal,
        None,
        "zz-changed",
        Some(facts.current_path.clone()),
        false,
        Instant::now(),
    );
    let inner = shared.inner.lock();
    assert_ne!(inner.engine.pane_runtime_facts(pane), Some(&facts));
}

#[cfg(unix)]
#[test]
fn pane_current_command_follows_an_exec_in_the_same_process() {
    let shared = Arc::new(Shared::new(1));
    shared.start_timers().expect("start timers");
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "a", "read -r line; exec cat"]),
        )
        .expect("session");
    let pane = context.pane.expect("session pane");
    wait_for_pane_runtime_facts(&shared, &[pane]);
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    assert!(terminal.send_raw_input(Arc::from(b"go\nafter-exec\n".as_slice())));
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let command = shared
            .inner
            .lock()
            .engine
            .pane_runtime_facts(pane)
            .map(|facts| facts.current_command.clone())
            .unwrap_or_default();
        if command == "cat" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "pane_current_command stayed {command:?} after the shell exec'd cat"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn command_output_ids_advance_and_stay_with_each_actor() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (session, pane, _) = output_view_session_fixture(&shared, "output-ids", "pane");
    shared
        .attach(client, session)
        .expect("attach output client");
    take_reliable_messages(&mailbox);

    shared
        .open_command_output(client, Some(pane), "first".to_owned(), "one\ntwo")
        .expect("open first command output");
    let first_id = command_output_message_id(&take_command_output_message(&mailbox));
    assert_ne!(first_id, 0);
    let first_terminal = Arc::clone(
        &shared.inner.lock().clients[&client]
            .command_output
            .as_ref()
            .unwrap()
            .terminal,
    );
    let first_viewport = first_terminal
        .latest_viewport_for(TerminalViewId(client.0))
        .expect("first command-output viewport");
    shared.publish_command_output(client, pane, &first_terminal, &first_viewport);
    assert_eq!(
        command_output_message_id(&take_command_output_message(&mailbox)),
        first_id
    );
    shared.send_resync(client, &mailbox);
    assert_eq!(
        command_output_message_id(&take_command_output_message(&mailbox)),
        first_id
    );
    take_reliable_messages(&mailbox);

    shared
        .open_command_output(client, Some(pane), "second".to_owned(), "three\nfour")
        .expect("replace command output");
    let (second_id, second_terminal) = {
        let inner = shared.inner.lock();
        let output = &inner.clients[&client].command_output.as_ref().unwrap();
        (output.output_id, Arc::clone(&output.terminal))
    };
    assert!(second_id > first_id);
    wait_for_command_output_close(&mailbox, first_id);
    assert_eq!(
        command_output_message_id(&take_command_output_message(&mailbox)),
        second_id
    );
    shared.close_command_output(client, &first_terminal);
    assert_eq!(
        shared.inner.lock().clients[&client]
            .command_output
            .as_ref()
            .unwrap()
            .output_id,
        second_id
    );
    shared.close_command_output(client, &second_terminal);
    wait_for_command_output_close(&mailbox, second_id);
    shared.send_resync(client, &mailbox);
    assert!(take_reliable_messages(&mailbox).into_iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::CommandOutput {
                    output_id: 0,
                    viewport: None,
                    ..
                },
                ..
            })
        )
    }));

    shared
        .open_command_output(client, Some(pane), "third".to_owned(), "five\nsix")
        .expect("open third command output");
    let (third_id, third_terminal) = {
        let inner = shared.inner.lock();
        let output = &inner.clients[&client].command_output.as_ref().unwrap();
        (output.output_id, Arc::clone(&output.terminal))
    };
    assert!(third_id > second_id);
    shared.close_command_output(client, &third_terminal);
    wait_for_command_output_close(&mailbox, third_id);
    assert!(mailbox.state.lock().command_output.is_none());
}

#[test]
fn command_output_retires_with_its_pane_or_attached_session() {
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
            &CommandInvocation::new("new-session", ["-s", "first"]),
        )
        .expect("first session");
    let first_session = context.session.expect("first session id");
    let source = context.pane.expect("source pane");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["-h"]),
        )
        .expect("surviving pane");
    shared.attach(client, first_session).expect("attach first");
    take_reliable_messages(&mailbox);

    shared
        .open_command_output(
            client,
            Some(source),
            "pane output".to_owned(),
            "pane-bound output",
        )
        .expect("open pane output");
    take_command_output_message(&mailbox);
    let pane_output_id = current_command_output_id(&shared, client);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("kill-pane", ["-t", &source.to_string()]),
        )
        .expect("remove output source");
    wait_for_command_output_close(&mailbox, pane_output_id);
    assert!(shared.read_client(client, |c| c.is_none_or(|c| c.command_output.is_none())));

    let remaining = context.pane.expect("remaining pane");
    shared
        .open_command_output(
            client,
            Some(remaining),
            "session output".to_owned(),
            "session-bound output",
        )
        .expect("open session output");
    take_command_output_message(&mailbox);
    let session_output_id = current_command_output_id(&shared, client);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "second"]),
        )
        .expect("second session");
    let second_session = context.session.expect("second session id");
    shared
        .attach(client, second_session)
        .expect("switch attached session");
    wait_for_command_output_close(&mailbox, session_output_id);
    let second_pane = context.pane.expect("second-session pane");
    shared
        .open_command_output(
            client,
            Some(second_pane),
            "window output".to_owned(),
            "window-bound output",
        )
        .expect("open window output");
    take_command_output_message(&mailbox);
    let window_output_id = current_command_output_id(&shared, client);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-window", [] as [&str; 0]),
        )
        .expect("change active window");
    wait_for_command_output_close(&mailbox, window_output_id);
    let inner = shared.inner.lock();
    assert!(
        inner
            .client(client)
            .is_none_or(|c| c.command_output.is_none())
    );
    assert_eq!(
        inner.clients[&client]
            .key_engine
            .as_ref()
            .unwrap()
            .active_table(),
        None
    );
}
