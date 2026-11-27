use super::test_support::request;
use super::*;
use crate::daemon::status_jobs::tests::renderer;
use std::time::{Duration, Instant};
use zz_mux::{PaneKind, SplitSize, StatusValues, expand_format_values};
use zz_protocol::Axis;
use zz_terminal::{SessionStatus, TerminalViewId};

#[test]
fn browser_url_status_uses_active_tab_and_is_empty_for_terminal() {
    let mut engine = MuxEngine::default();
    let (_, _, terminal) = engine.state.create_session("urls").unwrap();
    let browser = engine
        .state
        .split_pane(
            terminal,
            Axis::Horizontal,
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
    let mut request = request(1, "url=#{browser_url}", "");
    Arc::make_mut(&mut request.facts).mux = Arc::new(engine.format_facts());
    let mut renderer = StatusRenderer::default();
    Arc::make_mut(&mut request.context).pane_id = browser.to_string();
    assert_eq!(
        renderer.render_initial(&request).left,
        "url=https://example.com/active"
    );
    Arc::make_mut(&mut request.context).pane_id = terminal.to_string();
    assert_eq!(renderer.render_initial(&request).left, "url=");
}

#[test]
fn last_command_status_format_reports_codes_and_claims_unknown_values() {
    let pane = PaneId(9);
    let terminal = Arc::new(TerminalSession::spawn_empty_with_appearance(
        64,
        Arc::new(zz_terminal::TerminalAppearance::default()),
    ));
    let facts = FormatHookFacts {
        terminals: Arc::new(BTreeMap::from([(pane, Arc::clone(&terminal))])),
        ..FormatHookFacts::default()
    };
    let mut hooks = DaemonFormatHooks::command(&facts);
    let context = StatusContext::from(StatusValues {
        pane_id: pane.to_string(),
        ..StatusValues::default()
    });
    for (bytes, expected) in [
        (b"".as_slice(), ""),
        (b"\x1b]133;D;7\x07".as_slice(), "7"),
        (b"\x1b]133;A\x07".as_slice(), "7"),
        (b"\x1b]133;D;0\x07".as_slice(), "0"),
        (b"\x1b]133;D\x07".as_slice(), ""),
    ] {
        assert!(terminal.feed(Arc::from(bytes)));
        terminal
            .capture(zz_terminal::CaptureOptions::default())
            .expect("processed output");
        assert_eq!(
            hooks.variable("pane_last_command_status", &context),
            Some(expected.to_owned())
        );
        assert_eq!(
            zz_mux::expand_format_values("#{pane_last_command_status}", &context, &mut hooks),
            expected
        );
    }
    for pane_id in ["%99", ""] {
        let context = StatusContext::from(StatusValues {
            pane_id: pane_id.to_owned(),
            ..StatusValues::default()
        });
        assert_eq!(
            hooks.variable("pane_last_command_status", &context),
            Some(String::new())
        );
    }
}

#[test]
fn daemon_delegated_format_consumers_match_mux_inventory() {
    let delegated = zz_mux::delegated_format_variable_names().collect::<Vec<_>>();
    assert_eq!(delegated.len(), 66);

    let session = SessionId(1);
    let pane = PaneId(1);
    let facts = FormatHookFacts {
        session_last_attached: Arc::new(BTreeMap::from([(session, 1)])),
        pane_pipes: Arc::new(BTreeMap::from([(pane, 4242)])),
        buffer: Some(BufferFormatFacts {
            name: String::new(),
            data: Arc::from([]),
            created: UNIX_EPOCH,
        }),
        client: Some(ClientFormatFacts {
            last_session: "last".to_owned(),
            theme: "dark".to_owned(),
            viewport: Some(ClientViewportFacts {
                columns: 80,
                rows: 23,
                window_width: 200,
                window_height: 60,
                cursor: None,
            }),
            ..ClientFormatFacts::default()
        }),
        copy_modes: Arc::new(BTreeMap::from([(
            pane,
            vec![(String::new(), Arc::new(CopyModeFacts::default()))],
        )])),
        ..FormatHookFacts::default()
    };
    let context = StatusContext::from(StatusValues {
        session_id: session.to_string(),
        window_id: "@1".to_owned(),
        pane_id: pane.to_string(),
        ..StatusValues::default()
    });
    let mut hooks = DaemonFormatHooks::command(&facts);
    for name in delegated {
        assert!(
            hooks.variable(name, &context).is_some() || PANE_OUTPUT_FORMATS.contains(&name),
            "daemon format hook does not consume {name}"
        );
    }
    let recorded = TerminalFacts {
        output: zz_terminal::PaneOutputFacts {
            output_generation: 3,
            last_output_time: 100,
            last_prompt_time: 90,
            command_start_time: 95,
            command_end_time: 99,
            command_running: false,
            command_status: Some(2),
        },
        private_modes: 0b1_0100,
        ..TerminalFacts::default()
    };
    for name in PANE_OUTPUT_FORMATS {
        assert!(
            pane_output_variable(name, &recorded, 200).is_some(),
            "{name}"
        );
    }
    assert_eq!(
        pane_output_variable("pane_private_modes", &recorded, 200).as_deref(),
        Some("7,25")
    );
    assert_eq!(
        pane_output_variable("pane_command_duration", &recorded, 200).as_deref(),
        Some("4")
    );
    let running = TerminalFacts {
        output: zz_terminal::PaneOutputFacts {
            command_running: true,
            command_end_time: 0,
            command_status: None,
            ..recorded.output
        },
        ..recorded
    };
    assert_eq!(
        pane_output_variable("pane_command_duration", &running, 200).as_deref(),
        Some("105")
    );
    assert_eq!(
        pane_output_variable("pane_command_status", &running, 200),
        None
    );
    assert_eq!(
        pane_output_variable("pane_last_prompt_time", &TerminalFacts::default(), 200),
        None
    );
}

#[test]
fn copy_mode_toggle_and_selection_formats_answer_from_the_copy_view() {
    let pane = PaneId(1);
    let context = StatusContext::from(StatusValues {
        pane_id: pane.to_string(),
        ..StatusValues::default()
    });
    let answer = |facts: &FormatHookFacts| {
        zz_mux::expand_format_values(
            "#{selection_active}:#{rectangle_toggle}",
            &context,
            &mut DaemonFormatHooks::command(facts),
        )
    };
    let in_mode = FormatHookFacts {
        copy_modes: Arc::new(BTreeMap::from([(
            pane,
            vec![(
                String::new(),
                Arc::new(CopyModeFacts {
                    rectangle_toggle: true,
                    selection_active: true,
                    ..CopyModeFacts::default()
                }),
            )],
        )])),
        ..FormatHookFacts::default()
    };
    assert_eq!(answer(&in_mode), "1:1");
    let entered = FormatHookFacts {
        copy_modes: Arc::new(BTreeMap::from([(
            pane,
            vec![(String::new(), Arc::new(CopyModeFacts::default()))],
        )])),
        ..FormatHookFacts::default()
    };
    assert_eq!(answer(&entered), "0:0");
    assert_eq!(answer(&FormatHookFacts::default()), ":");
}

#[test]
fn status_pane_index_uses_effective_pane_base_and_pane_order() {
    let mut engine = MuxEngine::default();
    let (session, window, target) = engine.state.create_session("work").unwrap();
    let source = engine
        .state
        .split_pane(target, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    engine
        .state
        .join_pane(
            source,
            target,
            Axis::Vertical,
            SplitSize::Default,
            true,
            false,
            false,
        )
        .unwrap();
    engine
        .execute(
            &mut zz_mux::ExecutionContext::default(),
            &zz_protocol::CommandInvocation::new(
                "set-window-option",
                ["-g", "pane-base-index", "1"],
            ),
        )
        .unwrap();
    let snapshot = engine.state.snapshot();
    let mut layout_order = Vec::new();
    snapshot.sessions[0].windows[0]
        .layout
        .panes(&mut layout_order);

    assert_eq!(layout_order, [source, target]);
    assert_eq!(engine.state.windows[&window].pane_order(), [target, source]);
    let context = status_context(&snapshot, &engine, Some(session), Some(window));
    assert_eq!(context.pane_index, 2);
    assert_eq!(
        context.window_layout,
        engine.state.windows[&window].layout_string(
            zz_mux::LayoutFormat::V2,
            engine.state.pane_base_index(window)
        )
    );
}

#[test]
fn content_search_reads_visible_terminal_rows() {
    let pane = PaneId(9);
    let terminal = Arc::new(TerminalSession::spawn_output_view(
        "search".to_owned(),
        "alpha\nbravo\ncharlie".to_owned(),
    ));
    terminal.attach_view(TerminalViewId(1));
    let deadline = Instant::now() + Duration::from_secs(2);
    while !matches!(terminal.latest_viewport().status, SessionStatus::Running) {
        assert!(
            Instant::now() < deadline,
            "output view did not become ready"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let facts = FormatHookFacts {
        terminals: Arc::new(BTreeMap::from([(pane, terminal)])),
        buffer: None,
        ..FormatHookFacts::default()
    };
    let context = StatusContext::from(StatusValues {
        pane_id: pane.to_string(),
        ..StatusValues::default()
    });
    let mut hooks = DaemonFormatHooks::command(&facts);
    assert_eq!(
        expand_format_values(
            "#{C:bravo}|#{C/i:BRAVO}|#{C/r:^charlie$}|#{C:missing}",
            &context,
            &mut hooks,
        ),
        "2|2|3|0"
    );
}

#[test]
fn window_cell_metrics_default_to_the_pin_size_and_follow_a_reporting_client() {
    let format = "#{window_cell_width}|#{window_cell_height}";
    let window = StatusContext::from(StatusValues {
        window_id: "@1".to_owned(),
        ..StatusValues::default()
    });

    let bare = FormatHookFacts::default();
    let mut hooks = DaemonFormatHooks::command(&bare);
    assert_eq!(expand_format_values(format, &window, &mut hooks), "16|32");

    let unsized_client = FormatHookFacts {
        client: Some(ClientFormatFacts {
            cell_width: "0".to_owned(),
            cell_height: String::new(),
            ..ClientFormatFacts::default()
        }),
        ..FormatHookFacts::default()
    };
    let mut hooks = DaemonFormatHooks::command(&unsized_client);
    assert_eq!(expand_format_values(format, &window, &mut hooks), "16|32");

    let reporting_client = FormatHookFacts {
        client: Some(ClientFormatFacts {
            cell_width: "9".to_owned(),
            cell_height: "18".to_owned(),
            ..ClientFormatFacts::default()
        }),
        ..FormatHookFacts::default()
    };
    let mut hooks = DaemonFormatHooks::command(&reporting_client);
    assert_eq!(expand_format_values(format, &window, &mut hooks), "9|18");

    let mut hooks = DaemonFormatHooks::command(&reporting_client);
    assert_eq!(
        expand_format_values(format, &StatusContext::default(), &mut hooks),
        "|"
    );
}

#[test]
fn buffer_samples_preserve_utf8_and_escape_control_and_invalid_bytes() {
    assert_eq!(buffer_sample("α\n\t\\".as_bytes()), "α\\n\\t\\\\");
    assert_eq!(buffer_sample(&[0xff]), "\\377");
    assert_eq!(buffer_sample(&[0, b'x', 0, b'7']), "\\0x\\0007");
    assert_eq!(buffer_full(b"a\0b"), "a\0b");
    assert_eq!(
        buffer_sample(&vec![b'x'; 201]),
        format!("{}...", "x".repeat(200))
    );
}

#[test]
fn strftime_and_shell_failures_degrade_to_text() {
    let mut renderer = renderer();
    let status = renderer.render_initial(&request(1, "%H:%M", "#(exit 3)"));
    assert_eq!(status.left.len(), 5, "a clock renders as HH:MM");
    assert!(
        status.right.is_empty(),
        "a failing command renders as blank"
    );
}
