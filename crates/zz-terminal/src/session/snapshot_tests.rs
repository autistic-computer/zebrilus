use super::test_support::*;
use super::*;

#[test]
fn reported_working_directories_keep_the_payload_the_pane_reported() {
    assert_eq!(
        reported_working_directory("file://workstation/tmp/a%20b/%E7%95%8C").as_deref(),
        Some("file://workstation/tmp/a%20b/%E7%95%8C")
    );
    assert_eq!(
        reported_working_directory("file://workstation/tmp/a b").as_deref(),
        Some("file://workstation/tmp/a b")
    );
    assert_eq!(
        reported_working_directory("/reported/path").as_deref(),
        Some("/reported/path")
    );
    assert_eq!(reported_working_directory(""), None);
    assert_eq!(
        reported_working_directory("file://[invalid").as_deref(),
        Some("file://[invalid")
    );
}

#[test]
fn snapshot_copies_styled_and_wide_cells() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"\x1b]2;fixture\x07\x1b[1;38;2;12;34;56mA\xe7\x95\x8c\x1b[0m");

    let mut render_state = RenderState::new().expect("render state");
    let mut rows = RowIterator::new().expect("rows");
    let mut cells = CellIterator::new().expect("cells");
    let mut generations = ViewportGenerations::default();
    let mut dictionary = ViewportDictionary::default();
    let viewport = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("snapshot");

    assert_eq!(viewport.title(), "fixture");
    assert_eq!(viewport.generation, 1);
    let a = viewport.cell(0, 0).expect("A cell");
    assert_eq!(viewport.cell_text(a), "A");
    assert!(viewport.style(a).expect("A style").bold());
    assert_eq!(
        viewport.style(a).expect("A style").foreground(),
        Color::rgb(12, 34, 56)
    );
    assert!(viewport.style(a).expect("A style").explicit_rgb());
    let wide = viewport.cell(0, 1).expect("wide cell");
    assert_eq!(viewport.cell_text(wide), "界");
    assert_eq!(wide.width(), CellWidth::Wide);
    assert_eq!(
        viewport.cell(0, 2).expect("wide spacer").width(),
        CellWidth::SpacerTail
    );
}

#[test]
fn configured_defaults_and_complete_palette_reach_ghostty() {
    let mut appearance = TerminalAppearance {
        foreground: Color::rgb(0x11, 0x22, 0x33),
        background: Color::rgb(0x04, 0x05, 0x06),
        cursor_color: Color::rgb(0xaa, 0xbb, 0xcc),
        cursor_style: CursorStyle::Underline,
        ..TerminalAppearance::default()
    };
    for index in 0_u16..=255 {
        let index = u8::try_from(index).expect("palette index");
        appearance.palette[usize::from(index)] =
            Color::rgb(index, u8::MAX - index, index.wrapping_mul(37));
    }

    let mut terminal = new_terminal(256, 2, 16).expect("terminal");
    apply_terminal_appearance(&mut terminal, &appearance).expect("apply appearance");
    assert_eq!(
        snapshot_fixture(&terminal).cursor.expect("cursor").style(),
        CursorStyle::Underline
    );
    let mut input = b"\x1b[0 q".to_vec();
    for index in 0_u16..=255 {
        input.extend_from_slice(format!("\x1b[38;5;{index}mX").as_bytes());
    }
    terminal.vt_write(&input);

    let viewport = snapshot_fixture(&terminal);
    assert_eq!(viewport.foreground, appearance.foreground);
    assert_eq!(viewport.background, appearance.background);
    assert_eq!(
        viewport.cursor.expect("cursor").color(),
        appearance.cursor_color
    );
    assert_eq!(
        viewport.cursor.expect("cursor").style(),
        CursorStyle::Underline
    );
    for (index, cell) in viewport.row(0).expect("first row").iter().enumerate() {
        assert_eq!(
            viewport.style(*cell).expect("style").foreground(),
            appearance.palette[index],
            "palette entry {index}"
        );
    }

    let mut reloaded = appearance.clone();
    reloaded.background = Color::rgb(0xde, 0xad, 0xbe);
    apply_terminal_appearance(&mut terminal, &reloaded).expect("reapply appearance");
    assert_eq!(
        terminal.bg_color().expect("effective background"),
        Some(ghostty_color(reloaded.background))
    );
}

#[test]
fn decscusr_outranks_the_configured_cursor_style_until_it_resets() {
    let appearance = TerminalAppearance {
        cursor_style: CursorStyle::Underline,
        ..TerminalAppearance::default()
    };
    let mut terminal = new_terminal(4, 1, 0).expect("terminal");
    apply_terminal_appearance(&mut terminal, &appearance).expect("apply appearance");

    terminal.vt_write(b"\x1b[5 q");
    assert_eq!(
        snapshot_fixture(&terminal).cursor.expect("cursor").style(),
        CursorStyle::Bar
    );

    let reloaded = TerminalAppearance {
        cursor_style: CursorStyle::Block,
        ..appearance
    };
    apply_terminal_appearance(&mut terminal, &reloaded).expect("reapply appearance");
    assert_eq!(
        snapshot_fixture(&terminal).cursor.expect("cursor").style(),
        CursorStyle::Bar,
        "a reload must not stomp the style the program selected"
    );

    terminal.vt_write(b"\x1b[0 q");
    let cursor = snapshot_fixture(&terminal).cursor.expect("cursor");
    assert_eq!(cursor.style(), CursorStyle::Block);
    assert!(cursor.blinking());
}

#[test]
fn osc_palette_override_takes_precedence_over_configured_palette() {
    let mut appearance = TerminalAppearance::default();
    appearance.palette[42] = Color::rgb(1, 2, 3);
    let mut terminal = new_terminal(2, 1, 16).expect("terminal");
    apply_terminal_appearance(&mut terminal, &appearance).expect("apply appearance");
    terminal.vt_write(b"\x1b]4;42;rgb:12/34/56\x1b\\\x1b[38;5;42mX");

    let viewport = snapshot_fixture(&terminal);
    let cell = viewport.row(0).expect("first row")[0];
    assert_eq!(
        viewport.style(cell).expect("style").foreground(),
        Color::rgb(0x12, 0x34, 0x56)
    );
}

#[test]
fn snapshot_cells_keep_the_colour_class_the_program_wrote() {
    let mut terminal = new_terminal(6, 1, 16).expect("terminal");
    apply_terminal_appearance(&mut terminal, &TerminalAppearance::default())
        .expect("apply appearance");
    terminal.vt_write(
        b"\x1b[31mR\x1b[0m\x1b[38;5;42mI\x1b[0m\x1b[38;2;10;20;30mX\x1b[0m\x1b[41mB\x1b[0mD\x1b[7mV",
    );
    let viewport = snapshot_fixture(&terminal);
    let row = viewport.row(0).expect("first row");
    let classes = |column: usize| {
        let style = viewport.style(row[column]).expect("style");
        (style.foreground_class(), style.background_class())
    };
    assert_eq!(classes(0), (ColourClass::Palette(1), ColourClass::Default));
    assert_eq!(classes(1), (ColourClass::Palette(42), ColourClass::Default));
    assert_eq!(classes(2), (ColourClass::Rgb, ColourClass::Default));
    assert_eq!(classes(3), (ColourClass::Default, ColourClass::Palette(1)));
    assert_eq!(classes(4), (ColourClass::Default, ColourClass::Default));
    assert_eq!(classes(5), (ColourClass::Resolved, ColourClass::Resolved));

    terminal.vt_write(b"\x1b]4;1;rgb:00/ff/00\x1b\\");
    let viewport = snapshot_fixture(&terminal);
    let row = viewport.row(0).expect("first row");
    let named = viewport.style(row[0]).expect("style");
    assert_eq!(named.foreground_class(), ColourClass::Rgb);
    assert_eq!(named.foreground(), Color::rgb(0, 255, 0));
    assert_eq!(
        viewport.style(row[3]).expect("style").background_class(),
        ColourClass::Rgb
    );
    assert_eq!(
        viewport.style(row[1]).expect("style").foreground_class(),
        ColourClass::Palette(42)
    );
}

#[test]
fn pane_colours_entries_leave_in_the_class_of_the_option_colour() {
    let mut appearance = TerminalAppearance::default();
    appearance.palette[1] = Color::rgb(0x12, 0x34, 0x56);
    appearance.palette[42] = appearance.palette[200];
    appearance.palette_classes = vec![(1, ColourClass::Rgb), (42, ColourClass::Palette(200))];
    let mut terminal = new_terminal(4, 1, 16).expect("terminal");
    apply_terminal_appearance(&mut terminal, &appearance).expect("apply appearance");
    terminal.vt_write(b"\x1b[31mR\x1b[0m\x1b[38;5;42mI\x1b[0m\x1b[32mG\x1b[0m\x1b[41mB\x1b[0m");
    fn cell_classes(
        terminal: &Terminal<'_, '_>,
        hints: &ClassHints,
    ) -> Vec<(ColourClass, ColourClass, Color)> {
        let mut render_state = RenderState::new().expect("render state");
        let mut rows = RowIterator::new().expect("rows");
        let mut cells = CellIterator::new().expect("cells");
        let mut generations = ViewportGenerations::default();
        let mut dictionary = ViewportDictionary {
            class_hints: hints.clone(),
            ..ViewportDictionary::default()
        };
        let viewport = snapshot(
            terminal,
            &mut render_state,
            &mut rows,
            &mut cells,
            &mut generations,
            SnapshotChange::Content,
            &mut dictionary,
            None,
            SessionStatus::Running,
            None,
        )
        .expect("snapshot");
        let row = viewport.row(0).expect("first row");
        row.iter()
            .take(4)
            .map(|cell| {
                let style = viewport.style(*cell).expect("style");
                (
                    style.foreground_class(),
                    style.background_class(),
                    style.foreground(),
                )
            })
            .collect()
    }
    let hints = ClassHints::new(&appearance);
    let classes = |terminal: &Terminal<'_, '_>| cell_classes(terminal, &hints);

    let stock = classes(&terminal);
    assert_eq!(
        stock[0],
        (
            ColourClass::Rgb,
            ColourClass::Default,
            Color::rgb(0x12, 0x34, 0x56)
        )
    );
    assert_eq!(stock[1].0, ColourClass::Palette(200));
    assert_eq!(stock[2].0, ColourClass::Palette(2));
    assert_eq!(stock[3].1, ColourClass::Rgb);

    terminal.vt_write(b"\x1b]4;1;rgb:00/ff/00\x1b\\");
    let osc = classes(&terminal);
    assert_eq!(
        osc[0],
        (
            ColourClass::Rgb,
            ColourClass::Default,
            Color::rgb(0, 255, 0)
        )
    );
    assert_eq!(osc[2].0, ColourClass::Palette(2));

    terminal.vt_write(b"\x1b]104\x1b\\");
    assert_eq!(classes(&terminal), stock);
}

#[test]
fn default_grounds_leave_in_the_class_of_the_window_style_or_osc_colour() {
    let mut appearance = TerminalAppearance::default();
    appearance.foreground = appearance.palette[2];
    appearance.background = appearance.palette[4];
    appearance.default_classes = [Some(ColourClass::Palette(2)), Some(ColourClass::Palette(4))];
    let hints = ClassHints::new(&appearance);
    let mut terminal = new_terminal(4, 1, 16).expect("terminal");
    apply_terminal_appearance(&mut terminal, &appearance).expect("apply appearance");
    let classes = |terminal: &Terminal<'_, '_>, text: &[u8]| {
        let mut terminal_output = text.to_vec();
        terminal_output.clear();
        let mut render_state = RenderState::new().expect("render state");
        let mut rows = RowIterator::new().expect("rows");
        let mut cells = CellIterator::new().expect("cells");
        let mut generations = ViewportGenerations::default();
        let mut dictionary = ViewportDictionary {
            class_hints: hints.clone(),
            ..ViewportDictionary::default()
        };
        let viewport = snapshot(
            terminal,
            &mut render_state,
            &mut rows,
            &mut cells,
            &mut generations,
            SnapshotChange::Content,
            &mut dictionary,
            None,
            SessionStatus::Running,
            None,
        )
        .expect("snapshot");
        let row = viewport.row(0).expect("first row");
        let cell = viewport.style(row[0]).expect("style");
        let blank = viewport.styles()[0];
        (
            (
                cell.foreground_class(),
                cell.background_class(),
                cell.foreground(),
            ),
            (blank.foreground_class(), blank.background_class()),
        )
    };

    terminal.vt_write(b"D");
    let window = classes(&terminal, b"");
    assert_eq!(
        window,
        (
            (
                ColourClass::Palette(2),
                ColourClass::Palette(4),
                appearance.palette[2]
            ),
            (ColourClass::Palette(2), ColourClass::Palette(4)),
        )
    );

    terminal.vt_write(b"\x1b]10;rgb:ff/00/00\x1b\\");
    let osc = classes(&terminal, b"");
    assert_eq!(
        osc.0,
        (
            ColourClass::Rgb,
            ColourClass::Palette(4),
            Color::rgb(255, 0, 0)
        )
    );
    assert_eq!(osc.1, (ColourClass::Rgb, ColourClass::Palette(4)));

    terminal.vt_write(b"\x1b]110\x1b\\");
    assert_eq!(classes(&terminal, b""), window);

    terminal.vt_write(b"\x1b]11;rgb:00/00/80\x1b\\");
    assert_eq!(
        classes(&terminal, b"").1,
        (ColourClass::Palette(2), ColourClass::Rgb)
    );
}

#[test]
fn an_osc_reset_leaves_the_default_grounds_following_the_appearance() {
    let theme = TerminalAppearance::default();
    let mut styled = TerminalAppearance::default();
    styled.foreground = styled.palette[2];
    styled.background = styled.palette[4];
    let mut terminal = new_terminal(4, 1, 16).expect("terminal");
    apply_terminal_appearance(&mut terminal, &theme).expect("apply theme");
    terminal.vt_write(
        b"\x1b]10;rgb:ff/00/00\x1b\\\x1b]11;rgb:00/00/80\x1b\\\x1b]110\x1b\\\x1b]111\x1b\\",
    );
    apply_terminal_appearance(&mut terminal, &styled).expect("apply styled");
    assert_eq!(
        terminal.fg_color().expect("foreground"),
        Some(ghostty_color(styled.foreground))
    );
    assert_eq!(
        terminal.bg_color().expect("background"),
        Some(ghostty_color(styled.background))
    );

    terminal.vt_write(b"\x1b]10;rgb:ff/00/00\x1b\\");
    apply_terminal_appearance(&mut terminal, &theme).expect("apply theme again");
    assert_eq!(
        terminal.fg_color().expect("foreground"),
        Some(ghostty_color(Color::rgb(255, 0, 0)))
    );
    assert_eq!(
        terminal.bg_color().expect("background"),
        Some(ghostty_color(theme.background))
    );
}

#[test]
fn overlay_snapshots_reuse_cells_and_dictionary_tables() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write(b"one");
    let mut render_state = RenderState::new().expect("render state");
    let mut rows = RowIterator::new().expect("rows");
    let mut cells = CellIterator::new().expect("cells");
    let mut generations = ViewportGenerations::default();
    let mut dictionary = ViewportDictionary::default();
    let first = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("first snapshot");
    let style_allocation = dictionary.row_styles.as_ptr();
    let style_capacity = dictionary.row_styles.capacity();
    let overlay_only = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::View,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("overlay snapshot");
    assert!(Arc::ptr_eq(
        &first.presentation.title,
        &overlay_only.presentation.title
    ));
    assert!(Arc::ptr_eq(&first.presentation, &overlay_only.presentation));
    assert!(Arc::ptr_eq(&first.cells, &overlay_only.cells));
    assert!(Arc::ptr_eq(&first.overlays, &overlay_only.overlays));
    assert!(Arc::ptr_eq(&first.dictionary, &overlay_only.dictionary));
    assert_eq!(dictionary.row_styles.as_ptr(), style_allocation);
    assert_eq!(dictionary.row_styles.capacity(), style_capacity);

    terminal.vt_write(b"two");
    let changed = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("changed snapshot");
    assert!(Arc::ptr_eq(
        &overlay_only.presentation.title,
        &changed.presentation.title
    ));
    assert!(Arc::ptr_eq(
        &overlay_only.presentation,
        &changed.presentation
    ));
    assert!(!Arc::ptr_eq(&overlay_only.cells, &changed.cells));
    assert!(Arc::ptr_eq(&overlay_only.dictionary, &changed.dictionary));
    assert_eq!(dictionary.row_styles.as_ptr(), style_allocation);
    assert_eq!(dictionary.row_styles.capacity(), style_capacity);

    terminal.vt_write(b"\x1b]0;renamed\x07");
    let renamed = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("renamed snapshot");
    assert_eq!(renamed.title(), "renamed");
    assert!(!Arc::ptr_eq(
        &changed.presentation.title,
        &renamed.presentation.title
    ));
    assert!(!Arc::ptr_eq(&changed.presentation, &renamed.presentation));
    assert_eq!(dictionary.row_styles.as_ptr(), style_allocation);
    assert_eq!(dictionary.row_styles.capacity(), style_capacity);
}

#[test]
fn dirty_snapshots_reuse_a_released_packed_cell_plane() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    let mut render_state = RenderState::new().expect("render state");
    let mut rows = RowIterator::new().expect("rows");
    let mut cells = CellIterator::new().expect("cells");
    let mut generations = ViewportGenerations::default();
    let mut dictionary = ViewportDictionary::default();

    terminal.vt_write(b"a");
    let first = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("first snapshot");
    let first_allocation = first.cells.as_ptr();

    terminal.vt_write(b"b");
    let second = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("second snapshot");
    assert_ne!(second.cells.as_ptr(), first_allocation);

    drop(first);
    terminal.vt_write(b"c");
    let third = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("third snapshot");
    assert_eq!(third.cells.as_ptr(), first_allocation);
    assert_eq!(third.cell_text(third.cell(0, 0).expect("a")), "a");
    assert_eq!(third.cell_text(third.cell(0, 1).expect("b")), "b");
    assert_eq!(third.cell_text(third.cell(0, 2).expect("c")), "c");
}

#[test]
fn dictionary_tables_publish_only_the_changed_plane() {
    let mut terminal = new_terminal(8, 2, 16).expect("terminal");
    terminal.vt_write("e\u{301}".as_bytes());
    let mut render_state = RenderState::new().expect("render state");
    let mut rows = RowIterator::new().expect("rows");
    let mut cells = CellIterator::new().expect("cells");
    let mut generations = ViewportGenerations::default();
    let mut dictionary = ViewportDictionary::default();
    let initial = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("initial dictionary snapshot");
    assert!(initial.grapheme_offsets().len() > 1);

    terminal.vt_write(b"\x1b[31mX");
    let styled = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("style-only dictionary append");
    assert!(!Arc::ptr_eq(&initial.dictionary, &styled.dictionary));
    assert!(!Arc::ptr_eq(
        &initial.dictionary.styles,
        &styled.dictionary.styles
    ));
    assert!(Arc::ptr_eq(
        &initial.dictionary.grapheme_offsets,
        &styled.dictionary.grapheme_offsets
    ));
    assert!(Arc::ptr_eq(
        &initial.dictionary.grapheme_bytes,
        &styled.dictionary.grapheme_bytes
    ));

    terminal.vt_write("o\u{302}".as_bytes());
    let grapheme = snapshot(
        &terminal,
        &mut render_state,
        &mut rows,
        &mut cells,
        &mut generations,
        SnapshotChange::Content,
        &mut dictionary,
        None,
        SessionStatus::Running,
        None,
    )
    .expect("grapheme-only dictionary append");
    assert!(!Arc::ptr_eq(&styled.dictionary, &grapheme.dictionary));
    assert!(Arc::ptr_eq(
        &styled.dictionary.styles,
        &grapheme.dictionary.styles
    ));
    assert!(!Arc::ptr_eq(
        &styled.dictionary.grapheme_offsets,
        &grapheme.dictionary.grapheme_offsets
    ));
    assert!(!Arc::ptr_eq(
        &styled.dictionary.grapheme_bytes,
        &grapheme.dictionary.grapheme_bytes
    ));
}
