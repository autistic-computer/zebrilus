use super::*;

#[test]
fn control_background_inserted_commands_use_flags_zero_after_later_input() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let session = "control-background-frames";
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", session, "exec /bin/cat"])
            .status
            .success()
    );
    let directory = fixture._directory.path().join("control background frames");
    std::fs::create_dir(&directory).expect("create background frame directory");
    let child_source = write_source(
        &directory,
        "child.conf",
        "display-message -p BACKGROUND_CHILD\n",
    );
    let missing_source = directory.join("missing.conf");
    let output_path = directory.join("control.raw");
    let (mut child, mut stdin) = spawn_control_to_file(
        &fixture,
        &["-C", "attach-session", "-t", &format!("={session}")],
        &output_path,
    );
    writeln!(
        stdin,
        "run-shell -bC -d 0.3 'source-file \"{child_source}\"'"
    )
    .expect("write background source command");
    writeln!(
        stdin,
        "run-shell -bC -d 0.6 'source-file \"{}\"'",
        missing_source.display()
    )
    .expect("write background missing source command");
    writeln!(
        stdin,
        "run-shell -bC -d 0.9 'kill-session -t background-runtime-missing'"
    )
    .expect("write background runtime command");
    writeln!(
        stdin,
        "run-shell -bC -d 1.2 'display-message -p BACKGROUND_RUN'"
    )
    .expect("write background success command");
    writeln!(
        stdin,
        "if-shell -b 'sleep 1.5; false' 'display-message -p WRONG_BRANCH' 'display-message -p BACKGROUND_ELSE'"
    )
    .expect("write background false branch");
    writeln!(stdin, "display-message -p BACKGROUND_LATER").expect("write later flags-one command");
    stdin.flush().expect("flush background commands");

    wait_for_control_output_marker(
        &output_path,
        "BACKGROUND_LATER",
        &mut child,
        "later flags-one frame",
    );
    wait_for_control_output_marker(
        &output_path,
        "BACKGROUND_CHILD",
        &mut child,
        "background child frame",
    );
    wait_for_control_error_marker(
        &output_path,
        &format!("No such file or directory: {}", missing_source.display()),
        &mut child,
        "background missing source frame",
    );
    wait_for_control_error_marker(
        &output_path,
        "can't find session: background-runtime-missing",
        &mut child,
        "background runtime frame",
    );
    wait_for_control_output_marker(
        &output_path,
        "BACKGROUND_RUN",
        &mut child,
        "background run-shell frame",
    );
    wait_for_control_output_marker(
        &output_path,
        "BACKGROUND_ELSE",
        &mut child,
        "background false branch frame",
    );
    writeln!(stdin, "display-message -p BACKGROUND_STICKY_LATER")
        .expect("write sticky-status later command");
    stdin.flush().expect("flush sticky-status later command");
    wait_for_control_output_marker(
        &output_path,
        "BACKGROUND_STICKY_LATER",
        &mut child,
        "sticky-status later frame",
    );
    writeln!(stdin).expect("write background return");
    stdin.flush().expect("flush background return");
    let output = collect_control_process(child, Some(stdin), "background flags zero");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stdout = std::fs::read(&output_path).expect("read background control output");
    let stream = parse_stream_allow_gaps(&stdout, false);
    assert_eq!(stream.blocks.len(), 14, "{stream:?}");
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    assert_block(&stream.blocks[2], 3, 1, &[], false);
    assert_block(&stream.blocks[3], 4, 1, &[], false);
    assert_block(&stream.blocks[4], 5, 1, &[], false);
    assert_block(&stream.blocks[5], 6, 1, &[], false);
    assert_block(&stream.blocks[6], 7, 1, &["BACKGROUND_LATER"], false);
    assert_block(&stream.blocks[7], 8, 0, &[], false);
    assert_block(&stream.blocks[8], 9, 0, &["BACKGROUND_CHILD"], false);
    assert_block(
        &stream.blocks[9],
        11,
        0,
        &[&format!(
            "No such file or directory: {}",
            missing_source.display()
        )],
        true,
    );
    assert_block(
        &stream.blocks[10],
        13,
        0,
        &["can't find session: background-runtime-missing"],
        true,
    );
    assert_block(&stream.blocks[11], 14, 0, &["BACKGROUND_RUN"], false);
    assert_block(&stream.blocks[12], 15, 0, &["BACKGROUND_ELSE"], false);
    assert_block(
        &stream.blocks[13],
        16,
        1,
        &["BACKGROUND_STICKY_LATER"],
        false,
    );
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line == &format!("%session-changed $0 {session}"))
    );
    assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
}

#[test]
fn held_control_detached_shutdown_drains_callback_guards_before_exit() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let session = "control-background-shutdown";
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", session, "exec /bin/cat"])
            .status
            .success()
    );
    let output_path = fixture._directory.path().join("control.raw");
    let (child, mut stdin) = spawn_control_to_file(
        &fixture,
        &["-C", "attach-session", "-t", &format!("={session}")],
        &output_path,
    );
    writeln!(
        stdin,
        "run-shell -bC 'kill-server ; display-message -p AFTER'"
    )
    .expect("write detached shutdown command");
    stdin.flush().expect("flush detached shutdown command");

    let output = collect_control_process(child, Some(stdin), "detached shutdown callback");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = std::fs::read(&output_path).expect("read detached shutdown output");
    let stream = parse_stream(&stdout, false);
    assert_eq!(stream.blocks.len(), 4, "{stream:?}");
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    assert_block(&stream.blocks[2], 3, 0, &[], false);
    assert_block(&stream.blocks[3], 4, 0, &["AFTER"], false);
    let lines = std::str::from_utf8(&stdout)
        .expect("UTF-8 detached shutdown output")
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(lines.iter().filter(|line| **line == "%exit").count(), 1);
    assert_eq!(lines.last().copied(), Some("%exit"));

    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", session, "exec /bin/cat"])
            .status
            .success()
    );
    let scenarios = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../compat/scenarios");
    let root = scenarios.join("source-file-output-hook-root.conf");
    let child = scenarios.join("source-file-output-child.conf");
    let hook = format!("source-file '{}'", child.display());
    assert!(
        fixture
            .run(&["set-hook", "-g", "after-display-message", hook.as_str()])
            .status
            .success()
    );
    let output_path = fixture._directory.path().join("nested-control.raw");
    let (child, mut stdin) = spawn_control_to_file(
        &fixture,
        &["-C", "attach-session", "-t", &format!("={session}")],
        &output_path,
    );
    writeln!(
        stdin,
        "run-shell -bC 'kill-server ; source-file \"{}\" ; display-message -p OUTER'",
        root.display()
    )
    .expect("write nested detached shutdown command");
    stdin
        .flush()
        .expect("flush nested detached shutdown command");

    let output = collect_control_process(child, Some(stdin), "nested detached shutdown callback");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = std::fs::read(&output_path).expect("read nested detached shutdown output");
    let stream = parse_stream_allow_gaps(&stdout, false);
    assert_eq!(stream.blocks.len(), 7, "{stream:?}");
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    assert_block(&stream.blocks[2], 3, 0, &[], false);
    assert_block(&stream.blocks[3], 4, 0, &[], false);
    assert_block(&stream.blocks[4], 5, 0, &["HOOK_TRIGGER"], false);
    assert_block(&stream.blocks[5], 6, 0, &[], false);
    assert_block(&stream.blocks[6], 9, 0, &["OUTER"], false);
    let payloads = stream
        .blocks
        .iter()
        .flat_map(|block| block.payload.iter().map(String::as_str))
        .collect::<Vec<_>>();
    for suppressed in ["HOOK_LATER", "CHILD_ONE", "CHILD_TWO"] {
        assert!(!payloads.contains(&suppressed), "{stream:?}");
    }
    let lines = std::str::from_utf8(&stdout)
        .expect("UTF-8 nested detached shutdown output")
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(lines.iter().filter(|line| **line == "%exit").count(), 1);
    assert_eq!(lines.last().copied(), Some("%exit"));
}

#[test]
fn control_background_malformed_lists_and_shell_jobs_stay_unframed() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let session = "control-background-silent";
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", session, "exec /bin/cat"])
            .status
            .success()
    );
    let directory = fixture._directory.path().join("control background silent");
    std::fs::create_dir(&directory).expect("create silent background directory");
    let finished = directory.join("condition.finished");
    let output_path = directory.join("control.raw");
    let (mut child, mut stdin) = spawn_control_to_file(
        &fixture,
        &["-C", "attach-session", "-t", &format!("={session}")],
        &output_path,
    );
    writeln!(
        stdin,
        "if-shell -b 'sleep 0.1; touch \"{}\"; true' 'if -x {{'",
        finished.display()
    )
    .expect("write malformed background command");
    writeln!(stdin, "run-shell -bC -d 0.15 'if -x {{'")
        .expect("write malformed background run-shell command");
    writeln!(stdin, "run-shell -b 'sleep 0.05; printf ordinary'")
        .expect("write ordinary background shell command");
    writeln!(stdin, "display-message -p MALFORMED_LATER").expect("write malformed later command");
    stdin.flush().expect("flush malformed commands");

    let deadline = Instant::now() + Duration::from_secs(10);
    while !finished.exists() {
        if let Some(status) = child.try_wait().expect("poll malformed process") {
            panic!("control process exited before malformed callback: {status}");
        }
        assert!(
            Instant::now() < deadline,
            "malformed background condition did not finish"
        );
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(200));
    writeln!(stdin, "display-message -p MALFORMED_DONE")
        .expect("write malformed completion command");
    stdin.flush().expect("flush malformed completion command");
    wait_for_control_output_marker(
        &output_path,
        "MALFORMED_DONE",
        &mut child,
        "malformed completion frame",
    );
    writeln!(stdin).expect("write malformed return");
    stdin.flush().expect("flush malformed return");
    let output = collect_control_process(child, Some(stdin), "malformed background list");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = std::fs::read(&output_path).expect("read malformed control output");
    let stream = parse_stream(&stdout, false);
    assert_eq!(stream.blocks.len(), 6, "{stream:?}");
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    assert_block(&stream.blocks[2], 3, 1, &[], false);
    assert_block(&stream.blocks[3], 4, 1, &[], false);
    assert_block(&stream.blocks[4], 5, 1, &["MALFORMED_LATER"], false);
    assert_block(&stream.blocks[5], 6, 1, &["MALFORMED_DONE"], false);
    assert!(!stream.outside.iter().any(|line| line == "ordinary"));
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line == &format!("%session-changed $0 {session}"))
    );
    assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
}

#[test]
fn control_disconnect_cancels_background_inserted_side_effects() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let session = "control-background-disconnect";
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", session, "exec /bin/cat"])
            .status
            .success()
    );
    let wait_marker = fixture._directory.path().join("disconnect.waited");
    assert!(
        fixture
            .run(&[
                "run-shell",
                "-b",
                &format!("sleep 0.8; touch '{}'", wait_marker.display()),
            ])
            .status
            .success()
    );
    let output_path = fixture._directory.path().join("disconnect.raw");
    let (mut child, mut stdin) = spawn_control_to_file(
        &fixture,
        &["-C", "attach-session", "-t", &format!("={session}")],
        &output_path,
    );
    writeln!(
        stdin,
        "run-shell -bC -d 0.5 'set-option -g @disconnected-run yes'"
    )
    .expect("write disconnected run-shell command");
    writeln!(
        stdin,
        "if-shell -b 'sleep 0.5; true' 'set-option -g @disconnected-if yes'"
    )
    .expect("write disconnected if-shell command");
    writeln!(stdin, "display-message -p DISCONNECT_READY")
        .expect("write disconnect readiness command");
    stdin.flush().expect("flush disconnect commands");
    wait_for_control_output_marker(
        &output_path,
        "DISCONNECT_READY",
        &mut child,
        "disconnect readiness frame",
    );
    drop(stdin);
    let output = collect_control_process(child, None, "background disconnect");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = std::fs::read(&output_path).expect("read disconnect control output");
    let stream = parse_stream(&stdout, false);
    assert_eq!(stream.blocks.len(), 4, "{stream:?}");
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    assert_block(&stream.blocks[2], 3, 1, &[], false);
    assert_block(&stream.blocks[3], 4, 1, &["DISCONNECT_READY"], false);
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line == &format!("%session-changed $0 {session}"))
    );
    assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));

    let deadline = Instant::now() + Duration::from_secs(10);
    while !wait_marker.exists() {
        assert!(
            Instant::now() < deadline,
            "disconnect wait marker did not arrive"
        );
        thread::sleep(Duration::from_millis(10));
    }
    for option in ["@disconnected-run", "@disconnected-if"] {
        let shown = fixture.run(&["show-options", "-gqv", option]);
        assert_eq!(shown.status.code(), Some(0), "{option}");
        assert!(shown.stdout.is_empty(), "{option}");
        assert!(shown.stderr.is_empty(), "{option}");
    }
}

#[test]
fn control_command_hook_frames_are_inserted_and_retain_failure_status() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let session = "control-hook-frames";
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", session, "exec /bin/cat"])
            .status
            .success()
    );
    let directory = fixture._directory.path().join("control hook frames");
    std::fs::create_dir(&directory).expect("create hook frame directory");
    let hook_source = write_source(
        &directory,
        "hook.conf",
        "display-message -p HOOK_CHILD\n\
         kill-session -t hook-runtime-missing\n\
         display-message -p HOOK_AFTER_ERROR\n",
    );
    assert!(
        fixture
            .run(&[
                "set-hook",
                "-g",
                "after-display-message[0]",
                &format!("source-file '{hook_source}'"),
            ])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&[
                "set-hook",
                "-g",
                "after-display-message[1]",
                "display-message -p HOOK_LATER_ARRAY",
            ])
            .status
            .success()
    );

    let marker = directory.join("complete");
    let output = run_control_until_return(
        &fixture,
        &["-C", "attach-session", "-t", &format!("={session}")],
        "display-message -p TRIGGER\n\
         set-hook -gu after-display-message\n\
         display-message -p LATER_FLAGS_ONE\n",
        &marker,
        "control command hook frames",
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(output.stderr.is_empty());
    let stream = parse_stream_allow_gaps(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 10, "{stream:?}");
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &["TRIGGER"], false);
    assert_block(&stream.blocks[2], 3, 0, &[], false);
    assert_block(&stream.blocks[3], 4, 0, &["HOOK_CHILD"], false);
    assert_block(
        &stream.blocks[4],
        5,
        0,
        &["can't find session: hook-runtime-missing"],
        true,
    );
    assert_block(&stream.blocks[5], 6, 0, &["HOOK_AFTER_ERROR"], false);
    assert_block(&stream.blocks[6], 8, 0, &["HOOK_LATER_ARRAY"], false);
    assert_block(&stream.blocks[7], 9, 1, &[], false);
    assert_block(&stream.blocks[8], 10, 1, &["LATER_FLAGS_ONE"], false);
    assert_block(&stream.blocks[9], 11, 1, &[], false);
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line == &format!("%session-changed $0 {session}"))
    );
    assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
}

#[test]
fn control_hook_source_diagnostics_keep_trigger_frames_and_status() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let session = "control-hook-source-diagnostics";
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", session, "exec /bin/cat"])
            .status
            .success()
    );
    let directory = fixture
        ._directory
        .path()
        .join("control hook source diagnostics");
    std::fs::create_dir(&directory).expect("create hook source diagnostic directory");
    let hit = write_source(&directory, "hit.conf", "display-message -p MIXED_HIT\n");
    let missing = directory.join("missing.conf");
    assert!(
        fixture
            .run(&[
                "set-hook",
                "-g",
                "after-display-message",
                &format!("source-file '{}' '{hit}'", missing.display()),
            ])
            .status
            .success()
    );

    let mixed_marker = directory.join("mixed-complete");
    let mixed = run_control_until_return(
        &fixture,
        &["-C", "attach-session", "-t", &format!("={session}")],
        "display-message -p MIXED_TRIGGER\n\
         set-hook -gu after-display-message\n\
         display-message -p MIXED_LATER\n",
        &mixed_marker,
        "control mixed hook source",
    );
    assert_eq!(mixed.status.code(), Some(1));
    assert!(mixed.stderr.is_empty());
    let mixed = parse_stream_allow_gaps(&mixed.stdout, false);
    assert_eq!(mixed.blocks.len(), 7, "{mixed:?}");
    assert_block(&mixed.blocks[0], 1, 0, &[], false);
    assert_block(&mixed.blocks[1], 2, 1, &["MIXED_TRIGGER"], false);
    assert_block(
        &mixed.blocks[2],
        3,
        0,
        &[&format!("No such file or directory: {}", missing.display())],
        false,
    );
    assert_block(&mixed.blocks[3], 4, 0, &["MIXED_HIT"], false);
    assert_block(&mixed.blocks[4], 6, 1, &[], false);
    assert_block(&mixed.blocks[5], 7, 1, &["MIXED_LATER"], false);
    assert_block(&mixed.blocks[6], 8, 1, &[], false);

    assert!(
        fixture
            .run(&[
                "set-hook",
                "-g",
                "after-display-message",
                &format!("source-file '{}'", directory.display()),
            ])
            .status
            .success()
    );
    let read_error =
        std::fs::read_to_string(&directory).expect_err("source directory must fail as a file read");
    let read_error = format!("{}: {}", os_error_text(&read_error), directory.display());
    let read_marker = directory.join("read-complete");
    let read = run_control_until_return(
        &fixture,
        &["-C", "attach-session", "-t", &format!("={session}")],
        "display-message -p READ_TRIGGER\n\
         set-hook -gu after-display-message\n\
         display-message -p READ_LATER\n",
        &read_marker,
        "control hook source read error",
    );
    assert_eq!(read.status.code(), Some(1));
    assert!(read.stderr.is_empty());
    let read_lines = std::str::from_utf8(&read.stdout)
        .expect("UTF-8 hook source read output")
        .lines()
        .collect::<Vec<_>>();
    let read_error_index = read_lines
        .iter()
        .position(|line| line == &read_error)
        .expect("raw hook source read error");
    let before = next_block_guard(read_lines[..read_error_index].iter().rev().copied());
    assert!(
        before.starts_with("%end ") && before.ends_with(" 3 0"),
        "{read_lines:?}"
    );
    let after = next_block_guard(read_lines[read_error_index + 1..].iter().copied());
    assert!(
        after.starts_with("%begin ") && after.ends_with(" 5 1"),
        "{read_lines:?}"
    );
    let read = parse_stream_allow_gaps(&read.stdout, false);
    assert_eq!(read.blocks.len(), 6, "{read:?}");
    assert_block(&read.blocks[0], 1, 0, &[], false);
    assert_block(&read.blocks[1], 2, 1, &["READ_TRIGGER"], false);
    assert_block(&read.blocks[2], 3, 0, &[], false);
    assert_block(&read.blocks[3], 5, 1, &[], false);
    assert_block(&read.blocks[4], 6, 1, &["READ_LATER"], false);
    assert_block(&read.blocks[5], 7, 1, &[], false);
    assert!(read.outside.contains(&read_error));
}

#[test]
fn control_kill_server_closes_its_block_before_exit() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let output = fixture.run_with_stdin(&["-C", "new-session", "-s", "stopping"], b"kill-server\n");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 2);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    assert_attached_startup(&stream.outside, "stopping");
    assert_eq!(
        stream
            .outside
            .iter()
            .filter(|line| line.as_str() == "%exit")
            .count(),
        1
    );
    assert!(
        !stream
            .outside
            .iter()
            .any(|line| line.contains("server exited unexpectedly"))
    );
}
