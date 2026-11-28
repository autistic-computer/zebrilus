use super::test_support::*;
use super::*;
use crate::DEFAULT_HISTORY_LIMIT;

#[cfg(all(test, not(windows)))]
#[test]
fn terminal_command_preserves_tmux_argv_shapes() {
    let direct = terminal_command(&TerminalSpawn {
        command: Some(vec![
            "printf".to_owned(),
            "%s".to_owned(),
            "$HOME".to_owned(),
        ]),
        shell: Some("/bin/zsh".to_owned()),
        ..TerminalSpawn::default()
    });
    assert_eq!(
        direct.get_argv().as_slice(),
        [
            std::ffi::OsString::from("printf"),
            std::ffi::OsString::from("%s"),
            std::ffi::OsString::from("$HOME"),
        ]
    );

    let shell = terminal_command(&TerminalSpawn {
        command: Some(vec!["printf '%s' \"$HOME\"".to_owned()]),
        shell: Some("/bin/zsh".to_owned()),
        ..TerminalSpawn::default()
    });
    assert_eq!(
        shell.get_argv().as_slice(),
        [
            std::ffi::OsString::from("/bin/zsh"),
            std::ffi::OsString::from("-c"),
            std::ffi::OsString::from("printf '%s' \"$HOME\""),
        ]
    );

    let default = terminal_command(&TerminalSpawn {
        shell: Some("/bin/sh".to_owned()),
        ..TerminalSpawn::default()
    });
    assert!(default.is_default_prog());
    assert_eq!(default.get_shell(), "/bin/sh");

    let plain = terminal_command(&TerminalSpawn {
        shell: Some("/bin/sh".to_owned()),
        non_login_shell: true,
        ..TerminalSpawn::default()
    });
    assert!(!plain.is_default_prog());
    assert_eq!(
        plain.get_argv().as_slice(),
        [std::ffi::OsString::from("/bin/sh")]
    );

    let empty = terminal_command(&TerminalSpawn {
        command: Some(vec![String::new()]),
        shell: Some("/bin/sh".to_owned()),
        non_login_shell: true,
        ..TerminalSpawn::default()
    });
    assert_eq!(
        empty.get_argv().as_slice(),
        [
            std::ffi::OsString::from("/bin/sh"),
            std::ffi::OsString::from("-c"),
            std::ffi::OsString::new(),
        ]
    );
}

#[cfg(unix)]
#[test]
fn live_shell_starts_in_the_requested_working_directory() {
    let directory = tempfile::tempdir().expect("temporary working directory");
    let expected = directory
        .path()
        .canonicalize()
        .expect("canonical working directory");
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            working_directory: Some(expected.clone()),
            ..TerminalSpawn::default()
        },
    );
    let view = TerminalViewId(43);
    attach_streaming(&session, view);
    session.send_text("printf 'ZZ_WORKING_DIRECTORY:'; /bin/pwd -P\n");

    let expected = expected.to_string_lossy();
    let viewport = wait_for_test_viewport(&session, |viewport| {
        let mut contents = String::new();
        for cell in viewport.cells.iter() {
            viewport.push_glyph(*cell, &mut contents);
        }
        contents.contains("ZZ_WORKING_DIRECTORY:") && contents.contains(expected.as_ref())
    });
    assert!(matches!(viewport.status, SessionStatus::Running));
    assert!(session.foreground_process_id().is_some());
}

#[cfg(unix)]
#[test]
fn live_command_applies_spawn_environment_additions_and_removals() {
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            command: Some(vec![
                "read _; printf 'ZZ_PHASE4D_SET=%s\\n' \"$ZZ_PHASE4D_SET\"; if [ \"${ZZ_PHASE4D_REMOVE+x}\" = x ]; then printf 'ZZ_PHASE4D_REMOVE=set\\n'; else printf 'ZZ_PHASE4D_REMOVE=unset\\n'; fi; read _"
                    .to_owned(),
            ]),
            env: vec![
                ("ZZ_PHASE4D_SET".into(), Some("session".into())),
                (
                    "ZZ_PHASE4D_REMOVE".into(),
                    Some("daemon".into()),
                ),
                ("ZZ_PHASE4D_REMOVE".into(), None),
            ],
            ..TerminalSpawn::default()
        },
    );
    attach_streaming(&session, TerminalViewId(47));
    wait_for_test_viewport(&session, |viewport| {
        matches!(viewport.status, SessionStatus::Running)
    });
    session.send_text("ready\n");

    wait_for_test_viewport(&session, |viewport| {
        let mut contents = String::new();
        for cell in viewport.cells.iter() {
            viewport.push_glyph(*cell, &mut contents);
        }
        contents.contains("ZZ_PHASE4D_REMOVE=")
    });
    let capture = session.capture(CaptureOptions::default()).expect("capture");
    assert_eq!(
        capture
            .lines()
            .filter(|line| line.starts_with("ZZ_PHASE4D_"))
            .collect::<Vec<_>>(),
        ["ZZ_PHASE4D_SET=session", "ZZ_PHASE4D_REMOVE=unset"]
    );
}

#[cfg(windows)]
#[test]
fn windows_pane_commands_keep_quotes_and_spaced_paths() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let spaced = directory.path().join("with space");
    std::fs::create_dir(&spaced).expect("directory with a space");
    let path = spaced.join("quoted.txt");
    let screen = |view: u64, command: Vec<String>, marker: &str| {
        let session = TerminalSession::spawn(
            DEFAULT_HISTORY_LIMIT,
            Arc::new(TerminalAppearance::default()),
            TerminalSpawn {
                command: Some(command),
                ..TerminalSpawn::default()
            },
        );
        attach_streaming(&session, TerminalViewId(view));
        let mut contents = String::new();
        wait_for_test_viewport(&session, |viewport| {
            contents.clear();
            for cell in viewport.cells.iter() {
                viewport.push_glyph(*cell, &mut contents);
            }
            contents.contains(marker)
        });
        session.terminate();
        contents
    };

    let shell = screen(
        48,
        vec![format!(
            "echo \"a  b\">\"{path}\"& type \"{path}\"& set ZZ_PANE_COMMAND& echo ZZ_DONE& pause >nul",
            path = path.display()
        )],
        "ZZ_DONE",
    );
    assert!(shell.contains("\"a  b\""), "{shell:?}");
    assert!(!shell.contains("ZZ_PANE_COMMAND="), "{shell:?}");

    let direct = screen(
        49,
        vec![
            "choice".to_owned(),
            "/C".to_owned(),
            "y".to_owned(),
            "/M".to_owned(),
            "a  b".to_owned(),
        ],
        "[Y]?",
    );
    assert!(direct.contains("a  b [Y]?"), "{direct:?}");
}

#[cfg(unix)]
#[test]
fn pane_environment_seeds_term_and_truecolor_and_spawn_environment_overrides_them() {
    let capture = |view: u64,
                   terminal_type: Option<&str>,
                   env: Vec<(std::ffi::OsString, Option<std::ffi::OsString>)>| {
        let session = TerminalSession::spawn(
                DEFAULT_HISTORY_LIMIT,
                Arc::new(TerminalAppearance::default()),
                TerminalSpawn {
                    command: Some(vec![
                        "printf 'ZZ_TERM:%s ZZ_TRUECOLOR:%s\\n' \"$TERM\" \"$CLAUDE_CODE_TMUX_TRUECOLOR\"; read _"
                            .to_owned(),
                    ]),
                    terminal_type: terminal_type.map(str::to_owned),
                    env,
                    ..TerminalSpawn::default()
                },
            );
        attach_streaming(&session, TerminalViewId(view));
        wait_for_test_viewport(&session, |viewport| {
            let mut contents = String::new();
            for cell in viewport.cells.iter() {
                viewport.push_glyph(*cell, &mut contents);
            }
            contents.contains("ZZ_TRUECOLOR:")
        });
        session
            .capture(CaptureOptions::default())
            .expect("capture")
            .lines()
            .find(|line| line.starts_with("ZZ_TERM:"))
            .unwrap_or_default()
            .trim_end()
            .to_owned()
    };

    assert_eq!(
        capture(47, None, Vec::new()),
        "ZZ_TERM:tmux-256color ZZ_TRUECOLOR:1"
    );
    assert_eq!(
        capture(48, Some("zz-term"), Vec::new()),
        "ZZ_TERM:zz-term ZZ_TRUECOLOR:1"
    );
    assert_eq!(
        capture(
            49,
            Some("zz-term"),
            vec![("TERM".into(), Some("override".into()))]
        ),
        "ZZ_TERM:override ZZ_TRUECOLOR:1"
    );
    assert_eq!(
        capture(
            50,
            None,
            vec![("CLAUDE_CODE_TMUX_TRUECOLOR".into(), Some("".into()))],
        ),
        "ZZ_TERM:tmux-256color ZZ_TRUECOLOR:"
    );
}
