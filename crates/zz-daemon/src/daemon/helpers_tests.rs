use super::super::*;
use super::*;
use std::{sync::mpsc, time::Instant};

#[test]
fn helpers_are_bounded_and_retire_within_one_second() {
    let pool = Pool::default();
    assert_eq!(pool.state.queue.lock().workers, 0);
    let (started, ready) = mpsc::sync_channel(2);
    let mut releases = Vec::new();
    for _ in 0..2 {
        let (release, wait) = mpsc::channel();
        releases.push(release);
        pool.submit(Task::Hold(wait, started.clone())).unwrap();
    }
    for _ in 0..2 {
        ready.recv_timeout(Duration::from_secs(1)).unwrap();
    }
    assert_eq!(pool.state.queue.lock().workers, 2);
    for _ in 0..MAX_PENDING {
        let (release, wait) = mpsc::channel();
        releases.push(release);
        pool.submit(Task::Hold(wait, started.clone())).unwrap();
    }
    let (_, wait) = mpsc::channel();
    assert_eq!(
        pool.submit(Task::Hold(wait, started)).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(pool.state.queue.lock().workers, 2);
    drop(ready);
    for release in releases {
        release.send(()).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    while pool.state.queue.lock().workers != 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let (release, wait) = mpsc::channel();
    let (started, ready) = mpsc::sync_channel(1);
    pool.submit(Task::Hold(wait, started)).unwrap();
    ready.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(pool.state.queue.lock().workers, 1);
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while pool.active() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn steady_send_keys_at_twenty_panes_starts_no_helpers() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    let run = |context: &mut ExecutionContext, name: &str, args: &[&str]| {
        shared
            .execute(
                ClientId(1),
                ClientKind::Command,
                context,
                &CommandInvocation::new(name, args.iter().copied()),
            )
            .unwrap();
    };
    run(
        &mut context,
        "new-session",
        &["-d", "-s", "s", "exec sleep 1000000"],
    );
    for _ in 0..19 {
        run(
            &mut context,
            "new-window",
            &["-d", "-t", "s:", "exec sleep 1000000"],
        );
    }
    run(&mut context, "send-keys", &["-t", "s:0.0", ""]);
    let (starts, submitted) = {
        let queue = shared.helpers.state.queue.lock();
        (queue.starts, queue.submitted)
    };
    for _ in 0..20 {
        run(&mut context, "send-keys", &["-t", "s:0.0", ""]);
    }
    {
        let queue = shared.helpers.state.queue.lock();
        eprintln!(
            "send-keys p20: starts={} tasks={}",
            queue.starts - starts,
            queue.submitted - submitted
        );
        assert_eq!(queue.starts, starts);
        assert_eq!(queue.submitted, submitted);
    }
    shared.request_shutdown();
}

#[cfg(unix)]
#[test]
fn a_full_helper_queue_never_parks_the_mux_loop() {
    let pool = Pool::default();
    *pool.state.loop_thread.lock() = Some(thread::current().id());
    let (started, ready) = std::sync::mpsc::sync_channel(2);
    let mut releases = Vec::new();
    for _ in 0..MAX_WORKERS + MAX_PENDING {
        let (release, wait) = std::sync::mpsc::channel();
        releases.push(release);
        pool.submit(Task::Hold(wait, started.clone())).unwrap();
        if releases.len() <= MAX_WORKERS {
            ready.recv_timeout(Duration::from_secs(1)).unwrap();
        }
    }
    let (_, wait) = std::sync::mpsc::channel();
    let before = std::time::Instant::now();
    assert_eq!(
        pool.submit_wait(Task::Hold(wait, started))
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert!(before.elapsed() < Duration::from_millis(50));
    drop(ready);
    for release in releases {
        release.send(()).unwrap();
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while pool.active() {
        assert!(std::time::Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(all(feature = "agent", unix))]
#[test]
fn a_peer_probe_leaves_no_idle_worker() {
    let pool = Pool::default();
    pool.submit(Task::Peers {
        panes: Vec::new(),
        reply: None,
        completed: None,
    })
    .unwrap();
    assert!(matches!(
        pool.results.recv_timeout(Duration::from_secs(1)).unwrap(),
        Result::Peers(_)
    ));
    let mut queue = pool.state.queue.lock();
    let deadline = std::time::Instant::now() + Duration::from_millis(50);
    while queue.workers != 0 {
        assert!(
            !pool
                .state
                .available
                .wait_until(&mut queue, deadline)
                .timed_out()
        );
    }
}

#[cfg(all(feature = "agent", unix))]
#[test]
fn a_panicking_peer_scan_completes_and_the_next_scan_can_finish() {
    let (completed, completions) = crossbeam_channel::unbounded();
    for panic in [true, false] {
        let (reply, result) = std::sync::mpsc::sync_channel(1);
        assert!(
            finish_peer_scan(
                || {
                    assert!(!panic, "peer scan failure");
                    Ok(Vec::new())
                },
                Some(reply),
                Some(completed.clone()),
            )
            .is_none()
        );
        assert_eq!(result.try_recv().unwrap().is_err(), panic);
        assert!(matches!(
            completions.try_recv().unwrap(),
            super::super::timers::TimerCompletion::Peer
        ));
    }
    assert!(matches!(
        finish_peer_scan(|| panic!("peer scan failure"), None, Some(completed)),
        Some(Result::Peers(Err(_)))
    ));
    assert!(matches!(
        completions.try_recv().unwrap(),
        super::super::timers::TimerCompletion::Peer
    ));
}

fn hold_helpers(shared: &Arc<Shared>) -> Vec<mpsc::Sender<()>> {
    let (started, ready) = mpsc::sync_channel(2);
    let mut releases = Vec::new();
    for _ in 0..2 {
        let (release, wait) = mpsc::channel();
        releases.push(release);
        shared
            .helpers
            .submit(super::Task::Hold(wait, started.clone()))
            .unwrap();
        ready.recv_timeout(Duration::from_secs(1)).unwrap();
    }
    releases
}

#[cfg(unix)]
#[test]
fn busy_helpers_park_history_without_stopping_another_client() {
    let shared = Arc::new(Shared::new(7));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history");
    fs::write(&path, "command:from-file\nsearch:needle\n").unwrap();
    shared.prompt_history_source.lock().source = Some((path, 100));
    shared
        .prompt_history_settled
        .store(false, Ordering::Release);
    let context = {
        let mut inner = shared.inner.lock();
        let (_, _, pane) = inner.engine.state.create_session("helperwait").unwrap();
        ExecutionContext::for_pane(&inner.engine.state, pane).unwrap()
    };
    let (target, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("helperwait-target".into()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(target, context.session.unwrap()).unwrap();
    let _event_loop = event_loop::EventLoop::empty(&shared).unwrap();
    let releases = hold_helpers(&shared);
    let command = CommandInvocation::new("command-prompt", ["-b", "-t", "helperwait-target"]);
    let mut prompt = wait_queue::CommandTask::new(
        &shared,
        ClientId(21),
        ClientKind::Command,
        &context,
        1,
        &command,
        false,
    )
    .unwrap();
    let start = Instant::now();
    assert!(matches!(prompt.run(true), wait_queue::Progress::Waiting));
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(!prompt.ready());
    assert!(
        shared
            .inner
            .lock()
            .client(target)
            .unwrap()
            .command_prompt
            .is_none()
    );
    let mut display = wait_queue::CommandTask::new(
        &shared,
        ClientId(22),
        ClientKind::Command,
        &context,
        2,
        &CommandInvocation::new("display-message", ["-p", "loop-alive"]),
        false,
    )
    .unwrap();
    assert!(matches!(display.run(true), wait_queue::Progress::Done));
    assert!(
        matches!(display.finish().0, CommandResponse::Success { output, .. } if output == "loop-alive")
    );
    assert_eq!(shared.helpers.state.queue.lock().busy, 2);
    for release in releases {
        release.send(()).unwrap();
    }
    let result = shared
        .helpers
        .results
        .recv_timeout(Duration::from_secs(1))
        .unwrap();
    shared.apply_helper_result(result);
    assert!(prompt.ready());
    assert!(matches!(prompt.run(true), wait_queue::Progress::Done));
    assert!(matches!(prompt.finish().0, CommandResponse::Success { .. }));
    assert!(
        shared
            .inner
            .lock()
            .client(target)
            .unwrap()
            .command_prompt
            .is_some()
    );
    assert_eq!(shared.inner.lock().history.command_history, ["from-file"]);
    assert_eq!(shared.connection_threads.worker_count(), 0);
}

#[cfg(unix)]
#[test]
fn synchronous_file_helpers_refuse_the_loop_thread() {
    let shared = Arc::new(Shared::new(7));
    let _event_loop = event_loop::EventLoop::empty(&shared).unwrap();
    let releases = hold_helpers(&shared);
    assert_eq!(
        shared
            .helpers
            .read(Path::new("/tmp/helperwait-missing"))
            .unwrap_err()
            .kind(),
        ErrorKind::WouldBlock
    );
    assert!(
        matches!(shared.helpers.import_source(Path::new("/tmp/helperwait-missing")), Err(DaemonError::Io(error)) if error.kind() == ErrorKind::WouldBlock)
    );
    #[cfg(feature = "agent")]
    shared.sync_claude_peer_states();
    assert_eq!(shared.helpers.state.queue.lock().busy, 2);
    drop(releases);
}

#[cfg(unix)]
#[test]
fn busy_helpers_preserve_record_clear_order_and_allow_a_loaded_prompt() {
    let shared = Arc::new(Shared::new(7));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history");
    let context = {
        let mut inner = shared.inner.lock();
        let (_, _, pane) = inner.engine.state.create_session("helperwait").unwrap();
        inner
            .engine
            .execute(
                &mut ExecutionContext::default(),
                &CommandInvocation::new(
                    "set-option",
                    ["-s", "history-file", path.to_str().unwrap()],
                ),
            )
            .unwrap();
        inner.history.search_history.push("needle".into());
        ExecutionContext::for_pane(&inner.engine.state, pane).unwrap()
    };
    let (target, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("helperwait-target".into()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(target, context.session.unwrap()).unwrap();
    let _event_loop = event_loop::EventLoop::empty(&shared).unwrap();
    let releases = hold_helpers(&shared);
    shared.record_prompt_history(CommandPromptType::Command, "before-clear");
    let mut clear = wait_queue::CommandTask::new(
        &shared,
        ClientId(21),
        ClientKind::Command,
        &context,
        1,
        &CommandInvocation::new("clear-prompt-history", ["-T", "command"]),
        false,
    )
    .unwrap();
    assert!(matches!(clear.run(true), wait_queue::Progress::Waiting));
    shared.record_prompt_history(CommandPromptType::Command, "after-clear");
    let mut prompt = wait_queue::CommandTask::new(
        &shared,
        ClientId(22),
        ClientKind::Command,
        &context,
        2,
        &CommandInvocation::new("command-prompt", ["-b", "-t", "helperwait-target"]),
        false,
    )
    .unwrap();
    assert!(matches!(prompt.run(true), wait_queue::Progress::Done));
    assert!(matches!(prompt.finish().0, CommandResponse::Success { .. }));
    let display = shared
        .execute(
            ClientId(23),
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("display-message", ["-p", "loop-alive"]),
        )
        .unwrap();
    assert_eq!(display.output, "loop-alive");
    assert_eq!(shared.helpers.state.queue.lock().busy, 2);
    for release in releases {
        release.send(()).unwrap();
    }
    for _ in 0..3 {
        let result = shared
            .helpers
            .results
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        shared.apply_helper_result(result);
    }
    assert!(clear.ready());
    assert!(matches!(clear.run(true), wait_queue::Progress::Done));
    assert!(matches!(clear.finish().0, CommandResponse::Success { .. }));
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "command:after-clear\nsearch:needle\n"
    );
    assert_eq!(shared.connection_threads.worker_count(), 0);
}

#[cfg(unix)]
#[test]
fn loop_completion_hooks_park_config_reads_on_execution_workers() {
    let shared = Arc::new(Shared::new(7));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mux.conf");
    fs::write(&path, "set -g @helper-hook-complete yes\n").unwrap();
    *shared.mux_config_selection.lock() = (true, Some(vec![path]));
    shared
        .execute(
            ClientId(21),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-hook", ["-g", "after-load-buffer", "reload-config"]),
        )
        .unwrap();
    let poll = mio::Poll::new().unwrap();
    let waker = Arc::new(mio::Waker::new(poll.registry(), mio::Token(1)).unwrap());
    shared.helpers.install(Arc::clone(&waker));
    shared.loop_active.store(true, Ordering::Release);
    let mut timers = timers::LoopTimers::new(&shared, &waker);
    let releases = hold_helpers(&shared);
    shared.run_event_hooks(vec![PendingHookEvent {
        name: "after-load-buffer",
        context: ExecutionContext::default(),
        variables: BTreeMap::new(),
        exclude_client: None,
        control_notified: false,
    }]);
    let start = Instant::now();
    timers.turn(&shared, &waker).unwrap();
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(!timers.hooks.is_empty());
    let display = shared
        .execute(
            ClientId(22),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("display-message", ["-p", "loop-alive"]),
        )
        .unwrap();
    assert_eq!(display.output, "loop-alive");
    assert_eq!(shared.helpers.state.queue.lock().busy, 2);
    drop(releases);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !timers.hooks.is_empty() {
        assert!(Instant::now() < deadline);
        timers.turn(&shared, &waker).unwrap();
        thread::sleep(Duration::from_millis(1));
    }
    let result = shared
        .execute(
            ClientId(22),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("show-options", ["-gqv", "@helper-hook-complete"]),
        )
        .unwrap();
    assert_eq!(result.output, "yes");
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.connection_threads.worker_count() != 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
}
