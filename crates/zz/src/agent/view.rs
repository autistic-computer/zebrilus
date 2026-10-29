use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};
use zz_ui::agent::composer::{COMPOSER_OUTER_PADDING, Composer};
use zz_ui::agent::controls::{
    ComposerAction, agent_queue_chip, composer_action, composer_action_button, composer_add_button,
    context_usage_meter, git_summary_footer,
};
#[cfg(test)]
use zz_ui::agent::controls::{context_usage_fraction, context_usage_tooltip, git_file_count_label};
#[cfg(test)]
use zz_ui::agent::presentation::MAX_RENDERED_ERROR_BYTES;
#[cfg(test)]
use zz_ui::agent::presentation::rendered_error;
use zz_ui::agent::presentation::{
    empty_state, error_card, lens_welcome, permission_card, permission_option, spinner_phase,
    terminal_wait_card,
};
use zz_ui::agent::question::{QuestionCardAction, QuestionCardState, QuestionCardStep};
use zz_ui::agent::tasks::{TaskTrayAction, TrayPanel, task_tray};

use ala::{
    AnyElement, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable, Image,
    IntoElement, KeyDownEvent, ListAlignment, ListState, MouseButton, MouseDownEvent, Render,
    ScrollStrategy, SharedString, Subscription, UniformListScrollHandle, Window, div, prelude::*,
    px, uniform_list,
};
use zz_client::agent_completion::{
    CommandCompletion, active_command_hint, bare_command_name, completion_query,
    meaningful_command_description, pane_commands, ranked_completions,
};
use zz_client::agent_transcript::rewind_command;
use zz_ui::{
    command::target_picker,
    target::{TargetPicker, mention_at_caret, mention_keys},
};

use crate::command::palette::palette_backend;
use zz_protocol::{AgentProvider, CommandInvocation, PaneId};
use zz_ui::agent::DisclosureKind;
use zz_ui::agent::{
    AGENT_CONTENT_MAX_WIDTH, AgentAside, AgentEntry, AgentMarkdown, AgentTimeline,
    AgentTimelineStore, AgentToolEntry, AgentToolKind, AgentToolPayload, AgentToolStatus,
    AgentToolText, FoldedTimelineRows, MarkdownSlot, TimelineRow, TimelineStick,
    agent_jump_to_bottom_button, append_timeline_row, composer_attachment, fold_timeline_rows,
    plan_progress, timeline_group_kind, timeline_parent, tool_output_text,
};
use zz_ui::{
    button::Button,
    h_flex,
    input::{IndentInline, InputEvent, InputState, MoveDown, MoveUp},
    scroll::Scrollbar,
};

use crate::{
    agent::attachment as agent_attachment,
    agent::controller::{
        AgentCommand, AgentConnectionState, AgentController, AgentPaneState, AgentPermissionKind,
        AgentPermissionRequest, AgentSessionCapabilities, AgentThreadEntry, AgentToolKindModel,
        AgentToolStatusModel, ToolPayload,
    },
    config::pane_content_radii,
    mux::client::MuxClient,
    window::corners::{WindowCorners, round_div_radii},
};

mod chat;

const AGENT_KEY_CONTEXT: &str = "Agent";
const STICK_FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_micros(16_667);

const fn pane_is_busy(connection: AgentConnectionState) -> bool {
    matches!(
        connection,
        AgentConnectionState::Starting
            | AgentConnectionState::Restoring
            | AgentConnectionState::Running
            | AgentConnectionState::Cancelling
    )
}

enum TimelineStoreUpdate {
    None,
    Clear,
    Synchronize(Vec<AgentEntry>),
}

enum TimelineModelUpdate {
    None,
    Rebuild,
    Incremental {
        store_entries: Vec<AgentEntry>,
        remeasure_rows: Vec<usize>,
        splice_start: usize,
        added_rows: usize,
    },
}

#[derive(Clone, Default)]
struct TimelineModel {
    rows: Arc<Vec<TimelineRow>>,
    entry_ids: Vec<u64>,
    entry_revisions: Vec<u64>,
    entry_to_row: Vec<usize>,
    markdown: HashMap<u64, AgentMarkdown>,
    tool_payloads: HashMap<u64, Vec<Option<AgentToolPayload>>>,
    plan: Option<u64>,
}

fn plan_entry_id(entries: &[AgentThreadEntry]) -> Option<u64> {
    entries.iter().rev().find_map(|entry| match entry {
        AgentThreadEntry::Plan { id, .. } => Some(*id),
        _ => None,
    })
}

impl TimelineModel {
    fn new(entries: &[AgentThreadEntry], revisions: &[u64], parents: ToolParents<'_>) -> Self {
        debug_assert_eq!(entries.len(), revisions.len());
        let mut markdown = HashMap::new();
        let mut tool_payloads = HashMap::new();
        let ui_entries = entries
            .iter()
            .map(|entry| {
                parented(
                    ui_entry_with_markdown(entry, &mut markdown, &mut tool_payloads),
                    parents,
                )
            })
            .collect::<Vec<_>>();
        let FoldedTimelineRows { rows, entry_to_row } = fold_timeline_rows(&ui_entries);
        Self {
            rows,
            entry_ids: entries.iter().map(AgentThreadEntry::id).collect(),
            entry_revisions: revisions.to_vec(),
            entry_to_row,
            markdown,
            tool_payloads,
            plan: plan_entry_id(entries),
        }
    }

    fn clear(&mut self) {
        self.rows = Arc::new(Vec::new());
        self.entry_ids.clear();
        self.entry_revisions.clear();
        self.entry_to_row.clear();
        self.markdown.clear();
        self.tool_payloads.clear();
        self.plan = None;
    }

    fn rebuild(
        &mut self,
        entries: &[AgentThreadEntry],
        revisions: &[u64],
        parents: ToolParents<'_>,
    ) {
        *self = Self::new(entries, revisions, parents);
    }

    fn synchronize(
        &mut self,
        entries: &[AgentThreadEntry],
        revisions: &[u64],
        changed_entries: Option<&[usize]>,
        parents: ToolParents<'_>,
    ) -> TimelineModelUpdate {
        debug_assert_eq!(entries.len(), revisions.len());
        if entries.len() != revisions.len() {
            return TimelineModelUpdate::None;
        }

        let old_entry_count = self.entry_ids.len();
        let append_only = old_entry_count <= entries.len()
            && if changed_entries.is_some() {
                old_entry_count == 0
                    || self.entry_ids.last().copied()
                        == entries.get(old_entry_count - 1).map(AgentThreadEntry::id)
            } else {
                self.entry_ids
                    .iter()
                    .zip(entries)
                    .all(|(id, entry)| *id == entry.id())
            };
        if !append_only {
            self.rebuild(entries, revisions, parents);
            return TimelineModelUpdate::Rebuild;
        }

        let changed_existing = changed_entries.map_or_else(
            || {
                (0..old_entry_count)
                    .filter(|index| self.entry_revisions[*index] != revisions[*index])
                    .collect::<Vec<_>>()
            },
            |changes| {
                changes
                    .iter()
                    .copied()
                    .filter(|index| {
                        *index < old_entry_count
                            && self.entry_revisions[*index] != revisions[*index]
                    })
                    .collect::<Vec<_>>()
            },
        );
        if changed_existing.is_empty() && old_entry_count == entries.len() {
            return TimelineModelUpdate::None;
        }

        let mut replacements = Vec::with_capacity(changed_existing.len());
        for &entry_index in &changed_existing {
            let id = self.entry_ids[entry_index];
            let Some(&row_index) = self.entry_to_row.get(entry_index) else {
                self.rebuild(entries, revisions, parents);
                return TimelineModelUpdate::Rebuild;
            };
            let next = parented(
                ui_entry_with_markdown(
                    &entries[entry_index],
                    &mut self.markdown,
                    &mut self.tool_payloads,
                ),
                parents,
            );
            let Some(previous) = self.rows.get(row_index).and_then(|row| row.entry(id)) else {
                self.rebuild(entries, revisions, parents);
                return TimelineModelUpdate::Rebuild;
            };
            if !replacement_preserves_folding(previous, &next) {
                self.rebuild(entries, revisions, parents);
                return TimelineModelUpdate::Rebuild;
            }
            replacements.push((entry_index, row_index, id, next));
        }

        let appended = entries
            .iter()
            .skip(old_entry_count)
            .map(|entry| {
                parented(
                    ui_entry_with_markdown(entry, &mut self.markdown, &mut self.tool_payloads),
                    parents,
                )
            })
            .collect::<Vec<_>>();
        let parent_arrives_later = appended.iter().enumerate().any(|(index, entry)| {
            timeline_parent(entry).is_some_and(|parent| {
                !self.entry_ids.contains(&parent)
                    && !appended[..index]
                        .iter()
                        .any(|earlier| earlier.id() == parent)
            })
        });
        if parent_arrives_later {
            self.rebuild(entries, revisions, parents);
            return TimelineModelUpdate::Rebuild;
        }
        let old_row_count = self.rows.len();
        let mut store_entries = Vec::with_capacity(replacements.len());
        let mut remeasure_rows = replacements
            .iter()
            .map(|(_, row_index, _, _)| *row_index)
            .collect::<Vec<_>>();
        let rows = Arc::make_mut(&mut self.rows);
        for (entry_index, row_index, id, entry) in replacements {
            store_entries.push(entry.clone());
            let replaced = rows[row_index].replace_entry(id, entry);
            debug_assert!(replaced);
            self.entry_revisions[entry_index] = revisions[entry_index];
        }

        for entry in appended {
            let (row_index, row_added) = append_timeline_row(rows, entry);
            self.entry_to_row.push(row_index);
            if !row_added && row_index < old_row_count {
                remeasure_rows.push(row_index);
            }
        }
        if let Some(plan) = plan_entry_id(&entries[old_entry_count..]) {
            self.plan = Some(plan);
        }
        self.entry_ids
            .extend(entries[old_entry_count..].iter().map(AgentThreadEntry::id));
        self.entry_revisions
            .extend_from_slice(&revisions[old_entry_count..]);

        remeasure_rows.sort_unstable();
        remeasure_rows.dedup();
        TimelineModelUpdate::Incremental {
            store_entries,
            remeasure_rows,
            splice_start: old_row_count,
            added_rows: rows.len() - old_row_count,
        }
    }
}

fn replacement_preserves_folding(previous: &AgentEntry, next: &AgentEntry) -> bool {
    timeline_group_kind(previous) == timeline_group_kind(next)
        && timeline_parent(previous) == timeline_parent(next)
}

type ToolParents<'a> = &'a dyn Fn(u64) -> Option<u64>;

fn parented(mut entry: AgentEntry, parents: ToolParents<'_>) -> AgentEntry {
    if let AgentEntry::Tool(tool) = &mut entry {
        tool.parent = parents(tool.id);
    }
    entry
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
    timeline: TimelineModel,
    timeline_store: Entity<AgentTimelineStore>,
    timeline_next_revision: u64,
    conversation_epoch: u64,
    timeline_scroll: ListState,
    stick: TimelineStick,
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
        let timeline_store = cx.new(|_| AgentTimelineStore::owned_by(pane.0));
        let timeline_store_observer = cx.observe(&timeline_store, |_, _, cx| cx.notify());
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
            .map(|pane| std::path::PathBuf::from(&pane.summary.cwd))
            .filter(|cwd| cwd.is_absolute())
            .unwrap_or_else(|| std::path::PathBuf::from("/"));
        controller.update(cx, |controller, _| {
            controller.ensure_pane(pane, provider, cwd);
        });
        let (pane_state, timeline, timeline_next_revision, conversation_epoch) = {
            let controller = controller.read(cx);
            let pane_state = controller
                .pane_state(pane)
                .unwrap_or_else(disconnected_pane_state);
            let (entries, revisions, next_revision) =
                controller.pane_entries(pane).unwrap_or((&[], &[], 0));
            (
                pane_state,
                TimelineModel::new(entries, revisions, &|id| {
                    controller.tool_parent_entry(pane, id)
                }),
                next_revision,
                controller.conversation_epoch(pane),
            )
        };
        let timeline_scroll = ListState::new(0, ListAlignment::Top, px(1_200.0));
        timeline_scroll.splice(0..0, timeline.rows.len());
        let stick = TimelineStick::new(&timeline_scroll, cx.reduce_motion());
        let scroll_view = cx.weak_entity();
        timeline_scroll.set_scroll_handler(move |_, _, cx| {
            let view = scroll_view.clone();
            cx.defer(move |cx| {
                view.update(cx, |view: &mut Self, cx| view.on_timeline_scroll(cx))
                    .ok();
            });
        });
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
        Self {
            pane,
            mux,
            controller,
            input,
            visible: false,
            pane_state,
            timeline,
            timeline_store,
            timeline_next_revision,
            conversation_epoch,
            timeline_scroll,
            stick,
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
                timeline_store_observer,
                input_subscription,
                controller_observer,
                mux_observer,
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
        let reduce_motion = cx.reduce_motion();
        let (changed, store_update) = {
            let controller = controller.read(cx);
            self.synchronize_controller_state(controller, reduce_motion)
        };
        let streaming = self
            .pane_state
            .connection
            .has_active_turn()
            .then(|| self.timeline.entry_ids.last().copied())
            .flatten();
        self.timeline_store
            .update(cx, |store, cx| store.set_streaming(streaming, cx));
        let store_changed = self.update_timeline_store(store_update, cx);
        let cwd = self.pane_state.cwd.clone();
        self.timeline_store
            .update(cx, |store, cx| store.set_cwd(Some(cwd), cx));
        changed || store_changed
    }

    fn update_timeline_store(
        &mut self,
        store_update: TimelineStoreUpdate,
        cx: &mut Context<Self>,
    ) -> bool {
        match store_update {
            TimelineStoreUpdate::None => false,
            TimelineStoreUpdate::Clear => self.timeline_store.update(cx, AgentTimelineStore::clear),
            TimelineStoreUpdate::Synchronize(entries) => {
                for entry in &entries {
                    synchronize_entry_store(&self.timeline_store, entry, cx);
                }
                false
            }
        }
    }

    fn synchronize_controller_state(
        &mut self,
        controller: &AgentController,
        reduce_motion: bool,
    ) -> (bool, TimelineStoreUpdate) {
        let next_state = controller
            .pane_state(self.pane)
            .unwrap_or_else(disconnected_pane_state);
        let state_changed = self.pane_state != next_state;
        let provider_changed = self.pane_state.provider != next_state.provider;
        let next_conversation_epoch = controller.conversation_epoch(self.pane);
        let conversation_changed = self.conversation_epoch != next_conversation_epoch
            || provider_changed
            || self.pane_state.session_id != next_state.session_id;
        self.conversation_epoch = next_conversation_epoch;
        let commands_changed = provider_changed
            || self.pane_state.available_commands.as_ref()
                != next_state.available_commands.as_ref();
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

        let pane = self.pane;
        let parents = |id| controller.tool_parent_entry(pane, id);
        let Some((entries, revisions, next_revision)) = controller.pane_entries(self.pane) else {
            if !self.timeline.rows.is_empty() {
                self.timeline.clear();
                self.timeline_next_revision = 0;
                self.timeline_scroll.reset(0);
                self.stick.engage_now(&self.timeline_scroll, reduce_motion);
                return (true, TimelineStoreUpdate::Clear);
            }
            return (state_changed, TimelineStoreUpdate::Clear);
        };
        if conversation_changed {
            self.timeline.rebuild(entries, revisions, &parents);
            self.timeline_next_revision = next_revision;
            self.timeline_scroll.reset(self.timeline.rows.len());
            self.stick.engage_now(&self.timeline_scroll, reduce_motion);
            return (true, TimelineStoreUpdate::Clear);
        }
        if self.timeline.entry_ids.len() == entries.len()
            && self.timeline_next_revision == next_revision
        {
            return (state_changed, TimelineStoreUpdate::None);
        }
        let Some(changed_entries) =
            controller.pane_entry_changes(self.pane, self.timeline_next_revision)
        else {
            self.timeline.rebuild(entries, revisions, &parents);
            self.timeline_next_revision = next_revision;
            self.timeline_scroll.reset(self.timeline.rows.len());
            self.stick.engage_now(&self.timeline_scroll, reduce_motion);
            return (true, TimelineStoreUpdate::Clear);
        };
        self.timeline_next_revision = next_revision;
        let (timeline_changed, store_update) = self.synchronize_timeline(
            entries,
            revisions,
            &changed_entries,
            &parents,
            reduce_motion,
        );
        (timeline_changed || state_changed, store_update)
    }

    fn synchronize_timeline(
        &mut self,
        entries: &[AgentThreadEntry],
        revisions: &[u64],
        changed_entries: &[usize],
        parents: ToolParents<'_>,
        reduce_motion: bool,
    ) -> (bool, TimelineStoreUpdate) {
        match self
            .timeline
            .synchronize(entries, revisions, Some(changed_entries), parents)
        {
            TimelineModelUpdate::None => (false, TimelineStoreUpdate::None),
            TimelineModelUpdate::Rebuild => {
                self.timeline_scroll.reset(self.timeline.rows.len());
                self.stick.engage_now(&self.timeline_scroll, reduce_motion);
                (true, TimelineStoreUpdate::Clear)
            }
            TimelineModelUpdate::Incremental {
                store_entries,
                remeasure_rows,
                splice_start,
                added_rows,
            } => {
                for row_index in remeasure_rows {
                    self.timeline_scroll
                        .remeasure_items(row_index..row_index + 1);
                }
                if added_rows > 0 {
                    self.timeline_scroll
                        .splice(splice_start..splice_start, added_rows);
                }
                if self.stick.is_pinned() {
                    self.stick.wake();
                }
                let store_update = if store_entries.is_empty() {
                    TimelineStoreUpdate::None
                } else {
                    TimelineStoreUpdate::Synchronize(store_entries)
                };
                (true, store_update)
            }
        }
    }

    fn recompute_completions(&mut self, commands: &[AgentCommand]) {
        let Some(query) = completion_query(&self.last_input, self.last_cursor) else {
            self.completions = Arc::from([]);
            self.completion_selected = None;
            return;
        };
        if self.completion_dismissed {
            self.completions = Arc::from([]);
            self.completion_selected = None;
            return;
        }
        self.completions = ranked_completions(commands, &query).into();
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
        self.completion_dismissed = true;
        self.input.update(cx, |input, cx| {
            input.set_selected_range(completion.replacement, cx);
            input.replace(insertion, window, cx);
        });
        self.completions = Arc::from([]);
        self.completion_selected = None;
        self.focus(cx).focus(window, cx);
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
                self.stick.engage(&self.timeline_scroll, cx.reduce_motion());
            }
            Err(error) => self.submission_error = Some(error),
        }
        cx.notify();
    }

    fn rewind_to(&mut self, message_id: &str, cx: &mut Context<Self>) {
        let pane = self.pane;
        let result = self.controller.update(cx, |controller, cx| {
            controller.prompt(pane, &rewind_command(message_id), Vec::new(), cx)
        });
        self.submission_error = result.err();
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
            AgentConnectionState::Starting | AgentConnectionState::Restoring => {
                format!("Reading {agent}’s transcript…").into()
            }
            AgentConnectionState::Ready => return lens_welcome(state.provider, cx),
            AgentConnectionState::Running => format!("Waiting for {agent}’s first update…").into(),
            AgentConnectionState::Cancelling => "Cancelling the current turn…".into(),
            AgentConnectionState::Failed => format!("Could not read {agent}’s transcript.").into(),
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
            let view = view.clone();
            return Some(
                terminal_wait_card(
                    format!("agent-show-terminal-{}-{request_id}", self.pane.0),
                    permission.title.clone(),
                    move |_, cx| view.update(cx, |_, cx| cx.emit(ShowTerminal)),
                    cx,
                )
                .into_any_element(),
            );
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
        Some(agent_queue_chip(
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
                    controller.stop_task(pane, task_id, cx);
                });
            }
            TaskTrayAction::Reveal(tool_call_id) => self.reveal_tool(&tool_call_id, cx),
        }
        cx.notify();
    }

    fn reveal_tool(&mut self, tool_call_id: &str, cx: &mut Context<Self>) {
        let Some(entry) = self.controller.read(cx).tool_entry(self.pane, tool_call_id) else {
            return;
        };
        let Some(row) = self
            .timeline
            .entry_ids
            .iter()
            .position(|id| *id == entry)
            .and_then(|index| self.timeline.entry_to_row.get(index).copied())
        else {
            return;
        };
        if let Some(turn) = self.timeline.rows.get(row).map(TimelineRow::id) {
            self.timeline_store.update(cx, |store, cx| {
                store.set_expanded(turn, DisclosureKind::Turn, true, cx);
            });
        }
        self.stick.reveal(&self.timeline_scroll, row);
    }

    fn current_plan(&self, state: &AgentPaneState) -> Option<String> {
        let source = self
            .timeline
            .markdown
            .get(&self.timeline.plan?)?
            .full_text();
        let (done, total, _) = plan_progress(&source);
        (total > 0 && (done < total || state.connection.has_active_turn())).then_some(source)
    }

    fn open_output(&self, tool: &AgentToolEntry, cx: &mut Context<Self>) {
        let Some(args) = zz_client::agent_output::output_pane_args(
            self.pane,
            &self.pane_state.cwd,
            &tool.label,
            tool.exit_code,
            &tool_output_text(tool),
        ) else {
            return;
        };
        self.mux
            .read(cx)
            .execute(CommandInvocation::new("split-window", args));
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
        let plan = self.current_plan(state);
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

    fn render_jump_to_end(&self, view: &Entity<Self>, cx: &ala::App) -> impl IntoElement {
        let view = view.clone();
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(2.0 * COMPOSER_OUTER_PADDING))
            .flex()
            .justify_center()
            .child(
                agent_jump_to_bottom_button(("agent-jump-to-end", self.pane.0), cx).on_click(
                    move |_, _, cx| {
                        view.update(cx, AgentView::jump_to_timeline_end);
                        cx.stop_propagation();
                    },
                ),
            )
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
                        let name = bare_command_name(&command.name);
                        let description = meaningful_command_description(&command.description)
                            .map(ToOwned::to_owned);
                        let is_selected = selected == Some(index);
                        let hover_view = rows_view.clone();
                        let click_view = rows_view.clone();
                        Some(
                            zz_ui::agent::slash::suggestion_row(
                                format!("agent-completion-{}-{index}", pane.0),
                                name,
                                description.map(Into::into),
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
        .h(zz_ui::agent::slash::suggestion_list_height(
            self.completions.len(),
        ))
        .track_scroll(&self.completion_scroll);
        Some(zz_ui::agent::slash::suggestion_list(rows, cx).into_any_element())
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
                "Mention a host, session or file",
                cx,
            )
            .on_click(move |_, window, cx| mention_at_caret(&input, "@", window, cx))
        };
        let mention = self
            .mention
            .read(cx)
            .is_open()
            .then(|| self.mention.clone().into_any_element());
        zz_ui::agent::composer::AgentComposer {
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
            git: git.map(IntoElement::into_any_element),
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

pub(crate) struct ShowTerminal;

impl EventEmitter<ShowTerminal> for AgentView {}

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
        self.stick.engage(&self.timeline_scroll, cx.reduce_motion());
    }

    fn on_timeline_scroll(&mut self, cx: &mut Context<Self>) {
        let reduce_motion = cx.reduce_motion();
        if self
            .stick
            .on_user_scroll(&self.timeline_scroll, reduce_motion)
        {
            cx.notify();
        }
    }

    fn jump_to_timeline_end(&mut self, cx: &mut Context<Self>) {
        self.stick.engage(&self.timeline_scroll, cx.reduce_motion());
        cx.notify();
    }

    fn drive_stick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if cx.reduce_motion() || !self.stick.wants_frame(&self.timeline_scroll) {
            return;
        }
        self.stick.arm();
        cx.spawn_in(window, async move |view, cx| {
            cx.background_executor().timer(STICK_FRAME_INTERVAL).await;
            view.update(cx, |view: &mut Self, cx| {
                let list = view.timeline_scroll.clone();
                if view.stick.step(&list) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
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
        let rows = self.timeline.rows.clone();
        let has_timeline = !rows.is_empty() && !use_chat;
        let show_jump = has_timeline && self.stick.shows_jump_button();
        self.stick.set_bottom_padding(COMPOSER_OUTER_PADDING);
        self.drive_stick(window, cx);
        let floats = self.floats;
        let view = cx.entity();
        let input_focus = self.focus(cx);
        let chat_body = use_chat.then(|| self.render_chat(&state, &view, cx));
        let timeline = AgentTimeline::new(
            rows,
            self.timeline_scroll.clone(),
            self.timeline_store.clone(),
        )
        .active_turn(state.connection.has_active_turn())
        .bottom_padding(COMPOSER_OUTER_PADDING)
        .open_output({
            let view = view.clone();
            move |tool, _, cx| view.update(cx, |view, cx| view.open_output(tool, cx))
        });
        let timeline = if state.session_capabilities.verbs {
            let rewind_view = view.clone();
            timeline.rewind(
                state.connection.accepts_prompt() && state.pending_permissions.is_empty(),
                move |message_id, _, cx| {
                    rewind_view.update(cx, |view, cx| view.rewind_to(message_id, cx));
                },
            )
        } else {
            timeline
        };
        let root = div()
            .id(("agent-lens", self.pane.0))
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
                    .when(!has_timeline && !use_chat, |this| {
                        this.child(
                            div().size_full().px_3().child(
                                div()
                                    .size_full()
                                    .max_w(px(AGENT_CONTENT_MAX_WIDTH))
                                    .mx_auto()
                                    .child(Self::render_empty_state(&state, cx.entity_id(), cx)),
                            ),
                        )
                    })
                    .when(has_timeline, |this| {
                        this.child(timeline).child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .right_0()
                                .bottom(px(COMPOSER_OUTER_PADDING))
                                .child(Scrollbar::vertical(&self.timeline_scroll)),
                        )
                    })
                    .when(show_jump, |this| {
                        this.child(self.render_jump_to_end(&view, cx))
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
        error: Some(Arc::from("the lens is not open")),
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
    }
}

#[cfg(test)]
fn ui_entries(entries: &[AgentThreadEntry]) -> Arc<[AgentEntry]> {
    let mut markdown = HashMap::new();
    let mut tool_payloads = HashMap::new();
    entries
        .iter()
        .map(|entry| ui_entry_with_markdown(entry, &mut markdown, &mut tool_payloads))
        .collect::<Vec<_>>()
        .into()
}

#[cfg(test)]
fn ui_entry(entry: &AgentThreadEntry) -> AgentEntry {
    ui_entry_with_markdown(entry, &mut HashMap::new(), &mut HashMap::new())
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
    entry: &AgentThreadEntry,
    markdown_sources: &mut HashMap<u64, AgentMarkdown>,
    tool_payloads: &mut HashMap<u64, Vec<Option<AgentToolPayload>>>,
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
            rewind_id: entry.rewind_id().map(SharedString::from),
        },
        AgentThreadEntry::Assistant {
            id,
            markdown,
            aside,
            ..
        } => AgentEntry::Assistant {
            id: *id,
            markdown: streaming_markdown(markdown_sources, *id, markdown),
            aside: aside.map(|aside| AgentAside {
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
            label: SharedString::from(label.clone()),
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
            let payloads = tool_payloads.entry(*id).or_default();
            payloads.resize_with(output.len() + 1, || None);
            if input.is_none() {
                payloads[0] = None;
            }
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
                label: SharedString::from(label.clone()),
                location: location.clone().map(SharedString::from),
                input: input
                    .as_ref()
                    .map(|payload| retained_tool_payload(&mut payloads[0], payload)),
                output: output
                    .iter()
                    .enumerate()
                    .map(|(index, payload)| {
                        retained_tool_payload(&mut payloads[index + 1], payload)
                    })
                    .collect::<Vec<_>>()
                    .into(),
                default_expanded: *default_expanded,
                parent: None,
                exit_code: *exit_code,
            })
        }
        AgentThreadEntry::Plan { id, markdown } => AgentEntry::Plan {
            id: *id,
            markdown: replaced_markdown(markdown_sources, *id, markdown),
        },
    }
}

fn synchronize_entry_store(
    store: &Entity<AgentTimelineStore>,
    entry: &AgentEntry,
    cx: &mut Context<AgentView>,
) {
    match entry {
        AgentEntry::User { id, markdown, .. }
        | AgentEntry::Assistant { id, markdown, .. }
        | AgentEntry::Reasoning { id, markdown, .. }
        | AgentEntry::Plan { id, markdown } => {
            store.update(cx, |store, cx| {
                store.synchronize_markdown(*id, MarkdownSlot::Body, markdown.clone(), cx);
            });
        }
        AgentEntry::Tool(_) => {}
    }
}

fn retained_tool_payload(
    retained: &mut Option<AgentToolPayload>,
    payload: &ToolPayload,
) -> AgentToolPayload {
    let next = match (retained.as_ref(), payload) {
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
    *retained = Some(next.clone());
    next
}

#[cfg(test)]
mod completion_tests {
    #[cfg(not(target_os = "macos"))]
    use std::{cell::RefCell, rc::Rc};

    #[cfg(not(target_os = "macos"))]
    use ala::{TestAppContext, VisualTestContext};
    #[cfg(not(target_os = "macos"))]
    use zz_daemon_client::DaemonError;
    #[cfg(not(target_os = "macos"))]
    use zz_ui::Root;

    use super::*;

    #[cfg(not(target_os = "macos"))]
    fn command(name: &str) -> AgentCommand {
        AgentCommand {
            name: name.to_owned(),
            description: format!("Run {name}"),
            input_hint: None,
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
    fn non_append_timeline_rebuild_clears_retained_store(cx: &mut TestAppContext) {
        cx.update(zz_ui::init);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let input = cx.new(|cx| InputState::new(window, cx));
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(DaemonError::Thread("test client".to_owned())),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            let controller = cx.new(|_| AgentController::new());
            let timeline_store = cx.new(|_| AgentTimelineStore::default());
            timeline_store.update(cx, |store, cx| {
                assert!(!store.expanded(1, DisclosureKind::Turn, false));
                store.toggle_expanded(1, DisclosureKind::Turn, false, cx);
                store.markdown(1, MarkdownSlot::Body, "old session".into(), cx);
            });
            let timeline_entries = [
                AgentThreadEntry::User {
                    id: 1,
                    markdown: "old session".to_owned(),
                    images: Vec::new(),
                    message_id: None,
                },
                AgentThreadEntry::Assistant {
                    id: 2,
                    markdown: "old response".to_owned(),
                    aside: None,
                },
            ];

            let timeline_scroll = ListState::new(2, ListAlignment::Top, px(1_200.0));
            let mention = target_picker(&palette_backend(mux.clone()), Some(input.clone()), cx);
            AgentView {
                pane: PaneId(7),
                mux,
                controller,
                input,
                visible: false,
                pane_state: disconnected_pane_state(),
                timeline: TimelineModel::new(&timeline_entries, &[1, 2], &|_| None),
                timeline_store,
                timeline_next_revision: 2,
                conversation_epoch: 0,
                stick: TimelineStick::new(&timeline_scroll, false),
                timeline_scroll,
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
                floats: false,
                chat: zz_ui::agent::chat::ChatPane::default(),
                chat_inline: false,
                _subscriptions: Vec::new(),
            }
        });
        let replacement = [AgentThreadEntry::User {
            id: 1,
            markdown: "new session".to_owned(),
            images: Vec::new(),
            message_id: None,
        }];
        let (changed, cleared) = cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let (changed, store_update) =
                    view.synchronize_timeline(&replacement, &[3], &[], &|_| None, false);
                let cleared = view.update_timeline_store(store_update, cx);
                (changed, cleared)
            })
        });

        assert!(changed);
        assert!(cleared);
        let timeline_store = cx.update(|_, cx| view.read(cx).timeline_store.clone());
        assert!(!cx.update(|_, cx| {
            timeline_store.update(cx, |store, _| {
                store.expanded(1, DisclosureKind::Turn, false)
            })
        }));
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
            let timeline_scroll = ListState::new(0, ListAlignment::Top, px(1_200.0));
            let mention = target_picker(&palette_backend(mux.clone()), Some(input.clone()), cx);
            let view = cx.new(|view_cx| AgentView {
                pane: PaneId(7),
                mux,
                controller,
                input,
                visible: false,
                pane_state: disconnected_pane_state(),
                timeline: TimelineModel::default(),
                timeline_store: view_cx.new(|_| AgentTimelineStore::default()),
                timeline_next_revision: 0,
                conversation_epoch: 0,
                stick: TimelineStick::new(&timeline_scroll, false),
                timeline_scroll,
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
        assert!(cx.debug_bounds("agent-task-stop-0").is_none());
        let chip = cx
            .debug_bounds("agent-task-tray")
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
            .debug_bounds("agent-task-stop-0")
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
    #[ala::test]
    fn subagent_steps_nest_from_the_controllers_parent_links(cx: &mut TestAppContext) {
        let pane = PaneId(44);
        let (view, controller, _sink, cx) = wired_view(cx, pane);
        let tool = |seq: u64, id: &str, meta: serde_json::Value| zz_daemon::AgentStreamItem {
            seq,
            payload: zz_daemon::AgentStreamPayload::Update {
                update: serde_json::json!({
                    "sessionUpdate": "tool_call",
                    "toolCallId": id,
                    "title": id,
                    "kind": "read",
                    "_meta": meta,
                }),
            },
        };
        cx.update(|_, cx| {
            controller.update(cx, |controller, cx| {
                controller.apply_stream_items(
                    pane,
                    vec![
                        tool(1, "agent-1", serde_json::json!({})),
                        tool(
                            2,
                            "read-1",
                            serde_json::json!({"zz": {"parent": "agent-1"}}),
                        ),
                        tool(
                            3,
                            "read-2",
                            serde_json::json!({"zz": {"parent": "agent-1"}}),
                        ),
                    ],
                    cx,
                );
            });
        });
        cx.run_until_parked();

        let rows = cx.update(|_, cx| view.read(cx).timeline.rows.clone());
        let [TimelineRow::Group { entries, .. }] = rows.as_slice() else {
            panic!("the steps join their agent's row: {rows:?}");
        };
        let parents = entries
            .iter()
            .map(zz_ui::agent::timeline_parent)
            .collect::<Vec<_>>();
        let agent = entries[0].id();
        assert_eq!(parents, [None, Some(agent), Some(agent)]);
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

    #[cfg(not(target_os = "macos"))]
    #[ala::test]
    fn a_step_that_arrives_before_its_agent_nests_once_the_agent_shows_up(cx: &mut TestAppContext) {
        let pane = PaneId(47);
        let (view, controller, _sink, cx) = wired_view(cx, pane);
        let tool = |id: &str, meta: serde_json::Value| zz_daemon::AgentStreamPayload::Update {
            update: serde_json::json!({
                "sessionUpdate": "tool_call",
                "toolCallId": id,
                "title": id,
                "kind": "read",
                "_meta": meta,
            }),
        };
        stream(
            cx,
            &controller,
            pane,
            vec![(
                1,
                tool("read-1", serde_json::json!({"zz": {"parent": "agent-1"}})),
            )],
        );
        stream(
            cx,
            &controller,
            pane,
            vec![(
                2,
                zz_daemon::AgentStreamPayload::Update {
                    update: serde_json::json!({
                        "sessionUpdate": "agent_message_chunk",
                        "content": {"type": "text", "text": "meanwhile"},
                    }),
                },
            )],
        );
        stream(
            cx,
            &controller,
            pane,
            vec![(3, tool("agent-1", serde_json::json!({})))],
        );

        let rows = cx.update(|_, cx| view.read(cx).timeline.rows.clone());
        let [TimelineRow::Group { entries, .. }] = rows.as_slice() else {
            panic!("the step lands in its agent's turn row: {rows:?}");
        };
        let [AgentEntry::Assistant { .. }, agent, step] = entries.as_slice() else {
            panic!("the step follows its agent in the turn: {entries:?}");
        };
        assert_eq!(zz_ui::agent::timeline_parent(step), Some(agent.id()));
    }

    #[cfg(not(target_os = "macos"))]
    #[ala::test]
    fn a_step_that_lands_in_the_same_batch_before_its_agent_still_nests(cx: &mut TestAppContext) {
        let pane = PaneId(49);
        let (view, controller, _sink, cx) = wired_view(cx, pane);
        let tool = |id: &str, meta: serde_json::Value| zz_daemon::AgentStreamPayload::Update {
            update: serde_json::json!({
                "sessionUpdate": "tool_call",
                "toolCallId": id,
                "title": id,
                "kind": "read",
                "_meta": meta,
            }),
        };
        stream(
            cx,
            &controller,
            pane,
            vec![(
                1,
                zz_daemon::AgentStreamPayload::Update {
                    update: serde_json::json!({
                        "sessionUpdate": "agent_message_chunk",
                        "content": {"type": "text", "text": "starting"},
                    }),
                },
            )],
        );
        stream(
            cx,
            &controller,
            pane,
            vec![
                (
                    2,
                    tool("read-1", serde_json::json!({"zz": {"parent": "agent-1"}})),
                ),
                (
                    3,
                    zz_daemon::AgentStreamPayload::Update {
                        update: serde_json::json!({
                            "sessionUpdate": "agent_message_chunk",
                            "messageId": "m-2",
                            "content": {"type": "text", "text": "meanwhile"},
                        }),
                    },
                ),
                (4, tool("agent-1", serde_json::json!({}))),
            ],
        );

        let rows = cx.update(|_, cx| view.read(cx).timeline.rows.clone());
        let [TimelineRow::Group { entries, .. }] = rows.as_slice() else {
            panic!("the step joins its agent's turn row: {rows:?}");
        };
        let [
            AgentEntry::Assistant { .. },
            AgentEntry::Assistant { .. },
            agent,
            step,
        ] = entries.as_slice()
        else {
            panic!("the step follows its agent in the turn: {entries:?}");
        };
        assert_eq!(zz_ui::agent::timeline_parent(step), Some(agent.id()));
    }

    #[cfg(not(target_os = "macos"))]
    fn ready_with_a_prompt(
        cx: &mut VisualTestContext,
        controller: &Entity<AgentController>,
        pane: PaneId,
        verbs: bool,
        text: &str,
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
                            verbs,
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
                (
                    3,
                    zz_daemon::AgentStreamPayload::Update {
                        update: serde_json::json!({
                            "sessionUpdate": "user_message_chunk",
                            "messageId": "u-1",
                            "content": {"type": "text", "text": text},
                        }),
                    },
                ),
            ],
        );
    }

    #[cfg(not(target_os = "macos"))]
    fn hover_prompt(cx: &mut VisualTestContext) -> Option<ala::Bounds<ala::Pixels>> {
        let bubble = cx
            .debug_bounds("agent-user-bubble")
            .expect("the prompt row is painted");
        cx.simulate_mouse_move(bubble.center(), None, ala::Modifiers::none());
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.debug_bounds("agent-user-rewind")
    }

    #[cfg(not(target_os = "macos"))]
    #[ala::test]
    fn rewind_to_here_sends_the_rows_id_the_way_the_composer_sends(cx: &mut TestAppContext) {
        let pane = PaneId(50);
        let (view, controller, sink, cx) = wired_view(cx, pane);
        ready_with_a_prompt(cx, &controller, pane, true, "fix the flaky test");

        let rewind = hover_prompt(cx).expect("an idle pane offers the rewind");
        cx.simulate_click(rewind.center(), ala::Modifiers::none());
        cx.run_until_parked();

        assert_eq!(
            sent_answers(&sink),
            [crate::mux::client::AgentRequest::Prompt {
                text: "/rewind u-1".to_owned(),
                images: Vec::new(),
            }]
        );
        let (rows, connection) = cx.update(|_, cx| {
            let view = view.read(cx);
            (view.timeline.rows.len(), view.pane_state.connection)
        });
        assert_eq!(rows, 1, "the command adds no prompt row of its own");
        assert_eq!(connection, AgentConnectionState::Ready, "nor a turn");
    }

    #[cfg(not(target_os = "macos"))]
    #[ala::test]
    fn rewind_to_here_waits_for_the_turn_to_end(cx: &mut TestAppContext) {
        let pane = PaneId(51);
        let (_view, controller, sink, cx) = wired_view(cx, pane);
        ready_with_a_prompt(cx, &controller, pane, true, "fix the flaky test");
        cx.update(|_, cx| {
            controller.update(cx, |controller, cx| {
                controller.apply_pane_state(
                    pane,
                    &zz_protocol::AgentPaneWire {
                        phase: zz_protocol::AgentConnectionPhase::Running,
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

        assert!(hover_prompt(cx).is_none());
        assert!(sent_answers(&sink).is_empty());
    }

    #[cfg(not(target_os = "macos"))]
    #[ala::test]
    fn a_slash_command_row_is_not_a_rewind_point(cx: &mut TestAppContext) {
        let pane = PaneId(52);
        let (_view, controller, _sink, cx) = wired_view(cx, pane);
        ready_with_a_prompt(cx, &controller, pane, true, "/compact");
        assert!(hover_prompt(cx).is_none());
    }

    #[cfg(not(target_os = "macos"))]
    #[ala::test]
    fn an_agent_without_zz_commands_offers_no_rewind(cx: &mut TestAppContext) {
        let pane = PaneId(53);
        let (_view, controller, _sink, cx) = wired_view(cx, pane);
        ready_with_a_prompt(cx, &controller, pane, false, "fix the flaky test");
        assert!(hover_prompt(cx).is_none());
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

    fn no_parent(_: u64) -> Option<u64> {
        None
    }

    #[test]
    fn subagent_steps_join_their_agents_row_across_appends() {
        let parents = |id: u64| matches!(id, 2 | 4).then_some(1);
        let mut entries = vec![
            thread_tool(1, "Survey", AgentToolStatusModel::Running),
            thread_tool(2, "Read a.rs", AgentToolStatusModel::Completed),
            AgentThreadEntry::Assistant {
                id: 3,
                markdown: "meanwhile".to_owned(),
                aside: None,
            },
        ];
        let mut revisions = vec![1, 1, 1];
        let mut timeline = TimelineModel::new(&entries, &revisions, &parents);
        assert_eq!(
            timeline.entry_to_row,
            [0, 0, 0],
            "the message joins the turn"
        );

        entries.push(thread_tool(4, "Read b.rs", AgentToolStatusModel::Running));
        revisions.push(1);
        let TimelineModelUpdate::Incremental {
            remeasure_rows,
            added_rows,
            ..
        } = timeline.synchronize(&entries, &revisions, None, &parents)
        else {
            panic!("a late step should grow its agent's row in place");
        };
        assert_eq!(remeasure_rows, [0]);
        assert_eq!(added_rows, 0);
        assert_eq!(timeline.entry_to_row, [0, 0, 0, 0]);
        assert!(matches!(
            &timeline.rows[0],
            TimelineRow::Group { entries, .. }
                if entries.iter().map(AgentEntry::id).eq([1, 3, 2, 4])
        ));
        assert!(matches!(
            timeline.rows[0].entry(4),
            Some(AgentEntry::Tool(AgentToolEntry {
                parent: Some(1),
                ..
            }))
        ));

        entries[3] = thread_tool(4, "Read b.rs", AgentToolStatusModel::Completed);
        revisions[3] = 2;
        assert!(matches!(
            timeline.synchronize(&entries, &revisions, None, &no_parent),
            TimelineModelUpdate::Rebuild
        ));
        assert_eq!(timeline.entry_to_row, [0, 0, 0, 0]);
    }

    fn thread_tool(id: u64, label: &str, status: AgentToolStatusModel) -> AgentThreadEntry {
        AgentThreadEntry::Tool {
            id,
            protocol_id: format!("tool-{id}"),
            kind: AgentToolKindModel::Edit,
            status,
            label: label.to_owned(),
            location: None,
            input: None,
            output: Vec::new(),
            default_expanded: false,
            exit_code: None,
        }
    }

    fn thread_reasoning(id: u64, markdown: &str) -> AgentThreadEntry {
        AgentThreadEntry::Reasoning {
            id,
            label: "Reasoning".to_owned(),
            markdown: markdown.to_owned(),
            default_expanded: false,
        }
    }

    #[test]
    fn reasoning_and_tools_fold_into_one_row() {
        let mut entries = vec![
            thread_reasoning(1, "first"),
            thread_reasoning(2, "second"),
            thread_tool(3, "Editing files", AgentToolStatusModel::Completed),
        ];
        let mut revisions = vec![1, 1, 1];
        let mut timeline = TimelineModel::new(&entries, &revisions, &no_parent);
        assert_eq!(timeline.entry_to_row, [0, 0, 0]);
        assert!(matches!(
            &timeline.rows[0],
            TimelineRow::Group { entries, .. } if entries.len() == 3
        ));

        entries.push(thread_reasoning(4, "third"));
        entries.push(thread_reasoning(5, "fourth"));
        revisions.extend([1, 1]);
        let TimelineModelUpdate::Incremental {
            remeasure_rows,
            splice_start,
            added_rows,
            ..
        } = timeline.synchronize(&entries, &revisions, None, &no_parent)
        else {
            panic!("appending reasoning after a tool should update the trailing group");
        };
        assert_eq!(remeasure_rows, [0]);
        assert_eq!(splice_start, 1);
        assert_eq!(added_rows, 0);
        assert_eq!(timeline.entry_to_row, [0, 0, 0, 0, 0]);

        entries[3] = thread_reasoning(4, "third, continued");
        revisions[3] = 2;
        let TimelineModelUpdate::Incremental {
            remeasure_rows,
            added_rows,
            ..
        } = timeline.synchronize(&entries, &revisions, None, &no_parent)
        else {
            panic!("a member revision should update its owning group row");
        };
        assert_eq!(remeasure_rows, [0]);
        assert_eq!(added_rows, 0);
    }

    #[test]
    fn timeline_model_updates_trailing_groups_and_maps_member_revisions() {
        let mut entries = vec![
            thread_tool(1, "Editing files", AgentToolStatusModel::Completed),
            thread_tool(2, "Editing files", AgentToolStatusModel::Running),
            thread_tool(3, "Editing files", AgentToolStatusModel::Completed),
        ];
        let mut revisions = vec![1, 1, 1];
        let mut timeline = TimelineModel::new(&entries, &revisions, &no_parent);
        assert_eq!(timeline.rows.len(), 1);
        assert_eq!(timeline.entry_to_row, [0, 0, 0]);

        entries.push(thread_tool(
            4,
            "Editing files",
            AgentToolStatusModel::Pending,
        ));
        revisions.push(1);
        let TimelineModelUpdate::Incremental {
            store_entries,
            remeasure_rows,
            splice_start,
            added_rows,
        } = timeline.synchronize(&entries, &revisions, None, &no_parent)
        else {
            panic!("tool append should update the trailing group");
        };
        assert!(store_entries.is_empty());
        assert_eq!(remeasure_rows, [0]);
        assert_eq!(splice_start, 1);
        assert_eq!(added_rows, 0);
        assert_eq!(timeline.rows.len(), 1);
        assert_eq!(timeline.entry_to_row, [0, 0, 0, 0]);
        assert!(matches!(
            &timeline.rows[0],
            TimelineRow::Group { entries, .. } if entries.len() == 4
        ));

        entries.push(thread_tool(
            5,
            "Running command",
            AgentToolStatusModel::Completed,
        ));
        revisions.push(1);
        let TimelineModelUpdate::Incremental {
            remeasure_rows,
            splice_start,
            added_rows,
            ..
        } = timeline.synchronize(&entries, &revisions, None, &no_parent)
        else {
            panic!("different-label tool append should update the trailing group");
        };
        assert_eq!(remeasure_rows, [0]);
        assert_eq!(splice_start, 1);
        assert_eq!(added_rows, 0);
        assert_eq!(timeline.rows.len(), 1);
        assert_eq!(timeline.entry_to_row, [0, 0, 0, 0, 0]);

        entries[1] = thread_tool(2, "Editing files", AgentToolStatusModel::Failed);
        revisions[1] = 2;
        let TimelineModelUpdate::Incremental {
            store_entries,
            remeasure_rows,
            added_rows,
            ..
        } = timeline.synchronize(&entries, &revisions, None, &no_parent)
        else {
            panic!("member revision should update its owning row");
        };
        assert_eq!(remeasure_rows, [0]);
        assert_eq!(added_rows, 0);
        assert_eq!(store_entries.len(), 1);
        assert_eq!(store_entries[0].id(), 2);
        assert!(matches!(
            timeline.rows[0].entry(2),
            Some(AgentEntry::Tool(AgentToolEntry {
                status: AgentToolStatus::Failed,
                ..
            }))
        ));

        entries[1] = thread_tool(2, "Renamed tool", AgentToolStatusModel::Failed);
        revisions[1] = 3;
        let TimelineModelUpdate::Incremental {
            store_entries,
            remeasure_rows,
            added_rows,
            ..
        } = timeline.synchronize(&entries, &revisions, None, &no_parent)
        else {
            panic!("tool label changes should preserve contiguous grouping");
        };
        assert_eq!(remeasure_rows, [0]);
        assert_eq!(added_rows, 0);
        assert_eq!(store_entries.len(), 1);
        assert_eq!(timeline.entry_to_row, [0, 0, 0, 0, 0]);
    }

    #[test]
    fn tool_entry_adapter_preserves_typed_payloads() {
        let entry = ui_entry(&AgentThreadEntry::Tool {
            id: 1,
            protocol_id: "tool".to_owned(),
            kind: AgentToolKindModel::Execute,
            status: AgentToolStatusModel::Completed,
            label: "run".to_owned(),
            location: Some("/workspace/src/lib.rs:9".to_owned()),
            input: Some(ToolPayload::Json("{\n  \"check\": true\n}".to_owned())),
            output: vec![
                ToolPayload::Diff {
                    path: "/workspace/src/lib.rs".to_owned(),
                    old: Some("old".to_owned()),
                    new: "new".to_owned(),
                },
                ToolPayload::Text("done".to_owned()),
                ToolPayload::Terminal("$ cargo check\nok\n[exit status: 0]".to_owned()),
            ],
            default_expanded: false,
            exit_code: None,
        });
        let AgentEntry::Tool(AgentToolEntry {
            location,
            input,
            output,
            ..
        }) = entry
        else {
            panic!("tool output should be present");
        };

        assert_eq!(location.as_deref(), Some("/workspace/src/lib.rs:9"));
        assert!(matches!(input, Some(AgentToolPayload::Json(json)) if json.contains("check")));
        assert!(matches!(
            output.as_ref(),
            [
                AgentToolPayload::Diff { path, old: Some(old), new },
                AgentToolPayload::Text(text),
                AgentToolPayload::Terminal(terminal),
            ]
                if path == "/workspace/src/lib.rs"
                    && old == "old"
                    && new == "new"
                    && text == "done"
                    && terminal.contains("exit status")
        ));
    }

    #[test]
    fn tool_entry_adapter_releases_replaced_payload_slots() {
        let mut entry = AgentThreadEntry::Tool {
            id: 7,
            protocol_id: "tool".to_owned(),
            kind: AgentToolKindModel::Execute,
            status: AgentToolStatusModel::Running,
            label: "run".to_owned(),
            location: None,
            input: Some(ToolPayload::Text("input".to_owned())),
            output: vec![
                ToolPayload::Text("one".to_owned()),
                ToolPayload::Text("two".to_owned()),
                ToolPayload::Text("three".to_owned()),
            ],
            default_expanded: false,
            exit_code: None,
        };
        let mut markdown = HashMap::new();
        let mut tool_payloads = HashMap::new();

        ui_entry_with_markdown(&entry, &mut markdown, &mut tool_payloads);
        assert_eq!(tool_payloads[&7].len(), 4);

        let AgentThreadEntry::Tool { input, output, .. } = &mut entry else {
            unreachable!();
        };
        *input = None;
        output.truncate(1);
        ui_entry_with_markdown(&entry, &mut markdown, &mut tool_payloads);

        assert_eq!(tool_payloads[&7].len(), 2);
        assert!(tool_payloads[&7][0].is_none());
        assert!(matches!(
            &tool_payloads[&7][1],
            Some(AgentToolPayload::Text(text)) if text == "one"
        ));
    }

    #[test]
    fn tool_payload_updates_preserve_live_handles_and_other_tools() {
        let mut markdown = HashMap::new();
        let mut tool_payloads = HashMap::new();
        let mut first = thread_tool(7, "first", AgentToolStatusModel::Running);
        let mut second = thread_tool(8, "second", AgentToolStatusModel::Running);
        for entry in [&mut first, &mut second] {
            let AgentThreadEntry::Tool { input, output, .. } = entry else {
                unreachable!();
            };
            *input = Some(ToolPayload::Json("input".to_owned()));
            output.push(ToolPayload::Text("initial".to_owned()));
            ui_entry_with_markdown(entry, &mut markdown, &mut tool_payloads);
        }
        let Some(AgentToolPayload::Text(retained_first)) = tool_payloads[&7][1].clone() else {
            panic!("first text handle");
        };
        let Some(AgentToolPayload::Text(retained_second)) = tool_payloads[&8][1].clone() else {
            panic!("second text handle");
        };
        let AgentThreadEntry::Tool { input, output, .. } = &mut first else {
            unreachable!();
        };
        *input = None;
        output[0] = ToolPayload::Text("initial, continued".to_owned());
        output.push(ToolPayload::Terminal("another output".to_owned()));
        ui_entry_with_markdown(&first, &mut markdown, &mut tool_payloads);
        assert_eq!(retained_first, *"initial, continued");
        assert_eq!(retained_second, *"initial");
        assert_eq!(tool_payloads[&8].len(), 2);
        assert!(tool_payloads[&8][0].is_some());
        assert!(tool_payloads[&7][0].is_none());
        let AgentThreadEntry::Tool { output, .. } = &mut first else {
            unreachable!();
        };
        output.clear();
        ui_entry_with_markdown(&first, &mut markdown, &mut tool_payloads);
        assert_eq!(tool_payloads[&7].len(), 1);
        assert_eq!(retained_second, *"initial");
    }

    #[test]
    fn flat_entry_adapter_covers_exactly_five_acp_shapes() {
        let entries = ui_entries(&[
            AgentThreadEntry::User {
                id: 1,
                markdown: "user".to_owned(),
                images: Vec::new(),
                message_id: None,
            },
            AgentThreadEntry::Assistant {
                id: 2,
                markdown: "assistant".to_owned(),
                aside: None,
            },
            AgentThreadEntry::Reasoning {
                id: 3,
                label: "reasoning".to_owned(),
                markdown: "thought".to_owned(),
                default_expanded: false,
            },
            AgentThreadEntry::Tool {
                id: 4,
                protocol_id: "tool".to_owned(),
                kind: AgentToolKindModel::Execute,
                status: AgentToolStatusModel::Running,
                label: "run".to_owned(),
                location: None,
                input: None,
                output: Vec::new(),
                default_expanded: false,
                exit_code: None,
            },
            AgentThreadEntry::Plan {
                id: 5,
                markdown: "- [ ] plan".to_owned(),
            },
        ]);

        assert_eq!(entries.len(), 5);
        assert!(matches!(entries[0], AgentEntry::User { .. }));
        assert!(matches!(entries[1], AgentEntry::Assistant { .. }));
        assert!(matches!(entries[2], AgentEntry::Reasoning { .. }));
        assert!(matches!(entries[3], AgentEntry::Tool(_)));
        assert!(matches!(entries[4], AgentEntry::Plan { .. }));
    }

    #[test]
    fn user_attachments_reach_the_timeline_without_a_second_copy() {
        let image = Arc::new(Image::from_bytes(ala::ImageFormat::Png, vec![0x89, 0x50]));
        let entries = ui_entries(&[AgentThreadEntry::User {
            id: 1,
            markdown: String::new(),
            images: vec![Arc::clone(&image)],
            message_id: None,
        }]);

        let AgentEntry::User { images, .. } = &entries[0] else {
            panic!("the adapter should keep this a user entry");
        };
        assert_eq!(images.len(), 1);
        assert!(
            Arc::ptr_eq(&images[0], &image),
            "the bytes should travel by handle, not by clone, so ala decodes them once"
        );
    }
}
