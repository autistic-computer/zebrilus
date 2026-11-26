use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RecentreTarget {
    Middle,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectionMode {
    Cell,
    Word,
    Line,
}

pub(super) type CopyModeSlot = Option<Box<CopyModeState>>;
pub(super) type SearchSlot = Option<Box<SearchState>>;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FrozenModeKind {
    Copy,
    View,
}

impl HoverLink {
    pub(super) fn contains(&self, point: PointerCellEvent) -> bool {
        self.row == point.row && (self.start..self.end).contains(&point.column)
    }
}

#[derive(Debug)]
struct PendingPasteWindow {
    token: u64,
    deadline: Instant,
    baseline: HashMap<u32, usize>,
}

#[derive(Default)]
pub(super) struct PastedImageBindings {
    pending: VecDeque<PendingPasteWindow>,
    bound_numbers: HashSet<u32>,
}

impl PastedImageBindings {
    pub(super) fn open(
        &mut self,
        terminal: &Terminal<'_, '_>,
        token: u64,
        now: Instant,
    ) -> Result<(), WorkerError> {
        self.pending.push_back(PendingPasteWindow {
            token,
            deadline: now + PENDING_PASTE_WINDOW,
            baseline: image_placeholder_occurrences(terminal)?,
        });
        Ok(())
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        self.pending.front().map(|pending| pending.deadline)
    }

    pub(super) fn expire(&mut self, now: Instant) -> Vec<u64> {
        let mut expired = Vec::new();
        while self
            .pending
            .front()
            .is_some_and(|pending| pending.deadline <= now)
        {
            if let Some(pending) = self.pending.pop_front() {
                expired.push(pending.token);
            }
        }
        expired
    }

    pub(super) fn observe(
        &mut self,
        terminal: &Terminal<'_, '_>,
    ) -> Result<Option<(u64, u32)>, WorkerError> {
        let Some(oldest) = self.pending.front() else {
            return Ok(None);
        };
        let current = image_placeholder_occurrences(terminal)?;
        let number = current
            .iter()
            .filter_map(|(number, count)| {
                (*count > oldest.baseline.get(number).copied().unwrap_or(0)).then_some(*number)
            })
            .max();
        let Some(number) = number else {
            return Ok(None);
        };
        let token = self
            .pending
            .pop_front()
            .expect("the observed pending paste remains at the front")
            .token;
        for pending in &mut self.pending {
            pending.baseline.clone_from(&current);
        }
        self.bound_numbers.insert(number);
        Ok(Some((token, number)))
    }

    pub(super) fn unbind(&mut self, number: u32) -> bool {
        self.bound_numbers.remove(&number)
    }

    pub(super) fn bound_numbers(&self) -> &HashSet<u32> {
        &self.bound_numbers
    }
}

#[derive(Default)]
pub(super) enum ViewportAnchor {
    #[default]
    FollowBottom,
    Pinned(TrackedGridRef),
}

pub(super) type ActiveTerminalViews = HashMap<TerminalViewId, Box<TerminalViewState>>;
pub(super) type InactiveTerminalViews = HashMap<TerminalViewId, Box<TerminalViewState>>;
pub(super) type InternHashMap<K, V> = HashMap<K, V, foldhash::fast::RandomState>;

impl Default for TerminalViewState {
    fn default() -> Self {
        Self::for_screen(Screen::Primary)
    }
}

impl std::ops::Deref for TerminalViewState {
    type Target = TerminalScreenViewState;

    fn deref(&self) -> &Self::Target {
        self.active()
    }
}

impl std::ops::DerefMut for TerminalViewState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.active_mut()
    }
}

impl TerminalViewState {
    pub(super) fn for_screen(screen: Screen) -> Self {
        Self {
            screen,
            primary: TerminalScreenViewState::default(),
            alternate: TerminalScreenViewState::default(),
        }
    }

    pub(super) fn active(&self) -> &TerminalScreenViewState {
        match self.screen {
            Screen::Primary => &self.primary,
            Screen::Alternate => &self.alternate,
        }
    }

    pub(super) fn active_mut(&mut self) -> &mut TerminalScreenViewState {
        match self.screen {
            Screen::Primary => &mut self.primary,
            Screen::Alternate => &mut self.alternate,
        }
    }

    pub(super) fn switch_screen(&mut self, screen: Screen) -> bool {
        if self.screen == screen {
            return false;
        }
        self.screen = screen;
        true
    }

    pub(super) fn invalidate_layout(&mut self) {
        invalidate_screen_layout(&mut self.primary);
        invalidate_screen_layout(&mut self.alternate);
    }

    pub(super) fn screen_mut(&mut self, screen: Screen) -> &mut TerminalScreenViewState {
        match screen {
            Screen::Primary => &mut self.primary,
            Screen::Alternate => &mut self.alternate,
        }
    }

    pub(super) fn note_output(&mut self, screen: Screen) {
        if self.active().copy_mode.is_some() {
            let active = self.active_mut();
            active.unseen_output = active.unseen_output.saturating_add(1);
            active.hover_link = None;
            if screen != self.screen {
                invalidate_screen_layout(self.screen_mut(screen));
            }
            return;
        }
        self.invalidate_layout();
        let view = self.screen_mut(screen);
        if matches!(view.viewport, ViewportAnchor::Pinned(_)) {
            view.unseen_output = view.unseen_output.saturating_add(1);
        }
    }
}

pub(super) fn clear_pasted_image_hover(view: &mut TerminalViewState, number: u32) -> bool {
    let uri = format!("{IMAGE_PLACEHOLDER_SCHEME}://{number}");
    let active = view.screen;
    let mut active_changed = false;
    for (screen, state) in [
        (Screen::Primary, &mut view.primary),
        (Screen::Alternate, &mut view.alternate),
    ] {
        if state
            .hover_link
            .as_ref()
            .is_some_and(|link| link.uri == uri)
        {
            state.hover_link = None;
            active_changed |= screen == active;
        }
    }
    active_changed
}

fn invalidate_screen_layout(view: &mut TerminalScreenViewState) {
    if view.copy_mode.is_some() {
        view.hover_link = None;
        return;
    }
    view.search_snapshot = None;
    view.hover_link = None;
    if let Some(search) = view.search.as_mut() {
        search.matches.clear();
        search.current = None;
        search.pending =
            !search.query.text.is_empty() && search.query.text.len() <= MAX_SEARCH_QUERY_BYTES;
        if search.pending {
            search.invalid_pattern = false;
        }
    }
}

const SHARED_STYLES_DIRTY: u8 = 1 << 0;
const SHARED_GRAPHEMES_DIRTY: u8 = 1 << 1;
const SHARED_ALL_DIRTY: u8 = SHARED_STYLES_DIRTY | SHARED_GRAPHEMES_DIRTY;

fn adaptive_dictionary_limit(
    minimum: usize,
    viewport_headroom: usize,
    working_set: usize,
    maximum: usize,
) -> usize {
    minimum
        .max(viewport_headroom)
        .max(working_set.saturating_mul(2))
        .min(maximum)
}

impl ViewportDictionary {
    pub(super) fn reset_live(&mut self, style: PackedStyle, palette: &[RgbColor; 256]) {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.mode_revision = None;
        self.mode_viewport = None;
        self.default_style = Some(style);
        self.palette = Some(Box::new(*palette));
        self.last_style_id = 0;
        self.styles.clear();
        self.styles.push(style);
        self.style_ids.clear();
        self.style_ids.insert(style, 0);
        self.grapheme_ids.clear();
        self.grapheme_offsets.clear();
        self.grapheme_offsets.push(0);
        self.grapheme_bytes.clear();
        self.shared_dirty = SHARED_ALL_DIRTY;
        self.style_compaction_limit = 0;
        self.grapheme_compaction_limit = 0;
        self.grapheme_byte_compaction_limit = 0;
        self.style_overflowed = false;
        self.grapheme_overflowed = false;
    }

    pub(super) fn should_compact_live(&self) -> bool {
        self.style_overflowed
            || self.grapheme_overflowed
            || (self.style_compaction_limit != 0 && self.styles.len() > self.style_compaction_limit)
            || (self.grapheme_compaction_limit != 0
                && self.grapheme_offsets.len().saturating_sub(1) > self.grapheme_compaction_limit)
            || (self.grapheme_byte_compaction_limit != 0
                && self.grapheme_bytes.len() > self.grapheme_byte_compaction_limit)
    }

    pub(super) fn tune_live_compaction_limits(&mut self, cell_count: usize) {
        self.style_compaction_limit = adaptive_dictionary_limit(
            MIN_STYLE_COMPACTION_LIMIT,
            cell_count.saturating_mul(2),
            self.styles.len(),
            MAX_VIEWPORT_STYLE_COUNT,
        );
        self.grapheme_compaction_limit = adaptive_dictionary_limit(
            MIN_GRAPHEME_COMPACTION_LIMIT,
            cell_count.saturating_mul(2),
            self.grapheme_offsets.len().saturating_sub(1),
            MAX_VIEWPORT_GRAPHEME_COUNT,
        );
        self.grapheme_byte_compaction_limit = adaptive_dictionary_limit(
            MIN_GRAPHEME_BYTE_COMPACTION_LIMIT,
            cell_count.saturating_mul(64),
            self.grapheme_bytes.len(),
            MAX_VIEWPORT_GRAPHEME_BYTES,
        );
    }

    pub(super) fn acquire_cell_plane(&mut self, len: usize, preserve: bool) -> Arc<[PackedCell]> {
        let reusable = self
            .cell_pool
            .iter_mut()
            .position(|plane| plane.len() == len && Arc::get_mut(plane).is_some());
        let (mut plane, blank) = reusable.map_or_else(
            || (std::iter::repeat_n(PackedCell::EMPTY, len).collect(), true),
            |index| (self.cell_pool.swap_remove(index), false),
        );
        let cells = Arc::get_mut(&mut plane).expect("new or uniquely retained cell plane");
        if preserve {
            cells.copy_from_slice(&self.shared_cells);
        } else if !blank {
            cells.fill(PackedCell::EMPTY);
        }
        plane
    }

    pub(super) fn retain_cell_plane(&mut self, plane: Arc<[PackedCell]>) {
        if plane.is_empty() {
            return;
        }
        if self.cell_pool.len() == RETAINED_CELL_PLANES {
            let discard = self
                .cell_pool
                .iter()
                .position(|plane| Arc::strong_count(plane) > 1)
                .unwrap_or(0);
            self.cell_pool.swap_remove(discard);
        }
        self.cell_pool.push(plane);
    }

    pub(super) fn release_pools(&mut self) {
        self.cell_pool.clear();
        self.overlay_pool.clear();
        self.row_copy = CellsCopy::new();
        self.row_styles = Vec::new();
        self.overlay_scratch = Vec::new();
    }

    pub(super) fn commit_cell_plane(&mut self, plane: Arc<[PackedCell]>) {
        let previous = std::mem::replace(&mut self.shared_cells, plane);
        if previous.len() == self.shared_cells.len() {
            self.retain_cell_plane(previous);
        } else {
            self.cell_pool
                .retain(|pooled| pooled.len() == self.shared_cells.len());
        }
    }

    fn acquire_overlay_plane(&mut self, overlays: &[OverlaySpan]) -> Arc<[OverlaySpan]> {
        let reusable = self
            .overlay_pool
            .iter_mut()
            .position(|plane| plane.len() == overlays.len() && Arc::get_mut(plane).is_some());
        reusable.map_or_else(
            || Arc::from(overlays),
            |index| {
                let mut plane = self.overlay_pool.swap_remove(index);
                Arc::get_mut(&mut plane)
                    .expect("uniquely retained overlay plane")
                    .copy_from_slice(overlays);
                plane
            },
        )
    }

    fn retain_overlay_plane(&mut self, plane: Arc<[OverlaySpan]>) {
        if plane.is_empty() {
            return;
        }
        if self.overlay_pool.len() == RETAINED_OVERLAY_PLANES {
            let discard = self
                .overlay_pool
                .iter()
                .position(|plane| Arc::strong_count(plane) > 1)
                .unwrap_or(0);
            self.overlay_pool.swap_remove(discard);
        }
        self.overlay_pool.push(plane);
    }

    pub(super) fn finish_overlays(&mut self, mut overlays: Vec<OverlaySpan>) -> Arc<[OverlaySpan]> {
        if self.shared_overlays.as_ref() != overlays.as_slice() {
            let next = self.acquire_overlay_plane(&overlays);
            let previous = std::mem::replace(&mut self.shared_overlays, next);
            self.retain_overlay_plane(previous);
        }
        overlays.clear();
        self.overlay_scratch = overlays;
        Arc::clone(&self.shared_overlays)
    }

    pub(super) fn shared_presentation(
        &mut self,
        title: &str,
        working_directory: Option<&str>,
        hovered_uri: Option<&str>,
    ) -> Arc<TerminalPresentation> {
        if self.shared_presentation.title.as_ref() != title
            || self.shared_presentation.working_directory.as_deref() != working_directory
            || self.shared_presentation.hovered_uri.as_deref() != hovered_uri
        {
            let title = if self.shared_presentation.title.as_ref() == title {
                Arc::clone(&self.shared_presentation.title)
            } else {
                Arc::from(title)
            };
            let working_directory = match (
                self.shared_presentation.working_directory.as_ref(),
                working_directory,
            ) {
                (Some(previous), Some(next)) if previous.as_ref() == next => {
                    Some(Arc::clone(previous))
                }
                (_, Some(next)) => Some(Arc::from(next)),
                (_, None) => None,
            };
            let hovered_uri = match (self.shared_presentation.hovered_uri.as_ref(), hovered_uri) {
                (Some(previous), Some(next)) if previous.as_ref() == next => {
                    Some(Arc::clone(previous))
                }
                (_, Some(next)) => Some(Arc::from(next)),
                (_, None) => None,
            };
            self.shared_presentation = Arc::new(TerminalPresentation::new(
                title,
                working_directory,
                hovered_uri,
            ));
        }
        Arc::clone(&self.shared_presentation)
    }

    pub(super) fn ensure_default(&mut self, style: PackedStyle, palette: &[RgbColor; 256]) {
        if self.mode_revision.is_none()
            && self.default_style == Some(style)
            && self.palette.as_deref() == Some(palette)
        {
            return;
        }
        self.reset_live(style, palette);
    }

    pub(super) fn mode_cells(
        &mut self,
        revision: &ModeRevision,
        offset: u32,
        extent: (u16, u16),
    ) -> Arc<[PackedCell]> {
        if self.mode_revision != Some(revision.id) {
            self.generation = self.generation.wrapping_add(1).max(1);
            self.mode_revision = Some(revision.id);
            self.mode_viewport = None;
            self.default_style = None;
            self.palette = None;
            self.last_style_id = 0;
            self.styles.clear();
            self.style_ids.clear();
            self.grapheme_ids.clear();
            self.grapheme_offsets.clear();
            self.grapheme_bytes.clear();
            self.shared_dirty = 0;
            self.style_compaction_limit = 0;
            self.grapheme_compaction_limit = 0;
            self.grapheme_byte_compaction_limit = 0;
            self.style_overflowed = false;
            self.grapheme_overflowed = false;
        }
        if self.mode_viewport
            != Some((
                revision.id,
                offset,
                revision.dictionary_generation(),
                extent,
            ))
        {
            self.shared_cells = revision.viewport_cells(offset, extent);
            self.mode_viewport = Some((
                revision.id,
                offset,
                revision.dictionary_generation(),
                extent,
            ));
        }
        Arc::clone(&self.shared_cells)
    }

    pub(super) fn intern_style(&mut self, style: PackedStyle) -> u16 {
        if self.styles.get(usize::from(self.last_style_id)) == Some(&style) {
            return self.last_style_id;
        }
        if let Some(id) = self.style_ids.get(&style) {
            self.last_style_id = *id;
            return *id;
        }
        if self.styles.len() >= MAX_VIEWPORT_STYLE_COUNT {
            if !self.style_overflowed {
                log::warn!(
                    "terminal viewport style working set exceeds {MAX_VIEWPORT_STYLE_COUNT} entries; using the default style for excess cells"
                );
            }
            self.style_overflowed = true;
            return 0;
        }
        let id = u16::try_from(self.styles.len()).expect("style table is bounded to u16 IDs");
        self.styles.push(style);
        self.style_ids.insert(style, id);
        self.last_style_id = id;
        self.shared_dirty |= SHARED_STYLES_DIRTY;
        id
    }

    pub(super) fn encode_glyph(&mut self, text: &str) -> u32 {
        let mut characters = text.chars();
        let Some(first) = characters.next() else {
            return 0;
        };
        if characters.next().is_none() {
            return u32::from(first);
        }
        if let Some(index) = self.grapheme_ids.get(text) {
            return GRAPHEME_TABLE_BIT | *index;
        }

        let index = self.grapheme_offsets.len().saturating_sub(1);
        let end = self.grapheme_bytes.len().checked_add(text.len());
        if index >= MAX_VIEWPORT_GRAPHEME_COUNT
            || end.is_none_or(|end| end > MAX_VIEWPORT_GRAPHEME_BYTES)
        {
            if !self.grapheme_overflowed {
                log::warn!(
                    "terminal viewport grapheme working set exceeds its protocol budget; using the first scalar for excess cells"
                );
            }
            self.grapheme_overflowed = true;
            return u32::from(first);
        }
        let index = u32::try_from(index).expect("grapheme table is bounded to u32 IDs");
        let end = u32::try_from(end.expect("checked above"))
            .expect("grapheme byte arena is bounded to u32 offsets");
        self.grapheme_bytes.extend_from_slice(text.as_bytes());
        self.grapheme_offsets.push(end);
        self.grapheme_ids.insert(text.to_owned(), index);
        self.shared_dirty |= SHARED_GRAPHEMES_DIRTY;
        GRAPHEME_TABLE_BIT | index
    }

    pub(super) fn shared_dictionary(&mut self) -> Arc<TerminalDictionary> {
        if self.shared_dirty != 0 {
            let styles = if self.shared_dirty & SHARED_STYLES_DIRTY != 0 {
                Arc::from(self.styles.as_slice())
            } else {
                Arc::clone(&self.shared_dictionary.styles)
            };
            let (grapheme_offsets, grapheme_bytes) =
                if self.shared_dirty & SHARED_GRAPHEMES_DIRTY != 0 {
                    (
                        Arc::from(self.grapheme_offsets.as_slice()),
                        Arc::from(self.grapheme_bytes.as_slice()),
                    )
                } else {
                    (
                        Arc::clone(&self.shared_dictionary.grapheme_offsets),
                        Arc::clone(&self.shared_dictionary.grapheme_bytes),
                    )
                };
            self.shared_dictionary = Arc::new(TerminalDictionary::from_shared(
                styles,
                grapheme_offsets,
                grapheme_bytes,
            ));
        }
        self.shared_dirty = 0;
        Arc::clone(&self.shared_dictionary)
    }
}

static NEXT_GENERATION_SEED: AtomicU64 = AtomicU64::new(0);
const GENERATION_SEED_STRIDE: u64 = 1 << 40;

impl ViewportGenerations {
    pub(super) fn new() -> Result<Self, WorkerError> {
        let seed = NEXT_GENERATION_SEED.fetch_add(GENERATION_SEED_STRIDE, Ordering::Relaxed);
        Ok(Self {
            content: seed,
            view: seed,
            kitty: Some(KittyGraphicsState::new()?),
        })
    }
}

pub(super) struct KittyGraphicsState {
    iterator: PlacementIterator<'static>,
    last_generation: u64,
    last_extraction_empty: bool,
    oversized_images: HashSet<(u32, u64)>,
}

impl KittyGraphicsState {
    pub(super) fn new() -> Result<Self, WorkerError> {
        Ok(Self {
            iterator: PlacementIterator::new()?,
            last_generation: 0,
            last_extraction_empty: true,
            oversized_images: HashSet::new(),
        })
    }

    pub(super) fn placements(
        &mut self,
        terminal: &Terminal<'_, '_>,
        scrollbar: ScrollbarState,
    ) -> Result<Arc<[KittyPlacement]>, WorkerError> {
        let graphics = terminal.kitty_graphics()?;
        let generation = graphics.generation()?;
        if generation == self.last_generation && self.last_extraction_empty {
            return Ok(Arc::from([]));
        }

        let mut iterator = self.iterator.update(&graphics)?;
        let mut saw_stored_placement = false;
        let mut placements = Vec::new();
        while let Some(placement) = iterator.advance() {
            let image_id = placement.image_id()?;
            if placement.is_virtual()? || image_id == 0 {
                continue;
            }
            saw_stored_placement = true;
            let Some(image) = graphics.image(image_id) else {
                continue;
            };
            let image_generation = image.generation()?;
            let image_width = image.width()?;
            let image_height = image.height()?;
            let z = placement.z()?;
            let info = placement.placement_render_info(&image, terminal)?;
            if !info.viewport_visible {
                continue;
            }
            if info.grid_cols == 0
                || info.grid_rows == 0
                || info.pixel_width == 0
                || info.pixel_height == 0
                || image_generation == 0
            {
                continue;
            }
            let source_rect = (
                info.source_x,
                info.source_y,
                info.source_width,
                info.source_height,
            );
            let source_rect =
                (source_rect != (0, 0, image_width, image_height)).then_some(source_rect);
            let absolute_row = if info.viewport_row < 0 {
                u64::from(scrollbar.offset)
                    .saturating_sub(u64::from(info.viewport_row.unsigned_abs()))
            } else {
                u64::from(scrollbar.offset)
                    .saturating_add(u64::try_from(info.viewport_row).unwrap_or_default())
            };
            placements.push(KittyPlacement {
                image_id,
                image_generation,
                layer: if z < i32::MIN / 2 {
                    KittyLayer::BelowBg
                } else if z < 0 {
                    KittyLayer::BelowText
                } else {
                    KittyLayer::AboveText
                },
                viewport_col: info.viewport_col,
                viewport_row: info.viewport_row,
                absolute_row,
                cell_offset_x: placement.x_offset()?,
                cell_offset_y: placement.y_offset()?,
                grid_cols: info.grid_cols,
                grid_rows: info.grid_rows,
                pixel_width: info.pixel_width,
                pixel_height: info.pixel_height,
                source_rect,
            });
            if placements.len() == MAX_KITTY_PLACEMENTS {
                break;
            }
        }
        self.last_generation = generation;
        self.last_extraction_empty = !saw_stored_placement;
        Ok(placements.into())
    }

    pub(super) fn image(
        &mut self,
        terminal: &Terminal<'_, '_>,
        image_id: u32,
    ) -> Result<Option<KittyImage>, WorkerError> {
        let graphics = terminal.kitty_graphics()?;
        let Some(image) = graphics.image(image_id) else {
            return Ok(None);
        };
        let width = image.width()?;
        let height = image.height()?;
        let generation = image.generation()?;
        let format = image.format()?;
        let Some(input) = image.data()? else {
            return Ok(None);
        };
        let Some(output_len) = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
        else {
            return Ok(None);
        };
        if output_len == 0 || output_len > MAX_KITTY_IMAGE_BYTES {
            if self.oversized_images.insert((image_id, generation)) {
                log::warn!(
                    "refusing Kitty image {image_id} generation {generation}: decoded BGRA size {output_len} exceeds {MAX_KITTY_IMAGE_BYTES} bytes",
                );
            }
            return Ok(None);
        }
        let Some(bytes_per_pixel) = kitty_bytes_per_pixel(format) else {
            return Ok(None);
        };
        let expected_input = output_len
            .checked_div(4)
            .and_then(|pixels| pixels.checked_mul(bytes_per_pixel));
        if expected_input != Some(input.len()) {
            log::warn!(
                "refusing malformed Kitty image {image_id} generation {generation}: format={format:?} bytes={} expected={expected_input:?}",
                input.len(),
            );
            return Ok(None);
        }
        let bgra = kitty_to_premultiplied_bgra(input, format, output_len);
        Ok(Some(KittyImage {
            image_id,
            generation,
            width,
            height,
            bgra,
        }))
    }

    pub(super) fn image_generation(
        terminal: &Terminal<'_, '_>,
        image_id: u32,
    ) -> Result<Option<u64>, WorkerError> {
        let graphics = terminal.kitty_graphics()?;
        let Some(image) = graphics.image(image_id) else {
            return Ok(None);
        };
        let generation = image.generation()?;
        Ok((generation != 0).then_some(generation))
    }
}

pub(super) fn configure_kitty_storage(terminal: &mut Terminal<'_, '_>) -> Result<(), WorkerError> {
    terminal.set_kitty_image_storage_limit(MAX_KITTY_IMAGE_BYTES as u64)?;
    Ok(())
}

fn kitty_bytes_per_pixel(format: ImageFormat) -> Option<usize> {
    match format {
        ImageFormat::Gray => Some(1),
        ImageFormat::GrayAlpha => Some(2),
        ImageFormat::Rgb => Some(3),
        ImageFormat::Rgba => Some(4),
        _ => None,
    }
}

fn premultiply(component: u8, alpha: u8) -> u8 {
    u8::try_from((u16::from(component) * u16::from(alpha) + 127) / 255).unwrap_or(u8::MAX)
}

pub(super) fn kitty_to_premultiplied_bgra(
    input: &[u8],
    format: ImageFormat,
    output_len: usize,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(output_len);
    match format {
        ImageFormat::Gray => {
            for gray in input {
                output.extend_from_slice(&[*gray, *gray, *gray, u8::MAX]);
            }
        }
        ImageFormat::GrayAlpha => {
            for pixel in input.as_chunks::<2>().0 {
                let gray = premultiply(pixel[0], pixel[1]);
                output.extend_from_slice(&[gray, gray, gray, pixel[1]]);
            }
        }
        ImageFormat::Rgb => {
            for pixel in input.as_chunks::<3>().0 {
                output.extend_from_slice(&[pixel[2], pixel[1], pixel[0], u8::MAX]);
            }
        }
        ImageFormat::Rgba => {
            for pixel in input.as_chunks::<4>().0 {
                output.extend_from_slice(&[
                    premultiply(pixel[2], pixel[3]),
                    premultiply(pixel[1], pixel[3]),
                    premultiply(pixel[0], pixel[3]),
                    pixel[3],
                ]);
            }
        }
        _ => {}
    }
    debug_assert_eq!(output.len(), output_len);
    output
}

struct KittyPngDecoder;

impl graphics::DecodePng for KittyPngDecoder {
    fn decode_png<'alloc>(
        &mut self,
        allocator: &'alloc Allocator<'_>,
        encoded: &[u8],
    ) -> Option<DecodedImage<'alloc>> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            decode_kitty_png(allocator, encoded)
        }))
        .ok()
        .flatten()
    }
}

pub(super) fn install_kitty_png_decoder() {
    if let Err(error) = graphics::set_png_decoder(Some(Box::new(KittyPngDecoder))) {
        log::warn!("could not install the Kitty PNG decoder on the terminal worker: {error}");
    }
}

fn decode_kitty_png<'alloc>(
    allocator: &'alloc Allocator<'_>,
    encoded: &[u8],
) -> Option<DecodedImage<'alloc>> {
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_KITTY_IMAGE_BYTES as u64);
    let max_dimension = u32::try_from(MAX_KITTY_IMAGE_BYTES / 4).ok()?;
    limits.max_image_width = Some(max_dimension);
    limits.max_image_height = Some(max_dimension);
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(encoded), image::ImageFormat::Png);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let (width, height) = (decoded.width(), decoded.height());
    let expected = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(4)?;
    if expected == 0 || expected > MAX_KITTY_IMAGE_BYTES {
        return None;
    }
    let decoded = decoded.into_rgba8().into_raw();
    if decoded.len() != expected {
        return None;
    }
    let mut data = Bytes::new_with_alloc(allocator, decoded.len()).ok()?;
    data.copy_from_slice(&decoded);
    Some(DecodedImage {
        width,
        height,
        data,
    })
}
