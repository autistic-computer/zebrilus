use agent_client_protocol::v1::schema::{
    ContentBlock, ContentChunk, MessageId, PermissionOptionKind, TextContent, ToolCall,
    ToolCallStatus, ToolCallUpdateFields,
};

use ala::TestAppContext;

use super::test_support::*;
use super::*;

fn unknown_update(tag: &str) -> AgentStreamPayload {
    AgentStreamPayload::Update {
        update: serde_json::json!({"sessionUpdate": tag, "payload": {"opaque": true}}),
    }
}

fn chunk_update(text: &str, message_id: &str) -> AgentStreamPayload {
    AgentStreamPayload::Update {
        update: json(&SessionUpdate::AgentMessageChunk(
            ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                .message_id(MessageId::new(message_id)),
        )),
    }
}

#[ala::test]
fn chat_events_fold_into_the_pane_chat_and_a_restart_clears_it(cx: &mut TestAppContext) {
    use zz_protocol::chat::{ChatEvent, TurnBy, TurnId};
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(4);
    let chat = |event| AgentStreamPayload::Chat { event };
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.apply_stream_items(
                pane,
                vec![
                    item(
                        1,
                        chat(ChatEvent::SessionReady {
                            session: "s1".to_owned(),
                        }),
                    ),
                    item(
                        2,
                        chat(ChatEvent::TurnStarted {
                            turn: TurnId(1),
                            by: TurnBy::Agent,
                            at: 5,
                        }),
                    ),
                ],
                cx,
            );
            let view = controller.pane_chat(pane).expect("pane chat");
            assert_eq!(view.turns.len(), 1);
            assert!(view.running().is_some());
            controller.apply_stream_items(pane, vec![item(3, chat(ChatEvent::Restart))], cx);
            assert!(
                controller
                    .pane_chat(pane)
                    .expect("pane chat")
                    .turns
                    .is_empty()
            );
        });
    });
}

#[ala::test]
fn stream_items_reduce_exactly_as_the_runtime_events_did(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(3);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller
                .viewports
                .get_mut(&pane)
                .expect("viewport")
                .queued_prompts = 1;
            controller.apply_stream_items(
                pane,
                vec![
                    item(
                        1,
                        AgentStreamPayload::Ready {
                            agent_name: "Codex".to_owned(),
                            agent_key: "codex".to_owned(),
                            auth_methods: vec![zz_daemon::AgentAuthMethod {
                                id: "api".to_owned(),
                                name: "API key".to_owned(),
                                description: None,
                            }],
                            capabilities: zz_daemon::AgentSessionCapabilities {
                                list: true,
                                load: true,
                                ..zz_daemon::AgentSessionCapabilities::default()
                            },
                        },
                    ),
                    item(
                        2,
                        AgentStreamPayload::SessionReady {
                            session_id: "s-1".to_owned(),
                            modes: None,
                            config_options: None,
                        },
                    ),
                    item(
                        3,
                        AgentStreamPayload::Update {
                            update: json(&SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new("hello")),
                            ))),
                        },
                    ),
                    item(
                        4,
                        AgentStreamPayload::Update {
                            update: json(&SessionUpdate::ToolCall(
                                ToolCall::new("tool-1", "Read file")
                                    .status(ToolCallStatus::InProgress),
                            )),
                        },
                    ),
                    item(
                        5,
                        AgentStreamPayload::PermissionRequested {
                            request_id: 7,
                            tool_call: json(&ToolCallUpdate::new(
                                "tool-1",
                                ToolCallUpdateFields::new(),
                            )),
                            options: json(&vec![PermissionOption::new(
                                "allow-once",
                                "Allow once",
                                PermissionOptionKind::AllowOnce,
                            )]),
                            questions: Vec::new(),
                        },
                    ),
                    item(
                        6,
                        AgentStreamPayload::PermissionResolved {
                            request_id: 7,
                            canceled: false,
                        },
                    ),
                    item(7, turn_finished(StopReason::EndTurn)),
                    item(
                        8,
                        AgentStreamPayload::PromptsReclaimed {
                            prompts: vec![zz_daemon::AgentPrompt {
                                owner: zz_protocol::ClientInstanceId::default(),
                                text: "retry that".to_owned(),
                                images: Vec::new(),
                            }],
                        },
                    ),
                ],
                cx,
            );

            let state = controller.pane_state(pane).expect("the pane is registered");
            assert_eq!(state.agent_name.as_deref(), Some("Codex"));
            assert_eq!(state.session_id.as_deref(), Some("s-1"));
            assert!(state.session_capabilities.list);
            assert_eq!(state.connection, AgentConnectionState::Ready);
            assert!(state.pending_permissions.is_empty());
            assert_eq!(state.queued_prompts, 0);
            let (entries, ..) = controller.pane_entries(pane).expect("entries");
            assert!(matches!(
                entries.first(),
                Some(AgentThreadEntry::Assistant { markdown, .. }) if markdown == "hello"
            ));
            assert!(matches!(
                entries.get(1),
                Some(AgentThreadEntry::Tool {
                    status: AgentToolStatusModel::Pending,
                    ..
                })
            ));
            assert_eq!(
                controller.take_pending_composer(pane).as_deref(),
                Some("retry that"),
                "a reclaimed prompt is visible in the draft again"
            );
            assert_eq!(
                controller.viewports[&pane].last_applied, 8,
                "the cursor follows the highest applied seq"
            );
        });
    });
}

#[ala::test]
fn an_item_the_client_cannot_re_type_is_dropped_without_stalling_the_cursor(
    cx: &mut TestAppContext,
) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(4);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.apply_stream_items(
                pane,
                vec![
                    item(
                        1,
                        AgentStreamPayload::Update {
                            update: serde_json::json!({"sessionUpdate": "from the future"}),
                        },
                    ),
                    item(
                        2,
                        AgentStreamPayload::Update {
                            update: json(&SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new("still here")),
                            ))),
                        },
                    ),
                ],
                cx,
            );

            let (entries, ..) = controller.pane_entries(pane).expect("entries");
            assert_eq!(entries.len(), 1);
            assert_eq!(controller.viewports[&pane].last_applied, 2);
            assert_eq!(controller.panes[&pane].unknown_updates, 1);
        });
    });
}

#[ala::test]
fn an_unknown_update_between_two_known_ones_only_skips_itself(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(40);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.apply_stream_items(
                pane,
                vec![
                    item(1, chunk_update("first", "message-1")),
                    item(2, unknown_update("terminal_output_chunk")),
                    item(3, chunk_update("second", "message-2")),
                ],
                cx,
            );

            let (entries, ..) = controller.pane_entries(pane).expect("entries");
            assert_eq!(entries.len(), 2, "an unknown item poisons nothing after it");
            assert!(matches!(
                (&entries[0], &entries[1]),
                (
                    AgentThreadEntry::Assistant { markdown: first, .. },
                    AgentThreadEntry::Assistant { markdown: second, .. },
                ) if first == "first" && second == "second"
            ));
            assert_eq!(controller.viewports[&pane].last_applied, 3);
            assert_eq!(controller.panes[&pane].unknown_updates, 1);
        });
    });
}

#[ala::test]
fn a_replay_over_a_surviving_transcript_reduces_each_item_once(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(44);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.apply_stream_items(
                pane,
                vec![
                    item(1, chunk_update("first", "message-1")),
                    item(2, chunk_update("second", "message-2")),
                ],
                cx,
            );

            controller.apply_stream_items(
                pane,
                vec![
                    item(1, chunk_update("first", "message-1")),
                    item(2, chunk_update("second", "message-2")),
                    item(3, chunk_update("third", "message-3")),
                ],
                cx,
            );

            let (entries, ..) = controller.pane_entries(pane).expect("entries");
            assert_eq!(
                entries.len(),
                3,
                "an attach replays over the transcript it already reduced"
            );
            assert_eq!(controller.viewports[&pane].last_applied, 3);
        });
    });
}

#[ala::test]
fn submitted_and_queued_user_messages_survive_session_switch_replay(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(44);
    let user = |text: &str, id: &str| {
        let mut chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text)));
        chunk.message_id = Some(MessageId::new(id));
        AgentStreamPayload::Update {
            update: json(&SessionUpdate::UserMessageChunk(chunk)),
        }
    };
    let items = vec![
        item(1, user("first", "zz-prompt-1")),
        item(2, chunk_update("answer one", "answer-1")),
        item(3, turn_finished(StopReason::EndTurn)),
        item(4, user("second", "zz-prompt-2")),
        item(5, chunk_update("answer two", "answer-2")),
        item(6, turn_finished(StopReason::EndTurn)),
    ];
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            for provider in [AgentProvider::Codex, AgentProvider::ClaudeCode] {
                ready_pane(controller, pane);
                controller.panes.get_mut(&pane).expect("pane").provider = provider;
                controller.prompt(pane, "first", Vec::new(), cx).expect("first prompt");
                controller.prompt(pane, "second", Vec::new(), cx).expect("queued prompt");
                controller.apply_stream_items(pane, items.clone(), cx);
                let expected = controller.pane_entries(pane).expect("entries").0.to_vec();
                assert_eq!(expected.len(), 4);
                assert!(matches!(&expected[0], AgentThreadEntry::User { markdown, .. } if markdown == "first"));
                assert!(matches!(&expected[2], AgentThreadEntry::User { markdown, .. } if markdown == "second"));
                controller.retain_panes(&BTreeSet::new(), &BTreeSet::new(), cx);
                ready_pane(controller, pane);
                controller.panes.get_mut(&pane).expect("pane").provider = provider;
                controller.apply_stream_items(pane, items.clone(), cx);
                controller.apply_stream_items(pane, items.clone(), cx);
                assert_eq!(controller.pane_entries(pane).expect("replayed entries").0, expected);
                controller.retain_panes(&BTreeSet::new(), &BTreeSet::new(), cx);
            }
        });
    });
}

#[ala::test]
fn an_unknown_update_mid_message_does_not_break_its_coalescing(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(41);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.apply_stream_items(
                pane,
                vec![
                    item(1, chunk_update("hello ", "message-1")),
                    item(2, unknown_update("_zz_private_extension")),
                    item(3, chunk_update("world", "message-1")),
                ],
                cx,
            );

            let (entries, ..) = controller.pane_entries(pane).expect("entries");
            assert_eq!(entries.len(), 1, "the message stays one entry");
            assert!(matches!(
                &entries[0],
                AgentThreadEntry::Assistant { markdown, .. } if markdown == "hello world"
            ));
            assert_eq!(controller.panes[&pane].unknown_updates, 1);
        });
    });
}

#[ala::test]
fn a_batch_of_only_unknown_updates_advances_the_cursor(cx: &mut TestAppContext) {
    let (controller, sink) = proxy_controller(cx);
    let pane = PaneId(42);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.apply_stream_items(
                pane,
                vec![
                    item(5, unknown_update("state_update")),
                    item(6, unknown_update("_vendor_thing")),
                    item(
                        7,
                        AgentStreamPayload::Update {
                            update: serde_json::json!({
                                "sessionUpdate": "agent_message_chunk",
                                "content": 42,
                            }),
                        },
                    ),
                ],
                cx,
            );

            assert!(
                controller
                    .pane_entries(pane)
                    .is_some_and(|(entries, ..)| entries.is_empty())
            );
            assert_eq!(
                controller.viewports[&pane].last_applied, 7,
                "every seq counts as consumed, so nothing asks to be replayed"
            );
            assert_eq!(controller.panes[&pane].unknown_updates, 3);
        });
    });
    assert!(
        sink.borrow().is_empty(),
        "a batch zz skipped whole is not a hole in the journal"
    );
}

#[ala::test]
fn a_restored_session_replays_around_updates_this_build_cannot_read(cx: &mut TestAppContext) {
    let (controller, _sink) = proxy_controller(cx);
    let pane = PaneId(43);
    cx.update(|cx| {
        controller.update(cx, |controller, cx| {
            ready_pane(controller, pane);
            controller.apply_stream_items(
                pane,
                vec![item(
                    1,
                    AgentStreamPayload::SessionSwitched {
                        session_id: "s-2".to_owned(),
                        cwd: PathBuf::from("/workspace"),
                        modes: None,
                        config_options: None,
                        replay: vec![
                            json(&SessionUpdate::AgentMessageChunk(
                                ContentChunk::new(ContentBlock::Text(TextContent::new("restored")))
                                    .message_id(MessageId::new("message-1")),
                            )),
                            serde_json::json!({"sessionUpdate": "plan_update", "planId": "p1"}),
                            json(&SessionUpdate::AgentMessageChunk(
                                ContentChunk::new(ContentBlock::Text(TextContent::new(
                                    " and whole",
                                )))
                                .message_id(MessageId::new("message-1")),
                            )),
                        ],
                    },
                )],
                cx,
            );

            let state = controller.pane_state(pane).expect("pane state");
            assert_eq!(state.session_id.as_deref(), Some("s-2"));
            assert_eq!(state.connection, AgentConnectionState::Ready);
            let (entries, ..) = controller.pane_entries(pane).expect("entries");
            assert_eq!(entries.len(), 1);
            assert!(matches!(
                &entries[0],
                AgentThreadEntry::Assistant { markdown, .. } if markdown == "restored and whole"
            ));
            assert_eq!(controller.panes[&pane].unknown_updates, 1);
        });
    });
}

#[test]
fn a_skipped_update_is_logged_by_tag_and_never_by_payload() {
    let payload = "x".repeat(4 * 1024);
    let update = serde_json::json!({
        "sessionUpdate": format!("_evil\n{payload}"),
        "content": payload,
    });

    let tag = session_update_tag(&update);
    assert!(tag.len() <= MAX_SESSION_UPDATE_TAG_BYTES);
    assert!(tag.starts_with("_evil"));
    assert!(!tag.contains('\n'), "a tag cannot forge a second log line");
    assert!(!tag.contains(&payload));

    let error = serde_json::from_value::<SessionUpdate>(update)
        .expect_err("an unknown discriminant does not decode");
    let excerpt = log_excerpt(&error.to_string(), MAX_DECODE_ERROR_BYTES);
    assert!(excerpt.len() <= MAX_DECODE_ERROR_BYTES);

    assert_eq!(
        session_update_tag(&serde_json::json!({"content": "no tag"})),
        UNTAGGED_SESSION_UPDATE
    );
}
