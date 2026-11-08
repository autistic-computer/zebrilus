use serde_json::{Value, json};
use std::{collections::BTreeSet, error::Error, fs};
use zz_client::{
    desktop_nav::NavTarget,
    desktop_nav_tree::{self, NavNode},
    sidebar::{self, TreeNavigation, TreeNavigationResult, TreeNavigationRow},
};
use zz_protocol::{PaneId, SessionId, WindowId};

#[derive(Clone)]
struct Window {
    id: WindowId,
    active: bool,
    pane: PaneId,
    panes: Vec<PaneId>,
    name: String,
}
#[derive(Clone)]
struct Session {
    id: SessionId,
    active: bool,
    windows: Vec<Window>,
    name: String,
}
#[derive(Clone)]
struct Host {
    id: u32,
    connected: bool,
    sessions: Vec<Session>,
}

fn node(value: NavNode<u32>) -> Value {
    match value {
        NavNode::Host(host) => json!([host, 0, 0]),
        NavNode::Target(host, target) => match target {
            NavTarget::Session(id) => json!([host, 1, id.0]),
            NavTarget::Window(id) => json!([host, 2, id.0]),
            NavTarget::Pane(id) => json!([host, 3, id.0]),
        },
    }
}

fn target(kind: u8, id: u64) -> NavTarget {
    match kind {
        0 => NavTarget::Session(SessionId(id)),
        1 => NavTarget::Window(WindowId(id)),
        _ => NavTarget::Pane(PaneId(id)),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut seed = 0x4253_b8dc_9057_3081_u64;
    let mut next = |n| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let mut cases = Vec::new();
    let mut rows_count = 0;
    let mut navigation_count = 0;
    for number in 0..96 {
        let mut hosts = Vec::new();
        for hi in 0..next(4) {
            let mut sessions = Vec::new();
            for si in 0..next(4) {
                let mut windows = Vec::new();
                for wi in 0..next(5) {
                    let panes = (0..next(5)).map(|_| PaneId(next(8))).collect();
                    windows.push(Window {
                        id: WindowId(next(6)),
                        active: next(2) != 0,
                        pane: PaneId(next(8)),
                        panes,
                        name: format!("w{number}/{hi}/{si}/{wi} λ"),
                    });
                }
                sessions.push(Session {
                    id: SessionId(next(5)),
                    active: next(2) != 0,
                    windows,
                    name: format!("s{number}/{hi}/{si} 中"),
                });
            }
            hosts.push(Host {
                id: u32::try_from(next(3))?,
                connected: next(2) != 0,
                sessions,
            });
        }
        let mut queries = Vec::new();
        for (hi, host) in hosts.iter().enumerate() {
            for kind in 0..3 {
                for id in [0, 1, 2, 3, 4, 5, 7, u64::MAX] {
                    let t = target(kind, id);
                    let owner = desktop_nav_tree::target_owner(
                        t,
                        host.sessions.iter().flat_map(|session| {
                            session.windows.iter().map(move |window| {
                                (session.id, window.id, window.panes.iter().copied())
                            })
                        }),
                    );
                    let name = desktop_nav_tree::renameable_name(
                        t,
                        host.sessions.iter().map(|session| {
                            (
                                session.id,
                                session.name.as_str(),
                                session
                                    .windows
                                    .iter()
                                    .map(|window| (window.id, window.name.as_str())),
                            )
                        }),
                    );
                    let expandable = desktop_nav_tree::target_expandable(
                        t,
                        host.sessions.iter().map(|session| {
                            (
                                session.id,
                                session
                                    .windows
                                    .iter()
                                    .map(|window| (window.id, window.panes.iter().copied())),
                            )
                        }),
                    );
                    queries.push(json!({"host_index":hi,"kind":kind,"id":id,"owner":owner.map(|v| [v.session.0,v.window.0]),"name":name,"expandable":expandable}));
                }
            }
        }
        let shortcuts: Vec<_> = desktop_nav_tree::shortcut_windows(hosts.iter().map(|host| {
            (
                host.id,
                host.connected,
                host.sessions
                    .iter()
                    .map(|session| session.windows.iter().map(|window| window.id)),
            )
        }))
        .map(|(host, window)| [u64::from(host), window.0])
        .collect();
        let mut patterns = Vec::new();
        for pattern in 0..12 {
            let mut expanded = BTreeSet::new();
            for host in &hosts {
                if pattern == 0 || next(2) != 0 {
                    expanded.insert(NavNode::Host(host.id));
                }
                for session in &host.sessions {
                    if pattern == 0 || next(2) != 0 {
                        expanded.insert(NavNode::Target(host.id, NavTarget::Session(session.id)));
                    }
                    for window in &session.windows {
                        if pattern == 0 || next(2) != 0 {
                            expanded.insert(NavNode::Target(host.id, NavTarget::Window(window.id)));
                        }
                    }
                }
            }
            expanded.insert(NavNode::Target(99, NavTarget::Pane(PaneId(u64::MAX))));
            let projection = desktop_nav_tree::project_tree(
                hosts.iter().map(|host| {
                    (
                        host.id,
                        host.sessions.iter().map(|session| {
                            (
                                session.id,
                                session.active,
                                session.windows.iter().map(|window| {
                                    (
                                        window.id,
                                        window.active,
                                        window.pane,
                                        window.panes.iter().copied(),
                                    )
                                }),
                            )
                        }),
                    )
                }),
                |node| expanded.contains(&node),
            );
            let rows: Vec<_> = projection
                .entries
                .iter()
                .map(|row| TreeNavigationRow {
                    depth: row.depth,
                    parent: row
                        .parent
                        .and_then(|p| projection.visible_indices.get(&p).copied()),
                    expandable: row.expandable,
                    expanded: row.expanded,
                })
                .collect();
            let mut navigation = Vec::new();
            for selected in std::iter::once(None).chain((0..rows.len() + 2).map(Some)) {
                for (direction, key) in [
                    TreeNavigation::Up,
                    TreeNavigation::Down,
                    TreeNavigation::First,
                    TreeNavigation::Last,
                    TreeNavigation::Left,
                    TreeNavigation::Right,
                ]
                .into_iter()
                .enumerate()
                {
                    let value = match sidebar::tree_navigation(&rows, selected, key) {
                        TreeNavigationResult::None => [0, 0],
                        TreeNavigationResult::Select(index) => [1, index],
                        TreeNavigationResult::Toggle(index) => [2, index],
                    };
                    navigation.push(json!([selected, direction, value]));
                }
            }
            rows_count += rows.len();
            navigation_count += navigation.len();
            let entries: Vec<_> = projection.entries.iter().map(|row| json!({"node":node(row.node),"parent":row.parent.map(node),"location":[Some(row.host_index),row.session_index,row.window_index,row.pane_index],"depth":row.depth,"expandable":row.expandable,"expanded":row.expanded,"active":row.on_active_path})).collect();
            patterns.push(json!({"expanded":expanded.into_iter().map(node).collect::<Vec<_>>(),"entries":entries,"indices":projection.visible_indices.into_iter().map(|(n,i)|json!([node(n),i])).collect::<Vec<_>>(),"expandable":projection.expandable.into_iter().map(node).collect::<Vec<_>>(),"navigation":navigation}));
        }
        let input: Vec<_> = hosts.iter().map(|host| json!({"id":host.id,"connected":host.connected,"sessions":host.sessions.iter().map(|session|json!({"id":session.id.0,"active":session.active,"name":session.name,"windows":session.windows.iter().map(|window|json!({"id":window.id.0,"active":window.active,"pane":window.pane.0,"name":window.name,"panes":window.panes.iter().map(|p|p.0).collect::<Vec<_>>()})).collect::<Vec<_>>()})).collect::<Vec<_>>()})).collect();
        cases.push(
            json!({"hosts":input,"queries":queries,"shortcuts":shortcuts,"patterns":patterns}),
        );
    }
    let output = json!({"cases":cases,"rows":rows_count,"navigation":navigation_count});
    fs::write(
        std::env::args().nth(1).ok_or("output path required")?,
        serde_json::to_vec(&output)?,
    )?;
    Ok(())
}
