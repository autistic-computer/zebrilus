use std::time::Instant;

use serde_json::json;
use zz_protocol::agent_stream::AgentStreamPayload;
use zz_protocol::chat::{
    Answer, ChatPhase, ChatView, ItemId, ItemKind, ItemStatus, RequestClose, RequestId,
};

use super::chat_bridge::{Shadow, chat_answer};
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
