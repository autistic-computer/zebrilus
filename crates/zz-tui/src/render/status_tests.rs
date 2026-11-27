use super::test_support::{attach_one_pane, block_model, block_status};
use super::*;
use crate::state::ClientMessage;
use zz_terminal::SessionStatus;

#[test]
fn styled_lines_clip_text_without_counting_markers() {
    let line = StyledLine::parsed("#[fg=red]abcd#[bold]ef");
    assert_eq!(line.len(), 6);
    assert_eq!(line.plain_text(), "abcdef");

    let clipped = line.truncate(5);
    assert_eq!(clipped.plain_text(), "abcde");
    assert_eq!(clipped.segments.len(), 2);
    assert_eq!(clipped.segments[0].style.fg, Some(TmuxColour::Basic(1)));
    assert_eq!(
        clipped.segments[1].style.attributes.bold,
        TmuxAttributeState::On
    );
}

#[test]
fn plain_text_clipping_uses_terminal_display_cells() {
    assert_eq!(truncate("界ab", 3), "界a");
    assert_eq!(truncate("界ab", 1), "");
    assert_eq!(truncate("e\u{301}界", 1), "e\u{301}");

    let padded = padded_segment("界e\u{301}", 5, ' ');
    assert_eq!(padded, "界e\u{301}  ");
    assert_eq!(text_display_width(&padded), 5);

    let wide_fill = padded_segment("a", 4, '界');
    assert_eq!(text_display_width(&wide_fill), 4);
    assert_eq!(wide_fill, "a界 ");
}

#[test]
fn style_only_changes_invalidate_cached_lines() {
    assert_ne!(
        StyledLine::parsed("#[fg=red]same"),
        StyledLine::parsed("#[fg=blue]same")
    );
}

#[test]
fn window_status_labels_use_daemon_text_and_keep_empty_formats_empty() {
    let mut line = StyledLine::plain("[session]");
    append_status_label(&mut line, "#[bold]CUSTOM");
    append_status_label(&mut line, "#[default]");

    assert_eq!(line.plain_text(), "[session] CUSTOM");
    assert_eq!(line.segments.len(), 2);
    assert_eq!(
        line.segments[1].style.attributes.bold,
        TmuxAttributeState::On
    );
}

#[test]
fn status_block_paints_daemon_rows_and_blank_rows_carry_base_style() {
    let mut model = block_model(40, 10);
    let mut status = block_status(vec!["#[fg=red,bold]HOT", ""], true);
    status.base_style = "bg=blue".to_owned();
    model.set_status(status);
    let mut renderer = Renderer::new();
    renderer.paint_status_block(&model, true);
    let output = String::from_utf8(renderer.output).unwrap();

    assert!(output.contains("\x1b[9;1H"), "row 8 paints: {output:?}");
    assert!(
        output.contains("\x1b[10;1H"),
        "blank row 9 paints: {output:?}"
    );
    assert!(output.contains("HOT"));
    assert!(output.contains("\x1b[1m"));
    assert!(output.contains("\x1b[44m"), "{output:?}");
    assert!(output.contains("\x1b[31m"), "{output:?}");
    assert!(!output.contains("#["));
}

#[test]
fn status_position_top_paints_the_block_at_row_zero() {
    let mut model = block_model(40, 10);
    let mut status = block_status(vec!["TOPROW"], true);
    status.position = zz_protocol::StatusPosition::Top;
    model.set_status(status);
    let mut renderer = Renderer::new();
    renderer.paint_status_block(&model, true);
    let output = String::from_utf8(renderer.output).unwrap();

    assert!(output.contains("\x1b[1;1H"), "{output:?}");
    assert!(output.contains("TOPROW"));
}

#[test]
fn a_client_message_replaces_the_message_line_row() {
    let mut model = block_model(40, 10);
    let mut status = block_status(vec!["ROWZERO", "ROWONE"], true);
    status.message_line = 1;
    model.set_status(status);
    model.client_message = Some(ClientMessage::local("hello message"));
    let mut renderer = Renderer::new();
    renderer.paint_status_block(&model, true);
    let output = String::from_utf8(renderer.output).unwrap();

    assert!(output.contains("hello message"));
    assert!(output.contains("ROWZERO"));
    assert!(!output.contains("ROWONE"), "{output:?}");
}

#[test]
fn the_detach_hint_is_sidebar_chrome_and_never_covers_the_status_row() {
    for customized in [false, true] {
        let mut model = block_model(40, 10);
        assert!(!model.sidebar_visible());
        model.set_status(block_status(
            vec!["[rows] 0:bash*   \"ubuntu\" 11:11"],
            customized,
        ));
        let mut renderer = Renderer::new();
        renderer.paint_status_block(&model, true);
        let output = String::from_utf8(renderer.output).unwrap();
        assert!(!output.contains("C-b d detach"), "{output:?}");
        assert!(
            output.contains("[rows] 0:bash*   \"ubuntu\" 11:11"),
            "{output:?}"
        );
    }

    let mut sidebar_model = block_model(120, 10);
    assert!(!sidebar_model.sidebar_visible(), "no width invokes it");
    sidebar_model.focus_sidebar();
    assert!(sidebar_model.sidebar_visible());
    sidebar_model.set_status(block_status(vec!["ROW"], false));
    let mut renderer = Renderer::new();
    renderer.paint_sidebar(&sidebar_model, true);
    renderer.paint_status_block(&sidebar_model, true);
    let output = String::from_utf8(renderer.output).unwrap();
    assert!(output.contains("C-b d detach"), "{output:?}");
    assert!(output.contains("ROW"), "{output:?}");

    sidebar_model.set_status(block_status(vec!["ROW"], true));
    let mut renderer = Renderer::new();
    renderer.paint_sidebar(&sidebar_model, true);
    renderer.paint_status_block(&sidebar_model, true);
    let output = String::from_utf8(renderer.output).unwrap();
    assert!(!output.contains("C-b d detach"), "{output:?}");
}

#[test]
fn a_message_with_status_off_paints_one_virtual_row() {
    let mut model = block_model(40, 10);
    model.set_status(zz_protocol::StatusLine {
        customized: true,
        ..zz_protocol::StatusLine::default()
    });
    model.client_message = Some(ClientMessage::local("virtual"));
    let mut renderer = Renderer::new();
    renderer.paint_status_block(&model, true);
    let output = String::from_utf8(renderer.output).unwrap();

    assert!(
        output.contains("\x1b[10;1H"),
        "bottom virtual row: {output:?}"
    );
    assert!(output.contains("virtual"));
}

#[test]
fn trailing_prompt_spaces_are_left_to_the_fill() {
    let mut model = block_model(40, 10);
    model.set_status(block_status(vec!["STATUS"], true));
    model.confirm = Some(zz_protocol::ConfirmState {
        prompt: "Go on?  ".to_owned(),
        confirm_key: b'y',
        default_yes: false,
        prompt_cursor: zz_protocol::PromptCursor::default(),
    });
    let Some(StatusOverlay { front, .. }) = status_overlay(&model, 40) else {
        panic!("confirm is a message-area overlay");
    };
    let text: String = front
        .segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect();
    assert_eq!(text, "Go on?");
}

#[test]
fn trailing_prompt_spaces_survive_with_no_status_row_under_them() {
    let mut model = block_model(40, 10);
    model.set_status(zz_protocol::StatusLine {
        customized: true,
        ..zz_protocol::StatusLine::default()
    });
    model.confirm = Some(zz_protocol::ConfirmState {
        prompt: "Go on?  ".to_owned(),
        confirm_key: b'y',
        default_yes: false,
        prompt_cursor: zz_protocol::PromptCursor::default(),
    });
    let Some(StatusOverlay { front, .. }) = status_overlay(&model, 40) else {
        panic!("confirm is a message-area overlay");
    };
    let text: String = front
        .segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect();
    assert_eq!(text, "Go on?  ");
}

#[test]
fn confirm_prompt_replaces_the_status_row_and_other_local_overlays() {
    let mut model = block_model(40, 10);
    model.set_status(zz_protocol::StatusLine {
        customized: true,
        ..zz_protocol::StatusLine::default()
    });
    model.command_output_search = Some(SearchQuery::literal("hidden search"));
    model.client_message = Some(ClientMessage::local("hidden message"));
    model.confirm = Some(zz_protocol::ConfirmState {
        prompt: "Confirm attached? (y/n) ".to_owned(),
        confirm_key: b'y',
        default_yes: false,
        prompt_cursor: zz_protocol::PromptCursor::default(),
    });
    let mut renderer = Renderer::new();
    renderer.paint_status_block(&model, true);
    let output = String::from_utf8(renderer.output).unwrap();

    assert!(output.contains("\x1b[10;1H"), "{output:?}");
    assert!(output.contains("Confirm attached? (y/n) "));
    assert!(!output.contains("hidden search"));
    assert!(!output.contains("hidden message"));

    let mut sidebar_model = block_model(120, 10);
    assert!(!sidebar_model.sidebar_visible(), "no width invokes it");
    sidebar_model.focus_sidebar();
    assert!(sidebar_model.sidebar_visible());
    sidebar_model.confirm = model.confirm;
    assert!(status_overlay(&sidebar_model, 120).is_none());
    let lines = sidebar_status_lines(&sidebar_model);
    assert_eq!(lines.len(), 3);
    assert!(lines.last().is_some_and(|line| {
        line.segments
            .iter()
            .any(|segment| segment.text.contains("Confirm attached? (y/n) "))
    }));

    let mut sidebar_renderer = Renderer::new();
    sidebar_renderer.paint_sidebar(&sidebar_model, true);
    sidebar_renderer.paint_status_block(&sidebar_model, true);
    let output = String::from_utf8(sidebar_renderer.output).unwrap();
    assert_eq!(output.matches("Confirm attached? (y/n) ").count(), 1);
}

#[test]
fn command_output_search_prompt_tracks_direction_and_status_overlay() {
    let mut model = block_model(12, 8);
    attach_one_pane(&mut model, PaneId(1));
    model.command_output = Some((
        PaneId(1),
        TerminalViewport::blank(12, 6, SessionStatus::Running),
    ));
    model.command_output_search = Some(SearchQuery {
        text: "needle".to_owned(),
        direction: SearchDirection::Backward,
        ..SearchQuery::default()
    });
    assert_eq!(
        command_output_search_prompt(model.command_output_search.as_ref().unwrap()),
        "?needle"
    );
    let Some(StatusOverlay { front, fill }) = status_overlay(&model, 12) else {
        panic!("command output search did not replace the status row");
    };
    assert_eq!(front.plain_text(), "?needle");
    let row = StyledLine::from_segments(crate::mode_view::over_underlay(
        &front.segments,
        fill,
        &[],
        12,
    ));
    assert_eq!(row.plain_text(), "?needle     ");

    model.command_output_search.as_mut().unwrap().direction = SearchDirection::Forward;
    assert_eq!(
        command_output_search_prompt(model.command_output_search.as_ref().unwrap()),
        "/needle"
    );

    let mut sidebar_model = block_model(120, 8);
    attach_one_pane(&mut sidebar_model, PaneId(1));
    sidebar_model.command_output = Some((
        PaneId(1),
        TerminalViewport::blank(120, 6, SessionStatus::Running),
    ));
    sidebar_model.focus_sidebar();
    assert!(sidebar_model.sidebar_visible());
    sidebar_model.command_output_search = Some(SearchQuery::literal("visible"));
    assert!(matches!(
        status_overlay(&sidebar_model, 120),
        Some(StatusOverlay { .. })
    ));

    let query = SearchQuery::literal("界e\u{301}界");
    let (painted, cursor) = command_output_search_display(&query, 6);
    assert_eq!(painted, "/界e\u{301}  ");
    assert_eq!(text_display_width(&painted), 6);
    assert_eq!(cursor, 4);
}

#[test]
fn the_status_row_resolves_theme_colours_through_the_published_table_and_keeps_the_class() {
    let mut model = block_model(40, 10);
    let mut status = block_status(vec!["#[fg=themegreen,bg=themeblack]ROW"], true);
    status.theme = zz_protocol::ThemeColours([
        TmuxColour::Indexed(124),
        TmuxColour::Rgb(0x0011_2233),
        TmuxColour::Basic(2),
        TmuxColour::Basic(3),
        TmuxColour::Indexed(231),
        TmuxColour::Basic(5),
        TmuxColour::Basic(6),
        TmuxColour::Basic(7),
        TmuxColour::Basic(1),
        TmuxColour::Basic(4),
    ]);
    model.set_status(status);
    let mut renderer = Renderer::new();
    renderer.paint_status_block(&model, true);
    let output = String::from_utf8(renderer.output).unwrap();

    assert!(output.contains("\x1b[38;5;231m"), "themegreen: {output:?}");
    assert!(output.contains("\x1b[48;5;124m"), "themeblack: {output:?}");
    assert!(
        !output.contains("\x1b[38;2;154;205;50m"),
        "the hard-coded dark table must not win: {output:?}"
    );

    let mut line = StyledLine::from_segments(parse_styled_segments("#[fg=themewhite]W"));
    line.resolve_theme(&zz_protocol::ThemeColours::default());
    assert_eq!(
        line.segments[0].style.fg,
        Some(TmuxColour::Rgb(0x00e5_e5e5))
    );
}

#[test]
fn the_status_row_never_carries_the_armed_prefix_hint() {
    let mut model = block_model(40, 10);
    model.set_status(block_status(vec!["ROW"], false));
    model.prefix_armed = true;
    assert_eq!(mode_indicators(&model), "");
    assert_eq!(status_indicators(&model), "PREFIX");
    assert!(status_overlay(&model, 40).is_none());
    let mut renderer = Renderer::new();
    renderer.paint_status_block(&model, true);
    let output = String::from_utf8(renderer.output).unwrap();
    assert!(!output.contains("PREFIX"), "{output:?}");

    let mut sidebar_model = block_model(120, 10);
    sidebar_model.focus_sidebar();
    assert!(sidebar_model.sidebar_visible());
    sidebar_model.set_status(block_status(vec!["ROW"], false));
    sidebar_model.prefix_armed = true;
    let mut renderer = Renderer::new();
    renderer.paint_sidebar(&sidebar_model, true);
    let output = String::from_utf8(renderer.output).unwrap();
    assert!(output.contains("PREFIX"), "{output:?}");
}

#[test]
fn a_pane_prompt_draws_over_its_pane_and_leaves_the_status_row() {
    let mut model = block_model(40, 8);
    attach_one_pane(&mut model, PaneId(1));
    model.set_status(block_status(vec!["ROW"], false));
    model.command_prompt = Some(zz_protocol::CommandPromptState {
        prompt: "(search down) ".to_owned(),
        input: "77".to_owned(),
        cursor: 2,
        kind: zz_protocol::CommandPromptKind::Value,
        history: Vec::new(),
        prompt_type: zz_protocol::CommandPromptType::Search,
        mode: zz_protocol::CommandPromptMode::Incremental,
        no_freeze: false,
        pane: Some(PaneId(1)),
        command_mode: false,
        prompt_cursor: zz_protocol::PromptCursor::default(),
    });

    let (_, content) = pane_prompt_target(&model).expect("the pane carries the prompt");
    assert_eq!(
        pane_prompt_row_y(&model, content),
        content.y + content.height - 1
    );
    assert!(status_overlay(&model, 40).is_none());

    let mut renderer = Renderer::new();
    renderer.paint_pane_prompt(&model);
    let painted = String::from_utf8(renderer.output).unwrap();
    assert!(painted.contains("(search down) 77"), "{painted:?}");
    let mut renderer = Renderer::new();
    renderer.place_active_cursor(&model);
    let placed = String::from_utf8(renderer.output).unwrap();
    assert!(
        placed.contains(&format!(
            "\x1b[{};{}H",
            pane_prompt_row_y(&model, content) + 1,
            content.x + 17
        )),
        "{placed:?}"
    );

    model
        .command_prompt
        .as_mut()
        .expect("the prompt is still up")
        .pane = None;
    assert!(pane_prompt_target(&model).is_none());
    assert!(matches!(
        status_overlay(&model, 40),
        Some(StatusOverlay { .. })
    ));
}

#[test]
fn the_mode_badge_lives_on_the_sidebar_and_never_on_the_status_row() {
    let mut model = block_model(120, 8);
    let mut viewport = TerminalViewport::blank(120, 6, SessionStatus::Running);
    viewport.mode = TerminalMode::View {
        position: 3,
        total: 40,
    };
    model.command_output = Some((PaneId(9), viewport));
    assert!(status_overlay(&model, 120).is_none());
    model.focus_sidebar();
    assert!(model.sidebar_visible());
    assert!(!status_indicators(&model).starts_with("VIEW"));
    attach_one_pane(&mut model, PaneId(9));
    assert!(status_indicators(&model).starts_with("VIEW 3/40"));
    assert!(status_overlay(&model, 120).is_none());
}
