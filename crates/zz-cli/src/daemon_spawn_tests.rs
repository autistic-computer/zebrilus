use std::path::{Path, PathBuf};

use zz_protocol::RawText;

use super::{
    DaemonBootstrapArgumentError, DaemonBootstrapArguments, parse_daemon_bootstrap_arguments,
    validated_bootstrap_client_working_directory,
};

#[test]
fn tmux_import_hint_only_when_cli_spawns_without_mux_config() {
    let donor = Path::new("/home/u/.tmux.conf");
    assert!(
        super::tmux_import_hint(true, false, Some(donor))
            .unwrap()
            .contains("zz import-tmux-config")
    );
    assert!(super::tmux_import_hint(false, false, Some(donor)).is_none());
    assert!(super::tmux_import_hint(true, true, Some(donor)).is_none());
    assert!(super::tmux_import_hint(true, false, None).is_none());
}

#[test]
fn daemon_bootstrap_arguments_accept_only_the_ordered_private_grammar() {
    let path = std::env::temp_dir().join("client cwd [literal]*? with spaces");
    let path_string = path.to_str().expect("UTF-8 temporary path").to_owned();

    assert_eq!(
        parse_daemon_bootstrap_arguments(&[]).unwrap(),
        DaemonBootstrapArguments::default()
    );
    assert_eq!(
        parse_daemon_bootstrap_arguments(&["--bootstrap-server-id".into(), "42".into(),]).unwrap(),
        DaemonBootstrapArguments {
            server_id: Some(42),
            ready_fd: None,
            client_working_directory: None,
        }
    );
    assert_eq!(
        parse_daemon_bootstrap_arguments(&[
            "--bootstrap-server-id".into(),
            "42".into(),
            "--bootstrap-ready-fd".into(),
            "9".into(),
            "--bootstrap-client-cwd".into(),
            path_string.clone().into(),
        ])
        .unwrap(),
        DaemonBootstrapArguments {
            server_id: Some(42),
            ready_fd: Some(9),
            client_working_directory: Some(path.clone()),
        }
    );
    for ready_fd in [
        vec!["--bootstrap-ready-fd"],
        vec!["--bootstrap-ready-fd", "1"],
    ] {
        let mut arguments = vec![RawText::from("--bootstrap-server-id"), "42".into()];
        arguments.extend(ready_fd.into_iter().map(RawText::from));
        assert_eq!(
            parse_daemon_bootstrap_arguments(&arguments),
            Err(DaemonBootstrapArgumentError::ReadyFd)
        );
    }
    assert_eq!(
        parse_daemon_bootstrap_arguments(&[
            "--bootstrap-server-id".into(),
            "42".into(),
            "--bootstrap-client-cwd".into(),
            path_string.clone().into(),
        ])
        .unwrap(),
        DaemonBootstrapArguments {
            server_id: Some(42),
            ready_fd: None,
            client_working_directory: Some(path),
        }
    );

    for arguments in [
        vec![RawText::from("extra")],
        vec![RawText::from("--bootstrap-server-id")],
        vec![RawText::from("--bootstrap-server-id"), "nope".into()],
        vec![
            RawText::from("--bootstrap-client-cwd"),
            RawText::from(path_string.clone()),
        ],
    ] as [Vec<RawText>; 4]
    {
        assert_eq!(
            parse_daemon_bootstrap_arguments(&arguments),
            Err(DaemonBootstrapArgumentError::ServerId),
            "{arguments:?}"
        );
    }
    for arguments in [
        vec![
            "--bootstrap-server-id".into(),
            "42".into(),
            "--bootstrap-client-cwd".into(),
        ],
        vec![
            "--bootstrap-server-id".into(),
            "42".into(),
            "--other".into(),
            path_string.clone().into(),
        ],
        vec![
            "--bootstrap-server-id".into(),
            "42".into(),
            "--bootstrap-client-cwd".into(),
            "relative".into(),
        ],
        vec![
            "--bootstrap-server-id".into(),
            "42".into(),
            "--bootstrap-client-cwd".into(),
            path_string.clone().into(),
            "extra".into(),
        ],
    ] as [Vec<RawText>; 4]
    {
        assert_eq!(
            parse_daemon_bootstrap_arguments(&arguments),
            Err(DaemonBootstrapArgumentError::ClientWorkingDirectory),
            "{arguments:?}"
        );
    }
    assert_eq!(
        DaemonBootstrapArgumentError::ServerId.message(),
        "invalid bootstrap server id"
    );
    assert_eq!(
        DaemonBootstrapArgumentError::ClientWorkingDirectory.message(),
        "invalid bootstrap client cwd"
    );
}

#[test]
fn daemon_bootstrap_client_cwd_filter_accepts_only_bounded_absolute_utf8() {
    #[cfg(unix)]
    let prefix = "/";
    #[cfg(windows)]
    let prefix = "C:\\";
    let path_with_length = |length: usize| {
        PathBuf::from(format!(
            "{prefix}{}",
            "x".repeat(length.saturating_sub(prefix.len()))
        ))
    };
    let boundary = path_with_length(zz_protocol::MAX_CLIENT_WORKING_DIRECTORY_BYTES);
    assert_eq!(
        validated_bootstrap_client_working_directory(boundary.clone()),
        Some(boundary)
    );
    assert!(
        validated_bootstrap_client_working_directory(path_with_length(
            zz_protocol::MAX_CLIENT_WORKING_DIRECTORY_BYTES + 1
        ))
        .is_none()
    );
    assert!(validated_bootstrap_client_working_directory(PathBuf::from("relative/path")).is_none());
}

#[cfg(unix)]
#[test]
fn daemon_bootstrap_client_cwd_filter_rejects_non_utf8() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt as _};

    let path = PathBuf::from(OsString::from_vec(b"/tmp/client-\xff".to_vec()));
    assert!(validated_bootstrap_client_working_directory(path).is_none());
}

#[test]
fn daemon_executable_selects_the_installed_cli_and_keeps_a_lone_cli() {
    let directory = tempfile::tempdir().expect("temporary installation");
    let application = directory
        .path()
        .join(if cfg!(windows) { "zz.exe" } else { "zz" });
    let cli = directory
        .path()
        .join(if cfg!(windows) { "cli.exe" } else { "cli" });
    std::fs::write(&application, b"").expect("write application");
    assert_eq!(super::daemon_executable_from(&application), application);
    std::fs::create_dir(&cli).expect("create non-file CLI path");
    assert_eq!(super::daemon_executable_from(&application), application);
    std::fs::remove_dir(&cli).expect("remove non-file CLI path");
    std::fs::write(&cli, b"").expect("write CLI");
    assert_eq!(super::daemon_executable_from(&application), cli);
    std::fs::remove_file(&application).expect("remove application");
    assert_eq!(super::daemon_executable_from(&cli), cli);

    let bin = directory.path().join("bin");
    std::fs::create_dir(&bin).expect("create bin");
    let headless = bin.join(if cfg!(windows) { "zz.exe" } else { "zz" });
    std::fs::write(&headless, b"").expect("write headless zz");
    std::fs::write(bin.join(if cfg!(windows) { "cli.exe" } else { "cli" }), b"")
        .expect("write an unrelated cli");
    assert_eq!(super::daemon_executable_from(&headless), headless);
}

#[cfg(unix)]
#[test]
fn a_spawned_daemon_that_exits_is_reaped_while_the_spawner_lives() {
    use zz_daemon_client::unmasked::SpawnUnmasked as _;

    let child = std::process::Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .spawn_unmasked()
        .expect("spawn a short-lived child");
    let pid = rustix::process::Pid::from_child(&child);
    super::reap_when_exited(child);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while rustix::process::test_kill_process(pid).is_ok() {
        assert!(
            std::time::Instant::now() < deadline,
            "the exited child is still a zombie"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
