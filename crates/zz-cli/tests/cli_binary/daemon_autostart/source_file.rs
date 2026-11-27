use super::*;

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
