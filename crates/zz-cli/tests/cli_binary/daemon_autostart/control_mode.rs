use super::*;

#[derive(Debug)]
struct Block {
    time: u64,
    number: u64,
    flags: u8,
    payload: Vec<String>,
    error: bool,
}

#[derive(Debug)]
struct Stream {
    blocks: Vec<Block>,
    outside: Vec<String>,
}

fn marker(line: &str, expected: &str) -> Option<(u64, u64, u8)> {
    let mut fields = line.split(' ');
    if fields.next()? != expected {
        return None;
    }
    let time = fields.next()?.parse().ok()?;
    let number = fields.next()?.parse().ok()?;
    let flags = fields.next()?.parse().ok()?;
    fields.next().is_none().then_some((time, number, flags))
}

fn parse_stream_impl(output: &[u8], double: bool, contiguous: bool) -> Stream {
    let output = if double {
        assert!(output.starts_with(b"\x1bP1000p"));
        assert!(output.ends_with(b"\x1b\\"));
        &output[b"\x1bP1000p".len()..output.len() - b"\x1b\\".len()]
    } else {
        assert!(!output.starts_with(b"\x1bP1000p"));
        assert!(!output.ends_with(b"\x1b\\"));
        output
    };
    let text = std::str::from_utf8(output).expect("UTF-8 control output");
    let text = text.strip_suffix('\n').unwrap_or(text);
    let lines = text.split('\n').collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut outside = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let Some((time, number, flags)) = marker(lines[index], "%begin") else {
            outside.push(lines[index].to_owned());
            index += 1;
            continue;
        };
        index += 1;
        let mut payload = Vec::new();
        let error = loop {
            assert!(index < lines.len(), "unterminated block {number}");
            if let Some(end) = marker(lines[index], "%end") {
                assert_eq!(end, (time, number, flags));
                index += 1;
                break false;
            }
            if let Some(end) = marker(lines[index], "%error") {
                assert_eq!(end, (time, number, flags));
                index += 1;
                break true;
            }
            payload.push(lines[index].to_owned());
            index += 1;
        };
        blocks.push(Block {
            time,
            number,
            flags,
            payload,
            error,
        });
    }
    for (index, block) in blocks.iter().enumerate() {
        if contiguous {
            assert_eq!(block.number, index as u64 + 1);
        }
        assert!(block.time > 0);
    }
    Stream { blocks, outside }
}

fn parse_stream(output: &[u8], double: bool) -> Stream {
    parse_stream_impl(output, double, true)
}

fn parse_stream_allow_gaps(output: &[u8], double: bool) -> Stream {
    parse_stream_impl(output, double, false)
}

fn os_error_text(error: &std::io::Error) -> String {
    let message = error.to_string();
    error.raw_os_error().map_or(message.clone(), |code| {
        message
            .strip_suffix(&format!(" (os error {code})"))
            .unwrap_or(&message)
            .to_owned()
    })
}

fn assert_block(block: &Block, number: u64, flags: u8, payload: &[&str], error: bool) {
    assert_eq!(block.number, number);
    assert_eq!(block.flags, flags);
    assert_eq!(
        block.payload,
        payload
            .iter()
            .map(|line| (*line).to_owned())
            .collect::<Vec<_>>()
    );
    assert_eq!(block.error, error);
}

fn next_block_guard<'a>(lines: impl Iterator<Item = &'a str>) -> &'a str {
    for line in lines {
        if line.starts_with("%begin ") || line.starts_with("%end ") || line.starts_with("%error ") {
            return line;
        }
        assert!(
            line.starts_with('%'),
            "raw line {line:?} between the error and its guard"
        );
    }
    panic!("no block guard next to the raw error");
}

fn assert_attached_startup(outside: &[String], name: &str) {
    let settled = outside
        .iter()
        .filter(|line| !line.starts_with("%window-renamed @0 ") && !line.starts_with("%output %0 "))
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_eq!(
        settled,
        [
            "%window-add @0",
            "%sessions-changed",
            &format!("%session-changed $0 {name}"),
            "%exit",
        ]
    );
}

fn wait_for_control_marker(path: &Path, child: &mut Child, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        if let Some(status) = child.try_wait().expect("poll control marker process") {
            panic!("control process exited before {label}: {status}");
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill stalled control marker process");
            panic!("control process did not reach {label}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_control_output_marker(path: &Path, marker: &str, child: &mut Child, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let output = std::fs::read(path).expect("read live control output");
        let mut marker_seen = false;
        let completed = String::from_utf8_lossy(&output).lines().any(|line| {
            if line == marker {
                marker_seen = true;
                false
            } else {
                marker_seen && line.starts_with("%end ")
            }
        });
        if completed {
            return;
        }
        if let Some(status) = child.try_wait().expect("poll control block process") {
            panic!(
                "control process exited before {label}: {status}: {}",
                String::from_utf8_lossy(&output)
            );
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill stalled control block process");
            panic!(
                "control process did not reach {label}: {}",
                String::from_utf8_lossy(&output)
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_control_error_marker(path: &Path, marker: &str, child: &mut Child, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let output = std::fs::read(path).expect("read live control output");
        let mut marker_seen = false;
        let completed = String::from_utf8_lossy(&output).lines().any(|line| {
            if line == marker {
                marker_seen = true;
                false
            } else {
                marker_seen && line.starts_with("%error ")
            }
        });
        if completed {
            return;
        }
        if let Some(status) = child.try_wait().expect("poll control error process") {
            panic!("control process exited before {label}: {status}");
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill stalled control error process");
            panic!("control process did not reach {label}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn spawn_control_to_file(
    fixture: &Fixture,
    arguments: &[&str],
    output_path: &Path,
) -> (Child, ChildStdin) {
    let output_file = File::create(output_path).expect("create live control output");
    let mut child = fixture
        .command()
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(output_file))
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn control with live output");
    let stdin = child.stdin.take().expect("piped control stdin");
    (child, stdin)
}

fn wait_for_control_clients(fixture: &Fixture, expected: usize, label: &str) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let listed = fixture.run(&[
            "-u",
            "list-clients",
            "-F",
            "#{client_name}\t#{client_flags}",
        ]);
        let names = String::from_utf8_lossy(&listed.stdout)
            .lines()
            .filter_map(|line| {
                let (name, flags) = line.split_once('\t')?;
                flags.contains("control-mode").then(|| name.to_owned())
            })
            .collect::<Vec<_>>();
        if listed.status.success() && names.len() == expected {
            return names;
        }
        assert!(
            Instant::now() < deadline,
            "control clients did not settle for {label}: status={} stdout={} stderr={}",
            listed.status,
            String::from_utf8_lossy(&listed.stdout),
            String::from_utf8_lossy(&listed.stderr)
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn prime_control_return_code(
    child: &mut Child,
    stdin: &mut ChildStdin,
    output_path: &Path,
    marker: &str,
    missing: &str,
) {
    writeln!(stdin, "kill-session -t {missing}").expect("write retval failure");
    writeln!(stdin, "display-message -p {marker}").expect("write retval marker");
    stdin.flush().expect("flush retval commands");
    wait_for_control_output_marker(output_path, marker, child, marker);
}

fn collect_control_process(mut child: Child, stdin: Option<ChildStdin>, label: &str) -> Output {
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll control process") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill stalled control process");
            panic!("control process did not exit for {label}");
        }
        thread::sleep(Duration::from_millis(10));
    };
    drop(stdin);
    let output = child.wait_with_output().expect("collect control output");
    assert_eq!(output.status, status, "{label}");
    output
}

fn terminate_control_process(mut child: Child, stdin: ChildStdin, label: &str) -> Output {
    let pid = i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .expect("control process PID");
    let started = Instant::now();
    if let Err(error) = rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("send SIGTERM for {label}: {error}");
    }
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll signalled control process") {
            break status;
        }
        if started.elapsed() >= Duration::from_secs(2) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("control process did not exit promptly after SIGTERM for {label}");
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert!(started.elapsed() < Duration::from_secs(2), "{label}");
    drop(stdin);
    let output = child
        .wait_with_output()
        .expect("collect signalled control output");
    assert_eq!(output.status, status, "{label}");
    output
}

fn run_control_until_return(
    fixture: &Fixture,
    arguments: &[&str],
    input: &str,
    marker: &Path,
    label: &str,
) -> Output {
    let (mut child, mut stdin) = fixture.spawn_with_open_stdin(arguments);
    stdin
        .write_all(input.as_bytes())
        .expect("write control commands");
    if !input.ends_with('\n') {
        writeln!(stdin).expect("terminate control command line");
    }
    writeln!(stdin, "run-shell 'touch \"{}\"'", marker.display())
        .expect("write control completion marker");
    stdin.flush().expect("flush control commands");
    wait_for_control_marker(marker, &mut child, label);
    writeln!(stdin).expect("write control return");
    stdin.flush().expect("flush control return");
    collect_control_process(child, Some(stdin), label)
}

#[test]
fn control_read_only_connect_failure_has_no_framing_or_exit() {
    let fixture = Fixture::new();
    let output = fixture.run(&["-C", "ls"]);
    assert_missing(&output, &fixture.missing_message());
    fixture.assert_not_started();
}

#[test]
fn control_list_commands_autostarts_and_frames_the_initial_command() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let output = fixture.run(&["-C", "list-commands"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 1);
    assert_eq!(stream.blocks[0].flags, 0);
    assert!(!stream.blocks[0].payload.is_empty());
    assert!(!stream.blocks[0].error);
    assert_eq!(stream.outside, ["%exit"]);
    assert!(fixture.socket.exists());
}

#[test]
fn detached_control_new_session_exits_without_reading_stdin() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let (mut child, stdin) = fixture.spawn_with_open_stdin(&["-C", "new-session", "-d", "-s", "x"]);
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll control process") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill stalled control process");
            panic!("detached control client read stdin");
        }
        thread::sleep(Duration::from_millis(10));
    };
    drop(stdin);
    let output = child.wait_with_output().expect("collect control output");
    assert_eq!(output.status, status);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 1);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_eq!(stream.outside, ["%exit"]);
}

#[test]
fn control_ls_with_a_daemon_frames_session_output() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "listed"]);
    assert_eq!(created.status.code(), Some(0));
    let output = fixture.run(&["-C", "ls"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 1);
    assert_eq!(stream.blocks[0].flags, 0);
    assert!(!stream.blocks[0].error);
    assert!(stream.blocks[0].payload[0].starts_with("listed:"));
    assert_eq!(stream.outside, ["%exit"]);
}

#[test]
fn control_initial_command_uses_a_live_daemon_alias() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "aliased"])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&[
                "set-option",
                "-s",
                "command-alias[40]",
                "live=list-sessions -F 'alias-#{session_name}'",
            ])
            .status
            .success()
    );
    let output = fixture.run(&["-C", "live"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 1);
    assert_block(&stream.blocks[0], 1, 0, &["alias-aliased"], false);
    assert_eq!(stream.outside, ["%exit"]);
}

#[test]
fn control_stdin_prepares_each_complete_alias_chain_once() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "alias-chain"])
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
    let marker = fixture._directory.path().join("alias-chain.complete");
    let output = run_control_until_return(
        &fixture,
        &["-C", "attach-session", "-t", "alias-chain"],
        "set-option -s command-alias[40] 'live=display-message -p new' ; live\nlive\n",
        &marker,
        "alias chain completion",
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 5);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    assert_block(&stream.blocks[2], 3, 1, &["old"], false);
    assert_block(&stream.blocks[3], 4, 1, &["new"], false);
    assert_block(&stream.blocks[4], 5, 1, &[], false);
    assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
}

#[test]
fn control_unknown_initial_command_is_bare_after_connect() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d"]);
    assert_eq!(created.status.code(), Some(0));
    let output = fixture.run(&["-C", "frobnicate"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert!(stream.blocks.is_empty());
    assert_eq!(stream.outside, ["unknown command: frobnicate", "%exit"]);
}

#[test]
fn control_command_error_uses_the_inner_server_message() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let output = fixture.run(&["-C", "list-commands", "bogus-command"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 1);
    assert_block(
        &stream.blocks[0],
        1,
        0,
        &["unknown command: bogus-command"],
        true,
    );
    assert_eq!(stream.outside, ["%exit"]);
}

#[test]
fn attached_control_executes_stdin_until_empty_line() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let output = fixture.run_with_stdin(&["-C", "new-session", "-s", "attached"], b"ls\n\n");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 2);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_eq!(stream.blocks[1].flags, 1);
    assert!(!stream.blocks[1].error);
    assert!(stream.blocks[1].payload[0].starts_with("attached:"));
    assert!(stream.blocks[1].payload[0].ends_with(" (attached)"));
    assert_attached_startup(&stream.outside, "attached");
}

#[test]
fn control_chains_frame_each_command_and_abort_only_the_failing_line() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let complete = fixture._directory.path().join("chain.complete");
    let (mut child, mut stdin) =
        fixture.spawn_with_open_stdin(&["-C", "new-session", "-s", "chain"]);
    writeln!(stdin, "display-message -p one ; display-message -p two")
        .expect("write successful chain");
    writeln!(stdin, "kill-session -t nosuch ; display-message -p skipped")
        .expect("write failing chain");
    writeln!(stdin, "display-message -p fresh").expect("write fresh command");
    writeln!(stdin, "run-shell 'touch \"{}\"'", complete.display())
        .expect("write chain completion marker");
    stdin.flush().expect("flush chain commands");
    wait_for_control_marker(&complete, &mut child, "chain completion");
    writeln!(stdin).expect("write chain return");
    stdin.flush().expect("flush chain return");
    let output = collect_control_process(child, Some(stdin), "chain return");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 6);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &["one"], false);
    assert_block(&stream.blocks[2], 3, 1, &["two"], false);
    assert_block(
        &stream.blocks[3],
        4,
        1,
        &["can't find session: nosuch"],
        true,
    );
    assert_block(&stream.blocks[4], 5, 1, &["fresh"], false);
    assert_block(&stream.blocks[5], 6, 1, &[], false);
    assert_attached_startup(&stream.outside, "chain");
}

fn control_handoff_transcript(relay: Option<&str>) -> Option<(Vec<String>, Option<i32>)> {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return None;
    }
    let mut command = fixture.command();
    if let Some(relay) = relay {
        command.env("ZZ_CONTROL_RELAY", relay);
    }
    let mut child = command
        .args([
            "-C",
            "new-session",
            "-s",
            "handoff",
            "-n",
            "fixed",
            "exec /bin/cat",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn control client");
    let mut stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let (lines, received) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in std::io::BufRead::lines(std::io::BufReader::new(stdout)) {
            let Ok(line) = line else {
                break;
            };
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    let mut transcript = Vec::new();
    let mut read_until = |prefixes: &[&str]| loop {
        let line = received
            .recv_timeout(Duration::from_secs(10))
            .expect("control output line");
        let mut fields = line.split(' ').collect::<Vec<_>>();
        if fields.len() == 4 && matches!(fields[0], "%begin" | "%end" | "%error") {
            fields[1] = "T";
        }
        if !line.starts_with("%window-renamed ") && !line.starts_with("%output ") {
            transcript.push(fields.join(" "));
        }
        if prefixes.iter().any(|prefix| line.starts_with(prefix)) {
            break;
        }
    };
    read_until(&["%session-changed "]);
    for line in [
        "display-message -p A",
        "bogus-command",
        "display-message -p B",
        "list-windows -F '#{window_index}'",
    ] {
        writeln!(stdin, "{line}").expect("write control line");
        stdin.flush().expect("flush control line");
        read_until(&["%end ", "%error "]);
    }
    writeln!(stdin).expect("write control return");
    stdin.flush().expect("flush control return");
    read_until(&["%exit"]);
    drop(stdin);
    let status = child.wait().expect("wait for control client");
    reader.join().expect("join control reader");
    Some((transcript, status.code()))
}

#[test]
fn control_stdio_handoff_frames_like_the_relay_rollback() {
    let Some(handoff) = control_handoff_transcript(None) else {
        return;
    };
    let Some(relay) = control_handoff_transcript(Some("1")) else {
        return;
    };
    assert_eq!(handoff, relay);
    assert!(handoff.0.iter().any(|line| line == "A"));
    assert!(handoff.0.iter().any(|line| line == "B"));
    assert!(
        handoff
            .0
            .iter()
            .any(|line| line.starts_with("%error T 3 1"))
    );
}

#[test]
fn control_parse_and_generic_nonzero_results_do_not_set_retval() {
    let cases: &[(&str, &[u8], i32, &[&str], bool)] = &[
        (
            "parse-return-status",
            b"wibble\n\n",
            0,
            &["parse error: unknown command: wibble"],
            true,
        ),
        (
            "flag-parse-return-status",
            b"list-sessions -Z\n\n",
            0,
            &["command list-sessions: unknown flag -Z"],
            true,
        ),
        (
            "arity-parse-eof-status",
            b"rename-session\n",
            0,
            &["command rename-session: too few arguments (need at least 1)"],
            true,
        ),
    ];
    for (session, input, expected_status, expected_payload, expected_error) in cases {
        let fixture = Fixture::new();
        if !local_socket_bind_available(&fixture.socket) {
            return;
        }
        let output = fixture.run_with_stdin(
            &["-C", "new-session", "-s", session, "exec /bin/cat"],
            input,
        );
        assert_eq!(output.status.code(), Some(*expected_status), "{session}");
        assert!(output.stderr.is_empty(), "{session}");
        let stream = parse_stream(&output.stdout, false);
        assert_eq!(stream.blocks.len(), 2, "{session}");
        assert_block(&stream.blocks[0], 1, 0, &[], false);
        assert_block(&stream.blocks[1], 2, 1, expected_payload, *expected_error);
        assert_attached_startup(&stream.outside, session);
    }
}

#[cfg(unix)]
#[test]
fn control_foreground_run_shell_closes_before_raw_output_and_continues() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let marker_path = fixture._directory.path().join("run-shell-order-complete");
    let output = run_control_until_return(
        &fixture,
        &[
            "-C",
            "new-session",
            "-s",
            "run-shell-order",
            "exec /bin/cat",
        ],
        "run-shell 'printf foreground; exit 3' ; display-message -p AFTER_FOREGROUND\n\
         run-shell 'kill -TERM $$'\n",
        &marker_path,
        "foreground run-shell completion",
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout.clone()).expect("UTF-8 control output");
    let lines = text.lines().collect::<Vec<_>>();
    let index = |expected: &str| {
        lines
            .iter()
            .position(|line| *line == expected)
            .unwrap_or_else(|| panic!("missing {expected:?}: {text}"))
    };
    let frame = |marker_name: &str, number: u64| {
        lines
            .iter()
            .position(|line| {
                marker(line, marker_name).is_some_and(|(_, candidate, _)| candidate == number)
            })
            .unwrap_or_else(|| panic!("missing {marker_name} for {number}: {text}"))
    };
    assert!(frame("%end", 2) < index("foreground"));
    assert!(index("foreground") < index("'printf foreground; exit 3' returned 3"));
    assert!(index("'printf foreground; exit 3' returned 3") < frame("%begin", 3));
    assert!(frame("%end", 4) < index("'kill -TERM $$' terminated by signal 15"));

    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 5, "{stream:?}");
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    assert_block(&stream.blocks[2], 3, 1, &["AFTER_FOREGROUND"], false);
    assert_block(&stream.blocks[3], 4, 1, &[], false);
    assert_block(&stream.blocks[4], 5, 1, &[], false);
    assert!(stream.outside.iter().any(|line| line == "foreground"));
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line == "'printf foreground; exit 3' returned 3")
    );
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line == "'kill -TERM $$' terminated by signal 15")
    );
    assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
}

#[test]
fn control_alias_groups_inherit_flags_and_continue_after_shell_status() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "control-alias", "exec /bin/cat"]);
    assert_eq!(created.status.code(), Some(0));
    let configured = fixture.run(&[
        "set-option",
        "-s",
        "command-alias[90]",
        "auditmulti=display-message -p first ; display-message -p",
    ]);
    assert_eq!(configured.status.code(), Some(0));

    let initial = fixture.run_with_stdin(&["-C", "auditmulti", "tail"], b"");
    assert_eq!(initial.status.code(), Some(0));
    assert!(initial.stderr.is_empty());
    let stream = parse_stream(&initial.stdout, false);
    assert_eq!(stream.blocks.len(), 2);
    assert_block(&stream.blocks[0], 1, 0, &["first"], false);
    assert_block(&stream.blocks[1], 2, 0, &["tail"], false);
    assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));

    let configured = fixture.run(&[
        "set-option",
        "-s",
        "command-alias[91]",
        "auditfail=display-message -p before ; run-shell 'exit 3' ; display-message -p after",
    ]);
    assert_eq!(configured.status.code(), Some(0));
    let marker_path = fixture._directory.path().join("alias-shell-complete");
    let output = run_control_until_return(
        &fixture,
        &["-C", "attach-session", "-t", "control-alias"],
        "auditfail ; display-message -p same-line\n",
        &marker_path,
        "control alias shell completion",
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 6, "{stream:?}");
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &["before"], false);
    assert_block(&stream.blocks[2], 3, 1, &[], false);
    assert_block(&stream.blocks[3], 4, 1, &["after"], false);
    assert_block(&stream.blocks[4], 5, 1, &["same-line"], false);
    assert_block(&stream.blocks[5], 6, 1, &[], false);
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line == "'exit 3' returned 3")
    );

    let source = write_source(
        fixture._directory.path(),
        "alias-source.conf",
        "display-message -p CHILD_ONE\ndisplay-message -p CHILD_TWO\n",
    );
    let configured = fixture.run(&[
        "set-option",
        "-s",
        "command-alias[92]",
        &format!("auditsource=source-file '{source}'"),
    ]);
    assert_eq!(configured.status.code(), Some(0));
    let initial = fixture.run_with_stdin(&["-C", "auditsource"], b"");
    assert_eq!(initial.status.code(), Some(0));
    let stream = parse_stream(&initial.stdout, false);
    assert_eq!(stream.blocks.len(), 3, "{stream:?}");
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 0, &["CHILD_ONE"], false);
    assert_block(&stream.blocks[2], 3, 0, &["CHILD_TWO"], false);

    let configured = fixture.run(&[
        "set-option",
        "-s",
        "command-alias[94]",
        &format!(
            "auditmiddle=display-message -p BEFORE ; source-file '{source}' ; display-message -p OUTER"
        ),
    ]);
    assert_eq!(configured.status.code(), Some(0));
    let initial = fixture.run_with_stdin(&["-C", "auditmiddle"], b"");
    assert_eq!(initial.status.code(), Some(0));
    let stream = parse_stream_allow_gaps(&initial.stdout, false);
    assert_eq!(stream.blocks.len(), 5, "{stream:?}");
    assert_block(&stream.blocks[0], 1, 0, &["BEFORE"], false);
    assert_block(&stream.blocks[1], 2, 0, &[], false);
    assert_block(&stream.blocks[2], 3, 0, &["CHILD_ONE"], false);
    assert_block(&stream.blocks[3], 4, 0, &["CHILD_TWO"], false);
    assert_block(&stream.blocks[4], 6, 0, &["OUTER"], false);

    let missing = fixture._directory.path().join("missing-alias-source.conf");
    let configured = fixture.run(&[
        "set-option",
        "-s",
        "command-alias[93]",
        &format!(
            "auditmissing=display-message -p before-missing ; source-file '{}' ; display-message -p after-missing",
            missing.display()
        ),
    ]);
    assert_eq!(configured.status.code(), Some(0));
    let marker_path = fixture
        ._directory
        .path()
        .join("alias-source-failure-complete");
    let output = run_control_until_return(
        &fixture,
        &["-C", "attach-session", "-t", "control-alias"],
        "auditmissing ; display-message -p same-line-missing\n",
        &marker_path,
        "control alias source failure",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stream = parse_stream_allow_gaps(&output.stdout, false);
    assert!(
        stream
            .blocks
            .iter()
            .any(|block| block.payload == ["before-missing"])
    );
    assert!(stream.blocks.iter().any(|block| {
        block.error
            && block
                .payload
                .iter()
                .any(|line| line.contains("missing-alias-source.conf"))
    }));
    assert!(
        !stream.blocks.iter().any(|block| {
            block
                .payload
                .iter()
                .any(|line| matches!(line.as_str(), "after-missing" | "same-line-missing"))
        }),
        "{stream:?}"
    );

    let configured = fixture.run(&[
        "set-option",
        "-s",
        "command-alias[95]",
        &format!(
            "auditpartial=display-message -p before-partial ; source-file '{}' '{source}' ; display-message -p after-partial",
            missing.display()
        ),
    ]);
    assert_eq!(configured.status.code(), Some(0));
    let marker_path = fixture
        ._directory
        .path()
        .join("alias-partial-source-complete");
    let output = run_control_until_return(
        &fixture,
        &["-C", "attach-session", "-t", "control-alias"],
        "auditpartial\n",
        &marker_path,
        "control alias partial source failure",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stream = parse_stream_allow_gaps(&output.stdout, false);
    let payloads = stream
        .blocks
        .iter()
        .flat_map(|block| block.payload.iter().map(String::as_str))
        .collect::<Vec<_>>();
    for expected in ["before-partial", "CHILD_ONE", "CHILD_TWO", "after-partial"] {
        assert!(payloads.contains(&expected), "{stream:?}");
    }
    assert!(stream.blocks.iter().any(|block| {
        !block.error
            && block
                .payload
                .iter()
                .any(|line| line.contains("missing-alias-source.conf"))
    }));

    let state_source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../compat/scenarios/source-file-output-child.conf");
    let configured = fixture.run(&[
        "set-option",
        "-s",
        "command-alias[96]",
        &format!(
            "auditshutdown=kill-server ; source-file '{}' ; display-message -p OUTER",
            state_source.display()
        ),
    ]);
    assert_eq!(configured.status.code(), Some(0));
    let output = fixture.run_with_stdin(&["-C", "auditshutdown"], b"");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let lines = std::str::from_utf8(&output.stdout)
        .expect("UTF-8 shutdown alias output")
        .lines()
        .collect::<Vec<_>>();
    let exits = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| **line == "%exit")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(exits.len(), 1, "{lines:?}");
    let child = lines
        .iter()
        .position(|line| *line == "CHILD_ONE")
        .expect("sourced child after shutdown");
    assert!(exits[0] < child, "{lines:?}");
    let stream = parse_stream_allow_gaps(&output.stdout, false);
    let child = stream
        .blocks
        .iter()
        .position(|block| block.payload == ["CHILD_ONE"])
        .expect("sourced child guard after shutdown");
    assert!(stream.blocks[child + 1].payload.is_empty(), "{stream:?}");
    assert!(
        !stream
            .blocks
            .iter()
            .any(|block| block.payload == ["CHILD_TWO"])
    );
    assert!(
        stream
            .blocks
            .iter()
            .skip(child + 2)
            .any(|block| block.payload == ["OUTER"])
    );
    fixture.assert_stopped();
}

#[test]
fn shutdown_hook_source_yields_locally_without_replaying_the_after_hook() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "hookprobe", "exec /bin/cat"]);
    assert_eq!(created.status.code(), Some(0));
    let scenarios = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../compat/scenarios");
    let root = scenarios.join("source-file-output-hook-root.conf");
    let child = scenarios.join("source-file-output-child.conf");
    let hook = format!("source-file '{}'", child.display());
    let configured = fixture.run(&["set-hook", "-g", "after-display-message", hook.as_str()]);
    assert_eq!(configured.status.code(), Some(0));
    let callback = format!(
        "kill-server ; source-file '{}' ; display-message -p OUTER",
        root.display()
    );
    let output = fixture.run_with_stdin(&["-C", "run-shell", "-C", callback.as_str()], b"");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream_allow_gaps(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 3, "{stream:?}");
    let payloads = stream
        .blocks
        .iter()
        .flat_map(|block| block.payload.iter().map(String::as_str))
        .collect::<Vec<_>>();
    assert_eq!(payloads, ["OUTER", "HOOK_TRIGGER"]);
    assert_eq!(
        stream
            .blocks
            .iter()
            .filter(|block| block.payload.is_empty())
            .count(),
        1
    );
    for suppressed in ["HOOK_LATER", "CHILD_ONE", "CHILD_TWO"] {
        assert!(!payloads.contains(&suppressed), "{stream:?}");
    }
    let lines = std::str::from_utf8(&output.stdout)
        .expect("UTF-8 shutdown hook output")
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(lines.iter().filter(|line| **line == "%exit").count(), 1);
    assert_eq!(lines.last().copied(), Some("%exit"));
    fixture.assert_stopped();
}

#[test]
fn foreground_callback_error_prevents_later_shutdown_without_losing_guards() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "callback-error"]);
    assert_eq!(created.status.code(), Some(0));
    let output = fixture.run_with_stdin(
        &[
            "-C",
            "run-shell",
            "-C",
            "display-message -F MATCH one ; kill-server",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 2);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(
        &stream.blocks[1],
        2,
        0,
        &["only one of -F or argument must be given"],
        true,
    );
    assert_eq!(stream.outside, ["%exit"]);

    let listed = fixture.run(&["list-sessions"]);
    assert_eq!(listed.status.code(), Some(0));
    let stopped = fixture.run(&["kill-server"]);
    assert_eq!(stopped.status.code(), Some(0));
    fixture.assert_stopped();
}

#[test]
fn draining_alias_shutdown_tears_down_panes_and_rejects_late_commands_cleanly() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let ready = fixture._directory.path().join("shutdown-pane-ready");
    let trigger = fixture._directory.path().join("shutdown-pane-trigger");
    let marker = fixture._directory.path().join("shutdown-pane-marker");
    let pane_command = format!(
        "printf ready > '{}'; while [ ! -e '{}' ]; do sleep 0.01; done; sleep 0.2; printf pane > '{}'; sleep 5",
        ready.display(),
        trigger.display(),
        marker.display()
    );
    let created = fixture.run(&[
        "new-session",
        "-d",
        "-s",
        "draining-shutdown",
        &pane_command,
    ]);
    assert_eq!(created.status.code(), Some(0));
    let deadline = Instant::now() + Duration::from_secs(2);
    while !ready.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(ready.exists(), "pane process did not start");
    let configured = fixture.run(&[
        "set-option",
        "-s",
        "command-alias[97]",
        "slowshutdown=kill-server ; run-shell 'sleep 0.8'",
    ]);
    assert_eq!(configured.status.code(), Some(0));

    let shutdown = fixture.run(&["slowshutdown"]);
    assert_eq!(shutdown.status.code(), Some(0));
    assert!(shutdown.stdout.is_empty());
    assert!(shutdown.stderr.is_empty());
    let late = fixture.run(&["list-sessions"]);
    assert_eq!(late.status.code(), Some(1));
    assert!(late.stdout.is_empty());
    assert_eq!(late.stderr, b"server exited unexpectedly\n");
    std::fs::write(&trigger, b"go").expect("release pane process");
    thread::sleep(Duration::from_millis(400));
    assert!(!marker.exists());
    fixture.assert_stopped();
    assert!(!marker.exists());
}

#[cfg(unix)]
#[test]
fn control_sourced_run_shell_closes_before_raw_output_and_same_line_continues() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture._directory.path().join("sourced run-shell");
    std::fs::create_dir(&directory).expect("create sourced run-shell directory");
    let source = write_source(
        &directory,
        "run-shell.conf",
        "run-shell 'printf CHILD; exit 3' ; display-message -p SAME\n",
    );
    let marker_path = directory.join("complete");
    let output = run_control_until_return(
        &fixture,
        &[
            "-C",
            "new-session",
            "-s",
            "sourced-run-shell",
            "exec /bin/cat",
        ],
        &format!("source-file '{source}'\n"),
        &marker_path,
        "sourced run-shell completion",
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout.clone()).expect("UTF-8 control output");
    let lines = text.lines().collect::<Vec<_>>();
    let child = lines
        .iter()
        .position(|line| *line == "CHILD")
        .unwrap_or_else(|| panic!("missing raw child output: {text}"));
    assert_eq!(lines[child + 1], "'printf CHILD; exit 3' returned 3");
    let before = next_block_guard(lines[..child].iter().rev().copied());
    let end = marker(before, "%end").expect("guard before raw child output");
    assert_eq!(end.2, 1);
    let after = next_block_guard(lines[child + 2..].iter().copied());
    let begin = marker(after, "%begin").expect("guard after raw diagnostic");
    assert_eq!(begin.2, 1);

    let stream = parse_stream_allow_gaps(&output.stdout, false);
    let shell = stream
        .blocks
        .iter()
        .find(|block| block.number == end.1)
        .expect("sourced run-shell guard");
    assert!(shell.payload.is_empty());
    assert!(!shell.error);
    let same = stream
        .blocks
        .iter()
        .find(|block| block.number == begin.1)
        .expect("same-line continuation guard");
    assert_eq!(same.payload, ["SAME"]);
    assert!(!same.error);
    assert!(stream.outside.iter().any(|line| line == "CHILD"));
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line == "'printf CHILD; exit 3' returned 3")
    );
}

#[test]
fn bare_control_defaults_to_attached_new_session() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let output = fixture.run_with_stdin(&["-C"], b"ls\n\n");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 2);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_eq!(stream.blocks[1].flags, 1);
    assert!(!stream.blocks[1].payload.is_empty());
    assert!(!stream.blocks[1].error);
    assert_attached_startup(&stream.outside, "0");
}

#[test]
fn double_control_wraps_stdout_in_dcs_and_st() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let output = fixture.run(&["-CC", "new-session", "-d"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, true);
    assert_eq!(stream.blocks.len(), 1);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_eq!(stream.outside, ["%exit"]);
}

#[test]
fn control_sigterm_exits_promptly_while_attached_and_idle() {
    for double in [false, true] {
        let fixture = Fixture::new();
        if !local_socket_bind_available(&fixture.socket) {
            return;
        }
        let session = "sigterm-idle";
        assert!(
            fixture
                .run(&["new-session", "-d", "-s", session, "exec /bin/cat"])
                .status
                .success()
        );
        let output_path = fixture._directory.path().join("sigterm-idle.output");
        let (mut child, mut stdin) = spawn_control_to_file(
            &fixture,
            &[
                if double { "-CC" } else { "-C" },
                "attach-session",
                "-t",
                session,
            ],
            &output_path,
        );
        writeln!(stdin, "display-message -p SIGTERM_IDLE_READY")
            .expect("write idle signal readiness command");
        stdin.flush().expect("flush idle signal readiness command");
        wait_for_control_output_marker(
            &output_path,
            "SIGTERM_IDLE_READY",
            &mut child,
            "idle SIGTERM readiness",
        );
        let output = terminate_control_process(child, stdin, "idle attached control");
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let stdout = std::fs::read(output_path).expect("read idle SIGTERM output");
        let stream = parse_stream(&stdout, double);
        assert_eq!(stream.blocks.len(), 2, "{stream:?}");
        assert_block(&stream.blocks[0], 1, 0, &[], false);
        assert_block(&stream.blocks[1], 2, 1, &["SIGTERM_IDLE_READY"], false);
        assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
        assert_eq!(
            stream
                .outside
                .iter()
                .filter(|line| *line == "%exit")
                .count(),
            1
        );
    }
}

#[test]
fn control_sigterm_closes_an_open_guard_while_waiting() {
    for double in [false, true] {
        let fixture = Fixture::new();
        if !local_socket_bind_available(&fixture.socket) {
            return;
        }
        let session = "sigterm-waiting";
        assert!(
            fixture
                .run(&["new-session", "-d", "-s", session, "exec /bin/cat"])
                .status
                .success()
        );
        let output_path = fixture._directory.path().join("sigterm-waiting.output");
        let (mut child, mut stdin) = spawn_control_to_file(
            &fixture,
            &[
                if double { "-CC" } else { "-C" },
                "attach-session",
                "-t",
                session,
            ],
            &output_path,
        );
        writeln!(stdin, "display-message -p SIGTERM_WAITING_READY")
            .expect("write waiting signal readiness command");
        stdin
            .flush()
            .expect("flush waiting signal readiness command");
        wait_for_control_output_marker(
            &output_path,
            "SIGTERM_WAITING_READY",
            &mut child,
            "waiting SIGTERM readiness",
        );
        writeln!(stdin, "wait-for SIGTERM_GUARD_HOLD").expect("write waiting command");
        stdin.flush().expect("flush waiting command");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let stdout = std::fs::read(&output_path).expect("read waiting control output");
            let text = String::from_utf8_lossy(&stdout);
            let started = text.lines().any(|line| {
                marker(line, "%begin").is_some_and(|(_, number, flags)| number == 3 && flags == 1)
            });
            if started {
                if text.lines().any(|line| {
                    marker(line, "%end")
                        .or_else(|| marker(line, "%error"))
                        .is_some_and(|(_, number, _)| number == 3)
                }) {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("waiting guard was already closed: {text}");
                }
                break;
            }
            if Instant::now() >= deadline
                || child
                    .try_wait()
                    .expect("poll waiting control process")
                    .is_some()
            {
                let _ = child.kill();
                let _ = child.wait();
                panic!("waiting command did not expose its open guard: {text}");
            }
            thread::sleep(Duration::from_millis(10));
        }
        let output = terminate_control_process(child, stdin, "open waiting control guard");
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let stdout = std::fs::read(output_path).expect("read waiting SIGTERM output");
        let stream = parse_stream(&stdout, double);
        assert_eq!(stream.blocks.len(), 3, "{stream:?}");
        assert_block(&stream.blocks[0], 1, 0, &[], false);
        assert_block(&stream.blocks[1], 2, 1, &["SIGTERM_WAITING_READY"], false);
        assert_block(&stream.blocks[2], 3, 1, &[], false);
        assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
        assert_eq!(
            stream
                .outside
                .iter()
                .filter(|line| *line == "%exit")
                .count(),
            1
        );
    }
}

#[test]
fn control_stdin_parse_error_is_framed_and_the_loop_continues() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let output = fixture.run_with_stdin(&["-C", "new-session", "-s", "parse-error"], b"wibble\n\n");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 2);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(
        &stream.blocks[1],
        2,
        1,
        &["parse error: unknown command: wibble"],
        true,
    );
    assert_attached_startup(&stream.outside, "parse-error");
}

#[test]
fn control_stdin_preflight_errors_abort_the_whole_line() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let marker = fixture._directory.path().join("preflight.complete");
    let output = run_control_until_return(
        &fixture,
        &["-C", "new-session", "-s", "preflight-error"],
        "set-option -g @should-not-run yes ; bogus-command\nshow-options -gqv @should-not-run\n",
        &marker,
        "preflight completion",
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 4);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(
        &stream.blocks[1],
        2,
        1,
        &["parse error: unknown command: bogus-command"],
        true,
    );
    assert_block(&stream.blocks[2], 3, 1, &[], false);
    assert_block(&stream.blocks[3], 4, 1, &[], false);
    assert_attached_startup(&stream.outside, "preflight-error");
}

#[test]
fn control_config_error_keeps_the_source_line_and_inner_message() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture._directory.path().join("a: b");
    std::fs::create_dir(&directory).expect("create config error directory");
    let source = directory.join("mux.conf");
    std::fs::write(&source, "wibble\ndisplay-message -p after-config-error\n")
        .expect("write invalid config");
    let input = format!("source-file '{}'\n\n", source.display());
    let output = fixture.run_with_stdin(
        &["-C", "new-session", "-s", "config-error"],
        input.as_bytes(),
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 2);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    let diagnostic = format!(
        "%config-error {}:1: unknown command: wibble",
        source.display()
    );
    assert!(stream.outside.contains(&diagnostic));
    let outside = stream
        .outside
        .into_iter()
        .filter(|line| line != &diagnostic)
        .collect::<Vec<_>>();
    assert_attached_startup(&outside, "config-error");
}

#[test]
fn control_first_sourced_construction_failure_uses_config_error() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let source = fixture._directory.path().join("classification.conf");
    std::fs::write(
        &source,
        "set-option -s command-alias[90] badalias=\n\
         set-option -s command-alias[91] source=\n\
         source ignored.conf\n\
         wibble\n\
         kill-s\n\
         badalias\n\
         set-environment -g\n\
         new-pane\n\
         display-message -p after-classification\n",
    )
    .expect("write sourced classification fixture");
    let input = format!("source-file '{}'\n\n", source.display());
    let output = fixture.run_with_stdin(
        &["-C", "new-session", "-s", "source-classification"],
        input.as_bytes(),
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 2);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    let diagnostics = [format!(
        "%config-error {}:4: unknown command: wibble",
        source.display()
    )];
    for diagnostic in &diagnostics {
        assert!(stream.outside.contains(diagnostic), "{diagnostic}");
    }
    let outside = stream
        .outside
        .into_iter()
        .filter(|line| !diagnostics.contains(line))
        .collect::<Vec<_>>();
    assert_attached_startup(&outside, "source-classification");
}

#[test]
fn control_sourced_success_quiet_and_partial_guards_keep_fifo_status() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture._directory.path().join("control sourced guards");
    std::fs::create_dir(&directory).expect("create sourced guard directory");
    let quiet = write_source(
        &directory,
        "quiet.conf",
        "set-option -g @guard-success yes\nsource-file -q quiet-missing.conf\n",
    );
    let quiet_input = format!("source-file '{quiet}'\ndisplay-message -p quiet-fresh\n");
    let quiet_marker = directory.join("quiet.complete");
    let quiet_output = run_control_until_return(
        &fixture,
        &["-C", "new-session", "-s", "quiet-guards"],
        &quiet_input,
        &quiet_marker,
        "quiet guard completion",
    );
    assert_eq!(quiet_output.status.code(), Some(0));
    assert!(quiet_output.stderr.is_empty());
    let quiet_stream = parse_stream_allow_gaps(&quiet_output.stdout, false);
    assert_eq!(quiet_stream.blocks.len(), 6);
    assert_block(&quiet_stream.blocks[0], 1, 0, &[], false);
    assert_block(&quiet_stream.blocks[1], 2, 1, &[], false);
    assert_block(&quiet_stream.blocks[2], 3, 1, &[], false);
    assert_block(&quiet_stream.blocks[3], 4, 1, &[], false);
    assert_block(&quiet_stream.blocks[4], 7, 1, &["quiet-fresh"], false);
    assert_block(&quiet_stream.blocks[5], 8, 1, &[], false);
    assert_attached_startup(&quiet_stream.outside, "quiet-guards");

    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture._directory.path().join("control partial guards");
    std::fs::create_dir(&directory).expect("create partial guard directory");
    let leaf = write_source(
        &directory,
        "leaf.conf",
        "set-option -g @guard-partial yes\n",
    );
    let entry = write_source(
        &directory,
        "entry.conf",
        &format!(
            "source-file partial-missing.conf '{leaf}'\n\
             display-message -p sourced-output\n"
        ),
    );
    let partial_input = format!("source-file '{entry}'\ndisplay-message -p partial-fresh\n");
    let partial_marker = directory.join("partial.complete");
    let partial_output = run_control_until_return(
        &fixture,
        &["-C", "new-session", "-s", "partial-guards"],
        &partial_input,
        &partial_marker,
        "partial guard completion",
    );
    assert_eq!(partial_output.status.code(), Some(1));
    assert!(partial_output.stderr.is_empty());
    let partial_stream = parse_stream_allow_gaps(&partial_output.stdout, false);
    assert_eq!(partial_stream.blocks.len(), 7);
    assert_block(&partial_stream.blocks[0], 1, 0, &[], false);
    assert_block(&partial_stream.blocks[1], 2, 1, &[], false);
    assert_block(
        &partial_stream.blocks[2],
        3,
        1,
        &["No such file or directory: partial-missing.conf"],
        false,
    );
    assert_block(&partial_stream.blocks[3], 4, 1, &[], false);
    assert_block(&partial_stream.blocks[4], 6, 1, &["sourced-output"], false);
    assert_block(&partial_stream.blocks[5], 8, 1, &["partial-fresh"], false);
    assert_block(&partial_stream.blocks[6], 9, 1, &[], false);
    assert_attached_startup(&partial_stream.outside, "partial-guards");
}

#[test]
fn config_file_bytes_load_like_the_pinned_signed_buffer() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture._directory.path().join("config byte input");
    std::fs::create_dir(&directory).expect("create byte config directory");
    let isolated = directory.join("isolated-ff.conf");
    std::fs::write(&isolated, b"\xff").expect("write isolated byte config");
    let isolated_path = isolated.to_str().expect("UTF-8 byte config path");
    let continued = directory.join("continued-ff.conf");
    std::fs::write(
        &continued,
        b"\xffset-environment -g CONFIG_BYTE_INPUT continued\n",
    )
    .expect("write continued byte config");
    let continued_path = continued.to_str().expect("UTF-8 continued config path");

    assert_eq!(
        fixture
            .run(&["new-session", "-d", "-s", "byte-input"])
            .status
            .code(),
        Some(0)
    );
    let sourced = fixture.run(&["source-file", "-v", isolated_path]);
    assert_eq!(sourced.status.code(), Some(0));
    assert!(sourced.stdout.is_empty());
    assert!(sourced.stderr.is_empty());

    let continued_run = fixture.run(&["source-file", "-v", continued_path]);
    assert_eq!(continued_run.status.code(), Some(0));
    assert!(continued_run.stderr.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&continued_run.stdout),
        format!("{continued_path}:1: set-environment -g CONFIG_BYTE_INPUT continued\n")
    );
    let shown = fixture.run(&["show-environment", "-g", "CONFIG_BYTE_INPUT"]);
    assert_eq!(shown.status.code(), Some(0));
    assert_eq!(shown.stdout, b"CONFIG_BYTE_INPUT=continued\n");
    fixture.run(&["kill-server"]);
    fixture.assert_stopped();
}

#[test]
fn startup_config_bytes_keep_the_pinned_file_command_name() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let startup = fixture._directory.path().join("startup-ff.conf");
    std::fs::write(
        &startup,
        b"\xffset-environment -g CONFIG_BYTE_STARTUP set\n",
    )
    .expect("write startup byte config");

    let started =
        fixture.run_with_configs(&[&startup], &["new-session", "-d", "-s", "byte-startup"]);
    assert_eq!(started.status.code(), Some(0));
    assert!(started.stdout.is_empty());
    assert!(started.stderr.is_empty());

    let shown = fixture.run_with_configs(
        &[&startup],
        &["show-environment", "-g", "CONFIG_BYTE_STARTUP"],
    );
    assert_eq!(shown.status.code(), Some(1));
    assert!(shown.stdout.is_empty());
    assert_eq!(shown.stderr, b"unknown variable: CONFIG_BYTE_STARTUP\n");
    fixture.run_with_configs(&[&startup], &["kill-server"]);
    fixture.assert_stopped();
}

#[test]
fn control_config_file_bytes_keep_the_pinned_hidden_item_numbering() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture._directory.path().join("control byte input");
    std::fs::create_dir(&directory).expect("create control byte directory");
    let isolated = directory.join("isolated-ff.conf");
    std::fs::write(&isolated, b"\xff").expect("write isolated byte config");
    let isolated_path = isolated.to_str().expect("UTF-8 byte config path");
    let empty = write_source(&directory, "empty.conf", "");

    let input = format!(
        "source-file '{empty}'\n\
         display-message -p empty-fresh\n\
         source-file '{isolated_path}'\n\
         display-message -p byte-fresh\n"
    );
    let marker = directory.join("byte.complete");
    let output = run_control_until_return(
        &fixture,
        &["-C", "new-session", "-s", "byte-frames"],
        &input,
        &marker,
        "byte guard completion",
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream_allow_gaps(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 6);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    assert_block(&stream.blocks[2], 4, 1, &["empty-fresh"], false);
    assert_block(&stream.blocks[3], 5, 1, &[], false);
    assert_block(&stream.blocks[4], 8, 1, &["byte-fresh"], false);
    assert_block(&stream.blocks[5], 9, 1, &[], false);
    assert_attached_startup(&stream.outside, "byte-frames");
}

#[test]
fn control_if_shell_config_file_bytes_continue_the_root_file() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture._directory.path().join("control byte if-shell");
    std::fs::create_dir(&directory).expect("create byte if-shell directory");
    let isolated = directory.join("isolated-ff.conf");
    std::fs::write(&isolated, b"\xff").expect("write isolated byte config");
    let root = write_source(
        &directory,
        "root.conf",
        &format!(
            "display-message -p root-before\n\
             if-shell -F 1 \"source-file '{}'\"\n\
             display-message -p root-after\n",
            isolated.display()
        ),
    );

    let input = format!("source-file '{root}'\ndisplay-message -p if-shell-fresh\n");
    let marker = directory.join("if-shell.complete");
    let output = run_control_until_return(
        &fixture,
        &["-C", "new-session", "-s", "byte-if-shell"],
        &input,
        &marker,
        "byte if-shell completion",
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream_allow_gaps(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 8);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    assert_block(&stream.blocks[2], 3, 1, &["root-before"], false);
    assert_block(&stream.blocks[3], 4, 1, &[], false);
    assert_block(&stream.blocks[4], 5, 1, &[], false);
    assert_block(&stream.blocks[5], 8, 1, &["root-after"], false);
    assert_block(&stream.blocks[6], 10, 1, &["if-shell-fresh"], false);
    assert_block(&stream.blocks[7], 11, 1, &[], false);
    assert_attached_startup(&stream.outside, "byte-if-shell");
}

#[test]
fn control_source_read_error_follows_the_parent_guard_and_sets_retval() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let unreadable = fixture._directory.path().join("source read directory");
    std::fs::create_dir(&unreadable).expect("create unreadable source directory");
    let entry = write_source(
        fixture._directory.path(),
        "read-entry.conf",
        &format!("source-file '{}'\n", unreadable.display()),
    );
    let read_error =
        std::fs::read_to_string(&unreadable).expect_err("reading the source directory must fail");
    let expected = format!("{}: {}", os_error_text(&read_error), unreadable.display());
    let input = format!("source-file '{entry}'\ndisplay-message -p after-read-error\n");
    let marker = fixture._directory.path().join("source-read.complete");
    let output = run_control_until_return(
        &fixture,
        &["-C", "new-session", "-s", "source-read-error"],
        &input,
        &marker,
        "source read completion",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let lines = std::str::from_utf8(&output.stdout)
        .expect("UTF-8 source read output")
        .lines()
        .collect::<Vec<_>>();
    let error_index = lines
        .iter()
        .position(|line| line == &expected)
        .expect("raw source read error");
    let before = next_block_guard(lines[..error_index].iter().rev().copied());
    assert!(
        before.starts_with("%end ") && before.ends_with(" 3 1"),
        "{lines:?}"
    );
    let after = next_block_guard(lines[error_index + 1..].iter().copied());
    assert!(
        after.starts_with("%begin ") && after.ends_with(" 6 1"),
        "{lines:?}"
    );
    let stream = parse_stream_allow_gaps(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 5);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    assert_block(&stream.blocks[2], 3, 1, &[], false);
    assert_block(&stream.blocks[3], 6, 1, &["after-read-error"], false);
    assert_block(&stream.blocks[4], 7, 1, &[], false);
    assert!(stream.outside.contains(&expected));
    let outside = stream
        .outside
        .into_iter()
        .filter(|line| line != &expected)
        .collect::<Vec<_>>();
    assert_attached_startup(&outside, "source-read-error");
}

#[test]
fn control_source_diagnostics_keep_tmux_chain_semantics() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture._directory.path().join("nested control");
    std::fs::create_dir(&directory).expect("create nested source directory");
    let loud = directory.join("loud entry.conf");
    let quiet = directory.join("quiet entry.conf");
    let invalid = directory.join("invalid entry.conf");
    std::fs::write(&loud, "source-file nested-missing.conf\n").expect("write loud nested source");
    std::fs::write(&quiet, "source-file -q nested-missing.conf\n")
        .expect("write quiet nested source");
    std::fs::write(&invalid, "wibble\n").expect("write invalid source");
    let input = format!(
        "source-file '{}' ; display-message -p same-nested\nsource-file top-missing.conf ; display-message -p same-top-missing\nsource-file '{}' ; display-message -p same-invalid\nsource-file '{}'\ndisplay-message -p fresh\n",
        loud.display(),
        invalid.display(),
        quiet.display()
    );
    let marker = directory.join("source-diagnostics.complete");
    let output = run_control_until_return(
        &fixture,
        &["-C", "new-session", "-s", "nested-source-error"],
        &input,
        &marker,
        "source diagnostic completion",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stream = parse_stream_allow_gaps(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 11, "{stream:?}");
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    assert_block(
        &stream.blocks[2],
        3,
        1,
        &["No such file or directory: nested-missing.conf"],
        true,
    );
    assert_block(&stream.blocks[3], 6, 1, &["same-nested"], false);
    assert_block(
        &stream.blocks[4],
        7,
        1,
        &["No such file or directory: top-missing.conf"],
        true,
    );
    assert_block(&stream.blocks[5], 9, 1, &[], false);
    assert_block(&stream.blocks[6], 11, 1, &["same-invalid"], false);
    assert_block(&stream.blocks[7], 12, 1, &[], false);
    assert_block(&stream.blocks[8], 13, 1, &[], false);
    assert_block(&stream.blocks[9], 16, 1, &["fresh"], false);
    assert_block(&stream.blocks[10], 17, 1, &[], false);
    let diagnostic = format!(
        "%config-error {}:1: unknown command: wibble",
        invalid.display()
    );
    assert!(stream.outside.contains(&diagnostic));
    let outside = stream
        .outside
        .into_iter()
        .filter(|line| line != &diagnostic)
        .collect::<Vec<_>>();
    assert_attached_startup(&outside, "nested-source-error");
}

#[test]
fn control_replayed_runtime_errors_use_bare_error_blocks_and_continue() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture._directory.path().join("control replay errors");
    std::fs::create_dir(&directory).expect("create control replay directory");
    let source = write_source(
        &directory,
        "runtime.conf",
        "kill-session -t missing-runtime\n\
         set-option -g nonexistent-option value\n\
         set-environment -g \"\" value\n\
         set-option -g @runtime-after yes\n",
    );
    let input = format!(
        "source-file '{source}'\nshow-options -gqv @runtime-after\ndisplay-message -p fresh\n"
    );
    let marker = directory.join("runtime.complete");
    let output = run_control_until_return(
        &fixture,
        &["-C", "new-session", "-s", "control-replay-errors"],
        &input,
        &marker,
        "runtime replay completion",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stream = parse_stream_allow_gaps(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 9);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    assert_block(
        &stream.blocks[2],
        3,
        1,
        &["can't find session: missing-runtime"],
        true,
    );
    assert_block(
        &stream.blocks[3],
        4,
        1,
        &["invalid option: nonexistent-option"],
        true,
    );
    assert_block(&stream.blocks[4], 5, 1, &["empty variable name"], true);
    assert_block(&stream.blocks[5], 6, 1, &[], false);
    assert_block(&stream.blocks[6], 8, 1, &["yes"], false);
    assert_block(&stream.blocks[7], 9, 1, &["fresh"], false);
    assert_block(&stream.blocks[8], 10, 1, &[], false);
    assert_attached_startup(&stream.outside, "control-replay-errors");
}

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
fn control_nested_source_depth_limit_uses_sourced_command_guards() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture._directory.path().join("control depth");
    std::fs::create_dir(&directory).expect("create control depth directory");
    let entry = write_source_chain(&directory, 50);
    let input = format!("source-file '{entry}' ; display-message -p after-the-limit\n");
    let marker = directory.join("depth.complete");
    let output = run_control_until_return(
        &fixture,
        &["-C", "new-session", "-s", "control-depth", "exec /bin/cat"],
        &input,
        &marker,
        "nested source depth completion",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stream = parse_stream_allow_gaps(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 155);
    assert_block(&stream.blocks[0], 1, 0, &[], false);
    assert_block(&stream.blocks[1], 2, 1, &[], false);
    let errors = stream
        .blocks
        .iter()
        .filter(|block| block.error)
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 2);
    assert!(
        errors
            .iter()
            .all(|block| block.payload == ["too many nested files"])
    );
    assert_block(&stream.blocks[153], 204, 1, &["after-the-limit"], false);
    assert_block(&stream.blocks[154], 205, 1, &[], false);
    assert_eq!(stream.blocks[154].number - stream.blocks.len() as u64, 50);
    assert_attached_startup(&stream.outside, "control-depth");
}

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

#[test]
fn control_notifications_layout_and_output_follow_the_live_socket_stream() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let (child, mut stdin) =
        fixture.spawn_with_open_stdin(&["-C", "new-session", "-s", "watched", "exec /bin/cat"]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ready = fixture.run(&["has-session", "-t", "watched"]);
        if ready.status.success() {
            break;
        }
        assert!(Instant::now() < deadline, "control session did not start");
        thread::sleep(Duration::from_millis(10));
    }

    let added = fixture.run(&[
        "new-window",
        "-d",
        "-t",
        "watched",
        "-n",
        "added",
        "exec /bin/cat",
    ]);
    assert_eq!(added.status.code(), Some(0));
    let foreign = fixture.run(&["new-session", "-d", "-s", "foreign", "exec /bin/cat"]);
    assert_eq!(foreign.status.code(), Some(0));
    let split = fixture.run(&[
        "split-window",
        "-h",
        "-d",
        "-t",
        "watched:0",
        "exec /bin/cat",
    ]);
    assert_eq!(split.status.code(), Some(0));
    let typed = fixture.run(&["send-keys", "-l", "-t", "watched:0.0", "CONTROL_OUTPUT"]);
    assert_eq!(typed.status.code(), Some(0));
    let entered = fixture.run(&["send-keys", "-t", "watched:0.0", "Enter"]);
    assert_eq!(entered.status.code(), Some(0));

    stdin
        .write_all(b"run-shell \"sleep 1\"\n")
        .expect("start slow control command");
    stdin.flush().expect("flush slow control command");
    thread::sleep(Duration::from_millis(100));
    let renamed = fixture.run(&["rename-window", "-t", "watched:0", "during"]);
    assert_eq!(renamed.status.code(), Some(0));
    thread::sleep(Duration::from_millis(1100));
    stdin.write_all(b"\n").expect("end control input");
    drop(stdin);

    let output = child.wait_with_output().expect("wait for control stream");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert_eq!(stream.blocks.len(), 2);
    assert!(
        stream
            .blocks
            .iter()
            .all(|block| { block.payload.iter().all(|line| !line.starts_with('%')) })
    );

    let notifications = stream
        .outside
        .iter()
        .filter(|line| {
            !line.starts_with("%output ")
                && !line.starts_with("%window-renamed @")
                && line.as_str() != "%exit"
        })
        .collect::<Vec<_>>();
    let position = |predicate: &dyn Fn(&str) -> bool| {
        notifications
            .iter()
            .position(|line| predicate(line))
            .unwrap_or_else(|| panic!("missing startup notification: {notifications:?}"))
    };
    let added = position(&|line| line.starts_with("%window-add @"));
    let listed = position(&|line| line == "%sessions-changed");
    let switched =
        position(&|line| line.starts_with("%session-changed $") && line.ends_with(" watched"));
    assert!(added < listed && listed < switched, "{notifications:?}");
    assert!(
        notifications
            .iter()
            .any(|line| line.starts_with("%window-add @"))
    );
    assert!(
        notifications
            .iter()
            .any(|line| line.starts_with("%unlinked-window-add @"))
    );
    assert!(notifications.iter().any(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        fields.first() == Some(&"%layout-change")
            && fields.get(2).is_some_and(|layout| checked_layout(layout))
            && fields.get(3).is_some_and(|layout| checked_layout(layout))
    }));
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line.starts_with("%window-renamed @") && line.ends_with(" during"))
    );
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line.starts_with("%output %") && line.contains("CONTROL_OUTPUT"))
    );
    assert_eq!(stream.outside.last().map(String::as_str), Some("%exit"));
}

#[test]
fn refresh_client_pane_states_gate_the_live_control_stream() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let (child, mut stdin) =
        fixture.spawn_with_open_stdin(&["-C", "new-session", "-s", "flow-state", "exec /bin/cat"]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ready = fixture.run(&["has-session", "-t", "flow-state"]);
        if ready.status.success() {
            break;
        }
        assert!(Instant::now() < deadline, "control session did not start");
        thread::sleep(Duration::from_millis(10));
    }
    let pane_output = fixture.run(&["list-panes", "-t", "flow-state", "-F", "#{pane_id}"]);
    assert!(pane_output.status.success());
    let pane = String::from_utf8(pane_output.stdout)
        .expect("pane id")
        .trim()
        .to_owned();
    let deadline = Instant::now() + Duration::from_secs(10);
    while fixture
        .run(&[
            "display-message",
            "-p",
            "-t",
            &pane,
            "#{pane_current_command}",
        ])
        .stdout
        != b"cat\n"
    {
        assert!(Instant::now() < deadline, "cat did not start");
        thread::sleep(Duration::from_millis(10));
    }
    fn control(stdin: &mut ChildStdin, pane: &str, state: &str) {
        writeln!(stdin, "refresh-client -A '{pane}:{state}'").expect("write pane state");
        stdin.flush().expect("flush pane state");
        thread::sleep(Duration::from_millis(100));
    }
    fn type_line(stdin: &mut ChildStdin, pane: &str, text: &str) {
        writeln!(stdin, "send-keys -l -t {pane} {text}").expect("write typed text");
        writeln!(stdin, "send-keys -t {pane} Enter").expect("write typed enter");
        stdin.flush().expect("flush typed text");
        thread::sleep(Duration::from_millis(150));
    }

    control(&mut stdin, &pane, "off");
    type_line(&mut stdin, &pane, "HIDDEN_WHILE_OFF");
    control(&mut stdin, &pane, "on");
    type_line(&mut stdin, &pane, "VISIBLE_AFTER_ON");
    thread::sleep(Duration::from_millis(400));
    control(&mut stdin, &pane, "pause");
    control(&mut stdin, &pane, "pause");
    type_line(&mut stdin, &pane, "HIDDEN_WHILE_PAUSED");
    control(&mut stdin, &pane, "continue");
    control(&mut stdin, &pane, "continue");
    type_line(&mut stdin, &pane, "VISIBLE_AFTER_CONTINUE");
    thread::sleep(Duration::from_millis(400));
    stdin.write_all(b"\n").expect("end control input");
    drop(stdin);

    let output = child.wait_with_output().expect("wait for control stream");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    let output_lines = stream
        .outside
        .iter()
        .filter(|line| line.starts_with("%output "))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!output_lines.contains("HIDDEN_WHILE_OFF"));
    assert!(!output_lines.contains("HIDDEN_WHILE_PAUSED"));
    assert!(output_lines.contains("VISIBLE_AFTER_ON"));
    assert!(output_lines.contains("VISIBLE_AFTER_CONTINUE"));
    let guarded = stream
        .blocks
        .iter()
        .flat_map(|block| block.payload.iter())
        .cloned()
        .collect::<Vec<_>>();
    for state in ["pause", "continue"] {
        let line = format!("%{state} {pane}");
        assert_eq!(
            stream
                .outside
                .iter()
                .filter(|candidate| candidate.as_str() == line)
                .count(),
            1
        );
        assert_eq!(
            guarded
                .iter()
                .filter(|candidate| candidate.as_str() == line)
                .count(),
            0
        );
    }
}

#[test]
fn refresh_client_c_sizes_a_control_target_and_display_menu_returns_at_once() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let (control, mut stdin) =
        fixture.spawn_with_open_stdin(&["-C", "new-session", "-s", "sizing", "exec /bin/cat"]);
    let target = wait_for_control_clients(&fixture, 1, "refresh-client sizing")
        .into_iter()
        .next()
        .expect("control client list row");
    let menu_args = [
        "display-menu",
        "-c",
        target.as_str(),
        "Item",
        "i",
        "display-message chosen",
    ];
    let no_size = fixture.run(&menu_args);
    assert_eq!(no_size.status.code(), Some(0));

    stdin
        .write_all(b"refresh-client -C 100x50\n")
        .expect("set control size");
    stdin.flush().expect("flush control size");
    let expected_width = format!("{target}:100");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let width = fixture.run(&["list-clients", "-F", "#{client_name}:#{client_width}"]);
        if String::from_utf8_lossy(&width.stdout)
            .lines()
            .any(|line| line == expected_width)
        {
            break;
        }
        assert!(Instant::now() < deadline, "control size did not settle");
        thread::sleep(Duration::from_millis(10));
    }

    let sized = fixture.run(&menu_args);
    assert_eq!(sized.status.code(), Some(0));
    stdin.write_all(b"\n").expect("detach sized control");
    drop(stdin);
    let control_output = control.wait_with_output().expect("wait for control client");
    assert_eq!(control_output.status.code(), Some(0));
}

#[test]
fn refresh_client_b_reports_initial_change_and_exact_removal() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let (child, mut stdin) =
        fixture.spawn_with_open_stdin(&["-C", "new-session", "-s", "subscribed", "exec /bin/cat"]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if fixture
            .run(&["has-session", "-t", "subscribed"])
            .status
            .success()
        {
            break;
        }
        assert!(Instant::now() < deadline, "control session did not start");
        thread::sleep(Duration::from_millis(10));
    }

    stdin
        .write_all(b"refresh-client -B 'watch::#{session_name}'\n")
        .expect("add subscription");
    stdin.flush().expect("flush subscription");
    thread::sleep(Duration::from_millis(1200));
    assert_eq!(
        fixture
            .run(&["rename-session", "-t", "subscribed", "renamed"])
            .status
            .code(),
        Some(0)
    );
    thread::sleep(Duration::from_millis(1200));
    stdin
        .write_all(b"refresh-client -B watch\nrename-session -t renamed removed\n")
        .expect("remove subscription and rename session");
    stdin.flush().expect("flush removal");
    thread::sleep(Duration::from_millis(1200));
    stdin.write_all(b"\n").expect("end control input");
    drop(stdin);

    let output = child.wait_with_output().expect("wait for control stream");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    let subscriptions = stream
        .outside
        .iter()
        .filter(|line| line.starts_with("%subscription-changed watch "))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(subscriptions.len(), 2);
    assert!(subscriptions[0].ends_with(" - - - : subscribed"));
    assert!(subscriptions[1].ends_with(" - - - : renamed"));
    assert!(
        subscriptions
            .iter()
            .all(|line| !line.ends_with(" : removed"))
    );
}

#[test]
fn pause_after_emits_extended_output_then_pauses_a_slow_control_client() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let (child, mut stdin) =
        fixture.spawn_with_open_stdin(&["-C", "new-session", "-s", "pause-after", "exec /bin/sh"]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ready = fixture.run(&["has-session", "-t", "pause-after"]);
        if ready.status.success() {
            break;
        }
        assert!(Instant::now() < deadline, "control session did not start");
        thread::sleep(Duration::from_millis(10));
    }
    let pane_output = fixture.run(&["list-panes", "-t", "pause-after", "-F", "#{pane_id}"]);
    assert!(pane_output.status.success());
    let pane = String::from_utf8(pane_output.stdout)
        .expect("pane id")
        .trim()
        .to_owned();
    stdin
        .write_all(b"refresh-client -f pause-after=1\n")
        .expect("enable pause-after");
    stdin.flush().expect("flush pause-after");
    thread::sleep(Duration::from_millis(100));
    let marker = fixture.socket.with_extension("flooded");
    let flood = format!("yes x | head -c 1048576; touch {}", marker.display());
    assert!(
        fixture
            .run(&["send-keys", "-l", "-t", &pane, &flood])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&["send-keys", "-t", &pane, "Enter"])
            .status
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while !marker.exists() {
        assert!(Instant::now() < deadline, "flood never completed");
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(1500));
    stdin.write_all(b"\n").expect("end control input");
    drop(stdin);

    let output = child
        .wait_with_output()
        .expect("wait for paused control stream");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stream = parse_stream(&output.stdout, false);
    assert!(stream.outside.iter().any(|line| {
        let fields = line.splitn(5, ' ').collect::<Vec<_>>();
        fields.first() == Some(&"%extended-output")
            && fields.get(1) == Some(&pane.as_str())
            && fields.get(2).is_some_and(|age| age.parse::<u64>().is_ok())
            && fields.get(3) == Some(&":")
    }));
    assert!(
        stream
            .outside
            .iter()
            .any(|line| line == &format!("%pause {pane}"))
    );
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

fn checked_layout(layout: &str) -> bool {
    layout.split_once(',').is_some_and(|(checksum, body)| {
        checksum.len() == 4
            && checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !body.is_empty()
    })
}
