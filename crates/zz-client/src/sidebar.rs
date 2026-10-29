use std::collections::BTreeSet;

use zz_protocol::{CommandInvocation, MuxSnapshot, PaneId, SessionId, WindowId};

use crate::{
    AgentAttentionStatus, ClientCore, agent_attention_status,
    navigation::{RenameTarget, ordered_panes, pane_label, rename_prompt_command, session_label},
    program::Program,
    window_summary::{PaneTag, pane_program, pane_tag},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum SidebarTarget {
    Host,
    Session(SessionId),
    Window(WindowId),
    Pane(PaneId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SidebarBadge {
    NeedsInput,
    Failed,
    Working,
    Finished,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SidebarNode {
    Host,
    Session,
    Window,
    Pane(PaneTag),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarRow {
    pub target: SidebarTarget,
    pub depth: u8,
    pub node: SidebarNode,
    pub label: String,
    pub index: Option<String>,
    pub on_active_path: bool,
    pub expanded: bool,
    pub expandable: bool,
    pub bell: bool,
    pub badge: Option<SidebarBadge>,
    pub program: Option<&'static Program>,
}

#[must_use]
pub fn pane_badge(
    pane: PaneId,
    core: &ClientCore,
    unseen: &BTreeSet<PaneId>,
) -> Option<SidebarBadge> {
    match agent_attention_status(core.agent_state(pane)?) {
        AgentAttentionStatus::NeedsInput => Some(SidebarBadge::NeedsInput),
        AgentAttentionStatus::Failed => Some(SidebarBadge::Failed),
        AgentAttentionStatus::Working => Some(SidebarBadge::Working),
        AgentAttentionStatus::Idle => unseen.contains(&pane).then_some(SidebarBadge::Finished),
    }
}

fn rollup(
    panes: impl Iterator<Item = PaneId>,
    core: &ClientCore,
    unseen: &BTreeSet<PaneId>,
) -> Option<SidebarBadge> {
    panes
        .filter_map(|pane| pane_badge(pane, core, unseen))
        .min()
}

#[must_use]
pub fn sidebar_rows(
    core: &ClientCore,
    collapsed: &BTreeSet<SidebarTarget>,
    unseen: &BTreeSet<PaneId>,
) -> Vec<SidebarRow> {
    let snapshot = core.snapshot();
    let attached = core.attached_session();
    let expanded = !collapsed.contains(&SidebarTarget::Host);
    let mut rows = vec![SidebarRow {
        target: SidebarTarget::Host,
        depth: 0,
        node: SidebarNode::Host,
        label: "zz daemon".into(),
        index: None,
        on_active_path: attached.is_some(),
        expanded,
        expandable: true,
        bell: snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .flat_map(|window| window.panes.values())
            .any(|pane| pane.bell),
        badge: rollup(
            snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .flat_map(|window| window.panes.keys().copied()),
            core,
            unseen,
        ),
        program: None,
    }];
    if !expanded {
        return rows;
    }
    for session in &snapshot.sessions {
        let active_session = attached == Some(session.id);
        let target = SidebarTarget::Session(session.id);
        let expanded = !collapsed.contains(&target);
        let label = session_label(session);
        rows.push(SidebarRow {
            target,
            depth: 1,
            node: SidebarNode::Session,
            label: label.text,
            index: label.index,
            on_active_path: active_session,
            expanded,
            expandable: !session.windows.is_empty(),
            bell: session
                .windows
                .iter()
                .flat_map(|window| window.panes.values())
                .any(|pane| pane.bell),
            badge: rollup(
                session
                    .windows
                    .iter()
                    .flat_map(|window| window.panes.keys().copied()),
                core,
                unseen,
            ),
            program: None,
        });
        if !expanded {
            continue;
        }
        for window in &session.windows {
            let active_window = active_session && snapshot.focused_window_for(session) == window.id;
            let target = SidebarTarget::Window(window.id);
            let expanded = !collapsed.contains(&target);
            rows.push(SidebarRow {
                target,
                depth: 2,
                node: SidebarNode::Window,
                label: window.name.clone(),
                index: None,
                on_active_path: active_window,
                expanded,
                expandable: !window.panes.is_empty(),
                bell: window.panes.values().any(|pane| pane.bell),
                badge: rollup(window.panes.keys().copied(), core, unseen),
                program: if expanded {
                    None
                } else {
                    window.panes.get(&window.active_pane).and_then(pane_program)
                },
            });
            if !expanded {
                continue;
            }
            rows.extend(ordered_panes(window).into_iter().map(|pane| SidebarRow {
                target: SidebarTarget::Pane(pane.id),
                depth: 3,
                node: SidebarNode::Pane(pane_tag(&pane.kind)),
                label: pane_label(pane),
                index: None,
                on_active_path: active_window && window.active_pane == pane.id,
                expanded: false,
                expandable: false,
                bell: pane.bell,
                badge: pane_badge(pane.id, core, unseen),
                program: pane_program(pane),
            }));
        }
    }
    rows
}

#[must_use]
pub fn sidebar_active(snapshot: &MuxSnapshot, attached: Option<SessionId>) -> SidebarTarget {
    let session = snapshot
        .sessions
        .iter()
        .find(|session| Some(session.id) == attached);
    let window = session.and_then(|session| {
        session
            .windows
            .iter()
            .find(|window| window.id == snapshot.focused_window_for(session))
    });
    window
        .map(|window| SidebarTarget::Pane(window.active_pane))
        .or_else(|| session.map(|session| SidebarTarget::Session(session.id)))
        .unwrap_or(SidebarTarget::Host)
}

#[must_use]
pub fn sidebar_active_path(
    snapshot: &MuxSnapshot,
    attached: Option<SessionId>,
) -> Vec<SidebarTarget> {
    let mut path = vec![SidebarTarget::Host];
    let Some(session) = snapshot
        .sessions
        .iter()
        .find(|session| Some(session.id) == attached)
    else {
        return path;
    };
    path.push(SidebarTarget::Session(session.id));
    if let Some(window) = session
        .windows
        .iter()
        .find(|window| window.id == snapshot.focused_window_for(session))
    {
        path.push(SidebarTarget::Window(window.id));
    }
    path
}

#[must_use]
pub fn sidebar_rename_command(
    snapshot: &MuxSnapshot,
    target: SidebarTarget,
) -> Option<(&'static str, CommandInvocation)> {
    match target {
        SidebarTarget::Host => None,
        SidebarTarget::Session(id) => snapshot
            .sessions
            .iter()
            .find(|session| session.id == id)
            .map(|session| rename_prompt_command(RenameTarget::Session(id), &session.name)),
        SidebarTarget::Window(id) => snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find(|window| window.id == id)
            .map(|window| rename_prompt_command(RenameTarget::Window(id), &window.name)),
        SidebarTarget::Pane(id) => snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find(|window| window.panes.contains_key(&id))
            .map(|window| rename_prompt_command(RenameTarget::Window(window.id), &window.name)),
    }
}

#[must_use]
pub fn sidebar_close_command(target: SidebarTarget) -> Option<(&'static str, CommandInvocation)> {
    let (label, command, id) = match target {
        SidebarTarget::Host => return None,
        SidebarTarget::Session(id) => ("Close session", "kill-session", id.to_string()),
        SidebarTarget::Window(id) => ("Close window", "kill-window", id.to_string()),
        SidebarTarget::Pane(id) => ("Close pane", "kill-pane", id.to_string()),
    };
    Some((
        label,
        CommandInvocation::new(command, ["-t".to_owned(), id]),
    ))
}

#[must_use]
pub fn sidebar_add_command(target: SidebarTarget) -> Option<(&'static str, CommandInvocation)> {
    match target {
        SidebarTarget::Host => Some((
            "New session",
            CommandInvocation::new("new-session", Vec::<String>::new()),
        )),
        SidebarTarget::Session(id) => Some((
            "New window",
            CommandInvocation::new("new-window", ["-t".to_owned(), id.to_string()]),
        )),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarActivation {
    pub attach: Option<SessionId>,
    pub commands: Vec<CommandInvocation>,
}

#[must_use]
pub fn sidebar_activation(
    snapshot: &MuxSnapshot,
    attached: Option<SessionId>,
    target: SidebarTarget,
) -> SidebarActivation {
    let owner = snapshot
        .sessions
        .iter()
        .find(|session| match target {
            SidebarTarget::Host => false,
            SidebarTarget::Session(id) => session.id == id,
            SidebarTarget::Window(id) => session.windows.iter().any(|window| window.id == id),
            SidebarTarget::Pane(id) => session
                .windows
                .iter()
                .any(|window| window.panes.contains_key(&id)),
        })
        .map(|session| session.id);
    let attach = owner
        .filter(|owner| matches!(target, SidebarTarget::Session(_)) || attached != Some(*owner));
    let commands = match target {
        SidebarTarget::Host | SidebarTarget::Session(_) => Vec::new(),
        SidebarTarget::Window(id) => vec![CommandInvocation::new(
            "select-window",
            ["-t".to_owned(), id.to_string()],
        )],
        SidebarTarget::Pane(id) => vec![
            CommandInvocation::new("select-window", ["-t".to_owned(), id.to_string()]),
            CommandInvocation::new(
                "select-pane",
                ["-Z".to_owned(), "-t".to_owned(), id.to_string()],
            ),
        ],
    };
    SidebarActivation { attach, commands }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeNavigationRow {
    pub depth: u8,
    pub parent: Option<usize>,
    pub expandable: bool,
    pub expanded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeNavigation {
    Up,
    Down,
    First,
    Last,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeNavigationResult {
    Select(usize),
    Toggle(usize),
    None,
}

#[must_use]
pub fn navigation_rows(rows: &[SidebarRow]) -> Vec<TreeNavigationRow> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| TreeNavigationRow {
            depth: row.depth,
            parent: rows[..index]
                .iter()
                .rposition(|parent| parent.depth < row.depth),
            expandable: row.expandable,
            expanded: row.expanded,
        })
        .collect()
}

#[must_use]
pub fn tree_navigation(
    rows: &[TreeNavigationRow],
    selected: Option<usize>,
    direction: TreeNavigation,
) -> TreeNavigationResult {
    use TreeNavigationResult::{None, Select, Toggle};
    if rows.is_empty() {
        return None;
    }
    if selected.is_none() && matches!(direction, TreeNavigation::Left | TreeNavigation::Right) {
        return None;
    }
    let index = selected.unwrap_or(0).min(rows.len() - 1);
    let row = rows[index];
    match direction {
        TreeNavigation::Up => Select(index.saturating_sub(1)),
        TreeNavigation::Down => Select((index + 1).min(rows.len() - 1)),
        TreeNavigation::First => Select(0),
        TreeNavigation::Last => Select(rows.len() - 1),
        TreeNavigation::Left if row.expandable && row.expanded => Toggle(index),
        TreeNavigation::Left => row.parent.map_or(None, Select),
        TreeNavigation::Right if row.expandable && !row.expanded => Toggle(index),
        TreeNavigation::Right
            if rows
                .get(index + 1)
                .is_some_and(|child| child.depth == row.depth + 1) =>
        {
            Select(index + 1)
        }
        TreeNavigation::Right => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        depth: u8,
        parent: Option<usize>,
        expandable: bool,
        expanded: bool,
    ) -> TreeNavigationRow {
        TreeNavigationRow {
            depth,
            parent,
            expandable,
            expanded,
        }
    }

    #[test]
    fn arrows_walk_the_tree_and_fold_its_rows() {
        let rows = [
            row(0, None, true, true),
            row(1, Some(0), true, false),
            row(1, Some(0), true, true),
            row(2, Some(2), false, false),
        ];
        assert_eq!(
            tree_navigation(&rows, Some(1), TreeNavigation::Right),
            TreeNavigationResult::Toggle(1)
        );
        assert_eq!(
            tree_navigation(&rows, Some(2), TreeNavigation::Right),
            TreeNavigationResult::Select(3)
        );
        assert_eq!(
            tree_navigation(&rows, Some(3), TreeNavigation::Left),
            TreeNavigationResult::Select(2)
        );
        assert_eq!(
            tree_navigation(&rows, Some(2), TreeNavigation::Left),
            TreeNavigationResult::Toggle(2)
        );
        assert_eq!(
            tree_navigation(&rows, None, TreeNavigation::Left),
            TreeNavigationResult::None
        );
        assert_eq!(
            tree_navigation(&rows, Some(3), TreeNavigation::Down),
            TreeNavigationResult::Select(3)
        );
    }

    #[test]
    fn a_pane_attaches_its_session_only_when_another_is_attached() {
        let snapshot = MuxSnapshot::default();
        assert_eq!(
            sidebar_activation(&snapshot, None, SidebarTarget::Pane(PaneId(3))).commands,
            vec![
                CommandInvocation::new("select-window", ["-t", "%3"]),
                CommandInvocation::new("select-pane", ["-Z", "-t", "%3"]),
            ]
        );
        assert_eq!(
            sidebar_activation(&snapshot, None, SidebarTarget::Host),
            SidebarActivation::default()
        );
        assert_eq!(
            sidebar_close_command(SidebarTarget::Window(WindowId(2))),
            Some((
                "Close window",
                CommandInvocation::new("kill-window", ["-t", "@2"])
            ))
        );
    }
}
