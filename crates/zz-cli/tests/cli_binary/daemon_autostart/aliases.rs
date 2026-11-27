use super::*;

#[test]
fn attach_prefix_defers_live_user_aliases_to_the_daemon() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "named"]);
    assert_eq!(created.status.code(), Some(0));

    let unaliased = fixture.run(&["a"]);
    assert_eq!(unaliased.status.code(), Some(1));
    assert!(unaliased.stdout.is_empty());
    assert_eq!(unaliased.stderr, b"open terminal failed: not a terminal\n");

    let configured = fixture.run(&[
        "set-option",
        "-s",
        "command-alias[40]",
        "a=list-sessions -F '#{session_name}'",
    ]);
    assert_eq!(configured.status.code(), Some(0));
    let aliased = fixture.run(&["a"]);
    assert_eq!(aliased.status.code(), Some(0));
    assert_eq!(aliased.stdout, b"named\n");
    assert!(aliased.stderr.is_empty());
}

#[test]
fn exact_attach_aliases_bypass_the_native_attach_wrapper() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "named"])
            .status
            .success()
    );
    for (index, command) in ["attach", "attach-session"].into_iter().enumerate() {
        let marker = format!("{command}-shadow");
        let alias = format!("{command}=display-message -p {marker}");
        let option = format!("command-alias[{}]", 40 + index);
        assert!(
            fixture
                .run(&["set-option", "-s", &option, &alias])
                .status
                .success()
        );
        let output = fixture.run(&[command]);
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, format!("{marker}\n").as_bytes());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn arbitrary_attach_alias_uses_the_tui_path() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "named"])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&[
                "set-option",
                "-s",
                "command-alias[40]",
                "go=display-message -p before ; attach-session -t named",
            ])
            .status
            .success()
    );
    let output = fixture.run(&["go"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"before\n");
    assert_eq!(output.stderr, b"open terminal failed: not a terminal\n");
}

#[test]
fn caller_stream_source_alias_group_applies_once_and_reports_the_spent_reader() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "stream"])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&[
                "set-option",
                "-s",
                "command-alias[40]",
                "stream=display-message -p before ; source-file - ; source-file - ; display-message -p after ; set -g @after yes",
            ])
            .status
            .success()
    );
    let output = fixture.run_with_stdin(&["stream"], b"set -ag @stream once\n");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"before\nafter\n");
    assert_eq!(
        fixture.run(&["show-options", "-gqv", "@after"]).stdout,
        b"yes\n"
    );
    assert_eq!(output.stderr, b"Bad file descriptor: -\n");
    assert_eq!(
        fixture.run(&["show-options", "-gqv", "@stream"]).stdout,
        b"once\n"
    );
}

#[test]
fn caller_stream_buffer_alias_groups_preserve_bytes_and_continue_after_a_spent_reader() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "stream"])
            .status
            .success()
    );
    let payload = b"a\xff\0z";
    for (body, stdout, stderr, status) in [
        (
            "load-buffer -b alias - ; save-buffer -b alias -",
            &b"a\xff\0z"[..],
            &b""[..],
            0,
        ),
        ("load-buffer -b alias -", &b""[..], &b""[..], 0),
        (
            "load-buffer -b alias - ; source-file -",
            &b""[..],
            &b"Bad file descriptor: -\n"[..],
            1,
        ),
        (
            "load-buffer -b alias - ; load-buffer -b second - ; display-message -p after-error",
            &b"after-error\n"[..],
            &b"Bad file descriptor: -\n"[..],
            1,
        ),
        (
            "load-buffer -b alias - ; display-message -p after",
            &b"after\n"[..],
            &b""[..],
            0,
        ),
        (
            "display-message -p before ; load-buffer -b alias -",
            &b"before\n"[..],
            &b""[..],
            0,
        ),
    ] {
        assert!(
            fixture
                .run(&[
                    "set-option",
                    "-s",
                    "command-alias[40]",
                    &format!("stream={body}")
                ])
                .status
                .success()
        );
        let output = fixture.run_with_stdin(&["stream"], payload);
        assert_eq!(output.status.code(), Some(status), "{body}: {output:?}");
        assert_eq!(output.stdout, stdout, "{body}");
        assert_eq!(output.stderr, stderr, "{body}");
        let saved = fixture.run(&["save-buffer", "-b", "alias", "-"]);
        assert!(saved.status.success());
        assert_eq!(saved.stdout, payload, "{body}");
        assert!(saved.stderr.is_empty());
        assert_eq!(
            fixture
                .run(&["list-buffers", "-F", "#{buffer_name}"])
                .stdout,
            b"alias\n"
        );
        assert!(
            fixture
                .run(&["delete-buffer", "-b", "alias"])
                .status
                .success()
        );
    }
    assert!(
        fixture
            .run(&[
                "set-option",
                "-s",
                "command-alias[40]",
                "stream=source-file - ; load-buffer -b second - ; display-message -p after-error",
            ])
            .status
            .success()
    );
    let output = fixture.run_with_stdin(&["stream"], b"set -g @stream source-first\n");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"after-error\n");
    assert_eq!(output.stderr, b"Bad file descriptor: -\n");
    assert_eq!(
        fixture.run(&["show-options", "-gqv", "@stream"]).stdout,
        b"source-first\n"
    );
    assert!(
        fixture
            .run(&["list-buffers", "-F", "#{buffer_name}"])
            .stdout
            .is_empty()
    );
}

#[test]
fn caller_stream_file_replay_keeps_one_reader_across_aliases_and_nested_files() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "stream"])
            .status
            .success()
    );
    let child = write_source(fixture._directory.path(), "child.conf", "stream\n");
    let parent = write_source(
        fixture._directory.path(),
        "parent.conf",
        &format!("source-file {child}\n"),
    );
    for (body, status, stderr) in [
        ("source-file -", 0, &b""[..]),
        ("set -g @nonreader yes ; source-file -", 0, &b""[..]),
        (
            "source-file - ; source-file - ; set -g @after yes",
            1,
            &b"Bad file descriptor: -\n"[..],
        ),
    ] {
        assert!(
            fixture
                .run(&["set", "-s", "command-alias[40]", &format!("stream={body}")])
                .status
                .success()
        );
        for path in [&child, &parent] {
            assert!(fixture.run(&["set", "-gu", "@stream"]).status.success());
            let output = fixture.run_with_stdin(&["source-file", path], b"set -ag @stream once\n");
            assert_eq!(output.status.code(), Some(status), "{body}: {output:?}");
            assert_eq!(output.stdout, b"", "{body}");
            assert_eq!(output.stderr, stderr, "{body}");
            assert_eq!(
                fixture.run(&["show-options", "-gqv", "@stream"]).stdout,
                b"once\n"
            );
        }
    }
    assert!(
        fixture
            .run(&[
                "set",
                "-s",
                "command-alias[40]",
                "stream=load-buffer -b file-stream - ; save-buffer -b file-stream -"
            ])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&[
                "set",
                "-s",
                "command-alias[41]",
                "tail=display-message -p ignored ; set -g @tail yes"
            ])
            .status
            .success()
    );
    let with_tail = write_source(
        fixture._directory.path(),
        "with-tail.conf",
        "stream\ntail\n",
    );
    for path in [&child, &parent, &with_tail] {
        let output = fixture.run_with_stdin(&["source-file", path], b"a\xff\0z");
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stdout, b"a\xff\0z");
        assert!(output.stderr.is_empty());
    }
    assert_eq!(
        fixture.run(&["show-options", "-gqv", "@after"]).stdout,
        b"yes\n"
    );
}

#[test]
fn live_agent_send_aliases_control_stdin_capture() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "agent-stdin"])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&[
                "set-option",
                "-s",
                "command-alias[40]",
                "pipe=display-message -p before ; agent-send -t %0",
            ])
            .status
            .success()
    );
    let aliased = fixture.run_with_stdin(&["pipe"], b"review this\n");
    assert_eq!(aliased.status.code(), Some(1));
    assert_eq!(aliased.stdout, b"before\n");
    assert_eq!(
        aliased.stderr,
        b"target not found: no agent runs in %0; send-text types into any pane\n"
    );

    assert!(
        fixture
            .run(&[
                "set-option",
                "-s",
                "command-alias[41]",
                "agent-send=display-message -p shadow",
            ])
            .status
            .success()
    );
    let shadowed = fixture.run_with_stdin(&["agent-send"], b"must stay unread\n");
    assert_eq!(shadowed.status.code(), Some(0));
    assert_eq!(shadowed.stdout, b"shadow\n");
    assert!(shadowed.stderr.is_empty());
}

#[test]
fn failing_kill_server_alias_does_not_enter_recovery() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "alive"])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&[
                "set-option",
                "-s",
                "command-alias[40]",
                "kill-server=has-session -t missing",
            ])
            .status
            .success()
    );
    let killed = fixture.run(&["kill-server"]);
    assert_eq!(killed.status.code(), Some(1));
    assert!(fixture.socket.exists());
    let alive = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(alive.status.code(), Some(0));
    assert_eq!(alive.stdout, b"alive\n");
    assert!(
        fixture
            .run(&["set-option", "-su", "command-alias[40]"])
            .status
            .success()
    );
}

#[test]
fn prepared_kill_server_stops_the_daemon_without_recovery() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "alive"])
            .status
            .success()
    );
    let killed = fixture.run(&["kill-server"]);
    assert_eq!(killed.status.code(), Some(0));
    assert!(killed.stdout.is_empty());
    assert!(killed.stderr.is_empty());
    let deadline = Instant::now() + Duration::from_secs(2);
    while fixture.socket.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!fixture.socket.exists());
}

#[test]
fn raw_kill_server_flag_ignores_the_live_alias_table() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "alive"])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&[
                "set-option",
                "-s",
                "command-alias[40]",
                "kill-server=display-message -p shadow",
            ])
            .status
            .success()
    );
    let killed = fixture.run(&["--kill-server"]);
    assert_eq!(killed.status.code(), Some(0));
    assert!(killed.stdout.is_empty());
    assert!(killed.stderr.is_empty());
    let deadline = Instant::now() + Duration::from_secs(2);
    while fixture.socket.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!fixture.socket.exists());
}

#[test]
fn cli_prepare_freezes_aliases_for_the_whole_command_chain() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "snapshot"])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&[
                "set-option",
                "-s",
                "command-alias[40]",
                "live=display-message -p old",
            ])
            .status
            .success()
    );
    let frozen = fixture.run(&[
        "set-option",
        "-s",
        "command-alias[40]",
        "live=display-message -p new",
        ";",
        "live",
    ]);
    assert_eq!(frozen.status.code(), Some(0));
    assert_eq!(frozen.stdout, b"old\n");
    assert!(frozen.stderr.is_empty());
    let next = fixture.run(&["live"]);
    assert_eq!(next.status.code(), Some(0));
    assert_eq!(next.stdout, b"new\n");
}

#[test]
fn prepared_cli_chain_rejects_later_alias_parse_errors_before_effects() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "atomic"])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&[
                "set-option",
                "-s",
                "command-alias[40]",
                "broken=display-message \\",
            ])
            .status
            .success()
    );

    let rejected = fixture.run(&[
        "set-environment",
        "-g",
        "CLI_CHAIN_BEFORE",
        "mutated",
        ";",
        "broken",
    ]);
    assert_eq!(rejected.status.code(), Some(1));
    assert!(rejected.stdout.is_empty());
    assert_eq!(rejected.stderr, b"unknown command: broken\n");

    let marker = fixture.run(&["show-environment", "-g", "CLI_CHAIN_BEFORE"]);
    assert_eq!(marker.status.code(), Some(1));
    assert!(marker.stdout.is_empty());
    assert_eq!(marker.stderr, b"unknown variable: CLI_CHAIN_BEFORE\n");

    let alive = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(alive.status.code(), Some(0));
    assert_eq!(alive.stdout, b"atomic\n");
    assert!(alive.stderr.is_empty());
}

#[test]
fn prepared_cli_chain_rejects_later_unaliased_argument_errors_before_effects() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "atomic-unaliased"])
            .status
            .success()
    );

    let cases: &[(&str, &[&str], &[u8])] = &[
        (
            "CLI_CHAIN_INVALID_FLAG",
            &["list-sessions", "-Z"],
            b"command list-sessions: unknown flag -Z\n",
        ),
        (
            "CLI_CHAIN_TOO_MANY",
            &["list-sessions", "extra"],
            b"command list-sessions: too many arguments (need at most 0)\n",
        ),
        (
            "CLI_CHAIN_MISSING_VALUE",
            &["list-sessions", "-F"],
            b"command list-sessions: -F expects an argument\n",
        ),
        (
            "CLI_CHAIN_LATE_ATTACH",
            &["attach", "atomic-unaliased"],
            b"command attach-session: too many arguments (need at most 0)\n",
        ),
        (
            "CLI_CHAIN_LATE_ATTACH_SESSION",
            &["attach-session", "atomic-unaliased"],
            b"command attach-session: too many arguments (need at most 0)\n",
        ),
    ];
    for &(marker, later, expected) in cases {
        let mut arguments = vec!["set-environment", "-g", marker, "mutated", ";"];
        arguments.extend_from_slice(later);
        let rejected = fixture.run(&arguments);
        assert_eq!(rejected.status.code(), Some(1), "{marker}");
        assert!(rejected.stdout.is_empty(), "{marker}");
        assert_eq!(rejected.stderr, expected, "{marker}");

        let marker_output = fixture.run(&["show-environment", "-g", marker]);
        assert_eq!(marker_output.status.code(), Some(1), "{marker}");
        assert!(marker_output.stdout.is_empty(), "{marker}");
        assert_eq!(
            marker_output.stderr,
            format!("unknown variable: {marker}\n").as_bytes(),
            "{marker}"
        );
    }

    let runtime = fixture.run(&[
        "set-environment",
        "-g",
        "CLI_RUNTIME_BEFORE",
        "kept",
        ";",
        "has-session",
        "-t",
        "missing",
        ";",
        "set-environment",
        "-g",
        "CLI_RUNTIME_AFTER",
        "bad",
    ]);
    assert_eq!(runtime.status.code(), Some(1));
    assert!(runtime.stdout.is_empty());
    assert_eq!(runtime.stderr, b"can't find session: missing\n");

    let before = fixture.run(&["show-environment", "-g", "CLI_RUNTIME_BEFORE"]);
    assert_eq!(before.status.code(), Some(0));
    assert_eq!(before.stdout, b"CLI_RUNTIME_BEFORE=kept\n");
    assert!(before.stderr.is_empty());

    let after = fixture.run(&["show-environment", "-g", "CLI_RUNTIME_AFTER"]);
    assert_eq!(after.status.code(), Some(1));
    assert!(after.stdout.is_empty());
    assert_eq!(after.stderr, b"unknown variable: CLI_RUNTIME_AFTER\n");
}

#[test]
fn cold_cli_parse_errors_do_not_start_or_mutate_a_daemon() {
    let cases: &[&[&str]] = &[
        &["new-session", "-d", "-s", "before", ";", "frobnicate"],
        &[
            "new-session",
            "-d",
            "-s",
            "before",
            ";",
            "list-sessions",
            "-Z",
        ],
        &[
            "new-session",
            "-d",
            "-s",
            "before",
            ";",
            "list-sessions",
            "-F",
        ],
        &[
            "new-session",
            "-d",
            "-s",
            "before",
            ";",
            "list-sessions",
            "extra",
        ],
        &["new", "-d", "-s", "before", ";", "lscm", "-Z"],
        &["new-session", "-s", "before", ";", "frobnicate"],
        &["-N", "new-session", "-s", "before", ";", "frobnicate"],
        &["-N", "attach"],
        &["-N", "attach-session"],
        &["attach", ";", "frobnicate"],
        &["attach-session", ";", "frobnicate"],
        &["new-session", "-d", "-s", "before", ";", "clock-mode", "-Z"],
        &[
            "new-session",
            "-d",
            "-s",
            "before",
            ";",
            "suspendc",
            "extra",
        ],
    ];
    for arguments in cases {
        let fixture = Fixture::new();
        if !local_socket_bind_available(&fixture.socket) {
            return;
        }
        let output = fixture.run(arguments);
        if arguments == &["-N", "attach"] || arguments == &["-N", "attach-session"] {
            assert_missing(&output, &fixture.missing_message());
        } else {
            let expected = match arguments.last().copied() {
                Some("frobnicate") => "unknown command: frobnicate",
                Some("-F") => "command list-sessions: -F expects an argument",
                Some("-Z") if arguments.contains(&"lscm") => {
                    "command list-commands: unknown flag -Z"
                }
                Some("-Z") if arguments.contains(&"clock-mode") => {
                    "command clock-mode: unknown flag -Z"
                }
                Some("-Z") => "command list-sessions: unknown flag -Z",
                Some("extra") if arguments.contains(&"suspendc") => {
                    "command suspend-client: too many arguments (need at most 0)"
                }
                Some("extra") => "command list-sessions: too many arguments (need at most 0)",
                _ => unreachable!("covered cold invocation"),
            };
            assert_eq!(output.status.code(), Some(1), "{arguments:?}");
            assert!(output.stdout.is_empty(), "{arguments:?}");
            assert_eq!(
                output.stderr,
                format!("{expected}\n").as_bytes(),
                "{arguments:?}"
            );
        }
        fixture.assert_not_started();
    }
}

#[test]
fn cold_cli_valid_builtin_alias_chain_still_starts_and_executes() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let output = fixture.run(&[
        "new",
        "-d",
        "-s",
        "before",
        ";",
        "ls",
        "-F",
        "#{session_name}",
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"before\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn arbitrary_startup_alias_cannot_trigger_cold_autostart() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    std::fs::write(
        &fixture.config,
        b"set-option -s command-alias[0] 'go=new-session -d -s from-alias'\n",
    )
    .expect("write startup alias");
    let output = fixture.run(&["go"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"unknown command: go\n");
    fixture.assert_not_started();
}

#[test]
fn canonical_startup_alias_shadow_is_prepared_after_config() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    std::fs::write(
        &fixture.config,
        b"set-option -s command-alias[0] 'list-sessions=display-message -p STARTUP_VALID'\n",
    )
    .expect("write startup alias");
    let output = fixture.run(&["new-session", "-d", "-s", "before", ";", "list-sessions"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"STARTUP_VALID\n");
    assert!(output.stderr.is_empty());
    let session = fixture.run(&["has-session", "-t", "=before"]);
    assert_eq!(session.status.code(), Some(0));
    assert!(session.stdout.is_empty());
    assert!(session.stderr.is_empty());
}

#[test]
fn canonical_attach_startup_alias_shadow_is_prepared_after_config() {
    for command in ["attach", "attach-session"] {
        let fixture = Fixture::new();
        if !local_socket_bind_available(&fixture.socket) {
            return;
        }
        std::fs::write(
            &fixture.config,
            format!(
                "set-option -s command-alias[0] '{command}=display-message -p STARTUP_ATTACH'\n"
            ),
        )
        .expect("write startup alias");
        let output = fixture.run(&[command]);
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, b"STARTUP_ATTACH\n");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn failed_preflight_handshake_still_runs_the_cold_static_gate() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let listener = std::os::unix::net::UnixListener::bind(&fixture.socket)
        .expect("bind fake preflight listener");
    let socket = fixture.socket.clone();
    let fake = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept preflight client");
        std::fs::remove_file(&socket).expect("unlink fake listener");
        drop(stream);
    });
    let output = fixture.run(&["new-session", "-d", "-s", "before-reset", ";", "frobnicate"]);
    fake.join().expect("join fake listener");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    fixture.assert_not_started();
}

#[test]
fn invalid_startup_alias_shadows_abort_before_cold_effects() {
    for alias in [
        "list-sessions=frobnicate",
        "list-sessions=confirm-before { frobnicate }",
    ] {
        let fixture = Fixture::new();
        if !local_socket_bind_available(&fixture.socket) {
            return;
        }
        std::fs::write(
            &fixture.config,
            format!("set-option -s command-alias[0] '{alias}'\n"),
        )
        .expect("write startup alias");
        let output = fixture.run(&["new-session", "-d", "-s", "before", ";", "list-sessions"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, b"unknown command: frobnicate\n");
        fixture.assert_stopped();
    }
}

#[test]
fn tui_handoff_executes_the_prepared_alias_snapshot() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "seed"])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&[
                "set-option",
                "-s",
                "command-alias[40]",
                "go=display-message -p before ; new-session -s frozen-old",
            ])
            .status
            .success()
    );
    let frozen = fixture.run(&[
        "set-option",
        "-s",
        "command-alias[40]",
        "go=display-message -p changed",
        ";",
        "go",
    ]);
    assert_eq!(frozen.status.code(), Some(1));
    assert_eq!(frozen.stdout, b"before\n");
    assert_eq!(frozen.stderr, b"open terminal failed: not a terminal\n");
    let sessions = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(sessions.status.code(), Some(0));
    assert!(
        !String::from_utf8_lossy(&sessions.stdout)
            .lines()
            .any(|session| session == "frozen-old")
    );
    let changed = fixture.run(&["go"]);
    assert_eq!(changed.status.code(), Some(0));
    assert_eq!(changed.stdout, b"changed\n");
}

#[test]
fn agent_send_prefix_reads_stdin_before_dispatch() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "agent-stdin"]);
    assert_eq!(created.status.code(), Some(0));
    let sent = fixture.run_with_stdin(&["agent-s", "-t", "%0"], b"review this\n");
    assert_eq!(sent.status.code(), Some(1));
    assert!(sent.stdout.is_empty());
    assert_eq!(
        sent.stderr,
        b"target not found: no agent runs in %0; send-text types into any pane\n"
    );
}
