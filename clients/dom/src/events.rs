use serde_json::{Value, json};
use zz_client::{AgentAttentionEdge, CoreEvent, ViewportDamage};

use crate::Client;

fn value(item: &impl serde::Serialize) -> Value {
    serde_json::to_value(item).unwrap_or(Value::Null)
}

fn variant(debug: &str) -> String {
    debug
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or_default()
        .to_owned()
}

pub fn describe(client: &mut Client, event: CoreEvent) -> Value {
    crate::terminal::observe(client, &event);
    match event {
        CoreEvent::HelloReceived => json!({ "type": "hello" }),
        CoreEvent::Attached { session } => json!({ "type": "attached", "session": session.0 }),
        CoreEvent::SnapshotChanged => json!({ "type": "tree" }),
        CoreEvent::ViewportChanged { pane, damage } => json!({
            "type": "viewport",
            "pane": pane.0,
            "rows": match damage {
                ViewportDamage::All => Value::Null,
                ViewportDamage::Rows(rows) => json!(rows),
            },
        }),
        CoreEvent::AppearanceChanged => json!({ "type": "appearance" }),
        CoreEvent::MuxOptionsChanged => json!({ "type": "options" }),
        CoreEvent::KeyTablesChanged => json!({ "type": "keyTables" }),
        CoreEvent::StatusChanged => json!({ "type": "status" }),
        CoreEvent::PrefixArmed { armed } => json!({ "type": "prefix", "armed": armed }),
        CoreEvent::PrefixCancelled { request_id } => {
            json!({ "type": "prefixCancelled", "requestId": request_id })
        }
        CoreEvent::KeyTableChanged => json!({ "type": "keyTable" }),
        CoreEvent::CommandPromptChanged => json!({ "type": "commandPrompt" }),
        CoreEvent::CommandOutputChanged => json!({ "type": "commandOutput" }),
        CoreEvent::ChooseTreeChanged => json!({ "type": "chooseTree" }),
        CoreEvent::ChooseBufferChanged => json!({ "type": "chooseBuffer" }),
        CoreEvent::DisplayPanesChanged => json!({ "type": "displayPanes" }),
        CoreEvent::MenuChanged => json!({ "type": "menu" }),
        CoreEvent::ConfirmChanged => json!({ "type": "confirm" }),
        CoreEvent::PaneRemoved { pane } => json!({ "type": "paneRemoved", "pane": pane.0 }),
        CoreEvent::Bell { pane } => json!({ "type": "bell", "pane": pane.0 }),
        CoreEvent::FocusSidebar => json!({ "type": "focusSidebar" }),
        CoreEvent::OpenPathPicker { pane, start_dir } => {
            json!({ "type": "pathPicker", "pane": pane.0, "startDir": start_dir })
        }
        CoreEvent::Detached {
            session,
            by,
            action,
        } => json!({
            "type": "detached",
            "session": session.0,
            "by": by,
            "action": format!("{action:?}"),
        }),
        CoreEvent::ServerStopping => json!({ "type": "serverStopping" }),
        CoreEvent::CommandResponse(response) => {
            json!({ "type": "commandResponse", "response": value(&response) })
        }
        CoreEvent::ClientMessage {
            pane,
            kind,
            text,
            duration_ms,
            message_id,
        } => json!({
            "type": "clientMessage",
            "pane": pane.map(|pane| pane.0),
            "kind": value(&kind),
            "text": text,
            "durationMs": duration_ms,
            "messageId": message_id,
        }),
        CoreEvent::ClientMessageCleared { message_id } => {
            json!({ "type": "clientMessageCleared", "messageId": message_id })
        }
        CoreEvent::Clipboard {
            pane,
            request_id,
            target,
            text,
            producer,
        } => json!({
            "type": "clipboard",
            "pane": pane.0,
            "requestId": request_id,
            "target": value(&target),
            "text": text,
            "producer": value(&producer),
        }),
        CoreEvent::OpenUri { pane, uri } => {
            json!({ "type": "openUri", "pane": pane.0, "uri": uri })
        }
        CoreEvent::BrowserCommand { pane, command } => {
            json!({ "type": "browserCommand", "pane": pane.0, "command": value(&command) })
        }
        CoreEvent::TerminalUiCommand { pane, command } => json!({
            "type": "terminalUiCommand",
            "pane": pane.0,
            "command": format!("{command:?}"),
            "terminal": crate::terminal::ui_command(client, pane, command),
        }),
        CoreEvent::HistoryChunk {
            pane,
            start,
            total,
            offset,
            columns,
            rows,
            dictionary,
        } => {
            let resume = crate::terminal::history_chunk(
                client, pane, start, total, offset, columns, rows, dictionary,
            );
            json!({ "type": "history", "pane": pane.0, "start": start, "total": total, "resume": resume })
        }
        CoreEvent::KittyImageBegin {
            pane,
            image_id,
            generation,
            width,
            height,
            total_bytes,
        } => {
            client
                .images
                .begin(pane, image_id, generation, width, height, total_bytes);
            json!({ "type": "kittyBegin", "pane": pane.0, "image": image_id })
        }
        CoreEvent::KittyImageChunk {
            pane,
            image_id,
            generation,
            bytes,
        } => {
            let complete = client.images.chunk(pane, image_id, generation, bytes);
            json!({ "type": "kittyChunk", "pane": pane.0, "image": image_id, "complete": complete })
        }
        CoreEvent::KittyImagesRemoved { pane, image_ids } => {
            client.images.remove(pane, &image_ids);
            json!({ "type": "kittyRemoved", "pane": pane.0, "images": image_ids })
        }
        CoreEvent::AgentUpdates {
            pane,
            first_seq,
            items,
        } => {
            client.agents.updates(pane, first_seq, items);
            json!({ "type": "agentUpdates", "pane": pane.0, "firstSeq": first_seq })
        }
        CoreEvent::AgentStateChanged { pane, attention } => json!({
            "type": "agentState",
            "pane": pane.0,
            "attention": attention.map(|edge| match edge {
                AgentAttentionEdge::Request => "request",
                AgentAttentionEdge::Done => "done",
                AgentAttentionEdge::Failed => "failed",
            }),
        }),
        CoreEvent::AgentLagged { pane, next_seq } => {
            client.agents.lagged(pane, next_seq);
            json!({ "type": "agentLagged", "pane": pane.0, "nextSeq": next_seq })
        }
        CoreEvent::AgentSessions {
            pane,
            request_id,
            result,
        } => json!({
            "type": "agentSessions",
            "pane": pane.0,
            "requestId": request_id,
            "result": result,
        }),
        CoreEvent::ProvidersChanged => json!({ "type": "providers" }),
        CoreEvent::Message(message) => {
            json!({ "type": "message", "name": variant(&format!("{message:?}")) })
        }
    }
}
