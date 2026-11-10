use std::time::Instant;

use serde_json::json;
use zz_protocol::agent_stream::{AgentImage, AgentStreamPayload};
use zz_protocol::chat::{
    Answer, Ask, AskOption, ChatPhase, ChatView, ItemId, ItemKind, ItemStatus, OptionKind,
    RequestClose, RequestId,
};

use super::chat_bridge::{MAX_PROMPT_IMAGES_BASE64, Shadow, attach_images, chat_answer};
use super::turn::{self, End};

fn view(shadow: &Shadow) -> ChatView {
    let mut view = ChatView::default();
    for event in &shadow.events {
        view.apply(event)
            .unwrap_or_else(|skip| panic!("skipped {event:?}: {skip:?}"));
    }
    view
}

fn ready(shadow: &mut Shadow) {
    shadow.turn(&turn::Fact::SessionReady, Some("s1"), Instant::now());
}

fn running(shadow: &mut Shadow, command: &str) {
    let now = Instant::now();
    shadow.turn(
        &turn::Fact::Prompt {
            turn: 1,
            command: Some(command.to_owned()),
        },
        None,
        now,
    );
    shadow.turn(
        &turn::Fact::Started {
            command: command.to_owned(),
        },
        None,
        now,
    );
}

fn update(update: serde_json::Value) -> AgentStreamPayload {
    AgentStreamPayload::Update { update }
}

fn tool(id: &str, status: &str) -> AgentStreamPayload {
    update(json!({
        "sessionUpdate": "tool_call", "toolCallId": id, "title": "cargo test",
        "kind": "execute", "status": status, "rawInput": { "command": "cargo test" },
    }))
}

#[test]
fn an_answer_is_recorded_as_the_answer_not_a_withdrawal() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    running(&mut shadow, "c1");
    shadow.payload(&tool("t1", "in_progress"));
    shadow.payload(&AgentStreamPayload::PermissionRequested {
        request_id: 3,
        tool_call: json!({ "toolCallId": "t1", "title": "Run cargo test" }),
        options: json!([{ "optionId": "allow", "name": "Allow", "kind": "allow_once" }]),
        questions: Vec::new(),
    });
    shadow.answered(3, chat_answer(Some("allow")));
    shadow.payload(&AgentStreamPayload::PermissionResolved {
        request_id: 3,
        canceled: false,
    });
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    assert_eq!(
        view(&shadow)
            .request(RequestId(3))
            .and_then(|r| r.close.clone()),
        Some(RequestClose::Answered {
            answer: Answer::Option {
                id: "allow".to_owned()
            }
        })
    );
}

#[test]
fn a_plan_approval_carries_the_plan_text() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    running(&mut shadow, "c1");
    shadow.payload(&AgentStreamPayload::PermissionRequested {
        request_id: 4,
        tool_call: json!({
            "toolCallId": "plan",
            "title": "Ready to code?",
            "kind": "switch_mode",
            "content": [{ "type": "content", "content": { "type": "text", "text": "1. Morph the composer" } }],
        }),
        options: json!([
            { "optionId": "approve", "name": "Yes", "kind": "allow_once" },
            { "optionId": "keep_planning", "name": "No, keep planning", "kind": "reject_once" },
        ]),
        questions: Vec::new(),
    });
    assert_eq!(
        view(&shadow)
            .request(RequestId(4))
            .map(|request| request.ask.clone()),
        Some(Ask::Plan {
            markdown: "1. Morph the composer".to_owned(),
            options: vec![
                AskOption {
                    id: "approve".to_owned(),
                    label: "Yes".to_owned(),
                    kind: OptionKind::Allow,
                },
                AskOption {
                    id: "keep_planning".to_owned(),
                    label: "No, keep planning".to_owned(),
                    kind: OptionKind::Deny,
                },
            ],
        })
    );
}

#[test]
fn history_restored_before_the_session_is_ready_shows_up_closed() {
    let mut shadow = Shadow::recording();
    shadow.payload(&update(json!({
        "sessionUpdate": "user_message_chunk", "messageId": "u1",
        "content": { "type": "text", "text": "fix it" },
    })));
    shadow.payload(&tool("t1", "in_progress"));
    shadow.payload(&update(json!({
        "sessionUpdate": "agent_message_chunk", "messageId": "a1",
        "content": { "type": "text", "text": "done" },
    })));
    assert!(shadow.events.is_empty());
    ready(&mut shadow);
    let view = view(&shadow);
    assert_eq!(view.items.len(), 3);
    assert!(view.open_items().next().is_none());
    assert_eq!(
        view.item(&ItemId::new("t1"))
            .map(|item| item.status.clone()),
        Some(ItemStatus::Interrupted)
    );
    assert_eq!(view.phase(), ChatPhase::Idle);
}

#[test]
fn a_codex_command_still_running_when_its_turn_completes_goes_to_the_background() {
    let mut shadow = Shadow::recording().codex();
    ready(&mut shadow);
    running(&mut shadow, "t1");
    shadow.payload(&tool("cmd", "in_progress"));
    shadow.turn(
        &turn::Fact::Ended {
            command: "t1".to_owned(),
            end: End::Completed,
        },
        None,
        Instant::now(),
    );
    assert!(view(&shadow).running().is_none());
    assert!(
        view(&shadow)
            .item(&ItemId::new("cmd"))
            .is_some_and(|item| item.status.is_open() && item.background)
    );
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "cmd", "status": "completed",
    })));
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    assert_eq!(
        view(&shadow)
            .item(&ItemId::new("cmd"))
            .map(|item| item.status.clone()),
        Some(ItemStatus::Completed)
    );
}

#[test]
fn a_claude_tool_marked_background_leaves_the_turn() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    running(&mut shadow, "c1");
    shadow.payload(&tool("b1", "in_progress"));
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "b1",
        "_meta": { "zz": { "background": true } },
    })));
    shadow.turn(
        &turn::Fact::Ended {
            command: "c1".to_owned(),
            end: End::Completed,
        },
        None,
        Instant::now(),
    );
    assert!(
        view(&shadow)
            .item(&ItemId::new("b1"))
            .is_some_and(|item| item.status.is_open() && item.background)
    );
}

fn end_turn(shadow: &mut Shadow, command: &str) {
    shadow.turn(
        &turn::Fact::Ended {
            command: command.to_owned(),
            end: End::Completed,
        },
        None,
        Instant::now(),
    );
}

fn prompt(shadow: &mut Shadow, owner: u64, command: &str) {
    let now = Instant::now();
    shadow.turn(
        &turn::Fact::Prompt {
            turn: owner,
            command: Some(command.to_owned()),
        },
        None,
        now,
    );
    shadow.turn(
        &turn::Fact::Started {
            command: command.to_owned(),
        },
        None,
        now,
    );
}

#[test]
fn a_stale_codex_completion_leaves_the_running_turn_alone() {
    let mut shadow = Shadow::recording().codex();
    ready(&mut shadow);
    prompt(&mut shadow, 1, "t1");
    end_turn(&mut shadow, "t1");
    prompt(&mut shadow, 2, "t2");
    shadow.payload(&tool("cmd", "in_progress"));
    end_turn(&mut shadow, "t1");
    let view = view(&shadow);
    let item = view.item(&ItemId::new("cmd")).expect("cmd");
    assert!(item.status.is_open() && !item.background);
}

#[test]
fn a_codex_subagent_still_running_at_turn_end_keeps_its_steps() {
    let mut shadow = Shadow::recording().codex();
    ready(&mut shadow);
    running(&mut shadow, "t1");
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "spawn", "title": "Explore", "kind": "think",
        "status": "in_progress", "_meta": { "codex": { "itemType": "collabAgentToolCall" } },
    })));
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "ls", "title": "ls", "kind": "execute",
        "status": "in_progress", "_meta": { "zz": { "parent": "spawn" } },
    })));
    end_turn(&mut shadow, "t1");
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "ls", "status": "completed",
    })));
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "spawn", "status": "completed",
    })));
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
}

#[test]
fn a_background_mark_reaches_a_child_still_waiting_for_its_parent() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    running(&mut shadow, "c1");
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "child", "title": "sleep", "kind": "execute",
        "status": "in_progress", "_meta": { "zz": { "parent": "agent" } },
    })));
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "child",
        "_meta": { "zz": { "background": true } },
    })));
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "agent", "title": "Agent", "kind": "think",
        "status": "in_progress", "_meta": { "claudeCode": { "toolName": "Agent" } },
    })));
    end_turn(&mut shadow, "c1");
    let view = view(&shadow);
    let child = view.item(&ItemId::new("child")).expect("child");
    assert!(child.status.is_open() && child.background);
}

#[test]
fn replayed_history_keeps_a_streamed_message_whole() {
    let mut shadow = Shadow::recording();
    for text in ["hel", "lo"] {
        shadow.payload(&update(json!({
            "sessionUpdate": "agent_message_chunk", "messageId": "m",
            "content": { "type": "text", "text": text },
        })));
    }
    ready(&mut shadow);
    let view = view(&shadow);
    assert_eq!(view.items.len(), 1);
    assert_eq!(
        view.item(&ItemId::new("a:m"))
            .map(|item| item.body.text.clone()),
        Some("hello".to_owned())
    );
}

#[test]
fn switching_sessions_starts_a_fresh_transcript_that_can_reuse_ids() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    running(&mut shadow, "c1");
    shadow.payload(&tool("x", "completed"));
    end_turn(&mut shadow, "c1");
    shadow.turn(&turn::Fact::SessionClosed, None, Instant::now());
    shadow.payload(&tool("x", "completed"));
    shadow.turn(&turn::Fact::SessionReady, Some("s2"), Instant::now());
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    let view = view(&shadow);
    assert!(view.turns.is_empty());
    assert_eq!(view.items.len(), 1);
}

#[test]
fn an_approval_that_arrives_before_the_session_is_ready_shows_up_after() {
    let mut shadow = Shadow::recording();
    shadow.payload(&tool("t1", "in_progress"));
    shadow.payload(&AgentStreamPayload::PermissionRequested {
        request_id: 5,
        tool_call: json!({ "toolCallId": "t1", "title": "Run cargo test" }),
        options: json!([]),
        questions: Vec::new(),
    });
    ready(&mut shadow);
    let view = view(&shadow);
    assert!(
        view.request(RequestId(5))
            .is_some_and(zz_protocol::chat::RequestView::is_open)
    );
    assert!(
        view.item(&ItemId::new("t1"))
            .is_some_and(|item| item.status.is_open())
    );
    assert_eq!(view.phase(), ChatPhase::Waiting);
}

#[test]
fn a_session_reset_drops_what_the_old_session_sent_while_offline() {
    let mut shadow = Shadow::recording();
    shadow.payload(&tool("old", "in_progress"));
    shadow.payload(&AgentStreamPayload::SessionReset { restoring: true });
    shadow.payload(&tool("new", "completed"));
    shadow.turn(&turn::Fact::SessionReady, Some("s2"), Instant::now());
    let view = view(&shadow);
    assert!(view.item(&ItemId::new("old")).is_none());
    assert!(view.item(&ItemId::new("new")).is_some());
}

#[test]
fn a_reset_while_loading_keeps_the_approvals_that_already_arrived() {
    let mut shadow = Shadow::recording();
    shadow.payload(&AgentStreamPayload::PermissionRequested {
        request_id: 12,
        tool_call: json!({ "toolCallId": "t9", "title": "Run ls" }),
        options: json!([]),
        questions: Vec::new(),
    });
    shadow.payload(&AgentStreamPayload::SessionReset { restoring: true });
    ready(&mut shadow);
    assert!(
        view(&shadow)
            .request(RequestId(12))
            .is_some_and(zz_protocol::chat::RequestView::is_open)
    );
}

#[test]
fn subagent_text_nests_under_its_subagent() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    running(&mut shadow, "c1");
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "toolu_parent", "title": "Explore",
        "kind": "think", "status": "in_progress",
    })));
    shadow.payload(&update(json!({
        "sessionUpdate": "agent_message_chunk", "messageId": "m1",
        "content": { "type": "text", "text": "inner" },
        "_meta": { "zz": { "parent": "toolu_parent" } },
    })));
    let chat = view(&shadow);
    let parent = chat.item(&ItemId::new("toolu_parent")).unwrap();
    assert_eq!(parent.children.len(), 1);
    let inner = chat.item(&parent.children[0]).unwrap();
    assert_eq!(inner.body.text, "inner");
    assert!(!inner.status.is_open());
    assert!(
        chat.turns
            .iter()
            .all(|turn| !turn.items.contains(&parent.children[0]))
    );
}

#[test]
fn a_claude_subagent_carries_its_prompt_as_the_target() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    running(&mut shadow, "c1");
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "agent", "title": "Survey", "kind": "think",
        "status": "in_progress",
        "rawInput": { "description": "Survey", "prompt": "Read the reducer" },
        "_meta": { "claudeCode": { "toolName": "Agent" } },
    })));
    let view = view(&shadow);
    let agent = view.item(&ItemId::new("agent")).expect("agent");
    assert_eq!(agent.kind, ItemKind::Subagent);
    assert_eq!(agent.body.target.as_deref(), Some("Read the reducer"));
}

#[test]
fn a_question_answered_in_the_terminal_becomes_an_answered_record() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    running(&mut shadow, "c1");
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "q1", "title": "Which file?",
        "kind": "other", "status": "in_progress",
    })));
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "q1", "status": "completed",
        "_meta": { "zz": { "answered": [
            { "question": "Which file?", "header": "Open first", "answer": "chat.rs" },
        ] } },
    })));
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    let view = view(&shadow);
    let request = view
        .requests
        .iter()
        .find(|request| request.item == Some(ItemId::new("q1")))
        .expect("a record for the question");
    let zz_protocol::chat::Ask::Questions { questions } = &request.ask else {
        panic!("not a question: {:?}", request.ask);
    };
    assert_eq!(questions[0].header.as_deref(), Some("Open first"));
    assert_eq!(
        request.close,
        Some(RequestClose::Answered {
            answer: Answer::Questions {
                answers: vec![zz_protocol::chat::QuestionAnswer {
                    question: questions[0].id.clone(),
                    answers: vec!["chat.rs".to_owned()],
                }],
            },
        })
    );
    assert_eq!(
        view.item(&ItemId::new("q1"))
            .map(|item| item.status.clone()),
        Some(ItemStatus::Completed)
    );
}

#[test]
fn a_question_already_asked_through_zz_is_not_recorded_twice() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    running(&mut shadow, "c1");
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "q1", "title": "Which file?",
        "kind": "other", "status": "in_progress",
    })));
    shadow.payload(&AgentStreamPayload::PermissionRequested {
        request_id: 4,
        tool_call: json!({ "toolCallId": "q1", "title": "Which file?" }),
        options: json!([]),
        questions: vec![zz_protocol::agent_stream::AgentQuestion {
            id: "q0".to_owned(),
            question: "Which file?".to_owned(),
            ..Default::default()
        }],
    });
    shadow.answered(
        4,
        Answer::Questions {
            answers: vec![zz_protocol::chat::QuestionAnswer {
                question: "q0".to_owned(),
                answers: vec!["chat.rs".to_owned()],
            }],
        },
    );
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "q1", "status": "completed",
        "_meta": { "zz": { "answered": [ { "question": "Which file?", "answer": "chat.rs" } ] } },
    })));
    assert_eq!(view(&shadow).requests.len(), 1);
}

#[test]
fn a_question_card_answered_in_the_terminal_still_leaves_its_answer() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    running(&mut shadow, "c1");
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "q1", "title": "Which file?",
        "kind": "other", "status": "in_progress",
    })));
    shadow.payload(&AgentStreamPayload::PermissionRequested {
        request_id: 5,
        tool_call: json!({ "toolCallId": "q1", "title": "Which file?" }),
        options: json!([]),
        questions: vec![zz_protocol::agent_stream::AgentQuestion {
            id: "q0".to_owned(),
            question: "Which file?".to_owned(),
            ..Default::default()
        }],
    });
    shadow.payload(&AgentStreamPayload::PermissionResolved {
        request_id: 5,
        canceled: false,
    });
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "q1", "status": "completed",
        "_meta": { "zz": { "answered": [ { "question": "Which file?", "answer": "chat.rs" } ] } },
    })));
    let view = view(&shadow);
    assert_eq!(view.requests.len(), 2);
    assert!(
        view.requests
            .iter()
            .any(|request| matches!(request.close, Some(RequestClose::Answered { .. })))
    );
}

#[test]
fn a_stamped_update_keeps_its_transcript_time() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    running(&mut shadow, "c1");
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "t1", "title": "cargo test",
        "kind": "execute", "status": "in_progress", "rawInput": { "command": "cargo test" },
        "_meta": { "zz": { "at": 1_790_000_000_000_u64 } },
    })));
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed",
        "_meta": { "zz": { "at": 1_790_000_004_000_u64 } },
    })));
    let view = view(&shadow);
    let item = view.item(&ItemId::new("t1")).expect("tool item");
    assert_eq!(item.started_at, 1_790_000_000_000);
    assert_eq!(item.ended_at, Some(1_790_000_004_000));
}

#[test]
fn prompt_images_reach_the_user_item() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    shadow.payload(&update(json!({
        "sessionUpdate": "user_message_chunk",
        "messageId": "p1",
        "content": { "type": "text", "text": "" },
        "_meta": { "zz": { "images": [{ "format": "image/png", "data": "aGk=" }] } },
    })));
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    let view = view(&shadow);
    let item = view.item(&ItemId::new("u:p1")).expect("user item");
    assert_eq!(item.kind, ItemKind::User);
    assert_eq!(
        item.body.images,
        [AgentImage {
            format: "image/png".to_owned(),
            data: b"hi".to_vec(),
        }]
    );
}

#[test]
fn a_chat_mode_prompt_shows_its_pasted_images_and_sends_them_once() {
    use agent_client_protocol::v1::schema::MessageId;
    use zz_protocol::ClientInstanceId;

    use super::chat_bridge::without_images;
    use super::runtime::{prompt_blocks, prompt_echo};
    use super::stream::AgentPrompt;

    let image = AgentImage {
        format: "image/png".to_owned(),
        data: b"hi".to_vec(),
    };
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    let mut sent = Vec::new();
    for (id, text) in [("p1", "what is this?"), ("p2", "")] {
        let blocks = prompt_blocks(AgentPrompt {
            owner: ClientInstanceId::default(),
            text: text.to_owned(),
            images: vec![image.clone()],
        });
        for echo in prompt_echo(&blocks, &MessageId::new(id)).expect("echo") {
            let payload = update(echo);
            shadow.payload(&payload);
            sent.push(without_images(payload));
        }
        shadow.turn(&turn::Fact::Idle, None, Instant::now());
    }
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    let view = view(&shadow);
    for (id, text) in [("u:p1", "what is this?"), ("u:p2", "")] {
        let item = view.item(&ItemId::new(id)).expect("user item");
        assert_eq!(item.body.text, text);
        assert_eq!(item.body.images, std::slice::from_ref(&image), "{id}");
    }
    let chunks = sent
        .iter()
        .map(|payload| {
            let AgentStreamPayload::Update { update } = payload else {
                unreachable!()
            };
            assert!(update["_meta"]["zz"]["images"].is_null(), "{update}");
            update["content"]["type"].as_str().unwrap_or_default()
        })
        .collect::<Vec<_>>();
    assert_eq!(chunks, ["text", "image", "text", "image"]);
}

#[test]
fn a_long_prompt_leaves_its_images_only_the_room_left_under_the_stream_cap() {
    let image = "A".repeat(MAX_PROMPT_IMAGES_BASE64 / 2);
    let mut short = json!({ "content": { "type": "text", "text": "look" } });
    attach_images(
        &mut short,
        [
            ("image/png".to_owned(), image.clone()),
            ("image/png".to_owned(), image.clone()),
        ],
    );
    assert_eq!(
        short["_meta"]["zz"]["images"].as_array().map(Vec::len),
        Some(1)
    );
    let text = "b".repeat(MAX_PROMPT_IMAGES_BASE64 / 2 + 1);
    let mut long = json!({ "content": { "type": "text", "text": text } });
    attach_images(&mut long, [("image/png".to_owned(), image)]);
    assert!(long["_meta"].is_null());
}

#[test]
fn a_tool_with_a_duration_starts_that_long_before_its_record() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    running(&mut shadow, "c1");
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "t1", "title": "sleep 20",
        "kind": "execute", "status": "in_progress", "rawInput": { "command": "sleep 20" },
        "_meta": { "zz": { "at": 1_790_000_020_000_u64, "ms": 20_000 } },
    })));
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed",
        "_meta": { "zz": { "at": 1_790_000_020_000_u64 } },
    })));
    let view = view(&shadow);
    let item = view.item(&ItemId::new("t1")).expect("tool item");
    assert_eq!(item.started_at, 1_790_000_000_000);
    assert_eq!(item.ended_at, Some(1_790_000_020_000));
}

#[test]
fn only_a_spawned_codex_agent_is_a_subagent() {
    let mut shadow = Shadow::recording().codex();
    ready(&mut shadow);
    running(&mut shadow, "t1");
    for (id, tool) in [("w", "wait"), ("s", "spawnAgent")] {
        shadow.payload(&update(json!({
            "sessionUpdate": "tool_call", "toolCallId": id, "title": id, "kind": "other",
            "status": "in_progress", "rawInput": { "tool": tool },
            "_meta": { "codex": { "itemType": "collabAgentToolCall" } },
        })));
    }
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "s~2", "title": "s", "kind": "think",
        "status": "in_progress", "rawInput": { "agentThreadId": "child" },
        "_meta": { "codex": { "itemType": "collabAgentToolCall" } },
    })));
    let view = view(&shadow);
    let kind = |id: &str| view.item(&ItemId::new(id)).map(|item| item.kind);
    assert_eq!(kind("w"), Some(ItemKind::Tool));
    assert_eq!(kind("s"), Some(ItemKind::Subagent));
    assert_eq!(kind("s~2"), Some(ItemKind::Subagent));
}

#[test]
fn an_async_codex_question_outlives_its_turn_and_takes_its_answer() {
    let mut shadow = Shadow::recording().codex();
    ready(&mut shadow);
    shadow.turn(&turn::Fact::Busy, None, Instant::now());
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "call_q", "title": "Which part?",
        "kind": "other", "status": "in_progress",
        "_meta": { "codex": { "itemType": "agentMessage", "delivery": "async" }, "zz": { "background": true } },
    })));
    shadow.payload(&AgentStreamPayload::PermissionRequested {
        request_id: 8,
        tool_call: json!({ "toolCallId": "call_q", "title": "Which part?", "kind": "other",
            "_meta": { "zz": { "background": true } } }),
        options: json!([]),
        questions: vec![zz_protocol::agent_stream::AgentQuestion {
            id: "q0".to_owned(),
            question: "Which part?".to_owned(),
            ..Default::default()
        }],
    });
    shadow.turn(&turn::Fact::Idle, None, Instant::now());
    let idle = view(&shadow);
    assert!(idle.running().is_none());
    assert!(
        idle.request(RequestId(8))
            .is_some_and(zz_protocol::chat::RequestView::is_open)
    );
    assert_eq!(idle.phase(), ChatPhase::Idle);
    assert_eq!(idle.asking().map(|request| request.id), Some(RequestId(8)));
    shadow.answered(
        8,
        Answer::Questions {
            answers: vec![zz_protocol::chat::QuestionAnswer {
                question: "q0".to_owned(),
                answers: vec!["B".to_owned()],
            }],
        },
    );
    shadow.payload(&AgentStreamPayload::PermissionResolved {
        request_id: 8,
        canceled: false,
    });
    shadow.turn(&turn::Fact::Busy, None, Instant::now());
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "call_q", "status": "completed",
        "_meta": { "zz": { "answered": [ { "question": "Which part?", "answer": "B" } ] } },
    })));
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    let view = view(&shadow);
    assert_eq!(view.requests.len(), 1);
    assert!(matches!(
        view.request(RequestId(8))
            .and_then(|request| request.close.clone()),
        Some(RequestClose::Answered { .. })
    ));
    assert_eq!(
        view.item(&ItemId::new("call_q"))
            .map(|item| item.status.clone()),
        Some(ItemStatus::Completed)
    );
}

#[test]
fn a_batch_summary_after_the_turn_still_names_its_tools_and_the_live_line_follows_the_turn() {
    use zz_protocol::chat::{ChatEvent, TurnId};

    let info = |zz: serde_json::Value| {
        update(json!({ "sessionUpdate": "session_info_update", "_meta": { "zz": zz } }))
    };
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    running(&mut shadow, "c1");
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "t1", "title": "wc -l a.txt",
        "kind": "execute", "status": "in_progress", "rawInput": { "command": "wc -l a.txt" },
        "_meta": { "zz": { "activity": "Count lines in a.txt" } },
    })));
    shadow.payload(&info(json!({ "activity": "Counting lines" })));
    shadow.payload(&update(json!({
        "sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed",
        "content": [{ "type": "content", "content": { "type": "text", "text": "2 a.txt" } }],
    })));
    shadow.payload(&tool("t2", "completed"));
    end_turn(&mut shadow, "c1");
    shadow.payload(&info(json!({
        "summary": "Counted lines in a.txt", "toolCallIds": ["t1", "t2", "ghost"],
    })));
    shadow.payload(&info(json!({ "activity": null })));
    let picked = shadow
        .events
        .iter()
        .filter(|event| {
            matches!(
                event,
                ChatEvent::TurnActivity { .. } | ChatEvent::ItemsSummarized { .. }
            )
        })
        .map(|event| match event {
            ChatEvent::TurnActivity { turn, text, .. } => ChatEvent::TurnActivity {
                turn: *turn,
                text: text.clone(),
                at: 0,
            },
            other => other.clone(),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        picked,
        [
            ChatEvent::TurnActivity {
                turn: TurnId(1),
                text: Some("Counting lines".to_owned()),
                at: 0,
            },
            ChatEvent::ItemsSummarized {
                items: vec![ItemId::new("t1"), ItemId::new("t2")],
                summary: "Counted lines in a.txt".to_owned(),
            },
        ]
    );
    let view = view(&shadow);
    let first = view.item(&ItemId::new("t1")).expect("t1");
    assert_eq!(first.body.activity.as_deref(), Some("Count lines in a.txt"));
    assert_eq!(first.body.text, "2 a.txt");
    assert_eq!(
        first.summary,
        view.item(&ItemId::new("t2")).expect("t2").summary
    );
    assert_eq!(
        first.summary.as_ref().map(|summary| summary.text.as_str()),
        Some("Counted lines in a.txt")
    );
}

#[test]
fn text_streamed_outside_a_turn_stays_one_item() {
    let mut shadow = Shadow::recording();
    ready(&mut shadow);
    for text in ["| 9 | Summ", "ary and pie |\n\n```", "mermaid\npie\n```"] {
        shadow.payload(&update(json!({
            "sessionUpdate": "agent_message_chunk", "messageId": "m:0",
            "content": { "type": "text", "text": text },
        })));
    }
    shadow.payload(&tool("t1", "in_progress"));
    assert!(shadow.rejects.is_empty(), "{:?}", shadow.rejects);
    let view = view(&shadow);
    let item = view.item(&ItemId::new("a:m:0")).expect("one text item");
    assert_eq!(
        item.body.text,
        "| 9 | Summary and pie |\n\n```mermaid\npie\n```"
    );
    assert_eq!(item.status, ItemStatus::Completed);
    assert!(view.item(&ItemId::new("a:m:0~1")).is_none());
}
