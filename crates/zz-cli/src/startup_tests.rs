use std::path::{Path, PathBuf};

use zz_protocol::RawText;

use super::{
    ApplicationArgumentError, TMUX_USAGE, TMUX_VERSION_OUTPUT, application_arguments,
    application_working_directory, implicit_tmux_endpoint_conflict,
};
#[cfg(unix)]
use super::{tmux_label_socket_path, tmux_socket_root};

#[test]
fn app_working_directory_prefers_the_launcher_and_uses_home_for_launch_services() {
    let home = tempfile::tempdir().expect("temporary home");
    let project = tempfile::tempdir().expect("temporary project");

    assert_eq!(
        application_working_directory(
            Some(project.path()),
            Some(Path::new("/")),
            Some(home.path()),
            true,
        ),
        Some(project.path().to_owned())
    );
    assert_eq!(
        application_working_directory(None, Some(Path::new("/")), Some(home.path()), true),
        Some(home.path().to_owned())
    );
    assert_eq!(
        application_working_directory(None, Some(project.path()), Some(home.path()), true),
        Some(project.path().to_owned())
    );
}

#[test]
fn socket_flag_overrides_the_environment_resolved_path() {
    let parsed = application_arguments(
        [
            "--socket".into(),
            "/tmp/forwarded.sock".into(),
            "list-sessions".into(),
        ],
        PathBuf::from("/tmp/zz-env.sock"),
    )
    .unwrap();
    assert_eq!(parsed.socket_path, PathBuf::from("/tmp/forwarded.sock"));
    assert_eq!(parsed.host, None);
    assert_eq!(parsed.remaining, ["list-sessions"]);

    let parsed = application_arguments(
        ["daemon".into(), "--socket=/tmp/daemon.sock".into()],
        PathBuf::from("/tmp/default.sock"),
    )
    .unwrap();
    assert_eq!(parsed.socket_path, PathBuf::from("/tmp/daemon.sock"));
    assert_eq!(parsed.host, None);
    assert_eq!(parsed.remaining, ["daemon"]);
}

#[test]
fn tmux_environment_never_becomes_an_implicit_zz_endpoint() {
    let tmux = std::ffi::OsStr::new("tmux.sock,123,4");
    assert!(implicit_tmux_endpoint_conflict(
        super::SocketSelectionSource::Default,
        None,
        Some(tmux),
    ));
    assert!(!implicit_tmux_endpoint_conflict(
        super::SocketSelectionSource::Default,
        Some(std::ffi::OsStr::new("/tmp/zz.sock")),
        Some(tmux),
    ));
    assert!(!implicit_tmux_endpoint_conflict(
        super::SocketSelectionSource::Path,
        None,
        Some(tmux),
    ));
    assert!(!implicit_tmux_endpoint_conflict(
        super::SocketSelectionSource::Default,
        None,
        None,
    ));
}

#[test]
fn tmux_version_and_help_are_exact_early_outputs() {
    let version = application_arguments(
        ["-2uV".into(), "ignored".into()],
        PathBuf::from("/tmp/default.sock"),
    )
    .unwrap();
    assert_eq!(version.early_output, Some(TMUX_VERSION_OUTPUT));

    let help = application_arguments(
        ["-vh".into(), "ignored".into()],
        PathBuf::from("/tmp/default.sock"),
    )
    .unwrap();
    assert_eq!(help.early_output, Some(TMUX_USAGE));
}

#[test]
fn tmux_flags_compose_before_the_command_word() {
    let parsed = application_arguments(
        [
            "-2u".into(),
            "-lN".into(),
            "-f".into(),
            "/tmp/first.conf".into(),
            "-f/tmp/second.conf".into(),
            "-S/tmp/tmux.sock".into(),
            "new-session".into(),
            "-d".into(),
            "-f".into(),
            "pane-command".into(),
        ],
        PathBuf::from("/tmp/default.sock"),
    )
    .unwrap();
    assert_eq!(parsed.socket_path, PathBuf::from("/tmp/tmux.sock"));
    assert!(parsed.socket_source.is_overridden());
    assert!(parsed.no_start_server);
    assert!(parsed.login_shell);
    assert_eq!(
        parsed.mux_config_files,
        [
            PathBuf::from("/tmp/first.conf"),
            PathBuf::from("/tmp/second.conf")
        ]
    );
    assert_eq!(
        parsed.remaining,
        ["new-session", "-d", "-f", "pane-command"]
    );
}

#[test]
fn the_last_zz_or_tmux_socket_selector_wins() {
    let parsed = application_arguments(
        [
            "-S".into(),
            "/tmp/tmux.sock".into(),
            "--socket=/tmp/zz.sock".into(),
            "list-sessions".into(),
        ],
        PathBuf::from("/tmp/default.sock"),
    )
    .unwrap();
    assert_eq!(parsed.socket_path, PathBuf::from("/tmp/zz.sock"));

    let parsed = application_arguments(
        [
            "--socket".into(),
            "/tmp/zz.sock".into(),
            "-S/tmp/tmux.sock".into(),
            "list-sessions".into(),
        ],
        PathBuf::from("/tmp/default.sock"),
    )
    .unwrap();
    assert_eq!(parsed.socket_path, PathBuf::from("/tmp/tmux.sock"));
}

#[test]
fn tmux_shell_command_is_exclusive_and_preserves_login_mode() {
    let parsed = application_arguments(
        ["-lc".into(), "printf ok".into()],
        PathBuf::from("/tmp/default.sock"),
    )
    .unwrap();
    assert_eq!(parsed.shell_command.as_deref(), Some("printf ok"));
    assert!(parsed.login_shell);
    assert!(parsed.remaining.is_empty());

    assert_eq!(
        application_arguments(
            ["-cprintf ok".into(), "list-sessions".into()],
            PathBuf::from("/tmp/default.sock")
        ),
        Err(ApplicationArgumentError::Usage)
    );
}

#[test]
fn unsupported_and_unknown_tmux_flags_fail_loudly() {
    for flag in ["-8", "-d", "-U", "-x"] {
        let expected = format!("zz: unknown option -- {}\n{TMUX_USAGE}", &flag[1..2]);
        assert!(
            matches!(
                application_arguments([RawText::from(flag)], PathBuf::from("/tmp/default.sock")),
                Err(ApplicationArgumentError::Raw(message)) if message == expected
            ),
            "{flag}"
        );
    }
    assert_eq!(
        application_arguments(["--unknown".into()], PathBuf::from("/tmp/default.sock")),
        Err(ApplicationArgumentError::Usage)
    );
    assert!(matches!(
        application_arguments(["-L".into()], PathBuf::from("/tmp/default.sock")),
        Err(ApplicationArgumentError::Raw(message))
            if message == format!("zz: option requires an argument -- L\n{TMUX_USAGE}")
    ));
    assert!(matches!(
        application_arguments(["-D".into()], PathBuf::from("/tmp/default.sock")),
        Err(ApplicationArgumentError::Message(message))
            if message == "-D foreground server mode is not supported; use `zz daemon`"
    ));
    assert_eq!(
        application_arguments(
            ["-D".into(), "list-sessions".into()],
            PathBuf::from("/tmp/default.sock")
        ),
        Err(ApplicationArgumentError::Usage)
    );
}

#[test]
fn control_flags_count_and_compose_with_tmux_options() {
    for (flag, expected) in [("-C", 1), ("-CC", 2), ("-CCC", 3)] {
        let parsed = application_arguments(
            [RawText::from(flag), "list-sessions".into()],
            PathBuf::from("/tmp/default.sock"),
        )
        .unwrap();
        assert_eq!(parsed.control_mode, expected);
        assert_eq!(parsed.remaining, ["list-sessions"]);
    }
    let parsed = application_arguments(
        [
            "-2CulN".into(),
            "-f/tmp/control.conf".into(),
            "-S/tmp/control.sock".into(),
            "new-session".into(),
        ],
        PathBuf::from("/tmp/default.sock"),
    )
    .unwrap();
    assert_eq!(parsed.control_mode, 1);
    assert!(parsed.login_shell);
    assert!(parsed.no_start_server);
    assert_eq!(
        parsed.mux_config_files,
        [PathBuf::from("/tmp/control.conf")]
    );
    assert_eq!(parsed.socket_path, PathBuf::from("/tmp/control.sock"));
    assert_eq!(parsed.remaining, ["new-session"]);
    let shell = application_arguments(
        ["-Cc".into(), "printf ok".into()],
        PathBuf::from("/tmp/default.sock"),
    )
    .unwrap();
    assert_eq!(shell.control_mode, 1);
    assert_eq!(shell.shell_command.as_deref(), Some("printf ok"));
}

#[cfg(unix)]
#[test]
fn tmux_label_uses_tmpdir_then_the_tmp_fallback() {
    use std::os::unix::fs::MetadataExt as _;

    let directory = tempfile::tempdir().expect("temporary TMUX_TMPDIR");
    let root = std::fs::canonicalize(directory.path()).expect("canonical TMUX_TMPDIR");
    assert_eq!(
        tmux_socket_root(Some(directory.path().as_os_str())),
        Some(root.clone())
    );
    assert_eq!(
        tmux_socket_root(Some(directory.path().join("missing").as_os_str())),
        std::fs::canonicalize("/tmp").ok()
    );
    assert_eq!(tmux_socket_root(None), std::fs::canonicalize("/tmp").ok());

    let uid = rustix::process::getuid().as_raw();
    let path = tmux_label_socket_path("work", Some(directory.path().as_os_str())).unwrap();
    assert_eq!(path, root.join(format!("tmux-{uid}/work")));
    let metadata = std::fs::symlink_metadata(path.parent().unwrap()).unwrap();
    assert_eq!(metadata.uid(), uid);
    assert_eq!(metadata.mode() & 0o007, 0);
}

#[test]
fn host_flag_accepts_both_spellings_and_conflicts_with_socket() {
    let parsed = application_arguments(
        ["--host".into(), "desktop".into(), "list-sessions".into()],
        PathBuf::from("/tmp/default.sock"),
    )
    .unwrap();
    assert_eq!(parsed.socket_path, PathBuf::from("/tmp/default.sock"));
    assert_eq!(parsed.host.as_deref(), Some("desktop"));
    assert_eq!(parsed.remaining, ["list-sessions"]);

    let parsed = application_arguments(
        ["--host=gpu".into(), "list-panes".into()],
        PathBuf::from("/tmp/default.sock"),
    )
    .unwrap();
    assert_eq!(parsed.host.as_deref(), Some("gpu"));
    assert_eq!(parsed.remaining, ["list-panes"]);

    for arguments in [
        &["list-panes", "--host=gpu"][..],
        &["run-pane", "-t", "%1", "--", "zz", "--host", "xps", "ls"][..],
        &["send-keys", "-t", "%1", "--", "--host"][..],
        &[
            "run-pane", "--", "npm", "run", "dev", "--", "--host", "0.0.0.0",
        ][..],
        &["list-panes", "--socket", "/tmp/other.sock"][..],
        &["run-pane", "--socket=/tmp/other.sock"][..],
        &["daemon", "--", "--socket", "/tmp/other.sock"][..],
        &["--", "--host", "gpu"][..],
    ] {
        let parsed = application_arguments(
            arguments.iter().copied().map(RawText::from),
            PathBuf::from("/tmp/default.sock"),
        )
        .unwrap();
        assert_eq!(parsed.host, None, "{arguments:?}");
        assert_eq!(
            parsed.socket_path,
            PathBuf::from("/tmp/default.sock"),
            "{arguments:?}"
        );
        let expected = arguments
            .iter()
            .skip(usize::from(arguments[0] == "--"))
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(parsed.remaining, expected, "{arguments:?}");
    }

    for verb in ["daemon", "proxy", "app"] {
        let parsed = application_arguments(
            [verb.into(), "--socket".into(), "/tmp/verb.sock".into()],
            PathBuf::from("/tmp/default.sock"),
        )
        .unwrap();
        assert_eq!(
            parsed.socket_path,
            PathBuf::from("/tmp/verb.sock"),
            "{verb}"
        );
        assert_eq!(parsed.remaining, [verb]);
    }

    for arguments in [
        vec![
            "--host".into(),
            "desktop".into(),
            "--socket=/tmp/daemon.sock".into(),
            "list-sessions".into(),
        ],
        vec![
            "--socket".into(),
            "/tmp/daemon.sock".into(),
            "--host=desktop".into(),
            "list-sessions".into(),
        ],
    ] as [Vec<RawText>; 2]
    {
        assert!(application_arguments(arguments, PathBuf::from("/tmp/default.sock")).is_err());
    }
}

#[test]
fn host_values_that_look_like_options_are_refused() {
    for arguments in [
        vec!["--host".into(), "-oProxyCommand=x".into(), "ls".into()],
        vec!["--host=-x".into(), "ls".into()],
    ] as [Vec<RawText>; 2]
    {
        assert!(matches!(
            application_arguments(arguments, PathBuf::from("/tmp/default.sock")),
            Err(ApplicationArgumentError::Message(message)) if message.contains("must not start with `-`")
        ));
    }
}

#[test]
fn a_prompt_would_reach_the_command_dispatcher_if_askpass_mode_did_not_come_first() {
    let parsed = application_arguments(
        ["demfabris@xps's password: ".into()],
        PathBuf::from("/tmp/default.sock"),
    )
    .unwrap();
    assert_eq!(parsed.remaining, ["demfabris@xps's password: "]);
}

#[test]
fn socket_flag_requires_a_non_empty_path() {
    assert!(
        application_arguments(["--socket".into()], PathBuf::from("/tmp/default.sock")).is_err()
    );
    assert!(
        application_arguments(["--socket=".into()], PathBuf::from("/tmp/default.sock")).is_err()
    );
    assert!(application_arguments(["--host".into()], PathBuf::from("/tmp/default.sock")).is_err());
    assert!(application_arguments(["--host=".into()], PathBuf::from("/tmp/default.sock")).is_err());
}

#[cfg(not(windows))]
#[test]
#[allow(unsafe_code, clippy::undocumented_unsafe_blocks)]
fn purge_delay_names_mimallocs_v3_option() {
    if std::env::var_os("MIMALLOC_PURGE_DELAY").is_some() {
        return;
    }
    unsafe extern "C" {
        fn mi_option_get(option: std::ffi::c_int) -> std::ffi::c_long;
    }
    assert_eq!(unsafe { mi_option_get(super::MI_OPTION_PURGE_DELAY) }, 1000);
}
