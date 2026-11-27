use std::collections::BTreeSet;

use zz_protocol::{PaneKindSnapshot, PaneSnapshot, PaneSummary, SessionSnapshot, WindowId};

use super::test_support::*;
use super::*;

#[test]
fn only_the_active_workspace_row_reads_as_active() {
    let pane = TreeNode::Target(HostId::LOCAL, TreeTarget::Pane(PaneId(7)));
    let other_pane = TreeNode::Target(HostId::LOCAL, TreeTarget::Pane(PaneId(8)));
    let window = TreeNode::Target(HostId::LOCAL, TreeTarget::Window(WindowId(3)));
    let remote_host = TreeNode::Host(host_ids(&["remote"])[0]);

    assert!(row_is_active(Some(pane), pane));
    assert!(!row_is_active(Some(pane), other_pane,));
    assert!(!row_is_active(Some(pane), window));
    assert!(row_is_active(Some(window), window));
    assert!(row_is_active(Some(remote_host), remote_host));
    assert!(!row_is_active(None, pane));
    assert!(!row_is_active(Some(pane), TreeNode::Host(HostId::LOCAL)));
}

#[test]
fn flattened_projection_starts_with_the_local_host() {
    let snapshot = snapshot_with_two_panes();
    let model = local_model(&snapshot, Some(SessionId(1)));
    let expanded = BTreeSet::from([
        TreeNode::Host(HostId::LOCAL),
        TreeNode::Target(HostId::LOCAL, TreeTarget::Session(SessionId(1))),
        TreeNode::Target(HostId::LOCAL, TreeTarget::Window(WindowId(11))),
    ]);
    let projection = TreeProjection::new(&model, &expanded);

    assert_eq!(
        projection
            .entries
            .iter()
            .map(|entry| entry.depth)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 3]
    );
    assert_eq!(projection.entries[0].node, TreeNode::Host(HostId::LOCAL));
    assert!(matches!(projection.entries[0].kind, TreeNodeKind::Host));
    assert!(projection.entries[0].expandable);
    assert_eq!(
        projection.entries.last().map(|entry| entry.node),
        Some(TreeNode::Target(
            HostId::LOCAL,
            TreeTarget::Pane(PaneId(101))
        ))
    );
}

#[test]
fn active_hierarchy_marks_session_window_and_focused_pane() {
    let snapshot = snapshot_with_two_panes();
    let model = local_model(&snapshot, Some(SessionId(1)));
    let expanded = BTreeSet::from([
        TreeNode::Host(HostId::LOCAL),
        TreeNode::Target(HostId::LOCAL, TreeTarget::Session(SessionId(1))),
        TreeNode::Target(HostId::LOCAL, TreeTarget::Window(WindowId(11))),
    ]);
    let projection = TreeProjection::new(&model, &expanded);

    assert_eq!(
        projection
            .entries
            .iter()
            .map(|entry| entry.on_active_path)
            .collect::<Vec<_>>(),
        vec![false, true, true, true, false]
    );
}

#[test]
fn panes_and_folded_windows_wear_the_program_running_in_them() {
    let mut snapshot = snapshot_with_two_panes();
    let window = &mut snapshot.sessions[0].windows[0];
    window.active_pane = PaneId(101);
    window.panes.get_mut(&PaneId(101)).unwrap().summary.process =
        vec!["/opt/homebrew/bin/nvim".to_owned()];
    let model = local_model(&snapshot, Some(SessionId(1)));
    let programs = |expanded: &BTreeSet<TreeNode>| {
        TreeProjection::new(&model, expanded)
            .entries
            .iter()
            .map(|entry| entry.program.map(|program| program.id))
            .collect::<Vec<_>>()
    };
    let mut expanded = BTreeSet::from([
        TreeNode::Host(HostId::LOCAL),
        TreeNode::Target(HostId::LOCAL, TreeTarget::Session(SessionId(1))),
    ]);

    assert_eq!(programs(&expanded), [None, None, Some("neovim")]);

    expanded.insert(TreeNode::Target(
        HostId::LOCAL,
        TreeTarget::Window(WindowId(11)),
    ));
    assert_eq!(
        programs(&expanded),
        [None, None, None, None, Some("neovim")]
    );
}

#[test]
fn stamped_window_focus_drives_the_local_active_tree() {
    let mut snapshot = snapshot_with_two_panes();
    let focused_window = mux_window(12, 3, "logs", 303);
    snapshot.sessions[0].windows.push(focused_window);
    snapshot.focused_window = Some(WindowId(12));

    let model = local_model(&snapshot, Some(SessionId(1)));
    let session = &model.host(HostId::LOCAL).unwrap().sessions[0];
    assert!(!session.windows[0].active);
    assert!(session.windows[1].active);
    assert_eq!(
        model.active_target,
        Some(TreeNode::Target(
            HostId::LOCAL,
            TreeTarget::Pane(PaneId(303))
        ))
    );
    assert_eq!(
        active_tree_target(&snapshot, Some(SessionId(1))),
        Some(TreeTarget::Pane(PaneId(303)))
    );
}

#[test]
fn projection_is_exactly_session_window_pane_and_uses_layout_order() {
    let snapshot = snapshot_with_two_panes();
    let model = local_model(&snapshot, Some(SessionId(1)));
    let sessions = &model.host(HostId::LOCAL).unwrap().sessions;

    assert_eq!(model.max_depth(), 3);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].windows.len(), 1);
    assert_eq!(
        sessions[0].windows[0]
            .panes
            .iter()
            .map(|pane| pane.id)
            .collect::<Vec<_>>(),
        vec![PaneId(202), PaneId(101)]
    );
    assert_eq!(sessions[0].windows[0].panes[0].label, "https://zed.dev");
    assert_eq!(
        sessions[0].windows[0].panes[0].kind,
        MuxTreePaneKind::Browser
    );
    assert_eq!(sessions[0].windows[0].panes[1].label, "terminal");
    assert_eq!(
        sessions[0].windows[0].panes[1].kind,
        MuxTreePaneKind::Terminal
    );
    assert_eq!(
        model.active_target,
        Some(TreeNode::Target(
            HostId::LOCAL,
            TreeTarget::Pane(PaneId(202))
        ))
    );
}

#[test]
fn sidebar_window_labels_omit_the_window_index() {
    let snapshot = snapshot_with_two_panes();
    let model = local_model(&snapshot, Some(SessionId(1)));

    assert_eq!(
        model.host(HostId::LOCAL).unwrap().sessions[0].windows[0].label(),
        "work"
    );
}

#[test]
fn pending_picker_panes_have_a_distinct_sidebar_kind_and_fallback_label() {
    let pane = PaneSnapshot {
        id: PaneId(303),
        title: String::new(),
        kind: PaneKindSnapshot::Picker,
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
    };

    let projected = MuxTreePane::from_snapshot(&pane);
    assert_eq!(projected.kind, MuxTreePaneKind::Picker);
    assert_eq!(projected.label, "new pane");
}

#[test]
fn every_session_projects_all_windows_and_panes() {
    let first_window = mux_window(11, 0, "shell", 101);
    let second_window = mux_window(22, 1, "web", 202);
    let snapshot = MuxSnapshot {
        generation: 8,
        focused_window: None,
        sessions: vec![
            SessionSnapshot {
                id: SessionId(1),
                name: "main".to_owned(),
                active_window: first_window.id,
                windows: vec![first_window],
                viewers: Vec::new(),
            },
            SessionSnapshot {
                id: SessionId(2),
                name: "research".to_owned(),
                active_window: second_window.id,
                windows: vec![second_window],
                viewers: Vec::new(),
            },
        ],
    };

    let model = local_model(&snapshot, Some(SessionId(2)));
    let sessions = &model.host(HostId::LOCAL).unwrap().sessions;
    assert_eq!(sessions[0].windows.len(), 1);
    assert!(!sessions[0].active);
    assert_eq!(sessions[0].windows[0].panes.len(), 1);
    assert!(!sessions[0].windows[0].active);
    assert_eq!(sessions[0].windows[0].index, 0);
    assert_eq!(sessions[1].windows.len(), 1);
    assert!(sessions[1].active);
    assert!(sessions[1].windows[0].active);
    assert_eq!(sessions[1].windows[0].index, 1);
    assert_eq!(
        model.session_for_target(HostId::LOCAL, TreeTarget::Window(WindowId(22))),
        Some(SessionId(2))
    );
    assert_eq!(
        model.session_for_target(HostId::LOCAL, TreeTarget::Pane(PaneId(202))),
        Some(SessionId(2))
    );
}

#[test]
fn tree_ids_are_stable_and_namespaced_by_host_and_node_type() {
    assert_eq!(TreeNode::Host(HostId::LOCAL).tree_id(), "host:HostId(0)");
    assert_eq!(
        TreeNode::Target(HostId::LOCAL, TreeTarget::Session(SessionId(9))).tree_id(),
        "host:HostId(0):session:$9"
    );
    assert_eq!(TreeTarget::Session(SessionId(9)).tree_id(), "session:$9");
    assert_eq!(TreeTarget::Window(WindowId(9)).tree_id(), "window:@9");
    assert_eq!(TreeTarget::Pane(PaneId(9)).tree_id(), "pane:%9");
}

#[test]
fn rebuild_restores_expansion_without_reopening_a_collapsed_window() {
    let snapshot = snapshot_with_two_panes();
    let model = local_model(&snapshot, Some(SessionId(1)));
    let host = TreeNode::Host(HostId::LOCAL);
    let session = TreeNode::Target(HostId::LOCAL, TreeTarget::Session(SessionId(1)));
    let window = TreeNode::Target(HostId::LOCAL, TreeTarget::Window(WindowId(11)));
    let pane = TreeNode::Target(HostId::LOCAL, TreeTarget::Pane(PaneId(202)));
    let expanded = BTreeSet::from([host, session, window]);

    let first = TreeProjection::new(&model, &expanded);
    assert_eq!(first.visible_indices[&pane], 3);

    let collapsed_window = BTreeSet::from([host, session]);
    let rebuilt = TreeProjection::new(&model, &collapsed_window);
    assert!(rebuilt.visible_indices.contains_key(&session));
    assert!(rebuilt.visible_indices.contains_key(&window));
    assert!(!rebuilt.visible_indices.contains_key(&pane));
}

#[test]
fn projection_keeps_parent_identity_and_hidden_expansion_metadata() {
    let snapshot = snapshot_with_two_panes();
    let model = local_model(&snapshot, Some(SessionId(1)));
    let host = TreeNode::Host(HostId::LOCAL);
    let session = TreeNode::Target(HostId::LOCAL, TreeTarget::Session(SessionId(1)));
    let window = TreeNode::Target(HostId::LOCAL, TreeTarget::Window(WindowId(11)));
    let pane = TreeNode::Target(HostId::LOCAL, TreeTarget::Pane(PaneId(202)));
    let collapsed = TreeProjection::new(&model, &BTreeSet::from([host]));

    assert_eq!(collapsed.entries.len(), 2);
    assert!(collapsed.expandable.contains(&host));
    assert!(collapsed.visible_indices.contains_key(&session));
    assert!(!collapsed.visible_indices.contains_key(&window));
    assert!(collapsed.expandable.contains(&session));
    assert!(collapsed.expandable.contains(&window));
    assert!(!collapsed.expandable.contains(&pane));

    let expanded = TreeProjection::new(&model, &BTreeSet::from([host, session, window]));
    assert_eq!(
        expanded.entries[expanded.visible_indices[&session]].parent,
        Some(host)
    );
    assert_eq!(
        expanded.entries[expanded.visible_indices[&window]].parent,
        Some(session)
    );
    assert_eq!(
        expanded.entries[expanded.visible_indices[&pane]].parent,
        Some(window)
    );
}

#[test]
fn disconnected_tree_disables_activation_without_hiding_targets() {
    let target = TreeTarget::Pane(PaneId(21));
    assert_eq!(
        activation_for_target(
            HostId::LOCAL,
            target,
            Some(SessionId(2)),
            Some(WindowId(13)),
            HostId::LOCAL,
            Some(SessionId(1)),
            false,
        ),
        None
    );
    assert_eq!(
        activation_for_target(
            HostId::LOCAL,
            target,
            Some(SessionId(1)),
            Some(WindowId(13)),
            HostId::LOCAL,
            Some(SessionId(1)),
            true,
        ),
        Some(SidebarActivation::Execute {
            host: HostId::LOCAL,
            commands: select_pane_commands(PaneId(21)),
        })
    );
}
