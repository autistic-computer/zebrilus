use std::collections::{BTreeMap, BTreeSet};
use zz_protocol::{PaneId, SessionId, WindowId};

use crate::desktop_nav::NavTarget;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NavNode<Host> {
    Host(Host),
    Target(Host, NavTarget),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavOwner {
    pub session: SessionId,
    pub window: WindowId,
}

#[must_use]
pub fn target_owner<Windows, Panes>(target: NavTarget, windows: Windows) -> Option<NavOwner>
where
    Windows: IntoIterator<Item = (SessionId, WindowId, Panes)>,
    Panes: IntoIterator<Item = PaneId>,
{
    if matches!(target, NavTarget::Session(_)) {
        return None;
    }
    for (session, window, panes) in windows {
        let matched = match target {
            NavTarget::Session(_) => false,
            NavTarget::Window(id) => window == id,
            NavTarget::Pane(id) => panes.into_iter().any(|pane| pane == id),
        };
        if matched {
            return Some(NavOwner { session, window });
        }
    }
    None
}

#[must_use]
pub fn renameable_name<'a, Sessions, Windows>(
    target: NavTarget,
    sessions: Sessions,
) -> Option<&'a str>
where
    Sessions: IntoIterator<Item = (SessionId, &'a str, Windows)>,
    Windows: IntoIterator<Item = (WindowId, &'a str)>,
{
    if matches!(target, NavTarget::Pane(_)) {
        return None;
    }
    for (session, name, windows) in sessions {
        if target == NavTarget::Session(session) {
            return Some(name);
        }
        if let NavTarget::Window(id) = target {
            for (window, name) in windows {
                if window == id {
                    return Some(name);
                }
            }
        }
    }
    None
}

#[must_use]
pub fn target_expandable<Sessions, Windows, Panes>(target: NavTarget, sessions: Sessions) -> bool
where
    Sessions: IntoIterator<Item = (SessionId, Windows)>,
    Windows: IntoIterator<Item = (WindowId, Panes)>,
    Windows::IntoIter: ExactSizeIterator,
    Panes: IntoIterator<Item = PaneId>,
    Panes::IntoIter: ExactSizeIterator,
{
    if matches!(target, NavTarget::Pane(_)) {
        return false;
    }
    for (session, windows) in sessions {
        let windows = windows.into_iter();
        if target == NavTarget::Session(session) {
            return windows.len() != 0;
        }
        if let NavTarget::Window(id) = target {
            for (window, panes) in windows {
                if window == id {
                    return panes.into_iter().len() != 0;
                }
            }
        }
    }
    false
}

pub fn shortcut_windows<Host, Hosts, Sessions, Windows>(
    hosts: Hosts,
) -> impl Iterator<Item = (Host, WindowId)>
where
    Host: Copy,
    Hosts: IntoIterator<Item = (Host, bool, Sessions)>,
    Sessions: IntoIterator<Item = Windows>,
    Windows: IntoIterator<Item = WindowId>,
{
    hosts
        .into_iter()
        .filter(|(_, connected, _)| *connected)
        .flat_map(|(host, _, sessions)| {
            sessions
                .into_iter()
                .flat_map(move |windows| windows.into_iter().map(move |window| (host, window)))
        })
        .take(9)
}

#[must_use]
pub fn active_for_session<Host, Windows, Panes>(
    mut previous: Option<NavNode<Host>>,
    host: Host,
    attached_host: Host,
    session: SessionId,
    attached_session: Option<SessionId>,
    focused: WindowId,
    windows: Windows,
) -> Option<NavNode<Host>>
where
    Host: Copy + Eq,
    Windows: IntoIterator<Item = (WindowId, PaneId, Panes)>,
    Panes: IntoIterator<Item = PaneId>,
{
    if host != attached_host || Some(session) != attached_session {
        return previous;
    }
    let mut first = None;
    for (window, active, panes) in windows {
        if window != focused {
            continue;
        }
        first.get_or_insert(window);
        if panes.into_iter().any(|pane| pane == active) {
            previous = Some(NavNode::Target(host, NavTarget::Pane(active)));
        }
    }
    previous.or(Some(NavNode::Target(
        host,
        first.map_or(NavTarget::Session(session), NavTarget::Window),
    )))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavVisibleEntry<Host> {
    pub node: NavNode<Host>,
    pub parent: Option<NavNode<Host>>,
    pub host_index: usize,
    pub session_index: Option<usize>,
    pub window_index: Option<usize>,
    pub pane_index: Option<usize>,
    pub depth: u8,
    pub expandable: bool,
    pub expanded: bool,
    pub on_active_path: bool,
}

pub struct NavProjection<Host> {
    pub entries: Vec<NavVisibleEntry<Host>>,
    pub visible_indices: BTreeMap<NavNode<Host>, usize>,
    pub expandable: BTreeSet<NavNode<Host>>,
}

#[must_use]
pub fn project_tree<Host, Hosts, Sessions, Windows, Panes>(
    hosts: Hosts,
    mut expanded: impl FnMut(NavNode<Host>) -> bool,
) -> NavProjection<Host>
where
    Host: Copy + Ord,
    Hosts: Iterator<Item = (Host, Sessions)> + Clone,
    Sessions: IntoIterator<Item = (SessionId, bool, Windows)>,
    Windows: IntoIterator<Item = (WindowId, bool, PaneId, Panes)>,
    Windows::IntoIter: ExactSizeIterator,
    Panes: IntoIterator<Item = PaneId>,
    Panes::IntoIter: ExactSizeIterator,
{
    let mut expandable = BTreeSet::new();
    for (host, sessions) in hosts.clone() {
        expandable.insert(NavNode::Host(host));
        for (session, _, windows) in sessions {
            let windows = windows.into_iter();
            if windows.len() != 0 {
                expandable.insert(NavNode::Target(host, NavTarget::Session(session)));
            }
            for (window, _, _, panes) in windows {
                if panes.into_iter().len() != 0 {
                    expandable.insert(NavNode::Target(host, NavTarget::Window(window)));
                }
            }
        }
    }
    let mut entries = Vec::new();
    let mut push = |entry| entries.push(entry);
    for (host_index, (host, sessions)) in hosts.enumerate() {
        let host_node = NavNode::Host(host);
        let host_expanded = expanded(host_node);
        push(NavVisibleEntry {
            node: host_node,
            parent: None,
            host_index,
            session_index: None,
            window_index: None,
            pane_index: None,
            depth: 0,
            expandable: true,
            expanded: host_expanded,
            on_active_path: false,
        });
        if !host_expanded {
            continue;
        }
        for (session_index, (session, active, windows)) in sessions.into_iter().enumerate() {
            let node = NavNode::Target(host, NavTarget::Session(session));
            let windows = windows.into_iter();
            let can_expand = windows.len() != 0;
            let is_expanded = can_expand && expanded(node);
            push(NavVisibleEntry {
                node,
                parent: Some(host_node),
                host_index,
                session_index: Some(session_index),
                window_index: None,
                pane_index: None,
                depth: 1,
                expandable: can_expand,
                expanded: is_expanded,
                on_active_path: active,
            });
            if !is_expanded {
                continue;
            }
            for (window_index, (window, active, active_pane, panes)) in windows.enumerate() {
                let node = NavNode::Target(host, NavTarget::Window(window));
                let panes = panes.into_iter();
                let can_expand = panes.len() != 0;
                let is_expanded = can_expand && expanded(node);
                push(NavVisibleEntry {
                    node,
                    parent: Some(NavNode::Target(host, NavTarget::Session(session))),
                    host_index,
                    session_index: Some(session_index),
                    window_index: Some(window_index),
                    pane_index: None,
                    depth: 2,
                    expandable: can_expand,
                    expanded: is_expanded,
                    on_active_path: active,
                });
                if !is_expanded {
                    continue;
                }
                for (pane_index, pane) in panes.enumerate() {
                    push(NavVisibleEntry {
                        node: NavNode::Target(host, NavTarget::Pane(pane)),
                        parent: Some(NavNode::Target(host, NavTarget::Window(window))),
                        host_index,
                        session_index: Some(session_index),
                        window_index: Some(window_index),
                        pane_index: Some(pane_index),
                        depth: 3,
                        expandable: false,
                        expanded: false,
                        on_active_path: active && pane == active_pane,
                    });
                }
            }
        }
    }
    let visible_indices = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.node, index))
        .collect();
    NavProjection {
        entries,
        visible_indices,
        expandable,
    }
}

#[must_use]
pub fn toggle_expanded<Node: Copy + Ord>(expanded: &mut BTreeSet<Node>, node: Node) -> bool {
    if expanded.remove(&node) {
        false
    } else {
        expanded.insert(node);
        true
    }
}

pub fn retain_projection<Node: Copy + Ord>(
    expanded: &mut BTreeSet<Node>,
    selected: &mut Option<Node>,
    expandable: &BTreeSet<Node>,
    visible: &BTreeMap<Node, usize>,
    first: Option<Node>,
) {
    expanded.retain(|node| expandable.contains(node));
    if selected.is_some_and(|node| !visible.contains_key(&node)) {
        *selected = first;
    }
}

pub fn expand_new_hosts<Host: Copy + Ord, Node: Ord>(
    expanded: &mut BTreeSet<Node>,
    previous: impl IntoIterator<Item = Host>,
    next: impl IntoIterator<Item = Host>,
    local: Host,
    mut node: impl FnMut(Host) -> Node,
) {
    let previous: BTreeSet<_> = previous.into_iter().collect();
    expanded.extend(
        next.into_iter()
            .filter(|host| *host == local && !previous.contains(host))
            .map(&mut node),
    );
}

pub fn follow_active<Node: Copy + Eq>(
    previous: Option<Node>,
    active: Option<Node>,
    selected: &mut Option<Node>,
    mut expand: impl FnMut(Node),
) -> bool {
    if previous != active
        && let Some(node) = active
    {
        expand(node);
        *selected = Some(node);
        true
    } else {
        false
    }
}

#[must_use]
pub fn path_ancestors<Host: Copy>(
    node: NavNode<Host>,
    exists: bool,
    owner: Option<NavOwner>,
) -> Vec<NavNode<Host>> {
    let NavNode::Target(host, target) = node else {
        return Vec::new();
    };
    if !exists {
        return Vec::new();
    }
    let mut result = vec![NavNode::Host(host)];
    if let Some(owner) = owner {
        result.push(NavNode::Target(host, NavTarget::Session(owner.session)));
        if matches!(target, NavTarget::Pane(_)) {
            result.push(NavNode::Target(host, NavTarget::Window(owner.window)));
        }
    }
    result
}
