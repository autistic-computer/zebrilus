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
mod terminal_pane;
mod workspace;

use colour::{RGB_COLOURS, colour_find_rgb, downgrade_palette, resolve_tmux_colour, theme_colour};
use cursor::{cursor_colour_rgb, prompt_cursor};
use sgr::{
    Ground, clear_screen, cursor_only, fill_unpainted_cells, sgr_is_reset, trailing_clear,
    write_border_runs, write_border_sgr, write_cell_ground, write_colored_sgr, write_colored_text,
    write_cursor_position, write_decimal, write_glyph, write_line_sgr, write_selection_sgr,
    write_sgr, write_styled_text, write_styled_text_over, write_tmux_sgr,
};
use sidebar_paint::sidebar_status_lines;
use status::{
    base_status_left, client_message_hides_the_cursor, combine_status,
    command_output_search_display, padded_segment, padded_styled, pane_prompt_row_y,
    pane_prompt_target, status_indicators, text_display_width, truncate, truncate_display_cells,
};
use workspace::{rects_overlap, source_viewport};

#[cfg(test)]
use chooser::{chooser_key_cell, chooser_key_column};
#[cfg(test)]
use colour::DARK_THEME_COLOURS;
#[cfg(test)]
use menu::floating_border;
#[cfg(test)]
use sgr::{write_palette_ground, write_rgb_ground};
#[cfg(test)]
use status::{
    append_status_label, command_output_search_prompt, mode_indicator, mode_indicators,
    status_overlay,
};
#[cfg(test)]
use terminal_pane::{changed_columns, source_placements};

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
mod chooser_tests;

#[cfg(test)]
mod colour_tests;

#[cfg(test)]
mod cursor_tests;

#[cfg(test)]
mod menu_tests;

#[cfg(test)]
mod sgr_tests;

#[cfg(test)]
mod status_tests;

#[cfg(test)]
mod terminal_pane_tests;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod workspace_tests;
