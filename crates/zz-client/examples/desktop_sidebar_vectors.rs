use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeSet, error::Error, fs};
use zz_client::{
    desktop_nav::NavTarget,
    desktop_nav_tree::{self as nav, NavNode, NavProjection},
    sidebar::{self, TreeNavigation, TreeNavigationResult, TreeNavigationRow},
};
use zz_protocol::{PaneId, SessionId, WindowId};

#[derive(Deserialize)]
struct Window {
    id: u64,
    active: bool,
    pane: u64,
    panes: Vec<u64>,
}
#[derive(Deserialize)]
struct Session {
    id: u64,
    active: bool,
    windows: Vec<Window>,
}
#[derive(Deserialize)]
struct Host {
    id: u32,
    sessions: Vec<Session>,
}
#[derive(Deserialize)]
struct Case {
    hosts: Vec<Host>,
}
#[derive(Deserialize)]
struct Data {
    cases: Vec<Case>,
}

fn encode(node: NavNode<u32>) -> Value {
    match node {
        NavNode::Host(h) => json!([h, 0, 0]),
        NavNode::Target(h, t) => match t {
            NavTarget::Session(id) => json!([h, 1, id.0]),
            NavTarget::Window(id) => json!([h, 2, id.0]),
            NavTarget::Pane(id) => json!([h, 3, id.0]),
        },
    }
}

fn project(hosts: &[Host], expanded: &BTreeSet<NavNode<u32>>) -> NavProjection<u32> {
    nav::project_tree(
        hosts.iter().map(|h| {
            (
                h.id,
                h.sessions.iter().map(|s| {
                    (
                        SessionId(s.id),
                        s.active,
                        s.windows.iter().map(|w| {
                            (
                                WindowId(w.id),
                                w.active,
                                PaneId(w.pane),
                                w.panes.iter().copied().map(PaneId),
                            )
                        }),
                    )
                }),
            )
        }),
        |n| expanded.contains(&n),
    )
}

fn expand(expanded: &mut BTreeSet<NavNode<u32>>, hosts: &[Host], node: NavNode<u32>) {
    let host = match node {
        NavNode::Host(h) | NavNode::Target(h, _) => hosts.iter().find(|v| v.id == h),
    };
    let owner = match node {
        NavNode::Target(_, t) => host.and_then(|h| {
            nav::target_owner(
                t,
                h.sessions.iter().flat_map(|s| {
                    s.windows.iter().map(move |w| {
                        (
                            SessionId(s.id),
                            WindowId(w.id),
                            w.panes.iter().copied().map(PaneId),
                        )
                    })
                }),
            )
        }),
        NavNode::Host(_) => None,
    };
    expanded.extend(nav::path_ancestors(node, host.is_some(), owner));
}

fn rebuild(
    hosts: &[Host],
    expanded: &mut BTreeSet<NavNode<u32>>,
    selected: &mut Option<NavNode<u32>>,
) -> NavProjection<u32> {
    let p = project(hosts, expanded);
    nav::retain_projection(
        expanded,
        selected,
        &p.expandable,
        &p.visible_indices,
        hosts.first().map(|h| NavNode::Host(h.id)),
    );
    p
}

fn expandable(hosts: &[Host], node: NavNode<u32>) -> bool {
    match node {
        NavNode::Host(h) => hosts.iter().any(|v| v.id == h),
        NavNode::Target(h, t) => hosts.iter().find(|v| v.id == h).is_some_and(|h| {
            nav::target_expandable(
                t,
                h.sessions.iter().map(|s| {
                    (
                        SessionId(s.id),
                        s.windows
                            .iter()
                            .map(|w| (WindowId(w.id), w.panes.iter().copied().map(PaneId))),
                    )
                }),
            )
        }),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let data: Data = serde_json::from_slice(&fs::read(args.get(1).ok_or("input path required")?)?)?;
    let mut seed = 0xc2b1_b078_d24a_1897_u64;
    let mut next = |n| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let mut traces = Vec::new();
    for _ in 0..64 {
        let mut current = None;
        let mut selected = Some(NavNode::Host(0));
        let mut expanded = BTreeSet::new();
        let mut pointer = false;
        let mut active = None;
        let mut projection = project(&[], &expanded);
        let mut events = Vec::new();
        for step in 0..96 {
            let mut scroll = None;
            let op = if step == 0 { 0 } else { next(7) };
            let mut chosen = NavNode::Host(u32::try_from(next(4))?);
            if next(4) != 0 {
                chosen = NavNode::Target(
                    u32::try_from(next(4))?,
                    match next(3) {
                        0 => NavTarget::Session(SessionId(next(7))),
                        1 => NavTarget::Window(WindowId(next(7))),
                        _ => NavTarget::Pane(PaneId(next(9))),
                    },
                );
            }
            let mut input = json!([op, encode(chosen)]);
            let hosts: &[Host] = current.map_or(&[], |i: usize| data.cases[i].hosts.as_slice());
            match op {
                0 => {
                    let index = usize::try_from(next(u64::try_from(data.cases.len())?))?;
                    let new_hosts = &data.cases[index].hosts;
                    let attached = u32::try_from(next(4))?;
                    let session = (next(4) != 0).then(|| SessionId(next(6)));
                    let focused = WindowId(next(7));
                    let mut new_active = None;
                    for h in new_hosts {
                        for s in &h.sessions {
                            new_active = nav::active_for_session(
                                new_active,
                                h.id,
                                attached,
                                SessionId(s.id),
                                session,
                                focused,
                                s.windows.iter().map(|w| {
                                    (
                                        WindowId(w.id),
                                        PaneId(w.pane),
                                        w.panes.iter().copied().map(PaneId),
                                    )
                                }),
                            );
                        }
                    }
                    input = json!([op, index, attached, session.map(|s| s.0), focused.0]);
                    nav::expand_new_hosts(
                        &mut expanded,
                        hosts.iter().map(|h| h.id),
                        new_hosts.iter().map(|h| h.id),
                        0,
                        NavNode::Host,
                    );
                    let previous = active;
                    active = new_active;
                    nav::follow_active(previous, active, &mut selected, |n| {
                        expand(&mut expanded, new_hosts, n);
                    });
                    projection = rebuild(new_hosts, &mut expanded, &mut selected);
                    if previous != active {
                        scroll = active.and_then(|n| projection.visible_indices.get(&n).copied());
                    }
                    current = Some(index);
                }
                1 => {
                    pointer = true;
                    selected = Some(chosen);
                }
                2 | 6 => {
                    if op == 2 {
                        pointer = true;
                    }
                    if expandable(hosts, chosen) {
                        let _ = nav::toggle_expanded(&mut expanded, chosen);
                        selected = Some(chosen);
                        projection = rebuild(hosts, &mut expanded, &mut selected);
                    } else if op == 2 {
                        selected = Some(chosen);
                    }
                }
                3 => {
                    let d = usize::try_from(next(6))?;
                    input = json!([op, d]);
                    let rows: Vec<_> = projection
                        .entries
                        .iter()
                        .map(|r| TreeNavigationRow {
                            depth: r.depth,
                            parent: r
                                .parent
                                .and_then(|n| projection.visible_indices.get(&n).copied()),
                            expandable: r.expandable,
                            expanded: r.expanded,
                        })
                        .collect();
                    let selected_index =
                        selected.and_then(|n| projection.visible_indices.get(&n).copied());
                    match sidebar::tree_navigation(
                        &rows,
                        selected_index,
                        [
                            TreeNavigation::Up,
                            TreeNavigation::Down,
                            TreeNavigation::First,
                            TreeNavigation::Last,
                            TreeNavigation::Left,
                            TreeNavigation::Right,
                        ][d],
                    ) {
                        TreeNavigationResult::None => {}
                        TreeNavigationResult::Select(i) => {
                            pointer = false;
                            selected = Some(projection.entries[i].node);
                            scroll = Some(i);
                        }
                        TreeNavigationResult::Toggle(i) => {
                            pointer = false;
                            let n = projection.entries[i].node;
                            if expandable(hosts, n) {
                                let _ = nav::toggle_expanded(&mut expanded, n);
                                selected = Some(n);
                                projection = rebuild(hosts, &mut expanded, &mut selected);
                            }
                        }
                    }
                }
                4 => {
                    if let Some(n) = active {
                        expand(&mut expanded, hosts, n);
                        pointer = false;
                        selected = Some(n);
                        projection = rebuild(hosts, &mut expanded, &mut selected);
                        scroll = projection.visible_indices.get(&n).copied();
                    }
                }
                _ => {
                    let i = usize::try_from(next(64))?;
                    input = json!([op, i]);
                    if let Some(r) = projection.entries.get(i) {
                        pointer = false;
                        selected = Some(r.node);
                        scroll = Some(i);
                    }
                }
            }
            events.push(json!({"input":input,"selected":selected.map(encode),"active":active.map(encode),"pointer":pointer,"expanded":expanded.iter().copied().map(encode).collect::<Vec<_>>(),"scroll":scroll,"rows":projection.entries.iter().map(|r|encode(r.node)).collect::<Vec<_>>()}));
        }
        traces.push(events);
    }
    fs::write(
        args.get(2).ok_or("output path required")?,
        serde_json::to_vec(&traces)?,
    )?;
    Ok(())
}
