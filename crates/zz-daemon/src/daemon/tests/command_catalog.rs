use super::*;

#[test]
fn daemon_command_catalog_matches_preemption_dispatch() {
    let names = zz_mux::CommandSpec::DAEMON_COMMAND_NAMES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let dispatches = DAEMON_COMMAND_DISPATCHES
        .iter()
        .map(|(name, _)| *name)
        .collect::<BTreeSet<_>>();
    assert_eq!(zz_mux::CommandSpec::DAEMON_COMMAND_NAMES.len(), names.len());
    assert_eq!(DAEMON_COMMAND_DISPATCHES.len(), dispatches.len());
    assert_eq!(names, dispatches);
    for name in dispatches {
        let canonical = canonical_command(name);
        assert_eq!(
            zz_protocol::catalog_command_spec(name).map(|spec| spec.name),
            Some(canonical),
            "daemon dispatch {name} has no shared spec"
        );
    }
    for name in ["new-session", "capture-pane-extra"] {
        assert!(daemon_command_dispatch(name).is_none());
    }
    for (prefix, dispatch) in [
        ("wait-pane", DaemonCommandDispatch::WaitPane),
        ("run-pane", DaemonCommandDispatch::RunPane),
        ("wait", DaemonCommandDispatch::WaitFor),
        ("run", DaemonCommandDispatch::RunShell),
        ("capture-pan", DaemonCommandDispatch::CapturePane),
        ("run-sh", DaemonCommandDispatch::RunShell),
        ("if-sh", DaemonCommandDispatch::IfShell),
        ("list-buf", DaemonCommandDispatch::Buffer),
        ("pipe-pa", DaemonCommandDispatch::PipePane),
        ("display-pop", DaemonCommandDispatch::DisplayPopup),
    ] {
        assert_eq!(daemon_command_dispatch(prefix), Some(dispatch), "{prefix}");
    }
}

#[test]
fn tmux_option_preflight_covers_daemon_commands_and_display_panes() {
    let daemon = zz_protocol::command_specs()
        .filter(|spec| spec.uses_tmux_option_grammar())
        .filter(|spec| daemon_command_dispatch(spec.name).is_some())
        .collect::<Vec<_>>();
    assert_eq!(daemon.len(), 24);
    assert!(daemon.iter().all(|spec| spec.name != "display-panes"));
    let display_panes =
        zz_protocol::catalog_command_spec("display-panes").expect("display-panes command spec");
    assert!(display_panes.uses_tmux_option_grammar());
    assert!(daemon_command_dispatch(display_panes.name).is_none());
    assert!(
        daemon
            .iter()
            .copied()
            .chain(std::iter::once(display_panes))
            .flat_map(|spec| spec.options)
            .all(|option| option.unsupported || !option.optional_value)
    );
}

#[test]
fn every_upstream_command_spelling_has_exact_flag_diagnostics_before_state_or_effects() {
    let shared = Arc::new(Shared::new(1));
    switch_test_session(&shared, "preserved");
    let (snapshot, keys) = {
        let inner = shared.inner.lock();
        (inner.engine.state.snapshot(), inner.engine.keys.snapshot())
    };
    let parse_error = |spelling: &str, arguments: Vec<String>| {
        let error = shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(spelling, arguments),
            )
            .expect_err("flag syntax must not return an execution");
        let DaemonError::Server(error) = error else {
            panic!("{spelling} returned a non-server error: {error:?}");
        };
        assert!(error.is_command_parse(), "{spelling}: {error:?}");
        let inner = shared.inner.lock();
        assert_eq!(inner.engine.state.snapshot(), snapshot, "{spelling}");
        assert_eq!(inner.engine.keys.snapshot(), keys, "{spelling}");
        assert!(inner.attachment.attached.is_empty(), "{spelling}");
        assert!(inner.buffers.paste_buffers.is_empty(), "{spelling}");
        assert!(
            inner.clients.values().all(|c| c.choose_tree.is_none()),
            "{spelling}"
        );
        assert!(
            inner.clients.values().all(|c| c.choose_buffer.is_none()),
            "{spelling}"
        );
        assert!(
            inner.clients.values().all(|c| c.display_panes.is_none()),
            "{spelling}"
        );
        assert!(
            inner.clients.values().all(|c| c.menu.is_none()),
            "{spelling}"
        );
        assert!(
            inner.clients.values().all(|c| c.confirm.is_none()),
            "{spelling}"
        );
        error
    };

    let specs = zz_protocol::command_specs()
        .filter(|spec| spec.uses_tmux_option_grammar())
        .collect::<Vec<_>>();
    assert_eq!(specs.len(), 90);
    assert_eq!(
        specs.iter().map(|spec| spec.aliases.len()).sum::<usize>(),
        76
    );
    let mut spellings = 0;
    let mut diagnostic_cases = 0;
    let mut required_cases = 0;
    for spec in &specs {
        let unknown = ('0'..='9')
            .chain('A'..='Z')
            .chain('a'..='z')
            .map(|flag| format!("-{flag}"))
            .find(|option| spec.option(option).is_none())
            .expect("every command has an unknown alphanumeric flag");
        for spelling in std::iter::once(spec.name).chain(spec.aliases.iter().copied()) {
            spellings += 1;
            for (arguments, expected) in [
                (
                    vec![unknown.clone()],
                    format!("command {}: unknown flag {unknown}", spec.name),
                ),
                (
                    vec!["-@".to_owned()],
                    format!("command {}: invalid flag -@", spec.name),
                ),
                (
                    vec!["--bogus".to_owned()],
                    format!("command {}: invalid flag --", spec.name),
                ),
                (
                    vec!["-?".to_owned()],
                    format!("usage: {} {}", spec.name, spec.pinned_tmux_usage()),
                ),
            ] {
                diagnostic_cases += 1;
                assert_eq!(
                    parse_error(spelling, arguments),
                    ServerError::CommandParse(expected),
                    "{spelling}"
                );
            }
            for option in spec.options.iter().filter(|option| {
                !option.optional_value && (option.value.is_some() || option.attached_value)
            }) {
                required_cases += 1;
                let message = format!("command {}: {} expects an argument", spec.name, option.name);
                let expected = if option.native {
                    ServerError::NativeCommandParse(message)
                } else {
                    ServerError::CommandParse(message)
                };
                assert_eq!(
                    parse_error(spelling, vec![option.name.to_owned()]),
                    expected,
                    "{spelling} {}",
                    option.name
                );
            }
        }
    }
    assert_eq!(spellings, 166);
    assert_eq!(diagnostic_cases, 664);
    assert_eq!(required_cases, 504);

    let mut prefix_cases = 0;
    for spec in &specs {
        for end in 1..spec.name.len() {
            let prefix = &spec.name[..end];
            if zz_protocol::catalog_command_spec(prefix).is_some()
                || !matches!(
                    zz_protocol::resolve_command(prefix),
                    zz_protocol::CommandResolution::Canonical(name) if name == spec.name
                )
            {
                continue;
            }
            prefix_cases += 1;
            assert_eq!(
                parse_error(prefix, vec!["-0".to_owned()]),
                ServerError::CommandParse(format!("command {}: unknown flag -0", spec.name)),
                "{prefix}"
            );
        }
    }
    assert_eq!(prefix_cases, 569);

    for spec in &specs {
        let unknown = ('0'..='9')
            .chain('A'..='Z')
            .chain('a'..='z')
            .map(|flag| format!("-{flag}"))
            .find(|option| spec.option(option).is_none())
            .expect("every command has an unknown alphanumeric flag");
        for option in spec.options.iter().filter(|option| option.unsupported) {
            let mut arguments = if option.optional_value {
                vec![format!("{}1", option.name)]
            } else if option.value.is_some() || option.attached_value {
                vec![option.name.to_owned(), "value".to_owned()]
            } else {
                vec![option.name.to_owned()]
            };
            arguments.push(unknown.clone());
            assert_eq!(
                parse_error(spec.name, arguments),
                ServerError::CommandParse(format!("command {}: unknown flag {unknown}", spec.name)),
                "{} {}",
                spec.name,
                option.name
            );
        }
    }
    assert_eq!(
        parse_error(
            "capturep",
            ["-C", "-Z"].map(str::to_owned).into_iter().collect()
        ),
        ServerError::CommandParse("command capture-pane: unknown flag -Z".to_owned())
    );
    assert_eq!(
        parse_error("displayp", vec!["-@".to_owned()]),
        ServerError::CommandParse("command display-panes: invalid flag -@".to_owned())
    );
}

#[test]
fn catalogued_positional_maximums_precede_daemon_targets_and_effects() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let output = directory.path().join("must-not-exist");
    let argument = output.to_string_lossy().into_owned();

    let shared = Arc::new(Shared::new(1));
    shared
        .buffer_command(
            &ExecutionContext::default(),
            "set-buffer",
            &["-b", "source", "alpha"].map(RawText::from),
        )
        .expect("source buffer");

    let specs = zz_protocol::command_specs()
        .filter(|spec| !zz_protocol::NATIVE_COMMAND_NAMES.contains(&spec.name))
        .filter(|spec| spec.positional_maximum().is_some())
        .collect::<Vec<_>>();
    assert_eq!(specs.len(), 78);
    for spec in specs {
        let maximum = spec.positional_maximum().expect("finite maximum");
        let arguments = vec![argument.clone(); maximum.saturating_add(1)];
        for spelling in std::iter::once(spec.name).chain(spec.aliases.iter().copied()) {
            let error = shared
                .execute(
                    ClientId(u64::MAX),
                    ClientKind::Command,
                    &mut ExecutionContext::default(),
                    &CommandInvocation::new(spelling, arguments.clone()),
                )
                .expect_err("positional maximum");
            let DaemonError::Server(error) = error else {
                panic!("unexpected error for {spelling}: {error:?}");
            };
            assert_eq!(
                error,
                ServerError::CommandParse(format!(
                    "command {}: too many arguments (need at most {maximum})",
                    spec.name
                )),
                "{spelling}"
            );
        }
    }
    for (spelling, canonical, arguments, maximum) in [
        (
            "capturep",
            "capture-pane",
            vec!["-C".to_owned(), argument.clone()],
            0,
        ),
        (
            "loadb",
            "load-buffer",
            vec!["-w".to_owned(), argument.clone(), argument.clone()],
            1,
        ),
    ] {
        let error = shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(spelling, arguments),
            )
            .expect_err("positional maximum before parked capability");
        let DaemonError::Server(error) = error else {
            panic!("unexpected error for {spelling}: {error:?}");
        };
        assert_eq!(
            error,
            ServerError::CommandParse(format!(
                "command {canonical}: too many arguments (need at most {maximum})"
            ))
        );
    }

    assert!(!output.exists());
    let inner = shared.inner.lock();
    assert!(inner.engine.state.sessions.is_empty());
    assert!(inner.clients.values().all(|c| c.choose_buffer.is_none()));
    assert!(inner.clients.values().all(|c| c.display_panes.is_none()));
    assert_eq!(inner.buffers.paste_buffers.len(), 1);
    assert_eq!(inner.buffers.paste_buffers[0].name, "source");
    assert_eq!(inner.buffers.paste_buffers[0].data.as_ref(), b"alpha");
}

#[test]
fn catalogued_positional_minimums_precede_daemon_targets_and_effects() {
    let shared = Arc::new(Shared::new(1));
    let cases = vec![
        ("bind", "bind-key", vec![], 1),
        ("confirm", "confirm-before", vec!["-t", "missing"], 1),
        (
            "menu",
            "display-menu",
            vec!["-c", "missing", "-t", "=missing"],
            1,
        ),
        ("findw", "find-window", vec!["-t", "=missing"], 1),
        ("if", "if-shell", vec!["-t", "=missing", "condition"], 2),
        ("loadb", "load-buffer", vec!["-w"], 1),
        ("rename", "rename-session", vec!["-t", "=missing"], 1),
        ("renamew", "rename-window", vec!["-t", "=missing"], 1),
        ("saveb", "save-buffer", vec![], 1),
        ("setenv", "set-environment", vec!["-t", "=missing"], 1),
        ("set", "set-option", vec!["-t", "=missing"], 1),
        ("setw", "set-window-option", vec!["-t", "=missing"], 1),
        ("source", "source-file", vec!["-t", "=missing"], 1),
        ("wait", "wait-for", vec![], 1),
    ];
    for (spelling, canonical, arguments, minimum) in cases {
        let error = shared
            .execute(
                ClientId(u64::MAX),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &CommandInvocation::new(spelling, arguments),
            )
            .expect_err("positional minimum");
        let DaemonError::Server(error) = error else {
            panic!("unexpected error: {error:?}");
        };
        assert_eq!(
            error,
            ServerError::CommandParse(format!(
                "command {canonical}: too few arguments (need at least {minimum})"
            ))
        );
    }

    let inner = shared.inner.lock();
    assert!(inner.engine.state.sessions.is_empty());
    assert!(inner.clients.values().all(|c| c.menu.is_none()));
    assert!(inner.clients.values().all(|c| c.confirm.is_none()));
    assert!(inner.waits.wait_channels.is_empty());
    assert!(inner.buffers.paste_buffers.is_empty());
}
