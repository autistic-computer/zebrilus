use super::test_support::command;
use super::*;

#[test]
fn tree_chooser_preserves_session_window_and_pane_scopes() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let pane = context.pane.unwrap();

    assert_eq!(
        engine
            .execute(&mut context, &command("choose-tree", &["-Zs"]))
            .unwrap()
            .effects,
        vec![MuxEffect::ChooseTree {
            pane,
            preview: ChooserPreviewSize::Normal,
            kind: ChooseTreeKind::Windows,
            info_preview: false,
            sessions_only: true,
            filter: None,
            format: None,
            hide_source: false,
            kill_source: false,
            prompt_accept: false,
            sort: TmuxSort::parse(None, false, Some(TmuxSortOrder::Index)).unwrap(),
            key_format: None,
            template: None,
            zoom: true,
        }]
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "choose-tree",
                    &["-Zw", "-f", "#{pane_active}", "-Oname", "-r"]
                ),
            )
            .unwrap()
            .effects,
        vec![MuxEffect::ChooseTree {
            pane,
            preview: ChooserPreviewSize::Normal,
            kind: ChooseTreeKind::Windows,
            info_preview: false,
            sessions_only: false,
            filter: Some("#{pane_active}".to_owned()),
            format: None,
            hide_source: false,
            kill_source: false,
            prompt_accept: false,
            sort: TmuxSort::parse(Some("name"), true, Some(TmuxSortOrder::Index)).unwrap(),
            key_format: None,
            template: None,
            zoom: true,
        }]
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("focus-sidebar", &[]))
            .unwrap()
            .effects,
        vec![MuxEffect::FocusSidebar { pane }]
    );
    assert_eq!(
        engine
            .execute(&mut context, &command("choose-path", &[]))
            .unwrap()
            .effects,
        vec![MuxEffect::ChoosePath {
            pane,
            start_dir: None,
        }]
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("choose-path", &["-c", "/tmp/#{pane_id}"])
            )
            .unwrap()
            .effects,
        vec![MuxEffect::ChoosePath {
            pane,
            start_dir: Some(format!("/tmp/{pane}")),
        }]
    );
    assert!(
        engine
            .execute(&mut context, &command("choose-path", &["extra"]))
            .is_err()
    );

    assert_eq!(
        engine
            .execute(&mut context, &command("choose-tree", &[]))
            .unwrap()
            .effects,
        vec![MuxEffect::ChooseTree {
            pane,
            preview: ChooserPreviewSize::Normal,
            kind: ChooseTreeKind::Panes,
            info_preview: false,
            sessions_only: false,
            filter: None,
            format: None,
            hide_source: false,
            kill_source: false,
            prompt_accept: false,
            sort: TmuxSort::parse(None, false, Some(TmuxSortOrder::Index)).unwrap(),
            key_format: None,
            template: None,
            zoom: false,
        }]
    );

    assert_eq!(
        engine
            .execute(&mut context, &command("choose-tree", &["-sw"]))
            .unwrap()
            .effects,
        vec![MuxEffect::ChooseTree {
            pane,
            preview: ChooserPreviewSize::Normal,
            kind: ChooseTreeKind::Windows,
            info_preview: false,
            sessions_only: true,
            filter: None,
            format: None,
            hide_source: false,
            kill_source: false,
            prompt_accept: false,
            sort: TmuxSort::parse(None, false, Some(TmuxSortOrder::Index)).unwrap(),
            key_format: None,
            template: None,
            zoom: false,
        }]
    );
    let typed =
        CommandInvocation::new("choose-tree", ["{ display -p %% }"]).with_command_blocks([0]);
    let effects = engine.execute(&mut context, &typed).unwrap().effects;
    let [MuxEffect::ChooseTree { template, .. }] = effects.as_slice() else {
        panic!("typed tree chooser effect");
    };
    assert_eq!(template.as_deref(), Some("display-message -p \"%%\""));
    let typed_groups = CommandInvocation::new(
        "choose-tree",
        ["{ display -p one ; display -p two\ndisplay -p three }"],
    )
    .with_command_blocks([0]);
    let effects = engine.execute(&mut context, &typed_groups).unwrap().effects;
    let [MuxEffect::ChooseTree { template, .. }] = effects.as_slice() else {
        panic!("typed tree chooser group effect");
    };
    assert_eq!(
        template.as_deref(),
        Some("display-message -p one ; display-message -p two ;; display-message -p three")
    );
    assert!(matches!(
        engine
            .execute(
                &mut context,
                &command("choose-tree", &["select-pane -t %%"]),
            )
            .unwrap()
            .effects
            .as_slice(),
        [MuxEffect::ChooseTree {
            template: Some(template),
            ..
        }] if template == "select-pane -t %%"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "choose-tree",
                &["-t", "missing", "-O", "not-an-order"],
            ),
        ),
        Err(ServerError::PaneNotFound(target)) if target == "missing"
    ));
}

#[test]
fn chooser_row_flags_parse_the_way_cmd_choose_tree_reads_them() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let pane = context.pane.unwrap();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("choose-tree", &["-hk", "-F", "<#{pane_id}>"]),
            )
            .unwrap()
            .effects,
        vec![MuxEffect::ChooseTree {
            pane,
            preview: ChooserPreviewSize::Normal,
            kind: ChooseTreeKind::Panes,
            info_preview: false,
            sessions_only: false,
            filter: None,
            format: Some("<#{pane_id}>".to_owned()),
            hide_source: true,
            kill_source: true,
            prompt_accept: false,
            sort: TmuxSort::parse(None, false, Some(TmuxSortOrder::Index)).unwrap(),
            key_format: None,
            template: None,
            zoom: false,
        }]
    );

    let control = engine
        .execute(&mut context, &command("choose-buffer", &[]))
        .unwrap()
        .effects;
    for args in [
        &["-y"][..],
        &["-yy"][..],
        &["-y", "-y"][..],
        &["-y", "-y", "-y"][..],
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command("choose-buffer", args))
                .unwrap()
                .effects,
            control,
            "choose-buffer {args:?}"
        );
    }
    let accepted = engine
        .execute(&mut context, &command("choose-tree", &["-y"]))
        .unwrap()
        .effects;
    assert!(matches!(
        accepted.as_slice(),
        [MuxEffect::ChooseTree {
            prompt_accept: true,
            ..
        }]
    ));
}

#[test]
fn choose_buffer_builds_native_and_command_template_effects() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let pane = context.pane.unwrap();

    assert_eq!(
        engine
            .execute(&mut context, &command("choose-buffer", &["-Z"]))
            .unwrap()
            .effects,
        vec![MuxEffect::ChooseBuffer {
            pane,
            preview: ChooserPreviewSize::Normal,
            filter: None,
            format: None,
            kill_source: false,
            sort: TmuxSort::parse(None, false, Some(TmuxSortOrder::Creation)).unwrap(),
            key_format: None,
            template: None,
            zoom: true,
        }]
    );
    assert!(matches!(
        engine
            .execute(
                &mut context,
                &command("choose-buffer", &["-F", "#{buffer_name}", "-k", "-y"]),
            )
            .unwrap()
            .effects
            .as_slice(),
        [MuxEffect::ChooseBuffer {
            format: Some(format),
            kill_source: true,
            ..
        }] if format == "#{buffer_name}"
    ));
    let typed_overflow = CommandInvocation::new(
        "choose-buffer",
        [
            "-F",
            "format",
            "{ display-message one }",
            "{ display-message two }",
        ],
    )
    .with_command_blocks([2, 3]);
    assert_eq!(
        engine
            .execute(&mut context, &typed_overflow)
            .expect_err("typed chooser arity precedes parked flags"),
        ServerError::CommandParse(
            "command choose-buffer: too many arguments (need at most 1)".to_owned()
        )
    );
    let typed =
        CommandInvocation::new("choose-buffer", ["{ display -p %% }"]).with_command_blocks([0]);
    let effects = engine.execute(&mut context, &typed).unwrap().effects;
    let [MuxEffect::ChooseBuffer { template, .. }] = effects.as_slice() else {
        panic!("typed buffer chooser effect");
    };
    assert_eq!(template.as_deref(), Some("display-message -p \"%%\""));
    let typed_empty = CommandInvocation::new("choose-buffer", ["{  }"]).with_command_blocks([0]);
    let effects = engine.execute(&mut context, &typed_empty).unwrap().effects;
    let [MuxEffect::ChooseBuffer { template, .. }] = effects.as_slice() else {
        panic!("empty buffer chooser effect");
    };
    assert_eq!(template.as_deref(), Some(""));
    assert!(matches!(
        engine
            .execute(
                &mut context,
                &command("choose-buffer", &["paste-buffer -b %%"]),
            )
            .unwrap()
            .effects
            .as_slice(),
        [MuxEffect::ChooseBuffer {
            template: Some(template),
            ..
        }] if template == "paste-buffer -b %%"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "choose-buffer",
                &["-t", "missing", "-O", "not-an-order"],
            ),
        ),
        Err(ServerError::PaneNotFound(target)) if target == "missing"
    ));
}

#[test]
fn display_panes_builds_a_pane_mode_effect_with_the_pin_flags() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let pane = context.pane.unwrap();
    let window = context.window.unwrap();
    let session = context.session.unwrap();
    let effect =
        |delay: Option<&str>, selectable, template, kill_source, zoom| MuxEffect::DisplayPanes {
            pane,
            delay: delay.map(str::to_owned),
            selectable,
            template,
            source: None,
            source_window: window,
            source_session: session,
            kill_source,
            zoom,
        };

    for (name, args, expected) in [
        (
            "display-panes",
            Vec::new(),
            effect(None, true, None, false, true),
        ),
        (
            "displayp",
            vec!["-d2500", "-k"],
            effect(Some("2500"), true, None, true, true),
        ),
        (
            "display-panes",
            vec!["-d", "0", "-Z"],
            effect(Some("0"), true, None, false, false),
        ),
        (
            "display-panes",
            vec!["-N", "-t", "work:0.0", "-s", "work:0"],
            effect(None, false, None, false, true),
        ),
    ] {
        assert_eq!(
            engine
                .execute(&mut context, &command(name, &args))
                .unwrap()
                .effects,
            vec![expected]
        );
    }
    engine
        .execute(
            &mut context,
            &command("set-option", &["display-panes-time", "1200"]),
        )
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("display-panes", &["select-pane -t %%%"]),
            )
            .unwrap()
            .effects,
        vec![effect(
            None,
            true,
            Some(CommandPromptTemplate::String(
                "select-pane -t %%%".to_owned()
            )),
            false,
            true,
        )]
    );
    assert_eq!(engine.display_panes_delay(window, None).unwrap(), 1200);
    assert_eq!(engine.display_panes_delay(window, Some("0")).unwrap(), 0);
    for (value, message) in [
        ("forever", "delay invalid"),
        ("-5", "delay too small"),
        ("99999999999", "delay too large"),
    ] {
        assert_eq!(
            engine
                .display_panes_delay(window, Some(value))
                .unwrap_err()
                .tmux_message(),
            message
        );
    }
    for (args, message) in [
        (
            vec!["-t", "missing", "-d", "forever"],
            "can't find pane: missing",
        ),
        (
            vec!["-s", "missing", "-t", "nope"],
            "can't find window: missing",
        ),
    ] {
        let error = engine
            .execute(&mut context, &command("display-panes", &args))
            .unwrap_err();
        assert_eq!(error.tmux_message(), message, "{args:?}");
    }
    assert!(
        engine
            .execute(&mut context, &command("display-panes", &["-b"]))
            .is_err()
    );
}
