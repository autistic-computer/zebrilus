use super::*;

#[test]
fn shell_command_arguments_match_the_pin_grammar() {
    assert_eq!(parse_shell_delay("").expect("empty delay"), Duration::ZERO);
    assert_eq!(
        parse_shell_delay("   ").expect("whitespace delay"),
        Duration::ZERO
    );
    assert_eq!(
        parse_shell_delay("0x2").expect("hex delay"),
        Duration::from_secs(2)
    );
    assert_eq!(
        parse_shell_delay(".125").expect("fractional delay"),
        Duration::from_millis(125)
    );
    for value in ["abc", "1abc"] {
        assert!(matches!(
            parse_shell_delay(value),
            Err(ServerError::InvalidCommand(message))
                if message == format!("invalid delay time: {value}")
        ));
    }

    let parsed = parse_run_shell_args(&["-s", "ignored", "printf ok"].map(RawText::from))
        .expect("hidden -s option");
    assert_eq!(parsed.positional, ["printf ok"]);
    assert!(!parsed.positive_delay);
    for value in ["", "0", "-0", "+0", "0x0"] {
        let parsed =
            parse_run_shell_args(&["-d".into(), value.to_owned().into(), "printf ok".into()])
                .expect("zero delay");
        assert!(!parsed.positive_delay, "{value}");
    }
    for value in [".001", "+.001", "0x2"] {
        let parsed =
            parse_run_shell_args(&["-d".into(), value.to_owned().into(), "printf ok".into()])
                .expect("positive delay");
        assert!(parsed.positive_delay, "{value}");
    }

    assert!(matches!(
        parse_if_shell_args(&["condition".into()]),
        Err(ServerError::CommandParse(message))
            if message == "command if-shell: too few arguments (need at least 2)"
    ));
    assert!(matches!(
        parse_if_shell_args(
            &["condition", "yes", "no", "extra"].map(RawText::from)
        ),
        Err(ServerError::CommandParse(message))
            if message == "command if-shell: too many arguments (need at most 3)"
    ));
}

#[test]
fn if_shell_typed_arguments_reject_conditions_and_preserve_branch_kind() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if-shell",
                [
                    "-F",
                    "{ display-message -p condition }",
                    "set-option -g @typed-condition-effect yes",
                ],
            )
            .with_command_blocks([1]),
        )
        .expect_err("typed condition must fail before its branch");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::CommandParse(message))
            if message == "command if-shell: argument 1 must be \"string\""
    ));
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-options", ["-gqv", "@typed-condition-effect"]),
        )
        .expect("query rejected branch effect");
    assert!(output.output.is_empty());

    for (spelling, condition, expected) in
        [("if-shell", "1", "typed-true"), ("if", "0", "typed-false")]
    {
        let output = shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    spelling,
                    [
                        "-F",
                        condition,
                        "{ display-message -p typed-true }",
                        "{ display-message -p typed-false }",
                    ],
                )
                .with_command_blocks([2, 3]),
            )
            .unwrap_or_else(|error| panic!("{spelling}: {error}"));
        assert_eq!(output.output, expected);
    }

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if-shell",
                [
                    test_shell::SUCCEED,
                    "{ display-message -p typed-shell-true }",
                    "{ display-message -p typed-shell-false }",
                ],
            )
            .with_command_blocks([1, 2]),
        )
        .expect("typed foreground shell branch");
    assert_eq!(output.output, "typed-shell-true");

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("if-shell", ["-F", "1", "{ display-message -p quoted }"]),
        )
        .expect_err("quoted braces remain a string");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::PostAdmissionCallback(error))
            if matches!(&*error, ServerError::CommandParse(message) if message == "syntax error")
    ));
}

#[test]
fn if_shell_bodies_expand_the_live_global_environment() {
    let shared = Arc::new(Shared::new(1));
    shared
        .inner
        .lock()
        .engine
        .seed_global_environment([("FOO", "hello")]);
    let output = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("if-shell", ["-F", "1", "display-message -p $FOO"]),
        )
        .expect("environment-backed if-shell body");

    assert_eq!(output.output, "hello");
}

#[test]
fn shell_job_working_directory_follows_tmux_precedence() {
    let mut inner = ServerState::default();
    let (target, target_window, target_pane) = inner
        .engine
        .state
        .create_session("job-target")
        .expect("target session");
    let (attached, _, _) = inner
        .engine
        .state
        .create_session("job-attached")
        .expect("attached session");
    let target_cwd = PathBuf::from("/job-target-cwd");
    let attached_cwd = PathBuf::from("/job-attached-cwd");
    inner
        .engine
        .state
        .set_session_working_directory(target, target_cwd.clone())
        .expect("target working directory");
    inner
        .engine
        .state
        .set_session_working_directory(attached, attached_cwd.clone())
        .expect("attached working directory");

    let invoking_client = ClientId(11);
    let attached_client = ClientId(12);
    let invoking_cwd = PathBuf::from("/job-invoking-cwd");
    inner
        .client_entry(invoking_client)
        .working_directory
        .replace(invoking_cwd.clone());
    inner
        .client_entry(attached_client)
        .working_directory
        .replace(PathBuf::from("/job-attached-client-decoy"));
    inner
        .attached
        .entry(attached)
        .or_default()
        .insert(attached_client);

    let target_context =
        ExecutionContext::new(Some(target), Some(target_window), Some(target_pane));
    assert_eq!(
        shell_job_working_directory(
            &inner,
            invoking_client,
            &target_context,
            Some("/job-explicit-cwd")
        ),
        PathBuf::from("/job-explicit-cwd")
    );

    inner.config.startup_source_client_working_directory = Some(PathBuf::from("/job-startup-cwd"));
    assert_eq!(
        shell_job_working_directory(&inner, invoking_client, &target_context, None),
        PathBuf::from("/job-startup-cwd")
    );
    inner.config.startup_source_client_working_directory = None;

    assert_eq!(
        shell_job_working_directory(&inner, invoking_client, &target_context, None),
        invoking_cwd
    );
    assert_eq!(
        shell_job_working_directory(&inner, attached_client, &target_context, None),
        target_cwd
    );

    let missing_target = ExecutionContext::default();
    assert_eq!(
        shell_job_working_directory(&inner, attached_client, &missing_target, None),
        attached_cwd
    );

    let mut clientless_target = target_context.clone();
    clientless_target.set_no_client();
    assert_eq!(
        shell_job_working_directory(&inner, invoking_client, &clientless_target, None),
        PathBuf::from("/job-target-cwd")
    );
    let mut clientless_missing = ExecutionContext::default();
    clientless_missing.set_no_client();
    assert_eq!(
        shell_job_working_directory(&inner, invoking_client, &clientless_missing, None),
        default_job_working_directory()
    );

    let mut replay = target_context;
    replay.set_replay_client(Some(invoking_client));
    assert_eq!(
        shell_job_working_directory(&inner, ClientId(u64::MAX), &replay, None),
        PathBuf::from("/job-invoking-cwd")
    );
}

#[cfg(unix)]
#[test]
fn positive_delay_shell_job_freezes_selected_client_working_directory() {
    let shared = Arc::new(Shared::new(1));
    let directory = tempfile::tempdir().expect("working directory fixture");
    let scheduled_cwd = directory.path().join("scheduled");
    let replacement_cwd = directory.path().join("replacement");
    fs::create_dir(&scheduled_cwd).expect("scheduled cwd");
    fs::create_dir(&replacement_cwd).expect("replacement cwd");
    let output = directory.path().join("output");
    let (client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    shared
        .inner
        .lock()
        .client_entry(client)
        .working_directory
        .replace(scheduled_cwd.clone());
    let command = format!("pwd -P > {}", shell_quote(&output));
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("run-shell", ["-bd", "0.2", &command]),
        )
        .expect("schedule shell job");
    shared
        .inner
        .lock()
        .client_entry(client)
        .working_directory
        .replace(replacement_cwd);

    let deadline = Instant::now() + Duration::from_secs(2);
    assert_eq!(
        wait_for_file_text(&output, deadline),
        format!(
            "{}\n",
            fs::canonicalize(scheduled_cwd).unwrap().to_string_lossy()
        )
    );
}

#[cfg(unix)]
#[test]
fn shell_jobs_receive_the_global_and_session_environment_overlays() {
    let shared = Arc::new(Shared::new(1));
    shared.inner.lock().engine.seed_global_environment([
        ("JOBENV_SEEDED", "daemon"),
        ("JOBENV_HIDDEN", "daemonval"),
        ("JOBENV_UNSET", "daemonval"),
        ("JOBENV_OVERRIDE", "global"),
        ("JOBENV_MASKED", "global"),
    ]);
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-environment", ["-g", "JOBENV_GLOBAL", "global"]),
        )
        .expect("global environment");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-environment",
                ["-g", "-h", "JOBENV_HIDDEN", "newhidden"],
            ),
        )
        .expect("hidden global environment");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-environment", ["-g", "-r", "JOBENV_UNSET"]),
        )
        .expect("child-unset global environment");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                ["printf '%s|%s|%s|%s|%s' \"$JOBENV_GLOBAL\" \"$JOBENV_SEEDED\" \"${JOBENV_HIDDEN-unset}\" \"${JOBENV_UNSET-unset}\" \"${HOME-unset}\""],
            ),
        )
        .expect("sessionless shell job");
    assert_eq!(output.output, "global|daemon|unset|unset|unset");

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "job-overlay", "--", "sleep", "30"],
            ),
        )
        .expect("job target session");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-environment",
                ["-t", "job-overlay", "JOBENV_SESSION", "session"],
            ),
        )
        .expect("session environment");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-environment",
                ["-t", "job-overlay", "JOBENV_OVERRIDE", "session"],
            ),
        )
        .expect("session override");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-environment",
                ["-h", "-t", "job-overlay", "JOBENV_MASKED", "session-hidden"],
            ),
        )
        .expect("session hidden override");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-environment",
                ["-r", "-t", "job-overlay", "JOBENV_GLOBAL"],
            ),
        )
        .expect("session child-unset override");
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                ["printf '%s|%s|%s|%s' \"$JOBENV_SESSION\" \"$JOBENV_OVERRIDE\" \"${JOBENV_MASKED-unset}\" \"${JOBENV_GLOBAL-unset}\""],
            ),
        )
        .expect("session shell job");
    assert_eq!(output.output, "session|session|unset|unset");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if-shell",
                [
                    "test \"$JOBENV_OVERRIDE\" = session && test -z \"${JOBENV_MASKED+x}\" && test -z \"${JOBENV_GLOBAL+x}\"",
                    "display-message -p yes",
                    "display-message -p no",
                ],
            ),
        )
        .expect("session if-shell environment");
    assert_eq!(output.output, "yes");
}

#[cfg(unix)]
#[test]
fn shell_job_term_identity_preserves_startup_and_replaces_post_startup_values() {
    let shared = Arc::new(Shared::new(41));
    shared.inner.lock().engine.seed_global_environment([
        ("TERM", "startup-term"),
        ("TERM_PROGRAM", "startup-program"),
        ("TERM_PROGRAM_VERSION", "startup-version"),
        ("COLORTERM", "startup-colorterm"),
        (
            zz_daemon_client::STARTUP_REENTRY_ENVIRONMENT_VARIABLE,
            "stale-reentry",
        ),
        (
            crate::TMUX_SHIM_EXECUTABLE_ENVIRONMENT_VARIABLE,
            "stale-executable",
        ),
    ]);
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "default-terminal", "job-terminal"]),
        )
        .expect("set job terminal");
    let probe = "printf '%s|%s|%s|%s|%s|%s' \"${TERM-unset}\" \"${TERM_PROGRAM-unset}\" \"${TERM_PROGRAM_VERSION-unset}\" \"${COLORTERM-unset}\" \"${ZZ_STARTUP_REENTRY-unset}\" \"${ZZ_TMUX_EXECUTABLE-unset}\"";

    shared.begin_startup();
    let startup = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("run-shell", [probe]),
        )
        .expect("startup shell job");
    let startup_if = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "if-shell",
                [
                    "test \"$TERM\" = startup-term && test \"$TERM_PROGRAM\" = startup-program && test \"$TERM_PROGRAM_VERSION\" = startup-version && test \"$COLORTERM\" = startup-colorterm && test \"$ZZ_STARTUP_REENTRY\" = 41 && test -z \"${ZZ_TMUX_EXECUTABLE+x}\"",
                    "display-message -p startup-if",
                    "display-message -p wrong",
                ],
            ),
        )
        .expect("startup if-shell job");
    shared.finish_startup();
    assert_eq!(
        startup.output,
        "startup-term|startup-program|startup-version|startup-colorterm|41|unset"
    );
    assert_eq!(startup_if.output, "startup-if");

    let post_startup = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("run-shell", [probe]),
        )
        .expect("post-startup shell job");
    assert_eq!(
        post_startup.output,
        "job-terminal|tmux|3.8-zz|truecolor|unset|unset"
    );
}

#[cfg(unix)]
#[test]
fn positive_delay_shell_job_samples_launch_environment_and_freezes_command_context() {
    let shared = Arc::new(Shared::new(73));
    let scratch = tempfile::tempdir().expect("temporary directory");
    let launch_cwd = scratch.path().join("launch-cwd");
    let launch_output = scratch.path().join("launch-output");
    let (session, window, pane) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("positive-delay")
        .expect("target session");
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let client = ClientId(7);
    shared
        .inner
        .lock()
        .engine
        .seed_global_environment([("TERM", "startup-terminal")]);
    for invocation in [
        CommandInvocation::new(
            "set-environment",
            ["-g", "ZZ_10AF_GLOBAL", "scheduled-global"],
        ),
        CommandInvocation::new(
            "set-environment",
            [
                "-t",
                "=positive-delay",
                "ZZ_10AF_SESSION",
                "scheduled-session",
            ],
        ),
        CommandInvocation::new(
            "set-environment",
            ["-t", "=positive-delay", "ZZ_10AF_HIDDEN", "visible"],
        ),
        CommandInvocation::new(
            "set-environment",
            ["-t", "=positive-delay", "ZZ_10AF_UNSET", "visible"],
        ),
        CommandInvocation::new(
            "set-environment",
            ["-t", "=positive-delay", "TMUX_PANE", "scheduled-pane"],
        ),
        CommandInvocation::new(
            "set-option",
            ["-g", "default-terminal", "scheduled-terminal"],
        ),
        CommandInvocation::new("set-option", ["-g", "@zz_10af_format", "scheduled-format"]),
    ] {
        shared
            .execute(client, ClientKind::Command, &mut context, &invocation)
            .expect("scheduled state");
    }
    shared.begin_startup();

    let worker = Arc::clone(&shared);
    let worker_context = context.clone();
    let launch_cwd_argument = launch_cwd.to_string_lossy().into_owned();
    let launch_probe = "printf '%s|%s|%s|%s|%s|%s|%s|%s|%s' \"$(pwd)\" \"$ZZ_10AF_GLOBAL\" \"$ZZ_10AF_SESSION\" \"${ZZ_10AF_HIDDEN-unset}\" \"${ZZ_10AF_UNSET-unset}\" \"$TMUX_PANE\" '#{@zz_10af_format}' '#{1}' \"$TERM|$TMUX\"";
    let launch_command = format!("{launch_probe} > {}", shell_quote(&launch_output));
    let caller = thread::spawn(move || {
        let mut context = worker_context;
        worker.execute(
            ClientId(8),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                [
                    "-d".to_owned(),
                    "0.5".to_owned(),
                    "-c".to_owned(),
                    launch_cwd_argument,
                    "-t".to_owned(),
                    "=positive-delay:".to_owned(),
                    launch_command,
                    "scheduled-argument".to_owned(),
                ],
            ),
        )
    });

    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.inner.lock().jobs.active_shell_jobs != 1 {
        assert!(Instant::now() < deadline, "delayed job was not scheduled");
        thread::sleep(Duration::from_millis(5));
    }
    fs::create_dir(&launch_cwd).expect("create delayed working directory");
    for invocation in [
        CommandInvocation::new("set-environment", ["-g", "ZZ_10AF_GLOBAL", "launch-global"]),
        CommandInvocation::new(
            "set-environment",
            ["-t", "=positive-delay", "ZZ_10AF_SESSION", "launch-session"],
        ),
        CommandInvocation::new(
            "set-environment",
            ["-h", "-t", "=positive-delay", "ZZ_10AF_HIDDEN", "secret"],
        ),
        CommandInvocation::new(
            "set-environment",
            ["-r", "-t", "=positive-delay", "ZZ_10AF_UNSET"],
        ),
        CommandInvocation::new(
            "set-environment",
            ["-t", "=positive-delay", "TMUX_PANE", "launch-pane"],
        ),
        CommandInvocation::new("set-option", ["-g", "default-terminal", "launch-terminal"]),
        CommandInvocation::new("set-option", ["-g", "@zz_10af_format", "launch-format"]),
    ] {
        shared
            .execute(client, ClientKind::Command, &mut context, &invocation)
            .expect("launch state");
    }
    shared.finish_startup();

    caller
        .join()
        .expect("foreground caller")
        .expect("delayed shell job");
    let expected_tmux = tmux_environment(&shared.socket_path, Some(session));
    assert_eq!(
        fs::read_to_string(&launch_output).expect("launch output"),
        format!(
            "{}|launch-global|launch-session|unset|unset|launch-pane|scheduled-format|scheduled-argument|launch-terminal|{expected_tmux}",
            launch_cwd.to_string_lossy()
        )
    );

    let removed_cwd = scratch.path().join("removed-cwd");
    let fallback_output = scratch.path().join("fallback-output");
    fs::create_dir(&removed_cwd).expect("create removable working directory");
    let worker = Arc::clone(&shared);
    let worker_context = context.clone();
    let removed_cwd_argument = removed_cwd.to_string_lossy().into_owned();
    let fallback_command = format!("pwd > {}", shell_quote(&fallback_output));
    let caller = thread::spawn(move || {
        let mut context = worker_context;
        worker.execute(
            ClientId(9),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                [
                    "-d".to_owned(),
                    "0.5".to_owned(),
                    "-c".to_owned(),
                    removed_cwd_argument,
                    "-t".to_owned(),
                    "=positive-delay:".to_owned(),
                    fallback_command,
                ],
            ),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.inner.lock().jobs.active_shell_jobs != 1 {
        assert!(Instant::now() < deadline, "fallback job was not scheduled");
        thread::sleep(Duration::from_millis(5));
    }
    fs::remove_dir(&removed_cwd).expect("remove delayed working directory");
    caller
        .join()
        .expect("fallback caller")
        .expect("fallback shell job");
    assert_eq!(
        fs::read_to_string(&fallback_output).expect("fallback output"),
        format!("{}\n", fallback_job_working_directory().to_string_lossy())
    );
}

#[cfg(unix)]
#[test]
fn positive_delay_shell_job_retains_destroyed_target_and_keeps_missing_target_sessionless() {
    let shared = Arc::new(Shared::new(74));
    let scratch = tempfile::tempdir().expect("temporary directory");
    let retained_output = scratch.path().join("retained-output");
    let missing_output = scratch.path().join("missing-output");
    let (keep, keep_window, keep_pane, original, original_window, original_pane) = {
        let mut inner = shared.inner.lock();
        let (keep, keep_window, keep_pane) = inner
            .engine
            .state
            .create_session("keep")
            .expect("keep session");
        let (original, original_window, original_pane) = inner
            .engine
            .state
            .create_session("retained-target")
            .expect("original target");
        (
            keep,
            keep_window,
            keep_pane,
            original,
            original_window,
            original_pane,
        )
    };
    let mut target_context =
        ExecutionContext::new(Some(original), Some(original_window), Some(original_pane));
    let mut keep_context = ExecutionContext::new(Some(keep), Some(keep_window), Some(keep_pane));
    let client = ClientId(7);
    for invocation in [
        CommandInvocation::new(
            "set-environment",
            ["-g", "ZZ_10AF_RETAINED_GLOBAL", "scheduled-global"],
        ),
        CommandInvocation::new(
            "set-environment",
            [
                "-t",
                "=retained-target",
                "ZZ_10AF_RETAINED_SESSION",
                "scheduled-session",
            ],
        ),
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut target_context,
                &invocation,
            )
            .expect("scheduled retained state");
    }
    let retained_command = format!(
        "printf '%s|%s|%s' \"$ZZ_10AF_RETAINED_GLOBAL\" \"${{ZZ_10AF_RETAINED_SESSION-unset}}\" \"$TMUX\" > {}",
        shell_quote(&retained_output)
    );
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut target_context,
            &CommandInvocation::new(
                "run-shell",
                ["-bd", "0.5", "-t", "=retained-target:", &retained_command],
            ),
        )
        .expect("schedule retained target job");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut keep_context,
            &CommandInvocation::new(
                "set-environment",
                [
                    "-t",
                    "=retained-target",
                    "ZZ_10AF_RETAINED_SESSION",
                    "before-kill",
                ],
            ),
        )
        .expect("update original target");
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut keep_context,
            &CommandInvocation::new("kill-session", ["-t", "=retained-target"]),
        )
        .expect("destroy original target");
    let (replacement, replacement_window, replacement_pane) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("retained-target")
        .expect("replacement target");
    assert_ne!(replacement, original);
    let mut replacement_context = ExecutionContext::new(
        Some(replacement),
        Some(replacement_window),
        Some(replacement_pane),
    );
    for invocation in [
        CommandInvocation::new(
            "set-environment",
            [
                "-t",
                "=retained-target",
                "ZZ_10AF_RETAINED_SESSION",
                "replacement",
            ],
        ),
        CommandInvocation::new(
            "set-environment",
            ["-g", "ZZ_10AF_RETAINED_GLOBAL", "launch-global"],
        ),
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut replacement_context,
                &invocation,
            )
            .expect("replacement state");
    }
    let expected_retained = format!(
        "launch-global|before-kill|{}",
        tmux_environment(&shared.socket_path, Some(original))
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if fs::read_to_string(&retained_output).is_ok_and(|output| output == expected_retained) {
            break;
        }
        assert!(Instant::now() < deadline, "retained job did not finish");
        thread::sleep(Duration::from_millis(5));
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.inner.lock().jobs.active_shell_jobs != 0 {
        assert!(Instant::now() < deadline, "retained job did not retire");
        thread::sleep(Duration::from_millis(5));
    }

    let missing_command = format!(
        "printf '%s|%s|%s' \"$ZZ_10AF_RETAINED_GLOBAL\" \"${{ZZ_10AF_MISSING_SESSION-unset}}\" \"$TMUX\" > {}",
        shell_quote(&missing_output)
    );
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut replacement_context,
            &CommandInvocation::new(
                "run-shell",
                ["-bd", "0.5", "-t", "=born-later:", &missing_command],
            ),
        )
        .expect("schedule missing target job");
    let (born, born_window, born_pane) = shared
        .inner
        .lock()
        .engine
        .state
        .create_session("born-later")
        .expect("create formerly missing target");
    let mut born_context = ExecutionContext::new(Some(born), Some(born_window), Some(born_pane));
    for invocation in [
        CommandInvocation::new(
            "set-environment",
            [
                "-t",
                "=born-later",
                "ZZ_10AF_MISSING_SESSION",
                "new-session",
            ],
        ),
        CommandInvocation::new(
            "set-environment",
            ["-g", "ZZ_10AF_RETAINED_GLOBAL", "missing-launch-global"],
        ),
    ] {
        shared
            .execute(client, ClientKind::Command, &mut born_context, &invocation)
            .expect("formerly missing target state");
    }
    let expected_missing = format!(
        "missing-launch-global|unset|{}",
        tmux_environment(&shared.socket_path, None)
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if fs::read_to_string(&missing_output).is_ok_and(|output| output == expected_missing) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "missing-target job did not finish"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
#[test]
fn shell_jobs_receive_both_socket_contracts_without_tmux_pane() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();
    let socket = shared.socket_path.display().to_string();
    let pid = std::process::id();

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                ["printf '%s|%s|%s' \"$TMUX\" \"$ZZ_SOCKET\" \"${TMUX_PANE-unset}\""],
            ),
        )
        .expect("sessionless shell job");
    assert_eq!(output.output, format!("{socket},{pid},-1|{socket}|unset"));

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "tmux-job-environment", "--", "sleep", "30"],
            ),
        )
        .expect("job target session");
    let session = context.session.expect("job session");
    let expected = format!("{socket},{pid},{}", session.0);
    for args in [
        vec!["-g", "JOB_TARGET_SCOPE", "global"],
        vec!["-g", "TMUX_PANE", "global-pane"],
        vec!["-t", "tmux-job-environment", "JOB_TARGET_SCOPE", "session"],
        vec!["-t", "tmux-job-environment", "TMUX_PANE", "session-pane"],
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-environment", args),
            )
            .expect("set target environment");
    }
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                ["printf '%s|%s|%s' \"$TMUX\" \"$ZZ_SOCKET\" \"${TMUX_PANE-unset}\""],
            ),
        )
        .expect("session shell job");
    assert_eq!(output.output, format!("{expected}|{socket}|session-pane"));

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if-shell",
                [
                    format!(
                        "test \"$TMUX\" = '{expected}' && test \"$ZZ_SOCKET\" = '{socket}' && test \"$TMUX_PANE\" = session-pane"
                    ),
                    "display-message -p yes".to_owned(),
                    "display-message -p no".to_owned(),
                ],
            ),
        )
        .expect("if-shell environment");
    assert_eq!(output.output, "yes");

    let missing = format!("{socket},{pid},-1");
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                [
                    "-t",
                    "%999999",
                    "printf '%s|%s|%s' \"$JOB_TARGET_SCOPE\" \"$TMUX\" \"$TMUX_PANE\"",
                ],
            ),
        )
        .expect("explicit missing run-shell target");
    assert_eq!(output.output, format!("global|{missing}|global-pane"));

    let missing_condition = format!(
        "test \"$JOB_TARGET_SCOPE\" = global && test \"$TMUX\" = '{missing}' && test \"$TMUX_PANE\" = global-pane"
    );
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if-shell",
                [
                    "-t",
                    "%999999",
                    &missing_condition,
                    "display-message -p missing-global",
                    "display-message -p wrong",
                ],
            ),
        )
        .expect("explicit missing if-shell target");
    assert_eq!(output.output, "missing-global");

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "tmux-job-environment"]),
        )
        .expect("remove job session");
}

#[cfg(unix)]
#[test]
fn shell_jobs_resolve_literal_tmux_through_the_private_zz_shim() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().expect("temporary shim executable");
    let executable = directory.path().join("fake-zz");
    fs::write(
        &executable,
        b"#!/bin/sh\nprintf '%s|%s|%s|%s' \"$1\" \"$2\" \"$ZZ_SOCKET\" \"$PATH\"\n",
    )
    .expect("write fake zz executable");
    let mut permissions = fs::metadata(&executable)
        .expect("fake executable metadata")
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&executable, permissions).expect("make fake zz executable runnable");
    let deadline = Instant::now() + Duration::from_secs(30);
    while let Err(error) = std::process::Command::new(&executable).output() {
        assert!(
            error.kind() == std::io::ErrorKind::ExecutableFileBusy && Instant::now() < deadline,
            "run fake zz executable: {error}"
        );
        thread::sleep(Duration::from_millis(10));
    }

    let shared = Arc::new(Shared::new(1));
    shared
        .inner
        .lock()
        .engine
        .seed_global_environment([("PATH", "/modeled/bin:/modeled/sbin")]);
    let shim = TmuxShimGuard::install(executable, false).expect("install tmux executable path");
    let expected_directory = shim.directory.clone();
    let expected_path = std::env::join_paths([
        shim.directory.clone(),
        PathBuf::from("/modeled/bin"),
        PathBuf::from("/modeled/sbin"),
    ])
    .expect("join modeled path")
    .to_string_lossy()
    .into_owned();
    *shared.tmux_shim.lock() = Some(shim);
    let socket = shared.socket_path.display().to_string();
    let output = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("run-shell", ["tmux one 'two three'"]),
        )
        .expect("run literal tmux through shim");
    assert_eq!(
        output.output,
        format!("one|two three|{socket}|{expected_path}")
    );

    let mut context = ExecutionContext::default();
    for invocation in [
        CommandInvocation::new("new-session", ["-d", "-s", "pane-shim", "/bin/sh"]),
        CommandInvocation::new("respawn-pane", ["-k", "/bin/sh"]),
    ] {
        shared
            .execute(ClientId(7), ClientKind::Command, &mut context, &invocation)
            .expect("spawn pane with private tmux shim");
        let pane = context.pane.expect("spawned pane");
        let inner = shared.inner.lock();
        let environment = &inner.panes.terminal_spawns[&pane].env;
        let value = |name: &str| {
            environment
                .iter()
                .rev()
                .find_map(|(candidate, value)| (candidate == name).then_some(value.as_deref()))
                .flatten()
        };
        assert_eq!(value("PATH"), Some(OsStr::new(&expected_path)));
        assert_eq!(
            value(crate::TMUX_SHIM_DIRECTORY_ENVIRONMENT_VARIABLE),
            Some(expected_directory.as_os_str())
        );
        assert_eq!(
            value(crate::TMUX_SHIM_EXECUTABLE_ENVIRONMENT_VARIABLE),
            Some(directory.path().join("fake-zz").as_os_str())
        );
    }
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "pane-shim"]),
        )
        .expect("remove shim pane");
}

#[cfg(unix)]
#[test]
fn tmux_shim_refuses_bare_nested_invocation_before_running_zz() {
    let shim =
        TmuxShimGuard::install(PathBuf::from("/bin/echo"), false).expect("install tmux wrapper");
    let output = std::process::Command::new(shim.directory.join("tmux"))
        .env("TMUX", "/tmp/zzprobe-nested.sock,1,0")
        .env(
            crate::TMUX_SHIM_EXECUTABLE_ENVIRONMENT_VARIABLE,
            &shim.executable,
        )
        .output()
        .expect("run bare nested wrapper");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"sessions should be nested with care, unset $TMUX to force\n"
    );
}

#[cfg(unix)]
#[test]
fn the_shim_folder_runs_the_daemons_own_zz() {
    let shim = TmuxShimGuard::install(PathBuf::from("/bin/echo"), false).expect("install shim");
    let output = std::process::Command::new(shim.directory.join("zz"))
        .arg("tools")
        .env(
            crate::TMUX_SHIM_EXECUTABLE_ENVIRONMENT_VARIABLE,
            &shim.executable,
        )
        .output()
        .expect("run zz wrapper");
    assert_eq!(output.stdout, b"tools\n");
    let directory = shim.directory.clone();
    drop(shim);
    assert!(!directory.exists());
}

#[cfg(unix)]
#[test]
fn shim_folders_of_dead_daemons_are_removed_and_live_ones_kept() {
    let root = tempfile::tempdir().expect("temporary directory");
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let dead = child.id();
    child.wait().expect("wait for true");
    let own = std::process::id();
    let names = [
        format!("zz-tmux-shim-{dead}-1"),
        format!("zz-tmux-shim-{own}-1"),
        "zz-tmux-shim-notes".to_owned(),
    ];
    for name in &names {
        fs::create_dir(root.path().join(name)).expect("create shim folder");
        fs::write(root.path().join(name).join("zz"), b"").expect("write shim file");
    }
    remove_dead_tmux_shims(root.path());
    assert!(!root.path().join(&names[0]).exists());
    assert!(root.path().join(&names[1]).exists());
    assert!(root.path().join(&names[2]).exists());
}
