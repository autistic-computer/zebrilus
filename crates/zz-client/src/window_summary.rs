use std::{cmp::Reverse, path::Path};

use url::Url;
use zz_protocol::{
    AgentProvider, MuxSnapshot, PaneId, PaneKindSnapshot, PaneSnapshot, PaneStatusKind,
    PaneStatusState, SessionId, SessionSnapshot, ShellCommand, WindowId, WindowSnapshot,
};

use crate::navigation::{SessionLabel, last_component, ordered_panes, pane_label, session_label};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneTag {
    Picker,
    Terminal,
    Browser,
    Agent(AgentProvider),
    Editor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneTile {
    pub pane: PaneId,
    pub tag: PaneTag,
    pub label: String,
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
        self.changed_at = self.changed_at.max(window.changed_at);
        let count = match window.attention {
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
        PaneKindSnapshot::Agent(agent) => PaneTag::Agent(agent.provider),
        PaneKindSnapshot::Editor(_) => PaneTag::Editor,
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
        .map(|pane| PaneTile {
            pane: pane.id,
            tag: pane_tag(&pane.kind),
            label: pane_label(pane),
        })
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
    let sessions = snapshot
        .sessions
        .iter()
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
    let title = pane.title.trim();
    let own = matches!(pane.kind, PaneKindSnapshot::Agent(_)) && title != "agent";
    [
        own.then_some(title),
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

    fn agent(id: u64, title: &str) -> PaneSnapshot {
        PaneSnapshot {
            title: title.to_owned(),
            ..pane(
                id,
                PaneKindSnapshot::Agent(AgentDescriptor {
                    provider: AgentProvider::ClaudeCode,
                    ..AgentDescriptor::default()
                }),
            )
        }
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
                renamed(window(1, vec![agent(1, "Fix titlebar")])),
                "Fix titlebar",
            ),
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
            (window(1, vec![agent(1, "agent")]), "New session"),
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
            (agent(1, "x"), vec![("Claude Code", Tone::Muted)]),
        ];
        for (pane, expected) in cases {
            let summary = summarize_window(&window(1, vec![pane]));
            assert_eq!(texts(&summary), expected);
        }
    }

    #[test]
    fn agents_fall_back_to_the_descriptor_cwd() {
        let mut pane = agent(1, "x");
        pane.kind = PaneKindSnapshot::Agent(AgentDescriptor {
            provider: AgentProvider::Codex,
            cwd: Some("/work/zz".into()),
            session_id: None,
        });
        let summary = summarize_window(&window(1, vec![pane]));
        assert_eq!(texts(&summary), vec![("zz", Tone::Muted)]);
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
            PaneKindSnapshot::Agent(AgentDescriptor::default()),
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
                PaneTag::Agent(AgentProvider::ClaudeCode),
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
}
