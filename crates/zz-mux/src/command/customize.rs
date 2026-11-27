use std::fmt::Write as _;

use super::mode_prompt::{ModeKey, ModeMouseKey, ModePrompt, PromptHistories, PromptOutcome};
use super::*;
use crate::tmux_option_metadata::TmuxOptionKind;
use zz_protocol::{
    ChooseTreeItem, ChooseTreeState, ChooseTreeTarget, ChooserPresentation, ChooserPreview,
    ChooserPreviewSize, ChooserRow, CommandPromptType,
};

mod draw;
mod edit;
mod input;
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
