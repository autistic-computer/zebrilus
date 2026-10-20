use super::*;

#[test]
fn cold_session_reset_keeps_queued_prompt_text_for_its_single_turn() {
    for restoring in [false, true] {
        let mut lane = PaneLane::new(1, AgentProvider::Codex, None);
        lane.pending_prompts.push_back("review this".to_owned());
        assert_eq!(
            lane.project(&AgentStreamPayload::SessionReset { restoring }),
            b"\x1b[H\x1b[2J\x1b[3J"
        );
        let bytes = lane.project(&AgentStreamPayload::TurnStarted { turn_id: 1 });
        let text = String::from_utf8(bytes).expect("projected prompt");
        assert_eq!(text.matches("review this").count(), 1, "{text:?}");
        assert!(lane.pending_prompts.is_empty());
        let bytes = lane.project(&AgentStreamPayload::TurnStarted { turn_id: 2 });
        assert!(
            !String::from_utf8(bytes)
                .expect("next turn")
                .contains("review this")
        );
    }
}

#[test]
fn an_explicit_reclaim_removes_only_its_queued_projection() {
    let mut lane = PaneLane::new(1, AgentProvider::Codex, None);
    lane.pending_prompts
        .extend(["reclaimed".to_owned(), "retained".to_owned()]);
    lane.project(&AgentStreamPayload::PromptsReclaimed {
        prompts: vec![AgentPrompt {
            owner: ClientInstanceId::default(),
            text: "reclaimed".to_owned(),
            images: Vec::new(),
        }],
    });
    let text = String::from_utf8(lane.project(&AgentStreamPayload::TurnStarted { turn_id: 1 }))
        .expect("remaining projection");
    assert!(!text.contains("reclaimed"), "{text:?}");
    assert_eq!(text.matches("retained").count(), 1, "{text:?}");
    assert!(lane.pending_prompts.is_empty());
}

#[test]
fn runtime_restart_drops_old_projection_text_before_the_new_turn() {
    let mut lane = PaneLane::new(1, AgentProvider::Codex, None);
    lane.pending_prompts.push_back("old generation".to_owned());
    lane.restart(2, AgentProvider::Codex, None);
    lane.pending_prompts.push_back("new generation".to_owned());
    lane.project(&AgentStreamPayload::SessionReset { restoring: false });
    let text = String::from_utf8(lane.project(&AgentStreamPayload::TurnStarted { turn_id: 1 }))
        .expect("new projection");
    assert!(!text.contains("old generation"), "{text:?}");
    assert_eq!(text.matches("new generation").count(), 1, "{text:?}");
    assert!(lane.pending_prompts.is_empty());
}

#[test]
fn a_restored_session_projects_its_prompts_and_keeps_messages_apart() {
    let mut lane = PaneLane::new(1, AgentProvider::ClaudeCode, None);
    let update = |kind: &str, id: &str, text: &str| AgentStreamPayload::Update {
        update: serde_json::json!({
            "sessionUpdate": kind,
            "messageId": id,
            "content": { "type": "text", "text": text },
        }),
    };
    let mut bytes = Vec::new();
    for payload in [
        AgentStreamPayload::SessionReset { restoring: true },
        update("user_message_chunk", "u1", "first"),
        update("agent_message_chunk", "a1", "one"),
        update("agent_message_chunk", "a1", " more"),
        update("agent_message_chunk", "n1", "notice"),
        update("user_message_chunk", "u2", "second"),
        update("agent_message_chunk", "a2", "two"),
        AgentStreamPayload::SessionReady {
            session_id: "s".to_owned(),
            modes: None,
            config_options: None,
        },
        update("user_message_chunk", "u3", "live echo"),
    ] {
        bytes.extend(lane.project(&payload));
    }
    let text = String::from_utf8(bytes).expect("projection");
    let mut plain = String::new();
    let mut chars = text.chars().peekable();
    while let Some(next) = chars.next() {
        match (next, chars.peek()) {
            ('\x1b', Some(']')) => {
                for skipped in chars.by_ref() {
                    if skipped == '\x07' {
                        break;
                    }
                }
            }
            ('\x1b', Some('[')) => {
                for skipped in chars.by_ref() {
                    if skipped.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            _ => plain.push(next),
        }
    }
    assert_eq!(
        plain, "> first\r\none more\r\n\r\nnotice\r\n> second\r\ntwo\r\n",
        "{text:?}"
    );
    assert_eq!(text.matches("\x1b]133;D;0\x07").count(), 2, "{text:?}");
}

#[test]
fn tool_calls_project_one_line_when_they_start_and_when_they_fail() {
    let mut lane = PaneLane::new(1, AgentProvider::ClaudeCode, None);
    let mut bytes = Vec::new();
    for payload in [
        AgentStreamPayload::TurnStarted { turn_id: 1 },
        AgentStreamPayload::Update {
            update: serde_json::json!({
                "sessionUpdate": "agent_message_chunk",
                "messageId": "a1",
                "content": { "type": "text", "text": "running tests" },
            }),
        },
        AgentStreamPayload::Update {
            update: serde_json::json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "t1",
                "title": "cargo  test",
                "kind": "execute",
                "status": "in_progress",
            }),
        },
        AgentStreamPayload::Update {
            update: serde_json::json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": "t1",
                "status": "failed",
            }),
        },
        AgentStreamPayload::Update {
            update: serde_json::json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "t2",
                "title": format!("ls {}", "x".repeat(200)),
                "status": "pending",
            }),
        },
        AgentStreamPayload::Update {
            update: serde_json::json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": "t2",
                "status": "completed",
            }),
        },
        AgentStreamPayload::Update {
            update: serde_json::json!({
                "sessionUpdate": "agent_message_chunk",
                "messageId": "a2",
                "content": { "type": "text", "text": "done" },
            }),
        },
    ] {
        let mut projection = lane.project(&payload);
        if let Some(call) = lane.tool_call(&payload) {
            lane.project_tool_call(&payload, &call, &mut projection);
        }
        bytes.extend(projection);
    }
    let text = String::from_utf8(bytes).expect("projection");
    let reply = text
        .split("\x1b]133;C\x07")
        .nth(1)
        .expect("reply after the prompt");
    assert_eq!(
        reply,
        format!(
            "\r\nrunning tests\r\n[tool] cargo test\r\n[tool failed] cargo test\r\n[tool] ls {}\r\ndone",
            "x".repeat(117)
        )
    );
}
