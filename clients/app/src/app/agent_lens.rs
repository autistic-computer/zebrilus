use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde_json::Value;
use zz_client::agent_completion::{
    AgentCommand, CommandCompletion, active_command_hint, bare_command_name, completion_query,
    meaningful_command_description, pane_commands, ranked_completions,
};
use zz_client::agent_transcript::{
    AgentPermissionKind, AgentThreadEntry, AgentToolKindModel, AgentToolStatusModel,
    AgentTranscript, ToolPayload, is_zz_command, rewind_command,
};
use zz_gpui::{
    AnyElement, App, Context, Corners, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ListAlignment, ListState, Pixels, Render, Subscription, Window, div, prelude::*, px,
};
use zz_protocol::{
    AgentConnectionPhase, AgentImage, AgentProvider, AgentQuestionAnswer, PaneId, PaneSnapshot,
    ProtocolMessage, agent_stream::AgentQuestion,
};
use zz_ui::{
    Disableable as _,
    agent::{
        AgentEntry, AgentMarkdown, AgentTimeline, AgentTimelineStore, AgentToolEntry,
        AgentToolKind, AgentToolPayload, AgentToolStatus, AgentToolText, MarkdownSlot, TimelineRow,
        TimelineStick, agent_jump_to_bottom_button,
        composer::{AgentComposer, COMPOSER_OUTER_PADDING, composer_ground},
        composer_attachment,
        controls::{
            ComposerAction, agent_queue_chip, composer_action, composer_action_button,
            context_usage_meter, git_summary_footer,
        },
        fold_timeline_rows,
        presentation::{
            empty_state, error_card, lens_welcome, permission_card, permission_option,
            terminal_wait_card,
        },
        question::{QuestionCardAction, QuestionCardState, QuestionCardStep},
        tasks::{TaskTrayAction, TrayPanel, task_tray},
        tool_output_text,
    },
    button::Button,
    input::{IndentInline, InputEvent, InputState, MoveDown, MoveUp},
    scroll::Scrollbar,
};

use crate::connection::Connection;

pub(super) struct AgentLens {
    pane: PaneId,
    connection: Entity<Connection>,
    connected: bool,
    input: Entity<InputState>,
    commands: Arc<[AgentCommand]>,
    completions: Arc<[CommandCompletion]>,
    completion_selected: Option<usize>,
    completion_dismissed: bool,
    completion_scroll: zz_gpui::UniformListScrollHandle,
    last_input: String,
    last_cursor: usize,
    transcript: Transcript,
    timeline: Entity<AgentTimelineStore>,
    rows: Arc<Vec<TimelineRow>>,
    entry_to_row: Vec<usize>,
    scroll: ListState,
    stick: TimelineStick,
    last_sequence: u64,
    last_reclaim_id: u64,
    attachments: Vec<AgentImage>,
    draft_error: Option<String>,
    permission_request_id: Option<u64>,
    permission_selected: usize,
    permission_answered: HashSet<u64>,
    question: Option<QuestionCardState>,
    tray_open: Option<TrayPanel>,
    usage: Option<(u64, u64)>,
    transcript_dirty: bool,
    corner_radii: Corners<Pixels>,
    _subscriptions: Vec<Subscription>,
}

impl AgentLens {
    pub(super) fn new(
        pane: PaneId,
        connection: Entity<Connection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let agent = connection
            .read(cx)
            .terminal_agent(pane)
            .map_or("the agent", AgentProvider::label);
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(format!("Message {agent}…"))
                .auto_grow(2, 8)
                .submit_on_enter(true)
                .context_menu(true)
        });
        let observation = cx.observe(&connection, |this, connection, cx| {
            let connected = connection.read(cx).connected;
            if this.connected != connected {
                this.connected = connected;
                this.permission_answered.clear();
                cx.notify();
            }
        });
        let events = cx.subscribe(
            &connection,
            move |this, _, event: &zz_client::CoreEvent, cx| {
                if matches!(
                    event,
                    zz_client::CoreEvent::HelloReceived | zz_client::CoreEvent::Attached { .. }
                ) {
                    this.transcript = Transcript::default();
                    this.usage = None;
                    this.last_sequence = 0;
                    this.rows = Arc::new(Vec::new());
                    this.entry_to_row.clear();
                    this.scroll.reset(0);
                    this.stick.engage_now(&this.scroll, cx.reduce_motion());
                    this.timeline.update(cx, |timeline, cx| {
                        timeline.clear(cx);
                    });
                    cx.notify();
                } else if matches!(event,
                zz_client::CoreEvent::AgentUpdates { pane: changed, .. }
                | zz_client::CoreEvent::AgentStateChanged { pane: changed, .. } if *changed == pane)
                {
                    cx.notify();
                }
            },
        );
        let subscription =
            cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => this.enter(window, cx),
                InputEvent::Change => {
                    this.draft_error = None;
                    cx.notify();
                }
                InputEvent::PasteImages(images) => this.attach_images(
                    images
                        .iter()
                        .map(|image| AgentImage {
                            format: image.format.mime_type().to_owned(),
                            data: image.bytes.clone(),
                        })
                        .collect(),
                    cx,
                ),
                _ => {}
            });
        let input_observer = cx.observe(&input, |this, _, cx| {
            if this.synchronize_completions(cx) {
                cx.notify();
            }
        });
        let timeline = cx.new(|_| AgentTimelineStore::default());
        let timeline_observer = cx.observe(&timeline, |_, _, cx| cx.notify());
        let scroll = ListState::new(0, ListAlignment::Top, px(1_200.0));
        let stick = TimelineStick::new(&scroll, cx.reduce_motion());
        let scroll_view = cx.weak_entity();
        scroll.set_scroll_handler(move |_, _, cx| {
            let view = scroll_view.clone();
            cx.defer(move |cx| {
                let _ = view.update(cx, |this: &mut Self, cx| {
                    if this.stick.on_user_scroll(&this.scroll, cx.reduce_motion()) {
                        cx.notify();
                    }
                });
            });
        });
        Self {
            pane,
            connection,
            connected: false,
            input,
            commands: Arc::from([]),
            completions: Arc::from([]),
            completion_selected: None,
            completion_dismissed: false,
            completion_scroll: zz_gpui::UniformListScrollHandle::new(),
            last_input: String::new(),
            last_cursor: 0,
            transcript: Transcript::default(),
            timeline,
            rows: Arc::new(Vec::new()),
            entry_to_row: Vec::new(),
            scroll,
            stick,
            last_sequence: 0,
            last_reclaim_id: 0,
            attachments: Vec::new(),
            draft_error: None,
            permission_request_id: None,
            permission_selected: 0,
            permission_answered: HashSet::new(),
            question: None,
            tray_open: None,
            usage: None,
            transcript_dirty: false,
            corner_radii: Corners::default(),
            _subscriptions: vec![
                observation,
                events,
                subscription,
                timeline_observer,
                input_observer,
            ],
        }
    }

    fn terminal<'a>(&self, cx: &'a App) -> Option<&'a PaneSnapshot> {
        self.connection
            .read(cx)
            .core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&self.pane))
    }

    fn cwd(&self, cx: &App) -> Option<PathBuf> {
        self.terminal(cx)
            .map(|pane| pane.summary.cwd.as_str())
            .filter(|cwd| !cwd.is_empty())
            .map(PathBuf::from)
    }

    fn synchronize_timeline_context(&self, cx: &mut Context<Self>) {
        let running = self
            .connection
            .read(cx)
            .core
            .agent_state(self.pane)
            .is_some_and(|state| {
                matches!(
                    state.phase,
                    AgentConnectionPhase::Running | AgentConnectionPhase::AwaitingPermission
                )
            });
        let streaming = running
            .then(|| self.transcript.entries.last().map(AgentEntry::id))
            .flatten();
        let cwd = self.cwd(cx);
        self.timeline.update(cx, |timeline, cx| {
            timeline.set_streaming(streaming, cx);
            timeline.set_cwd(cwd, cx);
        });
    }

    pub(super) fn set_corner_radii(&mut self, radii: Corners<Pixels>, cx: &mut Context<Self>) {
        if self.corner_radii != radii {
            self.corner_radii = radii;
            cx.notify();
        }
    }

    fn synchronize_completions(&mut self, cx: &mut Context<Self>) -> bool {
        let input = self.input.read(cx);
        let value = input.value().to_string();
        let cursor = input.cursor();
        let commands: Arc<[AgentCommand]> = {
            let connection = self.connection.read(cx);
            pane_commands(
                &connection.agent_commands(self.pane),
                connection.agent_verbs_supported(self.pane),
            )
            .into()
        };
        let input_changed = value != self.last_input || cursor != self.last_cursor;
        if !input_changed && self.commands == commands {
            return false;
        }
        if input_changed {
            self.last_input = value;
            self.last_cursor = cursor;
            self.completion_dismissed = false;
        }
        self.commands = commands;
        self.completions = if self.completion_dismissed {
            Arc::from([])
        } else {
            completion_query(&self.last_input, self.last_cursor).map_or_else(
                || Arc::from([]),
                |query| ranked_completions(&self.commands, &query).into(),
            )
        };
        self.completion_selected = (!self.completions.is_empty()).then_some(
            self.completion_selected
                .unwrap_or_default()
                .min(self.completions.len().saturating_sub(1)),
        );
        if let Some(selected) = self.completion_selected {
            self.completion_scroll
                .scroll_to_item(selected, zz_gpui::ScrollStrategy::Nearest);
        }
        true
    }

    fn enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.read(cx).value().trim().is_empty() {
            let head = self.current_permission(cx);
            self.synchronize_question_card(head.as_ref(), window, cx);
            if let Some(card) = &self.question {
                let step = card.card.submit();
                self.apply_question_step(step, window, cx);
                return;
            }
            if let Some((request_id, option)) = head.and_then(|permission| {
                let option = permission.options.get(self.permission_selected)?.id.clone();
                Some((permission.request_id, option))
            }) {
                self.choose_permission(request_id, option, cx);
                return;
            }
        }
        self.synchronize_completions(cx);
        if let Some(index) = self.completion_selected {
            self.accept_completion(index, window, cx);
        } else {
            self.submit(window, cx);
        }
    }

    fn accept_completion(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(completion) = self.completions.get(index).cloned() else {
            return;
        };
        let insertion = completion.insertion();
        self.completion_dismissed = true;
        self.input.update(cx, |input, cx| {
            input.set_selected_range(completion.replacement, cx);
            input.replace(insertion, window, cx);
        });
        self.completions = Arc::from([]);
        self.completion_selected = None;
        self.input.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn complete(&mut self, _: &IndentInline, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.completion_selected {
            self.accept_completion(index, window, cx);
            cx.stop_propagation();
        }
    }

    fn navigate_completion(&mut self, direction: isize, cx: &mut Context<Self>) {
        if self.completions.is_empty() {
            return;
        }
        let count = self.completions.len();
        let current = self.completion_selected.unwrap_or_default();
        let selected = if direction < 0 {
            current.checked_sub(1).unwrap_or(count - 1)
        } else {
            (current + 1) % count
        };
        self.completion_selected = Some(selected);
        self.completion_scroll
            .scroll_to_item(selected, zz_gpui::ScrollStrategy::Nearest);
        cx.notify();
    }

    fn move_completion_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        if !self.completions.is_empty() {
            self.navigate_completion(-1, cx);
            cx.stop_propagation();
        }
    }

    fn move_completion_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        if !self.completions.is_empty() {
            self.navigate_completion(1, cx);
            cx.stop_propagation();
        }
    }

    fn render_completions(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if self.completions.is_empty() {
            return None;
        }
        let completions = self.completions.clone();
        let selected = self.completion_selected;
        let view = cx.entity();
        let pane = self.pane;
        let rows = zz_gpui::uniform_list(
            ("web-agent-completion-rows", pane.0),
            completions.len(),
            move |range, _, cx| {
                range
                    .filter_map(|index| {
                        let completion = completions.get(index)?;
                        let command = &completion.command;
                        let hover = view.clone();
                        let click = view.clone();
                        Some(
                            zz_ui::agent::slash::suggestion_row(
                                format!("web-agent-completion-{}-{index}", pane.0),
                                bare_command_name(&command.name),
                                meaningful_command_description(&command.description)
                                    .map(|description| description.to_owned().into()),
                                selected == Some(index),
                                cx,
                            )
                            .on_hover(move |hovered, _, cx| {
                                if *hovered {
                                    hover.update(cx, |this, cx| {
                                        if this.completion_selected != Some(index) {
                                            this.completion_selected = Some(index);
                                            cx.notify();
                                        }
                                    });
                                }
                            })
                            .on_click(move |_, window, cx| {
                                click.update(cx, |this, cx| {
                                    this.accept_completion(index, window, cx);
                                });
                                cx.stop_propagation();
                            }),
                        )
                    })
                    .collect::<Vec<_>>()
            },
        )
        .w_full()
        .h(zz_ui::agent::slash::suggestion_list_height(
            self.completions.len(),
        ))
        .track_scroll(&self.completion_scroll);
        Some(zz_ui::agent::slash::suggestion_list(rows, cx).into_any_element())
    }

    fn send_prompt(&mut self, text: &str, images: &[AgentImage], cx: &mut Context<Self>) -> bool {
        let connection = self.connection.read(cx);
        let allowed = connection.connected
            && !connection.core.attached_read_only()
            && connection.core.agent_state(self.pane).is_some_and(|state| {
                matches!(
                    state.phase,
                    AgentConnectionPhase::Running | AgentConnectionPhase::AwaitingPermission
                ) || (state.phase == AgentConnectionPhase::Ready
                    && state.pending_permission.is_none())
            });
        if !allowed || (text.trim().is_empty() && images.is_empty()) {
            return false;
        }
        if let Err(error) = crate::attachments::validate_images(text, images) {
            self.draft_error = Some(error);
            cx.notify();
            return false;
        }
        self.connection.update(cx, |connection, cx| {
            connection.send(
                ProtocolMessage::AgentPrompt {
                    pane: self.pane,
                    text: text.to_owned(),
                    images: images.to_vec(),
                },
                cx,
            );
        });
        let connection = self.connection.read(cx);
        if !connection.connected {
            return false;
        }
        let command = connection.agent_verbs_supported(self.pane) && is_zz_command(text);
        let queueing = connection.core.agent_state(self.pane).is_some_and(|state| {
            matches!(
                state.phase,
                AgentConnectionPhase::Running | AgentConnectionPhase::AwaitingPermission
            )
        });
        if !queueing && !command {
            self.transcript.local_prompt(text, images);
            self.transcript_dirty = true;
        }
        true
    }

    fn rewind_to(&mut self, message_id: &str, cx: &mut Context<Self>) {
        if self.send_prompt(&rewind_command(message_id), &[], cx) {
            cx.notify();
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        let attachments = self.attachments.clone();
        if !self.send_prompt(&text, &attachments, cx) {
            return;
        }
        self.attachments.clear();
        self.draft_error = None;
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.stick.engage(&self.scroll, cx.reduce_motion());
        cx.notify();
    }

    fn track_usage(&mut self, update: &Value) {
        if matches!(
            update.get("item").and_then(Value::as_str),
            Some("sessionReset" | "sessionSwitched")
        ) {
            self.usage = None;
            for update in update
                .get("replay")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                self.track_usage(update);
            }
        }
        if let Some(usage) = usage_from_update(update) {
            self.usage = Some(usage);
        }
    }

    fn synchronize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let events = self.connection.read(cx).agent_events.get(&self.pane);
        let newest = events
            .and_then(|events| events.last())
            .map_or(0, |(sequence, _)| *sequence);
        if newest == self.last_sequence && !self.transcript_dirty {
            self.synchronize_timeline_context(cx);
            return;
        }
        let skipped = events
            .and_then(|events| events.first())
            .is_some_and(|(first, _)| {
                self.last_sequence > 0 && *first > self.last_sequence.saturating_add(1)
            });
        if newest < self.last_sequence || skipped {
            self.transcript = Transcript::default();
            self.timeline.update(cx, |timeline, cx| {
                timeline.clear(cx);
            });
            self.last_sequence = 0;
        }
        let old_first = self.transcript.entries.first().map(AgentEntry::id);
        let old_count = self.rows.len();
        let journal = self.connection.read(cx).agent_events.get(&self.pane);
        let updates: Vec<_> = journal
            .map(|journal| {
                &journal[journal.partition_point(|(sequence, _)| *sequence <= self.last_sequence)..]
            })
            .into_iter()
            .flatten()
            .filter_map(|(sequence, bytes)| {
                serde_json::from_slice::<Value>(bytes)
                    .ok()
                    .map(|value| (*sequence, value))
            })
            .collect();
        let conversation_changed = updates.iter().any(|(_, update)| {
            matches!(
                update.get("item").and_then(Value::as_str),
                Some("sessionReset" | "sessionSwitched")
            )
        });
        if conversation_changed {
            self.permission_answered.clear();
        }
        for (sequence, update) in updates {
            self.restore_prompts(&update, window, cx);
            self.track_usage(&update);
            if matches!(
                update.get("item").and_then(Value::as_str),
                Some("sessionReset" | "sessionSwitched")
            ) {
                self.timeline.update(cx, |timeline, cx| {
                    timeline.clear(cx);
                });
            }
            self.transcript.apply(sequence, &update);
        }
        self.transcript.synchronize();
        self.last_sequence = newest;
        self.transcript_dirty = false;
        self.synchronize_timeline_context(cx);
        let folded = fold_timeline_rows(&self.transcript.entries);
        self.rows = folded.rows;
        self.entry_to_row = folded.entry_to_row;
        if conversation_changed
            || self.transcript.entries.first().map(AgentEntry::id) != old_first
            || self.rows.len() < old_count
        {
            self.scroll.reset(self.rows.len());
            self.stick.engage_now(&self.scroll, cx.reduce_motion());
        } else {
            self.scroll.remeasure_items(0..old_count);
            if self.rows.len() > old_count {
                self.scroll
                    .splice(old_count..old_count, self.rows.len() - old_count);
            }
        }
        self.stick.wake();
        self.timeline.update(cx, |timeline, cx| {
            for entry in &self.transcript.entries {
                match entry {
                    AgentEntry::User { id, markdown, .. }
                    | AgentEntry::Assistant { id, markdown, .. }
                    | AgentEntry::Reasoning { id, markdown, .. }
                    | AgentEntry::Plan { id, markdown } => {
                        timeline.synchronize_markdown(
                            *id,
                            MarkdownSlot::Body,
                            markdown.clone(),
                            cx,
                        );
                    }
                    AgentEntry::Tool(_) => {}
                }
            }
        });
    }

    fn restore_prompts(&mut self, update: &Value, window: &mut Window, cx: &mut Context<Self>) {
        let item = update.get("item").and_then(Value::as_str);
        if !matches!(item, Some("promptsReclaimed" | "promptsRestored")) {
            return;
        }
        let reclaim_id = update
            .get("reclaim_id")
            .or_else(|| update.get("reclaimId"))
            .and_then(Value::as_u64);
        if reclaim_id.is_some_and(|id| id <= self.last_reclaim_id) {
            return;
        }
        let client = self.connection.read(cx).client_instance_id();
        let prompts = restored_prompts(update, client);
        if prompts.is_empty() {
            return;
        }
        let mut draft = self.input.read(cx).value().to_string();
        for (text, images) in prompts {
            if !text.is_empty() {
                if !draft.is_empty() {
                    draft.push('\n');
                }
                draft.push_str(&text);
            }
            self.attachments.extend(images);
        }
        self.input
            .update(cx, |input, cx| input.set_value(draft, window, cx));
        if let Some(reclaim_id) = reclaim_id {
            self.last_reclaim_id = reclaim_id;
            self.connection.update(cx, |connection, cx| {
                connection.send(
                    ProtocolMessage::AgentAcknowledgePromptRestore {
                        pane: self.pane,
                        reclaim_id,
                    },
                    cx,
                );
            });
        }
        cx.notify();
    }

    fn attach_images(&mut self, images: Vec<AgentImage>, cx: &mut Context<Self>) {
        let mut combined = self.attachments.clone();
        combined.extend(images);
        match crate::attachments::validate_images(&self.input.read(cx).value(), &combined) {
            Ok(()) => {
                self.attachments = combined;
                self.draft_error = None;
            }
            Err(error) => self.draft_error = Some(error),
        }
        cx.notify();
    }

    fn pending_permissions(&self, cx: &App) -> Vec<PendingPermission> {
        let mut requests = self
            .transcript
            .model
            .permissions()
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
        if let Some(permission) = self
            .connection
            .read(cx)
            .core
            .agent_state(self.pane)
            .and_then(|state| state.pending_permission.as_ref())
            && !requests
                .iter()
                .any(|request| request.request_id == permission.request_id)
        {
            let payload = serde_json::from_str::<Value>(&permission.payload).unwrap_or_default();
            requests.push(PendingPermission {
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
        requests
    }

    fn current_permission(&self, cx: &App) -> Option<PendingPermission> {
        self.pending_permissions(cx)
            .into_iter()
            .find(|request| !self.permission_answered.contains(&request.request_id))
    }

    fn permissions(
        &self,
        permission: PendingPermission,
        index: usize,
        count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let request_id = permission.request_id;
        if permission.options.is_empty() {
            let lens = cx.entity();
            return terminal_wait_card(
                format!("agent-show-terminal-{}-{request_id}", self.pane.0),
                permission.title,
                move |_, cx| lens.update(cx, |_, cx| cx.emit(ShowTerminal)),
                cx,
            )
            .into_any_element();
        }
        let writable = self.connection.read(cx).connected
            && !self.connection.read(cx).core.attached_read_only()
            && !self.permission_answered.contains(&request_id);
        let options = permission
            .options
            .into_iter()
            .enumerate()
            .map(|(index, option)| {
                let enabled = writable;
                let id = option.id;
                permission_option(
                    format!(
                        "agent-permission-option-{}-{request_id}-{index}",
                        self.pane.0
                    ),
                    index,
                    option.name,
                    option.allow == Some(false),
                    index == self.permission_selected,
                    enabled,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.choose_permission(request_id, id.clone(), cx);
                    cx.stop_propagation();
                }))
                .on_hover(cx.listener(move |this, hovered, _, cx| {
                    if *hovered && this.permission_selected != index {
                        this.permission_selected = index;
                        cx.notify();
                    }
                }))
                .into_any_element()
            })
            .collect();
        permission_card(
            permission.title,
            (count > 1).then(|| format!("{}/{count}", index + 1).into()),
            options,
            Button::new(format!(
                "agent-permission-cancel-{}-{request_id}",
                self.pane.0
            ))
            .disabled(!writable)
            .tooltip("Cancel request")
            .on_click(cx.listener(move |this, _, _, cx| {
                this.respond_permission(request_id, None, cx);
                cx.stop_propagation();
            })),
            cx,
        )
        .into_any_element()
    }

    fn can_answer(&self, request_id: u64, cx: &App) -> bool {
        let connection = self.connection.read(cx);
        connection.connected
            && !connection.core.attached_read_only()
            && !self.permission_answered.contains(&request_id)
            && self
                .pending_permissions(cx)
                .iter()
                .any(|request| request.request_id == request_id)
    }

    fn choose_permission(&mut self, request_id: u64, option_id: String, cx: &mut Context<Self>) {
        self.respond_permission(request_id, Some(option_id), cx);
    }

    fn respond_permission(
        &mut self,
        request_id: u64,
        option_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if !self.can_answer(request_id, cx) {
            return;
        }
        self.permission_answered.insert(request_id);
        self.connection.update(cx, |connection, cx| {
            connection.send(
                ProtocolMessage::AgentRespondPermission {
                    pane: self.pane,
                    request_id,
                    option_id,
                },
                cx,
            );
        });
        cx.notify();
    }

    fn answer_question(
        &mut self,
        request_id: u64,
        answers: Vec<AgentQuestionAnswer>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.can_answer(request_id, cx) {
            return false;
        }
        self.permission_answered.insert(request_id);
        self.connection.update(cx, |connection, cx| {
            connection.send(
                ProtocolMessage::AgentAnswerQuestion {
                    pane: self.pane,
                    request_id,
                    answers,
                },
                cx,
            );
        });
        true
    }

    fn synchronize_question_card(
        &mut self,
        head: Option<&PendingPermission>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let head = head.filter(|request| !request.questions.is_empty());
        let current = self.question.as_ref().map(QuestionCardState::request_id);
        if current == head.map(|request| request.request_id) {
            return;
        }
        if self
            .question
            .as_ref()
            .is_some_and(|card| card.editing(window, cx))
        {
            self.input.read(cx).focus_handle(cx).focus(window, cx);
        }
        self.question = head.map(|request| {
            QuestionCardState::new(
                request.request_id,
                request.questions.clone(),
                window,
                cx,
                Self::on_question_input,
            )
        });
    }

    fn on_question_input(
        &mut self,
        question: usize,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(card) = self.question.as_mut() else {
            return;
        };
        match event {
            InputEvent::Change => {
                if card.input_changed(question, cx) {
                    cx.notify();
                }
            }
            InputEvent::Focus => {
                if card.card.focus(question) {
                    cx.notify();
                }
            }
            InputEvent::PressEnter { .. } => {
                let step = card.card.submit();
                self.apply_question_step(step, window, cx);
            }
            _ => {}
        }
    }

    fn question_action(
        &mut self,
        action: QuestionCardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(card) = self.question.as_mut() else {
            return;
        };
        let step = card.action(action, window, cx);
        self.apply_question_step(step, window, cx);
    }

    fn apply_question_step(
        &mut self,
        step: QuestionCardStep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(request_id) = self.question.as_ref().map(QuestionCardState::request_id) else {
            return false;
        };
        let editing = self
            .question
            .as_ref()
            .is_some_and(|card| card.editing(window, cx));
        let answered = match step {
            QuestionCardStep::Stay => return false,
            QuestionCardStep::Handled | QuestionCardStep::Other(_) => false,
            QuestionCardStep::Submit(answers) => self.answer_question(request_id, answers, cx),
            QuestionCardStep::Dismiss => {
                let open = self.can_answer(request_id, cx);
                self.respond_permission(request_id, None, cx);
                open
            }
        };
        if answered && editing {
            self.input.read(cx).focus_handle(cx).focus(window, cx);
        }
        cx.notify();
        true
    }

    fn open_output(&self, tool: &AgentToolEntry, cx: &mut Context<Self>) {
        let cwd = self.cwd(cx).unwrap_or_default();
        let Some(args) = zz_client::agent_output::output_pane_args(
            self.pane,
            &cwd,
            &tool.label,
            tool.exit_code,
            &tool_output_text(tool),
        ) else {
            return;
        };
        self.connection.update(cx, |connection, cx| {
            connection.command("split-window", args, cx);
        });
    }

    fn current_plan(&self, running: bool) -> Option<String> {
        let source = self
            .transcript
            .entries
            .iter()
            .rev()
            .find_map(|entry| match entry {
                AgentEntry::Plan { markdown, .. } => Some(markdown.full_text()),
                _ => None,
            })?;
        let (done, total, _) = zz_ui::agent::plan_progress(&source);
        (total > 0 && (done < total || running)).then_some(source)
    }

    fn task_action(&mut self, action: TaskTrayAction, cx: &mut Context<Self>) {
        match action {
            TaskTrayAction::Toggle(panel) => {
                self.tray_open = (self.tray_open != Some(panel)).then_some(panel);
            }
            TaskTrayAction::Stop(task_id) => {
                let pane = self.pane;
                self.connection.update(cx, |connection, cx| {
                    connection.send(ProtocolMessage::AgentStopTask { pane, task_id }, cx);
                });
            }
            TaskTrayAction::Reveal(tool_call_id) => {
                if let Some(row) = self.tool_row(&tool_call_id) {
                    self.stick.reveal(&self.scroll, row);
                }
            }
        }
        cx.notify();
    }

    fn tool_row(&self, tool_call_id: &str) -> Option<usize> {
        let entry = self.transcript.model.tool_entry(tool_call_id)?;
        let index = self
            .transcript
            .entries
            .iter()
            .position(|candidate| candidate.id() == entry)?;
        self.entry_to_row.get(index).copied()
    }

    fn key_down(
        &mut self,
        event: &zz_gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.completions.is_empty() {
            self.permission_key_down(event, window, cx);
            return;
        }
        let modifiers = event.keystroke.modifiers;
        match event.keystroke.key.as_str() {
            "up" if !modifiers.platform && !modifiers.alt => self.navigate_completion(-1, cx),
            "down" if !modifiers.platform && !modifiers.alt => self.navigate_completion(1, cx),
            "escape" => {
                self.completion_dismissed = true;
                self.completions = Arc::from([]);
                self.completion_selected = None;
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    fn permission_key_down(
        &mut self,
        event: &zz_gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.alt || modifiers.control || modifiers.function {
            return;
        }
        let head = self.current_permission(cx);
        self.synchronize_question_card(head.as_ref(), window, cx);
        if let Some(card) = self.question.as_mut() {
            let key = event.keystroke.key.as_str();
            if card.editing(window, cx) {
                if key == "escape" {
                    self.input.read(cx).focus_handle(cx).focus(window, cx);
                    cx.stop_propagation();
                    cx.notify();
                }
                return;
            }
            let input = self.input.read(cx);
            if !input.value().trim().is_empty() && input.focus_handle(cx).is_focused(window) {
                return;
            }
            let step = card.key(key, modifiers.shift, window, cx);
            if self.apply_question_step(step, window, cx) {
                cx.stop_propagation();
            }
            return;
        }
        let input = self.input.read(cx);
        let composer_focused = input.focus_handle(cx).is_focused(window);
        if !input.value().trim().is_empty() && composer_focused {
            return;
        }
        let Some(permission) = self.current_permission(cx) else {
            return;
        };
        let options = permission.options;
        match event.keystroke.key.as_str() {
            "enter" if composer_focused => return,
            "escape" => self.respond_permission(permission.request_id, None, cx),
            "up" if !options.is_empty() => {
                self.permission_selected = self
                    .permission_selected
                    .checked_sub(1)
                    .unwrap_or(options.len() - 1);
            }
            "down" if !options.is_empty() => {
                self.permission_selected = (self.permission_selected + 1) % options.len();
            }
            key => {
                let index = if key == "enter" {
                    Some(self.permission_selected)
                } else {
                    key.parse::<usize>()
                        .ok()
                        .filter(|digit| (1..=9).contains(digit))
                        .map(|digit| digit - 1)
                };
                let Some(option) = index.and_then(|index| options.get(index)) else {
                    return;
                };
                self.choose_permission(permission.request_id, option.id.clone(), cx);
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn render_error(&self, state: &zz_protocol::AgentPaneWire, cx: &App) -> Option<AnyElement> {
        let error = state
            .error
            .as_deref()
            .or(match &state.phase {
                AgentConnectionPhase::Failed { message } => Some(message.as_str()),
                _ => None,
            })
            .or(self.draft_error.as_deref())?;
        let failed = matches!(state.phase, AgentConnectionPhase::Failed { .. });
        Some(error_card(error, failed, cx).into_any_element())
    }

    fn drive_stick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if cx.reduce_motion() || !self.stick.wants_frame(&self.scroll) {
            return;
        }
        self.stick.arm();
        let view = cx.weak_entity();
        window.on_next_frame(move |_, cx| {
            let _ = view.update(cx, |this: &mut Self, cx| {
                if this.stick.step(&this.scroll) {
                    cx.notify();
                }
            });
        });
    }
}

pub(super) struct ShowTerminal;

impl EventEmitter<ShowTerminal> for AgentLens {}

impl Focusable for AgentLens {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

impl Render for AgentLens {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.synchronize(window, cx);
        self.synchronize_completions(cx);
        let state = self
            .connection
            .read(cx)
            .core
            .agent_state(self.pane)
            .cloned()
            .unwrap_or_default();
        let connected = self.connection.read(cx).connected;
        let writable = connected && !self.connection.read(cx).core.attached_read_only();
        let running = matches!(
            state.phase,
            AgentConnectionPhase::Running | AgentConnectionPhase::AwaitingPermission
        );
        let ready = writable
            && matches!(
                state.phase,
                AgentConnectionPhase::Ready | AgentConnectionPhase::Running
            );
        let permissions = self.pending_permissions(cx);
        self.permission_answered
            .retain(|id| permissions.iter().any(|request| request.request_id == *id));
        let permissions = permissions
            .into_iter()
            .filter(|request| !self.permission_answered.contains(&request.request_id))
            .collect::<Vec<_>>();
        let current_permission = (!permissions.is_empty()).then_some(0);
        let permission_id = current_permission.map(|index| permissions[index].request_id);
        if self.permission_request_id != permission_id {
            self.permission_request_id = permission_id;
            self.permission_selected = 0;
        }
        self.synchronize_question_card(permissions.first(), window, cx);
        if state.tasks.is_empty() && self.tray_open == Some(TrayPanel::Tasks) {
            self.tray_open = None;
        }
        self.stick.set_bottom_padding(COMPOSER_OUTER_PADDING);
        self.drive_stick(window, cx);
        let show_jump = !self.rows.is_empty() && self.stick.shows_jump_button();
        let mut prefix = Vec::new();
        if let Some(index) = current_permission {
            let request_id = permissions[index].request_id;
            if let Some(card) = self
                .question
                .as_ref()
                .filter(|card| card.request_id() == request_id)
            {
                let entity = cx.entity();
                prefix.push(
                    card.render(
                        &format!("web-agent-question-{}-{request_id}", self.pane.0),
                        (permissions.len() > 1)
                            .then(|| format!("{}/{}", index + 1, permissions.len()).into()),
                        writable,
                        move |action, window, cx| {
                            entity.update(cx, |this, cx| {
                                this.question_action(action, window, cx);
                            });
                        },
                        cx,
                    )
                    .into_any_element(),
                );
            } else {
                prefix.push(self.permissions(
                    permissions[index].clone(),
                    index,
                    permissions.len(),
                    cx,
                ));
            }
        }
        if state.queued_prompts > 0 {
            let connection = self.connection.clone();
            let pane = self.pane;
            prefix.push(
                agent_queue_chip(
                    ("agent-unqueue", self.pane.0),
                    state.queued_prompts,
                    writable,
                    move |_, cx| {
                        connection.update(cx, |connection, cx| {
                            connection.send(ProtocolMessage::AgentUnqueue { pane }, cx);
                        });
                    },
                    cx,
                )
                .into_any_element(),
            );
        }
        let plan = self.current_plan(running);
        let entity = cx.entity();
        let phase = if state.tasks.is_empty() {
            0.0
        } else {
            zz_ui::agent::presentation::spinner_phase(entity.entity_id(), cx)
        };
        prefix.extend(
            task_tray(
                &format!("web-agent-{}", self.pane.0),
                plan.as_deref(),
                &state.tasks,
                self.tray_open,
                writable,
                phase,
                move |action, _, cx| {
                    entity.update(cx, |this, cx| this.task_action(action, cx));
                },
                cx,
            )
            .map(IntoElement::into_any_element),
        );
        prefix.extend(self.render_error(&state, cx));
        prefix.extend(self.render_completions(cx));
        let has_content =
            !self.input.read(cx).value().trim().is_empty() || !self.attachments.is_empty();
        let action_kind = composer_action(running, has_content);
        let action = composer_action_button(
            ("agent-action", self.pane.0),
            action_kind,
            match action_kind {
                ComposerAction::Stop | ComposerAction::Queue => writable,
                ComposerAction::Send => ready && state.pending_permission.is_none() && has_content,
            },
        )
        .on(composer_ground(cx));
        let action = if action_kind == ComposerAction::Stop {
            let connection = self.connection.clone();
            let pane = self.pane;
            action.on_click(move |_, _, cx| {
                connection.update(cx, |connection, cx| {
                    connection.send(ProtocolMessage::AgentCancel { pane }, cx);
                });
            })
        } else {
            action.on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)))
        };
        let usage = self.usage.map(|(used, size)| {
            context_usage_meter(("agent-context-usage", self.pane.0), used, size, cx)
        });
        let pane = self.pane;
        let watch_only = self.connection.read(cx).agent_watch_only(self.pane);
        let composer = if watch_only {
            div()
                .flex()
                .flex_col()
                .w_full()
                .flex_none()
                .gap_2()
                .px_3()
                .pb_3()
                .children(prefix)
                .into_any_element()
        } else {
            AgentComposer {
                input: self.input.clone(),
                action: action.into_any_element(),
                settings: Vec::new(),
                usage,
                footer_actions: Vec::new(),
                git: state.git.as_ref().map(|git| {
                    git_summary_footer(
                        ("agent-git-summary", self.pane.0),
                        git.branch.clone().map(Into::into),
                        git.changed_files,
                        git.additions,
                        git.deletions,
                        cx,
                    )
                }),
                command_hint: active_command_hint(&self.last_input, &self.commands).map(Into::into),
                prefix,
                attachments: (!self.attachments.is_empty()).then(|| {
                    zz_ui::h_flex()
                        .w_full()
                        .flex_wrap()
                        .gap_2()
                        .px_3()
                        .pt_2()
                        .children(self.attachments.iter().enumerate().filter_map(
                            |(index, image)| {
                                let preview = preview_image(image)?;
                                let view = cx.entity();
                                Some(composer_attachment(
                                    ("web-agent-image", index),
                                    ("web-agent-remove-image", index),
                                    preview,
                                    move |_, cx| {
                                        view.update(cx, |this, cx| {
                                            if index < this.attachments.len() {
                                                this.attachments.remove(index);
                                            }
                                            this.draft_error = None;
                                            cx.notify();
                                        });
                                    },
                                    cx,
                                ))
                            },
                        ))
                        .into_any_element()
                }),
            }
            .into_any_element()
        };
        let provider = self
            .terminal(cx)
            .and_then(zz_client::window_summary::pane_agent);
        let agent = provider.map_or("the agent", AgentProvider::label);
        let empty = self.rows.is_empty().then(|| {
            let empty_message = if connected {
                match state.phase {
                    AgentConnectionPhase::Starting => format!("Reading {agent}’s transcript…"),
                    AgentConnectionPhase::Ready => match provider {
                        Some(provider) => return lens_welcome(provider, cx),
                        None => format!("{agent} is waiting for its first prompt."),
                    },
                    AgentConnectionPhase::Running => {
                        format!("Waiting for {agent}’s first update…")
                    }
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
            empty_state(
                empty_message,
                connected
                    && matches!(
                        state.phase,
                        AgentConnectionPhase::Starting | AgentConnectionPhase::Running
                    ),
                cx.entity_id(),
                cx,
            )
        });
        let timeline = AgentTimeline::new(
            self.rows.clone(),
            self.scroll.clone(),
            self.timeline.clone(),
        )
        .active_turn(running)
        .bottom_padding(COMPOSER_OUTER_PADDING)
        .open_output({
            let entity = cx.entity();
            move |tool, _, cx| entity.update(cx, |this, cx| this.open_output(tool, cx))
        });
        let timeline = if self.connection.read(cx).agent_verbs_supported(pane) {
            let rewind_view = cx.entity();
            timeline.rewind(
                writable
                    && state.phase == AgentConnectionPhase::Ready
                    && state.pending_permission.is_none(),
                move |message_id, _, cx| {
                    rewind_view.update(cx, |this, cx| this.rewind_to(message_id, cx));
                },
            )
        } else {
            timeline
        };
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .rounded_tl(self.corner_radii.top_left)
            .rounded_tr(self.corner_radii.top_right)
            .rounded_bl(self.corner_radii.bottom_left)
            .rounded_br(self.corner_radii.bottom_right)
            .child(
                div()
                    .id("web-agent-timeline")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .children(empty)
                    .when(!self.rows.is_empty(), |area| {
                        area.child(timeline).child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .right_0()
                                .bottom(px(COMPOSER_OUTER_PADDING))
                                .child(Scrollbar::vertical(&self.scroll)),
                        )
                    })
                    .when(show_jump, |area| {
                        area.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom(px(2.0 * COMPOSER_OUTER_PADDING))
                                .flex()
                                .justify_center()
                                .child(
                                    agent_jump_to_bottom_button(
                                        ("web-agent-jump-to-end", self.pane.0),
                                        cx,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.stick.engage(&this.scroll, cx.reduce_motion());
                                            cx.notify();
                                            cx.stop_propagation();
                                        },
                                    )),
                                ),
                        )
                    }),
            )
            .child(composer)
            .capture_action(cx.listener(Self::complete))
            .capture_action(cx.listener(Self::move_completion_up))
            .capture_action(cx.listener(Self::move_completion_down))
            .capture_key_down(cx.listener(Self::key_down))
    }
}

#[derive(Clone)]
struct PendingPermission {
    request_id: u64,
    title: String,
    options: Vec<PermissionChoice>,
    questions: Vec<AgentQuestion>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PermissionChoice {
    id: String,
    name: String,
    allow: Option<bool>,
}

fn permission_choices(payload: &Value) -> Vec<PermissionChoice> {
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

fn permission_questions(payload: &Value) -> Vec<AgentQuestion> {
    payload
        .get("questions")
        .and_then(|questions| serde_json::from_value(questions.clone()).ok())
        .unwrap_or_default()
}

fn decode_transcript_image(format: &str, data: Vec<u8>) -> Option<Arc<zz_gpui::Image>> {
    let image = AgentImage {
        format: format.to_owned(),
        data,
    };
    crate::attachments::validate_images("", std::slice::from_ref(&image)).ok()?;
    preview_image(&image)
}

fn preview_image(image: &AgentImage) -> Option<Arc<zz_gpui::Image>> {
    Some(Arc::new(zz_gpui::Image::from_bytes(
        zz_gpui::ImageFormat::from_mime_type(&image.format)?,
        image.data.clone(),
    )))
}

fn restored_prompts(
    update: &Value,
    client: Option<zz_protocol::ClientInstanceId>,
) -> Vec<(String, Vec<AgentImage>)> {
    update
        .get("prompts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|prompt| {
            let owner = match prompt.get("owner") {
                Some(owner) => {
                    serde_json::from_value::<zz_protocol::ClientInstanceId>(owner.clone()).ok()?
                }
                None => zz_protocol::ClientInstanceId::default(),
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

struct Transcript {
    model: AgentTranscript<Arc<zz_gpui::Image>>,
    entries: Vec<AgentEntry>,
    markdown: HashMap<u64, AgentMarkdown>,
    tool_payloads: HashMap<(u64, usize), AgentToolPayload>,
    revision: u64,
    session_reset: bool,
}

impl Default for Transcript {
    fn default() -> Self {
        Self {
            model: AgentTranscript::new(decode_transcript_image),
            entries: Vec::new(),
            markdown: HashMap::new(),
            tool_payloads: HashMap::new(),
            revision: 0,
            session_reset: false,
        }
    }
}

impl Transcript {
    fn local_prompt(&mut self, text: &str, images: &[AgentImage]) {
        self.model.begin_prompt(
            text.to_owned(),
            images.iter().filter_map(preview_image).collect(),
        );
        self.synchronize();
    }

    fn synchronize(&mut self) {
        let changed = self.model.changed_entries(self.revision);
        let indices = changed.unwrap_or_else(|| (0..self.model.entries().len()).collect());
        for index in indices {
            let source = &self.model.entries()[index];
            let parent = self
                .model
                .tool_parent(source.id())
                .and_then(|parent| self.model.tool_entry(parent));
            let entry =
                ui_entry_with_markdown(source, parent, &mut self.markdown, &mut self.tool_payloads);
            if index < self.entries.len() {
                self.entries[index] = entry;
            } else {
                self.entries.push(entry);
            }
        }
        self.revision = self.model.revision();
    }

    fn apply(&mut self, _sequence: u64, item: &Value) {
        match item.get("item").and_then(Value::as_str).unwrap_or_default() {
            "sessionReset" => {
                *self = Self::default();
                self.session_reset = true;
            }
            "sessionReady" => {
                self.session_reset = false;
                self.model.finish_replay();
                self.model.finish_turn();
            }
            "sessionSwitched" => {
                if !self.session_reset {
                    *self = Self::default();
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
        self.synchronize();
    }
}

fn streaming_markdown(
    markdown: &mut HashMap<u64, AgentMarkdown>,
    id: u64,
    source: &str,
) -> AgentMarkdown {
    let markdown = markdown
        .entry(id)
        .or_insert_with(|| AgentMarkdown::new(source));
    markdown.synchronize_append(source);
    markdown.clone()
}

fn replaced_markdown(
    markdown: &mut HashMap<u64, AgentMarkdown>,
    id: u64,
    source: &str,
) -> AgentMarkdown {
    let markdown = markdown
        .entry(id)
        .or_insert_with(|| AgentMarkdown::new(source));
    markdown.replace(source);
    markdown.clone()
}

fn ui_entry_with_markdown(
    entry: &AgentThreadEntry<Arc<zz_gpui::Image>>,
    parent: Option<u64>,
    markdown_sources: &mut HashMap<u64, AgentMarkdown>,
    tool_payloads: &mut HashMap<(u64, usize), AgentToolPayload>,
) -> AgentEntry {
    match entry {
        AgentThreadEntry::User {
            id,
            markdown,
            images,
            ..
        } => AgentEntry::User {
            id: *id,
            markdown: streaming_markdown(markdown_sources, *id, markdown),
            images: images.clone().into(),
            rewind_id: entry.rewind_id().map(zz_gpui::SharedString::from),
        },
        AgentThreadEntry::Assistant {
            id,
            markdown,
            aside,
            ..
        } => AgentEntry::Assistant {
            id: *id,
            markdown: streaming_markdown(markdown_sources, *id, markdown),
            aside: aside.map(|aside| zz_ui::agent::AgentAside {
                side: aside.side,
                reply_to: aside.reply_to,
            }),
        },
        AgentThreadEntry::Reasoning {
            id,
            label,
            markdown,
            default_expanded,
        } => AgentEntry::Reasoning {
            id: *id,
            label: zz_gpui::SharedString::from(label.clone()),
            markdown: streaming_markdown(markdown_sources, *id, markdown),
            default_expanded: *default_expanded,
        },
        AgentThreadEntry::Tool {
            id,
            kind,
            status,
            label,
            location,
            input,
            output,
            default_expanded,
            exit_code,
            ..
        } => {
            tool_payloads.retain(|(entry_id, slot), _| {
                *entry_id != *id
                    || (*slot == 0 && input.is_some())
                    || (*slot > 0 && *slot <= output.len())
            });
            AgentEntry::Tool(AgentToolEntry {
                id: *id,
                kind: match kind {
                    AgentToolKindModel::Read => AgentToolKind::Read,
                    AgentToolKindModel::Search => AgentToolKind::Search,
                    AgentToolKindModel::Edit
                    | AgentToolKindModel::Delete
                    | AgentToolKindModel::Move => AgentToolKind::Edit,
                    AgentToolKindModel::Execute => AgentToolKind::Execute,
                    AgentToolKindModel::Fetch => AgentToolKind::Fetch,
                    AgentToolKindModel::Think => AgentToolKind::Think,
                    AgentToolKindModel::SwitchMode | AgentToolKindModel::Other => {
                        AgentToolKind::Other
                    }
                },
                status: match status {
                    AgentToolStatusModel::Pending => AgentToolStatus::Pending,
                    AgentToolStatusModel::Running => AgentToolStatus::Running,
                    AgentToolStatusModel::NeedsApproval => AgentToolStatus::NeedsApproval,
                    AgentToolStatusModel::Completed => AgentToolStatus::Completed,
                    AgentToolStatusModel::Failed => AgentToolStatus::Failed,
                    AgentToolStatusModel::Canceled => AgentToolStatus::Canceled,
                },
                label: zz_gpui::SharedString::from(label.clone()),
                location: location.clone().map(zz_gpui::SharedString::from),
                input: input
                    .as_ref()
                    .map(|payload| retained_tool_payload(tool_payloads, *id, 0, payload)),
                output: output
                    .iter()
                    .enumerate()
                    .map(|(index, payload)| {
                        retained_tool_payload(tool_payloads, *id, index + 1, payload)
                    })
                    .collect::<Vec<_>>()
                    .into(),
                default_expanded: *default_expanded,
                parent,
                exit_code: *exit_code,
            })
        }
        AgentThreadEntry::Plan { id, markdown } => AgentEntry::Plan {
            id: *id,
            markdown: replaced_markdown(markdown_sources, *id, markdown),
        },
    }
}

fn retained_tool_payload(
    retained: &mut HashMap<(u64, usize), AgentToolPayload>,
    entry_id: u64,
    slot: usize,
    payload: &ToolPayload,
) -> AgentToolPayload {
    let key = (entry_id, slot);
    let next = match (retained.get(&key), payload) {
        (
            Some(AgentToolPayload::Diff {
                old: retained_old,
                new: retained_new,
                ..
            }),
            ToolPayload::Diff { path, old, new },
        ) if retained_old.is_some() == old.is_some() => {
            if let (Some(retained_old), Some(old)) = (retained_old, old) {
                retained_old.synchronize(old);
            }
            retained_new.synchronize(new);
            AgentToolPayload::Diff {
                path: path.clone().into(),
                old: retained_old.clone(),
                new: retained_new.clone(),
            }
        }
        (Some(AgentToolPayload::Text(retained)), ToolPayload::Text(text)) => {
            retained.synchronize(text);
            AgentToolPayload::Text(retained.clone())
        }
        (Some(AgentToolPayload::Json(retained)), ToolPayload::Json(text)) => {
            retained.synchronize(text);
            AgentToolPayload::Json(retained.clone())
        }
        (Some(AgentToolPayload::Terminal(retained)), ToolPayload::Terminal(text)) => {
            retained.synchronize(text);
            AgentToolPayload::Terminal(retained.clone())
        }
        (_, ToolPayload::Diff { path, old, new }) => AgentToolPayload::Diff {
            path: path.clone().into(),
            old: old.as_deref().map(AgentToolText::new),
            new: AgentToolText::new(new),
        },
        (_, ToolPayload::Text(text)) => AgentToolPayload::Text(AgentToolText::new(text)),
        (_, ToolPayload::Json(text)) => AgentToolPayload::Json(AgentToolText::new(text)),
        (_, ToolPayload::Terminal(text)) => AgentToolPayload::Terminal(AgentToolText::new(text)),
    };
    retained.insert(key, next.clone());
    next
}

fn usage_from_update(update: &Value) -> Option<(u64, u64)> {
    let update = update.get("update").unwrap_or(update);
    (update.get("sessionUpdate").and_then(Value::as_str) == Some("usage_update"))
        .then(|| Some((update.get("used")?.as_u64()?, update.get("size")?.as_u64()?)))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn replayed_image_only_messages_and_text_chunks_share_the_user_row() {
        let content =
            json!({"type":"image","mimeType":"image/png","data":BASE64.encode([1, 2, 3])});
        let mut transcript = Transcript::default();
        transcript.apply(1, &json!({"item":"sessionSwitched","replay":[
            {"sessionUpdate":"user_message_chunk","messageId":"a","content":content},
            {"sessionUpdate":"user_message_chunk","messageId":"a","content":{"type":"text","text":"Look at this"}},
            {"sessionUpdate":"user_message_chunk","messageId":"b","content":content}
        ]}));
        assert_eq!(transcript.entries.len(), 2);
        let AgentEntry::User {
            markdown, images, ..
        } = &transcript.entries[0]
        else {
            panic!()
        };
        assert_eq!(markdown.full_text(), "Look at this");
        assert_eq!(images.len(), 1);
        let AgentEntry::User {
            markdown, images, ..
        } = &transcript.entries[1]
        else {
            panic!()
        };
        assert!(markdown.trim_is_empty());
        assert_eq!(images.len(), 1);
    }

    #[test]
    fn local_image_echo_is_suppressed_but_a_later_clients_image_is_kept() {
        let image = AgentImage {
            format: "image/png".into(),
            data: vec![1, 2, 3],
        };
        let content =
            json!({"type":"image","mimeType":image.format,"data":BASE64.encode(&image.data)});
        let mut transcript = Transcript::default();
        transcript.local_prompt("", &[image]);
        transcript.apply(1, &json!({"item":"turnStarted"}));
        transcript.apply(2, &json!({"item":"update","update":{"sessionUpdate":"user_message_chunk","content":content}}));
        assert_eq!(transcript.entries.len(), 1);
        transcript.apply(3, &json!({"item":"promptFinished"}));
        transcript.apply(4, &json!({"item":"update","update":{"sessionUpdate":"user_message_chunk","content":content}}));
        assert_eq!(transcript.entries.len(), 2);
        for entry in &transcript.entries {
            let AgentEntry::User { images, .. } = entry else {
                panic!()
            };
            assert_eq!(images.len(), 1);
        }
    }

    #[test]
    fn plan_updates_replace_the_existing_row_and_keep_in_progress_markers() {
        let mut transcript = Transcript::default();
        for (sequence, status) in [(1, "in_progress"), (2, "completed")] {
            transcript.apply(sequence, &json!({"item":"update","update":{
                "sessionUpdate":"plan","entries":[{"content":"Ship shared UI","priority":"high","status":status}]
            }}));
            let AgentEntry::Plan { markdown, .. } = &transcript.entries[0] else {
                panic!()
            };
            assert_eq!(
                markdown.full_text(),
                if sequence == 1 {
                    "- [~] Ship shared UI"
                } else {
                    "- [x] Ship shared UI"
                }
            );
        }
        assert_eq!(transcript.entries.len(), 1);
    }

    #[test]
    fn structured_tool_output_permissions_and_cancellation_follow_the_shared_model() {
        let mut transcript = Transcript::default();
        transcript.apply(1, &json!({"item":"update","update":{
            "sessionUpdate":"tool_call","toolCallId":"t","title":"Run tests","kind":"execute","status":"in_progress",
            "locations":[{"path":"src/main.rs","line":12}],
            "content":[{"type":"terminal","terminalId":"pty-1"}],"rawOutput":{"ignored":true}
        }}));
        let AgentEntry::Tool(tool) = &transcript.entries[0] else {
            panic!()
        };
        assert_eq!(tool.location.as_deref(), Some("src/main.rs:12"));
        assert!(matches!(&tool.output[0], AgentToolPayload::Terminal(_)));
        transcript.apply(2, &json!({"item":"permissionRequested","request_id":7,
            "tool_call":{"toolCallId":"t"},"options":[{"optionId":"allow","name":"Allow once","kind":"allow_once"}]}));
        let AgentEntry::Tool(tool) = &transcript.entries[0] else {
            panic!()
        };
        assert_eq!(tool.status, AgentToolStatus::NeedsApproval);
        assert_eq!(transcript.model.permissions().len(), 1);
        transcript.apply(
            3,
            &json!({"item":"permissionResolved","request_id":7,"canceled":true}),
        );
        let AgentEntry::Tool(tool) = &transcript.entries[0] else {
            panic!()
        };
        assert_eq!(tool.status, AgentToolStatus::Canceled);
        assert!(transcript.model.permissions().is_empty());
    }

    #[test]
    fn question_cards_ride_the_stream_and_the_parked_state() {
        let questions = json!([{
            "id": "fruit", "question": "Which fruit?", "header": "Snack",
            "options": [{"label": "apple"}, {"label": "pear", "description": "green"}],
            "multiSelect": true, "allowOther": true,
        }]);
        let mut transcript = Transcript::default();
        transcript.apply(1, &json!({"item":"permissionRequested","request_id":9,
            "tool_call":{"toolCallId":"ask","title":"Which fruit?"},"options":[],"questions":questions}));
        let pending = transcript.model.permissions();
        let asked = &pending[0].questions[0];
        assert_eq!(asked.header.as_deref(), Some("Snack"));
        assert_eq!(asked.options[1].description.as_deref(), Some("green"));
        assert!(asked.multi_select && asked.allow_other);
        assert_eq!(
            permission_questions(
                &json!({"toolCall":{"toolCallId":"ask"},"options":[],"questions":questions})
            ),
            pending[0].questions,
            "a late client reads the same card from the parked request"
        );
        assert!(permission_questions(&json!({"toolCall":{},"options":[]})).is_empty());
    }

    #[test]
    fn subagent_steps_carry_their_agents_entry_into_the_fold() {
        let mut transcript = Transcript::default();
        transcript.apply(
            1,
            &json!({"item":"update","update":{
            "sessionUpdate":"tool_call","toolCallId":"agent-1","title":"Survey","kind":"think"}}),
        );
        transcript.apply(
            2,
            &json!({"item":"update","update":{
            "sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"meanwhile"}}}),
        );
        transcript.apply(
            3,
            &json!({"item":"update","update":{
            "sessionUpdate":"tool_call","toolCallId":"read-1","title":"Read a.rs","kind":"read",
            "_meta":{"zz":{"parent":"agent-1"}}}}),
        );
        let agent = transcript.entries[0].id();
        let parents = transcript
            .entries
            .iter()
            .map(zz_ui::agent::timeline_parent)
            .collect::<Vec<_>>();
        assert_eq!(parents, [None, None, Some(agent)]);
        assert_eq!(
            fold_timeline_rows(&transcript.entries).entry_to_row,
            [0, 0, 0],
            "the agent, the message and the step share the turn's row"
        );
    }

    #[test]
    fn a_step_that_arrives_before_its_agent_learns_its_parent_later() {
        let mut transcript = Transcript::default();
        transcript.apply(
            1,
            &json!({"item":"update","update":{
            "sessionUpdate":"tool_call","toolCallId":"read-1","title":"Read a.rs","kind":"read",
            "_meta":{"zz":{"parent":"agent-1"}}}}),
        );
        assert_eq!(zz_ui::agent::timeline_parent(&transcript.entries[0]), None);
        transcript.apply(
            2,
            &json!({"item":"update","update":{
            "sessionUpdate":"tool_call","toolCallId":"agent-1","title":"Survey","kind":"think"}}),
        );
        assert_eq!(
            zz_ui::agent::timeline_parent(&transcript.entries[0]),
            Some(transcript.entries[1].id())
        );
    }

    #[test]
    fn a_step_that_arrives_before_its_agent_leaves_its_place_for_the_agents_row() {
        let mut transcript = Transcript::default();
        transcript.apply(
            1,
            &json!({"item":"update","update":{
            "sessionUpdate":"tool_call","toolCallId":"read-1","title":"Read a.rs","kind":"read",
            "_meta":{"zz":{"parent":"agent-1"}}}}),
        );
        transcript.apply(
            2,
            &json!({"item":"update","update":{
            "sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"meanwhile"}}}),
        );
        transcript.apply(
            3,
            &json!({"item":"update","update":{
            "sessionUpdate":"tool_call","toolCallId":"agent-1","title":"Survey","kind":"think"}}),
        );
        assert_eq!(
            fold_timeline_rows(&transcript.entries).entry_to_row,
            [0, 0, 0],
            "a step that came before its agent still lands in the turn's row"
        );
    }

    #[test]
    fn prompt_rows_carry_their_rewind_id_into_the_timeline() {
        let prompt = |id: &str, text: &str| json!({"sessionUpdate":"user_message_chunk","messageId":id,"content":{"type":"text","text":text}});
        let mut transcript = Transcript::default();
        transcript.apply(
            1,
            &json!({"item":"sessionSwitched","replay":[prompt("u-1", "first"), prompt("zz-command-1-in", "/btw why")]}),
        );
        transcript.local_prompt("second", &[]);
        transcript.apply(
            2,
            &json!({"item":"update","update":prompt("u-2", "second")}),
        );
        let rewind_ids = transcript
            .entries
            .iter()
            .map(|entry| match entry {
                AgentEntry::User { rewind_id, .. } => rewind_id.as_deref(),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(rewind_ids, [Some("u-1"), None, Some("u-2")]);
    }

    #[test]
    fn failed_and_canceled_turns_settle_unfinished_tools() {
        for (outcome, expected) in [
            (
                json!({"outcome":"failed","message":"lost connection"}),
                AgentToolStatus::Failed,
            ),
            (
                json!({"outcome":"finished","stop_reason":"cancelled"}),
                AgentToolStatus::Canceled,
            ),
        ] {
            let mut transcript = Transcript::default();
            transcript.apply(1, &json!({"item":"update","update":{
                "sessionUpdate":"tool_call","toolCallId":"t","title":"Read file","kind":"read","status":"in_progress"
            }}));
            transcript.apply(2, &json!({"item":"promptFinished","outcome":outcome}));
            let AgentEntry::Tool(tool) = &transcript.entries[0] else {
                panic!()
            };
            assert_eq!(tool.status, expected);
        }
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
    }

    #[test]
    fn submitted_prompt_is_visible_without_a_provider_echo_and_not_duplicated_by_one() {
        let mut transcript = Transcript::default();
        transcript.local_prompt("hello world", &[]);
        assert_eq!(transcript.entries.len(), 1);
        transcript.apply(1, &json!({"item":"turnStarted"}));
        for (sequence, text) in [(2, "hello"), (3, " world")] {
            transcript.apply(sequence, &json!({"item":"update","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":text}}}));
        }
        assert_eq!(transcript.entries.len(), 1);
        transcript.apply(4, &json!({"item":"promptFinished"}));
        transcript.apply(5, &json!({"item":"update","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"another client"}}}));
        assert_eq!(transcript.entries.len(), 2);
    }

    #[test]
    fn restored_drafts_keep_unicode_images_and_only_the_owners_prompts() {
        let update = json!({"item":"promptsRestored","prompts":[
            {"owner":7,"text":BASE64.encode("héllo"),"images":[{"format":"image/png","data":BASE64.encode([1,2,3])}]},
            {"owner":8,"text":BASE64.encode("someone else's draft"),"images":[]},
            {"owner":0,"text":BASE64.encode("legacy"),"images":[]},
            {"owner":"bad","text":BASE64.encode("invalid owner"),"images":[]}
        ]});
        let prompts = restored_prompts(&update, Some(zz_protocol::ClientInstanceId(7)));
        assert_eq!(prompts.len(), 2);
        assert_eq!(prompts[0].0, "héllo");
        assert_eq!(prompts[0].1[0].data, [1, 2, 3]);
        assert_eq!(prompts[1].0, "legacy");
        assert_eq!(restored_prompts(&update, None).len(), 1);
    }

    #[test]
    fn chunks_keep_message_identity_and_turn_boundaries() {
        let mut transcript = Transcript::default();
        for (sequence, text) in [(1, "hello"), (2, " world")] {
            transcript.apply(sequence, &json!({"item":"update","update":{"sessionUpdate":"agent_message_chunk","messageId":"a","content":{"type":"text","text":text}}}));
        }
        assert_eq!(transcript.entries.len(), 1);
        let AgentEntry::Assistant { markdown, .. } = &transcript.entries[0] else {
            panic!()
        };
        assert_eq!(markdown.full_text(), "hello world");
        transcript.apply(3, &json!({"item":"promptFinished"}));
        transcript.apply(4, &json!({"item":"update","update":{"sessionUpdate":"agent_message_chunk","messageId":"a","content":{"type":"text","text":"next"}}}));
        assert_eq!(transcript.entries.len(), 2);
    }

    #[test]
    fn session_switch_preserves_streamed_history_and_replaces_a_previous_session() {
        let mut transcript = Transcript::default();
        transcript.apply(1, &json!({"item":"sessionReset","restoring":true}));
        transcript.apply(2, &json!({"item":"update","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"restored"}}}));
        transcript.apply(3, &json!({"item":"sessionSwitched","replay":[]}));
        let AgentEntry::Assistant { markdown, .. } = &transcript.entries[0] else {
            panic!()
        };
        assert_eq!(markdown.full_text(), "restored");
        transcript.apply(4, &json!({"item":"sessionSwitched","replay":[{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"another session"}}]}));
        assert_eq!(transcript.entries.len(), 1);
        let AgentEntry::Assistant { markdown, .. } = &transcript.entries[0] else {
            panic!()
        };
        assert_eq!(markdown.full_text(), "another session");
    }

    #[test]
    fn session_reset_discards_old_messages_and_tools() {
        let mut transcript = Transcript::default();
        transcript.apply(1, &json!({"item":"update","update":{"sessionUpdate":"tool_call","toolCallId":"t","title":"Run tests"}}));
        transcript.apply(2, &json!({"item":"update","update":{"sessionUpdate":"tool_call_update","toolCallId":"t","status":"completed"}}));
        let AgentEntry::Tool(tool) = &transcript.entries[0] else {
            panic!()
        };
        assert_eq!(tool.status, AgentToolStatus::Completed);
        transcript.apply(3, &json!({"item":"sessionReset"}));
        assert!(transcript.entries.is_empty());
        assert!(transcript.model.permissions().is_empty());
    }
}
