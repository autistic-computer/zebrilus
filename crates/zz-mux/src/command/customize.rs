use std::fmt::Write as _;

use super::mode_prompt::{ModeKey, ModeMouseKey, ModePrompt, PromptHistories, PromptOutcome};
use super::*;
use crate::tmux_option_metadata::TmuxOptionKind;
use zz_protocol::{
    ChooseTreeItem, ChooseTreeState, ChooseTreeTarget, ChooserPresentation, ChooserPreview,
    ChooserPreviewSize, ChooserRow, CommandPromptType,
};

mod draw;
mod rows;

const CUSTOMIZE_COLOUR_FLAG_OPTIONS: &[&str] = &[
    "clock-mode-colour",
    "cursor-colour",
    "dark-theme-black",
    "dark-theme-blue",
    "dark-theme-cyan",
    "dark-theme-dark-grey",
    "dark-theme-green",
    "dark-theme-light-grey",
    "dark-theme-magenta",
    "dark-theme-red",
    "dark-theme-white",
    "dark-theme-yellow",
    "display-panes-active-colour",
    "display-panes-colour",
    "light-theme-black",
    "light-theme-blue",
    "light-theme-cyan",
    "light-theme-dark-grey",
    "light-theme-green",
    "light-theme-light-grey",
    "light-theme-magenta",
    "light-theme-red",
    "light-theme-white",
    "light-theme-yellow",
    "prompt-command-cursor-colour",
    "prompt-cursor-colour",
];

pub type CustomizeExpand<'a> = dyn FnMut(&str, &BTreeMap<String, String>) -> String + 'a;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Preview {
    #[default]
    Normal,
    Off,
    Big,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PromptPurpose {
    Search,
    Filter,
    Option {
        name: String,
        array_key: Option<String>,
        target: TmuxOptionTarget,
    },
    ArrayKey {
        name: String,
        array_key: String,
        target: TmuxOptionTarget,
    },
    Command {
        table: String,
        key: String,
    },
    Note {
        table: String,
        key: String,
    },
    Environment {
        name: String,
        session: Option<SessionId>,
        hidden: bool,
    },
    AddOption {
        target: TmuxOptionTarget,
        hook: bool,
    },
    AddEnvironment {
        session: Option<SessionId>,
    },
    AddKey {
        table: String,
    },
    Reset,
    Unset,
    ResetTagged,
    UnsetTagged,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomizeMode {
    current: usize,
    offset: usize,
    height: usize,
    expanded: BTreeSet<String>,
    tagged: BTreeSet<String>,
    preview: Preview,
    filter: Option<String>,
    search: Option<String>,
    prompt: Option<(ModePrompt, PromptPurpose)>,
    help: bool,
    format: Option<String>,
    hide_global: bool,
    changed_only: bool,
    accept: bool,
    rebuild: bool,
    rebuild_tag: Option<String>,
    pub kill_source: bool,
    pub zoom: bool,
}

impl CustomizeMode {
    #[must_use]
    pub fn prompt_command_mode(&self) -> bool {
        self.prompt
            .as_ref()
            .is_some_and(|(prompt, _)| prompt.command_mode())
    }

    #[must_use]
    pub fn prompt_kind(&self) -> (&'static str, &'static [&'static str]) {
        match &self.prompt {
            Some((prompt, _)) if prompt.is_single() => ("command", &["SINGLE", "NOFORMAT"]),
            Some((_, PromptPurpose::Search | PromptPurpose::Filter)) => ("search", &["NOFORMAT"]),
            _ => ("command", &["NOFORMAT"]),
        }
    }

    #[must_use]
    pub fn prompt_input(&self) -> String {
        self.prompt
            .as_ref()
            .map(|(prompt, _)| prompt.input())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Item {
    Section,
    Option {
        name: String,
        array_key: Option<String>,
        target: TmuxOptionTarget,
        metadata: Option<TmuxOption>,
        value: String,
        global: bool,
        hook: bool,
        monitor: Option<String>,
    },
    Environment {
        name: String,
        session: Option<SessionId>,
        value: Option<String>,
        hidden: bool,
    },
    Key {
        table: String,
        key: String,
    },
    KeyField {
        table: String,
        key: String,
    },
}

#[derive(Clone, Debug)]
struct Row {
    id: String,
    name: String,
    text: Option<String>,
    depth: u8,
    parent: Option<usize>,
    children: bool,
    no_tag: bool,
    item: Item,
}

struct ModePromptOptions {
    vi: bool,
    separators: String,
}

impl ModePromptOptions {
    fn prompt(&self, label: impl Into<String>, input: &str) -> ModePrompt {
        ModePrompt::new(label, input, &self.separators).with_status_keys(self.vi)
    }

    fn single(&self, label: impl Into<String>) -> ModePrompt {
        ModePrompt::single(label).with_status_keys(self.vi)
    }

    fn search(&self, label: impl Into<String>, input: &str) -> ModePrompt {
        self.prompt(label, input)
            .with_history_type(CommandPromptType::Search)
    }
}

pub struct CustomizeResult {
    pub close: bool,
    pub commands: Vec<CommandInvocation>,
    pub menu: Option<CustomizeMenu>,
    pub edit: Option<CustomizeEdit>,
    pub stop_on_error: bool,
    pub message: Option<String>,
    pub remembered: Option<(CommandPromptType, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomizeEdit {
    pub value: String,
    kind: EditKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EditKind {
    Option {
        name: String,
        array_key: Option<String>,
        target: TmuxOptionTarget,
    },
    KeyCommand {
        table: String,
        key: String,
    },
    KeyNote {
        table: String,
        key: String,
    },
    Environment {
        name: String,
        session: Option<SessionId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomizeMenu {
    pub line: usize,
    pub outside: bool,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CustomizeMenuItem {
    pub name: &'static str,
    pub key: &'static str,
    pub annotation: &'static str,
    pub feed: &'static str,
}

pub const CUSTOMIZE_MENU_ITEMS: [Option<CustomizeMenuItem>; 11] = [
    Some(CustomizeMenuItem {
        name: "Select",
        key: "Enter",
        annotation: "Enter",
        feed: "Enter",
    }),
    Some(CustomizeMenuItem {
        name: "Edit",
        key: "e",
        annotation: "e",
        feed: "e",
    }),
    Some(CustomizeMenuItem {
        name: "Expand",
        key: "Right",
        annotation: "Right",
        feed: "Right",
    }),
    None,
    Some(CustomizeMenuItem {
        name: "Tag",
        key: "t",
        annotation: "t",
        feed: "t",
    }),
    Some(CustomizeMenuItem {
        name: "Tag All",
        key: "[DC4]",
        annotation: "[DC4]",
        feed: "\x14",
    }),
    Some(CustomizeMenuItem {
        name: "Tag None",
        key: "T",
        annotation: "T",
        feed: "T",
    }),
    None,
    Some(CustomizeMenuItem {
        name: "Changed Only",
        key: "C",
        annotation: "C",
        feed: "C",
    }),
    None,
    Some(CustomizeMenuItem {
        name: "Cancel",
        key: "q",
        annotation: "q",
        feed: "q",
    }),
];

pub const CUSTOMIZE_OUTSIDE_MENU_ITEMS: [Option<CustomizeMenuItem>; 4] = [
    Some(CustomizeMenuItem {
        name: "Scroll Left",
        key: "<",
        annotation: "<",
        feed: "<",
    }),
    Some(CustomizeMenuItem {
        name: "Scroll Right",
        key: ">",
        annotation: ">",
        feed: ">",
    }),
    None,
    Some(CustomizeMenuItem {
        name: "Cancel",
        key: "q",
        annotation: "q",
        feed: "q",
    }),
];

#[must_use]
pub fn customize_menu_feed(outside: bool, index: usize) -> Option<&'static str> {
    let items: &[Option<CustomizeMenuItem>] = if outside {
        &CUSTOMIZE_OUTSIDE_MENU_ITEMS
    } else {
        &CUSTOMIZE_MENU_ITEMS
    };
    items.get(index).and_then(|item| item.map(|item| item.feed))
}

impl CustomizeResult {
    fn message(message: String) -> Self {
        Self {
            message: Some(message),
            ..Self::stay()
        }
    }

    const fn stay() -> Self {
        Self {
            close: false,
            commands: Vec::new(),
            menu: None,
            edit: None,
            stop_on_error: false,
            message: None,
            remembered: None,
        }
    }
}

fn customize_lines(rows: &[Row], mode: &CustomizeMode) -> Vec<usize> {
    let mut hidden_below: Option<u8> = None;
    let mut lines = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if hidden_below.is_some_and(|depth| row.depth > depth) {
            continue;
        }
        hidden_below = (!mode.expanded.contains(&row.id)).then_some(row.depth);
        lines.push(index);
    }
    lines
}

fn below(height: usize, current: usize) -> bool {
    height.checked_sub(1).is_some_and(|last| current > last)
}

impl CustomizeMode {
    fn up(&mut self, size: usize, wrap: bool) {
        if size == 0 {
            return;
        }
        if self.current == 0 {
            if wrap {
                self.current = size - 1;
                if size >= self.height {
                    self.offset = size - self.height;
                }
            }
        } else {
            self.current -= 1;
            if self.current < self.offset {
                self.offset -= 1;
            }
        }
    }

    fn down(&mut self, size: usize, wrap: bool) -> bool {
        if size == 0 {
            return false;
        }
        if self.current == size - 1 {
            if !wrap {
                return false;
            }
            self.current = 0;
            self.offset = 0;
        } else {
            self.current += 1;
            if self.current + 1 > self.offset + self.height {
                self.offset += 1;
            }
        }
        true
    }

    fn check_selected(&mut self, size: usize) {
        if self.height == 0 {
            return;
        }
        if size <= self.height {
            self.offset = 0;
        } else if self.offset > size - self.height {
            self.offset = size - self.height;
        }
        if self.current < self.offset {
            self.offset = self.current;
        } else if self.current >= self.offset + self.height {
            self.offset = self.current + 1 - self.height;
        }
    }

    fn offset_for_current(&mut self) {
        self.offset = if below(self.height, self.current) {
            self.current + 1 - self.height
        } else {
            0
        };
    }
}

impl MuxEngine {
    pub(super) fn customize_mode(
        &self,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, ServerError> {
        let (options, positional) = parse_command_options("customize-mode", args)?;
        reject_positionals("customize-mode", &positional)?;
        let pane = self.resolve_pane(options.value("-t"), context.window, context.pane)?;
        let mut mode = CustomizeMode {
            format: options.value("-F").map(str::to_owned),
            filter: options.value("-f").map(str::to_owned),
            preview: match options.count("-N") {
                0 => Preview::Normal,
                1 => Preview::Off,
                _ => Preview::Big,
            },
            accept: options.has("-y"),
            kill_source: options.has("-k"),
            zoom: options.has("-Z"),
            ..CustomizeMode::default()
        };
        self.customize_height(pane, &mut mode);
        Ok(Execution::effect(MuxEffect::PaneModeChanged {
            pane,
            mode: Some(PaneModeRequest::Customize(Box::new(mode))),
        }))
    }

    fn customize_screen_rows(&self, pane: PaneId) -> usize {
        self.pane_geometry(pane)
            .map_or(24, |(_, rows)| usize::from(rows))
    }

    fn customize_screen_columns(&self, pane: PaneId) -> usize {
        self.pane_geometry(pane)
            .map_or(80, |(columns, _)| usize::from(columns))
    }

    fn customize_height(&self, pane: PaneId, mode: &mut CustomizeMode) {
        let sy = self.customize_screen_rows(pane);
        if mode.preview == Preview::Off {
            mode.height = sy;
        } else if 12 < sy {
            mode.height = sy - 12;
        }
        if sy.checked_sub(mode.height).is_some_and(|rest| rest < 2) {
            mode.height = sy;
        }
    }

    fn customize_array_values(&self, target: TmuxOptionTarget, name: &str) -> StringArray {
        self.array_option_readback(target, name, true)
            .map(|(values, _)| values.clone())
            .unwrap_or_default()
    }

    fn customize_build(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        tag: Option<String>,
        expand: &mut CustomizeExpand<'_>,
    ) {
        let rows = self.customize_rows(pane, mode, expand);
        mode.tagged.retain(|id| {
            rows.iter().find(|row| row.id == *id).is_some_and(|row| {
                row.parent
                    .is_none_or(|parent| mode.expanded.contains(&rows[parent].id))
            })
        });
        let lines = customize_lines(&rows, mode);
        self.customize_height(pane, mode);
        if let Some(found) = tag.and_then(|tag| lines.iter().position(|line| rows[*line].id == tag))
        {
            mode.current = found;
        } else if mode.current >= lines.len() && !lines.is_empty() {
            mode.current = lines.len() - 1;
        }
        mode.check_selected(lines.len());
    }

    pub fn customize_finish(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        expand: &mut CustomizeExpand<'_>,
    ) {
        if std::mem::take(&mut mode.rebuild) {
            let tag = mode.rebuild_tag.take();
            self.customize_build(pane, mode, tag, expand);
        }
    }

    pub fn customize_key(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        name: &str,
        history: PromptHistories<'_>,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let key = ModeKey::parse(name);
        let Some(lines) = self.customize_ready(pane, mode, expand) else {
            return CustomizeResult {
                close: true,
                commands: Vec::new(),
                menu: None,
                edit: None,
                stop_on_error: false,
                message: None,
                remembered: None,
            };
        };
        if let Some((prompt, _)) = &mut mode.prompt {
            let outcome = prompt.key_with_history(key, history.of(prompt.history_type()));
            let mut remembered = None;
            let (value, purpose) = match outcome {
                PromptOutcome::Done => {
                    let value = prompt.input();
                    remembered = prompt.remembered();
                    (Some(value), mode.prompt.take().map(|(_, purpose)| purpose))
                }
                PromptOutcome::Cancelled => (None, mode.prompt.take().map(|(_, purpose)| purpose)),
                PromptOutcome::Closed => {
                    mode.prompt = None;
                    return CustomizeResult::stay();
                }
                _ => return CustomizeResult::stay(),
            };
            let Some(purpose) = purpose else {
                return CustomizeResult::stay();
            };
            let mut result = self.customize_answer(pane, mode, purpose, value, expand);
            result.remembered = remembered;
            return result;
        }
        if mode.help {
            mode.help = false;
            return CustomizeResult::stay();
        }
        self.customize_tree_key(pane, mode, key, lines, expand)
    }

    fn customize_ready(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        expand: &mut CustomizeExpand<'_>,
    ) -> Option<usize> {
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        if lines.is_empty() {
            return None;
        }
        mode.current = mode.current.min(lines.len() - 1);
        Some(lines.len())
    }

    pub fn customize_mouse(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        name: &str,
        x: usize,
        y: usize,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let Some(size) = self.customize_ready(pane, mode, expand) else {
            return CustomizeResult {
                close: true,
                commands: Vec::new(),
                menu: None,
                edit: None,
                stop_on_error: false,
                message: None,
                remembered: None,
            };
        };
        let columns = self.customize_screen_columns(pane);
        let screen_rows = self.customize_screen_rows(pane);
        let prompt_top = self.customize_prompt_top(pane);
        if let Some((prompt, _)) = &mut mode.prompt {
            let row = if prompt_top {
                0
            } else {
                screen_rows.saturating_sub(1)
            };
            let handled = ModeMouseKey::is_press1(name)
                && y == row
                && prompt.mouse(x, columns) != PromptOutcome::NotHandled;
            if handled {
                return CustomizeResult::stay();
            }
        }
        if mode.help {
            return CustomizeResult::stay();
        }
        let button = ModeMouseKey::parse(name);
        if x > columns || y > mode.height {
            if button != ModeMouseKey::Down3 {
                return CustomizeResult::stay();
            }
            let line = if mode.offset + y < size {
                mode.offset + y
            } else {
                mode.current
            };
            return self.customize_menu_result(pane, mode, line, true, expand);
        }
        if mode.offset + y >= size {
            if button != ModeMouseKey::Down3 {
                return CustomizeResult::stay();
            }
            return self.customize_menu_result(pane, mode, mode.current, false, expand);
        }
        if matches!(
            button,
            ModeMouseKey::Down1 | ModeMouseKey::Down3 | ModeMouseKey::DoubleClick1
        ) {
            mode.current = mode.offset + y;
        }
        if button == ModeMouseKey::DoubleClick1 {
            return self.customize_tree_key(pane, mode, ModeKey::Char('\r'), size, expand);
        }
        if button != ModeMouseKey::Down3 {
            return CustomizeResult::stay();
        }
        self.customize_menu_result(pane, mode, mode.current, false, expand)
    }

    pub fn customize_menu_choice(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        line: usize,
        name: &str,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let Some(size) = self.customize_ready(pane, mode, expand) else {
            return CustomizeResult {
                close: true,
                commands: Vec::new(),
                menu: None,
                edit: None,
                stop_on_error: false,
                message: None,
                remembered: None,
            };
        };
        if line >= size {
            return CustomizeResult::stay();
        }
        mode.current = line;
        self.customize_key(pane, mode, name, PromptHistories::default(), expand)
    }

    fn customize_menu_result(
        &self,
        pane: PaneId,
        mode: &CustomizeMode,
        line: usize,
        outside: bool,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        let name = lines
            .get(line)
            .and_then(|index| rows.get(*index))
            .map_or_else(String::new, |row| row.name.clone());
        CustomizeResult {
            close: false,
            commands: Vec::new(),
            menu: Some(CustomizeMenu {
                line,
                outside,
                name,
            }),
            edit: None,
            stop_on_error: false,
            message: None,
            remembered: None,
        }
    }

    fn customize_tree_key(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        key: ModeKey,
        size: usize,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        if lines.is_empty() {
            return CustomizeResult {
                close: true,
                commands: Vec::new(),
                menu: None,
                edit: None,
                stop_on_error: false,
                message: None,
                remembered: None,
            };
        }
        let mut key = key;
        if let Some(choice) = (0..size.min(36)).find(|index| {
            key == if *index < 10 {
                ModeKey::Char(char::from(b'0' + *index as u8))
            } else {
                ModeKey::Meta(char::from(b'a' + (*index - 10) as u8))
            }
        }) {
            mode.current = choice;
            key = ModeKey::Char('\r');
        }
        let row_at = |mode: &CustomizeMode| &rows[lines[mode.current]];
        match key {
            ModeKey::Char('q' | '\u{1b}') | ModeKey::Ctrl('[' | 'g') => {
                return CustomizeResult {
                    close: true,
                    commands: Vec::new(),
                    menu: None,
                    edit: None,
                    stop_on_error: false,
                    message: None,
                    remembered: None,
                };
            }
            ModeKey::Char('e') => {
                if let Some(edit) = self.customize_start_edit(row_at(mode)) {
                    return CustomizeResult {
                        edit: Some(edit),
                        ..CustomizeResult::stay()
                    };
                }
            }
            ModeKey::F1 | ModeKey::Ctrl('h') => mode.help = true,
            ModeKey::Up | ModeKey::Char('k') | ModeKey::Ctrl('p') => mode.up(size, true),
            ModeKey::Down | ModeKey::Char('j') | ModeKey::Ctrl('n') => {
                mode.down(size, true);
            }
            ModeKey::PageUp | ModeKey::Ctrl('b') => {
                for _ in 0..mode.height {
                    if mode.current == 0 {
                        break;
                    }
                    mode.up(size, true);
                }
            }
            ModeKey::PageDown | ModeKey::Ctrl('f') => {
                for _ in 0..mode.height {
                    if mode.current == size - 1 {
                        break;
                    }
                    mode.down(size, true);
                }
            }
            ModeKey::Char('g') | ModeKey::Home => {
                mode.current = 0;
                mode.offset = 0;
            }
            ModeKey::Char('G') | ModeKey::End => {
                mode.current = size - 1;
                mode.offset_for_current();
            }
            ModeKey::Char('t') => {
                let row = row_at(mode);
                if !row.no_tag {
                    if mode.tagged.contains(&row.id) {
                        mode.tagged.remove(&row.id);
                    } else {
                        let mut parent = row.parent;
                        while let Some(index) = parent {
                            mode.tagged.remove(&rows[index].id);
                            parent = rows[index].parent;
                        }
                        let id = row.id.clone();
                        mode.tagged
                            .retain(|tagged| !customize_is_descendant(&rows, &id, tagged));
                        mode.tagged.insert(id);
                    }
                }
            }
            ModeKey::Char('T') => {
                for line in &lines {
                    mode.tagged.remove(&rows[*line].id);
                }
            }
            ModeKey::Ctrl('t') => {
                for line in &lines {
                    let row = &rows[*line];
                    let tag = match row.parent {
                        None => !row.no_tag,
                        Some(parent) => rows[parent].no_tag,
                    };
                    if tag {
                        mode.tagged.insert(row.id.clone());
                    } else {
                        mode.tagged.remove(&row.id);
                    }
                }
            }
            ModeKey::Char('O' | 'r') => {
                let tag = row_at(mode).id.clone();
                self.customize_build(pane, mode, Some(tag), expand);
            }
            ModeKey::Left | ModeKey::Char('h' | '-') => {
                let line = lines[mode.current];
                let flat = customize_flat(&rows, line);
                let mut target = Some(line);
                if flat || !mode.expanded.contains(&rows[line].id) {
                    target = rows[line].parent;
                }
                match target {
                    None => mode.up(size, false),
                    Some(target) => {
                        mode.expanded.remove(&rows[target].id);
                        if let Some(position) = lines.iter().position(|line| *line == target) {
                            mode.current = position;
                        }
                        let tag = row_at(mode).id.clone();
                        self.customize_build(pane, mode, Some(tag), expand);
                    }
                }
            }
            ModeKey::Right | ModeKey::Char('l' | '+') => {
                let line = lines[mode.current];
                if customize_flat(&rows, line) || mode.expanded.contains(&rows[line].id) {
                    mode.down(size, false);
                } else {
                    mode.expanded.insert(rows[line].id.clone());
                    let tag = rows[line].id.clone();
                    self.customize_build(pane, mode, Some(tag), expand);
                }
            }
            ModeKey::Meta('-') => {
                let tag = row_at(mode).id.clone();
                for row in rows.iter().filter(|row| row.parent.is_none()) {
                    mode.expanded.remove(&row.id);
                }
                self.customize_build(pane, mode, Some(tag), expand);
            }
            ModeKey::Meta('+') => {
                let tag = row_at(mode).id.clone();
                for row in rows.iter().filter(|row| row.parent.is_none()) {
                    mode.expanded.insert(row.id.clone());
                }
                self.customize_build(pane, mode, Some(tag), expand);
            }
            ModeKey::Char('?' | '/') | ModeKey::Ctrl('s') => {
                mode.prompt = Some((
                    self.customize_prompt_options(pane).search("(search) ", ""),
                    PromptPurpose::Search,
                ));
            }
            ModeKey::Char(direction @ ('n' | 'N')) => {
                self.customize_search_set(pane, mode, direction == 'N', expand);
            }
            ModeKey::Char('f') => {
                let input = mode.filter.clone().unwrap_or_default();
                mode.prompt = Some((
                    self.customize_prompt_options(pane)
                        .search("(filter) ", &input),
                    PromptPurpose::Filter,
                ));
            }
            ModeKey::Char('c') => {
                mode.prompt = None;
                mode.filter = None;
                let tag = row_at(mode).id.clone();
                self.customize_build(pane, mode, Some(tag), expand);
            }
            ModeKey::Char('v') => {
                mode.preview = match mode.preview {
                    Preview::Off => Preview::Big,
                    Preview::Normal => Preview::Off,
                    Preview::Big => Preview::Normal,
                };
                let tag = row_at(mode).id.clone();
                self.customize_build(pane, mode, Some(tag), expand);
            }
            _ => {}
        }
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        let Some(row) = lines.get(mode.current).map(|line| rows[*line].clone()) else {
            return CustomizeResult::stay();
        };
        let tag = Some(row.id.clone());
        let separators = self.customize_prompt_options(pane);
        match key {
            ModeKey::Char('a') => {
                if let Item::Option {
                    name,
                    array_key: Some(array_key),
                    target,
                    ..
                } = &row.item
                {
                    mode.prompt = Some((
                        separators.prompt(format!("({name}[{array_key}]) "), array_key),
                        PromptPurpose::ArrayKey {
                            name: name.clone(),
                            array_key: array_key.clone(),
                            target: *target,
                        },
                    ));
                }
            }
            ModeKey::Char('\r' | 's' | 'w' | 'S' | 'W') => {
                let global = matches!(key, ModeKey::Char('S' | 'W'));
                let pane_scope = matches!(key, ModeKey::Char('\r' | 's'));
                let mut commands = Vec::new();
                match &row.item {
                    Item::Section => {
                        if matches!(key, ModeKey::Char('\r' | 's')) {
                            self.customize_add_current(pane, mode, &row, &separators);
                            return CustomizeResult::stay();
                        }
                    }
                    Item::Environment { name, session, .. } => {
                        if matches!(key, ModeKey::Char('w')) {
                            return CustomizeResult::stay();
                        }
                        self.customize_set_environment(mode, name, *session, global, &separators);
                    }
                    Item::Key {
                        table,
                        key: binding,
                    }
                    | Item::KeyField {
                        table,
                        key: binding,
                    } => {
                        if !matches!(key, ModeKey::Char('\r' | 's')) {
                            return CustomizeResult::stay();
                        }
                        if let Some(command) =
                            self.customize_set_key(mode, &row, table, binding, &separators)
                        {
                            commands.push(command);
                        }
                    }
                    Item::Option { .. } => {
                        if let Some(command) = self.customize_set_option(
                            pane,
                            mode,
                            &row,
                            global,
                            pane_scope,
                            &separators,
                        ) {
                            commands.push(command);
                        }
                    }
                }
                return self.customize_commands(pane, mode, commands, tag, expand);
            }
            ModeKey::Char('d') => {
                if matches!(row.item, Item::Section | Item::Environment { .. })
                    || matches!(
                        row.item,
                        Item::Option {
                            array_key: Some(_),
                            ..
                        }
                    )
                {
                    return CustomizeResult::stay();
                }
                let name = customize_item_name(&row);
                return self.customize_confirm(
                    pane,
                    mode,
                    format!("Reset {name} to default? "),
                    PromptPurpose::Reset,
                    expand,
                );
            }
            ModeKey::Char('u') => {
                if matches!(row.item, Item::Section) {
                    return CustomizeResult::stay();
                }
                let label = match &row.item {
                    Item::Option {
                        name,
                        array_key: Some(array_key),
                        ..
                    } => format!("Unset {name}[{array_key}]? "),
                    _ => format!("Unset {}? ", customize_item_name(&row)),
                };
                return self.customize_confirm(pane, mode, label, PromptPurpose::Unset, expand);
            }
            ModeKey::Char(change @ ('D' | 'U')) => {
                let count = lines
                    .iter()
                    .filter(|line| mode.tagged.contains(&rows[**line].id))
                    .count();
                if count == 0 {
                    return CustomizeResult::stay();
                }
                let (label, purpose) = if change == 'D' {
                    (
                        format!("Reset {count} tagged to default? "),
                        PromptPurpose::ResetTagged,
                    )
                } else {
                    (
                        format!("Unset {count} tagged? "),
                        PromptPurpose::UnsetTagged,
                    )
                };
                return self.customize_confirm(pane, mode, label, purpose, expand);
            }
            ModeKey::Char('H') => {
                mode.hide_global = !mode.hide_global;
                self.customize_build(pane, mode, tag, expand);
            }
            ModeKey::Char('C') => {
                mode.changed_only = !mode.changed_only;
                self.customize_build(pane, mode, tag, expand);
            }
            _ => {}
        }
        CustomizeResult::stay()
    }

    fn customize_prompt_options(&self, pane: PaneId) -> ModePromptOptions {
        let session = self
            .state
            .window_for_pane(pane)
            .map(|window| self.state.windows[&window].session);
        let (vi, separators) = self.prompt_key_options(session);
        ModePromptOptions { vi, separators }
    }

    fn customize_confirm(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        label: String,
        purpose: PromptPurpose,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        if mode.accept {
            return self.customize_answer(pane, mode, purpose, Some("y".to_owned()), expand);
        }
        mode.prompt = Some((self.customize_prompt_options(pane).single(label), purpose));
        CustomizeResult::stay()
    }

    fn customize_commands(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        commands: Vec<CommandInvocation>,
        tag: Option<String>,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        if commands.is_empty() {
            self.customize_build(pane, mode, tag, expand);
        } else {
            mode.rebuild = true;
            mode.rebuild_tag = tag;
        }
        CustomizeResult {
            close: false,
            commands,
            menu: None,
            edit: None,
            stop_on_error: false,
            message: None,
            remembered: None,
        }
    }

    fn customize_start_edit(&self, row: &Row) -> Option<CustomizeEdit> {
        match &row.item {
            Item::Option {
                name,
                array_key,
                target,
                metadata,
                value,
                ..
            } => {
                if metadata.is_some_and(|option| {
                    matches!(
                        option.metadata.kind,
                        TmuxOptionKind::Flag | TmuxOptionKind::Choice
                    )
                }) {
                    return None;
                }
                Some(CustomizeEdit {
                    value: value.clone(),
                    kind: EditKind::Option {
                        name: name.clone(),
                        array_key: array_key.clone(),
                        target: *target,
                    },
                })
            }
            Item::KeyField { table, key } => {
                let binding = self.keys.get(table, key)?;
                let (value, kind) = match row.name.as_str() {
                    "Command" => (
                        customize_command_print(binding),
                        EditKind::KeyCommand {
                            table: table.clone(),
                            key: key.clone(),
                        },
                    ),
                    "Note" => (
                        binding.note.clone().unwrap_or_default(),
                        EditKind::KeyNote {
                            table: table.clone(),
                            key: key.clone(),
                        },
                    ),
                    _ => return None,
                };
                Some(CustomizeEdit { value, kind })
            }
            Item::Environment {
                name,
                session,
                value: Some(value),
                ..
            } => Some(CustomizeEdit {
                value: value.clone(),
                kind: EditKind::Environment {
                    name: name.clone(),
                    session: *session,
                },
            }),
            _ => None,
        }
    }

    pub fn customize_edited(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        edit: &CustomizeEdit,
        value: &str,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        let tag = lines.get(mode.current).map(|line| rows[*line].id.clone());
        let command = match &edit.kind {
            EditKind::Option {
                name,
                array_key,
                target,
            } => self.customize_option_set_command(name, array_key.as_deref(), *target, value),
            EditKind::KeyCommand { table, key } => self.keys.get(table, key).map(|binding| {
                customize_bind(table, key, binding.repeat, binding.note.as_deref(), value)
            }),
            EditKind::KeyNote { table, key } => self.keys.get(table, key).map(|binding| {
                customize_bind(
                    table,
                    key,
                    binding.repeat,
                    (!value.is_empty()).then_some(value),
                    &customize_command_print(binding),
                )
            }),
            EditKind::Environment { name, session } => {
                Some(self.customize_environment_set(name, *session, value, false))
            }
        };
        self.customize_commands(pane, mode, command.into_iter().collect(), tag, expand)
    }

    fn customize_option_set_command(
        &self,
        name: &str,
        array_key: Option<&str>,
        target: TmuxOptionTarget,
        value: &str,
    ) -> Option<CommandInvocation> {
        let array = tmux_options().any(|option| option.name == name && option.is_array);
        if !array {
            return Some(customize_set_command(name, target, value));
        }
        let key = match array_key {
            Some(key) => key.to_owned(),
            None if tmux_option_is_hook(name) => {
                let used = self
                    .hook_array(target, name)
                    .map(|hook| hook.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                first_free_array_index(used.iter()).ok()?.to_string()
            }
            None => {
                let values = self.customize_array_values(target, name);
                first_free_array_index(values.keys()).ok()?.to_string()
            }
        };
        Some(customize_set_command(
            &format!("{name}[{key}]"),
            target,
            value,
        ))
    }

    fn customize_environment_set(
        &self,
        name: &str,
        session: Option<SessionId>,
        value: &str,
        hidden: bool,
    ) -> CommandInvocation {
        let hidden = self
            .customize_environment_entries(session)
            .iter()
            .find(|(entry, _, _)| entry == name)
            .map_or(hidden, |(_, _, hidden)| *hidden);
        let mut args = Vec::new();
        if hidden {
            args.push("-h".to_owned());
        }
        args.extend(customize_environment_target(session));
        args.extend([name.to_owned(), value.to_owned()]);
        CommandInvocation::new("set-environment", args)
    }

    fn customize_set_key(
        &self,
        mode: &mut CustomizeMode,
        row: &Row,
        table: &str,
        key: &str,
        separators: &ModePromptOptions,
    ) -> Option<CommandInvocation> {
        let binding = self.keys.get(table, key)?;
        match row.name.as_str() {
            "Repeat" if matches!(row.item, Item::KeyField { .. }) => Some(customize_bind(
                table,
                key,
                !binding.repeat,
                binding.note.as_deref(),
                &customize_command_print(binding),
            )),
            "Command" if matches!(row.item, Item::KeyField { .. }) => {
                mode.prompt = Some((
                    separators.prompt(format!("({key}) "), &customize_command_print(binding)),
                    PromptPurpose::Command {
                        table: table.to_owned(),
                        key: key.to_owned(),
                    },
                ));
                None
            }
            "Note" if matches!(row.item, Item::KeyField { .. }) => {
                mode.prompt = Some((
                    separators.prompt(
                        format!("({key}) "),
                        binding.note.as_deref().unwrap_or_default(),
                    ),
                    PromptPurpose::Note {
                        table: table.to_owned(),
                        key: key.to_owned(),
                    },
                ));
                None
            }
            _ => None,
        }
    }

    fn customize_add_current(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        row: &Row,
        separators: &ModePromptOptions,
    ) {
        let Some(window) = self.state.window_for_pane(pane) else {
            return;
        };
        let session = self.state.windows[&window].session;
        let (label, input, purpose) = match row.name.as_str() {
            "Server Options" => (
                "New user option: ".to_owned(),
                "@",
                PromptPurpose::AddOption {
                    target: TmuxOptionTarget::Server,
                    hook: false,
                },
            ),
            "Session Options" | "Session Hooks" => {
                let hook = row.name == "Session Hooks";
                (
                    format!("New user {}: ", if hook { "hook" } else { "option" }),
                    "@",
                    PromptPurpose::AddOption {
                        target: TmuxOptionTarget::Session(session),
                        hook,
                    },
                )
            }
            "Window & Pane Options" | "Window & Pane Hooks" => {
                let hook = row.name == "Window & Pane Hooks";
                (
                    format!("New user {}: ", if hook { "hook" } else { "option" }),
                    "@",
                    PromptPurpose::AddOption {
                        target: TmuxOptionTarget::Pane(pane),
                        hook,
                    },
                )
            }
            "Global Environment" => (
                "New environment: ".to_owned(),
                "",
                PromptPurpose::AddEnvironment { session: None },
            ),
            "Session Environment" => (
                "New environment: ".to_owned(),
                "",
                PromptPurpose::AddEnvironment {
                    session: Some(session),
                },
            ),
            name => {
                let Some(table) = name.strip_prefix("Key Table - ") else {
                    return;
                };
                (
                    format!("New key in {table}: "),
                    "",
                    PromptPurpose::AddKey {
                        table: table.to_owned(),
                    },
                )
            }
        };
        mode.prompt = Some((separators.prompt(label, input), purpose));
    }

    fn customize_set_environment(
        &self,
        mode: &mut CustomizeMode,
        name: &str,
        session: Option<SessionId>,
        global: bool,
        separators: &ModePromptOptions,
    ) {
        let entries = self.customize_environment_entries(session);
        let Some((_, value, hidden)) = entries.iter().find(|(entry, _, _)| entry == name) else {
            return;
        };
        let session = if global { None } else { session };
        let label = match session {
            Some(session) => format!(
                "({name}, for {}) ",
                self.customize_scope(TmuxOptionTarget::Session(session))
            ),
            None => format!("({name}, global) "),
        };
        mode.prompt = Some((
            separators.prompt(label, value.as_deref().unwrap_or_default()),
            PromptPurpose::Environment {
                name: name.to_owned(),
                session,
                hidden: *hidden,
            },
        ));
    }

    fn customize_set_option(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        row: &Row,
        global: bool,
        pane_scope: bool,
        separators: &ModePromptOptions,
    ) -> Option<CommandInvocation> {
        let Item::Option {
            name,
            array_key,
            target: owner,
            metadata,
            ..
        } = &row.item
        else {
            return None;
        };
        let window = self.state.window_for_pane(pane)?;
        let session = self.state.windows.get(&window)?.session;
        let pane_scope =
            pane_scope && metadata.is_none_or(|option| option.scope == TmuxOptionScope::WindowPane);
        let array = metadata.is_some_and(|option| option.is_array);
        let local_window = if pane_scope {
            TmuxOptionTarget::Pane(pane)
        } else {
            TmuxOptionTarget::Window(window)
        };
        let target = if array {
            *owner
        } else if global {
            match owner {
                TmuxOptionTarget::Session(_) => TmuxOptionTarget::GlobalSession,
                TmuxOptionTarget::Window(_) | TmuxOptionTarget::Pane(_) => {
                    TmuxOptionTarget::GlobalWindow
                }
                other => *other,
            }
        } else {
            match owner {
                TmuxOptionTarget::GlobalSession => TmuxOptionTarget::Session(session),
                TmuxOptionTarget::Window(_)
                | TmuxOptionTarget::Pane(_)
                | TmuxOptionTarget::GlobalWindow => local_window,
                other => *other,
            }
        };
        if let Some(option) = metadata {
            let current = || {
                self.tmux_option_readback(*option, target, true)
                    .ok()
                    .flatten()
                    .map(|(value, _)| value)
                    .unwrap_or_default()
            };
            match option.metadata.kind {
                TmuxOptionKind::Flag => {
                    return Some(customize_set_command(
                        name,
                        target,
                        if current() == "on" { "off" } else { "on" },
                    ));
                }
                TmuxOptionKind::Choice if !option.metadata.choices.is_empty() => {
                    let choices = option.metadata.choices;
                    let value = current();
                    let index = choices.iter().position(|choice| *choice == value);
                    let next = index.map_or(0, |index| (index + 1) % choices.len());
                    return Some(customize_set_command(name, target, choices[next]));
                }
                _ => {}
            }
        }
        let scope = self.customize_scope(target);
        let space = if !scope.is_empty() {
            ", for "
        } else if target == TmuxOptionTarget::Server {
            ""
        } else {
            ", global"
        };
        let label = match (array, array_key) {
            (true, None) => format!("({name}[+]{space}{scope}) "),
            (true, Some(key)) => format!("({name}[{key}]{space}{scope}) "),
            (false, _) => format!("({name}{space}{scope}) "),
        };
        let value = match &row.item {
            Item::Option { value, .. } => value.clone(),
            _ => String::new(),
        };
        mode.prompt = Some((
            separators.prompt(label, &value),
            PromptPurpose::Option {
                name: name.clone(),
                array_key: array_key.clone(),
                target,
            },
        ));
        None
    }

    fn customize_answer(
        &self,
        pane: PaneId,
        mode: &mut CustomizeMode,
        purpose: PromptPurpose,
        value: Option<String>,
        expand: &mut CustomizeExpand<'_>,
    ) -> CustomizeResult {
        let rows = self.customize_rows(pane, mode, expand);
        let lines = customize_lines(&rows, mode);
        let current = lines.get(mode.current).map(|line| &rows[*line]);
        let tag = current.map(|row| row.id.clone());
        match purpose {
            PromptPurpose::Search => {
                mode.search = value.filter(|value| !value.is_empty());
                if mode.search.is_some() {
                    self.customize_search_set(pane, mode, false, expand);
                }
                CustomizeResult::stay()
            }
            PromptPurpose::Filter => {
                mode.filter = value.filter(|value| !value.is_empty());
                self.customize_build(pane, mode, tag, expand);
                CustomizeResult::stay()
            }
            PromptPurpose::Option {
                name,
                array_key,
                target,
            } => {
                let Some(value) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                let Some(command) =
                    self.customize_option_set_command(&name, array_key.as_deref(), target, &value)
                else {
                    return CustomizeResult::stay();
                };
                self.customize_commands(pane, mode, vec![command], tag, expand)
            }
            PromptPurpose::ArrayKey {
                name,
                array_key,
                target,
            } => {
                let Some(new_key) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                let Some(new_key) = customize_array_key(&new_key) else {
                    return CustomizeResult::message(format!("Bad array key: {new_key}"));
                };
                let values = self.customize_array_entries(target, &name);
                if values.iter().any(|(key, _)| *key == new_key) {
                    return CustomizeResult::stay();
                }
                let Some(entry) = values
                    .iter()
                    .find(|(key, _)| *key == array_key)
                    .map(|(_, entry)| entry.clone())
                else {
                    return CustomizeResult::stay();
                };
                let mut result = self.customize_commands(
                    pane,
                    mode,
                    vec![
                        customize_set_command(&format!("{name}[{new_key}]"), target, &entry),
                        customize_unset_command(&format!("{name}[{array_key}]"), target),
                    ],
                    tag,
                    expand,
                );
                result.stop_on_error = true;
                result
            }
            PromptPurpose::Environment {
                name,
                session,
                hidden,
            } => {
                let Some(value) = value else {
                    return CustomizeResult::stay();
                };
                let command = self.customize_environment_set(&name, session, &value, hidden);
                self.customize_commands(pane, mode, vec![command], tag, expand)
            }
            PromptPurpose::AddOption { target, hook } => {
                let Some(value) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                match customize_add_option(&value, target, hook) {
                    Ok(command) => self.customize_commands(pane, mode, vec![command], tag, expand),
                    Err(message) => CustomizeResult::message(message),
                }
            }
            PromptPurpose::AddEnvironment { session } => {
                let Some(value) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                match customize_add_environment(&value, session) {
                    Ok(command) => self.customize_commands(pane, mode, vec![command], tag, expand),
                    Err(message) => CustomizeResult::message(message),
                }
            }
            PromptPurpose::AddKey { table } => {
                let Some(value) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                match customize_add_key(&value, &table) {
                    Ok(command) => self.customize_commands(pane, mode, vec![command], tag, expand),
                    Err(message) => CustomizeResult::message(message),
                }
            }
            PromptPurpose::Command { table, key } => {
                let Some(command) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                let Some(binding) = self.keys.get(&table, &key) else {
                    return CustomizeResult::stay();
                };
                let bind = customize_bind(
                    &table,
                    &key,
                    binding.repeat,
                    binding.note.as_deref(),
                    &command,
                );
                self.customize_commands(pane, mode, vec![bind], tag, expand)
            }
            PromptPurpose::Note { table, key } => {
                let Some(note) = value.filter(|value| !value.is_empty()) else {
                    return CustomizeResult::stay();
                };
                let Some(binding) = self.keys.get(&table, &key) else {
                    return CustomizeResult::stay();
                };
                let bind = customize_bind(
                    &table,
                    &key,
                    binding.repeat,
                    Some(&note),
                    &customize_command_print(binding),
                );
                self.customize_commands(pane, mode, vec![bind], tag, expand)
            }
            PromptPurpose::Reset
            | PromptPurpose::Unset
            | PromptPurpose::ResetTagged
            | PromptPurpose::UnsetTagged => {
                if !value.is_some_and(|value| value.eq_ignore_ascii_case("y")) {
                    return CustomizeResult::stay();
                }
                let reset = matches!(purpose, PromptPurpose::Reset | PromptPurpose::ResetTagged);
                let targets = if matches!(purpose, PromptPurpose::Reset | PromptPurpose::Unset) {
                    current.map(|row| vec![row.clone()]).unwrap_or_default()
                } else {
                    lines
                        .iter()
                        .map(|line| &rows[*line])
                        .filter(|row| mode.tagged.contains(&row.id))
                        .cloned()
                        .collect()
                };
                let current_id = current.map(|row| row.id.clone());
                let mut commands = Vec::new();
                for row in targets {
                    let is_current = current_id.as_ref() == Some(&row.id)
                        || current.is_some_and(|current| customize_same_key(current, &row));
                    commands.extend(
                        self.customize_change(mode, &rows, &lines, &row, reset, is_current),
                    );
                }
                let tag = lines.get(mode.current).map(|line| rows[*line].id.clone());
                self.customize_commands(pane, mode, commands, tag, expand)
            }
        }
    }

    fn customize_change(
        &self,
        mode: &mut CustomizeMode,
        rows: &[Row],
        lines: &[usize],
        row: &Row,
        reset: bool,
        is_current: bool,
    ) -> Vec<CommandInvocation> {
        match &row.item {
            Item::Section => Vec::new(),
            Item::Environment { name, session, .. } => {
                if reset {
                    return Vec::new();
                }
                if is_current {
                    mode.up(lines.len(), false);
                }
                let mut args = vec!["-u".to_owned()];
                args.extend(customize_environment_target(*session));
                args.push(name.clone());
                vec![CommandInvocation::new("set-environment", args)]
            }
            Item::Option {
                name,
                array_key,
                target,
                ..
            } => {
                if reset && array_key.is_some() {
                    return Vec::new();
                }
                if !reset && array_key.is_some() && is_current {
                    mode.up(lines.len(), false);
                }
                if let Some(key) = array_key {
                    return vec![customize_unset_command(&format!("{name}[{key}]"), *target)];
                }
                if !reset {
                    return self.customize_remove_option(name, *target);
                }
                self.customize_option_chain(*target)
                    .into_iter()
                    .filter(|target| self.customize_holds(name, *target))
                    .flat_map(|target| self.customize_remove_option(name, target))
                    .collect()
            }
            Item::Key { table, key } | Item::KeyField { table, key } => {
                let Some(binding) = self.keys.get(table, key) else {
                    return Vec::new();
                };
                let default = KeyTables::default().get(table, key).cloned();
                if reset && let Some(default) = &default {
                    if default.commands == binding.commands {
                        return Vec::new();
                    }
                    return vec![customize_bind(
                        table,
                        key,
                        default.repeat,
                        default.note.as_deref(),
                        &customize_command_print(default),
                    )];
                }
                if is_current {
                    if let Some(line) = lines.get(mode.current) {
                        mode.expanded.remove(&rows[*line].id);
                    }
                    mode.up(lines.len(), false);
                }
                vec![CommandInvocation::new(
                    "unbind-key",
                    ["-T".to_owned(), table.clone(), key.clone()],
                )]
            }
        }
    }
}

fn customize_option_changed(
    metadata: Option<TmuxOption>,
    value: &str,
    entries: &[(String, String)],
    key: Option<&str>,
) -> bool {
    let Some(option) = metadata else {
        return true;
    };
    if tmux_option_is_hook(option.name) {
        return key.is_some() || !entries.is_empty();
    }
    if option.is_array {
        let defaults = customize_default_array(option);
        if let Some(key) = key {
            let default = defaults
                .iter()
                .find(|(default_key, _)| default_key == key)
                .map(|(_, value)| value.as_str());
            return default != Some(value);
        }
        let printed = |entries: &[(String, String)]| {
            entries
                .iter()
                .map(|(_, value)| value.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        };
        return printed(entries) != printed(&defaults);
    }
    customize_default_value(option).is_none_or(|default| default != value)
}

fn customize_default_array(option: TmuxOption) -> Vec<(String, String)> {
    thread_local! {
        static DEFAULTS: MuxEngine = MuxEngine::default();
    }
    let target = match option.scope {
        TmuxOptionScope::Server => TmuxOptionTarget::Server,
        TmuxOptionScope::Session => TmuxOptionTarget::GlobalSession,
        TmuxOptionScope::Window | TmuxOptionScope::WindowPane => TmuxOptionTarget::GlobalWindow,
    };
    DEFAULTS.with(|engine| {
        engine
            .customize_array_values(target, option.name)
            .into_iter()
            .map(|(key, value)| (key.display(), value))
            .collect()
    })
}

fn customize_default_value(option: TmuxOption) -> Option<String> {
    thread_local! {
        static DEFAULTS: MuxEngine = MuxEngine::default();
    }
    let target = match option.scope {
        TmuxOptionScope::Server => TmuxOptionTarget::Server,
        TmuxOptionScope::Session => TmuxOptionTarget::GlobalSession,
        TmuxOptionScope::Window | TmuxOptionScope::WindowPane => TmuxOptionTarget::GlobalWindow,
    };
    DEFAULTS.with(|engine| {
        engine
            .tmux_option_readback(option, target, false)
            .ok()
            .flatten()
            .map(|(value, _)| value)
    })
}

fn customize_is_descendant(rows: &[Row], ancestor: &str, id: &str) -> bool {
    let Some(mut row) = rows.iter().find(|row| row.id == id) else {
        return false;
    };
    while let Some(parent) = row.parent {
        if rows[parent].id == ancestor {
            return true;
        }
        row = &rows[parent];
    }
    false
}

fn customize_same_key(left: &Row, right: &Row) -> bool {
    match (&left.item, &right.item) {
        (
            Item::Key { table, key } | Item::KeyField { table, key },
            Item::Key {
                table: other_table,
                key: other_key,
            }
            | Item::KeyField {
                table: other_table,
                key: other_key,
            },
        ) => table == other_table && key == other_key,
        _ => false,
    }
}

fn customize_flat(rows: &[Row], line: usize) -> bool {
    let parent = rows[line].parent;
    let depth = rows[line].depth;
    !rows
        .iter()
        .any(|row| row.parent == parent && row.depth == depth && row.children)
}

fn customize_item_name(row: &Row) -> String {
    match &row.item {
        Item::Option { name, .. } | Item::Environment { name, .. } => name.clone(),
        Item::Key { key, .. } | Item::KeyField { key, .. } => key.clone(),
        Item::Section => row.name.clone(),
    }
}

fn customize_command_print(binding: &Binding) -> String {
    binding
        .commands
        .iter()
        .flat_map(|command| match parse_command_alias_group(command) {
            Ok(Some(commands)) => commands.iter().map(tmux_command_print).collect::<Vec<_>>(),
            _ => vec![tmux_command_print(command)],
        })
        .collect::<Vec<_>>()
        .join(" ; ")
}

fn customize_bind(
    table: &str,
    key: &str,
    repeat: bool,
    note: Option<&str>,
    command: &str,
) -> CommandInvocation {
    let mut args = Vec::new();
    if repeat {
        args.push("-r".to_owned());
    }
    if let Some(note) = note {
        args.extend(["-N".to_owned(), note.to_owned()]);
    }
    args.extend([
        "-T".to_owned(),
        table.to_owned(),
        key.to_owned(),
        command.to_owned(),
    ]);
    CommandInvocation::new("bind-key", args)
}

fn customize_target_args(target: TmuxOptionTarget) -> Vec<String> {
    match target {
        TmuxOptionTarget::Server => vec!["-s".to_owned()],
        TmuxOptionTarget::GlobalSession => vec!["-g".to_owned()],
        TmuxOptionTarget::GlobalWindow => vec!["-gw".to_owned()],
        TmuxOptionTarget::Session(id) => vec!["-t".to_owned(), id.to_string()],
        TmuxOptionTarget::Window(id) => vec!["-w".to_owned(), "-t".to_owned(), id.to_string()],
        TmuxOptionTarget::Pane(id) => vec!["-p".to_owned(), "-t".to_owned(), id.to_string()],
    }
}

fn customize_array_key(key: &str) -> Option<String> {
    if key.contains(']') {
        return None;
    }
    if !key.bytes().all(|byte| byte.is_ascii_digit()) {
        return Some(key.to_owned());
    }
    key.parse::<u32>().ok().map(|index| index.to_string())
}

fn customize_split_pair(value: &str) -> Option<(&str, &str)> {
    let end = value.find([' ', '\t'])?;
    let rest = value[end..].trim_start_matches([' ', '\t']);
    (end != 0 && !rest.is_empty()).then(|| (&value[..end], rest))
}

fn customize_add_option(
    value: &str,
    target: TmuxOptionTarget,
    hook: bool,
) -> Result<CommandInvocation, String> {
    let Some((name, value)) = customize_split_pair(value) else {
        return Err("User option must be @name value".to_owned());
    };
    if !name.starts_with('@') || name.contains('[') {
        return Err(format!(
            "User {} name must start with @",
            if hook { "hook" } else { "option" }
        ));
    }
    if !hook {
        return Ok(customize_set_command(name, target, value));
    }
    let mut args = customize_target_args(target);
    args.extend([name.to_owned(), value.to_owned()]);
    Ok(CommandInvocation::new("set-hook", args))
}

fn customize_add_environment(
    value: &str,
    session: Option<SessionId>,
) -> Result<CommandInvocation, String> {
    let mut args = customize_environment_target(session);
    if let Some(name) = value.strip_prefix('-') {
        if name.is_empty() || name.contains('=') {
            return Err(format!("Bad environment variable: {value}"));
        }
        args.extend(["-r".to_owned(), name.to_owned()]);
    } else {
        let Some((name, value)) = value.split_once('=').filter(|(name, _)| !name.is_empty()) else {
            return Err("Environment variable must be NAME=value".to_owned());
        };
        args.extend([name.to_owned(), value.to_owned()]);
    }
    Ok(CommandInvocation::new("set-environment", args))
}

fn customize_add_key(value: &str, table: &str) -> Result<CommandInvocation, String> {
    let Some((key, command)) = customize_split_pair(value) else {
        return Err("Key binding must be key command".to_owned());
    };
    if parse_tmux_key_details(key).is_none() {
        return Err(format!("Unknown key: {key}"));
    }
    Ok(CommandInvocation::new(
        "bind-key",
        [
            "-T".to_owned(),
            table.to_owned(),
            key.to_owned(),
            command.to_owned(),
        ],
    ))
}

fn customize_environment_target(session: Option<SessionId>) -> Vec<String> {
    match session {
        None => vec!["-g".to_owned()],
        Some(session) => vec!["-t".to_owned(), session.to_string()],
    }
}

fn customize_set_command(name: &str, target: TmuxOptionTarget, value: &str) -> CommandInvocation {
    let mut args = customize_target_args(target);
    args.extend([name.to_owned(), value.to_owned()]);
    CommandInvocation::new("set-option", args)
}

fn customize_unset_command(name: &str, target: TmuxOptionTarget) -> CommandInvocation {
    let mut args = vec!["-u".to_owned()];
    args.extend(customize_target_args(target));
    args.push(name.to_owned());
    CommandInvocation::new("set-option", args)
}

struct PreviewWriter {
    width: usize,
    height: usize,
    lines: Vec<String>,
    x: usize,
    y: usize,
}

#[cfg(test)]
fn customize_expand(format: &str, variables: &BTreeMap<String, String>) -> String {
    struct Hooks<'a>(&'a BTreeMap<String, String>);
    impl StatusHooks for Hooks<'_> {
        fn strftime(&mut self, value: &str) -> String {
            value.to_owned()
        }
        fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
            String::new()
        }
        fn variable(&mut self, name: &str, _context: &StatusContext) -> Option<String> {
            self.0.get(name).cloned()
        }
    }
    crate::expand_format_values(format, &StatusContext::default(), &mut Hooks(variables))
}

#[cfg(test)]
mod edit_tests;

#[cfg(test)]
mod input_tests;

#[cfg(test)]
mod rows_tests;

#[cfg(test)]
mod test_support;
