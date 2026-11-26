use super::test_support::{command, window_layout};
use super::*;

fn window_index_named(engine: &MuxEngine, session: SessionId, name: &str) -> u32 {
    engine.state.sessions[&session]
        .windows
        .iter()
        .find_map(|window| {
            let window = &engine.state.windows[window];
            (window.name == name).then_some(window.index)
        })
        .expect("named window")
}

#[test]
fn first_window_uses_the_default_extent() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    let window = context.window.unwrap();
    assert_eq!(engine.state.windows[&window].layout.extent(), (80, 24));
}

#[test]
fn new_session_honors_explicit_window_extent() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-s", "work", "-x", "200", "-y", "50"],
            ),
        )
        .unwrap();

    let window = context.window.unwrap();
    assert_eq!(engine.state.windows[&window].layout.extent(), (200, 50));
}

#[test]
fn detached_new_session_uses_default_size_with_per_dimension_overrides() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "default-size", "132x43"]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "wide"]))
        .unwrap();
    assert_eq!(
        engine.state.windows[&context.window.unwrap()]
            .layout
            .extent(),
        (132, 43)
    );
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "tall", "-x", "90"]),
        )
        .unwrap();
    assert_eq!(
        engine.state.windows[&context.window.unwrap()]
            .layout
            .extent(),
        (90, 43)
    );
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "dash-width", "-x", "-"]),
        )
        .unwrap();
    assert_eq!(
        engine.state.windows[&context.window.unwrap()]
            .layout
            .extent(),
        (80, 43)
    );
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "dash-height", "-y", "-"]),
        )
        .unwrap();
    assert_eq!(
        engine.state.windows[&context.window.unwrap()]
            .layout
            .extent(),
        (132, 24)
    );
    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-s", "dash-both", "-x", "-", "-y", "-"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine.state.windows[&context.window.unwrap()]
            .layout
            .extent(),
        (80, 24)
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "default-size", "0x20000tail"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "clamped"]),
        )
        .unwrap();
    assert_eq!(
        engine.state.windows[&context.window.unwrap()]
            .layout
            .extent(),
        (1, 10_000)
    );
}

#[test]
fn dash_creation_dimensions_use_the_caller_terminal_size() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    context.set_client_size(Some((132, 43)));
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "sized", "-x", "-", "-y", "-"]),
        )
        .unwrap();
    assert_eq!(
        engine.state.windows[&context.window.unwrap()]
            .layout
            .extent(),
        (132, 43)
    );
    context.set_client_size(None);
    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-s", "sizeless", "-x", "-", "-y", "-"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine.state.windows[&context.window.unwrap()]
            .layout
            .extent(),
        (80, 24)
    );
}

#[test]
fn projected_client_extent_preserves_split_and_zoom_geometry() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-x", "80", "-y", "24"]),
        )
        .unwrap();
    let first = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let second = context.pane.unwrap();
    let first_size = engine
        .pane_geometry_at_window_extent(first, 100, 50)
        .unwrap();
    let second_size = engine
        .pane_geometry_at_window_extent(second, 100, 50)
        .unwrap();
    assert_eq!(first_size.0 + second_size.0 + 1, 100);
    assert_eq!((first_size.1, second_size.1), (50, 50));

    engine
        .execute(&mut context, &command("resize-pane", &["-Z"]))
        .unwrap();
    assert_eq!(
        engine.pane_geometry_at_window_extent(second, 100, 50),
        Some((100, 50))
    );
    assert_eq!(engine.pane_geometry_at_window_extent(first, 100, 50), None);
}

#[test]
fn measurement_write_back_bumps_generation_and_reports_change() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let pane = context.pane.unwrap();
    let generation = engine.state.generation();
    assert!(engine.set_pane_geometry(pane, 200, 50));
    assert!(engine.state.generation() > generation);
    let repeat_generation = engine.state.generation();
    assert!(!engine.set_pane_geometry(pane, 200, 50));
    assert_eq!(engine.state.generation(), repeat_generation);
}

#[test]
fn new_window_inherits_the_active_window_extent() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    let first_window = context.window.unwrap();
    let first_pane = context.pane.unwrap();
    engine.set_pane_geometry(first_pane, 200, 50);

    engine
        .execute(&mut context, &command("new-window", &["-d"]))
        .unwrap();

    let inherited = engine.state.sessions[&session]
        .windows
        .iter()
        .copied()
        .find(|window| *window != first_window)
        .unwrap();
    assert_eq!(engine.state.windows[&inherited].layout.extent(), (200, 50));
}

#[test]
fn break_pane_destination_inherits_the_active_window_extent() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let original_window = context.window.unwrap();
    let first = context.pane.unwrap();
    engine.set_pane_geometry(first, 200, 50);
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let moving = context.pane.unwrap();

    engine
        .execute(
            &mut context,
            &command("break-pane", &["-d", "-s", &moving.to_string()]),
        )
        .unwrap();

    let inherited = engine.state.window_for_pane(moving).unwrap();
    assert_ne!(inherited, original_window);
    assert_eq!(engine.state.windows[&inherited].layout.extent(), (200, 50));
}

#[test]
fn new_window_dash_d_creates_without_selecting() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    let first = context.window.unwrap();

    let created = engine
        .execute(&mut context, &command("new-window", &["-d", "-n", "logs"]))
        .expect("detached window");
    assert_eq!(context.window, Some(first));
    assert_eq!(engine.state.sessions[&session].active_window, first);
    let pane = match created.effects.first() {
        Some(MuxEffect::PaneCreated { pane, .. }) => *pane,
        other => panic!("expected a created pane: {other:?}"),
    };
    let created_window = engine.state.window_for_pane(pane).expect("window");
    assert_eq!(engine.state.windows[&created_window].name, "logs");
    assert!(engine.state.validate().is_ok());
}

#[test]
fn new_window_keeps_invoking_and_target_format_clients_distinct() {
    let mut engine = MuxEngine::default();
    let (session, window, pane) = engine.state.create_session("work").unwrap();
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    context.set_client_attached(false);
    context.set_format_client(FormatClient::Attached(session));

    let created = engine
        .execute(
            &mut context,
            &command(
                "new-window",
                &[
                    "-d",
                    "-n",
                    "#{session_active}",
                    "-c",
                    "#{session_active}",
                    "-P",
                    "-F",
                    "#{session_active}",
                ],
            ),
        )
        .unwrap();
    assert_eq!(created.output, "1\n");
    let created_pane = created
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated {
                pane,
                cwd: Some(cwd),
                ..
            } if cwd == "0" => Some(*pane),
            _ => None,
        })
        .expect("created pane with invoking-client cwd");
    let created_window = engine.state.window_for_pane(created_pane).unwrap();
    assert_eq!(engine.state.windows[&created_window].name, "0");
    assert!(created.effects.iter().any(|effect| {
        matches!(
            effect,
            MuxEffect::PaneFormatOutput {
                format_client: FormatClient::Attached(client),
                ..
            } if *client == session
        )
    }));
}

#[test]
fn new_window_dash_a_inserts_after_the_target_and_shifts_the_run_up() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    for name in ["one", "two", "three"] {
        engine
            .execute(&mut context, &command("new-window", &["-n", name]))
            .unwrap();
    }
    assert_eq!(
        window_layout(&engine, session),
        ["0:0", "1:one", "2:two", "3:three"]
    );

    engine
        .execute(
            &mut context,
            &command("new-window", &["-a", "-t", "1", "-n", "inserted"]),
        )
        .expect("insert after window 1");
    assert_eq!(
        window_layout(&engine, session),
        ["0:0", "1:one", "2:inserted", "3:two", "4:three"]
    );
    assert_eq!(
        engine.state.windows[&engine.state.sessions[&session].active_window].name,
        "inserted"
    );
    assert!(engine.state.validate().is_ok());
}

#[test]
fn new_window_dash_a_keeps_an_explicit_free_index() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-a", "-t", "5"]),
        )
        .unwrap();
    assert_eq!(window_layout(&engine, session), ["0:0", "5:5"]);
    assert!(engine.state.validate().is_ok());
}

#[test]
fn new_window_dash_b_inserts_before_the_target_and_prefers_before_over_after() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "work:2", "-n", "two"]),
        )
        .unwrap();

    engine
        .execute(
            &mut context,
            &command("new-window", &["-b", "-d", "-t", "work:2", "-n", "before"]),
        )
        .unwrap();
    assert_eq!(
        window_layout(&engine, session),
        ["0:0", "2:before", "3:two"]
    );

    engine
        .execute(
            &mut context,
            &command(
                "new-window",
                &["-a", "-b", "-d", "-t", "work:2", "-n", "both"],
            ),
        )
        .unwrap();
    assert_eq!(
        window_layout(&engine, session),
        ["0:0", "2:both", "3:before", "4:two"]
    );

    engine
        .execute(
            &mut context,
            &command("new-window", &["-b", "-d", "-t", "work:7", "-n", "free"]),
        )
        .unwrap();
    assert_eq!(
        window_layout(&engine, session),
        ["0:0", "2:both", "3:before", "4:two", "7:free"]
    );
    assert!(engine.state.validate().is_ok());
}

#[test]
fn new_window_dash_k_replaces_the_window_holding_the_index() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    let doomed = context.window.unwrap();
    let doomed_pane = context.pane.unwrap();

    let replaced = engine
        .execute(
            &mut context,
            &command("new-window", &["-k", "-t", "0", "-n", "replacement"]),
        )
        .expect("replace the only window");
    assert!(
        replaced
            .effects
            .contains(&MuxEffect::PanesRemoved(vec![doomed_pane]))
    );
    assert!(!engine.state.windows.contains_key(&doomed));
    assert_eq!(window_layout(&engine, session), ["0:replacement"]);
    assert!(engine.state.validate().is_ok());
}

#[test]
fn new_window_dash_s_selects_an_existing_window_with_the_same_name() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-n", "logs"]))
        .unwrap();
    let logs = context.window.unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-n", "editor"]))
        .unwrap();

    let selected = engine
        .execute(
            &mut context,
            &command(
                "new-window",
                &["-P", "-F", "should-not-print", "-S", "-n", "logs"],
            ),
        )
        .expect("select the existing window");
    assert_eq!(
        selected.effects,
        [MuxEffect::SuppressAfterHook, MuxEffect::SnapshotChanged]
    );
    assert!(selected.output.is_empty());
    assert_eq!(context.window, Some(logs));
    assert_eq!(engine.state.sessions[&session].windows.len(), 3);

    let expanded = engine
        .execute(
            &mut context,
            &command("new-window", &["-S", "-n", "#{window_name}"]),
        )
        .expect("expand the reuse name");
    assert_eq!(
        expanded.effects,
        [MuxEffect::SuppressAfterHook, MuxEffect::SnapshotChanged]
    );
    assert_eq!(context.window, Some(logs));

    engine
        .execute(&mut context, &command("new-window", &["-S", "-n", "fresh"]))
        .expect("create the missing window");
    assert_eq!(engine.state.sessions[&session].windows.len(), 4);

    let indexed = engine
        .execute(
            &mut context,
            &command(
                "new-window",
                &[
                    "-d",
                    "-S",
                    "-t",
                    "work:5",
                    "-n",
                    "logs",
                    "-P",
                    "-F",
                    "#{window_index}:#{window_name}",
                ],
            ),
        )
        .expect("an explicit index bypasses reuse");
    assert_eq!(indexed.output, "5:logs\n");
    assert_eq!(engine.state.sessions[&session].windows.len(), 5);
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("new-window", &["-S", "-n", "logs"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "multiple windows named logs"
    ));
}

#[test]
fn new_window_dash_s_reexpands_only_the_unindexed_lookup_name() {
    struct LookupHooks {
        first_calls: usize,
        second_calls: usize,
    }

    impl StatusHooks for LookupHooks {
        fn strftime(&mut self, literal: &str) -> String {
            literal.to_owned()
        }

        fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
            String::new()
        }

        fn variable(&mut self, name: &str, _context: &StatusContext) -> Option<String> {
            match name {
                "first" => {
                    self.first_calls += 1;
                    Some("#{second}".to_owned())
                }
                "second" => {
                    self.second_calls += 1;
                    Some(
                        match self.second_calls {
                            1 => "lookup-hit",
                            2 => "lookup-miss",
                            _ => "lookup-index",
                        }
                        .to_owned(),
                    )
                }
                _ => None,
            }
        }
    }

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    let mut hooks = LookupHooks {
        first_calls: 0,
        second_calls: 0,
    };

    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-s", "reuse-hit", "-n", "lookup-hit"],
            ),
        )
        .unwrap();
    let hit_session = session_named(&engine.state, "reuse-hit").unwrap();
    let hit = engine
        .execute_with_format_hooks(
            &mut context,
            &command(
                "new-window",
                &["-d", "-S", "-t", "=reuse-hit:", "-n", "#{first}"],
            ),
            &mut hooks,
        )
        .unwrap();
    assert!(hit.effects.contains(&MuxEffect::SuppressAfterHook));
    assert_eq!(engine.state.sessions[&hit_session].windows.len(), 1);
    assert_eq!(hooks.first_calls, 1);
    assert_eq!(hooks.second_calls, 1);

    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "reuse-miss", "-n", "anchor"]),
        )
        .unwrap();
    let miss_session = session_named(&engine.state, "reuse-miss").unwrap();
    let miss = engine
        .execute_with_format_hooks(
            &mut context,
            &command(
                "new-window",
                &["-d", "-S", "-t", "=reuse-miss:", "-n", "#{first}"],
            ),
            &mut hooks,
        )
        .unwrap();
    assert!(!miss.effects.contains(&MuxEffect::SuppressAfterHook));
    let miss_window = engine.state.window_at_index(miss_session, 1).unwrap();
    assert_eq!(engine.state.windows[&miss_window].name, "#{second}");
    assert_eq!(hooks.first_calls, 2);
    assert_eq!(hooks.second_calls, 2);

    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-s", "reuse-index", "-n", "lookup-index"],
            ),
        )
        .unwrap();
    let index_session = session_named(&engine.state, "reuse-index").unwrap();
    let indexed = engine
        .execute_with_format_hooks(
            &mut context,
            &command(
                "new-window",
                &["-d", "-S", "-t", "=reuse-index:5", "-n", "#{first}"],
            ),
            &mut hooks,
        )
        .unwrap();
    assert!(!indexed.effects.contains(&MuxEffect::SuppressAfterHook));
    let indexed_window = engine.state.window_at_index(index_session, 5).unwrap();
    assert_eq!(engine.state.windows[&indexed_window].name, "#{second}");
    assert_eq!(hooks.first_calls, 3);
    assert_eq!(hooks.second_calls, 2);
    assert!(engine.state.validate().is_ok());
}

#[test]
fn new_window_name_expands_once_then_validates_and_cleans() {
    struct NameHooks {
        calls: usize,
    }

    impl StatusHooks for NameHooks {
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
                    Some(if self.calls == 1 {
                        "value-#{late}".to_owned()
                    } else {
                        "twice".to_owned()
                    })
                }
                "invalid_name" => Some("line\nname".to_owned()),
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
                &["-d", "-s", "target", "-n", "target-window"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("select-pane", &["-T", "target-pane"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "current"]),
        )
        .unwrap();

    let mut hooks = NameHooks { calls: 0 };
    let created = engine
        .execute_with_format_hooks(
            &mut context,
            &command(
                "neww",
                &[
                    "-d",
                    "-t",
                    "=target:5",
                    "-n",
                    r"janela-日本語\#{command}-#{session_name}-#{window_name}-#{pane_title}-#{once}",
                ],
            ),
            &mut hooks,
        )
        .unwrap();
    assert_eq!(hooks.calls, 1);
    let pane = created
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .expect("created pane");
    let window = engine.state.window_for_pane(pane).expect("created window");
    assert_eq!(
        engine.state.windows[&window].name,
        r"janela-日本語\\new-window-target-target-window-target-pane-value-#{late}"
    );

    let target = session_named(&engine.state, "target").unwrap();
    let windows_before = engine.state.sessions[&target].windows.len();
    assert!(matches!(
        engine.execute_with_format_hooks(
            &mut context,
            &command(
                "new-window",
                &["-d", "-t", "=target:6", "-n", "#{invalid_name}"],
            ),
            &mut hooks,
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "invalid window name: line\nname"
    ));
    assert_eq!(engine.state.sessions[&target].windows.len(), windows_before);

    engine
        .execute(
            &mut context,
            &command("select-window", &["-t", "=target:0"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-n", r"collision\#{command}"]),
        )
        .unwrap();
    let windows_before = engine.state.sessions[&target].windows.len();
    let reused = engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-S", "-n", r"collision\#{command}"]),
        )
        .unwrap();
    assert!(reused.effects.contains(&MuxEffect::SuppressAfterHook));
    assert_eq!(engine.state.sessions[&target].windows.len(), windows_before);
}

#[test]
fn name_commands_use_pin_format_types() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    let created = engine
        .execute(
            &mut context,
            &command(
                "new-window",
                &[
                    "-d",
                    "-n",
                    "new-#{session_format}:#{window_format}:#{pane_format}",
                ],
            ),
        )
        .unwrap();
    let pane = created
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .unwrap();
    let window = engine.state.window_for_pane(pane).unwrap();
    assert_eq!(engine.state.windows[&window].name, "new-1:0:0");

    engine
        .execute(
            &mut context,
            &command(
                "rename-window",
                &[
                    "-t",
                    &window.to_string(),
                    "renamed-#{session_format}:#{window_format}:#{pane_format}",
                ],
            ),
        )
        .unwrap();
    assert_eq!(engine.state.windows[&window].name, "renamed-0:0:1");

    engine
        .execute(
            &mut context,
            &command(
                "rename-session",
                &[
                    "-t",
                    &session.to_string(),
                    "renamed-#{session_format}:#{window_format}:#{pane_format}",
                ],
            ),
        )
        .unwrap();
    assert_eq!(engine.state.sessions[&session].name, "renamed-0:0:1");
}

#[test]
fn base_index_drives_default_allocation_but_not_explicit_targets() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "base-index", "1"]),
        )
        .expect("global base index");
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session at the global base");
    let session = context.session.expect("session id");
    let first_window = context.window.expect("first window");
    assert_eq!(engine.state.windows[&first_window].index, 1);
    assert_eq!(engine.state.windows[&first_window].name, "1");

    engine
        .execute(
            &mut context,
            &command("split-window", &["-h", "-t", "work:1.0"]),
        )
        .expect("split first window");
    let moving = context.pane.expect("split pane");
    engine
        .execute(
            &mut context,
            &command("break-pane", &["-d", "-s", &moving.to_string()]),
        )
        .expect("default break destination");
    let broken_window = engine.state.window_for_pane(moving).expect("broken window");
    assert_eq!(engine.state.windows[&broken_window].index, 2);

    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "work:0", "-n", "low"]),
        )
        .expect("explicit index below the base");
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "work", "-n", "next"]),
        )
        .expect("next default index");
    assert_eq!(window_index_named(&engine, session, "low"), 0);
    assert_eq!(window_index_named(&engine, session, "next"), 3);

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-t", &session.to_string(), "base-index", "5"],
            ),
        )
        .expect("session base override");
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "work", "-n", "override"]),
        )
        .expect("session default index");
    assert_eq!(window_index_named(&engine, session, "override"), 5);

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-u", "-t", &session.to_string(), "base-index"],
            ),
        )
        .expect("restore session inheritance");
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "work", "-n", "inherited"]),
        )
        .expect("inherited default index");
    assert_eq!(window_index_named(&engine, session, "inherited"), 4);

    engine
        .execute(&mut context, &command("set-option", &["-gu", "base-index"]))
        .expect("restore global default");
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "zero"]))
        .expect("default-base session");
    let zero_window = context.window.expect("zero-base window");
    assert_eq!(engine.state.windows[&zero_window].index, 0);
    assert!(engine.state.validate().is_ok());

    let mut edge = MuxEngine::default();
    let mut edge_context = ExecutionContext::default();
    edge.execute(
        &mut edge_context,
        &command(
            "set-option",
            &["-g", "base-index", &MAX_BASE_INDEX.to_string()],
        ),
    )
    .expect("maximum base index");
    edge.execute(&mut edge_context, &command("new-session", &["-s", "edge"]))
        .expect("maximum-base session");
    assert_eq!(
        edge.state.windows[&edge_context.window.expect("edge window")].index,
        MAX_BASE_INDEX
    );
    edge.execute(
        &mut edge_context,
        &command("new-window", &["-d", "-t", "edge", "-n", "wrapped"]),
    )
    .expect("wrapped allocation");
    assert_eq!(
        window_index_named(
            &edge,
            edge_context.session.expect("edge session"),
            "wrapped"
        ),
        0
    );
}

#[test]
fn alert_window_steps_cover_pinned_bell_activity_and_silence_flags() {
    for flag in ["bell", "activity", "silence"] {
        for verb in ["next-window", "previous-window"] {
            let mut engine = MuxEngine::default();
            let mut context = ExecutionContext::default();
            engine
                .execute(
                    &mut context,
                    &command("new-session", &["-s", "alerts", "-n", "first"]),
                )
                .expect("session");
            engine
                .execute(&mut context, &command("new-window", &["-n", "second"]))
                .expect("second window");
            let alerted = context.window.expect("second window id");
            let pane = context.pane.expect("second pane");
            engine
                .execute(&mut context, &command("select-window", &["-t", "alerts:0"]))
                .expect("first window");
            match flag {
                "bell" => {
                    engine.state.set_pane_bell(pane, true);
                }
                "activity" => {
                    engine
                        .state
                        .windows
                        .get_mut(&mut engine.state.journal, &alerted)
                        .unwrap()
                        .activity_flag = true;
                }
                "silence" => {
                    engine
                        .state
                        .windows
                        .get_mut(&mut engine.state.journal, &alerted)
                        .unwrap()
                        .silence_flag = true;
                }
                _ => unreachable!(),
            }
            engine
                .execute(&mut context, &command(verb, &["-a"]))
                .expect("select alerted window");
            assert_eq!(context.window, Some(alerted), "{verb} {flag}");
            assert!(!engine.window_alerted(alerted));
            assert!(
                engine
                    .execute(&mut context, &command(verb, &["-a"]))
                    .is_err()
            );
        }
    }
}

#[test]
fn format_geometry_reads_tree_allocations_and_tracks_measurements() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let second = context.pane.unwrap();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-panes",
                    &["-F", "#{pane_id}:#{pane_width}x#{pane_height}"],
                ),
            )
            .unwrap()
            .output,
        format!("{first}:40x24\n{second}:39x24")
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-windows", &["-F", "#{window_width}x#{window_height}"],),
            )
            .unwrap()
            .output,
        "80x24"
    );

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &first.to_string()]),
        )
        .unwrap();
    engine.set_pane_geometry(first, 80, 24);
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-panes",
                    &["-F", "#{pane_id}:#{pane_width}x#{pane_height}"],
                ),
            )
            .unwrap()
            .output,
        format!("{first}:80x24\n{second}:79x24")
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-windows", &["-F", "#{window_width}x#{window_height}"],),
            )
            .unwrap()
            .output,
        "160x24"
    );
}

#[test]
fn format_geometry_reports_headless_defaults() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-panes",
                    &[
                        "-F",
                        "#{pane_width}:#{pane_height}:#{window_width}:#{window_height}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "80:24:80:24"
    );
}

#[test]
fn window_extent_survives_a_zoomed_measurement() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &first.to_string()]),
        )
        .unwrap();
    engine.set_pane_geometry(first, 80, 24);
    let extent = |engine: &mut MuxEngine, context: &mut ExecutionContext| {
        engine
            .execute(
                context,
                &command("list-windows", &["-F", "#{window_width}x#{window_height}"]),
            )
            .unwrap()
            .output
    };

    assert_eq!(extent(&mut engine, &mut context), "160x24");
    engine
        .execute(
            &mut context,
            &command("resize-pane", &["-Z", "-t", &first.to_string()]),
        )
        .unwrap();
    engine.set_pane_geometry(first, 160, 24);
    assert_eq!(extent(&mut engine, &mut context), "160x24");
    engine
        .execute(
            &mut context,
            &command("resize-pane", &["-Z", "-t", &first.to_string()]),
        )
        .unwrap();
    engine.set_pane_geometry(first, 80, 24);
    assert_eq!(extent(&mut engine, &mut context), "160x24");
}

#[test]
fn alternating_three_pane_measurements_reach_a_fixed_extent() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let second = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let third = context.pane.unwrap();
    let window = context.window.unwrap();

    for (pane, columns) in [(first, 80), (second, 38), (third, 38)] {
        engine.set_pane_geometry(pane, columns, 24);
    }
    let stable = engine.state.windows[&window].layout.extent();
    assert_eq!(stable, (160, 24));

    for _ in 0..8 {
        for (pane, columns) in [(first, 80), (second, 38), (third, 38)] {
            engine.set_pane_geometry(pane, columns, 24);
        }
        assert_eq!(engine.state.windows[&window].layout.extent(), stable);
    }
}

#[test]
fn hidden_pane_measurement_is_ignored_while_zoomed() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let hidden = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let zoomed = context.pane.unwrap();
    let window = context.window.unwrap();
    engine
        .execute(
            &mut context,
            &command("resize-pane", &["-Z", "-t", &zoomed.to_string()]),
        )
        .unwrap();

    engine.set_pane_geometry(hidden, 200, 50);

    assert_eq!(engine.state.windows[&window].layout.extent(), (80, 24));
}

#[test]
fn zoomed_pane_measurement_sets_the_window_extent_directly() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let zoomed = context.pane.unwrap();
    let window = context.window.unwrap();
    engine
        .execute(
            &mut context,
            &command("resize-pane", &["-Z", "-t", &zoomed.to_string()]),
        )
        .unwrap();

    engine.set_pane_geometry(zoomed, 200, 50);

    assert_eq!(engine.state.windows[&window].layout.extent(), (200, 50));
}

#[test]
fn extent_probe_memo_accepts_a_different_measurement() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let pane = context.pane.unwrap();
    let window = context.window.unwrap();

    engine.set_pane_geometry(pane, 160, 48);
    assert_eq!(engine.state.windows[&window].layout.extent(), (160, 48));
    assert_eq!(
        engine.state.windows[&window].last_extent_probe,
        Some((pane, 160, 48))
    );
    engine.set_pane_geometry(pane, 160, 48);
    assert_eq!(engine.state.windows[&window].layout.extent(), (160, 48));

    engine.set_pane_geometry(pane, 200, 50);
    assert_eq!(engine.state.windows[&window].layout.extent(), (200, 50));
    assert_eq!(
        engine.state.windows[&window].last_extent_probe,
        Some((pane, 200, 50))
    );
}

#[test]
fn zoomed_pane_reports_the_full_window_extent_like_tmux() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let second = context.pane.unwrap();
    let sizes = |engine: &mut MuxEngine, context: &mut ExecutionContext| {
        engine
            .execute(
                context,
                &command("list-panes", &["-F", "#{pane_width}x#{pane_height}"]),
            )
            .unwrap()
            .output
    };

    assert_eq!(sizes(&mut engine, &mut context), "40x24\n39x24");
    engine
        .execute(
            &mut context,
            &command("resize-pane", &["-Z", "-t", &second.to_string()]),
        )
        .unwrap();
    assert_eq!(sizes(&mut engine, &mut context), "40x24\n80x24");
    assert_eq!(engine.pane_geometry(second), Some((80, 24)));
    engine
        .execute(
            &mut context,
            &command("resize-pane", &["-Z", "-t", &second.to_string()]),
        )
        .unwrap();
    assert_eq!(sizes(&mut engine, &mut context), "40x24\n39x24");
}

#[test]
fn new_window_cwd_origin_is_the_target_session_not_the_context_pane() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "target"]))
        .expect("target session");
    let target_pane = context.pane.expect("target pane");
    assert!(engine.set_pane_runtime_facts(
        target_pane,
        PaneRuntimeFacts {
            current_path: "/private/tmp".to_owned(),
            ..PaneRuntimeFacts::default()
        },
    ));
    engine
        .execute(&mut context, &command("new-session", &["-s", "other"]))
        .expect("other session");
    let other_pane = context.pane.expect("other pane");
    assert_ne!(other_pane, target_pane);

    let window = engine
        .execute(
            &mut context,
            &command(
                "new-window",
                &["-d", "-t", "target", "-c", "#{pane_current_path}"],
            ),
        )
        .expect("cross-session new-window");
    assert!(matches!(
        window.effects.first(),
        Some(MuxEffect::PaneCreated {
            inherit_cwd_from: Some(source),
            cwd: Some(cwd),
            ..
        }) if *source == target_pane && cwd == "/private/tmp"
    ));
}

#[test]
fn new_windows_and_sessions_expand_cwd_and_keep_the_origin_pane_as_donor() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let first = context.pane.expect("first pane");
    assert!(engine.set_pane_runtime_facts(
        first,
        PaneRuntimeFacts {
            current_path: "/private/tmp".to_owned(),
            reported_path: "/tmp".to_owned(),
            ..PaneRuntimeFacts::default()
        },
    ));
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &["-p", "#{pane_current_path}|#{pane_path}"],
                ),
            )
            .expect("source paths")
            .output,
        "/private/tmp|/tmp"
    );

    let mut outside = ExecutionContext::default();
    let formatted = engine
        .execute(
            &mut outside,
            &command(
                "new-window",
                &["-d", "-t", "work", "-c", "#{pane_current_path}"],
            ),
        )
        .expect("formatted cwd window from outside a pane");
    assert!(matches!(
        formatted.effects.first(),
        Some(MuxEffect::PaneCreated {
            inherit_cwd_from: Some(source),
            cwd: Some(cwd),
            ..
        }) if *source == first && cwd == "/private/tmp"
    ));

    let window = engine
        .execute(&mut context, &command("new-window", &["-c", "/tmp"]))
        .expect("literal cwd window");
    assert!(matches!(
        window.effects.first(),
        Some(MuxEffect::PaneCreated {
            inherit_cwd_from: Some(source),
            cwd: Some(cwd),
            ..
        }) if *source == first && cwd == "/tmp"
    ));
    let second = context.pane.expect("second pane");
    assert_ne!(second, first);

    let session = engine
        .execute(
            &mut context,
            &command("new-session", &["-s", "next", "-c", "/tmp"]),
        )
        .expect("literal cwd session");
    assert!(matches!(
        session.effects.first(),
        Some(MuxEffect::PaneCreated {
            inherit_cwd_from: Some(source),
            cwd: Some(cwd),
            ..
        }) if *source == second && cwd == "/tmp"
    ));
}

#[test]
fn rename_commands_preserve_ids_and_support_aliases_and_targets() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "first"]))
        .expect("first session");
    let first_session = context.session.expect("first session id");
    let first_window = context.window.expect("first window id");

    let renamed = engine
        .execute(&mut context, &command("rename", &["primary"]))
        .expect("rename current session");
    assert_eq!(renamed.effects, [MuxEffect::SnapshotChanged]);
    assert_eq!(engine.state.sessions[&first_session].name, "primary");
    assert_eq!(context.session, Some(first_session));

    let generation = engine.state.generation();
    let unchanged = engine
        .execute(&mut context, &command("rename-session", &["primary"]))
        .expect("same session name is a no-op");
    assert!(unchanged.effects.is_empty());
    assert_eq!(engine.state.generation(), generation);

    engine
        .execute(&mut context, &command("new-session", &["-s", "second"]))
        .expect("second session");
    let second_session = context.session.expect("second session id");
    engine
        .execute(
            &mut context,
            &command(
                "rename-session",
                &["-t", &first_session.to_string(), "work"],
            ),
        )
        .expect("rename targeted session");
    assert_eq!(engine.state.sessions[&first_session].name, "work");
    assert_eq!(context.session, Some(second_session));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("rename-session", &["-t", "second", "work"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "duplicate session: work"
    ));

    engine
        .execute(&mut context, &command("renamew", &["editor"]))
        .expect("rename current window");
    let second_window = context.window.expect("second window id");
    assert_eq!(engine.state.windows[&second_window].name, "editor");
    engine
        .execute(
            &mut context,
            &command(
                "rename-window",
                &["-t", &first_window.to_string(), "editor"],
            ),
        )
        .expect("duplicate window names are allowed");
    assert_eq!(engine.state.windows[&first_window].name, "editor");
    assert_eq!(context.window, Some(second_window));

    engine
        .execute(&mut context, &command("rename-window", &["--", "-logs"]))
        .expect("double dash permits a leading hyphen");
    assert_eq!(engine.state.windows[&second_window].name, "-logs");
    assert!(engine.state.validate().is_ok());
}

#[test]
fn rename_commands_require_one_name_and_reject_unknown_flags() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .expect("session");

    for invalid in [
        command("rename-session", &[]),
        command("rename-session", &["one", "two"]),
        command("rename-window", &[]),
        command("rename-window", &["-q", "name"]),
    ] {
        assert!(matches!(
            engine.execute(&mut context, &invalid),
            Err(ServerError::CommandParse(_))
        ));
    }
}

#[test]
fn rename_names_validate_clean_collide_and_expand_once() {
    struct RenameHooks {
        calls: usize,
    }

    impl StatusHooks for RenameHooks {
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
                    Some(if self.calls == 1 {
                        "value-#{late}".to_owned()
                    } else {
                        "twice".to_owned()
                    })
                }
                "invalid_name" => Some("line\nname".to_owned()),
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
                &["-d", "-s", "target", "-n", "target-window"],
            ),
        )
        .unwrap();
    let target_session = context.session.unwrap();
    let target_window = context.window.unwrap();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", r"collision\rename-session"]),
        )
        .unwrap();

    let mut hooks = RenameHooks { calls: 0 };
    engine
        .execute_with_format_hooks(
            &mut context,
            &command(
                "rename-session",
                &[
                    "-t",
                    &target_session.to_string(),
                    r"renamed\#{command}-#{once}",
                ],
            ),
            &mut hooks,
        )
        .unwrap();
    assert_eq!(hooks.calls, 1);
    assert_eq!(
        engine.state.sessions[&target_session].name,
        r"renamed\\rename-session-value-#{late}"
    );

    hooks.calls = 0;
    engine
        .execute_with_format_hooks(
            &mut context,
            &command(
                "rename-window",
                &[
                    "-t",
                    &target_window.to_string(),
                    r"renamed\#{command}-#{once}",
                ],
            ),
            &mut hooks,
        )
        .unwrap();
    assert_eq!(hooks.calls, 1);
    assert_eq!(
        engine.state.windows[&target_window].name,
        r"renamed\\rename-window-value-#{late}"
    );

    engine
        .execute(
            &mut context,
            &command(
                "rename-session",
                &["-t", &target_session.to_string(), "sessão-日本語"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "rename-window",
                &["-t", &target_window.to_string(), "janela-日本語"],
            ),
        )
        .unwrap();
    assert_eq!(engine.state.sessions[&target_session].name, "sessão-日本語");
    assert_eq!(engine.state.windows[&target_window].name, "janela-日本語");

    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "rename-session",
                &[
                    "-t",
                    &target_session.to_string(),
                    r"collision\rename-session",
                ],
            ),
        ),
        Err(ServerError::InvalidCommand(message))
            if message == r"duplicate session: collision\\rename-session"
    ));
    assert_eq!(engine.state.sessions[&target_session].name, "sessão-日本語");

    for command_name in ["rename-session", "rename-window"] {
        let (target, kind) = if command_name == "rename-session" {
            (target_session.to_string(), "session")
        } else {
            (target_window.to_string(), "window")
        };
        assert!(matches!(
            engine.execute_with_format_hooks(
                &mut context,
                &command(command_name, &["-t", &target, "#{invalid_name}"]),
                &mut hooks,
            ),
            Err(ServerError::InvalidCommand(message))
                if message == format!("invalid {kind} name: line\nname")
        ));
    }
    assert_eq!(engine.state.sessions[&target_session].name, "sessão-日本語");
    assert_eq!(engine.state.windows[&target_window].name, "janela-日本語");

    engine
        .execute(
            &mut context,
            &command(
                "rename-session",
                &["-t", &target_session.to_string(), "#{missing}"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "rename-window",
                &["-t", &target_window.to_string(), "#{missing}"],
            ),
        )
        .unwrap();
    assert!(engine.state.sessions[&target_session].name.is_empty());
    assert!(engine.state.windows[&target_window].name.is_empty());
}

#[test]
fn resize_window_sets_manual_extent_and_ignores_client_measurements() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-d", "-s", "work", "-x", "100", "-y", "30"],
            ),
        )
        .unwrap();
    let window = context.window.unwrap();
    let pane = context.pane.unwrap();

    let execution = engine
        .execute(
            &mut context,
            &command("resize-window", &["-x", "90", "-y", "20"]),
        )
        .unwrap();
    assert_eq!(
        execution.effects,
        [
            MuxEffect::WindowSizeChanged {
                window: Some(window)
            },
            MuxEffect::SnapshotChanged,
        ]
    );
    assert_eq!(engine.window_size(window), WindowSize::Manual);
    assert_eq!(engine.state.windows[&window].layout.extent(), (90, 20));
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &[
                        "-p",
                        "#{window_width}x#{window_height}|#{window_manual_width}:#{window_manual_height}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "90x20|90:20"
    );
    assert!(!engine.set_pane_geometry(pane, 200, 50));
    assert_eq!(engine.state.windows[&window].layout.extent(), (90, 20));

    for (args, expected) in [
        (vec!["-R", "3"], (93, 20)),
        (vec!["-D", "2"], (93, 22)),
        (vec!["-L"], (92, 22)),
        (vec!["-U"], (92, 21)),
    ] {
        engine
            .execute(&mut context, &command("resizew", &args))
            .unwrap();
        assert_eq!(engine.state.windows[&window].layout.extent(), expected);
    }

    engine
        .execute(
            &mut context,
            &command("set-window-option", &["window-size", "latest"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &["-p", "#{window_manual_width}:#{window_manual_height}"],
                ),
            )
            .unwrap()
            .output,
        ":"
    );
    assert!(engine.set_pane_geometry(pane, 120, 40));
    assert_eq!(engine.state.windows[&window].layout.extent(), (120, 40));
    engine
        .execute(
            &mut context,
            &command("set-window-option", &["window-size", "manual"]),
        )
        .unwrap();
    assert_eq!(engine.state.windows[&window].layout.extent(), (92, 21));
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &["-p", "#{window_manual_width}:#{window_manual_height}"],
                ),
            )
            .unwrap()
            .output,
        "92:21"
    );
    assert!(!engine.set_pane_geometry(pane, 80, 24));
}

#[test]
fn resize_window_matches_numeric_errors_and_target_precedence() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "work"]))
        .unwrap();

    for (args, expected) in [
        (vec!["-x", "bad"], "width invalid"),
        (vec!["-x", "0"], "width too small"),
        (vec!["-x", "10001"], "width too large"),
        (vec!["bad"], "adjustment invalid"),
        (vec!["0"], "adjustment too small"),
    ] {
        assert!(matches!(
            engine.execute(&mut context, &command("resize-window", &args)),
            Err(ServerError::InvalidCommand(message)) if message == expected
        ));
    }
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("resize-window", &["one", "two"]),
        ),
        Err(ServerError::CommandParse(message))
            if message == "command resize-window: too many arguments (need at most 1)"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("resize-window", &["-t", "missing", "-x", "bad"]),
        ),
        Err(ServerError::WindowNotFound(message)) if message == "missing"
    ));
    let window = context.window.unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("resize-window", &["-a"]))
            .unwrap()
            .effects,
        [MuxEffect::ResizeWindowFromClients {
            window,
            mode: WindowSize::Smallest,
        }]
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("resize-window", &["-Aa", "-R", "-x", "77", "-y", "17", "4"],),
            )
            .unwrap()
            .effects,
        [MuxEffect::ResizeWindowFromClients {
            window,
            mode: WindowSize::Largest,
        }]
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("resize-window", &["-A", "-t", "missing", "bad"]),
        ),
        Err(ServerError::WindowNotFound(message)) if message == "missing"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("resize-window", &["-A", "bad"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "adjustment invalid"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("resize-window", &["-A", "-x", "bad"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "width invalid"
    ));
}

#[test]
fn last_window_toggles_history_and_survives_window_removal() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first = context.window.unwrap();

    assert!(matches!(
        engine.execute(&mut context, &command("last-window", &[])),
        Err(ServerError::InvalidCommand(_))
    ));

    engine
        .execute(&mut context, &command("new-window", &[]))
        .unwrap();
    let second = context.window.unwrap();
    engine.execute(&mut context, &command("last", &[])).unwrap();
    assert_eq!(context.window, Some(first));
    engine
        .execute(&mut context, &command("select-window", &["-l"]))
        .unwrap();
    assert_eq!(context.window, Some(second));

    engine
        .execute(&mut context, &command("kill-window", &[]))
        .unwrap();
    assert_eq!(context.window, Some(first));
    assert!(matches!(
        engine.execute(&mut context, &command("last-window", &[])),
        Err(ServerError::InvalidCommand(_))
    ));
}

#[test]
fn renumber_windows_compacts_close_paths_and_preserves_window_identity() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "base-index", "1"]),
        )
        .expect("global base index");
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "renumber-windows", "on"]),
        )
        .expect("global renumbering");
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("first window");
    let session = context.session.expect("session");
    let first = context.window.expect("first window");
    engine
        .execute(&mut context, &command("new-window", &["-n", "second"]))
        .expect("second window");
    let second = context.window.expect("second window");
    let second_pane = context.pane.expect("second pane");
    engine
        .execute(&mut context, &command("new-window", &["-n", "third"]))
        .expect("third window");
    let third = context.window.expect("third window");
    let third_pane = context.pane.expect("third pane");
    assert_eq!(engine.state.sessions[&session].active_window, third);
    assert_eq!(engine.state.sessions[&session].last_window(), Some(second));

    let generation = engine.state.generation();
    let killed = engine
        .execute(&mut context, &command("kill-window", &["-t", "work:1"]))
        .expect("kill first window");
    assert!(killed.effects.contains(&MuxEffect::SnapshotChanged));
    assert!(engine.state.generation() > generation);
    assert!(!engine.state.windows.contains_key(&first));
    assert_eq!(engine.state.windows[&second].index, 1);
    assert_eq!(engine.state.windows[&third].index, 2);
    assert_eq!(engine.state.sessions[&session].active_window, third);
    assert_eq!(engine.state.sessions[&session].last_window(), Some(second));
    let snapshot = engine.state.snapshot();
    assert_eq!(
        snapshot.sessions[0]
            .windows
            .iter()
            .map(|window| (window.id, window.index))
            .collect::<Vec<_>>(),
        [(second, 1), (third, 2)]
    );

    engine
        .execute(
            &mut context,
            &command("kill-pane", &["-t", &second_pane.to_string()]),
        )
        .expect("last pane closes its window");
    assert!(!engine.state.windows.contains_key(&second));
    assert_eq!(engine.state.windows[&third].index, 1);

    let source = engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-n", "source"]),
        )
        .expect("join source")
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .expect("source pane");
    let source_window = engine.state.window_for_pane(source).expect("source window");
    let survivor = engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-n", "survivor"]),
        )
        .expect("surviving window")
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .expect("survivor pane");
    let survivor_window = engine
        .state
        .window_for_pane(survivor)
        .expect("survivor window");
    assert_eq!(engine.state.windows[&source_window].index, 2);
    assert_eq!(engine.state.windows[&survivor_window].index, 3);

    engine
        .execute(
            &mut context,
            &command(
                "join-pane",
                &[
                    "-d",
                    "-s",
                    &source.to_string(),
                    "-t",
                    &third_pane.to_string(),
                ],
            ),
        )
        .expect("join closes source window");
    assert!(!engine.state.windows.contains_key(&source_window));
    assert_eq!(engine.state.windows[&third].index, 1);
    assert_eq!(engine.state.windows[&survivor_window].index, 2);
    assert!(engine.state.validate().is_ok());
}

#[test]
fn join_and_move_preflight_post_close_renumber_capacity() {
    for name in ["join-pane", "move-pane"] {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(&mut context, &command("new-session", &["-s", "work"]))
            .unwrap();
        let target = context.pane.unwrap();
        engine
            .execute(&mut context, &command("new-window", &["-n", "source"]))
            .unwrap();
        let source = context.pane.unwrap();
        engine
            .execute(&mut context, &command("new-window", &["-n", "survivor"]))
            .unwrap();
        engine
            .execute(
                &mut context,
                &command(
                    "set-option",
                    &["-g", "base-index", &MAX_BASE_INDEX.to_string()],
                ),
            )
            .unwrap();
        engine
            .execute(
                &mut context,
                &command("set-option", &["-g", "renumber-windows", "on"]),
            )
            .unwrap();
        let snapshot = engine.state.snapshot();
        let generation = engine.state.generation();
        let original_context = context.clone();

        let error = engine
            .execute(
                &mut context,
                &command(
                    name,
                    &["-s", &source.to_string(), "-t", &target.to_string()],
                ),
            )
            .unwrap_err();
        assert!(matches!(error, ServerError::InvalidCommand(message)
            if message == "no free window index"));
        assert_eq!(engine.state.snapshot(), snapshot, "{name}");
        assert_eq!(engine.state.generation(), generation, "{name}");
        assert_eq!(context, original_context, "{name}");
        assert!(engine.state.validate().is_ok(), "{name}");
    }
}

#[test]
fn kill_commands_preflight_post_close_renumber_capacity() {
    for name in ["kill-window", "kill-pane"] {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(&mut context, &command("new-session", &["-s", "work"]))
            .unwrap();
        engine
            .execute(&mut context, &command("new-window", &["-n", "target"]))
            .unwrap();
        let target = if name == "kill-window" {
            context.window.unwrap().to_string()
        } else {
            context.pane.unwrap().to_string()
        };
        engine
            .execute(&mut context, &command("new-window", &["-n", "survivor"]))
            .unwrap();
        engine
            .execute(
                &mut context,
                &command(
                    "set-option",
                    &["-g", "base-index", &MAX_BASE_INDEX.to_string()],
                ),
            )
            .unwrap();
        engine
            .execute(
                &mut context,
                &command("set-option", &["-g", "renumber-windows", "on"]),
            )
            .unwrap();
        let snapshot = engine.state.snapshot();
        let generation = engine.state.generation();
        let original_context = context.clone();

        let error = engine
            .execute(&mut context, &command(name, &["-t", &target]))
            .unwrap_err();
        assert!(matches!(error, ServerError::InvalidCommand(message)
            if message == "no free window index"));
        assert_eq!(engine.state.snapshot(), snapshot, "{name}");
        assert_eq!(engine.state.generation(), generation, "{name}");
        assert_eq!(context, original_context, "{name}");
        assert!(engine.state.validate().is_ok(), "{name}");
    }
}

#[test]
fn move_window_supports_insertion_replacement_and_cross_session_moves() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "alpha"]))
        .unwrap();
    let alpha = context.session.unwrap();
    let main = context.window.unwrap();
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "alpha:1", "-n", "target"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "alpha:2", "-n", "moving"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "alpha:5", "-n", "spare"]),
        )
        .unwrap();
    let target = engine.state.window_named(alpha, "target").unwrap();
    let moving = engine.state.window_named(alpha, "moving").unwrap();
    let spare = engine.state.window_named(alpha, "spare").unwrap();
    let target_pane = engine.state.windows[&target].active_pane;

    let snapshot = engine.state.snapshot();
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("move-window", &["-s", "alpha:spare", "-t", "alpha:2"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "index in use: 2"
    ));
    assert_eq!(engine.state.snapshot(), snapshot);

    engine
        .execute(
            &mut context,
            &command(
                "move-window",
                &["-a", "-d", "-s", "alpha:spare", "-t", "alpha:target"],
            ),
        )
        .unwrap();
    assert_eq!(
        window_layout(&engine, alpha),
        ["0:0", "1:target", "2:spare", "3:moving"]
    );
    assert_eq!(engine.state.sessions[&alpha].active_window, main);

    engine
        .execute(
            &mut context,
            &command(
                "move-window",
                &["-b", "-s", "alpha:moving", "-t", "alpha:target"],
            ),
        )
        .unwrap();
    assert_eq!(
        window_layout(&engine, alpha),
        ["0:0", "1:moving", "2:target", "3:spare"]
    );
    assert_eq!(context.window, Some(moving));
    assert_eq!(engine.state.sessions[&alpha].active_window, moving);

    let replaced = engine
        .execute(
            &mut context,
            &command(
                "move-window",
                &["-d", "-k", "-s", "alpha:spare", "-t", "alpha:2"],
            ),
        )
        .unwrap();
    assert!(
        replaced
            .effects
            .contains(&MuxEffect::PanesRemoved(vec![target_pane]))
    );
    assert!(!engine.state.windows.contains_key(&target));
    assert_eq!(
        window_layout(&engine, alpha),
        ["0:0", "1:moving", "2:spare"]
    );

    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "beta", "-n", "home"]),
        )
        .unwrap();
    let beta = session_named(&engine.state, "beta").unwrap();
    let spare_pane = engine.state.windows[&spare].active_pane;
    let moved = engine
        .execute(
            &mut context,
            &command("move-window", &["-d", "-s", "alpha:spare", "-t", "beta:4"]),
        )
        .unwrap();
    assert!(moved.effects.contains(&MuxEffect::PaneRelocated {
        pane: spare_pane,
        from: alpha,
        to: beta,
    }));
    assert_eq!(engine.state.windows[&spare].session, beta);
    assert_eq!(engine.state.windows[&spare].index, 4);
    assert_eq!(engine.state.sessions[&alpha].active_window, moving);
    assert_eq!(engine.state.windows[&main].session, alpha);
    assert!(engine.state.validate().is_ok());
}

#[test]
fn move_window_r_still_resolves_a_bogus_source_and_mutates_nothing() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "rn3"]))
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "rn3:3", "-n", "a3"]),
        )
        .unwrap();

    let error = engine
        .execute(
            &mut context,
            &command("move-window", &["-r", "-s", "rn3:99", "-t", "rn3"]),
        )
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        ServerError::WindowNotFound("99".to_owned()).to_string()
    );
    let indexes = engine
        .execute(
            &mut context,
            &command("list-windows", &["-t", "rn3", "-F", "#{window_index}"]),
        )
        .unwrap()
        .output;
    assert_eq!(indexes, "0\n3");
}

#[test]
fn move_window_renumbers_from_base_index_and_preflights_option_compaction() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "base-index", "1"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "renumber-windows", "on"]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let work = context.session.unwrap();
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "work:3", "-n", "three"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("new-window", &["-t", "work:5", "-n", "five"]),
        )
        .unwrap();
    let five = context.window.unwrap();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "remote"]),
        )
        .unwrap();
    let remote = session_named(&engine.state, "remote").unwrap();
    engine
        .execute(
            &mut context,
            &command("select-window", &["-t", "work:five"]),
        )
        .unwrap();

    engine
        .execute(
            &mut context,
            &command("move-window", &["-d", "-t", "remote:4"]),
        )
        .unwrap();
    assert_eq!(engine.state.windows[&five].session, remote);
    assert_eq!(engine.state.windows[&five].index, 4);
    assert_eq!(window_layout(&engine, work), ["1:1", "2:three"]);

    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "work:9", "-n", "nine"]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("move-window", &["-r", "-t", "work"]))
        .unwrap();
    assert_eq!(window_layout(&engine, work), ["1:1", "2:three", "3:nine"]);

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "base-index", "2147483647"]),
        )
        .unwrap();
    let snapshot = engine.state.snapshot();
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("move-window", &["-t", "remote:7"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "no free window index"
    ));
    assert_eq!(engine.state.snapshot(), snapshot);
    assert!(engine.state.validate().is_ok());
}

#[test]
fn swap_window_exchanges_slots_and_dash_d_selects_destination_slots() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-s", "alpha", "-n", "first"]),
        )
        .unwrap();
    let alpha = context.session.unwrap();
    let first = context.window.unwrap();
    let first_pane = context.pane.unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-n", "second"]))
        .unwrap();
    let second = context.window.unwrap();
    let second_pane = context.pane.unwrap();
    engine
        .execute(
            &mut context,
            &command("select-window", &["-t", "alpha:first"]),
        )
        .unwrap();

    engine
        .execute(
            &mut context,
            &command("swap-window", &["-s", "alpha:first", "-t", "alpha:second"]),
        )
        .unwrap();
    assert_eq!(engine.state.windows[&first].index, 1);
    assert_eq!(engine.state.windows[&second].index, 0);
    assert_eq!(engine.state.sessions[&alpha].active_window, second);
    assert_eq!(engine.state.windows[&first].active_pane, first_pane);
    assert_eq!(engine.state.windows[&second].active_pane, second_pane);

    engine
        .execute(
            &mut context,
            &command(
                "swap-window",
                &["-d", "-s", "alpha:first", "-t", "alpha:second"],
            ),
        )
        .unwrap();
    assert_eq!(engine.state.windows[&first].index, 0);
    assert_eq!(engine.state.windows[&second].index, 1);
    assert_eq!(engine.state.sessions[&alpha].active_window, first);

    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "beta", "-n", "third"]),
        )
        .unwrap();
    let beta = session_named(&engine.state, "beta").unwrap();
    let third = engine.state.window_named(beta, "third").unwrap();
    let third_pane = engine.state.windows[&third].active_pane;
    let swapped = engine
        .execute(
            &mut context,
            &command("swap-window", &["-s", "alpha:first", "-t", "beta:third"]),
        )
        .unwrap();
    assert!(swapped.effects.contains(&MuxEffect::PaneRelocated {
        pane: first_pane,
        from: alpha,
        to: beta,
    }));
    assert!(swapped.effects.contains(&MuxEffect::PaneRelocated {
        pane: third_pane,
        from: beta,
        to: alpha,
    }));
    assert_eq!(engine.state.windows[&first].session, beta);
    assert_eq!(engine.state.windows[&third].session, alpha);
    assert_eq!(engine.state.sessions[&alpha].active_window, third);
    assert_eq!(engine.state.sessions[&beta].active_window, first);

    engine
        .execute(
            &mut context,
            &command(
                "swap-window",
                &["-d", "-s", &first.to_string(), "-t", &third.to_string()],
            ),
        )
        .unwrap();
    assert_eq!(engine.state.windows[&first].session, alpha);
    assert_eq!(engine.state.windows[&third].session, beta);
    assert_eq!(engine.state.sessions[&alpha].active_window, first);
    assert_eq!(engine.state.sessions[&beta].active_window, third);
    assert!(engine.state.validate().is_ok());
}

#[test]
fn chooser_swap_leaves_the_other_window_of_the_pair_as_last() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-s", "alpha", "-n", "w0"]),
        )
        .unwrap();
    let alpha = context.session.unwrap();
    let w0 = context.window.unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-n", "w1"]))
        .unwrap();
    let w1 = context.window.unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-n", "w2"]))
        .unwrap();
    let w2 = context.window.unwrap();
    engine
        .execute(&mut context, &command("select-window", &["-t", "alpha:w1"]))
        .unwrap();
    engine
        .execute(&mut context, &command("select-window", &["-t", "alpha:w0"]))
        .unwrap();
    assert_eq!(engine.state.sessions[&alpha].active_window, w0);
    assert_eq!(engine.state.sessions[&alpha].last_window(), Some(w1));
    let (slot0, slot1, slot2) = (
        engine.state.windows[&w0].index,
        engine.state.windows[&w1].index,
        engine.state.windows[&w2].index,
    );

    engine.state.swap_windows_keeping_current(w1, w0).unwrap();

    assert_eq!(engine.state.windows[&w1].index, slot0);
    assert_eq!(engine.state.windows[&w0].index, slot1);
    assert_eq!(engine.state.sessions[&alpha].active_window, w0);
    assert_eq!(engine.state.sessions[&alpha].last_window(), Some(w1));

    engine.state.swap_windows_keeping_current(w0, w2).unwrap();

    assert_eq!(engine.state.windows[&w0].index, slot2);
    assert_eq!(engine.state.windows[&w2].index, slot1);
    assert_eq!(engine.state.sessions[&alpha].active_window, w0);
    assert_eq!(engine.state.sessions[&alpha].last_window(), Some(w2));
    assert!(engine.state.validate().is_ok());
}
