use super::test_support::command;
use super::*;

#[test]
fn session_targets_use_configured_pane_indices() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command("new-session", &["-s", "work", "-n", "win"]),
        )
        .unwrap();
    let session = context.session.unwrap();
    let pane = context.pane.unwrap();
    for (flags, value, target) in [
        ("-gw", "1", "=work:win.1"),
        ("-w", "3", "=work:win.3"),
        ("-g", "2", "=work:win.3"),
    ] {
        engine
            .execute(
                &mut context,
                &command("set-option", &[flags, "pane-base-index", value]),
            )
            .unwrap();
        for name in ["has-session", "list-windows"] {
            engine
                .execute(&mut context, &command(name, &["-t", target]))
                .unwrap();
            assert!(
                engine
                    .execute(&mut context, &command(name, &["-t", "=work:win.0"]))
                    .is_err()
            );
        }
        let index = target.rsplit_once('.').unwrap().1;
        for exact_target in [
            format!("work:=.{index}"),
            format!("=work:=.{index}"),
            format!(":=.{index}"),
            format!("work:=.{pane}"),
            "work:=.".to_owned(),
        ] {
            assert_eq!(
                engine.resolve_session(Some(&exact_target), Some(session)),
                Ok(session),
                "{exact_target}"
            );
        }
        assert_eq!(
            engine.resolve_session(Some("work:=.0"), Some(session)),
            Err(ServerError::PaneNotFound("0".to_owned()))
        );
        assert_eq!(
            engine.resolve_session(Some("work:=.9"), Some(session)),
            Err(ServerError::PaneNotFound("9".to_owned()))
        );
        assert_eq!(
            engine.resolve_session(Some(&pane.to_string()), None),
            Ok(session)
        );
        assert_eq!(
            engine.resolve_session(Some(&format!("work:.{pane}")), None),
            Ok(session)
        );
    }
    engine
        .execute(
            &mut context,
            &command("set-option", &["-uw", "pane-base-index"]),
        )
        .unwrap();
    assert_eq!(
        engine.resolve_session(Some("=work:win.2"), None),
        Ok(session)
    );
    assert!(engine.resolve_session(Some("=work:win.3"), None).is_err());
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "base-index", "5"]),
        )
        .unwrap();
    assert_eq!(engine.resolve_session(Some("=work:0.2"), None), Ok(session));
    assert!(engine.resolve_session(Some("=work:5.2"), None).is_err());
}

#[test]
fn pane_base_index_drives_targets_formats_and_window_inheritance() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let window = context.window.expect("window");
    let first = context.pane.expect("first pane");
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .expect("second pane");
    let second = context.pane.expect("second pane");

    let changed = engine
        .execute(
            &mut context,
            &command("set-window-option", &["-g", "pane-base-index", "1"]),
        )
        .expect("global pane base");
    assert_eq!(changed.effects, [MuxEffect::SnapshotChanged]);
    assert_eq!(engine.pane_index(window, first), Some(1));
    assert_eq!(engine.pane_index(window, second), Some(2));
    assert_eq!(
        engine.state.snapshot().sessions[0].windows[0].pane_base_index,
        1
    );
    assert_eq!(
        engine
            .resolve_pane(Some("work:0.1"), Some(window), Some(second))
            .unwrap(),
        first
    );
    assert!(matches!(
        engine.resolve_pane(Some("work:0.0"), Some(window), Some(second)),
        Err(ServerError::PaneNotFound(target)) if target == "0"
    ));
    assert_eq!(
        engine
            .resolve_pane(Some(&second.to_string()), Some(window), Some(first))
            .unwrap(),
        second
    );

    engine
        .execute(&mut context, &command("select-pane", &["-t", "work:0.1"]))
        .expect("one-based pane target");
    assert_eq!(context.pane, Some(first));
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-panes",
                    &["-t", "work:0.1", "-F", "#{pane_index}:#P:#{window_index}"],
                ),
            )
            .unwrap()
            .output,
        "1:1:0\n2:2:0"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &["-p", "-t", "work:0.2", "#{pane_index}:#P:#{window_index}",],
                ),
            )
            .unwrap()
            .output,
        "2:2:0"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-o", "-t", &window.to_string(), "pane-base-index", "3"],
            ),
        )
        .expect("window pane base");
    assert_eq!(engine.pane_index(window, first), Some(3));
    assert_eq!(
        engine.state.snapshot().sessions[0].windows[0].pane_base_index,
        3
    );
    let quiet = engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-oq", "-t", &window.to_string(), "pane-base-index", "4"],
            ),
        )
        .expect("quiet duplicate override");
    assert!(quiet.effects.is_empty());
    assert_eq!(engine.pane_index(window, first), Some(3));

    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-u", "-t", &window.to_string(), "pane-base-index"],
            ),
        )
        .expect("restore window inheritance");
    assert_eq!(engine.pane_index(window, first), Some(1));
    engine
        .execute(
            &mut context,
            &command("set-window-option", &["-gu", "pane-base-index"]),
        )
        .expect("restore global pane base");
    assert_eq!(engine.pane_index(window, first), Some(0));
    assert_eq!(engine.pane_index(window, second), Some(1));
}

#[test]
fn pane_commands_descend_bare_session_and_window_targets() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "w"]))
        .expect("target session");
    let session = context.session.expect("target session id");
    let window = context.window.expect("target window");
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .expect("initial split");
    let active = context.pane.expect("active target pane");
    engine
        .execute(
            &mut context,
            &command("new-session", &["-d", "-s", "elsewhere"]),
        )
        .expect("current session");

    let session_target = engine
        .execute(
            &mut context,
            &command("split-window", &["-d", "-h", "-t", "w"]),
        )
        .expect("bare session target");
    assert!(matches!(
        session_target.effects.first(),
        Some(MuxEffect::PaneCreated {
            inherit_cwd_from: Some(source),
            ..
        }) if *source == active
    ));

    let window_target = engine
        .execute(
            &mut context,
            &command("split-window", &["-d", "-v", "-t", "w:0"]),
        )
        .expect("bare window target");
    assert!(matches!(
        window_target.effects.first(),
        Some(MuxEffect::PaneCreated {
            inherit_cwd_from: Some(source),
            ..
        }) if *source == active
    ));
    assert_eq!(engine.state.windows[&window].active_pane, active);

    engine
        .execute(&mut context, &command("break-pane", &["-d", "-s", "w:0"]))
        .expect("bare window source");
    let moved_window = engine.state.window_for_pane(active).expect("moved pane");
    assert_ne!(moved_window, window);
    assert_eq!(engine.state.windows[&moved_window].session, session);
}

#[test]
fn commands_survive_their_context_pane_dying_on_another_connection() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let first = context.pane.expect("first pane");
    engine
        .execute(&mut context, &command("split-window", &["-v"]))
        .expect("split");
    let second = context.pane.expect("second pane");
    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &first.to_string()]),
        )
        .expect("select first");
    assert_eq!(context.pane, Some(first));

    let mut watcher = ExecutionContext::default();
    engine
        .execute(
            &mut watcher,
            &command("kill-pane", &["-t", &first.to_string()]),
        )
        .expect("external kill");

    let split = engine
        .execute(&mut context, &command("split-window", &["-v"]))
        .expect("split resolves the live active pane, not the corpse");
    assert!(matches!(
        split.effects.first(),
        Some(MuxEffect::PaneCreated { .. })
    ));
    let third = context.pane.expect("third pane");
    assert_ne!(third, first);

    engine
        .execute(&mut context, &command("select-pane", &["-U"]))
        .expect("directional select from a healed context");
    assert_eq!(context.pane, Some(second));
}

#[test]
fn marked_target_alias_is_a_whole_target_and_names_its_own_miss() {
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

    for target in ["{marked}", "~"] {
        assert!(matches!(
            engine.execute(&mut context, &command("kill-pane", &["-t", target])),
            Err(ServerError::InvalidCommand(message)) if message == "no marked target"
        ));
        assert!(matches!(
            engine.execute(&mut context, &command("select-window", &["-t", target])),
            Err(ServerError::InvalidCommand(message)) if message == "no marked target"
        ));
        assert_eq!(
            engine
                .execute(
                    &mut context,
                    &command(
                        "display-message",
                        &[
                            "-p",
                            "-t",
                            target,
                            "#{session_name}:#{window_index}.#{pane_index}"
                        ],
                    ),
                )
                .expect("marked miss falls through display-message")
                .output,
            ":."
        );
    }

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-m", "-t", &second.to_string()]),
        )
        .expect("mark second");
    for target in ["{marked}", "~"] {
        assert_eq!(
            engine.resolve_pane(Some(target), context.window, context.pane),
            Ok(second),
            "pane slot for {target}"
        );
        assert_eq!(
            engine.resolve_window(Some(target), context.session, context.window),
            Ok(window),
            "window slot for {target}"
        );
        assert_eq!(
            engine
                .state
                .resolve_session(Some(target), context.session)
                .expect("session slot"),
            engine.state.windows[&window].session
        );
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &[
                        "-p",
                        "-t",
                        "{marked}.0",
                        "#{session_name}:#{window_index}.#{pane_index}"
                    ],
                ),
            )
            .expect("a split component is an ordinary name")
            .output,
        ":."
    );
}

#[test]
fn pane_targets_read_the_pins_direction_and_compass_aliases() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    engine
        .execute(&mut context, &command("split-window", &["-d", "-h"]))
        .expect("horizontal split");
    let window = context.window.expect("window");
    let first = context.pane.expect("first pane").to_string();
    engine
        .execute(
            &mut context,
            &command("split-window", &["-d", "-v", "-t", &first]),
        )
        .expect("vertical split");
    let order = engine.state.windows[&window].pane_order().to_vec();
    let (top_left, bottom_left, right) = (order[0], order[1], order[2]);
    assert_eq!(engine.state.windows[&window].active_pane, top_left);

    for (target, expected) in [
        ("top", top_left),
        ("bottom", bottom_left),
        ("left", top_left),
        ("right", right),
        ("top-left", top_left),
        ("top-right", right),
        ("bottom-left", bottom_left),
        ("bottom-right", right),
        ("TOP-RIGHT", right),
        ("{top}", top_left),
        ("{bottom}", bottom_left),
        ("{left}", top_left),
        ("{right}", right),
        ("{top-left}", top_left),
        ("{top-right}", right),
        ("{bottom-left}", bottom_left),
        ("{bottom-right}", right),
        ("{up-of}", bottom_left),
        ("{down-of}", bottom_left),
        ("{left-of}", right),
        ("{right-of}", right),
    ] {
        assert_eq!(
            engine.resolve_pane(Some(target), context.window, context.pane),
            Ok(expected),
            "target {target}"
        );
    }

    engine
        .execute(
            &mut context,
            &command("resize-pane", &["-Z", "-t", &top_left.to_string()]),
        )
        .expect("zoom");
    for target in ["bottom", "bottom-right", "{right}"] {
        assert_eq!(
            engine.resolve_pane(Some(target), context.window, context.pane),
            Ok(top_left),
            "zoomed target {target}"
        );
    }
}

#[test]
fn exact_match_prefix_is_scoped_to_the_session_and_window_slots() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "alpha"]))
        .expect("session");
    engine
        .execute(
            &mut context,
            &command("rename-window", &["-t", "alpha:0", "win0"]),
        )
        .expect("rename");

    for (arguments, message) in [
        (
            vec!["-t", "=alpha", "history-limit", "1234"],
            "no such session: =alpha",
        ),
        (
            vec!["-t", "=alp", "history-limit", "1234"],
            "no such session: =alp",
        ),
        (
            vec!["-t", "nope", "history-limit", "1234"],
            "no such session: nope",
        ),
        (
            vec!["-w", "-t", "=win0", "aggressive-resize", "on"],
            "no such window: =win0",
        ),
        (
            vec!["-p", "-t", "=alpha", "remain-on-exit", "on"],
            "no such pane: =alpha",
        ),
    ] {
        assert!(
            matches!(
                engine.execute(&mut context, &command("set-option", &arguments)),
                Err(ServerError::InvalidCommand(actual)) if actual == message
            ),
            "set-option {arguments:?}"
        );
    }

    assert!(matches!(
        engine.execute(&mut context, &command("kill-pane", &["-t", "=alpha"])),
        Err(ServerError::PaneNotFound(target)) if target == "=alpha"
    ));
    assert!(matches!(
        engine.execute(&mut context, &command("select-pane", &["-t", "=win0"])),
        Err(ServerError::PaneNotFound(target)) if target == "=win0"
    ));
    assert!(matches!(
        engine.execute(&mut context, &command("has-session", &["-t", "=alp"])),
        Err(ServerError::SessionNotFound(target)) if target == "alp"
    ));
    assert!(matches!(
        engine.execute(&mut context, &command("kill-window", &["-t", "=wi"])),
        Err(ServerError::WindowNotFound(target)) if target == "wi"
    ));

    engine
        .execute(
            &mut context,
            &command("set-option", &["-t", "=alpha:", "history-limit", "1234"]),
        )
        .expect("a colon moves the token into the session slot");
    assert_eq!(
        engine.history_limit_for_session(context.session.expect("session")),
        1234
    );
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-t", "=win0", "aggressive-resize", "on"],
            ),
        )
        .expect("set-window-option resolves a window target");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-window-options",
                    &["-v", "-t", "alpha:0", "aggressive-resize"]
                ),
            )
            .expect("aggressive-resize readback")
            .output,
        "on"
    );
    engine
        .execute(&mut context, &command("has-session", &["-t", "=alpha"]))
        .expect("has-session resolves a session target");
}

#[test]
fn relative_pane_targets_and_window_rotation_follow_canonical_pane_order() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first = context.pane.unwrap();
    let window = context.window.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let second = context.pane.unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "split-window",
                &["--kind", "browser", "-h", "https://rotate.example"],
            ),
        )
        .unwrap();
    let browser = context.pane.unwrap();
    assert_eq!(
        engine.state.windows[&window].pane_order(),
        [first, second, browser]
    );

    engine
        .execute(
            &mut context,
            &command("select-pane", &["-t", &first.to_string()]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("selectp", &["-t:.+"]))
        .unwrap();
    assert_eq!(context.pane, Some(second));
    engine
        .execute(&mut context, &command("select-pane", &["-t:.-"]))
        .unwrap();
    assert_eq!(context.pane, Some(first));
    engine
        .execute(&mut context, &command("select-pane", &["-l"]))
        .unwrap();
    assert_eq!(context.pane, Some(second));

    let rotated = engine
        .execute(&mut context, &command("rotatew", &[]))
        .unwrap();
    assert!(rotated.effects.contains(&MuxEffect::SnapshotChanged));
    assert_eq!(
        engine.state.windows[&window].pane_order(),
        [second, browser, first]
    );
    assert_eq!(context.pane, Some(browser));

    engine
        .execute(&mut context, &command("resize-pane", &["-Z"]))
        .unwrap();
    engine
        .execute(&mut context, &command("rotate-window", &["-D", "-Z"]))
        .unwrap();
    assert_eq!(
        engine.state.windows[&window].pane_order(),
        [first, second, browser]
    );
    assert_eq!(context.pane, Some(second));
    assert_eq!(engine.state.windows[&window].zoomed_pane, Some(second));
    assert!(engine.state.validate().is_ok());

    assert!(matches!(
        engine.execute(&mut context, &command("rotate-window", &["-d"])),
        Err(ServerError::CommandParse(message))
            if message == "command rotate-window: unknown flag -d"
    ));
    engine
        .execute(&mut context, &command("select-pane", &["-m"]))
        .expect("mark the zoomed active pane");
    assert_eq!(engine.state.marked_pane(), Some(second));
}
