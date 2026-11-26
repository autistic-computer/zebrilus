use super::*;

#[test]
fn pane_cursor_options_clone_only_for_concrete_overrides() {
    let base = Arc::new(TerminalAppearance::default());
    assert!(Arc::ptr_eq(
        &pane_terminal_appearance(&base, "default", "", (None, None), &[]),
        &base
    ));

    for (cursor_style, expected_style, expected_blink) in [
        ("blinking-block", CursorStyle::Block, CursorBlinkPolicy::On),
        ("block", CursorStyle::Block, CursorBlinkPolicy::Off),
        (
            "blinking-underline",
            CursorStyle::Underline,
            CursorBlinkPolicy::On,
        ),
        ("underline", CursorStyle::Underline, CursorBlinkPolicy::Off),
        ("blinking-bar", CursorStyle::Bar, CursorBlinkPolicy::On),
        ("bar", CursorStyle::Bar, CursorBlinkPolicy::Off),
    ] {
        let appearance = pane_terminal_appearance(&base, cursor_style, "", (None, None), &[]);
        assert_eq!(appearance.cursor_style, expected_style);
        assert_eq!(appearance.cursor_blink_policy, expected_blink);
    }

    let indexed = pane_terminal_appearance(&base, "default", "colour42", (None, None), &[]);
    assert_eq!(indexed.cursor_color, base.palette.as_array()[42]);
    let x11 = pane_terminal_appearance(&base, "default", "sky blue", (None, None), &[]);
    assert_eq!(x11.cursor_color, Color::rgb(135, 206, 235));

    let tinted = pane_terminal_appearance(
        &base,
        "default",
        "",
        (
            Some(TmuxColour::Rgb(0x0000_ff00)),
            Some(TmuxColour::Indexed(17)),
        ),
        &[],
    );
    assert_eq!(tinted.foreground, Color::rgb(0, 255, 0));
    assert_eq!(tinted.background, base.palette.as_array()[17]);
    assert_eq!(tinted.cursor_style, base.cursor_style);
    let defaulted = pane_terminal_appearance(
        &base,
        "default",
        "",
        (Some(TmuxColour::Default), Some(TmuxColour::Terminal)),
        &[],
    );
    assert!(Arc::ptr_eq(&defaulted, &base));

    let palette = pane_terminal_appearance(
        &base,
        "default",
        "",
        (None, None),
        &[
            (1, "#123456".to_owned()),
            (200, "colour17".to_owned()),
            (2, "default".to_owned()),
        ],
    );
    assert_eq!(palette.palette.as_array()[1], Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(
        palette.palette.as_array()[2],
        base.palette.as_array()[2],
        "pane-colours[N] default keeps the theme slot; the pin resolves that cell to the terminal default"
    );
    assert_eq!(palette.palette.as_array()[200], base.palette.as_array()[17]);
    assert_eq!(palette.palette.as_array()[2], base.palette.as_array()[2]);
    assert_eq!(palette.palette.as_array()[0], base.palette.as_array()[0]);
    assert_eq!(palette.foreground, base.foreground);
    let untouched = pane_terminal_appearance(
        &base,
        "default",
        "",
        (None, None),
        &[(3, "notacolour".to_owned())],
    );
    assert!(Arc::ptr_eq(&untouched, &base));
}

#[test]
fn window_styles_tint_the_pane_appearance_through_worker_options() {
    let shared = Shared::new(1);
    let mut context = ExecutionContext::default();
    let mut inner = shared.inner.lock();
    inner
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "tint", "-n", "main"]),
        )
        .expect("create session");
    let window = context.window.expect("window id");
    inner
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("split-window", ["-d"]),
        )
        .expect("split window");
    let order = inner.engine.state.windows[&window].pane_order().to_vec();
    let (active, inactive) = (order[0], order[1]);

    let untouched = terminal_worker_options(
        &inner.engine,
        &inner.config.appearance,
        &inner.config.config_files,
        active,
    )
    .expect("worker options");
    assert!(Arc::ptr_eq(&untouched.appearance, &inner.config.appearance));

    inner
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "window-style", "bg=#101010"]),
        )
        .expect("set window-style");
    inner
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-g", "window-active-style", "bg=#202020,fg=#f0f0f0"],
            ),
        )
        .expect("set window-active-style");

    let active_options = terminal_worker_options(
        &inner.engine,
        &inner.config.appearance,
        &inner.config.config_files,
        active,
    )
    .expect("worker options");
    assert_eq!(
        active_options.appearance.background,
        Color::rgb(0x20, 0x20, 0x20)
    );
    assert_eq!(
        active_options.appearance.foreground,
        Color::rgb(0xf0, 0xf0, 0xf0)
    );
    let inactive_options = terminal_worker_options(
        &inner.engine,
        &inner.config.appearance,
        &inner.config.config_files,
        inactive,
    )
    .expect("worker options");
    assert_eq!(
        inactive_options.appearance.background,
        Color::rgb(0x10, 0x10, 0x10)
    );
    assert_eq!(
        inactive_options.appearance.foreground,
        inner.config.appearance.foreground
    );

    inner
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-g", "window-style", "bg=#{?pane_active,#303030,#404040}"],
            ),
        )
        .expect("set conditional window-style");
    inner
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-option", ["-gu", "window-active-style"]),
        )
        .expect("unset window-active-style");
    let conditional_active = terminal_worker_options(
        &inner.engine,
        &inner.config.appearance,
        &inner.config.config_files,
        active,
    )
    .expect("worker options");
    assert_eq!(
        conditional_active.appearance.background,
        Color::rgb(0x30, 0x30, 0x30)
    );
    let conditional_inactive = terminal_worker_options(
        &inner.engine,
        &inner.config.appearance,
        &inner.config.config_files,
        inactive,
    )
    .expect("worker options");
    assert_eq!(
        conditional_inactive.appearance.background,
        Color::rgb(0x40, 0x40, 0x40)
    );
}

#[test]
fn renderer_style_conditionals_see_loaded_config_files() {
    let shared = Shared::new(1);
    let mut context = ExecutionContext::default();
    let mut inner = shared.inner.lock();
    inner.config.config_files = "/tmp/loaded.conf".to_owned();
    inner
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "style-config"]),
        )
        .expect("create session");
    let pane = context.pane.expect("pane id");
    inner
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-g", "window-style", "bg=#{?config_files,#123456,#654321}"],
            ),
        )
        .expect("set conditional window style");
    inner
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                ["-g", "mode-style", "bg=#{?config_files,#112233,#332211}"],
            ),
        )
        .expect("set conditional mode style");

    let terminal = terminal_worker_options(
        &inner.engine,
        &inner.config.appearance,
        &inner.config.config_files,
        pane,
    )
    .expect("worker options");
    assert_eq!(terminal.appearance.background, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(
        published_appearance(&inner).selection_background,
        AppearanceColor::rgba(0x11, 0x22, 0x33, 255)
    );
}

#[test]
fn copy_mode_styles_patch_the_published_appearance_only_when_set() {
    let shared = Shared::new(1);
    let mut context = ExecutionContext::default();
    let mut inner = shared.inner.lock();
    let base = published_appearance(&inner);
    assert!(Arc::ptr_eq(&base, &inner.config.appearance));

    inner
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                ["-g", "copy-mode-match-style", "bg=#ff0000"],
            ),
        )
        .expect("set match style");
    inner
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                ["-g", "copy-mode-current-match-style", "fg=#00ff00"],
            ),
        )
        .expect("set current match style");
    inner
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                ["-g", "mode-style", "bg=#0000ff,fg=#111111"],
            ),
        )
        .expect("set mode style");

    let patched = published_appearance(&inner);
    assert_eq!(
        patched.search_match_color,
        AppearanceColor::rgba(255, 0, 0, 255)
    );
    assert_eq!(
        patched.search_current_color,
        AppearanceColor::rgba(0, 255, 0, 255)
    );
    assert_eq!(
        patched.selection_background,
        AppearanceColor::rgba(0, 0, 255, 255)
    );
    assert_eq!(patched.selection_foreground, Color::rgb(0x11, 0x11, 0x11));

    inner
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                ["-g", "copy-mode-mark-style", "bg=#123456"],
            ),
        )
        .expect("set mark style");
    let with_mark = published_appearance(&inner);
    assert_eq!(with_mark.search_match_color, patched.search_match_color);
    assert_eq!(with_mark.selection_background, patched.selection_background);

    for name in [
        "copy-mode-match-style",
        "copy-mode-current-match-style",
        "mode-style",
        "copy-mode-mark-style",
    ] {
        inner
            .engine
            .execute(
                &mut context,
                &CommandInvocation::new("set-window-option", ["-gu", name]),
            )
            .expect("unset style");
    }
    assert!(Arc::ptr_eq(
        &published_appearance(&inner),
        &inner.config.appearance
    ));
}

#[test]
fn pane_border_style_colours_stamp_only_when_explicitly_set() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "borders", "-n", "main"]),
        )
        .expect("create session");
    let window = context.window.expect("window id");
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("split-window", ["-d"]),
        )
        .expect("split window");
    let order = engine.state.windows[&window].pane_order().to_vec();
    let (active, inactive) = (order[0], order[1]);
    let facts = FormatHookFacts::default();

    let mut snapshot = engine.state.snapshot();
    stamp_pane_border_colours(
        &engine,
        "",
        &facts,
        &engine.format_context_snapshot(FormatClient::NoClient),
        &mut snapshot,
    );
    for session in &snapshot.sessions {
        for window in &session.windows {
            for pane in window.panes.values() {
                assert_eq!(pane.border_colour, None);
                assert_eq!(pane.active_border_colour, None);
            }
        }
    }

    engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "pane-border-style", "fg=colour100"]),
        )
        .expect("set border style");
    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-p",
                    "-t",
                    &inactive.to_string(),
                    "pane-active-border-style",
                    "fg=#{?pane_active,#ff0000,#00ff00}",
                ],
            ),
        )
        .expect("set active border style");

    let mut snapshot = engine.state.snapshot();
    stamp_pane_border_colours(
        &engine,
        "",
        &facts,
        &engine.format_context_snapshot(FormatClient::NoClient),
        &mut snapshot,
    );
    let panes = &snapshot.sessions[0].windows[0].panes;
    assert_eq!(panes[&active].border_colour, Some(TmuxColour::Indexed(100)));
    assert_eq!(panes[&active].active_border_colour, None);
    assert_eq!(
        panes[&inactive].border_colour,
        Some(TmuxColour::Indexed(100))
    );
    assert_eq!(
        panes[&inactive].active_border_colour,
        Some(TmuxColour::Rgb(0x0000_ff00))
    );
}

#[test]
fn a_pane_border_style_reaches_a_client_as_its_foreground_alone() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "residue", "-n", "main"]),
        )
        .expect("create session");
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("split-window", ["-d"]),
        )
        .expect("split window");
    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-g", "pane-border-style", "bold,fg=colour190,bg=colour17"],
            ),
        )
        .expect("set border style");

    let facts = FormatHookFacts::default();
    let mut snapshot = engine.state.snapshot();
    stamp_pane_border_colours(
        &engine,
        "",
        &facts,
        &engine.format_context_snapshot(FormatClient::NoClient),
        &mut snapshot,
    );
    for pane in snapshot.sessions[0].windows[0].panes.values() {
        assert_eq!(pane.border_colour, Some(TmuxColour::Indexed(190)));
    }
    assert_eq!(
        zz_protocol::parse_style("bold,fg=colour190,bg=colour17")
            .expect("the style parses")
            .bg,
        Some(TmuxColour::Indexed(17)),
        "the background the pin draws is parsed and then dropped"
    );
}

#[test]
fn display_panes_labels_expand_the_format_per_pane() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "labels", "-n", "main"]),
        )
        .expect("create session");
    let window = context.window.expect("window id");
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("split-window", ["-d"]),
        )
        .expect("split window");
    let session = engine.state.windows[&window].session;
    let facts = FormatHookFacts::default();

    let (_, _, state) = build_display_panes_state(
        &engine,
        "",
        &facts,
        engine.state.windows[&window].active_pane,
        1_000,
        session,
    )
    .expect("display panes state");
    assert_eq!(state.indicators.len(), 2);
    for indicator in &state.indicators {
        let expected = {
            let context = engine.format_status_context(
                Some(engine.state.windows[&window].session),
                Some(window),
                Some(indicator.pane),
            );
            format!(
                "#[align=right]{}x{}",
                context.pane_width.expect("pane width"),
                context.pane_height.expect("pane height")
            )
        };
        assert_eq!(indicator.label, expected);
    }

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-option",
                [
                    "-g",
                    "display-panes-format",
                    "pane #{pane_index} #{session_active}",
                ],
            ),
        )
        .expect("set format");
    let (_, _, state) = build_display_panes_state(
        &engine,
        "",
        &facts,
        engine.state.windows[&window].active_pane,
        1_000,
        session,
    )
    .expect("display panes state");
    assert_eq!(state.indicators[0].label, "pane 0 1");
    assert_eq!(state.indicators[1].label, "pane 1 1");

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-option",
                ["-g", "display-panes-format", &"x".repeat(2 * 1024)],
            ),
        )
        .expect("set oversized format");
    let (_, _, state) = build_display_panes_state(
        &engine,
        "",
        &facts,
        engine.state.windows[&window].active_pane,
        1_000,
        session,
    )
    .expect("display panes state");
    assert_eq!(
        state.indicators[0].label.len(),
        zz_protocol::MAX_PANE_INDICATOR_LABEL_BYTES
    );
}

#[test]
fn custom_appearance_reaches_hello_and_both_terminal_actor_kinds() {
    let appearance = TerminalAppearance {
        font_families: vec!["Review Fixture Mono".to_owned()],
        font_size_points: 17.0,
        foreground: zz_terminal::Color::rgb(0x12, 0x34, 0x56),
        background: zz_terminal::Color::rgb(0x65, 0x43, 0x21),
        ..TerminalAppearance::default()
    };
    let shared = Arc::new(Shared::with_appearance(41, Arc::new(appearance.clone())));
    let mailbox = OutboundMailbox::new();
    let (client, hello) = shared.register_subscribed(ClientKind::Interactive, None, None, mailbox);
    assert_eq!(hello.appearance, appearance);

    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "appearance"]),
        )
        .expect("create terminal pane");
    let session = context.session.expect("session id");
    let pane = context.pane.expect("terminal pane");
    shared.attach(client, session).expect("attach client");

    let live = Arc::clone(&shared.inner.lock().terminals[&pane]);
    assert_eq!(live.latest_viewport().background, appearance.background);

    shared
        .open_command_output(client, Some(pane), "fixture".to_owned(), "hello")
        .expect("open command-output actor");
    let output = Arc::clone(
        &shared.inner.lock().clients[&client]
            .command_output
            .as_ref()
            .unwrap()
            .terminal,
    );
    assert_eq!(output.latest_viewport().background, appearance.background);
}

#[test]
fn reload_updates_live_actor_kinds_and_broadcasts_the_appearance() {
    let initial = TerminalAppearance {
        background: zz_terminal::Color::rgb(0x12, 0x34, 0x56),
        ..TerminalAppearance::default()
    };
    let shared = Arc::new(Shared::with_appearance(42, Arc::new(initial.clone())));
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        Some(TerminalColorScheme::Light),
        Arc::clone(&mailbox),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "reload"]),
        )
        .expect("create terminal pane");
    let session = context.session.expect("session");
    let pane = context.pane.expect("pane");
    shared.attach(client, session).expect("attach");
    shared
        .open_command_output(client, Some(pane), "fixture".to_owned(), "preserved")
        .expect("command output");
    let (live, output) = {
        let inner = shared.inner.lock();
        (
            Arc::clone(&inner.terminals[&pane]),
            Arc::clone(
                &inner.clients[&client]
                    .command_output
                    .as_ref()
                    .unwrap()
                    .terminal,
            ),
        )
    };

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("reload-config", [] as [&str; 0]),
        )
        .expect("reload configuration");

    let expected = TerminalAppearance::default_for(TerminalColorScheme::Light);
    assert_eq!(*shared.inner.lock().config.appearance, expected);
    let deadline = Instant::now() + Duration::from_secs(30);
    while live.latest_viewport().background != expected.background
        || output.latest_viewport().background != expected.background
    {
        assert!(Instant::now() < deadline, "appearance did not reach actors");
        thread::sleep(Duration::from_millis(10));
    }

    let state = mailbox.state.lock();
    assert!(state.reliable.iter().any(|frame| {
        matches!(
            decode_protocol_frame(frame),
            Ok(ProtocolMessage::Event(Event {
                payload: EventPayload::AppearanceChanged { appearance, .. },
                ..
            })) if *appearance == expected
        )
    }));
}

#[test]
fn system_color_scheme_change_applies_and_broadcasts_the_appearance() {
    let initial = TerminalAppearance {
        color_scheme: TerminalColorScheme::Dark,
        background: zz_terminal::Color::rgb(0x12, 0x34, 0x56),
        ..TerminalAppearance::default()
    };
    let shared = Arc::new(Shared::with_appearance(43, Arc::new(initial.clone())));
    let mailbox = OutboundMailbox::new();
    let (client, hello) = shared.register_subscribed(
        ClientKind::Interactive,
        None,
        Some(TerminalColorScheme::Dark),
        Arc::clone(&mailbox),
    );
    assert_eq!(hello.appearance, initial);

    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "scheme-change"]),
        )
        .expect("create terminal pane");
    let pane = context.pane.expect("pane");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    assert_eq!(terminal.latest_viewport().background, initial.background);

    shared.set_client_color_scheme(client, TerminalColorScheme::Light);

    let expected = TerminalAppearance::default_for(TerminalColorScheme::Light);
    assert_eq!(*shared.inner.lock().config.appearance, expected);
    let deadline = Instant::now() + Duration::from_secs(30);
    while terminal.latest_viewport().background != expected.background {
        assert!(Instant::now() < deadline, "appearance did not reach actor");
        thread::sleep(Duration::from_millis(10));
    }
    let state = mailbox.state.lock();
    assert!(state.reliable.iter().any(|frame| {
        matches!(
            decode_protocol_frame(frame),
            Ok(ProtocolMessage::Event(Event {
                payload: EventPayload::AppearanceChanged { appearance, .. },
                ..
            })) if *appearance == expected
        )
    }));
}

#[test]
fn copy_mode_style_writes_broadcast_the_patched_appearance() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (client, hello) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    assert_eq!(
        hello.appearance.search_match_color,
        TerminalAppearance::default().search_match_color
    );

    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                ["-g", "copy-mode-match-style", "bg=#ff0000"],
            ),
        )
        .expect("set copy-mode-match-style");

    let expected = AppearanceColor::rgba(255, 0, 0, 255);
    let state = mailbox.state.lock();
    assert!(state.reliable.iter().any(|frame| {
        matches!(
            decode_protocol_frame(frame),
            Ok(ProtocolMessage::Event(Event {
                payload: EventPayload::AppearanceChanged { appearance, .. },
                ..
            })) if appearance.search_match_color == expected
        )
    }));
    drop(state);

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("set-window-option", ["-gu", "copy-mode-match-style"]),
        )
        .expect("unset copy-mode-match-style");
    let state = mailbox.state.lock();
    assert!(state.reliable.iter().any(|frame| {
        matches!(
            decode_protocol_frame(frame),
            Ok(ProtocolMessage::Event(Event {
                payload: EventPayload::AppearanceChanged { appearance, .. },
                ..
            })) if appearance.search_match_color
                == TerminalAppearance::default().search_match_color
        )
    }));
}
