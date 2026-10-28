use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;
use zz_client::{
    agent_completion::{
        AgentCommand, CommandCompletion, active_command_hint, bare_command_name, completion_query,
        meaningful_command_description, pane_commands, ranked_completions, rendered_error,
    },
    agent_lens::{
        AgentCursor, LENS_WELCOME_DETAIL, LensEmpty, LensTranscript, MAX_AGENT_HISTORY_BYTES,
        MAX_AGENT_HISTORY_ITEMS, PendingPermission, accepts_prompt, conversation_changed,
        lens_empty, lens_placeholder, lens_welcome_title, pending_permissions, restore_reclaim_id,
        restored_prompts, track_usage, turn_running,
    },
    agent_question::{QuestionCard, QuestionCardStep},
    agent_timeline::{
        AgentToolKind, AgentToolStatus, ComposerAction, StepNesting, TRACE_COUNT_NOUNS,
        TimelineGroupKind, TimelineItem, TimelineRow, TimelineShape, TraceHeader, TurnClocks,
        composer_action, context_usage_fraction, context_usage_tooltip, ending_failure,
        final_assistant_entry_id, fold_timeline_rows, git_file_count_label, git_summary_tooltip,
        is_turn_row, plan_chip_label, plan_items, plan_progress, single_line, split_turn,
        task_tray_label, thought_tail, tool_tag, trace_header,
    },
    agent_transcript::{AgentThreadEntry, ToolPayload, is_zz_command, rewind_command},
    driver_chat::{DriverMessageKind, driver_messages, driver_pane, driver_steps},
    driver_log::DriverLog,
    mend::mend,
    providers::{
        provider_action, provider_description, provider_notes, provider_rows, version_current,
        version_text,
    },
    window_summary::{pane_agent, pane_program},
};
use zz_protocol::{
    AgentConnectionPhase, AgentPaneWire, AgentProvider, AgentQuestionAnswer, CommandInvocation,
    PaneId, PaneSnapshot, ProtocolMessage, agent_stream::AgentStreamPayload,
};

use crate::{Client, pane_id};

const MAX_STREAMING_MEND_BYTES: usize = 64 * 1024;
const DRIVER_REPLAY_SETTLE_MS: f64 = 3_000.0;
const DRIVER_WAKE_GRACE_MS: f64 = 20_000.0;

type Image = String;

fn decode_image(format: &str, data: Vec<u8>) -> Option<Image> {
    matches!(
        format,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
    .then(|| format!("data:{format};base64,{}", BASE64.encode(data)))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tray {
    Plan,
    Tasks,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Wake {
    Asleep,
    Waking,
    Stalled,
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
    tray: Option<Tray>,
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
            tray: None,
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

struct Driver {
    pane: Option<PaneId>,
    program: Option<&'static str>,
    log: DriverLog,
    applied: u64,
    usage: Option<(u64, u64)>,
    times: HashMap<u64, String>,
    following_since: f64,
    read: usize,
    wake: Wake,
    wake_since: f64,
}

impl Default for Driver {
    fn default() -> Self {
        Self {
            pane: None,
            program: None,
            log: DriverLog::default(),
            applied: 0,
            usage: None,
            times: HashMap::new(),
            following_since: 0.0,
            read: 0,
            wake: Wake::Asleep,
            wake_since: 0.0,
        }
    }
}

impl Driver {
    fn reset(&mut self, now: f64) {
        self.log = DriverLog::default();
        self.applied = 0;
        self.usage = None;
        self.times.clear();
        self.read = 0;
        self.following_since = now;
    }
}

#[derive(Default)]
pub struct Agents {
    connected: bool,
    lenses: BTreeSet<PaneId>,
    follows: BTreeSet<PaneId>,
    cursors: HashMap<PaneId, AgentCursor>,
    journals: HashMap<PaneId, Vec<(u64, Vec<u8>)>>,
    views: HashMap<PaneId, Lens>,
    replays: Vec<(PaneId, u64)>,
    status: Option<String>,
    driver: Driver,
    providers_requested: bool,
}

impl Agents {
    pub fn updates(&mut self, pane: PaneId, first_seq: u64, items: Vec<Vec<u8>>) {
        if !self.watched(pane) {
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
        if self.watched(pane) {
            self.request_replay(pane);
        }
    }

    fn watched(&self, pane: PaneId) -> bool {
        self.lenses.contains(&pane) || self.follows.contains(&pane)
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

    fn follow(&mut self, pane: Option<PaneId>) {
        let followed = pane.into_iter().collect::<BTreeSet<_>>();
        if followed == self.follows {
            return;
        }
        let dropped = self
            .follows
            .difference(&followed)
            .copied()
            .collect::<Vec<_>>();
        let added = followed
            .difference(&self.follows)
            .copied()
            .collect::<Vec<_>>();
        self.follows = followed;
        for pane in dropped {
            if !self.watched(pane) {
                self.forget_stream(pane);
            }
        }
        for pane in added {
            if !self.lenses.contains(&pane) {
                self.forget_stream(pane);
                self.request_replay(pane);
            }
        }
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
    fn lens_view(&mut self, pane: PaneId, value: &str, cursor: usize) -> Value {
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
        if state.tasks.is_empty() && lens.tray == Some(Tray::Tasks) {
            lens.tray = None;
        }
        let tray = (plan.is_some() || !state.tasks.is_empty()).then(|| {
            json!({
                "plan": plan.as_deref().and_then(|source| {
                    let (label, current) = plan_chip_label(source)?;
                    Some(json!({
                        "label": label,
                        "current": current,
                        "open": lens.tray == Some(Tray::Plan),
                        "items": plan_items(source).into_iter().map(|(state, text)| json!({
                            "state": state,
                            "text": text,
                        })).collect::<Vec<_>>(),
                    }))
                }),
                "tasks": (!state.tasks.is_empty()).then(|| json!({
                    "label": task_tray_label(state.tasks.len()),
                    "open": lens.tray == Some(Tray::Tasks),
                    "rows": state.tasks.iter().map(|task| json!({
                        "id": task.id,
                        "kind": task.kind,
                        "description": task.description,
                        "toolCall": task.tool_call_id,
                    })).collect::<Vec<_>>(),
                })),
            })
        });
        let card = head.map(|head| {
            let counter = (pending_count > 1).then(|| format!("1/{pending_count}"));
            if let Some(card) = lens
                .question
                .as_ref()
                .filter(|card| card.request_id() == head.request_id)
            {
                question_json(card, &head.title, counter, context.writable)
            } else if head.options.is_empty() {
                json!({ "kind": "wait", "requestId": head.request_id, "title": head.title })
            } else {
                json!({
                    "kind": "permission",
                    "requestId": head.request_id,
                    "title": head.title,
                    "counter": counter,
                    "writable": context.writable,
                    "options": head.options.iter().enumerate().map(|(index, option)| json!({
                        "id": option.id,
                        "name": option.name,
                        "danger": option.allow == Some(false),
                        "selected": index == lens.permission_selected,
                    })).collect::<Vec<_>>(),
                })
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
            match lens_empty(context.connected, &state.phase, context.provider) {
                LensEmpty::Welcome(provider) => json!({
                    "welcome": {
                        "title": lens_welcome_title(provider),
                        "detail": LENS_WELCOME_DETAIL,
                        "provider": provider_id(provider),
                    }
                }),
                LensEmpty::Message { text, busy } => json!({ "text": text, "busy": busy }),
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
            "placeholder": lens_placeholder(context.provider),
            "cwd": cwd,
            "connected": context.connected,
            "writable": context.writable,
            "running": running,
            "watchOnly": context.watch,
            "rows": row_values,
            "empty": empty,
            "card": card,
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

    fn driver_sync(&mut self, now: f64) {
        let found = if self.agents.connected {
            driver_pane(self.core.snapshot())
                .map(|(pane, _)| (pane.id, pane_program(pane).map(|program| program.id)))
        } else {
            None
        };
        let pane = found.map(|(pane, _)| pane);
        if pane != self.agents.driver.pane {
            self.agents.follow(pane);
            self.agents.driver.pane = pane;
            self.agents.driver.program = found.and_then(|(_, program)| program);
            if pane.is_some() {
                self.agents.driver.wake = Wake::Asleep;
            }
            self.agents.driver.reset(now);
        }
        if self.agents.driver.wake == Wake::Waking
            && now - self.agents.driver.wake_since > DRIVER_WAKE_GRACE_MS
        {
            self.agents.driver.wake = Wake::Stalled;
        }
        let Some(pane) = pane else {
            return;
        };
        let journal = self
            .agents
            .journals
            .get(&pane)
            .map_or(&[][..], Vec::as_slice);
        let driver = &mut self.agents.driver;
        let newest = journal.last().map_or(0, |(sequence, _)| *sequence);
        let skipped = journal
            .first()
            .is_some_and(|(first, _)| driver.applied > 0 && *first > driver.applied + 1);
        if newest == driver.applied && !skipped {
            return;
        }
        let restart = newest < driver.applied || skipped;
        let start = if restart { 0 } else { driver.applied };
        let items: Vec<Value> = journal
            [journal.partition_point(|(sequence, _)| *sequence <= start)..]
            .iter()
            .filter_map(|(_, bytes)| serde_json::from_slice(bytes).ok())
            .collect();
        if restart {
            let read = driver.read;
            driver.reset(now);
            driver.read = read;
        }
        for item in items {
            track_usage(&mut driver.usage, &item);
            if let Ok(payload) = serde_json::from_value::<AgentStreamPayload>(item) {
                driver.log.apply(payload);
            }
        }
        driver.applied = newest;
    }

    fn driver_running(&self) -> bool {
        let Some(pane) = self.agents.driver.pane else {
            return false;
        };
        self.agents.driver.log.live()
            || self
                .core
                .agent_state(pane)
                .is_some_and(|state| turn_running(&state.phase))
    }

    fn sender_name(&self, pane: PaneId) -> (String, Option<&'static str>) {
        self.core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| {
                window.panes.get(&pane).map(|snapshot| {
                    (
                        window.name.clone(),
                        pane_program(snapshot).map(|program| program.id),
                    )
                })
            })
            .unwrap_or_else(|| (pane.to_string(), None))
    }
}

fn question_json(
    card: &QuestionCard,
    title: &str,
    counter: Option<String>,
    writable: bool,
) -> Value {
    json!({
        "kind": "question",
        "requestId": card.request_id(),
        "title": title,
        "counter": counter,
        "writable": writable,
        "focused": card.focused(),
        "complete": card.is_complete(),
        "problem": card.problem(),
        "questions": card.questions().iter().enumerate().map(|(question, asked)| json!({
            "header": asked.header,
            "question": asked.question,
            "multiSelect": asked.multi_select,
            "allowOther": asked.allow_other,
            "answered": card.is_answered(question),
            "other": card.other_selected(question),
            "options": asked.options.iter().enumerate().map(|(option, choice)| json!({
                "label": choice.label,
                "description": choice.description,
                "picked": card.is_picked(question, option),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
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
            let watched = self
                .agents
                .lenses
                .union(&self.agents.follows)
                .copied()
                .collect::<Vec<_>>();
            for pane in watched {
                self.agents.request_replay(pane);
            }
            self.pump_agents();
        }
    }

    pub fn agent_prune(&mut self) {
        let stale = self
            .agents
            .lenses
            .union(&self.agents.follows)
            .copied()
            .filter(|pane| self.pane_snapshot(*pane).and_then(pane_agent).is_none())
            .collect::<Vec<_>>();
        for pane in stale {
            self.agents.lenses.remove(&pane);
            self.agents.follows.remove(&pane);
            self.agents.views.remove(&pane);
            self.agents.forget_stream(pane);
        }
    }

    pub fn agent_forget(&mut self, pane: f64) {
        let pane = pane_id(pane);
        self.agents.lenses.remove(&pane);
        self.agents.follows.remove(&pane);
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

    pub fn agent_lens_open(&self, pane: f64) -> bool {
        self.agents.lenses.contains(&pane_id(pane))
    }

    pub fn agent_open(&mut self, pane: f64) -> bool {
        let pane = pane_id(pane);
        if self.pane_snapshot(pane).and_then(pane_agent).is_none() {
            return false;
        }
        let watched = self.agents.watched(pane);
        if !self.agents.lenses.insert(pane) {
            return true;
        }
        self.agents.views.insert(pane, Lens::new());
        if !watched {
            self.agents.forget_stream(pane);
            self.agents.request_replay(pane);
        }
        self.pump_agents();
        true
    }

    pub fn agent_close(&mut self, pane: f64) {
        let pane = pane_id(pane);
        if self.agents.lenses.remove(&pane) {
            self.agents.views.remove(&pane);
            if !self.agents.watched(pane) {
                self.agents.forget_stream(pane);
            }
        }
    }

    pub fn agent_view(&mut self, pane: f64, value: &str, cursor: f64) -> String {
        let pane = pane_id(pane);
        let view = self.lens_view(pane, value, cursor as usize);
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
                    .and_then(|lens| lens.question.as_ref().map(QuestionCard::submit));
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

    pub fn agent_tray(&mut self, pane: f64, panel: &str) {
        let panel = if panel == "plan" {
            Tray::Plan
        } else {
            Tray::Tasks
        };
        if let Some(lens) = self.agents.views.get_mut(&pane_id(pane)) {
            lens.tray = (lens.tray != Some(panel)).then_some(panel);
        }
    }

    pub fn agent_stop_task(&mut self, pane: f64, task_id: &str) {
        if self.writable() {
            self.send(&ProtocolMessage::AgentStopTask {
                pane: pane_id(pane),
                task_id: task_id.to_owned(),
            });
        }
    }

    pub fn agent_reveal(&self, pane: f64, tool_call_id: &str) -> Option<String> {
        let lens = self.agents.views.get(&pane_id(pane))?;
        let entry = lens.transcript.model.tool_entry(tool_call_id)?;
        let entries = items(&lens.transcript);
        let index = entries.iter().position(|item| item.id == entry)?;
        let folded = fold_timeline_rows(&entries);
        let row = folded.rows.get(*folded.entry_to_row.get(index)?)?;
        let prefix = match row {
            TimelineRow::Group {
                kind: TimelineGroupKind::Reply,
                ..
            } => "r",
            TimelineRow::Single(item) if item.shape == TimelineShape::User => "u",
            TimelineRow::Single(item) if item.shape == TimelineShape::Notice => "n",
            _ => "t",
        };
        Some(format!("{prefix}{}", row.id()))
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
            if !composer_empty && composer_focused {
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
        if !composer_empty && composer_focused {
            return false;
        }
        let Some(permission) = head else {
            return false;
        };
        let options = permission.options;
        let Some(lens) = self.agents.views.get_mut(&pane_key) else {
            return false;
        };
        match key {
            "enter" if composer_focused => return false,
            "escape" => {
                self.respond_permission(pane_key, permission.request_id, None);
            }
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
                let index = if key == "enter" {
                    Some(lens.permission_selected)
                } else {
                    key.parse::<usize>()
                        .ok()
                        .filter(|digit| (1..=9).contains(digit))
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

    pub fn driver_view(&mut self, now: f64, clock: &str, host: &str, open: bool) -> String {
        self.driver_sync(now);
        self.pump_agents();
        let running = self.driver_running();
        let driver = &self.agents.driver;
        let replaying = now - driver.following_since < DRIVER_REPLAY_SETTLE_MS;
        let entries = driver.log.entries().to_vec();
        let messages = if driver.pane.is_some() {
            driver_messages(&entries, running)
        } else {
            Vec::new()
        };
        for message in &messages {
            self.agents
                .driver
                .times
                .entry(message.id)
                .or_insert_with(|| {
                    if replaying {
                        String::new()
                    } else {
                        clock.to_owned()
                    }
                });
        }
        let steps = running.then(|| driver_steps(&entries));
        let rendered = messages
            .iter()
            .map(|message| {
                let time = self.agents.driver.times.get(&message.id).cloned().unwrap_or_default();
                match &message.kind {
                    DriverMessageKind::Mine { read } => json!({
                        "id": message.id, "kind": "mine", "read": read, "text": message.text, "time": time,
                    }),
                    DriverMessageKind::Driver => json!({
                        "id": message.id, "kind": "driver", "text": message.text, "time": time,
                        "html": markdown_html(&message.text),
                    }),
                    DriverMessageKind::Agent(pane) => {
                        let (name, program) = self.sender_name(*pane);
                        json!({
                            "id": message.id, "kind": "agent", "text": message.text, "time": time,
                            "html": markdown_html(&message.text),
                            "sender": { "name": name, "program": program },
                        })
                    }
                }
            })
            .collect::<Vec<_>>();
        let driver = &mut self.agents.driver;
        let unread = messages.len() > driver.read;
        if open {
            driver.read = messages.len();
        }
        let (note, label, force) = match driver.wake {
            Wake::Stalled => (
                "No driver started. When too few of your hosts answer, zz holds off so two drivers never run at once.",
                "Start here",
                true,
            ),
            Wake::Asleep | Wake::Waking => (
                "The Driver Agent is asleep. Wake follows a driver another host runs, or starts one on this host.",
                if driver.wake == Wake::Waking {
                    "Waking"
                } else {
                    "Wake"
                },
                false,
            ),
        };
        let permission = driver.log.permissions().first().map(|request| {
            json!({
                "requestId": request.request_id,
                "title": request.title,
                "options": request.options.iter().map(|option| json!({
                    "id": option.id,
                    "name": option.name,
                    "danger": matches!(option.kind, zz_client::agent_transcript::AgentPermissionKind::RejectOnce | zz_client::agent_transcript::AgentPermissionKind::RejectAlways),
                })).collect::<Vec<_>>(),
            })
        });
        json!({
            "driver": driver.pane.map(|pane| json!({ "pane": pane.0, "host": host, "program": driver.program })),
            "asleep": driver.pane.is_none().then(|| json!({
                "note": note,
                "label": label,
                "force": force,
                "waking": driver.wake == Wake::Waking,
            })),
            "messages": rendered,
            "typing": steps,
            "permission": permission,
            "running": running,
            "unread": unread,
            "usage": driver.usage.map(|(used, size)| json!({
                "fraction": context_usage_fraction(used, size),
                "tooltip": context_usage_tooltip(used, size),
            })),
        })
        .to_string()
    }

    pub fn driver_send(&mut self, text: &str) -> bool {
        let Some(pane) = self.agents.driver.pane else {
            return false;
        };
        if text.trim().is_empty() || !self.writable() {
            return false;
        }
        let message = if self.driver_running() {
            ProtocolMessage::AgentSteer {
                pane,
                text: text.to_owned(),
            }
        } else {
            ProtocolMessage::AgentPrompt {
                pane,
                text: text.to_owned(),
                images: Vec::new(),
            }
        };
        self.send(&message);
        true
    }

    pub fn driver_stop(&mut self) -> bool {
        let Some(pane) = self.agents.driver.pane else {
            return false;
        };
        if !self.driver_running() || !self.writable() {
            return false;
        }
        self.send(&ProtocolMessage::AgentCancel { pane });
        true
    }

    pub fn driver_respond(&mut self, request_id: f64, option: Option<String>) {
        let Some(pane) = self.agents.driver.pane else {
            return;
        };
        if self.writable() {
            self.send(&ProtocolMessage::AgentRespondPermission {
                pane,
                request_id: request_id as u64,
                option_id: option,
            });
        }
    }

    pub fn driver_wake(&mut self, force: bool, now: f64) {
        if self.agents.driver.pane.is_some() || self.agents.driver.wake == Wake::Waking {
            return;
        }
        if !self.writable() {
            return;
        }
        let line = if force {
            "zz driver recover --force"
        } else {
            "zz driver recover"
        };
        self.invoke(CommandInvocation::new("run-shell", ["-b", line]));
        self.agents.driver.wake = Wake::Waking;
        self.agents.driver.wake_since = now;
    }

    pub fn driver_terminal(&self) -> Option<String> {
        let pane = self.agents.driver.pane?;
        let session = self
            .core
            .snapshot()
            .sessions
            .iter()
            .find(|session| session.name == zz_client::driver_chat::DRIVER_SESSION)?;
        Some(json!({ "session": session.id.0, "pane": pane.0 }).to_string())
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
        let view = client.lens_view(pane, "/re", 3);
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
        assert_eq!(view["card"]["kind"], "permission");
        assert_eq!(view["card"]["options"][1]["danger"], true);
        let names = view["completions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|completion| completion["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert!(names.contains(&"review".to_owned()) && names.contains(&"rewind".to_owned()));

        let view = client.lens_view(pane, "é /rev", 6);
        assert_eq!(view["completions"][0]["name"], "review");
        let accepted: Value =
            serde_json::from_str(&client.agent_completion_accept(4.0, None).unwrap()).unwrap();
        assert_eq!(accepted, json!({"value": "é /review ", "cursor": 10}));
        assert_eq!(
            client.lens_view(pane, "/review ", 8)["hint"],
            "Argument · branch"
        );

        assert!(client.agent_key(4.0, "down", true, true));
        assert_eq!(
            client.lens_view(pane, "", 0)["card"]["options"][1]["selected"],
            true
        );
        assert!(client.agent_key(4.0, "enter", true, false));
        let messages = sent(&mut client);
        assert!(messages.iter().any(|message| matches!(
            message,
            ProtocolMessage::AgentRespondPermission { request_id: 9, option_id: Some(option), .. } if option == "no"
        )));
        assert!(client.lens_view(pane, "", 0)["card"].is_null());
    }
}
