use super::test_support::command;
use super::*;

fn engine_with_panes() -> (MuxEngine, WindowId, PaneId, PaneId) {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "runtime"]),
        )
        .expect("session");
    let window = context.window.expect("window");
    let active = context.pane.expect("pane");
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("split-window", ["-d"]),
        )
        .expect("split");
    let other = engine.state.windows[&window]
        .pane_order()
        .iter()
        .copied()
        .find(|pane| *pane != active)
        .expect("second pane");
    (engine, window, active, other)
}

fn run(engine: &mut MuxEngine, pane: PaneId, command: &str) -> bool {
    let facts = PaneRuntimeFacts {
        current_command: command.to_owned(),
        ..engine.pane_runtime_facts(pane).cloned().unwrap_or_default()
    };
    engine.set_pane_runtime_facts(pane, facts)
}

#[test]
fn runtime_facts_do_not_move_the_tree_generation() {
    let (mut engine, _, active, _) = engine_with_panes();
    engine
        .execute(
            &mut ExecutionContext::default(),
            &CommandInvocation::new("set-option", ["-g", "automatic-rename", "off"]),
        )
        .expect("disable rename");
    let tree = engine.state.generation();
    assert!(run(&mut engine, active, "vim"));
    assert_eq!(engine.state.generation(), tree);
    assert_eq!(
        engine
            .pane_runtime_facts(active)
            .map(|facts| facts.current_command.as_str()),
        Some("vim")
    );
    assert!(!run(&mut engine, active, "vim"));
    assert_eq!(engine.state.generation(), tree);
}

#[test]
fn a_rename_moves_the_tree_generation() {
    let (mut engine, window, active, _) = engine_with_panes();
    let tree = engine.state.generation();
    assert!(run(&mut engine, active, "vim"));
    assert_eq!(engine.state.windows[&window].name, "vim");
    assert!(engine.state.generation() > tree);
}

#[test]
fn renames_inside_the_name_interval_wait_for_the_deadline() {
    let (mut engine, window, active, other) = engine_with_panes();
    engine.set_automatic_rename_throttle(true);
    assert!(run(&mut engine, active, "vim"));
    assert_eq!(engine.state.windows[&window].name, "vim");
    assert_eq!(engine.next_window_rename_deadline(), None);
    assert!(run(&mut engine, other, "top"));
    assert_eq!(engine.next_window_rename_deadline(), None);
    assert!(run(&mut engine, active, "less"));
    assert_eq!(engine.state.windows[&window].name, "vim");
    let deadline = engine
        .next_window_rename_deadline()
        .expect("rename deferred");
    let mut hooks = CommandHooks::new(engine.format_now());
    let early = deadline
        .checked_sub(NAME_INTERVAL / 2)
        .expect("deadline after the interval");
    assert!(!engine.apply_due_window_renames(early, &mut hooks));
    assert_eq!(engine.state.windows[&window].name, "vim");
    assert!(engine.apply_due_window_renames(deadline, &mut hooks));
    assert_eq!(engine.state.windows[&window].name, "less");
    assert_eq!(engine.next_window_rename_deadline(), None);
}

#[test]
fn output_inside_the_name_interval_leaves_the_name_check_to_the_deadline() {
    let (mut engine, window, active, other) = engine_with_panes();
    engine.set_automatic_rename_throttle(true);
    let mut hooks = CommandHooks::new(engine.format_now());
    let start = Instant::now();
    let facts = |engine: &MuxEngine, command: &str| PaneRuntimeFacts {
        current_command: command.to_owned(),
        ..engine
            .pane_runtime_facts(active)
            .cloned()
            .unwrap_or_default()
    };
    let vim = facts(&engine, "vim");
    assert!(engine.set_pane_runtime_facts_at(active, vim, &mut hooks, start));
    assert_eq!(engine.state.windows[&window].name, "vim");
    engine.note_automatic_rename_output(active, start);
    engine.note_automatic_rename_output(other, start + NAME_INTERVAL / 5);
    assert_eq!(engine.next_window_rename_deadline(), None);
    engine.note_automatic_rename_output(active, start + NAME_INTERVAL / 5);
    assert_eq!(
        engine.next_window_rename_deadline(),
        Some(start + NAME_INTERVAL)
    );
    assert!(
        engine
            .due_window_rename_panes(start + NAME_INTERVAL / 2)
            .is_empty()
    );
    assert_eq!(
        engine.due_window_rename_panes(start + NAME_INTERVAL),
        vec![active]
    );
    assert!(!engine.apply_due_window_renames(start + NAME_INTERVAL, &mut hooks));
    let quiet = start + NAME_INTERVAL * 3;
    engine.note_automatic_rename_output(active, quiet);
    assert_eq!(engine.next_window_rename_deadline(), None);
    let less = facts(&engine, "less");
    assert!(engine.set_pane_runtime_facts_at(active, less, &mut hooks, quiet + NAME_INTERVAL / 5));
    assert_eq!(engine.state.windows[&window].name, "vim");
    assert_eq!(
        engine.next_window_rename_deadline(),
        Some(quiet + NAME_INTERVAL)
    );
}

#[test]
fn an_unthrottled_engine_renames_every_change() {
    let (mut engine, window, active, _) = engine_with_panes();
    assert!(run(&mut engine, active, "vim"));
    assert!(run(&mut engine, active, "less"));
    assert_eq!(engine.state.windows[&window].name, "less");
    assert_eq!(engine.next_window_rename_deadline(), None);
}

#[test]
fn format_monitors_are_visible_to_the_status_tick() {
    let (mut engine, _, _, _) = engine_with_panes();
    assert!(!engine.has_format_monitors());
    engine
        .execute(
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "set-hook",
                ["-B", "@watch:@*:#{window_name}", "set -g @fired yes"],
            ),
        )
        .expect("monitor");
    assert!(engine.has_format_monitors());
}

#[test]
fn presentation_templates_that_read_runtime_facts_are_detected() {
    let (mut engine, window, active, _) = engine_with_panes();
    assert!(!engine.runtime_facts_reach_presentation());
    let mut set = |args: &[&str], expected: bool| {
        engine
            .execute(
                &mut ExecutionContext::default(),
                &CommandInvocation::new("set-option", args.iter().copied()),
            )
            .expect("option");
        assert_eq!(
            engine.runtime_facts_reach_presentation(),
            expected,
            "{args:?}"
        );
    };
    set(&["-g", "status-right", "#{pane_current_command}"], true);
    set(&["-gu", "status-right"], false);
    let window_target = window.to_string();
    set(
        &[
            "-w",
            "-t",
            &window_target,
            "window-status-format",
            "#{@label}",
        ],
        true,
    );
    set(
        &["-wu", "-t", &window_target, "window-status-format"],
        false,
    );
    let pane_target = active.to_string();
    set(
        &[
            "-p",
            "-t",
            &pane_target,
            "pane-border-format",
            "#{b:pane_current_path}",
        ],
        true,
    );
    set(&["-pu", "-t", &pane_target, "pane-border-format"], false);
    set(&["-g", "status-left", "plain #S"], false);
}

#[test]
fn only_clock_driven_window_labels_need_the_status_tick() {
    let (mut engine, _, _, _) = engine_with_panes();
    assert!(!engine.window_labels_follow_the_clock());
    let mut set = |args: &[&str], expected: bool| {
        engine
            .execute(
                &mut ExecutionContext::default(),
                &CommandInvocation::new("set-option", args.iter().copied()),
            )
            .expect("option");
        assert_eq!(
            engine.window_labels_follow_the_clock(),
            expected,
            "{args:?}"
        );
    };
    set(&["-g", "status-right", "%H:%M:%S"], false);
    set(&["-g", "window-status-format", "#I %H:%M"], true);
    set(&["-gu", "window-status-format"], false);
    set(&["-g", "window-status-current-format", "#(date)"], true);
    set(&["-gu", "window-status-current-format"], false);
    set(&["-g", "pane-border-format", "#{t:pane_start_time}"], true);
}

#[test]
fn the_rename_decision_and_the_rename_read_one_clock() {
    let (mut engine, window, active, _) = engine_with_panes();
    engine.set_automatic_rename_throttle(true);
    let mut hooks = CommandHooks::new(engine.format_now());
    let mut set_at = |engine: &mut MuxEngine, command: &str, now: Instant| {
        let facts = PaneRuntimeFacts {
            current_command: command.to_owned(),
            ..engine
                .pane_runtime_facts(active)
                .cloned()
                .unwrap_or_default()
        };
        engine.set_pane_runtime_facts_at(active, facts, &mut hooks, now)
    };
    let start = Instant::now();
    assert!(engine.automatic_rename_due(active, start));
    assert!(set_at(&mut engine, "vim", start));
    assert_eq!(engine.state.windows[&window].name, "vim");
    let early = start + Duration::from_millis(499);
    assert!(!engine.automatic_rename_due(active, early));
    assert!(set_at(&mut engine, "less", early));
    assert_eq!(engine.state.windows[&window].name, "vim");
    let boundary = start + NAME_INTERVAL;
    assert!(engine.automatic_rename_due(active, boundary));
    assert!(set_at(&mut engine, "top", boundary));
    assert_eq!(engine.state.windows[&window].name, "top");
    assert_eq!(engine.next_window_rename_deadline(), None);
}

#[test]
fn daemon_format_facts_feed_runtime_values_and_session_time() {
    let mut engine = MuxEngine::default();
    engine.set_format_server_context("tower.local", "tower", "/tmp/zz.sock", 40);
    engine.set_format_server_identity(41, "501", "fab");
    engine.set_format_now(55);
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let pane = context.pane.unwrap();
    let facts = PaneRuntimeFacts {
        current_command: "fish".to_owned(),
        current_path: "/work/live".to_owned(),
        dead_signal: String::new(),
        reported_path: "/work/reported".to_owned(),
        start_path: "/work/start".to_owned(),
        pid: Some(4242),
        tty: "/dev/ttys007".to_owned(),
    };
    assert!(engine.set_pane_runtime_facts(pane, facts.clone()));
    assert!(!engine.set_pane_runtime_facts(pane, facts));
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &[
                        "-p",
                        "#{pane_current_command}|#{pane_current_path}|#{pane_path}|#{pane_start_path}|#{pane_pid}|#{pane_tty}|#{session_created}|#{cursor_flag}|#{wrap_flag}|#{pid}|#{uid}|#{user}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "fish|/work/live|/work/reported|/work/start|4242|/dev/ttys007|55||1|41|501|fab"
    );
}

#[test]
fn parse_window_name_reads_a_start_command_the_way_the_pin_does() {
    for (input, expected) in [
        ("\"exec sleep 0.2\"", "sleep"),
        ("\"sh -c \\\"sleep 0.1; exit 3\\\"\"", "sh"),
        ("\"\\\"/bin/true\\\" a\"", "\\\\"),
        ("\" true\"", "true"),
        ("\"FOO=1 true\"", "FOO=1"),
        ("/bin/true x \"y z\"", "true"),
        ("-bash", "bash"),
        ("/usr/local/bin/", "bin"),
        ("/", "/"),
        ("''", "''"),
        ("/bin/sh", "sh"),
    ] {
        assert_eq!(parse_window_name(input), expected, "{input}");
    }
}

#[test]
fn explicit_window_names_pin_automatic_rename_off() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first = context.window.unwrap();
    assert!(engine.state.window_automatic_rename(first).unwrap());

    engine
        .execute(&mut context, &command("rename-window", &["manual"]))
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-options",
                    &["-wv", "-t", &first.to_string(), "automatic-rename"],
                ),
            )
            .unwrap()
            .output,
        "off"
    );
    assert!(!engine.state.snapshot().sessions[0].windows[0].automatic_rename);

    engine
        .execute(&mut context, &command("new-window", &["-n", "explicit"]))
        .unwrap();
    let named = context.window.unwrap();
    assert!(!engine.state.window_automatic_rename(named).unwrap());

    engine
        .execute(&mut context, &command("new-window", &[]))
        .unwrap();
    let automatic = context.window.unwrap();
    assert!(engine.state.window_automatic_rename(automatic).unwrap());

    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "named", "-n", "first-window"]),
        )
        .unwrap();
    let first_named = context.window.unwrap();
    assert!(!engine.state.window_automatic_rename(first_named).unwrap());
}

#[test]
fn dead_signal_spelling_matches_the_platform_pin() {
    let term = if cfg!(target_os = "linux") {
        "15"
    } else {
        "term"
    };
    for value in ["SIGTERM", "term", "Terminated", "Terminated: 15"] {
        assert_eq!(tmux_signal_name(value), term);
    }
    assert_eq!(tmux_signal_name(""), "");
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    assert_eq!(tmux_signal_name("15"), term);
    #[cfg(target_os = "linux")]
    for (value, expected) in [
        ("SIGBUS", "7"),
        ("Bus error", "7"),
        ("Trace/breakpoint trap", "5"),
        ("SIGUSR1", "10"),
        ("User defined signal 2", "12"),
        ("SIGSTOP", "19"),
        ("31", "31"),
        ("SIGPWR", "30"),
        ("Power failure", "30"),
        ("SIGSTKFLT", "16"),
        ("Stack fault", "16"),
    ] {
        assert_eq!(tmux_signal_name(value), expected);
    }
    #[cfg(target_os = "macos")]
    for (value, expected) in [("7", "emt"), ("10", "bus"), ("30", "usr1"), ("32", "32")] {
        assert_eq!(tmux_signal_name(value), expected);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn dead_signal_realtime_descriptions_restore_the_signal_number() {
    let first = nix::libc::SIGRTMIN();
    let last = nix::libc::SIGRTMAX();
    for offset in [0, 1, last - first] {
        assert_eq!(
            tmux_signal_name(&format!("Real-time signal {offset}")),
            (first + offset).to_string()
        );
    }
    assert_ne!(
        tmux_signal_name(&format!("Real-time signal {}", last - first + 1)),
        (last + 1).to_string()
    );
}

#[test]
fn runtime_facts_apply_automatic_window_names_and_dead_signals() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let pane = context.pane.unwrap();
    let window = context.window.unwrap();

    assert!(engine.set_pane_runtime_facts(
        pane,
        PaneRuntimeFacts {
            current_command: "sleep".to_owned(),
            ..PaneRuntimeFacts::default()
        },
    ));
    assert_eq!(engine.state.windows[&window].name, "sleep");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-windows", &["-F", "#{window_name}"]),
            )
            .unwrap()
            .output,
        "sleep"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["automatic-rename-format", "PFX-#{pane_current_command}-SFX"],
            ),
        )
        .unwrap();
    assert!(engine.set_pane_runtime_facts(
        pane,
        PaneRuntimeFacts {
            current_command: "fish".to_owned(),
            ..PaneRuntimeFacts::default()
        },
    ));
    assert_eq!(engine.state.windows[&window].name, "PFX-fish-SFX");

    engine
        .execute(
            &mut context,
            &command("set-window-option", &["-u", "automatic-rename-format"]),
        )
        .unwrap();
    engine
        .set_pane_shell(pane, "/usr/local/bin/fish".to_owned())
        .unwrap();
    assert!(
        engine
            .mark_pane_dead(pane, None, Some("Terminated: 15"))
            .unwrap()
    );
    assert_eq!(engine.state.windows[&window].name, "fish[dead]");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    ["-p", "#{pane_dead}:#{pane_dead_status}:#{pane_dead_signal}"].as_slice(),
                ),
            )
            .unwrap()
            .output,
        if cfg!(target_os = "linux") {
            "1::15"
        } else {
            "1::term"
        }
    );

    engine
        .execute(&mut context, &command("rename-window", &["manual"]))
        .unwrap();
    assert!(engine.set_pane_runtime_facts(
        pane,
        PaneRuntimeFacts {
            current_command: "vim".to_owned(),
            ..PaneRuntimeFacts::default()
        },
    ));
    assert_eq!(engine.state.windows[&window].name, "manual");
}

#[test]
fn retained_dead_facts_and_respawn_keep_pane_identity_and_layout() {
    let mut engine = MuxEngine::default();
    engine.set_format_now(1_700_000_123);
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let pane = context.pane.unwrap();
    let window = context.window.unwrap();
    let layout = engine.state.windows[&window].layout.project();

    engine
        .execute(
            &mut context,
            &command("set-window-option", &["-g", "remain-on-exit", "on"]),
        )
        .unwrap();
    engine.mark_pane_dead(pane, Some(7), None).unwrap();
    assert!(engine.retain_exited_pane(pane, false).unwrap());
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &[
                        "-p",
                        "-t",
                        &pane.to_string(),
                        "#{pane_id}:#{pane_dead}:#{pane_dead_status}:#{pane_dead_time}"
                    ],
                ),
            )
            .unwrap()
            .output,
        format!("{pane}:1:7:1700000123")
    );
    assert!(matches!(
        engine
            .execute(
                &mut context,
                &command("send-keys", &["-t", &pane.to_string(), "x"]),
            )
            .unwrap()
            .effects
            .as_slice(),
        [MuxEffect::SendKeys { pane: target, .. }] if *target == pane
    ));

    let respawn = engine
        .execute(
            &mut context,
            &command(
                "respawn-pane",
                &[
                    "-t",
                    &pane.to_string(),
                    "-c",
                    "/tmp",
                    "-e",
                    "ONE=1",
                    "-e",
                    "TWO=2",
                    "printf ready",
                ],
            ),
        )
        .unwrap();
    assert!(matches!(
        respawn.effects.as_slice(),
        [
            MuxEffect::PaneRespawned {
                pane: actual,
                cwd: Some(cwd),
                command: Some(command),
                environment,
                empty: false,
            },
            MuxEffect::SnapshotChanged,
        ] if *actual == pane
            && cwd == "/tmp"
            && command.as_slice() == ["printf ready"]
            && environment == &vec![("ONE".to_owned(), "1".to_owned()), ("TWO".to_owned(), "2".to_owned())]
    ));
    assert_eq!(engine.state.windows[&window].layout.project(), layout);
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &[
                        "-p",
                        "-t",
                        &pane.to_string(),
                        "#{pane_id}:#{pane_dead}:#{pane_dead_status}:#{pane_dead_time}"
                    ],
                ),
            )
            .unwrap()
            .output,
        format!("{pane}:0::")
    );

    assert!(matches!(
        engine.execute(
            &mut context,
            &command("respawn-pane", &["-t", &pane.to_string()]),
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "respawn pane failed: pane work:0.1 still active"
    ));
    assert!(
        engine
            .execute(
                &mut context,
                &command("respawn-pane", &["-k", "-t", &pane.to_string()]),
            )
            .is_ok()
    );
}
