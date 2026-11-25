use super::super::{ClientId, ClientKind, CommandInvocation, ExecutionContext, Shared};
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
