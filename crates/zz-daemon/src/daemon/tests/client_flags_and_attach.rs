use super::*;

#[test]
fn popup_client_resolution_errors_are_exact_and_precede_target_lookup() {
    let shared = Arc::new(Shared::new(1));
    assert!(matches!(
        shared.execute(
            ClientId(99),
            ClientKind::Command,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("display-popup", [] as [&str; 0]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "no current client"
    ));
    for command in [
        CommandInvocation::new("display-menu", ["Item", "i", "display-message item"]),
        CommandInvocation::new("confirm-before", ["display-message confirmed"]),
    ] {
        assert!(matches!(
            shared.execute(
                ClientId(99),
                ClientKind::Command,
                &mut ExecutionContext::default(),
                &command,
            ),
            Err(DaemonError::Server(ServerError::InvalidCommand(message)))
                if message == "no current client"
        ));
    }

    let (shared, _, _, context) = popup_test_workspace("desktop");
    assert!(matches!(
        shared.execute(
            ClientId(99),
            ClientKind::Command,
            &mut context.clone(),
            &CommandInvocation::new("display-popup", ["-c", "bogus"]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "can't find client: bogus"
    ));
    for command in [
        CommandInvocation::new(
            "display-menu",
            ["-c", "bogus", "Item", "i", "display-message item"],
        ),
        CommandInvocation::new(
            "confirm-before",
            ["-t", "bogus", "display-message confirmed"],
        ),
    ] {
        assert!(matches!(
            shared.execute(
                ClientId(99),
                ClientKind::Command,
                &mut context.clone(),
                &command,
            ),
            Err(DaemonError::Server(ServerError::InvalidCommand(message)))
                if message == "can't find client: bogus"
        ));
    }
}

#[test]
fn list_clients_sorts_before_session_filtering_and_preserves_global_lines() {
    let shared = Arc::new(Shared::new(1));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "z"]),
        )
        .unwrap();
    let z = context.session.unwrap();
    shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "A"]),
        )
        .unwrap();
    let a = context.session.unwrap();

    let (zeta, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("zeta".to_owned()),
        Some(TerminalColorScheme::Dark),
        OutboundMailbox::new(),
    );
    let (alpha, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("alpha".to_owned()),
        Some(TerminalColorScheme::Light),
        OutboundMailbox::new(),
    );
    let (_detached, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("ghost".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(zeta, z).unwrap();
    shared.attach(alpha, a).unwrap();

    let listed = shared
        .execute(
            ClientId(91),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("list-clients", [] as [&str; 0]),
        )
        .unwrap();
    assert_eq!(
        listed.output,
        "zeta: z [80x24 unknown] (attached,focused)\nalpha: A [80x24 unknown] (attached,focused)"
    );

    let sorted = shared
        .execute(
            ClientId(91),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("list-clients", ["-O", "name", "-F", "#{client_name}"]),
        )
        .unwrap();
    assert_eq!(sorted.output, "alpha\nzeta");

    let index_fallback = shared
        .execute(
            ClientId(91),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new("list-clients", ["-O", "index", "-F", "#{client_name}"]),
        )
        .unwrap();
    assert_eq!(index_fallback.output, "alpha\nzeta");

    assert!(matches!(
        shared.execute(
            ClientId(91),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "list-clients",
                ["-t", "missing", "-O", "not-an-order"],
            ),
        ),
        Err(DaemonError::Server(ServerError::SessionNotFound(target)))
            if target == "missing"
    ));

    let filtered = shared
        .execute(
            ClientId(91),
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "lsc",
                [
                    "-t",
                    "z",
                    "-O",
                    "name",
                    "-f",
                    "#{==:#{command},list-clients}",
                    "-F",
                    "#{command}:#{line}:#{client_name}:#{session_name}:#{client_width}x#{client_height}:#{client_termname}",
                ],
            ),
        )
        .unwrap();
    assert_eq!(filtered.output, "list-clients:1:zeta:z:80x24:unknown");
}

#[test]
fn list_rows_carry_the_invoking_client() {
    let shared = Arc::new(Shared::new(1));
    let (session, window, pane) = switch_test_session(&shared, "rows-attached");
    switch_test_session(&shared, "rows-other");
    let (attached, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("rows-client".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (bystander, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("rows-bystander".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(attached, session).expect("attach client");
    shared.attach(bystander, session).expect("attach bystander");
    {
        let mut inner = shared.inner.lock();
        inner.client_entry(attached).has_terminal = true;
        inner.client_entry(bystander).has_terminal = true;
        inner.client_entry(bystander).activity.replace(100);
    }
    let format = "#{session_name}=[#{session_active}]:[#{client_name}]";
    let lists = |client: ClientId, kind: ClientKind| -> Vec<String> {
        [
            CommandInvocation::new("list-sessions", ["-F", format]),
            CommandInvocation::new("list-windows", ["-a", "-F", format]),
            CommandInvocation::new("list-panes", ["-a", "-F", format]),
        ]
        .iter()
        .map(|command| {
            let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
            shared
                .execute(client, kind, &mut context, command)
                .expect("list rows")
                .output
                .to_string()
        })
        .collect()
    };

    assert_eq!(
        lists(attached, ClientKind::Interactive),
        vec!["rows-attached=[1]:[rows-client]\nrows-other=[0]:[rows-client]"; 3]
    );

    let (command_client, _) =
        shared.register_subscribed(ClientKind::Command, None, None, OutboundMailbox::new());
    shared
        .inner
        .lock()
        .client_entry(command_client)
        .pid
        .replace(4242);
    assert_eq!(
        lists(command_client, ClientKind::Command),
        vec!["rows-attached=[0]:[client-4242]\nrows-other=[0]:[client-4242]"; 3]
    );

    let unattached_facts = "#{client_pid}|#{client_flags}|#{client_key_table}|#{client_width}|#{client_height}|#{client_session}|#{client_prefix}|#{client_readonly}|#{client_control_mode}|#{window_bigger}|#{window_offset_x}";
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let filtered = shared
        .execute(
            command_client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "list-windows",
                ["-a", "-f", "#{client_pid}", "-F", unattached_facts],
            ),
        )
        .expect("unattached list rows");
    assert_eq!(
        filtered.output,
        "4242|focused|root|80|||0|0|0|0|\n".repeat(2).trim_end()
    );

    let mut context = ExecutionContext::default();
    let created = shared
        .execute(
            command_client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-d",
                    "-P",
                    "-s",
                    "rows-new",
                    "-F",
                    "#{client_name}|#{session_active}",
                ],
            ),
        )
        .expect("new-session -P from an unattached client");
    assert_eq!(created.output, "client-4242|0");

    shared
        .inner
        .lock()
        .client_entry(command_client)
        .features
        .replace(client_features_fact(&["client-features-v1:RGB".to_owned()]));
    let mut context = ExecutionContext::new(Some(session), Some(window), Some(pane));
    let features = shared
        .execute(
            command_client,
            ClientKind::Command,
            &mut context,
            &CommandInvocation::new(
                "list-windows",
                ["-t", "=rows-other", "-F", "#{client_termfeatures}"],
            ),
        )
        .expect("unattached client features");
    assert_eq!(features.output, "RGB");

    let (control, _) =
        shared.register_subscribed(ClientKind::Control, None, None, OutboundMailbox::new());
    let mut context = ExecutionContext::default();
    let attached_print = shared
        .execute(
            control,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                [
                    "-P",
                    "-s",
                    "rows-demo",
                    "-F",
                    "#{client_session}|#{session_active}|#{client_flags}",
                ],
            ),
        )
        .expect("new-session -P from a fresh control client");
    assert_eq!(attached_print.output, "rows-demo|1|focused,control-mode");
}

#[test]
fn requested_colour_features_join_the_roster_alone() {
    let shared = Arc::new(Shared::new(1));
    let (session, _, _) = switch_test_session(&shared, "requested-features");
    let facts =
        |requested: &str| {
            let (client, _) = shared.register_subscribed(
                ClientKind::Interactive,
                Some("tui".to_owned()),
                None,
                OutboundMailbox::new(),
            );
            shared.attach(client, session).expect("attach client");
            let mut inner = shared.inner.lock();
            inner.client_entry(client).has_terminal = true;
            inner.client_entry(client).environment.replace(Arc::new(
                ClientEnvironmentBlob::from_map(BTreeMap::from([("TERM".into(), "xterm".into())])),
            ));
            inner
                .client_entry(client)
                .features
                .replace(client_features_fact(&[format!(
                    "client-features-v1:{requested}"
                )]));
            (
                client_colour_count(&inner, client),
                client_term_features(&inner, client),
            )
        };
    let has = |roster: &str, name: &str| roster.split(',').any(|item| item == name);

    let (colours, roster) = facts("RGB");
    assert_eq!(colours, Some(16_777_216));
    assert!(has(&roster, "RGB"), "{roster}");
    assert!(!has(&roster, "256"), "{roster}");

    let (colours, roster) = facts("256");
    assert_eq!(colours, Some(256));
    assert!(has(&roster, "256"), "{roster}");
    assert!(!has(&roster, "RGB"), "{roster}");

    let (colours, roster) = facts("");
    assert_eq!(colours, Some(8));
    assert!(!has(&roster, "256") && !has(&roster, "RGB"), "{roster}");
}

#[test]
fn a_reply_after_the_hello_joins_the_roster_the_terminfo_entry_started() {
    let shared = Arc::new(Shared::new(1));
    let (session, _, _) = switch_test_session(&shared, "learned-features");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("tui".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared.attach(client, session).expect("attach client");
    {
        let mut inner = shared.inner.lock();
        inner.client_entry(client).has_terminal = true;
        inner
            .client_entry(client)
            .environment
            .replace(Arc::new(ClientEnvironmentBlob::from_map(BTreeMap::from([
                ("TERM".into(), "xterm".into()),
            ]))));
    }
    let facts = || {
        let inner = shared.inner.lock();
        (
            client_colour_count(&inner, client),
            client_term_features(&inner, client),
        )
    };
    let base = facts();
    if base.1.is_empty() {
        return;
    }
    assert_eq!(base.0, Some(8));
    assert_eq!(base.1, "bpaste,ccolour,clipboard,cstyle,focus,title");

    let learned = zz_daemon_client::terminal_default_features("tmux")
        .split(',')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    shared.add_client_terminal_features(client, ClientKind::Interactive, &learned);
    let after = facts();
    assert_eq!(after.0, Some(16_777_216));
    assert_eq!(
        after.1,
        "256,bpaste,ccolour,clipboard,hyperlinks,cstyle,extkeys,focus,mouse,overline,progressbar,RGB,strikethrough,title,usstyle"
    );

    shared.add_client_terminal_features(client, ClientKind::Command, &["sixel".to_owned()]);
    assert_eq!(facts().1, after.1);
}

#[cfg(unix)]
#[test]
fn client_context_formats_share_retained_facts_across_surfaces() {
    let shared = Arc::new(Shared::new(1));
    let (session, window, pane) = switch_test_session(&shared, "format-client");
    let (last_session, _, _) = switch_test_session(&shared, "format-last");
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("desktop".to_owned()),
        Some(TerminalColorScheme::Light),
        Arc::clone(&mailbox),
    );
    shared.attach(client, session).expect("attach client");
    {
        let mut state = mailbox.state.lock();
        state.written_bytes = 1234;
        state.discarded_bytes = 56;
    }
    {
        let mut inner = shared.inner.lock();
        inner.client_entry(client).pid.replace(4242);
        inner
            .client_entry(client)
            .tty
            .replace("/dev/pts/42".to_owned());
        inner.client_entry(client).size.replace((132, 43));
        inner
            .client_entry(client)
            .environment
            .replace(Arc::new(ClientEnvironmentBlob::from_map(BTreeMap::from([
                ("COLORTERM".into(), "truecolor".into()),
                ("LANG".into(), "en_US.UTF-8".into()),
                ("TERM".into(), "xterm-256color".into()),
            ]))));
        inner.client_entry(client).activity_time.replace(222);
        inner.client_entry(client).created_time.replace(111);
        inner.session_last_attached.insert(session, 333);
        inner
            .client_entry(client)
            .last_session
            .replace(last_session);
        inner.client_flags.apply(client, "read-only,active-pane");
        inner
            .client_entry(client)
            .key_engine
            .get_or_insert_default()
            .switch_table(Some("copy-mode".to_owned()));
        inner.terminal_geometries.entry(pane).or_default().insert(
            client,
            TerminalGeometry {
                columns: 120,
                rows: 40,
                cell_width_px: 9,
                cell_height_px: 19,
            },
        );
    }
    let format = "#{client_activity}|#{client_cell_height}|#{client_cell_width}|#{client_colours}|#{client_control_mode}|#{client_created}|#{client_discarded}|#{client_flags}|#{client_height}|#{client_key_table}|#{client_last_session}|#{client_name}|#{client_pid}|#{client_prefix}|#{client_readonly}|#{client_session}|#{client_termfeatures}|#{client_termname}|#{client_termtype}|#{client_theme}|#{client_tty}|#{client_uid}|#{client_user}|#{client_utf8}|#{client_width}|#{client_written}|#{session_last_attached}|#{session_active}";
    let listed = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::new(Some(session), Some(window), Some(pane)),
            &CommandInvocation::new("list-clients", ["-f", "#{session_active}", "-F", format]),
        )
        .expect("list retained client facts")
        .output;
    let fields = listed.split('|').collect::<Vec<_>>();
    assert_eq!(fields.len(), 28);
    assert_eq!(
        &fields[..21],
        [
            "222",
            "19",
            "9",
            "16777216",
            "0",
            "111",
            "56",
            "attached,focused,read-only,UTF-8",
            "43",
            "copy-mode",
            "format-last",
            "/dev/pts/42",
            "4242",
            "1",
            "1",
            "format-client",
            if cfg!(unix) {
                "bpaste,ccolour,clipboard,cstyle,focus,RGB,title"
            } else {
                ""
            },
            "xterm-256color",
            "",
            "light",
            "/dev/pts/42",
        ]
    );
    let expected_identity = shared
        .inner
        .lock()
        .engine
        .format_status_context(Some(session), Some(window), None)
        .detach(FormatNeeds::NONE);
    assert_eq!(fields[21], expected_identity.uid);
    assert_eq!(fields[22], expected_identity.user);
    assert_eq!(&fields[23..27], ["1", "132", "1234", "333"]);
    assert_eq!(fields[27], "1");

    let ordinary = shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::new(Some(session), Some(window), Some(pane)),
            &CommandInvocation::new(
                "list-panes",
                ["-t", "=format-client", "-F", "#{client_name}|#{client_pid}"],
            ),
        )
        .expect("ordinary target client facts");
    assert_eq!(ordinary.output, "/dev/pts/42|4242");
    let targeted = shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::new(Some(session), Some(window), Some(pane)),
            &CommandInvocation::new(
                "display-message",
                [
                    "-p",
                    "-t",
                    "=format-client:",
                    "#{client_name}|#{client_pid}",
                ],
            ),
        )
        .expect("targeted client facts");
    assert_eq!(targeted.output, "/dev/pts/42|4242");

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::new(Some(session), Some(window), Some(pane)),
            &CommandInvocation::new(
                "run-shell",
                [
                    "-C",
                    "set-option -g @inserted-client-facts '#{client_name}|#{client_pid}'",
                ],
            ),
        )
        .expect("inserted command client facts");
    let inserted = shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut ExecutionContext::new(Some(session), Some(window), Some(pane)),
            &CommandInvocation::new("show-options", ["-gqv", "@inserted-client-facts"]),
        )
        .expect("show inserted command client facts");
    assert_eq!(inserted.output, "/dev/pts/42|4242");

    let mut status_request = {
        let mut inner = shared.inner.lock();
        inner
            .engine
            .execute(
                &mut ExecutionContext::new(Some(session), Some(window), Some(pane)),
                &CommandInvocation::new(
                    "set-option",
                    [
                        "-g",
                        "status-left",
                        "#{client_pid}:#{client_tty}:#{session_active}",
                    ],
                ),
            )
            .expect("configure status client facts before capture");
        let option_snapshot = Arc::new(inner.engine.format_option_snapshot());
        status_request(
            &inner,
            client,
            &inner.engine.state.snapshot(),
            option_snapshot,
            format_hook_facts(&inner),
            true,
            FormatNeeds::NONE,
        )
    };
    Arc::make_mut(&mut status_request.formats).enabled = true;
    Arc::make_mut(&mut status_request.formats).lines = 1;
    Arc::make_mut(&mut status_request.formats).left_length = u16::MAX;
    let status = StatusRenderer::default().render_initial(&status_request);
    assert!(
        status.left.contains("4242:/dev/pts/42:1"),
        "{}",
        status.left
    );

    let no_client = shared
        .execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut ExecutionContext::new(Some(session), Some(window), Some(pane)),
            &CommandInvocation::new(
                "display-message",
                [
                    "-p",
                    "-t",
                    "=format-client:",
                    "#{client_name}|#{session_last_attached}|#{session_active}",
                ],
            ),
        )
        .expect("clientless display context");
    assert_eq!(no_client.output, "/dev/pts/42|333|1");

    let control_mailbox = OutboundMailbox::new();
    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        control_mailbox,
    );
    shared.attach(control, session).expect("attach control");
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut ExecutionContext::new(Some(session), Some(window), Some(pane)),
            &CommandInvocation::new("refresh-client", ["-C", "91x31"]),
        )
        .expect("size control client");
    let control_facts = {
        let mut inner = shared.inner.lock();
        inner.client_entry(control).pid.replace(4343);
        inner
            .client_entry(control)
            .tty
            .replace("/dev/pts/43".to_owned());
        inner
            .client_entry(control)
            .environment
            .replace(Arc::new(ClientEnvironmentBlob::from_map(BTreeMap::from([
                ("LANG".into(), "C.UTF-8".into()),
                ("TERM".into(), "xterm-256color".into()),
            ]))));
        client_format_facts(&inner, control, session)
    };
    assert_eq!(control_facts.cell_height, "");
    assert_eq!(control_facts.cell_width, "");
    assert_eq!(control_facts.colours, "");
    assert_eq!(control_facts.control_mode, "1");
    assert_eq!(control_facts.flags, "attached,focused,control-mode,UTF-8");
    assert_eq!(control_facts.height, "");
    assert_eq!(control_facts.name, "/dev/pts/43");
    assert_eq!(control_facts.pid, "4343");
    assert_eq!(control_facts.termfeatures, "");
    assert_eq!(control_facts.termname, "xterm-256color");
    assert_eq!(control_facts.termtype, "");
    assert_eq!(control_facts.theme, "");
    assert_eq!(control_facts.tty, "/dev/pts/43");
    assert_eq!(control_facts.utf8, "1");
    assert_eq!(control_facts.width, "91");
}

#[test]
fn switch_mode_lifetime_kill_follows_the_entry_below_the_top() {
    let shared = Shared::new(1);
    let (_, _, pane) = switch_test_session(&shared, "lifetime");
    let mut inner = shared.inner.lock();
    assert!(push_pane_mode(
        &mut inner,
        pane,
        switch_lifetime_mode(true, false)
    ));
    assert!(push_pane_mode(&mut inner, pane, PaneModeRequest::Clock));
    assert!(pop_pane_mode(&mut inner, pane));
    assert!(inner.modes.pane_mode_kill_panes.is_empty());
    assert_eq!(pane_mode_format_facts(&inner)[&pane], (1, "switch-mode"));
    assert!(pop_pane_mode(&mut inner, pane));
    assert_eq!(inner.modes.pane_mode_kill_panes, [pane]);
    assert!(!inner.modes.pane_modes.contains_key(&pane));
}

#[test]
fn switch_mode_lifetime_promotion_updates_kill_but_reentry_does_not() {
    let shared = Shared::new(1);
    let (_, _, pane) = switch_test_session(&shared, "lifetime");
    let mut inner = shared.inner.lock();
    push_pane_mode(&mut inner, pane, switch_lifetime_mode(false, false));
    assert!(!push_pane_mode(
        &mut inner,
        pane,
        switch_lifetime_mode(true, true)
    ));
    assert!(matches!(
        inner.modes.pane_modes[&pane].last(),
        Some(PaneModeRequest::Switch(mode)) if !mode.kill_source && !mode.zoom
    ));
    push_pane_mode(&mut inner, pane, PaneModeRequest::Clock);
    push_pane_mode(&mut inner, pane, switch_lifetime_mode(true, true));
    assert!(!inner.modes.pane_mode_zooms.contains(&pane));
    assert!(clear_pane_modes(&mut inner, pane));
    assert_eq!(inner.modes.pane_mode_kill_panes, [pane]);
    assert!(!clear_pane_modes(&mut inner, pane));
    inner.modes.pane_mode_kill_panes.clear();
    push_pane_mode(&mut inner, pane, switch_lifetime_mode(true, false));
    push_pane_mode(&mut inner, pane, PaneModeRequest::Clock);
    push_pane_mode(&mut inner, pane, switch_lifetime_mode(false, false));
    clear_pane_modes(&mut inner, pane);
    assert!(inner.modes.pane_mode_kill_panes.is_empty());
}

#[test]
fn switch_mode_lifetime_zoom_restores_only_the_mode_owned_zoom() {
    let shared = Shared::new(1);
    let (_, window, first) = switch_test_session(&shared, "lifetime");
    let mut inner = shared.inner.lock();
    let pane = inner
        .engine
        .state
        .split_pane(first, zz_protocol::Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    push_pane_mode(&mut inner, pane, switch_lifetime_mode(false, true));
    assert_eq!(inner.engine.state.windows[&window].zoomed_pane, Some(pane));
    push_pane_mode(&mut inner, pane, PaneModeRequest::Clock);
    pop_pane_mode(&mut inner, pane);
    assert_eq!(inner.engine.state.windows[&window].zoomed_pane, Some(pane));
    pop_pane_mode(&mut inner, pane);
    assert_eq!(inner.engine.state.windows[&window].zoomed_pane, None);
    inner.engine.state.toggle_zoom(first).unwrap();
    push_pane_mode(&mut inner, pane, switch_lifetime_mode(false, true));
    clear_pane_modes(&mut inner, pane);
    assert_eq!(inner.engine.state.windows[&window].zoomed_pane, Some(first));
}

#[test]
fn switch_mode_lifetime_single_pane_remembers_unzoomed_entry() {
    let shared = Shared::new(1);
    let (_, window, pane) = switch_test_session(&shared, "lifetime");
    let mut inner = shared.inner.lock();
    push_pane_mode(&mut inner, pane, switch_lifetime_mode(false, true));
    assert_eq!(inner.engine.state.windows[&window].zoomed_pane, None);
    inner
        .engine
        .state
        .split_pane(pane, zz_protocol::Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    inner.engine.state.toggle_zoom(pane).unwrap();
    assert_eq!(inner.engine.state.windows[&window].zoomed_pane, Some(pane));
    clear_pane_modes(&mut inner, pane);
    assert_eq!(inner.engine.state.windows[&window].zoomed_pane, None);
    assert!(inner.modes.pane_mode_zooms.is_empty());
}

#[test]
fn switch_mode_window_order_skips_orphan_sessions() {
    let shared = Shared::new(1);
    let (cli, cli_window, pane) = switch_test_session(&shared, "cli");
    let (alpha, alpha_window, _) = switch_test_session(&shared, "alpha");
    let (_, zulu_window, _) = switch_test_session(&shared, "zulu");
    let mut inner = shared.inner.lock();
    for window in [cli_window, alpha_window, zulu_window] {
        inner.engine.state.window_mut(window).unwrap().name = "win".into();
    }
    let mut mode = zz_mux::SwitchMode::new(
        true,
        Some("#{session_name}".to_owned()),
        None,
        false,
        false,
        " ",
    );
    let texts = |inner: &ServerState, mode: &zz_mux::SwitchMode| {
        chooser_presentation::switch_matches(inner, pane, mode)
            .into_iter()
            .map(|entry| (entry.text, entry.target))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        texts(&inner, &mode),
        [
            ("alpha".to_owned(), "=alpha:0.".to_owned()),
            ("cli".to_owned(), "=cli:0.".to_owned()),
            ("zulu".to_owned(), "=zulu:0.".to_owned())
        ]
    );
    mode.key("z", 3, 23);
    assert_eq!(
        chooser_presentation::switch_matches(&inner, pane, &mode)
            .into_iter()
            .map(|entry| (entry.text, entry.columns))
            .collect::<Vec<_>>(),
        [("zulu".to_owned(), vec![0])]
    );
    mode.key("BSpace", 1, 23);
    inner.engine.state.remove_session(alpha);
    assert_eq!(texts(&inner, &mode)[0].1, "=cli:0.");
    inner.engine.state.remove_session(cli);
    assert_eq!(texts(&inner, &mode)[0].1, "=zulu:0.");
    for session in inner
        .engine
        .state
        .sessions
        .keys()
        .copied()
        .collect::<Vec<_>>()
    {
        inner.engine.state.remove_session(session);
    }
    assert!(texts(&inner, &mode).is_empty());
}

#[test]
fn attach_session_r_marks_the_client_read_only_and_gates_its_routes() {
    let shared = Arc::new(Shared::new(1));
    let (_a, _, pane) = switch_test_session(&shared, "A");
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("viewer".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    let mut context = ExecutionContext::default();
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("attach-session", ["-r", "-t", "A"]),
        )
        .expect("read-only attach");
    assert!(shared.inner.lock().client_flags.contains(client));
    assert_eq!(
        format_client_flags(&shared.inner.lock(), client),
        "attached,focused,read-only"
    );

    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::Text {
                pane,
                text: "blocked".to_owned(),
            },
        )
        .expect("read-only text is dropped at the funnel");
    shared
        .input(
            client,
            ClientKind::Interactive,
            &mut context,
            InputMessage::ResizeSplit {
                window: WindowId(0),
                split: SplitId(0),
                ratio_basis_points: 5000,
            },
        )
        .expect("read-only divider drag is dropped at the funnel");

    let blocked = shared.execute_command_request(
        client,
        ClientKind::Interactive,
        &mut context,
        7,
        &CommandInvocation::new("rename-session", ["renamed"]),
    );
    assert!(matches!(
        blocked,
        CommandResponse::Error {
            error: ServerError::InvalidCommand(message),
            ..
        } if message == "client is read-only"
    ));
    let allowed = shared.execute_command_request(
        client,
        ClientKind::Interactive,
        &mut context,
        8,
        &CommandInvocation::new("list-clients", ["-F", "#{client_flags}"]),
    );
    assert!(matches!(
        allowed,
        CommandResponse::Success { output, .. } if output.contains("attached,focused,read-only")
    ));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-r"]),
        )
        .expect("switch-client -r clears read-only");
    assert!(!shared.inner.lock().client_flags.contains(client));
}

#[test]
fn requested_client_flags_mutate_in_tmux_order_and_survive_detach_and_switch() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, _) = switch_test_session(&shared, "A");
    let (b, _, _) = switch_test_session(&shared, "B");
    let mailbox = OutboundMailbox::new();
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("flags".to_owned()),
        None,
        Arc::clone(&mailbox),
    );
    let mut context = ExecutionContext::default();

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "attach-session",
                [
                    "-t",
                    "A",
                    "-f",
                    "read-only,ignore-size,active-pane,no-detach-on-destroy,no-output,wait-exit,pause-after=4,,Ignore-Size",
                ],
            ),
        )
        .expect("attach with requested flags");
    assert_eq!(
        format_client_flags(&shared.inner.lock(), client),
        "attached,focused,ignore-size,no-detach-on-destroy,read-only"
    );
    let flags = shared.inner.lock().client_flags.get(client);
    assert_eq!(flags.reconnect_flags(), "ignore-size,no-detach-on-destroy");
    let messages = take_reliable_messages(&mailbox);
    assert!(messages.iter().any(|message| matches!(
        message,
        ProtocolMessage::Attached {
            read_only: true,
            client_flags,
            ..
        } if client_flags == "ignore-size,no-detach-on-destroy"
    )));
    assert!(client_ignores_size(&shared.inner.lock(), client));

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("attach-session", ["-t", "B"]),
        )
        .expect("plain attach preserves flags");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), client),
        Some(b)
    );
    assert!(client_ignores_size(&shared.inner.lock(), client));

    shared.detach(client);
    assert_eq!(
        format_client_flags(&shared.inner.lock(), client),
        "focused,ignore-size,no-detach-on-destroy,read-only"
    );
    shared.attach(client, a).expect("plain reattach");
    assert_eq!(
        format_client_flags(&shared.inner.lock(), client),
        "attached,focused,ignore-size,no-detach-on-destroy,read-only"
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "attach-session",
                [
                    "-t",
                    "A",
                    "-f",
                    "!read-only,!ignore-size,!active-pane,!no-detach-on-destroy",
                ],
            ),
        )
        .expect("clear mutable requested flags");
    assert_eq!(
        format_client_flags(&shared.inner.lock(), client),
        "attached,focused,read-only"
    );
    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("switch-client", ["-r"]),
        )
        .expect("switch-client toggles read-only off");
    assert_eq!(
        format_client_flags(&shared.inner.lock(), client),
        "attached,focused"
    );

    shared.unregister(client);
    assert_eq!(
        shared.inner.lock().client_flags.get(client),
        ClientFlagState::default()
    );
}

#[test]
fn requested_control_flags_use_the_common_mutation_and_format_contract() {
    let shared = Arc::new(Shared::new(1));
    switch_test_session(&shared, "control");
    let (client, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let mut context = ExecutionContext::default();

    shared
        .execute(
            client,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new(
                "attach-session",
                [
                    "-t",
                    "control",
                    "-f",
                    "ignore-size,no-detach-on-destroy,no-output,wait-exit,pause-after=3suffix,read-only,active-pane",
                ],
            ),
        )
        .expect("control attach flags");
    assert_eq!(
        format_client_flags(&shared.inner.lock(), client),
        "attached,focused,control-mode,ignore-size,no-detach-on-destroy,no-output,wait-exit,pause-after=3,read-only"
    );

    shared
        .execute(
            client,
            ClientKind::Control,
            &mut context,
            &CommandInvocation::new(
                "attach-session",
                [
                    "-t",
                    "control",
                    "-f",
                    "!ignore-size,!no-detach-on-destroy,!no-output,!wait-exit,!pause-after,!active-pane",
                ],
            ),
        )
        .expect("clear control flags");
    assert_eq!(
        format_client_flags(&shared.inner.lock(), client),
        "attached,focused,control-mode,read-only"
    );
}

#[test]
fn attach_flags_follow_resolution_and_precede_the_terminal_open_error() {
    let shared = Arc::new(Shared::new(1));
    switch_test_session(&shared, "target");
    let (client, _) = shared
        .register(
            ClientKind::Interactive,
            ClientInstanceId::default(),
            Some("headless".to_owned()),
            None,
            false,
            false,
        )
        .expect("register headless client");
    let mut context = ExecutionContext::default();

    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "attach-session",
                ["-t", "missing", "-f", "ignore-size"],
            ),
        ),
        Err(DaemonError::Server(ServerError::SessionNotFound(target))) if target == "missing"
    ));
    assert!(!client_ignores_size(&shared.inner.lock(), client));

    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "attach-session",
                ["-t", "target", "-c", "/changed", "-f", "ignore-size"],
            ),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "open terminal failed: not a terminal"
    ));
    assert!(client_ignores_size(&shared.inner.lock(), client));
    let target = shared
        .inner
        .lock()
        .engine
        .state
        .sessions
        .values()
        .find(|session| session.name == "target")
        .unwrap()
        .id;
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .state
            .session_working_directory(target),
        Some(Path::new("/changed"))
    );

    let sessions = shared.inner.lock().engine.state.sessions.len();
    assert!(matches!(
        shared.execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-s", "fresh", "-f", "no-detach-on-destroy"],
            ),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "open terminal failed: not a terminal"
    ));
    assert_eq!(shared.inner.lock().engine.state.sessions.len(), sessions);
    assert!(
        !shared
            .inner
            .lock()
            .client_flags
            .get(client)
            .no_detach_on_destroy
    );

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-d", "-s", "detached", "-f", "no-detach-on-destroy"],
            ),
        )
        .expect("detached creation ignores client flags");
    assert!(
        !shared
            .inner
            .lock()
            .client_flags
            .get(client)
            .no_detach_on_destroy
    );
}

#[test]
fn attach_environment_refresh_runs_after_terminal_preflight_and_honors_dash_e() {
    let shared = Arc::new(Shared::new(1));
    let (session, _, _) = switch_test_session(&shared, "target");
    let mut setup = ExecutionContext::default();
    shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut setup,
            &CommandInvocation::new(
                "set-option",
                ["-t", "target", "update-environment", "ATTACHED_ENV"],
            ),
        )
        .expect("session update environment");
    shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut setup,
            &CommandInvocation::new(
                "set-environment",
                ["-t", "target", "ATTACHED_ENV", "before"],
            ),
        )
        .expect("baseline session environment");
    let original = shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut setup,
            &CommandInvocation::new("new-window", ["-d", "-t", "target", "--", "sleep", "30"]),
        )
        .expect("original environment window");
    let original_pane = original
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .expect("original environment pane");
    let (headless, _) = shared
        .register(
            ClientKind::Interactive,
            ClientInstanceId::default(),
            Some("headless-environment".to_owned()),
            None,
            false,
            false,
        )
        .expect("register headless client");
    shared
        .inner
        .lock()
        .client_entry(headless)
        .environment
        .replace(client_environment_fact(&["ATTACHED_ENV=headless".into()]));

    assert!(matches!(
        shared.execute(
            headless,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("attach-session", ["-t", "target"]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "open terminal failed: not a terminal"
    ));
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .environment_for_session(session)
            .unwrap()
            .into_iter()
            .find(|(name, _)| name == "ATTACHED_ENV"),
        Some(("ATTACHED_ENV".into(), Some("before".into())))
    );

    let (control, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("control-environment".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    shared
        .inner
        .lock()
        .client_entry(control)
        .environment
        .replace(client_environment_fact(&["ATTACHED_ENV=control".into()]));
    let mut control_context = ExecutionContext::default();
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new("attach-session", ["-t", "target"]),
        )
        .expect("control environment refresh");
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .environment_for_session(session)
            .unwrap()
            .into_iter()
            .find(|(name, _)| name == "ATTACHED_ENV"),
        Some(("ATTACHED_ENV".into(), Some("control".into())))
    );
    assert!(
        shared.inner.lock().terminal_spawns[&original_pane]
            .env
            .contains(&("ATTACHED_ENV".into(), Some("before".into())))
    );
    let future = shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new("new-window", ["-d", "-t", "target", "--", "sleep", "30"]),
        )
        .expect("refreshed environment window");
    let future_pane = future
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .expect("refreshed environment pane");
    assert!(
        shared.inner.lock().terminal_spawns[&future_pane]
            .env
            .contains(&("ATTACHED_ENV".into(), Some("control".into())))
    );
    shared
        .execute(
            ClientId(90),
            ClientKind::Command,
            &mut setup,
            &CommandInvocation::new(
                "set-environment",
                ["-t", "target", "ATTACHED_ENV", "preserved"],
            ),
        )
        .expect("preserved environment");
    shared
        .execute(
            control,
            ClientKind::Control,
            &mut control_context,
            &CommandInvocation::new("attach-session", ["-E", "-t", "target"]),
        )
        .expect("control attach without refresh");
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .environment_for_session(session)
            .unwrap()
            .into_iter()
            .find(|(name, _)| name == "ATTACHED_ENV"),
        Some(("ATTACHED_ENV".into(), Some("preserved".into())))
    );

    shared
        .inner
        .lock()
        .client_entry(control)
        .environment
        .replace(client_environment_fact(&["ATTACHED_ENV=native".into()]));
    shared
        .attach_target(control, ClientKind::Control, &mut control_context, "target")
        .expect("native attach environment refresh");
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .environment_for_session(session)
            .unwrap()
            .into_iter()
            .find(|(name, _)| name == "ATTACHED_ENV"),
        Some(("ATTACHED_ENV".into(), Some("native".into())))
    );
}

#[test]
fn targeted_switch_uses_the_selected_clients_environment() {
    let shared = Arc::new(Shared::new(1));
    let (a, _, _) = switch_test_session(&shared, "A");
    let (b, _, _) = switch_test_session(&shared, "B");
    let (target, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("environment-target".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let (caller, _) = shared
        .register(
            ClientKind::Command,
            ClientInstanceId::default(),
            Some("environment-caller".to_owned()),
            None,
            false,
            false,
        )
        .expect("register command client");
    {
        let mut inner = shared.inner.lock();
        inner
            .client_entry(target)
            .environment
            .replace(client_environment_fact(&["SWITCH_ENV=target".into()]));
        inner
            .client_entry(caller)
            .environment
            .replace(client_environment_fact(&["SWITCH_ENV=caller".into()]));
    }
    shared.attach(target, a).expect("target attachment");
    let mut caller_context = ExecutionContext::default();
    shared
        .execute(
            caller,
            ClientKind::Command,
            &mut caller_context,
            &CommandInvocation::new(
                "set-option",
                ["-t", "B", "update-environment", "SWITCH_ENV"],
            ),
        )
        .expect("switch destination environment option");
    shared
        .execute(
            caller,
            ClientKind::Command,
            &mut caller_context,
            &CommandInvocation::new("switch-client", ["-c", "environment-target", "-t", "B"]),
        )
        .expect("targeted switch");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), target),
        Some(b)
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .environment_for_session(b)
            .unwrap()
            .into_iter()
            .find(|(name, _)| name == "SWITCH_ENV"),
        Some(("SWITCH_ENV".into(), Some("target".into())))
    );

    shared
        .execute(
            caller,
            ClientKind::Command,
            &mut caller_context,
            &CommandInvocation::new("set-environment", ["-t", "B", "SWITCH_ENV", "preserved"]),
        )
        .expect("switch preserve baseline");
    shared
        .execute(
            caller,
            ClientKind::Command,
            &mut caller_context,
            &CommandInvocation::new(
                "switch-client",
                ["-E", "-c", "environment-target", "-t", "B"],
            ),
        )
        .expect("targeted switch without refresh");
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .environment_for_session(b)
            .unwrap()
            .into_iter()
            .find(|(name, _)| name == "SWITCH_ENV"),
        Some(("SWITCH_ENV".into(), Some("preserved".into())))
    );
}

#[test]
fn a_remote_client_leaves_session_environments_to_the_daemon() {
    let shared = Arc::new(Shared::new(1));
    let (session, _, _) = switch_test_session(&shared, "target");
    let (other, _, _) = switch_test_session(&shared, "other");
    let run = |client, kind, context: &mut ExecutionContext, args: &[&str]| {
        shared.execute(
            client,
            kind,
            context,
            &CommandInvocation::new(args[0], args[1..].iter().copied()),
        )
    };
    let mut setup = ExecutionContext::default();
    for args in [
        ["set-option", "-g", "update-environment", "REMOTE_ENV"].as_slice(),
        &["set-environment", "-t", "target", "REMOTE_ENV", "host"],
        &["set-environment", "-t", "other", "REMOTE_ENV", "host"],
    ] {
        run(ClientId(90), ClientKind::Command, &mut setup, args).expect("setup");
    }
    let value = |session| {
        shared
            .inner
            .lock()
            .engine
            .environment_for_session(session)
            .unwrap()
            .into_iter()
            .find(|(name, _)| name == "REMOTE_ENV")
            .map(|(_, value)| value)
    };
    let host = Some(Some(RawText::from("host")));
    let (remote, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("remote-environment".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    {
        let mut inner = shared.inner.lock();
        let entry = inner.client_entry(remote);
        entry.remote = true;
        entry
            .environment
            .replace(client_environment_fact(&["REMOTE_ENV=caller".into()]));
    }
    let mut remote_context = ExecutionContext::default();
    run(
        remote,
        ClientKind::Control,
        &mut remote_context,
        &["attach-session", "-t", "target"],
    )
    .expect("remote attach");
    assert_eq!(value(session), host);
    shared
        .attach_target(remote, ClientKind::Control, &mut remote_context, "target")
        .expect("remote native attach");
    assert_eq!(value(session), host);
    run(
        ClientId(90),
        ClientKind::Command,
        &mut setup,
        &["switch-client", "-c", "remote-environment", "-t", "other"],
    )
    .expect("targeted switch of a remote client");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), remote),
        Some(other)
    );
    assert_eq!(value(other), host);
    run(
        remote,
        ClientKind::Control,
        &mut remote_context,
        &["new-session", "-d", "-s", "fresh"],
    )
    .expect("remote new session");
    let fresh = shared
        .inner
        .lock()
        .engine
        .resolve_session(Some("fresh"), None)
        .expect("fresh session");
    assert_eq!(value(fresh), None);

    shared.inner.lock().client_entry(remote).remote = false;
    run(
        remote,
        ClientKind::Control,
        &mut remote_context,
        &["attach-session", "-t", "target"],
    )
    .expect("local attach");
    assert_eq!(value(session), Some(Some(RawText::from("caller"))));
}

#[test]
fn a_remote_clients_environment_never_names_its_hook_origin() {
    let shared = Arc::new(Shared::new(1));
    let (_, _, first) = switch_test_session(&shared, "first");
    let (_, _, second) = switch_test_session(&shared, "second");
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("origin".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let mut inner = shared.inner.lock();
    let recent = inner
        .engine
        .state
        .most_recent_context()
        .map(|(_, _, pane)| pane)
        .expect("recent pane");
    let named = if recent == first { second } else { first };
    inner
        .client_entry(client)
        .environment
        .replace(client_environment_fact(&[RawText::from(format!(
            "ZZ_PANE={named}"
        ))]));
    let origin = |inner: &ServerState| {
        client_lifecycle_event(inner, "client-created", client).and_then(|event| event.context.pane)
    };
    assert_eq!(origin(&inner), Some(named));
    inner.client_entry(client).remote = true;
    assert_eq!(origin(&inner), Some(recent));
}

#[test]
fn attached_fresh_new_session_applies_flags_after_terminal_materialization() {
    let shared = Arc::new(Shared::new(1));
    let (client, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("fresh-flags".to_owned()),
        None,
        OutboundMailbox::new(),
    );
    let mut context = ExecutionContext::default();

    shared
        .execute(
            client,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new(
                "new-session",
                ["-s", "fresh-flags", "-f", "ignore-size,active-pane"],
            ),
        )
        .expect("create and attach with flags");
    let flags = shared.inner.lock().client_flags.get(client);
    assert!(flags.ignore_size);
    assert_eq!(
        format_client_flags(&shared.inner.lock(), client),
        "attached,focused,ignore-size"
    );
}

#[cfg(unix)]
#[test]
fn nested_new_session_errors_precede_refusal_without_mutation() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (creator, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut creator_context = ExecutionContext::default();
    shared
        .execute(
            creator,
            ClientKind::Interactive,
            &mut creator_context,
            &CommandInvocation::new("new-session", ["-s", "outer"]),
        )
        .unwrap();
    let outer = creator_context.session.unwrap();
    shared
        .execute(
            creator,
            ClientKind::Interactive,
            &mut creator_context,
            &CommandInvocation::new(
                "set-option",
                ["-s", "command-alias[90]", "nest=new-session"],
            ),
        )
        .unwrap();
    let pane = creator_context.pane.unwrap();
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    wait_for_terminal_identity(&terminal);
    let pane_tty = terminal.tty().unwrap().to_string_lossy().into_owned();
    let nested_mailbox = OutboundMailbox::new();
    let (nested, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("nested-errors".to_owned()),
        None,
        Arc::clone(&nested_mailbox),
    );
    {
        let mut inner = shared.inner.lock();
        inner.client_entry(nested).tty.replace(pane_tty);
        inner.client_entry(nested).nested = true;
    }
    let mut nested_context = ExecutionContext::default();
    let names_before = shared
        .inner
        .lock()
        .engine
        .state
        .sessions
        .values()
        .map(|session| session.name.clone())
        .collect::<BTreeSet<_>>();
    let refusal = "sessions should be nested with care, unset $TMUX to force";
    {
        let mut error = |name: &str, arguments: &[&str]| {
            let invocation = CommandInvocation::new(name, arguments.iter().copied());
            match shared
                .execute(
                    nested,
                    ClientKind::Interactive,
                    &mut nested_context,
                    &invocation,
                )
                .unwrap_err()
            {
                DaemonError::Server(ServerError::InvalidCommand(message)) => message,
                other => panic!("unexpected nested error: {other:?}"),
            }
        };

        for (spelling, session) in [
            ("new-session", "canonical-inner"),
            ("new", "alias-inner"),
            ("new-s", "prefix-inner"),
            ("nest", "user-alias-inner"),
        ] {
            assert_eq!(error(spelling, &["-s", session]), refusal);
        }
        assert_eq!(
            error("new-session", &["-t", "group", "echo"]),
            "command or window name given with target"
        );
        assert_eq!(
            error(
                "new-session",
                &["-t", "group", "-n", "window", "-s", "fresh"],
            ),
            "command or window name given with target"
        );
        assert_eq!(
            error("new-session", &["-n", "bad\nwindow", "-s", "bad\nsession"],),
            "invalid window name: bad\nwindow"
        );
        assert_eq!(
            error("new-session", &["-n", "window", "-s", "bad\nsession"],),
            "invalid session name: bad\nsession"
        );
        assert_eq!(
            error(
                "new-session",
                &["-s", "outer", "-t", "bad\ngroup", "-x", "0"],
            ),
            "duplicate session: outer"
        );
        assert_eq!(
            error(
                "new-session",
                &["-s", "fresh", "-t", "bad\ngroup", "-x", "0"],
            ),
            "invalid session group name: bad\ngroup"
        );
        assert_eq!(
            error(
                "new-session",
                &["-s", "fresh", "-t", "valid-group", "-x", "0"],
            ),
            refusal
        );
        assert_eq!(error("new-session", &["-A", "-s", "outer"]), refusal);
        assert_eq!(error("new-session", &["-s", "fresh", "-y", "0"]), refusal);
    }

    let names_after_errors = shared
        .inner
        .lock()
        .engine
        .state
        .sessions
        .values()
        .map(|session| session.name.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(names_after_errors, names_before);

    shared
        .execute(
            nested,
            ClientKind::Interactive,
            &mut nested_context,
            &CommandInvocation::new("new-session", ["-d", "-s", "nested-detached"]),
        )
        .unwrap();
    assert!(
        shared
            .inner
            .lock()
            .engine
            .state
            .sessions
            .values()
            .any(|session| session.name == "nested-detached")
    );

    shared
        .inner
        .lock()
        .client_mut(nested)
        .map(|client| std::mem::take(&mut client.nested));
    shared
        .attach_target(
            nested,
            ClientKind::Interactive,
            &mut nested_context,
            "outer",
        )
        .unwrap();
    shared.inner.lock().client_entry(nested).nested = true;
    shared
        .execute(
            nested,
            ClientKind::Interactive,
            &mut nested_context,
            &CommandInvocation::new("new-session", ["-s", "attached-fresh"]),
        )
        .unwrap();
    let attached_fresh = client_attached_session(&shared.inner.lock(), nested).unwrap();
    assert_eq!(
        shared.inner.lock().engine.state.sessions[&attached_fresh].name,
        "attached-fresh"
    );
    assert!(matches!(
        shared.execute(
            nested,
            ClientKind::Interactive,
            &mut nested_context,
            &CommandInvocation::new(
                "new-session",
                ["-A", "-s", "outer", "-c", "/blocked-interactive"],
            ),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == refusal
    ));
    assert_eq!(
        client_attached_session(&shared.inner.lock(), nested),
        Some(attached_fresh)
    );
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .state
            .session_working_directory(outer),
        None
    );
    assert!(matches!(
        shared.execute(
            nested,
            ClientKind::Interactive,
            &mut nested_context,
            &CommandInvocation::new("new-session", ["-A", "-d", "-s", "outer"]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == refusal
    ));
    shared
        .execute(
            nested,
            ClientKind::Interactive,
            &mut nested_context,
            &CommandInvocation::new("new-session", ["-A", "-d", "-s", "attached-detached-miss"]),
        )
        .unwrap();
    assert_eq!(
        client_attached_session(&shared.inner.lock(), nested),
        Some(attached_fresh)
    );
}

#[cfg(unix)]
#[test]
fn nested_attach_refuses_only_preexisting_sessions_when_the_caller_tty_matches_a_pane() {
    let shared = Arc::new(Shared::new(1));
    let mailbox = OutboundMailbox::new();
    let (creator, _) =
        shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
    let mut context = ExecutionContext::default();
    shared
        .execute(
            creator,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-s", "outer"]),
        )
        .expect("new session");
    let outer = context.session.expect("outer session");
    let pane = context.pane.expect("pane");
    let terminal = Arc::clone(&shared.inner.lock().terminals[&pane]);
    wait_for_terminal_identity(&terminal);
    let pane_tty = terminal
        .tty()
        .expect("pane tty")
        .to_string_lossy()
        .into_owned();
    shared
        .execute(
            creator,
            ClientKind::Interactive,
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "##{command}"]),
        )
        .expect("literal format-shaped attach target");
    assert!(
        shared
            .inner
            .lock()
            .engine
            .state
            .sessions
            .values()
            .any(|session| session.name == "#{command}")
    );

    let nested_mailbox = OutboundMailbox::new();
    let (nested, _) = shared.register_subscribed(
        ClientKind::Control,
        Some("nested".to_owned()),
        None,
        Arc::clone(&nested_mailbox),
    );
    {
        let mut inner = shared.inner.lock();
        inner.client_entry(nested).tty.replace(pane_tty);
        inner.client_entry(nested).nested = true;
    }
    let mut nested_context = ExecutionContext::default();
    let refusal = shared
        .execute(
            nested,
            ClientKind::Control,
            &mut nested_context,
            &CommandInvocation::new("attach-session", ["-t", "outer"]),
        )
        .expect_err("nested attach refused");
    let message = match refusal {
        DaemonError::Server(ServerError::InvalidCommand(message)) => message,
        other => panic!("unexpected refusal shape: {other:?}"),
    };
    assert_eq!(
        message,
        "sessions should be nested with care, unset $TMUX to force"
    );
    assert!(matches!(
        shared.execute(
            nested,
            ClientKind::Control,
            &mut nested_context,
            &CommandInvocation::new("attach-session", ["-t", "missing"]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "sessions should be nested with care, unset $TMUX to force"
    ));
    assert!(matches!(
        shared.execute(
            nested,
            ClientKind::Control,
            &mut nested_context,
            &CommandInvocation::new("new-session", ["-s", "outer"]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "duplicate session: outer"
    ));
    shared
        .execute(
            nested,
            ClientKind::Control,
            &mut nested_context,
            &CommandInvocation::new("new-session", ["-s", "inner"]),
        )
        .expect("fresh attached creation from a nested tty is allowed");
    let inner_session = nested_context.session.expect("fresh attached session");
    assert_eq!(
        shared.inner.lock().engine.state.sessions[&inner_session].name,
        "inner"
    );
    assert_eq!(
        client_attached_session(&shared.inner.lock(), nested),
        Some(inner_session)
    );
    shared.detach_client_state(nested, false);
    shared
        .execute(
            nested,
            ClientKind::Control,
            &mut nested_context,
            &CommandInvocation::new("new-session", ["-A", "-s", "inner-a"]),
        )
        .expect("fresh new-session attach miss from a nested tty is allowed");
    let inner_a_session = nested_context.session.expect("fresh -A attached session");
    assert_eq!(
        shared.inner.lock().engine.state.sessions[&inner_a_session].name,
        "inner-a"
    );
    assert_eq!(
        client_attached_session(&shared.inner.lock(), nested),
        Some(inner_a_session)
    );
    shared.detach_client_state(nested, false);
    assert!(matches!(
        shared.execute(
            nested,
            ClientKind::Control,
            &mut nested_context,
            &CommandInvocation::new(
                "new-session",
                ["-A", "-s", "outer", "-c", "/blocked-control"],
            ),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "sessions should be nested with care, unset $TMUX to force"
    ));
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .state
            .session_working_directory(outer),
        None
    );
    assert!(matches!(
        shared.execute(
            nested,
            ClientKind::Control,
            &mut nested_context,
            &CommandInvocation::new(
                "new-session",
                ["-A", "-d", "-s", "outer", "-c", "/blocked-detached"],
            ),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "sessions should be nested with care, unset $TMUX to force"
    ));
    assert_eq!(
        shared
            .inner
            .lock()
            .engine
            .state
            .session_working_directory(outer),
        None
    );
    shared
        .execute(
            nested,
            ClientKind::Control,
            &mut nested_context,
            &CommandInvocation::new("new-session", ["-A", "-d", "-s", "#{command}"]),
        )
        .expect("formatted detached creation is not refused from its raw text");
    {
        let inner = shared.inner.lock();
        let expanded = inner
            .engine
            .state
            .sessions
            .iter()
            .find_map(|(id, session)| (session.name == "new-session").then_some(*id))
            .expect("expanded detached session");
        assert!(
            inner
                .attached
                .get(&expanded)
                .is_none_or(|clients| !clients.contains(&nested))
        );
    }
    nested_context.set_client_size(Some((1, 1)));
    nested_context.set_client_working_directory(Some(PathBuf::from("/stale")));
    {
        let mut inner = shared.inner.lock();
        inner.client_entry(nested).size.replace((132, 43));
        inner
            .client_entry(nested)
            .working_directory
            .replace(PathBuf::from("/fresh"));
    }
    let target_before_formatted_attach = (
        nested_context.session,
        nested_context.window,
        nested_context.pane,
    );
    assert!(matches!(
        shared.execute(
            nested,
            ClientKind::Control,
            &mut nested_context,
            &CommandInvocation::new(
                "new-session",
                ["-A", "-d", "-s", "#{command}"],
            ),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "sessions should be nested with care, unset $TMUX to force"
    ));
    assert_eq!(
        (
            nested_context.session,
            nested_context.window,
            nested_context.pane,
        ),
        target_before_formatted_attach
    );
    assert_eq!(nested_context.client_size(), Some((132, 43)));
    assert_eq!(
        nested_context.client_working_directory(),
        Some(Path::new("/fresh"))
    );
    assert!(
        shared
            .inner
            .lock()
            .attached
            .values()
            .all(|clients| !clients.contains(&nested))
    );
    shared
        .execute(
            nested,
            ClientKind::Control,
            &mut nested_context,
            &CommandInvocation::new("new-session", ["-A", "-d", "-s", "fresh-#{command}"]),
        )
        .expect("detached creation from a nested tty is allowed");
    {
        let inner = shared.inner.lock();
        let fresh = inner
            .engine
            .state
            .sessions
            .iter()
            .find_map(|(id, session)| (session.name == "fresh-new-session").then_some(*id))
            .expect("fresh detached session");
        assert!(
            inner
                .attached
                .get(&fresh)
                .is_none_or(|clients| !clients.contains(&nested))
        );
    }
    assert!(matches!(
        shared.attach_target(nested, ClientKind::Control, &mut nested_context, "outer"),
        Err(ServerError::InvalidCommand(message))
            if message == "sessions should be nested with care, unset $TMUX to force"
    ));

    shared
        .inner
        .lock()
        .client_mut(nested)
        .and_then(|c| c.tty.take());
    shared
        .attach_target(nested, ClientKind::Control, &mut nested_context, "outer")
        .expect("a nested marker without a tty identity is allowed");
    shared.detach_client_state(nested, false);
    shared
        .inner
        .lock()
        .client_entry(nested)
        .tty
        .replace("/dev/zz-not-a-pane".to_owned());
    shared
        .attach_target(nested, ClientKind::Control, &mut nested_context, "outer")
        .expect("a nested marker with a mismatched tty identity is allowed");
    shared.detach_client_state(nested, false);
    shared
        .inner
        .lock()
        .client_entry(nested)
        .tty
        .replace(terminal.tty().unwrap().to_string_lossy().into_owned());

    shared
        .inner
        .lock()
        .client_mut(nested)
        .map(|client| std::mem::take(&mut client.nested));
    shared
        .execute(
            nested,
            ClientKind::Control,
            &mut nested_context,
            &CommandInvocation::new("new-session", ["-A", "-s", "outer"]),
        )
        .expect("new-session attach without nested capability is allowed");
    assert_eq!(
        client_attached_session(&shared.inner.lock(), nested),
        nested_context.session
    );
    shared.detach_client_state(nested, false);
    shared
        .execute(
            nested,
            ClientKind::Control,
            &mut nested_context,
            &CommandInvocation::new("attach-session", ["-t", "outer"]),
        )
        .expect("attach without nested capability is allowed");
    assert!(shared.read_client(nested, |c| c.is_some_and(|c| c.tty.is_some())));

    let interactive_mailbox = OutboundMailbox::new();
    let (interactive_nested, _) = shared.register_subscribed(
        ClientKind::Interactive,
        Some("interactive-nested".to_owned()),
        None,
        Arc::clone(&interactive_mailbox),
    );
    {
        let mut inner = shared.inner.lock();
        inner
            .client_entry(interactive_nested)
            .tty
            .replace(terminal.tty().unwrap().to_string_lossy().into_owned());
        inner.client_entry(interactive_nested).nested = true;
    }
    assert!(matches!(
        shared.execute(
            interactive_nested,
            ClientKind::Interactive,
            &mut ExecutionContext::default(),
            &CommandInvocation::new("new-session", ["-s", "interactive-inner"]),
        ),
        Err(DaemonError::Server(ServerError::InvalidCommand(message)))
            if message == "sessions should be nested with care, unset $TMUX to force"
    ));
    assert!(
        shared
            .inner
            .lock()
            .engine
            .state
            .sessions
            .values()
            .all(|session| session.name != "interactive-inner")
    );
    shared.unregister(interactive_nested);
    shared.unregister(nested);
    {
        let inner = shared.inner.lock();
        assert!(!inner.client(nested).is_some_and(|c| c.nested));
        assert!(inner.client(nested).is_none_or(|c| c.tty.is_none()));
    }
}
