use super::*;

#[test]
fn editor_sniff_matches_the_pin_basename_rule() {
    for editor in ["vi", "vim", "nvim", "gvim", "/usr/bin/evil", "nano-vi-x"] {
        assert_eq!(
            editor_derived_keys(None, Some(OsStr::new(editor))),
            Some("vi"),
            "{editor}"
        );
    }
    for editor in ["emacsclient", "VI", "/usr/bin/emacs", "/opt/vi/bin/ed"] {
        assert_eq!(
            editor_derived_keys(None, Some(OsStr::new(editor))),
            Some("emacs"),
            "{editor}"
        );
    }
    assert_eq!(editor_derived_keys(None, None), None);
    assert_eq!(
        editor_derived_keys(Some(OsStr::new("")), Some(OsStr::new("vim"))),
        Some("emacs")
    );
    assert_eq!(
        editor_derived_keys(Some(OsStr::new("vim")), Some(OsStr::new("emacs"))),
        Some("vi")
    );
}

#[test]
fn editor_derived_keys_reach_status_keys_as_well_as_mode_keys() {
    let boot = |keys: Option<&str>| {
        let shared = Shared::configured_with_boot_environment(
            1,
            Arc::new(TerminalAppearance::default()),
            AppearanceProvenance::default(),
            false,
            std::env::temp_dir().join("zz-test-paste"),
            std::env::temp_dir().join("zz-test.sock"),
            keys,
            "/usr/bin/vi",
            std::iter::empty::<(String, String)>(),
        );
        let mut inner = shared.inner.lock();
        let read = |inner: &mut ServerState, args: [&str; 2]| {
            inner
                .engine
                .execute(
                    &mut ExecutionContext::default(),
                    &CommandInvocation::new("show-options", args),
                )
                .expect("read keys option")
                .output
        };
        (
            read(&mut inner, ["-gv", "status-keys"]),
            read(&mut inner, ["-gwv", "mode-keys"]),
        )
    };

    assert_eq!(boot(None), ("emacs".into(), "emacs".into()));
    assert_eq!(boot(Some("vi")), ("vi".into(), "vi".into()));
    assert_eq!(boot(Some("emacs")), ("emacs".into(), "emacs".into()));
}

#[test]
fn editor_option_uses_daemon_boot_environment_precedence() {
    assert_eq!(
        editor_from_environment(Some(OsStr::new("nvim")), Some(OsStr::new("vim"))),
        "nvim"
    );
    assert_eq!(
        editor_from_environment(None, Some(OsStr::new("emacs"))),
        "emacs"
    );
    assert_eq!(
        editor_from_environment(Some(OsStr::new("")), Some(OsStr::new("vim"))),
        ""
    );
    assert_eq!(editor_from_environment(None, None), "/usr/bin/vi");
}

#[test]
fn configured_engine_receives_boot_defaults_before_initialization() {
    let shared = Shared::configured_with_boot_environment(
        1,
        Arc::new(TerminalAppearance::default()),
        AppearanceProvenance::default(),
        false,
        std::env::temp_dir().join("zz-test-paste"),
        std::env::temp_dir().join("zz-test.sock"),
        Some("vi"),
        "nvim",
        [("PHASE4D_BOOT", "seeded")],
    );
    let mut inner = shared.inner.lock();
    assert_eq!(inner.engine.mux_option_value(MuxOptionKey::ModeKeys), "vi");
    assert_eq!(
        inner
            .engine
            .execute(
                &mut ExecutionContext::default(),
                &CommandInvocation::new("show-options", ["-sv", "editor"]),
            )
            .unwrap()
            .output,
        "nvim"
    );
    assert_eq!(
        inner
            .engine
            .execute(
                &mut ExecutionContext::default(),
                &CommandInvocation::new("show-environment", ["-g", "PHASE4D_BOOT"],),
            )
            .unwrap()
            .output,
        "PHASE4D_BOOT=seeded"
    );
}

#[test]
fn tmux_environment_uses_socket_pid_and_numeric_session() {
    let socket = Path::new("/tmp/zz-tmux-environment.sock");
    assert_eq!(
        tmux_environment(socket, Some(SessionId(42))),
        format!("/tmp/zz-tmux-environment.sock,{},42", std::process::id())
    );
    assert_eq!(
        tmux_environment(socket, None),
        format!("/tmp/zz-tmux-environment.sock,{},-1", std::process::id())
    );
}

#[test]
fn explicit_boot_configs_replace_default_discovery_and_load_in_order() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let first = directory.path().join("first.conf");
    let second = directory.path().join("second.conf");
    let sourced = directory.path().join("sourced.conf");
    let mux = directory.path().join("mux.conf");
    fs::write(&first, "set -g prefix C-a\n").expect("first config");
    fs::write(&second, "set -g prefix C-x\n").expect("second config");
    fs::write(&sourced, "set -g prefix C-z\n").expect("sourced config");
    fs::write(&mux, "").expect("isolated mux config");
    let configs = [first, second];
    let shared = Arc::new(Shared::new(1));
    *shared.zz_mux_config_path.lock() = Some(mux.clone());
    shared
        .initialize_with_mux_config_files(true, Some(&configs), None)
        .expect("initialize explicit configs");
    let output = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("show-options", ["-gqv", "prefix"]),
        )
        .expect("show boot option")
        .output;
    assert_eq!(output, "C-x");

    let expected = format_config_files(&configs);
    let config_files = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("display-message", ["-p", "#{config_files}"]),
        )
        .expect("show startup config files")
        .output;
    assert_eq!(config_files, expected);
    {
        let inner = shared.inner.lock();
        let snapshot = inner.engine.state.snapshot();
        let option_snapshot = Arc::new(inner.engine.format_option_snapshot());
        let request = status_request(
            &inner,
            ClientId(7),
            &snapshot,
            option_snapshot,
            FormatHookFacts::default(),
            true,
            FormatNeeds::NONE,
        );
        assert_eq!(request.context.config_files, expected);
    }

    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("source-file", [sourced.display().to_string()]),
        )
        .expect("source additional config");
    let config_files = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("display-message", ["-p", "#{config_files}"]),
        )
        .expect("show retained startup config files")
        .output;
    assert_eq!(config_files, expected);
}

#[test]
fn startup_sources_use_the_initial_client_cwd_until_replay_finishes() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let initial_cwd = directory.path().join("client cwd [literal] with spaces");
    let config_directory = directory.path().join("config directory");
    let runtime_cwd = directory.path().join("runtime cwd");
    fs::create_dir_all(initial_cwd.join("a")).expect("initial source directory");
    fs::create_dir_all(config_directory.join("a")).expect("config source directory");
    fs::create_dir_all(&runtime_cwd).expect("runtime source directory");

    fs::write(
        initial_cwd.join("a/entry.conf"),
        "set-option -g @startup-source-direct initial\nsource-file leaf.conf\n",
    )
    .expect("initial direct source");
    fs::write(
        config_directory.join("a/entry.conf"),
        "set-option -g @startup-source-direct config-decoy\n",
    )
    .expect("config direct decoy");
    fs::write(
        initial_cwd.join("leaf.conf"),
        "set-option -g @startup-source-nested initial\n",
    )
    .expect("initial nested source");
    fs::write(
        initial_cwd.join("a/leaf.conf"),
        "set-option -g @startup-source-nested containing-decoy\n",
    )
    .expect("containing nested decoy");
    fs::write(
        config_directory.join("leaf.conf"),
        "set-option -g @startup-source-nested config-decoy\n",
    )
    .expect("config nested decoy");
    fs::write(
        initial_cwd.join("if.conf"),
        "set-option -g @startup-source-if initial\n",
    )
    .expect("initial if-shell source");
    fs::write(
        config_directory.join("if.conf"),
        "set-option -g @startup-source-if config-decoy\n",
    )
    .expect("config if-shell decoy");
    fs::write(
        initial_cwd.join("run.conf"),
        "set-option -g @startup-source-run initial\nsource-file run-leaf.conf\n",
    )
    .expect("initial run-shell source");
    fs::write(
        initial_cwd.join("run-leaf.conf"),
        "set-option -g @startup-source-run-nested initial\n",
    )
    .expect("initial run-shell nested source");
    fs::write(
        config_directory.join("run.conf"),
        "set-option -g @startup-source-run config-decoy\n",
    )
    .expect("config run-shell decoy");
    fs::write(
        initial_cwd.join("post.conf"),
        "set-option -g @startup-source-post initial-decoy\n",
    )
    .expect("initial post-startup decoy");
    fs::write(
        runtime_cwd.join("post.conf"),
        "set-option -g @startup-source-post runtime\n",
    )
    .expect("runtime post-startup source");

    let root = config_directory.join("root.conf");
    fs::write(
        &root,
        "source-file a/entry.conf\n\
             if-shell -F 1 'source-file if.conf'\n\
             run-shell -C 'source-file run.conf'\n",
    )
    .expect("startup root");

    let shared = Arc::new(Shared::new(1));
    shared
        .initialize_with_mux_config_files(
            true,
            Some(std::slice::from_ref(&root)),
            Some(&initial_cwd),
        )
        .expect("initialize startup sources");

    for option in [
        "@startup-source-direct",
        "@startup-source-nested",
        "@startup-source-if",
        "@startup-source-run",
        "@startup-source-run-nested",
    ] {
        assert_eq!(read_global_option(&shared, option), "initial", "{option}");
    }
    assert!(
        shared
            .inner
            .lock()
            .config
            .startup_source_client_working_directory
            .is_none()
    );

    let runtime_mailbox = OutboundMailbox::new();
    let (runtime_client, _) = shared.register_subscribed(
        ClientKind::Command,
        None,
        None,
        Arc::clone(&runtime_mailbox),
    );
    shared
        .inner
        .lock()
        .client_entry(runtime_client)
        .working_directory
        .replace(runtime_cwd);
    let mut runtime_context = ExecutionContext::default();
    shared
        .execute(
            runtime_client,
            ClientKind::Command,
            &mut runtime_context,
            &CommandInvocation::new("source-file", ["post.conf"]),
        )
        .expect("runtime source");
    assert_eq!(
        read_global_option(&shared, "@startup-source-post"),
        "runtime"
    );
}

#[test]
fn startup_source_cwd_overrides_a_registered_reentry_client() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let initial_cwd = directory.path().join("initial cwd");
    let reentry_cwd = directory.path().join("reentry cwd");
    fs::create_dir_all(&initial_cwd).expect("initial source directory");
    fs::create_dir_all(&reentry_cwd).expect("reentry source directory");
    fs::write(
        initial_cwd.join("entry.conf"),
        "set-option -g @startup-reentry initial\nsource-file leaf.conf\n",
    )
    .expect("initial reentry source");
    fs::write(
        initial_cwd.join("leaf.conf"),
        "set-option -g @startup-reentry-nested initial\n",
    )
    .expect("initial reentry nested source");
    fs::write(
        reentry_cwd.join("entry.conf"),
        "set-option -g @startup-reentry reentry-decoy\n",
    )
    .expect("reentry source decoy");
    fs::write(
        reentry_cwd.join("leaf.conf"),
        "set-option -g @startup-reentry-nested reentry-decoy\n",
    )
    .expect("reentry nested decoy");

    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (reentry, _) =
        shared.register_subscribed(ClientKind::Command, None, None, Arc::clone(&mailbox));
    {
        let mut inner = shared.inner.lock();
        inner
            .client_entry(reentry)
            .working_directory
            .replace(reentry_cwd.clone());
        inner.config.startup_source_client_working_directory = Some(initial_cwd);
    }
    let mut context = ExecutionContext::default();
    context.set_client_working_directory(Some(reentry_cwd));
    shared
        .execute(
            reentry,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("source-file", ["entry.conf"]),
        )
        .expect("startup reentry source");

    assert_eq!(read_global_option(&shared, "@startup-reentry"), "initial");
    assert_eq!(
        read_global_option(&shared, "@startup-reentry-nested"),
        "initial"
    );
}

#[test]
fn missing_default_startup_config_stays_silent() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let missing = directory.path().join("missing.conf");
    let shared = Arc::new(Shared::new(1));
    let mut report = ConfigLoadReport::startup();

    assert!(
        shared
            .parse_startup_config_file(&missing, &mut report, false)
            .is_none()
    );
    assert!(report.take_startup_causes().is_none());
}

#[cfg(unix)]
#[test]
fn implicit_startup_read_errors_are_retained_without_os_error_suffixes() {
    let path = Path::new("/zz/default/mux.conf");
    for (code, expected) in [
        (libc::EACCES, "/zz/default/mux.conf: Permission denied"),
        (libc::EISDIR, "/zz/default/mux.conf: Is a directory"),
    ] {
        let mut report = ConfigLoadReport::startup();
        report.note_startup_root_read_error(path, &std::io::Error::from_raw_os_error(code), true);
        assert_eq!(
            report.take_startup_causes().as_deref(),
            Some(&[expected.to_owned()][..])
        );
    }
}

#[cfg(unix)]
#[test]
fn startup_source_read_errors_keep_location_and_normalized_child_path() {
    let declaring = "/zz/default/mux.conf";
    let child = Path::new("/zz/default/child.conf");
    let command = CommandInvocation::new("source-file", [child.display().to_string()]).with_source(
        SourceSpan {
            source: declaring.to_owned(),
            line: 7,
            column: 1,
        },
    );
    for (code, expected) in [
        (
            libc::EACCES,
            "/zz/default/mux.conf:7: Permission denied: /zz/default/child.conf",
        ),
        (
            libc::EISDIR,
            "/zz/default/mux.conf:7: Is a directory: /zz/default/child.conf",
        ),
    ] {
        let warning = source_read_error_warning(child, &std::io::Error::from_raw_os_error(code));
        assert_eq!(config_command_error(&command, &warning), expected);
    }
}

#[test]
fn source_read_errors_use_posix_text_on_every_os() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let missing = directory.path().join("missing.conf");
    let error = fs::read(&missing).expect_err("missing source file");
    assert_eq!(
        source_read_error_warning(&missing, &error),
        missing_source_error(&missing)
    );
}

#[test]
fn startup_roots_parse_before_replay_and_retain_ordered_causes() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let first = directory.path().join("first.conf");
    let missing = directory.path().join("missing.conf");
    let invalid = directory.path().join("invalid.conf");
    fs::write(
        &first,
        "display-message -p EARLY\n\
             kill-session -t =missing\n\
             set-option -g @startup-two-phase-after yes\n",
    )
    .expect("first startup config");
    fs::write(&invalid, "set-option 'unterminated\n").expect("invalid startup config");

    let shared = Arc::new(Shared::new(1));
    shared
        .initialize_with_mux_config_files(
            true,
            Some(&[first.clone(), missing.clone(), invalid.clone()]),
            None,
        )
        .expect("initialize mixed startup roots");

    assert_eq!(
        retained_startup_causes(&shared),
        [
            format!("{}: No such file or directory", missing.display()),
            format!("{}:1: EARLY", first.display()),
            format!("{}:2: can't find session: missing", first.display()),
            format!("{}:2: invalid option: unterminated\n", invalid.display()),
        ]
    );
    assert_eq!(
        read_global_option(&shared, "@startup-two-phase-after"),
        "yes"
    );
}

#[test]
fn config_alias_snapshot_spans_startup_roots() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let first = directory.path().join("first.conf");
    let second = directory.path().join("second.conf");
    fs::write(
        &first,
        "set-option -g @startup-root-one yes\n\
             set-option -s command-alias[93] 'startup_alias=set-option -g @startup-alias-seen yes'\n",
    )
    .expect("first startup root");
    fs::write(&second, "startup_alias\n").expect("second startup root");

    let shared = Arc::new(Shared::new(1));
    shared
        .initialize_with_mux_config_files(true, Some(&[first.clone(), second.clone()]), None)
        .expect("initialize startup alias roots");

    assert_eq!(read_global_option(&shared, "@startup-root-one"), "yes");
    assert!(read_global_option(&shared, "@startup-alias-seen").is_empty());
    assert_eq!(
        retained_startup_causes(&shared),
        [format!(
            "{}:1: unknown command: startup_alias",
            second.display()
        )]
    );
    assert_eq!(
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new("show-options", ["-sv", "command-alias[93]"],),
            )
            .expect("read startup alias")
            .output,
        "startup_alias=set-option -g @startup-alias-seen yes"
    );
}

#[test]
fn startup_display_causes_follow_root_and_nested_depth_first_order() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let first = directory.path().join("first.conf");
    let child = directory.path().join("child.conf");
    let grandchild = directory.path().join("grandchild.conf");
    let second = directory.path().join("second.conf");
    fs::write(
        &first,
        format!(
            "new-session -d -s listed\n\
                 display-message -p ROOT_BEFORE\n\
                 source-file '{}'\n\
                 list-sessions -F 'LIST_#{{session_name}}'\n\
                 display-message -p ROOT_AFTER\n",
            child.display()
        ),
    )
    .expect("first startup root");
    fs::write(
        &child,
        format!(
            "display-message -p CHILD_BEFORE\n\
                 source-file '{}'\n\
                 display-message -p CHILD_AFTER\n",
            grandchild.display()
        ),
    )
    .expect("nested startup config");
    fs::write(&grandchild, "display-message -p GRANDCHILD\n").expect("deep startup config");
    fs::write(&second, "display-message -p SECOND_ROOT\n").expect("second startup root");

    let shared = Arc::new(Shared::new(1));
    shared
        .initialize_with_mux_config_files(true, Some(&[first.clone(), second.clone()]), None)
        .expect("initialize valid startup roots");

    let causes = retained_startup_causes(&shared);
    assert_eq!(
        causes,
        [
            format!("{}:2: ROOT_BEFORE", first.display()),
            format!("{}:1: CHILD_BEFORE", child.display()),
            format!("{}:1: GRANDCHILD", grandchild.display()),
            format!("{}:3: CHILD_AFTER", child.display()),
            format!("{}:5: ROOT_AFTER", first.display()),
            format!("{}:1: SECOND_ROOT", second.display()),
        ]
    );
    assert!(causes.iter().all(|cause| !cause.contains("LIST_listed")));
}

#[test]
fn startup_source_invocations_share_one_budget_across_explicit_configs() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let first = directory.path().join("first.conf");
    let second = directory.path().join("second.conf");
    let mut first_body = String::new();
    let mut second_body = String::new();
    for index in 1..=60 {
        let leaf = directory.path().join(format!("leaf{index}.conf"));
        fs::write(&leaf, format!("set-option -g @startup{index} yes\n")).expect("startup leaf");
        let line = format!("source-file '{}'\n", leaf.display());
        if index <= 30 {
            first_body.push_str(&line);
        } else {
            second_body.push_str(&line);
        }
    }
    second_body.push_str("set-option -g @startup-after yes\n");
    fs::write(&first, first_body).expect("first startup config");
    fs::write(&second, second_body).expect("second startup config");

    let shared = Arc::new(Shared::new(1));
    shared
        .initialize_with_mux_config_files(true, Some(&[first, second]), None)
        .expect("initialize startup configs");

    assert_eq!(read_global_option(&shared, "@startup50"), "yes");
    assert!(read_global_option(&shared, "@startup51").is_empty());
    assert!(read_global_option(&shared, "@startup60").is_empty());
    assert_eq!(read_global_option(&shared, "@startup-after"), "yes");
}

#[test]
fn startup_source_depth_does_not_count_the_top_level_config() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let root = directory.path().join("root.conf");
    for level in 1..=51 {
        let mut body = format!("set-option -g @startup-depth {level}\n");
        if level < 51 {
            let _ = writeln!(
                body,
                "source-file '{}'",
                directory
                    .path()
                    .join(format!("f{}.conf", level + 1))
                    .display()
            );
        }
        let _ = writeln!(body, "set-option -g @startup-after{level} yes");
        fs::write(directory.path().join(format!("f{level}.conf")), body)
            .expect("startup chain level");
    }
    fs::write(
        &root,
        format!(
            "source-file '{}'\n",
            directory.path().join("f1.conf").display()
        ),
    )
    .expect("startup chain root");

    let shared = Arc::new(Shared::new(1));
    shared
        .initialize_with_mux_config_files(true, Some(std::slice::from_ref(&root)), None)
        .expect("initialize startup chain");

    assert_eq!(read_global_option(&shared, "@startup-depth"), "50");
    assert_eq!(read_global_option(&shared, "@startup-after50"), "yes");
    assert!(read_global_option(&shared, "@startup-after51").is_empty());
    assert_eq!(
        retained_startup_causes(&shared),
        [format!(
            "{}:2: {NESTED_SOURCE_LIMIT_ERROR}",
            directory.path().join("f50.conf").display()
        )]
    );
}

#[cfg(unix)]
#[test]
fn startup_command_and_source_causes_keep_declaring_locations() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let invalid_root = directory.path().join("invalid-root.conf");
    let root = directory.path().join("root.conf");
    let nested = directory.path().join("nested.conf");
    let unreadable = directory.path().join("unreadable");
    fs::create_dir(&unreadable).expect("unreadable startup source");
    fs::write(&invalid_root, "wibble\n").expect("invalid startup root");
    fs::write(&nested, "wibble\n").expect("nested startup config");
    fs::write(
        &root,
        format!(
            "link-window\n\
                 source-file nested-missing.conf\n\
                 source-file 'invalid\0pattern.conf'\n\
                 source-file '{}'\n\
                 source-file '{}'\n\
                 display-message -p \"MULTI_FIRST\n\
                 MULTI_SECOND\"\n",
            unreadable.display(),
            nested.display(),
        ),
    )
    .expect("mixed startup source failures");

    let shared = Arc::new(Shared::new(1));
    shared
        .initialize_with_mux_config_files(true, Some(&[invalid_root.clone(), root.clone()]), None)
        .expect("initialize startup source failures");
    let causes = retained_startup_causes(&shared);

    assert_eq!(
        causes[0],
        format!("{}:1: unknown command: wibble", invalid_root.display())
    );
    assert_eq!(
        causes[1],
        format!(
            "{}:1: unsupported tmux command: link-window",
            root.display()
        )
    );
    assert_eq!(
        causes[2],
        format!(
            "{}:2: No such file or directory: nested-missing.conf",
            root.display()
        )
    );
    assert!(causes[3].starts_with(&format!("{}:3: ", root.display())));
    assert!(causes[3].contains("invalid\0pattern.conf"));
    assert_eq!(
        causes[4],
        format!(
            "{}:4: Is a directory: {}",
            root.display(),
            unreadable.display()
        )
    );
    assert_eq!(
        causes[5],
        format!("{}:1: unknown command: wibble", nested.display())
    );
    assert_eq!(
        causes[6],
        format!("{}:7: MULTI_FIRST\nMULTI_SECOND", root.display())
    );
    assert_eq!(causes.len(), 7);
}

#[test]
fn startup_hook_callback_failure_reports_and_continues() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let root = directory.path().join("hook-callback.conf");
    fs::write(
        &root,
        "set-hook -g after-display-message 'run-shell -C \"{ display-message -p quoted }\" ; set-option -g @startup-hook-after yes'\n\
             display-message -p TRIGGER\n\
             set-option -g @startup-after yes\n",
    )
    .expect("startup hook callback fixture");

    let shared = Arc::new(Shared::new(1));
    shared
        .initialize_with_mux_config_files(true, Some(std::slice::from_ref(&root)), None)
        .expect("initialize startup hook callback config");

    assert_eq!(read_global_option(&shared, "@startup-hook-after"), "yes");
    assert_eq!(read_global_option(&shared, "@startup-after"), "yes");
    assert_eq!(
        retained_startup_causes(&shared),
        [
            "syntax error".to_owned(),
            format!("{}:2: TRIGGER", root.display())
        ]
    );
}

#[test]
fn startup_source_refusal_keeps_the_declaring_location() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let root = directory.path().join("root.conf");
    let present = directory.path().join("present.conf");
    fs::write(&present, "set-option -g @startup-present yes\n").expect("present source");
    let mut body = String::new();
    for index in 1..=50 {
        let _ = writeln!(body, "source-file -q missing{index}.conf");
    }
    let _ = writeln!(body, "source-file '{}'", present.display());
    body.push_str("set-option -g @startup-continued yes\n");
    fs::write(&root, body).expect("startup config");

    let shared = Arc::new(Shared::new(1));
    let mut report = ConfigLoadReport::default();
    let mut source_invocations = SourceInvocationAccounting::Startup { used: 0 };
    shared
        .load_config_file_with_report_for_terminal(
            &root,
            &mut ExecutionContext::default(),
            0,
            &mut report,
            ClientTerminal::NoClient,
            None,
            &mut source_invocations,
        )
        .expect("load startup config");

    assert_eq!(
        report.source_errors(),
        [format!(
            "{}:51: {NESTED_SOURCE_LIMIT_ERROR}",
            root.display()
        )]
    );
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);
    shared.route_config_replay_errors(control, ClientKind::Control, None, &mut report);
    assert!(matches!(
        take_reliable_messages(&mailbox).as_slice(),
        [ProtocolMessage::Event(Event {
            payload: EventPayload::ClientMessage {
                kind: ClientMessageKind::Error,
                text,
                ..
            },
            ..
        })] if text == &format!("{}:51: {NESTED_SOURCE_LIMIT_ERROR}", root.display())
    ));
    assert!(read_global_option(&shared, "@startup-present").is_empty());
    assert_eq!(read_global_option(&shared, "@startup-continued"), "yes");
}

#[test]
fn control_home_lookup_reads_the_server_home_then_the_daemon_passwd_entry() {
    let shared = Arc::new(Shared::new(1));
    let passwd = zz_mux::user_home(None).expect("daemon host passwd home");

    assert_eq!(
        shared.resolve_home_directories(&[String::new()]),
        [Some(passwd.clone())]
    );

    shared
        .inner
        .lock()
        .engine
        .seed_global_environment([("HOME", "")]);
    assert_eq!(
        shared.resolve_home_directories(&[String::new()]),
        [Some(passwd.clone())]
    );

    shared
        .inner
        .lock()
        .engine
        .seed_global_environment([("HOME", "/server/home")]);
    assert_eq!(
        shared.resolve_home_directories(&[String::new()]),
        [Some("/server/home".to_owned())]
    );

    assert_eq!(
        shared.resolve_home_directories(&[
            "root".to_owned(),
            "zz-no-such-user".to_owned(),
            String::new(),
        ]),
        [
            zz_mux::user_home(Some("root")),
            None,
            Some("/server/home".to_owned()),
        ]
    );
}

#[test]
fn a_forged_alias_group_block_is_an_unknown_command_at_every_entry() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let config = directory.path().join("forgery.conf");
    fs::write(
        &config,
        "set-environment -g FORGE_BEFORE yes\n\
             __zz-command-alias-group { set-environment -g FORGE_CHILD ran }\n\
             set-environment -g FORGE_AFTER yes\n",
    )
    .expect("forged config");

    let shared = Arc::new(Shared::new(1));
    let client = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "forgery"]),
        )
        .expect("forgery session");

    let forged = {
        let inner = shared.inner.lock();
        inner.engine.parse_config(
            "<forgery>",
            "__zz-command-alias-group { set-environment -g FORGE_TYPED ran }",
        )
    };
    assert!(forged.diagnostics.is_empty());
    let forged = forged.commands.into_iter().next().expect("forged command");
    assert!(forged.argument_is_command_block(0));
    assert!(!MuxEngine::is_command_alias_group(&forged));

    let unknown = ServerError::CommandParse("unknown command: __zz-command-alias-group".to_owned());
    let prepared = shared
        .prepare_command_list(vec![forged.clone()])
        .into_iter()
        .next()
        .expect("prepared forgery");
    assert_eq!(prepared.canonical_name, None);
    assert!(!prepared.alias_matched);
    assert_eq!(
        prepared.result,
        PreparedCommandResult::Error(unknown.clone())
    );

    for (request_id, kind, claims_prepared) in [
        (1, ClientKind::Command, false),
        (2, ClientKind::Control, false),
        (3, ClientKind::Control, true),
    ] {
        assert_eq!(
            shared.execute_command_request_with_prepared(
                client,
                kind,
                &mut context,
                request_id,
                &forged,
                claims_prepared,
            ),
            CommandResponse::Error {
                request_id,
                error: unknown.clone(),
                output: RawText::default(),
            }
        );
    }

    assert_eq!(
        shared.execute_command_request(
            client,
            ClientKind::Command,
            &mut context,
            4,
            &CommandInvocation::new("source-file", [config.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 4,
            output: format!(
                "{}:2: unknown command: __zz-command-alias-group\n",
                config.display()
            )
            .into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );

    for name in ["FORGE_TYPED", "FORGE_CHILD", "FORGE_BEFORE", "FORGE_AFTER"] {
        assert_eq!(
            shared.inner.lock().engine.global_environment_variable(name),
            None
        );
    }

    shared
        .execute(
            client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[90]",
                    "genuine=set-environment -g GENUINE_FIRST yes ; set-environment -g GENUINE_SECOND yes",
                ],
            ),
        )
        .expect("genuine alias");
    let genuine = shared
        .prepare_command_list(vec![CommandInvocation::new("genuine", [] as [&str; 0])])
        .into_iter()
        .next()
        .expect("prepared genuine alias");
    assert!(genuine.alias_matched);
    assert_eq!(genuine.result, PreparedCommandResult::Ready);
    assert!(MuxEngine::is_command_alias_group(&genuine.invocation));
    assert!(matches!(
        shared.execute_command_request_with_prepared(
            client,
            ClientKind::Command,
            &mut context,
            5,
            &genuine.invocation,
            true,
        ),
        CommandResponse::Success { exit_code: 0, .. }
    ));
    for name in ["GENUINE_FIRST", "GENUINE_SECOND"] {
        assert_eq!(
            shared.inner.lock().engine.global_environment_variable(name),
            Some("yes".to_owned())
        );
    }
}

#[test]
fn config_alias_snapshot_aborts_file_and_preserves_parse_only() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let root = directory.path().join("root.conf");
    let parse_only = directory.path().join("parse-only.conf");
    fs::write(
        &root,
        "CONFIG_ABORT_ASSIGN=kept\n\
             set-option -s command-alias[90] 'same=set-option -g @alias-same new' ; same\n\
             set-option -s command-alias[91] 'later=set-option -g @alias-later new'\n\
             later\n\
             set-option -g @before-preparation-error yes\n\
             badtyped\n\
             set-option -g @after-preparation-error yes\n",
    )
    .expect("alias snapshot config");
    fs::write(
        &parse_only,
        "CONFIG_PARSE_ONLY_ASSIGN=hidden\n\
             set-option -s command-alias[90] 'same=set-option -g @parse-only-alias changed'\n\
             same\n\
             badtyped\n",
    )
    .expect("parse-only alias config");

    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "config-alias"]),
        )
        .expect("config alias session");
    for (index, value) in [
        ("90", "same=set-option -g @alias-same old"),
        ("91", "later=set-option -g @alias-later old"),
        ("92", "badtyped=confirm-before { wibble }"),
    ] {
        shared
            .execute(
                command,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    ["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .expect("seed config alias");
    }

    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            1,
            &CommandInvocation::new("source-file", [root.display().to_string()]),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: format!("{}:6: unknown command: wibble\n", root.display()).into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert!(read_global_option(&shared, "@alias-same").is_empty());
    assert!(read_global_option(&shared, "@alias-later").is_empty());
    assert!(read_global_option(&shared, "@before-preparation-error").is_empty());
    assert!(read_global_option(&shared, "@after-preparation-error").is_empty());
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("CONFIG_ABORT_ASSIGN"),
        Some("kept".to_owned())
    );

    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            2,
            &CommandInvocation::new(
                "source-file",
                ["-n".to_owned(), parse_only.display().to_string()],
            ),
        ),
        CommandResponse::Success {
            request_id: 2,
            output: format!("{}:4: unknown command: wibble\n", parse_only.display()).into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert!(read_global_option(&shared, "@parse-only-alias").is_empty());
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("CONFIG_PARSE_ONLY_ASSIGN"),
        None
    );
    assert_eq!(
        shared
            .execute(
                command,
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new("show-options", ["-sv", "command-alias[90]"]),
            )
            .expect("read preserved alias")
            .output,
        "same=set-option -g @alias-same old"
    );
}

#[test]
fn config_alias_groups_preserve_authoritative_client_terminal() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let root = directory.path().join("aliases.conf");
    fs::write(&root, "single\nmulti\n").expect("alias config");
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    for (index, value) in [
        ("40", "single=new-session -s alias-single"),
        (
            "41",
            "multi=new-session -s alias-multi ; display-message -p reached",
        ),
    ] {
        shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    ["-s", &format!("command-alias[{index}]"), value],
                ),
            )
            .expect("seed config alias");
    }
    let mut report = ConfigLoadReport::default();
    let mut source_invocations = SourceInvocationAccounting::default();
    shared
        .load_config_file_with_report_for_terminal(
            &root,
            &mut context,
            0,
            &mut report,
            ClientTerminal::NoClient,
            None,
            &mut source_invocations,
        )
        .expect("load alias config");

    let inner = shared.inner.lock();
    for name in ["alias-single", "alias-multi"] {
        assert!(
            inner
                .engine
                .state
                .sessions
                .values()
                .any(|session| session.name == name),
            "missing {name}"
        );
    }
    assert!(report.replay_issues.is_empty());
}

#[test]
fn parse_only_construction_uses_prefile_environment_and_truncates_verbose_output() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let source = directory.path().join("parse-only-environment.conf");
    fs::write(
        &source,
        "PARSE_ONLY_VALUE=new\n\
             display-message -p $PARSE_ONLY_VALUE\n\
             parse_only_alias\n\
             parse_only_echo argument\n\
             parse_only_alias ; display-message -Q\n\
             set-option -g @after-parse-only-construction yes\n",
    )
    .expect("parse-only environment config");

    let shared = Arc::new(Shared::new(1));
    let command = ClientId(u64::from(u16::MAX));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("set-environment", ["-g", "PARSE_ONLY_VALUE", "old"]),
        )
        .expect("seed parse-only environment");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[93]",
                    "parse_only_alias=display-message -p $PARSE_ONLY_VALUE",
                ],
            ),
        )
        .expect("seed parse-only alias");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-s",
                    "command-alias[94]",
                    "parse_only_echo=display-message -p",
                ],
            ),
        )
        .expect("seed parse-only argument alias");

    assert_eq!(
        shared.execute_command_request(
            command,
            ClientKind::Command,
            &mut context,
            1,
            &CommandInvocation::new(
                "source-file",
                ["-nv".to_owned(), source.display().to_string()],
            ),
        ),
        CommandResponse::Success {
            request_id: 1,
            output: format!(
                "{}:2: display-message -p old\n\
                     {}:3: display-message -p old\n\
                     {}:3: display-message -p old\n\
                     {}:4: display-message -p argument\n\
                     {}:4: display-message -p argument\n\
                     {}:5: display-message -p old\n\
                     {}:5: command display-message: unknown flag -Q\n",
                source.display(),
                source.display(),
                source.display(),
                source.display(),
                source.display(),
                source.display(),
                source.display(),
            )
            .into(),
            exit_code: 1,
            stderr: String::new(),
            stdout_claim: StdoutClaim::Print,
        }
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .global_environment_variable("PARSE_ONLY_VALUE"),
        Some("old".to_owned())
    );
    assert!(read_global_option(&shared, "@after-parse-only-construction").is_empty());
}

#[test]
fn config_replay_drops_failed_same_line_groups() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let root = directory.path().join("root.conf");
    let leaf = directory.path().join("leaf.conf");
    let nested_failure = directory.path().join("nested-failure.conf");
    let nested_parser_failure = directory.path().join("nested-parser-failure.conf");
    let mixed_match = directory.path().join("mixed-match.conf");
    let matched_read_failure = directory.path().join("matched-read-failure");
    let matched_read_parent = directory.path().join("matched-read-parent.conf");
    let source_identity = directory.path().join("source-identity.conf");
    fs::write(&leaf, "set-option -g @depth-leaf yes\n").expect("depth leaf");
    fs::write(
        &nested_failure,
        "kill-session -t =missing ; set-option -g @nested-same yes\n\
             set-option -g @nested-next yes\n",
    )
    .expect("nested failure config");
    fs::write(&nested_parser_failure, "set \\").expect("nested parser failure config");
    fs::write(&mixed_match, "set-option -g @mixed-loaded yes\n").expect("mixed source match");
    fs::create_dir(&matched_read_failure).expect("matched read failure directory");
    fs::write(
        &matched_read_parent,
        format!(
            "source-file '{}' ; set-option -g @read-same yes\n\
                 set-option -g @read-next yes\n",
            matched_read_failure.display()
        ),
    )
    .expect("matched read failure parent");
    fs::write(&source_identity, "set-option -g @source-identity yes\n")
        .expect("source identity config");

    for level in (1..=49).rev() {
        let body = if level == 49 {
            format!(
                "source-file '{}' ; set-option -g @depth-same yes\n\
                     set-option -g @depth-next yes\n",
                leaf.display()
            )
        } else {
            format!(
                "source-file '{}'\n",
                directory
                    .path()
                    .join(format!("f{}.conf", level + 1))
                    .display()
            )
        };
        fs::write(directory.path().join(format!("f{level}.conf")), body).expect("depth config");
    }

    let glob_error_probe = if cfg!(unix) {
        "source-file 'invalid\0pattern.conf' ; set-option -g @glob-same yes\n\
             set-option -g @glob-next yes\n"
    } else {
        ""
    };
    fs::write(
        &root,
        format!(
            "source-file '{}'\n\
                 source-file missing-loud.conf ; set-option -g @loud-same yes\n\
                 set-option -g @loud-next yes\n\
                 {glob_error_probe}\
                 source-file -q missing-quiet.conf ; set-option -g @quiet-same yes\n\
                 set-option -g @quiet-next yes\n\
                 link-window ; set-option -g @unsupported-command-same yes\n\
                 set-option -g @unsupported-command-next yes\n\
                 kill-session -t =missing ; set-option -g @runtime-same yes\n\
                 set-option -g @runtime-next yes\n\
                 source-file '{}' ; set-option -g @nested-parent-same yes\n\
                 set-option -g @nested-parent-next yes\n\
                 source-file '{}' ; set-option -g @parser-parent-same yes\n\
                 set-option -g @parser-parent-next yes\n\
                 source-file missing-mixed.conf '{}' ; set-option -g @mixed-same yes\n\
                 set-option -g @mixed-next yes\n\
                 source-file - ; set-option -g @stdin-same yes\n\
                 set-option -g @stdin-next yes\n\
                 run-shell 'exit 1' ; set-option -g @async-same yes\n\
                 set-option -g @async-next yes\n\
                 source-file '{}'\n",
            directory.path().join("f1.conf").display(),
            nested_failure.display(),
            nested_parser_failure.display(),
            mixed_match.display(),
            source_identity.display(),
        ),
    )
    .expect("root config");

    let shared = Arc::new(Shared::new(1));
    shared
        .load_config_file(&root, &mut ExecutionContext::default(), 0)
        .expect("replay config");
    let mut read_report = ConfigLoadReport::default();
    shared
        .load_config_file_with_report(
            &matched_read_parent,
            &mut ExecutionContext::default(),
            0,
            &mut read_report,
        )
        .expect("replay matched read failure");

    let matched_read_suffix = format!(": {}", matched_read_failure.display());
    assert!(
        read_report
            .source_errors()
            .iter()
            .any(|error| error.ends_with(&matched_read_suffix)),
        "{:?}",
        read_report.source_errors()
    );

    for marker in [
        "@depth-leaf",
        "@depth-same",
        "@loud-same",
        "@runtime-same",
        "@nested-same",
    ] {
        assert!(read_global_option(&shared, marker).is_empty(), "{marker}");
    }
    #[cfg(unix)]
    {
        assert!(read_global_option(&shared, "@glob-same").is_empty());
        assert_eq!(read_global_option(&shared, "@glob-next"), "yes");
    }
    for marker in [
        "@depth-next",
        "@loud-next",
        "@quiet-same",
        "@quiet-next",
        "@unsupported-command-same",
        "@unsupported-command-next",
        "@runtime-next",
        "@nested-next",
        "@nested-parent-same",
        "@nested-parent-next",
        "@parser-parent-same",
        "@parser-parent-next",
        "@mixed-loaded",
        "@mixed-same",
        "@mixed-next",
        "@stdin-same",
        "@stdin-next",
        "@async-same",
        "@async-next",
        "@read-same",
        "@read-next",
        "@source-identity",
    ] {
        assert_eq!(read_global_option(&shared, marker), "yes", "{marker}");
    }
}

#[test]
fn one_startup_source_command_can_load_more_than_fifty_paths() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let root = directory.path().join("root.conf");
    let mut body = String::from("source-file");
    for index in 1..=60 {
        let leaf = directory.path().join(format!("leaf{index}.conf"));
        fs::write(&leaf, format!("set-option -g @startup-many {index}\n")).expect("startup leaf");
        let _ = write!(body, " '{}'", leaf.display());
    }
    body.push('\n');
    fs::write(&root, body).expect("startup config");

    let shared = Arc::new(Shared::new(1));
    shared
        .initialize_with_mux_config_files(true, Some(std::slice::from_ref(&root)), None)
        .expect("initialize startup config");

    assert_eq!(read_global_option(&shared, "@startup-many"), "60");
}

#[test]
fn reload_config_replays_the_root_under_one_startup_budget() {
    fn write_leaves(directory: &Path, generation: &str) {
        for index in 1..=52 {
            fs::write(
                directory.join(format!("leaf{index}.conf")),
                format!("set-option -g @reload-source {generation}{index}\n"),
            )
            .expect("reload source leaf");
        }
    }

    let directory = tempfile::tempdir().expect("temporary config directory");
    let root = directory.path().join("root.conf");
    let mut body = String::new();
    for index in 1..=52 {
        let _ = writeln!(
            body,
            "source-file '{}'",
            directory.path().join(format!("leaf{index}.conf")).display()
        );
    }
    body.push_str("set-option -g @reload-after yes\n");
    fs::write(&root, body).expect("reload config");
    write_leaves(directory.path(), "startup");

    let shared = Arc::new(Shared::new(1));
    shared
        .initialize_with_mux_config_files(true, Some(std::slice::from_ref(&root)), None)
        .expect("initialize startup config");
    assert_eq!(read_global_option(&shared, "@reload-source"), "startup50");

    write_leaves(directory.path(), "reload");
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    take_reliable_messages(&mailbox);
    let mut context = ExecutionContext::default();
    shared
        .reload_user_config_with_mux_file(client, &mut context, Some(&root))
        .expect("reload user configuration");

    assert_eq!(read_global_option(&shared, "@reload-source"), "reload50");
    assert_eq!(read_global_option(&shared, "@reload-after"), "yes");
    let warnings = take_reliable_messages(&mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload:
                    EventPayload::ClientMessage {
                        kind: ClientMessageKind::Warning,
                        text,
                        ..
                    },
                ..
            }) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    for line in [51, 52] {
        let expected = format!("{}:{line}: {NESTED_SOURCE_LIMIT_ERROR}", root.display());
        assert!(
            warnings.contains(&expected),
            "missing {expected} in {warnings:?}"
        );
    }
}

#[test]
fn tmux_import_prepends_and_reloads_a_binding() {
    let directory = tempfile::tempdir().unwrap();
    let donor = directory.path().join("donor.conf");
    let target = directory.path().join("mux.conf");
    fs::write(
        &donor,
        "bind-key -T prefix F11 display-message IMPORTED\nset -g @import-priority donor\n",
    )
    .unwrap();
    let tail = "# user tail\nset -g @import-priority user\n";
    fs::write(&target, tail).unwrap();
    let shared = Arc::new(Shared::new(1));
    *shared.zz_mux_config_path.lock() = Some(target.clone());
    shared.initialize(true).unwrap();
    let mut context = ExecutionContext::default();
    let output = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("import-tmux-config", [donor.display().to_string()]),
        )
        .unwrap()
        .output;
    let saved = fs::read_to_string(&target).unwrap();
    assert!(saved.starts_with(MUX_IMPORT_START));
    assert!(saved.ends_with(tail));
    assert!(output.contains("2 commands copied, 0 commented"));
    let binding = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("list-keys", ["-T", "prefix", "F11"]),
        )
        .unwrap()
        .output;
    assert!(binding.contains("IMPORTED"));
    assert_eq!(read_global_option(&shared, "@import-priority"), "user");
}

#[test]
fn tmux_reimport_replaces_block_and_preserves_surrounding_text() {
    let first = replace_tmux_import(
        "# tail\r\nset -g mouse off",
        Path::new("/old"),
        "set -g prefix C-a\n",
    )
    .unwrap();
    let existing = format!("# before\r\n{first}");
    let next = replace_tmux_import(&existing, Path::new("/new"), "set -g prefix C-x\n").unwrap();
    assert!(next.starts_with("# before\r\n# zz-import-tmux-begin: /new\n"));
    assert!(next.ends_with("# tail\r\nset -g mouse off"));
    assert!(!next.contains("/old"));
    assert_eq!(next.matches(MUX_IMPORT_START).count(), 1);
    assert!(!next.contains("prefix C-a"));
}

#[test]
fn tmux_reimport_ignores_marker_comments_inside_the_donor() {
    let donor = "# zz-import-tmux-end: /old\nset -g prefix C-a\n";
    let first = replace_tmux_import("# tail\n", Path::new("/old"), donor).unwrap();
    let next = replace_tmux_import(&first, Path::new("/new"), "set -g prefix C-x\n").unwrap();
    assert_eq!(
        next,
        "# zz-import-tmux-begin: /new\nset -g prefix C-x\n# zz-import-tmux-end: /new\n# tail\n"
    );
}

#[test]
fn tmux_import_comments_unsupported_constructs_without_running_commands() {
    let engine = MuxEngine::default();
    let source =
        "customize-mode\nlink-window \\\n -a user\nrun-shell 'exit 1'\nset -g prefix C-a\n";
    let (text, copied, unsupported) = prepare_tmux_import(&engine, Path::new("/donor"), source);
    assert!(text.starts_with("customize-mode\n# zz-unsupported: link-window"));
    assert!(text.contains("# zz-unsupported:  -a user\n"));
    assert!(text.contains("run-shell 'exit 1'\n"));
    assert_eq!(copied, 3);
    assert_eq!(unsupported.len(), 1);
}

#[test]
fn tmux_import_missing_path_reports_the_path() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.conf");
    let shared = Arc::new(Shared::new(1));
    let error = shared
        .import_tmux_configuration(
            Some(missing.to_str().unwrap()),
            &ExecutionContext::default(),
        )
        .unwrap_err();
    let DaemonError::Server(ServerError::InvalidCommand(message)) = error else {
        panic!("{error:?}");
    };
    assert_eq!(message, format!("{}: entity not found", missing.display()));
}

#[test]
fn tmux_import_without_path_uses_discover_tmux_config() {
    let actual = resolve_tmux_import_path(None, &ExecutionContext::default());
    match discover_tmux_config() {
        Some(expected) => assert_eq!(actual.unwrap(), expected),
        None => assert!(
            actual
                .unwrap_err()
                .to_string()
                .contains("no tmux configuration found")
        ),
    }
}

#[test]
fn startup_config_selects_only_explicit_files_or_mux() {
    let mux = PathBuf::from("/home/u/.config/zz/mux.conf");
    let explicit = vec![
        PathBuf::from("/tmp/first.conf"),
        PathBuf::from("/tmp/second.conf"),
    ];
    assert_eq!(
        startup_mux_config_files(true, None, || Some(mux.clone())),
        [mux]
    );
    assert_eq!(
        startup_mux_config_files(true, Some(&explicit), || panic!("default with -f")),
        explicit
    );
    assert!(
        startup_mux_config_files(false, Some(&explicit), || panic!("disabled config")).is_empty()
    );
    assert!(startup_mux_config_files(true, None, || None).is_empty());
}

#[test]
fn config_files_is_empty_when_loading_is_disabled_and_retains_the_default_selection() {
    let default = PathBuf::from("/tmp/default.conf");
    assert_eq!(
        startup_mux_config_files(true, None, || Some(default.clone())),
        [default]
    );
    let shared = Arc::new(Shared::new(1));
    shared.initialize(false).expect("initialize without config");
    let config_files = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("display-message", ["-p", "#{config_files}"]),
        )
        .expect("show empty startup config files")
        .output;
    assert_eq!(config_files, "\n");
}

#[test]
fn reload_config_preserves_explicit_root_bindings() {
    let directory = tempfile::tempdir().expect("scratch config");
    let root = directory.path().join("explicit.conf");
    let mux = directory.path().join("mux.conf");
    fs::write(
        &root,
        "bind-key -T prefix F11 display-message EXPLICIT_BINDING\n",
    )
    .unwrap();
    fs::write(&mux, "bind-key -T prefix F12 display-message MUX_IGNORED\n").unwrap();
    let shared = Arc::new(Shared::new(1));
    *shared.zz_mux_config_path.lock() = Some(mux);
    shared
        .initialize_with_mux_config_files(true, Some(std::slice::from_ref(&root)), None)
        .unwrap();
    fs::write(
        &root,
        "bind-key -T prefix F11 display-message RELOADED_BINDING\n",
    )
    .unwrap();
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("reload-config", [] as [&str; 0]),
        )
        .unwrap();
    let output = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("list-keys", ["-T", "prefix", "F11"]),
        )
        .unwrap()
        .output;
    assert!(output.contains("RELOADED_BINDING"));
    assert_eq!(shared.selected_mux_config_files(), [root]);
}

#[test]
fn reload_config_files_tracks_the_selected_file_or_empty_selection() {
    let directory = tempfile::tempdir().expect("temporary config directory");
    let startup = directory.path().join("startup.conf");
    let reloaded = directory.path().join("reloaded.conf");
    fs::write(&startup, "set -g prefix C-a\n").expect("startup config");
    fs::write(&reloaded, "set -g prefix C-x\n").expect("reload config");
    let shared = Arc::new(Shared::new(1));
    shared
        .initialize_with_mux_config_files(true, Some(std::slice::from_ref(&startup)), None)
        .expect("initialize startup config");
    let mut context = ExecutionContext::default();

    shared
        .reload_user_config_with_mux_file(ClientId(7), &mut context, Some(&reloaded))
        .expect("reload selected config");
    let selected = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("display-message", ["-p", "#{config_files}"]),
        )
        .expect("show reloaded config file")
        .output;
    assert_eq!(selected.as_str(), reloaded.to_string_lossy());

    shared
        .reload_user_config_with_mux_file(ClientId(7), &mut context, None)
        .expect("reload without a selected config");
    let selected = shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("display-message", ["-p", "#{config_files}"]),
        )
        .expect("show empty reload selection")
        .output;
    assert_eq!(selected, "\n");
}

#[test]
fn startup_causes_wait_for_attachment_and_never_drain_to_command_clients() {
    let shared = Arc::new(Shared::new(1));
    *shared.startup_config_causes.lock() = Some(vec!["retained".to_owned()]);
    let command_mailbox = OutboundMailbox::new();
    let (command, _) = shared.register_subscribed(
        ClientKind::Command,
        None,
        None,
        Arc::clone(&command_mailbox),
    );
    assert_eq!(
        shared.try_deliver_startup_config_causes(command, &command_mailbox, true),
        StartupConfigDelivery::Ineligible
    );

    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    assert_eq!(
        shared.try_deliver_startup_config_causes(control, &control_mailbox, false),
        StartupConfigDelivery::Ineligible
    );
    assert_eq!(retained_startup_causes(&shared), ["retained"]);
    assert!(startup_config_cause_events(&take_reliable_messages(&control_mailbox)).is_empty());

    let (session, _, _) = switch_test_session(&shared, "startup-control");
    let snapshot = shared.attach(control, session).expect("attach control");
    assert!(shared.send_attached(control, &control_mailbox, session, snapshot));
    let messages = take_reliable_messages(&control_mailbox);
    let attached = messages
        .iter()
        .position(|message| matches!(message, ProtocolMessage::Attached { .. }))
        .expect("attached delivery");
    let causes = messages
        .iter()
        .position(|message| {
            matches!(
                message,
                ProtocolMessage::Event(Event {
                    payload: EventPayload::StartupConfigCauses { .. },
                    ..
                })
            )
        })
        .expect("startup cause delivery");
    assert!(attached < causes);
    assert_eq!(
        startup_config_cause_events(&messages),
        [vec!["retained".to_owned()]]
    );
    assert!(shared.startup_config_causes.lock().is_none());
}

#[test]
fn attached_interactive_client_opens_the_startup_configuration_error_view() {
    let shared = Arc::new(Shared::new(1));
    *shared.startup_config_causes.lock() = Some(vec![
        "root.conf:1: first".to_owned(),
        "nested.conf:2: second\ncontinued".to_owned(),
    ]);
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (session, _, pane) = switch_test_session(&shared, "startup-interactive");
    let snapshot = shared.attach(client, session).expect("attach interactive");

    assert!(shared.send_attached(client, &mailbox, session, snapshot));
    let messages = take_reliable_messages(&mailbox);
    assert!(matches!(
        messages.first(),
        Some(ProtocolMessage::Attached { .. })
    ));
    let ProtocolMessage::Event(Event {
        payload:
            EventPayload::CommandOutput {
                output_id,
                pane: output_pane,
                viewport: Some(viewport),
            },
        ..
    }) = &messages[1]
    else {
        panic!("expected reliable startup command-output viewport after Attached");
    };
    assert_eq!(*output_pane, pane);
    assert_eq!(viewport.title(), "configuration errors");
    let text = viewport_text(viewport);
    assert!(text.contains("root.conf:1: first"));
    assert!(text.contains("nested.conf:2: second"));
    assert!(text.contains("continued"));
    assert!(!messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Event(Event {
            payload: EventPayload::Snapshot(_),
            ..
        })
    )));
    assert!(mailbox.state.lock().command_output.is_none());
    assert!(shared.startup_config_causes.lock().is_none());

    let terminal = Arc::clone(
        &shared.inner.lock().clients[&client]
            .command_output
            .as_ref()
            .unwrap()
            .terminal,
    );
    terminal.resize(47, 9, 8, 18);
    let update = take_command_output_message(&mailbox);
    assert_eq!(command_output_message_id(&update), *output_id);
    assert!(matches!(
        update,
        ProtocolMessage::Event(Event {
            payload: EventPayload::CommandOutput {
                viewport: Some(viewport),
                ..
            },
            ..
        }) if viewport.columns == 47 && viewport.rows == 9
    ));
}

#[test]
fn control_startup_delivery_keeps_the_raw_tail_beyond_the_interactive_preview() {
    let mut report = ConfigLoadReport::startup();
    report.note_startup_cause(&format!(
        "BEGIN{}",
        "x".repeat(MAX_STARTUP_CONFIG_CAUSE_BYTES - "BEGIN".len())
    ));
    report.note_startup_cause("CONTROL_TAIL");
    let causes = report.take_startup_causes().expect("raw startup causes");
    assert!(!startup_config_preview(&causes).contains("CONTROL_TAIL"));

    let shared = Arc::new(Shared::new(1));
    *shared.startup_config_causes.lock() = Some(causes.clone());
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    let (session, _, _) = switch_test_session(&shared, "startup-control-raw-tail");
    let snapshot = shared.attach(client, session).expect("attach control");
    take_reliable_messages(&mailbox);

    assert!(shared.send_attached(client, &mailbox, session, snapshot));
    assert_eq!(
        startup_config_cause_events(&take_reliable_messages(&mailbox)),
        [causes]
    );
}

#[test]
fn interactive_startup_output_retains_blank_history_from_beginning_to_end() {
    let shared = Arc::new(Shared::new(1));
    *shared.startup_config_causes.lock() = Some(vec![format!("BEGIN{}END", "\n".repeat(2_048))]);
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let (session, _, _) = switch_test_session(&shared, "startup-interactive-history");
    let snapshot = shared.attach(client, session).expect("attach interactive");
    take_reliable_messages(&mailbox);

    assert!(shared.send_attached(client, &mailbox, session, snapshot));
    let messages = take_reliable_messages(&mailbox);
    assert!(matches!(
        messages.get(1),
        Some(ProtocolMessage::Event(Event {
            payload: EventPayload::CommandOutput {
                viewport: Some(viewport),
                ..
            },
            ..
        })) if viewport_text(viewport).contains("BEGIN")
            && !viewport_text(viewport).contains("END")
    ));

    let terminal = Arc::clone(
        &shared.inner.lock().clients[&client]
            .command_output
            .as_ref()
            .unwrap()
            .terminal,
    );
    assert_eq!(terminal.max_scrollback(), 64 * 1024 * 1024);
    let capture = terminal
        .capture(CaptureOptions {
            start: CaptureBoundary::HistoryStart,
            end: CaptureBoundary::Relative(i64::MAX),
            mode: true,
            ..CaptureOptions::default()
        })
        .expect("capture startup output");
    assert!(capture.contains("BEGIN"));
    assert!(capture.contains("END"));

    let view = TerminalViewId(client.0);
    terminal.view_action(view, TerminalViewAction::ScrollBottom);
    wait_for_viewport(
        &terminal,
        view,
        "startup output did not scroll to its tail",
        |viewport| viewport_text(viewport).contains("END"),
    );
}

#[test]
fn control_startup_transaction_retains_causes_when_the_attach_overflows() {
    let shared = Arc::new(Shared::new(1));
    *shared.startup_config_causes.lock() = Some(vec!["retry me".to_owned()]);
    let full = OutboundMailbox::new();
    let (first, _) = shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&full));
    let (session, _, _) = switch_test_session(&shared, "startup-retry");
    let first_snapshot = shared.attach(first, session).expect("attach first control");
    take_reliable_messages(&full);
    for _ in 0..MAX_RELIABLE_MESSAGES.saturating_sub(1) {
        assert!(full.enqueue_reliable(&Shared::event(EventPayload::ServerStopping)));
    }

    assert!(!shared.send_attached(first, &full, session, first_snapshot));
    assert_eq!(retained_startup_causes(&shared), ["retry me"]);

    let retry = OutboundMailbox::new();
    let (second, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&retry));
    let second_snapshot = shared
        .attach(second, session)
        .expect("attach second control");
    assert!(shared.send_attached(second, &retry, session, second_snapshot));
    assert_eq!(
        startup_config_cause_events(&take_reliable_messages(&retry)),
        [vec!["retry me".to_owned()]]
    );
    assert!(shared.startup_config_causes.lock().is_none());
}

#[test]
fn interactive_startup_transaction_retires_and_retries_when_the_attach_overflows() {
    let shared = Arc::new(Shared::new(1));
    *shared.startup_config_causes.lock() = Some(vec!["retry interactive".to_owned()]);
    let full = OutboundMailbox::new();
    let (first, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&full));
    let (session, _, pane) = switch_test_session(&shared, "startup-interactive-retry");
    let first_snapshot = shared
        .attach(first, session)
        .expect("attach first interactive");
    take_reliable_messages(&full);
    for _ in 0..MAX_RELIABLE_MESSAGES.saturating_sub(1) {
        assert!(full.enqueue_reliable(&Shared::event(EventPayload::ServerStopping)));
    }

    assert!(!shared.send_attached(first, &full, session, first_snapshot));
    assert_eq!(retained_startup_causes(&shared), ["retry interactive"]);
    assert!(shared.read_client(first, |c| c.is_none_or(|c| c.command_output.is_none())));

    let retry = OutboundMailbox::new();
    let (second, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&retry));
    let second_snapshot = shared
        .attach(second, session)
        .expect("attach retry interactive");
    assert!(shared.send_attached(second, &retry, session, second_snapshot));
    let messages = take_reliable_messages(&retry);
    assert!(matches!(
        messages.first(),
        Some(ProtocolMessage::Attached { .. })
    ));
    assert!(matches!(
        messages.get(1),
        Some(ProtocolMessage::Event(Event {
            payload: EventPayload::CommandOutput {
                pane: output_pane,
                viewport: Some(viewport),
                ..
            },
            ..
        })) if *output_pane == pane
            && viewport.title() == "configuration errors"
            && viewport_text(viewport).contains("retry interactive")
    ));
    assert!(retry.state.lock().command_output.is_none());
    assert!(shared.startup_config_causes.lock().is_none());
}

#[test]
fn startup_transactions_retain_causes_under_control_and_interactive_byte_pressure() {
    for kind in [ClientKind::Control, ClientKind::Interactive] {
        let (attached_bytes, causes_bytes) = startup_attach_prefix_bytes(kind);
        let shared = Arc::new(Shared::new(1));
        *shared.startup_config_causes.lock() = Some(vec!["byte pressure".to_owned()]);
        let mailbox = OutboundMailbox::new();
        let (client, _) = shared.register_subscribed(kind, None, None, Arc::clone(&mailbox));
        let (session, _, _) = switch_test_session(&shared, "startup-byte-pressure");
        let snapshot = shared
            .attach(client, session)
            .expect("attach byte-pressure client");
        take_reliable_messages(&mailbox);
        let available = attached_bytes + causes_bytes - 1;
        mailbox.state.lock().queued_bytes = MAX_OUTBOUND_BYTES - available;

        assert!(!shared.send_attached(client, &mailbox, session, snapshot));
        assert_eq!(retained_startup_causes(&shared), ["byte pressure"]);
        if kind == ClientKind::Interactive {
            assert!(shared.read_client(client, |c| c.is_none_or(|c| c.command_output.is_none())));
        }
    }
}

#[test]
fn startup_cause_delivery_is_atomic_across_control_and_interactive_clients() {
    let shared = Arc::new(Shared::new(1));
    let (session, _, _) = switch_test_session(&shared, "startup-race");
    let control_mailbox = OutboundMailbox::new();
    let interactive_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        None,
        None,
        Arc::clone(&control_mailbox),
    );
    let (interactive, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&interactive_mailbox),
    );
    shared.attach(control, session).expect("attach control");
    shared
        .attach(interactive, session)
        .expect("attach interactive");
    *shared.startup_config_causes.lock() = Some(vec!["one shot".to_owned()]);
    let barrier = Arc::new(std::sync::Barrier::new(3));

    let control_thread = {
        let shared = Arc::clone(&shared);
        let mailbox = Arc::clone(&control_mailbox);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            shared.try_deliver_startup_config_causes(control, &mailbox, false)
        })
    };
    let interactive_thread = {
        let shared = Arc::clone(&shared);
        let mailbox = Arc::clone(&interactive_mailbox);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            shared.try_deliver_startup_config_causes(interactive, &mailbox, false)
        })
    };
    barrier.wait();
    let control_delivered = control_thread.join().expect("control delivery thread");
    let interactive_delivered = interactive_thread
        .join()
        .expect("interactive delivery thread");

    assert_ne!(
        control_delivered.delivered(),
        interactive_delivered.delivered()
    );
    assert_eq!(
        startup_config_cause_events(&take_reliable_messages(&control_mailbox)).len()
            + usize::from(shared.read_client(interactive, |c| {
                c.is_some_and(|c| c.command_output.is_some())
            })),
        1
    );
    assert!(shared.startup_config_causes.lock().is_none());
}

#[test]
fn failed_startup_winner_keeps_causes_for_the_waiting_healthy_client() {
    let shared = Arc::new(Shared::new(1));
    let (session, _, pane) = switch_test_session(&shared, "startup-failed-winner");
    let failed_mailbox = OutboundMailbox::new();
    let healthy_mailbox = OutboundMailbox::new();
    let (failed, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&failed_mailbox));
    let (healthy, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        None,
        Arc::clone(&healthy_mailbox),
    );
    let failed_snapshot = shared
        .attach(failed, session)
        .expect("attach failed winner");
    let healthy_snapshot = shared
        .attach(healthy, session)
        .expect("attach healthy waiter");
    take_reliable_messages(&failed_mailbox);
    take_reliable_messages(&healthy_mailbox);
    for _ in 0..MAX_RELIABLE_MESSAGES.saturating_sub(1) {
        assert!(failed_mailbox.enqueue_reliable(&Shared::event(EventPayload::ServerStopping)));
    }
    *shared.startup_config_causes.lock() = Some(vec!["serialized retry".to_owned()]);

    let failed_state = failed_mailbox.state.lock();
    let failed_thread = {
        let shared = Arc::clone(&shared);
        let mailbox = Arc::clone(&failed_mailbox);
        thread::spawn(move || shared.send_attached(failed, &mailbox, session, failed_snapshot))
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while let Some(guard) = shared.startup_config_causes.try_lock() {
        drop(guard);
        assert!(
            Instant::now() < deadline,
            "failed winner did not claim startup causes"
        );
        thread::yield_now();
    }

    let (started_tx, started_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let healthy_thread = {
        let shared = Arc::clone(&shared);
        let mailbox = Arc::clone(&healthy_mailbox);
        thread::spawn(move || {
            started_tx.send(()).expect("signal healthy waiter");
            let delivered = shared.send_attached(healthy, &mailbox, session, healthy_snapshot);
            result_tx.send(delivered).expect("return healthy result");
        })
    };
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("healthy waiter started");
    assert!(result_rx.recv_timeout(Duration::from_millis(25)).is_err());

    drop(failed_state);
    assert!(!failed_thread.join().expect("failed winner thread"));
    assert!(
        result_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("healthy waiter result")
    );
    healthy_thread.join().expect("healthy waiter thread");

    let messages = take_reliable_messages(&healthy_mailbox);
    assert!(matches!(
        messages.get(1),
        Some(ProtocolMessage::Event(Event {
            payload: EventPayload::CommandOutput {
                pane: output_pane,
                viewport: Some(viewport),
                ..
            },
            ..
        })) if *output_pane == pane && viewport_text(viewport).contains("serialized retry")
    ));
    assert!(shared.startup_config_causes.lock().is_none());
}

#[test]
fn configuration_override_updates_state_provenance_and_broadcast() {
    let shared = Arc::new(Shared::new(44));
    let mailbox = OutboundMailbox::new();
    let (client, hello) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        Some(TerminalColorScheme::Dark),
        Arc::clone(&mailbox),
    );
    assert_eq!(
        hello
            .appearance_provenance
            .source(zz_terminal::AppearanceConfigKey::Background),
        zz_terminal::AppearanceSource::Default
    );

    shared.set_config_overrides(
        client,
        ClientKind::Interactive,
        &[("background".to_owned(), "#123456".to_owned())],
    );

    let inner = shared.inner.lock();
    assert_eq!(
        inner.config.appearance.background,
        zz_terminal::Color::rgb(0x12, 0x34, 0x56)
    );
    assert_eq!(
        inner
            .config
            .appearance_provenance
            .source(zz_terminal::AppearanceConfigKey::Background),
        zz_terminal::AppearanceSource::Override
    );
    drop(inner);
    let state = mailbox.state.lock();
    assert!(state.reliable.iter().any(|frame| {
        matches!(
            decode_protocol_frame(frame),
            Ok(ProtocolMessage::Event(Event {
                payload: EventPayload::AppearanceChanged {
                    appearance,
                    provenance,
                },
                ..
            })) if appearance.background == zz_terminal::Color::rgb(0x12, 0x34, 0x56)
                && provenance.source(zz_terminal::AppearanceConfigKey::Background)
                    == zz_terminal::AppearanceSource::Override
        )
    }));
}

#[test]
fn a_remote_client_cannot_restyle_a_machine_with_a_local_client() {
    let appearance_events = |mailbox: &OutboundMailbox| {
        take_reliable_messages(mailbox)
            .into_iter()
            .filter_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::AppearanceChanged { appearance, .. },
                    ..
                }) => Some((appearance.background, appearance.color_scheme)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let shared = Arc::new(Shared::new(46));
    let local_mailbox = OutboundMailbox::new();
    let (local, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        Some(TerminalColorScheme::Dark),
        Arc::clone(&local_mailbox),
    );
    let (remote, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        Some(TerminalColorScheme::Dark),
        OutboundMailbox::new(),
    );
    shared
        .inner
        .lock()
        .client_mut(remote)
        .expect("remote client")
        .remote = true;
    let before = Arc::clone(&shared.inner.lock().config.appearance);
    assert_eq!(before.color_scheme, TerminalColorScheme::Dark);
    take_reliable_messages(&local_mailbox);
    let overrides = [("background".to_owned(), "#123456".to_owned())];

    shared.set_config_overrides(remote, ClientKind::Interactive, &overrides);
    shared.set_client_color_scheme(remote, TerminalColorScheme::Light);
    assert_eq!(*shared.inner.lock().config.appearance, *before);
    assert_eq!(appearance_events(&local_mailbox), []);

    shared.set_config_overrides(local, ClientKind::Interactive, &overrides);
    assert_eq!(
        appearance_events(&local_mailbox),
        [(
            zz_terminal::Color::rgb(0x12, 0x34, 0x56),
            TerminalColorScheme::Dark
        )]
    );

    shared.unregister(local);
    shared.set_client_color_scheme(remote, TerminalColorScheme::Light);
    assert_eq!(
        shared.inner.lock().config.appearance.color_scheme,
        TerminalColorScheme::Light
    );
}

#[test]
fn mux_override_beats_mux_config_file_and_survives_reload_user_config() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mux_config = directory.path().join("mux.conf");
    fs::write(&mux_config, "set -g prefix C-b\nset -g mode-keys emacs\n")
        .expect("mux config fixture");

    let shared = Arc::new(Shared::new(45));
    let mailbox = OutboundMailbox::new();
    let (client, hello) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        Some(TerminalColorScheme::Dark),
        Arc::clone(&mailbox),
    );
    assert_eq!(
        hello
            .mux_options
            .get(MuxOptionKey::Prefix)
            .expect("prefix in hello")
            .source,
        MuxOptionSource::Default
    );

    let mut context = ExecutionContext::default();
    shared
        .load_config_file(&mux_config, &mut context, 0)
        .expect("initial mux config replay");
    shared.set_config_overrides(
        client,
        ClientKind::Interactive,
        &[
            ("prefix".to_owned(), "C-a".to_owned()),
            ("mode-keys".to_owned(), "vi".to_owned()),
        ],
    );
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.keys.prefix(), "C-a");
        assert_eq!(
            inner
                .config
                .mux_options
                .get(MuxOptionKey::Prefix)
                .expect("effective prefix"),
            &zz_protocol::MuxOptionValue {
                value: "C-a".to_owned(),
                source: MuxOptionSource::Override,
            }
        );
        assert_eq!(inner.engine.mux_option_value(MuxOptionKey::ModeKeys), "vi");
    }

    shared
        .reload_user_config_with_mux_file(client, &mut context, Some(&mux_config))
        .expect("reload user configuration");
    let inner = shared.inner.lock();
    assert_eq!(inner.engine.keys.prefix(), "C-a");
    assert_eq!(inner.engine.mux_option_value(MuxOptionKey::ModeKeys), "vi");
    assert_eq!(
        inner
            .config
            .mux_options
            .get(MuxOptionKey::Prefix)
            .expect("replayed prefix")
            .source,
        MuxOptionSource::Override
    );
    drop(inner);

    let state = mailbox.state.lock();
    assert!(state.reliable.iter().any(|frame| {
        matches!(
            decode_protocol_frame(frame),
            Ok(ProtocolMessage::Event(Event {
                payload: EventPayload::MuxOptionsChanged { options },
                ..
            })) if options.get(MuxOptionKey::Prefix).is_some_and(|entry| {
                entry.value == "C-a" && entry.source == MuxOptionSource::Override
            })
        )
    }));
}

#[test]
fn reload_user_config_resets_key_tables_to_the_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mux_config = directory.path().join("mux.conf");
    fs::write(&mux_config, "bind-key F2 new-window\n").expect("mux config fixture");

    let shared = Arc::new(Shared::new(49));
    let mut context = ExecutionContext::default();
    shared
        .load_config_file(&mux_config, &mut context, 0)
        .expect("source mux config");
    shared
        .execute(
            ClientId(7),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("bind-key", ["F3", "kill-pane"]),
        )
        .expect("interactive bind");
    {
        let inner = shared.inner.lock();
        assert!(inner.engine.keys.get("prefix", "F2").is_some());
        assert!(inner.engine.keys.get("prefix", "F3").is_some());
    }

    fs::write(&mux_config, "bind-key F4 new-window\n").expect("rewrite mux config");
    shared
        .reload_user_config_with_mux_file(ClientId(7), &mut context, Some(&mux_config))
        .expect("reload user configuration");
    let inner = shared.inner.lock();
    assert!(inner.engine.keys.get("prefix", "F2").is_none());
    assert!(inner.engine.keys.get("prefix", "F3").is_none());
    assert!(inner.engine.keys.get("prefix", "F4").is_some());
}

#[test]
fn sourced_mux_config_file_binds_keys_and_missing_file_is_tolerated() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mux_config = directory.path().join("mux.conf");
    fs::write(&mux_config, "set -g prefix C-a\nbind-key F2 new-window\n")
        .expect("mux config fixture");

    let shared = Arc::new(Shared::new(48));
    let mut context = ExecutionContext::default();
    shared
        .load_config_file(&mux_config, &mut context, 0)
        .expect("source mux config");
    {
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.keys.prefix(), "C-a");
        assert!(inner.engine.keys.get("prefix", "F2").is_some());
    }

    shared
        .load_config_file(&directory.path().join("missing.conf"), &mut context, 0)
        .expect("a missing mux config file loads defaults");
}

#[test]
fn nested_source_replay_keeps_the_explicit_client_base() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let client_base = directory.path().join(literal_directory_name("client cwd"));
    let entry_directory = client_base.join("a");
    fs::create_dir_all(&entry_directory).expect("nested source directory");
    fs::write(
        client_base.join("leaf.conf"),
        "set-option -g @nested-client-base root\n",
    )
    .expect("client-base leaf");
    fs::write(
        entry_directory.join("leaf.conf"),
        "set-option -g @nested-client-base decoy\n",
    )
    .expect("containing-file decoy");
    let entry = entry_directory.join("entry.conf");
    fs::write(
        &entry,
        "set-option -g @nested-replay-started yes\nsource-file leaf.conf\n",
    )
    .expect("nested source entry");

    let clientless = Arc::new(Shared::new(47));
    clientless
        .load_config_file(&entry, &mut ExecutionContext::default(), 0)
        .expect("clientless nested source replay");
    assert_eq!(
        read_global_option(&clientless, "@nested-client-base"),
        "decoy"
    );

    let shared = Arc::new(Shared::new(48));
    let mut source_invocations = SourceInvocationAccounting::default();
    shared
        .load_config_file_with_report_for_terminal(
            &entry,
            &mut ExecutionContext::default(),
            0,
            &mut ConfigLoadReport::default(),
            ClientTerminal::NoClient,
            Some(&client_base),
            &mut source_invocations,
        )
        .expect("nested source replay");

    assert_eq!(read_global_option(&shared, "@nested-replay-started"), "yes");
    assert_eq!(read_global_option(&shared, "@nested-client-base"), "root");
}

#[test]
fn attached_source_file_uses_selected_session_cwd() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let command_cwd = directory.path().join(literal_directory_name("command cwd"));
    let session_cwd = directory.path().join(literal_directory_name("session cwd"));
    let target_cwd = directory.path().join(literal_directory_name("target cwd"));
    for cwd in [&command_cwd, &session_cwd, &target_cwd] {
        fs::create_dir_all(cwd).expect("source cwd");
    }
    fs::write(
        command_cwd.join("relative.conf"),
        "set-option -g @attached-source-base command-decoy\n",
    )
    .expect("command cwd decoy");
    fs::write(
        session_cwd.join("relative.conf"),
        "set-option -g @attached-source-base session-selected\n",
    )
    .expect("session cwd source");
    fs::write(
        target_cwd.join("relative.conf"),
        "set-option -g @attached-source-base target-decoy\n",
    )
    .expect("source target decoy");

    let shared = Arc::new(Shared::new(48));
    let command = ClientId(7);
    let mut command_context = ExecutionContext::default();
    command_context.set_client_working_directory(Some(command_cwd.clone()));
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d",
                    "-c",
                    command_cwd.to_str().expect("utf-8 command cwd"),
                    "-s",
                    "attached-source",
                ],
            ),
        )
        .expect("attached source session");
    shared
        .execute(
            command,
            ClientKind::Command,
            &mut command_context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d",
                    "-c",
                    target_cwd.to_str().expect("utf-8 target cwd"),
                    "-s",
                    "source-target",
                ],
            ),
        )
        .expect("source target session");

    let mailbox = OutboundMailbox::new();
    let (interactive, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    shared
        .inner
        .lock()
        .client_entry(interactive)
        .working_directory
        .replace(command_cwd.clone());
    let mut interactive_context = ExecutionContext::default();
    shared
        .execute(
            interactive,
            ClientKind::Interactive,
            &mut interactive_context,
            &CommandInvocation::new(
                "attach-session",
                [
                    "-c",
                    session_cwd.to_str().expect("utf-8 session cwd"),
                    "-t",
                    "=attached-source",
                ],
            ),
        )
        .expect("attach with selected session cwd");
    take_reliable_messages(&mailbox);
    {
        let inner = shared.inner.lock();
        assert_eq!(
            inner
                .client(interactive)
                .and_then(|c| c.working_directory.as_ref()),
            Some(&command_cwd)
        );
        let session = inner
            .engine
            .resolve_session(Some("=attached-source"), None)
            .expect("attached source session id");
        assert_eq!(
            inner.engine.state.session_working_directory(session),
            Some(session_cwd.as_path())
        );
    }

    let response = shared.execute_command_request(
        interactive,
        ClientKind::Interactive,
        &mut interactive_context,
        1,
        &CommandInvocation::new("source-file", ["-t", "=source-target:0.0", "relative.conf"]),
    );
    assert_eq!(
        response,
        CommandResponse::Success {
            request_id: 1,
            output: RawText::default(),
            exit_code: 0,
            stderr: String::new(),
            stdout_claim: StdoutClaim::None,
        }
    );
    assert_eq!(
        read_global_option(&shared, "@attached-source-base"),
        "session-selected"
    );
}

#[test]
fn config_diagnostics_pin_the_control_mode_sniffer_wording() {
    let mut report = ConfigLoadReport::default();
    report.note_invalid_at("/tmp/mux.conf", 1, "unknown command: wibble");
    assert_eq!(
        report.message().as_deref(),
        Some("/tmp/mux.conf:1: unknown command: wibble")
    );
    assert_eq!(
        report.diagnostics(),
        ["/tmp/mux.conf:1: unknown command: wibble"]
    );

    let mut report = ConfigLoadReport::default();
    report.note_skip("focus-events");
    assert_eq!(
        report.skipped_summary().as_deref(),
        Some("skipped 1 unsupported tmux command: focus-events")
    );
    assert_eq!(report.message(), report.skipped_summary());
    report.note_skip("status-keys");
    assert_eq!(
        report.skipped_summary().as_deref(),
        Some("skipped 2 unsupported tmux commands: focus-events, status-keys")
    );

    let mut report = ConfigLoadReport::default();
    report.note_skip("focus-events");
    report.note_invalid_at("/tmp/mux.conf", 3, "unknown command: wibble");
    assert_eq!(
        report.message().as_deref(),
        Some(
            "skipped 1 unsupported tmux command: focus-events; \
                 1 invalid line: /tmp/mux.conf:3: unknown command: wibble"
        )
    );

    assert_eq!(
        source_glob_error_warning(Path::new("/tmp/["), "Pattern syntax error"),
        "Pattern syntax error: /tmp/["
    );
    assert_eq!(
        STANDARD_INPUT_SOURCE_WARNING,
        "source-file from standard input is not supported"
    );
    assert_eq!(
        missing_source_error(Path::new("/tmp/mux.conf")),
        "No such file or directory: /tmp/mux.conf"
    );
}

#[test]
fn startup_causes_obey_wire_count_item_and_aggregate_bounds() {
    let mut per_item = ConfigLoadReport::startup();
    per_item.note_startup_cause(&"界".repeat(MAX_STARTUP_CONFIG_CAUSE_BYTES));
    let causes = per_item.take_startup_causes().expect("bounded item");
    assert_eq!(causes.len(), 1);
    assert!(causes[0].len() <= MAX_STARTUP_CONFIG_CAUSE_BYTES);
    assert!(causes[0].is_char_boundary(causes[0].len()));

    let mut count = ConfigLoadReport::startup();
    for _ in 0..=MAX_STARTUP_CONFIG_CAUSES {
        count.note_startup_cause("x");
    }
    assert_eq!(
        count.take_startup_causes().expect("bounded count").len(),
        MAX_STARTUP_CONFIG_CAUSES
    );

    let mut aggregate = ConfigLoadReport::startup();
    let full = "x".repeat(MAX_STARTUP_CONFIG_CAUSE_BYTES);
    for _ in 0..=MAX_STARTUP_CONFIG_CAUSES_BYTES / MAX_STARTUP_CONFIG_CAUSE_BYTES {
        aggregate.note_startup_cause(&full);
    }
    let causes = aggregate.take_startup_causes().expect("bounded aggregate");
    assert_eq!(
        causes.iter().map(String::len).sum::<usize>(),
        MAX_STARTUP_CONFIG_CAUSES_BYTES
    );
}

#[test]
fn startup_config_preview_is_safe_utf8_and_explicitly_truncated() {
    let causes = vec![format!(
        "BEGIN\x1b\r\u{0085}\t\n{}{}END",
        "界".repeat(30_000),
        "\n".repeat(100_001)
    )];
    let preview = startup_config_preview(&causes);

    assert_eq!(
        preview,
        format!(
            "BEGIN\u{fffd}\u{fffd}\u{fffd}\t\n{}\n... startup diagnostics truncated; restart in Control mode for full output",
            "界".repeat(21_815)
        )
    );
    assert_eq!(preview.len(), MAX_STARTUP_CONFIG_PREVIEW_BYTES);
}

#[test]
fn config_report_routes_bound_command_failures_as_runtime_errors() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mux_config = directory.path().join("mux.conf");
    fs::write(
        &mux_config,
        "bind F2 new-window\nbind x not-a-command\nbind r run-shell x\nbind F3 kill-pane\n",
    )
    .expect("mux config fixture");
    let defaults = KeyTables::default();
    let default_x = defaults.get("prefix", "x").cloned();

    let shared = Arc::new(Shared::new(52));
    let mailbox = OutboundMailbox::new();
    let (client, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .reload_user_config_with_mux_file(client, &mut context, Some(&mux_config))
        .expect("reload mixed config");
    {
        let inner = shared.inner.lock();
        assert!(inner.engine.keys.get("prefix", "F2").is_some());
        assert!(inner.engine.keys.get("prefix", "F3").is_some());
        assert_eq!(inner.engine.keys.get("prefix", "x"), default_x.as_ref());
        assert_eq!(
            inner.engine.keys.get("prefix", "r").unwrap().commands,
            [CommandInvocation::new("run-shell", ["x"])]
        );
    }
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientMessage {
                    kind: ClientMessageKind::Warning,
                    text,
                    ..
                },
                ..
            }) if text == "Unknown command: not-a-command"
        )
    }));
    assert!(messages.iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientMessage {
                    kind: ClientMessageKind::Success,
                    text,
                    ..
                },
                ..
            }) if text == "Reloaded zz configuration"
        )
    }));
    assert!(!messages.iter().any(|message| {
        matches!(
            message,
            ProtocolMessage::Event(Event {
                payload: EventPayload::ClientMessage { text, .. },
                ..
            }) if text.contains("invalid line")
                || text.contains("run-shell")
                || text.contains("unsupported tmux command")
        )
    }));
}

#[test]
fn config_report_keeps_top_level_unknown_command_source_line() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mux_config = directory.path().join("mux.conf");
    fs::write(&mux_config, "wibble\n").expect("mux config fixture");
    let shared = Arc::new(Shared::new(52));
    let mut context = ExecutionContext::default();
    let mut report = ConfigLoadReport::default();

    shared
        .load_config_file_with_report(&mux_config, &mut context, 0, &mut report)
        .expect("load invalid config");

    assert_eq!(
        report.message(),
        Some(format!(
            "{}:1: unknown command: wibble",
            mux_config.display()
        ))
    );
}

#[test]
fn invalid_mux_override_is_diagnostic_and_later_entries_still_apply() {
    let shared = Arc::new(Shared::new(46));
    let report = shared.apply_mux_config_overrides(
        &[
            ("mode-keys".to_owned(), "not-a-mode".to_owned()),
            ("prefix".to_owned(), "C-a".to_owned()),
        ],
        "test-invalid",
    );

    assert_eq!(report.applied, 1);
    assert_eq!(report.diagnostics.len(), 1);
    assert!(report.diagnostics[0].contains("invalid mode-keys value"));
    let inner = shared.inner.lock();
    assert_eq!(inner.engine.keys.prefix(), "C-a");
    assert_eq!(
        inner.engine.mux_option_value(MuxOptionKey::ModeKeys),
        "emacs"
    );
}

#[test]
fn mux_option_source_tracks_default_tmux_override_and_runtime_writers() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let tmux_config = directory.path().join("source-tiers.conf");
    fs::write(&tmux_config, "set -g prefix C-c\n").expect("tmux fixture");
    let shared = Arc::new(Shared::new(47));
    let mut context = ExecutionContext::default();

    assert_eq!(
        shared
            .inner
            .lock()
            .config
            .mux_options
            .get(MuxOptionKey::Prefix)
            .expect("default prefix")
            .source,
        MuxOptionSource::Default
    );
    shared
        .load_config_file(&tmux_config, &mut context, 0)
        .expect("tmux replay");
    assert_eq!(
        shared
            .inner
            .lock()
            .config
            .mux_options
            .get(MuxOptionKey::Prefix)
            .expect("tmux prefix")
            .source,
        MuxOptionSource::TmuxConfig
    );

    shared.apply_mux_config_overrides(
        &[("prefix".to_owned(), "C-a".to_owned())],
        "test-source-tier",
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .config
            .mux_options
            .get(MuxOptionKey::Prefix)
            .expect("override prefix")
            .source,
        MuxOptionSource::Override
    );

    shared
        .execute(
            ClientId(9),
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "prefix", "C-z"]),
        )
        .expect("runtime prefix command");
    let inner = shared.inner.lock();
    let runtime = inner
        .config
        .mux_options
        .get(MuxOptionKey::Prefix)
        .expect("runtime prefix");
    assert_eq!(runtime.value, "C-z");
    assert_eq!(runtime.source, MuxOptionSource::RuntimeCommand);
}
