use super::*;
use crate::daemon::tests::spawn_foreground_shell_job;
use std::process::{Command, Stdio};

fn until(event_loop: &mut EventLoop, shared: &Arc<Shared>, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "shutdown phase did not complete");
        event_loop.poll_test_turn(shared, Duration::from_millis(2));
    }
}

#[test]
fn signal_shutdown_waits_for_a_foreground_job_that_ends_within_its_grace() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("finished");
    let shared = Arc::new(Shared::new(1));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    event_loop.signals = Some(SignalPipes::new(&event_loop.poll).unwrap());
    let worker = spawn_foreground_shell_job(
        &shared,
        ClientId(301),
        format!(
            "sleep 0.2; printf finished > {}",
            zz_daemon_client::shell_quote(marker.to_str().unwrap())
        ),
    );
    event_loop.request_signal_shutdown(&shared, Duration::from_secs(2));
    assert!(!shared.stopping.load(Ordering::Acquire));
    until(&mut event_loop, &shared, || {
        shared.shutdown_cleanup_complete.load(Ordering::Acquire)
            && shared.inner.lock().active_shell_jobs == 0
    });
    worker.join().unwrap().unwrap();
    assert_eq!(fs::read_to_string(marker).unwrap(), "finished");
}

#[test]
fn signal_shutdown_stops_a_foreground_job_that_outlives_its_grace() {
    let shared = Arc::new(Shared::new(1));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    event_loop.signals = Some(SignalPipes::new(&event_loop.poll).unwrap());
    let worker = spawn_foreground_shell_job(&shared, ClientId(302), "sleep 30".to_owned());
    let grace = Duration::from_millis(200);
    let signalled = Instant::now();
    event_loop.request_signal_shutdown(&shared, grace);
    until(&mut event_loop, &shared, || {
        shared.shutdown_cleanup_complete.load(Ordering::Acquire)
            && shared.inner.lock().active_shell_jobs == 0
    });
    assert!(signalled.elapsed() >= grace);
    assert!(worker.join().unwrap().is_err());
    assert_eq!(shared.active_shutdown_blockers(), 0);
    assert!(shared.inner.lock().shell_jobs.is_empty());
}

#[test]
fn repeated_signal_stops_past_active_shutdown_blockers() {
    let shared = Arc::new(Shared::new(1));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let blocker = ShutdownBlocker::acquire(&shared, false).unwrap();
    event_loop.request_signal_shutdown(&shared, Duration::from_secs(2));
    assert!(shared.shutdown_pending.load(Ordering::Acquire));
    assert!(!shared.stopping.load(Ordering::Acquire));
    let second = Instant::now();
    event_loop.request_signal_shutdown(&shared, Duration::from_secs(2));
    until(&mut event_loop, &shared, || {
        shared.shutdown_cleanup_complete.load(Ordering::Acquire)
            && shared.inner.lock().active_shell_jobs == 0
    });
    assert!(second.elapsed() < Duration::from_secs(1));
    assert!(ShutdownBlocker::acquire(&shared, false).is_none());
    drop(blocker);
    assert_eq!(shared.active_shutdown_blockers(), 0);
}

#[test]
fn response_admission_and_writer_drain_leave_socket_output_on_the_loop() {
    let shared = Arc::new(Shared::new(1));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let (token, mut peer, _, outbound) =
        shutdown_tests::registered(&mut event_loop, &shared, ClientKind::Command);
    let mut admission = ResponseAdmissionGuard::new(&shared).unwrap();
    shared.request_shutdown_without_hooks();
    event_loop.turn(&shared).unwrap();
    assert!(matches!(
        event_loop.shutdown_phase,
        ShutdownPhase::Admissions
    ));
    assert!(ResponseAdmissionGuard::new(&shared).is_none());
    io_tests::messages(&mut peer, &mut Inbound::default());
    let output = ProtocolMessage::TreeSync;
    assert!(outbound.enqueue_reliable(&output));
    event_loop.turn(&shared).unwrap();
    assert_eq!(
        io_tests::messages(&mut peer, &mut Inbound::default()),
        vec![output]
    );
    assert!(!event_loop.shutdown_completed());
    admission.finish();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !event_loop.shutdown_completed() {
        assert!(Instant::now() < deadline);
        event_loop.turn(&shared).unwrap();
        thread::sleep(Duration::from_millis(2));
    }
    assert!(!event_loop.connections.contains_key(&token));
    assert!(outbound.state.lock().writer_finished);
}

#[test]
fn child_notification_does_not_reap_a_worker_owned_child() {
    let poll = Poll::new().unwrap();
    let mut pipes = SignalPipes::new(&poll).unwrap();
    let mut child = std::process::Command::new("/bin/sh")
        .args(["-c", "exit 17"])
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while SignalPipes::drain(&mut pipes.child).unwrap() == 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(child.wait().unwrap().code(), Some(17));
}

#[test]
fn child_signal_reaps_only_registered_children_with_coalesced_exits() {
    let socket = PathBuf::from(format!("/tmp/zz-e12-{}.sock", server_id()));
    let listener = LocalTransport::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let _socket_guard = SocketGuard::new(socket);
    let shared = Arc::new(Shared::new(1));
    let mut event_loop = EventLoop::new::<LocalTransport>(&listener, &shared).unwrap();
    let (completed, results) = mpsc::channel();
    let count = Arc::new(AtomicU64::new(0));
    let mut worker_child = Command::new("/bin/sh")
        .args(["-c", "exit 17"])
        .spawn()
        .unwrap();
    for _ in 0..20 {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "sleep 0.02; printf registered; exit 21"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let fd = child.stdout.take().unwrap().into();
        let shared = Arc::clone(&shared);
        let count = Arc::clone(&count);
        let completed = completed.clone();
        event_loop
            .register_job(jobs::Launch {
                child: child.into(),
                descriptors: vec![jobs::Descriptor {
                    fd,
                    read: true,
                    input: None,
                    socket: false,
                }],
                policy: jobs::CompletionPolicy::ChildExitAndEof,
                deadline: Some(Instant::now() + Duration::from_secs(5)),
                process_group: false,
                output_limit: None,
                stream: None,
                pipe: None,
                cancel: None,
                complete: Box::new(move |result| {
                    completed.send(result).unwrap();
                    if count.fetch_add(1, Ordering::SeqCst) == 19 {
                        shared.request_shutdown();
                    }
                }),
            })
            .unwrap();
    }
    thread::sleep(Duration::from_millis(50));
    event_loop
        .run::<LocalTransport>(&listener, &shared, || {})
        .unwrap();
    assert_eq!(worker_child.wait().unwrap().code(), Some(17));
    let results = results.try_iter().collect::<Vec<_>>();
    assert_eq!(results.len(), 20);
    for result in results {
        assert_eq!(result.status.unwrap().code(), Some(21));
        assert_eq!(result.output, vec![b"registered".to_vec()]);
        assert!(result.error.is_none());
        assert!(!result.cancelled);
        let pid = rustix::process::Pid::from_raw(i32::try_from(result.pid).unwrap()).unwrap();
        assert_eq!(
            rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::NOHANG).unwrap_err(),
            rustix::io::Errno::CHILD
        );
    }
    assert_eq!(event_loop.poll_timeout(Instant::now()), None);
}

fn pump(event_loop: &mut EventLoop, id: jobs::JobId) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while event_loop.jobs.contains(id) {
        assert!(Instant::now() < deadline, "status job did not complete");
        event_loop
            .jobs
            .turn(event_loop.poll.registry(), Instant::now());
        event_loop.jobs.child_signal(event_loop.poll.registry());
        match event_loop
            .poll
            .poll(&mut event_loop.events, Some(Duration::from_millis(2)))
        {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => panic!("status job poll failed: {error}"),
        }
        let tokens = event_loop
            .events
            .iter()
            .map(mio::event::Event::token)
            .collect::<Vec<_>>();
        for token in tokens {
            event_loop
                .jobs
                .ready(event_loop.poll.registry(), token, true, false);
        }
    }
}

#[test]
fn status_job_alive_at_loop_shutdown_is_cancelled_and_reaped_once() {
    let shared = Arc::new(Shared::new(1));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let client = shared.status.lock().job_client();
    let updates = client
        .launch(
            1,
            crate::shell_process("printf 'ready\\n'; exec sleep 30"),
            None,
        )
        .unwrap();
    event_loop.turn_status_jobs();
    let id = event_loop.status_jobs[&1];
    let deadline = Instant::now() + Duration::from_secs(3);
    let pid = loop {
        event_loop.jobs.child_signal(event_loop.poll.registry());
        if let Ok(update) = updates.try_recv()
            && update.latest.as_deref() == Some("ready")
        {
            break update.pid;
        }
        assert!(Instant::now() < deadline, "status job did not publish");
        thread::sleep(Duration::from_millis(1));
    };
    event_loop.start_shutdown(&shared);
    event_loop.jobs.cancel_all(event_loop.poll.registry());
    pump(&mut event_loop, id);
    event_loop.jobs.child_signal(event_loop.poll.registry());
    let update = updates.try_recv().unwrap();
    assert!(update.complete);
    assert!(updates.try_recv().is_err());
    let pid = rustix::process::Pid::from_raw(pid as i32).unwrap();
    loop {
        match rustix::process::waitpid(Some(pid), rustix::process::WaitOptions::NOHANG) {
            Err(rustix::io::Errno::INTR) => {}
            result => {
                assert_eq!(result.unwrap_err(), rustix::io::Errno::CHILD);
                break;
            }
        }
    }
    assert!(
        client
            .launch(2, crate::shell_process("exit 0"), None)
            .is_err()
    );
}

#[test]
fn cancellation_kills_inherited_status_output_after_the_shell_exits() {
    let shared = Arc::new(Shared::new(1));
    let mut event_loop = EventLoop::empty(&shared).unwrap();
    let client = shared.status.lock().job_client();
    let updates = client
        .launch(
            1,
            crate::shell_process("sleep 30 & printf 'ready\\n'"),
            None,
        )
        .unwrap();
    event_loop.turn_status_jobs();
    let id = event_loop.status_jobs[&1];
    let deadline = Instant::now() + Duration::from_secs(3);
    let pid = loop {
        event_loop.jobs.child_signal(event_loop.poll.registry());
        if let Ok(update) = updates.try_recv()
            && update.latest.as_deref() == Some("ready")
        {
            break update.pid;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    };
    thread::sleep(Duration::from_millis(20));
    event_loop.jobs.child_signal(event_loop.poll.registry());
    assert!(event_loop.jobs.contains(id));
    client.cancel(1);
    event_loop.turn_status_jobs();
    pump(&mut event_loop, id);
    assert!(updates.try_recv().unwrap().complete);
    let pid = rustix::process::Pid::from_raw(pid as i32).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while rustix::process::test_kill_process_group(pid).is_ok() {
        assert!(
            Instant::now() < deadline,
            "inherited status pipe survived cancellation"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn discovery_jobs_drain_cancel_limit_and_reap_on_the_loop() {
    let socket = PathBuf::from(format!("/tmp/zz-e19-{}.sock", server_id()));
    let listener = LocalTransport::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let _socket_guard = SocketGuard::new(socket);
    let shared = Arc::new(Shared::new(1));
    let mut event_loop = EventLoop::new::<LocalTransport>(&listener, &shared).unwrap();
    let jobs = shared.helpers.jobs.clone();
    let owner = Arc::clone(&shared);
    let worker = thread::spawn(move || {
        let run = |script: &str, limit, timeout, cancel: Arc<AtomicBool>| {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script]);
            jobs.output(command, limit, Instant::now() + timeout, cancel)
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let output = run(
                "i=0; while [ $i -lt 10000 ]; do printf 'error-output-line\\n' >&2; i=$((i + 1)); done; printf ok",
                1024 * 1024,
                Duration::from_secs(5),
                Arc::new(AtomicBool::new(false)),
            ).unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"ok");
            assert_eq!(output.stderr.len(), 180_000);
            let started = Instant::now();
            let cancel = Arc::new(AtomicBool::new(false));
            let signal = Arc::clone(&cancel);
            let notifier = jobs.clone();
            let cancellation = thread::spawn(move || {
                thread::sleep(Duration::from_millis(50));
                signal.store(true, Ordering::Release);
                notifier.notify();
            });
            let error = run("sleep 20", 1024, Duration::from_secs(5), cancel).unwrap_err();
            cancellation.join().unwrap();
            assert!(error.contains("cancelled"), "{error}");
            assert!(started.elapsed() < Duration::from_secs(1));
            let error = run(
                "head -c 10000 /dev/zero",
                1024,
                Duration::from_secs(2),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap_err();
            assert!(error.contains("output limit"), "{error}");
            let started = Instant::now();
            let error = run(
                "sleep 20",
                1024,
                Duration::from_millis(50),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap_err();
            assert!(error.contains("timed out"), "{error}");
            assert!(started.elapsed() < Duration::from_secs(1));
        }));
        owner.request_shutdown();
        outcome.unwrap();
    });
    event_loop
        .run::<LocalTransport>(&listener, &shared, || {})
        .unwrap();
    worker.join().unwrap();
    assert!(
        event_loop
            .helper_jobs
            .iter()
            .all(|(id, _)| !event_loop.jobs.contains(*id))
    );
    assert_eq!(event_loop.jobs.next(Instant::now()), None);
}
