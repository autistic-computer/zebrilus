use std::{
    ffi::OsString,
    fmt::Write as _,
    fs::File,
    io::{self, Read as _, Write as _},
    os::fd::{AsRawFd as _, FromRawFd as _},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

struct Fixture {
    _directory: tempfile::TempDir,
    socket: PathBuf,
    config: PathBuf,
    home: PathBuf,
}

struct InstalledLayout {
    _directory: tempfile::TempDir,
    path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::Builder::new()
            .prefix("zz-cli-")
            .tempdir_in("/tmp")
            .expect("temporary CLI directory");
        let socket = directory.path().join("daemon.sock");
        let config = directory.path().join("empty.conf");
        std::fs::write(&config, b"").expect("empty mux config");
        let home = directory.path().join("home");
        std::fs::create_dir_all(&home).expect("scratch CLI home");
        Self {
            _directory: directory,
            socket,
            config,
            home,
        }
    }

    fn command_from(&self, executable: &Path) -> Command {
        let mut command = Command::new(executable);
        command
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.home)
            .arg("-f")
            .arg(&self.config)
            .arg("-S")
            .arg(&self.socket);
        command
    }

    fn command(&self) -> Command {
        self.command_from(Path::new(env!("CARGO_BIN_EXE_zz_cli")))
    }

    fn host_command(&self) -> Command {
        let config = self.home.join("zz");
        std::fs::create_dir_all(&config).expect("scratch zz config directory");
        std::fs::write(
            config.join("config"),
            format!("host-peer = unix://{}\n", self.socket.display()),
        )
        .expect("host config");
        let mut command = Command::new(env!("CARGO_BIN_EXE_zz_cli"));
        super::scratch_env(&mut command, &self.home)
            .arg("-f")
            .arg(&self.config)
            .arg("--host")
            .arg("peer");
        command
    }

    fn run(&self, arguments: &[&str]) -> Output {
        self.command()
            .args(arguments)
            .output()
            .expect("run zz command")
    }

    fn run_with_configs(&self, configs: &[&Path], arguments: &[&str]) -> Output {
        let mut command = Command::new(Path::new(env!("CARGO_BIN_EXE_zz_cli")));
        command
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.home);
        for config in configs {
            command.arg("-f").arg(config);
        }
        command
            .arg("-S")
            .arg(&self.socket)
            .args(arguments)
            .output()
            .expect("run zz command")
    }

    fn run_with_stdin(&self, arguments: &[&str], input: &[u8]) -> Output {
        let mut child = self
            .command()
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn zz command");
        let mut stdin = child.stdin.take().expect("piped stdin");
        stdin.write_all(input).expect("write command stdin");
        drop(stdin);
        child.wait_with_output().expect("wait for zz command")
    }

    fn spawn_with_open_stdin(&self, arguments: &[&str]) -> (Child, ChildStdin) {
        let mut child = self
            .command()
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn zz command");
        let stdin = child.stdin.take().expect("piped stdin");
        (child, stdin)
    }

    fn missing_message(&self) -> Vec<u8> {
        format!(
            "error connecting to {} (No such file or directory)\n",
            self.socket.display()
        )
        .into_bytes()
    }

    fn assert_not_started(&self) {
        assert!(!self.socket.exists(), "socket was created");
        assert!(
            !identity_path(&self.socket).exists(),
            "identity was created"
        );
    }

    fn assert_stopped(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let identity = identity_path(&self.socket);
        while (self.socket.exists() || identity.exists()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        self.assert_not_started();
    }
}

impl InstalledLayout {
    fn new() -> Self {
        let directory = tempfile::Builder::new()
            .prefix("zz installed layout fixture ")
            .tempdir_in("/tmp")
            .expect("temporary installed layout directory");
        let install = directory.path().join("installed zz with spaces");
        std::fs::create_dir_all(&install).expect("create installed layout directory");
        let cli = install.join("cli");
        std::fs::copy(env!("CARGO_BIN_EXE_zz_cli"), &cli).expect("copy headless executable");
        let bin = directory.path().join("bin");
        std::fs::create_dir_all(&bin).expect("create installed bin directory");
        let path = bin.join("zz");
        std::os::unix::fs::symlink(&cli, &path).expect("link installed zz executable");
        assert!(path.to_string_lossy().contains(' '));
        Self {
            _directory: directory,
            path,
        }
    }

    fn command(&self, fixture: &Fixture) -> Command {
        let mut command = Command::new(&self.path);
        command
            .env("ZZ_SOCKET", &fixture.socket)
            .env("XDG_CONFIG_HOME", &fixture.home)
            .env("HOME", &fixture.home);
        command
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.run(&["--kill-server"]);
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.socket.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn identity_path(socket: &Path) -> PathBuf {
    let mut path = OsString::from(socket.as_os_str());
    path.push(".identity");
    PathBuf::from(path)
}

fn local_socket_bind_available(path: &Path) -> bool {
    match std::os::unix::net::UnixListener::bind(path) {
        Ok(listener) => {
            drop(listener);
            std::fs::remove_file(path).expect("remove socket capability probe");
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => false,
        Err(error) => panic!("probe Unix socket binding: {error}"),
    }
}

const WIDE_COLUMNS: u16 = 120;

const STOCK_COLUMNS: u16 = 80;

const STOCK_PANE_ROWS: &str = "23";

fn open_pty() -> io::Result<(File, File)> {
    open_pty_sized(80)
}

#[allow(unsafe_code, clippy::undocumented_unsafe_blocks)]
fn open_pty_sized(columns: u16) -> io::Result<(File, File)> {
    let mut master = -1;
    let mut slave = -1;
    let mut size = libc::winsize {
        ws_row: 24,
        ws_col: columns,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    if unsafe {
        libc::openpty(
            &raw mut master,
            &raw mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut size,
        )
    } == -1
    {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) })
}

fn assert_missing(output: &Output, expected: &[u8]) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, expected);
}

#[test]
fn read_only_command_without_a_daemon_uses_the_pin_connect_error() {
    let fixture = Fixture::new();
    let output = fixture.run(&["list-sessions"]);
    assert_missing(&output, &fixture.missing_message());
    fixture.assert_not_started();
}

#[test]
fn daemon_panes_own_their_tty_and_report_the_program_they_exec() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) || !Path::new("/bin/zsh").exists() {
        eprintln!("SKIPPED: needs Unix socket binding and /bin/zsh");
        return;
    }
    std::fs::write(&fixture.config, b"set -g default-shell /bin/zsh\n").expect("zsh default shell");
    let created = fixture.run(&[
        "new-session",
        "-d",
        "-s",
        "tty",
        "-x",
        "80",
        "-y",
        "10",
        "exec /bin/zsh -fi",
    ]);
    assert_eq!(created.status.code(), Some(0), "{created:?}");
    let screen = || {
        String::from_utf8_lossy(&fixture.run(&["capture-pane", "-p", "-t", "tty"]).stdout)
            .into_owned()
    };
    let wait = |what: &str, ready: &dyn Fn() -> bool| {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !ready() {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}: {}",
                screen()
            );
            thread::sleep(Duration::from_millis(20));
        }
    };
    let keys = |arguments: &[&str]| {
        let mut command = vec!["send-keys", "-t", "tty"];
        command.extend_from_slice(arguments);
        assert_eq!(fixture.run(&command).status.code(), Some(0));
    };
    keys(&["[[ -o monitor ]] && print ZZ_MONITOR_ON", "Enter"]);
    wait("job control", &|| screen().contains("\nZZ_MONITOR_ON"));
    keys(&["sleep 30", "Enter"]);
    wait("sleep in the foreground", &|| {
        String::from_utf8_lossy(
            &fixture
                .run(&[
                    "display-message",
                    "-p",
                    "-t",
                    "tty",
                    "#{pane_current_command}",
                ])
                .stdout,
        )
        .trim()
            == "sleep"
    });
    let interrupted = Instant::now();
    keys(&["C-c"]);
    keys(&["print ZZ_BACK", "Enter"]);
    wait("the prompt after C-c", &|| screen().contains("\nZZ_BACK"));
    assert!(interrupted.elapsed() < Duration::from_secs(15));

    for window in 1..=5 {
        let created = fixture.run(&["new-window", "-d", "-t", "tty:", "sleep", "30"]);
        assert_eq!(created.status.code(), Some(0), "{created:?}");
        let target = format!("tty:{window}");
        wait("a silent pane to report its program", &|| {
            String::from_utf8_lossy(
                &fixture
                    .run(&[
                        "display-message",
                        "-p",
                        "-t",
                        &target,
                        "#{pane_current_command}",
                    ])
                    .stdout,
            )
            .trim()
                == "sleep"
        });
    }
}

#[test]
fn the_first_pane_of_a_new_daemon_names_its_window_after_its_program() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "first", "sleep", "30"]);
    assert_eq!(created.status.code(), Some(0), "{created:?}");
    let format = |format: &str| {
        String::from_utf8_lossy(
            &fixture
                .run(&["display-message", "-p", "-t", "first", format])
                .stdout,
        )
        .trim()
        .to_owned()
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while format("#{pane_current_command}") != "sleep" {
        assert!(Instant::now() < deadline, "the pane did not exec sleep");
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(format("#{window_name}"), "sleep");
}

fn ready_then_window_hook(fixture: &Fixture, mut events: Command) -> Vec<serde_json::Value> {
    use std::io::BufRead as _;

    let created = fixture.run(&["new-session", "-d", "-s", "events"]);
    assert_eq!(created.status.code(), Some(0));
    let mut child = events
        .arg("events")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start event stream");
    let stdout = child.stdout.take().expect("event stdout");
    let (sender, receiver) = mpsc::sync_channel(32);
    let reader = thread::spawn(move || {
        for line in io::BufReader::new(stdout).lines() {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let result = (|| -> Result<Vec<serde_json::Value>, String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let next = || -> Result<serde_json::Value, String> {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or("event deadline elapsed")?;
            let line = receiver
                .recv_timeout(remaining)
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
            serde_json::from_str(&line).map_err(|error| error.to_string())
        };
        let ready = next()?;
        if ready["event"] != "ready" || ready["seq"] != 0 {
            return Err(format!("expected ready first: {ready}"));
        }
        let created = fixture.run(&["new-window"]);
        if !created.status.success() {
            return Err(format!("new-window failed: {:?}", created.stderr));
        }
        let mut lines = vec![ready];
        for _ in 0..20 {
            let event = next()?;
            let window = matches!(
                event["event"].as_str(),
                Some("window-linked" | "session-window-changed")
            );
            lines.push(event);
            if window {
                return Ok(lines);
            }
        }
        Err("no window hook in 20 lines".to_owned())
    })();
    let _ = child.kill();
    let _ = child.wait();
    drop(receiver);
    reader.join().expect("event reader");
    result.expect("ready then window hook within 10 seconds")
}

#[test]
fn events_stream_ready_then_window_hook() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        eprintln!("SKIPPED: Unix socket binding is unavailable");
        return;
    }
    let lines = ready_then_window_hook(&fixture, fixture.command());
    assert!(
        lines.iter().all(|line| line.get("host").is_none()),
        "{lines:?}"
    );
}

#[test]
fn events_over_a_host_name_it_on_every_line() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        eprintln!("SKIPPED: Unix socket binding is unavailable");
        return;
    }
    let lines = ready_then_window_hook(&fixture, fixture.host_command());
    assert!(lines.iter().all(|line| line["host"] == "peer"), "{lines:?}");
}

#[test]
fn agent_send_progress_streams_over_a_host() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        eprintln!("SKIPPED: Unix socket binding is unavailable");
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "agent"])
            .status
            .success()
    );
    let state = |value: &str| {
        assert!(
            fixture
                .run(&["set-option", "-p", "-t", "%0", "@agent_state", value])
                .status
                .success()
        );
    };
    state("idle");
    let child = fixture
        .host_command()
        .args([
            "agent-send",
            "-t",
            "%0",
            "--wait",
            "--progress",
            "--timeout",
            "10",
            "hi",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("agent-send over the host");
    thread::sleep(Duration::from_millis(500));
    state("working");
    thread::sleep(Duration::from_millis(1200));
    state("idle");
    let output = child.wait_with_output().expect("agent-send result");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("[zz %0 +00:0"), "{stderr}");
    assert!(stderr.contains("] working"), "{stderr}");
    assert!(!stderr.contains("local only"), "{stderr}");
}

#[test]
fn host_local_verbs_refuse_a_local_socket_host() {
    let fixture = Fixture::new();
    for verb in [
        &["fleet", "list"][..],
        &["list-hosts"],
        &["tools", "--context"],
        &["tool", "--install-hooks"],
        &["tool", "--remove-hooks"],
        &["protocol-version"],
    ] {
        let output = fixture
            .host_command()
            .args(verb)
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("CODEX_HOME")
            .stdin(Stdio::null())
            .output()
            .expect("host-local verb");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{verb:?}: {stderr}");
        assert!(
            stderr.starts_with("zz: peer is the local socket "),
            "{verb:?}: {stderr}"
        );
    }
    for written in [".claude", ".codex", ".agents"] {
        assert!(!fixture.home.join(written).exists(), "{written}");
    }
}

#[test]
fn control_mode_runs_over_a_host() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        eprintln!("SKIPPED: Unix socket binding is unavailable");
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "remote"])
            .status
            .success()
    );
    let output = fixture
        .host_command()
        .args(["-C", "list-sessions", "-F", "#{session_name}"])
        .stdin(Stdio::null())
        .output()
        .expect("control mode over the host");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("%begin "), "{stdout}");
    assert!(stdout.contains("\nremote\n%end "), "{stdout}");
}

#[test]
fn events_session_filter_passes_pane_events_of_that_session_only() {
    use std::io::BufRead as _;

    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        eprintln!("SKIPPED: Unix socket binding is unavailable");
        return;
    }
    for name in ["mine", "other"] {
        let created = fixture.run(&["new-session", "-d", "-s", name]);
        assert_eq!(created.status.code(), Some(0));
    }
    let ids = fixture.run(&[
        "display-message",
        "-p",
        "-t",
        "mine",
        "#{session_id} #{pane_id}",
    ]);
    let ids = String::from_utf8(ids.stdout).expect("session and pane ids");
    let (session, pane) = ids.trim().split_once(' ').expect("session and pane ids");
    let mut child = fixture
        .command()
        .args(["events", "-t", session])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start event stream");
    let stdout = child.stdout.take().expect("event stdout");
    let (sender, receiver) = mpsc::sync_channel(32);
    let reader = thread::spawn(move || {
        for line in io::BufReader::new(stdout).lines() {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let result = (|| -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let next = || -> Result<serde_json::Value, String> {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or("event deadline elapsed")?;
            let line = receiver
                .recv_timeout(remaining)
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
            serde_json::from_str(&line).map_err(|error| error.to_string())
        };
        let ready = next()?;
        if ready["event"] != "ready" {
            return Err(format!("expected ready first: {ready}"));
        }
        for (target, title) in [("other:", "title-other"), ("mine:", "title-mine")] {
            let titled = fixture.run(&["select-pane", "-t", target, "-T", title]);
            if !titled.status.success() {
                return Err(format!("select-pane failed: {:?}", titled.stderr));
            }
        }
        for _ in 0..20 {
            let event = next()?;
            if event["event"] != "pane-title-changed" {
                continue;
            }
            if event.get("hook_session").is_some() {
                return Err(format!("pane event kept hook_session: {event}"));
            }
            if event["hook_pane"] != pane {
                return Err(format!("event from another session passed: {event}"));
            }
            if event["hook_new_title"] == "title-mine" {
                return Ok(());
            }
        }
        Err("no pane-title-changed in 20 lines".to_owned())
    })();
    let _ = child.kill();
    let _ = child.wait();
    drop(receiver);
    reader.join().expect("event reader");
    result.expect("only the filtered session's pane event");
}

#[test]
fn pane_json_contains_format_ids_and_native_kind() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        eprintln!("SKIPPED: Unix socket binding is unavailable");
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "json"]);
    assert_eq!(created.status.code(), Some(0));
    let output = fixture.run(&["list-panes", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).expect("UTF-8 JSON output");
    let rows = text.lines().collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    let row: serde_json::Value = serde_json::from_str(rows[0]).expect("pane JSON object");
    assert_eq!(row["pane_id"], "%0");
    assert_eq!(row["pane_kind"], "terminal");
    assert!(
        row.as_object()
            .unwrap()
            .values()
            .all(serde_json::Value::is_string)
    );
}

#[test]
fn host_flag_guards_input_and_fans_read_only_chains_out_to_every_host() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        eprintln!("SKIPPED: Unix socket binding is unavailable");
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "fleet"]);
    assert_eq!(created.status.code(), Some(0));
    let config = fixture.home.join("zz/config");
    std::fs::create_dir_all(config.parent().unwrap()).expect("config directory");
    std::fs::write(
        &config,
        format!(
            "tailnet = off\nhost-peer = unix://{}\nhost-gone = unix://{}/gone.sock\n",
            fixture.socket.display(),
            fixture.home.display()
        ),
    )
    .expect("write host config");
    let zz = |arguments: &[&str]| {
        super::scratch_env(
            &mut Command::new(env!("CARGO_BIN_EXE_zz_cli")),
            &fixture.home,
        )
        .env_remove("TMUX")
        .env_remove("ZZ_SOCKET")
        .args(arguments)
        .output()
        .expect("run zz --host")
    };

    let untargeted = zz(&["--host", "peer", "send-keys", "x"]);
    assert_eq!(untargeted.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&untargeted.stderr),
        "zz --host peer: name the pane with -t %N (zz --host peer list-panes -a, or make one \
         with zz --host peer new-session -d -P -F '#{pane_id}')\n"
    );
    let targeted = zz(&["--host", "peer", "send-keys", "-t", "%0", "x"]);
    assert_eq!(targeted.status.code(), Some(0), "{targeted:?}");
    let payload = zz(&["--host", "peer", "display-message", "-p", "--", "--host"]);
    assert_eq!(payload.stdout, b"--host\n", "{payload:?}");
    let bare = zz(&["--host", "peer"]);
    assert_eq!(bare.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&bare.stderr),
        "open terminal failed: not a terminal\n"
    );

    let text = zz(&["--host", "all", "list-sessions", "-F", "#{session_name}"]);
    assert_eq!(text.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&text.stdout), "peer: fleet\n");
    let errors = String::from_utf8_lossy(&text.stderr);
    assert!(errors.starts_with("gone: can't reach gone: "), "{errors}");
    assert_eq!(errors.lines().count(), 1, "{errors}");

    let json = zz(&["--host", "all", "inspect", "-a", "--json"]);
    let rows = String::from_utf8(json.stdout).expect("UTF-8 JSON rows");
    let row: serde_json::Value =
        serde_json::from_str(rows.lines().next().expect("one pane row")).expect("JSON row");
    assert_eq!(row["host_name"], "peer");
    assert_eq!(row["pane_id"], "%0");

    for mutating in [
        &["--host", "all", "send-keys", "-t", "%0", "x"][..],
        &["--host", "all", "list-panes", ";", "kill-server"][..],
        &["--host", "all"][..],
    ] {
        let refused = zz(mutating);
        assert_eq!(refused.status.code(), Some(2), "{mutating:?}");
        assert!(refused.stdout.is_empty(), "{mutating:?}");
        assert!(
            String::from_utf8_lossy(&refused.stderr).contains("read-only"),
            "{mutating:?}"
        );
    }
    for (usage, status) in [
        (&["--host", "all", "bogus"][..], 1),
        (&["--host", "all", "list-panes", "-Z"][..], 1),
        (&["--host", "all", "inspect", "--bogus"][..], 2),
    ] {
        let refused = zz(usage);
        assert_eq!(refused.status.code(), Some(status), "{usage:?}");
        let errors = String::from_utf8_lossy(&refused.stderr);
        assert_eq!(errors.lines().count(), 1, "{usage:?} {errors}");
        assert!(
            !errors.contains("peer") && !errors.contains("can change state"),
            "{usage:?} {errors}"
        );
    }
    let sessions = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(sessions.stdout, b"fleet\n");
}

#[test]
fn pane_mode_syntax_errors_keep_the_pinned_exit_status_with_a_daemon() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "mode-errors"])
            .status
            .success()
    );
    for (arguments, expected) in [
        (
            &["choose-client", "-Q"][..],
            "command choose-client: unknown flag -Q\n",
        ),
        (
            &["choose-client", "one", "two"][..],
            "command choose-client: too many arguments (need at most 1)\n",
        ),
        (
            &["clock-mode", "-Q"][..],
            "command clock-mode: unknown flag -Q\n",
        ),
        (
            &["switch-mode", "one", "two"][..],
            "command switch-mode: too many arguments (need at most 1)\n",
        ),
        (
            &["refresh-client", "-r"][..],
            "command refresh-client: -r expects an argument\n",
        ),
    ] {
        let output = fixture.run(arguments);
        assert_eq!(output.status.code(), Some(1), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        assert_eq!(output.stderr, expected.as_bytes(), "{arguments:?}");
    }
}

#[test]
fn inspect_json_contains_pane_facts_verbs_and_events() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        eprintln!("SKIPPED: Unix socket binding is unavailable");
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "inspect"]);
    assert_eq!(created.status.code(), Some(0));
    let output = fixture.run(&["inspect", "--json", "-t", "%0"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).expect("UTF-8 JSON output");
    let rows = text.lines().collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    let row: serde_json::Value = serde_json::from_str(rows[0]).expect("inspect JSON object");
    assert_eq!(row["session_id"], "$0");
    assert_eq!(row["session_name"], "inspect");
    assert_eq!(row["window_id"], "@0");
    assert_eq!(row["window_size"], "latest");
    assert_eq!(row["pane_id"], "%0");
    assert_eq!(row["pane_kind"], "terminal");
    assert!(
        !row["verbs"]
            .as_array()
            .unwrap()
            .contains(&"select-pane-kind".into())
    );
    assert!(
        row["verbs"]
            .as_array()
            .unwrap()
            .contains(&"wait-pane".into())
    );
    assert!(
        row["events"]
            .as_array()
            .unwrap()
            .contains(&"pane-exited".into())
    );
    assert_eq!(row.as_object().unwrap().len(), 43);
    for (key, value) in row.as_object().unwrap() {
        if matches!(key.as_str(), "verbs" | "events") {
            assert!(
                value
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(serde_json::Value::is_string)
            );
        } else if key == "permission" {
            assert!(value.is_null());
        } else {
            assert!(value.is_string(), "{key}");
        }
    }
}

#[test]
fn json_and_format_conflict_exits_two_with_a_daemon() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        eprintln!("SKIPPED: Unix socket binding is unavailable");
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "json-conflict"]);
    assert_eq!(created.status.code(), Some(0));
    let output = fixture.run(&["list-sessions", "-F", "x", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--json"));
}

#[test]
fn usage_errors_keep_their_surface_status_with_a_daemon() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "usage-contract"])
            .status
            .success()
    );
    for (args, status) in [
        (&["refresh-client", "-X"][..], 1),
        (&["choose-tree", "-Q"][..], 1),
        (&["list-panes", "-Z"][..], 1),
        (&["set-option"][..], 1),
        (&["new-window", "-Q"][..], 1),
        (&["display-message", "-Q"][..], 1),
        (&["kill-pane", "-Q"][..], 1),
        (&["rename-window"][..], 1),
        (&["bogus-verb"][..], 1),
        (&["clock-mode", "-Q"][..], 1),
        (&["list-panes", "--json", "-Z"][..], 1),
        (&["list-panes", "--json", "-F"][..], 1),
        (&["list-panes", "--json", "-F", "x"][..], 2),
        (&["list-panes", "--json=true"][..], 2),
        (&["reload-config", "extra"][..], 2),
        (&["set-option", "-a", "history-trickle", "1"][..], 2),
        (
            &["set-option", "-a", "experimental-editor-pane", "on"][..],
            2,
        ),
        (&["split-window", "--kind", "browser", "-I"][..], 2),
        (&["split-window", "--profile", "x"][..], 2),
        (&["agent-catalog", "-Q"][..], 2),
        (&["agent-catalog", "claude", "not-a-number"][..], 2),
        (&["set-browser-url"][..], 2),
    ] {
        let output = fixture.run(args);
        assert_eq!(
            output.status.code(),
            Some(status),
            "{args:?}: {:?}",
            output.stderr
        );
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(!output.stderr.is_empty(), "{args:?}");
    }
}

#[test]
fn new_session_immediately_after_kill_server_starts_a_fresh_daemon() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let first = fixture.run(&["new-session", "-d", "-s", "a"]);
    assert_eq!(first.status.code(), Some(0));
    let killed = fixture.run(&["kill-server"]);
    assert_eq!(killed.status.code(), Some(0));
    assert!(killed.stdout.is_empty());
    assert!(killed.stderr.is_empty());
    let second = fixture.run(&["new-session", "-d", "-s", "b"]);
    assert_eq!(
        second.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let sessions = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(sessions.status.code(), Some(0));
    assert_eq!(sessions.stdout, b"b\n");
}

#[test]
fn non_start_server_commands_do_not_spawn_a_daemon() {
    let fixture = Fixture::new();
    for command in [
        "ls",
        "list-panes",
        "show-options",
        "kill-server",
        "--kill-server",
    ] {
        let output = fixture.run(&[command]);
        assert_missing(&output, &fixture.missing_message());
        fixture.assert_not_started();
    }
    let output = fixture.run(&["source-file"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"command source-file: too few arguments (need at least 1)\n"
    );
    fixture.assert_not_started();
}

#[test]
fn detached_new_session_still_autostarts_the_daemon() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "autostart"]);
    assert_eq!(created.status.code(), Some(0));
    assert!(created.stdout.is_empty());
    assert!(created.stderr.is_empty());
    assert!(fixture.socket.exists());

    let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(listed.status.code(), Some(0));
    assert_eq!(listed.stdout, b"autostart\n");
    assert!(listed.stderr.is_empty());
}

#[test]
fn autostarted_daemon_does_not_inherit_client_descriptors() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let marker = File::open(&fixture.config).expect("open inherited marker");
    rustix::io::fcntl_setfd(&marker, rustix::io::FdFlags::empty())
        .expect("make marker inheritable");
    let descriptor = marker.as_raw_fd();
    let created = fixture.run(&["new-session", "-d", "-s", "descriptors"]);
    drop(marker);
    assert_eq!(created.status.code(), Some(0));

    let probe = fixture.run(&[
        "run-shell",
        &format!("[ -e /dev/fd/{descriptor} ] && echo inherited || echo clean"),
    ]);
    assert_eq!(probe.status.code(), Some(0));
    assert_eq!(probe.stdout, b"clean\n");
}

#[test]
fn startup_config_new_session_is_forced_detached() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    std::fs::write(
        &fixture.config,
        b"new-session -s fromconfig\nnew-session -A -s fromconfig\nattach-session -t bogus\nnew-session -d -s after-clientless-attach\nnew-session -d -s configured-extra\nif-shell -F 1 'new-session -s fromconditional'\n",
    )
    .expect("write new-session startup config");

    let created = fixture.run(&["new-session", "-d", "-s", "command-extra"]);
    assert_eq!(created.status.code(), Some(0));
    assert!(created.stdout.is_empty());
    assert!(created.stderr.is_empty());

    let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(listed.status.code(), Some(0));
    assert_eq!(
        listed.stdout,
        b"after-clientless-attach\ncommand-extra\nconfigured-extra\nfromconditional\nfromconfig\n"
    );
    assert!(listed.stderr.is_empty());
}

#[test]
fn cold_autospawn_uses_the_launching_client_cwd_for_startup_sources() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let caller_cwd = source_directory(&fixture, "startup client cwd [literal]*? with spaces");
    let caller_entry_directory = caller_cwd.join("a");
    std::fs::create_dir(&caller_entry_directory).expect("caller entry directory");
    std::fs::write(
        caller_cwd.join("startup-top.conf"),
        b"set-option -g @startup_client_cwd_top caller-root\n",
    )
    .expect("caller top-level startup source");
    std::fs::write(
        caller_entry_directory.join("entry.conf"),
        b"set-option -g @startup_client_cwd_entry caller-entry\nsource-file startup-leaf.conf\n",
    )
    .expect("caller nested startup entry");
    std::fs::write(
        caller_cwd.join("startup-leaf.conf"),
        b"set-option -g @startup_client_cwd_leaf caller-root\n",
    )
    .expect("caller-root nested startup source");
    std::fs::write(
        caller_entry_directory.join("startup-leaf.conf"),
        b"set-option -g @startup_client_cwd_leaf containing-file-decoy\n",
    )
    .expect("containing-file startup decoy");

    let config_directory = fixture.config.parent().expect("fixture config directory");
    let config_entry_directory = config_directory.join("a");
    std::fs::create_dir(&config_entry_directory).expect("config entry directory");
    std::fs::write(
        config_directory.join("startup-top.conf"),
        b"set-option -g @startup_client_cwd_top config-decoy\n",
    )
    .expect("config top-level startup decoy");
    std::fs::write(
        config_entry_directory.join("entry.conf"),
        b"set-option -g @startup_client_cwd_entry config-decoy\n",
    )
    .expect("config nested startup decoy");
    std::fs::write(
        &fixture.config,
        b"source-file startup-top.conf\nsource-file a/entry.conf\n",
    )
    .expect("startup source config");

    let started = fixture
        .command()
        .current_dir(&caller_cwd)
        .args(["new-session", "-d", "-s", "startup-client-cwd"])
        .output()
        .expect("cold autospawn from startup client cwd");
    assert_eq!(
        started.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&started.stderr)
    );
    assert!(started.stdout.is_empty());
    assert!(started.stderr.is_empty());

    for (option, expected) in [
        ("@startup_client_cwd_top", b"caller-root\n" as &[u8]),
        ("@startup_client_cwd_entry", b"caller-entry\n"),
        ("@startup_client_cwd_leaf", b"caller-root\n"),
    ] {
        let shown = fixture.run(&["show-options", "-gqv", option]);
        assert_eq!(shown.status.code(), Some(0), "{option}");
        assert_eq!(shown.stdout, expected, "{option}");
        assert!(shown.stderr.is_empty(), "{option}");
    }
}

#[test]
fn startup_config_shares_a_fifty_source_command_budget() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let directory = fixture
        .config
        .parent()
        .expect("fixture config directory")
        .join("startup-sources");
    std::fs::create_dir_all(&directory).expect("startup source directory");
    let source = |first: usize, last: usize| {
        let mut config = String::new();
        for index in first..=last {
            let leaf = directory.join(format!("leaf{index}.conf"));
            std::fs::write(&leaf, format!("set-option -g @startup{index} yes\n"))
                .expect("startup source leaf");
            let _ = writeln!(config, "source-file '{}'", leaf.display());
        }
        config
    };
    std::fs::write(&fixture.config, source(1, 45)).expect("first startup config");
    let second = directory.join("second-root.conf");
    let mut tail = source(46, 60);
    tail.push_str("set-option -g @startup-after yes\n");
    std::fs::write(&second, tail).expect("second startup config");

    let started = fixture.run_with_configs(
        &[fixture.config.as_path(), second.as_path()],
        &["new-session", "-d", "-s", "startup-depth"],
    );
    assert_eq!(started.status.code(), Some(0));
    assert!(started.stdout.is_empty());
    assert!(started.stderr.is_empty());
    assert_eq!(
        fixture.run(&["show-options", "-gqv", "@startup45"]).stdout,
        b"yes\n"
    );
    assert_eq!(
        fixture.run(&["show-options", "-gqv", "@startup50"]).stdout,
        b"yes\n"
    );
    assert!(
        fixture
            .run(&["show-options", "-gqv", "@startup51"])
            .stdout
            .is_empty()
    );
    assert_eq!(
        fixture
            .run(&["show-options", "-gqv", "@startup-after"])
            .stdout,
        b"yes\n"
    );
}

#[test]
fn event_hooks_are_clientless_and_command_hooks_retain_the_command_client() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    std::fs::write(
        &fixture.config,
        b"set-hook -g session-created 'new-session -s fromevent'\nset-hook -g after-new-session 'new-session -s fromcommand'\n",
    )
    .expect("write hook client config");

    let created = fixture.run(&["new-session", "-d", "-s", "base"]);
    assert_eq!(created.status.code(), Some(0));
    assert!(created.stdout.is_empty());
    assert!(created.stderr.is_empty());

    let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(listed.status.code(), Some(0));
    if listed.stdout != b"base\nfromevent\n" {
        let deadline = Instant::now() + Duration::from_secs(10);
        let later = loop {
            let later = fixture
                .run(&["list-sessions", "-F", "#{session_name}"])
                .stdout;
            if later != listed.stdout || Instant::now() >= deadline {
                break later;
            }
            thread::sleep(Duration::from_millis(20));
        };
        panic!(
            "list-sessions right after new-session: {:?}; after polling up to 10 s: {:?}",
            String::from_utf8_lossy(&listed.stdout),
            String::from_utf8_lossy(&later),
        );
    }
    assert!(listed.stderr.is_empty());
}

#[test]
fn switch_client_from_a_plain_shell_reports_the_pinned_client_errors() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "w"]);
    assert_eq!(created.status.code(), Some(0));

    let clientless = fixture.run(&["switch-client", "-t", "w"]);
    assert_eq!(clientless.status.code(), Some(1));
    assert_eq!(clientless.stderr, b"no current client\n");

    let unknown = fixture.run(&["switch-client", "-c", "bogus:", "-t", "w"]);
    assert_eq!(unknown.status.code(), Some(1));
    assert_eq!(unknown.stderr, b"can't find client: bogus\n");
}

#[test]
fn runtime_config_commands_keep_the_command_client_terminal_state() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let started = fixture.run(&["start-server"]);
    assert_eq!(started.status.code(), Some(0));
    let runtime_config = fixture.config.with_file_name("runtime.conf");
    std::fs::write(&runtime_config, b"new-session -s sourced\n")
        .expect("write runtime source config");

    let _ = fixture.run(&[
        "source-file",
        runtime_config.to_str().expect("UTF-8 runtime config path"),
    ]);

    let conditional = fixture.run(&["if-shell", "-F", "1", "new-session -s conditional"]);
    assert_eq!(conditional.status.code(), Some(1));
    assert_eq!(
        conditional.stderr,
        b"open terminal failed: not a terminal\n"
    );

    let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(listed.status.code(), Some(0));
    assert!(listed.stdout.is_empty());
    assert!(listed.stderr.is_empty());
}

fn source_directory(fixture: &Fixture, name: &str) -> PathBuf {
    let directory = fixture.config.with_file_name(name);
    std::fs::create_dir_all(&directory).expect("source fixture directory");
    directory
}

fn write_source(directory: &Path, name: &str, body: &str) -> String {
    let path = directory.join(name);
    std::fs::write(&path, body).expect("write source fixture");
    path.to_str().expect("UTF-8 source fixture path").to_owned()
}

fn write_source_chain(directory: &Path, invocations: usize) -> String {
    let leaf = directory.join("leaf.conf");
    write_source_chain_with_deepest(
        directory,
        invocations,
        &format!(
            "source-file '{}'\nsource-file -q '{}'\n",
            leaf.display(),
            leaf.display()
        ),
    )
}

fn write_source_chain_with_deepest(directory: &Path, invocations: usize, deepest: &str) -> String {
    for level in 1..=invocations {
        let nested = if level < invocations {
            format!(
                "source-file '{}'\n",
                directory.join(format!("f{}.conf", level + 1)).display()
            )
        } else {
            deepest.to_owned()
        };
        let body =
            format!("set-option -g @depth {level}\n{nested}set-option -g @after{level} yes\n");
        write_source(directory, &format!("f{level}.conf"), &body);
    }
    write_source(
        directory,
        "leaf.conf",
        &format!("set-option -g @leaf{invocations} yes\n"),
    );
    directory
        .join("f1.conf")
        .to_str()
        .expect("UTF-8 chain entry")
        .to_owned()
}

#[test]
fn detached_new_session_accepts_dash_dimensions() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }

    let created = fixture.run(&["new-session", "-d", "-s", "dash-size", "-x", "-", "-y", "-"]);
    assert_eq!(created.status.code(), Some(0));
    assert!(created.stdout.is_empty());
    assert!(created.stderr.is_empty());

    let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(listed.status.code(), Some(0));
    assert_eq!(listed.stdout, b"dash-size\n");
    assert!(listed.stderr.is_empty());
}

#[test]
fn headless_attaching_new_session_fails_without_creating_the_session() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }

    let created = fixture.run(&["new-session", "-s", "headless"]);
    assert_eq!(created.status.code(), Some(1));
    assert!(created.stdout.is_empty());
    assert_eq!(created.stderr, b"open terminal failed: not a terminal\n");

    let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(listed.status.code(), Some(0));
    assert!(listed.stdout.is_empty());
    assert!(listed.stderr.is_empty());
}

#[test]
fn tty_new_session_error_is_bare_and_does_not_start_the_browser() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let Ok((mut master, slave)) = open_pty() else {
        return;
    };
    rustix::io::ioctl_fionbio(&master, true).expect("set pty master nonblocking");
    let stdin = slave.try_clone().expect("clone pty stdin");
    let mut child = fixture
        .command()
        .args(["new-session", "-s", "tty-width", "-x", "0"])
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(slave))
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn TTY new-session error");

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stdout = Vec::new();
    let status = loop {
        let mut buffer = [0_u8; 4096];
        match master.read(&mut buffer) {
            Ok(0) => {}
            Ok(count) => stdout.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => {}
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                drop(master);
                panic!("poll TTY new-session error: {error}");
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            drop(master);
            panic!("TTY new-session error did not exit before deadline");
        }
        thread::sleep(Duration::from_millis(5));
    };
    drop(master);
    let mut stderr = Vec::new();
    child
        .stderr
        .take()
        .expect("piped TTY error stderr")
        .read_to_end(&mut stderr)
        .expect("read TTY error stderr");

    assert_eq!(status.code(), Some(1));
    assert!(stdout.is_empty());
    assert_eq!(stderr, b"width too small\n");

    let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(listed.status.code(), Some(0));
    assert!(listed.stdout.is_empty());
    assert!(listed.stderr.is_empty());
}

#[test]
fn duplicate_new_session_error_is_exact_and_precedes_the_terminal_check() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }

    let first = fixture.run(&["new-session", "-d", "-s", "dup"]);
    assert_eq!(first.status.code(), Some(0));

    let detached_duplicate = fixture.run(&["new-session", "-d", "-s", "dup"]);
    assert_eq!(detached_duplicate.status.code(), Some(1));
    assert!(detached_duplicate.stdout.is_empty());
    assert_eq!(detached_duplicate.stderr, b"duplicate session: dup\n");

    let attaching_duplicate = fixture.run(&["new-session", "-s", "dup"]);
    assert_eq!(attaching_duplicate.status.code(), Some(1));
    assert!(attaching_duplicate.stdout.is_empty());
    assert_eq!(attaching_duplicate.stderr, b"duplicate session: dup\n");
}

#[test]
fn headless_dash_a_ignores_detach_for_existing_but_not_new_sessions() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }

    let existing = fixture.run(&["new-session", "-d", "-s", "existing"]);
    assert_eq!(existing.status.code(), Some(0));

    let attaching = fixture.run(&["new-session", "-A", "-d", "-s", "existing"]);
    assert_eq!(attaching.status.code(), Some(1));
    assert!(attaching.stdout.is_empty());
    assert_eq!(attaching.stderr, b"open terminal failed: not a terminal\n");

    let fresh = fixture.run(&["new-session", "-A", "-d", "-s", "fresh"]);
    assert_eq!(fresh.status.code(), Some(0));
    assert!(fresh.stdout.is_empty());
    assert!(fresh.stderr.is_empty());

    let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(listed.status.code(), Some(0));
    assert_eq!(listed.stdout, b"existing\nfresh\n");
    assert!(listed.stderr.is_empty());
}

#[test]
fn detached_new_session_prints_default_and_custom_formats() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }

    let default = fixture.run(&["new-session", "-P", "-d", "-s", "printed"]);
    assert_eq!(default.status.code(), Some(0));
    assert_eq!(default.stdout, b"printed:\n");
    assert!(default.stderr.is_empty());

    let formatted = fixture.run(&[
        "new-session",
        "-P",
        "-d",
        "-F",
        "#{session_name}/#{window_index}",
        "-s",
        "formatted",
    ]);
    assert_eq!(formatted.status.code(), Some(0));
    assert_eq!(formatted.stdout, b"formatted/0\n");
    assert!(formatted.stderr.is_empty());

    let empty = fixture.run(&["new-window", "-d", "-t", "formatted:", "-P", "-F", ""]);
    assert_eq!(empty.status.code(), Some(0));
    assert_eq!(empty.stdout, b"\n");
    assert!(empty.stderr.is_empty());

    let trailing_newline =
        fixture.run(&["new-window", "-d", "-t", "formatted:", "-P", "-F", "X\n"]);
    assert_eq!(trailing_newline.status.code(), Some(0));
    assert_eq!(trailing_newline.stdout, b"X\n\n");
    assert!(trailing_newline.stderr.is_empty());

    let split = fixture.run(&["split-window", "-d", "-t", "formatted:0"]);
    assert_eq!(split.status.code(), Some(0));
    assert!(split.stdout.is_empty());
    assert!(split.stderr.is_empty());

    let broken = fixture.run(&["break-pane", "-d", "-P", "-s", "formatted:0.1", "-F", "X\n"]);
    assert_eq!(broken.status.code(), Some(0));
    assert_eq!(broken.stdout, b"X\n\n");
    assert!(broken.stderr.is_empty());
}

#[test]
fn attaching_new_session_inside_a_later_alias_enters_the_alternate_screen() {
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
                "go=new-session -s pty-attached ; split-window -h",
            ])
            .status
            .success()
    );
    let Ok((mut master, slave)) = open_pty() else {
        return;
    };
    let stdin = slave.try_clone().expect("clone pty stdin");
    let stdout = slave.try_clone().expect("clone pty stdout");
    let mut command = fixture.command();
    command
        .args(["new-session", "-d", "-s", "chain-before", ";", "go"])
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(slave));
    let mut child = command.spawn().expect("spawn attaching new-session");

    let (bytes_sender, bytes_receiver) = mpsc::channel();
    let (stop_sender, stop_receiver) = mpsc::channel();
    let (reader_done_sender, reader_done_receiver) = mpsc::channel();
    rustix::io::ioctl_fionbio(&master, true).expect("set pty master nonblocking");
    let reader = thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match master.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if bytes_sender.send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => break,
            }
            if stop_receiver.try_recv().is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        drop(master);
        let _ = reader_done_sender.send(());
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut captured = Vec::new();
    let mut early_status = None;
    let entered_alternate_screen = loop {
        if captured
            .windows(b"\x1b[?1049h".len())
            .any(|window| window == b"\x1b[?1049h")
        {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        if let Some(status) = child.try_wait().expect("poll attaching new-session") {
            early_status = Some(status);
            break false;
        }
        match bytes_receiver.recv_timeout(Duration::from_millis(20)) {
            Ok(bytes) => captured.extend_from_slice(&bytes),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break false,
        }
    };

    let _ = child.kill();
    let _ = child.wait();
    let _ = stop_sender.send(());
    reader_done_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("pty reader stopped before deadline");
    reader.join().expect("join pty reader");

    assert!(
        entered_alternate_screen,
        "child exited early={early_status:?}; pty output={}",
        String::from_utf8_lossy(&captured),
    );
    let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(listed.status.code(), Some(0));
    assert_eq!(listed.stdout, b"chain-before\npty-attached\nseed\n");
    assert!(listed.stderr.is_empty());
    let panes = fixture.run(&["list-panes", "-t", "pty-attached", "-F", "#{pane_index}"]);
    assert_eq!(panes.status.code(), Some(0));
    assert_eq!(panes.stdout, b"0\n1\n");
    assert!(panes.stderr.is_empty());
}

fn capture_tui_until(
    fixture: &Fixture,
    attach: &[&str],
    needles: &[&[u8]],
) -> (bool, Vec<u8>, Option<std::process::ExitStatus>) {
    let mut command = fixture.command();
    command.args(attach);
    capture_command_until(command, needles, 80)
}

fn capture_command_until(
    mut command: Command,
    needles: &[&[u8]],
    columns: u16,
) -> (bool, Vec<u8>, Option<std::process::ExitStatus>) {
    let Ok((mut master, slave)) = open_pty_sized(columns) else {
        return (true, Vec::new(), None);
    };
    rustix::io::ioctl_fionbio(&master, true).expect("set pty master nonblocking");
    let stdin = slave.try_clone().expect("clone pty stdin");
    let stdout = slave.try_clone().expect("clone pty stdout");
    let mut child = command
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(slave))
        .spawn()
        .expect("spawn TUI attach");

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
        if needles.iter().all(|needle| {
            captured
                .windows(needle.len())
                .any(|window| window == *needle)
        }) {
            drain_settled_output(&mut master, &mut captured);
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        if let Some(status) = child.try_wait().expect("poll TUI attach") {
            early_status = Some(status);
            break false;
        }
        thread::sleep(Duration::from_millis(5));
    };
    let _ = child.kill();
    let _ = child.wait();
    drop(master);
    (rendered, captured, early_status)
}

fn drain_settled_output(master: &mut File, captured: &mut Vec<u8>) {
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut quiet_since = Instant::now();
    while Instant::now() < deadline && quiet_since.elapsed() < Duration::from_millis(150) {
        let mut buffer = [0_u8; 4096];
        match master.read(&mut buffer) {
            Ok(count) if count > 0 => {
                captured.extend_from_slice(&buffer[..count]);
                quiet_since = Instant::now();
            }
            _ => thread::sleep(Duration::from_millis(5)),
        }
    }
}

#[test]
fn startup_config_reenters_through_the_private_tmux_shim() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    std::fs::write(
        &fixture.config,
        b"run-shell \"tmux set-option -g @boot-shim ready\"\nset-option -g @after-shim loaded\n",
    )
    .expect("write reentrant startup config");

    let created = fixture.run(&["new-session", "-d", "-s", "shim"]);
    assert_eq!(
        created.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    let values = fixture.run(&[
        "show-options",
        "-gqv",
        "@boot-shim",
        ";",
        "show-options",
        "-gqv",
        "@after-shim",
    ]);
    assert_eq!(values.status.code(), Some(0));
    assert_eq!(values.stdout, b"ready\nloaded\n");
    assert!(values.stderr.is_empty());
}

fn wait_with_deadline(mut child: Child, limit: Duration) -> Output {
    let deadline = Instant::now() + limit;
    while child.try_wait().expect("poll child").is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("command did not finish within {limit:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().expect("collect child output")
}

#[test]
fn concurrent_cold_commands_finish_while_startup_and_hook_children_call_back() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let plugin = fixture.home.join("plugin.tmux");
    std::fs::write(
        &plugin,
        b"#!/bin/sh
tmux set-option -g @plugin loaded
",
    )
    .expect("write plugin");
    std::fs::set_permissions(&plugin, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("plugin is executable");
    let sourced = fixture.home.join("tpm.conf");
    std::fs::write(
        &sourced,
        format!(
            "run-shell '{}'\nrun-shell \"tmux set-option -g @tpm sourced\"\n",
            plugin.display()
        ),
    )
    .expect("write sourced config");
    std::fs::write(
        &fixture.config,
        format!(
            "run-shell \"tmux set-option -g @boot ready\"\nsource-file '{}'\nset-hook -g after-select-pane 'run-shell \"tmux set-option -g @hooked yes\"'\n",
            sourced.display()
        ),
    )
    .expect("write startup config");

    let children = (0..2)
        .map(|index| {
            fixture
                .command()
                .args(["new-session", "-d", "-s", &format!("cold{index}")])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn cold command")
        })
        .collect::<Vec<_>>();
    for child in children {
        let output = wait_with_deadline(child, Duration::from_secs(20));
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let split = fixture.run(&["split-window", "-d", "-t", "cold0"]);
    assert_eq!(split.status.code(), Some(0));
    let selected = wait_with_deadline(
        fixture
            .command()
            .args(["select-pane", "-t", "cold0.1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn select-pane"),
        Duration::from_secs(20),
    );
    assert_eq!(selected.status.code(), Some(0));
    let resourced = fixture.run(&["source-file", sourced.to_str().expect("UTF-8 path")]);
    assert_eq!(resourced.status.code(), Some(0));
    let values = fixture.run(&[
        "show-options",
        "-gqv",
        "@boot",
        ";",
        "show-options",
        "-gqv",
        "@plugin",
        ";",
        "show-options",
        "-gqv",
        "@tpm",
        ";",
        "show-options",
        "-gqv",
        "@hooked",
    ]);
    assert_eq!(
        String::from_utf8_lossy(&values.stdout),
        "ready\nloaded\nsourced\nyes\n"
    );
    let sessions = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(String::from_utf8_lossy(&sessions.stdout), "cold0\ncold1\n");
}

#[test]
fn gui_style_default_attach_uses_its_working_directory_with_an_empty_daemon() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let started = fixture.run(&["start-server"]);
    assert_eq!(
        started.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&started.stderr)
    );
    let working_directory = tempfile::Builder::new()
        .prefix("zz app cwd ")
        .tempdir_in("/tmp")
        .expect("temporary app working directory");
    let client = zz_daemon_client::InteractiveClient::connect(&fixture.socket)
        .expect("connect GUI-style interactive client");
    client
        .attach_default_in(working_directory.path())
        .expect("attach from app working directory");
    let expected =
        std::fs::canonicalize(working_directory.path()).expect("canonical app working directory");

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let panes = fixture.run(&["list-panes", "-a", "-F", "#{pane_current_path}"]);
        if panes.status.success() && panes.stdout == format!("{}\n", expected.display()).as_bytes()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "pane cwd never matched {}; stdout={} stderr={}",
            expected.display(),
            String::from_utf8_lossy(&panes.stdout),
            String::from_utf8_lossy(&panes.stderr),
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn native_attach_accepts_targets_and_stops_options_at_positional_sessions() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "named"]);
    assert_eq!(created.status.code(), Some(0));
    for command in ["attach", "attach-session"] {
        let output = fixture.run(&[command, "-t", "named"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, b"open terminal failed: not a terminal\n");

        let positional = fixture.run(&[command, "named", "-@"]);
        assert_eq!(positional.status.code(), Some(1));
        assert!(positional.stdout.is_empty());
        assert_eq!(
            positional.stderr,
            b"zz: usage: zz [--host <name>] attach [--restart-daemon] [-dEr] [-c working-directory] [-f flags] [session]\n"
        );
    }
}

#[test]
fn exact_native_attach_executes_a_valid_command_chain_tail_after_a_positional_session() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "named"]);
    assert_eq!(created.status.code(), Some(0));
    for (command, variable) in [
        ("attach", "NATIVE_ATTACH_TAIL"),
        ("attach-session", "NATIVE_ATTACH_SESSION_TAIL"),
    ] {
        let (rendered, captured, early_status) = capture_tui_until(
            &fixture,
            &[
                command,
                "named",
                ";",
                "set-environment",
                "-g",
                variable,
                "yes",
            ],
            &[b"\x1b[?1049h"],
        );
        assert!(
            rendered,
            "{command}: child exited early={early_status:?}; pty output={}",
            String::from_utf8_lossy(&captured)
        );

        let value = fixture.run(&["show-environment", "-g", variable]);
        assert_eq!(value.status.code(), Some(0));
        assert_eq!(value.stdout, format!("{variable}=yes\n").as_bytes());
        assert!(value.stderr.is_empty());
    }
}

#[test]
fn native_attach_flag_errors_match_tmux_for_both_spellings() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    for command in ["attach", "attach-session"] {
        for (arguments, expected) in [
            (
                &["-0"][..],
                b"command attach-session: unknown flag -0\n".as_slice(),
            ),
            (
                &["-@"][..],
                b"command attach-session: invalid flag -@\n".as_slice(),
            ),
            (
                &["--bogus"][..],
                b"command attach-session: invalid flag --\n".as_slice(),
            ),
            (
                &["-?"][..],
                b"usage: attach-session [-dErx] [-c working-directory] [-f flags] [-t target-session]\n"
                    .as_slice(),
            ),
            (
                &["-t"][..],
                b"command attach-session: -t expects an argument\n".as_slice(),
            ),
            (
                &["-x0"][..],
                b"command attach-session: unknown flag -0\n".as_slice(),
            ),
            (
                &["-x"][..],
                b"can't find session: current session\n".as_slice(),
            ),
        ] {
            let mut invocation = vec![command];
            invocation.extend_from_slice(arguments);
            let output = fixture.run(&invocation);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{invocation:?}"
            );
            assert!(output.stdout.is_empty(), "{invocation:?}");
            assert_eq!(output.stderr, expected, "{invocation:?}");
        }
    }
}

#[test]
fn a_pane_killed_mid_stream_exits_one_without_a_daemon_error() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "dying"])
            .status
            .success()
    );
    let split = || {
        let created = fixture.run(&[
            "split-window",
            "-d",
            "-t",
            "=dying:",
            "-P",
            "-F",
            "#{pane_id}",
            "",
        ]);
        assert!(created.status.success());
        String::from_utf8(created.stdout)
            .expect("pane id")
            .trim()
            .to_owned()
    };
    for after in [&b""[..], b"AFTER-"] {
        let pane = split();
        let (child, mut stdin) = fixture.spawn_with_open_stdin(&[
            "display-message",
            "-I",
            "-t",
            &pane,
            ";",
            "display-message",
            "-p",
            "tail-out",
            ";",
            "set-option",
            "-g",
            "@after",
            "yes",
        ]);
        stdin.write_all(b"RED").expect("write the stream");
        stdin.flush().expect("flush the stream");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !String::from_utf8_lossy(&fixture.run(&["capture-pane", "-p", "-t", &pane]).stdout)
            .contains("RED")
        {
            assert!(Instant::now() < deadline, "the stream never reached {pane}");
            thread::sleep(Duration::from_millis(20));
        }
        assert!(fixture.run(&["kill-pane", "-t", &pane]).status.success());
        stdin.write_all(after).expect("write after the kill");
        drop(stdin);
        let output = child.wait_with_output().expect("collect the stream client");
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(output.stdout, b"");
        assert_eq!(String::from_utf8_lossy(&output.stderr), "");
        assert_eq!(fixture.run(&["show-options", "-gqv", "@after"]).stdout, b"");
    }
}

#[test]
fn headless_attach_resolves_the_target_before_rejecting_the_terminal() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "named"]);
    assert_eq!(created.status.code(), Some(0));
    for command in ["attach", "attach-session"] {
        let output = fixture.run(&[command, "-t", "bogus"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, b"can't find session: bogus\n");
    }
}

#[test]
fn explicit_targetless_attach_autostarts_then_reports_no_sessions() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    std::fs::write(&fixture.config, b"set-environment -g started YES\n")
        .expect("write autostart marker config");
    for command in ["attach", "attach-session"] {
        let output = fixture.run(&[command]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, b"no sessions\n");
        assert!(fixture.socket.exists());
    }

    let started = fixture.run(&["show-environment", "-g", "started"]);
    assert_eq!(started.status.code(), Some(0));
    assert_eq!(started.stdout, b"started=YES\n");
    assert!(started.stderr.is_empty());
}

#[test]
fn attach_restart_daemon_survives_missing_preflight_daemon() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let output = fixture.run(&["attach", "--restart-daemon"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"no sessions\n");
    assert!(fixture.socket.exists());
}

#[test]
fn installed_layout_routes_bare_new_and_attach_across_empty_and_existing_daemons() {
    let Ok((master, slave)) = open_pty() else {
        return;
    };
    drop((master, slave));
    let installed_layout = InstalledLayout::new();
    let cases = [
        ("bare-empty", &[][..], false, &["0"][..]),
        ("bare-existing", &[][..], true, &["existing"][..]),
        (
            "new-empty",
            &["new", "-s", "created"][..],
            false,
            &["created"][..],
        ),
        (
            "new-existing",
            &["new", "-s", "created"][..],
            true,
            &["created", "existing"][..],
        ),
        (
            "attach-existing",
            &["attach", "-t", "existing"][..],
            true,
            &["existing"][..],
        ),
    ];

    for (name, arguments, existing, expected) in cases {
        let fixture = Fixture::new();
        if !local_socket_bind_available(&fixture.socket) {
            return;
        }
        if existing {
            let created = fixture.run(&["new-session", "-d", "-s", "existing"]);
            assert_eq!(
                created.status.code(),
                Some(0),
                "{name}: {}",
                String::from_utf8_lossy(&created.stderr)
            );
        }
        let mut command = installed_layout.command(&fixture);
        command.args(arguments);
        let (rendered, captured, early_status) =
            capture_command_until(command, &[b"\x1b[?1049h"], 80);
        assert!(
            rendered,
            "{name}: child exited early={early_status:?}; pty output={}",
            String::from_utf8_lossy(&captured)
        );

        let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
        assert_eq!(
            listed.status.code(),
            Some(0),
            "{name}: {}",
            String::from_utf8_lossy(&listed.stderr)
        );
        let output = String::from_utf8_lossy(&listed.stdout);
        let mut actual = output.lines().collect::<Vec<_>>();
        actual.sort_unstable();
        assert_eq!(actual, expected, "{name}");
    }

    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let explicit = installed_layout
        .command(&fixture)
        .arg("attach")
        .output()
        .unwrap();
    assert_eq!(explicit.status.code(), Some(1));
    assert!(explicit.stdout.is_empty());
    assert_eq!(explicit.stderr, b"no sessions\n");
    let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(listed.status.code(), Some(0));
    assert!(listed.stdout.is_empty());
}

#[test]
fn explicit_attach_can_fall_back_to_new_session() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(concat!(
            "\"$ZZ_BIN\" -f \"$ZZ_CONF\" -S \"$ZZ_TEST_SOCKET\" attach || ",
            "exec \"$ZZ_BIN\" -f \"$ZZ_CONF\" -S \"$ZZ_TEST_SOCKET\" new-session -s fallback"
        ))
        .env("HOME", &fixture.home)
        .env("XDG_CONFIG_HOME", &fixture.home)
        .env("ZZ_BIN", env!("CARGO_BIN_EXE_zz_cli"))
        .env("ZZ_CONF", &fixture.config)
        .env("ZZ_TEST_SOCKET", &fixture.socket);
    let (rendered, captured, early_status) = capture_command_until(command, &[b"\x1b[?1049h"], 80);
    assert!(
        rendered,
        "child exited early={early_status:?}; pty output={}",
        String::from_utf8_lossy(&captured)
    );
    assert!(captured.windows(11).any(|window| window == b"no sessions"));

    let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(listed.status.code(), Some(0));
    assert_eq!(listed.stdout, b"fallback\n");
}

#[test]
fn native_attach_rejects_unsupported_x_like_the_engine() {
    let fixture = Fixture::new();
    let invocation = zz_protocol::CommandInvocation::new("attach-session", ["-x"]);
    let error = zz_mux::MuxEngine::default()
        .execute(&mut zz_mux::ExecutionContext::default(), &invocation)
        .expect_err("engine rejects the unsupported attach option");
    let expected = format!("{}\n", error.tmux_message());
    for command in ["attach", "attach-session"] {
        let output = fixture
            .command()
            .args([command, "-x"])
            .output()
            .expect("run native attach rejection");
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, expected.as_bytes());
    }
}

#[test]
fn native_attach_e_preserves_the_session_environment() {
    const NAME: &str = "ZZ_NATIVE_ATTACH_E";

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
                "native-attach-e",
                "printf 'ZZ_NATIVE_ATTACH_E_READY\\r\\n'; exec /bin/cat",
            ])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&[
                "set-option",
                "-t",
                "=native-attach-e:",
                "update-environment",
                NAME,
            ])
            .status
            .success()
    );
    assert!(
        fixture
            .run(&[
                "set-environment",
                "-t",
                "=native-attach-e",
                NAME,
                "before-E",
            ])
            .status
            .success()
    );

    let mut preserved = fixture.command();
    preserved
        .env(NAME, "ignored")
        .args(["attach-session", "-E", "-t", "=native-attach-e"]);
    let (rendered, captured, early_status) =
        capture_command_until(preserved, &[b"ZZ_NATIVE_ATTACH_E_READY"], 80);
    if rendered && captured.is_empty() {
        return;
    }
    assert!(
        rendered,
        "native attach -E exited early={early_status:?}; output={}",
        String::from_utf8_lossy(&captured)
    );
    let environment = fixture.run(&["show-environment", "-t", "=native-attach-e", NAME]);
    assert_eq!(environment.status.code(), Some(0));
    assert_eq!(environment.stdout, format!("{NAME}=before-E\n").as_bytes());
    assert!(environment.stderr.is_empty());

    let mut refreshed = fixture.command();
    refreshed
        .env(NAME, "refreshed")
        .args(["attach-session", "-t", "=native-attach-e"]);
    let (rendered, captured, early_status) =
        capture_command_until(refreshed, &[b"ZZ_NATIVE_ATTACH_E_READY"], 80);
    assert!(
        rendered,
        "ordinary native attach exited early={early_status:?}; output={}",
        String::from_utf8_lossy(&captured)
    );
    let environment = fixture.run(&["show-environment", "-t", "=native-attach-e", NAME]);
    assert_eq!(environment.status.code(), Some(0));
    assert_eq!(environment.stdout, format!("{NAME}=refreshed\n").as_bytes());
    assert!(environment.stderr.is_empty());
}

#[test]
fn mouse_option_gates_the_outer_terminal_mouse_modes() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "mousey"]);
    assert_eq!(created.status.code(), Some(0));

    let (rendered, captured, early_status) = capture_tui_until(
        &fixture,
        &["attach-session", "-t", "mousey"],
        &[b"[mousey]"],
    );
    assert!(
        rendered,
        "child exited early={early_status:?}; pty output={}",
        String::from_utf8_lossy(&captured)
    );
    assert!(
        contains(&captured, b"\x1b[?1000h\x1b[?1002h"),
        "the pinned default keeps mouse on, in button-event tracking: {}",
        String::from_utf8_lossy(&captured)
    );
    assert!(
        !contains(&captured, b"\x1b[?1003h"),
        "any-event tracking is for a menu, focus-follows-mouse or a pane \
         that asked for it: {}",
        String::from_utf8_lossy(&captured)
    );

    let disabled = fixture.run(&["set", "-g", "mouse", "off"]);
    assert_eq!(disabled.status.code(), Some(0));
    let (rendered, captured, early_status) = capture_tui_until(
        &fixture,
        &["attach-session", "-t", "mousey"],
        &[b"[mousey]"],
    );
    assert!(
        rendered,
        "child exited early={early_status:?}; pty output={}",
        String::from_utf8_lossy(&captured)
    );
    for armed in [
        b"\x1b[?1000h".as_slice(),
        b"\x1b[?1002h".as_slice(),
        b"\x1b[?1003h".as_slice(),
    ] {
        assert!(
            !contains(&captured, armed),
            "mouse off with no app-requested tracking must arm no outer mouse mode: {}",
            String::from_utf8_lossy(&captured)
        );
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn nested_attach_inside_a_pane_prints_the_pinned_refusal() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let nested_attach = format!(
        "{} -f {} -S {} attach; exec cat",
        env!("CARGO_BIN_EXE_zz_cli"),
        fixture.config.display(),
        fixture.socket.display(),
    );
    let created = fixture.run(&["new-session", "-d", "-s", "outer", &nested_attach]);
    assert_eq!(
        created.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let captured = fixture.run(&["capture-pane", "-p", "-t", "outer"]);
        assert_eq!(captured.status.code(), Some(0));
        let pane_text = String::from_utf8_lossy(&captured.stdout).into_owned();
        if pane_text.contains("sessions should be nested with care, unset $TMUX to force") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "nested refusal did not appear; pane content: {pane_text}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn read_only_attach_renders_but_typed_keys_never_reach_the_pane() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&[
        "new-session",
        "-d",
        "-s",
        "ro",
        "printf 'ZZ_ATTACH_READY\\r\\n'; exec /bin/cat",
    ]);
    assert_eq!(created.status.code(), Some(0));

    let attach_and_type =
        |attach: &[&str], text: &[u8], expected: Option<&str>| -> (bool, Vec<u8>, String) {
            let Ok((mut master, slave)) = open_pty() else {
                return (true, Vec::new(), String::new());
            };
            rustix::io::ioctl_fionbio(&master, true).expect("set pty master nonblocking");
            let stdin = slave.try_clone().expect("clone pty stdin");
            let stdout = slave.try_clone().expect("clone pty stdout");
            let mut child = fixture
                .command()
                .args(attach)
                .stdin(Stdio::from(stdin))
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(slave))
                .spawn()
                .expect("spawn TUI attach");
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut captured = Vec::new();
            let rendered = loop {
                let mut buffer = [0_u8; 4096];
                match master.read(&mut buffer) {
                    Ok(0) => {}
                    Ok(count) => captured.extend_from_slice(&buffer[..count]),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(_) => {}
                }
                if captured
                    .windows(b"ZZ_ATTACH_READY".len())
                    .any(|window| window == b"ZZ_ATTACH_READY")
                {
                    break true;
                }
                if Instant::now() >= deadline || child.try_wait().expect("poll attach").is_some() {
                    break false;
                }
                thread::sleep(Duration::from_millis(5));
            };
            let mut flags = String::new();
            if rendered {
                master.write_all(text).expect("type into the attach pty");
                let settle = Instant::now() + Duration::from_millis(600);
                while Instant::now() < settle {
                    let mut buffer = [0_u8; 4096];
                    match master.read(&mut buffer) {
                        Ok(count) if count > 0 => captured.extend_from_slice(&buffer[..count]),
                        _ => thread::sleep(Duration::from_millis(10)),
                    }
                }
                if let Some(marker) = expected {
                    let deadline = Instant::now() + Duration::from_secs(10);
                    loop {
                        let pane = fixture.run(&["capture-pane", "-p", "-t", "ro"]);
                        if String::from_utf8_lossy(&pane.stdout).contains(marker) {
                            break;
                        }
                        assert!(
                            Instant::now() < deadline,
                            "typed {marker} never reached the pane"
                        );
                        let mut buffer = [0_u8; 4096];
                        if let Ok(count) = master.read(&mut buffer)
                            && count > 0
                        {
                            captured.extend_from_slice(&buffer[..count]);
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                }
                let listed = fixture.run(&["list-clients", "-F", "#{client_flags}"]);
                flags = String::from_utf8_lossy(&listed.stdout).into_owned();
            }
            let _ = child.kill();
            let _ = child.wait();
            drop(master);
            (rendered, captured, flags)
        };

    let (rendered, captured, flags) =
        attach_and_type(&["attach-session", "-r", "-t", "ro"], b"echo ZZRO\r", None);
    if captured.is_empty() && flags.is_empty() && rendered {
        return;
    }
    assert!(
        rendered,
        "read-only attach did not render: {}",
        String::from_utf8_lossy(&captured)
    );
    assert!(
        flags.lines().any(|line| {
            line.split(',').any(|flag| flag == "attached")
                && line.split(',').any(|flag| flag == "read-only")
                && !line.split(',').any(|flag| flag == "ignore-size")
        }),
        "client_flags must report read-only without ignore-size: {flags}"
    );

    let (rendered, writable_attach, flags) = attach_and_type(
        &["attach-session", "-t", "ro"],
        b"echo ZZRW\r",
        Some("ZZRW"),
    );
    assert!(
        rendered,
        "writable attach did not render: {}",
        String::from_utf8_lossy(&writable_attach)
    );
    assert!(
        flags.lines().any(|line| {
            line.split(',').any(|flag| flag == "attached")
                && !line.split(',').any(|flag| flag == "read-only")
                && !line.split(',').any(|flag| flag == "ignore-size")
        }),
        "plain attach reports no read-only flag: {flags}"
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let captured = fixture.run(&["capture-pane", "-p", "-t", "ro"]);
        assert_eq!(captured.status.code(), Some(0));
        let pane_text = String::from_utf8_lossy(&captured.stdout).into_owned();
        assert!(
            !pane_text.contains("ZZRO"),
            "read-only keystrokes reached the pane: {pane_text}"
        );
        if pane_text.contains("ZZRW") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "writable keystrokes never reached the pane: {pane_text}; client flags: {flags}; attach output: {}",
            String::from_utf8_lossy(&writable_attach)
        );
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn option_value_errors_match_pinned_tmux_stderr() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "error-shapes"]);
    assert_eq!(created.status.code(), Some(0));
    let cases: &[(&[&str], &str)] = &[
        (
            &["set", "-g", "display-time", "-5"],
            "value is too small: -5",
        ),
        (
            &["set", "-g", "display-time", "abc"],
            "value is invalid: abc",
        ),
        (&["set", "-g", "display-time"], "empty value"),
        (&["set", "-g", "@novalue"], "empty value"),
        (
            &["set", "-g", "status-keys", "bogus"],
            "unknown value: bogus",
        ),
        (&["set", "-g", "focus-events", "maybe"], "bad value: maybe"),
        (&["set", "-g", "status-bg", "xxxyyy"], "bad colour: xxxyyy"),
        (
            &["set", "-g", "status-style", "bg=xxxyyy"],
            "invalid style: bg=xxxyyy",
        ),
        (&["set", "-g", "prefix", "boguskey"], "bad key: boguskey"),
        (
            &["set", "-g", "default-shell", "/not/a/shell"],
            "not a suitable shell: /not/a/shell",
        ),
        (
            &["set", "-g", "default-client-command", "if -x {"],
            "syntax error",
        ),
    ];
    for (arguments, expected) in cases {
        let output = fixture.run(arguments);
        assert_eq!(output.status.code(), Some(1), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        assert_eq!(
            output.stderr,
            format!("{expected}\n").as_bytes(),
            "{arguments:?}"
        );
    }

    let first = fixture.run(&["set", "-g", "@once", "first"]);
    assert_eq!(first.status.code(), Some(0));
    assert!(first.stdout.is_empty());
    assert!(first.stderr.is_empty());
    let duplicate = fixture.run(&["set", "-go", "@once", "second"]);
    assert_eq!(duplicate.status.code(), Some(1));
    assert!(duplicate.stdout.is_empty());
    assert_eq!(duplicate.stderr, b"already set: @once\n");
}

#[test]
fn target_and_unknown_command_errors_match_pinned_tmux_stderr() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let created = fixture.run(&["new-session", "-d", "-s", "error-shapes"]);
    assert_eq!(created.status.code(), Some(0));
    for (arguments, expected, exit_code) in [
        (
            &["kill-session", "-t", "bogus"] as &[&str],
            b"can't find session: bogus\n" as &[u8],
            1,
        ),
        (&["wibble"], b"unknown command: wibble\n", 1),
    ] {
        let output = fixture.run(arguments);
        assert_eq!(output.status.code(), Some(exit_code));
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, expected);
    }
}

#[test]
fn ls_or_new_session_idiom_starts_one_fresh_daemon() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let output = Command::new("/bin/sh")
        .env("HOME", &fixture.home)
        .env("XDG_CONFIG_HOME", &fixture.home)
        .arg("-c")
        .arg(r#""$1" -f "$2" -S "$3" ls || "$1" -f "$2" -S "$3" new-session -d"#)
        .arg("zz-ls-or-new")
        .arg(env!("CARGO_BIN_EXE_zz_cli"))
        .arg(&fixture.config)
        .arg(&fixture.socket)
        .output()
        .expect("run ls-or-new-session idiom");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, fixture.missing_message());
    assert!(fixture.socket.exists());

    let listed = fixture.run(&["list-sessions", "-F", "#{session_name}"]);
    assert_eq!(listed.status.code(), Some(0));
    assert_eq!(listed.stdout, b"0\n");
    assert!(listed.stderr.is_empty());
}

#[test]
fn no_start_server_suppresses_new_session_autostart() {
    let fixture = Fixture::new();
    let output = fixture.run(&["-N", "new-session", "-d"]);
    assert_missing(&output, &fixture.missing_message());
    fixture.assert_not_started();
}

#[test]
fn has_session_on_a_fresh_socket_leaves_no_daemon() {
    let fixture = Fixture::new();
    let output = fixture.run(&["has-session"]);
    assert_missing(&output, &fixture.missing_message());
    fixture.assert_not_started();
}

#[test]
fn slash_label_never_creates_intermediate_directories() {
    let directory = tempfile::Builder::new()
        .prefix("zz-label-")
        .tempdir_in("/tmp")
        .expect("temporary label directory");
    let root = std::fs::canonicalize(directory.path()).expect("canonical label root");
    let config = directory.path().join("empty.conf");
    std::fs::write(&config, b"").expect("empty mux config");
    let base = root.join(format!("tmux-{}", rustix::process::getuid().as_raw()));
    let socket = base.join("a/b");
    let run = |label: &str, arguments: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_zz_cli"))
            .env("HOME", directory.path())
            .env("XDG_CONFIG_HOME", directory.path())
            .env("TMUX_TMPDIR", directory.path())
            .arg("-f")
            .arg(&config)
            .args(["-L", label])
            .args(arguments)
            .output()
            .expect("run slash-label command")
    };

    let listed = run("a/b", &["list-sessions"]);
    assert_missing(
        &listed,
        format!(
            "error connecting to {} (No such file or directory)\n",
            socket.display()
        )
        .as_bytes(),
    );
    assert!(base.is_dir());
    assert!(!base.join("a").exists());

    let created = run("a/b", &["new-session", "-d"]);
    assert_eq!(created.status.code(), Some(0));
    assert!(created.stdout.is_empty());
    assert_eq!(
        created.stderr,
        format!(
            "error creating {} (No such file or directory)\n",
            socket.display()
        )
        .as_bytes()
    );
    assert!(!base.join("a").exists());
    assert!(!socket.exists());
    assert!(!identity_path(&socket).exists());

    let file = base.join("file");
    std::fs::write(&file, b"").expect("non-directory label parent");
    let not_a_directory = run("file/socket", &["new-session", "-d"]);
    assert_eq!(not_a_directory.status.code(), Some(1));
    assert!(not_a_directory.stdout.is_empty());
    assert_eq!(
        not_a_directory.stderr,
        format!(
            "error connecting to {} (Not a directory)\n",
            file.join("socket").display()
        )
        .as_bytes()
    );
}

mod aliases;
mod control_mode;
mod source_file;
mod tui_status;
