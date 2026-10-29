use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::Value;
use zz_protocol::chat::{ChatEvent, ChatView};
use zz_protocol::{
    AgentConnectionPhase, AgentImage, AgentPaneWire, AgentPermissionWire, AgentProvider,
    ClientInstanceId, agent_stream::AgentQuestion,
};

use crate::{
    agent_completion::AgentCommand,
    agent_transcript::{
        AgentPermissionKind, AgentPermissionRequest, AgentTranscript, ImageDecoder,
    },
};

pub const MAX_AGENT_HISTORY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_AGENT_HISTORY_ITEMS: usize = 10_000;

#[derive(Default)]
pub struct AgentCursor {
    pub last_seq: u64,
    pub replay_pending: bool,
    pub bytes: usize,
    pub history_supported: bool,
    pub session_load_supported: bool,
    pub session_delete_supported: bool,
    pub verbs_supported: bool,
    pub watch: bool,
    pub commands: Arc<[AgentCommand]>,
    command_session_reset: bool,
}

#[derive(serde::Deserialize)]
struct AgentEnvelope<'a> {
    seq: u64,
    #[serde(borrow)]
    item: &'a str,
    #[serde(default)]
    restoring: bool,
    #[serde(default)]
    capabilities: AgentCapabilities,
    #[serde(default)]
    update: Option<AgentCommandUpdate>,
    #[serde(default)]
    replay: Vec<AgentCommandUpdate>,
}

#[derive(serde::Deserialize)]
struct AgentCommandUpdate {
    #[serde(default, rename = "sessionUpdate")]
    kind: Value,
    #[serde(default, rename = "availableCommands")]
    commands: Value,
}

#[derive(Default, serde::Deserialize)]
struct AgentCapabilities {
    #[serde(default)]
    list: bool,
    #[serde(default)]
    load: bool,
    #[serde(default)]
    delete: bool,
    #[serde(default)]
    verbs: bool,
    #[serde(default)]
    watch: bool,
}

impl AgentCursor {
    pub fn apply(
        &mut self,
        journal: &mut Vec<(u64, Vec<u8>)>,
        first_seq: u64,
        items: &[Vec<u8>],
    ) -> (Option<u64>, bool) {
        let mut accepted = false;
        let mut invalid = false;
        for (offset, bytes) in items.iter().enumerate() {
            let positional = first_seq.saturating_add(offset as u64);
            let Ok(item) = serde_json::from_slice::<AgentEnvelope<'_>>(bytes) else {
                if positional > self.last_seq.saturating_add(1) {
                    let request = (!self.replay_pending).then_some(self.last_seq);
                    self.replay_pending = true;
                    return (request, true);
                }
                self.last_seq = self.last_seq.max(positional);
                invalid = true;
                continue;
            };
            if item.seq <= self.last_seq {
                continue;
            }
            if item.item == "sessionReset" && item.restoring {
                self.last_seq = item.seq.saturating_sub(1);
            }
            if item.seq > self.last_seq.saturating_add(1) {
                let request = (!self.replay_pending).then_some(self.last_seq);
                self.replay_pending = true;
                return (request, invalid);
            }
            if item.item == "ready" {
                self.history_supported = item.capabilities.list;
                self.session_load_supported = item.capabilities.load;
                self.session_delete_supported = item.capabilities.delete;
                self.verbs_supported = item.capabilities.verbs;
                self.watch = item.capabilities.watch;
            }
            if item.item == "sessionReset" {
                journal.clear();
                self.bytes = 0;
                self.commands = Arc::from([]);
                self.command_session_reset = true;
            } else if item.item == "sessionReady" {
                self.command_session_reset = false;
            } else if item.item == "sessionSwitched" {
                if !self.command_session_reset {
                    self.commands = Arc::from([]);
                }
                self.command_session_reset = false;
            }
            for update in item.update.iter().chain(&item.replay) {
                if update.kind.as_str() == Some("available_commands_update") {
                    self.commands = update
                        .commands
                        .as_array()
                        .into_iter()
                        .flatten()
                        .take(zz_protocol::MAX_AGENT_AVAILABLE_COMMANDS)
                        .filter_map(|command| {
                            Some(AgentCommand {
                                name: command.get("name")?.as_str()?.to_owned(),
                                description: command
                                    .get("description")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                                input_hint: command
                                    .pointer("/input/hint")
                                    .and_then(Value::as_str)
                                    .map(ToOwned::to_owned),
                            })
                        })
                        .collect::<Vec<_>>()
                        .into();
                }
            }
            journal.push((item.seq, bytes.clone()));
            self.bytes = self.bytes.saturating_add(bytes.len());
            self.last_seq = item.seq;
            accepted = true;
        }
        if accepted {
            self.replay_pending = false;
        }
        (None, invalid)
    }

    pub fn trim(
        &mut self,
        journal: &mut Vec<(u64, Vec<u8>)>,
        byte_limit: usize,
        item_limit: usize,
    ) -> bool {
        let mut remove = 0;
        while remove < journal.len()
            && (self.bytes > byte_limit || journal.len() - remove > item_limit)
        {
            self.bytes = self.bytes.saturating_sub(journal[remove].1.len());
            remove += 1;
        }
        journal.drain(..remove);
        remove > 0
    }

    pub fn replay_from(&mut self) -> u64 {
        self.replay_pending = true;
        self.last_seq
    }
}

pub struct LensTranscript<I> {
    pub model: AgentTranscript<I>,
    pub chat: ChatView,
    decode: ImageDecoder<I>,
    session_reset: bool,
}

impl<I> LensTranscript<I> {
    pub fn new(decode: ImageDecoder<I>) -> Self {
        Self {
            model: AgentTranscript::new(decode),
            chat: ChatView::default(),
            decode,
            session_reset: false,
        }
    }

    fn restart(&mut self) {
        self.model = AgentTranscript::new(self.decode);
        self.session_reset = false;
    }

    pub fn apply(&mut self, item: &Value) -> bool {
        let mut restarted = false;
        match item.get("item").and_then(Value::as_str).unwrap_or_default() {
            "sessionReset" => {
                self.restart();
                restarted = true;
                self.session_reset = true;
            }
            "sessionReady" => {
                self.session_reset = false;
                self.model.finish_replay();
                self.model.finish_turn();
            }
            "sessionSwitched" => {
                if !self.session_reset {
                    self.restart();
                    restarted = true;
                }
                for update in item
                    .get("replay")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Ok(update) = serde_json::from_value(update.clone()) {
                        self.model.apply_update(update);
                    }
                }
                self.session_reset = false;
                self.model.finish_replay();
            }
            "chat" => {
                if let Some(event) = item.get("event")
                    && let Ok(event) = serde_json::from_value::<ChatEvent>(event.clone())
                {
                    let _ = self.chat.apply(&event);
                }
            }
            "update" => {
                if let Some(update) = item.get("update")
                    && let Ok(update) = serde_json::from_value(update.clone())
                {
                    self.model.apply_update(update);
                }
            }
            "permissionRequested" => {
                if let (Some(request_id), Some(tool), Some(options)) = (
                    item.get("request_id").and_then(Value::as_u64),
                    item.get("tool_call"),
                    item.get("options"),
                ) && let (Ok(tool), Ok(options)) = (
                    serde_json::from_value(tool.clone()),
                    serde_json::from_value(options.clone()),
                ) {
                    self.model.request_questions(
                        request_id,
                        tool,
                        options,
                        permission_questions(item),
                    );
                }
            }
            "permissionResolved" => {
                if let Some(request_id) = item.get("request_id").and_then(Value::as_u64) {
                    self.model.resolve_permission(
                        request_id,
                        item.get("canceled")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    );
                }
            }
            "promptFinished" => {
                if item.pointer("/outcome/outcome").and_then(Value::as_str) == Some("failed") {
                    self.model.fail_inflight();
                } else if item.pointer("/outcome/stop_reason").and_then(Value::as_str)
                    == Some("cancelled")
                {
                    self.model.cancel_inflight();
                } else {
                    self.model.finish_turn();
                }
                self.model.finish_replay();
            }
            "paneFailed" => self.model.fail_inflight(),
            _ => {}
        }
        restarted
    }
}

#[must_use]
pub fn conversation_changed(item: &Value) -> bool {
    matches!(
        item.get("item").and_then(Value::as_str),
        Some("sessionReset" | "sessionSwitched")
    )
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PermissionChoice {
    pub id: String,
    pub name: String,
    pub allow: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PendingPermission {
    pub request_id: u64,
    pub title: String,
    pub options: Vec<PermissionChoice>,
    pub questions: Vec<AgentQuestion>,
}

#[must_use]
pub fn permission_choices(payload: &Value) -> Vec<PermissionChoice> {
    payload
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|option| {
            let id = option
                .get("optionId")
                .or_else(|| option.get("id"))?
                .as_str()?
                .to_owned();
            Some(PermissionChoice {
                name: option
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_owned(),
                allow: match option.get("kind").and_then(Value::as_str) {
                    Some("allow_once" | "allow_always") => Some(true),
                    Some("reject_once" | "reject_always") => Some(false),
                    _ => None,
                },
                id,
            })
        })
        .collect()
}

#[must_use]
pub fn permission_questions(payload: &Value) -> Vec<AgentQuestion> {
    payload
        .get("questions")
        .and_then(|questions| serde_json::from_value(questions.clone()).ok())
        .unwrap_or_default()
}

#[must_use]
pub fn pending_permissions(
    requests: &[AgentPermissionRequest],
    parked: Option<&AgentPermissionWire>,
) -> Vec<PendingPermission> {
    let mut pending = requests
        .iter()
        .map(|request| PendingPermission {
            request_id: request.request_id,
            title: request.title.clone(),
            questions: request.questions.clone(),
            options: request
                .options
                .iter()
                .map(|option| PermissionChoice {
                    id: option.id.clone(),
                    name: option.name.clone(),
                    allow: Some(matches!(
                        option.kind,
                        AgentPermissionKind::AllowOnce | AgentPermissionKind::AllowAlways
                    )),
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    if let Some(permission) = parked
        && !pending
            .iter()
            .any(|request| request.request_id == permission.request_id)
    {
        let payload = serde_json::from_str::<Value>(&permission.payload).unwrap_or_default();
        pending.push(PendingPermission {
            request_id: permission.request_id,
            title: payload
                .pointer("/tool_call/title")
                .or_else(|| payload.pointer("/toolCall/title"))
                .and_then(Value::as_str)
                .unwrap_or("This agent needs your permission")
                .to_owned(),
            options: permission_choices(&payload),
            questions: permission_questions(&payload),
        });
    }
    pending
}

#[must_use]
pub fn restored_prompts(
    update: &Value,
    client: Option<ClientInstanceId>,
) -> Vec<(String, Vec<AgentImage>)> {
    update
        .get("prompts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|prompt| {
            let owner = match prompt.get("owner") {
                Some(owner) => serde_json::from_value::<ClientInstanceId>(owner.clone()).ok()?,
                None => ClientInstanceId::default(),
            };
            if owner.0 != 0 && Some(owner) != client {
                return None;
            }
            let text =
                String::from_utf8(BASE64.decode(prompt.get("text")?.as_str()?).ok()?).ok()?;
            let images = prompt
                .get("images")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|image| {
                    Some(AgentImage {
                        format: image.get("format")?.as_str()?.to_owned(),
                        data: BASE64.decode(image.get("data")?.as_str()?).ok()?,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some((text, images))
        })
        .collect()
}

#[must_use]
pub fn restore_reclaim_id(update: &Value) -> Option<Option<u64>> {
    matches!(
        update.get("item").and_then(Value::as_str),
        Some("promptsReclaimed" | "promptsRestored")
    )
    .then(|| {
        update
            .get("reclaim_id")
            .or_else(|| update.get("reclaimId"))
            .and_then(Value::as_u64)
    })
}

#[must_use]
pub fn usage_from_update(update: &Value) -> Option<(u64, u64)> {
    let update = update.get("update").unwrap_or(update);
    (update.get("sessionUpdate").and_then(Value::as_str) == Some("usage_update"))
        .then(|| Some((update.get("used")?.as_u64()?, update.get("size")?.as_u64()?)))?
}

pub fn track_usage(usage: &mut Option<(u64, u64)>, update: &Value) {
    if conversation_changed(update) {
        *usage = None;
        for update in update
            .get("replay")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            track_usage(usage, update);
        }
    }
    if let Some(found) = usage_from_update(update) {
        *usage = Some(found);
    }
}

#[must_use]
pub const fn turn_running(phase: &AgentConnectionPhase) -> bool {
    matches!(
        phase,
        AgentConnectionPhase::Running | AgentConnectionPhase::AwaitingPermission
    )
}

#[must_use]
pub fn accepts_prompt(state: &AgentPaneWire) -> bool {
    turn_running(&state.phase)
        || (state.phase == AgentConnectionPhase::Ready && state.pending_permission.is_none())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LensEmpty {
    Welcome(AgentProvider),
    Message { text: String, busy: bool },
}

#[must_use]
pub fn lens_welcome_title(provider: AgentProvider) -> String {
    format!("{} is waiting for its first prompt", provider.label())
}

pub const LENS_WELCOME_DETAIL: &str = "What you send here is typed into the terminal.";

#[must_use]
pub fn lens_placeholder(provider: Option<AgentProvider>) -> String {
    format!(
        "Message {}…",
        provider.map_or("the agent", AgentProvider::label)
    )
}

#[must_use]
pub fn lens_empty(
    connected: bool,
    phase: &AgentConnectionPhase,
    provider: Option<AgentProvider>,
) -> LensEmpty {
    let agent = provider.map_or("the agent", AgentProvider::label);
    let text = if connected {
        match phase {
            AgentConnectionPhase::Starting => format!("Reading {agent}’s transcript…"),
            AgentConnectionPhase::Ready => match provider {
                Some(provider) => return LensEmpty::Welcome(provider),
                None => format!("{agent} is waiting for its first prompt."),
            },
            AgentConnectionPhase::Running => format!("Waiting for {agent}’s first update…"),
            AgentConnectionPhase::AwaitingPermission => {
                "The agent needs your permission.".to_owned()
            }
            AgentConnectionPhase::Failed { .. } => {
                format!("Could not read {agent}’s transcript.")
            }
        }
    } else {
        "The agent is offline.".to_owned()
    };
    LensEmpty::Message {
        text,
        busy: connected
            && matches!(
                phase,
                AgentConnectionPhase::Starting | AgentConnectionPhase::Running
            ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn blob(seq: u64) -> Vec<u8> {
        format!(r#"{{"seq":{seq},"item":"promptAccepted","turn_id":1}}"#).into_bytes()
    }

    fn reset(seq: u64, restoring: bool) -> Vec<u8> {
        format!(r#"{{"seq":{seq},"item":"sessionReset","restoring":{restoring}}}"#).into_bytes()
    }

    #[test]
    fn agent_commands_survive_history_trimming_and_follow_session_replays() {
        let catalog = |name| {
            serde_json::json!({
                "sessionUpdate":"available_commands_update",
                "availableCommands":[{"name":name,"description":"Review changes","input":{"hint":"branch or files"}}]
            })
        };
        let message = |seq, update| {
            serde_json::to_vec(&serde_json::json!({"seq":seq,"item":"update","update":update}))
                .unwrap()
        };
        let mut cursor = AgentCursor::default();
        let mut journal = Vec::new();
        let initial = message(1, catalog("review"));
        assert_eq!(
            cursor.apply(&mut journal, 1, std::slice::from_ref(&initial)),
            (None, false)
        );
        assert_eq!(cursor.commands[0].name, "review");
        assert_eq!(
            cursor.commands[0].input_hint.as_deref(),
            Some("branch or files")
        );
        assert!(cursor.trim(&mut journal, 0, 0));
        assert!(journal.is_empty());
        assert_eq!(cursor.commands[0].name, "review");
        cursor.apply(&mut journal, 2, &[reset(2, true)]);
        assert!(cursor.commands.is_empty());
        cursor.apply(&mut journal, 3, &[message(3, catalog("explain"))]);
        cursor.apply(
            &mut journal,
            4,
            &[br#"{"seq":4,"item":"sessionSwitched","replay":[]}"#.to_vec()],
        );
        assert_eq!(cursor.commands[0].name, "explain");
        cursor.apply(&mut journal, 1, &[initial]);
        assert_eq!(cursor.commands[0].name, "explain");
        let switched = serde_json::to_vec(
            &serde_json::json!({"seq":5,"item":"sessionSwitched","replay":[catalog("test")]}),
        )
        .unwrap();
        cursor.apply(&mut journal, 5, &[switched]);
        assert_eq!(cursor.commands.len(), 1);
        assert_eq!(cursor.commands[0].name, "test");
        cursor.apply(&mut journal, 6, &[message(6, serde_json::json!({"sessionUpdate":"available_commands_update","availableCommands":[]}))]);
        assert!(cursor.commands.is_empty());
    }

    #[test]
    fn agent_replay_accepts_sequence_one_and_deduplicates_overlap() {
        let mut cursor = AgentCursor::default();
        let mut journal = Vec::new();
        assert_eq!(
            cursor.apply(&mut journal, 1, &[blob(1), blob(2)]),
            (None, false)
        );
        assert_eq!(
            cursor.apply(&mut journal, 1, &[blob(1), blob(2), blob(3)]),
            (None, false)
        );
        assert_eq!(
            journal.iter().map(|(seq, _)| *seq).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(cursor.last_seq, 3);
        assert_eq!(
            cursor.bytes,
            journal.iter().map(|(_, bytes)| bytes.len()).sum::<usize>()
        );
    }

    #[test]
    fn agent_gap_requests_one_inclusive_replay_until_it_closes() {
        let mut cursor = AgentCursor::default();
        let mut journal = Vec::new();
        cursor.apply(&mut journal, 1, &[blob(1)]);
        assert_eq!(cursor.apply(&mut journal, 4, &[blob(4)]), (Some(1), false));
        assert_eq!(cursor.apply(&mut journal, 5, &[blob(5)]), (None, false));
        assert_eq!(cursor.apply(&mut journal, 1, &[blob(1)]), (None, false));
        assert!(cursor.replay_pending);
        assert_eq!(
            cursor.apply(&mut journal, 1, &[blob(1), blob(2), blob(3), blob(4)]),
            (None, false)
        );
        assert!(!cursor.replay_pending);
        assert_eq!(cursor.last_seq, 4);
        assert_eq!(cursor.apply(&mut journal, 7, &[blob(7)]), (Some(4), false));
    }

    #[test]
    fn evicted_agent_history_accepts_a_restoring_reset_at_a_new_sequence() {
        let mut cursor = AgentCursor::default();
        let mut journal = Vec::new();
        cursor.apply(&mut journal, 1, &[blob(1), blob(2)]);
        assert_eq!(
            cursor.apply(&mut journal, 40, &[reset(40, true), blob(41)]),
            (None, false)
        );
        assert_eq!(
            journal.iter().map(|(seq, _)| *seq).collect::<Vec<_>>(),
            [40, 41]
        );
        assert_eq!(cursor.last_seq, 41);
        assert!(!cursor.replay_pending);
        assert_eq!(
            cursor.bytes,
            journal.iter().map(|(_, bytes)| bytes.len()).sum::<usize>()
        );
        assert_eq!(
            cursor.apply(&mut journal, 80, &[reset(80, false)]),
            (Some(41), false)
        );
    }

    #[test]
    fn session_switch_completion_keeps_streamed_history() {
        let mut cursor = AgentCursor::default();
        let mut journal = Vec::new();
        let switched = br#"{"seq":3,"item":"sessionSwitched","replay":[]}"#.to_vec();
        cursor.apply(&mut journal, 1, &[reset(1, true), blob(2), switched]);
        assert_eq!(
            journal.iter().map(|(seq, _)| *seq).collect::<Vec<_>>(),
            [1, 2, 3]
        );
    }

    #[test]
    fn agent_capabilities_survive_session_reset_and_transcript_eviction() {
        let mut cursor = AgentCursor::default();
        let mut journal = Vec::new();
        assert!(!cursor.history_supported);
        assert!(!cursor.session_load_supported);
        assert!(!cursor.session_delete_supported);
        let ready =
            br#"{"seq":1,"item":"ready","capabilities":{"list":true,"load":true,"delete":true}}"#
                .to_vec();
        assert_eq!(
            cursor.apply(&mut journal, 1, &[ready.clone(), reset(2, false), blob(3)]),
            (None, false)
        );
        assert!(cursor.history_supported);
        assert!(cursor.session_load_supported);
        assert!(cursor.session_delete_supported);
        assert_eq!(journal[0].0, 2);
        cursor.trim(&mut journal, 0, 0);
        assert!(cursor.history_supported);
        assert!(cursor.session_load_supported);
        assert!(cursor.session_delete_supported);
        assert!(journal.is_empty());
        let unsupported = br#"{"seq":4,"item":"ready","capabilities":{"list":false}}"#.to_vec();
        cursor.apply(&mut journal, 4, &[unsupported]);
        cursor.apply(&mut journal, 1, &[ready]);
        assert!(!cursor.history_supported);
        assert!(!cursor.session_load_supported);
        assert!(!cursor.session_delete_supported);
    }

    #[test]
    fn agent_history_limits_bytes_and_items_without_rewinding_replay() {
        let mut cursor = AgentCursor::default();
        let mut journal = Vec::new();
        cursor.apply(&mut journal, 1, &[blob(1), blob(2), blob(3)]);
        let limit = blob(2).len() + blob(3).len();
        assert!(cursor.trim(&mut journal, limit, 10));
        assert_eq!(
            journal.iter().map(|(seq, _)| *seq).collect::<Vec<_>>(),
            [2, 3]
        );
        assert_eq!(cursor.bytes, limit);
        assert_eq!(cursor.last_seq, 3);
        assert!(cursor.trim(&mut journal, limit, 1));
        assert_eq!(journal[0].0, 3);
        assert_eq!(
            cursor.apply(&mut journal, 3, &[blob(3), blob(4)]),
            (None, false)
        );
        assert_eq!(
            journal.iter().map(|(seq, _)| *seq).collect::<Vec<_>>(),
            [3, 4]
        );
    }

    #[test]
    fn invalid_agent_items_do_not_skip_an_unseen_gap() {
        let mut cursor = AgentCursor::default();
        let mut journal = Vec::new();
        assert_eq!(
            cursor.apply(&mut journal, 3, &[b"invalid".to_vec()]),
            (Some(0), true)
        );
        assert_eq!(cursor.last_seq, 0);
        assert_eq!(
            cursor.apply(&mut journal, 1, &[b"invalid".to_vec(), blob(2)]),
            (None, true)
        );
        assert_eq!(cursor.last_seq, 2);
        assert_eq!(journal[0].0, 2);
    }

    #[test]
    fn usage_reads_live_and_history_updates() {
        let update = json!({"sessionUpdate":"usage_update","used":400,"size":1000});
        assert_eq!(usage_from_update(&update), Some((400, 1000)));
        assert_eq!(
            usage_from_update(&json!({"item":"update","update":update})),
            Some((400, 1000))
        );
        assert_eq!(
            usage_from_update(&json!({"sessionUpdate":"usage_update","used":1})),
            None
        );
        let mut usage = Some((1, 2));
        track_usage(
            &mut usage,
            &json!({"item":"sessionSwitched","replay":[update]}),
        );
        assert_eq!(usage, Some((400, 1000)));
        track_usage(&mut usage, &json!({"item":"sessionReset"}));
        assert_eq!(usage, None);
    }

    #[test]
    fn restored_drafts_keep_unicode_images_and_only_the_owners_prompts() {
        let update = json!({"item":"promptsRestored","reclaim_id":3,"prompts":[
            {"owner":7,"text":BASE64.encode("héllo"),"images":[{"format":"image/png","data":BASE64.encode([1,2,3])}]},
            {"owner":8,"text":BASE64.encode("someone else's draft"),"images":[]},
            {"owner":0,"text":BASE64.encode("legacy"),"images":[]},
            {"owner":"bad","text":BASE64.encode("invalid owner"),"images":[]}
        ]});
        let prompts = restored_prompts(&update, Some(ClientInstanceId(7)));
        assert_eq!(prompts.len(), 2);
        assert_eq!(prompts[0].0, "héllo");
        assert_eq!(prompts[0].1[0].data, [1, 2, 3]);
        assert_eq!(prompts[1].0, "legacy");
        assert_eq!(restored_prompts(&update, None).len(), 1);
        assert_eq!(restore_reclaim_id(&update), Some(Some(3)));
        assert_eq!(restore_reclaim_id(&json!({"item":"update"})), None);
    }

    #[test]
    fn a_parked_permission_joins_the_streamed_ones_once() {
        let parked = AgentPermissionWire {
            request_id: 4,
            payload: json!({"tool_call":{"title":"Run ls"},"options":[
                {"optionId":"yes","name":"Allow","kind":"allow_once"},
                {"optionId":"no","name":"Deny","kind":"reject_once"}
            ]})
            .to_string(),
        };
        let pending = pending_permissions(&[], Some(&parked));
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].title, "Run ls");
        assert_eq!(
            pending[0]
                .options
                .iter()
                .map(|option| option.allow)
                .collect::<Vec<_>>(),
            [Some(true), Some(false)]
        );
    }

    #[test]
    fn empty_lens_names_the_agent_and_its_phase() {
        assert_eq!(
            lens_empty(
                true,
                &AgentConnectionPhase::Ready,
                Some(AgentProvider::Codex)
            ),
            LensEmpty::Welcome(AgentProvider::Codex)
        );
        assert_eq!(
            lens_empty(
                true,
                &AgentConnectionPhase::Starting,
                Some(AgentProvider::Codex)
            ),
            LensEmpty::Message {
                text: "Reading Codex’s transcript…".to_owned(),
                busy: true
            }
        );
        assert_eq!(
            lens_empty(false, &AgentConnectionPhase::Ready, None),
            LensEmpty::Message {
                text: "The agent is offline.".to_owned(),
                busy: false
            }
        );
    }

    #[test]
    fn chat_items_fold_into_the_chat_view() {
        use zz_protocol::agent_stream::{AgentStreamItem, AgentStreamPayload};
        use zz_protocol::chat::{ChatPhase, TurnBy, TurnId};
        let mut transcript = LensTranscript::<()>::new(|_, _| None);
        for (seq, event) in [
            ChatEvent::SessionReady {
                session: "s".to_owned(),
            },
            ChatEvent::TurnStarted {
                turn: TurnId(1),
                by: TurnBy::User { prompt: 1 },
                at: 0,
            },
        ]
        .into_iter()
        .enumerate()
        {
            let item = serde_json::to_value(AgentStreamItem {
                seq: seq as u64 + 1,
                payload: AgentStreamPayload::Chat { event },
            })
            .expect("encode");
            transcript.apply(&item);
        }
        assert_eq!(transcript.chat.phase(), ChatPhase::Running);
        assert_eq!(transcript.chat.skipped, 0);
    }
}
