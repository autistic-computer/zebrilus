use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use zz_client::{
    agent_ask::{
        ApprovalTopic, KEYED_OPTIONS, approval_hints, approval_send, approval_topic,
        question_hints, question_other_key, question_send,
    },
    agent_completion::{
        AgentCommand, CommandCompletion, active_command_hint, bare_command_name, completion_query,
        meaningful_command_description, pane_commands, ranked_completions, rendered_error,
    },
    agent_flip::{AgentFlips, FlipMode, FlipTo, flip_target},
    agent_lens::{
        AGENT_WELCOME_DETAIL, AgentCursor, AgentEmpty, LensTranscript, MAX_AGENT_HISTORY_BYTES,
        MAX_AGENT_HISTORY_ITEMS, PendingPermission, accepts_prompt, agent_empty, agent_placeholder,
        agent_welcome_title, conversation_changed, pending_permissions, restore_reclaim_id,
        restored_prompts, track_usage, turn_running,
    },
    agent_question::{QuestionCard, QuestionCardStep},
    agent_timeline::{
        AgentToolKind, AgentToolStatus, ComposerAction, StepNesting, TRACE_COUNT_NOUNS,
        TimelineGroupKind, TimelineItem, TimelineRow, TimelineShape, TraceHeader, TurnClocks,
        composer_action, context_usage_fraction, context_usage_tooltip, ending_failure,
        final_assistant_entry_id, fold_timeline_rows, git_file_count_label, git_summary_tooltip,
        is_turn_row, plan_chip_label, plan_items, plan_progress, single_line, split_turn,
        thought_tail, tool_tag, trace_header,
    },
    agent_transcript::{AgentThreadEntry, ToolPayload, is_zz_command, rewind_command},
    mend::mend,
    providers::{
        provider_action, provider_description, provider_notes, provider_rows, version_current,
        version_text,
    },
    window_summary::pane_agent,
};
use zz_protocol::chat::{Ask, ItemKind, RequestId};
use zz_protocol::{
    AgentConnectionPhase, AgentPaneWire, AgentProvider, AgentQuestionAnswer, CommandInvocation,
    CommandResponse, PaneId, PaneKindSnapshot, PaneSnapshot, ProtocolMessage,
};

use crate::{Client, pane_id};

const MAX_STREAMING_MEND_BYTES: usize = 64 * 1024;

type Image = String;

fn decode_image(format: &str, data: Vec<u8>) -> Option<Image> {
    matches!(
        format,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
    .then(|| format!("data:{format};base64,{}", BASE64.encode(data)))
}

struct Lens {
    transcript: LensTranscript<Image>,
    last_sequence: u64,
    usage: Option<(u64, u64)>,
    last_reclaim_id: u64,
    answered: HashSet<u64>,
    permission_request: Option<u64>,
    permission_selected: usize,
    question: Option<QuestionCard>,
    expanded: HashMap<u64, bool>,
    clocks: TurnClocks,
    plan_open: bool,
    last_input: String,
    last_cursor: usize,
    completion_dismissed: bool,
    completion_selected: Option<usize>,
    completions: Vec<CommandCompletion>,
    commands: Arc<[AgentCommand]>,
    drafts: Vec<String>,
    markdown: HashMap<u64, (String, String)>,
}

impl Lens {
    fn new() -> Self {
        Self {
            transcript: LensTranscript::new(decode_image),
            last_sequence: 0,
            usage: None,
            last_reclaim_id: 0,
            answered: HashSet::new(),
            permission_request: None,
            permission_selected: 0,
            question: None,
            expanded: HashMap::new(),
            clocks: TurnClocks::default(),
            plan_open: false,
            last_input: String::new(),
            last_cursor: 0,
            completion_dismissed: false,
            completion_selected: None,
            completions: Vec::new(),
            commands: Arc::from([]),
            drafts: Vec::new(),
            markdown: HashMap::new(),
        }
    }

    fn reconnect(&mut self) {
        self.transcript = LensTranscript::new(decode_image);
        self.usage = None;
        self.last_sequence = 0;
        self.answered.clear();
        self.clear_timeline();
    }

    fn clear_timeline(&mut self) {
        self.expanded.clear();
        self.clocks.clear();
        self.markdown.clear();
    }

    fn synchronize(
        &mut self,
        journal: &[(u64, Vec<u8>)],
        client: zz_protocol::ClientInstanceId,
    ) -> Vec<u64> {
        let newest = journal.last().map_or(0, |(sequence, _)| *sequence);
        if newest == self.last_sequence {
            return Vec::new();
        }
        let skipped = journal.first().is_some_and(|(first, _)| {
            self.last_sequence > 0 && *first > self.last_sequence.saturating_add(1)
        });
        if newest < self.last_sequence || skipped {
            self.transcript = LensTranscript::new(decode_image);
            self.clear_timeline();
            self.last_sequence = 0;
        }
        let start = journal.partition_point(|(sequence, _)| *sequence <= self.last_sequence);
        let mut acknowledgements = Vec::new();
        for (_, bytes) in &journal[start..] {
            let Ok(update) = serde_json::from_slice::<Value>(bytes) else {
                continue;
            };
            if conversation_changed(&update) {
                self.answered.clear();
                self.clear_timeline();
            }
            if let Some(reclaim_id) = restore_reclaim_id(&update)
                && reclaim_id.is_none_or(|id| id > self.last_reclaim_id)
            {
                let prompts = restored_prompts(&update, Some(client));
                if !prompts.is_empty() {
                    self.drafts.extend(
                        prompts
                            .into_iter()
                            .map(|(text, _)| text)
                            .filter(|text| !text.is_empty()),
                    );
                    if let Some(reclaim_id) = reclaim_id {
                        self.last_reclaim_id = reclaim_id;
                        acknowledgements.push(reclaim_id);
                    }
                }
            }
            track_usage(&mut self.usage, &update);
            if self.transcript.apply(&update) {
                self.markdown.clear();
            }
        }
        self.last_sequence = newest;
        acknowledgements
    }

    fn html(&mut self, id: u64, source: &str, streaming: bool) -> String {
        let mended = (streaming && source.len() <= MAX_STREAMING_MEND_BYTES)
            .then(|| mend(source))
            .flatten();
        let display = mended.as_deref().unwrap_or(source);
        if let Some((cached, html)) = self.markdown.get(&id)
            && cached == display
        {
            return html.clone();
        }
        let html = markdown_html(display);
        self.markdown.insert(id, (display.to_owned(), html.clone()));
        html
    }

    fn synchronize_completions(
        &mut self,
        value: &str,
        cursor: usize,
        commands: Arc<[AgentCommand]>,
    ) {
        let input_changed = value != self.last_input || cursor != self.last_cursor;
        if !input_changed && self.commands == commands {
            return;
        }
        if input_changed {
            value.clone_into(&mut self.last_input);
            self.last_cursor = cursor;
            self.completion_dismissed = false;
        }
        self.commands = commands;
        self.completions = if self.completion_dismissed {
            Vec::new()
        } else {
            completion_query(&self.last_input, self.last_cursor)
                .map(|query| ranked_completions(&self.commands, &query))
                .unwrap_or_default()
        };
        self.completion_selected = (!self.completions.is_empty()).then(|| {
            self.completion_selected
                .unwrap_or_default()
                .min(self.completions.len().saturating_sub(1))
        });
    }

    fn dismiss_completions(&mut self) {
        self.completion_dismissed = true;
        self.completions.clear();
        self.completion_selected = None;
    }
}

#[derive(Clone)]
struct Item {
    index: usize,
    id: u64,
    parent: Option<u64>,
    shape: TimelineShape,
    label: String,
}

impl TimelineItem for Item {
    fn id(&self) -> u64 {
        self.id
    }

    fn parent(&self) -> Option<u64> {
        self.parent
    }

    fn shape(&self) -> TimelineShape {
        self.shape
    }

    fn label(&self) -> &str {
        &self.label
    }
}

fn items(transcript: &LensTranscript<Image>) -> Vec<Item> {
    let model = &transcript.model;
    model
        .entries()
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let (shape, parent, label) = match entry {
                AgentThreadEntry::User { .. } => (TimelineShape::User, None, String::new()),
                AgentThreadEntry::Assistant { aside: None, .. } => {
                    (TimelineShape::Assistant, None, String::new())
                }
                AgentThreadEntry::Assistant {
                    aside: Some(aside), ..
                } => (TimelineShape::Notice, aside.reply_to, String::new()),
                AgentThreadEntry::Reasoning { .. } => {
                    (TimelineShape::Reasoning, None, String::new())
                }
                AgentThreadEntry::Plan { .. } => (TimelineShape::Plan, None, String::new()),
                AgentThreadEntry::Tool {
                    id,
                    kind,
                    status,
                    label,
                    ..
                } => (
                    TimelineShape::Tool((*kind).into(), (*status).into()),
                    model
                        .tool_parent(*id)
                        .and_then(|parent| model.tool_entry(parent)),
                    label.clone(),
                ),
            };
            Item {
                index,
                id: entry.id(),
                parent,
                shape,
                label,
            }
        })
        .collect()
}

fn markdown_html(source: &str) -> String {
    markdown::to_html_with_options(source, &markdown::Options::gfm())
        .unwrap_or_else(|_| format!("<p>{}</p>", escape(source)))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn tool_output_text(output: &[ToolPayload]) -> String {
    let mut text = String::new();
    for payload in output {
        if let ToolPayload::Text(part) | ToolPayload::Terminal(part) = payload {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(part);
        }
    }
    text
}

fn tool_opens_output(kind: AgentToolKind, status: AgentToolStatus, output: &[ToolPayload]) -> bool {
    kind == AgentToolKind::Execute
        && !status.running()
        && output.iter().any(|payload| match payload {
            ToolPayload::Text(text) | ToolPayload::Terminal(text) => !text.trim().is_empty(),
            ToolPayload::Json(_) | ToolPayload::Diff { .. } => false,
        })
}

fn kind_name(kind: AgentToolKind) -> &'static str {
    match kind {
        AgentToolKind::Read => "read",
        AgentToolKind::Search => "search",
        AgentToolKind::Edit => "edit",
        AgentToolKind::Execute => "execute",
        AgentToolKind::Fetch => "fetch",
        AgentToolKind::Think => "think",
        AgentToolKind::Other => "other",
    }
}

fn utf16_to_byte(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (index, character) in text.char_indices() {
        if units >= offset {
            return index;
        }
        units += character.len_utf16();
    }
    text.len()
}

fn byte_to_utf16(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].encode_utf16().count()
}

fn provider_id(provider: AgentProvider) -> &'static str {
    match provider {
        AgentProvider::ClaudeCode => "claude",
        AgentProvider::Codex => "codex",
    }
}

#[derive(Default)]
pub struct Agents {
    connected: bool,
    lenses: BTreeSet<PaneId>,
    cursors: HashMap<PaneId, AgentCursor>,
    journals: HashMap<PaneId, Vec<(u64, Vec<u8>)>>,
    views: HashMap<PaneId, Lens>,
    replays: Vec<(PaneId, u64)>,
    status: Option<String>,
    providers_requested: bool,
    flips: AgentFlips,
}

impl Agents {
    pub fn updates(&mut self, pane: PaneId, first_seq: u64, items: Vec<Vec<u8>>) {
        if !self.lenses.contains(&pane) {
            return;
        }
        let cursor = self.cursors.entry(pane).or_default();
        let journal = self.journals.entry(pane).or_default();
        let (replay, invalid) = cursor.apply(journal, first_seq, &items);
        let pane_trimmed = cursor.trim(journal, MAX_AGENT_HISTORY_BYTES, MAX_AGENT_HISTORY_ITEMS);
        let history_trimmed = self.trim_history();
        if pane_trimmed || history_trimmed {
            self.status = Some("Showing recent agent history to limit memory use.".to_owned());
        }
        if invalid {
            self.status = Some("Some agent updates could not be read.".to_owned());
        }
        if let Some(from_seq) = replay {
            self.replays.push((pane, from_seq));
        }
    }

    pub fn lagged(&mut self, pane: PaneId, _next_seq: u64) {
        if self.lenses.contains(&pane) {
            self.request_replay(pane);
        }
    }

    fn request_replay(&mut self, pane: PaneId) {
        let from_seq = self.cursors.entry(pane).or_default().replay_from();
        self.replays.push((pane, from_seq));
    }

    fn forget_stream(&mut self, pane: PaneId) {
        self.journals.remove(&pane);
        self.cursors.remove(&pane);
    }

    fn trim_history(&mut self) -> bool {
        let mut bytes: usize = self.cursors.values().map(|cursor| cursor.bytes).sum();
        let mut trimmed = false;
        while bytes > MAX_AGENT_HISTORY_BYTES {
            let Some((&pane, _)) = self.cursors.iter().max_by_key(|(_, cursor)| cursor.bytes)
            else {
                break;
            };
            let (Some(cursor), Some(journal)) =
                (self.cursors.get_mut(&pane), self.journals.get_mut(&pane))
            else {
                break;
            };
            let old_bytes = cursor.bytes;
            let limit = old_bytes.saturating_sub(bytes - MAX_AGENT_HISTORY_BYTES);
            trimmed |= cursor.trim(journal, limit, MAX_AGENT_HISTORY_ITEMS);
            if cursor.bytes == old_bytes {
                break;
            }
            bytes -= old_bytes - cursor.bytes;
        }
        trimmed
    }
}

#[allow(clippy::struct_excessive_bools)]
struct Context {
    connected: bool,
    writable: bool,
    state: AgentPaneWire,
    provider: Option<AgentProvider>,
    verbs: bool,
    watch: bool,
    commands: Arc<[AgentCommand]>,
}

impl Client {
    pub(crate) fn observe_flips(&mut self) {
        let snapshot = self.core.snapshot();
        self.agents.flips.observe(snapshot);
    }

    pub(crate) fn flip_reply(&mut self, response: &CommandResponse) -> Option<Value> {
        let (request, result) = match response {
            CommandResponse::Success { request_id, .. } => (*request_id, Ok(())),
            CommandResponse::Error {
                request_id, error, ..
            } => (*request_id, Err(error.to_string())),
        };
        if !self.agents.flips.owns(request) {
            return None;
        }
        let error = self
            .agents
            .flips
            .settle(request, result.as_ref().copied().map_err(String::as_str));
        Some(json!({
            "type": "flip",
            "error": error,
        }))
    }

    fn pane_snapshot(&self, pane: PaneId) -> Option<&PaneSnapshot> {
        self.core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&pane))
    }

    fn writable(&self) -> bool {
        self.agents.connected && !self.core.attached_read_only()
    }

    fn pump_agents(&mut self) {
        for (pane, from_seq) in std::mem::take(&mut self.agents.replays) {
            self.send(&ProtocolMessage::AgentReplay { pane, from_seq });
        }
    }

    fn lens_context(&self, pane: PaneId) -> Context {
        let cursor = self.agents.cursors.get(&pane);
        let verbs = cursor.is_some_and(|cursor| cursor.verbs_supported);
        Context {
            connected: self.agents.connected,
            writable: self.writable(),
            state: self.core.agent_state(pane).cloned().unwrap_or_default(),
            provider: self.pane_snapshot(pane).and_then(pane_agent),
            verbs,
            watch: cursor.is_some_and(|cursor| cursor.watch),
            commands: pane_commands(
                &cursor.map_or_else(|| Arc::from([]), |cursor| cursor.commands.clone()),
                verbs,
            )
            .into(),
        }
    }

    fn synchronize_lens(&mut self, pane: PaneId) {
        let client = zz_protocol::ClientInstanceId(self.instance);
        let journal = self
            .agents
            .journals
            .get(&pane)
            .map_or(&[][..], Vec::as_slice);
        let Some(lens) = self.agents.views.get_mut(&pane) else {
            return;
        };
        let acknowledgements = lens.synchronize(journal, client);
        for reclaim_id in acknowledgements {
            self.send(&ProtocolMessage::AgentAcknowledgePromptRestore { pane, reclaim_id });
        }
    }

    fn pending(&self, pane: PaneId) -> Vec<PendingPermission> {
        let Some(lens) = self.agents.views.get(&pane) else {
            return Vec::new();
        };
        pending_permissions(
            lens.transcript.model.permissions(),
            self.core
                .agent_state(pane)
                .and_then(|state| state.pending_permission.as_ref()),
        )
    }

    fn can_answer(&self, pane: PaneId, request_id: u64) -> bool {
        self.writable()
            && self
                .agents
                .views
                .get(&pane)
                .is_some_and(|lens| !lens.answered.contains(&request_id))
            && self
                .pending(pane)
                .iter()
                .any(|request| request.request_id == request_id)
    }

    fn respond_permission(
        &mut self,
        pane: PaneId,
        request_id: u64,
        option_id: Option<String>,
    ) -> bool {
        if !self.can_answer(pane, request_id) {
            return false;
        }
        if let Some(lens) = self.agents.views.get_mut(&pane) {
            lens.answered.insert(request_id);
        }
        self.send(&ProtocolMessage::AgentRespondPermission {
            pane,
            request_id,
            option_id,
        });
        true
    }

    fn answer_question(
        &mut self,
        pane: PaneId,
        request_id: u64,
        answers: Vec<AgentQuestionAnswer>,
    ) -> bool {
        if !self.can_answer(pane, request_id) {
            return false;
        }
        if let Some(lens) = self.agents.views.get_mut(&pane) {
            lens.answered.insert(request_id);
        }
        self.send(&ProtocolMessage::AgentAnswerQuestion {
            pane,
            request_id,
            answers,
        });
        true
    }

    fn apply_question_step(&mut self, pane: PaneId, step: QuestionCardStep) -> bool {
        let Some(request_id) = self
            .agents
            .views
            .get(&pane)
            .and_then(|lens| lens.question.as_ref())
            .map(QuestionCard::request_id)
        else {
            return false;
        };
        match step {
            QuestionCardStep::Stay => false,
            QuestionCardStep::Handled | QuestionCardStep::Other(_) => true,
            QuestionCardStep::Submit(answers) => {
                self.answer_question(pane, request_id, answers);
                true
            }
            QuestionCardStep::Dismiss => {
                self.respond_permission(pane, request_id, None);
                true
            }
        }
    }

    fn send_prompt(&mut self, pane: PaneId, text: &str) -> bool {
        let allowed = self.writable() && self.core.agent_state(pane).is_some_and(accepts_prompt);
        if !allowed || text.trim().is_empty() {
            return false;
        }
        self.send(&ProtocolMessage::AgentPrompt {
            pane,
            text: text.to_owned(),
            images: Vec::new(),
        });
        let command = self
            .agents
            .cursors
            .get(&pane)
            .is_some_and(|cursor| cursor.verbs_supported)
            && is_zz_command(text);
        let queueing = self
            .core
            .agent_state(pane)
            .is_some_and(|state| turn_running(&state.phase));
        if !queueing
            && !command
            && let Some(lens) = self.agents.views.get_mut(&pane)
        {
            lens.transcript
                .model
                .begin_prompt(text.to_owned(), Vec::new());
        }
        true
    }

    fn head_permission(&mut self, pane: PaneId) -> Option<PendingPermission> {
        let permissions = self.pending(pane);
        let lens = self.agents.views.get_mut(&pane)?;
        lens.answered
            .retain(|id| permissions.iter().any(|request| request.request_id == *id));
        let head = permissions
            .into_iter()
            .find(|request| !lens.answered.contains(&request.request_id));
        let head_id = head.as_ref().map(|request| request.request_id);
        if lens.permission_request != head_id {
            lens.permission_request = head_id;
            lens.permission_selected = 0;
        }
        let question = head
            .as_ref()
            .filter(|request| !request.questions.is_empty());
        if lens.question.as_ref().map(QuestionCard::request_id)
            != question.map(|request| request.request_id)
        {
            lens.question = question
                .map(|request| QuestionCard::new(request.request_id, request.questions.clone()));
        }
        head
    }

    #[allow(clippy::too_many_lines)]
    fn lens_view(&mut self, pane: PaneId, value: &str, cursor: usize, reply: &str) -> Value {
        self.synchronize_lens(pane);
        let head = self.head_permission(pane);
        let context = self.lens_context(pane);
        let pending_count = self
            .pending(pane)
            .iter()
            .filter(|request| {
                self.agents
                    .views
                    .get(&pane)
                    .is_none_or(|lens| !lens.answered.contains(&request.request_id))
            })
            .count();
        let cwd = self
            .pane_snapshot(pane)
            .map(|snapshot| snapshot.summary.cwd.clone())
            .unwrap_or_default();
        let Some(lens) = self.agents.views.get_mut(&pane) else {
            return Value::Null;
        };
        let state = &context.state;
        let running = turn_running(&state.phase);
        let ready = context.writable
            && matches!(
                state.phase,
                AgentConnectionPhase::Ready | AgentConnectionPhase::Running
            );
        let byte_cursor = utf16_to_byte(value, cursor);
        lens.synchronize_completions(value, byte_cursor, context.commands.clone());
        let entries = items(&lens.transcript);
        let folded = fold_timeline_rows(&entries);
        let rows = folded.rows;
        let streaming = running
            .then(|| {
                lens.transcript
                    .model
                    .entries()
                    .last()
                    .map(AgentThreadEntry::id)
            })
            .flatten();
        let live_row = running
            .then(|| rows.len().checked_sub(1))
            .flatten()
            .filter(|last| is_turn_row(&rows[*last]));
        lens.clocks.tick(live_row.map(|row| rows[row].id()));
        let copyable = if running {
            None
        } else {
            final_assistant_entry_id(&rows)
        };
        let rewind = context.verbs.then_some(
            context.writable
                && !running
                && state.phase == AgentConnectionPhase::Ready
                && state.pending_permission.is_none(),
        );
        let row_values = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                lens_row(
                    lens,
                    row,
                    index > 0,
                    live_row == Some(index),
                    copyable,
                    rewind,
                    streaming,
                )
            })
            .collect::<Vec<_>>();
        let has_content = !value.trim().is_empty();
        let action = composer_action(running, has_content);
        let action_enabled = match action {
            ComposerAction::Stop | ComposerAction::Queue => context.writable,
            ComposerAction::Send => ready && state.pending_permission.is_none() && has_content,
        };
        let plan = lens
            .transcript
            .model
            .entries()
            .iter()
            .rev()
            .find_map(|entry| match entry {
                AgentThreadEntry::Plan { markdown, .. } => Some(markdown.clone()),
                _ => None,
            })
            .filter(|source| {
                let (done, total, _) = plan_progress(source);
                total > 0 && (done < total || running)
            });
        let tray = plan.as_deref().and_then(|source| {
            let (label, current) = plan_chip_label(source)?;
            Some(json!({
                "plan": {
                    "label": label,
                    "current": current,
                    "open": lens.plan_open,
                    "items": plan_items(source).into_iter().map(|(state, text)| json!({
                        "state": state,
                        "text": text,
                    })).collect::<Vec<_>>(),
                },
            }))
        });
        let ask = head.map(|head| {
            let counter = (pending_count > 1).then(|| format!("1 of {pending_count}"));
            if let Some(card) = lens
                .question
                .as_ref()
                .filter(|card| card.request_id() == head.request_id)
            {
                question_ask(card, counter, context.writable)
            } else {
                let cwd = zz_client::providers::home_relative(&cwd);
                approval_ask(
                    &head,
                    approval_topic(
                        Some(&lens.transcript.chat),
                        head.request_id,
                        &head.title,
                        Some(&cwd),
                    ),
                    counter,
                    lens.permission_selected,
                    !reply.trim().is_empty(),
                    context.writable,
                )
            }
        });
        let error = state
            .error
            .as_deref()
            .or(match &state.phase {
                AgentConnectionPhase::Failed { message } => Some(message.as_str()),
                _ => None,
            })
            .map(|error| {
                json!({
                    "text": rendered_error(error),
                    "stopped": matches!(state.phase, AgentConnectionPhase::Failed { .. }),
                })
            });
        let empty = rows.is_empty().then(|| {
            match agent_empty(context.connected, &state.phase, context.provider) {
                AgentEmpty::Welcome(provider) => json!({
                    "welcome": {
                        "title": agent_welcome_title(provider),
                        "detail": AGENT_WELCOME_DETAIL,
                        "provider": provider_id(provider),
                    }
                }),
                AgentEmpty::Message { text, busy } => json!({ "text": text, "busy": busy }),
            }
        });
        let completions = lens
            .completions
            .iter()
            .map(|completion| {
                json!({
                    "name": bare_command_name(&completion.command.name),
                    "description": meaningful_command_description(&completion.command.description),
                })
            })
            .collect::<Vec<_>>();
        json!({
            "pane": pane.0,
            "provider": context.provider.map(provider_id),
            "placeholder": agent_placeholder(context.provider),
            "cwd": cwd,
            "connected": context.connected,
            "writable": context.writable,
            "running": running,
            "watchOnly": context.watch,
            "rows": row_values,
            "empty": empty,
            "ask": ask,
            "queued": (state.queued_prompts > 0).then_some(state.queued_prompts),
            "tray": tray,
            "error": error,
            "completions": completions,
            "completionSelected": lens.completion_selected,
            "hint": active_command_hint(value, &lens.commands),
            "action": action,
            "actionEnabled": action_enabled,
            "usage": lens.usage.map(|(used, size)| json!({
                "fraction": context_usage_fraction(used, size),
                "tooltip": context_usage_tooltip(used, size),
            })),
            "git": state.git.as_ref().map(|git| json!({
                "branch": git.branch,
                "files": git_file_count_label(git.changed_files),
                "additions": format!("+{}", git.additions),
                "deletions": format!("-{}", git.deletions),
                "tooltip": git_summary_tooltip(git.branch.as_deref(), git.changed_files, git.additions, git.deletions),
            })),
            "drafts": std::mem::take(&mut lens.drafts),
        })
    }
}

fn kind_icon(kind: Option<ItemKind>) -> &'static str {
    match kind {
        Some(ItemKind::Execute) => "square-terminal",
        Some(ItemKind::Edit) => "pencil",
        Some(ItemKind::Read) => "file",
        Some(ItemKind::Search) => "search",
        Some(ItemKind::Fetch | ItemKind::Web) => "globe",
        _ => "info",
    }
}

fn key_label(index: usize) -> Option<String> {
    (index < KEYED_OPTIONS).then(|| (index + 1).to_string())
}

fn question_ask(card: &QuestionCard, counter: Option<String>, writable: bool) -> Value {
    let index = card.focused();
    let count = card.questions().len();
    let Some(question) = card.questions().get(index) else {
        return Value::Null;
    };
    let key = question_other_key(card).map(|key| key.to_string());
    let leading =
        if question.multi_select && card.other_selected(index) && !question.options.is_empty() {
            json!({ "key": key, "lit": true })
        } else {
            key.map_or(Value::Null, |key| json!({ "key": key }))
        };
    json!({
        "kind": "question",
        "requestId": card.request_id(),
        "index": index,
        "counter": (count == 1).then_some(counter).flatten(),
        "pager": (count > 1).then(|| json!({ "index": index, "count": count })),
        "question": question.question,
        "problem": card.problem(),
        "options": question.options.iter().enumerate().map(|(option, choice)| json!({
            "label": choice.label,
            "detail": choice.description,
            "under": true,
            "key": key_label(option),
            "highlighted": option == card.highlight(),
            "picked": question.multi_select.then(|| card.is_picked(index, option)),
        })).collect::<Vec<_>>(),
        "reply": question.allow_other.then(|| json!({
            "placeholder": if question.options.is_empty() { "Type your answer" } else { "Something else" },
            "secret": question.secret,
            "value": card.other_text(index),
            "key": format!("{}-{index}", card.request_id()),
        })),
        "leading": leading,
        "send": question_send(card),
        "skip": true,
        "hints": question_hints(card),
        "writable": writable,
    })
}

fn approval_ask(
    head: &PendingPermission,
    topic: ApprovalTopic,
    counter: Option<String>,
    selected: usize,
    replying: bool,
    writable: bool,
) -> Value {
    let plan = topic.plan();
    let reject = head
        .options
        .iter()
        .position(|option| option.allow == Some(false));
    let denying = head
        .options
        .get(selected)
        .is_some_and(|option| option.allow == Some(false));
    let send = approval_send(replying, denying, plan);
    let count = head.options.len();
    let (icon, title, inset, note, markdown) = match &topic {
        ApprovalTopic::Plan { markdown } => {
            ("list", "Plan", None, None, Some(markdown_html(markdown)))
        }
        ApprovalTopic::Tool {
            kind,
            heading,
            inset,
            note,
        } => (
            kind_icon(*kind),
            *heading,
            inset.clone(),
            note.clone(),
            None,
        ),
    };
    json!({
        "kind": if plan { "plan" } else { "approval" },
        "requestId": head.request_id,
        "icon": icon,
        "title": title,
        "counter": counter,
        "inset": inset,
        "note": note,
        "plan": markdown,
        "options": head.options.iter().enumerate().map(|(index, option)| json!({
            "id": option.id,
            "label": option.name,
            "key": key_label(index),
            "highlighted": index == selected,
            "divider": Some(index) == reject && index > 0,
        })).collect::<Vec<_>>(),
        "reply": reject.map(|_| json!({
            "placeholder": topic.placeholder(),
            "secret": false,
            "key": format!("{}", head.request_id),
        })),
        "leading": (reject.is_some() && count < KEYED_OPTIONS)
            .then(|| json!({ "key": (count + 1).to_string() })),
        "send": send,
        "skip": false,
        "hints": approval_hints(count, replying, plan, send),
        "writable": writable,
    })
}

fn tool_json(entry: &AgentThreadEntry<Image>, item: &Item, substeps: usize, live: bool) -> Value {
    let AgentThreadEntry::Tool {
        output, exit_code, ..
    } = entry
    else {
        return Value::Null;
    };
    let TimelineShape::Tool(kind, status) = item.shape else {
        return Value::Null;
    };
    let spin = status.running() && live;
    json!({
        "kind": "tool",
        "id": item.id,
        "label": single_line(&item.label),
        "tool": kind_name(kind),
        "agent": substeps > 0 && !spin,
        "spin": spin,
        "tint": match status {
            AgentToolStatus::NeedsApproval => Some("warning"),
            AgentToolStatus::Failed => Some("danger"),
            _ => None,
        },
        "tag": tool_tag(status, *exit_code).map(|(tag, danger)| json!({ "text": tag, "danger": danger })),
        "opens": tool_opens_output(kind, status, output),
    })
}

fn lens_row(
    lens: &mut Lens,
    row: &TimelineRow<Item>,
    later: bool,
    live: bool,
    copyable: Option<u64>,
    rewind: Option<bool>,
    streaming: Option<u64>,
) -> Value {
    let members = row.members().to_vec();
    match row {
        TimelineRow::Single(item) if item.shape == TimelineShape::User => {
            user_json(lens, item, rewind, streaming, later)
        }
        TimelineRow::Single(item) if item.shape == TimelineShape::Notice => {
            let html = entry_html(lens, item, streaming);
            json!({ "kind": "notice", "id": item.id, "key": format!("n{}", item.id), "html": html })
        }
        TimelineRow::Group {
            kind: TimelineGroupKind::Reply,
            ..
        } => {
            let Some((first, replies)) = members.split_first() else {
                return Value::Null;
            };
            let user = user_json(lens, first, rewind, streaming, later);
            let replies = replies
                .iter()
                .filter(|item| matches!(item.shape, TimelineShape::Assistant | TimelineShape::Notice))
                .map(|item| {
                    let side = matches!(
                        lens.transcript.model.entries().get(item.index),
                        Some(AgentThreadEntry::Assistant { aside: Some(aside), .. }) if aside.side
                    );
                    json!({ "id": item.id, "html": entry_html(lens, item, streaming), "side": side })
                })
                .collect::<Vec<_>>();
            json!({ "kind": "reply", "id": row.id(), "key": format!("r{}", row.id()), "later": later, "user": user, "replies": replies })
        }
        _ => turn_json(lens, row.id(), &members, live, copyable, streaming),
    }
}

fn entry_text(lens: &Lens, item: &Item) -> String {
    match lens.transcript.model.entries().get(item.index) {
        Some(
            AgentThreadEntry::User { markdown, .. }
            | AgentThreadEntry::Assistant { markdown, .. }
            | AgentThreadEntry::Reasoning { markdown, .. }
            | AgentThreadEntry::Plan { markdown, .. },
        ) => markdown.clone(),
        _ => String::new(),
    }
}

fn entry_html(lens: &mut Lens, item: &Item, streaming: Option<u64>) -> String {
    let text = entry_text(lens, item);
    lens.html(item.id, &text, streaming == Some(item.id))
}

fn user_json(
    lens: &mut Lens,
    item: &Item,
    rewind: Option<bool>,
    streaming: Option<u64>,
    later: bool,
) -> Value {
    let (images, rewind_id) = match lens.transcript.model.entries().get(item.index) {
        Some(entry @ AgentThreadEntry::User { images, .. }) => {
            (images.clone(), entry.rewind_id().map(str::to_owned))
        }
        _ => (Vec::new(), None),
    };
    let text = entry_text(lens, item);
    let html =
        (!text.trim().is_empty()).then(|| lens.html(item.id, &text, streaming == Some(item.id)));
    json!({
        "kind": "user",
        "id": item.id,
        "key": format!("u{}", item.id),
        "later": later,
        "html": html,
        "images": images,
        "rewind": rewind.and_then(|enabled| rewind_id.map(|id| json!({ "id": id, "enabled": enabled }))),
    })
}

fn turn_json(
    lens: &mut Lens,
    id: u64,
    members: &[Item],
    live: bool,
    copyable: Option<u64>,
    streaming: Option<u64>,
) -> Value {
    let nesting = StepNesting::new(members);
    let (steps, answer) = split_turn(members, &nesting);
    let answering = !answer.is_empty();
    let expanded = lens.expanded.get(&id).copied().unwrap_or(false);
    let trace = (!steps.is_empty()).then(|| {
        let header = trace_header(
            members,
            &nesting,
            &steps,
            answering,
            live,
            lens.clocks.elapsed(id),
        );
        let (state, detail, tool, counts) = match &header {
            TraceHeader::Waiting { detail } => ("waiting", Some(detail.clone()), false, Vec::new()),
            TraceHeader::Live { tool, .. } => ("live", None, *tool, Vec::new()),
            TraceHeader::Done { counts, .. } => ("done", None, false, counts.clone()),
        };
        let thought = (live && !expanded && !answering)
            .then(|| {
                steps.iter().rev().find_map(|index| {
                    let item = &members[*index];
                    matches!(item.shape, TimelineShape::Reasoning | TimelineShape::Assistant)
                        .then(|| thought_tail(&entry_text(lens, item)))
                        .flatten()
                })
            })
            .flatten();
        let failure = (!live && !expanded)
            .then(|| ending_failure(members, &steps))
            .flatten()
            .and_then(|index| {
                let item = &members[index];
                let entry = lens.transcript.model.entries().get(item.index)?;
                let AgentThreadEntry::Tool { output, .. } = entry else {
                    return None;
                };
                let text = tool_output_text(output);
                let mut tail = text
                    .lines()
                    .rev()
                    .map(str::trim_end)
                    .filter(|line| !line.trim().is_empty())
                    .take(2)
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                tail.reverse();
                Some(json!({ "tool": tool_json(entry, item, 0, false), "tail": tail }))
            });
        let rendered_steps = if expanded {
                steps
                    .iter()
                    .map(|index| {
                        let item = &members[*index];
                        match item.shape {
                            TimelineShape::Reasoning => {
                                let text = entry_text(lens, item);
                                let empty = text.trim().is_empty();
                                json!({
                                    "kind": "thought",
                                    "id": item.id,
                                    "html": (!empty).then(|| lens.html(item.id, &text, streaming == Some(item.id))),
                                })
                            }
                            TimelineShape::Assistant | TimelineShape::Notice => json!({
                                "kind": "note",
                                "id": item.id,
                                "html": entry_html(lens, item, streaming),
                            }),
                            TimelineShape::Tool(..) => lens
                                .transcript
                                .model
                                .entries()
                                .get(item.index)
                                .map_or(Value::Null, |entry| {
                                    tool_json(entry, item, nesting.substeps(item.id), live)
                                }),
                            TimelineShape::User | TimelineShape::Plan => Value::Null,
                        }
                    })
                    .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        json!({
            "state": state,
            "label": header.label(),
            "detail": detail,
            "tool": tool,
            "counts": counts.iter().map(|count| json!({
                "slot": count.slot,
                "count": count.count,
                "failed": count.failed,
                "describe": count.describe(),
                "noun": TRACE_COUNT_NOUNS[count.slot].0,
            })).collect::<Vec<_>>(),
            "thought": thought,
            "failure": failure,
            "expanded": expanded,
            "steps": rendered_steps,
        })
    });
    let answers = answer
        .iter()
        .filter_map(|index| {
            let item = &members[*index];
            (item.shape == TimelineShape::Assistant).then(|| {
                let text = entry_text(lens, item);
                json!({
                    "id": item.id,
                    "html": lens.html(item.id, &text, streaming == Some(item.id)),
                    "copy": (copyable == Some(item.id)).then_some(text),
                })
            })
        })
        .collect::<Vec<_>>();
    json!({
        "kind": "turn",
        "id": id,
        "key": format!("t{id}"),
        "live": live,
        "trace": trace,
        "answers": answers,
    })
}

#[wasm_bindgen]
impl Client {
    pub fn agent_connection(&mut self, connected: bool) {
        self.agents.connected = connected;
        self.agents.providers_requested = false;
        self.agents.cursors.clear();
        self.agents.journals.clear();
        for lens in self.agents.views.values_mut() {
            lens.reconnect();
        }
        if connected {
            self.agent_prune();
            let watched = self.agents.lenses.iter().copied().collect::<Vec<_>>();
            for pane in watched {
                self.agents.request_replay(pane);
            }
            self.pump_agents();
        }
    }

    pub fn agent_prune(&mut self) {
        let snapshot = self.core.snapshot();
        self.agents.flips.observe(snapshot);
        let stale = self
            .agents
            .lenses
            .iter()
            .copied()
            .filter(|pane| !self.agent_is_chat(pane.0 as f64))
            .collect::<Vec<_>>();
        for pane in stale {
            self.agents.lenses.remove(&pane);
            self.agents.views.remove(&pane);
            self.agents.forget_stream(pane);
        }
    }

    pub fn agent_forget(&mut self, pane: f64) {
        let pane = pane_id(pane);
        self.agents.lenses.remove(&pane);
        self.agents.views.remove(&pane);
        self.agents.forget_stream(pane);
    }

    pub fn agent_pump(&mut self) {
        self.pump_agents();
    }

    pub fn agent_status(&mut self) -> Option<String> {
        self.agents.status.take()
    }

    pub fn agent_provider(&self, pane: f64) -> Option<String> {
        self.pane_snapshot(pane_id(pane))
            .and_then(pane_agent)
            .map(|provider| provider_id(provider).to_owned())
    }

    pub fn agent_is_chat(&self, pane: f64) -> bool {
        self.pane_snapshot(pane_id(pane))
            .is_some_and(|pane| matches!(pane.kind, PaneKindSnapshot::Agent(_)))
    }

    pub fn agent_flip_state(&self, pane: f64) -> String {
        let pane = pane_id(pane);
        let Some(target) = self.pane_snapshot(pane).and_then(flip_target) else {
            return Value::Null.to_string();
        };
        let flip = self.agents.flips.state(pane);
        json!({
            "to": match target.to {
                FlipTo::Chat => "chat",
                FlipTo::Cli => "cli",
            },
            "icon": match target.to {
                FlipTo::Chat => "chat",
                FlipTo::Cli => "square-terminal",
            },
            "title": zz_client::agent_flip::flip_tooltip(target.to, flip.armed()),
            "armed": flip.armed(),
            "armedLabel": zz_client::agent_flip::armed_label(target.to),
            "pending": flip.pending(),
            "busy": flip.busy().map(|reason| json!({
                "title": zz_client::agent_flip::BUSY_TITLE,
                "detail": zz_client::agent_flip::busy_detail(reason),
            })),
        })
        .to_string()
    }

    pub fn agent_flip(&mut self, pane: f64) -> bool {
        let pane = pane_id(pane);
        let Some(target) = self.pane_snapshot(pane).and_then(flip_target) else {
            return false;
        };
        if !self.writable() {
            return false;
        }
        let mut flips = std::mem::take(&mut self.agents.flips);
        let sent = flips.press(pane, target.to, |command| Some(self.invoke(command)));
        self.agents.flips = flips;
        sent
    }

    pub fn agent_flip_choose(&mut self, pane: f64, choice: &str) -> bool {
        let pane = pane_id(pane);
        let mode = match choice {
            "now" => FlipMode::Now,
            "when-idle" => FlipMode::WhenIdle,
            "cancel" => FlipMode::Cancel,
            _ => {
                self.agents.flips.dismiss(pane);
                return true;
            }
        };
        if !self.writable() {
            return false;
        }
        let mut flips = std::mem::take(&mut self.agents.flips);
        let sent = flips.choose(pane, mode, |command| Some(self.invoke(command)));
        self.agents.flips = flips;
        sent
    }

    pub fn agent_open(&mut self, pane: f64) -> bool {
        let pane = pane_id(pane);
        if !self.agent_is_chat(pane.0 as f64) {
            return false;
        }
        if !self.agents.lenses.insert(pane) {
            return true;
        }
        self.agents.views.insert(pane, Lens::new());
        self.agents.forget_stream(pane);
        self.agents.request_replay(pane);
        self.pump_agents();
        true
    }

    pub fn agent_close(&mut self, pane: f64) {
        let pane = pane_id(pane);
        if self.agents.lenses.remove(&pane) {
            self.agents.views.remove(&pane);
            self.agents.forget_stream(pane);
        }
    }

    pub fn agent_view(&mut self, pane: f64, value: &str, cursor: f64, reply: &str) -> String {
        let pane = pane_id(pane);
        let view = self.lens_view(pane, value, cursor as usize, reply);
        self.pump_agents();
        view.to_string()
    }

    pub fn agent_submit(&mut self, pane: f64, text: &str) -> bool {
        let pane = pane_id(pane);
        if text.trim().is_empty() {
            let head = self.head_permission(pane);
            if self
                .agents
                .views
                .get(&pane)
                .is_some_and(|lens| lens.question.is_some())
            {
                let step = self
                    .agents
                    .views
                    .get_mut(&pane)
                    .and_then(|lens| lens.question.as_mut().map(QuestionCard::enter));
                if let Some(step) = step {
                    self.apply_question_step(pane, step);
                }
                return false;
            }
            if let Some((request_id, option)) = head.and_then(|permission| {
                let selected = self.agents.views.get(&pane)?.permission_selected;
                Some((
                    permission.request_id,
                    permission.options.get(selected)?.id.clone(),
                ))
            }) {
                self.respond_permission(pane, request_id, Some(option));
            }
            return false;
        }
        if let Some(lens) = self.agents.views.get_mut(&pane) {
            lens.dismiss_completions();
        }
        self.send_prompt(pane, text)
    }

    pub fn agent_approve(&mut self, pane: f64, text: &str) -> bool {
        let pane = pane_id(pane);
        let Some(head) = self.head_permission(pane) else {
            return false;
        };
        let text = text.trim();
        if text.is_empty() {
            let selected = self
                .agents
                .views
                .get(&pane)
                .map_or(0, |lens| lens.permission_selected);
            if let Some(option) = head.options.get(selected) {
                self.respond_permission(pane, head.request_id, Some(option.id.clone()));
            }
            return false;
        }
        let Some(reject) = head
            .options
            .iter()
            .find(|option| option.allow == Some(false))
        else {
            return false;
        };
        self.respond_permission(pane, head.request_id, Some(reject.id.clone()));
        self.send_prompt(pane, text)
    }

    pub fn agent_open_plan(&mut self, pane: f64, request_id: f64) {
        let pane = pane_id(pane);
        let Some(Ask::Plan { markdown, .. }) = self
            .agents
            .views
            .get(&pane)
            .and_then(|lens| lens.transcript.chat.request(RequestId(request_id as u64)))
            .map(|request| request.ask.clone())
        else {
            return;
        };
        if self.writable() {
            self.invoke(CommandInvocation::new(
                "view-pane",
                [
                    "-t".to_owned(),
                    pane.to_string(),
                    "-T".to_owned(),
                    "Plan".to_owned(),
                    "-k".to_owned(),
                    "markdown".to_owned(),
                    "--".to_owned(),
                    markdown,
                ],
            ));
        }
    }

    pub fn agent_cancel(&mut self, pane: f64) {
        self.send(&ProtocolMessage::AgentCancel {
            pane: pane_id(pane),
        });
    }

    pub fn agent_unqueue(&mut self, pane: f64) {
        if self.writable() {
            self.send(&ProtocolMessage::AgentUnqueue {
                pane: pane_id(pane),
            });
        }
    }

    pub fn agent_respond(&mut self, pane: f64, request_id: f64, option: Option<String>) -> bool {
        self.respond_permission(pane_id(pane), request_id as u64, option)
    }

    pub fn agent_rewind(&mut self, pane: f64, message_id: &str) -> bool {
        self.send_prompt(pane_id(pane), &rewind_command(message_id))
    }

    pub fn agent_toggle(&mut self, pane: f64, id: f64) {
        if let Some(lens) = self.agents.views.get_mut(&pane_id(pane)) {
            let expanded = lens.expanded.entry(id as u64).or_insert(false);
            *expanded = !*expanded;
        }
    }

    pub fn agent_plan(&mut self, pane: f64) {
        if let Some(lens) = self.agents.views.get_mut(&pane_id(pane)) {
            lens.plan_open = !lens.plan_open;
        }
    }

    pub fn agent_open_output(&mut self, pane: f64, id: f64) {
        let pane = pane_id(pane);
        let cwd = self
            .pane_snapshot(pane)
            .map(|snapshot| snapshot.summary.cwd.clone())
            .unwrap_or_default();
        let Some(args) = self.agents.views.get(&pane).and_then(|lens| {
            lens.transcript
                .model
                .entries()
                .iter()
                .find_map(|entry| match entry {
                    AgentThreadEntry::Tool {
                        id: entry_id,
                        label,
                        exit_code,
                        output,
                        ..
                    } if *entry_id == id as u64 => zz_client::agent_output::output_pane_args(
                        pane,
                        std::path::Path::new(&cwd),
                        label,
                        *exit_code,
                        &tool_output_text(output),
                    ),
                    _ => None,
                })
        }) else {
            return;
        };
        if self.writable() {
            self.invoke(CommandInvocation::new("split-window", args));
        }
    }

    pub fn agent_question(&mut self, pane: f64, action: &str) -> bool {
        let pane = pane_id(pane);
        let Ok(action) = serde_json::from_str::<Value>(action) else {
            return false;
        };
        let Some(card) = self
            .agents
            .views
            .get_mut(&pane)
            .and_then(|lens| lens.question.as_mut())
        else {
            return false;
        };
        let index = |key: &str, at: usize| {
            action
                .get(key)
                .and_then(|value| value.get(at).or(Some(value)))
                .and_then(Value::as_u64)
                .map(|value| value as usize)
        };
        let step = if let (Some(question), Some(option)) = (index("pick", 0), index("pick", 1)) {
            card.pick(question, option)
        } else if let Some(question) = action.get("other").and_then(Value::as_u64) {
            card.select_other(question as usize)
        } else if let (Some(question), Some(text)) = (
            index("otherText", 0),
            action.pointer("/otherText/1").and_then(Value::as_str),
        ) {
            card.set_other_text(question, text);
            QuestionCardStep::Handled
        } else if let Some(question) = action.get("focus").and_then(Value::as_u64) {
            card.focus(question as usize);
            QuestionCardStep::Handled
        } else if let Some(key) = action.get("key").and_then(Value::as_str) {
            card.key(
                key,
                action
                    .get("shift")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )
        } else if action.get("enter").is_some() {
            card.enter()
        } else if action.get("submit").is_some() {
            card.submit()
        } else if action.get("dismiss").is_some() {
            QuestionCardStep::Dismiss
        } else {
            QuestionCardStep::Stay
        };
        self.apply_question_step(pane, step)
    }

    pub fn agent_key(
        &mut self,
        pane: f64,
        key: &str,
        composer_empty: bool,
        composer_focused: bool,
    ) -> bool {
        let pane_key = pane_id(pane);
        let Some(lens) = self.agents.views.get_mut(&pane_key) else {
            return false;
        };
        if !lens.completions.is_empty() {
            let count = lens.completions.len();
            let current = lens.completion_selected.unwrap_or_default();
            match key {
                "up" => {
                    lens.completion_selected = Some(current.checked_sub(1).unwrap_or(count - 1));
                }
                "down" => lens.completion_selected = Some((current + 1) % count),
                "escape" => lens.dismiss_completions(),
                _ => return false,
            }
            return true;
        }
        let head = self.head_permission(pane_key);
        if self
            .agents
            .views
            .get(&pane_key)
            .is_some_and(|lens| lens.question.is_some())
        {
            if !composer_empty && composer_focused && key != "escape" {
                return false;
            }
            let step = self
                .agents
                .views
                .get_mut(&pane_key)
                .and_then(|lens| lens.question.as_mut())
                .map(|card| card.key(key, false));
            return step.is_some_and(|step| self.apply_question_step(pane_key, step));
        }
        let Some(permission) = head else {
            return false;
        };
        if key == "escape" {
            self.respond_permission(pane_key, permission.request_id, None);
            return true;
        }
        if !composer_empty && composer_focused {
            return false;
        }
        let options = permission.options;
        let replies = options.iter().any(|option| option.allow == Some(false));
        let Some(lens) = self.agents.views.get_mut(&pane_key) else {
            return false;
        };
        match key {
            "enter" if composer_focused => return false,
            "up" if !options.is_empty() => {
                lens.permission_selected = lens
                    .permission_selected
                    .checked_sub(1)
                    .unwrap_or(options.len() - 1);
            }
            "down" if !options.is_empty() => {
                lens.permission_selected = (lens.permission_selected + 1) % options.len();
            }
            key => {
                if replies && key.parse::<usize>().ok() == Some(options.len() + 1) {
                    return true;
                }
                let index = if key == "enter" {
                    Some(lens.permission_selected)
                } else {
                    key.parse::<usize>()
                        .ok()
                        .filter(|digit| (1..=KEYED_OPTIONS).contains(digit))
                        .map(|digit| digit - 1)
                };
                let Some(option) = index.and_then(|index| options.get(index)) else {
                    return false;
                };
                let option = option.id.clone();
                self.respond_permission(pane_key, permission.request_id, Some(option));
            }
        }
        true
    }

    pub fn agent_permission_select(&mut self, pane: f64, index: f64) {
        if let Some(lens) = self.agents.views.get_mut(&pane_id(pane)) {
            lens.permission_selected = index as usize;
        }
    }

    pub fn agent_completion_select(&mut self, pane: f64, index: f64) {
        if let Some(lens) = self.agents.views.get_mut(&pane_id(pane))
            && (index as usize) < lens.completions.len()
        {
            lens.completion_selected = Some(index as usize);
        }
    }

    pub fn agent_completion_accept(&mut self, pane: f64, index: Option<f64>) -> Option<String> {
        let lens = self.agents.views.get_mut(&pane_id(pane))?;
        let index = index.map_or(lens.completion_selected?, |index| index as usize);
        let completion = lens.completions.get(index)?.clone();
        let value = lens.last_input.clone();
        let range = completion.replacement.clone();
        if range.end > value.len() {
            return None;
        }
        let insertion = completion.insertion();
        let next = format!(
            "{}{insertion}{}",
            &value[..range.start],
            &value[range.end..]
        );
        let cursor = byte_to_utf16(&next, range.start + insertion.len());
        lens.dismiss_completions();
        Some(json!({ "value": next, "cursor": cursor }).to_string())
    }

    pub fn agent_providers(&mut self, force: bool) -> String {
        if self.agents.connected && (force || !self.agents.providers_requested) {
            self.agents.providers_requested = true;
            self.invoke(CommandInvocation::new("providers", Vec::<String>::new()));
        }
        let rows = self.core.providers().map(|report| {
            json!({
                "checking": report.checking && report.providers.is_empty(),
                "rows": provider_rows(report).iter().map(|status| json!({
                    "provider": status.provider.as_str(),
                    "label": status.provider.label(),
                    "description": provider_description(status),
                    "version": version_text(status),
                    "current": version_current(status),
                    "action": provider_action(status, report.checking),
                    "notes": provider_notes(status).into_iter().map(|(text, tone)| json!({ "text": text, "tone": tone })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })
        });
        json!({
            "title": "This computer",
            "connected": self.agents.connected,
            "host": rows,
        })
        .to_string()
    }

    pub fn agent_provider_update(&mut self, provider: &str) {
        if self.writable() {
            self.invoke(CommandInvocation::new("providers", ["-u", provider]));
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use zz_protocol::{PaneId, ProtocolMessage, decode_protocol_frame};

    use super::Lens;
    use crate::Client;

    fn item(seq: u64, body: Value) -> Vec<u8> {
        let mut body = body;
        body["seq"] = json!(seq);
        serde_json::to_vec(&body).unwrap()
    }

    fn client(pane: PaneId) -> Client {
        let mut client = Client::new(7.0);
        client.agents.connected = true;
        client.agents.lenses.insert(pane);
        client.agents.views.insert(pane, Lens::new());
        client
    }

    fn sent(client: &mut Client) -> Vec<ProtocolMessage> {
        std::iter::from_fn(|| client.outbound())
            .filter_map(|frame| decode_protocol_frame(&frame).ok())
            .collect()
    }

    #[test]
    fn a_replayed_lens_folds_turns_offers_commands_and_answers_permissions() {
        let pane = PaneId(4);
        let mut client = client(pane);
        let prompt = |text: &str| json!({"item":"update","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":text}}});
        client.agents.updates(
            pane,
            1,
            vec![
                item(1, json!({"item":"ready","capabilities":{"verbs":true}})),
                item(2, json!({"item":"update","update":{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"review","description":"Review changes","input":{"hint":"branch"}}]}})),
                item(3, prompt("Run ls")),
                item(4, json!({"item":"update","update":{"sessionUpdate":"tool_call","toolCallId":"t1","title":"ls","kind":"execute","status":"completed","content":[{"type":"content","content":{"type":"text","text":"notes.txt"}}]}})),
                item(5, json!({"item":"update","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"- one\n- two"}}})),
                item(6, json!({"item":"promptFinished","outcome":{"outcome":"finished","stop_reason":"end_turn"}})),
                item(7, prompt("Delete it")),
                item(8, json!({"item":"permissionRequested","request_id":9,"tool_call":{"toolCallId":"t2","title":"Delete it"},"options":[{"optionId":"yes","name":"Allow","kind":"allow_once"},{"optionId":"no","name":"Deny","kind":"reject_once"}]})),
            ],
        );
        let view = client.lens_view(pane, "/re", 3, "");
        let rows = view["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["kind"], "user");
        assert_eq!(rows[1]["kind"], "turn");
        assert_eq!(rows[3]["trace"]["state"], "done");
        assert_eq!(rows[1]["trace"]["label"], "Worked");
        assert_eq!(rows[1]["trace"]["counts"][0]["describe"], "1 command");
        assert!(
            rows[1]["answers"][0]["html"]
                .as_str()
                .unwrap()
                .contains("<li>one</li>")
        );
        assert_eq!(view["ask"]["kind"], "approval");
        assert_eq!(view["ask"]["options"][1]["divider"], true);
        assert_eq!(view["ask"]["send"], "Allow");
        let names = view["completions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|completion| completion["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert!(names.contains(&"review".to_owned()) && names.contains(&"rewind".to_owned()));

        let view = client.lens_view(pane, "é /rev", 6, "");
        assert_eq!(view["completions"][0]["name"], "review");
        let accepted: Value =
            serde_json::from_str(&client.agent_completion_accept(4.0, None).unwrap()).unwrap();
        assert_eq!(accepted, json!({"value": "é /review ", "cursor": 10}));
        assert_eq!(
            client.lens_view(pane, "/review ", 8, "")["hint"],
            "Argument · branch"
        );

        assert!(client.agent_key(4.0, "down", true, true));
        let view = client.lens_view(pane, "", 0, "");
        assert_eq!(view["ask"]["options"][1]["highlighted"], true);
        assert_eq!(view["ask"]["send"], "Deny");
        assert!(client.agent_key(4.0, "enter", true, false));
        let messages = sent(&mut client);
        assert!(messages.iter().any(|message| matches!(
            message,
            ProtocolMessage::AgentRespondPermission { request_id: 9, option_id: Some(option), .. } if option == "no"
        )));
        assert!(client.lens_view(pane, "", 0, "")["ask"].is_null());
    }
}
