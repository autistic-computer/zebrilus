mod places;

use std::{collections::BTreeMap, ops::Range};

use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use zz_client::{
    AgentAttentionStatus, ChromeKeymap, ChromeProfile, UI_TABLE, agent_attention_status,
    completion::{
        CompletionKind, CompletionSuggestion, PaneKindAvailability, apply_completion,
        complete_command,
    },
    navigation::{ordered_panes, pane_index, pane_label, session_label},
    palette::{
        count, fuzzy_match, has_command_arguments, needs_arguments, target_kind,
        tmux_keystroke_source,
    },
    place::Place,
};
use zz_protocol::{
    ChooseTreeAction, ChooseTreePaneKind, ChooseTreeState, ChooseTreeTarget, CommandInvocation,
    CommandPromptAction, CommandPromptKind, CommandPromptMode, CommandSpec, CommandValueKind,
    InputMessage, MAX_COMMAND_PROMPT_BYTES, MuxOptionKey, PaneId, PaneKindSnapshot,
    PaneStatusState, SessionId, WindowId, catalog_command_spec, command_specs,
};
use zz_terminal::KeyAction;

use crate::{Client, overlays::glyphs};

const ROW_HEIGHT: f32 = 28.0;
const MAX_VISIBLE_ROWS: usize = 8;
const HOST: &str = "zz daemon";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Command,
    Target,
}

impl Mode {
    const fn label(self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::Target => "Go to",
        }
    }

    const fn prefix(self) -> &'static str {
        match self {
            Self::Command => ":",
            Self::Target => "@",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Session(SessionId),
    Window(WindowId),
    Pane(PaneId),
}

impl Target {
    fn spelled(self) -> String {
        match self {
            Self::Session(id) => id.to_string(),
            Self::Window(id) => id.to_string(),
            Self::Pane(id) => id.to_string(),
        }
    }
}

#[derive(Clone)]
enum Action {
    Target { target: Target, label: String },
    Command(&'static CommandSpec),
    History(String),
    Completion(CompletionSuggestion),
    ChooseWindow(u32),
    Place(Place),
    Verb(usize),
    Fill(Place),
    Submit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Anchor {
    Target(Target),
    Place(Place),
}

impl Action {
    const fn anchor(&self) -> Option<Anchor> {
        match self {
            Self::Target { target, .. } => Some(Anchor::Target(*target)),
            Self::Place(place) | Self::Fill(place) => Some(Anchor::Place(*place)),
            _ => None,
        }
    }
}

#[derive(Clone, Default)]
struct Row {
    label: String,
    detail: String,
    right: String,
    matches: Vec<Range<usize>>,
    muted_prefix: usize,
    status: Option<&'static str>,
    running: bool,
    indent: f32,
    shortcut: Option<String>,
    icon: Option<&'static str>,
    expanded: Option<bool>,
    badge: Option<&'static str>,
}

#[derive(Clone)]
struct Entry {
    row: Row,
    action: Option<Action>,
    score: usize,
}

impl Entry {
    const fn row(row: Row, action: Action) -> Self {
        Self {
            row,
            action: Some(action),
            score: 0,
        }
    }

    fn header(label: &str) -> Self {
        Self {
            row: Row {
                label: label.to_owned(),
                ..Row::default()
            },
            action: None,
            score: 0,
        }
    }

    fn matching(mut self, query: &str, metadata: &str) -> Option<Self> {
        if let Some((score, hits)) = fuzzy_match(query, &self.row.label) {
            self.score = score;
            self.row.matches = hits;
        } else if !query.is_empty() && !metadata.is_empty() {
            self.score = fuzzy_match(query, metadata)?.0.saturating_add(1000);
        } else {
            return None;
        }
        Some(self)
    }
}

struct TreePane {
    id: PaneId,
    index: u32,
    label: String,
    program: Option<&'static str>,
    detail: &'static str,
    icon: &'static str,
    running: bool,
    needs_you: bool,
    terminal: bool,
    cwd: String,
}

struct TreeWindow {
    id: WindowId,
    index: u32,
    name: String,
    active: bool,
    active_pane: PaneId,
    panes: Vec<TreePane>,
}

impl TreeWindow {
    fn running(&self) -> bool {
        self.panes.iter().any(|pane| pane.running)
    }
}

struct TreeSession {
    id: SessionId,
    name: String,
    tmux_name: String,
    active: bool,
    windows: Vec<TreeWindow>,
}

#[derive(Default)]
struct ChooserSearch {
    expanded: Vec<ChooseTreeTarget>,
    pending: Option<(ChooseTreeTarget, bool)>,
    selection: Option<ChooseTreeTarget>,
}

impl ChooserSearch {
    fn advance(&mut self, state: &ChooseTreeState, searching: bool) -> Option<(u32, bool)> {
        if let Some((target, expanded)) = self.pending {
            if state.items.iter().any(|item| {
                item.target == target
                    && item.expanded() != expanded
                    && (!expanded || item.has_children())
            }) {
                return None;
            }
            self.pending = None;
        }
        let next = if searching {
            state.items.iter().enumerate().find_map(|(index, item)| {
                (item.has_children() && !item.expanded()).then_some((index, item.target, true))
            })
        } else {
            self.expanded.iter().rev().find_map(|target| {
                state.items.iter().enumerate().find_map(|(index, item)| {
                    (item.target == *target && item.expanded()).then_some((index, *target, false))
                })
            })
        };
        let Some((index, target, expanded)) = next else {
            if !searching {
                self.expanded.clear();
            }
            return None;
        };
        if expanded && !self.expanded.contains(&target) {
            self.expanded.push(target);
        }
        self.pending = Some((target, expanded));
        Some((index as u32, expanded))
    }
}

#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Palette {
    open: bool,
    local: bool,
    unified: bool,
    mode: Option<Mode>,
    command: Option<&'static CommandSpec>,
    places: places::Places,
    chooser_current_window: Option<u32>,
    flat: bool,
    hide_keys: bool,
    rows: Vec<Entry>,
    prompt: String,
    kind: Option<CommandPromptKind>,
    prompt_mode: CommandPromptMode,
    history: Vec<String>,
    suggestions: Vec<CompletionSuggestion>,
    input: String,
    cursor: usize,
    set_input: bool,
    selected: Option<usize>,
    engaged: bool,
    finishing: bool,
    window_chooser: Option<ChooseTreeState>,
    chooser_search: ChooserSearch,
    chooser_activation: Option<ChooseTreeTarget>,
    recent: Vec<String>,
    prompt_revision: u64,
    chooser_revision: u64,
    revision: u64,
    local_revisions: Option<(u64, u64)>,
    connected: bool,
    notices: Vec<String>,
}

fn byte_index_for_char(value: &str, cursor: u32) -> Option<usize> {
    let cursor = usize::try_from(cursor).ok()?;
    value
        .char_indices()
        .nth(cursor)
        .map(|(index, _)| index)
        .or_else(|| (cursor == value.chars().count()).then_some(value.len()))
}

fn byte_from_utf16(value: &str, units: usize) -> usize {
    let mut seen = 0;
    for (index, character) in value.char_indices() {
        if seen >= units {
            return index;
        }
        seen += character.len_utf16();
    }
    value.len()
}

fn utf16_from_byte(value: &str, byte: usize) -> usize {
    value[..byte.min(value.len())].encode_utf16().count()
}

fn shortcut_glyphs(shortcut: &str) -> Value {
    let keys: Option<Vec<Value>> = shortcut
        .split_whitespace()
        .map(|spelling| {
            tmux_keystroke_source(spelling).map(|(source, base)| {
                if base == "minus" {
                    glyphs(&source.replace("minus", "-"), true)
                } else {
                    glyphs(&source, true)
                }
            })
        })
        .collect();
    match keys {
        Some(keys) if !keys.is_empty() => json!(keys),
        _ => json!([[{ "text": shortcut }]]),
    }
}

fn hint(key: &str, label: &str) -> Value {
    json!({
        "keys": key
            .split([' ', '/'])
            .filter(|key| !key.is_empty())
            .map(|key| glyphs(key, false))
            .collect::<Vec<_>>(),
        "label": label,
    })
}

fn label_segments(row: &Row) -> Value {
    let mut segments: Vec<(String, &'static str)> = Vec::new();
    for (start, character) in row.label.char_indices() {
        let matched = row.matches.iter().any(|range| range.contains(&start));
        let class = if matched {
            "match"
        } else if start < row.muted_prefix {
            "muted"
        } else {
            ""
        };
        match segments.last_mut() {
            Some((text, last)) if *last == class => text.push(character),
            _ => segments.push((character.to_string(), class)),
        }
    }
    Value::Array(
        segments
            .into_iter()
            .map(|(text, class)| json!({ "text": text, "class": class }))
            .collect(),
    )
}

fn list_height(viewport: f32, usage: bool) -> f32 {
    (viewport * 0.88 - if usage { 175.0 } else { 140.0 }).clamp(40.0, 440.0)
}

const fn relays_keys(mode: CommandPromptMode) -> bool {
    matches!(
        mode,
        CommandPromptMode::Single | CommandPromptMode::Numeric | CommandPromptMode::Key
    )
}

impl Palette {
    const fn completes(&self) -> bool {
        matches!(
            self.prompt_mode,
            CommandPromptMode::Text | CommandPromptMode::BackspaceExit
        )
    }

    fn reset_unified(&mut self, mode: Option<Mode>) {
        self.unified = true;
        self.mode = mode;
        self.command = None;
        self.chooser_current_window = None;
        self.rows.clear();
    }

    fn selected_action(&self) -> Option<&Action> {
        self.rows.get(self.selected?)?.action.as_ref()
    }

    fn navigate_rows(&self, selected: Option<usize>, direction: isize) -> Option<usize> {
        let indices: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| row.action.as_ref().map(|_| index))
            .collect();
        if indices.is_empty() {
            return None;
        }
        let current = selected
            .and_then(|selected| indices.iter().position(|index| *index == selected))
            .unwrap_or_default();
        let next = if direction < 0 {
            current.checked_sub(1).unwrap_or(indices.len() - 1)
        } else {
            (current + 1) % indices.len()
        };
        Some(indices[next])
    }

    fn initial_selection(&self) -> Option<usize> {
        self.rows
            .iter()
            .position(|entry| {
                entry.row.right == "current"
                    && match entry.action {
                        Some(Action::Target {
                            target: Target::Window(_),
                            ..
                        }) => true,
                        Some(Action::ChooseWindow(index)) => {
                            self.chooser_current_window == Some(index)
                        }
                        _ => false,
                    }
            })
            .or_else(|| {
                self.rows
                    .iter()
                    .position(|entry| entry.action.is_some() && entry.row.right == "current")
            })
            .or_else(|| self.rows.iter().position(|entry| entry.action.is_some()))
    }

    fn parent_index(&self, index: usize) -> Option<usize> {
        let depth = self.rows.get(index)?.row.indent;
        self.rows[..index]
            .iter()
            .enumerate()
            .rev()
            .take_while(|(_, entry)| entry.action.is_some())
            .find_map(|(index, entry)| (entry.row.indent < depth).then_some(index))
    }

    fn child_index(&self, index: usize) -> Option<usize> {
        let entry = self.rows.get(index)?;
        (entry.row.expanded == Some(true)
            && self
                .rows
                .get(index + 1)
                .is_some_and(|child| child.action.is_some() && child.row.indent > entry.row.indent))
        .then_some(index + 1)
    }

    fn pills(&self) -> Vec<Value> {
        if self.is_places() {
            return self.place_pills();
        }
        let mut pills = Vec::new();
        if let Some(mode) = self.mode {
            pills.push(json!({ "kind": "mode", "prefix": mode.prefix(), "label": mode.label() }));
        }
        if let Some(command) = self.command {
            pills.push(json!({ "kind": "command", "label": command.name }));
        }
        pills
    }

    fn placeholder(&self) -> String {
        if self.window_chooser.is_some() {
            return "Search sessions, windows, panes".to_owned();
        }
        if !self.unified {
            return match self.kind {
                Some(CommandPromptKind::Value) => "Enter a value…",
                _ => "Type a zz command…",
            }
            .to_owned();
        }
        if self.is_places() {
            return self.place_placeholder();
        }
        if let Some(command) = self.command {
            return if target_kind(command) == Some(CommandValueKind::Window) {
                "Target window"
            } else {
                "Target pane"
            }
            .to_owned();
        }
        match self.mode {
            Some(Mode::Command) => "Type a command".to_owned(),
            Some(Mode::Target) | None => "Search sessions, windows, panes".to_owned(),
        }
    }

    fn usage(&self) -> Option<String> {
        if self.is_places() {
            return self.place_usage();
        }
        if self.mode != Some(Mode::Command) {
            return None;
        }
        self.command
            .or_else(|| match self.selected_action() {
                Some(Action::Command(spec)) => Some(*spec),
                _ => None,
            })
            .map(|spec| spec.usage)
            .or_else(|| {
                self.input
                    .split_whitespace()
                    .next()
                    .and_then(catalog_command_spec)
                    .map(|spec| spec.usage)
            })
            .map(str::to_owned)
    }

    fn rebuild_window_chooser(&mut self, state: &ChooseTreeState) {
        let query = self.input.trim().to_owned();
        let tree = !self.flat && query.is_empty();
        let mut session_name = String::new();
        let mut window_name = String::new();
        let mut session_active = false;
        let mut window_active = false;
        self.rows.clear();
        self.chooser_current_window = None;
        for (index, item) in state.items.iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            let name = if !item.text.is_empty() {
                item.text.as_str()
            } else if matches!(item.target, ChooseTreeTarget::Window(_))
                && let Some((prefix, name)) = item.label.split_once(':')
                && prefix.parse::<u32>().is_ok()
            {
                name
            } else {
                item.label.as_str()
            };
            let (path, active, icon) = match item.target {
                ChooseTreeTarget::Session(_) => {
                    name.clone_into(&mut session_name);
                    window_name.clear();
                    session_active = item.active();
                    window_active = false;
                    (String::new(), session_active, "folder")
                }
                ChooseTreeTarget::Window(_) => {
                    name.clone_into(&mut window_name);
                    window_active = session_active && item.active();
                    if window_active {
                        self.chooser_current_window = Some(index);
                    }
                    (session_name.clone(), window_active, "panels-top-left")
                }
                ChooseTreeTarget::Pane(_) => (
                    [session_name.as_str(), window_name.as_str()]
                        .into_iter()
                        .filter(|name| !name.is_empty())
                        .collect::<Vec<_>>()
                        .join(" / "),
                    window_active && item.active(),
                    match item.pane_kind {
                        Some(ChooseTreePaneKind::Terminal) => "square-terminal",
                        Some(ChooseTreePaneKind::Browser) => "globe",
                        Some(ChooseTreePaneKind::Editor) => "file",
                        None => "plus",
                    },
                ),
                ChooseTreeTarget::Client(_) => continue,
            };
            let prefix = if !tree && !path.is_empty() {
                format!("{path} / ")
            } else {
                String::new()
            };
            let metadata = format!("{path} {} {} {}", item.label, item.detail, item.target);
            let row = Row {
                label: format!("{prefix}{name}"),
                detail: if item.text.is_empty() {
                    item.detail.clone()
                } else {
                    String::new()
                },
                muted_prefix: prefix.len(),
                right: if active { "current" } else { "" }.to_owned(),
                icon: Some(icon),
                expanded: (tree && item.has_children()).then_some(item.expanded()),
                indent: if tree {
                    f32::from(item.depth) * 18.0
                } else {
                    0.0
                },
                ..Row::default()
            };
            if let Some(entry) =
                Entry::row(row, Action::ChooseWindow(index)).matching(&query, &metadata)
            {
                self.rows.push(entry);
            }
        }
        if !tree {
            self.rows.sort_by_key(|entry| entry.score);
        }
    }

    fn navigation_entries(&self, tree: &[TreeSession], query: &str) -> Vec<Entry> {
        let kind = self.command.and_then(target_kind);
        let pane_only = kind == Some(CommandValueKind::Pane);
        let window_only = kind == Some(CommandValueKind::Window);
        let mut rows = Vec::new();
        if !self.connected {
            return rows;
        }
        for session in tree {
            if !pane_only && !window_only {
                let entry = Entry::row(
                    Row {
                        label: session.name.clone(),
                        detail: count(session.windows.len(), "window"),
                        right: if session.active { "current" } else { "" }.to_owned(),
                        icon: Some("folder"),
                        running: session.windows.iter().any(TreeWindow::running),
                        ..Row::default()
                    },
                    Action::Target {
                        target: Target::Session(session.id),
                        label: format!("{} on {HOST}", session.name),
                    },
                );
                let metadata = format!("{} {}", session.id, entry.row.detail);
                rows.extend(entry.matching(query, &metadata));
            }
            for window in &session.windows {
                if !pane_only {
                    let prefix = format!("{} / ", session.name);
                    let label = format!("{prefix}{}", window.name);
                    let entry = Entry::row(
                        Row {
                            label: label.clone(),
                            muted_prefix: prefix.len(),
                            detail: count(window.panes.len(), "pane"),
                            right: if window.active { "current" } else { "" }.to_owned(),
                            icon: Some("panels-top-left"),
                            running: window.running(),
                            ..Row::default()
                        },
                        Action::Target {
                            target: Target::Window(window.id),
                            label,
                        },
                    );
                    let metadata = format!("{} {} {}", session.id, window.id, entry.row.detail);
                    rows.extend(entry.matching(query, &metadata));
                }
                if window_only {
                    continue;
                }
                for pane in &window.panes {
                    let prefix = format!("{} / {} / ", session.name, window.name);
                    let entry = Entry::row(
                        Row {
                            label: format!("{prefix}{}", pane.label),
                            muted_prefix: prefix.len(),
                            detail: pane.detail.to_owned(),
                            icon: Some(pane.icon),
                            right: if window.active && window.active_pane == pane.id {
                                "current"
                            } else {
                                ""
                            }
                            .to_owned(),
                            ..Row::default()
                        },
                        Action::Target {
                            target: Target::Pane(pane.id),
                            label: format!("{} / {} / {}", session.name, window.name, pane.label),
                        },
                    );
                    let metadata = format!(
                        "{} {} {} {}",
                        session.id, window.id, pane.id, entry.row.detail
                    );
                    rows.extend(entry.matching(query, &metadata));
                }
            }
        }
        rows.sort_by_key(|entry| entry.score);
        rows
    }

    fn command_entries(
        &self,
        query: &str,
        availability: PaneKindAvailability,
        shortcut: &dyn Fn(&str) -> Option<String>,
    ) -> Vec<Entry> {
        let mut entries: Vec<_> = command_specs()
            .filter(|spec| availability.allows_command(spec.name))
            .filter_map(|spec| {
                Entry::row(
                    Row {
                        label: spec.name.to_owned(),
                        detail: spec.description.to_owned(),
                        icon: Some("terminal"),
                        shortcut: (!self.hide_keys).then(|| shortcut(spec.name)).flatten(),
                        ..Row::default()
                    },
                    Action::Command(spec),
                )
                .matching(query, &spec.aliases.join(" "))
            })
            .collect();
        entries.sort_by_key(|entry| entry.score);
        entries
    }

    fn rebuild(
        &mut self,
        tree: &[TreeSession],
        history: &[String],
        availability: PaneKindAvailability,
        shortcut: &dyn Fn(&str) -> Option<String>,
    ) {
        self.chooser_current_window = None;
        let query = self.input.trim().to_owned();
        self.rows.clear();
        if self.command.is_some() {
            self.rows = self.navigation_entries(tree, &query);
        } else if self.mode == Some(Mode::Command) {
            self.rows = self.command_entries(&query, availability, shortcut);
        } else if self.mode.is_none() {
            self.rebuild_places(tree, history, availability, shortcut);
        }
    }
}

fn append_section(rows: &mut Vec<Entry>, label: &str, entries: Vec<Entry>) {
    if entries.is_empty() {
        return;
    }
    rows.push(Entry::header(label));
    rows.extend(entries);
}

#[wasm_bindgen]
impl Client {
    pub fn palette_prompt_changed(&mut self) {
        self.palette.prompt_revision = self.palette.prompt_revision.wrapping_add(1).max(1);
    }

    pub fn palette_chooser_changed(&mut self) {
        self.palette.chooser_revision = self.palette.chooser_revision.wrapping_add(1).max(1);
    }

    pub fn palette_settings(&mut self, grouped: bool, show_keys: bool) {
        self.palette.flat = !grouped;
        self.palette.hide_keys = !show_keys;
    }

    pub fn palette_open(&mut self, mode: Option<String>) -> bool {
        if self.core.menu().is_some() || self.core.confirm().is_some() {
            return false;
        }
        if self.core.command_prompt().is_some() {
            self.input(InputMessage::CommandPrompt {
                action: CommandPromptAction::Close,
            });
        }
        if self.core.choose_tree().is_some() {
            self.input(InputMessage::ChooseTree {
                action: ChooseTreeAction::Close,
            });
        }
        let field = if mode.as_deref() == Some(":") {
            ":"
        } else {
            ""
        };
        self.open_local(None, field);
        self.palette.local_revisions =
            Some((self.palette.prompt_revision, self.palette.chooser_revision));
        true
    }

    pub fn palette_is_open(&self) -> bool {
        self.palette.open
    }

    pub fn palette_view(&mut self, viewport: f32) -> Option<String> {
        self.palette_sync();
        if !self.palette.open {
            return None;
        }
        Some(self.palette_json(viewport).to_string())
    }

    pub fn palette_input(&mut self, value: String, cursor: usize) {
        let cursor = byte_from_utf16(&value, cursor);
        if value == self.palette.input && cursor == self.palette.cursor {
            return;
        }
        if value.len() > MAX_COMMAND_PROMPT_BYTES {
            self.palette.set_input = true;
            return;
        }
        if self.palette.input.trim().is_empty() && !value.trim().is_empty() {
            self.palette.chooser_search.selection = self.selected_chooser_target();
        }
        if self.commit_typed(&value) {
            return;
        }
        self.palette.input.clone_from(&value);
        self.palette.cursor = cursor;
        self.palette.engaged = false;
        self.palette.selected = None;
        self.recompute_suggestions();
        self.palette_refresh();
        self.advance_chooser_search();
        if !self.palette.finishing && !self.palette.local && self.palette.window_chooser.is_none() {
            let cursor = u32::try_from(value[..cursor].chars().count()).unwrap_or(u32::MAX);
            self.input(InputMessage::CommandPrompt {
                action: CommandPromptAction::Update {
                    input: value,
                    cursor,
                },
            });
        }
    }

    pub fn palette_key(&mut self, key: &str) -> bool {
        if !self.palette.open {
            return false;
        }
        match key {
            "ArrowUp" => self.palette_navigate(-1),
            "ArrowDown" => self.palette_navigate(1),
            "Escape" => self.palette_close(),
            "ShiftTab" if self.palette.is_places() => {
                self.step_back();
            }
            "Tab" | "ShiftTab" => {
                if self.palette.unified {
                    self.activate_unified(true);
                } else {
                    self.accept_selected();
                }
            }
            "Enter" => self.palette_enter(),
            "Backspace" => return self.leave_layer(),
            "ArrowRight" if self.palette.is_places() => return self.place_into(),
            "ArrowLeft" | "ArrowRight" => return self.move_in_tree(key == "ArrowRight"),
            _ => return false,
        }
        true
    }

    pub fn palette_raw_key(
        &mut self,
        key: &str,
        text: Option<String>,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) -> bool {
        if !self.palette.open || self.palette.finishing || self.palette.unified {
            return false;
        }
        if self.palette.prompt_mode == CommandPromptMode::BackspaceExit {
            if key == "Backspace" && self.palette.input.is_empty() {
                self.palette_close();
                return true;
            }
            return false;
        }
        if !relays_keys(self.palette.prompt_mode) {
            return false;
        }
        if let Some(pane) = self.active_pane() {
            let input = crate::overlays::key_input(
                key,
                text,
                (shift, control, alt, meta),
                KeyAction::Press,
            );
            self.input(InputMessage::Key {
                pane,
                input,
                text_follows: false,
            });
        }
        true
    }

    pub fn palette_hover(&mut self, index: usize) -> bool {
        if self.palette.unified {
            if self
                .palette
                .rows
                .get(index)
                .is_some_and(|row| row.action.is_some())
                && self.palette.selected != Some(index)
            {
                self.palette.selected = Some(index);
                self.palette.engaged = true;
                return true;
            }
            return false;
        }
        if index < self.palette.suggestions.len()
            && (self.palette.selected != Some(index) || !self.palette.engaged)
        {
            self.palette.selected = Some(index);
            self.palette.engaged = true;
            return true;
        }
        false
    }

    pub fn palette_click(&mut self, index: usize, clicks: u32) {
        if self.palette.unified {
            self.palette.selected = Some(index);
            self.palette.engaged = true;
            self.activate_unified(false);
            return;
        }
        let history = self
            .palette
            .suggestions
            .get(index)
            .is_some_and(|suggestion| suggestion.kind == CompletionKind::History);
        self.accept(index);
        if history && clicks >= 2 {
            self.palette_enter();
        }
    }

    pub fn palette_toggle(&mut self, index: usize) {
        self.palette.selected = Some(index);
        let expanded = self
            .palette
            .rows
            .get(index)
            .and_then(|entry| entry.row.expanded)
            .unwrap_or(false);
        self.change_tree_expansion(!expanded);
    }

    pub fn palette_dismiss(&mut self) {
        self.palette_close();
    }

    pub fn chrome_action(
        &self,
        key: &str,
        text: Option<String>,
        shift: bool,
        control: bool,
        alt: bool,
        meta: bool,
    ) -> Option<String> {
        let input =
            crate::overlays::key_input(key, text, (shift, control, alt, meta), KeyAction::Press);
        ChromeKeymap::for_profile(ChromeProfile::Desktop)
            .resolve(UI_TABLE, &input)
            .or_else(|| {
                ChromeKeymap::for_profile(ChromeProfile::DesktopApple).resolve(UI_TABLE, &input)
            })
            .map(|action| action.name().to_owned())
    }

    pub fn palette_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.palette.notices)
    }
}

impl Client {
    fn active_pane(&self) -> Option<PaneId> {
        let snapshot = self.core.snapshot();
        let session = snapshot
            .sessions
            .iter()
            .find(|session| Some(session.id) == self.core.attached_session())?;
        let focused = snapshot.focused_window_for(session);
        session
            .windows
            .iter()
            .find(|window| window.id == focused)
            .map(|window| window.active_pane)
    }

    fn availability() -> PaneKindAvailability {
        PaneKindAvailability {
            browser: false,
            editor: false,
        }
    }

    fn command_shortcut(&self, command: &str) -> Option<String> {
        self.core
            .prefix_bindings()
            .iter()
            .find(|binding| binding.commands.len() == 1 && binding.commands[0].name == command)
            .map(|binding| {
                format!(
                    "{} {}",
                    self.core
                        .mux_options()
                        .get(MuxOptionKey::Prefix)
                        .map_or_else(
                            || "prefix".to_owned(),
                            |option| zz_protocol::canonical_key(&option.value)
                        ),
                    binding.key
                )
            })
    }

    fn palette_tree(&self) -> Vec<TreeSession> {
        let snapshot = self.core.snapshot();
        snapshot
            .sessions
            .iter()
            .map(|session| {
                let active = self.core.attached_session() == Some(session.id);
                let focused = snapshot.focused_window_for(session);
                TreeSession {
                    id: session.id,
                    name: session_label(session).to_string(),
                    tmux_name: session.name.clone(),
                    active,
                    windows: session
                        .windows
                        .iter()
                        .map(|window| TreeWindow {
                            id: window.id,
                            index: window.index,
                            name: window.name.clone(),
                            active: active && window.id == focused,
                            active_pane: window.active_pane,
                            panes: ordered_panes(window)
                                .into_iter()
                                .map(|pane| {
                                    let attention = matches!(pane.kind, PaneKindSnapshot::Terminal)
                                        .then(|| {
                                            self.core
                                                .agent_state(pane.id)
                                                .map(agent_attention_status)
                                        })
                                        .flatten();
                                    TreePane {
                                        id: pane.id,
                                        index: pane_index(window, pane.id),
                                        label: pane_label(pane),
                                        program: zz_client::window_summary::pane_program(pane)
                                            .map(|program| program.name),
                                        detail: match &pane.kind {
                                            PaneKindSnapshot::Terminal => match attention {
                                                Some(AgentAttentionStatus::Working) => {
                                                    "Agent · running"
                                                }
                                                Some(AgentAttentionStatus::NeedsInput) => {
                                                    "Agent · waiting for input"
                                                }
                                                Some(AgentAttentionStatus::Idle) => "Agent · idle",
                                                Some(AgentAttentionStatus::Failed) => {
                                                    "Agent · failed"
                                                }
                                                None => "Terminal",
                                            },
                                            PaneKindSnapshot::Browser(_) => "Browser · unavailable",
                                            PaneKindSnapshot::Editor(_) => "Editor · unavailable",
                                            PaneKindSnapshot::Picker => "New pane",
                                        },
                                        icon: match &pane.kind {
                                            PaneKindSnapshot::Picker => "plus",
                                            PaneKindSnapshot::Terminal => "square-terminal",
                                            PaneKindSnapshot::Browser(_) => "globe",
                                            PaneKindSnapshot::Editor(_) => "file",
                                        },
                                        running: attention == Some(AgentAttentionStatus::Working),
                                        needs_you: pane.status.as_ref().is_some_and(|status| {
                                            matches!(
                                                status.state,
                                                PaneStatusState::Blocked | PaneStatusState::Error
                                            )
                                        }),
                                        terminal: matches!(pane.kind, PaneKindSnapshot::Terminal),
                                        cwd: pane.summary.cwd.clone(),
                                    }
                                })
                                .collect(),
                        })
                        .collect(),
                }
            })
            .collect()
    }

    fn open_local(&mut self, mode: Option<Mode>, field: &str) {
        let palette = &mut self.palette;
        palette.open = true;
        palette.local = true;
        ":".clone_into(&mut palette.prompt);
        palette.kind = Some(CommandPromptKind::Command);
        palette.prompt_mode = CommandPromptMode::Text;
        palette.history.clear();
        palette.window_chooser = None;
        palette.chooser_search = ChooserSearch::default();
        palette.chooser_activation = None;
        palette.finishing = false;
        palette.engaged = false;
        palette.selected = None;
        palette.input.clear();
        palette.cursor = 0;
        palette.set_input = true;
        palette.reset_unified(mode);
        self.start_places(field);
    }

    fn palette_sync(&mut self) {
        self.palette.connected = self.core.attached_session().is_some();
        let prompt = self.core.command_prompt().cloned();
        let window_chooser = self
            .core
            .choose_tree()
            .filter(|state| state.kind == zz_protocol::ChooseTreeKind::Windows)
            .cloned();
        if self.palette.open {
            let gone = if self.palette.local {
                self.palette.finishing
            } else if self.palette.window_chooser.is_some() {
                window_chooser.is_none()
            } else {
                prompt.is_none()
            };
            if gone {
                self.palette.open = false;
                self.palette.window_chooser = None;
            }
        }
        let reopened =
            self.palette
                .local_revisions
                .is_none_or(|(prompt_revision, chooser_revision)| {
                    (prompt.is_some() && prompt_revision != self.palette.prompt_revision)
                        || (window_chooser.is_some()
                            && chooser_revision != self.palette.chooser_revision)
                });
        if !reopened && self.palette.open && self.palette.local {
            self.palette_refresh();
            return;
        }
        if let Some(state) = window_chooser {
            if !self.palette.open || self.palette.window_chooser.is_none() {
                self.open_local(Some(Mode::Target), "");
                self.palette.local = false;
                self.palette.window_chooser = None;
            }
            self.synchronize_window_chooser(state);
            return;
        }
        let Some(state) = prompt else {
            if self.palette.open && !self.palette.local {
                self.palette.open = false;
            }
            return;
        };
        if self.palette.open && (self.palette.local || self.palette.window_chooser.is_some()) {
            self.palette.open = false;
        }
        if !self.palette.open || self.palette.revision != self.palette.prompt_revision {
            self.synchronize_prompt(&state);
        } else {
            self.recompute_suggestions_if_snapshot();
            self.palette_refresh();
        }
    }

    fn recompute_suggestions_if_snapshot(&mut self) {
        if !self.palette.unified {
            let selected = self.palette.selected;
            self.recompute_suggestions();
            if self.palette.engaged {
                self.palette.selected =
                    selected.filter(|index| *index < self.palette.suggestions.len());
            }
        }
    }

    fn synchronize_prompt(&mut self, state: &zz_protocol::CommandPromptState) {
        let palette = &mut self.palette;
        palette.open = true;
        palette.revision = palette.prompt_revision;
        palette.prompt.clone_from(&state.prompt);
        palette.kind = Some(state.kind);
        palette.prompt_mode = state.mode;
        palette.local = false;
        palette.window_chooser = None;
        let unified = state.kind == CommandPromptKind::Command
            && state.mode == CommandPromptMode::Text
            && state.prompt.trim() == ":";
        if unified {
            palette.reset_unified(Some(Mode::Command));
        } else {
            palette.unified = false;
            palette.mode = None;
            palette.command = None;
            palette.rows.clear();
        }
        palette.history.clone_from(&state.history);
        palette.finishing = false;
        palette.engaged = false;
        palette.selected = None;
        let cursor = byte_index_for_char(&state.input, state.cursor).unwrap_or(state.input.len());
        palette.input.clone_from(&state.input);
        palette.cursor = cursor;
        palette.set_input = true;
        self.recompute_suggestions();
        self.palette_refresh();
    }

    fn synchronize_window_chooser(&mut self, state: ChooseTreeState) {
        if self.palette.window_chooser.is_some()
            && self.palette.revision == self.palette.chooser_revision
        {
            return;
        }
        let target = self.selected_chooser_target().or_else(|| {
            state
                .items
                .get(state.selected as usize)
                .map(|item| item.target)
        });
        self.palette.window_chooser = Some(state);
        self.palette.revision = self.palette.chooser_revision;
        self.palette.finishing = false;
        self.palette_refresh();
        if let Some(target) = target {
            self.palette.selected = self
                .row_for_chooser_target(target)
                .or(self.palette.selected);
        }
        self.advance_chooser_search();
    }

    fn selected_chooser_target(&self) -> Option<ChooseTreeTarget> {
        let state = self.palette.window_chooser.as_ref()?;
        let entry = self.palette.rows.get(self.palette.selected?)?;
        let Some(Action::ChooseWindow(index)) = entry.action else {
            return None;
        };
        state.items.get(index as usize).map(|item| item.target)
    }

    fn row_for_chooser_target(&self, target: ChooseTreeTarget) -> Option<usize> {
        let state = self.palette.window_chooser.as_ref()?;
        self.palette.rows.iter().position(|entry| {
            matches!(entry.action, Some(Action::ChooseWindow(index))
                if state.items.get(index as usize).is_some_and(|item| item.target == target))
        })
    }

    fn send_tree(&mut self, action: ChooseTreeAction) {
        self.input(InputMessage::ChooseTree { action });
    }

    fn advance_chooser_search(&mut self) {
        let Some(state) = self.palette.window_chooser.clone() else {
            return;
        };
        if self.palette.finishing {
            return;
        }
        let searching = !self.palette.input.trim().is_empty();
        if searching && self.palette.chooser_search.selection.is_none() {
            self.palette.chooser_search.selection = state
                .items
                .get(state.selected as usize)
                .map(|item| item.target);
        }
        if self.palette.chooser_activation.is_some() {
            if let Some((target, expanded)) = self.palette.chooser_search.pending
                && state.items.iter().any(|item| {
                    item.target == target
                        && item.expanded() != expanded
                        && (!expanded || item.has_children())
                })
            {
                return;
            }
            if let Some(target) = self.palette.chooser_activation.take()
                && let Some(index) = state.items.iter().position(|item| item.target == target)
            {
                self.palette.finishing = true;
                self.send_tree(ChooseTreeAction::ActivateIndex(index as u32));
                return;
            }
        }
        if let Some((index, expand)) = self.palette.chooser_search.advance(&state, searching) {
            self.send_tree(ChooseTreeAction::Select(index));
            self.send_tree(if expand {
                ChooseTreeAction::Expand
            } else {
                ChooseTreeAction::Collapse
            });
        } else if !searching
            && self.palette.chooser_search.pending.is_none()
            && let Some(target) = self.palette.chooser_search.selection.take()
        {
            self.palette.selected = self
                .row_for_chooser_target(target)
                .or(self.palette.selected);
        }
    }

    fn recompute_suggestions(&mut self) {
        let palette = &self.palette;
        let suggestions = if palette.kind == Some(CommandPromptKind::Command)
            && palette.completes()
            && !palette.unified
        {
            complete_command(
                &palette.input,
                palette.cursor,
                &palette.history,
                self.core.snapshot(),
                Self::availability(),
            )
        } else {
            Vec::new()
        };
        let palette = &mut self.palette;
        palette.suggestions = suggestions;
        if !palette.unified {
            palette.selected = (!palette.suggestions.is_empty()).then_some(
                palette
                    .selected
                    .unwrap_or_default()
                    .min(palette.suggestions.len().saturating_sub(1)),
            );
        }
    }

    fn palette_refresh(&mut self) {
        if !self.palette.unified {
            return;
        }
        let anchor = self.palette.selected_action().and_then(Action::anchor);
        let mut history = self.palette.recent.clone();
        for entry in &self.palette.history {
            if !history.contains(entry) {
                history.push(entry.clone());
            }
        }
        if let Some(state) = self.palette.window_chooser.clone() {
            self.palette.rebuild_window_chooser(&state);
        } else {
            let tree = self.palette_tree();
            let shortcuts: BTreeMap<&'static str, String> = command_specs()
                .filter_map(|spec| self.command_shortcut(spec.name).map(|key| (spec.name, key)))
                .collect();
            self.palette
                .rebuild(&tree, &history, Self::availability(), &|name| {
                    shortcuts.get(name).cloned()
                });
        }
        let palette = &mut self.palette;
        let places = palette.is_places();
        let command_input = if places {
            palette.raw_command().map(|rest| {
                let start = palette.input.len() - palette.input.trim_start().len() + 1;
                (rest, palette.cursor.saturating_sub(start))
            })
        } else {
            (palette.mode == Some(Mode::Command) && palette.command.is_none())
                .then(|| (palette.input.clone(), palette.cursor))
        };
        if let Some((input, cursor)) =
            command_input.filter(|(input, _)| has_command_arguments(input))
        {
            palette.rows = complete_command(
                &input,
                cursor,
                &history,
                self.core.snapshot(),
                Self::availability(),
            )
            .into_iter()
            .map(|suggestion| {
                let target = suggestion.kind == CompletionKind::Value
                    && suggestion.label.starts_with(['$', '@', '%']);
                Entry {
                    row: Row {
                        label: if target {
                            suggestion.detail.clone()
                        } else {
                            suggestion.label.clone()
                        },
                        detail: if target {
                            String::new()
                        } else {
                            suggestion.detail.clone()
                        },
                        ..Row::default()
                    },
                    action: Some(Action::Completion(suggestion)),
                    score: 0,
                }
            })
            .collect();
        }
        if let Some(anchor) = anchor {
            palette.selected = palette
                .rows
                .iter()
                .position(|entry| entry.action.as_ref().and_then(Action::anchor) == Some(anchor));
        }
        palette.selected = palette
            .selected
            .filter(|index| {
                palette
                    .rows
                    .get(*index)
                    .is_some_and(|row| row.action.is_some())
            })
            .or_else(|| {
                if palette.input.trim().is_empty() && !places {
                    palette.initial_selection()
                } else {
                    palette.rows.iter().position(|row| row.action.is_some())
                }
            });
    }

    fn set_query(&mut self, value: String) {
        self.palette.cursor = value.len();
        self.palette.input = value;
        self.palette.set_input = true;
        self.palette.selected = None;
        self.palette.engaged = false;
        self.recompute_suggestions();
        self.palette_refresh();
    }

    fn palette_navigate(&mut self, direction: isize) {
        if self.palette.unified {
            if let Some(next) = self.palette.navigate_rows(self.palette.selected, direction) {
                self.palette.selected = Some(next);
                self.palette.engaged = true;
            }
            return;
        }
        let count = self.palette.suggestions.len();
        if count == 0 {
            return;
        }
        let selected = if self.palette.engaged {
            let current = self.palette.selected.unwrap_or_default();
            if direction < 0 {
                current.checked_sub(1).unwrap_or(count - 1)
            } else {
                (current + 1) % count
            }
        } else if direction < 0 {
            count - 1
        } else {
            0
        };
        self.palette.engaged = true;
        self.palette.selected = Some(selected);
    }

    fn accept(&mut self, index: usize) {
        let Some(suggestion) = self.palette.suggestions.get(index).cloned() else {
            return;
        };
        let (completed, cursor) = apply_completion(&self.palette.input, &suggestion);
        if completed.len() > MAX_COMMAND_PROMPT_BYTES {
            return;
        }
        self.palette.engaged = false;
        self.palette_input(completed.clone(), utf16_from_byte(&completed, cursor));
        self.palette.set_input = true;
    }

    fn accept_selected(&mut self) {
        if let Some(index) = self
            .palette
            .selected
            .or((!self.palette.suggestions.is_empty()).then_some(0))
        {
            self.accept(index);
        }
    }

    fn palette_enter(&mut self) {
        if self.palette.finishing {
            return;
        }
        if self.palette.unified {
            self.activate_unified(false);
            return;
        }
        if self.palette.engaged && !self.palette.suggestions.is_empty() {
            self.accept_selected();
            return;
        }
        self.palette.finishing = true;
        let input = self.palette.input.clone();
        self.input(InputMessage::CommandPrompt {
            action: CommandPromptAction::Submit { input },
        });
    }

    fn palette_close(&mut self) {
        if self.palette.finishing {
            return;
        }
        self.palette.finishing = true;
        if self.palette.window_chooser.is_some() {
            self.send_tree(ChooseTreeAction::Close);
        } else if !self.palette.local {
            self.input(InputMessage::CommandPrompt {
                action: CommandPromptAction::Close,
            });
        }
        if self.palette.local {
            self.palette.open = false;
        }
    }

    fn return_to_default(&mut self) {
        if self.palette.finishing {
            return;
        }
        if self.palette.window_chooser.take().is_some() {
            self.send_tree(ChooseTreeAction::Close);
        } else if !self.palette.local {
            self.input(InputMessage::CommandPrompt {
                action: CommandPromptAction::Close,
            });
        } else {
            return;
        }
        self.palette.local = true;
        self.palette.chooser_search = ChooserSearch::default();
        self.palette.chooser_activation = None;
        self.palette.reset_unified(None);
        self.palette.local_revisions =
            Some((self.palette.prompt_revision, self.palette.chooser_revision));
        self.start_places("");
    }

    fn leave_layer(&mut self) -> bool {
        if !self.palette.input.is_empty() {
            return false;
        }
        if self.palette.is_places() {
            return self.step_back();
        }
        if self.palette.unified && self.palette.command.take().is_some() {
            self.palette.selected = None;
            self.palette.engaged = false;
            self.palette_refresh();
            return true;
        }
        if self.palette.unified {
            self.return_to_default();
            return true;
        }
        false
    }

    fn move_in_tree(&mut self, expand: bool) -> bool {
        if !self.palette.input.is_empty() || self.palette.window_chooser.is_none() {
            return false;
        }
        let Some(index) = self.palette.selected else {
            return false;
        };
        if self
            .palette
            .rows
            .get(index)
            .is_some_and(|entry| entry.row.expanded == Some(!expand))
        {
            self.change_tree_expansion(expand);
        } else if let Some(next) = if expand {
            self.palette.child_index(index)
        } else {
            self.palette.parent_index(index)
        } {
            self.palette.selected = Some(next);
            self.palette.engaged = true;
        }
        true
    }

    fn change_tree_expansion(&mut self, expand: bool) {
        if self.palette.chooser_search.pending.is_some() || !self.palette.input.is_empty() {
            return;
        }
        let Some(index) = self.palette.selected else {
            return;
        };
        self.palette.engaged = true;
        if let Some(Action::ChooseWindow(source)) = self
            .palette
            .rows
            .get(index)
            .and_then(|entry| entry.action.clone())
        {
            self.palette.chooser_search.pending = self
                .palette
                .window_chooser
                .as_ref()
                .and_then(|state| state.items.get(source as usize))
                .map(|item| (item.target, expand));
            self.send_tree(ChooseTreeAction::Select(source));
            self.send_tree(if expand {
                ChooseTreeAction::Expand
            } else {
                ChooseTreeAction::Collapse
            });
        }
    }

    fn activate_unified(&mut self, complete: bool) {
        if self.palette.is_places() {
            self.activate_places(complete);
            return;
        }
        let raw_command = self.palette.mode == Some(Mode::Command)
            && self.palette.command.is_none()
            && has_command_arguments(&self.palette.input);
        if raw_command && !complete && !self.palette.engaged {
            self.run_command(self.palette.input.clone(), None);
            return;
        }
        let action = self
            .palette
            .selected
            .and_then(|index| self.palette.rows.get(index))
            .and_then(|row| row.action.clone());
        let Some(action) = action else {
            if self.palette.mode == Some(Mode::Command)
                && !self.palette.input.trim().is_empty()
                && self.palette.command.is_none()
            {
                self.run_command(self.palette.input.clone(), None);
            }
            return;
        };
        match action {
            Action::ChooseWindow(index) => {
                if self.palette.chooser_search.pending.is_some() {
                    self.palette.chooser_activation = self
                        .palette
                        .window_chooser
                        .as_ref()
                        .and_then(|state| state.items.get(index as usize))
                        .map(|item| item.target);
                    return;
                }
                self.palette.finishing = true;
                self.send_tree(ChooseTreeAction::ActivateIndex(index));
            }
            Action::Command(spec) => {
                if complete {
                    self.set_query(format!("{} ", spec.name));
                } else if target_kind(spec).is_some() {
                    self.palette.mode = Some(Mode::Command);
                    self.palette.command = Some(spec);
                    self.set_query(String::new());
                } else if needs_arguments(spec) {
                    self.palette.mode = Some(Mode::Command);
                    self.set_query(format!("{} ", spec.name));
                } else {
                    self.run_command(
                        spec.name.to_owned(),
                        Some(CommandInvocation::new(spec.name, [] as [&str; 0])),
                    );
                }
            }
            Action::History(command) => self.run_command(command, None),
            Action::Completion(suggestion) => {
                let (completed, _) = apply_completion(&self.palette.input, &suggestion);
                if completed.len() <= MAX_COMMAND_PROMPT_BYTES {
                    self.set_query(completed);
                }
            }
            Action::Target { target, label } => {
                if let Some(command) = self.palette.command {
                    let spelled = target.spelled();
                    self.run_command(
                        format!("{} -t {spelled}", command.name),
                        Some(CommandInvocation::new(command.name, ["-t", &spelled])),
                    );
                } else if self.activate_target(target) {
                    self.palette_close();
                    self.palette.notices.push(format!("Switched to {label}"));
                }
            }
            Action::Place(_) | Action::Verb(_) | Action::Fill(_) | Action::Submit => {}
        }
    }

    fn activate_target(&mut self, target: Target) -> bool {
        if self.core.attached_session().is_none() {
            return false;
        }
        let Some((session, window, pane)) =
            self.core
                .snapshot()
                .sessions
                .iter()
                .find_map(|session| match target {
                    Target::Session(id) => (id == session.id).then_some((id, None, None)),
                    Target::Window(id) => session
                        .windows
                        .iter()
                        .find(|window| window.id == id)
                        .map(|window| (session.id, Some(window.id), None)),
                    Target::Pane(id) => session
                        .windows
                        .iter()
                        .find(|window| window.panes.contains_key(&id))
                        .map(|window| (session.id, Some(window.id), Some(id))),
                })
        else {
            return false;
        };
        if self.core.attached_session() != Some(session) {
            self.attach(&session.to_string());
        }
        if let Some(window) = window {
            self.command("select-window", vec!["-t".into(), window.to_string()]);
        }
        if let Some(pane) = pane {
            self.command(
                "select-pane",
                vec!["-Z".into(), "-t".into(), pane.to_string()],
            );
        }
        true
    }

    fn run_command(&mut self, input: String, command: Option<CommandInvocation>) {
        if self.core.attached_session().is_none() {
            self.palette
                .notices
                .push("Connect to this host before running a command".to_owned());
            return;
        }
        self.palette.recent.retain(|entry| entry != &input);
        self.palette.recent.insert(0, input.clone());
        self.palette.recent.truncate(20);
        let display = self
            .palette
            .command
            .and_then(|command| {
                self.palette
                    .selected
                    .and_then(|index| self.palette.rows.get(index))
                    .and_then(|row| match &row.action {
                        Some(Action::Target { label, .. }) => {
                            Some(format!("{} · {label}", command.name))
                        }
                        _ => None,
                    })
            })
            .unwrap_or_else(|| input.split_whitespace().next().unwrap_or(&input).to_owned());
        self.palette.finishing = true;
        if !self.palette.local && self.palette.window_chooser.is_none() {
            self.input(InputMessage::CommandPrompt {
                action: CommandPromptAction::Submit { input },
            });
        } else {
            if self.palette.window_chooser.is_some() {
                self.send_tree(ChooseTreeAction::Close);
            }
            self.invoke(
                command.unwrap_or_else(|| CommandInvocation::new("if-shell", ["-F", "1", &input])),
            );
        }
        if self.palette.local {
            self.palette.open = false;
        }
        self.palette.notices.push(format!("Ran {display}"));
    }

    fn enter_hint(&self) -> &'static str {
        self.palette
            .selected_action()
            .map_or("choose", |action| match action {
                Action::Target { target, .. } => match target {
                    Target::Session(_) => "switch session",
                    Target::Window(_) => "switch window",
                    Target::Pane(_) => "focus pane",
                },
                Action::ChooseWindow(index) => self
                    .palette
                    .window_chooser
                    .as_ref()
                    .and_then(|state| state.items.get(*index as usize))
                    .map_or("switch", |item| match item.target {
                        ChooseTreeTarget::Session(_) => "switch session",
                        ChooseTreeTarget::Window(_) => "switch window",
                        ChooseTreeTarget::Pane(_) => "focus pane",
                        ChooseTreeTarget::Client(_) => "choose",
                    }),
                Action::Command(_) | Action::History(_) => "run command",
                Action::Completion(_) => "complete",
                Action::Place(_) => "go",
                Action::Verb(_) => "choose",
                Action::Fill(_) => "pick",
                Action::Submit => "run",
            })
    }

    fn unified_hints(&self) -> Vec<Value> {
        let palette = &self.palette;
        if palette.is_places() {
            return self.place_hints();
        }
        let enter = self.enter_hint();
        let mut hints: Vec<(&str, &str)> = if palette.command.is_some() {
            vec![
                ("up down", "navigate"),
                ("enter", "run"),
                ("backspace", "back"),
            ]
        } else {
            match palette.mode {
                Some(Mode::Command) => vec![
                    ("tab", "complete"),
                    ("enter", "run"),
                    ("backspace", "leave mode"),
                    ("escape", "close"),
                ],
                Some(Mode::Target) | None => vec![
                    ("up down", "navigate"),
                    ("left right", "expand"),
                    ("enter", enter),
                    ("backspace", "leave mode"),
                    ("escape", "close"),
                ],
            }
        };
        let keeps_expand = palette.input.is_empty()
            && palette.selected.is_some_and(|index| {
                palette
                    .rows
                    .get(index)
                    .is_some_and(|entry| entry.row.expanded.is_some())
                    || palette.parent_index(index).is_some()
            });
        if !keeps_expand {
            hints.retain(|(key, _)| *key != "left right");
        }
        hints
            .into_iter()
            .map(|(key, label)| hint(key, label))
            .collect()
    }

    fn prompt_hints(&self) -> Vec<Value> {
        let palette = &self.palette;
        if relays_keys(palette.prompt_mode) {
            let (key, label) = match palette.prompt_mode {
                CommandPromptMode::Numeric => ("digits", "collect"),
                CommandPromptMode::Key => ("any key", "name it"),
                _ => ("any key", "submit"),
            };
            return vec![hint(key, label)];
        }
        let mut hints = Vec::new();
        if palette.kind == Some(CommandPromptKind::Command) && palette.completes() {
            hints.push(hint("tab", "complete"));
        }
        let enter = if palette.engaged {
            "accept"
        } else if palette.kind == Some(CommandPromptKind::Command) {
            "run"
        } else {
            "apply"
        };
        hints.push(hint("enter", enter));
        hints.push(hint("escape", "close"));
        hints
    }

    fn palette_json(&mut self, viewport: f32) -> Value {
        let set_input = std::mem::take(&mut self.palette.set_input);
        let palette = &self.palette;
        let input = json!({
            "value": palette.input,
            "cursor": utf16_from_byte(&palette.input, palette.cursor),
            "set": set_input,
            "placeholder": palette.placeholder(),
        });
        if !palette.unified {
            let rows: Vec<Value> = palette
                .suggestions
                .iter()
                .map(|suggestion| {
                    json!({
                        "label": [{ "text": suggestion.label, "class": "" }],
                        "detail": suggestion.detail,
                        "badge": match suggestion.kind {
                            CompletionKind::History => Some("HISTORY"),
                            CompletionKind::Command => None,
                            CompletionKind::Option => Some("OPTION"),
                            CompletionKind::Value => Some("VALUE"),
                        },
                    })
                })
                .collect();
            let visible = palette.suggestions.len().min(MAX_VISIBLE_ROWS) as f32;
            return json!({
                "kind": "prompt",
                "prompt": palette.prompt,
                "input": input,
                "relay": relays_keys(palette.prompt_mode),
                "rows": rows,
                "selected": if palette.engaged { palette.selected } else { None },
                "listHeight": visible * ROW_HEIGHT,
                "hints": self.prompt_hints(),
            });
        }
        let usage = palette.usage();
        let rows: Vec<Value> = palette
            .rows
            .iter()
            .map(|entry| {
                if entry.action.is_none() {
                    return json!({ "header": entry.row.label });
                }
                let row = &entry.row;
                json!({
                    "label": label_segments(row),
                    "text": row.label,
                    "detail": row.detail,
                    "right": row.right,
                    "icon": row.icon,
                    "expanded": row.expanded,
                    "indent": row.indent,
                    "running": row.running,
                    "status": row.status,
                    "badge": row.badge,
                    "shortcut": row.shortcut.as_deref().map(shortcut_glyphs),
                })
            })
            .collect();
        let height = (rows.len() as f32 * ROW_HEIGHT).min(list_height(viewport, usage.is_some()));
        let empty = if palette.is_places() {
            palette.place_empty()
        } else {
            Some("No matches")
        };
        json!({
            "kind": "unified",
            "input": input,
            "pills": palette.pills(),
            "usage": usage,
            "rows": rows,
            "selected": palette.selected,
            "empty": empty,
            "listHeight": height,
            "top": viewport * 0.12,
            "hints": self.unified_hints(),
        })
    }
}
