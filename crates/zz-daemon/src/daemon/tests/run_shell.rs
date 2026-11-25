use super::*;

#[cfg(unix)]
#[test]
fn foreground_run_shell_captures_output_status_and_numeric_arguments() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(7);
    let mut context = ExecutionContext::default();

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("run-shell", ["printf hi"]),
        )
        .expect("successful shell job");
    assert_eq!(output.output, "hi");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("run", ["printf '%s' '#{1}'", "numeric-value"]),
        )
        .expect("numeric format argument");
    assert_eq!(output.output, "numeric-value");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("run-shell", ["-s", "ignored", "printf hidden-option"]),
        )
        .expect("hidden option remains accepted");
    assert_eq!(output.output, "hidden-option");

    let started = Instant::now();
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("run-shell", ["-d", ".03"]),
        )
        .expect("delay without a command");
    assert!(output.output.is_empty());
    assert!(started.elapsed() >= Duration::from_millis(20));

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("run-shell", ["-d", "1abc"]),
        )
        .expect_err("invalid delay");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "invalid delay time: 1abc"
    ));

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("run-shell", ["-E", "printf out; printf err >&2"]),
        )
        .expect("stderr capture");
    assert_eq!(output.output, "outerr");

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("run-shell", ["exit 3"]),
        )
        .expect_err("nonzero shell status");
    assert!(matches!(
        error,
        DaemonError::CommandExit { output, exit_code: 3 }
            if output == "'exit 3' returned 3"
    ));

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("run-shell", ["kill -TERM $$"]),
        )
        .expect_err("shell signal status");
    assert!(matches!(
        error,
        DaemonError::CommandExit { output, exit_code: 143 }
            if output == "'kill -TERM $$' terminated by signal 15"
    ));

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("run-shell", ["-c", "/tmp", "pwd"]),
        )
        .expect("literal working directory");
    assert_eq!(output.output, "/tmp\n");

    let fallback = fallback_job_working_directory();
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                ["-c", "/zz-run-shell-path-that-does-not-exist", "pwd"],
            ),
        )
        .expect("invalid working directory falls back");
    assert_eq!(output.output, format!("{}\n", fallback.to_string_lossy()));

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "default-shell", "/usr/bin/false"]),
        )
        .expect("configure an unusable session shell");
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("run-shell", ["printf bourne-shell"]),
        )
        .expect("run-shell always uses the Bourne shell");
    assert_eq!(output.output, "bourne-shell");
}

#[cfg(unix)]
#[test]
fn shell_formats_use_the_selected_current_client() {
    let shared = Arc::new(Shared::new(1));
    let mut event_loop = event_loop::EventLoop::empty(&shared).unwrap();
    let (first_session, first_window, first_pane, second_session, second_pane) = {
        let mut inner = shared.inner.lock();
        let (first_session, first_window, first_pane) = inner
            .engine
            .state
            .create_session("shell-first")
            .expect("first session");
        let (second_session, _, second_pane) = inner
            .engine
            .state
            .create_session("shell-second")
            .expect("second session");
        (
            first_session,
            first_window,
            first_pane,
            second_session,
            second_pane,
        )
    };
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("shell-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (command_client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    shared
        .attach(interactive, first_session)
        .expect("attach current client");
    shared
        .inner
        .lock()
        .client_entry(command_client)
        .origin
        .replace(first_pane);
    let mut context =
        ExecutionContext::new(Some(first_session), Some(first_window), Some(first_pane));

    let output = loop_reviewfixes_tests::execute_and_drive_loop(
        &shared,
        &mut event_loop,
        command_client,
        ClientKind::Command,
        &mut context,
        &CommandInvocation::new("run-shell", ["printf '#{session_active}'"]),
    )
    .expect("current-session run-shell format");
    assert_eq!(output.output, "1");

    let directory = tempfile::tempdir().expect("shell format directory");
    let other_output = directory.path().join("other-session-active");
    let command = format!(
        "printf '#{{session_active}}' > {}",
        shell_quote(&other_output)
    );
    loop_reviewfixes_tests::execute_and_drive_loop(
        &shared,
        &mut event_loop,
        command_client,
        ClientKind::Command,
        &mut context,
        &CommandInvocation::new(
            "run-shell",
            ["-t", &second_pane.to_string(), command.as_str()],
        ),
    )
    .expect("other-session run-shell format");
    assert_eq!(fs::read_to_string(other_output).unwrap(), "0");

    let output = loop_reviewfixes_tests::execute_and_drive_loop(
        &shared,
        &mut event_loop,
        command_client,
        ClientKind::Command,
        &mut context,
        &CommandInvocation::new(
            "if-shell",
            [
                "-F",
                "#{session_active}",
                "display-message -p active",
                "display-message -p inactive",
            ],
        ),
    )
    .expect("current-session if-shell format");
    assert_eq!(output.output, "active");

    let output = loop_reviewfixes_tests::execute_and_drive_loop(
        &shared,
        &mut event_loop,
        command_client,
        ClientKind::Command,
        &mut context,
        &CommandInvocation::new(
            "if-shell",
            [
                "-F",
                "-t",
                &second_pane.to_string(),
                "#{session_active}",
                "display-message -p active",
                "display-message -p inactive",
            ],
        ),
    )
    .expect("other-session if-shell format");
    assert_eq!(output.output, "inactive");

    let (second_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("shell-second-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .attach(second_client, second_session)
        .expect("attach newer second client");
    context.retarget(&ExecutionContext::new(
        Some(first_session),
        Some(first_window),
        Some(first_pane),
    ));
    let source_output = directory.path().join("sourced-session-active");
    let source = directory.path().join("session-active.conf");
    fs::write(
        &source,
        format!(
            "run-shell \"printf '#{{session_name}}|#{{session_active}}|#{{client_session}}' > {}\"\n\
                 display-message -p sourced-trigger\n\
                 run-shell -bC {{ new-session -d -s 'sourced-run-#{{session_active}}' }}\n\
                 if-shell -b true {{ new-session -d -s 'sourced-if-#{{session_active}}' }}\n",
            shell_quote(&source_output)
        ),
    )
    .expect("write sourced format command");
    loop_reviewfixes_tests::execute_and_drive_loop(
        &shared,
        &mut event_loop,
        interactive,
        ClientKind::Control,
        &mut context,
        &CommandInvocation::new(
            "set-hook",
            [
                "-g",
                "after-display-message",
                "new-session -d -s 'sourced-hook-#{session_active}'",
            ],
        ),
    )
    .expect("install sourced client hook");
    loop_reviewfixes_tests::execute_and_drive_loop(
        &shared,
        &mut event_loop,
        interactive,
        ClientKind::Control,
        &mut context,
        &CommandInvocation::new("source-file", [source.display().to_string()]),
    )
    .expect("source format command");
    assert_eq!(
        fs::read_to_string(source_output).unwrap(),
        "shell-first|1|shell-first"
    );
    loop_reviewfixes_tests::drive_loop_until(&shared, &mut event_loop, || {
        shared
            .inner
            .lock()
            .engine
            .state
            .sessions
            .values()
            .any(|session| session.name == "sourced-hook-1")
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let inner = shared.inner.lock();
        let run_created = inner
            .engine
            .state
            .sessions
            .values()
            .any(|session| session.name == "sourced-run-1");
        let if_created = inner
            .engine
            .state
            .sessions
            .values()
            .any(|session| session.name == "sourced-if-1");
        drop(inner);
        if run_created && if_created {
            break;
        }
        assert!(Instant::now() < deadline, "background format sessions");
        event_loop.shell_test_turn(&shared);
    }

    let hook_output = directory.path().join("hook-session-active");
    let hook_command = format!(
        "run-shell \"printf '#{{session_active}}' > {}\"",
        shell_quote(&hook_output)
    );
    loop_reviewfixes_tests::execute_and_drive_loop(
        &shared,
        &mut event_loop,
        interactive,
        ClientKind::Control,
        &mut context,
        &CommandInvocation::new(
            "set-hook",
            ["-g", "after-display-message", hook_command.as_str()],
        ),
    )
    .expect("install client format hook");
    loop_reviewfixes_tests::execute_and_drive_loop(
        &shared,
        &mut event_loop,
        interactive,
        ClientKind::Control,
        &mut context,
        &CommandInvocation::new("display-message", ["-p", "trigger"]),
    )
    .expect("trigger client format hook");
    loop_reviewfixes_tests::drive_loop_until(&shared, &mut event_loop, || {
        fs::read_to_string(&hook_output).is_ok_and(|output| output == "1")
    });

    loop_reviewfixes_tests::execute_and_drive_loop(
        &shared,
        &mut event_loop,
        interactive,
        ClientKind::Control,
        &mut context,
        &CommandInvocation::new(
            "set-hook",
            [
                "-g",
                "session-created",
                "new-session -d -s 'event-#{session_active}'",
            ],
        ),
    )
    .expect("install clientless event hook");
    loop_reviewfixes_tests::execute_and_drive_loop(
        &shared,
        &mut event_loop,
        interactive,
        ClientKind::Control,
        &mut context,
        &CommandInvocation::new("new-session", ["-d", "-s", "event-trigger"]),
    )
    .expect("trigger clientless event hook");
    loop_reviewfixes_tests::drive_loop_until(&shared, &mut event_loop, || {
        shared
            .inner
            .lock()
            .engine
            .state
            .sessions
            .values()
            .any(|session| session.name == "event-")
    });
    let inner = shared.inner.lock();
    assert!(
        inner
            .engine
            .state
            .sessions
            .values()
            .any(|session| session.name == "event-")
    );
    assert!(
        !inner
            .engine
            .state
            .sessions
            .values()
            .any(|session| session.name == "event-0")
    );
    drop(inner);

    shared.detach(second_client);
    shared.detach(interactive);
    let output = loop_reviewfixes_tests::execute_and_drive_loop(
        &shared,
        &mut event_loop,
        command_client,
        ClientKind::Command,
        &mut context,
        &CommandInvocation::new("run-shell", ["printf '[#{session_active}]'"]),
    )
    .expect("clientless run-shell format");
    assert_eq!(output.output, "[]");
}

#[cfg(unix)]
#[test]
fn direct_shell_formats_resolve_target_options_before_variables() {
    let shared = Arc::new(Shared::new(1));
    let (session, window, pane, target_pane) = {
        let mut inner = shared.inner.lock();
        let (session, window, pane) = inner
            .engine
            .state
            .create_session("direct-option-formats")
            .expect("format session");
        let (_, target_pane) = inner
            .engine
            .state
            .create_window_at(
                session,
                None,
                Some("local".to_owned()),
                PaneKind::Terminal,
                false,
            )
            .expect("target window");
        (session, window, pane, target_pane)
    };
    let client = ClientId(8);
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    context
        .format_variables
        .insert("automatic-rename".to_owned(), "WRONG".to_owned());
    let target = target_pane.to_string();
    let missing = "=missing";
    for args in [
        vec!["-g", "automatic-rename", "off"],
        vec!["-t", target.as_str(), "automatic-rename", "on"],
    ] {
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("set-window-option", args),
            )
            .expect("set format option");
    }

    let directory = tempfile::tempdir().expect("format directory");
    for (target, expected, name, format) in [
        (
            target.as_str(),
            "1|[]",
            "local-shell",
            "#{automatic-rename}|[#{automatic-ren}]",
        ),
        (missing, "0", "missing-shell", "#{automatic-rename}"),
    ] {
        let path = directory.path().join(name);
        let command = format!("printf '{format}' > {}", shell_quote(&path));
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("run-shell", ["-t", target, command.as_str()]),
            )
            .expect("shell option format");
        assert_eq!(fs::read_to_string(path).unwrap(), expected);
    }

    for (target, name) in [
        (target.as_str(), "@direct-run-local"),
        (missing, "@direct-run-missing"),
    ] {
        let command = format!("set-option -g {name} '#{{automatic-rename}}'");
        shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("run-shell", ["-C", "-t", target, command.as_str()]),
            )
            .expect("command-mode format");
    }
    for (name, expected) in [("@direct-run-local", "1"), ("@direct-run-missing", "0")] {
        let output = shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-options", ["-gqv", name]),
            )
            .expect("show command-mode format");
        assert_eq!(output.output, expected);
    }

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                [
                    "-C",
                    "-t",
                    missing,
                    "set-option @direct-run-context '#{automatic-rename}'",
                ],
            ),
        )
        .expect("missing-target command execution context");
    let local = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-options", ["-qv", "@direct-run-context"]),
        )
        .expect("show local command execution result");
    let global = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-options", ["-gqv", "@direct-run-context"]),
        )
        .expect("show global command execution result");
    assert_eq!(local.output, "0");
    assert_eq!(global.output, "");

    for (target, expected) in [(target.as_str(), "local"), (missing, "global")] {
        let output = shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "if-shell",
                    [
                        "-F",
                        "-t",
                        target,
                        "#{automatic-rename}",
                        "display-message -p local",
                        "display-message -p global",
                    ],
                ),
            )
            .expect("if-shell option format");
        assert_eq!(output.output, expected);
    }

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if-shell",
                [
                    "-F",
                    "-t",
                    missing,
                    "#{automatic-rename}",
                    "set-option @if-shell-context wrong",
                    "set-option @if-shell-context global-value",
                ],
            ),
        )
        .expect("missing-target branch execution context");
    let local = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-options", ["-qv", "@if-shell-context"]),
        )
        .expect("show local branch execution result");
    let global = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-options", ["-gqv", "@if-shell-context"]),
        )
        .expect("show global branch execution result");
    assert_eq!(local.output, "global-value");
    assert_eq!(global.output, "");
}

#[cfg(unix)]
#[test]
fn run_shell_command_mode_and_if_shell_execute_inserted_commands() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(8);
    let mut context = ExecutionContext::default();

    for spelling in ["run-shell", "run", "run-s"] {
        let output = shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(spelling, ["printf '#{command}'"]),
            )
            .unwrap_or_else(|error| panic!("{spelling}: {error}"));
        assert_eq!(output.output, "run-shell");
    }
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-option", ["-s", "command-alias[31]", "zzrun=run-shell"]),
        )
        .expect("install daemon command alias");
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("zzrun", ["printf '#{command}'"]),
        )
        .expect("user alias command item");
    assert_eq!(output.output, "run-shell");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("run-shell", ["-C", "display-message -p '#{command}'"]),
        )
        .expect("run-shell command insertion");
    assert_eq!(output.output, "run-shell");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("run-shell", ["-C", "{ display-message -p '#{command}' }"])
                .with_command_blocks([1]),
        )
        .expect("run-shell typed block child");
    assert_eq!(output.output, "display-message");

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                ["{ display-message -p forbidden-shell-block }"],
            )
            .with_command_blocks([0]),
        )
        .expect_err("typed shell command without -C");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::CommandParse(message))
            if message == "command run-shell: argument 1 must be \"string\""
    ));

    let error = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                ["-C", "{ display-message -p quoted-command-text }"],
            ),
        )
        .expect_err("quoted brace text stays a command string");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::PostAdmissionCallback(error))
            if matches!(&*error, ServerError::CommandParse(message) if message == "syntax error")
    ));

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                [
                    "-C",
                    "display-message -p first-command",
                    "{ display-message -p ignored-command }",
                ],
            )
            .with_command_blocks([2]),
        )
        .expect("typed extra command-mode argument");
    assert_eq!(output.output, "first-command");

    for (name, command) in [
        (
            "RUN_GROUP",
            CommandInvocation::new(
                "run-shell",
                [
                    "-C",
                    "{ kill-session -t missing ; set-environment -g RUN_GROUP_SAME yes\nset-environment -g RUN_GROUP_NEXT yes }",
                ],
            )
            .with_command_blocks([1]),
        ),
        (
            "IF_GROUP",
            CommandInvocation::new(
                "if-shell",
                [
                    "-F",
                    "1",
                    "{ kill-session -t missing ; set-environment -g IF_GROUP_SAME yes\nset-environment -g IF_GROUP_NEXT yes }",
                ],
            )
            .with_command_blocks([2]),
        ),
    ] {
        let _error = shared
            .execute(client, ClientKind::Command, &mut context, &command)
            .expect_err("typed callback group fails");
        let inner = shared.inner.lock();
        assert_eq!(
            inner
                .engine
                .global_environment_variable(&format!("{name}_SAME")),
            None
        );
        assert_eq!(
            inner
                .engine
                .global_environment_variable(&format!("{name}_NEXT")),
            Some("yes".to_owned())
        );
    }

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                [
                    "-C",
                    "kill-session -t missing ; set-environment -g STRING_GROUP_SAME yes\nset-environment -g STRING_GROUP_NEXT yes",
                ],
            ),
        )
        .expect_err("string callback is one failure group");
    let inner = shared.inner.lock();
    assert_eq!(
        inner
            .engine
            .global_environment_variable("STRING_GROUP_SAME"),
        None
    );
    assert_eq!(
        inner
            .engine
            .global_environment_variable("STRING_GROUP_NEXT"),
        None
    );
    drop(inner);

    context
        .format_variables
        .insert("command".to_owned(), "explicit".to_owned());
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("run-shell", ["printf '#{command}'"]),
        )
        .expect("explicit command format state");
    assert_eq!(output.output, "explicit");
    context.format_variables.remove("command");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                ["-C", "display-message -p 'x#{1}'", "not-numeric"],
            ),
        )
        .expect("command insertion without numeric variables");
    assert_eq!(output.output, "x");

    for (condition, expected) in [("true", "yes"), ("false", "no")] {
        let output = shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "if-shell",
                    [condition, "display-message -p yes", "display-message -p no"],
                ),
            )
            .expect("shell branch");
        assert_eq!(output.output, expected);
    }

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("if-shell", ["false", "display-message -p unreachable"]),
        )
        .expect("missing false branch");
    assert!(output.output.is_empty());

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "format-truth"]),
        )
        .expect("format session");

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                [
                    "-C",
                    "set-option -g @expanded-command '#{session_name}:#{1}'",
                    "not-numeric",
                ],
            ),
        )
        .expect("command string format expansion");
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-options", ["-gqv", "@expanded-command"]),
        )
        .expect("expanded command value");
    assert_eq!(output.output, "format-truth:");

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                ["-C", "{ set-option -g @raw-block '#{session_name}' }"],
            )
            .with_command_blocks([1]),
        )
        .expect("unexpanded command block");
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-options", ["-gqv", "@raw-block"]),
        )
        .expect("raw block value");
    assert_eq!(output.output, "#{session_name}");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if",
                [
                    "-F",
                    "#{session_name}",
                    "display-message -p truthy",
                    "display-message -p falsy",
                ],
            ),
        )
        .expect("truthy format branch");
    assert_eq!(output.output, "truthy");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if-shell",
                [
                    "test '#{session_name}' = format-truth",
                    "display-message -p expanded-condition",
                    "display-message -p missed-condition",
                ],
            ),
        )
        .expect("non-format condition expansion");
    assert_eq!(output.output, "expanded-condition");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if-shell",
                [
                    "test '#{command}' = if-shell",
                    "display-message -p canonical-condition",
                    "display-message -p missed-command",
                ],
            ),
        )
        .expect("command item condition expansion");
    assert_eq!(output.output, "canonical-condition");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("if-shell", ["-F", "1", "display-message -p '#{command}'"]),
        )
        .expect("if-shell child command item");
    assert_eq!(output.output, "display-message");

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if-shell",
                ["-F", "1", "set-option -g @raw-branch '#{session_name}'"],
            ),
        )
        .expect("unexpanded branch command");
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-options", ["-gqv", "@raw-branch"]),
        )
        .expect("raw branch value");
    assert_eq!(output.output, "#{session_name}");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if-shell",
                [
                    "-bF",
                    "#{missing_value}",
                    "display-message -p truthy",
                    "display-message -p falsy",
                ],
            ),
        )
        .expect("falsy format branch");
    assert_eq!(output.output, "falsy");

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if-shell",
                [
                    "-F",
                    "1",
                    "{ display-message -p first ; display-message -p second }",
                ],
            )
            .with_command_blocks([2]),
        )
        .expect("brace command block");
    assert_eq!(output.output, "first\nsecond");

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if-shell",
                [
                    "-F",
                    "1",
                    "not-a-command ; set-option -g @after-error reached",
                ],
            ),
        )
        .expect_err("first inserted command aborts the branch");
    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("show-options", ["-gqv", "@after-error"]),
        )
        .expect("query skipped branch mutation");
    assert!(output.output.is_empty());
}

#[cfg(unix)]
#[test]
fn run_shell_routes_to_command_interactive_control_and_background_sinks() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (interactive, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "shell-routes"]),
        )
        .expect("routing session");
    let target = context.pane.expect("target pane");
    let session = context.session.expect("target session");
    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    shared
        .attach(control, session)
        .expect("attach control client");
    let peer_mailbox = OutboundMailbox::new();
    let (peer, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&peer_mailbox));
    shared
        .attach(peer, session)
        .expect("attach peer control client");
    take_reliable_messages(&control_mailbox);
    take_reliable_messages(&peer_mailbox);

    let output = shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("run-shell", ["-t", &target.to_string(), "printf targeted"]),
        )
        .expect("targeted shell output");
    assert!(output.output.is_empty());
    assert_eq!(
        shared.inner.lock().clients[&interactive]
            .command_output
            .as_ref()
            .unwrap()
            .pane,
        target
    );

    let error = shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("run-shell", ["-t", &target.to_string(), "exit 4"]),
        )
        .expect_err("targeted status still propagates");
    assert!(matches!(
        error,
        DaemonError::CommandExit { output, exit_code: 4 } if output.is_empty()
    ));

    let output = shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("run-shell", ["-t", "%999999", "printf fallback"]),
        )
        .expect("missing target falls back to command output");
    assert_eq!(output.output, "fallback");

    shared
        .inner
        .lock()
        .client_mut(interactive)
        .and_then(|c| c.command_output.take());
    let output = shared
        .execute(
            control,
            ClientKind::Control,
            &mut context.clone(),
            &CommandInvocation::new("run-shell", ["-t", &target.to_string(), "printf control"]),
        )
        .expect("control shell output");
    assert!(output.output.is_empty());
    assert_eq!(
        shared.inner.lock().clients[&interactive]
            .command_output
            .as_ref()
            .unwrap()
            .pane,
        target
    );
    assert!(shared.read_client(control, |c| c.is_none_or(|c| c.command_output.is_none())));
    assert!(control_command_outputs(take_reliable_messages(&control_mailbox)).is_empty());
    assert!(control_command_outputs(take_reliable_messages(&peer_mailbox)).is_empty());

    shared
        .inner
        .lock()
        .client_mut(interactive)
        .and_then(|c| c.command_output.take());
    let output = shared
        .execute(
            control,
            ClientKind::Control,
            &mut context.clone(),
            &CommandInvocation::new("run-shell", ["printf control-raw"]),
        )
        .expect("targetless control shell output");
    assert!(output.output.is_empty());
    assert_eq!(
        control_command_outputs(take_reliable_messages(&control_mailbox)),
        ["control-raw"]
    );
    assert!(control_command_outputs(take_reliable_messages(&peer_mailbox)).is_empty());
    assert!(shared.read_client(interactive, |c| {
        c.is_none_or(|c| c.command_output.is_none())
    }));

    let output = shared
        .execute(
            control,
            ClientKind::Control,
            &mut context.clone(),
            &CommandInvocation::new(
                "run-shell",
                ["-C", "display-message -p CONTROL_COMMAND_MODE"],
            ),
        )
        .expect("control command-mode output stays synchronous");
    assert_eq!(output.output, "CONTROL_COMMAND_MODE");
    assert!(control_command_outputs(take_reliable_messages(&control_mailbox)).is_empty());

    let output = shared
        .execute(
            control,
            ClientKind::Control,
            &mut context.clone(),
            &CommandInvocation::new("run-shell", ["-t", "%999999", "printf invalid-target"]),
        )
        .expect("invalid target falls back to control output");
    assert!(output.output.is_empty());
    assert_eq!(
        control_command_outputs(take_reliable_messages(&control_mailbox)),
        ["invalid-target"]
    );
    assert!(control_command_outputs(take_reliable_messages(&peer_mailbox)).is_empty());

    let mut control_context = context.clone();
    let response = shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut control_context,
        71,
        &CommandInvocation::new("run-shell", ["exit 4"]),
    );
    assert!(
        matches!(
            response,
            CommandResponse::Success {
                request_id: 71,
                ref output,
                exit_code: 4,
                ..
            } if output.is_empty()
        ),
        "{response:?}"
    );
    assert_eq!(
        control_command_outputs(take_reliable_messages(&control_mailbox)),
        ["'exit 4' returned 4"]
    );
    assert!(control_command_outputs(take_reliable_messages(&peer_mailbox)).is_empty());

    let response = shared.execute_command_request(
        control,
        ClientKind::Control,
        &mut control_context,
        72,
        &CommandInvocation::new("run-shell", ["kill -TERM $$"]),
    );
    assert!(
        matches!(
            response,
            CommandResponse::Success {
                request_id: 72,
                ref output,
                exit_code: 143,
                ..
            } if output.is_empty()
        ),
        "{response:?}"
    );
    assert_eq!(
        control_command_outputs(take_reliable_messages(&control_mailbox)),
        ["'kill -TERM $$' terminated by signal 15"]
    );
    assert!(control_command_outputs(take_reliable_messages(&peer_mailbox)).is_empty());
    assert!(shared.read_client(control, |c| c.is_none_or(|c| c.command_output.is_none())));

    let output = shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("run-shell", ["printf interactive"]),
        )
        .expect("interactive output overlay");
    assert!(output.output.is_empty());
    assert_eq!(
        shared.inner.lock().clients[&interactive]
            .command_output
            .as_ref()
            .unwrap()
            .pane,
        target
    );

    shared
        .inner
        .lock()
        .client_mut(interactive)
        .and_then(|c| c.command_output.take());
    let output = shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("run-shell", ["-b", "sleep 0.2; printf background"]),
        )
        .expect("background shell job");
    assert!(output.output.is_empty());
    assert_eq!(shared.inner.lock().active_shell_jobs, 1);
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.read_client(interactive, |c| {
        c.is_none_or(|c| c.command_output.is_none())
    }) && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(10));
    }
    let inner = shared.inner.lock();
    assert_eq!(
        inner.clients[&interactive]
            .command_output
            .as_ref()
            .unwrap()
            .pane,
        target
    );
    drop(inner);
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.inner.lock().active_shell_jobs != 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(shared.inner.lock().active_shell_jobs, 0);

    shared
        .inner
        .lock()
        .client_mut(interactive)
        .and_then(|c| c.command_output.take());
    let output = shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("run-shell", ["-b", "sleep 0.05; printf control-background"]),
        )
        .expect("control background shell job");
    assert!(output.output.is_empty());
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.read_client(interactive, |c| {
        c.is_none_or(|c| c.command_output.is_none())
    }) && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        shared.inner.lock().clients[&interactive]
            .command_output
            .as_ref()
            .unwrap()
            .pane,
        target
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.inner.lock().active_shell_jobs != 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(shared.inner.lock().active_shell_jobs, 0);
    assert!(shared.read_client(control, |c| c.is_none_or(|c| c.command_output.is_none())));
    assert!(control_command_outputs(take_reliable_messages(&control_mailbox)).is_empty());
    assert!(control_command_outputs(take_reliable_messages(&peer_mailbox)).is_empty());

    let wait_for_error = |mailbox: &OutboundMailbox| {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(text) = take_reliable_messages(mailbox)
                .into_iter()
                .find_map(|message| match message {
                    ProtocolMessage::Event(Event {
                        payload:
                            EventPayload::ClientMessage {
                                kind: ClientMessageKind::Error,
                                text,
                                ..
                            },
                        ..
                    }) => Some(text),
                    ProtocolMessage::Event(Event {
                        payload:
                            EventPayload::TimedClientMessage {
                                kind: ClientMessageKind::Error,
                                text,
                                duration_ms: 750,
                                ..
                            },
                        ..
                    }) => Some(format!("timed {text}")),
                    _ => None,
                })
            {
                break text;
            }
            assert!(
                Instant::now() < deadline,
                "background command error was not delivered"
            );
            thread::sleep(Duration::from_millis(10));
        }
    };

    take_reliable_messages(&control_mailbox);
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new("run-shell", ["-b", "-C", "not-a-command"]),
        )
        .expect("control background inserted command");
    assert_eq!(
        wait_for_error(&control_mailbox),
        "unknown command: not-a-command"
    );

    take_reliable_messages(&mailbox);
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("run-shell", ["-b", "-C", "not-a-command"]),
        )
        .expect("interactive background inserted command");
    assert_eq!(
        wait_for_error(&mailbox),
        "timed Unknown command: not-a-command"
    );
}

#[cfg(windows)]
#[test]
fn shell_job_closes_stdin_before_waiting_for_the_child() {
    let process = Arc::new(Mutex::new(None));
    let worker_process = Arc::clone(&process);
    let (finished, result) = mpsc::sync_channel(1);
    let environment = std::env::vars()
        .map(|(name, value)| {
            (
                RawText::from(name.as_str()),
                Some(RawText::from(value.as_str())),
            )
        })
        .collect::<Vec<_>>();
    let worker = thread::spawn(move || {
        let stopping = AtomicBool::new(false);
        let result = run_shell_job(
            "sort",
            &std::env::temp_dir(),
            "",
            &environment,
            "tmux-256color",
            Path::new("zz-shell-job-test"),
            None,
            None,
            None,
            false,
            &worker_process,
            &stopping,
            false,
            None,
        );
        let _ = finished.send(result);
    });
    let result = match result.recv_timeout(Duration::from_secs(5)) {
        Ok(result) => result.expect("shell job"),
        Err(error) => {
            terminate_managed_process(&process);
            worker.join().expect("shell job worker");
            panic!("shell job kept waiting for stdin to close: {error}");
        }
    };
    worker.join().expect("shell job worker");
    assert!(result.status.success());
}

#[cfg(unix)]
#[test]
fn foreground_shell_jobs_run_concurrently_and_obey_the_process_cap() {
    let shared = Arc::new(Shared::new(1));
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let workers = [(ClientId(101), "first"), (ClientId(102), "second")]
        .into_iter()
        .map(|(client, output)| {
            let shared = Arc::clone(&shared);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let mut context = ExecutionContext::default();
                barrier.wait();
                shared.execute(
                    client,
                    ClientKind::Command,
                    &mut context,
                    &CommandInvocation::new("run-shell", [format!("sleep 0.3; printf {output}")]),
                )
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.inner.lock().active_shell_jobs != 2 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(shared.inner.lock().active_shell_jobs, 2);
    let outputs = workers
        .into_iter()
        .map(|worker| {
            worker
                .join()
                .expect("foreground request thread")
                .expect("foreground shell job")
                .output
        })
        .collect::<Vec<_>>();
    assert_eq!(outputs, ["first", "second"]);
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.inner.lock().active_shell_jobs != 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(shared.inner.lock().active_shell_jobs, 0);

    shared.inner.lock().active_shell_jobs = MAX_SHELL_JOBS;
    let error = shared
        .execute(
            ClientId(103),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("run-shell", ["printf never"]),
        )
        .expect_err("job process cap");
    assert!(matches!(
        error,
        DaemonError::Server(ServerError::InvalidCommand(message))
            if message == "failed to run command: printf never"
    ));
    shared.inner.lock().active_shell_jobs = 0;
}

#[cfg(unix)]
#[test]
fn kill_server_reaps_running_shell_job_process_groups() {
    let shared = Arc::new(Shared::new(1));
    let scratch = tempfile::tempdir().expect("temporary directory");
    let group_path = scratch.path().join("shell-job-group");
    let command = format!(
        "printf '%s' \"$$\" > {}; sleep 30 & wait",
        shell_quote(&group_path)
    );
    shared
        .execute(
            ClientId(105),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("run-shell", ["-b", command.as_str()]),
        )
        .expect("background shell job");

    let deadline = Instant::now() + Duration::from_secs(2);
    let group = loop {
        if let Some(group) = fs::read_to_string(&group_path)
            .ok()
            .and_then(|group| group.parse().ok())
            .and_then(rustix::process::Pid::from_raw)
        {
            break group;
        }
        assert!(Instant::now() < deadline, "shell job never started");
        thread::sleep(Duration::from_millis(10));
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        {
            let inner = shared.inner.lock();
            assert_eq!(inner.active_shell_jobs, 1);
            assert_eq!(inner.shell_jobs.len(), 1);
            if inner
                .shell_jobs
                .values()
                .all(|process| process.lock().is_some())
            {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "shell job process handle never registered"
        );
        thread::sleep(Duration::from_millis(10));
    }

    shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("kill-server", [] as [&str; 0]),
        )
        .expect("kill server");
    #[cfg(unix)]
    lifecycle::wait_for_cleanup(&shared);
    assert!(shared.inner.lock().shell_jobs.is_empty());

    wait_for_process_group_exit(group, Duration::from_secs(2), "shell job");
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.inner.lock().active_shell_jobs != 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(shared.inner.lock().active_shell_jobs, 0);
}

#[cfg(unix)]
#[test]
fn background_command_insertion_and_if_shell_return_before_their_work_runs() {
    let shared = Arc::new(Shared::new(1));
    let client = ClientId(104);
    let mut context = ExecutionContext::default();

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "run-shell",
                ["-bC", "set-option -g @background-command '#{command}'"],
            ),
        )
        .expect("background command insertion");
    assert!(output.output.is_empty());
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let value = shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-options", ["-gqv", "@background-command"]),
            )
            .expect("background command value");
        if value.output == "run-shell" {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }

    let output = shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "if-shell",
                [
                    "-b",
                    "sleep 0.1; true",
                    "{ set-option -g @background-if true }",
                    "{ set-option -g @background-if false }",
                ],
            )
            .with_command_blocks([2, 3]),
        )
        .expect("background if-shell");
    assert!(output.output.is_empty());
    assert_eq!(shared.inner.lock().active_shell_jobs, 1);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let value = shared
            .execute(
                client,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-options", ["-gqv", "@background-if"]),
            )
            .expect("background branch value");
        if value.output == "true" {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(shared.inner.lock().active_shell_jobs, 0);
}

#[cfg(unix)]
#[test]
fn control_background_inserted_commands_emit_flags_zero_frames() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let child = directory.path().join("child.conf");
    fs::write(&child, "display-message -p BACKGROUND_CHILD\n").expect("background child source");
    let missing = directory.path().join("missing.conf");

    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "control-background-frames"]),
        )
        .expect("background frame session");
    let session = context.session.expect("background frame session id");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(control, session)
        .expect("attach background frame client");
    take_reliable_messages(&mailbox);

    for (request_id, command, expected) in [
        (
            1,
            CommandInvocation::new("run-shell", ["-bC", "display-message -p BACKGROUND_RUN"]),
            vec![("BACKGROUND_RUN".to_owned(), false, false, 0)],
        ),
        (
            2,
            CommandInvocation::new(
                "if-shell",
                [
                    "-b".to_owned(),
                    "true".to_owned(),
                    format!("source-file '{}'", child.display()),
                ],
            ),
            vec![
                (String::new(), false, false, 0),
                ("BACKGROUND_CHILD".to_owned(), false, false, 0),
            ],
        ),
        (
            3,
            CommandInvocation::new(
                "if-shell",
                [
                    "-b".to_owned(),
                    "false".to_owned(),
                    "display-message -p WRONG_BRANCH".to_owned(),
                    "display-message -p BACKGROUND_ELSE".to_owned(),
                ],
            ),
            vec![("BACKGROUND_ELSE".to_owned(), false, false, 0)],
        ),
        (
            4,
            CommandInvocation::new(
                "if-shell",
                [
                    "-b".to_owned(),
                    "true".to_owned(),
                    format!("source-file '{}'", missing.display()),
                ],
            ),
            vec![(missing_source_error(&missing), true, true, 0)],
        ),
        (
            5,
            CommandInvocation::new(
                "if-shell",
                ["-b", "true", "kill-session -t background-runtime-missing"],
            ),
            vec![(
                "can't find session: background-runtime-missing".to_owned(),
                true,
                true,
                0,
            )],
        ),
    ] {
        assert_eq!(
            shared.execute_command_request(
                control,
                ClientKind::Control,
                &mut context,
                request_id,
                &command,
            ),
            CommandResponse::Success {
                request_id,
                output: RawText::default(),
                exit_code: 0,
                stderr: String::new(),
                stdout_claim: StdoutClaim::None,
            }
        );
        assert_eq!(
            wait_for_control_command_frames(&mailbox, expected.len()),
            expected
        );
    }
}

#[cfg(unix)]
#[test]
fn control_background_callbacks_cancel_after_disconnect() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let ready = directory.path().join("ready");
    let release = directory.path().join("release");
    let finished = directory.path().join("finished");
    let condition = format!(
        "touch '{}'; while [ ! -e '{}' ]; do sleep 0.01; done; touch '{}'; true",
        ready.display(),
        release.display(),
        finished.display(),
    );

    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "control-background-cancel"]),
        )
        .expect("background cancellation session");
    let session = context.session.expect("background cancellation session id");
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared
        .attach(control, session)
        .expect("attach background cancellation client");
    take_reliable_messages(&mailbox);

    assert!(matches!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            2,
            &CommandInvocation::new(
                "run-shell",
                ["-bC", "-d", "0.5", "set-option -g @disconnected-run yes",],
            ),
        ),
        CommandResponse::Success { .. }
    ));
    assert!(matches!(
        shared.execute_command_request(
            control,
            ClientKind::Control,
            &mut context,
            3,
            &CommandInvocation::new(
                "if-shell",
                [
                    "-b".to_owned(),
                    condition,
                    "set-option -g @disconnected-if yes".to_owned(),
                ],
            ),
        ),
        CommandResponse::Success { .. }
    ));
    let ready_deadline = Instant::now() + Duration::from_secs(2);
    while !ready.exists() {
        assert!(Instant::now() < ready_deadline, "condition did not start");
        thread::sleep(Duration::from_millis(10));
    }

    shared.unregister(control);
    let replacement_mailbox = OutboundMailbox::new();
    let (replacement, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&replacement_mailbox),
    );
    shared
        .attach(replacement, session)
        .expect("attach replacement Control client");
    take_reliable_messages(&replacement_mailbox);
    fs::write(&release, b"").expect("release background condition");
    let finished_deadline = Instant::now() + Duration::from_secs(2);
    while !finished.exists() {
        assert!(
            Instant::now() < finished_deadline,
            "condition did not finish"
        );
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(600));

    for option in ["@disconnected-run", "@disconnected-if"] {
        let execution = shared
            .execute(
                command,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-options", ["-gqv", option]),
            )
            .expect("query disconnected callback side effect");
        assert!(execution.output.is_empty(), "{option} was set");
    }
    for mailbox in [&mailbox, &replacement_mailbox] {
        assert!(take_reliable_messages(mailbox).into_iter().all(|message| {
            !matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ControlCommandGuard { .. }
                        | EventPayload::ClientMessage { .. },
                    ..
                })
            )
        }));
    }
}
