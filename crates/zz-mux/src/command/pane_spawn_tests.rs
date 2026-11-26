use super::test_support::{command, window_layout};
use super::*;

fn relocated_pane_geometries(command_name: &str, args: &[&str]) -> ((u16, u16), (u16, u16)) {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let target = context.pane.unwrap();
    engine.set_pane_geometry(target, 80, 24);
    let source = engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-n", "source"]),
        )
        .unwrap()
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .unwrap();
    assert!(engine.set_pane_geometry(source, 120, 40));
    let mut join = command(command_name, args);
    join.args.extend([
        RawText::from("-s"),
        source.to_string().into(),
        "-t".into(),
        target.to_string().into(),
    ]);
    engine.execute(&mut context, &join).unwrap();
    (
        engine.pane_geometry(target).unwrap(),
        engine.pane_geometry(source).unwrap(),
    )
}

fn break_placement_session(
    engine: &mut MuxEngine,
    context: &mut ExecutionContext,
    name: &str,
) -> SessionId {
    engine
        .execute(
            context,
            &command("new-session", &["-d", "-s", name, "-n", "source"]),
        )
        .unwrap();
    engine
        .execute(
            context,
            &command("split-window", &["-d", "-h", "-t", &format!("{name}:0.0")]),
        )
        .unwrap();
    engine
        .execute(
            context,
            &command(
                "new-window",
                &["-d", "-t", &format!("{name}:1"), "-n", "anchor"],
            ),
        )
        .unwrap();
    engine
        .execute(
            context,
            &command(
                "new-window",
                &["-d", "-t", &format!("{name}:2"), "-n", "tail"],
            ),
        )
        .unwrap();
    engine
        .execute(
            context,
            &command("select-window", &["-t", &format!("{name}:1")]),
        )
        .unwrap();
    session_named(&engine.state, name).unwrap()
}

fn absolute_test_path(name: &str) -> PathBuf {
    std::env::current_dir()
        .expect("current test directory")
        .join("target")
        .join("zz-mux-tests")
        .join(name)
}

#[test]
fn pane_creation_commands_print_the_created_pane_format() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    let new_window = engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-P", "-n", "logs"]),
        )
        .unwrap();
    assert_eq!(new_window.output, "work:1.0\n");

    let split = engine
        .execute(
            &mut context,
            &command(
                "split-window",
                &["-P", "-F", "#{session_name}|#{window_index}|#{pane_index}"],
            ),
        )
        .unwrap();
    assert_eq!(split.output, "work|0|1\n");
    let moving = context.pane.unwrap();

    let broken = engine
        .execute(
            &mut context,
            &command(
                "break-pane",
                &[
                    "-d",
                    "-P",
                    "-F",
                    "#{session_name}|#{window_index}|#{pane_index}",
                    "-s",
                    &moving.to_string(),
                ],
            ),
        )
        .unwrap();
    assert_eq!(broken.output, "work|2|0\n");

    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let trailing_newline = engine
        .execute(
            &mut context,
            &command("break-pane", &["-d", "-P", "-F", "X\n"]),
        )
        .unwrap();
    assert_eq!(trailing_newline.output.as_bytes(), b"X\n\n");

    let format_without_print = engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-F", "#{window_name}"]),
        )
        .unwrap();
    assert!(format_without_print.output.is_empty());
}

#[test]
fn pane_creation_defers_the_exact_format_client_state() {
    let mut engine = MuxEngine::default();
    let (session, window, pane) = engine.state.create_session("work").unwrap();
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    context.set_attached_client_context(Some((session, window, pane)));
    context.set_format_client(FormatClient::Attached(session));

    let attached = engine
        .execute(
            &mut context,
            &command(
                "new-window",
                &["-d", "-P", "-F", "#{session_active}", "-n", "attached"],
            ),
        )
        .unwrap();
    assert_eq!(attached.output, "1\n");
    assert!(attached.effects.iter().any(|effect| {
        matches!(
            effect,
            MuxEffect::PaneFormatOutput {
                active_session: Some(active),
                format_client: FormatClient::Attached(client),
                ..
            } if *active == session && *client == session
        )
    }));

    context.set_client_attached(false);
    context.set_format_client(FormatClient::Unattached);
    let unattached = engine
        .execute(
            &mut context,
            &command("split-window", &["-d", "-P", "-F", "#{session_active}"]),
        )
        .unwrap();
    assert_eq!(unattached.output, "0\n");
    assert!(unattached.effects.iter().any(|effect| {
        matches!(
            effect,
            MuxEffect::PaneFormatOutput {
                format_client: FormatClient::Unattached,
                ..
            }
        )
    }));

    context.set_no_client();
    context.set_format_client(FormatClient::NoClient);
    let clientless = engine
        .execute(
            &mut context,
            &command("split-window", &["-d", "-P", "-F", "#{session_active}"]),
        )
        .unwrap();
    assert_eq!(clientless.output, "\n");
    assert!(clientless.effects.iter().any(|effect| {
        matches!(
            effect,
            MuxEffect::PaneFormatOutput {
                format_client: FormatClient::NoClient,
                ..
            }
        )
    }));
}

#[test]
fn break_pane_dash_t_names_the_destination_index() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    let home = context.window.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let broken = context.pane.unwrap();

    engine
        .execute(
            &mut context,
            &command("break-pane", &["-d", "-t", "work:5", "-n", "moved"]),
        )
        .expect("break to an explicit index");
    assert_eq!(window_layout(&engine, session), ["0:0", "5:moved"]);
    assert_eq!(context.window, Some(home));
    assert_eq!(engine.state.sessions[&session].active_window, home);

    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let second = context.pane.unwrap();
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("break-pane", &["-d", "-t", "work:5"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "index in use: 5"
    ));

    engine
        .execute(&mut context, &command("break-pane", &["-t", "7"]))
        .expect("break to a bare index in the current session");
    assert_eq!(engine.state.window_for_pane(second), context.window);
    assert_eq!(
        engine.state.windows[&context.window.unwrap()].index,
        7,
        "the broken pane lands at the requested index"
    );
    let first_break = engine
        .state
        .window_for_pane(broken)
        .expect("the first broken pane kept its window");
    assert_eq!(engine.state.windows[&first_break].index, 5);
    assert!(engine.state.validate().is_ok());
}

#[test]
fn break_pane_name_stays_literal_then_validates_and_cleans() {
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
            (name == "probe").then(|| {
                self.calls += 1;
                "expanded".to_owned()
            })
        }
    }

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    engine
        .execute(&mut context, &command("split-window", &[]))
        .unwrap();
    let first = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &[]))
        .unwrap();
    let second = context.pane.unwrap();
    let original_window = context.window.unwrap();
    let windows_before = engine.state.windows.len();
    let panes_before = engine
        .state
        .windows
        .values()
        .map(|window| window.panes.len())
        .sum::<usize>();
    let layout_before = window_layout(&engine, session);
    let cell_layout_before = engine.state.windows[&original_window].layout.clone();
    let indices_before = engine.state.sessions[&session]
        .windows
        .iter()
        .map(|window| engine.state.windows[window].index)
        .collect::<Vec<_>>();
    let mut hooks = NameHooks { calls: 0 };
    assert!(matches!(
        engine.execute_with_format_hooks(
            &mut context,
            &command(
                "break-pane",
                &[
                    "-d",
                    "-b",
                    "-s",
                    &first.to_string(),
                    "-t",
                    "work:0",
                    "-n",
                    "line\nname",
                ],
            ),
            &mut hooks,
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "invalid window name: line\nname"
    ));
    assert_eq!(window_layout(&engine, session), layout_before);
    assert_eq!(engine.state.windows[&original_window].index, 0);
    assert_eq!(
        engine.state.windows[&original_window].layout,
        cell_layout_before
    );
    assert_eq!(
        engine.state.sessions[&session]
            .windows
            .iter()
            .map(|window| engine.state.windows[window].index)
            .collect::<Vec<_>>(),
        indices_before
    );
    assert_eq!(engine.state.windows.len(), windows_before);
    assert_eq!(
        engine
            .state
            .windows
            .values()
            .map(|window| window.panes.len())
            .sum::<usize>(),
        panes_before
    );

    let literal = r"janela-日本語\#{probe}";
    for pane in [first, second] {
        engine
            .execute_with_format_hooks(
                &mut context,
                &command("breakp", &["-d", "-s", &pane.to_string(), "-n", literal]),
                &mut hooks,
            )
            .unwrap();
    }
    assert_eq!(hooks.calls, 0);
    assert_eq!(
        engine
            .state
            .windows
            .values()
            .filter(|window| window.name == r"janela-日本語\\#{probe}")
            .count(),
        2
    );
    assert!(engine.state.validate().is_ok());
}

#[test]
fn break_pane_name_disables_automatic_rename_on_both_window_paths() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "relink"]))
        .unwrap();
    let relink_window = context.window.unwrap();
    let relink_pane = context.pane.unwrap();
    engine
        .state
        .set_window_automatic_rename(relink_window, Some(true))
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "break-pane",
                &[
                    "-d",
                    "-s",
                    &relink_pane.to_string(),
                    "-t",
                    "=relink:5",
                    "-n",
                    "relinked",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        engine.state.window_for_pane(relink_pane),
        Some(relink_window)
    );
    assert_eq!(engine.state.windows[&relink_window].name, "relinked");
    assert_eq!(engine.state.windows[&relink_window].index, 5);
    assert_eq!(
        engine
            .state
            .window_automatic_rename_override(relink_window)
            .unwrap(),
        Some(false)
    );

    engine
        .execute(&mut context, &command("new-session", &["-s", "created"]))
        .unwrap();
    let source_window = context.window.unwrap();
    engine
        .execute(&mut context, &command("split-window", &[]))
        .unwrap();
    let moved_pane = context.pane.unwrap();
    engine
        .state
        .set_window_automatic_rename(source_window, Some(true))
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "break-pane",
                &[
                    "-d",
                    "-s",
                    &moved_pane.to_string(),
                    "-t",
                    "=created:5",
                    "-n",
                    "created-name",
                ],
            ),
        )
        .unwrap();
    let created_window = engine.state.window_for_pane(moved_pane).unwrap();
    assert_ne!(created_window, source_window);
    assert_eq!(engine.state.windows[&created_window].name, "created-name");
    assert_eq!(engine.state.windows[&created_window].index, 5);
    assert_eq!(
        engine
            .state
            .window_automatic_rename_override(created_window)
            .unwrap(),
        Some(false)
    );
    assert_eq!(
        engine
            .state
            .window_automatic_rename_override(source_window)
            .unwrap(),
        Some(true)
    );
    assert!(engine.state.validate().is_ok());
}

#[test]
fn break_pane_without_a_name_names_the_window_from_the_command_not_the_title() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "w"]))
        .unwrap();
    let break_out = |engine: &mut MuxEngine, context: &mut ExecutionContext, index: &str| {
        engine
            .execute(context, &command("split-window", &["-t", "=w:0"]))
            .unwrap();
        let pane = context.pane.unwrap();
        engine
            .set_pane_shell(pane, "/usr/bin/bash".to_owned())
            .unwrap();
        engine
            .state
            .update_pane_title(pane, "user@host:/home/user")
            .unwrap();
        (pane, index.to_owned())
    };

    let (shell_pane, index) = break_out(&mut engine, &mut context, "=w:6");
    engine
        .execute(
            &mut context,
            &command(
                "break-pane",
                &["-d", "-s", &shell_pane.to_string(), "-t", &index],
            ),
        )
        .unwrap();
    let shell_window = engine.state.window_for_pane(shell_pane).unwrap();
    assert_eq!(engine.state.windows[&shell_window].name, "bash");
    assert_eq!(
        engine
            .state
            .window_automatic_rename_override(shell_window)
            .unwrap(),
        None
    );

    let (command_pane, index) = break_out(&mut engine, &mut context, "=w:7");
    engine
        .set_pane_start_command(
            command_pane,
            vec!["/usr/bin/vim".to_owned(), "notes.txt".to_owned()],
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "break-pane",
                &["-d", "-s", &command_pane.to_string(), "-t", &index],
            ),
        )
        .unwrap();
    let command_window = engine.state.window_for_pane(command_pane).unwrap();
    assert_eq!(engine.state.windows[&command_window].name, "vim");

    let (busy_pane, index) = break_out(&mut engine, &mut context, "=w:8");
    engine.set_pane_runtime_facts(
        busy_pane,
        PaneRuntimeFacts {
            current_command: "sleep".to_owned(),
            ..PaneRuntimeFacts::default()
        },
    );
    engine
        .execute(
            &mut context,
            &command(
                "break-pane",
                &["-d", "-s", &busy_pane.to_string(), "-t", &index],
            ),
        )
        .unwrap();
    let busy_window = engine.state.window_for_pane(busy_pane).unwrap();
    assert_eq!(engine.state.windows[&busy_window].name, "sleep");
    assert!(engine.state.validate().is_ok());
}

#[test]
fn break_pane_places_after_before_and_around_the_current_window() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    let after = break_placement_session(&mut engine, &mut context, "after");
    engine
        .execute(
            &mut context,
            &command(
                "break-pane",
                &[
                    "-d",
                    "-a",
                    "-s",
                    "after:0.1",
                    "-t",
                    "after:1",
                    "-n",
                    "moved",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        window_layout(&engine, after),
        ["0:source", "1:anchor", "2:moved", "3:tail"]
    );

    let before = break_placement_session(&mut engine, &mut context, "before");
    engine
        .execute(
            &mut context,
            &command(
                "break-pane",
                &[
                    "-d",
                    "-a",
                    "-b",
                    "-s",
                    "before:0.1",
                    "-t",
                    "before:1",
                    "-n",
                    "moved",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        window_layout(&engine, before),
        ["0:source", "1:moved", "2:anchor", "3:tail"]
    );

    let fallback = break_placement_session(&mut engine, &mut context, "fallback");
    engine
        .execute(
            &mut context,
            &command(
                "break-pane",
                &[
                    "-d",
                    "-a",
                    "-s",
                    "fallback:0.1",
                    "-t",
                    "fallback:8",
                    "-n",
                    "after-current",
                ],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("split-window", &["-d", "-h", "-t", "fallback:0.0"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "break-pane",
                &[
                    "-d",
                    "-b",
                    "-s",
                    "fallback:0.1",
                    "-t",
                    "fallback:9",
                    "-n",
                    "before-current",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        window_layout(&engine, fallback),
        [
            "0:source",
            "1:before-current",
            "2:anchor",
            "3:after-current",
            "4:tail",
        ]
    );
    assert!(engine.state.validate().is_ok());
}

#[test]
fn break_pane_placement_preserves_argument_source_and_target_precedence() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    let session = break_placement_session(&mut engine, &mut context, "work");
    let before = window_layout(&engine, session);

    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "break-pane",
                &["-a", "-s", "missing", "-t", "missing:8", "extra"],
            ),
        ),
        Err(ServerError::CommandParse(message))
            if message == "command break-pane: too many arguments (need at most 0)"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("break-pane", &["-a", "-s", "missing", "-t", "missing:8"]),
        ),
        Err(ServerError::PaneNotFound(pane)) if pane == "missing"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("break-pane", &["-a", "-s", "work:0.1", "-t", "missing:8"]),
        ),
        Err(ServerError::SessionNotFound(session)) if session == "missing"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "break-pane",
                &["-a", "-s", "work:0.1", "-t", "work:garbage"],
            ),
        ),
        Err(ServerError::WindowNotFound(window)) if window == "garbage"
    ));
    assert_eq!(window_layout(&engine, session), before);
}

#[test]
fn break_pane_moves_a_single_pane_window_without_replacing_it() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(&mut context, &command("rename-window", &["kept"]))
        .unwrap();
    let session = context.session.unwrap();
    let window = context.window.unwrap();
    let pane = context.pane.unwrap();
    let layout = engine.state.windows[&window].layout.clone();

    engine
        .execute(&mut context, &command("break-pane", &["-d", "-t", "1"]))
        .unwrap();

    assert_eq!(engine.state.window_for_pane(pane), Some(window));
    assert_eq!(engine.state.windows[&window].name, "kept");
    assert_eq!(engine.state.windows[&window].index, 1);
    assert_eq!(engine.state.windows[&window].layout, layout);
    assert_eq!(engine.state.sessions[&session].windows, [window]);
    assert_eq!(engine.state.sessions[&session].active_window, window);
    assert!(engine.state.validate().is_ok());
}

#[test]
fn terminal_splits_expand_cwd_and_keep_the_target_pane_as_donor() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let first = context.pane.expect("first pane");

    let plain = engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .expect("plain split");
    assert!(matches!(
        plain.effects.first(),
        Some(MuxEffect::PaneCreated {
            inherit_cwd_from: Some(source),
            cwd: None,
            ..
        }) if *source == first
    ));
    let second = context.pane.expect("second pane");

    let parsed = crate::parse_config(
        "test.conf",
        "bind | split-window -v -c \"#{pane_current_path}\"",
    );
    assert!(parsed.diagnostics.is_empty());
    engine
        .execute(&mut context, &parsed.commands[0])
        .expect("tmux-style binding");
    let bound_split = engine
        .keys
        .get("prefix", "|")
        .expect("split binding")
        .commands[0]
        .clone();
    let tmux_style = engine
        .execute(&mut context, &bound_split)
        .expect("tmux-style split");
    assert!(matches!(
        tmux_style.effects.first(),
        Some(MuxEffect::PaneCreated {
            inherit_cwd_from: Some(source),
            cwd: None,
            ..
        }) if *source == second
    ));
    let third = context.pane.expect("third pane");

    let literal = engine
        .execute(&mut context, &command("split-window", &["-c", "/tmp"]))
        .expect("literal cwd split");
    assert!(matches!(
        literal.effects.first(),
        Some(MuxEffect::PaneCreated {
            inherit_cwd_from: Some(source),
            cwd: Some(cwd),
            ..
        }) if *source == third && cwd == "/tmp"
    ));
    let literal_pane = context.pane.expect("literal cwd pane");

    let empty = engine
        .execute(
            &mut context,
            &command("split-window", &["-c", "#{not_a_tmux_variable}"]),
        )
        .expect("empty cwd format split");
    assert!(matches!(
        empty.effects.first(),
        Some(MuxEffect::PaneCreated {
            inherit_cwd_from: Some(source),
            cwd: None,
            ..
        }) if *source == literal_pane
    ));
}

#[test]
fn new_and_split_window_create_live_empty_panes() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");

    for (name, arguments, keeps_empty_argument) in [
        ("new-window", &["-d", "-E"][..], false),
        ("new-window", &["-d", "-E", ""][..], true),
        ("split-window", &["-d", "-E"][..], false),
        ("split-window", &["-d", ""][..], true),
    ] {
        let execution = engine
            .execute(&mut context, &command(name, arguments))
            .expect("empty pane");
        let Some(MuxEffect::PaneCreated {
            pane,
            command,
            empty,
            ..
        }) = execution.effects.first()
        else {
            panic!("missing pane creation effect");
        };
        assert!(*empty);
        assert_eq!(
            command.as_deref().is_some_and(|command| command == [""]),
            keeps_empty_argument
        );
        let pane = engine.state.pane(*pane).expect("empty pane state");
        assert!(pane.empty);
        assert!(!pane.dead);
    }
}

#[test]
fn split_window_style_and_title_flags_match_pinned_spawn_semantics() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first = context.pane.unwrap();
    engine.state.update_pane_title(first, "origin").unwrap();

    for (name, value) in [
        ("window-style", "bg=colour236"),
        ("window-active-style", "bg=colour237"),
        ("pane-border-style", "fg=colour238"),
        ("pane-active-border-style", "fg=colour239"),
    ] {
        engine
            .execute(&mut context, &command("set-option", &["-w", name, value]))
            .unwrap();
    }
    for (name, value) in [
        ("window-style", "bg=red"),
        ("window-active-style", "bg=blue"),
        ("pane-border-style", "fg=green"),
        ("pane-active-border-style", "fg=yellow"),
    ] {
        engine
            .execute(
                &mut context,
                &command("set-option", &["-p", "-t", &first.to_string(), name, value]),
            )
            .unwrap();
    }

    let detached = engine
        .execute(
            &mut context,
            &command(
                "split-window",
                &[
                    "-d",
                    "-t",
                    &first.to_string(),
                    "-s",
                    "fg=ignored",
                    "-s",
                    "fg=not-a-colour",
                    "-S",
                    "fg=active-first",
                    "-S",
                    "fg=active-last",
                    "-R",
                    "fg=not-a-colour",
                    "-T",
                    "ignored",
                    "-T",
                    r"from=#{pane_id},active=#{pane_active},old=#{pane_title}\tail",
                ],
            ),
        )
        .unwrap();
    let detached_pane = detached
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .unwrap();
    assert_eq!(context.pane, Some(first));
    assert_eq!(
        engine.state.pane(detached_pane).unwrap().title,
        format!(r"from={first},active=1,old=origin\\tail")
    );
    assert_eq!(
        engine.window_style_values(detached_pane),
        WindowStyleValues {
            style: Some("fg=not-a-colour".to_owned()),
            active_style: Some("fg=not-a-colour".to_owned()),
        }
    );
    assert_eq!(
        engine.pane_border_style_values(detached_pane),
        PaneBorderStyleValues {
            border: Some("fg=not-a-colour".to_owned()),
            active_border: Some("fg=active-last".to_owned()),
        }
    );

    let attached = engine
        .execute(
            &mut context,
            &command(
                "split-window",
                &[
                    "-t",
                    &first.to_string(),
                    "-T",
                    "from=#{pane_id},active=#{pane_active},old=#{pane_title}",
                ],
            ),
        )
        .unwrap();
    let attached_pane = attached
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .unwrap();
    assert_eq!(context.pane, Some(attached_pane));
    assert_eq!(
        engine.state.pane(attached_pane).unwrap().title,
        format!("from={first},active=0,old=origin")
    );
    assert_eq!(
        engine.window_style_values(attached_pane),
        WindowStyleValues {
            style: Some("bg=colour236".to_owned()),
            active_style: Some("bg=colour237".to_owned()),
        }
    );
    assert_eq!(
        engine.pane_border_style_values(attached_pane),
        PaneBorderStyleValues {
            border: Some("fg=colour238".to_owned()),
            active_border: Some("fg=colour239".to_owned()),
        }
    );
}

#[test]
fn empty_pane_command_errors_follow_target_resolution() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let windows = engine.state.windows.len();
    let panes = engine
        .state
        .windows
        .values()
        .map(|window| window.panes.len())
        .sum::<usize>();

    for (name, arguments) in [
        ("new-window", &["-E", "echo", "nope"][..]),
        ("split-window", &["-E", "echo", "nope"][..]),
        ("split-window", &["-E", "-l", "bad", "echo", "nope"][..]),
    ] {
        assert!(matches!(
            engine.execute(&mut context, &command(name, arguments)),
            Err(ServerError::InvalidCommand(message))
                if message == "command cannot be given for empty pane"
        ));
    }
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("new-window", &["-t", "missing:", "-E", "echo", "nope"]),
        ),
        Err(ServerError::SessionNotFound(session)) if session == "missing"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("split-window", &["-t", "missing:", "-E", "echo", "nope"]),
        ),
        Err(ServerError::SessionNotFound(session)) if session == "missing"
    ));
    assert_eq!(engine.state.windows.len(), windows);
    assert_eq!(
        engine
            .state
            .windows
            .values()
            .map(|window| window.panes.len())
            .sum::<usize>(),
        panes
    );
}

#[test]
fn view_pane_floats_over_the_right_of_its_chat_and_swaps_what_it_shows() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .unwrap();
    let chat = PaneId(0);
    let window = engine.state.window_for_pane(chat).unwrap();
    let opened = engine
        .execute(
            &mut context,
            &command(
                "view-pane",
                &[
                    "-t",
                    "%0",
                    "-k",
                    "code",
                    "-l",
                    "rust",
                    "-T",
                    "main.rs",
                    "fn main() {}",
                ],
            ),
        )
        .unwrap();
    let view = opened
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .expect("the first view opens a pane");
    let layout = &engine.state.windows[&window].layout;
    assert!(layout.is_floating(view));
    assert_eq!(
        layout.pane_geometry(view),
        Some(CellGeometry {
            sx: 46,
            sy: 22,
            xoff: 33,
            yoff: 1
        })
    );

    engine
        .execute(
            &mut context,
            &command("view-pane", &["-t", "%0", "-k", "markdown", "# Plan"]),
        )
        .unwrap();
    assert_eq!(engine.state.windows[&window].panes.len(), 2);
    assert_eq!(
        engine.state.pane(view).map(|pane| pane.title.as_str()),
        Some("Markdown")
    );
    assert_eq!(
        engine.state.pane(view).map(|pane| &pane.kind),
        Some(&PaneKind::View(ViewDescriptor {
            origin: Some(chat),
            title: "Markdown".to_owned(),
            content: ViewContent::Markdown {
                text: "# Plan".to_owned()
            },
        }))
    );
}

#[test]
fn pane_kind_option_rejects_invalid_combinations_before_creation() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .unwrap();
    for (name, args) in [
        ("split-window", vec!["--kind", "unknown"]),
        ("split-window", vec!["--profile", "work"]),
        (
            "split-window",
            vec!["--kind", "picker", "--profile", "work"],
        ),
        ("split-window", vec!["--kind", "browser", "-I"]),
        ("split-window", vec!["--kind", "picker", "-e", "A=B"]),
        ("split-window", vec!["--kind", "agent"]),
        ("split-window", vec!["--kind", "browser", "one", "two"]),
        ("new-window", vec!["--kind", "picker", "-e", "A=B"]),
        ("new-window", vec!["--kind", "agent"]),
        ("new-pane", vec!["--kind", "agent"]),
        ("new-pane", vec!["--kind", "agent", "gemini", "hi"]),
        ("new-pane", vec!["--kind", "agent", "codex", "--model"]),
        (
            "new-pane",
            vec!["--kind", "agent", "--model", "a b", "codex"],
        ),
        ("split-window", vec!["--model", "m"]),
        ("new-window", vec!["--profile", "work"]),
        ("new-window", vec!["--kind", "browser", "-e", "A=B"]),
    ] {
        let generation = engine.state.generation();
        let error = engine
            .execute(&mut context, &command(name, &args))
            .unwrap_err();
        assert!(
            matches!(error, ServerError::NativeCommandParse(_)),
            "{name} {args:?}: {error}"
        );
        assert_eq!(error.exit_code(), 2);
        assert_eq!(engine.state.generation(), generation);
    }
    let error = engine
        .command_stdin_destination_ready(
            &context,
            &command("split-window", &["--kind", "browser", "-I"]),
        )
        .unwrap_err();
    assert_eq!(error.exit_code(), 2);
}

#[test]
fn agent_kind_reads_flags_on_either_side_of_the_provider_and_queues_the_brief() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let created = engine
        .execute(
            &mut context,
            &command(
                "new-pane",
                &[
                    "-d", "--kind", "agent", "--effort", "high", "codex", "--model", "m1", "fix",
                    "the", "build",
                ],
            ),
        )
        .unwrap();
    let [
        MuxEffect::PaneCreated { pane, .. },
        MuxEffect::AgentBrief {
            pane: briefed,
            text,
        },
        ..,
    ] = created.effects.as_slice()
    else {
        panic!("{:?}", created.effects);
    };
    assert_eq!(briefed, pane);
    assert_eq!(text, "fix the build");
    assert_eq!(
        engine.state.pane(*pane).map(|pane| &pane.kind),
        Some(&PaneKind::Agent(AgentDescriptor {
            provider: AgentProvider::Codex,
            model: Some("m1".to_owned()),
            effort: Some("high".to_owned()),
            ..AgentDescriptor::default()
        }))
    );
}

#[test]
fn pane_kind_splits_share_tmux_placement_and_default_output() {
    for kind in ["terminal", "browser", "picker"] {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(&mut context, &command("new-session", &["-s", "work"]))
            .unwrap();
        let created = engine
            .execute(
                &mut context,
                &command(
                    "split-window",
                    &["--kind", kind, "-h", "-b", "-f", "-l", "20", "-Z", "-P"],
                ),
            )
            .unwrap();
        let pane = context.pane.unwrap();
        let window = context.window.unwrap();
        assert_eq!(engine.state.windows[&window].zoomed_pane, Some(pane));
        engine.state.toggle_zoom(pane).unwrap();
        assert_eq!(engine.pane_geometry(pane), Some((20, 24)));
        assert!(created.effects.iter().any(|effect| matches!(effect,
            MuxEffect::PaneFormatOutput { format, .. } if format == DEFAULT_PANE_CREATION_FORMAT
        )));
        assert!(!created.output.is_empty());
    }
}

#[test]
fn browser_windows_use_profiles_and_tmux_window_options() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .unwrap();
    let created = engine
        .execute(
            &mut context,
            &command(
                "new-window",
                &[
                    "--kind",
                    "browser",
                    "--profile",
                    "work",
                    "-b",
                    "-n",
                    "web",
                    "-P",
                    "-F",
                    "#{pane_id}",
                ],
            ),
        )
        .unwrap();
    let pane = context.pane.unwrap();
    assert_eq!(created.output, format!("{pane}\n"));
    assert_eq!(engine.state.windows[&context.window.unwrap()].index, 0);
    assert_eq!(
        engine.state.pane(pane).unwrap().kind,
        PaneKind::Browser(BrowserDescriptor::single(
            "about:blank".to_owned(),
            "work".to_owned()
        ))
    );
    engine
        .execute(
            &mut context,
            &command(
                "new-window",
                &["--kind", "browser", "-k", "-t", "0", "https://example.com"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine.state.pane(context.pane.unwrap()).unwrap().kind,
        PaneKind::Browser(BrowserDescriptor::single(
            "https://example.com".to_owned(),
            DEFAULT_BROWSER_PROFILE.to_owned()
        ))
    );
}

#[test]
fn new_pane_floats_a_browser_or_a_picker() {
    for kind in ["browser", "picker"] {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(&mut context, &command("new-session", &[]))
            .expect("session");
        let created = engine
            .execute(
                &mut context,
                &command(
                    "new-pane",
                    &["--kind", kind, "-P", "-F", "#{pane_floating_flag}"],
                ),
            )
            .unwrap_or_else(|error| panic!("new-pane --kind {kind}: {error}"));
        assert_eq!(created.output.trim(), "1", "{kind}");
    }
}

#[test]
fn native_splits_print_the_created_pane_and_picker_ids_survive_materialization() {
    for (verb, args, kind) in [
        (
            "split-window",
            vec!["--kind", "browser", "-P", "-F", "pane=#{pane_id}"],
            "browser",
        ),
        (
            "split-window",
            vec!["--kind", "picker", "-P", "-F", "#{pane_id}"],
            "picker",
        ),
    ] {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(&mut context, &command("new-session", &[]))
            .expect("session");
        let created = engine
            .execute(&mut context, &command(verb, &args))
            .expect("split");
        let pane = context.pane.expect("new pane");
        let format = if kind == "browser" {
            "pane=#{pane_id}"
        } else {
            "#{pane_id}"
        };
        let expected = if kind == "browser" {
            format!("pane={pane}\n")
        } else {
            format!("{pane}\n")
        };
        assert_eq!(created.output, expected);
        assert!(created.effects.iter().any(|effect| matches!(
            effect,
            MuxEffect::PaneFormatOutput { pane: printed, format: printed_format, .. }
                if *printed == pane && printed_format == format
        )));
        if kind == "picker" {
            engine
                .execute(&mut context, &command("select-pane-kind", &["terminal"]))
                .expect("materialize");
            assert_eq!(context.pane, Some(pane));
            assert!(matches!(
                &engine.state.pane(pane).expect("same pane").kind,
                PaneKind::Terminal
            ));
        }
    }
}

#[test]
fn a_picker_window_starts_a_command_in_the_chosen_directory() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    engine
        .execute(
            &mut context,
            &command("new-window", &["-t", "work:", "--kind", "picker"]),
        )
        .expect("picker window");
    let picker = context.pane.expect("picker pane");
    assert!(matches!(
        engine.state.pane(picker).map(|pane| &pane.kind),
        Some(PaneKind::Picker { .. })
    ));
    let target = picker.to_string();
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("select-pane-kind", &["-t", &target, "-c", "/tmp", "browser"]),
        ),
        Err(error) if error.to_string().contains("terminal kind")
    ));
    let materialized = engine
        .execute(
            &mut context,
            &command(
                "select-pane-kind",
                &[
                    "-t", &target, "-c", "/srv/zz", "terminal", "claude", "--resume",
                ],
            ),
        )
        .expect("agent start");
    assert!(matches!(
        materialized.effects.first(),
        Some(MuxEffect::PaneMaterialized {
            pane,
            kind: PaneKindSnapshot::Terminal,
            cwd: Some(cwd),
            command: Some(words),
            ..
        }) if *pane == picker && cwd == "/srv/zz" && words == &["claude", "--resume"]
    ));
}

#[cfg(unix)]
#[test]
fn a_leading_tilde_names_the_home_directory() {
    let home = std::env::var("HOME").expect("HOME");
    assert_eq!(expand_home("~"), home);
    assert_eq!(expand_home("~/dev/zz"), format!("{home}/dev/zz"));
    assert_eq!(expand_home("~other/dev"), "~other/dev");
    assert_eq!(expand_home("/srv/zz"), "/srv/zz");
}

#[test]
fn new_panes_preserve_their_id_while_materializing_the_selected_kind() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let source = context.pane.expect("source pane");

    let created = engine
        .execute(
            &mut context,
            &command("split-window", &["--kind", "picker", "-h"]),
        )
        .expect("picker split");
    let picker = context.pane.expect("picker pane");
    assert_ne!(picker, source);
    assert!(matches!(
        engine.state.pane(picker).map(|pane| &pane.kind),
        Some(PaneKind::Picker {
            inherit_cwd_from: Some(donor)
        }) if *donor == source
    ));
    assert!(matches!(
        created.effects.first(),
        Some(MuxEffect::PaneCreated {
            pane,
            kind: PaneKindSnapshot::Picker,
            inherit_cwd_from: None,
            ..
        }) if *pane == picker
    ));

    let materialized = engine
        .execute(
            &mut context,
            &command("select-pane-kind", &["-t", &picker.to_string(), "terminal"]),
        )
        .expect("terminal selection");
    assert!(matches!(
        engine.state.pane(picker).map(|pane| &pane.kind),
        Some(PaneKind::Terminal)
    ));
    assert!(matches!(
        materialized.effects.first(),
        Some(MuxEffect::PaneMaterialized {
            pane,
            kind: PaneKindSnapshot::Terminal,
            inherit_cwd_from: Some(donor),
            ..
        }) if *pane == picker && *donor == source
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("select-pane-kind", &["-t", &picker.to_string(), "browser"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message.contains("not awaiting")
    ));

    engine
        .execute(
            &mut context,
            &command("split-window", &["--kind", "picker", "-v"]),
        )
        .expect("second picker");
    let browser_picker = context.pane.expect("browser picker");
    engine
        .execute(
            &mut context,
            &command(
                "select-pane-kind",
                &["-t", &browser_picker.to_string(), "browser"],
            ),
        )
        .expect("browser selection");
    assert!(matches!(
        engine.state.pane(browser_picker).map(|pane| &pane.kind),
        Some(PaneKind::Browser(browser))
            if browser.url() == "about:blank" && browser.profile == DEFAULT_BROWSER_PROFILE
    ));

    assert!(engine.state.validate().is_ok());
}

#[cfg(unix)]
#[test]
fn agent_selection_materializes_a_picker_with_provider_cwd_and_resume() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "agent"]))
        .expect("session");
    let mut picker = || {
        engine
            .execute(
                &mut context,
                &command("split-window", &["--kind", "picker", "-h"]),
            )
            .expect("picker split");
        context.pane.expect("picker pane").to_string()
    };
    let (tail, option, bare) = (picker(), picker(), picker());
    let mut select = |args: &[&str]| {
        engine.execute(
            &mut ExecutionContext::default(),
            &command("select-pane-kind", args),
        )
    };

    for args in [
        &["-t", tail.as_str(), "agent"][..],
        &["-t", tail.as_str(), "agent", "gemini"],
        &["-t", tail.as_str(), "agent", "claude", "--resume"],
        &["-t", tail.as_str(), "agent", "claude", "--fork", "abc"],
        &[
            "-t",
            tail.as_str(),
            "agent",
            "claude",
            "--resume",
            "abc",
            "x",
        ],
        &[
            "-t",
            tail.as_str(),
            "--resume",
            "a",
            "agent",
            "claude",
            "--resume",
            "b",
        ],
        &["-t", tail.as_str(), "--resume", "abc", "terminal"],
        &["-t", tail.as_str(), "-c", "relative/dir", "agent", "codex"],
        &["-t", tail.as_str(), "agent", "codex", "--resume", "bad\nid"],
    ] {
        assert!(select(args).is_err(), "{args:?}");
    }

    let tail_effect = select(&[
        "-t",
        &tail,
        "-c",
        "/work/repo",
        "agent",
        "claude",
        "--resume",
        "session-1",
    ])
    .expect("agent with a trailing resume");
    let option_effect = select(&["-t", &option, "--resume", "thread-2", "agent", "codex"])
        .expect("agent with a resume option");
    let bare_effect = select(&["-t", &bare, "--account", "default", "agent", "claude-code"])
        .expect("bare agent on the default account");

    let expected = [
        (
            tail_effect,
            AgentDescriptor {
                provider: AgentProvider::ClaudeCode,
                cwd: Some(PathBuf::from("/work/repo")),
                session_id: Some("session-1".to_owned()),
                ..AgentDescriptor::default()
            },
        ),
        (
            option_effect,
            AgentDescriptor {
                provider: AgentProvider::Codex,
                session_id: Some("thread-2".to_owned()),
                ..AgentDescriptor::default()
            },
        ),
        (
            bare_effect,
            AgentDescriptor {
                provider: AgentProvider::ClaudeCode,
                ..AgentDescriptor::default()
            },
        ),
    ];
    for (execution, descriptor) in expected {
        let Some(MuxEffect::PaneMaterialized {
            pane,
            kind: PaneKindSnapshot::Agent(agent),
            command,
            ..
        }) = execution.effects.first()
        else {
            panic!("agent selection materializes the picker: {execution:?}");
        };
        assert_eq!(agent, &descriptor);
        assert_eq!(command, &None);
        assert_eq!(
            engine.state.pane(*pane).map(|pane| &pane.kind),
            Some(&PaneKind::Agent(descriptor))
        );
        assert_eq!(
            engine.state.pane(*pane).map(|pane| pane.title.as_str()),
            Some("agent")
        );
    }
    assert!(engine.state.validate().is_ok());
}

#[cfg(unix)]
#[test]
fn a_terminal_turns_into_an_agent_chat_and_back_keeping_its_pane() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "flip"]))
        .expect("session");
    let pane = context.pane.expect("terminal");
    let target = pane.to_string();
    let window = context.window.expect("window");
    let layout = engine.state.windows[&window].layout.clone();

    let to_chat = engine
        .execute(
            &mut ExecutionContext::default(),
            &command(
                "select-pane-kind",
                &[
                    "-t",
                    &target,
                    "-c",
                    "/work",
                    "--resume",
                    "s-1",
                    "--model",
                    "haiku",
                    "--effort",
                    "low",
                    "--account",
                    "a1",
                    "agent",
                    "claude",
                ],
            ),
        )
        .expect("terminal to chat");
    let descriptor = AgentDescriptor {
        provider: AgentProvider::ClaudeCode,
        cwd: Some(PathBuf::from("/work")),
        session_id: Some("s-1".to_owned()),
        model: Some("haiku".to_owned()),
        effort: Some("low".to_owned()),
        account: Some("a1".to_owned()),
    };
    assert_eq!(
        to_chat.effects.first(),
        Some(&MuxEffect::PaneMaterialized {
            pane,
            kind: PaneKindSnapshot::Agent(descriptor.clone()),
            inherit_cwd_from: Some(pane),
            cwd: Some("/work".to_owned()),
            command: None,
        })
    );
    assert_eq!(
        engine.state.pane(pane).map(|pane| &pane.kind),
        Some(&PaneKind::Agent(descriptor))
    );
    assert_eq!(engine.state.windows[&window].layout, layout);
    assert!(
        engine
            .execute(
                &mut ExecutionContext::default(),
                &command("select-pane-kind", &["-t", &target, "agent", "codex"]),
            )
            .is_err(),
        "a chat is already an agent pane"
    );
    for account in ["A1", "-a", "a/b", &"a".repeat(33)] {
        assert_eq!(
            engine
                .execute(
                    &mut ExecutionContext::default(),
                    &command(
                        "select-pane-kind",
                        &["-t", &target, "--account", account, "agent", "claude"]
                    ),
                )
                .map(|_| ())
                .map_err(|error| error.to_string()),
            Err(if account == "-a" {
                "invalid command: select-pane-kind: --account takes one word".to_owned()
            } else {
                "invalid command: select-pane-kind: --account takes an account id (a-z, 0-9, - and _, starting with a letter or digit, at most 32 bytes) or default".to_owned()
            }),
            "{account}"
        );
    }

    let to_cli = engine
        .execute(
            &mut ExecutionContext::default(),
            &command(
                "select-pane-kind",
                &[
                    "-t", &target, "-c", "/work", "terminal", "/bin/sh", "-c", "claude",
                ],
            ),
        )
        .expect("chat to terminal");
    assert_eq!(
        to_cli.effects.first(),
        Some(&MuxEffect::PaneRespawned {
            pane,
            cwd: Some("/work".to_owned()),
            command: Some(vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                "claude".to_owned()
            ]),
            environment: Vec::new(),
            empty: false,
        })
    );
    assert_eq!(
        engine.state.pane(pane).map(|pane| &pane.kind),
        Some(&PaneKind::Terminal)
    );
    assert_eq!(engine.state.windows[&window].layout, layout);
    assert!(
        engine
            .execute(
                &mut ExecutionContext::default(),
                &command(
                    "select-pane-kind",
                    &["-t", &target, "--model", "x", "terminal"]
                ),
            )
            .is_err()
    );
    assert!(engine.state.validate().is_ok());
}

#[test]
fn a_chat_names_its_pane_and_window_after_its_session_title() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "chats"]))
        .expect("session");
    let pane = context.pane.expect("terminal");
    let window = context.window.expect("window");
    let target = pane.to_string();
    assert!(!engine.update_agent_title(pane, Some("Not a chat")));
    engine
        .execute(
            &mut ExecutionContext::default(),
            &command("select-pane-kind", &["-t", &target, "agent", "claude"]),
        )
        .expect("terminal to chat");
    let names = |engine: &MuxEngine| {
        (
            engine.state.pane(pane).expect("pane").title.clone(),
            engine.state.windows[&window].name.clone(),
        )
    };
    assert_eq!(names(&engine), ("agent".to_owned(), "agent".to_owned()));

    assert!(engine.update_agent_title(pane, Some("Fix the flaky resize test")));
    assert_eq!(
        names(&engine),
        (
            "Fix the flaky resize test".to_owned(),
            "Fix the flaky resize test".to_owned()
        )
    );
    assert!(!engine.update_agent_title(pane, Some("Fix the flaky resize test")));
    assert!(engine.update_agent_title(pane, None));
    assert_eq!(names(&engine), ("agent".to_owned(), "agent".to_owned()));

    engine
        .execute(
            &mut ExecutionContext::default(),
            &command("rename-window", &["-t", &window.to_string(), "notes"]),
        )
        .expect("rename");
    assert!(engine.update_agent_title(pane, Some("Renamed in the CLI")));
    assert_eq!(
        names(&engine),
        ("Renamed in the CLI".to_owned(), "notes".to_owned())
    );
    assert!(engine.state.validate().is_ok());
}

#[test]
fn a_fork_opens_a_new_pane_on_a_copy_of_the_session() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "fork"]))
        .expect("session");
    let source = context.pane.expect("pane");
    engine
        .execute(&mut context, &command("new-window", &[]))
        .expect("window for the fork");
    let pane = context.pane.expect("new window's pane");
    assert_ne!(pane, source);
    let forked = engine
        .execute(
            &mut context,
            &command(
                "select-pane-kind",
                &["--resume", "s-1", "--fork", "--at", "u-2", "agent", "codex"],
            ),
        )
        .expect("fork");
    assert!(matches!(
        forked.effects.as_slice(),
        [
            MuxEffect::PaneMaterialized {
                pane: opened,
                kind: PaneKindSnapshot::Agent(agent),
                ..
            },
            MuxEffect::AgentForked { pane: copied, at },
            ..
        ] if *opened == pane
            && *copied == pane
            && agent.session_id.as_deref() == Some("s-1")
            && at.as_deref() == Some("u-2")
    ));
    assert!(matches!(
        engine.state.pane(source).map(|pane| &pane.kind),
        Some(PaneKind::Terminal)
    ));
    let target = source.to_string();
    for args in [
        &["-t", target.as_str(), "--fork", "agent", "claude"][..],
        &[
            "-t",
            target.as_str(),
            "--resume",
            "s",
            "--at",
            "u",
            "agent",
            "claude",
        ],
        &[
            "-t",
            target.as_str(),
            "--resume",
            "s",
            "--fork",
            "--at",
            "../x",
            "agent",
            "claude",
        ],
        &["-t", target.as_str(), "--fork", "terminal"],
    ] {
        assert!(
            engine
                .execute(
                    &mut ExecutionContext::default(),
                    &command("select-pane-kind", args)
                )
                .is_err(),
            "{args:?}"
        );
    }
}

#[test]
fn editor_materialization_and_restore_path_preserve_the_picker_identity() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "editor"]))
        .expect("session");
    let terminal = context.pane.expect("terminal pane");
    engine
        .execute(
            &mut context,
            &command("split-window", &["--kind", "picker", "-h"]),
        )
        .expect("picker split");
    let editor = context.pane.expect("editor pane");
    let window = context.window.expect("window");
    let layout = engine.state.windows[&window].layout.clone();

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "experimental-editor-pane", "on"]),
        )
        .expect("enable editor panes");
    let materialized = engine
        .execute(
            &mut context,
            &command("select-pane-kind", &["-t", &editor.to_string(), "editor"]),
        )
        .expect("editor selection");
    assert_eq!(context.pane, Some(editor));
    assert_eq!(engine.state.windows[&window].layout, layout);
    assert!(matches!(
        materialized.effects.first(),
        Some(MuxEffect::PaneMaterialized {
            pane,
            kind: PaneKindSnapshot::Editor(descriptor),
            inherit_cwd_from: Some(donor),
            ..
        }) if *pane == editor && *donor == terminal && descriptor.validate().is_ok()
    ));

    let path = absolute_test_path("editor pane.rs")
        .to_string_lossy()
        .into_owned();
    let persisted = engine
        .execute(
            &mut context,
            &command("set-editor-path", &["-t", &editor.to_string(), &path]),
        )
        .expect("persist editor path");
    assert_eq!(persisted.effects, [MuxEffect::SnapshotChanged]);
    assert!(matches!(
        &engine.state.snapshot().sessions[0].windows[0].panes[&editor].kind,
        PaneKindSnapshot::Editor(descriptor)
            if descriptor.path.as_deref() == Some(path.as_str())
                && descriptor.validate().is_ok()
    ));

    for invalid in [
        "relative.rs".to_owned(),
        absolute_test_path("bad\nname.rs")
            .to_string_lossy()
            .into_owned(),
        absolute_test_path(&"x".repeat(16 * 1024))
            .to_string_lossy()
            .into_owned(),
    ] {
        assert!(matches!(
            engine.execute(
                &mut context,
                &command("set-editor-path", &["-t", &editor.to_string(), &invalid],),
            ),
            Err(ServerError::InvalidCommand(_))
        ));
    }

    engine
        .execute(
            &mut context,
            &command("set-editor-path", &["-t", &editor.to_string()]),
        )
        .expect("clear editor path");
    assert!(matches!(
        &engine.state.pane(editor).expect("editor state").kind,
        PaneKind::Editor(descriptor) if descriptor.path.is_none()
    ));
}

#[test]
fn agent_auto_approve_reads_by_default_and_still_takes_the_flag_spellings() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    assert_eq!(
        engine.mux_option_value(MuxOptionKey::AgentAutoApprove),
        "reads"
    );

    for (written, effective) in [("on", "all"), ("off", "off"), ("all", "all")] {
        engine
            .execute(
                &mut context,
                &command("set-option", &["-g", "agent-auto-approve", written]),
            )
            .expect("set the auto-approve tier");
        assert_eq!(
            engine.mux_option_value(MuxOptionKey::AgentAutoApprove),
            effective,
            "{written} sets the {effective} tier"
        );
    }

    assert!(matches!(
        engine.execute(
            &mut context,
            &command("set-option", &["-g", "agent-auto-approve", "sometimes"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message.contains("sometimes")
    ));
}

#[test]
fn simulator_float_sits_centre_right_and_carries_its_device() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-x", "200", "-y", "60"]),
        )
        .unwrap();
    let window = engine.state.window_for_pane(PaneId(0)).unwrap();
    let created = engine
        .execute(
            &mut context,
            &command("new-pane", &["-t", "%0", "--kind", "simulator", "ABC"]),
        )
        .unwrap();
    let pane = created
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .expect("new-pane creates the simulator pane");
    let device = |engine: &MuxEngine| match engine.state.pane(pane).map(|pane| &pane.kind) {
        Some(PaneKind::Simulator(descriptor)) => descriptor.device.clone(),
        other => panic!("expected a simulator pane, got {other:?}"),
    };
    let layout = &engine.state.windows[&window].layout;
    assert!(layout.is_floating(pane));
    assert_eq!(
        layout.pane_geometry(pane),
        Some(CellGeometry {
            sx: 33,
            sy: 34,
            xoff: 166,
            yoff: 13
        })
    );
    assert_eq!(
        engine.format_pane_kind(&pane.to_string()),
        Some("simulator")
    );
    assert_eq!(
        engine.state.snapshot().sessions[0].windows[0].panes[&pane].kind,
        PaneKindSnapshot::Other {
            kind: "simulator".to_owned()
        }
    );
    assert_eq!(engine.state.pane(pane).unwrap().title, "Simulator");
    assert_eq!(device(&engine), "ABC");

    engine
        .execute(
            &mut context,
            &command("set-simulator-device", &["-t", &pane.to_string(), "DEF"]),
        )
        .unwrap();
    assert_eq!(device(&engine), "DEF");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("set-simulator-device", &["-t", "%0", "DEF"]),
            )
            .unwrap_err(),
        ServerError::InvalidCommand("pane %0 is not a simulator".to_owned())
    );
}

#[test]
fn pane_kinds_are_rejected_when_their_flag_is_disabled() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "gated"]))
        .expect("session");
    engine
        .execute(
            &mut context,
            &command("split-window", &["--kind", "picker", "-h"]),
        )
        .expect("picker split");
    let picker = context.pane.expect("picker pane");

    assert_eq!(
        engine.mux_option_value(MuxOptionKey::ExperimentalEditorPane),
        "off"
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("select-pane-kind", &["-t", &picker.to_string(), "editor"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message.contains("experimental-editor-pane")
    ));
    assert!(matches!(
        engine.state.pane(picker).map(|pane| &pane.kind),
        Some(PaneKind::Picker { .. })
    ));

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "experimental-editor-pane", "true"]),
        )
        .expect("enable editor panes");
    assert_eq!(
        engine.mux_option_value(MuxOptionKey::ExperimentalEditorPane),
        "on"
    );
    engine
        .execute(
            &mut context,
            &command("select-pane-kind", &["-t", &picker.to_string(), "editor"]),
        )
        .expect("editor materializes once enabled");
    assert!(matches!(
        engine.state.pane(picker).map(|pane| &pane.kind),
        Some(PaneKind::Editor(_))
    ));
}

#[test]
fn browser_profile_commands_create_and_switch_named_profiles() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .expect("session");
    engine
        .execute(
            &mut context,
            &command(
                "split-window",
                &[
                    "--kind",
                    "browser",
                    "-h",
                    "--profile",
                    "personal",
                    "https://zed.dev",
                ],
            ),
        )
        .expect("profiled browser");

    let browser = context.pane.expect("browser pane");
    assert!(matches!(
        &engine.state.pane(browser).expect("browser state").kind,
        PaneKind::Browser(descriptor) if descriptor.profile == "personal"
    ));
    let switched = engine
        .execute(
            &mut context,
            &command(
                "set-browser-profile",
                &["-t", &browser.to_string(), " Work "],
            ),
        )
        .expect("switch browser profile");
    assert_eq!(switched.effects, [MuxEffect::SnapshotChanged]);
    assert!(matches!(
        &engine.state.pane(browser).expect("browser state").kind,
        PaneKind::Browser(descriptor) if descriptor.profile == "Work"
    ));
}

#[test]
fn split_window_retain_flags_write_the_pins_pane_local_key_retention() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let first = context.pane.expect("first pane");
    let window = context.window.expect("window");

    for (arguments, format) in [
        (vec!["-d", "-k", "-t", "%0"], None),
        (
            vec!["-d", "-m", "gone #{pane_dead_status}", "-t", "%0"],
            Some("gone #{pane_dead_status}"),
        ),
        (vec!["-d", "-k", "-m", "both", "-t", "%0"], Some("both")),
    ] {
        let arguments = arguments
            .into_iter()
            .map(|argument| {
                if argument == "%0" {
                    first.to_string()
                } else {
                    argument.to_owned()
                }
            })
            .collect::<Vec<_>>();
        let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        let execution = engine
            .execute(&mut context, &command("split-window", &arguments))
            .expect("retained split");
        let Some(MuxEffect::PaneCreated { pane, .. }) = execution.effects.first() else {
            panic!("missing pane creation effect");
        };
        let pane = *pane;
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command(
                        "show-options",
                        &["-pv", "-t", &pane.to_string(), "remain-on-exit"]
                    ),
                )
                .expect("remain-on-exit readback")
                .output,
            "key"
        );
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command(
                        "show-options",
                        &["-pv", "-t", &pane.to_string(), "remain-on-exit-format"],
                    ),
                )
                .expect("remain-on-exit-format readback")
                .output,
            format.unwrap_or_default()
        );
        assert!(engine.retain_exited_pane(pane, false).expect("clean exit"));
        assert!(engine.retain_exited_pane(pane, true).expect("failed exit"));
        engine
            .state
            .mark_pane_dead(pane, Some(0))
            .expect("retained pane dies");
        assert!(engine.dead_pane_dismisses_on_key(pane));
        assert!(!engine.dead_pane_dismisses_on_key(first));
        engine
            .execute(
                &mut context,
                &command("kill-pane", &["-t", &pane.to_string()]),
            )
            .expect("kill the retained pane");
    }

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-window-options",
                    &["-v", "-t", "work:0", "remain-on-exit"]
                ),
            )
            .expect("window scope readback")
            .output,
        ""
    );
    assert_eq!(engine.state.windows[&window].panes.len(), 1);

    for arguments in [vec!["-d", "-m"], vec!["-d", "-k", "-m"]] {
        assert!(matches!(
            engine.execute(&mut context, &command("split-window", &arguments)),
            Err(ServerError::CommandParse(message))
                if message == "command split-window: -m expects an argument"
        ));
    }
}

#[test]
fn break_pane_floating_placement_flags_float_the_pane() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    engine
        .execute(&mut context, &command("split-window", &["-d"]))
        .expect("split");
    let window = context.window.expect("window");
    let source_pane = engine.state.windows[&window].pane_order()[1];
    let source = source_pane.to_string();
    engine
        .execute(
            &mut context,
            &command(
                "break-pane",
                &[
                    "-W", "-x", "20", "-y", "6", "-X", "10", "-Y", "3", "-s", &source,
                ],
            ),
        )
        .expect("break-pane -W");
    let state = &engine.state.windows[&window];
    assert!(state.is_floating(source_pane));
    assert_eq!(state.displayed_pane_geometry(source_pane), Some((18, 4)));
    assert_eq!(state.panes.len(), 2);
    assert_eq!(engine.state.windows.len(), 1);
}

#[test]
fn split_window_zoom_follows_the_post_spawn_active_pane() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first = context.pane.unwrap();
    let window = context.window.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-d", "-h"]))
        .unwrap();

    engine
        .execute(
            &mut context,
            &command("resize-pane", &["-Z", "-t", &first.to_string()]),
        )
        .unwrap();
    assert_eq!(engine.state.windows[&window].zoomed_pane, Some(first));
    engine
        .execute(
            &mut context,
            &command("split-window", &["-d", "-h", "-t", &first.to_string()]),
        )
        .unwrap();
    assert_eq!(engine.state.windows[&window].active_pane, first);
    assert_eq!(engine.state.windows[&window].zoomed_pane, None);

    engine
        .execute(
            &mut context,
            &command(
                "split-window",
                &["-d", "-h", "-Z", "-t", &first.to_string()],
            ),
        )
        .unwrap();
    assert_eq!(engine.state.windows[&window].active_pane, first);
    assert_eq!(engine.state.windows[&window].zoomed_pane, Some(first));
    engine
        .execute(
            &mut context,
            &command("resize-pane", &["-Z", "-t", &first.to_string()]),
        )
        .unwrap();

    engine
        .execute(
            &mut context,
            &command("split-window", &["-h", "-Z", "-t", &first.to_string()]),
        )
        .unwrap();
    let created = context.pane.unwrap();
    assert_ne!(created, first);
    assert_eq!(engine.state.windows[&window].active_pane, created);
    assert_eq!(engine.state.windows[&window].zoomed_pane, Some(created));
}

#[test]
fn nonterminal_splits_unzoom_even_when_detached() {
    for (name, reveal_args, detached_args) in [
        (
            "split-window",
            &["--kind", "picker", "-h"][..],
            &["--kind", "picker", "-d", "-h"][..],
        ),
        (
            "split-window",
            &["--kind", "browser", "-h", "https://example.com"][..],
            &["--kind", "browser", "-d", "-h", "https://detached.example"][..],
        ),
    ] {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(&mut context, &command("new-session", &["-s", "work"]))
            .unwrap();
        let first = context.pane.unwrap();
        let window = context.window.unwrap();
        engine
            .execute(&mut context, &command("split-window", &["-d", "-h"]))
            .unwrap();
        engine
            .execute(
                &mut context,
                &command("resize-pane", &["-Z", "-t", &first.to_string()]),
            )
            .unwrap();

        engine
            .execute(&mut context, &command(name, reveal_args))
            .unwrap();
        let revealed = context.pane.unwrap();
        assert_ne!(revealed, first);
        assert_eq!(engine.state.windows[&window].active_pane, revealed);
        assert_eq!(engine.state.windows[&window].zoomed_pane, None);

        engine
            .execute(
                &mut context,
                &command("resize-pane", &["-Z", "-t", &revealed.to_string()]),
            )
            .unwrap();
        let detached = engine
            .execute(&mut context, &command(name, detached_args))
            .unwrap();
        let hidden = match detached.effects.first() {
            Some(MuxEffect::PaneCreated { pane, .. }) => *pane,
            effects => panic!("expected pane creation, got {effects:?}"),
        };
        assert_ne!(hidden, revealed);
        assert_eq!(context.pane, Some(revealed));
        assert_eq!(engine.state.windows[&window].active_pane, revealed);
        assert_eq!(engine.state.windows[&window].zoomed_pane, None);
    }
}

#[test]
fn break_and_join_commands_reparent_mixed_surfaces_and_validate_the_subset() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let terminal = context.pane.unwrap();
    let session = context.session.unwrap();
    let original_window = context.window.unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "split-window",
                &["--kind", "browser", "-h", "https://example.com"],
            ),
        )
        .unwrap();
    let browser = context.pane.unwrap();

    engine
        .execute(
            &mut context,
            &command("breakp", &["-n", "docs", "-s", &browser.to_string()]),
        )
        .unwrap();
    let broken_window = context.window.unwrap();
    assert_ne!(broken_window, original_window);
    assert_eq!(engine.state.windows[&broken_window].name, "docs");
    assert_eq!(
        engine.state.windows[&broken_window].layout.project(),
        zz_protocol::LayoutNode::Pane(browser)
    );

    engine
        .execute(
            &mut context,
            &command(
                "joinp",
                &[
                    "-b",
                    "-h",
                    "-p",
                    "30",
                    "-s",
                    &browser.to_string(),
                    "-t",
                    &terminal.to_string(),
                ],
            ),
        )
        .unwrap();
    assert!(!engine.state.windows.contains_key(&broken_window));
    assert_eq!(context.pane, Some(browser));
    let panes = engine.state.windows[&original_window]
        .layout
        .panes_in_order();
    assert_eq!(panes, [browser, terminal]);
    assert_eq!(
        engine.state.windows[&original_window].pane_order(),
        [terminal, browser]
    );

    engine
        .execute(
            &mut context,
            &command("break-pane", &["-d", "-s", &browser.to_string()]),
        )
        .unwrap();
    let detached_window = engine.state.window_for_pane(browser).unwrap();
    assert_ne!(detached_window, original_window);
    assert_eq!(context.pane, Some(terminal));
    assert_eq!(
        engine.state.sessions[&session].active_window,
        original_window
    );
    engine
        .execute(
            &mut context,
            &command(
                "movep",
                &[
                    "-f",
                    "-v",
                    "-l25%",
                    "-s",
                    &browser.to_string(),
                    "-t",
                    &terminal.to_string(),
                ],
            ),
        )
        .unwrap();
    assert!(!engine.state.windows.contains_key(&detached_window));
    assert_eq!(context.pane, Some(browser));

    engine
        .execute(
            &mut context,
            &command("break-pane", &["-d", "-s", &browser.to_string()]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("new-session", &["-s", "remote"]))
        .unwrap();
    let remote = context.pane.unwrap();
    let remote_session = context.session.unwrap();
    let relocated = engine
        .execute(
            &mut context,
            &command(
                "join-pane",
                &["-s", &browser.to_string(), "-t", &remote.to_string()],
            ),
        )
        .unwrap();
    assert!(relocated.effects.contains(&MuxEffect::PaneRelocated {
        pane: browser,
        from: session,
        to: remote_session,
    }));
    assert!(relocated.effects.contains(&MuxEffect::SnapshotChanged));
    assert_eq!(context.pane, Some(browser));
    assert!(engine.state.validate().is_ok());

    assert!(matches!(
        engine.execute(&mut context, &command("resize-pane", &["-T"])),
        Err(ServerError::UnsupportedCommand(message))
            if message == "resize-pane -T"
    ));
}

#[test]
fn join_pane_length_supports_cells_percent_formats_and_full_window_basis() {
    assert_eq!(
        relocated_pane_geometries("join-pane", &["-h", "-l", "10"]),
        ((69, 24), (10, 24))
    );
    assert_eq!(
        relocated_pane_geometries("join-pane", &["-h", "-l", "25%"]),
        ((59, 24), (20, 24))
    );
    assert_eq!(
        relocated_pane_geometries("join-pane", &["-h", "-l", "#{pane_height}"]),
        ((55, 24), (24, 24))
    );
    assert_eq!(
        relocated_pane_geometries("join-pane", &["-h", "-l", "#{pane_height}%"]),
        ((60, 24), (19, 24))
    );
    assert_eq!(
        relocated_pane_geometries("join-pane", &["-h", "-p", "50", "-l", "10"],),
        ((69, 24), (10, 24))
    );
    assert_eq!(
        relocated_pane_geometries("move-pane", &["-h", "-l", "10"]),
        ((69, 24), (10, 24))
    );

    for command_name in ["join-pane", "move-pane"] {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(&mut context, &command("new-session", &["-s", "work"]))
            .unwrap();
        let target = context.pane.unwrap();
        engine.set_pane_geometry(target, 80, 24);
        engine
            .execute(
                &mut context,
                &command("split-window", &["-d", "-v", "-p", "50"]),
            )
            .unwrap();
        let source = engine
            .execute(
                &mut context,
                &command("new-window", &["-d", "-n", "source"]),
            )
            .unwrap()
            .effects
            .iter()
            .find_map(|effect| match effect {
                MuxEffect::PaneCreated { pane, .. } => Some(*pane),
                _ => None,
            })
            .unwrap();
        engine
            .execute(
                &mut context,
                &command(
                    command_name,
                    &[
                        "-d",
                        "-v",
                        "-f",
                        "-l",
                        "25%",
                        "-s",
                        &source.to_string(),
                        "-t",
                        &target.to_string(),
                    ],
                ),
            )
            .unwrap();
        assert_eq!(
            engine.pane_geometry(source),
            Some((80, 6)),
            "{command_name}"
        );
        assert!(engine.state.validate().is_ok(), "{command_name}");
    }
}

#[test]
fn join_pane_length_preserves_target_and_size_error_precedence() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let target = context.pane.unwrap();
    let source = engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-n", "source"]),
        )
        .unwrap()
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .unwrap();

    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "join-pane",
                &["-l", "bogus", "-s", "missing", "-t", &target.to_string()],
            ),
        ),
        Err(ServerError::PaneNotFound(pane)) if pane == "missing"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "join-pane",
                &["-l", "bogus", "-s", &source.to_string(), "-t", "missing"],
            ),
        ),
        Err(ServerError::PaneNotFound(pane)) if pane == "missing"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "join-pane",
                &[
                    "-l",
                    "bogus",
                    "-s",
                    &target.to_string(),
                    "-t",
                    &target.to_string(),
                ],
            ),
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "source and target panes must be different"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "join-pane",
                &[
                    "-l",
                    "bogus",
                    "-s",
                    &source.to_string(),
                    "-t",
                    &target.to_string(),
                ],
            ),
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "size or position invalid tiled geometry invalid"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "join-pane",
                &[
                    "-l",
                    "1001%",
                    "-s",
                    &source.to_string(),
                    "-t",
                    &target.to_string(),
                ],
            ),
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "size or position invalid tiled geometry too large"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "join-pane",
                &[
                    "-p",
                    "25%",
                    "-s",
                    &source.to_string(),
                    "-t",
                    &target.to_string(),
                ],
            ),
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "size or position invalid tiled geometry invalid"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "move-pane",
                &[
                    "-p",
                    "25",
                    "-s",
                    &source.to_string(),
                    "-t",
                    &target.to_string(),
                ],
            ),
        ),
        Err(ServerError::CommandParse(message))
            if message == "command move-pane: unknown flag -p"
    ));
    assert!(engine.state.validate().is_ok());
}

#[test]
fn join_pane_before_keeps_tmux_list_order_after_the_target() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let target = context.pane.unwrap();
    let target_window = context.window.unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-n", "source"]))
        .unwrap();
    let source = context.pane.unwrap();

    engine
        .execute(
            &mut context,
            &command(
                "join-pane",
                &[
                    "-b",
                    "-v",
                    "-s",
                    &source.to_string(),
                    "-t",
                    &target.to_string(),
                ],
            ),
        )
        .unwrap();

    let window = &engine.state.windows[&target_window];
    assert_eq!(window.layout.panes_in_order(), [source, target]);
    assert_eq!(window.pane_order(), [target, source]);
    assert_eq!(window.layout.pane_geometry(source).unwrap().yoff, 0);
    assert_eq!(window.layout.pane_geometry(source).unwrap().sy, 12);
    assert_eq!(window.layout.pane_geometry(target).unwrap().yoff, 13);
    assert_eq!(window.layout.pane_geometry(target).unwrap().sy, 11);
    assert!(engine.state.validate().is_ok());
}
