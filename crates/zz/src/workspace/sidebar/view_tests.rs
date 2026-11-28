use std::cell::RefCell;

use ala::TestAppContext;
use zz_daemon_client::DaemonError;
use zz_protocol::{PaneStatus, PaneStatusState, WindowId};

use super::test_support::*;
use super::*;

#[ala::test]
fn tree_model_tracks_snapshot_focus_and_attachment_revisions(cx: &mut TestAppContext) {
    let mux = cx.new(|cx| {
        MuxClient::new(
            Err(DaemonError::Thread("test client".to_owned())),
            zz_daemon_client::default_socket_path(),
            cx,
        )
    });
    let sidebar = cx.new(|cx| WorkspaceSidebar::new(mux.clone(), cx));
    sidebar.update(cx, |sidebar, cx| {
        assert!(sidebar.synchronize_tree(cx));
        assert_eq!(sidebar.tree_model.hosts.len(), 1);
        assert!(!sidebar.synchronize_tree(cx));
    });

    let mut snapshot = snapshot_with_two_panes();
    snapshot.sessions[0]
        .windows
        .push(mux_window(12, 3, "logs", 303));
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(1), snapshot.clone(), cx);
    });
    sidebar.update(cx, |sidebar, cx| {
        assert!(!sidebar.synchronize_tree(cx));
        assert_eq!(
            sidebar.active_target,
            Some(TreeNode::Target(
                HostId::LOCAL,
                TreeTarget::Pane(PaneId(202))
            ))
        );
    });

    snapshot.focused_window = Some(WindowId(12));
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(1), snapshot.clone(), cx);
    });
    sidebar.update(cx, |sidebar, cx| {
        assert!(!sidebar.synchronize_tree(cx));
        assert_eq!(
            sidebar.active_target,
            Some(TreeNode::Target(
                HostId::LOCAL,
                TreeTarget::Pane(PaneId(303))
            ))
        );
    });

    snapshot.sessions[0].name = "renamed".to_owned();
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(1), snapshot.clone(), cx);
    });
    sidebar.update(cx, |sidebar, cx| {
        assert!(!sidebar.synchronize_tree(cx));
        assert_eq!(sidebar.tree_model.hosts[0].sessions[0].name, "renamed");
    });

    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(999), snapshot, cx);
    });
    sidebar.update(cx, |sidebar, cx| {
        assert!(!sidebar.synchronize_tree(cx));
        assert_eq!(sidebar.active_target, None);
    });
}

#[ala::test]
fn attention_follows_the_mux_without_a_render(cx: &mut TestAppContext) {
    let mux = cx.new(|cx| {
        MuxClient::new(
            Err(DaemonError::Thread("test client".to_owned())),
            zz_daemon_client::default_socket_path(),
            cx,
        )
    });
    let sidebar = cx.new(|cx| WorkspaceSidebar::new(mux.clone(), cx));
    let mut snapshot = snapshot_with_two_panes();
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(1), snapshot.clone(), cx);
    });

    snapshot.sessions[0].windows[0]
        .panes
        .get_mut(&PaneId(101))
        .expect("terminal pane")
        .status = Some(PaneStatus {
        state: PaneStatusState::Blocked,
        kind: None,
        progress: None,
        app: String::new(),
        title: String::new(),
        message: String::new(),
    });
    mux.update(cx, |mux, cx| {
        mux.attach_snapshot_for_test(SessionId(1), snapshot, cx);
    });
    sidebar.read_with(cx, |sidebar, _| {
        assert_eq!(
            sidebar.badges.get(&(HostId::LOCAL, PaneId(101))),
            Some(&AgentBadge::NeedsInput)
        );
    });
}

#[ala::test]
fn settings_view_is_lazy_and_retained(cx: &mut TestAppContext) {
    cx.update(zz_ui::init);
    let captured = Rc::new(RefCell::new(None));
    let captured_for_window = Rc::clone(&captured);
    let (_, cx) = cx.add_window_view(move |_, cx| {
        let mux = cx.new(|cx| {
            MuxClient::new(
                Err(DaemonError::Thread("test client".to_owned())),
                zz_daemon_client::default_socket_path(),
                cx,
            )
        });
        captured_for_window.replace(Some(cx.entity()));
        WorkspaceSidebar::new(mux, cx)
    });
    let sidebar = captured.borrow().clone().expect("captured sidebar");

    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.route()),
        WorkspaceRoute::App
    );
    assert!(
        sidebar
            .read_with(cx, |sidebar, _| sidebar.settings_view())
            .is_none()
    );

    let first = cx.update(|window, cx| {
        sidebar.update(cx, |sidebar, cx| {
            sidebar.open_settings(window, cx);
            sidebar.settings_view().expect("settings created on entry")
        })
    });
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.route()),
        WorkspaceRoute::Settings
    );

    cx.update(|window, cx| {
        first.update(cx, |settings, cx| {
            settings.set_section(zz_ui::settings::SettingsSection::Terminal, window, cx);
        });
    });
    cx.draw(
        ala::Point::default(),
        ala::size(
            ala::AvailableSpace::Definite(px(800.0)),
            ala::AvailableSpace::Definite(px(600.0)),
        ),
        |_, _| first.clone().into_any_element(),
    );
    assert!(first.read_with(cx, |settings, _| settings.has_terminal_preview()));

    cx.update(|window, cx| {
        sidebar.update(cx, |sidebar, cx| sidebar.close_settings(window, cx));
    });
    assert!(!first.read_with(cx, |settings, _| settings.has_terminal_preview()));
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.route()),
        WorkspaceRoute::App
    );

    let reopened = cx.update(|window, cx| {
        sidebar.update(cx, |sidebar, cx| {
            sidebar.open_settings(window, cx);
            sidebar
                .settings_view()
                .expect("settings retained on re-entry")
        })
    });
    assert_eq!(first.entity_id(), reopened.entity_id());
    cx.draw(
        ala::Point::default(),
        ala::size(
            ala::AvailableSpace::Definite(px(800.0)),
            ala::AvailableSpace::Definite(px(600.0)),
        ),
        |_, _| reopened.clone().into_any_element(),
    );
    assert!(reopened.read_with(cx, |settings, _| settings.has_terminal_preview()));
}

#[test]
fn plain_keys_bind_only_inside_the_focused_workspace_tree() {
    let keymap = ala::Keymap::new(workspace_tree_key_bindings(&crate::keymap::test_chords(
        SIDEBAR_TABLE,
    )));
    let tree_context =
        ala::KeyContext::parse(TREE_KEY_CONTEXT).expect("valid workspace tree context");
    let root_context = ala::KeyContext::parse("Root").expect("valid root context");
    for (key, action) in [
        ("r", std::any::TypeId::of::<TreeRename>()),
        (":", std::any::TypeId::of::<TreeCommandPalette>()),
    ] {
        let key = ala::Keystroke::parse(key).expect("valid tree key");

        let (tree_bindings, pending) = keymap.bindings_for_input(
            std::slice::from_ref(&key),
            std::slice::from_ref(&tree_context),
        );
        assert_eq!(tree_bindings.len(), 1);
        assert_eq!(tree_bindings[0].action().as_any().type_id(), action);
        assert!(!pending);

        let (root_bindings, pending) = keymap.bindings_for_input(
            std::slice::from_ref(&key),
            std::slice::from_ref(&root_context),
        );
        assert!(root_bindings.is_empty());
        assert!(!pending);
    }
}

#[test]
fn sidebar_resize_clamps_to_a_readable_column() {
    assert_eq!(sidebar_resize_width(0.0, 1_000.0), SIDEBAR_MIN_WIDTH);
    assert_eq!(sidebar_resize_width(300.0, 1_000.0), 300.0);
    assert_eq!(sidebar_resize_width(800.0, 1_000.0), 500.0);
    assert_eq!(sidebar_resize_width(800.0, 2_000.0), SIDEBAR_MAX_WIDTH);
}
