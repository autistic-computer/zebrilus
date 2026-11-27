use super::test_support::{engine_request, execute, request};
use super::*;
use zz_mux::PaneKind;
use zz_protocol::Axis;

#[test]
fn status_context_uses_the_attached_format_client() {
    let mut engine = MuxEngine::default();
    let mut execution = zz_mux::ExecutionContext::default();
    execute(
        &mut engine,
        &mut execution,
        &["new-session", "-s", "status-active"],
    );
    let session = execution.session.expect("session id");
    let snapshot = engine.state.snapshot();

    let attached = status_context(&snapshot, &engine, Some(session), None);
    assert_eq!(attached.session_active, Some(true));

    let detached = status_context(&snapshot, &engine, None, None);
    assert_eq!(detached.session_active, None);
}

#[test]
fn status_option_snapshot_retargets_every_loop_after_the_engine_is_released() {
    let mut engine = MuxEngine::default();
    let (attached, first_window, first_pane) = engine.state.create_session("attached").unwrap();
    let second_pane = engine
        .state
        .split_pane(first_pane, Axis::Horizontal, PaneKind::Terminal)
        .unwrap();
    let (second_window, _) = engine
        .state
        .create_window_at(
            attached,
            None,
            Some("second".to_owned()),
            PaneKind::Terminal,
            false,
        )
        .unwrap();
    let (other, _, _) = engine.state.create_session("other").unwrap();
    let mut context =
        zz_mux::ExecutionContext::new(Some(attached), Some(first_window), Some(first_pane));
    let second_window = second_window.to_string();
    let first_pane = first_pane.to_string();

    execute(
        &mut engine,
        &mut context,
        &["set-option", "-g", "mouse", "off"],
    );
    execute(&mut engine, &mut context, &["set-option", "mouse", "on"]);
    execute(
        &mut engine,
        &mut context,
        &["set-window-option", "-g", "automatic-rename", "off"],
    );
    execute(
        &mut engine,
        &mut context,
        &[
            "set-window-option",
            "-t",
            &second_window,
            "automatic-rename",
            "on",
        ],
    );
    execute(
        &mut engine,
        &mut context,
        &["set-option", "-gp", "allow-set-title", "on"],
    );
    execute(
        &mut engine,
        &mut context,
        &[
            "set-option",
            "-p",
            "-t",
            &first_pane,
            "allow-set-title",
            "off",
        ],
    );

    execute(
        &mut engine,
        &mut context,
        &[
            "set-option",
            "status-left",
            "OPTCHAIN:#{mouse}:#{S:#{mouse}}:#{W:#{automatic-rename}}:#{P:#{allow-set-title}}",
        ],
    );
    execute(
        &mut engine,
        &mut context,
        &["set-option", "status-left-length", "100"],
    );
    let request = engine_request(1, &engine, Some(attached));
    assert!(
        request
            .option_snapshot
            .sessions
            .contains_key(&other.to_string())
    );
    assert!(request.option_snapshot.windows.contains_key(&second_window));
    assert!(
        request
            .option_snapshot
            .panes
            .contains_key(&second_pane.to_string())
    );
    drop(engine);

    let status = StatusRenderer::default().render_initial(&request);
    assert!(
        status.left.contains("OPTCHAIN:1:10:01:01"),
        "{}",
        status.left
    );
}

#[test]
fn the_status_line_publishes_the_theme_the_way_the_pin_resolves_it_per_client() {
    let mut engine = MuxEngine::default();
    let mut context = zz_mux::ExecutionContext::default();
    execute(
        &mut engine,
        &mut context,
        &["new-session", "-s", "alpha", "-n", "main"],
    );
    let session = context.session;
    let mut renderer = StatusRenderer::default();

    let stock = renderer.render_initial(&engine_request(1, &engine, session));
    assert_eq!(
        stock.theme.slot(0),
        Some(TmuxColour::Basic(0)),
        "themeblack"
    );
    assert_eq!(
        stock.theme.slot(4),
        Some(TmuxColour::Basic(2)),
        "themegreen"
    );
    assert!(!stock.theme.is_circular());

    execute(
        &mut engine,
        &mut context,
        &["set-option", "-s", "dark-theme-green", "colour124"],
    );
    let indexed = renderer.render_initial(&engine_request(1, &engine, session));
    assert_eq!(indexed.theme.slot(4), Some(TmuxColour::Indexed(124)));

    execute(
        &mut engine,
        &mut context,
        &["set-option", "-s", "light-theme-green", "colour99"],
    );
    execute(
        &mut engine,
        &mut context,
        &["set-option", "-s", "theme", "light"],
    );
    let light = renderer.render_initial(&engine_request(1, &engine, session));
    assert_eq!(light.theme.slot(4), Some(TmuxColour::Indexed(99)));

    execute(
        &mut engine,
        &mut context,
        &["set-option", "-s", "theme", "terminal"],
    );
    let terminal = renderer.render_initial(&engine_request(1, &engine, session));
    assert_eq!(
        terminal.theme,
        zz_protocol::ThemeColours([
            TmuxColour::Basic(0),
            TmuxColour::Basic(7),
            TmuxColour::Basic(7),
            TmuxColour::Basic(0),
            TmuxColour::Basic(2),
            TmuxColour::Basic(3),
            TmuxColour::Basic(1),
            TmuxColour::Basic(4),
            TmuxColour::Basic(6),
            TmuxColour::Basic(5),
        ])
    );

    execute(
        &mut engine,
        &mut context,
        &["set-option", "-s", "theme", "dark"],
    );
    execute(
        &mut engine,
        &mut context,
        &["set-option", "-s", "dark-theme-red", "themered"],
    );
    let circular = renderer.render_initial(&engine_request(1, &engine, session));
    assert_eq!(circular.theme.slot(6), Some(TmuxColour::Basic(8)));
    assert!(!circular.theme.is_circular());
    assert_eq!(circular.validate(), Ok(()));
}

#[test]
fn set_titles_expands_per_client_and_survives_status_off() {
    let mut engine = MuxEngine::default();
    let mut context = zz_mux::ExecutionContext::default();
    execute(
        &mut engine,
        &mut context,
        &["new-session", "-s", "alpha", "-n", "main"],
    );
    let session = context.session;
    let mut renderer = StatusRenderer::default();

    let baseline = renderer.render_initial(&engine_request(1, &engine, session));
    assert_eq!(baseline.title, "");

    execute(
        &mut engine,
        &mut context,
        &["set-option", "-g", "set-titles", "on"],
    );
    let titled = renderer.render_initial(&engine_request(1, &engine, session));
    assert!(
        titled.title.starts_with("alpha:0:main"),
        "default set-titles-string expands in client context: {}",
        titled.title
    );

    execute(
        &mut engine,
        &mut context,
        &["set-option", "-g", "set-titles-string", "#S custom"],
    );
    let custom = renderer.render_initial(&engine_request(1, &engine, session));
    assert_eq!(custom.title, "alpha custom");

    execute(&mut engine, &mut context, &["set-option", "status", "off"]);
    let off = renderer.render_initial(&engine_request(1, &engine, session));
    assert!(off.rows.is_empty());
    assert_eq!(off.title, "alpha custom");

    execute(
        &mut engine,
        &mut context,
        &["set-option", "-g", "set-titles", "off"],
    );
    let untitled = renderer.render_initial(&engine_request(1, &engine, session));
    assert_eq!(untitled.title, "");
}

#[test]
fn personalized_rows_expand_per_session_and_dedupe() {
    let mut engine = MuxEngine::default();
    let mut context = zz_mux::ExecutionContext::default();
    execute(
        &mut engine,
        &mut context,
        &["new-session", "-s", "alpha", "-n", "main"],
    );
    let alpha = context.session.expect("alpha session");
    execute(
        &mut engine,
        &mut context,
        &["new-session", "-d", "-s", "beta", "-n", "logs"],
    );
    let beta = engine
        .state
        .sessions
        .iter()
        .find(|(_, session)| session.name == "beta")
        .map(|(id, _)| *id)
        .expect("beta session");
    execute(
        &mut engine,
        &mut context,
        &["set-option", "-g", "status-right", "static"],
    );

    let requests = [
        engine_request(1, &engine, Some(alpha)),
        engine_request(2, &engine, Some(beta)),
    ];
    let mut renderer = StatusRenderer::default();
    let first = renderer.render_changed(&requests);
    assert_eq!(first.len(), 2);
    let alpha_status = &first[0].1;
    let beta_status = &first[1].1;
    assert_eq!(alpha_status.rows.len(), 1);
    assert!(
        alpha_status.rows[0].contains("[alpha]"),
        "row 0 carries status-left: {}",
        alpha_status.rows[0]
    );
    assert!(
        alpha_status.rows[0].contains("0:main"),
        "row 0 carries the window list: {}",
        alpha_status.rows[0]
    );
    assert!(
        alpha_status.rows[0].contains("static"),
        "row 0 carries status-right: {}",
        alpha_status.rows[0]
    );
    assert!(beta_status.rows[0].contains("[beta]"));
    assert!(beta_status.rows[0].contains("0:logs"));
    assert_ne!(alpha_status.rows, beta_status.rows);
    assert_eq!(alpha_status.validate(), Ok(()));
    assert_eq!(beta_status.validate(), Ok(()));
    assert!(renderer.render_changed(&requests).is_empty());
}

#[test]
fn status_row_counts_follow_the_status_option() {
    let mut engine = MuxEngine::default();
    let mut context = zz_mux::ExecutionContext::default();
    execute(
        &mut engine,
        &mut context,
        &["new-session", "-s", "work", "-n", "main"],
    );
    let session = context.session.expect("session id");
    execute(
        &mut engine,
        &mut context,
        &["set-option", "-g", "status", "off"],
    );
    let off = StatusRenderer::default().render_initial(&engine_request(1, &engine, Some(session)));
    assert!(off.rows.is_empty());
    assert_eq!(off.message_line, 0);
    assert!(off.customized);
    assert!(off.is_empty());

    execute(
        &mut engine,
        &mut context,
        &["set-option", "-g", "status", "5"],
    );
    execute(
        &mut engine,
        &mut context,
        &["set-option", "-g", "message-line", "4"],
    );
    let five = StatusRenderer::default().render_initial(&engine_request(1, &engine, Some(session)));
    assert_eq!(five.rows.len(), 5);
    assert!(five.rows[0].contains("[work]"));
    assert!(five.rows[1].starts_with("#[align=left]    P: "));
    assert!(five.rows[2].starts_with("#[align=left]    S: "));
    assert!(!five.rows[1].contains("#{R:"));
    assert!(!five.rows[2].contains("#{R:"));
    assert_eq!(five.rows[3], "");
    assert_eq!(five.rows[4], "");
    assert_eq!(five.message_line, 4);
    assert_eq!(five.validate(), Ok(()));

    execute(
        &mut engine,
        &mut context,
        &["set-option", "-g", "status", "2"],
    );
    let two = StatusRenderer::default().render_initial(&engine_request(1, &engine, Some(session)));
    assert_eq!(two.rows.len(), 2);
    assert_eq!(
        two.message_line, 1,
        "message-line clamps below the row count"
    );
    assert_eq!(two.validate(), Ok(()));
}

#[test]
fn per_window_status_overrides_style_the_rendered_row() {
    let mut engine = MuxEngine::default();
    let mut context = zz_mux::ExecutionContext::default();
    execute(
        &mut engine,
        &mut context,
        &["new-session", "-s", "work", "-n", "main"],
    );
    execute(
        &mut engine,
        &mut context,
        &["new-window", "-d", "-n", "other"],
    );
    let session = context.session.expect("session id");
    execute(
        &mut engine,
        &mut context,
        &[
            "set-window-option",
            "-t",
            "work:0",
            "window-status-current-format",
            "OVERRIDE",
        ],
    );
    let status =
        StatusRenderer::default().render_initial(&engine_request(1, &engine, Some(session)));
    let row = status.rows.first().expect("status row");
    assert!(row.contains("OVERRIDE"), "row: {row}");
    assert!(
        !row.contains("0:main"),
        "the current window's default label must be replaced: {row}"
    );
    assert!(
        row.contains("1:other"),
        "windows without overrides keep the global format: {row}"
    );
}

#[test]
fn window_status_separator_uses_each_item_scope_and_skips_the_trailing_item() {
    let mut engine = MuxEngine::default();
    let mut context = zz_mux::ExecutionContext::default();
    execute(
        &mut engine,
        &mut context,
        &["new-session", "-s", "work", "-n", "main"],
    );
    execute(
        &mut engine,
        &mut context,
        &["new-window", "-d", "-n", "logs"],
    );
    let session = context.session.expect("session id");
    for args in [
        &["set-option", "-g", "status-left", ""] as &[&str],
        &["set-option", "-g", "status-right", ""],
        &[
            "set-option",
            "-g",
            "status-format[0]",
            "#{W:#{T:window-status-format}#{?loop_last_flag,,#{E:window-status-separator}},#{T:window-status-current-format}#{?loop_last_flag,,#{E:window-status-separator}}}",
        ],
        &[
            "set-window-option",
            "-g",
            "window-status-format",
            "N#{window_index}:#{window_name}",
        ],
        &[
            "set-window-option",
            "-g",
            "window-status-current-format",
            "C#{window_index}:#{window_name}",
        ],
        &[
            "set-window-option",
            "-g",
            "window-status-separator",
            "#{?#{==:#{window_index},0},#[fg=red]G#{window_index}:#{window_name},#[fg=blue]BAD}",
        ],
    ] {
        execute(&mut engine, &mut context, args);
    }

    let global =
        StatusRenderer::default().render_initial(&engine_request(1, &engine, Some(session)));
    assert_eq!(global.left, "");
    assert_eq!(global.right, "");
    assert_eq!(global.rows, ["C0:main#[fg=red]G0:mainN1:logs"]);

    execute(
        &mut engine,
        &mut context,
        &[
            "set-window-option",
            "-t",
            "work:0",
            "window-status-separator",
            "#{?#{==:#{window_index},0},#[bold]L#{window_index}:#{window_name},#[reverse]BAD}",
        ],
    );
    let local =
        StatusRenderer::default().render_initial(&engine_request(2, &engine, Some(session)));
    assert_eq!(local.rows, ["C0:main#[bold]L0:mainN1:logs"]);
}

#[test]
fn sparse_session_status_formats_keep_blank_rows_without_compaction() {
    let mut engine = MuxEngine::default();
    let mut context = zz_mux::ExecutionContext::default();
    execute(&mut engine, &mut context, &["new-session", "-s", "work"]);
    let session = context.session.expect("session id");
    execute(&mut engine, &mut context, &["set-option", "status", "5"]);
    execute(
        &mut engine,
        &mut context,
        &["set-option", "status-format[3]", "MARK #S"],
    );
    let status =
        StatusRenderer::default().render_initial(&engine_request(1, &engine, Some(session)));
    assert_eq!(status.rows, ["", "", "", "MARK work", ""]);
    assert!(status.customized);
    assert_eq!(status.validate(), Ok(()));
}

#[test]
fn base_style_applies_status_style_then_fg_bg_overrides() {
    let mut renderer = StatusRenderer::default();
    let mut styled = request(1, "", "");
    Arc::make_mut(&mut styled.formats).style = "bg=blue,fg=white".to_owned();
    Arc::make_mut(&mut styled.formats).foreground = "red".to_owned();
    let status = renderer.render_initial(&styled);
    assert_eq!(status.base_style, "bg=blue,fg=white,fg=red");
    assert_eq!(status.validate(), Ok(()));

    let mut dynamic = request(2, "", "");
    Arc::make_mut(&mut dynamic.formats).style = "fg=#{?window_zoomed,red,green}".to_owned();
    Arc::make_mut(&mut dynamic.formats).background = "black".to_owned();
    let status = renderer.render_initial(&dynamic);
    assert_eq!(status.base_style, "fg=green,bg=black");
    assert_eq!(status.validate(), Ok(()));
}

#[test]
fn an_unparseable_expanded_status_style_degrades_instead_of_dropping_the_event() {
    let mut renderer = StatusRenderer::default();
    let mut broken = request(1, "LEFT", "RIGHT");
    Arc::make_mut(&mut broken.formats).style = "bg=#{@theme_bg}".to_owned();
    let status = renderer.render_initial(&broken);
    assert_eq!(status.base_style, "");
    assert_eq!(status.rows.len(), 1);
    assert!(status.left.ends_with("LEFT"));
    assert!(status.right.ends_with("RIGHT"));
    assert_eq!(
        status.validate(),
        Ok(()),
        "the event must survive the encode seam"
    );

    let mut overridden = request(2, "", "");
    Arc::make_mut(&mut overridden.formats).style = "bg=#{@theme_bg}".to_owned();
    Arc::make_mut(&mut overridden.formats).foreground = "red".to_owned();
    let status = renderer.render_initial(&overridden);
    assert_eq!(status.base_style, "fg=red");
    assert_eq!(status.validate(), Ok(()));
}

#[test]
fn message_line_clamps_against_the_published_row_count() {
    let mut renderer = StatusRenderer::default();
    let mut clamped = request(1, "", "");
    Arc::make_mut(&mut clamped.formats).lines = 2;
    clamped.message_line = 4;
    let status = renderer.render_initial(&clamped);
    assert_eq!(status.rows.len(), 2);
    assert_eq!(status.message_line, 1);

    let mut disabled = request(2, "", "");
    Arc::make_mut(&mut disabled.formats).enabled = false;
    disabled.message_line = 3;
    assert_eq!(renderer.render_initial(&disabled).message_line, 0);
}

#[test]
fn only_changed_clients_are_published() {
    let mut renderer = StatusRenderer::default();
    let requests = [request(1, "[#S]", ""), request(2, "[#S]", "")];

    let first = renderer.render_changed(&requests);
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].1.left, "[work]");
    assert!(renderer.render_changed(&requests).is_empty());

    let renamed = [request(1, "[#S]", ""), {
        let mut request = request(2, "[#S]", "");
        Arc::make_mut(&mut request.context).session_name = "infra".to_owned();
        request
    }];
    let second = renderer.render_changed(&renamed);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].0, ClientId(2));
    assert_eq!(second[0].1.left, "[infra]");
}

#[test]
fn status_sides_left_trim_display_width_without_counting_styles() {
    assert_eq!(trim_status_left("a#[bold]界bc", 4), "a#[bold]界b");
    assert_eq!(trim_status_left("##[bold]界", 3), "##[b");
}

#[test]
fn status_sides_carry_base_then_side_styles() {
    let mut request = request(1, "abcdef", "uvwxyz");
    Arc::make_mut(&mut request.formats).style = "bg=blue,fg=white".to_owned();
    Arc::make_mut(&mut request.formats).foreground = "red".to_owned();
    Arc::make_mut(&mut request.formats).left_style = "bold".to_owned();
    Arc::make_mut(&mut request.formats).right_style = "italics".to_owned();
    Arc::make_mut(&mut request.formats).left_length = 4;
    Arc::make_mut(&mut request.formats).right_length = 3;
    let status = StatusRenderer::default().render_initial(&request);
    assert_eq!(
        status.left,
        "#[bg=blue,fg=white,fg=red]#[bold]#[push-default]abcd"
    );
    assert_eq!(
        status.right,
        "#[bg=blue,fg=white,fg=red]#[italics]#[push-default]uvw"
    );
}

#[test]
fn status_style_wrapping_stays_inside_the_wire_limit() {
    let mut request = request(1, &"x".repeat(MAX_STATUS_TEXT_BYTES), "");
    Arc::make_mut(&mut request.formats).style = "bold,".repeat(800);
    Arc::make_mut(&mut request.formats).left_style = "italics,".repeat(800);
    let status = StatusRenderer::default().render_initial(&request);
    assert_eq!(status.validate(), Ok(()));
    assert!(status.left.is_char_boundary(status.left.len()));
}

#[test]
fn oversized_row_title_and_base_style_stay_inside_the_wire_limit() {
    let overflow = "#{R:x,9000}";
    let mut request = request(1, overflow, overflow);
    Arc::make_mut(&mut request.formats).lines = 2;
    Arc::make_mut(&mut request.formats).style = format!("bold,{}", "italics,".repeat(900));
    *Arc::make_mut(&mut request.title_format) = Some(overflow.to_owned());
    *Arc::make_mut(&mut request.row_formats) =
        BTreeMap::from([(0, overflow.to_owned()), (1, overflow.to_owned())]);
    let status = StatusRenderer::default().render_initial(&request);
    assert_eq!(status.validate(), Ok(()));
    assert_eq!(status.title.len(), MAX_STATUS_TEXT_BYTES);
    assert_eq!(status.rows.len(), 2);
    for row in &status.rows {
        assert_eq!(row.len(), MAX_STATUS_TEXT_BYTES);
    }

    let unstyled = self::request(1, overflow, "");
    let status = StatusRenderer::default().render_initial(&unstyled);
    assert_eq!(status.validate(), Ok(()));
    assert!(status.left.len() <= MAX_STATUS_TEXT_BYTES);

    let mut markers = self::request(1, "#{R:#[bold],700}x", "");
    Arc::make_mut(&mut markers.formats).left_length = 10;
    let status = StatusRenderer::default().render_initial(&markers);
    assert_eq!(status.validate(), Ok(()));
    assert!(status.left.len() <= MAX_STATUS_TEXT_BYTES);
}

#[test]
fn a_disabled_status_renders_empty() {
    let mut renderer = StatusRenderer::default();
    let mut request = request(1, "[#S]", "%H");
    Arc::make_mut(&mut request.formats).enabled = false;
    let status = renderer.render_initial(&request);
    assert!(status.is_empty());
}
