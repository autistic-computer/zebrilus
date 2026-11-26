use super::test_support::*;
use super::*;
use crate::DEFAULT_HISTORY_LIMIT;

#[cfg(unix)]
fn wait_for_last_command(
    session: &TerminalSession,
    accept: impl Fn(&LastCommandCapture) -> bool,
) -> LastCommandCapture {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let capture = session.capture_last_command();
        if let Ok(capture) = &capture
            && accept(capture)
        {
            return capture.clone();
        }
        assert!(
            Instant::now() < deadline,
            "last command capture never converged: {capture:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn shell_integration_command_lifecycle(shell: &str) {
    if !std::path::Path::new(shell).is_file() {
        eprintln!("skipping shell integration fixture: {shell} is not installed");
        return;
    }
    if matches!(
        std::env::var("ZZ_SHELL_INTEGRATION")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "none" | "false" | "0"
    ) {
        eprintln!("skipping shell integration fixture: ZZ_SHELL_INTEGRATION disables it");
        return;
    }
    let temporary = tempfile::tempdir().expect("shell integration fixture home");
    let home = temporary.path();
    let setup = r#"__zz_fixture_prompt=0
__zz_fixture_precmd() {
  __zz_fixture_status=$?
  __zz_fixture_prompt=$((__zz_fixture_prompt + 1))
  PS1="zz-fixture-${__zz_fixture_prompt}:${__zz_fixture_status}> "
}
"#;
    let hooks = if shell.ends_with("bash") {
        r#"PROMPT_COMMAND='__zz_fixture_precmd;'
trap 'printf "%s\n" "$BASH_COMMAND" >> "$HOME/debug-commands"' DEBUG
"#
    } else {
        r#"precmd() { __zz_fixture_precmd; }
__zz_fixture_preexec() { print -r -- "$1" >> "$HOME/debug-commands"; }
preexec_functions+=(__zz_fixture_preexec)
"#
    };
    for name in [".bash_profile", ".zshrc"] {
        std::fs::write(home.join(name), format!("{setup}{hooks}")).expect("isolated shell startup");
    }
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            shell: Some(shell.to_owned()),
            working_directory: Some(home.to_path_buf()),
            env: vec![
                ("HOME".into(), Some(home.as_os_str().to_owned())),
                (
                    "HISTFILE".into(),
                    Some(home.join("history").into_os_string()),
                ),
                ("ZZ_ZSH_ZDOTDIR".into(), Some(home.as_os_str().to_owned())),
                ("PROMPT_COMMAND".into(), None),
                ("PS1".into(), None),
                ("PS0".into(), None),
            ],
            ..TerminalSpawn::default()
        },
    );
    attach_streaming(&session, TerminalViewId(144));
    wait_for_test_capture(&session, |capture| capture.contains("zz-fixture-1:"));
    assert_eq!(
        session
            .capture_last_command()
            .expect("initial prompt marks"),
        LastCommandCapture::default()
    );
    let (sink, output) = TestOutputSink::install(&session);
    let submit = |line: &str, prompt: &str| {
        session.send_text(line);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bytes = Vec::new();
        let end = format!("{prompt}\x1b]133;B\x07");
        while !bytes
            .windows(end.len())
            .any(|window| window == end.as_bytes())
        {
            let chunk = output.recv_deadline(deadline).unwrap_or_else(|error| {
                panic!(
                    "{shell}: waiting for {prompt:?}: {error}; output: {:?}",
                    String::from_utf8_lossy(&bytes)
                )
            });
            bytes.extend_from_slice(&chunk);
        }
        let visible = prompt.trim_end();
        wait_for_test_capture(&session, |capture| capture.contains(visible));
        String::from_utf8(bytes).expect("fixture output is UTF-8")
    };
    let completed = |bytes: &str, status: u8| {
        let end = format!("\x1b]133;D;{status}\x07");
        assert_eq!(bytes.matches("\x1b]133;C\x07").count(), 1, "{bytes:?}");
        assert_eq!(bytes.matches("\x1b]133;D;").count(), 1, "{bytes:?}");
        let start = bytes.find("\x1b]133;C\x07").expect("command start");
        let finish = bytes.find(&end).expect("command exit status");
        let prompt = bytes.find("\x1b]133;A").expect("prompt start");
        let input = bytes.rfind("\x1b]133;B\x07").expect("prompt end");
        assert!(
            start < finish && finish < prompt && prompt < input,
            "{bytes:?}"
        );
    };

    let bytes = submit("echo hi\n", "zz-fixture-2:0> ");
    completed(&bytes, 0);
    let first = wait_for_last_command(&session, |capture| capture.command == "echo hi");
    assert_eq!(first.output, "hi");
    assert_eq!(first.truncated_rows, 0);
    assert_eq!(session.last_command_status(), Some(0));

    let bytes = submit("\n", "zz-fixture-3:0> ");
    assert!(!bytes.contains("\x1b]133;C"), "{bytes:?}");
    assert!(!bytes.contains("\x1b]133;D"), "{bytes:?}");
    assert_eq!(
        wait_for_last_command(&session, |capture| capture.command == "echo hi"),
        first
    );

    let bytes = submit("false\n", "zz-fixture-4:1> ");
    completed(&bytes, 1);
    let failed = wait_for_last_command(&session, |capture| capture.command == "false");
    assert_eq!(failed.output, "");
    assert_eq!(session.last_command_status(), Some(1));

    let bytes = submit("echo one; echo two\n", "zz-fixture-5:0> ");
    completed(&bytes, 0);
    let compound =
        wait_for_last_command(&session, |capture| capture.command == "echo one; echo two");
    assert_eq!(compound.output, "one\ntwo");
    let hooks = std::fs::read_to_string(home.join("debug-commands"))
        .expect("preserved user execution hook");
    assert!(hooks.contains("echo hi"), "{hooks:?}");
    assert!(hooks.contains("false"), "{hooks:?}");
    sink.take(false);
}

#[cfg(unix)]
#[test]
fn bash_shell_integration_marks_command_lifecycle() {
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &["/opt/homebrew/bin/bash", "/usr/local/bin/bash"]
    } else {
        &["/bin/bash", "/usr/bin/bash"]
    };
    let Some(shell) = candidates
        .iter()
        .find(|shell| std::path::Path::new(shell).is_file())
    else {
        eprintln!("skipping bash shell integration fixture: no supported bash installed");
        return;
    };
    shell_integration_command_lifecycle(shell);
}

#[cfg(unix)]
#[test]
fn zsh_shell_integration_marks_command_lifecycle() {
    shell_integration_command_lifecycle("/bin/zsh");
}

#[cfg(unix)]
fn shell_integration_restores_tmux_directory_after_startup(shell: &str) {
    if !std::path::Path::new(shell).is_file() {
        eprintln!("skipping shell integration fixture: {shell} is not installed");
        return;
    }
    if matches!(
        std::env::var("ZZ_SHELL_INTEGRATION")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "none" | "false" | "0"
    ) {
        eprintln!("skipping shell integration fixture: ZZ_SHELL_INTEGRATION disables it");
        return;
    }
    let temporary = tempfile::tempdir().expect("shell integration fixture home");
    let home = temporary.path();
    let tmux_directory = home.join("tmux");
    std::fs::create_dir(&tmux_directory).expect("tmux directory");
    let startup = r#"export PATH="/usr/bin:$HOME/bin:$PATH"
PS1='zz-path-fixture> '
"#;
    let startup_file = if shell.ends_with("bash") {
        ".bash_profile"
    } else {
        ".zlogin"
    };
    std::fs::write(home.join(startup_file), startup).expect("isolated shell startup");
    let path = std::env::join_paths([
        tmux_directory.as_path(),
        std::path::Path::new("/usr/bin"),
        std::path::Path::new("/bin"),
    ])
    .expect("fixture PATH");
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            shell: Some(shell.to_owned()),
            working_directory: Some(home.to_path_buf()),
            env: vec![
                ("HOME".into(), Some(home.as_os_str().to_owned())),
                ("PATH".into(), Some(path)),
                (
                    "ZZ_TMUX_SHIM_DIR".into(),
                    Some(tmux_directory.as_os_str().to_owned()),
                ),
                (
                    "HISTFILE".into(),
                    Some(home.join("history").into_os_string()),
                ),
                ("ZZ_ZSH_ZDOTDIR".into(), Some(home.as_os_str().to_owned())),
                ("PROMPT_COMMAND".into(), None),
                ("PS1".into(), None),
                ("PS0".into(), None),
            ],
            ..TerminalSpawn::default()
        },
    );
    attach_streaming(&session, TerminalViewId(145));
    wait_for_test_capture(&session, |capture| capture.contains("zz-path-fixture>"));
    let command = "printf '%s\\n' \"${PATH%%:*}\"";
    session.send_text(format!("{command}\n").as_str());
    wait_for_test_capture(&session, |capture| {
        capture.matches("zz-path-fixture>").count() == 2
    });
    let capture = wait_for_last_command(&session, |capture| capture.command == command);
    assert_eq!(capture.output, tmux_directory.to_string_lossy());
}

#[cfg(unix)]
#[test]
fn bash_shell_integration_restores_tmux_directory_after_startup() {
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &["/opt/homebrew/bin/bash", "/usr/local/bin/bash"]
    } else {
        &["/bin/bash", "/usr/bin/bash"]
    };
    let Some(shell) = candidates
        .iter()
        .find(|shell| std::path::Path::new(shell).is_file())
    else {
        eprintln!("skipping bash shell integration fixture: no supported bash installed");
        return;
    };
    shell_integration_restores_tmux_directory_after_startup(shell);
}

#[cfg(unix)]
#[test]
fn zsh_shell_integration_restores_tmux_directory_after_startup() {
    shell_integration_restores_tmux_directory_after_startup("/bin/zsh");
}

#[cfg(unix)]
#[test]
fn zsh_shell_integration_redraws_one_prompt_across_resizes() {
    let shell = "/bin/zsh";
    if !std::path::Path::new(shell).is_file() {
        eprintln!("skipping shell integration fixture: {shell} is not installed");
        return;
    }
    let temporary = tempfile::tempdir().expect("shell integration fixture home");
    let home = temporary.path();
    std::fs::write(
        home.join(".zlogin"),
        "PS1='zz-resize> '\nRPROMPT='zz-right'\n",
    )
    .expect("isolated shell startup");
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance::default()),
        TerminalSpawn {
            shell: Some(shell.to_owned()),
            working_directory: Some(home.to_path_buf()),
            initial_size: Some(TerminalSize::cells(80, 24)),
            env: vec![
                ("HOME".into(), Some(home.as_os_str().to_owned())),
                (
                    "HISTFILE".into(),
                    Some(home.join("history").into_os_string()),
                ),
                ("ZZ_ZSH_ZDOTDIR".into(), Some(home.as_os_str().to_owned())),
                ("PS1".into(), None),
                ("RPROMPT".into(), None),
            ],
            ..TerminalSpawn::default()
        },
    );
    attach_streaming(&session, TerminalViewId(146));
    wait_for_test_capture(&session, |capture| capture.contains("zz-right"));
    let generation = session.facts().output.output_generation;
    session.resize(80, 31, 8, 18);
    let deadline = Instant::now() + Duration::from_secs(5);
    while session.facts().output.output_generation == generation {
        assert!(Instant::now() < deadline, "zsh never redrew at 80x31");
        thread::sleep(Duration::from_millis(10));
    }
    session.resize(170, 45, 8, 18);
    wait_for_test_capture(&session, |capture| {
        capture.lines().any(|line| {
            line.starts_with("zz-resize>") && line.ends_with("zz-right") && line.len() > 160
        })
    });
    let capture = session
        .capture(CaptureOptions {
            start: CaptureBoundary::HistoryStart,
            ..CaptureOptions::default()
        })
        .expect("capture with history");
    let (_, history, _, _, _) = session.history(0, u32::MAX).expect("history");
    assert_eq!(capture.matches("zz-resize>").count(), 1, "{capture:?}");
    assert_eq!(history.len(), 0, "{capture:?}");
}

#[cfg(unix)]
#[test]
fn shell_integration_updates_title_and_working_directory() {
    let shell = std::env::var("SHELL").unwrap_or_default();
    let shell_name = std::path::Path::new(&shell)
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .map(ToOwned::to_owned);
    if !matches!(shell_name.as_deref(), Some("bash" | "zsh"))
        || cfg!(target_os = "macos") && shell == "/bin/bash"
    {
        return;
    }

    let temporary = tempfile::tempdir().expect("temporary working directory");
    let directory = temporary.path().join("title fixture");
    std::fs::create_dir(&directory).expect("working directory with spaces");
    let expected_directory = directory
        .canonicalize()
        .expect("canonical working directory");
    let home = tempfile::tempdir().expect("temporary home");
    let session = TerminalSession::spawn(
        DEFAULT_HISTORY_LIMIT,
        Arc::new(TerminalAppearance {
            cursor_style: CursorStyle::Underline,
            ..TerminalAppearance::default()
        }),
        TerminalSpawn {
            working_directory: Some(expected_directory.clone()),
            terminal_type: (shell_name.as_deref() == Some("bash")).then(|| "dumb".to_owned()),
            env: vec![
                ("HOME".into(), Some(home.path().as_os_str().to_owned())),
                ("ZDOTDIR".into(), None),
            ],
            ..TerminalSpawn::default()
        },
    );
    attach_streaming(&session, TerminalViewId(44));

    let expected_directory = expected_directory.to_string_lossy();
    wait_for_test_viewport(&session, |viewport| {
        viewport.title() == shell_name.as_deref().unwrap_or_default()
            && viewport
                .cursor
                .is_some_and(|cursor| cursor.style() == CursorStyle::Underline)
            && viewport
                .working_directory()
                .is_some_and(|working_directory| {
                    working_directory.ends_with(expected_directory.as_ref())
                })
    });
    session.send_text("sleep 1\n");
    wait_for_test_viewport(&session, |viewport| {
        viewport.title() == "sleep 1"
            && viewport
                .cursor
                .is_some_and(|cursor| cursor.style() == CursorStyle::Underline)
            && viewport
                .working_directory()
                .is_some_and(|working_directory| {
                    working_directory.ends_with(expected_directory.as_ref())
                })
    });
    let viewport = wait_for_test_viewport(&session, |viewport| {
        viewport.title() == shell_name.as_deref().unwrap_or_default()
            && viewport
                .cursor
                .is_some_and(|cursor| cursor.style() == CursorStyle::Underline)
            && viewport
                .working_directory()
                .is_some_and(|working_directory| {
                    working_directory.ends_with(expected_directory.as_ref())
                })
    });
    assert!(matches!(viewport.status, SessionStatus::Running));
}

#[cfg(unix)]
#[test]
fn shell_integration_idle_title_yields_to_a_title_the_prompt_sets() {
    for (name, startup, hook) in [
        (
            "bash",
            ".bash_profile",
            "PS1='yield-ready> '\nPROMPT_COMMAND+=('printf \"\\033]0;%s\\007\" prompt-title')\n",
        ),
        (
            "zsh",
            ".zshrc",
            "PS1='yield-ready> '\nprompt_title() { print -n '\\e]2;prompt-title\\a'; }\nprecmd_functions+=(prompt_title)\n",
        ),
    ] {
        let Some(shell) = ["/usr/bin", "/bin"]
            .iter()
            .map(|directory| std::path::Path::new(directory).join(name))
            .find(|path| path.is_file())
        else {
            continue;
        };
        if cfg!(target_os = "macos") && shell == std::path::Path::new("/bin/bash") {
            continue;
        }
        let home = tempfile::tempdir().expect("temporary home");
        std::fs::write(home.path().join(startup), hook).expect("startup file");
        let session = TerminalSession::spawn(
            DEFAULT_HISTORY_LIMIT,
            Arc::new(TerminalAppearance::default()),
            TerminalSpawn {
                shell: Some(shell.to_string_lossy().into_owned()),
                working_directory: Some(home.path().to_path_buf()),
                env: vec![
                    ("HOME".into(), Some(home.path().as_os_str().to_owned())),
                    (
                        "ZZ_ZSH_ZDOTDIR".into(),
                        Some(home.path().as_os_str().to_owned()),
                    ),
                    ("PROMPT_COMMAND".into(), None),
                    ("PS1".into(), None),
                    ("PS0".into(), None),
                ],
                ..TerminalSpawn::default()
            },
        );
        attach_streaming(&session, TerminalViewId(45));
        wait_for_test_capture(&session, |capture| capture.contains("yield-ready>"));
        wait_for_test_viewport(&session, |viewport| viewport.title() == "prompt-title");
        session.send_text("sleep 1\n");
        wait_for_test_viewport(&session, |viewport| viewport.title() == "sleep 1");
        wait_for_test_viewport(&session, |viewport| viewport.title() == "prompt-title");
    }
}
