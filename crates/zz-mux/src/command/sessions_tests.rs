use super::sessions::*;
use super::test_support::command;
use super::*;

#[test]
fn creates_navigates_and_removes_mux_resources() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    let created = engine
        .execute(&mut context, &command("new", &["-s", "work"]))
        .unwrap();
    assert!(matches!(created.effects[0], MuxEffect::PaneCreated { .. }));
    let first = context.pane.unwrap();
    engine
        .execute(&mut context, &command("splitw", &["-h"]))
        .unwrap();
    let second = context.pane.unwrap();
    assert_ne!(first, second);
    assert!(engine.state.validate().is_ok());

    engine
        .execute(&mut context, &command("selectp", &["-L"]))
        .unwrap();
    assert_eq!(context.pane, Some(first));

    engine
        .execute(
            &mut context,
            &command("selectp", &["-t", &first.to_string()]),
        )
        .unwrap();
    assert_eq!(context.pane, Some(first));
    engine
        .execute(
            &mut context,
            &command("killp", &["-t", &second.to_string()]),
        )
        .unwrap();
    assert!(engine.state.pane(second).is_none());
    assert!(engine.state.validate().is_ok());
}

#[test]
fn new_session_requests_attachment_after_creating_its_terminal() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    let execution = engine
        .execute(&mut context, &command("new-session", &[]))
        .expect("new session");
    let session = context.session.expect("new session id");
    let pane = context.pane.expect("new pane id");

    assert!(matches!(
        execution.effects.as_slice(),
        [
            MuxEffect::PaneCreated { pane: created, .. },
            MuxEffect::Attach {
                session: attached,
                detach_others: false,
                detach_others_hangup: false,
                read_only: false,
                flags: None,
                update_environment: false,
            },
            MuxEffect::SnapshotChanged,
        ] if *created == pane && *attached == session
    ));
}

#[test]
fn new_session_working_directory_prefers_explicit_attached_then_caller() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    context.set_client_working_directory(Some(PathBuf::from("/caller")));

    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "explicit", "-c", "/explicit"]),
        )
        .unwrap();
    let explicit = context.session.unwrap();
    assert_eq!(
        engine.state.session_working_directory(explicit),
        Some(Path::new("/explicit"))
    );

    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "attached"]),
        )
        .unwrap();
    let attached = context.session.unwrap();
    assert_eq!(
        engine.state.session_working_directory(attached),
        Some(Path::new("/explicit"))
    );

    context.set_client_attached(false);
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "caller"]),
        )
        .unwrap();
    let caller = context.session.unwrap();
    assert_eq!(
        engine.state.session_working_directory(caller),
        Some(Path::new("/caller"))
    );

    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "override", "-c", "/override"]),
        )
        .unwrap();
    let overridden = context.session.unwrap();
    assert_eq!(
        engine.state.session_working_directory(overridden),
        Some(Path::new("/override"))
    );
}

#[test]
fn new_session_cwd_expands_storage_and_spawn_against_distinct_targets() {
    let mut engine = MuxEngine::default();
    let (first, first_window, first_pane) = engine.state.create_session("first").unwrap();
    let mut context = ExecutionContext::new(None, None, None);
    context.set_attached_client_context(Some((first, first_window, first_pane)));
    context.set_format_client(FormatClient::NoClient);

    let attached = engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-s", "attached", "-c", "#{session_active}"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .state
            .session_working_directory(context.session.unwrap()),
        Some(Path::new("1"))
    );
    assert!(attached.effects.iter().any(|effect| {
        matches!(effect, MuxEffect::PaneCreated { cwd: Some(cwd), .. } if cwd == "0")
    }));

    context.set_client_attached(false);
    context.set_format_client(FormatClient::Attached(first));
    let unattached = engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-s", "unattached", "-c", "#{session_active}"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .state
            .session_working_directory(context.session.unwrap()),
        Some(Path::new(""))
    );
    assert!(unattached.effects.iter().any(|effect| {
        matches!(effect, MuxEffect::PaneCreated { cwd: Some(cwd), .. } if cwd == "0")
    }));

    context.set_no_client();
    context.set_format_client(FormatClient::Attached(first));
    let clientless = engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-s", "clientless", "-c", "#{session_active}"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .state
            .session_working_directory(context.session.unwrap()),
        Some(Path::new(""))
    );
    assert!(
        clientless
            .effects
            .iter()
            .any(|effect| { matches!(effect, MuxEffect::PaneCreated { cwd: None, .. }) })
    );
}

#[test]
fn new_session_attach_or_create_cwd_uses_the_target_once_and_honors_inert_guards() {
    struct TrackingHooks {
        calls: usize,
    }

    impl StatusHooks for TrackingHooks {
        fn strftime(&mut self, literal: &str) -> String {
            literal.to_owned()
        }

        fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
            String::new()
        }

        fn variable(&mut self, name: &str, _context: &StatusContext) -> Option<String> {
            (name == "cwd_probe").then(|| {
                self.calls += 1;
                self.calls.to_string()
            })
        }
    }

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "source", "-c", "/source"]),
        )
        .unwrap();
    let source = context.session.unwrap();
    let source_pane = context.pane.unwrap();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "target", "-c", "/target"]),
        )
        .unwrap();
    let target = context.session.unwrap();
    let target_pane = context.pane.unwrap();
    assert!(engine.set_pane_runtime_facts(
        target_pane,
        PaneRuntimeFacts {
            current_path: "/target-pane".to_owned(),
            ..PaneRuntimeFacts::default()
        },
    ));

    context.retarget(&ExecutionContext::for_pane(&engine.state, source_pane).unwrap());
    let mut hooks = TrackingHooks { calls: 0 };
    let execution = engine
        .execute_with_format_hooks(
            &mut context,
            &command(
                "new-session",
                &[
                    "-A",
                    "-s",
                    "target",
                    "-c",
                    "/#{session_name}/#{pane_id}/#{pane_current_path}/#{cwd_probe}",
                ],
            ),
            &mut hooks,
        )
        .unwrap();
    assert_eq!(hooks.calls, 1);
    let expected_target_cwd = format!("/target/{target_pane}//target-pane/1");
    assert_eq!(
        engine.state.session_working_directory(target),
        Some(Path::new(&expected_target_cwd))
    );
    assert_eq!(
        engine.state.session_working_directory(source),
        Some(Path::new("/source"))
    );
    assert!(matches!(
        execution.effects.as_slice(),
        [MuxEffect::Attach { session, .. }, MuxEffect::SnapshotChanged]
            if *session == target
    ));

    context.retarget(&ExecutionContext::for_pane(&engine.state, source_pane).unwrap());
    context.set_refuse_new_session_attach(true);
    assert_eq!(
        engine
            .execute_with_format_hooks(
                &mut context,
                &command(
                    "new-session",
                    &["-A", "-s", "target", "-c", "/blocked/#{cwd_probe}"],
                ),
                &mut hooks,
            )
            .unwrap_err(),
        ServerError::InvalidCommand(
            "sessions should be nested with care, unset $TMUX to force".to_owned()
        )
    );
    assert_eq!(hooks.calls, 1);
    assert_eq!(context.session, Some(source));
    assert_eq!(
        engine.state.session_working_directory(target),
        Some(Path::new(&expected_target_cwd))
    );

    context.set_refuse_new_session_attach(false);
    context.set_no_client();
    let inert = engine
        .execute_with_format_hooks(
            &mut context,
            &command(
                "new-session",
                &["-A", "-s", "target", "-c", "/clientless/#{cwd_probe}"],
            ),
            &mut hooks,
        )
        .unwrap();
    assert!(inert.effects.is_empty());
    assert_eq!(hooks.calls, 1);
    assert_eq!(context.session, Some(source));
}

#[test]
fn new_session_empty_cwd_is_session_state_but_not_a_pane_spawn_override() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "source", "-c", "/source"]),
        )
        .unwrap();
    let source = context.session.unwrap();
    let source_pane = context.pane.unwrap();

    let mut fresh_context = ExecutionContext::for_pane(&engine.state, source_pane).unwrap();
    let fresh = engine
        .execute(
            &mut fresh_context,
            &command(
                "new-session",
                &["-d", "-s", "fresh-empty", "-c", "#{missing}"],
            ),
        )
        .unwrap();
    let fresh_session = fresh_context.session.unwrap();
    assert_eq!(
        engine.state.session_working_directory(fresh_session),
        Some(Path::new(""))
    );
    assert!(matches!(
        fresh.effects.first(),
        Some(MuxEffect::PaneCreated {
            inherit_cwd_from: Some(donor),
            cwd: None,
            ..
        }) if *donor == source_pane
    ));

    let mut miss_context = ExecutionContext::for_pane(&engine.state, source_pane).unwrap();
    let miss = engine
        .execute(
            &mut miss_context,
            &command(
                "new-session",
                &["-A", "-d", "-s", "miss-empty", "-c", "#{missing}"],
            ),
        )
        .unwrap();
    let miss_session = miss_context.session.unwrap();
    assert_eq!(
        engine.state.session_working_directory(miss_session),
        Some(Path::new(""))
    );
    assert!(matches!(
        miss.effects.first(),
        Some(MuxEffect::PaneCreated {
            inherit_cwd_from: Some(donor),
            cwd: None,
            ..
        }) if *donor == source_pane
    ));

    let mut omitted_context = ExecutionContext::for_pane(&engine.state, source_pane).unwrap();
    engine
        .execute(
            &mut omitted_context,
            &command("new-session", &["-d", "-s", "omitted"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .state
            .session_working_directory(omitted_context.session.unwrap()),
        engine.state.session_working_directory(source)
    );
}

#[test]
fn attach_session_working_directory_uses_target_context_and_isolation() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "source", "-c", "/source"]),
        )
        .unwrap();
    let source = context.session.unwrap();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "target", "-c", "/target"]),
        )
        .unwrap();
    let target = context.session.unwrap();
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "=target:1", "-n", "selected"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("split-window", &["-d", "-t", "=target:1.0"]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("attach-session", &["-t", "=source"]))
        .unwrap();
    let selected_pane = engine
        .resolve_pane(Some("=target:1.1"), context.window, context.pane)
        .unwrap();
    let selected_window = engine.state.window_for_pane(selected_pane).unwrap();

    let execution = engine
        .execute(
            &mut context,
            &command(
                "attach-session",
                &[
                    "-t",
                    "=target:1.1",
                    "-c",
                    "/selected/#{session_name}-#{window_index}-#{pane_index}",
                ],
            ),
        )
        .unwrap();

    assert_eq!(
        engine.state.session_working_directory(source),
        Some(Path::new("/source"))
    );
    assert_eq!(
        engine.state.session_working_directory(target),
        Some(Path::new("/selected/target-1-1"))
    );
    assert_eq!(
        engine.state.sessions[&target].active_window,
        selected_window
    );
    assert_eq!(
        engine.state.windows[&selected_window].active_pane,
        selected_pane
    );
    assert_eq!(context.window, Some(selected_window));
    assert_eq!(context.pane, Some(selected_pane));
    assert!(matches!(
        execution.effects.as_slice(),
        [
            MuxEffect::Attach {
                session,
                detach_others: false,
                detach_others_hangup: false,
                read_only: false,
                flags: None,
                update_environment: true,
            },
            MuxEffect::SnapshotChanged,
        ] if *session == target
    ));
}

#[test]
fn attach_session_working_directory_and_flags_follow_target_resolution() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "target", "-c", "/original"]),
        )
        .unwrap();
    let target = context.session.unwrap();
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "=target:1", "-n", "selected"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("split-window", &["-d", "-t", "=target:1.0"]),
        )
        .unwrap();
    let selected_pane = engine
        .resolve_pane(Some("=target:1.1"), context.window, context.pane)
        .unwrap();
    let selected_window = engine.state.window_for_pane(selected_pane).unwrap();
    context.set_client_terminal(false);
    let generation = engine.state.generation();

    let execution = engine
        .execute(
            &mut context,
            &command(
                "attach-session",
                &["-t", "=target:1.1", "-c", "/changed", "-f", "ignore-size"],
            ),
        )
        .unwrap();
    assert!(matches!(
        execution.effects.as_slice(),
        [
            MuxEffect::Attach {
                session,
                detach_others: false,
                detach_others_hangup: false,
                read_only: false,
                flags: Some(flags),
                update_environment: true,
            },
            MuxEffect::SnapshotChanged,
        ] if *session == target && flags == "ignore-size"
    ));
    assert_eq!(
        engine.state.session_working_directory(target),
        Some(Path::new("/changed"))
    );
    assert_eq!(
        engine.state.sessions[&target].active_window,
        selected_window
    );
    assert_eq!(
        engine.state.windows[&selected_window].active_pane,
        selected_pane
    );
    assert_eq!(context.session, Some(target));
    assert_eq!(context.window, Some(selected_window));
    assert_eq!(context.pane, Some(selected_pane));
    assert!(engine.state.generation() > generation);

    let generation = engine.state.generation();
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "attach-session",
                &["-t", "=missing", "-c", "/missing"],
            ),
        ),
        Err(ServerError::SessionNotFound(target)) if target == "missing"
    ));
    assert_eq!(
        engine.state.session_working_directory(target),
        Some(Path::new("/changed"))
    );
    assert_eq!(engine.state.generation(), generation);
}

#[test]
fn attaching_commands_keep_only_the_last_requested_flag_mutation() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "target"]),
        )
        .unwrap();
    let session = context.session.unwrap();

    let attached = engine
        .execute(
            &mut context,
            &command(
                "attach-session",
                &["-t", "target", "-f", "ignore-size", "-f", "read-only"],
            ),
        )
        .unwrap();
    assert_eq!(
        attached.effects,
        [MuxEffect::Attach {
            session,
            detach_others: false,
            detach_others_hangup: false,
            read_only: false,
            flags: Some("read-only".to_owned()),
            update_environment: true,
        }]
    );

    let created = engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-s", "fresh", "-f", "read-only", "-f", "ignore-size"],
            ),
        )
        .unwrap();
    assert!(matches!(
        created.effects.as_slice(),
        [
            MuxEffect::PaneCreated { .. },
            MuxEffect::Attach {
                flags: Some(flags),
                ..
            },
            MuxEffect::SnapshotChanged,
        ] if flags == "ignore-size"
    ));
}

#[test]
fn detached_new_session_does_not_request_attachment() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    let execution = engine
        .execute(&mut context, &command("new-session", &["-d"]))
        .expect("detached new session");
    let pane = context.pane.expect("new pane id");

    assert!(matches!(
        execution.effects.as_slice(),
        [
            MuxEffect::PaneCreated { pane: created, .. },
            MuxEffect::SnapshotChanged,
        ] if *created == pane
    ));
}

#[test]
fn clientless_new_session_forces_detached_creation() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext {
        client_terminal: ClientTerminal::NoClient,
        ..ExecutionContext::default()
    };

    let execution = engine
        .execute(&mut context, &command("new-session", &["-s", "fromconfig"]))
        .expect("clientless new session");
    let pane = context.pane.expect("new pane id");

    assert_eq!(
        execution.effects,
        [
            MuxEffect::PaneCreated {
                pane,
                kind: PaneKindSnapshot::Terminal,
                inherit_cwd_from: None,
                cwd: None,
                command: None,
                environment: Vec::new(),
                empty: false,
            },
            MuxEffect::SnapshotChanged,
        ]
    );
    assert!(session_named(&engine.state, "fromconfig").is_some());
}

#[test]
fn clientless_attach_commands_are_silent_noops() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "base"]))
        .expect("base session");
    let session = context.session;
    context.client_terminal = ClientTerminal::NoClient;

    let new_session = engine
        .execute(&mut context, &command("new-session", &["-A", "-s", "base"]))
        .expect("clientless new-session -A");
    assert!(new_session.output.is_empty());
    assert!(new_session.effects.is_empty());
    assert_eq!(context.session, session);

    let attach = engine
        .execute(&mut context, &command("attach-session", &["-t", "bogus"]))
        .expect("clientless attach-session");
    assert!(attach.output.is_empty());
    assert!(attach.effects.is_empty());
    assert_eq!(context.session, session);
}

#[test]
fn new_session_check_order_matches_terminal_duplicate_and_size_rules() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext {
        client_terminal: ClientTerminal::Absent,
        ..ExecutionContext::default()
    };

    assert!(matches!(
        engine.execute(
            &mut context,
            &command("new-session", &["-s", "missing-terminal"]),
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "open terminal failed: not a terminal"
    ));
    assert!(engine.state.sessions.is_empty());

    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "duplicate"]),
        )
        .expect("detached session");
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("new-session", &["-s", "duplicate", "-x", "0"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "duplicate session: duplicate"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("new-session", &["-s", "fresh-size", "-x", "0"]),
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "open terminal failed: not a terminal"
    ));
    assert!(matches!(
        engine
            .execute(
                &mut context,
                &command("new-session", &["-A", "-d", "-s", "duplicate"]),
            )
            .unwrap()
            .effects
            .as_slice(),
        [MuxEffect::Attach { .. }]
    ));
    assert!(matches!(
        engine
            .execute(
                &mut context,
                &command("attach-session", &["-t", "duplicate"]),
            )
            .unwrap()
            .effects
            .as_slice(),
        [MuxEffect::Attach { .. }]
    ));

    engine
        .execute(
            &mut context,
            &command("new-session", &["-A", "-d", "-s", "detached"]),
        )
        .expect("fresh -A -d remains detached");
    context.client_terminal = ClientTerminal::Present;
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("new-session", &["-s", "bad-width", "-x", "0"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "width too small"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("new-session", &["-s", "bad-height", "-y", "0"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "height too small"
    ));
    assert_eq!(engine.state.sessions.len(), 2);
}

#[test]
fn new_session_name_expands_once_before_attach_and_duplicate_lookup() {
    struct Once {
        calls: usize,
        order: usize,
    }

    impl StatusHooks for Once {
        fn strftime(&mut self, literal: &str) -> String {
            literal.to_owned()
        }

        fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
            String::new()
        }

        fn variable(&mut self, name: &str, _context: &StatusContext) -> Option<String> {
            match name {
                "once" => {
                    self.calls += 1;
                    Some(if self.calls == 1 { "once" } else { "twice" }.to_owned())
                }
                "order" => {
                    self.order += 1;
                    Some(self.order.to_string())
                }
                _ => None,
            }
        }
    }

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-s", "origin", "-n", "origin-window"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("select-pane", &["-T", "origin-pane"]),
        )
        .unwrap();

    let mut hooks = Once { calls: 0, order: 0 };
    engine
        .execute_with_format_hooks(
            &mut context,
            &command(
                "new",
                &[
                    "-d",
                    "-n",
                    "created-window-#{order}-#{command}",
                    "-s",
                    "#{session_name}-#{window_name}-#{pane_title}-#{once}-#{order}-#{command}",
                ],
            ),
            &mut hooks,
        )
        .unwrap();
    assert_eq!(hooks.calls, 1);
    assert_eq!(hooks.order, 2);
    let name = "origin-origin-window-origin-pane-once-2-new-session";
    let created = session_named(&engine.state, name).unwrap();
    assert_eq!(context.session, Some(created));
    let created_window = engine.state.sessions[&created].active_window;
    assert_eq!(
        engine.state.windows[&created_window].name,
        "created-window-1-new-session"
    );

    let attached = engine
        .execute(
            &mut context,
            &command("new-session", &["-A", "-d", "-s", "#{session_name}"]),
        )
        .unwrap();
    assert!(
        attached.effects.iter().any(
            |effect| matches!(effect, MuxEffect::Attach { session, .. } if *session == created)
        )
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("new-session", &["-d", "-s", "#{session_name}"]),
        ),
        Err(ServerError::InvalidCommand(message))
            if message == format!("duplicate session: {name}")
    ));
}

#[test]
fn new_session_name_uses_canonical_and_explicit_command_item_values() {
    for spelling in ["new-session", "new", "new-s"] {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(
                &mut context,
                &command(
                    spelling,
                    &["-d", "-n", "window-#{command}", "-s", "#{command}"],
                ),
            )
            .unwrap();
        let session = session_named(&engine.state, "new-session").unwrap();
        assert_eq!(
            engine.state.windows[&engine.state.sessions[&session].active_window].name,
            "window-new-session"
        );
    }

    struct ExplicitCommand;

    impl StatusHooks for ExplicitCommand {
        fn strftime(&mut self, literal: &str) -> String {
            literal.to_owned()
        }

        fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
            String::new()
        }

        fn variable(&mut self, name: &str, _context: &StatusContext) -> Option<String> {
            (name == "command").then(|| "explicit-item".to_owned())
        }
    }

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute_with_format_hooks(
            &mut context,
            &command(
                "new",
                &["-d", "-n", "window-#{command}", "-s", "#{command}"],
            ),
            &mut ExplicitCommand,
        )
        .unwrap();
    let session = session_named(&engine.state, "explicit-item").unwrap();
    assert_eq!(
        engine.state.windows[&engine.state.sessions[&session].active_window].name,
        "window-explicit-item"
    );
}

#[test]
fn new_session_name_keeps_empty_values_and_applies_pin_name_cleaning() {
    let mut engine = MuxEngine::default();
    let mut setup_context = ExecutionContext::default();
    engine
        .execute(
            &mut setup_context,
            &command("new-session", &["-d", "-s", "origin"]),
        )
        .unwrap();
    let mut context = ExecutionContext::new(
        setup_context.session,
        setup_context.window,
        setup_context.pane,
    );
    context.set_client_attached(false);
    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-n", "#{window_name}", "-s", "#{session_name}"],
            ),
        )
        .unwrap();
    let empty = session_named(&engine.state, "").unwrap();
    assert!(
        engine.state.windows[&engine.state.sessions[&empty].active_window]
            .name
            .is_empty()
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("new-session", &["-d", "-s", "#{session_name}"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "duplicate session: "
    ));

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-n", r"window\#{command}", "-s", r"slash\#{command}"],
            ),
        )
        .unwrap();
    let slash = session_named(&engine.state, r"slash\\new-session").unwrap();
    assert_eq!(
        engine.state.windows[&engine.state.sessions[&slash].active_window].name,
        r"window\\new-session"
    );
    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-n", "janela-日本語", "-s", "sessão-日本語"],
            ),
        )
        .unwrap();
    let unicode = session_named(&engine.state, "sessão-日本語").unwrap();
    assert_eq!(
        engine.state.windows[&engine.state.sessions[&unicode].active_window].name,
        "janela-日本語"
    );
}

#[test]
fn new_session_name_rejects_ascii_control_values_before_creation() {
    struct InvalidName {
        session_calls: usize,
    }

    impl StatusHooks for InvalidName {
        fn strftime(&mut self, literal: &str) -> String {
            literal.to_owned()
        }

        fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
            String::new()
        }

        fn variable(&mut self, name: &str, _context: &StatusContext) -> Option<String> {
            match name {
                "invalid_name" => Some("line\nname".to_owned()),
                "session_probe" => {
                    self.session_calls += 1;
                    Some("untouched".to_owned())
                }
                _ => None,
            }
        }
    }

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    let mut hooks = InvalidName { session_calls: 0 };
    assert!(matches!(
        engine.execute_with_format_hooks(
            &mut context,
            &command(
                "new-session",
                &[
                    "-d",
                    "-n",
                    "#{invalid_name}",
                    "-s",
                    "#{session_probe}",
                ],
            ),
            &mut hooks,
        ),
        Err(ServerError::InvalidCommand(message)) if message == "invalid window name: line\nname"
    ));
    assert_eq!(hooks.session_calls, 0);
    assert!(matches!(
        engine.execute_with_format_hooks(
            &mut context,
            &command("new-session", &["-d", "-s", "#{invalid_name}"]),
            &mut hooks,
        ),
        Err(ServerError::InvalidCommand(message)) if message == "invalid session name: line\nname"
    ));
    assert!(engine.state.sessions.is_empty());

    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "existing", "-n", "original"]),
        )
        .unwrap();
    let existing = context.session.unwrap();
    let existing_window = context.window.unwrap();
    assert!(matches!(
        engine.execute_with_format_hooks(
            &mut context,
            &command(
                "new-session",
                &["-A", "-d", "-n", "#{invalid_name}", "-s", "existing"],
            ),
            &mut hooks,
        ),
        Err(ServerError::InvalidCommand(message)) if message == "invalid window name: line\nname"
    ));
    let attached = engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-A", "-d", "-n", r"ignored\#{command}", "-s", "existing"],
            ),
        )
        .unwrap();
    assert!(
        attached.effects.iter().any(
            |effect| matches!(effect, MuxEffect::Attach { session, .. } if *session == existing)
        )
    );
    assert_eq!(engine.state.windows[&existing_window].name, "original");
    assert_eq!(engine.state.sessions.len(), 1);
}

#[test]
fn new_session_prints_the_created_session_with_the_requested_format() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    let default = engine
        .execute(
            &mut context,
            &command("new-session", &["-P", "-d", "-s", "printed"]),
        )
        .expect("printed session");
    assert_eq!(default.output, "printed:");

    let formatted = engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &[
                    "-P",
                    "-d",
                    "-F",
                    "#{session_name}/#{window_index}",
                    "-s",
                    "formatted",
                ],
            ),
        )
        .expect("formatted session");
    assert_eq!(formatted.output, "formatted/0");

    let ignored = engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-F", "#{session_name}", "-s", "silent"],
            ),
        )
        .expect("silent session");
    assert!(ignored.output.is_empty());
}

#[test]
fn new_session_print_format_uses_post_attach_client_state_only_when_attaching() {
    let mut engine = MuxEngine::default();
    let (first, first_window, first_pane) = engine.state.create_session("first").unwrap();
    let mut context = ExecutionContext::new(Some(first), Some(first_window), Some(first_pane));
    context.set_attached_client_context(Some((first, first_window, first_pane)));
    context.set_format_client(FormatClient::Attached(first));

    let attached = engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-P", "-F", "#{session_active}", "-s", "attached"],
            ),
        )
        .unwrap();
    assert_eq!(attached.output, "1");

    context.retarget(&ExecutionContext::new(
        Some(first),
        Some(first_window),
        Some(first_pane),
    ));
    context.set_attached_client_context(Some((first, first_window, first_pane)));
    context.set_format_client(FormatClient::Attached(first));
    let detached = engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-P", "-F", "#{session_active}", "-s", "detached"],
            ),
        )
        .unwrap();
    assert_eq!(detached.output, "0");

    context.set_client_attached(false);
    context.set_format_client(FormatClient::Unattached);
    let unattached = engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-P", "-F", "#{session_active}", "-s", "unattached"],
            ),
        )
        .unwrap();
    assert_eq!(unattached.output, "0");

    context.set_no_client();
    context.set_format_client(FormatClient::NoClient);
    let clientless = engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-P", "-F", "#{session_active}", "-s", "clientless"],
            ),
        )
        .unwrap();
    assert!(clientless.output.is_empty());
}

#[test]
fn most_recent_context_drives_originless_new_session_cwd() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "first"]),
        )
        .expect("first session");
    let first = context.session.expect("first session id");
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "second"]),
        )
        .expect("second session");
    let second = context.session.expect("second session id");
    let second_pane = context.pane.expect("second session pane");
    assert!(engine.set_pane_runtime_facts(
        second_pane,
        PaneRuntimeFacts {
            current_path: "/private/tmp".to_owned(),
            ..PaneRuntimeFacts::default()
        },
    ));

    assert_eq!(
        engine.state.default_context().map(|state| state.0),
        Some(first)
    );
    let (session, window, pane) = engine.state.most_recent_context().expect("recent context");
    assert_eq!(session, second);

    let mut command_context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let execution = engine
        .execute(
            &mut command_context,
            &command(
                "new-session",
                &["-d", "-s", "third", "-c", "#{pane_current_path}"],
            ),
        )
        .expect("originless new session");
    assert!(matches!(
        execution.effects.first(),
        Some(MuxEffect::PaneCreated {
            inherit_cwd_from: Some(source),
            cwd: None,
            ..
        }) if *source == second_pane
    ));
    assert_eq!(
        engine
            .state
            .session_working_directory(command_context.session.unwrap()),
        Some(Path::new("/private/tmp"))
    );
}

#[test]
fn command_targeting_does_not_change_the_most_recent_session() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "A"]))
        .expect("first session");
    let first = context.session.expect("first session");
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "B"]))
        .expect("second session");
    let second = context.session.expect("second session");
    let (session, window, pane) = engine.state.most_recent_context().expect("recent context");
    let mut command_context = ExecutionContext::new(Some(session), Some(window), Some(pane));

    engine
        .execute(
            &mut command_context,
            &command("select-window", &["-t", "A:0"]),
        )
        .expect("target first session");

    assert_eq!(command_context.session, Some(first));
    assert_eq!(
        engine.state.most_recent_context().map(|context| context.0),
        Some(second)
    );
}

#[test]
fn most_recent_session_controls_empty_session_window_targets() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "A"]))
        .expect("first session");
    engine
        .execute(&mut context, &command("new-window", &["-d", "-t", "A"]))
        .expect("second window in first session");
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "B"]))
        .expect("second session");
    let (session, window, pane) = engine.state.most_recent_context().expect("recent context");
    let mut command_context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    engine
        .execute(
            &mut command_context,
            &command("select-window", &["-t", "A:0"]),
        )
        .expect("target first session");

    let (session, window, pane) = engine.state.most_recent_context().expect("recent context");
    let mut followup_context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    assert!(matches!(
        engine.execute(
            &mut followup_context,
            &command("list-panes", &["-t", ":1"]),
        ),
        Err(ServerError::WindowNotFound(target)) if target == "1"
    ));
}

#[test]
fn listing_everything_on_a_server_with_no_sessions_prints_nothing() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    for verb in ["list-panes", "list-windows"] {
        let execution = engine
            .execute(&mut context, &command(verb, &["-a"]))
            .unwrap_or_else(|error| panic!("{verb} -a: {error:?}"));
        assert!(execution.output.is_empty(), "{verb}");
    }
    assert!(
        engine
            .execute(&mut context, &command("list-panes", &[]))
            .is_err()
    );
}

#[test]
fn new_session_dash_a_attaches_to_the_named_session_instead_of_failing() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("first session");
    let session = context.session.expect("session id");
    let pane = context.pane.expect("pane id");
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "other"]),
        )
        .expect("second session");

    let attached = engine
        .execute(&mut context, &command("new-session", &["-A", "-s", "work"]))
        .expect("attach or create");
    assert_eq!(
        attached.effects,
        [MuxEffect::Attach {
            session,
            detach_others: false,
            detach_others_hangup: false,
            read_only: false,
            flags: None,
            update_environment: true,
        }]
    );
    assert_eq!(context.session, Some(session));
    assert_eq!(context.pane, Some(pane));
    assert_eq!(engine.state.sessions.len(), 2);

    let skips_update = engine
        .execute(
            &mut context,
            &command("new-session", &["-A", "-E", "-s", "work"]),
        )
        .expect("attach without environment update");
    assert!(matches!(
        skips_update.effects.as_slice(),
        [MuxEffect::Attach {
            update_environment: false,
            ..
        }]
    ));

    let ignores_detach = engine
        .execute(
            &mut context,
            &command("new-session", &["-A", "-d", "-s", "work"]),
        )
        .expect("attach or create");
    assert_eq!(
        ignores_detach.effects,
        [MuxEffect::Attach {
            session,
            detach_others: false,
            detach_others_hangup: false,
            read_only: false,
            flags: None,
            update_environment: true,
        }]
    );

    let detaches_others = engine
        .execute(
            &mut context,
            &command("new-session", &["-A", "-D", "-s", "work"]),
        )
        .expect("attach or create");
    assert_eq!(
        detaches_others.effects,
        [MuxEffect::Attach {
            session,
            detach_others: true,
            detach_others_hangup: false,
            read_only: false,
            flags: None,
            update_environment: true,
        }]
    );

    let bare = engine
        .execute(&mut context, &command("new-session", &["-A"]))
        .expect("attach to the current session");
    assert_eq!(bare.effects.len(), 1);
    assert!(matches!(bare.effects[0], MuxEffect::Attach { .. }));
    assert_eq!(engine.state.sessions.len(), 2);

    engine
        .execute(
            &mut context,
            &command("new-session", &["-A", "-d", "-s", "fresh"]),
        )
        .expect("create");
    assert!(
        engine
            .state
            .sessions
            .values()
            .any(|session| session.name == "fresh")
    );

    assert!(matches!(
        engine.execute(&mut context, &command("new-session", &["-s", "work"])),
        Err(ServerError::InvalidCommand(_))
    ));
}

#[test]
fn attach_session_preserves_detach_others_flag_in_effect() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "shared"]),
        )
        .expect("detached session");
    let session = context.session.expect("session id");

    let execution = engine
        .execute(
            &mut context,
            &command("attach-session", &["-d", "-t", "shared"]),
        )
        .expect("steal attach");

    assert_eq!(
        execution.effects,
        [MuxEffect::Attach {
            session,
            detach_others: true,
            detach_others_hangup: false,
            read_only: false,
            flags: None,
            update_environment: true,
        }]
    );

    let skipped = engine
        .execute(
            &mut context,
            &command("attach-session", &["-E", "-t", "shared"]),
        )
        .expect("attach without environment update");
    assert!(matches!(
        skipped.effects.as_slice(),
        [MuxEffect::Attach {
            update_environment: false,
            ..
        }]
    ));
}

#[test]
fn session_listing_and_loops_use_creation_and_logical_activity_orders() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine.set_format_now(1_700_000_000);
    for name in ["w", "A", "B"] {
        engine
            .execute(&mut context, &command("new-session", &["-d", "-s", name]))
            .unwrap();
    }
    assert!(engine.state.sessions.values().all(
        |session| session.activity == session.created && session.created == Some(1_700_000_000)
    ));

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-sessions", &["-F", "#{session_name}"]),
            )
            .unwrap()
            .output,
        "A\nB\nw"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-sessions",
                    &[
                        "-O",
                        "name",
                        "-f",
                        "#{==:#{session_active},0}",
                        "-F",
                        "#{session_name}=[#{session_active}]",
                    ],
                ),
            )
            .unwrap()
            .output,
        "A=[0]\nw=[0]"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("display-message", &["-p", "#{S:#{session_name} }"]),
            )
            .unwrap()
            .output,
        "w A B "
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("display-message", &["-p", "#{S/t:#{session_name} }"]),
            )
            .unwrap()
            .output,
        "B A w "
    );

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-sessions",
                    &["-O", "creation", "-F", "#{session_name}"],
                ),
            )
            .unwrap()
            .output,
        "w\nA\nB"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-sessions",
                    &["-O", "activity", "-F", "#{session_name}"],
                ),
            )
            .unwrap()
            .output,
        "B\nA\nw"
    );
    let w = engine.state.resolve_session(Some("w"), None).unwrap();
    let a = engine.state.resolve_session(Some("A"), None).unwrap();
    engine.mark_session_active_at(w, 1_700_000_123);
    assert_eq!(engine.state.sessions[&w].activity, Some(1_700_000_123));
    engine.mark_session_active_at(a, 1_700_000_123);
    assert_eq!(engine.state.sessions[&a].activity, Some(1_700_000_123));
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-sessions",
                    &["-O", "activity", "-F", "#{session_name}"],
                ),
            )
            .unwrap()
            .output,
        "A\nw\nB"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("display-message", &["-p", "#{S/t:#{session_name} }"]),
            )
            .unwrap()
            .output,
        "A w B "
    );
    for session in engine.state.sessions.keys().copied().collect::<Vec<_>>() {
        engine
            .state
            .session_mut(session)
            .expect("listed session")
            .sort_activity = 41;
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-sessions",
                    &["-O", "activity", "-F", "#{session_name}"],
                ),
            )
            .unwrap()
            .output,
        "A\nB\nw"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("display-message", &["-p", "#{S/t:#{session_name} }"]),
            )
            .unwrap()
            .output,
        "A B w "
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-sessions",
                    &["-r", "-O", "creation", "-F", "#{session_name}"],
                ),
            )
            .unwrap()
            .output,
        "B\nA\nw"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-sessions", &["-r", "-F", "#{session_name}"]),
            )
            .unwrap()
            .output,
        "A\nB\nw"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-sessions",
                    &[
                        "-f",
                        "#{==:#{session_name},w}",
                        "-F",
                        "#{line}:#{session_name}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "2:w"
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("list-sessions", &["-O", "not-an-order"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "invalid sort order"
    ));
}

#[cfg(unix)]
#[test]
fn server_access_answers_the_pins_lookups_and_refuses_a_second_identity() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    let owner = server_socket_owner();
    assert_eq!(
        engine
            .execute(&mut context, &command("server-access", &["-l"]))
            .expect("access list")
            .output
            .to_string(),
        format!("{owner} (U,W)")
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("server-access", &[]))
            .expect_err("no argument")
            .tmux_message(),
        "missing user or group argument"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("server-access", &["-w", "zzcc-nobody"])
            )
            .expect_err("unknown user")
            .tmux_message(),
        "unknown user: zzcc-nobody"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("server-access", &["-g", "zzcc-nogroup"])
            )
            .expect_err("unknown group")
            .tmux_message(),
        "unknown group: zzcc-nogroup"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("server-access", &["-a", "-d", "zzcc-nobody"])
            )
            .expect_err("lookup runs before the flag conflict")
            .tmux_message(),
        "unknown user: zzcc-nobody"
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("server-access", &["-a", &owner]))
            .expect_err("the owner")
            .tmux_message(),
        format!("{owner} owns the server, can't change access")
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("server-access", &["-d", &owner]))
            .expect_err("the owner")
            .tmux_message(),
        format!("{owner} owns the server, can't change access")
    );
    assert!(
        engine
            .execute(
                &mut context,
                &command("server-access", &["#{?#{==:1,1},nobody,root}"])
            )
            .expect("formatted non-owner without action")
            .output
            .is_empty()
    );
    let group = primary_group_name().expect("a primary group");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("server-access", &["-g", "-a", "-d", &group])
            )
            .expect_err("both actions")
            .tmux_message(),
        "-a and -d cannot be used together"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("server-access", &["-g", "-r", "-w", &group])
            )
            .expect_err("both rights")
            .tmux_message(),
        "-r and -w cannot be used together"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("server-access", &["-g", "-d", &group])
            )
            .expect_err("not in the list")
            .tmux_message(),
        format!("group {group} not found")
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("server-access", &["-g", "-a", &group])
            )
            .expect_err("no access list")
            .tmux_message(),
        format!("zz has no socket access list: the daemon socket admits {owner} alone")
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("server-access", &["-g", &group]))
            .expect("no action flag changes nothing")
            .output
            .to_string(),
        ""
    );
}

#[cfg(unix)]
fn primary_group_name() -> Option<String> {
    use nix::unistd::{Gid, Group};

    Group::from_gid(Gid::current())
        .ok()
        .flatten()
        .map(|group| group.name)
}

#[test]
fn has_session_uses_existing_target_errors_and_aliases() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    assert_eq!(
        engine
            .execute(&mut context, &command("has", &["-t", "work"]))
            .unwrap(),
        Execution::default()
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("has-session", &["-t", "missing"]),
        ),
        Err(ServerError::SessionNotFound(target)) if target == "missing"
    ));
}

#[test]
fn automatic_session_names_use_the_first_canonical_numeric_gap() {
    let mut state = MuxState::default();
    for name in [
        "0",
        "1",
        "3",
        "",
        "00",
        "01",
        "+2",
        " 2",
        "184467440737095516160",
    ] {
        state.create_session(name).expect("unique session name");
    }

    assert_eq!(next_session_name(&state), "2");
    state.create_session("2").expect("numeric gap is unused");
    assert_eq!(next_session_name(&state), "4");
}

#[test]
fn new_session_attach_routing_uses_the_command_option_parser() {
    let arguments = |args: &[&str]| {
        args.iter()
            .map(|arg| RawText::from(*arg))
            .collect::<Vec<_>>()
    };
    let attaches = |args: &[&str]| {
        MuxEngine::new_session_attaches(&arguments(args)).expect("valid new-session arguments")
    };

    assert!(attaches(&["-s", "a", "/usr/bin/true", "-d"]));
    assert!(attaches(&["-s", "b", "--", "-d"]));
    assert!(attaches(&["-dA", "-s", "existing"]));
    assert!(attaches(&["-t", "group"]));
    assert!(attaches(&["-At", "existing"]));
    assert!(!attaches(&["-dsfoo"]));
    assert!(!attaches(&["-d", "-t", "group"]));
    assert!(attaches(&["-X"]));
    assert!(MuxEngine::new_session_attaches(&arguments(&["-t"])).is_err());
}

#[test]
fn new_session_attach_preflight_defers_every_detached_attach_or_create() {
    let args = |args: &[&str]| {
        args.iter()
            .map(|argument| RawText::from(*argument))
            .collect::<Vec<_>>()
    };
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    let mut hooks = CommandHooks::new(0);
    context.set_client_attached(false);

    assert!(
        engine
            .new_session_needs_attach_preflight(&context, &args(&[]), &mut hooks)
            .unwrap()
    );
    assert!(
        !engine
            .new_session_needs_attach_preflight(
                &context,
                &args(&["-d", "-s", "detached"]),
                &mut hooks,
            )
            .unwrap()
    );
    assert!(
        !engine
            .new_session_needs_attach_preflight(
                &context,
                &args(&["-A", "-d", "-s", "missing"]),
                &mut hooks,
            )
            .unwrap()
    );

    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "existing"]),
        )
        .unwrap();
    assert!(
        !engine
            .new_session_needs_attach_preflight(
                &context,
                &args(&["-A", "-d", "-s", "existing"]),
                &mut hooks,
            )
            .unwrap()
    );
    assert!(
        engine
            .new_session_needs_attach_preflight(
                &context,
                &args(&["-A", "-s", "existing"]),
                &mut hooks,
            )
            .unwrap()
    );

    context.set_client_attached(true);
    assert!(
        !engine
            .new_session_needs_attach_preflight(
                &context,
                &args(&["-A", "-s", "existing"]),
                &mut hooks,
            )
            .unwrap()
    );
    context.set_client_attached(false);
    context.set_client_terminal(false);
    assert!(
        !engine
            .new_session_needs_attach_preflight(
                &context,
                &args(&["-A", "-s", "existing"]),
                &mut hooks,
            )
            .unwrap()
    );
    context.set_no_client();
    assert!(
        !engine
            .new_session_needs_attach_preflight(
                &context,
                &args(&["-A", "-s", "existing"]),
                &mut hooks,
            )
            .unwrap()
    );
}

#[test]
fn nested_new_session_preflight_matches_tmux_validation_order() {
    let args = |arguments: &[&str]| {
        arguments
            .iter()
            .map(|argument| RawText::from(*argument))
            .collect::<Vec<_>>()
    };
    let preflight = |engine: &MuxEngine, context: &ExecutionContext, arguments: &[&str]| {
        let mut hooks = CommandHooks::new(0);
        engine.new_session_needs_attach_preflight(context, &args(arguments), &mut hooks)
    };
    let mut engine = MuxEngine::default();
    let mut setup = ExecutionContext::default();
    engine
        .execute(
            &mut setup,
            &command("new-session", &["-d", "-s", "existing"]),
        )
        .unwrap();
    let mut context = ExecutionContext::default();
    context.set_client_attached(false);
    let generation = engine.state.generation();

    assert_eq!(
        preflight(&engine, &context, &["-t", "group", "echo"]).unwrap_err(),
        ServerError::InvalidCommand("command or window name given with target".to_owned())
    );
    assert_eq!(
        preflight(
            &engine,
            &context,
            &["-t", "group", "-n", "window", "-s", "fresh"],
        )
        .unwrap_err(),
        ServerError::InvalidCommand("command or window name given with target".to_owned())
    );
    assert_eq!(
        preflight(
            &engine,
            &context,
            &["-n", "bad\nwindow", "-s", "bad\nsession"],
        )
        .unwrap_err(),
        ServerError::InvalidCommand("invalid window name: bad\nwindow".to_owned())
    );
    assert_eq!(
        preflight(&engine, &context, &["-n", "window", "-s", "bad\nsession"]).unwrap_err(),
        ServerError::InvalidCommand("invalid session name: bad\nsession".to_owned())
    );
    assert!(preflight(&engine, &context, &["-A", "-s", "existing"]).unwrap());
    assert_eq!(
        preflight(
            &engine,
            &context,
            &["-s", "existing", "-t", "bad\ngroup", "-x", "0"],
        )
        .unwrap_err(),
        ServerError::InvalidCommand("duplicate session: existing".to_owned())
    );
    assert_eq!(
        preflight(
            &engine,
            &context,
            &["-s", "fresh", "-t", "bad\ngroup", "-x", "0"],
        )
        .unwrap_err(),
        ServerError::InvalidCommand("invalid session group name: bad\ngroup".to_owned())
    );
    assert!(
        preflight(
            &engine,
            &context,
            &["-s", "fresh", "-t", "valid-group", "-x", "0"],
        )
        .unwrap()
    );
    assert!(preflight(&engine, &context, &["-s", "fresh", "-y", "0"]).unwrap());
    assert_eq!(engine.state.generation(), generation);
    assert_eq!(engine.state.sessions.len(), 1);
}

#[test]
fn nested_new_session_preflight_expands_each_item_once_with_canonical_command() {
    struct TrackingHooks {
        order: usize,
        command_fallbacks: usize,
    }

    impl StatusHooks for TrackingHooks {
        fn strftime(&mut self, literal: &str) -> String {
            literal.to_owned()
        }

        fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
            String::new()
        }

        fn variable(&mut self, name: &str, _context: &StatusContext) -> Option<String> {
            match name {
                "order" => {
                    self.order += 1;
                    Some(self.order.to_string())
                }
                "command" => {
                    self.command_fallbacks += 1;
                    None
                }
                _ => None,
            }
        }
    }

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    context.set_client_attached(false);
    let mut hooks = TrackingHooks {
        order: 0,
        command_fallbacks: 0,
    };
    let arguments = [
        "-n",
        "window-#{order}-#{command}",
        "-s",
        "session-#{order}-#{command}",
        "-c",
        "/tmp/#{order}-#{command}",
    ]
    .map(RawText::from);
    assert!(
        engine
            .new_session_needs_attach_preflight(&context, &arguments, &mut hooks,)
            .unwrap()
    );
    assert_eq!(hooks.order, 3);
    assert_eq!(hooks.command_fallbacks, 3);
    assert!(engine.state.sessions.is_empty());

    engine
        .execute(
            &mut ExecutionContext::default(),
            &command("new-session", &["-d", "-s", "new-session"]),
        )
        .unwrap();
    hooks.order = 0;
    hooks.command_fallbacks = 0;
    let generation = engine.state.generation();
    assert_eq!(
        engine
            .new_session_needs_attach_preflight(
                &context,
                &[RawText::from("-s"), "#{command}".into()],
                &mut hooks,
            )
            .unwrap_err(),
        ServerError::InvalidCommand("duplicate session: new-session".to_owned())
    );
    assert_eq!(hooks.order, 0);
    assert_eq!(hooks.command_fallbacks, 1);
    assert_eq!(engine.state.generation(), generation);
    assert_eq!(engine.state.sessions.len(), 1);
}
