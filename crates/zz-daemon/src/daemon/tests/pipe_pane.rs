use super::*;

#[test]
fn pipe_pane_enforces_arity_and_hard_targets() {
    let shared = Arc::new(Shared::new(1));
    #[cfg(unix)]
    let _loop = pipe_jobs::Driver::new(&shared);
    let error = shared
        .pipe_pane(
            &ExecutionContext::default(),
            "pipe-pane",
            &["one".into(), "two".into()],
        )
        .expect_err("extra shell command");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::CommandParse(message))
            if message == "command pipe-pane: too many arguments (need at most 1)"
    ));
    let error = shared
        .pipe_pane(
            &ExecutionContext::default(),
            "pipe-pane",
            &["-t".into(), "%999".into()],
        )
        .expect_err("missing hard target");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::PaneNotFound(target)) if target == "%999"
    ));
}

#[cfg(unix)]
#[test]
fn pipe_pane_rejects_dead_targets_before_closing_the_existing_pipe() {
    let shared = Arc::new(Shared::new(1));
    #[cfg(unix)]
    let _loop = pipe_jobs::Driver::new(&shared);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(1),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "dead-pipe", "exec /bin/cat"]),
        )
        .expect("new session");
    let pane = context.pane.expect("pane");
    let target = pane.to_string();
    shared
        .pipe_pane(
            &context,
            "pipe-pane",
            &["-t".into(), target.clone().into(), "sleep 60".into()],
        )
        .expect("open pipe");
    shared
        .inner
        .lock()
        .engine
        .state
        .pane_mut(pane)
        .expect("pane state")
        .dead = true;
    let error = shared
        .pipe_pane(&context, "pipe-pane", &["-t".into(), target.into()])
        .expect_err("dead target");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "target pane has exited"
    ));
    assert!(shared.inner.lock().jobs.pane_pipes.contains_key(&pane));
    shared.request_shutdown();
}

#[cfg(unix)]
#[test]
fn pipe_pane_streams_single_shell_burst_without_loss() {
    let shared = Arc::new(Shared::new(1));
    #[cfg(unix)]
    let _loop = pipe_jobs::Driver::new(&shared);
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "pipe-throughput", "exec /bin/sh"]),
        )
        .expect("new throughput session");
    let session = context.session.expect("session");
    let pane = context.pane.expect("pane");
    let target = pane.to_string();
    shared
        .attach(client, session)
        .expect("attach throughput session");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    terminal.resize(200, 50, 8, 16);
    wait_for_viewport(
        &terminal,
        TerminalViewId(client.0),
        "throughput terminal did not attach",
        |viewport| matches!(viewport.status, SessionStatus::Running),
    );
    assert!(terminal.send_raw_input(Arc::from(
        b"stty raw -echo; set +m; printf 'ZZ_PIPE_%s\\n' READY; IFS= read -r command; eval \"$command\" 2>/dev/null; IFS= read -r _\n"
            .as_slice()
    )));
    let ready_deadline = Instant::now() + Duration::from_secs(10);
    while !viewport_text(&terminal.latest_viewport()).contains("ZZ_PIPE_READY") {
        assert!(
            Instant::now() < ready_deadline,
            "throughput terminal did not start"
        );
        thread::sleep(Duration::from_millis(10));
    }

    let directory = tempfile::tempdir().expect("pipe directory");
    let output = directory.path().join("throughput.bin");
    let command_item = directory.path().join("command-item");
    let command = format!(
        "printf '#{{command}}' > {}; sleep 0.1; cat > {}",
        shell_quote(&command_item),
        shell_quote(&output)
    );
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("pipe-pane", ["-t", target.as_str(), command.as_str()]),
        )
        .expect("open throughput pipe");
    let payload_bytes = 2_usize * 1024 * 1024;
    let payload_pattern = b"0123456789abcdef\n";
    let stall_limit = Duration::from_secs(5);
    let started = Instant::now();
    assert!(
        terminal.send_raw_input(Arc::from(
            format!("yes 0123456789abcdef | head -c {payload_bytes}; printf 'ENDMARK\\n'\n")
                .into_bytes()
        ))
    );

    let mut progress = 0;
    let mut deadline = started + stall_limit;
    let captured = loop {
        let captured = fs::read(&output).unwrap_or_default();
        if captured.ends_with(b"ENDMARK\n") {
            break captured;
        }
        if captured.len() > progress {
            progress = captured.len();
            deadline = Instant::now() + stall_limit;
        }
        assert!(
            Instant::now() < deadline,
            "single-burst pipe stalled after {} bytes",
            captured.len()
        );
        thread::sleep(Duration::from_millis(10));
    };
    let elapsed = started.elapsed();
    eprintln!(
        "pipe throughput: bytes={} elapsed={elapsed:?} bytes_per_second={:.0}",
        captured.len(),
        captured.len() as f64 / elapsed.as_secs_f64(),
    );
    let mut expected = payload_pattern.repeat(payload_bytes.div_ceil(payload_pattern.len()));
    expected.truncate(payload_bytes);
    expected.extend_from_slice(b"ENDMARK\n");
    let mismatch = captured
        .iter()
        .zip(&expected)
        .position(|(actual, expected)| actual != expected)
        .unwrap_or(captured.len().min(expected.len()));
    let start = mismatch.saturating_sub(16);
    let captured_end = (mismatch + 16).min(captured.len());
    let expected_end = (mismatch + 16).min(expected.len());
    assert!(
        captured == expected,
        "single-burst output differs at byte {mismatch}: captured {} bytes {:?}, expected {} bytes {:?}",
        captured.len(),
        &captured[start..captured_end],
        expected.len(),
        &expected[start..expected_end]
    );
    assert_eq!(
        fs::read_to_string(&command_item).expect("pipe command item marker"),
        "pipe-pane"
    );
    assert!(shared.inner.lock().jobs.pane_pipes.contains_key(&pane));

    shared.request_shutdown();
}

#[cfg(unix)]
#[test]
fn pipe_pane_has_no_gap_when_control_attaches_during_a_flood() {
    let shared = Arc::new(Shared::new(1));
    #[cfg(unix)]
    let _loop = pipe_jobs::Driver::new(&shared);
    let interactive_mailbox = OutboundMailbox::new();
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&interactive_mailbox),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "pipe-control-flood", "exec /bin/sh"]),
        )
        .expect("new flood session");
    let session = context.session.expect("session");
    let pane = context.pane.expect("pane");
    shared
        .attach(interactive, session)
        .expect("attach flood session");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    terminal.resize(200, 50, 8, 16);
    wait_for_viewport(
        &terminal,
        TerminalViewId(interactive.0),
        "flood terminal did not attach",
        |viewport| matches!(viewport.status, SessionStatus::Running),
    );
    assert!(terminal.send_raw_input(Arc::from(
        b"stty raw -echo; set +m; printf 'ZZ_HANDOFF_%s\\n' READY; IFS= read -r command; eval \"$command\" 2>/dev/null; IFS= read -r _\n"
            .as_slice()
    )));
    let ready_deadline = Instant::now() + Duration::from_secs(10);
    while !viewport_text(&terminal.latest_viewport()).contains("ZZ_HANDOFF_READY") {
        assert!(
            Instant::now() < ready_deadline,
            "flood terminal did not start"
        );
        thread::sleep(Duration::from_millis(10));
    }

    let directory = tempfile::tempdir().expect("pipe directory");
    let output = directory.path().join("handoff.bin");
    let pipe_command = format!("cat > {}", shell_quote(&output));
    shared
        .execute(
            interactive,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "pipe-pane",
                ["-t", pane.to_string().as_str(), pipe_command.as_str()],
            ),
        )
        .expect("open handoff pipe");
    let payload_bytes = 512_usize * 1024;
    assert!(
        terminal.send_raw_input(Arc::from(
            format!("yes 0123456789abcdef | head -c {payload_bytes}; printf 'ENDMARK\\n'\n")
                .into_bytes()
        ))
    );
    let attach_deadline = Instant::now() + Duration::from_secs(10);
    while fs::metadata(&output).map_or(0, |metadata| metadata.len()) < 128 * 1024 {
        assert!(
            Instant::now() < attach_deadline,
            "flood did not reach the handoff point"
        );
        thread::sleep(Duration::from_millis(5));
    }

    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    shared
        .attach(control, session)
        .expect("attach control during flood");
    let control_writer = {
        let mailbox = Arc::clone(&control_mailbox);
        thread::spawn(move || {
            let mut frames = Vec::new();
            while mailbox.recv_batch(&mut frames, MAX_OUTBOUND_BYTES) {
                mailbox.finish_batch(&mut frames);
            }
        })
    };

    let deadline = Instant::now() + Duration::from_secs(30);
    let captured = loop {
        let captured = fs::read(&output).unwrap_or_default();
        if captured.ends_with(b"ENDMARK\n") {
            break captured;
        }
        assert!(
            Instant::now() < deadline,
            "handoff flood stalled after {} bytes",
            captured.len()
        );
        thread::sleep(Duration::from_millis(5));
    };
    let pattern = b"0123456789abcdef\n";
    let mut expected = pattern.repeat(payload_bytes.div_ceil(pattern.len()));
    expected.truncate(payload_bytes);
    expected.extend_from_slice(b"ENDMARK\n");
    if captured != expected {
        let common = captured
            .iter()
            .zip(expected.iter())
            .take_while(|(a, b)| a == b)
            .count();
        let window_end = (common + 256).min(captured.len());
        panic!(
            "pipe handoff mismatch: captured={} expected={} first_divergence={common} window={:?}",
            captured.len(),
            expected.len(),
            String::from_utf8_lossy(&captured[common.saturating_sub(48)..window_end]),
        );
    }

    control_mailbox.close();
    control_writer.join().expect("control writer");
    shared.request_shutdown();
}

#[cfg(unix)]
#[test]
fn pipe_pane_survives_respawn_with_the_same_child() {
    let shared = Arc::new(Shared::new(1));
    #[cfg(unix)]
    let _loop = pipe_jobs::Driver::new(&shared);
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "pipe-respawn", "exec /bin/cat"]),
        )
        .expect("new respawn session");
    let session = context.session.expect("session");
    let pane = context.pane.expect("pane");
    let target = pane.to_string();
    shared
        .attach(client, session)
        .expect("attach respawn session");
    let original_terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    let view = TerminalViewId(client.0);
    wait_for_viewport(
        &original_terminal,
        view,
        "original pipe terminal did not start",
        |viewport| matches!(viewport.status, SessionStatus::Running),
    );

    let directory = tempfile::tempdir().expect("pipe directory");
    let output = directory.path().join("respawn.bin");
    let command = format!("cat > {}", shell_quote(&output));
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("pipe-pane", ["-t", target.as_str(), command.as_str()]),
        )
        .expect("open respawn pipe");
    let (pid, process) = {
        let inner = shared.inner.lock();
        let pipe = &inner.jobs.pane_pipes[&pane];
        (pipe.pid, Arc::clone(&pipe.cancel))
    };

    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    shared
        .attach(control, session)
        .expect("attach control to piped session");
    {
        let inner = shared.inner.lock();
        let pipe = &inner.jobs.pane_pipes[&pane];
        assert_eq!(pipe.pid, pid);
        assert!(Arc::ptr_eq(&pipe.cancel, &process));
        assert!(output_routed(&inner, pane));
    }
    original_terminal.send_raw_input(Arc::from(b"WITH_CONTROL\n".as_slice()));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fs::read(&output)
        .unwrap_or_default()
        .windows(12)
        .any(|window| window == b"WITH_CONTROL")
    {
        assert!(
            Instant::now() < deadline,
            "control attach evicted the live pane pipe"
        );
        thread::sleep(Duration::from_millis(10));
    }
    shared.detach(control);
    {
        let inner = shared.inner.lock();
        let pipe = &inner.jobs.pane_pipes[&pane];
        assert_eq!(pipe.pid, pid);
        assert!(Arc::ptr_eq(&pipe.cancel, &process));
        assert!(output_routed(&inner, pane));
    }
    original_terminal.send_raw_input(Arc::from(b"AFTER_CONTROL\n".as_slice()));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fs::read(&output)
        .unwrap_or_default()
        .windows(13)
        .any(|window| window == b"AFTER_CONTROL")
    {
        assert!(
            Instant::now() < deadline,
            "control detach did not restore the live pane pipe"
        );
        thread::sleep(Duration::from_millis(10));
    }

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "respawn-pane",
                ["-k", "-t", target.as_str(), "--", "/bin/cat"],
            ),
        )
        .expect("respawn piped pane");
    let replacement_terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    assert!(!Arc::ptr_eq(&original_terminal, &replacement_terminal));
    wait_for_viewport(
        &replacement_terminal,
        view,
        "replacement pipe terminal did not start",
        |viewport| matches!(viewport.status, SessionStatus::Running),
    );
    {
        let inner = shared.inner.lock();
        let pipe = &inner.jobs.pane_pipes[&pane];
        assert_eq!(pipe.pid, pid);
        assert!(Arc::ptr_eq(&pipe.cancel, &process));
    }

    replacement_terminal.send_raw_input(Arc::from(b"POST_RESPAWN_PIPE\n".as_slice()));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fs::read(&output)
        .unwrap_or_default()
        .windows(17)
        .any(|window| window == b"POST_RESPAWN_PIPE")
    {
        assert!(
            Instant::now() < deadline,
            "post-respawn output was not piped"
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "display-message",
                    ["-p", "-t", target.as_str(), "#{pane_pipe}:#{pane_pipe_pid}"],
                ),
            )
            .expect("respawn pipe format")
            .output,
        format!("1:{pid}")
    );

    shared.request_shutdown();
    #[cfg(unix)]
    lifecycle::wait_for_cleanup(&shared);
    assert!(process.load(Ordering::Acquire));
}

#[cfg(unix)]
#[test]
fn pipe_pane_streams_raw_output_toggles_formats_injects_and_expands_time() {
    let shared = Arc::new(Shared::new(1));
    #[cfg(unix)]
    let _loop = pipe_jobs::Driver::new(&shared);
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-s",
                    "pipe-pane",
                    "stty raw -echo; printf 'ZZ_PIPE_%s\\n' READY; exec /bin/cat",
                ],
            ),
        )
        .expect("new pipe session");
    let session = context.session.expect("session");
    let pane = context.pane.expect("pane");
    shared.attach(client, session).expect("attach pipe session");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    let view = TerminalViewId(client.0);
    wait_for_viewport(&terminal, view, "pipe terminal did not start", |viewport| {
        matches!(viewport.status, SessionStatus::Running)
            && viewport_text(viewport).contains("ZZ_PIPE_READY")
    });
    terminal.send_raw_input(Arc::from(b"BEFORE_PIPE\n".as_slice()));
    wait_for_viewport(
        &terminal,
        view,
        "pre-pipe output was not rendered",
        |viewport| viewport_text(viewport).contains("BEFORE_PIPE"),
    );

    let directory = tempfile::tempdir().expect("pipe directory");
    let output = directory.path().join("output.bin");
    let command = format!("cat > {}", shell_quote(&output));
    let target = pane.to_string();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("pipe-pane", ["-t", target.as_str(), command.as_str()]),
        )
        .expect("open output pipe");
    let live = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                ["-p", "-t", target.as_str(), "#{pane_pipe}:#{pane_pipe_pid}"],
            ),
        )
        .expect("live pipe format")
        .output;
    let (flag, pid) = live.split_once(':').expect("pipe format fields");
    assert_eq!(flag, "1");
    assert!(pid.parse::<u32>().is_ok(), "{live:?}");
    terminal.send_raw_input(Arc::from(b"AFTER_PIPE\n".as_slice()));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let bytes = fs::read(&output).unwrap_or_default();
        if bytes.windows(10).any(|window| window == b"AFTER_PIPE") {
            assert!(!bytes.windows(11).any(|window| window == b"BEFORE_PIPE"));
            break;
        }
        assert!(Instant::now() < deadline, "pipe output was not written");
        thread::sleep(Duration::from_millis(10));
    }
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("pipe-pane", ["-t", target.as_str()]),
        )
        .expect("close output pipe");
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "display-message",
                    ["-p", "-t", target.as_str(), "#{pane_pipe}:#{pane_pipe_pid}"],
                ),
            )
            .expect("closed pipe format")
            .output,
        "0:"
    );

    let toggled = directory.path().join("toggle.bin");
    let toggle_command = format!("cat > {}", shell_quote(&toggled));
    for expected in [true, false] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "pipep",
                    ["-o", "-t", target.as_str(), toggle_command.as_str()],
                ),
            )
            .expect("toggle pipe");
        assert_eq!(
            shared.inner.lock().jobs.pane_pipes.contains_key(&pane),
            expected
        );
    }
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "pipe-pane",
                ["-t", target.as_str(), toggle_command.as_str()],
            ),
        )
        .expect("reopen pipe");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("pipe-pane", ["-t", target.as_str(), ""]),
        )
        .expect("empty command closes pipe");
    assert!(!shared.inner.lock().jobs.pane_pipes.contains_key(&pane));

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "pipe-pane",
                ["-I", "-t", target.as_str(), "printf 'ZZ_PIPE_INPUT\\n'"],
            ),
        )
        .expect("input pipe");
    wait_for_viewport(
        &terminal,
        view,
        "pipe input did not reach the PTY",
        |viewport| viewport_text(viewport).contains("ZZ_PIPE_INPUT"),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    while shared.inner.lock().jobs.pane_pipes.contains_key(&pane) {
        assert!(Instant::now() < deadline, "input pipe did not reap");
        thread::sleep(Duration::from_millis(10));
    }

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "pipe-pane",
                [
                    "-IO",
                    "-t",
                    target.as_str(),
                    "IFS= read -r line; printf 'ZZ_PIPE_BOTH\\n'",
                ],
            ),
        )
        .expect("bidirectional pipe");
    terminal.send_raw_input(Arc::from(b"ZZ_PIPE_TRIGGER\n".as_slice()));
    wait_for_viewport(
        &terminal,
        view,
        "bidirectional pipe did not reach the PTY",
        |viewport| viewport_text(viewport).contains("ZZ_PIPE_BOTH"),
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    while shared.inner.lock().jobs.pane_pipes.contains_key(&pane) {
        assert!(Instant::now() < deadline, "bidirectional pipe did not reap");
        thread::sleep(Duration::from_millis(10));
    }

    let dated = directory.path().join("dated-#{session_active}-%Y.bin");
    let dated_command = format!("cat > {}", shell_quote(&dated));
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("pipe-pane", ["-t", target.as_str(), dated_command.as_str()]),
        )
        .expect("dated pipe");
    terminal.send_raw_input(Arc::from(b"DATED_PIPE\n".as_slice()));
    let expanded = directory
        .path()
        .join(format!("dated-1-{}.bin", chrono::Local::now().format("%Y")));
    let deadline = Instant::now() + Duration::from_secs(20);
    while !fs::read(&expanded)
        .unwrap_or_default()
        .windows(10)
        .any(|window| window == b"DATED_PIPE")
    {
        assert!(Instant::now() < deadline, "strftime pipe was not written");
        thread::sleep(Duration::from_millis(10));
    }
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("pipe-pane", ["-t", target.as_str()]),
        )
        .expect("close dated pipe");

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("pipe-pane", ["-t", target.as_str(), "sleep 60"]),
        )
        .expect("shutdown pipe");
    #[cfg(unix)]
    let pid = shared.inner.lock().jobs.pane_pipes[&pane].pid;
    #[cfg(windows)]
    let process = Arc::clone(&shared.inner.lock().jobs.pane_pipes[&pane].process);
    shared.request_shutdown();
    #[cfg(unix)]
    lifecycle::wait_for_cleanup(&shared);
    assert!(shared.inner.lock().jobs.pane_pipes.is_empty());
    #[cfg(windows)]
    assert!(process.lock().is_none());
    #[cfg(unix)]
    wait_for_process_group_exit(
        rustix::process::Pid::from_raw(pid as i32).unwrap(),
        Duration::from_secs(8),
        "pipe shutdown",
    );
}
