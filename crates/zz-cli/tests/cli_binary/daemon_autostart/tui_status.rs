use super::*;

#[test]
fn attached_tui_renders_daemon_authored_styled_status_labels() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    for arguments in [
        ["new-session", "-d", "-s", "styled", "-n", "main"].as_slice(),
        ["set", "-t", "styled", "status-left-length", "40"].as_slice(),
        ["set", "-t", "styled", "status-left", "#[fg=red,bold]LEFT"].as_slice(),
        ["set", "-t", "styled", "status-right", "#[bg=blue]RIGHT"].as_slice(),
        [
            "setw",
            "-t",
            "styled",
            "window-status-current-format",
            "#[underscore]CUSTOM",
        ]
        .as_slice(),
    ] {
        let output = fixture.run(arguments);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let Ok((mut master, slave)) = open_pty_sized(WIDE_COLUMNS) else {
        return;
    };
    rustix::io::ioctl_fionbio(&master, true).expect("set pty master nonblocking");
    let stdin = slave.try_clone().expect("clone pty stdin");
    let stdout = slave.try_clone().expect("clone pty stdout");
    let mut child = fixture
        .command()
        .args(["attach-session", "-t", "styled"])
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(slave))
        .spawn()
        .expect("spawn styled TUI attach");

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut captured = Vec::new();
    let mut early_status = None;
    let rendered = loop {
        let mut buffer = [0_u8; 4096];
        match master.read(&mut buffer) {
            Ok(0) => {}
            Ok(count) => captured.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => {}
        }
        if [
            b"LEFT".as_slice(),
            b"RIGHT".as_slice(),
            b"CUSTOM".as_slice(),
        ]
        .into_iter()
        .all(|needle| {
            captured
                .windows(needle.len())
                .any(|window| window == needle)
        }) {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        if let Some(status) = child.try_wait().expect("poll styled TUI attach") {
            early_status = Some(status);
            break false;
        }
        thread::sleep(Duration::from_millis(5));
    };

    let _ = child.kill();
    let _ = child.wait();
    drop(master);

    assert!(
        rendered,
        "child exited early={early_status:?}; pty output={}",
        String::from_utf8_lossy(&captured),
    );
    assert!(!captured.windows(2).any(|window| window == b"#["));
    assert!(!captured.windows(6).any(|window| window == b"0:main"));
}

#[test]
fn stock_eighty_column_attach_draws_the_whole_status_row() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    for arguments in [
        ["new-session", "-d", "-s", "stock", "-n", "main"].as_slice(),
        ["set", "-t", "stock", "status-left", "LEFT"].as_slice(),
        ["set", "-t", "stock", "status-right", "RIGHT"].as_slice(),
        [
            "setw",
            "-t",
            "stock",
            "window-status-current-format",
            "CUSTOM",
        ]
        .as_slice(),
    ] {
        let output = fixture.run(arguments);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let Ok((mut master, slave)) = open_pty_sized(STOCK_COLUMNS) else {
        return;
    };
    rustix::io::ioctl_fionbio(&master, true).expect("set pty master nonblocking");
    let stdin = slave.try_clone().expect("clone pty stdin");
    let stdout = slave.try_clone().expect("clone pty stdout");
    let mut child = fixture
        .command()
        .args(["attach-session", "-t", "stock"])
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(slave))
        .spawn()
        .expect("spawn stock TUI attach");

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut captured = Vec::new();
    let mut early_status = None;
    let mut pane_rows = Vec::new();
    let rendered = loop {
        let mut buffer = [0_u8; 4096];
        match master.read(&mut buffer) {
            Ok(0) => {}
            Ok(count) => captured.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => {}
        }
        let text = String::from_utf8_lossy(&captured).into_owned();
        if let Some(left) = text.find("LEFT")
            && let Some(custom) = text[left..].find("CUSTOM")
            && text[left + custom..].contains("RIGHT")
        {
            pane_rows = fixture
                .run(&["display-message", "-p", "-t", "stock", "#{pane_height}"])
                .stdout;
            if pane_rows == format!("{STOCK_PANE_ROWS}\n").into_bytes() {
                break true;
            }
        }
        if Instant::now() >= deadline {
            break false;
        }
        if let Some(status) = child.try_wait().expect("poll stock TUI attach") {
            early_status = Some(status);
            break false;
        }
        thread::sleep(Duration::from_millis(5));
    };

    let _ = child.kill();
    let _ = child.wait();
    drop(master);

    assert!(
        rendered,
        "child exited early={early_status:?}; pane_height={}; pty output={}",
        String::from_utf8_lossy(&pane_rows),
        String::from_utf8_lossy(&captured),
    );
}

fn capture_tui_until_wide(
    fixture: &Fixture,
    attach: &[&str],
    needles: &[&[u8]],
) -> (bool, Vec<u8>, Option<std::process::ExitStatus>) {
    let mut command = fixture.command();
    command.args(attach);
    capture_command_until(command, needles, WIDE_COLUMNS)
}

fn visible_text_after(captured: &[u8], cursor: &[u8]) -> Vec<String> {
    let mut collected = Vec::new();
    let mut search = 0;
    while let Some(offset) = captured[search..]
        .windows(cursor.len())
        .position(|window| window == cursor)
    {
        let mut index = search + offset + cursor.len();
        search = index;
        let mut text = Vec::new();
        while index < captured.len() && text.len() < 24 {
            if captured[index] == 0x1b {
                let Some(end) = captured[index..].iter().position(u8::is_ascii_alphabetic) else {
                    break;
                };
                if captured[index + end] == b'H' {
                    break;
                }
                index += end + 1;
                continue;
            }
            text.push(captured[index]);
            index += 1;
        }
        collected.push(String::from_utf8_lossy(&text).into_owned());
    }
    collected
}

#[test]
fn styled_multi_row_status_renders_two_rows_without_literal_markers() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    for arguments in [
        ["new-session", "-d", "-s", "multirow", "-n", "main"].as_slice(),
        ["set", "-g", "status", "2"].as_slice(),
        ["set", "-g", "status-format[1]", "#[fg=red,bold]ROWTWO"].as_slice(),
    ] {
        let output = fixture.run(arguments);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let (rendered, captured, early_status) = capture_tui_until_wide(
        &fixture,
        &["attach-session", "-t", "multirow"],
        &[b"[multirow]", b"ROWTWO"],
    );
    assert!(
        rendered,
        "child exited early={early_status:?}; pty output={}",
        String::from_utf8_lossy(&captured),
    );
    assert!(!captured.windows(2).any(|window| window == b"#["));
    for row in [b"\x1b[23;1H".as_slice(), b"\x1b[24;1H".as_slice()] {
        assert!(
            captured.windows(row.len()).any(|window| window == row),
            "both status rows paint above the last line: {}",
            String::from_utf8_lossy(&captured),
        );
    }
    assert!(
        visible_text_after(&captured, b"\x1b[24;1H")
            .iter()
            .any(|text| text.starts_with("ROWTWO")),
        "row 1 carries the second status-format row: {}",
        String::from_utf8_lossy(&captured),
    );
}

#[test]
fn status_position_top_puts_the_status_block_at_row_zero() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    for arguments in [
        ["new-session", "-d", "-s", "toppos", "-n", "main"].as_slice(),
        ["set", "-t", "toppos", "status-position", "top"].as_slice(),
        ["set", "-t", "toppos", "status-left", "TOPMARK"].as_slice(),
    ] {
        let output = fixture.run(arguments);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let (rendered, captured, early_status) =
        capture_tui_until_wide(&fixture, &["attach-session", "-t", "toppos"], &[b"TOPMARK"]);
    assert!(
        rendered,
        "child exited early={early_status:?}; pty output={}",
        String::from_utf8_lossy(&captured),
    );
    assert!(!captured.windows(2).any(|window| window == b"#["));
    assert!(
        visible_text_after(&captured, b"\x1b[1;1H")
            .iter()
            .any(|text| text.starts_with("TOPMARK")),
        "the status block owns row zero of the main columns: {}",
        String::from_utf8_lossy(&captured),
    );
}

#[test]
fn chain_error_after_attachment_is_rendered_inside_the_tui() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let Ok((mut master, slave)) = open_pty() else {
        return;
    };
    rustix::io::ioctl_fionbio(&master, true).expect("set pty master nonblocking");
    let stdin = slave.try_clone().expect("clone pty stdin");
    let stdout = slave.try_clone().expect("clone pty stdout");
    let mut child = fixture
        .command()
        .args([
            "new-session",
            "-s",
            "pty-error",
            ";",
            "split-window",
            "-t",
            ":99",
        ])
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(slave))
        .spawn()
        .expect("spawn attaching chain error");

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut captured = Vec::new();
    let mut early_status = None;
    let rendered_error = loop {
        let mut buffer = [0_u8; 4096];
        match master.read(&mut buffer) {
            Ok(0) => {}
            Ok(count) => captured.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => {}
        }
        let alternate_screen = captured
            .windows(b"\x1b[?1049h".len())
            .any(|window| window == b"\x1b[?1049h");
        let error_text = captured
            .windows(b"can't find win".len())
            .any(|window| window == b"can't find win");
        if alternate_screen && error_text {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                early_status = Some(status);
                break false;
            }
            Ok(None) => {}
            Err(_) => break false,
        }
        thread::sleep(Duration::from_millis(5));
    };
    let still_running = matches!(child.try_wait(), Ok(None));
    let _ = child.kill();
    let _ = child.wait();
    drop(master);

    assert!(
        rendered_error,
        "child exited early={early_status:?}; pty output={}",
        String::from_utf8_lossy(&captured),
    );
    assert!(still_running, "TUI exited after rendering the chain error");
    let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(listed.status.code(), Some(0));
    assert_eq!(listed.stdout, b"pty-error\n");
    assert!(listed.stderr.is_empty());
}
