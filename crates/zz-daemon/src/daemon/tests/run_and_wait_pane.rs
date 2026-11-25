use super::*;

#[test]
fn capture_pane_parser_supports_history_mode_and_wrapped_output() {
    let args = ["-pJS", "-", "-ME4", "-t%7", "-bcaptured"].map(RawText::from);
    let parsed = parse_capture_pane_args(&args).expect("capture args");
    assert_eq!(parsed.target.as_deref(), Some("%7"));
    assert_eq!(parsed.buffer_name.as_deref(), Some("captured"));
    assert_eq!(parsed.start.as_deref(), Some("-"));
    assert_eq!(parsed.end.as_deref(), Some("4"));
    assert!(parsed.options.mode);
    assert!(parsed.options.join_wrapped);
    assert!(parsed.options.preserve_trailing);
    assert!(parsed.print);

    let compact = ["-pS0", "-pE5", "-pb", "named"].map(RawText::from);
    let parsed = parse_capture_pane_args(&compact).expect("compact capture args");
    assert_eq!(parsed.start.as_deref(), Some("0"));
    assert_eq!(parsed.end.as_deref(), Some("5"));
    assert_eq!(parsed.buffer_name.as_deref(), Some("named"));
    assert!(parsed.print);

    let value_wins = parse_capture_pane_args(&["-pSx".into()])
        .expect("a value option consumes the cluster remainder");
    assert_eq!(value_wins.start.as_deref(), Some("x"));

    let error = parse_capture_pane_args(&["-pS".into()])
        .expect_err("a clustered value option still requires a value");
    assert!(matches!(
        error,
        ServerError::CommandParse(message)
            if message == "capture-pane -S requires a value"
    ));
}

#[test]
fn capture_pane_parser_takes_line_flags_and_hyperlinks() {
    let parsed = parse_capture_pane_args(&["-pFH".into()]).expect("capture args");
    assert!(parsed.options.line_flags);
    assert!(parsed.options.hyperlinks);
    let parsed = parse_capture_pane_args(&["-p".into()]).expect("capture args");
    assert!(!parsed.options.line_flags);
    assert!(!parsed.options.hyperlinks);
}

#[cfg(unix)]
#[test]
fn capture_pane_prints_line_flags_and_each_new_link_like_tmux() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    let run = |context: &mut ExecutionContext, args: &[&str]| {
        shared
            .execute(
                client,
                ClientKind::Command,
                context,
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .expect("command")
            .output
    };
    run(
        &mut context,
        &[
            "new-session",
            "-d",
            "-s",
            "links",
            "-x",
            "20",
            "-y",
            "6",
            "printf 'plain\\r\\n\\033]133;C\\007out\\r\\n\\033]8;;http://a\\007aa\\033]8;;\\007 \\033]8;;http://b\\007bb\\033]8;;\\007\\r\\nzz-ready'; exec /bin/cat",
        ],
    );
    let target = context.pane.expect("links pane").to_string();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !run(&mut context, &["capture-pane", "-p", "-t", &target]).contains("zz-ready") {
        assert!(Instant::now() < deadline, "the pane never printed");
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        run(
            &mut context,
            &["capture-pane", "-pF", "-S", "0", "-E", "3", "-t", &target]
        ),
        "- plain\nO out\nH aa bb\n- zz-ready\n"
    );
    assert_eq!(
        run(&mut context, &["capture-pane", "-pH", "-t", &target]),
        "http://a http://b\n"
    );
    assert_eq!(
        run(
            &mut context,
            &["capture-pane", "-pH", "-E", "1", "-t", &target]
        ),
        "\n"
    );
    run(
        &mut context,
        &["capture-pane", "-H", "-b", "links", "-t", &target],
    );
    run(
        &mut context,
        &[
            "capture-pane",
            "-H",
            "-b",
            "nolinks",
            "-E",
            "1",
            "-t",
            &target,
        ],
    );
    assert_eq!(
        run(&mut context, &["list-buffers", "-F", "#{buffer_name}"]),
        "links"
    );
    assert_eq!(
        run(&mut context, &["show-buffer", "-b", "links"]),
        "http://a http://b\n"
    );
}

#[cfg(unix)]
#[test]
fn run_pane_returns_the_child_exit_code_and_output() {
    let (shared, client, target) =
        send_text_fixture("runchild", "printf 'zz-ready\\r\\n'; PS1='' exec /bin/sh");
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "run-pane",
                ["-t", &target, "--", "sh", "-c", "echo hi; exit 7"],
            ),
        )
        .expect_err("child exits seven");
    assert!(
        matches!(&error, DaemonError::CommandExit { output, exit_code: 7 } if output == "hi\n"),
        "{error:?}"
    );
    let execution = shared
        .execute(
            client,
            ClientKind::Control,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "run-pane",
                ["-t", &target, "--timeout", "0", "--", "printf 'ok'"],
            ),
        )
        .expect("zero child status");
    assert_eq!(execution.output, "ok");
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("run-pane", ["-t", &target, "--", "sh -c 'exit 124'"]),
        )
        .expect_err("child status 124 is preserved");
    assert!(matches!(
        error,
        DaemonError::CommandExit { exit_code: 124, .. }
    ));
}

#[cfg(unix)]
#[test]
fn run_pane_allows_echo_to_take_longer_than_two_seconds() {
    let (shared, client, target) = send_text_fixture(
        "runechodelay",
        r#"stty -echo -icanon min 1; exec sh -c 'printf "zz-ready\r\n"; line=$(dd bs=4096 count=1 2>/dev/null); sleep 2.2; printf %s "$line"; dd bs=1 count=1 >/dev/null 2>&1; eval "$line"; sleep 5'"#,
    );
    let started = Instant::now();
    let result = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "run-pane",
                ["-t", &target, "--timeout", "10", "printf delayed"],
            ),
        )
        .expect("echo uses the full command timeout");
    assert_eq!(result.output, "delayed");
    assert!(started.elapsed() >= Duration::from_millis(2200));
}

#[cfg(unix)]
#[test]
fn run_pane_times_out_with_125() {
    let (shared, client, target) =
        send_text_fixture("runtimeout", "printf 'zz-ready\\r\\n'; PS1='' exec /bin/sh");
    shared
        .inner
        .lock()
        .client_entry(client)
        .command_streams
        .replace(CommandStreams::default());
    let started = Instant::now();
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "run-pane",
                [
                    "-t",
                    &target,
                    "--timeout",
                    "1",
                    "--",
                    "printf 'partial\\n'; sleep 10",
                ],
            ),
        )
        .expect_err("child did not finish before timeout");
    assert!(started.elapsed() >= Duration::from_secs(1));
    assert!(
        matches!(&error, DaemonError::CommandExit { output, exit_code: 125 } if output == "partial\n"),
        "{error:?}"
    );
    assert_eq!(
        shared.inner.lock().clients[&client]
            .command_streams
            .as_ref()
            .unwrap()
            .stderr,
        format!("run-pane: timed out after 1s on {target}; the command keeps running\n")
    );
}

#[cfg(unix)]
#[test]
fn run_pane_refuses_a_pane_that_is_not_running_a_shell() {
    let (shared, client, target) =
        send_text_fixture("runcat", "printf 'zz-ready\\r\\n'; exec /bin/cat");
    let (result, stderr, elapsed) = run_pane_exit(&shared, client, &target, &["true"]);
    assert_eq!(result, Err((String::new(), 3)));
    assert_eq!(
        stderr,
        format!("run-pane: {target} is running cat, not a shell; nothing was typed\n")
    );
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
    let screen = wait_for_capture(&shared, client, &target, |_| true);
    assert!(!screen.contains("ZZRUN-"), "{screen:?}");
}

#[cfg(unix)]
#[test]
fn run_pane_interrupts_a_shell_waiting_for_more_input() {
    let (shared, client, target) =
        send_text_fixture("runps2", "printf 'zz-ready\\r\\n'; PS1='' exec /bin/sh");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("send-keys", ["-t", &target, "echo 'open", "Enter"]),
        )
        .expect("open a quote");
    wait_for_capture(&shared, client, &target, |screen| {
        screen.lines().any(|line| line.trim_end() == ">")
    });
    let directory = tempfile::tempdir().expect("temporary directory");
    let kept = directory.path().join("kept.txt");
    std::fs::write(&kept, "kept").expect("write the kept file");
    let command = format!(
        "cd {} && rm -f kept.txt && grep -c 'x' /dev/null",
        directory.path().display()
    );
    let (result, stderr, elapsed) = run_pane_exit(&shared, client, &target, &["--", &command]);
    assert_eq!(result, Err((String::new(), 3)));
    assert!(kept.exists(), "the typed command ran inside the open quote");
    assert_eq!(
        stderr,
        format!(
            "run-pane: {target}'s shell did not start the command within 2s (it was waiting \
                 for more input, such as an unclosed quote); sent C-c, nothing ran\n"
        )
    );
    assert!(elapsed < Duration::from_secs(8), "{elapsed:?}");
    let (result, _, _) = run_pane_exit(&shared, client, &target, &["--", "echo ok"]);
    assert_eq!(result, Ok("ok\n".to_owned()));
    let (result, _, elapsed) = run_pane_exit(&shared, client, &target, &["--", "echo 'unbalanced"]);
    assert!(
        matches!(&result, Err((_, code)) if ![0, 3, 124, 125].contains(code)),
        "{result:?}"
    );
    assert!(elapsed < Duration::from_secs(8), "{elapsed:?}");
    let (result, _, _) =
        run_pane_exit(&shared, client, &target, &["--", "echo \"it's\" 'a b' hi!"]);
    assert_eq!(result, Ok("it's a b hi!\n".to_owned()));
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("send-keys", ["-t", &target, "false &&", "Enter"]),
        )
        .expect("leave an unfinished line");
    wait_for_capture(&shared, client, &target, |screen| {
        screen
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .is_some_and(|line| line.trim_end() == ">")
    });
    let (result, stderr, _) = run_pane_exit(&shared, client, &target, &["--", "echo joined"]);
    assert_eq!(result, Ok("joined\n".to_owned()));
    assert_eq!(
        stderr,
        format!(
            "run-pane: {target}'s shell joined the command to an unfinished line (one ending \
                 in && or \\, for example); the output may include text from that line\n"
        )
    );
}

#[cfg(unix)]
#[test]
fn run_pane_refuses_a_shell_script_running_in_the_foreground() {
    let (shared, client, target) =
        send_text_fixture("runscript", "printf 'zz-ready\\r\\n'; PS1='' exec /bin/sh");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "send-keys",
                [
                    "-t",
                    &target,
                    "sh -c 'echo started; sleep 1.5; echo slept'",
                    "Enter",
                ],
            ),
        )
        .expect("start a script");
    wait_for_capture(&shared, client, &target, |screen| {
        screen.lines().any(|line| line.trim_end() == "started")
    });
    let (result, stderr, elapsed) = run_pane_exit(&shared, client, &target, &["--", "echo typed"]);
    assert_eq!(result, Err((String::new(), 3)));
    assert!(
        stderr.starts_with(&format!("run-pane: {target} is running a "))
            && stderr.ends_with(" script, not a shell; nothing was typed\n"),
        "{stderr:?}"
    );
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    let screen = wait_for_capture(&shared, client, &target, |screen| {
        screen.lines().any(|line| line.trim_end() == "slept")
    });
    assert!(!screen.contains("ZZRUN-"), "{screen:?}");
}

#[cfg(unix)]
#[test]
fn run_pane_without_an_echo_submits_nothing() {
    let (shared, client, target) = send_text_fixture(
        "runnoecho",
        "stty -echo; printf 'zz-ready\\r\\n'; exec sh -c 'sleep 30; :'",
    );
    let (result, stderr, _) = run_pane_exit(
        &shared,
        client,
        &target,
        &["--timeout", "0.5", "--", "true"],
    );
    assert_eq!(result, Err((String::new(), 3)));
    assert_eq!(
        stderr,
        format!("run-pane: {target} did not echo the command within 0.5s; nothing was submitted\n")
    );
}

#[cfg(unix)]
#[test]
fn run_pane_takes_turns_on_one_pane() {
    let (shared, client, target) = send_text_fixture(
        "runturns",
        "printf 'zz-ready\\r\\n'; PS1='' exec /bin/sh +m",
    );
    let other = ClientId(8);
    let first = {
        let shared = Arc::clone(&shared);
        let target = target.clone();
        thread::spawn(move || {
            run_pane_exit(&shared, client, &target, &["--", "sleep 1; echo A-done"])
        })
    };
    thread::sleep(Duration::from_millis(200));
    let (result, _, elapsed) = run_pane_exit(&shared, other, &target, &["--", "echo B-done"]);
    assert_eq!(result, Ok("B-done\n".to_owned()));
    assert!(elapsed >= Duration::from_millis(600), "{elapsed:?}");
    assert_eq!(first.join().unwrap().0, Ok("A-done\n".to_owned()));
    let first = {
        let shared = Arc::clone(&shared);
        let target = target.clone();
        thread::spawn(move || {
            run_pane_exit(&shared, client, &target, &["--", "sleep 1; echo A-again"])
        })
    };
    thread::sleep(Duration::from_millis(200));
    let (result, stderr, _) = run_pane_exit(
        &shared,
        other,
        &target,
        &["--timeout", "0.3", "--", "echo B-never"],
    );
    assert_eq!(result, Err((String::new(), 3)));
    assert_eq!(
        stderr,
        format!("run-pane: {target} is busy with another run-pane; nothing was typed\n")
    );
    assert_eq!(first.join().unwrap().0, Ok("A-again\n".to_owned()));
    let screen = wait_for_capture(&shared, client, &target, |_| true);
    assert!(!screen.contains("B-never"), "{screen:?}");
}

#[cfg(unix)]
#[test]
fn run_pane_tells_a_queued_call_with_no_time_left_the_pane_is_busy() {
    let (shared, client, target) =
        send_text_fixture("runfloor", "printf 'zz-ready\\r\\n'; PS1='' exec /bin/sh");
    let first = {
        let shared = Arc::clone(&shared);
        let target = target.clone();
        thread::spawn(move || {
            run_pane_exit(&shared, client, &target, &["--", "sleep 0.8; echo A-done"])
        })
    };
    thread::sleep(Duration::from_millis(200));
    let (result, stderr, _) = run_pane_exit(
        &shared,
        ClientId(8),
        &target,
        &["--timeout", "1", "--", "echo B-never"],
    );
    assert_eq!(result, Err((String::new(), 3)));
    assert_eq!(
        stderr,
        format!("run-pane: {target} is busy with another run-pane; nothing was typed\n")
    );
    assert_eq!(first.join().unwrap().0, Ok("A-done\n".to_owned()));
    let screen = wait_for_capture(&shared, client, &target, |_| true);
    assert!(!screen.contains("B-never"), "{screen:?}");
}

#[cfg(unix)]
#[test]
fn run_pane_refuses_a_pipeline_or_a_shell_driving_a_raw_mode_program() {
    let (shared, client, target) = send_text_fixture(
        "runpipeline",
        "printf 'zz-ready\\r\\n'; PS1='' exec /bin/sh",
    );
    for (command, ready) in [
        ("true | sleep 30", None),
        (
            "sh -c 'stty raw -echo; (sleep 0.3; printf \"cat-up\\r\\n\") & cat; :'",
            Some("cat-up"),
        ),
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("send-keys", ["-t", &target, command, "Enter"]),
            )
            .expect("start the foreground job");
        match ready {
            Some(ready) => {
                wait_for_capture(&shared, client, &target, |screen| {
                    screen.lines().any(|line| line.trim() == ready)
                });
            }
            None => thread::sleep(Duration::from_millis(300)),
        }
        let (result, stderr, elapsed) =
            run_pane_exit(&shared, client, &target, &["--", "echo typed"]);
        assert_eq!(result, Err((String::new(), 3)), "{command}: {stderr}");
        assert!(
            stderr.starts_with(&format!("run-pane: {target} is running "))
                && stderr.ends_with(", not a shell; nothing was typed\n"),
            "{command}: {stderr:?}"
        );
        assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
        let screen = wait_for_capture(&shared, client, &target, |_| true);
        assert!(!screen.contains("ZZRUN-"), "{screen:?}");
        send_keys(&shared, client, &target, &["C-c"]);
    }
}

#[cfg(unix)]
#[test]
fn run_pane_refuses_a_dead_pane_at_once() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    for command in [
        CommandInvocation::new("set-window-option", ["-g", "remain-on-exit", "on"]),
        CommandInvocation::new("new-session", ["-d", "-s", "rundead", "exit 3"]),
    ] {
        shared
            .execute(client, ClientKind::Command, &mut context, &command)
            .expect("seed the dead pane");
    }
    let pane = context.pane.expect("dead pane");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !shared
        .inner
        .lock()
        .engine
        .state
        .pane(pane)
        .is_some_and(|pane| pane.dead)
    {
        assert!(Instant::now() < deadline, "the pane never died");
        thread::sleep(Duration::from_millis(10));
    }
    let (result, stderr, elapsed) =
        run_pane_exit(&shared, client, &pane.to_string(), &["--", "true"]);
    assert_eq!(result, Err((String::new(), 3)));
    assert_eq!(
        stderr,
        format!("run-pane: {pane} is dead (its program exited); nothing was typed\n")
    );
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
}

#[cfg(unix)]
#[test]
fn run_pane_refuses_a_line_longer_than_the_shell_takes() {
    let command = format!("echo {}", "\"(x)\"".repeat(60));
    let bytes = run_pane_line("ZZRUN-0123456789abcdef", "command eval", &command).len();
    let (shared, client, target) = send_text_fixture(
        "runlong",
        "printf 'zz-ready\\r\\n'; PS1='' exec /bin/bash --norc --noediting",
    );
    let (result, stderr, elapsed) = run_pane_exit(&shared, client, &target, &["--", &command]);
    assert_eq!(result, Err((String::new(), 3)));
    assert_eq!(
        stderr,
        format!(
            "run-pane: the command is {bytes} bytes once quoted, more than {target}'s shell \
                 takes on one line; put it in a script file\n"
        )
    );
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    let screen = wait_for_capture(&shared, client, &target, |_| true);
    assert!(!screen.contains("ZZRUN-"), "{screen:?}");
    let (result, _, _) = run_pane_exit(&shared, client, &target, &["--", "echo short"]);
    assert_eq!(result, Ok("short\n".to_owned()));
}

#[cfg(unix)]
#[test]
fn run_pane_says_nothing_ran_when_the_shell_rejects_the_line() {
    let (shared, client, target) = send_text_fixture(
        "runrejected",
        "printf 'zz-ready\\r\\n'; PS1='$ ' exec /bin/sh",
    );
    send_keys(&shared, client, &target, &["case x in", "Enter"]);
    let directory = tempfile::tempdir().expect("temporary directory");
    let flag = directory.path().join("flag");
    let command = format!("touch {}; echo 'ran it'", flag.display());
    let (result, stderr, elapsed) = run_pane_exit(&shared, client, &target, &["--", &command]);
    assert_eq!(result, Err((String::new(), 3)), "{stderr}");
    assert_eq!(
        stderr,
        format!(
            "run-pane: {target}'s shell rejected the line (it was inside an unfinished \
                 construct, such as case x in); nothing ran\n"
        )
    );
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
    assert!(!flag.exists());
    let (result, _, _) = run_pane_exit(&shared, client, &target, &["--", "echo after"]);
    assert_eq!(result, Ok("after\n".to_owned()));
}

#[cfg(unix)]
#[test]
fn run_pane_clears_a_line_its_client_left_without_enter() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let received = directory.path().join("received");
    let (shared, _, target) = send_text_fixture(
        "runabandoned",
        &format!(
            "stty raw -echo; printf 'zz-ready\\r\\n'; exec sh -c 'cat > {}; :'",
            received.display()
        ),
    );
    let (client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    let run = {
        let shared = Arc::clone(&shared);
        let target = target.clone();
        thread::spawn(move || {
            shared.execute(
                client,
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("run-pane", ["-t", &target, "--", "touch abandoned"]),
            )
        })
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !std::fs::read(&received)
        .unwrap_or_default()
        .ends_with(b"$?")
    {
        assert!(Instant::now() < deadline, "the line never reached the pane");
        thread::sleep(Duration::from_millis(10));
    }
    shared.unregister(client);
    let _ = run.join().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let bytes = std::fs::read(&received).unwrap_or_default();
        if bytes.ends_with(b"$?\x15") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the abandoned line was not cleared: {:?}",
            String::from_utf8_lossy(&bytes)
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!std::fs::read(&received).unwrap().contains(&b'\r'));
}

#[cfg(unix)]
#[test]
fn run_pane_notes_output_the_scrollback_dropped() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    for command in [
        CommandInvocation::new("set-option", ["-g", "history-limit", "200"]),
        CommandInvocation::new(
            "new-session",
            [
                "-d",
                "-s",
                "runtruncated",
                "printf 'zz-ready\\r\\n'; PS1='' exec /bin/sh",
            ],
        ),
    ] {
        shared
            .execute(client, ClientKind::Command, &mut context, &command)
            .expect("seed the run-pane terminal");
    }
    let target = context.pane.expect("run-pane pane").to_string();
    wait_for_capture(&shared, client, &target, |screen| {
        screen.contains("zz-ready")
    });
    shared
        .inner
        .lock()
        .client_entry(client)
        .command_streams
        .replace(CommandStreams::default());
    let execution = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("run-pane", ["-t", &target, "--", "seq 1 2000"]),
        )
        .expect("seq exits zero");
    assert!(
        execution.output.ends_with("\n1999\n2000\n"),
        "{:?}",
        execution.output
    );
    let lines = execution.output.lines().count();
    assert!(lines < 2000, "{lines}");
    assert_eq!(
        shared.inner.lock().clients[&client]
            .command_streams
            .as_ref()
            .unwrap()
            .stderr,
        format!(
            "run-pane: output truncated to its last {lines} lines ({target} history-limit \
                 200); redirect to a file for all of it\n"
        )
    );
}

#[cfg(unix)]
#[test]
fn run_pane_names_its_own_line_cap_when_that_cut_the_output() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    for command in [
        CommandInvocation::new("set-option", ["-g", "history-limit", "50000"]),
        CommandInvocation::new(
            "new-session",
            [
                "-d",
                "-s",
                "runcapped",
                "printf 'zz-ready\\r\\n'; PS1='' exec /bin/sh",
            ],
        ),
    ] {
        shared
            .execute(client, ClientKind::Command, &mut context, &command)
            .expect("seed the run-pane terminal");
    }
    let target = context.pane.expect("run-pane pane").to_string();
    wait_for_capture(&shared, client, &target, |screen| {
        screen.contains("zz-ready")
    });
    let (result, stderr, _) = run_pane_exit(&shared, client, &target, &["--", "seq 1 12000"]);
    let output = result.expect("seq exits zero");
    assert!(output.ends_with("\n11999\n12000\n"), "{output:?}");
    assert_eq!(
        stderr,
        format!(
            "run-pane: output truncated to its last {} lines (run-pane keeps the last \
                 {RUN_PANE_CAPTURE_LINES} lines); redirect to a file for all of it\n",
            output.lines().count()
        )
    );
}

#[cfg(unix)]
#[test]
fn wait_pane_until_matches_an_unwrapped_line() {
    let (shared, client, target) =
        send_text_fixture("waitwrapped", "printf 'zz-ready\\r\\n'; exec /bin/cat");
    let line = format!("{}needle{}", "a".repeat(77), "z".repeat(80));
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("send-text", ["-t", &target, &line]),
        )
        .expect("write a wrapped line");
    let screen = wait_for_capture(&shared, client, &target, |screen| {
        screen.lines().filter(|row| row.trim_end() == "zzz").count() == 2
    });
    assert!(!screen.contains(&line), "fixture must wrap the line");
    let execution = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("wait-pane", ["-t", &target, "--until", &line]),
        )
        .expect("logical line matches across wraps");
    assert_eq!(execution.output, format!("{line}\n"));
    let execution = shared
        .execute(
            client,
            ClientKind::Control,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "wait-pane",
                ["-t", &target, "--regex", "^a{77}needlez{80}$"],
            ),
        )
        .expect("regex matches the same logical line");
    assert_eq!(execution.output, format!("{line}\n"));
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("send-text", ["-t", &target, "--no-enter", "tailmarker"]),
        )
        .expect("send-text");
    wait_for_capture(&shared, client, &target, |screen| {
        screen.trim_end().ends_with("tailmarker")
    });
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "wait-pane",
                [
                    "-t",
                    &target,
                    "--until",
                    &line,
                    "--tail",
                    "1",
                    "--timeout",
                    "0.05",
                ],
            ),
        )
        .expect_err("last visible line is the parked text, not the needle");
    assert!(matches!(
        error,
        DaemonError::CommandExit { exit_code: 124, .. }
    ));
}

#[cfg(unix)]
#[test]
fn wait_pane_until_skips_the_prompt_and_typed_command() {
    let (shared, client, target) = send_text_fixture(
        "waitprompt",
        r"printf 'zz-ready\r\n\033]133;A\007$ \033]133;B\007echo DONE\033]133;C\007\r\n'; exec /bin/cat",
    );
    wait_for_capture(&shared, client, &target, |screen| {
        screen.contains("$ echo DONE")
    });
    for condition in ["--until", "--regex"] {
        let error = shared
            .execute(
                client,
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(
                    "wait-pane",
                    ["-t", &target, condition, "DONE", "--timeout", "0.5"],
                ),
            )
            .expect_err("the typed command is not output");
        assert!(matches!(
            error,
            DaemonError::CommandExit { exit_code: 124, .. }
        ));
    }
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("send-text", ["-t", &target, "DONE"]),
        )
        .expect("print a DONE line");
    let execution = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("wait-pane", ["-t", &target, "--until", "DONE"]),
        )
        .expect("an output line matches");
    assert_eq!(execution.output, "DONE\n");
}

#[cfg(unix)]
#[test]
fn wait_pane_until_matches_an_idle_prompt_row_only_once_the_pane_settles() {
    let (shared, client, target) = send_text_fixture(
        "waitidleprompt",
        "stty raw -echo; printf 'zz-ready\\r\\n'; exec /bin/cat",
    );
    let pane = target.parse::<PaneId>().unwrap();
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    assert!(terminal.send_raw_input(Arc::from(
        b"\x1b]133;A\x07$ \x1b]133;B\x07echo DONE-T\r\n".as_slice()
    )));
    wait_for_capture(&shared, client, &target, |screen| {
        screen.contains("$ echo DONE-T")
    });
    let mailbox = OutboundMailbox::new();
    shared
        .client_writers
        .lock()
        .insert(client, Arc::clone(&mailbox));
    let waiting = Arc::clone(&shared);
    let wait_target = target.clone();
    let waiter = thread::spawn(move || {
        let waiting = waiting.command_item(Some((client, 1)));
        waiting.execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "wait-pane",
                ["-t", &wait_target, "--until", "DONE-T", "--timeout", "5"],
            ),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !waiter.is_finished()
        && !take_reliable_messages(&mailbox).iter().any(|message| {
            matches!(
                message,
                ProtocolMessage::CommandQueueParked { request_id: 1 }
            )
        })
    {
        assert!(Instant::now() < deadline, "wait did not park");
        thread::sleep(Duration::from_millis(2));
    }
    assert!(terminal.send_raw_input(Arc::from(
        b"\x1b]133;C\x07DONE-T-OUT\r\n\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07".as_slice()
    )));
    let execution = waiter
        .join()
        .unwrap()
        .expect("the output matches, not the submitted line");
    assert_eq!(execution.output, "DONE-T-OUT\n");
    assert!(terminal.send_raw_input(Arc::from(b"READY-BG\r\n".as_slice())));
    let started = Instant::now();
    let execution = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "wait-pane",
                ["-t", &target, "--until", "READY-BG", "--timeout", "5"],
            ),
        )
        .expect("background output on the idle prompt row matches");
    assert_eq!(execution.output, "$ READY-BG\n");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[cfg(unix)]
#[test]
fn wait_pane_scans_new_scrollback_but_ignores_old_history() {
    let (shared, client, target) = send_text_fixture(
        "waitscrollback",
        "stty raw -echo; printf 'zz-ready\\r\\n'; exec /bin/cat",
    );
    let pane = target.parse::<PaneId>().unwrap();
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    assert!(terminal.send_raw_input(Arc::from(
        format!("oldneedle\r\n{}", "old filler\r\n".repeat(100)).into_bytes()
    )));
    wait_for_capture(&shared, client, &target, |screen| {
        screen.contains("old filler") && !screen.contains("oldneedle")
    });
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "wait-pane",
                ["-t", &target, "--until", "oldneedle", "--timeout", "0.05"],
            ),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        DaemonError::CommandExit { exit_code: 124, .. }
    ));
    for (condition, pattern, expected) in [
        ("--until", "freshneedle", "freshneedle"),
        ("--regex", "^regexneedle$", "regexneedle"),
    ] {
        let mailbox = OutboundMailbox::new();
        shared
            .client_writers
            .lock()
            .insert(client, Arc::clone(&mailbox));
        let waiting = Arc::clone(&shared);
        let wait_target = target.clone();
        let waiter = thread::spawn(move || {
            let waiting = waiting.command_item(Some((client, 1)));
            waiting.execute(
                client,
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(
                    "wait-pane",
                    ["-t", &wait_target, condition, pattern, "--timeout", "2"],
                ),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while !take_reliable_messages(&mailbox).iter().any(|message| {
            matches!(
                message,
                ProtocolMessage::CommandQueueParked { request_id: 1 }
            )
        }) {
            assert!(Instant::now() < deadline, "wait did not park");
            thread::sleep(SEND_TEXT_POLL_INTERVAL);
        }
        assert!(terminal.send_raw_input(Arc::from(
            format!("{expected}\r\n{}", "new filler\r\n".repeat(100)).into_bytes()
        )));
        let execution = waiter
            .join()
            .unwrap()
            .expect("match survives scrolling past the screen between polls");
        assert_eq!(execution.output, format!("{expected}\n"));
    }
}

#[cfg(unix)]
#[test]
fn wait_pane_tail_skips_the_blank_rows_below_the_cursor() {
    let (shared, client, target) =
        send_text_fixture("waittailblank", "printf 'zz-ready\\r\\n'; exec /bin/cat");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("send-text", ["-t", &target, "--no-enter", "tailneedle"]),
        )
        .expect("send-text");
    wait_for_capture(&shared, client, &target, |screen| {
        screen.trim_end().ends_with("tailneedle")
    });
    let execution = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "wait-pane",
                [
                    "-t",
                    &target,
                    "--until",
                    "tailneedle",
                    "--tail",
                    "1",
                    "--timeout",
                    "0",
                ],
            ),
        )
        .expect("the last non-empty line holds the needle");
    assert_eq!(execution.output.to_string().trim_end(), "tailneedle");
}

#[cfg(unix)]
#[test]
fn wait_pane_idle_returns_after_the_dwell() {
    let (shared, client, target) =
        send_text_fixture("waitidle", "printf 'zz-ready\\r\\n'; exec /bin/cat");
    let writer = Arc::clone(&shared);
    let destination = target.clone();
    let started = Instant::now();
    let writing = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        writer
            .execute(
                ClientId(8),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("send-text", ["-t", &destination, "activity"]),
            )
            .expect("output during the idle dwell");
    });
    let execution = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("wait-pane", ["-t", &target]),
        )
        .expect("default idle dwell");
    writing.join().expect("writer");
    assert!(execution.output.is_empty());
    assert!(started.elapsed() >= Duration::from_millis(500));
    let pane = shared
        .inner
        .lock()
        .engine
        .resolve_pane(Some(&target), None, None)
        .expect("pane");
    let last_output = shared.inner.lock().terminals[&pane]
        .last_output()
        .expect("the pane printed");
    assert!(last_output > started);
    assert!(last_output.elapsed() >= Duration::from_millis(500));
    let started = Instant::now();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("wait-pane", ["-t", &target, "--idle", "100"]),
        )
        .expect("an already idle pane still needs a fresh dwell");
    assert!(started.elapsed() >= Duration::from_millis(100));
}

#[cfg(unix)]
#[test]
fn wait_pane_times_out_with_124() {
    let (shared, client, target) =
        send_text_fixture("waittimeout", "printf 'zz-ready\\r\\n'; exec /bin/cat");
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "wait-pane",
                ["-t", &target, "--idle", "2000", "--timeout", "1"],
            ),
        )
        .expect_err("idle dwell exceeds timeout");
    assert!(
        matches!(error, DaemonError::CommandExit { output, exit_code: 124 } if output.contains("--idle 2000") && output.lines().count() == 1)
    );
}

#[cfg(unix)]
#[test]
fn wait_pane_exit_mirrors_the_child_status() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-window-option", ["-g", "remain-on-exit", "on"]),
        )
        .expect("retain the exited pane");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "waitexitstatus", "sh -c 'sleep 0.3; exit 7'"],
            ),
        )
        .expect("create the exiting pane");
    for call in 0..2 {
        let started = Instant::now();
        let error = shared
            .execute(
                client,
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("wait-pane", ["--exit", "-t", "%0"]),
            )
            .expect_err("mirror the child status");
        assert!(matches!(
            error,
            DaemonError::CommandExit { output, exit_code: 7 } if output.is_empty()
        ));
        if call == 1 {
            assert!(started.elapsed() < Duration::from_secs(2));
        }
    }
}

#[cfg(unix)]
#[test]
fn wait_pane_exit_times_out_with_124() {
    let (shared, client, target) =
        send_text_fixture("waitexittimeout", "printf 'zz-ready\\r\\n'; exec sleep 30");
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("wait-pane", ["--exit", "-t", &target, "--timeout", "1"]),
        )
        .expect_err("child outlives the timeout");
    assert!(matches!(
        error,
        DaemonError::CommandExit { output, exit_code: 124 }
            if output == format!("wait-pane: timed out waiting for --exit on {target}\n")
    ));
}

#[cfg(unix)]
#[test]
fn wait_pane_exit_shares_registration_after_another_wait_times_out() {
    let (shared, client, target) =
        send_text_fixture("waitshared", "printf 'zz-ready\\r\\n'; PS1='' exec /bin/sh");
    let pane = target.parse::<PaneId>().unwrap();
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("wait-pane", ["-t", &target, "--exit", "--timeout", "0.05"]),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        DaemonError::CommandExit { exit_code: 124, .. }
    ));
    let waiters = (8..10)
        .map(|id| {
            let waiting = Arc::clone(&shared);
            let wait_target = target.clone();
            thread::spawn(move || {
                waiting.execute(
                    ClientId(id),
                    ClientKind::Command,
                    &mut ExecutionContext::default(),
                    &CommandInvocation::new(
                        "wait-pane",
                        ["-t", &wait_target, "--exit", "--timeout", "2"],
                    ),
                )
            })
        })
        .collect::<Vec<_>>();
    let deadline = Instant::now() + Duration::from_secs(5);
    while shared
        .inner
        .lock()
        .pane_exit_waits
        .get(&pane)
        .is_none_or(|entry| entry.current.waiters() < 2)
    {
        assert!(Instant::now() < deadline, "both waits must share the entry");
        thread::sleep(SEND_TEXT_POLL_INTERVAL);
    }
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("send-text", ["-t", &target, "exit 7"]),
        )
        .unwrap();
    for waiter in waiters {
        assert!(matches!(
            waiter.join().unwrap(),
            Err(DaemonError::CommandExit { exit_code: 7, .. })
        ));
    }
    assert!(!shared.inner.lock().pane_exit_waits.contains_key(&pane));
}

#[cfg(unix)]
#[test]
fn wait_pane_exit_returns_zero_when_the_pane_is_killed_or_respawned() {
    for action in ["kill-pane", "respawn-pane"] {
        let (shared, client, target) =
            send_text_fixture("waitexitkill", "printf 'zz-ready\\r\\n'; exec sleep 30");
        let mailbox = OutboundMailbox::new();
        shared
            .client_writers
            .lock()
            .insert(client, Arc::clone(&mailbox));
        let waiting = Arc::clone(&shared);
        let wait_target = target.clone();
        let waiter = thread::spawn(move || {
            let waiting = waiting.command_item(Some((client, 1)));
            waiting.execute(
                client,
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(
                    "wait-pane",
                    ["--exit", "-t", &wait_target, "--timeout", "10"],
                ),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if take_reliable_messages(&mailbox).iter().any(|message| {
                matches!(
                    message,
                    ProtocolMessage::CommandQueueParked { request_id: 1 }
                )
            }) {
                break;
            }
            assert!(Instant::now() < deadline, "wait did not park");
            thread::sleep(SEND_TEXT_POLL_INTERVAL);
        }
        shared
            .execute(
                ClientId(8),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(
                    action,
                    if action == "kill-pane" {
                        vec!["-t", &target]
                    } else {
                        vec!["-k", "-t", &target, "sleep 30"]
                    },
                ),
            )
            .expect("kill the waited pane");
        let execution = waiter
            .join()
            .expect("wait thread")
            .expect("kill releases wait");
        assert!(execution.output.is_empty());
    }
}

#[cfg(unix)]
#[test]
fn agent_send_wait_follows_the_agent_state_edge_on_terminal_panes() {
    let (shared, client, target) =
        send_text_fixture("agentstateedge", "printf 'zz-ready\\r\\n'; exec /bin/cat");
    let pane = target.parse::<PaneId>().expect("pane id");
    shared.write_pane_agent_state(pane, "idle");
    let waiting = Arc::clone(&shared);
    let wait_target = target.clone();
    let waiter = thread::spawn(move || {
        waiting.execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "agent-send",
                ["-t", &wait_target, "--wait", "--timeout", "10", "hello"],
            ),
        )
    });
    wait_for_capture(&shared, client, &target, |screen| screen.contains("hello"));
    thread::sleep(Duration::from_millis(100));
    assert!(
        !waiter.is_finished(),
        "pre-send idle must not finish the wait"
    );
    shared.write_pane_agent_state(pane, "working");
    thread::sleep(Duration::from_millis(100));
    assert!(!waiter.is_finished(), "working must not finish the wait");
    shared.write_pane_agent_state(pane, "idle");
    let execution = waiter
        .join()
        .expect("wait thread")
        .expect("idle after working");
    assert!(execution.output.is_empty());
}

#[cfg(unix)]
#[test]
fn agent_send_wait_without_text_waits_for_the_running_turn() {
    let (shared, client, target) =
        send_text_fixture("agentstateonly", "printf 'zz-ready\\r\\n'; exec /bin/cat");
    let pane = target.parse::<PaneId>().expect("pane id");
    shared
        .inner
        .lock()
        .engine
        .state
        .pane_mut(pane)
        .expect("pane")
        .summary
        .process = vec!["codex".to_owned()];
    let wait = move |shared: &Arc<Shared>, target: &str| {
        shared.execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "agent-send",
                ["-t", target, "--wait", "--timeout", "10", ""],
            ),
        )
    };
    shared.write_pane_agent_state(pane, "idle");
    assert!(
        wait(&shared, &target)
            .expect("an idle pane")
            .output
            .is_empty()
    );
    shared.write_pane_agent_state(pane, "working");
    let waiting = Arc::clone(&shared);
    let wait_target = target.clone();
    let waiter = thread::spawn(move || wait(&waiting, &wait_target));
    thread::sleep(Duration::from_millis(100));
    assert!(!waiter.is_finished(), "working must not finish the wait");
    shared.write_pane_agent_state(pane, "idle");
    let execution = waiter.join().expect("wait thread").expect("the turn ended");
    assert!(execution.output.is_empty());
    let screen = wait_for_capture(&shared, client, &target, |_| true);
    assert_eq!(screen.trim(), "zz-ready", "nothing was typed");
}

#[cfg(unix)]
#[test]
fn agent_send_wait_reports_blocked_with_on_block_fail() {
    let (shared, client, target) = send_text_fixture(
        "agentstateblocked",
        "printf 'zz-ready\\r\\n'; exec /bin/cat",
    );
    let pane = target.parse::<PaneId>().expect("pane id");
    shared.write_pane_agent_state(pane, "idle");
    let waiting = Arc::clone(&shared);
    let wait_target = target.clone();
    let waiter = thread::spawn(move || {
        waiting.execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "agent-send",
                [
                    "-t",
                    &wait_target,
                    "--wait",
                    "--timeout",
                    "10",
                    "--on-block",
                    "fail",
                    "hello",
                ],
            ),
        )
    });
    wait_for_capture(&shared, client, &target, |screen| screen.contains("hello"));
    thread::sleep(Duration::from_millis(100));
    shared.write_pane_agent_state(pane, "working");
    thread::sleep(Duration::from_millis(100));
    shared.write_pane_agent_state(pane, "blocked");
    let error = waiter
        .join()
        .expect("wait thread")
        .expect_err("blocked state");
    assert!(matches!(
        error,
        DaemonError::CommandExit { output, exit_code: 3 }
            if output == format!("{target}: blocked (agent_state=blocked)")
    ));
}

#[cfg(unix)]
#[test]
fn agent_send_wait_refuses_panes_without_agent_state() {
    let (shared, client, target) =
        send_text_fixture("agentstatenone", "printf 'zz-ready\\r\\n'; exec /bin/cat");
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("agent-send", ["-t", &target, "--wait", "hello"]),
        )
        .expect_err("state is required before sending");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == format!("{target}: agent-send --wait needs an agent state; the pane reports none (@agent_state is unset)")
    ));
}

#[cfg(unix)]
#[test]
fn agent_send_wait_times_out_when_the_state_never_leaves_idle() {
    let (shared, client, target) =
        send_text_fixture("agentstateidle", "printf 'zz-ready\\r\\n'; exec /bin/cat");
    shared.write_pane_agent_state(target.parse().expect("pane id"), "idle");
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "agent-send",
                ["-t", &target, "--wait", "--timeout", "1", "hello"],
            ),
        )
        .expect_err("idle without work times out");
    assert!(matches!(
        error,
        DaemonError::CommandExit { output, exit_code: 124 }
            if output == format!("{target}: no idle state within 1 seconds; the turn may still be running")
    ));
}

#[test]
fn wait_and_run_pane_reject_invalid_arguments() {
    for args in [
        vec!["--idle", "1", "--until", "ready"],
        vec!["--until", "ready", "--regex", "ready"],
        vec!["--regex", "ready", "--idle", "1"],
        vec!["--regex", "["],
        vec!["--exit", "--until", "ready"],
        vec!["--idle", "1", "--exit"],
        vec!["--exit", "--exit"],
        vec!["--exit", "--tail", "1"],
        vec!["--timeout", "-1"],
        vec!["--tail", "-1"],
    ] {
        assert!(matches!(
            parse_wait_pane_args(&args.into_iter().map(RawText::from).collect::<Vec<_>>()),
            Err(ServerError::CommandParse(_))
        ));
    }
    for args in [
        vec![],
        vec!["--"],
        vec!["--timeout", "1"],
        vec!["--", "echo one\necho two"],
    ] {
        assert!(matches!(
            parse_run_pane_args(&args.into_iter().map(RawText::from).collect::<Vec<_>>()),
            Err(ServerError::CommandParse(_))
        ));
    }
    let parsed = parse_run_pane_args(
        &[
            "-t%3",
            "--timeout=4",
            "printf",
            "%s|",
            "a b",
            "it's",
            "",
            "c",
        ]
        .map(RawText::from),
    )
    .expect("command words");
    assert_eq!(parsed.target.as_deref(), Some("%3"));
    assert_eq!(parsed.timeout, Duration::from_secs(4));
    assert_eq!(parsed.command, "printf '%s|' 'a b' 'it'\\''s' '' c");
    let line = parse_run_pane_args(&["--", "sh -c 'echo hi; exit 7' | wc"].map(RawText::from))
        .expect("command line");
    assert_eq!(line.command, "sh -c 'echo hi; exit 7' | wc");
    let shared = Arc::new(Shared::new(1));
    for (verb, args) in [("wait-pane", vec!["--regex", "["]), ("run-pane", vec![])] {
        let error = shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(verb, args),
            )
            .expect_err("usage error before target resolution");
        assert!(matches!(
            error,
            DaemonError::Server(ServerError::NativeCommandParse(_))
        ));
    }
    for (verb, args) in [("wait-pane", vec![]), ("run-pane", vec!["true"])] {
        let error = shared
            .execute(
                ClientId(7),
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(verb, args),
            )
            .expect_err("interactive clients cannot block");
        assert!(
            matches!(error, DaemonError::Server(ServerError::InvalidCommand(message)) if message.contains("command or control client"))
        );
    }
}

#[test]
fn terminal_timeout_contract_uses_fractional_seconds() {
    for verb in ["wait-pane", "run-pane", "send-text", "agent-send"] {
        for (value, expected) in [
            ("0", Duration::ZERO),
            ("0.5", Duration::from_millis(500)),
            ("2.5", Duration::from_millis(2500)),
            ("2000", Duration::from_secs(2000)),
        ] {
            assert_eq!(parse_pane_timeout(verb, value).unwrap(), expected);
        }
        for value in [
            "-1",
            "+1",
            "NaN",
            "inf",
            "1e3",
            "1.",
            ".5",
            "1.2.3",
            "",
            " 2",
            "18446744073709551616",
        ] {
            assert!(
                matches!(parse_pane_timeout(verb, value), Err(ServerError::CommandParse(message))
                if message == format!("{verb} --timeout needs a nonnegative number of seconds"))
            );
        }
    }
    assert_eq!(
        parse_send_text_args(&["--timeout", "0.5", "hello"].map(RawText::from))
            .unwrap()
            .timeout,
        Duration::from_millis(500)
    );
    assert_eq!(
        parse_run_pane_args(&["--timeout", "0", "true"].map(RawText::from))
            .unwrap()
            .timeout,
        Duration::ZERO
    );
    assert_eq!(
        parse_wait_pane_args(&["--timeout", "2.5"].map(RawText::from))
            .unwrap()
            .timeout,
        Duration::from_millis(2500)
    );
    assert_eq!(
        parse_agent_send_args(&["--wait", "--timeout", "0.5", "hello"].map(RawText::from))
            .unwrap()
            .wait_timeout(),
        Some(Duration::from_millis(500))
    );
    let shared = Arc::new(Shared::new(1));
    for verb in ["send-text", "agent-send"] {
        for args in [
            vec!["--target", "%1", "hello"],
            vec!["--target=%1", "hello"],
        ] {
            assert!(matches!(
                shared.execute(
                    ClientId(7),
                    ClientKind::Command,
                    &mut ExecutionContext::default(),
                    &CommandInvocation::new(verb, args)
                ),
                Err(DaemonError::Server(ServerError::NativeCommandParse(_)))
            ));
        }
    }
}

#[cfg(unix)]
#[test]
fn agent_send_rejects_agent_only_flags_before_terminal_delivery() {
    let (shared, client, target) =
        send_text_fixture("agentflags", "printf 'zz-ready\\r\\n'; exec /bin/cat");
    for (flags, flag) in [
        (vec!["--final"], "--final"),
        (vec!["--on-block", "allow"], "--on-block allow|deny"),
        (vec!["--on-block", "deny"], "--on-block allow|deny"),
    ] {
        let message = format!("agent-send: {flag} needs an agent chat pane");
        let mut args = vec!["-t", &target, "--wait"];
        args.extend(flags);
        args.push("must-not-arrive");
        let error = shared
            .execute(
                client,
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("agent-send", args),
            )
            .unwrap_err();
        assert!(
            matches!(error, DaemonError::Server(ServerError::NativeCommandParse(actual)) if actual == message)
        );
    }
    assert!(!wait_for_capture(&shared, client, &target, |_| true).contains("must-not-arrive"));
}

#[test]
fn run_pane_markers_ignore_echo_and_preserve_output() {
    let marker = "ZZRUN-0123456789abcdef";
    let echo = format!(
        "printf '\\n%s\\n' '{marker}-BEGIN'; echo hi; printf '\\n{marker}-RC=%d=END\\n' $?\n"
    );
    assert_eq!(
        run_pane_result(&echo, marker, false),
        (None, None, RunStart::Marked)
    );
    let screen =
        format!("{echo}{marker}-BEGIN\nstale\n{marker}-BEGIN\nhi  \n\n{marker}-RC=7=END\nprompt");
    assert_eq!(
        run_pane_result(&screen, marker, false),
        (Some("hi  \n"), Some(7), RunStart::Marked)
    );
    let screen = format!("before\n{echo}hi\n\n{marker}-RC=0=END\n$ ");
    assert_eq!(
        run_pane_result(&screen, marker, false),
        (Some("hi\n"), Some(0), RunStart::Joined)
    );
    for code in ["%d", "-1", "+1", "256", ""] {
        let screen = format!("{marker}-BEGIN\n{marker}-RC={code}=END\n");
        assert_eq!(run_pane_result(&screen, marker, false).1, None);
    }
    let screen = format!("{marker}-BEGIN\n{}", "line\n".repeat(10_001));
    let (output, code, _) = run_pane_result(&screen, marker, true);
    assert_eq!(code, None);
    assert_eq!(output.expect("retained tail").lines().count(), 10_000);
    let screen = format!("{}\n{marker}-RC=0=END\n$ ", "line\n".repeat(10_001));
    let (output, code, begin) = run_pane_result(&screen, marker, true);
    assert_eq!((code, begin), (Some(0), RunStart::Dropped));
    assert_eq!(output.expect("retained tail").lines().count(), 9_997);
    let screen = format!(
        "{marker}-BEGIN\n{}\n{marker}-RC=0=END\n$ ",
        "line\n".repeat(20)
    );
    assert_eq!(run_pane_result(&screen, marker, false).2, RunStart::Marked);
    let screen = format!("{marker}-BEGIN\npartial\n{}", "\n".repeat(30));
    assert_eq!(
        run_pane_result(&screen, marker, true),
        (Some("partial\n"), None, RunStart::Marked)
    );
    let screen = format!("{marker}-BEGIN\n50%");
    assert_eq!(run_pane_result(&screen, marker, false).0, Some("50%"));
    let screen = format!("{marker}-BEGIN\n{}", "\n".repeat(5));
    assert_eq!(run_pane_result(&screen, marker, false).0, Some(""));
    let echo = echo.trim_end();
    let screen = format!("{echo}    {marker}-BEGIN    after1    {marker}-RC=0=END    $ ");
    assert_eq!(
        run_pane_result(&screen, marker, false),
        (Some("after1"), Some(0), RunStart::Marked)
    );
    let screen = format!("{echo}    {marker}-BEGIN    one\ntwo\n\n{marker}-RC=4=END\n$ ");
    assert_eq!(
        run_pane_result(&screen, marker, false),
        (Some("one\ntwo\n"), Some(4), RunStart::Marked)
    );
    let screen = format!("{echo}    {marker}-BEGIN    still");
    assert_eq!(
        run_pane_result(&screen, marker, false),
        (Some("still"), None, RunStart::Marked)
    );
    let screen = format!("{echo}    after    {marker}-RC=0=END    $ ");
    assert_eq!(
        run_pane_result(&screen, marker, false),
        (Some("after"), Some(0), RunStart::Joined)
    );
}

#[cfg(unix)]
#[test]
fn send_text_verifies_the_echo_then_submits() {
    let (shared, client, target) =
        send_text_fixture("sendtext", "printf 'zz-ready\\r\\n'; exec /bin/cat");
    let execution = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("send-text", ["-t", &target, "hello", "there"]),
        )
        .expect("send-text delivers and submits");
    assert!(execution.output.is_empty());
    wait_for_capture(&shared, client, &target, |screen| {
        screen.matches("hello there").count() == 2
    });
}

#[cfg(unix)]
#[test]
fn send_text_and_run_pane_skip_synchronized_siblings() {
    let (shared, client, target) =
        send_text_fixture("sendsync", "printf 'zz-ready\\r\\n'; PS1='' exec /bin/sh");
    let mut context = ExecutionContext::default();
    for command in [
        CommandInvocation::new(
            "split-window",
            [
                "-d",
                "-t",
                &target,
                "printf 'zz-ready\\r\\n'; PS1='' exec /bin/sh",
            ],
        ),
        CommandInvocation::new(
            "set-option",
            ["-w", "-t", &target, "synchronize-panes", "on"],
        ),
    ] {
        shared
            .execute(client, ClientKind::Command, &mut context, &command)
            .expect("synchronized sibling");
    }
    let sibling = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("list-panes", ["-t", &target, "-F", "#{pane_id}"]),
        )
        .expect("list panes")
        .output
        .lines()
        .find(|pane| *pane != target)
        .expect("sibling pane")
        .to_owned();
    wait_for_capture(&shared, client, &sibling, |screen| {
        screen.contains("zz-ready")
    });
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("send-text", ["-t", &target, "echo sent-once"]),
        )
        .expect("send-text");
    let execution = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("run-pane", ["-t", &target, "--", "echo ran-once"]),
        )
        .expect("run-pane");
    assert_eq!(execution.output, "ran-once\n");
    wait_for_capture(&shared, client, &target, |screen| {
        screen.matches("sent-once").count() == 2
    });
    let screen = wait_for_capture(&shared, client, &sibling, |_| true);
    assert!(
        !screen.contains("sent-once") && !screen.contains("ZZRUN-"),
        "{screen:?}"
    );
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("send-keys", ["-t", &target, "keys-both"]),
        )
        .expect("send-keys");
    wait_for_capture(&shared, client, &sibling, |screen| {
        screen.contains("keys-both")
    });
}

#[cfg(unix)]
#[test]
fn send_text_reports_a_stall_when_nothing_echoes() {
    let (shared, client, target) = send_text_fixture(
        "sendquiet",
        "stty -echo; printf 'zz-ready\\r\\n'; exec /bin/cat",
    );
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("send-text", ["-t", &target, "--timeout", "0.2", "hidden"]),
        )
        .expect_err("no echo means no submit");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message.contains("not echoed within 0.2 seconds")
    ));
    let screen = wait_for_capture(&shared, client, &target, |_| true);
    assert!(
        !screen.contains("hidden"),
        "nothing was submitted: {screen:?}"
    );
}

#[cfg(unix)]
#[test]
fn send_text_submits_a_paste_codex_folds_into_a_placeholder() {
    let (shared, client, target) = send_text_fixture(
        "sendfold",
        r#"stty -echo -icanon min 1; exec sh -c 'printf "zz-ready\r\n"; dd bs=4096 count=1 >/dev/null 2>&1; printf "[Pasted Content 1200 chars]\r\n"; dd bs=1 count=1 >/dev/null 2>&1; printf "submitted\r\n"; sleep 5'"#,
    );
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("send-text", ["-t", &target, "a long brief"]),
        )
        .expect("the placeholder counts as the echo");
    wait_for_capture(&shared, client, &target, |screen| {
        screen.contains("submitted")
    });
}

#[test]
fn send_text_rejects_non_terminal_panes_and_bad_flags() {
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
            &CommandInvocation::new("new-session", ["-s", "sendkinds"]),
        )
        .expect("session");
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "picker", "-v"]),
        )
        .expect("picker");
    let picker = context.pane.expect("picker").to_string();
    for (verb, args) in [
        ("wait-pane", vec!["-t", picker.as_str()]),
        ("run-pane", vec!["-t", picker.as_str(), "true"]),
    ] {
        let error = shared
            .execute(
                client,
                ClientKind::Command,
                &mut context.clone(),
                &CommandInvocation::new(verb, args),
            )
            .expect_err("wait and run require a terminal pane");
        assert!(matches!(
            error,
            DaemonError::Server(ServerError::InvalidTarget(_))
        ));
    }
    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("send-text", ["-t", &picker, "hi"]),
        )
        .expect_err("picker is not a terminal");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidTarget(_))
    ));

    for args in [
        &["--timeout", "-1", "hi"][..],
        &["--timeout", "soon", "hi"],
        &["-x", "hi"],
    ] {
        assert!(matches!(
            parse_send_text_args(
                &args
                    .iter()
                    .map(|arg| RawText::from(*arg))
                    .collect::<Vec<_>>()
            ),
            Err(ServerError::CommandParse(_))
        ));
    }
    let empty = parse_send_text_args(&[]).expect("no flags parse");
    assert!(matches!(
        empty.payload(),
        Err(ServerError::InvalidCommand(_))
    ));
    let parsed = parse_send_text_args(
        &["--no-enter", "--timeout=0.05", "-t%3", "a", "b"].map(RawText::from),
    )
    .expect("flags");
    assert!(parsed.no_enter);
    assert_eq!(parsed.timeout, Duration::from_millis(50));
    assert_eq!(parsed.target.as_deref(), Some("%3"));
    assert_eq!(parsed.payload().expect("payload"), "a b");
    assert_eq!(
        echo_tail("first line\n  second   line  \n\n"),
        "second line"
    );
    assert!(send_text_reads_stdin(&["-t%3".into()]));
    assert!(!send_text_reads_stdin(&["-t%3".into(), "typed".into()]));
    assert!(!send_text_reads_stdin(&["--timeout".into(), "x".into()]));
}

#[test]
fn capture_pane_routes_stdout_buffers_and_reversed_ranges_like_tmux() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d",
                    "-s",
                    "capture",
                    if cfg!(windows) {
                        "echo zz-terminal-ready& echo alpha& echo beta& echo gamma& pause >nul"
                    } else {
                        "printf 'zz-terminal-ready\\r\\nalpha\\r\\nbeta\\r\\ngamma\\r\\n'; exec /bin/cat"
                    },
                ],
            ),
        )
        .expect("seed capture terminal");
    let pane = context.pane.expect("capture pane");
    let target = pane.to_string();
    let expected = "zz-terminal-ready\nalpha\nbeta\ngamma\n";
    let deadline = Instant::now() + Duration::from_secs(5);
    let forward = loop {
        let output = shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "capture-pane",
                    ["-p", "-S", "0", "-E", "3", "-t", &target],
                ),
            )
            .expect("capture forward range")
            .output;
        if output == expected {
            break output;
        }
        assert!(
            Instant::now() < deadline,
            "capture rows did not converge: {output:?}"
        );
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(forward, expected);

    let reversed = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("capture-pane", ["-p", "-S", "3", "-E", "0", "-t", &target]),
        )
        .expect("capture reversed range");
    assert_eq!(reversed.output, expected);
    let middle = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("capture-pane", ["-p", "-S", "1", "-E", "2", "-t", &target]),
        )
        .expect("capture middle range");
    assert_eq!(middle.output, "alpha\nbeta\n");

    let compact = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("capture-pane", ["-pS0", "-pE3", "-t", &target]),
        )
        .expect("capture compact range");
    assert_eq!(compact.output, expected);

    let fallback = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("capture-pane", ["-pSbogus", "-Ebogus", "-t", &target]),
        )
        .expect("capture invalid range defaults");
    let fallback = fallback.output.to_string();
    assert!(fallback.starts_with(expected), "{fallback:?}");
    assert!(
        fallback[expected.len()..].chars().all(|row| row == '\n'),
        "{fallback:?}"
    );

    let expanded = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "capture-pane",
                [
                    "-pS#{?#{==:#{command},capture-pane},1,bogus}",
                    "-E#{?#{==:#{command},capture-pane},2,bogus}",
                    "-t",
                    &target,
                ],
            ),
        )
        .expect("capture format-expanded range");
    assert_eq!(expanded.output, "alpha\nbeta\n");

    let bare = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("capture-pane", ["-S", "0", "-E", "3", "-t", &target]),
        )
        .expect("capture automatic buffer");
    assert!(bare.output.is_empty());
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-buffer", [] as [&str; 0]),
            )
            .expect("show automatic capture")
            .output,
        expected
    );

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-buffer", ["-b", "pinned", "sentinel"]),
        )
        .expect("seed named buffer");
    let automatic_before = shared
        .inner
        .lock()
        .buffers
        .paste_buffers
        .iter()
        .filter(|buffer| buffer.automatic)
        .count();
    let printed = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "capture-pane",
                ["-p", "-b", "pinned", "-S", "0", "-E", "3", "-t", &target],
            ),
        )
        .expect("print capture with ignored buffer name");
    assert_eq!(printed.output, expected);
    assert_eq!(
        shared
            .inner
            .lock()
            .buffers
            .paste_buffers
            .iter()
            .filter(|buffer| buffer.automatic)
            .count(),
        automatic_before
    );
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-buffer", ["-b", "pinned"]),
            )
            .expect("show untouched named buffer")
            .output,
        "sentinel"
    );

    let named = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "capture-pane",
                ["-b", "captured", "-S", "0", "-E", "3", "-t", &target],
            ),
        )
        .expect("capture named buffer");
    assert!(named.output.is_empty());
    assert_eq!(
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-buffer", ["-b", "captured"]),
            )
            .expect("show named capture")
            .output,
        expected
    );
}
