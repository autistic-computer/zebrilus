use super::*;

#[test]
fn daemon_context_format_registration_matches_the_oracle() {
    let literal_scopes = [
        (
            "cmd-display-menu.c",
            "cmd_display_menu_get_menu_pos",
            POPUP_POSITION_CONTEXT_FORMATS.as_slice(),
        ),
        (
            "cmd-display-menu.c",
            "cmd_display_menu_get_popup_pos",
            POPUP_POSITION_CONTEXT_FORMATS.as_slice(),
        ),
        (
            "cmd-list-clients.c",
            "cmd_list_clients_exec",
            crate::status::LIST_CLIENTS_CONTEXT_FORMATS.as_slice(),
        ),
        (
            "cmd-show-messages.c",
            "cmd_show_messages_exec",
            crate::status::SHOW_MESSAGES_CONTEXT_FORMATS.as_slice(),
        ),
        (
            "window-buffer.c",
            "window_buffer_get_key",
            CHOOSER_ROW_CONTEXT_FORMATS.as_slice(),
        ),
        (
            "window-tree.c",
            "window_tree_get_key",
            CHOOSER_ROW_CONTEXT_FORMATS.as_slice(),
        ),
    ];
    assert_eq!(
        literal_scopes
            .iter()
            .map(|(_, _, names)| names.len())
            .sum::<usize>(),
        44
    );
    let (mux_hook_payload_names, _) = zz_mux::mux_hook_payload_format_contexts();
    for name in NOTIFY_ADD_CONTEXT_FORMATS
        .iter()
        .filter(|name| **name != HOOK_CONTEXT_FORMAT)
    {
        assert!(
            mux_hook_payload_names.contains(name),
            "daemon hook payload format {name} is not registered"
        );
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let oracle: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("compat/tmux-oracle.json")).expect("read pinned tmux oracle"),
    )
    .expect("parse pinned tmux oracle");
    let oracle_literals = oracle["format_contexts"]["literal_scopes"]
        .as_array()
        .expect("literal format context scopes")
        .iter()
        .flat_map(|scope| {
            let path = scope["path"].as_str().expect("literal context path");
            let function = scope["function"]
                .as_str()
                .expect("literal context function");
            scope["names"]
                .as_array()
                .expect("literal context names")
                .iter()
                .map(move |name| {
                    (
                        path.to_owned(),
                        function.to_owned(),
                        name.as_str().expect("literal context name").to_owned(),
                    )
                })
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(oracle_literals.len(), 204);

    let mut non_daemon_literals = BTreeSet::new();
    for (path, function, names) in zz_mux::mux_literal_format_context_scopes()
        .chain(zz_mux::accepted_native_literal_format_context_scopes())
        .chain(zz_mux::missing_literal_format_context_scopes())
    {
        for name in names {
            assert!(non_daemon_literals.insert((
                path.to_owned(),
                function.to_owned(),
                (*name).to_owned(),
            )));
        }
    }
    assert_eq!(non_daemon_literals.len(), 160);
    assert!(non_daemon_literals.is_subset(&oracle_literals));
    let delegated_literals = oracle_literals
        .difference(&non_daemon_literals)
        .cloned()
        .collect::<BTreeSet<_>>();
    let daemon_literals = literal_scopes
        .into_iter()
        .flat_map(|(path, function, names)| {
            names
                .iter()
                .map(move |name| (path.to_owned(), function.to_owned(), (*name).to_owned()))
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(daemon_literals, delegated_literals);

    let oracle_derived_families = oracle["format_contexts"]["derived_families"]
        .as_array()
        .expect("derived format context families");
    let classified_families = zz_mux::mux_derived_format_context_families()
        .chain(zz_mux::missing_derived_format_context_families())
        .map(|(family, names, patterns)| {
            let oracle_family = oracle_derived_families
                .iter()
                .find(|entry| entry["family"].as_str() == Some(family))
                .unwrap_or_else(|| panic!("missing derived format family {family}"));
            assert_eq!(
                oracle_family["names"],
                serde_json::json!(names),
                "{family} names"
            );
            assert_eq!(
                oracle_family["patterns"],
                serde_json::json!(patterns),
                "{family} patterns"
            );
            family
        })
        .collect::<BTreeSet<_>>();
    let delegated_families = oracle_derived_families
        .iter()
        .filter(|family| {
            !classified_families.contains(
                family["family"]
                    .as_str()
                    .expect("derived format family name"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(delegated_families.len(), 1);
    let run_shell_family = delegated_families[0];
    assert_eq!(
        run_shell_family["family"],
        serde_json::json!("run-shell-position")
    );
    assert_eq!(run_shell_family["names"], serde_json::json!([]));
    assert_eq!(
        run_shell_family["patterns"],
        serde_json::json!([RUN_SHELL_POSITION_CONTEXT_PATTERN])
    );
    assert_eq!(
        run_shell_family["producers"],
        serde_json::json!([{
            "path": "cmd-run-shell.c",
            "function": "cmd_run_shell_exec"
        }])
    );
    assert_eq!(run_shell_position_context_name(1), "1");
    assert_eq!(run_shell_position_context_name(42), "42");
}

#[test]
fn pinned_hook_producer_partition_matches_the_oracle() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let oracle: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("compat/tmux-oracle.json")).expect("read pinned tmux oracle"),
    )
    .expect("parse pinned tmux oracle");
    let oracle_hooks = oracle["hooks"]
        .as_array()
        .expect("pinned tmux hooks")
        .iter()
        .map(|hook| hook.as_str().expect("pinned tmux hook name").to_owned())
        .collect::<Vec<_>>();
    let pinned_hooks = oracle_hooks.iter().cloned().collect::<BTreeSet<_>>();
    assert_eq!(
        pinned_hooks.len(),
        oracle_hooks.len(),
        "duplicate pinned hook"
    );
    assert_eq!(pinned_hooks.len(), 89, "pinned hook count changed");

    let command_names = zz_protocol::command_specs()
        .map(|spec| spec.name)
        .collect::<BTreeSet<_>>();
    let produced_after_hooks = pinned_hooks
        .iter()
        .filter(|hook| {
            hook.strip_prefix("after-")
                .is_some_and(|command| command_names.contains(command))
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        produced_after_hooks.len(),
        38,
        "catalog-derived after-command hook count changed"
    );

    let produced_non_after_hooks = PRODUCED_NON_AFTER_PINNED_HOOKS
        .iter()
        .map(|hook| (*hook).to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        produced_non_after_hooks.len(),
        PRODUCED_NON_AFTER_PINNED_HOOKS.len(),
        "duplicate explicit hook producer"
    );
    assert_eq!(
        produced_non_after_hooks.len(),
        49,
        "explicit hook producer count changed"
    );
    assert!(
        produced_after_hooks.is_disjoint(&produced_non_after_hooks),
        "after-command and explicit hook producers overlap"
    );

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("compat/tmux-gaps.json")).expect("read tmux gap manifest"),
    )
    .expect("parse tmux gap manifest");
    let mut tracked_hooks = BTreeSet::new();
    for gap in manifest["gaps"].as_array().expect("tmux gap list") {
        for item in gap["items"].as_array().expect("tmux gap items") {
            let item = item.as_str().expect("tmux gap item");
            if let Some(hook) = item.strip_prefix("hook:") {
                assert!(
                    tracked_hooks.insert(hook.to_owned()),
                    "duplicate hook gap: {hook}"
                );
            }
        }
    }
    assert_eq!(
        tracked_hooks.len(),
        2,
        "runtime hook gap roster changed: {tracked_hooks:?}"
    );

    let produced_hooks = produced_after_hooks
        .union(&produced_non_after_hooks)
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(produced_hooks.len(), 87, "produced hook count changed");
    assert!(
        produced_hooks.is_disjoint(&tracked_hooks),
        "produced and tracked hooks overlap"
    );
    let partition = produced_hooks
        .union(&tracked_hooks)
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(partition.len(), 89, "hook partition count changed");
    assert_eq!(
        partition, pinned_hooks,
        "hook partition differs from the pin"
    );
}

#[test]
fn window_status_labels_expand_current_plain_markers_and_flags() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "labels", "-n", "main"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-window", ["-d", "-t", "labels:", "-n", "logs"]),
        )
        .unwrap();

    let mut snapshot = engine.state.snapshot();
    expand_window_status_labels(
        &engine,
        "",
        &FormatHookFacts::default(),
        &engine.format_context_snapshot(FormatClient::NoClient),
        &mut snapshot,
    );
    assert_eq!(
        snapshot.sessions[0].windows[0].status_label,
        "#[underscore]#[push-default]0:main*"
    );
    assert_eq!(
        snapshot.sessions[0].windows[1].status_label,
        "#[default]#[push-default]1:logs "
    );

    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                ["-g", "window-status-format", "#[italics]PLAIN:#I:#W:#F"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new(
                "set-window-option",
                [
                    "-g",
                    "window-status-current-format",
                    "#[bold]CURRENT:#I:#W:#F",
                ],
            ),
        )
        .unwrap();
    let mut snapshot = engine.state.snapshot();
    expand_window_status_labels(
        &engine,
        "",
        &FormatHookFacts::default(),
        &engine.format_context_snapshot(FormatClient::NoClient),
        &mut snapshot,
    );
    assert_eq!(
        snapshot.sessions[0].windows[0].status_label,
        "#[underscore]#[push-default]#[bold]CURRENT:0:main:*"
    );
    assert_eq!(
        snapshot.sessions[0].windows[1].status_label,
        "#[default]#[push-default]#[italics]PLAIN:1:logs:"
    );
}

#[test]
fn snapshots_expand_client_formats_for_each_session() {
    let shared = Arc::new(Shared::new(1));
    let (first, second) = {
        let mut inner = shared.inner.lock();
        let (first, first_window, first_pane) = inner
            .engine
            .state
            .create_session("snapshot-first")
            .expect("first session");
        let (second, _, _) = inner
            .engine
            .state
            .create_session("snapshot-second")
            .expect("second session");
        let mut context = ExecutionContext::new(Some(first), Some(first_window), Some(first_pane));
        for (option, value) in [
            ("window-status-format", "#{session_name}=#{session_active}"),
            (
                "window-status-current-format",
                "#{session_name}=#{session_active}",
            ),
        ] {
            inner
                .engine
                .execute(
                    &mut context,
                    &CommandInvocation::new("set-window-option", ["-g", option, value]),
                )
                .expect("set window label format");
        }
        inner
            .engine
            .execute(
                &mut context,
                &CommandInvocation::new(
                    "set-option",
                    ["-g", "pane-border-style", "fg=#{?session_active,red,blue}"],
                ),
            )
            .expect("set pane border format");
        (first, second)
    };
    let (first_client, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("snapshot-first-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (second_client, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("snapshot-second-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .attach(first_client, first)
        .expect("attach first client");
    shared
        .attach(second_client, second)
        .expect("attach second client");

    let inner = shared.inner.lock();
    let presence = snapshot_presence(&inner);
    let mut snapshot = inner.engine.state.snapshot();
    stamp_snapshot_for_client(&inner, first_client, &mut snapshot, &presence);
    for (session, expected_label, expected_colour) in [
        (
            first,
            "#[underscore]#[push-default]snapshot-first=1",
            TmuxColour::Basic(1),
        ),
        (
            second,
            "#[underscore]#[push-default]snapshot-second=0",
            TmuxColour::Basic(4),
        ),
    ] {
        let session = snapshot
            .sessions
            .iter()
            .find(|candidate| candidate.id == session)
            .expect("snapshot session");
        let window = session.windows.first().expect("snapshot window");
        assert_eq!(window.status_label, expected_label);
        assert_eq!(
            window
                .panes
                .values()
                .map(|pane| pane.border_colour)
                .collect::<Vec<_>>(),
            [Some(expected_colour)]
        );
    }
}

#[test]
fn originless_format_client_follows_global_client_activity() {
    let shared = Arc::new(Shared::new(1));
    let (first, second, unattached) = {
        let mut inner = shared.inner.lock();
        let (first, _, _) = inner
            .engine
            .state
            .create_session("format-first")
            .expect("first session");
        let (second, _, _) = inner
            .engine
            .state
            .create_session("format-second")
            .expect("second session");
        let (unattached, _, _) = inner
            .engine
            .state
            .create_session("format-unattached")
            .expect("unattached session");
        (first, second, unattached)
    };
    let (first_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("format-first-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (second_client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("format-second-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .attach(first_client, first)
        .expect("attach first client");
    shared
        .attach(second_client, second)
        .expect("attach second client");

    let mut inner = shared.inner.lock();
    inner.engine.mark_session_active_at(first, 1_700_000_001);
    inner
        .engine
        .mark_session_active_at(unattached, 1_700_000_002);
    inner.client_entry(first_client).activity.replace(1);
    inner.client_entry(second_client).activity.replace(2);
    assert_eq!(best_attached_session(&inner), Some(first));
    assert_eq!(
        current_format_client(&inner, ClientId(u64::MAX - 1)),
        Some(second_client)
    );
}

#[test]
fn window_status_label_styles_follow_pinned_precedence() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "styles", "-n", "main"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-window", ["-d", "-t", "styles:", "-n", "logs"]),
        )
        .unwrap();
    for (option, value) in [
        ("window-status-format", "#I:#W"),
        ("window-status-current-format", "#I:#W"),
        ("window-status-style", "fg=red,bold"),
        ("window-status-current-style", "fg=blue"),
        ("window-status-last-style", "bg=green,nobold"),
        ("window-status-bell-style", "fg=yellow,italics"),
    ] {
        engine
            .execute(
                &mut context,
                &CommandInvocation::new("set-window-option", ["-g", option, value]),
            )
            .unwrap();
    }
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("select-window", ["-t", "styles:1"]),
        )
        .unwrap();

    let snapshot = engine.state.snapshot();
    let main_pane = snapshot.sessions[0].windows[0].active_pane;
    let logs_pane = snapshot.sessions[0].windows[1].active_pane;
    assert!(engine.state.set_pane_bell(main_pane, true));

    let mut snapshot = engine.state.snapshot();
    expand_window_status_labels(
        &engine,
        "",
        &FormatHookFacts::default(),
        &engine.format_context_snapshot(FormatClient::NoClient),
        &mut snapshot,
    );
    let main = &snapshot.sessions[0].windows[0];
    let logs = &snapshot.sessions[0].windows[1];
    assert!(
        main.status_label
            .starts_with("#[fg=red,bold bg=green,nobold fg=yellow,italics]#[push-default]")
    );
    assert!(logs.status_label.starts_with("#[fg=blue]#[push-default]"));

    let main_style = &zz_mux::parse_styled_segments(&main.status_label)[0].style;
    assert_eq!(main_style.fg, Some(zz_mux::TmuxColour::Basic(3)));
    assert_eq!(main_style.bg, Some(zz_mux::TmuxColour::Basic(2)));
    assert_eq!(main_style.attributes.bold, zz_mux::TmuxAttributeState::Off);
    assert_eq!(
        main_style.attributes.italics,
        zz_mux::TmuxAttributeState::On
    );
    let logs_style = &zz_mux::parse_styled_segments(&logs.status_label)[0].style;
    assert_eq!(logs_style.fg, Some(zz_mux::TmuxColour::Basic(4)));
    assert_eq!(logs_style.bg, None);
    assert_eq!(
        logs_style.attributes.bold,
        zz_mux::TmuxAttributeState::Unset
    );

    assert!(engine.state.set_pane_bell(main_pane, false));
    assert!(engine.state.set_pane_bell(logs_pane, true));
    let mut snapshot = engine.state.snapshot();
    expand_window_status_labels(
        &engine,
        "",
        &FormatHookFacts::default(),
        &engine.format_context_snapshot(FormatClient::NoClient),
        &mut snapshot,
    );
    let logs_style =
        &zz_mux::parse_styled_segments(&snapshot.sessions[0].windows[1].status_label)[0].style;
    assert_eq!(logs_style.fg, Some(zz_mux::TmuxColour::Basic(3)));
    assert_eq!(logs_style.bg, None);
    assert_eq!(
        logs_style.attributes.bold,
        zz_mux::TmuxAttributeState::Unset
    );
    assert_eq!(
        logs_style.attributes.italics,
        zz_mux::TmuxAttributeState::On
    );

    for (option, value) in [
        (
            "window-status-current-style",
            "#{?window_active,default,fg=blue}",
        ),
        ("window-status-last-style", "default"),
        ("window-status-bell-style", "default"),
    ] {
        engine
            .execute(
                &mut context,
                &CommandInvocation::new("set-window-option", ["-g", option, value]),
            )
            .unwrap();
    }
    let mut snapshot = engine.state.snapshot();
    expand_window_status_labels(
        &engine,
        "",
        &FormatHookFacts::default(),
        &engine.format_context_snapshot(FormatClient::NoClient),
        &mut snapshot,
    );
    for window in &snapshot.sessions[0].windows {
        let style = &zz_mux::parse_styled_segments(&window.status_label)[0].style;
        assert_eq!(style.fg, Some(zz_mux::TmuxColour::Basic(1)));
        assert_eq!(style.bg, None);
        assert_eq!(style.attributes.bold, zz_mux::TmuxAttributeState::On);
        assert_eq!(style.attributes.italics, zz_mux::TmuxAttributeState::Unset);
    }
}

#[test]
fn browser_url_formats_follow_active_tab_and_subscription_changes() {
    let shared = Arc::new(Shared::new(1));
    let (session, window, terminal, browser) = {
        let mut inner = shared.inner.lock();
        let (session, window, terminal) = inner.engine.state.create_session("urls").unwrap();
        let browser = inner
            .engine
            .state
            .split_pane(
                terminal,
                zz_protocol::Axis::Horizontal,
                PaneKind::Browser(zz_protocol::BrowserDescriptor {
                    tabs: vec![
                        "https://example.com/first".to_owned(),
                        "https://example.com/active".to_owned(),
                    ],
                    active_tab: 1,
                    profile: "default".to_owned(),
                }),
            )
            .unwrap();
        (session, window, terminal, browser)
    };
    let mailbox = OutboundMailbox::new();
    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, Arc::clone(&mailbox));
    shared.attach(control, session).unwrap();
    let run = |args: &[&str]| {
        let mut context = ExecutionContext::new(Some(session), Some(window), Some(browser));
        shared
            .execute(
                control,
                ClientKind::Control,
                &mut context,
                &CommandInvocation::new(args[0], args[1..].iter().copied()),
            )
            .unwrap()
            .output
    };
    assert_eq!(
        run(&[
            "display-message",
            "-p",
            "-t",
            &browser.to_string(),
            "#{browser_url}"
        ]),
        "https://example.com/active"
    );
    assert_eq!(
        run(&[
            "display-message",
            "-p",
            "-t",
            &terminal.to_string(),
            "url=#{browser_url}"
        ]),
        "url="
    );
    let listed = run(&["list-panes", "-F", "#{pane_id}|#{browser_url}"]);
    assert!(
        listed
            .lines()
            .any(|line| line == format!("{browser}|https://example.com/active"))
    );
    assert!(listed.lines().any(|line| line == format!("{terminal}|")));
    run(&[
        "refresh-client",
        "-B",
        &format!("url:{browser}:#{{browser_url}}"),
    ]);
    take_reliable_messages(&mailbox);
    shared.refresh_control_subscriptions();
    let changed_url = |expected: &str| {
        take_reliable_messages(&mailbox).iter().any(|message| matches!(message,
            ProtocolMessage::Event(Event { payload: EventPayload::SubscriptionChanged { name, value, .. }, .. })
            if name == "url" && value == expected))
    };
    assert!(changed_url("https://example.com/active"));
    shared.refresh_control_subscriptions();
    assert!(take_reliable_messages(&mailbox).is_empty());
    {
        let mut inner = shared.inner.lock();
        let pane = inner.engine.state.pane_mut(browser).unwrap();
        let PaneKind::Browser(descriptor) = &mut pane.kind else {
            panic!("browser pane");
        };
        descriptor.active_tab = 0;
    }
    shared.refresh_control_subscriptions();
    assert!(changed_url("https://example.com/first"));
}
