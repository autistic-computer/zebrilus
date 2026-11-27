use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    io::{self, Write as _},
    rc::Rc,
    sync::Arc,
};

use unicode_width::UnicodeWidthChar as _;
use zz_protocol::{
    PaneBorderIndicators, PaneBorderLines, PaneBorderStatus, PaneId, PaneKindSnapshot,
    PopupBorderLines, StyledSegment, ThemeColours, TmuxAttributeState, TmuxColour, TmuxStyle,
    apply_style, parse_style, parse_styled_segments,
};
use zz_terminal::ColourClass;
use zz_terminal::{
    CellWidth, Color, Glyph, KittyPlacement, OverlayKind, OverlaySpan, PackedCell, PackedStyle,
    SearchDirection, SearchQuery, TerminalAppearance, TerminalMode, TerminalViewport,
    UnderlineStyle,
};

use crate::{
    browser::{BROWSER_IMAGE_ID, BrowserFrameUpdate},
    kitty::{FrameTransport, KittyBridge, KittyImageData},
    layout::{
        BORDER_D, BORDER_L, BORDER_R, BORDER_U, CELL_LR, Divider, FloatFrame, FloatingSpec,
        PaneRect, Rect, border_glyph, cell_type_of, resolve_floating,
    },
    picker, sidebar,
    state::Model,
    writer::{Submission, TerminalWriter},
};

mod chooser;
mod colour;
mod cursor;
mod menu;
mod pane_mode;
mod sgr;
mod sidebar_paint;
mod status;

#[cfg(test)]
use menu::floating_border;

use sidebar_paint::sidebar_status_lines;

use cursor::{cursor_colour_rgb, prompt_cursor};

#[cfg(test)]
use status::{
    append_status_label, command_output_search_prompt, mode_indicator, mode_indicators,
    status_overlay,
};
use status::{
    base_status_left, client_message_hides_the_cursor, combine_status,
    command_output_search_display, padded_segment, padded_styled, pane_prompt_row_y,
    pane_prompt_target, status_indicators, text_display_width, truncate, truncate_display_cells,
};

use sgr::{
    Ground, clear_screen, cursor_only, fill_unpainted_cells, sgr_is_reset, trailing_clear,
    write_border_runs, write_border_sgr, write_cell_ground, write_colored_sgr, write_colored_text,
    write_cursor_position, write_decimal, write_glyph, write_line_sgr, write_selection_sgr,
    write_sgr, write_styled_text, write_styled_text_over, write_tmux_sgr,
};
#[cfg(test)]
use sgr::{write_palette_ground, write_rgb_ground};

#[cfg(test)]
use chooser::{chooser_key_cell, chooser_key_column};
#[cfg(test)]
use colour::DARK_THEME_COLOURS;
use colour::{RGB_COLOURS, colour_find_rgb, downgrade_palette, resolve_tmux_colour, theme_colour};

pub(crate) use zz_client::ViewportDamage as FrameDamage;

pub(crate) fn merge_damage(damage: &mut FrameDamage, incoming: FrameDamage) {
    match (&mut *damage, incoming) {
        (FrameDamage::All, _) | (_, FrameDamage::All) => *damage = FrameDamage::All,
        (FrameDamage::Rows(existing), FrameDamage::Rows(mut rows)) => {
            existing.append(&mut rows);
            existing.sort_unstable();
            existing.dedup();
        }
    }
}

#[derive(Clone)]
struct PaintedPane {
    viewport: TerminalViewport,
    rect: Rect,
    defaults: [Option<TmuxColour>; 2],
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct StyledLine {
    segments: Vec<StyledSegment>,
}

impl StyledLine {
    fn parsed(value: &str) -> Self {
        Self {
            segments: parse_styled_segments(value),
        }
    }

    fn from_segments(segments: Vec<StyledSegment>) -> Self {
        let mut line = Self::default();
        for segment in segments {
            line.push_segment(&segment.text, segment.style);
        }
        line
    }

    fn plain(value: &str) -> Self {
        let mut line = Self::default();
        line.push_plain(value);
        line
    }

    fn push_plain(&mut self, value: &str) {
        self.push_segment(value, TmuxStyle::default());
    }

    fn append(&mut self, other: &Self) {
        for segment in &other.segments {
            self.push_segment(&segment.text, segment.style.clone());
        }
    }

    fn resolve_theme(&mut self, theme: &ThemeColours) {
        for segment in &mut self.segments {
            for slot in [
                &mut segment.style.fg,
                &mut segment.style.bg,
                &mut segment.style.us,
            ] {
                if let Some(TmuxColour::Theme(index)) = slot
                    && let Some(resolved) = theme.slot(*index)
                {
                    *slot = Some(resolved);
                }
            }
        }
    }

    fn push_segment(&mut self, value: &str, style: TmuxStyle) {
        if value.is_empty() {
            return;
        }
        if let Some(last) = self.segments.last_mut().filter(|last| last.style == style) {
            last.text.push_str(value);
        } else {
            self.segments.push(StyledSegment {
                text: value.to_owned(),
                style,
            });
        }
    }

    fn len(&self) -> usize {
        self.segments
            .iter()
            .map(|segment| text_display_width(&segment.text))
            .sum()
    }

    fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    fn truncate(&self, width: usize) -> Self {
        let mut truncated = Self::default();
        let mut remaining = width;
        for segment in &self.segments {
            let text = truncate_display_cells(&segment.text, remaining);
            let segment_was_truncated = text.len() != segment.text.len();
            remaining = remaining.saturating_sub(text_display_width(&text));
            truncated.push_segment(&text, segment.style.clone());
            if segment_was_truncated {
                break;
            }
        }
        truncated
    }

    #[cfg(test)]
    fn plain_text(&self) -> String {
        self.segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PaintedSidebarRow {
    text: StyledLine,
    selected: bool,
    status: bool,
}

const PAINT_BEGIN: &[u8] = b"\x1b[?2026h\x1b[?25l";
const PAINT_END: &[u8] = b"\x1b[?2026l";
const PAINT_TAIL_BYTES: usize = 64;
const SHOW_CURSOR: &[u8] = b"\x1b[?25h";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TtyState {
    cursor: Option<(u16, u16)>,
    sgr_reset: bool,
    visible: bool,
}

type TerminalStyleKey = (
    PackedStyle,
    bool,
    [Color; 2],
    [Option<TmuxColour>; 2],
    Option<u32>,
);

type PaneMap<V> = HashMap<PaneId, V, foldhash::fast::FixedState>;

pub(crate) struct Renderer {
    output: Vec<u8>,
    queued_control: Vec<u8>,
    overlay_mask: Vec<bool>,
    match_mask: Vec<u8>,
    match_styles: [Option<TmuxStyle>; 2],
    current_line: Option<(u16, TmuxStyle)>,
    selection_mask: Vec<bool>,
    selection_style: Option<TmuxStyle>,
    selection_trim: Option<(u16, u16)>,
    painted: PaneMap<PaintedPane>,
    headers: PaneMap<String>,
    picker_cards: PaneMap<(Rect, usize)>,
    cards: PaneMap<(Rect, &'static str, String, String)>,
    sidebar_rows: Vec<PaintedSidebarRow>,
    status_rows: Vec<StyledLine>,
    status_geometry: Option<(u16, u16, u16)>,
    status_source: Option<Arc<zz_protocol::StatusLine>>,
    damage: PaneMap<FrameDamage>,
    browser_placements: PaneMap<KittyPlacement>,
    browser_painted: PaneMap<bool>,
    last_title: String,
    border_chrome: Option<(
        PaneBorderStatus,
        PaneBorderLines,
        PaneBorderIndicators,
        Arc<zz_protocol::StatusLine>,
    )>,
    mode_tree: chooser::ModeTree,
    pane_modes_painted: PaneMap<bool>,
    kitty: KittyBridge,
    writer: std::rc::Rc<std::cell::RefCell<TerminalWriter>>,
    control_replay: Vec<u8>,
    paint_tail: Vec<u8>,
    terminal_colours: Option<u32>,
    terminal_defaults: TmuxStyle,
    terminal_sgr: Option<(TerminalStyleKey, Vec<u8>)>,
    blank_is_default: bool,
    default_blank: PaneMap<Rect>,
    tty: Option<(TtyState, usize)>,
    settled: Option<TtyState>,
    cursor_style: Option<u8>,
    cursor_colour: Option<u32>,
    cursor_colour_known: bool,
    cursor_written: bool,
}

impl Renderer {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_sink(Box::new(|_| Ok(())))
    }

    #[cfg(test)]
    pub(crate) fn with_sink(sink: crate::writer::Sink) -> Self {
        Self::with_writer(std::rc::Rc::new(std::cell::RefCell::new(
            TerminalWriter::with_sink(sink),
        )))
    }

    pub(crate) fn with_writer(writer: std::rc::Rc<std::cell::RefCell<TerminalWriter>>) -> Self {
        Self {
            output: Vec::with_capacity(64 * 1024),
            queued_control: Vec::new(),
            overlay_mask: Vec::new(),
            match_mask: Vec::new(),
            match_styles: [None, None],
            current_line: None,
            selection_mask: Vec::new(),
            selection_style: None,
            selection_trim: None,
            painted: PaneMap::default(),
            headers: PaneMap::default(),
            picker_cards: PaneMap::default(),
            cards: PaneMap::default(),
            sidebar_rows: Vec::new(),
            status_rows: Vec::new(),
            status_geometry: None,
            status_source: None,
            damage: PaneMap::default(),
            browser_placements: PaneMap::default(),
            browser_painted: PaneMap::default(),
            last_title: String::new(),
            border_chrome: None,
            mode_tree: chooser::ModeTree::default(),
            pane_modes_painted: PaneMap::default(),
            kitty: KittyBridge::default(),
            writer,
            control_replay: Vec::new(),
            paint_tail: Vec::new(),
            terminal_colours: crate::tty::terminal_colours(),
            terminal_defaults: TmuxStyle::default(),
            terminal_sgr: None,
            blank_is_default: false,
            default_blank: PaneMap::default(),
            tty: None,
            settled: None,
            cursor_style: Some(0),
            cursor_colour: None,
            cursor_colour_known: true,
            cursor_written: false,
        }
    }

    fn note_terminal_colours(&mut self) {
        let colours = crate::tty::terminal_colours();
        if self.terminal_colours != colours {
            self.terminal_colours = colours;
            self.invalidate();
        }
    }

    pub fn invalidate(&mut self) {
        self.tty = None;
        self.settled = None;
        self.terminal_sgr = None;
        self.paint_tail.clear();
        self.painted.clear();
        self.default_blank.clear();
        self.headers.clear();
        self.picker_cards.clear();
        self.cards.clear();
        self.sidebar_rows.clear();
        self.status_rows.clear();
        self.status_geometry = None;
        self.status_source = None;
        self.damage.clear();
        self.browser_painted.clear();
        self.border_chrome = None;
        self.kitty.invalidate();
    }

    pub fn note_frame(&mut self, pane: PaneId, damage: FrameDamage) {
        if let Some(pending) = self.damage.get_mut(&pane) {
            merge_damage(pending, damage);
        } else {
            self.damage.insert(pane, damage);
        }
    }

    pub fn queue_control(&mut self, output: Vec<u8>) {
        self.queued_control.extend(output);
    }

    pub fn enable_kitty_graphics(&mut self) {
        self.kitty.enable(&mut self.queued_control);
    }

    pub fn set_frame_transport(&mut self, transport: FrameTransport) {
        self.kitty
            .set_transport(transport, &mut self.queued_control);
    }

    pub fn disable_kitty_graphics(&mut self) {
        self.kitty.disable();
        self.browser_placements.clear();
        self.browser_painted.clear();
    }

    pub fn install_kitty_image(&mut self, image: KittyImageData) {
        let _ = self.kitty.install(image, &mut self.queued_control);
    }

    pub fn install_browser_frame(&mut self, update: BrowserFrameUpdate) -> usize {
        let pane = update.image.pane;
        let transmitted = self.kitty.install(update.image, &mut self.queued_control);
        self.browser_placements.insert(pane, update.placement);
        transmitted
    }

    pub fn resize_browser_frame(&mut self, pane: PaneId, cells: (u16, u16)) {
        let Some(placement) = self.browser_placements.get_mut(&pane) else {
            return;
        };
        placement.grid_cols = u32::from(cells.0);
        placement.grid_rows = u32::from(cells.1);
    }

    pub fn remove_browser_frame(&mut self, pane: PaneId) {
        self.browser_placements.remove(&pane);
        self.browser_painted.remove(&pane);
        self.kitty
            .remove_images(pane, &[BROWSER_IMAGE_ID], &mut self.queued_control);
    }

    fn browser_frame_live(&self, pane: PaneId) -> bool {
        self.browser_placements.contains_key(&pane)
    }

    pub fn remove_kitty_images(&mut self, pane: PaneId, image_ids: &[u32]) {
        self.kitty
            .remove_images(pane, image_ids, &mut self.queued_control);
    }

    pub fn forget_pane(&mut self, pane: PaneId) {
        self.painted.remove(&pane);
        self.default_blank.remove(&pane);
        self.headers.remove(&pane);
        self.picker_cards.remove(&pane);
        self.cards.remove(&pane);
        self.damage.remove(&pane);
        self.browser_placements.remove(&pane);
        self.browser_painted.remove(&pane);
        self.kitty.remove_pane(pane, &mut self.queued_control);
    }

    pub fn reset_kitty_images(&mut self) {
        self.browser_placements.clear();
        self.browser_painted.clear();
        self.kitty.reset(&mut self.queued_control);
    }

    pub fn paint(&mut self, model: &Model, force: bool) -> io::Result<()> {
        if force {
            self.paint_tail.clear();
        }
        self.note_terminal_colours();
        self.forget_default_blank_under_overlays(model);
        self.output.clear();
        self.output.extend_from_slice(PAINT_BEGIN);
        self.begin_tty_state();
        let floating_input = model.menu.is_some() || model.confirm.is_some();
        if model.status.title != self.last_title {
            if !model.status.title.is_empty() {
                self.output.extend_from_slice(b"\x1b]2;");
                self.output.extend(
                    model
                        .status
                        .title
                        .bytes()
                        .filter(|byte| *byte >= 0x20 && *byte != 0x7f),
                );
                self.output.push(0x07);
            }
            self.last_title.clone_from(&model.status.title);
        }
        if model.choose_tree.is_some() || model.choose_buffer.is_some() {
            match self.paint_mode_tree(model) {
                chooser::ModeTreePaint::Painted(cursor) => {
                    self.emit_queued_control();
                    self.kitty.suspend(&mut self.output);
                    self.place_mode_tree_cursor(Some(cursor));
                }
                chooser::ModeTreePaint::Pending => {
                    self.emit_queued_control();
                    self.restore_mode_tree_cursor_now();
                }
                chooser::ModeTreePaint::Unavailable => {
                    self.default_blank.clear();
                    self.paint_chooser(model);
                    self.emit_queued_control();
                    self.kitty.suspend(&mut self.output);
                    self.hide_cursor();
                }
            }
        } else {
            let cleared_to_default = force;
            if force {
                self.default_blank.clear();
            }
            if cleared_to_default {
                self.output.extend_from_slice(b"\x1b[0m\x1b[2J");
                fill_unpainted_cells(&mut self.output, model);
            } else if force {
                clear_screen(&mut self.output, model.appearance.background);
            }
            self.paint_workspace(model, force, cleared_to_default);
            if model.sidebar_visible() {
                self.paint_sidebar(model, force);
            }
            self.paint_status_block(model, force);
            self.paint_pane_prompt(model);
            self.emit_queued_control();
            if !floating_input {
                self.reconcile_kitty_images(model);
                self.place_active_cursor(model);
            }
        }
        if model.menu.is_some() {
            self.kitty.suspend(&mut self.output);
            self.paint_menu(model);
            self.place_menu_cursor(model);
        } else if model.confirm.is_some() {
            self.kitty.suspend(&mut self.output);
            if model.sidebar_visible() {
                self.paint_sidebar(model, true);
            } else {
                self.paint_status_block(model, true);
            }
            self.place_confirm_cursor(model);
        }

        self.update_terminal_cursor(model);
        self.output.extend_from_slice(b"\x1b[?2026l");
        self.flush_output()
    }

    pub fn paint_frames(&mut self, model: &Model) -> io::Result<()> {
        self.note_terminal_colours();
        self.forget_default_blank_under_overlays(model);
        self.output.clear();
        self.output.extend_from_slice(PAINT_BEGIN);
        self.begin_tty_state();
        if model.choose_tree.is_none()
            && model.choose_buffer.is_none()
            && model.menu.is_none()
            && model.confirm.is_none()
        {
            self.paint_workspace(model, false, false);
            self.paint_status_area(model);
            self.paint_pane_prompt(model);
            self.emit_queued_control();
            self.reconcile_kitty_images(model);
            self.place_active_cursor(model);
        } else {
            self.emit_queued_control();
            self.kitty.suspend(&mut self.output);
            if model.menu.is_some() {
                self.place_menu_cursor(model);
            } else if model.confirm.is_some() {
                self.place_confirm_cursor(model);
            } else {
                self.restore_mode_tree_cursor(model);
            }
        }
        self.update_terminal_cursor(model);
        self.output.extend_from_slice(b"\x1b[?2026l");
        self.flush_output()
    }

    fn begin_tty_state(&mut self) {
        self.tty = self.settled.map(|state| {
            (
                TtyState {
                    visible: false,
                    ..state
                },
                self.output.len(),
            )
        });
    }

    fn tty_state(&self) -> Option<TtyState> {
        self.tty
            .filter(|(_, at)| *at == self.output.len())
            .map(|(state, _)| state)
    }

    fn set_tty_state(&mut self, state: TtyState) {
        self.tty = Some((state, self.output.len()));
    }

    fn move_to(&mut self, column: u16, row: u16) {
        let state = self.tty_state();
        if state.is_some_and(|state| state.cursor == Some((column, row))) {
            return;
        }
        write_cursor_position(&mut self.output, column, row);
        self.set_tty_state(TtyState {
            cursor: Some((column, row)),
            sgr_reset: state.is_some_and(|state| state.sgr_reset),
            visible: state.is_some_and(|state| state.visible),
        });
    }

    fn show_cursor(&mut self) {
        let state = self.tty_state();
        self.output.extend_from_slice(SHOW_CURSOR);
        if let Some(state) = state {
            self.set_tty_state(TtyState {
                visible: true,
                ..state
            });
        }
    }

    fn settle(&mut self) -> Option<TtyState> {
        let previous = self.settled.take();
        let end = self.output.len().checked_sub(PAINT_END.len());
        self.settled = self
            .tty
            .take()
            .filter(|(_, at)| Some(*at) == end && self.output.ends_with(PAINT_END))
            .map(|(state, _)| state);
        previous
    }

    fn flush_output(&mut self) -> io::Result<()> {
        let previous = self.settle();
        let control = std::mem::take(&mut self.control_replay);
        if control.is_empty()
            && previous.is_some_and(|state| state.visible)
            && self.settled.is_some_and(|state| state.visible)
            && let Some(text) = self
                .output
                .strip_prefix(PAINT_BEGIN)
                .and_then(|body| body.strip_suffix(PAINT_END))
                .and_then(|body| body.strip_suffix(SHOW_CURSOR))
            && text.iter().all(|byte| *byte >= 0x20 && *byte != 0x7f)
        {
            let text = PAINT_BEGIN.len()..PAINT_BEGIN.len() + text.len();
            self.output.truncate(text.end);
            self.output.drain(..text.start);
            self.paint_tail.clear();
            if self.output.is_empty() {
                return Ok(());
            }
            if self.writer.borrow_mut().submit(&mut self.output)? == Submission::Dropped {
                self.invalidate();
                self.forget_terminal_cursor();
            }
            return Ok(());
        }
        let body = self
            .output
            .strip_prefix(PAINT_BEGIN)
            .and_then(|body| body.strip_suffix(PAINT_END));
        if control.is_empty()
            && let Some(body) = body
            && cursor_only(body)
            && self.paint_tail.ends_with(body)
        {
            self.output.clear();
            return Ok(());
        }
        self.paint_tail.clear();
        if let Some(body) = body {
            let tail = body.len().saturating_sub(PAINT_TAIL_BYTES);
            self.paint_tail.extend_from_slice(&body[tail..]);
        }
        let submission = self.writer.borrow_mut().submit(&mut self.output)?;
        match submission {
            Submission::Queued => Ok(()),
            Submission::Dropped => {
                if !control.is_empty() {
                    self.queued_control.splice(0..0, control);
                }
                self.invalidate();
                self.forget_terminal_cursor();
                Ok(())
            }
        }
    }

    fn emit_queued_control(&mut self) {
        self.control_replay.extend_from_slice(&self.queued_control);
        self.output.append(&mut self.queued_control);
    }

    pub fn pause(&mut self, paused: bool) {
        self.output.clear();
        self.writer.borrow_mut().pause(paused);
        self.invalidate();
        self.cursor_style = Some(0);
        self.cursor_colour = None;
        self.cursor_colour_known = true;
        self.cursor_written = false;
    }

    fn forget_terminal_cursor(&mut self) {
        if self.cursor_written {
            self.cursor_style = None;
            self.cursor_colour_known = false;
        }
    }

    fn update_terminal_cursor(&mut self, model: &Model) {
        self.write_terminal_cursor(
            model,
            crate::tty::terminal_carries("cstyle"),
            crate::tty::terminal_carries("ccolour"),
        );
    }

    fn write_terminal_cursor(&mut self, model: &Model, writes_style: bool, writes_colour: bool) {
        let wanted = prompt_cursor(model);
        let colour = wanted
            .colour
            .and_then(|colour| cursor_colour_rgb(colour, &model.status.theme));
        if writes_colour && (!self.cursor_colour_known || self.cursor_colour != colour) {
            match colour {
                Some(rgb) => {
                    let _ = write!(
                        self.output,
                        "\x1b]12;rgb:{:02x}/{:02x}/{:02x}\x07",
                        rgb >> 16 & 0xff,
                        rgb >> 8 & 0xff,
                        rgb & 0xff
                    );
                }
                None => self.output.extend_from_slice(b"\x1b]112\x07"),
            }
            self.cursor_colour = colour;
            self.cursor_colour_known = true;
            self.cursor_written = true;
        }
        let style = wanted.style.min(6);
        if writes_style && self.cursor_style != Some(style) {
            let _ = write!(self.output, "\x1b[{} q", if style == 0 { 2 } else { style });
            if style != 0 {
                crate::tty::note_cursor_style_sent();
            }
            self.cursor_style = Some(style);
            self.cursor_written = true;
        }
    }

    pub fn discard_queued_paints(&mut self) {
        self.tty = None;
        self.settled = None;
        self.output.clear();
        self.control_replay.clear();
        self.writer.borrow_mut().abandon();
    }

    fn forget_default_blank_under_overlays(&mut self, model: &Model) {
        if model.menu.is_some()
            || model.confirm.is_some()
            || model.display_panes.is_some()
            || pane_prompt_target(model).is_some()
        {
            self.default_blank.clear();
        }
    }

    fn paint_workspace(&mut self, model: &Model, force: bool, cleared_to_default: bool) {
        let lines = model.pane_border_lines();
        let indicators = model.pane_border_indicators();
        let border_changed = self.border_chrome.as_mut().is_none_or(|cached| {
            let changed = cached.0 != model.pane_border_status()
                || cached.1 != lines
                || cached.2 != indicators
                || !Arc::ptr_eq(&cached.3, &model.status)
                    && (cached.3.pane_borders != model.status.pane_borders
                        || cached.3.theme != model.status.theme);
            if !changed {
                cached.3 = Arc::clone(&model.status);
            }
            changed
        });
        let force = force || border_changed;
        if border_changed {
            self.border_chrome = Some((
                model.pane_border_status(),
                lines,
                indicators,
                Arc::clone(&model.status),
            ));
        }
        if force {
            let cells = divider_cells(&model.layout.dividers);
            let mut border = BTreeMap::new();
            for divider in &model.layout.dividers {
                let highlighted = divider.highlighted;
                let fallback = if highlighted {
                    model.appearance.link_color
                } else {
                    model.appearance.foreground
                };
                let color = divider
                    .style_pane
                    .and_then(|pane| model.pane_border_colour(pane, highlighted))
                    .map_or(fallback, |colour| {
                        resolve_tmux_colour(colour, fallback, &model.appearance)
                    });
                let style = divider
                    .style_pane
                    .and_then(|pane| model.pane_border_style(pane));
                let mut sgr = Vec::new();
                if let Some(style) = &style {
                    write_border_sgr(&mut sgr, style, &model.appearance);
                } else {
                    write_colored_sgr(&mut sgr, color, model.appearance.background);
                }
                let sgr = Rc::new(sgr);
                let index = divider.style_pane.and_then(|pane| model.pane_index(pane));
                for row in divider.rect.y..divider.rect.y.saturating_add(divider.rect.height) {
                    for column in divider.rect.x..divider.rect.x.saturating_add(divider.rect.width)
                    {
                        let mut mask = 0;
                        for (present, bit) in [
                            (cells.contains(&(column.wrapping_sub(1), row)), BORDER_L),
                            (cells.contains(&(column.saturating_add(1), row)), BORDER_R),
                            (cells.contains(&(column, row.wrapping_sub(1))), BORDER_U),
                            (cells.contains(&(column, row.saturating_add(1))), BORDER_D),
                        ] {
                            if present {
                                mask |= bit;
                            }
                        }
                        let cell_type = cell_type_of(mask);
                        let glyph = border_arrow(model, indicators, column, row)
                            .map_or_else(|| border_glyph(lines, cell_type, index), str::to_owned);
                        border.insert((row, column), (Rc::clone(&sgr), glyph));
                    }
                }
            }
            write_border_runs(&mut self.output, &border, model.size.columns);
        }

        if force {
            self.paint_empty_window(model);
        }
        let tiled = model.layout.tiled().len();
        let mut touched: Vec<Rect> = Vec::new();
        for (index, entry) in model.layout.panes.iter().enumerate() {
            let float = index
                .checked_sub(tiled)
                .and_then(|index| model.layout.floats.get(index));
            let entry_force = force
                || float.is_some_and(|float| {
                    touched.iter().any(|rect| rects_overlap(*rect, float.frame))
                });
            let mark = self.output.len();
            if let Some(float) = float
                && entry_force
            {
                self.paint_float_border(float, model);
            }
            self.paint_entry(
                model,
                entry,
                entry_force,
                cleared_to_default && float.is_none(),
            );
            if self.output.len() != mark {
                touched.push(float.map_or_else(|| entry_extent(entry), |float| float.frame));
            }
        }
        self.painted
            .retain(|pane, _| model.layout.panes.iter().any(|entry| entry.pane == *pane));
        self.damage
            .retain(|pane, _| model.layout.panes.iter().any(|entry| entry.pane == *pane));
        self.picker_cards
            .retain(|pane, _| model.layout.panes.iter().any(|entry| entry.pane == *pane));
        self.cards
            .retain(|pane, _| model.layout.panes.iter().any(|entry| entry.pane == *pane));
        self.browser_painted
            .retain(|pane, _| model.layout.panes.iter().any(|entry| entry.pane == *pane));
        self.pane_modes_painted
            .retain(|pane, _| model.layout.panes.iter().any(|entry| entry.pane == *pane));

        if let Some(display) = &model.display_panes {
            for indicator in &display.indicators {
                if let Some(entry) = model.pane_rect(indicator.pane) {
                    for write in
                        crate::overlay::display_panes_writes(display, indicator, entry.content())
                    {
                        let mut line = StyledLine::from_segments(vec![write.segment]);
                        line.resolve_theme(&model.status.theme);
                        write_styled_text(
                            &mut self.output,
                            write.column,
                            write.row,
                            &line,
                            model.appearance.foreground,
                            model.appearance.background,
                            &model.appearance,
                        );
                    }
                }
            }
        }
    }

    fn paint_entry(
        &mut self,
        model: &Model,
        entry: &PaneRect,
        force: bool,
        cleared_to_default: bool,
    ) {
        let active = model.active_pane() == Some(entry.pane);
        let floating = model.is_float(entry.pane);
        let lines = model.pane_lines(entry.pane);
        let Some(pane) = model.pane_snapshot(entry.pane) else {
            return;
        };
        let header_changed = if entry.border_status.is_on() {
            let header = pane.border_status_text.clone();
            let changed = self.headers.get(&entry.pane) != Some(&header);
            if force || changed {
                self.paint_border_status_row(
                    entry.status_row(),
                    &header,
                    active,
                    entry.pane,
                    lines,
                    model.pane_index(entry.pane),
                    model,
                );
                self.headers.insert(entry.pane, header);
            }
            changed
        } else {
            self.headers.remove(&entry.pane).is_some()
        };
        let content = entry.content();
        let known_blank = !floating && self.default_blank.remove(&entry.pane) == Some(content);
        let browser_live = matches!(pane.kind, PaneKindSnapshot::Browser(_))
            && self.browser_frame_live(entry.pane);
        let browser_state_changed = if matches!(pane.kind, PaneKindSnapshot::Browser(_)) {
            self.browser_painted.insert(entry.pane, browser_live) != Some(browser_live)
        } else {
            self.browser_painted.remove(&entry.pane);
            false
        };
        match &pane.kind {
            PaneKindSnapshot::Terminal => {
                self.picker_cards.remove(&entry.pane);
                let pane_mode_changed = self
                    .pane_modes_painted
                    .insert(entry.pane, pane.mode.is_some())
                    != Some(pane.mode.is_some());
                if let Some(pane_mode) = pane.mode.as_ref() {
                    self.damage.remove(&entry.pane);
                    self.painted.remove(&entry.pane);
                    self.paint_pane_mode(pane_mode, entry, model);
                    return;
                }
                let force = force || pane_mode_changed;
                if let Some(viewport) = model.pane_viewport(entry.pane) {
                    let viewport = &*source_viewport(viewport, entry.source);
                    let damage = self
                        .damage
                        .remove(&entry.pane)
                        .map(|damage| source_damage(damage, entry.source.1));
                    let mode = crate::mode_view::presentation(model, entry.pane, viewport);
                    self.selection_style = mode.and_then(|mode| {
                        crate::mode_view::resolved_style(&mode.selection_style, &model.status.theme)
                    });
                    self.selection_trim = mode
                        .filter(|mode| !mode.vi_keys)
                        .and_then(|_| crate::mode_view::emacs_selection_trim(viewport));
                    self.match_styles = mode.map_or([None, None], |mode| {
                        [&mode.match_style, &mode.current_match_style].map(|style| {
                            crate::mode_view::resolved_style(style, &model.status.theme)
                        })
                    });
                    self.current_line = mode.and_then(|mode| {
                        crate::mode_view::current_line(mode, viewport, &model.status.theme)
                    });
                    let gutter =
                        mode.and_then(|mode| crate::mode_view::line_number_gutter(mode, viewport));
                    let body = gutter.map_or(content, |gutter| gutter.body(content));
                    self.blank_is_default = cleared_to_default || known_blank;
                    self.terminal_defaults = model.pane_window_style(entry.pane);
                    self.paint_terminal(entry.pane, viewport, body, force, damage.as_ref());
                    self.terminal_defaults = TmuxStyle::default();
                    self.blank_is_default = false;
                    if let Some(mode) = mode {
                        self.paint_copy_chrome(mode, gutter, viewport, content, model);
                    }
                    self.selection_style = None;
                    self.selection_trim = None;
                    self.match_styles = [None, None];
                    self.current_line = None;
                } else if floating {
                    self.painted.remove(&entry.pane);
                    if force {
                        self.paint_background(content, model);
                    }
                } else if cleared_to_default || known_blank {
                    self.painted.remove(&entry.pane);
                    self.default_blank.insert(entry.pane, content);
                }
            }
            PaneKindSnapshot::Browser(_) if browser_live => {
                self.picker_cards.remove(&entry.pane);
                if force || header_changed || browser_state_changed {
                    self.paint_background(content, model);
                }
            }
            PaneKindSnapshot::Picker if active => {
                let card_state = (content, model.picker_selection);
                if force || self.picker_cards.get(&entry.pane) != Some(&card_state) {
                    self.paint_picker_card(content, model.picker_selection, model);
                    self.picker_cards.insert(entry.pane, card_state);
                }
            }
            kind => {
                self.picker_cards.remove(&entry.pane);
                let (label, detail) = placeholder_text(kind);
                let card = (content, label, pane.title.clone(), detail);
                if force
                    || header_changed
                    || browser_state_changed
                    || self.cards.get(&entry.pane) != Some(&card)
                {
                    self.paint_card(content, card.1, &card.2, &card.3, model);
                    self.cards.insert(entry.pane, card);
                }
            }
        }
    }

    fn paint_float_border(&mut self, float: &FloatFrame, model: &Model) {
        let style = model.pane_border_style(float.pane);
        let fallback = if float.active {
            model.appearance.link_color
        } else {
            model.appearance.foreground
        };
        let color = model
            .pane_border_colour(float.pane, float.active)
            .map_or(fallback, |colour| {
                resolve_tmux_colour(colour, fallback, &model.appearance)
            });
        let mut sgr = Vec::new();
        if let Some(style) = &style {
            write_border_sgr(&mut sgr, style, &model.appearance);
        } else {
            write_colored_sgr(&mut sgr, color, model.appearance.background);
        }
        let sgr = Rc::new(sgr);
        let index = model.pane_index(float.pane);
        let border: BTreeMap<(u16, u16), (Rc<Vec<u8>>, String)> = float
            .border_cells()
            .into_iter()
            .map(|(column, row, cell_type)| {
                (
                    (row, column),
                    (Rc::clone(&sgr), border_glyph(float.lines, cell_type, index)),
                )
            })
            .collect();
        write_border_runs(&mut self.output, &border, model.size.columns);
    }

    fn paint_empty_window(&mut self, model: &Model) {
        let Some(window) = model.window() else {
            return;
        };
        if window.zoomed_pane.is_some() || !matches!(window.layout, zz_protocol::LayoutNode::Empty)
        {
            return;
        }
        let canvas = model.canvas();
        if canvas.width == 0 {
            return;
        }
        let mut line = StyledLine::parsed(&format!(
            "#[bg=themedarkgrey]{}",
            " ".repeat(usize::from(canvas.width))
        ));
        line.resolve_theme(&model.status.theme);
        for row in canvas.y..canvas.y.saturating_add(canvas.height) {
            write_styled_text(
                &mut self.output,
                canvas.x,
                row,
                &line,
                model.appearance.foreground,
                model.appearance.background,
                &model.appearance,
            );
        }
    }

    fn reconcile_kitty_images(&mut self, model: &Model) {
        let browser_placements = &self.browser_placements;
        let cropped: HashMap<PaneId, Vec<zz_terminal::KittyPlacement>> = model
            .layout
            .panes
            .iter()
            .filter(|entry| entry.source != (0, 0))
            .filter_map(|entry| {
                let viewport = model.pane_viewport(entry.pane)?;
                Some((
                    entry.pane,
                    source_placements(&viewport.kitty_placements, entry.source),
                ))
            })
            .collect();
        let tiled = model.layout.tiled().len();
        let panes = model
            .layout
            .panes
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let front = if index < tiled { 0 } else { index - tiled + 1 };
                let covered = model.layout.floats.get(front..).is_some_and(|floats| {
                    floats
                        .iter()
                        .any(|float| rects_overlap(float.frame, entry.content()))
                });
                if covered {
                    return None;
                }
                let pane = model.pane_snapshot(entry.pane)?;
                match &pane.kind {
                    PaneKindSnapshot::Terminal => {
                        let viewport = model.pane_viewport(entry.pane)?;
                        let placements = cropped
                            .get(&entry.pane)
                            .map_or_else(|| viewport.kitty_placements.as_ref(), Vec::as_slice);
                        Some((entry.pane, entry.content(), placements))
                    }
                    PaneKindSnapshot::Browser(_) => {
                        let placement = browser_placements.get(&entry.pane)?;
                        Some((entry.pane, entry.content(), std::slice::from_ref(placement)))
                    }
                    _ => None,
                }
            });
        self.kitty.reconcile(panes, &mut self.output);
    }

    fn paint_terminal(
        &mut self,
        pane: PaneId,
        viewport: &TerminalViewport,
        rect: Rect,
        force: bool,
        damage: Option<&FrameDamage>,
    ) {
        let defaults = [self.terminal_defaults.fg, self.terminal_defaults.bg];
        if !force
            && self.painted.get(&pane).is_some_and(|previous| {
                previous.rect == rect
                    && previous.defaults == defaults
                    && previous.viewport.columns == viewport.columns
                    && previous.viewport.rows == viewport.rows
                    && previous.viewport.foreground == viewport.foreground
                    && previous.viewport.background == viewport.background
                    && Arc::ptr_eq(&previous.viewport.cells, &viewport.cells)
                    && Arc::ptr_eq(&previous.viewport.overlays, &viewport.overlays)
                    && Arc::ptr_eq(&previous.viewport.dictionary, &viewport.dictionary)
            })
        {
            return;
        }
        let previous = self.painted.remove(&pane);
        let structural_change = previous.as_ref().is_none_or(|previous| {
            previous.rect != rect
                || previous.defaults != defaults
                || previous.viewport.columns != viewport.columns
                || previous.viewport.rows != viewport.rows
                || previous.viewport.foreground != viewport.foreground
                || previous.viewport.background != viewport.background
        });
        if force || structural_change {
            for row in 0..rect.height {
                self.blit_row(viewport, row, rect);
            }
        } else if let Some(FrameDamage::Rows(rows)) = damage {
            for row in rows.iter().copied().filter(|row| *row < rect.height) {
                if let Some(columns) = previous.as_ref().map_or_else(
                    || Some(0..rect.width),
                    |previous| changed_columns(&previous.viewport, viewport, row, rect.width),
                ) {
                    self.blit_columns(viewport, row, rect, columns);
                }
            }
        } else {
            for row in 0..rect.height {
                if let Some(columns) = previous.as_ref().map_or_else(
                    || Some(0..rect.width),
                    |previous| changed_columns(&previous.viewport, viewport, row, rect.width),
                ) {
                    self.blit_columns(viewport, row, rect, columns);
                }
            }
        }
        self.painted.insert(
            pane,
            PaintedPane {
                viewport: viewport.clone(),
                rect,
                defaults,
            },
        );
    }

    fn write_match_sgr(&mut self, matched: u8) -> bool {
        let Some(style) = matched
            .checked_sub(1)
            .and_then(|index| self.match_styles.get(usize::from(index)))
            .and_then(Option::as_ref)
        else {
            return false;
        };
        self.output.extend_from_slice(b"\x1b[0m");
        write_selection_sgr(&mut self.output, style);
        true
    }

    fn write_terminal_sgr(
        &mut self,
        style: PackedStyle,
        reverse: bool,
        viewport: &TerminalViewport,
        reset: bool,
    ) -> bool {
        let key = (
            style,
            reverse,
            [viewport.foreground, viewport.background],
            [self.terminal_defaults.fg, self.terminal_defaults.bg],
            crate::tty::terminal_colours(),
        );
        if self
            .terminal_sgr
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            let mut bytes = self
                .terminal_sgr
                .take()
                .map_or_else(Vec::new, |(_, mut bytes)| {
                    bytes.clear();
                    bytes
                });
            write_sgr(
                &mut bytes,
                style,
                reverse,
                viewport.foreground,
                viewport.background,
                &self.terminal_defaults,
            );
            self.terminal_sgr = Some((key, bytes));
        }
        let Some((_, bytes)) = &self.terminal_sgr else {
            return false;
        };
        let equivalent = sgr_is_reset(bytes);
        if !(reset && equivalent) {
            self.output.extend_from_slice(bytes);
        }
        equivalent
    }

    fn blit_row(&mut self, viewport: &TerminalViewport, row: u16, rect: Rect) {
        self.blit_columns(viewport, row, rect, 0..rect.width);
    }

    fn blit_columns(
        &mut self,
        viewport: &TerminalViewport,
        row: u16,
        rect: Rect,
        columns: std::ops::Range<u16>,
    ) {
        if rect.width == 0 {
            return;
        }
        self.overlay_mask.resize(usize::from(rect.width), false);
        self.overlay_mask.fill(false);
        self.selection_mask.resize(usize::from(rect.width), false);
        self.selection_mask.fill(false);
        self.match_mask.resize(usize::from(rect.width), 0);
        self.match_mask.fill(0);
        for overlay in viewport
            .overlays
            .iter()
            .filter(|overlay| overlay.row == row && overlay.kind() != OverlayKind::CopyCursor)
        {
            let start = usize::from(overlay.start.min(rect.width));
            let end = usize::from(overlay.end.min(rect.width));
            let matched = match overlay.kind() {
                OverlayKind::SearchMatch => 1,
                OverlayKind::SearchCurrent => 2,
                _ => 0,
            };
            if matched != 0 && self.match_styles[usize::from(matched - 1)].is_some() {
                for cell in &mut self.match_mask[start..end] {
                    *cell = (*cell).max(matched);
                }
            }
            if overlay.kind() == OverlayKind::Selection && self.selection_style.is_some() {
                let end = match self.selection_trim {
                    Some((trim_row, trim_end)) if trim_row == row => {
                        end.min(usize::from(trim_end)).max(start)
                    }
                    _ => end,
                };
                self.selection_mask[start..end].fill(true);
            } else {
                self.overlay_mask[start..end].fill(true);
            }
        }
        let selection_style = self.selection_style.take();
        let line_style = self
            .current_line
            .as_ref()
            .filter(|(line, _)| *line == row)
            .map(|(_, style)| style.clone());

        let default_style = viewport.styles().first().copied().unwrap_or_else(|| {
            PackedStyle::new(
                viewport.foreground,
                viewport.background,
                None,
                0,
                UnderlineStyle::None,
            )
        });
        let grounded_defaults =
            self.terminal_defaults.fg.is_some() || self.terminal_defaults.bg.is_some();
        let clear_from = if grounded_defaults || columns.end < rect.width || line_style.is_some() {
            None
        } else {
            trailing_clear(
                viewport,
                row,
                rect.width,
                default_style,
                &self.overlay_mask,
                &self.selection_mask,
            )
            .map(|start| start.max(columns.start))
        };
        let ground = match default_style.background_class() {
            ColourClass::Default => None,
            ColourClass::Resolved if default_style.background() == viewport.background => None,
            class => Some(class),
        };
        let erase_from = clear_from.filter(|_| !self.blank_is_default || ground.is_some());
        if clear_from == Some(0) && erase_from.is_none() {
            self.selection_style = selection_style;
            return;
        }
        self.move_to(
            rect.x.saturating_add(columns.start),
            rect.y.saturating_add(row),
        );
        let entry = self.tty_state();
        let mut sgr_reset = entry.is_some_and(|state| state.sgr_reset);
        let visible = entry.is_some_and(|state| state.visible);
        let mut current_style = None;
        let mut terminal_column = columns.start;
        for column in columns.start..clear_from.unwrap_or(columns.end) {
            let cell = viewport.cell(row, column).unwrap_or(PackedCell::EMPTY);
            let style = viewport.style(cell).unwrap_or(default_style);
            let reverse = self.overlay_mask[usize::from(column)];
            let selected = self.selection_mask[usize::from(column)];
            let matched = self.match_mask[usize::from(column)];
            if matches!(cell.width(), CellWidth::SpacerTail | CellWidth::SpacerHead) {
                if terminal_column <= column {
                    if terminal_column != column {
                        write_cursor_position(
                            &mut self.output,
                            rect.x.saturating_add(column),
                            rect.y.saturating_add(row),
                        );
                    }
                    if current_style != Some((style, reverse, selected, matched)) {
                        sgr_reset = self.write_terminal_sgr(style, reverse, viewport, sgr_reset);
                        if let Some(line) = &line_style {
                            write_line_sgr(&mut self.output, line);
                            sgr_reset = false;
                        }
                        if self.write_match_sgr(matched) {
                            sgr_reset = false;
                        }
                        if selected && let Some(selection) = &selection_style {
                            write_selection_sgr(&mut self.output, selection);
                            sgr_reset = false;
                        }
                        current_style = Some((style, reverse, selected, matched));
                    }
                    self.output.push(b' ');
                    terminal_column = column.saturating_add(1);
                }
                continue;
            }
            if terminal_column != column {
                write_cursor_position(
                    &mut self.output,
                    rect.x.saturating_add(column),
                    rect.y.saturating_add(row),
                );
            }
            if current_style != Some((style, reverse, selected, matched)) {
                sgr_reset = self.write_terminal_sgr(style, reverse, viewport, sgr_reset);
                if let Some(line) = &line_style {
                    write_line_sgr(&mut self.output, line);
                    sgr_reset = false;
                }
                if self.write_match_sgr(matched) {
                    sgr_reset = false;
                }
                if selected && let Some(selection) = &selection_style {
                    write_selection_sgr(&mut self.output, selection);
                    sgr_reset = false;
                }
                current_style = Some((style, reverse, selected, matched));
            }
            let advance = if cell.width() == CellWidth::Wide {
                2
            } else {
                1
            };
            if advance == 2 && column.saturating_add(1) >= rect.width {
                self.output.push(b' ');
                terminal_column = column.saturating_add(1);
                continue;
            }
            write_glyph(&mut self.output, viewport.glyph(cell), advance);
            terminal_column = column.saturating_add(advance);
        }
        let mut reset = !sgr_reset;
        if let Some(start) = erase_from {
            if terminal_column != start {
                write_cursor_position(
                    &mut self.output,
                    rect.x.saturating_add(start),
                    rect.y.saturating_add(row),
                );
            }
            self.output.extend_from_slice(b"\x1b[0m");
            reset = ground.is_some();
            if let Some(class) = ground {
                write_cell_ground(
                    &mut self.output,
                    class,
                    default_style.background(),
                    viewport.background,
                    Ground::Background,
                );
            }
            self.output.extend_from_slice(b"\x1b[");
            write_decimal(&mut self.output, rect.width - start);
            self.output.push(b'X');
            terminal_column = start;
        }
        if reset {
            self.output.extend_from_slice(b"\x1b[0m");
        }
        self.set_tty_state(TtyState {
            cursor: (terminal_column < rect.width).then(|| {
                (
                    rect.x.saturating_add(terminal_column),
                    rect.y.saturating_add(row),
                )
            }),
            sgr_reset: true,
            visible,
        });
        self.selection_style = selection_style;
    }

    fn paint_line_numbers(
        &mut self,
        mode: &zz_protocol::ModePresentation,
        gutter: crate::mode_view::LineNumberGutter,
        rect: Rect,
        model: &Model,
    ) {
        let width = gutter.width.min(rect.width);
        for row in 0..rect.height {
            let (text, current) = gutter.label(row);
            let segment =
                crate::mode_view::line_number_segment(mode, text, current, &model.status.theme);
            let mut line = StyledLine::from_segments(vec![segment]);
            line.resolve_theme(&model.status.theme);
            let line = line.truncate(usize::from(width));
            write_styled_text(
                &mut self.output,
                rect.x,
                rect.y.saturating_add(row),
                &line,
                model.appearance.foreground,
                model.appearance.background,
                &model.appearance,
            );
        }
    }

    fn paint_copy_chrome(
        &mut self,
        mode: &zz_protocol::ModePresentation,
        gutter: Option<crate::mode_view::LineNumberGutter>,
        viewport: &TerminalViewport,
        rect: Rect,
        model: &Model,
    ) {
        let Some(gutter) = gutter else {
            self.paint_mode_position(mode, viewport, rect, model);
            return;
        };
        self.paint_line_numbers(mode, gutter, rect, model);
        self.paint_mode_position(mode, viewport, gutter.body(rect), model);
        if let Some((column, row)) = gutter.cursor
            && row < rect.height
            && column >= gutter.content_width(rect.width)
        {
            let line = StyledLine::from_segments(vec![StyledSegment {
                text: "$".to_owned(),
                style: TmuxStyle {
                    fg: Some(TmuxColour::Default),
                    bg: Some(TmuxColour::Default),
                    ..TmuxStyle::default()
                },
            }]);
            write_styled_text(
                &mut self.output,
                rect.x.saturating_add(rect.width.saturating_sub(1)),
                rect.y.saturating_add(row),
                &line,
                model.appearance.foreground,
                model.appearance.background,
                &model.appearance,
            );
        }
    }

    fn paint_mode_position(
        &mut self,
        mode: &zz_protocol::ModePresentation,
        viewport: &TerminalViewport,
        rect: Rect,
        model: &Model,
    ) {
        if rect.height == 0 {
            return;
        }
        self.blit_row(viewport, 0, rect);
        for (column, segments) in crate::mode_view::position_runs(mode, rect.width) {
            let mut line = StyledLine::from_segments(segments);
            line.resolve_theme(&model.status.theme);
            write_styled_text(
                &mut self.output,
                rect.x.saturating_add(column),
                rect.y,
                &line,
                viewport.foreground,
                viewport.background,
                &model.appearance,
            );
        }
    }

    fn paint_border_status_row(
        &mut self,
        rect: Rect,
        expanded: &str,
        active: bool,
        pane: PaneId,
        lines: PaneBorderLines,
        index: Option<u32>,
        model: &Model,
    ) {
        if rect.height == 0 || rect.width == 0 {
            return;
        }
        let fallback = if active {
            model.appearance.link_color
        } else {
            model.appearance.foreground
        };
        let color = model
            .pane_border_colour(pane, active)
            .map_or(fallback, |colour| {
                resolve_tmux_colour(colour, fallback, &model.appearance)
            });
        let glyph = border_glyph(lines, CELL_LR, index);
        let lead = 2.min(rect.width);
        let width = rect.width.saturating_sub(lead);
        let underlay = vec![glyph.clone(); usize::from(width)];
        let composed = zz_client::compose_status_row_over(expanded, &underlay, "");
        let mut line = StyledLine::default();
        for _ in 0..lead {
            line.push_plain(&glyph);
        }
        line.append(&StyledLine::from_segments(composed.segments));
        let line = line.truncate(usize::from(rect.width));
        if let Some(style) = model.pane_border_style(pane) {
            write_styled_text_over(
                &mut self.output,
                rect.x,
                rect.y,
                &line,
                &style,
                &model.appearance,
            );
            return;
        }
        write_styled_text(
            &mut self.output,
            rect.x,
            rect.y,
            &line,
            color,
            model.appearance.background,
            &model.appearance,
        );
    }

    fn paint_background(&mut self, rect: Rect, model: &Model) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        let line = " ".repeat(usize::from(rect.width));
        for row in 0..rect.height {
            write_colored_text(
                &mut self.output,
                rect.x,
                rect.y.saturating_add(row),
                &line,
                model.appearance.foreground,
                model.appearance.background,
            );
        }
    }
}

fn changed_columns(
    previous: &TerminalViewport,
    current: &TerminalViewport,
    row: u16,
    width: u16,
) -> Option<std::ops::Range<u16>> {
    if (!Arc::ptr_eq(&previous.dictionary, &current.dictionary)
        && previous.dictionary.as_ref() != current.dictionary.as_ref())
        || previous
            .overlays
            .iter()
            .filter(|overlay| overlay.row == row)
            .ne(current.overlays.iter().filter(|overlay| overlay.row == row))
    {
        return Some(0..width);
    }
    let (Some(before), Some(after)) = (
        viewport_row(previous, row, width),
        viewport_row(current, row, width),
    ) else {
        return Some(0..width);
    };
    if before.len() != after.len() {
        return Some(0..width);
    }
    let mut cells = before.iter().zip(after);
    let start = cells.position(|(before, after)| before.bits() != after.bits())?;
    let end = cells
        .rposition(|(before, after)| before.bits() != after.bits())
        .map_or(start + 1, |last| start + last + 2);
    if before[start..end]
        .iter()
        .chain(&after[start..end])
        .any(|cell| cell.width() != CellWidth::Narrow)
    {
        Some(0..width)
    } else {
        Some(u16::try_from(start).ok()?..u16::try_from(end).ok()?)
    }
}

fn viewport_row(viewport: &TerminalViewport, row: u16, width: u16) -> Option<&[PackedCell]> {
    viewport
        .row(row)
        .map(|cells| &cells[..cells.len().min(usize::from(width))])
}

fn source_viewport(
    viewport: &TerminalViewport,
    source: (u16, u16),
) -> std::borrow::Cow<'_, TerminalViewport> {
    if source == (0, 0) {
        return std::borrow::Cow::Borrowed(viewport);
    }
    let (left, top) = source;
    let columns = viewport.columns.saturating_sub(left);
    let rows = viewport.rows.saturating_sub(top);
    let mut cropped = viewport.clone();
    cropped.columns = columns;
    cropped.rows = rows;
    cropped.cells = (top..viewport.rows)
        .filter_map(|row| viewport.row(row))
        .flat_map(|cells| cells.iter().skip(usize::from(left)).copied())
        .collect();
    cropped.overlays = viewport
        .overlays
        .iter()
        .filter(|overlay| overlay.row >= top && overlay.end > left)
        .map(|overlay| {
            let mut overlay = *overlay;
            overlay.row -= top;
            overlay.start = overlay.start.saturating_sub(left);
            overlay.end -= left;
            overlay
        })
        .collect();
    cropped.kitty_placements = Arc::from([]);
    cropped.cursor = viewport.cursor.and_then(|cursor| {
        let column = cursor.column().checked_sub(left)?;
        let row = cursor.row().checked_sub(top)?;
        Some(zz_terminal::Cursor::new(
            column,
            row,
            cursor.visible(),
            cursor.blinking(),
            cursor.at_wide_tail(),
            cursor.style(),
            cursor.color(),
        ))
    });
    std::borrow::Cow::Owned(cropped)
}

fn source_damage(damage: FrameDamage, top: u16) -> FrameDamage {
    match damage {
        FrameDamage::Rows(rows) if top != 0 => FrameDamage::Rows(
            rows.into_iter()
                .filter_map(|row| row.checked_sub(top))
                .collect(),
        ),
        damage => damage,
    }
}

fn source_placements(
    placements: &[zz_terminal::KittyPlacement],
    source: (u16, u16),
) -> Vec<zz_terminal::KittyPlacement> {
    placements
        .iter()
        .filter_map(|placement| {
            let mut placement = placement.clone();
            placement.viewport_col -= i32::from(source.0);
            placement.viewport_row -= i32::from(source.1);
            (placement.viewport_col >= 0 && placement.viewport_row >= 0).then_some(placement)
        })
        .collect()
}

const fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x.saturating_add(b.width)
        && b.x < a.x.saturating_add(a.width)
        && a.y < b.y.saturating_add(b.height)
        && b.y < a.y.saturating_add(a.height)
}

fn entry_extent(entry: &PaneRect) -> Rect {
    let status = entry.status_row();
    if status.height == 0 {
        return entry.rect;
    }
    let top = entry.rect.y.min(status.y);
    let bottom = entry
        .rect
        .y
        .saturating_add(entry.rect.height)
        .max(status.y.saturating_add(1));
    Rect {
        x: entry.rect.x,
        y: top,
        width: entry.rect.width,
        height: bottom - top,
    }
}

fn divider_cells(dividers: &[Divider]) -> BTreeSet<(u16, u16)> {
    let mut cells = BTreeSet::new();
    for divider in dividers {
        for row in divider.rect.y..divider.rect.y.saturating_add(divider.rect.height) {
            for column in divider.rect.x..divider.rect.x.saturating_add(divider.rect.width) {
                cells.insert((column, row));
            }
        }
    }
    cells
}

fn border_arrow(
    model: &Model,
    indicators: PaneBorderIndicators,
    column: u16,
    row: u16,
) -> Option<&'static str> {
    if !indicators.arrows() {
        return None;
    }
    let marked = model.layout.panes.iter().any(|entry| {
        let rect = entry.rect;
        let vertical = column == rect.x.saturating_add(1)
            && ((rect.y > 0 && row == rect.y.saturating_sub(1))
                || row == rect.y.saturating_add(rect.height));
        let horizontal = row == rect.y.saturating_add(1)
            && ((rect.x > 0 && column == rect.x.saturating_sub(1))
                || column == rect.x.saturating_add(rect.width));
        vertical || horizontal
    });
    if !marked {
        return None;
    }
    let rect = model.pane_rect(model.active_pane()?)?.rect;
    let within_columns =
        column >= rect.x.saturating_sub(1) && column <= rect.x.saturating_add(rect.width);
    let within_rows = row >= rect.y.saturating_sub(1) && row <= rect.y.saturating_add(rect.height);
    if column == rect.x.saturating_add(rect.width) && within_rows {
        return Some("←");
    }
    if rect.x > 0 && column == rect.x.saturating_sub(1) && within_rows {
        return Some("→");
    }
    if row == rect.y.saturating_add(rect.height) && within_columns {
        return Some("↑");
    }
    if rect.y > 0 && row == rect.y.saturating_sub(1) && within_columns {
        return Some("↓");
    }
    None
}

fn placeholder_text(kind: &PaneKindSnapshot) -> (&'static str, String) {
    match kind {
        PaneKindSnapshot::Picker => ("Pane picker", "choose a pane in the zz app".to_owned()),
        PaneKindSnapshot::Browser(browser) => ("Browser", browser.url().to_owned()),
        PaneKindSnapshot::Agent(agent) => (
            "Agent",
            agent.cwd.as_ref().map_or_else(
                || agent.provider.label().to_owned(),
                |cwd| format!("{} · {}", agent.provider.label(), cwd.display()),
            ),
        ),
        PaneKindSnapshot::Editor(editor) => (
            "Editor",
            editor.path.clone().unwrap_or_else(|| editor.cwd.clone()),
        ),
        PaneKindSnapshot::Terminal => ("Terminal", String::new()),
        PaneKindSnapshot::View(view) => (
            view.content.kind_name(),
            format!("{} · open it in the zz app", view.title),
        ),
        PaneKindSnapshot::Other { kind } => {
            ("Unknown pane", zz_client::navigation::newer_pane_text(kind))
        }
    }
}

struct StatusOverlay {
    front: StyledLine,
    fill: Option<TmuxColour>,
}

#[derive(Clone, Copy)]
struct FloatingBorder {
    top_left: &'static str,
    top_right: &'static str,
    bottom_left: &'static str,
    bottom_right: &'static str,
    horizontal: &'static str,
    vertical: &'static str,
    separator_left: &'static str,
    separator_right: &'static str,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::state::ClientMessage;
    use zz_protocol::MenuState;
    use zz_terminal::{CellWidth, Cursor, CursorStyle, SessionStatus, TerminalDictionary};

    fn styled_viewport() -> TerminalViewport {
        let mut viewport = TerminalViewport::blank(3, 1, SessionStatus::Running);
        let first = PackedStyle::new(
            Color::rgb(255, 0, 0),
            viewport.background,
            None,
            zz_terminal::ATTR_BOLD,
            UnderlineStyle::None,
        );
        let second = PackedStyle::new(
            Color::rgb(0, 255, 0),
            viewport.background,
            None,
            0,
            UnderlineStyle::None,
        );
        viewport.dictionary = Arc::new(TerminalDictionary::from_shared(
            Arc::from([first, second]),
            Arc::from([0]),
            Arc::from([]),
        ));
        viewport.cells = Arc::from([
            PackedCell::new('a' as u32, 0, CellWidth::Narrow),
            PackedCell::new('b' as u32, 0, CellWidth::Narrow),
            PackedCell::new('c' as u32, 1, CellWidth::Narrow),
        ]);
        viewport
    }

    #[test]
    fn a_terminal_under_its_colours_takes_the_pin_nearest_one() {
        assert_eq!(downgrade_palette(42, 16_777_216), 42);
        assert_eq!(downgrade_palette(42, 256), 42);
        assert_eq!(downgrade_palette(42, 16), 10);
        assert_eq!(downgrade_palette(42, 8), 2);
        assert_eq!(downgrade_palette(9, 16), 9);
        assert_eq!(downgrade_palette(9, 8), 1);
        assert_eq!(downgrade_palette(12, 8), 4);
        assert_eq!(downgrade_palette(1, 8), 1);
        let dark = Color::rgb(10, 20, 30);
        assert_eq!(colour_find_rgb(dark), 233);
        assert_eq!(downgrade_palette(colour_find_rgb(dark), 256), 233);
        assert_eq!(downgrade_palette(colour_find_rgb(dark), 8), 0);
        assert_eq!(colour_find_rgb(Color::rgb(0x5f, 0x87, 0xaf)), 67);
        assert_eq!(colour_find_rgb(Color::rgb(255, 255, 255)), 231);
        assert_eq!(colour_find_rgb(Color::rgb(0, 0, 0)), 16);
    }

    #[test]
    fn a_terminal_that_answered_nothing_leaves_every_colour_alone() {
        assert_eq!(crate::tty::terminal_colours(), None);
        let mut output = Vec::new();
        write_rgb_ground(&mut output, Color::rgb(10, 20, 30), Ground::Foreground);
        write_palette_ground(&mut output, 42, Ground::Foreground);
        assert_eq!(output, b"\x1b[38;2;10;20;30m\x1b[38;5;42m");
    }

    #[test]
    fn sgr_changes_once_per_style_run() {
        let viewport = styled_viewport();
        let mut renderer = Renderer::new();
        renderer.blit_row(
            &viewport,
            0,
            Rect {
                x: 0,
                y: 0,
                width: 3,
                height: 1,
            },
        );
        let output = String::from_utf8(renderer.output).unwrap();

        assert_eq!(output.matches("\x1b[38;2;").count(), 2);
        assert!(output.contains("ab"));
        assert!(output.contains('c'));
    }

    #[test]
    fn pane_cells_reach_the_terminal_in_the_class_the_program_wrote() {
        let default_foreground = Color::rgb(216, 222, 233);
        let default_background = Color::rgb(16, 19, 24);
        let sgr = |foreground: ColourClass, background: ColourClass, colour: Color| {
            let style = PackedStyle::new(colour, colour, None, 0, UnderlineStyle::None)
                .with_classes(foreground, background);
            let mut output = Vec::new();
            write_sgr(
                &mut output,
                style,
                false,
                default_foreground,
                default_background,
                &TmuxStyle::default(),
            );
            String::from_utf8(output).unwrap()
        };
        let red = Color::rgb(205, 0, 0);
        assert_eq!(
            sgr(ColourClass::Palette(1), ColourClass::Palette(9), red),
            "\x1b[0m\x1b[31m\x1b[101m"
        );
        assert_eq!(
            sgr(ColourClass::Palette(42), ColourClass::Palette(200), red),
            "\x1b[0m\x1b[38;5;42m\x1b[48;5;200m"
        );
        assert_eq!(
            sgr(ColourClass::Rgb, ColourClass::Default, default_foreground),
            "\x1b[0m\x1b[38;2;216;222;233m\x1b[49m"
        );
        assert_eq!(
            sgr(ColourClass::Default, ColourClass::Rgb, red),
            "\x1b[0m\x1b[39m\x1b[48;2;205;0;0m"
        );
        assert_eq!(
            sgr(
                ColourClass::Resolved,
                ColourClass::Resolved,
                default_foreground
            ),
            "\x1b[0m\x1b[39m\x1b[48;2;216;222;233m"
        );
    }

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
        assert!(output.rfind('$').unwrap() > output.rfind("POS").unwrap());
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
    fn styled_lines_send_named_and_indexed_colours_the_way_the_pin_does() {
        let appearance = TerminalAppearance::default();
        let line = StyledLine::parsed(
            "#[fg=red,bg=#010203,bold,underscore,reverse]X#[fg=colour42,nobold,nounderscore,noreverse]Y#[fg=brightgreen]Z",
        );
        let mut output = Vec::new();
        write_styled_text(
            &mut output,
            0,
            0,
            &line,
            appearance.background,
            appearance.foreground,
            &appearance,
        );
        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("\x1b[1m"));
        assert!(output.contains("\x1b[4m"));
        assert!(output.contains("\x1b[7m"));
        assert!(output.contains("\x1b[31m"), "{output:?}");
        assert!(output.contains("\x1b[48;2;1;2;3m"), "{output:?}");
        assert!(output.contains("\x1b[38;5;42m"), "{output:?}");
        assert!(output.contains("\x1b[92m"), "{output:?}");
        let red = appearance.palette[1];
        assert!(
            !output.contains(&format!("38;2;{};{};{}", red.r, red.g, red.b)),
            "a named colour must not be resolved against zz's own palette: {output:?}"
        );
        assert!(output.contains('X'));
        assert!(output.contains('Y'));
        assert!(output.contains('Z'));
        assert!(!output.contains("#["));
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
        assert!(output.starts_with("\x1b[4;8H"), "{output:?}");
        assert!(output.contains('9'), "{output:?}");
        assert!(
            !output.contains("line") && !output.contains("padding"),
            "{output:?}"
        );

        let mut shorter = after.clone();
        Arc::make_mut(&mut shorter.cells)[5..10].fill(PackedCell::EMPTY);
        renderer.output.clear();
        renderer.paint_terminal(PaneId(1), &shorter, rect, false, None);
        let output = String::from_utf8(renderer.output).unwrap();
        assert!(output.starts_with("\x1b[4;8H"), "{output:?}");
        assert!(output.contains("     "), "{output:?}");
        assert!(!output.contains("padding"), "{output:?}");
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

    fn waiting_pane_model() -> Model {
        let mut model = block_model(40, 10);
        let session = zz_protocol::SessionId(1);
        let window = zz_protocol::WindowId(1);
        let pane = PaneId(3);
        model.attached_session = Some(session);
        model.update_snapshot(Arc::new(zz_protocol::MuxSnapshot {
            generation: 1,
            sessions: vec![zz_protocol::SessionSnapshot {
                id: session,
                name: "s".to_owned(),
                active_window: window,
                windows: vec![zz_protocol::WindowSnapshot {
                    id: window,
                    index: 0,
                    name: "w".to_owned(),
                    automatic_rename: true,
                    active_pane: pane,
                    zoomed_pane: None,
                    layout: zz_protocol::LayoutNode::Pane(pane),
                    panes: std::collections::BTreeMap::from([(
                        pane,
                        zz_protocol::PaneSnapshot {
                            id: pane,
                            title: "shell".to_owned(),
                            kind: PaneKindSnapshot::Terminal,
                            synchronized_input: false,
                            bell: false,
                            dead: false,
                            dead_status: None,
                            border_colour: None,
                            active_border_colour: None,
                            border_status_text: String::new(),
                            mode: None,
                            status: None,
                            summary: zz_protocol::PaneSummary::default(),
                        },
                    )]),
                    layout_dump: String::new(),
                    visible_layout_dump: String::new(),
                    status_label: String::new(),
                    activity: false,
                    silence: false,
                    pane_border_status: PaneBorderStatus::Off,
                    pane_border_lines: PaneBorderLines::Single,
                    pane_border_indicators: PaneBorderIndicators::Colour,
                    pane_order: vec![pane],
                    pane_z_order: Vec::new(),
                    floating: Vec::new(),
                    modal: None,
                    sx: 0,
                    sy: 0,
                    pane_base_index: 0,
                }],
                viewers: Vec::new(),
            }],
            focused_window: Some(window),
        }));
        model
    }

    #[test]
    fn a_terminal_pane_waiting_for_its_first_frame_paints_nothing() {
        let model = waiting_pane_model();
        let mut renderer = Renderer::new();
        renderer.paint_workspace(&model, true, false);
        let output = String::from_utf8(renderer.output).unwrap();
        assert!(!output.contains("waiting for frame"), "{output:?}");
        assert!(!output.contains('┌'), "{output:?}");
    }

    fn erases(output: &str) -> usize {
        output
            .split("\x1b[")
            .skip(1)
            .filter(|sequence| {
                let digits = sequence.bytes().take_while(u8::is_ascii_digit).count();
                digits > 0 && sequence.as_bytes().get(digits) == Some(&b'X')
            })
            .count()
    }

    #[test]
    fn blank_pane_rows_are_never_written_over_a_default_clear() {
        let mut model = waiting_pane_model();
        let pane = PaneId(3);
        let (send, receive) = std::sync::mpsc::channel();
        let mut renderer = Renderer::with_sink(Box::new(move |bytes| {
            send.send(bytes.to_vec()).unwrap();
            Ok(())
        }));
        let wait = std::time::Duration::from_secs(5);
        let read = || String::from_utf8(receive.recv_timeout(wait).unwrap()).unwrap();
        renderer.paint(&model, true).unwrap();
        let clear = read();
        assert!(clear.contains("\x1b[0m\x1b[2J"), "{clear:?}");
        assert_eq!(erases(&clear), 0, "{clear:?}");

        let content = model.layout.panes[0].content();
        let mut viewport =
            TerminalViewport::blank(content.width, content.height, SessionStatus::Running);
        let mut cells = viewport.cells.to_vec();
        cells[0] = PackedCell::new('x' as u32, 0, CellWidth::Narrow);
        viewport.cells = Arc::from(cells);
        model.viewports.insert(pane, viewport);
        renderer.note_frame(pane, FrameDamage::All);
        renderer.paint_frames(&model).unwrap();
        let first_frame = read();
        assert!(first_frame.contains('x'), "{first_frame:?}");
        assert_eq!(erases(&first_frame), 0, "{first_frame:?}");
        assert!(!first_frame.contains("\x1b[0m\x1b[0m"), "{first_frame:?}");

        renderer.invalidate();
        renderer.note_frame(pane, FrameDamage::All);
        renderer.paint_frames(&model).unwrap();
        let unknown = read();
        assert_eq!(
            erases(&unknown),
            usize::from(content.height),
            "rows over a screen the renderer no longer knows are erased: {unknown:?}"
        );
        assert!(!unknown.contains("\x1b[0m\x1b[0m"), "{unknown:?}");
    }

    #[test]
    fn cells_no_painter_covers_keep_the_theme_background() {
        let mut model = waiting_pane_model();
        let mut snapshot = (*model.snapshot).clone();
        let window = snapshot.sessions[0].windows.first_mut().unwrap();
        window.panes.get_mut(&PaneId(3)).unwrap().mode = Some(zz_protocol::PaneMode::Clock {
            time: "12:00".to_owned(),
            colour: "blue".to_owned(),
        });
        model.update_snapshot(Arc::new(snapshot));
        let mut output = Vec::new();
        fill_unpainted_cells(&mut output, &model);
        let output = String::from_utf8(output).unwrap();
        let content = model.layout.panes[0].content();
        assert_eq!(erases(&output), usize::from(content.height), "{output:?}");
        let background = model.appearance.background;
        assert!(output.starts_with(&format!(
            "\x1b[48;2;{};{};{}m",
            background.r, background.g, background.b
        )));
    }

    #[test]
    fn a_paint_that_only_puts_the_cursor_back_writes_nothing() {
        let model = waiting_pane_model();
        let (send, receive) = std::sync::mpsc::channel();
        let mut renderer = Renderer::with_sink(Box::new(move |bytes| {
            send.send(bytes.to_vec()).unwrap();
            Ok(())
        }));
        let wait = std::time::Duration::from_secs(5);
        renderer.paint(&model, true).unwrap();
        let first = receive.recv_timeout(wait).unwrap();
        renderer.paint(&model, false).unwrap();
        assert!(
            receive
                .recv_timeout(std::time::Duration::from_millis(100))
                .is_err(),
            "the cursor is already where {first:?} left it"
        );
        renderer.paint(&model, true).unwrap();
        assert!(
            receive.recv_timeout(wait).is_ok(),
            "a forced paint is always written"
        );
        renderer.invalidate();
        renderer.paint(&model, false).unwrap();
        assert!(
            receive.recv_timeout(wait).is_ok(),
            "an invalidated screen repaints"
        );
    }

    #[test]
    fn only_cursor_placement_counts_as_cursor_only() {
        assert!(cursor_only(b"\x1b[1;31H\x1b[?25h"));
        assert!(cursor_only(b"\x1b[?25l"));
        assert!(cursor_only(b""));
        assert!(!cursor_only(b"\x1b[1;31Hx"));
        assert!(!cursor_only(b"\x1b[0m\x1b[1;1H"));
        assert!(!cursor_only(b"\x1b[2J"));
    }

    #[test]
    fn border_runs_place_the_cursor_once_per_run_and_the_rendition_once_per_change() {
        let red = Rc::new(b"\x1b[0;31m".to_vec());
        let green = Rc::new(b"\x1b[0;32m".to_vec());
        let cells = BTreeMap::from([
            ((0, 0), (Rc::clone(&red), "─".to_owned())),
            ((0, 1), (Rc::clone(&red), "─".to_owned())),
            ((0, 2), (Rc::clone(&red), "┬".to_owned())),
            ((1, 2), (Rc::new(b"\x1b[0;31m".to_vec()), "│".to_owned())),
            ((2, 2), (green, "│".to_owned())),
        ]);
        let mut output = Vec::new();
        write_border_runs(&mut output, &cells, 80);
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "\x1b[1;1H\x1b[0;31m──┬\x08\n│\x08\n\x1b[0;32m│\x1b[0m"
        );
        let mut output = Vec::new();
        write_border_runs(&mut output, &cells, 3);
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "\x1b[1;1H\x1b[0;31m──┬\x1b[2;3H│\x1b[3;3H\x1b[0;32m│\x1b[0m",
            "a cell in the last column leaves the cursor waiting to wrap"
        );
    }

    fn block_model(columns: u16, rows: u16) -> Model {
        let core = zz_client::ClientCore::new();
        let endpoint = zz_daemon_client::Endpoint::parse("unix:///tmp/zz-render-test.sock")
            .expect("test endpoint");
        Model::new(
            &core,
            crate::tty::TerminalSize {
                columns,
                rows,
                cell_width_px: 8,
                cell_height_px: 16,
            },
            "host".to_owned(),
            "host".to_owned(),
            endpoint.clone(),
            endpoint,
            Vec::new(),
        )
    }

    fn menu_state() -> MenuState {
        MenuState {
            left: 5,
            top: 2,
            width: 20,
            height: 5,
            client_columns: 40,
            client_rows: 12,
            cell_width_px: 8,
            cell_height_px: 16,
            title: "Actions".to_owned(),
            style: "fg=white,bg=blue".to_owned(),
            selected_style: "fg=black,bg=yellow,bold".to_owned(),
            border_style: "fg=red".to_owned(),
            border_lines: PopupBorderLines::Single,
            items: vec![
                Some(zz_protocol::MenuItem {
                    name: "界 item".to_owned(),
                    key: Some("q".to_owned()),
                    annotation: Some("q".to_owned()),
                    enabled: true,
                }),
                None,
                Some(zz_protocol::MenuItem {
                    name: "Disabled".to_owned(),
                    key: None,
                    annotation: None,
                    enabled: false,
                }),
            ],
            selected: Some(0),
            stay_open: false,
            mouse_keys: false,
        }
    }

    #[test]
    fn floating_border_tables_cover_every_published_line_style() {
        assert_eq!(floating_border(PopupBorderLines::Single).top_left, "┌");
        assert_eq!(
            floating_border(PopupBorderLines::Double).separator_left,
            "╠"
        );
        assert_eq!(floating_border(PopupBorderLines::Heavy).bottom_right, "┛");
        assert_eq!(floating_border(PopupBorderLines::Simple).vertical, "|");
        assert_eq!(floating_border(PopupBorderLines::Rounded).top_right, "╮");
        assert_eq!(floating_border(PopupBorderLines::Padded).horizontal, " ");
        assert_eq!(floating_border(PopupBorderLines::None).top_left, " ");
    }

    #[test]
    fn menu_paints_title_rows_shortcuts_selection_and_disabled_style() {
        let mut model = block_model(40, 12);
        model.menu = Some(menu_state());
        model.menu_selection = Some(0);
        let mut renderer = Renderer::new();
        renderer.paint_menu(&model);
        let output = String::from_utf8(renderer.output).unwrap();

        assert!(output.contains("\x1b[3;6H"), "{output:?}");
        assert!(output.contains("Actions"));
        assert!(output.contains("界 item"));
        assert!(output.contains("(q)"));
        assert!(output.contains('┌'));
        assert!(output.contains('├'));
        assert!(output.contains('┘'));
        assert!(output.contains("\x1b[1m"));
        assert!(output.contains("\x1b[2m"));
    }

    #[test]
    fn sidebar_refocus_hides_the_cursor_without_changing_visibility_or_geometry() {
        let mut model = block_model(120, 30);
        let pane = PaneId(1);
        attach_one_pane(&mut model, pane);
        let mut viewport = TerminalViewport::blank(120, 29, SessionStatus::Running);
        viewport.cursor = Some(Cursor::new(
            0,
            0,
            true,
            false,
            false,
            CursorStyle::Bar,
            viewport.foreground,
        ));
        model.viewports.insert(pane, viewport);
        let mut renderer = Renderer::new();

        assert!(model.focus_sidebar());
        assert_eq!(model.status_area(), (29, 91));
        renderer.place_active_cursor(&model);
        assert_eq!(renderer.output, b"\x1b[?25l");

        model.focus_active_pane();
        assert!(model.sidebar_visible());
        renderer.output.clear();
        renderer.place_active_cursor(&model);
        assert!(renderer.output.ends_with(b"\x1b[?25h"));

        assert!(!model.focus_sidebar());
        assert!(model.sidebar_focused());
        assert!(model.sidebar_visible());
        assert_eq!(model.status_area(), (29, 91));
        renderer.output.clear();
        renderer.place_active_cursor(&model);
        assert_eq!(renderer.output, b"\x1b[?25l");

        assert!(model.hide_sidebar());
        assert!(!model.sidebar_focused());
        assert!(!model.sidebar_visible());
        assert_eq!(model.status_area(), (0, 120));
        renderer.output.clear();
        renderer.place_active_cursor(&model);
        assert!(renderer.output.ends_with(b"\x1b[?25h"));
    }

    #[test]
    fn menu_hides_the_workspace_cursor() {
        let mut model = block_model(40, 12);
        model.menu = Some(menu_state());
        let mut renderer = Renderer::new();
        renderer.place_active_cursor(&model);
        assert_eq!(renderer.output, b"\x1b[?25l");
    }

    fn block_status(rows: Vec<&str>, customized: bool) -> zz_protocol::StatusLine {
        zz_protocol::StatusLine {
            rows: rows.into_iter().map(str::to_owned).collect(),
            customized,
            ..zz_protocol::StatusLine::default()
        }
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
    fn browser_frame_switches_the_renderer_between_card_and_live_modes() {
        let pane = PaneId(7);
        let mut renderer = Renderer::new();
        assert!(!renderer.browser_frame_live(pane));
        renderer.enable_kitty_graphics();
        renderer.install_browser_frame(BrowserFrameUpdate {
            image: KittyImageData {
                pane,
                image_id: BROWSER_IMAGE_ID,
                generation: 4,
                width: 2,
                height: 1,
                bytes: vec![0, 0, 255, 255, 0, 255, 0, 255],
            },
            placement: KittyPlacement {
                image_id: BROWSER_IMAGE_ID,
                image_generation: 4,
                layer: zz_terminal::KittyLayer::AboveText,
                viewport_col: 0,
                viewport_row: 0,
                absolute_row: 0,
                cell_offset_x: 0,
                cell_offset_y: 0,
                grid_cols: 2,
                grid_rows: 1,
                pixel_width: 2,
                pixel_height: 1,
                source_rect: None,
            },
        });

        assert!(renderer.browser_frame_live(pane));
        assert!(String::from_utf8_lossy(&renderer.queued_control).contains("\x1b_Ga=t"));
        renderer.resize_browser_frame(pane, (4, 3));
        assert_eq!(renderer.browser_placements[&pane].grid_cols, 4);
        assert_eq!(renderer.browser_placements[&pane].grid_rows, 3);

        renderer.remove_browser_frame(pane);
        assert!(!renderer.browser_frame_live(pane));
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
    fn the_published_theme_defaults_are_the_table_the_client_used_to_hard_code() {
        let published = zz_protocol::ThemeColours::default();
        for (index, name) in DARK_THEME_COLOURS.iter().enumerate() {
            let packed = zz_terminal::parse_x11_color(name)
                .unwrap_or_else(|| panic!("{name} is an X11 colour"))
                .packed();
            assert_eq!(
                published.slot(u8::try_from(index).expect("ten slots")),
                Some(TmuxColour::Rgb(packed)),
                "slot {index} ({name}) drifted from the wire default"
            );
        }
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
    fn a_status_row_message_takes_the_cursor_and_a_prompt_keeps_it() {
        let mut model = block_model(40, 10);
        model.set_status(block_status(vec!["ROW"], false));
        assert!(!client_message_hides_the_cursor(&model));
        model.client_message = Some(ClientMessage::local("a message"));
        assert!(client_message_hides_the_cursor(&model));

        let mut sidebar_model = block_model(120, 10);
        sidebar_model.focus_sidebar();
        assert!(sidebar_model.sidebar_visible());
        sidebar_model.set_status(block_status(vec!["ROW"], false));
        sidebar_model.client_message = Some(ClientMessage::local("a sidebar message"));
        assert!(!client_message_hides_the_cursor(&sidebar_model));

        model.command_prompt = Some(zz_protocol::CommandPromptState {
            prompt: ":".to_owned(),
            input: "list".to_owned(),
            cursor: 4,
            kind: zz_protocol::CommandPromptKind::Command,
            history: Vec::new(),
            prompt_type: zz_protocol::CommandPromptType::Command,
            mode: zz_protocol::CommandPromptMode::Text,
            no_freeze: false,
            pane: None,
            command_mode: false,
            prompt_cursor: zz_protocol::PromptCursor::default(),
        });
        let mut renderer = Renderer::new();
        renderer.place_active_cursor(&model);
        let output = String::from_utf8(renderer.output).unwrap();
        assert!(output.ends_with("\x1b[?25h"), "{output:?}");
    }

    #[test]
    fn a_client_prompt_sends_its_cursor_and_takes_it_back_when_it_closes() {
        let mut model = block_model(40, 10);
        model.set_status(block_status(vec!["ROW"], false));
        let prompt = zz_protocol::CommandPromptState {
            prompt: ":".to_owned(),
            input: "abc".to_owned(),
            cursor: 3,
            kind: zz_protocol::CommandPromptKind::Command,
            history: Vec::new(),
            prompt_type: zz_protocol::CommandPromptType::Command,
            mode: zz_protocol::CommandPromptMode::Text,
            no_freeze: false,
            pane: None,
            command_mode: false,
            prompt_cursor: zz_protocol::PromptCursor {
                style: 6,
                colour: Some(TmuxColour::Basic(1)),
            },
        };
        let mut renderer = Renderer::new();
        let written = |renderer: &mut Renderer, model: &Model| {
            renderer.write_terminal_cursor(model, true, true);
            String::from_utf8(std::mem::take(&mut renderer.output)).unwrap()
        };
        assert_eq!(written(&mut renderer, &model), "");

        model.command_prompt = Some(prompt.clone());
        assert_eq!(
            written(&mut renderer, &model),
            "\x1b]12;rgb:80/00/00\x07\x1b[6 q"
        );
        assert_eq!(written(&mut renderer, &model), "");

        model.command_prompt = Some(zz_protocol::CommandPromptState {
            command_mode: true,
            prompt_cursor: zz_protocol::PromptCursor {
                style: 3,
                colour: Some(TmuxColour::Theme(6)),
            },
            ..prompt.clone()
        });
        let theme = model.status.theme.0[6];
        let TmuxColour::Rgb(rgb) = theme else {
            panic!("the default theme slot is rgb: {theme:?}");
        };
        assert_eq!(
            written(&mut renderer, &model),
            format!(
                "\x1b]12;rgb:{:02x}/{:02x}/{:02x}\x07\x1b[3 q",
                rgb >> 16 & 0xff,
                rgb >> 8 & 0xff,
                rgb & 0xff
            )
        );

        model.command_prompt = Some(zz_protocol::CommandPromptState {
            pane: Some(PaneId(1)),
            ..prompt.clone()
        });
        assert_eq!(written(&mut renderer, &model), "\x1b]112\x07\x1b[2 q");

        model.command_prompt = None;
        model.confirm = Some(zz_protocol::ConfirmState {
            prompt: "sure? ".to_owned(),
            confirm_key: b'y',
            default_yes: false,
            prompt_cursor: zz_protocol::PromptCursor {
                style: 1,
                colour: None,
            },
        });
        assert_eq!(written(&mut renderer, &model), "\x1b[1 q");
        model.confirm = None;
        assert_eq!(written(&mut renderer, &model), "\x1b[2 q");

        model.command_prompt = Some(prompt);
        renderer.write_terminal_cursor(&model, false, false);
        assert!(renderer.output.is_empty());
        assert_eq!(
            written(&mut renderer, &model),
            "\x1b]12;rgb:80/00/00\x07\x1b[6 q",
            "a cursor the terminal could not take yet is sent once it can"
        );
    }

    #[test]
    fn a_cursor_style_is_restored_at_teardown_even_when_its_reset_was_only_built() {
        let mut model = block_model(40, 10);
        model.set_status(block_status(vec!["ROW"], false));
        model.command_prompt = Some(zz_protocol::CommandPromptState {
            prompt: ":".to_owned(),
            input: String::new(),
            cursor: 0,
            kind: zz_protocol::CommandPromptKind::Command,
            history: Vec::new(),
            prompt_type: zz_protocol::CommandPromptType::Command,
            mode: zz_protocol::CommandPromptMode::Text,
            no_freeze: false,
            pane: None,
            command_mode: false,
            prompt_cursor: zz_protocol::PromptCursor {
                style: 6,
                colour: None,
            },
        });
        let mut renderer = Renderer::new();
        renderer.write_terminal_cursor(&model, true, true);
        model.command_prompt = None;
        renderer.write_terminal_cursor(&model, true, true);
        assert!(String::from_utf8_lossy(&renderer.output).ends_with("\x1b[2 q"));
        assert!(
            crate::tty::cursor_style_needs_restore(),
            "a reset that may never reach the terminal leaves the teardown restore armed"
        );
    }

    #[test]
    fn a_dropped_paint_sends_the_prompt_cursor_again() {
        let mut model = block_model(40, 10);
        model.set_status(block_status(vec!["ROW"], false));
        model.command_prompt = Some(zz_protocol::CommandPromptState {
            prompt: ":".to_owned(),
            input: String::new(),
            cursor: 0,
            kind: zz_protocol::CommandPromptKind::Command,
            history: Vec::new(),
            prompt_type: zz_protocol::CommandPromptType::Command,
            mode: zz_protocol::CommandPromptMode::Text,
            no_freeze: false,
            pane: None,
            command_mode: false,
            prompt_cursor: zz_protocol::PromptCursor {
                style: 6,
                colour: Some(TmuxColour::Basic(1)),
            },
        });
        let sent = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut renderer = Renderer::with_sink(Box::new({
            let sent = Arc::clone(&sent);
            move |bytes| {
                sent.lock().unwrap().extend_from_slice(bytes);
                Ok(())
            }
        }));
        let paint = |renderer: &mut Renderer, model: &Model| {
            renderer.output.extend_from_slice(PAINT_BEGIN);
            renderer.write_terminal_cursor(model, true, true);
            renderer.output.extend_from_slice(PAINT_END);
            renderer.flush_output().unwrap();
            String::from_utf8(std::mem::take(&mut *sent.lock().unwrap())).unwrap()
        };

        renderer.writer.borrow_mut().pause(true);
        assert_eq!(paint(&mut renderer, &model), "");
        renderer.writer.borrow_mut().pause(false);
        let opened = paint(&mut renderer, &model);
        assert!(opened.contains("\x1b[6 q"), "{opened:?}");
        assert!(opened.contains("\x1b]12;rgb:80/00/00\x07"), "{opened:?}");

        model.command_prompt = None;
        renderer.writer.borrow_mut().pause(true);
        assert_eq!(paint(&mut renderer, &model), "");
        renderer.writer.borrow_mut().pause(false);
        let closed = paint(&mut renderer, &model);
        assert!(closed.contains("\x1b[2 q"), "{closed:?}");
        assert!(closed.contains("\x1b]112\x07"), "{closed:?}");
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

    fn attach_one_pane(model: &mut Model, pane: PaneId) {
        let session = zz_protocol::SessionId(1);
        let window = zz_protocol::WindowId(1);
        model.attached_session = Some(session);
        model.update_snapshot(std::sync::Arc::new(zz_protocol::MuxSnapshot {
            generation: 1,
            sessions: vec![zz_protocol::SessionSnapshot {
                id: session,
                name: "s".to_owned(),
                active_window: window,
                windows: vec![zz_protocol::WindowSnapshot {
                    id: window,
                    index: 0,
                    name: "w".to_owned(),
                    automatic_rename: true,
                    active_pane: pane,
                    zoomed_pane: None,
                    layout: zz_protocol::LayoutNode::Pane(pane),
                    panes: std::collections::BTreeMap::new(),
                    layout_dump: String::new(),
                    visible_layout_dump: String::new(),
                    status_label: String::new(),
                    activity: false,
                    silence: false,
                    pane_border_status: zz_protocol::PaneBorderStatus::Off,
                    pane_border_lines: zz_protocol::PaneBorderLines::Single,
                    pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
                    pane_order: Vec::new(),
                    pane_z_order: Vec::new(),
                    floating: Vec::new(),
                    modal: None,
                    sx: 0,
                    sy: 0,
                    pane_base_index: 0,
                }],
                viewers: Vec::new(),
            }],
            focused_window: Some(window),
        }));
    }

    fn browser_under_float(float: Option<zz_protocol::FloatingPaneSnapshot>) -> Model {
        let browser = PaneId(7);
        let mut model = block_model(40, 12);
        attach_one_pane(&mut model, browser);
        let mut snapshot = (*model.snapshot).clone();
        let window = &mut snapshot.sessions[0].windows[0];
        let pane = |id: PaneId, kind: PaneKindSnapshot| zz_protocol::PaneSnapshot {
            id,
            title: String::new(),
            kind,
            synchronized_input: false,
            bell: false,
            dead: false,
            dead_status: None,
            border_colour: None,
            active_border_colour: None,
            border_status_text: String::new(),
            mode: None,
            status: None,
            summary: zz_protocol::PaneSummary::default(),
        };
        window.panes.insert(
            browser,
            pane(
                browser,
                PaneKindSnapshot::Browser(zz_protocol::BrowserDescriptor::single(
                    "about:blank".to_owned(),
                    "default".to_owned(),
                )),
            ),
        );
        if let Some(float) = float {
            window
                .panes
                .insert(float.pane, pane(float.pane, PaneKindSnapshot::Terminal));
            window.floating = vec![float];
        }
        model.update_snapshot(std::sync::Arc::new(snapshot));
        model
    }

    fn browser_placement_output(model: &Model) -> String {
        let mut renderer = Renderer::new();
        renderer.enable_kitty_graphics();
        renderer.install_browser_frame(BrowserFrameUpdate {
            image: KittyImageData {
                pane: PaneId(7),
                image_id: BROWSER_IMAGE_ID,
                generation: 1,
                width: 2,
                height: 1,
                bytes: vec![0, 0, 255, 255, 0, 255, 0, 255],
            },
            placement: KittyPlacement {
                image_id: BROWSER_IMAGE_ID,
                image_generation: 1,
                layer: zz_terminal::KittyLayer::AboveText,
                viewport_col: 0,
                viewport_row: 0,
                absolute_row: 0,
                cell_offset_x: 0,
                cell_offset_y: 0,
                grid_cols: 40,
                grid_rows: 11,
                pixel_width: 2,
                pixel_height: 1,
                source_rect: None,
            },
        });
        renderer.output.clear();
        renderer.reconcile_kitty_images(model);
        String::from_utf8_lossy(&renderer.output).into_owned()
    }

    fn tiled_under_float(
        float: zz_protocol::FloatingPaneSnapshot,
        mode: Option<zz_protocol::PaneMode>,
    ) -> Model {
        let tiled = PaneId(7);
        let mut model = block_model(40, 12);
        attach_one_pane(&mut model, tiled);
        let mut snapshot = (*model.snapshot).clone();
        let window = &mut snapshot.sessions[0].windows[0];
        let pane = |id: PaneId, mode: Option<zz_protocol::PaneMode>| zz_protocol::PaneSnapshot {
            id,
            title: String::new(),
            kind: PaneKindSnapshot::Terminal,
            synchronized_input: false,
            bell: false,
            dead: false,
            dead_status: None,
            border_colour: None,
            active_border_colour: None,
            border_status_text: String::new(),
            mode,
            status: None,
            summary: zz_protocol::PaneSummary::default(),
        };
        window.panes.insert(tiled, pane(tiled, mode));
        window.panes.insert(float.pane, pane(float.pane, None));
        window.floating = vec![float];
        model.update_snapshot(std::sync::Arc::new(snapshot));
        model
    }

    const CLIPPED: zz_protocol::FloatingPaneSnapshot = zz_protocol::FloatingPaneSnapshot {
        pane: PaneId(8),
        xoff: -3,
        yoff: 2,
        sx: 10,
        sy: 4,
        visible: true,
        border_lines: PaneBorderLines::Single,
        border_status: zz_protocol::PaneBorderStatus::Off,
    };

    fn placement_at(column: i32, columns: u32) -> KittyPlacement {
        KittyPlacement {
            image_id: 9,
            image_generation: 1,
            layer: zz_terminal::KittyLayer::AboveText,
            viewport_col: column,
            viewport_row: 0,
            absolute_row: 0,
            cell_offset_x: 0,
            cell_offset_y: 0,
            grid_cols: columns,
            grid_rows: 1,
            pixel_width: 2,
            pixel_height: 1,
            source_rect: None,
        }
    }

    fn clipped_float_placements(column: i32) -> String {
        let mut model = tiled_under_float(CLIPPED, None);
        let mut viewport = TerminalViewport::blank(10, 4, SessionStatus::Running);
        viewport.kitty_placements = std::sync::Arc::from([placement_at(column, 2)]);
        model.viewports.insert(PaneId(8), viewport);
        let mut renderer = Renderer::new();
        renderer.enable_kitty_graphics();
        renderer.install_kitty_image(KittyImageData {
            pane: PaneId(8),
            image_id: 9,
            generation: 1,
            width: 2,
            height: 1,
            bytes: vec![0, 0, 255, 255, 0, 255, 0, 255],
        });
        renderer.output.clear();
        renderer.reconcile_kitty_images(&model);
        String::from_utf8_lossy(&renderer.output).into_owned()
    }

    #[test]
    fn a_clipped_float_places_only_the_images_it_still_shows_where_it_shows_them() {
        assert!(
            !clipped_float_placements(0).contains("\x1b_Ga=p"),
            "an image in the clipped-away columns is not placed"
        );
        assert!(clipped_float_placements(5).contains("\x1b_Ga=p"));
        assert_eq!(
            source_placements(&[placement_at(5, 2)], (3, 0))[0].viewport_col,
            2
        );
    }

    #[test]
    fn a_float_paints_its_default_cells_in_its_window_style() {
        let float = zz_protocol::FloatingPaneSnapshot {
            xoff: 4,
            yoff: 2,
            sx: 6,
            sy: 2,
            ..CLIPPED
        };
        let paint = |window_style: &str, window_active_style: &str| {
            let mut model = tiled_under_float(float, None);
            let mut status = (*model.status).clone();
            status.pane_borders = vec![zz_protocol::PaneBorderPresentation {
                pane: PaneId(8),
                style: String::new(),
                window_style: window_style.to_owned(),
                window_active_style: window_active_style.to_owned(),
            }];
            model.status = std::sync::Arc::new(status);
            model.viewports.insert(
                PaneId(8),
                TerminalViewport::blank(6, 2, SessionStatus::Running),
            );
            let entry = model.pane_rect(PaneId(8)).expect("the float's rect");
            let mut renderer = Renderer::new();
            renderer.paint_entry(&model, &entry, true, false);
            (
                model.pane_window_style(PaneId(8)),
                String::from_utf8_lossy(&renderer.output).into_owned(),
            )
        };

        let (style, output) = paint("bg=themedarkgrey,fg=themewhite", "");
        assert_eq!(style.fg, Some(TmuxColour::Rgb(0x00e5_e5e5)));
        assert_eq!(style.bg, Some(TmuxColour::Rgb(0x0026_2626)));
        assert!(
            output.contains("\x1b[38;2;229;229;229m\x1b[48;2;38;38;38m      "),
            "{output:?}"
        );

        let (_, plain) = paint("", "");
        assert!(!plain.contains("\x1b[48;2;38;38;38m"), "{plain:?}");

        let (style, _) = paint("fg=blue,bg=red", "bg=green,fg=default");
        assert_eq!(style.fg, Some(TmuxColour::Basic(4)));
        assert_eq!(style.bg, Some(TmuxColour::Basic(2)));
    }

    #[test]
    fn a_float_clipped_at_the_right_or_bottom_crops_its_pane_mode_instead_of_reflowing_it() {
        let clock = |float: zz_protocol::FloatingPaneSnapshot| {
            let mut model = tiled_under_float(float, None);
            let mut snapshot = (*model.snapshot).clone();
            snapshot.sessions[0].windows[0]
                .panes
                .get_mut(&PaneId(8))
                .unwrap()
                .mode = Some(zz_protocol::PaneMode::Clock {
                time: "12:00".to_owned(),
                colour: "blue".to_owned(),
            });
            model.update_snapshot(std::sync::Arc::new(snapshot));
            let entry = model.pane_rect(PaneId(8)).expect("the float's rect");
            let mut renderer = Renderer::new();
            renderer.paint_entry(&model, &entry, true, false);
            (
                entry.content(),
                entry.mode_rect(),
                String::from_utf8_lossy(&renderer.output).into_owned(),
            )
        };

        let right = zz_protocol::FloatingPaneSnapshot {
            xoff: 34,
            yoff: 2,
            sx: 10,
            sy: 5,
            ..CLIPPED
        };
        let (content, whole, output) = clock(right);
        assert_eq!((content.width, content.height), (6, 5));
        assert_eq!((whole.width, whole.height), (10, 5));
        assert!(output.contains("12:"), "{output:?}");
        assert!(
            !output.contains("12:00"),
            "the 10-column clock centres on the whole pane and loses its last columns: {output:?}"
        );

        let rows = tiled_under_float(right, None)
            .pane_rect(PaneId(7))
            .unwrap()
            .rect
            .height;
        let bottom = zz_protocol::FloatingPaneSnapshot {
            xoff: 4,
            yoff: i32::from(rows) - 2,
            sx: 10,
            sy: 5,
            ..CLIPPED
        };
        let (content, whole, output) = clock(bottom);
        assert_eq!(content.height, 2, "{content:?}");
        assert_eq!((whole.width, whole.height), (10, 5));
        assert!(
            !output.contains("12:00"),
            "the clock's row is below the window's last row: {output:?}"
        );
    }

    #[test]
    fn a_pane_mode_cursor_under_a_float_is_hidden() {
        let mode = zz_protocol::PaneMode::Switch {
            rows: vec!["cli".to_owned()],
            selected: 0,
            offset: 0,
            selection_style: "noattr,bg=themeyellow,fg=themeblack".to_owned(),
            prompt: "(search) ".to_owned(),
            prompt_style: "bg=themeyellow,fg=themeblack".to_owned(),
            prompt_cursor: 9,
            matches: Vec::new(),
            match_style: String::new(),
            prompt_shape: zz_protocol::PromptCursor::default(),
        };
        let open = zz_protocol::FloatingPaneSnapshot {
            xoff: 30,
            yoff: 8,
            sx: 4,
            sy: 1,
            ..CLIPPED
        };
        let model = tiled_under_float(open, Some(mode.clone()));
        let content = model.pane_rect(PaneId(7)).unwrap().content();
        let cursor = pane_mode::surface(&mode, content, &model.status.theme).cursor;
        let mut renderer = Renderer::new();
        renderer.place_pane_cursor(&model);
        assert!(String::from_utf8_lossy(&renderer.output).ends_with("\x1b[?25h"));

        let covering = zz_protocol::FloatingPaneSnapshot {
            xoff: i32::from(cursor.0) - 1,
            yoff: i32::from(cursor.1),
            sx: 4,
            sy: 1,
            ..CLIPPED
        };
        let model = tiled_under_float(covering, Some(mode));
        let mut renderer = Renderer::new();
        renderer.place_pane_cursor(&model);
        let output = String::from_utf8_lossy(&renderer.output).into_owned();
        assert!(!output.contains("\x1b[?25h"), "{output:?}");
        assert!(output.ends_with("\x1b[?25l"), "{output:?}");
    }

    #[test]
    fn a_browser_pane_a_float_covers_has_no_kitty_placement_while_covered() {
        let float = zz_protocol::FloatingPaneSnapshot {
            pane: PaneId(8),
            xoff: 5,
            yoff: 2,
            sx: 10,
            sy: 4,
            visible: true,
            border_lines: PaneBorderLines::Single,
            border_status: zz_protocol::PaneBorderStatus::Off,
        };
        let bare = browser_under_float(None);
        assert!(browser_placement_output(&bare).contains("\x1b_Ga=p"));

        let covered = browser_under_float(Some(float));
        assert_eq!(
            covered.layout.float(PaneId(8)).map(|float| float.frame),
            Some(Rect {
                x: 4,
                y: 1,
                width: 12,
                height: 6,
            })
        );
        assert!(!browser_placement_output(&covered).contains("\x1b_Ga=p"));

        let hidden = browser_under_float(Some(zz_protocol::FloatingPaneSnapshot {
            visible: false,
            ..float
        }));
        assert!(browser_placement_output(&hidden).contains("\x1b_Ga=p"));
    }

    #[test]
    fn a_command_output_on_another_pane_owns_neither_the_prompt_nor_the_cursor() {
        let mut model = block_model(40, 8);
        attach_one_pane(&mut model, PaneId(1));
        let viewport = TerminalViewport::blank(40, 6, SessionStatus::Running);
        model.command_output = Some((PaneId(2), viewport.clone()));
        model.command_output_search = Some(SearchQuery::literal("elsewhere"));
        assert_eq!(model.command_output_focus(), None);
        assert!(status_overlay(&model, 40).is_none());
        model.command_output = Some((PaneId(1), viewport));
        assert_eq!(model.command_output_focus(), Some(PaneId(1)));
        assert!(matches!(
            status_overlay(&model, 40),
            Some(StatusOverlay { .. })
        ));
    }

    #[test]
    fn the_chooser_key_gutter_sizes_once_and_pads_keyless_rows() {
        assert_eq!(chooser_key_column(["0", "M-a", ""].into_iter()), 6);
        assert_eq!(chooser_key_column(["", ""].into_iter()), 0);
        assert_eq!(chooser_key_cell("0", 6), "(0)   ");
        assert_eq!(chooser_key_cell("M-a", 6), "(M-a) ");
        assert_eq!(chooser_key_cell("", 6), "      ");
        assert_eq!(chooser_key_cell("", 0), "");
        assert_eq!(chooser_key_cell("0", 0), "");
    }

    #[test]
    fn forced_repaints_preserve_a_chooser_until_its_presentation_arrives() {
        let mut model = block_model(60, 12);
        Arc::make_mut(&mut model.status).rows = vec!["CHOOSER-STATUS".to_owned()];
        Arc::make_mut(&mut model.status).customized = true;
        model.choose_tree = Some(zz_protocol::ChooseTreeState {
            items: ["alpha", "beta"]
                .into_iter()
                .enumerate()
                .map(|(index, name)| zz_protocol::ChooseTreeItem {
                    label: name.to_owned(),
                    detail: String::new(),
                    target: zz_protocol::ChooseTreeTarget::Session(zz_protocol::SessionId(
                        index as u64,
                    )),
                    depth: 0,
                    flags: 0,
                    pane_kind: None,
                    key: index.to_string(),
                    text: String::new(),
                })
                .collect(),
            search: None,
            selected: 1,
            kind: zz_protocol::ChooseTreeKind::Windows,
            filter_no_matches: false,
            prompt: String::new(),
            help: false,
        });
        let presentation = zz_protocol::ChooserPresentation {
            selected: 1,
            rows: ["alpha", "beta"]
                .into_iter()
                .map(|name| zz_protocol::ChooserRow {
                    name: name.to_owned(),
                    ..Default::default()
                })
                .collect(),
            sort: "index".to_owned(),
            view: "preview".to_owned(),
            filter: false,
            selection_style: String::new(),
            border_style: String::new(),
            prompt_style: String::new(),
            preview_size: zz_protocol::ChooserPreviewSize::Off,
            preview: None,
            prompt_cursor: zz_protocol::PromptCursor::default(),
            prompt_column: 0,
            prompt_line: String::new(),
        };
        let (send, receive) = std::sync::mpsc::channel();
        let mut renderer = Renderer::with_sink(Box::new(move |bytes| {
            send.send(bytes.to_vec()).unwrap();
            Ok(())
        }));
        let read_paint = || {
            String::from_utf8(
                receive
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap(),
            )
            .unwrap()
        };
        for missing in [false, true] {
            model.chooser_presentation = Some(presentation.clone());
            renderer.paint(&model, true).unwrap();
            let complete = read_paint();
            assert!(complete.contains("alpha"), "{complete:?}");
            assert!(complete.contains("beta"), "{complete:?}");
            assert!(complete.contains("CHOOSER-STATUS"), "{complete:?}");

            if missing {
                model.chooser_presentation = None;
            } else {
                model.chooser_presentation.as_mut().unwrap().selected = 0;
            }
            renderer.paint(&model, true).unwrap();
            let pending = read_paint();
            assert!(!pending.contains("\x1b[2J"), "{pending:?}");
            assert!(pending.contains("\x1b[2;1H\x1b[?25l"), "{pending:?}");
        }
        model.chooser_presentation = Some(presentation);
        renderer.paint(&model, true).unwrap();
        let restored = read_paint();
        assert!(restored.contains("alpha"), "{restored:?}");
        assert!(restored.contains("beta"), "{restored:?}");
        assert!(restored.contains("CHOOSER-STATUS"), "{restored:?}");
        model.choose_tree = None;
        model.chooser_presentation = None;
        renderer.paint(&model, true).unwrap();
        assert!(read_paint().contains("\x1b[2J"));
    }

    #[test]
    fn the_chooser_paints_every_row_key_in_its_gutter() {
        let mut model = block_model(60, 12);
        model.choose_tree = Some(zz_protocol::ChooseTreeState {
            items: vec![
                zz_protocol::ChooseTreeItem {
                    label: "alpha".to_owned(),
                    detail: "1 window".to_owned(),
                    target: zz_protocol::ChooseTreeTarget::Session(zz_protocol::SessionId(1)),
                    depth: 0,
                    flags: 0,
                    pane_kind: None,
                    key: "0".to_owned(),
                    text: String::new(),
                },
                zz_protocol::ChooseTreeItem {
                    label: "beta".to_owned(),
                    detail: "1 window".to_owned(),
                    target: zz_protocol::ChooseTreeTarget::Session(zz_protocol::SessionId(2)),
                    depth: 0,
                    flags: 0,
                    pane_kind: None,
                    key: "M-a".to_owned(),
                    text: String::new(),
                },
                zz_protocol::ChooseTreeItem {
                    label: "gamma".to_owned(),
                    detail: "1 window".to_owned(),
                    target: zz_protocol::ChooseTreeTarget::Session(zz_protocol::SessionId(3)),
                    depth: 0,
                    flags: 0,
                    pane_kind: None,
                    key: String::new(),
                    text: String::new(),
                },
            ],
            search: None,
            selected: 0,
            kind: zz_protocol::ChooseTreeKind::Windows,
            filter_no_matches: false,
            prompt: String::new(),
            help: false,
        });
        let mut renderer = Renderer::new();
        renderer.paint_chooser(&model);
        let output = String::from_utf8(renderer.output).unwrap();

        assert!(output.contains("(0)   alpha"), "{output}");
        assert!(output.contains("(M-a) beta"), "{output}");
        assert!(output.contains("      gamma"), "{output}");
    }

    #[test]
    fn a_row_format_takes_over_the_row_the_chooser_would_have_composed() {
        let mut model = block_model(60, 12);
        model.choose_tree = Some(zz_protocol::ChooseTreeState {
            items: vec![zz_protocol::ChooseTreeItem {
                label: "alpha".to_owned(),
                detail: "1 window".to_owned(),
                target: zz_protocol::ChooseTreeTarget::Session(zz_protocol::SessionId(1)),
                depth: 0,
                flags: 0,
                pane_kind: None,
                key: "0".to_owned(),
                text: "ZZTREE<%3>".to_owned(),
            }],
            search: None,
            selected: 0,
            kind: zz_protocol::ChooseTreeKind::Windows,
            filter_no_matches: false,
            prompt: String::new(),
            help: false,
        });
        let mut renderer = Renderer::new();
        renderer.paint_chooser(&model);
        let output = String::from_utf8(renderer.output).unwrap();
        assert!(output.contains("(0) ZZTREE<%3>"), "{output}");
        assert!(!output.contains("alpha"), "{output}");
        assert!(!output.contains("1 window"), "{output}");

        let mut model = block_model(60, 12);
        model.choose_buffer = Some(zz_protocol::ChooseBufferState {
            items: vec![zz_protocol::ChooseBufferItem {
                name: "buffer0".to_owned(),
                preview: "hello".to_owned(),
                size_bytes: 5,
                created_unix_seconds: 0,
                key: "0".to_owned(),
                text: "ZZBUF<buffer0>".to_owned(),
                tagged: false,
            }],
            search: None,
            selected: 0,
            filter_no_matches: false,
            help: false,
            prompt: String::new(),
        });
        let mut renderer = Renderer::new();
        renderer.paint_chooser(&model);
        let output = String::from_utf8(renderer.output).unwrap();
        assert!(output.contains("(0) ZZBUF<buffer0>"), "{output}");
        assert!(!output.contains("5 bytes"), "{output}");
        assert!(!output.contains("hello"), "{output}");
    }

    #[test]
    fn chooser_fallback_status_keeps_fully_keyless_rows_selectable_without_a_gutter() {
        let mut model = block_model(60, 12);
        model.choose_buffer = Some(zz_protocol::ChooseBufferState {
            items: vec![zz_protocol::ChooseBufferItem {
                name: "keyless".to_owned(),
                preview: "fallback row".to_owned(),
                size_bytes: 1,
                created_unix_seconds: 0,
                key: String::new(),
                text: String::new(),
                tagged: false,
            }],
            search: None,
            selected: 0,
            filter_no_matches: true,
            help: false,
            prompt: String::new(),
        });
        let mut renderer = Renderer::new();
        renderer.paint_chooser(&model);
        let output = String::from_utf8(renderer.output).unwrap();

        assert!(output.contains("filter: no matches"), "{output}");
        assert!(output.contains("> keyless  1 bytes"), "{output}");
        assert!(!output.contains(">      keyless"), "{output}");
    }
}
