use std::sync::Arc;

use super::test_support::{block_model, styled_viewport};
use super::*;
use zz_terminal::{CellWidth, SessionStatus};

#[test]
fn the_overflow_dollar_is_drawn_after_the_position_indicator() {
    let mut viewport = TerminalViewport::blank(8, 2, SessionStatus::Running);
    viewport.mode = TerminalMode::Copy {
        position: 0,
        total: 0,
        hide_position: false,
    };
    viewport.overlays = Arc::from([OverlaySpan::new(0, 7, 8, OverlayKind::CopyCursor)]);
    let mode = zz_protocol::ModePresentation {
        pane: PaneId(1),
        view: false,
        position: "#[align=right]POS".to_owned(),
        position_style: String::new(),
        selection_style: String::new(),
        vi_keys: false,
        match_style: String::new(),
        current_match_style: String::new(),
        line_numbers: 1,
        line_number_style: String::new(),
        current_line_number_style: String::new(),
        current_line_style: String::new(),
    };
    let gutter = crate::mode_view::line_number_gutter(&mode, &viewport);
    let rect = Rect {
        x: 0,
        y: 0,
        width: 8,
        height: 2,
    };
    let model = block_model(8, 2);
    let mut renderer = Renderer::new();
    renderer.paint_copy_chrome(&mode, gutter, &viewport, rect, &model);
    let output = String::from_utf8(renderer.output).unwrap();
    assert_eq!(
        output,
        "\u{1b}[1;1H\u{1b}[0m\u{1b}[39m\u{1b}[49m  0 \u{1b}[0m\u{1b}[2;1H\u{1b}[0m\u{1b}[39m\u{1b}[49m  1 \u{1b}[0m\u{1b}[1;5H\u{1b}[0m\u{1b}[39m\u{1b}[49m    \u{1b}[1;6H\u{1b}[0m\u{1b}[39m\u{1b}[49mPOS\u{1b}[0m\u{1b}[1;8H\u{1b}[0m\u{1b}[39m\u{1b}[49m$\u{1b}[0m"
    );
}

#[test]
fn the_copy_cursor_line_takes_copy_mode_current_line_style_to_the_edge() {
    let mut viewport = styled_viewport();
    viewport.mode = TerminalMode::Copy {
        position: 0,
        total: 0,
        hide_position: false,
    };
    viewport.overlays = Arc::from([OverlaySpan::new(0, 1, 2, OverlayKind::CopyCursor)]);
    let mut mode = zz_protocol::ModePresentation {
        pane: PaneId(1),
        view: false,
        position: String::new(),
        position_style: String::new(),
        selection_style: String::new(),
        vi_keys: false,
        match_style: String::new(),
        current_match_style: String::new(),
        line_numbers: 0,
        line_number_style: String::new(),
        current_line_number_style: String::new(),
        current_line_style: String::new(),
    };
    let theme = zz_protocol::ThemeColours::default();
    assert_eq!(
        crate::mode_view::current_line(&mode, &viewport, &theme),
        None
    );
    mode.current_line_style = "bg=red,bold".to_owned();
    let line = crate::mode_view::current_line(&mode, &viewport, &theme);
    assert_eq!(line.as_ref().map(|(row, _)| *row), Some(0));
    let rect = Rect {
        x: 0,
        y: 0,
        width: 6,
        height: 1,
    };
    let mut plain = Renderer::new();
    plain.blit_row(&viewport, 0, rect);
    let plain = String::from_utf8(plain.output).unwrap();
    assert!(!plain.contains("\x1b[41m"), "{plain:?}");

    let mut renderer = Renderer::new();
    renderer.current_line = line;
    renderer.blit_row(&viewport, 0, rect);
    let output = String::from_utf8(renderer.output.clone()).unwrap();
    assert!(output.contains("\x1b[1m\x1b[41m"), "{output:?}");
    assert!(!output.contains('X'), "{output:?}");

    renderer.output.clear();
    renderer.blit_row(&viewport, 1, rect);
    assert!(
        !String::from_utf8(renderer.output.clone())
            .unwrap()
            .contains("\x1b[41m")
    );

    mode.current_line_style = "curly-underscore".to_owned();
    let mut renderer = Renderer::new();
    renderer.current_line = crate::mode_view::current_line(&mode, &viewport, &theme);
    renderer.blit_row(&viewport, 0, rect);
    let output = String::from_utf8(renderer.output).unwrap();
    assert!(output.contains("\x1b[4:3m"), "{output:?}");
}

#[test]
fn the_copy_cursor_is_the_terminal_cursor_and_not_a_reversed_cell() {
    let mut viewport = styled_viewport();
    viewport.overlays = Arc::from([OverlaySpan::new(0, 1, 2, OverlayKind::Selection)]);
    let rect = Rect {
        x: 0,
        y: 0,
        width: 3,
        height: 1,
    };

    let mut renderer = Renderer::new();
    renderer.blit_row(&viewport, 0, rect);
    assert!(
        String::from_utf8(renderer.output)
            .unwrap()
            .contains("\x1b[7m")
    );

    viewport.overlays = Arc::from([OverlaySpan::new(0, 1, 2, OverlayKind::CopyCursor)]);
    let mut renderer = Renderer::new();
    renderer.blit_row(&viewport, 0, rect);
    assert!(
        !String::from_utf8(renderer.output)
            .unwrap()
            .contains("\x1b[7m")
    );

    let model = block_model(3, 1);
    let mut renderer = Renderer::new();
    renderer.place_viewport_cursor(PaneId(1), &viewport, rect, &model);
    let output = String::from_utf8(renderer.output).unwrap();
    assert!(output.contains("\x1b[1;2H"));
    assert!(output.contains("\x1b[?25h"));

    viewport.overlays = Arc::from([OverlaySpan::new(0, 3, 4, OverlayKind::CopyCursor)]);
    let mut renderer = Renderer::new();
    renderer.place_viewport_cursor(PaneId(1), &viewport, rect, &model);
    assert_eq!(String::from_utf8(renderer.output).unwrap(), "\x1b[?25l");
}

#[test]
fn cached_terminal_styles_keep_ansi_when_style_defaults_or_reverse_change() {
    let foreground = Color::rgb(120, 90, 40);
    let background = Color::rgb(20, 30, 40);
    let base = PackedStyle::new(foreground, background, None, 0, UnderlineStyle::None);
    let bold = PackedStyle::new(
        foreground,
        background,
        Some(Color::rgb(1, 2, 3)),
        zz_terminal::ATTR_BOLD,
        UnderlineStyle::Curly,
    );
    let mut viewport = styled_viewport();
    let mut renderer = Renderer::new();
    for (style, reverse, colours, defaults) in [
        (base, false, [foreground, background], [None, None]),
        (base, false, [foreground, background], [None, None]),
        (base, true, [foreground, background], [None, None]),
        (bold, false, [foreground, background], [None, None]),
        (base, false, [Color::rgb(1, 1, 1), background], [None, None]),
        (base, false, [foreground, Color::rgb(2, 2, 2)], [None, None]),
        (
            base,
            false,
            [foreground, background],
            [Some(TmuxColour::Indexed(33)), None],
        ),
        (
            base,
            false,
            [foreground, background],
            [None, Some(TmuxColour::Basic(3))],
        ),
    ] {
        viewport.foreground = colours[0];
        viewport.background = colours[1];
        renderer.terminal_defaults.fg = defaults[0];
        renderer.terminal_defaults.bg = defaults[1];
        let mut expected = Vec::new();
        write_sgr(
            &mut expected,
            style,
            reverse,
            viewport.foreground,
            viewport.background,
            &renderer.terminal_defaults,
        );
        renderer.output.clear();
        renderer.write_terminal_sgr(style, reverse, &viewport, false);
        assert_eq!(renderer.output, expected);
    }
    assert!(renderer.terminal_sgr.is_some());
    renderer.invalidate();
    assert!(renderer.terminal_sgr.is_none());
}

#[test]
fn unchanged_grid_emits_no_row_output() {
    let viewport = styled_viewport();
    let rect = Rect {
        x: 0,
        y: 0,
        width: 3,
        height: 1,
    };
    let mut renderer = Renderer::new();
    renderer.paint_terminal(PaneId(1), &viewport, rect, true, None);
    assert!(!renderer.output.is_empty());
    renderer.output.clear();

    renderer.paint_terminal(PaneId(1), &viewport, rect, false, None);

    assert!(renderer.output.is_empty());
}

#[test]
fn incremental_rows_only_repaint_changed_columns_and_clear_shorter_text() {
    let viewport = |text: &str| {
        let mut viewport = TerminalViewport::blank(24, 1, SessionStatus::Running);
        let cells = Arc::make_mut(&mut viewport.cells);
        for (column, value) in text.chars().enumerate() {
            cells[column] = PackedCell::new(value as u32, 0, CellWidth::Narrow);
        }
        viewport
    };
    let before = viewport("line 12345 padding");
    let mut after = before.clone();
    after.dictionary = Arc::new((*after.dictionary).clone());
    Arc::make_mut(&mut after.cells)[5] = PackedCell::new('9' as u32, 0, CellWidth::Narrow);
    let rect = Rect {
        x: 2,
        y: 3,
        width: 24,
        height: 1,
    };
    let mut renderer = Renderer::new();
    renderer.paint_terminal(PaneId(1), &before, rect, true, None);
    renderer.output.clear();
    renderer.paint_terminal(
        PaneId(1),
        &after,
        rect,
        false,
        Some(&FrameDamage::Rows(vec![0])),
    );
    let output = String::from_utf8(renderer.output.clone()).unwrap();
    assert_eq!(output, "\u{1b}[4;8H\u{1b}[0m\u{1b}[39m\u{1b}[49m9");

    let mut shorter = after.clone();
    Arc::make_mut(&mut shorter.cells)[5..10].fill(PackedCell::EMPTY);
    renderer.output.clear();
    renderer.paint_terminal(PaneId(1), &shorter, rect, false, None);
    let output = String::from_utf8(renderer.output).unwrap();
    assert_eq!(output, "\u{1b}[4;8H\u{1b}[0m\u{1b}[39m\u{1b}[49m     ");
}

#[test]
fn full_frame_damage_compares_replaced_rows_with_the_retained_screen() {
    let before = styled_viewport();
    let mut after = before.clone();
    after.dictionary = Arc::new((*after.dictionary).clone());
    Arc::make_mut(&mut after.cells)[1] = PackedCell::new('z' as u32, 0, CellWidth::Narrow);
    let rect = Rect {
        x: 0,
        y: 0,
        width: 3,
        height: 1,
    };
    let mut renderer = Renderer::new();
    renderer.paint_terminal(PaneId(1), &before, rect, true, None);
    renderer.output.clear();
    renderer.paint_terminal(PaneId(1), &after, rect, false, Some(&FrameDamage::All));
    let output = String::from_utf8(renderer.output.clone()).unwrap();
    assert!(output.starts_with("\x1b[1;2H"), "{output:?}");
    assert!(output.contains('z'), "{output:?}");
    assert!(!output.contains('a') && !output.contains('c'), "{output:?}");
    renderer.output.clear();
    renderer.paint_terminal(PaneId(1), &after, rect, false, Some(&FrameDamage::All));
    assert!(renderer.output.is_empty());
}

#[test]
fn changed_wide_cells_and_overlay_rows_keep_full_row_repaints() {
    let before = styled_viewport();
    let mut after = before.clone();
    Arc::make_mut(&mut after.cells)[1] = PackedCell::new('界' as u32, 0, CellWidth::Wide);
    assert_eq!(changed_columns(&before, &after, 0, 3), Some(0..3));

    let mut after = before.clone();
    after.overlays = Arc::from([OverlaySpan::new(0, 1, 2, OverlayKind::Selection)]);
    assert_eq!(changed_columns(&before, &after, 0, 3), Some(0..3));
}

#[test]
fn copy_mode_hides_only_its_position_when_the_daemon_asks() {
    assert_eq!(
        mode_indicator(TerminalMode::Copy {
            position: 3,
            total: 40,
            hide_position: false,
        }),
        "COPY 3/40"
    );
    assert_eq!(
        mode_indicator(TerminalMode::Copy {
            position: 3,
            total: 40,
            hide_position: true,
        }),
        "COPY"
    );
    assert_eq!(
        mode_indicator(TerminalMode::View {
            position: 3,
            total: 40,
        }),
        "VIEW 3/40"
    );
    assert_eq!(mode_indicator(TerminalMode::Live), "");
}
