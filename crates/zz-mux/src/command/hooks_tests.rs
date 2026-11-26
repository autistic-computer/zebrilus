use super::test_support::command;
use super::*;

#[test]
fn show_hooks_does_not_expand_forwarded_option_names_twice() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "--", "@##{session_name}", "literal"]),
        )
        .unwrap();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["-g", "@##{session_name}"]),
            )
            .unwrap()
            .output,
        "@#{session_name} literal"
    );
}

#[test]
fn show_hooks_renders_commands_like_the_pinned_cmd_print() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    for args in [
        &["-g", "after-new-window", "run-shell \"echo hi\""] as &[&str],
        &[
            "-g",
            "after-kill-pane",
            "splitw -v -c \"#{pane_current_path}\"",
        ],
        &["-g", "after-select-pane", "display-message -p 'it''s'"],
    ] {
        engine
            .execute(&mut context, &command("set-hook", args))
            .unwrap();
    }
    for (hook, expected) in [
        (
            "after-new-window",
            "after-new-window[0] run-shell \"echo hi\"",
        ),
        (
            "after-kill-pane",
            "after-kill-pane[0] split-window -v -c \"#{pane_current_path}\"",
        ),
        (
            "after-select-pane",
            "after-select-pane[0] display-message -p its",
        ),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("show-hooks", &["-g", hook]))
                .unwrap()
                .output,
            expected
        );
    }
}

#[test]
fn set_hook_command_blocks_normalize_for_every_storage_path() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "after-new-window",
                    "{ display -p typed ; list-sessions }",
                ],
            )
            .with_command_blocks([2]),
        )
        .expect("typed event hook");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["-g", "after-new-window"]),
            )
            .expect("typed event hook readback")
            .output,
        "after-new-window[0] display-message -p typed ; list-sessions"
    );

    for (name, body, expected) in [
        (
            "@same-line",
            "{ display -p first ; list-sessions }",
            "display-message -p first ; list-sessions",
        ),
        (
            "@multi-line",
            "{\n display -p first\n list-sessions\n}",
            "display-message -p first ;; list-sessions",
        ),
    ] {
        engine
            .execute(
                &mut context,
                &CommandInvocation::new("set-hook", ["-g", name, body]).with_command_blocks([2]),
            )
            .expect("typed user hook");
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", &["-g", "-v", name]),)
                .expect("typed user hook readback")
                .output,
            expected
        );
    }

    engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-hook", ["-g", "@empty-typed", "{}"])
                .with_command_blocks([2]),
        )
        .expect("empty typed user hook");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "@empty-typed"]),
            )
            .expect("empty typed user hook readback")
            .output,
        "@empty-typed ''"
    );

    engine
        .execute(
            &mut context,
            &command("set-hook", &["-g", "@quoted", "{ display -p quoted }"]),
        )
        .expect("quoted user hook");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "-v", "@quoted"]),
            )
            .expect("quoted user hook readback")
            .output,
        "{ display -p quoted }"
    );

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-hook",
                [
                    "-g",
                    "default-client-command",
                    "{ display -p forwarded ; neww -d }",
                ],
            )
            .with_command_blocks([2]),
        )
        .expect("forwarded typed option");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-s", "-v", "default-client-command"],),
            )
            .expect("forwarded typed option readback")
            .output,
        "display-message -p forwarded ; new-window -d"
    );
}

#[test]
fn after_command_hook_names_reuse_the_declared_registry() {
    let mut previous = None;
    let mut after_hooks_ended = false;
    for hook in HOOK_NAMES {
        let Some(name) = hook.strip_prefix("after-") else {
            after_hooks_ended = true;
            continue;
        };
        assert!(!after_hooks_ended);
        assert!(previous.is_none_or(|previous| previous < name));
        assert_eq!(MuxEngine::after_command_hook(name), Some(*hook));
        previous = Some(name);
    }
    for name in [
        "",
        "has-session",
        "command-error",
        "display-message-extra",
        "zzzz",
    ] {
        assert_eq!(MuxEngine::after_command_hook(name), None);
    }
    assert_eq!(
        MuxEngine::after_command_hook("new-pane"),
        Some("after-split-window")
    );
}

#[test]
fn effective_hook_presence_preserves_an_empty_session_override() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-d"]))
        .expect("model session");
    let session = context.session;
    assert!(!engine.has_hook_commands(session, "after-display-message"));
    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-g", "after-display-message", "display-message global"],
            ),
        )
        .expect("global hook");
    assert!(engine.has_hook_commands(session, "after-display-message"));
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-hook", ["after-display-message", "{}"])
                .with_command_blocks([1]),
        )
        .expect("empty session hook");
    assert!(!engine.has_hook_commands(session, "after-display-message"));
    assert!(engine.has_hook_commands(None, "after-display-message"));
    assert_eq!(
        engine.hook_commands(session, "after-display-message"),
        Some(Vec::new())
    );
    engine
        .execute(
            &mut context,
            &command("set-hook", &["-u", "after-display-message"]),
        )
        .expect("remove session override");
    assert!(engine.has_hook_commands(session, "after-display-message"));
    assert!(!engine.has_hook_commands(session, "missing-hook"));
}

#[test]
fn set_hook_empty_builtin_values_follow_array_assignment() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("hook target session");
    let session = context.session.expect("hook target session");

    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-g", "after-select-window", "display-message baseline"],
            ),
        )
        .expect("baseline unindexed hook");
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-hook", ["-g", "after-select-window", "{}"])
                .with_command_blocks([2]),
        )
        .expect("empty unindexed hook");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["-g", "after-select-window"]),
            )
            .expect("cleared unindexed hook")
            .output,
        "after-select-window"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-g", "after-select-window[7]", "display-message indexed"],
            ),
        )
        .expect("baseline indexed hook");
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-hook", ["-ga", "after-select-window", "{}"])
                .with_command_blocks([2]),
        )
        .expect("empty appended hook");
    let hooks = engine
        .hook_array(TmuxOptionTarget::GlobalSession, "after-select-window")
        .expect("global hook array");
    assert_eq!(hooks.len(), 1);
    assert!(hooks.contains_key(&ArrayIndex::Numeric(7)));

    engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-hook", ["-g", "after-select-window[8]", "{}"])
                .with_command_blocks([2]),
        )
        .expect("empty indexed hook");
    let hooks = engine
        .hook_array(TmuxOptionTarget::GlobalSession, "after-select-window")
        .expect("global hook array");
    assert_eq!(hooks.len(), 2);
    assert!(
        hooks
            .get(&ArrayIndex::Numeric(8))
            .is_some_and(Vec::is_empty)
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["-g", "after-select-window[8]"]),
            )
            .expect("empty indexed hook readback")
            .output,
        "after-select-window[8] "
    );

    engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-hook", ["-a", "after-select-window", "{}"])
                .with_command_blocks([2]),
        )
        .expect("empty local append");
    assert!(
        engine
            .hook_array(TmuxOptionTarget::Session(session), "after-select-window")
            .is_some_and(BTreeMap::is_empty)
    );
    assert_eq!(
        engine.event_hook_commands(&context, "after-select-window"),
        Some(Vec::new())
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-g", "after-new-window", "display-message inherited"],
            ),
        )
        .expect("inherited hook");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "set-hook",
                    &["-a", "after-new-window", "{ display-message invalid }"],
                ),
            )
            .expect_err("invalid local append")
            .tmux_message(),
        "syntax error"
    );
    assert!(
        engine
            .hook_array(TmuxOptionTarget::Session(session), "after-new-window")
            .is_some_and(BTreeMap::is_empty)
    );
    assert_eq!(
        engine.event_hook_commands(&context, "after-new-window"),
        Some(Vec::new())
    );
}

#[test]
fn set_hook_storage_constructs_aliases_at_the_typed_or_string_boundary() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("hook target session");
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-s", "command-alias[70]", "hook10g=display-message -p"],
            ),
        )
        .expect("initial hook alias");

    engine
        .execute(
            &mut context,
            &command("set-hook", &["-g", "after-new-window", "hook10g string"]),
        )
        .expect("string event hook");
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-hook", ["-g", "@typed-alias", "{ hook10g typed }"])
                .with_command_blocks([2]),
        )
        .expect("typed user hook alias");
    engine
        .execute(
            &mut context,
            &command("set-hook", &["-g", "@string-alias", "hook10g deferred"]),
        )
        .expect("string user hook alias");

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &[
                    "-s",
                    "command-alias[70]",
                    "hook10g=set-environment -g HOOK10G",
                ],
            ),
        )
        .expect("replacement hook alias");

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["-g", "after-new-window"]),
            )
            .expect("frozen string event hook")
            .output,
        "after-new-window[0] display-message -p string"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "-v", "@typed-alias"]),
            )
            .expect("frozen typed user hook")
            .output,
        "display-message -p typed"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "-v", "@string-alias"]),
            )
            .expect("deferred string user hook")
            .output,
        "hook10g deferred"
    );

    let fired = engine
        .execute(
            &mut context,
            &command("set-hook", &["-g", "-R", "@string-alias"]),
        )
        .expect("run deferred string user hook");
    let expected =
        vec![parse_hook_commands(&engine, "hook10g deferred").expect("current string hook alias")];
    let commands = fired
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::RunHook { commands, .. } => Some(commands),
            _ => None,
        })
        .expect("deferred user hook effect");
    assert_eq!(commands, &expected);
}

#[test]
fn set_hook_block_failures_follow_hook_replacement_order() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-g", "after-new-window", "display-message preserved"],
            ),
        )
        .expect("baseline event hook");

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "set-hook",
                    &["-g", "after-new-window", "{ display -p quoted }"],
                ),
            )
            .expect_err("quoted event hook block")
            .tmux_message(),
        "syntax error"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["-g", "after-new-window"]),
            )
            .expect("cleared event hook")
            .output,
        "after-new-window"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-g", "after-new-window[3]", "display-message indexed"],
            ),
        )
        .expect("indexed event hook");
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "set-hook",
                    &["-g", "after-new-window[3]", "{ display -p quoted }",],
                ),
            )
            .expect_err("quoted indexed event hook block")
            .tmux_message(),
        "syntax error"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &CommandInvocation::new(
                    "set-hook",
                    ["-g", "after-new-window", "{ list-sessions unexpected }",],
                )
                .with_command_blocks([2]),
            )
            .expect_err("invalid typed event hook")
            .tmux_message(),
        "command list-sessions: too many arguments (need at most 0)"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &CommandInvocation::new(
                    "set-hook",
                    [
                        "after-new-window",
                        "display-message replacement",
                        "{ display-message extra }",
                    ],
                )
                .with_command_blocks([2]),
            )
            .expect_err("typed extra hook argument")
            .tmux_message(),
        "command set-hook: argument 3 must be \"string\""
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["-g", "after-new-window"]),
            )
            .expect("preserved event hook")
            .output,
        "after-new-window[3] display-message indexed"
    );

    let ignored_typed = CommandInvocation::new(
        "set-hook",
        ["-gR", "after-new-window", "{ list-sessions unexpected }"],
    )
    .with_command_blocks([2]);
    assert_eq!(
        engine
            .execute(&mut context, &ignored_typed)
            .expect_err("typed ignored hook value")
            .tmux_message(),
        "command list-sessions: too many arguments (need at most 0)"
    );
    let ignored_quoted = engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-gR", "after-new-window", "{ list-sessions unexpected }"],
            ),
        )
        .expect("quoted ignored hook value");
    let expected =
        vec![parse_hook_commands(&engine, "display-message indexed").expect("stored indexed hook")];
    assert!(matches!(
        ignored_quoted.effects.as_slice(),
        [MuxEffect::RunHook { commands, .. }]
            if commands == &expected
    ));

    let monitor = CommandInvocation::new(
        "set-hook",
        [
            "-g",
            "-B",
            "@monitor:@*:#{window_name}",
            "{ display-message stored }",
        ],
    )
    .with_command_blocks([3]);
    engine
        .execute(&mut context, &monitor)
        .expect("format monitor arms");
    assert_eq!(
        engine
            .execute(&mut context, &command("show-hooks", &["-g", "-B"]))
            .expect("monitor readback")
            .output,
        "@monitor:@*:#{window_name}"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "-v", "@monitor"])
            )
            .expect("monitor body readback")
            .output,
        "display-message stored"
    );
}

#[test]
fn hooks_store_show_and_override_in_numeric_first_order() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.expect("session");
    let window = context.window.expect("window");
    let pane = context.pane.expect("pane");

    let global = engine
        .execute(&mut context, &command("show-hooks", &["-g"]))
        .unwrap()
        .output;
    assert_eq!(
        global.lines().collect::<Vec<_>>(),
        MuxEngine::hook_names_for_target(TmuxOptionTarget::GlobalSession)
    );
    assert_eq!(global.lines().count(), 62);
    assert_eq!(
        global
            .lines()
            .filter(|name| name.starts_with("client-"))
            .collect::<Vec<_>>(),
        [
            "client-active",
            "client-attached",
            "client-created",
            "client-closed",
            "client-detached",
            "client-focus-in",
            "client-focus-out",
            "client-resized",
            "client-session-changed",
            "client-light-theme",
            "client-dark-theme",
        ]
    );
    let global_window = engine
        .execute(&mut context, &command("show-hooks", &["-g", "-w"]))
        .unwrap()
        .output;
    assert_eq!(
        global_window.lines().collect::<Vec<_>>(),
        MuxEngine::hook_names_for_target(TmuxOptionTarget::GlobalWindow)
    );
    assert_eq!(global_window.lines().count(), 27);
    assert_eq!(
        engine
            .execute(&mut context, &command("show-hooks", &[]))
            .unwrap()
            .output,
        ""
    );

    for args in [
        &["after-select-window", "display-message zero"] as &[&str],
        &["-a", "after-select-window", "display-message one"],
        &["after-select-window[named]", "display-message named"],
    ] {
        engine
            .execute(&mut context, &command("set-hook", args))
            .unwrap();
    }
    let plain = engine
        .execute(&mut context, &command("show-options", &["-g"]))
        .unwrap()
        .output;
    assert!(
        plain
            .lines()
            .all(|line| !line.starts_with("after-select-window"))
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["after-select-window"]),
            )
            .unwrap()
            .output,
        "after-select-window[0] display-message zero\n\
             after-select-window[1] display-message one\n\
             after-select-window[named] display-message named"
    );

    engine
        .execute(
            &mut context,
            &command("set-hook", &["-u", "after-select-window[0]"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-a", "after-select-window", "display-message reused"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["after-select-window[1]", "display-message replaced"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["after-select-window"]),
            )
            .unwrap()
            .output,
        "after-select-window[0] display-message reused\n\
             after-select-window[1] display-message replaced\n\
             after-select-window[named] display-message named"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-g", "after-select-window", "display-message global"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .hook_commands(Some(session), "after-select-window")
            .expect("session hook")
            .len(),
        3
    );
    engine
        .execute(
            &mut context,
            &command("set-hook", &["-u", "after-select-window"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .hook_commands(Some(session), "after-select-window")
            .expect("global fallback"),
        [parse_hook_commands(&engine, "display-message global").unwrap()]
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-w", "after-select-window", "display-message window"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-p", "after-select-window", "display-message pane"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-hooks", &["-w", "after-select-window"]),
            )
            .unwrap()
            .output,
        "after-select-window[0] display-message pane"
    );

    engine
        .execute(
            &mut context,
            &command("set-hook", &["-w", "pane-died", "display-message window"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-hook", &["-p", "pane-died", "display-message pane"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-w", "window-renamed", "display-message renamed"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("show-hooks", &["-w"]))
            .unwrap()
            .output,
        "pane-died[0] display-message window\n\
             window-renamed[0] display-message renamed"
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("show-hooks", &["-p"]))
            .unwrap()
            .output,
        "pane-died[0] display-message pane"
    );
    assert_eq!(
        engine
            .hook_array(TmuxOptionTarget::Window(window), "pane-died")
            .expect("window hook")
            .len(),
        1
    );
    assert_eq!(
        engine
            .hook_array(TmuxOptionTarget::Pane(pane), "pane-died")
            .expect("pane hook")
            .len(),
        1
    );

    engine
        .execute(
            &mut context,
            &command("set-hook", &["-g", "alert-b", "display-message alert"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("show-hooks", &["-g", "alert-bell"]),)
            .unwrap()
            .output,
        "alert-bell[0] display-message alert"
    );
    engine
        .execute(
            &mut context,
            &command("set-hook", &["-gu", "alert-bell[0]"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("show-hooks", &["-g", "alert-bell"]),)
            .unwrap()
            .output,
        "alert-bell"
    );
    engine
        .execute(
            &mut context,
            &command("set-hook", &["-g", "alert-bell", "display-message alert"]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("set-hook", &["-gu", "alert-bell"]))
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("show-hooks", &["-g", "alert-bell"]),)
            .unwrap()
            .output,
        "alert-bell"
    );
}

#[test]
fn global_hook_listings_filter_and_route_by_declared_scope() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-g", "pane-died", "display-message global-window"],
            ),
        )
        .unwrap();

    let session_hooks = engine
        .execute(&mut context, &command("show-hooks", &["-g"]))
        .unwrap()
        .output;
    assert_eq!(session_hooks.lines().count(), 62);
    assert!(
        !session_hooks
            .lines()
            .any(|line| line.starts_with("pane-died"))
    );

    let window_hooks = engine
        .execute(&mut context, &command("show-hooks", &["-g", "-w"]))
        .unwrap()
        .output;
    assert_eq!(window_hooks.lines().count(), 27);
    assert!(
        window_hooks
            .lines()
            .any(|line| line == "pane-died[0] display-message global-window")
    );
    assert!(!engine.global_hooks.contains_key("pane-died"));
    assert_eq!(
        engine
            .global_window_hooks
            .get("pane-died")
            .expect("global window hook")
            .len(),
        1
    );
}

#[test]
fn show_options_h_lists_hook_catalogues_in_table_order_and_scope() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    let plain = engine
        .execute(&mut context, &command("show-options", &["-g"]))
        .unwrap()
        .output;
    assert!(plain.lines().all(|line| !HOOK_NAMES.contains(&line)));

    for (args, target, count, inherited) in [
        (
            &["-g", "-H"] as &[&str],
            TmuxOptionTarget::GlobalSession,
            62,
            false,
        ),
        (
            &["-g", "-w", "-H"],
            TmuxOptionTarget::GlobalWindow,
            27,
            false,
        ),
        (
            &["-A", "-H"],
            TmuxOptionTarget::Session(context.session.unwrap()),
            62,
            true,
        ),
        (
            &["-p", "-A", "-H"],
            TmuxOptionTarget::Pane(context.pane.unwrap()),
            19,
            true,
        ),
    ] {
        let output = engine
            .execute(&mut context, &command("show-options", args))
            .unwrap()
            .output;
        let expected = MuxEngine::hook_names_for_target(target)
            .into_iter()
            .map(|name| {
                if inherited {
                    format!("{name}*")
                } else {
                    name.to_owned()
                }
            })
            .collect::<Vec<_>>();
        let actual = output
            .lines()
            .filter(|line| {
                HOOK_NAMES.iter().any(|name| {
                    *line == *name || line.strip_suffix('*').is_some_and(|line| line == *name)
                })
            })
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(actual.len(), count);
        assert_eq!(actual, expected);
        assert_eq!(
            &output.lines().rev().take(count).collect::<Vec<_>>(),
            &expected
                .iter()
                .rev()
                .map(String::as_str)
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-A", "after-capture-pane"]),
            )
            .unwrap()
            .output,
        "after-capture-pane"
    );

    let server = engine
        .execute(&mut context, &command("show-options", &["-s", "-H"]))
        .unwrap()
        .output;
    assert!(server.lines().all(|line| !HOOK_NAMES.contains(&line)));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("show-window-options", &["-H"]),
        ),
        Err(ServerError::CommandParse(message))
            if message == "command show-window-options: unknown flag -H"
    ));
    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["-g", "-H", "-v"]))
            .unwrap()
            .output,
        engine
            .execute(&mut context, &command("show-options", &["-g", "-v"]))
            .unwrap()
            .output
    );
}

#[test]
fn show_options_hook_queries_preserve_indices_inheritance_and_array_shadowing() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    for args in [
        &["-g", "after-bind-key[0]", "display-message global-zero"] as &[&str],
        &[
            "-g",
            "after-bind-key[named]",
            "display-message global-named",
        ],
    ] {
        engine
            .execute(&mut context, &command("set-hook", args))
            .unwrap();
    }
    assert!(
        engine
            .execute(&mut context, &command("show-options", &["-g"]))
            .unwrap()
            .output
            .lines()
            .all(|line| !line.starts_with("after-bind-key"))
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "after-bind-key"]),
            )
            .unwrap()
            .output,
        "after-bind-key[0] display-message global-zero\n\
             after-bind-key[named] display-message global-named"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "-H", "after-bind-key"]),
            )
            .unwrap()
            .output,
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "after-bind-key"]),
            )
            .unwrap()
            .output
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "-v", "after-bind-key[named]"]),
            )
            .unwrap()
            .output,
        "display-message global-named"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["after-bind-key[local]", "display-message local"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-A", "after-bind-key"]),
            )
            .unwrap()
            .output,
        "after-bind-key[local] display-message local"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-A", "after-bind-key[named]"]),
            )
            .unwrap()
            .output,
        "after-bind-key[named] "
    );
    engine
        .execute(
            &mut context,
            &command("set-hook", &["-u", "after-bind-key[local]"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-A", "after-bind-key"]),
            )
            .unwrap()
            .output,
        "after-bind-key"
    );
    engine
        .execute(
            &mut context,
            &command("set-hook", &["-u", "after-bind-key"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-A", "after-bind-key"]),
            )
            .unwrap()
            .output,
        "after-bind-key[0]* display-message global-zero\n\
             after-bind-key[named]* display-message global-named"
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-g", "pane-died[global]", "display-message global-window"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-p", "-A", "pane-died"]),
            )
            .unwrap()
            .output,
        "pane-died[global]* display-message global-window"
    );
    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-w", "pane-died[window]", "display-message window"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-p", "-A", "pane-died"]),
            )
            .unwrap()
            .output,
        "pane-died[window]* display-message window"
    );
    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-p", "pane-died[pane]", "display-message pane"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-p", "-A", "pane-died"]),
            )
            .unwrap()
            .output,
        "pane-died[pane] display-message pane"
    );
}

#[test]
fn user_hooks_share_user_option_storage_and_stay_unlisted() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "@myhook", "v"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "-v", "@myhook"]),
            )
            .unwrap()
            .output,
        "v"
    );

    let hook_command = "display-message -p AT-HOOK-RAN";
    engine
        .execute(
            &mut context,
            &command("set-hook", &["-g", "@myhook", hook_command]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "-v", "@myhook"]),
            )
            .unwrap()
            .output,
        hook_command
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("show-hooks", &["-g", "@myhook"]),)
            .unwrap()
            .output,
        engine
            .execute(&mut context, &command("show-options", &["-g", "@myhook"]),)
            .unwrap()
            .output
    );
    assert!(
        !engine
            .execute(&mut context, &command("show-hooks", &["-g"]))
            .unwrap()
            .output
            .lines()
            .any(|line| line.starts_with('@'))
    );

    let fired = engine
        .execute(&mut context, &command("set-hook", &["-R", "@myhook"]))
        .unwrap();
    assert!(matches!(
        fired.effects.as_slice(),
        [MuxEffect::RunHook {
            name,
            commands,
            context: target,
        }] if name == "@myhook"
            && target == &context
            && commands == &vec![parse_hook_commands(&engine, hook_command).unwrap()]
    ));

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "@myhook", "not-a-command"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("set-hook", &["-R", "@myhook"]),)
            .unwrap(),
        Execution::default()
    );

    engine
        .execute(
            &mut context,
            &command("set-hook", &["-g", "@myhook", hook_command]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("set-hook", &["-g", "-u", "@myhook"]))
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "-q", "-v", "@myhook"]),
            )
            .unwrap(),
        Execution::default()
    );

    engine
        .execute(
            &mut context,
            &command("set-hook", &["-g", "@myhook", hook_command]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "-u", "@myhook"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("set-hook", &["-R", "@myhook"]),)
            .unwrap(),
        Execution::default()
    );

    engine
        .execute(
            &mut context,
            &command("set-hook", &["-w", "@scoped", "display-message window"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-hook", &["-g", "@scoped", "display-message session"]),
        )
        .unwrap();
    let session_scoped = engine
        .execute(&mut context, &command("set-hook", &["-R", "@scoped"]))
        .unwrap();
    assert!(matches!(
        session_scoped.effects.as_slice(),
        [MuxEffect::RunHook { commands, .. }]
            if commands == &vec![parse_hook_commands(&engine, "display-message session").unwrap()]
    ));
    engine
        .execute(&mut context, &command("set-hook", &["-g", "-u", "@scoped"]))
        .unwrap();
    let window_scoped = engine
        .execute(&mut context, &command("set-hook", &["-R", "@scoped"]))
        .unwrap();
    assert!(matches!(
        window_scoped.effects.as_slice(),
        [MuxEffect::RunHook { commands, .. }]
            if commands == &vec![parse_hook_commands(&engine, "display-message window").unwrap()]
    ));
}

#[test]
fn hook_validation_and_bound_command_support_match_the_command_surface() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    for (args, expected, parse) in [
        (&[] as &[&str], "missing argument", true),
        (&["not-a-hook"], "invalid option: not-a-hook", false),
        (&["after-"], "ambiguous option: after-", false),
        (
            &["-B", "name:what:format"],
            "monitor hook name must start with @",
            false,
        ),
        (&["-B", "@name"], "invalid subscription: @name", false),
    ] {
        let error = engine
            .execute(&mut context, &command("set-hook", args))
            .expect_err("invalid hook");
        assert_eq!(error.tmux_message(), expected);
        assert_eq!(error.is_command_parse(), parse);
    }
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("set-hook", &["base-index[0]", "1"]),
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "not an array: base-index[0]"
    ));
    assert_eq!(
        engine
            .execute(&mut context, &command("show-hooks", &["-B"]))
            .expect("empty monitor listing")
            .output,
        String::new()
    );

    let invalid = "display-message \\";
    let expected = crate::parse_config("<set-hook>", invalid)
        .diagnostics
        .into_iter()
        .next()
        .expect("parser diagnostic")
        .message;
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("set-hook", &["after-select-window", invalid]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == expected
    ));

    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "set-hook",
                &[
                    "after-select-window",
                    "display-message -p A=#{window_name}",
                ],
            ),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "syntax error"
    ));
    for value in [
        "display-message -p 'A=#{window_name}'",
        r#"display-message -p "A=#{window_name}""#,
    ] {
        engine
            .execute(
                &mut context,
                &command("set-hook", &["after-select-window", value]),
            )
            .unwrap();
    }

    engine
        .execute(
            &mut context,
            &command(
                "bind-key",
                &[
                    "x",
                    "set-hook",
                    "after-select-window",
                    "display-message bound",
                ],
            ),
        )
        .expect("set-hook binding");
    assert_eq!(
        engine.keys.get("prefix", "x").expect("binding").commands,
        [CommandInvocation::new(
            "set-hook",
            ["after-select-window", "display-message bound"],
        )]
    );
}

#[test]
fn event_hooks_follow_declared_scope_and_isolate_windows() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first_window = context.window.expect("first window");
    let first_pane = context.pane.expect("first pane");
    engine
        .execute(&mut context, &command("new-window", &["-n", "second"]))
        .unwrap();
    let second_window = context.window.expect("second window");
    let second_pane = context.pane.expect("second pane");
    let first_window_target = first_window.to_string();
    let second_window_target = second_window.to_string();
    let first_pane_target = first_pane.to_string();

    for args in [
        &["-g", "pane-died", "display-message global-window"] as &[&str],
        &[
            "-w",
            "-t",
            first_window_target.as_str(),
            "pane-died",
            "display-message first-window",
        ],
    ] {
        engine
            .execute(&mut context, &command("set-hook", args))
            .unwrap();
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-hooks",
                    &["-w", "-t", first_window_target.as_str(), "pane-died",],
                ),
            )
            .unwrap()
            .output,
        "pane-died[0] display-message first-window"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-hooks",
                    &["-w", "-t", second_window_target.as_str(), "pane-died",],
                ),
            )
            .unwrap()
            .output,
        ""
    );

    let first_context =
        ExecutionContext::for_pane(&engine.state, first_pane).expect("first context");
    let second_context =
        ExecutionContext::for_pane(&engine.state, second_pane).expect("second context");
    assert_eq!(
        engine
            .event_hook_commands(&first_context, "pane-died")
            .expect("first window hook"),
        [parse_hook_commands(&engine, "display-message first-window").unwrap()]
    );
    assert_eq!(
        engine
            .event_hook_commands(&second_context, "pane-died")
            .expect("second window fallback"),
        [parse_hook_commands(&engine, "display-message global-window").unwrap()]
    );

    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &[
                    "-p",
                    "-t",
                    first_pane_target.as_str(),
                    "pane-died",
                    "display-message first-pane",
                ],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .event_hook_commands(&first_context, "pane-died")
            .expect("first pane hook"),
        [parse_hook_commands(&engine, "display-message first-pane").unwrap()]
    );
    engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &["-p", "-u", "-t", first_pane_target.as_str(), "pane-died"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine
            .event_hook_commands(&first_context, "pane-died")
            .expect("first window hook after pane unset"),
        [parse_hook_commands(&engine, "display-message first-window").unwrap()]
    );

    for args in [
        &["-g", "alert-bell", "display-message global-session"] as &[&str],
        &["alert-bell", "display-message local-session"],
    ] {
        engine
            .execute(&mut context, &command("set-hook", args))
            .unwrap();
    }
    assert_eq!(
        engine
            .event_hook_commands(&second_context, "alert-bell")
            .expect("local session hook"),
        [parse_hook_commands(&engine, "display-message local-session").unwrap()]
    );
    engine
        .execute(&mut context, &command("set-hook", &["-u", "alert-bell"]))
        .unwrap();
    assert_eq!(
        engine
            .event_hook_commands(&second_context, "alert-bell")
            .expect("global session fallback"),
        [parse_hook_commands(&engine, "display-message global-session").unwrap()]
    );
}

#[test]
fn set_hook_run_now_returns_effective_units_in_index_order() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let pane = context.pane.expect("pane");
    for args in [
        &["-g", "after-select-window[2]", "display-message second"] as &[&str],
        &["-g", "after-select-window[0]", "display-message first"],
        &["-g", "after-select-window[named]", "display-message named"],
    ] {
        engine
            .execute(&mut context, &command("set-hook", args))
            .unwrap();
    }
    let control_target = (ClientId(41), 0);
    context.set_control_command_target(Some(control_target));
    context.set_replay_client(Some(ClientId(42)));
    context.set_client_working_directory(Some(PathBuf::from("/caller/working-directory")));

    let execution = engine
        .execute(
            &mut context,
            &command(
                "set-hook",
                &[
                    "-R",
                    "-t",
                    &pane.to_string(),
                    "after-select-window",
                    "ignored",
                ],
            ),
        )
        .unwrap();
    let expected_commands = [
        parse_hook_commands(&engine, "display-message first").unwrap(),
        parse_hook_commands(&engine, "display-message second").unwrap(),
        parse_hook_commands(&engine, "display-message named").unwrap(),
    ];
    assert!(matches!(
        execution.effects.as_slice(),
        [MuxEffect::RunHook {
            name,
            commands,
            context: target,
        }] if name == "after-select-window"
            && target.pane == Some(pane)
            && target.control_command_target() == Some(control_target)
            && target.replay_client().is_none()
            && target.client_working_directory().is_none()
            && commands == &expected_commands
    ));
    assert_eq!(
        engine
            .execute(&mut context, &command("set-hook", &["-R", "unknown-hook"]),)
            .unwrap(),
        Execution::default()
    );
}

#[test]
fn hook_format_variables_cover_arguments_positionals_and_flags() {
    let variables = hook_format_variables(
        &CommandInvocation::new("select-window", ["-T", "-t", "work:1", "-t", "work:2"]),
        "after-select-window",
    );
    assert_eq!(variables["hook"], "after-select-window");
    assert_eq!(variables["hook_arguments"], "-T -t work:1 -t work:2");
    assert_eq!(variables["hook_flag_T"], "1");
    assert_eq!(variables["hook_flag_t"], "work:2");
    assert_eq!(variables["hook_flag_t_0"], "work:1");
    assert_eq!(variables["hook_flag_t_1"], "work:2");
    assert!(!variables.contains_key("hook_argument_0"));

    let variables = hook_format_variables(
        &CommandInvocation::new("set-option", ["-g", "@name", "two words"]),
        "after-set-option",
    );
    assert_eq!(variables["hook_arguments"], "-g @name \"two words\"");
    assert_eq!(variables["hook_argument_0"], "@name");
    assert_eq!(variables["hook_argument_1"], "two words");
    assert_eq!(variables["hook_flag_g"], "1");
}

#[test]
fn format_monitor_subscriptions_follow_monitor_parse_and_check_value() {
    assert_eq!(parse_format_monitor("nocolons"), None);
    assert_eq!(parse_format_monitor("one:colon"), None);
    for (value, scope, readback) in [
        (
            "@w:%*:#{pane_title}",
            FormatMonitorScope::AllPanes,
            "@w:%*:#{pane_title}",
        ),
        (
            "@w:%0:#{pane_title}",
            FormatMonitorScope::Pane(PaneId(0)),
            "@w:%0:#{pane_title}",
        ),
        (
            "@w:%0abc:#{pane_title}",
            FormatMonitorScope::Pane(PaneId(0)),
            "@w:%0:#{pane_title}",
        ),
        (
            "@w:@*:#{window_name}",
            FormatMonitorScope::AllWindows,
            "@w:@*:#{window_name}",
        ),
        (
            "@w:@7:#{window_name}",
            FormatMonitorScope::Window(WindowId(7)),
            "@w:@7:#{window_name}",
        ),
        (
            "@w::#{session_name}",
            FormatMonitorScope::Session,
            "@w::#{session_name}",
        ),
        (
            "@w:junk:#{session_name}",
            FormatMonitorScope::Session,
            "@w::#{session_name}",
        ),
        (
            "@w:%:#{session_name}",
            FormatMonitorScope::Session,
            "@w::#{session_name}",
        ),
        (
            "@w:@-1:#{session_name}",
            FormatMonitorScope::Session,
            "@w::#{session_name}",
        ),
    ] {
        let (name, parsed, format) =
            parse_format_monitor(value).expect("two colons parse a subscription");
        assert_eq!(name, "@w");
        assert_eq!(parsed, scope, "{value}");
        assert_eq!(format_monitor_display(&name, parsed, &format), readback);
    }

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "mon"]))
        .expect("session");
    for (subscription, body) in [
        ("@watch:@*:#{window_name}", "set -g @fired one"),
        ("@sess::#{session_name}", "set -g @fired two"),
        ("@panes:%*:#{pane_title}", "set -g @fired three"),
    ] {
        engine
            .execute(
                &mut context,
                &command("set-hook", &["-B", subscription, body]),
            )
            .expect("arm subscription");
    }
    assert_eq!(
        engine
            .execute(&mut context, &command("show-hooks", &["-B"]))
            .expect("listing")
            .output,
        "@panes:%*:#{pane_title}\n@sess::#{session_name}\n@watch:@*:#{window_name}"
    );

    let monitors = engine.format_monitors();
    assert_eq!(monitors.len(), 3);
    let watch = monitors
        .iter()
        .find(|monitor| monitor.name == "@watch")
        .expect("window subscription");
    assert_eq!(watch.scope, FormatMonitorScope::AllWindows);
    assert_eq!(watch.format, "#{window_name}");
    assert_eq!(
        engine.format_monitor_hook_body(watch.id).as_deref(),
        Some("set -g @fired one")
    );

    let window = engine.state.windows.keys().copied().next().expect("window");
    let session = engine.state.windows[&window].session;
    let target = FormatMonitorTarget {
        session: Some(session),
        window: Some(window),
        pane: None,
    };
    assert_eq!(
        engine.record_format_monitor_sample(watch.id, target, "first"),
        None
    );
    assert_eq!(
        engine.record_format_monitor_sample(watch.id, target, "first"),
        None
    );
    assert_eq!(
        engine.record_format_monitor_sample(watch.id, target, "renamed"),
        Some("first".to_owned())
    );
    engine.sweep_format_monitor_targets(watch.id, &BTreeSet::new());
    assert_eq!(
        engine.record_format_monitor_sample(watch.id, target, "renamed"),
        None
    );

    engine
        .execute(&mut context, &command("set-hook", &["-u", "-B", "@watch"]))
        .expect("remove subscription");
    assert_eq!(
        engine
            .execute(&mut context, &command("show-hooks", &["-B", "@watch"]))
            .expect("removed listing")
            .output,
        String::new()
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("set-hook", &["-Bu", "@watch"]))
            .expect_err("-B swallows the u")
            .tmux_message(),
        "invalid subscription: u"
    );
}

#[test]
fn format_monitor_hook_variables_carry_notify_monitor_cb_names() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "mon"]))
        .expect("session");
    let window = engine.state.windows.keys().copied().next().expect("window");
    let session = engine.state.windows[&window].session;
    let pane = engine.state.windows[&window].active_pane;

    let variables = engine.format_monitor_hook_variables(
        "@watch",
        "renamed",
        "first",
        FormatMonitorTarget {
            session: Some(session),
            window: Some(window),
            pane: None,
        },
    );
    assert_eq!(variables["hook"], "@watch");
    assert_eq!(variables["hook_value"], "renamed");
    assert_eq!(variables["hook_last"], "first");
    assert_eq!(variables["hook_window"], window.to_string());
    assert_eq!(
        variables["hook_window_name"],
        engine.state.windows[&window].name
    );
    assert_eq!(variables["hook_window_index"], "0");
    assert_eq!(variables["hook_session"], session.to_string());
    assert_eq!(variables["hook_session_name"], "mon");
    assert!(!variables.contains_key("hook_pane"));

    let variables = engine.format_monitor_hook_variables(
        "@sess",
        "mon2",
        "mon",
        FormatMonitorTarget {
            session: Some(session),
            window: None,
            pane: None,
        },
    );
    assert!(!variables.contains_key("hook_window"));
    assert!(!variables.contains_key("hook_window_name"));
    assert!(!variables.contains_key("hook_window_index"));
    assert!(!variables.contains_key("hook_pane"));

    let variables = engine.format_monitor_hook_variables(
        "@panes",
        "after",
        "before",
        FormatMonitorTarget {
            session: Some(session),
            window: Some(window),
            pane: Some(pane),
        },
    );
    assert_eq!(variables["hook_pane"], pane.to_string());
    assert_eq!(variables.len(), 9);
}

#[test]
fn show_hooks_counts_each_entrys_fires_like_3_8() {
    let mut engine = MuxEngine::default();
    let (session, window, pane) = engine.state.create_session("work").unwrap();
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let fired = "#{option_name}:#{hook_fire_count}:#{?hook_fire_time,#{hook_fire_time},unset}";
    let mut run = |engine: &mut MuxEngine, args: &[&str]| {
        engine
            .execute(&mut context, &command(args[0], &args[1..]))
            .unwrap()
            .output
    };
    run(
        &mut engine,
        &["set-hook", "-g", "after-new-window", "set -g @a 1"],
    );
    run(
        &mut engine,
        &["set-hook", "-g", "window-renamed", "set -g @b 1"],
    );
    assert_eq!(
        run(
            &mut engine,
            &["show-hooks", "-g", "-F", fired, "after-new-window"]
        ),
        "after-new-window:0:unset"
    );
    engine.note_hook_fired(Some(session), "after-new-window", 1_700_000_000);
    engine.note_hook_fired(Some(session), "after-new-window", 1_700_000_005);
    assert_eq!(
        run(
            &mut engine,
            &["show-hooks", "-g", "-F", fired, "after-new-window"]
        ),
        "after-new-window:2:1700000005"
    );
    run(
        &mut engine,
        &["set-hook", "after-new-window", "set -g @c 1"],
    );
    engine.note_hook_fired(Some(session), "after-new-window", 1_700_000_009);
    assert_eq!(
        run(
            &mut engine,
            &["show-hooks", "-F", fired, "after-new-window"]
        ),
        "after-new-window:1:1700000009"
    );
    assert_eq!(
        run(
            &mut engine,
            &["show-hooks", "-g", "-F", fired, "after-new-window"]
        ),
        "after-new-window:2:1700000005"
    );
    let hook_context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    engine.note_event_hook_fired(&hook_context, "window-renamed", 1_700_000_010);
    assert_eq!(
        run(
            &mut engine,
            &["show-hooks", "-gw", "-F", fired, "window-renamed"]
        ),
        "window-renamed:1:1700000010"
    );
    assert_eq!(
        run(&mut engine, &["show-options", "-g", "-F", fired, "status"]),
        "status::unset"
    );
    engine.note_hook_fired(Some(session), "after-kill-pane", 1_700_000_011);
    assert_eq!(
        run(
            &mut engine,
            &["show-hooks", "-g", "-F", fired, "after-kill-pane"]
        ),
        "after-kill-pane:1:1700000011"
    );
    run(&mut engine, &["set-hook", "-u", "after-new-window"]);
    run(
        &mut engine,
        &["set-hook", "after-new-window", "set -g @e 1"],
    );
    assert_eq!(
        run(
            &mut engine,
            &["show-hooks", "-F", fired, "after-new-window"]
        ),
        "after-new-window:0:unset"
    );
    run(&mut engine, &["set-hook", "-g", "@h", "set -g @u 1"]);
    engine.note_event_hook_fired(&hook_context, "@h", 1_700_000_012);
    assert_eq!(
        run(&mut engine, &["show-hooks", "-g", "-F", fired, "@h"]),
        "@h:1:1700000012"
    );
    assert_eq!(
        run(&mut engine, &["show-options", "-g", "-F", fired, "@h"]),
        "@h::unset"
    );

    run(
        &mut engine,
        &["set-hook", "-B", "@watch::#{window_name}", "set -g @d 1"],
    );
    let monitor = engine.format_monitors()[0].id;
    let target = FormatMonitorTarget {
        session: Some(session),
        window: None,
        pane: None,
    };
    engine.set_format_now(1_700_000_020);
    assert_eq!(
        engine.record_format_monitor_sample(monitor, target, "a"),
        None
    );
    assert_eq!(
        run(&mut engine, &["show-hooks", "-B", "-F", fired, "@watch"]),
        "@watch:0:unset"
    );
    assert_eq!(
        engine.record_format_monitor_sample(monitor, target, "b"),
        Some("a".to_owned())
    );
    assert_eq!(
        run(&mut engine, &["show-hooks", "-B", "-F", fired, "@watch"]),
        "@watch:1:1700000020"
    );
    assert_eq!(
        run(&mut engine, &["show-hooks", "-F", fired, "@watch"]),
        "@watch:1:1700000020"
    );
    assert_eq!(
        run(&mut engine, &["show-hooks", "-g", "-F", fired, "status"]),
        "status:0:unset"
    );
    run(&mut engine, &["set-hook", "-gu", "after-new-window"]);
    assert_eq!(
        run(
            &mut engine,
            &["show-hooks", "-g", "-F", fired, "after-new-window"]
        ),
        "after-new-window:0:unset"
    );
    run(&mut engine, &["set-option", "-g", "@broken", "{"]);
    run(&mut engine, &["set-hook", "-R", "@broken"]);
    assert_eq!(
        run(
            &mut engine,
            &[
                "show-hooks",
                "-g",
                "-F",
                "#{option_name}:#{hook_fire_count}",
                "@broken"
            ]
        ),
        "@broken:1"
    );
}

#[test]
fn display_popup_catalogue_and_dead_hook_match_the_pin() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let listing = engine
        .execute(&mut context, &command("list-commands", &["display-popup"]))
        .unwrap()
        .output;
    assert_eq!(
        listing,
        "display-popup (popup) [-BCEkN] [-b border-lines] [-c target-client] [-d start-directory] [-e environment] [-h height] [-s style] [-S border-style] [-t target-pane] [-T title] [-w width] [-x position] [-y position] [shell-command [argument ...]]"
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "set-hook",
                &["after-display-popup", "display-message nope"]
            )
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "invalid option: after-display-popup"
    ));
    assert_eq!(
        engine
            .execute(&mut context, &command("list-commands", &["display-menu"]))
            .unwrap()
            .output,
        "display-menu (menu) [-MO] [-b border-lines] [-c target-client] [-C starting-choice] [-H selected-style] [-s style] [-S border-style] [-t target-pane] [-T title] [-x position] [-y position] name [key] [command] ..."
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("set-hook", &["after-display-menu", "display-message nope"])
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "invalid option: after-display-menu"
    ));
    assert_eq!(
        engine
            .execute(&mut context, &command("list-commands", &["confirm-before"]))
            .unwrap()
            .output,
        "confirm-before (confirm) [-by] [-c confirm-key] [-p prompt] [-t target-client] command"
    );
    for (name, expected) in [
        ("lock-server", "lock-server (lock) "),
        ("lock-session", "lock-session (locks) [-t target-session]"),
        ("lock-client", "lock-client (lockc) [-t target-client]"),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("list-commands", &[name]))
                .unwrap()
                .output,
            expected
        );
    }
}
