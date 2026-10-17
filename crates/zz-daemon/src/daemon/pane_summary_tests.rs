use super::*;
use zz_protocol::PaneSummary;
use zz_terminal::PaneOutputFacts;

fn status(state: PaneStatusState, message: &str) -> PaneStatus {
    PaneStatus {
        state,
        kind: None,
        progress: None,
        app: String::new(),
        title: String::new(),
        message: message.to_owned(),
    }
}

fn summary(inner: &ServerState, pane: PaneId) -> PaneSummary {
    inner.engine.state.pane(pane).expect("pane").summary.clone()
}

fn pane_status(inner: &ServerState, pane: PaneId) -> Option<PaneStatus> {
    inner.engine.state.pane(pane).expect("pane").status.clone()
}

fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(5));
    }
}

fn empty_terminal_pane(shared: &Shared, name: &str) -> (PaneId, Arc<TerminalSession>) {
    let terminal = Arc::new(TerminalSession::spawn_empty_with_appearance(
        64,
        Arc::new(TerminalAppearance::default()),
    ));
    let mut inner = shared.inner.lock();
    let (_, _, pane) = inner.engine.state.create_session(name).expect("session");
    inner.terminals_mut().insert(pane, Arc::clone(&terminal));
    (pane, terminal)
}

fn run(shared: &Arc<Shared>, args: &[&str]) {
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(args[0], args[1..].iter().copied()),
        )
        .expect("command");
}

#[test]
fn status_edges_stamp_and_message_changes_do_not() {
    let mut inner = ServerState::default();
    let (_, _, pane) = inner.engine.state.create_session("edges").expect("session");

    assert!(apply_pane_status(
        &mut inner,
        pane,
        Some(status(PaneStatusState::Working, "")),
        10
    ));
    assert_eq!(
        (
            summary(&inner, pane).changed_at,
            summary(&inner, pane).unseen
        ),
        (10, false)
    );

    assert!(apply_pane_status(
        &mut inner,
        pane,
        Some(status(PaneStatusState::Done, "first")),
        20
    ));
    assert_eq!(
        (
            summary(&inner, pane).changed_at,
            summary(&inner, pane).unseen
        ),
        (20, true)
    );

    assert!(apply_pane_status(
        &mut inner,
        pane,
        Some(status(PaneStatusState::Done, "second")),
        30
    ));
    assert_eq!(
        (
            summary(&inner, pane).changed_at,
            summary(&inner, pane).unseen
        ),
        (20, true)
    );
    assert_eq!(
        pane_status(&inner, pane).map(|status| status.message),
        Some("second".to_owned())
    );

    let generation = inner.engine.state.generation();
    assert!(!apply_pane_status(
        &mut inner,
        pane,
        Some(status(PaneStatusState::Done, "second")),
        40
    ));
    assert_eq!(inner.engine.state.generation(), generation);

    assert!(apply_pane_status(
        &mut inner,
        pane,
        Some(status(PaneStatusState::Error, "")),
        50
    ));
    assert_eq!(
        (
            summary(&inner, pane).changed_at,
            summary(&inner, pane).unseen
        ),
        (50, true)
    );
    assert!(!apply_pane_status(
        &mut inner,
        PaneId(9999),
        Some(status(PaneStatusState::Done, "")),
        60
    ));
}

#[test]
fn a_finish_in_a_watched_window_stays_seen() {
    let mut inner = ServerState::default();
    let (session, first, watched) = inner
        .engine
        .state
        .create_session("watched")
        .expect("session");
    let (_, hidden) = inner
        .engine
        .state
        .create_window(session, None, PaneKind::Terminal)
        .expect("second window");
    inner
        .engine
        .state
        .select_window(session, first)
        .expect("select first window");
    inner
        .attached
        .entry(session)
        .or_default()
        .insert(ClientId(1));
    assert!(pane_window_watched(&inner, watched));
    assert!(!pane_window_watched(&inner, hidden));

    for pane in [watched, hidden] {
        apply_pane_status(
            &mut inner,
            pane,
            Some(status(PaneStatusState::Working, "")),
            1,
        );
        apply_pane_status(&mut inner, pane, Some(status(PaneStatusState::Done, "")), 2);
    }
    assert!(!summary(&inner, watched).unseen);
    assert!(summary(&inner, hidden).unseen);

    inner.attached.clear();
    assert!(!pane_window_watched(&inner, watched));
    apply_pane_status(
        &mut inner,
        watched,
        Some(status(PaneStatusState::Working, "")),
        3,
    );
    apply_pane_status(
        &mut inner,
        watched,
        Some(status(PaneStatusState::Idle, "")),
        4,
    );
    assert!(summary(&inner, watched).unseen);
}

#[test]
fn a_finish_behind_an_unfocused_client_stays_unseen_until_focus_in() {
    let mut inner = ServerState::default();
    let (session, _, pane) = inner.engine.state.create_session("away").expect("session");
    inner
        .attached
        .entry(session)
        .or_default()
        .insert(ClientId(1));
    inner.clients.insert(
        ClientId(1),
        Box::new(Client {
            focused: Some(false),
            ..Client::default()
        }),
    );
    assert!(pane_window_watched(&inner, pane));
    assert!(!pane_window_seen(&inner, pane));

    apply_pane_status(
        &mut inner,
        pane,
        Some(status(PaneStatusState::Working, "")),
        1,
    );
    apply_pane_status(&mut inner, pane, Some(status(PaneStatusState::Done, "")), 2);
    assert!(summary(&inner, pane).unseen);

    inner.client_mut(ClientId(1)).expect("client").focused = Some(true);
    assert!(pane_window_seen(&inner, pane));
    assert!(mark_focused_window_seen(&mut inner, ClientId(1)));
    assert!(!summary(&inner, pane).unseen);
    assert!(!mark_focused_window_seen(&mut inner, ClientId(1)));
}

#[test]
fn a_new_turn_drops_a_stale_unseen() {
    let mut inner = ServerState::default();
    let (session, _, pane) = inner.engine.state.create_session("turns").expect("session");
    apply_pane_status(
        &mut inner,
        pane,
        Some(status(PaneStatusState::Working, "")),
        1,
    );
    apply_pane_status(&mut inner, pane, Some(status(PaneStatusState::Done, "")), 2);
    assert!(summary(&inner, pane).unseen);

    apply_pane_status(
        &mut inner,
        pane,
        Some(status(PaneStatusState::Working, "")),
        3,
    );
    assert!(!summary(&inner, pane).unseen);

    inner
        .attached
        .entry(session)
        .or_default()
        .insert(ClientId(1));
    apply_pane_status(&mut inner, pane, Some(status(PaneStatusState::Done, "")), 4);
    assert!(!summary(&inner, pane).unseen);
    assert_eq!(summary(&inner, pane).changed_at, 4);
}

#[test]
fn opening_never_stamps_and_agent_panes_finish_only_on_done() {
    let mut inner = ServerState::default();
    let (_, _, terminal) = inner.engine.state.create_session("opens").expect("session");
    let mut context = ExecutionContext::for_pane(&inner.engine.state, terminal).expect("context");
    for command in [
        CommandInvocation::new("set-option", ["-g", "experimental-agent-pane", "on"]),
        CommandInvocation::new("split-window", ["--kind", "picker"]),
        CommandInvocation::new("select-pane-kind", ["agent"]),
    ] {
        inner
            .engine
            .execute(&mut context, &command)
            .expect("agent setup");
    }
    let agent = context.pane.expect("agent pane");

    for pane in [terminal, agent] {
        assert!(apply_pane_status(
            &mut inner,
            pane,
            Some(status(PaneStatusState::Idle, "")),
            10
        ));
        assert_eq!(summary(&inner, pane), PaneSummary::default());
        apply_pane_status(
            &mut inner,
            pane,
            Some(status(PaneStatusState::Working, "")),
            20,
        );
        apply_pane_status(
            &mut inner,
            pane,
            Some(status(PaneStatusState::Idle, "partial")),
            30,
        );
        assert_eq!(summary(&inner, pane).changed_at, 30);
    }
    assert!(summary(&inner, terminal).unseen);
    assert!(!summary(&inner, agent).unseen);

    apply_pane_status(
        &mut inner,
        agent,
        Some(status(PaneStatusState::Done, "")),
        40,
    );
    assert!(summary(&inner, agent).unseen);
}

#[test]
fn agent_state_values_map_to_status_records() {
    let mapped = |value| agent_state_status(Some(value)).map(|status| (status.state, status.kind));
    assert_eq!(mapped("working"), Some((PaneStatusState::Working, None)));
    assert_eq!(mapped("blocked"), Some((PaneStatusState::Blocked, None)));
    assert_eq!(
        mapped("needs-approval"),
        Some((PaneStatusState::Blocked, Some(PaneStatusKind::Permission)))
    );
    assert_eq!(
        mapped("needs-input"),
        Some((PaneStatusState::Blocked, Some(PaneStatusKind::Question)))
    );
    assert_eq!(mapped("failed"), Some((PaneStatusState::Error, None)));
    assert_eq!(mapped("error"), Some((PaneStatusState::Error, None)));
    assert_eq!(mapped("done"), Some((PaneStatusState::Done, None)));
    assert_eq!(mapped("idle"), Some((PaneStatusState::Idle, None)));
    assert_eq!(mapped("starting"), Some((PaneStatusState::Idle, None)));
    assert_eq!(mapped("thinking"), None);
    assert_eq!(agent_state_status(None), None);
}

#[test]
fn agent_state_mirror_covers_terminals_without_7501() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "mirror"]),
        )
        .expect("mux session");
    let pane = context.pane.expect("terminal");
    let target = pane.to_string();
    let set = |value: &str| {
        run(
            &shared,
            &["set-option", "-p", "-t", &target, "@agent_state", value],
        );
    };
    let read = || {
        let inner = shared.inner.lock();
        (pane_status(&inner, pane), summary(&inner, pane))
    };

    set("idle");
    assert_eq!(read().0, None);

    set("working");
    let (status, working) = read();
    assert_eq!(
        status.map(|status| (status.state, status.kind)),
        Some((PaneStatusState::Working, None))
    );
    assert!(working.changed_at > 0);
    assert!(!working.unseen);

    set("idle");
    let (status, idle) = read();
    assert_eq!(
        status.map(|status| status.state),
        Some(PaneStatusState::Idle)
    );
    assert!(idle.unseen);

    set("needs-approval");
    let (status, blocked) = read();
    assert_eq!(
        status.map(|status| (status.state, status.kind)),
        Some((PaneStatusState::Blocked, Some(PaneStatusKind::Permission)))
    );
    assert!(!blocked.unseen);

    run(
        &shared,
        &["set-option", "-pu", "-t", &target, "@agent_state"],
    );
    assert_eq!(read().0, None);

    shared.inner.lock().program_status_panes.insert(pane);
    set("working");
    assert_eq!(read().0, None);
    shared.inner.lock().program_status_panes.remove(&pane);

    let agent = {
        let mut inner = shared.inner.lock();
        for command in [
            CommandInvocation::new("set-option", ["-g", "experimental-agent-pane", "on"]),
            CommandInvocation::new("split-window", ["--kind", "picker"]),
            CommandInvocation::new("select-pane-kind", ["agent"]),
        ] {
            inner
                .engine
                .execute(&mut context, &command)
                .expect("agent setup");
        }
        context.pane.expect("agent pane")
    };
    assert!(matches!(
        shared
            .inner
            .lock()
            .engine
            .state
            .pane(agent)
            .map(|pane| &pane.kind),
        Some(PaneKind::Agent(_))
    ));
    run(
        &shared,
        &[
            "set-option",
            "-p",
            "-t",
            &agent.to_string(),
            "@agent_state",
            "working",
        ],
    );
    assert_eq!(pane_status(&shared.inner.lock(), agent), None);
}

#[test]
fn pane_name_option_reaches_the_summary() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "names"]),
        )
        .expect("mux session");
    let pane = context.pane.expect("terminal");
    let target = pane.to_string();

    run(
        &shared,
        &["set-option", "-p", "-t", &target, "@name", "reviewer"],
    );
    assert_eq!(summary(&shared.inner.lock(), pane).name, "reviewer");

    run(&shared, &["set-option", "-w", "@name", "window"]);
    assert_eq!(summary(&shared.inner.lock(), pane).name, "reviewer");

    let long = "é".repeat(MAX_PANE_NAME_BYTES);
    run(
        &shared,
        &["set-option", "-p", "-t", &target, "@name", &long],
    );
    let name = summary(&shared.inner.lock(), pane).name;
    assert!(name.len() <= MAX_PANE_NAME_BYTES);
    assert!(long.starts_with(&name));

    run(&shared, &["set-option", "-pu", "-t", &target, "@name"]);
    assert_eq!(summary(&shared.inner.lock(), pane).name, "");
    assert_eq!(summary(&shared.inner.lock(), pane).changed_at, 0);
}

#[test]
fn shell_marks_reach_terminal_panes_only() {
    let mut inner = ServerState::default();
    let (_, _, terminal) = inner.engine.state.create_session("marks").expect("session");
    let mut context = ExecutionContext::for_pane(&inner.engine.state, terminal).expect("context");
    let agent = {
        for command in [
            CommandInvocation::new("set-option", ["-g", "experimental-agent-pane", "on"]),
            CommandInvocation::new("split-window", ["--kind", "picker"]),
            CommandInvocation::new("select-pane-kind", ["agent"]),
        ] {
            inner
                .engine
                .execute(&mut context, &command)
                .expect("agent setup");
        }
        context.pane.expect("agent pane")
    };
    let mark = |kind, start, end, status| ShellMark {
        kind,
        facts: PaneOutputFacts {
            command_start_time: start,
            command_end_time: end,
            command_status: status,
            ..PaneOutputFacts::default()
        },
    };

    assert!(!note_shell_mark(
        &mut inner,
        terminal,
        &mark(ShellMarkKind::Prompt, 0, 0, None)
    ));
    assert!(note_shell_mark(
        &mut inner,
        terminal,
        &mark(ShellMarkKind::CommandStarted, 7, 0, None)
    ));
    assert_eq!(
        (
            summary(&inner, terminal).command,
            summary(&inner, terminal).changed_at
        ),
        (Some(ShellCommand::Running), 7)
    );
    assert!(note_shell_mark(
        &mut inner,
        terminal,
        &mark(ShellMarkKind::CommandFinished, 7, 9, Some(2))
    ));
    assert_eq!(
        (
            summary(&inner, terminal).command,
            summary(&inner, terminal).changed_at
        ),
        (Some(ShellCommand::Finished(2)), 9)
    );
    assert!(!note_shell_mark(
        &mut inner,
        terminal,
        &mark(ShellMarkKind::CommandFinished, 7, 9, Some(2))
    ));
    assert!(note_shell_mark(
        &mut inner,
        terminal,
        &mark(ShellMarkKind::CommandStarted, 0, 0, None)
    ));
    assert!(summary(&inner, terminal).changed_at > 9);

    assert!(!note_shell_mark(
        &mut inner,
        agent,
        &mark(ShellMarkKind::CommandFinished, 7, 9, Some(0))
    ));
    assert_eq!(summary(&inner, agent), PaneSummary::default());
}

#[test]
fn plain_output_never_moves_the_tree() {
    let shared = Arc::new(Shared::new(1));
    let (pane, terminal) = empty_terminal_pane(&shared, "churn");
    let generation = || shared.inner.lock().engine.state.generation();

    assert!(terminal.feed(Arc::from(
        b"plain output\r\nmore plain output\r\n".as_slice()
    )));
    wait_until("plain output", || terminal.last_output().is_some());
    let before = generation();
    for _ in 0..20 {
        shared.note_pane_output(pane, &terminal, Instant::now());
    }
    assert_eq!(generation(), before);
    assert_eq!(summary(&shared.inner.lock(), pane), PaneSummary::default());
}

#[cfg(unix)]
fn quiet_pane(shared: &Arc<Shared>, name: &str) -> (PaneId, Arc<TerminalSession>) {
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", name, super::super::tests::QUIET_PANE_COMMAND],
            ),
        )
        .expect("quiet pane");
    let pane = context.pane.expect("pane");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    wait_until("the pane to start", || {
        matches!(
            terminal.latest_viewport().status,
            zz_terminal::SessionStatus::Running
        )
    });
    (pane, terminal)
}

#[cfg(unix)]
#[test]
fn shell_command_marks_reach_the_summary() {
    let shared = Arc::new(Shared::new(1));
    let (pane, terminal) = quiet_pane(&shared, "marks");
    let read = || summary(&shared.inner.lock(), pane);

    terminal.send_text("printf '\\033]133;C\\007'\n");
    wait_until("the command start", || {
        read().command == Some(ShellCommand::Running)
    });
    let started = read().changed_at;
    assert!(started > 0);

    terminal.send_text("printf 'output\\n\\033]133;D;1\\007'\n");
    wait_until("the command finish", || {
        read().command == Some(ShellCommand::Finished(1))
    });
    assert!(read().changed_at >= started);
    assert!(!read().unseen);
    shared.request_shutdown();
}

#[cfg(unix)]
#[test]
fn a_shell_mark_rides_the_title_publish_of_its_frame() {
    let shared = Arc::new(Shared::new(1));
    let terminal = Arc::new(TerminalSession::spawn(
        64,
        Arc::new(TerminalAppearance::default()),
        zz_terminal::TerminalSpawn {
            command: Some(vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                "printf '\\033]133;C\\007'; exec sleep 30".to_owned(),
            ]),
            ..zz_terminal::TerminalSpawn::default()
        },
    ));
    let pane = {
        let mut inner = shared.inner.lock();
        let (_, _, pane) = inner
            .engine
            .state
            .create_session("batched")
            .expect("session");
        inner.terminals_mut().insert(pane, Arc::clone(&terminal));
        pane
    };
    let mailbox = OutboundMailbox::new();
    shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    thread::sleep(Duration::from_millis(50));
    super::super::tests::take_reliable_messages(&mailbox);
    let watcher = Arc::new(Shared {
        server: Arc::clone(&shared.server),
        command_item: None,
        owner: Some(Arc::clone(&shared)),
        watcher_effects: Some(Mutex::new(Vec::new())),
    });

    wait_until("the command mark", || {
        watcher.raise_shell_marks(pane, &terminal);
        summary(&shared.inner.lock(), pane).command == Some(ShellCommand::Running)
    });
    watcher.synchronize_pane_title(pane, &terminal, "make", true);
    let effects = watcher
        .watcher_effects
        .as_ref()
        .expect("watcher context")
        .lock()
        .drain(..)
        .collect::<Vec<_>>();
    assert!(!effects.is_empty());
    for effect in effects {
        effect(&shared);
    }

    let published = super::super::tests::take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::Snapshot(snapshot),
                ..
            }) => snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .find_map(|window| window.panes.get(&pane))
                .map(|pane| (pane.summary.command, pane.title.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        published,
        [(Some(ShellCommand::Running), "make".to_owned())]
    );
    terminal.terminate();
}

#[cfg(unix)]
#[test]
fn program_status_reports_stamp_through_the_daemon() {
    let shared = Arc::new(Shared::new(1));
    let (pane, terminal) = quiet_pane(&shared, "osc-7501");
    let read = || {
        let inner = shared.inner.lock();
        (pane_status(&inner, pane), summary(&inner, pane))
    };
    let sentinel = || {
        shared
            .inner
            .lock()
            .engine
            .state
            .update_pane_summary(pane, |summary| summary.changed_at = 1);
    };

    terminal.send_text("printf '\\033]7501;state=working\\007'\n");
    wait_until("working", || {
        read()
            .0
            .is_some_and(|status| status.state == PaneStatusState::Working)
    });
    let (_, working) = read();
    assert!(working.changed_at > 0);
    assert!(!working.unseen);

    sentinel();
    terminal.send_text("printf '\\033]7501;state=done:msg=YWJj\\007'\n");
    wait_until("done", || {
        read()
            .0
            .is_some_and(|status| status.state == PaneStatusState::Done)
    });
    let (status, done) = read();
    assert_eq!(status.map(|status| status.message), Some("abc".to_owned()));
    assert!(done.changed_at > 1);
    assert!(done.unseen);

    sentinel();
    terminal.send_text("printf '\\033]7501;state=done:msg=eHl6\\007'\n");
    wait_until("the new message", || {
        read().0.is_some_and(|status| status.message == "xyz")
    });
    let (status, message) = read();
    assert_eq!(
        status.map(|status| status.state),
        Some(PaneStatusState::Done)
    );
    assert_eq!(message.changed_at, 1);
    assert!(message.unseen);
    shared.request_shutdown();
}

#[test]
fn a_raised_bell_stamps_the_summary() {
    let shared = Arc::new(Shared::new(1));
    let (pane, _) = empty_terminal_pane(&shared, "bells");
    shared.raise_pane_bell(pane);
    let stamped = summary(&shared.inner.lock(), pane).changed_at;
    assert!(stamped > 0);
    assert!(
        shared
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .expect("pane")
            .bell
    );

    shared
        .inner
        .lock()
        .engine
        .state
        .update_pane_summary(pane, |summary| summary.changed_at = 1);
    shared.raise_pane_bell(pane);
    assert_eq!(summary(&shared.inner.lock(), pane).changed_at, 1);
}

#[test]
fn selecting_attaching_or_typing_clears_unseen() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    let (session, first, second) = {
        let mut inner = shared.inner.lock();
        for command in [
            CommandInvocation::new("new-session", ["-d", "-s", "clears"]),
            CommandInvocation::new("split-window", [] as [&str; 0]),
        ] {
            inner
                .engine
                .execute(&mut context, &command)
                .expect("mux setup");
        }
        let session = context.session.expect("session");
        let window = context.window.expect("window");
        let panes = inner.engine.state.windows[&window].pane_order().to_vec();
        (session, panes[0], panes[1])
    };
    let raise = |pane| {
        assert!(
            shared
                .inner
                .lock()
                .engine
                .state
                .update_pane_summary(pane, |summary| summary.unseen = true)
        );
    };
    let unseen = |pane| summary(&shared.inner.lock(), pane).unseen;
    let active = |pane| {
        let inner = shared.inner.lock();
        let window = inner.engine.state.window_for_pane(pane).expect("window");
        inner.engine.state.windows[&window].active_pane == pane
    };

    let quiet = if active(first) { second } else { first };
    raise(quiet);
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("select-pane", ["-t", &quiet.to_string()]),
        )
        .expect("select pane");
    assert!(active(quiet));
    assert!(!unseen(quiet));

    raise(quiet);
    shared.attach(client, session).expect("attach");
    assert!(!unseen(quiet));

    let other = if quiet == first { second } else { first };
    raise(other);
    shared.note_terminal_input(client, other);
    assert!(!unseen(other));
}

#[cfg(unix)]
#[test]
fn a_pane_death_stamps_the_summary() {
    let shared = Arc::new(Shared::new(1));
    run(
        &shared,
        &["set-window-option", "-g", "remain-on-exit", "on"],
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "died", "exit 3"]),
        )
        .expect("short-lived pane");
    let pane = context.pane.expect("pane");
    wait_until("the pane to die", || {
        shared
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .is_some_and(|pane| pane.dead)
    });
    assert!(summary(&shared.inner.lock(), pane).changed_at > 0);
    shared.request_shutdown();
}

#[cfg(unix)]
fn git_repo(dir: &Path, head: &str) {
    fs::create_dir_all(dir.join(".git")).expect("git dir");
    fs::write(dir.join(".git").join("HEAD"), head).expect("HEAD");
}

#[cfg(unix)]
#[test]
fn find_repo_reads_the_root_and_head_without_git() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let branch = |dir: &Path| find_repo(dir).and_then(|(_, branch)| branch);
    let plain = scratch.path().join("plain");
    git_repo(&plain, "ref: refs/heads/main\n");
    assert_eq!(
        find_repo(&plain),
        Some((plain.as_path(), Some("main".to_owned())))
    );

    let nested = plain.join("src").join("deep");
    fs::create_dir_all(&nested).expect("nested dir");
    assert_eq!(
        find_repo(&nested),
        Some((plain.as_path(), Some("main".to_owned())))
    );

    let linked = plain.join(".git").join("worktrees").join("wt");
    fs::create_dir_all(&linked).expect("worktree git dir");
    fs::write(linked.join("HEAD"), "ref: refs/heads/feature/sidebar\n").expect("worktree HEAD");
    let worktree = scratch.path().join("wt");
    fs::create_dir_all(worktree.join("crates")).expect("worktree");
    fs::write(
        worktree.join(".git"),
        "gitdir: ../plain/.git/worktrees/wt\n",
    )
    .expect("gitdir file");
    assert_eq!(
        find_repo(&worktree.join("crates")),
        Some((worktree.as_path(), Some("feature/sidebar".to_owned())))
    );

    let detached = scratch.path().join("detached");
    git_repo(&detached, "0123456789abcdef0123456789abcdef01234567\n");
    assert_eq!(branch(&detached).as_deref(), Some("0123456"));

    let remote = scratch.path().join("remote");
    git_repo(&remote, "ref: refs/remotes/origin/main\n");
    assert_eq!(branch(&remote).as_deref(), Some("refs/remotes/origin/main"));

    let broken = scratch.path().join("broken");
    fs::create_dir_all(&broken).expect("broken dir");
    fs::write(broken.join(".git"), "not a link\n").expect("broken file");
    assert_eq!(find_repo(&broken), None);

    let reftable = scratch.path().join("reftable");
    git_repo(&reftable, "ref: refs/heads/.invalid\n");
    assert_eq!(find_repo(&reftable), Some((reftable.as_path(), None)));

    let outside = scratch.path().join("outside");
    fs::create_dir_all(&outside).expect("plain dir");
    assert_eq!(find_repo(&outside), None);
}

#[cfg(unix)]
#[test]
fn display_path_folds_home() {
    let home = &[PathBuf::from("/home/me")];
    assert_eq!(display_path(Path::new("/home/me"), home), "~");
    assert_eq!(display_path(Path::new("/home/me/dev/zz"), home), "~/dev/zz");
    assert_eq!(display_path(Path::new("/home/meow"), home), "/home/meow");
    assert_eq!(display_path(Path::new("/srv/app"), &[]), "/srv/app");
    let fits = format!("/{}", "a".repeat(MAX_PANE_CWD_BYTES - 1));
    assert_eq!(display_path(Path::new(&fits), home), fits);
    let long = format!("/{}", "a".repeat(MAX_PANE_CWD_BYTES));
    assert_eq!(display_path(Path::new(&long), home), "");
}

#[cfg(unix)]
#[test]
fn a_symlinked_home_folds_in_both_spellings() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let real = fs::canonicalize(scratch.path())
        .expect("resolved temp dir")
        .join("real");
    fs::create_dir_all(real.join("dev")).expect("home dir");
    let link = scratch.path().join("link");
    std::os::unix::fs::symlink(&real, &link).expect("home symlink");

    let homes = home_forms(link.clone());
    assert_eq!(homes, [link.clone(), real.clone()]);
    assert_eq!(display_path(&link.join("dev"), &homes), "~/dev");
    assert_eq!(display_path(&real.join("dev"), &homes), "~/dev");
    assert_eq!(display_path(&real, &homes), "~");
    assert_eq!(
        resolve_location("", &real.join("dev"), &homes),
        ("~/dev".to_owned(), String::new(), String::new())
    );
    assert_eq!(home_forms(real.clone()), [real]);
}

#[cfg(unix)]
#[test]
fn location_prefers_the_osc7_report_over_the_live_path() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let repo = scratch.path().join("repo");
    git_repo(&repo, "ref: refs/heads/main\n");
    let live = scratch.path().join("live");
    fs::create_dir_all(&live).expect("live dir");
    let report = format!("file://host{}", repo.display());
    let unknown = (String::new(), String::new(), String::new());

    let home = &[scratch.path().to_path_buf()];
    assert_eq!(
        resolve_location(&report, &live, home),
        ("~/repo".to_owned(), "~/repo".to_owned(), "main".to_owned())
    );
    assert_eq!(
        resolve_location(&report, &live, &[]),
        (
            repo.display().to_string(),
            repo.display().to_string(),
            "main".to_owned()
        )
    );
    assert_eq!(
        resolve_location("", &live, home),
        ("~/live".to_owned(), String::new(), String::new())
    );
    assert_eq!(
        resolve_location("file://host/missing/zz-location", &live, &[]),
        unknown
    );
    assert_eq!(resolve_location("", Path::new("live"), &[]), unknown);
    assert_eq!(resolve_location("", Path::new(""), &[]), unknown);

    let long = scratch.path().join("long");
    git_repo(&long, &format!("ref: refs/heads/{}\n", "b".repeat(300)));
    let (.., branch) = resolve_location("", &long, &[]);
    assert_eq!(branch, "b".repeat(MAX_PANE_BRANCH_BYTES));
}

#[cfg(unix)]
#[test]
fn location_jobs_coalesce_while_in_flight() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let repo = scratch.path().join("repo");
    git_repo(&repo, "ref: refs/heads/main\n");
    let report = format!("file://host{}", repo.display());
    let expected = resolve_location(&report, Path::new(""), daemon_home());
    let shared = Arc::new(Shared::new(1));
    let (pane, terminal) = empty_terminal_pane(&shared, "located");
    let location = || {
        let summary = summary(&shared.inner.lock(), pane);
        (summary.cwd, summary.repo, summary.branch)
    };

    shared.inner.lock().locations.jobs.insert(pane, false);
    shared.synchronize_pane_reported_path(pane, &terminal, &report);
    assert_eq!(shared.inner.lock().locations.jobs.get(&pane), Some(&true));
    assert_eq!(location(), Default::default());

    shared.finish_pane_location(
        pane,
        (report.clone(), PathBuf::new()),
        ("stale".to_owned(), "stale".to_owned(), "old".to_owned()),
    );
    assert_ne!(
        location(),
        ("stale".to_owned(), "stale".to_owned(), "old".to_owned())
    );
    wait_until("the resubmitted location", || location() == expected);
    assert!(shared.inner.lock().locations.jobs.is_empty());
    assert_eq!(summary(&shared.inner.lock(), pane).changed_at, 0);

    fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/next\n").expect("HEAD");
    shared.request_pane_location(pane);
    wait_until("the new branch", || location().2 == "next");
    assert!(shared.inner.lock().locations.jobs.is_empty());
}

#[cfg(unix)]
#[test]
fn a_stalled_location_leaves_the_helper_pool_free() {
    use std::os::unix::fs::OpenOptionsExt;

    let scratch = tempfile::tempdir().expect("temp dir");
    let trap = scratch.path().join("trap");
    fs::create_dir_all(trap.join(".git")).expect("git dir");
    let head = trap.join(".git").join("HEAD");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&head)
            .status()
            .expect("mkfifo")
            .success()
    );
    let plain = scratch.path().join("plain.txt");
    fs::write(&plain, b"hello").expect("plain file");
    let shared = Arc::new(Shared::new(1));
    let start = trap.display().to_string();
    for name in ["stall-a", "stall-b"] {
        run(
            &shared,
            &["new-session", "-d", "-s", name, "-c", &start, "exec cat"],
        );
    }

    let reader = Arc::clone(&shared);
    let read = thread::spawn(move || reader.helpers.read(&plain));
    wait_until("an unrelated helper read", || read.is_finished());
    assert_eq!(read.join().expect("reader").expect("read"), b"hello");
    assert!(!shared.inner.lock().locations.jobs.is_empty());

    wait_until("the stalled locations", || {
        if let Ok(writer) = fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&head)
        {
            drop(writer);
        }
        shared.inner.lock().locations.jobs.is_empty()
    });
    shared.request_shutdown();
}

#[cfg(all(unix, feature = "agent"))]
#[test]
fn agent_panes_locate_their_descriptor_cwd() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let repo = scratch.path().join("agent-repo");
    git_repo(&repo, "ref: refs/heads/agents\n");
    let expected = resolve_location("", &repo, daemon_home());
    let shared = Arc::new(Shared::new(1));
    let agent = {
        let mut inner = shared.inner.lock();
        let mut context = ExecutionContext::default();
        let cwd = repo.display().to_string();
        for command in [
            CommandInvocation::new("new-session", ["-d", "-s", "agent-cwd"]),
            CommandInvocation::new("set-option", ["-g", "experimental-agent-pane", "on"]),
            CommandInvocation::new("split-window", ["--kind", "picker"]),
            CommandInvocation::new("select-pane-kind", ["-c", cwd.as_str(), "agent"]),
        ] {
            inner
                .engine
                .execute(&mut context, &command)
                .expect("agent setup");
        }
        context.pane.expect("agent pane")
    };
    let location = || {
        let summary = summary(&shared.inner.lock(), agent);
        (summary.cwd, summary.repo, summary.branch)
    };

    shared.apply_agent_status(agent, status(PaneStatusState::Working, ""));
    wait_until("the agent location", || location() == expected);
    assert_eq!(location().2, "agents");
}

#[cfg(unix)]
#[test]
fn a_shell_pane_finds_its_branch_and_follows_cd() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let first = scratch.path().join("first");
    let second = scratch.path().join("second");
    git_repo(&first, "ref: refs/heads/main\n");
    git_repo(&second, "ref: refs/heads/next\n");
    let shared = Arc::new(Shared::new(1));
    shared.start_timers().expect("timers");
    let mut context = ExecutionContext::default();
    let start = first.display().to_string();
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d",
                    "-s",
                    "follows",
                    "-c",
                    start.as_str(),
                    super::super::tests::QUIET_PANE_COMMAND,
                ],
            ),
        )
        .expect("quiet pane");
    let pane = context.pane.expect("pane");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    let branch = || summary(&shared.inner.lock(), pane).branch;

    wait_until("the first branch", || branch() == "main");
    terminal.send_text(format!(
        "cd '{}' && printf 'moved\\n\\033]133;D;0\\007'\n",
        second.display()
    ));
    wait_until("the second branch", || branch() == "next");
    assert_eq!(
        summary(&shared.inner.lock(), pane).cwd,
        display_path(
            &fs::canonicalize(&second).expect("canonical path"),
            daemon_home()
        )
    );
    shared.request_shutdown();
}

#[test]
fn known_locations_stay_bounded_newest_first() {
    let source = |index: usize| (String::new(), PathBuf::from(format!("/dir/{index}")));
    let mut known = VecDeque::new();
    for index in 0..KNOWN_LOCATIONS + 8 {
        remember_location(
            &mut known,
            source(index),
            &format!("/dir/{index}"),
            "",
            "main",
        );
    }
    assert_eq!(known.len(), KNOWN_LOCATIONS);
    assert_eq!(known[0].source, source(KNOWN_LOCATIONS + 7));
    assert_eq!(known[KNOWN_LOCATIONS - 1].source, source(8));

    remember_location(&mut known, source(20), "/dir/20", "", "next");
    assert_eq!(known.len(), KNOWN_LOCATIONS);
    assert_eq!(
        (&known[0].source, known[0].branch.as_str()),
        (&source(20), "next")
    );
    assert_eq!(
        known
            .iter()
            .filter(|known| known.source == source(20))
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn a_spawn_in_a_known_directory_rides_its_own_publish() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let repo = fs::canonicalize(scratch.path())
        .expect("resolved temp dir")
        .join("repo");
    git_repo(&repo, "ref: refs/heads/main\n");
    let start = repo.display().to_string();
    let expected = resolve_location("", &repo, daemon_home());
    let quiet = super::super::tests::QUIET_PANE_COMMAND;
    let shared = Arc::new(Shared::new(1));
    run(&shared, &["set-option", "-g", "automatic-rename", "off"]);
    run(
        &shared,
        &["new-session", "-d", "-s", "known", "-c", &start, quiet],
    );
    shared.settle_pane_locations();
    let age_known = |age: Duration| {
        let read_at = Instant::now().checked_sub(age).expect("monotonic clock");
        for known in &mut shared.inner.lock().locations.known {
            known.read_at = read_at;
        }
        read_at
    };
    let read_at = || shared.inner.lock().locations.known[0].read_at;
    let spawn = || {
        let before = shared
            .inner
            .lock()
            .terminals
            .keys()
            .copied()
            .collect::<Vec<_>>();
        run(
            &shared,
            &["split-window", "-d", "-t", "known", "-c", &start, quiet],
        );
        let inner = shared.inner.lock();
        let pane = inner
            .terminals
            .keys()
            .copied()
            .find(|pane| !before.contains(pane))
            .expect("new pane");
        let summary = summary(&inner, pane);
        let published = inner.last_published_mux_generation == inner.engine.state.generation();
        (pane, (summary.cwd, summary.repo, summary.branch), published)
    };
    let generation = || shared.inner.lock().engine.state.generation();

    let fresh = age_known(Duration::ZERO);
    let (_, location, published) = spawn();
    assert_eq!(location, expected);
    assert!(published);
    let after_spawn = generation();
    shared.settle_pane_locations();
    assert_eq!(generation(), after_spawn);
    assert_eq!(read_at(), fresh, "a fresh entry is not read again");

    let stale = age_known(LOCATION_FRESH);
    let (_, location, published) = spawn();
    assert_eq!(location, expected);
    assert!(published);
    let after_spawn = generation();
    shared.settle_pane_locations();
    wait_until("the refresh", || read_at() > stale);
    assert_eq!(generation(), after_spawn);

    age_known(LOCATION_FRESH);
    fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/next\n").expect("HEAD");
    let (pane, ..) = spawn();
    wait_until("the switched branch", || {
        summary(&shared.inner.lock(), pane).branch == "next"
    });
    assert_eq!(shared.inner.lock().locations.known[0].branch, "next");
    shared.request_shutdown();
}

#[test]
fn a_read_renews_every_answer_for_its_directory() {
    let live = (String::new(), PathBuf::from("/repo"));
    let report = ("file://host/repo".to_owned(), PathBuf::new());
    let other = (String::new(), PathBuf::from("/other"));
    let gone = (String::new(), PathBuf::from("/gone"));
    let mut known = VecDeque::new();
    remember_location(&mut known, live.clone(), "/repo", "/repo", "main");
    remember_location(&mut known, other.clone(), "/other", "/other", "main");
    remember_location(&mut known, gone.clone(), "", "", "");
    let old = Instant::now()
        .checked_sub(Duration::from_mins(1))
        .expect("monotonic clock");
    for known in &mut known {
        known.read_at = old;
    }

    remember_location(&mut known, report.clone(), "/repo", "/repo", "next");
    remember_location(
        &mut known,
        (String::new(), PathBuf::from("/void")),
        "",
        "",
        "",
    );
    let entry = |source: &LocationSource| {
        let known = known
            .iter()
            .find(|known| known.source == *source)
            .expect("known");
        (known.branch.as_str(), known.read_at > old)
    };
    assert_eq!(entry(&report), ("next", true));
    assert_eq!(entry(&live), ("next", true));
    assert_eq!(entry(&other), ("main", false));
    assert_eq!(entry(&gone), ("", false));
}

#[cfg(unix)]
fn new_pane(shared: &Arc<Shared>, args: &[&str]) -> PaneId {
    let before = shared
        .inner
        .lock()
        .terminals
        .keys()
        .copied()
        .collect::<Vec<_>>();
    run(shared, args);
    shared
        .inner
        .lock()
        .terminals
        .keys()
        .copied()
        .find(|pane| !before.contains(pane))
        .expect("new pane")
}

#[cfg(unix)]
#[test]
fn a_read_reaches_every_pane_and_answer_for_its_directory() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let repo = fs::canonicalize(scratch.path())
        .expect("resolved temp dir")
        .join("repo");
    git_repo(&repo, "ref: refs/heads/main\n");
    let start = repo.display().to_string();
    let quiet = super::super::tests::QUIET_PANE_COMMAND;
    let shared = Arc::new(Shared::new(1));
    run(&shared, &["set-option", "-g", "automatic-rename", "off"]);
    let split = ["split-window", "-d", "-t", "alias", "-c", &start, quiet];
    let first = new_pane(
        &shared,
        &["new-session", "-d", "-s", "alias", "-c", &start, quiet],
    );
    shared.settle_pane_locations();
    let second = new_pane(&shared, &split);
    let branch = |pane| summary(&shared.inner.lock(), pane).branch;
    assert_eq!(
        (branch(first), branch(second)),
        ("main".into(), "main".into())
    );

    fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/next\n").expect("HEAD");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&first]);
    let report = format!("file://host{}", repo.display());
    shared.synchronize_pane_reported_path(first, &terminal, &report);
    shared.settle_pane_locations();
    assert_eq!(
        (branch(first), branch(second)),
        ("next".into(), "next".into())
    );

    let third = new_pane(&shared, &split);
    assert_eq!(branch(third), "next");
    assert!(shared.inner.lock().locations.jobs.is_empty());
    shared.request_shutdown();
}

#[cfg(unix)]
#[test]
fn a_reused_answer_is_read_again_within_the_window() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let repo = fs::canonicalize(scratch.path())
        .expect("resolved temp dir")
        .join("repo");
    git_repo(&repo, "ref: refs/heads/main\n");
    let start = repo.display().to_string();
    let quiet = super::super::tests::QUIET_PANE_COMMAND;
    let shared = Arc::new(Shared::new(1));
    shared.start_timers().expect("timers");
    run(&shared, &["set-option", "-g", "automatic-rename", "off"]);
    let first = new_pane(
        &shared,
        &["new-session", "-d", "-s", "recheck", "-c", &start, quiet],
    );
    shared.settle_pane_locations();
    let branch = |pane| summary(&shared.inner.lock(), pane).branch;
    assert_eq!(branch(first), "main");

    fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/next\n").expect("HEAD");
    let second = new_pane(
        &shared,
        &["split-window", "-d", "-t", "recheck", "-c", &start, quiet],
    );
    let taken = Instant::now();
    assert_eq!(branch(second), "main");
    assert!(shared.inner.lock().locations.jobs.is_empty());
    wait_until("the recheck", || {
        branch(first) == "next" && branch(second) == "next"
    });
    assert!(taken.elapsed() < LOCATION_FRESH);
    assert!(shared.inner.lock().locations.unread.is_empty());
    shared.request_shutdown();
}

#[cfg(unix)]
#[test]
fn a_move_into_a_known_directory_publishes_the_tree() {
    let scratch = tempfile::tempdir().expect("temp dir");
    let repo = fs::canonicalize(scratch.path())
        .expect("resolved temp dir")
        .join("repo");
    fs::create_dir_all(&repo).expect("repo dir");
    let report = format!("file://host{}", repo.display());
    let shared = Arc::new(Shared::new(1));
    let (pane, terminal) = empty_terminal_pane(&shared, "moves");
    for (source, cwd, branch) in [
        ((String::new(), repo.clone()), "~/live", "main"),
        ((report.clone(), PathBuf::new()), "~/report", "next"),
    ] {
        remember_location(
            &mut shared.inner.lock().locations.known,
            source,
            cwd,
            "",
            branch,
        );
    }
    let location = || {
        let inner = shared.inner.lock();
        let summary = summary(&inner, pane);
        (
            summary.cwd,
            summary.branch,
            inner.last_published_mux_generation == inner.engine.state.generation(),
            inner.locations.jobs.is_empty(),
        )
    };

    shared.apply_pane_runtime(
        pane,
        &terminal,
        "",
        Some(repo.display().to_string()),
        false,
        Instant::now(),
    );
    assert_eq!(location(), ("~/live".into(), "main".into(), true, true));

    shared.publish_flush.lock().frozen = Some(Instant::now() + Duration::from_hours(1));
    shared.synchronize_pane_reported_path(pane, &terminal, &report);
    assert_eq!(location(), ("~/report".into(), "next".into(), true, true));
}
