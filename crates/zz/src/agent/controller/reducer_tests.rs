use agent_client_protocol::v1::schema::{
    AvailableCommand, AvailableCommandInput, AvailableCommandsUpdate, ContentBlock, ContentChunk,
    Diff, MessageId, PermissionOptionKind, Plan, PlanEntry, PlanEntryPriority, PlanEntryStatus,
    Terminal, TextContent, ToolCall, ToolCallContent, ToolCallLocation, ToolCallStatus,
    ToolCallUpdateFields, ToolKind,
};
use zz_protocol::{MAX_AGENT_PERMISSION_OPTIONS, MAX_AGENT_TOOL_CONTENT_ITEMS};

use super::test_support::*;
use super::*;

#[test]
fn the_model_menu_takes_picks_while_a_turn_runs() {
    use AgentConnectionState::{
        Cancelling, Disconnected, Failed, Ready, Restoring, Running, Starting,
    };
    assert!(Ready.accepts_setting() && Running.accepts_setting());
    assert!(!Running.accepts_prompt());
    for state in [Starting, Restoring, Cancelling, Failed, Disconnected] {
        assert!(!state.accepts_setting(), "{state:?}");
    }
}

#[test]
fn reducer_coalesces_message_chunks_by_protocol_id() {
    let mut thread = thread();
    thread.apply_update(SessionUpdate::AgentMessageChunk(
        ContentChunk::new(ContentBlock::Text(TextContent::new("hello ")))
            .message_id(MessageId::new("message-1")),
    ));
    let entry_id = thread.transcript.entries()[0].id();
    let first_revision = thread.transcript.entry_revisions()[0];
    thread.apply_update(SessionUpdate::AgentMessageChunk(
        ContentChunk::new(ContentBlock::Text(TextContent::new("world")))
            .message_id(MessageId::new("message-1")),
    ));

    assert_eq!(thread.transcript.entries().len(), 1);
    assert_eq!(
        thread.transcript.entry_revisions().len(),
        thread.transcript.entries().len()
    );
    assert_eq!(thread.transcript.entries()[0].id(), entry_id);
    assert!(thread.transcript.entry_revisions()[0] > first_revision);
    assert!(matches!(
        &thread.transcript.entries()[0],
        AgentThreadEntry::Assistant { markdown, .. } if markdown == "hello world"
    ));
}

#[test]
fn reducer_tracks_available_commands() {
    let mut thread = thread();
    thread.apply_update(SessionUpdate::AvailableCommandsUpdate(
        AvailableCommandsUpdate::new(vec![
            AvailableCommand::new("review", "Review changes").input(
                AvailableCommandInput::Unstructured(
                    agent_client_protocol::v1::schema::UnstructuredCommandInput::new(
                        "optional instructions",
                    ),
                ),
            ),
            AvailableCommand::new("brainstorm", "Explore an idea"),
        ]),
    ));
    assert!(matches!(
        thread.available_commands.as_ref(),
        [AgentCommand {
            name,
            input_hint: Some(hint),
            ..
        }, AgentCommand {
            name: second_name,
            ..
        }] if name == "review"
            && hint == "optional instructions"
            && second_name == "brainstorm"
    ));
}

#[test]
fn adapter_controlled_ui_collections_are_bounded() {
    let mut thread = thread();
    let commands = (0..MAX_AGENT_AVAILABLE_COMMANDS + 8)
        .map(|index| AvailableCommand::new(format!("command-{index}"), "run it"))
        .collect();
    thread.apply_update(SessionUpdate::AvailableCommandsUpdate(
        AvailableCommandsUpdate::new(commands),
    ));
    assert_eq!(
        thread.available_commands.len(),
        MAX_AGENT_AVAILABLE_COMMANDS
    );

    let content = (0..MAX_AGENT_TOOL_CONTENT_ITEMS + 8)
        .map(|index| ContentBlock::Text(TextContent::new(index.to_string())).into())
        .collect::<Vec<_>>();
    thread.apply_update(SessionUpdate::ToolCall(
        ToolCall::new("bounded", "Bounded").content(content),
    ));
    assert!(
        matches!(thread.transcript.entries().last(), Some(AgentThreadEntry::Tool { output, .. }) if output.len() == MAX_AGENT_TOOL_CONTENT_ITEMS)
    );

    let permission_options = (0..MAX_AGENT_PERMISSION_OPTIONS + 8)
        .map(|index| {
            PermissionOption::new(
                format!("allow-{index}"),
                format!("Allow {index}"),
                PermissionOptionKind::AllowOnce,
            )
        })
        .collect();
    thread.transcript.request_permission(
        42,
        ToolCallUpdate::new(
            "tool-cap",
            ToolCallUpdateFields::new().title("Bound choices"),
        ),
        permission_options,
    );
    assert_eq!(
        thread.transcript.permissions()[0].options.len(),
        MAX_AGENT_PERMISSION_OPTIONS
    );
}

#[test]
fn tool_updates_keep_a_stable_native_entry_id() {
    let mut thread = thread();
    thread.apply_update(SessionUpdate::ToolCall(
        ToolCall::new("tool-1", "Run tests")
            .kind(ToolKind::Execute)
            .status(ToolCallStatus::InProgress),
    ));
    let id = thread.transcript.entries()[0].id();
    thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "tool-1",
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Completed)
            .raw_output(serde_json::json!({"ok": true})),
    )));

    assert_eq!(thread.transcript.entries().len(), 1);
    assert_eq!(thread.transcript.entries()[0].id(), id);
    assert!(matches!(
        &thread.transcript.entries()[0],
        AgentThreadEntry::Tool {
            status: AgentToolStatusModel::Completed,
            output,
            ..
        } if matches!(
            output.as_slice(),
            [ToolPayload::Json(output)] if output.contains("ok")
        )
    ));
}

#[test]
fn vendor_metadata_is_inert_and_parented_updates_stay_flat() {
    let mut thread = thread();
    let metadata = serde_json::json!({
        "claudeCode": {
            "subagent": true,
            "parentToolUseId": "parent-tool"
        },
        "codex": {
            "subagent": {
                "threadId": "thread-1",
                "activity": "started"
            },
            "collaboration": {
                "tool": "spawnAgent"
            }
        },
        "contextCompaction": true,
        "terminal_info": {
            "terminal_id": "terminal-1"
        }
    })
    .as_object()
    .unwrap()
    .clone();
    thread.apply_update(SessionUpdate::ToolCall(
        ToolCall::new("tool-1", "Task")
            .status(ToolCallStatus::InProgress)
            .raw_input(serde_json::json!({"prompt": "sleep"}))
            .meta(metadata.clone()),
    ));
    thread.apply_update(SessionUpdate::ToolCallUpdate(
        ToolCallUpdate::new("tool-1", ToolCallUpdateFields::new()).meta(metadata.clone()),
    ));
    thread.apply_update(SessionUpdate::AgentMessageChunk(
        ContentChunk::new(ContentBlock::Text(TextContent::new("child output"))).meta(metadata),
    ));

    assert!(matches!(
        thread.transcript.entries(),
        [
            AgentThreadEntry::Tool {
                label,
                status: AgentToolStatusModel::Running,
                input: Some(ToolPayload::Json(input)),
                output,
                ..
            },
            AgentThreadEntry::Assistant { markdown, .. }
        ] if label == "Task"
            && input.contains("sleep")
            && output.is_empty()
            && markdown == "child output"
    ));
}

#[test]
fn prompt_completion_ends_activity_without_fabricating_tool_completion() {
    let mut thread = thread();
    thread.connection = AgentConnectionState::Running;
    thread.apply_update(SessionUpdate::ToolCall(
        ToolCall::new("tool-running", "Run tests").status(ToolCallStatus::InProgress),
    ));
    thread.apply_update(SessionUpdate::ToolCall(ToolCall::new(
        "tool-pending",
        "Read file",
    )));

    thread.transcript.finish_turn();
    thread.connection = AgentConnectionState::Ready;

    let statuses = thread
        .transcript
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            AgentThreadEntry::Tool { status, .. } => Some(*status),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        statuses,
        [AgentToolStatusModel::Running, AgentToolStatusModel::Pending,]
    );
    assert_eq!(thread.connection, AgentConnectionState::Ready);
}

#[test]
fn late_standard_updates_change_data_without_reviving_pane_activity() {
    let mut thread = thread();
    thread.connection = AgentConnectionState::Ready;

    thread.apply_runtime_update(SessionUpdate::ToolCall(
        ToolCall::new("tool-late", "Read replayed file").status(ToolCallStatus::InProgress),
    ));
    thread.apply_runtime_update(SessionUpdate::AgentMessageChunk(ContentChunk::new(
        ContentBlock::Text(TextContent::new("late output")),
    )));

    assert_eq!(thread.connection, AgentConnectionState::Ready);
    assert!(matches!(
        thread.transcript.entries(),
        [
            AgentThreadEntry::Tool {
                status: AgentToolStatusModel::Running,
                ..
            },
            AgentThreadEntry::Assistant { markdown, .. }
        ] if markdown == "late output"
    ));
}

#[test]
fn tool_payloads_preserve_structure_and_prefer_content_over_raw_output() {
    let mut thread = thread();
    thread.apply_update(SessionUpdate::ToolCall(ToolCall::new(
        "tool-1",
        "Edit file",
    )));
    thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "tool-1",
        ToolCallUpdateFields::new()
            .locations(vec![
                ToolCallLocation::new("/workspace/src/lib.rs").line(12),
            ])
            .raw_input(serde_json::json!({"path": "/workspace/src/lib.rs"}))
            .content(vec![
                Diff::new("/workspace/src/lib.rs", "new text")
                    .old_text("old text")
                    .into(),
                ContentBlock::Text(TextContent::new("finished")).into(),
                ToolCallContent::Terminal(Terminal::new("terminal-1")),
            ])
            .raw_output(serde_json::json!({"ignored": true})),
    )));

    assert!(matches!(
        &thread.transcript.entries()[0],
        AgentThreadEntry::Tool {
            location: Some(location),
            input: Some(ToolPayload::Json(input)),
            output,
            ..
        } if location == "/workspace/src/lib.rs:12"
            && input.contains("path")
            && matches!(
                output.as_slice(),
                [
                    ToolPayload::Diff { path, old: Some(old), new },
                    ToolPayload::Text(text),
                    ToolPayload::Terminal(terminal),
                ] if path == "/workspace/src/lib.rs"
                    && old == "old text"
                    && new == "new text"
                    && text == "finished"
                    && terminal == "[terminal terminal-1]"
            )
    ));

    thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "tool-1",
        ToolCallUpdateFields::new().raw_output(serde_json::json!({"late": true})),
    )));
    assert!(matches!(
        &thread.transcript.entries()[0],
        AgentThreadEntry::Tool { output, .. }
            if matches!(output.first(), Some(ToolPayload::Diff { .. }))
    ));

    thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "tool-1",
        ToolCallUpdateFields::new()
            .content(Vec::new())
            .raw_output(serde_json::json!({"fallback": true})),
    )));
    assert!(matches!(
        &thread.transcript.entries()[0],
        AgentThreadEntry::Tool { output, .. }
            if matches!(
                output.as_slice(),
                [ToolPayload::Json(output)] if output.contains("fallback")
            )
    ));
}

#[test]
fn plan_updates_replace_the_existing_plan_entry() {
    let mut thread = thread();
    let plan = |status| {
        Plan::new(vec![PlanEntry::new(
            "Implement ACP",
            PlanEntryPriority::High,
            status,
        )])
    };
    thread.apply_update(SessionUpdate::Plan(plan(PlanEntryStatus::InProgress)));
    let id = thread.transcript.entries()[0].id();
    thread.apply_update(SessionUpdate::Plan(plan(PlanEntryStatus::Completed)));

    assert_eq!(thread.transcript.entries().len(), 1);
    assert_eq!(thread.transcript.entries()[0].id(), id);
    assert!(matches!(
        &thread.transcript.entries()[0],
        AgentThreadEntry::Plan { markdown, .. } if markdown.starts_with("- [x]")
    ));
}

#[test]
fn permission_lifecycle_marks_and_releases_the_tool() {
    let mut thread = thread();
    thread.apply_update(SessionUpdate::ToolCall(ToolCall::new(
        "tool-1",
        "Edit file",
    )));
    thread.transcript.request_permission(
        7,
        ToolCallUpdate::new(
            "tool-1",
            ToolCallUpdateFields::new().title("Edit file".to_owned()),
        ),
        vec![PermissionOption::new(
            "allow-once",
            "Allow once",
            PermissionOptionKind::AllowOnce,
        )],
    );
    assert!(matches!(
        &thread.transcript.entries()[0],
        AgentThreadEntry::Tool {
            status: AgentToolStatusModel::NeedsApproval,
            ..
        }
    ));
    thread.transcript.resolve_permission(7, false);
    assert!(thread.transcript.permissions().is_empty());
    assert!(matches!(
        &thread.transcript.entries()[0],
        AgentThreadEntry::Tool {
            status: AgentToolStatusModel::Pending,
            ..
        }
    ));
}

#[test]
fn transport_loss_clears_approvals_and_cancels_inflight_tools() {
    let mut thread = thread();
    thread.begin_prompt("make a change".to_owned(), Vec::new());
    thread.apply_update(SessionUpdate::ToolCall(
        ToolCall::new("tool-1", "Edit file")
            .kind(ToolKind::Edit)
            .status(ToolCallStatus::InProgress),
    ));
    thread.transcript.request_permission(
        7,
        ToolCallUpdate::new("tool-1", ToolCallUpdateFields::new()),
        vec![PermissionOption::new(
            "allow-once",
            "Allow once",
            PermissionOptionKind::AllowOnce,
        )],
    );

    thread.transcript.cancel_inflight();

    assert!(thread.transcript.permissions().is_empty());
    assert!(matches!(
        thread.transcript.entries().last(),
        Some(AgentThreadEntry::Tool {
            status: AgentToolStatusModel::Canceled,
            ..
        })
    ));
}

#[test]
fn acp_v1_rejects_permission_options_with_an_unknown_kind() {
    assert!(
        serde_json::from_value::<PermissionOption>(serde_json::json!({
            "optionId": "pick-a",
            "name": "Option A",
            "kind": "answer"
        }))
        .is_err()
    );
}

#[test]
fn status_only_updates_never_retype_a_typed_tool() {
    let mut thread = thread();
    thread.apply_update(SessionUpdate::ToolCall(
        ToolCall::new("tool-1", "cargo test")
            .kind(ToolKind::Execute)
            .status(ToolCallStatus::InProgress),
    ));
    thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "tool-1",
        ToolCallUpdateFields::new()
            .kind(ToolKind::Other)
            .status(ToolCallStatus::Completed)
            .raw_output(serde_json::json!({"ok": true})),
    )));
    assert!(matches!(
        &thread.transcript.entries()[0],
        AgentThreadEntry::Tool {
            kind: AgentToolKindModel::Execute,
            status: AgentToolStatusModel::Completed,
            label,
            ..
        } if label == "cargo test"
    ));

    thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "tool-1",
        ToolCallUpdateFields::new().title(String::new()),
    )));
    assert!(matches!(
        &thread.transcript.entries()[0],
        AgentThreadEntry::Tool { label, .. } if label == "cargo test"
    ));

    thread.apply_update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        "tool-1",
        ToolCallUpdateFields::new()
            .kind(ToolKind::Other)
            .title("Something else".to_owned()),
    )));
    assert!(matches!(
        &thread.transcript.entries()[0],
        AgentThreadEntry::Tool {
            kind: AgentToolKindModel::Other,
            label,
            ..
        } if label == "Something else"
    ));
}

#[test]
fn oversized_tool_payloads_are_capped_on_char_boundaries() {
    let text = capped("é".repeat(16), TRUNCATION_MARKER.len() + 5);
    assert_eq!(text, "éé".to_owned() + TRUNCATION_MARKER);

    let mut thread = thread();
    thread.apply_update(SessionUpdate::ToolCall(
        ToolCall::new("tool-big", "Read file").content(vec![ToolCallContent::from(
            ContentBlock::Text(TextContent::new("é".repeat(MAX_TOOL_PAYLOAD_BYTES))),
        )]),
    ));
    assert!(matches!(
        &thread.transcript.entries()[0],
        AgentThreadEntry::Tool { output, .. }
            if matches!(
                output.as_slice(),
                [ToolPayload::Text(text)]
                    if text.len() <= MAX_TOOL_PAYLOAD_BYTES
                        && text.ends_with(TRUNCATION_MARKER)
            )
    ));
}
