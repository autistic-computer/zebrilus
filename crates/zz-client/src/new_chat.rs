use std::cmp::Reverse;

use zz_protocol::{
    AgentProvider, CommandInvocation, MuxSnapshot, PaneId, PaneKindSnapshot, PaneSnapshot,
    SessionId, SessionSnapshot, WindowId, WindowSnapshot,
};

use crate::{
    agent_launch::AgentLaunches,
    fuzzy::fuzzy_match,
    navigation::{last_component, pane_place, session_place},
    program::Program,
    sidebar::SidebarTarget,
    window_summary::{pane_agent, summarize_chat},
};

const HOME: &str = "~";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentChoice {
    pub program: &'static str,
    pub shortcut: &'static str,
}

pub const AGENT_CHOICES: &[AgentChoice] = &[
    AgentChoice {
        program: "claude",
        shortcut: "c",
    },
    AgentChoice {
        program: "codex",
        shortcut: "x",
    },
];

impl AgentChoice {
    #[must_use]
    pub fn program(self) -> &'static Program {
        Program::by_id(self.program).expect("agent choices name catalog programs")
    }

    #[must_use]
    pub fn title(self) -> &'static str {
        self.program().name
    }

    #[must_use]
    pub fn provider(self) -> AgentProvider {
        match self.program {
            "codex" => AgentProvider::Codex,
            _ => AgentProvider::ClaudeCode,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderChoice {
    pub path: String,
    pub agents: usize,
}

#[must_use]
pub fn recent_folders(snapshot: &MuxSnapshot) -> Vec<FolderChoice> {
    let mut folders: Vec<(FolderChoice, u64)> = Vec::new();
    let panes = snapshot
        .sessions
        .iter()
        .flat_map(|session| &session.windows)
        .flat_map(|window| window.panes.values());
    for pane in panes {
        let Some(place) = pane_place(pane) else {
            continue;
        };
        let agents = usize::from(pane_agent(pane).is_some());
        match folders.iter_mut().find(|(folder, _)| folder.path == place) {
            Some((folder, changed_at)) => {
                folder.agents += agents;
                *changed_at = (*changed_at).max(pane.summary.changed_at);
            }
            None => folders.push((
                FolderChoice {
                    path: place.to_owned(),
                    agents,
                },
                pane.summary.changed_at,
            )),
        }
    }
    folders.sort_by_key(|(folder, changed_at)| (Reverse(folder.agents > 0), Reverse(*changed_at)));
    let mut folders = folders
        .into_iter()
        .map(|(folder, _)| folder)
        .collect::<Vec<_>>();
    if folders.is_empty() {
        folders.push(FolderChoice {
            path: HOME.to_owned(),
            agents: 0,
        });
    }
    folders
}

#[must_use]
pub fn session_for_folder(snapshot: &MuxSnapshot, folder: &str) -> Option<SessionId> {
    snapshot
        .sessions
        .iter()
        .find(|session| session_place(session) == Some(folder))
        .map(|session| session.id)
}

fn session_name(folder: &str) -> String {
    let name = last_component(folder).replace(['.', ':'], "-");
    if name.is_empty() || name == HOME {
        "home".to_owned()
    } else {
        name
    }
}

fn command(name: &str, arguments: impl IntoIterator<Item = String>) -> CommandInvocation {
    CommandInvocation::new(name, arguments.into_iter().collect::<Vec<_>>())
}

fn session_target(session: SessionId) -> String {
    format!("{session}:")
}

fn holder(snapshot: &MuxSnapshot, pane: PaneId) -> Option<(&SessionSnapshot, WindowId)> {
    snapshot.sessions.iter().find_map(|session| {
        session
            .windows
            .iter()
            .find(|window| window.panes.contains_key(&pane))
            .map(|window| (session, window.id))
    })
}

#[must_use]
fn agent_pane_command(
    target: String,
    folder: &str,
    agent: AgentChoice,
    launches: &AgentLaunches,
) -> CommandInvocation {
    let mut args = vec!["-t".to_owned(), target, "-c".to_owned(), folder.to_owned()];
    args.extend(launches.agent_flags(agent.provider()));
    args.extend(["agent".to_owned(), agent.program.to_owned()]);
    command("select-pane-kind", args)
}

#[must_use]
pub fn first_chat_commands(
    folder: &str,
    agent: AgentChoice,
    launches: &AgentLaunches,
) -> Vec<CommandInvocation> {
    let name = session_name(folder);
    vec![
        command(
            "new-session",
            [
                "-d".to_owned(),
                "-s".to_owned(),
                name.clone(),
                "-c".to_owned(),
                folder.to_owned(),
            ],
        ),
        agent_pane_command(format!("{name}:"), folder, agent, launches),
        command("attach-session", ["-t".to_owned(), name]),
    ]
}

#[must_use]
pub fn landing_chat_commands(
    snapshot: &MuxSnapshot,
    folder: &str,
    agent: AgentChoice,
    launches: &AgentLaunches,
) -> Vec<CommandInvocation> {
    let Some(session) = session_for_folder(snapshot, folder) else {
        return first_chat_commands(folder, agent, launches);
    };
    let target = session_target(session);
    vec![
        command(
            "new-window",
            [
                "-t".to_owned(),
                target.clone(),
                "-c".to_owned(),
                folder.to_owned(),
            ],
        ),
        agent_pane_command(target, folder, agent, launches),
        command("attach-session", ["-t".to_owned(), session.to_string()]),
    ]
}

#[must_use]
pub fn start_agent_commands(
    snapshot: &MuxSnapshot,
    pane: PaneId,
    folder: &str,
    agent: AgentChoice,
    launches: &AgentLaunches,
) -> Vec<CommandInvocation> {
    let mut commands = vec![agent_pane_command(
        pane.to_string(),
        folder,
        agent,
        launches,
    )];
    let Some((session, window)) = holder(snapshot, pane) else {
        return commands;
    };
    let alone = session.windows.len() == 1;
    let place = session_place(session);
    let target = session_for_folder(snapshot, folder);
    let move_to = |target: String| {
        command(
            "move-window",
            ["-s".to_owned(), window.to_string(), "-t".to_owned(), target],
        )
    };
    match target {
        Some(target) if target == session.id => {}
        Some(target) => commands.push(move_to(session_target(target))),
        None if alone => commands.push(command(
            "rename-session",
            [
                "-t".to_owned(),
                session.id.to_string(),
                session_name(folder),
            ],
        )),
        None if place.is_none() => {}
        None => {
            let name = session_name(folder);
            commands.extend([
                command(
                    "new-session",
                    [
                        "-d".to_owned(),
                        "-s".to_owned(),
                        name.clone(),
                        "-c".to_owned(),
                        folder.to_owned(),
                    ],
                ),
                move_to(format!("{name}:")),
                command(
                    "kill-window",
                    ["-a".to_owned(), "-t".to_owned(), window.to_string()],
                ),
            ]);
        }
    }
    commands
}

#[must_use]
pub fn agent_picker_place(
    snapshot: &MuxSnapshot,
    pane: PaneId,
    chat_view: bool,
) -> Option<(SessionId, WindowId)> {
    if !chat_view {
        return None;
    }
    snapshot.sessions.iter().find_map(|session| {
        session.windows.iter().find_map(|window| {
            let held = window.panes.get(&pane)?;
            matches!(held.kind, PaneKindSnapshot::Picker).then_some((session.id, window.id))
        })
    })
}

#[must_use]
pub fn picker_in_split(snapshot: &MuxSnapshot, pane: PaneId) -> bool {
    snapshot
        .sessions
        .iter()
        .flat_map(|session| &session.windows)
        .find_map(|window| window.panes.get(&pane).map(|held| (window, held)))
        .is_some_and(|(window, held)| {
            window.panes.len() > 1 && matches!(held.kind, PaneKindSnapshot::Picker)
        })
}

fn find_pane(snapshot: &MuxSnapshot, pane: PaneId) -> Option<&PaneSnapshot> {
    snapshot
        .sessions
        .iter()
        .flat_map(|session| &session.windows)
        .find_map(|window| window.panes.get(&pane))
}

#[must_use]
pub fn picker_folders(snapshot: &MuxSnapshot, pane: PaneId) -> Vec<FolderChoice> {
    let mut folders = recent_folders(snapshot);
    let own = find_pane(snapshot, pane)
        .filter(|held| matches!(held.kind, PaneKindSnapshot::Terminal))
        .and_then(pane_place);
    if let Some(index) = own.and_then(|own| folders.iter().position(|folder| folder.path == own)) {
        let folder = folders.remove(index);
        folders.insert(0, folder);
    }
    folders
}

#[must_use]
pub fn close_picker_command(snapshot: &MuxSnapshot, pane: PaneId) -> Option<CommandInvocation> {
    find_pane(snapshot, pane)
        .filter(|held| matches!(held.kind, PaneKindSnapshot::Picker))
        .map(|_| command("kill-pane", ["-t".to_owned(), pane.to_string()]))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatViewEntry {
    Stay,
    Open(WindowId),
    NewChat,
}

fn all_windows(snapshot: &MuxSnapshot) -> impl Iterator<Item = &WindowSnapshot> {
    snapshot
        .sessions
        .iter()
        .flat_map(|session| &session.windows)
}

fn starting_chat(window: &WindowSnapshot) -> bool {
    !window.panes.is_empty()
        && window
            .panes
            .values()
            .all(|pane| matches!(pane.kind, PaneKindSnapshot::Picker))
}

#[must_use]
pub fn focused_window(snapshot: &MuxSnapshot, attached: Option<SessionId>) -> Option<WindowId> {
    let session = snapshot
        .sessions
        .iter()
        .find(|session| Some(session.id) == attached)?;
    Some(snapshot.focused_window_for(session))
}

#[must_use]
pub fn entering_chat_view(
    snapshot: &MuxSnapshot,
    focused: Option<WindowId>,
    last: Option<WindowId>,
) -> ChatViewEntry {
    let find = |id: WindowId| all_windows(snapshot).find(|window| window.id == id);
    let is_chat = |id: WindowId| find(id).and_then(summarize_chat).is_some();
    if snapshot.sessions.is_empty()
        || focused.is_some_and(|id| is_chat(id) || find(id).is_some_and(starting_chat))
    {
        return ChatViewEntry::Stay;
    }
    last.filter(|id| is_chat(*id))
        .or_else(|| {
            all_windows(snapshot)
                .filter_map(summarize_chat)
                .max_by_key(|chat| chat.changed_at)
                .map(|chat| chat.window)
        })
        .map_or(ChatViewEntry::NewChat, ChatViewEntry::Open)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatViewPlace {
    landing: bool,
    landing_from: Option<WindowId>,
    last_chat: Option<WindowId>,
    entered: bool,
}

impl ChatViewPlace {
    #[must_use]
    pub const fn landing(&self) -> bool {
        self.landing
    }

    fn land(&mut self, from: Option<WindowId>) {
        self.landing = true;
        self.landing_from = from;
    }

    pub fn new_chat(&mut self, snapshot: &MuxSnapshot, attached: Option<SessionId>) {
        if !self.landing {
            self.land(focused_window(snapshot, attached));
        }
    }

    pub const fn close_landing(&mut self) {
        self.landing = false;
    }

    pub fn leave(&mut self, snapshot: &MuxSnapshot, attached: Option<SessionId>) {
        if !std::mem::take(&mut self.landing) {
            self.last_chat = focused_window(snapshot, attached);
        }
    }

    pub fn enter(
        &mut self,
        snapshot: &MuxSnapshot,
        attached: Option<SessionId>,
    ) -> Option<WindowId> {
        self.entered = !snapshot.sessions.is_empty() && attached.is_some();
        if !self.entered {
            return None;
        }
        let focused = focused_window(snapshot, attached);
        match entering_chat_view(snapshot, focused, self.last_chat) {
            ChatViewEntry::Stay => None,
            ChatViewEntry::Open(window) => Some(window),
            ChatViewEntry::NewChat => {
                self.land(focused);
                None
            }
        }
    }

    pub fn follow(
        &mut self,
        snapshot: &MuxSnapshot,
        attached: Option<SessionId>,
    ) -> Option<WindowId> {
        if !self.entered {
            return self.enter(snapshot, attached);
        }
        let focused = focused_window(snapshot, attached);
        if self.landing
            && let Some(focused) = focused
            && Some(focused) != self.landing_from
            && all_windows(snapshot)
                .find(|window| window.id == focused)
                .and_then(summarize_chat)
                .is_some()
        {
            self.landing = false;
        }
        None
    }
}

#[must_use]
pub fn leave_picker_command(
    snapshot: &MuxSnapshot,
    attached: Option<SessionId>,
    target: SidebarTarget,
) -> Option<CommandInvocation> {
    let to = match target {
        SidebarTarget::Window(window) => window,
        SidebarTarget::Pane(pane) => {
            all_windows(snapshot)
                .find(|window| window.panes.contains_key(&pane))?
                .id
        }
        SidebarTarget::Host | SidebarTarget::Session(_) => return None,
    };
    let leaving = focused_window(snapshot, attached)?;
    (leaving != to
        && all_windows(snapshot).any(|window| window.id == leaving && starting_chat(window)))
    .then(|| command("kill-window", ["-t".to_owned(), leaving.to_string()]))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatStage {
    #[default]
    Agents,
    Folders,
    Machines,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatKey {
    Start(AgentChoice),
    ChooseFolder,
    ChooseMachine(usize),
    Terminal,
    Browser,
    Close,
    Changed,
    Ignored,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineChoice {
    pub host: u64,
    pub name: String,
    pub remote: bool,
    pub live: bool,
    pub current: bool,
}

impl MachineChoice {
    #[must_use]
    pub fn current(machines: &[Self]) -> Option<&Self> {
        machines.iter().find(|machine| machine.current)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderRow {
    pub path: String,
    pub note: Option<String>,
}

const LISTED_ROWS: usize = 8;

#[must_use]
pub fn listing_dir(query: &str) -> Option<&str> {
    if !(query.starts_with('/') || query.starts_with('~')) {
        return None;
    }
    query.rfind('/').map(|end| &query[..=end])
}

#[must_use]
pub fn folder_rows(query: &str, recents: &[FolderChoice], listed: &[String]) -> Vec<FolderRow> {
    let recent = |folder: &FolderChoice| FolderRow {
        path: folder.path.clone(),
        note: agents_note(folder.agents),
    };
    if query.is_empty() {
        return recents.iter().map(recent).collect();
    }
    let mut rows = Vec::new();
    let typed = query.trim_end_matches('/');
    if listing_dir(query).is_some() {
        let path = if typed.is_empty() { "/" } else { typed };
        rows.push(
            recents
                .iter()
                .find(|folder| folder.path == path)
                .map_or_else(
                    || FolderRow {
                        path: path.to_owned(),
                        note: None,
                    },
                    recent,
                ),
        );
    }
    let mut matched = recents
        .iter()
        .filter_map(|folder| fuzzy_match(query, &folder.path).map(|(score, _)| (score, folder)))
        .collect::<Vec<_>>();
    matched.sort_by_key(|(score, _)| *score);
    rows.extend(matched.into_iter().map(|(_, folder)| recent(folder)));
    if let Some(dir) = listing_dir(query) {
        let name = &query[dir.len()..];
        let mut found = listed
            .iter()
            .filter_map(|path| {
                let rest = path.strip_prefix(dir)?;
                fuzzy_match(name, rest).map(|(score, _)| (rest.matches('/').count(), score, path))
            })
            .collect::<Vec<_>>();
        found.sort_by_key(|(depth, score, path)| (*depth, *score, path.len()));
        rows.extend(
            found
                .into_iter()
                .take(LISTED_ROWS)
                .map(|(.., path)| FolderRow {
                    path: path.clone(),
                    note: None,
                }),
        );
    }
    let mut seen = Vec::new();
    rows.retain(|row| {
        let fresh = !seen.contains(&row.path);
        seen.push(row.path.clone());
        fresh
    });
    rows
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatPicker {
    pub stage: ChatStage,
    pub selected: usize,
    pub folder: Option<String>,
    pub query: String,
    pub machines: usize,
    pub terminal: bool,
    pub browser: bool,
}

impl ChatPicker {
    #[must_use]
    pub fn folder<'a>(&'a self, recents: &'a [FolderChoice]) -> &'a str {
        self.folder
            .as_deref()
            .or_else(|| recents.first().map(|folder| folder.path.as_str()))
            .unwrap_or(HOME)
    }

    #[must_use]
    pub fn pane_offers(&self) -> Vec<ChatKey> {
        [
            (self.terminal, ChatKey::Terminal),
            (self.browser, ChatKey::Browser),
        ]
        .into_iter()
        .filter_map(|(offered, key)| offered.then_some(key))
        .collect()
    }

    pub fn open(&mut self, stage: ChatStage) {
        self.stage = stage;
        self.selected = 0;
        self.query.clear();
    }

    pub fn pick(&mut self, path: &str) {
        self.folder = Some(path.to_owned());
        self.open(ChatStage::Agents);
    }

    fn step(&mut self, delta: isize, len: usize) -> ChatKey {
        if len == 0 {
            return ChatKey::Ignored;
        }
        self.selected = (self.selected + len).wrapping_add_signed(delta) % len;
        ChatKey::Changed
    }

    pub fn key(
        &mut self,
        key: &str,
        text: Option<&str>,
        modified: bool,
        rows: &[FolderRow],
    ) -> ChatKey {
        if modified {
            return ChatKey::Ignored;
        }
        match self.stage {
            ChatStage::Agents => {
                if let Some(agent) = AGENT_CHOICES.iter().find(|agent| agent.shortcut == key) {
                    return ChatKey::Start(*agent);
                }
                let offers = self.pane_offers();
                let choices = AGENT_CHOICES.len() + offers.len();
                match key {
                    "j" | "down" => self.step(1, choices),
                    "k" | "up" => self.step(-1, choices),
                    "t" if self.terminal => ChatKey::Terminal,
                    "b" if self.browser => ChatKey::Browser,
                    "enter" => AGENT_CHOICES
                        .get(self.selected)
                        .map(|agent| ChatKey::Start(*agent))
                        .or_else(|| {
                            self.selected
                                .checked_sub(AGENT_CHOICES.len())
                                .and_then(|index| offers.get(index).copied())
                        })
                        .unwrap_or(ChatKey::Ignored),
                    "tab" => ChatKey::ChooseFolder,
                    "m" if self.machines > 1 => {
                        self.open(ChatStage::Machines);
                        ChatKey::Changed
                    }
                    "escape" => ChatKey::Close,
                    _ => match text {
                        Some(start @ ("/" | "~")) => {
                            self.open(ChatStage::Folders);
                            self.query.push_str(start);
                            ChatKey::Changed
                        }
                        _ => ChatKey::Ignored,
                    },
                }
            }
            ChatStage::Machines => match key {
                "j" | "down" => self.step(1, self.machines),
                "k" | "up" => self.step(-1, self.machines),
                "enter" if self.selected < self.machines => {
                    let machine = self.selected;
                    self.open(ChatStage::Agents);
                    ChatKey::ChooseMachine(machine)
                }
                "escape" | "m" => {
                    self.open(ChatStage::Agents);
                    ChatKey::Changed
                }
                _ => ChatKey::Ignored,
            },
            ChatStage::Folders => match key {
                "down" => self.step(1, rows.len()),
                "up" => self.step(-1, rows.len()),
                "enter" => {
                    match rows.get(self.selected) {
                        Some(row) => {
                            let path = row.path.clone();
                            self.pick(&path);
                        }
                        None => self.open(ChatStage::Agents),
                    }
                    ChatKey::Changed
                }
                "tab" => {
                    if let Some(row) = rows.get(self.selected) {
                        self.query = format!("{}/", row.path.trim_end_matches('/'));
                        self.selected = 0;
                    }
                    ChatKey::Changed
                }
                "escape" => {
                    self.open(ChatStage::Agents);
                    ChatKey::Changed
                }
                "backspace" => {
                    self.query.pop();
                    self.selected = 0;
                    ChatKey::Changed
                }
                _ => match text
                    .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
                {
                    Some(text) => {
                        self.query.push_str(text);
                        self.selected = 0;
                        ChatKey::Changed
                    }
                    None => ChatKey::Ignored,
                },
            },
        }
    }
}

#[must_use]
pub fn folder_start(folder: &str) -> String {
    let folder = folder.trim_end_matches('/');
    match folder.rsplit_once('/') {
        Some(("", _)) | None if folder.starts_with('/') => "/".to_owned(),
        Some((parent, _)) => format!("{parent}/"),
        None => format!("{folder}/"),
    }
}

#[must_use]
pub fn agents_note(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 agent".to_owned()),
        count => Some(format!("{count} agents")),
    }
}

#[cfg(test)]
mod tests {
    use zz_protocol::{LayoutNode, PaneKindSnapshot, PaneSnapshot, PaneSummary, WindowSnapshot};

    use super::*;

    fn pane(
        id: u64,
        kind: PaneKindSnapshot,
        repo: &str,
        process: &str,
        changed_at: u64,
    ) -> PaneSnapshot {
        PaneSnapshot {
            id: PaneId(id),
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
            summary: PaneSummary {
                changed_at,
                cwd: repo.to_owned(),
                repo: repo.to_owned(),
                process: process.split_whitespace().map(str::to_owned).collect(),
                ..PaneSummary::default()
            },
        }
    }

    fn shell(id: u64, repo: &str, changed_at: u64) -> PaneSnapshot {
        pane(id, PaneKindSnapshot::Terminal, repo, "zsh", changed_at)
    }

    fn agent(id: u64, repo: &str, changed_at: u64) -> PaneSnapshot {
        pane(id, PaneKindSnapshot::Terminal, repo, "claude", changed_at)
    }

    fn picker(id: u64) -> PaneSnapshot {
        pane(id, PaneKindSnapshot::Picker, "", "", 0)
    }

    fn window(id: u64, panes: Vec<PaneSnapshot>) -> WindowSnapshot {
        let ids = panes.iter().map(|pane| pane.id).collect::<Vec<_>>();
        WindowSnapshot {
            id: WindowId(id),
            index: 0,
            name: "zsh".to_owned(),
            automatic_rename: true,
            active_pane: ids[0],
            zoomed_pane: None,
            layout: LayoutNode::Pane(ids[0]),
            panes: panes.into_iter().map(|pane| (pane.id, pane)).collect(),
            layout_dump: String::new(),
            visible_layout_dump: String::new(),
            status_label: String::new(),
            activity: false,
            silence: false,
            pane_border_status: zz_protocol::PaneBorderStatus::Off,
            pane_border_lines: zz_protocol::PaneBorderLines::Single,
            pane_border_indicators: zz_protocol::PaneBorderIndicators::Colour,
            pane_order: ids.clone(),
            pane_z_order: ids,
            floating: Vec::new(),
            modal: None,
            sx: 80,
            sy: 24,
            pane_base_index: 0,
        }
    }

    fn session(id: u64, windows: Vec<WindowSnapshot>) -> SessionSnapshot {
        SessionSnapshot {
            id: SessionId(id),
            name: id.to_string(),
            active_window: windows[0].id,
            windows,
            viewers: Vec::new(),
        }
    }

    fn snapshot(sessions: Vec<SessionSnapshot>) -> MuxSnapshot {
        MuxSnapshot {
            sessions,
            ..MuxSnapshot::default()
        }
    }

    fn lines(commands: &[CommandInvocation]) -> Vec<String> {
        commands
            .iter()
            .map(|command| {
                let args = command
                    .args
                    .iter()
                    .map(zz_protocol::RawText::as_str)
                    .collect::<Vec<_>>();
                format!("{} {}", command.name, args.join(" "))
            })
            .collect()
    }

    fn fleet() -> MuxSnapshot {
        snapshot(vec![
            session(
                1,
                vec![
                    window(1, vec![agent(1, "~/dev/zz", 50), shell(2, "~/dev/zz", 90)]),
                    window(2, vec![agent(3, "~/dev/zz", 40)]),
                    window(3, vec![picker(4)]),
                ],
            ),
            session(2, vec![window(4, vec![shell(5, "~/dev/clairvo", 99)])]),
        ])
    }

    #[test]
    fn folders_with_agents_come_first_then_the_newest() {
        assert_eq!(
            recent_folders(&fleet()),
            [
                FolderChoice {
                    path: "~/dev/zz".to_owned(),
                    agents: 2
                },
                FolderChoice {
                    path: "~/dev/clairvo".to_owned(),
                    agents: 0
                },
            ]
        );
        assert_eq!(recent_folders(&snapshot(Vec::new()))[0].path, "~");
    }

    #[test]
    fn the_folder_decides_the_session_the_chat_lands_in() {
        let fleet = fleet();
        let claude = AGENT_CHOICES[0];
        let none = AgentLaunches::default();
        let start = |folder: &str, agent: AgentChoice| {
            lines(&start_agent_commands(
                &fleet,
                PaneId(4),
                folder,
                agent,
                &none,
            ))
        };
        assert_eq!(claude.title(), "Claude Code");
        assert_eq!(
            start("~/dev/zz", claude),
            ["select-pane-kind -t %4 -c ~/dev/zz agent claude"]
        );
        assert_eq!(
            start("~/dev/clairvo", AGENT_CHOICES[1]),
            [
                "select-pane-kind -t %4 -c ~/dev/clairvo agent codex",
                "move-window -s @3 -t $2:",
            ]
        );
        assert_eq!(
            start("~/notes", claude),
            [
                "select-pane-kind -t %4 -c ~/notes agent claude",
                "new-session -d -s notes -c ~/notes",
                "move-window -s @3 -t notes:",
                "kill-window -a -t @3",
            ]
        );
    }

    #[test]
    fn a_first_chat_makes_its_own_session_and_attaches_to_it() {
        let none = AgentLaunches::default();
        assert_eq!(
            lines(&first_chat_commands("~", AGENT_CHOICES[0], &none)),
            [
                "new-session -d -s home -c ~",
                "select-pane-kind -t home: -c ~ agent claude",
                "attach-session -t home",
            ]
        );
        assert_eq!(
            lines(&first_chat_commands(
                "~/dev/zz.app",
                AGENT_CHOICES[1],
                &none
            ))[1],
            "select-pane-kind -t zz-app: -c ~/dev/zz.app agent codex"
        );
    }

    #[test]
    fn a_new_chat_starts_on_the_model_and_effort_last_picked() {
        let mut launches = AgentLaunches::default();
        launches.pick(
            AgentProvider::ClaudeCode,
            crate::agent_settings::MODEL,
            "haiku",
        );
        launches.pick(
            AgentProvider::ClaudeCode,
            crate::agent_settings::EFFORT,
            "low",
        );
        launches.pick(
            AgentProvider::Codex,
            crate::agent_settings::EFFORT,
            "medium",
        );
        assert_eq!(
            lines(&start_agent_commands(
                &fleet(),
                PaneId(4),
                "~/dev/zz",
                AGENT_CHOICES[0],
                &launches
            )),
            ["select-pane-kind -t %4 -c ~/dev/zz --model haiku --effort low agent claude"]
        );
        assert_eq!(
            lines(&first_chat_commands("~", AGENT_CHOICES[1], &launches))[1],
            "select-pane-kind -t home: -c ~ --effort medium agent codex"
        );
    }

    #[test]
    fn the_sidebar_view_decides_what_a_picker_offers() {
        let fleet = fleet();
        assert_eq!(
            agent_picker_place(&fleet, PaneId(4), true),
            Some((SessionId(1), WindowId(3)))
        );
        assert_eq!(agent_picker_place(&fleet, PaneId(4), false), None);
        let split = snapshot(vec![session(
            1,
            vec![window(1, vec![agent(1, "~/dev/zz", 50), picker(2)])],
        )]);
        assert_eq!(
            agent_picker_place(&split, PaneId(2), true),
            Some((SessionId(1), WindowId(1)))
        );
        assert_eq!(agent_picker_place(&split, PaneId(2), false), None);
        assert_eq!(agent_picker_place(&split, PaneId(1), true), None);
        assert_eq!(agent_picker_place(&fleet, PaneId(99), true), None);
    }

    #[test]
    fn chat_view_never_draws_the_agent_picker_over_a_terminal() {
        let idle = |id| pane(id, PaneKindSnapshot::Terminal, "~/notes", "", 10);
        let mut exited = pane(2, PaneKindSnapshot::Terminal, "~/notes", "claude", 10);
        exited.dead = true;
        let fleet = snapshot(vec![session(
            1,
            vec![
                window(1, vec![idle(1)]),
                window(2, vec![exited]),
                window(3, vec![pane(3, PaneKindSnapshot::Terminal, "~", "vim", 10)]),
                window(4, vec![agent(4, "~/dev/zz", 10)]),
                window(5, vec![idle(5), idle(6)]),
            ],
        )]);
        assert!((1..=6).all(|pane| agent_picker_place(&fleet, PaneId(pane), true).is_none()));
        assert_eq!(picker_folders(&fleet, PaneId(1))[0].path, "~/notes");
        assert_eq!(picker_folders(&fleet, PaneId(4))[0].path, "~/dev/zz");
        assert_eq!(
            picker_folders(&snapshot(Vec::new()), PaneId(1))[0].path,
            "~"
        );
        assert_eq!(
            lines(&start_agent_commands(
                &fleet,
                PaneId(1),
                "~/notes",
                AGENT_CHOICES[0],
                &AgentLaunches::default()
            )),
            ["select-pane-kind -t %1 -c ~/notes agent claude"]
        );
    }

    #[test]
    fn entering_chat_view_lands_on_a_chat_or_new_chat() {
        let fleet = fleet();
        let enter = |focused, last| entering_chat_view(&fleet, Some(WindowId(focused)), last);
        assert_eq!(enter(1, None), ChatViewEntry::Stay);
        assert_eq!(enter(3, None), ChatViewEntry::Stay);
        assert_eq!(
            enter(4, Some(WindowId(2))),
            ChatViewEntry::Open(WindowId(2))
        );
        assert_eq!(
            enter(4, Some(WindowId(4))),
            ChatViewEntry::Open(WindowId(1))
        );
        let shells = snapshot(vec![session(1, vec![window(1, vec![shell(1, "~", 9)])])]);
        assert_eq!(
            entering_chat_view(&shells, Some(WindowId(1)), None),
            ChatViewEntry::NewChat
        );
        assert_eq!(
            entering_chat_view(&snapshot(Vec::new()), None, None),
            ChatViewEntry::Stay
        );
        let leave = |focused: u64, target| {
            let mut held = fleet.clone();
            held.sessions[0].active_window = WindowId(focused);
            leave_picker_command(&held, Some(SessionId(1)), target)
        };
        assert_eq!(
            lines(
                &leave(3, SidebarTarget::Pane(PaneId(3)))
                    .into_iter()
                    .collect::<Vec<_>>()
            ),
            ["kill-window -t @3"]
        );
        assert_eq!(leave(3, SidebarTarget::Window(WindowId(3))), None);
        assert_eq!(leave(1, SidebarTarget::Window(WindowId(2))), None);
    }

    #[test]
    fn the_chat_view_place_enters_once_loaded_and_follows_new_chats() {
        let attached = Some(SessionId(1));
        let mut place = ChatViewPlace::default();
        assert_eq!(place.follow(&snapshot(Vec::new()), None), None);
        let mut shells = snapshot(vec![session(1, vec![window(1, vec![shell(1, "~", 9)])])]);
        assert_eq!(place.follow(&shells, attached), None);
        assert!(place.landing());
        shells.sessions[0]
            .windows
            .push(window(2, vec![agent(2, "~", 10)]));
        assert_eq!(place.follow(&shells, attached), None);
        assert!(place.landing());
        shells.sessions[0].active_window = WindowId(2);
        assert_eq!(place.follow(&shells, attached), None);
        assert!(!place.landing());
        place.leave(&shells, attached);
        shells.sessions[0].active_window = WindowId(1);
        assert_eq!(place.enter(&shells, attached), Some(WindowId(2)));
        place.new_chat(&shells, attached);
        assert!(place.landing());
        place.close_landing();
        assert!(!place.landing());
    }

    #[test]
    fn escape_closes_only_a_picker_pane() {
        let fleet = fleet();
        assert_eq!(
            close_picker_command(&fleet, PaneId(4))
                .as_ref()
                .map(|command| lines(std::slice::from_ref(command))),
            Some(vec!["kill-pane -t %4".to_owned()])
        );
        assert_eq!(close_picker_command(&fleet, PaneId(2)), None);
        assert_eq!(close_picker_command(&fleet, PaneId(99)), None);
    }

    #[test]
    fn the_picker_starts_agents_by_key_and_tabs_through_folders() {
        let recents = recent_folders(&fleet());
        let rows =
            |picker: &ChatPicker, listed: &[String]| folder_rows(&picker.query, &recents, listed);
        let mut picker = ChatPicker::default();
        assert_eq!(picker.folder(&recents), "~/dev/zz");
        assert_eq!(
            picker.key("x", Some("x"), false, &[]),
            ChatKey::Start(AGENT_CHOICES[1])
        );
        assert_eq!(picker.key("c", Some("c"), true, &[]), ChatKey::Ignored);
        assert_eq!(picker.key("tab", None, false, &[]), ChatKey::ChooseFolder);
        picker.open(ChatStage::Folders);
        let listed = rows(&picker, &[]);
        assert_eq!(picker.key("down", None, false, &listed), ChatKey::Changed);
        assert_eq!(picker.key("enter", None, false, &listed), ChatKey::Changed);
        assert_eq!(picker.folder(&recents), "~/dev/clairvo");
        assert_eq!(picker.stage, ChatStage::Agents);
        assert_eq!(
            picker.key("enter", None, false, &[]),
            ChatKey::Start(AGENT_CHOICES[0])
        );
        assert_eq!(picker.key("escape", None, false, &[]), ChatKey::Close);
    }

    #[test]
    fn t_and_b_open_a_pane_only_where_the_picker_offers_one() {
        let mut picker = ChatPicker::default();
        assert_eq!(picker.key("t", Some("t"), false, &[]), ChatKey::Ignored);
        assert_eq!(picker.key("b", Some("b"), false, &[]), ChatKey::Ignored);
        picker.terminal = true;
        picker.browser = true;
        assert_eq!(picker.key("t", Some("t"), false, &[]), ChatKey::Terminal);
        assert_eq!(picker.key("b", Some("b"), false, &[]), ChatKey::Browser);
        assert_eq!(picker.key("up", None, false, &[]), ChatKey::Changed);
        assert_eq!(picker.selected, AGENT_CHOICES.len() + 1);
        assert_eq!(picker.key("enter", None, false, &[]), ChatKey::Browser);
        assert_eq!(picker.key("up", None, false, &[]), ChatKey::Changed);
        assert_eq!(picker.key("enter", None, false, &[]), ChatKey::Terminal);
    }

    #[test]
    fn only_a_picker_beside_another_pane_is_a_split() {
        let snapshot = snapshot(vec![session(
            1,
            vec![
                window(1, vec![picker(1)]),
                window(2, vec![agent(2, "~/dev/zz", 5), picker(3)]),
                window(3, vec![shell(4, "~/dev/zz", 5), shell(5, "~/dev/zz", 5)]),
            ],
        )]);
        assert!(!picker_in_split(&snapshot, PaneId(1)));
        assert!(picker_in_split(&snapshot, PaneId(3)));
        assert!(!picker_in_split(&snapshot, PaneId(4)));
    }

    #[test]
    fn new_chat_opens_a_window_in_the_folders_session_or_makes_the_session() {
        let none = AgentLaunches::default();
        let snapshot = snapshot(vec![session(
            2,
            vec![window(1, vec![shell(1, "~/dev/zz", 5)])],
        )]);
        assert_eq!(
            lines(&landing_chat_commands(
                &snapshot,
                "~/dev/zz",
                AGENT_CHOICES[0],
                &none
            )),
            [
                "new-window -t $2: -c ~/dev/zz",
                "select-pane-kind -t $2: -c ~/dev/zz agent claude",
                "attach-session -t $2",
            ]
        );
        assert_eq!(
            lines(&landing_chat_commands(
                &snapshot,
                "~/notes",
                AGENT_CHOICES[1],
                &none
            )),
            lines(&first_chat_commands("~/notes", AGENT_CHOICES[1], &none))
        );
    }

    #[test]
    fn m_lists_the_machines_and_enter_picks_one_when_there_is_more_than_one() {
        let mut picker = ChatPicker::default();
        assert_eq!(picker.key("m", Some("m"), false, &[]), ChatKey::Ignored);
        picker.machines = 3;
        assert_eq!(picker.key("m", Some("m"), false, &[]), ChatKey::Changed);
        assert_eq!(picker.stage, ChatStage::Machines);
        assert_eq!(picker.key("x", Some("x"), false, &[]), ChatKey::Ignored);
        assert_eq!(picker.key("k", Some("k"), false, &[]), ChatKey::Changed);
        assert_eq!(
            picker.key("enter", None, false, &[]),
            ChatKey::ChooseMachine(2)
        );
        assert_eq!(picker.stage, ChatStage::Agents);
        picker.key("m", Some("m"), false, &[]);
        assert_eq!(picker.key("escape", None, false, &[]), ChatKey::Changed);
        assert_eq!(picker.stage, ChatStage::Agents);
    }

    #[test]
    fn typing_a_path_offers_it_with_the_folders_under_it() {
        let recents = recent_folders(&fleet());
        let listed = [
            "~/dev/zz",
            "~/dev/zz-landing",
            "~/dev/zz/crates",
            "~/dev/notes",
        ]
        .map(str::to_owned);
        let mut picker = ChatPicker::default();
        assert_eq!(picker.key("~", Some("~"), false, &[]), ChatKey::Changed);
        assert_eq!(picker.stage, ChatStage::Folders);
        for character in "/dev/zz-l".chars() {
            let text = character.to_string();
            picker.key(&text, Some(&text), false, &[]);
        }
        assert_eq!(picker.query, "~/dev/zz-l");
        assert_eq!(listing_dir(&picker.query), Some("~/dev/"));
        let paths = |rows: Vec<FolderRow>| rows.into_iter().map(|row| row.path).collect::<Vec<_>>();
        assert_eq!(
            paths(folder_rows(&picker.query, &recents, &listed)),
            ["~/dev/zz-l", "~/dev/zz-landing"]
        );
        picker.key("backspace", None, false, &[]);
        picker.key("backspace", None, false, &[]);
        assert_eq!(
            paths(folder_rows(&picker.query, &recents, &listed)),
            ["~/dev/zz", "~/dev/zz-landing", "~/dev/zz/crates"]
        );
        let rows = folder_rows(&picker.query, &recents, &listed);
        picker.key("down", None, false, &rows);
        picker.key("tab", None, false, &rows);
        assert_eq!(picker.query, "~/dev/zz-landing/");
        picker.key(
            "enter",
            None,
            false,
            &folder_rows(&picker.query, &recents, &listed),
        );
        assert_eq!(picker.folder(&recents), "~/dev/zz-landing");
        assert_eq!(picker.stage, ChatStage::Agents);
        assert_eq!(listing_dir("zz"), None);
        assert_eq!(folder_start("~/dev/zz"), "~/dev/");
        assert_eq!(folder_start("~"), "~/");
        assert_eq!(folder_start("/srv"), "/");
        assert_eq!(folder_start("/"), "/");
        assert_eq!(agents_note(2).as_deref(), Some("2 agents"));
    }
}
