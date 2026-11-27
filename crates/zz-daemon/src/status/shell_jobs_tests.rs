#[cfg(unix)]
use super::test_support::{counted_value_job, engine_request, execute};
use super::test_support::{request, settled};
#[cfg(unix)]
use super::*;
use crate::daemon::status_jobs::tests::renderer;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[test]
fn commands_run_once_and_then_come_from_the_cache() {
    let mut renderer = renderer();
    let directory = tempfile::Builder::new()
        .prefix("zz-status-cache-")
        .tempdir_in(".")
        .expect("the cache fixture is created");
    let source = directory.path().join("value");
    std::fs::write(&source, "first\n").expect("the first value is written");
    #[cfg(unix)]
    let command = format!("cat '{}'", source.display());
    #[cfg(windows)]
    let command = format!("type {}", source.display());
    let format = format!("#({command})");
    let requests = [request(1, &format, "")];

    let first = renderer.render_changed(&requests);
    assert_eq!(
        first[0].1.left, "",
        "a first render starts the command without waiting"
    );
    assert_eq!(settled(&mut renderer, &requests[0]).left, "first");
    std::fs::write(&source, "second\n").expect("the second value is written");
    assert_eq!(renderer.render_forced(&requests[0]).left, "first");
    assert_eq!(settled(&mut renderer, &requests[0]).left, "second");
}

#[cfg(unix)]
#[test]
fn attached_status_jobs_start_in_each_clients_working_directory() {
    let directory = tempfile::tempdir().expect("working directory fixture");
    let first_cwd = directory.path().join("first");
    let second_cwd = directory.path().join("second");
    std::fs::create_dir(&first_cwd).expect("first cwd");
    std::fs::create_dir(&second_cwd).expect("second cwd");
    let first_cwd = std::fs::canonicalize(first_cwd).expect("first cwd resolves");
    let second_cwd = std::fs::canonicalize(second_cwd).expect("second cwd resolves");
    let mut first = request(1, "#(pwd -P)", "");
    Arc::make_mut(&mut first.context).session_path = first_cwd.to_string_lossy().into_owned();
    Arc::make_mut(&mut first.facts).client = Some(ClientFormatFacts::default());
    let mut second = request(2, "#(pwd -P)", "");
    Arc::make_mut(&mut second.context).session_path = second_cwd.to_string_lossy().into_owned();
    Arc::make_mut(&mut second.facts).client = Some(ClientFormatFacts::default());

    let mut renderer = renderer();
    let statuses = [
        (first.client, settled(&mut renderer, &first)),
        (second.client, settled(&mut renderer, &second)),
    ];

    assert_eq!(statuses.len(), 2);
    assert_eq!(statuses[0].1.left, first_cwd.to_string_lossy());
    assert_eq!(statuses[1].1.left, second_cwd.to_string_lossy());
}

#[cfg(unix)]
#[test]
fn a_client_with_no_attached_session_runs_no_status_job() {
    let directory = tempfile::tempdir().expect("working directory fixture");
    let source = directory.path().join("value");
    std::fs::write(&source, "first\n").expect("the first value is written");
    let format = format!("#(cat '{}')", source.display());
    let cwd = std::fs::canonicalize(directory.path()).expect("working directory resolves");
    let mut attached = request(1, &format, "");
    Arc::make_mut(&mut attached.context).session_path = cwd.to_string_lossy().into_owned();
    let mut clientless = request(2, &format, "");
    Arc::make_mut(&mut clientless.context).session_path = cwd.to_string_lossy().into_owned();
    Arc::make_mut(&mut clientless.facts).client = None;
    let mut renderer = renderer();

    assert_eq!(settled(&mut renderer, &attached).left, "first");
    assert_eq!(renderer.shell_cache.len(), 1);
    assert_eq!(settled(&mut renderer, &clientless).left, "");
    assert_eq!(renderer.shell_cache.len(), 1);
    assert_eq!(
        renderer.shell_cache.keys().next().unwrap().0,
        ClientId(1),
        "a clientless render adds no second job for the same command"
    );
}

#[cfg(unix)]
#[test]
fn status_command_cache_keeps_attached_clients_independent() {
    let directory = tempfile::tempdir().expect("working directory fixture");
    let source = directory.path().join("value");
    std::fs::write(&source, "first\n").expect("the first value is written");
    let format = format!("#(cat '{}')", source.display());
    let cwd = std::fs::canonicalize(directory.path()).expect("working directory resolves");
    let mut first = request(1, &format, "");
    Arc::make_mut(&mut first.context).session_path = cwd.to_string_lossy().into_owned();
    Arc::make_mut(&mut first.facts).client = Some(ClientFormatFacts::default());
    let mut renderer = renderer();

    assert_eq!(settled(&mut renderer, &first).left, "first");
    std::fs::write(&source, "second\n").expect("the second value is written");
    let mut second = request(2, &format, "");
    Arc::make_mut(&mut second.context).session_path = cwd.to_string_lossy().into_owned();
    Arc::make_mut(&mut second.facts).client = Some(ClientFormatFacts::default());

    assert_eq!(settled(&mut renderer, &second).left, "second");
    assert_eq!(renderer.shell_cache.len(), 2);
    renderer.forget(ClientId(1));
    assert_eq!(renderer.shell_cache.len(), 1);
}

#[cfg(unix)]
#[test]
fn status_commands_receive_tmux_and_working_directory_without_tmux_pane() {
    let directory = tempfile::Builder::new()
        .prefix("zz-status-environment-")
        .tempdir_in(".")
        .expect("the working directory is created");
    let pane_directory = tempfile::Builder::new()
        .prefix("zz-status-pane-decoy-")
        .tempdir_in(".")
        .expect("the pane working directory is created");
    let socket = "/tmp/zz-status-environment.sock";
    let mut status_request = request(
        1,
        "#(echo \"$TMUX|$ZZ_SOCKET|$PWD|${TMUX_PANE-unset}\")",
        "",
    );
    Arc::make_mut(&mut status_request.context).socket_path = socket.to_owned();
    Arc::make_mut(&mut status_request.context).session_path = directory
        .path()
        .canonicalize()
        .expect("the working directory resolves")
        .to_string_lossy()
        .into_owned();
    Arc::make_mut(&mut status_request.context).pane_current_path = pane_directory
        .path()
        .canonicalize()
        .expect("the pane working directory resolves")
        .to_string_lossy()
        .into_owned();

    let status = settled(&mut renderer(), &status_request);

    assert_eq!(
        status.left,
        format!(
            "{socket},{},-1|{socket}|{}|unset",
            std::process::id(),
            status_request.context.session_path
        )
    );
}

#[cfg(unix)]
#[test]
fn status_commands_use_global_only_clean_environment_and_the_startup_gate() {
    let mut engine = MuxEngine::default();
    engine.seed_global_environment([
        ("STATUS_SCOPE", "global"),
        ("TMUX_PANE", "status-global-pane"),
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
    let mut execution = zz_mux::ExecutionContext::default();
    execute(
        &mut engine,
        &mut execution,
        &["new-session", "-d", "-s", "status-environment"],
    );
    let session = execution.session.expect("status session");
    for args in [
        vec!["set-environment", "-g", "-h", "STATUS_HIDDEN", "hidden"],
        vec!["set-environment", "-g", "-r", "STATUS_UNSET"],
        vec![
            "set-environment",
            "-t",
            "status-environment",
            "STATUS_SCOPE",
            "session",
        ],
        vec![
            "set-environment",
            "-t",
            "status-environment",
            "TMUX_PANE",
            "session-pane",
        ],
        vec!["set-option", "-g", "default-terminal", "status-terminal"],
    ] {
        execute(&mut engine, &mut execution, &args);
    }
    let command = "#(printf '%%s|%%s|%%s|%%s|%%s|%%s|%%s|%%s|%%s|%%s|%%s|%%s' \"$STATUS_SCOPE\" \"${STATUS_HIDDEN-unset}\" \"${STATUS_UNSET-unset}\" \"${HOME-unset}\" \"${TERM-unset}\" \"${TERM_PROGRAM-unset}\" \"${TERM_PROGRAM_VERSION-unset}\" \"${COLORTERM-unset}\" \"$TMUX\" \"$TMUX_PANE\" \"${ZZ_STARTUP_REENTRY-unset}\" \"${ZZ_TMUX_EXECUTABLE-unset}\")";
    let socket = "/tmp/zz-status-clean-environment.sock";
    let expected_tmux = format!("{socket},{},-1", std::process::id());

    let mut post_startup = engine_request(1, &engine, Some(session));
    post_startup.formats = StatusFormats {
        enabled: true,
        left: command.to_owned(),
        style: String::new(),
        left_style: String::new(),
        right_style: String::new(),
        foreground: "default".to_owned(),
        background: "default".to_owned(),
        left_length: u16::MAX,
        right_length: u16::MAX,
        ..StatusFormats::default()
    }
    .into();
    Arc::make_mut(&mut post_startup.context).socket_path = socket.to_owned();
    let status = settled(&mut renderer(), &post_startup);
    assert_eq!(
        status.left,
        format!(
            "global|unset|unset|unset|status-terminal|tmux|3.8-zz|truecolor|{expected_tmux}|status-global-pane|unset|unset"
        )
    );

    let mut startup = engine_request(1, &engine, Some(session));
    startup.formats = StatusFormats {
        enabled: true,
        left: command.to_owned(),
        style: String::new(),
        left_style: String::new(),
        right_style: String::new(),
        foreground: "default".to_owned(),
        background: "default".to_owned(),
        left_length: u16::MAX,
        right_length: u16::MAX,
        ..StatusFormats::default()
    }
    .into();
    Arc::make_mut(&mut startup.context).socket_path = socket.to_owned();
    startup.startup = true;
    let status = settled(&mut renderer(), &startup);
    assert_eq!(
        status.left,
        format!(
            "global|unset|unset|unset|startup-term|startup-program|startup-version|startup-colorterm|{expected_tmux}|status-global-pane|unset|unset"
        )
    );
}

#[cfg(unix)]
#[test]
fn status_commands_resolve_literal_tmux_through_the_private_zz_shim() {
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().expect("temporary status shim");
    let executable = directory.path().join("fake-zz");
    std::fs::write(
        &executable,
        b"#!/bin/sh\nprintf '%s|%s' \"$1\" \"$ZZ_SOCKET\"\n",
    )
    .expect("write fake zz executable");
    let shim = directory.path().join("shim");
    std::fs::create_dir(&shim).expect("create shim directory");
    std::fs::write(
        shim.join("tmux"),
        b"#!/bin/sh\nexec \"$ZZ_TMUX_EXECUTABLE\" \"$@\"\n",
    )
    .expect("write tmux shim");
    for path in [&executable, &shim.join("tmux")] {
        let mut permissions = std::fs::metadata(path)
            .expect("shim metadata")
            .permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(path, permissions).expect("make shim runnable");
    }

    let socket = "/tmp/zz-status-shim.sock";
    let mut status_request = request(1, "#(tmux status)", "");
    Arc::make_mut(&mut status_request.context).socket_path = socket.to_owned();
    let mut renderer = renderer();
    renderer.set_tmux_shim(shim, executable);

    assert_eq!(
        settled(&mut renderer, &status_request).left,
        format!("status|{socket}")
    );
}

#[cfg(unix)]
#[test]
fn status_job_output_wakes_the_renderer_without_a_periodic_deadline() {
    let mut renderer = renderer();
    struct JobWake(std::sync::mpsc::Sender<()>);
    impl std::task::Wake for JobWake {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }
    let (sender, updates) = std::sync::mpsc::channel();
    renderer.set_job_waker(std::task::Waker::from(Arc::new(JobWake(sender))));
    let request = request(1, "#(printf ready)", "");
    assert!(renderer.render_initial(&request).left.is_empty());
    let deadline = Instant::now() + Duration::from_millis(750);
    loop {
        updates
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("job notification");
        renderer.poll_jobs();
        if renderer
            .shell_cache
            .values()
            .any(|entry| entry.output.as_deref() == Some("ready"))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "shell output did not wake the renderer"
        );
    }
    assert!(
        Instant::now() < deadline,
        "shell output did not arrive promptly"
    );
}

#[cfg(unix)]
#[test]
fn status_jobs_keep_the_last_line_and_trailing_spaces() {
    let status = settled(
        &mut renderer(),
        &request(1, "#(printf 'one\\ntwo \\t\\n')", "#(printf 'one\\ntail')"),
    );
    assert_eq!(status.left, "two \t");
    assert_eq!(status.right, "tail");
}

#[cfg(unix)]
#[test]
fn status_jobs_preserve_complete_long_lines_in_the_cache() {
    let mut renderer = renderer();
    let expected = format!("{}TAIL", "0".repeat(5000));
    settled(
        &mut renderer,
        &request(
            1,
            &format!("#(printf '{expected}\\n')"),
            &format!("#(printf '{expected}')"),
        ),
    );
    assert_eq!(renderer.shell_cache.len(), 2);
    for entry in renderer.shell_cache.values() {
        assert_eq!(entry.output.as_deref(), Some(expected.as_str()));
    }
}

#[cfg(unix)]
#[test]
fn a_running_job_publishes_lines_and_is_cancelled_on_forget() {
    let directory = tempfile::tempdir().expect("job fixture");
    let release = directory.path().join("release");
    let format = format!(
        "#(mkfifo '{}'; printf 'ready\\n'; read token < '{}'; printf 'done\\n')",
        release.display(),
        release.display()
    );
    let mut request = request(1, &format, "");
    Arc::make_mut(&mut request.facts).client = Some(ClientFormatFacts::default());
    let mut renderer = renderer();
    let started = Instant::now();
    assert!(renderer.render_initial(&request).left.is_empty());
    assert!(started.elapsed() < Duration::from_secs(1));
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        renderer.poll_jobs();
        if renderer.render_initial(&request).left == "ready" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "running job output never published"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let pid = renderer
        .shell_cache
        .values()
        .next()
        .unwrap()
        .job
        .as_ref()
        .unwrap()
        .pid;
    renderer.forget(request.client);
    assert!(renderer.shell_cache.is_empty());
    let deadline = Instant::now() + Duration::from_secs(3);
    while rustix::process::test_kill_process(rustix::process::Pid::from_raw(pid as i32).unwrap())
        .is_ok()
    {
        assert!(
            Instant::now() < deadline,
            "cancelled status job was not reaped"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
#[test]
fn the_same_job_in_two_status_formats_runs_once() {
    let directory = tempfile::tempdir().expect("job count fixture");
    let count = directory.path().join("count");
    let format = counted_value_job(&count);
    let mut renderer = renderer();
    let request = request(1, &format, &format);
    assert_eq!(renderer.render_forced(&request).left, "");
    let status = settled(&mut renderer, &request);
    assert_eq!(status.left, "value");
    assert_eq!(status.right, "value");
    assert_eq!(std::fs::read_to_string(count).unwrap(), "run\n");
}
