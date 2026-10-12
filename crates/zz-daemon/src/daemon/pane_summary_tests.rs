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
