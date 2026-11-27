use ala::TestAppContext;

use super::test_support::*;
use super::*;

#[test]
fn pending_composer_text_stacks_and_is_taken_once() {
    let mut controller = AgentController::new();
    let pane = PaneId(7);
    assert!(controller.take_pending_composer(pane).is_none());

    assert!(controller.queue_composer_text(pane, "first"));
    assert!(controller.queue_composer_text(pane, "second"));
    assert!(!controller.queue_composer_text(pane, "   "));
    assert!(controller.pane_state(pane).is_none());

    controller.panes.insert(pane, thread());
    assert_eq!(
        controller
            .pane_state(pane)
            .expect("state")
            .pending_composer
            .as_deref(),
        Some("first\nsecond")
    );
    assert_eq!(
        controller.take_pending_composer(pane).as_deref(),
        Some("first\nsecond")
    );
    assert!(controller.take_pending_composer(pane).is_none());
    assert!(
        controller
            .pane_state(pane)
            .expect("state")
            .pending_composer
            .is_none()
    );
}

#[test]
fn a_prompt_is_refused_unless_the_pane_is_ready() {
    let mut thread = thread();
    assert!(thread.prompt_refusal(false).is_some());
    thread.connection = AgentConnectionState::Ready;
    assert!(thread.prompt_refusal(false).is_none());
    thread.connection = AgentConnectionState::Running;
    assert!(thread.prompt_refusal(false).is_some());
}

#[ala::test]
fn restored_prompts_only_refill_their_owners_composer(cx: &mut TestAppContext) {
    let (controller, sink) = proxy_controller(cx);
    let pane = PaneId(4);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.apply_stream_items(
                pane,
                vec![
                    item(
                        1,
                        AgentStreamPayload::PromptsRestored {
                            reclaim_id: 1,
                            prompts: vec![zz_daemon::AgentPrompt {
                                owner: zz_protocol::ClientInstanceId(2),
                                text: "someone else's draft".to_owned(),
                                images: Vec::new(),
                            }],
                        },
                    ),
                    item(
                        2,
                        AgentStreamPayload::PromptsRestored {
                            reclaim_id: 2,
                            prompts: vec![zz_daemon::AgentPrompt {
                                owner: zz_protocol::ClientInstanceId(1),
                                text: "my draft".to_owned(),
                                images: Vec::new(),
                            }],
                        },
                    ),
                ],
                cx,
            );

            assert_eq!(
                controller.take_pending_composer(pane).as_deref(),
                Some("my draft")
            );
        });
    });
    assert_eq!(
        &*sink.borrow(),
        &[(
            pane,
            AgentRequest::AcknowledgePromptRestore { reclaim_id: 2 }
        )]
    );
}

#[ala::test]
fn prompts_handed_back_by_a_flip_wait_in_the_draft_until_the_pane_closes(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(5);
    let restored = |reclaim_id, text: &str| {
        item(
            reclaim_id,
            AgentStreamPayload::PromptsRestored {
                reclaim_id,
                prompts: vec![zz_daemon::AgentPrompt {
                    owner: zz_protocol::ClientInstanceId(1),
                    text: text.to_owned(),
                    images: Vec::new(),
                }],
            },
        )
    };
    let flipped = BTreeSet::from([pane]);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.apply_stream_items(pane, vec![restored(1, "queued second")], cx);
            controller.keep_draft(pane, "typed first", Vec::new());
            controller.retain_panes(&BTreeSet::new(), &flipped, cx);
            ready_pane(controller, pane);
            assert_eq!(
                controller.take_pending_composer(pane).as_deref(),
                Some("typed first\nqueued second")
            );

            controller.apply_stream_items(pane, vec![restored(2, "queued again")], cx);
            controller.retain_panes(&BTreeSet::new(), &flipped, cx);
            controller.retain_panes(&BTreeSet::new(), &BTreeSet::new(), cx);
            ready_pane(controller, pane);
            assert_eq!(controller.take_pending_composer(pane), None);
        });
    });
}
