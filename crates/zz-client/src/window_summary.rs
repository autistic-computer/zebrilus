use std::{cmp::Reverse, collections::BTreeMap, path::Path};

use url::Url;
use zz_protocol::{
    AgentProvider, MuxSnapshot, PaneId, PaneKindSnapshot, PaneSnapshot, PaneStatusKind,
    PaneStatusState, SessionId, SessionSnapshot, ShellCommand, WindowId, WindowSnapshot,
};

use crate::{
    navigation::{SessionLabel, last_component, ordered_panes, pane_label, session_label},
    program::{Program, resolve},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneTag {
    Picker,
    Terminal,
    Browser,
    Editor,
    Agent(AgentProvider),
    View,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneTile {
    pub pane: PaneId,
    pub tag: PaneTag,
    pub label: String,
    pub program: Option<&'static Program>,
    pub attention: Attention,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Muted,
    Warning,
    Danger,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetailPart {
    pub text: String,
    pub tone: Tone,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Attention {
    #[default]
    None,
    Working,
    Bell,
    Done,
    Failed,
    NeedsYou,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowSummary {
    pub window: WindowId,
    pub index: u32,
    pub headline: PaneId,
    pub title: String,
    pub detail: Vec<DetailPart>,
    pub changed_at: u64,
    pub tiles: Vec<PaneTile>,
    pub attention: Attention,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttentionRollup {
    pub needs_you: u32,
    pub failed: u32,
    pub done: u32,
    pub bell: u32,
    pub working: u32,
    pub changed_at: u64,
}

impl AttentionRollup {
    pub fn add(&mut self, window: &WindowSummary) {
        self.count(window.attention, window.changed_at);
    }

    fn count(&mut self, attention: Attention, changed_at: u64) {
        self.changed_at = self.changed_at.max(changed_at);
        let count = match attention {
            Attention::NeedsYou => &mut self.needs_you,
            Attention::Failed => &mut self.failed,
            Attention::Done => &mut self.done,
            Attention::Bell => &mut self.bell,
            Attention::Working => &mut self.working,
            Attention::None => return,
        };
        *count = count.saturating_add(1);
    }

    pub fn merge(&mut self, other: &Self) {
        self.needs_you = self.needs_you.saturating_add(other.needs_you);
        self.failed = self.failed.saturating_add(other.failed);
        self.done = self.done.saturating_add(other.done);
        self.bell = self.bell.saturating_add(other.bell);
        self.working = self.working.saturating_add(other.working);
        self.changed_at = self.changed_at.max(other.changed_at);
    }

    #[must_use]
    pub fn top(&self) -> Attention {
        [
            (self.needs_you, Attention::NeedsYou),
            (self.failed, Attention::Failed),
            (self.done, Attention::Done),
            (self.bell, Attention::Bell),
            (self.working, Attention::Working),
        ]
        .into_iter()
        .find(|(count, _)| *count > 0)
        .map_or(Attention::None, |(_, attention)| attention)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSummary {
    pub session: SessionId,
    pub label: SessionLabel,
    pub windows: Vec<WindowSummary>,
    pub rollup: AttentionRollup,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DaemonClock {
    offset: i64,
}

impl DaemonClock {
    #[must_use]
    pub fn from_hello(server_clock: u64, local_unix: u64) -> Self {
        if server_clock == 0 {
            return Self::default();
        }
        let offset = i128::from(server_clock) - i128::from(local_unix);
        Self {
            offset: i64::try_from(offset).unwrap_or(if offset < 0 { i64::MIN } else { i64::MAX }),
        }
    }

    #[must_use]
    pub const fn now(self, local_unix: u64) -> u64 {
        local_unix.saturating_add_signed(self.offset)
    }
}

#[must_use]
pub fn pane_tag(kind: &PaneKindSnapshot) -> PaneTag {
    match kind {
        PaneKindSnapshot::Picker => PaneTag::Picker,
        PaneKindSnapshot::Terminal => PaneTag::Terminal,
        PaneKindSnapshot::Browser(_) => PaneTag::Browser,
        PaneKindSnapshot::Editor(_) => PaneTag::Editor,
        PaneKindSnapshot::Agent(agent) => PaneTag::Agent(agent.provider),
        PaneKindSnapshot::View(_) => PaneTag::View,
        PaneKindSnapshot::Other { .. } => PaneTag::Other,
    }
}

#[must_use]
pub fn pane_program(pane: &PaneSnapshot) -> Option<&'static Program> {
    match &pane.kind {
        PaneKindSnapshot::Terminal => resolve(pane.summary.process.iter().map(String::as_str)),
        PaneKindSnapshot::Agent(agent) => provider_program(agent.provider),
        _ => None,
    }
}

#[must_use]
pub fn provider_program(provider: AgentProvider) -> Option<&'static Program> {
    Program::by_id(match provider {
        AgentProvider::ClaudeCode => "claude",
        AgentProvider::Codex => "codex",
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneHeading {
    pub title: String,
    pub program: Option<&'static Program>,
}

#[must_use]
pub fn pane_heading(pane: &PaneSnapshot) -> PaneHeading {
    if !matches!(pane.kind, PaneKindSnapshot::Terminal) {
        return PaneHeading {
            title: pane_label(pane),
            program: None,
        };
    }
    let program = pane_program(pane);
    let title = program
        .map(|program| program.name.to_owned())
        .or_else(|| {
            pane.summary
                .process
                .first()
                .map(|entry| last_component(entry).to_owned())
        })
        .or_else(|| (!pane.summary.shell.is_empty()).then(|| pane.summary.shell.clone()))
        .unwrap_or_else(|| pane_label(pane));
    PaneHeading { title, program }
}

#[must_use]
pub fn pane_agent(pane: &PaneSnapshot) -> Option<AgentProvider> {
    match pane_program(pane)?.id {
        "claude" => Some(AgentProvider::ClaudeCode),
        "codex" => Some(AgentProvider::Codex),
        _ => None,
    }
}

#[must_use]
pub fn pane_attention(pane: &PaneSnapshot) -> Attention {
    let state = pane.status.as_ref().map(|status| status.state);
    match state {
        Some(PaneStatusState::Blocked) => Attention::NeedsYou,
        Some(PaneStatusState::Error) => Attention::Failed,
        Some(PaneStatusState::Working) => Attention::Working,
        _ if pane.summary.unseen => Attention::Done,
        _ if pane.bell => Attention::Bell,
        _ => Attention::None,
    }
}

#[must_use]
pub fn summarize_window(window: &WindowSnapshot) -> WindowSummary {
    let panes = ordered_panes(window);
    let active = window.panes.get(&window.active_pane);
    let headline = headline_pane(window, &panes).or(active);
    let title = window_title(window, headline, active);
    let detail = headline.map_or_else(
        || vec![part("Empty", Tone::Muted)],
        |pane| pane_detail(window, pane, &title),
    );
    let tiles = active
        .into_iter()
        .chain(
            panes
                .iter()
                .copied()
                .filter(|pane| pane.id != window.active_pane),
        )
        .map(pane_tile)
        .collect();
    WindowSummary {
        window: window.id,
        index: window.index,
        headline: headline.map_or(window.active_pane, |pane| pane.id),
        title,
        detail,
        changed_at: headline.map_or(0, |pane| pane.summary.changed_at),
        tiles,
        attention: panes
            .iter()
            .map(|pane| pane_attention(pane))
            .max()
            .unwrap_or_default(),
    }
}

fn pane_tile(pane: &PaneSnapshot) -> PaneTile {
    PaneTile {
        pane: pane.id,
        tag: pane_tag(&pane.kind),
        label: pane_label(pane),
        program: pane_program(pane),
        attention: pane_attention(pane),
    }
}

#[must_use]
pub fn summarize_chat(window: &WindowSnapshot) -> Option<WindowSummary> {
    let panes = ordered_panes(window);
    let runs_agent = |pane: &&PaneSnapshot| pane_agent(pane).is_some();
    let attention = panes
        .iter()
        .map(|pane| pane_attention(pane))
        .max()
        .unwrap_or_default();
    let agent = panes
        .iter()
        .copied()
        .filter(runs_agent)
        .find(|pane| pane.id == window.active_pane)
        .or_else(|| panes.iter().copied().find(runs_agent))?;
    let title = chat_title(window, agent);
    let detail = chat_detail(window, agent, &title);
    Some(WindowSummary {
        window: window.id,
        index: window.index,
        headline: agent.id,
        detail,
        title,
        changed_at: agent.summary.changed_at,
        tiles: std::iter::once(agent)
            .chain(panes.iter().copied().filter(|pane| pane.id != agent.id))
            .map(pane_tile)
            .collect(),
        attention,
    })
}

#[must_use]
pub fn summarize_chats<'a>(
    sessions: impl IntoIterator<Item = &'a SessionSnapshot>,
) -> (Vec<WindowSummary>, AttentionRollup) {
    let chats = sessions
        .into_iter()
        .flat_map(|session| &session.windows)
        .filter_map(summarize_chat)
        .collect::<Vec<_>>();
    let mut rollup = AttentionRollup::default();
    for chat in &chats {
        rollup.add(chat);
    }
    (chats, rollup)
}

const CHAT_SHORTCUTS: u8 = 9;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneEntry {
    pub face: PaneTile,
    pub title: String,
    pub detail: Vec<DetailPart>,
    pub changed_at: u64,
    pub chat: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowRow {
    pub session: SessionId,
    pub summary: WindowSummary,
    pub open_detail: Vec<DetailPart>,
    pub chat: bool,
    pub opens: bool,
    pub panes: Vec<PaneEntry>,
    pub shortcut: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pile {
    pub summary: WindowSummary,
    pub open_detail: Vec<DetailPart>,
    pub working: bool,
    pub faces: Vec<WindowId>,
    pub windows: Vec<WindowRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatParent {
    Window(WindowId),
    Pile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatRow {
    Window(WindowRow),
    Pile(Pile),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatMachine {
    pub rows: Vec<ChatRow>,
    pub rollup: AttentionRollup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatCurrent {
    pub row: usize,
    pub entry: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatHints {
    pub last_chat: BTreeMap<WindowId, PaneId>,
    pub last_plain: Option<WindowId>,
}

impl ChatHints {
    pub fn observe(&mut self, window: &WindowSnapshot) {
        if window.panes.get(&window.active_pane).is_some_and(is_chat) {
            self.last_chat.insert(window.id, window.active_pane);
        } else if chat_view_lists(window) && !window.panes.values().any(is_chat) {
            self.last_plain = Some(window.id);
        }
    }

    pub fn retain(&mut self, live: impl Fn(WindowId) -> bool) {
        self.last_chat.retain(|window, _| live(*window));
        self.last_plain = self.last_plain.filter(|window| live(*window));
    }
}

impl ChatMachine {
    #[must_use]
    pub fn current(&self, window: WindowId, pane: PaneId) -> Option<ChatCurrent> {
        self.rows
            .iter()
            .enumerate()
            .find_map(|(row, candidate)| match candidate {
                ChatRow::Window(found) => (found.summary.window == window).then(|| ChatCurrent {
                    row,
                    entry: found.panes.iter().position(|entry| entry.face.pane == pane),
                }),
                ChatRow::Pile(pile) => pile.faces.contains(&window).then(|| ChatCurrent {
                    row,
                    entry: pile
                        .windows
                        .iter()
                        .position(|found| found.summary.window == window),
                }),
            })
    }
}

#[must_use]
pub fn summarize_chat_machine<'a>(
    sessions: impl IntoIterator<Item = &'a SessionSnapshot>,
    hints: &ChatHints,
    open: impl Fn(ChatParent) -> bool,
) -> ChatMachine {
    let mut chats = Vec::new();
    let mut plain = Vec::new();
    for session in sessions {
        for window in session
            .windows
            .iter()
            .filter(|window| chat_view_lists(window))
        {
            let panes = ordered_panes(window);
            let hint = hints.last_chat.get(&window.id).copied();
            let opened = open(ChatParent::Window(window.id));
            if let Some(row) = chat_row(session.id, window, &panes, hint, opened) {
                chats.push(row);
            } else if let Some(found) = Plain::new(session.id, window, panes) {
                plain.push(found);
            }
        }
    }
    let mut rollup = AttentionRollup::default();
    for row in &chats {
        rollup.add(&row.summary);
    }
    let mut rows = chats.into_iter().map(ChatRow::Window).collect::<Vec<_>>();
    match plain.as_slice() {
        [] => {}
        [lone] => {
            let row = lone.row(open(ChatParent::Window(lone.window.id)));
            rollup.add(&row.summary);
            rows.push(ChatRow::Window(row));
        }
        _ => {
            let pile = pile(&plain, hints.last_plain, open(ChatParent::Pile));
            for found in &plain {
                rollup.count(found.attention(), found.front.summary.changed_at);
            }
            rows.push(ChatRow::Pile(pile));
        }
    }
    ChatMachine { rows, rollup }
}

pub fn number_chat_rows<'a>(machines: impl IntoIterator<Item = &'a mut ChatMachine>) {
    let rows = machines
        .into_iter()
        .flat_map(|machine| &mut machine.rows)
        .filter_map(|row| match row {
            ChatRow::Window(row) if row.chat => Some(row),
            _ => None,
        });
    for (row, shortcut) in rows.zip(0..CHAT_SHORTCUTS) {
        row.shortcut = Some(shortcut);
    }
}

#[must_use]
pub fn chat_view_lists(window: &WindowSnapshot) -> bool {
    window
        .panes
        .values()
        .any(|pane| !matches!(pane.kind, PaneKindSnapshot::Picker))
}

#[must_use]
pub fn pane_entry(window: &WindowSnapshot, pane: &PaneSnapshot) -> PaneEntry {
    let chat = is_chat(pane);
    let (title, detail) = if chat {
        let title = chat_title(window, pane);
        let detail = chat_detail(window, pane, &title);
        (title, detail)
    } else {
        (plain_title(pane), own_line(pane))
    };
    PaneEntry {
        face: pane_tile(pane),
        title,
        detail,
        changed_at: pane.summary.changed_at,
        chat,
    }
}

type Alert = (Attention, DetailPart);

fn is_chat(pane: &PaneSnapshot) -> bool {
    pane_agent(pane).is_some()
}

const fn needs_user(attention: Attention) -> bool {
    matches!(attention, Attention::NeedsYou | Attention::Failed)
}

fn chat_row(
    session: SessionId,
    window: &WindowSnapshot,
    panes: &[&PaneSnapshot],
    hint: Option<PaneId>,
    open: bool,
) -> Option<WindowRow> {
    let chats = panes
        .iter()
        .copied()
        .filter(|pane| is_chat(pane))
        .collect::<Vec<_>>();
    let pick = |id: PaneId| chats.iter().copied().find(|pane| pane.id == id);
    let front = pick(window.active_pane)
        .or_else(|| hint.and_then(pick))
        .or_else(|| chats.first().copied())?;
    let title = chat_title(window, front);
    let behind = loudest(
        chats.iter().copied().filter(|pane| pane.id != front.id),
        needs_user,
    );
    let open_detail = chat_detail(window, front, &title);
    let detail = match behind {
        Some((_, word, pane)) if !needs_user(pane_attention(front)) => vec![
            word,
            part(format!(" · {}", chat_title(window, pane)), Tone::Muted),
        ],
        _ => open_detail.clone(),
    };
    let faces = std::iter::once(front)
        .chain(chats.iter().copied().filter(|pane| pane.id != front.id))
        .chain(panes.iter().copied().filter(|pane| !is_chat(pane)))
        .map(pane_tile)
        .collect();
    Some(WindowRow {
        session,
        summary: row_summary(window, panes, front, title, detail, faces),
        open_detail,
        chat: true,
        opens: panes.len() > 1,
        panes: if open {
            pane_entries(window, panes)
        } else {
            Vec::new()
        },
        shortcut: None,
    })
}

struct Plain<'a> {
    session: SessionId,
    window: &'a WindowSnapshot,
    panes: Vec<&'a PaneSnapshot>,
    front: &'a PaneSnapshot,
}

impl<'a> Plain<'a> {
    fn new(
        session: SessionId,
        window: &'a WindowSnapshot,
        panes: Vec<&'a PaneSnapshot>,
    ) -> Option<Self> {
        let front = window
            .panes
            .get(&window.active_pane)
            .or_else(|| panes.first().copied())?;
        Some(Self {
            session,
            window,
            panes,
            front,
        })
    }

    fn title(&self) -> String {
        let name = self.window.name.trim();
        if self.window.automatic_rename || name.is_empty() {
            plain_title(self.front)
        } else {
            name.to_owned()
        }
    }

    fn detail(&self) -> Vec<DetailPart> {
        let behind = loudest(
            self.panes
                .iter()
                .copied()
                .filter(|pane| pane.id != self.front.id),
            |_| true,
        );
        match behind {
            Some((_, word, pane)) if alert(self.front).is_none() => {
                vec![word, part(format!(" · {}", plain_title(pane)), Tone::Muted)]
            }
            _ => own_line(self.front),
        }
    }

    fn alert(&self) -> Option<Alert> {
        loudest(self.panes.iter().copied(), |_| true).map(|(attention, word, _)| (attention, word))
    }

    fn attention(&self) -> Attention {
        self.panes
            .iter()
            .map(|pane| pane_attention(pane))
            .max()
            .unwrap_or_default()
    }

    fn row(&self, open: bool) -> WindowRow {
        let faces = std::iter::once(self.front)
            .chain(
                self.panes
                    .iter()
                    .copied()
                    .filter(|pane| pane.id != self.front.id),
            )
            .map(pane_tile)
            .collect();
        WindowRow {
            session: self.session,
            summary: row_summary(
                self.window,
                &self.panes,
                self.front,
                self.title(),
                self.detail(),
                faces,
            ),
            open_detail: own_line(self.front),
            chat: false,
            opens: self.panes.len() > 1,
            panes: if open {
                pane_entries(self.window, &self.panes)
            } else {
                Vec::new()
            },
            shortcut: None,
        }
    }
}

fn pile(windows: &[Plain<'_>], last: Option<WindowId>, open: bool) -> Pile {
    let front = last
        .and_then(|last| windows.iter().position(|plain| plain.window.id == last))
        .unwrap_or_default();
    let lead = &windows[front];
    let order = std::iter::once(front).chain((0..windows.len()).filter(|index| *index != front));
    let tiles = order
        .clone()
        .map(|index| PaneTile {
            attention: windows[index].attention(),
            ..pane_tile(windows[index].front)
        })
        .collect();
    let behind = windows
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != front)
        .filter_map(|(index, plain)| {
            let (attention, word) = plain.alert()?;
            Some(((attention, Reverse(index)), word, plain))
        })
        .max_by_key(|(rank, ..)| *rank);
    let detail = match behind {
        Some((_, word, plain)) if lead.alert().is_none() => {
            vec![word, part(format!(" · {}", plain.title()), Tone::Muted)]
        }
        _ => lead.detail(),
    };
    let summary = WindowSummary {
        window: lead.window.id,
        index: lead.window.index,
        headline: lead.front.id,
        title: lead.title(),
        detail,
        changed_at: lead.front.summary.changed_at,
        tiles,
        attention: windows
            .iter()
            .map(Plain::attention)
            .max()
            .unwrap_or_default(),
    };
    Pile {
        summary,
        open_detail: own_line(lead.front),
        working: pane_attention(lead.front) == Attention::Working,
        faces: order.map(|index| windows[index].window.id).collect(),
        windows: if open {
            windows.iter().map(|plain| plain.row(false)).collect()
        } else {
            Vec::new()
        },
    }
}

fn row_summary(
    window: &WindowSnapshot,
    panes: &[&PaneSnapshot],
    front: &PaneSnapshot,
    title: String,
    detail: Vec<DetailPart>,
    tiles: Vec<PaneTile>,
) -> WindowSummary {
    WindowSummary {
        window: window.id,
        index: window.index,
        headline: front.id,
        title,
        detail,
        changed_at: front.summary.changed_at,
        tiles,
        attention: panes
            .iter()
            .map(|pane| pane_attention(pane))
            .max()
            .unwrap_or_default(),
    }
}

fn pane_entries(window: &WindowSnapshot, panes: &[&PaneSnapshot]) -> Vec<PaneEntry> {
    if panes.len() < 2 {
        return Vec::new();
    }
    panes.iter().map(|pane| pane_entry(window, pane)).collect()
}

fn loudest<'a>(
    panes: impl IntoIterator<Item = &'a PaneSnapshot>,
    keep: impl Fn(Attention) -> bool,
) -> Option<(Attention, DetailPart, &'a PaneSnapshot)> {
    panes
        .into_iter()
        .enumerate()
        .filter_map(|(position, pane)| {
            let (attention, word) = alert(pane).filter(|(attention, _)| keep(*attention))?;
            Some((attention, Reverse(position), word, pane))
        })
        .max_by_key(|(attention, position, ..)| (*attention, *position))
        .map(|(attention, _, word, pane)| (attention, word, pane))
}

fn alert(pane: &PaneSnapshot) -> Option<Alert> {
    let attention = pane_attention(pane);
    if needs_user(attention) {
        return status_detail(pane)?
            .into_iter()
            .next()
            .map(|word| (attention, word));
    }
    let failed = match (pane.dead, pane.dead_status, pane.summary.command) {
        (true, Some(status), _) if status != 0 => Some(format!("Exited {status}")),
        (false, _, Some(ShellCommand::Finished(code)))
            if code != 0 && matches!(pane.kind, PaneKindSnapshot::Terminal) =>
        {
            Some(format!("Exit {code}"))
        }
        _ => None,
    };
    if let Some(text) = failed {
        return Some((Attention::Failed, part(text, Tone::Danger)));
    }
    (attention == Attention::Bell).then(|| (Attention::Bell, part("Bell", Tone::Normal)))
}

fn own_line(pane: &PaneSnapshot) -> Vec<DetailPart> {
    let location = location(pane);
    match alert(pane) {
        Some((_, word)) => std::iter::once(word)
            .chain(location.map(|location| part(format!(" · {location}"), Tone::Muted)))
            .collect(),
        None => location
            .map(|location| part(location, Tone::Muted))
            .into_iter()
            .collect(),
    }
}

fn plain_title(pane: &PaneSnapshot) -> String {
    match &pane.kind {
        PaneKindSnapshot::Terminal => [
            Some(pane.title.trim()),
            pane.summary
                .process
                .first()
                .map(|entry| last_component(entry)),
            Some(pane.summary.shell.trim()),
        ]
        .into_iter()
        .flatten()
        .find(|text| !text.is_empty())
        .map_or_else(|| pane_label(pane), str::to_owned),
        PaneKindSnapshot::Browser(browser) => browser_host(browser.url()),
        _ => pane_label(pane),
    }
}

fn chat_title(window: &WindowSnapshot, pane: &PaneSnapshot) -> String {
    window_title(window, Some(pane), Some(pane))
}

fn chat_detail(window: &WindowSnapshot, pane: &PaneSnapshot, title: &str) -> Vec<DetailPart> {
    let quiet = pane.status.is_none() && !pane.summary.unseen && !pane.dead;
    match (quiet, location(pane), pane_program(pane)) {
        (true, None, Some(program)) => vec![part(program.name, Tone::Muted)],
        _ => pane_detail(window, pane, title),
    }
}

#[must_use]
pub fn summarize_session(session: &SessionSnapshot) -> SessionSummary {
    let windows = session
        .windows
        .iter()
        .map(summarize_window)
        .collect::<Vec<_>>();
    let mut rollup = AttentionRollup::default();
    for window in &windows {
        rollup.add(window);
    }
    SessionSummary {
        session: session.id,
        label: session_label(session),
        windows,
        rollup,
    }
}

#[must_use]
pub fn summarize_host(snapshot: &MuxSnapshot) -> (Vec<SessionSummary>, AttentionRollup) {
    summarize_sessions(&snapshot.sessions)
}

#[must_use]
pub fn summarize_sessions<'a>(
    sessions: impl IntoIterator<Item = &'a SessionSnapshot>,
) -> (Vec<SessionSummary>, AttentionRollup) {
    let sessions = sessions
        .into_iter()
        .map(summarize_session)
        .collect::<Vec<_>>();
    let mut rollup = AttentionRollup::default();
    for session in &sessions {
        rollup.merge(&session.rollup);
    }
    (sessions, rollup)
}

#[must_use]
pub fn short_age(seconds: u64) -> String {
    match seconds {
        0..60 => "now".to_owned(),
        60..3_600 => format!("{}m", seconds / 60),
        3_600..86_400 => format!("{}h", seconds / 3_600),
        _ => format!("{}d", seconds / 86_400),
    }
}

#[must_use]
pub fn age_label(changed_at: u64, daemon_now: u64) -> Option<String> {
    (changed_at != 0).then(|| short_age(daemon_now.saturating_sub(changed_at)))
}

#[must_use]
pub fn hold_detail(window: &WindowSnapshot, daemon_now: u64) -> String {
    let panes = ordered_panes(window);
    let Some(pane) =
        headline_pane(window, &panes).or_else(|| window.panes.get(&window.active_pane))
    else {
        return String::new();
    };
    let work = pane
        .status
        .as_ref()
        .filter(|status| status.state == PaneStatusState::Working)
        .and_then(|_| status_detail(pane))
        .map(|parts| parts.into_iter().map(|part| part.text).collect::<String>());
    [
        work,
        location(pane),
        age_label(pane.summary.changed_at, daemon_now),
    ]
    .into_iter()
    .flatten()
    .filter(|text| !text.is_empty())
    .collect::<Vec<_>>()
    .join(" · ")
}

fn headline_pane<'a>(
    window: &WindowSnapshot,
    panes: &[&'a PaneSnapshot],
) -> Option<&'a PaneSnapshot> {
    panes
        .iter()
        .enumerate()
        .filter_map(|(position, pane)| {
            let attention = pane_attention(pane);
            (!matches!(attention, Attention::None | Attention::Bell)).then_some((
                (
                    attention,
                    pane.id == window.active_pane,
                    pane.summary.changed_at,
                    Reverse(position),
                ),
                *pane,
            ))
        })
        .max_by_key(|(rank, _)| *rank)
        .map(|(_, pane)| pane)
}

fn agent_like(pane: &PaneSnapshot) -> bool {
    matches!(pane.kind, PaneKindSnapshot::Agent(_))
        || pane.status.is_some()
        || !pane.summary.name.trim().is_empty()
}

fn agent_title(pane: &PaneSnapshot) -> Option<String> {
    [
        pane.status.as_ref().map(|status| status.title.trim()),
        Some(pane.summary.name.trim()),
    ]
    .into_iter()
    .flatten()
    .find(|text| !text.is_empty())
    .map(str::to_owned)
}

fn window_title(
    window: &WindowSnapshot,
    headline: Option<&PaneSnapshot>,
    active: Option<&PaneSnapshot>,
) -> String {
    let pane = headline.filter(|pane| agent_like(pane)).or(active);
    pane.and_then(agent_title)
        .or_else(|| {
            let name = window.name.trim();
            (!window.automatic_rename && !name.is_empty()).then(|| name.to_owned())
        })
        .or_else(|| pane.map(pane_label))
        .unwrap_or_else(|| window.name.clone())
}

fn part(text: impl Into<String>, tone: Tone) -> DetailPart {
    DetailPart {
        text: text.into(),
        tone,
    }
}

fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn labelled(label: &str, tone: Tone, message: &str) -> Vec<DetailPart> {
    let mut parts = vec![part(label, tone)];
    if !message.is_empty() {
        parts.push(part(format!(" · {message}"), Tone::Muted));
    }
    parts
}

fn or_else(text: String, fallback: &str) -> String {
    if text.is_empty() {
        fallback.to_owned()
    } else {
        text
    }
}

fn status_detail(pane: &PaneSnapshot) -> Option<Vec<DetailPart>> {
    let status = pane.status.as_ref()?;
    let message = single_line(&status.message);
    match (status.state, status.kind) {
        (PaneStatusState::Blocked, Some(PaneStatusKind::Permission)) => {
            Some(labelled("Needs approval", Tone::Warning, &message))
        }
        (PaneStatusState::Blocked, Some(PaneStatusKind::Question)) if message.is_empty() => {
            Some(vec![part("Has a question", Tone::Warning)])
        }
        (PaneStatusState::Blocked, Some(PaneStatusKind::Question)) => {
            Some(labelled("Question", Tone::Warning, &message))
        }
        (PaneStatusState::Blocked, _) => Some(labelled("Needs you", Tone::Warning, &message)),
        (PaneStatusState::Error, _) => Some(labelled("Failed", Tone::Danger, &message)),
        (PaneStatusState::Done, _) => {
            let tone = if pane.summary.unseen {
                Tone::Normal
            } else {
                Tone::Muted
            };
            Some(labelled("Done", tone, &message))
        }
        (PaneStatusState::Working, _) => {
            let text = if message.is_empty() {
                or_else(single_line(&status.title), "Working")
            } else {
                message
            };
            let mut parts = vec![part(text, Tone::Muted)];
            if let Some(progress) = status.progress {
                parts.push(part(format!(" {progress}%"), Tone::Muted));
            }
            Some(parts)
        }
        (PaneStatusState::Idle, _) if pane.summary.unseen => Some(vec![part("Done", Tone::Normal)]),
        (PaneStatusState::Idle, _) if !message.is_empty() => Some(vec![part(message, Tone::Muted)]),
        (PaneStatusState::Idle, _) => None,
    }
}

#[must_use]
pub fn pane_detail(window: &WindowSnapshot, pane: &PaneSnapshot, title: &str) -> Vec<DetailPart> {
    if let Some(parts) = status_detail(pane) {
        return parts;
    }
    if pane.status.is_none() && pane.summary.unseen {
        return vec![part("Done", Tone::Normal)];
    }
    if pane.dead {
        return match pane.dead_status {
            Some(status) if status != 0 => vec![part(format!("Exited {status}"), Tone::Danger)],
            _ => vec![part("Exited", Tone::Muted)],
        };
    }
    let location = location(pane);
    match &pane.kind {
        PaneKindSnapshot::Terminal => {
            if let Some(ShellCommand::Finished(code)) = pane.summary.command
                && code != 0
            {
                let mut parts = vec![part(format!("Exit {code}"), Tone::Danger)];
                if let Some(location) = location {
                    parts.push(part(format!(" · {location}"), Tone::Muted));
                }
                return parts;
            }
            let name = window.name.trim();
            let text = location.unwrap_or_else(|| {
                if name.is_empty() || name == title {
                    "Shell".to_owned()
                } else {
                    name.to_owned()
                }
            });
            vec![part(text, Tone::Muted)]
        }
        PaneKindSnapshot::Browser(browser) => vec![part(browser_host(browser.url()), Tone::Muted)],
        PaneKindSnapshot::Editor(editor) => {
            let file = editor
                .path
                .as_deref()
                .and_then(|path| Path::new(path).file_name())
                .map(|name| name.to_string_lossy().into_owned());
            vec![part(
                file.or(location).unwrap_or_else(|| "Editor".to_owned()),
                Tone::Muted,
            )]
        }
        PaneKindSnapshot::Picker => vec![part("New pane", Tone::Muted)],
        PaneKindSnapshot::Agent(agent) => vec![part(
            location.unwrap_or_else(|| agent.provider.label().to_owned()),
            Tone::Muted,
        )],
        PaneKindSnapshot::View(view) => vec![part(view.content.kind_name(), Tone::Muted)],
        PaneKindSnapshot::Other { .. } => vec![part("Needs a newer zz", Tone::Muted)],
    }
}

fn location(pane: &PaneSnapshot) -> Option<String> {
    let summary = &pane.summary;
    if !summary.cwd.is_empty() {
        let folder = last_component(&summary.cwd);
        return Some(if summary.branch.is_empty() {
            folder.to_owned()
        } else {
            format!("{folder} · {}", summary.branch)
        });
    }
    let fallback = match &pane.kind {
        PaneKindSnapshot::Agent(agent) => agent.cwd.as_deref()?.to_string_lossy().into_owned(),
        PaneKindSnapshot::Editor(editor) => editor.cwd.clone(),
        _ => return None,
    };
    let folder = last_component(&fallback);
    (!folder.is_empty()).then(|| folder.to_owned())
}

fn browser_host(url: &str) -> String {
    let url = url.trim();
    if let Ok(parsed) = Url::parse(url)
        && let Some(host) = parsed.host_str()
    {
        let host = host.strip_prefix("www.").unwrap_or(host);
        return parsed
            .port()
            .map_or_else(|| host.to_owned(), |port| format!("{host}:{port}"));
    }
    if url.is_empty() || url.starts_with("about:") {
        "New tab".to_owned()
    } else {
        url.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use zz_protocol::{
        AgentDescriptor, BrowserDescriptor, EditorDescriptor, LayoutNode, PaneStatus, PaneSummary,
        SplitId,
    };

    use super::*;

    fn pane(id: u64, kind: PaneKindSnapshot) -> PaneSnapshot {
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
            summary: PaneSummary::default(),
        }
    }

    fn terminal(id: u64, title: &str) -> PaneSnapshot {
        PaneSnapshot {
            title: title.to_owned(),
            ..pane(id, PaneKindSnapshot::Terminal)
        }
    }

    #[test]
    fn a_terminal_is_headed_by_its_program_then_its_command_then_its_title() {
        let heading = |title: &str, process: &[&str]| {
            let mut pane = terminal(1, title);
            pane.summary.process = process.iter().map(|entry| (*entry).to_owned()).collect();
            let heading = pane_heading(&pane);
            (heading.title, heading.program.map(|program| program.id))
        };
        assert_eq!(
            heading("vim ~/dev/zz", &["/opt/homebrew/bin/nvim", "nvim"]),
            ("Neovim".to_owned(), Some("neovim"))
        );
        assert_eq!(
            heading("~/dev/zz", &["/usr/local/bin/frobnicate", "frobnicate"]),
            ("frobnicate".to_owned(), None)
        );
        assert_eq!(heading("bash", &[]), ("bash".to_owned(), None));
        let mut fish = terminal(3, "~/d/a/zebrilus");
        fish.summary.shell = "fish".to_owned();
        assert_eq!(pane_heading(&fish).title, "fish");
        let mut chat = agent(2, "Fix the build");
        chat.kind = PaneKindSnapshot::Agent(AgentDescriptor::default());
        assert_eq!(pane_heading(&chat).title, "Fix the build");
    }

    fn agent(id: u64, title: &str) -> PaneSnapshot {
        let mut pane = terminal(id, title);
        pane.summary.process = vec!["/usr/local/bin/claude".to_owned()];
        pane
    }

    fn status(state: PaneStatusState, kind: Option<PaneStatusKind>, message: &str) -> PaneStatus {
        PaneStatus {
            state,
            kind,
            progress: None,
            app: String::new(),
            title: String::new(),
            message: message.to_owned(),
        }
    }

    fn with_status(mut pane: PaneSnapshot, status: PaneStatus) -> PaneSnapshot {
        pane.status = Some(status);
        pane
    }

    fn window(id: u64, panes: Vec<PaneSnapshot>) -> WindowSnapshot {
        let ids = panes.iter().map(|pane| pane.id).collect::<Vec<_>>();
        let layout = ids
            .iter()
            .rev()
            .map(|id| LayoutNode::Pane(*id))
            .reduce(|second, first| LayoutNode::Split {
                id: SplitId(0),
                axis: zz_protocol::Axis::Horizontal,
                ratio: 0.5,
                first: Box::new(first),
                second: Box::new(second),
            })
            .unwrap_or(LayoutNode::Empty);
        WindowSnapshot {
            id: WindowId(id),
            index: 0,
            name: "zsh".to_owned(),
            automatic_rename: true,
            active_pane: ids.first().copied().unwrap_or_default(),
            zoomed_pane: None,
            layout,
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

    fn texts(summary: &WindowSummary) -> Vec<(&str, Tone)> {
        summary
            .detail
            .iter()
            .map(|part| (part.text.as_str(), part.tone))
            .collect()
    }

    #[test]
    fn title_prefers_agent_title_then_rename_then_pane_label() {
        let renamed = |mut window: WindowSnapshot| {
            window.name = "Reviews".to_owned();
            window.automatic_rename = false;
            window
        };
        let named = |mut pane: PaneSnapshot, name: &str| {
            pane.summary.name = name.to_owned();
            pane
        };
        let titled = |title: &str| {
            let mut status = status(PaneStatusState::Working, None, "");
            status.title = title.to_owned();
            status
        };
        for (window, expected) in [
            (
                renamed(window(
                    1,
                    vec![with_status(agent(1, "agent"), titled("Plan"))],
                )),
                "Plan",
            ),
            (
                renamed(window(1, vec![named(terminal(1, "claude"), "reviewer")])),
                "reviewer",
            ),
            (renamed(window(1, vec![agent(1, "agent")])), "Reviews"),
            (renamed(window(1, vec![terminal(1, "vim")])), "Reviews"),
            (window(1, vec![terminal(1, "vim")]), "vim"),
            (window(1, vec![agent(1, "agent")]), "agent"),
            (
                window(1, vec![with_status(terminal(1, "claude"), titled(""))]),
                "claude",
            ),
        ] {
            assert_eq!(summarize_window(&window).title, expected);
        }

        let mut window = window(1, vec![terminal(1, "vim"), agent(2, "Review PR")]);
        assert_eq!(summarize_window(&window).title, "vim");
        window.panes.get_mut(&PaneId(2)).unwrap().status =
            Some(status(PaneStatusState::Working, None, ""));
        assert_eq!(summarize_window(&window).title, "Review PR");

        let claude = with_status(
            terminal(2, "Refactor parser"),
            status(
                PaneStatusState::Blocked,
                Some(PaneStatusKind::Permission),
                "",
            ),
        );
        window.panes.insert(PaneId(2), claude);
        let summary = summarize_window(&window);
        assert_eq!(summary.headline, PaneId(2));
        assert_eq!(summary.title, "Refactor parser");
        assert_eq!(texts(&summary)[0], ("Needs approval", Tone::Warning));
    }

    #[test]
    fn line_two_follows_the_priority_table() {
        let mut progress = status(PaneStatusState::Working, None, "");
        progress.progress = Some(40);
        let mut titled = status(PaneStatusState::Working, None, "");
        titled.title = "Compiling".to_owned();
        let unseen = |mut pane: PaneSnapshot| {
            pane.summary.unseen = true;
            pane
        };
        let dead = |status| PaneSnapshot {
            dead: true,
            dead_status: status,
            ..terminal(1, "zsh")
        };
        let finished = |code, cwd: &str, branch: &str| {
            let mut pane = terminal(1, "zsh");
            pane.summary.command = Some(ShellCommand::Finished(code));
            pane.summary.cwd = cwd.to_owned();
            pane.summary.branch = branch.to_owned();
            pane
        };
        let browser = |url: &str| {
            pane(
                1,
                PaneKindSnapshot::Browser(BrowserDescriptor::single(
                    url.to_owned(),
                    "default".to_owned(),
                )),
            )
        };
        let editor = |path: Option<&str>| {
            pane(
                1,
                PaneKindSnapshot::Editor(EditorDescriptor {
                    path: path.map(str::to_owned),
                    cwd: "/work/zz".to_owned(),
                }),
            )
        };
        let located = |mut pane: PaneSnapshot, cwd: &str, branch: &str| {
            pane.summary.cwd = cwd.to_owned();
            pane.summary.branch = branch.to_owned();
            pane
        };
        let cases: Vec<(PaneSnapshot, Vec<(&str, Tone)>)> = vec![
            (
                with_status(
                    agent(1, "x"),
                    status(
                        PaneStatusState::Blocked,
                        Some(PaneStatusKind::Permission),
                        "Run  cargo\ntest",
                    ),
                ),
                vec![
                    ("Needs approval", Tone::Warning),
                    (" · Run cargo test", Tone::Muted),
                ],
            ),
            (
                with_status(
                    agent(1, "x"),
                    status(
                        PaneStatusState::Blocked,
                        Some(PaneStatusKind::Question),
                        "Which branch?",
                    ),
                ),
                vec![
                    ("Question", Tone::Warning),
                    (" · Which branch?", Tone::Muted),
                ],
            ),
            (
                with_status(
                    agent(1, "x"),
                    status(PaneStatusState::Blocked, Some(PaneStatusKind::Question), ""),
                ),
                vec![("Has a question", Tone::Warning)],
            ),
            (
                with_status(
                    terminal(1, "x"),
                    status(PaneStatusState::Blocked, Some(PaneStatusKind::Auth), ""),
                ),
                vec![("Needs you", Tone::Warning)],
            ),
            (
                with_status(
                    terminal(1, "x"),
                    status(PaneStatusState::Error, None, "rate limited"),
                ),
                vec![("Failed", Tone::Danger), (" · rate limited", Tone::Muted)],
            ),
            (
                with_status(
                    agent(1, "x"),
                    status(PaneStatusState::Done, None, "All green"),
                ),
                vec![("Done", Tone::Muted), (" · All green", Tone::Muted)],
            ),
            (
                unseen(with_status(
                    agent(1, "x"),
                    status(PaneStatusState::Done, None, "All green"),
                )),
                vec![("Done", Tone::Normal), (" · All green", Tone::Muted)],
            ),
            (
                with_status(agent(1, "x"), status(PaneStatusState::Done, None, "")),
                vec![("Done", Tone::Muted)],
            ),
            (
                with_status(agent(1, "x"), progress),
                vec![("Working", Tone::Muted), (" 40%", Tone::Muted)],
            ),
            (
                with_status(agent(1, "x"), titled),
                vec![("Compiling", Tone::Muted)],
            ),
            (
                unseen(with_status(
                    terminal(1, "x"),
                    status(PaneStatusState::Idle, None, "old"),
                )),
                vec![("Done", Tone::Normal)],
            ),
            (
                with_status(
                    agent(1, "x"),
                    status(PaneStatusState::Idle, None, "Last reply"),
                ),
                vec![("Last reply", Tone::Muted)],
            ),
            (dead(Some(2)), vec![("Exited 2", Tone::Danger)]),
            (dead(Some(0)), vec![("Exited", Tone::Muted)]),
            (
                finished(1, "~/dev/zz", "main"),
                vec![("Exit 1", Tone::Danger), (" · zz · main", Tone::Muted)],
            ),
            (finished(127, "", ""), vec![("Exit 127", Tone::Danger)]),
            (
                finished(1, "~\\dev\\zz", "main"),
                vec![("Exit 1", Tone::Danger), (" · zz · main", Tone::Muted)],
            ),
            (
                located(terminal(1, "pwsh"), "C:\\Users\\me\\proj\\", ""),
                vec![("proj", Tone::Muted)],
            ),
            (
                browser("https://www.example.com:8443/path?q=1"),
                vec![("example.com:8443", Tone::Muted)],
            ),
            (browser("about:blank"), vec![("New tab", Tone::Muted)]),
            (
                editor(Some("/work/zz/src/main.rs")),
                vec![("main.rs", Tone::Muted)],
            ),
            (editor(None), vec![("zz", Tone::Muted)]),
            (
                pane(1, PaneKindSnapshot::Picker),
                vec![("New pane", Tone::Muted)],
            ),
            (
                located(finished(0, "", ""), "~", "main"),
                vec![("~ · main", Tone::Muted)],
            ),
            (
                located(agent(1, "x"), "/work/zz/", ""),
                vec![("zz", Tone::Muted)],
            ),
            (terminal(1, "zsh"), vec![("Shell", Tone::Muted)]),
            (terminal(1, "vim notes.md"), vec![("zsh", Tone::Muted)]),
            (agent(1, "x"), vec![("zsh", Tone::Muted)]),
        ];
        for (pane, expected) in cases {
            let summary = summarize_window(&window(1, vec![pane]));
            assert_eq!(texts(&summary), expected);
        }
    }

    #[test]
    fn every_kind_gets_a_second_line() {
        for kind in [
            PaneKindSnapshot::Picker,
            PaneKindSnapshot::Terminal,
            PaneKindSnapshot::Browser(BrowserDescriptor::single(
                String::new(),
                "default".to_owned(),
            )),
            PaneKindSnapshot::Editor(EditorDescriptor {
                path: None,
                cwd: "/".to_owned(),
            }),
        ] {
            let summary = summarize_window(&window(1, vec![pane(1, kind.clone())]));
            assert!(!summary.title.is_empty(), "{kind:?}");
            assert!(!summary.detail.is_empty(), "{kind:?}");
            assert!(
                summary.detail.iter().all(|part| !part.text.is_empty()),
                "{kind:?}"
            );
        }
        let mut empty = window(1, Vec::new());
        empty.active_pane = PaneId(9);
        let summary = summarize_window(&empty);
        assert!(!summary.detail.is_empty());
        assert_eq!(summary.title, "zsh");
    }

    #[test]
    fn needs_you_takes_the_headline_from_the_active_pane() {
        let blocked = with_status(
            agent(2, "Deploy"),
            status(
                PaneStatusState::Blocked,
                Some(PaneStatusKind::Permission),
                "rm",
            ),
        );
        let working = with_status(
            agent(1, "Build"),
            status(PaneStatusState::Working, None, "compiling"),
        );
        let summary = summarize_window(&window(1, vec![working, blocked]));
        assert_eq!(summary.headline, PaneId(2));
        assert_eq!(summary.title, "Deploy");
        assert_eq!(summary.attention, Attention::NeedsYou);
        assert_eq!(texts(&summary)[0], ("Needs approval", Tone::Warning));
    }

    #[test]
    fn a_hold_sheet_says_the_work_then_where_and_when() {
        let mut testing = status(PaneStatusState::Working, None, "Testing");
        testing.progress = Some(62);
        let mut pane = with_status(agent(1, "Fix pane stack"), testing);
        pane.summary.cwd = "/Users/fabrico/dev/zebrilus".to_owned();
        pane.summary.branch = "pane-stack".to_owned();
        pane.summary.changed_at = 1_000;
        assert_eq!(
            hold_detail(&window(1, vec![pane.clone()]), 1_030),
            "Testing 62% · zebrilus · pane-stack · now"
        );
        pane.status = None;
        assert_eq!(
            hold_detail(&window(1, vec![pane]), 1_000 + 7_200),
            "zebrilus · pane-stack · 2h"
        );
    }

    #[test]
    fn ties_go_to_the_active_pane() {
        let working = |id, changed_at| {
            let mut pane = with_status(agent(id, "x"), status(PaneStatusState::Working, None, ""));
            pane.summary.changed_at = changed_at;
            pane
        };
        let mut window = window(1, vec![working(1, 10), working(2, 50), working(3, 50)]);
        assert_eq!(summarize_window(&window).headline, PaneId(1));
        window.active_pane = PaneId(9);
        assert_eq!(summarize_window(&window).headline, PaneId(2));
        window.panes.get_mut(&PaneId(3)).unwrap().summary.changed_at = 60;
        assert_eq!(summarize_window(&window).headline, PaneId(3));
    }

    #[test]
    fn tiles_wear_the_program_running_in_the_pane() {
        let running = |id, process: &[&str]| {
            let mut pane = terminal(id, "x");
            pane.summary.process = process.iter().map(|entry| (*entry).to_owned()).collect();
            pane
        };
        let mut browser = running(4, &["/opt/homebrew/bin/nvim"]);
        browser.kind = PaneKindSnapshot::Browser(BrowserDescriptor::single(
            String::new(),
            "default".to_owned(),
        ));
        let summary = summarize_window(&window(
            1,
            vec![
                running(
                    1,
                    &[
                        "/home/me/bin/claude.py",
                        "/home/me/bin/claude",
                        "/usr/bin/python3",
                        "python3",
                    ],
                ),
                running(2, &["codex", "sleep", "/bin/sleep"]),
                running(3, &[]),
                browser,
                agent(5, "x"),
                running(6, &["/usr/local/bin/unknown-tool", "unknown-tool"]),
            ],
        ));
        assert_eq!(
            summary
                .tiles
                .iter()
                .map(|tile| tile.program.map(|program| program.id))
                .collect::<Vec<_>>(),
            [
                Some("claude"),
                Some("codex"),
                None,
                None,
                Some("claude"),
                None
            ]
        );
    }

    #[test]
    fn a_bell_never_takes_line_two() {
        let mut belled = terminal(2, "make");
        belled.bell = true;
        belled.summary.changed_at = 99;
        let summary = summarize_window(&window(1, vec![terminal(1, "vim"), belled]));
        assert_eq!(summary.headline, PaneId(1));
        assert_eq!(summary.attention, Attention::Bell);
        assert_eq!(summary.changed_at, 0);
    }

    #[test]
    fn age_follows_the_headline_pane() {
        let mut active = terminal(1, "vim");
        active.summary.changed_at = 500;
        let mut done = with_status(agent(2, "x"), status(PaneStatusState::Done, None, ""));
        done.summary.changed_at = 100;
        done.summary.unseen = true;
        let summary = summarize_window(&window(1, vec![active.clone(), done]));
        assert_eq!(summary.headline, PaneId(2));
        assert_eq!(summary.changed_at, 100);
        assert_eq!(age_label(summary.changed_at, 220), Some("2m".to_owned()));

        let summary = summarize_window(&window(1, vec![active]));
        assert_eq!(summary.changed_at, 500);
        assert_eq!(age_label(0, 220), None);
    }

    #[test]
    fn tiles_put_the_active_pane_first() {
        let browser = pane(
            3,
            PaneKindSnapshot::Browser(BrowserDescriptor::single(
                "https://example.com".to_owned(),
                "default".to_owned(),
            )),
        );
        let mut window = window(1, vec![terminal(1, "zsh"), agent(2, "x"), browser]);
        window.active_pane = PaneId(2);
        window
            .panes
            .insert(PaneId(4), pane(4, PaneKindSnapshot::Picker));
        assert_eq!(
            summarize_window(&window)
                .tiles
                .into_iter()
                .map(|tile| tile.tag)
                .collect::<Vec<_>>(),
            vec![
                PaneTag::Terminal,
                PaneTag::Terminal,
                PaneTag::Browser,
                PaneTag::Picker,
            ]
        );
    }

    #[test]
    fn unseen_shows_done_only_when_not_working() {
        let mut pane = with_status(agent(1, "x"), status(PaneStatusState::Working, None, ""));
        pane.summary.unseen = true;
        pane.bell = true;
        assert_eq!(pane_attention(&pane), Attention::Working);
        pane.status = Some(status(PaneStatusState::Idle, None, ""));
        assert_eq!(pane_attention(&pane), Attention::Done);
        pane.status = None;
        assert_eq!(pane_attention(&pane), Attention::Done);
        assert_eq!(
            texts(&summarize_window(&window(1, vec![pane.clone()]))),
            vec![("Done", Tone::Normal)]
        );
        pane.summary.unseen = false;
        assert_eq!(pane_attention(&pane), Attention::Bell);
        pane.bell = false;
        assert_eq!(pane_attention(&pane), Attention::None);
    }

    #[test]
    fn rollups_count_windows_and_keep_the_newest_change() {
        let stamped = |mut pane: PaneSnapshot, changed_at| {
            pane.summary.changed_at = changed_at;
            pane
        };
        let blocked = stamped(
            with_status(agent(1, "x"), status(PaneStatusState::Blocked, None, "")),
            30,
        );
        let failed = stamped(
            with_status(agent(2, "x"), status(PaneStatusState::Error, None, "")),
            70,
        );
        let mut belled = stamped(terminal(3, "zsh"), 20);
        belled.bell = true;
        let first = SessionSnapshot {
            id: SessionId(1),
            name: String::new(),
            active_window: WindowId(1),
            windows: vec![
                window(1, vec![blocked, failed]),
                window(2, vec![belled]),
                window(3, vec![stamped(terminal(4, "zsh"), 90)]),
            ],
            viewers: Vec::new(),
        };
        let mut done = stamped(
            with_status(agent(5, "x"), status(PaneStatusState::Done, None, "")),
            40,
        );
        done.summary.unseen = true;
        let second = SessionSnapshot {
            id: SessionId(2),
            name: "other".to_owned(),
            active_window: WindowId(4),
            windows: vec![window(4, vec![done])],
            viewers: Vec::new(),
        };
        let snapshot = MuxSnapshot {
            generation: 1,
            sessions: vec![first, second],
            focused_window: None,
        };
        let (sessions, host) = summarize_host(&snapshot);
        assert_eq!(sessions[0].label.to_string(), "session $1");
        assert_eq!(sessions[1].label.to_string(), "other");
        assert_eq!(
            sessions[0].rollup,
            AttentionRollup {
                needs_you: 1,
                failed: 0,
                done: 0,
                bell: 1,
                working: 0,
                changed_at: 90,
            }
        );
        assert_eq!(sessions[0].rollup.top(), Attention::NeedsYou);
        assert_eq!(sessions[1].rollup.top(), Attention::Done);
        assert_eq!(
            host,
            AttentionRollup {
                needs_you: 1,
                failed: 0,
                done: 1,
                bell: 1,
                working: 0,
                changed_at: 90,
            }
        );
        assert_eq!(AttentionRollup::default().top(), Attention::None);
    }

    #[test]
    fn short_age_boundaries() {
        for (seconds, label) in [
            (0, "now"),
            (59, "now"),
            (60, "1m"),
            (3_599, "59m"),
            (3_600, "1h"),
            (86_399, "23h"),
            (86_400, "1d"),
        ] {
            assert_eq!(short_age(seconds), label);
        }
    }

    #[test]
    fn daemon_clock_corrects_skew_and_saturates() {
        let ahead = DaemonClock::from_hello(1_000_100, 1_000_000);
        assert_eq!(ahead.now(1_000_050), 1_000_150);
        let behind = DaemonClock::from_hello(1_000_000, 1_000_100);
        assert_eq!(behind.now(1_000_150), 1_000_050);
        assert_eq!(behind.now(10), 0);
        assert_eq!(DaemonClock::from_hello(0, 1_000_000).now(42), 42);
        assert_eq!(DaemonClock::default().now(42), 42);
        assert_eq!(
            DaemonClock::from_hello(u64::MAX, 0).now(u64::MAX - 5),
            u64::MAX
        );
        assert_eq!(DaemonClock::from_hello(1, u64::MAX).now(5), 0);
        assert_eq!(
            age_label(1_000_200, behind.now(1_000_150)),
            Some("now".to_owned())
        );
    }

    #[test]
    fn numbered_sessions_take_the_name_of_where_their_panes_are() {
        let at = |id, cwd: &str, repo: &str| {
            let mut pane = terminal(id, "zsh");
            pane.summary.cwd = cwd.to_owned();
            pane.summary.repo = repo.to_owned();
            pane
        };
        let label = |name: &str, panes: Vec<PaneSnapshot>| {
            session_label(&SessionSnapshot {
                id: SessionId(1),
                name: name.to_owned(),
                active_window: WindowId(1),
                windows: vec![window(1, panes)],
                viewers: Vec::new(),
            })
            .to_string()
        };
        let site = || at(1, "~/dev/site", "~/dev/site");
        let zz = |id| at(id, "~/dev/zz/crates", "~/dev/zz");
        assert_eq!(label("0", vec![site(), zz(2), zz(3)]), "zz 0");
        assert_eq!(label("0", vec![site(), zz(2)]), "site 0");
        assert_eq!(label("3", vec![at(1, "~", ""), at(2, "/tmp", "")]), "~ 3");
        assert_eq!(label("0", vec![terminal(1, "zsh")]), "0");
        assert_eq!(label("api", vec![zz(1)]), "api");
        assert_eq!(label("", vec![zz(1)]), "zz");
        assert_eq!(label("", Vec::new()), "session $1");
    }

    #[test]
    fn a_chat_keeps_its_agent_in_front_of_the_panes_it_split() {
        let browser = pane(
            3,
            PaneKindSnapshot::Browser(BrowserDescriptor::single(
                "http://localhost:4321/docs".to_owned(),
                "default".to_owned(),
            )),
        );
        let mut window = window(
            1,
            vec![
                terminal(1, "npm run dev"),
                agent(2, "Copy mode docs"),
                browser,
            ],
        );
        window.active_pane = PaneId(3);
        let chat = summarize_chat(&window).expect("a window running claude is a chat");
        assert_eq!(chat.headline, PaneId(2));
        assert_eq!(chat.title, "Copy mode docs");
        assert_eq!(
            chat.tiles
                .iter()
                .map(|tile| tile.pane.0)
                .collect::<Vec<_>>(),
            [2, 1, 3]
        );
        assert_eq!(summarize_window(&window).tiles[0].pane, PaneId(3));
        assert_eq!(texts(&chat), [("Claude Code", Tone::Muted)]);
    }

    #[test]
    fn only_windows_with_an_agent_are_chats() {
        assert_eq!(summarize_chat(&window(1, vec![terminal(1, "zsh")])), None);
        assert_eq!(
            summarize_chat(&window(2, vec![pane(5, PaneKindSnapshot::Picker)])),
            None
        );
        let split = summarize_chat(&window(
            3,
            vec![
                terminal(1, "zsh"),
                pane(2, PaneKindSnapshot::Picker),
                agent(3, "Docs"),
            ],
        ))
        .expect("a window with an agent is a chat");
        assert_eq!(split.headline, PaneId(3));
    }

    #[test]
    fn chats_flatten_sessions_and_roll_up_only_chats() {
        let session = |id, windows| SessionSnapshot {
            id: SessionId(id),
            name: id.to_string(),
            active_window: WindowId(1),
            windows,
            viewers: Vec::new(),
        };
        let blocked = with_status(
            agent(2, "Release"),
            status(PaneStatusState::Blocked, None, "push the tag?"),
        );
        let failing = with_status(
            terminal(3, "make"),
            status(PaneStatusState::Error, None, ""),
        );
        let sessions = [
            session(1, vec![window(1, vec![blocked]), window(2, vec![failing])]),
            session(2, vec![window(3, vec![agent(4, "Docs")])]),
        ];
        let (chats, rollup) = summarize_chats(&sessions);
        assert_eq!(
            chats
                .iter()
                .map(|chat| chat.title.as_str())
                .collect::<Vec<_>>(),
            ["Release", "Docs"]
        );
        assert_eq!((rollup.needs_you, rollup.failed), (1, 0));
    }

    type Line<'a> = Vec<(&'a str, Tone)>;

    fn line(detail: &[DetailPart]) -> Line<'_> {
        detail
            .iter()
            .map(|part| (part.text.as_str(), part.tone))
            .collect()
    }

    fn faces(summary: &WindowSummary) -> Vec<(u64, Attention)> {
        summary
            .tiles
            .iter()
            .map(|tile| (tile.pane.0, tile.attention))
            .collect()
    }

    fn shown(
        row: &WindowRow,
    ) -> (
        bool,
        Option<u8>,
        &str,
        Line<'_>,
        Vec<(u64, Attention)>,
        Vec<(u64, Attention, &str, Line<'_>)>,
    ) {
        (
            row.chat,
            row.shortcut,
            row.summary.title.as_str(),
            line(&row.summary.detail),
            faces(&row.summary),
            row.panes
                .iter()
                .map(|entry| {
                    (
                        entry.face.pane.0,
                        entry.face.attention,
                        entry.title.as_str(),
                        line(&entry.detail),
                    )
                })
                .collect(),
        )
    }

    fn window_row(machine: &ChatMachine, index: usize) -> &WindowRow {
        match &machine.rows[index] {
            ChatRow::Window(row) => row,
            ChatRow::Pile(_) => panic!("row {index} is the pile"),
        }
    }

    fn in_session(id: u64, windows: Vec<WindowSnapshot>) -> SessionSnapshot {
        SessionSnapshot {
            id: SessionId(id),
            name: id.to_string(),
            active_window: windows.first().map_or_default(|window| window.id),
            windows,
            viewers: Vec::new(),
        }
    }

    fn placed(mut pane: PaneSnapshot, cwd: &str, branch: &str) -> PaneSnapshot {
        pane.summary.cwd = cwd.to_owned();
        pane.summary.branch = branch.to_owned();
        pane
    }

    fn running(mut pane: PaneSnapshot, process: &[&str]) -> PaneSnapshot {
        pane.summary.process = process.iter().map(|entry| (*entry).to_owned()).collect();
        pane
    }

    #[test]
    fn chat_view_lists_chat_windows_then_piles_the_plain_ones() {
        use Attention::{Bell, NeedsYou, Working};
        let quiet = Attention::None;
        let hit_tests = window(
            1,
            vec![with_status(
                agent(11, "Fix stack hit tests"),
                status(PaneStatusState::Working, None, "Running the tests"),
            )],
        );
        let mut bento = window(
            2,
            vec![
                agent(21, "Port bento to penna"),
                placed(terminal(22, "vite"), "~/dev/zebrilus", "bento"),
                terminal(23, "zsh"),
            ],
        );
        bento.active_pane = PaneId(23);
        let mut split = window(
            3,
            vec![
                agent(31, "Pill menu New chat"),
                with_status(
                    agent(32, "Review the pill diff"),
                    status(PaneStatusState::Working, None, ""),
                ),
                running(terminal(33, "cargo test -p zz-ui"), &["cargo"]),
            ],
        );
        split.active_pane = PaneId(32);
        let mut plan = window(4, vec![agent(41, "Plan the chat rows")]);
        plan.panes.get_mut(&PaneId(41)).unwrap().summary.changed_at = 100;
        let mut fork = with_status(
            agent(42, "What the kit draws"),
            status(
                PaneStatusState::Blocked,
                Some(PaneStatusKind::Permission),
                "rg guide",
            ),
        );
        fork.summary.changed_at = 300;
        plan.panes.insert(PaneId(42), fork);
        plan.floating.push(zz_protocol::FloatingPaneSnapshot {
            pane: PaneId(42),
            xoff: 4,
            yoff: 2,
            sx: 60,
            sy: 16,
            visible: true,
            border_lines: zz_protocol::PaneBorderLines::Single,
            border_status: zz_protocol::PaneBorderStatus::Off,
        });
        let mut editing = window(
            5,
            vec![
                placed(
                    running(
                        terminal(51, "nvim flat.rs"),
                        &["/opt/homebrew/bin/nvim", "nvim"],
                    ),
                    "~/dev/zebrilus",
                    "main",
                ),
                placed(
                    running(terminal(52, "cargo watch -x check"), &["cargo"]),
                    "~/dev/zebrilus",
                    "main",
                ),
            ],
        );
        editing.panes.get_mut(&PaneId(52)).unwrap().summary.command =
            Some(ShellCommand::Finished(1));
        let docs = window(
            6,
            vec![pane(
                61,
                PaneKindSnapshot::Browser(BrowserDescriptor::single(
                    "https://docs.rs/ala/latest/ala/".to_owned(),
                    "default".to_owned(),
                )),
            )],
        );
        let mut shell = placed(terminal(71, ""), "~/dev", "");
        shell.summary.shell = "zsh".to_owned();
        shell.bell = true;
        let rang = window(7, vec![shell]);
        let picker = window(8, vec![pane(81, PaneKindSnapshot::Picker)]);
        let mut hints = ChatHints::default();
        hints.observe(&split);
        hints.observe(&editing);
        hints.observe(&picker);
        let mut sessions = [
            in_session(1, vec![hit_tests, bento]),
            in_session(2, vec![split, docs, editing, plan, rang, picker]),
        ];
        let mut machine = summarize_chat_machine(&sessions, &hints, |_| true);
        number_chat_rows([&mut machine]);

        assert_eq!(machine.rows.len(), 5);
        assert_eq!(
            shown(window_row(&machine, 0)),
            (
                true,
                Some(0),
                "Fix stack hit tests",
                vec![("Running the tests", Tone::Muted)],
                vec![(11, Working)],
                vec![],
            )
        );
        assert_eq!(
            shown(window_row(&machine, 1)),
            (
                true,
                Some(1),
                "Port bento to penna",
                vec![("Claude Code", Tone::Muted)],
                vec![(21, quiet), (22, quiet), (23, quiet)],
                vec![
                    (
                        21,
                        quiet,
                        "Port bento to penna",
                        vec![("Claude Code", Tone::Muted)]
                    ),
                    (22, quiet, "vite", vec![("zebrilus · bento", Tone::Muted)]),
                    (23, quiet, "zsh", vec![]),
                ],
            )
        );
        assert_eq!(
            shown(window_row(&machine, 2)),
            (
                true,
                Some(2),
                "Review the pill diff",
                vec![("Working", Tone::Muted)],
                vec![(32, Working), (31, quiet), (33, quiet)],
                vec![
                    (
                        31,
                        quiet,
                        "Pill menu New chat",
                        vec![("Claude Code", Tone::Muted)]
                    ),
                    (
                        32,
                        Working,
                        "Review the pill diff",
                        vec![("Working", Tone::Muted)]
                    ),
                    (33, quiet, "cargo test -p zz-ui", vec![]),
                ],
            )
        );
        assert_eq!(
            shown(window_row(&machine, 3)),
            (
                true,
                Some(3),
                "Plan the chat rows",
                vec![
                    ("Needs approval", Tone::Warning),
                    (" · What the kit draws", Tone::Muted)
                ],
                vec![(41, quiet), (42, NeedsYou)],
                vec![
                    (
                        41,
                        quiet,
                        "Plan the chat rows",
                        vec![("Claude Code", Tone::Muted)]
                    ),
                    (
                        42,
                        NeedsYou,
                        "What the kit draws",
                        vec![
                            ("Needs approval", Tone::Warning),
                            (" · rg guide", Tone::Muted)
                        ]
                    ),
                ],
            )
        );

        assert_eq!(
            line(&window_row(&machine, 3).open_detail),
            [("Claude Code", Tone::Muted)]
        );
        let ChatRow::Pile(pile) = &machine.rows[4] else {
            panic!("the plain windows pile up last");
        };
        assert_eq!(
            (
                pile.summary.title.as_str(),
                line(&pile.summary.detail),
                line(&pile.open_detail),
                faces(&pile.summary),
                pile.summary.attention,
            ),
            (
                "nvim flat.rs",
                vec![
                    ("Exit 1", Tone::Danger),
                    (" · cargo watch -x check", Tone::Muted)
                ],
                vec![("zebrilus · main", Tone::Muted)],
                vec![(51, quiet), (61, quiet), (71, Bell)],
                Bell,
            )
        );
        assert_eq!(
            pile.windows.iter().map(shown).collect::<Vec<_>>(),
            [
                (false, None, "docs.rs", vec![], vec![(61, quiet)], vec![]),
                (
                    false,
                    None,
                    "nvim flat.rs",
                    vec![
                        ("Exit 1", Tone::Danger),
                        (" · cargo watch -x check", Tone::Muted)
                    ],
                    vec![(51, quiet), (52, quiet)],
                    vec![],
                ),
                (
                    false,
                    None,
                    "zsh",
                    vec![("Bell", Tone::Normal), (" · dev", Tone::Muted)],
                    vec![(71, Bell)],
                    vec![],
                ),
            ]
        );
        assert_eq!(
            machine.rollup,
            AttentionRollup {
                needs_you: 1,
                failed: 0,
                done: 0,
                bell: 1,
                working: 2,
                changed_at: 100,
            }
        );
        assert_eq!(
            machine.current(WindowId(6), PaneId(61)),
            Some(ChatCurrent {
                row: 4,
                entry: Some(0)
            })
        );
        assert_eq!(
            machine.current(WindowId(3), PaneId(33)),
            Some(ChatCurrent {
                row: 2,
                entry: Some(2)
            })
        );
        assert_eq!(
            machine.current(WindowId(1), PaneId(11)),
            Some(ChatCurrent {
                row: 0,
                entry: None
            })
        );

        let closed = summarize_chat_machine(&sessions, &hints, |_| false);
        let ChatRow::Pile(shut) = &closed.rows[4] else {
            panic!("the plain windows pile up last");
        };
        assert_eq!(
            (
                window_row(&closed, 1).opens,
                window_row(&closed, 1).panes.len(),
                shut.windows.len(),
                closed.current(WindowId(6), PaneId(61)),
            ),
            (
                true,
                0,
                0,
                Some(ChatCurrent {
                    row: 4,
                    entry: None
                })
            )
        );
        let mut docs_last = hints.clone();
        docs_last.last_plain = Some(WindowId(6));
        let ChatRow::Pile(docs_front) = summarize_chat_machine(&sessions, &docs_last, |_| false)
            .rows
            .remove(4)
        else {
            panic!("the plain windows pile up last");
        };
        assert_eq!(
            line(&docs_front.summary.detail),
            [("Exit 1", Tone::Danger), (" · nvim flat.rs", Tone::Muted)]
        );

        sessions[1].windows[0].active_pane = PaneId(33);
        let front = |hints: &ChatHints| {
            faces(&window_row(&summarize_chat_machine(&sessions, hints, |_| true), 2).summary)
        };
        assert_eq!(front(&hints), [(32, Working), (31, quiet), (33, quiet)]);
        assert_eq!(
            front(&ChatHints::default()),
            [(31, quiet), (32, Working), (33, quiet)]
        );
    }

    #[test]
    fn a_lone_plain_window_is_an_unnumbered_row_after_the_chats() {
        let first = [in_session(
            1,
            vec![
                window(1, vec![agent(11, "Docs")]),
                window(2, vec![agent(21, "Release")]),
            ],
        )];
        let mut make = terminal(82, "make");
        make.summary.command = Some(ShellCommand::Finished(2));
        let mut build = window(
            8,
            vec![placed(terminal(81, "zsh"), "~/dev/zz", "main"), make],
        );
        build.name = "build".to_owned();
        build.automatic_rename = false;
        let second = [in_session(
            2,
            vec![build, window(9, vec![agent(91, "Tidy notes")])],
        )];
        let mut machines = [
            summarize_chat_machine(&first, &ChatHints::default(), |_| true),
            summarize_chat_machine(&second, &ChatHints::default(), |_| true),
        ];
        number_chat_rows(&mut machines);
        let [first, second] = &machines;
        assert_eq!(
            [window_row(first, 0).shortcut, window_row(first, 1).shortcut],
            [Some(0), Some(1)]
        );
        assert_eq!(second.rows.len(), 2);
        assert_eq!(
            (
                window_row(second, 0).summary.title.as_str(),
                window_row(second, 0).shortcut
            ),
            ("Tidy notes", Some(2))
        );
        assert_eq!(
            shown(window_row(second, 1)),
            (
                false,
                None,
                "build",
                vec![("Exit 2", Tone::Danger), (" · make", Tone::Muted)],
                vec![(81, Attention::None), (82, Attention::None)],
                vec![
                    (81, Attention::None, "zsh", vec![("zz · main", Tone::Muted)]),
                    (82, Attention::None, "make", vec![("Exit 2", Tone::Danger)]),
                ],
            )
        );
        assert_eq!(
            second.current(WindowId(8), PaneId(82)),
            Some(ChatCurrent {
                row: 1,
                entry: Some(1)
            })
        );
    }
}
