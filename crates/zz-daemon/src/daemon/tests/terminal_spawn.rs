use super::*;

#[cfg(unix)]
#[test]
fn checkshell_rejects_relative_non_executable_and_own_binary_paths() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().expect("temporary directory");
    assert!(!shell_is_valid_for_program(
        Path::new("sh"),
        OsStr::new("zz")
    ));
    assert!(shell_is_valid_for_program(
        directory.path(),
        OsStr::new("zz")
    ));

    let non_executable = directory.path().join("not-executable");
    fs::write(&non_executable, []).expect("write non-executable shell");
    fs::set_permissions(&non_executable, fs::Permissions::from_mode(0o600))
        .expect("set non-executable permissions");
    assert!(!shell_is_valid_for_program(
        &non_executable,
        OsStr::new("zz")
    ));

    let own_binary = directory.path().join("zz-shell-test");
    fs::write(&own_binary, []).expect("write own binary");
    fs::set_permissions(&own_binary, fs::Permissions::from_mode(0o700))
        .expect("set own binary permissions");
    assert!(!shell_is_valid_for_program(
        &own_binary,
        OsStr::new("-zz-shell-test")
    ));

    let valid = directory.path().join("valid-shell");
    fs::write(&valid, []).expect("write valid shell");
    fs::set_permissions(&valid, fs::Permissions::from_mode(0o700))
        .expect("set valid shell permissions");
    assert!(shell_is_valid_for_program(&valid, OsStr::new("zz")));
}

#[cfg(unix)]
#[test]
fn default_shell_readback_is_the_resolved_runtime_value() {
    let shared = Arc::new(Shared::new(1));
    let mut inner = shared.inner.lock();
    let readback = inner
        .engine
        .execute(
            &mut ExecutionContext::default(),
            &CommandInvocation::new("show-options", ["-gv", "default-shell"]),
        )
        .expect("default-shell readback")
        .output;
    assert!(readback.starts_with('/'));
    assert!(shell_is_valid(Path::new(readback.as_str())));
    assert_eq!(readback, inner.engine.global_default_shell());
}

#[cfg(unix)]
#[test]
fn default_shell_rejects_invalid_values_and_unset_controls_the_child_shell() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    let initial = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-options", ["-gv", "default-shell"]),
        )
        .expect("read initial default-shell")
        .output;
    let current_exe = std::env::current_exe()
        .expect("current test executable")
        .to_string_lossy()
        .into_owned();
    for invalid in [
        "/nonexistent/sh",
        "relative/sh",
        "/etc/passwd",
        "",
        current_exe.as_str(),
    ] {
        let error = shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-option", ["-g", "default-shell", invalid]),
            )
            .expect_err("invalid default-shell accepted");
        assert!(matches!(
            error,
            DaemonError::Server(ServerError::InvalidCommand(message))
                if message == format!("not a suitable shell: {invalid}")
        ));
        assert_eq!(
            shared
                .execute(
                    ClientId(7),
                    ClientKind::Command,
                    &mut context,
                    &CommandInvocation::new("show-options", ["-gv", "default-shell"]),
                )
                .expect("read unchanged default-shell")
                .output,
            initial
        );
    }

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "default-shell", "/bin/sh"]),
        )
        .expect("set valid default-shell");
    let error = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-ga", "default-shell", "-invalid"]),
        )
        .expect_err("invalid appended default-shell accepted");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "not a suitable shell: /bin/sh-invalid"
    ));
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-gu", "default-shell"]),
        )
        .expect("unset global default-shell");
    assert_eq!(
        shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-options", ["-gv", "default-shell"]),
            )
            .expect("read unset default-shell")
            .output,
        "/bin/sh"
    );

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-window-option", ["-g", "remain-on-exit", "on"]),
        )
        .expect("enable retained exits");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "shell-after-unset"]),
        )
        .expect("create default-shell pane");
    let pane = context.pane.expect("default-shell pane");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !matches!(terminal.latest_viewport().status, SessionStatus::Running) {
        assert!(
            Instant::now() < deadline,
            "default-shell pane did not start"
        );
        thread::sleep(Duration::from_millis(10));
    }
    terminal.send_text("printf 'ZZ_DEFAULT_SHELL=%s\\n' \"$SHELL\"; exit\n");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if shared
            .inner
            .lock()
            .engine
            .state
            .pane(pane)
            .is_some_and(|pane| pane.dead && pane.dead_status == Some(0))
        {
            break;
        }
        assert!(Instant::now() < deadline, "default-shell pane did not exit");
        thread::sleep(Duration::from_millis(10));
    }
    let capture = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("capture-pane", ["-p", "-t", &pane.to_string()]),
        )
        .expect("capture default-shell pane")
        .output;
    assert!(capture.contains("ZZ_DEFAULT_SHELL=/bin/sh"), "{capture:?}");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "shell-after-unset"]),
        )
        .expect("remove default-shell fixture");
}

#[cfg(unix)]
#[test]
fn detached_panes_spawn_at_their_laid_out_size() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "sized", "-x", "100", "-y", "30"],
            ),
        )
        .expect("create a 100x30 session");
    let pane = context.pane.expect("sized pane");
    assert_eq!(
        shared.inner.lock().panes.terminal_spawns[&pane].initial_size,
        Some(TerminalSize::cells(100, 30))
    );
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "sized"]),
        )
        .expect("remove sized session");
}

#[cfg(unix)]
#[test]
fn default_command_is_injected_only_when_nonempty_at_create() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "default-command", "sleep 30"]),
        )
        .expect("set default-command");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "with-default-command"]),
        )
        .expect("create session with default-command");
    let pane = context.pane.expect("default-command pane");
    assert_eq!(
        shared.inner.lock().panes.terminal_spawns[&pane]
            .command
            .as_deref(),
        Some(["sleep 30".to_owned()].as_slice())
    );
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "with-default-command"]),
        )
        .expect("remove default-command session");

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-gu", "default-command"]),
        )
        .expect("unset default-command");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "without-default-command"]),
        )
        .expect("create session without default-command");
    let pane = context.pane.expect("integrated-shell pane");
    assert!(
        shared.inner.lock().panes.terminal_spawns[&pane]
            .command
            .is_none()
    );
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "without-default-command"]),
        )
        .expect("remove integrated-shell session");
}

#[cfg(unix)]
#[test]
fn respawn_with_new_argv_reuses_the_creation_shell() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "default-shell", "/bin/sh"]),
        )
        .expect("set creation shell");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "respawn-shell", "--", "sleep", "30"],
            ),
        )
        .expect("create respawn pane");
    let pane = context.pane.expect("respawn pane");
    assert_eq!(
        shared.inner.lock().panes.terminal_spawns[&pane]
            .shell
            .as_deref(),
        Some("/bin/sh")
    );
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "default-shell", "/usr/bin/false"]),
        )
        .expect("change current default-shell");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "respawn-pane",
                ["-k", "-t", &pane.to_string(), "--", "sleep", "29"],
            ),
        )
        .expect("respawn with new argv");
    let inner = shared.inner.lock();
    assert_eq!(
        inner.panes.terminal_spawns[&pane].command.as_deref(),
        Some(["sleep".to_owned(), "29".to_owned()].as_slice())
    );
    assert_eq!(
        inner.panes.terminal_spawns[&pane].shell.as_deref(),
        Some("/bin/sh")
    );
    drop(inner);
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("kill-session", ["-t", "respawn-shell"]),
        )
        .expect("remove respawn fixture");
}

#[test]
fn same_session_join_delivers_destination_wrap_search_to_the_worker() {
    let shared = Arc::new(Shared::new(53));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(3),
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "work", "-n", "source"]),
        )
        .expect("source window");
    let source = context.pane.expect("source pane");
    let source_window = context.window.expect("source window");
    shared
        .execute(
            ClientId(3),
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                ["-t", &source_window.to_string(), "wrap-search", "off"],
            ),
        )
        .expect("disable source wrapping");
    shared
        .execute(
            ClientId(3),
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-window", ["-n", "destination"]),
        )
        .expect("destination window");
    let target = context.pane.expect("target pane");
    shared.delivered_wrap_search_commands.lock().clear();

    shared
        .execute(
            ClientId(3),
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "join-pane",
                ["-d", "-s", &source.to_string(), "-t", &target.to_string()],
            ),
        )
        .expect("join into destination window");

    assert!(
        shared
            .inner
            .lock()
            .engine
            .terminal_worker_options_for_pane(source)
            .expect("joined pane options")
            .wrap_search
    );
    assert_eq!(*shared.delivered_wrap_search_commands.lock(), [true]);
}

#[test]
fn terminal_environment_scrubs_before_explicit_values_and_preserves_claude_settings() {
    let mut engine = MuxEngine::default();
    let settings = [
        ("CLAUDE_CODE_EXECUTABLE", "/opt/claude"),
        ("CLAUDE_CONFIG_DIR", "/tmp/claude-config"),
        ("CLAUDE_CODE_DISABLE_TERMINAL_TITLE", "1"),
        ("ANTHROPIC_API_KEY", "test-key"),
        ("ANTHROPIC_BASE_URL", "https://example.invalid"),
    ];
    engine.seed_global_environment(settings);
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "claude-settings"]),
        )
        .expect("session");
    for args in [
        vec!["-g", "CLAUDE_CODE_CHILD_SESSION", "global"],
        vec!["CLAUDE_CODE_SESSION_ID", "session"],
    ] {
        engine
            .execute(
                &mut context,
                &CommandInvocation::new("set-environment", args),
            )
            .expect("explicit Claude session environment");
    }
    let environment = terminal_environment_for_session(&engine, context.session.unwrap())
        .expect("terminal environment");
    let removals = crate::PARENT_CLAUDE_SESSION_ENVIRONMENT
        .iter()
        .map(|name| (OsString::from(*name), None))
        .collect::<Vec<_>>();
    assert_eq!(&environment[..removals.len()], removals.as_slice());
    let overlay = &environment[removals.len()..];
    for (name, value) in settings.into_iter().chain([
        ("CLAUDE_CODE_CHILD_SESSION", "global"),
        ("CLAUDE_CODE_SESSION_ID", "session"),
    ]) {
        assert!(overlay.contains(&(name.into(), Some(value.into()))));
        assert!(!overlay.contains(&(name.into(), None)));
    }
    for (name, _) in settings {
        assert!(!environment.contains(&(name.into(), None)));
    }
}

#[cfg(unix)]
#[test]
fn seeded_global_environment_and_session_markers_reach_terminal_spawn() {
    let shared = Arc::new(Shared::new(1));
    shared.inner.lock().engine.seed_global_environment([
        ("PHASE4D_SEEDED", "daemon"),
        ("HIDDENPROBE", "daemonval"),
        ("TERM", "inherited-term"),
    ]);
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-environment", ["-g", "-h", "HIDDENPROBE", "newhidden"]),
        )
        .expect("hidden global environment");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "seeded-environment"]),
        )
        .expect("session");
    let pane = context.pane.expect("terminal pane");
    let session = context.session.expect("terminal session");
    let expected_tmux = tmux_environment(&shared.socket_path, Some(session));
    assert_eq!(
        shared.inner.lock().panes.terminal_spawns[&pane].terminal_type,
        Some("tmux-256color".to_owned())
    );
    {
        let inner = shared.inner.lock();
        let environment = &inner.panes.terminal_spawns[&pane].env;
        assert!(environment.contains(&("TMUX".into(), Some(expected_tmux.clone().into()))));
        assert!(environment.contains(&("TMUX_PANE".into(), Some(pane.to_string().into()))));
    }
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-buffer",
                [
                    "printf 'PHASE4D_SEEDED=[%s] DISPLAY=[%s] HP=[%s] TERM=[%s]\\n' \"$PHASE4D_SEEDED\" \"${DISPLAY-unset}\" \"${HIDDENPROBE-}\" \"$TERM\"; printf 'TMUX=[%s]\\nTMUX_PANE=[%s]\\n' \"$TMUX\" \"$TMUX_PANE\"\n",
                ],
            ),
        )
        .expect("shell probe");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("paste-buffer", ["-t", &pane.to_string()]),
        )
        .expect("paste shell probe");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let captured = shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("capture-pane", ["-p", "-t", &pane.to_string()]),
            )
            .expect("capture terminal")
            .output;
        if captured.contains("PHASE4D_SEEDED=[daemon] DISPLAY=[unset] HP=[] TERM=[tmux-256color]")
            && captured.contains(&format!("TMUX=[{expected_tmux}]"))
            && captured.contains(&format!("TMUX_PANE=[{pane}]"))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "seeded environment did not reach terminal spawn"
        );
        thread::sleep(Duration::from_millis(10));
    }
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("respawn-pane", ["-k", "-t", &pane.to_string()]),
        )
        .expect("respawn terminal pane");
    let inner = shared.inner.lock();
    let environment = &inner.panes.terminal_spawns[&pane].env;
    assert!(environment.contains(&("TMUX".into(), Some(expected_tmux.into()))));
    assert!(environment.contains(&("TMUX_PANE".into(), Some(pane.to_string().into()))));
}

#[cfg(unix)]
#[test]
fn new_session_creation_environment_reaches_the_first_terminal() {
    let shared = Arc::new(Shared::new(1));
    shared
        .inner
        .lock()
        .engine
        .seed_global_environment([("DISPLAY", "client")]);
    let mut context = ExecutionContext::default();
    let spawned_value = |pane: PaneId, name: &str| {
        shared.inner.lock().panes.terminal_spawns[&pane]
            .env
            .iter()
            .rev()
            .find_map(|(candidate, value)| (candidate == name).then(|| value.clone()))
            .flatten()
    };

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d",
                    "-s",
                    "creation-environment",
                    "-e",
                    "DISPLAY=first",
                    "-e",
                    "MALFORMED",
                    "-e",
                    "=ignored",
                    "-e",
                    "DISPLAY=last",
                    "--",
                    "sleep",
                    "30",
                ],
            ),
        )
        .expect("session environment");
    let pane = context.pane.expect("first pane");
    assert_eq!(
        spawned_value(pane, "DISPLAY").as_deref(),
        Some(OsStr::new("last"))
    );
    assert!(
        !shared.inner.lock().panes.terminal_spawns[&pane]
            .env
            .iter()
            .any(|(name, _)| name.is_empty())
    );
    assert_eq!(
        shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "show-environment",
                    ["-t", "creation-environment", "DISPLAY"],
                ),
            )
            .unwrap()
            .output,
        "DISPLAY=last"
    );
    assert_eq!(
        shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-environment", ["-t", "creation-environment", ""],),
            )
            .unwrap()
            .output,
        "=ignored"
    );

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-E", "-s", "creation-unseeded", "--", "sleep", "30"],
            ),
        )
        .expect("unseeded session");
    let pane = context.pane.expect("unseeded first pane");
    assert_eq!(
        spawned_value(pane, "DISPLAY").as_deref(),
        Some(OsStr::new("client"))
    );
    assert!(
        shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-environment", ["-t", "creation-unseeded"],),
            )
            .unwrap()
            .output
            .is_empty()
    );

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d",
                    "-E",
                    "-s",
                    "creation-explicit",
                    "-e",
                    "DISPLAY=explicit",
                    "--",
                    "sleep",
                    "30",
                ],
            ),
        )
        .expect("explicit unseeded session");
    let pane = context.pane.expect("explicit first pane");
    assert_eq!(
        spawned_value(pane, "DISPLAY").as_deref(),
        Some(OsStr::new("explicit"))
    );
}

#[cfg(unix)]
#[test]
fn new_and_split_window_apply_local_environment_and_spawn_empty_panes() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "pane-spawn-options", "--", "sleep", "30"],
            ),
        )
        .expect("session");
    let source = context.pane.expect("source pane");

    let window = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-window",
                [
                    "-d",
                    "-t",
                    "pane-spawn-options",
                    "-e",
                    "PANE_ONLY=first",
                    "-e",
                    "MALFORMED",
                    "-e",
                    "=ignored",
                    "-e",
                    "PANE_ONLY=last",
                    "-e",
                    "TMUX=overlay",
                    "-e",
                    "TMUX_PANE=bogus",
                    "-e",
                    "PWD=/bogus",
                    "--",
                    "sleep",
                    "30",
                ],
            ),
        )
        .expect("environment window");
    let window_pane = window
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .expect("window pane");
    {
        let inner = shared.inner.lock();
        let spawn = &inner.panes.terminal_spawns[&window_pane];
        let expected_pane = window_pane.to_string();
        let value = |name: &str| {
            spawn
                .env
                .iter()
                .rev()
                .find_map(|(candidate, value)| (candidate == name).then_some(value.as_deref()))
                .flatten()
        };
        assert_eq!(value("PANE_ONLY"), Some(OsStr::new("last")));
        assert_eq!(value("TMUX"), Some(OsStr::new("overlay")));
        assert_eq!(value("TMUX_PANE"), Some(OsStr::new(expected_pane.as_str())));
        assert_eq!(
            value("PWD"),
            spawn.working_directory.as_deref().map(Path::as_os_str)
        );
        assert!(!spawn.env.iter().any(|(name, _)| name.is_empty()));
        assert!(!spawn.env.iter().any(|(name, _)| name == "MALFORMED"));
    }

    let split = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "split-window",
                [
                    "-d",
                    "-t",
                    &source.to_string(),
                    "-e",
                    "SPLIT_ONLY=first",
                    "-e",
                    "SPLIT_ONLY=last",
                    "-e",
                    "CLAUDE_CODE_CHILD_SESSION=1",
                    "--",
                    "sleep",
                    "30",
                ],
            ),
        )
        .expect("environment split");
    let split_pane = split
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .expect("split pane");
    {
        let inner = shared.inner.lock();
        let environment = &inner.panes.terminal_spawns[&split_pane].env;
        let scrub = environment
            .iter()
            .position(|entry| entry == &("CLAUDE_CODE_CHILD_SESSION".into(), None))
            .expect("inherited Claude session removal");
        let explicit = environment
            .iter()
            .rposition(|(name, _)| name == "CLAUDE_CODE_CHILD_SESSION")
            .expect("explicit Claude session value");
        assert!(scrub < explicit);
        assert_eq!(environment[explicit].1.as_deref(), Some(OsStr::new("1")));
        assert_eq!(
            environment
                .iter()
                .rev()
                .find_map(|(name, value)| { (name == "SPLIT_ONLY").then_some(value.as_deref()) }),
            Some(Some(OsStr::new("last")))
        );
    }
    assert!(matches!(
        shared.execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "show-environment",
                ["-t", "pane-spawn-options", "PANE_ONLY"],
            ),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "unknown variable: PANE_ONLY"
    ));

    let source_target = source.to_string();
    for (name, arguments, keeps_empty_argument) in [
        (
            "new-window",
            vec!["-d", "-t", "pane-spawn-options", "-E"],
            false,
        ),
        (
            "new-window",
            vec!["-d", "-t", "pane-spawn-options", "-E", ""],
            true,
        ),
        (
            "split-window",
            vec!["-d", "-t", source_target.as_str(), "-E"],
            false,
        ),
    ] {
        let execution = shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(name, arguments),
            )
            .expect("empty pane");
        let pane = execution
            .effects
            .iter()
            .find_map(|effect| match effect {
                MuxEffect::PaneCreated { pane, .. } => Some(*pane),
                _ => None,
            })
            .expect("empty pane id");
        let inner = shared.inner.lock();
        assert_eq!(inner.terminals[&pane].process_id(), None);
        assert!(!inner.engine.state.pane(pane).unwrap().dead);
        assert_eq!(
            inner.panes.terminal_spawns[&pane]
                .command
                .as_deref()
                .is_some_and(|command| command == [""]),
            keeps_empty_argument
        );
    }
}

#[test]
fn history_limit_is_captured_when_each_terminal_actor_is_created() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "history-limit", "7"]),
        )
        .expect("global history limit");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "history"]),
        )
        .expect("session");
    let session = context.session.expect("session id");
    let first = context.pane.expect("first pane");
    assert_eq!(shared.inner.lock().terminals[&first].max_scrollback(), 7);

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-t", &session.to_string(), "history-limit", "2"],
            ),
        )
        .expect("session history limit");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("split-window", ["-h"]),
        )
        .expect("second terminal");
    let second = context.pane.expect("second pane");
    let inner = shared.inner.lock();
    assert_eq!(inner.terminals[&first].max_scrollback(), 7);
    assert_eq!(inner.terminals[&second].max_scrollback(), 2);
}

#[test]
fn configured_split_window_binding_creates_a_terminal_without_rewriting_the_command() {
    let shared = Arc::new(Shared::new(1));
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let mut context = ExecutionContext::default();
    let config = zz_mux::parse_config(
        "picker.conf",
        r##"set -g prefix C-a
bind | split-window -h -c "#{pane_current_path}"
bind - split-window -v -c "#{pane_current_path}"
"##,
    );
    assert!(config.diagnostics.is_empty());
    for command in &config.commands {
        shared
            .execute(client, ClientKind::Interactive, &mut context, command)
            .expect("tmux config command");
    }
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "picker"]),
        )
        .expect("session");
    let source = context.pane.expect("source terminal");
    let window = context.window.expect("window");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.keys.prefix(), "C-a");
        assert_eq!(
            inner.engine.keys.get("prefix", "|").unwrap().commands[0].name,
            "split-window"
        );
        assert_eq!(
            inner.engine.keys.get("prefix", "-").unwrap().commands[0].name,
            "split-window"
        );
    }

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: source,
                input: test_key(
                    KeyCode::Character('a'),
                    Modifiers::new(false, true, false, false),
                    None,
                ),
                text_follows: false,
            },
        )
        .expect("configured prefix");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: source,
                input: test_key(KeyCode::Character('|'), Modifiers::default(), Some("|")),
                text_follows: true,
            },
        )
        .expect("configured horizontal split binding");
    let terminal = context.pane.expect("terminal id");
    {
        let inner = shared.inner.lock();
        assert!(inner.terminals.contains_key(&source));
        assert!(inner.terminals.contains_key(&terminal));
        assert!(matches!(
            inner.engine.state.pane(terminal).map(|pane| &pane.kind),
            Some(PaneKind::Terminal)
        ));
        assert!(matches!(
            inner.engine.state.windows[&window].layout.project(),
            LayoutNode::Split {
                axis: zz_protocol::Axis::Horizontal,
                ..
            }
        ));
    }

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: terminal,
                input: test_key(
                    KeyCode::Character('a'),
                    Modifiers::new(false, true, false, false),
                    None,
                ),
                text_follows: false,
            },
        )
        .expect("configured prefix again");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: terminal,
                input: test_key(KeyCode::Character('-'), Modifiers::default(), Some("-")),
                text_follows: true,
            },
        )
        .expect("configured vertical split binding");
    let vertical = context.pane.expect("vertical terminal id");
    {
        let inner = shared.inner.lock();
        assert!(inner.terminals.contains_key(&vertical));
        assert!(matches!(
            inner.engine.state.pane(vertical).map(|pane| &pane.kind),
            Some(PaneKind::Terminal)
        ));
    }
}

#[test]
fn default_percent_binding_splits_like_the_pin_and_picker_kind_stays_bindable() {
    let shared = Arc::new(Shared::new(1));
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, OutboundMailbox::new());
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "picker"]),
        )
        .expect("session");
    let source = context.pane.expect("source terminal");
    let window = context.window.expect("window");
    {
        let inner = shared.inner.lock();
        let binding = inner.engine.keys.get("prefix", "%").unwrap();
        assert_eq!(binding.commands[0].name, "split-window");
        assert_eq!(binding.commands[0].args, ["-h"]);
    }

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: source,
                input: test_key(
                    KeyCode::Character('b'),
                    Modifiers::new(false, true, false, false),
                    None,
                ),
                text_follows: false,
            },
        )
        .expect("default prefix");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: source,
                input: test_key(KeyCode::Character('%'), Modifiers::default(), Some("%")),
                text_follows: true,
            },
        )
        .expect("default horizontal split binding");
    let split = context.pane.expect("split pane id");
    {
        let inner = shared.inner.lock();
        assert!(inner.terminals.contains_key(&source));
        assert!(inner.terminals.contains_key(&split));
        assert!(matches!(
            inner.engine.state.windows[&window].layout.project(),
            LayoutNode::Split {
                axis: zz_protocol::Axis::Horizontal,
                ..
            }
        ));
    }

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "bind-key",
                [
                    "-T",
                    "prefix",
                    "%",
                    "split-window",
                    "--kind",
                    "picker",
                    "-h",
                ],
            ),
        )
        .expect("rebind the stock chord");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: split,
                input: test_key(
                    KeyCode::Character('b'),
                    Modifiers::new(false, true, false, false),
                    None,
                ),
                text_follows: false,
            },
        )
        .expect("default prefix");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Key {
                pane: split,
                input: test_key(KeyCode::Character('%'), Modifiers::default(), Some("%")),
                text_follows: true,
            },
        )
        .expect("rebound horizontal split binding");
    let picker = context.pane.expect("picker id");
    let inner = shared.inner.lock();
    assert!(!inner.terminals.contains_key(&picker));
    assert!(matches!(
        inner.engine.state.pane(picker).map(|pane| &pane.kind),
        Some(PaneKind::Picker { .. })
    ));
}

#[cfg(unix)]
#[test]
fn terminal_cwd_flag_prefers_a_valid_literal_and_bogus_paths_land_in_home() {
    let donor_directory = tempfile::tempdir().expect("donor working directory");
    let donor_physical = donor_directory.path().join("physical");
    fs::create_dir(&donor_physical).expect("physical donor working directory");
    let donor_literal = donor_directory.path().join("literal");
    std::os::unix::fs::symlink(&donor_physical, &donor_literal)
        .expect("literal donor working directory");
    let donor_path = donor_physical
        .canonicalize()
        .expect("canonical donor working directory");
    let literal_directory = tempfile::tempdir().expect("literal working directory");
    let literal_path = literal_directory
        .path()
        .canonicalize()
        .expect("canonical literal working directory");
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d",
                    "-s",
                    "cwd",
                    "-c",
                    donor_literal.to_string_lossy().as_ref(),
                ],
            ),
        )
        .expect("session with literal cwd");
    let donor = context.pane.expect("donor pane");
    let donor_terminal = Arc::clone(&shared.inner.lock().terminals[&donor]);
    let wait_for_cwd = |terminal: &TerminalSession, expected: &Path| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if terminal_working_directory(terminal).as_deref() == Some(expected) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for terminal cwd; expected={expected:?}, pid={:?}, cwd={:?}",
                terminal.foreground_process_id(),
                terminal_working_directory(terminal),
            );
            thread::sleep(Duration::from_millis(10));
        }
    };
    wait_for_cwd(&donor_terminal, &donor_path);
    assert_eq!(
        shared.inner.lock().panes.terminal_spawns[&donor].working_directory,
        Some(donor_literal.clone())
    );
    assert_eq!(
        shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("display-message", ["-p", "#{pane_start_path}"],),
            )
            .expect("literal pane start path")
            .output
            .to_string(),
        donor_literal.to_string_lossy()
    );
    donor_terminal.send_text("printf 'ZZ_LITERAL_PWD=[%s]\\n' \"$PWD\"\n");
    let expected_pwd = format!("ZZ_LITERAL_PWD=[{}]", donor_literal.display());
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let captured = shared
            .execute(
                ClientId(7),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("capture-pane", ["-pJ", "-t", &donor.to_string()]),
            )
            .expect("capture literal PWD")
            .output;
        if captured.contains(&expected_pwd) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "literal PWD did not reach the child; expected={expected_pwd:?}, captured={captured:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "split-window",
                ["-h", "-c", literal_path.to_string_lossy().as_ref()],
            ),
        )
        .expect("split with valid literal cwd");
    let literal = context.pane.expect("literal cwd pane");
    let literal_terminal = Arc::clone(&shared.inner.lock().terminals[&literal]);
    wait_for_cwd(&literal_terminal, &literal_path);

    let bogus_path = literal_directory.path().join("missing");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "split-window",
                ["-v", "-c", bogus_path.to_string_lossy().as_ref()],
            ),
        )
        .expect("split with bogus literal cwd");
    let fallback = context.pane.expect("fallback cwd pane");
    let fallback_terminal = Arc::clone(&shared.inner.lock().terminals[&fallback]);
    let home_path = std::env::var_os("HOME")
        .map(PathBuf::from)
        .expect("HOME is set")
        .canonicalize()
        .expect("canonical home directory");
    wait_for_cwd(&fallback_terminal, &home_path);
}

#[cfg(unix)]
#[test]
fn terminal_and_editor_panes_inherit_live_working_directory() {
    let directory = tempfile::tempdir().expect("temporary working directory");
    let physical = directory.path().join("physical");
    fs::create_dir(&physical).expect("physical working directory");
    let reported = directory.path().join("reported");
    std::os::unix::fs::symlink(&physical, &reported).expect("working directory symlink");
    let expected = physical
        .canonicalize()
        .expect("canonical working directory");
    let shared = Arc::new(Shared::new(1));
    shared.start_timers().expect("start timers");
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "cwd"]),
        )
        .expect("session");
    let first = context.pane.expect("first pane");
    let source = Arc::clone(&shared.inner.lock().terminals[&first]);
    let reported_payload = format!("file://workstation{}", reported.to_string_lossy());
    source.send_text(format!(
        "cd '{}'\nprintf '\\033]7;{}\\a'\n",
        reported.display(),
        reported_payload
    ));

    let wait_for_cwd = |terminal: &TerminalSession| {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if terminal_working_directory(terminal).as_deref() == Some(expected.as_path()) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for terminal cwd; pid={:?}, cwd={:?}",
                terminal.foreground_process_id(),
                terminal_working_directory(terminal),
            );
            thread::sleep(Duration::from_millis(10));
        }
    };
    wait_for_cwd(&source);

    let expected_path = expected.to_string_lossy().into_owned();
    let reported_path = reported.to_string_lossy().into_owned();
    let deadline = Instant::now() + Duration::from_secs(5);
    let settled_report = loop {
        let facts = shared
            .inner
            .lock()
            .engine
            .pane_runtime_facts(first)
            .cloned()
            .unwrap_or_default();
        if facts.current_path == expected_path
            && facts.reported_path.starts_with("file://")
            && facts.reported_path.ends_with(&reported_path)
        {
            break facts.reported_path;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for pane path facts; facts={facts:?}",
        );
        thread::sleep(Duration::from_millis(10));
    };
    let first_target = first.to_string();
    let paths = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "display-message",
                [
                    "-p",
                    "-t",
                    first_target.as_str(),
                    "#{pane_current_path}|#{pane_path}",
                ],
            ),
        )
        .expect("live pane paths");
    assert_eq!(paths.output, format!("{expected_path}|{settled_report}"));

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("split-window", ["-h"]),
        )
        .expect("split terminal");
    let second = context.pane.expect("second pane");
    let inherited = Arc::clone(&shared.inner.lock().terminals[&second]);
    wait_for_cwd(&inherited);

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("split-window", ["--kind", "picker", "-v"]),
        )
        .expect("editor picker");
    let editor = context.pane.expect("editor picker pane");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "experimental-editor-pane", "on"]),
        )
        .expect("enable editor panes");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("select-pane-kind", ["-t", &editor.to_string(), "editor"]),
        )
        .expect("editor materialization");
    let restore_path = expected.join("restored.rs");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-editor-path",
                [
                    "-t",
                    &editor.to_string(),
                    restore_path.to_string_lossy().as_ref(),
                ],
            ),
        )
        .expect("editor restore metadata");
    let inner = shared.inner.lock();
    assert!(matches!(
        inner.engine.state.pane(editor).map(|pane| &pane.kind),
        Some(PaneKind::Editor(descriptor))
            if descriptor.cwd == expected.to_string_lossy()
                && descriptor.path.as_deref()
                    == Some(restore_path.to_string_lossy().as_ref())
    ));
}
