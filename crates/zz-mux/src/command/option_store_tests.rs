use super::test_support::command;
use super::*;

#[test]
fn indexed_option_spellings_route_like_the_pin() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "status-format", "one,two three"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "status-format"]),
            )
            .unwrap()
            .output,
        "status-format[0] one\nstatus-format[1] two\nstatus-format[2] three"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-ga", "status-format[1]", " tail"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "status-format[1]"]),
            )
            .unwrap()
            .output,
        "status-format[1] \"two tail\""
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-gu", "status-format[0]"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-ga", "status-format", "reused"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gv", "status-format[0]"]),
            )
            .unwrap()
            .output,
        "reused"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "terminal-features", "one,two"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "terminal-features"]),
            )
            .unwrap()
            .output,
        "one\ntwo"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-gw", "pane-colors", "red,blue"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gw", "pane-colours"]),
            )
            .unwrap()
            .output,
        "pane-colours[0] red\npane-colours[1] blue"
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("set-option", &["-gw", "pane-colours[0]", "value"]),
        ),
        Err(ServerError::InvalidCommand(message)) if message == "bad colour: value"
    ));

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "@plain", "solo"]),
        )
        .unwrap();
    for index in ["0", "7", "key"] {
        let spelling = format!("@plain[{index}]");
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", &["-gv", &spelling]),)
                .unwrap()
                .output,
            "solo"
        );
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "@plain[000]"]),
            )
            .unwrap()
            .output,
        "@plain[0] solo"
    );

    for args in [
        &["-g", "@arr[0]", "first"] as &[&str],
        &["-gq", "@arr[0]", "first"],
        &["-g", "base-index[0]", "1"],
        &["-gq", "base-index[0]", "1"],
        &["-g", "escape-time[0]", "1"],
    ] {
        let spelling = args[1];
        assert!(matches!(
            engine.execute(&mut context, &command("set-option", args)),
            Err(ServerError::InvalidCommand(message))
                if message == format!("not an array: {spelling}")
        ));
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "base-index[000]"]),
            )
            .unwrap()
            .output,
        "base-index[0] 0"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "escape-time[0]"]),
            )
            .unwrap()
            .output,
        "escape-time[0] 10"
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command("show-options", &["-g", "status-format[]"]),
        ),
        Err(ServerError::InvalidCommand(message))
            if message == "invalid option: status-format[]"
    ));
}

#[test]
fn lane2_array_storage_and_hook_option_routing_match_the_pin_shapes() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "lane2"]))
        .unwrap();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-s", "command-alias"]),
            )
            .unwrap()
            .output,
        concat!(
            "command-alias[0] split-pane=split-window\n",
            "command-alias[1] splitp=split-window\n",
            "command-alias[2] \"server-info=show-messages -JT\"\n",
            "command-alias[3] \"info=show-messages -JT\"\n",
            "command-alias[4] \"choose-window=choose-tree -w\"\n",
            "command-alias[5] \"choose-session=choose-tree -s\"",
        )
    );
    for (name, value) in [
        ("command-alias", "alias=display-message"),
        ("codepoint-widths", "U+41=1"),
        ("terminal-overrides", "term:RGB"),
        ("terminal-features", "term:title"),
        ("user-keys", "abc"),
    ] {
        let indexed = format!("{name}[7]");
        engine
            .execute(
                &mut context,
                &command("set-option", &["-s", &indexed, value]),
            )
            .unwrap();
        assert_eq!(
            engine
                .execute(&mut context, &command("show-options", &["-sv", &indexed]),)
                .unwrap()
                .output,
            value,
            "{name}"
        );
    }
    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "update-environment", "FOO BAR"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-g", "update-environment"]),
            )
            .unwrap()
            .output,
        "update-environment[0] FOO\nupdate-environment[1] BAR"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-s", "codepoint-widths", ""]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-s", "codepoint-widths"]),
            )
            .unwrap()
            .output,
        "codepoint-widths"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-sv", "codepoint-widths"]),
            )
            .unwrap()
            .output,
        ""
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "status-format", "global"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(&mut context, &command("show-options", &["status-format"]),)
            .unwrap()
            .output,
        ""
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-A", "status-format"]),
            )
            .unwrap()
            .output,
        "status-format[0]* global"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["status-format[3]", "local"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-A", "status-format"]),
            )
            .unwrap()
            .output,
        "status-format[3] local"
    );
    engine
        .execute(
            &mut context,
            &command("set-option", &["-u", "status-format"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-A", "status-format"]),
            )
            .unwrap()
            .output,
        "status-format[0]* global"
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-p", "pane-colours[0]", "red"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set-option", &["-wU", "pane-colours"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-pA", "pane-colours"]),
            )
            .unwrap()
            .output,
        "pane-colours"
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "alert-bell", "display-message"]),
        )
        .unwrap();
    let through_hooks = engine
        .execute(&mut context, &command("show-hooks", &["-g", "alert-bell"]))
        .unwrap()
        .output;
    let through_options = engine
        .execute(
            &mut context,
            &command("show-options", &["-g", "alert-bell"]),
        )
        .unwrap()
        .output;
    assert_eq!(through_options, through_hooks);
    assert_eq!(through_options, "alert-bell[0] display-message");
    engine
        .execute(
            &mut context,
            &command("set-option", &["-ga", "alert-bell", "kill-pane"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("show-options", &["-gv", "alert-bell"]),
            )
            .unwrap()
            .output,
        "display-message\nkill-pane"
    );

    let server_listing = engine
        .execute(&mut context, &command("show-options", &["-s"]))
        .unwrap()
        .output;
    assert!(
        server_listing.find("command-alias[0]").unwrap()
            < server_listing.find("codepoint-widths").unwrap()
    );
    assert!(
        server_listing.find("terminal-overrides[0]").unwrap()
            < server_listing.find("terminal-features[0]").unwrap()
    );
    let window_listing = engine
        .execute(&mut context, &command("show-options", &["-gw"]))
        .unwrap()
        .output;
    let scrollbars = window_listing.find("pane-scrollbars off").unwrap();
    let timeout = window_listing.find("pane-scrollbars-timeout 500").unwrap();
    let style = window_listing.find("pane-scrollbars-style ").unwrap();
    let position = window_listing
        .find("pane-scrollbars-position right")
        .unwrap();
    assert!(scrollbars < timeout && timeout < style && style < position);
    assert!(!server_listing.contains("alert-bell"));
    assert!(!window_listing.contains("alert-bell"));
}

#[test]
fn index_options_validate_pin_bounds_scopes_and_unset_values() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let session = context.session.expect("session");
    let window = context.window.expect("window");
    let pane = context.pane.expect("pane");

    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-g", "base-index", &MAX_BASE_INDEX.to_string()],
            ),
        )
        .expect("maximum base index");
    assert_eq!(engine.base_index_for_session(session), MAX_BASE_INDEX);
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-g", "pane-base-index", &MAX_PANE_BASE_INDEX.to_string()],
            ),
        )
        .expect("maximum pane base index");
    assert_eq!(engine.pane_index(window, pane), Some(MAX_PANE_BASE_INDEX));

    for invalid in [
        command("set-option", &["-g", "base-index"]),
        command("set-option", &["-g", "base-index", "-1"]),
        command("set-option", &["-g", "base-index", "nope"]),
        command(
            "set-option",
            &["-g", "base-index", &(MAX_BASE_INDEX + 1).to_string()],
        ),
        command("set-window-option", &["-g", "pane-base-index"]),
        command("set-window-option", &["-g", "pane-base-index", "-1"]),
        command("set-window-option", &["-g", "pane-base-index", "nope"]),
        command(
            "set-window-option",
            &[
                "-g",
                "pane-base-index",
                &(MAX_PANE_BASE_INDEX + 1).to_string(),
            ],
        ),
        command("set-option", &["-g", "renumber-windows", "maybe"]),
    ] {
        assert!(
            engine.execute(&mut context, &invalid).is_err(),
            "{invalid:?}"
        );
    }
    for (invalid, expected) in [
        (
            command("set-option", &["-g", "base-index", "nope"]),
            "value is invalid: nope",
        ),
        (
            command("set-option", &["-g", "base-index", "-1"]),
            "value is too small: -1",
        ),
        (
            command(
                "set-window-option",
                &[
                    "-g",
                    "pane-base-index",
                    &(MAX_PANE_BASE_INDEX + 1).to_string(),
                ],
            ),
            "value is too large: 65536",
        ),
    ] {
        assert!(matches!(
            engine.execute(&mut context, &invalid),
            Err(ServerError::InvalidCommand(message)) if message == expected
        ));
    }

    engine
        .execute(&mut context, &command("set-option", &["-gu", "base-index"]))
        .expect("restore base default");
    engine
        .execute(
            &mut context,
            &command("set-window-option", &["-gu", "pane-base-index"]),
        )
        .expect("restore pane base default");
    assert_eq!(engine.base_index_for_session(session), DEFAULT_BASE_INDEX);
    assert_eq!(
        engine.pane_index(window, pane),
        Some(DEFAULT_PANE_BASE_INDEX)
    );

    engine
        .execute(
            &mut context,
            &command("set-option", &["-g", "renumber-windows"]),
        )
        .expect("toggle global renumbering on");
    assert!(engine.renumber_windows_for_session(session));
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-t", &session.to_string(), "renumber-windows", "off"],
            ),
        )
        .expect("session renumber override");
    assert!(!engine.renumber_windows_for_session(session));
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-u", "-t", &session.to_string(), "renumber-windows"],
            ),
        )
        .expect("restore renumber inheritance");
    assert!(engine.renumber_windows_for_session(session));
    engine
        .execute(
            &mut context,
            &command("set-option", &["-gu", "renumber-windows"]),
        )
        .expect("restore renumber default");
    assert!(!engine.renumber_windows_for_session(session));
}

#[test]
fn index_options_accept_tmux_operation_flag_combinations() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    let window = context.window.unwrap();
    let pane = context.pane.unwrap();

    engine
        .execute(&mut context, &command("set", &["-a", "base-index", "2"]))
        .unwrap();
    assert_eq!(engine.base_index_for_session(session), 2);
    engine
        .execute(
            &mut context,
            &command("set", &["-u", "base-index", "not-a-number"]),
        )
        .unwrap();
    assert_eq!(engine.base_index_for_session(session), DEFAULT_BASE_INDEX);
    engine
        .execute(
            &mut context,
            &command("set", &["-ou", "base-index", "not-a-number"]),
        )
        .unwrap();
    engine
        .execute(&mut context, &command("set", &["-o", "base-index", "3"]))
        .unwrap();
    assert_eq!(engine.base_index_for_session(session), 3);
    assert!(
        engine
            .execute(&mut context, &command("set", &["-o", "base-index", "4"]))
            .is_err()
    );
    engine
        .execute(&mut context, &command("set", &["-oq", "base-index", "4"]))
        .unwrap();
    assert_eq!(engine.base_index_for_session(session), 3);
    engine
        .execute(
            &mut context,
            &command("set", &["-U", "base-index", "not-a-number"]),
        )
        .unwrap();
    assert_eq!(engine.base_index_for_session(session), DEFAULT_BASE_INDEX);

    engine
        .execute(
            &mut context,
            &command("set", &["-a", "pane-base-index", "2"]),
        )
        .unwrap();
    assert_eq!(engine.pane_index(window, pane), Some(2));
    engine
        .execute(
            &mut context,
            &command("set", &["-ou", "pane-base-index", "not-a-number"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set", &["-o", "pane-base-index", "3"]),
        )
        .unwrap();
    assert_eq!(engine.pane_index(window, pane), Some(3));
    engine
        .execute(
            &mut context,
            &command("set", &["-U", "pane-base-index", "not-a-number"]),
        )
        .unwrap();
    assert_eq!(
        engine.pane_index(window, pane),
        Some(DEFAULT_PANE_BASE_INDEX)
    );

    engine
        .execute(
            &mut context,
            &command("set", &["-a", "renumber-windows", "on"]),
        )
        .unwrap();
    assert!(engine.renumber_windows_for_session(session));
    engine
        .execute(
            &mut context,
            &command("set", &["-u", "renumber-windows", "true"]),
        )
        .unwrap();
    assert!(!engine.renumber_windows_for_session(session));
    engine
        .execute(
            &mut context,
            &command("set", &["-o", "renumber-windows", "on"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command("set", &["-ou", "renumber-windows", "false"]),
        )
        .unwrap();
    assert!(!engine.renumber_windows_for_session(session));
}
