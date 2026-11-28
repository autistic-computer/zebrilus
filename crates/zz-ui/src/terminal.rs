use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, hash_map::Entry},
    hash::{BuildHasherDefault, Hasher},
    ops::Range,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use crate::scroll::{
    MIN_THUMB_SIZE, THUMB_INSET, THUMB_WIDTH, fade_frame, thumb_color, thumb_radius,
};
use crate::{ActiveTheme as _, Colorize as _, oklch, to_oklch};
use ala::{
    App, BorderStyle, Bounds, ContentMask, Corners, Font, FontFallbacks, FontFeatures,
    FontSmoothing, FontStyle, FontWeight, GlyphRasterData, GlyphRenderOptions, Hsla, PaintQuad,
    Path, PathBuilder, Pixels, Point, RenderImage, Rgba, ShapedLine, StrikethroughStyle, TextAlign,
    TextRun, UnderlineStyle, Window, fill, font, point, px, quad, size,
};
use smallvec::SmallVec;
use zz_client::local_scroll::{
    GridPlacement, LocalRowSource, RowProjection, grid_placement, kitty_display_row,
    local_live_projection, local_row_source, peek_row_source, scrolled_back,
};
#[cfg(test)]
use zz_client::terminal_color::is_decorative_character;
use zz_client::terminal_color::{
    contrast_ratio, decorative_glyph, ensure_minimum_contrast, faint_foreground, oklab,
};
pub use zz_client::terminal_view::{CURSOR_BLINK_TIMEOUT, cursor_should_blink};
use zz_terminal::{
    AppearanceColor, CellWidth, Color, Cursor, CursorStyle, GRAPHEME_TABLE_BIT, Glyph, KittyLayer,
    LocalColors, OVERLAY_RECTANGLE, OverlayKind, PackedCell, PackedStyle, ScrollbarState,
    TerminalAppearance, TerminalDictionary, TerminalViewport,
    UnderlineStyle as TerminalUnderlineStyle,
};

use penna::{
    ThemeMode,
    foundation::interface_theme::{BaseColors, InterfaceTheme, ModeLock, Tint},
};

mod box_drawing;
mod copy_flash;
mod cursor;
mod kitty_images;
mod look;
mod metrics;
mod scrollbar;
mod selection;
mod text_shaping;

pub use copy_flash::{CopyFlash, CopyFlashTracker, paint_copy_flash};
pub use look::{
    TerminalLook, terminal_background, terminal_look, theme_terminal_colors, wear_terminal_look,
};
pub use metrics::{terminal_cell_size, terminal_grid_size};
pub use scrollbar::{ScrollbarFade, scroll_fraction, scrollbar_strip_hit};
pub use selection::selection_autoscroll_lines;
pub use text_shaping::terminal_font_for_style;

use box_drawing::{
    box_stroke_width, connected_light_box_drawing, connected_solid_block_element,
    push_box_drawing_connectors, push_solid_blocks,
};
use cursor::{cursor_cell, cursor_glyph_line, cursor_quad, cursor_visible_for_paint};
use kitty_images::{collect_kitty_images, paint_kitty_images};
use look::{appearance_color, color, resolved_foreground};
use metrics::{device_pixel, measure_cell, snap, snap_length, terminal_grid_bounds};
use scrollbar::scrollbar_quad;
use selection::{collect_overlays, collect_selection_text};
use text_shaping::{
    base_glyph_render_options, glyph_for, glyph_render_options, position_text_row, shape_text_row,
    style_for, styled_font,
};

#[cfg(test)]
use box_drawing::{light_box_diagonals, light_box_drawing, light_box_edges, solid_block_element};
#[cfg(test)]
use look::{contrasted_foreground, terminal_color};
#[cfg(test)]
use metrics::grid_dimension;
#[cfg(test)]
use selection::{
    copy_cursor_line_color, overlay_color, selection_corners, selection_hidden_columns,
};
#[cfg(test)]
use text_shaping::{
    align_line_to_cells, finish_backgrounds, glyph_for_dictionary, push_background,
    push_cell_text_run, same_visual_style, text_run,
};

const DIAGNOSTIC_TARGET: &str = "zz::diagnostics::terminal_render";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSize {
    pub columns: u16,
    pub rows: u16,
    pub cell_width_px: u32,
    pub cell_height_px: u32,
}

pub struct TerminalHistoryRow<'a> {
    pub cells: &'a [PackedCell],
    pub dictionary: &'a TerminalDictionary,
    pub revision: u64,
}

pub trait TerminalHistorySource {
    fn row_count(&self) -> usize;
    fn row(&self, index: usize) -> Option<TerminalHistoryRow<'_>>;

    fn below_count(&self) -> usize {
        0
    }

    fn below(&self, _index: usize) -> Option<TerminalHistoryRow<'_>> {
        None
    }
}

impl TerminalHistorySource for zz_client::scrollback::HistoryRing {
    fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn row(&self, index: usize) -> Option<TerminalHistoryRow<'_>> {
        self.rows.get(index).map(|row| TerminalHistoryRow {
            cells: &row.cells,
            dictionary: &row.dictionary,
            revision: row.revision,
        })
    }
}

impl TerminalHistorySource for zz_client::scrollback::RetainedTerminalViewport {
    fn row_count(&self) -> usize {
        self.history.row_count()
    }

    fn row(&self, index: usize) -> Option<TerminalHistoryRow<'_>> {
        self.history.row(index)
    }

    fn below_count(&self) -> usize {
        self.below.row_count()
    }

    fn below(&self, index: usize) -> Option<TerminalHistoryRow<'_>> {
        self.below.row(index)
    }
}

pub trait TerminalImageSource {
    fn image(&self, image_id: u32, generation: u64) -> Option<Arc<RenderImage>>;
}

#[derive(Clone, Copy)]
pub struct TerminalRenderInput<'a> {
    pub viewport: &'a TerminalViewport,
    pub row_revisions: &'a [u64],
    pub revision_epoch: u64,
    pub history: Option<&'a dyn TerminalHistorySource>,
    pub images: Option<&'a dyn TerminalImageSource>,
    pub local_scroll_target: Option<u32>,
    pub scroll_pixel_offset: Pixels,
    pub overscroll: Pixels,
    pub scrollbar_opacity: f32,
    pub extra_height: Pixels,
    pub command_output: bool,
    pub appearance: &'a TerminalAppearance,
    pub appearance_hash: u64,
    pub text_opacity: f32,
    pub focused: bool,
    pub cursor_blink_visible: bool,
    pub marked_text: Option<&'a str>,
    pub rows_above: Option<Pixels>,
    pub rows_below: Option<Pixels>,
    pub header: Option<crate::pane::HeaderScrim>,
}

#[derive(Clone, Copy, Debug)]
pub struct TerminalGeometry {
    pub grid: GridSize,
    pub grid_bounds: Bounds<Pixels>,
    pub surface_bounds: Bounds<Pixels>,
    pub cell_width: Pixels,
    pub line_height: Pixels,
    pub content_offset: Pixels,
    pub row_projection: RowProjection,
    pub input_bounds: Option<Bounds<Pixels>>,
    pub link_hover_bounds: Option<Bounds<Pixels>>,
}

struct PositionedTextRow {
    row: Rc<CachedTextRow>,
    origin: Point<Pixels>,
}

struct PositionedLine {
    line: ShapedLine,
    origin: Point<Pixels>,
    glyph_options: GlyphRenderOptions,
}

struct PositionedKittyImage {
    visible_bounds: Bounds<Pixels>,
    image_bounds: Bounds<Pixels>,
    image: Arc<RenderImage>,
}

struct CachedLine {
    line: ShapedLine,
    start_column: usize,
    glyph_options: GlyphRenderOptions,
    raster: RefCell<RasterState>,
}

enum RasterState {
    Untried,
    Cached(GlyphRasterData),
    SlowPathOnly,
}

#[derive(Clone, Copy)]
struct CachedBackground {
    start_column: usize,
    cell_count: usize,
    color: Color,
}

#[derive(Clone, Copy)]
struct BackgroundBatch {
    start_column: usize,
    color: Color,
}

#[derive(Clone, Copy)]
struct CachedOverline {
    start_column: usize,
    cell_count: usize,
    color: Color,
    alpha: f32,
}

#[derive(Clone, Copy)]
struct CachedUnderline {
    start_column: usize,
    cell_count: usize,
    color: Color,
    alpha: f32,
    kind: TerminalUnderlineStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BoxDrawingEdges(u8);

impl BoxDrawingEdges {
    const UP: u8 = 1 << 0;
    const RIGHT: u8 = 1 << 1;
    const DOWN: u8 = 1 << 2;
    const LEFT: u8 = 1 << 3;

    const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    const fn connects(self, edge: u8) -> bool {
        self.0 & edge != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BoxDiagonals(u8);

impl BoxDiagonals {
    const FALLING: u8 = 1 << 0;
    const RISING: u8 = 1 << 1;

    const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    const fn contains(self, diagonal: u8) -> bool {
        self.0 & diagonal != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LightBoxDrawing {
    Segments(BoxDrawingEdges),
    Rounded(BoxDrawingEdges),
    Diagonals(BoxDiagonals),
}

#[derive(Clone, Copy)]
struct CachedBoxConnector {
    column: usize,
    drawing: LightBoxDrawing,
    color: Color,
    alpha: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SolidBlockElement {
    Rect {
        left_eighths: u8,
        top_eighths: u8,
        right_eighths: u8,
        bottom_eighths: u8,
    },
    Quadrants(u8),
}

impl SolidBlockElement {
    const TOP_LEFT: u8 = 1 << 0;
    const TOP_RIGHT: u8 = 1 << 1;
    const BOTTOM_LEFT: u8 = 1 << 2;
    const BOTTOM_RIGHT: u8 = 1 << 3;
}

#[derive(Clone, Copy)]
struct CachedSolidBlock {
    column: usize,
    element: SolidBlockElement,
    color: Color,
    alpha: f32,
}

enum TerminalGraphicPaint {
    Quad(PaintQuad),
    Path { path: Path<Pixels>, color: Hsla },
}

#[derive(Default)]
struct CachedTextRow {
    backgrounds: Vec<CachedBackground>,
    box_connectors: Vec<CachedBoxConnector>,
    solid_blocks: Vec<CachedSolidBlock>,
    lines: Vec<CachedLine>,
    overlines: Vec<CachedOverline>,
    underlines: Vec<CachedUnderline>,
}

#[derive(Clone, PartialEq)]
struct CellMetricsSignature {
    scale_bits: u32,
    font: Font,
    font_size: Pixels,
}

#[derive(Clone, Copy)]
struct CellMetrics {
    width: Pixels,
    line_height: Pixels,
    box_stroke: Pixels,
}

struct CachedCellMetrics {
    signature: CellMetricsSignature,
    metrics: CellMetrics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SelectionCacheKey {
    revision: u64,
    start: u16,
    end: u16,
}

#[derive(Clone, PartialEq)]
struct RowCacheSignature {
    dictionary_generation: u32,
    scale_bits: u32,
    font: Font,
    font_size: Pixels,
    cell_width: Pixels,
    foreground: Color,
    background: Color,
    appearance_hash: u64,
    text_opacity_bits: u32,
}

#[derive(Default)]
struct RevisionHasher(u64);

impl Hasher for RevisionHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.0 = hash;
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = value;
    }
}

type RevisionMap<T> = HashMap<u64, T, BuildHasherDefault<RevisionHasher>>;
type RevisionSet = HashSet<u64, BuildHasherDefault<RevisionHasher>>;

#[derive(Default)]
pub struct RowRenderCache {
    signature: Option<RowCacheSignature>,
    retention_key: Option<(u64, bool)>,
    rows: RevisionMap<Rc<CachedTextRow>>,
    selection_rows: HashMap<SelectionCacheKey, Rc<CachedTextRow>>,
    live_selection_keys: HashSet<SelectionCacheKey>,
    live_revisions: RevisionSet,
    cell_metrics: Option<CachedCellMetrics>,
    paint: PaintBuffers,
}

impl RowRenderCache {
    fn cell_metrics(
        &mut self,
        signature: CellMetricsSignature,
        probe_color: Hsla,
        window: &mut Window,
    ) -> CellMetrics {
        if let Some(metrics) = self.cell_metrics.as_ref()
            && metrics.signature == signature
        {
            return metrics.metrics;
        }
        let scale = f32::from_bits(signature.scale_bits);
        let (width, line_height, font_id) = measure_cell(
            &signature.font,
            signature.font_size,
            scale,
            probe_color,
            window,
        );
        let box_stroke = box_stroke_width(
            window
                .text_system()
                .underline_thickness(font_id, signature.font_size),
            scale,
        );
        let metrics = CellMetrics {
            width,
            line_height,
            box_stroke,
        };
        self.cell_metrics = Some(CachedCellMetrics { signature, metrics });
        metrics
    }

    fn prepare(
        &mut self,
        signature: RowCacheSignature,
        revision_epoch: u64,
        local_scroll_active: bool,
        revisions: impl Iterator<Item = u64>,
    ) {
        if self.signature.as_ref() != Some(&signature) {
            if let Some(previous) = self.signature.as_ref() {
                log::trace!(
                    target: "zz::diagnostics::terminal_render",
                    "row_cache_invalidate previous_scale_bits={} next_scale_bits={} previous_appearance_hash={} next_appearance_hash={} previous_dictionary_generation={} next_dictionary_generation={}",
                    previous.scale_bits,
                    signature.scale_bits,
                    previous.appearance_hash,
                    signature.appearance_hash,
                    previous.dictionary_generation,
                    signature.dictionary_generation,
                );
            }
            self.signature = Some(signature);
            self.rows.clear();
            self.selection_rows.clear();
            self.live_selection_keys.clear();
            self.retention_key = None;
            self.live_revisions.clear();
        }
        let retention_key = (revision_epoch, local_scroll_active);
        if self.retention_key == Some(retention_key) {
            return;
        }

        self.live_revisions.clear();
        self.live_revisions.extend(revisions);
        let live_revisions = &self.live_revisions;
        self.rows
            .retain(|revision, _| live_revisions.contains(revision));
        self.selection_rows
            .retain(|key, _| live_revisions.contains(&key.revision));
        self.retention_key = Some(retention_key);
    }
}

#[derive(Default)]
struct PaintBuffers {
    kitty_below_bg: Vec<PositionedKittyImage>,
    backgrounds: Vec<PaintQuad>,
    overlays: Vec<PaintQuad>,
    selection: Vec<Bounds<Pixels>>,
    box_connectors: Vec<TerminalGraphicPaint>,
    kitty_below_text: Vec<PositionedKittyImage>,
    cursor: Vec<PaintQuad>,
    text: Vec<PositionedTextRow>,
    decorations: Vec<PaintQuad>,
    kitty_above_text: Vec<PositionedKittyImage>,
}

impl PaintBuffers {
    fn clear(&mut self) {
        self.kitty_below_bg.clear();
        self.backgrounds.clear();
        self.overlays.clear();
        self.selection.clear();
        self.box_connectors.clear();
        self.kitty_below_text.clear();
        self.cursor.clear();
        self.text.clear();
        self.decorations.clear();
        self.kitty_above_text.clear();
    }
}

pub struct PaintState {
    pub geometry: TerminalGeometry,
    focused: bool,
    buffers: PaintBuffers,
    row_clip: Option<Bounds<Pixels>>,
    rows_above_top: Option<Pixels>,
    rows_below_bottom: Option<Pixels>,
    header_scrim: Option<crate::pane::HeaderScrim>,
    scrollbar: Option<PaintQuad>,
    cursor_glyph: Option<PositionedLine>,
    composition: Option<PositionedLine>,
    cell_width: Pixels,
    line_height: Pixels,
}

impl PaintState {
    #[must_use]
    pub fn selection_bounds(&self) -> &[Bounds<Pixels>] {
        &self.buffers.selection
    }

    #[must_use]
    pub const fn header_scrim(&self) -> Option<crate::pane::HeaderScrim> {
        self.header_scrim
    }
}

struct TextBatch {
    text: String,
    start_column: usize,
    cell_count: usize,
    runs: SmallVec<[TextRun; 4]>,
    last_style: Option<PackedStyle>,
    last_foreground: Option<Color>,
    font_style: TerminalFontStyle,
    glyph_options: GlyphRenderOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalFontStyle {
    bold: bool,
    italic: bool,
}

impl TerminalFontStyle {
    fn from_packed(style: PackedStyle) -> Self {
        Self {
            bold: style.bold(),
            italic: style.italic(),
        }
    }
}

#[derive(Clone, Copy)]
struct CursorCellPaint {
    column: usize,
    row: usize,
    width: usize,
    cell: PackedCell,
}

impl RowRenderCache {
    pub fn prepaint(
        &mut self,
        input: TerminalRenderInput<'_>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> PaintState {
        let started = log::log_enabled!(target: DIAGNOSTIC_TARGET, log::Level::Trace)
            .then(web_time::Instant::now);
        let track = bounds;
        let bounds = Bounds::new(
            bounds.origin + point(px(0.0), input.overscroll),
            bounds.size,
        );
        let viewport = input.viewport;
        let row_revisions = input.row_revisions;
        let row_revision_epoch = input.revision_epoch;
        let command_output = input.command_output;
        let local_scroll_target = input.local_scroll_target;
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let base_font = text_style.font();
        let scale = window.scale_factor();
        let CellMetrics {
            width: cell_width,
            line_height: natural_line_height,
            box_stroke,
        } = self.cell_metrics(
            CellMetricsSignature {
                scale_bits: scale.to_bits(),
                font: base_font.clone(),
                font_size,
            },
            color(viewport.foreground, 1.0),
            window,
        );
        let mut buffers = {
            let row_cache = &mut *self;
            let mut buffers = std::mem::take(&mut row_cache.paint);
            buffers.clear();
            buffers
        };
        let raw_line_height = px(input
            .appearance
            .cell_height_adjustment
            .apply(f32::from(natural_line_height))
            .max(1.0));
        let line_height = snap_length(raw_line_height, scale);
        let extra_height = input.extra_height.max(px(0.0));
        let measured = terminal_grid_size(
            size(bounds.size.width, bounds.size.height + extra_height),
            cell_width,
            line_height,
            scale,
        );
        let columns = measured.columns;
        let rows = measured.rows;
        let spare_height = (bounds.size.height + extra_height - line_height * usize::from(rows))
            .max(px(0.0))
            - extra_height;
        let scroll_shift = if input.scroll_pixel_offset > px(0.0) {
            snap(input.scroll_pixel_offset.min(line_height), scale)
        } else {
            px(0.0)
        };
        let scrolled_locally = local_scroll_target.is_some() || scroll_shift > px(0.0);
        let GridPlacement {
            bottom_anchored,
            project_from_bottom,
        } = grid_placement(
            viewport,
            input.appearance.local_colors.as_ref(),
            command_output,
            scrolled_locally,
        );
        let header_scrim = input
            .header
            .filter(|_| bottom_anchored && scrolled_back(viewport, scrolled_locally));
        let history_rows = input.history.map_or(0, TerminalHistorySource::row_count);
        let below_rows = input.history.map_or(0, TerminalHistorySource::below_count);
        let row_projection = RowProjection::new(viewport.rows, rows, project_from_bottom);
        let live_row_projection = local_scroll_target.map_or(row_projection, |target_offset| {
            local_live_projection(
                target_offset,
                viewport.scrollbar.offset,
                viewport.rows,
                rows,
            )
        });
        let origin = point(
            snap(bounds.origin.x, scale),
            snap(
                bounds.origin.y
                    + if bottom_anchored {
                        spare_height
                    } else {
                        px(0.0)
                    },
                scale,
            ),
        );
        let grid_bounds = terminal_grid_bounds(origin, columns, rows, cell_width, line_height);
        let content_origin = point(origin.x, origin.y + scroll_shift);
        let rows_above_top = input
            .rows_above
            .or(header_scrim.map(|scrim| scrim.band.top()))
            .filter(|top| bottom_anchored && *top < grid_bounds.origin.y);
        let row_clip = match rows_above_top {
            Some(top) => Some(Bounds::new(
                point(bounds.origin.x, top),
                size(
                    bounds.size.width,
                    grid_bounds.origin.y + grid_bounds.size.height - top,
                ),
            )),
            None => (scroll_shift > px(0.0)).then(|| {
                Bounds::new(
                    point(bounds.origin.x, grid_bounds.origin.y),
                    size(bounds.size.width, grid_bounds.size.height),
                )
            }),
        };
        let last_row = row_projection
            .count
            .checked_sub(1)
            .and_then(|offset| Some((offset, row_projection.source_row(offset)?)));
        let grid_bottom = grid_bounds.origin.y + grid_bounds.size.height;
        let rows_below_bottom = input.rows_below.filter(|bottom| {
            (local_scroll_target.is_some() || below_rows > 0) && *bottom > grid_bottom
        });
        let row_clip = rows_below_bottom.map_or(row_clip, |bottom| {
            let clip = row_clip.unwrap_or(bounds);
            Some(Bounds::new(
                clip.origin,
                size(clip.size.width, bottom - clip.origin.y),
            ))
        });
        let below_count = rows_below_bottom.map_or(0, |bottom| {
            (f32::from(bottom - grid_bottom) / f32::from(line_height)).ceil() as u32 + 1
        });
        let below = || {
            (1..=below_count).filter_map(move |distance| {
                let target = local_scroll_target.unwrap_or(viewport.scrollbar.offset);
                let (offset, source_row) = last_row?;
                let source = local_row_source(
                    target
                        .saturating_add(u32::from(source_row))
                        .saturating_add(distance),
                    viewport.scrollbar.offset,
                    viewport.rows,
                    history_rows,
                    below_rows,
                );
                let row =
                    u32::from(row_projection.destination_start) + u32::from(offset) + distance;
                (source != LocalRowSource::Shimmer).then_some((row, source))
            })
        };
        let above_count = rows_above_top.map_or(u32::from(scroll_shift > px(0.0)), |top| {
            (f32::from(content_origin.y - top) / f32::from(line_height)).ceil() as u32
        });
        let above_top_offset = local_scroll_target.unwrap_or(viewport.scrollbar.offset);
        let above = || {
            (1..=above_count).filter_map(move |distance| {
                peek_row_source(
                    above_top_offset,
                    distance,
                    viewport.scrollbar.offset,
                    viewport.rows,
                    history_rows,
                )
                .map(|source| (distance, source))
            })
        };
        let cursor = viewport.cursor;
        let cursor_cell =
            cursor.and_then(|cursor| cursor_cell(viewport, cursor, columns, live_row_projection));
        let cursor_bounds = cursor_cell.map(|cursor| {
            Bounds::new(
                point(
                    content_origin.x + cell_width * cursor.column,
                    content_origin.y + line_height * cursor.row,
                ),
                size(cell_width * cursor.width, line_height),
            )
        });
        let composing = input.marked_text.is_some_and(|text| !text.is_empty());
        let composition = input.marked_text.and_then(|marked_text| {
            if marked_text.is_empty() {
                return None;
            }
            let bounds = cursor_bounds?;
            let run = TextRun {
                len: marked_text.len(),
                font: base_font.clone(),
                color: color(viewport.foreground, input.text_opacity),
                background_color: None,
                underline: Some(UnderlineStyle {
                    thickness: device_pixel(scale),
                    color: Some(color(viewport.foreground, input.text_opacity)),
                    wavy: false,
                }),
                strikethrough: None,
            };
            Some(PositionedLine {
                line: window.text_system().shape_line(
                    marked_text.to_owned().into(),
                    font_size,
                    &[run],
                    None,
                ),
                origin: bounds.origin,
                glyph_options: base_glyph_render_options(input.appearance),
            })
        });
        let hidden_composition_cells = composition.as_ref().and_then(|composition| {
            let cursor = cursor_cell?;
            let shaped_cells = (f32::from(composition.line.width) / f32::from(cell_width))
                .ceil()
                .max(1.0) as usize;
            let width = cursor.width.max(shaped_cells);
            Some((
                cursor.row,
                cursor.column..cursor.column.saturating_add(width),
            ))
        });
        let grid_size = measured;
        let mut cached_row_hits = 0;
        let mut cached_row_misses = 0;
        let mut uncached_rows = 0;
        {
            let row_cache = &mut *self;
            row_cache.prepare(
                RowCacheSignature {
                    dictionary_generation: viewport.dictionary_generation,
                    scale_bits: scale.to_bits(),
                    font: base_font.clone(),
                    font_size,
                    cell_width,
                    foreground: viewport.foreground,
                    background: viewport.background,
                    appearance_hash: input.appearance_hash,
                    text_opacity_bits: input.text_opacity.to_bits(),
                },
                row_revision_epoch,
                local_scroll_target.is_some(),
                (0..history_rows)
                    .filter(|_| local_scroll_target.is_some())
                    .filter_map(|index| input.history.and_then(|history| history.row(index)))
                    .map(|row| row.revision)
                    .chain(
                        (0..below_rows)
                            .filter(|_| local_scroll_target.is_some())
                            .filter_map(|index| {
                                input.history.and_then(|history| history.below(index))
                            })
                            .map(|row| row.revision),
                    )
                    .chain(row_revisions.iter().copied())
                    .chain(above().chain(below()).filter_map(|(_, source)| {
                        match source {
                            LocalRowSource::History(index) => input
                                .history
                                .and_then(|history| history.row(index))
                                .map(|row| row.revision),
                            LocalRowSource::Below(index) => input
                                .history
                                .and_then(|history| history.below(index))
                                .map(|row| row.revision),
                            _ => None,
                        }
                    })),
            );
            let projected_rows = (0..row_projection.count).map(|offset| {
                let source_row = row_projection
                    .source_row(offset)
                    .expect("projection offset is bounded by its count");
                let row_index = usize::from(row_projection.destination_start + offset);
                let source =
                    local_scroll_target.map_or(LocalRowSource::Live(source_row), |target_offset| {
                        local_row_source(
                            target_offset.saturating_add(u32::from(source_row)),
                            viewport.scrollbar.offset,
                            viewport.rows,
                            history_rows,
                            below_rows,
                        )
                    });
                (
                    Some(row_index),
                    point(content_origin.x, content_origin.y + line_height * row_index),
                    source,
                )
            });
            for (row_index, row_origin, source) in above()
                .map(|(distance, source)| {
                    (
                        None,
                        point(
                            content_origin.x,
                            content_origin.y - line_height * distance as f32,
                        ),
                        source,
                    )
                })
                .chain(projected_rows)
                .chain(below().map(|(row, source)| {
                    (
                        None,
                        point(
                            content_origin.x,
                            content_origin.y + line_height * row as f32,
                        ),
                        source,
                    )
                }))
            {
                let resolved = match source {
                    LocalRowSource::Live(live_row) => Some((
                        viewport.row(live_row).unwrap_or_default(),
                        viewport.dictionary.as_ref(),
                        row_revisions.get(usize::from(live_row)).copied(),
                        true,
                    )),
                    LocalRowSource::History(history_row) => input
                        .history
                        .and_then(|history| history.row(history_row))
                        .map(|row| (row.cells, row.dictionary, Some(row.revision), false)),
                    LocalRowSource::Below(below_row) => input
                        .history
                        .and_then(|history| history.below(below_row))
                        .map(|row| (row.cells, row.dictionary, Some(row.revision), false)),
                    LocalRowSource::Shimmer => None,
                };
                let Some((row, dictionary, revision, is_live)) = resolved else {
                    uncached_rows += 1;
                    buffers.backgrounds.push(fill(
                        Bounds::new(
                            row_origin,
                            size(cell_width * usize::from(columns), line_height),
                        ),
                        color(viewport.foreground, 0.04),
                    ));
                    continue;
                };
                let hidden_columns = hidden_composition_cells
                    .as_ref()
                    .filter(|(hidden_row, _)| is_live && Some(*hidden_row) == row_index)
                    .map(|(_, columns)| columns.clone());
                if hidden_columns.is_some() {
                    uncached_rows += 1;
                    let masked = Rc::new(shape_text_row(
                        row,
                        dictionary,
                        viewport.foreground,
                        viewport.background,
                        input.appearance,
                        cell_width,
                        font_size,
                        scale,
                        &base_font,
                        input.text_opacity,
                        None,
                        hidden_columns.as_ref(),
                        window,
                    ));
                    position_text_row(
                        &masked,
                        row_origin,
                        cell_width,
                        line_height,
                        box_stroke,
                        scale,
                        &mut buffers,
                    );
                    continue;
                }
                if let Some(revision) = revision {
                    let cached: &Rc<CachedTextRow> = match row_cache.rows.entry(revision) {
                        Entry::Occupied(entry) => {
                            cached_row_hits += 1;
                            entry.into_mut()
                        }
                        Entry::Vacant(entry) => {
                            cached_row_misses += 1;
                            entry.insert(Rc::new(shape_text_row(
                                row,
                                dictionary,
                                viewport.foreground,
                                viewport.background,
                                input.appearance,
                                cell_width,
                                font_size,
                                scale,
                                &base_font,
                                input.text_opacity,
                                None,
                                None,
                                window,
                            )))
                        }
                    };
                    position_text_row(
                        cached,
                        row_origin,
                        cell_width,
                        line_height,
                        box_stroke,
                        scale,
                        &mut buffers,
                    );
                } else {
                    uncached_rows += 1;
                    let cached = Rc::new(shape_text_row(
                        row,
                        dictionary,
                        viewport.foreground,
                        viewport.background,
                        input.appearance,
                        cell_width,
                        font_size,
                        scale,
                        &base_font,
                        input.text_opacity,
                        None,
                        None,
                        window,
                    ));
                    position_text_row(
                        &cached,
                        row_origin,
                        cell_width,
                        line_height,
                        box_stroke,
                        scale,
                        &mut buffers,
                    );
                }
            }
        }
        {
            let row_cache = &mut *self;
            collect_selection_text(
                viewport,
                row_revisions,
                input.appearance,
                live_row_projection,
                hidden_composition_cells.as_ref(),
                content_origin,
                cell_width,
                line_height,
                box_stroke,
                font_size,
                scale,
                &base_font,
                input.text_opacity,
                window,
                row_cache,
                &mut buffers.text,
                &mut buffers.box_connectors,
            );
        }
        if let Some(images) = input.images {
            collect_kitty_images(
                viewport,
                images,
                local_scroll_target,
                row_projection,
                content_origin,
                grid_bounds,
                cell_width,
                line_height,
                scale,
                &mut buffers,
            );
        }
        let link_hover_bounds = collect_overlays(
            viewport,
            input.appearance,
            live_row_projection,
            columns,
            content_origin,
            cell_width,
            line_height,
            scale,
            &mut buffers.overlays,
            &mut buffers.selection,
        );
        let scrollbar =
            local_scroll_target.map_or(viewport.scrollbar, |target_offset| ScrollbarState {
                total: viewport.scrollbar.total,
                offset: target_offset,
                len: viewport.scrollbar.len,
            });
        let scrollbar = scrollbar_quad(
            scrollbar,
            track,
            input.overscroll,
            input.scrollbar_opacity,
            window.rem_size(),
            cx,
        );
        let focused = input.focused;
        let cursor_visible =
            cursor_visible_for_paint(cursor, composing, focused, input.cursor_blink_visible);
        let mut cursor_bounds = cursor_bounds;
        let mut cursor_glyph = None;
        if let Some(mut visible_cursor_bounds) = cursor_bounds.filter(|_| cursor_visible) {
            let cursor = cursor.expect("cursor bounds require a cursor");
            if focused && matches!(cursor.style(), CursorStyle::Block) {
                cursor_glyph = cursor_cell.and_then(|cursor_cell| {
                    cursor_glyph_line(
                        viewport,
                        cursor_cell,
                        visible_cursor_bounds.origin,
                        font_size,
                        &base_font,
                        input.appearance,
                        window,
                    )
                });
                if let Some(glyph) = cursor_glyph.as_ref() {
                    visible_cursor_bounds.size.width =
                        visible_cursor_bounds.size.width.max(glyph.line.width);
                    cursor_bounds = Some(visible_cursor_bounds);
                }
            }
            buffers.cursor.push(cursor_quad(
                visible_cursor_bounds,
                cursor.style(),
                cursor.color(),
                focused,
                scale,
            ));
        }

        let input_bounds = composition.as_ref().map_or(cursor_bounds, |composition| {
            Some(Bounds::new(
                composition.origin,
                size(composition.line.width.max(cell_width), line_height),
            ))
        });
        if started.is_some() {
            let cache = &*self;
            log::trace!(
                target: "zz::diagnostics::terminal_render",
                "prepaint bounds={bounds:?} scale_factor={} grid={grid_size:?} origin={origin:?} raw_line_height={} snapped_line_height={} spare_height={} bottom_anchored={} scroll_shift={} appearance_hash={} viewport_generation={} viewport_view_generation={} viewport_dictionary_generation={} viewport_columns={} viewport_rows={} viewport_cells={} viewport_overlays={} row_revision_epoch={} row_revisions={} cached_row_hits={} cached_row_misses={} uncached_rows={} cache_rows={} cache_selection_rows={} cache_live_revisions={} cache_paint_backgrounds_capacity={} cache_paint_overlays_capacity={} cache_paint_box_connectors_capacity={} cache_paint_cursor_capacity={} cache_paint_text_capacity={} cache_paint_decorations_capacity={} prepared_backgrounds={} prepared_overlays={} prepared_box_connectors={} prepared_cursor={} prepared_text_rows={} prepared_decorations={} cursor_style={:?} cursor_width={} blink_policy={:?} composition={} cursor_suppressed_by_composition={} elapsed_us={}",
                scale,
                f32::from(raw_line_height),
                f32::from(line_height),
                f32::from(spare_height),
                bottom_anchored,
                f32::from(scroll_shift),
                input.appearance_hash,
                viewport.generation,
                viewport.view_generation,
                viewport.dictionary_generation,
                viewport.columns,
                viewport.rows,
                viewport.cells.len(),
                viewport.overlays.len(),
                row_revision_epoch,
                row_revisions.len(),
                cached_row_hits,
                cached_row_misses,
                uncached_rows,
                cache.rows.len(),
                cache.selection_rows.len(),
                cache.live_revisions.len(),
                cache.paint.backgrounds.capacity(),
                cache.paint.overlays.capacity(),
                cache.paint.box_connectors.capacity(),
                cache.paint.cursor.capacity(),
                cache.paint.text.capacity(),
                cache.paint.decorations.capacity(),
                buffers.backgrounds.len(),
                buffers.overlays.len(),
                buffers.box_connectors.len(),
                buffers.cursor.len(),
                buffers.text.len(),
                buffers.decorations.len(),
                viewport.cursor.map(Cursor::style),
                cursor_cell.map_or(0, |cursor| cursor.width),
                input.appearance.cursor_blink_policy,
                composition.is_some(),
                composing,
                started.map_or(0, |started| started.elapsed().as_micros()),
            );
        }

        PaintState {
            geometry: TerminalGeometry {
                grid: grid_size,
                grid_bounds,
                surface_bounds: bounds,
                cell_width,
                line_height,
                content_offset: scroll_shift,
                row_projection,
                input_bounds,
                link_hover_bounds,
            },
            focused,
            buffers,
            row_clip,
            rows_above_top,
            rows_below_bottom,
            header_scrim,
            scrollbar,
            cursor_glyph,
            composition,
            cell_width,
            line_height,
        }
    }

    pub fn paint(
        &mut self,
        paint: &mut PaintState,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let started = log::log_enabled!(target: DIAGNOSTIC_TARGET, log::Level::Trace)
            .then(web_time::Instant::now);
        let kitty_below_bg_count = paint.buffers.kitty_below_bg.len();
        let kitty_below_text_count = paint.buffers.kitty_below_text.len();
        let kitty_above_text_count = paint.buffers.kitty_above_text.len();
        let background_count = paint.buffers.backgrounds.len();
        let overlay_count = paint.buffers.overlays.len();
        let box_connector_count = paint.buffers.box_connectors.len();
        let cursor_count = paint.buffers.cursor.len();
        let text_count = paint.buffers.text.len();
        let decoration_count = paint.buffers.decorations.len();
        let had_composition = paint.composition.is_some();
        let top = paint.rows_above_top.unwrap_or(bounds.origin.y);
        let bottom = paint
            .rows_below_bottom
            .map_or(bounds.bottom(), |bottom| bottom.max(bounds.bottom()));
        let row_clip = paint.row_clip.map_or(bounds, |row_clip| {
            row_clip.intersect(&Bounds::new(
                point(bounds.origin.x, top),
                size(bounds.size.width, bottom - top),
            ))
        });
        window.with_content_mask(Some(ContentMask { bounds: row_clip }), |window| {
            paint_kitty_images(&mut paint.buffers.kitty_below_bg, window);
            for quad in paint.buffers.backgrounds.drain(..) {
                window.paint_quad(quad);
            }
            for quad in paint.buffers.overlays.drain(..) {
                window.paint_quad(quad);
            }
        });
        let (scrollbar, scrollbar_over_scrim) = match paint.header_scrim {
            Some(_) => (None, paint.scrollbar.take()),
            None => (paint.scrollbar.take(), None),
        };
        if let Some(quad) = scrollbar {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                window.paint_quad(quad);
            });
        }
        window.with_content_mask(Some(ContentMask { bounds: row_clip }), |window| {
            for graphic in paint.buffers.box_connectors.drain(..) {
                match graphic {
                    TerminalGraphicPaint::Quad(quad) => window.paint_quad(quad),
                    TerminalGraphicPaint::Path { path, color } => window.paint_path(path, color),
                }
            }
            paint_kitty_images(&mut paint.buffers.kitty_below_text, window);
            for row in paint.buffers.text.drain(..) {
                for line in &row.row.lines {
                    let origin = point(
                        row.origin.x + paint.cell_width * line.start_column,
                        row.origin.y,
                    );
                    let should_compute_raster = {
                        let raster = line.raster.borrow();
                        match &*raster {
                            RasterState::Cached(data) => {
                                if line.line.paint_with_raster_data(
                                    data,
                                    origin,
                                    paint.line_height,
                                    window,
                                ) {
                                    continue;
                                }
                                true
                            }
                            RasterState::Untried => true,
                            RasterState::SlowPathOnly => false,
                        }
                    };
                    if should_compute_raster {
                        *line.raster.borrow_mut() = RasterState::Untried;
                        match line.line.compute_glyph_raster_data(
                            origin,
                            paint.line_height,
                            line.glyph_options,
                            window,
                            cx,
                        ) {
                            Ok(Some(data)) => {
                                let painted = line.line.paint_with_raster_data(
                                    &data,
                                    origin,
                                    paint.line_height,
                                    window,
                                );
                                *line.raster.borrow_mut() = RasterState::Cached(data);
                                if painted {
                                    continue;
                                }
                            }
                            Ok(None) => {
                                *line.raster.borrow_mut() = RasterState::SlowPathOnly;
                            }
                            Err(error) => {
                                log::trace!(
                                    target: "zz::diagnostics::terminal_render",
                                    "failed to compute terminal glyph raster data: {error}"
                                );
                            }
                        }
                    }
                    if let Err(error) = line.line.paint_with_options(
                        origin,
                        paint.line_height,
                        TextAlign::Left,
                        None,
                        line.glyph_options,
                        window,
                        cx,
                    ) {
                        log::error!("failed to paint terminal text: {error}");
                    }
                }
            }
            for quad in paint.buffers.decorations.drain(..) {
                window.paint_quad(quad);
            }
            paint_kitty_images(&mut paint.buffers.kitty_above_text, window);
            for quad in paint.buffers.cursor.drain(..) {
                window.paint_quad(quad);
            }
            if let Some(cursor_glyph) = paint.cursor_glyph.take()
                && let Err(error) = cursor_glyph.line.paint_with_options(
                    cursor_glyph.origin,
                    paint.line_height,
                    TextAlign::Left,
                    None,
                    cursor_glyph.glyph_options,
                    window,
                    cx,
                )
            {
                log::error!("failed to paint terminal cursor glyph: {error}");
            }
            if let Some(composition) = paint.composition.take()
                && let Err(error) = composition.line.paint_with_options(
                    composition.origin,
                    paint.line_height,
                    TextAlign::Left,
                    None,
                    composition.glyph_options,
                    window,
                    cx,
                )
            {
                log::error!("failed to paint IME composition: {error}");
            }
        });
        if let Some(scrim) = paint.header_scrim {
            crate::pane::paint_header_scrim(window, scrim);
            if let Some(quad) = scrollbar_over_scrim {
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    window.paint_quad(quad);
                });
            }
        }
        self.paint = std::mem::take(&mut paint.buffers);
        log::trace!(
            target: "zz::diagnostics::terminal_render",
            "paint bounds={bounds:?} focused={} kitty_below_bg={} backgrounds={} overlays={} box_connectors={} kitty_below_text={} cursors={} text_rows={} decorations={} kitty_above_text={} composition={} elapsed_us={}",
            paint.focused,
            kitty_below_bg_count,
            background_count,
            overlay_count,
            box_connector_count,
            kitty_below_text_count,
            cursor_count,
            text_count,
            decoration_count,
            kitty_above_text_count,
            had_composition,
            started.map_or(0, |started| started.elapsed().as_micros()),
        );
    }
}

pub use overlays::{FindBarAction, pasted_image_preview, search_result_text, terminal_find_bar};

mod overlays {
    use std::sync::Arc;

    use ala::{
        Anchor, AnyElement, App, Bounds, Entity, Image, ImageSource, MouseButton, ObjectFit,
        Pixels, Window, anchored, deferred, div, img, prelude::*, px,
    };
    use zz_terminal::{SearchCase, SearchMode, SearchQuery, SearchStatus};

    use penna::{Label, Panel, PanelLook, TextStyle};

    use crate::{
        ActiveTheme as _, Colorize as _, Disableable as _, Icon, IconName, Selectable as _,
        StyledExt as _,
        button::{Button, ButtonLook, ButtonVariants as _},
        input::{Input, InputState},
    };

    const IMAGE_PREVIEW_SIDE: f32 = 300.0;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum FindBarAction {
        ToggleRegex,
        ToggleCase,
        Previous,
        Next,
        Close,
    }

    pub use zz_client::terminal_view::search_result_text;

    pub fn terminal_find_bar(
        input: &Entity<InputState>,
        query: &SearchQuery,
        status: Option<SearchStatus>,
        on_action: impl Fn(FindBarAction, &mut Window, &mut App) + 'static,
        cx: &App,
    ) -> Panel {
        let on_action = std::rc::Rc::new(on_action);
        let enabled = status.is_some_and(|status| {
            status.total > 0 && !status.pending() && !status.invalid_pattern()
        });
        let ground = cx.theme().ground_of(PanelLook::Popover);
        let button = |id: &'static str, icon: IconName, action: FindBarAction| {
            let on_action = std::rc::Rc::clone(&on_action);
            Button::compact_icon(id, icon)
                .ghost()
                .look(ButtonLook::Flat)
                .on(ground)
                .on_click(move |_, window, cx| {
                    on_action(action, window, cx);
                    cx.stop_propagation();
                })
        };
        Panel::new("terminal-search", PanelLook::Popover)
            .debug_selector(|| "terminal-search".to_owned())
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(4.0))
            .w(px(460.0))
            .max_w_full()
            .p(px(4.0))
            .occlude()
            .font_family(cx.theme().font_family.clone())
            .type_style(TextStyle::Body, cx)
            .line_height(px(16.0))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div().flex_1().min_w(px(110.0)).child(
                    Input::new(input)
                        .on(ground)
                        .prefix(Icon::new(IconName::Search).size(px(13.0))),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(2.0))
                    .ml_auto()
                    .child(
                        Label::new(
                            TextStyle::Caption,
                            if query.text.is_empty() {
                                String::new()
                            } else {
                                search_result_text(status)
                            },
                        )
                        .px(px(4.0))
                        .text_color(
                            if status.is_some_and(SearchStatus::invalid_pattern) {
                                cx.theme().danger
                            } else {
                                cx.theme().foreground.muted()
                            },
                        ),
                    )
                    .child(
                        button(
                            "terminal-search-regex",
                            IconName::Asterisk,
                            FindBarAction::ToggleRegex,
                        )
                        .selected(query.mode == SearchMode::Regex)
                        .tooltip("Regular expression"),
                    )
                    .child(
                        button(
                            "terminal-search-case",
                            IconName::CaseSensitive,
                            FindBarAction::ToggleCase,
                        )
                        .selected(query.case == SearchCase::Sensitive)
                        .tooltip(zz_client::terminal_view::search_case_label(query.case)),
                    )
                    .child(
                        button(
                            "terminal-search-previous",
                            IconName::ChevronUp,
                            FindBarAction::Previous,
                        )
                        .disabled(!enabled)
                        .tooltip("Previous match"),
                    )
                    .child(
                        button(
                            "terminal-search-next",
                            IconName::ChevronDown,
                            FindBarAction::Next,
                        )
                        .disabled(!enabled)
                        .tooltip("Next match"),
                    )
                    .child(
                        button(
                            "terminal-search-close",
                            IconName::Xmark,
                            FindBarAction::Close,
                        )
                        .tooltip("Close search"),
                    ),
            )
    }

    pub fn pasted_image_preview(link: Bounds<Pixels>, image: Arc<Image>, _: &App) -> AnyElement {
        let (anchor, position) = if link.origin.y >= px(IMAGE_PREVIEW_SIDE + 8.0) {
            (Anchor::BottomLeft, link.origin)
        } else {
            (Anchor::TopLeft, link.bottom_left())
        };
        deferred(
            anchored()
                .anchor(anchor)
                .position(position)
                .snap_to_window_with_margin(px(8.0))
                .child(
                    Panel::new("pasted-image-preview", PanelLook::Popover)
                        .size(px(IMAGE_PREVIEW_SIDE))
                        .p_1()
                        .child(
                            img(ImageSource::Image(image))
                                .size_full()
                                .object_fit(ObjectFit::ScaleDown),
                        ),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn terminal_search_feedback_uses_server_results() {
            assert_eq!(search_result_text(Some(SearchStatus::new(2, 4))), "2 / 4");
            assert_eq!(
                search_result_text(Some(SearchStatus::new(0, 0))),
                "0 matches"
            );
            assert_eq!(
                search_result_text(Some(SearchStatus::default().with_pending(true))),
                "Searching…"
            );
            assert_eq!(
                search_result_text(Some(SearchStatus::default().with_invalid_pattern(true))),
                "Invalid pattern"
            );
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "terminal renderer tests use exact terminal palette fixtures"
)]
mod box_drawing_tests;

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "terminal renderer tests use exact terminal palette fixtures"
)]
mod cursor_tests;

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "terminal renderer tests use exact terminal palette fixtures"
)]
mod look_tests;

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "terminal renderer tests use exact terminal palette fixtures"
)]
mod metrics_tests;

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "terminal renderer tests use exact terminal palette fixtures"
)]
mod row_cache_tests;

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "terminal renderer tests use exact terminal palette fixtures"
)]
mod scrollbar_tests;

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "terminal renderer tests use exact terminal palette fixtures"
)]
mod selection_tests;

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "terminal renderer tests use exact terminal palette fixtures"
)]
mod text_shaping_tests;
