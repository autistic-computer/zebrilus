use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
};

use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use web_time::Instant;
use zz_client::{
    ChromeAction, ChromeKeymap, ChromeProfile, CoreEvent, TERMINAL_TABLE,
    local_scroll::{
        LocalRowSource, LocalScrollEffect, LocalScrollState, LocalScrollStep, LocalScrollSync,
        RowProjection, grid_placement, kitty_display_row, local_live_projection, local_row_source,
        local_scroll_available, peek_row_source, scroll_fraction_offset,
    },
    scrollback::{DeferredBackfill, HistoryFollowUp, HistoryPacer, RetainedTerminalViewport},
    terminal_color::{decorative_glyph, resolved_foreground},
    terminal_view::{
        PointerGrid, accumulate_scroll, cursor_should_blink, dead_label, mode_text, openable_uri,
        pasted_image_number, presented_uri, scroll_fraction, scrollbar_strip_hit,
        search_case_label, search_key_action, search_result_text, selection_autoscroll_lines,
        status_text, terminal_shortcut,
    },
};
use zz_protocol::{
    CommandInvocation, InputMessage, MAX_KITTY_IMAGE_BYTES, MAX_PASTE_UPLOAD_BYTES,
    MAX_PASTE_UPLOAD_CHUNK_BYTES, PaneId, PasteUploadPurpose, PastedImageFormat, ProtocolMessage,
    TerminalUiCommand,
};
use zz_terminal::{
    AppearanceColor, CellWidth, ClipboardTarget, Color, CursorStyle, GRAPHEME_TABLE_BIT, Glyph,
    KeyAction, KeyCode, KeyInput, KittyLayer, Modifiers, OVERLAY_RECTANGLE, OverlayKind,
    PackedCell, PackedStyle, PointerCellEvent, SearchCase, SearchDirection, SearchMode,
    SearchQuery, TerminalAppearance, TerminalDictionary, TerminalMouseButton, TerminalMousePhase,
    TerminalViewAction, TerminalViewport, UnderlineStyle,
};

use crate::{Client, pane_id};

const GUTTER_WIDTH: f32 = 16.0;
const PASTE_FORMAT_ERROR: &str = "Paste a PNG, JPEG, GIF, or WebP image of 6 MiB or less.";

#[derive(Default)]
pub struct History {
    pacer: HistoryPacer,
}

struct Assembly {
    generation: u64,
    width: u32,
    height: u32,
    total: usize,
    bytes: Vec<u8>,
}

struct Ready {
    generation: u64,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

enum Pasted {
    Requested,
    Loading {
        format: PastedImageFormat,
        total: usize,
        bytes: Vec<u8>,
    },
    Ready {
        format: PastedImageFormat,
        bytes: Vec<u8>,
    },
    Unavailable,
}

#[derive(Default)]
pub struct Images {
    pending: HashMap<(PaneId, u32), Assembly>,
    ready: HashMap<(PaneId, u32), Ready>,
    pasted: HashMap<(PaneId, u32), Pasted>,
}

impl Images {
    pub fn begin(
        &mut self,
        pane: PaneId,
        image: u32,
        generation: u64,
        width: u32,
        height: u32,
        total: u32,
    ) {
        let expected = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4));
        if width == 0 || height == 0 || total > MAX_KITTY_IMAGE_BYTES || expected != Some(total) {
            return;
        }
        if self
            .ready
            .get(&(pane, image))
            .is_some_and(|ready| ready.generation >= generation)
        {
            return;
        }
        self.pending.insert(
            (pane, image),
            Assembly {
                generation,
                width,
                height,
                total: total as usize,
                bytes: Vec::with_capacity(total as usize),
            },
        );
    }

    pub fn chunk(&mut self, pane: PaneId, image: u32, generation: u64, bytes: Vec<u8>) -> bool {
        let Some(entry) = self.pending.get_mut(&(pane, image)) else {
            return false;
        };
        if entry.generation != generation {
            return false;
        }
        if entry.bytes.len() + bytes.len() > entry.total {
            self.pending.remove(&(pane, image));
            return false;
        }
        entry.bytes.extend_from_slice(&bytes);
        if entry.bytes.len() < entry.total {
            return false;
        }
        let Some(done) = self.pending.remove(&(pane, image)) else {
            return false;
        };
        self.ready.insert(
            (pane, image),
            Ready {
                generation: done.generation,
                width: done.width,
                height: done.height,
                rgba: rgba_from_premultiplied_bgra(done.bytes),
            },
        );
        true
    }

    pub fn remove(&mut self, pane: PaneId, images: &[u32]) {
        for image in images {
            self.pending.remove(&(pane, *image));
            self.ready.remove(&(pane, *image));
        }
    }

    fn forget_pane(&mut self, pane: PaneId) {
        self.pending.retain(|(id, _), _| *id != pane);
        self.ready.retain(|(id, _), _| *id != pane);
        self.pasted.retain(|(id, _), _| *id != pane);
    }

    fn clear(&mut self) {
        self.pending.clear();
        self.ready.clear();
        self.pasted.clear();
    }

    fn pasted_message(&mut self, message: &ProtocolMessage) {
        match message {
            ProtocolMessage::PastedImageBegin {
                pane,
                number,
                format,
                total_bytes,
            } => {
                if let Some(entry @ Pasted::Requested) = self.pasted.get_mut(&(*pane, *number)) {
                    *entry = if *total_bytes > 0 && *total_bytes <= MAX_PASTE_UPLOAD_BYTES {
                        Pasted::Loading {
                            format: *format,
                            total: *total_bytes as usize,
                            bytes: Vec::with_capacity(*total_bytes as usize),
                        }
                    } else {
                        Pasted::Unavailable
                    };
                }
            }
            ProtocolMessage::PastedImageChunk {
                pane,
                number,
                bytes,
            } => {
                let Some(entry) = self.pasted.get_mut(&(*pane, *number)) else {
                    return;
                };
                if let Pasted::Loading {
                    format,
                    total,
                    bytes: loaded,
                } = entry
                {
                    if loaded.len() + bytes.len() > *total {
                        *entry = Pasted::Unavailable;
                        return;
                    }
                    loaded.extend_from_slice(bytes);
                    if loaded.len() == *total {
                        *entry = Pasted::Ready {
                            format: *format,
                            bytes: std::mem::take(loaded),
                        };
                    }
                }
            }
            ProtocolMessage::PastedImageUnavailable { pane, number } => {
                if let Some(entry) = self.pasted.get_mut(&(*pane, *number)) {
                    *entry = Pasted::Unavailable;
                }
            }
            _ => {}
        }
    }
}

fn rgba_from_premultiplied_bgra(mut bytes: Vec<u8>) -> Vec<u8> {
    for pixel in bytes.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        let channel = |value: u8| {
            (u32::from(value) * 255 + alpha / 2)
                .checked_div(alpha)
                .map_or(0, |value| value.min(255) as u8)
        };
        let (blue, green, red) = (channel(pixel[0]), channel(pixel[1]), channel(pixel[2]));
        pixel[0] = red;
        pixel[1] = green;
        pixel[2] = blue;
    }
    bytes
}

#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
struct View {
    scroll: LocalScrollState,
    remainder: f32,
    search: Option<SearchQuery>,
    accept_on_enter: bool,
    forwarded: HashSet<String>,
    swallowed: Option<String>,
    grid: PointerGrid,
    surface: [f32; 4],
    selecting: bool,
    scrollbar_dragging: bool,
    force_local: bool,
    pressed: HashSet<u8>,
    selection_pointer: Option<PointerCellEvent>,
    autoscroll: i32,
    focused: bool,
}

impl View {
    fn end_drag(&mut self) {
        self.selecting = false;
        self.scrollbar_dragging = false;
        self.force_local = false;
        self.selection_pointer = None;
        self.autoscroll = 0;
    }
}

pub struct Views {
    panes: HashMap<PaneId, View>,
    output: View,
    keymaps: [ChromeKeymap; 2],
    uploads: HashMap<u64, (Vec<u8>, usize)>,
    contrast: HashMap<(u32, u32, bool, bool), Color>,
}

impl Default for Views {
    fn default() -> Self {
        Self {
            panes: HashMap::new(),
            output: View::default(),
            keymaps: [
                ChromeKeymap::for_profile(ChromeProfile::Desktop),
                ChromeKeymap::for_profile(ChromeProfile::DesktopApple),
            ],
            uploads: HashMap::new(),
            contrast: HashMap::new(),
        }
    }
}

impl Views {
    fn get(&mut self, pane: PaneId, output: bool) -> &mut View {
        if output {
            &mut self.output
        } else {
            self.panes.entry(pane).or_default()
        }
    }

    fn peek(&self, pane: PaneId, output: bool) -> Option<&View> {
        if output {
            Some(&self.output)
        } else {
            self.panes.get(&pane)
        }
    }
}

pub fn observe(client: &mut Client, event: &CoreEvent) {
    match event {
        CoreEvent::HelloReceived => {
            client.history.pacer.clear();
            client.images.clear();
        }
        CoreEvent::Attached { .. } => {
            client.history.pacer.clear();
            client.images.clear();
            for view in client.views.panes.values_mut() {
                view.scroll.cancel();
            }
        }
        CoreEvent::PaneRemoved { pane } => {
            client.history.pacer.forget(*pane);
            client.images.forget_pane(*pane);
            client.views.panes.remove(pane);
        }
        CoreEvent::ViewportChanged { pane, .. } => {
            client.request_history(*pane, None);
            let restore = client.core.retained_viewport(*pane).and_then(|retained| {
                client
                    .views
                    .panes
                    .get_mut(pane)?
                    .scroll
                    .observe(retained, false, Instant::now())
                    .restore
            });
            if let Some(target) = restore {
                client.send_view(*pane, false, TerminalViewAction::ScrollToOffset(target));
            }
        }
        CoreEvent::Message(message) => client.images.pasted_message(message),
        _ => {}
    }
}

pub fn history_chunk(
    client: &mut Client,
    pane: PaneId,
    start: u32,
    total: u32,
    offset: u32,
    columns: u16,
    rows: Vec<Vec<PackedCell>>,
    dictionary: TerminalDictionary,
) -> bool {
    let mutations = client
        .core
        .retained_viewport(pane)
        .map(|retained| retained.history_mutations);
    let (apply, follow_up) = client.history.pacer.chunk_arrived(pane, mutations);
    if apply {
        client
            .core
            .apply_history_chunk(pane, start, total, offset, columns, rows, dictionary);
    }
    match follow_up {
        HistoryFollowUp::Prefetch(target) => {
            client.request_history(pane, Some(target));
            false
        }
        HistoryFollowUp::Defer(mutations) => client.history.pacer.defer(pane, mutations),
        HistoryFollowUp::Backfill => {
            client.request_history(pane, None);
            false
        }
    }
}

pub fn ui_command(client: &mut Client, pane: PaneId, command: TerminalUiCommand) -> Value {
    let TerminalUiCommand::BeginSearch { direction } = command;
    let output = client
        .core
        .command_output()
        .is_some_and(|(output, _)| output == pane);
    client.begin_search(
        pane,
        output,
        SearchQuery {
            direction,
            ..SearchQuery::default()
        },
        true,
    );
    json!({ "search": true, "output": output })
}

fn local_retained(
    core: &zz_client::ClientCore,
    pane: PaneId,
    output: bool,
) -> Option<&RetainedTerminalViewport> {
    if output {
        return None;
    }
    core.retained_viewport(pane)
        .filter(|retained| local_scroll_available(retained))
}

fn mouse_button(button: u8) -> Option<TerminalMouseButton> {
    match button {
        0 => Some(TerminalMouseButton::Left),
        1 => Some(TerminalMouseButton::Middle),
        2 => Some(TerminalMouseButton::Right),
        _ => None,
    }
}

fn step_json(stop: bool, redraw: bool, syncs: &[u64]) -> String {
    json!({ "stop": stop, "redraw": redraw, "syncs": syncs }).to_string()
}

#[wasm_bindgen]
impl Client {
    pub fn viewport(
        &mut self,
        pane: f64,
        output: bool,
        grid_rows: u16,
        hidden: u16,
    ) -> Option<String> {
        let pane = pane_id(pane);
        let retained = if output {
            self.core
                .retained_command_output()
                .map(|(_, retained)| retained)
        } else {
            self.core.retained_viewport(pane)
        }?;
        let fallback = TerminalAppearance::default();
        let appearance = self.core.appearance().unwrap_or(&fallback);
        let view = if output {
            Some(&self.views.output)
        } else {
            self.views.panes.get(&pane)
        };
        let mut painter = Painter {
            appearance,
            foreground: retained.viewport.foreground,
            background: retained.viewport.background,
            styles: Vec::new(),
            index: HashMap::new(),
            contrast: &mut self.views.contrast,
        };
        let images = &self.images;
        Some(
            render(
                retained,
                appearance,
                view,
                output,
                grid_rows,
                hidden,
                &mut painter,
                |image, generation| {
                    images
                        .ready
                        .get(&(pane, image))
                        .filter(|ready| ready.generation == generation)
                        .map(|ready| (ready.width, ready.height))
                },
            )
            .to_string(),
        )
    }

    pub fn pane_status(&self, pane: f64) -> String {
        let id = pane_id(pane);
        self.core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| {
                window.panes.get(&id).map(|snapshot| {
                    json!({
                        "dead": dead_label(snapshot.dead, snapshot.dead_status),
                        "synchronized": snapshot.synchronized_input,
                        "zoomed": window.zoomed_pane == Some(id),
                    })
                })
            })
            .unwrap_or(Value::Null)
            .to_string()
    }

    pub fn terminal_metrics(&self, natural_height: f32, scale: f32) -> f32 {
        let height = self
            .core
            .appearance()
            .map_or(natural_height, |appearance| {
                appearance.cell_height_adjustment.apply(natural_height)
            })
            .max(1.0);
        let device = 1.0 / scale.max(f32::EPSILON);
        ((height.max(device) * scale).round() / scale).max(device)
    }

    pub fn command_output_pane(&self) -> Option<f64> {
        self.core.command_output().map(|(pane, _)| pane.0 as f64)
    }

    pub fn cursor_blink_interval(&self) -> u32 {
        self.core
            .appearance()
            .map_or(500, |appearance| appearance.cursor_blink_interval_ms)
    }

    pub fn openable_uri(&self, uri: &str) -> bool {
        openable_uri(uri)
    }

    pub fn image_bytes(&self, pane: f64, image: u32) -> Option<Vec<u8>> {
        self.images
            .ready
            .get(&(pane_id(pane), image))
            .map(|ready| ready.rgba.clone())
    }

    pub fn fetch_pasted_image(&mut self, pane: f64, number: u32) {
        let pane = pane_id(pane);
        if self.images.pasted.contains_key(&(pane, number)) {
            return;
        }
        self.images.pasted.insert((pane, number), Pasted::Requested);
        self.send(&ProtocolMessage::FetchPastedImage { pane, number });
    }

    pub fn pasted_image(&self, pane: f64, number: u32) -> Option<Vec<u8>> {
        match self.images.pasted.get(&(pane_id(pane), number))? {
            Pasted::Ready { bytes, .. } => Some(bytes.clone()),
            _ => None,
        }
    }

    pub fn pasted_image_type(&self, pane: f64, number: u32) -> Option<String> {
        match self.images.pasted.get(&(pane_id(pane), number))? {
            Pasted::Ready { format, .. } => Some(
                match format {
                    PastedImageFormat::Png => "image/png",
                    PastedImageFormat::Jpeg => "image/jpeg",
                    PastedImageFormat::Gif => "image/gif",
                    PastedImageFormat::Webp => "image/webp",
                }
                .to_owned(),
            ),
            _ => None,
        }
    }

    pub fn upload_image(
        &mut self,
        pane: f64,
        extension: &str,
        bytes: Vec<u8>,
    ) -> Result<f64, JsError> {
        if PastedImageFormat::from_extension(extension).is_none()
            || bytes.is_empty()
            || bytes.len() > MAX_PASTE_UPLOAD_BYTES as usize
        {
            return Err(JsError::new(PASTE_FORMAT_ERROR));
        }
        let upload_id = self.next_request;
        self.next_request += 1;
        self.send(&ProtocolMessage::PasteUploadBegin {
            upload_id,
            pane: pane_id(pane),
            purpose: PasteUploadPurpose::PastePath,
            extension: extension.into(),
            total_bytes: bytes.len() as u32,
        });
        self.views.uploads.insert(upload_id, (bytes, 0));
        Ok(upload_id as f64)
    }

    pub fn upload_next(&mut self, upload_id: f64) -> bool {
        let upload_id = upload_id as u64;
        let Some((bytes, offset)) = self.views.uploads.get_mut(&upload_id) else {
            return false;
        };
        let end = (*offset + MAX_PASTE_UPLOAD_CHUNK_BYTES).min(bytes.len());
        let chunk = bytes[*offset..end].to_vec();
        *offset = end;
        let more = end < bytes.len();
        if !more {
            self.views.uploads.remove(&upload_id);
        }
        self.send(&ProtocolMessage::PasteUploadChunk {
            upload_id,
            bytes: chunk,
        });
        more
    }

    pub fn upload_cancel(&mut self) {
        self.views.uploads.clear();
    }

    pub fn terminal_geometry(
        &mut self,
        pane: f64,
        output: bool,
        grid: Vec<f32>,
        surface: Vec<f32>,
    ) {
        let view = self.views.get(pane_id(pane), output);
        if let [
            left,
            top,
            width,
            height,
            cell_width,
            line_height,
            content_offset,
            scale,
            columns,
            rows,
        ] = grid[..]
        {
            view.grid = PointerGrid {
                left,
                top,
                width,
                height,
                cell_width,
                line_height,
                content_offset,
                scale,
                columns: columns as u16,
                rows: rows as u16,
            };
        }
        if let [left, top, width, height] = surface[..] {
            view.surface = [left, top, width, height];
        }
    }

    pub fn terminal_focus(&mut self, pane: f64, output: bool, focused: bool) -> bool {
        let id = pane_id(pane);
        let view = self.views.get(id, output);
        if view.focused == focused {
            return false;
        }
        view.focused = focused;
        if !focused {
            view.end_drag();
            view.pressed.clear();
            view.forwarded.clear();
            view.swallowed = None;
        }
        self.send_view(id, output, TerminalViewAction::Focus(focused));
        if !focused {
            self.send_view(id, output, TerminalViewAction::ClearLinkHover);
        }
        true
    }

    pub fn cursor_blinks(&self, pane: f64, output: bool, focused: bool) -> bool {
        let viewport = if output {
            self.core.command_output().map(|(_, viewport)| viewport)
        } else {
            self.core.viewport(pane_id(pane))
        };
        viewport.is_some_and(|viewport| {
            cursor_should_blink(
                viewport.cursor,
                self.core
                    .appearance()
                    .map_or(zz_terminal::CursorBlinkPolicy::Terminal, |appearance| {
                        appearance.cursor_blink_policy
                    }),
                focused,
            )
        })
    }

    pub fn mouse_down(
        &mut self,
        pane: f64,
        output: bool,
        x: f32,
        y: f32,
        button: u8,
        count: u32,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) -> String {
        let id = pane_id(pane);
        if !output {
            self.invoke(CommandInvocation::new(
                "select-pane",
                vec!["-Z".into(), "-t".into(), id.to_string()],
            ));
        }
        let Some((tracking, scrollbar)) = self
            .surface_viewport(id, output)
            .map(|viewport| (viewport.mouse_tracking, viewport.scrollbar))
        else {
            return step_json(false, false, &[]);
        };
        let view = self.views.get(id, output);
        let [left, top, width, height] = view.surface;
        if button == 0
            && scrollbar_strip_hit(
                left + width,
                top,
                top + height,
                GUTTER_WIDTH,
                scrollbar,
                x,
                y,
            )
        {
            view.end_drag();
            view.scrollbar_dragging = true;
            self.send_view(id, output, TerminalViewAction::ClearLinkHover);
            let (redraw, syncs) = self.scroll_to_pointer(id, output, y);
            return step_json(true, redraw, &syncs);
        }
        if !view.grid.contains(x, y) {
            return step_json(false, false, &[]);
        }
        if button == 0 {
            view.force_local = count >= 3 && (control || meta);
            view.selecting = !tracking || shift || view.force_local;
        }
        view.pressed.insert(button);
        let input = view.grid.input(
            x,
            y,
            TerminalMousePhase::Press,
            mouse_button(button),
            count as usize,
            Modifiers::new(shift, control, alt, meta),
            shift || view.force_local,
        );
        view.selection_pointer = view.selecting.then_some(input.cell);
        if let Some(target) = view.scroll.flush() {
            self.send_view(id, output, TerminalViewAction::ScrollToOffset(target));
        }
        self.send_view(id, output, TerminalViewAction::Mouse(input));
        step_json(true, false, &[])
    }

    pub fn mouse_up(
        &mut self,
        pane: f64,
        output: bool,
        x: f32,
        y: f32,
        button: u8,
        count: u32,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) -> String {
        let id = pane_id(pane);
        let view = self.views.get(id, output);
        if view.scrollbar_dragging && button == 0 {
            let (redraw, syncs) = self.scroll_to_pointer(id, output, y);
            self.views.get(id, output).end_drag();
            return step_json(true, redraw, &syncs);
        }
        if !view.pressed.remove(&button) {
            return step_json(false, false, &[]);
        }
        let input = view.grid.input(
            x,
            y,
            TerminalMousePhase::Release,
            mouse_button(button),
            count as usize,
            Modifiers::new(shift, control, alt, meta),
            shift || view.force_local,
        );
        if button == 0 {
            view.end_drag();
        }
        self.send_view(id, output, TerminalViewAction::Mouse(input));
        step_json(true, false, &[])
    }

    pub fn mouse_move(
        &mut self,
        pane: f64,
        output: bool,
        x: f32,
        y: f32,
        pressed: i8,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) -> String {
        let id = pane_id(pane);
        let view = self.views.get(id, output);
        let pressed = u8::try_from(pressed).ok();
        if pressed.is_none() {
            view.end_drag();
            view.pressed.clear();
        }
        if view.scrollbar_dragging {
            let (redraw, syncs) = self.scroll_to_pointer(id, output, y);
            return step_json(true, redraw, &syncs);
        }
        if pressed.is_some_and(|button| !view.pressed.contains(&button)) {
            return step_json(false, false, &[]);
        }
        if !view.selecting && !view.grid.contains(x, y) {
            self.send_view(id, output, TerminalViewAction::ClearLinkHover);
            return step_json(false, false, &[]);
        }
        let input = view.grid.input(
            x,
            y,
            TerminalMousePhase::Motion,
            pressed.and_then(mouse_button),
            1,
            Modifiers::new(shift, control, alt, meta),
            shift || view.force_local,
        );
        let mut autoscroll = false;
        if view.selecting {
            view.selection_pointer = Some(input.cell);
            view.autoscroll = selection_autoscroll_lines(
                view.grid.top,
                view.grid.top + view.grid.height,
                view.grid.line_height,
                y,
            );
            autoscroll = view.autoscroll != 0;
        }
        self.send_view(id, output, TerminalViewAction::Mouse(input));
        json!({ "stop": pressed.is_some(), "autoscroll": autoscroll }).to_string()
    }

    pub fn mouse_leave(&mut self, pane: f64, output: bool) {
        self.send_view(pane_id(pane), output, TerminalViewAction::ClearLinkHover);
    }

    pub fn mouse_modifiers(
        &mut self,
        pane: f64,
        output: bool,
        x: f32,
        y: f32,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) {
        let id = pane_id(pane);
        let view = self.views.get(id, output);
        if !view.grid.contains(x, y) {
            return;
        }
        let input = view.grid.input(
            x,
            y,
            TerminalMousePhase::Motion,
            None,
            1,
            Modifiers::new(shift, control, alt, meta),
            shift || view.force_local,
        );
        self.send_view(id, output, TerminalViewAction::Mouse(input));
    }

    pub fn selection_autoscroll(&mut self, pane: f64, output: bool) -> bool {
        let id = pane_id(pane);
        let view = self.views.get(id, output);
        let Some(pointer) = view.selection_pointer else {
            return false;
        };
        if !view.selecting || view.autoscroll == 0 {
            return false;
        }
        let lines = view.autoscroll;
        self.send_view(
            id,
            output,
            TerminalViewAction::SelectionAutoscroll { lines, pointer },
        );
        true
    }

    pub fn wheel(
        &mut self,
        pane: f64,
        output: bool,
        delta: f32,
        pixels: bool,
        x: f32,
        y: f32,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) -> String {
        let id = pane_id(pane);
        let line_height = self.views.get(id, output).grid.line_height.max(1.0);
        let delta = if pixels { delta } else { delta * line_height };
        let local = pixels && local_retained(&self.core, id, output).is_some();
        if pixels && delta == 0.0 {
            return step_json(false, false, &[]);
        }
        let mut redraw = false;
        let mut syncs = Vec::new();
        if local {
            let now = Instant::now();
            let step = local_retained(&self.core, id, output).and_then(|retained| {
                self.views.panes.get_mut(&id)?.scroll.scroll_by_pixels(
                    delta,
                    line_height,
                    retained,
                    now,
                )
            });
            if let Some(step) = step {
                self.views.get(id, output).remainder = 0.0;
                (redraw, syncs) = self.apply_step(id, step);
                return step_json(true, redraw, &syncs);
            }
        }
        let view = self.views.get(id, output);
        redraw |= view.scroll.clear_sub_row();
        let lines = accumulate_scroll(&mut view.remainder, -delta / line_height);
        if lines == 0 {
            return step_json(false, redraw, &syncs);
        }
        if lines > 0 && local_retained(&self.core, id, output).is_some() {
            let now = Instant::now();
            let step = local_retained(&self.core, id, output).and_then(|retained| {
                Some(self.views.panes.get_mut(&id)?.scroll.scroll_by_rows(
                    i64::from(lines),
                    retained,
                    now,
                ))
            });
            if let Some(step) = step {
                let (stepped, more) = self.apply_step(id, step);
                redraw |= stepped;
                syncs = more;
            }
        } else {
            let view = self.views.get(id, output);
            if lines < 0
                && let Some(target) = view.scroll.release()
            {
                redraw = true;
                self.send_view(id, output, TerminalViewAction::ScrollToOffset(target));
            }
            let view = self.views.get(id, output);
            let input = view.grid.input(
                x,
                y,
                TerminalMousePhase::Press,
                Some(if lines < 0 {
                    TerminalMouseButton::ScrollUp
                } else {
                    TerminalMouseButton::ScrollDown
                }),
                1,
                Modifiers::new(shift, control, alt, meta),
                shift || view.force_local,
            );
            self.send_view(id, output, TerminalViewAction::ScrollWheel { lines, input });
        }
        step_json(true, redraw, &syncs)
    }

    pub fn scroll_sync(&mut self, pane: f64, generation: f64) -> u8 {
        let id = pane_id(pane);
        let generation = generation as u64;
        let Some(view) = self.views.panes.get_mut(&id) else {
            return 0;
        };
        let sync = match self.core.retained_viewport(id) {
            Some(retained) => view.scroll.sync(generation, retained),
            None => view.scroll.clear().then_some(LocalScrollSync::Retired),
        };
        match sync {
            None => 0,
            Some(LocalScrollSync::Retired) => 1,
            Some(LocalScrollSync::Requested(target)) => {
                self.send_view(id, false, TerminalViewAction::ScrollToOffset(target));
                2
            }
        }
    }

    pub fn scroll_expire(&mut self, pane: f64, generation: f64) -> bool {
        self.views
            .panes
            .get_mut(&pane_id(pane))
            .is_some_and(|view| view.scroll.expire(generation as u64, Instant::now()))
    }

    pub fn history_resume(&mut self, pane: f64) -> bool {
        let id = pane_id(pane);
        let mutations = self
            .core
            .retained_viewport(id)
            .map(|retained| retained.history_mutations);
        match self.history.pacer.resume_deferred(id, mutations) {
            DeferredBackfill::Request => {
                self.request_history(id, None);
                false
            }
            DeferredBackfill::Wait => true,
            DeferredBackfill::Done => false,
        }
    }

    pub fn key_down(
        &mut self,
        pane: f64,
        output: bool,
        key: &str,
        text: Option<String>,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
        repeat: bool,
    ) -> String {
        let id = pane_id(pane);
        let input = key_input(
            key,
            text,
            Modifiers::new(shift, control, alt, meta),
            if repeat {
                KeyAction::Repeat
            } else {
                KeyAction::Press
            },
        );
        if input.key == KeyCode::Unidentified {
            return "{}".into();
        }
        if self.views.get(id, output).swallowed.as_deref() == Some(key) {
            return json!({ "stop": true }).to_string();
        }
        if self.views.get(id, output).search.is_some() {
            let search = self
                .views
                .keymaps
                .iter()
                .find_map(|keymap| keymap.resolve(TERMINAL_TABLE, &input))
                == Some(ChromeAction::TerminalSearch);
            return if search {
                json!({ "action": "search" })
            } else {
                json!({ "focusSearch": true })
            }
            .to_string();
        }
        let action = terminal_shortcut(
            [&self.views.keymaps[0], &self.views.keymaps[1]],
            self.core.claims_prefix_input(&input),
            self.core.modal_capture(),
            &input,
        );
        let name = match action {
            Some(ChromeAction::TerminalFontIncrease) => "fontIncrease",
            Some(ChromeAction::TerminalFontDecrease) => "fontDecrease",
            Some(ChromeAction::TerminalCopy) => {
                self.send_view(
                    id,
                    output,
                    TerminalViewAction::CopySelection {
                        request_id: 1,
                        target: ClipboardTarget::Clipboard,
                    },
                );
                "copy"
            }
            Some(ChromeAction::TerminalSelectAll) => {
                self.send_view(id, output, TerminalViewAction::SelectAll);
                "selectAll"
            }
            Some(ChromeAction::TerminalClearHistory) => {
                self.send_view(id, output, TerminalViewAction::ClearHistory);
                "clearHistory"
            }
            Some(ChromeAction::TerminalPaste) => "paste",
            Some(ChromeAction::TerminalSearch) => "search",
            _ => "",
        };
        if !name.is_empty() {
            return json!({ "action": name }).to_string();
        }
        let kitty = self
            .surface_viewport(id, output)
            .is_some_and(|viewport| viewport.kitty_keyboard);
        let raw = kitty || !matches!(input.key, KeyCode::Character(_)) || control || alt || meta;
        if !raw {
            return json!({ "text": true }).to_string();
        }
        let view = self.views.get(id, output);
        let redraw = view.scroll.cancel();
        view.forwarded.insert(key.to_owned());
        self.send_key(id, input);
        json!({ "sent": true, "redraw": redraw }).to_string()
    }

    pub fn key_up(
        &mut self,
        pane: f64,
        output: bool,
        key: &str,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) -> bool {
        let id = pane_id(pane);
        let view = self.views.get(id, output);
        if view.swallowed.as_deref() == Some(key) {
            view.swallowed = None;
            return true;
        }
        if view.search.is_some() {
            view.forwarded.remove(key);
            return true;
        }
        if !view.forwarded.remove(key) {
            return false;
        }
        let input = key_input(
            key,
            None,
            Modifiers::new(shift, control, alt, meta),
            KeyAction::Release,
        );
        self.send_key(id, input);
        true
    }

    pub fn type_text(&mut self, pane: f64, output: bool, text: &str, composed: bool) -> bool {
        if text.is_empty() {
            return false;
        }
        let id = pane_id(pane);
        let redraw = self.views.get(id, output).scroll.cancel();
        if !composed && (text.contains(['\n', '\r']) || text.chars().count() > 1) {
            self.send_view(id, output, TerminalViewAction::Paste(text.into()));
        } else {
            self.input(InputMessage::Text {
                pane: id,
                text: text.into(),
            });
        }
        redraw
    }

    pub fn terminal_paste(&mut self, pane: f64, output: bool, text: String) {
        let id = pane_id(pane);
        self.views.get(id, output).scroll.cancel();
        self.send_view(id, output, TerminalViewAction::Paste(text));
    }

    pub fn terminal_action(&mut self, pane: f64, output: bool, name: &str) {
        let action = match name {
            "copy" => TerminalViewAction::CopySelection {
                request_id: 1,
                target: ClipboardTarget::Clipboard,
            },
            "selectAll" => TerminalViewAction::SelectAll,
            "clearHistory" => TerminalViewAction::ClearHistory,
            "clearSelection" => TerminalViewAction::ClearSelection,
            _ => return,
        };
        self.send_view(pane_id(pane), output, action);
    }

    pub fn unzoom(&mut self, pane: f64) {
        if !self.core.attached_read_only() {
            self.invoke(CommandInvocation::new(
                "resize-pane",
                vec!["-Z".into(), "-t".into(), pane_id(pane).to_string()],
            ));
        }
    }

    pub fn search_open(&mut self, pane: f64, output: bool) {
        self.begin_search(pane_id(pane), output, SearchQuery::default(), false);
    }

    pub fn search_state(&self, pane: f64, output: bool) -> Option<String> {
        let view = self.views.peek(pane_id(pane), output)?;
        let query = view.search.as_ref()?;
        Some(
            json!({
                "text": query.text,
                "regex": query.mode == SearchMode::Regex,
                "caseSensitive": query.case == SearchCase::Sensitive,
                "caseLabel": search_case_label(query.case),
            })
            .to_string(),
        )
    }

    pub fn search_text(&mut self, pane: f64, output: bool, text: String) {
        let id = pane_id(pane);
        let view = self.views.get(id, output);
        let Some(query) = view.search.as_mut() else {
            return;
        };
        query.text = text;
        let query = query.clone();
        view.scroll.cancel();
        self.send_view(id, output, TerminalViewAction::SearchUpdate(query));
    }

    pub fn search_toggle(&mut self, pane: f64, output: bool, which: &str) {
        let id = pane_id(pane);
        let Some(query) = self.views.get(id, output).search.as_mut() else {
            return;
        };
        if which == "regex" {
            query.mode = match query.mode {
                SearchMode::Literal => SearchMode::Regex,
                SearchMode::Regex => SearchMode::Literal,
            };
        } else {
            query.case = match query.case {
                SearchCase::Sensitive => SearchCase::Insensitive,
                _ => SearchCase::Sensitive,
            };
        }
        let query = query.clone();
        self.send_view(id, output, TerminalViewAction::SearchUpdate(query));
    }

    pub fn search_step(&mut self, pane: f64, output: bool, backward: bool) {
        let id = pane_id(pane);
        self.views.get(id, output).scroll.cancel();
        self.send_view(
            id,
            output,
            if backward {
                TerminalViewAction::SearchPrevious
            } else {
                TerminalViewAction::SearchNext
            },
        );
    }

    pub fn search_enter(&mut self, pane: f64, output: bool, shift: bool) -> bool {
        let id = pane_id(pane);
        let view = self.views.get(id, output);
        let Some(query) = view.search.as_ref() else {
            return false;
        };
        if view.accept_on_enter {
            view.search = None;
            view.swallowed = Some("Enter".into());
            return true;
        }
        let backward = (query.direction == SearchDirection::Backward) ^ shift;
        self.search_step(pane, output, backward);
        false
    }

    pub fn search_key(
        &mut self,
        pane: f64,
        output: bool,
        key: &str,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) -> bool {
        let id = pane_id(pane);
        let input = key_input(
            key,
            None,
            Modifiers::new(shift, control, alt, meta),
            KeyAction::Press,
        );
        let Some(query) = self.views.get(id, output).search.as_mut() else {
            return false;
        };
        let Some(action) = search_key_action(query, &input) else {
            return false;
        };
        self.send_view(id, output, action);
        true
    }

    pub fn search_close(&mut self, pane: f64, output: bool, swallow: Option<String>) {
        let id = pane_id(pane);
        let view = self.views.get(id, output);
        view.search = None;
        view.swallowed = swallow;
        self.send_view(id, output, TerminalViewAction::SearchClose);
    }

    pub fn key(
        &mut self,
        pane: f64,
        key: &str,
        text: Option<String>,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) {
        let response = self.key_down(
            pane,
            false,
            key,
            text.clone(),
            shift,
            control,
            alt,
            meta,
            false,
        );
        if response.contains("\"text\"")
            && let Some(text) = text
        {
            self.type_text(pane, false, &text, false);
        }
    }

    pub fn paste(&mut self, pane: f64, text: String) {
        self.terminal_paste(pane, false, text);
    }

    pub fn scroll(&mut self, pane: f64, lines: i32) {
        self.view(pane, TerminalViewAction::ScrollLines(lines));
    }

    pub fn resize(
        &mut self,
        pane: f64,
        columns: u16,
        rows: u16,
        cell_width: u32,
        cell_height: u32,
    ) {
        self.input(InputMessage::ResizeTerminalV2 {
            pane: pane_id(pane),
            columns,
            rows,
            cell_width_px: cell_width,
            cell_height_px: cell_height,
            layout_generation: self.core.layout_generation(),
        });
    }

    pub fn resize_output(&mut self, columns: u16, rows: u16, cell_width: u32, cell_height: u32) {
        self.input(InputMessage::ResizeCommandOutput {
            columns,
            rows,
            cell_width_px: cell_width,
            cell_height_px: cell_height,
        });
    }
}

impl Client {
    pub fn view(&mut self, pane: f64, action: TerminalViewAction) {
        self.send_view(pane_id(pane), false, action);
    }

    fn send_view(&mut self, pane: PaneId, output: bool, action: TerminalViewAction) {
        self.input(if output {
            InputMessage::CommandOutputView { action }
        } else {
            InputMessage::TerminalView { pane, action }
        });
    }

    fn send_key(&mut self, pane: PaneId, input: KeyInput) {
        self.input(InputMessage::Key {
            pane,
            input,
            text_follows: false,
        });
    }

    fn surface_viewport(&self, pane: PaneId, output: bool) -> Option<&TerminalViewport> {
        if output {
            self.core.command_output().map(|(_, viewport)| viewport)
        } else {
            self.core.viewport(pane)
        }
    }

    fn request_history(&mut self, pane: PaneId, prefetch: Option<u32>) {
        if let Some((start, count)) = self.history.pacer.request(
            pane,
            prefetch,
            self.core.history_trickle_budget(),
            self.core.retained_viewport(pane),
            Instant::now(),
        ) {
            self.send(&ProtocolMessage::HistoryRequest { pane, start, count });
        }
    }

    fn apply_step(&mut self, pane: PaneId, step: LocalScrollStep) -> (bool, Vec<u64>) {
        let mut syncs = Vec::new();
        for effect in step.effects {
            match effect {
                LocalScrollEffect::ScrollToOffset(offset) => {
                    self.send_view(pane, false, TerminalViewAction::ScrollToOffset(offset));
                }
                LocalScrollEffect::Prefetch(target) => self.request_history(pane, Some(target)),
                LocalScrollEffect::Sync(generation) => syncs.push(generation),
            }
        }
        (step.redraw, syncs)
    }

    fn scroll_to_pointer(&mut self, pane: PaneId, output: bool, y: f32) -> (bool, Vec<u64>) {
        let view = self.views.get(pane, output);
        let [_, top, _, height] = view.surface;
        let fraction = scroll_fraction(top, height, y);
        let redraw = view.scroll.clear_sub_row();
        let maximum = local_retained(&self.core, pane, output)
            .map(|retained| retained.viewport.scrollbar)
            .map(|bar| bar.total.saturating_sub(bar.len));
        let Some(maximum) = maximum else {
            self.send_view(pane, output, TerminalViewAction::ScrollToFraction(fraction));
            return (redraw, Vec::new());
        };
        let now = Instant::now();
        let step = local_retained(&self.core, pane, output).and_then(|retained| {
            Some(self.views.panes.get_mut(&pane)?.scroll.scroll_to(
                scroll_fraction_offset(fraction, maximum),
                retained,
                now,
            ))
        });
        let (stepped, syncs) = step.map_or((false, Vec::new()), |step| self.apply_step(pane, step));
        (redraw || stepped, syncs)
    }

    fn begin_search(
        &mut self,
        pane: PaneId,
        output: bool,
        query: SearchQuery,
        accept_on_enter: bool,
    ) {
        let view = self.views.get(pane, output);
        view.scroll.cancel();
        view.search = Some(query.clone());
        view.accept_on_enter = accept_on_enter;
        self.send_view(pane, output, TerminalViewAction::SearchBegin(query));
    }
}

fn key_input(key: &str, text: Option<String>, modifiers: Modifiers, action: KeyAction) -> KeyInput {
    let code = key_code(key);
    let character = match code {
        KeyCode::Character(value) => Some(value),
        _ => None,
    };
    KeyInput {
        action,
        key: code,
        modifiers,
        text: text
            .filter(|_| !modifiers.alt())
            .or_else(|| character.map(|value| value.to_string()))
            .filter(|text| !text.chars().any(char::is_control))
            .map(String::into_boxed_str),
        unshifted_codepoint: character,
    }
}

pub fn key_code(key: &str) -> KeyCode {
    match key {
        "Backspace" => KeyCode::Backspace,
        "Enter" => KeyCode::Enter,
        "Tab" => KeyCode::Tab,
        "Escape" => KeyCode::Escape,
        "Delete" => KeyCode::Delete,
        "Insert" => KeyCode::Insert,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "ArrowUp" => KeyCode::ArrowUp,
        "ArrowDown" => KeyCode::ArrowDown,
        "ArrowLeft" => KeyCode::ArrowLeft,
        "ArrowRight" => KeyCode::ArrowRight,
        value if value.chars().count() == 1 => {
            KeyCode::Character(value.chars().next().unwrap_or(' '))
        }
        value => value
            .strip_prefix('F')
            .and_then(|number| number.parse::<u8>().ok())
            .filter(|number| (1..=24).contains(number))
            .map_or(KeyCode::Unidentified, KeyCode::Function),
    }
}

pub fn hex(color: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

fn rgba(color: AppearanceColor) -> String {
    format!(
        "rgb({} {} {} / {:.3})",
        color.r,
        color.g,
        color.b,
        f32::from(color.a) / 255.0
    )
}

fn ink(color: Color) -> String {
    format!(
        "rgb({} {} {} / var(--term-alpha, 1))",
        color.r, color.g, color.b
    )
}

fn glyph(dictionary: &TerminalDictionary, cell: PackedCell) -> Glyph<'_> {
    let glyph = cell.glyph();
    if glyph == 0 {
        return Glyph::Empty;
    }
    if glyph & GRAPHEME_TABLE_BIT == 0 {
        return char::from_u32(glyph).map_or(Glyph::Empty, Glyph::Scalar);
    }
    let index = (glyph & !GRAPHEME_TABLE_BIT) as usize;
    let Some((&start, &end)) = dictionary
        .grapheme_offsets
        .get(index)
        .zip(dictionary.grapheme_offsets.get(index.saturating_add(1)))
    else {
        return Glyph::Empty;
    };
    dictionary
        .grapheme_bytes
        .get(start as usize..end as usize)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map_or(Glyph::Empty, Glyph::Grapheme)
}

struct Painter<'a> {
    appearance: &'a TerminalAppearance,
    foreground: Color,
    background: Color,
    styles: Vec<String>,
    index: HashMap<String, usize>,
    contrast: &'a mut HashMap<(u32, u32, bool, bool), Color>,
}

impl Painter<'_> {
    fn style(&self, dictionary: &TerminalDictionary, cell: PackedCell) -> PackedStyle {
        dictionary
            .styles
            .get(usize::from(cell.style_id()))
            .copied()
            .unwrap_or_else(|| {
                PackedStyle::new(
                    self.foreground,
                    self.background,
                    None,
                    0,
                    UnderlineStyle::None,
                )
            })
    }

    fn foreground(&mut self, style: PackedStyle, decorative: bool) -> Color {
        let key = (
            style.foreground().packed(),
            style.background().packed(),
            style.faint(),
            decorative,
        );
        if let Some(color) = self.contrast.get(&key) {
            return *color;
        }
        if self.contrast.len() >= 4096 {
            self.contrast.clear();
        }
        let color = resolved_foreground(style, self.appearance.minimum_contrast, decorative);
        self.contrast.insert(key, color);
        color
    }

    fn css(&mut self, style: PackedStyle, decorative: bool, selected: bool) -> usize {
        let foreground = if selected {
            self.appearance.selection_foreground
        } else {
            self.foreground(style, decorative)
        };
        let mut rules = String::new();
        if style.invisible() {
            rules.push_str("color:transparent;");
        } else if foreground != self.foreground {
            let _ = write!(rules, "color:{};", ink(foreground));
        }
        if style.bold() {
            let _ = write!(
                rules,
                "font-weight:{};",
                (self.appearance.font_weight + 300).min(900)
            );
        }
        if style.italic() {
            rules.push_str("font-style:italic;");
        }
        let underline = match style.underline() {
            UnderlineStyle::Single => {
                Some(("solid", style.underline_color().unwrap_or(foreground)))
            }
            UnderlineStyle::Curly => Some(("wavy", style.underline_color().unwrap_or(foreground))),
            UnderlineStyle::Double => {
                Some(("double", style.underline_color().unwrap_or(foreground)))
            }
            UnderlineStyle::Dotted => {
                Some(("dotted", style.underline_color().unwrap_or(foreground)))
            }
            UnderlineStyle::Dashed => {
                Some(("dashed", style.underline_color().unwrap_or(foreground)))
            }
            UnderlineStyle::None if style.hyperlink() => {
                Some(("solid", self.appearance.link_color))
            }
            UnderlineStyle::None => None,
        };
        let mut lines = Vec::new();
        if underline.is_some() {
            lines.push("underline");
        }
        if style.strikethrough() {
            lines.push("line-through");
        }
        if !lines.is_empty() {
            let (kind, color) = underline.unwrap_or(("solid", foreground));
            let _ = write!(
                rules,
                "text-decoration:{} {kind} {};text-decoration-thickness:var(--term-hair);",
                lines.join(" "),
                ink(color)
            );
        }
        if let Some(index) = self.index.get(&rules) {
            return *index;
        }
        let index = self.styles.len();
        self.index.insert(rules.clone(), index);
        self.styles.push(rules);
        index
    }

    fn line(
        &mut self,
        cells: &[PackedCell],
        dictionary: &TerminalDictionary,
        selected: &[(u16, u16)],
        hidden: Option<(u16, u16)>,
    ) -> Value {
        let mut runs: Vec<(String, usize, bool)> = Vec::new();
        let mut backgrounds: Vec<(usize, usize, Color)> = Vec::new();
        for (column, cell) in cells.iter().copied().enumerate() {
            let style = self.style(dictionary, cell);
            let background = style.background();
            match backgrounds.last_mut() {
                Some((start, count, color))
                    if *color == background && *start + *count == column =>
                {
                    *count += 1;
                }
                _ => backgrounds.push((column, 1, background)),
            }
            if cell.width() == CellWidth::SpacerTail {
                continue;
            }
            let at = column as u16;
            let shown = !hidden.is_some_and(|(start, end)| at >= start && at < end);
            let glyph = glyph(dictionary, cell);
            let text = match glyph {
                _ if !shown => " ".to_owned(),
                Glyph::Empty => " ".to_owned(),
                Glyph::Scalar(character) => character.to_string(),
                Glyph::Grapheme(text) => text.to_owned(),
            };
            let is_selected = selected
                .iter()
                .any(|(start, end)| at >= *start && at < *end);
            let css = self.css(style, decorative_glyph(glyph), is_selected);
            let wide = cell.width() == CellWidth::Wide;
            match runs.last_mut() {
                Some((run, index, false)) if !wide && *index == css => run.push_str(&text),
                _ => runs.push((text, css, wide)),
            }
        }
        json!({
            "runs": runs
                .into_iter()
                .map(|(text, css, wide)| if wide { json!([text, css, 1]) } else { json!([text, css]) })
                .collect::<Vec<_>>(),
            "bg": backgrounds
                .into_iter()
                .filter(|(_, _, color)| *color != self.background)
                .map(|(start, count, color)| json!([start, count, hex(color)]))
                .collect::<Vec<_>>(),
        })
    }
}

fn overlay_color(kind: OverlayKind, appearance: &TerminalAppearance) -> String {
    rgba(match kind {
        OverlayKind::Selection => appearance.selection_background,
        OverlayKind::SearchMatch => appearance.search_match_color,
        OverlayKind::SearchCurrent => appearance.search_current_color,
        OverlayKind::LinkHover => AppearanceColor::rgba(
            appearance.link_color.r,
            appearance.link_color.g,
            appearance.link_color.b,
            72,
        ),
        OverlayKind::CopyCursor => appearance.copy_cursor_color,
    })
}

fn selection_covers(viewport: &TerminalViewport, row: u16, column: u16) -> bool {
    viewport.overlays.iter().any(|candidate| {
        candidate.kind() == OverlayKind::Selection
            && candidate.flags() & OVERLAY_RECTANGLE == 0
            && candidate.row == row
            && candidate.start <= column
            && column < candidate.end
    })
}

fn selection_corners(viewport: &TerminalViewport, row: u16, start: u16, end: u16) -> [bool; 4] {
    let end = end.saturating_sub(1);
    let above = |column| {
        row.checked_sub(1)
            .is_some_and(|row| selection_covers(viewport, row, column))
    };
    let below = |column| {
        row.checked_add(1)
            .is_some_and(|row| selection_covers(viewport, row, column))
    };
    [!above(start), !above(end), !below(end), !below(start)]
}

fn kind_name(kind: OverlayKind) -> &'static str {
    match kind {
        OverlayKind::Selection => "selection",
        OverlayKind::SearchMatch => "match",
        OverlayKind::SearchCurrent => "current",
        OverlayKind::LinkHover => "link",
        OverlayKind::CopyCursor => "copy",
    }
}

fn render(
    retained: &RetainedTerminalViewport,
    appearance: &TerminalAppearance,
    view: Option<&View>,
    output: bool,
    grid_rows: u16,
    hidden: u16,
    painter: &mut Painter<'_>,
    image_size: impl Fn(u32, u64) -> Option<(u32, u32)>,
) -> Value {
    let viewport = &retained.viewport;
    let target = view
        .filter(|_| !output)
        .and_then(|view| view.scroll.target());
    let sub_row = view.map_or(0.0, |view| view.scroll.sub_row());
    let grid_rows = if grid_rows == 0 {
        viewport.rows
    } else {
        grid_rows
    };
    let placement = grid_placement(viewport, None, output, target.is_some() || sub_row > 0.0);
    let projection = RowProjection::new(viewport.rows, grid_rows, placement.project_from_bottom);
    let live = target.map_or(projection, |target| {
        local_live_projection(target, viewport.scrollbar.offset, viewport.rows, grid_rows)
    });
    let history_rows = retained.history.len();
    let below_rows = retained.below.len();
    let server = viewport.scrollbar.offset;
    let cursor = viewport.cursor.filter(|cursor| cursor.visible());
    let hidden_cells = cursor.filter(|_| hidden > 0).map(|cursor| {
        (
            cursor.row(),
            cursor.column(),
            cursor.column().saturating_add(hidden),
        )
    });
    let mut selections: HashMap<u16, Vec<(u16, u16)>> = HashMap::new();
    for span in viewport
        .overlays
        .iter()
        .filter(|span| span.kind() == OverlayKind::Selection && span.start < span.end)
    {
        selections
            .entry(span.row)
            .or_default()
            .push((span.start, span.end.min(viewport.columns)));
    }
    let row_json = |source: LocalRowSource, painter: &mut Painter<'_>| -> Value {
        match source {
            LocalRowSource::Live(row) => viewport.row(row).map_or(Value::Null, |cells| {
                painter.line(
                    cells,
                    &viewport.dictionary,
                    selections.get(&row).map_or(&[][..], Vec::as_slice),
                    hidden_cells
                        .filter(|(hidden_row, _, _)| *hidden_row == row)
                        .map(|(_, start, end)| (start, end)),
                )
            }),
            LocalRowSource::History(index) => {
                retained.history.rows.get(index).map_or(Value::Null, |row| {
                    painter.line(&row.cells, &row.dictionary, &[], None)
                })
            }
            LocalRowSource::Below(index) => {
                retained.below.rows.get(index).map_or(Value::Null, |row| {
                    painter.line(&row.cells, &row.dictionary, &[], None)
                })
            }
            LocalRowSource::Shimmer => json!({ "shimmer": true }),
        }
    };
    let mut lines = vec![Value::Null; usize::from(grid_rows)];
    for offset in 0..projection.count {
        let Some(source_row) = projection.source_row(offset) else {
            continue;
        };
        let display = usize::from(projection.destination_start + offset);
        let source = target.map_or(LocalRowSource::Live(source_row), |target| {
            local_row_source(
                target.saturating_add(u32::from(source_row)),
                server,
                viewport.rows,
                history_rows,
                below_rows,
            )
        });
        if let Some(line) = lines.get_mut(display) {
            *line = row_json(source, painter);
        }
    }
    let above = (sub_row > 0.0)
        .then(|| {
            peek_row_source(
                target.unwrap_or(server),
                1,
                server,
                viewport.rows,
                history_rows,
            )
        })
        .flatten()
        .map_or(Value::Null, |source| row_json(source, painter));
    let mut overlays = Vec::new();
    for span in viewport
        .overlays
        .iter()
        .filter(|span| span.kind() == OverlayKind::CopyCursor)
    {
        if let Some(row) = live.display_row(span.row) {
            let mut color = appearance.copy_cursor_color;
            color.a = (f32::from(color.a) * 0.25).round() as u8;
            overlays.push(
                json!({ "row": row, "start": 0, "end": -1, "kind": "line", "color": rgba(color) }),
            );
        }
    }
    for span in viewport.overlays.iter() {
        let Some(row) = live.display_row(span.row) else {
            continue;
        };
        if span.start >= span.end {
            continue;
        }
        let start = span.start.min(viewport.columns);
        let end = span.end.min(viewport.columns);
        let rounded = appearance.rounded_selection
            && span.kind() == OverlayKind::Selection
            && span.flags() & OVERLAY_RECTANGLE == 0;
        overlays.push(json!({
            "row": row,
            "start": start,
            "end": end,
            "kind": kind_name(span.kind()),
            "color": overlay_color(span.kind(), appearance),
            "corners": rounded.then(|| selection_corners(viewport, span.row, span.start, span.end)),
            "underline": (span.kind() == OverlayKind::LinkHover).then(|| hex(appearance.link_color)),
        }));
    }
    let cursor_json = cursor.and_then(|cursor| {
        let row = live.display_row(cursor.row())?;
        let cells = viewport.row(cursor.row())?;
        let mut column = usize::from(cursor.column());
        if column >= usize::from(viewport.columns) {
            return None;
        }
        let mut cell = *cells.get(column)?;
        if (cursor.at_wide_tail() || cell.width() == CellWidth::SpacerTail) && column > 0 {
            column -= 1;
            cell = *cells.get(column)?;
        }
        let width = if cell.width() == CellWidth::Wide {
            2
        } else {
            1
        };
        let style = painter.style(&viewport.dictionary, cell);
        let text = match glyph(&viewport.dictionary, cell) {
            _ if style.invisible() => String::new(),
            Glyph::Empty => String::new(),
            Glyph::Scalar(character) if character.is_whitespace() => String::new(),
            Glyph::Scalar(character) => character.to_string(),
            Glyph::Grapheme(text) if text.chars().all(char::is_whitespace) => String::new(),
            Glyph::Grapheme(text) => text.to_owned(),
        };
        Some(json!({
            "column": column,
            "row": row,
            "width": width.min(usize::from(viewport.columns).saturating_sub(column)),
            "text": text,
            "bold": style.bold(),
            "italic": style.italic(),
            "color": hex(cursor.color()),
            "blinks": cursor_should_blink(Some(cursor), appearance.cursor_blink_policy, true),
            "style": match cursor.style() {
                CursorStyle::Bar => "bar",
                CursorStyle::Block => "block",
                CursorStyle::Underline => "underline",
                CursorStyle::BlockHollow => "hollow",
            },
        }))
    });
    let kitty = viewport
        .kitty_placements
        .iter()
        .filter_map(|placement| {
            let (width, height) = image_size(placement.image_id, placement.image_generation)?;
            let row = kitty_display_row(placement, target, projection)?;
            if placement.pixel_width == 0
                || placement.pixel_height == 0
                || placement.grid_cols == 0
                || placement.grid_rows == 0
            {
                return None;
            }
            Some(json!({
                "image": placement.image_id,
                "generation": placement.image_generation,
                "layer": match placement.layer {
                    KittyLayer::BelowBg => "bg",
                    KittyLayer::BelowText => "below",
                    KittyLayer::AboveText => "above",
                },
                "column": placement.viewport_col,
                "row": row,
                "offsetX": placement.cell_offset_x,
                "offsetY": placement.cell_offset_y,
                "columns": placement.grid_cols,
                "rows": placement.grid_rows,
                "source": placement.source_rect.map(|(x, y, w, h)| [x, y, w, h]),
                "width": width,
                "height": height,
            }))
        })
        .collect::<Vec<_>>();
    let scrollbar = target.map_or(viewport.scrollbar, |offset| zz_terminal::ScrollbarState {
        total: viewport.scrollbar.total,
        offset,
        len: viewport.scrollbar.len,
    });
    let hovered = viewport.presentation.hovered_uri.as_deref();
    let search = view.and_then(|view| view.search.as_ref()).map(|query| {
        let status = viewport.search;
        json!({
            "text": query.text,
            "regex": query.mode == SearchMode::Regex,
            "caseSensitive": query.case == SearchCase::Sensitive,
            "caseLabel": search_case_label(query.case),
            "result": if query.text.is_empty() { String::new() } else { search_result_text(status) },
            "invalid": status.is_some_and(zz_terminal::SearchStatus::invalid_pattern),
            "enabled": status.is_some_and(|status| status.total > 0 && !status.pending() && !status.invalid_pattern()),
        })
    });
    json!({
        "columns": viewport.columns,
        "rows": viewport.rows,
        "foreground": hex(viewport.foreground),
        "background": hex(viewport.background),
        "title": viewport.presentation.title.as_ref(),
        "cursor": cursor_json,
        "lines": lines,
        "above": above,
        "styles": painter.styles,
        "overlays": overlays,
        "kitty": kitty,
        "scrollbar": [scrollbar.total, scrollbar.offset, scrollbar.len],
        "subRow": sub_row,
        "bottomAnchored": placement.bottom_anchored,
        "mode": mode_text(viewport.mode, viewport.unseen_output).map(|(label, detail)| json!({
            "label": label,
            "detail": detail,
        })),
        "status": status_text(&viewport.status),
        "hovered": hovered.map(|uri| json!({
            "uri": presented_uri(uri),
            "image": pasted_image_number(uri),
        })),
        "search": search,
        "mouseTracking": viewport.mouse_tracking,
        "kittyKeyboard": viewport.kitty_keyboard,
        "unseen": viewport.unseen_output,
    })
}
