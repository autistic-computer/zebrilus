use std::{fmt, rc::Rc};

use ala::SharedString;
pub(crate) use zz_client::fuzzy::fuzzy_match;
pub(crate) use zz_client::palette::{count, needs_arguments, target_kind};
use zz_client::{
    completion::{CompletionSuggestion, PaneKindAvailability},
    place::{Fleet, Place},
    recents::RecentKey,
    verbs::Verb,
};
use zz_protocol::{
    ChooseTreeState, ChooseTreeTarget, CommandSpec, CommandValueKind, PaneId, SessionId, WindowId,
    command_specs,
};

use super::{
    PalettePill, PaletteRow, PaletteStatus,
    palette_places::{Chip, Draft},
};
use crate::IconName;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteMode {
    Command,
    /// `@`: a target in zz's grammar, a host, session, window, pane or file
    /// ([`crate::target`]).
    Target,
}

impl PaletteMode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::Target => "Go to",
        }
    }

    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Command => ":",
            Self::Target => "@",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PaletteHostId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PaletteTarget {
    Session(SessionId),
    Window(WindowId),
    Pane(PaneId),
}

impl fmt::Display for PaletteTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session(id) => id.fmt(formatter),
            Self::Window(id) => id.fmt(formatter),
            Self::Pane(id) => id.fmt(formatter),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteSettings {
    pub grouped: bool,
    pub show_keys: bool,
    pub vim: bool,
}

impl Default for PaletteSettings {
    fn default() -> Self {
        Self {
            grouped: true,
            show_keys: true,
            vim: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PaletteTree {
    pub attached: PaletteHostId,
    pub hosts: Vec<PaletteTreeHost>,
}

impl PaletteTree {
    #[must_use]
    pub fn host(&self, id: PaletteHostId) -> Option<&PaletteTreeHost> {
        self.hosts.iter().find(|host| host.id == id)
    }
}

#[derive(Clone, Debug)]
pub struct PaletteTreeHost {
    pub id: PaletteHostId,
    pub name: String,
    pub detail: String,
    pub right: String,
    pub status: PaletteStatus,
    pub sessions: Vec<PaletteTreeSession>,
}

impl PaletteTreeHost {
    #[must_use]
    pub fn connected(&self) -> bool {
        self.status == PaletteStatus::Online
    }
}

#[derive(Clone, Debug)]
pub struct PaletteTreeSession {
    pub id: SessionId,
    pub name: String,
    pub tmux_name: String,
    pub active: bool,
    pub windows: Vec<PaletteTreeWindow>,
}

#[derive(Clone, Debug)]
pub struct PaletteTreeWindow {
    pub id: WindowId,
    pub index: u32,
    pub name: String,
    pub active: bool,
    pub active_pane: PaneId,
    pub panes: Vec<PaletteTreePane>,
}

impl PaletteTreeWindow {
    fn running(&self) -> bool {
        self.panes.iter().any(|pane| pane.running)
    }
}

#[derive(Clone, Debug)]
pub struct PaletteTreePane {
    pub id: PaneId,
    pub index: u32,
    pub label: String,
    pub detail: SharedString,
    /// The program in the foreground, such as Claude Code.
    pub program: Option<SharedString>,
    pub icon: IconName,
    pub running: bool,
    pub needs_you: bool,
    pub terminal: bool,
    pub cwd: String,
}

#[derive(Clone)]
pub(crate) enum PaletteAction {
    Place(Place),
    Recent(RecentKey),
    Target {
        host: PaletteHostId,
        target: PaletteTarget,
        label: String,
    },
    Command(&'static CommandSpec),
    History(String),
    Completion(CompletionSuggestion),
    ChooseWindow(u32),
    Verb(usize),
    Fill(Place),
    Submit,
}

#[derive(Clone)]
pub(crate) struct PaletteEntry {
    pub row: PaletteRow,
    pub action: Option<PaletteAction>,
    pub hint: &'static str,
    pub score: usize,
}

impl PaletteEntry {
    pub(crate) fn row(row: PaletteRow, action: PaletteAction) -> Self {
        Self {
            row,
            action: Some(action),
            hint: "",
            score: 0,
        }
    }

    pub(crate) fn header(label: impl Into<SharedString>, hint: &'static str) -> Self {
        Self {
            row: PaletteRow {
                label: label.into(),
                ..Default::default()
            },
            action: None,
            hint,
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

pub(crate) struct UnifiedPalette {
    pub mode: Option<PaletteMode>,
    pub host: Option<PaletteHostId>,
    pub command: Option<&'static CommandSpec>,
    pub chips: Vec<Chip>,
    pub fleet: Rc<Fleet>,
    pub verbs: Rc<[Verb]>,
    pub draft: Option<Draft>,
    pub tree: PaletteTree,
    pub rows: Vec<PaletteEntry>,
    chooser_current_window: Option<u32>,
    pub grouped: bool,
    pub show_keys: bool,
}

impl UnifiedPalette {
    pub fn new(mode: Option<PaletteMode>) -> Self {
        let settings = PaletteSettings::default();
        Self {
            mode,
            host: None,
            command: None,
            chips: Vec::new(),
            fleet: Rc::default(),
            verbs: Rc::from(Vec::new()),
            draft: None,
            tree: PaletteTree::default(),
            rows: Vec::new(),
            chooser_current_window: None,
            grouped: settings.grouped,
            show_keys: settings.show_keys,
        }
    }

    pub fn apply_settings(&mut self, settings: PaletteSettings) {
        self.grouped = settings.grouped;
        self.show_keys = settings.show_keys;
    }

    pub fn pop_layer(&mut self) -> bool {
        self.command.take().is_some() || self.mode.take().is_some() || self.host.take().is_some()
    }

    pub fn navigate(&self, selected: Option<usize>, direction: isize) -> Option<usize> {
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

    pub fn initial_selection(&self) -> Option<usize> {
        self.rows
            .iter()
            .position(|entry| {
                entry.row.right == "current"
                    && match entry.action {
                        Some(PaletteAction::Target {
                            target: PaletteTarget::Window(_),
                            ..
                        }) => true,
                        Some(PaletteAction::ChooseWindow(index)) => {
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

    pub fn parent_index(&self, index: usize) -> Option<usize> {
        let depth = self.rows.get(index)?.row.indent;
        self.rows[..index]
            .iter()
            .enumerate()
            .rev()
            .take_while(|(_, entry)| entry.action.is_some())
            .find_map(|(index, entry)| (entry.row.indent < depth).then_some(index))
    }

    pub fn child_index(&self, index: usize) -> Option<usize> {
        let entry = self.rows.get(index)?;
        (entry.row.expanded == Some(true)
            && self
                .rows
                .get(index + 1)
                .is_some_and(|child| child.action.is_some() && child.row.indent > entry.row.indent))
        .then_some(index + 1)
    }

    pub fn pills(&self) -> Vec<PalettePill> {
        if self.is_places() {
            return self.place_pills();
        }
        let mut pills = Vec::new();
        if let Some(host) = self.host.and_then(|id| self.tree.host(id)) {
            pills.push(PalettePill::Host {
                label: host.name.clone().into(),
                status: host.status,
            });
        }
        if let Some(mode) = self.mode {
            pills.push(PalettePill::Mode {
                prefix: mode.prefix().into(),
                label: mode.label().into(),
            });
        }
        if let Some(command) = self.command {
            pills.push(PalettePill::Command(command.name.into()));
        }
        pills
    }

    pub fn placeholder(&self) -> String {
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
            Some(PaletteMode::Command) => "Type a command".to_owned(),
            Some(PaletteMode::Target) => "A session, window, pane or ~/file".to_owned(),
            None => {
                let scope = self
                    .host
                    .and_then(|id| self.tree.host(id))
                    .map_or(String::new(), |host| format!(" {}", host.name));
                format!("Search{scope}, or type : or @")
            }
        }
    }

    pub fn usage(&self, selected: Option<usize>) -> Option<&'static str> {
        if self.mode != Some(PaletteMode::Command) {
            return None;
        }
        self.command
            .or_else(|| {
                selected
                    .and_then(|index| self.rows.get(index))
                    .and_then(|entry| match entry.action {
                        Some(PaletteAction::Command(spec)) => Some(spec),
                        _ => None,
                    })
            })
            .map(|spec| spec.usage)
    }

    pub fn rebuild_window_chooser(&mut self, state: &ChooseTreeState, query: &str) {
        let query = query.trim();
        let tree = self.grouped && query.is_empty();
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
                    (String::new(), session_active, IconName::Folder)
                }
                ChooseTreeTarget::Window(_) => {
                    name.clone_into(&mut window_name);
                    window_active = session_active && item.active();
                    if window_active {
                        self.chooser_current_window = Some(index);
                    }
                    (session_name.clone(), window_active, IconName::PanelsTopLeft)
                }
                ChooseTreeTarget::Pane(_) => (
                    [session_name.as_str(), window_name.as_str()]
                        .into_iter()
                        .filter(|name| !name.is_empty())
                        .collect::<Vec<_>>()
                        .join(" / "),
                    window_active && item.active(),
                    match item.pane_kind {
                        Some(zz_protocol::ChooseTreePaneKind::Terminal) => IconName::SquareTerminal,
                        Some(zz_protocol::ChooseTreePaneKind::Browser) => IconName::Globe,
                        Some(zz_protocol::ChooseTreePaneKind::Editor) => IconName::File,
                        None => IconName::Plus,
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
            let row = PaletteRow {
                label: format!("{prefix}{name}").into(),
                detail: if item.text.is_empty() {
                    item.detail.clone()
                } else {
                    String::new()
                }
                .into(),
                muted_prefix: prefix.len(),
                right: if active { "current" } else { "" }.into(),
                icon: Some(icon),
                expanded: (tree && item.has_children()).then_some(item.expanded()),
                indent: if tree {
                    f32::from(item.depth) * 18.0
                } else {
                    0.0
                },
                ..Default::default()
            };
            if let Some(entry) = PaletteEntry::row(row, PaletteAction::ChooseWindow(index))
                .matching(query, &metadata)
            {
                self.rows.push(entry);
            }
        }
        if !tree {
            self.rows.sort_by_key(|entry| entry.score);
        }
    }

    fn navigation_entries(&self, query: &str) -> Vec<PaletteEntry> {
        let window_only = self.command.and_then(target_kind) == Some(CommandValueKind::Window);
        let scope = self.host.unwrap_or(self.tree.attached);
        let mut rows = Vec::new();
        for host in self
            .tree
            .hosts
            .iter()
            .filter(|host| host.connected() && host.id == scope)
        {
            for session in &host.sessions {
                for window in &session.windows {
                    if window_only {
                        let entry = window_entry(host, session, window);
                        let metadata = format!("{} {} {}", session.id, window.id, entry.row.detail);
                        rows.extend(entry.matching(query, &metadata));
                        continue;
                    }
                    for pane in &window.panes {
                        let mut entry = pane_entry(host, session, window, pane);
                        let prefix = format!("{} / {} / ", session.name, window.name);
                        entry.row.label = format!("{prefix}{}", pane.label).into();
                        entry.row.muted_prefix = prefix.len();
                        let metadata = format!(
                            "{} {} {} {}",
                            session.id, window.id, pane.id, entry.row.detail
                        );
                        rows.extend(entry.matching(query, &metadata));
                    }
                }
            }
        }
        rows.sort_by_key(|entry| entry.score);
        rows
    }

    pub fn rebuild(
        &mut self,
        query: &str,
        tree: PaletteTree,
        availability: PaneKindAvailability,
        shortcut: &dyn Fn(&str) -> Option<SharedString>,
    ) {
        self.tree = tree;
        self.chooser_current_window = None;
        let query = query.trim();
        self.rows.clear();
        if self.command.is_some() {
            self.rows = self.navigation_entries(query);
        } else if self.mode == Some(PaletteMode::Command) {
            let show_keys = self.show_keys;
            let shortcut = move |name: &str| show_keys.then(|| shortcut(name)).flatten();
            self.rows = command_entries(query, availability, &shortcut);
        }
    }
}

pub(crate) fn command_entries(
    query: &str,
    availability: PaneKindAvailability,
    shortcut: &dyn Fn(&str) -> Option<SharedString>,
) -> Vec<PaletteEntry> {
    let mut entries: Vec<_> = command_specs()
        .filter(|spec| availability.allows_command(spec.name))
        .filter_map(|spec| {
            PaletteEntry::row(
                PaletteRow {
                    label: spec.name.into(),
                    detail: spec.description.into(),
                    icon: Some(IconName::Terminal),
                    shortcut: shortcut(spec.name),
                    ..Default::default()
                },
                PaletteAction::Command(spec),
            )
            .matching(query, &spec.aliases.join(" "))
        })
        .collect();
    entries.sort_by_key(|entry| entry.score);
    entries
}

fn window_entry(
    host: &PaletteTreeHost,
    session: &PaletteTreeSession,
    window: &PaletteTreeWindow,
) -> PaletteEntry {
    let prefix = format!("{} / ", session.name);
    let label = format!("{prefix}{}", window.name);
    PaletteEntry::row(
        PaletteRow {
            label: label.clone().into(),
            muted_prefix: prefix.len(),
            detail: count(window.panes.len(), "pane").into(),
            right: if window.active { "current" } else { "" }.into(),
            icon: Some(IconName::PanelsTopLeft),
            running: window.running(),
            ..Default::default()
        },
        PaletteAction::Target {
            host: host.id,
            target: PaletteTarget::Window(window.id),
            label,
        },
    )
}

fn pane_entry(
    host: &PaletteTreeHost,
    session: &PaletteTreeSession,
    window: &PaletteTreeWindow,
    pane: &PaletteTreePane,
) -> PaletteEntry {
    PaletteEntry::row(
        PaletteRow {
            label: pane.label.clone().into(),
            detail: pane.detail.clone(),
            icon: Some(pane.icon.clone()),
            right: if window.active && window.active_pane == pane.id {
                "current"
            } else {
                ""
            }
            .into(),
            ..Default::default()
        },
        PaletteAction::Target {
            host: host.id,
            target: PaletteTarget::Pane(pane.id),
            label: format!("{} / {} / {}", session.name, window.name, pane.label),
        },
    )
}

#[cfg(test)]
#[path = "palette_model_tests.rs"]
mod tests;
