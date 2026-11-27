use super::*;

fn signature(dictionary_generation: u32) -> RowCacheSignature {
    RowCacheSignature {
        dictionary_generation,
        scale_bits: 1.0_f32.to_bits(),
        font: ala::font("monospace"),
        font_size: px(14.0),
        cell_width: px(8.0),
        foreground: Color::rgb(216, 222, 233),
        background: Color::rgb(16, 19, 24),
        appearance_hash: TerminalAppearance::default().stable_hash(),
        text_opacity_bits: 1.0_f32.to_bits(),
    }
}

#[test]
fn retained_row_cache_prunes_only_stale_revisions() {
    let mut cache = RowRenderCache::default();
    let initial = [1, 2];
    cache.prepare(signature(1), 1, false, initial.iter().copied());
    cache.rows.insert(1, Rc::new(CachedTextRow::default()));
    cache.rows.insert(3, Rc::new(CachedTextRow::default()));
    cache.selection_rows.insert(
        SelectionCacheKey {
            revision: 1,
            start: 0,
            end: 2,
        },
        Rc::new(CachedTextRow::default()),
    );
    cache.selection_rows.insert(
        SelectionCacheKey {
            revision: 3,
            start: 0,
            end: 2,
        },
        Rc::new(CachedTextRow::default()),
    );

    let updated = [1, 2];
    cache.prepare(signature(1), 2, false, updated.iter().copied());
    assert!(cache.rows.contains_key(&1));
    assert!(!cache.rows.contains_key(&3));
    assert_eq!(cache.selection_rows.len(), 1);
    let live_capacity = cache.live_revisions.capacity();

    let reduced = [1];
    cache.prepare(signature(1), 3, false, reduced.iter().copied());
    assert!(cache.live_revisions.capacity() >= live_capacity);

    cache.prepare(signature(2), 3, false, reduced.iter().copied());
    assert!(cache.rows.is_empty());
    assert!(cache.selection_rows.is_empty());
}

#[test]
fn returning_from_local_scroll_releases_cached_history_without_new_output() {
    let mut cache = RowRenderCache::default();
    let history = 1..=10_000;
    let live = 10_001..=10_040;
    cache.prepare(signature(1), 1, false, live.clone());
    for revision in live.clone() {
        cache
            .rows
            .insert(revision, Rc::new(CachedTextRow::default()));
    }
    let retained_live_row = Rc::clone(&cache.rows[&10_001]);

    cache.prepare(signature(1), 1, true, history.clone().chain(live.clone()));
    for revision in history {
        cache
            .rows
            .insert(revision, Rc::new(CachedTextRow::default()));
    }
    assert_eq!(cache.rows.len(), 10_040);

    cache.prepare(signature(1), 1, false, live);
    assert_eq!(cache.rows.len(), 40);
    assert!(Rc::ptr_eq(&cache.rows[&10_001], &retained_live_row));
}

#[test]
fn text_opacity_change_invalidates_shaped_rows() {
    let mut cache = RowRenderCache::default();
    let revisions = [1];
    cache.prepare(signature(1), 1, false, revisions.iter().copied());
    cache.rows.insert(1, Rc::new(CachedTextRow::default()));
    cache.selection_rows.insert(
        SelectionCacheKey {
            revision: 1,
            start: 0,
            end: 2,
        },
        Rc::new(CachedTextRow::default()),
    );

    let mut dimmed = signature(1);
    dimmed.text_opacity_bits = 0.7_f32.to_bits();
    cache.prepare(dimmed, 1, false, revisions.iter().copied());

    assert!(cache.rows.is_empty());
    assert!(cache.selection_rows.is_empty());
}

#[test]
fn pending_live_resize_projects_retained_rows_from_the_bottom() {
    let shrink = RowProjection::new(30, 24, true);
    assert_eq!(shrink.source_start, 6);
    assert_eq!(shrink.destination_start, 0);
    assert_eq!(shrink.display_row(6), Some(0));
    assert_eq!(shrink.display_row(29), Some(23));
    assert_eq!(shrink.display_row(5), None);

    let grow = RowProjection::new(24, 30, true);
    assert_eq!(grow.source_start, 0);
    assert_eq!(grow.destination_start, 6);
    assert_eq!(grow.display_row(0), Some(6));
    assert_eq!(grow.display_row(23), Some(29));

    let top = RowProjection::new(30, 24, false);
    assert_eq!(top.display_row(0), Some(0));
    assert_eq!(top.display_row(23), Some(23));
    assert_eq!(top.display_row(24), None);
}

#[test]
fn local_scroll_projection_splits_history_live_and_shimmer_rows() {
    assert_eq!(local_row_source(96, 100, 4, 3, 0), LocalRowSource::Shimmer);
    assert_eq!(
        local_row_source(97, 100, 4, 3, 0),
        LocalRowSource::History(0)
    );
    assert_eq!(
        local_row_source(98, 100, 4, 3, 0),
        LocalRowSource::History(1)
    );
    assert_eq!(
        local_row_source(99, 100, 4, 3, 0),
        LocalRowSource::History(2)
    );
    assert_eq!(local_row_source(100, 100, 4, 3, 0), LocalRowSource::Live(0));
    assert_eq!(local_row_source(103, 100, 4, 3, 0), LocalRowSource::Live(3));
    assert_eq!(local_row_source(104, 100, 4, 3, 0), LocalRowSource::Shimmer);
    assert_eq!(
        local_row_source(104, 100, 4, 3, 2),
        LocalRowSource::Below(0)
    );
    assert_eq!(
        local_row_source(105, 100, 4, 3, 2),
        LocalRowSource::Below(1)
    );
    assert_eq!(local_row_source(106, 100, 4, 3, 2), LocalRowSource::Shimmer);

    let projection = local_live_projection(97, 100, 4, 4);
    assert_eq!(
        projection,
        RowProjection {
            source_start: 0,
            destination_start: 3,
            count: 1,
        }
    );
    assert_eq!(projection.display_row(0), Some(3));

    assert_eq!(
        local_live_projection(95, 100, 4, 4),
        RowProjection {
            source_start: 0,
            destination_start: 0,
            count: 0,
        }
    );
}

#[test]
fn retained_rows_decode_graphemes_with_their_own_dictionary() {
    let cell = PackedCell::new(GRAPHEME_TABLE_BIT, 0, CellWidth::Narrow);
    let dictionary = |text: &'static [u8]| {
        TerminalDictionary::from_shared(
            Arc::<[PackedStyle]>::from([]),
            Arc::from([0, u32::try_from(text.len()).expect("small grapheme")]),
            Arc::from(text),
        )
    };
    let older = dictionary(b"old");
    let newer = dictionary(b"new");

    assert_eq!(glyph_for_dictionary(&older, cell), Glyph::Grapheme("old"));
    assert_eq!(glyph_for_dictionary(&newer, cell), Glyph::Grapheme("new"));
}

#[test]
fn internal_revision_keys_bypass_general_purpose_hashing() {
    let mut hasher = RevisionHasher::default();
    hasher.write_u64(0x1234_5678_9abc_def0);
    assert_eq!(hasher.finish(), 0x1234_5678_9abc_def0);

    let mut revisions = RevisionMap::default();
    for revision in 1_u64..=128 {
        revisions.insert(revision, revision * 2);
    }
    assert_eq!(revisions.len(), 128);
    assert_eq!(revisions.get(&64), Some(&128));
}

#[test]
#[cfg(target_pointer_width = "64")]
fn cached_row_handles_keep_paint_records_compact() {
    assert_eq!(std::mem::size_of::<PositionedTextRow>(), 16);
    assert_eq!(std::mem::align_of::<PositionedTextRow>(), 8);
    assert!(std::mem::size_of::<PositionedTextRow>() < std::mem::size_of::<CachedLine>());

    let cached = Rc::new(CachedTextRow::default());
    let positioned = PositionedTextRow {
        row: Rc::clone(&cached),
        origin: point(px(1.0), px(2.0)),
    };
    assert!(Rc::ptr_eq(&positioned.row, &cached));
}

#[test]
fn appearance_hash_change_invalidates_shaped_rows() {
    let mut cache = RowRenderCache::default();
    let first = signature(1);
    cache.prepare(first.clone(), 1, false, [7].into_iter());
    cache.rows.insert(7, Rc::new(CachedTextRow::default()));
    let mut changed = first;
    changed.appearance_hash = changed.appearance_hash.wrapping_add(1);
    cache.prepare(changed, 1, false, [7].into_iter());
    assert!(cache.rows.is_empty());
}

fn filled_viewport(columns: u16, rows: u16, glyph: char) -> TerminalViewport {
    let mut viewport = TerminalViewport::blank(columns, rows, zz_terminal::SessionStatus::Running);
    viewport.cells = (0..usize::from(columns) * usize::from(rows))
        .map(|_| PackedCell::new(u32::from(glyph), 0, CellWidth::Narrow))
        .collect();
    viewport
}

#[test]
fn scrolled_grids_stay_bottom_anchored_so_a_gesture_never_jumps_by_the_spare_strip() {
    let mut viewport = TerminalViewport::blank(8, 4, zz_terminal::SessionStatus::Running);
    viewport.scrollbar = ScrollbarState {
        total: 40,
        offset: 36,
        len: 4,
    };
    let top = GridPlacement {
        bottom_anchored: false,
        project_from_bottom: false,
    };
    let scrolled = GridPlacement {
        bottom_anchored: true,
        project_from_bottom: false,
    };
    assert_eq!(grid_placement(&viewport, None, false, false), top);
    assert_eq!(grid_placement(&viewport, None, false, true), scrolled);

    let mut filled = filled_viewport(8, 4, 'x');
    filled.scrollbar = viewport.scrollbar;
    let live_bottom = GridPlacement {
        bottom_anchored: true,
        project_from_bottom: true,
    };
    assert_eq!(grid_placement(&filled, None, false, false), live_bottom);
    assert_eq!(grid_placement(&filled, None, false, true), scrolled);
    filled.scrollbar.offset = 20;
    assert_eq!(grid_placement(&filled, None, false, false), scrolled);
    viewport.scrollbar.offset = 20;
    assert_eq!(grid_placement(&viewport, None, false, false), scrolled);

    assert_eq!(grid_placement(&filled, None, true, true), top);
    filled.mode = zz_terminal::TerminalMode::View {
        position: 1,
        total: 40,
    };
    assert_eq!(grid_placement(&filled, None, false, true), top);
}

#[test]
fn entering_copy_mode_keeps_the_live_placement() {
    let mut filled = filled_viewport(8, 4, 'x');
    filled.scrollbar = ScrollbarState {
        total: 40,
        offset: 36,
        len: 4,
    };
    let live = grid_placement(&filled, None, false, false);
    filled.mode = zz_terminal::TerminalMode::Copy {
        position: 40,
        total: 40,
        hide_position: false,
    };
    assert_eq!(grid_placement(&filled, None, false, false), live);
    filled.search = Some(zz_terminal::SearchStatus::new(0, 0));
    assert_eq!(grid_placement(&filled, None, false, false), live);
    filled.scrollbar.offset = 20;
    assert_eq!(
        grid_placement(&filled, None, false, false),
        GridPlacement {
            bottom_anchored: true,
            project_from_bottom: false,
        }
    );
}

#[test]
fn the_peek_row_comes_from_the_ring_and_never_shimmers() {
    assert_eq!(
        peek_row_source(100, 1, 100, 4, 3),
        Some(LocalRowSource::History(2))
    );
    assert_eq!(
        peek_row_source(98, 1, 100, 4, 3),
        Some(LocalRowSource::History(0))
    );
    assert_eq!(
        peek_row_source(100, 3, 100, 4, 3),
        Some(LocalRowSource::History(0))
    );
    assert_eq!(peek_row_source(97, 1, 100, 4, 3), None);
    assert_eq!(peek_row_source(100, 4, 100, 4, 3), None);
    assert_eq!(peek_row_source(100, 1, 100, 4, 0), None);
    assert_eq!(peek_row_source(0, 1, 0, 4, 3), None);
}

struct RingRows(Vec<Vec<PackedCell>>, TerminalDictionary);

impl TerminalHistorySource for RingRows {
    fn row_count(&self) -> usize {
        self.0.len()
    }

    fn row(&self, index: usize) -> Option<TerminalHistoryRow<'_>> {
        self.0.get(index).map(|cells| TerminalHistoryRow {
            cells,
            dictionary: &self.1,
            revision: 1_000 + index as u64,
        })
    }
}

#[ala::test]
fn extra_height_adds_rows_past_the_edge_the_grid_is_not_anchored_to(cx: &mut ala::TestAppContext) {
    cx.update(crate::init);
    let cx = cx.add_empty_window();
    let filled = filled_viewport(8, 4, 'x');
    let blank = TerminalViewport::blank(8, 4, zz_terminal::SessionStatus::Running);
    let revisions = [1, 2, 3, 4];
    let appearance = TerminalAppearance::default();
    cx.update(|window, cx| {
        let mut cache = RowRenderCache::default();
        let mut prepaint = |viewport: &TerminalViewport,
                            height: Pixels,
                            extra_height: Pixels,
                            window: &mut Window| {
            cache
                .prepaint(
                    TerminalRenderInput {
                        viewport,
                        row_revisions: &revisions,
                        revision_epoch: 1,
                        history: None,
                        images: None,
                        local_scroll_target: None,
                        scroll_pixel_offset: px(0.0),
                        overscroll: px(0.0),
                        scrollbar_opacity: 1.0,
                        extra_height,
                        command_output: false,
                        appearance: &appearance,
                        appearance_hash: 0,
                        text_opacity: 1.0,
                        focused: false,
                        cursor_blink_visible: true,
                        marked_text: None,
                        rows_above: None,
                        rows_below: None,
                        header: None,
                    },
                    Bounds::new(point(px(0.0), px(10.0)), size(px(400.0), height)),
                    window,
                    cx,
                )
                .geometry
        };
        let line_height = prepaint(&filled, px(400.0), px(0.0), window).line_height;
        let height = line_height * 4.0 + px(3.0);
        let anchored = prepaint(&filled, height, line_height * 2.0, window);
        let top = prepaint(&blank, height, line_height * 2.0, window);

        assert_eq!(anchored.grid.rows, 6);
        assert_eq!(anchored.grid_bounds.bottom(), px(10.0) + height);
        assert_eq!(anchored.surface_bounds.size.height, height);
        assert_eq!(top.grid.rows, 6);
        assert_eq!(top.grid_bounds.origin.y, px(10.0));
    });
}

#[ala::test]
fn a_pixel_offset_shifts_rows_down_and_peeks_the_ring_row_above(cx: &mut ala::TestAppContext) {
    cx.update(crate::init);
    let cx = cx.add_empty_window();
    let mut viewport = filled_viewport(8, 4, 'x');
    viewport.scrollbar = ScrollbarState {
        total: 40,
        offset: 36,
        len: 4,
    };
    let ring = RingRows(
        vec![vec![PackedCell::new(u32::from('h'), 0, CellWidth::Narrow); 8]; 2],
        TerminalDictionary::default(),
    );
    let revisions = [1, 2, 3, 4];
    let appearance = TerminalAppearance::default();
    cx.update(|window, cx| {
        let mut cache = RowRenderCache::default();
        let mut prepaint = |offset: Pixels, height: Pixels, window: &mut Window| {
            cache.prepaint(
                TerminalRenderInput {
                    viewport: &viewport,
                    row_revisions: &revisions,
                    revision_epoch: 1,
                    history: Some(&ring),
                    images: None,
                    local_scroll_target: None,
                    scroll_pixel_offset: offset,
                    overscroll: px(0.0),
                    scrollbar_opacity: 1.0,
                    extra_height: px(0.0),
                    command_output: false,
                    appearance: &appearance,
                    appearance_hash: 0,
                    text_opacity: 1.0,
                    focused: false,
                    cursor_blink_visible: true,
                    marked_text: None,
                    rows_above: None,
                    rows_below: None,
                    header: None,
                },
                Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), height)),
                window,
                cx,
            )
        };
        let line_height = prepaint(px(0.0), px(400.0), window).geometry.line_height;
        let height = line_height * 4.0 + px(3.0);
        let rest = prepaint(px(0.0), height, window);
        let shifted = prepaint(px(5.0), height, window);
        let rows = |paint: &PaintState| {
            paint
                .buffers
                .text
                .iter()
                .map(|row| row.origin.y)
                .collect::<Vec<_>>()
        };
        let rest_rows = rows(&rest);
        let shifted_rows = rows(&shifted);

        assert_eq!(rest.geometry.grid.rows, 4);
        assert_eq!(rest.geometry.grid_bounds.origin.y, px(3.0));
        assert_eq!(shifted.geometry.grid_bounds, rest.geometry.grid_bounds);
        assert_eq!(rest.geometry.content_offset, px(0.0));
        assert_eq!(shifted.geometry.content_offset, px(5.0));
        assert_eq!(rest_rows.len(), 4);
        assert_eq!(shifted_rows.len(), 5);
        assert_eq!(shifted_rows[0], rest_rows[0] + px(5.0) - line_height);
        for (shifted, rest) in shifted_rows[1..].iter().zip(&rest_rows) {
            assert_eq!(*shifted, *rest + px(5.0));
        }
        assert!(rest.row_clip.is_none());
        assert_eq!(
            shifted.row_clip,
            Some(Bounds::new(
                point(px(0.0), px(3.0)),
                size(px(400.0), line_height * 4.0),
            ))
        );
        assert!(shifted.scrollbar.is_some());
        assert!(cache.rows.contains_key(&1_001));
    });
}

#[ala::test]
fn rows_above_the_grid_reach_the_given_top(cx: &mut ala::TestAppContext) {
    cx.update(crate::init);
    let cx = cx.add_empty_window();
    let mut live = filled_viewport(8, 4, 'x');
    live.scrollbar = ScrollbarState {
        total: 40,
        offset: 36,
        len: 4,
    };
    let mut view = live.clone();
    view.mode = zz_terminal::TerminalMode::View {
        position: 30,
        total: 40,
    };
    let ring = RingRows(
        vec![vec![PackedCell::new(u32::from('h'), 0, CellWidth::Narrow); 8]; 6],
        TerminalDictionary::default(),
    );
    let revisions = [1, 2, 3, 4];
    let appearance = TerminalAppearance::default();
    cx.update(|window, cx| {
        let mut cache = RowRenderCache::default();
        let mut prepaint = |viewport: &TerminalViewport,
                            offset: Pixels,
                            rows_above: Option<Pixels>,
                            bounds: Bounds<Pixels>,
                            window: &mut Window| {
            cache.prepaint(
                TerminalRenderInput {
                    viewport,
                    row_revisions: &revisions,
                    revision_epoch: 1,
                    history: Some(&ring),
                    images: None,
                    local_scroll_target: None,
                    scroll_pixel_offset: offset,
                    overscroll: px(0.0),
                    scrollbar_opacity: 1.0,
                    extra_height: px(0.0),
                    command_output: false,
                    appearance: &appearance,
                    appearance_hash: 0,
                    text_opacity: 1.0,
                    focused: false,
                    cursor_blink_visible: true,
                    marked_text: None,
                    rows_above,
                    rows_below: None,
                    header: None,
                },
                bounds,
                window,
                cx,
            )
        };
        let line_height = prepaint(
            &live,
            px(0.0),
            None,
            Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(400.0))),
            window,
        )
        .geometry
        .line_height;
        let bounds = Bounds::new(
            point(px(0.0), line_height * 2.0),
            size(px(400.0), line_height * 4.0 + px(3.0)),
        );
        let rows = |paint: &PaintState| {
            paint
                .buffers
                .text
                .iter()
                .map(|row| row.origin.y)
                .collect::<Vec<_>>()
        };

        let rest = prepaint(&live, px(0.0), None, bounds, window);
        assert_eq!(rows(&rest).len(), 4);
        assert!(rest.row_clip.is_none());

        let shifted = prepaint(&live, px(5.0), Some(px(0.0)), bounds, window);
        let grid = shifted.geometry.grid_bounds;
        assert_eq!(grid.origin.y, line_height * 2.0 + px(3.0));
        let top = grid.origin.y + px(5.0);
        assert_eq!(
            rows(&shifted)[..4],
            [
                top - line_height,
                top - line_height * 2.0,
                top - line_height * 3.0,
                top,
            ]
        );
        assert_eq!(rows(&shifted).len(), 7);
        assert_eq!(
            shifted.row_clip,
            Some(Bounds::new(
                point(px(0.0), px(0.0)),
                size(px(400.0), grid.origin.y + grid.size.height),
            ))
        );

        let unshifted = prepaint(&live, px(0.0), Some(px(0.0)), bounds, window);
        assert_eq!(rows(&unshifted).len(), 7);
        let clipped = prepaint(&live, px(0.0), Some(line_height), bounds, window);
        assert_eq!(rows(&clipped).len(), 6);
        let below = prepaint(&live, px(0.0), Some(grid.origin.y), bounds, window);
        assert_eq!(rows(&below).len(), 4);
        assert!(below.row_clip.is_none());
        let unanchored = prepaint(&view, px(0.0), Some(px(0.0)), bounds, window);
        assert_eq!(rows(&unanchored).len(), 4);
    });
}

#[ala::test]
fn the_header_scrim_shows_only_while_scrolled_back(cx: &mut ala::TestAppContext) {
    cx.update(crate::init);
    let cx = cx.add_empty_window();
    let mut viewport = filled_viewport(8, 4, 'x');
    let ring = RingRows(
        vec![vec![PackedCell::new(u32::from('h'), 0, CellWidth::Narrow); 8]; 6],
        TerminalDictionary::default(),
    );
    let revisions = [1, 2, 3, 4];
    let appearance = TerminalAppearance::default();
    let header = crate::pane::HeaderScrim {
        band: Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(34.0))),
        ground: ala::hsla(0.0, 0.0, 0.1, 1.0),
    };
    let bounds = Bounds::new(point(px(6.0), px(40.0)), size(px(388.0), px(300.0)));
    cx.update(|window, cx| {
        let mut cache = RowRenderCache::default();
        let mut scrim_at = |offset: u32, window: &mut Window| {
            viewport.scrollbar = ScrollbarState {
                total: 40,
                offset,
                len: 4,
            };
            let paint = cache.prepaint(
                TerminalRenderInput {
                    viewport: &viewport,
                    row_revisions: &revisions,
                    revision_epoch: 1,
                    history: Some(&ring),
                    images: None,
                    local_scroll_target: None,
                    scroll_pixel_offset: px(0.0),
                    overscroll: px(0.0),
                    scrollbar_opacity: 1.0,
                    extra_height: px(0.0),
                    command_output: false,
                    appearance: &appearance,
                    appearance_hash: 0,
                    text_opacity: 1.0,
                    focused: false,
                    cursor_blink_visible: true,
                    marked_text: None,
                    rows_above: None,
                    rows_below: None,
                    header: Some(header),
                },
                bounds,
                window,
                cx,
            );
            (paint.header_scrim(), paint.rows_above_top)
        };
        assert_eq!(scrim_at(36, window), (None, None));
        assert_eq!(scrim_at(35, window), (Some(header), Some(px(0.0))));
        assert_eq!(scrim_at(36, window), (None, None));
    });
}

#[ala::test]
fn rows_below_the_grid_show_newer_rows_while_scrolled_back(cx: &mut ala::TestAppContext) {
    cx.update(crate::init);
    let cx = cx.add_empty_window();
    let mut live = filled_viewport(8, 4, 'x');
    live.scrollbar = ScrollbarState {
        total: 40,
        offset: 36,
        len: 4,
    };
    let ring = RingRows(
        vec![vec![PackedCell::new(u32::from('h'), 0, CellWidth::Narrow); 8]; 6],
        TerminalDictionary::default(),
    );
    let revisions = [1, 2, 3, 4];
    let appearance = TerminalAppearance::default();
    cx.update(|window, cx| {
        let mut cache = RowRenderCache::default();
        let mut prepaint = |target: Option<u32>,
                            rows_below: Option<Pixels>,
                            bounds: Bounds<Pixels>,
                            window: &mut Window| {
            cache.prepaint(
                TerminalRenderInput {
                    viewport: &live,
                    row_revisions: &revisions,
                    revision_epoch: 1,
                    history: Some(&ring),
                    images: None,
                    local_scroll_target: target,
                    scroll_pixel_offset: px(0.0),
                    overscroll: px(0.0),
                    scrollbar_opacity: 1.0,
                    extra_height: px(0.0),
                    command_output: false,
                    appearance: &appearance,
                    appearance_hash: 0,
                    text_opacity: 1.0,
                    focused: false,
                    cursor_blink_visible: true,
                    marked_text: None,
                    rows_above: None,
                    rows_below,
                    header: None,
                },
                bounds,
                window,
                cx,
            )
        };
        let line_height = prepaint(
            None,
            None,
            Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(400.0))),
            window,
        )
        .geometry
        .line_height;
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), line_height * 4.0));
        let bottom = line_height * 10.0;
        let rows = |paint: &PaintState| {
            let mut rows = paint
                .buffers
                .text
                .iter()
                .map(|row| row.origin.y)
                .collect::<Vec<_>>();
            rows.sort_by(|a, b| a.partial_cmp(b).unwrap());
            rows
        };

        let back = prepaint(Some(33), Some(bottom), bounds, window);
        let grid = back.geometry.grid_bounds;
        let grid_bottom = grid.origin.y + grid.size.height;
        assert_eq!(
            rows(&back)[4..],
            [
                grid_bottom,
                grid_bottom + line_height,
                grid_bottom + line_height * 2.0,
            ]
        );
        assert_eq!(back.row_clip.map(|clip| clip.bottom()), Some(bottom));

        let live_bottom = prepaint(None, Some(bottom), bounds, window);
        assert_eq!(rows(&live_bottom).len(), 4);
        assert!(live_bottom.row_clip.is_none());
    });
}

struct KeptRows(RingRows, RingRows);

impl TerminalHistorySource for KeptRows {
    fn row_count(&self) -> usize {
        self.0.row_count()
    }

    fn row(&self, index: usize) -> Option<TerminalHistoryRow<'_>> {
        self.0.row(index)
    }

    fn below_count(&self) -> usize {
        self.1.row_count()
    }

    fn below(&self, index: usize) -> Option<TerminalHistoryRow<'_>> {
        self.1.row(index)
    }
}

#[ala::test]
fn a_settled_view_draws_the_kept_rows_below_it(cx: &mut ala::TestAppContext) {
    cx.update(crate::init);
    let cx = cx.add_empty_window();
    let mut view = filled_viewport(8, 4, 'x');
    view.scrollbar = ScrollbarState {
        total: 40,
        offset: 30,
        len: 4,
    };
    view.mode = zz_terminal::TerminalMode::View {
        position: 30,
        total: 40,
    };
    let row = |glyph: char| vec![PackedCell::new(u32::from(glyph), 0, CellWidth::Narrow); 8];
    let kept = KeptRows(
        RingRows(vec![row('h'); 6], TerminalDictionary::default()),
        RingRows(vec![row('b'); 2], TerminalDictionary::default()),
    );
    let revisions = [1, 2, 3, 4];
    let appearance = TerminalAppearance::default();
    cx.update(|window, cx| {
        let mut cache = RowRenderCache::default();
        let mut prepaint = |rows_below: Option<Pixels>, bounds: Bounds<Pixels>| {
            cache.prepaint(
                TerminalRenderInput {
                    viewport: &view,
                    row_revisions: &revisions,
                    revision_epoch: 1,
                    history: Some(&kept),
                    images: None,
                    local_scroll_target: None,
                    scroll_pixel_offset: px(0.0),
                    overscroll: px(0.0),
                    scrollbar_opacity: 1.0,
                    extra_height: px(0.0),
                    command_output: false,
                    appearance: &appearance,
                    appearance_hash: 0,
                    text_opacity: 1.0,
                    focused: false,
                    cursor_blink_visible: true,
                    marked_text: None,
                    rows_above: None,
                    rows_below,
                    header: None,
                },
                bounds,
                window,
                cx,
            )
        };
        let line_height = prepaint(
            None,
            Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(400.0))),
        )
        .geometry
        .line_height;
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), line_height * 4.0));
        let settled = prepaint(Some(line_height * 10.0), bounds);
        let grid = settled.geometry.grid_bounds;
        let grid_bottom = grid.origin.y + grid.size.height;
        let mut rows = settled
            .buffers
            .text
            .iter()
            .map(|row| row.origin.y)
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(rows[4..], [grid_bottom, grid_bottom + line_height]);
    });
}
