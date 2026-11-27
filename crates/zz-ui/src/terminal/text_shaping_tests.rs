use super::*;

#[test]
fn adjacent_styles_share_one_multirun_shape_batch() {
    let foreground = Color::rgb(216, 222, 233);
    let background = Color::rgb(16, 19, 24);
    let normal = PackedStyle::new(
        foreground,
        background,
        None,
        0,
        TerminalUnderlineStyle::None,
    );
    let bold = PackedStyle::new(
        foreground,
        background,
        None,
        zz_terminal::ATTR_BOLD,
        TerminalUnderlineStyle::None,
    );
    let font = ala::font("monospace");
    let appearance = TerminalAppearance::default();
    let mut runs = SmallVec::new();
    let mut last_style = None;
    let mut last_foreground = None;

    push_cell_text_run(
        &mut runs,
        &mut last_style,
        &mut last_foreground,
        normal,
        foreground,
        1,
        &font,
        &appearance,
        1.0,
        1.0,
    );
    push_cell_text_run(
        &mut runs,
        &mut last_style,
        &mut last_foreground,
        normal,
        foreground,
        2,
        &font,
        &appearance,
        1.0,
        1.0,
    );
    push_cell_text_run(
        &mut runs,
        &mut last_style,
        &mut last_foreground,
        bold,
        foreground,
        1,
        &font,
        &appearance,
        1.0,
        1.0,
    );
    push_cell_text_run(
        &mut runs,
        &mut last_style,
        &mut last_foreground,
        bold,
        foreground,
        3,
        &font,
        &appearance,
        1.0,
        1.0,
    );

    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].len, 3);
    assert_eq!(runs[1].len, 4);
    assert_eq!(last_style, Some(bold));
    assert!(!same_visual_style(&runs[0], &runs[1]));
}

fn line_with_glyph_positions(text: &str, positions: &[(usize, f32)]) -> ShapedLine {
    let mut line = ShapedLine::default();
    line.text = text.to_owned().into();
    *line = Arc::new(ala::LineLayout {
        font_size: px(13.0),
        width: px(text.chars().count() as f32 * 7.8),
        runs: vec![ala::ShapedRun {
            font_id: ala::FontId(0),
            glyphs: positions
                .iter()
                .map(|&(index, x)| ala::ShapedGlyph {
                    id: ala::GlyphId(0),
                    position: point(px(x), px(0.0)),
                    index,
                    is_emoji: false,
                })
                .collect(),
        }],
        len: text.len(),
        ..Default::default()
    });
    line
}

#[test]
fn terminal_cell_positions_do_not_change_at_style_boundaries() {
    let text = "  TYPESAFE_AI_API_KEY=";
    let shape = |text: &str| {
        line_with_glyph_positions(
            text,
            &text
                .char_indices()
                .enumerate()
                .map(|(column, (index, _))| (index, column as f32 * 7.8))
                .collect::<Vec<_>>(),
        )
    };
    let mut whole = shape(text);
    align_line_to_cells(&mut whole, px(8.0));
    for boundary in 1..text.len() {
        let mut suffix = shape(&text[boundary..]);
        align_line_to_cells(&mut suffix, px(8.0));
        for (column, glyph) in suffix.runs[0].glyphs.iter().enumerate() {
            let expected = px(8.0) * (boundary + column);
            assert_eq!(whole.runs[0].glyphs[boundary + column].position.x, expected);
            assert_eq!(px(8.0) * boundary + glyph.position.x, expected);
        }
        assert_eq!(suffix.width, px(8.0) * (text.len() - boundary));
    }
}

#[test]
fn terminal_cell_alignment_preserves_clusters_and_wide_cell_reservations() {
    let mut line = line_with_glyph_positions(
        "é=>界 x",
        &[
            (0, 0.0),
            (0, 1.5),
            (2, 7.8),
            (4, 23.4),
            (7, 31.2),
            (8, 39.0),
        ],
    );
    let layout = Arc::get_mut(&mut line).unwrap();
    layout.runs[0].glyphs[1].position.y = px(-2.0);
    let remainder = layout.runs[0].glyphs.split_off(1);
    layout.runs.push(ala::ShapedRun {
        font_id: ala::FontId(1),
        glyphs: remainder,
    });
    let original = line.clone();

    align_line_to_cells(&mut line, px(8.0));

    let positions: Vec<_> = line
        .runs
        .iter()
        .flat_map(|run| &run.glyphs)
        .map(|glyph| f32::from(glyph.position.x))
        .collect();
    assert_eq!(positions, [0.0, 1.5, 8.0, 24.0, 32.0, 40.0]);
    assert_eq!(line.runs[1].glyphs[0].position.y, px(-2.0));
    assert_eq!(line.width, px(48.0));
    assert_eq!(original.runs[1].glyphs[1].position.x, px(7.8));
}

#[test]
fn text_opacity_scales_glyphs_and_decorations() {
    let foreground = Color::rgb(216, 222, 233);
    let style = PackedStyle::new(
        foreground,
        Color::rgb(16, 19, 24),
        None,
        0,
        TerminalUnderlineStyle::Single,
    );
    let run = text_run(
        style,
        1,
        &ala::font("monospace"),
        foreground,
        &TerminalAppearance::default(),
        1.0,
        0.7,
    );

    assert!((run.color.a - 0.7).abs() < f32::EPSILON);
    let underline = run.underline.expect("single underline is shaped with text");
    assert!(
        (underline
            .color
            .expect("terminal underline has an explicit color")
            .a
            - 0.7)
            .abs()
            < f32::EPSILON
    );
    assert!(run.background_color.is_none());
}

#[test]
fn cached_backgrounds_group_styled_cells_once_per_row_revision() {
    let mut viewport = TerminalViewport::blank(4, 1, zz_terminal::SessionStatus::Running);
    let highlighted = Color::rgb(44, 52, 68);
    let mut styles = viewport.styles().to_vec();
    styles.push(PackedStyle::new(
        viewport.foreground,
        highlighted,
        None,
        0,
        TerminalUnderlineStyle::None,
    ));
    Arc::make_mut(&mut viewport.dictionary).styles = styles.into();
    let cells = Arc::make_mut(&mut viewport.cells);
    cells[0] = PackedCell::new(u32::from('a'), 1, CellWidth::Narrow);
    cells[1] = PackedCell::new(u32::from('b'), 1, CellWidth::Narrow);
    cells[3] = PackedCell::new(u32::from('c'), 1, CellWidth::Narrow);

    let mut backgrounds = Vec::new();
    let mut batch = None;
    for (column, cell) in viewport
        .row(0)
        .expect("first row")
        .iter()
        .copied()
        .enumerate()
    {
        push_background(
            &mut batch,
            column,
            style_for(&viewport, None, cell).background(),
            viewport.background,
            &mut backgrounds,
        );
    }
    finish_backgrounds(batch, 4, viewport.background, &mut backgrounds);

    assert_eq!(backgrounds.len(), 2);
    assert_eq!(
        (backgrounds[0].start_column, backgrounds[0].cell_count),
        (0, 2)
    );
    assert_eq!(
        (backgrounds[1].start_column, backgrounds[1].cell_count),
        (3, 1)
    );
    assert!(backgrounds.iter().all(|run| run.color == highlighted));
}

#[test]
fn bold_italic_preserves_terminal_features_and_fallbacks() {
    let appearance = TerminalAppearance {
        font_families: vec!["Primary Mono".to_owned(), "Emoji Fallback".to_owned()],
        font_features: vec![zz_terminal::FontFeature::new(*b"ss01", 1)],
        font_weight: 450,
        ..TerminalAppearance::default()
    };
    let base = terminal_font_for_style(&appearance, "monospace", false, false);
    let style = PackedStyle::new(
        appearance.foreground,
        appearance.background,
        None,
        zz_terminal::ATTR_BOLD | zz_terminal::ATTR_ITALIC,
        TerminalUnderlineStyle::None,
    );

    let styled = styled_font(style, &base, &appearance);
    assert_eq!(styled.features, base.features);
    assert_eq!(styled.fallbacks, base.fallbacks);
    assert_eq!(styled.weight, FontWeight(750.0));
    assert_eq!(styled.style, FontStyle::Italic);
}

#[test]
fn ghostty_font_thickening_maps_to_explicit_glyph_smoothing() {
    let disabled = base_glyph_render_options(&TerminalAppearance::default());
    assert_eq!(disabled.font_smoothing, FontSmoothing::Disabled);
    assert!(!disabled.synthetic_bold);
    assert!(!disabled.synthetic_italic);

    let enabled = base_glyph_render_options(&TerminalAppearance {
        font_thicken: true,
        font_thicken_strength: 173,
        ..TerminalAppearance::default()
    });
    assert_eq!(enabled.font_smoothing, FontSmoothing::Enabled(173));
}
