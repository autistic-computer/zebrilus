use super::*;

#[test]
fn viewport_generations_never_restart_across_terminal_lifetimes() {
    let first = ViewportGenerations::new().expect("generations");
    let second = ViewportGenerations::new().expect("generations");

    assert!(second.content > first.content);
    assert!(second.view > first.view);
    assert_eq!(second.content, second.view);
}

#[test]
fn dictionary_last_style_cache_bypasses_hash_lookup_for_runs() {
    let foreground = Color::rgb(216, 222, 233);
    let background = Color::rgb(16, 19, 24);
    let normal = PackedStyle::new(foreground, background, None, 0, UnderlineStyle::None);
    let bold = PackedStyle::new(
        foreground,
        background,
        None,
        ATTR_BOLD,
        UnderlineStyle::None,
    );
    let mut dictionary = ViewportDictionary::default();
    dictionary.ensure_default(normal, &[RgbColor::default(); 256]);

    dictionary.style_ids.clear();
    assert_eq!(dictionary.intern_style(normal), 0);
    assert_eq!(dictionary.styles.len(), 1);

    assert_eq!(dictionary.intern_style(bold), 1);
    dictionary.style_ids.clear();
    assert_eq!(dictionary.intern_style(bold), 1);
    assert_eq!(dictionary.styles.len(), 2);
}

#[test]
fn palette_changes_reset_the_resolved_style_dictionary() {
    let foreground = Color::rgb(216, 222, 233);
    let background = Color::rgb(16, 19, 24);
    let normal = PackedStyle::new(foreground, background, None, 0, UnderlineStyle::None);
    let indexed = PackedStyle::new(
        Color::rgb(10, 20, 30),
        background,
        None,
        0,
        UnderlineStyle::None,
    );
    let mut palette = [RgbColor::default(); 256];
    let mut dictionary = ViewportDictionary::default();
    dictionary.ensure_default(normal, &palette);
    assert_eq!(dictionary.intern_style(indexed), 1);
    let generation = dictionary.generation;

    palette[42] = RgbColor {
        r: 0xaa,
        g: 0xbb,
        b: 0xcc,
    };
    dictionary.ensure_default(normal, &palette);

    assert_ne!(dictionary.generation, generation);
    assert_eq!(dictionary.styles, [normal]);
    assert_eq!(dictionary.last_style_id, 0);
}

#[test]
fn dictionary_hard_limits_degrade_excess_cells_instead_of_failing() {
    let foreground = Color::rgb(216, 222, 233);
    let background = Color::rgb(16, 19, 24);
    let normal = PackedStyle::new(foreground, background, None, 0, UnderlineStyle::None);
    let excess = PackedStyle::new(
        Color::rgb(1, 2, 3),
        background,
        None,
        ATTR_BOLD,
        UnderlineStyle::None,
    );
    let palette = [RgbColor::default(); 256];
    let mut dictionary = ViewportDictionary::default();
    dictionary.ensure_default(normal, &palette);
    dictionary.styles.resize(MAX_VIEWPORT_STYLE_COUNT, normal);

    assert_eq!(dictionary.intern_style(excess), 0);
    assert!(dictionary.style_overflowed);

    dictionary.reset_live(normal, &palette);
    dictionary
        .grapheme_offsets
        .resize(MAX_VIEWPORT_GRAPHEME_COUNT + 1, 0);
    assert_eq!(dictionary.encode_glyph("e\u{301}"), u32::from('e'));
    assert!(dictionary.grapheme_overflowed);
}

#[test]
fn live_dictionary_compaction_rebuilds_the_visible_working_set() {
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
    .expect("initial snapshot");
    let previous_dictionary_generation = first.dictionary_generation;
    let compaction_limit = dictionary.style_compaction_limit;
    assert!(compaction_limit >= MIN_STYLE_COMPACTION_LIMIT);
    let default_style = dictionary.styles[0];
    dictionary
        .styles
        .resize(compaction_limit.saturating_add(1), default_style);

    terminal.vt_write(b"b");
    let compacted = snapshot(
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
    .expect("compacted snapshot");

    assert_ne!(
        compacted.dictionary_generation,
        previous_dictionary_generation
    );
    assert!(dictionary.styles.len() < compaction_limit);
    assert_eq!(
        compacted.cell_text(compacted.cell(0, 0).expect("first cell")),
        "a"
    );
    assert_eq!(
        compacted.cell_text(compacted.cell(0, 1).expect("second cell")),
        "b"
    );
    assert!(TerminalViewport::diff(&first, &compacted).is_none());
}

#[test]
fn viewport_dictionary_reuses_a_released_cell_plane() {
    let mut dictionary = ViewportDictionary {
        shared_cells: Arc::from([
            PackedCell::new(u32::from('a'), 0, CellWidth::Narrow),
            PackedCell::EMPTY,
        ]),
        ..ViewportDictionary::default()
    };
    let retained_snapshot = Arc::clone(&dictionary.shared_cells);
    let original_allocation = dictionary.shared_cells.as_ptr();

    let mut next = dictionary.acquire_cell_plane(2, true);
    Arc::get_mut(&mut next).expect("unique next plane")[1] =
        PackedCell::new(u32::from('b'), 0, CellWidth::Narrow);
    dictionary.commit_cell_plane(next);
    assert_eq!(dictionary.cell_pool.len(), 1);

    drop(retained_snapshot);
    let recycled = dictionary.acquire_cell_plane(2, true);
    assert_eq!(recycled.as_ptr(), original_allocation);
    assert_eq!(recycled.as_ref(), dictionary.shared_cells.as_ref());
}

#[test]
fn viewport_dictionary_cell_pool_stays_bounded_for_retained_snapshots() {
    let mut dictionary = ViewportDictionary {
        shared_cells: Arc::from([PackedCell::EMPTY; 2]),
        ..ViewportDictionary::default()
    };
    let mut retained = Vec::new();

    for glyph in b'a'..=b'h' {
        retained.push(Arc::clone(&dictionary.shared_cells));
        let mut next = dictionary.acquire_cell_plane(2, true);
        Arc::get_mut(&mut next).expect("unique next plane")[0] =
            PackedCell::new(u32::from(glyph), 0, CellWidth::Narrow);
        dictionary.commit_cell_plane(next);
        assert!(dictionary.cell_pool.len() <= RETAINED_CELL_PLANES);
    }

    assert_eq!(retained.len(), 8);
    assert_eq!(dictionary.cell_pool.len(), RETAINED_CELL_PLANES);
}

#[test]
fn viewport_dictionary_geometry_changes_release_incompatible_planes() {
    for (previous_len, next_len) in [(8, 2), (2, 8)] {
        let original = PackedCell::new(u32::from('a'), 0, CellWidth::Narrow);
        let mut dictionary = ViewportDictionary {
            shared_cells: std::iter::repeat_n(original, previous_len).collect(),
            ..ViewportDictionary::default()
        };
        let retained_snapshot = Arc::clone(&dictionary.shared_cells);
        let incompatible: Arc<[PackedCell]> = std::iter::repeat_n(original, previous_len).collect();
        let released = Arc::downgrade(&incompatible);
        let compatible: Arc<[PackedCell]> =
            std::iter::repeat_n(PackedCell::EMPTY, next_len).collect();
        let reusable = compatible.as_ptr();
        dictionary.cell_pool.extend([incompatible, compatible]);

        let next = std::iter::repeat_n(PackedCell::EMPTY, next_len).collect();
        dictionary.commit_cell_plane(next);

        assert!(released.upgrade().is_none());
        assert_eq!(dictionary.cell_pool.len(), 1);
        assert_eq!(dictionary.cell_pool[0].len(), next_len);
        assert_eq!(Arc::strong_count(&retained_snapshot), 1);
        assert!(retained_snapshot.iter().all(|cell| *cell == original));
        let next = dictionary.acquire_cell_plane(next_len, false);
        assert_eq!(next.as_ptr(), reusable);
        dictionary.commit_cell_plane(next);
        assert_eq!(dictionary.cell_pool.len(), 1);
        assert_eq!(dictionary.cell_pool[0].len(), next_len);
    }
}

#[test]
fn viewport_dictionary_recycles_an_uncommitted_resized_plane() {
    let original = PackedCell::new(u32::from('a'), 0, CellWidth::Narrow);
    let mut dictionary = ViewportDictionary {
        shared_cells: Arc::from([original; 8]),
        ..ViewportDictionary::default()
    };
    let retained_snapshot = Arc::clone(&dictionary.shared_cells);
    let mut uncommitted = dictionary.acquire_cell_plane(2, false);
    let allocation = uncommitted.as_ptr();
    Arc::get_mut(&mut uncommitted).expect("unique plane")[0] = original;
    dictionary.retain_cell_plane(uncommitted);

    assert!(Arc::ptr_eq(&dictionary.shared_cells, &retained_snapshot));
    assert_eq!(dictionary.cell_pool.len(), 1);
    let retried = dictionary.acquire_cell_plane(2, false);
    assert_eq!(retried.as_ptr(), allocation);
    assert_eq!(retried.as_ref(), [PackedCell::EMPTY; 2]);
    dictionary.commit_cell_plane(retried);
    assert!(dictionary.cell_pool.is_empty());
    assert_eq!(retained_snapshot.as_ref(), [original; 8]);
}

#[test]
fn viewport_dictionary_reuses_a_released_overlay_plane() {
    let first_span = OverlaySpan::new(0, 0, 1, OverlayKind::Selection);
    let mut dictionary = ViewportDictionary {
        shared_overlays: Arc::from([first_span]),
        ..ViewportDictionary::default()
    };
    let retained_snapshot = Arc::clone(&dictionary.shared_overlays);
    let first_allocation = dictionary.shared_overlays.as_ptr();

    let second =
        dictionary.finish_overlays(vec![OverlaySpan::new(0, 1, 2, OverlayKind::Selection)]);
    assert_ne!(second.as_ptr(), first_allocation);

    drop(retained_snapshot);
    let recycled =
        dictionary.finish_overlays(vec![OverlaySpan::new(0, 2, 3, OverlayKind::Selection)]);
    assert_eq!(recycled.as_ptr(), first_allocation);
    assert_eq!(
        recycled.as_ref(),
        [OverlaySpan::new(0, 2, 3, OverlayKind::Selection)]
    );
    assert!(dictionary.overlay_pool.len() <= RETAINED_OVERLAY_PLANES);
}
