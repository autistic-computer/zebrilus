use std::{collections::HashSet, rc::Rc, sync::Arc};

use ala::{
    AnyElement, App, Context, Corners, Entity, FocusHandle, Focusable, IntoElement, Pixels, Render,
    SharedString, Subscription, Window, div, prelude::*,
};
use penna::{
    ActiveTheme as _, Ground,
    blocks::chat::{
        ChatComposer, Composer, composer_ground, empty_state, queue_chip, suggestion_list,
        suggestion_list_height, suggestion_row,
    },
    components::input::{
        IndentInline, InputEvent, InputState, MoveDown, MoveLeft, MoveRight, MoveUp,
    },
};
use serde_json::Value;
use zz_client::agent_ask::{DENY_PLACEHOLDER, REPLAN_PLACEHOLDER};
use zz_client::agent_completion::{
    AgentCommand, CommandCompletion, active_command_hint, command_label, composer_completions,
    meaningful_command_description, pane_commands,
};
use zz_client::agent_lens::{
    AgentEmpty, LensTranscript, PendingPermission, accepts_prompt, agent_empty, agent_placeholder,
    conversation_changed, pending_permissions, restore_reclaim_id, restored_prompts, track_usage,
    turn_running,
};
use zz_client::agent_settings::{
    self, ComposerSetter, LensSettings, composer_setter, fork_chat_command, real_session,
    source_message,
};
use zz_client::agent_transcript::{AgentThreadEntry, is_zz_command};
use zz_protocol::chat::{Ask, ChatView, ItemId, RequestId};
use zz_protocol::{
    AgentConnectionPhase, AgentImage, AgentPaneWire, AgentQuestionAnswer, PaneId, PaneSnapshot,
    ProtocolMessage,
};
use zz_ui::{
    agent::{
        ask::{
            Approval, ApprovalAction, AskChoice, AskParts, AskSubject, PromptRow, approval_subject,
        },
        chat::{ChatHost, ChatLook, ChatPane, chat_surface, shows_chat},
        composer_attachment,
        controls::{
            AccountAction, ComposerAction, ContextUsage, UsagePill, agent_model_picker,
            agent_usage_pill, composer_action, composer_action_button, git_summary_footer,
        },
        presentation::{agent_welcome, error_card},
        preview::ViewRequest,
        question::{QuestionCardAction, QuestionCardState, QuestionCardStep},
        refs::{ChatRefs, open_path},
        tasks::plan_tray,
    },
    command::{PaletteSettings, PaletteTarget, mention_picker, target_hosts},
    target::{
        RefTarget, ResolvedTarget, TargetPicker, mention_button, mention_keys, mention_layer,
    },
};

use crate::{command_palette::palette_backend, connection::Connection};

impl AgentPane {
    fn lens_settings(&self, state: &AgentPaneWire) -> Option<LensSettings> {
        self.transcript
            .lens
            .settings
            .clone()
            .or_else(|| LensSettings::from_json(&state.config_options))
    }

    fn session(&self, cx: &App) -> Option<String> {
        self.connection
            .read(cx)
            .core
            .agent_state(self.pane)?
            .session_id
            .clone()
            .filter(|session| real_session(session))
    }

    fn provider(&self, cx: &App) -> Option<zz_protocol::AgentProvider> {
        self.pane_snapshot(cx)
            .and_then(zz_client::window_summary::pane_agent)
    }

    fn forks(&self, cx: &App) -> bool {
        self.session(cx).is_some() && self.provider(cx).is_some()
    }

    fn chat_cwd(&self, cx: &App) -> String {
        self.pane_snapshot(cx)
            .map(|pane| match &pane.kind {
                zz_protocol::PaneKindSnapshot::Agent(zz_protocol::AgentDescriptor {
                    cwd: Some(cwd),
                    ..
                }) => cwd.to_string_lossy().into_owned(),
                _ => pane.summary.cwd.clone(),
            })
            .unwrap_or_default()
    }

    fn open_view(&self, request: &ViewRequest, cx: &mut Context<Self>) {
        let rows = self.float_rows.clone().and_then(|rows| rows(cx));
        let pane = self.pane;
        self.connection.update(cx, |connection, cx| {
            connection.invoke(request.command(pane, rows), cx);
        });
    }

    fn chat_refs(&self, entity: &Entity<Self>, cx: &App) -> ChatRefs {
        let view = entity.downgrade();
        let view_rows = view.clone();
        let cwd = self.chat_cwd(cx);
        let connection = self.connection.downgrade();
        let pane = self.pane;
        ChatRefs {
            hosts: target_hosts(
                &palette_backend(self.connection.clone(), PaletteSettings::default()).tree(cx),
            ),
            cwd: (!cwd.is_empty()).then_some(cwd),
            open: Arc::new(move |target, _, cx| {
                view.update(cx, |this, cx| this.open_ref(target, cx)).ok();
            }),
            view: Some(Arc::new(move |request, _, cx| {
                let rows = view_rows
                    .upgrade()
                    .and_then(|pane| pane.read(cx).float_rows.clone())
                    .and_then(|rows| rows(cx));
                connection
                    .update(cx, |connection, cx| {
                        connection.invoke(request.command(pane, rows), cx);
                    })
                    .ok();
            })),
        }
    }

    fn open_ref(&self, target: &ResolvedTarget, cx: &mut App) {
        let backend = palette_backend(self.connection.clone(), PaletteSettings::default());
        let place = match &target.target {
            RefTarget::Host => {
                backend.activate_host(target.host, cx);
                return;
            }
            RefTarget::Session(id) => PaletteTarget::Session(*id),
            RefTarget::Window(id) => PaletteTarget::Window(*id),
            RefTarget::Pane(id) => PaletteTarget::Pane(*id),
            RefTarget::Path { .. } => {
                if let Some(command) = open_path(&target.target, target.here.then_some(self.pane)) {
                    backend.execute(target.host, command, cx);
                }
                return;
            }
        };
        backend.activate(target.host, place, cx);
    }

    fn usage_pill(&self, enabled: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        let provider = self.provider(cx)?;
        let pane = self.pane;
        let connection = self.connection.clone();
        let read = self.connection.read(cx);
        let context = ContextUsage::reported(self.usage);
        let context = if shows_chat(&self.transcript.lens.chat) {
            context
        } else {
            context.in_empty_chat()
        };
        Some(agent_usage_pill(
            ("agent-account", pane.0),
            &UsagePill {
                provider,
                account: agent_settings::pane_account(read.core.snapshot(), pane),
                accounts: read.core.agent_accounts(),
                quotas: read.core.agent_quotas(),
                context,
                enabled,
            },
            move |action, _, cx| {
                connection.update(cx, |connection, cx| match action {
                    AccountAction::Refresh => connection.request_agent_accounts(cx),
                    AccountAction::Add => connection.add_agent_account(provider, cx),
                    AccountAction::Use(account) => {
                        connection.switch_agent_account(pane, account.as_deref(), cx);
                    }
                });
            },
            cx,
        ))
    }

    fn harness_controls(
        &self,
        state: &AgentPaneWire,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let account = self.usage_pill(enabled, cx);
        let Some(settings) = self.lens_settings(state) else {
            return account.into_iter().collect();
        };
        let pane = self.pane;
        let connection = self.connection.clone();
        let pick = move |option: &'static str, value: String, _: &mut Window, cx: &mut App| {
            connection.update(cx, |connection, cx| {
                connection.send(
                    ProtocolMessage::AgentSetConfigOption {
                        pane,
                        option_id: option.to_owned(),
                        value,
                    },
                    cx,
                );
            });
        };
        let picker = agent_model_picker(
            ("agent-settings", self.pane.0),
            self.provider(cx),
            &settings,
            enabled,
            pick,
            cx,
        );
        account.into_iter().chain([picker]).collect()
    }

    fn fork_from(&mut self, item: &str, cx: &mut Context<Self>) {
        let (Some(message), Some(session), Some(provider)) =
            (source_message(item), self.session(cx), self.provider(cx))
        else {
            return;
        };
        let command = fork_chat_command(
            provider,
            &session,
            &self.chat_cwd(cx),
            Some(message),
            self.transcript.lens.settings.as_ref(),
            agent_settings::pane_account(self.connection.read(cx).core.snapshot(), self.pane),
        );
        self.connection.update(cx, |connection, cx| {
            connection.invoke(command, cx);
        });
    }
}

pub(super) struct AgentPane {
    pane: PaneId,
    connection: Entity<Connection>,
    connected: bool,
    input: Entity<InputState>,
    reply: Entity<InputState>,
    commands: Arc<[AgentCommand]>,
    completions: Arc<[CommandCompletion]>,
    completion_selected: Option<usize>,
    completion_dismissed: bool,
    completion_scroll: ala::UniformListScrollHandle,
    last_input: String,
    last_cursor: usize,
    transcript: Transcript,
    last_sequence: u64,
    last_reclaim_id: u64,
    attachments: Vec<AgentImage>,
    mention: Entity<TargetPicker>,
    draft_error: Option<String>,
    permission_request_id: Option<u64>,
    permission_selected: usize,
    permission_answered: HashSet<u64>,
    question: Option<QuestionCardState>,
    plan_open: bool,
    usage: Option<(u64, u64)>,
    transcript_dirty: bool,
    corner_radii: Corners<Pixels>,
    ground: Ground,
    chat: ChatPane,
    float_rows: Option<Rc<dyn Fn(&App) -> Option<u16>>>,
    _subscriptions: Vec<Subscription>,
}

impl AgentPane {
    pub(super) fn set_float_rows_handler(
        &mut self,
        handler: impl Fn(&App) -> Option<u16> + 'static,
    ) {
        self.float_rows = Some(Rc::new(handler));
    }

    pub(super) fn new(
        pane: PaneId,
        connection: Entity<Connection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = agent_placeholder(connection.read(cx).terminal_agent(pane));
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .auto_grow(1, 8)
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
                    cx.notify();
                } else if matches!(event,
                zz_client::CoreEvent::AgentUpdates { pane: changed, .. }
                | zz_client::CoreEvent::AgentStateChanged { pane: changed, .. } if *changed == pane)
                    || matches!(event, zz_client::CoreEvent::AgentAccountsChanged)
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
        let reply = cx.new(|cx| {
            InputState::new(window, cx)
                .auto_grow(1, 8)
                .submit_on_enter(true)
                .placeholder(DENY_PLACEHOLDER)
        });
        let reply_subscription =
            cx.subscribe_in(&reply, window, |this, _, event, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => this.send_approval(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            });
        let mention = mention_picker(
            &palette_backend(connection.clone(), PaletteSettings::default()),
            input.clone(),
            window,
            cx,
        );
        mention.update(cx, |picker, _| picker.set_own_chat(Some(pane)));
        connection.update(cx, |connection, cx| {
            if connection.core.agent_accounts().is_none() {
                connection.request_agent_accounts(cx);
            }
        });
        let chat_release = ChatPane::close_on_release(window, cx, |lens: &mut Self| &mut lens.chat);
        Self {
            pane,
            connection,
            connected: false,
            input,
            reply,
            commands: Arc::from([]),
            completions: Arc::from([]),
            completion_selected: None,
            completion_dismissed: false,
            completion_scroll: ala::UniformListScrollHandle::new(),
            last_input: String::new(),
            last_cursor: 0,
            transcript: Transcript::default(),
            last_sequence: 0,
            last_reclaim_id: 0,
            attachments: Vec::new(),
            mention,
            draft_error: None,
            permission_request_id: None,
            permission_selected: 0,
            permission_answered: HashSet::new(),
            question: None,
            plan_open: false,
            usage: None,
            transcript_dirty: false,
            corner_radii: Corners::default(),
            ground: cx.theme().ground(),
            chat: ChatPane::default(),
            float_rows: None,
            _subscriptions: vec![
                observation,
                events,
                subscription,
                reply_subscription,
                input_observer,
                chat_release,
            ],
        }
    }

    fn pane_snapshot<'a>(&self, cx: &'a App) -> Option<&'a PaneSnapshot> {
        self.connection
            .read(cx)
            .core
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&self.pane))
    }

    pub(super) fn set_corner_radii(&mut self, radii: Corners<Pixels>, cx: &mut Context<Self>) {
        if self.corner_radii != radii {
            self.corner_radii = radii;
            cx.notify();
        }
    }

    pub(super) fn set_ground(&mut self, ground: Ground, cx: &mut Context<Self>) {
        if self.ground != ground {
            self.ground = ground;
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
            let settings = self
                .connection
                .read(cx)
                .core
                .agent_state(self.pane)
                .and_then(|state| self.lens_settings(state));
            composer_completions(
                &self.last_input,
                self.last_cursor,
                &self.commands,
                settings.as_ref(),
            )
            .into()
        };
        self.completion_selected = (!self.completions.is_empty()).then_some(
            self.completion_selected
                .unwrap_or_default()
                .min(self.completions.len().saturating_sub(1)),
        );
        if let Some(selected) = self.completion_selected {
            self.completion_scroll
                .scroll_to_item(selected, ala::ScrollStrategy::Nearest);
        }
        true
    }

    fn enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.read(cx).value().trim().is_empty() {
            let head = self.current_permission(cx);
            self.synchronize_question_card(head.as_ref(), window, cx);
            if let Some(card) = self.question.as_mut() {
                let step = card.card.enter();
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
        let submits = completion.submits();
        self.completion_dismissed = true;
        self.input.update(cx, |input, cx| {
            input.set_selected_range(completion.replacement, cx);
            input.replace(insertion, window, cx);
        });
        self.completions = Arc::from([]);
        self.completion_selected = None;
        self.input.read(cx).focus_handle(cx).focus(window, cx);
        if submits {
            self.submit(window, cx);
        }
        cx.notify();
    }

    fn complete(&mut self, _: &IndentInline, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.completion_selected {
            self.accept_completion(index, window, cx);
            cx.stop_propagation();
        } else {
            self.ask_arrow("tab", window, cx);
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
            .scroll_to_item(selected, ala::ScrollStrategy::Nearest);
        cx.notify();
    }

    fn move_completion_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        if self.completions.is_empty() {
            self.ask_arrow("up", window, cx);
            return;
        }
        self.navigate_completion(-1, cx);
        cx.stop_propagation();
    }

    fn move_completion_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        if self.completions.is_empty() {
            self.ask_arrow("down", window, cx);
            return;
        }
        self.navigate_completion(1, cx);
        cx.stop_propagation();
    }

    fn move_ask_left(&mut self, _: &MoveLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.ask_arrow("left", window, cx);
    }

    fn move_ask_right(&mut self, _: &MoveRight, window: &mut Window, cx: &mut Context<Self>) {
        self.ask_arrow("right", window, cx);
    }

    fn ask_arrow(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.completions.is_empty() && self.ask_key(key, false, window, cx) {
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
        let rows = ala::uniform_list(
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
                            suggestion_row(
                                format!("web-agent-completion-{}-{index}", pane.0),
                                command_label(&command.name),
                                meaningful_command_description(&command.description)
                                    .map(|description| description.to_owned().into()),
                                command.skill.then(|| "skill".into()),
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
        .h(suggestion_list_height(self.completions.len()))
        .track_scroll(&self.completion_scroll);
        Some(suggestion_list("agent-slash-suggestions", rows, cx).into_any_element())
    }

    fn send_prompt(&mut self, text: &str, images: &[AgentImage], cx: &mut Context<Self>) -> bool {
        let connection = self.connection.read(cx);
        let allowed = connection.connected
            && !connection.core.attached_read_only()
            && connection
                .core
                .agent_state(self.pane)
                .is_some_and(accepts_prompt);
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
        let queueing = connection
            .core
            .agent_state(self.pane)
            .is_some_and(|state| turn_running(&state.phase));
        if !queueing && !command {
            self.transcript.local_prompt(text, images);
            self.transcript_dirty = true;
        }
        true
    }

    fn set_from_composer(
        &mut self,
        picked: ComposerSetter<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = match picked {
            ComposerSetter::Set(option, value) => {
                let connection = self.connection.read(cx);
                if !connection.connected || connection.core.attached_read_only() {
                    return;
                }
                let (pane, value) = (self.pane, value.to_owned());
                self.connection.update(cx, |connection, cx| {
                    connection.send(
                        ProtocolMessage::AgentSetConfigOption {
                            pane,
                            option_id: option.to_owned(),
                            value,
                        },
                        cx,
                    );
                });
                self.draft_error = None;
                String::new()
            }
            ComposerSetter::Keep => {
                self.draft_error = None;
                String::new()
            }
            ComposerSetter::List(option) => format!("/{option} "),
        };
        self.input
            .update(cx, |input, cx| input.set_value(text, window, cx));
        self.last_input.clear();
        self.completions = Arc::from([]);
        self.completion_selected = None;
        self.completion_dismissed = false;
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.input.read(cx).value().to_string();
        let text = self.mention.update(cx, |picker, _| picker.raw_text(&shown));
        let settings = self
            .connection
            .read(cx)
            .core
            .agent_state(self.pane)
            .and_then(|state| self.lens_settings(state));
        if self.attachments.is_empty()
            && let Some(picked) = composer_setter(&text, settings.as_ref())
        {
            self.set_from_composer(picked, window, cx);
            return;
        }
        let attachments = self.attachments.clone();
        if !self.send_prompt(&text, &attachments, cx) {
            return;
        }
        self.attachments.clear();
        self.draft_error = None;
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.chat.follow();
        cx.notify();
    }

    fn synchronize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let events = self.connection.read(cx).agent_events.get(&self.pane);
        let newest = events
            .and_then(|events| events.last())
            .map_or(0, |(sequence, _)| *sequence);
        if newest == self.last_sequence && !self.transcript_dirty {
            return;
        }
        let skipped = events
            .and_then(|events| events.first())
            .is_some_and(|(first, _)| {
                self.last_sequence > 0 && *first > self.last_sequence.saturating_add(1)
            });
        if newest < self.last_sequence || skipped {
            self.transcript = Transcript::default();
            self.last_sequence = 0;
        }
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
        let switched = updates
            .iter()
            .any(|(_, update)| conversation_changed(update));
        if switched {
            self.permission_answered.clear();
        }
        for (sequence, update) in updates {
            self.restore_prompts(&update, window, cx);
            track_usage(&mut self.usage, &update);
            self.transcript.apply(sequence, &update);
        }
        self.last_sequence = newest;
        self.transcript_dirty = false;
    }

    fn restore_prompts(&mut self, update: &Value, window: &mut Window, cx: &mut Context<Self>) {
        let Some(reclaim_id) = restore_reclaim_id(update) else {
            return;
        };
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
        pending_permissions(
            self.transcript.lens.model.permissions(),
            self.connection
                .read(cx)
                .core
                .agent_state(self.pane)
                .and_then(|state| state.pending_permission.as_ref()),
        )
    }

    fn current_permission(&self, cx: &App) -> Option<PendingPermission> {
        self.pending_permissions(cx)
            .into_iter()
            .find(|request| !self.permission_answered.contains(&request.request_id))
    }

    fn asking_input(&self, cx: &App) -> Option<Entity<InputState>> {
        let head = self.current_permission(cx)?;
        if let Some(card) = self
            .question
            .as_ref()
            .filter(|card| card.request_id() == head.request_id)
        {
            return card.other_input(card.card.focused()).cloned();
        }
        (head.questions.is_empty()
            && head
                .options
                .iter()
                .any(|option| option.allow == Some(false)))
        .then(|| self.reply.clone())
    }

    fn row_input(&self, cx: &App) -> Entity<InputState> {
        self.asking_input(cx).unwrap_or_else(|| self.input.clone())
    }

    fn synchronize_row_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        let row = self.row_input(cx).read(cx).focus_handle(cx);
        if row.is_focused(window) {
            return;
        }
        let held = self.input.read(cx).focus_handle(cx).is_focused(window)
            || self.reply.read(cx).focus_handle(cx).is_focused(window)
            || self
                .question
                .as_ref()
                .is_some_and(|card| card.editing(window, cx));
        if held {
            row.focus(window, cx);
        }
    }

    fn ask_parts(
        &self,
        head: &PendingPermission,
        count: usize,
        writable: bool,
        cx: &mut Context<Self>,
    ) -> Option<AskParts> {
        let request_id = head.request_id;
        let counter = (count > 1).then(|| SharedString::from(format!("1 of {count}")));
        if let Some(card) = self
            .question
            .as_ref()
            .filter(|card| card.request_id() == request_id)
        {
            let entity = cx.entity();
            return Some(card.parts(
                &format!("web-agent-question-{}-{request_id}", self.pane.0),
                counter,
                writable,
                move |action, window, cx| {
                    entity.update(cx, |this, cx| this.question_action(action, window, cx));
                },
                composer_ground(self.ground, cx),
                cx,
            ));
        }
        if !head.questions.is_empty() || head.options.is_empty() {
            return None;
        }
        let cwd = zz_client::providers::home_relative(&self.chat_cwd(cx));
        let subject = approval_subject(
            Some(&self.transcript.lens.chat),
            request_id,
            &head.title,
            Some(&cwd),
        );
        let placeholder = match subject {
            AskSubject::Plan { .. } => REPLAN_PLACEHOLDER,
            AskSubject::Tool { .. } => DENY_PLACEHOLDER,
        };
        self.reply
            .update(cx, |input, cx| input.set_placeholder(placeholder, cx));
        let reply = head
            .options
            .iter()
            .any(|option| option.allow == Some(false))
            .then(|| self.reply.clone());
        let replying = reply
            .as_ref()
            .is_some_and(|reply| !reply.read(cx).value().trim().is_empty());
        let entity = cx.entity();
        Some(
            Approval {
                id: &format!("web-agent-permission-{}-{request_id}", self.pane.0),
                choices: head
                    .options
                    .iter()
                    .map(|option| AskChoice {
                        label: option.name.clone().into(),
                        rejects: option.allow == Some(false),
                    })
                    .collect(),
                subject,
                counter,
                highlighted: self.permission_selected,
                enabled: writable,
                reply,
                replying,
                owner: Some(self.pane.0),
            }
            .parts(
                move |action, window, cx| {
                    entity.update(cx, |this, cx| {
                        this.approval_action(request_id, action, window, cx);
                    });
                },
                composer_ground(self.ground, cx),
                cx,
            ),
        )
    }

    fn approval_action(
        &mut self,
        request_id: u64,
        action: ApprovalAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            ApprovalAction::Pick(index) => {
                let option = self
                    .pending_permissions(cx)
                    .into_iter()
                    .find(|permission| permission.request_id == request_id)
                    .and_then(|permission| permission.options.into_iter().nth(index));
                if let Some(option) = option {
                    self.choose_permission(request_id, option.id, cx);
                }
            }
            ApprovalAction::Send => self.send_approval(window, cx),
            ApprovalAction::Open => {
                if let Some(Ask::Plan { markdown, .. }) = self
                    .transcript
                    .lens
                    .chat
                    .request(RequestId(request_id))
                    .map(|request| request.ask.clone())
                {
                    self.open_view(
                        &ViewRequest {
                            title: "Plan".to_owned(),
                            content: zz_protocol::ViewContent::Markdown { text: markdown },
                        },
                        cx,
                    );
                }
            }
        }
    }

    fn send_approval(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(head) = self.current_permission(cx) else {
            return;
        };
        let text = self.reply.read(cx).value().trim().to_owned();
        if text.is_empty() {
            if let Some(option) = head.options.get(self.permission_selected) {
                self.choose_permission(head.request_id, option.id.clone(), cx);
            }
            return;
        }
        let Some(reject) = head
            .options
            .iter()
            .find(|option| option.allow == Some(false))
        else {
            return;
        };
        self.choose_permission(head.request_id, reject.id.clone(), cx);
        if self.send_prompt(&text, &[], cx) {
            self.reply
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.chat.follow();
        }
        cx.notify();
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
            InputEvent::PressEnter { shift: false, .. } => {
                let step = card.card.enter();
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

    fn current_plan(&self, running: bool) -> Option<String> {
        let source =
            self.transcript
                .lens
                .model
                .entries()
                .iter()
                .rev()
                .find_map(|entry| match entry {
                    AgentThreadEntry::Plan { markdown, .. } => Some(markdown.clone()),
                    _ => None,
                })?;
        let (done, total, _) = zz_ui::agent::plan_progress(&source);
        (total > 0 && (done < total || running)).then_some(source)
    }

    fn key_down(&mut self, event: &ala::KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        if self.completions.is_empty() {
            if !(modifiers.platform || modifiers.alt || modifiers.control || modifiers.function)
                && self.ask_key(event.keystroke.key.as_str(), modifiers.shift, window, cx)
            {
                cx.stop_propagation();
            }
            return;
        }
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

    fn ask_key(
        &mut self,
        key: &str,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(head) = self.current_permission(cx) else {
            return false;
        };
        self.synchronize_question_card(Some(&head), window, cx);
        let typed = !self.row_input(cx).read(cx).value().trim().is_empty();
        if typed && key != "escape" {
            return false;
        }
        if let Some(card) = self.question.as_mut() {
            let step = card.key(key, shift, window, cx);
            return self.apply_question_step(step, window, cx);
        }
        let count = head.options.len();
        match key {
            "escape" => self.respond_permission(head.request_id, None, cx),
            "up" | "down" if count > 0 => {
                self.permission_selected = if key == "up" {
                    (self.permission_selected + count - 1) % count
                } else {
                    (self.permission_selected + 1) % count
                };
                cx.notify();
            }
            _ => match key.parse::<usize>() {
                Ok(digit) if (1..=count.min(9)).contains(&digit) => {
                    let option = head.options[digit - 1].id.clone();
                    self.choose_permission(head.request_id, option, cx);
                }
                Ok(digit) if digit == count + 1 && self.asking_input(cx).is_some() => {}
                _ => return false,
            },
        }
        true
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
}

impl Focusable for AgentPane {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.row_input(cx).read(cx).focus_handle(cx)
    }
}

impl Render for AgentPane {
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
        let running = turn_running(&state.phase);
        let ready = writable
            && matches!(
                state.phase,
                AgentConnectionPhase::Ready | AgentConnectionPhase::Running
            );
        let pending = self.pending_permissions(cx);
        self.permission_answered
            .retain(|id| pending.iter().any(|request| request.request_id == *id));
        let permissions = pending
            .iter()
            .filter(|request| !self.permission_answered.contains(&request.request_id))
            .cloned()
            .collect::<Vec<_>>();
        let current_permission = (!permissions.is_empty()).then_some(0);
        let permission_id = current_permission.map(|index| permissions[index].request_id);
        if self.permission_request_id != permission_id {
            self.permission_request_id = permission_id;
            self.permission_selected = 0;
        }
        self.synchronize_question_card(permissions.first(), window, cx);
        self.synchronize_row_focus(window, cx);
        let use_chat = shows_chat(&self.transcript.lens.chat);
        let ask = permissions
            .first()
            .and_then(|head| self.ask_parts(head, permissions.len(), writable, cx));
        let asking = ask.is_some();
        let prompting = ask.as_ref().is_none_or(|ask| ask.input.is_none());
        let mut prefix = Vec::new();
        if state.queued_prompts > 0 {
            let connection = self.connection.clone();
            let pane = self.pane;
            prefix.push(
                queue_chip(
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
        if let Some(plan) = self.current_plan(running) {
            let entity = cx.entity();
            prefix.extend(
                plan_tray(
                    &format!("web-agent-{}", self.pane.0),
                    &plan,
                    self.plan_open,
                    move |_, cx| {
                        entity.update(cx, |this, cx| {
                            this.plan_open = !this.plan_open;
                            cx.notify();
                        });
                    },
                    cx,
                )
                .map(IntoElement::into_any_element),
            );
        }
        prefix.extend(self.render_error(&state, cx));
        if prompting {
            prefix.extend(self.render_completions(cx));
        }
        let over = mention_layer(&self.mention, cx).filter(|_| prompting);
        let has_content =
            !self.input.read(cx).value().trim().is_empty() || !self.attachments.is_empty();
        let action_kind = composer_action(running, has_content);
        let inside = composer_ground(self.ground, cx);
        let action = composer_action_button(
            ("agent-action", self.pane.0),
            action_kind,
            match action_kind {
                ComposerAction::Stop | ComposerAction::Queue => writable,
                ComposerAction::Send => ready && state.pending_permission.is_none() && has_content,
            },
            inside,
        );
        let action = if action_kind == ComposerAction::Stop {
            let connection = self.connection.clone();
            let pane = self.pane;
            action
                .on_click(move |_, _, cx| {
                    connection.update(cx, |connection, cx| {
                        connection.send(ProtocolMessage::AgentCancel { pane }, cx);
                    });
                })
                .into_any_element()
        } else if asking {
            div().into_any_element()
        } else {
            action
                .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)))
                .into_any_element()
        };
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
            let controls = self.harness_controls(&state, ready, cx);
            let attachments =
                (prompting && !self.attachments.is_empty()).then(|| {
                    penna::h_flex()
                        .w_full()
                        .flex_wrap()
                        .gap_2()
                        .px_2()
                        .py_1()
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
                                    inside,
                                ))
                            },
                        ))
                        .into_any_element()
                });
            let command_hint = active_command_hint(&self.last_input, &self.commands)
                .filter(|_| prompting)
                .map(Into::into);
            let add = mention_button(("agent-mention", self.pane.0), &self.mention, inside, cx)
                .into_any_element();
            let (composer, footer) = match ask {
                Some(ask) => {
                    let (composer, hints) = ask.composer(
                        ("web-agent-ask", self.pane.0),
                        window.viewport_size().height,
                        PromptRow {
                            input: self.input.clone(),
                            add,
                            action,
                            attachments,
                            command_hint,
                        },
                        self.ground,
                    );
                    (composer, vec![hints])
                }
                None => (
                    Composer {
                        input: self.input.clone(),
                        leading: Some(add),
                        usage: None,
                        action,
                        attachments,
                        command_hint,
                        ask: None,
                        secret: false,
                        ground: self.ground,
                    },
                    state
                        .git
                        .as_ref()
                        .map(|git| {
                            git_summary_footer(
                                ("agent-git-summary", self.pane.0),
                                git.branch.clone().map(Into::into),
                                git.changed_files,
                                git.additions,
                                git.deletions,
                                cx,
                            )
                        })
                        .into_iter()
                        .collect(),
                ),
            };
            ChatComposer {
                controls,
                composer,
                footer,
                prefix,
                over,
            }
            .into_any_element()
        };
        let chat = use_chat.then(|| self.render_chat(window, cx));
        let provider = self
            .pane_snapshot(cx)
            .and_then(zz_client::window_summary::pane_agent);
        let empty = (!use_chat).then(|| match agent_empty(connected, &state.phase, provider) {
            AgentEmpty::Welcome(provider) => agent_welcome(provider, cx),
            AgentEmpty::Message { text, busy } => empty_state(text, busy, cx.entity_id(), cx),
        });
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
                    .children(chat),
            )
            .child(composer)
            .capture_action(cx.listener(Self::move_ask_left))
            .capture_action(cx.listener(Self::move_ask_right))
            .capture_action(cx.listener(Self::complete))
            .capture_action(cx.listener(Self::move_completion_up))
            .capture_action(cx.listener(Self::move_completion_down))
            .map(|root| mention_keys(root, &self.mention))
            .capture_key_down(cx.listener(Self::key_down))
    }
}

impl ChatHost for AgentPane {
    fn chat_pane(&mut self) -> &mut ChatPane {
        &mut self.chat
    }

    fn chat_parts<'a>(&'a mut self, _: &'a App) -> (&'a mut ChatPane, Option<&'a ChatView>) {
        (&mut self.chat, Some(&self.transcript.lens.chat))
    }

    fn chat_read<'a>(&'a self, _: &'a App) -> (&'a ChatPane, Option<&'a ChatView>) {
        (&self.chat, Some(&self.transcript.lens.chat))
    }

    fn chat_fork(&mut self, item: &ItemId, cx: &mut Context<Self>) {
        self.fork_from(item.as_str(), cx);
    }
}

impl AgentPane {
    fn render_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let look = ChatLook {
            pane: self.pane.0,
            fork: self.forks(cx),
            refs: Some(self.chat_refs(&entity, cx)),
            vendor: self
                .provider(cx)
                .and_then(zz_client::window_summary::provider_program),
            ground: Some(self.ground),
        };
        chat_surface(self, look, window, cx)
    }
}

fn decode_transcript_image(format: &str, data: Vec<u8>) -> Option<Arc<ala::Image>> {
    let image = AgentImage {
        format: format.to_owned(),
        data,
    };
    crate::attachments::validate_images("", std::slice::from_ref(&image)).ok()?;
    preview_image(&image)
}

fn preview_image(image: &AgentImage) -> Option<Arc<ala::Image>> {
    Some(Arc::new(ala::Image::from_bytes(
        ala::ImageFormat::from_mime_type(&image.format)?,
        image.data.clone(),
    )))
}

struct Transcript {
    lens: LensTranscript<Arc<ala::Image>>,
}

impl Default for Transcript {
    fn default() -> Self {
        Self {
            lens: LensTranscript::new(decode_transcript_image),
        }
    }
}

impl Transcript {
    fn local_prompt(&mut self, text: &str, images: &[AgentImage]) {
        self.lens.model.begin_prompt(
            text.to_owned(),
            images.iter().filter_map(preview_image).collect(),
        );
    }

    fn apply(&mut self, _sequence: u64, item: &Value) {
        self.lens.apply(item);
    }

    #[cfg(test)]
    fn entries(&self) -> &[AgentThreadEntry<Arc<ala::Image>>] {
        self.lens.model.entries()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use serde_json::json;
    use zz_client::agent_lens::permission_questions;
    use zz_client::agent_transcript::{AgentToolStatusModel, ToolPayload};

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
        assert_eq!(transcript.entries().len(), 2);
        let AgentThreadEntry::User {
            markdown, images, ..
        } = &transcript.entries()[0]
        else {
            panic!()
        };
        assert_eq!(markdown, "Look at this");
        assert_eq!(images.len(), 1);
        let AgentThreadEntry::User {
            markdown, images, ..
        } = &transcript.entries()[1]
        else {
            panic!()
        };
        assert!(markdown.trim().is_empty());
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
        assert_eq!(transcript.entries().len(), 1);
        transcript.apply(3, &json!({"item":"promptFinished"}));
        transcript.apply(4, &json!({"item":"update","update":{"sessionUpdate":"user_message_chunk","content":content}}));
        assert_eq!(transcript.entries().len(), 2);
        for entry in transcript.entries() {
            let AgentThreadEntry::User { images, .. } = entry else {
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
            let AgentThreadEntry::Plan { markdown, .. } = &transcript.entries()[0] else {
                panic!()
            };
            assert_eq!(
                markdown,
                if sequence == 1 {
                    "- [~] Ship shared UI"
                } else {
                    "- [x] Ship shared UI"
                }
            );
        }
        assert_eq!(transcript.entries().len(), 1);
    }

    #[test]
    fn structured_tool_output_permissions_and_cancellation_follow_the_shared_model() {
        let mut transcript = Transcript::default();
        transcript.apply(1, &json!({"item":"update","update":{
            "sessionUpdate":"tool_call","toolCallId":"t","title":"Run tests","kind":"execute","status":"in_progress",
            "locations":[{"path":"src/main.rs","line":12}],
            "content":[{"type":"terminal","terminalId":"pty-1"}],"rawOutput":{"ignored":true}
        }}));
        let AgentThreadEntry::Tool {
            location, output, ..
        } = &transcript.entries()[0]
        else {
            panic!()
        };
        assert_eq!(location.as_deref(), Some("src/main.rs:12"));
        assert!(matches!(&output[0], ToolPayload::Terminal(_)));
        transcript.apply(2, &json!({"item":"permissionRequested","request_id":7,
            "tool_call":{"toolCallId":"t"},"options":[{"optionId":"allow","name":"Allow once","kind":"allow_once"}]}));
        let AgentThreadEntry::Tool { status, .. } = &transcript.entries()[0] else {
            panic!()
        };
        assert_eq!(*status, AgentToolStatusModel::NeedsApproval);
        assert_eq!(transcript.lens.model.permissions().len(), 1);
        transcript.apply(
            3,
            &json!({"item":"permissionResolved","request_id":7,"canceled":true}),
        );
        let AgentThreadEntry::Tool { status, .. } = &transcript.entries()[0] else {
            panic!()
        };
        assert_eq!(*status, AgentToolStatusModel::Canceled);
        assert!(transcript.lens.model.permissions().is_empty());
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
        let pending = transcript.lens.model.permissions();
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
    fn prompt_rows_carry_their_rewind_id() {
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
            .entries()
            .iter()
            .filter(|entry| matches!(entry, AgentThreadEntry::User { .. }))
            .map(AgentThreadEntry::rewind_id)
            .collect::<Vec<_>>();
        assert_eq!(rewind_ids, [Some("u-1"), None, Some("u-2")]);
    }

    #[test]
    fn failed_and_canceled_turns_settle_unfinished_tools() {
        for (outcome, expected) in [
            (
                json!({"outcome":"failed","message":"lost connection"}),
                AgentToolStatusModel::Failed,
            ),
            (
                json!({"outcome":"finished","stop_reason":"cancelled"}),
                AgentToolStatusModel::Canceled,
            ),
        ] {
            let mut transcript = Transcript::default();
            transcript.apply(1, &json!({"item":"update","update":{
                "sessionUpdate":"tool_call","toolCallId":"t","title":"Read file","kind":"read","status":"in_progress"
            }}));
            transcript.apply(2, &json!({"item":"promptFinished","outcome":outcome}));
            let AgentThreadEntry::Tool { status, .. } = &transcript.entries()[0] else {
                panic!()
            };
            assert_eq!(*status, expected);
        }
    }

    #[test]
    fn submitted_prompt_is_visible_without_a_provider_echo_and_not_duplicated_by_one() {
        let mut transcript = Transcript::default();
        transcript.local_prompt("hello world", &[]);
        assert_eq!(transcript.entries().len(), 1);
        transcript.apply(1, &json!({"item":"turnStarted"}));
        for (sequence, text) in [(2, "hello"), (3, " world")] {
            transcript.apply(sequence, &json!({"item":"update","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":text}}}));
        }
        assert_eq!(transcript.entries().len(), 1);
        transcript.apply(4, &json!({"item":"promptFinished"}));
        transcript.apply(5, &json!({"item":"update","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"another client"}}}));
        assert_eq!(transcript.entries().len(), 2);
    }

    #[test]
    fn chunks_keep_message_identity_and_turn_boundaries() {
        let mut transcript = Transcript::default();
        for (sequence, text) in [(1, "hello"), (2, " world")] {
            transcript.apply(sequence, &json!({"item":"update","update":{"sessionUpdate":"agent_message_chunk","messageId":"a","content":{"type":"text","text":text}}}));
        }
        assert_eq!(transcript.entries().len(), 1);
        let AgentThreadEntry::Assistant { markdown, .. } = &transcript.entries()[0] else {
            panic!()
        };
        assert_eq!(markdown, "hello world");
        transcript.apply(3, &json!({"item":"promptFinished"}));
        transcript.apply(4, &json!({"item":"update","update":{"sessionUpdate":"agent_message_chunk","messageId":"a","content":{"type":"text","text":"next"}}}));
        assert_eq!(transcript.entries().len(), 2);
    }

    #[test]
    fn session_switch_preserves_streamed_history_and_replaces_a_previous_session() {
        let mut transcript = Transcript::default();
        transcript.apply(1, &json!({"item":"sessionReset","restoring":true}));
        transcript.apply(2, &json!({"item":"update","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"restored"}}}));
        transcript.apply(3, &json!({"item":"sessionSwitched","replay":[]}));
        let AgentThreadEntry::Assistant { markdown, .. } = &transcript.entries()[0] else {
            panic!()
        };
        assert_eq!(markdown, "restored");
        transcript.apply(4, &json!({"item":"sessionSwitched","replay":[{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"another session"}}]}));
        assert_eq!(transcript.entries().len(), 1);
        let AgentThreadEntry::Assistant { markdown, .. } = &transcript.entries()[0] else {
            panic!()
        };
        assert_eq!(markdown, "another session");
    }

    #[test]
    fn session_reset_discards_old_messages_and_tools() {
        let mut transcript = Transcript::default();
        transcript.apply(1, &json!({"item":"update","update":{"sessionUpdate":"tool_call","toolCallId":"t","title":"Run tests"}}));
        transcript.apply(2, &json!({"item":"update","update":{"sessionUpdate":"tool_call_update","toolCallId":"t","status":"completed"}}));
        let AgentThreadEntry::Tool { status, .. } = &transcript.entries()[0] else {
            panic!()
        };
        assert_eq!(*status, AgentToolStatusModel::Completed);
        transcript.apply(3, &json!({"item":"sessionReset"}));
        assert!(transcript.entries().is_empty());
        assert!(transcript.lens.model.permissions().is_empty());
    }
}
