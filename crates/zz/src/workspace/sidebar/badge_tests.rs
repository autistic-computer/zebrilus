use std::collections::BTreeMap;

use zz_protocol::WindowId;

use super::test_support::*;
use super::*;

#[test]
fn agent_badges_bubble_to_every_collapsed_ancestor() {
    let snapshot = snapshot_with_two_panes();
    let model = local_model(&snapshot, Some(SessionId(1)));
    let badges = BTreeMap::from([
        ((HostId::LOCAL, PaneId(101)), AgentBadge::Finished),
        ((HostId::LOCAL, PaneId(202)), AgentBadge::NeedsInput),
    ]);
    let badge = |node| node_agent_badge(&model, &badges, node);

    assert_eq!(
        badge(TreeNode::Target(
            HostId::LOCAL,
            TreeTarget::Pane(PaneId(101))
        )),
        Some(AgentBadge::Finished)
    );
    for node in [
        TreeNode::Host(HostId::LOCAL),
        TreeNode::Target(HostId::LOCAL, TreeTarget::Session(SessionId(1))),
        TreeNode::Target(HostId::LOCAL, TreeTarget::Window(WindowId(11))),
    ] {
        assert_eq!(badge(node), Some(AgentBadge::NeedsInput));
    }
    assert_eq!(
        node_agent_badge(&model, &BTreeMap::new(), TreeNode::Host(HostId::LOCAL)),
        None
    );
}

#[test]
fn agent_badges_do_not_cross_host_id_boundaries() {
    let remote = host_ids(&["studio"])[0];
    let connected = HostState::Connected;
    let snapshot = snapshot_with_two_panes();
    let model = MuxTreeModel::from_hosts(
        HostId::LOCAL,
        Some(SessionId(1)),
        [
            (HostId::LOCAL, "local", &connected, Some(&snapshot)),
            (remote, "studio", &connected, Some(&snapshot)),
        ],
    );
    let badges = BTreeMap::from([((HostId::LOCAL, PaneId(202)), AgentBadge::NeedsInput)]);

    assert_eq!(
        node_agent_badge(
            &model,
            &badges,
            TreeNode::Target(HostId::LOCAL, TreeTarget::Pane(PaneId(202)))
        ),
        Some(AgentBadge::NeedsInput)
    );
    assert_eq!(
        node_agent_badge(
            &model,
            &badges,
            TreeNode::Target(remote, TreeTarget::Pane(PaneId(202)))
        ),
        None
    );
    assert_eq!(
        node_agent_badge(&model, &badges, TreeNode::Host(remote)),
        None
    );
}

#[test]
fn pending_bells_bubble_through_collapsed_remote_ancestors() {
    let remote = host_ids(&["studio"])[0];
    let connected = HostState::Connected;
    let local_snapshot = snapshot_with_two_panes();
    let mut remote_snapshot = remote_snapshot();
    remote_snapshot.sessions[1].windows[0]
        .panes
        .get_mut(&PaneId(303))
        .unwrap()
        .bell = true;
    let model = MuxTreeModel::from_hosts(
        HostId::LOCAL,
        Some(SessionId(1)),
        [
            (HostId::LOCAL, "local", &connected, Some(&local_snapshot)),
            (remote, "studio", &connected, Some(&remote_snapshot)),
        ],
    );

    for node in [
        TreeNode::Host(remote),
        TreeNode::Target(remote, TreeTarget::Session(SessionId(9))),
        TreeNode::Target(remote, TreeTarget::Window(WindowId(21))),
        TreeNode::Target(remote, TreeTarget::Pane(PaneId(303))),
    ] {
        assert!(model.has_pending_bell(node), "missing bell on {node:?}");
    }
    assert!(!model.has_pending_bell(TreeNode::Host(HostId::LOCAL)));
}
