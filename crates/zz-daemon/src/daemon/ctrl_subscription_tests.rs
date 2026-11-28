use super::ctrl_test_support::{
    bounded_reliable_children, compact_command, compact_registered, reliable_children,
};
use super::*;

#[test]
fn compact_subscription_rename_is_bounded_and_empty_diff_sends_nothing() {
    let shared = Arc::new(Shared::new(32));
    let mut context = ExecutionContext::default();
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "ctrl-tree"]),
        )
        .expect("model session");
    let session = context.session.expect("session");
    let (client, mailbox) = compact_registered(
        &shared,
        zz_protocol::Subscriptions {
            tree: zz_protocol::TreeSubscription::Attached,
            status: false,
            options: 0,
            keys: zz_protocol::KeySubscription::None,
            pane_stream: false,
        },
    );
    shared
        .attach_target(client, ClientKind::Interactive, &mut context, "ctrl-tree")
        .expect("attach");
    shared.compact_tree_messages(client, true);
    reliable_children(&mailbox);
    compact_command(
        &shared,
        &mut context,
        "rename-session",
        &["-t", "ctrl-tree", "renamed"],
    );
    shared.publish_compact_trees();
    let messages = bounded_reliable_children(&mailbox, 100);
    let deltas = messages
        .iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::TreeDelta(delta),
                ..
            }) => Some(delta),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(deltas.len(), 1, "{messages:?}");
    assert!(deltas[0].ops.iter().any(|op| matches!(op, zz_protocol::TreeOp::SessionName { session: changed, name } if *changed == session && name == "renamed")));
    let encoded = zz_protocol::encode_protocol_message(&Shared::event(EventPayload::TreeDelta(
        deltas[0].clone(),
    )))
    .expect("encode");
    assert!(encoded.len() <= 100, "rename frame {} bytes", encoded.len());
    shared.publish_compact_trees();
    assert!(reliable_children(&mailbox).is_empty());
    compact_command(&shared, &mut context, "kill-session", &["-t", "renamed"]);
}

#[test]
fn all_subscriber_unattached_session_rename_is_bounded_without_a_view_update() {
    let shared = Arc::new(Shared::new(40));
    let mut context = ExecutionContext::default();
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "ctrl-own"]),
        )
        .expect("model session");
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut context,
            &CommandInvocation::new("new-session", ["-d", "-s", "ctrl-other"]),
        )
        .expect("model session");
    let (client, mailbox) = compact_registered(
        &shared,
        zz_protocol::Subscriptions {
            tree: zz_protocol::TreeSubscription::All,
            status: false,
            options: 0,
            keys: zz_protocol::KeySubscription::None,
            pane_stream: false,
        },
    );
    shared
        .attach_target(client, ClientKind::Interactive, &mut context, "ctrl-own")
        .expect("attach own session");
    shared.send_compact_state(client, &mailbox, true);
    reliable_children(&mailbox);
    compact_command(
        &shared,
        &mut context,
        "rename-session",
        &["-t", "ctrl-other", "ctrl-else"],
    );
    shared.publish_snapshot_state();
    let messages = bounded_reliable_children(&mailbox, 100);
    assert!(
        matches!(messages.as_slice(), [ProtocolMessage::Event(Event { payload: EventPayload::TreeDelta(delta), .. })] if !delta.ops.is_empty())
    );
    shared.publish_snapshot_state();
    assert!(reliable_children(&mailbox).is_empty());
    compact_command(&shared, &mut context, "kill-session", &["-t", "ctrl-else"]);
    compact_command(&shared, &mut context, "kill-session", &["-t", "ctrl-own"]);
}

#[test]
fn new_full_subscriber_does_not_hide_pending_key_patch() {
    let shared = Arc::new(Shared::new(33));
    let (first, first_mailbox) = compact_registered(&shared, zz_protocol::Subscriptions::default());
    shared.send_compact_keys(first, &first_mailbox);
    shared.publish_key_tables_if_changed();
    reliable_children(&first_mailbox);
    shared
        .inner
        .lock()
        .engine
        .execute(
            &mut ExecutionContext::default(),
            &CommandInvocation::new(
                "bind-key",
                ["-T", "ctrl-test", "x", "display-message", "changed"],
            ),
        )
        .expect("bind");
    let (second, second_mailbox) =
        compact_registered(&shared, zz_protocol::Subscriptions::default());
    shared.send_compact_keys(second, &second_mailbox);
    shared.publish_key_tables_if_changed();
    let patched = reliable_children(&first_mailbox)
        .into_iter()
        .filter_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::KeyTablesPatched { tables, removed },
                ..
            }) => Some((
                tables
                    .into_iter()
                    .map(|table| table.name)
                    .collect::<Vec<_>>(),
                removed,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(patched, [(vec!["ctrl-test".to_owned()], Vec::new())]);
}

#[test]
fn first_full_subscriber_does_not_hide_a_pending_hash_update() {
    let shared = Arc::new(Shared::new(42));
    let (hash_client, hash_mailbox) = compact_registered(
        &shared,
        zz_protocol::Subscriptions {
            keys: zz_protocol::KeySubscription::Hash,
            ..zz_protocol::Subscriptions::control()
        },
    );
    shared.send_compact_keys(hash_client, &hash_mailbox);
    shared.publish_key_tables_if_changed();
    reliable_children(&hash_mailbox);
    let generation = {
        let mut inner = shared.inner.lock();
        inner
            .engine
            .execute(
                &mut ExecutionContext::default(),
                &CommandInvocation::new(
                    "bind-key",
                    ["-T", "ctrl-hash-pending", "x", "display-message", "changed"],
                ),
            )
            .expect("pending bind");
        inner.engine.keys.generation()
    };
    let (full_client, full_mailbox) =
        compact_registered(&shared, zz_protocol::Subscriptions::default());
    shared.send_compact_keys(full_client, &full_mailbox);
    reliable_children(&full_mailbox);
    shared.publish_key_tables_if_changed();
    let messages = bounded_reliable_children(&hash_mailbox, 80);
    assert!(
        matches!(messages.as_slice(), [ProtocolMessage::Event(Event { payload: EventPayload::KeyTablesHashChanged { hash, .. }, .. })] if *hash == generation)
    );
    assert!(reliable_children(&full_mailbox).is_empty());
}

#[test]
fn hash_subscriber_receives_bounded_mouse_hash_without_full_bindings() {
    let shared = Arc::new(Shared::new(37));
    let (_, mailbox) = compact_registered(
        &shared,
        zz_protocol::Subscriptions {
            keys: zz_protocol::KeySubscription::Hash,
            tree: zz_protocol::TreeSubscription::None,
            status: false,
            options: 0,
            pane_stream: false,
        },
    );
    compact_command(
        &shared,
        &mut ExecutionContext::default(),
        "bind-key",
        &["-T", "root", "x", "display-message", "changed"],
    );
    let messages = bounded_reliable_children(&mailbox, 80);
    let (generation, mouse) = {
        let inner = shared.inner.lock();
        (
            inner.engine.keys.generation(),
            zz_protocol::MouseBindings::from_key_tables(&inner.engine.keys),
        )
    };
    assert!(
        matches!(
            messages.as_slice(),
            [ProtocolMessage::Event(Event {
                payload: EventPayload::KeyTablesHashChanged { hash, mouse: sent },
                ..
            })] if *hash == generation && *sent == mouse
        ),
        "{messages:?}"
    );
}

#[test]
fn personalized_view_materializes_the_existing_stamped_tree() {
    let shared = Arc::new(Shared::new(38));
    let mut context = ExecutionContext::default();
    compact_command(
        &shared,
        &mut context,
        "new-session",
        &["-d", "-s", "ctrl-view-a", "-n", "first", "sleep 30"],
    );
    compact_command(
        &shared,
        &mut context,
        "new-session",
        &["-d", "-s", "ctrl-view-b", "-n", "second", "sleep 30"],
    );
    compact_command(
        &shared,
        &mut context,
        "set-option",
        &[
            "-gw",
            "window-status-format",
            "#{client_session}|#{pane_title}",
        ],
    );
    compact_command(
        &shared,
        &mut context,
        "set-option",
        &[
            "-gw",
            "window-status-current-format",
            "#{client_session}|#{pane_title}",
        ],
    );
    compact_command(
        &shared,
        &mut context,
        "set-option",
        &["-gw", "pane-border-status", "top"],
    );
    compact_command(
        &shared,
        &mut context,
        "set-option",
        &[
            "-gw",
            "pane-border-format",
            "#{client_session}|#{pane_title}",
        ],
    );
    let mut clients = Vec::new();
    for session in ["ctrl-view-a", "ctrl-view-b"] {
        let (client, mailbox) = compact_registered(&shared, zz_protocol::Subscriptions::default());
        shared
            .attach_target(
                client,
                ClientKind::Interactive,
                &mut ExecutionContext::default(),
                session,
            )
            .expect("attach");
        clients.push((client, mailbox));
    }
    shared.settle_pane_locations();
    for (client, _) in &clients {
        let messages = shared.compact_tree_messages(*client, true);
        let mut raw = messages
            .iter()
            .find_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::Snapshot(snapshot),
                    ..
                }) => Some(snapshot.clone()),
                _ => None,
            })
            .expect("tree");
        let view = messages
            .iter()
            .find_map(|message| match message {
                ProtocolMessage::Event(Event {
                    payload: EventPayload::ClientView(view),
                    ..
                }) => Some(view),
                _ => None,
            })
            .expect("view");
        view.apply(&mut raw).expect("overlay");
        let inner = shared.inner.lock();
        let mut expected = inner.engine.state.snapshot();
        stamp_snapshot_for_client(&inner, *client, &mut expected, &snapshot_presence(&inner));
        expected.generation = raw.generation;
        assert_eq!(raw, expected);
    }
    compact_command(
        &shared,
        &mut context,
        "kill-session",
        &["-t", "ctrl-view-a"],
    );
    compact_command(
        &shared,
        &mut context,
        "kill-session",
        &["-t", "ctrl-view-b"],
    );
}

#[cfg(feature = "agent")]
#[test]
fn an_unattached_tree_client_sees_another_sessions_agent_finish() {
    let shared = Arc::new(Shared::new(54));
    let mut context = ExecutionContext::default();
    let pane = {
        let mut inner = shared.inner.lock();
        inner
            .engine
            .execute(
                &mut context,
                &CommandInvocation::new("new-session", ["-d", "-s", "elsewhere"]),
            )
            .expect("agent setup");
        context.pane.expect("agent terminal")
    };
    let (client, mailbox) = compact_registered(
        &shared,
        zz_protocol::Subscriptions {
            tree: zz_protocol::TreeSubscription::All,
            status: false,
            options: 0,
            keys: zz_protocol::KeySubscription::None,
            pane_stream: false,
        },
    );
    shared.send_compact_state(client, &mailbox, true);
    reliable_children(&mailbox);

    let done = PaneStatus {
        state: PaneStatusState::Done,
        kind: None,
        progress: None,
        app: "codex".to_owned(),
        title: String::new(),
        message: "All tests pass".to_owned(),
    };
    let now = unix_timestamp();
    pane_summary::apply_pane_status(&mut shared.inner.lock(), pane, Some(done.clone()), now);
    shared.publish_snapshot_state();
    let ops = reliable_children(&mailbox)
        .into_iter()
        .flat_map(|message| match message {
            ProtocolMessage::Event(Event {
                payload: EventPayload::TreeDelta(delta),
                ..
            }) => delta.ops,
            _ => Vec::new(),
        })
        .collect::<Vec<_>>();
    let status = ops.iter().find_map(|op| match op {
        zz_protocol::TreeOp::PaneStatus {
            pane: changed,
            status,
            ..
        } if *changed == pane => Some(status.clone()),
        _ => None,
    });
    assert_eq!(status, Some(Some(done)), "{ops:?}");
    let stamp = ops.iter().find_map(|op| match op {
        zz_protocol::TreeOp::PaneSummary {
            pane: changed,
            summary,
            ..
        } if *changed == pane => Some((summary.unseen, summary.changed_at)),
        _ => None,
    });
    assert_eq!(stamp, Some((true, now)), "{ops:?}");
}
