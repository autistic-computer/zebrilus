use super::test_support::command;
use super::*;

#[test]
fn kill_filters_select_other_sessions_windows_and_panes() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "work"]))
        .unwrap();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "keep"]))
        .unwrap();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "drop"]))
        .unwrap();

    engine
        .execute(
            &mut context,
            &command(
                "kill-session",
                &[
                    "-a",
                    "-t",
                    "keep",
                    "-f",
                    "#{&&:#{==:#{session_active},},#{==:#{session_name},drop}}",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-sessions", &["-F", "#{session_name}", "-O", "name"]),
            )
            .unwrap()
            .output,
        "keep\nwork"
    );

    engine
        .execute(
            &mut context,
            &command("rename-window", &["-t", "keep:0", "base"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "keep", "-n", "stay"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "keep", "-n", "drop"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "kill-window",
                &[
                    "-a",
                    "-t",
                    "keep:0",
                    "-f",
                    "#{&&:#{==:#{session_active},},#{==:#{window_name},drop}}",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-windows",
                    &["-t", "keep", "-F", "#{window_name}", "-O", "name"],
                ),
            )
            .unwrap()
            .output,
        "base\nstay"
    );

    engine
        .execute(
            &mut context,
            &command("split-window", &["-d", "-t", "keep:stay"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "kill-pane",
                &[
                    "-a",
                    "-t",
                    "keep:stay.0",
                    "-f",
                    "#{&&:#{==:#{session_active},},#{==:#{pane_index},1}}",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-panes", &["-t", "keep:stay", "-F", "#{pane_index}"]),
            )
            .unwrap()
            .output,
        "0"
    );

    for invalid in [
        command("kill-session", &["-t", "keep", "-f", "1"]),
        command("kill-session", &["-a", "-C", "-t", "keep", "-f", "1"]),
        command("kill-window", &["-t", "keep:0", "-f", "1"]),
        command("kill-pane", &["-t", "keep:stay.0", "-f", "1"]),
    ] {
        assert!(matches!(
            engine.execute(&mut context, &invalid),
            Err(ServerError::InvalidCommand(message)) if message == "-f only valid with -a"
        ));
    }
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("kill-window", &["-t", "missing", "-f", "1"]),
        ),
        Err(ServerError::WindowNotFound(target)) if target == "missing"
    ));
}

#[test]
fn select_pane_title_updates_metadata_without_changing_the_active_pane() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &[]))
        .expect("session");
    let first = context.pane.expect("first pane");
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .expect("second pane");
    let second = context.pane.expect("second pane");

    let renamed = engine
        .execute(
            &mut context,
            &command(
                "select-pane",
                &["-t", &first.to_string(), "-T", r"~/src\zz"],
            ),
        )
        .expect("set pane title");
    assert_eq!(
        renamed.effects,
        [MuxEffect::SuppressAfterHook, MuxEffect::SnapshotChanged]
    );
    assert_eq!(engine.state.pane(first).unwrap().title, r"~/src\\zz");
    assert_eq!(context.pane, Some(second));
    assert_eq!(
        engine.state.windows[&context.window.unwrap()].active_pane,
        second
    );

    let unchanged = engine
        .execute(
            &mut context,
            &command(
                "select-pane",
                &["-t", &first.to_string(), "-T", r"~/src\zz"],
            ),
        )
        .expect("same title is a no-op");
    assert_eq!(unchanged.effects, [MuxEffect::SuppressAfterHook]);
    assert_eq!(context.pane, Some(second));

    for title in ["bad\n", "bad\x7f"] {
        let invalid = engine
            .execute(
                &mut context,
                &command("select-pane", &["-t", &first.to_string(), "-T", title]),
            )
            .expect("invalid title is ignored");
        assert_eq!(invalid.effects, [MuxEffect::SuppressAfterHook]);
        assert_eq!(engine.state.pane(first).unwrap().title, r"~/src\\zz");
    }
}

#[test]
fn select_pane_title_expands_from_the_original_directional_target() {
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
        .execute(
            &mut context,
            &command(
                "select-pane",
                &[
                    "-t",
                    &first.to_string(),
                    "-R",
                    "-T",
                    "#{pane_id}:#{pane_title}",
                ],
            ),
        )
        .unwrap();

    assert_eq!(
        engine.state.pane(second).unwrap().title,
        format!("{first}:terminal")
    );
    assert_eq!(context.pane, Some(second));
}

#[test]
fn respawn_empty_is_live_and_can_be_respawned_without_kill() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let pane = context.pane.unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("list-commands", &["respawn-pane"]),)
            .unwrap()
            .output,
        "respawn-pane (respawnp) [-Ek] [-c start-directory] [-e environment] [-t target-pane] [shell-command [argument ...]]"
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("list-commands", &["respawn-window"]),)
            .unwrap()
            .output,
        "respawn-window (respawnw) [-Ek] [-c start-directory] [-e environment] [-t target-window] [shell-command [argument ...]]"
    );
    engine.state.mark_pane_dead(pane, Some(7)).unwrap();

    let empty = engine
        .execute(
            &mut context,
            &command(
                "respawn-pane",
                &["-E", "-t", &pane.to_string(), "printf ready"],
            ),
        )
        .unwrap();
    assert!(matches!(
        empty.effects.as_slice(),
        [MuxEffect::PaneRespawned {
            pane: actual,
            command: Some(command),
            empty: true,
            ..
        }, MuxEffect::SnapshotChanged] if *actual == pane && command.as_slice() == ["printf ready"]
    ));
    let pane_state = engine.state.pane(pane).unwrap();
    assert!(!pane_state.dead);
    assert!(pane_state.empty);

    let respawned = engine
        .execute(
            &mut context,
            &command("respawn-pane", &["-t", &pane.to_string()]),
        )
        .unwrap();
    assert!(matches!(
        respawned.effects.as_slice(),
        [MuxEffect::PaneRespawned {
            pane: actual,
            command: None,
            empty: false,
            ..
        }, MuxEffect::SnapshotChanged] if *actual == pane
    ));
    assert!(!engine.state.pane(pane).unwrap().empty);
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
                        "#{pane_start_command}|#{pane_start_command_list}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "\"printf ready\"|'printf ready'"
    );
}

#[test]
fn respawn_window_keeps_the_first_pane_and_collapses_the_layout() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-v"]))
        .unwrap();
    let window = context.window.unwrap();
    let panes = engine.state.windows[&window].pane_order().to_vec();
    let retained = panes[0];
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("respawn-window", &["-t", &window.to_string()]),
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "respawn window failed: window work:0 still active"
    ));
    for pane in &panes {
        engine.state.mark_pane_dead(*pane, Some(0)).unwrap();
    }

    let execution = engine
        .execute(
            &mut context,
            &command("respawn-window", &["-t", &window.to_string()]),
        )
        .unwrap();
    assert!(matches!(
        execution.effects.as_slice(),
        [MuxEffect::PanesRemoved(removed), MuxEffect::PaneRespawned { pane, .. }, MuxEffect::SnapshotChanged]
            if removed == &panes[1..] && *pane == retained
    ));
    let window_state = &engine.state.windows[&window];
    assert_eq!(window_state.pane_order(), &[retained]);
    assert_eq!(
        window_state.layout.project(),
        zz_protocol::LayoutNode::Pane(retained)
    );
    assert!(!window_state.panes[&retained].dead);
}

#[test]
fn resize_pane_zoom_toggles_and_select_pane_can_preserve_it() {
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
    let window = context.window.unwrap();

    engine
        .execute(&mut context, &command("resizep", &["-Z"]))
        .unwrap();
    assert_eq!(engine.state.windows[&window].zoomed_pane, Some(second));
    engine
        .execute(
            &mut context,
            &command("select-pane", &["-Z", "-t", &first.to_string()]),
        )
        .unwrap();
    assert_eq!(engine.state.windows[&window].active_pane, first);
    assert_eq!(engine.state.windows[&window].zoomed_pane, Some(first));

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &second.to_string()]),
        )
        .unwrap();
    assert_eq!(engine.state.windows[&window].active_pane, second);
    assert_eq!(engine.state.windows[&window].zoomed_pane, None);
}

fn format_of(
    engine: &mut MuxEngine,
    context: &mut ExecutionContext,
    target: &str,
    format: &str,
) -> RawText {
    engine
        .execute(
            context,
            &command("display-message", &["-p", "-t", target, format]),
        )
        .expect("format readback")
        .output
}

#[test]
fn select_pane_mark_moves_toggles_and_dies_like_the_pin() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let first = context.pane.expect("first pane");
    engine
        .execute(&mut context, &command("split-window", &["-d"]))
        .expect("split");
    let window = context.window.expect("window");
    let second = engine.state.windows[&window]
        .pane_order()
        .iter()
        .copied()
        .find(|pane| *pane != first)
        .expect("second pane");

    let marks = |engine: &mut MuxEngine, context: &mut ExecutionContext| {
        (
            format_of(engine, context, &first.to_string(), "#{pane_marked}"),
            format_of(engine, context, &second.to_string(), "#{pane_marked}"),
            format_of(engine, context, &first.to_string(), "#{pane_marked_set}"),
            format_of(engine, context, "work", "#{session_marked}"),
            format_of(engine, context, "work:0", "#{window_marked_flag}"),
            format_of(engine, context, "work:0", "#{window_flags}"),
        )
    };

    assert_eq!(
        marks(&mut engine, &mut context),
        (
            "0".into(),
            "0".into(),
            "0".into(),
            "0".into(),
            "0".into(),
            "*".into()
        )
    );

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-m", "-t", &first.to_string()]),
        )
        .expect("mark first");
    assert_eq!(engine.state.marked_pane(), Some(first));
    assert_eq!(
        marks(&mut engine, &mut context),
        (
            "1".into(),
            "0".into(),
            "1".into(),
            "1".into(),
            "1".into(),
            "*M".into()
        )
    );
    assert_eq!(engine.state.windows[&window].active_pane, first);

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-m", "-t", &second.to_string()]),
        )
        .expect("mark second");
    assert_eq!(engine.state.marked_pane(), Some(second));
    assert_eq!(engine.state.windows[&window].active_pane, first);

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-m", "-t", &second.to_string()]),
        )
        .expect("re-mark second");
    assert_eq!(engine.state.marked_pane(), None);
    assert_eq!(
        format_of(&mut engine, &mut context, "work:0", "#{window_flags}"),
        "*"
    );

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-m", "-t", &first.to_string()]),
        )
        .expect("mark first again");
    engine
        .execute(
            &mut context,
            &command("select-pane", &["-M", "-t", &second.to_string()]),
        )
        .expect("clear mark");
    assert_eq!(engine.state.marked_pane(), None);

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-m", "-t", &first.to_string()]),
        )
        .expect("mark first once more");
    engine
        .execute(
            &mut context,
            &command("kill-pane", &["-t", &first.to_string()]),
        )
        .expect("kill the marked pane");
    assert_eq!(engine.state.marked_pane(), None);
    assert_eq!(
        format_of(
            &mut engine,
            &mut context,
            &second.to_string(),
            "#{pane_marked_set}"
        ),
        "0"
    );
}

#[test]
fn marked_pane_stays_with_the_session_it_was_taken_in_like_the_pin() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "w"]))
        .expect("w");
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "w", "-n", "one"]),
        )
        .expect("one");
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "w", "-n", "two"]),
        )
        .expect("two");
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "other"]),
        )
        .expect("other");
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "other", "-n", "spare"]),
        )
        .expect("spare");
    let marks = |engine: &mut MuxEngine, context: &mut ExecutionContext, target: &str| {
        format_of(
            engine,
            context,
            target,
            "#{pane_marked}:#{pane_marked_set}:#{session_marked}:#{window_marked_flag}",
        )
    };

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-m", "-t", "w:one.0"]),
        )
        .expect("mark w:one");
    engine
        .execute(
            &mut context,
            &command("move-window", &["-d", "-s", "w:one", "-t", "w:7"]),
        )
        .expect("move inside w");
    assert_eq!(
        marks(&mut engine, &mut context, "w:7.0"),
        "1:1:1:1",
        "pinned tmux keeps the mark across a same-session move-window"
    );

    engine
        .execute(
            &mut context,
            &command("move-window", &["-d", "-s", "w:7", "-t", "other:5"]),
        )
        .expect("move to other");
    assert_eq!(
        marks(&mut engine, &mut context, "other:5.0"),
        "0:0:0:0",
        "pinned tmux answers 0:0:0:0 on the moved window's pane after move-window -d -s w:0 -t other:5"
    );
    assert_eq!(engine.state.marked_pane(), None);

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-m", "-t", "w:two.0"]),
        )
        .expect("mark w:two");
    engine
        .execute(
            &mut context,
            &command("swap-window", &["-d", "-s", "w:two", "-t", "other:1"]),
        )
        .expect("swap into other");
    assert_eq!(
        marks(&mut engine, &mut context, "other:1.0"),
        "0:0:0:0",
        "pinned tmux answers 0:0:0:0 on the swapped window's pane after swap-window -d -s w:1 -t other:1"
    );
    assert_eq!(engine.state.marked_pane(), None);
    assert_eq!(
        format_of(&mut engine, &mut context, "w:1.0", "#{pane_marked_set}"),
        "0"
    );
}

#[test]
fn marked_pane_formats_scope_to_the_marked_session_and_window() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "alpha"]))
        .expect("alpha");
    let marked = context.pane.expect("alpha pane");
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-t", "alpha", "-n", "second"]),
        )
        .expect("second window");
    engine
        .execute(&mut context, &command("new-session", &["-s", "beta"]))
        .expect("beta");

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-m", "-t", &marked.to_string()]),
        )
        .expect("mark alpha");

    for (target, session_marked, window_marked, marked_set) in [
        ("alpha:0", "1", "1", "1"),
        ("alpha:second", "1", "0", "1"),
        ("beta:0", "0", "0", "1"),
    ] {
        assert_eq!(
            format_of(&mut engine, &mut context, target, "#{session_marked}"),
            session_marked,
            "session_marked for {target}"
        );
        assert_eq!(
            format_of(&mut engine, &mut context, target, "#{window_marked_flag}"),
            window_marked,
            "window_marked_flag for {target}"
        );
        assert_eq!(
            format_of(&mut engine, &mut context, target, "#{pane_marked_set}"),
            marked_set,
            "pane_marked_set for {target}"
        );
    }
    assert!(format_of(&mut engine, &mut context, "alpha:0", "#{window_flags}").contains('M'));
    assert!(!format_of(&mut engine, &mut context, "alpha:second", "#{window_flags}").contains('M'));
}

#[test]
fn select_pane_mark_skips_zoom_hidden_panes_but_clear_still_lands() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let first = context.pane.expect("first pane");
    engine
        .execute(&mut context, &command("split-window", &["-d"]))
        .expect("split");
    let window = context.window.expect("window");
    let second = engine.state.windows[&window]
        .pane_order()
        .iter()
        .copied()
        .find(|pane| *pane != first)
        .expect("second pane");
    engine
        .execute(
            &mut context,
            &command("resize-pane", &["-Z", "-t", &first.to_string()]),
        )
        .expect("zoom");
    assert_eq!(engine.state.windows[&window].zoomed_pane, Some(first));

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-m", "-t", &second.to_string()]),
        )
        .expect("mark hidden pane");
    assert_eq!(engine.state.marked_pane(), None);

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-m", "-t", &first.to_string()]),
        )
        .expect("mark the zoomed pane");
    assert_eq!(engine.state.marked_pane(), Some(first));

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-M", "-t", &second.to_string()]),
        )
        .expect("clear through a hidden target");
    assert_eq!(engine.state.marked_pane(), None);
}

#[test]
fn select_pane_input_flags_retarget_without_selecting() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let first = context.pane.expect("first pane");
    engine
        .execute(&mut context, &command("split-window", &["-d"]))
        .expect("split");
    let window = context.window.expect("window");
    let second = engine.state.windows[&window]
        .pane_order()
        .iter()
        .copied()
        .find(|pane| *pane != first)
        .expect("second pane");

    for (arguments, first_off, second_off) in [
        (vec!["-d", "-t", &second.to_string()[..]], "0", "1"),
        (vec!["-e", "-t", &second.to_string()[..]], "0", "0"),
        (vec!["-d", "-t", &first.to_string()[..]], "1", "0"),
        (vec!["-e", "-t", &first.to_string()[..]], "0", "0"),
    ] {
        engine
            .execute(&mut context, &command("select-pane", &arguments))
            .expect("input toggle");
        assert_eq!(
            format_of(
                &mut engine,
                &mut context,
                &first.to_string(),
                "#{pane_input_off}"
            ),
            first_off
        );
        assert_eq!(
            format_of(
                &mut engine,
                &mut context,
                &second.to_string(),
                "#{pane_input_off}"
            ),
            second_off
        );
        assert_eq!(engine.state.windows[&window].active_pane, first);
    }

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-d", "-t", &second.to_string()]),
        )
        .expect("disable input");
    engine
        .execute(
            &mut context,
            &command("select-pane", &["-d", "-e", "-t", &second.to_string()]),
        )
        .expect("enable wins over disable");
    assert_eq!(
        format_of(
            &mut engine,
            &mut context,
            &second.to_string(),
            "#{pane_input_off}"
        ),
        "0"
    );

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-D", "-d", "-t", &first.to_string()]),
        )
        .expect("direction resolves before the input flag");
    assert_eq!(
        format_of(
            &mut engine,
            &mut context,
            &second.to_string(),
            "#{pane_input_off}"
        ),
        "1"
    );
    assert_eq!(engine.state.windows[&window].active_pane, first);

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &second.to_string()]),
        )
        .expect("visit second");
    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &first.to_string()]),
        )
        .expect("back to first");
    engine
        .execute(&mut context, &command("select-pane", &["-l", "-e"]))
        .expect("enable input on the last pane");
    assert_eq!(
        format_of(
            &mut engine,
            &mut context,
            &second.to_string(),
            "#{pane_input_off}"
        ),
        "0"
    );
    engine
        .execute(&mut context, &command("select-pane", &["-l", "-d"]))
        .expect("disable input on the last pane");
    assert_eq!(
        format_of(
            &mut engine,
            &mut context,
            &second.to_string(),
            "#{pane_input_off}"
        ),
        "1"
    );
    assert_eq!(engine.state.windows[&window].active_pane, first);
}

#[test]
fn select_pane_style_flag_writes_both_pane_styles_and_then_selects() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let first = context.pane.expect("first pane");
    engine
        .execute(&mut context, &command("split-window", &["-d"]))
        .expect("split");
    let window = context.window.expect("window");
    let second = engine.state.windows[&window]
        .pane_order()
        .iter()
        .copied()
        .find(|pane| *pane != first)
        .expect("second pane");

    let styled = engine
        .execute(
            &mut context,
            &command("select-pane", &["-P", "bg=red", "-t", &second.to_string()]),
        )
        .expect("pane style");
    assert_eq!(
        styled.effects,
        vec![
            MuxEffect::TerminalKnobsChanged {
                window: None,
                pane: Some(second),
            },
            MuxEffect::SnapshotChanged,
        ]
    );
    assert_eq!(engine.state.windows[&window].active_pane, second);
    for name in ["window-style", "window-active-style"] {
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command("show-options", &["-pv", "-t", &second.to_string(), name]),
                )
                .expect("style readback")
                .output,
            "bg=red"
        );
    }

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &first.to_string()]),
        )
        .expect("back to first");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("select-pane", &["-g", "-t", &second.to_string()]),
            )
            .expect("print pane style")
            .output,
        "bg=red"
    );
    assert_eq!(engine.state.windows[&window].active_pane, first);
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("select-pane", &["-g", "-t", &first.to_string()]),
            )
            .expect("print default pane style")
            .output,
        "default"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-w", "-t", "work:0", "window-style", "bg=green"],
            ),
        )
        .expect("window style");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("select-pane", &["-g", "-t", &first.to_string()]),
            )
            .expect("print inherited pane style")
            .output,
        "bg=green"
    );

    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "set-option",
                &["-p", "-t", &first.to_string(), "window-style", "bogus=zzz"],
            ),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "invalid style: bogus=zzz"
    ));
    engine
        .execute(
            &mut context,
            &command(
                "select-pane",
                &["-P", "bogus=zzz", "-t", &first.to_string()],
            ),
        )
        .expect("the pin's -P never validates the style");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("select-pane", &["-g", "-t", &first.to_string()]),
            )
            .expect("print unvalidated pane style")
            .output,
        "bogus=zzz"
    );

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &second.to_string()]),
        )
        .expect("select second");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "select-pane",
                    &["-P", "bg=blue", "-g", "-t", &first.to_string()],
                ),
            )
            .expect("style then print")
            .output,
        "bg=blue"
    );
    assert_eq!(engine.state.windows[&window].active_pane, second);

    engine
        .execute(
            &mut context,
            &command(
                "select-pane",
                &["-m", "-P", "bg=yellow", "-t", &second.to_string()],
            ),
        )
        .expect("mark wins over style");
    assert_eq!(engine.state.marked_pane(), Some(second));
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("select-pane", &["-g", "-t", &second.to_string()]),
            )
            .expect("style untouched by the mark path")
            .output,
        "bg=red"
    );
}

#[test]
fn select_pane_changes_pane_activity_without_touching_window_or_session_selection() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    let first_window = context.window.unwrap();
    let first = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let second = context.pane.unwrap();
    let window_activity = engine.state.windows[&first_window].activity;
    let first_active_point = engine.state.windows[&first_window].panes[&first].active_point;

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &first.to_string()]),
        )
        .unwrap();
    assert_eq!(engine.state.sessions[&session].active_window, first_window);
    assert_eq!(engine.state.windows[&first_window].active_pane, first);
    assert_eq!(context.pane, Some(first));
    assert_eq!(
        engine.state.windows[&first_window].activity,
        window_activity
    );
    assert!(engine.state.windows[&first_window].panes[&first].active_point > first_active_point);

    engine
        .execute(&mut context, &command("new-window", &["-n", "other"]))
        .unwrap();
    let other_window = context.window.unwrap();
    let other_pane = context.pane.unwrap();
    let first_window_activity = engine.state.windows[&first_window].activity;
    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &second.to_string()]),
        )
        .unwrap();
    assert_eq!(engine.state.sessions[&session].active_window, other_window);
    assert_eq!(engine.state.windows[&first_window].active_pane, second);
    assert_eq!(context.window, Some(first_window));
    assert_eq!(context.pane, Some(second));
    assert_eq!(
        engine.state.windows[&first_window].activity,
        first_window_activity
    );

    context = ExecutionContext::for_pane(&engine.state, other_pane).unwrap();
    let generation = engine.state.generation();
    let no_op = engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &second.to_string()]),
        )
        .unwrap();
    assert_eq!(engine.state.sessions[&session].active_window, other_window);
    assert_eq!(context.window, Some(other_window));
    assert_eq!(context.pane, Some(other_pane));
    assert_eq!(engine.state.generation(), generation);
    assert_eq!(no_op.effects, [MuxEffect::SuppressAfterHook]);
}

#[test]
fn select_pane_directions_follow_layout_geometry_from_explicit_targets() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let left = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let right_top = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &[]))
        .unwrap();
    let right_bottom = context.pane.unwrap();

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &left.to_string()]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("select-pane", &["-R"]))
        .unwrap();
    assert_eq!(context.pane, Some(right_bottom));

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &left.to_string()]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &right_top.to_string(), "-D"]),
        )
        .unwrap();
    assert_eq!(context.pane, Some(right_bottom));

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &left.to_string()]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("select-pane", &["-U"]))
        .unwrap();
    assert_eq!(context.pane, Some(left));
}

#[test]
fn last_pane_selects_or_changes_input_without_selecting() {
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
    let window = context.window.unwrap();

    engine
        .execute(&mut context, &command("lastp", &[]))
        .unwrap();
    assert_eq!(context.pane, Some(first));
    engine
        .execute(&mut context, &command("last-pane", &[]))
        .unwrap();
    assert_eq!(context.pane, Some(second));

    engine
        .execute(&mut context, &command("resize-pane", &["-Z"]))
        .unwrap();
    engine
        .execute(&mut context, &command("last-pane", &["-Z"]))
        .unwrap();
    assert_eq!(context.pane, Some(first));
    assert_eq!(engine.state.windows[&window].zoomed_pane, Some(first));

    engine
        .execute(&mut context, &command("last-pane", &["-d", "-Z"]))
        .unwrap();
    assert_eq!(context.pane, Some(first));
    assert_eq!(engine.state.windows[&window].zoomed_pane, Some(first));
    assert!(engine.pane_input_off(second).unwrap());
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &["-p", "-t", &second.to_string(), "#{pane_input_off}"],
                ),
            )
            .unwrap()
            .output,
        "1"
    );

    for flags in [["-d", "-e"], ["-e", "-d"]] {
        engine
            .execute(&mut context, &command("last-pane", &["-d"]))
            .unwrap();
        engine
            .execute(&mut context, &command("last-pane", &flags))
            .unwrap();
        assert!(!engine.pane_input_off(second).unwrap());
        assert_eq!(context.pane, Some(first));
    }

    assert!(matches!(
        engine.execute(
            &mut context,
            &command("last-pane", &["-d", "-t", "missing"]),
        ),
        Err(ServerError::WindowNotFound(target)) if target == "missing"
    ));
}

#[test]
fn last_pane_uses_two_pane_fallback_and_rejects_empty_larger_history() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-d", "-h"]))
        .unwrap();
    engine
        .execute(&mut context, &command("last-pane", &["-d"]))
        .unwrap();
    let other = engine.state.windows[&context.window.unwrap()]
        .panes
        .keys()
        .find(|pane| **pane != first)
        .copied()
        .unwrap();
    assert!(engine.pane_input_off(other).unwrap());
    assert_eq!(context.pane, Some(first));

    engine
        .execute(&mut context, &command("split-window", &["-d", "-v"]))
        .unwrap();
    assert!(matches!(
        engine.execute(&mut context, &command("last-pane", &["-e"])),
        Err(ServerError::InvalidCommand(message)) if message == "no last pane"
    ));
}

#[test]
fn resize_pane_moves_by_cells_when_geometry_is_known() {
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
    let window = context.window.unwrap();

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &first.to_string()]),
        )
        .unwrap();
    engine.set_pane_geometry(first, 100, 50);
    engine
        .execute(&mut context, &command("resize-pane", &["-R", "10"]))
        .unwrap();
    assert_eq!(engine.pane_geometry(first), Some((110, 50)));
    assert_eq!(engine.pane_geometry(second), Some((89, 50)));
    assert_eq!(engine.window_extent(window, Axis::Horizontal), Some(200));
    assert_eq!(engine.window_extent(window, Axis::Vertical), Some(50));
}

#[test]
fn swap_pane_moves_native_layout_slots_and_reports_cross_session_relocation() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "first"]))
        .unwrap();
    let left = context.pane.unwrap();
    let first_session = context.session.unwrap();
    let first_window = context.window.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let middle = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &[]))
        .unwrap();
    let right = context.pane.unwrap();

    engine
        .execute(&mut context, &command("swapp", &["-U"]))
        .unwrap();
    let mut panes = engine.state.windows[&first_window].layout.panes_in_order();
    assert_eq!(panes, [left, right, middle]);
    assert_eq!(context.pane, Some(right));

    engine
        .execute(&mut context, &command("swap-pane", &["-d", "-D"]))
        .unwrap();
    panes = engine.state.windows[&first_window].layout.panes_in_order();
    assert_eq!(panes, [left, middle, right]);
    assert_eq!(context.pane, Some(middle));

    engine
        .execute(&mut context, &command("new-session", &["-s", "second"]))
        .unwrap();
    let target = context.pane.unwrap();
    let second_session = context.session.unwrap();
    let second_window = context.window.unwrap();
    let swapped = engine
        .execute(
            &mut context,
            &command(
                "swap-pane",
                &["-s", &right.to_string(), "-t", &target.to_string()],
            ),
        )
        .unwrap();
    assert_eq!(
        &swapped.effects[..2],
        [
            MuxEffect::PaneRelocated {
                pane: right,
                from: first_session,
                to: second_session,
            },
            MuxEffect::PaneRelocated {
                pane: target,
                from: second_session,
                to: first_session,
            },
        ]
    );
    assert!(swapped.effects.contains(&MuxEffect::SnapshotChanged));
    assert!(
        engine.state.windows[&first_window]
            .panes
            .contains_key(&target)
    );
    assert!(
        engine.state.windows[&second_window]
            .panes
            .contains_key(&right)
    );
    assert_eq!(context.pane, Some(right));
    assert!(engine.state.validate().is_ok());

    assert!(matches!(
        engine.execute(&mut context, &command("swap-pane", &[])),
        Err(ServerError::InvalidCommand(_))
    ));
}

#[test]
fn killing_current_pane_keeps_command_context_in_its_session() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "zero"]))
        .expect("default session");
    let default_session = context.session.expect("default session id");
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("work session");
    let work_session = context.session.expect("work session id");
    let work_window = context.window.expect("work window");
    engine
        .execute(&mut context, &command("split-window", &[]))
        .expect("second work pane");
    let killed = context.pane.expect("active work pane");

    engine
        .execute(
            &mut context,
            &command("kill-pane", &["-t", &killed.to_string()]),
        )
        .expect("kill active work pane");

    assert_ne!(work_session, default_session);
    assert_eq!(context.session, Some(work_session));
    assert_eq!(context.window, Some(work_window));
    assert_eq!(
        context.pane,
        Some(engine.state.windows[&work_window].active_pane)
    );

    engine
        .execute(&mut context, &command("new-window", &["-n", "doomed"]))
        .expect("second work window");
    let doomed_pane = context.pane.expect("doomed pane");
    engine
        .execute(
            &mut context,
            &command("kill-pane", &["-t", &doomed_pane.to_string()]),
        )
        .expect("kill sole pane in second work window");

    assert_eq!(context.session, Some(work_session));
    assert_eq!(context.window, Some(work_window));
    assert_eq!(
        context.pane,
        Some(engine.state.windows[&work_window].active_pane)
    );
}

#[test]
fn layout_commands_cycle_target_restore_and_validate_names() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first = context.pane.unwrap();
    let window = context.window.unwrap();
    for _ in 0..3 {
        engine
            .execute(&mut context, &command("split-window", &["-h"]))
            .unwrap();
    }
    let panes = engine.state.windows[&window].panes.clone();

    let selected = engine
        .execute(
            &mut context,
            &command("selectl", &["-t", &first.to_string(), "even-h"]),
        )
        .unwrap();
    assert!(selected.effects.contains(&MuxEffect::SnapshotChanged));
    assert_eq!(engine.state.windows[&window].panes, panes);
    assert!(matches!(
        engine.state.windows[&window].layout.project(),
        zz_protocol::LayoutNode::Split {
            axis: Axis::Horizontal,
            ..
        }
    ));
    let horizontal = engine.state.windows[&window].layout.clone();

    engine
        .execute(
            &mut context,
            &command("nextl", &["-t", &window.to_string()]),
        )
        .unwrap();
    assert!(matches!(
        engine.state.windows[&window].layout.project(),
        zz_protocol::LayoutNode::Split {
            axis: Axis::Vertical,
            ..
        }
    ));
    engine
        .execute(
            &mut context,
            &command("prevl", &["-t", &window.to_string()]),
        )
        .unwrap();
    assert!(matches!(
        engine.state.windows[&window].layout.project(),
        zz_protocol::LayoutNode::Split {
            axis: Axis::Horizontal,
            ..
        }
    ));

    engine
        .execute(
            &mut context,
            &command("select-layout", &["-t", &window.to_string(), "tiled"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("select-layout", &["-o", "-t", &window.to_string()]),
        )
        .unwrap();
    assert_ne!(engine.state.windows[&window].layout, horizontal);
    assert!(matches!(
        engine.state.windows[&window].layout.project(),
        zz_protocol::LayoutNode::Split {
            axis: Axis::Horizontal,
            ..
        }
    ));
    assert_eq!(context.window, Some(window));
    assert!(engine.state.validate().is_ok());

    assert!(matches!(
        engine.execute(&mut context, &command("select-layout", &["main"])),
        Err(ServerError::InvalidCommand(message)) if message == "malformed layout header: main"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("select-layout", &["b25f,80x24,0,0{40x24,0,0,0}"]),
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "invalid layout checksum: b25f,80x24,0,0{40x24,0,0,0}"
    ));
    assert!(matches!(
        engine.execute(&mut context, &command("next-layout", &["-n"])),
        Err(ServerError::CommandParse(message))
            if message == "command next-layout: unknown flag -n"
    ));
}

#[test]
fn select_layout_applies_serialized_layouts_and_reports_window_layout() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("split-window", &["-h", "-t", "w:0.0"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("split-window", &["-v", "-t", "w:0.1"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "select-layout",
                &[
                    "-t",
                    "w:0",
                    "b78d,120x30,0,0{50x30,0,0,9,69x30,51,0[69x14,51,0,8,69x15,51,15,7]}",
                ],
            ),
        )
        .unwrap();
    let first_dump = r#"{"V":2,"L":{"t":"h","w":120,"h":30,"x":0,"y":0,"c":[{"t":"p","w":50,"h":30,"x":0,"y":0,"l":1,"i":0,"I":"%0"},{"t":"v","w":69,"h":30,"x":51,"y":0,"c":[{"t":"p","w":69,"h":14,"x":51,"y":0,"l":0,"i":1,"I":"%1"},{"t":"p","w":69,"h":15,"x":51,"y":15,"a":true,"i":2,"I":"%2"}]}]}}"#;
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-windows", &["-t", "w", "-F", "#{window_layout}"]),
            )
            .unwrap()
            .output,
        first_dump
    );

    engine
        .execute(&mut context, &command("kill-pane", &["-t", "w:0.1"]))
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "select-layout",
                &[
                    "-t",
                    "w:0",
                    "e7f0,100x20,0,0[100x9,0,0{49x9,0,0,50,50x9,50,0,51},100x10,0,10,52]",
                ],
            ),
        )
        .unwrap();
    let window = context.window.unwrap();
    assert_eq!(
        engine.state.windows[&window].layout.dump(),
        "eaf9,100x20,0,0{49x20,0,0,0,50x20,50,0,2}"
    );

    let before = engine.state.windows[&window].layout.clone();
    assert_eq!(
        engine.execute(
            &mut context,
            &command("select-layout", &["-t", "w:0", "0000,80x24,0,0,0"],),
        ),
        Err(ServerError::InvalidCommand(
            "invalid layout checksum: 0000,80x24,0,0,0".to_owned()
        ))
    );
    assert_eq!(engine.state.windows[&window].layout, before);
    assert!(engine.state.validate().is_ok());
}

#[test]
fn select_layout_restore_without_history_primes_the_next_restore() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let window = context.window.unwrap();
    let panes = engine.state.windows[&window].layout.panes_in_order();
    let original = panes
        .iter()
        .map(|pane| engine.state.windows[&window].layout.pane_geometry(*pane))
        .collect::<Vec<_>>();
    let generation = engine.state.generation();

    let first = engine
        .execute(&mut context, &command("select-layout", &["-o"]))
        .expect("first restore");
    assert!(first.effects.is_empty());
    assert_eq!(engine.state.generation(), generation);

    engine
        .execute(&mut context, &command("resize-pane", &["-x", "20"]))
        .unwrap();
    let resized = panes
        .iter()
        .map(|pane| engine.state.windows[&window].layout.pane_geometry(*pane))
        .collect::<Vec<_>>();
    assert_ne!(resized, original);
    let restored = engine
        .execute(&mut context, &command("select-layout", &["-o"]))
        .expect("second restore");
    let restored_geometry = panes
        .iter()
        .map(|pane| engine.state.windows[&window].layout.pane_geometry(*pane))
        .collect::<Vec<_>>();
    assert_eq!(restored_geometry, original);
    assert_eq!(restored.effects, [MuxEffect::SnapshotChanged]);
}

#[test]
fn select_layout_unzooms_before_success_or_failure() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let window = context.window.unwrap();

    engine
        .execute(&mut context, &command("resize-pane", &["-Z"]))
        .unwrap();
    let layout = engine.state.windows[&window].layout.clone();
    let generation = engine.state.generation();
    assert!(matches!(
        engine.execute(&mut context, &command("select-layout", &["bogus"])),
        Err(ServerError::InvalidCommand(message))
            if message == "malformed layout header: bogus"
    ));
    assert_eq!(engine.state.windows[&window].layout, layout);
    assert_eq!(engine.state.windows[&window].zoomed_pane, None);
    assert_eq!(engine.state.generation(), generation + 1);

    engine
        .execute(&mut context, &command("resize-pane", &["-Z"]))
        .unwrap();
    let generation = engine.state.generation();
    let execution = engine
        .execute(&mut context, &command("select-layout", &[]))
        .unwrap();
    assert_eq!(engine.state.windows[&window].zoomed_pane, None);
    assert_eq!(engine.state.generation(), generation + 1);
    assert_eq!(execution.effects, [MuxEffect::SnapshotChanged]);
}

#[test]
fn resize_pane_unzooms_before_parse_failure() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let window = context.window.unwrap();
    engine
        .execute(&mut context, &command("resize-pane", &["-Z"]))
        .unwrap();
    let layout = engine.state.windows[&window].layout.clone();
    let generation = engine.state.generation();

    assert!(matches!(
        engine.execute(&mut context, &command("resize-pane", &["-x", "abc"])),
        Err(ServerError::InvalidCommand(_))
    ));
    assert_eq!(engine.state.windows[&window].zoomed_pane, None);
    assert_eq!(engine.state.windows[&window].layout, layout);
    assert_eq!(engine.state.generation(), generation + 1);
}

#[test]
fn select_layout_resolves_window_typed_session_targets() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let window = context.window.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-v"]))
        .unwrap();
    assert!(matches!(
        engine.state.windows[&window].layout.project(),
        zz_protocol::LayoutNode::Split {
            axis: Axis::Vertical,
            ..
        }
    ));

    for (target, layout, axis) in [
        ("work", "even-horizontal", Axis::Horizontal),
        ("work:", "even-vertical", Axis::Vertical),
        ("", "even-horizontal", Axis::Horizontal),
    ] {
        engine
            .execute(
                &mut context,
                &command("select-layout", &["-t", target, layout]),
            )
            .unwrap();
        assert!(matches!(
            engine.state.windows[&window].layout.project(),
            zz_protocol::LayoutNode::Split {
                axis: actual,
                ..
            } if actual == axis
        ));
    }
}
