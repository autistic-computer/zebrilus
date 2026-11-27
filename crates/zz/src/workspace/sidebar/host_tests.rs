use std::collections::BTreeSet;

use zz_protocol::{SessionSnapshot, WindowId};

use super::test_support::*;
use super::*;

#[test]
fn sidebar_host_row_formats_the_hostname_with_a_local_fallback() {
    assert_eq!(
        sidebar_hostname(Some("studio.local")).as_ref(),
        "studio.local"
    );
    assert_eq!(sidebar_hostname(Some("  ")).as_ref(), "localhost");
    assert_eq!(sidebar_hostname(None).as_ref(), "localhost");
}

#[test]
fn an_added_host_takes_the_next_row_while_it_connects() {
    let studio = host_ids(&["studio"])[0];
    let connected = HostState::Connected;
    let snapshot = snapshot_with_two_panes();
    let before = MuxTreeModel::from_hosts(
        HostId::LOCAL,
        Some(SessionId(1)),
        [(HostId::LOCAL, "local", &connected, Some(&snapshot))],
    );
    assert_eq!(
        TreeProjection::new(&before, &BTreeSet::new())
            .entries
            .iter()
            .map(|entry| entry.node)
            .collect::<Vec<_>>(),
        [TreeNode::Host(HostId::LOCAL)]
    );

    let connecting = HostState::Connecting;
    let after = MuxTreeModel::from_hosts(
        HostId::LOCAL,
        Some(SessionId(1)),
        [
            (HostId::LOCAL, "local", &connected, Some(&snapshot)),
            (studio, "studio", &connecting, None),
        ],
    );
    assert_eq!(
        TreeProjection::new(&after, &BTreeSet::new())
            .entries
            .iter()
            .map(|entry| entry.node)
            .collect::<Vec<_>>(),
        [TreeNode::Host(HostId::LOCAL), TreeNode::Host(studio)]
    );
    assert_eq!(
        after.host(studio).and_then(MuxTreeHost::indicator),
        Some(HostIndicator::Connecting)
    );
}

#[test]
fn remote_hosts_project_their_cached_snapshot_beside_the_local_tree() {
    let remote = host_ids(&["studio"])[0];
    let connected = HostState::Connected;
    let local_snapshot = snapshot_with_two_panes();
    let remote_snapshot = remote_snapshot();
    let model = MuxTreeModel::from_hosts(
        remote,
        Some(SessionId(1)),
        [
            (HostId::LOCAL, "local", &connected, Some(&local_snapshot)),
            (remote, "studio", &connected, Some(&remote_snapshot)),
        ],
    );
    let local_session = TreeNode::Target(HostId::LOCAL, TreeTarget::Session(SessionId(1)));
    let local_window = TreeNode::Target(HostId::LOCAL, TreeTarget::Window(WindowId(11)));
    let remote_session = TreeNode::Target(remote, TreeTarget::Session(SessionId(1)));
    let remote_window = TreeNode::Target(remote, TreeTarget::Window(WindowId(11)));
    let remote_pane = TreeNode::Target(remote, TreeTarget::Pane(PaneId(202)));
    let expanded = BTreeSet::from([
        TreeNode::Host(HostId::LOCAL),
        TreeNode::Host(remote),
        local_session,
        local_window,
        remote_session,
        remote_window,
    ]);
    let projection = TreeProjection::new(&model, &expanded);

    assert_eq!(
        model.hosts.iter().map(|host| host.id).collect::<Vec<_>>(),
        [HostId::LOCAL, remote]
    );
    assert!(model.is_expandable(TreeNode::Host(remote)));
    assert!(model.is_expandable(remote_session));
    assert!(model.is_expandable(remote_window));
    assert_eq!(model.active_target, Some(remote_pane));
    assert!(row_is_active(model.active_target, remote_pane));
    assert_eq!(
        projection
            .entries
            .iter()
            .map(|entry| (entry.node, entry.depth))
            .collect::<Vec<_>>(),
        [
            (TreeNode::Host(HostId::LOCAL), 0),
            (local_session, 1),
            (local_window, 2),
            (
                TreeNode::Target(HostId::LOCAL, TreeTarget::Pane(PaneId(202))),
                3,
            ),
            (
                TreeNode::Target(HostId::LOCAL, TreeTarget::Pane(PaneId(101))),
                3,
            ),
            (TreeNode::Host(remote), 0),
            (remote_session, 1),
            (remote_window, 2),
            (remote_pane, 3),
            (
                TreeNode::Target(remote, TreeTarget::Session(SessionId(9))),
                1
            ),
        ]
    );
    assert!(
        projection.entries[projection.visible_indices[&remote_pane]].on_active_path,
        "the attached machine's focused pane row reads as active"
    );

    let collapsed = TreeProjection::new(&model, &BTreeSet::from([TreeNode::Host(HostId::LOCAL)]));
    assert!(collapsed.expandable.contains(&TreeNode::Host(remote)));
    assert!(!collapsed.visible_indices.contains_key(&remote_session));
}

#[test]
fn window_shortcuts_count_windows_across_sessions_and_connected_hosts() {
    let hosts = host_ids(&["studio", "attic"]);
    let (studio, attic) = (hosts[0], hosts[1]);
    let connected = HostState::Connected;
    let disconnected = HostState::Disconnected;
    let session = |id: u64, windows: &[u64]| SessionSnapshot {
        id: SessionId(id),
        name: id.to_string(),
        active_window: WindowId(windows[0]),
        windows: windows
            .iter()
            .map(|&window| mux_window(window, 0, "shell", window * 10))
            .collect(),
        viewers: Vec::new(),
    };
    let snapshot = |sessions| MuxSnapshot {
        generation: 1,
        focused_window: None,
        sessions,
    };
    let local = snapshot(vec![session(2, &[21, 22, 23]), session(0, &[1])]);
    let attic_snapshot = snapshot(vec![session(5, &[51])]);
    let studio_snapshot = snapshot((10..16).map(|id| session(id, &[id * 10])).collect());
    let model = MuxTreeModel::from_hosts(
        HostId::LOCAL,
        Some(SessionId(0)),
        [
            (HostId::LOCAL, "local", &connected, Some(&local)),
            (attic, "attic", &disconnected, Some(&attic_snapshot)),
            (studio, "studio", &connected, Some(&studio_snapshot)),
        ],
    );
    let local_window = |id| (HostId::LOCAL, WindowId(id));
    let studio_window = |id| (studio, WindowId(id));
    assert_eq!(
        shortcut_windows(&model),
        [
            local_window(21),
            local_window(22),
            local_window(23),
            local_window(1),
            studio_window(100),
            studio_window(110),
            studio_window(120),
            studio_window(130),
            studio_window(140),
        ]
    );
}

#[test]
fn remote_rows_attach_directly_to_the_clicked_row() {
    let remote = host_ids(&["studio"])[0];
    let connected = HostState::Connected;
    let disconnected = HostState::Disconnected;
    let local_snapshot = snapshot_with_two_panes();
    let remote_snapshot = remote_snapshot();
    let model = MuxTreeModel::from_hosts(
        HostId::LOCAL,
        Some(SessionId(1)),
        [
            (HostId::LOCAL, "local", &connected, Some(&local_snapshot)),
            (remote, "studio", &connected, Some(&remote_snapshot)),
        ],
    );
    let activation = |node| model.activation_for_node(node, HostId::LOCAL, Some(SessionId(1)));

    assert_eq!(
        activation(TreeNode::Target(remote, TreeTarget::Session(SessionId(1)))),
        Some(SidebarActivation::Attach {
            host: remote,
            session: SessionId(1),
        })
    );
    assert_eq!(
        activation(TreeNode::Target(remote, TreeTarget::Session(SessionId(9)))),
        Some(SidebarActivation::Attach {
            host: remote,
            session: SessionId(9),
        })
    );
    assert_eq!(
        activation(TreeNode::Target(remote, TreeTarget::Window(WindowId(21)))),
        Some(SidebarActivation::AttachAt {
            host: remote,
            target: "$9:@21".to_owned(),
        })
    );
    assert_eq!(
        activation(TreeNode::Target(remote, TreeTarget::Pane(PaneId(303)))),
        Some(SidebarActivation::AttachAt {
            host: remote,
            target: "$9:@21.%303".to_owned(),
        })
    );
    assert_eq!(
        activation(TreeNode::Target(remote, TreeTarget::Pane(PaneId(202)))),
        Some(SidebarActivation::AttachAt {
            host: remote,
            target: "$1:@11.%202".to_owned(),
        })
    );
    assert_eq!(
        activation(TreeNode::Host(remote)),
        Some(SidebarActivation::AttachHost(remote))
    );

    let offline = MuxTreeModel::from_hosts(
        HostId::LOCAL,
        Some(SessionId(1)),
        [
            (HostId::LOCAL, "local", &connected, Some(&local_snapshot)),
            (remote, "studio", &disconnected, Some(&remote_snapshot)),
        ],
    );
    assert_eq!(
        offline.activation_for_node(
            TreeNode::Target(remote, TreeTarget::Session(SessionId(9))),
            HostId::LOCAL,
            Some(SessionId(1)),
        ),
        None,
        "a stale subtree stays inert until its machine is back"
    );
    assert_eq!(
        offline.activation_for_node(TreeNode::Host(remote), HostId::LOCAL, Some(SessionId(1))),
        Some(SidebarActivation::Reconnect(remote))
    );
}

#[test]
fn every_expandable_row_carries_a_disclosure_even_when_clicking_it_activates() {
    let remote = host_ids(&["studio"])[0];
    let connected = HostState::Connected;
    let local_snapshot = snapshot_with_two_panes();
    let remote_snapshot = remote_snapshot();
    let model = MuxTreeModel::from_hosts(
        HostId::LOCAL,
        Some(SessionId(1)),
        [
            (HostId::LOCAL, "local", &connected, Some(&local_snapshot)),
            (remote, "studio", &connected, Some(&remote_snapshot)),
        ],
    );
    let remote_host = TreeNode::Host(remote);
    let remote_session = TreeNode::Target(remote, TreeTarget::Session(SessionId(9)));
    let remote_window = TreeNode::Target(remote, TreeTarget::Window(WindowId(21)));
    let remote_pane = TreeNode::Target(remote, TreeTarget::Pane(PaneId(303)));

    assert!(
        model
            .activation_for_node(remote_host, HostId::LOCAL, Some(SessionId(1)))
            .is_some()
    );

    let collapsed = TreeProjection::new(&model, &BTreeSet::new());
    let row = |projection: &TreeProjection, node| {
        let entry = &projection.entries[projection.visible_indices[&node]];
        (entry.expandable, entry.expanded)
    };
    assert_eq!(row(&collapsed, remote_host), (true, false));

    let expanded = TreeProjection::new(
        &model,
        &BTreeSet::from([remote_host, remote_session, remote_window]),
    );
    assert_eq!(row(&expanded, remote_host), (true, true));
    assert_eq!(row(&expanded, remote_session), (true, true));
    assert_eq!(row(&expanded, remote_window), (true, true));
    assert_eq!(row(&expanded, remote_pane), (false, false));
    let half_open = TreeProjection::new(&model, &BTreeSet::from([remote_host]));
    assert_eq!(row(&half_open, remote_session), (true, false));
}

#[test]
fn row_controls_and_rename_actions_belong_to_their_machine() {
    let remote = host_ids(&["studio"])[0];
    let connected = HostState::Connected;
    let local_snapshot = snapshot_with_two_panes();
    let remote_snapshot = remote_snapshot();
    let model = MuxTreeModel::from_hosts(
        remote,
        Some(SessionId(9)),
        [
            (HostId::LOCAL, "local", &connected, Some(&local_snapshot)),
            (remote, "studio", &connected, Some(&remote_snapshot)),
        ],
    );
    let projection = TreeProjection::new(
        &model,
        &BTreeSet::from([
            TreeNode::Host(HostId::LOCAL),
            TreeNode::Target(HostId::LOCAL, TreeTarget::Session(SessionId(1))),
            TreeNode::Target(HostId::LOCAL, TreeTarget::Window(WindowId(11))),
            TreeNode::Host(remote),
            TreeNode::Target(remote, TreeTarget::Session(SessionId(9))),
            TreeNode::Target(remote, TreeTarget::Window(WindowId(21))),
        ]),
    );
    let actions = |node| node_actions(&projection.entries[projection.visible_indices[&node]]);

    assert_eq!(
        actions(TreeNode::Host(remote)),
        [NodeAction::NewSession(remote)]
    );
    assert_eq!(
        actions(TreeNode::Target(remote, TreeTarget::Session(SessionId(9)))),
        [
            NodeAction::NewWindow(remote, SessionId(9)),
            NodeAction::Delete(remote, TreeTarget::Session(SessionId(9))),
        ]
    );
    assert_eq!(
        actions(TreeNode::Target(remote, TreeTarget::Window(WindowId(21)))),
        [NodeAction::Delete(remote, TreeTarget::Window(WindowId(21)))]
    );
    assert_eq!(
        actions(TreeNode::Target(remote, TreeTarget::Pane(PaneId(303)))),
        [NodeAction::Delete(remote, TreeTarget::Pane(PaneId(303)))]
    );
    assert_eq!(
        actions(TreeNode::Target(
            HostId::LOCAL,
            TreeTarget::Window(WindowId(11))
        )),
        [NodeAction::Delete(
            HostId::LOCAL,
            TreeTarget::Window(WindowId(11))
        )]
    );

    assert_eq!(
        model.renameable_name(remote, TreeTarget::Session(SessionId(9))),
        Some("builds")
    );
    assert_eq!(
        model.renameable_name(remote, TreeTarget::Window(WindowId(21))),
        Some("compile")
    );
    assert_eq!(
        model.rename_activation_for_node(
            TreeNode::Target(remote, TreeTarget::Window(WindowId(21))),
            remote,
        ),
        Some((
            "Rename Window…",
            SidebarActivation::Execute {
                host: remote,
                commands: vec![
                    rename_prompt_command(TreeTarget::Window(WindowId(21)), "compile")
                        .unwrap()
                        .1,
                ],
            },
        ))
    );
    assert_eq!(
        model.rename_activation_for_node(
            TreeNode::Target(HostId::LOCAL, TreeTarget::Pane(PaneId(202))),
            remote,
        ),
        Some((
            "Rename Window…",
            SidebarActivation::AttachThenExecute {
                host: HostId::LOCAL,
                session: SessionId(1),
                commands: vec![
                    rename_prompt_command(TreeTarget::Window(WindowId(11)), "work")
                        .unwrap()
                        .1,
                ],
            },
        ))
    );

    let disconnected = HostState::Disconnected;
    let offline = MuxTreeModel::from_hosts(
        remote,
        Some(SessionId(9)),
        [
            (HostId::LOCAL, "local", &disconnected, Some(&local_snapshot)),
            (remote, "studio", &connected, Some(&remote_snapshot)),
        ],
    );
    assert_eq!(
        offline.rename_activation_for_node(
            TreeNode::Target(HostId::LOCAL, TreeTarget::Window(WindowId(11))),
            remote,
        ),
        None,
    );
}

#[test]
fn attaching_to_a_machine_expands_it_down_to_the_active_pane() {
    let remote = host_ids(&["studio"])[0];
    let connected = HostState::Connected;
    let local_snapshot = snapshot_with_two_panes();
    let remote_snapshot = remote_snapshot();
    let model = MuxTreeModel::from_hosts(
        remote,
        Some(SessionId(9)),
        [
            (HostId::LOCAL, "local", &connected, Some(&local_snapshot)),
            (remote, "studio", &connected, Some(&remote_snapshot)),
        ],
    );
    let active = model
        .active_target
        .expect("the attached machine names an active pane");
    let mut expanded = BTreeSet::from([TreeNode::Host(HostId::LOCAL)]);

    expand_path_to(&mut expanded, &model, active);

    assert_eq!(
        active,
        TreeNode::Target(remote, TreeTarget::Pane(PaneId(303)))
    );
    assert_eq!(
        expanded,
        BTreeSet::from([
            TreeNode::Host(HostId::LOCAL),
            TreeNode::Host(remote),
            TreeNode::Target(remote, TreeTarget::Session(SessionId(9))),
            TreeNode::Target(remote, TreeTarget::Window(WindowId(21))),
        ])
    );
    let projection = TreeProjection::new(&model, &expanded);
    assert!(projection.visible_indices.contains_key(&active));
}

#[test]
fn host_indicators_cover_loading_connecting_and_failure_states() {
    let ids = host_ids(&[
        "ready", "loading", "dialing", "roaming", "offline", "skewed", "idle",
    ]);
    let connected = HostState::Connected;
    let connecting = HostState::Connecting;
    let reconnecting = HostState::Reconnecting { attempt: 4 };
    let unreachable = HostState::Unreachable {
        reason: "connection refused\nssh detail".to_owned(),
    };
    let incompatible = HostState::Incompatible {
        local: 33,
        remote: Some(31),
    };
    let disconnected = HostState::Disconnected;
    let loaded_snapshot = MuxSnapshot::default();
    let model = MuxTreeModel::from_hosts(
        HostId::LOCAL,
        None,
        [
            (ids[0], "ready", &connected, Some(&loaded_snapshot)),
            (ids[1], "loading", &connected, None),
            (ids[2], "dialing", &connecting, None),
            (ids[3], "roaming", &reconnecting, None),
            (ids[4], "offline", &unreachable, None),
            (ids[5], "skewed", &incompatible, None),
            (ids[6], "idle", &disconnected, None),
        ],
    );

    assert_eq!(
        model
            .hosts
            .iter()
            .map(MuxTreeHost::indicator)
            .collect::<Vec<_>>(),
        [
            None,
            Some(HostIndicator::Connecting),
            Some(HostIndicator::Connecting),
            Some(HostIndicator::Connecting),
            Some(HostIndicator::Failed {
                detail: Some("connection refused\nssh detail".into()),
            }),
            Some(HostIndicator::Failed {
                detail: Some(
                    "This zz speaks protocol v33; that machine speaks v31.\nUpgrade whichever \
                     side is older, then reconnect."
                        .into(),
                ),
            }),
            Some(HostIndicator::Failed { detail: None }),
        ]
    );
}

#[test]
fn host_activation_switches_connected_machines_and_retries_failed_rows() {
    let ids = host_ids(&["connected", "roaming", "offline", "skewed"]);
    let connected = HostState::Connected;
    let reconnecting = HostState::Reconnecting { attempt: 2 };
    let unreachable = HostState::Unreachable {
        reason: "connection refused".to_owned(),
    };
    let incompatible = HostState::Incompatible {
        local: 33,
        remote: Some(31),
    };
    let remote_snapshot = MuxSnapshot {
        generation: 2,
        focused_window: None,
        sessions: vec![SessionSnapshot {
            id: SessionId(7),
            name: "remote".to_owned(),
            active_window: WindowId(0),
            windows: Vec::new(),
            viewers: Vec::new(),
        }],
    };
    let model = MuxTreeModel::from_hosts(
        HostId::LOCAL,
        Some(SessionId(1)),
        [
            (ids[0], "connected", &connected, Some(&remote_snapshot)),
            (ids[1], "roaming", &reconnecting, None),
            (ids[2], "offline", &unreachable, None),
            (ids[3], "skewed", &incompatible, None),
        ],
    );

    assert_eq!(
        model.activation_for_node(TreeNode::Host(ids[0]), HostId::LOCAL, Some(SessionId(1)),),
        Some(SidebarActivation::AttachHost(ids[0]))
    );
    assert_eq!(
        model.activation_for_node(TreeNode::Host(ids[1]), HostId::LOCAL, Some(SessionId(1)),),
        Some(SidebarActivation::Reconnect(ids[1]))
    );
    assert_eq!(
        model.activation_for_node(TreeNode::Host(ids[2]), HostId::LOCAL, Some(SessionId(1)),),
        Some(SidebarActivation::Reconnect(ids[2]))
    );
    assert_eq!(
        model.activation_for_node(TreeNode::Host(ids[3]), HostId::LOCAL, Some(SessionId(1)),),
        Some(SidebarActivation::Reconnect(ids[3]))
    );

    let local_snapshot = MuxSnapshot::default();
    let switch_back_model = MuxTreeModel::from_hosts(
        ids[0],
        Some(SessionId(7)),
        [
            (HostId::LOCAL, "local", &connected, Some(&local_snapshot)),
            (ids[0], "connected", &connected, Some(&remote_snapshot)),
        ],
    );
    assert_eq!(
        switch_back_model.activation_for_node(
            TreeNode::Host(HostId::LOCAL),
            ids[0],
            Some(SessionId(7)),
        ),
        Some(SidebarActivation::AttachHost(HostId::LOCAL))
    );
    assert_eq!(
        switch_back_model.activation_for_node(TreeNode::Host(ids[0]), ids[0], Some(SessionId(7)),),
        None
    );

    let local_model = MuxTreeModel::from_hosts(
        HostId::LOCAL,
        Some(SessionId(1)),
        [(HostId::LOCAL, "local", &unreachable, None)],
    );
    assert_eq!(
        local_model.activation_for_node(
            TreeNode::Host(HostId::LOCAL),
            HostId::LOCAL,
            Some(SessionId(1)),
        ),
        Some(SidebarActivation::Reconnect(HostId::LOCAL))
    );
}

#[test]
fn only_the_local_machine_auto_expands_when_it_joins_the_fleet() {
    let ids = host_ids(&["studio", "server"]);
    let disconnected = HostState::Disconnected;
    let previous = MuxTreeModel::from_hosts(
        HostId::LOCAL,
        None,
        [
            (HostId::LOCAL, "local", &disconnected, None),
            (ids[0], "studio", &disconnected, None),
        ],
    );
    let next = MuxTreeModel::from_hosts(
        HostId::LOCAL,
        None,
        [
            (HostId::LOCAL, "local", &disconnected, None),
            (ids[0], "studio", &disconnected, None),
            (ids[1], "server", &disconnected, None),
        ],
    );
    let mut expanded = BTreeSet::new();

    expand_new_hosts(&mut expanded, &MuxTreeModel::default(), &previous);
    assert_eq!(expanded, BTreeSet::from([TreeNode::Host(HostId::LOCAL)]),);

    expand_new_hosts(&mut expanded, &previous, &next);

    assert_eq!(expanded, BTreeSet::from([TreeNode::Host(HostId::LOCAL)]));
}
