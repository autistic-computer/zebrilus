use penna::blocks::chat::{
    CHAT_CONTENT_MAX_WIDTH, ChatComposer, Composer, TaskTrayAction, TrayPanel, composer_add_button,
    empty_state, permission_card, permission_option, queue_chip, spinner_phase, suggestion_list,
    suggestion_list_height, suggestion_row,
};
use std::{collections::BTreeSet, sync::Arc};
use zz_ui::agent::controls::{
    ComposerAction, composer_action, composer_action_button, context_usage_meter,
    git_summary_footer,
};
#[cfg(test)]
use zz_ui::agent::controls::{context_usage_fraction, context_usage_tooltip, git_file_count_label};
#[cfg(test)]
use zz_ui::agent::presentation::MAX_RENDERED_ERROR_BYTES;
#[cfg(test)]
use zz_ui::agent::presentation::rendered_error;
use zz_ui::agent::presentation::{agent_welcome, error_card};
use zz_ui::agent::question::{QuestionCardAction, QuestionCardState, QuestionCardStep};
use zz_ui::agent::tasks::task_tray;

use ala::{
    AnyElement, Context, Entity, EntityId, FocusHandle, Focusable, Image, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, Render, ScrollStrategy, SharedString, Subscription,
    UniformListScrollHandle, Window, div, prelude::*, px, uniform_list,
};
use zz_client::agent_completion::{
    CommandCompletion, active_command_hint, command_label, composer_completions,
    meaningful_command_description, pane_commands,
};
use zz_client::agent_settings::{ComposerSetter, composer_setter};
use zz_ui::{
    command::target_picker,
    target::{TargetPicker, mention_at_caret, mention_keys},
};

use crate::command::palette::palette_backend;
use penna::{
    components::button::Button,
    components::input::{IndentInline, InputEvent, InputState, MoveDown, MoveUp},
    h_flex,
};
use zz_protocol::{AgentProvider, CommandInvocation, PaneId};
use zz_ui::agent::{composer_attachment, plan_progress};

use crate::{
    agent::attachment as agent_attachment,
    agent::controller::{
        AgentCommand, AgentConnectionState, AgentController, AgentPaneState, AgentPermissionKind,
        AgentPermissionRequest, AgentSessionCapabilities, AgentThreadEntry,
    },
    config::pane_content_radii,
    mux::client::MuxClient,
    window::corners::{WindowCorners, round_div_radii},
};

mod chat;
mod harness;

const AGENT_KEY_CONTEXT: &str = "Agent";

const fn pane_is_busy(connection: AgentConnectionState) -> bool {
    matches!(
        connection,
        AgentConnectionState::Starting
            | AgentConnectionState::Restoring
            | AgentConnectionState::Running
            | AgentConnectionState::Cancelling
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PermissionStep {
    Stay,
    Answer {
        request_id: u64,
        option_id: Option<String>,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PermissionWizard {
    page: usize,
    focused: Option<u64>,
    highlighted: usize,
    answered: BTreeSet<u64>,
}

impl PermissionWizard {
    fn sync(&mut self, requests: &[AgentPermissionRequest]) {
        self.answered
            .retain(|id| requests.iter().any(|request| request.request_id == *id));
        let pages = self.pages(requests);
        let Some(last) = pages.len().checked_sub(1) else {
            self.page = 0;
            self.focused = None;
            self.highlighted = 0;
            return;
        };
        self.page = self
            .focused
            .and_then(|id| pages.iter().position(|request| request.request_id == id))
            .unwrap_or_else(|| self.page.min(last));
        let focused = pages[self.page].request_id;
        if self.focused != Some(focused) {
            self.focused = Some(focused);
            self.highlighted = 0;
        }
    }

    fn pages<'a>(&self, requests: &'a [AgentPermissionRequest]) -> Vec<&'a AgentPermissionRequest> {
        requests
            .iter()
            .filter(|request| !self.answered.contains(&request.request_id))
            .collect()
    }

    fn current<'a>(
        &self,
        requests: &'a [AgentPermissionRequest],
    ) -> Option<&'a AgentPermissionRequest> {
        self.pages(requests).get(self.page).copied()
    }

    fn page_label(&self, requests: &[AgentPermissionRequest]) -> Option<String> {
        let count = self.pages(requests).len();
        (count > 1).then(|| format!("{}/{count}", self.page + 1))
    }

    fn highlight(&mut self, option: usize) -> bool {
        let changed = self.highlighted != option;
        self.highlighted = option;
        changed
    }

    fn answer(
        &mut self,
        requests: &[AgentPermissionRequest],
        option: Option<usize>,
    ) -> PermissionStep {
        let Some(request) = self.current(requests) else {
            return PermissionStep::Stay;
        };
        let request_id = request.request_id;
        let option_id = match option {
            Some(index) => match request.options.get(index) {
                Some(option) => Some(option.id.clone()),
                None => return PermissionStep::Stay,
            },
            None => None,
        };
        self.answered.insert(request_id);
        self.sync(requests);
        PermissionStep::Answer {
            request_id,
            option_id,
        }
    }

    fn press_digit(
        &mut self,
        requests: &[AgentPermissionRequest],
        digit: usize,
        composer_engaged: bool,
    ) -> PermissionStep {
        let Some(option) = digit.checked_sub(1) else {
            return PermissionStep::Stay;
        };
        if composer_engaged {
            return PermissionStep::Stay;
        }
        self.answer(requests, Some(option))
    }

    fn take(&mut self, requests: &[AgentPermissionRequest], request_id: u64) -> bool {
        if !self
            .pages(requests)
            .iter()
            .any(|request| request.request_id == request_id)
        {
            return false;
        }
        self.answered.insert(request_id);
        self.sync(requests);
        true
    }

    fn release(&mut self, requests: &[AgentPermissionRequest], request_id: u64) {
        self.answered.remove(&request_id);
        self.sync(requests);
    }

    fn confirm(&mut self, requests: &[AgentPermissionRequest]) -> PermissionStep {
        self.answer(requests, Some(self.highlighted))
    }

    fn cancel(&mut self, requests: &[AgentPermissionRequest]) -> PermissionStep {
        self.answer(requests, None)
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "independent visibility, completion, and history UI states"
)]
pub(crate) struct AgentView {
    pane: PaneId,
    mux: Entity<MuxClient>,
    controller: Entity<AgentController>,
    input: Entity<InputState>,
    visible: bool,
    pane_state: AgentPaneState,
    chat_revision: u64,
    completion_scroll: UniformListScrollHandle,
    submission_error: Option<Arc<str>>,
    permission_wizard: PermissionWizard,
    question: Option<QuestionCardState>,
    tray_open: Option<TrayPanel>,
    attachments: Vec<Arc<Image>>,
    mention: Entity<TargetPicker>,
    completions: Arc<[CommandCompletion]>,
    completion_selected: Option<usize>,
    completion_dismissed: bool,
    last_input: String,
    last_cursor: usize,
    window_corners: WindowCorners,
    floats: bool,
    chat: zz_ui::agent::chat::ChatPane,
    chat_inline: bool,
    _subscriptions: Vec<Subscription>,
}

impl AgentView {
    pub(crate) fn new(
        pane: PaneId,
        provider: AgentProvider,
        controller: Entity<AgentController>,
        mux: Entity<MuxClient>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .auto_grow(1, 8)
                .submit_on_enter(true)
                .placeholder(format!("Message {}…", provider.label()))
                .context_menu(true)
        });
        let input_observer = cx.observe(&input, |view, input, cx| {
            view.synchronize_input(&input, cx);
        });
        let input_subscription = cx.subscribe_in(
            &input,
            window,
            |view, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => view.enter(window, cx),
                InputEvent::Change if view.submission_error.take().is_some() => cx.notify(),
                InputEvent::PasteImages(images) => view.attach_images(images, cx),
                _ => {}
            },
        );
        let cwd = mux
            .read(cx)
            .snapshot()
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find_map(|window| window.panes.get(&pane))
            .map(|pane| match &pane.kind {
                zz_protocol::PaneKindSnapshot::Agent(zz_protocol::AgentDescriptor {
                    cwd: Some(cwd),
                    ..
                }) if pane.summary.cwd.is_empty() => cwd.clone(),
                _ => std::path::PathBuf::from(&pane.summary.cwd),
            })
            .filter(|cwd| cwd.is_absolute())
            .unwrap_or_else(|| std::path::PathBuf::from("/"));
        controller.update(cx, |controller, _| {
            controller.ensure_pane(pane, provider, cwd);
        });
        let (pane_state, chat_revision) = {
            let controller = controller.read(cx);
            (
                controller
                    .pane_state(pane)
                    .unwrap_or_else(disconnected_pane_state),
                controller.pane_chat_revision(pane),
            )
        };
        let controller_observer = cx.observe(&controller, |view, controller, cx| {
            if view.visible && view.synchronize_controller(&controller, cx) {
                cx.notify();
            }
        });
        let floats = zz_client::floating::pane_floats(&mux.read(cx).snapshot(), pane);
        let mux_observer = cx.observe(&mux, |view, mux, cx| {
            let floats = zz_client::floating::pane_floats(&mux.read(cx).snapshot(), view.pane);
            if view.floats != floats {
                view.floats = floats;
                cx.notify();
            }
        });
        let mention = target_picker(&palette_backend(mux.clone()), Some(input.clone()), cx);
        mention.update(cx, |picker, _| picker.set_own_chat(Some(pane)));
        let chat_release =
            zz_ui::agent::chat::ChatPane::close_on_release(window, cx, |view: &mut Self| {
                &mut view.chat
            });
        Self {
            pane,
            mux,
            controller,
            input,
            visible: false,
            pane_state,
            chat_revision,
            completion_scroll: UniformListScrollHandle::new(),
            submission_error: None,
            permission_wizard: PermissionWizard::default(),
            question: None,
            tray_open: None,
            attachments: Vec::new(),
            mention,
            completions: Arc::from([]),
            completion_selected: None,
            completion_dismissed: false,
            last_input: String::new(),
            last_cursor: 0,
            window_corners: WindowCorners::NONE,
            floats,
            chat: zz_ui::agent::chat::ChatPane::default(),
            chat_inline: false,
            _subscriptions: vec![
                input_observer,
                input_subscription,
                controller_observer,
                mux_observer,
                chat_release,
            ],
        }
    }

    pub(crate) fn focus(&self, cx: &ala::App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }

    pub(crate) fn set_window_corners(&mut self, corners: WindowCorners, cx: &mut Context<Self>) {
        if self.window_corners != corners {
            self.window_corners = corners;
            Self::notify_after_draw(cx);
        }
    }

    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        if visible {
            let controller = self.controller.clone();
            if self.synchronize_controller(&controller, cx) {
                Self::notify_after_draw(cx);
            }
        }
    }

    fn notify_after_draw(cx: &mut Context<Self>) {
        let view = cx.weak_entity();
        cx.defer(move |cx| {
            view.update(cx, |_, cx| cx.notify()).ok();
        });
    }

    fn on_mouse_down(&mut self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.mux.read(cx).execute(CommandInvocation::new(
            "select-pane",
            ["-t", &self.pane.to_string()],
        ));
    }

    fn synchronize_input(&mut self, input: &Entity<InputState>, cx: &mut Context<Self>) {
        let input = input.read(cx);
        let value = input.value().to_string();
        let cursor = input.cursor();
        if value == self.last_input && cursor == self.last_cursor {
            return;
        }
        self.last_input.clone_from(&value);
        self.last_cursor = cursor;
        self.completion_dismissed = false;
        let commands = pane_commands(
            &self.pane_state.available_commands,
            self.pane_state.session_capabilities.verbs,
        );
        self.recompute_completions(&commands);
        cx.notify();
    }

    fn synchronize_controller(
        &mut self,
        controller: &Entity<AgentController>,
        cx: &mut Context<Self>,
    ) -> bool {
        let controller = controller.read(cx);
        let next_state = controller
            .pane_state(self.pane)
            .unwrap_or_else(disconnected_pane_state);
        let chat_revision = controller.pane_chat_revision(self.pane);
        let state_changed = self.pane_state != next_state;
        let commands_changed = self.pane_state.provider != next_state.provider
            || self.pane_state.available_commands.as_ref()
                != next_state.available_commands.as_ref();
        let chat_changed = self.chat_revision != chat_revision;
        self.chat_revision = chat_revision;
        if state_changed {
            self.pane_state = next_state;
        }
        if commands_changed {
            let commands = pane_commands(
                &self.pane_state.available_commands,
                self.pane_state.session_capabilities.verbs,
            );
            self.recompute_completions(&commands);
        }
        state_changed || chat_changed
    }

    fn recompute_completions(&mut self, commands: &[AgentCommand]) {
        self.completions = if self.completion_dismissed {
            Arc::from([])
        } else {
            composer_completions(
                &self.last_input,
                self.last_cursor,
                commands,
                self.pane_state.settings.as_ref(),
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
                .scroll_to_item(selected, ScrollStrategy::Nearest);
        }
    }

    fn enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.completions.is_empty() {
            self.accept_selected_completion(window, cx);
            return;
        }
        if self.confirm_permission(window, cx) {
            return;
        }
        self.submit(window, cx);
    }

    fn confirm_permission(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.input.read(cx).value().trim().is_empty() || self.asks_out_of_view() {
            return false;
        }
        self.synchronize_question_card(window, cx);
        if let Some(card) = &self.question {
            let step = card.card.submit();
            self.apply_question_step(step, window, cx);
            return true;
        }
        let requests = self.pane_state.pending_permissions.clone();
        let step = self.permission_wizard.confirm(&requests);
        self.apply_permission_step(step, cx)
    }

    fn apply_permission_step(&mut self, step: PermissionStep, cx: &mut Context<Self>) -> bool {
        let PermissionStep::Answer {
            request_id,
            option_id,
        } = step
        else {
            return false;
        };
        let pane = self.pane;
        let sent = self.controller.update(cx, |controller, cx| {
            controller.respond_permission(pane, request_id, option_id, cx)
        });
        if !sent {
            self.permission_wizard.answered.remove(&request_id);
            self.permission_wizard
                .sync(&self.pane_state.pending_permissions);
        }
        cx.notify();
        true
    }

    fn composer_engaged(&self, window: &Window, cx: &ala::App) -> bool {
        let input = self.input.read(cx);
        !input.value().trim().is_empty() && input.focus_handle(cx).is_focused(window)
    }

    fn composer_has_content(&self) -> bool {
        !self.last_input.trim().is_empty() || !self.attachments.is_empty()
    }

    fn handle_permission_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.pane_state.pending_permissions.is_empty() || self.asks_out_of_view() {
            return false;
        }
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.alt || modifiers.control || modifiers.function {
            return false;
        }
        let engaged = self.composer_engaged(window, cx);
        self.synchronize_question_card(window, cx);
        if self.question.is_some() {
            return self.handle_question_key(event, engaged, window, cx);
        }
        let requests = self.pane_state.pending_permissions.clone();
        let step = match event.keystroke.key.as_str() {
            "escape" if !engaged => self.permission_wizard.cancel(&requests),
            key => match key.parse::<usize>() {
                Ok(digit) if (1..=9).contains(&digit) => self
                    .permission_wizard
                    .press_digit(&requests, digit, engaged),
                _ => PermissionStep::Stay,
            },
        };
        self.apply_permission_step(step, cx)
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
            .scroll_to_item(selected, ScrollStrategy::Nearest);
        cx.notify();
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
        self.focus(cx).focus(window, cx);
        if submits {
            self.submit(window, cx);
        }
        cx.notify();
    }

    fn accept_selected_completion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self
            .completion_selected
            .or((!self.completions.is_empty()).then_some(0))
        {
            self.accept_completion(index, window, cx);
        }
    }

    fn complete(&mut self, _: &IndentInline, window: &mut Window, cx: &mut Context<Self>) {
        if self.completions.is_empty() {
            return;
        }
        self.accept_selected_completion(window, cx);
        cx.stop_propagation();
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

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.completions.is_empty() && self.handle_permission_key(event, window, cx) {
            cx.stop_propagation();
            return;
        }
        if self.completions.is_empty() {
            return;
        }
        let modifiers = event.keystroke.modifiers;
        match event.keystroke.key.as_str() {
            "up" if !modifiers.platform && !modifiers.alt => {
                self.navigate_completion(-1, cx);
            }
            "down" if !modifiers.platform && !modifiers.alt => {
                self.navigate_completion(1, cx);
            }
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

    fn attach_images(&mut self, images: &[Arc<Image>], cx: &mut Context<Self>) {
        if !self.pane_state.session_capabilities.images {
            self.submission_error = Some(Arc::from("this agent does not accept images"));
            cx.notify();
            return;
        }
        for image in images {
            match agent_attachment::normalize(image) {
                Ok(normalized) => {
                    self.attachments.push(normalized);
                    self.submission_error = None;
                }
                Err(error) => self.submission_error = Some(Arc::from(error.as_ref())),
            }
        }
        cx.notify();
    }

    fn remove_attachment(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.attachments.len() {
            self.attachments.remove(index);
            cx.notify();
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input.read(cx).value().to_string();
        if value.trim().is_empty() && self.attachments.is_empty() {
            return;
        }
        if self.attachments.is_empty()
            && let Some(picked) = composer_setter(&value, self.pane_state.settings.as_ref())
        {
            self.set_from_composer(picked, window, cx);
            return;
        }
        let attachments = self.attachments.clone();
        let result = self.controller.update(cx, |controller, cx| {
            controller.prompt(self.pane, &value, attachments, cx)
        });
        match result {
            Ok(()) => {
                self.input.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                });
                self.attachments.clear();
                self.submission_error = None;
                self.completions = Arc::from([]);
                self.completion_selected = None;
                self.chat.follow();
            }
            Err(error) => self.submission_error = Some(error),
        }
        cx.notify();
    }

    fn set_from_composer(
        &mut self,
        picked: ComposerSetter<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = match picked {
            ComposerSetter::Set(option, value) => {
                let pane = self.pane;
                let sent = self.controller.update(cx, |controller, cx| {
                    controller.set_option(pane, option, value, cx)
                });
                if !sent {
                    self.submission_error = Some(Arc::from("the agent is not connected"));
                    cx.notify();
                    return;
                }
                self.submission_error = None;
                String::new()
            }
            ComposerSetter::Keep => {
                self.submission_error = None;
                String::new()
            }
            ComposerSetter::List(option) => format!("/{option} "),
        };
        self.input.update(cx, |input, cx| {
            input.set_value(text.clone(), window, cx);
        });
        self.last_cursor = text.len();
        self.last_input = text;
        self.completion_dismissed = false;
        let commands = pane_commands(
            &self.pane_state.available_commands,
            self.pane_state.session_capabilities.verbs,
        );
        self.recompute_completions(&commands);
        cx.notify();
    }

    fn render_attachments(&self, view: &Entity<Self>, cx: &ala::App) -> Option<impl IntoElement> {
        if self.attachments.is_empty() {
            return None;
        }
        Some(h_flex().w_full().flex_wrap().gap_2().py_1().children(
            self.attachments.iter().enumerate().map(|(index, image)| {
                let remove_view = view.clone();
                composer_attachment(
                    ("agent-composer-attachment", index),
                    format!("agent-attachment-remove-{}-{index}", self.pane.0),
                    Arc::clone(image),
                    move |_, cx| {
                        remove_view.update(cx, |view, cx| {
                            view.remove_attachment(index, cx);
                        });
                    },
                    cx,
                )
            }),
        ))
    }

    fn render_empty_state(
        state: &AgentPaneState,
        view: EntityId,
        cx: &mut ala::App,
    ) -> impl IntoElement {
        let agent = state
            .agent_name
            .as_deref()
            .unwrap_or(state.provider.label());
        let message: SharedString = match state.connection {
            AgentConnectionState::Starting => format!("Starting {agent}…").into(),
            AgentConnectionState::Restoring => format!("Resuming {agent}’s session…").into(),
            AgentConnectionState::Ready => return agent_welcome(state.provider, cx),
            AgentConnectionState::Running => format!("Waiting for {agent}’s first update…").into(),
            AgentConnectionState::Cancelling => "Cancelling the current turn…".into(),
            AgentConnectionState::Failed => format!("{agent} stopped.").into(),
            AgentConnectionState::Disconnected => "The agent is offline.".into(),
        };
        empty_state(message, pane_is_busy(state.connection), view, cx)
    }

    fn render_permission_wizard(
        &self,
        state: &AgentPaneState,
        view: &Entity<Self>,
        cx: &ala::App,
    ) -> Option<AnyElement> {
        let permission = self.permission_wizard.current(&state.pending_permissions)?;
        self.render_permission(permission, state, view, cx)
    }

    fn render_permission(
        &self,
        permission: &AgentPermissionRequest,
        state: &AgentPaneState,
        view: &Entity<Self>,
        cx: &ala::App,
    ) -> Option<AnyElement> {
        let request_id = permission.request_id;
        let current = self
            .permission_wizard
            .current(&state.pending_permissions)
            .is_some_and(|head| head.request_id == request_id);
        let counter = current
            .then(|| {
                self.permission_wizard
                    .page_label(&state.pending_permissions)
            })
            .flatten();
        if let Some(card) = self
            .question
            .as_ref()
            .filter(|card| current && card.request_id() == request_id)
        {
            let view = view.clone();
            return Some(
                card.render(
                    &format!("agent-question-{}-{request_id}", self.pane.0),
                    counter.map(Into::into),
                    true,
                    move |action, window, cx| {
                        view.update(cx, |view, cx| view.question_action(action, window, cx));
                    },
                    cx,
                )
                .into_any_element(),
            );
        }
        if !permission.questions.is_empty() {
            return None;
        }
        if permission.options.is_empty() {
            return None;
        }
        let enabled = !self.permission_wizard.answered.contains(&request_id);
        let highlighted = current.then_some(self.permission_wizard.highlighted);
        let options = permission
            .options
            .iter()
            .enumerate()
            .map(|(index, option)| {
                let answer_view = view.clone();
                let hover_view = view.clone();
                permission_option(
                    format!(
                        "agent-permission-option-{}-{request_id}-{index}",
                        self.pane.0
                    ),
                    index,
                    option.name.clone(),
                    matches!(
                        option.kind,
                        AgentPermissionKind::RejectOnce | AgentPermissionKind::RejectAlways
                    ),
                    highlighted == Some(index),
                    enabled,
                    cx,
                )
                .when(enabled, |tile| {
                    tile.on_click(move |_, _, cx| {
                        answer_view.update(cx, |view, cx| {
                            view.answer_request(request_id, Some(index), cx);
                        });
                        cx.stop_propagation();
                    })
                })
                .when(current, |tile| {
                    tile.on_hover(move |hovered, _, cx| {
                        if *hovered {
                            hover_view.update(cx, |view, cx| {
                                if view.permission_wizard.highlight(index) {
                                    cx.notify();
                                }
                            });
                        }
                    })
                })
                .into_any_element()
            })
            .collect::<Vec<_>>();
        let cancel_view = view.clone();
        Some(
            permission_card(
                permission.title.clone(),
                counter.map(Into::into),
                options,
                Button::new(format!(
                    "agent-permission-cancel-{}-{request_id}",
                    self.pane.0
                ))
                .tooltip("Cancel request")
                .on_click(move |_, _, cx| {
                    cancel_view.update(cx, |view, cx| view.answer_request(request_id, None, cx));
                    cx.stop_propagation();
                }),
                cx,
            )
            .into_any_element(),
        )
    }

    fn answer_request(&mut self, request_id: u64, option: Option<usize>, cx: &mut Context<Self>) {
        let requests = self.pane_state.pending_permissions.clone();
        let Some(request) = requests
            .iter()
            .find(|request| request.request_id == request_id)
        else {
            return;
        };
        let option_id = match option {
            Some(index) => match request.options.get(index) {
                Some(option) => Some(option.id.clone()),
                None => return,
            },
            None => None,
        };
        if self.permission_wizard.take(&requests, request_id) {
            self.apply_permission_step(
                PermissionStep::Answer {
                    request_id,
                    option_id,
                },
                cx,
            );
        }
    }

    fn render_queue_chip(&self, state: &AgentPaneState, cx: &ala::App) -> Option<impl IntoElement> {
        let queued = state.queued_prompts;
        if queued == 0 {
            return None;
        }
        let controller = self.controller.clone();
        let pane = self.pane;
        Some(queue_chip(
            format!("agent-unqueue-{}", self.pane.0),
            queued,
            true,
            move |_, cx| {
                controller.update(cx, |controller, cx| {
                    controller.unqueue_prompts(pane, cx);
                });
            },
            cx,
        ))
    }

    fn synchronize_question_card(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let head = self
            .permission_wizard
            .current(&self.pane_state.pending_permissions)
            .filter(|request| !request.questions.is_empty())
            .map(|request| (request.request_id, request.questions.clone()));
        let current = self.question.as_ref().map(QuestionCardState::request_id);
        if current == head.as_ref().map(|(request_id, _)| *request_id) {
            return;
        }
        if self
            .question
            .as_ref()
            .is_some_and(|card| card.editing(window, cx))
        {
            self.input.read(cx).focus_handle(cx).focus(window, cx);
        }
        self.question = head.map(|(request_id, questions)| {
            QuestionCardState::new(request_id, questions, window, cx, Self::on_question_input)
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

    fn handle_question_key(
        &mut self,
        event: &KeyDownEvent,
        engaged: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(card) = self.question.as_mut() else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        if card.editing(window, cx) {
            if key != "escape" {
                return false;
            }
            self.input.read(cx).focus_handle(cx).focus(window, cx);
            cx.notify();
            return true;
        }
        if engaged {
            return false;
        }
        let step = card.key(key, event.keystroke.modifiers.shift, window, cx);
        self.apply_question_step(step, window, cx)
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
        let requests = self.pane_state.pending_permissions.clone();
        let answers = match step {
            QuestionCardStep::Stay => return false,
            QuestionCardStep::Handled | QuestionCardStep::Other(_) => {
                cx.notify();
                return true;
            }
            QuestionCardStep::Submit(answers) => Some(answers),
            QuestionCardStep::Dismiss => None,
        };
        if !self.permission_wizard.take(&requests, request_id) {
            return true;
        }
        let pane = self.pane;
        let answered = self.controller.update(cx, |controller, cx| match answers {
            Some(answers) => controller.answer_question(pane, request_id, answers, cx),
            None => controller.respond_permission(pane, request_id, None, cx),
        });
        if !answered {
            self.permission_wizard.release(&requests, request_id);
        }
        if answered && editing {
            self.input.read(cx).focus_handle(cx).focus(window, cx);
        }
        cx.notify();
        true
    }

    fn task_action(&mut self, action: TaskTrayAction, cx: &mut Context<Self>) {
        match action {
            TaskTrayAction::Toggle(panel) => {
                self.tray_open = (self.tray_open != Some(panel)).then_some(panel);
            }
            TaskTrayAction::Stop(task_id) => {
                let pane = self.pane;
                self.controller.update(cx, |controller, cx| {
                    controller.stop_task(pane, task_id.to_string(), cx);
                });
            }
            TaskTrayAction::Reveal(tool_call_id) => self.reveal_tool(&tool_call_id, cx),
        }
        cx.notify();
    }

    fn reveal_tool(&mut self, tool_call_id: &str, cx: &mut Context<Self>) {
        let Some(chat) = self.controller.read(cx).pane_chat(self.pane) else {
            return;
        };
        self.chat
            .reveal(chat, &zz_protocol::chat::ItemId::new(tool_call_id));
    }

    fn current_plan(&self, state: &AgentPaneState, cx: &ala::App) -> Option<String> {
        let (entries, _, _) = self.controller.read(cx).pane_entries(self.pane)?;
        let source = entries.iter().rev().find_map(|entry| match entry {
            AgentThreadEntry::Plan { markdown, .. } => Some(markdown.clone()),
            _ => None,
        })?;
        let (done, total, _) = plan_progress(&source);
        (total > 0 && (done < total || state.connection.has_active_turn())).then_some(source)
    }

    fn render_task_tray(
        &self,
        state: &AgentPaneState,
        view: &Entity<Self>,
        cx: &mut ala::App,
    ) -> Option<impl IntoElement> {
        let phase = if state.tasks.is_empty() {
            0.0
        } else {
            spinner_phase(view.entity_id(), cx)
        };
        let plan = self.current_plan(state, cx);
        let view = view.clone();
        task_tray(
            &format!("agent-{}", self.pane.0),
            plan.as_deref(),
            &state.tasks,
            self.tray_open,
            true,
            phase,
            move |action, _, cx| {
                view.update(cx, |view, cx| view.task_action(action, cx));
            },
            cx,
        )
    }

    fn render_error(&self, state: &AgentPaneState, cx: &ala::App) -> Option<impl IntoElement> {
        let runtime_error = state.error.clone();
        let error = runtime_error
            .clone()
            .or_else(|| self.submission_error.clone())?;
        let stopped = runtime_error.is_some() && state.connection == AgentConnectionState::Failed;
        Some(error_card(&error, stopped, cx))
    }

    fn render_completions(&self, view: &Entity<Self>, cx: &ala::App) -> Option<ala::AnyElement> {
        if self.completions.is_empty() {
            return None;
        }
        let selected = self.completion_selected;
        let pane = self.pane;
        let completions = Arc::clone(&self.completions);
        let rows_view = view.clone();
        let rows = uniform_list(
            ("agent-completion-rows", pane.0),
            completions.len(),
            move |range, _, cx| {
                range
                    .filter_map(|index| {
                        let completion = completions.get(index)?.clone();
                        let command = completion.command.clone();
                        let label = command_label(&command.name);
                        let description = meaningful_command_description(&command.description)
                            .map(ToOwned::to_owned);
                        let is_selected = selected == Some(index);
                        let hover_view = rows_view.clone();
                        let click_view = rows_view.clone();
                        Some(
                            suggestion_row(
                                format!("agent-completion-{}-{index}", pane.0),
                                label,
                                description.map(Into::into),
                                command.skill.then(|| "skill".into()),
                                is_selected,
                                cx,
                            )
                            .on_hover(move |hovered, _, cx| {
                                if *hovered {
                                    hover_view.update(cx, |view, cx| {
                                        if view.completion_selected != Some(index) {
                                            view.completion_selected = Some(index);
                                            cx.notify();
                                        }
                                    });
                                }
                            })
                            .on_click(move |_, window, cx| {
                                click_view.update(cx, |view, cx| {
                                    view.accept_completion(index, window, cx);
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

    fn render_composer(
        &self,
        state: &AgentPaneState,
        view: &Entity<Self>,
        cx: &mut ala::App,
    ) -> impl IntoElement {
        let can_submit = state.connection.accepts_prompt();
        let action_kind = composer_action(
            state.connection.has_active_turn(),
            self.composer_has_content(),
        );
        let action = composer_action_button(
            format!("agent-action-{}", self.pane.0),
            action_kind,
            action_kind != ComposerAction::Send || (can_submit && self.composer_has_content()),
            cx,
        );
        let action = if action_kind == ComposerAction::Stop {
            let controller = self.controller.clone();
            let pane = self.pane;
            action.on_click(move |_, _, cx| {
                controller.update(cx, |controller, cx| controller.cancel(pane, cx));
            })
        } else {
            let submit_view = view.clone();
            action.on_click(move |_, window, cx| {
                submit_view.update(cx, |view, cx| view.submit(window, cx));
            })
        };
        let usage = state.usage.map(|(used, size)| {
            context_usage_meter(("agent-context-usage", self.pane.0), used, size, cx)
        });
        let git = state.git.as_ref().map(|git| {
            git_summary_footer(
                ("agent-git-summary", self.pane.0),
                git.branch.clone().map(Into::into),
                git.changed_files,
                git.additions,
                git.deletions,
                cx,
            )
        });
        let command_hint = active_command_hint(
            &self.last_input,
            &pane_commands(&state.available_commands, state.session_capabilities.verbs),
        );
        let completions = self.render_completions(view, cx);
        let add = {
            let input = self.input.clone();
            composer_add_button(
                format!("agent-mention-{}", self.pane.0),
                "Mention a chat, host, session or file",
                cx,
            )
            .on_click(move |_, window, cx| mention_at_caret(&input, "@", window, cx))
        };
        let mention = self
            .mention
            .read(cx)
            .is_open()
            .then(|| self.mention.clone().into_any_element());
        let (directory, controls) = self.render_harness_controls(state, view, cx);
        ChatComposer {
            controls,
            composer: Composer {
                input: self.input.clone(),
                leading: Some(add.into_any_element()),
                usage: usage.map(IntoElement::into_any_element),
                action: action.into_any_element(),
                attachments: self
                    .render_attachments(view, cx)
                    .map(IntoElement::into_any_element),
                command_hint: command_hint.map(Into::into),
                ground: None,
            },
            footer: directory
                .into_iter()
                .chain(git.map(IntoElement::into_any_element))
                .collect(),
            prefix: self
                .render_permission_wizard(state, view, cx)
                .filter(|_| !self.chat_inline)
                .into_iter()
                .map(IntoElement::into_any_element)
                .chain(
                    self.render_queue_chip(state, cx)
                        .map(IntoElement::into_any_element),
                )
                .chain(
                    self.render_task_tray(state, view, cx)
                        .map(IntoElement::into_any_element),
                )
                .chain(
                    self.render_error(state, cx)
                        .map(IntoElement::into_any_element),
                )
                .chain(completions)
                .chain(mention)
                .collect(),
        }
    }
}

impl Focusable for AgentView {
    fn focus_handle(&self, cx: &ala::App) -> FocusHandle {
        self.focus(cx)
    }
}

impl AgentView {
    fn drain_pending_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.pane;
        let (pending, images) = self.controller.update(cx, |controller, _| {
            (
                controller.take_pending_composer(pane),
                controller.take_pending_images(pane),
            )
        });
        if pending.is_none() && images.is_empty() {
            return;
        }
        self.attachments.extend(images);
        if let Some(pending) = pending {
            self.input.update(cx, |input, cx| {
                let current = input.value().to_string();
                let value = if current.trim().is_empty() {
                    pending
                } else {
                    format!("{}\n{pending}", current.trim_end_matches('\n'))
                };
                input.set_value(value, window, cx);
            });
        }
        self.submission_error = None;
        self.chat.follow();
    }
}

impl Render for AgentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.drain_pending_composer(window, cx);
        self.permission_wizard
            .sync(&self.pane_state.pending_permissions);
        self.synchronize_question_card(window, cx);
        if self.pane_state.tasks.is_empty() && self.tray_open == Some(TrayPanel::Tasks) {
            self.tray_open = None;
        }
        let state = self.pane_state.clone();
        let (use_chat, inline) =
            self.controller
                .read(cx)
                .pane_chat(self.pane)
                .map_or((false, false), |chat| {
                    (
                        zz_ui::agent::chat::shows_chat(chat),
                        self.asks_inline(chat, &state),
                    )
                });
        self.chat_inline = inline;
        let floats = self.floats;
        let view = cx.entity();
        let input_focus = self.focus(cx);
        let chat_body = use_chat.then(|| self.render_chat(window, cx));
        let root = div()
            .id(("agent-pane", self.pane.0))
            .key_context(AGENT_KEY_CONTEXT)
            .track_focus(&input_focus)
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .capture_action(cx.listener(Self::move_completion_up))
            .capture_action(cx.listener(Self::move_completion_down))
            .capture_action(cx.listener(Self::complete))
            .map(|root| mention_keys(root, &self.mention))
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                div()
                    .id(("agent-thread-scroll", self.pane.0))
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .when_some(chat_body, ParentElement::child)
                    .when(!use_chat, |this| {
                        this.child(
                            div().size_full().px_3().child(
                                div()
                                    .size_full()
                                    .max_w(px(CHAT_CONTENT_MAX_WIDTH))
                                    .mx_auto()
                                    .child(Self::render_empty_state(&state, cx.entity_id(), cx)),
                            ),
                        )
                    }),
            )
            .child(self.render_composer(&state, &view, cx));
        round_div_radii(root, pane_content_radii(cx, self.window_corners))
            .when(!floats, |root| root.rounded_tl(px(0.0)).rounded_tr(px(0.0)))
    }
}

fn disconnected_pane_state() -> AgentPaneState {
    AgentPaneState {
        provider: AgentProvider::Codex,
        connection: AgentConnectionState::Disconnected,
        pending_permissions: Arc::from([]),
        error: Some(Arc::from("the agent is not connected")),
        agent_name: None,
        cwd: std::path::PathBuf::from("/"),
        session_id: None,
        session_capabilities: AgentSessionCapabilities::default(),
        available_commands: Arc::from([]),
        usage: None,
        git: None,
        tasks: Arc::from([]),
        pending_composer: None,
        queued_prompts: 0,
        settings: None,
    }
}

#[cfg(test)]
mod completion_tests {
    #[cfg(not(target_os = "macos"))]
    use std::{cell::RefCell, rc::Rc};

    #[cfg(not(target_os = "macos"))]
    use ala::{TestAppContext, VisualTestContext};
    #[cfg(not(target_os = "macos"))]
    use penna::Root;
    #[cfg(not(target_os = "macos"))]
    use zz_daemon_client::DaemonError;

    use super::*;

    #[cfg(not(target_os = "macos"))]
    fn command(name: &str) -> AgentCommand {
        AgentCommand {
            name: name.to_owned(),
            description: format!("Run {name}"),
            ..AgentCommand::default()
        }
    }

    fn permission(request_id: u64) -> AgentPermissionRequest {
        use crate::agent::controller::AgentPermissionOption;

        AgentPermissionRequest {
            request_id,
            tool_call_id: format!("tool-{request_id}"),
            title: format!("Run command {request_id}"),
            options: vec![
                AgentPermissionOption {
                    id: format!("allow-{request_id}"),
                    name: "Allow once".to_owned(),
                    kind: AgentPermissionKind::AllowOnce,
                },
                AgentPermissionOption {
                    id: format!("reject-{request_id}"),
                    name: "Reject".to_owned(),
                    kind: AgentPermissionKind::RejectOnce,
                },
            ],
            questions: Vec::new(),
        }
    }

    #[test]
    fn a_card_takes_its_own_request_off_the_wizard_whatever_page_is_focused() {
        let requests = [permission(1), permission(2)];
        let mut wizard = PermissionWizard::default();
        wizard.sync(&requests);
        assert!(wizard.take(&requests, 2));
        assert_eq!(wizard.current(&requests).map(|r| r.request_id), Some(1));
        assert!(
            !wizard.take(&requests, 2),
            "an answered request is taken once"
        );
        assert!(!wizard.take(&requests, 9));
        wizard.release(&requests, 2);
        assert_eq!(wizard.page_label(&requests).as_deref(), Some("1/2"));
    }

    #[test]
    fn composer_action_follows_the_live_turn_and_the_draft() {
        assert_eq!(composer_action(false, false), ComposerAction::Send);
        assert_eq!(composer_action(false, true), ComposerAction::Send);
        assert_eq!(composer_action(true, true), ComposerAction::Queue);
        assert_eq!(composer_action(true, false), ComposerAction::Stop);
    }

    #[test]
    fn permission_wizard_answers_the_focused_page_then_advances() {
        let requests = vec![permission(1), permission(2)];
        let mut wizard = PermissionWizard::default();
        wizard.sync(&requests);

        assert_eq!(wizard.page_label(&requests).as_deref(), Some("1/2"));
        assert_eq!(
            wizard.answer(&requests, Some(1)),
            PermissionStep::Answer {
                request_id: 1,
                option_id: Some("reject-1".to_owned()),
            }
        );
        assert_eq!(
            wizard.current(&requests).map(|request| request.request_id),
            Some(2)
        );
        assert_eq!(wizard.page_label(&requests), None);
    }

    #[test]
    fn permission_wizard_latches_an_answered_page_until_the_controller_drops_it() {
        let requests = vec![permission(1), permission(2)];
        let mut wizard = PermissionWizard::default();
        wizard.sync(&requests);
        wizard.answer(&requests, Some(0));

        assert_eq!(
            wizard.answer(&requests, Some(0)),
            PermissionStep::Answer {
                request_id: 2,
                option_id: Some("allow-2".to_owned()),
            }
        );
        assert_eq!(wizard.current(&requests), None);

        wizard.sync(&[]);

        assert!(wizard.answered.is_empty());
    }

    #[test]
    fn permission_wizard_keeps_its_page_when_another_request_resolves_elsewhere() {
        let requests = vec![permission(1), permission(2), permission(3)];
        let mut wizard = PermissionWizard::default();
        wizard.sync(&requests);
        wizard.answer(&requests, Some(0));
        wizard.highlight(1);

        let remaining = vec![permission(2), permission(3)];
        wizard.sync(&remaining);

        assert_eq!(
            wizard.current(&remaining).map(|request| request.request_id),
            Some(2)
        );
        assert_eq!(wizard.highlighted, 1);

        let auto_approved = vec![permission(3)];
        wizard.sync(&auto_approved);

        assert_eq!(
            wizard
                .current(&auto_approved)
                .map(|request| request.request_id),
            Some(3)
        );
        assert_eq!(wizard.highlighted, 0);
    }

    #[test]
    fn permission_digits_defer_to_a_composer_the_user_is_typing_into() {
        let requests = vec![permission(1)];
        let mut wizard = PermissionWizard::default();
        wizard.sync(&requests);

        assert_eq!(wizard.press_digit(&requests, 1, true), PermissionStep::Stay);
        assert_eq!(
            wizard.press_digit(&requests, 0, false),
            PermissionStep::Stay
        );
        assert_eq!(
            wizard.press_digit(&requests, 7, false),
            PermissionStep::Stay
        );
        assert_eq!(
            wizard.press_digit(&requests, 2, false),
            PermissionStep::Answer {
                request_id: 1,
                option_id: Some("reject-1".to_owned()),
            }
        );
    }

    #[test]
    fn permission_wizard_confirms_the_highlight_and_cancels_the_page() {
        let requests = vec![permission(1), permission(2)];
        let mut wizard = PermissionWizard::default();
        wizard.sync(&requests);
        wizard.highlight(1);

        assert_eq!(
            wizard.confirm(&requests),
            PermissionStep::Answer {
                request_id: 1,
                option_id: Some("reject-1".to_owned()),
            }
        );
        assert_eq!(
            wizard.cancel(&requests),
            PermissionStep::Answer {
                request_id: 2,
                option_id: None,
            }
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[ala::test]
    fn arrow_actions_navigate_completions_before_the_multiline_input(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let view_slot = Rc::new(RefCell::new(None));
        let captured = Rc::clone(&view_slot);
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .auto_grow(2, 8)
                    .default_value("/")
            });
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(DaemonError::Thread("test client".to_owned())),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            let controller = cx.new(|_| AgentController::new());
            let mention = target_picker(&palette_backend(mux.clone()), Some(input.clone()), cx);
            let view = cx.new(|_| AgentView {
                pane: PaneId(7),
                mux,
                controller,
                input,
                visible: false,
                pane_state: disconnected_pane_state(),
                chat_revision: 0,
                completion_scroll: UniformListScrollHandle::new(),
                submission_error: None,
                permission_wizard: PermissionWizard::default(),
                question: None,
                tray_open: None,
                attachments: Vec::new(),
                mention,
                completions: vec![
                    CommandCompletion {
                        command: command("first"),
                        replacement: 0..1,
                    },
                    CommandCompletion {
                        command: command("second"),
                        replacement: 0..1,
                    },
                ]
                .into(),
                completion_selected: Some(0),
                completion_dismissed: false,
                last_input: "/".to_owned(),
                last_cursor: 1,
                window_corners: WindowCorners::NONE,
                floats: false,
                chat: zz_ui::agent::chat::ChatPane::default(),
                chat_inline: false,
                _subscriptions: Vec::new(),
            });
            view.read(cx).focus(cx).focus(window, cx);
            captured.replace(Some(view.clone()));
            Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        let view = view_slot.borrow().clone().expect("captured agent view");

        cx.simulate_keystrokes("down");
        assert_eq!(
            cx.update(|_, cx| view.read(cx).completion_selected),
            Some(1)
        );

        cx.simulate_keystrokes("up");
        assert_eq!(
            cx.update(|_, cx| view.read(cx).completion_selected),
            Some(0)
        );

        cx.simulate_keystrokes("tab");
        assert_eq!(
            cx.update(|_, cx| view.read(cx).input.read(cx).value().to_string()),
            "/first "
        );
        assert!(cx.update(|_, cx| view.read(cx).completions.is_empty()));
    }

    #[cfg(not(target_os = "macos"))]
    type RecordedRequests = Rc<RefCell<Vec<(PaneId, crate::mux::client::AgentRequest)>>>;

    #[cfg(not(target_os = "macos"))]
    fn wired_view(
        cx: &mut TestAppContext,
        pane: PaneId,
    ) -> (
        Entity<AgentView>,
        Entity<AgentController>,
        RecordedRequests,
        &mut VisualTestContext,
    ) {
        cx.update(zz_ui::init);
        let slot = Rc::new(RefCell::new(None));
        let captured = Rc::clone(&slot);
        let (_, cx) = cx.add_window_view(move |window, cx| {
            crate::config::set_fleet_hosts_for_test(Vec::new(), cx);
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(DaemonError::Thread("test client".to_owned())),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            let sink = mux.update(cx, |mux, _| {
                mux.set_agent_client_instance_id_for_test(zz_protocol::ClientInstanceId(1));
                mux.record_agent_requests_for_test()
            });
            let controller = cx.new(|_| AgentController::new());
            controller.update(cx, |controller, _| controller.attach_mux(mux.clone()));
            let view = cx.new(|cx| {
                let mut view = AgentView::new(
                    pane,
                    AgentProvider::ClaudeCode,
                    controller.clone(),
                    mux,
                    window,
                    cx,
                );
                view.set_visible(true, cx);
                view
            });
            view.read(cx).focus(cx).focus(window, cx);
            captured.replace(Some((view.clone(), controller, sink)));
            Root::new(view, window, cx)
        });
        let (view, controller, sink) = slot.borrow_mut().take().expect("captured agent view");
        (view, controller, sink, cx)
    }

    #[cfg(not(target_os = "macos"))]
    fn ask(
        cx: &mut VisualTestContext,
        controller: &Entity<AgentController>,
        pane: PaneId,
        request_id: u64,
        questions: Vec<zz_protocol::agent_stream::AgentQuestion>,
    ) {
        cx.update(|_, cx| {
            controller.update(cx, |controller, cx| {
                controller.apply_stream_items(
                    pane,
                    vec![zz_daemon::AgentStreamItem {
                        seq: request_id,
                        payload: zz_daemon::AgentStreamPayload::PermissionRequested {
                            request_id,
                            tool_call: serde_json::json!({
                                "toolCallId": format!("ask-{request_id}"),
                                "title": "Claude has questions",
                            }),
                            options: serde_json::json!([]),
                            questions,
                        },
                    }],
                    cx,
                );
            });
        });
        cx.run_until_parked();
    }

    #[cfg(not(target_os = "macos"))]
    fn asked(
        id: &str,
        labels: &[&str],
        multi_select: bool,
    ) -> zz_protocol::agent_stream::AgentQuestion {
        zz_protocol::agent_stream::AgentQuestion {
            id: id.to_owned(),
            question: format!("{id}?"),
            options: labels
                .iter()
                .map(|label| zz_protocol::agent_stream::AgentQuestionOption {
                    label: (*label).to_owned(),
                    description: None,
                })
                .collect(),
            multi_select,
            allow_other: true,
            ..Default::default()
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn reply(id: &str, answers: &[&str]) -> zz_protocol::AgentQuestionAnswer {
        zz_protocol::AgentQuestionAnswer {
            id: id.to_owned(),
            answers: answers.iter().map(|answer| (*answer).to_owned()).collect(),
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[ala::test]
    fn a_question_card_answers_choices_and_typed_text_from_the_keyboard(cx: &mut TestAppContext) {
        let pane = PaneId(41);
        let (view, controller, sink, cx) = wired_view(cx, pane);
        ask(
            cx,
            &controller,
            pane,
            1,
            vec![
                asked("fruit", &["apple", "pear"], false),
                asked("tools", &["saw", "drill"], true),
            ],
        );
        assert!(cx.update(|_, cx| view.read(cx).question.is_some()));

        cx.simulate_keystrokes("2 1 3");
        assert!(
            cx.update(|_, cx| view.read(cx).input.read(cx).value().is_empty()),
            "the card keeps its digits out of the composer"
        );
        cx.simulate_input("chisel");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        assert_eq!(
            sink.borrow().as_slice(),
            &[(
                pane,
                crate::mux::client::AgentRequest::AnswerQuestion {
                    request_id: 1,
                    answers: vec![
                        reply("fruit", &["pear"]),
                        reply("tools", &["saw", "chisel"])
                    ],
                }
            )]
        );
        assert!(cx.update(|_, cx| view.read(cx).question.is_none()));
        assert!(
            cx.update(|window, cx| view.read(cx).focus(cx).is_focused(window)),
            "the composer takes the keyboard back"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[ala::test]
    fn one_plain_question_answers_on_its_digit_and_escape_dismisses(cx: &mut TestAppContext) {
        let pane = PaneId(42);
        let (_view, controller, sink, cx) = wired_view(cx, pane);
        ask(
            cx,
            &controller,
            pane,
            1,
            vec![asked("fruit", &["apple", "pear"], false)],
        );
        cx.simulate_keystrokes("1");
        ask(
            cx,
            &controller,
            pane,
            2,
            vec![asked("tools", &["saw"], true)],
        );
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();

        assert_eq!(
            sink.borrow().as_slice(),
            &[
                (
                    pane,
                    crate::mux::client::AgentRequest::AnswerQuestion {
                        request_id: 1,
                        answers: vec![reply("fruit", &["apple"])],
                    }
                ),
                (
                    pane,
                    crate::mux::client::AgentRequest::RespondPermission {
                        request_id: 2,
                        option_id: None,
                    }
                ),
            ]
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[ala::test]
    fn the_task_tray_opens_and_stops_a_task(cx: &mut TestAppContext) {
        let pane = PaneId(43);
        let (view, controller, sink, cx) = wired_view(cx, pane);
        cx.update(|_, cx| {
            controller.update(cx, |controller, cx| {
                controller.apply_pane_state(
                    pane,
                    &zz_protocol::AgentPaneWire {
                        phase: zz_protocol::AgentConnectionPhase::Running,
                        tasks: vec![zz_protocol::AgentTaskWire {
                            id: "b1".to_owned(),
                            kind: "shell".to_owned(),
                            description: "cargo build".to_owned(),
                            tool_call_id: None,
                        }],
                        ..Default::default()
                    },
                    cx,
                );
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert!(cx.debug_bounds("chat-task-stop-0").is_none());
        let chip = cx
            .debug_bounds("chat-task-tray")
            .expect("the tray chip should be painted");
        cx.simulate_click(chip.center(), ala::Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert_eq!(
            cx.update(|_, cx| view.read(cx).tray_open),
            Some(TrayPanel::Tasks)
        );
        let stop = cx
            .debug_bounds("chat-task-stop-0")
            .expect("an open tray lists the task");
        cx.simulate_click(stop.center(), ala::Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            sink.borrow().as_slice(),
            &[(
                pane,
                crate::mux::client::AgentRequest::StopTask {
                    task_id: "b1".to_owned()
                }
            )]
        );
    }

    #[cfg(not(target_os = "macos"))]
    fn stream(
        cx: &mut VisualTestContext,
        controller: &Entity<AgentController>,
        pane: PaneId,
        items: Vec<(u64, zz_daemon::AgentStreamPayload)>,
    ) {
        cx.update(|_, cx| {
            controller.update(cx, |controller, cx| {
                controller.apply_stream_items(
                    pane,
                    items
                        .into_iter()
                        .map(|(seq, payload)| zz_daemon::AgentStreamItem { seq, payload })
                        .collect(),
                    cx,
                );
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    #[cfg(not(target_os = "macos"))]
    fn sent_answers(sink: &RecordedRequests) -> Vec<crate::mux::client::AgentRequest> {
        sink.borrow()
            .iter()
            .map(|(_, request)| request.clone())
            .filter(|request| {
                matches!(
                    request,
                    crate::mux::client::AgentRequest::AnswerQuestion { .. }
                        | crate::mux::client::AgentRequest::RespondPermission { .. }
                        | crate::mux::client::AgentRequest::Prompt { .. }
                )
            })
            .collect()
    }

    #[cfg(not(target_os = "macos"))]
    fn ready_with_an_image(
        cx: &mut VisualTestContext,
        view: &Entity<AgentView>,
        controller: &Entity<AgentController>,
        pane: PaneId,
    ) {
        stream(
            cx,
            controller,
            pane,
            vec![
                (
                    1,
                    zz_daemon::AgentStreamPayload::Ready {
                        agent_name: "Claude Code".to_owned(),
                        agent_key: "claude".to_owned(),
                        auth_methods: Vec::new(),
                        capabilities: zz_protocol::agent_stream::AgentSessionCapabilities {
                            images: true,
                            ..Default::default()
                        },
                    },
                ),
                (
                    2,
                    zz_daemon::AgentStreamPayload::SessionReady {
                        session_id: "s-1".to_owned(),
                        modes: None,
                        config_options: None,
                    },
                ),
            ],
        );
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.attachments.push(Arc::new(Image::from_bytes(
                    ala::ImageFormat::Png,
                    vec![0x89, b'P', b'N', b'G'],
                )));
            });
        });
    }

    #[cfg(not(target_os = "macos"))]
    #[ala::test]
    fn one_enter_answers_the_card_and_nothing_behind_it(cx: &mut TestAppContext) {
        let pane = PaneId(45);
        let (view, controller, sink, cx) = wired_view(cx, pane);
        ready_with_an_image(cx, &view, &controller, pane);
        ask(
            cx,
            &controller,
            pane,
            3,
            vec![
                asked("fruit", &["apple", "pear"], false),
                asked("tools", &["saw", "drill"], true),
            ],
        );
        stream(
            cx,
            &controller,
            pane,
            vec![(
                4,
                zz_daemon::AgentStreamPayload::PermissionRequested {
                    request_id: 4,
                    tool_call: serde_json::json!({"toolCallId": "bash-1", "title": "Run ls"}),
                    options: serde_json::json!([
                        {"optionId": "allow-once", "name": "Allow once", "kind": "allow_once"},
                    ]),
                    questions: Vec::new(),
                },
            )],
        );

        cx.simulate_keystrokes("2 1 enter");
        cx.run_until_parked();

        assert_eq!(
            sent_answers(&sink),
            [crate::mux::client::AgentRequest::AnswerQuestion {
                request_id: 3,
                answers: vec![reply("fruit", &["pear"]), reply("tools", &["saw"])],
            }],
            "the Enter that answers the card neither approves the next request nor sends the images"
        );
        assert_eq!(
            cx.update(|_, cx| view
                .read(cx)
                .pane_state
                .pending_permissions
                .iter()
                .map(|request| request.request_id)
                .collect::<Vec<_>>()),
            [4]
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[ala::test]
    fn the_enter_that_answers_a_card_keeps_the_attached_images(cx: &mut TestAppContext) {
        let pane = PaneId(48);
        let (view, controller, sink, cx) = wired_view(cx, pane);
        ready_with_an_image(cx, &view, &controller, pane);
        ask(
            cx,
            &controller,
            pane,
            3,
            vec![asked("fruit", &["apple"], true)],
        );

        cx.simulate_keystrokes("1 enter");
        cx.run_until_parked();

        assert_eq!(
            sent_answers(&sink),
            [crate::mux::client::AgentRequest::AnswerQuestion {
                request_id: 3,
                answers: vec![reply("fruit", &["apple"])],
            }]
        );
        assert_eq!(cx.update(|_, cx| view.read(cx).attachments.len()), 1);
    }

    #[cfg(not(target_os = "macos"))]
    #[ala::test]
    fn a_card_resolved_elsewhere_hands_the_keyboard_back(cx: &mut TestAppContext) {
        let pane = PaneId(46);
        let (view, controller, _sink, cx) = wired_view(cx, pane);
        ask(
            cx,
            &controller,
            pane,
            1,
            vec![asked("fruit", &["apple", "pear"], false)],
        );
        cx.simulate_keystrokes("3");
        assert!(cx.update(|window, cx| !view.read(cx).focus(cx).is_focused(window)));
        assert!(cx.update(|window, cx| {
            view.read(cx)
                .question
                .as_ref()
                .is_some_and(|card| card.editing(window, cx))
        }));

        stream(
            cx,
            &controller,
            pane,
            vec![(
                2,
                zz_daemon::AgentStreamPayload::PermissionResolved {
                    request_id: 1,
                    canceled: true,
                },
            )],
        );

        assert!(cx.update(|_, cx| view.read(cx).question.is_none()));
        assert!(cx.update(|window, cx| view.read(cx).focus(cx).is_focused(window)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_usage_is_bounded_and_handles_an_unknown_window() {
        assert_eq!(context_usage_fraction(0, 0), 0.0);
        assert_eq!(context_usage_fraction(25, 100), 0.25);
        assert_eq!(context_usage_fraction(125, 100), 1.0);
    }

    #[test]
    fn context_usage_tooltip_keeps_the_exact_counts() {
        assert_eq!(
            context_usage_tooltip(32_000, 128_000),
            "32000 of 128000 context tokens used (25%)"
        );
        assert_eq!(
            context_usage_tooltip(4, 0),
            "Context window usage unavailable"
        );
    }

    #[test]
    fn git_footer_pluralizes_the_file_count() {
        assert_eq!(git_file_count_label(0), "0 files");
        assert_eq!(git_file_count_label(1), "1 file");
        assert_eq!(git_file_count_label(12), "12 files");
    }

    #[test]
    fn busy_chrome_follows_the_connection_and_not_a_tool_row() {
        for connection in [
            AgentConnectionState::Starting,
            AgentConnectionState::Restoring,
            AgentConnectionState::Running,
            AgentConnectionState::Cancelling,
        ] {
            assert!(pane_is_busy(connection));
        }
        for connection in [
            AgentConnectionState::Ready,
            AgentConnectionState::Failed,
            AgentConnectionState::Disconnected,
        ] {
            assert!(!pane_is_busy(connection));
        }
    }

    #[test]
    fn rendered_errors_are_bounded_and_strip_control_bytes() {
        let rendered = rendered_error(&format!("bad\0{}", "x".repeat(4096)));
        assert!(rendered.len() <= MAX_RENDERED_ERROR_BYTES);
        assert!(rendered.starts_with("bad�"));
        assert!(rendered.ends_with('…'));
        assert!(!rendered.contains('\0'));
    }
}
