use super::*;

#[test]
fn control_return_and_explicit_detach_follow_the_full_retval_matrix() {
    #[derive(Clone, Copy)]
    enum ExitPath {
        Eof,
        Blank,
        DetachCompleted,
        DetachQueuedOpen,
        DetachQueuedEof,
    }

    struct Row {
        name: &'static str,
        command: String,
        return_code: i32,
        payload: &'static str,
        error: bool,
    }

    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture._directory.path().join("control return matrix");
    std::fs::create_dir(&directory).expect("create control return matrix directory");
    let runtime_source = write_source(
        &directory,
        "runtime.conf",
        "kill-session -t matrix-sourced-runtime\n",
    );
    let source_failure = write_source(
        &directory,
        "source-failure.conf",
        "source-file matrix-nested-missing.conf\n",
    );
    let rows = [
        Row {
            name: "direct-runtime",
            command: "kill-session -t matrix-direct-runtime".to_owned(),
            return_code: 1,
            payload: "can't find session: matrix-direct-runtime",
            error: true,
        },
        Row {
            name: "sourced-runtime",
            command: format!("source-file '{runtime_source}'"),
            return_code: 1,
            payload: "can't find session: matrix-sourced-runtime",
            error: true,
        },
        Row {
            name: "sourced-command",
            command: format!("source-file '{source_failure}'"),
            return_code: 1,
            payload: "No such file or directory: matrix-nested-missing.conf",
            error: true,
        },
        Row {
            name: "generic-nonzero",
            command: "run-shell 'exit 3'".to_owned(),
            return_code: 0,
            payload: "'exit 3' returned 3",
            error: false,
        },
    ];
    let exits = [
        ("eof", ExitPath::Eof),
        ("blank", ExitPath::Blank),
        ("detach-completed", ExitPath::DetachCompleted),
        ("detach-queued-open", ExitPath::DetachQueuedOpen),
        ("detach-queued-eof", ExitPath::DetachQueuedEof),
    ];

    for (row_index, row) in rows.iter().enumerate() {
        for (exit_index, (exit_name, exit_path)) in exits.iter().enumerate() {
            let label = format!("{}-{exit_name}", row.name);
            let session = format!("matrix-{row_index}-{exit_index}");
            let ready = directory.join(format!("{label}.ready"));
            let release = directory.join(format!("{label}.release"));
            let output_path = directory.join(format!("{label}.output"));
            let (mut child, mut stdin) = spawn_control_to_file(
                &fixture,
                &["-C", "new-session", "-s", &session, "exec /bin/cat"],
                &output_path,
            );
            writeln!(stdin, "{}", row.command).expect("write matrix command");

            match exit_path {
                ExitPath::Eof | ExitPath::Blank | ExitPath::DetachCompleted => {
                    let complete = format!("MATRIX_COMPLETE_{row_index}_{exit_index}");
                    writeln!(stdin, "display-message -p {complete}")
                        .expect("write completion marker command");
                    stdin.flush().expect("flush completed matrix commands");
                    wait_for_control_output_marker(&output_path, &complete, &mut child, &label);
                }
                ExitPath::DetachQueuedOpen | ExitPath::DetachQueuedEof => {
                    writeln!(
                        stdin,
                        "run-shell 'touch \"{}\"; while [ ! -e \"{}\" ]; do sleep 0.01; done'",
                        ready.display(),
                        release.display()
                    )
                    .expect("write held matrix command");
                    stdin.flush().expect("flush held matrix commands");
                    wait_for_control_marker(&ready, &mut child, &label);
                }
            }

            let output = match exit_path {
                ExitPath::Eof => {
                    drop(stdin);
                    collect_control_process(child, None, &label)
                }
                ExitPath::Blank => {
                    writeln!(stdin).expect("write matrix blank return");
                    stdin.flush().expect("flush matrix blank return");
                    collect_control_process(child, Some(stdin), &label)
                }
                ExitPath::DetachCompleted => {
                    writeln!(stdin, "detach-client").expect("write completed detach");
                    stdin.flush().expect("flush completed detach");
                    collect_control_process(child, Some(stdin), &label)
                }
                ExitPath::DetachQueuedOpen => {
                    writeln!(stdin, "detach-client").expect("queue held-open detach");
                    stdin.flush().expect("flush held-open detach");
                    std::fs::write(&release, b"").expect("release held-open command");
                    collect_control_process(child, Some(stdin), &label)
                }
                ExitPath::DetachQueuedEof => {
                    writeln!(stdin, "detach-client").expect("queue detach before EOF");
                    stdin.flush().expect("flush detach before EOF");
                    drop(stdin);
                    thread::sleep(Duration::from_millis(100));
                    std::fs::write(&release, b"").expect("release EOF-held command");
                    collect_control_process(child, None, &label)
                }
            };
            let expected_status = match exit_path {
                ExitPath::Eof | ExitPath::Blank | ExitPath::DetachQueuedEof => row.return_code,
                ExitPath::DetachCompleted | ExitPath::DetachQueuedOpen => 0,
            };
            let stdout = std::fs::read(&output_path).expect("read matrix control output");
            assert_eq!(
                output.status.code(),
                Some(expected_status),
                "{label}: {}",
                String::from_utf8_lossy(&stdout)
            );
            assert!(output.stderr.is_empty(), "{label}");
            let hidden = match row.name {
                "sourced-runtime" => 1,
                "sourced-command" => 2,
                _ => 0,
            };
            let stream = if hidden == 0 {
                parse_stream(&stdout, false)
            } else {
                parse_stream_allow_gaps(&stdout, false)
            };
            assert_eq!(
                stream.blocks.last().expect("matrix block").number - stream.blocks.len() as u64,
                hidden,
                "{label}: {stream:?}"
            );
            if row.name == "generic-nonzero" {
                assert!(
                    stream.outside.iter().any(|line| line == row.payload),
                    "{label}: {stream:?}"
                );
                assert!(
                    stream.blocks.iter().any(|block| {
                        block.flags == 1 && !block.error && block.payload.is_empty()
                    }),
                    "{label}: {stream:?}"
                );
            } else {
                assert!(
                    stream.blocks.iter().any(|block| {
                        block.error == row.error
                            && block.payload.iter().any(|line| line == row.payload)
                    }),
                    "{label}: {stream:?}"
                );
            }
            assert_eq!(
                stream.outside.last().map(String::as_str),
                Some("%exit"),
                "{label}"
            );
        }
    }
}

#[test]
fn control_immediate_eof_keeps_a_direct_source_failure_status() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let missing = fixture._directory.path().join("immediate-eof-missing.conf");
    let output = fixture.run_with_stdin(
        &[
            "-C",
            "new-session",
            "-s",
            "immediate-eof-source",
            "exec /bin/cat",
        ],
        format!("source-file '{}'\n", missing.display()).as_bytes(),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stream = parse_stream_allow_gaps(&output.stdout, false);
    assert!(stream.blocks.iter().any(|block| {
        block.error
            && block
                .payload
                .iter()
                .any(|line| line.contains("immediate-eof-missing.conf"))
    }));
    assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
}

#[test]
fn control_attached_eof_releases_the_queue_at_the_first_parked_command() {
    for kind in ["runtime", "source", "run-shell", "confirm-before"] {
        let fixture = Fixture::new();
        if !local_socket_bind_available(&fixture.socket) {
            return;
        }
        let session = format!("attached-eof-{kind}");
        assert!(
            fixture
                .run(&["new-session", "-d", "-s", &session, "exec /bin/cat"])
                .status
                .success()
        );
        let output_path = fixture
            ._directory
            .path()
            .join(format!("attached-eof-{kind}.output"));
        let missing = fixture
            ._directory
            .path()
            .join("attached-eof-source-missing.conf");
        let (failure, expected) = match kind {
            "runtime" => (
                "kill-session -t attached-eof-runtime-missing".to_owned(),
                "can't find session: attached-eof-runtime-missing",
            ),
            "source" => (
                format!("source-file '{}'", missing.display()),
                "attached-eof-source-missing.conf",
            ),
            "run-shell" => (
                "run-shell -d not-a-number 'true'".to_owned(),
                "invalid delay time: not-a-number",
            ),
            "confirm-before" => (
                "confirm-before -c xx { display-message -p no }".to_owned(),
                "invalid confirm key",
            ),
            _ => unreachable!(),
        };
        let blocker_ready = fixture
            ._directory
            .path()
            .join(format!("attached-eof-{kind}.ready"));
        let blocker_release = fixture
            ._directory
            .path()
            .join(format!("attached-eof-{kind}.release"));
        let (mut child, mut stdin) = spawn_control_to_file(
            &fixture,
            &["-C", "attach-session", "-t", &format!("={session}")],
            &output_path,
        );
        wait_for_control_clients(&fixture, 1, &format!("attached EOF {kind}"));
        writeln!(stdin, "display-message -p ATTACHED_EOF_READY")
            .expect("write attached EOF readiness command");
        stdin.flush().expect("flush attached EOF readiness command");
        wait_for_control_output_marker(
            &output_path,
            "ATTACHED_EOF_READY",
            &mut child,
            &format!("attached EOF {kind} readiness"),
        );
        writeln!(
            stdin,
            "run-shell 'touch \"{}\"; while [ ! -e \"{}\" ]; do sleep 0.01; done' ; {failure}",
            blocker_ready.display(),
            blocker_release.display(),
        )
        .expect("write attached EOF failure");
        stdin.flush().expect("flush attached EOF failure");
        wait_for_control_marker(
            &blocker_ready,
            &mut child,
            &format!("attached EOF {kind} blocker"),
        );
        drop(stdin);
        thread::sleep(Duration::from_millis(100));
        std::fs::write(&blocker_release, b"").expect("release attached EOF blocker");

        let output = collect_control_process(child, None, &format!("attached EOF {kind} failure"));
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let stdout = std::fs::read(&output_path).expect("read attached EOF output");
        let stream = parse_stream_allow_gaps(&stdout, false);
        assert!(
            !stream
                .blocks
                .iter()
                .any(|block| block.payload.iter().any(|line| line.contains(expected))),
            "{stream:?}"
        );
        assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
    }
}

#[test]
fn control_nonself_detach_stays_attached_and_preserves_queued_return() {
    for (label, detach, create_other, blank_return) in [
        ("detach-others", "detach-client -a", false, true),
        (
            "detach-other-session",
            "detach-client -s =scope-other",
            true,
            false,
        ),
        (
            "detach-missing-session",
            "detach-client -s =scope-missing",
            false,
            false,
        ),
    ] {
        let fixture = Fixture::new();
        if !local_socket_bind_available(&fixture.socket) {
            return;
        }
        assert!(
            fixture
                .run(&["new-session", "-d", "-s", "scope-caller", "exec /bin/cat",])
                .status
                .success()
        );
        if create_other {
            assert!(
                fixture
                    .run(&["new-session", "-d", "-s", "scope-other", "exec /bin/cat",])
                    .status
                    .success()
            );
        }
        let output_path = fixture._directory.path().join(format!("{label}.output"));
        let (mut child, mut stdin) = spawn_control_to_file(
            &fixture,
            &["-C", "attach-session", "-t", "=scope-caller"],
            &output_path,
        );
        wait_for_control_clients(&fixture, 1, label);
        let ready = format!("{}_READY", label.replace('-', "_").to_uppercase());
        prime_control_return_code(
            &mut child,
            &mut stdin,
            &output_path,
            &ready,
            &format!("{label}-missing"),
        );
        let after = format!("{}_AFTER", label.replace('-', "_").to_uppercase());
        writeln!(stdin, "{detach}").expect("write nonself detach");
        writeln!(stdin, "display-message -p {after}").expect("write command after nonself detach");
        stdin.flush().expect("flush nonself detach commands");
        wait_for_control_output_marker(&output_path, &after, &mut child, label);
        let output = if blank_return {
            writeln!(stdin).expect("write nonself blank return");
            stdin.flush().expect("flush nonself blank return");
            collect_control_process(child, Some(stdin), label)
        } else {
            drop(stdin);
            collect_control_process(child, None, label)
        };
        assert_eq!(output.status.code(), Some(1), "{label}");
        assert!(output.stderr.is_empty(), "{label}");
        let stdout = std::fs::read(&output_path).expect("read nonself detach output");
        let stream = parse_stream(&stdout, false);
        assert!(
            stream
                .blocks
                .iter()
                .any(|block| block.payload == [after.as_str()]),
            "{label}: {stream:?}"
        );
        assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
    }

    for (label, blank_return, continue_after) in [
        ("target-other-eof", false, false),
        ("target-other-blank", true, false),
        ("target-other-continue", true, true),
    ] {
        let fixture = Fixture::new();
        if !local_socket_bind_available(&fixture.socket) {
            return;
        }
        assert!(
            fixture
                .run(&["new-session", "-d", "-s", "target-other", "exec /bin/cat",])
                .status
                .success()
        );
        let (peer, peer_stdin) =
            fixture.spawn_with_open_stdin(&["-C", "attach-session", "-t", "=target-other"]);
        let peer_name = wait_for_control_clients(&fixture, 1, label)
            .into_iter()
            .next()
            .expect("peer control client");
        let output_path = fixture._directory.path().join(format!("{label}.output"));
        let (mut child, mut stdin) = spawn_control_to_file(
            &fixture,
            &["-C", "attach-session", "-t", "=target-other"],
            &output_path,
        );
        let clients = wait_for_control_clients(&fixture, 2, label);
        assert!(clients.contains(&peer_name), "{label}: {clients:?}");
        let ready = format!("{}_READY", label.replace('-', "_").to_uppercase());
        prime_control_return_code(
            &mut child,
            &mut stdin,
            &output_path,
            &ready,
            &format!("{label}-missing"),
        );
        writeln!(stdin, "detach-client -t '{peer_name}'").expect("write target-other detach");
        let output = if continue_after {
            writeln!(stdin, "display-message -p TARGET_OTHER_AFTER")
                .expect("write command after target-other detach");
            stdin.flush().expect("flush target-other continuation");
            wait_for_control_output_marker(&output_path, "TARGET_OTHER_AFTER", &mut child, label);
            writeln!(stdin).expect("write target-other continuation return");
            stdin
                .flush()
                .expect("flush target-other continuation return");
            collect_control_process(child, Some(stdin), label)
        } else if blank_return {
            writeln!(stdin).expect("write target-other blank return");
            stdin.flush().expect("flush target-other blank return");
            collect_control_process(child, Some(stdin), label)
        } else {
            stdin.flush().expect("flush target-other detach");
            drop(stdin);
            collect_control_process(child, None, label)
        };
        assert_eq!(output.status.code(), Some(1), "{label}");
        assert!(output.stderr.is_empty(), "{label}");
        let peer_output = collect_control_process(peer, Some(peer_stdin), label);
        assert_eq!(peer_output.status.code(), Some(0), "{label}");
        assert!(peer_output.stderr.is_empty(), "{label}");
    }
}

#[test]
fn control_detach_aliases_follow_the_authoritative_self_victim() {
    for (label, command, install_alias) in [
        ("bare-self-detach", "detach-client".to_owned(), false),
        ("built-in-alias-self-detach", "detach".to_owned(), false),
        ("alias-self-detach", "dc".to_owned(), true),
        (
            "scoped-self-detach",
            "detach-client -s =scoped-self-detach".to_owned(),
            false,
        ),
    ] {
        let fixture = Fixture::new();
        if !local_socket_bind_available(&fixture.socket) {
            return;
        }
        assert!(
            fixture
                .run(&["new-session", "-d", "-s", label, "exec /bin/cat",])
                .status
                .success()
        );
        if install_alias {
            assert!(
                fixture
                    .run(&["set-option", "-s", "command-alias[40]", "dc=detach-client",])
                    .status
                    .success()
            );
        }
        let output_path = fixture._directory.path().join(format!("{label}.output"));
        let (mut child, mut stdin) = spawn_control_to_file(
            &fixture,
            &["-C", "attach-session", "-t", &format!("={label}")],
            &output_path,
        );
        wait_for_control_clients(&fixture, 1, label);
        let ready = format!("{}_READY", label.replace('-', "_").to_uppercase());
        prime_control_return_code(
            &mut child,
            &mut stdin,
            &output_path,
            &ready,
            &format!("{label}-missing"),
        );
        writeln!(stdin, "{command}").expect("write self detach");
        stdin.flush().expect("flush self detach");
        drop(stdin);
        let output = collect_control_process(child, None, label);
        assert_eq!(output.status.code(), Some(0), "{label}");
        assert!(output.stderr.is_empty(), "{label}");
        let stdout = std::fs::read(&output_path).expect("read self-detach output");
        let stream = parse_stream(&stdout, false);
        assert_block(
            stream.blocks.last().expect("self-detach response block"),
            4,
            1,
            &[],
            false,
        );
        assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
    }

    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&[
                "new-session",
                "-d",
                "-s",
                "alias-other-detach",
                "exec /bin/cat",
            ])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&["set-option", "-s", "command-alias[40]", "dc=detach-client",])
            .status
            .success()
    );
    let output_path = fixture._directory.path().join("alias-other-detach.output");
    let (mut child, mut stdin) = spawn_control_to_file(
        &fixture,
        &["-C", "attach-session", "-t", "=alias-other-detach"],
        &output_path,
    );
    wait_for_control_clients(&fixture, 1, "alias other detach");
    prime_control_return_code(
        &mut child,
        &mut stdin,
        &output_path,
        "ALIAS_OTHER_READY",
        "alias-other-missing",
    );
    writeln!(stdin, "dc -a").expect("write aliased nonself detach");
    writeln!(stdin, "display-message -p ALIAS_OTHER_AFTER")
        .expect("write command after aliased nonself detach");
    stdin.flush().expect("flush aliased nonself detach");
    wait_for_control_output_marker(
        &output_path,
        "ALIAS_OTHER_AFTER",
        &mut child,
        "alias other detach",
    );
    writeln!(stdin).expect("write aliased nonself blank return");
    stdin.flush().expect("flush aliased nonself blank return");
    let output = collect_control_process(child, Some(stdin), "aliased nonself detach return");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
}

#[test]
fn control_eof_drops_a_parked_wrappers_later_failure_and_detach_eof_is_zero() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture._directory.path().join("control return precedence");
    std::fs::create_dir(&directory).expect("create return precedence directory");
    for wrapper in ["if-shell", "if-shell-format", "run-shell"] {
        let ready = directory.join(format!("pre-{wrapper}-failure.ready"));
        let release = directory.join(format!("pre-{wrapper}-failure.release"));
        let missing = format!("pre-{wrapper}-failure-missing");
        let blocker = format!(
            "run-shell 'touch \"{}\"; while [ ! -e \"{}\" ]; do sleep 0.01; done'",
            ready.display(),
            release.display()
        );
        let delayed_command = match wrapper {
            "if-shell" => format!(
                "if-shell 'touch \"{}\"; while [ ! -e \"{}\" ]; do sleep 0.01; done; true' 'kill-session -t {missing}'",
                ready.display(),
                release.display()
            ),
            "if-shell-format" => {
                format!("if-shell -F 1 {{ {blocker} ; kill-session -t {missing} }}")
            }
            "run-shell" => {
                format!("run-shell -C {{ {blocker} ; kill-session -t {missing} }}")
            }
            _ => unreachable!(),
        };
        let session = format!("pre-{wrapper}-failure-return");
        let (mut child, mut stdin) =
            fixture.spawn_with_open_stdin(&["-C", "new-session", "-s", &session, "exec /bin/cat"]);
        writeln!(stdin, "{delayed_command}").expect("write delayed command");
        stdin.flush().expect("flush delayed command");
        wait_for_control_marker(
            &ready,
            &mut child,
            &format!("pre-{wrapper}-failure command wait"),
        );
        drop(stdin);
        thread::sleep(Duration::from_millis(100));
        std::fs::write(&release, b"").expect("release delayed source");
        let output =
            collect_control_process(child, None, &format!("pre-{wrapper}-failure EOF snapshot"));
        let stream = parse_stream(&output.stdout, false);
        assert_eq!(output.status.code(), Some(0), "{stream:?}");
        assert!(output.stderr.is_empty());
        assert!(
            !stream
                .blocks
                .iter()
                .any(|block| block.payload.iter().any(|line| line.contains(&missing))),
            "{stream:?}"
        );
        assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
    }

    let runtime_source = write_source(
        &directory,
        "detach-runtime.conf",
        "kill-session -t post-detach-missing\n",
    );
    let complete = "POST_DETACH_COMPLETE";
    let output_path = directory.join("post-detach.output");
    let output_file = File::create(&output_path).expect("create live control output");
    let mut child = fixture
        .command()
        .args([
            "-C",
            "new-session",
            "-s",
            "post-detach-eof",
            "exec /bin/cat",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::from(output_file))
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn post-detach control");
    let mut stdin = child.stdin.take().expect("piped control stdin");
    writeln!(stdin, "source-file '{runtime_source}'").expect("write detach source");
    writeln!(stdin, "display-message -p {complete}").expect("write detach completion marker");
    stdin.flush().expect("flush detach completion commands");
    wait_for_control_output_marker(
        &output_path,
        complete,
        &mut child,
        "post-detach completed frame",
    );
    writeln!(stdin, "detach-client").expect("write post-completion detach");
    stdin.flush().expect("flush post-completion detach");
    drop(stdin);
    let output = collect_control_process(child, None, "post-detach immediate EOF");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&std::fs::read(&output_path).expect("read control output")),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let stdout = std::fs::read(output_path).expect("read completed control output");
    let stream = parse_stream_allow_gaps(&stdout, false);
    assert_eq!(
        stream.blocks.last().expect("post-detach block").number - stream.blocks.len() as u64,
        1
    );
    assert!(stream.blocks.iter().any(|block| {
        block.error && block.payload == ["can't find session: post-detach-missing"]
    }));
}

#[test]
fn control_server_stop_preserves_the_completed_return_code() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture._directory.path().join("control server stop status");
    std::fs::create_dir(&directory).expect("create server stop status directory");
    let source = write_source(
        &directory,
        "runtime.conf",
        "kill-session -t server-stop-missing\n",
    );
    let complete = directory.join("runtime.complete");
    let (mut child, mut stdin) = fixture.spawn_with_open_stdin(&[
        "-C",
        "new-session",
        "-s",
        "server-stop-status",
        "exec /bin/cat",
    ]);
    writeln!(stdin, "source-file '{source}'").expect("write server stop source");
    writeln!(stdin, "run-shell 'touch \"{}\"'", complete.display())
        .expect("write server stop completion marker");
    stdin.flush().expect("flush server stop setup");
    wait_for_control_marker(&complete, &mut child, "server stop completion");
    writeln!(stdin, "kill-server").expect("write server stop command");
    stdin.flush().expect("flush server stop command");
    let output = collect_control_process(child, Some(stdin), "server stop status");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stream = parse_stream_allow_gaps(&output.stdout, false);
    assert_eq!(
        stream.blocks.last().expect("server-stop block").number - stream.blocks.len() as u64,
        1
    );
    assert!(stream.blocks.iter().any(|block| {
        block.error && block.payload == ["can't find session: server-stop-missing"]
    }));
    assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
}

#[test]
fn wait_exit_holds_the_control_process_until_a_second_blank_line() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let (mut child, mut stdin) =
        fixture.spawn_with_open_stdin(&["-C", "new-session", "-s", "wait-exit", "exec /bin/cat"]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ready = fixture.run(&["has-session", "-t", "wait-exit"]);
        if ready.status.success() {
            break;
        }
        assert!(Instant::now() < deadline, "control session did not start");
        thread::sleep(Duration::from_millis(10));
    }
    stdin
        .write_all(b"refresh-client -f wait-exit\n")
        .expect("enable wait-exit");
    stdin.flush().expect("flush wait-exit flag");
    thread::sleep(Duration::from_millis(100));
    stdin.write_all(b"\n").expect("detach control client");
    stdin.flush().expect("flush detach");
    let hold_deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < hold_deadline {
        assert!(
            child.try_wait().expect("poll wait-exit process").is_none(),
            "wait-exit process exited before its acknowledgement"
        );
        thread::sleep(Duration::from_millis(10));
    }
    stdin.write_all(b"\n").expect("acknowledge exit");
    drop(stdin);

    let output = child
        .wait_with_output()
        .expect("wait for wait-exit control");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
}

#[test]
fn wait_exit_returns_at_eof_after_the_blank_line_in_both_stdio_modes() {
    for relay in [None, Some("1")] {
        let fixture = Fixture::new();
        if !local_socket_bind_available(&fixture.socket) {
            return;
        }
        let mut command = fixture.command();
        if let Some(relay) = relay {
            command.env("ZZ_CONTROL_RELAY", relay);
        }
        let mut child = command
            .args(["-C", "new-session", "-s", "wait-eof", "exec /bin/cat"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn wait-exit control client");
        let mut stdin = child.stdin.take().expect("piped stdin");
        stdin
            .write_all(b"refresh-client -f wait-exit\n\n")
            .expect("write wait-exit input");
        drop(stdin);
        let deadline = Instant::now() + Duration::from_secs(10);
        while child.try_wait().expect("poll wait-exit client").is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                panic!("wait-exit control client held past EOF (relay {relay:?})");
            }
            thread::sleep(Duration::from_millis(10));
        }
        let output = child
            .wait_with_output()
            .expect("wait for wait-exit control");
        assert_eq!(output.status.code(), Some(0), "relay {relay:?}");
        let stream = parse_stream(&output.stdout, false);
        assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
    }
}
