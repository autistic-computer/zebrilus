use std::cmp::Reverse;

use zz_protocol::{
    AgentProvider, CommandInvocation, MuxSnapshot, PaneId, SessionId, SessionSnapshot, WindowId,
};

use crate::{
    fuzzy::fuzzy_match,
    navigation::{last_component, pane_place, session_place},
    program::Program,
    window_summary::pane_agent,
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

    fn command(self) -> &'static str {
        self.program().commands[0]
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

#[must_use]
pub fn open_picker_commands(
    snapshot: &MuxSnapshot,
    attached: Option<SessionId>,
) -> Vec<CommandInvocation> {
    let folders = recent_folders(snapshot);
    let folder = folders[0].path.as_str();
    let session = session_for_folder(snapshot, folder)
        .or(attached.filter(|attached| {
            snapshot
                .sessions
                .iter()
                .any(|session| session.id == *attached)
        }))
        .or_else(|| snapshot.sessions.first().map(|session| session.id));
    let picker = |target: String| {
        command(
            "new-window",
            [
                "-t".to_owned(),
                target,
                "--kind".to_owned(),
                "picker".to_owned(),
            ],
        )
    };
    if let Some(session) = session {
        return vec![picker(session_target(session))];
    }
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
        picker(format!("{name}:")),
        command(
            "kill-window",
            ["-a".to_owned(), "-t".to_owned(), format!("{name}:")],
        ),
    ]
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
pub fn start_agent_commands(
    snapshot: &MuxSnapshot,
    pane: PaneId,
    folder: &str,
    agent: AgentChoice,
) -> Vec<CommandInvocation> {
    let mut commands = vec![command(
        "select-pane-kind",
        [
            "-t".to_owned(),
            pane.to_string(),
            "-c".to_owned(),
            folder.to_owned(),
            "terminal".to_owned(),
            agent.command().to_owned(),
        ],
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
pub fn chat_picker_place(snapshot: &MuxSnapshot, pane: PaneId) -> Option<(SessionId, WindowId)> {
    snapshot.sessions.iter().find_map(|session| {
        session
            .windows
            .iter()
            .find(|window| window.panes.contains_key(&pane))
            .filter(|window| window.panes.len() == 1)
            .map(|window| (session.id, window.id))
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatStage {
    #[default]
    Agents,
    Folders,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatKey {
    Start(AgentChoice),
    ChooseFolder,
    Close,
    Changed,
    Ignored,
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
}

impl ChatPicker {
    #[must_use]
    pub fn folder<'a>(&'a self, recents: &'a [FolderChoice]) -> &'a str {
        self.folder
            .as_deref()
            .or_else(|| recents.first().map(|folder| folder.path.as_str()))
            .unwrap_or(HOME)
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
                match key {
                    "j" | "down" => self.step(1, AGENT_CHOICES.len()),
                    "k" | "up" => self.step(-1, AGENT_CHOICES.len()),
                    "enter" => AGENT_CHOICES
                        .get(self.selected)
                        .map_or(ChatKey::Ignored, |agent| ChatKey::Start(*agent)),
                    "tab" => ChatKey::ChooseFolder,
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
    fn a_new_chat_opens_its_picker_in_the_session_of_the_last_folder() {
        assert_eq!(
            lines(&open_picker_commands(&fleet(), Some(SessionId(2)))),
            ["new-window -t $1: --kind picker"]
        );
        assert_eq!(
            lines(&open_picker_commands(&snapshot(Vec::new()), None)),
            [
                "new-session -d -s home -c ~",
                "new-window -t home: --kind picker",
                "kill-window -a -t home:",
            ]
        );
    }

    #[test]
    fn the_folder_decides_the_session_the_chat_lands_in() {
        let fleet = fleet();
        let claude = AGENT_CHOICES[0];
        assert_eq!(claude.title(), "Claude Code");
        assert_eq!(
            lines(&start_agent_commands(&fleet, PaneId(4), "~/dev/zz", claude)),
            ["select-pane-kind -t %4 -c ~/dev/zz terminal claude"]
        );
        assert_eq!(
            lines(&start_agent_commands(
                &fleet,
                PaneId(4),
                "~/dev/clairvo",
                AGENT_CHOICES[1]
            )),
            [
                "select-pane-kind -t %4 -c ~/dev/clairvo terminal codex",
                "move-window -s @3 -t $2:",
            ]
        );
        assert_eq!(
            lines(&start_agent_commands(&fleet, PaneId(4), "~/notes", claude)),
            [
                "select-pane-kind -t %4 -c ~/notes terminal claude",
                "new-session -d -s notes -c ~/notes",
                "move-window -s @3 -t notes:",
                "kill-window -a -t @3",
            ]
        );
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
        assert_eq!(
            chat_picker_place(&fleet(), PaneId(4)),
            Some((SessionId(1), WindowId(3)))
        );
        assert_eq!(chat_picker_place(&fleet(), PaneId(1)), None);
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
