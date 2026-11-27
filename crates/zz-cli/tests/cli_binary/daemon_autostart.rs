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

#[test]
fn source_file_resolves_relative_paths_from_the_command_client_cwd() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let daemon_cwd = std::fs::canonicalize(source_directory(&fixture, "daemon cwd"))
        .expect("canonical daemon cwd");
    let caller_cwd = std::fs::canonicalize(source_directory(
        &fixture,
        "caller cwd [literal]*? with spaces",
    ))
    .expect("canonical caller cwd");
    let daemon_home = std::fs::canonicalize(source_directory(
        &fixture,
        "daemon home [literal]*? with spaces",
    ))
    .expect("canonical daemon home");
    assert_ne!(daemon_cwd, caller_cwd);

    let literal_tilde_name = "literal-source.conf";
    std::fs::write(
        daemon_home.join(literal_tilde_name),
        b"set-option -g @literal_tilde_source decoy-home\n",
    )
    .expect("write home-expansion decoy source");
    let literal_tilde_directory = caller_cwd.join("~");
    std::fs::create_dir(&literal_tilde_directory).expect("literal tilde directory");
    std::fs::write(
        literal_tilde_directory.join(literal_tilde_name),
        b"set-option -g @literal_tilde_source caller-literal\n",
    )
    .expect("write literal tilde source");

    let relative_directory = "relative configs with spaces";
    let sources = caller_cwd.join(relative_directory);
    std::fs::create_dir_all(&sources).expect("relative source directory");
    std::fs::write(
        sources.join("source-file-w-0-0-10.conf"),
        b"set-option -g @cwd_order ten\n",
    )
    .expect("write first relative source");
    std::fs::write(
        sources.join("source-file-w-0-0-20.conf"),
        b"set-option -g @cwd_order twenty\n",
    )
    .expect("write second relative source");
    let nested_entry_directory = caller_cwd.join("a");
    std::fs::create_dir(&nested_entry_directory).expect("nested source entry directory");
    std::fs::write(
        caller_cwd.join("leaf.conf"),
        b"set-option -g @nested_client_cwd caller-root\n",
    )
    .expect("write caller-root nested source");
    std::fs::write(
        nested_entry_directory.join("leaf.conf"),
        b"set-option -g @nested_client_cwd containing-file-decoy\n",
    )
    .expect("write containing-file nested source decoy");
    std::fs::write(
        nested_entry_directory.join("entry.conf"),
        b"set-option -g @nested_replay_started yes\nsource-file leaf.conf\n",
    )
    .expect("write nested source entry");
    assert!(!daemon_cwd.join(relative_directory).exists());

    let started = fixture
        .command()
        .env("HOME", &daemon_home)
        .current_dir(&daemon_cwd)
        .args(["new-session", "-d", "-s", "w"])
        .output()
        .expect("start daemon from its cwd");
    assert_eq!(
        started.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&started.stderr)
    );

    let run_from_caller = |arguments: &[&str]| {
        fixture
            .command()
            .env("HOME", &daemon_home)
            .current_dir(&caller_cwd)
            .args(arguments)
            .output()
            .expect("run command from caller cwd")
    };
    let format_prefix = format!(
        "{relative_directory}/source-file-#{{session_name}}-#{{window_index}}-#{{pane_index}}"
    );
    let glob = format!("{format_prefix}-[12]0.conf");
    let ten = format!("{format_prefix}-10.conf");
    let twenty = format!("{format_prefix}-20.conf");
    let missing = format!("{relative_directory}/source-file-#{{session_name}}-missing.conf");

    let literal_tilde = format!("~/{literal_tilde_name}");
    let sourced_literal_tilde = run_from_caller(&["source-file", &literal_tilde]);
    assert_eq!(sourced_literal_tilde.status.code(), Some(0));
    assert!(sourced_literal_tilde.stdout.is_empty());
    assert!(sourced_literal_tilde.stderr.is_empty());
    let literal_tilde_value = run_from_caller(&["show-options", "-gqv", "@literal_tilde_source"]);
    assert_eq!(literal_tilde_value.status.code(), Some(0));
    assert_eq!(literal_tilde_value.stdout, b"caller-literal\n");
    assert!(literal_tilde_value.stderr.is_empty());

    let nested = run_from_caller(&["source-file", "a/entry.conf"]);
    assert_eq!(nested.status.code(), Some(0));
    assert!(nested.stdout.is_empty());
    assert!(nested.stderr.is_empty());
    let nested_replay = run_from_caller(&["show-options", "-gqv", "@nested_replay_started"]);
    assert_eq!(nested_replay.status.code(), Some(0));
    assert_eq!(nested_replay.stdout, b"yes\n");
    assert!(nested_replay.stderr.is_empty());
    let nested_base = run_from_caller(&["show-options", "-gqv", "@nested_client_cwd"]);
    assert_eq!(nested_base.status.code(), Some(0));
    assert_eq!(nested_base.stdout, b"caller-root\n");
    assert!(nested_base.stderr.is_empty());

    let globbed = run_from_caller(&["source-file", "-F", &glob]);
    assert_eq!(globbed.status.code(), Some(0));
    assert!(globbed.stdout.is_empty());
    assert!(globbed.stderr.is_empty());
    let glob_order = run_from_caller(&["show-options", "-gqv", "@cwd_order"]);
    assert_eq!(glob_order.status.code(), Some(0));
    assert_eq!(glob_order.stdout, b"twenty\n");
    assert!(glob_order.stderr.is_empty());

    let explicit = run_from_caller(&["source-file", "-F", &twenty, &ten]);
    assert_eq!(explicit.status.code(), Some(0));
    assert!(explicit.stdout.is_empty());
    assert!(explicit.stderr.is_empty());
    let explicit_order = run_from_caller(&["show-options", "-gqv", "@cwd_order"]);
    assert_eq!(explicit_order.status.code(), Some(0));
    assert_eq!(explicit_order.stdout, b"ten\n");
    assert!(explicit_order.stderr.is_empty());

    let quiet = run_from_caller(&["source-file", "-Fq", &missing, &twenty]);
    assert_eq!(quiet.status.code(), Some(0));
    assert!(quiet.stdout.is_empty());
    assert!(quiet.stderr.is_empty());
    let quiet_continued = run_from_caller(&["show-options", "-gqv", "@cwd_order"]);
    assert_eq!(quiet_continued.status.code(), Some(0));
    assert_eq!(quiet_continued.stdout, b"twenty\n");
    assert!(quiet_continued.stderr.is_empty());

    let loud = run_from_caller(&["source-file", "-F", &missing]);
    assert_eq!(loud.status.code(), Some(1));
    assert!(loud.stdout.is_empty());
    assert_eq!(
        loud.stderr,
        format!(
            "No such file or directory: {}\n",
            Path::new(relative_directory)
                .join("source-file-w-missing.conf")
                .display()
        )
        .into_bytes()
    );
}

#[test]
fn control_config_initial_source_finishes_shell_waits_and_replays_later_commands() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "config"])
            .status
            .success()
    );
    let directory = source_directory(&fixture, "control-config");
    let bad = write_source(&directory, "bad.conf", "wibble\n");
    let config = write_source(
        &directory,
        "entry.conf",
        &format!(
            "run-shell 'sleep 0.1'\n\
             source-file '{bad}'\n\
             bind-key -n F12 display-message -p loaded\n\
             set-environment -g CONTROL_CONFIG_DONE yes\n"
        ),
    );
    let output = fixture
        .command()
        .args(["-C", "source-file", &config])
        .stdin(Stdio::null())
        .output()
        .expect("control source with closed stdin");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("control transcript");
    assert!(
        stdout.contains(&format!("%config-error {bad}:1: unknown command: wibble")),
        "{stdout}"
    );
    let binding = fixture.run(&["list-keys", "-T", "root", "-F", "#{key_command}", "F12"]);
    assert!(binding.status.success());
    assert_eq!(binding.stdout, b"display-message -p loaded\n");
    let after = fixture.run(&["show-environment", "-g", "CONTROL_CONFIG_DONE"]);
    assert!(after.status.success());
    assert_eq!(after.stdout, b"CONTROL_CONFIG_DONE=yes\n");
}

#[test]
fn control_config_typed_argument_rejection_keeps_parse_prefix_and_next_line() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert!(
        fixture
            .run(&["new-session", "-d", "-s", "typed"])
            .status
            .success()
    );
    let mut child = fixture
        .command()
        .args(["-C", "attach-session", "-t", "typed"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("typed control client");
    child
        .stdin
        .take()
        .expect("control stdin")
        .write_all(
            b"bind-key -T { set-environment -g CONTROL_FORBIDDEN yes } F11 display-message\n\
          bind-key -T control-test F12 { display-message -p accepted }\n\
          detach-client\n",
        )
        .expect("typed control lines");
    let output = child.wait_with_output().expect("control result");
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("typed control transcript");
    assert!(
        stdout.contains("parse error: command bind-key: -T argument must be a string\n"),
        "{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("%error ") && line.ends_with(" 1")),
        "{stdout}"
    );
    let binding = fixture.run(&[
        "list-keys",
        "-T",
        "control-test",
        "-F",
        "#{key_command}",
        "F12",
    ]);
    assert!(binding.status.success());
    assert_eq!(binding.stdout, b"display-message -p accepted\n");
    assert!(
        !fixture
            .run(&["show-environment", "-g", "CONTROL_FORBIDDEN"])
            .status
            .success()
    );
}

#[test]
fn source_file_diagnostics_split_stdout_stderr_and_the_exit_code() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert_eq!(
        fixture
            .run(&["new-session", "-d", "-s", "diagnostics"])
            .status
            .code(),
        Some(0)
    );
    let directory = source_directory(&fixture, "diagnostics");
    let bad = write_source(&directory, "bad.conf", "wibble\n");
    let missing = directory
        .join("missing.conf")
        .to_str()
        .expect("UTF-8 missing path")
        .to_owned();

    let pattern = directory.join("bad*.conf");
    let globbed = fixture.run(&[
        "source-file",
        pattern.to_str().expect("UTF-8 source glob pattern"),
    ]);
    assert_eq!(globbed.status.code(), Some(1));
    assert_eq!(
        globbed.stdout,
        format!("{bad}:1: unknown command: wibble\n").into_bytes()
    );
    assert!(globbed.stderr.is_empty());

    let quiet_miss = fixture.run(&["source-file", "-q", &missing]);
    assert_eq!(quiet_miss.status.code(), Some(0));
    assert!(quiet_miss.stdout.is_empty());
    assert!(quiet_miss.stderr.is_empty());

    let loud_miss = fixture.run(&["source-file", &missing]);
    assert_eq!(loud_miss.status.code(), Some(1));
    assert!(loud_miss.stdout.is_empty());
    assert_eq!(
        loud_miss.stderr,
        format!("No such file or directory: {missing}\n").into_bytes()
    );

    let mixed = fixture.run(&["source-file", &bad, &missing]);
    assert_eq!(mixed.status.code(), Some(1));
    assert_eq!(
        mixed.stdout,
        format!("{bad}:1: unknown command: wibble\n").into_bytes()
    );
    assert_eq!(
        mixed.stderr,
        format!("No such file or directory: {missing}\n").into_bytes()
    );

    let quiet_mixed = fixture.run(&["source-file", "-q", &bad, &missing]);
    assert_eq!(quiet_mixed.status.code(), Some(1));
    assert_eq!(
        quiet_mixed.stdout,
        format!("{bad}:1: unknown command: wibble\n").into_bytes()
    );
    assert!(quiet_mixed.stderr.is_empty());

    let leaf = write_source(&directory, "leaf.conf", "wibble\nwibble\nblorp\n");
    let entry = write_source(&directory, "entry.conf", &format!("source-file {leaf}\n"));
    let second = write_source(&directory, "second.conf", "flurb\n");
    let nested = fixture.run(&["source-file", &entry, &second]);
    assert_eq!(nested.status.code(), Some(1));
    assert_eq!(
        nested.stdout,
        format!(
            "{leaf}:1: unknown command: wibble\n\
             {second}:1: unknown command: flurb\n"
        )
        .into_bytes()
    );
    assert!(nested.stderr.is_empty());

    let verbose_leaf = write_source(
        &directory,
        "verbose-leaf.conf",
        "set-option -g @verbose-loaded yes\n",
    );
    let verbose_entry = write_source(
        &directory,
        "verbose.conf",
        &format!("source-file -v {verbose_leaf}\nset-option -g @loaded yes\n"),
    );
    let verbose = fixture.run(&["source-file", &verbose_entry]);
    assert_eq!(verbose.status.code(), Some(0));
    assert_eq!(
        verbose.stdout,
        format!("{verbose_leaf}:1: set-option -g @verbose-loaded yes\n").into_bytes()
    );
    assert!(verbose.stderr.is_empty());
    let loaded = fixture.run(&["show-options", "-gqv", "@loaded"]);
    assert_eq!(loaded.status.code(), Some(0));
    assert_eq!(loaded.stdout, b"yes\n");

    let chained = fixture.run(&[
        "source-file",
        &bad,
        ";",
        "display-message",
        "-p",
        "after-the-diagnostic",
    ]);
    assert_eq!(chained.status.code(), Some(1));
    assert_eq!(
        chained.stdout,
        format!("{bad}:1: unknown command: wibble\nafter-the-diagnostic\n").into_bytes()
    );
    assert!(chained.stderr.is_empty());

    let stopped = fixture.run(&[
        "kill-window",
        "-t",
        "nosuchwindow",
        ";",
        "display-message",
        "-p",
        "never-runs",
    ]);
    assert_eq!(stopped.status.code(), Some(1));
    assert!(stopped.stdout.is_empty());
    assert_eq!(stopped.stderr, b"can't find window: nosuchwindow\n");
}

#[test]
fn source_file_replayed_output_batches_top_level_paths() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert_eq!(
        fixture
            .run(&["new-session", "-d", "-s", "output"])
            .status
            .code(),
        Some(0)
    );
    let directory = source_directory(&fixture, "replayed-output");
    let child = write_source(
        &directory,
        "10-child.conf",
        "display-message -p CHILD_ONE\nlist-sessions -F CHILD_TWO\n",
    );
    let root = write_source(
        &directory,
        "20-root.conf",
        &format!(
            "display-message -p ROOT_ONE\n\
             source-file -v {child}\n\
             display-message -p ROOT_TWO\n"
        ),
    );
    let child_verbose = format!(
        "{child}:1: display-message -p CHILD_ONE\n\
         {child}:2: list-sessions -F CHILD_TWO"
    );
    let root_verbose = format!(
        "{root}:1: display-message -p ROOT_ONE\n\
         {root}:2: source-file -v {child}\n\
         {root}:3: display-message -p ROOT_TWO"
    );
    let child_replay = "CHILD_ONE\nCHILD_TWO";
    let root_replay = format!("ROOT_ONE\n{child_verbose}\n{child_replay}\nROOT_TWO");
    let nested_expected = format!("{root_verbose}\n{root_replay}\n");
    let aggregate_expected =
        format!("{child_verbose}\n{root_verbose}\n{child_replay}\n{root_replay}\n");

    let nested = fixture.run(&["source-file", "-v", &root]);
    assert_eq!(nested.status.code(), Some(0));
    assert_eq!(nested.stdout, nested_expected.into_bytes());
    assert!(nested.stderr.is_empty());

    let declared = fixture.run(&["source-file", "-v", &child, &root]);
    assert_eq!(declared.status.code(), Some(0));
    assert_eq!(declared.stdout, aggregate_expected.as_bytes());
    assert!(declared.stderr.is_empty());

    let glob = directory.join("*.conf").display().to_string();
    let globbed = fixture.run(&["source-file", "-v", &glob]);
    assert_eq!(globbed.status.code(), Some(0));
    assert_eq!(globbed.stdout, aggregate_expected.as_bytes());
    assert!(globbed.stderr.is_empty());

    assert!(
        fixture
            .run(&[
                "set-option",
                "-s",
                "command-alias[90]",
                &format!("indirect=source-file -v {child}"),
            ])
            .status
            .success()
    );
    let alias_root = write_source(
        &directory,
        "alias-root.conf",
        "display-message -p ALIAS_BEFORE\nindirect\ndisplay-message -p ALIAS_AFTER\n",
    );
    let aliased = fixture.run(&["source-file", &alias_root]);
    assert_eq!(aliased.status.code(), Some(0));
    assert_eq!(
        aliased.stdout,
        format!("ALIAS_BEFORE\n{child_verbose}\n{child_replay}\nALIAS_AFTER\n").into_bytes()
    );
    assert!(aliased.stderr.is_empty());

    let conditional_root = write_source(
        &directory,
        "conditional-root.conf",
        &format!(
            "display-message -p CONDITIONAL_BEFORE\n\
             if-shell -F 1 'source-file -v {child}'\n\
             display-message -p CONDITIONAL_AFTER\n"
        ),
    );
    let conditional = fixture.run(&["source-file", &conditional_root]);
    assert_eq!(conditional.status.code(), Some(0));
    assert_eq!(
        conditional.stdout,
        format!("CONDITIONAL_BEFORE\n{child_verbose}\n{child_replay}\nCONDITIONAL_AFTER\n")
            .into_bytes()
    );
    assert!(conditional.stderr.is_empty());

    let hook_child = write_source(
        &directory,
        "hook-child.conf",
        "list-sessions -F HOOK_CHILD\n",
    );
    let hook_root = write_source(
        &directory,
        "hook-root.conf",
        "display-message -p HOOK_TRIGGER\nlist-sessions -F HOOK_LATER\n",
    );
    assert!(
        fixture
            .run(&[
                "set-hook",
                "-g",
                "after-display-message",
                &format!("source-file {hook_child}"),
            ])
            .status
            .success()
    );
    let hooked = fixture.run(&["source-file", &hook_root]);
    assert_eq!(hooked.status.code(), Some(0));
    assert_eq!(hooked.stdout, b"HOOK_TRIGGER\nHOOK_CHILD\nHOOK_LATER\n");
    assert!(hooked.stderr.is_empty());
    assert!(
        fixture
            .run(&["set-hook", "-gu", "after-display-message"])
            .status
            .success()
    );

    let indirect_error_child = write_source(
        &directory,
        "indirect-error-child.conf",
        "display-message -p BEFORE_CHILD\n\
         kill-session -t missing-indirect\n\
         display-message -p AFTER_CHILD\n",
    );
    let indirect_error_root = write_source(
        &directory,
        "indirect-error-root.conf",
        &format!(
            "display-message -p ROOT_BEFORE\n\
             if-shell -F 1 'source-file {indirect_error_child}'\n\
             display-message -p ROOT_AFTER\n"
        ),
    );
    let indirect_error = fixture.run(&["source-file", &indirect_error_root]);
    assert_eq!(indirect_error.status.code(), Some(1));
    assert_eq!(
        indirect_error.stdout,
        b"ROOT_BEFORE\nBEFORE_CHILD\nAFTER_CHILD\nROOT_AFTER\n"
    );
    assert_eq!(
        indirect_error.stderr,
        b"can't find session: missing-indirect\n"
    );

    let ordered_error_child = write_source(
        &directory,
        "ordered-error-child.conf",
        "kill-session -t missing-B\n",
    );
    let ordered_error_root = write_source(
        &directory,
        "ordered-error-root.conf",
        &format!(
            "kill-session -t missing-A\n\
             if-shell -F 1 'source-file {ordered_error_child}'\n\
             kill-session -t missing-C\n"
        ),
    );
    let ordered_error = fixture.run(&["source-file", &ordered_error_root]);
    assert_eq!(ordered_error.status.code(), Some(1));
    assert!(ordered_error.stdout.is_empty());
    assert_eq!(
        ordered_error.stderr,
        b"can't find session: missing-A\n\
          can't find session: missing-B\n\
          can't find session: missing-C\n"
    );

    let invalid_parse = write_source(
        &directory,
        "invalid-parse.conf",
        "display-message -p CHILD_SHOULD_NOT_RUN\nset @bad \\400\n",
    );
    let diagnostic = format!("{invalid_parse}:2: invalid octal escape");
    let direct_parse = write_source(
        &directory,
        "direct-parse.conf",
        &format!(
            "display-message -p ROOT_BEFORE\n\
             source-file {invalid_parse}\n\
             display-message -p ROOT_AFTER\n"
        ),
    );
    let direct_diagnostic = fixture.run(&["source-file", "-v", &direct_parse]);
    assert_eq!(direct_diagnostic.status.code(), Some(1));
    assert_eq!(
        direct_diagnostic.stdout,
        format!(
            "{direct_parse}:1: display-message -p ROOT_BEFORE\n\
             {direct_parse}:2: source-file {invalid_parse}\n\
             {direct_parse}:3: display-message -p ROOT_AFTER\n\
             ROOT_BEFORE\n{diagnostic}\nROOT_AFTER\n"
        )
        .into_bytes()
    );
    assert!(direct_diagnostic.stderr.is_empty());

    let conditional_parse = write_source(
        &directory,
        "conditional-parse.conf",
        &format!(
            "display-message -p ROOT_BEFORE\n\
             if-shell -F 1 'source-file {invalid_parse}'\n\
             display-message -p ROOT_AFTER\n"
        ),
    );
    let conditional_diagnostic = fixture.run(&["source-file", &conditional_parse]);
    assert_eq!(conditional_diagnostic.status.code(), Some(1));
    assert_eq!(
        conditional_diagnostic.stdout,
        format!("ROOT_BEFORE\n{diagnostic}\nROOT_AFTER\n").into_bytes()
    );
    assert!(conditional_diagnostic.stderr.is_empty());

    let diagnostic_good = write_source(
        &directory,
        "diagnostic-good.conf",
        "display-message -p GOOD_DIAGNOSTIC_OUTPUT\n",
    );
    let diagnostic_later = write_source(
        &directory,
        "diagnostic-later.conf",
        "display-message -p LATER_DIAGNOSTIC_OUTPUT\n",
    );
    let top_level_diagnostic = fixture.run(&[
        "source-file",
        "-v",
        &diagnostic_good,
        &invalid_parse,
        &diagnostic_later,
    ]);
    assert_eq!(top_level_diagnostic.status.code(), Some(1));
    assert_eq!(
        top_level_diagnostic.stdout,
        format!(
            "{diagnostic_good}:1: display-message -p GOOD_DIAGNOSTIC_OUTPUT\n\
             {diagnostic_later}:1: display-message -p LATER_DIAGNOSTIC_OUTPUT\n\
             GOOD_DIAGNOSTIC_OUTPUT\nLATER_DIAGNOSTIC_OUTPUT\n{diagnostic}\n"
        )
        .into_bytes()
    );
    assert!(top_level_diagnostic.stderr.is_empty());

    let nested_multi_parse = write_source(
        &directory,
        "nested-multi-parse.conf",
        &format!(
            "display-message -p ROOT_BEFORE\n\
             source-file -v {diagnostic_good} {invalid_parse} {diagnostic_later}\n\
             display-message -p ROOT_AFTER\n"
        ),
    );
    let nested_multi_diagnostic = fixture.run(&["source-file", "-v", &nested_multi_parse]);
    assert_eq!(nested_multi_diagnostic.status.code(), Some(1));
    assert_eq!(
        nested_multi_diagnostic.stdout,
        format!(
            "{nested_multi_parse}:1: display-message -p ROOT_BEFORE\n\
             {nested_multi_parse}:2: source-file -v {diagnostic_good} {invalid_parse} {diagnostic_later}\n\
             {nested_multi_parse}:3: display-message -p ROOT_AFTER\n\
             ROOT_BEFORE\n\
             {diagnostic_good}:1: display-message -p GOOD_DIAGNOSTIC_OUTPUT\n\
             {diagnostic_later}:1: display-message -p LATER_DIAGNOSTIC_OUTPUT\n\
             GOOD_DIAGNOSTIC_OUTPUT\nLATER_DIAGNOSTIC_OUTPUT\n{diagnostic}\nROOT_AFTER\n"
        )
        .into_bytes()
    );
    assert!(nested_multi_diagnostic.stderr.is_empty());

    let unknown_command = write_source(&directory, "unknown-command.conf", "wibble\n");
    let nested_unknown = write_source(
        &directory,
        "nested-unknown.conf",
        &format!(
            "display-message -p ROOT_BEFORE\n\
             source-file {unknown_command}\n\
             display-message -p ROOT_AFTER\n"
        ),
    );
    let unknown_diagnostic = fixture.run(&["source-file", "-v", &nested_unknown]);
    assert_eq!(unknown_diagnostic.status.code(), Some(1));
    assert_eq!(
        unknown_diagnostic.stdout,
        format!(
            "{nested_unknown}:1: display-message -p ROOT_BEFORE\n\
             {nested_unknown}:2: source-file {unknown_command}\n\
             {nested_unknown}:3: display-message -p ROOT_AFTER\n\
             ROOT_BEFORE\n\
             {unknown_command}:1: unknown command: wibble\n\
             ROOT_AFTER\n"
        )
        .into_bytes()
    );
    assert!(unknown_diagnostic.stderr.is_empty());

    let good = write_source(
        &directory,
        "30-good.conf",
        "display-message -p GOOD_OUTPUT\n",
    );
    let unreadable = directory.join("middle-directory");
    std::fs::create_dir(&unreadable).expect("unreadable source directory");
    let later = write_source(
        &directory,
        "40-later.conf",
        "display-message -p LATER_OUTPUT\n",
    );
    let continued = fixture.run(&[
        "source-file",
        &good,
        unreadable.to_str().expect("UTF-8 unreadable path"),
        &later,
    ]);
    assert_eq!(continued.status.code(), Some(1));
    assert_eq!(continued.stdout, b"GOOD_OUTPUT\nLATER_OUTPUT\n");
    assert_eq!(
        continued.stderr,
        format!("Input/output error: {}\n", unreadable.display()).into_bytes()
    );
}

#[test]
fn source_file_replayed_runtime_errors_are_bare_and_propagate_outward() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert_eq!(
        fixture
            .run(&["new-session", "-d", "-s", "replayed-errors"])
            .status
            .code(),
        Some(0)
    );
    let directory = source_directory(&fixture, "replayed-errors");
    let runtime = write_source(
        &directory,
        "runtime.conf",
        "kill-session -t missing-runtime\n\
         set-option -g nonexistent-option value\n\
         set-environment -g \"\" value\n\
         set-option -g @runtime-after yes\n",
    );

    let replayed = fixture.run(&["source-file", &runtime]);
    assert_eq!(replayed.status.code(), Some(1));
    assert!(replayed.stdout.is_empty());
    assert_eq!(
        replayed.stderr,
        b"can't find session: missing-runtime\ninvalid option: nonexistent-option\nempty variable name\n"
    );
    let after = fixture.run(&["show-options", "-gqv", "@runtime-after"]);
    assert_eq!(after.status.code(), Some(0));
    assert_eq!(after.stdout, b"yes\n");

    let inner = write_source(
        &directory,
        "inner.conf",
        "kill-session -t missing-inner\nset-option -g @inner-after yes\n",
    );
    let outer = write_source(
        &directory,
        "outer.conf",
        &format!("source-file '{inner}'\nset-option -g @outer-after yes\n"),
    );
    let nested = fixture.run(&["source-file", &outer]);
    assert_eq!(nested.status.code(), Some(1));
    assert!(nested.stdout.is_empty());
    assert_eq!(nested.stderr, b"can't find session: missing-inner\n");
    for option in ["@inner-after", "@outer-after"] {
        let after = fixture.run(&["show-options", "-gqv", option]);
        assert_eq!(after.status.code(), Some(0));
        assert_eq!(after.stdout, b"yes\n");
    }

    assert!(
        fixture
            .run(&[
                "set-option",
                "-s",
                "command-alias[100]",
                "source=new-window -t missing:",
            ])
            .status
            .success()
    );
    let alias_root = write_source(
        &directory,
        "source-alias.conf",
        "source ignored.conf\nset-option -g @source-alias-after yes\n",
    );
    let aliased = fixture.run(&["source-file", &alias_root]);
    assert_eq!(aliased.status.code(), Some(1));
    assert!(aliased.stdout.is_empty());
    assert_eq!(aliased.stderr, b"can't find session: missing\n");
    assert_eq!(
        fixture
            .run(&["show-options", "-gqv", "@source-alias-after"])
            .stdout,
        b"yes\n"
    );

    let hook_runtime = write_source(
        &directory,
        "hook-runtime.conf",
        "display-message -p BEFORE\n\
         kill-session -t missing-runtime\n\
         display-message -p AFTER\n\
         list-sessions -F 'LIST_#{session_name}'\n",
    );
    assert!(
        fixture
            .run(&["set-hook", "-g", "command-error", "display-message -p HOOK",])
            .status
            .success()
    );
    let hooked = fixture.run(&["source-file", &hook_runtime]);
    assert_eq!(hooked.status.code(), Some(1));
    assert_eq!(
        hooked.stdout,
        b"BEFORE\nHOOK\nAFTER\nLIST_replayed-errors\n"
    );
    assert_eq!(hooked.stderr, b"can't find session: missing-runtime\n");
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
fn source_file_allows_fifty_invocations_and_refuses_the_fifty_first() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert_eq!(
        fixture
            .run(&["new-session", "-d", "-s", "depth"])
            .status
            .code(),
        Some(0)
    );

    let allowed = source_directory(&fixture, "depth-allowed");
    let entry = write_source_chain(&allowed, 49);
    let sourced = fixture.run(&["source-file", &entry]);
    assert_eq!(sourced.status.code(), Some(0));
    assert!(sourced.stdout.is_empty());
    assert!(sourced.stderr.is_empty());
    assert_eq!(
        fixture.run(&["show-options", "-gv", "@depth"]).stdout,
        b"49\n"
    );
    assert_eq!(
        fixture.run(&["show-options", "-gv", "@leaf49"]).stdout,
        b"yes\n"
    );

    let refused = source_directory(&fixture, "depth-refused");
    let entry = write_source_chain(&refused, 50);
    let sourced = fixture.run(&["source-file", &entry]);
    assert_eq!(sourced.status.code(), Some(1));
    assert!(sourced.stdout.is_empty());
    assert_eq!(
        sourced.stderr,
        b"too many nested files\ntoo many nested files\n"
    );
    assert_eq!(
        fixture.run(&["show-options", "-gv", "@depth"]).stdout,
        b"50\n"
    );
    assert_eq!(
        fixture.run(&["show-options", "-gv", "@after50"]).stdout,
        b"yes\n"
    );
    assert!(
        fixture
            .run(&["show-options", "-gqv", "@leaf50"])
            .stdout
            .is_empty()
    );
}

#[test]
fn source_file_diagnoses_a_malformed_refused_invocation_instead_of_the_depth_limit() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    assert_eq!(
        fixture
            .run(&["new-session", "-d", "-s", "malformed-depth"])
            .status
            .code(),
        Some(0)
    );

    for (name, deepest, message) in [
        (
            "depth-no-path",
            "source-file\n",
            "command source-file: too few arguments (need at least 1)",
        ),
        (
            "depth-bad-flag",
            "source-file -z leaf.conf\n",
            "command source-file: unknown flag -z",
        ),
    ] {
        let directory = source_directory(&fixture, name);
        let entry = write_source_chain_with_deepest(&directory, 50, deepest);
        let deepest_path = directory
            .join("f50.conf")
            .to_str()
            .expect("UTF-8 chain leaf")
            .to_owned();
        let sourced = fixture.run(&["source-file", &entry]);
        assert_eq!(sourced.status.code(), Some(1));
        assert_eq!(
            sourced.stdout,
            format!("{deepest_path}:2: {message}\n").into_bytes()
        );
        assert!(sourced.stderr.is_empty());
    }
}

#[test]
fn source_file_treats_the_default_config_as_an_ordinary_ordered_path() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let config_home = source_directory(&fixture, "runtime-default-xdg");
    let isolated_home = source_directory(&fixture, "runtime-default-home");
    let default_config = config_home.join("zz").join("mux.conf");
    std::fs::create_dir_all(default_config.parent().expect("default config parent"))
        .expect("default config directory");
    std::fs::write(&default_config, b"wibble\n").expect("write invalid default mux config");
    let started = fixture
        .command()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", &isolated_home)
        .arg("start-server")
        .output()
        .expect("run zz start-server");
    assert_eq!(started.status.code(), Some(0));

    let sourced = fixture
        .command()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", &isolated_home)
        .arg("source-file")
        .arg(&default_config)
        .output()
        .expect("run zz source-file on the default config");
    assert_eq!(sourced.status.code(), Some(1));
    assert_eq!(
        sourced.stdout,
        format!("{}:1: unknown command: wibble\n", default_config.display()).into_bytes()
    );
    assert!(sourced.stderr.is_empty());

    std::fs::write(&default_config, b"set-option -ag @default_order D\n")
        .expect("write ordered default mux config");
    let after = isolated_home.join("after.conf");
    std::fs::write(&after, b"set-option -ag @default_order A\n")
        .expect("write ordered after config");
    let initialized = fixture
        .command()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", &isolated_home)
        .args(["set-option", "-g", "@default_order", ""])
        .output()
        .expect("initialize source order");
    assert_eq!(initialized.status.code(), Some(0));
    assert!(initialized.stdout.is_empty());
    assert!(initialized.stderr.is_empty());
    let missing = isolated_home.join("missing.conf");
    let ordered = fixture
        .command()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", &isolated_home)
        .arg("source-file")
        .arg("-v")
        .arg(&default_config)
        .arg(&missing)
        .arg(&after)
        .arg(&default_config)
        .output()
        .expect("run ordered default source");
    assert_eq!(ordered.status.code(), Some(1));
    assert_eq!(
        ordered.stdout,
        format!(
            "{}:1: set-option -ag @default_order D\n\
             {}:1: set-option -ag @default_order A\n\
             {}:1: set-option -ag @default_order D\n",
            default_config.display(),
            after.display(),
            default_config.display(),
        )
        .into_bytes()
    );
    assert_eq!(
        ordered.stderr,
        format!("No such file or directory: {}\n", missing.display()).into_bytes()
    );
    let order = fixture
        .command()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", &isolated_home)
        .args(["show-options", "-gqv", "@default_order"])
        .output()
        .expect("show default source order");
    assert_eq!(order.status.code(), Some(0));
    assert_eq!(order.stdout, b"DAD\n");
    assert!(order.stderr.is_empty());
}

#[test]
fn reload_config_of_the_default_config_stays_silent_and_resets_keys() {
    let fixture = Fixture::new();
    if !local_socket_bind_available(&fixture.socket) {
        return;
    }
    let config_home = source_directory(&fixture, "reload-default-xdg");
    let isolated_home = source_directory(&fixture, "reload-default-home");
    let default_config = config_home.join("zz").join("mux.conf");
    std::fs::create_dir_all(default_config.parent().expect("default config parent"))
        .expect("default config directory");
    let caller_cwd = std::fs::canonicalize(source_directory(
        &fixture,
        "default caller cwd [literal]*? with spaces",
    ))
    .expect("canonical default source caller cwd");
    std::fs::write(
        caller_cwd.join("leaf.conf"),
        b"set-option -g @default_nested_client_cwd caller-root\n",
    )
    .expect("write default source caller-root leaf");
    std::fs::write(
        default_config
            .parent()
            .expect("default config parent")
            .join("leaf.conf"),
        b"set-option -g @default_nested_client_cwd containing-file-decoy\n",
    )
    .expect("write default source containing-file decoy");
    std::fs::write(
        &default_config,
        b"set-option -g @default_nested_replay_started yes\nsource-file leaf.conf\nbind-key -T reload-loaded z display-message loaded\nwibble\n",
    )
    .expect("write default mux config");
    let started = fixture
        .command()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", &isolated_home)
        .arg("start-server")
        .output()
        .expect("run zz start-server");
    assert_eq!(started.status.code(), Some(0));
    let bound = fixture
        .command()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", &isolated_home)
        .args([
            "bind-key",
            "-T",
            "reload-stale",
            "z",
            "display-message",
            "stale",
        ])
        .output()
        .expect("bind stale reload key");
    assert_eq!(bound.status.code(), Some(0));
    assert!(bound.stdout.is_empty());
    assert!(bound.stderr.is_empty());

    let reloaded = fixture
        .command()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", &isolated_home)
        .current_dir(&caller_cwd)
        .arg("reload-config")
        .output()
        .expect("run zz reload-config from the caller cwd");
    assert_eq!(reloaded.status.code(), Some(0));
    assert!(reloaded.stdout.is_empty());
    assert!(reloaded.stderr.is_empty());
    let replay_started = fixture
        .command()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", &isolated_home)
        .current_dir(&caller_cwd)
        .args(["show-options", "-gqv", "@default_nested_replay_started"])
        .output()
        .expect("show direct reload replay marker");
    assert_eq!(replay_started.status.code(), Some(0));
    assert!(replay_started.stdout.is_empty());
    assert!(replay_started.stderr.is_empty());
    let nested_base = fixture
        .command()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", &isolated_home)
        .current_dir(&caller_cwd)
        .args(["show-options", "-gqv", "@default_nested_client_cwd"])
        .output()
        .expect("show direct reload nested base");
    assert_eq!(nested_base.status.code(), Some(0));
    assert!(nested_base.stdout.is_empty());
    assert!(nested_base.stderr.is_empty());
    let stale = fixture
        .command()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", &isolated_home)
        .args(["list-keys", "-T", "reload-stale", "z"])
        .output()
        .expect("query stale reload key");
    assert_eq!(stale.status.code(), Some(1));
    assert!(stale.stdout.is_empty());
    assert_eq!(stale.stderr, b"table reload-stale doesn't exist\n");
    let loaded = fixture
        .command()
        .env("XDG_CONFIG_HOME", &config_home)
        .env("HOME", &isolated_home)
        .args([
            "list-keys",
            "-1",
            "-T",
            "reload-loaded",
            "-F",
            "#{key_table}:#{key_string}",
            "z",
        ])
        .output()
        .expect("query reloaded key");
    assert_eq!(loaded.status.code(), Some(1));
    assert!(loaded.stdout.is_empty());
    assert_eq!(loaded.stderr, b"table reload-loaded doesn't exist\n");
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

fn capture_tui_until(
    fixture: &Fixture,
    attach: &[&str],
    needles: &[&[u8]],
) -> (bool, Vec<u8>, Option<std::process::ExitStatus>) {
    let mut command = fixture.command();
    command.args(attach);
    capture_command_until(command, needles, 80)
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

mod control_mode;
