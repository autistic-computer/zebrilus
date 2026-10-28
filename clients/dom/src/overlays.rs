use std::collections::BTreeSet;

use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use zz_client::{
    MenuBox, MenuKeyResult, MenuPointerKind, chooser,
    floating::PixelRect,
    menu_hit_cell, menu_row_height, resolve_menu_key, resolve_menu_mouse, resolve_menu_paste,
    tmux_style::{literal_rgb, literal_style_colour, split_tmux_alignment},
};
use zz_protocol::{
    ChooseBufferAction, ChooseTreeAction, ChooseTreeKind, ChooseTreePaneKind, ChooseTreeTarget,
    ConfirmAction, DisplayPanesAction, InputMessage, MenuState, PopupBorderLines, StyledSegment,
    TmuxAttributeState, TmuxColour, parse_styled_segments,
};
use zz_terminal::{KeyAction, KeyCode, KeyInput, Modifiers};

use crate::{Client, pane_id};

const DISPLAY_PANES_COLOUR: TmuxColour = TmuxColour::Theme(7);
const DISPLAY_PANES_ACTIVE_COLOUR: TmuxColour = TmuxColour::Theme(6);

#[derive(Default)]
pub struct Overlays {
    menu_selection: Option<usize>,
    chooser_keys: BTreeSet<String>,
    prefix_cancel: Option<u64>,
    next_cancel: u64,
}

pub fn key_input(
    key: &str,
    text: Option<String>,
    (shift, control, alt, meta): (bool, bool, bool, bool),
    action: KeyAction,
) -> KeyInput {
    let code = crate::terminal::key_code(key);
    let character = match code {
        KeyCode::Character(value) => Some(value),
        _ => None,
    };
    KeyInput {
        action,
        key: code,
        modifiers: Modifiers::new(shift, control, alt, meta),
        text: text
            .or_else(|| character.map(|value| value.to_string()))
            .filter(|text| !text.chars().any(char::is_control))
            .map(String::into_boxed_str),
        unshifted_codepoint: character,
    }
}

fn hex(rgb: u32) -> String {
    format!("#{rgb:06x}")
}

pub fn colour(colour: Option<TmuxColour>) -> Value {
    match colour {
        Some(TmuxColour::Theme(index)) => json!(format!("theme:{index}")),
        Some(colour) => literal_rgb(colour).map_or(Value::Null, |rgb| json!(hex(rgb))),
        None => Value::Null,
    }
}

fn on(state: TmuxAttributeState) -> Option<bool> {
    match state {
        TmuxAttributeState::On => Some(true),
        TmuxAttributeState::Off => Some(false),
        TmuxAttributeState::Unset => None,
    }
}

pub fn segments(segments: &[StyledSegment]) -> Value {
    Value::Array(
        segments
            .iter()
            .map(|segment| {
                let style = &segment.style;
                let attributes = &style.attributes;
                let underline = [
                    attributes.underscore,
                    attributes.double_underscore,
                    attributes.curly_underscore,
                    attributes.dotted_underscore,
                    attributes.dashed_underscore,
                ]
                .into_iter()
                .any(|state| state == TmuxAttributeState::On);
                json!({
                    "text": segment.text,
                    "fg": colour(style.fg),
                    "bg": colour(style.bg),
                    "us": colour(style.us),
                    "bold": on(attributes.bold),
                    "italic": on(attributes.italics),
                    "reverse": attributes.reverse == TmuxAttributeState::On,
                    "underline": underline,
                    "strike": attributes.strikethrough == TmuxAttributeState::On,
                    "fade": style.dim_percentage.map_or_else(
                        || (attributes.dim == TmuxAttributeState::On).then_some(0.5),
                        |percentage| Some(f32::from(percentage) / 100.0),
                    ),
                })
            })
            .collect(),
    )
}

fn aligned(label: &str) -> Value {
    Value::Array(
        split_tmux_alignment(label)
            .iter()
            .map(|bucket| segments(bucket))
            .collect(),
    )
}

fn literal(style: &str, key: &str) -> Value {
    literal_style_colour(style, key).map_or(Value::Null, |rgb| json!(hex(rgb)))
}

fn menu_box(state: &MenuState) -> MenuBox {
    MenuBox {
        left: state.left,
        top: state.top,
        width: state.width,
        items: state.items.len(),
    }
}

fn rect(values: &[f32]) -> PixelRect {
    let at = |index: usize| values.get(index).copied().unwrap_or_default();
    PixelRect {
        x: at(0),
        y: at(1),
        width: at(2),
        height: at(3),
    }
}

fn pointer_kind(kind: &str) -> MenuPointerKind {
    match kind {
        "press" => MenuPointerKind::Press,
        "release" => MenuPointerKind::Release,
        "drag" => MenuPointerKind::Drag,
        "wheel" => MenuPointerKind::Wheel,
        _ => MenuPointerKind::Motion,
    }
}

fn hints(hints: &[chooser::ChooserHint]) -> Value {
    Value::Array(
        hints
            .iter()
            .map(|hint| {
                json!({
                    "keys": hint.keys.iter().map(|key| glyphs(key, false)).collect::<Vec<_>>(),
                    "label": hint.label,
                })
            })
            .collect(),
    )
}

fn key_icon(key: &str) -> Option<&'static str> {
    Some(match key {
        "ctrl" => "key-control-iso",
        "alt" => "key-alt-iso",
        "shift" => "key-shift",
        "escape" => "key-escape",
        "tab" => "key-tab-iso",
        "enter" => "key-return",
        "backspace" => "key-backspace",
        "up" => "arrow-up",
        "down" => "arrow-down",
        "left" => "arrow-left",
        "right" => "arrow-right",
        _ => return None,
    })
}

fn key_symbol(key: &str) -> String {
    match key {
        "ctrl" => "Ctrl".to_owned(),
        "alt" => "Alt".to_owned(),
        "shift" => "Shift".to_owned(),
        "cmd" => "Win".to_owned(),
        "backspace" => "Backspace".to_owned(),
        "delete" => "Delete".to_owned(),
        "escape" => "Esc".to_owned(),
        "enter" => "Enter".to_owned(),
        "left" => "Left".to_owned(),
        "right" => "Right".to_owned(),
        "up" => "Up".to_owned(),
        "down" => "Down".to_owned(),
        "space" => "Space".to_owned(),
        "pagedown" => "Page Down".to_owned(),
        "pageup" => "Page Up".to_owned(),
        other => {
            let mut chars = other.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    }
}

pub fn glyphs(stroke: &str, lowercase: bool) -> Value {
    let (modifiers, key) = match stroke.strip_suffix("--") {
        Some(rest) => (rest, "-"),
        None => stroke.rsplit_once('-').unwrap_or(("", stroke)),
    };
    let held: Vec<&str> = modifiers
        .split('-')
        .filter(|part| !part.is_empty())
        .collect();
    let mut parts: Vec<Result<&'static str, String>> = ["ctrl", "alt", "shift", "cmd"]
        .into_iter()
        .filter(|name| held.contains(name))
        .map(|name| key_icon(name).ok_or_else(|| key_symbol(name)))
        .collect();
    parts.push(key_icon(key).ok_or_else(|| {
        let name = key_symbol(key);
        if lowercase { name.to_lowercase() } else { name }
    }));
    let mut merged: Vec<Result<&'static str, String>> = Vec::new();
    for part in parts {
        match (merged.last_mut(), part) {
            (Some(Err(text)), Err(next)) => {
                text.push('+');
                text.push_str(&next);
            }
            (_, part) => merged.push(part),
        }
    }
    Value::Array(
        merged
            .into_iter()
            .map(|part| match part {
                Ok(icon) => json!({ "icon": icon }),
                Err(text) => json!({ "text": text }),
            })
            .collect(),
    )
}

#[wasm_bindgen]
impl Client {
    pub fn menu_changed(&mut self) {
        self.overlays.menu_selection = self
            .core
            .menu()
            .and_then(|menu| menu.selected)
            .map(|index| index as usize);
    }

    pub fn menu(
        &self,
        canvas_width: f32,
        canvas_height: f32,
        scale: f32,
        room_height: f32,
    ) -> Option<String> {
        let state = self.core.menu()?;
        let titled = state.border_lines != PopupBorderLines::None && !state.title.is_empty();
        let outlined = !matches!(
            state.border_lines,
            PopupBorderLines::None | PopupBorderLines::Padded
        );
        let frame = zz_client::floating::overlay_frame(
            (state.left, state.top, state.width, state.height),
            (state.client_columns, state.client_rows),
            (state.cell_width_px, state.cell_height_px),
            state.border_lines != PopupBorderLines::None,
            (canvas_width, canvas_height),
            scale,
        );
        let selected = self.overlays.menu_selection;
        Some(
            json!({
                "frame": { "x": frame.bounds.x, "y": frame.bounds.y },
                "titled": titled,
                "outlined": outlined,
                "title": aligned(&state.title),
                "rowHeight": menu_row_height(state, titled, room_height, 26.0, true),
                "fill": literal(&state.style, "bg"),
                "text": literal(&state.style, "fg"),
                "outline": literal(&state.border_style, "fg"),
                "selectedFill": literal(&state.selected_style, "bg"),
                "selectedText": literal(&state.selected_style, "fg"),
                "items": state.items.iter().enumerate().map(|(index, item)| match item {
                    Some(item) => json!({
                        "name": segments(&parse_styled_segments(&item.name)),
                        "annotation": item.annotation,
                        "enabled": item.enabled,
                        "selected": selected == Some(index) && item.enabled,
                    }),
                    None => Value::Null,
                }).collect::<Vec<_>>(),
            })
            .to_string(),
        )
    }

    pub fn menu_pointer(
        &mut self,
        kind: &str,
        buttons: u8,
        x: f32,
        y: f32,
        surface: Vec<f32>,
        rows: Vec<f32>,
        tops: Vec<f32>,
    ) -> bool {
        let Some(state) = self.core.menu().cloned() else {
            return false;
        };
        let kind = pointer_kind(kind);
        if !state.mouse_keys && kind == MenuPointerKind::Motion {
            return false;
        }
        let (column, row) = menu_hit_cell(&state, rect(&surface), rect(&rows), &tops, (x, y));
        let result = resolve_menu_mouse(
            &state,
            self.overlays.menu_selection,
            menu_box(&state),
            kind,
            buttons,
            column,
            row,
        );
        self.menu_result(result)
    }

    pub fn menu_paste(&mut self, text: &str) -> Option<String> {
        let state = self.core.menu()?.clone();
        let result = resolve_menu_paste(&state, self.overlays.menu_selection, text);
        self.overlays.menu_selection = result.selection;
        if let Some(action) = result.action {
            self.input(InputMessage::Menu { action });
        }
        Some(result.remainder)
    }

    pub fn confirm(&self, canvas_width: f32) -> Option<String> {
        let state = self.core.confirm()?;
        let (width, height) = zz_client::floating::confirm_size(&state.prompt, canvas_width);
        Some(json!({ "prompt": state.prompt, "width": width, "height": height }).to_string())
    }

    pub fn choose_tree(&self) -> Option<String> {
        let state = self.core.choose_tree()?;
        if state.kind == ChooseTreeKind::Windows {
            return None;
        }
        let (title, targets) = chooser::tree_title(state.kind);
        let selected = state.selected as usize;
        let rows: Vec<_> = state
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let (label, detail) = if item.text.is_empty() {
                    (item.label.clone(), item.detail.clone())
                } else {
                    (item.text.clone(), String::new())
                };
                let icon = match item.pane_kind {
                    Some(ChooseTreePaneKind::Terminal) => "square-terminal",
                    Some(ChooseTreePaneKind::Browser) => "globe",
                    Some(ChooseTreePaneKind::Editor) => "file",
                    None if matches!(item.target, ChooseTreeTarget::Session(_)) => "folder",
                    None => "panels-top-left",
                };
                json!({
                    "key": chooser::chooser_key_label(&item.key),
                    "target": item.target.to_string(),
                    "label": label,
                    "detail": detail,
                    "depth": item.depth,
                    "disclosure": item.has_children().then(|| item.expanded()),
                    "icon": icon,
                    "active": item.active(),
                    "tagged": item.tagged(),
                    "selected": index == selected,
                })
            })
            .collect();
        Some(
            json!({
                "kind": "tree",
                "title": title,
                "subtitle": chooser::chooser_subtitle(
                    format!("{} {targets}", state.items.len()),
                    state.filter_no_matches,
                ),
                "maxWidth": 600,
                "keys": chooser::chooser_has_key_gutter(state.items.iter().map(|item| item.key.as_str())),
                "prompt": state.prompt,
                "help": state.help,
                "search": state.search.as_ref().map(|search| json!({
                    "prefix": if search.reverse { "?" } else { "/" },
                    "value": search.query,
                })),
                "hints": hints(if state.search.is_some() { chooser::SEARCH_HINTS } else { chooser::TREE_HINTS }),
                "selected": selected,
                "rows": rows,
            })
            .to_string(),
        )
    }

    pub fn choose_buffer(&self, now: f64) -> Option<String> {
        let state = self.core.choose_buffer()?;
        let selected = state.selected as usize;
        let rows: Vec<_> = state
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let (name, preview, size, age) = chooser::buffer_row_text(item, now as u64);
                json!({
                    "key": chooser::chooser_key_label(&item.key),
                    "name": name,
                    "preview": preview,
                    "size": size,
                    "age": age,
                    "tagged": item.tagged,
                    "selected": index == selected,
                })
            })
            .collect();
        Some(
            json!({
                "kind": "buffer",
                "title": "Paste buffer",
                "subtitle": chooser::chooser_subtitle(
                    format!("{} buffers", state.items.len()),
                    state.filter_no_matches,
                ),
                "maxWidth": 640,
                "keys": chooser::chooser_has_key_gutter(state.items.iter().map(|item| item.key.as_str())),
                "prompt": "",
                "help": state.help,
                "search": state.search.as_ref().map(|search| json!({
                    "prefix": if search.reverse { "?" } else { "/" },
                    "value": search.query,
                })),
                "hints": hints(if state.search.is_some() { chooser::SEARCH_HINTS } else { chooser::BUFFER_HINTS }),
                "selected": selected,
                "rows": rows,
            })
            .to_string(),
        )
    }

    pub fn chooser_action(&mut self, action: &str, index: u32) {
        if self.core.choose_buffer().is_some() && self.core.choose_tree().is_none() {
            let action = match action {
                "select" => ChooseBufferAction::Select(index),
                "activate" => ChooseBufferAction::PasteIndex(index),
                _ => ChooseBufferAction::Close,
            };
            self.input(InputMessage::ChooseBuffer { action });
        } else {
            let action = match action {
                "select" => ChooseTreeAction::Select(index),
                "activate" => ChooseTreeAction::ActivateIndex(index),
                _ => ChooseTreeAction::Close,
            };
            self.input(InputMessage::ChooseTree { action });
        }
    }

    pub fn display_panes(&self) -> Option<String> {
        let state = self.core.display_panes()?;
        let indicators: Vec<_> = state
            .indicators
            .iter()
            .map(|indicator| {
                let active = indicator.active();
                let (option, default) = if active {
                    (state.active_colour, DISPLAY_PANES_ACTIVE_COLOUR)
                } else {
                    (state.colour, DISPLAY_PANES_COLOUR)
                };
                let option = option.filter(|colour| *colour != default);
                let key = indicator.selection_key();
                json!({
                    "pane": indicator.pane.0,
                    "active": active,
                    "glyph": key.map_or_else(|| indicator.index.to_string(), String::from),
                    "keyed": key.is_some(),
                    "index": key
                        .is_some_and(|key| key.is_ascii_alphabetic())
                        .then(|| indicator.index.to_string()),
                    "colour": colour(option),
                    "label": aligned(&indicator.label),
                })
            })
            .collect();
        Some(json!({ "window": state.window.0, "indicators": indicators }).to_string())
    }

    pub fn which_key(&self) -> String {
        let prefix = self
            .core
            .mux_options()
            .get(zz_protocol::MuxOptionKey::Prefix)
            .map_or_else(
                || "C-b".to_owned(),
                |option| zz_protocol::canonical_key(&option.value),
            );
        let rows: Vec<Value> =
            zz_client::which_key::action_rows(self.core.key_tables(), "prefix", &prefix)
                .into_iter()
                .filter(|row| zz_client::palette::tmux_keystroke_source(row.first_key()).is_some())
                .map(|row| {
                    json!({
                        "id": row.first_key(),
                        "label": row.label,
                        "group": row.group.map(zz_client::which_key::WhichKeyGroup::title),
                        "repeat": row.repeat,
                        "caps": row.keys.iter().map(|set| json!({
                            "raw": set.keys.join(" "),
                            "yours": set.yours,
                            "keys": set.keys.iter().filter_map(|key| {
                                zz_client::palette::tmux_keystroke_source(key)
                                    .map(|(source, _)| glyphs(&source, true))
                            }).collect::<Vec<_>>(),
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect();
        json!({ "prefix": prefix, "armed": self.core.prefix_armed(), "table": self.core.key_table().map(|(table, _)| table), "rows": rows }).to_string()
    }

    pub fn daemon_overlay_open(&self) -> bool {
        self.core.menu().is_some()
            || self.core.confirm().is_some()
            || self.core.choose_tree().is_some()
            || self.core.choose_buffer().is_some()
            || self.core.display_panes().is_some()
            || self.core.command_prompt().is_some()
    }

    pub fn display_panes_select(&mut self, pane: f64) {
        self.input(InputMessage::DisplayPanes {
            action: DisplayPanesAction::Select(pane_id(pane)),
        });
    }

    pub fn overlay_key(
        &mut self,
        key: &str,
        text: Option<String>,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) -> bool {
        if self.core.command_prompt().is_some() {
            return false;
        }
        let input = key_input(key, text, (shift, control, alt, meta), KeyAction::Press);
        if let Some(menu) = self.core.menu().cloned() {
            let result = resolve_menu_key(&menu, self.overlays.menu_selection, &input);
            self.menu_result(result);
            return true;
        }
        if let Some(confirm) = self.core.confirm() {
            let key_char = input.text.as_deref().map(str::to_owned);
            let name = match input.key {
                KeyCode::Enter => "enter",
                _ => "",
            };
            let reply = zz_client::confirm_accepts(
                confirm.confirm_key,
                confirm.default_yes,
                name,
                key_char.as_deref(),
                (control, meta, false),
            );
            self.input(InputMessage::Confirm {
                action: ConfirmAction::Reply(reply),
            });
            return true;
        }
        let message = if self
            .core
            .choose_tree()
            .is_some_and(|state| state.kind != ChooseTreeKind::Windows)
        {
            InputMessage::ChooseTree {
                action: ChooseTreeAction::Key(input),
            }
        } else if self.core.choose_buffer().is_some() {
            InputMessage::ChooseBuffer {
                action: ChooseBufferAction::Key(input),
            }
        } else if self.core.display_panes().is_some() {
            InputMessage::DisplayPanes {
                action: DisplayPanesAction::Key(input),
            }
        } else {
            return false;
        };
        if !matches!(message, InputMessage::DisplayPanes { .. }) {
            self.overlays.chooser_keys.insert(key.to_owned());
        }
        self.input(message);
        true
    }

    pub fn overlay_key_up(
        &mut self,
        key: &str,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) -> bool {
        if !self.overlays.chooser_keys.remove(key) {
            return false;
        }
        let input = key_input(key, None, (shift, control, alt, meta), KeyAction::Release);
        let message = if self.core.choose_buffer().is_some() {
            InputMessage::ChooseBuffer {
                action: ChooseBufferAction::Key(input),
            }
        } else {
            InputMessage::ChooseTree {
                action: ChooseTreeAction::Key(input),
            }
        };
        self.input(message);
        true
    }

    pub fn cancel_prefix(&mut self) {
        if self.overlays.prefix_cancel.is_none() && self.core.attached_session().is_some() {
            self.overlays.next_cancel = self.overlays.next_cancel.wrapping_add(1).max(1);
            let request_id = self.overlays.next_cancel;
            self.overlays.prefix_cancel = Some(request_id);
            self.input(InputMessage::CancelPrefix { request_id });
        }
    }

    pub fn prefix_cancelled(&mut self, request_id: f64) {
        if self.overlays.prefix_cancel == Some(request_id as u64) {
            self.overlays.prefix_cancel = None;
        }
    }

    pub fn prefix_cancel_pending(&self) -> bool {
        self.overlays.prefix_cancel.is_some()
    }

    pub fn reset_prefix_cancel(&mut self) {
        self.overlays.prefix_cancel = None;
    }
}

impl Client {
    fn menu_result(&mut self, result: MenuKeyResult) -> bool {
        match result {
            MenuKeyResult::Action(action) => {
                self.input(InputMessage::Menu { action });
                true
            }
            MenuKeyResult::Select(selection) => {
                let changed = self.overlays.menu_selection != selection;
                self.overlays.menu_selection = selection;
                changed
            }
            MenuKeyResult::Consumed => false,
        }
    }
}
